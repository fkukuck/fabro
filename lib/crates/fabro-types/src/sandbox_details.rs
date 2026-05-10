use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SandboxDetails {
    pub provider:     String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name:         Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id:           Option<String>,
    pub state:        SandboxState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region:       Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image:        Option<String>,
    pub resources:    SandboxResources,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels:       BTreeMap<String, String>,
    pub timestamps:   SandboxTimestamps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxState {
    Unknown,
    Provisioning,
    Starting,
    Running,
    Stopping,
    Stopped,
    Paused,
    Deleting,
    Deleted,
    Archived,
    Restoring,
    Resizing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct SandboxResources {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_cores:    Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_bytes:   Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct SandboxTimestamps {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at:       Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use serde_json::json;

    use super::*;

    #[test]
    fn serializes_with_snake_case_state() {
        let details = SandboxDetails {
            provider:     "docker".to_string(),
            name:         Some("fabro-run-abc".to_string()),
            id:           Some("abcdef123456".to_string()),
            state:        SandboxState::Running,
            native_state: Some("running".to_string()),
            region:       None,
            image:        Some("ghcr.io/fabro/sandbox:latest".to_string()),
            resources:    SandboxResources {
                cpu_cores:    Some(2.0),
                memory_bytes: Some(4 * 1024 * 1024 * 1024),
                disk_bytes:   None,
            },
            labels:       BTreeMap::from([("run".to_string(), "abc".to_string())]),
            timestamps:   SandboxTimestamps {
                created_at:       Some(Utc.with_ymd_and_hms(2026, 5, 9, 12, 0, 0).unwrap()),
                last_activity_at: None,
            },
        };

        assert_eq!(
            serde_json::to_value(&details).unwrap(),
            json!({
                "provider": "docker",
                "name": "fabro-run-abc",
                "id": "abcdef123456",
                "state": "running",
                "native_state": "running",
                "image": "ghcr.io/fabro/sandbox:latest",
                "resources": {
                    "cpu_cores": 2.0,
                    "memory_bytes": 4_294_967_296_u64,
                },
                "labels": {
                    "run": "abc"
                },
                "timestamps": {
                    "created_at": "2026-05-09T12:00:00Z"
                }
            })
        );
    }

    #[test]
    fn deserializes_with_minimal_fields() {
        let details: SandboxDetails = serde_json::from_value(json!({
            "provider": "local",
            "state": "unknown",
            "resources": {},
            "timestamps": {}
        }))
        .unwrap();

        assert_eq!(details.provider, "local");
        assert_eq!(details.state, SandboxState::Unknown);
        assert!(details.name.is_none());
        assert!(details.id.is_none());
        assert!(details.image.is_none());
        assert!(details.labels.is_empty());
        assert_eq!(details.resources, SandboxResources::default());
        assert_eq!(details.timestamps, SandboxTimestamps::default());
    }

    #[test]
    fn state_serializes_each_variant_in_snake_case() {
        fn check(state: SandboxState, expected: &str) {
            assert_eq!(
                serde_json::to_value(state).unwrap(),
                serde_json::Value::String(expected.to_string()),
            );
        }
        check(SandboxState::Unknown, "unknown");
        check(SandboxState::Provisioning, "provisioning");
        check(SandboxState::Starting, "starting");
        check(SandboxState::Running, "running");
        check(SandboxState::Stopping, "stopping");
        check(SandboxState::Stopped, "stopped");
        check(SandboxState::Paused, "paused");
        check(SandboxState::Deleting, "deleting");
        check(SandboxState::Deleted, "deleted");
        check(SandboxState::Archived, "archived");
        check(SandboxState::Restoring, "restoring");
        check(SandboxState::Resizing, "resizing");
        check(SandboxState::Error, "error");
    }
}
