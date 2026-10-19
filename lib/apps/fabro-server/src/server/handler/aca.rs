//! The Azure Container Apps Sandboxes connection, managed from Settings:
//! view it redacted, test a connection without saving it, save it after a
//! passing test, and disconnect.
//!
//! The connection is one vault secret, [`EnvVars::ACA_CONNECTION`], stored
//! as a file-type secret so it never joins a run's secret provider and
//! never reaches an agent's workspace. The generic `/secrets` API refuses
//! the name; this module is the only writer. Runs and the sandbox views
//! read the vault when they need the connection, so a save or a disconnect
//! takes effect without a restart.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use fabro_sandbox_aca::{AcaConnection, MAX_RETENTION_SECS};
use fabro_static::EnvVars;
use fabro_types::SandboxProviderKind;
use fabro_vault::SecretType;
use serde::Serialize;
use tracing::{info, warn};

use super::super::AppState;
use crate::aca_connection::{ConnectionInput, TestResult, run_test};
use crate::error::ApiError;
use crate::principal_middleware::RequiredUser;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/sandbox-providers/aca/connection",
            get(retrieve_connection)
                .put(save_connection)
                .delete(delete_connection),
        )
        .route(
            "/sandbox-providers/aca/connection/test",
            post(test_connection),
        )
}

/// The connection without its secret; see `AcaConnectionSummary`.
#[derive(Debug, Serialize)]
struct ConnectionSummary {
    connected:           bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_id:           Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_id:           Option<String>,
    client_secret_saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    subscription_id:     Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_group:      Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox_group:       Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    location:            Option<String>,
    allowed_images:      Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    default_image:       Option<String>,
    retention_hours:     u64,
    enabled:             bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_at:          Option<DateTime<Utc>>,
}

impl ConnectionSummary {
    fn of(
        connection: Option<&AcaConnection>,
        enabled: bool,
        updated_at: Option<DateTime<Utc>>,
    ) -> Self {
        let Some(connection) = connection else {
            return Self {
                connected: false,
                tenant_id: None,
                client_id: None,
                client_secret_saved: false,
                subscription_id: None,
                resource_group: None,
                sandbox_group: None,
                location: None,
                allowed_images: Vec::new(),
                default_image: None,
                retention_hours: MAX_RETENTION_SECS / 3600,
                enabled,
                updated_at: None,
            };
        };
        Self {
            connected: true,
            tenant_id: Some(connection.credential.tenant_id.clone()),
            client_id: Some(connection.credential.client_id.clone()),
            client_secret_saved: !connection.credential.client_secret.is_empty(),
            subscription_id: Some(connection.target.subscription_id.clone()),
            resource_group: Some(connection.target.resource_group.clone()),
            sandbox_group: Some(connection.target.sandbox_group.clone()),
            location: connection.target.location.clone(),
            allowed_images: connection.policy.allowed_images.clone(),
            default_image: connection.policy.default_image.clone(),
            retention_hours: connection.policy.effective_retention_secs().div_ceil(3600),
            enabled,
            updated_at,
        }
    }
}

async fn saved(state: &AppState) -> Result<Option<(AcaConnection, DateTime<Utc>)>, Response> {
    let entry = state
        .stores
        .vault
        .get(EnvVars::ACA_CONNECTION)
        .await
        .map_err(|err| {
            warn!(error = %err, "reading the ACA connection failed");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the saved Azure connection could not be read",
            )
            .into_response()
        })?;
    let Some(entry) = entry else {
        return Ok(None);
    };
    match AcaConnection::from_json(&entry.value) {
        Ok(connection) => Ok(Some((connection, entry.updated_at))),
        Err(err) => {
            warn!(error = %err, "the saved ACA connection is not readable");
            Ok(None)
        }
    }
}

fn enabled(state: &AppState) -> bool {
    state
        .server_settings()
        .server
        .sandbox
        .providers
        .is_enabled(&SandboxProviderKind::ACA)
}

async fn retrieve_connection(_auth: RequiredUser, State(state): State<Arc<AppState>>) -> Response {
    match saved(&state).await {
        Ok(saved) => Json(ConnectionSummary::of(
            saved.as_ref().map(|(connection, _)| connection),
            enabled(&state),
            saved.as_ref().map(|(_, updated)| *updated),
        ))
        .into_response(),
        Err(response) => response,
    }
}

async fn test_connection(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    body: Option<Json<ConnectionInput>>,
) -> Response {
    let input = body.map(|Json(input)| input).unwrap_or_default();
    let saved = match saved(&state).await {
        Ok(saved) => saved.map(|(connection, _)| connection),
        Err(response) => return response,
    };
    let connection = if input.is_empty() {
        match saved {
            Some(saved) => saved,
            None => {
                return Json(TestResult::failed(
                    "no Azure connection is saved; enter one and test it",
                ))
                .into_response();
            }
        }
    } else {
        match input.resolve(saved.as_ref()) {
            Ok(connection) => connection,
            Err(message) => {
                return ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, message).into_response();
            }
        }
    };
    Json(run_test(&connection, state.http_client().ok()).await).into_response()
}

async fn save_connection(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Json(input): Json<ConnectionInput>,
) -> Response {
    let saved = match saved(&state).await {
        Ok(saved) => saved.map(|(connection, _)| connection),
        Err(response) => return response,
    };
    let connection = match input.resolve(saved.as_ref()) {
        Ok(connection) => connection,
        Err(message) => {
            return ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, message).into_response();
        }
    };
    let result = run_test(&connection, state.http_client().ok()).await;
    if !result.ready {
        return ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "the Azure connection was not saved: {}",
                result.message.unwrap_or_else(|| result.problems.join("; "))
            ),
        )
        .into_response();
    }
    let json = match connection.to_json() {
        Ok(json) => json,
        Err(err) => {
            return ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
                .into_response();
        }
    };
    match state
        .stores
        .vault
        .set(
            EnvVars::ACA_CONNECTION,
            &json,
            SecretType::File,
            Some("Azure Container Apps Sandboxes connection, managed in Settings"),
        )
        .await
    {
        Ok(metadata) => {
            info!(
                sandbox_group = %connection.target.sandbox_group,
                "ACA connection saved"
            );
            Json(ConnectionSummary::of(
                Some(&connection),
                enabled(&state),
                Some(metadata.updated_at),
            ))
            .into_response()
        }
        Err(err) => {
            warn!(error = %err, "saving the ACA connection failed");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the Azure connection could not be saved",
            )
            .into_response()
        }
    }
}

async fn delete_connection(_auth: RequiredUser, State(state): State<Arc<AppState>>) -> Response {
    match state.stores.vault.remove(EnvVars::ACA_CONNECTION).await {
        Ok(()) | Err(fabro_vault::SecretStoreError::NotFound(_)) => {
            info!("ACA connection removed");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(err) => {
            warn!(error = %err, "removing the ACA connection failed");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the Azure connection could not be removed",
            )
            .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_never_carries_the_secret() {
        let connection = ConnectionInput {
            tenant_id: Some("tenant".into()),
            client_id: Some("client".into()),
            client_secret: Some("secret".into()),
            subscription_id: Some("sub".into()),
            resource_group: Some("rg".into()),
            sandbox_group: Some("group".into()),
            ..ConnectionInput::default()
        }
        .resolve(None)
        .unwrap();
        let summary = ConnectionSummary::of(Some(&connection), true, None);
        let json = serde_json::to_string(&summary).unwrap();
        assert!(!json.contains("\"secret\""), "{json}");
        assert!(json.contains("\"client_secret_saved\":true"), "{json}");
    }
}
