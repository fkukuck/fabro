//! Helpers for CLI code that manages the local Fabro server on this host.
//!
//! This module is the only generic CLI lifecycle surface allowed to read
//! `[server.*]` settings. User-facing CLI commands outside same-host server
//! lifecycle should not call into it.

use std::path::{Path, PathBuf};

use anyhow::Result;
use fabro_config::bind::BindRequest;
use fabro_config::{ServerSettingsBuilder, parse_settings_layer};
use fabro_types::ServerSettings;
use fabro_types::settings::{ServerAuthMethod, SettingsLayer};

use crate::user_config;

pub(crate) struct LocalServerConfig {
    storage_dir:      PathBuf,
    auth_methods:     Vec<ServerAuthMethod>,
    config_log_level: Option<String>,
    server_settings:  std::result::Result<ServerSettings, String>,
}

impl LocalServerConfig {
    pub(crate) fn load(config_path: Option<&Path>, storage_dir: Option<&Path>) -> Result<Self> {
        let settings =
            user_config::load_settings_with_config_and_storage_dir(config_path, storage_dir)?;
        Self::from_layer(&settings)
    }

    pub(crate) fn load_with_storage_dir(storage_dir: Option<&Path>) -> Result<Self> {
        let settings = user_config::load_settings_with_storage_dir(storage_dir)?;
        Self::from_layer(&settings)
    }

    fn from_layer(settings: &SettingsLayer) -> Result<Self> {
        let storage_dir = storage_dir(settings)?;
        let config_log_level = settings
            .server
            .as_ref()
            .and_then(|server| server.logging.as_ref())
            .and_then(|logging| logging.level.clone());
        let server_settings = resolved_server_settings(settings).map_err(|err| err.to_string());
        let auth_methods = server_settings
            .as_ref()
            .map(|resolved| resolved.server.auth.methods.clone())
            .unwrap_or_default();
        Ok(Self {
            storage_dir,
            auth_methods,
            config_log_level,
            server_settings,
        })
    }

    pub(crate) fn storage_dir(&self) -> &Path {
        &self.storage_dir
    }

    pub(crate) fn auth_methods(&self) -> &[ServerAuthMethod] {
        &self.auth_methods
    }

    pub(crate) fn config_log_level(&self) -> Option<&str> {
        self.config_log_level.as_deref()
    }

    pub(crate) fn bind_request(&self, cli_override: Option<&str>) -> Result<BindRequest> {
        let settings = self
            .server_settings
            .as_ref()
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        fabro_server::serve::resolve_bind_request_from_server_settings(settings, cli_override)
    }
}

pub(crate) fn storage_dir_from_toml(source: &str) -> Result<PathBuf> {
    let settings = parse_settings_layer(source)
        .map_err(|err| anyhow::anyhow!("failed to parse settings file: {err}"))?;
    storage_dir(&settings)
}

pub(crate) fn storage_dir(settings: &SettingsLayer) -> Result<PathBuf> {
    storage_dir_with_lookup(settings, &|name| std::env::var(name).ok())
}

pub(crate) fn storage_dir_with_lookup(
    settings: &SettingsLayer,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<PathBuf> {
    let storage_root = settings
        .server
        .as_ref()
        .and_then(|server| server.storage.as_ref())
        .and_then(|storage| storage.root.clone())
        .unwrap_or_else(|| {
            fabro_types::settings::InterpString::parse(
                &fabro_config::user::default_storage_dir().to_string_lossy(),
            )
        });
    let resolved_root = storage_root
        .resolve(lookup)
        .map_err(|err| anyhow::anyhow!("failed to resolve {}: {err}", storage_root.as_source()))?;
    Ok(PathBuf::from(resolved_root.value))
}

fn resolved_server_settings(settings: &SettingsLayer) -> Result<ServerSettings> {
    ServerSettingsBuilder::from_layer(settings).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::storage_dir_from_toml;

    #[test]
    fn storage_dir_from_toml_reads_explicit_root_without_full_server_resolution() {
        let path = storage_dir_from_toml(
            r#"
_version = 1

[server.storage]
root = "/srv/fabro"
"#,
        )
        .expect("storage root should resolve");

        assert_eq!(path, PathBuf::from("/srv/fabro"));
    }

    #[test]
    fn storage_dir_from_toml_defaults_without_auth_methods() {
        let path = storage_dir_from_toml("_version = 1\n").expect("default storage dir");

        assert_eq!(path, fabro_config::user::default_storage_dir());
    }
}
