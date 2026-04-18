use fabro_http::{HttpClient, HttpClientBuilder};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::azure::config::AzurePlatformConfig;
use crate::azure::resource_id::ContainerGroupResourceId;

#[derive(Clone, Debug)]
pub struct AzureArmClient {
    http:         HttpClient,
    config:       AzurePlatformConfig,
    arm_base_url: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ContainerGroupView {
    #[serde(default)]
    pub id:         Option<String>,
    #[serde(default)]
    pub name:       Option<String>,
    #[serde(default)]
    pub location:   Option<String>,
    #[serde(default)]
    pub properties: ContainerGroupPropertiesView,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ContainerGroupPropertiesView {
    #[serde(default, rename = "provisioningState")]
    pub provisioning_state: Option<String>,
    #[serde(default, rename = "instanceView")]
    pub instance_view:      Option<ContainerGroupInstanceView>,
    #[serde(default, rename = "ipAddress")]
    pub ip_address:         Option<ContainerGroupIpAddress>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ContainerGroupInstanceView {
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ContainerGroupIpAddress {
    #[serde(default)]
    pub ip:   Option<String>,
    #[serde(default)]
    pub fqdn: Option<String>,
}

impl AzureArmClient {
    pub fn new(config: AzurePlatformConfig) -> Result<Self, String> {
        let http = HttpClientBuilder::new()
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self {
            http,
            config,
            arm_base_url: "https://management.azure.com".to_string(),
        })
    }

    #[must_use]
    pub fn new_with_base_url(
        http: HttpClient,
        config: AzurePlatformConfig,
        arm_base_url: String,
    ) -> Self {
        Self {
            http,
            config,
            arm_base_url,
        }
    }

    pub async fn create_container_group(
        &self,
        name: &str,
        image: &str,
        cpu: f64,
        memory_gb: f64,
    ) -> Result<ContainerGroupResourceId, String> {
        let id = ContainerGroupResourceId::new(
            self.config.subscription_id.clone(),
            self.config.resource_group.clone(),
            name.to_string(),
        );
        let body = build_container_group_body(&self.config, name, image, cpu, memory_gb);
        self.put_json(id.arm_url_with_base(&self.arm_base_url), &body)
            .await?;
        Ok(id)
    }

    pub async fn get_container_group(
        &self,
        id: &ContainerGroupResourceId,
    ) -> Result<ContainerGroupView, String> {
        self.get_json(id.arm_url_with_base(&self.arm_base_url))
            .await
    }

    pub async fn delete_container_group(
        &self,
        id: &ContainerGroupResourceId,
    ) -> Result<(), String> {
        self.delete(id.arm_url_with_base(&self.arm_base_url)).await
    }

    async fn put_json(&self, url: String, body: &Value) -> Result<(), String> {
        self.authorized_request(reqwest::Method::PUT, url)
            .await?
            .json(body)
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    async fn get_json<T: DeserializeOwned>(&self, url: String) -> Result<T, String> {
        self.authorized_request(reqwest::Method::GET, url)
            .await?
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json()
            .await
            .map_err(|err| err.to_string())
    }

    async fn delete(&self, url: String) -> Result<(), String> {
        let response = self
            .authorized_request(reqwest::Method::DELETE, url)
            .await?
            .send()
            .await
            .map_err(|err| err.to_string())?;

        if response.status().is_success() || response.status() == reqwest::StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(response.error_for_status().err().map_or_else(
                || "Azure ARM delete failed".to_string(),
                |err| err.to_string(),
            ))
        }
    }

    async fn authorized_request(
        &self,
        method: reqwest::Method,
        url: String,
    ) -> Result<reqwest::RequestBuilder, String> {
        let token = self.acquire_bearer_token().await?;
        Ok(self.http.request(method, url).bearer_auth(token))
    }

    async fn acquire_bearer_token(&self) -> Result<String, String> {
        if let Ok(endpoint) = std::env::var("IDENTITY_ENDPOINT") {
            let header = std::env::var("IDENTITY_HEADER").map_err(|_| {
                "IDENTITY_HEADER is required when IDENTITY_ENDPOINT is set".to_string()
            })?;
            let resource = "https://management.azure.com/";
            let mut request = self
                .http
                .get(endpoint)
                .header("X-IDENTITY-HEADER", header)
                .query(&[("api-version", "2019-08-01"), ("resource", resource)]);
            if let Ok(client_id) = std::env::var("AZURE_CLIENT_ID") {
                request = request.query(&[("client_id", client_id)]);
            }
            let response: ManagedIdentityTokenResponse = request
                .send()
                .await
                .map_err(|err| err.to_string())?
                .error_for_status()
                .map_err(|err| err.to_string())?
                .json()
                .await
                .map_err(|err| err.to_string())?;
            return Ok(response.access_token);
        }

        let mut request = self
            .http
            .get("http://169.254.169.254/metadata/identity/oauth2/token")
            .header("Metadata", "true")
            .query(&[
                ("api-version", "2018-02-01"),
                ("resource", "https://management.azure.com/"),
            ]);
        if let Ok(client_id) = std::env::var("AZURE_CLIENT_ID") {
            request = request.query(&[("client_id", client_id)]);
        }
        let response: ManagedIdentityTokenResponse = request
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json()
            .await
            .map_err(|err| err.to_string())?;
        Ok(response.access_token)
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ManagedIdentityTokenResponse {
    access_token: String,
}

pub(crate) fn build_container_group_body(
    config: &AzurePlatformConfig,
    name: &str,
    image: &str,
    cpu: f64,
    memory_gb: f64,
) -> Value {
    let mut properties = json!({
        "containers": [
            {
                "name": name,
                "properties": {
                    "image": image,
                    "command": ["fabro-sandboxd"],
                    "ports": [{ "port": config.sandboxd_port }],
                    "resources": {
                        "requests": {
                            "cpu": cpu,
                            "memoryInGB": memory_gb,
                        }
                    },
                    "volumeMounts": [
                        {
                            "name": "workspace",
                            "mountPath": "/workspace",
                        }
                    ]
                }
            }
        ],
        "osType": "Linux",
        "restartPolicy": "Never",
        "ipAddress": {
            "type": "Private",
            "ports": [
                {
                    "protocol": "TCP",
                    "port": config.sandboxd_port,
                }
            ]
        },
        "subnetIds": [
            {
                "id": config.subnet_id,
            }
        ],
        "volumes": [
            {
                "name": "workspace",
                "azureFile": {
                    "shareName": config.storage_share,
                    "storageAccountName": config.storage_account,
                }
            }
        ]
    });

    if let (Some(username), Some(password)) = (&config.acr_username, &config.acr_password) {
        properties["imageRegistryCredentials"] = json!([
            {
                "server": config.acr_server,
                "username": username,
                "password": password,
            }
        ]);
    }

    json!({
        "name": name,
        "location": config.location,
        "properties": properties,
    })
}

#[cfg(test)]
mod tests {
    use super::build_container_group_body;
    use crate::azure::config::AzurePlatformConfig;

    #[test]
    fn build_container_group_body_includes_workspace_mount_and_resources() {
        let config = AzurePlatformConfig {
            subscription_id: "sub-1".to_string(),
            resource_group:  "rg-1".to_string(),
            location:        "eastus".to_string(),
            subnet_id:       "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci".to_string(),
            storage_account: "stor1".to_string(),
            storage_share:   "workspace".to_string(),
            acr_server:      "fabro.azurecr.io".to_string(),
            sandboxd_port:   7777,
            acr_username:    Some("user".to_string()),
            acr_password:    Some("pass".to_string()),
        };

        let body = build_container_group_body(
            &config,
            "fabro-run-1",
            "fabro.azurecr.io/fabro-sandboxes/base:latest",
            2.0,
            4.0,
        );

        assert_eq!(body["name"], "fabro-run-1");
        assert_eq!(body["location"], "eastus");
        assert!(body["properties"]["containers"][0]["image"].is_null());
        assert_eq!(
            body["properties"]["containers"][0]["properties"]["image"],
            "fabro.azurecr.io/fabro-sandboxes/base:latest"
        );
        assert_eq!(
            body["properties"]["containers"][0]["properties"]["resources"]["requests"]["cpu"],
            2.0
        );
        assert_eq!(
            body["properties"]["containers"][0]["properties"]["resources"]["requests"]["memoryInGB"],
            4.0
        );
        assert_eq!(
            body["properties"]["volumes"][0]["azureFile"]["shareName"],
            "workspace"
        );
        assert_eq!(
            body["properties"]["containers"][0]["properties"]["volumeMounts"][0]["mountPath"],
            "/workspace"
        );
    }
}
