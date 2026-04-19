# Remote GitHub Token Request Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a run-scoped GitHub permission request that survives remote-run persistence and drives sandbox `GITHUB_TOKEN` injection for Azure and Daytona workflows.

**Architecture:** Extend `run.scm.github` with a permissions map, resolve it through the existing run settings pipeline, and keep worker/preflight token logic reading that run-owned request from persisted settings. Preserve the existing owner-domain rule that `server.*` remains host-local and inert in workflow/project config during remote-server execution.

**Tech Stack:** Rust, TOML/serde settings layers, Fabro config resolution, workflow persistence, server preflight, workflow start pipeline.

---

## File Map

- Modify: `lib/crates/fabro-types/src/settings/run.rs`
  Purpose: add the run-layer and resolved-layer GitHub permissions fields under `run.scm.github`.
- Modify: `lib/crates/fabro-config/src/resolve/run.rs`
  Purpose: resolve `run.scm.github.permissions` into the resolved run settings.
- Modify: `lib/crates/fabro-config/tests/resolve_run.rs`
  Purpose: cover parsing and resolution of the new run-scoped permissions.
- Modify: `lib/crates/fabro-workflow/src/operations/start.rs`
  Purpose: read resolved run-scoped GitHub permissions and pass them into sandbox env construction.
- Modify: `lib/crates/fabro-workflow/src/operations/create.rs`
  Purpose: assert persisted run settings retain the new subtree after create/materialize.
- Modify: `lib/crates/fabro-server/src/run_manifest.rs`
  Purpose: drive preflight GitHub token checks from the run-scoped request rather than workflow-level `server.*` config.
- Modify: `lib/crates/fabro-server/src/server.rs`
  Purpose: decide whether remote workers need GitHub credentials based on persisted run-scoped permissions.
- Modify: `lib/crates/fabro-workflow/tests/materialize_run.rs`
  Purpose: verify run materialization preserves the run-scoped GitHub request.
- Modify: `lib/crates/fabro-workflow/src/operations/start.rs` tests
  Purpose: verify the start path exposes run-scoped GitHub permissions to sandbox env setup.
- Modify: `.fabro/workflows/gh-triage/workflow.toml`
  Purpose: migrate built-in workflow to the new run-scoped schema.
- Modify: `.fabro/workflows/implement-issue/workflow.toml`
  Purpose: migrate built-in workflow to the new run-scoped schema.
- Modify: `docs/integrations/github.mdx`
  Purpose: document the new run-scoped request path and remove the misleading workflow-level `server.*` example.

### Task 1: Add Run-Scoped GitHub Permissions To Settings Resolution

**Files:**
- Modify: `lib/crates/fabro-types/src/settings/run.rs`
- Modify: `lib/crates/fabro-config/src/resolve/run.rs`
- Test: `lib/crates/fabro-config/tests/resolve_run.rs`

- [ ] **Step 1: Write the failing resolve test**

```rust
#[test]
fn resolves_run_scm_github_permissions() {
    let file = parse(
        r#"
_version = 1

[run.scm.github.permissions]
contents = "write"
issues = "read"
"#,
    );

    let settings = fabro_config::resolve_run_from_file(&file).expect("run settings should resolve");

    let github = settings.scm.github.expect("github scm settings should resolve");
    assert_eq!(
        github.permissions.get("contents").map(InterpString::as_source).as_deref(),
        Some("write")
    );
    assert_eq!(
        github.permissions.get("issues").map(InterpString::as_source).as_deref(),
        Some("read")
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo nextest run -p fabro-config resolve_run_scm_github_permissions`
Expected: FAIL because `ScmGitHubLayer` and `ScmGitHubSettings` do not expose `permissions` yet.

