# Azure Hosting

## Control Plane

Deploy `fabro-server` to Azure Container Apps with:

- exactly one active replica
- scale-to-zero disabled
- managed identity enabled

The first Azure sandbox branch assumes a singleton control plane. Do not run multiple active `fabro-server` replicas against the same Azure sandbox fleet yet.

## Required Environment Variables

- `FABRO_AZURE_SUBSCRIPTION_ID`
- `FABRO_AZURE_RESOURCE_GROUP`
- `FABRO_AZURE_LOCATION`
- `FABRO_AZURE_SANDBOX_SUBNET_ID`
- `FABRO_AZURE_STORAGE_ACCOUNT`
- `FABRO_AZURE_STORAGE_SHARE`
- `FABRO_AZURE_STORAGE_KEY`
- `FABRO_AZURE_ACR_SERVER`

## Optional Environment Variables

- `FABRO_AZURE_SANDBOXD_PORT`
- `AZURE_CLIENT_ID`
- `FABRO_AZURE_ACR_USERNAME`
- `FABRO_AZURE_ACR_PASSWORD`

## Sandbox Runtime

Workflow sandboxes run as Azure Container Instances with `/workspace` mounted from Azure Files.

The control plane provisions sandboxes through Azure ARM, waits for the in-sandbox `sandboxd` daemon, and then performs repo setup inside the container group.

## Validated Smoke-Test Path

The commands below are the greenfield path that was validated against the `azure-sandbox` branch after:

- `c646efec` `fix(sandbox): correct azure container group payload`
- `17b963da` `fix(sandbox): include azure files storage key`
- `0f843550` `fix(sandbox): lowercase azure container names`

This path proves the live Azure smoke test in `lib/crates/fabro-workflow/tests/it/azure_integration.rs` can create an Azure sandbox, wait for `sandboxd`, run `printf hello`, and clean up.

### 1. Log in and choose a subscription

Run this on the machine where you use `az`:

```bash
az login
az account show

export FABRO_AZURE_LOCATION="northeurope"
export FABRO_AZURE_RESOURCE_GROUP="fabro-azure-trial"
export FABRO_AZURE_STORAGE_SHARE="workspace"
export FABRO_AZURE_SANDBOXD_PORT="7777"

export FABRO_AZURE_SUBSCRIPTION_ID="$(az account show --query id -o tsv)"
az account set --subscription "$FABRO_AZURE_SUBSCRIPTION_ID"
```

### 2. Create the resource group and network

These commands create the resource group, the delegated ACI subnet used by Azure Container Instances, and a separate VM subnet for the build/test machine.

```bash
az group create --name "$FABRO_AZURE_RESOURCE_GROUP" --location "$FABRO_AZURE_LOCATION"

az network vnet create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name fabro-vnet \
  --address-prefix 10.10.0.0/16 \
  --subnet-name aci-subnet \
  --subnet-prefix 10.10.1.0/24

az network vnet subnet update \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --vnet-name fabro-vnet \
  --name aci-subnet \
  --delegations Microsoft.ContainerInstance/containerGroups

az network vnet subnet create \
  --subscription "$FABRO_AZURE_SUBSCRIPTION_ID" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --vnet-name fabro-vnet \
  --name vm-subnet \
  --address-prefixes 10.10.2.0/24

export FABRO_AZURE_SANDBOX_SUBNET_ID="$(az network vnet subnet show \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --vnet-name fabro-vnet \
  --name aci-subnet \
  --query id -o tsv)"
```

### 3. Create storage and Azure Container Registry

Use globally unique names for the storage account and registry. The examples below derive names from the current timestamp.

