//! The Azure connection a provider is built from: where the sandbox group
//! lives, and the service principal that reaches it.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The Azure public cloud's Entra authority.
pub(crate) const DEFAULT_AUTHORITY_URL: &str = "https://login.microsoftonline.com";
/// Azure Resource Manager in the public cloud.
pub(crate) const DEFAULT_ARM_URL: &str = "https://management.azure.com";
/// The data-plane API version both planes accept.
pub const API_VERSION: &str = "2026-02-01-preview";
/// The OAuth scope of the sandbox data plane: the Dynamic Sessions
/// audience, not the data-plane host.
pub const DATA_PLANE_SCOPE: &str = "https://dynamicsessions.io/.default";

/// The built-in role a principal needs on the sandbox group.
pub const DATA_OWNER_ROLE: &str = "Container Apps SandboxGroup Data Owner";
/// Read access to the sandbox group resource itself.
pub const READER_ROLE: &str = "Reader";

/// Everything that names the sandbox group: non-secret, and stable for
/// the life of the sandboxes it holds. A lease's fingerprint is built
/// from it, so a resume against a different group refuses.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AcaTarget {
    pub subscription_id: String,
    pub resource_group:  String,
    pub sandbox_group:   String,
    /// The group's Azure region, e.g. `westus2`. Read from the group when
    /// unset, so it is only needed when the group cannot be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location:        Option<String>,
}

impl AcaTarget {
    /// The path below `/subscriptions/...` both planes share, without the
    /// ARM provider segment.
    pub(crate) fn data_plane_group_path(&self) -> String {
        format!(
            "/subscriptions/{}/resourceGroups/{}/sandboxGroups/{}",
            segment(&self.subscription_id),
            segment(&self.resource_group),
            segment(&self.sandbox_group)
        )
    }

    pub(crate) fn arm_group_path(&self) -> String {
        format!(
            "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.App/sandboxGroups/{}",
            segment(&self.subscription_id),
            segment(&self.resource_group),
            segment(&self.sandbox_group)
        )
    }

    /// The ARM resource id of the group, which is also the namespace a
    /// lease records.
    #[must_use]
    pub fn resource_id(&self) -> String {
        self.arm_group_path()
    }

    /// The fields an operator left empty, by the setting name.
    #[must_use]
    pub fn missing_fields(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.subscription_id.trim().is_empty() {
            missing.push("subscription_id");
        }
        if self.resource_group.trim().is_empty() {
            missing.push("resource_group");
        }
        if self.sandbox_group.trim().is_empty() {
            missing.push("sandbox_group");
        }
        missing
    }
}

fn segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value.trim(), percent_encoding::NON_ALPHANUMERIC)
        .to_string()
        .replace("%2D", "-")
        .replace("%5F", "_")
        .replace("%2E", ".")
}

/// A service principal's client credentials. `Debug` never prints the
/// secret.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AcaServicePrincipal {
    pub tenant_id:     String,
    pub client_id:     String,
    pub client_secret: String,
}

impl fmt::Debug for AcaServicePrincipal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcaServicePrincipal")
            .field("tenant_id", &self.tenant_id)
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}

impl AcaServicePrincipal {
    #[must_use]
    pub fn missing_fields(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if self.tenant_id.trim().is_empty() {
            missing.push("tenant_id");
        }
        if self.client_id.trim().is_empty() {
            missing.push("client_id");
        }
        if self.client_secret.is_empty() {
            missing.push("client_secret");
        }
        missing
    }
}

/// The policy an administrator sets on what ACA runs may use.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AcaPolicy {
    /// Disk images a run may name. Empty allows any image the group can
    /// see.
    #[serde(default)]
    pub allowed_images: Vec<String>,
    /// The image a run gets when its environment names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_image:  Option<String>,
    /// How long a stopped, retained sandbox lives before ACA deletes it.
    /// Capped at [`MAX_RETENTION_SECS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_secs: Option<u64>,
}

/// The longest a retained sandbox may live: 24 hours.
pub const MAX_RETENTION_SECS: u64 = 24 * 60 * 60;

impl AcaPolicy {
    /// The retention a sandbox is created with: the configured value,
    /// capped at 24 hours, and 24 hours when unset.
    #[must_use]
    pub fn effective_retention_secs(&self) -> u64 {
        self.retention_secs.map_or(MAX_RETENTION_SECS, |secs| {
            secs.clamp(60, MAX_RETENTION_SECS)
        })
    }

