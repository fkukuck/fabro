import type { ServerSettings } from "@qltysh/fabro-api-client";
import { CollapsibleFile } from "../components/collapsible-file";
import { useServerSettings } from "../lib/queries";

export function meta({}: any) {
  return [{ title: "Settings — Fabro" }];
}

export default function Settings() {
  const settingsQuery = useServerSettings();
  const settings = (settingsQuery.data ?? {}) as ServerSettings;

  return (
    <>
      <p className="mb-6 max-w-[60ch] text-sm/6 text-fg-3 text-pretty">
        Snapshot of the server configuration. Edit via{" "}
        <code className="font-mono text-fg-2">settings.toml</code>; changes
        take effect on the next server restart.
      </p>
      <CollapsibleFile
        file={{ name: "server.json", contents: JSON.stringify(settings, null, 2), lang: "json" }}
      />
    </>
  );
}