```bash
export FABRO_AZURE_STORAGE_ACCOUNT="fabro$(date +%s | tail -c 11)"
export FABRO_AZURE_ACR_NAME="fabroacr$(date +%s | tail -c 11)"

az storage account create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --location "$FABRO_AZURE_LOCATION" \
  --sku Standard_LRS

export FABRO_AZURE_STORAGE_KEY="$(az storage account keys list \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --account-name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --query '[0].value' -o tsv)"

az storage share create \
  --account-name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --account-key "$FABRO_AZURE_STORAGE_KEY" \
  --name "$FABRO_AZURE_STORAGE_SHARE"

az acr create \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name "$FABRO_AZURE_ACR_NAME" \
  --sku Basic \
  --admin-enabled true

export FABRO_AZURE_ACR_SERVER="$(az acr show --name "$FABRO_AZURE_ACR_NAME" --query loginServer -o tsv)"
export FABRO_AZURE_ACR_USERNAME="$(az acr credential show --name "$FABRO_AZURE_ACR_NAME" --query username -o tsv)"
export FABRO_AZURE_ACR_PASSWORD="$(az acr credential show --name "$FABRO_AZURE_ACR_NAME" --query 'passwords[0].value' -o tsv)"
```

At this point you have the full Azure environment expected by the current provider implementation:

```bash
export FABRO_AZURE_SUBSCRIPTION_ID
export FABRO_AZURE_RESOURCE_GROUP
export FABRO_AZURE_LOCATION
export FABRO_AZURE_SANDBOX_SUBNET_ID
export FABRO_AZURE_STORAGE_ACCOUNT
export FABRO_AZURE_STORAGE_SHARE
export FABRO_AZURE_STORAGE_KEY
export FABRO_AZURE_ACR_SERVER
export FABRO_AZURE_ACR_USERNAME
export FABRO_AZURE_ACR_PASSWORD
export FABRO_AZURE_SANDBOXD_PORT
```

### 4. Create the Azure VM used for the smoke test

The VM must use a non-delegated subnet. `aci-subnet` cannot host the VM NIC.

```bash
az vm create \
  --subscription "$FABRO_AZURE_SUBSCRIPTION_ID" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  --name fabro-azure-vm \
  --location "$FABRO_AZURE_LOCATION" \
  --image Canonical:0001-com-ubuntu-server-jammy:22_04-lts:latest \
  --size Standard_B2ls_v2 \
  --admin-username azureuser \
  --ssh-key-values "$HOME/.ssh/id_ed25519.pub" \
  --assign-identity \
  --vnet-name fabro-vnet \
  --subnet vm-subnet

export VM_IP="$(az vm show -d --resource-group "$FABRO_AZURE_RESOURCE_GROUP" --name fabro-azure-vm --query publicIps -o tsv)"
export VM_PRINCIPAL_ID="$(az vm show --resource-group "$FABRO_AZURE_RESOURCE_GROUP" --name fabro-azure-vm --query identity.principalId -o tsv)"

az role assignment create \
  --subscription "$FABRO_AZURE_SUBSCRIPTION_ID" \
  --assignee-object-id "$VM_PRINCIPAL_ID" \
  --assignee-principal-type ServicePrincipal \
  --role Contributor \
  --scope "/subscriptions/$FABRO_AZURE_SUBSCRIPTION_ID/resourceGroups/$FABRO_AZURE_RESOURCE_GROUP"

ssh azureuser@"$VM_IP"
```

### 5. Prepare the VM

Run the rest of the commands on the VM.

```bash
sudo apt-get update && \
sudo apt-get install -y build-essential pkg-config libssl-dev libarchive-dev git curl docker.io && \
sudo usermod -aG docker "$USER"

newgrp docker

curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup toolchain install nightly-2026-04-14 --profile minimal --component rustfmt,clippy
cargo install cargo-nextest --locked
```

### 6. Build the sandbox image from the branch under test

