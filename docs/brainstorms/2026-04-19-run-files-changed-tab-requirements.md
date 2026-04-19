---
date: 2026-04-19
topic: run-files-changed-tab
---

# Run "Files Changed" Tab — Make It Real

## Problem Frame

Every workflow run modifies files in a sandboxed worktree. Today the run detail UI has a **Files** tab skeleton that imports `@pierre/diffs` and renders hardcoded `fallbackFiles` when the loader's response is empty — which is always the case in a non-demo server, because the endpoint `GET /runs/{id}/files` the tab calls does not exist. The OpenAPI spec even defines `PaginatedRunFileList` and `FileDiff`, but no operation references them.

Users cannot see what a run actually changed without leaving the product. The goal of this brainstorm is to replace the demo/fallback content with a real, per-run diff rendered by `@pierre/diffs`, with sensible behavior across the run lifecycle (running → succeeded/partial → failed → sandbox destroyed).

Persistence reality today (verified in code):
- `RunProjection.final_patch: Option<String>` is captured on `RunCompleted` via the lifecycle git hook (`lib/crates/fabro-workflow/src/lifecycle/git.rs`) and stored in SlateDB (`lib/crates/fabro-store/src/run_state.rs`).
- It is a **unified-patch string** (output of `git diff <base_sha> HEAD`), not structured per-file `{old_file, new_file}` contents.
- It is set only for `Success` / `PartialSuccess` — `RunFailed` does not populate it.
- Successful checkpoint commits are pushed to `origin` (E2E: `daytona_integration.rs:1541`), so the branch + SHAs are typically reachable from origin after the sandbox is gone — though **branch retention is not a guarantee** (auto-delete on merge, admin cleanup, force-push, repo archival can prune refs). We treat origin as a best-effort source, not an authority.

### Failure modes this plan must avoid
- **Partial-coverage trust erosion.** The tab silently shows content in some states and nothing in others. Users form the impression "works sometimes."
- **Stale-but-plausible content.** The UI shows a diff that's minutes old with no indication of freshness; users act on it as current.
- **Secret exfiltration.** A working-tree file contains a token/credential and the tab happily displays it to anyone with run access.
- **Server DoS.** An authenticated caller hits the endpoint in a loop against a massive run and pegs the box.

Each requirement below names the failure mode it addresses.

## Requirements

**Presentation (Files tab UI)**
- R1. The Files tab renders a real `FileDiff[]` via `@pierre/diffs`' `MultiFileDiff` for the current run.
- R2. Keep the existing split/unified and disable-background toggles. Persist the split/unified choice in browser localStorage (scope is per-browser, not per-account; note: shared-machine users inherit the last setting). If an md-breakpoint collapse fires (R8), it overrides the rendered view without overwriting the stored preference.
- R3. Remove (or hide behind a feature flag) the inline "Steer" comment affordance — out of scope here; it will be brainstormed separately when the backend `/runs/{id}/steer` is designed.
- R4. **Empty-state taxonomy** (expected: no diff exists): distinct messaging for (a) the run has not produced a `base_sha` yet (Submitted/Starting), (b) the run produced a `base_sha` but touched no files, (c) the run completed but has no recoverable diff (Failed with no checkpoint, or Succeeded-but-`final_patch`-absent + origin unreachable). The toolbar stays usable in all three.
- R5. **Error-state taxonomy** (diff should exist but we could not retrieve it): distinct UI treatment for (a) transient / retriable (network, timeout, reconnect failure) with a Retry affordance, (b) permanent / data-loss — typically caught by R4(c) instead, (c) auth/config (e.g. origin credentials invalid) with a contact-admin affordance, (d) unknown 5xx with a request ID for support. When `final_patch` is available and origin is unreachable, the response flows into R12's degraded view, not R5.
- R6. **Loading and refresh**: initial fetch shows a diff-area skeleton (toolbar still visible). The tab exposes an explicit **Refresh** button in the toolbar. For **Running** runs the freshness indicator reads "Checkpoint Xm ago" sourced from the `to_sha` commit time (not the HTTP response time) so users can distinguish "tab fetched seconds ago" from "nothing has changed on the agent side in minutes". For completed runs it reads "Fetched Xs ago" sourced from HTTP response time. Prior content remains visible during re-fetch; a failed refresh leaves the last good data on screen with an inline error banner. The Refresh button is disabled when the last response's `to_sha` equals the currently-displayed `to_sha`. No auto-refresh in v1.
- R7. **Deep linking**: the URL supports a per-file anchor (`#file=<path>`) that scrolls to that file and expands it on load.
- R8. **Accessibility and responsive**: below Tailwind `md` the split view auto-collapses to unified; `j`/`k` keys move between files; `change_kind` indicators carry accessible names; toolbar controls meet WCAG touch-target size; focus returns to the refresh button after refresh.

