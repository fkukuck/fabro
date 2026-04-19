import { useEffect, useMemo, useRef, useState } from "react";
import { useParams } from "react-router";
import { marked } from "marked";
import { CommandLineIcon, ChatBubbleLeftIcon, PlayIcon } from "@heroicons/react/24/outline";
import { ToolBlock } from "../components/tool-use";
import type { ToolUse } from "../components/tool-use";
import { StageSidebar, statusConfig } from "../components/stage-sidebar";
import type { Stage } from "../components/stage-sidebar";
import { EmptyState } from "../components/state";
import { apiJson, apiJsonOrNull } from "../api";
import { isVisibleStage } from "../data/runs";
import { formatDurationSecs } from "../lib/format";
import type { PaginatedRunStageList, StageTurn as ApiStageTurn, PaginatedStageTurnList, PaginatedEventList } from "@qltysh/fabro-api-client";

export const handle = { wide: true };

type TurnType =
  | { kind: "system"; content: string }
  | { kind: "assistant"; content: string }
  | { kind: "tool"; tools: ToolUse[] }
  | { kind: "command"; script: string; language: string; stdout?: string; stderr?: string; exitCode?: number | null; durationMs?: number; timedOut?: boolean; running: boolean };

interface RawEvent {
  node_id?: string;
  event: string;
  properties?: Record<string, unknown>;
  text?: string;
  tool_name?: string;
  tool_call_id?: string;
  arguments?: unknown;
  output?: unknown;
  is_error?: boolean;
}

function turnsFromEvents(events: RawEvent[], stageId: string): TurnType[] {
  const stageEvents = events.filter((e) => e.node_id === stageId);
  const turns: TurnType[] = [];
  // Collect tool pairs: started → completed
  const pendingTools = new Map<string, { toolName: string; input: string }>();
  // Track pending command for pairing started → completed
  let pendingCommand: { script: string; language: string } | undefined;

  for (const e of stageEvents) {
    const props = e.properties ?? {};
    switch (e.event) {
      case "stage.prompt":
        turns.push({ kind: "system", content: props.text as string ?? e.text ?? "" });
        break;
      case "agent.message": {
        const msg = props.text as string ?? e.text ?? "";
        if (msg) turns.push({ kind: "assistant", content: msg });
        break;
      }
      case "agent.tool.started": {
        const callId = props.tool_call_id as string ?? e.tool_call_id ?? "";
        pendingTools.set(callId, {
          toolName: props.tool_name as string ?? e.tool_name ?? "",
          input: typeof (props.arguments ?? e.arguments) === "string"
            ? (props.arguments ?? e.arguments) as string
            : JSON.stringify(props.arguments ?? e.arguments ?? ""),
        });
        break;
      }
      case "agent.tool.completed": {
        const callId = props.tool_call_id as string ?? e.tool_call_id ?? "";
        const started = pendingTools.get(callId);
        const output = props.output ?? e.output ?? "";
        const result = typeof output === "string" ? output : JSON.stringify(output);
        const tool: ToolUse = {
          id: callId,
          toolName: started?.toolName ?? props.tool_name as string ?? e.tool_name ?? "",
          input: started?.input ?? "",
          result,
          isError: (props.is_error ?? e.is_error) === true,
        };
        pendingTools.delete(callId);
        turns.push({ kind: "tool", tools: [tool] });
        break;
      }
      case "command.started": {
        pendingCommand = {
          script: props.script as string ?? "",
          language: props.language as string ?? "shell",
        };
        break;
      }
      case "command.completed": {
        turns.push({
          kind: "command",
          script: pendingCommand?.script ?? "",
          language: pendingCommand?.language ?? "shell",
          stdout: props.stdout as string ?? "",
          stderr: props.stderr as string ?? "",
          exitCode: props.exit_code as number | null ?? null,
          durationMs: props.duration_ms as number ?? 0,
          timedOut: props.timed_out as boolean ?? false,
          running: false,
        });
        pendingCommand = undefined;
        break;
      }
    }
  }

  // If command.started was seen but no command.completed, it's still running
  if (pendingCommand) {
    turns.push({
      kind: "command",
      script: pendingCommand.script,
      language: pendingCommand.language,
      running: true,
    });
  }

  return turns;
}