```bash
git clone <your-repo-url> fabro && \
cd fabro && \
git checkout azure-sandbox && \
git pull --ff-only

export FABRO_AZURE_SUBSCRIPTION_ID="<your-subscription-id>"
export FABRO_AZURE_RESOURCE_GROUP="<your-resource-group>"
export FABRO_AZURE_LOCATION="northeurope"
export FABRO_AZURE_SANDBOX_SUBNET_ID="<your-aci-subnet-id>"
export FABRO_AZURE_STORAGE_ACCOUNT="<your-storage-account>"
export FABRO_AZURE_STORAGE_SHARE="workspace"
export FABRO_AZURE_STORAGE_KEY="<your-storage-key>"
export FABRO_AZURE_ACR_SERVER="<your-acr-login-server>"
export FABRO_AZURE_ACR_USERNAME="<your-acr-username>"
export FABRO_AZURE_ACR_PASSWORD="<your-acr-password>"
export FABRO_AZURE_SANDBOXD_PORT="7777"

source "$HOME/.cargo/env"
cargo build --release -p fabro-sandboxd

mkdir -p .azure-image
cp target/release/fabro-sandboxd .azure-image/fabro-sandboxd

cat > .azure-image/Dockerfile <<'EOF'
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates git bash coreutils findutils grep curl && \
    rm -rf /var/lib/apt/lists/*
COPY fabro-sandboxd /usr/local/bin/fabro-sandboxd
WORKDIR /workspace
CMD ["fabro-sandboxd"]
EOF

docker login "$FABRO_AZURE_ACR_SERVER" -u "$FABRO_AZURE_ACR_USERNAME" -p "$FABRO_AZURE_ACR_PASSWORD"
docker build -f .azure-image/Dockerfile -t "$FABRO_AZURE_ACR_SERVER/fabro-sandboxes/base:trial" .azure-image
docker push "$FABRO_AZURE_ACR_SERVER/fabro-sandboxes/base:trial"

export FABRO_AZURE_TEST_IMAGE="$FABRO_AZURE_ACR_SERVER/fabro-sandboxes/base:trial"
```

### 7. Clean up stale container groups before the smoke test

The trial subscription used during validation had a `StandardCores` ACI quota of `4`. Two stuck `2 CPU` sandboxes will exhaust the quota and cause `ContainerGroupQuotaReached` on the next attempt.

Run this on the machine with `az` before re-running the smoke test if any old `fabro-*` groups are still present:

```bash
az container list \
  --subscription "$FABRO_AZURE_SUBSCRIPTION_ID" \
  --resource-group "$FABRO_AZURE_RESOURCE_GROUP" \
  -o table

az container delete --subscription "$FABRO_AZURE_SUBSCRIPTION_ID" --resource-group "$FABRO_AZURE_RESOURCE_GROUP" --name "<stale-fabro-container-group>" --yes
```

### 8. Run the validated live smoke test

Run this on the VM:

```bash
source "$HOME/.cargo/env"
cargo test -p fabro-workflow --test it azure_integration::azure_exec_command_round_trip -- --ignored --exact --nocapture
```

Expected result:

```text
test azure_integration::azure_exec_command_round_trip ... ok
```

## Validated Server-Hosted Path

The smoke test above only proves raw Azure sandbox creation and command execution. The server-hosted path below is the validated end-to-end path for running a real repository through a VM-hosted `fabro-server` and Azure Container Instance sandboxes.

This path validates:

- `fabro-server` running on an Azure VM
- the local CLI submitting runs to the VM over `--server`
- private-repo cloning into Azure sandboxes
- workflow-scoped GitHub token injection for `gh` CLI usage via `run.scm.github.permissions`
- custom Azure workflow images built from a repo-owned `snapshot.Dockerfile`

### 1. Use a repo you control

The target repository should live under a GitHub user or organization you control. The validated path used a private repository under a personal account so the VM's GitHub token could clone it and the workflow could call `gh` against it.

At minimum, the repo should contain:

- `.fabro/workflows/<name>/workflow.fabro`
- `.fabro/workflows/<name>/workflow.toml`
- any helper scripts referenced by the workflow under `.fabro/workflows/<name>/scripts/`

