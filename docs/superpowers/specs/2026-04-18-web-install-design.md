# Web-based install — design

**Status:** approved spec, pre-implementation
**Date:** 2026-04-18
**Owner:** bryan@qlty.ai

## Summary

Add a browser-based installation wizard as an alternative to `fabro install`. When `fabro server` boots without `~/.fabro/settings.toml`, it enters **install mode**: prints a one-time install token, mounts an install-only HTTP router, and serves a wizard from the existing `fabro-web` bundle. On successful completion, the server persists the same on-disk state the CLI install produces and exits cleanly so the supervisor restarts it into normal mode.

This unblocks remote-first deployments (Docker, Railway, VPS) where the operator has no terminal-time access to run `fabro install`. The CLI wizard remains supported.

## Goals

- Enable first-run configuration of a Fabro server entirely from a browser.
- Work for both local (laptop) and remote (PaaS / VPS) deployments under a single mechanism.
- Reach the same on-disk end state as `fabro install`, using the same persistence helpers.
- Keep the install surface transient and isolated: zero install code paths reachable on a configured server.

## Non-goals (v1)

- OAuth-based LLM provider auth. API-key providers (Anthropic API key, OpenAI, Gemini, OpenAI-compatible) only in v1; Codex (OAuth-only via the local `codex` binary) and Anthropic OAuth are explicitly deferred. The CLI `fabro install` continues to support these for users who need them today.
- Resume of a partially-completed install across server restarts.
- Replacing the CLI `fabro install` command — additive, not a replacement.
- Multi-operator coordination during install (two browsers using the same token).
- Automatic deletion of GitHub Apps created on github.com if local persistence then fails.

## Decisions log

The following decisions were made during brainstorming and are load-bearing for the rest of the design:

| # | Decision | Why |
|---|---|---|
| 1 | Support both local and remote deployments | Remote is the use case that motivates this; local is trivial once remote works |
| 2 | Trust = one-time install token, printed to stdout/log on startup | Operator must already have log access; matches Jupyter / Gitea / Vaultwarden prior art |
| 3 | Friendly landing page when token is missing/invalid (not 401) | First-time users will hit `/` before they read the logs; explain instead of error |
| 4 | Regular `fabro server` self-detects unconfigured state and enters install mode | Single mental model: "I started the app, it told me how to finish setup." Required for Docker/Railway. |
| 5 | On wizard completion: persist, respond, exit cleanly with code 0 | Avoids in-process mode transition. Supervisor restarts into normal mode. |
| 6 | Update orchestration configs (Docker Compose, Railway) with restart policies | Required for (5) to work end-to-end |
| 7 | Full parity with CLI install (long-term) | The CLI is the source of truth for "configured." A web flow that diverges is half a feature. |
| 8 | v1 ships API-key LLM providers only | Defers OAuth-based provider flows to v2 |
| 9 | Server config screen detects canonical URL from forwarded headers, prefills, user confirms | Works for any reverse proxy; no per-PaaS detection in the prefill path |
| 10 | Per-step API calls with in-memory server-side install session | Live validation; GitHub App OAuth needs server-side state regardless |
| 11 | Install endpoints in main `fabro-api.yaml`, tagged for exclusion from Mintlify | Reuses progenitor + openapi-generator pipelines and the conformance test |
| 12 | Same bundle (`fabro-web`) hosts install wizard | One codebase, one design system; bundle bloat is negligible |
| 13 | After install, `/install/*` routes return 404 (not 401) | Routes are permanently absent in normal mode, not auth-gated |
| 14 | Detection trigger: absence of `~/.fabro/settings.toml`. Parse errors fail loudly | Matches `install.rs:1647`; one-line check; no ambiguity |
| 15 | Refactor shared persistence into a new `fabro-install` crate | Avoids circular deps; both `fabro-cli` and `fabro-server` depend on it |

## Architecture

### Process model

`fabro server` (and the `serve` library entrypoint) gains a startup precheck that runs after CLI/env arg parsing and before the main router is built:

