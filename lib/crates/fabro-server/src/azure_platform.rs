use fabro_sandbox::azure::config::AzurePlatformConfig;
use fabro_types::settings::server::ServerNamespace;

pub(crate) fn resolve_azure_platform_config(
    settings: &ServerNamespace,
    _vault_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<AzurePlatformConfig>, String> {
    let Some(platform) = settings
        .sandbox
        .azure
        .as_ref()
        .and_then(|azure| azure.platform.as_ref())
    else {
        return Ok(None);
    };

    Ok(Some(AzurePlatformConfig {
        subscription_id:          platform.subscription_id.clone(),
        resource_group:           platform.resource_group.clone(),
        location:                 platform.location.clone(),
        subnet_id:                platform.subnet_id.clone(),
        acr_server:               platform.acr_server.clone(),
        acr_identity_resource_id: platform.acr_identity_resource_id.clone(),
        sandboxd_port:            platform.sandboxd_port,
    }))
}

#[cfg(test)]
mod tests {
    use fabro_config::ServerSettingsBuilder;
    use fabro_sandbox::azure::config::AzurePlatformConfig;

    use super::resolve_azure_platform_config;

    #[test]
    fn resolve_azure_platform_config_reads_settings_only() {
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
acr_identity_resource_id = "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/fabro-acr"
"#,
        )
        .unwrap()
        .server;

        let resolved = resolve_azure_platform_config(&settings, &|_| None)
            .unwrap()
            .expect("azure config should resolve");

        assert_eq!(
            resolved,
            AzurePlatformConfig {
                subscription_id: "sub-1".into(),
                resource_group: "rg-1".into(),
                location: "eastus".into(),
                subnet_id: "/subscriptions/sub-1/.../aci".into(),
                acr_server: "fabro.azurecr.io".into(),
                acr_identity_resource_id: "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/fabro-acr".into(),
                sandboxd_port: 7777,
            }
        );
    }
}
