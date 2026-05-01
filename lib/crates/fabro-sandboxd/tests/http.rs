use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use fabro_sandbox::azure::protocol::{ExecRequest, ReadFileRequest, WriteFileRequest};
use fabro_sandboxd::build_router;
use tower::ServiceExt;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let app = build_router();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn exec_endpoint_runs_command() {
    let app = build_router();
    let body = serde_json::to_vec(&ExecRequest {
        command:     "printf hello".to_string(),
        working_dir: None,
        env:         std::collections::HashMap::new(),
        timeout_ms:  5_000,
    })
    .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn write_then_read_round_trip() {
    let app = build_router();
    let write_body = serde_json::to_vec(&WriteFileRequest {
        path:           "/tmp/sandboxd-round-trip.txt".to_string(),
        content_base64: STANDARD.encode("hello azure"),
    })
    .unwrap();
    let write_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/write-file")
                .header("content-type", "application/json")
                .body(Body::from(write_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(write_response.status(), StatusCode::NO_CONTENT);

    let read_body = serde_json::to_vec(&ReadFileRequest {
        path: "/tmp/sandboxd-round-trip.txt".to_string(),
    })
    .unwrap();
    let read_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/read-file")
                .header("content-type", "application/json")
                .body(Body::from(read_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read_response.status(), StatusCode::OK);
}
