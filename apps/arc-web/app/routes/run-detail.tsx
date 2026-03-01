import { ChevronRightIcon } from "@heroicons/react/20/solid";
import { Link, Outlet, useLocation } from "react-router";
import { findRun, statusColors } from "../data/runs";
import { workflowData } from "./workflow-detail";
import type { Route } from "./+types/run-detail";

const tabs = [
  { name: "Overview", path: "", count: null },
  { name: "Stages", path: "/stages/detect-drift", count: 4 },
  { name: "Files Changed", path: "/files", count: 3 },
  { name: "Verifications", path: "/verifications", count: null },
  { name: "Retro", path: "/retro", count: null },
  { name: "Usage", path: "/usage", count: null },
];

export const handle = { hideHeader: true };

export function meta({ params }: Route.MetaArgs) {
  const run = findRun(params.id);
  return [{ title: run ? `${run.title} — Arc` : "Run — Arc" }];
}

export default function RunDetail({ params }: Route.ComponentProps) {
  const run = findRun(params.id);
  const { pathname } = useLocation();
  const basePath = `/runs/${params.id}`;

  if (!run) {
    return <p className="py-8 text-center text-sm text-fg-muted">Run not found.</p>;
  }

  const colors = statusColors[run.status];

  return (
    <div>
      <nav className="mb-4 flex items-center gap-1 text-sm text-fg-muted">
        <Link to="/runs" className="text-fg-3 hover:text-fg">Runs</Link>
        <ChevronRightIcon className="size-3" />
        <Link to={`/workflows/${run.workflow}`} className="text-fg-3 hover:text-fg">
          {workflowData[run.workflow]?.title ?? run.workflow}
        </Link>
        <ChevronRightIcon className="size-3" />
        <span>{run.title}</span>
      </nav>

      <div className="mb-6 flex items-center gap-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-semibold text-fg">{run.title}</h2>
          <div className="mt-2 flex items-center gap-3 text-sm">
            <span className="flex items-center gap-1.5">
              <span className={`size-2 rounded-full ${colors.dot}`} />
              <span className={`font-medium ${colors.text}`}>{run.statusLabel}</span>
            </span>
            <span className="font-mono text-xs text-fg-muted">{run.repo}</span>
            {run.elapsed && (
              <span className={`font-mono text-xs ${run.elapsedWarning ? "text-amber" : "text-fg-muted"}`}>{run.elapsed}</span>
            )}
          </div>
        </div>
        <button
          type="button"
          title="Open pull request"
          className="flex shrink-0 items-center gap-1.5 rounded-md border border-mint/20 px-3 py-1.5 text-sm font-medium text-mint transition-colors hover:border-mint/50 hover:bg-mint/10 hover:text-fg"
        >
          <svg viewBox="0 0 16 16" fill="currentColor" className="size-3.5" aria-hidden="true">
            <path d="M1.5 3.25a2.25 2.25 0 1 1 3 2.122v5.256a2.251 2.251 0 1 1-1.5 0V5.372A2.25 2.25 0 0 1 1.5 3.25Zm5.677-.177L9.573.677A.25.25 0 0 1 10 .854V2.5h1A2.5 2.5 0 0 1 13.5 5v5.628a2.251 2.251 0 1 1-1.5 0V5a1 1 0 0 0-1-1h-1v1.646a.25.25 0 0 1-.427.177L7.177 3.427a.25.25 0 0 1 0-.354ZM3.75 2.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm0 9.5a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Zm8.25.75a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Z" />
          </svg>
          Open PR
        </button>
      </div>

      <div className="border-b border-line">
        <nav className="-mb-px flex gap-6">
          {tabs.map((tab) => {
            const tabPath = `${basePath}${tab.path}`;
            const isActive = tab.name === "Stages"
              ? pathname.startsWith(`${basePath}/stages`)
              : pathname === tabPath;
            return (
              <Link
                key={tab.name}
                to={tabPath}
                className={`border-b-2 pb-3 text-sm font-medium transition-colors ${
                  isActive
                    ? "border-teal-500 text-fg"
                    : "border-transparent text-fg-muted hover:border-line-strong hover:text-fg-3"
                }`}
              >
                {tab.name}
                {tab.count != null && (
                  <span className={`ml-1.5 rounded-full px-1.5 py-0.5 text-xs font-normal tabular-nums ${
                    isActive ? "bg-overlay-strong text-fg-3" : "bg-overlay text-fg-muted"
                  }`}>
                    {tab.count}
                  </span>
                )}
              </Link>
            );
          })}
        </nav>
      </div>

      <div className="mt-6">
        <Outlet />
      </div>
    </div>
  );
}
