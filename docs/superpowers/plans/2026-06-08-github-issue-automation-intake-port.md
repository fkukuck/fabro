# GitHub Issue Automation Intake Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the fork's GitHub issue intake feature to upstream Fabro as automation-based `github_issue` triggers, excluding the legacy `github.toml` implementation and all Azure/fork-specific changes.

**Architecture:** GitHub issue intake is an automation trigger type. The existing signed GitHub webhook route receives `issues` events, matches enabled automation triggers for the target repository and labels, materializes a run with issue inputs and source metadata, starts it, records trigger-cycle state for idempotency, and optionally comments on the issue.

**Tech Stack:** Rust crates (`fabro-automation`, `fabro-types`, `fabro-store`, `fabro-github`, `fabro-server`, `fabro-api`), OpenAPI (`docs/public/api-reference/fabro-api.yaml`), generated TypeScript client (`lib/packages/fabro-api-client`), React web app (`apps/fabro-web`), `cargo nextest`, `bun`.

---

## Scope Boundaries

- Port only automation-based issue intake from `/Users/ipt/repos/agentic-factory`.
- Do not port legacy `github.toml`, `github_issue_config.rs`, `github_issue_orchestrator.rs`, or `fabro-store/src/slate/github_issue_triggers.rs`.
- Do not port Azure install/deploy/runtime, ipt branding, sandboxd, hosted sandbox, or generated-client churn caused by fork-only Azure schemas.
- Create a new branch in `/Users/ipt/repos/fabro-fkukuck` before implementation.
- Do not commit unless the user explicitly asks; use diffs and verification checkpoints instead.

## File Map

- Modify `lib/crates/fabro-automation/src/model.rs`: add `GithubIssueTrigger`, enum variant, validation, and tests.
- Modify `lib/crates/fabro-automation/src/lib.rs`: export `GithubIssueTrigger`.
- Modify `lib/crates/fabro-types/src/run.rs`: add `RunSourceContext` and `GithubIssueRunSource`, plus `RunSpec.source_context`.
- Modify `lib/crates/fabro-types/src/run_event/run.rs`: add `RunCreatedProps.source_context`.
- Modify `lib/crates/fabro-types/src/run_summary.rs`: add `Run.source_context`.
- Modify `lib/crates/fabro-types/src/run_projection.rs`: project source context from run-created events.
- Modify `lib/crates/fabro-types/src/lib.rs`: export source context types.
- Modify affected tests under `lib/crates/fabro-types/tests/` to add default `source_context: None` and serde coverage.
- Create `lib/crates/fabro-store/src/slate/automation_trigger_runs.rs`: persistent trigger-cycle and delivery dedupe state.
- Modify `lib/crates/fabro-store/src/slate/mod.rs` and `lib/crates/fabro-store/src/lib.rs`: expose trigger run store.
- Modify `lib/crates/fabro-store/Cargo.toml`: add `percent-encoding` if missing.
- Modify `lib/crates/fabro-github/src/lib.rs`: add `create_issue_comment` and tests.
- Create `lib/crates/fabro-server/src/automation_runner.rs`: shared automation materialize/create/start helper.
- Create `lib/crates/fabro-server/src/github_issue_events.rs`: parse issue webhook payloads and build run inputs.
- Create `lib/crates/fabro-server/src/github_issue_automation_trigger.rs`: match issue events, fire automations, record trigger runs, and post comments.
- Modify `lib/crates/fabro-server/src/lib.rs`: expose new server modules.
- Modify `lib/crates/fabro-server/src/server.rs`: attach `AppState` to webhook route state and dispatch issue events after signature verification.
- Modify `lib/crates/fabro-server/src/server/handler/automations.rs`: use `automation_runner` for manual/API automation runs.
- Modify `lib/crates/fabro-server/src/server/automation_scheduler.rs`: use `automation_runner` for scheduled automation runs.
- Modify `lib/crates/fabro-server/src/automation_materializer.rs`: support input overrides and title override for issue runs.
- Modify run creation call sites in `lib/crates/fabro-server/src/server/handler/runs.rs` and any compile-reported call sites: accept and persist `source_context`.
- Modify `docs/public/api-reference/fabro-api.yaml`: add `AutomationGithubIssueTrigger`, `RunSourceContext`, `GithubIssueRunSource`, and `source_context` fields.
- Modify `lib/crates/fabro-api/build.rs`: replace API schemas with shared Rust types.
- Modify `lib/crates/fabro-api/src/lib.rs` and tests under `lib/crates/fabro-api/tests/`: expose and prove type identity/round trips.
- Regenerate `lib/packages/fabro-api-client/src/` from the upstream OpenAPI spec.
- Modify `apps/fabro-web/app/lib/automation.ts`: add `findGithubIssueTrigger`.
- Modify `apps/fabro-web/app/components/automation-form.tsx`: add issue trigger form fields.
- Modify `apps/fabro-web/app/data/runs.ts`, `apps/fabro-web/app/routes/runs.tsx`, and `apps/fabro-web/app/routes/run-detail/header.tsx`: surface GitHub issue source links.
- Modify web tests under `apps/fabro-web/app/routes/*.test.tsx`: cover create form and issue source links.