- [ ] **Step 3: Write the minimal settings implementation**

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ScmGitHubSettings {
    pub permissions: HashMap<String, InterpString>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScmGitHubLayer {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub permissions: HashMap<String, InterpString>,
}
```

and in `resolve_scm()`:

```rust
github: scm.github.as_ref().map(|github| ScmGitHubSettings {
    permissions: github.permissions.clone(),
}),
```

- [ ] **Step 4: Run the resolve test to verify it passes**

Run: `cargo nextest run -p fabro-config resolve_run_scm_github_permissions`
Expected: PASS

- [ ] **Step 5: Run the crate tests covering run resolution**

Run: `cargo nextest run -p fabro-config`
Expected: PASS

### Task 2: Preserve The Request In Materialized And Persisted Run Settings

**Files:**
- Modify: `lib/crates/fabro-workflow/tests/materialize_run.rs`
- Modify: `lib/crates/fabro-workflow/src/operations/create.rs`
- Test: `lib/crates/fabro-workflow/tests/materialize_run.rs`

- [ ] **Step 1: Write the failing materialization test**

```rust
#[test]
fn materialize_run_preserves_run_scm_github_permissions() {
    let source = r#"digraph Test {
        graph [goal="Build feature"]
        start [shape=Mdiamond]
        exit  [shape=Msquare]
        start -> exit
    }"#;

    let settings: SettingsLayer = toml::from_str(
        r#"
_version = 1

[run.scm.github.permissions]
contents = "write"
issues = "read"
"#,
    )
    .unwrap();

    let materialized = materialize_run(settings, &graph(source), Catalog::builtin(), &[]);

    let permissions = materialized
        .run
        .as_ref()
        .and_then(|run| run.scm.as_ref())
        .and_then(|scm| scm.github.as_ref())
        .map(|github| github.permissions.clone())
        .unwrap();

    assert_eq!(permissions.get("contents").map(InterpString::as_source).as_deref(), Some("write"));
    assert_eq!(permissions.get("issues").map(InterpString::as_source).as_deref(), Some("read"));
}
```

- [ ] **Step 2: Write the failing persisted-run assertion**

Add a create-path test assertion similar to:

```rust
assert_eq!(
    created
        .persisted
        .run_record()
        .settings
        .run
        .as_ref()
        .and_then(|run| run.scm.as_ref())
        .and_then(|scm| scm.github.as_ref())
        .and_then(|github| github.permissions.get("contents"))
        .map(InterpString::as_source)
        .as_deref(),
    Some("write")
);
```

- [ ] **Step 3: Run the focused workflow tests to verify failure**

Run: `cargo nextest run -p fabro-workflow materialize_run_preserves_run_scm_github_permissions`
Expected: FAIL before the new settings shape is wired through tests and fixtures

- [ ] **Step 4: Update fixtures to include the new run-scoped subtree where needed**

Use TOML-backed or `SettingsLayer`-backed test input that includes:

```rust
settings: toml::from_str::<SettingsLayer>(
    r#"
_version = 1

[run.execution]
mode = "dry_run"

[run.scm.github.permissions]
contents = "write"
"#,
)
.unwrap(),
```

- [ ] **Step 5: Re-run the focused workflow tests**

Run: `cargo nextest run -p fabro-workflow materialize_run_preserves_run_scm_github_permissions`
Expected: PASS

### Task 3: Drive Worker And Preflight Token Logic From Run-Scoped Permissions

**Files:**
- Modify: `lib/crates/fabro-workflow/src/operations/start.rs`
- Modify: `lib/crates/fabro-server/src/run_manifest.rs`
- Modify: `lib/crates/fabro-server/src/server.rs`
- Test: `lib/crates/fabro-workflow/src/operations/start.rs`
- Test: `lib/crates/fabro-server/src/run_manifest.rs`

- [ ] **Step 1: Write the failing worker test**

Add a start-path test that creates a persisted run with:

```toml
_version = 1

[run.execution]
mode = "dry_run"

