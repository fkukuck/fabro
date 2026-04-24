---
title: "refactor: Converge rewind into fork with archive-after"
type: refactor
status: active
date: 2026-04-23
---

# refactor: Converge rewind into fork with archive-after

## Overview

Collapse the rewind workflow operation into fork by treating rewind as `fork(source, target) + archive(source)`, wrapped in a new server-side endpoint `POST /runs/{id}/rewind` so the composition is atomic from the client's perspective and produces a single audit trail. Both operations produce new RunIds; the source run is never mutated in place. Delete the `RunRewound` event, `reset_for_rewind` projection logic, the `ensure_not_archived` guard specific to rewind, and the custom CLI event-relay plumbing.

Introduce a new `RunSupersededBy { new_run_id }` event emitted on the source when rewind archives it, so anyone reading the source's event stream can answer "why is this run archived?" without cross-correlating fork + archive events. Keep `fabro rewind` as a CLI verb — it's the semantic users reach for — but it becomes a thin wrapper around the new server endpoint.

The user-visible shift: `fabro rewind <ID> @3` now returns a new RunId and archives the source, instead of rewinding the source's branches in place. This is a semantic contract change for anyone scripting against rewind's old RunId-preservation behavior, not a pure refactor.

## Problem Frame

Rewind and fork share ~80% of their implementation (target resolution, timeline walk, branch plumbing, metadata snapshot construction) but diverge in one substantive way: rewind mutates the source run's refs in place, while fork creates a new run. That in-place mutation forces rewind to carry a large tail of special-case code:

- A dedicated `RunRewound` event
- `reset_for_rewind()` on the projection to unwind the source's terminal state so it can resume
- A `current_status` precondition check (`ensure_not_archived`) to prevent rewinding archived runs
- ~100 lines of CLI-side event relay logic (`reset_rewound_run_state`) that appends `RunRewound` + `CheckpointCompleted` + `RunSubmitted` to reconstitute the source's runnable state after the git refs move
- A server guard arm that clears `accepted_questions` on `RunRewound`

All of this exists solely to un-terminate the source run. If we archive the source and spawn a new run instead, none of it is needed — a new run starts clean by construction, and the source stays terminated.

This convergence was brainstormed conversationally on 2026-04-23 (no formal `docs/brainstorms/` document). The chosen approach is option 2 of three: rewind = fork + archive source. This preserves the user-facing distinction between fork (parallel continuation, source keeps running) and rewind (replace the path, source is abandoned), without maintaining two implementations.

## Requirements Trace

**Code Consolidation**
- R1. A single codepath creates the new run and its branches. No in-place ref mutation for rewind.

**CLI Behavior (Preserved & Changed)**
- R2. `fabro rewind <ID> <target>` archives the source run and returns a new RunId initialized at the target checkpoint.
- R3. `fabro fork <ID> [target]` continues to leave the source run untouched.
- R4. The `--list` and `--no-push` flags continue to work on both commands with unchanged semantics.

**Cleanup & Deletion**
- R5. `RunRewound` event, `RunRewoundProps`, `reset_for_rewind`, and the rewind-specific `ensure_not_archived` usage are removed from the codebase. The greenfield constraint lets us delete rather than deprecate.

**Regression Prevention**
- R6. No regression in timeline resolution (ordinal `@N`, `node`, `node@N`) or parallel-interior handling.
- R7. User-facing documentation that currently teaches in-place-rewind semantics is updated to match the new behavior (see Unit 5).

## Scope Boundaries

- **Not** adding provenance fields (`forked_from: Option<RunId>`) on forked runs. Covered for rewind by `RunSupersededBy` on the source; adding symmetric provenance on the new run is a separate follow-up covering both fork and rewind.
- **Not** changing fork's CLI surface. `ForkRunInput` and the `fabro fork` CLI continue to work unchanged. Correction from earlier plan text: **`POST /runs/{id}/fork` does not exist today** — fork is CLI-only, operating directly on the local git `Store`. Unit 2 therefore introduces the first git-touching HTTP endpoint in `fabro-server`; there is no ForkResponse shape to align RewindResponse with.
- **Not** changing `build_timeline_or_rebuild` behavior or the rebuild-from-events path. The new `GET /runs/{id}/timeline` endpoint wraps `build_timeline`; it does not modify the underlying function.
- **Not** migrating stored `RunRewound` events — greenfield, no deployed instances.
- **Not** widening `operations::archive`'s precondition. Rewind inherits the "terminal status required" rule; non-terminal sources (Paused, Blocked, Running, etc.) must be canceled or allowed to finish before they can be rewound. This is a deliberate narrowing from today's behavior — see User Decisions log.

## Context & Research

### Relevant Code and Patterns

- `lib/crates/fabro-workflow/src/operations/fork.rs` — destination op; already accepts `Option<RewindTarget>` and defaults to latest checkpoint when `None`. Reuse unchanged.
- `lib/crates/fabro-workflow/src/operations/rewind.rs` — source of shared timeline helpers to extract (`RewindTarget`, `TimelineEntry`, `RunTimeline`, `build_timeline`, `find_run_id_by_prefix`, `run_commit_shas_by_node`, `load_parallel_map`, `detect_parallel_interior`, `read_projection_at_commit`, `backfill_run_shas`). Fork already imports from this module; extraction makes the dependency explicit.
- `lib/crates/fabro-workflow/src/operations/archive.rs` — `pub async fn archive(&Database, &RunId, Option<ActorRef>) -> Result<ArchiveOutcome, Error>`. Idempotent on already-archived runs (`ArchiveOutcome::AlreadyArchived`). Returns `Precondition` error if the run is still running.
- `lib/crates/fabro-cli/src/commands/runs/archive.rs` — CLI archive wrapper. Shows the `client.archive_run(&run_id)` HTTP pattern the new rewind handler will call.
- `lib/crates/fabro-cli/src/commands/run/fork.rs` — template for the new rewind handler. Same shape: resolve run, load state, build timeline, handle `--list`, call `fork()`, print result.

### Institutional Learnings

- No relevant `docs/solutions/` entries found for rewind/fork convergence.
- Memory note: greenfield app, no migration concerns — lets us delete `RunRewound` cleanly instead of leaving it as a stub for historical replay.

### External References

Not needed. This is an internal refactor with no external contract surfaces; timeline resolution and branch manipulation already have well-tested implementations in the repo.

## Key Technical Decisions

- **Rewind becomes a server-side composite endpoint, not a CLI orchestration.** Add `POST /runs/{id}/rewind` to the fabro-api server. The handler:
  1. Loads source status from the projection store
  2. Pre-checks terminal state (rejects Running/Paused/Blocked/etc. with a clear 409 Conflict before any git work)
  3. Calls `operations::fork()` synchronously (git branch creation)
  4. Appends `RunSupersededBy { new_run_id }` to the source's event stream (async database write)
  5. Transitions source via `operations::archive()` (reuses existing archive logic)
  6. Returns `{ source_run_id, new_run_id, target, archived: true }`

  Rationale: user explicitly chose the server-side composite endpoint over CLI orchestration. Benefits: atomicity from the client's perspective, a single audit event on the source (`RunSupersededBy`) answers "why is this archived?" directly, and a future web UI has a single endpoint to call. The async/sync boundary is internal to the handler — `fork()` stays sync; the event append and archive call are async. Pre-check before fork avoids orphan runs on precondition failure; graceful degradation on post-fork archive failure is handled in Unit 3's error path. Does introduce a new endpoint that needs OpenAPI spec + progenitor regeneration.

