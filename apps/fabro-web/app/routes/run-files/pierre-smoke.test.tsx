import { describe, expect, test } from "bun:test";
import { MultiFileDiff, PatchDiff, Virtualizer } from "@pierre/diffs/react";

// Regression coverage for the @pierre/diffs 1.0 -> 1.1 upgrade. We assert
// only that the public React components the Run Files route uses remain
// exported as callable function components — a full mount-under-test hits
// pierre's useLayoutEffect teardown path, which is incompatible with
// react-test-renderer under React 19. Functional mount coverage lives in
// the dev-server smoke check.

describe("@pierre/diffs public API", () => {
  test("MultiFileDiff is a callable component export", () => {
    expect(typeof MultiFileDiff).toBe("function");
  });

  test("PatchDiff is a callable component export", () => {
    expect(typeof PatchDiff).toBe("function");
  });

  test("Virtualizer is a callable component export", () => {
    expect(typeof Virtualizer).toBe("function");
  });
});
