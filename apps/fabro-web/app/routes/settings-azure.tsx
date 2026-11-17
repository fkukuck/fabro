import { useState, type ReactNode } from "react";
import { Link } from "react-router";
import { useSWRConfig } from "swr";
import { ChevronRightIcon } from "@heroicons/react/20/solid";

import { ApiError } from "../lib/api-client";
import { useAzureConnection } from "../lib/queries";
import {
  SANDBOX_SIZES,
  azureQueryKey,
  deleteAzureConnection,
  formProblems,
  formToInput,
  initialFormState,
  saveAzureConnection,
  sizeLabel,
  testAzureConnection,
  type AzureConnection,
  type AzureFormState,
  type AzureReadinessReport,
} from "../lib/azure";
import { Dot, Label, Panel, PanelSkeleton, Row } from "../components/settings-panel";
import {
  ConfirmDialog,
  ErrorMessage,
  INPUT_CLASS,
  PRIMARY_BUTTON_CLASS,
  SECONDARY_BUTTON_CLASS,
} from "../components/ui";
import { useToast } from "../components/toast";

export function meta() {
  return [{ title: "Azure Container Apps — Fabro" }];
}

export default function SettingsAzure() {
  const query = useAzureConnection();
  const connection = query.data;

  return (
    <div className="space-y-6">
      <PageHeader />
      <SetupGuide />
      {query.error ? (
        <ErrorMessage message={errorText(query.error, "Couldn't load the Azure connection.")} />
      ) : connection ? (
        <AzureConnectionForm
          // A saved connection remounts the form with the saved values.
          key={connection.updated_at ?? "new"}
          connection={connection}
        />
      ) : (
        <PanelSkeleton />
      )}
    </div>
  );
}

function PageHeader() {
  return (
    <nav className="flex items-center gap-1 text-sm text-fg-muted">
      <Link to="/settings/sandboxes" className="text-fg-3 hover:text-fg">
        Sandboxes
      </Link>
      <ChevronRightIcon className="size-3" aria-hidden="true" />
      <span>Azure Container Apps</span>
    </nav>
  );
}

function SetupGuide() {
  return (
    <Panel title="Before you start">
      <div className="space-y-3 px-4 py-3.5 text-sm/6 text-fg-3">
        <p>
          Fabro connects to Azure Container Apps sandboxes with an app registration (service
          principal) and a client secret, so the hosted server works unattended without anyone
          staying signed in on a terminal.
        </p>
        <ul className="list-disc space-y-1 pl-5">
          <li>
            <strong className="text-fg-2">Must already exist:</strong> a subscription, a resource
            group, and a sandbox group in that resource group (create it at{" "}
            <ExternalLink href="https://sandboxes.azure.com/sandbox-groups">
              sandboxes.azure.com
            </ExternalLink>
            ). Import any custom disk image into the group first; <code>ubuntu</code> is built in.
          </li>
          <li>
            <strong className="text-fg-2">App registration:</strong> in the Azure portal, open
            Microsoft Entra ID, App registrations, New registration. The overview shows the{" "}
            <em>Directory (tenant) ID</em> and <em>Application (client) ID</em>. Under Certificates
            &amp; secrets, create a client secret and copy its <em>Value</em>. Note its expiry date;
            Fabro reports an expired secret when you test.
          </li>
          <li>
            <strong className="text-fg-2">Permissions:</strong> assign the app registration the{" "}
            <em>Reader</em> role and the <em>Container Apps SandboxGroup Data Owner</em> role on the
            sandbox group (Access control (IAM), Add role assignment).
          </li>
          <li>
            <strong className="text-fg-2">Costs:</strong> testing the connection only reads from
            Azure and starts nothing billable. Once workflows run on ACA, Azure bills vCPU per
            core-second and memory per GiB-second while a sandbox runs, plus storage for custom
            images and snapshots. See{" "}
            <ExternalLink href="https://sandboxes.azure.com/docs/sandboxes/cost">
              ACA sandbox costs
            </ExternalLink>
            .
          </li>
        </ul>
        <p>
          The client secret is stored on the Fabro server outside the secrets vault, so workflows and
          agent sandboxes cannot read it. It is never shown again after saving.
        </p>
      </div>
    </Panel>
  );
}

function ExternalLink({ href, children }: { href: string; children: ReactNode }) {
  return (
    <a href={href} target="_blank" rel="noreferrer" className="text-teal-500 hover:underline">
      {children}
    </a>
  );
}

