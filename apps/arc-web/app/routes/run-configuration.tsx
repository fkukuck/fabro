import { Link, useParams } from "react-router";
import { CheckCircleIcon, ArrowPathIcon, PauseCircleIcon, XCircleIcon } from "@heroicons/react/24/solid";
import { DocumentTextIcon, MapIcon } from "@heroicons/react/24/outline";
import { findRun } from "../data/runs";
import { workflowData } from "./workflow-detail";
import { CollapsibleFile } from "../components/collapsible-file";

export const handle = { wide: true };

type StageStatus = "completed" | "running" | "pending" | "failed";

interface Stage {
  id: string;
  name: string;
  status: StageStatus;
  duration: string;
}

const stages: Stage[] = [
  { id: "detect-drift", name: "Detect Drift", status: "completed", duration: "1m 12s" },
  { id: "propose-changes", name: "Propose Changes", status: "completed", duration: "2m 34s" },
  { id: "review-changes", name: "Review Changes", status: "completed", duration: "0m 45s" },
  { id: "apply-changes", name: "Apply Changes", status: "running", duration: "1m 58s" },
];

const statusConfig: Record<StageStatus, { icon: typeof CheckCircleIcon; color: string }> = {
  completed: { icon: CheckCircleIcon, color: "text-mint" },
  running: { icon: ArrowPathIcon, color: "text-teal-500" },
  pending: { icon: PauseCircleIcon, color: "text-fg-muted" },
  failed: { icon: XCircleIcon, color: "text-coral" },
};

export default function RunConfiguration() {
  const { id } = useParams();
  const run = findRun(id ?? "");
  const workflow = run ? workflowData[run.workflow] : undefined;

  return (
    <div className="flex gap-6">
      <nav className="w-56 shrink-0 space-y-6">
        <div>
          <h3 className="px-2 text-xs font-medium uppercase tracking-wider text-fg-muted">Stages</h3>
          <ul className="mt-2 space-y-0.5">
            {stages.map((stage) => {
              const config = statusConfig[stage.status];
              const Icon = config.icon;
              return (
                <li key={stage.id}>
                  <Link
                    to={`/runs/${id}/stages/${stage.id}`}
                    className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-fg-3 transition-colors hover:bg-overlay hover:text-fg"
                  >
                    <Icon className={`size-4 shrink-0 ${config.color} ${stage.status === "running" ? "animate-spin" : ""}`} />
                    <span className="flex-1 truncate">{stage.name}</span>
                    <span className="font-mono text-xs tabular-nums text-fg-muted">{stage.duration}</span>
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>

        {workflow && (
          <div>
            <h3 className="px-2 text-xs font-medium uppercase tracking-wider text-fg-muted">Workflow</h3>
            <ul className="mt-2 space-y-0.5">
              <li>
                <Link
                  to={`/runs/${id}/configuration`}
                  className="flex items-center gap-2 rounded-md bg-overlay px-2 py-1.5 text-sm text-fg transition-colors"
                >
                  <DocumentTextIcon className="size-4 shrink-0 text-fg-muted" />
                  Run Configuration
                </Link>
              </li>
              <li>
                <Link
                  to={`/runs/${id}/graph`}
                  className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-fg-3 transition-colors hover:bg-overlay hover:text-fg"
                >
                  <MapIcon className="size-4 shrink-0 text-fg-muted" />
                  Workflow Graph
                </Link>
              </li>
            </ul>
          </div>
        )}
      </nav>

      <div className="min-w-0 flex-1">
        {workflow ? (
          <CollapsibleFile
            file={{ name: "task.toml", contents: workflow.config, lang: "toml" }}
          />
        ) : (
          <p className="text-sm text-fg-muted">No configuration found.</p>
        )}
      </div>
    </div>
  );
}
