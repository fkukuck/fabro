use anyhow::Context as _;
use fabro_config::RuntimeDirectory;
use fabro_sandbox::azure::config::AzurePlatformConfig;
use fabro_static::EnvVars;
use fabro_types::settings::server::ServerNamespace;

pub(crate) fn resolve_azure_platform_config(
    settings: &ServerNamespace,
    vault_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<AzurePlatformConfig>, String> {
    let Some(platform) = settings
        .sandbox
        .azure
        .as_ref()
        .and_then(|azure| azure.platform.as_ref())
    else {
        return Ok(None);
    };

    let acr_username = vault_lookup(EnvVars::FABRO_AZURE_ACR_USERNAME)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let acr_password = vault_lookup(EnvVars::FABRO_AZURE_ACR_PASSWORD)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if acr_username.is_some() ^ acr_password.is_some() {
        return Err(
            "Azure ACR credentials must be configured together. Run fabro install to update the Azure step."
                .to_string(),
        );
    }

    Ok(Some(AzurePlatformConfig {
        subscription_id: platform.subscription_id.clone(),
        resource_group: platform.resource_group.clone(),
        location: platform.location.clone(),
        subnet_id: platform.subnet_id.clone(),
        acr_server: platform.acr_server.clone(),
        sandboxd_port: platform.sandboxd_port,
        acr_username,
        acr_password,
    }))
}

pub(crate) fn write_azure_platform_snapshot(
    runtime_directory: &RuntimeDirectory,
    config: Option<AzurePlatformConfig>,
) -> anyhow::Result<()> {
    let path = runtime_directory.azure_platform_config_path();
    match config {
        Some(config) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("creating directory {}", parent.display()))?;
            }
            let bytes = serde_json::to_vec_pretty(&serde_json::json!({
                "subscription_id": config.subscription_id,
                "resource_group": config.resource_group,
                "location": config.location,
                "subnet_id": config.subnet_id,
                "acr_server": config.acr_server,
                "sandboxd_port": config.sandboxd_port,
                "acr_username": config.acr_username,
                "acr_password": config.acr_password,
            }))?;
            std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        }
        None => match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(anyhow::Error::new(err).context(format!("removing {}", path.display())));
            }
        },
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use fabro_config::ServerSettingsBuilder;
    use fabro_static::EnvVars;

    use super::resolve_azure_platform_config;

    #[test]
    fn resolve_azure_platform_config_reads_settings_and_vault_secrets() {
        let settings = ServerSettingsBuilder::from_toml(
            r#"
_version = 1

[server.auth]
methods = ["dev-token"]

[server.sandbox.azure.platform]
subscription_id = "sub-1"
resource_group = "rg-1"
location = "eastus"
subnet_id = "/subscriptions/sub-1/.../aci"
acr_server = "fabro.azurecr.io"
"#,
        )
        .unwrap()
        .server;

        let resolved = resolve_azure_platform_config(&settings, &|name| match name {
            EnvVars::FABRO_AZURE_ACR_USERNAME => Some("azure-user".to_string()),
            EnvVars::FABRO_AZURE_ACR_PASSWORD => Some("azure-pass".to_string()),
            _ => None,
        })
        .unwrap()
        .expect("azure config should resolve");

        assert_eq!(resolved.subscription_id, "sub-1");
        assert_eq!(resolved.acr_username.as_deref(), Some("azure-user"));
    }
}
