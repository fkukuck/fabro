#![expect(
    clippy::disallowed_methods,
    reason = "These blocking CLI scenario tests spawn the real fabro binary and stream child pipes."
)]

use std::io::{BufRead as _, BufReader, Read};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{Request, State as AxumState};
use axum::middleware::{self, Next};
use axum::response::Response as AxumResponse;
use chrono::{Duration as ChronoDuration, Utc};
use fabro_config::{parse_settings_layer, resolve_server_from_file};
use fabro_server::auth::GithubEndpoints;
use fabro_server::ip_allowlist::IpAllowlistConfig;
use fabro_server::jwt_auth::resolve_auth_mode_with_lookup;
use fabro_server::server::{
    RouterOptions, build_router_with_options, create_app_state_with_env_lookup,
};
use fabro_test::{GitHubAppState, apply_test_isolation, test_context};
use fabro_types::RunAuthMethod;
use hkdf::Hkdf;
use httpmock::Method::{GET, POST};
use httpmock::MockServer;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde_json::{Value, json};
use sha2::Sha256;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use ulid::Ulid;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(10);
const TEST_SESSION_SECRET: &str =
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const TEST_DEV_TOKEN: &str =
    "fabro_dev_abababababababababababababababababababababababababababababababab";

#[test]
fn auth_login_refresh_logout_flow() {
    let context = test_context!();
    let server = MockServer::start();
    let target = server_target(&server);

    let config_mock = server.mock(|when, then| {
        when.method(GET).path("/api/v1/auth/cli/config");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "enabled": true,
                "web_url": server.base_url(),
                "methods": ["github"]
            }));
    });
    let token_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/auth/cli/token")
            .header("content-type", "application/json");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(token_response("access-1", "refresh-1", 600, 3_600));
    });

    let login_output = complete_login(&context, &target);
    assert!(
        login_output.status.success(),
        "auth login failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&login_output.stdout),
        String::from_utf8_lossy(&login_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&login_output.stderr).contains("Logged in to"),
        "login output should confirm success:\n{}",
        String::from_utf8_lossy(&login_output.stderr)
    );
    config_mock.assert();
    token_mock.assert();

    let status = auth_status(&context, &target);
    assert_eq!(status["servers"].as_array().map(Vec::len), Some(1));
    assert_eq!(status["servers"][0]["oauth_state"], "active");
    assert_eq!(status["servers"][0]["login"], "octocat");

    let expired_access_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/system/info")
            .header("authorization", "Bearer access-1");
        then.status(401)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "errors": [{
                    "status": "401",
                    "title": "Unauthorized",
                    "detail": "Access token expired.",
                    "code": "access_token_expired"
                }]
            }));
    });
    let refresh_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/auth/cli/refresh")
            .header("authorization", "Bearer refresh-1");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(token_response("access-2", "refresh-2", 600, 3_600));
    });
    let refreshed_info_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/system/info")
            .header("authorization", "Bearer access-2");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "version": "1.2.3",
                "git_sha": "abcdef0",
                "build_date": "2026-04-20",
                "profile": "release",
                "os": "darwin",
                "arch": "arm64",
                "storage_dir": "/tmp/fabro-auth-flow",
                "storage_engine": "slatedb",
                "runs": { "total": 0, "active": 0 },
                "uptime_secs": 42
            }));
    });

    let system_info = context
        .command()
        .args(["--json", "system", "info", "--server", &target])
        .output()
        .expect("system info should run");
    assert!(
        system_info.status.success(),
        "system info failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&system_info.stdout),
        String::from_utf8_lossy(&system_info.stderr)
    );
    let system_info_json: Value =
        serde_json::from_slice(&system_info.stdout).expect("system info JSON should parse");
    assert_eq!(system_info_json["version"], "1.2.3");
    expired_access_mock.assert();
    refresh_mock.assert();
    refreshed_info_mock.assert();

    let logout_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/auth/cli/logout")
            .header("authorization", "Bearer refresh-2");
        then.status(204);
    });

    let logout = context
        .command()
        .args(["auth", "logout", "--server", &target])
        .output()
        .expect("auth logout should run");
    assert!(
        logout.status.success(),
        "auth logout failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&logout.stdout),
        String::from_utf8_lossy(&logout.stderr)
    );
    logout_mock.assert();

    let logged_out_status = auth_status(&context, &target);
    assert_eq!(
        logged_out_status["servers"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn auth_refresh_failure_clears_local_session() {
    let context = test_context!();
    let server = MockServer::start();
    let target = server_target(&server);

    server.mock(|when, then| {
        when.method(GET).path("/api/v1/auth/cli/config");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "enabled": true,
                "web_url": server.base_url(),
                "methods": ["github"]
            }));
    });
    server.mock(|when, then| {
        when.method(POST)
            .path("/auth/cli/token")
            .header("content-type", "application/json");
        then.status(200)
            .header("Content-Type", "application/json")
            .json_body(token_response(
                "access-revoked",
                "refresh-revoked",
                600,
                3_600,
            ));
    });

    let login_output = complete_login(&context, &target);
    assert!(
        login_output.status.success(),
        "auth login failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&login_output.stdout),
        String::from_utf8_lossy(&login_output.stderr)
    );

    let expired_access_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/api/v1/system/info")
            .header("authorization", "Bearer access-revoked");
        then.status(401)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "errors": [{
                    "status": "401",
                    "title": "Unauthorized",
                    "detail": "Access token expired.",
                    "code": "access_token_expired"
                }]
            }));
    });
    let refresh_mock = server.mock(|when, then| {
        when.method(POST)
            .path("/auth/cli/refresh")
            .header("authorization", "Bearer refresh-revoked");
        then.status(401)
            .header("Content-Type", "application/json")
            .json_body(json!({
                "error": "refresh_token_revoked",
                "error_description": "CLI session has expired. Run `fabro auth login` again."
            }));
    });

    let system_info = context
        .command()
        .args(["--json", "system", "info", "--server", &target])
        .output()
        .expect("system info should run");
    assert!(
        !system_info.status.success(),
        "system info should fail when refresh is revoked"
    );
    assert!(
        String::from_utf8_lossy(&system_info.stderr).contains("fabro auth login"),
        "refresh failure should direct the user to log in again:\n{}",
        String::from_utf8_lossy(&system_info.stderr)
    );
    expired_access_mock.assert();
    refresh_mock.assert();

    let status = auth_status(&context, &target);
    assert_eq!(status["servers"].as_array().map(Vec::len), Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_login_refresh_logout_flow_against_real_server_and_twin_github() {
    let context = test_context!();
    let harness = RealAuthHarness::start(GitHubAppState::new()).await;
    let target = harness.api_target();

    let (login_output, browser_url) = complete_login_via_browser(&context, &target).await;
    assert!(
        login_output.status.success(),
        "auth login failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&login_output.stdout),
        String::from_utf8_lossy(&login_output.stderr)
    );
    assert!(
        browser_url.starts_with(&format!("{}/auth/cli/start", harness.web_base_url)),
        "browser flow should open the configured web origin, got: {browser_url}"
    );
    assert_ne!(harness.api_base_url, harness.web_base_url);

    let status = auth_status(&context, &target);
    assert_eq!(status["servers"].as_array().map(Vec::len), Some(1));
    assert_eq!(status["servers"][0]["oauth_state"], "active");
    assert_eq!(status["servers"][0]["login"], "octocat");
    assert_eq!(status["servers"][0]["server"], harness.api_base_url);
    assert!(harness.api_requests.contains("GET /api/v1/auth/cli/config"));
    assert!(harness.web_requests.contains("GET /auth/cli/start"));
    assert!(harness.api_requests.contains("POST /auth/cli/token"));
    assert!(!harness.web_requests.contains("POST /auth/cli/token"));

    expire_saved_access_token(&context, &harness.web_base_url);
    let system_info = context
        .command()
        .args(["--json", "system", "info", "--server", &target])
        .output()
        .expect("system info should run");
    assert!(
        system_info.status.success(),
        "system info failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&system_info.stdout),
        String::from_utf8_lossy(&system_info.stderr)
    );

    let refreshed_entry = saved_auth_entry(&context);
    let refreshed_refresh_token = refreshed_entry["refresh_token"]
        .as_str()
        .expect("saved auth should include refresh token")
        .to_string();

    let logout = context
        .command()
        .args(["auth", "logout", "--server", &target])
        .output()
        .expect("auth logout should run");
    assert!(
        logout.status.success(),
        "auth logout failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&logout.stdout),
        String::from_utf8_lossy(&logout.stderr)
    );

    let logged_out_status = auth_status(&context, &target);
    assert_eq!(
        logged_out_status["servers"].as_array().map(Vec::len),
        Some(0)
    );

    let refresh_response = fabro_test::test_http_client()
        .post(format!("{}/auth/cli/refresh", harness.api_base_url))
        .bearer_auth(&refreshed_refresh_token)
        .send()
        .await
        .expect("refresh request should succeed");
    assert_eq!(refresh_response.status(), 401);
    let refresh_body: Value = refresh_response
        .json()
        .await
        .expect("refresh error body should parse");
    assert_eq!(refresh_body["error"], "refresh_token_expired");
    assert!(harness.api_requests.contains("POST /auth/cli/refresh"));
    assert!(harness.api_requests.contains("POST /auth/cli/logout"));

    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_login_surfaces_access_denied_from_real_browser_flow() {
    let context = test_context!();
    let mut github_state = GitHubAppState::new();
    github_state.allow_authorize = false;
    let harness = RealAuthHarness::start(github_state).await;
    let target = harness.api_target();

    let (login_output, browser_url) = complete_login_via_browser(&context, &target).await;
    assert!(
        browser_url.starts_with(&format!("{}/auth/cli/start", harness.web_base_url)),
        "browser flow should open the configured web origin, got: {browser_url}"
    );
    assert!(
        !login_output.status.success(),
        "auth login should fail when GitHub denies access\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&login_output.stdout),
        String::from_utf8_lossy(&login_output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&login_output.stderr).contains("Authorization denied."),
        "denial should surface in CLI stderr:\n{}",
        String::from_utf8_lossy(&login_output.stderr)
    );

    let status = auth_status(&context, &target);
    assert_eq!(status["servers"].as_array().map(Vec::len), Some(0));

    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_cli_start_ignores_dev_token_session_and_redirects_to_github_login() {
    let harness = RealAuthHarness::start_with_dev_token(GitHubAppState::new()).await;
    let client = no_redirect_browser_client();

    let login_response = client
        .post(format!("{}/auth/login/dev-token", harness.web_base_url))
        .json(&json!({ "token": TEST_DEV_TOKEN }))
        .send()
        .await
        .expect("dev-token login request should succeed");
    assert_eq!(login_response.status(), reqwest::StatusCode::OK);

    let response = client
        .get(format!(
            "{}/auth/cli/start?redirect_uri=http://127.0.0.1:4444/callback&state=abcdefghijklmnop&code_challenge=challenge&code_challenge_method=S256",
            harness.web_base_url
        ))
        .send()
        .await
        .expect("cli start request should succeed");

    assert_eq!(response.status(), reqwest::StatusCode::SEE_OTHER);
    assert_eq!(
        response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("/auth/login/github?return_to=/auth/cli/resume")
    );

    harness.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_relogin_overwrites_local_entry_and_keeps_previous_refresh_chain_alive() {
    let context = test_context!();
    let harness = RealAuthHarness::start(GitHubAppState::new()).await;
    let target = harness.api_target();

    let (first_login_output, _) = complete_login_via_browser(&context, &target).await;
    assert!(
        first_login_output.status.success(),
        "first auth login failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first_login_output.stdout),
        String::from_utf8_lossy(&first_login_output.stderr)
    );
    let first_entry = saved_auth_entry(&context);
    let first_refresh_token = first_entry["refresh_token"]
        .as_str()
        .expect("first auth entry should include refresh token")
        .to_string();
    let first_access_token = first_entry["access_token"]
        .as_str()
        .expect("first auth entry should include access token")
        .to_string();

    let (second_login_output, _) = complete_login_via_browser(&context, &target).await;
    assert!(
        second_login_output.status.success(),
        "second auth login failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second_login_output.stdout),
        String::from_utf8_lossy(&second_login_output.stderr)
    );
    let second_entry = saved_auth_entry(&context);
    let second_refresh_token = second_entry["refresh_token"]
        .as_str()
        .expect("second auth entry should include refresh token")
        .to_string();
    let second_access_token = second_entry["access_token"]
        .as_str()
        .expect("second auth entry should include access token")
        .to_string();

    assert_ne!(second_refresh_token, first_refresh_token);
    assert_ne!(second_access_token, first_access_token);

    let status = auth_status(&context, &target);
    assert_eq!(status["servers"].as_array().map(Vec::len), Some(1));
    assert_eq!(status["servers"][0]["server"], harness.api_base_url);

    let old_refresh_response = fabro_test::test_http_client()
        .post(format!("{}/auth/cli/refresh", harness.api_base_url))
        .bearer_auth(&first_refresh_token)
        .send()
        .await
        .expect("old refresh token should reach the server");
    assert_eq!(old_refresh_response.status(), reqwest::StatusCode::OK);
    let old_refresh_body: Value = old_refresh_response
        .json()
        .await
        .expect("old refresh body should parse");
    assert_ne!(
        old_refresh_body["refresh_token"].as_str(),
        Some(first_refresh_token.as_str())
    );

    let saved_after_old_refresh = saved_auth_entry(&context);
    assert_eq!(
        saved_after_old_refresh["refresh_token"].as_str(),
        Some(second_refresh_token.as_str())
    );

    harness.shutdown().await;
}

fn server_target(server: &MockServer) -> String {
    format!("{}/api/v1", server.base_url())
}

fn token_response(
    access_token: &str,
    refresh_token: &str,
    access_lifetime_secs: i64,
    refresh_lifetime_secs: i64,
) -> Value {
    let now = chrono::Utc::now();
    json!({
        "access_token": access_token,
        "access_token_expires_at": (now + chrono::Duration::seconds(access_lifetime_secs)).to_rfc3339(),
        "refresh_token": refresh_token,
        "refresh_token_expires_at": (now + chrono::Duration::seconds(refresh_lifetime_secs)).to_rfc3339(),
        "subject": {
            "idp_issuer": "https://github.com",
            "idp_subject": "12345",
            "login": "octocat",
            "name": "The Octocat",
            "email": "octocat@example.com"
        }
    })
}

fn auth_status(context: &fabro_test::TestContext, target: &str) -> Value {
    let output = context
        .command()
        .args(["--json", "auth", "status", "--server", target])
        .output()
        .expect("auth status should run");
    assert!(
        output.status.success(),
        "auth status failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("auth status JSON should parse")
}

fn complete_login(context: &fabro_test::TestContext, target: &str) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fabro"));
    apply_test_isolation(&mut cmd, &context.home_dir);
    cmd.current_dir(&context.temp_dir);
    cmd.args(["auth", "login", "--no-browser", "--server", target]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("auth login should spawn");
    let mut stdout = child
        .stdout
        .take()
        .expect("auth login stdout should be piped");
    let stderr = child
        .stderr
        .take()
        .expect("auth login stderr should be piped");
    let (url_tx, url_rx) = mpsc::channel();
    let stderr_reader = std::thread::spawn(move || read_stderr_and_capture_url(stderr, url_tx));

    let browser_url = wait_for_login_url(&mut child, &mut stdout, &url_rx);
    deliver_callback(&browser_url);

    let status = child.wait().expect("auth login should exit");
    let mut stdout_bytes = Vec::new();
    stdout
        .read_to_end(&mut stdout_bytes)
        .expect("auth login stdout should be readable");
    let stderr_bytes = stderr_reader.join().expect("stderr reader should join");

    Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    }
}

fn read_stderr_and_capture_url(
    stderr: impl std::io::Read,
    url_tx: mpsc::Sender<String>,
) -> Vec<u8> {
    let mut reader = BufReader::new(stderr);
    let mut stderr_bytes = Vec::new();
    let mut line = Vec::new();

    loop {
        line.clear();
        let read = reader
            .read_until(b'\n', &mut line)
            .expect("auth login stderr should be readable");
        if read == 0 {
            break;
        }
        let trimmed = String::from_utf8_lossy(&line).trim().to_string();
        if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            let _ = url_tx.send(trimmed);
        }
        stderr_bytes.extend_from_slice(&line);
    }

    stderr_bytes
}

fn wait_for_login_url(
    child: &mut std::process::Child,
    stdout: &mut impl Read,
    url_rx: &mpsc::Receiver<String>,
) -> String {
    let deadline = Instant::now() + LOGIN_TIMEOUT;

    loop {
        match url_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(url) => return url,
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => {}
        }

        if let Some(status) = child.try_wait().expect("auth login should stay alive") {
            let mut stdout_bytes = Vec::new();
            stdout
                .read_to_end(&mut stdout_bytes)
                .expect("auth login stdout should be readable");
            panic!(
                "auth login exited before printing the browser URL: {status}\nstdout:\n{}",
                String::from_utf8_lossy(&stdout_bytes),
            );
        }

        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait().expect("auth login should exit after kill");
            let mut stdout_bytes = Vec::new();
            stdout
                .read_to_end(&mut stdout_bytes)
                .expect("auth login stdout should be readable");
            panic!(
                "timed out waiting for auth login browser URL\nstatus: {status}\nstdout:\n{}",
                String::from_utf8_lossy(&stdout_bytes),
            );
        }
    }
}

fn deliver_callback(browser_url: &str) {
    let parsed = fabro_http::Url::parse(browser_url).expect("browser URL should parse");
    let mut redirect_uri = None;
    let mut state = None;

    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "redirect_uri" => redirect_uri = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            _ => {}
        }
    }

    let redirect_uri = redirect_uri.expect("browser URL should include redirect_uri");
    let state = state.expect("browser URL should include state");
    super::block_on(async move {
        let response = fabro_test::test_http_client()
            .get(format!("{redirect_uri}?code=cli-code&state={state}"))
            .send()
            .await
            .expect("callback request should succeed");
        assert!(
            response.status().is_success(),
            "callback request failed: {}",
            response.status()
        );
    });
}

