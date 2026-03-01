export type VerificationStatus = "pass" | "fail" | "na";

export type VerificationType = "ai" | "automated" | "analysis" | "ai-analysis";

export interface Criterion {
  name: string;
  description: string;
  type: VerificationType | null;
  status: VerificationStatus;
}

export interface VerificationCategory {
  name: string;
  question: string;
  status: VerificationStatus;
  criteria: Criterion[];
}

export const statusConfig = {
  pass: {
    label: "Pass",
    color: "text-mint",
    bg: "bg-mint/15",
    dot: "bg-mint",
    border: "border-l-mint/50",
  },
  fail: {
    label: "Fail",
    color: "text-coral",
    bg: "bg-coral/15",
    dot: "bg-coral",
    border: "border-l-coral/50",
  },
  na: {
    label: "N/A",
    color: "text-navy-600",
    bg: "bg-white/[0.04]",
    dot: "bg-navy-600",
    border: "border-l-navy-600/50",
  },
} as const satisfies Record<
  VerificationStatus,
  { label: string; color: string; bg: string; dot: string; border: string }
>;

export const typeConfig = {
  ai: { label: "AI", color: "text-teal-300", bg: "bg-teal-500/10" },
  automated: { label: "Automated", color: "text-mint", bg: "bg-mint/10" },
  analysis: { label: "Analysis", color: "text-amber", bg: "bg-amber/10" },
  "ai-analysis": { label: "AI + Analysis", color: "text-teal-300", bg: "bg-teal-500/10" },
} as const satisfies Record<
  VerificationType,
  { label: string; color: string; bg: string }
>;

