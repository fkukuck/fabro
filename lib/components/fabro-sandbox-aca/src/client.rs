//! The REST client for both planes: Entra client-credentials tokens, the
//! sandbox group on Azure Resource Manager, and the regional data plane.
//!
//! Requests that are safe to repeat (reads, deletes, file writes, stop and
//! resume) are retried on throttling, server errors and dropped
//! connections. A command launch is never retried here; the exec layer
//! makes its launch idempotent and decides itself.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use fabro_http::HttpClient;
use sandbox_driver::{
    AuthError, Error, ProviderError, ProviderKind, ResourceKind, Result, TransportError,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, OnceCell};

use crate::config::{API_VERSION, AcaEndpoints, AcaServicePrincipal, AcaTarget, DATA_PLANE_SCOPE};
use crate::models::Page;

/// How long before expiry a cached token is replaced.
const TOKEN_REFRESH_MARGIN: Duration = Duration::from_secs(300);
/// Attempts for a request that is safe to repeat.
const MAX_ATTEMPTS: u32 = 5;
const FIRST_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(10);
/// Per-request budget for ordinary calls; a command launch returns at once.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// A bound on list pages, so a looping link cannot spin forever.
const MAX_PAGES: usize = 100;

pub(crate) fn kind() -> ProviderKind {
    ProviderKind::try_new(crate::KIND).expect("the ACA provider kind is valid")
}

/// Whether a request may be sent again after a failure whose effect is
/// unknown.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Replay {
    Safe,
    Never,
}

#[derive(Clone)]
struct CachedToken {
    value:      String,
    expires_at: Instant,
}

/// A connected client. Cheap to clone; clones share the token cache.
#[derive(Clone)]
pub struct AcaClient {
    inner: Arc<Inner>,
}

struct Inner {
    http:       HttpClient,
    target:     AcaTarget,
    credential: AcaServicePrincipal,
    endpoints:  AcaEndpoints,
    tokens:     Mutex<BTreeMap<String, CachedToken>>,
    data_base:  OnceCell<String>,
}

impl fmt::Debug for AcaClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AcaClient")
            .field("target", &self.inner.target)
            .field("credential", &self.inner.credential)
            .finish_non_exhaustive()
    }
}

/// The sandbox group as ARM reports it.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct SandboxGroupInfo {
    #[serde(default)]
    pub id:         Option<String>,
    #[serde(default)]
    pub location:   Option<String>,
    #[serde(default)]
    pub properties: SandboxGroupProperties,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxGroupProperties {
    #[serde(default)]
    pub provisioning_state: Option<String>,
}

impl SandboxGroupInfo {
    /// ARM's `Succeeded` is the only ready state.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.properties
            .provisioning_state
            .as_deref()
            .is_none_or(|state| state.eq_ignore_ascii_case("succeeded"))
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    expires_in:   Option<u64>,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
    error: Option<ErrorBody>,
}

#[derive(Deserialize)]
struct ErrorBody {
    #[serde(default)]
    code:    Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// A failed response, as status and the service's own words.
#[derive(Debug)]
pub(crate) struct HttpFailure {
    pub(crate) status:  u16,
    pub(crate) code:    Option<String>,
    pub(crate) message: String,
}

impl HttpFailure {
    fn from_body(status: u16, body: &[u8]) -> Self {
        let parsed = serde_json::from_slice::<ErrorEnvelope>(body)
            .ok()
            .and_then(|envelope| envelope.error);
        let text = String::from_utf8_lossy(body).trim().to_owned();
        let (code, message) = match parsed {
            Some(body) => (body.code, body.message.unwrap_or_else(|| text.clone())),
            None => (None, text),
        };
        Self {
            status,
            code,
            message: truncate(&message, 500),
        }
    }

    /// The driver error for this failure on `resource` `id`.
    pub(crate) fn into_error(self, resource: ResourceKind, id: &str) -> Error {
        let detail = match &self.code {
            Some(code) => format!("{code}: {}", self.message),
            None => self.message.clone(),
        };
        match self.status {
            401 => Error::Auth(AuthError::new(
                kind(),
                format!(
                    "Azure rejected the credential (HTTP 401): {detail}. Check the tenant, client \
                     id and client secret, and whether the secret has expired"
                ),
            )),
            403 => Error::Auth(AuthError::new(
                kind(),
                format!(
                    "the service principal is not allowed to do this (HTTP 403): {detail}. Assign \
                     it the `{}` role on the sandbox group; a new assignment can take a few \
                     minutes to apply",
                    crate::config::DATA_OWNER_ROLE
                ),
            )),
            404 => Error::NotFound {
                resource,
                id: id.to_owned(),
            },
            429 => Error::RateLimited { retry_after: None },
            status => {
                let mut error = ProviderError::new(kind(), format!("HTTP {status}: {detail}"));
                error.code = self.code;
                error.retryable = status >= 500;
                Error::Provider(error)
            }
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// What a request carries.
pub(crate) enum Body {
    None,
    Json(serde_json::Value),
    Bytes(Vec<u8>),
}

impl AcaClient {
    #[must_use]
    pub fn new(
        target: AcaTarget,
        credential: AcaServicePrincipal,
        endpoints: AcaEndpoints,
        http: HttpClient,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                http,
                target,
                credential,
                endpoints,
                tokens: Mutex::new(BTreeMap::new()),
                data_base: OnceCell::new(),
            }),
        }
    }

