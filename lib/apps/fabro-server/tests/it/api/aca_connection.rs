use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

use crate::helpers::{api, response_json, response_status, test_app_state};

fn json_request(method: Method, path: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(api(path))
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(body).expect("request body should serialize"),
        ))
        .expect("request should build")
}

fn empty_request(method: Method, path: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(api(path))
        .body(Body::empty())
        .expect("request should build")
}

const CONNECTION: &str = "/sandbox-providers/aca/connection";

#[tokio::test]
async fn an_unconfigured_server_reports_no_connection() {
    let app = fabro_server::test_support::build_test_router(test_app_state());
    let response = app
        .clone()
        .oneshot(empty_request(Method::GET, CONNECTION))
        .await
        .expect("GET should route");
    let body = response_json(response, StatusCode::OK, "GET connection").await;
    assert_eq!(body["connected"], false);
    assert_eq!(body["client_secret_saved"], false);
    assert_eq!(body["retention_hours"], 24);
    assert_eq!(body["enabled"], true);

    let tested = app
        .oneshot(empty_request(Method::POST, &format!("{CONNECTION}/test")))
        .await
        .expect("POST test should route");
    let body = response_json(tested, StatusCode::OK, "POST test").await;
    assert_eq!(body["ready"], false);
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("no Azure connection is saved"),
        "{body}"
    );
}

#[tokio::test]
async fn an_incomplete_connection_is_refused_with_the_missing_fields() {
    let app = fabro_server::test_support::build_test_router(test_app_state());
    let response = app
        .clone()
        .oneshot(json_request(
            Method::PUT,
            CONNECTION,
            &serde_json::json!({
                "tenant_id": "tenant",
                "client_id": "client",
                "client_secret": "a-secret-value",
                "subscription_id": "sub",
            }),
        ))
        .await
        .expect("PUT should route");
    let body = response_json(response, StatusCode::UNPROCESSABLE_ENTITY, "PUT connection").await;
    let message = body.to_string();
    assert!(message.contains("resource_group"), "{message}");
    assert!(message.contains("sandbox_group"), "{message}");
    assert!(!message.contains("a-secret-value"), "{message}");

    let response = app
        .oneshot(json_request(
            Method::PUT,
            CONNECTION,
            &serde_json::json!({
                "tenant_id": "tenant",
                "client_id": "client",
                "client_secret": "a-secret-value",
                "subscription_id": "sub",
                "resource_group": "rg",
                "sandbox_group": "group",
                "retention_hours": 72,
            }),
        ))
        .await
        .expect("PUT should route");
    response_status(
        response,
        StatusCode::UNPROCESSABLE_ENTITY,
        "PUT connection with a long retention",
    )
    .await;
}

#[tokio::test]
async fn the_connection_secret_cannot_be_written_through_the_secrets_api() {
    let app = fabro_server::test_support::build_test_router(test_app_state());
    let response = app
        .oneshot(json_request(
            Method::POST,
            "/secrets",
            &serde_json::json!({
                "name": "ACA_CONNECTION",
                "value": "{}",
                "type": "file",
            }),
        ))
        .await
        .expect("POST /secrets should route");
    let body = response_json(response, StatusCode::BAD_REQUEST, "POST /secrets").await;
    assert!(body.to_string().contains("Settings"), "{body}");
}

#[tokio::test]
async fn disconnecting_is_harmless_to_repeat() {
    let app = fabro_server::test_support::build_test_router(test_app_state());
    for _ in 0..2 {
        let response = app
            .clone()
            .oneshot(empty_request(Method::DELETE, CONNECTION))
            .await
            .expect("DELETE should route");
        response_status(response, StatusCode::NO_CONTENT, "DELETE connection").await;
    }
}
