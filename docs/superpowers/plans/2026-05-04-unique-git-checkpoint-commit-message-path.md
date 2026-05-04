# Unique Git Checkpoint Commit Message Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the flaky checkpoint metadata lifecycle test by making git checkpoint commit message files unique per invocation.

**Architecture:** The checkpoint path will keep writing commit messages outside the repository so `git add -A -- .` cannot stage them, but the basename will include a UUID instead of only `run_id` and `node_id`. Cleanup will be best-effort and must not affect checkpoint success or failure behavior.

**Tech Stack:** Rust, Tokio async tests, `uuid`, `cargo nextest`, git CLI through the sandbox abstraction.

---

## Summary

Fix the CI flake by removing the shared `/tmp/fabro-commit-msg-{run_id}-{node_id}` path used by git checkpoint commits. Each checkpoint invocation will write its commit message to a UUID-based temp path, so concurrently running tests or workflows with the same run id and node id cannot collide.

## Key Changes

- In `lib/crates/fabro-workflow/src/sandbox_git.rs`, change the commit message path to a UUID-based basename:

  ```rust
  let msg_path = format!("/tmp/fabro-commit-msg-{}", uuid::Uuid::new_v4());
  ```

- Pass the path to `git commit -F` through `shell_quote(&msg_path)`:

  ```rust
  let msg_path_q = shell_quote(&msg_path);
  ```

- After the `git commit` command returns, call:

  ```rust
  let _ = sandbox.delete_file(&msg_path).await;
  ```

  Do this before matching the commit result, so successful and failed commit attempts do not leave growing UUID files in `/tmp`.

- No public APIs, event schemas, types, or CLI behavior change.

## Test Plan

- Extend the existing `ScriptedSandbox` test helper in `sandbox_git.rs` to record `write_file` paths, `delete_file` paths, and executed commands.
- Add a unit test that calls `git_checkpoint()` twice with the same `run_id` and `node_id`, then asserts:
  - two commit message paths were written
  - both paths start with `/tmp/fabro-commit-msg-`
  - the paths differ
  - both paths were deleted
  - commit commands use `-F /tmp/fabro-commit-msg-...`

Run focused checks:

```bash
cargo nextest run -p fabro-workflow git_checkpoint_uses_unique_commit_message_paths_for_same_run_and_node
cargo nextest run -p fabro-workflow checkpoint_metadata --profile ci --stress-count 100 -j 32
cargo nextest run -p fabro-workflow lifecycle::git::tests::checkpoint_metadata_load_state_failure_emits_scoped_failed_before_notice
cargo +nightly-2026-04-14 fmt --check --all
```

## Assumptions

- `uuid` is already available to `fabro-workflow`, so no dependency change is needed.
- Keeping the temp file under `/tmp` is intentional: it stays outside the repo, so it cannot be staged by `git add -A -- .`.
- Cleanup is best-effort and must never change checkpoint success or failure semantics.