    /// Whether `image` may be used under this policy.
    #[must_use]
    pub fn allows_image(&self, image: &str) -> bool {
        self.allowed_images.is_empty()
            || self
                .allowed_images
                .iter()
                .any(|allowed| allowed.trim() == image.trim())
    }
}

/// The saved Azure connection: the target, the credential and the policy.
/// Persisted as one JSON vault secret, so `Debug` never prints the
/// credential.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AcaConnection {
    #[serde(flatten)]
    pub target:     AcaTarget,
    #[serde(flatten)]
    pub credential: AcaServicePrincipal,
    #[serde(default)]
    pub policy:     AcaPolicy,
}

impl AcaConnection {
    /// Parses the vault secret's JSON.
    pub fn from_json(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Every required field left empty, by setting name.
    #[must_use]
    pub fn missing_fields(&self) -> Vec<&'static str> {
        let mut missing = self.target.missing_fields();
        missing.extend(self.credential.missing_fields());
        missing
    }
}

/// Endpoints the client reaches. The defaults are Azure's public cloud;
/// tests point them at a local server.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcaEndpoints {
    pub authority:  String,
    pub arm:        String,
    /// The data plane's base URL. `None` derives
    /// `https://management.<location>.azuredevcompute.io` from the group's
    /// location.
    pub data_plane: Option<String>,
}

impl Default for AcaEndpoints {
    fn default() -> Self {
        Self {
            authority:  DEFAULT_AUTHORITY_URL.to_owned(),
            arm:        DEFAULT_ARM_URL.to_owned(),
            data_plane: None,
        }
    }
}

impl AcaEndpoints {
    pub(crate) fn data_plane_for(&self, location: &str) -> String {
        self.data_plane.clone().unwrap_or_else(|| {
            format!(
                "https://management.{}.azuredevcompute.io",
                location.trim().to_ascii_lowercase()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> AcaConnection {
        AcaConnection {
            target:     AcaTarget {
                subscription_id: "sub-1".into(),
                resource_group:  "rg-1".into(),
                sandbox_group:   "group-1".into(),
                location:        None,
            },
            credential: AcaServicePrincipal {
                tenant_id:     "tenant".into(),
                client_id:     "client".into(),
                client_secret: "aca-secret-value".into(),
            },
            policy:     AcaPolicy::default(),
        }
    }

    #[test]
    fn debug_never_prints_the_client_secret() {
        let rendered = format!("{:?}", connection());
        assert!(!rendered.contains("aca-secret-value"), "{rendered}");
        assert!(rendered.contains("client"));
    }

    #[test]
    fn the_connection_round_trips_as_flat_json() {
        let json = connection().to_json().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["subscription_id"], "sub-1");
        assert_eq!(value["client_secret"], "aca-secret-value");
        assert_eq!(AcaConnection::from_json(&json).unwrap(), connection());
    }

    #[test]
    fn missing_fields_names_every_empty_setting() {
        let mut connection = connection();
        connection.target.sandbox_group = " ".into();
        connection.credential.client_secret = String::new();
        assert_eq!(connection.missing_fields(), [
            "sandbox_group",
            "client_secret"
        ]);
    }

    #[test]
    fn retention_is_capped_at_a_day() {
        let mut policy = AcaPolicy::default();
        assert_eq!(policy.effective_retention_secs(), MAX_RETENTION_SECS);
        policy.retention_secs = Some(7 * 24 * 3600);
        assert_eq!(policy.effective_retention_secs(), MAX_RETENTION_SECS);
        policy.retention_secs = Some(3600);
        assert_eq!(policy.effective_retention_secs(), 3600);
    }

    #[test]
    fn an_empty_allow_list_allows_every_image() {
        let mut policy = AcaPolicy::default();
        assert!(policy.allows_image("ubuntu"));
        policy.allowed_images = vec!["azure-dev".into()];
        assert!(policy.allows_image("azure-dev"));
        assert!(!policy.allows_image("ubuntu"));
    }

    #[test]
    fn paths_encode_segments_and_keep_the_arm_provider_segment_apart() {
        let target = connection().target;
        assert_eq!(
            target.data_plane_group_path(),
            "/subscriptions/sub-1/resourceGroups/rg-1/sandboxGroups/group-1"
        );
        assert_eq!(
            target.arm_group_path(),
            "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.App/sandboxGroups/group-1"
        );
    }

    #[test]
    fn the_data_plane_is_regional() {
        assert_eq!(
            AcaEndpoints::default().data_plane_for("WestUS2"),
            "https://management.westus2.azuredevcompute.io"
        );
    }
}
