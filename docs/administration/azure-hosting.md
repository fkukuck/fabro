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

The smoke test above only proves raw Azure sandbox creation and command execution. The server-hosted path below was also validated far enough to:

- start `fabro-server` on an Azure VM
- access the embedded web UI over SSH port forwarding
- configure GitHub App browser auth
- authenticate the local CLI to the remote server
- submit a real run that provisions an Azure sandbox

### 1. Use a repo you can install the GitHub App on

If the target repository is private and you do not control the owning organization, create a private repo under a personal account you control and push the code there first.

The current Azure validation path assumes you can:

- install the GitHub App on the repository
- let Fabro mint repo-scoped credentials for clone, issue intake, and PR creation

### 2. Install or build `fabro` on the VM

If `fabro` is not already on `PATH`, either install it or run it through Cargo.

```bash
cargo install --path lib/crates/fabro-cli
export PATH="$HOME/.cargo/bin:$PATH"
```

Fallback:

```bash
cargo run -p fabro-cli -- server start --bind 127.0.0.1:3000
```

### 3. Create the minimal server config on the VM

The CLI/server path expects `~/.fabro/settings.toml` to exist.

```bash
mkdir -p ~/.fabro
printf '_version = 1\n' > ~/.fabro/settings.toml
```

### 4. Run the web UI through the server, not Bun

For this validation path, the embedded server UI was sufficient. `bun run dev` was not required on the VM.

Start the server on the VM:

```bash
fabro server start --bind 127.0.0.1:3000
```

Then from your laptop:

```bash
ssh -L 3000:127.0.0.1:3000 azureuser@<vm-ip-or-host>
```

Open:

```text
http://localhost:3000
```

Only install Bun on the VM if you want to modify `apps/fabro-web` and rebuild frontend assets there.

### 5. Configure GitHub App auth on the VM

Run this on the VM:

```bash
fabro install github
```

Choose:

- `GitHub App`
- a personal account or organization that owns the repository you will actually run against

`fabro install github` opens a temporary localhost callback such as `http://127.0.0.1:44207/`. On a remote VM, you must forward that exact ephemeral port from your laptop to the VM while the installer is waiting.

Example, in a second terminal on your laptop:

```bash
ssh -L 44207:127.0.0.1:44207 azureuser@<vm-ip-or-host>
```

Then open the printed `http://127.0.0.1:<port>/` URL in the laptop browser and complete the GitHub App manifest flow.

After the app is created, install it on the repository Fabro should access.

### 6. Configure GitHub browser login correctly

Fabro builds the GitHub OAuth callback from `server.web.url`, so the GitHub App callback URL must exactly match:

```text
{server.web.url}/auth/callback/github
```

The validated setup used:

```toml
[server.web]
enabled = true
url = "http://localhost:3000"

[server.auth]
methods = ["github", "dev-token"]

[server.auth.github]
allowed_usernames = ["<your-github-username>"]
```

With that configuration, the GitHub App callback URL must be:

```text
http://localhost:3000/auth/callback/github
```

`localhost` and `127.0.0.1` are not interchangeable for GitHub OAuth callback matching.

### 7. Keep `dev-token` enabled for CLI access

Browser login authenticates the web UI session only. `fabro run --server ...` is a separate API client and must authenticate separately.

The validated setup kept `dev-token` enabled and exported the server-generated token on the laptop before running the remote CLI.

On the VM:

```bash
cat ~/.fabro/dev-token
```

On the laptop:

```bash
export FABRO_DEV_TOKEN='fabro_dev_...'
fabro run .fabro/workflows/software-factory/workflow.toml --server http://localhost:3000/api/v1
```

### 8. Start the server from a shell that already has all Azure and LLM env vars

The server process reads Azure platform configuration from its own environment. Export these variables before starting `fabro server start`:

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

export OPENAI_API_KEY="..."
# or
export ANTHROPIC_API_KEY="..."
```

Then restart the server from that same shell:

```bash
fabro server stop
fabro server start --bind 127.0.0.1:3000
```

### 9. Validated `settings.toml` shape for the Azure VM server

The validated VM used a server-local `~/.fabro/settings.toml` shaped like this:

```toml
_version = 1

[run.sandbox]
provider = "azure"

[run.sandbox.azure]
image = "<acr-login-server>/fabro-sandboxes/base:trial"

[server.auth]
methods = ["github", "dev-token"]

[server.auth.github]
allowed_usernames = ["<your-github-username>"]

[server.integrations.github]
strategy = "app"
app_id = "..."
client_id = "..."
slug = "..."
permissions = { contents = "write", issues = "read", pull_requests = "write" }

[server.web]
enabled = true
url = "http://localhost:3000"
```

### 10. Branch-specific limitations discovered during validation

The following limitations or workarounds were observed on the validated `azure-sandbox` branch:

- `run.sandbox.azure.image` in server-local defaults was not reliably picked up for remote runs. The validated workaround was to also set the image explicitly in the checked-in workflow TOML.
- GitHub App browser auth and `fabro doctor` succeeded, but the remote worker path still emitted `GITHUB_TOKEN not configured` for a real workflow run. The validated workaround was to export `GITHUB_TOKEN` in the server shell before starting `fabro server`.
- Azure Container Instance rejects mixed-case container names. The branch required `0f843550` so run-based sandbox names are lowercased before provisioning.
- Trial subscriptions can have very small ACI quota. Clean up stale `fabro-*` container groups before retrying failed runs.

### 11. Remaining warning seen during validation

The validated real-run path still emitted this warning before sandbox startup:

```text
Failed to push fabro-software-factory to origin: Engine error: git push failed: No such file or directory (os error 2)
```

That warning did not block Azure sandbox creation, but it indicates the current branch still has at least one unresolved remote checkpoint or push-path issue.