struct RealAuthHarness {
    api_base_url: String,
    web_base_url: String,
    api_server:   RunningHttpServer,
    web_server:   RunningHttpServer,
    twin:         fabro_test::TwinGitHub,
    api_requests: ListenerRequestLog,
    web_requests: ListenerRequestLog,
}

impl RealAuthHarness {
    async fn start(github_state: GitHubAppState) -> Self {
        Self::start_with_settings(github_state, &["github"], None).await
    }

    async fn start_with_dev_token(github_state: GitHubAppState) -> Self {
        Self::start_with_settings(github_state, &["github", "dev-token"], Some(TEST_DEV_TOKEN))
            .await
    }

    async fn start_with_settings(
        github_state: GitHubAppState,
        auth_methods: &[&str],
        dev_token: Option<&str>,
    ) -> Self {
        let github_client_id = github_state.oauth_client_id.clone();
        let github_client_secret = github_state.oauth_client_secret.clone();
        let twin = fabro_test::TwinGitHub::start(github_state).await;

        let (api_listener, api_base_url) = bind_listener().await;
        let (web_listener, web_base_url) = bind_listener().await;

        let settings = auth_settings(&web_base_url, &github_client_id, auth_methods);
        let resolved = resolve_server_from_file(&settings).expect("settings should resolve");
        let dev_token = dev_token.map(str::to_string);
        let auth_mode = resolve_auth_mode_with_lookup(&resolved, |name| match name {
            "SESSION_SECRET" => Some(TEST_SESSION_SECRET.to_string()),
            "GITHUB_APP_CLIENT_SECRET" => Some(github_client_secret.clone()),
            "FABRO_DEV_TOKEN" => dev_token.clone(),
            _ => None,
        })
        .expect("auth mode should resolve");
        let state = create_app_state_with_env_lookup(settings, 5, move |name| match name {
            "SESSION_SECRET" => Some(TEST_SESSION_SECRET.to_string()),
            "GITHUB_APP_CLIENT_SECRET" => Some(github_client_secret.clone()),
            "FABRO_DEV_TOKEN" => dev_token.clone(),
            _ => None,
        });
        let github_base = github_base_url(&twin.base_url);
        let router = build_router_with_options(
            state,
            auth_mode,
            Arc::new(IpAllowlistConfig::default()),
            RouterOptions {
                web_enabled:      true,
                github_endpoints: Some(Arc::new(GithubEndpoints::with_bases(
                    github_base.clone(),
                    github_base,
                ))),
            },
        );

        let api_requests = ListenerRequestLog::default();
        let web_requests = ListenerRequestLog::default();
        let api_server = RunningHttpServer::start(api_listener, router.clone(), &api_requests);
        let web_server = RunningHttpServer::start(web_listener, router, &web_requests);
        wait_for_http_ready(&api_base_url).await;
        wait_for_http_ready(&web_base_url).await;
        api_requests.clear();
        web_requests.clear();

        Self {
            api_base_url,
            web_base_url,
            api_server,
            web_server,
            twin,
            api_requests,
            web_requests,
        }
    }

