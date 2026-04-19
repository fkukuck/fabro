---
date: 2026-04-19
topic: run-archived-status
---

# Add a Terminal `archived` Status to Workflow Runs

## Problem Frame

Workflow runs accumulate over time. Today the three terminal statuses (`succeeded`, `failed`, `dead`) all share the same shelf: they stay visible in `fabro ps`/`fabro runs list` with `-a`, clutter API listings, and carry no signal about whether the user has actually finished with the run. Users want a way to mark a run "I've reviewed this, no further action needed" that (a) captures that intent explicitly, (b) drops the run out of default listings, and (c) leaves all data readable for reference. The mark is also the natural signal an eventual retention/delete policy would key off, though deletion itself is out of scope here.

## Behavior by Status

| Operation | Active (submitted/starting/running/paused) | Terminal (succeeded/failed/dead) | Archived |
|---|---|---|---|
| Read detail, logs, artifacts, attach-replay | ✓ | ✓ | ✓ |
| Resume / rewind / retro re-run | ✓ where valid | ✓ where valid | ✗ (unarchive first) |
| Archive | ✗ (finish first) | ✓ | — |
| Unarchive | — | — | ✓ (restores prior terminal status) |
| Shown in default `fabro ps` / `runs list` | ✓ | ✗ | ✗ (hidden; opt-in flag) |

## Requirements

**Status model**
- R1. Add `archived` as a new terminal variant of `RunStatus`, surfaced as the run's `status` field in CLI, API, and UI consumers.
- R2. Archiving is permitted only from existing terminal statuses: `succeeded`, `failed`, `dead`. Archiving an active run is rejected.
- R3. Archiving is reversible. `unarchive` restores the exact terminal status the run held immediately before it was archived.
- R4. The prior terminal status must be retained durably — either captured in the unarchive event payload or persisted on the run record — so unarchive is deterministic across restarts, reloads, and event replay.
- R5. All three source terminal statuses archive identically — no per-source gating, reasons, or side effects differ between `succeeded`/`failed`/`dead`.

**Operations**
- R6. `fabro archive <run_id> [<run_id>...]` archives one or more runs in a single invocation. Per-id failures are surfaced individually; the batch does not abort on the first error.
- R7. `fabro unarchive <run_id> [<run_id>...]` reverses the archive for one or more runs in a single invocation, with the same per-id failure semantics as R6.
- R8. HTTP API exposes archive and unarchive operations and supports the same multi-id bulk behavior the CLI uses.
- R9. Archive and unarchive appear on the run-timeline event stream with the actor that performed the action, so the transition is captured for audit alongside existing status events. Whether this is implemented as new `RunArchived`/`RunUnarchived` event variants or by extending the existing status-transition event family is deferred to planning.

**Visibility**
- R10. `fabro ps` and `fabro runs list` hide archived runs by default. An explicit flag opts them in; the exact flag shape (extend `-a/--all` vs. add `--archived`) is a planning decision.
- R11. The JSON listing API exposes an equivalent archived-visibility control so API consumers opt in to archived runs explicitly and don't get surprised by a new default set.
- R12. In v1, direct run-detail reads by run ID continue to succeed regardless of archived state — archive is a listing-filter concern, not an access-control one. Future retention/deletion layers may reintroduce access-control semantics on top of archive; v1 does not foreclose that.

**Safety of archived runs**
- R13. Archived runs are read-only (see R14 for the mutation-rejection contract). Read surfaces — viewing detail, logs, artifacts, attach-replay, checkpoint inspection — all continue to work as they did in the prior terminal status.
- R14. Any operation that would mutate a run — lifecycle (resume, rewind, retro re-run, run-control actions, further status transitions) *and* metadata (label edits, tag changes, any future annotation/comment surface) — must reject with a clear error that tells the user to `fabro unarchive` first. Archived is a fully frozen state from a user perspective; R13's "read-only" is defined by this rejection list. The one preserved escape hatch is the system's existing "always-allow `Dead`" path in `can_transition_to` — supervisor/orphan handling may still mark an archived run `Dead` (with the archive then effectively discarded). The exhaustive endpoint-level audit is a planning task (see Deferred Questions).
- R15. Archiving does not change storage, retention, or cleanup behavior. Checkpoints, artifacts, logs, and blob references remain as they were in the prior terminal status.
- R16. When a consumer explicitly asks for archived runs — either by the archived-visibility opt-in flag, or (if/when a status filter is introduced) by `status=archived` — the explicit ask is sufficient; consumers should not need to *also* set a second gate. Note: today neither the CLI (`RunsListArgs`) nor the `listRuns` API exposes a generic `status=` filter; introducing one is not a v1 requirement, but if it arrives later it should interact with the archived opt-in per this rule.

