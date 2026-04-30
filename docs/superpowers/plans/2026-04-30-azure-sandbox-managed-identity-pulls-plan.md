# Azure Sandbox Managed-Identity ACR Pulls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Azure sandbox container groups pull private ACR images with managed identity only, using a dedicated low-privilege Terraform-managed user-assigned identity.

**Architecture:** Extend the Azure install/config path with one new non-secret field, `acr_identity_resource_id`, then carry that field through runtime resolution into the ACI create payload. The sandbox provider will attach a dedicated low-privilege user-assigned identity to the container group and reference that same identity in `imageRegistryCredentials`, while Terraform creates that identity, grants it `AcrPull`, exposes its resource ID, and grants the server identity permission to attach it.

**Tech Stack:** Rust, Axum, React 19, TypeScript, OpenAPI, Terraform (`azurerm`), Azure Container Instances, Azure Container Registry, Bun, Cargo.

---

## File map

- `docs/public/api-reference/fabro-api.yaml`
  Add `acr_identity_resource_id` to the install-time Azure request and summary schema.
- `lib/packages/fabro-api-client/src/models/install-azure-config-input.ts`
  Generated TypeScript request model for the Azure install step.
- `lib/packages/fabro-api-client/src/models/install-azure-summary.ts`
  Generated TypeScript summary model for installer hydration/review.
- `apps/fabro-web/app/install-app.tsx`
  Azure install step UI, validation, payload construction, hydration, and review summary.
- `apps/fabro-web/app/install-app.test.tsx`
  Browser installer coverage for the new Azure identity field.
- `lib/crates/fabro-server/src/install.rs`
  Install-time Azure payload parsing, validation, session summary, and persistence.
- `lib/crates/fabro-server/tests/it/api/install.rs`
  Install API coverage for the new Azure field.
- `lib/crates/fabro-server/src/azure_platform.rs`
  Runtime resolution and snapshot writing for Azure platform config.
- `lib/crates/fabro-sandbox/src/azure/config.rs`
  Runtime Azure sandbox config shape loaded by the sandbox provider.
- `lib/crates/fabro-sandbox/src/azure/arm.rs`
  Azure Container Instance create request body generation.
- `lib/crates/fabro-sandbox/tests/azure_provider.rs`
  Provider-level serialization/config tests for Azure sandbox config.
- `lib/crates/fabro-types/src/settings/server.rs`
  Canonical persisted Azure platform settings shape shared across server config.
- `terraform/modules/identity/variables.tf`
  Identity-module input surface for optional attach-scope RBAC.
- `terraform/modules/identity/main.tf`
  Role assignment wiring so the running server identity can attach the dedicated sandbox-pull identity to created ACI groups.
- `terraform/environments/sandbox/main.tf`
  Environment wiring for the new sandbox-pull identity and server-to-sandbox identity attachment permission.
- `terraform/environments/sandbox/outputs.tf`
  Expose the sandbox-pull identity resource ID for operators/install.
- `docs/public/administration/deploy-azure.mdx`
  Operator docs for pulling the identity resource ID from Terraform outputs.
- `docs/public/administration/server-configuration.mdx`
  Config docs for `acr_identity_resource_id` and the managed-identity-only Azure sandbox path.

## Task 1: Extend Azure Install/API/UI With `acr_identity_resource_id`

**Files:**
- Modify: `docs/public/api-reference/fabro-api.yaml`
- Modify: `apps/fabro-web/app/install-app.tsx`
- Modify: `apps/fabro-web/app/install-app.test.tsx`
- Regenerate: `lib/packages/fabro-api-client/src/models/install-azure-config-input.ts`
- Regenerate: `lib/packages/fabro-api-client/src/models/install-azure-summary.ts`
- Test: `apps/fabro-web/app/install-app.test.tsx`

- [ ] **Step 1: Write the failing web test**

Add a test to `apps/fabro-web/app/install-app.test.tsx` that:

```tsx
test("requires Azure ACR identity resource ID before continuing", async () => {
  // Start at /install/azure with a valid session payload.
  // Fill subscription_id, resource_group, location, subnet_id, acr_server.
  // Leave acr_identity_resource_id blank.
  // Submit the form.
  // Assert the UI stays on the Azure step and shows:
  // "Enter the Azure ACR identity resource ID before continuing."
});
```

Add a second test that persists and rehydrates the field:

