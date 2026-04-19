import type { RefObject } from "react";

export type DiffStyle = "split" | "unified";

export function Toolbar({
  onRefresh,
  refreshing,
  refreshDisabled,
  freshness,
  refreshButtonRef,
  diffStyle,
  onDiffStyleChange,
  diffStyleForced,
}: {
  onRefresh: () => void;
  refreshing: boolean;
  /** True when the server has nothing new to show (to_sha unchanged). */
  refreshDisabled: boolean;
  freshness: string | null;
  refreshButtonRef?: RefObject<HTMLButtonElement | null>;
  diffStyle: DiffStyle;
  onDiffStyleChange: (style: DiffStyle) => void;
  /**
   * True when the md breakpoint has forced unified view — the toggle
   * reflects the forced state but saving it would stomp the user's
   * desktop preference, so the parent keeps persistence off while
   * `diffStyleForced` is true.
   */
  diffStyleForced: boolean;
}) {
  const disabled = refreshing || refreshDisabled;
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-line bg-panel/40 px-3 py-2 text-xs text-fg-muted">
      <div className="flex items-center gap-3">
        <span aria-live="polite" className="min-w-0 truncate">
          {freshness ?? "\u00A0"}
        </span>
      </div>
      <div className="flex items-center gap-2">
        <DiffStyleToggle
          value={diffStyle}
          onChange={onDiffStyleChange}
          forced={diffStyleForced}
        />
        <button
          ref={refreshButtonRef}
          type="button"
          onClick={onRefresh}
          disabled={disabled}
          aria-label={refreshing ? "Refreshing files" : "Refresh files"}
          className="min-h-[44px] min-w-[44px] rounded-md border border-line bg-panel px-3 py-1 text-xs font-medium text-fg-2 transition-colors hover:bg-overlay disabled:opacity-60"
        >
          {refreshing ? "Refreshing…" : "Refresh"}
        </button>
      </div>
    </div>
  );
}

function DiffStyleToggle({
  value,
  onChange,
  forced,
}: {
  value: DiffStyle;
  onChange: (style: DiffStyle) => void;
  forced: boolean;
}) {
  const btn =
    "min-h-[44px] rounded-md border border-line px-3 py-1 text-xs font-medium transition-colors disabled:opacity-60";
  const active = "bg-overlay text-fg-1";
  const inactive = "bg-panel text-fg-2 hover:bg-overlay";
  return (
    <div
      className="flex items-center gap-1"
      role="group"
      aria-label="Diff layout"
    >
      <button
        type="button"
        onClick={() => onChange("split")}
        disabled={forced}
        aria-pressed={value === "split"}
        className={`${btn} ${value === "split" ? active : inactive}`}
      >
        Split
      </button>
      <button
        type="button"
        onClick={() => onChange("unified")}
        disabled={forced}
        aria-pressed={value === "unified"}
        className={`${btn} ${value === "unified" ? active : inactive}`}
      >
        Unified
      </button>
    </div>
  );
}
