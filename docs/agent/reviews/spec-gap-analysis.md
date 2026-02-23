# Attractor Spec Gap Analysis

Comparison of the implementation in `crates/attractor/` against `docs/specs/attractor-spec.md`.

## Summary

The core pipeline engine, DOT parsing, edge selection, condition evaluation, retry logic, checkpoint/resume, validation, and all 10 handler types are implemented. The HTTP server with SSE is implemented. Context fidelity preamble synthesis, thread ID plumbing to backends, engine cancellation, recording/replay, and preset retry policies are all implemented. The remaining gaps are **SVG graph rendering** and **no DOT-level mechanism for custom retry predicates**.

---

## Implemented Features (Complete or Substantially Complete)

| Spec Section | Feature | Status |
|---|---|---|
| 2. DOT DSL | Parser, grammar, value types, chained edges, subgraphs, defaults, class attr | Done |
| 3.1 | Run lifecycle (parse, validate, initialize, execute, finalize) | Done |
| 3.2 | Core execution loop | Done |
| 3.3 | Edge selection (5-step: condition, preferred label, suggested IDs, weight, lexical) | Done |
| 3.4 | Goal gate enforcement with retry target fallback chain | Done |
| 3.5-3.6 | Retry logic with backoff, jitter, preset policies, allow_partial | Done |
| 3.6 | Preset retry policies selectable by name from DOT (`retry_policy` attr) | Done |
| 3.7 | Failure routing (fail edge, retry_target, fallback, termination) | Done |
| 3.8 | Single-threaded traversal with parallel handler isolation | Done |
| 4.1-4.2 | Handler interface and registry (explicit type > shape > default) | Done |
| 4.3-4.4 | Start/Exit handlers | Done |
| 4.5 | Codergen handler with CodergenBackend, simulation mode, $goal expansion, log files | Done |
| 4.5 | CodergenBackend receives thread_id for session reuse | Done |
| 4.6 | Wait.human handler with accelerator keys, freeform edges, timeout, choice matching | Done |
| 4.7 | Conditional handler (no-op, routing via edge selection) | Done |
| 4.8 | Parallel handler (fan-out, join policies, error policies, bounded concurrency) | Done |
| 4.9 | Fan-in handler (heuristic + LLM-based evaluation) | Done |
| 4.10 | Tool handler (shell command, timeout) | Done |
| 4.11 | Manager loop handler (observe/steer/wait, child autostart, stop condition) | Done |
| 4.12 | Custom handler registration | Done |
| 5.1 | Context (key-value store, thread-safe, snapshot, clone, apply_updates) | Done |
| 5.2 | Outcome (all StageStatus values, context_updates, preferred_label, suggested_next_ids) | Done |
| 5.3 | Checkpoint (save/load, resume from checkpoint, node_outcomes, retry counters) | Done |
| 5.3 | Checkpoint resume fidelity degradation (full -> summary:high on first resumed node) | Done |
| 5.4 | Fidelity resolution (edge > node > graph > default) | Done |
| 5.4 | Thread ID resolution (5-level precedence) | Done |
| 5.4 | Context fidelity preamble synthesis (truncate, compact, summary:low/medium/high) | Done |
| 5.5 | Artifact store | Done |
| 5.6 | Run directory structure (manifest.json, status.json, prompt.md, response.md) | Done |
| 6.1 | Interviewer.inform() called at pipeline/stage lifecycle points | Done |
| 6.1-6.5 | Interviewer interface + all implementations (AutoApprove, Console, Callback, Queue, Recording, Web) | Done |
| 6.4 | RecordingInterviewer serialization (to_json/from_json, save/load file) | Done |
| 6.4 | ReplayInterviewer for replaying recorded Q&A sessions | Done |
| 7.1-7.4 | Validation with 15 lint rules, diagnostic model, custom rules | Done |
| 8.1-8.6 | Model stylesheet (parse, selectors: *, .class, #id, specificity, application) | Done |
| 9.1-9.3 | Transforms (variable expansion, stylesheet application, custom transforms) | Done |
| 9.4 | Graph merging transform (namespace-prefixed node/edge merge) | Done |
| 9.5 | HTTP server mode (POST /pipelines, GET status, SSE events, question answering, cancel) | Done |
| 9.5 | GET /pipelines/{id}/checkpoint and GET /pipelines/{id}/context endpoints | Done |
| 9.5 | Pipeline cancel with engine-level cancellation token (checked between nodes) | Done |
| 9.6 | Event emitter (pipeline/stage/parallel/interview/checkpoint events) | Done |
| 9.7 | Tool call hooks (pre/post hooks on codergen handler) | Done |
| 10.1-10.6 | Condition expression language (=, !=, &&, outcome, preferred_label, context.*) | Done |
| N/A | Sub-pipeline handler (inline DOT, context isolation with diff propagation) | Done (bonus) |
| 2.7 | Edge `loop_restart` attribute | Done |
| 2.6 | Node `auto_status` attribute | Done |
| 2.6 | Node `timeout` attribute enforcement | Done |

---

## Gaps

### 1. GET /pipelines/{id}/graph (SVG Rendering) (Spec 9.5)

**Status: Not implemented**

The spec lists `GET /pipelines/{id}/graph` to return a rendered graph visualization (SVG). The HTTP server does not implement this endpoint. No graphviz dependency exists, and the original DOT source is not stored in `ManagedPipeline` after parsing.

### 2. `should_retry` Predicate Customization (Spec 3.6)

**Status: Default predicate only**

The `RetryPolicy` struct has a `should_retry: ShouldRetryFn` field and the Rust API supports custom predicates. However, all preset policies (`none`, `standard`, `aggressive`, `linear`, `patient`) and `build_retry_policy()` use the same `default_should_retry()` predicate. There's no DOT-level mechanism for per-node retry predicate customization. The spec defines `should_retry` in the `RetryPolicy` struct but only describes a default predicate — it's unclear whether per-node customization is required.

Note: the spec's default predicate description references HTTP status codes (429, 5xx, 401, 403, 400) but the implementation classifies retryability by `AttractorError` variant (`Handler`/`Engine`/`Io` = retryable). Reasonable for Rust but not a 1:1 mapping.

---

## Spec Contradictions (not implementation gaps)

These items have conflicting definitions within the spec itself. The spec needs to be reconciled; no implementation changes are needed.

### Stylesheet Shape Selectors (Spec 8 vs 11.12)

The grammar (section 8.2) defines `Selector ::= '*' | '#' Identifier | '.' ClassName` — no shape selectors. But the DoD checklist (section 11.12) says "Selectors by shape name work (e.g., `box { ... }`)" and lists a 4-level specificity order including shape. The implementation follows the grammar.

### Orphan Node Severity (Spec 7 vs 11.12)

The validation table (section 7) defines `reachability` as **ERROR**. The DoD checklist (section 11.12) says "Validate: orphan node -> **warning**". The implementation uses ERROR, matching section 7.

---

## Cross-Feature Parity Matrix Status

Based on code review, these items from Spec 11.12 appear covered:

- [x] Parse simple linear pipeline
- [x] Parse pipeline with graph-level attributes
- [x] Parse multi-line node attributes
- [x] Validate: missing start/exit node -> error
- [x] Execute linear 3-node pipeline end-to-end
- [x] Execute with conditional branching
- [x] Execute with retry on failure
- [x] Goal gate blocks exit when unsatisfied
- [x] Goal gate allows exit when all satisfied
- [x] Wait.human presents choices and routes on selection
- [x] Wait.human with freeform edge routes free-text input
- [x] Edge selection: condition match wins over weight
- [x] Edge selection: weight breaks ties
- [x] Edge selection: lexical tiebreak
- [x] Context updates visible to next node
- [x] Checkpoint save and resume
- [x] Stylesheet applies model override by class/ID
- [x] Prompt variable expansion ($goal)
- [x] Parallel fan-out and fan-in
- [x] Custom handler registration and execution
- [x] Pipeline with 10+ nodes (via integration tests)
- [x] Validate: orphan node -> error (spec contradiction: section 7 says ERROR, DoD says warning; implementation matches section 7)
