# Mechanical Event Module Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split `fabro-workflow`'s oversized event module into focused child modules while preserving `fabro_workflow::event::{...}` as the public API.

**Architecture:** Keep `src/event.rs` as a facade that declares child modules and re-exports the existing public symbols. Move code mechanically by responsibility, co-locate tests with the modules they cover, and move `StageScope` to a crate-root module while preserving `fabro_workflow::event::StageScope`. `event/events.rs` intentionally remains the largest file because it keeps the `Event` enum and its exhaustive tracing behavior together.

**Tech Stack:** Rust, Tokio, serde/serde_json, chrono, uuid, fabro-types `RunEvent` / `EventBody`, fabro-store `EventPayload`, existing `cargo nextest` workflow tests.

---

## Files

- Modify: `lib/crates/fabro-workflow/src/event.rs`
- Create: `lib/crates/fabro-workflow/src/event/events.rs`
- Create: `lib/crates/fabro-workflow/src/event/names.rs`
- Create: `lib/crates/fabro-workflow/src/event/stored_fields.rs`
- Create: `lib/crates/fabro-workflow/src/event/convert.rs`
- Create: `lib/crates/fabro-workflow/src/event/redaction.rs`
- Create: `lib/crates/fabro-workflow/src/event/sink.rs`
- Create: `lib/crates/fabro-workflow/src/event/emitter.rs`
- Create: `lib/crates/fabro-workflow/src/stage_scope.rs`
- Modify: `lib/crates/fabro-workflow/src/lib.rs`
- Modify: `docs/internal/events-strategy.md`

## Task 1: Confirm Private Helpers and Build the Facade

- [x] Confirm these helpers are not used outside `event.rs` before moving them:

```bash
rg -n "StoredEventFields|event_body_from_event|normalized_event_value|redacted_event_value|RunEventCommand|RunEventSinkFuture|RunEventSinkCallback|RunEventTransform|agent_tool_call_id|agent_actor_for_event|default_node_label|node_stored_fields|billed_token_counts_from_llm|stage_status_from_string|epoch_millis" . --glob '!target' --glob '!apps/fabro-web/dist/**'
```

Expected: production hits are limited to `lib/crates/fabro-workflow/src/event.rs`; plan and historical docs may mention the names.

- [x] Replace `event.rs` with child module declarations and public re-exports:

```rust
mod convert;
mod emitter;
mod events;
mod names;
mod redaction;
mod sink;
mod stored_fields;

pub use fabro_types::{EventBody, RunNoticeLevel};

pub use self::convert::{to_run_event, to_run_event_at};
pub use self::emitter::Emitter;
pub use self::events::Event;
pub use self::names::event_name;
pub use self::redaction::{
    build_redacted_event_payload, event_payload_from_redacted_json, redacted_event_json,
};
pub use self::sink::{
    RunEventLogger, RunEventSink, StoreProgressLogger, append_event, append_event_to_sink,
};
pub use crate::stage_scope::StageScope;
```

- [x] Add `mod stage_scope;` to `lib.rs`. Do not expose a new `fabro_workflow::stage_scope` public module in this pass; preserve the existing public path through `pub use crate::stage_scope::StageScope` in `event.rs`.

## Task 2: Move Event, Names, Stage Scope, and Stored Fields

- [x] Move the `Event` enum, `Event::pull_request_created`, and `Event::trace` into `event/events.rs`. Keep all derives, serde attributes, clippy allowances, variant fields, tracing levels, tracing fields, and `PullRequestRecord` behavior unchanged.

- [x] Move `event_name` into `event/names.rs`. Keep the exhaustive match and all returned strings unchanged.

- [x] Move `StageScope` into `stage_scope.rs`. Keep constructors and `stage_id()` unchanged, including use of `visit_from_context`. Preserve `fabro_workflow::event::StageScope` by re-exporting it from `event.rs`.

- [x] In `stage_scope.rs`, import only the dependencies needed by `StageScope`: `fabro_types::{ParallelBranchId, StageId}`, `crate::context::{Context as WfContext, WorkflowContext}`, and `crate::run_dir::visit_from_context`. Do not import from `crate::event`.

- [x] Move stored-field helpers into `event/stored_fields.rs`:
  - `StoredEventFields`
  - `default_node_label`
  - `node_stored_fields`
  - `stored_event_fields`
  - `stored_event_fields_for_variant`
  - `agent_tool_call_id`
  - `agent_actor_for_event`

- [x] Make both `StoredEventFields` and `stored_event_fields` `pub(super)` because `convert.rs` calls the function and reads fields from its return value. Keep `default_node_label`, `node_stored_fields`, `stored_event_fields_for_variant`, `agent_tool_call_id`, and `agent_actor_for_event` private to `stored_fields.rs`.

## Task 3: Move Conversion and Redaction

- [x] Move conversion helpers into `event/convert.rs`:
  - `billed_token_counts_from_llm`
  - `stage_status_from_string`
  - `event_body_from_event`
  - `to_run_event`
  - `to_run_event_at`

