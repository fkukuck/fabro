use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fabro_config::parse_settings_layer;
use fabro_server::ip_allowlist::IpAllowlistConfig;
use fabro_server::jwt_auth::{AuthMode, ConfiguredAuth};
use fabro_server::server::{
    RouterOptions, build_router_with_options, create_app_state_with_options,
};
use fabro_types::settings::ServerAuthMethod;
use tower::ServiceExt;

use crate::helpers::body_json;

fn settings(source: &str) -> fabro_types::settings::SettingsLayer {
    parse_settings_layer(source).expect("fixture should parse")
}

fn build_app(
    settings: fabro_types::settings::SettingsLayer,
    auth_mode: &AuthMode,
    options: RouterOptions,
) -> axum::Router {
    build_router_with_options(
        create_app_state_with_options(settings, 5),
        auth_mode,
        Arc::new(IpAllowlistConfig::default()),
        options,
    )
}

#[tokio::test]
async fn cli_auth_config_reports_enabled_github_login() {
    let app = build_app(
        settings(
            r#"
_version = 1

[server.auth]
methods = ["github"]

[server.auth.github]
allowed_usernames = ["alice"]

[server.web]
url = "https://fabro.example"

[server.integrations.github]
client_id = "Iv1.test"
"#,
        ),
        &AuthMode::Enabled(ConfiguredAuth::new(vec![ServerAuthMethod::Github], None)),
        RouterOptions::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/cli/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response.into_body()).await;
    assert_eq!(
        body,
        serde_json::json!({
            "enabled": true,
            "web_url": "https://fabro.example",
            "methods": ["github"]
        })
    );
}

#[tokio::test]
async fn cli_auth_config_reports_github_not_enabled() {
    let app = build_app(
        settings(
            r#"
_version = 1

[server.auth]
methods = ["dev-token"]
"#,
        ),
        &AuthMode::Enabled(ConfiguredAuth::new(vec![ServerAuthMethod::DevToken], None)),
        RouterOptions::default(),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/cli/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = body_json(response.into_body()).await;
    assert_eq!(
        body,
        serde_json::json!({
            "enabled": false,
            "web_url": null,
            "methods": ["dev-token"],
            "reason": "github_not_enabled"
        })
    );
}

#[tokio::test]
async fn cli_auth_config_reports_web_not_enabled_and_api_mount_survives() {
    let app = build_app(
        settings(
            r#"
_version = 1

[server.auth]
methods = ["dev-token"]

[server.web]
enabled = false
"#,
        ),
        &AuthMode::Enabled(ConfiguredAuth::new(vec![ServerAuthMethod::DevToken], None)),
        RouterOptions {
            web_enabled: false,
            ..RouterOptions::default()
        },
    );

    let config_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/cli/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(config_response.status(), StatusCode::OK);
    let body = body_json(config_response.into_body()).await;
    assert_eq!(
        body,
        serde_json::json!({
            "enabled": false,
            "web_url": null,
            "methods": ["dev-token"],
            "reason": "web_not_enabled"
        })
    );

    let start_response = app
        .oneshot(
            Request::builder()
                .uri("/auth/cli/start")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start_response.status(), StatusCode::NOT_FOUND);
}
