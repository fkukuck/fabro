import { Link, useParams } from "react-router";
import { CheckCircleIcon, ArrowPathIcon, PauseCircleIcon, XCircleIcon } from "@heroicons/react/24/solid";
import { DocumentTextIcon, MapIcon } from "@heroicons/react/24/outline";
import { CollapsibleFile } from "../components/collapsible-file";
import { apiFetch, apiJson } from "../api-client";
import { formatDurationSecs } from "../lib/format";
import type { RunStage } from "@qltysh/arc-api-client";
import type { Route } from "./+types/run-configuration";

export const handle = { wide: true };

type StageStatus = "completed" | "running" | "pending" | "failed";

interface Stage {
  id: string;
  name: string;
  status: StageStatus;
  duration: string;
}

const statusConfig: Record<StageStatus, { icon: typeof CheckCircleIcon; color: string }> = {
  completed: { icon: CheckCircleIcon, color: "text-mint" },
  running: { icon: ArrowPathIcon, color: "text-teal-500" },
  pending: { icon: PauseCircleIcon, color: "text-fg-muted" },
  failed: { icon: XCircleIcon, color: "text-coral" },
};

export async function loader({ params }: Route.LoaderArgs) {
  const [apiStages, configRes] = await Promise.all([
    apiJson<RunStage[]>(`/runs/${params.id}/stages`),
    apiFetch(`/runs/${params.id}/configuration`),
  ]);
  const stages: Stage[] = apiStages.map((s) => ({
    id: s.id,
    name: s.name,
    status: s.status as StageStatus,
    duration: s.duration_secs != null ? formatDurationSecs(s.duration_secs) : "--",
  }));
  const configText = configRes.ok ? await configRes.text() : null;
  return { stages, configText };
}

export default function RunConfiguration({ loaderData }: Route.ComponentProps) {
  const { id } = useParams();
  const { stages, configText } = loaderData;

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
      </nav>

      <div className="min-w-0 flex-1">
        {configText ? (
          <CollapsibleFile
            file={{ name: "task.toml", contents: configText, lang: "toml" }}
          />
        ) : (
          <p className="text-sm text-fg-muted">No configuration found.</p>
        )}
      </div>
    </div>
  );
}
