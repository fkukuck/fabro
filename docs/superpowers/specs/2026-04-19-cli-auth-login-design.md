# `fabro auth login` — CLI authentication via GitHub OAuth

**Status:** Draft for review
**Date:** 2026-04-19
**Author:** Bryan Helmkamp (with Claude)

## Problem

Today the CLI authenticates to the Fabro server with a single shared `fabro_dev_*` secret ("dev token"). Dev tokens work for local development but cannot be used on servers where `server.auth.methods` omits `dev-token`. When only `github` is enabled, the CLI has no way to authenticate: the existing GitHub OAuth flow mints an encrypted session cookie, which is browser-only.

We need a way for the CLI to obtain per-user credentials against a server that accepts only GitHub OAuth logins. This manifests as a new `fabro auth login` subcommand, plus `logout` and `status`.

## Goals

- CLI users can log in to a server via GitHub OAuth without leaving the terminal.
- Credentials are per-user, per-device, server-side revocable, and short-lived on the wire.
- The existing dev-token flow is preserved unchanged.
- The identity model is forward-compatible with Google Workspace and GitHub Enterprise Server.
- Zero new TOML configuration in v1.

## Non-goals (v1)

- Device flow (`--device`). Fast-follow PR; same token model, different exchange endpoint.
- OS keychain storage. Plain 0600 file matches the existing `~/.fabro/dev-token` pattern.
- Rate limiting. Out of scope entirely; operators may add a reverse-proxy-level limiter.
- Per-device session management UI (`fabro auth sessions`).
- Emergency per-user kick independent of allowlist removal.
- Configurable token TTLs via TOML.
- RS256/ES256 signing.
- Rename of `SESSION_SECRET` / `ServerAuthMethod::Github`.
- Full Google Workspace, GHES, per-IdP allowlist, audit tables, service accounts.

## Architecture overview

`fabro auth login` implements **OAuth 2.0 Authorization Code Grant with PKCE** between the CLI and the Fabro server, with GitHub acting as the upstream IdP via the existing web OAuth flow. The server issues a **short-lived HS256 JWT access token (10 min)** plus an **opaque, single-use, rotating refresh token (sliding 30 d)**. Refresh tokens persist in a new SlateDB keyspace; authorization codes live in an in-memory TTL map on the server.

CLI credentials live in a single `~/.fabro/auth.json` keyed by normalized server URL, mode 0600.

Identity is the OIDC-style composite `(idp_issuer, idp_subject)`, future-proof for Google Workspace and GHES. `login` is display-only.

No new auth primitives on the wire: bearer credentials ride `Authorization: Bearer <token>`; web sessions continue to ride the `Cookie` header. The existing `jwt_auth.rs` extractor keeps reading session cookies where it already does, and the bearer-header validation is extended to distinguish (by prefix) dev-token vs JWT access token.

**Canonical origin.** All browser-visible CLI-flow URLs (`/auth/cli/start`, `/auth/cli/resume`, the chained `/auth/login/github` → `/auth/callback/github`) run against `server.web.url`, not against whatever `--server` URL the CLI was given. The existing OAuth flow is hard-coded to redirect to `{server.web.url}/auth/callback/github` (`web_auth.rs:339`), and `fabro_oauth_state` / the new `fabro_cli_flow` cookies are cookie-jar-bound to that origin. If the CLI target differs (e.g. `--server http://127.0.0.1:3000` with `server.web.url=https://fabro.example.com`, or a Unix-socket target), the browser flow still runs on `server.web.url`. The CLI discovers this origin via a new unauthenticated preflight endpoint `GET /api/v1/auth/cli/config` (see Server endpoints), called over the CLI's normal transport. The non-browser step (POST `/auth/cli/token`, `/auth/cli/refresh`, `/auth/cli/logout`) continues to use the CLI's normal transport and can therefore target a Unix socket.

**Web mode and config validation.** `/auth/*` only mounts when `server.web.enabled = true` (`server.rs:916-918`). The config-validity matrix:

- `auth.methods = [..., "github", ...]` **and** `web.enabled = false` → **startup error**. These two settings contradict each other; the admin must pick one.
- `auth.methods` excludes `"github"` (regardless of `web.enabled`) → server starts normally. CLI login is not available; preflight `GET /api/v1/auth/cli/config` returns `enabled: false`.
- `web.enabled = false` with `"github"` also absent → server starts; `/auth/cli/*` routes don't mount; preflight (mounted on the always-available API router) returns `enabled: false`.
- `web.enabled = true` and `auth.methods` includes `"github"` → everything mounted; preflight returns `enabled: true`.

The CLI always hits the preflight first and fails fast with an actionable message rather than opening a doomed browser.

**Browser-to-CLI error handoff.** Server-side failures during the browser flow (invalid PKCE params, ineligible session, allowlist rejection at callback) need to reach the terminal without devolving into a loopback-listener timeout. The contract, matching OAuth 2.0 RFC 6749 §4.1.2.1:

- **When the CLI-supplied `redirect_uri` and `state` have passed validation** (loopback host, `/callback` path, well-formed `state`): errors redirect to `redirect_uri?error=<code>&error_description=<text>&state=<state>`. The CLI's loopback handler treats a request carrying `error` as a terminal failure, surfaces `<error_description>` to the user, and exits non-zero. `state` must match the stored value or the CLI treats it as attacker-injected and ignores.
- **When `redirect_uri` or `state` have not validated** (we cannot trust them as a redirect target): server renders a plain HTML error page in the browser. CLI eventually times out with a generic message. This is accepted degradation — these cases are CLI-authored bugs, not user-recoverable states.

All `/auth/cli/start` and `/auth/cli/resume` error paths follow this contract. `/auth/cli/token`, `/refresh`, `/logout` are direct CLI→server JSON calls and return flat OAuth-style errors (no redirect involved).

**New crates:** none beyond `hkdf` (server) and `fs2` (CLI). `jsonwebtoken`, `sha2`, `uuid`, `open`, `url`, `cookie`, and SlateDB are all already in the workspace.

### Modules

**Server (new):**

- `lib/crates/fabro-server/src/auth/cli_flow.rs` — `/auth/cli/*` HTTP handlers
- `lib/crates/fabro-server/src/auth/jwt.rs` — HS256 encode/verify, HKDF subkey derivation, claims types
- `lib/crates/fabro-server/src/auth/refresh_store.rs` — `RefreshToken` type, `RefreshTokenStore` trait, SlateDB implementation
- `lib/crates/fabro-server/src/auth/code_store.rs` — `AuthCode` type, in-memory store with reaper task
- `lib/crates/fabro-server/src/auth/mod.rs` — `AuthServices` bundle re-exported as Axum extension

**Server (modified):**

- `web_auth.rs` — add `return_to` query param on `/auth/login/github` (strict whitelist); migrate `SessionCookie` from `provider_id: Option<i64>` to `idp_issuer: String, idp_subject: String`; bump cookie version (old cookies force re-login).
- `jwt_auth.rs` — extend extractor to accept JWT bearers alongside dev tokens and session cookies; add `CredentialSource::JwtAccessToken` variant.
- `server.rs` — register new routes; wire `AuthServices`.