function AzureConnectionForm({ connection }: { connection: AzureConnection }) {
  const { mutate } = useSWRConfig();
  const toast = useToast();
  const [form, setForm] = useState<AzureFormState>(() => initialFormState(connection));
  const [busy, setBusy] = useState<"save" | "test" | "delete" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<AzureReadinessReport | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const secretSaved = connection.client_secret_set;
  const problems = formProblems(form, secretSaved);

  function update<K extends keyof AzureFormState>(key: K, value: AzureFormState[K]) {
    setForm((current) => ({ ...current, [key]: value }));
    setReport(null);
  }

  async function onTest() {
    setBusy("test");
    setError(null);
    try {
      setReport(await testAzureConnection(formToInput(form)));
    } catch (cause) {
      setError(errorText(cause, "Couldn't test the connection. Please try again."));
    } finally {
      setBusy(null);
    }
  }

  async function onSave(event: React.FormEvent) {
    event.preventDefault();
    if (problems.length > 0) {
      setError(problems.join(" "));
      return;
    }
    setBusy("save");
    setError(null);
    try {
      const saved = await saveAzureConnection(formToInput(form));
      toast.push({ message: "Azure connection saved." });
      await mutate(azureQueryKey, saved, { revalidate: false });
    } catch (cause) {
      setError(errorText(cause, "Couldn't save the Azure connection. Please try again."));
      setBusy(null);
    }
  }

  async function onDelete() {
    setBusy("delete");
    setError(null);
    try {
      await deleteAzureConnection();
      toast.push({ message: "Azure disconnected." });
      setConfirmingDelete(false);
      await mutate(azureQueryKey);
    } catch (cause) {
      setError(errorText(cause, "Couldn't disconnect Azure. Please try again."));
      setConfirmingDelete(false);
      setBusy(null);
    }
  }

  return (
    <form onSubmit={onSave} className="space-y-6">
      <StatusPanel connection={connection} />

      <Panel title="Credentials">
        <Row
          title={<Label required>Tenant ID</Label>}
          help="Directory (tenant) ID from the app registration's overview."
        >
          <TextInput
            name="tenant_id"
            label="Tenant ID"
            value={form.tenantId}
            onChange={(value) => update("tenantId", value)}
            placeholder="00000000-0000-0000-0000-000000000000"
          />
        </Row>
        <Row
          title={<Label required>Client ID</Label>}
          help="Application (client) ID of the app registration."
        >
          <TextInput
            name="client_id"
            label="Client ID"
            value={form.clientId}
            onChange={(value) => update("clientId", value)}
            placeholder="00000000-0000-0000-0000-000000000000"
          />
        </Row>
        <Row
          title={<Label required={!secretSaved}>Client secret</Label>}
          help={
            secretSaved
              ? "A secret is saved and hidden. Leave blank to keep it, or enter a new value to replace it."
              : "The secret's Value from Certificates & secrets. Never shown again after saving."
          }
        >
          <input
            type="password"
            name="client_secret"
            aria-label="Client secret"
            value={form.clientSecret}
            onChange={(event) => update("clientSecret", event.target.value)}
            placeholder={secretSaved ? "•••••••• (saved)" : ""}
            autoComplete="new-password"
            spellCheck={false}
            className={`${INPUT_CLASS} font-mono`}
          />
        </Row>
      </Panel>

      <Panel title="Azure resources">
        <Row title={<Label required>Subscription ID</Label>} help="The subscription that holds the sandbox group.">
          <TextInput
            name="subscription_id"
            label="Subscription ID"
            value={form.subscriptionId}
            onChange={(value) => update("subscriptionId", value)}
            placeholder="00000000-0000-0000-0000-000000000000"
          />
        </Row>
        <Row title={<Label required>Location</Label>} help="The sandbox group's Azure region, for example westus2.">
          <TextInput
            name="location"
            label="Location"
            value={form.location}
            onChange={(value) => update("location", value)}
            placeholder="westus2"
          />
        </Row>
        <Row title={<Label required>Resource group</Label>}>
          <TextInput
            name="resource_group"
            label="Resource group"
            value={form.resourceGroup}
            onChange={(value) => update("resourceGroup", value)}
          />
        </Row>
        <Row title={<Label required>Sandbox group</Label>} help="An existing ACA sandbox group. Fabro does not create it.">
          <TextInput
            name="sandbox_group"
            label="Sandbox group"
            value={form.sandboxGroup}
            onChange={(value) => update("sandboxGroup", value)}
          />
        </Row>
      </Panel>

      <Panel title="Execution environment">
        <Row
          title={<Label>ACA execution</Label>}
          help="Turn off to keep the connection saved while ACA cannot be used for execution."
        >
          <label className="inline-flex items-center gap-2 text-sm text-fg-2">
            <input
              type="checkbox"
              name="enabled"
              checked={form.enabled}
              onChange={(event) => update("enabled", event.target.checked)}
            />
            Allow ACA execution
          </label>
        </Row>
        <Row
          title={<Label optional>Allowed images</Label>}
          help="One prepared disk image per line. Leave empty to allow any image in the sandbox group."
        >
          <textarea
            name="allowed_images"
            aria-label="Allowed images"
            value={form.allowedImages}
            onChange={(event) => update("allowedImages", event.target.value)}
            rows={3}
            spellCheck={false}
            className={`${INPUT_CLASS} resize-y font-mono`}
          />
        </Row>
        <Row title={<Label required>Image</Label>} help="The prepared disk image sandboxes boot from. ubuntu is built in.">
          <TextInput
            name="image"
            label="Image"
            value={form.image}
            onChange={(value) => update("image", value)}
            placeholder="ubuntu"
          />
        </Row>
        <Row title={<Label required>Size</Label>} help="CPU and memory for each sandbox.">
          <select
            name="size"
            aria-label="Size"
            value={form.size}
            onChange={(event) => update("size", event.target.value)}
            className={INPUT_CLASS}
          >
            {SANDBOX_SIZES.map((size) => (
              <option key={size.name} value={size.name}>
                {size.name}: {sizeLabel(size.cpu_millicores, size.memory_mib)}
              </option>
            ))}
          </select>
        </Row>
      </Panel>

      {report ? <ReadinessPanel report={report} /> : null}
      {error ? <ErrorMessage message={error} /> : null}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          {connection.configured ? (
            <button
              type="button"
              onClick={() => setConfirmingDelete(true)}
              disabled={busy !== null}
              className={SECONDARY_BUTTON_CLASS}
            >
              Disconnect
            </button>
          ) : null}
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            onClick={onTest}
            disabled={busy !== null}
            className={SECONDARY_BUTTON_CLASS}
          >
            {busy === "test" ? "Testing…" : "Test connection"}
          </button>
          <button type="submit" disabled={busy !== null} className={PRIMARY_BUTTON_CLASS}>
            {busy === "save" ? "Saving…" : "Save"}
          </button>
        </div>
      </div>

      <ConfirmDialog
        open={confirmingDelete}
        title="Disconnect Azure?"
        description="This removes the saved credentials and the ACA execution environment from Fabro. Nothing in Azure is changed."
        confirmLabel="Disconnect"
        pendingLabel="Disconnecting…"
        pending={busy === "delete"}
        onConfirm={onDelete}
        onCancel={() => setConfirmingDelete(false)}
      />
    </form>
  );
}

