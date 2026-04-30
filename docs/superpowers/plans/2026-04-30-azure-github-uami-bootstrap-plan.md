# Azure GitHub UAMI Bootstrap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Azure AD app-registration bootstrap path with a Terraform-managed GitHub Actions user-assigned managed identity, keep the validated deploy workflow behavior in-repo, and move all durable Azure setup instructions into the user-facing docs and root READMEs.

**Architecture:** Keep `terraform/bootstrap/github_actions` as the one-time root that creates backend state plus CI identity, but switch it from `azuread_*` resources to a UAMI plus federated credential and backend-storage RBAC. Keep `terraform/environments/sandbox` consuming the same `github_actions_principal_id` contract, preserve the corrected GitHub Actions workflow behavior, and make `docs/public/administration/deploy-azure.mdx` the canonical end-to-end greenfield guide with the two Terraform READMEs and `README.md` as focused companion docs.

**Tech Stack:** Terraform (`azurerm`), GitHub Actions, Azure Container Registry, Azure Container Apps, Azure Blob backend state, Mintlify docs, GitHub CLI, Bun, Cargo.

---

## File map

- `terraform/bootstrap/github_actions/main.tf`
  One-time bootstrap resources. Will stop creating Entra app/service-principal resources and instead create the GitHub Actions UAMI, federated credential, and backend RBAC.
- `terraform/bootstrap/github_actions/variables.tf`
  Bootstrap input contract. Will replace app-registration naming input with managed-identity naming input.
- `terraform/bootstrap/github_actions/outputs.tf`
  Bootstrap outputs copied into the GitHub `production` environment.
- `terraform/bootstrap/github_actions/versions.tf`
  Provider set for the bootstrap root. Will drop `azuread` if no longer needed and pin the `azurerm` version that supports the managed identity/federated credential resources we use.
- `terraform/bootstrap/github_actions/providers.tf`
  Provider configuration for the bootstrap root.
- `terraform/bootstrap/github_actions/terraform.tfvars.example`
  Example bootstrap inputs shown to operators.
- `terraform/bootstrap/github_actions/README.md`
  Root-local bootstrap instructions and output handoff.
- `terraform/environments/sandbox/variables.tf`
  Describes `github_actions_principal_id`; wording must change from service-principal to UAMI principal.
- `terraform/environments/sandbox/README.md`
  Root-local steady-state environment instructions.
- `.github/workflows/deploy-azure.yml`
  The validated deploy workflow; must preserve the runner/action/backend-preflight fixes that made the GitHub Actions path actually run.
- `docs/public/administration/deploy-azure.mdx`
  Canonical greenfield Azure + GitHub repo admin guide.
- `README.md`
  Short durable `Azure + GitHub Setup` pointer section that routes readers to the canonical guide and the two Terraform READMEs.
- `docs/superpowers/specs/2026-04-30-azure-github-uami-bootstrap-design.md`
  Temporary approved design doc to delete after durable docs are updated and validation is complete.
- `docs/superpowers/specs/2026-04-30-azure-greenfield-followup-validation.md`
  Temporary validation note to either fold into durable docs or delete at the end.
- `docs/superpowers/plans/2026-04-30-azure-github-uami-bootstrap-plan.md`
  Temporary implementation plan to delete after the work is complete.

## Task 1: Replace Bootstrap App Registration With Terraform-Managed UAMI

**Files:**
- Modify: `terraform/bootstrap/github_actions/main.tf`
- Modify: `terraform/bootstrap/github_actions/variables.tf`
- Modify: `terraform/bootstrap/github_actions/outputs.tf`
- Modify: `terraform/bootstrap/github_actions/versions.tf`
- Modify: `terraform/bootstrap/github_actions/providers.tf`
- Modify: `terraform/bootstrap/github_actions/terraform.tfvars.example`
- Test: `terraform/bootstrap/github_actions`

- [ ] **Step 1: Verify the current bootstrap root still uses the old Azure AD app model**

Run:

```bash
rg -n "azuread_application|azuread_service_principal|azuread_application_federated_identity_credential|github_actions_application_name" terraform/bootstrap/github_actions
```