    fn api_target(&self) -> String {
        format!("{}/api/v1", self.api_base_url)
    }

    async fn shutdown(self) {
        self.api_server.shutdown().await;
        self.web_server.shutdown().await;
        self.twin.shutdown().await;
    }
}

async fn complete_login_via_browser(
    context: &fabro_test::TestContext,
    target: &str,
) -> (Output, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fabro"));
    apply_test_isolation(&mut cmd, &context.home_dir);
    cmd.current_dir(&context.temp_dir);
    cmd.args(["auth", "login", "--no-browser", "--server", target]);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("auth login should spawn");
    let mut stdout = child
        .stdout
        .take()
        .expect("auth login stdout should be piped");
    let stderr = child
        .stderr
        .take()
        .expect("auth login stderr should be piped");
    let (url_tx, url_rx) = mpsc::channel();
    let stderr_reader = std::thread::spawn(move || read_stderr_and_capture_url(stderr, url_tx));

    let browser_url = wait_for_login_url(&mut child, &mut stdout, &url_rx);
    drive_browser_flow(&browser_url).await;

    let status = child.wait().expect("auth login should exit");
    let mut stdout_bytes = Vec::new();
    stdout
        .read_to_end(&mut stdout_bytes)
        .expect("auth login stdout should be readable");
    let stderr_bytes = stderr_reader.join().expect("stderr reader should join");

    (
        Output {
            status,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        },
        browser_url,
    )
}

