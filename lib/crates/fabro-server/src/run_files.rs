#![allow(clippy::result_large_err, unreachable_pub)]

//! GET /api/v1/runs/{id}/files — per-request coalescing and handler.
//!
//! This module exposes the per-run request-coalescing primitive consumed by
//! the Files Changed endpoint. Concurrent HTTP callers for the same run share
//! one materialization; concurrent callers for different runs proceed in
//! parallel. See Unit 4 of the Run Files Changed plan for design rationale.
//!
//! The materialization is deliberately driven by [`tokio::spawn`] rather than
//! polling a `Shared` future: the spawned task makes progress regardless of
//! whether any caller is still waiting, so an abandoned request cannot leave
//! orphan git subprocesses in the sandbox. All panics are caught and surfaced
//! as a 500 `ApiError`, and the registry entry is removed on task completion
//! so a follow-up request triggers a fresh materialization.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Instant;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use fabro_agent::Sandbox;
use fabro_api::types::{
    DiffFile, FileDiff, FileDiffChangeKind, FileDiffTruncationReason, PaginatedRunFileList,
    RunFilesMeta, RunFilesMetaDegradedReason, RunFilesMetaToSha,
};
use fabro_sandbox::reconnect::reconnect;
use fabro_types::RunId;
use fabro_workflow::sandbox_git::{
    BlobMeta, DiffError, RawDiffEntry, SubmoduleChange, SymlinkChange, list_binary_paths,
    list_changed_files_raw, stream_blob_metadata, stream_blobs,
};
use futures_util::FutureExt;
use serde::Deserialize;
use tokio::sync::{Mutex, watch};
use tracing::info;

use crate::error::ApiError;
use crate::jwt_auth::AuthenticatedService;
use crate::server::{AppState, parse_run_id_path_pub};

/// Per-file cap: 256 KiB OR 20k lines (whichever comes first).
pub(crate) const PER_FILE_BYTES_CAP: u64 = 256 * 1024;
pub(crate) const PER_FILE_LINES_CAP: usize = 20_000;
/// Aggregate response cap: 5 MiB of textual content across all files.
pub(crate) const AGGREGATE_BYTES_CAP: u64 = 5 * 1024 * 1024;
/// Per-response file-count cap.
pub(crate) const FILE_COUNT_CAP: usize = 200;
/// Sandbox git timeout. Matches Unit 3 helpers (10 s).
const SANDBOX_GIT_TIMEOUT_MS: u64 = 10_000;

/// Query parameters accepted by `GET /runs/{id}/files`.
#[derive(Debug, Deserialize, Default)]
pub struct ListRunFilesParams {
    #[serde(rename = "page[limit]")]
    #[allow(dead_code)]
    page_limit:   Option<u32>,
    #[serde(rename = "page[offset]")]
    #[allow(dead_code)]
    page_offset:  Option<u32>,
    #[serde(default)]
    pub from_sha: Option<String>,
    #[serde(default)]
    pub to_sha:   Option<String>,
}

/// Shared outcome of a single materialization. Wrapped in [`Arc`] so every
/// coalesced caller walks away with a cheap clone rather than an owned copy.
pub type ListRunFilesResult = std::result::Result<PaginatedRunFileList, ApiError>;

type Shared = Arc<ListRunFilesResult>;

/// Registry type held on `AppState`. Maps each `RunId` to the watch channel
/// that downstream callers subscribe to while a materialization is in flight.
pub type FilesInFlight = Arc<Mutex<HashMap<RunId, watch::Receiver<Option<Shared>>>>>;

/// Construct a fresh, empty `FilesInFlight` registry.
pub fn new_files_in_flight() -> FilesInFlight {
    Arc::new(Mutex::new(HashMap::new()))
}

/// Run `materialize` at most once per `run_id`, sharing the result with any
/// concurrent callers that arrive while it is still in flight.
///
/// The spawned task owns the materialization. Dropping the returned future
/// only unsubscribes *this* caller — the task still runs to completion and
/// cleans itself up from the registry. Panics inside `materialize` are
/// caught and returned as an internal-server-error `ApiError` to every
/// concurrent caller; a subsequent call on the same `run_id` after the
/// panic triggers a fresh materialization (no poisoning).
pub async fn coalesced_list_run_files<F, Fut>(
    inflight: &FilesInFlight,
    run_id: &RunId,
    materialize: F,
) -> Shared
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = ListRunFilesResult> + Send + 'static,
{
    let mut rx = {
        let mut guard = inflight.lock().await;
        if let Some(existing) = guard.get(run_id) {
            existing.clone()
        } else {
            let (tx, rx) = watch::channel::<Option<Shared>>(None);
            guard.insert(*run_id, rx.clone());

            let inflight = Arc::clone(inflight);
            let run_id_cloned = *run_id;
            tokio::spawn(async move {
                let result = AssertUnwindSafe(async move { materialize().await })
                    .catch_unwind()
                    .await;
                let shared: Shared = match result {
                    Ok(value) => Arc::new(value),
                    Err(_) => Arc::new(Err(ApiError::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Run files materialization panicked.",
                    ))),
                };
                // Send before unregistering so a new receiver subscribed via
                // `rx.clone()` still sees the cached value via `borrow()`.
                let _ = tx.send(Some(shared));
                inflight.lock().await.remove(&run_id_cloned);
            });
            rx
        }
    };

    loop {
        let snapshot: Option<Shared> = rx.borrow_and_update().clone();
        if let Some(value) = snapshot {
            return value;
        }
        if rx.changed().await.is_err() {
            // Sender dropped without sending. Shouldn't happen in practice
            // because the spawned task always sends before dropping, but be
            // defensive.
            return Arc::new(Err(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Run files materialization channel closed.",
            )));
        }
    }
}