export const verificationCategories: VerificationCategory[] = [
  {
    name: "Traceability",
    question: "Do we understand what this change is and why we're making it?",
    status: "pass",
    criteria: [
      { name: "Motivation", description: "Origin of proposal identified", type: "ai", status: "pass" },
      { name: "Specifications", description: "Requirements written down", type: "ai", status: "pass" },
      { name: "Documentation", description: "Developer and user docs added", type: "ai", status: "pass" },
      { name: "Minimization", description: "No extraneous changes", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Readability",
    question: "Can a human or agent quickly read this and understand what it does?",
    status: "pass",
    criteria: [
      { name: "Formatting", description: "Code layout matches standard", type: "automated", status: "pass" },
      { name: "Linting", description: "Linter issues resolved", type: "automated", status: "pass" },
      { name: "Style", description: "House style applied", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Reliability",
    question: "Will this behave correctly and safely under real-world conditions and failures?",
    status: "pass",
    criteria: [
      { name: "Completeness", description: "Implementation covers requirements", type: "ai", status: "pass" },
      { name: "Defects", description: "Potential or likely bugs remediated", type: "ai-analysis", status: "pass" },
      { name: "Performance", description: "Hot path impact identified", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Code Coverage",
    question: "Do we have trustworthy, automated evidence that it works and won't regress?",
    status: "fail",
    criteria: [
      { name: "Test Coverage", description: "Production code exercised by unit tests", type: "analysis", status: "pass" },
      { name: "Test Quality", description: "Tests are robust and clear", type: "ai", status: "fail" },
      { name: "E2E Coverage", description: "Browser automation exercises UX", type: "analysis", status: "na" },
    ],
  },
  {
    name: "Maintainability",
    question: "Will this be easy to modify or extend later without creating new risk?",
    status: "pass",
    criteria: [
      { name: "Architecture", description: "Layering and dependency graph meets design", type: "analysis", status: "pass" },
      { name: "Interfaces", description: "", type: null, status: "pass" },
      { name: "Duplication", description: "Similar and identical code blocks identified", type: "analysis", status: "pass" },
      { name: "Simplicity", description: "Extra review for reducing complexity", type: "ai", status: "pass" },
      { name: "Dead Code", description: "Unexecuted code and dependencies removed", type: "analysis", status: "pass" },
    ],
  },
  {
    name: "Security",
    question: "Does this preserve or improve our security posture and avoid vulnerabilities?",
    status: "pass",
    criteria: [
      { name: "Vulnerabilities", description: "Security issues are remediated", type: "ai-analysis", status: "pass" },
      { name: "IaC Scanning", description: "", type: null, status: "pass" },
      { name: "Dependency Alerts", description: "Known CVEs are patched", type: "analysis", status: "pass" },
      { name: "Security Controls", description: "Organization standards applied", type: "ai", status: "pass" },
    ],
  },
  {
    name: "Deployability",
    question: "Is this changeset safe to ship to production immediately?",
    status: "fail",
    criteria: [
      { name: "Compatibility", description: "Breaking changes are avoided", type: "analysis", status: "pass" },
      { name: "Rollout / Rollback", description: "Known rollback plan if deploy fails", type: "ai", status: "fail" },
      { name: "Observability", description: "Logging, metrics, tracing instrumented", type: "ai", status: "fail" },
      { name: "Cost", description: "Tech ops costs estimated", type: "analysis", status: "pass" },
    ],
  },
  {
    name: "Compliance",
    question: "Does this meet our regulatory, contractual, and policy obligations?",
    status: "pass",
    criteria: [
      { name: "Change Control", description: "Separation of Duties policy met", type: "analysis", status: "pass" },
      { name: "AI Governance", description: "AI involvement was acceptable", type: "analysis", status: "pass" },
      { name: "Privacy", description: "PII is identified and handled to standards", type: "ai", status: "pass" },
      { name: "Accessibility", description: "Software meets accessibility requirements", type: "analysis", status: "pass" },
      { name: "Licensing", description: "Supply chain meets IP policy", type: "analysis", status: "pass" },
    ],
  },
];

export type EvaluationResult = "pass" | "fail" | "skip";

export type VerificationMode = "active" | "evaluate" | "disabled";

export interface CriterionPerformance {
  f1: number | null;
  passAt1: number | null;
  mode: VerificationMode;
  evaluations: EvaluationResult[];
}

export const modeConfig = {
  active: { label: "Active", color: "text-mint", bg: "bg-mint/10" },
  evaluate: { label: "Evaluate", color: "text-amber", bg: "bg-amber/10" },
  disabled: { label: "Disabled", color: "text-navy-600", bg: "bg-white/[0.04]" },
} as const satisfies Record<
  VerificationMode,
  { label: string; color: string; bg: string }
>;

export const criterionPerformance: Record<string, CriterionPerformance> = {
  "Motivation":          { f1: 0.87, passAt1: 0.82, mode: "active",   evaluations: ["pass","pass","fail","pass","pass","pass","pass","fail","pass","pass"] },
  "Specifications":      { f1: 0.83, passAt1: 0.78, mode: "active",   evaluations: ["pass","fail","pass","pass","pass","fail","pass","pass","pass","pass"] },
  "Documentation":       { f1: 0.79, passAt1: 0.74, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","fail","pass","pass","fail"] },
  "Minimization":        { f1: 0.72, passAt1: 0.68, mode: "evaluate", evaluations: ["pass","fail","pass","fail","pass","pass","fail","pass","pass","pass"] },
  "Formatting":          { f1: 0.99, passAt1: 0.98, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","pass","pass"] },
  "Linting":             { f1: 0.98, passAt1: 0.97, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","fail","pass"] },
  "Style":               { f1: 0.81, passAt1: 0.76, mode: "active",   evaluations: ["pass","fail","pass","pass","pass","pass","fail","pass","pass","pass"] },
  "Completeness":        { f1: 0.76, passAt1: 0.71, mode: "active",   evaluations: ["pass","pass","fail","pass","fail","pass","pass","pass","fail","pass"] },
  "Defects":             { f1: 0.84, passAt1: 0.79, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","pass","pass","pass","fail"] },
  "Performance":         { f1: 0.69, passAt1: 0.63, mode: "evaluate", evaluations: ["fail","pass","pass","fail","pass","fail","pass","pass","fail","pass"] },
  "Test Coverage":       { f1: 0.95, passAt1: 0.93, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","fail","pass","pass","pass"] },
  "Test Quality":        { f1: 0.71, passAt1: 0.65, mode: "evaluate", evaluations: ["pass","fail","fail","pass","pass","fail","pass","fail","pass","pass"] },
  "E2E Coverage":        { f1: 0.91, passAt1: 0.88, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","pass","pass","pass","pass"] },
  "Architecture":        { f1: 0.88, passAt1: 0.84, mode: "active",   evaluations: ["pass","pass","pass","pass","fail","pass","pass","pass","pass","pass"] },
  "Interfaces":          { f1: null, passAt1: null, mode: "disabled",  evaluations: [] },
  "Duplication":         { f1: 0.96, passAt1: 0.94, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","fail","pass","pass"] },
  "Simplicity":          { f1: 0.74, passAt1: 0.69, mode: "active",   evaluations: ["pass","fail","pass","pass","fail","pass","pass","fail","pass","pass"] },
  "Dead Code":           { f1: 0.93, passAt1: 0.90, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","fail","pass","pass","pass","pass"] },
  "Vulnerabilities":     { f1: 0.86, passAt1: 0.81, mode: "active",   evaluations: ["pass","pass","fail","pass","pass","pass","pass","pass","fail","pass"] },
  "IaC Scanning":        { f1: null, passAt1: null, mode: "disabled",  evaluations: [] },
  "Dependency Alerts":   { f1: 0.97, passAt1: 0.95, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","pass","fail"] },
  "Security Controls":   { f1: 0.80, passAt1: 0.75, mode: "active",   evaluations: ["pass","pass","fail","pass","pass","fail","pass","pass","pass","pass"] },
  "Compatibility":       { f1: 0.89, passAt1: 0.85, mode: "active",   evaluations: ["pass","pass","pass","pass","fail","pass","pass","pass","pass","pass"] },
  "Rollout / Rollback":  { f1: 0.66, passAt1: 0.60, mode: "evaluate", evaluations: ["fail","pass","fail","pass","fail","pass","pass","fail","pass","fail"] },
  "Observability":       { f1: 0.73, passAt1: 0.67, mode: "evaluate", evaluations: ["pass","fail","pass","fail","pass","pass","fail","pass","fail","pass"] },
  "Cost":                { f1: 0.78, passAt1: 0.72, mode: "evaluate", evaluations: ["pass","pass","fail","pass","fail","pass","pass","fail","pass","pass"] },
  "Change Control":      { f1: 0.94, passAt1: 0.91, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","fail","pass"] },
  "AI Governance":       { f1: 0.85, passAt1: 0.80, mode: "active",   evaluations: ["pass","pass","pass","fail","pass","pass","pass","pass","pass","pass"] },
  "Privacy":             { f1: 0.77, passAt1: 0.72, mode: "active",   evaluations: ["pass","fail","pass","pass","pass","fail","pass","pass","pass","fail"] },
  "Accessibility":       { f1: 0.90, passAt1: 0.87, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","fail","pass","pass","pass","pass"] },
  "Licensing":           { f1: 0.96, passAt1: 0.93, mode: "active",   evaluations: ["pass","pass","pass","pass","pass","pass","pass","pass","pass","pass"] },
};

export function getCategorySummary(categories: readonly VerificationCategory[]) {
  const passing = categories.filter((c) => c.status === "pass").length;
  return { passing, total: categories.length };
}

export function getCriteriaSummary(criteria: readonly Criterion[]) {
  return {
    passing: criteria.filter((c) => c.status === "pass").length,
    failing: criteria.filter((c) => c.status === "fail").length,
    na: criteria.filter((c) => c.status === "na").length,
    total: criteria.length,
  };
}

export function getAllCriteria(categories: readonly VerificationCategory[]) {
  return categories.flatMap((c) => c.criteria);
}
