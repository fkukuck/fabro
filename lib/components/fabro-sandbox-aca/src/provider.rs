//! The provider: creates, attaches, lists and deletes sandboxes in one
//! sandbox group, and checks the group is ready for runs.

use std::sync::Arc;

use async_trait::async_trait;
use sandbox_driver::{
    Capabilities, Error, EventContext, HealthStatus, Isolation, NetworkPolicy, ProviderError,
    ProviderHealth, ProviderKind, Result, Sandbox, SandboxFilter, SandboxId, SandboxProvider,
    SandboxSource, SandboxSpec, SandboxState, SandboxStatus,
};
use serde_json::{Value, json};

use crate::client::{AcaClient, Body, Replay, kind};
use crate::config::{AcaPolicy, DATA_OWNER_ROLE, MAX_RETENTION_SECS, READER_ROLE};
use crate::exec::sandbox_error;
use crate::models::{WireDiskImage, WireResources, WireSandbox};
use crate::sandbox::{AcaSandbox, SETTLE_TIMEOUT, delete_by_id, fetch, status_of, wait_for};

/// The workspace inside every sandbox.
pub const WORKSPACE: &str = "/workspace";

/// The image a sandbox gets when neither the run nor the policy names one.
/// `azure-dev` ships Docker, so container workloads run inside it.
pub const DEFAULT_IMAGE: &str = "azure-dev";

/// The capability set every ACA sandbox has.
#[must_use]
pub fn aca_capabilities() -> Capabilities {
    let mut caps = Capabilities::minimal(Isolation::Vm);
    caps.exec.stdin = true;
    caps.exec.stdin_stream = true;
    caps.exec.stop = true;
    caps.exec.live_streaming = true;
    caps.exec.streams_separated = true;
    caps.exec.environment = true;
    caps.fs.native = true;
    caps.fs.upload = true;
    caps.fs.download = true;
    caps.fs.permissions = true;
    caps.search.supported = true;
    caps.git.supported = true;
    caps.services.supported = true;
    caps.network.allow_all = true;
    caps.network.block_all = true;
    // A private relay through the sandbox, for sandbox-hosted MCP servers.
    caps.access.preview_urls = true;
    caps
}

/// How a run's sandbox is created, beyond the driver's spec: carried in
/// `SandboxSpec::provider_config`.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AcaProviderConfig {
    /// Seconds a stopped sandbox is kept before ACA deletes it.
    pub delete_after_stop_secs:  Option<u64>,
    /// Idle seconds before ACA suspends the sandbox, keeping its memory;
    /// [`DEFAULT_SUSPEND_AFTER_IDLE_SECS`] when unset, and never at `0`.
    /// A running command is polled, which counts as activity, so only a
    /// sandbox nothing uses is suspended: a run's whose Fabro went away is
    /// stopped and then deleted after its retention.
    pub suspend_after_idle_secs: Option<u64>,
}

/// How long a sandbox may sit idle before ACA suspends it.
pub const DEFAULT_SUSPEND_AFTER_IDLE_SECS: u64 = 30 * 60;

/// An ACA sandbox group as a sandbox-driver provider.
pub struct AcaProvider {
    kind:         ProviderKind,
    client:       AcaClient,
    policy:       AcaPolicy,
    capabilities: Capabilities,
}

impl AcaProvider {
    #[must_use]
    pub fn new(client: AcaClient, policy: AcaPolicy) -> Self {
        Self {
            kind: kind(),
            client,
            policy,
            capabilities: aca_capabilities(),
        }
    }

    #[must_use]
    pub fn client(&self) -> &AcaClient {
        &self.client
    }

    fn handle(&self, id: SandboxId) -> Arc<dyn Sandbox> {
        Arc::new(AcaSandbox::new(
            id,
            self.client.clone(),
            self.capabilities.clone(),
            WORKSPACE.to_owned(),
        ))
    }

