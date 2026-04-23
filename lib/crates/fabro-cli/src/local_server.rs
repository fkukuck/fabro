//! Helpers for CLI code that manages the local Fabro server on this host.
//!
//! This module is the only generic CLI lifecycle surface allowed to read
//! `[server.*]` settings. User-facing CLI commands outside same-host server
//! lifecycle should not call into it.

use std::path::PathBuf;

use anyhow::Result;
use fabro_config::bind::BindRequest;
use fabro_config::ServerSettingsBuilder;
use fabro_types::settings::{ServerAuthMethod, SettingsLayer};
use fabro_types::ServerSettings;

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

pub(crate) fn bind_request(
    settings: &SettingsLayer,
    cli_override: Option<&str>,
) -> Result<BindRequest> {
    fabro_server::serve::resolve_bind_request_from_settings(settings, cli_override)
}

pub(crate) fn auth_methods(settings: &SettingsLayer) -> Vec<ServerAuthMethod> {
    resolved_server_settings(settings)
        .map(|resolved| resolved.server.auth.methods)
        .unwrap_or_default()
}

pub(crate) fn config_log_level(settings: &SettingsLayer) -> Option<String> {
    resolved_server_settings(settings)
        .ok()
        .and_then(|settings| settings.server.logging.level)
}

fn resolved_server_settings(settings: &SettingsLayer) -> Result<ServerSettings> {
    ServerSettingsBuilder::from_layer(settings).map_err(Into::into)
}
