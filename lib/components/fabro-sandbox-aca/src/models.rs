//! The data plane's JSON, read leniently: every field is optional and the
//! aliases the SDKs accept are accepted here too.

use std::collections::BTreeMap;

use sandbox_driver::{Resources, SandboxState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireSandbox {
    pub(crate) id:            String,
    #[serde(default)]
    pub(crate) state:         Option<String>,
    #[serde(default)]
    pub(crate) state_details: Option<WireStateDetails>,
    #[serde(default)]
    pub(crate) labels:        BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) resources:     Option<WireResources>,
    #[serde(default)]
    pub(crate) sources_ref:   Option<WireSourcesRef>,
    #[serde(default)]
    pub(crate) region:        Option<String>,
    #[serde(default)]
    pub(crate) created_at:    Option<String>,
    #[serde(default)]
    pub(crate) egress_policy: Option<WireEgressPolicy>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireStateDetails {
    #[serde(default)]
    pub(crate) stopped_reason: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct WireResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) cpu:    Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) memory: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) disk:   Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireSourcesRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) disk_image: Option<WireDiskImageRef>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireDiskImageRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) name:      Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) id:        Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) is_public: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireEgressPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) default_action: Option<String>,
}

/// A list page: `{"value": [...], "nextLink": ...}` or a bare array.
#[derive(Deserialize)]
#[serde(untagged)]
pub(crate) enum Page<T> {
    Wrapped {
        #[serde(default = "Vec::new")]
        value:     Vec<T>,
        #[serde(default, rename = "nextLink")]
        next_link: Option<String>,
    },
    Bare(Vec<T>),
}

impl<T> Page<T> {
    /// The page's items and the absolute link to the next page.
    pub(crate) fn into_parts(self) -> (Vec<T>, Option<String>) {
        match self {
            Self::Wrapped { value, next_link } => {
                (value, next_link.filter(|link| !link.trim().is_empty()))
            }
            Self::Bare(value) => (value, None),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireShellResult {
    #[serde(default)]
    pub(crate) exit_code: Option<i32>,
    #[serde(default)]
    pub(crate) stdout:    String,
    #[serde(default)]
    pub(crate) stderr:    String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireDiskImage {
    #[serde(default)]
    pub(crate) id:     Option<String>,
    #[serde(default)]
    pub(crate) name:   Option<String>,
    #[serde(default)]
    pub(crate) labels: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) status: Option<WireImageStatus>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct WireImageStatus {
    #[serde(default)]
    pub(crate) state:   Option<String>,
    #[serde(default)]
    pub(crate) message: Option<String>,
}

impl WireDiskImage {
    /// The name a run refers to the image by: its name, its `name` label,
    /// or its id.
    pub(crate) fn display_name(&self) -> Option<String> {
        self.name
            .clone()
            .or_else(|| self.labels.get("name").cloned())
            .or_else(|| self.id.clone())
    }
}

/// The driver's state for ACA's.
pub(crate) fn sandbox_state(state: Option<&str>) -> SandboxState {
    match state.map(str::to_ascii_lowercase).as_deref() {
        Some("creating" | "pending" | "provisioning") => SandboxState::Creating,
        Some("running") => SandboxState::Running,
        Some("stopping" | "suspending") => SandboxState::Stopping,
        Some("stopped" | "suspended" | "idle") => SandboxState::Stopped,
        Some("resuming" | "starting") => SandboxState::Starting,
        Some("deleting") => SandboxState::Deleting,
        Some("deleted") => SandboxState::Deleted,
        Some("failed" | "error") => SandboxState::Error,
        _ => SandboxState::Unknown,
    }
}

/// A Kubernetes-style quantity as whole MiB: `2048Mi`, `4Gi`, `20G`.
pub(crate) fn quantity_mib(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let number: f64 = number.parse().ok()?;
    let mib = match unit {
        "Ki" => number / 1024.0,
        "Mi" | "M" => number,
        "Gi" | "G" => number * 1024.0,
        "Ti" | "T" => number * 1024.0 * 1024.0,
        "" => number / (1024.0 * 1024.0),
        _ => return None,
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a quantity the service reported, rounded to whole MiB"
    )]
    Some(mib.round() as u64)
}

/// Millicores as whole cores, rounding up: `"2000m"` → 2, `"500m"` → 1.
pub(crate) fn quantity_cores(text: &str) -> Option<u32> {
    let text = text.trim();
    let millis = match text.strip_suffix('m') {
        Some(millis) => millis.parse::<f64>().ok()?,
        None => text.parse::<f64>().ok()? * 1000.0,
    };
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a CPU quantity the service reported, rounded up to whole cores"
    )]
    Some((millis / 1000.0).ceil() as u32)
}

impl WireResources {
    pub(crate) fn from_driver(resources: &Resources) -> Option<Self> {
        if resources.cpu_cores.is_none() && resources.memory_mb.is_none() {
            return None;
        }
        Some(Self {
            cpu:    Some(format!("{}m", resources.cpu_cores.unwrap_or(1) * 1000)),
            memory: Some(format!("{}Mi", resources.memory_mb.unwrap_or(2048))),
            disk:   resources.disk_mb.map(|mb| format!("{mb}Mi")),
        })
    }

    pub(crate) fn to_driver(&self) -> Resources {
        let mut resources = Resources::default();
        resources.cpu_cores = self.cpu.as_deref().and_then(quantity_cores);
        resources.memory_mb = self.memory.as_deref().and_then(quantity_mib);
        resources.disk_mb = self.disk.as_deref().and_then(quantity_mib);
        resources
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_map_suspended_and_idle_to_stopped() {
        assert_eq!(sandbox_state(Some("Running")), SandboxState::Running);
        assert_eq!(sandbox_state(Some("Suspended")), SandboxState::Stopped);
        assert_eq!(sandbox_state(Some("Idle")), SandboxState::Stopped);
        assert_eq!(sandbox_state(Some("Failed")), SandboxState::Error);
        assert_eq!(sandbox_state(Some("Novel")), SandboxState::Unknown);
    }

    #[test]
    fn quantities_round_trip_through_driver_resources() {
        assert_eq!(quantity_mib("2048Mi"), Some(2048));
        assert_eq!(quantity_mib("4Gi"), Some(4096));
        assert_eq!(quantity_cores("2000m"), Some(2));
        assert_eq!(quantity_cores("250m"), Some(1));
        assert_eq!(quantity_cores("4"), Some(4));
        let mut resources = Resources::default();
        resources.cpu_cores = Some(2);
        resources.memory_mb = Some(4096);
        let wire = WireResources::from_driver(&resources).unwrap();
        assert_eq!(wire.cpu.as_deref(), Some("2000m"));
        assert_eq!(wire.to_driver().memory_mb, Some(4096));
    }

    #[test]
    fn pages_accept_both_shapes() {
        let wrapped: Page<WireSandbox> =
            serde_json::from_str(r#"{"value":[{"id":"a"}],"nextLink":null}"#).unwrap();
        assert_eq!(wrapped.into_parts().0.len(), 1);
        let bare: Page<WireSandbox> = serde_json::from_str(r#"[{"id":"a"},{"id":"b"}]"#).unwrap();
        assert_eq!(bare.into_parts().0.len(), 2);
    }
}