Expected: matches in `main.tf`, `variables.tf`, `versions.tf`, `providers.tf`, and `terraform.tfvars.example`, proving the old app-registration model is still present before you replace it.

- [ ] **Step 2: Replace the bootstrap input contract and provider set**

Update `terraform/bootstrap/github_actions/variables.tf`:

```tf
variable "subscription_id" {
  type = string
}

variable "location" {
  type = string
}

variable "backend_resource_group_name" {
  type = string
}

variable "backend_storage_account_name" {
  type = string
}

variable "backend_container_name" {
  type = string
}

variable "backend_state_key" {
  type = string
}

variable "github_repository" {
  type = string
}

variable "github_environment_name" {
  type = string
}

variable "github_actions_identity_name" {
  type = string
}

variable "tags" {
  type    = map(string)
  default = {}
}
```

Update `terraform/bootstrap/github_actions/versions.tf`:

```tf
terraform {
  required_version = ">= 1.8.0"

  required_providers {
    azurerm = {
      source  = "hashicorp/azurerm"
      version = "~> 4.69"
    }
  }
}
```

Update `terraform/bootstrap/github_actions/providers.tf` so it no longer declares `provider "azuread" {}`:

```tf
provider "azurerm" {
  features {}

  subscription_id = var.subscription_id
}
```

- [ ] **Step 3: Replace the bootstrap resources with UAMI, federated credential, and backend RBAC**

Rewrite `terraform/bootstrap/github_actions/main.tf` to this shape:

```tf
resource "azurerm_resource_group" "backend" {
  name     = var.backend_resource_group_name
  location = var.location
  tags     = var.tags
}

resource "azurerm_storage_account" "backend" {
  name                     = var.backend_storage_account_name
  resource_group_name      = azurerm_resource_group.backend.name
  location                 = azurerm_resource_group.backend.location
  account_tier             = "Standard"
  account_replication_type = "LRS"
  tags                     = var.tags
}

resource "azurerm_storage_container" "backend" {
  name                  = var.backend_container_name
  storage_account_id    = azurerm_storage_account.backend.id
  container_access_type = "private"
}

data "azurerm_client_config" "current" {}

resource "azurerm_user_assigned_identity" "github_actions" {
  name                = var.github_actions_identity_name
  resource_group_name = azurerm_resource_group.backend.name
  location            = azurerm_resource_group.backend.location
  tags                = var.tags
}

resource "azurerm_federated_identity_credential" "github_actions" {
  name                = "github-actions-${var.github_environment_name}"
  resource_group_name = azurerm_resource_group.backend.name
  parent_id           = azurerm_user_assigned_identity.github_actions.id
  audience            = ["api://AzureADTokenExchange"]
  issuer              = "https://token.actions.githubusercontent.com"
  subject             = "repo:${var.github_repository}:environment:${var.github_environment_name}"
}

resource "azurerm_role_assignment" "backend_blob_access" {
  scope                = azurerm_storage_account.backend.id
  role_definition_name = "Storage Blob Data Contributor"
  principal_id         = azurerm_user_assigned_identity.github_actions.principal_id
}

resource "azurerm_role_assignment" "backend_reader" {
  scope                = azurerm_storage_account.backend.id
  role_definition_name = "Reader"
  principal_id         = azurerm_user_assigned_identity.github_actions.principal_id
}

resource "azurerm_role_assignment" "backend_key_operator" {
  scope                = azurerm_storage_account.backend.id
  role_definition_name = "Storage Account Key Operator Service Role"
  principal_id         = azurerm_user_assigned_identity.github_actions.principal_id
}
```

- [ ] **Step 4: Keep the output contract stable while sourcing values from the UAMI**

Update `terraform/bootstrap/github_actions/outputs.tf`:

```tf
output "github_actions_client_id" {
  value = azurerm_user_assigned_identity.github_actions.client_id
}

output "github_actions_principal_id" {
  value = azurerm_user_assigned_identity.github_actions.principal_id
}

output "tenant_id" {
  value = data.azurerm_client_config.current.tenant_id
}

output "subscription_id" {
  value = data.azurerm_client_config.current.subscription_id
}

output "backend_resource_group_name" {
  value = azurerm_resource_group.backend.name
}

output "backend_storage_account_name" {
  value = azurerm_storage_account.backend.name
}

output "backend_container_name" {
  value = azurerm_storage_container.backend.name
}

output "backend_state_key" {
  value = var.backend_state_key
}
```

