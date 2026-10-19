//! The provider against a local stand-in for Azure: Entra's token endpoint,
//! the sandbox group on ARM, and a data plane whose `executeShellCommand`
//! runs the command with this machine's `sh` and whose `files` endpoint
//! reads and writes this machine's disk. The sandbox's `/workspace` is a
//! temporary directory, so the exec runner, the poll protocol and the
//! files API run for real.

#![allow(
    clippy::absolute_paths,
    clippy::format_push_string,
    clippy::type_complexity,
    clippy::unwrap_used,
    reason = "A self-contained fake of Azure prefers inline paths and panics over pedantic style lints."
)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use fabro_sandbox_aca::{
    AcaConnection, AcaEndpoints, AcaPolicy, AcaServicePrincipal, AcaTarget, KIND,
};
use sandbox_driver::{
    ExecControls, ExecSpec, HealthStatus, OutputStream, SandboxFilter, SandboxProvider,
    SandboxSource, SandboxSpec, SandboxState, StdinSource, Termination,
};
use serde_json::{Value, json};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

const GROUP: &str = "/subscriptions/sub-1/resourceGroups/rg-1/sandboxGroups/group-1";

#[derive(Default)]
struct FakeState {
    sandboxes:      Mutex<BTreeMap<String, Value>>,
    tokens_issued:  AtomicUsize,
    creates:        AtomicUsize,
    /// Fails the next `n` executeShellCommand calls after running them.
    drop_answers:   AtomicUsize,
    workspace_root: PathBuf,
    reject_secret:  bool,
}

#[derive(Clone)]
struct Fake {
    state: Arc<FakeState>,
}

impl Fake {
    fn map_path(&self, path: &str) -> PathBuf {
        match path.strip_prefix("/workspace") {
            Some(rest) => self.state.workspace_root.join(rest.trim_start_matches('/')),
            None => PathBuf::from(path),
        }
    }

    /// The provider wraps every script as `printf '%s' '<base64>' | base64
    /// -d | sh`; the fake decodes it to move `/workspace` here, nested
    /// runner and spec payloads included.
    fn map_command(&self, command: &str) -> String {
        let root = self.state.workspace_root.to_string_lossy().into_owned();
        let mut out = String::new();
        let mut rest = command;
        while let Some(start) = rest.find("printf '%s' '") {
            out.push_str(&rest[..start]);
            let after = &rest[start + "printf '%s' '".len()..];
            let end = after.find('\'').unwrap();
            let decoded = BASE64.decode(&after[..end]).unwrap();
            let decoded = String::from_utf8(decoded).unwrap();
            let mapped = self.map_command(&decoded);
            out.push_str(&format!("printf '%s' '{}'", BASE64.encode(mapped)));
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out.replace("/workspace", &root)
    }
}

async fn token(State(fake): State<Fake>, body: String) -> Response {
    if fake.state.reject_secret || !body.contains("client_secret=right-secret") {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(json!({
                "error": "invalid_client",
                "error_description": "AADSTS7000222: The provided client secret keys are expired.\r\nTrace ID: abc"
            })),
        )
            .into_response();
    }
    fake.state.tokens_issued.fetch_add(1, Ordering::SeqCst);
    axum::Json(json!({ "access_token": "token", "expires_in": 3600, "token_type": "Bearer" }))
        .into_response()
}

async fn group() -> Response {
    axum::Json(json!({
        "id": "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.App/sandboxGroups/group-1",
        "location": "westus2",
        "properties": { "provisioningState": "Succeeded" },
    }))
    .into_response()
}

