import { describe, expect, test } from "bun:test";
import {
  formProblems,
  formToInput,
  initialFormState,
  sizeName,
  type AzureConnection,
} from "./azure";

const saved: AzureConnection = {
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
      enabled: false,
      image: "fabro-dev",
      allowed_images: ["fabro-dev", "ubuntu"],
      cpu_millicores: 2000,
      memory_mib: 4096,
    },
  },
};

describe("Azure connection form", () => {
  test("a saved connection fills the form but never the secret", () => {
    const form = initialFormState(saved);
    expect(form.clientSecret).toBe("");
    expect(form.sandboxGroup).toBe("fabro-sandboxes");
    expect(form.enabled).toBe(false);
    expect(form.allowedImages).toBe("fabro-dev\nubuntu");
    expect(form.size).toBe("L");
  });

  test("a blank secret is left out so the saved one is kept", () => {
    const input = formToInput(initialFormState(saved));
    expect("client_secret" in input).toBe(false);
    expect(input.execution).toEqual(saved.settings!.execution);
  });

  test("a typed secret replaces the saved one", () => {
    const form = { ...initialFormState(saved), clientSecret: "new-secret" };
    expect(formToInput(form).client_secret).toBe("new-secret");
  });

  test("a new connection needs every detail and a secret", () => {
    const problems = formProblems(initialFormState(undefined), false);
    expect(problems).toContain("Tenant ID is required.");
    expect(problems).toContain("Sandbox group is required.");
    expect(problems).toContain("Client secret is required.");
  });

  test("a saved secret does not need typing again", () => {
    expect(formProblems(initialFormState(saved), true)).toEqual([]);
  });

  test("an image outside the allowed list is refused", () => {
    const form = { ...initialFormState(saved), image: "other" };
    expect(formProblems(form, true)).toEqual([
      "Image other is not in the list of allowed images.",
    ]);
  });

  test("sizes are named only when the server supports them", () => {
    expect(sizeName(1000, 2048)).toBe("M");
    expect(sizeName(3000, 2048)).toBeNull();
  });
});
