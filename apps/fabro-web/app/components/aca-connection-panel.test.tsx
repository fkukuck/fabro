import { afterEach, describe, expect, mock, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";

import type {
  AcaConnectionInput,
  AcaConnectionSummary,
  AcaConnectionTestResult,
} from "@qltysh/fabro-api-client";

let currentSummary: AcaConnectionSummary | undefined;
const saveMock = mock((_input: AcaConnectionInput) =>
  Promise.resolve({ data: currentSummary }),
);
const testMock = mock((_input?: AcaConnectionInput) =>
  Promise.resolve({ data: { ready: true, problems: [], images: [] } }),
);
const deleteMock = mock(() => Promise.resolve({ data: undefined }));
const mutateMock = mock((..._args: unknown[]) => Promise.resolve(undefined));
const toastMock = mock((_toast: unknown) => undefined);

const actualQueries = await import("../lib/queries");
mock.module("../lib/queries", () => ({
  ...actualQueries,
  useAcaConnection: () => ({ data: currentSummary, error: undefined }),
}));

// Spread the real module: bun shares one module registry across test files,
// so a partial mock would break later files that import the client.
const actualApiClient = await import("../lib/api-client");
mock.module("../lib/api-client", () => ({
  ...actualApiClient,
  sandboxesApi: {
    saveAcaConnection: (input: AcaConnectionInput) => saveMock(input),
    testAcaConnection: (input?: AcaConnectionInput) => testMock(input),
    deleteAcaConnection: () => deleteMock(),
  },
}));

const actualSwr = await import("swr");
mock.module("swr", () => ({
  ...actualSwr,
  useSWRConfig: () => ({ mutate: mutateMock }),
}));

const actualToast = await import("./toast");
mock.module("./toast", () => ({ ...actualToast, useToast: () => ({ push: toastMock }) }));

const {
  AcaConnectionPanel,
  TestResultView,
  formFromSummary,
  inputFromForm,
  missingFields,
} = await import("./aca-connection-panel");

const connected: AcaConnectionSummary = {
  connected:           true,
  tenant_id:           "tenant-1",
  client_id:           "client-1",
  client_secret_saved: true,
  subscription_id:     "sub-1",
  resource_group:      "rg-1",
  sandbox_group:       "group-1",
  location:            "westus2",
  allowed_images:      ["azure-dev"],
  default_image:       "azure-dev",
  retention_hours:     6,
  enabled:             true,
  updated_at:          "2026-10-19T14:00:00Z",
};

const disconnected: AcaConnectionSummary = {
  connected:           false,
  client_secret_saved: false,
  allowed_images:      [],
  retention_hours:     24,
  enabled:             true,
};

function textFromNode(node: ReturnType<TestRenderer.ReactTestRenderer["toJSON"]>): string {
  if (!node) return "";
  if (typeof node === "string") return node;
  if (Array.isArray(node)) return node.map(textFromNode).join(" ");
  return (node.children ?? []).map(textFromNode).join(" ");
}

const mounted: TestRenderer.ReactTestRenderer[] = [];

function render(element: React.ReactElement): TestRenderer.ReactTestRenderer {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  let renderer!: TestRenderer.ReactTestRenderer;
  act(() => {
    renderer = TestRenderer.create(element);
  });
  mounted.push(renderer);
  return renderer;
}

afterEach(() => {
  for (const renderer of mounted.splice(0)) {
    act(() => renderer.unmount());
  }
  currentSummary = undefined;
  saveMock.mockClear();
  testMock.mockClear();
  deleteMock.mockClear();
  mutateMock.mockClear();
  delete (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT;
});

describe("ACA connection form helpers", () => {
  test("an empty client secret is left out so the saved one is kept", () => {
    const input = inputFromForm(formFromSummary(connected));
    expect(input.client_secret).toBeUndefined();
    expect(input.sandbox_group).toBe("group-1");
    expect(input.allowed_images).toEqual(["azure-dev"]);
    expect(input.retention_hours).toBe(6);
  });

  test("approved images split on commas and whitespace", () => {
    const form = { ...formFromSummary(connected), allowed_images: "azure-dev, node-22  ubuntu" };
    expect(inputFromForm(form).allowed_images).toEqual(["azure-dev", "node-22", "ubuntu"]);
  });

  test("a new connection needs every required field and a secret", () => {
    const form = formFromSummary(undefined);
    expect(missingFields(form, false)).toEqual([
      "tenant_id",
      "client_id",
      "subscription_id",
      "resource_group",
      "sandbox_group",
      "client_secret",
    ]);
    expect(missingFields(formFromSummary(connected), true)).toEqual([]);
  });
});

describe("AcaConnectionPanel", () => {
  test("shows the setup form with the required roles when disconnected", () => {
    currentSummary = disconnected;
    const text = textFromNode(render(<AcaConnectionPanel />).toJSON());
    expect(text).toContain("Tenant ID");
    expect(text).toContain("Container Apps SandboxGroup Data Owner");
    expect(text).toContain("creates no sandbox");
  });

  test("shows the saved connection without its secret", () => {
    currentSummary = connected;
    const text = textFromNode(render(<AcaConnectionPanel />).toJSON());
    expect(text).toContain("group-1");
    expect(text).toContain("saved (hidden)");
    expect(text).toContain("Disconnect");
  });

  test("a failed test names every problem and the roles", () => {
    const result: AcaConnectionTestResult = {
      ready:          false,
      problems:       ["Azure did not issue a token (HTTP 401): the client secret is wrong or has expired"],
      images:         [],
      required_roles: ["Reader on the sandbox group"],
    };
    const text = textFromNode(render(<TestResultView result={result} />).toJSON());
    expect(text).toContain("Not ready");
    expect(text).toContain("has expired");
    expect(text).toContain("Reader on the sandbox group");
  });
});