Update `terraform/bootstrap/github_actions/terraform.tfvars.example`:

```tf
subscription_id            = "00000000-0000-0000-0000-000000000000"
location                   = "northeurope"
backend_resource_group_name  = "fkukuck-fabro-tfstate"
backend_storage_account_name = "fkukuckfabrotfstate"
backend_container_name       = "tfstate"
backend_state_key            = "sandbox-prod.tfstate"
github_repository            = "fkukuck/fabro"
github_environment_name      = "production"
github_actions_identity_name = "fkukuck-fabro-gha-production"

tags = {
  environment = "bootstrap"
  managed_by  = "terraform"
}
```

- [ ] **Step 5: Validate the new bootstrap root and prove the Azure AD resources are gone**

Run:

```bash
terraform -chdir=terraform/bootstrap/github_actions init -backend=false -reconfigure
terraform -chdir=terraform/bootstrap/github_actions validate
rg -n "azuread_application|azuread_service_principal|azuread_application_federated_identity_credential|provider \"azuread\"|github_actions_application_name" terraform/bootstrap/github_actions
```

Expected:
- first command: PASS
- second command: `Success! The configuration is valid.`
- third command: no matches

- [ ] **Step 6: Commit**

```bash
git add terraform/bootstrap/github_actions/main.tf terraform/bootstrap/github_actions/variables.tf terraform/bootstrap/github_actions/outputs.tf terraform/bootstrap/github_actions/versions.tf terraform/bootstrap/github_actions/providers.tf terraform/bootstrap/github_actions/terraform.tfvars.example
git commit -m "feat(terraform): bootstrap github actions with managed identity"
```

## Task 2: Finalize the Deploy Workflow and Sandbox CI Contract

**Files:**
- Modify: `.github/workflows/deploy-azure.yml`
- Modify: `terraform/environments/sandbox/variables.tf`
- Test: `.github/workflows/deploy-azure.yml`

- [ ] **Step 1: Capture the old workflow assumptions before changing them**

Run:

```bash
rg -n "ubuntu-24.04-x86-32-cores|azure/login@4c8e52|setup-terraform@3c0c8c|terraform -chdir=terraform/environments/sandbox validate|AZURE_GITHUB_ACTIONS_PRINCIPAL_ID" .github/workflows/deploy-azure.yml terraform/environments/sandbox/variables.tf
```

Expected on pre-fix code: matches for the unavailable larger-runner label, stale action pins, or the old service-principal wording in `terraform/environments/sandbox/variables.tf`.

- [ ] **Step 2: Keep the validated workflow behavior in-repo**

Update `.github/workflows/deploy-azure.yml` so it has these exact behaviors:

```yml
jobs:
  deploy:
    name: Deploy production Azure environment
    runs-on: ubuntu-24.04
```

```yml
      - uses: hashicorp/setup-terraform@b9cd54a3c349d3f38e8881555d616ced269862dd # v3.1.2
      - uses: azure/login@a457da9ea143d694b1b9c7c869ebb04ebe844ef5 # v2.3.0
```

