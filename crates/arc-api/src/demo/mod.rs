//! Demo mode handlers that return static data for all API endpoints.
//! Used with `arc serve --demo` to showcase the UI without a real backend.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use crate::jwt_auth::AuthenticatedService;
use crate::server::AppState;

// ── Runs ───────────────────────────────────────────────────────────────

pub async fn list_runs(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(runs::list_items())).into_response()
}

pub async fn start_run_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"id": "demo-run-new"})),
    )
        .into_response()
}

pub async fn get_run_stages(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(runs::stages())).into_response()
}

pub async fn get_stage_turns(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path((_id, _stage_id)): Path<(String, String)>,
) -> Response {
    (StatusCode::OK, Json(runs::turns())).into_response()
}

#[derive(serde::Deserialize)]
pub struct FilesQuery {
    #[allow(dead_code)]
    checkpoint: Option<String>,
}

pub async fn get_run_files(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
    Query(_q): Query<FilesQuery>,
) -> Response {
    (StatusCode::OK, Json(runs::files())).into_response()
}

pub async fn get_run_usage(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(runs::usage())).into_response()
}

pub async fn get_run_verifications(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(runs::verifications())).into_response()
}

pub async fn get_run_configuration(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (
        StatusCode::OK,
        [("content-type", "text/plain")],
        runs::configuration(),
    )
        .into_response()
}

pub async fn steer_run_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(serde_json::json!({"accepted": true}))).into_response()
}

pub async fn get_run_status(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match runs::list_items().into_iter().find(|r| r.id == id) {
        Some(_) => (
            StatusCode::OK,
            Json(arc_types::RunStatusResponse {
                id: id.clone(),
                status: arc_types::RunStatus::Running,
                error: None,
            }),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn get_questions_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    let empty: Vec<arc_types::ApiQuestion> = vec![];
    (StatusCode::OK, Json(empty)).into_response()
}

pub async fn answer_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path((_id, _qid)): Path<(String, String)>,
) -> Response {
    (StatusCode::OK, Json(serde_json::json!({"accepted": true}))).into_response()
}

pub async fn run_events_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    StatusCode::GONE.into_response()
}

pub async fn checkpoint_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(serde_json::json!(null))).into_response()
}

pub async fn context_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(serde_json::json!({}))).into_response()
}

pub async fn cancel_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(serde_json::json!({"cancelled": true}))).into_response()
}

pub async fn get_run_graph(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    // Use graphviz to render the demo DOT source
    let dot_source = "digraph demo {\n  graph [goal=\"Demo\"]\n  rankdir=LR\n  start [shape=Mdiamond, label=\"Start\"]\n  detect [label=\"Detect\\nDrift\"]\n  exit [shape=Msquare, label=\"Exit\"]\n  propose [label=\"Propose\\nChanges\"]\n  review [label=\"Review\\nChanges\"]\n  apply [label=\"Apply\\nChanges\"]\n  start -> detect\n  detect -> exit [label=\"No drift\"]\n  detect -> propose [label=\"Drift found\"]\n  propose -> review\n  review -> propose [label=\"Revise\"]\n  review -> apply [label=\"Accept\"]\n  apply -> exit\n}";

    let mut child = match tokio::process::Command::new("dot")
        .arg("-Tsvg")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": "graphviz dot command not available"})),
            )
                .into_response();
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        let _ = stdin.write_all(dot_source.as_bytes()).await;
    }

    match child.wait_with_output().await {
        Ok(output) if output.status.success() => (
            StatusCode::OK,
            [("content-type", "image/svg+xml")],
            output.stdout,
        )
            .into_response(),
        _ => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": "dot rendering failed"})),
        )
            .into_response(),
    }
}

pub async fn get_run_retro(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    // Return a demo retro as JSON
    (StatusCode::OK, Json(serde_json::json!({
        "run_id": "run-1",
        "pipeline_name": "implement",
        "goal": "Add rate limiting to auth endpoints",
        "smoothness": "smooth",
        "intent": "Implement token-bucket rate limiting on /auth/login and /auth/register to prevent brute-force attacks.",
        "outcome": "Rate limiter deployed with configurable per-IP limits. Integration tests added. Redis-backed counter with sliding window.",
        "stages": [
            {"stage_id": "detect-drift", "stage_label": "Detect Drift", "status": "completed", "duration_ms": 72000, "retries": 0, "cost": 0.48, "files_touched": ["src/middleware/rate-limit.ts"]},
            {"stage_id": "propose-changes", "stage_label": "Propose Changes", "status": "completed", "duration_ms": 154000, "retries": 0, "cost": 1.12, "files_touched": ["src/middleware/rate-limit.ts", "src/routes/auth.ts", "src/config.ts"]},
            {"stage_id": "review-changes", "stage_label": "Review Changes", "status": "completed", "duration_ms": 45000, "retries": 0, "cost": 0.31, "files_touched": []},
            {"stage_id": "apply-changes", "stage_label": "Apply Changes", "status": "completed", "duration_ms": 118000, "retries": 0, "cost": 0.87, "files_touched": ["src/middleware/rate-limit.ts", "src/routes/auth.ts", "src/config.ts", "tests/rate-limit.test.ts"]}
        ],
        "stats": {
            "total_duration_ms": 389000,
            "total_cost": 2.78,
            "total_retries": 0,
            "files_touched": ["src/middleware/rate-limit.ts", "src/routes/auth.ts", "src/config.ts", "tests/rate-limit.test.ts"],
            "stages_completed": 4,
            "stages_failed": 0
        },
        "learnings": [
            {"category": "repo", "text": "Redis client is initialized lazily in src/infra/redis.ts -- reuse existing connection pool."},
            {"category": "code", "text": "Auth middleware chain order matters: rate-limit must run before JWT validation."}
        ],
        "friction_points": [],
        "open_items": [
            {"kind": "follow_up", "description": "Add rate-limit headers (X-RateLimit-Remaining) to response."}
        ]
    }))).into_response()
}

// ── Workflows ──────────────────────────────────────────────────────────

pub async fn list_workflows(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(workflows::list_items())).into_response()
}

pub async fn get_workflow(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Response {
    match workflows::detail(&name) {
        Some(detail) => (StatusCode::OK, Json(detail)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn list_workflow_runs(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Response {
    let items: Vec<_> = runs::list_items()
        .into_iter()
        .filter(|r| r.workflow == name)
        .collect();
    (StatusCode::OK, Json(items)).into_response()
}

pub async fn trigger_workflow_run_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_name): Path<String>,
) -> Response {
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"id": "demo-workflow-run"})),
    )
        .into_response()
}

// ── Verifications ──────────────────────────────────────────────────────