**CLI (new):**

- `lib/crates/fabro-cli/src/commands/auth/mod.rs` — `AuthNamespace`, `AuthCommand` enum, dispatch
- `lib/crates/fabro-cli/src/commands/auth/login.rs` — browser + loopback + token exchange
- `lib/crates/fabro-cli/src/commands/auth/logout.rs` — POST `/auth/cli/logout`, clear local entry
- `lib/crates/fabro-cli/src/commands/auth/status.rs` — read local state, format output
- `lib/crates/fabro-cli/src/auth_store.rs` — `AuthStore`, `AuthEntry`, `ServerUrl` normalization, file-locked read/write

**CLI (modified):**

- `server_client.rs` — bearer priority (env `FABRO_DEV_TOKEN` → `AuthStore` JWT → dev-token fallback); auto-refresh on `access_token_expired` 401; persist rotated tokens.

### Settings and secrets

**Zero TOML changes.** JWT issuance is implicitly enabled when `server.auth.methods` includes `github`.

**No new environment variables.** The `SESSION_SECRET` env var is treated as a master symmetric secret from which per-purpose subkeys are derived via HKDF-SHA256:

- `cookie-subkey = HKDF-Expand(SESSION_SECRET, "fabro-cookie-v1", 64)` — used by the `cookie` crate (matches current behavior).
- `jwt-subkey = HKDF-Expand(SESSION_SECRET, "fabro-jwt-hs256-v1", 32)` — used for HS256 signing.

The `-v1` suffix reserves a rotation path: future claim-shape or algorithm changes bump the label, rotating every token without touching `SESSION_SECRET`.

## Identity model

Canonical identity: the OIDC-style pair `(idp_issuer, idp_subject)`.

- `idp_issuer`: `"https://github.com"` for github.com; `"https://github.acme.com"` for GHES; `"https://accounts.google.com"` for Google Workspace (future).
- `idp_subject`: stored as `String`. Numeric string for GitHub (e.g. `"12345"`), OIDC `sub` for Google.
- `login`: display-only. Never used as a key. May change over time (GitHub renames).

Derived canonical subject string used as JWT `sub` claim: `<idp_issuer>#<idp_subject>` (e.g. `"https://github.com#12345"`).

`SessionCookie` is migrated to the same fields. Cookie version bumps; existing web sessions force re-login (acceptable given 30d TTL).

## Token model

### Access token (JWT, HS256)

- Algorithm: **HS256** only. Header-level rejection of any other `alg` (prevents `alg: none` and algorithm-confusion attacks).
- Signing key: HKDF-derived subkey per above.
- Lifetime: **10 min** (code constant; not configurable in v1).

Claims:

```json
{
  "iss": "https://fabro.example.com",
  "aud": "fabro-cli",
  "sub": "https://github.com#12345",
  "exp": 1713543600,
  "iat": 1713543000,
  "jti": "<uuid-v4>",
  "idp_issuer": "https://github.com",
  "idp_subject": "12345",
  "login": "bhelmkamp",
  "name": "Bryan Helmkamp",
  "email": "bryan@qlty.ai",
  "auth_method": "github"
}
```

- `iss` is the server's own public URL; verified on every request.
- `aud` is the fixed string `"fabro-cli"`. Reserves `"fabro-web"` for a future JWT-based web session.
- `jti` is present for future blocklist support; unused in v1.
- `name` and `email` are login-time snapshots; frozen until explicit re-login.

Verification sequence (in `jwt_auth.rs`):

1. Parse header. Reject if `alg != "HS256"`.
2. Verify signature with HKDF-derived subkey.
3. Validate `exp > now`; allow `iat <= now + 30s` clock skew.
4. Validate `iss` equals the server's own URL.
5. Validate `aud == "fabro-cli"`.
6. Build `VerifiedAuth { login, auth_method, credential_source: JwtAccessToken, idp_issuer, idp_subject }`.

**Error envelope on protected endpoints.** 401/403 responses on protected routes use the existing `ApiError` envelope (`error.rs:66`) — `{"errors": [{"status": "401", "title": "...", "detail": "...", "code": "..."}]}` — extended with an optional machine-readable `code` field. The new codes used by this design:

- `access_token_expired` — the JWT `exp` claim has passed; CLI should refresh and retry once.
- `access_token_invalid` — signature, issuer, or audience check failed; CLI must not retry.
- `unauthorized` — no credential present; CLI falls through its bearer-priority chain.

`code` is additive and backwards-compatible — existing callers that inspect only `status`/`title`/`detail` keep working. `fabro-api.yaml` is updated to document the field and the specific codes.

**Error envelope on OAuth token endpoints.** The three direct-JSON endpoints (`/auth/cli/token`, `/auth/cli/refresh`, `/auth/cli/logout`) return RFC 6749-shaped flat errors: `{"error": "<code>", "error_description": "<human>"}`. This matches what every off-the-shelf OAuth client library expects when talking to a token endpoint. Codes: `invalid_request`, `invalid_code`, `pkce_verification_failed`, `redirect_uri_mismatch`, `refresh_token_expired`, `refresh_token_revoked`, `github_session_required`, `unauthorized`, `github_auth_not_configured`, `server_error`.

**Error envelope on browser-flow endpoints.** `/auth/cli/start` and `/auth/cli/resume` never return JSON error bodies — their errors flow through either (a) a 302 redirect to `redirect_uri?error=<code>&error_description=<text>&state=<state>` when `redirect_uri`+`state` have validated, or (b) a plain HTML error page otherwise. See "Browser-to-CLI error handoff" in the Architecture overview.

### Refresh token (opaque)

- Format: `fabro_refresh_<base64url(32 bytes from OsRng)>` — 256 bits of entropy. Prefix is for log redaction / bearer dispatch, not security.
- Lifetime: **30 days sliding** — `expires_at = last_used_at + 30d` on each rotation.
- Single-use with rotation on every `/auth/cli/refresh` call.
- Stored hashed only (SHA-256). Never stored in cleartext.

`RefreshToken` entity (domain type, not suffixed `Row`):

```rust
struct RefreshToken {
    token_hash:    [u8; 32],       // SHA-256 of the presented secret
    chain_id:      Uuid,           // v4, shared across rotation chain
    idp_issuer:    String,
    idp_subject:   String,
    login:         String,         // snapshot at login
    name:          String,
    email:         String,
    issued_at:     DateTime<Utc>,
    expires_at:    DateTime<Utc>,
    last_used_at:  DateTime<Utc>,
    used:          bool,           // true after rotation
    user_agent:    String,         // e.g. "fabro-cli 0.208.0 (darwin/arm64)"
}
```

`RefreshTokenStore` trait:

