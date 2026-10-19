//! One ACA sandbox: its lifecycle over the data plane, and its facets.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use sandbox_driver::{
    Capabilities, Error, Exec, ExecFailure, ExecSpec, Filesystem, NetworkPolicy, PlatformInfo,
    ResourceKind, Result, Sandbox, SandboxId, SandboxState, SandboxStatus,
};

use crate::client::{AcaClient, Body, Replay};
use crate::exec::{AcaExec, sandbox_error};
use crate::fs::AcaFs;
use crate::models::{WireSandbox, sandbox_state};

/// How long a create, start or stop may take to settle.
pub(crate) const SETTLE_TIMEOUT: Duration = Duration::from_secs(300);
const SETTLE_POLL: Duration = Duration::from_secs(2);

pub(crate) struct AcaSandbox {
    id:                SandboxId,
    client:            AcaClient,
    capabilities:      Capabilities,
    working_directory: String,
    exec:              Arc<AcaExec>,
    fs:                AcaFs,
}

impl AcaSandbox {
    pub(crate) fn new(
        id: SandboxId,
        client: AcaClient,
        capabilities: Capabilities,
        working_directory: String,
    ) -> Self {
        let exec = Arc::new(AcaExec::new(
            client.clone(),
            id.as_str().to_owned(),
            working_directory.clone(),
        ));
        let fs = AcaFs::new(
            client.clone(),
            id.as_str().to_owned(),
            Arc::clone(&exec) as Arc<dyn Exec>,
        );
        Self {
            id,
            client,
            capabilities,
            working_directory,
            exec,
            fs,
        }
    }

    fn path(&self, suffix: &str) -> String {
        format!("/sandboxes/{}{suffix}", self.id.as_str())
    }

    pub(crate) async fn wire(&self) -> Result<WireSandbox> {
        fetch(&self.client, self.id.as_str()).await
    }

    /// Posts a lifecycle action. A 409 means the sandbox is already in or
    /// moving to the asked-for state, and is not an error.
    async fn post(&self, action: &str) -> Result<()> {
        match self
            .client
            .data(
                reqwest::Method::POST,
                &self.path(&format!("/{action}")),
                &[],
                Body::Json(serde_json::json!({})),
                Replay::Safe,
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(error) if error.status() == Some(409) => Ok(()),
            Err(error) => Err(sandbox_error(error, self.id.as_str())),
        }
    }

    /// Waits until the sandbox is in one of `targets`, failing on `Error`
    /// or when it is deleted under the wait.
    async fn settle(&self, targets: &[SandboxState], action: &str) -> Result<()> {
        wait_for(&self.client, self.id.as_str(), targets, action).await
    }
}

/// Brings the sandbox to `Running`: a no-op when it runs, a resume when it
/// is stopped or suspended, and a wait when it is already resuming.
pub(crate) async fn resume(client: &AcaClient, id: &str) -> Result<()> {
    let wire = fetch(client, id).await?;
    match sandbox_state(wire.state.as_deref()) {
        SandboxState::Running => return Ok(()),
        SandboxState::Starting | SandboxState::Creating => {}
        _ => match client
            .data(
                reqwest::Method::POST,
                &format!("/sandboxes/{id}/resume"),
                &[],
                Body::Json(serde_json::json!({})),
                Replay::Safe,
            )
            .await
        {
            Ok(_) => {}
            // Already resuming or running.
            Err(error) if error.status() == Some(409) => {}
            Err(error) => return Err(sandbox_error(error, id)),
        },
    }
    wait_for(client, id, &[SandboxState::Running], "start").await
}

pub(crate) async fn fetch(client: &AcaClient, id: &str) -> Result<WireSandbox> {
    client
        .data_json(
            reqwest::Method::GET,
            &format!("/sandboxes/{id}"),
            &[],
            Body::None,
            Replay::Safe,
        )
        .await
        .map_err(|error| sandbox_error(error, id))
}