- **Add `RunSupersededBy { new_run_id }` event (supersedes deprecated `RunRewound`).** Lives in `fabro-types::EventBody` and the `fabro-workflow::Event` enum. Emitted on the source run only, by the rewind endpoint, AFTER `operations::archive` succeeds. Projection arm on `run_state.rs` sets `superseded_by: Option<RunId>` on `RunProjection` so consumers can answer "what replaced this run?" with a single projection read (no event-log replay). Rationale: audit trail was the primary justification for the server-side endpoint; the projection field makes that audit first-class for UI/CLI consumers.

- **Retry semantics: accept orphan-run cost; clients SHOULD NOT auto-retry.** `POST /runs/{id}/rewind` is not idempotent — each call mints a fresh RunId via `fork()`. Fabro has no idempotency-key infrastructure today, and adding it for one endpoint is scope creep. Rationale: orphan runs are a known, acceptable cost; single-shot semantics from the CLI wrapper avoids the common case. A future cross-cutting idempotency-key mechanism can apply retroactively. Documented in Unit 3's CLI error-path notes ("do not retry on network error; check server state; rewind may have succeeded").

- **Shared timeline logic moves to `lib/crates/fabro-workflow/src/operations/timeline.rs`.** Naming: `timeline` = read-side (timeline parsing, target resolution, prefix lookup), `fork` = write-side (branch creation, metadata snapshot write). Rationale: `rebuild_meta.rs` already imports `RunTimeline` and `build_timeline` from rewind.rs — the `rewind` name no longer describes what's in that file.

- **Rename `RewindTarget` → `ForkTarget`.** Done as part of the module extraction so downstream renames land in one commit. Rationale: the type is now shared between fork and rewind (which is itself a fork call), and keeping the old name would imply rewind is the primary owner.

- **Delete `RunRewound` entirely.** Variant on `Event`, `EventBody::RunRewound`, `RunRewoundProps`, `"run.rewound"` discriminant. Also delete `reset_for_rewind()` on `RunProjection` and its caller in `lib/crates/fabro-store/src/run_state.rs`. Rationale: in option 2 the source run is archived, not resurrected; there is no projection state to reset. Greenfield constraint lets us delete rather than deprecate.

- **Remove `RewindInput.current_status` and the old in-place-rewind's `ensure_not_archived` call.** Rationale: in the server-endpoint design, archived-source rejection happens at `reject_if_archived` (handler step 1, 409 Conflict) and non-terminal rejection happens at the explicit status pre-check (handler step 3, 409 Conflict). The old `RewindInput.current_status` precondition is subsumed. Other `ensure_not_archived` call sites (resume, etc.) stay untouched.

- **Keep distinct rewind vs fork CLI output text.** Rewind prints "Rewound <source>... new run <new>"; fork prints "Forked <source> -> <new>". Both output the new RunId and a `fabro resume <new>` hint. Rationale: the archive-source side effect is invisible from the new-run's branches, so the message is how users learn their source was archived.

## Open Questions

### Resolved During Planning

- **Where do shared timeline helpers live?** → New `lib/crates/fabro-workflow/src/operations/timeline.rs` module.
- **Does `RewindTarget` get renamed?** → Yes, to `ForkTarget`, as part of the extraction.
- **Output text alignment with fork?** → Keep distinct. Rewind emphasizes the abandoned source; fork emphasizes the parallel continuation.
- **Archived source as rewind input?** → **Rejected with 409 Conflict** via `reject_if_archived` (mirrors archive/unarchive handlers). Users must `fabro unarchive <id>` first if they intend to rewind. This supersedes the earlier "Resolved" note that said archived-source rewinds would succeed as a no-op — that note was written for the CLI-orchestration shape and doesn't apply to the server-endpoint shape. The `ArchiveOutcome::AlreadyArchived` path is therefore unreachable from rewind; the reject-pattern fires first.

### User Decisions (recorded 2026-04-23)

**Decisions from the first pass (pre-adversarial review):**
- **Archive precondition: non-terminal sources?** → **Accept the narrowing.** Rewind now requires source to be Succeeded/Failed/Dead. Documented explicitly in Scope Boundaries.
- **Fork-then-archive half-success handling?** → **Both pre-check and graceful degradation.** Pre-check before fork; graceful degradation on post-fork archive failure.
- **Recovery scenario restructuring?** → **Split into two scenarios** (`rewind_recovers_metadata_from_real_run_state` + `fork_chain_rebuilds_metadata`).
- **Server-side endpoint vs. CLI-only?** → **Server-side composite endpoint.** Adds `POST /runs/{id}/rewind`; CLI becomes a thin wrapper.

**Decisions from the Unit 2 adversarial review:**
- **HTTP status code for partial success?** → **207 Multi-Status.** Archive-failure-after-fork returns 207 with `archived: false, archive_error: <msg>`.
- **TOCTOU race mapping (post-archive Precondition)?** → **Graceful degradation.** Treat as concurrent-mutation race, return 207 (same shape as transport failure). Not a server bug; not a 500.
- **Event ordering (RunSupersededBy vs archive)?** → **Archive first, RunSupersededBy second.** If archive fails, source is cleanly-terminal-with-missing-provenance (repairable) rather than "superseded-but-still-Succeeded" (misleading).
- **Idempotency?** → **Accept orphan-run cost; document in Key Technical Decisions.** No Idempotency-Key infrastructure. CLI is single-shot and does not auto-retry. Future cross-cutting idempotency mechanism can apply retroactively.
- **`superseded_by` projection field?** → **Add now.** `RunProjection.superseded_by: Option<RunId>` set by the RunSupersededBy event arm. Makes "what replaced this run?" a single projection read for future UI and `fabro ps` consumers.
- **Handler structure?** → **Operations-layer composite.** Business logic lives in new `operations::rewind` async function; handler is a 4-line delegator matching `archive_run`'s pattern. File `operations/rewind.rs` is repurposed, not deleted (Unit 4 updated accordingly).

**Decisions from the second external review (2026-04-24):**
- **Archived runs as rewind input?** → **Reject with 409 Conflict.** `reject_if_archived` fires at step 1; users must `fabro unarchive <id>` first. Removes the contradiction with the old "Resolved During Planning" text.
- **Event ordering invariant on failure?** → **Only emit `RunSupersededBy` if archive succeeded.** No supersede event on 207 path — preserves the ordering rationale and prevents the "superseded but still Succeeded" state the ordering was designed to avoid.
- **`AppState.repo_path` gap (P1-3)?** → **Keep server-endpoint; solve explicitly.** Handler reads `working_directory` from the run's `RunSpec` projection, opens a git Store at that path inside `spawn_blocking`. New 501 Not Implemented failure mode for runs whose working_directory isn't accessible from the server process (sandboxes, remote workers).
- **`superseded_by` plumbing?** → **Plumb through RunSummary + OpenAPI in Unit 2.** Honor the "helps fabro ps" claim by adding the field to `RunSummary`, the projection→summary mapping, and the OpenAPI schema.
- **Status code convention (412 vs 409)?** → **Use 409 Conflict** for both archived-source and non-terminal-source rejections. Matches fabro-server's consistent use of `StatusCode::CONFLICT`; error message disambiguates the two cases. No 412 in this plan.
- **Server-side timeline/list endpoint?** → **Add `GET /runs/{id}/timeline` to Unit 2.** Matches the mutating-rewind server-side move for web-UI parity; shares the working_directory/git-Store machinery with the rewind endpoint. CLI `--list` calls this endpoint instead of reading local git state.

### Deferred to Implementation

