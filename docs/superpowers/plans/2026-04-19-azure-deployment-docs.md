# Azure Deployment Docs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the legacy Azure hosting runbook with a first-class Azure deployment guide that matches Fabro's existing deployment docs style, includes concrete `az` commands, and removes the old page after migration.

**Architecture:** Keep `docs/administration/deploy-server.mdx` generic, add a new `docs/administration/deploy-azure.mdx` that owns the Azure control-plane and sandbox-plane story, and wire the page into the Deployment navigation. Migrate the durable operator knowledge from `docs/administration/azure-hosting.md`, strip branch-history material, and finish with a docs smoke test through the Mintlify dev server.

**Tech Stack:** Mintlify MDX docs, JSON navigation config, Azure CLI command examples, Docker-based Mintlify dev server.

---

## File Map

- Create: `docs/administration/deploy-azure.mdx`
  Purpose: canonical Azure deployment guide with concrete `az` commands, Container Apps hosting steps, ACI sandbox prerequisites, remote-run flow, validation, and caveats.
- Modify: `docs/administration/deploy-server.mdx`
  Purpose: add one small Azure card in `## Next steps` without turning the page into a provider-specific runbook.
- Modify: `docs/docs.json`
  Purpose: expose the new Azure guide under the Deployment section.
- Delete: `docs/administration/azure-hosting.md`
  Purpose: remove the legacy branch-oriented runbook after migrating its durable operator guidance.

## Task 1: Create The Azure Deployment Guide Skeleton And Infrastructure Sections

**Files:**
- Create: `docs/administration/deploy-azure.mdx`

- [ ] **Step 1: Run the failing existence check**

Run:

```bash
test -f docs/administration/deploy-azure.mdx && rg '^## 4\. Create the Container Apps environment$' docs/administration/deploy-azure.mdx
```

Expected: FAIL because `docs/administration/deploy-azure.mdx` does not exist yet.

- [ ] **Step 2: Create the new file with frontmatter, overview, prerequisites, and infrastructure setup**

Create `docs/administration/deploy-azure.mdx` with this exact starting content:

````mdx
---
title: "Azure"
description: "Deploy Fabro to Azure Container Apps with Azure Container Instances for workflow sandboxes"
---

<Warning>
  The server interface is in private early access. Contact [bryan@qlty.sh](mailto:bryan@qlty.sh) if you're interested in trying it.
</Warning>

Azure is a good fit for hosting Fabro when you want Azure-native infrastructure for both the long-running control plane and the workflow sandbox runtime. In this setup, `fabro-server` runs in Azure Container Apps, while workflow sandboxes run as Azure Container Instances provisioned by the server.

This guide documents the current production-ready Azure topology for Fabro:

- Azure Container Apps hosts `fabro-server`
- Azure Container Instances run workflow sandboxes
- Azure Files backs sandbox workspace state at `/workspace`
- Azure-backed storage mounted at `/storage` keeps Fabro server state across redeploys
- Azure Container Registry stores the sandbox images referenced by Azure runs

<Note>
  Fabro's server currently assumes a single active writer on `/storage`. Keep the Azure Container App at exactly one replica.
</Note>

## Reference architecture

- **Control plane:** `fabro-server` runs as a singleton Azure Container App.
- **Sandbox plane:** each workflow run provisions an Azure Container Instance container group.
- **Workspace plane:** Azure Files backs `/workspace` for sandboxes.
- **State plane:** Azure storage mounted at `/storage` persists runs, checkpoints, sessions, and auth state for the server.
- **Image plane:** Azure Container Registry stores the base sandbox image and any workflow-specific sandbox images.

## First-deploy checklist

- Create a resource group, VNet, and the subnets required by Container Apps and ACI.
- Create a storage account and file shares for `/storage` and `/workspace`.
- Create an Azure Container Registry.
- Create a managed identity and grant it access to manage Azure resources in the deployment resource group.
- Deploy `fabro-server` to Azure Container Apps with exactly one replica.
- Configure the Azure sandbox provider environment variables, server auth secrets, GitHub token, and LLM API keys.
- Build and push the Azure sandbox image to ACR.
- Run a remote workflow through the hosted server and confirm an Azure sandbox starts successfully.

## Prerequisites