```rust
// Atomic read+rotate. Exactly one concurrent caller sees Rotated; any
// other concurrent caller sees Reused (and triggers theft handling).
enum ConsumeOutcome {
    Rotated(RefreshToken),   // old token, returned after atomic transition
    Reused(RefreshToken),    // token was already used — theft signal
    Expired,                 // expires_at <= now
    NotFound,                // never existed, or already deleted
}

async fn consume_and_rotate(
    &self,
    presented_hash: &[u8; 32],
    new_token: RefreshToken,   // caller pre-builds the new row
    now: DateTime<Utc>,
) -> Result<ConsumeOutcome>;

// Primary insertion at first login (not a rotation). Fails if token_hash
// already exists (statistically impossible with 256 bits, but treated as a
// hard error rather than a silent overwrite).
async fn insert(&self, token: RefreshToken) -> Result<()>;

async fn find(&self, token_hash: &[u8; 32]) -> Result<Option<RefreshToken>>;
async fn delete_chain(&self, chain_id: Uuid) -> Result<u64>;
async fn gc_expired(&self, cutoff: DateTime<Utc>) -> Result<u64>;
```

`consume_and_rotate` is the only *rotation* primitive — there is no standalone `mark_used` / `insert` pair on the refresh path. `insert` is reserved for the primary login path (`/auth/cli/token`), where no prior row is being replaced. It performs, atomically from the caller's perspective:

1. Read the row at `presented_hash`.
2. If missing → return `NotFound`.
3. If `expires_at <= now` → return `Expired`.
4. If `used == true` → return `Reused(row)` (do **not** mutate; theft is handled by the caller).
5. Else: mark the old row `used = true, last_used_at = now`, persist the new row at `new_token.token_hash`, delete the old row, return `Rotated(old_row)`.

The atomicity boundary is an in-memory per-`presented_hash` async mutex (a `DashMap<[u8;32], Arc<Mutex<()>>>` with coarse GC). This is correct because the design is single-node (Q5). If SlateDB later grows a compare-and-swap primitive, the implementation swaps underneath without changing the trait. `find` and `delete_chain` continue to exist for logout and the non-rotation inspection paths but are never combined with `insert` on the rotation path.

### Revocation semantics

**Delete-on-compromise** (no separate compromised-chain map). All state changes flow through the trait methods above:

- **Normal rotation:** `consume_and_rotate(presented_hash, new_token, now)` → `Rotated(old)` → mint JWT, return 200.
- **Reuse detection:** `consume_and_rotate` → `Reused(old)` → call `delete_chain(old.chain_id)`, log `WARN` with `chain_id`, `idp_subject`, `user_agent`, return 401 `refresh_token_revoked`.
- **Token expired or unknown:** `NotFound` / `Expired` → 401 `refresh_token_expired`.
- **Logout:** `find(hash)` → if present, `delete_chain(row.chain_id)`. Return 204 regardless (no oracle).
- **Deallowlisted on refresh:** checked *after* a successful `Rotated` outcome; on failure, immediately `delete_chain(row.chain_id)`, return 403 `unauthorized`.

A server crash mid-`consume_and_rotate` is fail-safe: either the in-memory mutex wasn't yet acquired (next attempt rotates cleanly) or the atomic step wasn't committed (next attempt sees `used == false`, rotates cleanly) or the atomic step committed (next attempt sees `used == true`, triggers theft handling). There is no partial state in which two descendants of the same old row both exist.

SlateDB is primary-key-only; `delete_chain` is implemented as a prefix scan over `auth/refresh/*` + filter by `chain_id`. Acceptable at expected scale.

### Lifecycle

- Authorization codes: in-memory `HashMap`, 60s TTL, single-use. Reaper task every 30s. Lost on server restart (user retries).
- Refresh tokens: SlateDB. Reaper task every 6h deletes rows with `expires_at < now - 7d` (7-day grace for audit).

### CLI storage (`~/.fabro/auth.json`)

```json
{
  "version": 1,
  "servers": {
    "https://fabro.example.com": {
      "access_token": "eyJhbGciOiJIUzI1NiIs...",
      "access_token_expires_at": "2026-04-19T14:30:00Z",
      "refresh_token": "fabro_refresh_AbCdEf...",
      "refresh_token_expires_at": "2026-05-19T14:20:00Z",
      "subject": {
        "idp_issuer": "https://github.com",
        "idp_subject": "12345",
        "login": "bhelmkamp",
        "name": "Bryan Helmkamp",
        "email": "bryan@qlty.ai"
      },
      "logged_in_at": "2026-04-19T14:20:00Z"
    }
  }
}
```

- File mode 0600; atomic temp+rename on write.
- Advisory exclusive file lock on `~/.fabro/auth.lock` during read-modify-write (protects against concurrent refresh races between two CLI processes against the same server).
- `version` field reserved for future format migrations.

`ServerUrl` is a newtype around `url::Url` that normalizes on construction: lowercase scheme+host, strip default port, strip trailing slash. Serializes back to canonical form. Used as the keyspace in `servers`.

## CLI UX

### Commands

```
fabro auth login                         # browser flow, stores credentials
fabro auth status                        # local state for all servers
fabro auth logout                        # revoke + clear for current server
fabro auth logout --all                  # revoke + clear for every server
```

Top-level dispatch lives in `Commands::Auth(AuthNamespace)`, parallel to the existing `Commands::Provider(ProviderNamespace)` pattern.

### `fabro auth login` happy path

```
$ fabro auth login
Opening https://fabro.example.com/auth/cli/start?... in your browser.
Listening on http://127.0.0.1:54213/callback for the auth response.

[browser opens, user completes GitHub OAuth, browser shows success page]

✓ Logged in to https://fabro.example.com as bhelmkamp (Bryan Helmkamp <bryan@qlty.ai>).
  Credentials stored in ~/.fabro/auth.json.
```

Flags:

