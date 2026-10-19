//! The Azure Container Apps Sandboxes connection as entered, and its test.
//! Shared by Settings (`/sandbox-providers/aca/connection`) and the install
//! wizard's sandbox step.

use fabro_petri::providers::AcaCredentials;
use fabro_sandbox_aca::{
    AcaConnection, AcaImage, AcaPolicy, AcaServicePrincipal, AcaTarget, DATA_OWNER_ROLE,
    READER_ROLE,
};
use sandbox_driver::{Error as DriverError, HealthStatus, SandboxProvider};
use serde::{Deserialize, Serialize};

/// How long a connection test may take before it is reported as
/// unreachable.
const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// The connection as entered; see `AcaConnectionInput` in the API spec.
/// `Debug` never prints the client secret.
#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ConnectionInput {
    pub(crate) tenant_id:       Option<String>,
    pub(crate) client_id:       Option<String>,
    pub(crate) client_secret:   Option<String>,
    pub(crate) subscription_id: Option<String>,
    pub(crate) resource_group:  Option<String>,
    pub(crate) sandbox_group:   Option<String>,
    pub(crate) location:        Option<String>,
    pub(crate) allowed_images:  Option<Vec<String>>,
    pub(crate) default_image:   Option<String>,
    pub(crate) retention_hours: Option<u64>,
}

impl std::fmt::Debug for ConnectionInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConnectionInput")
            .field("tenant_id", &self.tenant_id)
            .field("client_id", &self.client_id)
            .field("subscription_id", &self.subscription_id)
            .field("resource_group", &self.resource_group)
            .field("sandbox_group", &self.sandbox_group)
            .finish_non_exhaustive()
    }
}

impl ConnectionInput {
    pub(crate) fn is_empty(&self) -> bool {
        self.tenant_id.is_none()
            && self.client_id.is_none()
            && self.client_secret.is_none()
            && self.subscription_id.is_none()
            && self.resource_group.is_none()
            && self.sandbox_group.is_none()
            && self.location.is_none()
            && self.allowed_images.is_none()
            && self.default_image.is_none()
            && self.retention_hours.is_none()
    }

    /// The connection this input describes, over `saved`: a field left out
    /// keeps the saved value, so a secret need not be entered again.
    pub(crate) fn resolve(self, saved: Option<&AcaConnection>) -> Result<AcaConnection, String> {
        fn field(value: Option<String>, saved: Option<&str>) -> String {
            trimmed(value)
                .or_else(|| saved.map(str::to_owned))
                .unwrap_or_default()
        }
        let retention_secs = match self.retention_hours {
            Some(hours) if (1..=24).contains(&hours) => Some(hours * 3600),
            Some(_) => return Err("retention_hours must be between 1 and 24".to_owned()),
            None => saved.and_then(|saved| saved.policy.retention_secs),
        };
        let connection = AcaConnection {
            target:     AcaTarget {
                subscription_id: field(
                    self.subscription_id,
                    saved.map(|saved| saved.target.subscription_id.as_str()),
                ),
                resource_group:  field(
                    self.resource_group,
                    saved.map(|saved| saved.target.resource_group.as_str()),
                ),
                sandbox_group:   field(
                    self.sandbox_group,
                    saved.map(|saved| saved.target.sandbox_group.as_str()),
                ),
                location:        match self.location {
                    Some(location) => trimmed(Some(location)),
                    None => saved.and_then(|saved| saved.target.location.clone()),
                },
            },
            credential: AcaServicePrincipal {
                tenant_id:     field(
                    self.tenant_id,
                    saved.map(|saved| saved.credential.tenant_id.as_str()),
                ),
                client_id:     field(
                    self.client_id,
                    saved.map(|saved| saved.credential.client_id.as_str()),
                ),
                client_secret: self
                    .client_secret
                    .filter(|secret| !secret.trim().is_empty())
                    .or_else(|| saved.map(|saved| saved.credential.client_secret.clone()))
                    .unwrap_or_default(),
            },
            policy:     AcaPolicy {
                allowed_images: match self.allowed_images {
                    Some(images) => images
                        .into_iter()
                        .filter_map(|image| trimmed(Some(image)))
                        .collect(),
                    None => saved
                        .map(|saved| saved.policy.allowed_images.clone())
                        .unwrap_or_default(),
                },
                default_image: match self.default_image {
                    Some(image) => trimmed(Some(image)),
                    None => saved.and_then(|saved| saved.policy.default_image.clone()),
                },
                retention_secs,
            },
        };
        let missing = connection.missing_fields();
        if !missing.is_empty() {
            return Err(format!("missing {}", missing.join(", ")));
        }
        if let Some(default) = &connection.policy.default_image {
            if !connection.policy.allows_image(default) {
                return Err(format!(
                    "the default image `{default}` is not on the approved image list"
                ));
            }
        }
        Ok(connection)
    }
}