## Success Criteria

- A user runs `fabro archive <id>`. Listings that would previously surface the run's terminal status (e.g. `fabro ps -a`, `fabro runs list -a`, or the JSON list API) no longer include it. A listing with the archived-opt-in flag surfaces it with `status = archived`.
- `fabro unarchive <id>` returns the run to its original terminal status; listings and behavior match the pre-archive state exactly.
- Attempting any mutating operation on an archived run produces an actionable error referencing `unarchive`.
- Archive/unarchive actions appear in the run's event timeline with actor information.
- Bulk form (`fabro archive A B C`) archives each independently and surfaces per-run success/failure without aborting the whole batch.

## Scope Boundaries

- Web UI archive/unarchive controls — deferred; CLI + API only in v1. The web UI is itself an API consumer, so it will pick up the new default-hide listing behavior passively; no UI code change is required in v1 to keep that surface consistent.
- Automatic archive policies (age, count, per-workflow retention) — deferred.
- Storage reduction tied to archive (checkpoint pruning, log compaction, artifact eviction) — explicitly out of scope.
- Deletion of archived runs — out of scope; archived is a signal that deletion *could* later key off, not a step toward it in this feature. Making archived runs efficiently queryable as a set (needed for a future retention job) is not a v1 requirement beyond what R16 already provides.
- Permission/authorization model for who can archive whose runs — assumed to match existing mutation authorization; no new authz surface introduced here. Planning should confirm this during the mutation-endpoint audit (see Deferred Questions).
- Archiving active (non-terminal) runs — not supported; users cancel/complete first.
- Forward-compat of stored `prior_status` values across a future `RunStatus` enum evolution — out of scope; if the enum later changes shape, archived runs persisted at that time will be handled by whatever migration that change introduces.
- Idempotency of repeated archive/unarchive invocations — planning decision (no-op vs. error). This *is* user-visible via the bulk per-id result shape in R6/R7, so planning must pick a concrete behavior before shipping; the decision is only deferred in the sense that brainstorm didn't choose between no-op and error.

## Alternatives Considered

- **Auto-hide terminal runs by default (no archive action)** — rejected. The whole premise is to *capture user intent*. Auto-hiding silently removes runs the user has not acknowledged, producing a false "it's handled" signal and leaving users guessing which of a large terminal set they've actually reviewed. The explicit-action model trades a small amount of ceremony for a clear, user-driven triage state. It is also compatible with a future auto-archive policy layered on top (out of scope for v1), so nothing is lost.
- **Boolean `archived` flag orthogonal to status** — rejected for v1. Keeps status semantics stable and avoids the prior-status-restoration problem entirely, but forces every consumer (CLI, API, UI, reports) to learn a second axis. The single-status-axis model is simpler at the point of use given the v1 shape; see Key Decisions for the accepted tradeoff.
- **Predicate-based bulk archive (`--status succeeded --older-than 14d`)** — deferred. Would make mass-triage frictionless for users with many accumulated runs, but ID-based bulk (R6) is enough to validate the core premise in v1 and avoids defining a query language up front. Can be added once adoption is observed.

## Key Decisions

- **New first-class terminal enum value, not a boolean flag** — shown as the surface status in `ps`, API, and UI. Keeping status as the single axis avoids teaching every consumer about a parallel `archived` dimension. Accepted tradeoff: a future orthogonal lifecycle axis (e.g. `superseded`, soft-delete) would need its own modeling rather than composing with `archived`. For the stated v1 shape (reversible user-intent triage on a reached-terminal run), the single-axis model is simpler at the point of use even though it costs more if lifecycle gains a second axis later. This choice is also load-bearing and expensive to reverse: once `Archived` is serialized into the event log, checkpoint metadata, the OpenAPI spec, and the TypeScript client, converting to an orthogonal boolean later would require a data migration across every one of those surfaces.
- **Reversible via unarchive, restoring prior terminal status** — archival is triage intent, not a destructive operation. Accidental archives must be trivially undoable without data loss.
- **Read-only while archived; mutation requires explicit unarchive** — prevents the "I said I was done with this" run from silently mutating. Forces a conscious lift.
- **Hidden from default listings with opt-in flag** — the whole point of the feature is reducing noise in default views. Direct-by-ID access stays unrestricted.
- **No storage impact** — keeps blast radius small. Storage/retention is a separate future feature.
- **All three source terminal statuses archive identically** — simplest state machine; avoids carving out `dead` or `failed` as "un-archivable."