```tsx
test("saves and rehydrates Azure ACR identity resource ID", async () => {
  // Mock /install/azure PUT and the follow-up /install/session response.
  // Submit acr_identity_resource_id along with the existing Azure fields.
  // Assert the request body includes:
  // { acr_identity_resource_id: "/subscriptions/sub-1/.../userAssignedIdentities/fabro-sandbox-pull" }
  // Assert the next session summary rehydrates the same value into the form/review step.
});
```

- [ ] **Step 2: Run the installer test and verify RED**

Run: `bun test app/install-app.test.tsx`

Expected: FAIL because the Azure form does not yet expose or require `acr_identity_resource_id`.

- [ ] **Step 3: Extend the OpenAPI Azure install schema and regenerate the client**

Update `docs/public/api-reference/fabro-api.yaml`:

```yaml
InstallAzureConfigInput:
  type: object
  required:
    - subscription_id
    - resource_group
    - location
    - subnet_id
    - acr_server
    - acr_identity_resource_id
  properties:
    subscription_id:
      type: string
    resource_group:
      type: string
    location:
      type: string
    subnet_id:
      type: string
    acr_server:
      type: string
    acr_identity_resource_id:
      type: string
    sandboxd_port:
      type: integer
      format: int32
      minimum: 1
      maximum: 65535

InstallAzureSummary:
  type: object
  required:
    - subscription_id
    - resource_group
    - location
    - subnet_id
    - acr_server
    - acr_identity_resource_id
    - sandboxd_port
  properties:
    subscription_id:
      type: string
    resource_group:
      type: string
    location:
      type: string
    subnet_id:
      type: string
    acr_server:
      type: string
    acr_identity_resource_id:
      type: string
    sandboxd_port:
      type: integer
      format: int32
```

Regenerate the client:

```bash
cd lib/packages/fabro-api-client && bun run generate
```

- [ ] **Step 4: Add the Azure field to the installer UI and payloads**

Update `apps/fabro-web/app/install-app.tsx`.

Extend the form type:

```tsx
type AzureForm = {
  subscriptionId: string;
  resourceGroup: string;
  location: string;
  subnetId: string;
  acrServer: string;
  acrIdentityResourceId: string;
  sandboxdPort: string;
};
```

Require the field before continue:

```tsx
if (!azureForm.acrIdentityResourceId.trim()) {
  setSaveError("Enter the Azure ACR identity resource ID before continuing.");
  focusInput(acrIdentityResourceIdInputRef);
  return;
}
```

Send it in the request:

```tsx
await putInstallAzure(installToken, {
  subscription_id: azureForm.subscriptionId.trim(),
  resource_group: azureForm.resourceGroup.trim(),
  location: azureForm.location.trim(),
  subnet_id: azureForm.subnetId.trim(),
  acr_server: azureForm.acrServer.trim(),
  acr_identity_resource_id: azureForm.acrIdentityResourceId.trim(),
  sandboxd_port: Number(azureForm.sandboxdPort),
});
```

Add the input field and review summary row:

```tsx
<Field label="ACR identity resource ID">
  <input
    ref={acrIdentityResourceIdInputRef}
    name="azure_acr_identity_resource_id"
    value={azureForm.acrIdentityResourceId}
    onChange={(event) =>
      setAzureForm((current) => ({
        ...current,
        acrIdentityResourceId: event.target.value,
      }))
    }
    className={`${INPUT_CLASS} font-mono`}
    placeholder="/subscriptions/.../userAssignedIdentities/fabro-sandbox-pull"
    spellCheck={false}
    autoCapitalize="off"
  />
</Field>
```

```tsx
<SummaryRow
  label="ACR identity"
  value={azure.acr_identity_resource_id ?? "Not set"}
  mono
/>
```

- [ ] **Step 5: Run web verification**

Run: `cd apps/fabro-web && bun test app/install-app.test.tsx && bun run typecheck`

Expected: PASS.

## Task 2: Persist And Resolve The Identity Resource ID In Server Runtime Config

**Files:**
- Modify: `lib/crates/fabro-types/src/settings/server.rs`
- Modify: `lib/crates/fabro-server/src/install.rs`
- Modify: `lib/crates/fabro-server/src/azure_platform.rs`
- Modify: `lib/crates/fabro-server/tests/it/api/install.rs`
- Test: `lib/crates/fabro-server/tests/it/api/install.rs`

- [ ] **Step 1: Write the failing install API test**

Add a test to `lib/crates/fabro-server/tests/it/api/install.rs` that submits:

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

Then assert the install session summary and persisted settings contain `acr_identity_resource_id`.

- [ ] **Step 2: Run the focused install test and verify RED**

Run: `ulimit -n 4096 && cargo nextest run -p fabro-server install`

