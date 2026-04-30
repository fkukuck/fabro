use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AzurePlatformConfig {
    pub subscription_id:          String,
    pub resource_group:           String,
    pub location:                 String,
    pub subnet_id:                String,
    pub acr_server:               String,
    #[serde(default)]
    pub acr_identity_resource_id: String,
    pub sandboxd_port:            u16,
}

impl AzurePlatformConfig {
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        serde_json::from_slice(&bytes)
            .map_err(|err| format!("failed to parse {}: {err}", path.display()))
    }

    pub fn from_env() -> Result<Self, String> {
        let required = [
            "FABRO_AZURE_SUBSCRIPTION_ID",
            "FABRO_AZURE_RESOURCE_GROUP",
            "FABRO_AZURE_LOCATION",
            "FABRO_AZURE_SANDBOX_SUBNET_ID",
            "FABRO_AZURE_ACR_SERVER",
            "FABRO_AZURE_ACR_IDENTITY_RESOURCE_ID",
        ];

        let missing: Vec<&str> = required
            .iter()
            .copied()
            .filter(|name| {
                std::env::var(name)
                    .ok()
                    .as_deref()
                    .is_none_or(str::is_empty)
            })
            .collect();

        if !missing.is_empty() {
            return Err(format!(
                "missing required Azure environment variables: {}",
                missing.join(", ")
            ));
        }

        let sandboxd_port = std::env::var("FABRO_AZURE_SANDBOXD_PORT")
            .ok()
            .filter(|value| !value.is_empty())
            .map(|value| {
                value.parse::<u16>().map_err(|err| {
                    format!("invalid FABRO_AZURE_SANDBOXD_PORT value {value:?}: {err}")
                })
            })
            .transpose()?
            .unwrap_or(7777);

        Ok(Self {
            subscription_id: std::env::var("FABRO_AZURE_SUBSCRIPTION_ID").unwrap_or_default(),
            resource_group: std::env::var("FABRO_AZURE_RESOURCE_GROUP").unwrap_or_default(),
            location: std::env::var("FABRO_AZURE_LOCATION").unwrap_or_default(),
            subnet_id: std::env::var("FABRO_AZURE_SANDBOX_SUBNET_ID").unwrap_or_default(),
            acr_server: std::env::var("FABRO_AZURE_ACR_SERVER").unwrap_or_default(),
            acr_identity_resource_id: std::env::var("FABRO_AZURE_ACR_IDENTITY_RESOURCE_ID")
                .unwrap_or_default(),
            sandboxd_port,
        })
    }
}

#[cfg(test)]
mod tests {
    use fabro_config::Storage;

    use super::AzurePlatformConfig;

    #[test]
    fn azure_platform_config_does_not_require_workspace_storage_env() {
        temp_env::with_vars(
            vec![
                ("FABRO_AZURE_SUBSCRIPTION_ID", Some("sub-1")),
                ("FABRO_AZURE_RESOURCE_GROUP", Some("rg-1")),
                ("FABRO_AZURE_LOCATION", Some("eastus")),
                (
                    "FABRO_AZURE_SANDBOX_SUBNET_ID",
                    Some(
                        "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci",
                    ),
                ),
                ("FABRO_AZURE_STORAGE_ACCOUNT", None::<&str>),
                ("FABRO_AZURE_STORAGE_SHARE", None::<&str>),
                ("FABRO_AZURE_STORAGE_KEY", None::<&str>),
                ("FABRO_AZURE_ACR_SERVER", Some("fabro.azurecr.io")),
                (
                    "FABRO_AZURE_ACR_IDENTITY_RESOURCE_ID",
                    Some(
                        "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull",
                    ),
                ),
            ],
            || {
                let config = AzurePlatformConfig::from_env().unwrap();
                assert_eq!(config.subscription_id, "sub-1");
                assert_eq!(config.acr_server, "fabro.azurecr.io");
                assert_eq!(
                    config.acr_identity_resource_id,
                    "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull"
                );
                assert_eq!(config.sandboxd_port, 7777);
            },
        );
    }

    #[test]
    fn azure_platform_config_loads_from_snapshot_file() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = Storage::new(dir.path()).runtime_directory();
        let path = runtime.azure_platform_config_path();
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "subscription_id": "sub-1",
                "resource_group": "rg-1",
                "location": "eastus",
                "subnet_id": "/subscriptions/sub-1/.../aci",
                "acr_server": "fabro.azurecr.io",
                "acr_identity_resource_id": "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull",
                "sandboxd_port": 7777,
            }))
            .unwrap(),
        )
        .unwrap();

        let loaded = AzurePlatformConfig::load_from_path(&path).unwrap();
        assert_eq!(loaded.subscription_id, "sub-1");
        assert_eq!(
            loaded.acr_identity_resource_id,
            "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull"
        );
    }
}