    /// The image `spec` asks for, checked against the policy.
    fn image_for(&self, spec: &SandboxSpec) -> Result<String> {
        let requested = match &spec.source {
            SandboxSource::Image { reference } if !reference.trim().is_empty() => {
                Some(reference.trim().to_owned())
            }
            SandboxSource::Snapshot { .. }
            | SandboxSource::Dockerfile { .. }
            | SandboxSource::HostDirectory => {
                return Err(Error::invalid_spec(
                    "source",
                    "ACA sandboxes are created from a prepared disk image; snapshots, Dockerfiles \
                     and host directories are not supported",
                ));
            }
            _ => None,
        };
        let image = requested
            .or_else(|| self.policy.default_image.clone())
            .unwrap_or_else(|| DEFAULT_IMAGE.to_owned());
        if !self.policy.allows_image(&image) {
            return Err(Error::invalid_spec(
                "image",
                format!(
                    "the image `{image}` is not on the administrator's list of approved ACA \
                     images ({})",
                    self.policy.allowed_images.join(", ")
                ),
            ));
        }
        Ok(image)
    }

    /// The create request for `spec`.
    fn create_body(
        &self,
        spec: &SandboxSpec,
        image: &str,
        private_id: Option<&str>,
    ) -> Result<Value> {
        let config: AcaProviderConfig = if spec.provider_config.is_null() {
            AcaProviderConfig::default()
        } else {
            serde_json::from_value(spec.provider_config.clone())
                .map_err(|error| Error::invalid_spec("provider_config", error.to_string()))?
        };
        let disk_image = match private_id {
            Some(id) => json!({ "id": id }),
            None => json!({ "name": image, "isPublic": true }),
        };
        let retention = config
            .delete_after_stop_secs
            .unwrap_or_else(|| self.policy.effective_retention_secs())
            .min(MAX_RETENTION_SECS);
        let suspend = match config
            .suspend_after_idle_secs
            .unwrap_or(DEFAULT_SUSPEND_AFTER_IDLE_SECS)
        {
            0 => json!({ "enabled": false }),
            secs => json!({ "enabled": true, "interval": secs, "mode": "Memory" }),
        };
        let mut body = json!({
            "sourcesRef": { "diskImage": disk_image },
            "lifecycle": {
                "autoSuspendPolicy": suspend,
                "autoDeletePolicy": { "enabled": true, "deleteIntervalInSeconds": retention },
            },
            "labels": spec.labels,
            "environment": spec.env,
        });
        if let Some(resources) = WireResources::from_driver(&spec.resources) {
            body["resources"] = serde_json::to_value(resources).unwrap_or(Value::Null);
        }
        match &spec.network {
            NetworkPolicy::ProviderDefault | NetworkPolicy::AllowAll => {}
            NetworkPolicy::Block => {
                body["egressPolicy"] = json!({ "defaultAction": "Deny", "rules": [] });
            }
            NetworkPolicy::DomainAllowList { domains } => {
                let rules: Vec<Value> = domains
                    .iter()
                    .enumerate()
                    .map(|(index, domain)| {
                        json!({
                            "name": format!("fabro-allow-{index}"),
                            "match": { "host": domain },
                            "action": { "type": "Allow" },
                        })
                    })
                    .collect();
                body["egressPolicy"] = json!({ "defaultAction": "Deny", "rules": rules });
            }
            _ => {
                return Err(Error::invalid_spec(
                    "network",
                    "ACA sandboxes support allow-all, blocked, or domain allow-list networking; a \
                     CIDR allow-list is not supported",
                ));
            }
        }
        Ok(body)
    }

    /// Every image the group can create sandboxes from: its private
    /// images and the public catalog.
    pub async fn images(&self) -> Result<Vec<AcaImage>> {
        let mut images = Vec::new();
        let private: Vec<WireDiskImage> = self
            .client
            .data_list("/diskimages", &[])
            .await
            .map_err(|error| sandbox_error(error, "diskimages"))?;
        for image in private {
            images.push(AcaImage::from_wire(&image, false));
        }
        let public: Vec<WireDiskImage> = self
            .client
            .data_list("/diskimages/public", &[])
            .await
            .map_err(|error| sandbox_error(error, "diskimages/public"))?;
        for image in public {
            images.push(AcaImage::from_wire(&image, true));
        }
        Ok(images)
    }