### 2. Shape the repo workflow config for Azure

For remote Azure runs, the checked-in workflow must express Azure runtime settings in the run domain, not in workflow-local `server.*` stanzas.

Validated pattern:

```toml
_version = 1

[workflow]
graph = "workflow.fabro"

[run.sandbox]
provider = "azure"
preserve = false

[run.sandbox.azure]
cpu = 4.0
memory_gb = 8.0
image = "<acr-login-server>/fabro-sandboxes/software-factory:20260419-2"

[run.scm.github.permissions]
contents = "write"
issues = "read"
pull_requests = "write"
```

Notes:

- `run.scm.github.permissions` is the run-owned request that survives remote-run persistence and drives sandbox `GITHUB_TOKEN` injection.
- Workflow and project `server.*` stanzas remain owner-scoped and inert in remote-server mode.
- Set `run.sandbox.azure.image` explicitly in the checked-in workflow TOML. Do not rely on a server-local default image for remote runs.

### 3. Build a workflow-specific Azure image when the workflow needs extra tools

Azure does not automatically build or use `.fabro/workflows/<name>/snapshot.Dockerfile`. If the workflow needs tools beyond the generic Azure base image, build and push a custom image yourself, then point `run.sandbox.azure.image` at it.

The validated workflow image had to include:

- `gh`
- `jq`
- `python3`
- `node`
- browser tooling

Important: start the workflow image from Fabro's Azure base image so `fabro-sandboxd` is still present.

Validated Dockerfile pattern:

```dockerfile
FROM <acr-login-server>/fabro-sandboxes/base:trial

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    gnupg \
    jq \
    python3 \
    python3-pip \
    unzip \
  && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
  && apt-get update \
  && apt-get install -y nodejs \
  && npm install -g npm@10 \
  && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/apt/keyrings \
  && curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | dd of=/etc/apt/keyrings/githubcli-archive-keyring.gpg \
  && chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg \
  && echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" > /etc/apt/sources.list.d/github-cli.list \
  && apt-get update \
  && apt-get install -y gh \
  && rm -rf /var/lib/apt/lists/*
```

Build and push from the repo root on a machine with Docker access:

```bash
docker login "$FABRO_AZURE_ACR_SERVER" -u "$FABRO_AZURE_ACR_USERNAME" -p "$FABRO_AZURE_ACR_PASSWORD"

docker buildx build \
  --platform linux/amd64 \
  -f ".fabro/workflows/software-factory/snapshot.Dockerfile" \
  -t "$FABRO_AZURE_ACR_SERVER/fabro-sandboxes/software-factory:20260419-2" \
  --push .
```

### 4. Install the same Fabro branch on both the laptop and the VM

When you are testing branch-specific workflow schema or Azure behavior, install the same `fabro` build on:

- the laptop that runs `fabro run --server ...`
- the Azure VM that runs `fabro server start`

On both machines:

```bash
cargo install --path lib/crates/fabro-cli --force
hash -r
type -a fabro
```

This matters because the local CLI parses `workflow.toml` before it uploads anything to the remote server. A stale local binary can reject config that the VM already understands.

### 5. Create the minimal server config on the VM

The CLI/server path expects `~/.fabro/settings.toml` to exist.

Validated minimal server shape for the token-based path:

```toml
_version = 1

[run.sandbox]
provider = "azure"

[server.auth]
methods = ["dev-token"]

[server.integrations.github]
strategy = "token"
```

Notes:

- `strategy = "token"` makes the VM use `GITHUB_TOKEN` from its own environment as the server-side GitHub credential source.
- Sandbox token scope is requested from the workflow via `run.scm.github.permissions`, not from `server.integrations.github.permissions`.
- If you also want browser GitHub login for the embedded web UI, configure the GitHub App flow as described in `docs/integrations/github.mdx`, but keep workflow token requests under `run.scm.github.permissions`.