fn trimmed(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// What a connection test found; see `AcaConnectionTestResult`.
#[derive(Debug, Default, Serialize)]
pub(crate) struct TestResult {
    pub(crate) ready:          bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message:        Option<String>,
    pub(crate) problems:       Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) location:       Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) image:          Option<String>,
    pub(crate) images:         Vec<AcaImage>,
    pub(crate) required_roles: Vec<String>,
}

impl TestResult {
    pub(crate) fn failed(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            ready: false,
            message: Some(message.clone()),
            problems: vec![message],
            required_roles: required_roles(),
            ..Self::default()
        }
    }
}

fn required_roles() -> Vec<String> {
    vec![
        format!("{READER_ROLE} on the sandbox group"),
        format!("{DATA_OWNER_ROLE} on the sandbox group"),
    ]
}

/// Tests `connection` end to end without creating anything: a token, the
/// group on ARM, the data-plane role, and the default image.
pub(crate) async fn run_test(
    connection: &AcaConnection,
    http: Option<fabro_http::HttpClient>,
) -> TestResult {
    let credentials = AcaCredentials::new(connection.clone()).with_http_client(http);
    let test = async {
        let provider = fabro_sandbox_aca::connect(
            credentials.connection(),
            credentials.endpoints().clone(),
            match credentials.http_client() {
                Some(http) => http.clone(),
                None => fabro_http::http_client()
                    .map_err(|err| DriverError::invalid_spec("http", err.to_string()))?,
            },
        );
        let health = provider.health().await?;
        if health.status != HealthStatus::Ok {
            let mut result = TestResult::failed(
                health
                    .message
                    .unwrap_or_else(|| "Azure did not accept the connection".to_owned()),
            );
            if !health.missing_permissions.is_empty() {
                result.problems.push(format!(
                    "missing role: {}",
                    health.missing_permissions.join(", ")
                ));
            }
            return Ok(result);
        }
        let report = provider
            .check_ready(None, &sandbox_driver::Resources::default())
            .await?;
        let images = provider.images().await.unwrap_or_default();
        Ok::<_, DriverError>(TestResult {
            ready: report.problems.is_empty(),
            message: (!report.problems.is_empty()).then(|| report.problems.join("; ")),
            problems: report.problems,
            location: report.location,
            image: report.image,
            images,
            required_roles: required_roles(),
        })
    };
    match tokio::time::timeout(TEST_TIMEOUT, test).await {
        Ok(Ok(result)) => result,
        Ok(Err(err)) => TestResult::failed(err.to_string()),
        Err(_) => TestResult::failed(format!(
            "Azure did not answer within {} seconds",
            TEST_TIMEOUT.as_secs()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ConnectionInput {
        ConnectionInput {
            tenant_id: Some("tenant".into()),
            client_id: Some("client".into()),
            client_secret: Some("secret".into()),
            subscription_id: Some("sub".into()),
            resource_group: Some("rg".into()),
            sandbox_group: Some("group".into()),
            ..ConnectionInput::default()
        }
    }

    #[test]
    fn an_omitted_secret_keeps_the_saved_one() {
        let saved = input().resolve(None).unwrap();
        let updated = ConnectionInput {
            client_secret: None,
            sandbox_group: Some("other".into()),
            ..input()
        }
        .resolve(Some(&saved))
        .unwrap();
        assert_eq!(updated.credential.client_secret, "secret");
        assert_eq!(updated.target.sandbox_group, "other");
    }

    #[test]
    fn a_missing_field_is_named() {
        let error = ConnectionInput {
            sandbox_group: None,
            ..input()
        }
        .resolve(None)
        .unwrap_err();
        assert!(error.contains("sandbox_group"), "{error}");
    }

    #[test]
    fn retention_is_limited_to_a_day() {
        let error = ConnectionInput {
            retention_hours: Some(48),
            ..input()
        }
        .resolve(None)
        .unwrap_err();
        assert!(error.contains("24"), "{error}");
        let connection = ConnectionInput {
            retention_hours: Some(2),
            ..input()
        }
        .resolve(None)
        .unwrap();
        assert_eq!(connection.policy.retention_secs, Some(7200));
    }

    #[test]
    fn the_default_image_must_be_approved() {
        let error = ConnectionInput {
            allowed_images: Some(vec!["azure-dev".into()]),
            default_image: Some("ubuntu".into()),
            ..input()
        }
        .resolve(None)
        .unwrap_err();
        assert!(error.contains("ubuntu"), "{error}");
    }
}
