import { describe, expect, test } from "bun:test";

import { isSafeMarkdownHref } from "./run-stages";

describe("isSafeMarkdownHref", () => {
  test("rejects protocol-relative URLs", () => {
    expect(isSafeMarkdownHref("//attacker.example/pixel.png")).toBe(false);
  });

  test("accepts root-relative, hash, http, https, and mailto URLs", () => {
    expect(isSafeMarkdownHref("/runs/run-1")).toBe(true);
    expect(isSafeMarkdownHref("#section-1")).toBe(true);
    expect(isSafeMarkdownHref("https://fabro.sh")).toBe(true);
    expect(isSafeMarkdownHref("http://localhost:3000")).toBe(true);
    expect(isSafeMarkdownHref("mailto:test@example.com")).toBe(true);
  });
});
