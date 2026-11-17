//! The Azure connection an administrator saves from Settings, for Azure
//! Container Apps (ACA) sandboxes.
//!
//! The connection is one row in `azure_connection`, beside the vault rather
//! than in it: a vault token can be resolved into a run's environment by
//! name, and the service principal's client secret must never reach a run.
//! The secret is written once and never returned; the API reports only
//! whether one is saved and when it last changed.
//!
//! The readiness check proves what it can without creating a sandbox or
//! starting anything billable: it signs in, reads the sandbox group from
//! Azure Resource Manager, and lists the group's disk images on the data
//! plane. Running workflows on ACA is not part of this module.

use std::fmt;
use std::time::Duration;

use chrono::{DateTime, Utc};
use fabro_db::DbPool;
use fabro_http::HttpClient;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;

/// The Azure public cloud's Entra authority.
const DEFAULT_AUTHORITY_URL: &str = "https://login.microsoftonline.com";
/// Azure Resource Manager in the public cloud.
const DEFAULT_ARM_URL: &str = "https://management.azure.com";
/// The ACA sandbox API version both planes accept.
const API_VERSION: &str = "2026-02-01-preview";
/// The OAuth scope of the sandbox data plane.
const DATA_PLANE_SCOPE: &str = "https://dynamicsessions.io/.default";
/// The data-plane role the service principal needs on the sandbox group.
pub(crate) const DATA_ROLE: &str = "Container Apps SandboxGroup Data Owner";
/// The built-in disk image every sandbox group offers.
pub(crate) const BUILT_IN_IMAGE: &str = "ubuntu";
/// The budget for each request a readiness check makes.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// The sandbox sizes an ACA execution environment may request, as
/// `(name, millicores, MiB)`.
pub(crate) const SANDBOX_SIZES: &[(&str, u32, u32)] = &[
    ("XS", 250, 512),
    ("S", 500, 1024),
    ("M", 1000, 2048),
    ("L", 2000, 4096),
    ("XL", 4000, 8192),
];

/// The non-secret part of the connection, as the API reads and writes it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct AzureConnectionSettings {
    pub(crate) tenant_id:       String,
    pub(crate) client_id:       String,
    pub(crate) subscription_id: String,
    pub(crate) location:        String,
    pub(crate) resource_group:  String,
    pub(crate) sandbox_group:   String,
    #[serde(default)]
    pub(crate) execution:       AcaExecutionSettings,
}

/// The ACA execution environment: what a sandbox would boot from and how
/// large it would be, and the administrator's policy over both.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct AcaExecutionSettings {
    /// Whether ACA execution is allowed at all.
    pub(crate) enabled:        bool,
    /// The disk image a sandbox boots from.
    pub(crate) image:          String,
    /// The images an environment may choose. Empty allows any image.
    #[serde(default)]
    pub(crate) allowed_images: Vec<String>,
    pub(crate) cpu_millicores: u32,
    pub(crate) memory_mib:     u32,
}

impl Default for AcaExecutionSettings {
    fn default() -> Self {
        Self {
            enabled:        true,
            image:          BUILT_IN_IMAGE.to_owned(),
            allowed_images: Vec::new(),
            cpu_millicores: 1000,
            memory_mib:     2048,
        }
    }
}

impl AzureConnectionSettings {
    /// The same settings with surrounding whitespace removed and blank
    /// allowed images dropped.
    #[must_use]
    pub(crate) fn normalized(mut self) -> Self {
        for field in [
            &mut self.tenant_id,
            &mut self.client_id,
            &mut self.subscription_id,
            &mut self.location,
            &mut self.resource_group,
            &mut self.sandbox_group,
            &mut self.execution.image,
        ] {
            *field = field.trim().to_owned();
        }
        self.location = normalize_location(&self.location);
        self.execution.allowed_images = self
            .execution
            .allowed_images
            .iter()
            .map(|image| image.trim().to_owned())
            .filter(|image| !image.is_empty())
            .collect();
        self
    }

