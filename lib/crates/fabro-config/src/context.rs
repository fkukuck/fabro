use fabro_types::settings::{CliNamespace, FeaturesNamespace, ServerNamespace, SettingsLayer};
use serde::{Deserialize, Serialize};

use crate::resolve::{resolve_cli, resolve_features, resolve_server};
use crate::user::load_settings_config;
use crate::{Error, Result, apply_builtin_defaults};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettings {
    pub server:   ServerNamespace,
    pub features: FeaturesNamespace,
}

impl ServerSettings {
    pub fn from_layer(layer: &SettingsLayer) -> Result<Self> {
        let layer = apply_builtin_defaults(layer.clone());
        let mut errors = Vec::new();
        let server_layer = layer.server.clone().unwrap_or_default();
        let features_layer = layer.features.clone().unwrap_or_default();
        let server = resolve_server(&server_layer, &mut errors);
        let features = resolve_features(&features_layer, &mut errors);
        if errors.is_empty() {
            Ok(Self { server, features })
        } else {
            Err(Error::resolve("failed to resolve server settings", errors))
        }
    }

    pub fn resolve() -> Result<Self> {
        let layer = load_settings_config(None)?;
        Self::from_layer(&layer)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UserSettings {
    pub cli:      CliNamespace,
    pub features: FeaturesNamespace,
}

impl UserSettings {
    pub fn from_layer(layer: &SettingsLayer) -> Result<Self> {
        let layer = apply_builtin_defaults(layer.clone());
        let mut errors = Vec::new();
        let cli_layer = layer.cli.clone().unwrap_or_default();
        let features_layer = layer.features.clone().unwrap_or_default();
        let cli = resolve_cli(&cli_layer, &mut errors);
        let features = resolve_features(&features_layer, &mut errors);
        if errors.is_empty() {
            Ok(Self { cli, features })
        } else {
            Err(Error::resolve("failed to resolve user settings", errors))
        }
    }

    pub fn resolve() -> Result<Self> {
        let layer = load_settings_config(None)?;
        Self::from_layer(&layer)
    }
}
