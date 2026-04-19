export type FabroMode = "normal" | "install";

export function resolveFabroMode(value: unknown): FabroMode {
  return value === "install" ? "install" : "normal";
}

export function consumeInstallTokenFromUrl(url: string): {
  token: string | null;
  sanitizedUrl: string;
} {
  const parsed = new URL(url);
  const token = parsed.searchParams.get("token");
  if (!token) {
    return { token: null, sanitizedUrl: parsed.toString() };
  }

  parsed.searchParams.delete("token");
  return {
    token,
    sanitizedUrl: parsed.toString(),
  };
}

const INSTALL_GITHUB_ERROR_MESSAGES: Record<string, string> = {
  "missing-install-github-app-state":
    "GitHub App setup could not resume because the install state was missing. Continue again to create a fresh handoff.",
  "missing-install-github-app-code":
    "GitHub did not return the manifest conversion code Fabro needed. Continue again to retry the GitHub App handoff.",
  "expired-install-github-app-state":
    "GitHub App setup took too long and the temporary install state expired. Continue again to create a fresh handoff.",
  "invalid-install-github-app-state":
    "GitHub App setup returned with the wrong install state. Continue again to restart the secure handoff.",
  "github-app-manifest-conversion-failed":
    "GitHub App setup failed before Fabro could save the app credentials. Continue again to retry the callback.",
};

export function shouldConsumeInstallGithubErrorForPath(pathname: string): boolean {
  return pathname === "/install/github" || pathname.startsWith("/install/github/");
}

export function consumeInstallGithubErrorFromUrl(url: string): {
  error: string | null;
  sanitizedUrl: string;
} {
  const parsed = new URL(url);
  const errorCode = parsed.searchParams.get("error");
  if (!errorCode) {
    return { error: null, sanitizedUrl: parsed.toString() };
  }

  parsed.searchParams.delete("error");
  return {
    error: INSTALL_GITHUB_ERROR_MESSAGES[errorCode] ?? "GitHub App setup failed. Continue again to retry.",
    sanitizedUrl: parsed.toString(),
  };
}
