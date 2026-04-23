use fabro_types::settings::{CliNamespace, FeaturesNamespace, ServerNamespace, SettingsLayer};
use serde::{Deserialize, Serialize};

use crate::resolve::Resolver;
use crate::user::load_settings_config;
use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSettings {
    pub server:   ServerNamespace,
    pub features: FeaturesNamespace,
}

impl ServerSettings {
    pub fn from_layer(layer: &SettingsLayer) -> Result<Self> {
        let resolver = Resolver::from_layer(layer);
        let mut errors = Vec::new();
        let server = resolver.server_into(&mut errors);
        let features = resolver.features_into(&mut errors);
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
        let resolver = Resolver::from_layer(layer);
        let mut errors = Vec::new();
        let cli = resolver.cli_into(&mut errors);
        let features = resolver.features_into(&mut errors);
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
