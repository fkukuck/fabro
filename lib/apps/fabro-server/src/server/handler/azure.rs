use std::sync::Arc;

use axum::routing::post;
use serde::Deserialize;

use super::super::{
    ApiError, AppState, IntoResponse, Json, RequiredUser, Response, Router, State, StatusCode, get,
};
use crate::azure::{
    self, AzureConnectionSettings, AzureConnectionView, AzureEndpoints, AzureStoreError,
    ClientSecret,
};

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/integrations/azure",
            get(get_connection)
                .put(save_connection)
                .delete(delete_connection),
        )
        .route("/integrations/azure/test", post(test_connection))
}

/// The form as submitted: the settings, and a client secret only when the
/// person typed one.
#[derive(Deserialize)]
struct AzureConnectionInput {
    #[serde(flatten)]
    settings:      AzureConnectionSettings,
    #[serde(default)]
    client_secret: Option<String>,
}

async fn get_connection(_auth: RequiredUser, State(state): State<Arc<AppState>>) -> Response {
    match azure::load(&state.db_pool).await {
        Ok(stored) => (
            StatusCode::OK,
            Json(AzureConnectionView::of(stored.as_ref())),
        )
            .into_response(),
        Err(err) => store_error(&err),
    }
}

async fn save_connection(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AzureConnectionInput>,
) -> Response {
    let settings = body.settings.normalized();
    let problems = settings.problems();
    if !problems.is_empty() {
        return ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, problems.join(" ")).into_response();
    }
    match azure::save(
        &state.db_pool,
        &settings,
        ClientSecret::from_input(body.client_secret),
    )
    .await
    {
        Ok(stored) => {
            tracing::info!(
                subscription_id = %settings.subscription_id,
                sandbox_group = %settings.sandbox_group,
                "Azure connection saved"
            );
            (StatusCode::OK, Json(AzureConnectionView::of(Some(&stored)))).into_response()
        }
        Err(AzureStoreError::SecretRequired) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Client secret is required to save a new Azure connection.",
        )
        .into_response(),
        Err(err) => store_error(&err),
    }
}

async fn delete_connection(_auth: RequiredUser, State(state): State<Arc<AppState>>) -> Response {
    match azure::delete(&state.db_pool).await {
        Ok(true) => {
            tracing::info!("Azure connection removed");
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => ApiError::not_found("no Azure connection is saved").into_response(),
        Err(err) => store_error(&err),
    }
}

/// Tests the submitted settings, falling back to the saved client secret
/// when none was typed. Nothing is saved and no sandbox is created.
async fn test_connection(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AzureConnectionInput>,
) -> Response {
    let settings = body.settings.normalized();
    let secret = match ClientSecret::from_input(body.client_secret) {
        Some(secret) => Some(secret),
        None => match azure::load(&state.db_pool).await {
            Ok(stored) => stored.map(|stored| stored.client_secret),
            Err(err) => return store_error(&err),
        },
    };
    let http = match state.http_client() {
        Ok(http) => http,
        Err(err) => {
            tracing::error!(error = %err, "Building the HTTP client for the Azure check failed");
            return ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "could not build an HTTP client",
            )
            .into_response();
        }
    };
    let report = azure::check_readiness(
        &http,
        &AzureEndpoints::default(),
        &settings,
        secret.as_ref(),
    )
    .await;
    tracing::info!(ready = report.ready, "Azure connection tested");
    (StatusCode::OK, Json(report)).into_response()
}

fn store_error(err: &AzureStoreError) -> Response {
    tracing::error!(error = %err, "Azure connection store operation failed");
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Azure connection store operation failed",
    )
    .into_response()
}
