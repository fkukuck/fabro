# Azure greenfield follow-up validation

Date: 2026-04-30
Worktree: `/Users/ipt/repos/fabro-fkukuck/.worktrees/azure-greenfield-stabilization`
Branch: `opencode/azure-greenfield-stabilization`

## Goal

Revalidate the greenfield bring-up path and the steady-state repeat-deploy path for the Azure deployment work completed in Tasks 1-4.

## Fresh evidence collected

### Local Terraform validation

- `terraform/bootstrap/github_actions`
  - `terraform init -backend=false`: succeeded.
  - `terraform validate`: succeeded.
- `terraform/environments/sandbox`
  - `terraform validate`: succeeded.

This proves both Terraform roots are syntactically valid in the current worktree.

### Local install-flow application tests

- `apps/fabro-web`
  - `bun test app/install-app.test.tsx`: passed (`17 pass`, `0 fail`).
- repository root
  - `ulimit -n 4096 && cargo nextest run -p fabro-server install`: passed (`64 tests run: 64 passed, 476 skipped`).

This provides fresh local coverage for the install wizard and server-side install/deploy state handling that underpins the Azure bring-up flow.

### GitHub and Azure environment inspection

- `gh repo view --json nameWithOwner,defaultBranchRef` on the real deployment source reported `fkukuck/fabro` with default branch `main`.
- `gh workflow list --repo fkukuck/fabro --json id,name,path,state` confirmed an active `.github/workflows/deploy-azure.yml` workflow in the target repo.
- `gh api repos/fkukuck/fabro/environments` returned no environments, so the documented GitHub `production` environment is not set up yet.
- `az account show --output json` succeeded and confirmed a live Azure login in subscription `97200cb2-456d-4471-876a-55f0a2bd8d54`.
- `az resource list --resource-group "fkukuck-fabro-tfstate" --query "[].{type:type,name:name}" -o tsv` showed the backend storage account `fkukuckfabrotfstate`.

## Greenfield path

The fresh greenfield infrastructure apply succeeded with a new remote backend key and a new resource group:

- Backend state key: `greenfield-20260430152528.tfstate`
- Resource group: `fkukuck-fabro-greenfield-20260430152528`
- Region: `northeurope`
- ACR: `fkukuckfabroacrgf0430152528.azurecr.io`
- Container App URL: `https://fkukuck-fabro-srv-gf0430152528--xhe8xj2.grayplant-53b27a9b.northeurope.azurecontainerapps.io`
- ACI subnet: `/subscriptions/97200cb2-456d-4471-876a-55f0a2bd8d54/resourceGroups/fkukuck-fabro-greenfield-20260430152528/providers/Microsoft.Network/virtualNetworks/fkukuck-fabro-vnet-gf0430152528/subnets/fkukuck-aci-subnet-gf0430152528`
- Sandbox pull identity: `/subscriptions/97200cb2-456d-4471-876a-55f0a2bd8d54/resourceGroups/fkukuck-fabro-greenfield-20260430152528/providers/Microsoft.ManagedIdentity/userAssignedIdentities/fkukuck-fabro-server-identity-gf0430152528-sandbox-pull`

The local-equivalent deployment path then succeeded end-to-end:

1. Applied `terraform/environments/sandbox` once with `fabro_server_enabled = false` and no `-target` workaround.
2. Built and pushed immutable images:
   - `fkukuckfabroacrgf0430152528.azurecr.io/fabro-server:20260430T153206Z-4130f65c2`
   - `fkukuckfabroacrgf0430152528.azurecr.io/fabro-azure-sandbox-base:20260430T153206Z-4130f65c2`
3. Applied `terraform/environments/sandbox` again with `fabro_server_enabled = true` and the immutable `fabro_server_image` value above.
4. Confirmed `GET /health` returned `{"status":"ok","mode":"install"}`.
5. Completed install manually through the Azure-aware wizard using:
   - Azure Blob object store
   - Azure Container Instances sandbox runtime
   - GitHub token auth path
6. Confirmed authenticated `GET /api/v1/models` succeeded with the emitted dev token.
7. Ran the Azure smoke workflow successfully against the deployed server.

## Repeat-deploy path

The same-input Terraform stability check now passes against the live deployed environment.

- Before the final follow-up fix, `terraform plan -detailed-exitcode` still wanted three in-place updates:
  - `module.fabro_server.azurerm_container_app.this[0]`: `workload_profile_name = "Consumption" -> null`
  - `module.network.azurerm_subnet.aca`: delegated `actions` drift
  - `module.network.azurerm_subnet.aci`: delegated `actions` drift
- After adding explicit subnet delegation actions in `terraform/modules/network/main.tf` and setting `workload_profile_name = "Consumption"` in `terraform/modules/fabro_server/main.tf`, the exact same command returned:
  - `No changes. Your infrastructure matches the configuration.`
  - exit code `0`

That proves the repeat-plan/no-op requirement is satisfied for the local-equivalent deploy path.

The remaining unvalidated piece is the intended GitHub Actions OIDC path. Two external blockers remain there:

1. The current Azure login does not have enough Azure AD privilege to create the GitHub Actions app registration and service principal in `terraform/bootstrap/github_actions` (`Authorization_RequestDenied: Insufficient privileges to complete the operation`).
2. The available GitHub push credentials do not have `workflow` scope, so GitHub rejects pushing the updated `.github/workflows/deploy-azure.yml` to `fkukuck/fabro`.

## Responsible conclusion

Task 5 is complete for the local-equivalent validation path:

- fresh greenfield bring-up succeeded without `-target`
- the deployed server passed `/health`
- manual install completed through the Azure-aware flow
- authenticated `GET /api/v1/models` succeeded
- the Azure smoke workflow succeeded
- the same-input repeat Terraform plan is now a no-op

### Smoke evidence

- Successful run ID: `01KQFHSXQY7J7QXN6V6JT4QKX7`
- Status: `SUCCESS`

### Remaining external follow-up

The GitHub `production` environment in `fkukuck/fabro` was created during this pass and populated with:

- the Azure environment variables for this fresh deployment,
- the backend variables for `greenfield-20260430152528.tfstate`, and
- the `FABRO_DEPLOY_DEV_TOKEN` secret emitted by the successful install.

The only remaining gap is proving the GitHub Actions OIDC/bootstrap path in the real repo, which still requires:

1. Azure AD permission to create or manage the GitHub Actions app registration/service principal used by `terraform/bootstrap/github_actions`.
2. A GitHub token or SSH credential with permission to push workflow-file changes (`workflow` scope for PAT-based pushes), since the current push credentials were rejected for `.github/workflows/deploy-azure.yml`.
