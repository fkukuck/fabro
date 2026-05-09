# Run Files Diff Scope Picker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a run-files diff scope picker for committed, uncommitted, and all sandbox changes.

**Architecture:** Keep the existing files endpoint default behavior as committed changes. Add a `scope` query parameter for live working-tree scopes, and use patch-backed response construction for scopes that include uncommitted state. The frontend stores the selected scope in the URL and gives each scope an independent SWR cache key.

**Tech Stack:** Rust, Axum, OpenAPI/progenitor, Bun, React, SWR, generated TypeScript Axios client.

---

## Summary

Add a picker to the run files page with three modes:

- `Committed changes`: current behavior, comparing `base_sha..HEAD`.
- `Uncommitted changes`: staged, unstaged, and safe untracked changes in the live sandbox.
- `All changes`: committed plus uncommitted sandbox changes from `base_sha` to the current working tree.

`Committed changes` remains the default. `Uncommitted changes` and `All changes` require a reachable live sandbox; stored `final_patch` fallback is only valid for committed changes.

## API And Backend Changes

- [ ] Add `scope` to `GET /api/v1/runs/{id}/files` in `docs/public/api-reference/fabro-api.yaml`.
  - Accepted values: `committed`, `uncommitted`, `all`.
  - Default: `committed`.
  - Keep `from_sha` and `to_sha` reserved and rejected when present.
- [ ] Run `cargo build -p fabro-api` so progenitor regenerates Rust API types.
- [ ] Add a server-side scope enum in `lib/crates/fabro-server/src/run_files.rs`.
  - `Committed` maps to the existing `base_sha..HEAD` materialization path.
  - `Uncommitted` maps to a new live-only working-tree materialization path.
  - `All` maps to the same live-only path with `base_sha` as the comparison base.
- [ ] Keep the existing committed path unchanged except for dispatching through the new scope enum.
  - It may still reconnect/start the sandbox.
  - It may still fall back to `RunProjection.final_patch` when live sandbox diffing cannot use the stored base.
- [ ] Implement a patch-backed working-tree path for `uncommitted` and `all`.
  - For `uncommitted`, use sandbox git diff output equivalent to `git diff HEAD`.
  - For `all`, use sandbox git diff output equivalent to `git diff <base_sha>`.
  - Include staged and unstaged changes.
  - Include safe untracked files by discovering untracked paths, validating repo-relative paths, applying sensitive filtering, enforcing size caps, and rendering them as added-file patch sections or placeholder entries.
- [ ] Reuse existing response semantics where possible.
  - Return `PaginatedRunFileList`.
  - Keep `FileDiff[]` shape.
  - For patch-backed live working-tree scopes, use `unified_patch` with null file contents, matching the degraded renderer branch.
  - Reuse existing sensitive, binary, symlink, submodule, truncation, file-count, and aggregate-byte behavior from the patch-section response code.
- [ ] Return `409 Conflict` for `scope=uncommitted` or `scope=all` when sandbox reconnect/start fails.
  - Do not silently return committed fallback data under a live-only scope.
  - Error detail: `Live sandbox access is required for this diff scope.`

## Frontend Changes

- [ ] Regenerate the TypeScript client.
  - Run: `cd lib/packages/fabro-api-client && bun run generate`.
- [ ] Update `apps/fabro-web/app/lib/queries.ts`.
  - Change `useRunFiles(id)` to `useRunFiles(id, scope)`.
  - Include `scope` in the SWR key so each mode caches independently.
  - Pass `scope` to `runOutputsApi.listRunFiles`.
- [ ] Update `apps/fabro-web/app/routes/run-files.tsx`.
  - Parse `scope` from the page URL.
  - Missing or invalid scope resolves to `committed`.
  - Persist picker changes with `?scope=committed`, `?scope=uncommitted`, or `?scope=all`.
  - Keep hash file deep links working with the existing `#file=...` format.
- [ ] Update `apps/fabro-web/app/routes/run-files/toolbar.tsx`.
  - Add a compact segmented picker with labels:
    - `All changes`
    - `Uncommitted`
    - `Committed`
  - Keep the existing diff layout toggle and refresh button.
  - Disable no mode by default; let the server decide whether live-only scopes are available.
- [ ] Show live-sandbox errors inline.
  - If `scope` is `uncommitted` or `all` and the API returns `409`, show: `Live sandbox access is required for this diff.`
  - Keep the selected scope visible.
  - Do not route the error through the full-page initial error state when previous data exists.

## Test Plan

- [ ] Add server/API tests for committed default behavior.
  - Request without `scope` still returns the same committed response as today.
  - Request with `scope=committed` uses the same path.
  - `from_sha` and `to_sha` are still rejected.
- [ ] Add server/API tests for fallback behavior.
  - `scope=committed` falls back to `final_patch` when sandbox access is unavailable.
  - `scope=uncommitted` returns `409` when sandbox access is unavailable.
  - `scope=all` returns `409` when sandbox access is unavailable.
- [ ] Add server/API tests for working-tree scopes.
  - `scope=uncommitted` includes staged changes.
  - `scope=uncommitted` includes unstaged changes.
  - `scope=uncommitted` includes safe untracked files.
  - `scope=all` includes committed plus live working-tree changes.
  - Sensitive untracked paths render as sensitive placeholders.
  - Large untracked files render as truncated placeholders.
- [ ] Add frontend tests.
  - Default picker state is `Committed`.
  - Picker changes update the URL query string.
  - `useRunFiles` calls the API with the selected scope.
  - Each scope has a distinct cache key.
  - `409` for live-only scopes renders the inline live-sandbox message.
- [ ] Run verification commands.
  - `cargo build -p fabro-api`
  - `cargo nextest run -p fabro-server run_files`
  - `cd lib/packages/fabro-api-client && bun run generate`
  - `cd apps/fabro-web && bun test`
  - `cd apps/fabro-web && bun run typecheck`

## Assumptions

- Existing behavior should remain the default, so missing `scope` means `committed`.
- `Uncommitted changes` means staged, unstaged, and safe untracked files inside the run sandbox.
- Remote clone-based sandboxes cannot include submitter-side local dirty changes unless those changes were already present in the sandbox.
- Stored `final_patch` is only used for committed/final run changes.
