import { apiData, apiResponse, generatedAxios } from "./api-client";

// The Azure Container Apps connection, as `/api/v1/integrations/azure`
// serves it. The client secret is write-only: the server reports only
// whether one is saved.

export type AcaExecutionSettings = {
  enabled: boolean;
  image: string;
  allowed_images: string[];
  cpu_millicores: number;
  memory_mib: number;
};

export type AzureConnectionSettings = {
  tenant_id: string;
  client_id: string;
  subscription_id: string;
  location: string;
  resource_group: string;
  sandbox_group: string;
  execution: AcaExecutionSettings;
};

export type AzureConnection = {
  configured: boolean;
  settings?: AzureConnectionSettings;
  client_secret_set: boolean;
  secret_updated_at?: string;
  updated_at?: string;
};

export type AzureConnectionInput = AzureConnectionSettings & {
  client_secret?: string;
};

export type AzureCheckStatus = "passed" | "failed" | "skipped";

export type AzureReadinessCheck = {
  id: string;
  label: string;
  status: AzureCheckStatus;
  message: string;
};

export type AzureReadinessReport = {
  ready: boolean;
  checks: AzureReadinessCheck[];
  checked_at: string;
};

const AZURE_PATH = "/api/v1/integrations/azure";

export const azureQueryKey = ["integrations", "azure"] as const;

export function fetchAzureConnection(): Promise<AzureConnection> {
  return apiData(() => generatedAxios.get<AzureConnection>(AZURE_PATH));
}

export function saveAzureConnection(input: AzureConnectionInput): Promise<AzureConnection> {
  return apiData(() => generatedAxios.put<AzureConnection>(AZURE_PATH, input));
}

export async function deleteAzureConnection(): Promise<void> {
  await apiResponse(() => generatedAxios.delete(AZURE_PATH));
}

export function testAzureConnection(input: AzureConnectionInput): Promise<AzureReadinessReport> {
  return apiData(() =>
    generatedAxios.post<AzureReadinessReport>(`${AZURE_PATH}/test`, input),
  );
}

// The sandbox sizes the server accepts. Keep in step with
// `SANDBOX_SIZES` in `lib/apps/fabro-server/src/azure.rs`.
export const SANDBOX_SIZES = [
  { name: "XS", cpu_millicores: 250, memory_mib: 512 },
  { name: "S", cpu_millicores: 500, memory_mib: 1024 },
  { name: "M", cpu_millicores: 1000, memory_mib: 2048 },
  { name: "L", cpu_millicores: 2000, memory_mib: 4096 },
  { name: "XL", cpu_millicores: 4000, memory_mib: 8192 },
] as const;

export const BUILT_IN_IMAGE = "ubuntu";

export function sizeLabel(cpuMillicores: number, memoryMib: number): string {
  return `${cpuMillicores / 1000} vCPU / ${memoryMib / 1024} GiB`;
}

export function sizeName(cpuMillicores: number, memoryMib: number): string | null {
  return (
    SANDBOX_SIZES.find(
      (size) => size.cpu_millicores === cpuMillicores && size.memory_mib === memoryMib,
    )?.name ?? null
  );
}

// The form's flat state: everything a person edits, as text.
export type AzureFormState = {
  tenantId: string;
  clientId: string;
  clientSecret: string;
  subscriptionId: string;
  location: string;
  resourceGroup: string;
  sandboxGroup: string;
  enabled: boolean;
  image: string;
  allowedImages: string;
  size: string;
};

export function initialFormState(connection: AzureConnection | undefined): AzureFormState {
  const settings = connection?.settings;
  const execution = settings?.execution;
  return {
    tenantId: settings?.tenant_id ?? "",
    clientId: settings?.client_id ?? "",
    clientSecret: "",
    subscriptionId: settings?.subscription_id ?? "",
    location: settings?.location ?? "",
    resourceGroup: settings?.resource_group ?? "",
    sandboxGroup: settings?.sandbox_group ?? "",
    enabled: execution?.enabled ?? true,
    image: execution?.image ?? BUILT_IN_IMAGE,
    allowedImages: (execution?.allowed_images ?? []).join("\n"),
    size: execution
      ? (sizeName(execution.cpu_millicores, execution.memory_mib) ?? "M")
      : "M",
  };
}

export function parseAllowedImages(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((image) => image.trim())
    .filter((image) => image !== "");
}

// The request body for a form. A blank secret is omitted, which keeps the
// saved one.
export function formToInput(form: AzureFormState): AzureConnectionInput {
  const size = SANDBOX_SIZES.find((candidate) => candidate.name === form.size) ?? SANDBOX_SIZES[2];
  const input: AzureConnectionInput = {
    tenant_id: form.tenantId.trim(),
    client_id: form.clientId.trim(),
    subscription_id: form.subscriptionId.trim(),
    location: form.location.trim(),
    resource_group: form.resourceGroup.trim(),
    sandbox_group: form.sandboxGroup.trim(),
    execution: {
      enabled: form.enabled,
      image: form.image.trim(),
      allowed_images: parseAllowedImages(form.allowedImages),
      cpu_millicores: size.cpu_millicores,
      memory_mib: size.memory_mib,
    },
  };
  if (form.clientSecret.trim() !== "") {
    input.client_secret = form.clientSecret;
  }
  return input;
}

// What the form still needs before it can be saved, in the order the
// fields appear. The server checks the same rules.
export function formProblems(form: AzureFormState, secretSaved: boolean): string[] {
  const problems: string[] = [];
  const required: Array<[string, string]> = [
    [form.tenantId, "Tenant ID"],
    [form.clientId, "Client ID"],
    [form.subscriptionId, "Subscription ID"],
    [form.location, "Location"],
    [form.resourceGroup, "Resource group"],
    [form.sandboxGroup, "Sandbox group"],
    [form.image, "Image"],
  ];
  for (const [value, label] of required) {
    if (value.trim() === "") problems.push(`${label} is required.`);
  }
  if (!secretSaved && form.clientSecret.trim() === "") {
    problems.push("Client secret is required.");
  }
  const allowed = parseAllowedImages(form.allowedImages);
  const image = form.image.trim();
  if (image !== "" && allowed.length > 0 && !allowed.includes(image)) {
    problems.push(`Image ${image} is not in the list of allowed images.`);
  }
  return problems;
}
