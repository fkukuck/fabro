import type { Route } from "./+types/start";

export function meta({}: Route.MetaArgs) {
  return [{ title: "Start — Arc" }];
}

export default function Start() {
  return null;
}
