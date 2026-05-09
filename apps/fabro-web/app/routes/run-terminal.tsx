import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Terminal as XtermTerminal } from "@xterm/xterm";
import type { FitAddon as XtermFitAddon } from "@xterm/addon-fit";
import {
  ArrowPathIcon,
  ClipboardDocumentIcon,
} from "@heroicons/react/20/solid";

import { SECONDARY_BUTTON_CLASS, Tooltip } from "../components/ui";
import { useToast } from "../components/toast";
import { apiData, humanInTheLoopApi } from "../lib/api-client";
import { useRunState } from "../lib/queries";

const ICON_BUTTON_CLASS =
  "inline-flex size-9 items-center justify-center rounded-lg text-fg-2 outline-1 -outline-offset-1 outline-white/10 transition-colors hover:bg-overlay hover:text-fg focus-visible:outline-2 focus-visible:-outline-offset-1 focus-visible:outline-teal-500";

export const handle = { wide: true, fullHeight: true };
export const TERMINAL_DOCK_CLEARANCE_CLASS =
  "pb-[calc(0.25rem+var(--fabro-interview-dock-clearance,0px))]";

type ConnectionStatus = "connecting" | "ready" | "closed" | "error";

interface TerminalServerMessage {
  type: "ready" | "error" | "closed";
  message?: string;
}

const TERMINAL_BACKGROUND = "#05080F";

const TERMINAL_THEME = {
  background:          TERMINAL_BACKGROUND,
  foreground:          "#E6EDF3",
  cursor:              "#7AC4E5",
  cursorAccent:        "#05080F",
  selectionBackground: "#1F4F73",

  black:   "#05080F",
  red:     "#FF6B6B",
  green:   "#5EE6A8",
  yellow:  "#FFC857",
  blue:    "#82AAFF",
  magenta: "#C792EA",
  cyan:    "#7AC4E5",
  white:   "#D5DCE3",

  brightBlack:   "#4B5563",
  brightRed:     "#FF8B8B",
  brightGreen:   "#85F5C2",
  brightYellow:  "#FFD98A",
  brightBlue:    "#A4C4FF",
  brightMagenta: "#E0B6FF",
  brightCyan:    "#A8DFF5",
  brightWhite:   "#FFFFFF",
};