- `--server <url>` — override server selection; otherwise uses normal resolution order (flag > `FABRO_SERVER` env > `settings.toml`).
- `--no-browser` — print URL instead of auto-opening (debugging aid; not a headless path — that's device flow, out of scope).
- `--timeout <duration>` — loopback wait timeout (default 5 min).

### `fabro auth status` output

GitHub-CLI-style per-server sections:

```
$ fabro auth status

https://fabro.example.com
  ✓ Logged in as bhelmkamp (Bryan Helmkamp <bryan@qlty.ai>)
  - Active account: true
  - Authentication: github
  - Access token: eyJ***************************** (expires in 6m)
  - Refresh token: fabro_refresh_****************** (expires in 29d)
  - Stored at: ~/.fabro/auth.json

http://127.0.0.1:8080
  ✓ Logged in via dev token
  - Token source: ~/.fabro/dev-token
```

- Single-server users see one block; multi-server users see one per server.
- Token values redacted to first few chars.
- Status computed purely from local clock vs stored `exp` fields — no server roundtrip. Works offline.
- States: `active`, `expired (refreshable)`, `expired`, `revoked (re-login required)`.
- `--server <url>` to scope to one server.
- `--json` for script consumption (emits structured equivalents).

### `fabro auth logout` behavior

1. Read refresh token for target server from `AuthStore`.
2. POST `/auth/cli/logout` with refresh token as bearer. Server deletes the chain.
3. Remove entry from `auth.json` atomically.
4. Print `✓ Logged out of <server>.`

If the server POST fails: still delete local creds, print a warning that remote revocation did not succeed and the refresh token may remain valid until its natural expiry. Fail-local-open so a broken state is always clearable.

### Bearer priority in `server_client.rs`

Per request:

1. If `FABRO_DEV_TOKEN` env var set → use it.
2. Else look up normalized server URL in `~/.fabro/auth.json` → if present and refresh not expired → use JWT (auto-refresh if access expired or within 30s of expiry).
3. Else fall back to dev-token search order (`<storage_dir>/server.dev-token`, then `~/.fabro/dev-token`).
4. Else send no auth header.

Auto-refresh triggers:

- **Pre-flight:** if `access_expires_at - now < 30s`, refresh before issuing the request. This is the primary path; reactive is a safety net for clock skew.
- **Reactive:** on 401 where the `ApiError` envelope carries `errors[0].code == "access_token_expired"`, refresh and retry once.

**Wiring through `map_api_error`.** The current CLI error path (`server_client.rs:1022-1047`) collapses `ApiError` into a stringy `anyhow::Error` by extracting only `errors[0].detail`, losing `code`. The auto-refresh path needs structured access without rewriting ~100 existing call sites. The contract:

1. Add a sibling helper `map_api_error_structured<E>(err) -> Result<T, ApiFailure>` with:
   ```rust
   pub(crate) struct ApiFailure {
       pub status:  http::StatusCode,
       pub code:    Option<String>,   // from errors[0].code if present
       pub detail:  String,           // from errors[0].detail, else fallback message
   }
   ```
2. The auto-refresh wrapper (a new thin layer around progenitor-generated calls on protected endpoints) uses `map_api_error_structured`. On `ApiFailure { status: 401, code: Some("access_token_expired"), .. }`, it refreshes and retries once.
3. Existing `map_api_error` is unchanged and keeps its `anyhow::Error` return type. All current callers continue to work. The auto-refresh wrapper converts `ApiFailure` to `anyhow::Error` at its boundary so callers see the same stringy errors as before when refresh isn't the answer.
4. The new `code` field is read from the JSON-decoded `ApiError` inner value via the same `serde_json::Value` traversal already used in `map_api_error`. Zero type-generation churn; no changes to `fabro-api-client` generation.

**Refresh-endpoint responses** are never routed through `map_api_error` because `/auth/cli/refresh` returns the flat OAuth envelope, not `ApiError`. The CLI calls it via a dedicated refresh helper that parses the flat `{error, error_description}` shape directly.

Refresh failures (`refresh_token_expired` / `refresh_token_revoked`) surface as: `error: session expired. Run 'fabro auth login'.` and non-zero exit. No interactive prompt; CLI stays scriptable.

## End-to-end auth flow

`fabro auth login` with `--server http://127.0.0.1:3000` and `server.web.url = https://fabro.example.com`:

```
CLI                                    Browser              Fabro server               GitHub
───                                    ───────              ────────────               ──────

0. Preflight:
   GET http://127.0.0.1:3000/api/v1/auth/cli/config
   ────────────────────────────────────────►  200:
                                                {
                                                  "enabled": true,
                                                  "web_url": "https://fabro.example.com",
                                                  "methods": ["github"]
                                                }
   If enabled == false → print reason from payload, exit non-zero (no browser).
   All subsequent browser URLs are constructed against web_url, not --server.

1. Generate PKCE pair:
   verifier = 32 random bytes (b64url)
   challenge = SHA256(verifier) (b64url)
   Generate csrf_state = 16 random bytes (b64url)

2. Bind loopback listener on 127.0.0.1:<random>, path /callback

3. Build URL (on web_url, NOT --server):
   https://fabro.example.com/auth/cli/start
     ?redirect_uri=http://127.0.0.1:54213/callback
     &state=<csrf_state>
     &code_challenge=<challenge>
     &code_challenge_method=S256

4. Open browser ───────────────────────►

                                       5. GET /auth/cli/start
                                          ────────────────────►  Validate query params strictly.
                                                                 Check __fabro_session cookie.

                                                                 Session eligibility: a session is
                                                                 usable here only if
                                                                   auth_method == Github
                                                                   AND non-empty idp_issuer
                                                                   AND non-empty idp_subject.
                                                                 A dev-token session is NOT eligible
                                                                 (it has no IdP identity) and is
                                                                 treated as "no session" for Case B
                                                                 routing. It is never an error by
                                                                 itself — the user just completes a
                                                                 GitHub login.

                                                                 Case A: eligible session → mint
                                                                   authz code and 302 directly to
                                                                   redirect_uri (skip steps 6–9).
                                                                 Case B: no eligible session:
                                                                   Set signed fabro_cli_flow cookie
                                                                   (10min TTL):
                                                                     { redirect_uri, state,
                                                                       code_challenge }
                                                                   302 → /auth/login/github
                                                                         ?return_to=/auth/cli/resume

                                       6. GET /auth/login/github
                                          ────────────────────►  Existing flow. Validate return_to
                                                                 against strict whitelist. Set
                                                                 fabro_oauth_state cookie.
                                                                 302 → github.com/login/oauth/authorize

                                       7. GitHub OAuth ──────────────────────────────► user approves

                                       8. GET /auth/callback/github?code=...&state=...
                                          ────────────────────►  Existing handler + return_to:
                                                                   - validate state cookie
                                                                   - exchange code at GitHub
                                                                   - fetch user + emails
                                                                   - allowlist check:
                                                                       on pass: mint __fabro_session
                                                                         cookie (auth_method=Github,
                                                                         idp_issuer, idp_subject),
                                                                         302 → return_to.
                                                                       on fail: NO session minted,
                                                                         302 → return_to
                                                                         ?error=unauthorized
                                                                         &error_description=...
                                                                   - other terminal failures (token
                                                                     exchange, user fetch): same
                                                                     return_to?error=... shape.

                                       9. GET /auth/cli/resume[?error=...]
                                          ────────────────────►  Read fabro_cli_flow cookie.
                                                                 If inbound ?error is set (passed
                                                                 through from step 8's rejection):
                                                                   clear fabro_cli_flow cookie,
                                                                   302 → redirect_uri?error=<code>
                                                                         &error_description=...
                                                                         &state=<state>
                                                                   (do NOT check session — allowlist
                                                                   rejection left no session minted).
                                                                 Else require __fabro_session with
                                                                 the SAME eligibility check as /start
                                                                 (auth_method == Github + non-empty
                                                                 idp_issuer/idp_subject). If not
                                                                 eligible:
                                                                   clear fabro_cli_flow cookie,
                                                                   302 → redirect_uri
                                                                         ?error=github_session_required
                                                                         &error_description=...
                                                                         &state=<state>.
                                                                 Otherwise:
                                                                   Mint authz_code (60s TTL) keyed
                                                                   to the session's (idp_issuer,
                                                                   idp_subject).
                                                                   Clear fabro_cli_flow cookie.
                                                                   302 → redirect_uri?code=...&state=...

                                       10. 302 → http://127.0.0.1:54213/callback?code=...&state=...

                                       11. Browser hits loopback ──────►  12. CLI listener:
                                                                              - verify state matches
                                                                              - if `error` present:
                                                                                  surface
                                                                                  error_description,
                                                                                  respond with error
                                                                                  HTML, shut down,
                                                                                  exit non-zero.
                                                                              - if `code` present:
                                                                                  respond with
                                                                                  "Logged in. You can
                                                                                   close this tab."
                                                                              - shut down listener

13. POST <cli_target or web_url>/auth/cli/token
    (uses CLI's normal transport — can be Unix socket, HTTP, or HTTPS)
    {
      "grant_type": "authorization_code",
      "code": "<authz_code>",
      "code_verifier": "<verifier>",
      "redirect_uri": "http://127.0.0.1:54213/callback"
    }
    ────────────────────────────────────────────►  Server:
                                                     - look up code in in-memory map
                                                     - burn (mark used)
                                                     - verify SHA256(verifier) == challenge
                                                     - verify redirect_uri matches
                                                     - assert code.idp_issuer and
                                                       code.idp_subject are non-empty
                                                       (defensive; only eligible GitHub
                                                       sessions can mint codes at /start
                                                       and /resume, so this is the belt
                                                       to that suspenders)
                                                     - allowlist re-check on code.login
                                                     - generate refresh token (32 bytes b64url)
                                                     - chain_id = Uuid::new_v4()
                                                     - insert RefreshToken (primary, not a rotation)
                                                     - mint JWT
                                                     - return 200 with both tokens + subject

14. Write ~/.fabro/auth.json (atomic, 0600, file-locked).
15. Print success, exit 0.
```

**Refresh flow:**

```
1. CLI notices access token expired or near-expiry.
2. POST /auth/cli/refresh
   Authorization: Bearer <refresh_token>
   ───────────────────────────────────►   Server:
                                            - SHA-256 presented token → presented_hash
                                            - pre-build new_token (fresh 32-byte secret,
                                              same chain_id, expires_at = now + 30d)
                                            - consume_and_rotate(presented_hash, new_token, now):
                                                NotFound | Expired → 401 refresh_token_expired
                                                Reused(old)       → delete_chain(old.chain_id),
                                                                    WARN log,
                                                                    401 refresh_token_revoked
                                                Rotated(old)      → continue
                                            - allowlist re-check against old.login.
                                              If removed → delete_chain(old.chain_id),
                                                           403 unauthorized.
                                            - mint new JWT
                                            - return 200 with both tokens + subject
3. Persist new pair to auth.json atomically.
4. Retry original request with new access token.
```

**Logout flow:**

```
1. POST /auth/cli/logout
   Authorization: Bearer <refresh_token>
   ───────────────────────────────────►   Server:
                                            - hash presented token, look up
                                            - if found, delete_chain(chain_id)
                                            - return 204 regardless
2. Delete entry in auth.json (atomic).
```

## Server endpoints

Six new routes total: one unauthenticated preflight under `/api/v1/`, five OAuth-flow routes under `/auth/cli/`.

**Mounting and startup validation.** `/auth/*` only exists when `server.web.enabled = true` (`server.rs:916-918`). If `server.auth.methods` includes `github` but `web.enabled = false`, the server fails at startup with a clear configuration error (caught during settings resolution, not at first request).

When `web.enabled = true` but `github` is not in `methods`, the routes are still mounted but every handler short-circuits at "not configured." The rejection shape is endpoint-appropriate:

- **JSON endpoints** (`/auth/cli/token`, `/auth/cli/refresh`, `/auth/cli/logout`): 403 with flat OAuth body `{"error": "github_auth_not_configured", "error_description": "..."}`.
- **Browser endpoints** (`/auth/cli/start`, `/auth/cli/resume`): render a plain HTML error page ("GitHub authentication is not configured on this server. Contact your administrator."). No redirect to loopback — we do not trust any `redirect_uri` supplied in this state because a CLI making this request has ignored the preflight.

In practice this path is mostly unreachable: the CLI should stop at preflight (`/api/v1/auth/cli/config` returning `enabled: false`) before reaching these endpoints. The in-handler checks are defense-in-depth for direct/scripted traffic.

**IP allowlist:** when enabled, applies to every endpoint including these without exception. No carve-outs.

### `GET /api/v1/auth/cli/config` (preflight, unauthenticated)

Discovery endpoint for the CLI. Lives under `/api/v1/` so it is reachable over whatever transport the CLI is configured for (HTTP, HTTPS, Unix socket) — it is **not** browser-flow-dependent and is mounted regardless of `web.enabled`.

Response when CLI login is available:

```json
{
  "enabled": true,
  "web_url": "https://fabro.example.com",
  "methods": ["github"]
}
```

Response when CLI login is unavailable:

```json
{
  "enabled": false,
  "reason": "github_not_enabled",
  "reason_description": "GitHub auth is not in server.auth.methods",
  "web_url": null,
  "methods": ["dev-token"]
}
```

Fields:

- `enabled` — `true` iff `web.enabled` and `server.auth.methods.contains(Github)`. The incoherent combination (`github` in methods but `web.enabled=false`) is a startup error, so it is never observable at this endpoint.
- `web_url` — canonical origin the CLI MUST open its browser against; equals `server.web.url`. `null` when `enabled=false`.
- `methods` — informational list of configured server auth methods. Reserved for Google Workspace / GHES expansion.
- `reason` (only when `enabled=false`) — machine-readable code. Defined values:
  - `github_not_enabled` — `auth.methods` does not include `"github"`.
  - `web_not_enabled` — `web.enabled=false` (and github is also absent, or the server would have failed at startup).
- `reason_description` (only when `enabled=false`) — human-readable sentence for CLI to surface to the user.

### `GET /auth/cli/start`

Initiates the CLI flow. Runs on `server.web.url`'s origin.

Query params:

- `redirect_uri` — must match `http://127.0.0.1:<port>/callback` or `http://[::1]:<port>/callback`. Any other host/path → plain HTML error (redirect target cannot be trusted).
- `state` — opaque, 16–512 chars, URL-safe. Required.
- `code_challenge` — base64url, ~43 chars. Required.
- `code_challenge_method` — must be exactly `S256`. Required.

Session eligibility gate (applied at every visit):

```
eligible := session.auth_method == RunAuthMethod::Github
         && !session.idp_issuer.is_empty()
         && !session.idp_subject.is_empty()
```

Response, happy path:

- Eligible session: mint authz code keyed to `(idp_issuer, idp_subject, login, name, email, code_challenge, redirect_uri)`; 302 to `redirect_uri?code=...&state=...`.
- Ineligible session *or* no session: set signed `fabro_cli_flow` cookie with the flow params (10 min TTL); 302 to `/auth/login/github?return_to=/auth/cli/resume`. A dev-token-authored session is not an error here — the user just completes a GitHub login to obtain the required identity.

Error handoff (per the Section "Browser-to-CLI error handoff" contract):

- If `redirect_uri` fails the host/path whitelist: render plain HTML error in browser (CLI times out).
- If `state` is missing/malformed: render plain HTML error in browser (we will not redirect to an untrusted URL with attacker-supplied state).
- If `code_challenge` or `code_challenge_method` fail validation: `redirect_uri` and `state` have already passed — 302 to `redirect_uri?error=invalid_request&error_description=...&state=<state>`.

### `GET /auth/cli/resume`

Continuation point after web OAuth completes. Not user-facing.

1. Read `fabro_cli_flow` cookie → recover `{redirect_uri, state, code_challenge}`. If missing/expired: render plain HTML error (we have no trusted `redirect_uri` to use); CLI times out.
2. **Error passthrough** (must run before any session check): if the inbound query string contains `?error=<code>` (set by `/auth/callback/github` on allowlist rejection or any other terminal failure), forward it to the CLI loopback:
   - 302 → `redirect_uri?error=<code>&error_description=<callback-supplied or default>&state=<state from cookie>`.
   - Clear `fabro_cli_flow` cookie.
   - Return. Do not inspect `__fabro_session`.

   Rationale: allowlist rejection happens in `/auth/callback/github` (`web_auth.rs:522-526`) **before** a session cookie is minted, so `/resume` will see no session on this path. Without this step the real cause (`unauthorized`) would be masked as `github_session_required`.
3. Require `__fabro_session`. Apply the same session eligibility gate as `/start`. If ineligible: 302 to `redirect_uri?error=github_session_required&error_description=...&state=<state>`, clear `fabro_cli_flow` cookie.
4. Mint authz code; insert into in-memory map (60s TTL), keyed to the session's `(idp_issuer, idp_subject, login, name, email, code_challenge, redirect_uri)`.
5. Clear `fabro_cli_flow` cookie.
6. 302 → `redirect_uri?code=...&state=...`.

Contract with `/auth/callback/github`: the existing handler hardcodes `Redirect::to("/login?error=unauthorized")` on allowlist rejection (`web_auth.rs:525`). With `return_to` threaded through, this becomes: when a `return_to=/auth/cli/resume` is in effect, rejection redirects to `/auth/cli/resume?error=unauthorized&error_description=<text>` instead of `/login?...`. Any other terminal failure the callback can emit (failed GitHub token exchange, failed user fetch) follows the same shape (`?error=<code>&error_description=...`). The success path is unchanged.

Whitelist of `error` codes the CLI may receive from this passthrough: `unauthorized`, `server_error`, `access_denied` (user clicked "Cancel" at GitHub). The CLI treats any unknown `error` code as `server_error` with the supplied description.

### Modified `/auth/login/github` and `/auth/callback/github`

`/auth/login/github` accepts an optional `return_to` query param with strict whitelist:

- Must be an absolute path starting with `/`.
- Must match `^/auth/cli/(resume|start)$`.
- Anything else → treated as absent; log `WARN`.

`return_to` is threaded through the OAuth state cookie so the callback knows where to land.

`/auth/callback/github` honors `return_to` on **both** success and error paths:

- **Success** (existing behavior, plus `return_to`): mint session, 302 to `return_to` (default `/`).
- **Allowlist rejection** (currently hardcoded to `/login?error=unauthorized` at `web_auth.rs:525`): 302 to `return_to?error=unauthorized&error_description=<login not permitted>` when `return_to` is present; retain existing `/login?error=unauthorized` when absent.
- **Other terminal failures** (GitHub token exchange fails, user/emails fetch fails, etc.): 302 to `return_to?error=server_error&error_description=<cause>` when `return_to` is present; existing behavior otherwise.
- **User denial at GitHub** (GitHub itself redirects to our callback with `?error=access_denied`): 302 to `return_to?error=access_denied&...` when `return_to` is present.

On every error-path redirect, no session cookie is set. The `/auth/cli/resume` endpoint's error-passthrough step (see above) forwards these to the CLI loopback.

### `POST /auth/cli/token`

Exchange authz code for access + refresh pair. Runs on whatever transport the CLI uses.

Request:

```json
{
  "grant_type": "authorization_code",
  "code": "<authz_code>",
  "code_verifier": "<verifier>",
  "redirect_uri": "<original>"
}
```

Steps:

1. Look up code in in-memory map → burn (mark used + remove) in a single atomic op. Missing/burned → 400 `invalid_code`.
2. Verify `SHA256(code_verifier)` matches stored `code_challenge` → else 400 `pkce_verification_failed`.
3. Verify presented `redirect_uri` equals stored → else 400 `redirect_uri_mismatch`.
4. Defensive identity check: the stored `AuthCode` has non-empty `idp_issuer` and `idp_subject`. If either is empty → 403 `github_session_required`. This should be unreachable in practice (only eligible GitHub sessions can mint codes at `/start` and `/resume`); present as belt-and-suspenders.
5. Re-check allowlist against stored `login` → else 403 `unauthorized`.
6. Generate 32-byte refresh token, `chain_id = Uuid::new_v4()`.
7. Insert `RefreshToken` via `RefreshTokenStore::insert` (this is a primary insertion, not a rotation — `consume_and_rotate` is only for `/refresh`).
8. Mint JWT.
9. Return 200:

```json
{
  "access_token": "eyJ...",
  "access_token_expires_at": "...",
  "refresh_token": "fabro_refresh_...",
  "refresh_token_expires_at": "...",
  "subject": { "idp_issuer": "...", "idp_subject": "...", "login": "...", "name": "...", "email": "..." }
}
```

Errors (flat RFC 6749 envelope): 400 `invalid_code` / `pkce_verification_failed` / `redirect_uri_mismatch`; 403 `github_session_required` / `unauthorized` / `github_auth_not_configured`.

### `POST /auth/cli/refresh`

Rotate refresh + access tokens. Empty body. Refresh token in `Authorization: Bearer`. Runs on whatever transport the CLI uses.

Pre-handler short-circuit (per the Mounting and startup validation section): if `github` is not in `server.auth.methods`, return 403 `github_auth_not_configured` (flat OAuth envelope) before any bearer parsing.

Steps (when GitHub auth is configured): parse + hash bearer → call `consume_and_rotate(presented_hash, new_token, now)` → branch on outcome (see Refresh flow in previous section) → allowlist re-check → mint new JWT → return same shape as `/auth/cli/token`.

Errors (flat RFC 6749 envelope): 401 `refresh_token_expired` (NotFound/Expired) / `refresh_token_revoked` (Reused); 403 `unauthorized` (allowlist removal) / `github_auth_not_configured` (pre-handler short-circuit).

### `POST /auth/cli/logout`

Empty body. Refresh token in bearer. Runs on whatever transport the CLI uses.

Pre-handler short-circuit (per the Mounting and startup validation section): if `github` is not in `server.auth.methods`, return 403 `github_auth_not_configured` (flat OAuth envelope) before any bearer parsing.

Once GitHub auth is configured and the logout handler is reached, it always returns 204 regardless of whether the presented token was valid (no oracle).

Errors (flat RFC 6749 envelope): 403 `github_auth_not_configured` (pre-handler short-circuit only).

### Bearer dispatch in `jwt_auth.rs`

Existing extractor continues to read `Cookie: __fabro_session` from its current location in request headers. The bearer-header path is extended:

```
if bearer.starts_with("fabro_dev_")      → validate as dev token (existing path)
else if bearer.starts_with("fabro_refresh_") → 401 (refresh tokens only valid
                                                    at /auth/cli/{refresh,logout})
else if bearer.starts_with("eyJ")        → validate as JWT (see verification sequence)
else                                      → 401
```

`CredentialSource` enum grows:

```rust
enum CredentialSource {
    DevToken,
    SessionCookie,
    JwtAccessToken,  // new
}
```

### Route registration

```rust
// API router (unauthenticated preflight):
.route("/auth/cli/config", get(cli_flow::config))   // mounted inside /api/v1

// Web router (browser-facing + token endpoints), gated on web.enabled:
.route("/auth/cli/start", get(cli_flow::start))
.route("/auth/cli/resume", get(cli_flow::resume))
.route("/auth/cli/token", post(cli_flow::token))
.route("/auth/cli/refresh", post(cli_flow::refresh))
.route("/auth/cli/logout", post(cli_flow::logout))
```

All handlers manage their own credential validation (no extractor-level auth). The five `/auth/cli/*` routes fall under the existing `web_enabled` gate; the `/api/v1/auth/cli/config` preflight is always mounted and reports `enabled: false` when the flow cannot succeed.

## Storage and persistence

**Server SlateDB keyspaces:**

```
auth/refresh/<hex(token_hash)>   →  bincode-serialized RefreshToken
```

One keyspace. No secondary index in v1 (prefix scan for chain deletion is acceptable at expected scale).

**Server in-memory state:**

```rust
struct AuthCodeStore { codes: Mutex<HashMap<String, AuthCode>> }

struct AuthCode {
    idp_issuer:     String,
    idp_subject:    String,
    login:          String,
    name:           String,
    email:          String,
    code_challenge: String,
    redirect_uri:   String,
    expires_at:     DateTime<Utc>,
}
```

Reaper task every 30s; canceled on graceful shutdown.

**CLI local state:** `~/.fabro/auth.json` as described above.

## Security properties

- **PKCE (S256)** prevents malicious apps on the same machine from exchanging an intercepted loopback code.
- **CSRF state** prevents a malicious site from tricking a signed-in user's browser into completing CLI login for an attacker account.
- **Strict redirect whitelist** (loopback-only, exact path match) prevents open-redirect abuse.
- **`return_to` whitelist** (strict path allowlist) prevents using `/auth/login/github` as an open redirect.
- **Single-use authz codes**, 60s TTL, burned on first exchange.
- **Atomic single-use rotating refresh tokens with chain reuse-detection** — `consume_and_rotate` is the only rotation primitive; two concurrent refreshes with the same token cannot both succeed, and any second presentation after rotation is definitively a theft signal.
- **Session identity gate** — `/start`, `/resume`, and `/token` all require `auth_method == Github` with non-empty `idp_issuer`/`idp_subject`. Dev-token and future non-GitHub sessions cannot bootstrap CLI credentials.
- **Allowlist re-checked on login and every refresh** — deallowlisted users are out within ≤10 min (one access-token TTL).
- **Hash-only storage** of refresh tokens — SlateDB snapshot leak does not directly yield usable tokens.
- **Loopback-only listener** (127.0.0.1 bind) — redirect cannot be intercepted over the network.
- **Canonical origin discipline** — browser flow runs only on `server.web.url`, avoiding cross-origin cookie loss that would silently break state/PKCE validation.
- **HS256 locked** — algorithm pinned at parse time; no `alg: none`, no RS/HS confusion.
- **Domain-separated subkeys** — cookie and JWT keys are independent despite sharing `SESSION_SECRET`.

## Testing strategy

### Prerequisite: twin-github OAuth extension

The existing `test/twin/github` covers GitHub App surface (installations, PRs, branches, git protocol) but does not implement user OAuth endpoints. As a prerequisite sub-task, extend twin-github with:

- `handlers/oauth.rs`:
  - `GET /login/oauth/authorize` — auto-approve (configurable to simulate denial); redirect to `redirect_uri?code=<fake>&state=<state>`.
  - `POST /login/oauth/access_token` — validate `client_secret` and `code`; return `{access_token, token_type, scope}`.
- `handlers/users.rs`:
  - `GET /user` — return fixture user (configurable via twin state).
  - `GET /user/emails` — return fixture email(s).
- Twin state holds a seeded `GithubUser` for the "current" OAuth subject.

Scope: happy path, explicit-denial, wrong-client-secret. Not a full-fidelity GitHub replica.

This lets CLI integration tests exercise the real `web_auth.rs` OAuth glue end-to-end instead of stubbing session cookies.

### Server unit tests

- **`auth/jwt.rs`:** round-trip encode/verify; reject `alg: none`, `alg: RS256`, mismatched `iss`, mismatched `aud`, expired; accept within clock skew; HKDF derivation deterministic.
- **`auth/refresh_store.rs`:** `consume_and_rotate` outcomes (`Rotated`, `Reused`, `Expired`, `NotFound`); concurrent `consume_and_rotate` with the same presented token — exactly one caller sees `Rotated`, the other sees `Reused` (use `tokio::task::JoinSet` and assert the outcome histogram over many trials); `delete_chain` removes all descendants; `gc_expired` leaves unexpired rows alone.
- **`auth/code_store.rs`:** insert/consume single-use; reaper drops expired; reaper preserves unexpired.
- **`ApiError` extension:** serializing `ApiError` with and without `code` produces backward-compatible JSON (existing fields unchanged when `code` is `None`).
- **Startup validation:** resolving settings with `auth.methods=[github]` and `web.enabled=false` produces a configuration error.

All tests use SlateDB with `object_store::memory::InMemory` backend (existing pattern in `fabro-store`).

### Server integration tests (`fabro-server/tests/it/api/cli_auth.rs`)

- **Happy path** using twin-github: full Section 3 flow end-to-end, including the `/api/v1/auth/cli/config` preflight.
- **Preflight when `github` not in methods:** `config` returns `{enabled: false, reason: "github_not_enabled", reason_description: "..."}`; CLI-side test asserts it refuses to open a browser and prints the description.
- **Startup error:** booting with `auth.methods=[github]` and `web.enabled=false` → server exits non-zero with a specific error message; assert via the settings-resolution unit test (no running process needed).
- **Canonical origin discipline:** launch the flow with `--server` pointing at an API origin distinct from `server.web.url`. Assert the browser URL the CLI would open is on `web_url`, not the API origin. (Dual-origin token exchange is covered by the happy-path test; this one guards the URL-construction step.)
- **Browser error handoff:** trigger each error class that should flow through the loopback:
  - PKCE challenge missing → expect loopback to receive `error=invalid_request` with matching `state`.
  - Dev-token session at `/auth/cli/resume` → expect loopback to receive `error=github_session_required`.
  - GitHub allowlist rejection during callback → callback redirects to `/auth/cli/resume?error=unauthorized` (no session minted); `/resume`'s error-passthrough forwards to loopback with `error=unauthorized`, NOT `github_session_required`. Explicitly assert the passthrough ordering (error-check before session-check).
  - GitHub user denies authorization at github.com (`?error=access_denied` arriving at `/auth/callback/github`) → loopback receives `error=access_denied`.
  - Assert CLI exits non-zero with the `error_description` text in each case.
- **Browser error fallthrough:** invalid `redirect_uri` (not loopback) → server renders plain HTML error page, loopback listener receives no callback, CLI eventually times out with a generic "login did not complete" message.
- **`github_auth_not_configured` short-circuit:** with `web.enabled=true` but `github` not in methods, assert the endpoint-appropriate rejection shape on direct hits that bypass preflight:
  - `/auth/cli/start` → plain HTML error page.
  - `/auth/cli/resume` → plain HTML error page.
  - `/auth/cli/token` → flat OAuth 403 `github_auth_not_configured`.
  - `/auth/cli/refresh` → flat OAuth 403 `github_auth_not_configured` (before bearer parsing).
  - `/auth/cli/logout` → flat OAuth 403 `github_auth_not_configured` (NOT 204; the unconditional-204 contract only applies once GitHub auth is configured).
- **PKCE mismatch:** wrong `code_verifier` → 400 `pkce_verification_failed`, code burned (retry with correct verifier also fails).
- **Redirect-URI whitelist:** non-loopback host, non-`/callback` path → 400.
- **`return_to` whitelist:** external URL ignored, WARN logged.
- **Dev-token session cannot bootstrap CLI flow:** POST `/auth/login/dev-token` to mint a dev-token session cookie; visit `/auth/cli/start` with that cookie → Case B routing (redirected to GitHub login), NOT Case A (no authz code minted). Same assertion for `/auth/cli/resume` with a dev-token session → 302 back to `redirect_uri?error=github_session_required&...` (covered by the "Browser error handoff" test suite).
- **Concurrent refresh:** kick off N=32 simultaneous `/auth/cli/refresh` calls with the same refresh token; assert exactly one gets 200 with a new token, all others get 401 `refresh_token_revoked`, chain is deleted.
- **Reuse detection:** old refresh token after rotation → 401, new token also invalidated (chain deleted).
- **Allowlist removed mid-session:** refresh after `allowed_usernames` mutation → 403, chain deleted.
- **Clock skew tolerance:** `iat` +20s accepted, +60s rejected.
- **Auth-code expiry:** sleep past 60s TTL → 400 `invalid_code`.
- **Auth-code single-use:** valid exchange then replay → 400.
- **IP allowlist enforcement:** with allowlist enabled, `/auth/cli/start` AND `/api/v1/auth/cli/config` from non-allowlisted IP → 403 (regression guard for no-carve-out rule).
- **Error envelope shapes:** protected endpoint 401 returns `ApiError` with `code="access_token_expired"`; `/auth/cli/refresh` 401 returns flat `{"error": "refresh_token_expired", ...}`.
- **Dev-token bearer unchanged.**
- **Bearer dispatch:** `fabro_refresh_...` to a protected endpoint → 401.

### CLI unit tests

- **`auth_store.rs`:** put/get/remove round-trips; URL normalization; concurrent put serialized by file lock; corrupt file → clear error.
- **`commands/auth/login.rs`:** PKCE pair generation (`challenge == b64url(SHA256(verifier))`); loopback listener binds ephemeral port; callback rejects mismatched `state`; callback bearing `error`+`error_description` exits non-zero and surfaces the description; preflight `enabled=false` aborts before opening a browser.
- **`server_client.rs`:** bearer priority matrix; auto-refresh on `access_token_expired`; refresh persists new tokens; non-auth 401 does not trigger refresh.

### CLI end-to-end test (`fabro-cli/tests/`)

Spin up real `fabro-server` in-process, configured to point at twin-github. Run `fabro auth login` in a subprocess; a test driver reaches twin-github's `authorize` endpoint to drive the OAuth approval (or the subprocess's browser launch is intercepted to complete the flow headlessly). Assert `auth.json` contents, snapshot `fabro auth status` output, run `fabro run` successfully with JWT bearer, `fabro auth logout`, subsequent `fabro run` fails with "session expired".

