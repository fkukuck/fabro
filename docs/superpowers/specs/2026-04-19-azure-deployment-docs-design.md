# Azure Deployment Docs Design

## Summary

Promote Azure hosting from a branch-specific operator runbook to a first-class Fabro deployment guide that matches the structure and tone of the existing deployment documentation.

The new Azure deployment story should keep `docs/administration/deploy-server.mdx` provider-agnostic, introduce a new `docs/administration/deploy-azure.mdx` page for the Azure-specific workflow, add Azure to the deployment navigation, migrate the useful operator knowledge out of `docs/administration/azure-hosting.md`, and remove that legacy page once the new structure fully covers it.

## Goals

- Document Azure as a first-class Fabro deployment target in the same part of the docs as Railway, Render, Fly.io, and DigitalOcean.
- Keep the generic server story in `docs/administration/deploy-server.mdx` intact.
- Present one coherent Azure deployment narrative instead of splitting setup across smoke-test and validation histories.
- Include concrete, executable `az` CLI commands for the Azure provisioning and deployment flow, in the same practical style that other deployment guides use for their platforms.
- Align the Azure docs with the real runtime architecture already implemented in the codebase:
  - `fabro-server` as the control plane
  - Azure Container Apps for the hosted server
  - Azure Container Instances for workflow sandboxes
  - Azure Files-backed sandbox workspace state
  - Azure Container Registry-hosted sandbox images
- Preserve the important operational constraints and caveats that are currently only documented in `docs/administration/azure-hosting.md`.

## Non-Goals

- Redesign the Azure sandbox abstraction or the server/runtime interfaces.
- Add a new Azure-specific runtime model distinct from the current `SandboxSpec::Azure` path.
- Turn `docs/administration/deploy-server.mdx` into an Azure runbook.
- Keep branch-history notes, commit references, or validation chronology in user-facing deployment docs.

## Current State

### Existing Deployment Docs

The current deployment docs follow a clear split:

- `docs/administration/deploy-server.mdx` explains server mode generically.
- `docs/administration/deploy-railway.mdx` is a provider-specific deployment guide.
- `docs/docs.json` exposes supported deploy guides in the Deployment section.

This structure makes Fabro's deployment surface easy to understand:

- one generic page for what the server is
- one provider-specific page per hosting target
- a shared configuration reference in `docs/administration/server-configuration.mdx`

### Existing Azure Material

`docs/administration/azure-hosting.md` currently mixes multiple concerns in one document:

- control-plane assumptions
- required Azure environment variables
- a greenfield smoke-test path
- a server-hosted validation path
- historical validation notes tied to branch state
- Azure-specific warnings and caveats

This content is useful, but its shape does not match the rest of the deployment documentation. It reads like an engineering validation log rather than a product deployment guide.

### Existing Runtime Alignment

The Azure code path already uses the normal Fabro seams:

- `lib/crates/fabro-workflow/src/operations/start.rs` resolves Azure run settings into sandbox runtime config.
- `lib/crates/fabro-server/src/run_manifest.rs` carries Azure runtime settings through the server path.
- `lib/crates/fabro-sandbox/src/config.rs` defines the Azure runtime config consumed by the sandbox provider.
- `lib/crates/fabro-sandbox/src/azure/` implements the provider and Azure ARM payloads.

This means the main gap is not architectural mismatch in the runtime code. The gap is that Azure is not yet documented and packaged like an established Fabro deployment target.

## Recommended Structure

### Keep `deploy-server.mdx` Generic

`docs/administration/deploy-server.mdx` should remain the provider-agnostic explanation of:

- direct CLI runs vs. server mode
- auth model
- web UI
- run lifecycle
- event streaming
- pointing the CLI at a server

The only Azure-related change to this page should be a small navigation addition under `## Next steps`, such as an Azure deployment card.

### Add `deploy-azure.mdx`