## Dependencies / Assumptions

- `RunStatus` and its transition guard in `lib/crates/fabro-types/src/status.rs` will be extended with `Archived`, and with transitions from each of `Succeeded`/`Failed`/`Dead` into `Archived` and back. *Verified against current code.* Note: `can_transition_to` today uses `is_terminal()` to hard-block any exit from a terminal state (except to `Dead`). Introducing archived therefore requires either narrowing `is_terminal()` into two concepts — "reached a terminal outcome" vs. "immutable / not transitionable" — or special-casing archive/unarchive transitions in the guard. Which direction to take is a planning decision; whichever is chosen, planning must audit every existing caller of `is_terminal()` (UI state, completion detection, billing roll-ups, reporting) and decide per caller whether `Archived` should be treated as "reached terminal" or "immutable / not transitionable."
- `RunStatusRecord` carries a single status + optional reason + `updated_at`. `RunProjection` in `lib/crates/fabro-store/src/run_state.rs` holds only the most recent `RunStatusRecord` — each status-change event overwrites `self.status` and there is no queryable status-history projection. *Verified against current code.* The event log itself retains the full sequence of status-change events via a per-run monotonic `event_seq` (`AtomicU32` in `lib/crates/fabro-store/src/slate/run_store.rs`), so events for a given run are replayed in append order; prior terminal status is therefore recoverable via replay. Given the existing projection shape, the cleanest pattern for R4 is to carry the restored status on the unarchive event payload itself so replay reconstructs state without scanning backward. Final mechanism choice is still a planning decision.
- The OpenAPI spec `docs/api-reference/fabro-api.yaml` is the source of truth for the HTTP interface; archive/unarchive endpoints and the new status value are added there first, and both the Rust progenitor client and the TS Axios client regenerate per the existing workflow. The existing `listRuns` operation must also gain a query parameter (e.g. `include_archived`) — today it only accepts pagination params — so API consumers have an explicit way to opt in; this is an edit to an existing endpoint, not purely additive.
- Run events are extensible; existing variants like `RunRemoving` in `lib/crates/fabro-workflow/src/event.rs` serve as shape precedents for `RunArchived`/`RunUnarchived`.
- Run IDs are assumed to be globally unique and resolvable without additional workspace/project scoping. R6/R7's bare-positional-ID CLI shape depends on this. If namespacing is later introduced, scoping will need to be added to the archive CLI as a compat-aware change.
- Existing mutation operations in `lib/crates/fabro-workflow/src/operations/` are the set that must be taught to reject when the run is archived.

## Outstanding Questions

### Resolve Before Planning

- (none)

### Deferred to Planning

- [Affects R4][Technical] Record prior terminal status on the unarchive event payload (preferred pattern given the existing projection shape — see Dependencies), vs. add a `prior_status` field to the run record, vs. walk the event log on unarchive. Pick the mechanism and update the projection accordingly.
- [Affects R10][User decision] Extend `-a/--all` to also include archived, vs. add a separate `--archived` flag (and whether `-a --archived` composes). This also drives the 'No running processes found' hint text. Compat note: today `-a` means "show all runs including terminal"; extending it silently narrows that meaning, while a separate `--archived` flag preserves it.
- [Affects R8][Technical] API shape — dedicated endpoints (`POST /runs/{id}/archive`, `POST /runs/{id}/unarchive`, plus a bulk form) vs. extending the existing `RunControlAction` enum with `Archive`/`Unarchive`.
- [Affects R9][Technical] Exact event variant names and payload shape (`RunArchived { actor }`, `RunUnarchived { actor, restored_status }`?), including whether they piggyback on the existing status-transition event family.
- [Affects R14][Technical] The full list of endpoints and operations that must reject on archived runs; planning should audit `operations/*.rs` and the server's mutation routes, including label/tag write paths and any event-producing endpoint that bypasses `can_transition_to`.
- [Affects R11][Technical] Whether other internal list surfaces (dashboards, summaries, PR cross-references, usage roll-ups) need the same default-hide treatment, or whether they're acceptable as-is.

## Next Steps

→ `/ce:plan` for structured implementation planning