- **Exact module visibility of timeline helpers.** Some helpers (`run_commit_shas_by_node`, `find_run_id_by_prefix_opt`) are `pub(crate)` or `pub(super)` today. Reclassify during the move based on who imports from outside `operations::`.
- **Whether to delete any timeline unit tests or move them unchanged.** The rewind-specific tests (`rewind_moves_metadata_ref`, `rewind_rejects_archived_runs`) go away with the op; timeline-resolution tests (`parse_target_ordinal`, `resolve_latest_visit`, `build_timeline_simple`, `parallel_interior_detection`, `find_run_id_prefix_match`) move to `timeline.rs`. If one bleeds into the other, sort it during extraction.

### Deferred to Follow-Up

- **Provenance field `forked_from: Option<RunId>` on forked run init events.** Useful for UI (showing the fork tree) and audit trails. Would apply symmetrically to both fork and rewind. Not required for this plan — `RunSupersededBy` on the source gives half the picture; the response body of both endpoints already returns `source_run_id`. File a follow-up issue after merge.

## High-Level Technical Design

> *This illustrates the intended control flow after convergence and is directional guidance for review, not implementation specification. The implementing agent should treat it as context, not code to reproduce. Module qualifiers below reflect the pre-extraction state; after Unit 1, `build_timeline` and `ForkTarget` live in `operations::timeline`, not `operations::rewind`.*

Today's control flow:

```
fabro rewind <ID> @3              fabro fork <ID> [@3]
         |                                |
         v                                v
 rewind CLI handler             fork CLI handler
  - build_timeline                - build_timeline
  - rewind() op                   - fork() op
    - move meta ref (in place)      - create new run branch
    - move run ref (in place)       - create new meta branch
    - emit RunRewound event         - write init + checkpoint snapshots
    - emit CheckpointCompleted      - return new RunId
    - emit RunSubmitted
    - reset projection state
  - print "To resume: fabro resume <SAME_ID>"
```

After convergence:

```
fabro rewind <ID> @3                  fabro fork <ID> [@3]
         |                                    |
         v                                    v
 rewind CLI handler (thin)           fork CLI handler
  - --list: client.run_timeline(id)    - build_timeline (local git)
  - mutate: client.rewind_run(id,...)  - fork() op (local git)
         |                              - print "Forked X -> Y"
         v
 POST /runs/{id}/rewind  (server)
  - reject_if_archived                    (409 if archived)
  - load RunSpec, check terminal status   (409 if non-terminal)
  - open git Store at spec.working_directory
  - spawn_blocking: fork() op  <---------- same fork() op
                                          (501 if working_dir inaccessible)
  - operations::archive(source)     [FIRST]
  - on archive OK: append RunSupersededBy [SECOND, only if archive succeeded]
  - return 200 (archive ok) | 207 (archive failed; archived:false, no supersede)

 GET /runs/{id}/timeline (server)
  - open git Store at spec.working_directory
  - spawn_blocking: build_timeline
  - return 200 with Vec<TimelineEntryResponse> (501 if working_dir inaccessible)
```

The shared `fork()` op is the only code that creates runs, moves refs, or writes metadata snapshots. Rewind's differentiator is a server-side composite endpoint that adds a source-status pre-check, appends `RunSupersededBy` for audit, and archives the source. Fork continues to work exactly as today.

## Implementation Units

- [ ] **Unit 1: Extract timeline module and rename RewindTarget → ForkTarget**

**Goal:** Move all timeline-reading logic out of `operations/rewind.rs` into a new `operations/timeline.rs` module. Rename `RewindTarget` to `ForkTarget` in the same pass so downstream callers update once.

**Requirements:** R1 (consolidate shared code), R6 (no regression in timeline resolution)

**Dependencies:** None — this is a pure code move.

**Files:**
- Create: `lib/crates/fabro-workflow/src/operations/timeline.rs`
- Modify: `lib/crates/fabro-workflow/src/operations/mod.rs` (add `mod timeline;`, re-export from `timeline` instead of `rewind`)
- Modify: `lib/crates/fabro-workflow/src/operations/rewind.rs` (remove the extracted symbols; the `rewind()` function and its helpers stay for now)
- Modify: `lib/crates/fabro-workflow/src/operations/fork.rs` (update import: `use super::timeline::{ForkTarget, TimelineEntry, build_timeline};`)
- Modify: `lib/crates/fabro-workflow/src/operations/rebuild_meta.rs` (update imports from `rewind::` to `timeline::`)
- Modify: `lib/crates/fabro-cli/src/commands/run/rewind.rs` (update `RewindTarget` → `ForkTarget` and import path)
- Modify: `lib/crates/fabro-cli/src/commands/run/fork.rs` (update `RewindTarget` → `ForkTarget` and import path)
- Test: tests move with the code — no new test file

**Approach:**
- Symbols to move verbatim into `timeline.rs`: `RewindTarget` (renamed `ForkTarget`), `TimelineEntry`, `RunTimeline`, `build_timeline`, `backfill_run_shas`, `run_commit_shas_by_node`, `detect_parallel_interior`, `find_run_id_by_prefix`, `find_run_id_by_prefix_opt`, `load_parallel_map`, `read_projection_at_commit`
- Symbols that stay in `rewind.rs` for Unit 3 deletion: `RewindInput`, `rewind()`, `rewind_to_entry()`
- The existing `#[cfg(test)] mod tests` block in `rewind.rs` splits: timeline-parsing and resolution tests (`parse_target_ordinal`, `parse_target_latest_visit`, `build_timeline_simple`, `resolve_latest_visit`, `parallel_interior_detection`, `find_run_id_prefix_match`) move to `timeline.rs`; rewind-specific tests (`rewind_moves_metadata_ref`, `rewind_rejects_archived_runs`) stay for Unit 3 deletion.
- Visibility: `find_run_id_by_prefix_opt` is `pub(super)` today — keep `pub(super)` so it's reachable from `rebuild_meta.rs`. Adjust if rustc complains.

**Patterns to follow:**
- `lib/crates/fabro-workflow/src/operations/mod.rs` — existing re-export style (`pub use timeline::{...};`)
- No glob imports (CLAUDE.md rust import style)

**Test scenarios:**
- Happy path: `cargo build --workspace` succeeds after the move with zero behavior changes.
- Happy path: existing unit tests that move to `timeline.rs` pass unchanged against renamed `ForkTarget`.
- Edge case: `operations/rebuild_meta.rs` test `build_timeline_or_rebuild_rebuilds_missing_branch` continues to pass — verifies the new import wiring.

