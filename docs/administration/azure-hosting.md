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