```yml
      - name: Validate Azure deploy inputs
        env:
          AZURE_CLIENT_ID: ${{ vars.AZURE_CLIENT_ID }}
          AZURE_TENANT_ID: ${{ vars.AZURE_TENANT_ID }}
          AZURE_SUBSCRIPTION_ID: ${{ vars.AZURE_SUBSCRIPTION_ID }}
          AZURE_LOCATION: ${{ vars.AZURE_LOCATION }}
          AZURE_RESOURCE_GROUP_NAME: ${{ vars.AZURE_RESOURCE_GROUP_NAME }}
          AZURE_VNET_NAME: ${{ vars.AZURE_VNET_NAME }}
          AZURE_VNET_CIDR: ${{ vars.AZURE_VNET_CIDR }}
          AZURE_ACA_SUBNET_NAME: ${{ vars.AZURE_ACA_SUBNET_NAME }}
          AZURE_ACA_SUBNET_CIDR: ${{ vars.AZURE_ACA_SUBNET_CIDR }}
          AZURE_ACI_SUBNET_NAME: ${{ vars.AZURE_ACI_SUBNET_NAME }}
          AZURE_ACI_SUBNET_CIDR: ${{ vars.AZURE_ACI_SUBNET_CIDR }}
          AZURE_STORAGE_ACCOUNT_NAME: ${{ vars.AZURE_STORAGE_ACCOUNT_NAME }}
          AZURE_SERVER_STORAGE_SHARE_NAME: ${{ vars.AZURE_SERVER_STORAGE_SHARE_NAME }}
          AZURE_ACR_NAME: ${{ vars.AZURE_ACR_NAME }}
          AZURE_IDENTITY_NAME: ${{ vars.AZURE_IDENTITY_NAME }}
          AZURE_CONTAINER_APPS_ENVIRONMENT_NAME: ${{ vars.AZURE_CONTAINER_APPS_ENVIRONMENT_NAME }}
          AZURE_FABRO_SERVER_NAME: ${{ vars.AZURE_FABRO_SERVER_NAME }}
          AZURE_GITHUB_ACTIONS_PRINCIPAL_ID: ${{ vars.AZURE_GITHUB_ACTIONS_PRINCIPAL_ID }}
          TF_BACKEND_RESOURCE_GROUP: ${{ vars.TF_BACKEND_RESOURCE_GROUP }}
          TF_BACKEND_STORAGE_ACCOUNT: ${{ vars.TF_BACKEND_STORAGE_ACCOUNT }}
          TF_BACKEND_CONTAINER: ${{ vars.TF_BACKEND_CONTAINER }}
          TF_BACKEND_KEY: ${{ vars.TF_BACKEND_KEY }}
        run: |
          for name in AZURE_CLIENT_ID AZURE_TENANT_ID AZURE_SUBSCRIPTION_ID AZURE_LOCATION AZURE_RESOURCE_GROUP_NAME AZURE_VNET_NAME AZURE_VNET_CIDR AZURE_ACA_SUBNET_NAME AZURE_ACA_SUBNET_CIDR AZURE_ACI_SUBNET_NAME AZURE_ACI_SUBNET_CIDR AZURE_STORAGE_ACCOUNT_NAME AZURE_SERVER_STORAGE_SHARE_NAME AZURE_ACR_NAME AZURE_IDENTITY_NAME AZURE_CONTAINER_APPS_ENVIRONMENT_NAME AZURE_FABRO_SERVER_NAME AZURE_GITHUB_ACTIONS_PRINCIPAL_ID TF_BACKEND_RESOURCE_GROUP TF_BACKEND_STORAGE_ACCOUNT TF_BACKEND_CONTAINER TF_BACKEND_KEY; do
            if [ -z "${!name}" ]; then
              printf 'Missing required GitHub environment variable: %s\n' "$name" >&2
              exit 1
            fi
          done
```

```yml
      - name: Init Terraform backend
        run: |
          terraform -chdir=terraform/environments/sandbox init \
            -backend-config="resource_group_name=${{ vars.TF_BACKEND_RESOURCE_GROUP }}" \
            -backend-config="storage_account_name=${{ vars.TF_BACKEND_STORAGE_ACCOUNT }}" \
            -backend-config="container_name=${{ vars.TF_BACKEND_CONTAINER }}" \
            -backend-config="key=${{ vars.TF_BACKEND_KEY }}"

      - name: Preflight Terraform plan
        run: |
          terraform -chdir=terraform/environments/sandbox plan \
            -refresh=false \
            -lock=false \
            -input=false \
            -var="fabro_server_image=example.azurecr.io/fabro-server:preflight"
```

Keep the post-apply steady-state gate:

```yml
      - name: Verify Terraform steady state
        run: |
          terraform -chdir=terraform/environments/sandbox plan -detailed-exitcode \
            -var="fabro_server_image=$ACR_SERVER/fabro-server:$DEPLOY_ID"
```

