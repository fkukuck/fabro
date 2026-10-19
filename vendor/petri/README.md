# Vendored Petri crates

Two crates of [Petri](https://github.com/lithoscomputer/petri), the workflow
engine, are vendored here and patched into the build with `[patch]` in the
workspace `Cargo.toml`. Every other Petri crate still comes from the git
source pinned in `Cargo.lock`.

| Crate | Upstream path | Upstream commit |
|---|---|---|
| `petri-executor-sandbox` | `crates/core/executor-sandbox` | `56ec3849ca693974ff778aa5ad4f9f9410d47d29` |
| `petri-frontend-fabro` | `crates/fabro/frontend` | `56ec3849ca693974ff778aa5ad4f9f9410d47d29` |

The only changes from upstream let a run use the `aca` provider (Azure
Container Apps Sandboxes, `lib/components/fabro-sandbox-aca`). Each one is
marked `Fabro's patch` in the source:

- `executor-sandbox`: `SandboxBackend::Aca`, its routing to the `aca`
  in-process factory, `SandboxOptions::{aca_image, aca_resources}`,
  `build_aca_spec`, `ProviderNetwork::isolated`, and `aca` in
  `infer_host_address`.
- `frontend-fabro`: `aca` is an accepted environment provider, and its
  `resources` are lowered like Daytona's.

Upstream's tests are not vendored; they need Petri's test kit.

When moving Petri forward, re-copy both crates from the new commit, reapply
the patch, and update the table. Remove this directory and the `[patch]`
section once Petri routes `aca` itself.

Petri is MIT licensed; see `LICENSE`.