## Task 1: Prepare Vanilla Branch And Baseline

**Files:**
- Read-only check: `/Users/ipt/repos/fabro-fkukuck`

- [ ] **Step 1: Confirm clean state and current branch**

Run:

```bash
git status --short
git branch --show-current
git log --oneline -5
```

Expected: no unexpected modified tracked files. If untracked files exist, leave them alone and proceed only if they do not overlap this plan.

- [ ] **Step 2: Create the feature branch**

Run:

```bash
git switch -c github-issue-automation-intake
```

Expected: branch switches to `github-issue-automation-intake`.

- [ ] **Step 3: Run targeted baseline checks**

Run:

```bash
cargo nextest run -p fabro-automation -p fabro-types -p fabro-store -p fabro-github -p fabro-server -p fabro-api
```

Expected: PASS. If baseline fails, capture the failing tests and stop before changing code.

## Task 2: Add Automation Trigger Model

**Files:**
- Modify: `lib/crates/fabro-automation/src/model.rs`
- Modify: `lib/crates/fabro-automation/src/lib.rs`

- [ ] **Step 1: Add failing model serde tests**

Add tests in the existing `#[cfg(test)] mod tests` in `lib/crates/fabro-automation/src/model.rs`:

```rust
#[test]
fn github_issue_trigger_round_trips() {
    let trigger = AutomationTrigger::GithubIssue(GithubIssueTrigger {
        id: AutomationTriggerId::new("github-issue").unwrap(),
        enabled: true,
        trigger_label: "fabro".to_string(),
        issue_label: Some("Bug".to_string()),
        comment: true,
    });

    let value = toml::Value::try_from(&trigger).unwrap();
    let parsed: AutomationTrigger = value.try_into().unwrap();

    assert_eq!(parsed, trigger);
}

#[test]
fn github_issue_trigger_defaults_comment_to_true() {
    let parsed: AutomationTrigger = toml::from_str(
        r#"
        type = "github_issue"
        id = "github-issue"
        enabled = true
        trigger_label = "fabro"
        "#,
    )
    .unwrap();

    assert_eq!(
        parsed,
        AutomationTrigger::GithubIssue(GithubIssueTrigger {
            id: AutomationTriggerId::new("github-issue").unwrap(),
            enabled: true,
            trigger_label: "fabro".to_string(),
            issue_label: None,
            comment: true,
        })
    );
}
```

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo nextest run -p fabro-automation github_issue_trigger
```

Expected: FAIL because `AutomationTrigger::GithubIssue` and `GithubIssueTrigger` do not exist.

- [ ] **Step 3: Implement trigger model**

In `lib/crates/fabro-automation/src/model.rs`, add:

```rust
fn default_true() -> bool {
    true
}
```

Add the variant:

```rust
pub enum AutomationTrigger {
    Api(ApiTrigger),
    Schedule(ScheduleTrigger),
    GithubIssue(GithubIssueTrigger),
}
```

Update `Automation::enabled_github_issue_triggers`:

```rust
pub fn enabled_github_issue_triggers(&self) -> impl Iterator<Item = &GithubIssueTrigger> {
    self.triggers.iter().filter_map(move |trigger| match trigger {
        AutomationTrigger::GithubIssue(trigger) if trigger.enabled => Some(trigger),
        _ => None,
    })
}
```

Update `AutomationTrigger::id` and `AutomationTrigger::enabled` match arms to include `Self::GithubIssue(trigger)`.

Add the struct after `ScheduleTrigger`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubIssueTrigger {
    pub id:            AutomationTriggerId,
    pub enabled:       bool,
    pub trigger_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_label:   Option<String>,
    #[serde(default = "default_true")]
    pub comment:       bool,
}
```

