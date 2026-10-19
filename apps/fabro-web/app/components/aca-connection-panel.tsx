import { useState, type FormEvent, type ReactNode } from "react";
import { useSWRConfig } from "swr";
import type {
  AcaConnectionInput,
  AcaConnectionSummary,
  AcaConnectionTestResult,
} from "@qltysh/fabro-api-client";

import { ApiError, apiData, sandboxesApi } from "../lib/api-client";
import { useAcaConnection } from "../lib/queries";
import { queryKeys } from "../lib/query-keys";
import { Dot, Label, Panel, PanelSkeleton, Row } from "./settings-panel";
import {
  ErrorMessage,
  INPUT_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from "./ui";
import { useToast } from "./toast";

const DOCS_URL = "https://docs.fabro.sh/integrations/azure-container-apps";

type FormState = {
  tenant_id: string;
  client_id: string;
  client_secret: string;
  subscription_id: string;
  resource_group: string;
  sandbox_group: string;
  location: string;
  allowed_images: string;
  default_image: string;
  retention_hours: string;
};

const REQUIRED_FIELDS = [
  "tenant_id",
  "client_id",
  "subscription_id",
  "resource_group",
  "sandbox_group",
] as const;

export function formFromSummary(summary: AcaConnectionSummary | undefined): FormState {
  return {
    tenant_id: summary?.tenant_id ?? "",
    client_id: summary?.client_id ?? "",
    client_secret: "",
    subscription_id: summary?.subscription_id ?? "",
    resource_group: summary?.resource_group ?? "",
    sandbox_group: summary?.sandbox_group ?? "",
    location: summary?.location ?? "",
    allowed_images: (summary?.allowed_images ?? []).join(", "),
    default_image: summary?.default_image ?? "",
    retention_hours: String(summary?.retention_hours ?? 24),
  };
}

// The request for a form: an empty client secret is left out so the saved
// one is kept.
export function inputFromForm(form: FormState): AcaConnectionInput {
  const text = (value: string) => value.trim();
  const optional = (value: string) => (value.trim() === "" ? undefined : value.trim());
  const hours = Number.parseInt(form.retention_hours, 10);
  return {
    tenant_id: text(form.tenant_id),
    client_id: text(form.client_id),
    client_secret: form.client_secret === "" ? undefined : form.client_secret,
    subscription_id: text(form.subscription_id),
    resource_group: text(form.resource_group),
    sandbox_group: text(form.sandbox_group),
    location: optional(form.location),
    allowed_images: form.allowed_images
      .split(/[,\s]+/)
      .map((image) => image.trim())
      .filter((image) => image !== ""),
    default_image: optional(form.default_image),
    retention_hours: Number.isFinite(hours) ? hours : undefined,
  };
}

export function missingFields(form: FormState, secretSaved: boolean): string[] {
  const missing: string[] = REQUIRED_FIELDS.filter((field) => form[field].trim() === "");
  if (!secretSaved && form.client_secret === "") missing.push("client_secret");
  return missing;
}

function errorMessage(cause: unknown, fallback: string): string {
  return cause instanceof ApiError && cause.message ? cause.message : fallback;
}

export function AcaConnectionPanel() {
  const query = useAcaConnection();
  if (!query.data) {
    return query.error ? (
      <ErrorMessage message="Couldn't load the Azure connection." />
    ) : (
      <PanelSkeleton />
    );
  }
  // Keyed on the save time, so a saved or removed connection resets the form.
  return (
    <ConnectionSection
      key={query.data.updated_at ?? "none"}
      summary={query.data}
    />
  );
}

function ConnectionSection({ summary }: { summary: AcaConnectionSummary }) {
  const [editing, setEditing] = useState(!summary.connected);
  return editing ? (
    <ConnectionForm summary={summary} onCancel={summary.connected ? () => setEditing(false) : undefined} />
  ) : (
    <ConnectionSummaryView summary={summary} onEdit={() => setEditing(true)} />
  );
}

function ConnectionSummaryView({
  summary,
  onEdit,
}: {
  summary: AcaConnectionSummary;
  onEdit: () => void;
}) {
  const { mutate } = useSWRConfig();
  const toast = useToast();
  const [busy, setBusy] = useState<"test" | "disconnect" | null>(null);
  const [result, setResult] = useState<AcaConnectionTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function onTest() {
    setBusy("test");
    setError(null);
    try {
      setResult(await apiData(() => sandboxesApi.testAcaConnection()));
    } catch (cause) {
      setError(errorMessage(cause, "Couldn't test the Azure connection."));
    } finally {
      setBusy(null);
    }
  }

  async function onDisconnect() {
    if (
      !window.confirm(
        "Disconnect Azure? New ACA runs will fail until a connection is saved again. Existing sandboxes are not deleted.",
      )
    ) {
      return;
    }
    setBusy("disconnect");
    setError(null);
    try {
      await apiData(() => sandboxesApi.deleteAcaConnection());
      await mutate(queryKeys.settings.acaConnection());
      toast.push({ message: "Azure disconnected." });
    } catch (cause) {
      setError(errorMessage(cause, "Couldn't disconnect Azure."));
      setBusy(null);
    }
  }

  return (
    <Panel title="Azure Container Apps connection">
      <Row title="Status">
        <span className="inline-flex items-center gap-2">
          <Dot on={summary.enabled ?? true} />
          {summary.enabled === false
            ? "Connected, but ACA runs are disabled by server.sandbox.providers.aca.enabled"
            : "Connected"}
        </span>
      </Row>
      <Row title="Sandbox group">
        <span className="font-mono text-xs">
          {summary.subscription_id} / {summary.resource_group} / {summary.sandbox_group}
          {summary.location ? ` (${summary.location})` : ""}
        </span>
      </Row>
      <Row title="Service principal">
        <span className="font-mono text-xs">
          {summary.client_id} in tenant {summary.tenant_id}
        </span>
        <div className="text-xs text-fg-3">
          Client secret {summary.client_secret_saved ? "saved (hidden)" : "missing"}
        </div>
      </Row>
      <Row title="Images">
        {summary.allowed_images.length > 0
          ? `Approved: ${summary.allowed_images.join(", ")}`
          : "Any image the sandbox group can use"}
        <div className="text-xs text-fg-3">
          Default: {summary.default_image ?? "azure-dev"}
        </div>
      </Row>
      <Row title="Retained sandboxes" help="Stopped sandboxes kept for inspection are deleted by Azure after this time.">
        {summary.retention_hours} {summary.retention_hours === 1 ? "hour" : "hours"}
      </Row>
      <div className="space-y-3 px-4 py-3.5">
        {result ? <TestResultView result={result} /> : null}
        {error ? <ErrorMessage message={error} /> : null}
        <div className="flex flex-wrap gap-2">
          <button type="button" onClick={onTest} disabled={busy !== null} className={SECONDARY_BUTTON_CLASS}>
            {busy === "test" ? "Testing…" : "Test connection"}
          </button>
          <button type="button" onClick={onEdit} disabled={busy !== null} className={SECONDARY_BUTTON_CLASS}>
            Update credentials
          </button>
          <button type="button" onClick={onDisconnect} disabled={busy !== null} className={SECONDARY_BUTTON_CLASS}>
            {busy === "disconnect" ? "Disconnecting…" : "Disconnect"}
          </button>
        </div>
      </div>
    </Panel>
  );
}

function ConnectionForm({
  summary,
  onCancel,
}: {
  summary: AcaConnectionSummary;
  onCancel?: () => void;
}) {
  const { mutate } = useSWRConfig();
  const toast = useToast();
  const [form, setForm] = useState<FormState>(() => formFromSummary(summary));
  const [busy, setBusy] = useState<"test" | "save" | null>(null);
  const [result, setResult] = useState<AcaConnectionTestResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const missing = missingFields(form, summary.client_secret_saved);

  const set = (field: keyof FormState) => (value: string) =>
    setForm((current) => ({ ...current, [field]: value }));

  async function onTest() {
    setBusy("test");
    setError(null);
    setResult(null);
    try {
      setResult(await apiData(() => sandboxesApi.testAcaConnection(inputFromForm(form))));
    } catch (cause) {
      setError(errorMessage(cause, "Couldn't test the Azure connection."));
    } finally {
      setBusy(null);
    }
  }

  async function onSave(event: FormEvent) {
    event.preventDefault();
    if (missing.length > 0) return;
    setBusy("save");
    setError(null);
    try {
      await apiData(() => sandboxesApi.saveAcaConnection(inputFromForm(form)));
      await mutate(queryKeys.settings.acaConnection());
      toast.push({ message: "Azure connection saved." });
    } catch (cause) {
      setError(errorMessage(cause, "Couldn't save the Azure connection."));
      setBusy(null);
    }
  }

  return (
    <form onSubmit={onSave}>
      <Panel title="Azure Container Apps connection">
        <div className="space-y-2 px-4 py-3.5 text-xs/5 text-fg-3">
          <p>
            Fabro runs workflows in an Azure Container Apps sandbox group with a service principal.
            You need a sandbox group, a Microsoft Entra app registration with a client secret, and
            two role assignments for that app on the sandbox group:{" "}
            <span className="text-fg-2">Reader</span> and{" "}
            <span className="text-fg-2">Container Apps SandboxGroup Data Owner</span>.
          </p>
          <p>
            Testing only reads from Azure: it creates no sandbox and costs nothing. The client secret
            is stored in the server's vault, is never shown again, and never reaches a run.{" "}
            <a href={DOCS_URL} target="_blank" rel="noreferrer" className="text-teal-500 hover:underline">
              Setup guide
            </a>
          </p>
        </div>
        <Field label="Tenant ID" required help="Microsoft Entra ID > Overview > Tenant ID." value={form.tenant_id} onChange={set("tenant_id")} />
        <Field label="Client ID" required help="The app registration's Application (client) ID." value={form.client_id} onChange={set("client_id")} />
        <Field
          label="Client secret"
          required={!summary.client_secret_saved}
          help={
            summary.client_secret_saved
              ? "Leave empty to keep the saved secret, or enter a new one to replace it."
              : "A client secret of the app registration. Shown once in the Azure portal."
          }
          value={form.client_secret}
          onChange={set("client_secret")}
          secret
        />
        <Field label="Subscription ID" required value={form.subscription_id} onChange={set("subscription_id")} />
        <Field label="Resource group" required value={form.resource_group} onChange={set("resource_group")} />
        <Field label="Sandbox group" required help="The name of the ACA sandbox group runs create sandboxes in." value={form.sandbox_group} onChange={set("sandbox_group")} />
        <Field label="Location" optional help="Read from the sandbox group when empty, e.g. westus2." value={form.location} onChange={set("location")} />
        <Field
          label="Approved images"
          optional
          help="Disk images runs may use, separated by commas. Empty allows any image the group can see."
          value={form.allowed_images}
          onChange={set("allowed_images")}
        />
        <Field label="Default image" optional help="Used when an environment names no image. Defaults to azure-dev, which includes Docker." value={form.default_image} onChange={set("default_image")} />
        <Field
          label="Retention (hours)"
          help="How long a stopped sandbox kept for inspection lives before Azure deletes it. At most 24."
          value={form.retention_hours}
          onChange={set("retention_hours")}
          inputMode="numeric"
        />
        <div className="space-y-3 px-4 py-3.5">
          {result ? <TestResultView result={result} /> : null}
          {error ? <ErrorMessage message={error} /> : null}
          <div className="flex flex-wrap gap-2">
            <button type="button" onClick={onTest} disabled={busy !== null || missing.length > 0} className={SECONDARY_BUTTON_CLASS}>
              {busy === "test" ? "Testing…" : "Test connection"}
            </button>
            <button type="submit" disabled={busy !== null || missing.length > 0} className={PRIMARY_BUTTON_CLASS}>
              {busy === "save" ? "Testing and saving…" : "Save connection"}
            </button>
            {onCancel ? (
              <button type="button" onClick={onCancel} disabled={busy !== null} className={SECONDARY_BUTTON_CLASS}>
                Cancel
              </button>
            ) : null}
          </div>
        </div>
      </Panel>
    </form>
  );
}

function Field({
  label,
  help,
  required,
  optional,
  value,
  onChange,
  secret,
  inputMode,
}: {
  label: string;
  help?: ReactNode;
  required?: boolean;
  optional?: boolean;
  value: string;
  onChange: (value: string) => void;
  secret?: boolean;
  inputMode?: "numeric";
}) {
  return (
    <Row
      title={
        <Label required={required} optional={optional}>
          {label}
        </Label>
      }
      help={help}
    >
      <input
        type={secret ? "password" : "text"}
        aria-label={label}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        autoComplete={secret ? "new-password" : "off"}
        spellCheck={false}
        inputMode={inputMode}
        className={`${INPUT_CLASS} font-mono`}
      />
    </Row>
  );
}

export function TestResultView({ result }: { result: AcaConnectionTestResult }) {
  if (result.ready) {
    return (
      <div role="status" className="rounded-md bg-teal-500/10 px-3 py-2 text-sm/6 text-fg-2 outline-1 -outline-offset-1 outline-teal-500/40">
        Ready for ACA runs
        {result.location ? ` in ${result.location}` : ""}
        {result.image ? `, default image ${result.image}` : ""}.
        {result.images.length > 0 ? (
          <div className="text-xs text-fg-3">
            Images: {result.images.map((image) => `${image.name}${image.ready ? "" : " (not ready)"}`).join(", ")}
          </div>
        ) : null}
      </div>
    );
  }
  return (
    <div role="alert" className="space-y-1 rounded-md bg-coral/10 px-3 py-2 text-sm/6 text-fg-2 outline-1 -outline-offset-1 outline-coral/40">
      <div>Not ready for ACA runs.</div>
      <ul className="list-disc pl-5 text-xs">
        {result.problems.map((problem) => (
          <li key={problem}>{problem}</li>
        ))}
      </ul>
      {result.required_roles && result.required_roles.length > 0 ? (
        <div className="text-xs text-fg-3">Required roles: {result.required_roles.join("; ")}</div>
      ) : null}
    </div>
  );
}
