import { describe, expect, test } from "bun:test";

import { extractRequestId } from "./run-files";

describe("extractRequestId", () => {
  test("reads `request_id` from the top level of the error body", () => {
    expect(extractRequestId({ request_id: "abc-123" })).toBe("abc-123");
  });

  test("reads `request_id` from errors[0] under the uniform envelope", () => {
    expect(
      extractRequestId({
        errors: [
          { status: "500", title: "Internal", request_id: "evt_42" },
        ],
      }),
    ).toBe("evt_42");
  });

  test("parses `Request ID: xyz` out of errors[0].detail", () => {
    expect(
      extractRequestId({
        errors: [
          {
            status: "500",
            title:  "Internal Server Error",
            detail: "Run files failed. Request ID: req_999 on shard 2.",
          },
        ],
      }),
    ).toBe("req_999");
  });

  test("returns null for bodies without any request_id", () => {
    expect(extractRequestId(null)).toBe(null);
    expect(extractRequestId(undefined)).toBe(null);
    expect(extractRequestId("not an object")).toBe(null);
    expect(extractRequestId({ errors: [] })).toBe(null);
    expect(extractRequestId({ errors: [{ detail: "no id in here" }] })).toBe(
      null,
    );
  });

  test("handles request_id values with hyphens and underscores", () => {
    expect(
      extractRequestId({
        errors: [
          { detail: "Something failed. request_id: RX-1A_2B-3C4D" },
        ],
      }),
    ).toBe("RX-1A_2B-3C4D");
  });
});