Expected: FAIL because the server-side install types do not yet parse/persist `acr_identity_resource_id`.

- [ ] **Step 3: Extend the canonical settings and install types**

Update `lib/crates/fabro-types/src/settings/server.rs` Azure platform settings:

```rust
pub struct ServerSandboxAzurePlatformSettings {
    pub subscription_id: String,
    pub resource_group:  String,
    pub location:        String,
    pub subnet_id:       String,
    pub acr_server:      String,
    pub acr_identity_resource_id: String,
    pub sandboxd_port:   u16,
}
```

Update `lib/crates/fabro-server/src/install.rs` request/state/summary types to carry the field and require it when trimming/validating Azure input.

Persist it into the platform selection written to `settings.toml`.

- [ ] **Step 4: Remove Azure sandbox registry-secret resolution from runtime config**

Update `lib/crates/fabro-server/src/azure_platform.rs`:

```rust
Ok(Some(AzurePlatformConfig {
    subscription_id: platform.subscription_id.clone(),
    resource_group: platform.resource_group.clone(),
    location: platform.location.clone(),
    subnet_id: platform.subnet_id.clone(),
    acr_server: platform.acr_server.clone(),
    acr_identity_resource_id: platform.acr_identity_resource_id.clone(),
    sandboxd_port: platform.sandboxd_port,
}))
```

Update snapshot writing to include the new field and stop snapshotting `acr_username` / `acr_password`.

- [ ] **Step 5: Run server verification for install/runtime config**

Run: `ulimit -n 4096 && cargo nextest run -p fabro-server install azure_platform`

Expected: PASS.

## Task 3: Update Azure Sandbox Config And ACI Request Generation

**Files:**
- Modify: `lib/crates/fabro-sandbox/src/azure/config.rs`
- Modify: `lib/crates/fabro-sandbox/src/azure/arm.rs`
- Modify: `lib/crates/fabro-sandbox/tests/azure_provider.rs`
- Test: `lib/crates/fabro-sandbox/src/azure/arm.rs`
- Test: `lib/crates/fabro-sandbox/tests/azure_provider.rs`

- [ ] **Step 1: Write the failing ACI request-body test**

Add a test in `lib/crates/fabro-sandbox/src/azure/arm.rs` asserting the container-group body includes both top-level identity and registry identity:

```rust
#[test]
fn build_container_group_body_uses_user_assigned_identity_for_private_acr_pull() {
    let config = AzurePlatformConfig {
        subscription_id: "sub-1".into(),
        resource_group: "rg-1".into(),
        location: "eastus".into(),
        subnet_id: "/subscriptions/sub-1/.../aci".into(),
        acr_server: "fabro.azurecr.io".into(),
        acr_identity_resource_id:
            "/subscriptions/sub-1/.../userAssignedIdentities/fabro-sandbox-pull".into(),
        sandboxd_port: 7777,
    };

    let body = build_container_group_body(&config, "run-1", "fabro.azurecr.io/image:tag", 1.0, 2.0);

    assert_eq!(body["identity"]["type"], "UserAssigned");
    assert_eq!(
        body["properties"]["imageRegistryCredentials"][0]["identity"],
        "/subscriptions/sub-1/.../userAssignedIdentities/fabro-sandbox-pull"
    );
}
```

- [ ] **Step 2: Run the sandbox tests and verify RED**

Run: `ulimit -n 4096 && cargo nextest run -p fabro-sandbox azure_provider build_container_group_body_uses_user_assigned_identity_for_private_acr_pull`

Expected: FAIL because the current config/body shape only knows about username/password registry auth.

- [ ] **Step 3: Make the sandbox config managed-identity-only**

Update `lib/crates/fabro-sandbox/src/azure/config.rs`:

```rust
pub struct AzurePlatformConfig {
    pub subscription_id: String,
    pub resource_group:  String,
    pub location:        String,
    pub subnet_id:       String,
    pub acr_server:      String,
    pub acr_identity_resource_id: String,
    pub sandboxd_port:   u16,
}
```

Require `acr_identity_resource_id` when loading from env/snapshot JSON.

- [ ] **Step 4: Attach the identity in the ACI request body**

Update `lib/crates/fabro-sandbox/src/azure/arm.rs`:

```rust
json!({
    "name": name,
    "location": config.location,
    "identity": {
        "type": "UserAssigned",
        "userAssignedIdentities": {
            config.acr_identity_resource_id.clone(): {}
        }
    },
    "properties": {
        ...,
        "imageRegistryCredentials": [
            {
                "server": config.acr_server,
                "identity": config.acr_identity_resource_id,
            }
        ]
    }
})
```