    /// The image named `name`, when the group can see it.
    async fn find_image(&self, name: &str) -> Result<Option<AcaImage>> {
        Ok(self
            .images()
            .await?
            .into_iter()
            .find(|image| image.name == name || image.id.as_deref() == Some(name)))
    }

    /// Checks that a run on `image` with `resources` can start: the
    /// credential, the group, the image's readiness, the policy and the
    /// allocation. Reads only; creates nothing.
    pub async fn check_ready(
        &self,
        image: Option<&str>,
        resources: &sandbox_driver::Resources,
    ) -> Result<ReadinessReport> {
        let mut report = ReadinessReport {
            problems: resource_problems(resources),
            ..ReadinessReport::default()
        };
        let group = self.client.sandbox_group().await?;
        report.location = group.location.clone();
        if !group.is_ready() {
            report.problems.push(format!(
                "the sandbox group is not ready (provisioning state {})",
                group
                    .properties
                    .provisioning_state
                    .as_deref()
                    .unwrap_or("unknown")
            ));
        }
        // Proves the data-plane role.
        self.list(&SandboxFilter::default()).await?;
        let image = image
            .map(str::to_owned)
            .or_else(|| self.policy.default_image.clone())
            .unwrap_or_else(|| DEFAULT_IMAGE.to_owned());
        if !self.policy.allows_image(&image) {
            report.problems.push(format!(
                "the image `{image}` is not on the approved list ({})",
                self.policy.allowed_images.join(", ")
            ));
        }
        match self.find_image(&image).await? {
            Some(found) if found.ready => {}
            Some(found) => report.problems.push(format!(
                "the image `{image}` is not ready (state {})",
                found.state.as_deref().unwrap_or("unknown")
            )),
            None => report.problems.push(format!(
                "the image `{image}` is not available to the sandbox group"
            )),
        }
        report.image = Some(image);
        Ok(report)
    }
}

/// The largest sandbox ACA offers (its 3XL tier).
pub const MAX_CPU_CORES: u32 = 16;
pub const MAX_MEMORY_MB: u64 = 32 * 1024;
pub const MAX_DISK_MB: u64 = 320 * 1024;

/// Why `resources` cannot be requested from ACA, as sentences for an
/// operator; empty when they can.
#[must_use]
pub fn resource_problems(resources: &sandbox_driver::Resources) -> Vec<String> {
    let mut problems = Vec::new();
    if resources.cpu_cores == Some(0) {
        problems.push("a sandbox needs at least one CPU".to_owned());
    }
    if let Some(cpu) = resources.cpu_cores.filter(|cpu| *cpu > MAX_CPU_CORES) {
        problems.push(format!(
            "{cpu} CPUs is more than the {MAX_CPU_CORES} an ACA sandbox can have"
        ));
    }
    if let Some(memory) = resources.memory_mb.filter(|memory| *memory > MAX_MEMORY_MB) {
        problems.push(format!(
            "{memory} MiB of memory is more than the {MAX_MEMORY_MB} MiB an ACA sandbox can have"
        ));
    }
    if let Some(disk) = resources.disk_mb.filter(|disk| *disk > MAX_DISK_MB) {
        problems.push(format!(
            "{disk} MiB of disk is more than the {MAX_DISK_MB} MiB an ACA sandbox can have"
        ));
    }
    if resources.gpus.is_some_and(|gpus| gpus > 0) {
        problems.push("ACA sandboxes have no GPUs".to_owned());
    }
    problems
}

/// What [`AcaProvider::check_ready`] found.
#[derive(Clone, Debug, Default)]
pub struct ReadinessReport {
    pub location: Option<String>,
    pub image:    Option<String>,
    /// Everything that blocks a run, as sentences for an operator.
    pub problems: Vec<String>,
}

/// A disk image the group can create sandboxes from.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AcaImage {
    pub name:   String,
    pub id:     Option<String>,
    pub public: bool,
    pub ready:  bool,
    pub state:  Option<String>,
}