1. Resolve the path to `~/.fabro/settings.toml` (honoring `--storage-dir` / env overrides).
2. If it exists → parse and proceed with the existing `serve::run` path. Parse errors fail loudly as today.
3. If it does **not** exist → enter install mode:
   - Generate a one-time install token (`ring::rand::SystemRandom`, 32 bytes, base64url, no padding).
   - Print the token + a fully formed install URL to stderr (visible in `journalctl`, `docker logs`, Railway logs).
   - Create an in-memory `InstallSession` registry holding a single `PendingInstall`.
   - Build an install-only router (`build_install_router`) mounting: `/install/*` API endpoints, the static-asset routes serving the existing `fabro-web` bundle, a `GET /health` endpoint returning `{ "mode": "install" }`, and a catch-all that returns the SPA `index.html`. Normal `/api/v1/*` routes are not mounted.
   - Bind to whatever the boot-time bind config dictates (env var, default `0.0.0.0` in containers / `127.0.0.1` for local).

When `POST /install/finish` succeeds, the handler persists outputs to disk, returns 202, then schedules a clean process exit (~500ms after responding). The supervisor restarts the process; the new process finds `settings.toml` present and boots into normal mode.

**No mode-transition inside a running process.** Avoiding hot-swap means we don't reason about half-mounted routers, in-flight install requests during shutdown, or partial state after a failed finalize. Process exit is the boundary.

### Crate placement

A new `fabro-install` crate holds:
- `PendingInstall` and `InstallSession` types
- TOML-merging primitives currently in `install.rs:164–299` (`merge_server_settings`, `write_token_settings`, `write_github_app_settings`, etc.)
- `persist_install_outputs` and supporting helpers (currently `install.rs:1335`)
- `setup_github_app` refactored to be transport-agnostic (no embedded callback server)
- Generation of secrets (session secret, JWT keypair, dev token)

`fabro-cli` and `fabro-server` both depend on `fabro-install`. The CLI's `commands::install` becomes thin glue around the new crate; `fabro-server::install` adds the install-mode router and handlers that drive the same primitives.

## Trust model

### Token generation and surfacing

On entering install mode, the server generates a single 32-byte token, base64url-encoded. Stored only in memory; never persisted.

At startup, the server logs to stderr:

```
  ⚒️  Fabro server is unconfigured — install mode active.

  Open this URL in your browser to finish setup:
    https://fabro.up.railway.app/install?token=8H_K2…

  Or visit the root path for the install token instructions.
```