In validation, keep existing duplicate-trigger-id and multiple-API-trigger behavior. Do not add special restrictions for GitHub issue triggers beyond existing trigger ID validation.

In `lib/crates/fabro-automation/src/lib.rs`, export `GithubIssueTrigger` with the other model exports.

- [ ] **Step 4: Run tests and verify pass**

Run:

```bash
cargo nextest run -p fabro-automation
```

Expected: PASS.

## Task 3: Add Run Source Context Types

**Files:**
- Modify: `lib/crates/fabro-types/src/run.rs`
- Modify: `lib/crates/fabro-types/src/run_event/run.rs`
- Modify: `lib/crates/fabro-types/src/run_summary.rs`
- Modify: `lib/crates/fabro-types/src/run_projection.rs`
- Modify: `lib/crates/fabro-types/src/lib.rs`
- Modify: `lib/crates/fabro-types/tests/run_spec_serde.rs`
- Modify: `lib/crates/fabro-types/tests/run_spec_methods.rs`
- Modify: `lib/crates/fabro-types/tests/run_event_serde.rs`

- [ ] **Step 1: Add failing serde tests**

Add a test to `lib/crates/fabro-types/tests/run_spec_serde.rs` using an existing minimal `RunSpec` fixture and assert this JSON fragment round-trips:

```json
"source_context": {
  "type": "github_issue",
  "repository": "owner/repo",
  "issue_number": 42,
  "issue_title": "Fix bug",
  "issue_url": "https://github.com/owner/repo/issues/42"
}
```

Add a test to `lib/crates/fabro-types/tests/run_event_serde.rs` asserting `RunCreatedProps` accepts the same `source_context` object.

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo nextest run -p fabro-types source_context
```

Expected: FAIL because source context types and fields do not exist.

- [ ] **Step 3: Implement types and projection fields**

In `lib/crates/fabro-types/src/run.rs`, add before `DirtyStatus`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunSourceContext {
    GithubIssue(GithubIssueRunSource),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubIssueRunSource {
    pub repository:   String,
    pub issue_number: u64,
    pub issue_title:  String,
    pub issue_url:    String,
}
```

Add this field to `RunSpec` after `automation`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub source_context: Option<RunSourceContext>,
```

In `lib/crates/fabro-types/src/run_event/run.rs`, import `RunSourceContext` and add this field to `RunCreatedProps` after `automation`:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub source_context: Option<RunSourceContext>,
```

In `lib/crates/fabro-types/src/run_summary.rs`, add `source_context: Option<RunSourceContext>` after `automation` and import the type.

In `lib/crates/fabro-types/src/run_projection.rs`, initialize `source_context: None` in default summaries and set `summary.source_context = props.source_context.clone()` when projecting `RunCreated`.

In `lib/crates/fabro-types/src/lib.rs`, export `GithubIssueRunSource` and `RunSourceContext`.

- [ ] **Step 4: Update all compile-reported literals**

For every `RunSpec`, `RunCreatedProps`, or `Run` literal reported by the compiler, add:

```rust
source_context: None,
```

Place it next to the `automation` field where possible.

- [ ] **Step 5: Run tests and verify pass**

