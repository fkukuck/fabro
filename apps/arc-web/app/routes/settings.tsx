import type { Route } from "./+types/settings";

export function meta({}: Route.MetaArgs) {
  return [{ title: "Settings — Arc" }];
}

export default function Settings() {
  return null;
}