// ── HTTP handler ──────────────────────────────────────────────────────────

/// `GET /api/v1/runs/{id}/files` — real handler.
///
/// Flow:
/// 1. Parse & authenticate
/// 2. Reject non-default `from_sha`/`to_sha` per R15 (v1 only serves the full
///    run diff)
/// 3. Load run projection (404 on missing/unauthorized — IDOR-safe)
/// 4. Try to reconnect the sandbox; on success, run the sandbox git helpers and
///    build a structured response
/// 5. Fall through to empty envelope when no sandbox path is available. Unit 6
///    replaces this branch with the `final_patch` degraded fallback.
///
/// All logging emits a single `tracing::info!` with an allowlisted field
/// set: `run_id, file_count, bytes_total, duration_ms, truncated,
/// binary_count, sensitive_count, symlink_count, submodule_count`. No
/// paths, contents, or raw git stderr are logged.
pub async fn list_run_files(
    _auth: AuthenticatedService,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<ListRunFilesParams>,
) -> Response {
    // 1. Parse run_id.
    let id = match parse_run_id_path_pub(&id) {
        Ok(id) => id,
        Err(resp) => return resp,
    };

    // 2. SHA format + non-default rejection.
    if let Err(resp) = validate_sha_params(&params) {
        return resp;
    }

    // 3. Coalesce the materialization.
    let state_cloned = Arc::clone(&state);
    let id_cloned = id;
    let result: Shared =
        coalesced_list_run_files(&state.files_in_flight, &id, move || async move {
            materialize_sandbox_path(&state_cloned, &id_cloned).await
        })
        .await;

    match (*result).clone() {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(err) => err.into_response(),
    }
}

fn validate_sha_params(params: &ListRunFilesParams) -> std::result::Result<(), Response> {
    validate_one_sha(params.from_sha.as_deref(), "from_sha")?;
    validate_one_sha(params.to_sha.as_deref(), "to_sha")?;
    // v1 rejects non-default values per R15 — default = absent.
    if params.from_sha.is_some() || params.to_sha.is_some() {
        return Err(ApiError::bad_request(
            "The `from_sha` and `to_sha` parameters are reserved for a future API version.",
        )
        .into_response());
    }
    Ok(())
}

fn validate_one_sha(value: Option<&str>, param_name: &str) -> std::result::Result<(), Response> {
    let Some(v) = value else {
        return Ok(());
    };
    if !(7..=40).contains(&v.len()) || !v.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request(format!(
            "Invalid `{param_name}` query parameter: expected a 7-40 char hex SHA."
        ))
        .into_response());
    }
    Ok(())
}