Keep existing subnet, IP, ports, and workspace volume logic unchanged.

- [ ] **Step 5: Run sandbox verification**

Run: `ulimit -n 4096 && cargo nextest run -p fabro-sandbox azure_provider build_container_group_body_uses_user_assigned_identity_for_private_acr_pull`

Expected: PASS.

## Task 4: Create The Sandbox-Pull Identity In Terraform And Update Docs

**Files:**
- Modify: `terraform/modules/identity/variables.tf`
- Modify: `terraform/modules/identity/main.tf`
- Modify: `terraform/environments/sandbox/main.tf`
- Modify: `terraform/environments/sandbox/outputs.tf`
- Modify: `docs/public/administration/deploy-azure.mdx`
- Modify: `docs/public/administration/server-configuration.mdx`
- Test: `terraform/environments/sandbox`

- [ ] **Step 1: Add a failing Terraform/doc expectation check**

Run:

```bash
rg -n "managed_identity_resource_id|acr_identity_resource_id|Managed Identity Operator" \
  terraform/environments/sandbox/outputs.tf \
  terraform/modules/identity/main.tf \
  docs/public/administration/deploy-azure.mdx \
  docs/public/administration/server-configuration.mdx
```

Expected: no matches for the new output/field/RBAC guidance yet.

- [ ] **Step 2: Create/expose the sandbox-pull identity and grant attach permission**

Update `terraform/environments/sandbox/outputs.tf`:

```hcl
output "sandbox_pull_identity_resource_id" {
  value = module.sandbox_pull_identity.id
}
```

Update `terraform/environments/sandbox/main.tf` to instantiate a second identity module for the sandbox pull identity with only ACR pull responsibility, and keep the existing server identity separate.

Then grant the server identity permission to attach that separate identity to created ACI groups via `Managed Identity Operator` on the sandbox-pull identity resource.

The target result is a checked-in Terraform model where:

- `module.identity` remains the privileged server identity
- `module.sandbox_pull_identity` is low-privilege and sandbox-attached
- only the sandbox-pull identity is persisted as `acr_identity_resource_id`

- [ ] **Step 3: Update operator docs for the new field**

Add to `docs/public/administration/deploy-azure.mdx`:

```mdx
Record these Terraform outputs for the install flow:

- `storage_account_name`
- `blob_data_container_name`
- `sandbox_pull_identity_resource_id`
```

And explain during the Azure install step that the operator must enter the sandbox-pull identity resource ID used for private ACR sandbox pulls.

Update `docs/public/administration/server-configuration.mdx`:

```mdx
| `acr_identity_resource_id` | User-assigned managed identity resource ID used for private ACR sandbox pulls | — |
```

Remove the old Azure sandbox guidance that described `FABRO_AZURE_ACR_USERNAME` and `FABRO_AZURE_ACR_PASSWORD` as the supported path.

- [ ] **Step 4: Run Terraform and docs verification**

Run:

```bash
terraform -chdir=terraform/environments/sandbox init -backend=false && terraform -chdir=terraform/environments/sandbox validate
bunx prettier --check docs/public/administration/deploy-azure.mdx docs/public/administration/server-configuration.mdx
```

Expected: PASS.

## Final Verification

- [ ] **Step 1: Re-run the focused checks from every task**

Run:

```bash
cd apps/fabro-web && bun test app/install-app.test.tsx && bun run typecheck
ulimit -n 4096 && cargo nextest run -p fabro-server install azure_platform
ulimit -n 4096 && cargo nextest run -p fabro-sandbox azure_provider build_container_group_body_uses_user_assigned_identity_for_private_acr_pull
terraform -chdir=terraform/environments/sandbox init -backend=false && terraform -chdir=terraform/environments/sandbox validate
bunx prettier --check docs/public/administration/deploy-azure.mdx docs/public/administration/server-configuration.mdx
```

Expected: all commands pass.

- [ ] **Step 2: Review the completed behavior against the spec**

Use this checklist before marking the follow-up complete:

```md
- Azure install/API/UI require `acr_identity_resource_id`.
- Persisted Azure sandbox platform config includes `acr_identity_resource_id`.
- Azure sandbox runtime config no longer depends on ACR username/password.
- ACI request bodies include both top-level `identity` and registry `identity`.
- Terraform exposes the sandbox-pull identity resource ID for operators.
- Sandbox-attached identity is separate from the privileged server identity.
- Docs describe managed-identity-only private ACR sandbox pulls.
```
