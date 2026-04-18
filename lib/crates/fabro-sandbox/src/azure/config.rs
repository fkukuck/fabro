#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AzurePlatformConfig {
    pub subscription_id: String,
    pub resource_group:  String,
    pub location:        String,
    pub subnet_id:       String,
    pub storage_account: String,
    pub storage_share:   String,
    pub acr_server:      String,
    pub sandboxd_port:   u16,
    pub acr_username:    Option<String>,
    pub acr_password:    Option<String>,
}

impl AzurePlatformConfig {
    pub fn from_env() -> Result<Self, String> {
        let required = [
            "FABRO_AZURE_SUBSCRIPTION_ID",
            "FABRO_AZURE_RESOURCE_GROUP",
            "FABRO_AZURE_LOCATION",
            "FABRO_AZURE_SANDBOX_SUBNET_ID",
            "FABRO_AZURE_STORAGE_ACCOUNT",
            "FABRO_AZURE_STORAGE_SHARE",
            "FABRO_AZURE_ACR_SERVER",
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
            storage_account: std::env::var("FABRO_AZURE_STORAGE_ACCOUNT").unwrap_or_default(),
            storage_share: std::env::var("FABRO_AZURE_STORAGE_SHARE").unwrap_or_default(),
            acr_server: std::env::var("FABRO_AZURE_ACR_SERVER").unwrap_or_default(),
            sandboxd_port,
            acr_username: std::env::var("FABRO_AZURE_ACR_USERNAME").ok(),
            acr_password: std::env::var("FABRO_AZURE_ACR_PASSWORD").ok(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AzurePlatformConfig;

    #[test]
    fn azure_platform_config_requires_core_env() {
        temp_env::with_vars(
            vec![
                ("FABRO_AZURE_SUBSCRIPTION_ID", None::<&str>),
                ("FABRO_AZURE_RESOURCE_GROUP", None::<&str>),
                ("FABRO_AZURE_LOCATION", None::<&str>),
                ("FABRO_AZURE_SANDBOX_SUBNET_ID", None::<&str>),
                ("FABRO_AZURE_STORAGE_ACCOUNT", None::<&str>),
                ("FABRO_AZURE_STORAGE_SHARE", None::<&str>),
                ("FABRO_AZURE_ACR_SERVER", None::<&str>),
            ],
            || {
                let err = AzurePlatformConfig::from_env().unwrap_err();
                assert!(err.contains("FABRO_AZURE_SUBSCRIPTION_ID"));
            },
        );
    }
}