/// Materialize the response for `GET /runs/{id}/files`. Prefers the live
/// sandbox path; falls through to a `final_patch`-based degraded response
/// when the sandbox is unreachable or gone; falls through to an empty
/// envelope when neither is available.
async fn materialize_sandbox_path(state: &Arc<AppState>, run_id: &RunId) -> ListRunFilesResult {
    let start = Instant::now();

    let projection = load_projection(state, run_id).await?;

    let Some(base_sha) = projection.start.as_ref().and_then(|s| s.base_sha.clone()) else {
        // Run hasn't started yet (no base_sha). UI maps this to R4(a).
        return Ok(empty_envelope());
    };

    // Try to reconnect; on failure fall through to the patch-only fallback.
    let Some(sandbox) = try_reconnect_run_sandbox(state, &projection).await? else {
        return Ok(build_fallback_response(
            &projection,
            reason_for_fallback(&projection),
        ));
    };

    // Resolve HEAD to a concrete `to_sha`.
    let to_sha = resolve_head_sha(sandbox.as_ref()).await?;

    // Enumerate changes. Permanent errors (bad_sha, missing object) fall
    // through to the patch-only fallback; transient errors surface as 503.
    let mut raw_entries = match list_changed_files_raw(sandbox.as_ref(), &base_sha, &to_sha).await {
        Ok(v) => v,
        Err(DiffError::Permanent { .. }) => {
            return Ok(build_fallback_response(
                &projection,
                RunFilesMetaDegradedReason::SandboxGone,
            ));
        }
        Err(DiffError::Transient { message }) => {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Sandbox git subprocess failed: {message}"),
            ));
        }
    };

    let binary_paths = match list_binary_paths(sandbox.as_ref(), &base_sha, &to_sha).await {
        Ok(v) => v,
        Err(DiffError::Permanent { .. }) => HashSet::new(),
        Err(DiffError::Transient { message }) => {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Sandbox git numstat failed: {message}"),
            ));
        }
    };

    let total_changed_before_cap = raw_entries.len();
    // Cap the file count per R27.
    if raw_entries.len() > FILE_COUNT_CAP {
        raw_entries.truncate(FILE_COUNT_CAP);
    }

    // Apply denylist + classify binary/truncation before fetching blobs.
    let (entries_to_fetch, mut prebuilt) =
        classify_entries(&raw_entries, &binary_paths, is_sensitive);

    // Batched blob fetch for (old_blob, new_blob) pairs we still need.
    let blob_metas = fetch_blob_sizes(sandbox.as_ref(), &entries_to_fetch).await?;
    let blob_contents = fetch_blob_contents(sandbox.as_ref(), &entries_to_fetch).await?;

    // Stitch metas and contents back into FileDiff entries, enforcing per-file
    // and aggregate caps.
    let mut aggregate_bytes: u64 = 0;
    let mut files_omitted_by_budget: u64 = 0;
    let mut response_data: Vec<FileDiff> = Vec::with_capacity(raw_entries.len());
    response_data.append(&mut prebuilt);

    for (idx, entry) in entries_to_fetch.iter().enumerate() {
        let file_diff = stitch_file_diff(
            entry,
            blob_metas.get(idx).cloned(),
            blob_contents.get(idx).cloned().flatten(),
            &mut aggregate_bytes,
            &mut files_omitted_by_budget,
        );
        response_data.push(file_diff);
    }

    let truncated = total_changed_before_cap > FILE_COUNT_CAP
        || response_data.iter().any(|f| f.truncated.unwrap_or(false))
        || files_omitted_by_budget > 0;

    let (binary_count, sensitive_count, symlink_count, submodule_count) =
        count_flags(&response_data);

    let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    info!(
        run_id = %run_id,
        file_count = response_data.len(),
        bytes_total = aggregate_bytes,
        duration_ms,
        truncated,
        binary_count,
        sensitive_count,
        symlink_count,
        submodule_count,
        "Run files response produced"
    );

    Ok(PaginatedRunFileList {
        data: response_data,
        meta: RunFilesMeta {
            truncated,
            files_omitted_by_budget: (files_omitted_by_budget > 0)
                .then(|| i64::try_from(files_omitted_by_budget).unwrap_or(i64::MAX)),
            total_changed: i64::try_from(total_changed_before_cap).unwrap_or(i64::MAX),
            to_sha: Some(to_sha_wrapper(&to_sha)),
            to_sha_committed_at: None,
            degraded: Some(false),
            degraded_reason: None,
            patch: None,
        },
    })
}

/// Choose a degraded reason given the current projection. Docker-provider
/// runs aren't supported by the deployed server; completed runs are "gone";
/// everything else is a transient "unreachable" (sandbox may come back).
fn reason_for_fallback(projection: &fabro_store::RunProjection) -> RunFilesMetaDegradedReason {
    let provider = projection
        .sandbox
        .as_ref()
        .map(|s| s.provider.to_ascii_lowercase());
    if matches!(provider.as_deref(), Some("docker")) {
        return RunFilesMetaDegradedReason::ProviderUnsupported;
    }
    let is_terminal = projection
        .status
        .as_ref()
        .is_some_and(|s| s.status.is_terminal());
    if is_terminal {
        RunFilesMetaDegradedReason::SandboxGone
    } else {
        RunFilesMetaDegradedReason::SandboxUnreachable
    }
}

/// Build the degraded patch-only response from the stored `final_patch`.
/// When `final_patch` is `None`, returns the empty envelope (UI maps this to
/// R4(c)). Applies a 5 MiB cap, strips denylisted file sections from the
/// patch, and counts `diff --git` headers to populate `total_changed`.
fn build_fallback_response(
    projection: &fabro_store::RunProjection,
    reason: RunFilesMetaDegradedReason,
) -> PaginatedRunFileList {
    let Some(patch) = projection.final_patch.as_deref() else {
        return empty_envelope();
    };

    let (filtered_patch, truncated_by_cap) = apply_patch_cap(patch, AGGREGATE_BYTES_CAP);
    let filtered_patch = strip_denylisted_sections(&filtered_patch, is_sensitive);
    let total_changed = count_diff_headers(&filtered_patch);

    let to_sha = projection
        .conclusion
        .as_ref()
        .and_then(|c| c.final_git_commit_sha.clone())
        .map(|s| to_sha_wrapper(&s));

    PaginatedRunFileList {
        data: Vec::new(),
        meta: RunFilesMeta {
            truncated: truncated_by_cap,
            files_omitted_by_budget: None,
            total_changed: i64::try_from(total_changed).unwrap_or(i64::MAX),
            to_sha,
            to_sha_committed_at: None,
            degraded: Some(true),
            degraded_reason: Some(reason),
            patch: Some(filtered_patch),
        },
    }
}