### 6. Keep `dev-token` enabled for CLI access

`fabro run --server ...` is a separate API client and must authenticate to the remote server independently.

On the VM:

```bash
cat ~/.fabro/dev-token
```

On the laptop:

```bash
export FABRO_DEV_TOKEN='fabro_dev_...'
```

### 7. Start the server from a shell that already has Azure, GitHub, and LLM env vars

The server process reads Azure platform configuration, GitHub token credentials, and LLM keys from its own environment. Export these variables before starting `fabro server start`:

```bash
export FABRO_AZURE_SUBSCRIPTION_ID="..."
export FABRO_AZURE_RESOURCE_GROUP="..."
export FABRO_AZURE_LOCATION="..."
export FABRO_AZURE_SANDBOX_SUBNET_ID="..."
export FABRO_AZURE_STORAGE_ACCOUNT="..."
export FABRO_AZURE_STORAGE_SHARE="workspace"
export FABRO_AZURE_STORAGE_KEY="..."
export FABRO_AZURE_ACR_SERVER="..."
export FABRO_AZURE_ACR_USERNAME="..."
export FABRO_AZURE_ACR_PASSWORD="..."
export FABRO_AZURE_SANDBOXD_PORT="7777"

export GITHUB_TOKEN="..."

export OPENAI_API_KEY="..."
# or
export ANTHROPIC_API_KEY="..."
```

Then start or restart the server from that same shell:

```bash
fabro server stop
fabro server start --bind 127.0.0.1:3000
```

### 8. Run the remote server UI through SSH port forwarding when needed

For server-hosted validation, the embedded UI served by `fabro server start` was sufficient. `bun run dev` was not required on the VM.

From your laptop:

```bash
ssh -L 3000:127.0.0.1:3000 azureuser@<vm-ip-or-host>
```

Open:

```text
http://localhost:3000
```

Only install Bun on the VM if you want to modify `apps/fabro-web` and rebuild frontend assets there.

### 9. Run the workflow from the laptop

From the repo on the laptop:

```bash
fabro run .fabro/workflows/software-factory/workflow.toml --server http://localhost:3000/api/v1
```

### 10. Use a fresh Azure Files share when isolating retries

Azure sandboxes mount `/workspace` from Azure Files, and the current Azure provider reuses that mounted workspace if `.git` is already present.

For clean retries while debugging clone, image, or startup problems, create a fresh share and restart the server with it:

```bash
az storage share create \
  --account-name "$FABRO_AZURE_STORAGE_ACCOUNT" \
  --account-key "$FABRO_AZURE_STORAGE_KEY" \
  --name workspace-fresh-1

export FABRO_AZURE_STORAGE_SHARE="workspace-fresh-1"
fabro server start --bind 127.0.0.1:3000
```

This avoids stale `/workspace` contents from masking newer code or config changes during validation.

### 11. Azure-specific behaviors confirmed during validation

The current `azure-sandbox` branch validated the following Azure-specific fixes and behaviors:

- Azure Container Instance rejects mixed-case container names, so run-based sandbox names must be lowercased before provisioning.
- Azure startup should wait for a reachable sandbox IP, not treat `0.0.0.0` as ready.
- Custom workflow images must keep `fabro-sandboxd` in `PATH`, which is why repo-owned workflow images should start from `fabro-sandboxes/base:trial`.
- Trial subscriptions can have very small ACI quota. Clean up stale `fabro-*` container groups before retrying failed runs.

### 12. Remaining warning seen during validation

The validated real-run path still emitted this warning before sandbox startup:

```text
Failed to push fabro-software-factory to origin: Engine error: git push failed: No such file or directory (os error 2)
```

That warning did not block Azure sandbox creation or `gh` usage inside the sandbox, but it indicates the branch still has at least one unresolved remote checkpoint or push-path issue.
