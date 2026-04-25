import { afterEach, describe, expect, mock, test } from "bun:test";

import {
  ApiError,
  apiFetcher,
  apiNullableFetcher,
  apiPaginatedFetcher,
  apiRequest,
  extractRequestId,
} from "./api-client";

afterEach(() => {
  mock.restore();
  delete (globalThis as { window?: unknown }).window;
});

describe("apiRequest", () => {
  test("includes credentials on API requests", async () => {
    const fetchMock = mock(() => Promise.resolve(new Response("{}", { status: 200 })));
    globalThis.fetch = fetchMock as typeof fetch;

    await apiRequest("/api/v1/runs/run-1");

    expect(fetchMock).toHaveBeenCalledWith("/api/v1/runs/run-1", {
      credentials: "include",
      headers: undefined,
    });
  });

  test("401 responses throw a typed ApiError", async () => {
    const fetchMock = mock(() =>
      Promise.resolve(
        new Response(JSON.stringify({ errors: [{ detail: "Request ID: req_401" }] }), {
          status: 401,
          statusText: "Unauthorized",
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );
    globalThis.fetch = fetchMock as typeof fetch;

    await expect(apiRequest("/api/v1/auth/me")).rejects.toMatchObject({
      status: 401,
      requestId: "req_401",
    });
  });
});

describe("apiFetcher", () => {
  test("throws ApiError with status, body, and request id on non-2xx responses", async () => {
    const body = {
      errors: [{ status: "500", title: "Internal", request_id: "req_500" }],
    };
    const fetchMock = mock(() =>
      Promise.resolve(
        new Response(JSON.stringify(body), {
          status: 500,
          statusText: "Internal Server Error",
          headers: { "Content-Type": "application/json" },
        }),
      ),
    );
    globalThis.fetch = fetchMock as typeof fetch;

    try {
      await apiFetcher("/api/v1/runs/run-1/files");
      throw new Error("expected apiFetcher to reject");
    } catch (error) {
      expect(error).toBeInstanceOf(ApiError);
      expect(error).toMatchObject({
        status: 500,
        message: "Internal Server Error",
        requestId: "req_500",
        body,
      });
    }
  });
});

describe("apiNullableFetcher", () => {
  test("returns null only for explicit availability statuses", async () => {
    const fetchMock = mock(() => Promise.resolve(new Response("", { status: 501 })));
    globalThis.fetch = fetchMock as typeof fetch;

    await expect(apiNullableFetcher("/api/v1/runs/run-1/files")).resolves.toBeNull();
  });
});

describe("apiPaginatedFetcher", () => {
  test("preserves first-page extras and stops at the page cap", async () => {
    const warnMock = mock(() => {});
    const originalWarn = console.warn;
    console.warn = warnMock;
    let calls = 0;
    const fetchMock = mock(() => {
      calls += 1;
      return Promise.resolve(
        new Response(
          JSON.stringify({
            columns: [{ id: "running", name: "Running" }],
            data: [{ id: `run-${calls}` }],
            meta: { has_more: true },
          }),
          {
            status: 200,
            headers: { "Content-Type": "application/json" },
          },
        ),
      );
    });
    globalThis.fetch = fetchMock as typeof fetch;

    try {
      const result = await apiPaginatedFetcher<{ id: string }, { columns: { id: string; name: string }[] }>(
        "/api/v1/boards/runs",
      );

      expect(result.columns).toEqual([{ id: "running", name: "Running" }]);
      expect(result.data).toHaveLength(50);
      expect(result.meta.has_more).toBe(true);
      expect(warnMock).toHaveBeenCalledTimes(1);
    } finally {
      console.warn = originalWarn;
    }
  });
});

describe("extractRequestId", () => {
  test("supports top-level, error-level, and detail-embedded request ids", () => {
    expect(extractRequestId({ request_id: "top" })).toBe("top");
    expect(extractRequestId({ errors: [{ request_id: "nested" }] })).toBe("nested");
    expect(extractRequestId({ errors: [{ detail: "Request ID: req-detail" }] })).toBe("req-detail");
  });
});