/// Truncate a patch at `cap_bytes` on a UTF-8 character boundary. Returns
/// `(truncated_patch, was_truncated)`.
fn apply_patch_cap(patch: &str, cap_bytes: u64) -> (String, bool) {
    let cap = usize::try_from(cap_bytes).unwrap_or(usize::MAX);
    if patch.len() <= cap {
        return (patch.to_string(), false);
    }
    // Find the largest char boundary at-or-before `cap`.
    let mut boundary = cap;
    while boundary > 0 && !patch.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (patch[..boundary].to_string(), true)
}

/// Scan a unified patch for `diff --git a/<path> b/<path>` headers and strip
/// out whole file sections whose `<path>` matches the denylist. Matched
/// sections are replaced with a single `# sensitive file omitted` placeholder
/// line so the client's `PatchDiff` still renders the surrounding context.
fn strip_denylisted_sections(patch: &str, is_sensitive_fn: fn(&str) -> bool) -> String {
    let mut out = String::with_capacity(patch.len());
    let sections: Vec<&str> = patch.split_inclusive('\n').collect();

    let mut current_section: Vec<&str> = Vec::new();
    let mut current_sensitive = false;

    let flush = |buf: &mut String, section: &[&str], sensitive: bool| {
        if section.is_empty() {
            return;
        }
        if sensitive {
            use std::fmt::Write;
            let first = section.first().copied().unwrap_or("");
            let path = extract_diff_header_path(first)
                .unwrap_or("<sensitive>")
                .to_string();
            let _ = writeln!(buf, "# sensitive file omitted: {}", path.replace('\n', " "));
        } else {
            for line in section {
                buf.push_str(line);
            }
        }
    };

    for line in sections {
        if line.starts_with("diff --git ") {
            // Finish the previous section.
            flush(&mut out, &current_section, current_sensitive);
            current_section.clear();
            current_sensitive = match extract_diff_header_path(line) {
                Some(path) => is_sensitive_fn(path),
                None => false,
            };
        }
        current_section.push(line);
    }
    flush(&mut out, &current_section, current_sensitive);
    out
}

/// Parse the new-side path from a `diff --git a/<old> b/<new>` header line.
fn extract_diff_header_path(header_line: &str) -> Option<&str> {
    let trimmed = header_line.strip_prefix("diff --git ")?;
    let trimmed = trimmed.strip_suffix('\n').unwrap_or(trimmed);
    // Formats: `a/<old> b/<new>`. Find the last `b/` token.
    let b_slash = trimmed.rfind(" b/")?;
    Some(&trimmed[b_slash + 3..])
}

/// Count `diff --git` header occurrences (each marks one changed file) in the
/// filtered patch.
fn count_diff_headers(patch: &str) -> usize {
    patch
        .lines()
        .filter(|l| l.starts_with("diff --git "))
        .count()
}

fn empty_envelope() -> PaginatedRunFileList {
    PaginatedRunFileList {
        data: Vec::new(),
        meta: RunFilesMeta {
            truncated:               false,
            files_omitted_by_budget: None,
            total_changed:           0,
            to_sha:                  None,
            to_sha_committed_at:     None,
            degraded:                Some(false),
            degraded_reason:         None,
            patch:                   None,
        },
    }
}

fn to_sha_wrapper(sha: &str) -> RunFilesMetaToSha {
    // `RunFilesMetaToSha` is a newtype wrapper around String with a pattern
    // constraint. Values we produce (via `git rev-parse HEAD`) always match.
    // `try_from` is expected to succeed; fall back to an empty wrapper on
    // the impossible failure rather than panicking.
    RunFilesMetaToSha::try_from(sha.to_string())
        .unwrap_or_else(|_| RunFilesMetaToSha::try_from(String::from("0000000")).unwrap())
}