**Verification:**
- `cargo build --workspace` and `cargo nextest run -p fabro-workflow` both succeed.
- `rg "use .*rewind::(RewindTarget|TimelineEntry|RunTimeline|build_timeline|find_run_id_by_prefix)"` returns no matches — all call sites now import from `timeline`.
- Clippy passes: `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.

- [ ] **Unit 2: Add `RunSupersededBy` event, `POST /runs/{id}/rewind`, and `GET /runs/{id}/timeline` server endpoints**

**Goal:** Introduce the new audit event and the two server-side endpoints (mutating rewind + read-side timeline). Both endpoints share the "open git Store from run's working_directory" machinery introduced here; solving that once enables the timeline endpoint essentially for free. Web-UI parity requires both.

**Requirements:** R1 (single codepath), R2 (archive source + new RunId)

**Dependencies:** Unit 1 (needs `ForkTarget` in scope).

**Files:**
- Create event variant in `lib/crates/fabro-types/src/run_event/run.rs` — add `pub struct RunSupersededByProps { pub new_run_id: RunId, pub target_checkpoint_ordinal: usize, pub target_node_id: String, pub target_visit: usize }`. Model on `RunRewoundProps` (which is being deleted).
- Modify: `lib/crates/fabro-types/src/run_event/mod.rs` — add `RunSupersededBy(RunSupersededByProps)` variant to `EventBody`, `#[serde(rename = "run.superseded_by")]`, add `"run.superseded_by"` discriminant.
- Modify: `lib/crates/fabro-workflow/src/event.rs` — add `Event::RunSupersededBy { new_run_id, target_checkpoint_ordinal, target_node_id, target_visit }` variant, logging arm, discriminant, and `EventBody` conversion. Model on the existing `Event::RunRewound` shape (being deleted in Unit 5).
- Modify: `lib/crates/fabro-types/src/run_projection.rs` — add `pub superseded_by: Option<RunId>` field to `RunProjection`, serde-defaulted to `None`.
- Modify: `lib/crates/fabro-store/src/run_state.rs` — add `EventBody::RunSupersededBy(props) => self.superseded_by = Some(props.new_run_id);` arm. Single-line projection update; source's archived-status transition still comes from the separate `RunArchived` event per normal lifecycle.
- Modify: `lib/crates/fabro-types/src/run_summary.rs` — add `pub superseded_by: Option<RunId>` field to `RunSummary` (serde-defaulted). This is the type exposed by list endpoints (`fabro ps`, web list views), so plumbing the field here is what makes the "fabro ps shows superseded" claim honest.
- Modify: the projection→summary mapping (exact file TBD — check `fabro-server` or `fabro-store` for where `RunSummary` is built from `RunProjection`; set `summary.superseded_by = projection.superseded_by`).
- Modify: `docs/api-reference/fabro-api.yaml` around the `RunSummary` schema definition (~line 3943) — add the `superseded_by` property. Also add the new `POST /runs/{id}/rewind` path, `RewindRequest`/`RewindResponse` schemas, `RunSupersededByProps` event schema, `"run.superseded_by"` in the event-name enum, and the new `GET /runs/{id}/timeline` path + `TimelineEntryResponse` schema (see Unit 2 timeline endpoint below).
- Modify: `docs/api-reference/fabro-api.yaml` — add a new `RewindRequest` schema (with `target: Option<String>`, `push: Option<bool>` defaulting to true), a new `RewindResponse` schema (`{ source_run_id, new_run_id, target, archived, archive_error?: String }`), and a `POST /runs/{id}/rewind` path. Register `"run.superseded_by"` as an allowable event name in the SSE schema if that enum exists there.
- Create: `pub async fn rewind(...) -> Result<RewindOutcome, Error>` in `lib/crates/fabro-workflow/src/operations/rewind.rs`. This is the file's new contents — replaces the old in-place-rewind function (which is deleted in Unit 4 by virtue of not being reintroduced). Mirror the signature style of `operations::archive`. The function composes `operations::fork` (inside a `spawn_blocking` block) + `operations::archive` + `RunSupersededBy` event append (archive-first-then-supersede, only-on-archive-success).
- Create: server handler in `lib/crates/fabro-server/src/server.rs` — thin `async fn rewind_run(...)` delegator into `operations::rewind`, matching the 4-line pattern of `archive_run` (line 6448). Add route `.route("/runs/{id}/rewind", post(rewind_run))` next to `archive_run` / `unarchive_run` (see lines 1086-1087).
- Create: `pub async fn timeline(...) -> Result<Vec<TimelineEntry>, Error>` in `lib/crates/fabro-workflow/src/operations/timeline.rs` (the new module from Unit 1). This is an async wrapper around the existing sync `build_timeline` — opens the git Store from the run's working_directory (same pattern as the rewind endpoint) inside `spawn_blocking`.
- Create: server handler in `lib/crates/fabro-server/src/server.rs` — thin `async fn run_timeline(...)` delegator. Add route `.route("/runs/{id}/timeline", get(run_timeline))`. Status codes: 200 with `Vec<TimelineEntryResponse>` on success; 404 for unknown run; 501 for inaccessible working_directory.
- Modify: `lib/crates/fabro-workflow/src/event.rs` — append_event support for `RunSupersededBy` via existing event append pathway.
- Modify: `lib/crates/fabro-client/src/client.rs` — add hand-written wrappers for both new endpoints (`rewind_run`, `run_timeline`) following the archive_run wrapper pattern.
- Test: unit tests for `operations::rewind` and `operations::timeline` in their respective test modules (axum-free, covers composite branches including the 501/working_directory-inaccessible path); plus thin handler tests for HTTP-layer behavior following existing archive/unarchive test patterns.

