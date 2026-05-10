import { afterEach, describe, expect, mock, test } from "bun:test";
import TestRenderer, { act } from "react-test-renderer";
import { MemoryRouter, Route, Routes } from "react-router";

import type { SandboxDetails } from "@qltysh/fabro-api-client";

let currentDetails: SandboxDetails | null = null;
let currentLoading = false;
let currentError: Error | null = null;

mock.module("../lib/queries", () => ({
  useRunSandboxDetails: () => ({
    data:         currentDetails,
    error:        currentError,
    isLoading:    currentLoading,
    isValidating: false,
    mutate:       mock(() => Promise.resolve(currentDetails)),
  }),
}));

mock.module("../components/terminal-view", () => ({
  default: () => null,
  TERMINAL_DOCK_CLEARANCE_CLASS: "",
}));

const { default: RunSandbox, formatBytesAsMemory } = await import("./run-sandbox");
mock.restore();

const mountedRenderers: TestRenderer.ReactTestRenderer[] = [];

function renderRoute() {
  let renderer!: TestRenderer.ReactTestRenderer;
  act(() => {
    renderer = TestRenderer.create(
      <MemoryRouter initialEntries={["/runs/run_1/sandbox"]}>
        <Routes>
          <Route path="/runs/:id/sandbox" element={<RunSandbox params={{ id: "run_1" }} />} />
        </Routes>
      </MemoryRouter>,
    );
  });
  mountedRenderers.push(renderer);
  return renderer;
}

afterEach(() => {
  for (const renderer of mountedRenderers.splice(0)) {
    act(() => renderer.unmount());
  }
  currentDetails = null;
  currentLoading = false;
  currentError = null;
});

describe("formatBytesAsMemory", () => {
  test("renders gibibytes for round values", () => {
    expect(formatBytesAsMemory(2 * 1024 * 1024 * 1024)).toBe("2 GiB");
  });

  test("renders fractional gibibytes with one decimal", () => {
    expect(formatBytesAsMemory(2.5 * 1024 * 1024 * 1024)).toBe("2.5 GiB");
  });

  test("falls back to mebibytes when below a gibibyte", () => {
    expect(formatBytesAsMemory(512 * 1024 * 1024)).toBe("512 MiB");
  });
});

describe("RunSandbox route", () => {
  test("renders panels for a fully populated sandbox", () => {
    currentDetails = {
      provider:     "docker",
      name:         "fabro-run-abc",
      id:           "abcdef123456",
      state:        "running",
      native_state: "running",
      region:       null,
      image:        "ghcr.io/fabro/sandbox:latest",
      resources:    {
        cpu_cores:    2,
        memory_bytes: 4 * 1024 * 1024 * 1024,
        disk_bytes:   null,
      },
      labels: { run: "abc" },
      timestamps: {
        created_at:       "2026-05-09T12:00:00Z",
        last_activity_at: null,
      },
    };
    const renderer = renderRoute();

    const panelHeadings = renderer.root
      .findAll((node) => node.type === "h3")
      .map((node) => node.children.find((child) => typeof child === "string"))
      .filter((text): text is string => typeof text === "string");
    expect(panelHeadings).toEqual(["Overview", "Resources", "Labels", "Timestamps"]);
  });

  test("renders without crashing when most fields are null", () => {
    currentDetails = {
      provider:     "local",
      name:         null,
      id:           null,
      state:        "unknown",
      native_state: null,
      region:       null,
      image:        null,
      resources:    {
        cpu_cores:    null,
        memory_bytes: null,
        disk_bytes:   null,
      },
      labels: {},
      timestamps: {
        created_at:       null,
        last_activity_at: null,
      },
    };
    const renderer = renderRoute();

    const labelsHeading = renderer.root.findAll(
      (node) =>
        node.type === "h3" &&
        node.children.find((child) => typeof child === "string") === "Labels",
    );
    expect(labelsHeading).toHaveLength(1);

    const noLabelsCopy = renderer.root.findAll(
      (node) =>
        node.type === "div" &&
        Array.isArray(node.children) &&
        node.children.includes("No labels"),
    );
    expect(noLabelsCopy).toHaveLength(1);
  });

  test("shows the empty state when no sandbox is reported", () => {
    currentDetails = null;
    const renderer = renderRoute();

    const titles = renderer.root.findAll(
      (node) =>
        node.type === "p" &&
        Array.isArray(node.children) &&
        node.children.includes("No sandbox"),
    );
    expect(titles).toHaveLength(1);
  });
});