The startup-log URL is best-effort: it uses env-var detection (Railway's `RAILWAY_PUBLIC_DOMAIN`, etc.) falling back to the bind address. The operator may already know their public hostname out-of-band; this is just a hint.

### Landing page

`GET /` (or any non-`/install/*`, non-asset path) with no/invalid token serves the SPA shell. The React install router renders an "Unconfigured server — find your token" page that explains:

- where to find the token (Docker logs, Railway logs, terminal output)
- example commands (`docker logs <container> | grep "Open this URL"`)
- a textarea to paste the token if the operator prefers not to use the URL

No 401, no error — a friendly explainer.

### Token validation

Every `/install/*` API call requires the token. Three places it can come from, in order:

1. `Authorization: Bearer <token>` header (used by the frontend after first load)
2. `?token=` query param (used for the initial human paste-and-go)
3. `X-Install-Token` header

A middleware checks the token against the in-memory session registry; mismatch returns 401 with a generic "invalid install token" body.

### Token lifecycle

**Session-lifetime, not strictly single-use.** The token is valid for any `/install/*` call until either (a) `/install/finish` succeeds and the process exits, or (b) the process restarts (which generates a fresh token). Strict single-use would force a re-auth handshake after every step; the token is already a high-entropy secret behind a transient process.

### CSRF and URL hygiene

- Install API endpoints don't accept cookies; they require `Authorization` header. Cross-origin form POSTs cannot set this. No CSRF token needed.
- The query-param token will appear in browser history and access logs. Acceptable: (i) the same token is already in server logs so the observability surface is symmetric; (ii) the token dies the moment install completes; (iii) URL-fragment alternatives add complexity for marginal benefit.
- The frontend strips the token from the URL after first load (`history.replaceState`) so screenshots / back-button don't expose it.

### Canonical URL detection

The server-config wizard step uses live request headers (the user is connected at that point):

1. `X-Forwarded-Proto` + `X-Forwarded-Host` if present (Railway, Caddy, nginx, fly-proxy)
2. Else `Host` header + request scheme

Basic syntactic validation (valid hostname[:port]). The result is prefilled into an editable form field; the operator confirms.

## Wizard flow

Linear flow with a sidebar showing progress. Users can back up to any prior completed step. All state lives in the server-side session, so refresh / back-button / network blip recover via `GET /install/session`.

### Screens

| # | Path | Purpose |
|---|---|---|
| 1 | `/install` | Landing / token entry. If query-param token validates → redirect to (2). Else: explainer page with paste-token textarea. |
| 2 | `/install/welcome` | One-paragraph "here's what we'll do" + Next. |
| 3 | `/install/llm` | Multi-select Anthropic / OpenAI / Gemini / OpenAI-compatible. Per-provider sub-form for API key. Each key live-validated against the provider's `/models` endpoint before advancing. |
| 4 | `/install/github` | Choose **App** or **Token**. Token path: paste PAT, validate via `GET /user`, capture username. App path: collect owner + display name → server builds manifest → user redirected to `https://github.com/settings/apps/new` (or org equivalent) → GitHub posts back to `/install/github/app/callback`. |
| 5 | `/install/server` | Single screen with prefilled canonical URL (from forwarded headers) and confirm/edit field. Listen address is read-only display. |
| 6 | `/install/review` | Read-only summary of every choice. "Install" button → `POST /install/finish`. |
| 7 | `/install/finishing` | Server: writes settings.toml, server.env, vault, dev token, then schedules process exit ~500ms after responding. Client renders the dev token (returned in the `/install/finish` response body) for the operator's records, then polls `GET /health` until the response no longer reports `"mode": "install"`, and redirects to the confirmed canonical URL. Includes a "if this takes more than 30s, check your supervisor logs" fallback. |

### Navigation rules

- Session tracks which steps are complete.
- Sidebar lets users re-enter any completed step to edit.
- "Next" submits to the per-step endpoint and advances on 200; errors render inline.
- No "Save and exit" — only ways out are finish or process termination.

### Generated secrets

Session secret, JWT keypair, dev token are produced server-side at `/install/finish` time. Never exposed to the client. The dev token is shown to the user on the completion screen for their records (they may want it for `fabro` CLI auth against the server later).

## API surface

Install endpoints live in `docs/api-reference/fabro-api.yaml` under a dedicated `install` tag. Mintlify exclusion via the existing tag-filter mechanism.

All endpoints require the install-token middleware. All under `/install`.

| Method & path | Purpose | Notable request/response |
|---|---|---|
| `GET /install/session` | Returns the current `PendingInstall` snapshot — completed steps, recorded values (with secrets redacted), prefill data (canonical URL from forwarded headers, etc.). Frontend calls on mount to rehydrate. | Resp: `InstallSession` |
| `POST /install/llm/test` | Validates an LLM API key against the provider. Does not persist. | Req: `{ provider, api_key }`, Resp: `{ ok, error? }` |
| `PUT /install/llm` | Records the chosen providers + keys into the session. | Req: `LlmProvidersInput` |
| `POST /install/github/token/test` | Validates a GitHub PAT via `GET /user`, returns username. | Req: `{ token }`, Resp: `{ username }` |
| `PUT /install/github/token` | Records the PAT + username. | Req: `GithubTokenInput` |
| `POST /install/github/app/manifest` | Builds the GitHub App manifest, returns the manifest JSON + the GitHub URL the client redirects to. Stores expected callback `state` in session. | Req: `{ owner, app_name }`, Resp: `{ manifest, github_url }` |
| `GET /install/github/app/callback` | OAuth callback target. Validates `state`, exchanges code for app credentials, stores in session, redirects browser to `/install/github/done`. | Query: `code`, `state` |
| `PUT /install/server` | Records canonical URL confirmation. | Req: `ServerConfigInput` |
| `POST /install/finish` | Validates session is complete, persists outputs, schedules clean exit, returns 202. | Resp: `{ status: "completing", restart_url }` |

### Why per-step `PUT` plus separate `test`

The validation calls (`POST /…/test`) make outbound network requests to the LLM provider / GitHub and shouldn't be conflated with state mutation. Lets the UI test multiple keys, fix typos, then commit. Aligns with how the CLI install today separates `authenticate_provider` from the recording step.

### Routing wiring

`build_install_router()` in `fabro_server::install::router` returns an `axum::Router` that mounts the table above plus the install-token middleware. The startup precheck calls `build_install_router()` (install mode) or the existing `build_router()` (normal mode); never both.

## Persistence parity

The web wizard produces **identical** on-disk state to the CLI install, using the same persistence helpers extracted into `fabro-install`:

- `~/.fabro/settings.toml` — server config, auth methods, GitHub integration strategy
- `<storage_dir>/server.env` — `FABRO_JWT_PRIVATE_KEY`, `FABRO_JWT_PUBLIC_KEY`, `SESSION_SECRET`, `FABRO_DEV_TOKEN`, plus GitHub App env pairs if applicable
- `<storage_dir>/secrets/…` — vault entries for LLM API keys and (if Token strategy) `GITHUB_TOKEN`
- `<storage_dir>/server-state/dev-token` — the dev token file
- artifact store metadata stamped with `FABRO_VERSION`

The `/install/finish` handler is essentially the back half of `run_install_inner` (`install.rs:1925–:2061`) with the input source replaced by the in-memory `PendingInstall`.

### `setup_github_app` refactor

The CLI version (`install.rs:1064`) spins up its own callback HTTP server because no other server is running at install time. The web version doesn't need that — the install-mode server is already running and exposes the callback at `/install/github/app/callback`. The refactor extracts the manifest-building, code-exchange, and credential-recording logic into transport-agnostic functions; the CLI keeps its embedded callback server, the web flow uses the install router.

## Error handling and edge cases

### Per-step validation errors

Bad API key, GitHub PAT lacks scopes, invalid hostname, etc. → 422 with structured error body the UI renders inline next to the offending field. Session unchanged.

### `/install/finish` failure

Load-bearing error case. The CLI's `persist_install_outputs` already tracks `previous_contents` for settings.toml and restores it on failure (`install.rs:1362` `restore_optional_file`). The web flow uses the same machinery.

On any persistence error:
- Local files (settings.toml, server.env, vault) are best-effort rolled back.
- Handler returns 500 with the error message.
- **Process does NOT exit.** Install mode stays up so the user can retry without losing the in-memory session.

The dangerous edge: the GitHub App may already exist on github.com (real external side effect that local rollback can't undo). The error surface tells the user this and gives a "delete the App and retry" path. v1 does not auto-delete — GitHub's API for that requires the very credentials we just failed to persist.

### GitHub App callback timeout

User starts the App-creation flow on github.com, closes the tab, never returns. Session holds `pending_github_app: { state, expires_at }` for ~10 min. After expiry, the UI lets them retry (generates a fresh `state`). A late-arriving callback hits an unknown-`state` 400.

### OAuth `state` CSRF protection

`POST /install/github/app/manifest` generates a 32-byte `state` value, stores it in the session, includes it in the GitHub redirect URL. `/install/github/app/callback` validates `state` matches before doing anything. Closes the open-redirect / CSRF angle on the only GET endpoint that mutates session state.

### Process restart mid-install

In-memory session is gone; new install token printed. User starts over. Acceptable because (a) install is short, (b) restarts shouldn't happen often, (c) supporting resume requires persisting partial install state — the surface we're explicitly keeping transient.

### Two operators with the same token

Both can load the wizard. They share the session; whoever submits a step last wins. The server logs a warning when a request's `User-Agent` or remote IP differs from the session's first-seen values but doesn't block. v1 acceptable.

### Health endpoint during install

`GET /health` returns 200 with `{ "mode": "install" }`. Required so supervisors (Docker, Railway) know the container is healthy and shouldn't restart it. Once normal mode boots, the same path returns the existing health response.

### `/api/v1/*` access during install

Install-mode router does not mount it. Any `/api/v1/*` request returns 404. Prevents partial-state queries against an unconfigured server.

### Logging

Per `docs-internal/logging-strategy.md` (re-read before implementing):

- `info!`: entering install mode (URL with token redacted in structured log fields, full URL only in the human-readable boot stderr message), each step completion (no values), finish success, scheduled exit.
- `warn!`: validation failures, suspected concurrent-operator activity.
- `error!`: persistence failures.

### Telemetry

Install-mode failures report to Sentry via `fabro-telemetry` with the existing anonymous-ID conventions. Successful installs emit a "first-run completed" telemetry event.

## Testing strategy

Per `files-internal/testing-strategy.md` (re-read before implementing).

### Unit tests (`fabro-install` crate)

- `merge_*` / `write_*_settings` TOML transforms (port from existing `install.rs` tests).
- `PendingInstall` state machine: which steps are required/optional, what counts as complete.
- Install-token middleware: header / query / `X-Install-Token` precedence; reject on mismatch; reject when no session exists.
- OAuth `state` generation and validation.

### Integration tests (`fabro-server/tests/it/install/`)

- **Spec/router conformance.** Existing fabro-server conformance test covers install endpoints automatically.
- **End-to-end happy path with stub providers.** Boot a server with temp empty `~/.fabro`, follow the per-step API: test LLM key → record → test GitHub PAT → record → finish. Assert resulting `settings.toml`, `server.env`, vault contents match snapshot. Use `httpmock` for upstream calls.
- **Mode detection.** Server boots without `settings.toml` → install mode (install endpoints respond, `/api/v1/*` 404s). With `settings.toml` → normal mode (install endpoints 404, `/api/v1/*` works).
- **Parse-error fail-loud.** Boot with malformed `settings.toml` → server exits with parse error, does NOT fall back to install mode.
- **Token rejection.** All install endpoints called without token → 401. Wrong token → 401. Valid token → 200/422.
- **GitHub App `state` validation.** Callback called with mismatched `state` → 400, session unchanged.
- **Finish failure rollback.** Force a vault write to fail; assert `settings.toml` is restored and process does not exit.
- **Forwarded-host detection.** Request with `X-Forwarded-Host: foo.com` + `X-Forwarded-Proto: https` → `GET /install/session` returns prefilled canonical URL `https://foo.com`.

### Frontend tests (`apps/fabro-web`, Bun test)

- Install router renders the correct screen for each session state.
- Token-from-URL extraction + `history.replaceState`.
- Per-step form validation.
- Polling logic for the post-finish "waiting for restart" screen.

### Manual smoke test (documented in implementation plan)

Process-exit behavior is too OS-flaky for CI. One manual test: run `fabro server start` with empty home, complete the wizard, assert process exits with 0, assert a fresh `fabro server start` boots into normal mode.

### E2E live tests

None for v1. Real GitHub App creation against github.com is out of scope for automated CI; the `setup_github_app` extraction means the manifest-building logic is unit-testable.

### Snapshot tests for wizard screens

Deferred. Design isn't stable enough to lock in screenshots.

## Orchestration config updates

For (5) — clean exit + supervisor restart — to work end-to-end:

- **`compose.yml`:** add `restart: unless-stopped` to the fabro service (verify it's present; add if missing).
- **Railway:** verify the service restart policy. Railway restarts crashed processes by default; confirm exit code 0 also triggers a restart (may require setting a restart policy explicitly).
- **Documentation:** the install-mode boot stderr should mention "the server will restart automatically" so operators expect the disconnect.

## Open questions

None at spec time. All design questions resolved during brainstorming.
