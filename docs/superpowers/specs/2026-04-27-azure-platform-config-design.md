# Azure Platform Config Design

## Summary

Fix the Azure sandbox regression introduced by worker env hardening by removing Azure runtime dependence on worker process environment variables.

Fabro will treat Azure platform setup as operator-managed server configuration collected during `fabro install`. Non-secret Azure platform values will live in structured server settings. Sensitive Azure registry credentials will live in the vault as environment-style secrets. The server will resolve a typed Azure platform config once and hand it to worker-side Azure runtime code through a runtime snapshot file under server storage instead of ambient env inheritance.

## Problem

Azure sandbox creation and reconnect currently call `AzurePlatformConfig::from_env()`. That worked before worker subprocess env hardening because workers inherited the parent process env.

After `spawn_env.rs` introduced `env_clear()` plus a fail-closed allowlist, worker subprocesses no longer receive `FABRO_AZURE_*`. Server preflight can still see Azure env, but worker execution cannot, which creates a split between validation and runtime behavior.

The current shape also does not match Fabro's preferred server model:

- operator-managed setup should be persisted through install
- subprocesses should receive explicit inputs, not broad env inheritance
- non-secret infrastructure coordinates should not be modeled as ad hoc env-only runtime state

## Goals

- Remove Azure runtime dependence on worker process env.
- Make Azure platform setup fit the `fabro install` flow.
- Keep worker env hardening intact.
- Keep validation and runtime resolution aligned.
- Store non-secret Azure platform values as structured config.
- Store sensitive Azure auth material using the best existing secret-storage fit.

## Non-Goals

- Reintroduce broad worker env inheritance.
- Keep legacy Azure storage env vars. `FABRO_AZURE_STORAGE_ACCOUNT`, `FABRO_AZURE_STORAGE_SHARE`, and `FABRO_AZURE_STORAGE_KEY` are obsolete because Azure sandboxes now use ephemeral storage.
- Move Azure ACR credentials into the LLM credential system.
- Redesign unrelated sandbox providers.

## Chosen Approach

### Structured settings for platform values

Add structured Azure platform settings under server-owned config.

Proposed shape:

```toml
[server.sandbox.azure.platform]
subscription_id = "sub-123"
resource_group = "rg-prod"
location = "eastus"
subnet_id = "/subscriptions/.../subnets/aci"
acr_server = "fabro.azurecr.io"
sandboxd_port = 7777
```

These values are operator-managed infrastructure coordinates, not secrets.

### Vault environment-style secrets for ACR auth

Persist Azure registry credentials in the vault as environment-style secrets, matching the `GITHUB_TOKEN` / `DAYTONA_API_KEY` style rather than the LLM credential-object style.

Secret names:

- `FABRO_AZURE_ACR_USERNAME`
- `FABRO_AZURE_ACR_PASSWORD`

These secrets are not model-provider credentials and do not belong in the `CredentialSource` abstraction.

### Explicit runtime handoff

The Azure runtime must stop reading platform config from process env inside worker-owned code paths.

Instead:

- the server resolves a typed `AzurePlatformConfig`
- Fabro writes that resolved config to a runtime snapshot file under server storage
- worker-side Azure sandbox creation and reconnect load that snapshot explicitly

This preserves the current fail-closed subprocess env model.

## Data Model

### Structured config

Add server-side settings for:

- `subscription_id: String`
- `resource_group: String`
- `location: String`
- `subnet_id: String`
- `acr_server: String`
- `sandboxd_port: u16` with default `7777`

### Secret storage

Store optional secrets for:

- `FABRO_AZURE_ACR_USERNAME`
- `FABRO_AZURE_ACR_PASSWORD`

### Resolved runtime type

Keep `AzurePlatformConfig` as the runtime type consumed by `fabro-sandbox`, but change its source so it is no longer built from direct process env reads in runtime code.

The resolved runtime type continues to contain:

- `subscription_id`
- `resource_group`
- `location`
- `subnet_id`
- `acr_server`
- `sandboxd_port`
- `acr_username`
- `acr_password`

## Resolution Semantics

Create one canonical Azure platform resolver owned by the server-side configuration boundary.

Inputs:

- structured server settings for non-secret values
- vault lookup for ACR secrets

Outputs:

- `AzurePlatformConfig`
- a validation error that points to missing install/config state

Rules:

- preflight and runtime must use the same resolver or the same resolved result
- worker-side Azure code must not independently rediscover config from env
- missing structured config is a configuration error
- missing ACR credentials are allowed when both are absent
- providing exactly one of `acr_username` or `acr_password` is a configuration error

## Install Flow

Extend `fabro install` with an Azure setup step when Azure sandboxing is selected or configured.

Install collects:

- subscription ID
- resource group
- Azure location
- sandbox subnet ID
- ACR server
- optional sandboxd port override
- optional ACR username
- optional ACR password

Install persistence:

- writes non-secret Azure platform values into `settings.toml`
- writes ACR credentials into the vault as environment-style secrets

Install should not write obsolete Azure storage variables.

## Worker and Reconnect Runtime

Azure runtime construction paths must stop calling `AzurePlatformConfig::from_env()`.

Required changes in behavior:

- server startup or run initialization writes a resolved Azure platform snapshot file under the server runtime directory
- `AzureSandbox::new(...)` loads that snapshot through explicit Fabro plumbing
- `AzureSandbox::reconnect(...)` loads the same snapshot through explicit Fabro plumbing
- `spawn_env.rs` remains fail-closed and does not add Azure env vars back to the worker allowlist

The runtime snapshot file is the single chosen handoff mechanism for this design.

## Error Handling

User-facing errors should move from shell-env wording toward persisted-config wording.

Examples:

- instead of `missing required Azure environment variables: ...`
- use messages that point to Azure platform config being incomplete and recommend completing or updating `fabro install`

Errors should distinguish between:

- missing structured Azure platform settings
- invalid `sandboxd_port`
- missing optional vs required ACR credentials when relevant

## Migration

For this change, Azure process env is no longer the supported steady-state runtime source for worker execution.

Migration path:

- operators run `fabro install` to persist Azure platform config
- runtime paths consume persisted config instead of ambient env

Compatibility fallback to worker env inheritance is intentionally not added.

An optional short-term server-side fallback from process env into install-time defaults may be considered only if needed for operator ergonomics, but it is not part of the core design because it keeps the old implicit model alive.

## Testing

### Unit tests

- resolver builds `AzurePlatformConfig` from structured settings plus vault secrets
- resolver fails clearly when required structured fields are missing
- resolver defaults `sandboxd_port` to `7777`
- resolver rejects partial ACR auth config when only one credential is set
- obsolete Azure storage vars are ignored by the new path

### Server tests

- install persists Azure structured config to `settings.toml`
- install persists ACR credentials to the vault with the expected secret names and types
- runtime initialization writes the Azure platform snapshot file
- preflight Azure validation uses persisted config, not process env
- worker command tests continue proving Azure vars are not reintroduced into worker env

### Sandbox tests

- Azure sandbox creation works with explicit resolved config and no process env dependency
- Azure reconnect works with explicit resolved config and no process env dependency

## Why This Design

This design matches Fabro's current architecture better than patching the allowlist or keeping Azure env-based runtime discovery.

It keeps subprocess hardening intact, gives Azure a first-class install-time home, separates config from secrets cleanly, and removes the specific regression vector created by rebasing onto the hardened worker env model.