[run.scm.github.permissions]
contents = "write"
pull_requests = "write"
```

and asserts the resolved sandbox env spec contains:

```rust
assert_eq!(
    sandbox_env.github_permissions
        .as_ref()
        .and_then(|permissions| permissions.get("contents"))
        .map(String::as_str),
    Some("write")
);
```

- [ ] **Step 2: Write the failing preflight test**

Build a prepared manifest from workflow config containing:

```toml
_version = 1

[run.scm.github.permissions]
issues = "read"
```

and assert the GitHub token check is present in the preflight report.

- [ ] **Step 3: Run focused tests to verify they fail**

Run: `cargo nextest run -p fabro-workflow start_ --status-level fail`

Run: `cargo nextest run -p fabro-server github_token --status-level fail`

Expected: FAIL because the runtime still reads `server.integrations.github.permissions`.

- [ ] **Step 4: Implement the minimal runtime changes**

In `start.rs`, replace:

```rust
let github_permissions: Option<HashMap<String, String>> =
    (!resolved_server.integrations.github.permissions.is_empty()).then(|| {
        resolved_server.integrations.github.permissions.iter() ...
    });
```

with logic that reads from `resolved.scm.github`:

```rust
let github_permissions = resolved
    .scm
    .github
    .as_ref()
    .filter(|github| !github.permissions.is_empty())
    .map(|github| {
        github
            .permissions
            .iter()
            .map(|(k, v)| (k.clone(), v.as_source()))
            .collect::<HashMap<_, _>>()
    });
```

In `run_manifest.rs`, drive `run_github_token_check()` from `resolved_run.scm.github.permissions` instead of `resolved_server.integrations.github.permissions`.

In `server.rs`, compute `required_github_credentials` from the persisted resolved run request instead of `github_settings.permissions`.

- [ ] **Step 5: Re-run the focused tests**

Run: `cargo nextest run -p fabro-workflow start_ --status-level fail`

Run: `cargo nextest run -p fabro-server github_token --status-level fail`

Expected: PASS for the new targeted assertions

- [ ] **Step 6: Run broader crate tests for the touched runtime crates**

Run: `ulimit -n 4096 && cargo nextest run -p fabro-workflow -p fabro-server`
Expected: PASS

### Task 4: Migrate Built-In Workflows And Docs

**Files:**
- Modify: `.fabro/workflows/gh-triage/workflow.toml`
- Modify: `.fabro/workflows/implement-issue/workflow.toml`
- Modify: `docs/integrations/github.mdx`

- [ ] **Step 1: Update built-in workflow TOMLs**

Replace:

```toml
[server.integrations.github]

[server.integrations.github.permissions]
pull_requests = "read"
issues = "read"
```

with:

```toml
[run.scm.github.permissions]
pull_requests = "read"
issues = "read"
```

and the analogous `implement-issue` permissions block.

- [ ] **Step 2: Update GitHub docs**

Replace the workflow-scoped example section with a run-scoped example:

```toml title="workflow.toml"
[run.scm.github.permissions]
contents = "write"
pull_requests = "write"
```
```

and clarify:

```md
- `server.integrations.github` configures the server's credential source and strategy
- `run.scm.github.permissions` requests a sandbox token for a run
- workflow/project `server.*` remains owner-specific and inert in remote-server mode
```

- [ ] **Step 3: Run the relevant docs and config tests**

Run: `cargo nextest run -p fabro-config -p fabro-server -p fabro-workflow`
Expected: PASS

- [ ] **Step 4: Manually verify the new built-in TOML shape parses**

Run: `cargo nextest run -p fabro-config resolve_run_scm_github_permissions`
Expected: PASS

## Self-Review

- Spec coverage: this plan covers the new run-scoped schema, persistence, runtime token injection, preflight, built-in workflow migration, and docs migration.
- Placeholder scan: all tasks reference exact files, commands, and target code shapes.
- Type consistency: the plan consistently uses `run.scm.github.permissions` for both layer and resolved settings.