### Not in automated CI

- Real github.com OAuth.
- Actual browser launch (untestable without a display).

### Manual verification checklist (PR description)

1. `fabro auth login` against a real dev server — browser opens, flow completes, credentials persist.
2. Kill server mid-login — CLI times out cleanly after `--timeout`.
3. Second login to same server — old refresh row deleted, new works; old JWT continues until expiry.
4. Two concurrent `fabro run` sharing `auth.json` — auto-refresh under load does not corrupt the file.
5. Remove login from `allowed_usernames`, force refresh — CLI fails with clear error within 10 min.

## Implementation order (for the plan writer)

1. Extend `twin-github` with OAuth + user endpoints (prerequisite).
2. Migrate `SessionCookie` from `provider_id` to `(idp_issuer, idp_subject)`; bump cookie version.
3. Extend `ApiError` with an optional `code` field (backwards compatible); update `fabro-api.yaml` to document it.
4. Add HKDF-derived subkeys for cookie key + JWT key; extract into a small helper.
5. Add startup validation: reject `auth.methods=[github]` combined with `web.enabled=false`.
6. Implement `auth/jwt.rs` (issue + verify; emits `code` via `ApiError` on expired access token).
7. Implement `auth/refresh_store.rs` (with `consume_and_rotate` atomic primitive) and `auth/code_store.rs`.
8. Implement `cli_flow::config` preflight (`/api/v1/auth/cli/config`, unauthenticated).
9. Implement `auth/cli_flow.rs` (five OAuth-flow handlers); apply session eligibility gate consistently at `/start`, `/resume`, `/token`.
10. Add `return_to` support to `/auth/login/github` (strict whitelist).
11. Extend `jwt_auth.rs` extractor to accept JWTs.
12. Write server-side unit + integration tests (including concurrent-refresh race test).
13. CLI: `auth_store.rs` with file locking and URL normalization.
14. CLI: `commands/auth/{mod,login,logout,status}.rs`; login flow calls preflight before opening a browser.
15. CLI: `server_client.rs` bearer priority + auto-refresh. Add sibling `map_api_error_structured` → `ApiFailure { status, code, detail }` (leave existing `map_api_error` untouched); wrap protected-endpoint calls with a thin auto-refresh layer keyed on `ApiFailure { status: 401, code: Some("access_token_expired"), .. }`; parse `/auth/cli/refresh` flat OAuth envelope directly.
16. CLI unit + end-to-end tests.
17. Manual verification.