export async function loader({ request, params }: any) {
  const stagesResult = await apiJsonOrNull<PaginatedRunStageList>(`/runs/${params.id}/stages`, { request });
  const stages: Stage[] = (stagesResult?.data ?? []).filter((s) => isVisibleStage(s.id)).map((s) => ({
    id: s.id,
    name: s.name,
    status: s.status as Stage["status"],
    duration: s.duration_secs != null ? formatDurationSecs(s.duration_secs) : "--",
  }));

  const selectedStageId = params.stageId ?? stages[0]?.id;

  // Try demo turns endpoint first, fall back to events.
  let turns: TurnType[] = [];
  if (selectedStageId) {
    const turnsResult = await apiJsonOrNull<PaginatedStageTurnList>(
      `/runs/${params.id}/stages/${selectedStageId}/turns`,
      { request },
    );
    if (turnsResult?.data?.length) {
      turns = turnsResult.data.map((t: ApiStageTurn): TurnType => {
        if (t.kind === "tool" && t.tools) {
          return {
            kind: "tool",
            tools: t.tools.map((tu) => ({
              id: tu.id,
              toolName: tu.tool_name,
              input: tu.input,
              result: tu.result,
              isError: tu.is_error,
              durationMs: tu.duration_ms,
            })),
          };
        }
        return { kind: t.kind as "system" | "assistant", content: t.content ?? "" };
      });
    } else {
      // Fetch events and build turns from them.
      const eventsResult = await apiJsonOrNull<PaginatedEventList>(
        `/runs/${params.id}/events?limit=1000`,
        { request },
      );
      if (eventsResult?.data) {
        turns = turnsFromEvents(eventsResult.data as unknown as RawEvent[], selectedStageId);
      }
    }
  }

  return { stages, turns };
}

