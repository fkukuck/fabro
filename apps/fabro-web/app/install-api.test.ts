import { describe, expect, test } from "bun:test";

import {
  putInstallAzure,
  putInstallObjectStore,
  putInstallSandbox,
  readInstallError,
  testInstallObjectStore,
  testInstallSandbox,
} from "./install-api";

describe("readInstallError", () => {
  test("prefers the structured install error payload", async () => {
    const response = new Response(
      JSON.stringify({
        errors: [{ status: "422", title: "Unprocessable Entity", detail: "invalid token" }],
      }),
      {
        status: 422,
        headers: { "Content-Type": "application/json" },
      },
    );

    await expect(
      readInstallError(response, "install request failed"),
    ).resolves.toBe("invalid token");
  });

  test("falls back to the provided message when the body is not structured JSON", async () => {
    const response = new Response("boom", {
      status: 500,
      headers: { "Content-Type": "text/plain" },
    });

    await expect(
      readInstallError(response, "install request failed"),
    ).resolves.toBe("install request failed (500)");
  });
});

describe("install object-store requests", () => {
  test("testInstallObjectStore posts the install payload to the validation endpoint", async () => {
    const calls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
    globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ input, init });
      return Promise.resolve(new Response(JSON.stringify({ ok: true }), { status: 200 }));
    }) as typeof fetch;

    await testInstallObjectStore("test-install-token", {
      provider: "s3",
      bucket: "fabro-data",
      region: "us-east-1",
      credential_mode: "runtime",
    });

    expect(calls).toHaveLength(1);
    expect(String(calls[0]!.input)).toBe("/install/object-store/test");
    expect(calls[0]!.init?.method).toBe("POST");
    expect(calls[0]!.init?.headers).toEqual({
      Authorization: "Bearer test-install-token",
      "Content-Type": "application/json",
    });
    expect(calls[0]!.init?.body).toBe(
      JSON.stringify({
        provider: "s3",
        bucket: "fabro-data",
        region: "us-east-1",
        credential_mode: "runtime",
      }),
    );
  });

  test("putInstallObjectStore surfaces structured API errors", async () => {
    globalThis.fetch = (() =>
      Promise.resolve(
        new Response(
          JSON.stringify({
            errors: [
              {
                status: "422",
                title: "Unprocessable Entity",
                detail: "Bucket is required.",
              },
            ],
          }),
          {
            status: 422,
            headers: { "Content-Type": "application/json" },
          },
        ),
      )) as typeof fetch;

    await expect(
      putInstallObjectStore("test-install-token", { provider: "s3" }),
    ).rejects.toThrow("Bucket is required.");
  });
});

describe("install sandbox requests", () => {
  test("testInstallSandbox posts the install payload to the validation endpoint", async () => {
    const calls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
    globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ input, init });
      return Promise.resolve(new Response(JSON.stringify({ ok: true }), { status: 200 }));
    }) as typeof fetch;

    await testInstallSandbox("test-install-token", {
      provider: "daytona",
      api_key:  "dtn_test",
    });

    expect(calls).toHaveLength(1);
    expect(String(calls[0]!.input)).toBe("/install/sandbox/test");
    expect(calls[0]!.init?.method).toBe("POST");
    expect(calls[0]!.init?.body).toBe(
      JSON.stringify({ provider: "daytona", api_key: "dtn_test" }),
    );
  });

  test("putInstallSandbox surfaces structured API errors", async () => {
    globalThis.fetch = (() =>
      Promise.resolve(
        new Response(
          JSON.stringify({
            errors: [
              {
                status: "422",
                title: "Unprocessable Entity",
                detail: "api_key is required for daytona",
              },
            ],
          }),
          { status: 422, headers: { "Content-Type": "application/json" } },
        ),
      )) as typeof fetch;

    await expect(
      putInstallSandbox("test-install-token", { provider: "daytona" }),
    ).rejects.toThrow("api_key is required for daytona");
  });
});

describe("install azure requests", () => {
  test("putInstallAzure posts the Azure config to /install/azure", async () => {
    const calls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
    globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ input, init });
      return Promise.resolve(new Response(null, { status: 204 }));
    }) as typeof fetch;

    await putInstallAzure("token-1", {
      subscription_id: "sub-1",
      resource_group: "rg-1",
      location: "eastus",
      subnet_id: "/subscriptions/sub-1/.../aci",
      acr_server: "fabro.azurecr.io",
      acr_identity_resource_id:
        "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/fabro-acr",
      sandboxd_port: 7777,
    });

    expect(String(calls[0]!.input)).toBe("/install/azure");
    expect(calls[0]!.init?.method).toBe("PUT");
  });
});