fn saved_auth_entry(context: &fabro_test::TestContext) -> Value {
    auth_file_json(context)["servers"]
        .as_object()
        .and_then(|servers| servers.values().next())
        .cloned()
        .expect("saved auth should contain one server entry")
}

fn expire_saved_access_token(context: &fabro_test::TestContext, issuer: &str) {
    let path = auth_store_path(context);
    let mut file = auth_file_json(context);
    let entry = file["servers"]
        .as_object_mut()
        .and_then(|servers| servers.values_mut().next())
        .and_then(Value::as_object_mut)
        .expect("saved auth should contain one mutable server entry");
    let subject = entry
        .get("subject")
        .and_then(Value::as_object)
        .cloned()
        .expect("saved auth entry should include subject");

    entry.insert(
        "access_token".to_string(),
        Value::String(expired_access_token(issuer, &subject)),
    );
    entry.insert(
        "access_token_expires_at".to_string(),
        Value::String((Utc::now() - ChronoDuration::seconds(30)).to_rfc3339()),
    );

    std::fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&file).expect("saved auth should serialize")
        ),
    )
    .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
}

struct RunningHttpServer {
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle:      Option<JoinHandle<()>>,
}

impl RunningHttpServer {
    fn start(listener: TcpListener, router: Router, request_log: &ListenerRequestLog) -> Self {
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let router = router.layer(middleware::from_fn_with_state(
            request_log.clone(),
            record_request,
        ));
        let handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("test server should serve");
        });

        Self {
            shutdown_tx: Some(shutdown_tx),
            handle:      Some(handle),
        }
    }

    async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            handle.await.expect("test server task should join");
        }
    }
}

