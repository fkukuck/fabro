// Shared UI primitives. The install wizard set the visual baseline; this file
// exposes the primary button, secondary button, input, error message, and
// copy button so the auth and in-app surfaces can match.

import { useState } from "react";
import {
  ClipboardDocumentCheckIcon,
  ClipboardIcon,
} from "@heroicons/react/16/solid";

export const INPUT_CLASS =
  "block w-full rounded-lg bg-panel-alt px-3.5 py-2.5 text-base text-fg outline-1 -outline-offset-1 outline-white/10 placeholder:text-fg-muted focus:outline-2 focus:-outline-offset-1 focus:outline-teal-500 sm:text-sm";

export const PRIMARY_BUTTON_CLASS =
  "inline-flex items-center justify-center gap-2 rounded-lg bg-teal-500 px-4 py-2 text-sm font-medium text-on-primary transition-colors hover:bg-teal-300 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:bg-teal-500";

export const SECONDARY_BUTTON_CLASS =
  "inline-flex items-center justify-center gap-2 rounded-lg bg-transparent px-3.5 py-2 text-sm font-medium text-fg-2 outline-1 -outline-offset-1 outline-white/10 hover:bg-overlay hover:text-fg focus-visible:outline-2 focus-visible:-outline-offset-1 focus-visible:outline-teal-500";

export function ErrorMessage({ message }: { message: string }) {
  return (
    <p
      role="alert"
      className="rounded-md bg-coral/10 px-3 py-2 text-sm/6 text-fg-2 outline-1 -outline-offset-1 outline-coral/40"
    >
      {message}
    </p>
  );
}

export function CopyButton({
  value,
  label,
  className = "",
}: {
  value: string;
  label: string;
  className?: string;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      onClick={async () => {
        try {
          await navigator.clipboard.writeText(value);
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1500);
        } catch {
          // Clipboard may be blocked; leave state unchanged.
        }
      }}
      className={`inline-flex size-6 shrink-0 items-center justify-center rounded text-fg-muted outline-teal-500 hover:bg-overlay hover:text-fg-2 focus-visible:outline-2 focus-visible:outline-offset-1 ${className}`}
      aria-label={copied ? "Copied" : label}
      title={copied ? "Copied" : label}
    >
      {copied ? (
        <ClipboardDocumentCheckIcon className="size-4 text-mint" />
      ) : (
        <ClipboardIcon className="size-4" />
      )}
    </button>
  );
}
