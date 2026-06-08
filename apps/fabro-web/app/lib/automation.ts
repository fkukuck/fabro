import type { Automation, AutomationTrigger } from "@qltysh/fabro-api-client";

type TriggerOfType<K extends AutomationTrigger["type"]> = Extract<
  AutomationTrigger,
  { type: K }
>;

export function findApiTrigger(automation: Automation): TriggerOfType<"api"> | undefined {
  return automation.triggers.find((t): t is TriggerOfType<"api"> => t.type === "api");
}

export function findScheduleTrigger(
  automation: Automation,
): TriggerOfType<"schedule"> | undefined {
  return automation.triggers.find((t): t is TriggerOfType<"schedule"> => t.type === "schedule");
}

export function findGithubIssueTrigger(
  automation: Automation,
): TriggerOfType<"github_issue"> | undefined {
  return automation.triggers.find((t): t is TriggerOfType<"github_issue"> => t.type === "github_issue");
}

export function hasEnabledApiTrigger(automation: Automation): boolean {
  return findApiTrigger(automation)?.enabled === true;
}
