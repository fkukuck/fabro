import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";
import { MemoryRouter } from "react-router";
import type { AzureConnection } from "../lib/azure";
import { setupReactTestEnv } from "../lib/test-utils";

let connection: AzureConnection | undefined;
let teardownReactTestEnv: (() => void) | undefined;

mock.module("../lib/queries", () => ({
  useAzureConnection: () => ({ data: connection }),
}));
mock.module("../components/toast", () => ({ useToast: () => ({ push: () => {} }) }));

const { default: SettingsAzure } = await import("./settings-azure");

const mounted: TestRenderer.ReactTestRenderer[] = [];

function render() {
  let renderer: TestRenderer.ReactTestRenderer | undefined;
  act(() => {
    renderer = TestRenderer.create(
      <MemoryRouter>
        <SettingsAzure />
      </MemoryRouter>,
    );
  });
  mounted.push(renderer!);
  return renderer!;
}

function textContent(node: ReturnType<TestRenderer.ReactTestRenderer["toJSON"]>): string {
  if (node == null || typeof node === "boolean") return "";
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textContent).join("");
  return node.children?.map(textContent).join("") ?? "";
}

beforeEach(() => {
  teardownReactTestEnv = setupReactTestEnv();
});

afterEach(() => {
  act(() => {
    for (const renderer of mounted.splice(0)) renderer.unmount();
  });
  teardownReactTestEnv?.();
  connection = undefined;
});

describe("SettingsAzure", () => {
  test("an unconfigured connection explains the setup and offers no disconnect", () => {
    connection = { configured: false, client_secret_set: false };
    const renderer = render();
    const text = textContent(renderer.toJSON());
    expect(text).toContain("Not configured");
    expect(text).toContain("Container Apps SandboxGroup Data Owner");
    expect(text).toContain("Not available yet: no workflow has run on ACA");
    const buttons = renderer.root.findAllByType("button").map((b) => textContent(b.props.children));
    expect(buttons).not.toContain("Disconnect");
  });

  test("a saved connection shows its details but keeps the secret hidden", () => {
    connection = {
      configured: true,
      client_secret_set: true,
      secret_updated_at: "2026-11-17T10:00:00Z",
      updated_at: "2026-11-17T10:00:00Z",
      settings: {
        tenant_id: "contoso.onmicrosoft.com",
        client_id: "11111111-2222-3333-4444-555555555555",
        subscription_id: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        location: "westus2",
        resource_group: "fabro-rg",
        sandbox_group: "fabro-sandboxes",
        execution: {
          enabled: true,
          image: "ubuntu",
          allowed_images: [],
          cpu_millicores: 1000,
          memory_mib: 2048,
        },
      },
    };
    const renderer = render();
    const secret = renderer.root.find(
      (node) => node.type === "input" && node.props.name === "client_secret",
    );
    expect(secret.props.type).toBe("password");
    expect(secret.props.value).toBe("");
    const group = renderer.root.find(
      (node) => node.type === "input" && node.props.name === "sandbox_group",
    );
    expect(group.props.value).toBe("fabro-sandboxes");
    const text = textContent(renderer.toJSON());
    expect(text).toContain("ubuntu, 1 vCPU / 2 GiB");
    const buttons = renderer.root.findAllByType("button").map((b) => textContent(b.props.children));
    expect(buttons).toContain("Disconnect");
  });
});