**Diff semantics**
- R9. Show the **net diff between the run's `base_sha` and its current head commit** (equivalent to `git diff <base_sha> HEAD`). Committed changes only; uncommitted working-tree state is **not** surfaced in v1.
- R10. **Live (Running)** reads the sandbox's HEAD via the sandbox's git (effectively "diff as of the most recent checkpoint"). **Semantics** match `sandbox_git::git_diff` (`git diff <base_sha> HEAD`), but that helper is `pub(crate)`, has a 30 s timeout that conflicts with R32, and returns a single unbounded string — it cannot satisfy R24/R25/R27/R28. A new sibling helper is required: enumerates via `git diff --raw -z --find-renames=50% <base>..HEAD` with an early-stop at the 200-file cap, then per-file materializes via `git cat-file --batch` (size-gated) and `git show <sha>:<path>` for non-binary entries. Committed-HEAD reads are safe to interleave with in-progress agent work (no index/working-tree mutation).
- R11. **Post-sandbox** reconstructs the diff from the origin-side checkpoint branch using the run's recorded `base_sha` and head SHA.
- R12. **Fallback when origin cannot be read** (branch pruned, fetch failure, repo archived) and `final_patch` is available (Success / PartialSuccess only per Persistence reality): parse the stored `final_patch` string into a degraded `FileDiff[]` — each entry has the file name, `change_kind`, and hunks, but **no full old/new contents**. The UI renders a "patch-only view" indicator so users know contents are not available. `@pierre/diffs` consumes this via empty `contents` plus a visible banner on each affected file (exact UI treatment is a planning concern; behavior is specified here). Note: the current `DiffFile` schema has only `{name, contents}`; R12 requires either (a) a new optional `hunks` field on `FileDiff`, or (b) dropping R12 in favor of R4's empty-state path — see Outstanding Questions. The degraded view must re-apply R24/R25/R26/R31 caps + denylist during parsing; filters apply to the response, not the source. If `final_patch` is `None` on a completed run, treat as R4(c) (no recoverable diff).
- R13. **Coverage by run state**: `Running` / `Succeeded` / `PartialSuccess` always produce a diff. `Failed` produces a diff **if** the run checkpointed at least once before failing: use `RunFailed.git_commit_sha` (already carried on the event) as the head SHA and reconstruct from origin via R11, or from the sandbox if reachable via R10. Failed runs with no successful checkpoint fall through to R4(c). No changes to the run lifecycle or the `RunFailed` event are required.

**Backend / API contract**
- R14. Add `GET /api/v1/runs/{id}/files` with `operationId: listRunFiles`, returning the existing `PaginatedRunFileList` schema. The name is retained (additive change); v1 repurposes the existing `meta` object with top-level `truncated: bool` + `total_changed: int` per R27 rather than populating cursor fields. A rename can happen in a later additive version.
- R15. The endpoint accepts optional `from_sha` and `to_sha` query parameters. Defaults: `from_sha = base_sha`, `to_sha = current HEAD`. Clients omit both in v1 and the server **rejects non-default values with 400** in v1 (prevents arbitrary-ref reads outside the run's branch; re-enable once an ancestry check against the run's branch head is in place). A future version can enable non-default values without a schema break. Rationale: keep the forward-compat slot without opening a read-ref surface before it's validated.
- R16. Materialize the `FileDiff[]` **on demand** at request time. No new run-time persistence of per-file contents.
- R17. Do not remove or change SlateDB's existing `final_patch` string projection. Other paths depend on it (PR body generation). The new endpoint is additive.
- R18. No changes to the run lifecycle (no new events, no push-on-failure, no new event props). The tab works with existing persisted state.

