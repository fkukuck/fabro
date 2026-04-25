import { describe, expect, test } from "bun:test";

import {
  shouldRefreshBoardForEvent,
  subscribeToBoardEvents,
} from "./board-events";
import { queryKeys } from "./query-keys";

type MessageHandler = ((event: { data: string }) => void) | null;

class FakeEventSource {
  onmessage: MessageHandler = null;
  closed = false;

  emit(payload: unknown) {
    this.onmessage?.({ data: JSON.stringify(payload) });
  }

  close() {
    this.closed = true;
  }
}

describe("shouldRefreshBoardForEvent", () => {
  test("refreshes board for run and interview status changes only", () => {
    expect(shouldRefreshBoardForEvent("run.running")).toBe(true);
    expect(shouldRefreshBoardForEvent("run.blocked")).toBe(true);
    expect(shouldRefreshBoardForEvent("interview.completed")).toBe(true);
    expect(shouldRefreshBoardForEvent("checkpoint.completed")).toBe(false);
  });
});

describe("subscribeToBoardEvents", () => {
  test("shares one source and invalidates the board runs key", () => {
    const source = new FakeEventSource();
    const created: string[] = [];
    const keys: string[] = [];
    const mutate = (key: string) => {
      keys.push(key);
      return Promise.resolve();
    };

    const firstCleanup = subscribeToBoardEvents(mutate, (url) => {
      created.push(url);
      return source;
    });
    const secondCleanup = subscribeToBoardEvents(mutate, () => {
      throw new Error("source should be reused");
    });

    source.emit({ event: "run.running" });

    expect(created).toEqual(["/api/v1/attach"]);
    expect(keys).toEqual([queryKeys.boards.runs()]);

    firstCleanup();
    expect(source.closed).toBe(false);
    secondCleanup();
    expect(source.closed).toBe(true);
  });
});