/// Load the run projection from the store, returning a 404 for the IDOR-safe
/// "run missing or inaccessible" case.
async fn load_projection(
    state: &Arc<AppState>,
    run_id: &RunId,
) -> std::result::Result<fabro_store::RunProjection, ApiError> {
    let reader = state
        .store_ref()
        .open_run_reader(run_id)
        .await
        .map_err(|_| ApiError::not_found("Run not found."))?;
    reader
        .state()
        .await
        .map_err(|err| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}

/// Reconnect semantics tailored to the Files endpoint:
/// - `Ok(Some(sandbox))`: reconnected, caller proceeds on the sandbox path.
/// - `Ok(None)`: no sandbox record, reconnect failed, or the provider isn't
///   supported by this build — caller falls through to the fallback branch
///   (Unit 6) instead of returning 409.
/// - `Err(ApiError)`: unrecoverable error loading run state.
async fn try_reconnect_run_sandbox(
    state: &Arc<AppState>,
    projection: &fabro_store::RunProjection,
) -> std::result::Result<Option<Box<dyn Sandbox>>, ApiError> {
    let Some(record) = projection.sandbox.clone() else {
        return Ok(None);
    };
    let daytona_api_key = state.vault_or_env_pub("DAYTONA_API_KEY");
    match reconnect(&record, daytona_api_key).await {
        Ok(sandbox) => Ok(Some(sandbox)),
        Err(_) => Ok(None),
    }
}

async fn resolve_head_sha(sandbox: &dyn Sandbox) -> std::result::Result<String, ApiError> {
    let res = sandbox
        .exec_command(
            "git rev-parse HEAD",
            SANDBOX_GIT_TIMEOUT_MS,
            None,
            None,
            None,
        )
        .await
        .map_err(|err| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, err))?;
    if res.exit_code != 0 {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Failed to resolve sandbox HEAD.",
        ));
    }
    let sha = res.stdout.trim().to_string();
    if sha.is_empty() {
        return Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Sandbox HEAD resolved to an empty value.",
        ));
    }
    Ok(sha)
}

/// Partition raw entries into (entries that need blob fetches) and
/// (already-complete FileDiff entries: sensitive/binary/symlink/submodule/
/// deleted). Returned `Vec` orderings are preserved so meta counts work.
fn classify_entries(
    raw: &[RawDiffEntry],
    binary_paths: &HashSet<String>,
    is_sensitive_fn: fn(&str) -> bool,
) -> (Vec<RawDiffEntry>, Vec<FileDiff>) {
    let mut fetch = Vec::with_capacity(raw.len());
    let mut prebuilt = Vec::new();

    for entry in raw {
        let (primary_path, old_path) = match entry {
            RawDiffEntry::Added { path, .. }
            | RawDiffEntry::Modified { path, .. }
            | RawDiffEntry::Deleted { path, .. }
            | RawDiffEntry::Symlink { path, .. }
            | RawDiffEntry::Submodule { path, .. } => (path.as_str(), path.as_str()),
            RawDiffEntry::Renamed {
                old_path, new_path, ..
            } => (new_path.as_str(), old_path.as_str()),
        };

        let sensitive = is_sensitive_fn(primary_path) || is_sensitive_fn(old_path);

        if sensitive {
            prebuilt.push(build_placeholder_file_diff(
                entry,
                &PlaceholderKind::Sensitive,
            ));
            continue;
        }

        match entry {
            RawDiffEntry::Symlink { .. } => {
                prebuilt.push(build_placeholder_file_diff(
                    entry,
                    &PlaceholderKind::Symlink,
                ));
            }
            RawDiffEntry::Submodule { .. } => {
                prebuilt.push(build_placeholder_file_diff(
                    entry,
                    &PlaceholderKind::Submodule,
                ));
            }
            _ if binary_paths.contains(primary_path) => {
                prebuilt.push(build_placeholder_file_diff(entry, &PlaceholderKind::Binary));
            }
            _ => {
                fetch.push(entry.clone());
            }
        }
    }

    (fetch, prebuilt)
}

enum PlaceholderKind {
    Sensitive,
    Binary,
    Symlink,
    Submodule,
}

fn build_placeholder_file_diff(entry: &RawDiffEntry, kind: &PlaceholderKind) -> FileDiff {
    let (old_name, new_name, change_kind) = names_and_kind(entry);
    FileDiff {
        binary:            match kind {
            PlaceholderKind::Binary => Some(true),
            _ => None,
        },
        change_kind:       Some(change_kind),
        new_file:          DiffFile {
            name:     new_name,
            contents: String::new(),
        },
        old_file:          DiffFile {
            name:     old_name,
            contents: String::new(),
        },
        sensitive:         matches!(kind, PlaceholderKind::Sensitive).then_some(true),
        truncated:         None,
        truncation_reason: None,
    }
}

