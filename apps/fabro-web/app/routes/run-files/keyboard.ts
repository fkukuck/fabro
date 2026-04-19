import type { RefObject } from "react";
import { useEffect } from "react";

export function isEditableElement(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName.toLowerCase();
  if (tag === "input" || tag === "textarea" || tag === "select") return true;
  return (el as HTMLElement).isContentEditable === true;
}

/**
 * Wire keyboard navigation across file rows. `j` / `k` move focus to the
 * next / previous row; `Enter` and `Space` trigger a `click` on the focused
 * row so diff wrappers that opt in can expand/collapse. Key presses while a
 * text-editable element is focused are left alone.
 */
export function useFileKeyboardNav(
  containerRef: RefObject<HTMLDivElement | null>,
  fileCount: number,
) {
  useEffect(() => {
    if (!containerRef.current) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (isEditableElement(document.activeElement)) return;
      const container = containerRef.current;
      if (!container) return;
      const rows = Array.from(
        container.querySelectorAll<HTMLElement>('[data-run-file-row="true"]'),
      );
      if (rows.length === 0) return;

      const active = document.activeElement as HTMLElement | null;
      const currentIdx = rows.findIndex((row) => row.contains(active));

      if (event.key === "j" || event.key === "k") {
        let nextIdx: number;
        if (currentIdx < 0) {
          nextIdx = 0;
        } else {
          nextIdx = event.key === "j" ? currentIdx + 1 : currentIdx - 1;
        }
        if (nextIdx < 0 || nextIdx >= rows.length) return;
        event.preventDefault();
        const target = rows[nextIdx];
        target.focus({ preventScroll: false });
        target.scrollIntoView({ block: "nearest", behavior: "smooth" });
        return;
      }

      if (event.key === "Enter" || event.key === " ") {
        if (currentIdx < 0) return;
        event.preventDefault();
        // Forward to the row element as a click so any @pierre/diffs
        // expand-handler the consumer wires up fires naturally.
        rows[currentIdx].click();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
    // fileCount drives re-attachment so rows picked up after data changes
    // stay addressable without stale references.
  }, [containerRef, fileCount]);
}
