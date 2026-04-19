# Remote GitHub Token Request Design

## Problem

Remote runs that expect sandbox `GITHUB_TOKEN` from workflow-declared GitHub permissions fail under `--server`, even when the workflow file declares:

```toml
[server.integrations.github.permissions]
contents = "write"
issues = "read"
pull_requests = "write"
```

The persisted run record for a failing Azure run showed no usable GitHub permission request in `record.settings`, so the worker never injected `GITHUB_TOKEN` and `gh` fell back to interactive auth.

That missing persisted state is only the visible symptom. The earlier root cause is the settings model itself:

- in remote-server mode, `workflow.toml` and `.fabro/project.toml` have their `server.*` domain stripped intentionally
- `server.*` is owner-specific and only trusted from the server host's local `~/.fabro/settings.toml`
- therefore workflow-level `server.integrations.github.permissions` never become effective for remote runs in the first place

This means the current built-in examples are misleading for remote-server execution. They may appear to work in local contexts, but they do not express a valid run-scoped request that a remote worker can persist and honor.

## Goal

Provide a proper run-owned way for workflows and projects to request sandbox GitHub permissions, so remote workers can inject `GITHUB_TOKEN` from persisted run settings without relaxing the owner-domain trust boundary.

## Non-Goals

This design does not change:

- where Fabro's actual GitHub credentials come from
- GitHub App vs token strategy selection
- clone-time authenticated remote URL rewriting
- Azure- or Daytona-specific auth workarounds

The server host still owns GitHub credentials. The workflow should only be able to request a scoped token for the run.

## Recommended Scope

Add a run-scoped GitHub permission request surface and wire the existing token-minting path to read it from persisted run settings.

Recommended schema:

```toml
[run.scm.github.permissions]
contents = "write"
issues = "read"
pull_requests = "write"
```

This keeps the change minimal by extending an existing run-owned GitHub namespace instead of weakening `server.*` ownership rules or inventing a server fallback.

## Approaches Considered

### 1. Add run-scoped GitHub permissions under `run.scm.github.permissions`

Workflows and projects request sandbox GitHub token scope through the run domain, and remote workers persist and consume that request from `record.settings`.

Pros:

- preserves the intended trust boundary that `server.*` is owner-only
- makes remote runs self-contained because the request survives in persisted settings
- fits the existing run-owned SCM namespace with a small schema change
- keeps worker behavior deterministic across Azure and Daytona

Cons:

- requires updating built-in workflows and docs from the old pattern

### 2. Stop stripping `server.integrations.github.permissions` in remote-server mode

Allow workflow/project `server.*` GitHub permissions to remain effective for remote runs.

Pros:

- small user-facing migration

Cons:

- violates the current settings ownership model
- reopens the broader question of which other `server.*` fields workflows are allowed to influence
- conflicts with the documented rule that workflow/project `server.*` is schema-valid but runtime-inert

### 3. Fall back to server-local GitHub permissions only

Require all sandbox token permissions to be declared only in the server host's local `~/.fabro/settings.toml`.

Pros:

- simplest runtime behavior

Cons:

- workflows cannot declare their own least-privilege token needs
- persisted runs are less informative because the request is hidden in host-local config
- does not fix the misleading built-in workflow examples

## Design

### Configuration Model

Add a new run-owned permission request map on `run.scm.github`.

Expected TOML shape:

```toml
[run.scm.github.permissions]
contents = "write"
issues = "read"
pull_requests = "write"
```

Behavior:

- the map is optional
- an empty or missing map means no sandbox `GITHUB_TOKEN` injection is requested
- the map layers like other `run.*` configuration and therefore can come from `workflow.toml`, `.fabro/project.toml`, or server defaults if desired

The server's GitHub strategy, app identifiers, and secrets remain under `server.integrations.github` and stay owner-only.

### Materialization and Persistence

No special-case persistence path should be introduced.

Instead:

- the new run-owned permission request must survive ordinary settings layering and run materialization
- the resulting request must appear in `record.settings`
- remote workers must continue reading only the persisted run record to decide whether sandbox token injection is needed

This keeps the run record self-contained and avoids coupling worker behavior to mutable host-local config.

### Token Minting and Injection

The worker flow stays structurally the same:

- resolve persisted run settings
- read the run-scoped GitHub permission request from `record.settings`
- if permissions are present, use the server host's configured GitHub credentials to mint or provide a token
- inject that token as `GITHUB_TOKEN` into the sandbox environment

Credential ownership remains unchanged:

- token strategy returns the configured token directly
- app strategy mints a scoped installation access token using the requested permissions and the repo origin URL

### Preflight Behavior

Preflight should check the new run-scoped request path instead of relying on workflow-level `server.integrations.github.permissions`.

That means:

- workflows declaring `run.scm.github.permissions` show a GitHub token check during preflight
- workflows using only workflow-level `server.integrations.github.permissions` in remote-server mode should no longer be treated as a valid request path

### Built-in Workflow and Docs Migration

Update built-in workflows such as `gh-triage` and `implement-issue` to use the new run-scoped field.

Update docs to reflect the actual model:

- `server.integrations.github` configures the server's credential source and strategy
- `run.scm.github.permissions` requests a sandbox token for a specific workflow/run
- workflow/project `server.*` remains owner-specific and inert in remote-server mode

## Testing

Add focused tests that fail before the change and pass after it.

1. Settings parse/resolve test
   - parse a settings layer containing `run.scm.github.permissions`
   - verify the resolved run settings retain the permission map

2. Persisted run record test
   - create a run from workflow/project settings that declare `run.scm.github.permissions`
   - verify `created.persisted.run_record().settings` keeps that subtree

3. Worker token request test
   - start from a persisted run record containing `run.scm.github.permissions`
   - verify the start path resolves non-empty GitHub permissions for sandbox env injection

4. Preflight test
   - verify remote preflight reports a GitHub token check when `run.scm.github.permissions` are present

5. Regression test for owner-domain stripping
   - verify workflow/project `server.*` remains inert in remote-server mode
   - confirm the new behavior comes from the run domain, not from weakening ownership rules

## Risks and Mitigations

### Risk: choosing the wrong namespace

Mitigation:

- use `run.scm.github` because it already exists as the run-owned GitHub surface
- keep the addition minimal by adding only the permissions map needed for token requests

### Risk: docs and built-in workflows stay inconsistent

Mitigation:

- update the built-in workflow TOMLs in the same change
- update the GitHub integration docs to remove the old workflow-level `server.*` example

### Risk: worker behavior still depends on host-local state in surprising ways

Mitigation:

- keep only credential source and strategy on the host-local server config
- keep the run's token request inside persisted run settings

## Success Criteria

- a workflow-declared `[run.scm.github.permissions]` block appears in `run.json` after `fabro store dump`
- remote Azure and Daytona runs receive sandbox `GITHUB_TOKEN` without interactive `gh auth login`
- built-in workflows use the new run-scoped field instead of workflow-level `server.integrations.github.permissions`
- workflow/project `server.*` remains inert in remote-server mode
