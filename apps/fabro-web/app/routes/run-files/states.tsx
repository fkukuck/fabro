import { isRouteErrorResponse, useRouteError } from "react-router";

/**
 * R4 empty-state taxonomy. See plan § Unit 11:
 *   - `starting` (R4a): run still spinning up, no base_sha yet
 *   - `no_changes` (R4b): run completed but touched no files
 *   - `failed_before_checkpoint` (R4c1): failed run without captured diff
 *   - `diff_lost` (R4c2): succeeded run whose diff is no longer recoverable
 *   - `unknown`: fallback — loader returned null (404/501/other)
 */
export type EmptyKind =
  | "starting"
  | "no_changes"
  | "failed_before_checkpoint"
  | "diff_lost"
  | "unknown";

export function EmptyState({ kind }: { kind: EmptyKind }) {
  return (
    <div
      role="status"
      className="rounded-md border border-dashed border-line bg-panel/40 px-6 py-10 text-center text-sm text-fg-muted"
    >
      {emptyStateCopy(kind)}
    </div>
  );
}

export function emptyStateCopy(kind: EmptyKind): string {
  switch (kind) {
    case "starting":
      return "Run is still starting. Files will appear once it begins.";
    case "no_changes":
      return "This run didn't change any files.";
    case "failed_before_checkpoint":
      return "This run failed before capturing any changes.";
    case "diff_lost":
      return "The diff for this run is no longer available. If you expect files here, please report it.";
    case "unknown":
    default:
      return "The diff for this run is not available right now.";
  }
}

/// Derive the empty-state variant from the full loader context. `runStatus`
/// comes from the parent run loader; its absence collapses to the "unknown"
/// catchall so the empty state never displays misleading copy.
export function deriveEmptyKind(args: {
  runStatus: string | undefined;
  totalChanged: number;
  degraded: boolean;
}): EmptyKind {
  const { runStatus, totalChanged, degraded } = args;
  if (!runStatus) {
    return "unknown";
  }
  const normalized = runStatus.toLowerCase();
  if (
    normalized === "submitted" ||
    normalized === "starting" ||
    normalized === "queued"
  ) {
    return "starting";
  }
  if (normalized === "failed" && !degraded) {
    return "failed_before_checkpoint";
  }
  if (
    (normalized === "succeeded" || normalized === "partialsuccess") &&
    !degraded
  ) {
    // If the run ran successfully and we still have no diff data, the
    // projection's final_patch was never captured (or was lost). R4(c2).
    if (totalChanged > 0) {
      return "diff_lost";
    }
    return "no_changes";
  }
  if (totalChanged === 0) {
    return "no_changes";
  }
  return "diff_lost";
}

export function LoadingSkeleton() {
  return (
    <div className="flex flex-col gap-3" aria-label="Loading files">
      <div className="h-8 rounded-md bg-panel/60 motion-safe:animate-pulse" />
      <div className="h-32 rounded-md bg-panel/60 motion-safe:animate-pulse" />
      <div className="h-32 rounded-md bg-panel/60 motion-safe:animate-pulse" />
    </div>
  );
}

export function InlineErrorBanner({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-rose-500/30 bg-rose-950/20 px-4 py-3 text-sm text-rose-100">
      <span>{message}</span>
      <button
        type="button"
        onClick={onRetry}
        className="min-h-[32px] rounded-md border border-rose-500/40 bg-rose-950/40 px-3 py-1 text-xs font-medium text-rose-50 transition-colors hover:bg-rose-950/60"
      >
        Retry
      </button>
    </div>
  );
}

export function Toast({ children }: { children: React.ReactNode }) {
  return (
    <div
      role="status"
      aria-live="polite"
      className="pointer-events-none fixed bottom-6 right-6 z-50 rounded-md border border-line bg-panel/95 px-3 py-2 text-xs text-fg-2 shadow-lg"
    >
      {children}
    </div>
  );
}

/**
 * Route-level ErrorBoundary that handles the documented status codes from
 * the plan § Unit 11 taxonomy. 500 responses with a `request_id` in the
 * body surface it in the copy so users can cite it when contacting support.
 */
export function RunFilesErrorBoundary() {
  const error = useRouteError();
  if (isRouteErrorResponse(error)) {
    if (error.status === 401 || error.status === 403) {
      return (
        <div className="rounded-md border border-dashed border-line bg-panel/40 px-6 py-10 text-center text-sm text-fg-muted">
          You don't have access to this run's files.
        </div>
      );
    }
    if (error.status === 503 || error.status === 429) {
      return (
        <InlineErrorBanner
          message="The diff service is temporarily unavailable."
          onRetry={() => window.location.reload()}
        />
      );
    }
    if (error.status === 500) {
      const requestId = extractRequestId(error.data);
      return (
        <div className="rounded-md border border-dashed border-line bg-panel/40 px-6 py-10 text-center text-sm text-fg-muted">
          Something went wrong.
          {requestId ? ` Request ID: ${requestId}.` : null} Please contact
          support if this persists.
        </div>
      );
    }
    return (
      <div className="rounded-md border border-dashed border-line bg-panel/40 px-6 py-10 text-center text-sm text-fg-muted">
        Something went wrong ({error.status}).
      </div>
    );
  }
  return (
    <div className="rounded-md border border-dashed border-line bg-panel/40 px-6 py-10 text-center text-sm text-fg-muted">
      Something went wrong loading this run's files.
    </div>
  );
}

function extractRequestId(body: unknown): string | null {
  if (!body || typeof body !== "object") return null;
  const b = body as Record<string, unknown>;
  if (typeof b.request_id === "string") return b.request_id;
  const errors = b.errors;
  if (Array.isArray(errors) && errors.length > 0) {
    const first = errors[0];
    if (first && typeof first === "object") {
      const detail = (first as Record<string, unknown>).detail;
      if (typeof detail === "string") {
        const match = detail.match(/request[_ ]id[=:]?\s*([a-zA-Z0-9-_]+)/i);
        if (match) return match[1];
      }
      const reqId = (first as Record<string, unknown>).request_id;
      if (typeof reqId === "string") return reqId;
    }
  }
  return null;
}