pub async fn list_verifications(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(verifications::categories())).into_response()
}

pub async fn get_verification_detail(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(slug): Path<String>,
) -> Response {
    match verifications::detail(&slug) {
        Some(detail) => (StatusCode::OK, Json(detail)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// ── Retros ─────────────────────────────────────────────────────────────

pub async fn list_retros(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(retros::list_items())).into_response()
}

// ── Sessions ───────────────────────────────────────────────────────────

pub async fn list_sessions(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(sessions::groups())).into_response()
}

pub async fn create_session_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"id": "demo-session-new"})),
    )
        .into_response()
}

pub async fn get_session(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match sessions::detail(&id) {
        Some(detail) => (StatusCode::OK, Json(detail)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn send_message_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({"accepted": true})),
    )
        .into_response()
}

pub async fn session_events_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    // Return an empty SSE-like response
    StatusCode::GONE.into_response()
}

// ── Insights ───────────────────────────────────────────────────────────

pub async fn list_saved_queries(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(insights::saved_queries())).into_response()
}

pub async fn save_query_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (
        StatusCode::CREATED,
        Json(serde_json::json!({"id": "new-q", "name": "New Query", "sql": "SELECT 1"})),
    )
        .into_response()
}

pub async fn update_query_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({"id": "1", "name": "Updated", "sql": "SELECT 1"})),
    )
        .into_response()
}

pub async fn delete_query_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    StatusCode::NO_CONTENT.into_response()
}

pub async fn execute_query_stub(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "columns": ["workflow_name", "count"],
            "rows": [["implement", 42], ["fix_build", 18], ["sync_drift", 7]],
            "elapsed": 0.342,
            "row_count": 3
        })),
    )
        .into_response()
}

pub async fn list_query_history(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(insights::history())).into_response()
}

// ── Settings ───────────────────────────────────────────────────────────

pub async fn get_settings(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(settings::groups())).into_response()
}

// ── Projects ───────────────────────────────────────────────────────────

pub async fn list_projects(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
) -> Response {
    (StatusCode::OK, Json(projects::list_items())).into_response()
}

pub async fn list_branches(
    _auth: AuthenticatedService,
    State(_state): State<Arc<AppState>>,
    Path(_id): Path<String>,
) -> Response {
    (StatusCode::OK, Json(projects::branches())).into_response()
}

// ── Data modules ───────────────────────────────────────────────────────

mod runs {
    use arc_types::*;

    pub fn list_items() -> Vec<RunListItem> {
        vec![
            RunListItem {
                id: "run-1".into(), repo: "api-server".into(),
                title: "Add rate limiting to auth endpoints".into(),
                workflow: "implement".into(), status: RunListItemStatus::Working,
                number: None, additions: None, deletions: None, checks: vec![],
                elapsed_secs: Some(420.0), elapsed_warning: Some(false),
                resources: Some("4 CPU / 8 GB".into()), comments: Some(0),
                question: None, sandbox_id: Some("sb-a1b2c3d4".into()),
            },
            RunListItem {
                id: "run-2".into(), repo: "web-dashboard".into(),
                title: "Migrate to React Router v7".into(),
                workflow: "implement".into(), status: RunListItemStatus::Working,
                number: None, additions: None, deletions: None, checks: vec![],
                elapsed_secs: Some(8100.0), elapsed_warning: Some(false),
                resources: Some("8 CPU / 16 GB".into()), comments: Some(0),
                question: None, sandbox_id: Some("sb-e5f6g7h8".into()),
            },
            RunListItem {
                id: "run-3".into(), repo: "cli-tools".into(),
                title: "Fix config parsing for nested values".into(),
                workflow: "fix_build".into(), status: RunListItemStatus::Working,
                number: None, additions: None, deletions: None, checks: vec![],
                elapsed_secs: Some(2700.0), elapsed_warning: Some(false),
                resources: Some("2 CPU / 4 GB".into()), comments: Some(0),
                question: None, sandbox_id: Some("sb-i9j0k1l2".into()),
            },
            RunListItem {
                id: "run-4".into(), repo: "api-server".into(),
                title: "Update OpenAPI spec for v3".into(),
                workflow: "expand".into(), status: RunListItemStatus::Pending,
                number: None, additions: Some(567), deletions: Some(234), checks: vec![],
                elapsed_secs: Some(4320.0), elapsed_warning: Some(false),
                resources: None, comments: Some(0),
                question: Some("Accept or push for another round?".into()),
                sandbox_id: Some("sb-q7r8s9t0".into()),
            },
            RunListItem {
                id: "run-5".into(), repo: "shared-types".into(),
                title: "Add pipeline event types".into(),
                workflow: "implement".into(), status: RunListItemStatus::Pending,
                number: None, additions: Some(145), deletions: Some(23), checks: vec![],
                elapsed_secs: Some(1680.0), elapsed_warning: Some(false),
                resources: None, comments: Some(0),
                question: Some("Proceed from investigation to fix?".into()),
                sandbox_id: Some("sb-u1v2w3x4".into()),
            },
            RunListItem {
                id: "run-6".into(), repo: "web-dashboard".into(),
                title: "Add dark mode toggle".into(),
                workflow: "implement".into(), status: RunListItemStatus::Review,
                number: Some(889), additions: Some(234), deletions: Some(67),
                checks: vec![
                    CheckRun { name: "lint".into(), status: CheckRunStatus::Success, duration_secs: Some(23.0) },
                    CheckRun { name: "typecheck".into(), status: CheckRunStatus::Success, duration_secs: Some(72.0) },
                    CheckRun { name: "unit-tests".into(), status: CheckRunStatus::Success, duration_secs: Some(154.0) },
                    CheckRun { name: "integration-tests".into(), status: CheckRunStatus::Failure, duration_secs: Some(296.0) },
                    CheckRun { name: "e2e / chrome".into(), status: CheckRunStatus::Failure, duration_secs: Some(182.0) },
                    CheckRun { name: "build".into(), status: CheckRunStatus::Success, duration_secs: Some(105.0) },
                    CheckRun { name: "coverage".into(), status: CheckRunStatus::Skipped, duration_secs: None },
                ],
                elapsed_secs: Some(2100.0), elapsed_warning: Some(false),
                resources: None, comments: Some(4),
                question: None, sandbox_id: Some("sb-m3n4o5p6".into()),
            },
            RunListItem {
                id: "run-7".into(), repo: "infrastructure".into(),
                title: "Terraform module for Redis cluster".into(),
                workflow: "implement".into(), status: RunListItemStatus::Review,
                number: Some(156), additions: Some(412), deletions: Some(0),
                checks: vec![
                    CheckRun { name: "lint".into(), status: CheckRunStatus::Success, duration_secs: Some(18.0) },
                    CheckRun { name: "typecheck".into(), status: CheckRunStatus::Success, duration_secs: Some(56.0) },
                    CheckRun { name: "unit-tests".into(), status: CheckRunStatus::Pending, duration_secs: None },
                    CheckRun { name: "integration-tests".into(), status: CheckRunStatus::Queued, duration_secs: None },
                    CheckRun { name: "build".into(), status: CheckRunStatus::Pending, duration_secs: None },
                ],
                elapsed_secs: Some(720.0), elapsed_warning: Some(false),
                resources: None, comments: Some(1),
                question: None, sandbox_id: Some("sb-y5z6a7b8".into()),
            },
            RunListItem {
                id: "run-8".into(), repo: "api-server".into(),
                title: "Implement webhook retry logic".into(),
                workflow: "implement".into(), status: RunListItemStatus::Merge,
                number: Some(1249), additions: Some(189), deletions: Some(45),
                checks: vec![
                    CheckRun { name: "lint".into(), status: CheckRunStatus::Success, duration_secs: Some(21.0) },
                    CheckRun { name: "typecheck".into(), status: CheckRunStatus::Success, duration_secs: Some(68.0) },
                    CheckRun { name: "unit-tests".into(), status: CheckRunStatus::Success, duration_secs: Some(192.0) },
                    CheckRun { name: "integration-tests".into(), status: CheckRunStatus::Success, duration_secs: Some(334.0) },
                    CheckRun { name: "build".into(), status: CheckRunStatus::Success, duration_secs: Some(121.0) },
                ],
                elapsed_secs: Some(259200.0), elapsed_warning: Some(true),
                resources: None, comments: Some(7),
                question: None, sandbox_id: Some("sb-c9d0e1f2".into()),
            },
            RunListItem {
                id: "run-9".into(), repo: "cli-tools".into(),
                title: "Add --verbose flag to run command".into(),
                workflow: "expand".into(), status: RunListItemStatus::Merge,
                number: Some(430), additions: Some(56), deletions: Some(12),
                checks: vec![
                    CheckRun { name: "lint".into(), status: CheckRunStatus::Success, duration_secs: Some(15.0) },
                    CheckRun { name: "typecheck".into(), status: CheckRunStatus::Success, duration_secs: Some(48.0) },
                    CheckRun { name: "unit-tests".into(), status: CheckRunStatus::Success, duration_secs: Some(116.0) },
                    CheckRun { name: "build".into(), status: CheckRunStatus::Success, duration_secs: Some(82.0) },
                ],
                elapsed_secs: Some(3900.0), elapsed_warning: Some(false),
                resources: None, comments: Some(2),
                question: None, sandbox_id: Some("sb-g3h4i5j6".into()),
            },
            RunListItem {
                id: "run-10".into(), repo: "shared-types".into(),
                title: "Export utility type helpers".into(),
                workflow: "sync_drift".into(), status: RunListItemStatus::Merge,
                number: Some(76), additions: Some(34), deletions: Some(8),
                checks: vec![
                    CheckRun { name: "lint".into(), status: CheckRunStatus::Success, duration_secs: Some(12.0) },
                    CheckRun { name: "typecheck".into(), status: CheckRunStatus::Success, duration_secs: Some(34.0) },
                    CheckRun { name: "unit-tests".into(), status: CheckRunStatus::Success, duration_secs: Some(75.0) },
                    CheckRun { name: "build".into(), status: CheckRunStatus::Success, duration_secs: Some(58.0) },
                ],
                elapsed_secs: Some(2880.0), elapsed_warning: Some(false),
                resources: None, comments: Some(0),
                question: None, sandbox_id: Some("sb-k7l8m9n0".into()),
            },
        ]
    }