- [ ] **Step 3: Update the sandbox environment variable description to the UAMI model**

Edit `terraform/environments/sandbox/variables.tf`:

```tf
variable "github_actions_principal_id" {
  type        = string
  description = "Principal ID of the bootstrap-created GitHub Actions managed identity."
  default     = null
}
```

- [ ] **Step 4: Verify workflow formatting and wording**

Run:

```bash
bunx prettier --check .github/workflows/deploy-azure.yml
rg -n "ubuntu-24.04-x86-32-cores|azure/login@4c8e52|setup-terraform@3c0c8|service principal" .github/workflows/deploy-azure.yml terraform/environments/sandbox/variables.tf
```

Expected:
- Prettier: PASS
- `rg`: no matches

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/deploy-azure.yml terraform/environments/sandbox/variables.tf
git commit -m "fix(ci): align azure deploy workflow with validated path"
```

## Task 3: Move All Durable Azure Setup Knowledge Into Repo Docs

**Files:**
- Modify: `docs/public/administration/deploy-azure.mdx`
- Modify: `terraform/bootstrap/github_actions/README.md`
- Modify: `terraform/environments/sandbox/README.md`
- Modify: `README.md`
- Test: `docs/public/administration/deploy-azure.mdx`

- [ ] **Step 1: Prove the current docs still describe the old bootstrap model or miss the repo-admin steps**

Run:

```bash
rg -n "app registration|service principal|production environment|AZURE_CLIENT_ID|Storage Account Key Operator Service Role|Azure \+ GitHub Setup" docs/public/administration/deploy-azure.mdx terraform/bootstrap/github_actions/README.md terraform/environments/sandbox/README.md README.md
```

Expected on pre-change docs: missing or incomplete coverage for the repo-admin `production` environment creation, missing backend `Reader` / `Storage Account Key Operator Service Role`, and no short Azure setup pointer in `README.md`.

- [ ] **Step 2: Make `deploy-azure.mdx` the canonical greenfield guide**

Update `docs/public/administration/deploy-azure.mdx` so it explicitly covers:

```md
## 1. Bootstrap GitHub Actions managed identity and remote state
```

and this operator contract:

```md
- create a GitHub environment named `production`
- copy bootstrap outputs into that environment
- add the environment-specific Azure naming variables
- run the first `terraform/environments/sandbox` apply with `fabro_server_enabled = false`
- trigger `.github/workflows/deploy-azure.yml`
- complete install using Azure Blob + Azure Container Instances
- store `FABRO_DEPLOY_DEV_TOKEN`
- rerun the deploy workflow to exercise authenticated checks and smoke
```

Also add an explicit backend RBAC note under bootstrap:

```md
The bootstrap root grants the CI identity all backend storage permissions the current
`azurerm` backend flow needs: `Storage Blob Data Contributor`, `Reader`, and
`Storage Account Key Operator Service Role` on the backend storage account.
```

- [ ] **Step 3: Update both Terraform READMEs to be root-local companions, not incomplete primary guides**

Update `terraform/bootstrap/github_actions/README.md` with these sections:

```md
# GitHub Actions Bootstrap

This root creates:

- the backend resource group, storage account, and storage container
- the GitHub Actions user-assigned managed identity
- the GitHub OIDC federated credential on that identity
- backend storage RBAC for Terraform state access
```

```md
Record these outputs in the GitHub `production` environment:
- `AZURE_CLIENT_ID`
- `AZURE_TENANT_ID`
- `AZURE_SUBSCRIPTION_ID`
- `AZURE_GITHUB_ACTIONS_PRINCIPAL_ID`
- `TF_BACKEND_RESOURCE_GROUP`
- `TF_BACKEND_STORAGE_ACCOUNT`
- `TF_BACKEND_CONTAINER`
- `TF_BACKEND_KEY`
```

Update `terraform/environments/sandbox/README.md` so the inputs section says:

```md
- Set `github_actions_principal_id` to the bootstrap-created GitHub Actions managed identity principal ID before the first manual apply.
```

- [ ] **Step 4: Add a short durable Azure setup pointer to the repository README**

Insert this section into `README.md` after the self-hosting section and before `Contributing to Fabro`:

```md
## Azure + GitHub Setup