async fn create(State(fake): State<Fake>, axum::Json(body): axum::Json<Value>) -> Response {
    fake.state.creates.fetch_add(1, Ordering::SeqCst);
    let id = uuid::Uuid::new_v4().to_string();
    let sandbox = json!({
        "id": id,
        "state": "Running",
        "labels": body["labels"],
        "resources": body.get("resources").cloned().unwrap_or(json!({"cpu": "1000m", "memory": "2048Mi"})),
        "sourcesRef": body["sourcesRef"],
        "egressPolicy": body.get("egressPolicy").cloned().unwrap_or(json!({"defaultAction": "Allow"})),
        "lifecycle": body["lifecycle"],
        "region": "westus2",
        "createdAt": "2026-10-19T14:44:57Z",
    });
    fake.state
        .sandboxes
        .lock()
        .unwrap()
        .insert(id, sandbox.clone());
    axum::Json(sandbox).into_response()
}

async fn list(State(fake): State<Fake>) -> Response {
    let sandboxes: Vec<Value> = fake
        .state
        .sandboxes
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect();
    axum::Json(json!({ "value": sandboxes })).into_response()
}

async fn get_sandbox(State(fake): State<Fake>, Path(id): Path<String>) -> Response {
    match fake.state.sandboxes.lock().unwrap().get(&id) {
        Some(sandbox) => axum::Json(sandbox.clone()).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"error": {"code": "NotFound", "message": "no sandbox"}})),
        )
            .into_response(),
    }
}