    pub fn stages() -> Vec<RunStage> {
        vec![
            RunStage { id: "detect-drift".into(), name: "Detect Drift".into(), status: StageStatus::Completed, duration_secs: Some(72.0), dot_id: Some("detect".into()) },
            RunStage { id: "propose-changes".into(), name: "Propose Changes".into(), status: StageStatus::Completed, duration_secs: Some(154.0), dot_id: Some("propose".into()) },
            RunStage { id: "review-changes".into(), name: "Review Changes".into(), status: StageStatus::Completed, duration_secs: Some(45.0), dot_id: Some("review".into()) },
            RunStage { id: "apply-changes".into(), name: "Apply Changes".into(), status: StageStatus::Running, duration_secs: Some(118.0), dot_id: Some("apply".into()) },
        ]
    }

    pub fn turns() -> Vec<StageTurn> {
        vec![
            StageTurn { kind: StageTurnKind::System, content: Some("You are a drift detection agent. Compare the production and staging environments and identify any configuration or code drift.".into()), tools: vec![] },
            StageTurn { kind: StageTurnKind::Assistant, content: Some("I'll start by loading the environment configurations for both production and staging to compare them.".into()), tools: vec![] },
            StageTurn {
                kind: StageTurnKind::Tool, content: None,
                tools: vec![
                    ToolUse { tool_name: "read_file".into(), args: r#"{ "path": "environments/production/config.toml" }"#.into(), result: "[redis]\nhost = \"redis-prod.internal\"\nport = 6379".into() },
                    ToolUse { tool_name: "read_file".into(), args: r#"{ "path": "environments/staging/config.toml" }"#.into(), result: "[redis]\nhost = \"redis-staging.internal\"\nport = 6379".into() },
                ],
            },
            StageTurn { kind: StageTurnKind::Assistant, content: Some("I've detected drift in 3 resources between production and staging:\n\n1. **redis.max_connections** — production has 200, staging has 100\n2. **redis.tls** — enabled in production, disabled in staging\n3. **iam.session_duration** — production uses 3600s, staging uses 1800s".into()), tools: vec![] },
        ]
    }

    pub fn files() -> RunFiles {
        RunFiles {
            checkpoints: vec![
                FileCheckpoint { id: "all".into(), label: "All changes".into() },
                FileCheckpoint { id: "cp-4".into(), label: "Checkpoint 4 — Apply Changes".into() },
                FileCheckpoint { id: "cp-3".into(), label: "Checkpoint 3 — Review Changes".into() },
                FileCheckpoint { id: "cp-2".into(), label: "Checkpoint 2 — Propose Changes".into() },
                FileCheckpoint { id: "cp-1".into(), label: "Checkpoint 1 — Detect Drift".into() },
            ],
            files: vec![
                FileDiff {
                    old_file: DiffFile { name: "src/commands/run.ts".into(), contents: "import { parseArgs } from \"node:util\";\nimport { loadConfig } from \"../config.js\";\nimport { execute } from \"../executor.js\";\n\ninterface RunOptions {\n  config: string;\n  dryRun: boolean;\n}\n\nexport async function run(argv: string[]) {\n  const { values } = parseArgs({\n    args: argv,\n    options: {\n      config: { type: \"string\", short: \"c\", default: \"arc.toml\" },\n      \"dry-run\": { type: \"boolean\", default: false },\n    },\n  });\n\n  const opts: RunOptions = {\n    config: values.config ?? \"arc.toml\",\n    dryRun: values[\"dry-run\"] ?? false,\n  };\n\n  const config = await loadConfig(opts.config);\n  const result = await execute(config, { dryRun: opts.dryRun });\n\n  if (result.success) {\n    console.log(\"Run completed successfully.\");\n  } else {\n    console.error(\"Run failed:\", result.error);\n    process.exitCode = 1;\n  }\n}\n".into() },
                    new_file: DiffFile { name: "src/commands/run.ts".into(), contents: "import { parseArgs } from \"node:util\";\nimport { loadConfig } from \"../config.js\";\nimport { execute } from \"../executor.js\";\nimport { createLogger, type Logger } from \"../logger.js\";\n\ninterface RunOptions {\n  config: string;\n  dryRun: boolean;\n  verbose: boolean;\n}\n\nexport async function run(argv: string[]) {\n  const { values } = parseArgs({\n    args: argv,\n    options: {\n      config: { type: \"string\", short: \"c\", default: \"arc.toml\" },\n      \"dry-run\": { type: \"boolean\", default: false },\n      verbose: { type: \"boolean\", short: \"v\", default: false },\n    },\n  });\n\n  const opts: RunOptions = {\n    config: values.config ?? \"arc.toml\",\n    dryRun: values[\"dry-run\"] ?? false,\n    verbose: values.verbose ?? false,\n  };\n\n  const logger: Logger = createLogger({ verbose: opts.verbose });\n\n  const config = await loadConfig(opts.config);\n  logger.debug(\"Loaded config from %s\", opts.config);\n\n  const result = await execute(config, { dryRun: opts.dryRun, logger });\n  logger.debug(\"Execution finished in %dms\", result.elapsed);\n\n  if (result.success) {\n    console.log(\"Run completed successfully.\");\n  } else {\n    console.error(\"Run failed:\", result.error);\n    process.exitCode = 1;\n  }\n}\n".into() },
                },
                FileDiff {
                    old_file: DiffFile { name: "src/logger.ts".into(), contents: "".into() },
                    new_file: DiffFile { name: "src/logger.ts".into(), contents: "export interface Logger {\n  info(message: string, ...args: unknown[]): void;\n  debug(message: string, ...args: unknown[]): void;\n  error(message: string, ...args: unknown[]): void;\n}\n\ninterface LoggerOptions {\n  verbose: boolean;\n}\n\nexport function createLogger({ verbose }: LoggerOptions): Logger {\n  return {\n    info(message, ...args) {\n      console.log(message, ...args);\n    },\n    debug(message, ...args) {\n      if (verbose) {\n        console.log(\"[debug]\", message, ...args);\n      }\n    },\n    error(message, ...args) {\n      console.error(message, ...args);\n    },\n  };\n}\n".into() },
                },
                FileDiff {
                    old_file: DiffFile { name: "src/executor.ts".into(), contents: "import type { Config } from \"./config.js\";\n\ninterface ExecuteOptions {\n  dryRun: boolean;\n}\n\ninterface ExecuteResult {\n  success: boolean;\n  error?: string;\n}\n\nexport async function execute(\n  config: Config,\n  options: ExecuteOptions,\n): Promise<ExecuteResult> {\n  if (options.dryRun) {\n    console.log(\"Dry run — skipping execution.\");\n    return { success: true };\n  }\n\n  try {\n    for (const step of config.steps) {\n      await step.run();\n    }\n    return { success: true };\n  } catch (err) {\n    const message = err instanceof Error ? err.message : String(err);\n    return { success: false, error: message };\n  }\n}\n".into() },
                    new_file: DiffFile { name: "src/executor.ts".into(), contents: "import type { Config } from \"./config.js\";\nimport type { Logger } from \"./logger.js\";\n\ninterface ExecuteOptions {\n  dryRun: boolean;\n  logger: Logger;\n}\n\ninterface ExecuteResult {\n  success: boolean;\n  elapsed: number;\n  error?: string;\n}\n\nexport async function execute(\n  config: Config,\n  options: ExecuteOptions,\n): Promise<ExecuteResult> {\n  const start = performance.now();\n\n  if (options.dryRun) {\n    options.logger.info(\"Dry run — skipping execution.\");\n    return { success: true, elapsed: performance.now() - start };\n  }\n\n  try {\n    for (const step of config.steps) {\n      options.logger.debug(\"Running step: %s\", step.name);\n      await step.run();\n    }\n    return { success: true, elapsed: performance.now() - start };\n  } catch (err) {\n    const message = err instanceof Error ? err.message : String(err);\n    return { success: false, elapsed: performance.now() - start, error: message };\n  }\n}\n".into() },
                },
            ],
            stats: DiffStats { additions: 567, deletions: 234 },
        }
    }

    pub fn usage() -> RunUsage {
        RunUsage {
            stages: vec![
                UsageStage { stage: "Detect Drift".into(), model: "Opus 4.6".into(), input_tokens: 12480, output_tokens: 3210, runtime_secs: 72.0, cost: 0.48 },
                UsageStage { stage: "Propose Changes".into(), model: "Gemini 3.1".into(), input_tokens: 28640, output_tokens: 8750, runtime_secs: 154.0, cost: 0.72 },
                UsageStage { stage: "Review Changes".into(), model: "Codex 5.3".into(), input_tokens: 9120, output_tokens: 2640, runtime_secs: 45.0, cost: 0.19 },
                UsageStage { stage: "Apply Changes".into(), model: "Opus 4.6".into(), input_tokens: 21300, output_tokens: 6480, runtime_secs: 118.0, cost: 0.87 },
            ],
            totals: UsageTotals { runtime_secs: 389.0, input_tokens: 71540, output_tokens: 21080, cost: 2.26 },
            by_model: vec![
                UsageByModel { model: "Opus 4.6".into(), stages: 2, input_tokens: 33780, output_tokens: 9690, cost: 1.35 },
                UsageByModel { model: "Gemini 3.1".into(), stages: 1, input_tokens: 28640, output_tokens: 8750, cost: 0.72 },
                UsageByModel { model: "Codex 5.3".into(), stages: 1, input_tokens: 9120, output_tokens: 2640, cost: 0.19 },
            ],
        }
    }

    pub fn verifications() -> Vec<RunVerification> {
        vec![
            RunVerification {
                name: "Traceability".into(),
                question: "Do we understand what this change is and why we're making it?".into(),
                status: VerificationStatus::Pass,
                controls: vec![
                    RunVerificationControl { name: "Motivation".into(), description: "Origin of proposal identified".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Specifications".into(), description: "Requirements written down".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Documentation".into(), description: "Developer and user docs added".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Minimization".into(), description: "No extraneous changes".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Pass },
                ],
            },
            RunVerification {
                name: "Readability".into(),
                question: "Can a human or agent quickly read this and understand what it does?".into(),
                status: VerificationStatus::Pass,
                controls: vec![
                    RunVerificationControl { name: "Formatting".into(), description: "Code layout matches standard".into(), type_: Some(VerificationType::Automated), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Linting".into(), description: "Linter issues resolved".into(), type_: Some(VerificationType::Automated), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Style".into(), description: "House style applied".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Pass },
                ],
            },
            RunVerification {
                name: "Reliability".into(),
                question: "Will this behave correctly and safely under real-world conditions?".into(),
                status: VerificationStatus::Pass,
                controls: vec![
                    RunVerificationControl { name: "Completeness".into(), description: "Implementation covers requirements".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Defects".into(), description: "Potential or likely bugs remediated".into(), type_: Some(VerificationType::AiAnalysis), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Performance".into(), description: "Hot path impact identified".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Pass },
                ],
            },
            RunVerification {
                name: "Code Coverage".into(),
                question: "Do we have trustworthy, automated evidence that it works?".into(),
                status: VerificationStatus::Fail,
                controls: vec![
                    RunVerificationControl { name: "Test Coverage".into(), description: "Production code exercised by unit tests".into(), type_: Some(VerificationType::Analysis), status: VerificationStatus::Pass },
                    RunVerificationControl { name: "Test Quality".into(), description: "Tests are robust and clear".into(), type_: Some(VerificationType::Ai), status: VerificationStatus::Fail },
                    RunVerificationControl { name: "E2E Coverage".into(), description: "Browser automation exercises UX".into(), type_: Some(VerificationType::Analysis), status: VerificationStatus::Na },
                ],
            },
        ]
    }

    pub fn configuration() -> String {
        r#"version = 1

[task]
goal = "Add rate limiting to auth endpoints"
repo = "api-server"
branch = "feature/rate-limiting"

[agent]
model = "claude-opus-4-6"
max_retries = 3

[verification]
enabled = true
categories = ["traceability", "readability", "reliability", "coverage"]
"#
        .to_string()
    }
}

mod workflows {
    use arc_types::*;

    pub fn list_items() -> Vec<WorkflowListItem> {
        vec![
            WorkflowListItem { name: "Fix Build".into(), slug: "fix_build".into(), filename: "fix_build.dot".into(), last_run: Some("2 hours ago".into()), schedule: None, next_run: None },
            WorkflowListItem { name: "Implement Feature".into(), slug: "implement".into(), filename: "implement.dot".into(), last_run: Some("4 days ago".into()), schedule: None, next_run: None },
            WorkflowListItem { name: "Sync Drift".into(), slug: "sync_drift".into(), filename: "sync_drift.dot".into(), last_run: Some("1 day ago".into()), schedule: None, next_run: None },
            WorkflowListItem { name: "Expand Product".into(), slug: "expand".into(), filename: "expand.dot".into(), last_run: Some("2 weeks ago".into()), schedule: None, next_run: None },
        ]
    }

    pub fn detail(name: &str) -> Option<WorkflowDetail> {
        let items = [
            WorkflowDetail {
                title: "Fix Build".into(), slug: "fix_build".into(), filename: "fix_build.dot".into(),
                description: "Automatically diagnoses and fixes CI build failures by analyzing error logs, identifying root causes, and applying targeted code changes.".into(),
                config: r#"version = 1
task = "Diagnose and fix CI build failures"
graph = "fix_build.dot"

[llm]
model = "claude-sonnet"

[vars]
repo_url = "https://github.com/org/service"
branch = "main"

[execution]
environment = "daytona"

[execution.daytona.sandbox]
auto_stop_interval = 60

[execution.daytona.sandbox.labels]
project = "fix-build"

[execution.daytona.snapshot]
name = "fix-build-dev"
cpu = 4
memory = 8
disk = 10
"#.into(),
                graph: r#"digraph fix_build {
    graph [
        goal="Diagnose and fix CI build failures",
        label="Fix Build"
    ]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    diagnose [label="Diagnose Failure", prompt="@prompts/fix_build/diagnose.md", reasoning_effort="high"]
    fix      [label="Apply Fix",        prompt="@prompts/fix_build/fix.md"]
    validate [label="Run Build",        prompt="@prompts/fix_build/validate.md", goal_gate=true]
    gate     [shape=diamond,            label="Build passing?"]

    start -> diagnose -> fix -> validate -> gate
    gate -> exit     [label="Yes", condition="outcome=success"]
    gate -> diagnose [label="No",  condition="outcome!=success", max_visits=3]
}
"#.into(),
            },
            WorkflowDetail {
                title: "Implement Feature".into(), slug: "implement".into(), filename: "implement.dot".into(),
                description: "Generates production-ready code from a technical blueprint, including tests, documentation, and a pull request ready for review.".into(),
                config: r#"version = 1
task = "Implement feature from technical blueprint"
graph = "implement.dot"

[llm]
model = "claude-sonnet"

[vars]
spec_path = "specs/feature.md"
test_framework = "vitest"

[setup]
commands = ["bun install", "bun run typecheck"]
timeout_ms = 120000

[execution]
environment = "daytona"

[execution.daytona.sandbox]
auto_stop_interval = 120

[execution.daytona.sandbox.labels]
project = "implement"
team = "engineering"

[execution.daytona.snapshot]
name = "implement-dev"
cpu = 4
memory = 8
disk = 20
"#.into(),
                graph: r#"digraph implement {
    graph [
        goal="",
        label="Implement"
    ]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    strategy [shape=hexagon, label="Choose decomposition strategy:"]

    subgraph cluster_impl {
        label="Implementation Loop"
        node [fidelity="full", thread_id="impl"]

        plan      [label="Plan Implementation", prompt="@prompts/implement/plan.md", reasoning_effort="high"]
        implement [label="Implement",            prompt="@prompts/implement/implement.md"]
        review    [label="Review",               prompt="@prompts/implement/review.md"]
        validate  [label="Validate",             prompt="@prompts/implement/validate.md", goal_gate=true]
        fix       [label="Fix Failures",         prompt="@prompts/implement/fix.md", max_visits=3]
    }

    start -> strategy
    strategy -> plan [label="[L] Layer-by-layer"]
    strategy -> plan [label="[F] Feature slice"]
    strategy -> plan [label="[P] Embarrassingly parallel"]
    strategy -> plan [label="[S] Sequential / linear"]
    plan -> implement -> review -> validate
    validate -> exit [condition="outcome=success"]
    validate -> fix  [condition="outcome!=success", label="Fix"]
    fix -> validate
}
"#.into(),
            },
            WorkflowDetail {
                title: "Sync Drift".into(), slug: "sync_drift".into(), filename: "sync_drift.dot".into(),
                description: "Detects configuration and code drift between environments, then generates reconciliation patches to bring everything back in sync.".into(),
                config: r#"version = 1
task = "Detect and reconcile configuration drift across environments"
graph = "sync_drift.dot"

[llm]
model = "claude-sonnet"

[vars]
source_env = "production"
target_env = "staging"
drift_threshold = "warn"

[execution]
environment = "daytona"

[execution.daytona.sandbox]
auto_stop_interval = 120

[execution.daytona.sandbox.labels]
project = "sync-drift"
team = "platform"

[execution.daytona.snapshot]
name = "sync-drift-dev"
cpu = 2
memory = 4
disk = 10
"#.into(),
                graph: r#"digraph sync {
    graph [
        goal="Detect and resolve drift between product docs, architecture docs, and code",
        label="Sync"
    ]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    detect  [label="Detect Drift",     prompt="@prompts/sync/detect.md", reasoning_effort="high"]
    propose [label="Propose Changes",  prompt="@prompts/sync/propose.md"]
    review  [shape=hexagon,            label="Review Changes"]
    apply   [label="Apply Changes",    prompt="@prompts/sync/apply.md"]

    start -> detect
    detect -> exit    [condition="context.drift_found=false", label="No drift"]
    detect -> propose [condition="context.drift_found=true", label="Drift found"]
    propose -> review
    review -> apply    [label="[A] Accept"]
    review -> propose  [label="[R] Revise"]
    apply -> exit
}
"#.into(),
            },
            WorkflowDetail {
                title: "Expand Product".into(), slug: "expand".into(), filename: "expand.dot".into(),
                description: "Evolves the product by analyzing usage patterns and specifications to propose and implement incremental improvements.".into(),
                config: r#"version = 1
task = "Propose and implement incremental product improvements"
graph = "expand.dot"

[llm]
model = "claude-sonnet"

[vars]
analytics_window = "30d"
min_confidence = "0.8"

[execution]
environment = "daytona"

[execution.daytona.sandbox]
auto_stop_interval = 180

[execution.daytona.sandbox.labels]
project = "expand"
team = "product"

[execution.daytona.snapshot]
name = "expand-dev"
cpu = 2
memory = 4
disk = 10
"#.into(),
                graph: r#"digraph expand {
    graph [
        goal="",
        label="Expand"
    ]
    rankdir=LR

    start [shape=Mdiamond, label="Start"]
    exit  [shape=Msquare, label="Exit"]

    propose [label="Propose Changes",  prompt="@prompts/expand/propose.md", reasoning_effort="high"]
    approve [shape=hexagon,            label="Approve Changes"]
    execute [label="Execute Changes",  prompt="@prompts/expand/execute.md"]

    start -> propose -> approve
    approve -> execute [label="[A] Accept"]
    approve -> propose [label="[R] Revise"]
    execute -> exit
}
"#.into(),
            },
        ];
        items.into_iter().find(|w| w.slug == name)
    }
}