Run:

```bash
cargo nextest run -p fabro-types
```

Expected: PASS.

## Task 4: Add Trigger Run Persistence

**Files:**
- Create: `lib/crates/fabro-store/src/slate/automation_trigger_runs.rs`
- Modify: `lib/crates/fabro-store/src/slate/mod.rs`
- Modify: `lib/crates/fabro-store/src/lib.rs`
- Modify: `lib/crates/fabro-store/Cargo.toml`

- [ ] **Step 1: Add failing store tests**

In the new file, include tests copied and adapted from the fork's `automation_trigger_runs.rs` that verify:

```rust
decide_github_issue_start starts first delivery;
decide_github_issue_start returns DuplicateDelivery for the same delivery;
decide_github_issue_start returns AlreadyOpen while the issue cycle marker is open;
close_github_issue_cycle allows a later delivery to start a new cycle;
record_started stores the run_id on the open marker;
```

Use the fork file as the source of exact test bodies and keep imports adjusted to upstream paths.

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
cargo nextest run -p fabro-store automation_trigger_runs
```

Expected: FAIL until the module is exported and dependencies are wired.

- [ ] **Step 3: Implement persistence module**

Copy the fork implementation from `/Users/ipt/repos/agentic-factory/lib/crates/fabro-store/src/slate/automation_trigger_runs.rs` into upstream with these public types:

```rust
pub struct AutomationTriggerRunRecord { ... }
pub enum AutomationTriggerKind { GithubIssue }
pub enum AutomationEventSource { Github }
pub enum AutomationTriggerRunStatus { Started, FailedToStart }
pub enum AutomationTriggerRunContext { GithubIssue { repository: String, issue_number: u64, trigger_label: String, issue_label: Option<String> } }
pub struct GithubIssueTriggerCycleKey { ... }
pub struct AutomationTriggerRunStore { pub(super) db: super::Database }
```

Include the methods:

```rust
decide_github_issue_start;
record_started;
record_failed_to_start;
close_github_issue_cycle;
records;
```

In `lib/crates/fabro-store/src/slate/mod.rs`, add:

```rust
mod automation_trigger_runs;

