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
- `FABRO_AZURE_ACR_SERVER`

## Optional Environment Variables

- `FABRO_AZURE_SANDBOXD_PORT`
- `AZURE_CLIENT_ID`
- `FABRO_AZURE_ACR_USERNAME`
- `FABRO_AZURE_ACR_PASSWORD`

## Sandbox Runtime

Workflow sandboxes run as Azure Container Instances with `/workspace` mounted from Azure Files.

The control plane provisions sandboxes through Azure ARM, waits for the in-sandbox `sandboxd` daemon, and then performs repo setup inside the container group.
