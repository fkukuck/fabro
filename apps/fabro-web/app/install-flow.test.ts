import { describe, expect, test } from "bun:test";

import { shouldRedirectAfterHealthPoll } from "./install-flow";

describe("shouldRedirectAfterHealthPoll", () => {
  test("waits when the health request fails", () => {
    expect(shouldRedirectAfterHealthPoll({ kind: "error" })).toBe(false);
  });

  test("waits when the server returns a non-success status", () => {
    expect(
      shouldRedirectAfterHealthPoll({
        kind: "response",
        ok: false,
      }),
    ).toBe(false);
  });

  test("waits while the server is still in install mode", () => {
    expect(
      shouldRedirectAfterHealthPoll({
        kind: "response",
        ok: true,
        mode: "install",
      }),
    ).toBe(false);
  });

  test("redirects only after the server returns success outside install mode", () => {
    expect(
      shouldRedirectAfterHealthPoll({
        kind: "response",
        ok: true,
        mode: "normal",
      }),
    ).toBe(true);
  });
});