fn names_and_kind(entry: &RawDiffEntry) -> (String, String, FileDiffChangeKind) {
    match entry {
        RawDiffEntry::Added { path, .. } => {
            (String::new(), path.clone(), FileDiffChangeKind::Added)
        }
        RawDiffEntry::Modified { path, .. } => {
            (path.clone(), path.clone(), FileDiffChangeKind::Modified)
        }
        RawDiffEntry::Deleted { path, .. } => {
            (path.clone(), String::new(), FileDiffChangeKind::Deleted)
        }
        RawDiffEntry::Renamed {
            old_path, new_path, ..
        } => (
            old_path.clone(),
            new_path.clone(),
            FileDiffChangeKind::Renamed,
        ),
        RawDiffEntry::Symlink {
            path, change_kind, ..
        } => {
            let (old, new) = match change_kind {
                SymlinkChange::Added => (String::new(), path.clone()),
                SymlinkChange::Deleted => (path.clone(), String::new()),
                SymlinkChange::Modified => (path.clone(), path.clone()),
            };
            (old, new, FileDiffChangeKind::Symlink)
        }
        RawDiffEntry::Submodule {
            path, change_kind, ..
        } => {
            let (old, new) = match change_kind {
                SubmoduleChange::Added => (String::new(), path.clone()),
                SubmoduleChange::Deleted => (path.clone(), String::new()),
                SubmoduleChange::Modified => (path.clone(), path.clone()),
            };
            (old, new, FileDiffChangeKind::Submodule)
        }
    }
}

fn stitch_file_diff(
    entry: &RawDiffEntry,
    _meta: Option<BlobMeta>,
    contents: Option<String>,
    aggregate_bytes: &mut u64,
    files_omitted_by_budget: &mut u64,
) -> FileDiff {
    let (old_name, new_name, change_kind) = names_and_kind(entry);

    // `contents` already has per-file size cap applied by `stream_blobs`; a
    // `None` here means that file exceeded the per-file cap.
    match contents {
        None => FileDiff {
            binary:            None,
            change_kind:       Some(change_kind),
            new_file:          DiffFile {
                name:     new_name,
                contents: String::new(),
            },
            old_file:          DiffFile {
                name:     old_name,
                contents: String::new(),
            },
            sensitive:         None,
            truncated:         Some(true),
            truncation_reason: Some(FileDiffTruncationReason::FileTooLarge),
        },
        Some(content) => {
            // Line-count cap.
            let lines = content.lines().count();
            if lines > PER_FILE_LINES_CAP {
                return FileDiff {
                    binary:            None,
                    change_kind:       Some(change_kind),
                    new_file:          DiffFile {
                        name:     new_name,
                        contents: String::new(),
                    },
                    old_file:          DiffFile {
                        name:     old_name,
                        contents: String::new(),
                    },
                    sensitive:         None,
                    truncated:         Some(true),
                    truncation_reason: Some(FileDiffTruncationReason::FileTooLarge),
                };
            }

            let byte_len = content.len() as u64;
            let new_total = aggregate_bytes.saturating_add(byte_len);
            if new_total > AGGREGATE_BYTES_CAP {
                *files_omitted_by_budget += 1;
                return FileDiff {
                    binary:            None,
                    change_kind:       Some(change_kind),
                    new_file:          DiffFile {
                        name:     new_name,
                        contents: String::new(),
                    },
                    old_file:          DiffFile {
                        name:     old_name,
                        contents: String::new(),
                    },
                    sensitive:         None,
                    truncated:         Some(true),
                    truncation_reason: Some(FileDiffTruncationReason::BudgetExhausted),
                };
            }

            *aggregate_bytes = new_total;

            // For added files, old side is empty; for deleted, new side is
            // empty; for modified/renamed, both sides hold the same `new`
            // contents (v1 keeps the wire simple; a richer two-blob fetch
            // can come later).
            let (old_contents, new_contents) = match entry {
                RawDiffEntry::Added { .. } => (String::new(), content),
                RawDiffEntry::Deleted { .. } => (content, String::new()),
                _ => (content.clone(), content),
            };

            FileDiff {
                binary:            None,
                change_kind:       Some(change_kind),
                new_file:          DiffFile {
                    name:     new_name,
                    contents: new_contents,
                },
                old_file:          DiffFile {
                    name:     old_name,
                    contents: old_contents,
                },
                sensitive:         None,
                truncated:         None,
                truncation_reason: None,
            }
        }
    }
}

async fn fetch_blob_sizes(
    sandbox: &dyn Sandbox,
    entries: &[RawDiffEntry],
) -> std::result::Result<Vec<BlobMeta>, ApiError> {
    let shas = collect_primary_blobs(entries);
    if shas.is_empty() {
        return Ok(Vec::new());
    }
    match stream_blob_metadata(sandbox, &shas).await {
        Ok(v) => Ok(v),
        Err(DiffError::Permanent { .. }) => Ok(Vec::new()),
        Err(DiffError::Transient { message }) => Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Sandbox git cat-file --batch-check failed: {message}"),
        )),
    }
}