- [x] Keep `event_body_from_event` private to `convert.rs`. Import `stored_event_fields` from `event/stored_fields.rs`. Keep `to_run_event` and `to_run_event_at` public through the facade re-export.

- [x] Move redaction helpers into `event/redaction.rs`:
  - `build_redacted_event_payload`
  - `redacted_event_json`
  - `normalized_event_value`
  - `redacted_event_value`
  - `event_payload_from_redacted_json`

- [x] Keep `normalized_event_value` and `redacted_event_value` private. Keep redaction behavior exactly as `RunEvent::to_value() -> normalize_json_value -> redact_json_value`.

## Task 4: Move Sink, Logger, and Emitter Plumbing

- [x] Move sink and logger code into `event/sink.rs`:
  - `append_event`
  - `append_event_to_sink`
  - `RunEventSink`
  - `RunEventSinkFuture`
  - `RunEventSinkCallback`
  - `RunEventTransform`
  - `RunEventCommand`
  - `RunEventLogger`
  - `StoreProgressLogger`

- [x] Keep `RunEventCommand` and callback type aliases private. Preserve the iterative `RunEventSink::write_run_event` stack logic and redacted JSONL output behavior.

- [x] Move emitter code into `event/emitter.rs`:
  - `epoch_millis`
  - `EventListener`
  - `Emitter`
  - `Debug`, `Default`, and inherent impls

- [x] Keep `dispatch_run_event` as `pub(crate)` and keep all public `Emitter` methods unchanged.

## Task 5: Co-locate Tests and Update Docs

- [x] Move each existing inline test into the module it characterizes. Keep test names, fixtures, assertions, and async test attributes unchanged.

- [x] Use this test placement:
  - `emitter.rs`: `event_emitter_*`
  - `sink.rs`: `run_event_sink_*`, `run_event_logger_*`, `append_event_writes_store_event_shape`
  - `redaction.rs`: `build_redacted_event_payload_*`
  - `names.rs`: `event_name_matches_new_dot_notation`, `run_archived_event_name_matches_dot_notation`
  - `stored_fields.rs`: only direct helper tests introduced during the move, if needed; it is acceptable for this module to have no tests
  - `convert.rs`: all `run_event_*` tests, actor envelope tests, tool call id tests, parallel id tests, stage id tests, metadata snapshot mapping tests, and `stage_scope_populates_stage_id_on_non_stage_events`
  - `stage_scope.rs`: direct `StageScope` constructor tests introduced during the move, if needed; it is acceptable for this module to have no tests

- [x] Update `docs/internal/events-strategy.md` references that say the canonical conversion is in `fabro-workflow/src/event.rs` so they refer to the `fabro-workflow::event` module. Do not change event-strategy rules.

- [x] Do not update production call sites outside the event module unless compilation requires an import fix. The intended result is that existing imports such as `crate::event::{Emitter, Event}` and `fabro_workflow::event::{Event, to_run_event}` continue to work.

## Task 6: Verify the Mechanical Split

- [x] Run focused event tests:

```bash
cargo nextest run -p fabro-workflow event
```

Expected: all matching tests pass.

- [x] Run the full workflow crate test suite:

```bash
cargo nextest run -p fabro-workflow
```

Expected: all `fabro-workflow` tests pass.

- [x] Run format check:

```bash
cargo +nightly-2026-04-14 fmt --check --all
```

Expected: no formatting diffs.

- [x] Run clippy:

```bash
cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings
```

Expected: no warnings.

## Acceptance Criteria

- `lib/crates/fabro-workflow/src/event.rs` is a small facade module.
- Existing public API paths under `fabro_workflow::event::{...}` still compile.
- `StageScope` lives at crate root and remains available from `fabro_workflow::event::StageScope`.
- Tests are co-located with the module they cover; there is no catch-all `event/tests.rs`.
- Event wire names, envelope metadata, `EventBody` conversion, redaction, JSONL sink output, store payload shape, and emitter dispatch behavior are unchanged.
- No macro registry, generated event table, or domain-split `Event` enum is introduced in this pass.
- `docs/internal/events-strategy.md` remains accurate after the file split.

## Assumptions

- This is a strictly mechanical refactor; deeper cleanup like domain-specific event enums or shared event DTO extraction is out of scope.
- `event/events.rs` remains intentionally large because keeping `Event::trace` with `Event` avoids splitting a pure inherent `Event` behavior into a separate file.
- Keeping `convert.rs` focused on `Event -> EventBody` body conversion is acceptable even if it remains one of the larger event files.
- `RunEventSink`, `RunEventLogger`, and `StoreProgressLogger` remain together in `sink.rs` for this pass; split them later only if that file remains hard to navigate after tests are co-located.
- Existing tests are sufficient characterization coverage for this split; new behavior tests are not required unless a moved module exposes an accidental visibility issue.