pub use automation_trigger_runs::{
    AutomationEventSource, AutomationTriggerKind, AutomationTriggerRunContext,
    AutomationTriggerRunRecord, AutomationTriggerRunStatus, AutomationTriggerRunStore,
    GithubIssueTriggerCycleKey,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriggerStartDecision {
    Start { trigger_cycle: u64 },
    AlreadyOpen { run_id: Option<RunId> },
    DuplicateDelivery,
}
```

Add an `automation_trigger_runs` `OnceCell` field to `Database`, initialize it in `Database::new`, and add:

```rust
pub async fn automation_trigger_runs(&self) -> Result<Arc<AutomationTriggerRunStore>> {
    self.automation_trigger_runs
        .get_or_init(|| async {
            Arc::new(AutomationTriggerRunStore { db: self.clone() })
        })
        .await
        .clone()
        .into()
}
```

In `lib/crates/fabro-store/src/lib.rs`, re-export the same public types plus `TriggerStartDecision`.

In `lib/crates/fabro-store/Cargo.toml`, add:

```toml
percent-encoding = { workspace = true }
```

- [ ] **Step 4: Run tests and verify pass**

Run:

```bash
cargo nextest run -p fabro-store automation_trigger_runs
cargo nextest run -p fabro-store
```

Expected: PASS.

## Task 5: Add GitHub Issue Comment Helper

**Files:**
- Modify: `lib/crates/fabro-github/src/lib.rs`

- [ ] **Step 1: Add failing helper test**

Port the fork test `create_issue_comment_posts_body` from `/Users/ipt/repos/agentic-factory/lib/crates/fabro-github/src/lib.rs` into upstream.

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo nextest run -p fabro-github create_issue_comment_posts_body
```

Expected: FAIL because `create_issue_comment` does not exist.

- [ ] **Step 3: Implement helper**

Add after repository file-fetch helpers:

```rust
/// Create a GitHub issue comment with an installation or PAT bearer token.
pub async fn create_issue_comment(
    client: &impl HttpClient,
    token: &str,
    owner: &str,
    repo: &str,
    issue_number: u64,
    body: &str,
    base_url: &str,
) -> anyhow::Result<()> {
    let url = format!("{base_url}/repos/{owner}/{repo}/issues/{issue_number}/comments");
    let auth = format!("Bearer {token}");
    let body = serde_json::json!({ "body": body });
    let resp = client
        .request(HttpMethod::Post, &url, &github_headers(&auth), Some(&body))
        .await
        .with_context(|| {
            format!("failed to create issue comment on {owner}/{repo}#{issue_number}")
        })?;

    match resp.status {
        200 | 201 => Ok(()),
        status => bail!(
            "Unexpected status {status} creating issue comment on {owner}/{repo}#{issue_number}: {}",
            resp.text()
        ),
    }
}
```

- [ ] **Step 4: Run tests and verify pass**

Run:

```bash
cargo nextest run -p fabro-github create_issue_comment_posts_body
cargo nextest run -p fabro-github
```

Expected: PASS.

## Task 6: Centralize Automation Run Firing

**Files:**
- Create: `lib/crates/fabro-server/src/automation_runner.rs`
- Modify: `lib/crates/fabro-server/src/lib.rs`
- Modify: `lib/crates/fabro-server/src/server/handler/automations.rs`
- Modify: `lib/crates/fabro-server/src/server/automation_scheduler.rs`
- Modify: `lib/crates/fabro-server/src/automation_materializer.rs`

- [ ] **Step 1: Add failing automation runner test**

Port the fork test `fire_automation_run_creates_started_run_with_automation_ref` into the new `automation_runner.rs`. Include `source_context` in the test so this helper proves issue source metadata flows to created run summaries.

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
cargo nextest run -p fabro-server fire_automation_run_creates_started_run_with_automation_ref
```

Expected: FAIL because `automation_runner` does not exist.

- [ ] **Step 3: Implement `automation_runner`**

Copy the fork's `lib/crates/fabro-server/src/automation_runner.rs` and adapt imports to upstream. `FireAutomationRunInput` must contain:

```rust
pub(crate) automation: Automation,
pub(crate) trigger_id: AutomationTriggerId,
pub(crate) actor: Principal,
pub(crate) headers: HeaderMap,
pub(crate) input_overrides: HashMap<String, toml::Value>,
pub(crate) title_override: Option<String>,
pub(crate) source_context: Option<RunSourceContext>,
```

`fire_automation_run` must materialize, create via upstream `create_run_from_manifest`, and call `queue_run_start`.

- [ ] **Step 4: Add input and title overrides to materializer**

In `AutomationRunMaterializeInput`, add:

```rust
pub input_overrides: HashMap<String, toml::Value>,
pub title_override: Option<String>,
```

In `build_manifest_from_checkout`, construct `ManifestBuildInput` with:

```rust
input_overrides: input.input_overrides.clone(),
```

After manifest build, apply title override:

```rust
if let Some(title) = input.title_override.clone() {
    manifest.title = Some(title.try_into().map_err(|err| {
        AutomationRunMaterializeError::Manifest(format!("invalid automation run title: {err}"))
    })?);
}
```

When overrides are present, set submitted manifest args:

```rust
if !submitted_input_overrides.is_empty() {
    manifest.args = Some(ManifestArgs {
        input: input_overrides_as_args(&submitted_input_overrides),
        ..ManifestArgs::default()
    });
}
```

Add helper functions `input_overrides_as_args` and `input_override_arg_value` from the fork.

- [ ] **Step 5: Replace manual and scheduled automation firing**

In `handler/automations.rs`, replace inline materialize/create/start code with `fire_automation_run` and pass empty overrides, no title override, and no source context.

In `automation_scheduler.rs`, replace inline scheduled materialize/create/start code with `fire_automation_run` and pass empty overrides, no title override, and no source context.

- [ ] **Step 6: Update compile-reported materializer inputs**

For every `AutomationRunMaterializeInput` literal, add:

```rust
input_overrides: HashMap::new(),
title_override: None,
```

- [ ] **Step 7: Run tests and verify pass**

Run:

```bash
cargo nextest run -p fabro-server fire_automation_run_creates_started_run_with_automation_ref
cargo nextest run -p fabro-server automation
```

Expected: PASS.

## Task 7: Add GitHub Issue Webhook Automation Handling

**Files:**
- Create: `lib/crates/fabro-server/src/github_issue_events.rs`
- Create: `lib/crates/fabro-server/src/github_issue_automation_trigger.rs`
- Modify: `lib/crates/fabro-server/src/lib.rs`
- Modify: `lib/crates/fabro-server/src/server.rs`
- Modify: `lib/crates/fabro-server/src/server/tests.rs`
- Modify: `lib/crates/fabro-server/src/test_support.rs` if test injection needs existing fork helpers

- [ ] **Step 1: Add event parser tests**

Port the fork tests `parses_issue_labeled_payload` and `builds_v1_workflow_inputs` into `github_issue_events.rs`.

- [ ] **Step 2: Run parser tests and verify failure**

Run:

```bash
cargo nextest run -p fabro-server github_issue_events
```

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Implement event parser**

Copy the fork file `lib/crates/fabro-server/src/github_issue_events.rs` unchanged except for import formatting required by upstream.

- [ ] **Step 4: Add webhook integration tests**

Port the fork server tests covering:

```rust
github_issue_labeled_webhook_starts_matching_automation;
github_issue_labeled_webhook_ignores_pull_requests;
github_issue_labeled_webhook_dedupes_open_issue_cycle;
github_issue_unlabeled_webhook_closes_issue_cycle;
github_issue_start_failure_posts_failure_comment;
```

If exact test names differ in the fork, keep upstream names descriptive and assert these outcomes:

```rust
created run has automation id and trigger id;
created run title equals issue title;
created run source_context is GithubIssue;
materializer input_overrides include github_issue_title and github_issue_url;
duplicate labeled delivery does not create a second run;
unlabeled event allows a later labeled event to create a new run;
comment request body is sent when comment is true;
```

- [ ] **Step 5: Run webhook tests and verify failure**

Run:

```bash
cargo nextest run -p fabro-server github_issue
```

Expected: FAIL because webhook dispatch and trigger handling are not implemented.

- [ ] **Step 6: Implement issue automation trigger handler**

Copy the fork file `lib/crates/fabro-server/src/github_issue_automation_trigger.rs`, adapting only imports and upstream names. Preserve behavior:

```rust
issues:labeled starts matching automations;
issues:unlabeled closes open cycles;
pull requests are ignored;
matching requires automation.target.repository == event.repository.full_name;
matching requires changed label == trigger.trigger_label;
optional trigger.issue_label must be present on the issue;
source_context is RunSourceContext::GithubIssue;
title_override is issue title;
input_overrides are built from GithubIssueRunInputs;
comments use issues:write token and fabro_github::create_issue_comment;
```

- [ ] **Step 7: Wire webhook route state**

In `server.rs`, change webhook route state from only `Arc<[u8]>` to:

```rust
#[derive(Clone)]
struct GithubWebhookRouteState {
    secret: Arc<[u8]>,
    app_state: Arc<AppState>,
}
```

Change `github_webhook_routes` to accept `app_state: Arc<AppState>` and `secret: Arc<[u8]>`, and call it from router construction with the existing app state. After successful signature verification and logging, call:

```rust
crate::github_issue_automation_trigger::handle_github_issue_webhook(
    Arc::clone(&route_state.app_state),
    headers,
    delivery_id,
    &body,
)
.await;
```

Return `StatusCode::OK` regardless of whether issue automation handling starts a run; webhook delivery was accepted after HMAC verification.

- [ ] **Step 8: Run server tests and verify pass**

Run:

```bash
cargo nextest run -p fabro-server github_webhook
cargo nextest run -p fabro-server github_issue
cargo nextest run -p fabro-server automation
```

Expected: PASS.

## Task 8: Expose API Schemas And Regenerate Clients

**Files:**
- Modify: `docs/public/api-reference/fabro-api.yaml`
- Modify: `lib/crates/fabro-api/build.rs`
- Modify: `lib/crates/fabro-api/src/lib.rs`
- Modify: `lib/crates/fabro-api/tests/automation_round_trip.rs`
- Modify: `lib/crates/fabro-api/tests/run_projection_round_trip.rs`
- Modify: `lib/crates/fabro-api/tests/run_summary_round_trip.rs`
- Regenerate: `lib/packages/fabro-api-client/src/**`

- [ ] **Step 1: Add failing API tests**

In `automation_round_trip.rs`, add a GitHub issue trigger to the existing automation fixture:

```rust
AutomationTrigger::GithubIssue(fabro_automation::GithubIssueTrigger {
    id: AutomationTriggerId::new("github-issue").unwrap(),
    enabled: true,
    trigger_label: "fabro".to_string(),
    issue_label: Some("Bug".to_string()),
    comment: true,
})
```

In run projection and summary round-trip tests, include a `RunSourceContext::GithubIssue` fixture and assert JSON parity.

- [ ] **Step 2: Run API tests and verify failure**

Run:

```bash
cargo nextest run -p fabro-api automation_round_trip run_projection_round_trip run_summary_round_trip
```

Expected: FAIL because OpenAPI lacks the schemas and replacements.

- [ ] **Step 3: Edit OpenAPI**

Add `AutomationGithubIssueTrigger` schema, add it to the `AutomationTrigger` discriminator mapping, add `RunSourceContext` and `GithubIssueRunSource`, and add optional `source_context` to `Run` and `RunSpec`. Use the fork's `docs/public/api-reference/fabro-api.yaml` as the reference, copying only these schemas and fields.

- [ ] **Step 4: Add Rust API replacements**

In `lib/crates/fabro-api/build.rs`, add replacements:

```rust
(
    "AutomationGithubIssueTrigger",
    "fabro_automation::GithubIssueTrigger",
    &[],
),
("RunSourceContext", "fabro_types::RunSourceContext", &[]),
```

In `lib/crates/fabro-api/src/lib.rs`, expose generated/shared types as needed for tests.

- [ ] **Step 5: Build API and run tests**

Run:

```bash
cargo build -p fabro-api
cargo nextest run -p fabro-api
```

Expected: PASS.

- [ ] **Step 6: Regenerate TypeScript client**

Run:

```bash
bun run generate
```

Working directory: `lib/packages/fabro-api-client`.

Expected: generated files include `automation-github-issue-trigger.ts`, `github-issue-run-source.ts`, `run-source-context.ts`, updated `automation-trigger.ts`, `run.ts`, and `run-spec.ts`. Generated diffs must not introduce Azure-only schemas from the fork.

## Task 9: Add Web UI Support

**Files:**
- Modify: `apps/fabro-web/app/lib/automation.ts`
- Modify: `apps/fabro-web/app/components/automation-form.tsx`
- Modify: `apps/fabro-web/app/routes/automations-new.test.tsx`
- Modify: `apps/fabro-web/app/data/runs.ts`
- Modify: `apps/fabro-web/app/routes/runs.tsx`
- Modify: `apps/fabro-web/app/routes/run-detail/header.tsx`
- Modify: `apps/fabro-web/app/routes/runs.test.tsx`

- [ ] **Step 1: Add failing form test**

Port the fork test `creates automation with GitHub issue trigger` into `automations-new.test.tsx`. Assert the submitted trigger is:

```ts
{
  id: "github-issue",
  type: "github_issue",
  enabled: true,
  trigger_label: "fabro",
  issue_label: "Bug",
  comment: true,
}
```

- [ ] **Step 2: Run test and verify failure**

Run:

```bash
bun test app/routes/automations-new.test.tsx
```

Working directory: `apps/fabro-web`.

Expected: FAIL because the form lacks issue trigger fields.

- [ ] **Step 3: Implement automation form fields**

In `automation.ts`, add:

```ts
export function findGithubIssueTrigger(
  automation: Automation,
): TriggerOfType<"github_issue"> | undefined {
  return automation.triggers.find(
    (t): t is TriggerOfType<"github_issue"> => t.type === "github_issue",
  );
}
```

In `automation-form.tsx`, add form values:

```ts
githubIssueEnabled: boolean;
githubIssueTriggerLabel: string;
githubIssueIssueLabel: string;
githubIssueComment: boolean;
```

Use defaults:

```ts
githubIssueEnabled: false,
githubIssueTriggerLabel: "fabro",
githubIssueIssueLabel: "",
githubIssueComment: true,
```

Add trigger serialization:

```ts
if (values.githubIssueEnabled) {
  const issueLabel = values.githubIssueIssueLabel.trim();
  triggers.push({
    id:            "github-issue",
    type:          "github_issue",
    enabled:       true,
    trigger_label: values.githubIssueTriggerLabel.trim(),
    issue_label:   issueLabel === "" ? null : issueLabel,
    comment:       values.githubIssueComment,
  });
}
```

Add validation:

```ts
(!values.githubIssueEnabled || values.githubIssueTriggerLabel.trim() !== "")
```

Add UI rows under Triggers for GitHub issues, trigger label, optional issue label, and comment switch, matching the fork implementation.

- [ ] **Step 4: Add failing source link tests**

Port fork `runs.test.tsx` assertions that GitHub issue source context renders a link to the issue in run lists and run detail headers.

- [ ] **Step 5: Implement source issue links**

Port fork changes in:

```text
apps/fabro-web/app/data/runs.ts
apps/fabro-web/app/routes/runs.tsx
apps/fabro-web/app/routes/run-detail/header.tsx
```

The UI should link to `run.source_context.issue_url` when `run.source_context.type === "github_issue"` and show issue number/title in the established upstream visual style.

- [ ] **Step 6: Run web checks**

Run:

```bash
bun test app/routes/automations-new.test.tsx app/routes/runs.test.tsx
bun run typecheck
```

Working directory: `apps/fabro-web`.

Expected: PASS.

## Task 10: Full Verification And Diff Review

**Files:**
- Review all changed files in `/Users/ipt/repos/fabro-fkukuck`

- [ ] **Step 1: Run Rust formatting**

Run:

```bash
cargo +nightly-2026-04-14 fmt --all
```

Expected: completes successfully and formats only intended Rust files.

- [ ] **Step 2: Run targeted Rust tests**

Run:

```bash
cargo nextest run -p fabro-automation -p fabro-types -p fabro-store -p fabro-github -p fabro-server -p fabro-api
```

Expected: PASS.

- [ ] **Step 3: Run web checks**

Run:

```bash
bun test
bun run typecheck
```

Working directory: `apps/fabro-web`.

Expected: PASS.

- [ ] **Step 4: Verify no excluded legacy or Azure code was ported**

Run:

```bash
git diff --name-only
git diff --name-only | grep -E 'azure|sandboxd|github_issue_config|github_issue_orchestrator|github_issue_triggers|github\.toml' || true
```

Expected: no matches for excluded paths or legacy implementation files.

- [ ] **Step 5: Review final diff**

Run:

```bash
git diff --stat
git diff
```

Expected: diff contains only automation-based GitHub issue intake, API/client updates generated from upstream, tests, and no Azure/fork-only code.

## Self-Review

- Spec coverage: covered trigger model, source metadata, trigger persistence, GitHub comments, server webhook dispatch, API/OpenAPI, TS client, web UI, and verification.
- Exclusions: explicitly excluded legacy `github.toml` files and Azure/fork-only code.
- Placeholder scan: no deferred implementation sections remain; each task names files, test commands, and concrete implementation content.
- Type consistency: trigger type is consistently `github_issue`; run source context type is consistently `RunSourceContext::GithubIssue` / `type: "github_issue"`.
