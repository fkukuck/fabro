import { describe, expect, test } from "bun:test";

import { INSTALL_PROVIDERS } from "./install-config";

describe("INSTALL_PROVIDERS", () => {
  test("excludes openai_compatible from install v1", () => {
    expect(INSTALL_PROVIDERS.map((provider) => provider.id)).toEqual([
      "anthropic",
      "openai",
      "gemini",
    ]);
  });
});
