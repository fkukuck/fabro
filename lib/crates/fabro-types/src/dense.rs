use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::settings::{
    CliNamespace, FeaturesNamespace, InterpString, ObjectStoreSettings, ProjectNamespace,
    RunNamespace, ServerNamespace, WorkflowNamespace,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettings {
    pub server:   ServerNamespace,
    pub features: FeaturesNamespace,
}

impl ServerSettings {
    #[must_use]
    pub fn with_storage_override(mut self, path: &Path) -> Self {
        let previous_storage_root = self.server.storage.root.clone();
        self.server.storage.root = InterpString::parse(&path.display().to_string());
        override_local_object_store_root(
            &mut self.server.artifacts.store,
            &previous_storage_root,
            path,
            "artifacts",
        );
        override_local_object_store_root(
            &mut self.server.slatedb.store,
            &previous_storage_root,
            path,
            "slatedb",
        );
        self
    }
}

fn override_local_object_store_root(
    store: &mut ObjectStoreSettings,
    previous_storage_root: &InterpString,
    storage_root: &Path,
    domain: &str,
) {
    let ObjectStoreSettings::Local { root } = store else {
        return;
    };
    if root.as_source() != default_local_object_store_root(previous_storage_root, domain) {
        return;
    }
    *root = InterpString::parse(
        &storage_root
            .join("objects")
            .join(domain)
            .display()
            .to_string(),
    );
}

fn default_local_object_store_root(storage_root: &InterpString, domain: &str) -> String {
    let root = storage_root.as_source();
    let root = root.trim_end_matches('/');
    format!("{root}/objects/{domain}")
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::ServerSettings;
    use crate::settings::{InterpString, ObjectStoreSettings, ServerNamespace};

    fn test_server_settings() -> ServerSettings {
        let mut server = ServerNamespace::test_default();
        server.storage.root = InterpString::parse("/storage");
        ServerSettings {
            server,
            features: crate::settings::FeaturesNamespace::default(),
        }
    }

    #[test]
    fn with_storage_override_preserves_explicit_local_object_store_roots() {
        let mut settings = test_server_settings();
        settings.server.artifacts.store = ObjectStoreSettings::Local {
            root: InterpString::parse("/tmp/fabro-objects"),
        };
        settings.server.slatedb.store = ObjectStoreSettings::Local {
            root: InterpString::parse("/tmp/fabro-objects"),
        };

        let updated = settings.with_storage_override(Path::new("/srv/fabro-storage"));

        let ObjectStoreSettings::Local {
            root: artifacts_root,
        } = &updated.server.artifacts.store
        else {
            panic!("artifacts store should stay local");
        };
        let ObjectStoreSettings::Local {
            root: slatedb_root,
        } = &updated.server.slatedb.store
        else {
            panic!("slatedb store should stay local");
        };

        assert_eq!(updated.server.storage.root.as_source(), "/srv/fabro-storage");
        assert_eq!(artifacts_root.as_source(), "/tmp/fabro-objects");
        assert_eq!(slatedb_root.as_source(), "/tmp/fabro-objects");
    }

    #[test]
    fn with_storage_override_updates_default_local_object_store_roots() {
        let mut settings = test_server_settings();
        settings.server.artifacts.store = ObjectStoreSettings::Local {
            root: InterpString::parse("/storage/objects/artifacts"),
        };
        settings.server.slatedb.store = ObjectStoreSettings::Local {
            root: InterpString::parse("/storage/objects/slatedb"),
        };

        let updated = settings.with_storage_override(Path::new("/srv/fabro-storage"));

        let ObjectStoreSettings::Local {
            root: artifacts_root,
        } = &updated.server.artifacts.store
        else {
            panic!("artifacts store should stay local");
        };
        let ObjectStoreSettings::Local {
            root: slatedb_root,
        } = &updated.server.slatedb.store
        else {
            panic!("slatedb store should stay local");
        };

        assert_eq!(artifacts_root.as_source(), "/srv/fabro-storage/objects/artifacts");
        assert_eq!(slatedb_root.as_source(), "/srv/fabro-storage/objects/slatedb");
    }
}