**Schema changes to `FileDiff`** (`docs/api-reference/fabro-api.yaml`)
- R19. Add `change_kind: added | modified | deleted | renamed` to `FileDiff` as an **optional** field (the server always populates it on new responses; absence is tolerated for schema back-compat with pre-existing clients). Clients should consume `change_kind` when present and fall back to inferring from `old_file`/`new_file` content presence per R20.
- R20. For added files: `old_file.contents = ""` and `old_file.name = new_file.name`. For deleted files: `new_file.contents = ""` and `new_file.name = old_file.name`. For renames: both `old_file.name` and `new_file.name` populated with distinct values; contents populated.
- R21. Add **independent** booleans `truncated: bool` and `binary: bool` (both default `false`). `truncated` means size/line caps were hit (R24) or response-level budget was exhausted (R25). `binary` means content is not textual (R26). The two can coexist on a single entry.
- R22. Add `sensitive: bool` (default `false`) to mark entries skipped by the denylist (R29). Contents empty, but the name/change_kind surface so reviewers know a change happened.
- R23. Schema additions are additive: `old_file`/`new_file` stay `required` on `FileDiff` (empty-string contents represent added/deleted sides). New fields are optional for client back-compat; existing consumers that ignore them still render correctly.

**Practical limits**
- R24. Per-file cap (**Provisional, pending cap calibration below**): **256 KB** and **20,000 lines**. Over-cap entries return `truncated: true` with empty `contents`.
- R25. Aggregate response cap (**Provisional**): **5 MB** of total `contents` bytes. Once the running byte budget is exceeded, subsequent entries are returned with `name` + `change_kind` only (empty contents, `truncated: true`). The response `meta.truncated` flag also flips to `true` so the UI can show an aggregate banner.
- R26. Skip binary files: return `{name, change_kind, binary: true, contents: ""}`. Do **not** set `truncated` on the binary entry itself unless it also exceeds the per-file size cap; binary files that exhaust the aggregate budget do not retroactively change; they affect only the cap for subsequent entries.
- R27. File-count cap (**Provisional**): **200 files**. Overflow surfaces as `meta.truncated: true` + `meta.total_changed` with a persistent banner on the tab. v1 repurposes the existing `meta` object rather than implementing cursor pagination; a real cursor is deferred until a consumer needs it.
- R28. Rename detection uses `git diff --find-renames=50%`. Below threshold, changes are emitted as add + delete (no `renamed` entry). This is a git invocation contract, not a hand-rolled heuristic.

**Security & observability**
- R29. The endpoint enforces **`AuthenticatedService` authentication** (the same extractor used by other `/runs/{id}/*` endpoints). Run-ownership / workspace authorization is **not** currently enforced on sibling endpoints; adding it is a cross-cutting concern tracked separately. Unauthorized callers receive 404 (not 403) to avoid run-ID enumeration. If a workspace-membership layer is introduced later, this endpoint inherits it automatically via the shared extractor.
- R30. **Committed-only scope is the primary secret-exposure control.** Because R9 restricts the diff to committed content, `.gitignore` is not itself a mitigation (git already respects it) — the real control is the denylist (R31) plus the caller's pre-existing run-access authorization. This requirement exists to make the threat model explicit: the tab shows what a user with the same repo access would see via `git diff` locally, no more and no less.
- R31. **Sensitive-path denylist** (defense-in-depth, not a complete control): skip entries whose repo-relative paths match a built-in denylist (`.env`, `.env.*`, `*.pem`, `id_rsa*`, `.aws/credentials`, `.git/config`, `*.p12`, `*.keystore`, `*.key`). Return a placeholder entry with `sensitive: true` (R22). The denylist is owned by `fabro-server` and can be extended via config in a follow-up. Content-based secret detection is **out of scope**: a committed secret in a non-matching path is visible to anyone with run access, same as it would be via `git show` against the origin repo. The denylist narrows the accidental-exposure surface on common filenames; it is not a guarantee.
- R32. **Request budget**: hard timeout on the git subprocess (10 s in sandbox, 20 s on host), per-caller rate limit on `/runs/{id}/files` (shared with sibling run endpoints), and **request coalescing per run**: concurrent callers for the same run share a single in-flight materialization — the second caller does not kick off its own git work nor receive a 429. Stop enumeration early once the 200-file cap is hit; do not enumerate-then-truncate.
- R33. **Server-side git hardening**: the `git` process (on host for the origin path) runs with `core.hooksPath=/dev/null`, `protocol.ext.allow=never`, smudge/clean filters disabled, and no `GIT_CONFIG_*` leakage. All interpolated inputs (SHAs, paths, refs) go through `shell_quote()` per project rules. Content comes from `git show <sha>:<path>` (blob addressing), never from filesystem reads. Committed symlinks are surfaced as a file whose `contents` is the literal link-target string; they are never dereferenced. **Applicability** depends on the post-sandbox read mechanism choice (see Outstanding Questions): under a GitHub REST read path, the subprocess flags are moot and blob addressing is via the REST API.
- R34. `X-Fabro-Demo: 1` short-circuits to fixture output and must never trigger a real run lookup or sandbox/origin access. A real run ID without the header always goes through the normal auth + materialization path; the two modes cannot cross-contaminate.
- R35. Diff contents, full changed-file paths, and raw git stderr must **not** appear in `tracing` logs, Sentry breadcrumbs, or Segment events. Log only counts, sizes, durations, and run IDs. See `docs-internal/logging-strategy.md`.