#[derive(Clone, Default)]
struct ListenerRequestLog {
    entries: Arc<Mutex<Vec<String>>>,
}

impl ListenerRequestLog {
    fn clear(&self) {
        self.entries
            .lock()
            .expect("request log mutex should lock")
            .clear();
    }

    fn contains(&self, needle: &str) -> bool {
        self.entries
            .lock()
            .expect("request log mutex should lock")
            .iter()
            .any(|entry| entry == needle)
    }
}

async fn record_request(
    AxumState(log): AxumState<ListenerRequestLog>,
    req: Request,
    next: Next,
) -> AxumResponse {
    log.entries
        .lock()
        .expect("request log mutex should lock")
        .push(format!("{} {}", req.method(), req.uri().path()));
    next.run(req).await
}

#[derive(serde::Serialize)]
struct TestJwtClaims {
    iss:         String,
    aud:         String,
    sub:         String,
    exp:         u64,
    iat:         u64,
    jti:         String,
    idp_issuer:  String,
    idp_subject: String,
    login:       String,
    name:        String,
    email:       String,
    auth_method: RunAuthMethod,
}

async fn bind_listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("test listener should bind");
    let addr = listener
        .local_addr()
        .expect("bound test listener should have an address");
    (listener, format!("http://127.0.0.1:{}", addr.port()))
}