async fn delete_sandbox(State(fake): State<Fake>, Path(id): Path<String>) -> Response {
    match fake.state.sandboxes.lock().unwrap().remove(&id) {
        Some(_) => StatusCode::ACCEPTED.into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn lifecycle_action(
    State(fake): State<Fake>,
    Path((id, action)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    if action == "executeShellCommand" {
        return execute(fake, id, body).await;
    }
    let mut sandboxes = fake.state.sandboxes.lock().unwrap();
    let Some(sandbox) = sandboxes.get_mut(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match action.as_str() {
        "stop" => sandbox["state"] = json!("Suspended"),
        "resume" => sandbox["state"] = json!("Running"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    }
    StatusCode::ACCEPTED.into_response()
}

async fn execute(fake: Fake, id: String, body: Bytes) -> Response {
    let state = fake
        .state
        .sandboxes
        .lock()
        .unwrap()
        .get(&id)
        .map(|sandbox| sandbox["state"].clone());
    match state {
        None => return StatusCode::NOT_FOUND.into_response(),
        Some(state) if state != "Running" => {
            return (
                StatusCode::CONFLICT,
                axum::Json(json!({"error": {"code": "SandboxNotRunning", "message": "stopped"}})),
            )
                .into_response();
        }
        Some(_) => {}
    }
    let body: Value = serde_json::from_slice(&body).unwrap();
    let command = fake.map_command(body["command"].as_str().unwrap());
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir("/")
        .output()
        .await
        .unwrap();
    if fake
        .state
        .drop_answers
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
    {
        return StatusCode::BAD_GATEWAY.into_response();
    }
    axum::Json(json!({
        "exitCode": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout),
        "stderr": String::from_utf8_lossy(&output.stderr),
    }))
    .into_response()
}

async fn read_file(
    State(fake): State<Fake>,
    Path(_id): Path<String>,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    match tokio::fs::read(fake.map_path(&query["path"])).await {
        Ok(bytes) => bytes.into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn write_file(
    State(fake): State<Fake>,
    Path(_id): Path<String>,
    Query(query): Query<BTreeMap<String, String>>,
    body: Bytes,
) -> Response {
    let path = fake.map_path(&query["path"]);
    tokio::fs::create_dir_all(path.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&path, &body).await.unwrap();
    StatusCode::NO_CONTENT.into_response()
}

async fn public_images() -> Response {
    axum::Json(json!([
        { "name": "azure-dev", "status": { "state": "Ready" } },
        { "name": "building", "status": { "state": "Pending" } },
    ]))
    .into_response()
}

async fn private_images() -> Response {
    axum::Json(json!({ "value": [
        { "id": "img-1", "labels": { "name": "team-image" }, "status": { "state": "Succeeded" } }
    ] }))
    .into_response()
}

struct Harness {
    fake:     Fake,
    provider: Arc<dyn SandboxProvider>,
    _dir:     tempfile::TempDir,
}

async fn harness_with(secret: &str, policy: AcaPolicy) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let fake = Fake {
        state: Arc::new(FakeState {
            workspace_root: dir.path().join("workspace"),
            ..FakeState::default()
        }),
    };
    let data = Router::new()
        .route(&format!("{GROUP}/sandboxes"), put(create).get(list))
        .route(
            &format!("{GROUP}/sandboxes/{{id}}"),
            get(get_sandbox).delete(delete_sandbox),
        )
        .route(
            &format!("{GROUP}/sandboxes/{{id}}/files"),
            get(read_file).put(write_file),
        )
        .route(
            &format!("{GROUP}/sandboxes/{{id}}/{{action}}"),
            post(lifecycle_action),
        )
        .route(&format!("{GROUP}/diskimages"), get(private_images))
        .route(&format!("{GROUP}/diskimages/public"), get(public_images))
        .route("/tenant-1/oauth2/v2.0/token", post(token))
        .route(
            "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.App/sandboxGroups/group-1",
            get(group),
        )
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, data).await.unwrap() });
    let connection = AcaConnection {
        target: AcaTarget {
            subscription_id: "sub-1".into(),
            resource_group:  "rg-1".into(),
            sandbox_group:   "group-1".into(),
            location:        None,
        },
        credential: AcaServicePrincipal {
            tenant_id:     "tenant-1".into(),
            client_id:     "client-1".into(),
            client_secret: secret.into(),
        },
        policy,
    };
    let provider = fabro_sandbox_aca::connect(
        &connection,
        AcaEndpoints {
            authority:  base.clone(),
            arm:        base.clone(),
            data_plane: Some(base),
        },
        fabro_http::test_http_client().unwrap(),
    );
    Harness {
        fake,
        provider: Arc::new(provider),
        _dir: dir,
    }
}

async fn harness() -> Harness {
    harness_with("right-secret", AcaPolicy::default()).await
}

fn spec() -> SandboxSpec {
    let mut spec = SandboxSpec::new(SandboxSource::Image {
        reference: "azure-dev".into(),
    });
    spec.labels.insert("petri.run".into(), "run-1".into());
    spec
}

fn sink_into(
    collected: Arc<AsyncMutex<Vec<(OutputStream, Vec<u8>)>>>,
) -> sandbox_driver::OutputSink {
    Arc::new(move |stream, chunk| {
        let collected = collected.clone();
        Box::pin(async move {
            collected.lock().await.push((stream, chunk));
            Ok(())
        })
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn health_names_the_group_and_rejects_a_bad_secret_clearly() {
    let harness = harness().await;
    let health = harness.provider.health().await.unwrap();
    assert_eq!(health.status, HealthStatus::Ok, "{:?}", health.message);
    assert!(health.identity.unwrap().ends_with("/sandboxgroups/group-1"));
    assert_eq!(harness.provider.kind().as_str(), KIND);

    let bad = harness_with("wrong-secret", AcaPolicy::default()).await;
    let health = bad.provider.health().await.unwrap();
    assert_eq!(health.status, HealthStatus::Unauthorized);
    let message = health.message.unwrap();
    assert!(message.contains("expired"), "{message}");
    assert!(!message.contains("wrong-secret"), "{message}");
    assert!(!message.contains("Trace ID"), "{message}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sandbox_runs_commands_with_exact_bytes_env_cwd_and_exit_codes() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");

    let result = sandbox
        .exec()
        .run(
            &ExecSpec::bash(
                "printf 'a\\0b\\377'; printf err >&2; echo \"$GREETING\" > note; pwd; exit 3",
            )
            .env_var("GREETING", "hello 'world'"),
        )
        .await
        .unwrap();
    assert_eq!(result.termination, Termination::Exited);
    assert_eq!(result.exit_code, Some(3));
    assert!(
        result.stdout.starts_with(b"a\0b\xff"),
        "{:?}",
        result.stdout
    );
    assert!(
        result.stdout_lossy().trim_end().ends_with("/workspace"),
        "{}",
        result.stdout_lossy()
    );
    assert_eq!(result.stderr, b"err");
    assert_eq!(
        sandbox.fs().read("/workspace/note").await.unwrap(),
        b"hello 'world'\n"
    );

    let argv = sandbox
        .exec()
        .run(&ExecSpec::new("printf").args(["%s|", "two words", "$HOME", "*"]))
        .await
        .unwrap();
    assert_eq!(argv.stdout, b"two words|$HOME|*|");

    let fixed_stdin = sandbox
        .exec()
        .run(&ExecSpec::new("cat").stdin(b"from stdin\n".to_vec()))
        .await
        .unwrap();
    assert_eq!(fixed_stdin.stdout, b"from stdin\n");

    let env = sandbox.environment().await.unwrap();
    assert!(env.contains_key("PATH"));
}

#[tokio::test(flavor = "multi_thread")]
async fn output_streams_while_the_command_runs_and_large_output_arrives_whole() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    let collected = Arc::new(AsyncMutex::new(Vec::new()));
    let first_seen = Arc::new(Mutex::new(None));
    let seen = first_seen.clone();
    let inner = sink_into(collected.clone());
    let sink: sandbox_driver::OutputSink = Arc::new(move |stream, chunk| {
        seen.lock()
            .unwrap()
            .get_or_insert_with(std::time::Instant::now);
        inner(stream, chunk)
    });
    let started = std::time::Instant::now();
    let streaming = sandbox
        .exec()
        .run_streaming(
            &ExecSpec::bash("echo first; sleep 3; echo second"),
            ExecControls {
                sink: Some(sink),
                retained_output_limit: Some(1024),
                ..ExecControls::default()
            },
        )
        .await
        .unwrap();
    assert!(streaming.live_streaming);
    assert!(streaming.streams_separated);
    let first = first_seen.lock().unwrap().unwrap();
    assert!(
        first.duration_since(started) < Duration::from_millis(2500),
        "the first line arrived only after {:?}",
        first.duration_since(started)
    );
    let text: Vec<u8> = collected
        .lock()
        .await
        .iter()
        .flat_map(|(_, chunk)| chunk.clone())
        .collect();
    assert_eq!(text, b"first\nsecond\n");

    // Three MiB crosses several poll chunks.
    let big = sandbox
        .exec()
        .run_streaming(
            &ExecSpec::bash("head -c 3145728 /dev/zero | tr '\\0' 'x'"),
            ExecControls {
                retained_output_limit: Some(4 * 1024 * 1024),
                ..ExecControls::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(big.stdout_capture.observed_bytes, 3 * 1024 * 1024);
    assert!(big.result.stdout.iter().all(|byte| *byte == b'x'));
}

#[tokio::test(flavor = "multi_thread")]
async fn streamed_stdin_reaches_the_command_in_order() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    let (mut writer, reader) = tokio::io::duplex(1024);
    let feeder = tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        for line in ["one\n", "two\n", "three\n"] {
            writer.write_all(line.as_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    });
    let streaming = sandbox
        .exec()
        .run_streaming(&ExecSpec::new("cat"), ExecControls {
            stdin: Some(StdinSource::new(reader)),
            retained_output_limit: Some(1024),
            ..ExecControls::default()
        })
        .await
        .unwrap();
    feeder.await.unwrap();
    assert_eq!(streaming.result.stdout, b"one\ntwo\nthree\n");
    assert!(streaming.result.success());
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_commands_overlap() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    let started = std::time::Instant::now();
    let runs = (0..4).map(|index| {
        let sandbox = sandbox.clone();
        async move {
            sandbox
                .exec()
                .run(&ExecSpec::bash(format!("sleep 2; echo {index}")))
                .await
                .unwrap()
        }
    });
    let results = futures::future::join_all(runs).await;
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result.stdout_lossy().trim(), index.to_string());
    }
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "four 2s commands took {:?}; they did not overlap",
        started.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_and_timing_out_stop_the_command_and_its_children() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    let marker = harness.fake.state.workspace_root.join("child-alive");

    let term = CancellationToken::new();
    let cancel = term.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        cancel.cancel();
    });
    let script = format!("(sleep 4; touch {}) & echo started; wait", marker.display());
    let streaming = sandbox
        .exec()
        .run_streaming(&ExecSpec::bash(script), ExecControls {
            term: Some(term),
            retained_output_limit: Some(1024),
            ..ExecControls::default()
        })
        .await
        .unwrap();
    assert_eq!(streaming.result.termination, Termination::Cancelled);
    assert!(!streaming.result.success());
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        !marker.exists(),
        "the command's child outlived the cancellation"
    );

    let timed_out = sandbox
        .exec()
        .run(&ExecSpec::bash("sleep 30").timeout(Duration::from_secs(1)))
        .await
        .unwrap();
    assert_eq!(timed_out.termination, Termination::TimedOut);
    let streaming = sandbox
        .exec()
        .run_streaming(
            &ExecSpec::bash("sleep 30").timeout(Duration::from_secs(1)),
            ExecControls::buffered(),
        )
        .await
        .unwrap();
    assert_eq!(streaming.result.termination, Termination::TimedOut);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_lost_launch_answer_never_runs_the_command_twice() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    harness.fake.state.drop_answers.store(1, Ordering::SeqCst);
    let result = sandbox
        .exec()
        .run(&ExecSpec::bash("echo x >> /workspace/count; echo done"))
        .await
        .unwrap();
    assert_eq!(result.stdout, b"done\n");
    let count = sandbox.fs().read("/workspace/count").await.unwrap();
    assert_eq!(count, b"x\n", "the command ran more than once");
}