For the fully supported Azure deployment path when you have an Azure subscription and admin access to a GitHub repo:

- Start with the canonical guide: [`docs/public/administration/deploy-azure.mdx`](docs/public/administration/deploy-azure.mdx)
- See the bootstrap-root details: [`terraform/bootstrap/github_actions/README.md`](terraform/bootstrap/github_actions/README.md)
- See the steady-state environment details: [`terraform/environments/sandbox/README.md`](terraform/environments/sandbox/README.md)

These three documents together describe the greenfield setup path for Azure infrastructure, GitHub `production` environment configuration, first deploy, install, and steady-state redeploys.
```

- [ ] **Step 5: Verify markdown formatting and coverage**

Run:

```bash
bunx prettier --check README.md docs/public/administration/deploy-azure.mdx terraform/bootstrap/github_actions/README.md terraform/environments/sandbox/README.md
rg -n "user-assigned managed identity|Storage Account Key Operator Service Role|production environment|FABRO_DEPLOY_DEV_TOKEN|Azure \+ GitHub Setup" README.md docs/public/administration/deploy-azure.mdx terraform/bootstrap/github_actions/README.md terraform/environments/sandbox/README.md
```

Expected:
- Prettier: PASS
- `rg`: each phrase appears in the durable docs

- [ ] **Step 6: Commit**

```bash
git add README.md docs/public/administration/deploy-azure.mdx terraform/bootstrap/github_actions/README.md terraform/environments/sandbox/README.md
git commit -m "docs(azure): publish greenfield github setup guide"
```

## Task 4: Revalidate The Full Greenfield Path With The UAMI Bootstrap

**Files:**
- Modify: `docs/public/administration/deploy-azure.mdx`
- Modify: `terraform/bootstrap/github_actions/README.md`
- Modify: `terraform/environments/sandbox/README.md`
- Test: `terraform/bootstrap/github_actions`
- Test: `.github/workflows/deploy-azure.yml`

- [ ] **Step 1: Apply the new bootstrap root in a fresh temp workspace**

Run:

```bash
STAMP=$(date -u +%Y%m%d%H%M%S)
mkdir -p "tmp/azure-uami-$STAMP"
cp terraform/bootstrap/github_actions/terraform.tfvars.example "tmp/azure-uami-$STAMP/bootstrap.tfvars"
cp terraform/environments/sandbox/terraform.tfvars.example "tmp/azure-uami-$STAMP/sandbox.tfvars"
```

Edit `tmp/azure-uami-$STAMP/bootstrap.tfvars` to this shape:

```tf
subscription_id            = "97200cb2-456d-4471-876a-55f0a2bd8d54"
location                   = "northeurope"
backend_resource_group_name  = "fkukuck-fabro-tfstate"
backend_storage_account_name = "fkukuckfabrotfstate"
backend_container_name       = "tfstate"
backend_state_key            = "uami-$STAMP.tfstate"
github_repository            = "fkukuck/fabro"
github_environment_name      = "production"
github_actions_identity_name = "fkukuck-fabro-gha-production"

