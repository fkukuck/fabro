export type FinishHealthPollResult =
  | { kind: "error" }
  | { kind: "response"; ok: boolean; mode?: string };

export function shouldRedirectAfterHealthPoll(
  result: FinishHealthPollResult,
): boolean {
  return result.kind === "response" && result.ok && result.mode !== "install";
}
