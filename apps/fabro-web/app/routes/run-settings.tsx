import { useParams } from "react-router";
import { CollapsibleFile } from "../components/collapsible-file";
import { StageSidebar } from "../components/stage-sidebar";
import type { Stage } from "../components/stage-sidebar";
import { isVisibleStage } from "../data/runs";
import { formatDurationSecs } from "../lib/format";
import { useRunSettings, useRunStages } from "../lib/queries";
import type { PaginatedRunStageList } from "@qltysh/fabro-api-client";

export const handle = { wide: true };
type WorkflowSettingsSnapshot = Record<string, unknown>;

function mapStages(stagesResult: PaginatedRunStageList | null | undefined): Stage[] {
  return (stagesResult?.data ?? []).filter((s) => isVisibleStage(s.id)).map((s) => ({
    id: s.id,
    name: s.name,
    status: s.status as Stage["status"],
    duration: s.duration_secs != null ? formatDurationSecs(s.duration_secs) : "--",
  }));
}

export default function RunSettingsPage() {
  const { id } = useParams();
  const stagesQuery = useRunStages(id);
  const settingsQuery = useRunSettings<WorkflowSettingsSnapshot>(id);
  const stages = mapStages(stagesQuery.data);
  const settings = settingsQuery.data ?? {};

  return (
    <div className="flex gap-6">
      <StageSidebar stages={stages} runId={id!} activeLink="settings" />

      <div className="min-w-0 flex-1">
        <header className="mb-4">
          <h2 className="text-base font-semibold text-fg">Run settings</h2>
          <p className="mt-1 text-sm/6 text-fg-3">
            Frozen settings snapshot used by this run.
          </p>
        </header>
        <CollapsibleFile
          file={{ name: "settings.json", contents: JSON.stringify(settings, null, 2), lang: "json" }}
        />
      </div>
    </div>
  );
}