**Demo mode**
- R36. Demo mode returns a static `FileDiff[]` fixture so homepage/marketing demos work. The fixture currently hardcoded in `apps/fabro-web/app/routes/run-files.tsx` (`fallbackFiles`) moves to the server's demo handler alongside the other `/runs/{id}/*` demo stubs; the frontend no longer ships a fallback.

## Success Criteria

- A typical successful run's Files tab shows the same content as `git diff <base_sha> HEAD` run against the sandbox — not `fallbackFiles`.
- A currently-running run's Files tab shows the diff as of its most recent checkpoint; a **Refresh** click re-queries the sandbox and updates within the p95 budget.
- A `Failed` run whose sandbox is still reachable shows the diff through its last checkpoint. A `Failed` run whose sandbox is gone shows the distinct empty-state message from R4, not a blank fallback.
- A completed run whose sandbox has been destroyed still shows its diff when the origin branch is present; if origin cannot be read, the UI renders the `final_patch`-derived degraded view with a visible banner.
- Pathological runs (many files, huge files, binary files) complete within **p95 < 3 s** for runs up to 200 files and 5 MB aggregate content on a warm host-side cache. First contentful paint within 500 ms of data arrival. Binary, truncated, and sensitive entries render with clear markers.
- Demo mode (`X-Fabro-Demo: 1`) still shows a recognizable fixture without touching any real run / sandbox / origin.

## Scope Boundaries

- "Steer" inline commenting is **not** in scope.
- **Uncommitted working-tree state is not surfaced in v1** — live runs show the diff as of the last checkpoint only. Revisit once @pierre/diffs and caching behavior are proven on committed state.
- No per-stage / per-checkpoint UI view; only the aggregate `base → current`. The optional `from_sha`/`to_sha` query params in R15 exist purely as forward compatibility.
- No search, filtering, grouping, or collapse UX beyond what `@pierre/diffs` provides out of the box.
- No diff download / export / copy-as-patch.
- No changes to the run lifecycle, `RunFailed` event, or the existing `final_patch` persistence.
- No live streaming updates — refresh is user-initiated via R6's Refresh control.

## Key Decisions

- **Committed-HEAD-only in v1.** Removes working-tree concurrency with the agent, removes the gitignore/secret-exfil risk unique to uncommitted content, and makes live and post-sandbox views semantically identical. Uncommitted-tree view is a deliberate v2 candidate, not a cut corner.
- **Materialize on demand + `final_patch` fallback, no new run-time persistence.** Primary path reads git (sandbox or origin). When origin cannot be read, fall back to parsing the existing `final_patch` into a degraded hunks-only view. This keeps the architecture read-heavy without duplicating content, and explicitly does not depend on origin retention as an authority.
- **Don't touch the run lifecycle.** An earlier draft proposed a push-on-failure lifecycle change; that approach is dropped. It introduced side-effect surface (ref pollution, CI triggers, crash-time credentials) that outweighed the benefit; the Failed-post-sandbox empty state is an acceptable v1 UX.
- **Extend `FileDiff` schema additively.** `change_kind`, `truncated`, `binary`, and `sensitive` are independent fields so each can be reasoned about on its own; `old_file`/`new_file` stay required for back-compat.
- **Security at the edge, not in clients.** Server-side enforces authz (R29), `.gitignore` (R30), sensitive-path denylist (R31), request budget (R32), and git-process hardening (R33). The UI assumes it receives safe data.
- **Drop `PaginationMeta` cursoring.** A single `meta.truncated: bool` + `total_changed: int` matches what v1 actually needs; a real cursor is deferred until there is a consumer.

