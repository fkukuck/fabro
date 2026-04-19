import { useEffect } from "react";
import { ChevronRightIcon } from "@heroicons/react/20/solid";
import { Link, Outlet, useFetcher, useLocation } from "react-router";
import { mapRunSummaryToRunItem, runStatusDisplay, isRunStatus } from "../data/runs";
import type { RunSummaryResponse } from "../data/runs";
import { apiJson } from "../api";
import { ErrorState } from "../components/state";
import { useDemoMode } from "../lib/demo-mode";
import type { PreviewUrlResponse } from "@qltysh/fabro-api-client";

const allTabs = [
  { name: "Overview", path: "", count: null, demoOnly: false },
  { name: "Stages", path: "/stages", count: null, demoOnly: false },
  { name: "Graph", path: "/graph", count: null, demoOnly: false },
  { name: "Billing", path: "/billing", count: null, demoOnly: false },
];

export const handle = { hideHeader: true };

export async function loader({ request, params }: any) {
  const response = await fetch(`/api/v1/runs/${params.id}`, {
    credentials: "include",
  });
  if (!response.ok) return { run: null };
  const summary: RunSummaryResponse = await response.json();
  const item = mapRunSummaryToRunItem(summary);
  const rawStatus = summary.status;
  const display = isRunStatus(rawStatus)
    ? runStatusDisplay[rawStatus]
    : { label: rawStatus, dot: "bg-fg-muted", text: "text-fg-muted" };
  return {
    run: {
      ...item,
      statusLabel: display.label,
      statusDot: display.dot,
      statusText: display.text,
    },
  };
}

export async function action({ params, request }: any) {
  const formData = await request.formData();
  const port = formData.get("port");
  const expiresInSecs = formData.get("expires_in_secs");
  const result = await apiJson<PreviewUrlResponse>(`/runs/${params.id}/preview`, {
    request,
    init: {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ port: Number(port), expires_in_secs: Number(expiresInSecs) }),
    },
  });
  return result;
}

export function meta({ data }: any) {
  const run = data?.run;
  return [{ title: run ? `${run.title} — Fabro` : "Run — Fabro" }];
}

export default function RunDetail({ loaderData, params }: any) {
  const { run } = loaderData;
  const { pathname } = useLocation();
  const basePath = `/runs/${params.id}`;
  const previewFetcher = useFetcher<PreviewUrlResponse>();
  const demoMode = useDemoMode();
  const tabs = allTabs.filter((t) => !t.demoOnly || demoMode);

  useEffect(() => {
    if (previewFetcher.data?.url) {
      window.open(previewFetcher.data.url, "_blank");
    }
  }, [previewFetcher.data]);

  if (!run) {
    return (
      <div className="py-12">
        <ErrorState
          title="Run not found"
          description="The run you're looking for doesn't exist or was deleted."
        />
      </div>
    );
  }

  return (
    <div>
      <nav className="mb-4 flex items-center gap-1 text-sm text-fg-muted">
        <Link to="/runs" className="text-fg-3 hover:text-fg">Runs</Link>
        {demoMode && (
          <>
            <ChevronRightIcon className="size-3" />
            <Link to={`/workflows/${run.workflow}`} className="text-fg-3 hover:text-fg">
              {run.workflow}
            </Link>
          </>
        )}
        <ChevronRightIcon className="size-3" />
        <span>{run.title}</span>
      </nav>

      <div className="mb-6 flex items-center gap-4">
        <div className="min-w-0 flex-1">
          <h2 className="text-xl font-semibold text-fg">{run.title}</h2>
          <div className="mt-2 flex items-center gap-3 text-sm">
            <span className="flex items-center gap-1.5">
              <span className={`size-2 rounded-full ${run.statusDot}`} />
              <span className={`font-medium ${run.statusText}`}>{run.statusLabel}</span>
            </span>
            <span className="font-mono text-xs text-fg-muted">{run.repo}</span>
            {run.elapsed && (
              <span className="font-mono text-xs text-fg-muted">{run.elapsed}</span>
            )}
          </div>
        </div>
        {/* TODO: restore an Open PR button when RunPullRequest gains a url field */}
        {run.sandboxId && (
          <previewFetcher.Form method="post">
            <input type="hidden" name="port" value="3000" />
            <input type="hidden" name="expires_in_secs" value="3600" />
            <button
              type="submit"
              disabled={previewFetcher.state !== "idle"}
              className="inline-flex shrink-0 items-center justify-center gap-2 rounded-lg bg-teal-500 px-3.5 py-1.5 text-sm font-medium text-on-primary transition-colors hover:bg-teal-300 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-teal-500 disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:bg-teal-500"
            >
              <svg viewBox="0 0 20 20" fill="currentColor" className="size-3.5 shrink-0" aria-hidden="true">
                <path d="M10 12.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5Z" />
                <path fillRule="evenodd" d="M.664 10.59a1.651 1.651 0 0 1 0-1.186A10.004 10.004 0 0 1 10 3c4.257 0 7.893 2.66 9.336 6.41.147.381.146.804 0 1.186A10.004 10.004 0 0 1 10 17c-4.257 0-7.893-2.66-9.336-6.41ZM14 10a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z" clipRule="evenodd" />
              </svg>
              {previewFetcher.state !== "idle" ? "Opening…" : "Preview"}
            </button>
          </previewFetcher.Form>
        )}
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
