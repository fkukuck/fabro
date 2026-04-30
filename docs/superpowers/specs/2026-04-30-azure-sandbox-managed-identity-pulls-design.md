# Azure sandbox managed-identity ACR pulls

**Status:** Draft for review
**Date:** 2026-04-30
**Owner:** Bryan (with OpenCode)

## Problem

The Azure deployment now uses managed identity for the long-running `fabro-server` Container App image pull, but Azure sandbox container groups still use a different and older auth model.

Today:

- `fabro-server` in Azure Container Apps pulls its image from private ACR via managed identity.
- Azure sandboxes are created as Azure Container Instances (ACI) by Fabro at runtime.
- Fabro only sends `imageRegistryCredentials` for an ACI sandbox when `acr_username` and `acr_password` exist.
- The deploy smoke workflow rewrites the sandbox image to the private ACR-hosted `fabro-azure-sandbox-base:<deploy-id>` image.

That leaves a mismatch:

- the control plane is managed-identity-based
- the sandbox image pull path is still static-credential-based

Under a managed-identity-only Azure deployment, a fresh environment with no stored ACR username/password cannot reliably launch Azure sandboxes from private ACR images.

## Goals

- Make Azure sandbox image pulls work from private ACR without stored registry username/password.
- Reuse the existing Terraform-managed user-assigned identity instead of introducing a second identity.
- Keep the Azure deployment fully managed-identity-based for both server and sandbox image pulls.
- Persist only non-secret identity metadata in Fabro configuration.
- Fail closed when the Azure sandbox identity configuration is missing or invalid.
- Keep the smoke workflow able to validate the private ACR-hosted shared sandbox base image.

## Non-goals

- Supporting both managed identity and static ACR credentials for Azure sandbox pulls in the Azure deployment path.
- Creating a second sandbox-specific identity.
- Making ACR admin credentials part of the preferred or documented Azure path.
- Redesigning the broader install flow outside the Azure sandbox auth path.

## Decisions

1. Azure sandbox pulls will use managed identity only.
2. Fabro will reuse the existing Terraform-managed user-assigned identity already attached to `fabro-server`.
3. Fabro will persist the identity resource ID, not a secret, in the Azure platform config.
4. ACI create requests will attach the user-assigned identity at the container-group level and reference the same identity in the registry credential entry.
5. The install flow will require `acr_identity_resource_id` for Azure sandbox configuration.
6. The runtime will reject Azure sandbox config that lacks `acr_identity_resource_id`.
7. The old Azure sandbox username/password path will be removed from the supported Azure deployment path rather than kept as a silent fallback.

## Architecture

The Azure sandbox image-pull path will use one identity end-to-end:

1. Terraform creates and owns the user-assigned managed identity.
2. Terraform grants that identity `AcrPull` on the target ACR.
3. Terraform exposes the identity resource ID as an environment output for operators.
4. The install flow persists that identity resource ID in `settings.toml` under the Azure sandbox platform config.
5. At runtime, Fabro resolves the Azure platform config into `AzurePlatformConfig` with the identity resource ID.
6. When Fabro creates an ACI sandbox, it sends:
   - a top-level ACI `identity` block for the user-assigned identity
   - an `imageRegistryCredentials` entry containing `server` plus `identity`
7. ACI uses that identity to pull the private image from ACR.

This keeps the server image-pull path and the sandbox image-pull path aligned on the same trust model.

## Configuration model

Fabro will extend the Azure sandbox platform config shape with one new required field:

```toml
[server.sandbox.azure.platform]
subscription_id = "sub-1"
resource_group = "rg-1"
location = "eastus"
subnet_id = "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci"
acr_server = "fabro.azurecr.io"
acr_identity_resource_id = "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/fabro-server"
sandboxd_port = 7777
```

Properties:

- `acr_identity_resource_id` is non-secret configuration.
- It is required whenever Azure sandbox support is configured.
- It replaces the Azure sandbox-specific use of `FABRO_AZURE_ACR_USERNAME` and `FABRO_AZURE_ACR_PASSWORD`.

The supported Azure deployment path will no longer rely on runtime registry credentials for ACI image pulls.

## API and install model

The install-time Azure payload and summary will add `acr_identity_resource_id`.

Requested shape:

```json
{
  "subscription_id": "sub-1",
  "resource_group": "rg-1",
  "location": "eastus",
  "subnet_id": "/subscriptions/sub-1/.../aci",
  "acr_server": "fabro.azurecr.io",
  "acr_identity_resource_id": "/subscriptions/sub-1/.../userAssignedIdentities/fabro-server",
  "sandboxd_port": 7777
}
```