async fn fetch_blob_contents(
    sandbox: &dyn Sandbox,
    entries: &[RawDiffEntry],
) -> std::result::Result<Vec<Option<String>>, ApiError> {
    let shas = collect_primary_blobs(entries);
    if shas.is_empty() {
        return Ok(Vec::new());
    }
    match stream_blobs(sandbox, &shas, PER_FILE_BYTES_CAP).await {
        Ok(v) => Ok(v),
        Err(DiffError::Permanent { .. }) => Ok(entries.iter().map(|_| None).collect()),
        Err(DiffError::Transient { message }) => Err(ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Sandbox git cat-file --batch failed: {message}"),
        )),
    }
}

fn collect_primary_blobs(entries: &[RawDiffEntry]) -> Vec<String> {
    entries
        .iter()
        .filter_map(|e| match e {
            RawDiffEntry::Added { new_blob, .. }
            | RawDiffEntry::Modified { new_blob, .. }
            | RawDiffEntry::Renamed { new_blob, .. } => Some(new_blob.clone()),
            RawDiffEntry::Deleted { old_blob, .. } => Some(old_blob.clone()),
            RawDiffEntry::Symlink { .. } | RawDiffEntry::Submodule { .. } => None,
        })
        .collect()
}

fn count_flags(data: &[FileDiff]) -> (u64, u64, u64, u64) {
    let mut binary = 0;
    let mut sensitive = 0;
    let mut symlink = 0;
    let mut submodule = 0;
    for d in data {
        if d.binary.unwrap_or(false) {
            binary += 1;
        }
        if d.sensitive.unwrap_or(false) {
            sensitive += 1;
        }
        match d.change_kind {
            Some(FileDiffChangeKind::Symlink) => symlink += 1,
            Some(FileDiffChangeKind::Submodule) => submodule += 1,
            _ => {}
        }
    }
    (binary, sensitive, symlink, submodule)
}