mod verifications {
    use arc_types::*;

    pub fn categories() -> Vec<VerificationCategory> {
        vec![
            VerificationCategory {
                name: "Traceability".into(),
                question: "Do we understand what this change is and why we're making it?".into(),
                controls: vec![
                    VerificationControl { name: "Motivation".into(), slug: "motivation".into(), description: "Origin of proposal identified".into(), type_: Some(VerificationType::Ai), mode: Some(VerificationMode::Active), f1: Some(0.87), pass_at_1: Some(0.82), evaluations: vec![EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Fail, EvaluationResult::Pass, EvaluationResult::Pass] },
                    VerificationControl { name: "Specifications".into(), slug: "specifications".into(), description: "Requirements written down".into(), type_: Some(VerificationType::Ai), mode: Some(VerificationMode::Active), f1: Some(0.83), pass_at_1: Some(0.78), evaluations: vec![EvaluationResult::Pass, EvaluationResult::Fail, EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass] },
                    VerificationControl { name: "Documentation".into(), slug: "documentation".into(), description: "Developer and user docs added".into(), type_: Some(VerificationType::Ai), mode: Some(VerificationMode::Active), f1: Some(0.79), pass_at_1: Some(0.74), evaluations: vec![EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Fail, EvaluationResult::Pass] },
                    VerificationControl { name: "Minimization".into(), slug: "minimization".into(), description: "No extraneous changes".into(), type_: Some(VerificationType::Ai), mode: Some(VerificationMode::Evaluate), f1: Some(0.72), pass_at_1: Some(0.68), evaluations: vec![EvaluationResult::Pass, EvaluationResult::Fail, EvaluationResult::Pass, EvaluationResult::Fail, EvaluationResult::Pass] },
                ],
            },
            VerificationCategory {
                name: "Readability".into(),
                question: "Can a human or agent quickly read this and understand what it does?".into(),
                controls: vec![
                    VerificationControl { name: "Formatting".into(), slug: "formatting".into(), description: "Code layout matches standard".into(), type_: Some(VerificationType::Automated), mode: Some(VerificationMode::Active), f1: Some(0.99), pass_at_1: Some(0.98), evaluations: vec![EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass] },
                    VerificationControl { name: "Linting".into(), slug: "linting".into(), description: "Linter issues resolved".into(), type_: Some(VerificationType::Automated), mode: Some(VerificationMode::Active), f1: Some(0.98), pass_at_1: Some(0.97), evaluations: vec![EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Fail] },
                    VerificationControl { name: "Style".into(), slug: "style".into(), description: "House style applied".into(), type_: Some(VerificationType::Ai), mode: Some(VerificationMode::Active), f1: Some(0.81), pass_at_1: Some(0.76), evaluations: vec![EvaluationResult::Pass, EvaluationResult::Fail, EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Pass] },
                ],
            },
        ]
    }