impl AcaImage {
    fn from_wire(image: &WireDiskImage, public: bool) -> Self {
        let state =
            image
                .status
                .as_ref()
                .and_then(|status| match (&status.state, &status.message) {
                    (Some(state), Some(message)) if !message.trim().is_empty() => {
                        Some(format!("{state}: {}", message.trim()))
                    }
                    (state, _) => state.clone(),
                });
        let ready = image
            .status
            .as_ref()
            .and_then(|status| status.state.as_deref())
            .is_none_or(|state| {
                state.eq_ignore_ascii_case("ready") || state.eq_ignore_ascii_case("succeeded")
            });
        Self {
            name: image.display_name().unwrap_or_default(),
            id: image.id.clone(),
            public,
            ready,
            state,
        }
    }
}

#[async_trait]
impl SandboxProvider for AcaProvider {
    fn kind(&self) -> &ProviderKind {
        &self.kind
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    #[tracing::instrument(skip_all, fields(provider_kind = "aca"), err)]
    async fn create(
        &self,
        spec: &SandboxSpec,
        _events: Option<EventContext>,
    ) -> Result<Arc<dyn Sandbox>> {
        spec.validate()?;
        if spec
            .working_directory
            .as_deref()
            .is_some_and(|dir| dir != WORKSPACE)
        {
            return Err(Error::invalid_spec(
                "working_directory",
                format!("ACA sandboxes work in {WORKSPACE}"),
            ));
        }
        let problems = resource_problems(&spec.resources);
        if !problems.is_empty() {
            return Err(Error::invalid_spec("resources", problems.join("; ")));
        }
        let image = self.image_for(spec)?;
        let found = self.find_image(&image).await?;
        let private_id = match &found {
            Some(found) if !found.ready => {
                return Err(Error::Provider(ProviderError::new(
                    kind(),
                    format!(
                        "the image `{image}` is not ready (state {})",
                        found.state.as_deref().unwrap_or("unknown")
                    ),
                )));
            }
            Some(found) if !found.public => found.id.clone(),
            Some(_) => None,
            None => {
                return Err(Error::Provider(ProviderError::new(
                    kind(),
                    format!("the image `{image}` is not available to the sandbox group"),
                )));
            }
        };
        let body = self.create_body(spec, &image, private_id.as_deref())?;
        // A create is never retried blindly: a lost answer would leave a
        // second sandbox. The labels find the first one instead.
        let created: WireSandbox = match self
            .client
            .data_json(
                reqwest::Method::PUT,
                "/sandboxes",
                &[],
                Body::Json(body),
                Replay::Never,
            )
            .await
        {
            Ok(created) => created,
            Err(error) => {
                let recovered = if spec.labels.is_empty() {
                    None
                } else {
                    let mut filter = SandboxFilter::default();
                    filter.labels.clone_from(&spec.labels);
                    self.list(&filter).await.ok().and_then(|found| {
                        (found.len() == 1).then(|| found[0].id.as_str().to_owned())
                    })
                };
                match recovered {
                    Some(id) => WireSandbox {
                        id,
                        ..WireSandbox::default()
                    },
                    None => return Err(sandbox_error(error, "new sandbox")),
                }
            }
        };
        let id = SandboxId::try_new(&created.id).map_err(|error| {
            Error::invalid_spec("id", format!("the service returned an invalid id: {error}"))
        })?;
        if let Err(error) =
            wait_for(&self.client, id.as_str(), &[SandboxState::Running], "start").await
        {
            // Leave nothing behind a failed start.
            if let Err(cleanup) = delete_by_id(&self.client, id.as_str()).await {
                tracing::warn!(error = %cleanup, sandbox = %id, "failed to delete an ACA sandbox that did not start");
            }
            return Err(error);
        }
        let handle = self.handle(id);
        // The workspace exists before anything runs in it.
        let prepared = handle
            .exec()
            .run(
                &sandbox_driver::ExecSpec::bash(format!(
                    "mkdir -p {WORKSPACE} && chmod 0777 {WORKSPACE}"
                ))
                .working_dir("/"),
            )
            .await;
        match prepared {
            Ok(result) if result.success() => Ok(handle),
            Ok(result) => {
                let _ = handle.delete().await;
                Err(Error::Provider(ProviderError::new(
                    kind(),
                    format!(
                        "the sandbox's workspace could not be prepared: {}",
                        result.stderr_lossy().trim()
                    ),
                )))
            }
            Err(error) => {
                let _ = handle.delete().await;
                Err(error)
            }
        }
    }

    async fn attach(
        &self,
        id: &SandboxId,
        _events: Option<EventContext>,
    ) -> Result<Arc<dyn Sandbox>> {
        // Proves the sandbox exists in this group before handing it out.
        fetch(&self.client, id.as_str()).await?;
        Ok(self.handle(id.clone()))
    }

    async fn delete(&self, id: &SandboxId, _events: Option<EventContext>) -> Result<()> {
        delete_by_id(&self.client, id.as_str()).await
    }

    async fn list(&self, filter: &SandboxFilter) -> Result<Vec<SandboxStatus>> {
        let labels = filter
            .labels
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(",");
        let query: Vec<(&str, &str)> = if labels.is_empty() {
            Vec::new()
        } else {
            vec![("labels", labels.as_str())]
        };
        let sandboxes: Vec<WireSandbox> = self
            .client
            .data_list("/sandboxes", &query)
            .await
            .map_err(|error| sandbox_error(error, "sandboxes"))?;
        sandboxes
            .iter()
            // The service's filter is trusted only as far as it goes.
            .filter(|wire| {
                filter
                    .labels
                    .iter()
                    .all(|(key, value)| wire.labels.get(key) == Some(value))
            })
            .map(status_of)
            .collect()
    }

    async fn health(&self) -> Result<ProviderHealth> {
        let required = vec![READER_ROLE.to_owned(), DATA_OWNER_ROLE.to_owned()];
        let group = match self.client.sandbox_group().await {
            Ok(group) => group,
            Err(Error::Auth(error)) => {
                let mut health = ProviderHealth::new(HealthStatus::Unauthorized);
                health.message = Some(error.reason);
                health.required_permissions = required;
                return Ok(health);
            }
            Err(Error::Transport(error)) => {
                let mut health = ProviderHealth::new(HealthStatus::Unreachable);
                health.message = Some(error.to_string());
                return Ok(health);
            }
            Err(Error::Provider(error)) => {
                let mut health = ProviderHealth::new(HealthStatus::Unauthorized);
                health.message = Some(error.message);
                health.required_permissions = required;
                return Ok(health);
            }
            Err(error) => return Err(error),
        };
        let mut health = match self.list(&SandboxFilter::default()).await {
            Ok(_) => ProviderHealth::new(HealthStatus::Ok),
            Err(Error::Auth(error)) => {
                let mut health = ProviderHealth::new(HealthStatus::Unauthorized);
                health.message = Some(error.reason);
                health.missing_permissions = vec![DATA_OWNER_ROLE.to_owned()];
                health
            }
            Err(Error::Transport(error)) => {
                let mut health = ProviderHealth::new(HealthStatus::Unreachable);
                health.message = Some(error.to_string());
                health
            }
            Err(error) => return Err(error),
        };
        if health.status == HealthStatus::Ok && !group.is_ready() {
            health.status = HealthStatus::Unreachable;
            health.message = Some(format!(
                "the sandbox group is not ready (provisioning state {})",
                group
                    .properties
                    .provisioning_state
                    .as_deref()
                    .unwrap_or("unknown")
            ));
        }
        health.required_permissions = required;
        health.identity = Some(
            group
                .id
                .unwrap_or_else(|| self.client.target().resource_id())
                .to_ascii_lowercase(),
        );
        Ok(health)
    }
}

/// How long a create may take overall, exported for callers' budgets.
pub const CREATE_TIMEOUT: std::time::Duration = SETTLE_TIMEOUT;

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::{AcaEndpoints, AcaServicePrincipal, AcaTarget};

