use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::outcome::StageStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conclusion {
    pub timestamp: DateTime<Utc>,
    pub status: StageStatus,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_git_commit_sha: Option<String>,
}

impl Conclusion {
    pub fn save(&self, path: &Path) -> Result<()> {
        crate::save_json(self, path, "conclusion")
    }

    pub fn load(path: &Path) -> Result<Self> {
        crate::load_json(path, "conclusion")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_conclusion() -> Conclusion {
        Conclusion {
            timestamp: Utc::now(),
            status: crate::outcome::StageStatus::Success,
            duration_ms: 12345,
            failure_reason: None,
            final_git_commit_sha: Some("deadbeef".to_string()),
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conclusion.json");

        let conclusion = sample_conclusion();
        conclusion.save(&path).unwrap();
        let loaded = Conclusion::load(&path).unwrap();

        assert_eq!(loaded.status, crate::outcome::StageStatus::Success);
        assert_eq!(loaded.duration_ms, 12345);
        assert!(loaded.failure_reason.is_none());
        assert_eq!(
            loaded.final_git_commit_sha.as_deref(),
            Some("deadbeef")
        );
    }

    #[test]
    fn load_nonexistent_file() {
        let result = Conclusion::load(Path::new("/nonexistent/conclusion.json"));
        assert!(result.is_err());
    }

    #[test]
    fn load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not json").unwrap();

        let result = Conclusion::load(&path);
        assert!(result.is_err());
    }

    #[test]
    fn optional_fields_omitted_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conclusion.json");

        let conclusion = Conclusion {
            timestamp: Utc::now(),
            status: crate::outcome::StageStatus::Fail,
            duration_ms: 500,
            failure_reason: None,
            final_git_commit_sha: None,
        };
        conclusion.save(&path).unwrap();

        let raw: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(raw.get("failure_reason").is_none());
        assert!(raw.get("final_git_commit_sha").is_none());
    }

    #[test]
    fn failure_reason_present() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conclusion.json");

        let conclusion = Conclusion {
            timestamp: Utc::now(),
            status: crate::outcome::StageStatus::Fail,
            duration_ms: 100,
            failure_reason: Some("timeout".to_string()),
            final_git_commit_sha: None,
        };
        conclusion.save(&path).unwrap();
        let loaded = Conclusion::load(&path).unwrap();

        assert_eq!(loaded.failure_reason.as_deref(), Some("timeout"));
    }
}