    #[must_use]
    pub fn target(&self) -> &AcaTarget {
        &self.inner.target
    }

    /// A bearer token for `scope`, from the cache while it is fresh.
    async fn token(&self, scope: &str) -> Result<String> {
        let mut tokens = self.inner.tokens.lock().await;
        if let Some(token) = tokens.get(scope) {
            if token.expires_at > Instant::now() + TOKEN_REFRESH_MARGIN {
                return Ok(token.value.clone());
            }
        }
        let credential = &self.inner.credential;
        let url = format!(
            "{}/{}/oauth2/v2.0/token",
            self.inner.endpoints.authority.trim_end_matches('/'),
            credential.tenant_id.trim()
        );
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", credential.client_id.trim()),
            ("client_secret", credential.client_secret.as_str()),
            ("scope", scope),
        ];
        let response = self
            .inner
            .http
            .post(&url)
            .form(&form)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|error| {
                Error::Transport(TransportError::with_source(
                    "requesting an Azure access token",
                    error,
                ))
            })?;
        let status = response.status().as_u16();
        let body = response.bytes().await.map_err(|error| {
            Error::Transport(TransportError::with_source(
                "reading the Azure token response",
                error,
            ))
        })?;
        if !(200..300).contains(&status) {
            let failure = TokenFailure::from_body(status, &body);
            return Err(Error::Auth(AuthError::new(kind(), failure.to_string())));
        }
        let parsed: TokenResponse = serde_json::from_slice(&body).map_err(|error| {
            Error::Auth(AuthError::new(
                kind(),
                format!("the Azure token response was not understood: {error}"),
            ))
        })?;
        let lifetime = Duration::from_secs(parsed.expires_in.unwrap_or(3600));
        tokens.insert(scope.to_owned(), CachedToken {
            value:      parsed.access_token.clone(),
            expires_at: Instant::now() + lifetime,
        });
        Ok(parsed.access_token)
    }

    fn arm_scope(&self) -> String {
        format!(
            "{}/.default",
            self.inner.endpoints.arm.trim_end_matches('/')
        )
    }

    /// The sandbox group from ARM: proves the credential, the role on the
    /// group, and where its data plane is.
    pub async fn sandbox_group(&self) -> Result<SandboxGroupInfo> {
        let url = format!(
            "{}{}?api-version={API_VERSION}",
            self.inner.endpoints.arm.trim_end_matches('/'),
            self.inner.target.arm_group_path()
        );
        let token = self.token(&self.arm_scope()).await?;
        let bytes = self
            .send_with_retry(
                &token,
                reqwest::Method::GET,
                &url,
                &Body::None,
                Replay::Safe,
            )
            .await
            .map_err(|failure| match failure {
                SendError::Http(failure) if failure.status == 404 => {
                    Error::Provider(ProviderError::new(
                        kind(),
                        format!(
                            "the sandbox group {} was not found in resource group {} of \
                             subscription {}",
                            self.inner.target.sandbox_group,
                            self.inner.target.resource_group,
                            self.inner.target.subscription_id
                        ),
                    ))
                }
                other => other.into_error(ResourceKind::Unknown, "sandbox group"),
            })?;
        serde_json::from_slice(&bytes).map_err(|error| {
            Error::Provider(ProviderError::new(
                kind(),
                format!("the sandbox group response was not understood: {error}"),
            ))
        })
    }

    /// The regional data plane's base URL, resolved once from the group's
    /// location when the target does not name it.
    async fn data_base(&self) -> Result<&str> {
        self.inner
            .data_base
            .get_or_try_init(|| async {
                if let Some(base) = &self.inner.endpoints.data_plane {
                    return Ok(base.trim_end_matches('/').to_owned());
                }
                let location = match &self.inner.target.location {
                    Some(location) if !location.trim().is_empty() => location.clone(),
                    _ => self.sandbox_group().await?.location.ok_or_else(|| {
                        Error::Provider(ProviderError::new(
                            kind(),
                            "the sandbox group reports no location",
                        ))
                    })?,
                };
                Ok(self.inner.endpoints.data_plane_for(&location))
            })
            .await
            .map(String::as_str)
    }

    /// The data-plane URL for `suffix` below the group, with the API
    /// version and `query` appended.
    pub(crate) async fn data_url(&self, suffix: &str, query: &[(&str, &str)]) -> Result<String> {
        let base = self.data_base().await?;
        let mut url = format!(
            "{base}{}{suffix}?api-version={API_VERSION}",
            self.inner.target.data_plane_group_path()
        );
        for (key, value) in query {
            url.push('&');
            url.push_str(key);
            url.push('=');
            url.push_str(
                &percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC)
                    .to_string(),
            );
        }
        Ok(url)
    }

    /// Sends a data-plane request and returns the body bytes.
    pub(crate) async fn data(
        &self,
        method: reqwest::Method,
        suffix: &str,
        query: &[(&str, &str)],
        body: Body,
        replay: Replay,
    ) -> std::result::Result<Vec<u8>, SendError> {
        let url = self
            .data_url(suffix, query)
            .await
            .map_err(SendError::Driver)?;
        let token = self
            .token(DATA_PLANE_SCOPE)
            .await
            .map_err(SendError::Driver)?;
        self.send_with_retry(&token, method, &url, &body, replay)
            .await
    }

    /// Sends a data-plane request and decodes its JSON answer.
    pub(crate) async fn data_json<T: DeserializeOwned>(
        &self,
        method: reqwest::Method,
        suffix: &str,
        query: &[(&str, &str)],
        body: Body,
        replay: Replay,
    ) -> std::result::Result<T, SendError> {
        let bytes = self.data(method, suffix, query, body, replay).await?;
        serde_json::from_slice(&bytes).map_err(|error| {
            SendError::Driver(Error::Provider(ProviderError::new(
                kind(),
                format!("the sandbox service's response was not understood: {error}"),
            )))
        })
    }

    /// Every item of a paged data-plane list, following `nextLink` while
    /// it stays on the data plane's host.
    pub(crate) async fn data_list<T: DeserializeOwned>(
        &self,
        suffix: &str,
        query: &[(&str, &str)],
    ) -> std::result::Result<Vec<T>, SendError> {
        let base = self
            .data_base()
            .await
            .map_err(SendError::Driver)?
            .to_owned();
        let mut url = self
            .data_url(suffix, query)
            .await
            .map_err(SendError::Driver)?;
        let mut items = Vec::new();
        for _ in 0..MAX_PAGES {
            let token = self
                .token(DATA_PLANE_SCOPE)
                .await
                .map_err(SendError::Driver)?;
            let bytes = self
                .send_with_retry(
                    &token,
                    reqwest::Method::GET,
                    &url,
                    &Body::None,
                    Replay::Safe,
                )
                .await?;
            let page: Page<T> = serde_json::from_slice(&bytes).map_err(|error| {
                SendError::Driver(Error::Provider(ProviderError::new(
                    kind(),
                    format!("the sandbox service's list was not understood: {error}"),
                )))
            })?;
            let (page_items, next) = page.into_parts();
            items.extend(page_items);
            match next {
                Some(next) if next.starts_with(&format!("{base}/")) => url = next,
                Some(next) => {
                    tracing::warn!(next = %redact_query(&next), "ignoring an off-host ACA next page link");
                    break;
                }
                None => break,
            }
        }
        Ok(items)
    }

    async fn send_with_retry(
        &self,
        token: &str,
        method: reqwest::Method,
        url: &str,
        body: &Body,
        replay: Replay,
    ) -> std::result::Result<Vec<u8>, SendError> {
        let mut backoff = FIRST_BACKOFF;
        let mut attempt = 1;
        loop {
            let outcome = self.send_once(token, method.clone(), url, body).await;
            let retry_after = match &outcome {
                Ok(_) => return outcome,
                Err(SendError::Http(failure))
                    if replay == Replay::Safe
                        && (failure.status == 429 || failure.status >= 500) =>
                {
                    Some(backoff)
                }
                Err(SendError::Transport(_)) if replay == Replay::Safe => Some(backoff),
                Err(_) => None,
            };
            match retry_after {
                Some(wait) if attempt < MAX_ATTEMPTS => {
                    tracing::debug!(attempt, url = %redact_query(url), "retrying ACA request");
                    tokio::time::sleep(wait).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                    attempt += 1;
                }
                _ => return outcome,
            }
        }
    }

    async fn send_once(
        &self,
        token: &str,
        method: reqwest::Method,
        url: &str,
        body: &Body,
    ) -> std::result::Result<Vec<u8>, SendError> {
        let mut request = self
            .inner
            .http
            .request(method, url)
            .bearer_auth(token)
            .header("x-ms-client-request-id", uuid::Uuid::new_v4().to_string())
            .timeout(REQUEST_TIMEOUT);
        request = match body {
            Body::None => request.header("accept", "application/json"),
            Body::Json(value) => request.json(value),
            Body::Bytes(bytes) => request
                .header("content-type", "application/octet-stream")
                .body(bytes.clone()),
        };
        let response = request
            .send()
            .await
            .map_err(|error| SendError::Transport(error.without_url().to_string()))?;
        let status = response.status().as_u16();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| SendError::Transport(error.without_url().to_string()))?;
        if (200..300).contains(&status) {
            Ok(bytes.to_vec())
        } else {
            Err(SendError::Http(HttpFailure::from_body(status, &bytes)))
        }
    }
}