pub(crate) async fn wait_for(
    client: &AcaClient,
    id: &str,
    targets: &[SandboxState],
    action: &str,
) -> Result<()> {
    let started = Instant::now();
    loop {
        let wire = match fetch(client, id).await {
            // A sandbox can be briefly invisible right after it is created.
            Err(Error::NotFound { .. }) if started.elapsed() < Duration::from_secs(30) => None,
            other => Some(other?),
        };
        if let Some(wire) = wire {
            let state = sandbox_state(wire.state.as_deref());
            if targets.contains(&state) {
                return Ok(());
            }
            if state == SandboxState::Error {
                return Err(Error::InvalidState {
                    current: state,
                    action:  sandbox_driver::Action::Start,
                });
            }
            if wire
                .state_details
                .as_ref()
                .and_then(|details| details.stopped_reason.as_deref())
                == Some("Disabled")
            {
                return Err(Error::Provider(sandbox_driver::ProviderError::new(
                    crate::client::kind(),
                    format!(
                        "sandbox {id} is disabled in Azure and cannot be resumed until it is \
                         enabled again"
                    ),
                )));
            }
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(Error::Timeout {
                operation: format!("waiting for ACA sandbox {id} to {action}"),
                elapsed:   started.elapsed(),
            });
        }
        tokio::time::sleep(SETTLE_POLL).await;
    }
}

/// The driver's status for the wire sandbox.
pub(crate) fn status_of(wire: &WireSandbox) -> Result<SandboxStatus> {
    let id = SandboxId::try_new(&wire.id).map_err(|error| {
        Error::invalid_spec("id", format!("the service returned an invalid id: {error}"))
    })?;
    let mut status = SandboxStatus::new(id, sandbox_state(wire.state.as_deref()));
    status.provider_state = wire.state.clone().unwrap_or_default();
    status.error_reason = wire
        .state_details
        .as_ref()
        .and_then(|details| details.stopped_reason.clone());
    status.labels = wire.labels.clone();
    status.region = wire.region.clone();
    status.resources = wire
        .resources
        .as_ref()
        .map(|resources| resources.to_driver());
    status.sandbox_kind = Some(sandbox_driver::SandboxKind::VirtualMachine);
    status.image = wire
        .sources_ref
        .as_ref()
        .and_then(|sources| sources.disk_image.as_ref())
        .and_then(|image| image.name.clone().or_else(|| image.id.clone()));
    status.network = wire
        .egress_policy
        .as_ref()
        .and_then(|policy| policy.default_action.as_deref())
        .map(|action| {
            if action.eq_ignore_ascii_case("deny") {
                NetworkPolicy::Block
            } else {
                NetworkPolicy::AllowAll
            }
        })
        .or(Some(NetworkPolicy::AllowAll));
    status.created_at = wire
        .created_at
        .as_deref()
        .and_then(|text| humantime_like(text));
    Ok(status)
}

/// RFC 3339 to `SystemTime`, without a date-time dependency: the service
/// writes UTC with a `Z` suffix.
fn humantime_like(text: &str) -> Option<std::time::SystemTime> {
    let text = text.trim().trim_end_matches('Z');
    let (date, time) = text.split_once('T')?;
    let mut date = date.split('-').map(str::parse::<i64>);
    let (year, month, day) = (date.next()?.ok()?, date.next()?.ok()?, date.next()?.ok()?);
    let time = time.split(['+', '.']).next()?;
    let mut time = time.split(':').map(str::parse::<i64>);
    let (hour, minute, second) = (time.next()?.ok()?, time.next()?.ok()?, time.next()?.ok()?);
    // Days from the civil date (Howard Hinnant's algorithm).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second;
    let secs = u64::try_from(secs).ok()?;
    Some(std::time::UNIX_EPOCH + Duration::from_secs(secs))
}

#[async_trait]
impl Sandbox for AcaSandbox {
    fn id(&self) -> &SandboxId {
        &self.id
    }

    fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    async fn describe(&self) -> Result<SandboxStatus> {
        status_of(&self.wire().await?)
    }

    fn working_directory(&self) -> &str {
        &self.working_directory
    }

    async fn environment(&self) -> Result<BTreeMap<String, String>> {
        let result = self.exec.run(&ExecSpec::new("env").arg("-0")).await?;
        if !result.success() {
            return Err(Error::Exec(
                ExecFailure::new(
                    "reading sandbox environment",
                    result.termination,
                    result.exit_code,
                    result.stdout,
                    result.stderr,
                )
                .with_duration(result.duration),
            ));
        }
        let output = String::from_utf8(result.stdout)
            .map_err(|error| Error::invalid_spec("environment", error.to_string()))?;
        output
            .split_terminator('\0')
            .filter_map(|entry| entry.split_once('='))
            .map(|(key, value)| Ok((key.to_owned(), value.to_owned())))
            .collect()
    }

