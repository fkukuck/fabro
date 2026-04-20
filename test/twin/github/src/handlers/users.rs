use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::server::SharedState;

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

pub async fn get_user(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let state = state.read().await;
    let Some(subject) = state.oauth_tokens.get(token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    axum::Json(subject.user.clone()).into_response()
}

pub async fn get_emails(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let Some(token) = bearer_token(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let state = state.read().await;
    let Some(subject) = state.oauth_tokens.get(token) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    axum::Json(subject.emails.clone()).into_response()
}

#[cfg(test)]
mod tests {
    use reqwest::redirect::Policy;

    use crate::server::TestServer;
    use crate::state::AppState;
    use crate::test_support::test_http_client;

    async fn authorize_and_exchange(server: &TestServer) -> String {
        let client = fabro_http::HttpClientBuilder::new()
            .redirect(Policy::none())
            .no_proxy()
            .build()
            .unwrap();
        let authorize = client
            .get(format!(
                "{}/login/oauth/authorize?client_id=github-client-id&redirect_uri=http://127.0.0.1/callback&state=test-state",
                server.url()
            ))
            .send()
            .await
            .unwrap();
        let location = authorize
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .unwrap()
            .to_string();
        let code = location
            .split("code=")
            .nth(1)
            .and_then(|value| value.split('&').next())
            .unwrap();

        let token = client
            .post(format!("{}/login/oauth/access_token", server.url()))
            .header("content-type", "application/x-www-form-urlencoded")
            .body(format!(
                "client_id=github-client-id&client_secret=github-client-secret&code={code}"
            ))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = token.json().await.unwrap();
        body["access_token"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn user_endpoints_return_seeded_profile() {
        let state = AppState::new();
        let server = TestServer::start(state).await;
        let client = test_http_client();
        let token = authorize_and_exchange(&server).await;

        let user = client
            .get(format!("{}/user", server.url()))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(user.status(), 200);
        let user_body: serde_json::Value = user.json().await.unwrap();
        assert_eq!(user_body["login"], "octocat");

        let emails = client
            .get(format!("{}/user/emails", server.url()))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap();
        assert_eq!(emails.status(), 200);
        let email_body: serde_json::Value = emails.json().await.unwrap();
        assert_eq!(email_body[0]["email"], "octocat@example.com");

        server.shutdown().await;
    }
}