- An Azure subscription
- The latest [`az`](https://learn.microsoft.com/cli/azure/install-azure-cli) CLI
- Docker with `buildx` support on the machine that will build sandbox images
- A GitHub token if your workflows need to clone private repos or use `gh`
- At least one LLM provider key such as `OPENAI_API_KEY` or `ANTHROPIC_API_KEY`

## 1. Sign in and choose names

Run these commands on the machine where you use `az`:

```bash
az login
az extension add -n containerapp --upgrade
az provider register --namespace Microsoft.App
az provider register --namespace Microsoft.OperationalInsights

export FABRO_AZURE_LOCATION="northeurope"
export FABRO_AZURE_RESOURCE_GROUP="fabro-azure-prod"
export FABRO_AZURE_SUBSCRIPTION_ID="$(az account show --query id -o tsv)"
export FABRO_AZURE_STORAGE_SHARE="workspace"
export FABRO_AZURE_SERVER_STORAGE_SHARE="fabro-storage"
export FABRO_AZURE_SANDBOXD_PORT="7777"
export ACA_ENV_NAME="fabro-env"
export ACA_APP_NAME="fabro-server"
export FABRO_AZURE_IDENTITY_NAME="fabro-server-identity"

export FABRO_AZURE_STORAGE_ACCOUNT="fabro$(date +%s | tail -c 11)"
export FABRO_AZURE_ACR_NAME="fabroacr$(date +%s | tail -c 11)"

az account set --subscription "$FABRO_AZURE_SUBSCRIPTION_ID"
```

## 2. Create the resource group and network

Create a VNet with one subnet for Azure Container Apps and a delegated subnet for Azure Container Instances:

```bash
az group create \
  --name "$FABRO_AZURE_RESOURCE_GROUP" \
  --location "$FABRO_AZURE_LOCATION"

az network vnet create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name fabro-vnet \
  --address-prefix 10.10.0.0/16 \
  --subnet-name aca-subnet \
  --subnet-prefix 10.10.0.0/23

az network vnet subnet create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --vnet-name fabro-vnet \
  --name aci-subnet \
  --address-prefixes 10.10.2.0/24

az network vnet subnet update \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --vnet-name fabro-vnet \
  --name aci-subnet \
  --delegations Microsoft.ContainerInstance/containerGroups

export ACA_SUBNET_ID="$(az network vnet subnet show \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --vnet-name fabro-vnet \
  --name aca-subnet \
  --query id -o tsv)"

export FABRO_AZURE_SANDBOX_SUBNET_ID="$(az network vnet subnet show \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --vnet-name fabro-vnet \
  --name aci-subnet \
  --query id -o tsv)"
```

## 3. Create storage and Azure Container Registry

Create one storage account, then create one file share for Fabro server state and one for Azure sandbox workspaces:

```bash
az storage account create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --location "$FABRO_AZURE_LOCATION" \
  --sku Standard_LRS \
  --kind StorageV2 \
  --enable-large-file-share

export FABRO_AZURE_STORAGE_KEY="$(az storage account keys list \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --account-name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --query '[0].value' -o tsv)"

az storage share create \
  --account-name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --account-key "$FABRO_AZURE_STORAGE_KEY" \
  --name "$FABRO_AZURE_STORAGE_SHARE"

az storage share create \
  --account-name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --account-key "$FABRO_AZURE_STORAGE_KEY" \
  --name "$FABRO_AZURE_SERVER_STORAGE_SHARE"

az acr create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name "$FABRO_AZURE_ACR_NAME" \
  --location "$FABRO_AZURE_LOCATION" \
  --sku Basic \
  --admin-enabled true

export FABRO_AZURE_ACR_SERVER="$(az acr show \
  --name "$FABRO_AZURE_ACR_NAME" \
  --query loginServer -o tsv)"

export FABRO_AZURE_ACR_USERNAME="$(az acr credential show \
  --name "$FABRO_AZURE_ACR_NAME" \
  --query username -o tsv)"

export FABRO_AZURE_ACR_PASSWORD="$(az acr credential show \
  --name "$FABRO_AZURE_ACR_NAME" \
  --query 'passwords[0].value' -o tsv)"
```

## 4. Create the Container Apps environment

Create a user-assigned identity for the Fabro server and grant it `Contributor` on the deployment resource group so it can provision and delete Azure sandboxes:

```bash
az identity create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name "$FABRO_AZURE_IDENTITY_NAME"

export FABRO_AZURE_IDENTITY_ID="$(az identity show \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name "$FABRO_AZURE_IDENTITY_NAME" \
  --query id -o tsv)"

export FABRO_AZURE_PRINCIPAL_ID="$(az identity show \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name "$FABRO_AZURE_IDENTITY_NAME" \
  --query principalId -o tsv)"

az role assignment create \
  --assignee-object-id "$FABRO_AZURE_PRINCIPAL_ID" \
  --assignee-principal-type ServicePrincipal \
  --role Contributor \
  --scope "/subscriptions/$FABRO_AZURE_SUBSCRIPTION_ID/resourceGroups/$FABRO_AZURE_RESOURCE_GROUP"

az containerapp env create \
  --name "$ACA_ENV_NAME" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --location "$FABRO_AZURE_LOCATION" \
  --infrastructure-subnet-resource-id "$ACA_SUBNET_ID"

az containerapp env storage set \
  --name "$ACA_ENV_NAME" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --storage-name fabrostorage \
  --access-mode ReadWrite \
  --azure-file-account-name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --azure-file-account-key "$FABRO_AZURE_STORAGE_KEY" \
  --azure-file-share-name "$FABRO_AZURE_SERVER_STORAGE_SHARE"
```
````

- [ ] **Step 3: Verify the new page contains the infrastructure headings**

Run:

```bash
rg '^title: "Azure"$|^## Reference architecture$|^## 4\. Create the Container Apps environment$' docs/administration/deploy-azure.mdx
```

Expected: PASS with matches for the frontmatter title and the three section headings.

- [ ] **Step 4: Commit the first half of the guide**

```bash
git add docs/administration/deploy-azure.mdx
git commit -m "docs: add Azure deployment guide scaffold"
```

## Task 2: Finish The Azure Guide With Server Deployment, Remote Runs, And Validation

**Files:**
- Modify: `docs/administration/deploy-azure.mdx`

- [ ] **Step 1: Run the failing late-section check**

Run:

```bash
rg '^## 5\. Deploy `fabro-server` to Azure Container Apps$|^## 10\. Troubleshooting and caveats$' docs/administration/deploy-azure.mdx
```

Expected: FAIL because the latter sections are not in the file yet.

- [ ] **Step 2: Append the deployment, image, workflow, validation, and caveat sections**

Append these exact sections to `docs/administration/deploy-azure.mdx`:

````mdx
## 5. Deploy `fabro-server` to Azure Container Apps

Use the public Fabro server image from GHCR, keep the app at one replica, and inject the Azure sandbox provider configuration through environment variables.

```bash
export FABRO_IMAGE="ghcr.io/fabro-sh/fabro:nightly"
export FABRO_DEV_TOKEN="fabro_dev_replace_this_with_a_real_token"
export SESSION_SECRET="$(openssl rand -hex 32)"

# Set one provider key before creating the app.
export OPENAI_API_KEY="replace-me"
# or
# export ANTHROPIC_API_KEY="replace-me"

# Optional, but recommended when workflows need private repo access or `gh`.
export GITHUB_TOKEN="replace-me"

az containerapp create \
  --name "$ACA_APP_NAME" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --environment "$ACA_ENV_NAME" \
  --image "$FABRO_IMAGE" \
  --ingress external \
  --target-port 32276 \
  --min-replicas 1 \
  --max-replicas 1 \
  --cpu 1.0 \
  --memory 2.0Gi \
  --user-assigned "$FABRO_AZURE_IDENTITY_ID" \
  --env-vars \
    FABRO_DEV_TOKEN="$FABRO_DEV_TOKEN" \
    SESSION_SECRET="$SESSION_SECRET" \
    FABRO_AZURE_SUBSCRIPTION_ID="$FABRO_AZURE_SUBSCRIPTION_ID" \
    FABRO_AZURE_RESOURCE_GROUP="$FABRO_AZURE_RESOURCE_GROUP" \
    FABRO_AZURE_LOCATION="$FABRO_AZURE_LOCATION" \
    FABRO_AZURE_SANDBOX_SUBNET_ID="$FABRO_AZURE_SANDBOX_SUBNET_ID" \
    FABRO_AZURE_STORAGE_ACCOUNT="$FABRO_AZURE_STORAGE_ACCOUNT" \
    FABRO_AZURE_STORAGE_SHARE="$FABRO_AZURE_STORAGE_SHARE" \
    FABRO_AZURE_STORAGE_KEY="$FABRO_AZURE_STORAGE_KEY" \
    FABRO_AZURE_ACR_SERVER="$FABRO_AZURE_ACR_SERVER" \
    FABRO_AZURE_ACR_USERNAME="$FABRO_AZURE_ACR_USERNAME" \
    FABRO_AZURE_ACR_PASSWORD="$FABRO_AZURE_ACR_PASSWORD" \
    FABRO_AZURE_SANDBOXD_PORT="$FABRO_AZURE_SANDBOXD_PORT" \
    GITHUB_TOKEN="$GITHUB_TOKEN" \
    OPENAI_API_KEY="$OPENAI_API_KEY"
```

The command above creates the app and configures ingress, identity, and environment variables, but it does not yet mount the Azure Files share at `/storage`. Export the app YAML, add the volume definition, and update the app:

```bash
az containerapp show \
  --name "$ACA_APP_NAME" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --output yaml > fabro-server.yaml
```

Update the `template` section in `fabro-server.yaml` so it includes this exact volume mount and volume definition:

```yaml
template:
  containers:
  - image: ghcr.io/fabro-sh/fabro:nightly
    name: fabro-server
    resources:
      cpu: 1.0
      memory: 2Gi
    volumeMounts:
    - mountPath: /storage
      volumeName: fabro-storage-volume
  scale:
    maxReplicas: 1
    minReplicas: 1
  volumes:
  - name: fabro-storage-volume
    storageName: fabrostorage
    storageType: AzureFile
```

Then apply the updated YAML:

```bash
az containerapp update \
  --name "$ACA_APP_NAME" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --yaml fabro-server.yaml

export FABRO_SERVER_FQDN="$(az containerapp show \
  --name "$ACA_APP_NAME" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --query properties.configuration.ingress.fqdn -o tsv)"

export FABRO_SERVER_URL="https://$FABRO_SERVER_FQDN"
```

## 6. Configure server settings and secrets

The server container already uses `/storage` for persistent state. For Azure-hosted runs, the important distinction is:

- **Server runtime secrets:** injected into the Container App as environment variables
- **Workflow-owned Azure runtime settings:** kept in checked-in `workflow.toml`

If you want to carry a custom `settings.toml` into your own Azure-hosted image or startup wrapper, start from this minimal shape:

```toml
_version = 1

[run.sandbox]
provider = "azure"

[server.auth]
methods = ["dev-token"]

[server.integrations.github]
strategy = "token"
```

The runtime environment variables required by the Azure sandbox provider are:

| Variable | Purpose |
|---|---|
| `FABRO_AZURE_SUBSCRIPTION_ID` | Azure subscription used for sandbox provisioning |
| `FABRO_AZURE_RESOURCE_GROUP` | Resource group that owns the ACI sandboxes |
| `FABRO_AZURE_LOCATION` | Azure region for sandboxes |
| `FABRO_AZURE_SANDBOX_SUBNET_ID` | Delegated subnet resource ID for Azure Container Instances |
| `FABRO_AZURE_STORAGE_ACCOUNT` | Storage account backing Azure Files |
| `FABRO_AZURE_STORAGE_SHARE` | Azure Files share mounted as `/workspace` inside sandboxes |
| `FABRO_AZURE_STORAGE_KEY` | Storage account key used by the current provider implementation |
| `FABRO_AZURE_ACR_SERVER` | ACR login server used by sandbox images |
| `FABRO_AZURE_SANDBOXD_PORT` | Port served by `fabro-sandboxd` inside the sandbox |
| `FABRO_AZURE_ACR_USERNAME` / `FABRO_AZURE_ACR_PASSWORD` | Optional but recommended registry credentials for private ACR images |
| `AZURE_CLIENT_ID` | Optional client ID when using a user-assigned managed identity |

## 7. Build sandbox images for Azure runs

The server image can come from GHCR, but the current Azure sandbox provider expects sandbox images to be available in your own ACR. Build and push a base image that contains `fabro-sandboxd`:

```bash
az acr login --name "$FABRO_AZURE_ACR_NAME"

cargo build --release -p fabro-sandboxd

mkdir -p .azure-image
cp target/release/fabro-sandboxd .azure-image/fabro-sandboxd
```

Create `.azure-image/Dockerfile` with this exact content:

```dockerfile
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git bash coreutils findutils grep curl \
    && rm -rf /var/lib/apt/lists/*
COPY fabro-sandboxd /usr/local/bin/fabro-sandboxd
WORKDIR /workspace
CMD ["fabro-sandboxd"]
```

Build and push the base image:

```bash
docker buildx build \
  --platform linux/amd64 \
  -f .azure-image/Dockerfile \
  -t "$FABRO_AZURE_ACR_SERVER/fabro-sandboxes/base:latest" \
  --push .azure-image
```

If a workflow needs extra tools, build a workflow-specific image that starts from the Fabro Azure base image:

```dockerfile
FROM <acr-login-server>/fabro-sandboxes/base:latest

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    jq \
    python3 \
    python3-pip \
    unzip \
  && rm -rf /var/lib/apt/lists/*
```

Build and push that image from the repo root:

```bash
docker buildx build \
  --platform linux/amd64 \
  -f .fabro/workflows/software-factory/snapshot.Dockerfile \
  -t "$FABRO_AZURE_ACR_SERVER/fabro-sandboxes/software-factory:latest" \
  --push .
```

## 8. Run your first workflow through the hosted server

For Azure-hosted remote runs, keep the Azure runtime settings in the run domain of the checked-in workflow file:

```toml
_version = 1

[workflow]
graph = "workflow.fabro"

[run.sandbox]
provider = "azure"
preserve = false

[run.sandbox.azure]
cpu = 2.0
memory_gb = 4.0
image = "<acr-login-server>/fabro-sandboxes/base:latest"

[run.scm.github.permissions]
contents = "write"
pull_requests = "write"
```

Point your local CLI at the hosted server:

```toml title="~/.fabro/settings.toml"
[cli.target]
type = "http"
url = "https://<your-container-app-fqdn>/api/v1"
```

Then authenticate and run a workflow from your laptop:

```bash
export FABRO_DEV_TOKEN="$FABRO_DEV_TOKEN"

fabro model list --server "$FABRO_SERVER_URL/api/v1"
fabro run .fabro/workflows/software-factory/workflow.toml --server "$FABRO_SERVER_URL/api/v1"
```

## 9. Validation

Use this sequence to validate the full Azure deployment path:

```bash
curl -sSf "$FABRO_SERVER_URL/health"

curl -sSf \
  -H "Authorization: Bearer $FABRO_DEV_TOKEN" \
  "$FABRO_SERVER_URL/api/v1/models"

fabro model list --server "$FABRO_SERVER_URL/api/v1"

az container list \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --query '[].name' -o table
```

Expected results:

1. `curl -sSf "$FABRO_SERVER_URL/health"` returns `ok`
2. the authenticated `/api/v1/models` request returns JSON instead of `401`
3. `fabro model list` succeeds against the hosted server
4. the workflow run creates at least one `fabro-*` Azure Container Instance during execution

## 10. Troubleshooting and caveats

- **Single replica only.** Keep `--min-replicas 1` and `--max-replicas 1` on the Container App.
- **ACI names must be lowercase.** Azure Container Instances reject mixed-case container group names.
- **ACI quota is easy to exhaust in trial subscriptions.** Clean up stale `fabro-*` container groups before retrying failed runs.
- **Set `run.sandbox.azure.image` explicitly.** Do not rely on a server-local default image for remote runs.
- **Custom images must inherit from the Fabro Azure base image.** `fabro-sandboxd` must remain on `PATH` inside the sandbox.
- **Azure Files can retain stale workspace state.** When isolating retries, create a fresh file share and restart the server with that value in `FABRO_AZURE_STORAGE_SHARE`.

To clean up stuck sandboxes, list and delete stale container groups:

```bash
az container list \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  -o table

az container delete \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name <stale-fabro-container-group> \
  --yes
```
````

- [ ] **Step 3: Verify the finished guide exposes the required late sections and command families**

Run:

```bash
rg '^## 5\. Deploy `fabro-server` to Azure Container Apps$|^## 10\. Troubleshooting and caveats$|az containerapp env storage set|az containerapp create|fabro run .* --server' docs/administration/deploy-azure.mdx
```

Expected: PASS with matches for both headings and all three command patterns.

- [ ] **Step 4: Commit the completed Azure guide**

```bash
git add docs/administration/deploy-azure.mdx
git commit -m "docs: document Azure deployment"
```

## Task 3: Wire Azure Into Deployment Navigation And The Generic Server Page

**Files:**
- Modify: `docs/docs.json`
- Modify: `docs/administration/deploy-server.mdx`

- [ ] **Step 1: Run the failing navigation check**

Run:

```bash
rg '/administration/deploy-azure|administration/deploy-azure' docs/administration/deploy-server.mdx docs/docs.json
```

Expected: FAIL because neither file references the new Azure guide yet.

- [ ] **Step 2: Add the Azure page to `docs/docs.json`**

Update the Deployment group in `docs/docs.json` so the `pages` array becomes:

```json
"pages": [
  "administration/deploy-server",
  "administration/deploy-azure",
  "administration/deploy-railway",
  "administration/deploy-render",
  "administration/deploy-fly-io",
  "administration/deploy-digital-ocean"
]
```

- [ ] **Step 3: Add one small Azure card to `deploy-server.mdx`**

In `docs/administration/deploy-server.mdx`, update `## Next steps` to include this exact card after the Railway card:

```mdx
  <Card title="Deploy to Azure" icon="cloud" href="/administration/deploy-azure">
    Host Fabro on Azure Container Apps with Azure Container Instances for workflow sandboxes.
  </Card>
```

Do not change any other section of `deploy-server.mdx`.

- [ ] **Step 4: Verify the navigation wiring**

Run:

```bash
rg '/administration/deploy-azure|administration/deploy-azure' docs/administration/deploy-server.mdx docs/docs.json
```

Expected: PASS with one match in `docs/administration/deploy-server.mdx` and one match in `docs/docs.json`.

- [ ] **Step 5: Commit the navigation changes**

```bash
git add docs/docs.json docs/administration/deploy-server.mdx
git commit -m "docs: add Azure deployment navigation"
```

## Task 4: Remove The Legacy Azure Page And Run A Docs Smoke Test

**Files:**
- Delete: `docs/administration/azure-hosting.md`

- [ ] **Step 1: Run the failing deletion check**

Run:

```bash
test ! -f docs/administration/azure-hosting.md
```

Expected: FAIL because the legacy file still exists.

- [ ] **Step 2: Delete the legacy page**

Delete `docs/administration/azure-hosting.md` with this exact patch:

```diff
*** Begin Patch
*** Delete File: docs/administration/azure-hosting.md
*** End Patch
```

- [ ] **Step 3: Verify the old page is gone and no longer referenced by live docs pages**

Run:

```bash
test ! -f docs/administration/azure-hosting.md && ! rg -n 'azure-hosting' docs/administration docs/docs.json
```

Expected: PASS with exit status `0` and no `rg` output.

- [ ] **Step 4: Run the Mintlify smoke test for the new page and the updated server page**

Run:

```bash
docker run --rm -d -p 3333:3333 -v "$(pwd)/docs:/docs" -w /docs --name mintlify-dev node:22-slim \
  bash -c "npx mintlify dev --host 0.0.0.0 --port 3333"

curl -sSf http://127.0.0.1:3333/administration/deploy-azure | grep -q "Deploy Fabro to Azure Container Apps"
curl -sSf http://127.0.0.1:3333/administration/deploy-server | grep -q "Deploy to Azure"

docker stop mintlify-dev
```

Expected: all commands exit successfully, the Azure page renders, and the generic server page exposes the new Azure card.

- [ ] **Step 5: Commit the cleanup and verification pass**

```bash
git add docs/administration/deploy-azure.mdx docs/administration/deploy-server.mdx docs/docs.json
git rm docs/administration/azure-hosting.md
git commit -m "docs: replace legacy Azure hosting guide"
```

## Self-Review Checklist

- Spec coverage:
  - `deploy-azure.mdx` is created and owns the Azure hosting story: covered by Tasks 1 and 2.
  - `deploy-server.mdx` stays generic with only a small Azure card: covered by Task 3.
  - `docs/docs.json` exposes Azure in Deployment navigation: covered by Task 3.
  - `azure-hosting.md` is removed after migration: covered by Task 4.
  - concrete `az` commands are part of the guide: covered by Tasks 1 and 2.
  - validation and caveats are explicit and production-oriented: covered by Task 2.
- Placeholder scan:
  - no deferred implementation markers remain in this plan.
  - remaining angle-bracket values are intentional documentation literals inside the final Azure guide examples.
- Consistency:
  - all docs edits are limited to the four scoped files from the approved spec.
  - the new page name is consistently `deploy-azure.mdx` and the route is consistently `/administration/deploy-azure`.
