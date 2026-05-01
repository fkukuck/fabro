import type { WorkflowSettings } from "@qltysh/fabro-api-client";

import { getObject } from "./unknown";

export type Snapshot = WorkflowSettings;

export {
  getArray,
  getBool,
  getNumber,
  getObject,
  getString,
  isRecord,
  type UnknownRecord,
} from "./unknown";

export function objectKeyCount(o: unknown, key: string): number {
  const v = getObject(o, key);
  return v ? Object.keys(v).length : 0;
}