Add `docs/administration/deploy-azure.mdx` as the Azure counterpart to `deploy-railway.mdx`.

This page should be the canonical user-facing Azure deployment guide.

### Keep `server-configuration.mdx` Normative

`docs/administration/server-configuration.mdx` should remain the reference page for:

- `settings.toml` sections
- auth configuration
- storage configuration
- server/runtime secret semantics

It should not absorb a long Azure provisioning walkthrough.

### Remove `azure-hosting.md` After Migration

Once the new Azure page fully covers the operator-relevant content, remove `docs/administration/azure-hosting.md`.

## Proposed Azure Page Narrative

The new Azure page should tell one coherent deployment story from infrastructure creation to first successful run.

### 1. Overview

Explain what Azure deployment means in Fabro:

- `fabro-server` runs as a singleton control plane in Azure Container Apps.
- Workflow sandboxes run as Azure Container Instances.
- Azure Files backs sandbox workspace state at `/workspace`.
- Fabro server state persists under `/storage` on Azure-backed storage.
- Azure Container Registry stores the sandbox images used by Azure runs.

This should make the control-plane and sandbox-plane split explicit up front.

### 2. Reference Topology

Describe the Azure resource model required for a production-ready deployment:

- resource group
- virtual network
- subnet for Azure Container Apps environment
- delegated subnet for Azure Container Instances
- storage account
- Azure Files share for sandbox workspace
- storage path or share for Fabro server state
- Azure Container Registry
- managed identity and role assignments
- Azure Container Apps service hosting `fabro-server`

This section should explain why Container Apps and ACI are both needed.

### 3. First-Deploy Checklist

Provide a concise checklist in the style of the other deployment guides:

- provision persistent storage for `/storage`
- provision Azure Files for `/workspace`
- create ACR and push required sandbox images
- configure Azure-managed identity/permissions
- set server auth and runtime secrets
- deploy `fabro-server`
- verify the CLI can connect

The checklist should be followed by concrete command-driven sections, not left at the conceptual level.

### 4. Azure Resource Creation

Describe the Azure provisioning flow in a stable operator-oriented order:

- create resource group
- create virtual network and subnets
- create storage account and shares
- create ACR
- create or assign managed identity
- grant permissions required by the current implementation

This section should contain concrete `az` commands that an operator can run in order, not pseudocode or abstract descriptions. It should read more like the DigitalOcean guide's executable setup flow and less like a conceptual architecture note.

Where Azure requires values to be captured and reused later, the guide should show the exact `export` pattern for those values.

### 5. Deploy `fabro-server` To Azure Container Apps

This should explain how to host the existing Fabro server container on Container Apps using the repository's standard server packaging.

It should document:

- singleton replica requirement
- scale-to-zero disabled
- managed identity enabled
- bind/API URL/web configuration as needed
- persistent `/storage` mount
- required secrets and environment variables

This section should also include the actual `az containerapp ...` commands or equivalent Azure CLI flow needed to create the Container Apps environment, configure the app, mount storage, assign identity, and set environment variables.

### 6. Configure Server Settings And Secrets

This section should show the minimal server-side configuration shape needed for Azure-hosted production use, including:

- `[run.sandbox] provider = "azure"`
- `[server.auth]`
- `[server.integrations.github] strategy = "token"` when using token-based GitHub access
- required runtime secrets like `FABRO_DEV_TOKEN`, `SESSION_SECRET`, and LLM keys
- Azure-specific runtime environment variables required by the Azure sandbox provider

It should explicitly distinguish:

- server-owned local settings
- runtime secrets
- workflow-owned Azure run settings like `run.sandbox.azure.image`

Where feasible, this section should pair config snippets with the exact Azure CLI commands used to store or inject those values into the deployment.

### 7. Sandbox Execution Plane

Document the Azure runtime prerequisites needed for remote workflows to succeed:

