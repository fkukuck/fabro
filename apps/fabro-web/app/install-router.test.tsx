import { describe, expect, test } from "bun:test";
import { matchRoutes } from "react-router";

import InstallApp from "./install-app";
import { installRoutes } from "./install-router";

function matchedComponents(path: string) {
  return matchRoutes(installRoutes, path)?.map((match) => match.route.Component) ?? [];
}

describe("install router", () => {
  test("mounts the install app for the root and install paths", () => {
    expect(matchedComponents("/")).toContain(InstallApp);
    expect(matchedComponents("/install")).toContain(InstallApp);
    expect(matchedComponents("/install/welcome")).toContain(InstallApp);
  });
});