**Approach:**
- Server handler flow (pseudo-code, directional):
  1. Parse run ID from path; reject if archived (via `reject_if_archived`, mirrors archive/unarchive).
  2. Read body → `RewindRequest { target: Option<String>, push: Option<bool> }`.
  3. Load source status from projection; reject with **409 Conflict** if not `Succeeded/Failed/Dead` (matches fabro-server's consistent use of `StatusCode::CONFLICT` for state preconditions — see multiple callers in `server.rs`). Include the canonical precondition message.
  4. **Open the git `Store` by looking up the run's working_directory.** `AppState` has no global `repo_path` — confirmed by grep: `pub struct AppState` at `server.rs:539` has no repo field. The handler loads the run's `RunSpec` from the projection store, reads `spec.working_directory` (`lib/crates/fabro-types/src/run.rs:58`), and opens a git `Store` at that path. **New precondition:** the server process must have filesystem access to the run's `working_directory`. If the path doesn't exist, isn't a git repo, or isn't accessible (e.g., the run was launched in a Daytona sandbox or on a remote worker whose filesystem isn't shared with the server), return **501 Not Implemented** with a message directing the user to the CLI rewind command for non-local runs. Exact error shape deferred to implementation, but this failure mode is documented in the error-path test scenarios. The Store must be Send + 'static so the whole git block can run inside `spawn_blocking`.
  5. **Wrap steps 5–6 in `tokio::task::spawn_blocking`** — `operations::fork` does sync libgit2 work including potential remote push, which can block for seconds. Precedent: `spawn_blocking` is the established pattern in `server.rs` (lines 1291, 1331, 1674, 1711, 4564). Running `fork()` directly on the async runtime stalls Tokio workers under load. The spawn_blocking return should carry the new_run_id back to async context.
  6. Inside spawn_blocking: build timeline (sync), resolve target (`None` defaults to latest checkpoint), call `operations::fork(...)` → `new_run_id`.
  7. Back on the async runtime: call `operations::archive(&state.store, &id, actor)` FIRST.
  8. On archive `Ok` → append `RunSupersededBy { new_run_id, ... }` to source's event stream, then return 200 with `{ source_run_id, new_run_id, target, archived: true }`. On archive `Err(Precondition)` (expected concurrent-mutation race where status changed between step 3 and step 7) or `Err(engine)` (transport/internal failure) → **return 207 Multi-Status** with `{ source_run_id, new_run_id, target, archived: false, archive_error: <message> }`. **Do NOT emit `RunSupersededBy` on archive failure** — emitting it would recreate the "superseded but still Succeeded" state the archive-first ordering exists to prevent. The response body still carries `new_run_id` so clients know about the new run; no source-side audit trail in this case, which is the honest representation of partial success. If the RunSupersededBy append itself fails after a successful archive, log the failure prominently; source is cleanly archived with missing provenance (repairable via follow-up manual append). **Invariant: `RunSupersededBy` is only on the event stream iff source is archived.**
- **Business logic should live in `operations::rewind`, not the handler.** Mirror the existing `archive_run` handler pattern (`server.rs:6448-6462`): a 4-line delegator into a `pub async fn rewind(...)` function in `fabro-workflow::operations`. Handler handles HTTP parsing, auth, and response shaping; the composite fork+archive+event-append flow lives in the ops layer and is unit-testable without axum. This changes Unit 4 from "delete `rewind.rs`" to "replace `rewind.rs` contents with the new composite op" — the file stays, its contents change. See `Files:` list below.

**Technical design:** *(directional)*

```
// Request body
struct RewindRequest {
    target: Option<String>,
    push:   Option<bool>,  // default true
}

// Response body
struct RewindResponse {
    source_run_id:  RunId,
    new_run_id:     RunId,
    target:         String,         // canonical resolved form, e.g. "@2" or "build@1"
                                    // (never None; if request.target was None, response carries
                                    //  the resolved latest-checkpoint form)
    archived:       bool,           // false iff archive step failed post-fork
    archive_error:  Option<String>, // present iff archived == false
}

// Status codes:
//   200 OK             — fork succeeded AND archive succeeded
//   207 Multi-Status   — fork succeeded AND archive failed (archived=false, archive_error set)
//   400 Bad Request    — target out of range, malformed request
//   404 Not Found      — run id unknown
//   409 Conflict       — source already archived OR source not terminal
//                        (matches fabro-server's consistent CONFLICT convention;
//                         error message disambiguates the two cases)
//   501 Not Implemented — run's working_directory not accessible from the server
//                         process (remote worker, container sandbox, missing path)
```

**Patterns to follow:**
- `lib/crates/fabro-server/src/server.rs:6448` (`archive_run`) and `:6456` (`unarchive_run`) — handler shape, `reject_if_archived` gate, actor extraction, `operations::archive` integration.
- `lib/crates/fabro-workflow/src/operations/fork.rs` — called as-is (sync, in-handler).
- `lib/crates/fabro-workflow/src/operations/archive.rs:53-95` — called as-is (async).
- `lib/crates/fabro-server/src/server.rs:6058` (`reject_if_archived`) — precondition pattern.
- `lib/crates/fabro-server/src/server.rs:6037-6053` (`denied_lifecycle_event_name`) — update: `RunSupersededBy` is a server-emitted event, so the rewind endpoint is its legitimate injection point. Comment should note this.

**Test scenarios:**
- Happy path: POST `/runs/{terminal_id}/rewind` with `{target: "@2"}` returns 200 with `{source, new, target, archived: true}`; source event log shows `RunArchived` then `RunSupersededBy` (archive-first ordering); source projection has `superseded_by: Some(new_run_id)`; source `RunSummary` exposes the same field; new run has its own initialized branches.
- Happy path (timeline): GET `/runs/{id}/timeline` returns 200 with a `Vec<TimelineEntryResponse>` matching the ordered checkpoints in the run's metadata branch.
- Happy path default: POST with no `target` field rewinds to the latest checkpoint.
- Happy path: POST with `push: false` skips remote push; archive still occurs.
- Error path: POST on a `Running` source → 409 Conflict with "must be terminal" message; NO new run created (pre-check blocks before fork).
- Error path: POST on an `Archived` source → 409 Conflict via `reject_if_archived`; no new run.
- Error path: POST on unknown run ID → 404.
- Error path: target `@99` out of range → fork error surfaces as 400 Bad Request; no archive attempt; source unchanged.
- Edge case: archive fails after fork (simulate via fault injection on the archive call — not via "archive source first", which is blocked by `reject_if_archived` before fork even runs) → returns **207 Multi-Status** with `archived: false, archive_error: <msg>`; new run is intact; source event log does **NOT** carry `RunSupersededBy` (only-on-archive-success rule).
- Edge case: source status changes between pre-check and archive (TOCTOU race, simulate with a concurrent event append) → archive returns `Err(Precondition)`; endpoint returns 207 (same shape as transport failure), NOT 500. Source event log does NOT carry `RunSupersededBy`.
- Edge case: `RunSupersededBy` append fails after archive succeeds (simulate storage error) → response is still 200 with `archived: true`; source is cleanly archived but provenance is missing in its event log. Log the append failure prominently; this is a repairable degradation.
- Error path: source already archived → `reject_if_archived` returns 409 before handler business logic runs; no fork attempt.
- Error path: source `working_directory` is not accessible (simulate by passing a path the server can't stat) → 501 Not Implemented with guidance to use the CLI rewind command.
- Integration: full CLI → server → git path in a CLI-level or scenario test (covered in Unit 5).

**Verification:**
- `cargo nextest run -p fabro-server` passes.
- `cargo build -p fabro-api` regenerates types cleanly after OpenAPI changes.
- Conformance test `fabro-server` run-catches-spec-drift (per CLAUDE.md API workflow) passes.
- `rg -n 'run\.superseded_by' lib/crates/ docs/api-reference/` finds matching wire identifiers in at least `fabro-types`, `fabro-workflow`, and `fabro-api.yaml`.

- [ ] **Unit 3: Rewrite `fabro rewind` CLI as a thin wrapper around the new endpoint**

**Goal:** Replace the current in-place rewind logic in the CLI handler with a single call to the new server endpoint, plus timeline-listing and output formatting. Output text continues to use "rewind" vocabulary.

**Requirements:** R2, R4 (`--list` / `--no-push` unchanged)

**Dependencies:** Units 1 and 2 (needs `ForkTarget` in scope, needs the server endpoint and generated client method).

**Files:**
- Modify: `lib/crates/fabro-cli/src/commands/run/rewind.rs` (full rewrite)
- Modify: `lib/crates/fabro-client/src/client.rs` — add hand-written wrapper `pub async fn rewind_run(&self, run_id: &RunId, req: &RewindRequest) -> Result<RewindResponse>` matching the style of existing `archive_run`/`unarchive_run` wrappers around the progenitor-generated call.
- Test: `lib/crates/fabro-cli/tests/it/cmd/rewind.rs` (assertions rewritten in Unit 5)

**Approach:**
- Mirror the shape of `lib/crates/fabro-cli/src/commands/run/fork.rs` for origin validation, but:
  - `--list` path: call `client.run_timeline(&run_id)` (the new endpoint) instead of reading local git state. This matches the mutating rewind's server-side move. Falls back gracefully with a helpful message if the endpoint returns 501 — but the common case (local runs) works through the server.
  - Non-list path: parse target, build `RewindRequest`, call `client.rewind_run(&run_id, &req)`, handle response.
- Delete the helpers `reset_rewound_run_state`, `restored_checkpoint_event`, `run_event` (and their `RunRewoundProps`/`CheckpointCompletedProps`/`RunSubmittedProps` imports). They have no consumer after this unit.
- Keep `print_timeline` and `timeline_entries_json` — `fork.rs` imports them; they now format data that arrived from the server, not data built locally.
- Output text format: `"Rewound {source[:8]}; new run {new[:8]}"` followed by `"To resume: fabro resume {new[:8]}"`. On HTTP 207 (`archived == false`), also print `"Warning: source not archived: {archive_error}. Run `fabro archive {source}` to finish."` so the user knows the source is still terminal-but-not-archived and how to clean up.
- JSON output: echo `response` shape plus the HTTP status code so scripts can branch on 200 vs 207 without re-parsing.
- **Retry posture: single-shot.** The CLI does NOT auto-retry `POST /rewind` on network error, timeout, or 5xx. On any non-response failure, print `"Network error during rewind. Check server state with 'fabro ps' before retrying — the rewind may have succeeded."`. Rationale: fork mints a fresh RunId each call, so naive retry creates orphans. See "Retry semantics" key decision.
- CLI no longer calls `fork()` directly; that's entirely server-side now.
- Git `Store` access stays CLI-side for the `--list` path (timeline display reads local git state). Origin validation (`ensure_matching_repo_origin`) still runs client-side.

**Patterns to follow:**
- `lib/crates/fabro-cli/src/commands/run/fork.rs` — same shape for `--list` path.
- `lib/crates/fabro-cli/src/commands/runs/archive.rs:70` — `client.archive_run(&run_id).await` call site pattern, will mirror `client.rewind_run(&run_id, &req).await`.
- `lib/crates/fabro-client/src/client.rs:725` (existing `archive_run` wrapper) — location and style for the new `rewind_run` wrapper.

**Test scenarios:**
- Happy path: `fabro rewind <ID> @2 --no-push` on a succeeded run exits 0, stderr contains "Rewound" and the new RunId prefix; source run transitions to `Archived` (via server); source event log shows `RunArchived` then `RunSupersededBy`; new run branches exist locally after the server's fork push/update.
- Happy path (JSON): `--json` emits `{source_run_id, new_run_id, target, archived: true}` with both IDs resolvable.
- Edge case: `fabro rewind <ID>` (no target, no `--list`) prints the timeline via `GET /runs/{id}/timeline`; no mutation.
- Edge case: `fabro rewind <ID> --list` prints the timeline via `GET /runs/{id}/timeline`; source unchanged.
- Edge case: `--no-push` translates into `push: false` in the request body; server honors it.
- Error path: target `@99` out of range → server returns 400; CLI prints the error; source unchanged.
- Error path: source run is still running or paused → server returns **409 Conflict** with "must be terminal" message; CLI prints it clearly; no new run anywhere.
- Error path: source already archived → server returns 409 Conflict; CLI prints "run is archived; run `fabro unarchive` first and retry"; no new run.
- Edge case: server returns 207 Multi-Status with `archived: false, archive_error: "..."` → CLI prints the new RunId, the archive-failure warning with the `fabro archive <source>` hint, and exits 0 so scripts can still pick up the new RunId.
- Edge case: server returns 501 Not Implemented (working_directory inaccessible) → CLI prints a clear message suggesting checkout-to-local-path-and-retry; exits non-zero.
- Edge case: network error or timeout during POST /rewind → CLI exits non-zero with the "check server state" message; does NOT auto-retry.
- Integration: after `rewind <ID> @2`, `fabro ps` shows source as Archived and the new RunId present and resumable.

**Verification:**
- `cargo nextest run -p fabro-cli` passes with Unit 5's updated assertions.
- `fabro rewind --help` output unchanged (args struct untouched).
- The CLI-snapshot test `rewind_target_updates_metadata_and_resume_hint` passes against new output text.

- [ ] **Unit 4: Delete RunRewound event, in-place rewind op, and projection reset plumbing**

**Goal:** Remove every code path that existed solely to support in-place rewind. Compile cleanly. Note: `rewind.rs` the file STAYS — Unit 2 replaced its contents with the new composite `operations::rewind` function. This unit deletes the old in-place `rewind()` body and associated wire-contract types, not the file.

**Requirements:** R5 (delete all RunRewound plumbing)

**Dependencies:** Units 1, 2, and 3 (nothing should import `rewind()` or reference `RunRewound` after those units; this unit verifies and deletes).

**Files:**
- Modify: `lib/crates/fabro-workflow/src/operations/rewind.rs` — confirm the in-place `rewind()` function, `RewindInput`, `rewind_to_entry`, and the `ensure_not_archived` precondition call are all gone. After Unit 2 the file contains only the new composite `pub async fn rewind(...)` and its helpers.
- Modify: `lib/crates/fabro-workflow/src/operations/mod.rs` — update the `rewind::` re-export block to expose the new composite function (`pub use rewind::{rewind, RewindInput, RewindOutcome};`) rather than the old one. Old `RewindTarget`/`TimelineEntry`/`RunTimeline`/`build_timeline`/`find_run_id_by_prefix` re-exports move to `timeline::` per Unit 1.
- Modify: `lib/crates/fabro-workflow/src/event.rs` — delete `Event::RunRewound` variant, its logging arm (~line 613), its `"run.rewound"` discriminant (~line 1178), and its `EventBody::RunRewound` conversion (~line 1586)
- Modify: `lib/crates/fabro-types/src/run_event/mod.rs` — delete `EventBody::RunRewound(RunRewoundProps)` variant (~line 128), its `"run.rewound"` discriminant (~line 393), AND the `"run.rewound"` string-match arm at line 524. Confirmed sites: `rg -n 'run\.rewound|RunRewound' lib/crates/fabro-types/src/run_event/mod.rs` returns lines 127, 128, 393, 524 — all four must go.
- Modify: `lib/crates/fabro-types/src/run_event/run.rs` — delete `pub struct RunRewoundProps` (~lines 90-99)
- Modify: `lib/crates/fabro-types/src/run_projection.rs` — delete `pub fn reset_for_rewind(&mut self)` (~lines 134-149)
- Modify: `lib/crates/fabro-store/src/run_state.rs` — delete the `EventBody::RunRewound(_) => self.reset_for_rewind()` arm (~lines 170-172)
- Modify: `lib/crates/fabro-server/src/server.rs` — drop `| EventBody::RunRewound(_)` from the `reconcile_live_interview_state_for_event` match (~line 3172); update the comment at line 6043 about what flows through `append_run_event`
- Test: no new tests — deletion only. Tests validating the deletion are in Unit 5.

**Approach:**
- This unit is mostly deletion. Run it last among the code-change units.
- Keep `ensure_not_archived` and `archived_rejection_message` in `archive.rs` — they're used by resume and by server guards, not just rewind.
- `operations::rewind` (the file) stays and now holds the composite op from Unit 2 — DO NOT delete the file.
- Before deleting `RunRewoundProps`, confirm: `rg "RunRewound"` returns only the planned deletion sites (the new event is `RunSupersededBy`, not a rename).
- **Symmetry check for `reset_for_rewind` deletion.** That method clears 13 fields on `RunProjection`. Its deletion is safe only if forked-run initialization starts clean equivalently. `fork.rs:92-100` uses `RunProjection::default()` and populates only `spec`, `graph_source`, `start`, `sandbox` — strictly cleaner than `reset_for_rewind` produces. The one deliberate carry-over is `sandbox` (correct: forked run should share the source's sandbox environment). Walk the field list once before deleting to verify no drift has been introduced since this plan was written.
- **`reset_for_rewind` deletion is reversible via git history.** If a future op requires un-terminating a projection (manual recovery tooling, undo-archive flow), reintroduce the method from git rather than carrying dead code now.

**Patterns to follow:**
- Matches the clean-deletion pattern used in recent refactors — e.g., the approach in `docs/plans/2026-04-23-003-refactor-pr-commands-server-side-plan.md` for removing obsolete code paths.

**Test scenarios:**
- Test expectation: none — pure deletion. Correctness is proven by the full workspace compiling and by Unit 5's updated tests passing.

**Verification:**
- `cargo build --workspace` succeeds.
- `cargo nextest run --workspace` passes.
- `rg "RunRewound|reset_for_rewind|RunRewoundProps"` returns zero hits.
- `rg "operations::rewind"` returns zero hits.

- [ ] **Unit 5: Update tests for new rewind semantics**

**Goal:** Rewrite tests that asserted old in-place rewind behavior to assert the new fork-and-archive semantics. Split the recovery scenario into two focused scenarios. Delete tests for behavior that no longer exists.

**Requirements:** R2, R4, R6 (verify behavior preserved where it should be; verify changed where it should be)

**Dependencies:** Units 1, 2, 3, 4 complete.

**Files:**
- Modify: `lib/crates/fabro-cli/tests/it/cmd/rewind.rs` (rewrite assertions; preserve `--help` snapshot structure)
- Modify: `lib/crates/fabro-cli/tests/it/cmd/resume.rs` — two tests use the old `rewind <source> ... resume <source>` (same RunId) pattern and will break under new semantics:
  - `resume_rewound_run_succeeds` (~line 61) — rewrite to capture the new RunId from rewind stderr/JSON and resume *that* id.
  - `resume_detached_does_not_create_launcher_record` (~line 125) — same pattern; same rewrite.
- Modify: `lib/crates/fabro-cli/tests/it/scenario/recovery.rs` — delete `rewind_and_fork_recover_missing_metadata_from_real_run_state` and split into two focused scenarios:
  - `rewind_recovers_metadata_from_real_run_state` — run a workflow, fork it, rewind the fork (new endpoint), verify the new-from-rewind run has the correct metadata and resumability.
  - `fork_chain_rebuilds_metadata` — run a workflow, fork, fork the fork, verify metadata reconstruction across the chain (no rewind involved).
- Modify: `lib/crates/fabro-store/src/run_state.rs` — delete any test that seeded a `RunRewound` event (none found in grep, but re-verify during implementation)

**Approach:**
- In `tests/it/cmd/rewind.rs`:
  - `rewind_outside_git_repo_errors` — unchanged.
  - `rewind_list_prints_timeline_for_completed_git_run` — unchanged (list path unmodified).
  - `rewind_target_updates_metadata_and_resume_hint` — rewrite. New assertions: (1) command succeeds; (2) stderr includes "Rewound" and "To resume: fabro resume"; (3) the resume hint points at a new RunId (not `setup.run.run_id`); (4) source run is now Archived. Drop the old assertion that the source's metadata ref moved.
  - `rewind_preserves_event_history_and_clears_terminal_snapshot_state` — delete. This test asserted `run.rewound` + `checkpoint.completed` + `run.submitted` event append and projection reset, all of which no longer happen. Replace with a test that asserts BOTH sides explicitly: (1) source event log gains exactly two new events in order: `run.archived` then `run.superseded_by` (matches the archive-first ordering and the only-on-archive-success rule); (2) the new run's event log contains the expected init events in order (`run.submitted`, `checkpoint.completed` from the target checkpoint), with the exact expected event count. The original test's event-count-delta assertion is the kind of coverage that catches helper-function run_id-mixup bugs; preserve that discipline in the rewrite.
- In `tests/it/scenario/recovery.rs`:
  - Delete the existing `rewind_and_fork_recover_missing_metadata_from_real_run_state`.
  - Add `rewind_recovers_metadata_from_real_run_state` — runs a workflow, forks from a checkpoint, rewinds the fork (hits the new endpoint), captures the new RunId from the response/output, asserts metadata-branch + run-branch are present for the new RunId and that `fabro resume <new>` can pick up the work.
  - Add `fork_chain_rebuilds_metadata` — runs a workflow, forks, forks again; asserts metadata rebuild across the two-step fork chain. Contains no rewind, so no dependency on the new endpoint.
- Delete snapshot files referenced by deleted/rewritten tests: `cargo insta pending-snapshots` after test changes, then `cargo insta accept --snapshot <path>` per-file after verifying.

**Patterns to follow:**
- `lib/crates/fabro-cli/tests/it/cmd/fork.rs` — mirror fork's assertion style for new-RunId verification (confirmed present at implementation time).
- Snapshot-test discipline per CLAUDE.md: check `cargo insta pending-snapshots` before accepting.

**Test scenarios:**
- Happy path: `rewind_target_creates_new_run_and_archives_source` — run rewind, assert new RunId in output, assert source status is Archived, assert source's event log gains exactly two events in order: (1) `RunArchived`, (2) `RunSupersededBy` (archive-first ordering). Assert source `RunProjection.superseded_by == Some(new_run_id)` and `RunSummary.superseded_by == Some(new_run_id)`. Assert new run has init + checkpoint events.
- Edge case: `rewind_list_unchanged` — `--list` still prints timeline without side effects (no server call).
- Edge case: `rewind_with_no_target_prints_timeline` — no-target invocation behaves like `--list`.
- Edge case: `rewind_no_push_skips_remote_but_still_archives` — `--no-push` translates to `push: false` on the request; source is still archived via the server endpoint.
- Error path: `rewind_target_out_of_range_does_not_archive` — bad target → server 400; source remains in original (non-archived) status; no new run branches created.
- Error path: `rewind_non_terminal_source_rejected` — source is still running/paused → server 409 Conflict with "must be terminal" message; no new run.
- Edge case: `rewind_graceful_degradation_on_archive_failure` — simulate archive failure (e.g., by archiving the source manually first so the precondition short-circuits) → CLI prints new RunId with warning; exit code 0.
- Integration: `recovery.rs` scenarios above — rewind then resume the new RunId; fork chain rebuilds metadata.

**Verification:**
- `cargo nextest run -p fabro-cli -p fabro-server` passes.
- `cargo insta pending-snapshots` is empty after acceptance.
- No test references `RunRewound`, `reset_for_rewind`, or `ensure_not_archived` in a rewind-specific context.

- [ ] **Unit 6: Update user-facing documentation for new rewind semantics**

**Goal:** Replace the "in-place destructive rewind" mental model in shipped user docs with the "rewind produces a new run from a prior checkpoint and archives the source" model. Add a changelog entry so users learn of the semantic shift.

**Requirements:** R7 (docs match behavior)

**Dependencies:** Units 1-5 complete and merged. Docs should describe the shipped behavior, not the planned behavior.

**Files:**
- Modify: `docs/execution/checkpoints.mdx` (lines 140-159 describe rewind; rewrite "resume from the same RunId" flow to "resume from the new RunId printed by rewind"; rewrite fork-vs-rewind contrast to "fork keeps both, rewind archives the source")
- Modify: `docs/reference/cli.mdx` (rewind CLI reference entry around line 584; remove "resets the original run in place" language; document the new output format including the `source_run_id` + `new_run_id` JSON fields)
- Create: `docs/changelog/<date>.mdx` — single entry announcing that `fabro rewind` now creates a new run and archives the source, replacing in-place rewind. Include a migration note for any scripts that parse rewind output.

**Approach:**
- Audit first: `rg -i "rewind|rewound" docs/ apps/` to confirm the file list. Ignore changelog history entries (they correctly describe behavior at their own date).
- Keep the `fabro rewind` CLI as the documented verb for "try from earlier checkpoint" — the semantic-name preservation is deliberate. Update the explanation of what it does, not the name.
- Mention in the docs that the source run is archived (not lost) and can be unarchived with `fabro unarchive` if needed.

**Patterns to follow:**
- Existing `docs/changelog/*.mdx` format for the new entry.
- Mintlify docs conventions elsewhere in `docs/`.

**Test scenarios:**
- Test expectation: none — documentation-only change, no executable behavior.

**Verification:**
- Mintlify docs dev server renders the updated pages without warnings (`docker run ... mintlify dev` per CLAUDE.md).
- Manual read-through: the new text accurately describes the Unit 2 CLI output format and the source-is-archived behavior.
- `rg -i "in[ -]place|destructive" docs/execution/ docs/reference/` returns no rewind-related hits after the change.

## System-Wide Impact

- **Interaction graph:** Rewind is now a single HTTP call from the CLI (`POST /runs/{id}/rewind`) that atomically composes fork + archive server-side. Pre-check before fork eliminates the precondition half-success case; transport-level archive failure is handled by the endpoint returning `archived: false, archive_error: ...` so the CLI can surface the warning while still delivering the new RunId.
- **Error propagation:** Fork errors surface as server 400. Archived source returns 409 (via `reject_if_archived`). Non-terminal source returns 409 (pre-check in handler; disambiguated in error body). Inaccessible `working_directory` returns 501. Post-archive Precondition errors (concurrent-mutation race) return 207 Multi-Status, same as transport failures — NOT 500. Archive errors are degradations, not bugs.
- **State lifecycle:** Source run transitions `Succeeded/Failed/Dead → Archived` via the existing archive pipeline. On success, `operations::archive` runs FIRST; then the server appends `RunSupersededBy { new_run_id }`. Event log reads `RunArchived, RunSupersededBy`. Ordering rationale: if RunSupersededBy fails after archive, source is cleanly archived with missing provenance (repairable). If we reversed, an archive failure after a supersede-append would leave source "superseded but still Succeeded" — a misleading projection state. Projection captures `superseded_by: Some(new_run_id)` so UIs/CLI can answer "what replaced this?" without event-log replay.
- **Event stream consumers:** `RunRewound` disappears from the event stream; `RunSupersededBy` appears. Any UI element, log filter, or downstream consumer that matched `"run.rewound"` will break. Per memory, this is greenfield with no deployed consumers — confirm during implementation that no docs/web consumers reference the old event name: `rg -i rewound docs/ apps/ lib/packages/` should return only documentation strings destined for update in Unit 6.
- **API surface parity:** `docs/api-reference/fabro-api.yaml` gets two additions (`POST /runs/{id}/rewind` endpoint with `RewindRequest`/`RewindResponse` schemas, and `"run.superseded_by"` event name in the SSE schema) and zero deletions — the spec does not currently reference rewound (verified: `rg -c rewound docs/api-reference/fabro-api.yaml` = 0). Regenerate the Rust client and TypeScript client per CLAUDE.md "API workflow" after spec edits.
- **Integration coverage:** The `recovery.rs` scenarios (post-split) are the main integration tests that cross the CLI / server / git boundary. Unit 5 covers them.
- **Unchanged invariants:** `operations::fork`, `operations::archive`, `operations::unarchive`, `operations::resume`, and the `ensure_not_archived` guards on non-rewind paths (e.g., resume) stay exactly as they are. The fork op's public signature is unchanged.

## Risks & Dependencies

| Risk | Mitigation |
|------|------------|
| Users/scripts relying on rewind preserving the source RunId break silently. | Output text explicitly states "new run <id>" so the change is loud; `--json` output includes both `source_run_id` and `new_run_id` so scripts can adapt without parsing prose. User-facing docs are updated in Unit 6 so the documented contract matches new behavior. |
| Fork-succeeded-then-archive-failed leaves an extra run on the server. | Pre-check before fork eliminates the precondition-failure case. Transport-level archive failures produce `archived: false` in the response so the CLI can surface a warning while still giving the user the new RunId. Archive is idempotent — retrying the CLI command against the same source archives it cleanly on the second attempt. |
| Recovery scenario changes miss a subtle assertion. | Unit 5 splits into two focused scenarios and explicitly asserts new-RunId resumability and the event count delta. Run locally before merging. |
| Stale `insta` snapshots silently accept changed output. | Follow CLAUDE.md discipline: `cargo insta pending-snapshots` before `cargo insta accept`; accept per-file, never globally. |
| Archive precondition rejects non-terminal runs that `ensure_not_archived` used to allow. | Resolved via User Decisions: accept the narrowing. Documented in Scope Boundaries and the CLI error message; users who need to rewind a paused/blocked run cancel-or-kill it first. |
| OpenAPI spec drift after adding the endpoint and event. | `fabro-server` conformance test catches router/spec divergence. Regenerate both Rust and TypeScript clients immediately after spec edits; commit the generated updates in the same commit as the spec changes. |
| New `RunSupersededBy` event shape conflicts with fabro-web or external SSE consumers. | Search `apps/fabro-web` and any external consumer repos for `run\.rewound` and related event-name strings before merging. Currently greenfield, but a one-line grep keeps the assumption honest. |
| Server-side rewind/timeline endpoints reject runs whose `working_directory` isn't server-accessible (501). | Documented explicitly in error-path scenarios. CLI surfaces the 501 clearly and suggests checkout-to-local-path as the workaround. In practice, most runs today are local. Remote/sandbox runs are a future concern that may need a streaming-fork-from-client protocol. |
| `RunSupersededBy` omitted on 207 leaves source with no source-side audit trail of the rewind. | Accepted trade-off per event-ordering-invariant decision. Response body still carries `new_run_id`, so forward-direction audit (new→source) is available via the deferred `forked_from` provenance follow-up. Backward direction (source→new) is only available on archive success — which is the common case. |

## Documentation / Operational Notes

- User-facing docs teach the old in-place-rewind model explicitly and must be updated (see Unit 5):
  - `docs/execution/checkpoints.mdx:140-159` — documents `fabro rewind <RUN_ID>` followed by `fabro resume <RUN_ID>` using the same ID; contrasts rewind (destructive, resets original) against fork (independent copy).
  - `docs/reference/cli.mdx` — CLI reference entry for `fabro rewind`; lines around 584 contrast rewind vs. fork as in-place-reset vs. independent-copy.
  - Changelog entries: `docs/changelog/2026-03-14.mdx:26-34` and `docs/changelog/2026-03-15.mdx:8` are historical and can stay, but a new changelog entry for this semantic change is required.
- **OpenAPI spec + client regeneration.** Unit 2 adds `POST /runs/{id}/rewind` with `RewindRequest`/`RewindResponse` schemas and the `"run.superseded_by"` event name to `docs/api-reference/fabro-api.yaml`. After spec edits, rerun `cargo build -p fabro-api` (progenitor regenerates Rust types + reqwest client) and `cd lib/packages/fabro-api-client && bun run generate` (openapi-generator regenerates TS client). The `fabro-server` conformance test catches spec/router drift — run it locally after the endpoint is wired.
- No rollout concerns — greenfield, no migration.

## Sources & References

- Conversational brainstorm on 2026-04-23 (this session). Option 2 selected: rewind = fork + archive source. Elevated to server-side composite endpoint per user decision during document-review.
- Related code:
  - `lib/crates/fabro-workflow/src/operations/fork.rs` (destination op, called server-side in Unit 2)
  - `lib/crates/fabro-workflow/src/operations/rewind.rs` (source of extraction + deletion)
  - `lib/crates/fabro-workflow/src/operations/archive.rs` (archive op, called server-side in Unit 2)
  - `lib/crates/fabro-cli/src/commands/run/rewind.rs` (CLI thin-wrapper rewrite target)
  - `lib/crates/fabro-cli/src/commands/run/fork.rs` (pattern template for `--list` path)
  - `lib/crates/fabro-server/src/server.rs` — archive_run (line 6448) and unarchive_run (line 6456) as server-handler template; route registration (line 1086); `reject_if_archived` precondition (line 6058)
  - `lib/crates/fabro-client/src/client.rs` — existing archive_run/unarchive_run wrappers as template for new `rewind_run` wrapper
  - `docs/api-reference/fabro-api.yaml` — OpenAPI spec, source of truth for new endpoint and event name
- Related plans:
  - `docs/plans/2026-04-22-003-refactor-lock-down-server-secrets-plan.md` (recent refactor precedent for wire-contract cleanup)
