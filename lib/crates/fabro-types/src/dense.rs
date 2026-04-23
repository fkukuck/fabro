use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::settings::{
    CliNamespace, FeaturesNamespace, InterpString, ProjectNamespace, RunNamespace,
    ServerNamespace, WorkflowNamespace,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettings {
    pub server:   ServerNamespace,
    pub features: FeaturesNamespace,
}

impl ServerSettings {
    #[must_use]
    pub fn with_storage_override(mut self, path: &Path) -> Self {
        self.server.storage.root = InterpString::parse(&path.display().to_string());
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UserSettings {
    pub cli:      CliNamespace,
    pub features: FeaturesNamespace,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkflowSettings {
    pub project:  ProjectNamespace,
    pub workflow: WorkflowNamespace,
    pub run:      RunNamespace,
}

impl WorkflowSettings {
    #[must_use]
    pub fn combined_labels(&self) -> HashMap<String, String> {
        let mut labels = self.project.metadata.clone();
        labels.extend(self.workflow.metadata.clone());
        labels.extend(self.run.metadata.clone());
        labels
    }
}