export function buildTerminalWebSocketUrl(location: Location, runId: string): string {
  const protocol = location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${location.host}/api/v1/runs/${encodeURIComponent(runId)}/terminal`;
}

export function parseTerminalServerMessage(data: string): TerminalServerMessage | null {
  try {
    const parsed = JSON.parse(data);
    if (!parsed || typeof parsed !== "object") return null;
    const type = (parsed as { type?: unknown }).type;
    if (type !== "ready" && type !== "error" && type !== "closed") return null;
    const message = (parsed as { message?: unknown }).message;
    return {
      type,
      message: typeof message === "string" ? message : undefined,
    };
  } catch {
    return null;
  }
}

function getObject(value: unknown, key: string): Record<string, unknown> | null {
  if (!value || typeof value !== "object") return null;
  const child = (value as Record<string, unknown>)[key];
  return child && typeof child === "object" ? child as Record<string, unknown> : null;
}

function getString(value: Record<string, unknown> | null, key: string): string | null {
  const child = value?.[key];
  return typeof child === "string" ? child : null;
}

function sendResize(socket: WebSocket | null, fitAddon: XtermFitAddon | null) {
  if (!socket || socket.readyState !== WebSocket.OPEN || !fitAddon) return;
  const proposed = fitAddon.proposeDimensions();
  if (!proposed || proposed.cols <= 0 || proposed.rows <= 0) return;
  socket.send(JSON.stringify({
    type: "resize",
    cols: proposed.cols,
    rows: proposed.rows,
  }));
}

function statusDotClasses(status: ConnectionStatus): string {
  switch (status) {
    case "ready":
      return "bg-teal-500";
    case "error":
      return "bg-coral";
    case "closed":
      return "bg-fg-muted";
    case "connecting":
      return "bg-amber animate-pulse";
  }
}

function statusLabel(status: ConnectionStatus): string {
  switch (status) {
    case "ready":
      return "Connected";
    case "error":
      return "Error";
    case "closed":
      return "Closed";
    case "connecting":
      return "Connecting";
  }
}

function StatusPill({
  status,
  provider,
}: {
  status: ConnectionStatus;
  provider: string | null;
}) {
  return (
    <span
      role="status"
      aria-live="polite"
      className="inline-flex items-center gap-2 rounded-full bg-overlay py-1 pr-3 pl-2 text-xs font-medium text-fg-2 outline-1 -outline-offset-1 outline-white/10"
    >
      <span
        className={`size-1.5 rounded-full ${statusDotClasses(status)}`}
        aria-hidden="true"
      />
      <span>{statusLabel(status)}</span>
      {provider ? (
        <>
          <span className="text-fg-muted" aria-hidden="true">·</span>
          <span className="text-fg-3">{provider}</span>
        </>
      ) : null}
    </span>
  );
}

export default function RunTerminal({ params }: { params: { id: string } }) {
  const { push } = useToast();
  const stateQuery = useRunState(params.id);
  const sandbox = getObject(getObject(stateQuery.data, "run"), "sandbox")
    ?? getObject(stateQuery.data, "sandbox");
  const provider = getString(sandbox, "provider");
  const canCopySsh = provider === "daytona";
  const [connectionKey, setConnectionKey] = useState(0);
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [error, setError] = useState<string | null>(null);
  const terminalEl = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<XtermTerminal | null>(null);
  const fitRef = useRef<XtermFitAddon | null>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const terminalId = useMemo(
    () => `run-terminal-${params.id}`,
    [params.id],
  );

  const reconnect = useCallback(() => {
    setConnectionKey((key) => key + 1);
  }, []);

  const copySshCommand = useCallback(async () => {
    if (!canCopySsh) return;
    try {
      const response = await apiData(() =>
        humanInTheLoopApi.createRunSshAccess(params.id, { ttl_minutes: 60 }),
      );
      await navigator.clipboard.writeText(response.command);
      push({ message: "SSH command copied." });
    } catch (err) {
      push({
        tone: "error",
        message: err instanceof Error ? err.message : "Could not copy SSH command.",
      });
    }
  }, [canCopySsh, params.id, push]);

  useEffect(() => {
    if (!terminalEl.current) return undefined;

    let disposed = false;
    let resizeObserver: ResizeObserver | null = null;
    const textEncoder = new TextEncoder();
    const disposables: Array<{ dispose: () => void }> = [];

    async function connect() {
      setStatus("connecting");
      setError(null);

      const [{ Terminal }, { FitAddon }] = await Promise.all([
        import("@xterm/xterm"),
        import("@xterm/addon-fit"),
      ]);
      if (disposed || !terminalEl.current) return;

      const terminal = new Terminal({
        cursorBlink: true,
        convertEol: true,
        fontFamily: "\"JetBrains Mono\", ui-monospace, monospace",
        fontSize: 13,
        lineHeight: 1.45,
        scrollback: 5000,
        theme: TERMINAL_THEME,
      });
      const fitAddon = new FitAddon();
      terminal.loadAddon(fitAddon);
      terminal.open(terminalEl.current);
      fitAddon.fit();
      terminal.focus();
      terminalRef.current = terminal;
      fitRef.current = fitAddon;

      const socket = new WebSocket(buildTerminalWebSocketUrl(window.location, params.id));
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;

      disposables.push(terminal.onData((data) => {
        if (socket.readyState === WebSocket.OPEN) {
          socket.send(textEncoder.encode(data));
        }
      }));

      socket.addEventListener("open", () => {
        sendResize(socket, fitAddon);
      });
      socket.addEventListener("message", (event) => {
        if (typeof event.data === "string") {
          const message = parseTerminalServerMessage(event.data);
          if (!message) return;
          if (message.type === "ready") {
            setStatus("ready");
            return;
          }
          if (message.type === "closed") {
            setStatus("closed");
            return;
          }
          setStatus("error");
          setError(message.message ?? "Terminal session failed.");
          return;
        }
        const bytes = event.data instanceof ArrayBuffer
          ? new Uint8Array(event.data)
          : event.data;
        terminal.write(bytes);
      });
      socket.addEventListener("close", () => {
        setStatus((current) => current === "error" ? current : "closed");
      });
      socket.addEventListener("error", () => {
        setStatus("error");
        setError("Terminal WebSocket connection failed.");
      });

      resizeObserver = new ResizeObserver(() => {
        fitAddon.fit();
        sendResize(socket, fitAddon);
      });
      resizeObserver.observe(terminalEl.current);
    }

    void connect();

    return () => {
      disposed = true;
      resizeObserver?.disconnect();
      for (const disposable of disposables) disposable.dispose();
      socketRef.current?.send(JSON.stringify({ type: "close" }));
      socketRef.current?.close();
      socketRef.current = null;
      terminalRef.current?.dispose();
      terminalRef.current = null;
      fitRef.current = null;
    };
  }, [connectionKey, params.id]);

  return (
    <main
      className={`flex h-full min-h-0 flex-col ${TERMINAL_DOCK_CLEARANCE_CLASS}`}
      aria-labelledby={terminalId}
    >
      <h2 id={terminalId} className="sr-only">Terminal</h2>
      <div className="mb-2 flex shrink-0 flex-wrap items-center justify-between gap-3">
        <StatusPill status={status} provider={provider} />
        <div className="flex items-center gap-2">
          <Tooltip label="Reconnect">
            <button
              type="button"
              className={ICON_BUTTON_CLASS}
              onClick={reconnect}
              aria-label="Reconnect terminal"
            >
              <ArrowPathIcon className="size-4" aria-hidden="true" />
            </button>
          </Tooltip>
          {canCopySsh && (
            <button
              type="button"
              className={SECONDARY_BUTTON_CLASS}
              onClick={() => void copySshCommand()}
              aria-label="Copy SSH command"
            >
              <ClipboardDocumentIcon className="size-4" aria-hidden="true" />
              SSH
            </button>
          )}
        </div>
      </div>
      {error ? (
        <p className="mb-2 shrink-0 text-xs text-coral" role="alert">
          {error}
        </p>
      ) : null}
      <div
        className="min-h-0 flex-1 overflow-hidden rounded border border-line"
        style={{ backgroundColor: TERMINAL_BACKGROUND }}
      >
        <div ref={terminalEl} className="h-full min-h-0 p-3" />
      </div>
    </main>
  );
}