    async fn platform_info(&self) -> Result<PlatformInfo> {
        let result = self
            .exec
            .run(
                &ExecSpec::new("uname")
                    .args(["-s", "-r", "-m"])
                    .timeout(Duration::from_secs(60)),
            )
            .await?;
        let text = result.stdout_lossy();
        let mut parts = text.split_whitespace();
        let os = parts.next().unwrap_or("linux").to_lowercase();
        let version = parts.next().unwrap_or("").to_owned();
        let arch = parts.next().unwrap_or("").to_owned();
        Ok(PlatformInfo::new(os, arch, version))
    }

    #[tracing::instrument(skip_all, fields(provider_kind = "aca", sandbox_id = %self.id), err)]
    async fn start(&self) -> Result<()> {
        resume(&self.client, self.id.as_str()).await
    }

    #[tracing::instrument(skip_all, fields(provider_kind = "aca", sandbox_id = %self.id), err)]
    async fn stop(&self) -> Result<()> {
        let wire = match self.wire().await {
            Err(Error::NotFound { .. }) => return Ok(()),
            other => other?,
        };
        if sandbox_state(wire.state.as_deref()) == SandboxState::Stopped {
            return Ok(());
        }
        self.post("stop").await?;
        self.settle(&[SandboxState::Stopped], "stop").await
    }

    #[tracing::instrument(skip_all, fields(provider_kind = "aca", sandbox_id = %self.id), err)]
    async fn delete(&self) -> Result<()> {
        delete_by_id(&self.client, self.id.as_str()).await
    }

    fn exec(&self) -> &dyn Exec {
        &*self.exec
    }

    fn fs(&self) -> &dyn Filesystem {
        &self.fs
    }
}

/// Deletes the sandbox and waits until it is gone. Idempotent.
pub(crate) async fn delete_by_id(client: &AcaClient, id: &str) -> Result<()> {
    match client
        .data(
            reqwest::Method::DELETE,
            &format!("/sandboxes/{id}"),
            &[],
            Body::None,
            Replay::Safe,
        )
        .await
    {
        Ok(_) => {}
        Err(error) if error.status() == Some(404) => return Ok(()),
        Err(error) => return Err(sandbox_error(error, id)),
    }
    let started = Instant::now();
    loop {
        match fetch(client, id).await {
            Err(Error::NotFound {
                resource: ResourceKind::Sandbox,
                ..
            }) => return Ok(()),
            Err(error) => return Err(error),
            Ok(wire) if sandbox_state(wire.state.as_deref()) == SandboxState::Deleted => {
                return Ok(());
            }
            Ok(_) if started.elapsed() > SETTLE_TIMEOUT => {
                return Err(Error::Timeout {
                    operation: format!("waiting for ACA sandbox {id} to be deleted"),
                    elapsed:   started.elapsed(),
                });
            }
            Ok(_) => tokio::time::sleep(SETTLE_POLL).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_at_parses_as_utc() {
        let parsed = humantime_like("2026-10-19T14:44:57.123Z").unwrap();
        let secs = parsed
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert_eq!(secs, 1_792_421_097);
    }

    #[test]
    fn status_reports_the_image_and_a_denied_egress_as_blocked() {
        let wire: WireSandbox = serde_json::from_str(
            r#"{"id":"11111111-2222-3333-4444-555555555555","state":"Suspended",
                "labels":{"petri.run":"r1"},
                "sourcesRef":{"diskImage":{"name":"azure-dev","isPublic":true}},
                "resources":{"cpu":"2000m","memory":"4096Mi"},
                "egressPolicy":{"defaultAction":"Deny"}}"#,
        )
        .unwrap();
        let status = status_of(&wire).unwrap();
        assert_eq!(status.state, SandboxState::Stopped);
        assert_eq!(status.provider_state, "Suspended");
        assert_eq!(status.image.as_deref(), Some("azure-dev"));
        assert_eq!(status.network, Some(NetworkPolicy::Block));
        assert_eq!(status.resources.unwrap().cpu_cores, Some(2));
        assert_eq!(
            status.labels.get("petri.run").map(String::as_str),
            Some("r1")
        );
    }
}
