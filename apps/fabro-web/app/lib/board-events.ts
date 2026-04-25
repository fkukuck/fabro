import { useEffect } from "react";
import { useSWRConfig, type MutatorCallback } from "swr";

import { queryKeys } from "./query-keys";

type MutateFn = (key: string) => ReturnType<MutatorCallback>;

interface BoardEventSourceLike {
  onmessage: ((event: { data: string }) => void) | null;
  close(): void;
}

interface BoardSubscription {
  source: BoardEventSourceLike;
  refcount: number;
  mutators: Map<MutateFn, number>;
}

const BOARD_STATUS_EVENTS = new Set([
  "run.submitted",
  "run.queued",
  "run.starting",
  "run.running",
  "run.removing",
  "run.paused",
  "run.unpaused",
  "run.blocked",
  "run.unblocked",
  "run.completed",
  "run.failed",
  "run.archived",
  "run.unarchived",
  "interview.started",
  "interview.completed",
  "interview.timeout",
  "interview.interrupted",
]);

let subscription: BoardSubscription | null = null;

function createBrowserEventSource(url: string): BoardEventSourceLike {
  return new EventSource(url);
}

export function shouldRefreshBoardForEvent(event: string) {
  return BOARD_STATUS_EVENTS.has(event);
}

export function subscribeToBoardEvents(
  mutate: MutateFn,
  eventSourceFactory: (url: string) => BoardEventSourceLike = createBrowserEventSource,
): () => void {
  if (!subscription) {
    const source = eventSourceFactory("/api/v1/attach");
    subscription = {
      source,
      refcount: 0,
      mutators: new Map(),
    };

    source.onmessage = (message) => {
      try {
        const payload = JSON.parse(message.data) as { event?: string };
        if (!payload.event || !shouldRefreshBoardForEvent(payload.event)) return;

        for (const mutator of subscription?.mutators.keys() ?? []) {
          void mutator(queryKeys.boards.runs());
        }
      } catch {
        // Ignore malformed events.
      }
    };
  }

  subscription.refcount += 1;
  subscription.mutators.set(mutate, (subscription.mutators.get(mutate) ?? 0) + 1);

  return () => {
    if (!subscription) return;
    const mutateCount = subscription.mutators.get(mutate) ?? 0;
    if (mutateCount <= 1) {
      subscription.mutators.delete(mutate);
    } else {
      subscription.mutators.set(mutate, mutateCount - 1);
    }

    subscription.refcount -= 1;
    if (subscription.refcount <= 0) {
      subscription.source.close();
      subscription = null;
    }
  };
}

export function useBoardEvents() {
  const { mutate } = useSWRConfig();

  useEffect(() => subscribeToBoardEvents(mutate as MutateFn), [mutate]);
}