fn auth_settings(
    web_base_url: &str,
    github_client_id: &str,
    auth_methods: &[&str],
) -> fabro_types::settings::SettingsLayer {
    let auth_methods = auth_methods
        .iter()
        .map(|method| format!("\"{method}\""))
        .collect::<Vec<_>>()
        .join(", ");
    parse_settings_layer(&format!(
        r#"
_version = 1

[server.auth]
methods = [{auth_methods}]

[server.auth.github]
allowed_usernames = ["octocat"]

[server.web]
url = "{web_base_url}"

[server.integrations.github]
client_id = "{github_client_id}"
"#
    ))
    .expect("test settings should parse")
}

fn github_base_url(base_url: &str) -> fabro_http::Url {
    fabro_http::Url::parse(&format!("{}/", base_url.trim_end_matches('/')))
        .expect("twin github base URL should parse")
}

async fn drive_browser_flow(browser_url: &str) {
    let response = browser_client()
        .get(browser_url)
        .send()
        .await
        .expect("browser flow request should succeed");
    let status = response.status();
    let body = response
        .text()
        .await
        .expect("browser flow response body should be readable");
    if status.is_success() {
        return;
    }
    assert!(
        status == reqwest::StatusCode::BAD_REQUEST && body.contains("Login failed:"),
        "browser flow failed with {status}\n{body}"
    );
}

