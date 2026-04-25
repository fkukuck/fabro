import { useEffect } from "react";
import { useSWRConfig, type MutatorCallback } from "swr";

import { queryKeys } from "./query-keys";

type MutateFn = (key: string) => ReturnType<MutatorCallback>;

interface RunEventPayload {
  event?: string;
  node_id?: string;
  properties?: Record<string, unknown>;
}

interface RunEventSourceLike {
  onmessage: ((event: { data: string }) => void) | null;
  close(): void;
}

interface RunSubscription {
  source: RunEventSourceLike;
  refcount: number;
  mutators: Map<MutateFn, number>;
}

const subscriptions = new Map<string, RunSubscription>();

const TERMINAL_EVENTS = new Set(["run.completed", "run.failed"]);
const RUN_SUMMARY_EVENTS = new Set([
  "run.submitted",
  "run.queued",
  "run.starting",
  "run.running",
  "run.paused",
  "run.unpaused",
  "run.blocked",
  "run.unblocked",
  "run.archived",
  "run.unarchived",
]);
const STAGE_EVENTS = new Set(["stage.started", "stage.completed", "stage.failed"]);
const COMMAND_EVENTS = new Set(["command.started", "command.completed"]);

function createBrowserEventSource(url: string): RunEventSourceLike {
  return new EventSource(url);
}

export function queryKeysForRunEvent(
  runId: string,
  event: string,
  stageId?: string,
): string[] {
  if (event === "checkpoint.completed") {
    return [queryKeys.runs.files(runId)];
  }

  if (TERMINAL_EVENTS.has(event)) {
    return [
      queryKeys.runs.detail(runId),
      queryKeys.runs.files(runId),
      queryKeys.runs.billing(runId),
      queryKeys.runs.stages(runId),
      queryKeys.runs.graph(runId, "LR"),
      queryKeys.runs.graph(runId, "TB"),
    ];
  }

  if (RUN_SUMMARY_EVENTS.has(event)) {
    return [queryKeys.runs.detail(runId)];
  }

  if (STAGE_EVENTS.has(event)) {
    const keys = [
      queryKeys.runs.stages(runId),
      queryKeys.runs.events(runId, 1000),
      queryKeys.runs.graph(runId, "LR"),
      queryKeys.runs.graph(runId, "TB"),
      queryKeys.runs.detail(runId),
    ];
    if (stageId) {
      keys.push(queryKeys.runs.stageTurns(runId, stageId));
    }
    return keys;
  }

  if (COMMAND_EVENTS.has(event)) {
    const keys = [
      queryKeys.runs.stages(runId),
      queryKeys.runs.events(runId, 1000),
    ];
    if (stageId) {
      keys.push(queryKeys.runs.stageTurns(runId, stageId));
    }
    return keys;
  }

  return [];
}

export function subscribeToRunEvents(
  runId: string,
  mutate: MutateFn,
  eventSourceFactory: (url: string) => RunEventSourceLike = createBrowserEventSource,
): () => void {
  let subscription = subscriptions.get(runId);
  if (!subscription) {
    const source = eventSourceFactory(`/api/v1/runs/${runId}/attach`);
    subscription = {
      source,
      refcount: 0,
      mutators: new Map(),
    };
    subscriptions.set(runId, subscription);

    source.onmessage = (message) => {
      let payload: RunEventPayload;
      try {
        payload = JSON.parse(message.data) as RunEventPayload;
      } catch {
        return;
      }

      const event = payload.event;
      if (!event) return;

      const stageId = stageIdFromPayload(payload);
      const keys = queryKeysForRunEvent(runId, event, stageId);
      if (keys.length > 0) {
        const current = subscriptions.get(runId);
        for (const mutator of current?.mutators.keys() ?? []) {
          for (const key of keys) {
            void mutator(key);
          }
        }
      }

      if (TERMINAL_EVENTS.has(event)) {
        closeRunSubscription(runId);
      }
    };
  }

  subscription.refcount += 1;
  subscription.mutators.set(mutate, (subscription.mutators.get(mutate) ?? 0) + 1);

  return () => {
    const current = subscriptions.get(runId);
    if (!current) return;

    const mutateCount = current.mutators.get(mutate) ?? 0;
    if (mutateCount <= 1) {
      current.mutators.delete(mutate);
    } else {
      current.mutators.set(mutate, mutateCount - 1);
    }

    current.refcount -= 1;
    if (current.refcount <= 0) {
      closeRunSubscription(runId);
    }
  };
}

function stageIdFromPayload(payload: RunEventPayload): string | undefined {
  if (typeof payload.node_id === "string") return payload.node_id;
  const nodeId = payload.properties?.node_id;
  return typeof nodeId === "string" ? nodeId : undefined;
}

function closeRunSubscription(runId: string) {
  const subscription = subscriptions.get(runId);
  if (!subscription) return;
  subscription.source.close();
  subscriptions.delete(runId);
}

export function useRunEvents(runId: string | undefined) {
  const { mutate } = useSWRConfig();

  useEffect(() => {
    if (!runId) return;
    return subscribeToRunEvents(runId, mutate as MutateFn);
  }, [mutate, runId]);
}