    pub fn detail(slug: &str) -> Option<VerificationDetailResponse> {
        let control_info = match slug {
            "motivation" => ControlInfo { name: "Motivation".into(), slug: "motivation".into(), description: "Origin of proposal identified".into(), type_: Some(VerificationType::Ai), category: "Traceability".into() },
            "formatting" => ControlInfo { name: "Formatting".into(), slug: "formatting".into(), description: "Code layout matches standard".into(), type_: Some(VerificationType::Automated), category: "Readability".into() },
            _ => return None,
        };

        Some(VerificationDetailResponse {
            control: control_info,
            performance: ControlPerformance {
                mode: VerificationMode::Active,
                f1: Some(0.87),
                pass_at_1: Some(0.82),
                evaluations: vec![EvaluationResult::Pass, EvaluationResult::Pass, EvaluationResult::Fail, EvaluationResult::Pass, EvaluationResult::Pass],
            },
            control_detail: ControlDetail {
                description: "Verifies that every change traces back to a clear origin — whether a ticket, RFC, customer request, or incident.".into(),
                checks: vec!["PR body or linked issue explains why the change is needed".into(), "Commit messages reference a ticket or context".into()],
                pass_example: "PR links to JIRA-1234 and explains the user-facing pain point being resolved.".into(),
                fail_example: "PR description is empty or says only 'fix stuff'.".into(),
            },
            recent_results: vec![
                RecentControlResult { run_id: "run-047".into(), run_title: "PR #312 — Add OAuth2 PKCE flow".into(), workflow: "code_review".into(), result: VerificationStatus::Pass, timestamp: "2h ago".into() },
                RecentControlResult { run_id: "run-046".into(), run_title: "PR #311 — Update rate limiter config".into(), workflow: "code_review".into(), result: VerificationStatus::Pass, timestamp: "5h ago".into() },
            ],
            siblings: vec![
                SiblingControl { name: "Specifications".into(), slug: "specifications".into(), type_: Some(VerificationType::Ai), mode: Some(VerificationMode::Active) },
                SiblingControl { name: "Documentation".into(), slug: "documentation".into(), type_: Some(VerificationType::Ai), mode: Some(VerificationMode::Active) },
            ],
        })
    }
}