## Dependencies / Assumptions

- `base_sha` is reliably recorded on `StartRecord` for runs that pass `RunStarted`. Runs in `Submitted`/`Starting` have no `base_sha` yet — covered by R4(a).
- The `fabro-server` process can read from origin using the same GitHub credentials resolver as `git_push_host`. The exact read mechanism (REST blob/tree API vs. a server-side bare clone vs. a shared mirror) is explicitly flagged below as Resolve-Before-Planning.
- `@pierre/diffs` accepts the existing `{old_file, new_file}` contents shape and treats the added fields (`change_kind`, `truncated`, `binary`, `sensitive`) as inert metadata the callsite can read. A quick spike confirms this before the OpenAPI change is locked.
- `reconnect_run_sandbox` in `fabro-server` dispatches to `fabro_sandbox::reconnect::reconnect`, which already supports Local, Docker, and Daytona providers (verified in `lib/crates/fabro-sandbox/src/reconnect.rs`). Live-state diff therefore works for all three. When reconnect fails at runtime for any provider, the endpoint returns R5(a) (transient, retriable) — failure is a runtime condition, not a provider-support gap.

## Outstanding Questions

### Resolve Before Planning
- [Affects R11][Needs research] **Post-sandbox read mechanism**: GitHub REST `git/trees` + `git/blobs` vs. server-side bare clone (with cache) vs. a shared mirror. Each has different latency, rate-limit, and privacy profile. Pick one before planning because it shapes the handler, caching, and failure modes. Current leaning: GitHub REST with a per-installation token scoped to the run's repo, because it avoids server-side disk state; validate with a quick LOE estimate for a bare-clone alternative.
- [Affects R24, R25, R27][Needs research] **Cap calibration**: sample P50/P95 `final_patch` sizes and file counts across recent runs before locking numeric caps. The 200 / 256 KB / 20 k / 5 MB targets are placeholders; confirm (or revise) against real data. Target: keep the truncation rate on common workflows under **5%**. Until calibrated, treat the Success Criteria p95 as conditional on whatever caps survive.
- [Affects R12][Decision] **Keep R12 (degraded patch-only view) or drop it in favor of R4(c) empty state?** R12 requires either (a) a new optional `hunks` field on `FileDiff`, or (b) a non-standard rendering in `@pierre/diffs` that may not be supported, plus a server-side patch parser. A scope-guardian review argues the empty state is sufficient; an adversarial review flags that the fallback is load-bearing only when origin retention fails (unknown frequency). Resolve before planning: run a two-day spike on (a) parse feasibility + @pierre/diffs input shape, and (b) telemetry on branch retention in the target repos. Then keep or drop R12 with evidence.

### Deferred to Planning
- [Affects R9, R10][Technical] Should the sandbox-side `git` invocation be reused via `sandbox_git::git_diff`, or does the `--find-renames` + `-z` + raw machine-readable listing suggest a sibling helper? Either is fine; pick the smaller diff.
- [Affects R12][Technical] The `final_patch`-derived degraded view needs a patch parser on the server. Is there an existing Rust patch parser we prefer (`gix-diff`, `unidiff`, hand-rolled)? Decide based on workspace dependency policy.
- [Affects R22, R31][Technical] Denylist lives in `fabro-server` initially; revisit whether it should be config-driven in a follow-up.
- [Affects R32][Technical] Concurrency cap of 1 per run: is this coarse-grained lock sufficient, or do we need request coalescing so concurrent viewers share one materialization?
- [Affects R6][Technical] "Last updated Xs ago" freshness indicator — is the timestamp the HTTP response time, or the `to_sha` commit time? Probably the former; confirm with a UX spike.
- [Affects R31][Technical] Which specific redaction-related primitives from `fabro-util` should we reuse? (Log scrubbing redaction is different from content redaction; content-redaction is explicitly **out of scope** per Key Decisions — skipping is simpler than masking.)

## Next Steps

→ `/ce:plan` for structured implementation planning.
