use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::outcome::StageOutcome;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatusRecord {
    pub status:         StageOutcome,
    #[serde(default)]
    pub notes:          Option<String>,
    #[serde(default)]
    pub failure_reason: Option<String>,
    pub timestamp:      DateTime<Utc>,
}