Install behavior:

- The browser wizard Azure step will require the identity resource ID.
- The server-side Azure install validation will reject empty or missing `acr_identity_resource_id`.
- The Azure review/summary step will show the configured identity resource ID.

The existing Azure install fields `acr_username` and `acr_password` will be removed from the supported Azure flow for sandbox pulls.

## Runtime resolution model

`resolve_azure_platform_config()` will:

- read `acr_identity_resource_id` from persisted Azure platform settings
- build `AzurePlatformConfig` with that value
- stop resolving Azure sandbox ACR username/password from vault secrets for image pulls

Failure behavior:

- if Azure sandbox config exists but the identity resource ID is missing, startup or config resolution fails with a clear operator-facing error
- there is no fallback to secret-based registry auth in this Azure deployment path

## Azure Container Instances request model

Fabro's ACI create request body will add two identity-specific pieces.

### 1. Top-level container-group identity

```json
"identity": {
  "type": "UserAssigned",
  "userAssignedIdentities": {
    "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/fabro-server": {}
  }
}
```

### 2. Registry credential by identity

```json
"imageRegistryCredentials": [
  {
    "server": "fabro.azurecr.io",
    "identity": "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/fabro-server"
  }
]
```

Fabro will no longer generate the Azure sandbox registry credential payload from username/password in the managed-identity Azure path.

## Terraform and RBAC model

Terraform already owns the user-assigned identity and grants it `AcrPull` on the ACR.

This follow-up must also ensure the running server can assign that identity to created ACI container groups.

Required Terraform support:

1. Expose the identity resource ID from the Azure environment outputs.
2. Ensure the server identity can attach that identity to new container groups.

The expected RBAC addition is:

- `Managed Identity Operator` on the identity resource for the same principal that runs `fabro-server`

This keeps the identity reusable without introducing another identity lifecycle.

## Documentation model

The Azure docs will shift from optional ACR credentials to required managed identity metadata.

Operator guidance will describe:

- retrieving the identity resource ID from Terraform outputs
- entering that value in the install wizard
- understanding that private Azure sandbox images now require managed identity, not static credentials

The server configuration docs will remove the Azure sandbox credential guidance for `FABRO_AZURE_ACR_USERNAME` and `FABRO_AZURE_ACR_PASSWORD` from the supported Azure path.

## Failure handling

Failure handling must be explicit.

- If the install payload omits `acr_identity_resource_id`, install validation fails.
- If runtime Azure sandbox config omits `acr_identity_resource_id`, config resolution fails.
- If ACI rejects the identity attachment or registry pull auth, the run fails with the Azure error surfaced clearly.
- Fabro does not silently retry with static credentials or anonymous pulls.

## Testing strategy

Validation should cover four layers.

### 1. API and install tests

- OpenAPI schema includes `acr_identity_resource_id`
- generated TypeScript client includes the new field
- browser install Azure step requires the field
- install API persists and rehydrates the field

### 2. Runtime config tests

- `AzurePlatformConfig` includes `acr_identity_resource_id`
- `resolve_azure_platform_config()` requires and resolves the field correctly
- no Azure sandbox registry-secret fallback remains in the managed-identity path

### 3. ACI request-body tests

- generated request body includes top-level `identity`
- generated request body includes `imageRegistryCredentials[].identity`
- request body still includes `server`, subnet, ports, and other existing fields correctly

### 4. Terraform validation

- sandbox Terraform outputs expose the identity resource ID
- Terraform validates with the new RBAC/output wiring in place

## Consequences

### Benefits

- Azure sandbox pulls match the same managed-identity trust model as the server image pull path.
- The smoke workflow can validate a private ACR-hosted sandbox image without stored registry credentials.
- Operators no longer need to manage ACR pull secrets for Azure sandboxes in the supported Azure path.

### Trade-offs

- The install flow and docs gain one more required Azure field.
- The Azure deployment becomes dependent on correct identity-attach RBAC for ACI creation.
- Older static-credential assumptions for Azure sandbox pulls are intentionally removed from the supported path.

## Implementation outline

1. Extend the Azure install/API/UI contract with `acr_identity_resource_id`.
2. Persist the new field in Azure platform settings and summaries.
3. Extend `AzurePlatformConfig` and runtime resolution to require the field.
4. Update ACI request generation to attach the identity and use registry auth by identity.
5. Remove the Azure sandbox registry-secret path from the managed-identity Azure deployment flow.
6. Expose the identity resource ID from Terraform outputs and add any missing RBAC for identity attachment.
7. Update Azure deployment and server configuration docs.
8. Verify the full path with tests and Terraform validation.
