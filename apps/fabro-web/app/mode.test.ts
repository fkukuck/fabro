import { describe, expect, test } from "bun:test";

import {
  consumeInstallGithubErrorFromUrl,
  consumeInstallTokenFromUrl,
  resolveFabroMode,
  shouldConsumeInstallGithubErrorForPath,
} from "./mode";

describe("resolveFabroMode", () => {
  test("returns install only for the explicit install marker", () => {
    expect(resolveFabroMode("install")).toBe("install");
    expect(resolveFabroMode("normal")).toBe("normal");
    expect(resolveFabroMode(undefined)).toBe("normal");
  });
});

describe("consumeInstallTokenFromUrl", () => {
  test("extracts the install token and preserves other query params", () => {
    expect(
      consumeInstallTokenFromUrl("https://fabro.example.com/install?token=abc123&step=welcome"),
    ).toEqual({
      token: "abc123",
      sanitizedUrl: "https://fabro.example.com/install?step=welcome",
    });
  });

  test("returns the original url when no install token is present", () => {
    expect(
      consumeInstallTokenFromUrl("https://fabro.example.com/install?step=welcome"),
    ).toEqual({
      token: null,
      sanitizedUrl: "https://fabro.example.com/install?step=welcome",
    });
  });
});

describe("consumeInstallGithubErrorFromUrl", () => {
  test("extracts the callback error and preserves other query params", () => {
    expect(
      consumeInstallGithubErrorFromUrl(
        "https://fabro.example.com/install/github?error=github-app-manifest-conversion-failed&step=github",
      ),
    ).toEqual({
      error: "GitHub App setup failed before Fabro could save the app credentials. Continue again to retry the callback.",
      sanitizedUrl: "https://fabro.example.com/install/github?step=github",
    });
  });

  test("returns the original url when no callback error is present", () => {
    expect(
      consumeInstallGithubErrorFromUrl("https://fabro.example.com/install/github?step=github"),
    ).toEqual({
      error: null,
      sanitizedUrl: "https://fabro.example.com/install/github?step=github",
    });
  });
});

describe("shouldConsumeInstallGithubErrorForPath", () => {
  test("only consumes GitHub callback errors on GitHub install routes", () => {
    expect(shouldConsumeInstallGithubErrorForPath("/install/github")).toBe(true);
    expect(shouldConsumeInstallGithubErrorForPath("/install/github/done")).toBe(true);
    expect(shouldConsumeInstallGithubErrorForPath("/install/llm")).toBe(false);
    expect(shouldConsumeInstallGithubErrorForPath("/install/server")).toBe(false);
  });
});