mod retros {
    use arc_types::*;

    pub fn list_items() -> Vec<RetroListItem> {
        vec![
            RetroListItem {
                run_id: "run-1".into(), workflow_name: "implement".into(),
                goal: "Add rate limiting to auth endpoints".into(),
                timestamp: "2026-02-28T14:32:00Z".into(),
                smoothness: Some(SmoothnessRating::Smooth),
                stats: RetroStats { total_duration_ms: 389000, total_cost: Some(2.78), total_retries: 0, files_touched: vec!["src/middleware/rate-limit.ts".into()], stages_completed: 4, stages_failed: 0 },
                friction_point_count: 0,
            },
            RetroListItem {
                run_id: "run-2".into(), workflow_name: "implement".into(),
                goal: "Migrate to React Router v7".into(),
                timestamp: "2026-02-28T10:15:00Z".into(),
                smoothness: Some(SmoothnessRating::Bumpy),
                stats: RetroStats { total_duration_ms: 975000, total_cost: Some(6.82), total_retries: 4, files_touched: vec!["src/routes.ts".into()], stages_completed: 4, stages_failed: 0 },
                friction_point_count: 2,
            },
            RetroListItem {
                run_id: "run-6".into(), workflow_name: "implement".into(),
                goal: "Add dark mode toggle".into(),
                timestamp: "2026-02-27T16:45:00Z".into(),
                smoothness: Some(SmoothnessRating::Effortless),
                stats: RetroStats { total_duration_ms: 216000, total_cost: Some(1.51), total_retries: 0, files_touched: vec!["src/components/ThemeToggle.tsx".into()], stages_completed: 3, stages_failed: 0 },
                friction_point_count: 0,
            },
            RetroListItem {
                run_id: "run-3".into(), workflow_name: "fix_build".into(),
                goal: "Fix config parsing for nested values".into(),
                timestamp: "2026-02-27T09:20:00Z".into(),
                smoothness: Some(SmoothnessRating::Struggled),
                stats: RetroStats { total_duration_ms: 830000, total_cost: Some(4.97), total_retries: 4, files_touched: vec!["src/config/parser.ts".into()], stages_completed: 4, stages_failed: 0 },
                friction_point_count: 3,
            },
            RetroListItem {
                run_id: "run-8".into(), workflow_name: "implement".into(),
                goal: "Implement webhook retry logic".into(),
                timestamp: "2026-02-26T11:00:00Z".into(),
                smoothness: Some(SmoothnessRating::Smooth),
                stats: RetroStats { total_duration_ms: 440000, total_cost: Some(3.09), total_retries: 1, files_touched: vec!["src/webhooks/retry.ts".into()], stages_completed: 4, stages_failed: 0 },
                friction_point_count: 1,
            },
        ]
    }
}