tags = {
  environment = "bootstrap"
  managed_by  = "terraform"
}
```

Then run:

```bash
terraform -chdir=terraform/bootstrap/github_actions init
terraform -chdir=terraform/bootstrap/github_actions apply -auto-approve -var-file="$(pwd)/tmp/azure-uami-$STAMP/bootstrap.tfvars"
```

Write `tmp/azure-uami-$STAMP/sandbox.tfvars` with this command:

```bash
SHORT=${STAMP:4:10}
cat > "tmp/azure-uami-$STAMP/sandbox.tfvars" <<EOF
subscription_id                         = "97200cb2-456d-4471-876a-55f0a2bd8d54"
location                                = "northeurope"
resource_group_name                     = "fkukuck-fabro-greenfield-$STAMP"
vnet_name                               = "fkukuck-fabro-vnet-$SHORT"
vnet_cidr                               = "10.42.0.0/16"
aca_subnet_name                         = "fkukuck-aca-subnet-$SHORT"
aca_subnet_cidr                         = "10.42.0.0/23"
aci_subnet_name                         = "fkukuck-aci-subnet-$SHORT"
aci_subnet_cidr                         = "10.42.2.0/24"
storage_account_name                    = "fkfabro$SHORT"
server_storage_share_name               = "fabrostorage"
acr_name                                = "fkukuckfabroacr$SHORT"
identity_name                           = "fkukuck-fabro-server-identity-$SHORT"
container_apps_environment_name         = "fkukuck-fabro-env-$SHORT"
container_apps_environment_storage_name = "fabrostorage"
fabro_server_name                       = "fkukuck-fabro-srv-$SHORT"
fabro_server_enabled                    = false
fabro_server_image                      = "example.azurecr.io/fabro-server:bootstrap"
fabro_server_cpu                        = 2
fabro_server_memory                     = "4Gi"
github_actions_principal_id             = "$(terraform -chdir=terraform/bootstrap/github_actions output -raw github_actions_principal_id)"

tags = {
  environment = "sandbox"
  managed_by  = "terraform"
}
EOF
```

Expected: the root creates the backend state resources if missing, the GitHub Actions UAMI, the federated credential, and the three backend storage RBAC assignments.

- [ ] **Step 2: Populate the GitHub `production` environment from bootstrap outputs and environment naming**

Run the bootstrap output handoff:

```bash
gh api --method PUT repos/fkukuck/fabro/environments/production > /dev/null
gh variable set AZURE_CLIENT_ID --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw github_actions_client_id)"
gh variable set AZURE_TENANT_ID --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw tenant_id)"
gh variable set AZURE_SUBSCRIPTION_ID --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw subscription_id)"
gh variable set AZURE_GITHUB_ACTIONS_PRINCIPAL_ID --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw github_actions_principal_id)"
gh variable set TF_BACKEND_RESOURCE_GROUP --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_resource_group_name)"
gh variable set TF_BACKEND_STORAGE_ACCOUNT --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_storage_account_name)"
gh variable set TF_BACKEND_CONTAINER --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_container_name)"
gh variable set TF_BACKEND_KEY --repo fkukuck/fabro --env production --body "$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_state_key)"
```

Then set the environment-specific Azure variables with `gh variable set ... --env production` for:

- `AZURE_LOCATION`
- `AZURE_RESOURCE_GROUP_NAME`
- `AZURE_VNET_NAME`
- `AZURE_VNET_CIDR`
- `AZURE_ACA_SUBNET_NAME`
- `AZURE_ACA_SUBNET_CIDR`
- `AZURE_ACI_SUBNET_NAME`
- `AZURE_ACI_SUBNET_CIDR`
- `AZURE_STORAGE_ACCOUNT_NAME`
- `AZURE_SERVER_STORAGE_SHARE_NAME`
- `AZURE_ACR_NAME`
- `AZURE_IDENTITY_NAME`
- `AZURE_CONTAINER_APPS_ENVIRONMENT_NAME`
- `AZURE_FABRO_SERVER_NAME`

Expected: `gh variable list --repo fkukuck/fabro --env production` shows every variable the workflow validates.

- [ ] **Step 3: Run the same greenfield deployment path the docs now promise**

Run the first environment apply, then the workflow, then install, then rerun:

```bash
terraform -chdir=terraform/environments/sandbox init \
  -backend-config="resource_group_name=$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_resource_group_name)" \
  -backend-config="storage_account_name=$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_storage_account_name)" \
  -backend-config="container_name=$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_container_name)" \
  -backend-config="key=$(terraform -chdir=terraform/bootstrap/github_actions output -raw backend_state_key)"

terraform -chdir=terraform/environments/sandbox apply -auto-approve -var-file="$(pwd)/tmp/azure-uami-$STAMP/sandbox.tfvars"