    /// Every problem a person must fix before the settings can be saved,
    /// in the words the form shows.
    #[must_use]
    pub(crate) fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (value, label) in [
            (&self.tenant_id, "Tenant ID"),
            (&self.client_id, "Client ID"),
            (&self.subscription_id, "Subscription ID"),
            (&self.location, "Location"),
            (&self.resource_group, "Resource group"),
            (&self.sandbox_group, "Sandbox group"),
        ] {
            if value.is_empty() {
                problems.push(format!("{label} is required."));
            }
        }
        if !self.client_id.is_empty() && !is_guid(&self.client_id) {
            problems.push("Client ID must be a GUID, as shown on the app registration.".to_owned());
        }
        if !self.subscription_id.is_empty() && !is_guid(&self.subscription_id) {
            problems.push("Subscription ID must be a GUID.".to_owned());
        }
        for (value, label) in [
            (&self.resource_group, "Resource group"),
            (&self.sandbox_group, "Sandbox group"),
        ] {
            if value.contains(['/', '?', '#']) {
                problems.push(format!("{label} must be a name, not a path."));
            }
        }
        problems.extend(self.execution.problems());
        problems
    }
}

impl AcaExecutionSettings {
    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.image.is_empty() {
            problems.push("Image is required.".to_owned());
        } else if !self.allows_image(&self.image) {
            problems.push(format!(
                "Image {} is not in the list of allowed images.",
                self.image
            ));
        }
        if sandbox_size(self.cpu_millicores, self.memory_mib).is_none() {
            problems.push(format!(
                "{} is not a supported sandbox size. Choose one of {}.",
                describe_size(self.cpu_millicores, self.memory_mib),
                SANDBOX_SIZES
                    .iter()
                    .map(|(name, cpu, memory)| format!("{name} ({})", describe_size(*cpu, *memory)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        problems
    }

    /// Whether `image` may be used under the allowed-images policy.
    #[must_use]
    pub(crate) fn allows_image(&self, image: &str) -> bool {
        self.allowed_images.is_empty() || self.allowed_images.iter().any(|allowed| allowed == image)
    }
}

/// The named size for a CPU and memory pair, if it is one ACA offers.
#[must_use]
pub(crate) fn sandbox_size(cpu_millicores: u32, memory_mib: u32) -> Option<&'static str> {
    SANDBOX_SIZES
        .iter()
        .find(|(_, cpu, memory)| *cpu == cpu_millicores && *memory == memory_mib)
        .map(|(name, _, _)| *name)
}

fn describe_size(cpu_millicores: u32, memory_mib: u32) -> String {
    let cores = f64::from(cpu_millicores) / 1000.0;
    let gib = f64::from(memory_mib) / 1024.0;
    format!("{cores} vCPU / {gib} GiB")
}

fn is_guid(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    groups.len() == 5
        && groups
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(group, len)| group.len() == len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Azure's programmatic region name: `West US 2` is `westus2`.
fn normalize_location(location: &str) -> String {
    location
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
}

/// A service principal's client secret. `Debug` and `Display` never print
/// it.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ClientSecret(String);

impl ClientSecret {
    /// A secret from what a person typed; blank is no secret.
    #[must_use]
    pub(crate) fn from_input(value: Option<String>) -> Option<Self> {
        value.filter(|value| !value.trim().is_empty()).map(Self)
    }

    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ClientSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ClientSecret(<redacted>)")
    }
}

/// The saved connection.
#[derive(Clone, Debug)]
pub(crate) struct StoredAzureConnection {
    pub(crate) settings:          AzureConnectionSettings,
    pub(crate) client_secret:     ClientSecret,
    pub(crate) secret_updated_at: DateTime<Utc>,
    pub(crate) updated_at:        DateTime<Utc>,
}

/// What the API returns about the connection: never the secret.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct AzureConnectionView {
    pub(crate) configured:        bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) settings:          Option<AzureConnectionSettings>,
    pub(crate) client_secret_set: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) secret_updated_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) updated_at:        Option<DateTime<Utc>>,
}