/// Path-based sensitive-file denylist (Unit 8 extracts this into a shared
/// module; an inline set keeps the handler self-contained for Unit 5).
///
/// `basename_lower` is already ASCII-lowercased, so the suffix checks below
/// are effectively case-insensitive despite clippy's heuristic lint.
#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn is_sensitive(path: &str) -> bool {
    let basename = path.rsplit_once('/').map_or(path, |(_, name)| name);
    let basename_lower = basename.to_ascii_lowercase();
    let basename_match = basename_lower == ".env"
        || basename_lower.starts_with(".env.")
        || basename_lower.ends_with(".pem")
        || basename_lower.starts_with("id_rsa")
        || basename_lower.ends_with(".p12")
        || basename_lower.ends_with(".keystore")
        || basename_lower.ends_with(".key");
    if basename_match {
        return true;
    }
    // Path-suffix patterns.
    path.ends_with(".aws/credentials")
        || path.ends_with(".git/config")
        || path.contains("/.ssh/")
        || path.starts_with(".ssh/")
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use fabro_types::RunId;
    use tokio::time::{Duration, sleep};

    use super::*;

    fn run_id(_name: &str) -> RunId {
        // RunIds are ULIDs, not arbitrary strings; each test just needs
        // distinct values.
        RunId::new()
    }

    fn new_registry() -> FilesInFlight {
        new_files_in_flight()
    }

    fn ok_response() -> PaginatedRunFileList {
        PaginatedRunFileList {
            data: Vec::new(),
            meta: fabro_api::types::RunFilesMeta {
                truncated:               false,
                files_omitted_by_budget: None,
                total_changed:           0,
                to_sha:                  None,
                to_sha_committed_at:     None,
                degraded:                None,
                degraded_reason:         None,
                patch:                   None,
            },
        }
    }

    #[tokio::test]
    async fn concurrent_calls_for_same_run_share_one_materialization() {
        let inflight = new_registry();
        let counter = Arc::new(AtomicUsize::new(0));
        let run = run_id("run_aaaaaaaaaaaaaaaaaaaaaaaaaa");

        let materialize = {
            let counter = Arc::clone(&counter);
            move || {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    sleep(Duration::from_millis(30)).await;
                    Ok(ok_response())
                }
            }
        };

        let mat_a = materialize.clone();
        let mat_b = materialize;
        let inflight_a = Arc::clone(&inflight);
        let inflight_b = Arc::clone(&inflight);
        let run_a = run;
        let run_b = run;

        let (a, b) = tokio::join!(
            tokio::spawn(async move { coalesced_list_run_files(&inflight_a, &run_a, mat_a).await }),
            tokio::spawn(async move { coalesced_list_run_files(&inflight_b, &run_b, mat_b).await }),
        );

        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert!(a.unwrap().is_ok());
        assert!(b.unwrap().is_ok());
    }

    #[tokio::test]
    async fn different_run_ids_materialize_in_parallel() {
        let inflight = new_registry();
        let counter = Arc::new(AtomicUsize::new(0));
        let run1 = run_id("run_aaaaaaaaaaaaaaaaaaaaaaaaaa");
        let run2 = run_id("run_bbbbbbbbbbbbbbbbbbbbbbbbbb");

        let make = |counter: Arc<AtomicUsize>| {
            move || {
                let counter = counter.clone();
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    sleep(Duration::from_millis(10)).await;
                    Ok(ok_response())
                }
            }
        };

        let i1 = Arc::clone(&inflight);
        let i2 = Arc::clone(&inflight);
        let m1 = make(Arc::clone(&counter));
        let m2 = make(Arc::clone(&counter));
        let (r1, r2) = tokio::join!(
            coalesced_list_run_files(&i1, &run1, m1),
            coalesced_list_run_files(&i2, &run2, m2),
        );

        assert_eq!(counter.load(Ordering::SeqCst), 2);
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }

    #[tokio::test]
    async fn panic_surfaces_as_internal_error_and_does_not_poison_future_calls() {
        let inflight = new_registry();
        let run = run_id("run_cccccccccccccccccccccccccc");

        let first = coalesced_list_run_files(&inflight, &run, || async {
            panic!("boom");
        })
        .await;
        assert!(first.is_err(), "expected panic to become error");
        assert_eq!(
            first.as_ref().as_ref().unwrap_err().status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );

        // Give the spawned cleanup task a moment to remove the entry.
        sleep(Duration::from_millis(20)).await;

        // A subsequent call on the same run_id triggers a fresh materialization.
        let second =
            coalesced_list_run_files(&inflight, &run, || async { Ok(ok_response()) }).await;
        assert!(second.is_ok());
    }

    #[tokio::test]
    async fn sequential_calls_trigger_fresh_materialization() {
        let inflight = new_registry();
        let counter = Arc::new(AtomicUsize::new(0));
        let run = run_id("run_dddddddddddddddddddddddddd");

        for _ in 0..3 {
            let counter_inner = Arc::clone(&counter);
            let _ = coalesced_list_run_files(&inflight, &run, move || async move {
                counter_inner.fetch_add(1, Ordering::SeqCst);
                Ok(ok_response())
            })
            .await;
            // Yield to let the spawned task clean up the registry entry before
            // the next iteration.
            sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    // ── Degraded-fallback helpers (Unit 6) ────────────────────────────────

    #[test]
    fn apply_patch_cap_truncates_at_char_boundary() {
        let patch = "Hello, world! 你好, 世界!";
        let (truncated, was) = apply_patch_cap(patch, 16);
        assert!(was);
        assert!(truncated.len() <= 16);
        // Must still be valid UTF-8 (implicit — String::from would have
        // panicked otherwise).
        assert!(!truncated.is_empty());
    }

    #[test]
    fn apply_patch_cap_keeps_short_patches_unchanged() {
        let (out, was) = apply_patch_cap("small", 100);
        assert_eq!(out, "small");
        assert!(!was);
    }

    #[test]
    fn strip_denylisted_sections_replaces_sensitive_files_with_placeholder() {
        let patch = "\
diff --git a/src/foo.rs b/src/foo.rs
@@ -1 +1 @@
-a
+b
diff --git a/.env.production b/.env.production
@@ -1 +1 @@
-SECRET=old
+SECRET=new
diff --git a/src/bar.rs b/src/bar.rs
@@ -1 +1 @@
-x
+y
";
        let out = strip_denylisted_sections(patch, is_sensitive);
        assert!(out.contains("src/foo.rs"), "kept non-sensitive: {out}");
        assert!(out.contains("src/bar.rs"), "kept non-sensitive: {out}");
        assert!(!out.contains("SECRET="), "denylisted content leaked: {out}");
        assert!(
            out.contains("# sensitive file omitted: .env.production"),
            "placeholder missing: {out}"
        );
    }

    #[test]
    fn count_diff_headers_counts_file_sections() {
        let patch = "diff --git a/a.rs b/a.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/b.rs b/b.rs\n@@ -1 +1 @@\n-c\n+d\n";
        assert_eq!(count_diff_headers(patch), 2);
    }

    #[test]
    fn extract_diff_header_path_pulls_new_side_from_header() {
        assert_eq!(
            extract_diff_header_path("diff --git a/src/foo.rs b/src/bar.rs\n"),
            Some("src/bar.rs")
        );
        // Paths with spaces in them exercise the `rfind(" b/")` heuristic —
        // git emits unquoted paths only when they're simple.
        assert_eq!(
            extract_diff_header_path("diff --git a/plain.rs b/plain.rs"),
            Some("plain.rs")
        );
    }

    #[test]
    fn is_sensitive_matches_common_secret_paths() {
        assert!(is_sensitive(".env.production"));
        assert!(is_sensitive("config/.env"));
        assert!(is_sensitive("keys/id_rsa"));
        assert!(is_sensitive("SERVER.PEM"));
        assert!(is_sensitive("home/user/.aws/credentials"));
        assert!(is_sensitive("home/user/.ssh/config"));
        assert!(!is_sensitive("src/main.rs"));
        assert!(!is_sensitive("README.md"));
    }
}