mod sessions {
    use arc_types::*;

    pub fn groups() -> Vec<SessionGroup> {
        vec![
            SessionGroup {
                label: "Today".into(),
                sessions: vec![
                    SessionListItem { id: "s1".into(), title: "Add rate limiting to auth endpoints".into(), repo: "api-server".into(), time: "2h ago".into() },
                    SessionListItem { id: "s2".into(), title: "Fix config parsing for nested values".into(), repo: "cli-tools".into(), time: "4h ago".into() },
                ],
            },
            SessionGroup {
                label: "Yesterday".into(),
                sessions: vec![
                    SessionListItem { id: "s3".into(), title: "Migrate to React Router v7".into(), repo: "web-dashboard".into(), time: "1d ago".into() },
                    SessionListItem { id: "s4".into(), title: "Add dark mode toggle".into(), repo: "web-dashboard".into(), time: "1d ago".into() },
                ],
            },
            SessionGroup {
                label: "Previous 7 days".into(),
                sessions: vec![
                    SessionListItem { id: "s6".into(), title: "Terraform module for Redis cluster".into(), repo: "infrastructure".into(), time: "3d ago".into() },
                    SessionListItem { id: "s7".into(), title: "Add workflow run event types".into(), repo: "shared-types".into(), time: "5d ago".into() },
                ],
            },
        ]
    }

