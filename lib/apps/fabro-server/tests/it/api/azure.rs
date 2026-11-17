use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use tower::ServiceExt;

use crate::helpers::{api, response_json, response_status, test_app_state};

const SECRET: &str = "never-echo-this-secret";

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

fn connection(secret: Option<&str>) -> serde_json::Value {
    let mut body = serde_json::json!({
        "tenant_id": "contoso.onmicrosoft.com",
        "client_id": "11111111-2222-3333-4444-555555555555",
        "subscription_id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        "location": "West US 2",
        "resource_group": "fabro-rg",
        "sandbox_group": "fabro-sandboxes",
        "execution": {
            "enabled": true,
            "image": "fabro-dev",
            "allowed_images": ["fabro-dev"],
            "cpu_millicores": 2000,
            "memory_mib": 4096
        }
    });
    if let Some(secret) = secret {
        body["client_secret"] = serde_json::json!(secret);
    }
    body
}

#[tokio::test]
async fn azure_connection_is_saved_edited_and_disconnected_without_exposing_the_secret() {
    let app = fabro_server::test_support::build_test_router(test_app_state());

    let empty = app
        .clone()
        .oneshot(empty_request(Method::GET, "/integrations/azure"))
        .await
        .expect("request should route");
    let body = response_json(empty, StatusCode::OK, "GET azure").await;
    assert_eq!(body["configured"], false);
    assert_eq!(body["client_secret_set"], false);

    let no_secret = app
        .clone()
        .oneshot(json_request(
            Method::PUT,
            "/integrations/azure",
            &connection(None),
        ))
        .await
        .expect("request should route");
    response_status(
        no_secret,
        StatusCode::UNPROCESSABLE_ENTITY,
        "PUT azure without secret",
    )
    .await;

    let saved = app
        .clone()
        .oneshot(json_request(
            Method::PUT,
            "/integrations/azure",
            &connection(Some(SECRET)),
        ))
        .await
        .expect("request should route");
    let body = response_json(saved, StatusCode::OK, "PUT azure").await;
    assert_eq!(body["configured"], true);
    assert_eq!(body["client_secret_set"], true);
    assert_eq!(body["settings"]["location"], "westus2");
    assert!(!body.to_string().contains(SECRET));

    let mut edited = connection(None);
    edited["execution"]["enabled"] = serde_json::json!(false);
    let edited = app
        .clone()
        .oneshot(json_request(Method::PUT, "/integrations/azure", &edited))
        .await
        .expect("request should route");
    let body = response_json(edited, StatusCode::OK, "PUT azure keeping secret").await;
    assert_eq!(body["settings"]["execution"]["enabled"], false);
    assert_eq!(body["client_secret_set"], true);

    let read = app
        .clone()
        .oneshot(empty_request(Method::GET, "/integrations/azure"))
        .await
        .expect("request should route");
    let body = response_json(read, StatusCode::OK, "GET azure").await;
    assert_eq!(body["settings"]["sandbox_group"], "fabro-sandboxes");
    assert!(!body.to_string().contains(SECRET));

    let deleted = app
        .clone()
        .oneshot(empty_request(Method::DELETE, "/integrations/azure"))
        .await
        .expect("request should route");
    response_status(deleted, StatusCode::NO_CONTENT, "DELETE azure").await;

    let read = app
        .clone()
        .oneshot(empty_request(Method::GET, "/integrations/azure"))
        .await
        .expect("request should route");
    let body = response_json(read, StatusCode::OK, "GET azure").await;
    assert_eq!(body["configured"], false);
}

#[tokio::test]
async fn azure_connection_rejects_an_image_outside_the_allowed_list() {
    let app = fabro_server::test_support::build_test_router(test_app_state());
    let mut body = connection(Some(SECRET));
    body["execution"]["image"] = serde_json::json!("other");

    let response = app
        .oneshot(json_request(Method::PUT, "/integrations/azure", &body))
        .await
        .expect("request should route");
    let body = response_json(response, StatusCode::UNPROCESSABLE_ENTITY, "PUT azure").await;
    assert!(body.to_string().contains("allowed images"), "{body}");
}

#[tokio::test]
async fn azure_test_reports_missing_details_without_calling_azure() {
    let app = fabro_server::test_support::build_test_router(test_app_state());
    let mut body = connection(None);
    body["sandbox_group"] = serde_json::json!("");

    let response = app
        .oneshot(json_request(
            Method::POST,
            "/integrations/azure/test",
            &body,
        ))
        .await
        .expect("request should route");
    let body = response_json(response, StatusCode::OK, "POST azure test").await;
    assert_eq!(body["ready"], false);
    assert_eq!(body["checks"][0]["id"], "settings");
    assert_eq!(body["checks"][0]["status"], "failed");
    assert_eq!(body["checks"][1]["status"], "skipped");
}