function StatusPanel({ connection }: { connection: AzureConnection }) {
  const execution = connection.settings?.execution;
  return (
    <Panel title="Status">
      <Row title="Azure connection">
        <span className="inline-flex items-center gap-2">
          <Dot on={connection.configured} />
          {connection.configured ? (
            <span>
              Saved
              {connection.secret_updated_at ? (
                <span className="text-fg-muted">
                  {" "}
                  (secret updated {formatDate(connection.secret_updated_at)})
                </span>
              ) : null}
            </span>
          ) : (
            <span className="text-fg-muted">Not configured</span>
          )}
        </span>
      </Row>
      {execution ? (
        <Row title="ACA execution environment">
          {execution.enabled ? (
            <span>
              {execution.image}, {sizeLabel(execution.cpu_millicores, execution.memory_mib)}
            </span>
          ) : (
            <span className="text-fg-muted">Disabled by an administrator</span>
          )}
        </Row>
      ) : null}
      <Row
        title="Workflow execution"
        help="Running workflows in ACA sandboxes ships in a later release. Existing sandbox providers are unaffected."
      >
        <span className="text-fg-muted">Not available yet: no workflow has run on ACA</span>
      </Row>
    </Panel>
  );
}

function ReadinessPanel({ report }: { report: AzureReadinessReport }) {
  return (
    <Panel title={report.ready ? "Connection test: ready" : "Connection test: needs attention"}>
      {report.checks.map((check) => (
        <Row key={check.id} title={check.label}>
          <span className="flex items-start gap-2">
            <span
              className={
                check.status === "passed"
                  ? "text-teal-500"
                  : check.status === "failed"
                    ? "text-coral"
                    : "text-fg-muted"
              }
            >
              {check.status === "passed" ? "Passed" : check.status === "failed" ? "Failed" : "Skipped"}
            </span>
            <span className="text-fg-3">{check.message}</span>
          </span>
        </Row>
      ))}
      <div className="px-4 py-3 text-xs text-fg-muted">
        This test only read from Azure; it created no sandbox. It does not verify workflow
        execution.
      </div>
    </Panel>
  );
}

function TextInput({
  name,
  label,
  value,
  onChange,
  placeholder,
}: {
  name: string;
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  return (
    <input
      type="text"
      name={name}
      aria-label={label}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder={placeholder}
      autoComplete="off"
      spellCheck={false}
      className={`${INPUT_CLASS} font-mono`}
    />
  );
}

function errorText(cause: unknown, fallback: string): string {
  return cause instanceof ApiError && cause.message ? cause.message : fallback;
}

function formatDate(iso: string): string {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? iso : date.toLocaleString();
}