    fn provider(policy: AcaPolicy) -> AcaProvider {
        let client = AcaClient::new(
            AcaTarget::default(),
            AcaServicePrincipal {
                tenant_id:     "t".into(),
                client_id:     "c".into(),
                client_secret: "s".into(),
            },
            AcaEndpoints::default(),
            fabro_http::test_http_client().unwrap(),
        );
        AcaProvider::new(client, policy)
    }

    #[test]
    fn the_policy_refuses_an_unapproved_image() {
        let provider = provider(AcaPolicy {
            allowed_images: vec!["azure-dev".into()],
            ..AcaPolicy::default()
        });
        let spec = SandboxSpec::new(SandboxSource::Image {
            reference: "ubuntu".into(),
        });
        let error = provider.image_for(&spec).unwrap_err();
        assert!(error.to_string().contains("approved"), "{error}");
    }

    #[test]
    fn an_empty_image_falls_back_to_the_policy_default() {
        let provider = provider(AcaPolicy {
            default_image: Some("node-22".into()),
            ..AcaPolicy::default()
        });
        let spec = SandboxSpec::new(SandboxSource::Image {
            reference: String::new(),
        });
        assert_eq!(provider.image_for(&spec).unwrap(), "node-22");
    }

    #[test]
    fn the_create_body_keeps_labels_caps_retention_and_blocks_egress() {
        let provider = provider(AcaPolicy {
            retention_secs: Some(3600),
            ..AcaPolicy::default()
        });
        let mut spec = SandboxSpec::new(SandboxSource::Image {
            reference: "azure-dev".into(),
        });
        spec.labels = BTreeMap::from([("petri.run".to_owned(), "r1".to_owned())]);
        spec.env = BTreeMap::from([("A".to_owned(), "b".to_owned())]);
        spec.network = NetworkPolicy::Block;
        spec.resources.cpu_cores = Some(2);
        spec.resources.memory_mb = Some(4096);
        let body = provider.create_body(&spec, "azure-dev", None).unwrap();
        assert_eq!(body["sourcesRef"]["diskImage"]["name"], "azure-dev");
        assert_eq!(body["sourcesRef"]["diskImage"]["isPublic"], true);
        assert_eq!(body["labels"]["petri.run"], "r1");
        assert_eq!(body["environment"]["A"], "b");
        assert_eq!(body["resources"]["cpu"], "2000m");
        assert_eq!(body["egressPolicy"]["defaultAction"], "Deny");
        assert_eq!(
            body["lifecycle"]["autoDeletePolicy"]["deleteIntervalInSeconds"],
            3600
        );
        assert_eq!(body["lifecycle"]["autoSuspendPolicy"]["enabled"], true);
        assert_eq!(
            body["lifecycle"]["autoSuspendPolicy"]["interval"],
            DEFAULT_SUSPEND_AFTER_IDLE_SECS
        );
        assert_eq!(body["lifecycle"]["autoSuspendPolicy"]["mode"], "Memory");

        spec.provider_config = json!({ "delete_after_stop_secs": 999_999 });
        let body = provider
            .create_body(&spec, "azure-dev", Some("img-1"))
            .unwrap();
        assert_eq!(body["sourcesRef"]["diskImage"]["id"], "img-1");
        assert_eq!(
            body["lifecycle"]["autoDeletePolicy"]["deleteIntervalInSeconds"],
            MAX_RETENTION_SECS
        );
    }

    #[test]
    fn a_cidr_allow_list_is_refused() {
        let mut spec = SandboxSpec::new(SandboxSource::Image {
            reference: "azure-dev".into(),
        });
        spec.network = NetworkPolicy::CidrAllowList {
            cidrs: vec!["10.0.0.0/8".into()],
        };
        assert!(
            provider(AcaPolicy::default())
                .create_body(&spec, "azure-dev", None)
                .is_err()
        );
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    #[test]
    fn resources_beyond_the_largest_tier_are_named() {
        let mut resources = sandbox_driver::Resources::default();
        assert!(resource_problems(&resources).is_empty());
        resources.cpu_cores = Some(32);
        resources.memory_mb = Some(64 * 1024);
        let problems = resource_problems(&resources);
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("32 CPUs"));
    }
}