function Markdown({ content }: { content: string }) {
  const html = useMemo(() => marked.parse(content, { async: false }) as string, [content]);
  return (
    <div
      className="prose prose-sm max-w-none text-fg-3 prose-headings:text-fg-2 prose-strong:text-fg-2 prose-code:rounded prose-code:bg-overlay-strong prose-code:px-1 prose-code:py-0.5 prose-code:text-[0.8em] prose-code:font-mono prose-code:text-fg-3 prose-code:before:content-none prose-code:after:content-none prose-pre:bg-overlay-strong prose-pre:text-fg-3 prose-a:text-teal-500"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

function SystemBlock({ content }: { content: string }) {
  return (
    <section className="border-l-2 border-amber/50 pl-4">
      <header className="mb-1.5 flex items-center gap-2">
        <CommandLineIcon className="size-4 shrink-0 text-amber" />
        <span className="text-xs font-medium text-fg-3">System prompt</span>
      </header>
      <Markdown content={content} />
    </section>
  );
}

function AssistantBlock({ content }: { content: string }) {
  return (
    <section className="border-l-2 border-teal-500/50 pl-4">
      <header className="mb-1.5 flex items-center gap-2">
        <ChatBubbleLeftIcon className="size-4 shrink-0 text-teal-500" />
        <span className="text-xs font-medium text-fg-3">Assistant</span>
      </header>
      <Markdown content={content} />
    </section>
  );
}

function StatusPill({
  tone,
  children,
}: {
  tone: "running" | "failed" | "success" | "neutral";
  children: React.ReactNode;
}) {
  const toneClass = {
    running: "bg-teal-500/15 text-teal-500",
    failed: "bg-coral/15 text-coral",
    success: "bg-mint/15 text-mint",
    neutral: "bg-overlay text-fg-3",
  }[tone];
  return (
    <span className={`rounded px-1.5 py-0.5 text-[11px] font-medium ${toneClass}`}>
      {children}
    </span>
  );
}

function StreamLabel({ label }: { label: string }) {
  return (
    <div className="mb-1 font-mono text-[11px] uppercase tracking-wider text-fg-muted">
      {label}
    </div>
  );
}

function CommandBlock({ turn }: { turn: Extract<TurnType, { kind: "command" }> }) {
  const failed = !turn.running && turn.exitCode !== 0;
  const borderColor = turn.running ? "border-teal-500/20" : failed ? "border-coral/15" : "border-mint/15";
  const bgColor = turn.running ? "bg-teal-500/5" : failed ? "bg-coral/5" : "bg-mint/5";

  return (
    <div className={`rounded-md border ${borderColor} ${bgColor} overflow-hidden`}>
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2">
        <PlayIcon className={`size-4 shrink-0 ${turn.running ? "text-teal-500 animate-pulse" : failed ? "text-coral" : "text-mint"}`} />
        <span className="text-xs font-medium text-fg-3">
          {turn.language === "python" ? "Python" : "Shell"}
        </span>
        <div className="ml-auto flex items-center gap-2">
          {turn.running ? (
            <StatusPill tone="running">Running…</StatusPill>
          ) : turn.timedOut ? (
            <StatusPill tone="failed">Timed out</StatusPill>
          ) : (
            <>
              <StatusPill tone={failed ? "failed" : "success"}>
                exit {turn.exitCode ?? "?"}
              </StatusPill>
              {turn.durationMs != null && (
                <StatusPill tone="neutral">
                  {turn.durationMs < 1000
                    ? `${turn.durationMs}ms`
                    : `${(turn.durationMs / 1000).toFixed(1)}s`}
                </StatusPill>
              )}
            </>
          )}
        </div>
      </div>

      {/* Script */}
      {turn.script && (
        <div className="border-t border-line px-3 py-2.5">
          <pre className="whitespace-pre-wrap font-mono text-sm leading-relaxed text-fg-3 sm:text-xs">{turn.script}</pre>
        </div>
      )}

      {/* stdout */}
      {turn.stdout && (
        <div className="border-t border-line px-3 py-2.5">
          <StreamLabel label="stdout" />
          <pre className="whitespace-pre-wrap font-mono text-sm leading-relaxed text-fg-3 sm:text-xs">{turn.stdout}</pre>
        </div>
      )}

      {/* stderr */}
      {turn.stderr && (
        <div className="border-t border-line px-3 py-2.5">
          <StreamLabel label="stderr" />
          <pre className="whitespace-pre-wrap font-mono text-sm leading-relaxed text-coral sm:text-xs">{turn.stderr}</pre>
        </div>
      )}
    </div>
  );
}

export default function RunStages({ loaderData }: any) {
  const { id, stageId } = useParams();
  const { stages, turns } = loaderData;

  const selectedStage = stages.find((s: Stage) => s.id === stageId) ?? stages[0];
  const isRunning = selectedStage?.status === "running";

  // Ticking timer for running stage header
  const runningStartRef = useRef(isRunning ? Date.now() : 0);
  const [, setTick] = useState(0);

  useEffect(() => {
    if (isRunning && runningStartRef.current === 0) {
      runningStartRef.current = Date.now();
    } else if (!isRunning) {
      runningStartRef.current = 0;
    }
  }, [isRunning]);

  useEffect(() => {
    if (!isRunning) return;
    const interval = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(interval);
  }, [isRunning]);

  if (!stages.length) {
    return (
      <div className="py-12">
        <EmptyState
          title="No stages yet"
          description="Stages will appear here once the run begins executing."
        />
      </div>
    );
  }

  const selectedConfig = statusConfig[selectedStage.status];
  const SelectedIcon = selectedConfig.icon;
  const headerDuration = isRunning && runningStartRef.current
    ? formatDurationSecs(Math.floor((Date.now() - runningStartRef.current) / 1000))
    : selectedStage.duration;

  return (
    <div className="flex gap-6">
      <StageSidebar stages={stages} runId={id!} selectedStageId={selectedStage.id} />

      <div className="min-w-0 flex-1 space-y-3">
        <div className="flex items-center gap-2">
          <SelectedIcon className={`size-5 ${selectedConfig.color} ${isRunning ? "animate-spin" : ""}`} />
          <h3 className="text-base font-semibold text-fg">{selectedStage.name}</h3>
          <span className="font-mono text-xs tabular-nums text-fg-muted">{headerDuration}</span>
        </div>

        {turns.map((turn: TurnType, i: number) => {
          switch (turn.kind) {
            case "system":
              return <SystemBlock key={`turn-${i}`} content={turn.content} />;
            case "assistant":
              return <AssistantBlock key={`turn-${i}`} content={turn.content} />;
            case "tool":
              return <ToolBlock key={`turn-${i}`} tools={turn.tools} />;
            case "command":
              return <CommandBlock key={`turn-${i}`} turn={turn} />;
          }
        })}
      </div>
    </div>
  );
}