#[tokio::test(flavor = "multi_thread")]
async fn files_are_binary_safe_and_listed() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    let bytes: Vec<u8> = (0..=255).collect();
    sandbox
        .fs()
        .write("/workspace/dir/bin", &bytes)
        .await
        .unwrap();
    assert_eq!(
        sandbox.fs().read("/workspace/dir/bin").await.unwrap(),
        bytes
    );
    let metadata = sandbox.fs().metadata("/workspace/dir/bin").await.unwrap();
    assert_eq!(metadata.size, 256);
    let entries = sandbox.fs().list_dir("/workspace", 2).await.unwrap();
    assert!(
        entries.iter().any(|entry| entry.path == "dir/bin"),
        "{entries:?}"
    );
    sandbox.fs().delete("/workspace/dir", true).await.unwrap();
    assert!(!sandbox.fs().exists("/workspace/dir/bin").await.unwrap());

    let local = tempfile::tempdir().unwrap();
    let upload = local.path().join("up.bin");
    tokio::fs::write(&upload, &bytes).await.unwrap();
    sandbox
        .fs()
        .upload(&upload, "/workspace/up.bin")
        .await
        .unwrap();
    let download = local.path().join("down.bin");
    sandbox
        .fs()
        .download("/workspace/up.bin", &download)
        .await
        .unwrap();
    assert_eq!(tokio::fs::read(download).await.unwrap(), bytes);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_resume_attach_list_and_delete_are_safe_to_repeat() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    sandbox
        .fs()
        .write("/workspace/keep", b"kept")
        .await
        .unwrap();
    sandbox.stop().await.unwrap();
    sandbox.stop().await.unwrap();
    assert_eq!(
        sandbox.describe().await.unwrap().state,
        SandboxState::Stopped
    );
    sandbox.start().await.unwrap();

    let attached = harness.provider.attach(sandbox.id(), None).await.unwrap();
    assert_eq!(
        attached.fs().read("/workspace/keep").await.unwrap(),
        b"kept"
    );

    let mut filter = SandboxFilter::default();
    filter.labels.insert("petri.run".into(), "run-1".into());
    let listed = harness.provider.list(&filter).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].image.as_deref(), Some("azure-dev"));
    filter
        .labels
        .insert("petri.run".into(), "another-run".into());
    assert!(harness.provider.list(&filter).await.unwrap().is_empty());

    harness.provider.delete(sandbox.id(), None).await.unwrap();
    harness.provider.delete(sandbox.id(), None).await.unwrap();
    assert!(matches!(
        harness.provider.attach(sandbox.id(), None).await,
        Err(sandbox_driver::Error::NotFound { .. })
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn images_must_be_approved_and_ready_before_anything_is_created() {
    let harness = harness_with("right-secret", AcaPolicy {
        allowed_images: vec!["azure-dev".into(), "building".into()],
        ..AcaPolicy::default()
    })
    .await;
    let mut unapproved = spec();
    unapproved.source = SandboxSource::Image {
        reference: "team-image".into(),
    };
    assert!(harness.provider.create(&unapproved, None).await.is_err());
    let mut pending = spec();
    pending.source = SandboxSource::Image {
        reference: "building".into(),
    };
    let error = harness.provider.create(&pending, None).await.err().unwrap();
    assert!(error.to_string().contains("not ready"), "{error}");
    assert_eq!(harness.fake.state.creates.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_command_on_a_suspended_sandbox_resumes_it_first() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    // ACA suspended it for idleness.
    for value in harness.fake.state.sandboxes.lock().unwrap().values_mut() {
        value["state"] = json!("Suspended");
    }
    let result = sandbox
        .exec()
        .run(&ExecSpec::bash("echo awake"))
        .await
        .unwrap();
    assert_eq!(result.stdout, b"awake\n");
    assert_eq!(
        sandbox.describe().await.unwrap().state,
        SandboxState::Running
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sandboxes_suspend_when_idle_and_are_deleted_after_their_retention() {
    let harness = harness_with("right-secret", AcaPolicy {
        retention_secs: Some(3600),
        ..AcaPolicy::default()
    })
    .await;
    harness.provider.create(&spec(), None).await.unwrap();
    let sandboxes = harness.fake.state.sandboxes.lock().unwrap();
    let lifecycle = &sandboxes.values().next().unwrap()["lifecycle"];
    assert_eq!(lifecycle["autoSuspendPolicy"]["enabled"], true);
    assert_eq!(
        lifecycle["autoDeletePolicy"]["deleteIntervalInSeconds"],
        3600
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sandbox_port_is_reached_through_a_private_relay() {
    let harness = harness().await;
    let sandbox = harness.provider.create(&spec(), None).await.unwrap();
    // A server "inside the sandbox": in the fake, the sandbox is this host.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                use tokio::io::{AsyncReadExt, AsyncWriteExt};
                let mut buffer = vec![0; 4096];
                let read = stream.read(&mut buffer).await.unwrap();
                let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
                let body = format!("{{\"echo\":{}}}", request.lines().count());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            });
        }
    });

    let previews = sandbox.preview_urls().expect("ACA routes ports privately");
    let route = previews.preview_url(port).await.unwrap();
    assert!(route.url.starts_with("http://127.0.0.1:"), "{}", route.url);
    let client = fabro_http::test_http_client().unwrap();
    for _ in 0..2 {
        let response = client
            .post(format!("{}/mcp", route.url))
            .body("{\"jsonrpc\":\"2.0\"}")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let body: Value = response.json().await.unwrap();
        assert!(body["echo"].as_u64().unwrap() > 1, "{body}");
    }
    previews.release_preview_url(port).await.unwrap();
    previews.release_preview_url(port).await.unwrap();
}