fn browser_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .no_proxy()
        .build()
        .expect("browser client should build")
}

fn no_redirect_browser_client() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .expect("no-redirect browser client should build")
}

async fn wait_for_http_ready(base_url: &str) {
    let client = fabro_test::test_http_client();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match client.get(format!("{base_url}/health")).send().await {
            Ok(response) if response.status().is_success() => return,
            Ok(_) | Err(_) if std::time::Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            Ok(response) => panic!("server at {base_url} was not ready: {}", response.status()),
            Err(err) => panic!("server at {base_url} was not ready: {err}"),
        }
    }
}

fn auth_store_path(context: &fabro_test::TestContext) -> std::path::PathBuf {
    context.home_dir.join(".fabro/auth.json")
}

fn auth_file_json(context: &fabro_test::TestContext) -> Value {
    let path = auth_store_path(context);
    let contents = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    serde_json::from_str(&contents).expect("saved auth should parse")
}

fn expired_access_token(issuer: &str, subject: &serde_json::Map<String, Value>) -> String {
    let key = derived_jwt_key();
    let now = Utc::now();
    let claims = TestJwtClaims {
        iss:         issuer.to_string(),
        aud:         "fabro-cli".to_string(),
        sub:         subject_value(subject, "idp_subject"),
        exp:         (now - ChronoDuration::minutes(10))
            .timestamp()
            .try_into()
            .expect("expired timestamp should be positive"),
        iat:         (now - ChronoDuration::minutes(20))
            .timestamp()
            .try_into()
            .expect("issued-at timestamp should be positive"),
        jti:         Ulid::new().to_string(),
        idp_issuer:  subject_value(subject, "idp_issuer"),
        idp_subject: subject_value(subject, "idp_subject"),
        login:       subject_value(subject, "login"),
        name:        subject_value(subject, "name"),
        email:       subject_value(subject, "email"),
        auth_method: RunAuthMethod::Github,
    };

    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(&key),
    )
    .expect("expired JWT should encode")
}

fn derived_jwt_key() -> [u8; 32] {
    let hkdf = Hkdf::<Sha256>::new(None, TEST_SESSION_SECRET.as_bytes());
    let mut key = [0_u8; 32];
    hkdf.expand(b"fabro-jwt-hs256-v1", &mut key)
        .expect("HKDF should derive the fixed-size JWT key");
    key
}

fn subject_value(subject: &serde_json::Map<String, Value>, key: &str) -> String {
    subject
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| panic!("saved auth subject should include `{key}`"))
}