impl AzureConnectionView {
    #[must_use]
    pub(crate) fn of(stored: Option<&StoredAzureConnection>) -> Self {
        match stored {
            Some(stored) => Self {
                configured:        true,
                settings:          Some(stored.settings.clone()),
                client_secret_set: true,
                secret_updated_at: Some(stored.secret_updated_at),
                updated_at:        Some(stored.updated_at),
            },
            None => Self {
                configured:        false,
                settings:          None,
                client_secret_set: false,
                secret_updated_at: None,
                updated_at:        None,
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum AzureStoreError {
    #[error("a client secret is required to save a new Azure connection")]
    SecretRequired,
    #[error("the saved Azure connection could not be read: {0}")]
    Corrupt(#[from] serde_json::Error),
    #[error("the saved Azure connection has an unreadable timestamp: {0}")]
    Timestamp(String),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Reads the saved connection, if there is one.
pub(crate) async fn load(pool: &DbPool) -> Result<Option<StoredAzureConnection>, AzureStoreError> {
    let row = sqlx::query(
        "SELECT config, client_secret, secret_updated_at, updated_at FROM azure_connection WHERE \
         id = 1",
    )
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let config: String = row.try_get("config")?;
    let secret_updated_at: String = row.try_get("secret_updated_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    Ok(Some(StoredAzureConnection {
        settings:          serde_json::from_str(&config)?,
        client_secret:     ClientSecret(row.try_get("client_secret")?),
        secret_updated_at: parse_timestamp(&secret_updated_at)?,
        updated_at:        parse_timestamp(&updated_at)?,
    }))
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, AzureStoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| AzureStoreError::Timestamp(value.to_owned()))
}

/// Saves the connection. A `None` secret keeps the saved one, so a person
/// can change the details without typing the secret again.
pub(crate) async fn save(
    pool: &DbPool,
    settings: &AzureConnectionSettings,
    secret: Option<ClientSecret>,
) -> Result<StoredAzureConnection, AzureStoreError> {
    let mut tx = pool.begin().await?;
    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT client_secret, secret_updated_at FROM azure_connection WHERE id = 1",
    )
    .fetch_optional(&mut *tx)
    .await?;
    let now = Utc::now();
    let (client_secret, secret_updated_at) = match (secret, existing) {
        (Some(secret), _) => (secret, now),
        (None, Some((saved, updated_at))) => (ClientSecret(saved), parse_timestamp(&updated_at)?),
        (None, None) => return Err(AzureStoreError::SecretRequired),
    };
    let config = serde_json::to_string(settings)?;
    sqlx::query(
        "INSERT INTO azure_connection (id, config, client_secret, secret_updated_at, updated_at) \
         VALUES (1, ?, ?, ?, ?) ON CONFLICT (id) DO UPDATE SET config = excluded.config, \
         client_secret = excluded.client_secret, secret_updated_at = excluded.secret_updated_at, \
         updated_at = excluded.updated_at",
    )
    .bind(&config)
    .bind(client_secret.expose())
    .bind(secret_updated_at.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StoredAzureConnection {
        settings: settings.clone(),
        client_secret,
        secret_updated_at,
        updated_at: now,
    })
}

/// Removes the connection. Returns whether there was one.
pub(crate) async fn delete(pool: &DbPool) -> Result<bool, AzureStoreError> {
    let result = sqlx::query("DELETE FROM azure_connection WHERE id = 1")
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Where the readiness check sends its requests. Tests point these at a
/// fake.
#[derive(Clone, Debug)]
pub(crate) struct AzureEndpoints {
    pub(crate) authority:  String,
    pub(crate) arm:        String,
    /// The data plane's base URL; derived from the location when unset.
    pub(crate) data_plane: Option<String>,
}

impl Default for AzureEndpoints {
    fn default() -> Self {
        Self {
            authority:  DEFAULT_AUTHORITY_URL.to_owned(),
            arm:        DEFAULT_ARM_URL.to_owned(),
            data_plane: None,
        }
    }
}

impl AzureEndpoints {
    fn data_plane_for(&self, location: &str) -> String {
        self.data_plane.clone().map_or_else(
            || format!("https://management.{location}.azuredevcompute.io"),
            |base| base.trim_end_matches('/').to_owned(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckStatus {
    Passed,
    Failed,
    /// Not run because an earlier check failed.
    Skipped,
}

/// One line of the readiness report.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReadinessCheck {
    pub(crate) id:      &'static str,
    pub(crate) label:   &'static str,
    pub(crate) status:  CheckStatus,
    pub(crate) message: String,
}

/// The readiness report. `ready` holds only when every check passed.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReadinessReport {
    pub(crate) ready:      bool,
    pub(crate) checks:     Vec<ReadinessCheck>,
    pub(crate) checked_at: DateTime<Utc>,
}

const CHECKS: &[(&str, &str)] = &[
    ("settings", "Connection details"),
    ("credentials", "Credentials"),
    ("sandbox_group", "Sandbox group"),
    ("data_plane", "Sandbox permissions"),
    ("image", "Image"),
    ("resources", "Computing resources"),
];

struct Report {
    checks: Vec<ReadinessCheck>,
}

impl Report {
    fn record(&mut self, id: &'static str, status: CheckStatus, message: impl Into<String>) {
        let label = CHECKS
            .iter()
            .find(|(check, _)| *check == id)
            .map_or(id, |(_, label)| *label);
        self.checks.push(ReadinessCheck {
            id,
            label,
            status,
            message: message.into(),
        });
    }

    fn pass(&mut self, id: &'static str, message: impl Into<String>) {
        self.record(id, CheckStatus::Passed, message);
    }

    fn fail(&mut self, id: &'static str, message: impl Into<String>) {
        self.record(id, CheckStatus::Failed, message);
    }

    /// Marks every check not yet recorded as skipped, and finishes.
    fn finish(mut self, reason: &str) -> ReadinessReport {
        for (id, _) in CHECKS {
            if !self.checks.iter().any(|check| check.id == *id) {
                self.record(id, CheckStatus::Skipped, reason);
            }
        }
        let ready = self
            .checks
            .iter()
            .all(|check| check.status == CheckStatus::Passed);
        ReadinessReport {
            ready,
            checks: self.checks,
            checked_at: Utc::now(),
        }
    }
}

/// Checks that `settings` and `secret` can reach a usable sandbox group,
/// without creating a sandbox. Every request is a read.
pub(crate) async fn check_readiness(
    http: &HttpClient,
    endpoints: &AzureEndpoints,
    settings: &AzureConnectionSettings,
    secret: Option<&ClientSecret>,
) -> ReadinessReport {
    let mut report = Report { checks: Vec::new() };
    let mut problems = settings.problems();
    if secret.is_none() {
        problems.insert(0, "Client secret is required.".to_owned());
    }
    if !problems.is_empty() {
        report.fail("settings", problems.join(" "));
        return report.finish("Fix the connection details first.");
    }
    report.pass("settings", "All required details are filled in.");
    let Some(secret) = secret else {
        return report.finish("Fix the connection details first.");
    };

    let arm_scope = format!("{}/.default", endpoints.arm.trim_end_matches('/'));
    let arm_token = match token(http, endpoints, settings, secret, &arm_scope).await {
        Ok(token) => token,
        Err(message) => {
            report.fail("credentials", message);
            return report.finish("Fix the credentials first.");
        }
    };
    report.pass(
        "credentials",
        "Signed in to Microsoft Entra ID as the app registration.",
    );

    match sandbox_group(http, endpoints, settings, &arm_token).await {
        Ok(()) => report.pass(
            "sandbox_group",
            format!(
                "Sandbox group {} is provisioned in {}.",
                settings.sandbox_group, settings.location
            ),
        ),
        Err(message) => {
            report.fail("sandbox_group", message);
            return report.finish("Fix access to the sandbox group first.");
        }
    }

    let images = match data_plane_images(http, endpoints, settings, secret).await {
        Ok(images) => {
            report.pass(
                "data_plane",
                "The app registration can read the sandbox group's data plane.",
            );
            images
        }
        Err(message) => {
            report.fail("data_plane", message);
            return report.finish("Fix the sandbox permissions first.");
        }
    };

    match image_readiness(&settings.execution.image, &images) {
        Ok(message) => report.pass("image", message),
        Err(message) => report.fail("image", message),
    }

    match sandbox_size(
        settings.execution.cpu_millicores,
        settings.execution.memory_mib,
    ) {
        Some(name) => report.pass(
            "resources",
            format!(
                "{name} ({}) is a supported sandbox size.",
                describe_size(
                    settings.execution.cpu_millicores,
                    settings.execution.memory_mib
                )
            ),
        ),
        None => report.fail("resources", "The requested size is not supported."),
    }

    report.finish("")
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct TokenError {
    #[serde(default)]
    error:       Option<String>,
    #[serde(default)]
    error_codes: Vec<u64>,
}

/// A client-credentials token for `scope`, or a message that says what to
/// fix. The message never includes the secret or Entra's own description,
/// which can echo request details.
async fn token(
    http: &HttpClient,
    endpoints: &AzureEndpoints,
    settings: &AzureConnectionSettings,
    secret: &ClientSecret,
    scope: &str,
) -> Result<String, String> {
    let url = format!(
        "{}/{}/oauth2/v2.0/token",
        endpoints.authority.trim_end_matches('/'),
        segment(&settings.tenant_id)
    );
    let form = [
        ("grant_type", "client_credentials"),
        ("client_id", settings.client_id.as_str()),
        ("client_secret", secret.expose()),
        ("scope", scope),
    ];
    let response = http
        .post(&url)
        .form(&form)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| "Could not reach Microsoft Entra ID. Check the server's network access to login.microsoftonline.com.".to_owned())?;
    let status = response.status();
    let body = response.bytes().await.unwrap_or_default();
    if status.is_success() {
        return serde_json::from_slice::<TokenResponse>(&body)
            .map(|parsed| parsed.access_token)
            .map_err(|_| {
                "Microsoft Entra ID returned a token response Fabro did not understand.".to_owned()
            });
    }
    let parsed = serde_json::from_slice::<TokenError>(&body).ok();
    Err(token_failure_message(parsed.as_ref()))
}

fn token_failure_message(error: Option<&TokenError>) -> String {
    let codes = error
        .map(|error| error.error_codes.as_slice())
        .unwrap_or_default();
    let has = |code: u64| codes.contains(&code);
    if has(7_000_222) {
        "The client secret has expired. Create a new secret on the app registration (Certificates & secrets) and replace it here.".to_owned()
    } else if has(7_000_215) {
        "The client secret is not valid for this app registration. Copy the secret's Value, not its Secret ID, and replace it here.".to_owned()
    } else if has(700_016) {
        "No app registration with this Client ID exists in the tenant. Check the Client ID and Tenant ID.".to_owned()
    } else if has(90_002) || has(900_023) {
        "The tenant was not found. Check the Tenant ID (Directory ID) on the app registration's overview.".to_owned()
    } else {
        let code = error
            .and_then(|error| error.error.as_deref())
            .unwrap_or("unknown_error");
        format!(
            "Microsoft Entra ID rejected the credentials ({code}). Check the Tenant ID, Client ID and client secret."
        )
    }
}

fn group_path(settings: &AzureConnectionSettings) -> String {
    format!(
        "/subscriptions/{}/resourceGroups/{}",
        segment(&settings.subscription_id),
        segment(&settings.resource_group)
    )
}

fn segment(value: &str) -> String {
    percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC)
        .to_string()
        .replace("%2D", "-")
        .replace("%5F", "_")
        .replace("%2E", ".")
}

#[derive(Deserialize)]
struct ArmSandboxGroup {
    #[serde(default)]
    location:   Option<String>,
    #[serde(default)]
    properties: ArmSandboxGroupProperties,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArmSandboxGroupProperties {
    #[serde(default)]
    provisioning_state: Option<String>,
}

async fn sandbox_group(
    http: &HttpClient,
    endpoints: &AzureEndpoints,
    settings: &AzureConnectionSettings,
    token: &str,
) -> Result<(), String> {
    let url = format!(
        "{}{}/providers/Microsoft.App/sandboxGroups/{}?api-version={API_VERSION}",
        endpoints.arm.trim_end_matches('/'),
        group_path(settings),
        segment(&settings.sandbox_group)
    );
    let response = http
        .get(&url)
        .bearer_auth(token)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await
        .map_err(|_| "Could not reach Azure Resource Manager. Check the server's network access to management.azure.com.".to_owned())?;
    let status = response.status().as_u16();
    let body = response.bytes().await.unwrap_or_default();
    match status {
        200..=299 => {}
        401 => {
            return Err("Azure Resource Manager rejected the sign-in. Replace the client secret and test again.".to_owned());
        }
        403 => {
            return Err(format!(
                "The app registration cannot read sandbox group {}. Assign it the Reader role on the sandbox group or its resource group.",
                settings.sandbox_group
            ));
        }
        404 => {
            return Err(format!(
                "Sandbox group {} was not found in resource group {} of subscription {}. Create it first, or check the names.",
                settings.sandbox_group, settings.resource_group, settings.subscription_id
            ));
        }
        status => {
            return Err(format!(
                "Azure Resource Manager answered HTTP {status} when reading the sandbox group. Try again later."
            ));
        }
    }
    let group: ArmSandboxGroup = serde_json::from_slice(&body).map_err(|_| {
        "Azure Resource Manager returned a sandbox group Fabro did not understand.".to_owned()
    })?;
    if let Some(state) = group.properties.provisioning_state.as_deref() {
        if !state.eq_ignore_ascii_case("succeeded") {
            return Err(format!(
                "Sandbox group {} is {state}, not provisioned. Wait for it to finish provisioning.",
                settings.sandbox_group
            ));
        }
    }
    if let Some(location) = group.location.as_deref() {
        let location = normalize_location(location);
        if location != settings.location {
            return Err(format!(
                "Sandbox group {} is in {location}, not {}. Set Location to {location}.",
                settings.sandbox_group, settings.location
            ));
        }
    }
    Ok(())
}

/// A disk image as the data plane lists it, read leniently.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct DiskImage {
    #[serde(default)]
    id:     Option<String>,
    #[serde(default)]
    name:   Option<String>,
    #[serde(default)]
    labels: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    status: Option<DiskImageStatus>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct DiskImageStatus {
    #[serde(default)]
    state:   Option<String>,
    #[serde(default)]
    message: Option<String>,
}

impl DiskImage {
    fn is_named(&self, image: &str) -> bool {
        [
            self.name.as_deref(),
            self.labels.get("name").map(String::as_str),
            self.id.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|name| name == image)
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum ImagePage {
    Wrapped {
        #[serde(default)]
        value: Vec<DiskImage>,
    },
    Bare(Vec<DiskImage>),
}

/// The group's disk images, private and public. Proves the data-plane
/// role.
async fn data_plane_images(
    http: &HttpClient,
    endpoints: &AzureEndpoints,
    settings: &AzureConnectionSettings,
    secret: &ClientSecret,
) -> Result<Vec<DiskImage>, String> {
    let token = token(http, endpoints, settings, secret, DATA_PLANE_SCOPE).await?;
    let base = format!(
        "{}{}/sandboxGroups/{}",
        endpoints.data_plane_for(&settings.location),
        group_path(settings),
        segment(&settings.sandbox_group)
    );
    let mut images = Vec::new();
    for suffix in ["/diskimages", "/diskimages/public"] {
        let url = format!("{base}{suffix}?api-version={API_VERSION}");
        let response = http
            .get(&url)
            .bearer_auth(&token)
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|_| format!(
                "Could not reach the sandbox service in {}. Check that ACA sandboxes are available in this location.",
                settings.location
            ))?;
        let status = response.status().as_u16();
        let body = response.bytes().await.unwrap_or_default();
        match status {
            200..=299 => {}
            401 | 403 => {
                return Err(format!(
                    "The app registration cannot use sandbox group {}. Assign it the {DATA_ROLE} role on the sandbox group.",
                    settings.sandbox_group
                ));
            }
            status => {
                return Err(format!(
                    "The sandbox service answered HTTP {status} when listing disk images. Try again later."
                ));
            }
        }
        let page: ImagePage = serde_json::from_slice(&body).map_err(|_| {
            "The sandbox service returned a disk image list Fabro did not understand.".to_owned()
        })?;
        images.extend(match page {
            ImagePage::Wrapped { value } | ImagePage::Bare(value) => value,
        });
    }
    Ok(images)
}

fn image_readiness(image: &str, images: &[DiskImage]) -> Result<String, String> {
    let Some(found) = images.iter().find(|candidate| candidate.is_named(image)) else {
        if image == BUILT_IN_IMAGE {
            return Ok(format!("{BUILT_IN_IMAGE} is the built-in image."));
        }
        return Err(format!(
            "Image {image} was not found in the sandbox group. Import it as a disk image first."
        ));
    };
    let state = found
        .status
        .as_ref()
        .and_then(|status| status.state.as_deref())
        .map(str::to_ascii_lowercase);
    match state.as_deref() {
        None | Some("succeeded" | "ready" | "available" | "completed") => {
            Ok(format!("Image {image} is ready."))
        }
        Some(state @ ("failed" | "error")) => {
            let detail = found
                .status
                .as_ref()
                .and_then(|status| status.message.as_deref())
                .map(|message| format!(" {message}"))
                .unwrap_or_default();
            Err(format!(
                "Image {image} is {state}.{detail} Import it again."
            ))
        }
        Some(state) => Err(format!(
            "Image {image} is {state}, not ready yet. Wait for the import to finish and test again."
        )),
    }
}

#[cfg(test)]
mod tests {
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;

    use super::*;

    const SECRET: &str = "very-secret-value";

    fn settings() -> AzureConnectionSettings {
        AzureConnectionSettings {
            tenant_id:       "contoso.onmicrosoft.com".to_owned(),
            client_id:       "11111111-2222-3333-4444-555555555555".to_owned(),
            subscription_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
            location:        "westus2".to_owned(),
            resource_group:  "fabro-rg".to_owned(),
            sandbox_group:   "fabro-sandboxes".to_owned(),
            execution:       AcaExecutionSettings::default(),
        }
    }

    fn secret() -> ClientSecret {
        ClientSecret(SECRET.to_owned())
    }

    fn endpoints(server: &MockServer) -> AzureEndpoints {
        AzureEndpoints {
            authority:  server.base_url(),
            arm:        server.base_url(),
            data_plane: Some(format!("{}/data", server.base_url())),
        }
    }

    fn http() -> HttpClient {
        fabro_http::test_http_client().unwrap()
    }

    fn mock_token(server: &MockServer) {
        server.mock(|when, then| {
            when.method(POST)
                .path("/contoso.onmicrosoft.com/oauth2/v2.0/token");
            then.status(200)
                .json_body(serde_json::json!({ "access_token": "token", "expires_in": 3600 }));
        });
    }

    fn mock_group(server: &MockServer, status: u16, body: serde_json::Value) {
        server.mock(|when, then| {
            when.method(GET).path(
                "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/fabro-rg/providers/Microsoft.App/sandboxGroups/fabro-sandboxes",
            );
            then.status(status).json_body(body);
        });
    }

    fn mock_images(server: &MockServer, private: serde_json::Value) {
        let base = "/data/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/fabro-rg/sandboxGroups/fabro-sandboxes";
        server.mock(|when, then| {
            when.method(GET).path(format!("{base}/diskimages"));
            then.status(200).json_body(private);
        });
        server.mock(|when, then| {
            when.method(GET).path(format!("{base}/diskimages/public"));
            then.status(200)
                .json_body(serde_json::json!({ "value": [{ "name": "ubuntu" }] }));
        });
    }

    fn status_of(report: &ReadinessReport, id: &str) -> CheckStatus {
        report
            .checks
            .iter()
            .find(|check| check.id == id)
            .unwrap()
            .status
    }

    fn ready_group() -> serde_json::Value {
        serde_json::json!({
            "location": "West US 2",
            "properties": { "provisioningState": "Succeeded" }
        })
    }

    #[tokio::test]
    async fn a_reachable_group_with_a_ready_image_is_ready() {
        let server = MockServer::start();
        mock_token(&server);
        mock_group(&server, 200, ready_group());
        let mut settings = settings();
        settings.execution.image = "fabro-dev".to_owned();
        mock_images(
            &server,
            serde_json::json!({ "value": [{ "name": "fabro-dev", "status": { "state": "Succeeded" } }] }),
        );

        let report =
            check_readiness(&http(), &endpoints(&server), &settings, Some(&secret())).await;

        assert!(report.ready, "{report:?}");
        assert_eq!(report.checks.len(), CHECKS.len());
    }

    #[tokio::test]
    async fn an_expired_secret_says_so_without_echoing_it() {
        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(POST)
                .path("/contoso.onmicrosoft.com/oauth2/v2.0/token");
            then.status(401).json_body(serde_json::json!({
                "error": "invalid_client",
                "error_description": format!("AADSTS7000222: secret {SECRET} expired"),
                "error_codes": [7_000_222]
            }));
        });

        let report =
            check_readiness(&http(), &endpoints(&server), &settings(), Some(&secret())).await;

        assert!(!report.ready);
        assert_eq!(status_of(&report, "credentials"), CheckStatus::Failed);
        assert_eq!(status_of(&report, "sandbox_group"), CheckStatus::Skipped);
        let text = serde_json::to_string(&report).unwrap();
        assert!(text.contains("expired"), "{text}");
        assert!(!text.contains(SECRET), "{text}");
    }

    #[tokio::test]
    async fn a_missing_reader_role_names_the_role() {
        let server = MockServer::start();
        mock_token(&server);
        mock_group(
            &server,
            403,
            serde_json::json!({ "error": { "code": "AuthorizationFailed" } }),
        );

        let report =
            check_readiness(&http(), &endpoints(&server), &settings(), Some(&secret())).await;

        assert_eq!(status_of(&report, "sandbox_group"), CheckStatus::Failed);
        let message = &report.checks[2].message;
        assert!(message.contains("Reader"), "{message}");
        assert_eq!(status_of(&report, "image"), CheckStatus::Skipped);
    }

    #[tokio::test]
    async fn a_group_in_another_location_is_not_ready() {
        let server = MockServer::start();
        mock_token(&server);
        mock_group(
            &server,
            200,
            serde_json::json!({ "location": "eastus2", "properties": { "provisioningState": "Succeeded" } }),
        );

        let report =
            check_readiness(&http(), &endpoints(&server), &settings(), Some(&secret())).await;

        assert_eq!(status_of(&report, "sandbox_group"), CheckStatus::Failed);
        assert!(report.checks[2].message.contains("eastus2"));
    }

    #[tokio::test]
    async fn a_missing_data_role_names_the_role() {
        let server = MockServer::start();
        mock_token(&server);
        mock_group(&server, 200, ready_group());
        server.mock(|when, then| {
            when.method(GET).path_includes("/diskimages");
            then.status(403);
        });

        let report =
            check_readiness(&http(), &endpoints(&server), &settings(), Some(&secret())).await;

        assert_eq!(status_of(&report, "data_plane"), CheckStatus::Failed);
        assert!(report.checks[3].message.contains(DATA_ROLE));
    }

    #[tokio::test]
    async fn an_image_still_importing_is_not_ready() {
        let server = MockServer::start();
        mock_token(&server);
        mock_group(&server, 200, ready_group());
        mock_images(
            &server,
            serde_json::json!([{ "name": "fabro-dev", "status": { "state": "Importing" } }]),
        );
        let mut settings = settings();
        settings.execution.image = "fabro-dev".to_owned();

        let report =
            check_readiness(&http(), &endpoints(&server), &settings, Some(&secret())).await;

        assert!(!report.ready);
        assert_eq!(status_of(&report, "image"), CheckStatus::Failed);
        assert_eq!(status_of(&report, "resources"), CheckStatus::Passed);
    }

    #[tokio::test]
    async fn missing_details_are_reported_without_any_request() {
        let mut settings = settings();
        settings.sandbox_group.clear();
        settings.execution.cpu_millicores = 3000;
        // No server: a request would fail the credentials check instead.
        let endpoints = AzureEndpoints {
            authority:  "http://127.0.0.1:9".to_owned(),
            arm:        "http://127.0.0.1:9".to_owned(),
            data_plane: None,
        };

        let report = check_readiness(&http(), &endpoints, &settings, None).await;

        assert_eq!(status_of(&report, "settings"), CheckStatus::Failed);
        assert_eq!(status_of(&report, "credentials"), CheckStatus::Skipped);
        let message = &report.checks[0].message;
        assert!(message.contains("Client secret is required"), "{message}");
        assert!(message.contains("Sandbox group is required"), "{message}");
        assert!(
            message.contains("not a supported sandbox size"),
            "{message}"
        );
    }

    #[test]
    fn an_image_outside_the_allowed_list_is_a_problem() {
        let mut settings = settings();
        settings.execution.allowed_images = vec!["fabro-dev".to_owned()];
        assert!(
            settings
                .problems()
                .iter()
                .any(|p| p.contains("not in the list of allowed images"))
        );
        settings.execution.image = "fabro-dev".to_owned();
        assert!(settings.problems().is_empty(), "{:?}", settings.problems());
    }

    #[test]
    fn normalizing_trims_and_lowercases_the_location() {
        let mut settings = settings();
        settings.location = " West US 2 ".to_owned();
        settings.execution.allowed_images = vec![" a ".to_owned(), "  ".to_owned()];
        let settings = settings.normalized();
        assert_eq!(settings.location, "westus2");
        assert_eq!(settings.execution.allowed_images, vec!["a".to_owned()]);
    }

    #[tokio::test]
    async fn the_connection_survives_a_restart_and_keeps_its_secret_on_edit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fabro.db");
        {
            let db = fabro_db::Database::connect(&path).await.unwrap();
            db.migrate().await.unwrap();
            assert!(load(db.pool()).await.unwrap().is_none());
            assert!(matches!(
                save(db.pool(), &settings(), None).await,
                Err(AzureStoreError::SecretRequired)
            ));
            save(db.pool(), &settings(), Some(secret())).await.unwrap();
        }

        let db = fabro_db::Database::connect(&path).await.unwrap();
        db.migrate().await.unwrap();
        let stored = load(db.pool())
            .await
            .unwrap()
            .expect("saved before the restart");
        assert_eq!(stored.settings, settings());
        assert_eq!(stored.client_secret, secret());

        let mut changed = settings();
        changed.sandbox_group = "other-group".to_owned();
        save(db.pool(), &changed, None).await.unwrap();
        let stored = load(db.pool()).await.unwrap().unwrap();
        assert_eq!(stored.settings.sandbox_group, "other-group");
        assert_eq!(
            stored.client_secret,
            secret(),
            "a blank secret keeps the saved one"
        );

        save(
            db.pool(),
            &changed,
            ClientSecret::from_input(Some("replaced".to_owned())),
        )
        .await
        .unwrap();
        let stored = load(db.pool()).await.unwrap().unwrap();
        assert_eq!(stored.client_secret, ClientSecret("replaced".to_owned()));

        assert!(delete(db.pool()).await.unwrap());
        assert!(load(db.pool()).await.unwrap().is_none());
        assert!(!delete(db.pool()).await.unwrap());
    }

    #[test]
    fn the_secret_never_prints() {
        let text = format!("{:?}", StoredAzureConnection {
            settings:          settings(),
            client_secret:     secret(),
            secret_updated_at: Utc::now(),
            updated_at:        Utc::now(),
        });
        assert!(!text.contains(SECRET), "{text}");
    }
}