gh workflow run ".github/workflows/deploy-azure.yml" --repo fkukuck/fabro --ref "$(git branch --show-current)" -f ref="$(git branch --show-current)"
gh run watch --repo fkukuck/fabro --exit-status
```

Complete the install wizard with:

- Azure Blob object store
- Azure Container Instances sandbox runtime
- GitHub token auth path

Store the emitted token:

```bash
read -r -p "Wizard dev token: " WIZARD_TOKEN
gh secret set FABRO_DEPLOY_DEV_TOKEN --repo fkukuck/fabro --env production --body "$WIZARD_TOKEN"
```

Then rerun the workflow:

```bash
gh workflow run ".github/workflows/deploy-azure.yml" --repo fkukuck/fabro --ref "$(git branch --show-current)" -f ref="$(git branch --show-current)"
gh run watch --repo fkukuck/fabro --exit-status
```

Expected: workflow succeeds through rollout, authenticated API check, and Azure smoke workflow.

- [ ] **Step 4: Prove same-input Terraform steady state again after the UAMI migration**

Run:

```bash
DEPLOY_ID=$(az acr repository show-tags \
  --name "$AZURE_ACR_NAME" \
  --repository fabro-server \
  --orderby time_desc \
  --top 1 \
  -o tsv)

terraform -chdir=terraform/environments/sandbox plan -detailed-exitcode \
  -var-file="$(pwd)/tmp/azure-uami-$STAMP/sandbox.tfvars" \
  -var="fabro_server_enabled=true" \
  -var="fabro_server_image=$(terraform -chdir=terraform/environments/sandbox output -raw acr_login_server)/fabro-server:$DEPLOY_ID"
```

Expected: `No changes. Your infrastructure matches the configuration.` and exit code `0`.

- [ ] **Step 5: Fold the validation outcome into the durable docs**

Append a short note to `docs/public/administration/deploy-azure.mdx` under `CI validation checklist` that says the supported path was revalidated with:

```md
- GitHub Actions managed identity bootstrap
- Azure-backed remote Terraform state
- manual install through the Azure Blob + Azure Container Instances path
- authenticated API checks and Azure smoke workflow
- same-input follow-up Terraform no-op plan
```

- [ ] **Step 6: Commit**

```bash
git add docs/public/administration/deploy-azure.mdx terraform/bootstrap/github_actions/README.md terraform/environments/sandbox/README.md
git commit -m "test(azure): revalidate managed identity bootstrap path"
```

## Task 5: Delete Temporary Superpowers Artifacts After Durable Docs Are Complete

**Files:**
- Delete: `docs/superpowers/specs/2026-04-30-azure-github-uami-bootstrap-design.md`
- Delete: `docs/superpowers/specs/2026-04-30-azure-greenfield-followup-validation.md`
- Delete: `docs/superpowers/plans/2026-04-30-azure-github-uami-bootstrap-plan.md`
- Test: `README.md`

- [ ] **Step 1: Verify every durable Azure setup doc now carries the required knowledge**

Run:

```bash
rg -n "user-assigned managed identity|production environment|Storage Account Key Operator Service Role|FABRO_DEPLOY_DEV_TOKEN|Azure \+ GitHub Setup" README.md docs/public/administration/deploy-azure.mdx terraform/bootstrap/github_actions/README.md terraform/environments/sandbox/README.md
```

Expected: all required operator/setup concepts are present outside `docs/superpowers/`.

- [ ] **Step 2: Delete the temporary design, validation, and implementation-plan artifacts**

Apply this patch:

```diff
*** Begin Patch
*** Delete File: docs/superpowers/specs/2026-04-30-azure-github-uami-bootstrap-design.md
*** Delete File: docs/superpowers/specs/2026-04-30-azure-greenfield-followup-validation.md
*** Delete File: docs/superpowers/plans/2026-04-30-azure-github-uami-bootstrap-plan.md
*** End Patch
```

- [ ] **Step 3: Verify only durable docs remain**

Run:

```bash
test ! -e docs/superpowers/specs/2026-04-30-azure-github-uami-bootstrap-design.md
test ! -e docs/superpowers/specs/2026-04-30-azure-greenfield-followup-validation.md
test ! -e docs/superpowers/plans/2026-04-30-azure-github-uami-bootstrap-plan.md
```

Expected: all three commands exit `0`.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers
git commit -m "docs: remove temporary azure planning artifacts"
```
