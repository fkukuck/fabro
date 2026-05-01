use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub command:     String,
    pub working_dir: Option<String>,
    pub env:         HashMap<String, String>,
    pub timeout_ms:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResponse {
    pub stdout:      String,
    pub stderr:      String,
    pub exit_code:   i32,
    pub timed_out:   bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileResponse {
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileRequest {
    pub path:           String,
    pub content_base64: String,
}