- required `FABRO_AZURE_*` environment variables
- expected Azure Files-backed `/workspace`
- ACR-hosted sandbox image references
- workflow-specific image extension from the Fabro Azure base image

This section should explain the relationship between:

- the base Azure sandbox image
- workflow-specific custom images
- `fabro-sandboxd`

### 8. First Remote Run

Document the expected checked-in workflow pattern for Azure-hosted runs:

- `run.sandbox.provider = "azure"`
- explicit `run.sandbox.azure.image`
- optional GitHub permission requests via `run.scm.github.permissions`

Then show:

- how to point the local CLI at the hosted server
- how to authenticate
- how to submit a remote run

This should include the exact shell commands an operator runs locally after the Azure deployment is live.

### 9. Validation

Include a short production-oriented validation sequence:

1. server health responds
2. dev-token auth works
3. CLI can talk to the hosted server
4. a minimal Azure-backed workflow run succeeds
5. a custom ACR-hosted workflow image succeeds when required

This section should verify the deployment as operators will actually use it.

### 10. Troubleshooting And Caveats

Move the existing Azure-specific operational gotchas into a dedicated section.

Stable caveats to preserve:

- Azure Container Instance names must be lowercase.
- `fabro-server` should remain single-replica for now.
- stale ACI groups can exhaust quota and block future runs.
- remote Azure runs should set `run.sandbox.azure.image` explicitly.
- workflow-specific images must inherit from the Fabro Azure base image so `fabro-sandboxd` remains present.

Branch-history notes, commit references, and validation chronology should be removed.

## Documentation Boundaries

### `docs/administration/deploy-server.mdx`

Keep generic. Only add a small Azure link/card in `## Next steps`.

### `docs/administration/deploy-azure.mdx`

Own the full Azure hosting and operations narrative.

### `docs/administration/server-configuration.mdx`

Remain the normative configuration and secret reference. Do not duplicate the Azure runbook there.

### `docs/docs.json`

Add the Azure page to the Deployment group so it is discoverable like the other hosting targets.

### `docs/administration/azure-hosting.md`

Delete after its useful material is migrated.

## Implementation Scope

The implementation work implied by this design is primarily documentation restructuring and packaging alignment, not runtime redesign.

Expected edits:

- create `docs/administration/deploy-azure.mdx`
- update `docs/docs.json`
- make a small Azure card/link change in `docs/administration/deploy-server.mdx`
- remove `docs/administration/azure-hosting.md`

The new Azure page should include concrete Azure CLI command sequences for provisioning and deployment, not just descriptive guidance.

Potential supporting doc adjustments are acceptable only when needed for accurate cross-links, but the work should stay tightly scoped.

## Acceptance Criteria

- Azure appears in the Deployment navigation alongside the other supported deployment guides.
- `deploy-server.mdx` remains generic, with only a minimal Azure navigation addition.
- `deploy-azure.mdx` is the canonical Azure deployment guide and reads like a first-class Fabro deployment page.
- `deploy-azure.mdx` includes concrete, ordered `az` CLI commands for creating the required Azure resources and deploying the service.
- The important operator knowledge from `azure-hosting.md` is preserved in the new structure.
- `azure-hosting.md` is removed.
- The Azure documentation no longer contains branch-history validation notes or commit-based narrative.
- The Azure guide clearly explains both the hosting control plane and the sandbox execution plane.

## Risks And Mitigations

### Risk: Over-documenting branch-specific behavior

Mitigation: convert only stable operational facts into the new guide, and remove commit-history framing.

### Risk: Duplicating config reference material

Mitigation: keep `server-configuration.mdx` as the normative source for server settings and use concise references from the Azure page.

### Risk: Making Azure look simpler than it is

Mitigation: keep the doc production-oriented and explicit about required Azure resources, identity, storage, and quota caveats.

## Recommendation

Implement Azure as a first-class documentation target with a full Azure deployment guide, while preserving the existing runtime architecture and keeping the generic server documentation mostly untouched.