fn redact_query(url: &str) -> &str {
    url.split('?').next().unwrap_or(url)
}

/// Why a request failed: the service answered with an error, the
/// connection failed so the effect is unknown, or the request was never
/// sent.
#[derive(Debug)]
pub(crate) enum SendError {
    Http(HttpFailure),
    Transport(String),
    Driver(Error),
}

impl SendError {
    pub(crate) fn into_error(self, resource: ResourceKind, id: &str) -> Error {
        match self {
            Self::Http(failure) => failure.into_error(resource, id),
            Self::Transport(message) => Error::Transport(TransportError::new(format!(
                "the Azure sandbox service could not be reached: {message}"
            ))),
            Self::Driver(error) => error,
        }
    }

    pub(crate) fn status(&self) -> Option<u16> {
        match self {
            Self::Http(failure) => Some(failure.status),
            _ => None,
        }
    }
}

struct TokenFailure {
    status:      u16,
    error:       Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct TokenErrorBody {
    #[serde(default)]
    error:             Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

impl TokenFailure {
    fn from_body(status: u16, body: &[u8]) -> Self {
        let parsed = serde_json::from_slice::<TokenErrorBody>(body).ok();
        Self {
            status,
            error: parsed.as_ref().and_then(|body| body.error.clone()),
            description: parsed.and_then(|body| body.error_description).map(|text| {
                // Entra appends trace and correlation ids on new lines.
                truncate(text.lines().next().unwrap_or_default(), 300)
            }),
        }
    }
}

impl fmt::Display for TokenFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hint = match self.error.as_deref() {
            Some("invalid_client") => {
                "the client secret is wrong or has expired; create a new secret for the app \
                 registration and save it again"
            }
            Some("unauthorized_client") => {
                "the client id is not known in this tenant; check the tenant id and client id"
            }
            Some("invalid_request") => "check the tenant id",
            _ => "check the tenant id, client id and client secret",
        };
        write!(
            f,
            "Azure did not issue a token (HTTP {}{}): {hint}",
            self.status,
            self.description
                .as_deref()
                .map(|text| format!(", {text}"))
                .unwrap_or_default()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_failures_explain_an_expired_secret_without_echoing_it() {
        let failure = TokenFailure::from_body(
            401,
            br#"{"error":"invalid_client","error_description":"AADSTS7000222: The provided client secret keys are expired.\r\nTrace ID: 1"}"#,
        );
        let text = failure.to_string();
        assert!(text.contains("expired"), "{text}");
        assert!(!text.contains("Trace ID"), "{text}");
    }

    #[test]
    fn a_403_names_the_role_to_assign() {
        let failure = HttpFailure::from_body(
            403,
            br#"{"error":{"code":"AuthorizationFailed","message":"no access"}}"#,
        );
        let error = failure.into_error(ResourceKind::Sandbox, "x");
        assert!(matches!(error, Error::Auth(_)));
        assert!(error.to_string().contains("SandboxGroup Data Owner"));
    }

    #[test]
    fn a_404_is_not_found_for_the_resource() {
        let error = HttpFailure::from_body(404, b"").into_error(ResourceKind::Sandbox, "sb-1");
        assert!(matches!(error, Error::NotFound {
            resource: ResourceKind::Sandbox,
            ..
        }));
    }

    #[test]
    fn group_readiness_follows_provisioning_state() {
        let mut group = SandboxGroupInfo::default();
        assert!(group.is_ready());
        group.properties.provisioning_state = Some("Creating".into());
        assert!(!group.is_ready());
        group.properties.provisioning_state = Some("Succeeded".into());
        assert!(group.is_ready());
    }
}
