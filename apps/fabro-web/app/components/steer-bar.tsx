import {
  forwardRef,
  useImperativeHandle,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from "react";
import { CheckIcon } from "@heroicons/react/16/solid";

export interface SteerBarProps {
  runId: string;
}

export interface SteerBarHandle {
  focus(): void;
}

export const SteerBar = forwardRef<SteerBarHandle, SteerBarProps>(function SteerBar(
  { runId: _runId },
  ref,
) {
  const [text, setText] = useState("");
  const [interrupt, setInterrupt] = useState(false);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  useImperativeHandle(ref, () => ({
    focus() {
      textareaRef.current?.focus();
    },
  }));
  const trimmed = text.trim();
  const canSubmit = trimmed.length > 0;

  function handleSubmit(e: FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    setText("");
  }

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (canSubmit) {
        setText("");
      }
    }
  }

  return (
    <form
      onSubmit={handleSubmit}
      aria-label="Steer running agent"
      className="mx-auto flex max-w-4xl items-end gap-2 px-4 py-3 sm:px-6 lg:px-8"
    >
      <textarea
        ref={textareaRef}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Steer the agent…"
        rows={1}
        maxLength={8192}
        aria-label="Steering message"
        className="flex-1 resize-none rounded-md bg-overlay px-3 py-2 text-sm text-fg outline-1 -outline-offset-1 outline-line-strong placeholder:text-fg-muted focus:outline-2 focus:-outline-offset-1 focus:outline-teal-500"
      />
      <button
        type="button"
        role="checkbox"
        aria-checked={interrupt}
        onClick={() => setInterrupt((v) => !v)}
        className={`inline-flex shrink-0 items-center gap-2 rounded-md px-3 py-2 text-sm font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500 ${
          interrupt
            ? "bg-amber/15 text-amber outline-1 -outline-offset-1 outline-amber/60 hover:bg-amber/20"
            : "bg-overlay text-fg-2 outline-1 -outline-offset-1 outline-line-strong hover:bg-overlay-strong hover:text-fg"
        }`}
      >
        <span
          aria-hidden="true"
          className={`flex size-3.5 items-center justify-center rounded-sm border ${
            interrupt
              ? "border-amber bg-amber"
              : "border-line-strong bg-panel-alt"
          }`}
        >
          <CheckIcon
            className={`size-2.5 text-on-primary ${interrupt ? "opacity-100" : "opacity-0"}`}
          />
        </span>
        Interrupt
      </button>
      <button
        type="submit"
        disabled={!canSubmit}
        className="inline-flex shrink-0 items-center justify-center rounded-md bg-teal-500 px-4 py-2 text-sm font-medium text-on-primary transition-colors hover:bg-teal-300 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:bg-teal-500"
      >
        Send
      </button>
    </form>
  );
});