    pub fn detail(id: &str) -> Option<SessionDetail> {
        match id {
            "s1" => Some(SessionDetail {
                id: "s1".into(), title: "Add rate limiting to auth endpoints".into(), repo: "api-server".into(), model: "Opus 4.6".into(),
                turns: vec![
                    SessionTurn { kind: SessionTurnKind::User, content: Some("Add rate limiting to the auth endpoints. Use a sliding window approach with Redis, 10 requests per minute per IP.".into()), date: Some("Feb 28".into()), tools: vec![] },
                    SessionTurn { kind: SessionTurnKind::Assistant, content: Some("I'll implement sliding window rate limiting using Redis. Let me first look at the existing auth routes and middleware setup.".into()), date: None, tools: vec![] },
                    SessionTurn { kind: SessionTurnKind::Tool, content: None, date: None, tools: vec![
                        ToolUse { tool_name: "read_file".into(), args: r#"{ "path": "src/routes/auth.ts" }"#.into(), result: "import { Router } from \"express\";\n// ...".into() },
                    ] },
                    SessionTurn { kind: SessionTurnKind::Assistant, content: Some("Good — there's already a Redis client at `src/lib/redis.ts`. I'll create a rate limiting middleware and apply it to the auth routes.".into()), date: None, tools: vec![] },
                ],
            }),
            _ => None,
        }
    }
}

mod insights {
    use arc_types::*;

    pub fn saved_queries() -> Vec<SavedQuery> {
        vec![
            SavedQuery { id: "1".into(), name: "Run duration by workflow".into(), sql: "SELECT workflow_name, AVG(duration_seconds) as avg_duration,\n       COUNT(*) as run_count\nFROM runs\nGROUP BY workflow_name\nORDER BY avg_duration DESC\nLIMIT 20".into() },
            SavedQuery { id: "2".into(), name: "Daily failure rate".into(), sql: "SELECT date_trunc('day', created_at) as day,\n       COUNT(*) FILTER (WHERE status = 'failed') as failures,\n       COUNT(*) as total\nFROM runs\nGROUP BY 1\nORDER BY 1 DESC\nLIMIT 30".into() },
            SavedQuery { id: "3".into(), name: "Top repos by activity".into(), sql: "SELECT repo, COUNT(*) as runs\nFROM runs\nGROUP BY repo\nORDER BY runs DESC".into() },
        ]
    }

    pub fn history() -> Vec<HistoryEntry> {
        vec![
            HistoryEntry { id: "h1".into(), sql: "SELECT workflow_name, COUNT(*) FROM runs GROUP BY 1".into(), timestamp: "2 min ago".into(), elapsed: 0.342, row_count: 6 },
            HistoryEntry { id: "h2".into(), sql: "SELECT * FROM runs WHERE status = 'failed' LIMIT 100".into(), timestamp: "8 min ago".into(), elapsed: 0.127, row_count: 23 },
            HistoryEntry { id: "h3".into(), sql: "SELECT date_trunc('day', created_at) as d, COUNT(*) FROM runs GROUP BY 1".into(), timestamp: "15 min ago".into(), elapsed: 0.531, row_count: 30 },
        ]
    }
}

mod settings {
    use arc_types::*;

    pub fn groups() -> Vec<SettingGroup> {
        vec![
            SettingGroup {
                id: "general".into(), name: "General".into(), description: "Core platform settings and defaults.".into(),
                fields: vec![
                    SettingField { key: "org_name".into(), label: "Organization name".into(), value: "Acme Corp".into(), type_: SettingFieldType::Text, options: vec![], description: None },
                    SettingField { key: "default_branch".into(), label: "Default branch".into(), value: "main".into(), type_: SettingFieldType::Text, options: vec![], description: None },
                    SettingField { key: "timezone".into(), label: "Timezone".into(), value: "America/New_York".into(), type_: SettingFieldType::Select, options: vec!["America/New_York".into(), "UTC".into(), "Europe/London".into()], description: None },
                    SettingField { key: "auto_cancel".into(), label: "Auto-cancel superseded runs".into(), value: "true".into(), type_: SettingFieldType::Toggle, options: vec![], description: None },
                ],
            },
            SettingGroup {
                id: "git".into(), name: "Git & VCS".into(), description: "Version control integration and repository settings.".into(),
                fields: vec![
                    SettingField { key: "github_org".into(), label: "GitHub organization".into(), value: "acme-corp".into(), type_: SettingFieldType::Text, options: vec![], description: None },
                    SettingField { key: "clone_protocol".into(), label: "Clone protocol".into(), value: "SSH".into(), type_: SettingFieldType::Select, options: vec!["SSH".into(), "HTTPS".into()], description: None },
                    SettingField { key: "auto_merge".into(), label: "Auto-merge when checks pass".into(), value: "false".into(), type_: SettingFieldType::Toggle, options: vec![], description: None },
                ],
            },
            SettingGroup {
                id: "compute".into(), name: "Compute".into(), description: "Resource allocation and execution environment.".into(),
                fields: vec![
                    SettingField { key: "default_cpu".into(), label: "Default CPU".into(), value: "4".into(), type_: SettingFieldType::Select, options: vec!["2".into(), "4".into(), "8".into(), "16".into()], description: None },
                    SettingField { key: "default_memory".into(), label: "Default memory".into(), value: "8 GB".into(), type_: SettingFieldType::Select, options: vec!["4 GB".into(), "8 GB".into(), "16 GB".into()], description: None },
                    SettingField { key: "max_parallel".into(), label: "Max parallel runs".into(), value: "10".into(), type_: SettingFieldType::Text, options: vec![], description: None },
                ],
            },
            SettingGroup {
                id: "notifications".into(), name: "Notifications".into(), description: "Alerts and notification delivery preferences.".into(),
                fields: vec![
                    SettingField { key: "notify_on_failure".into(), label: "Notify on failure".into(), value: "true".into(), type_: SettingFieldType::Toggle, options: vec![], description: None },
                    SettingField { key: "notify_on_success".into(), label: "Notify on success".into(), value: "false".into(), type_: SettingFieldType::Toggle, options: vec![], description: None },
                ],
            },
            SettingGroup {
                id: "security".into(), name: "Security".into(), description: "Access control and security policies.".into(),
                fields: vec![
                    SettingField { key: "sso_provider".into(), label: "SSO provider".into(), value: "Okta".into(), type_: SettingFieldType::Select, options: vec!["None".into(), "Okta".into(), "Azure AD".into()], description: None },
                    SettingField { key: "mfa_required".into(), label: "Require MFA".into(), value: "true".into(), type_: SettingFieldType::Toggle, options: vec![], description: None },
                ],
            },
        ]
    }
}

mod projects {
    use arc_types::*;

    pub fn list_items() -> Vec<Project> {
        vec![
            Project { id: "arc-web".into(), name: "arc-web".into() },
            Project { id: "arc-workflows".into(), name: "arc-workflows".into() },
            Project { id: "arc-cli".into(), name: "arc-cli".into() },
        ]
    }

    pub fn branches() -> Vec<Branch> {
        vec![
            Branch { id: "main".into(), name: "main".into() },
            Branch { id: "develop".into(), name: "develop".into() },
            Branch { id: "feature/start-page".into(), name: "feature/start-page".into() },
        ]
    }
}
