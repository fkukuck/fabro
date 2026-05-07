# Azure Deploy Runbook

## Inputs to establish

- GitHub repo and deploy ref.
- GitHub environment name, normally `production`.
- Azure subscription ID and tenant from `az account show`.
- Terraform backend resource group, storage account, container, and state key.
- Sandbox resource names: resource group, VNet/subnets, storage account/share, ACR, identity names, Container Apps environment, server app name.
- GitHub App values: App ID, Client ID, slug, allowed username, client secret, private-key PEM, optional webhook secret.

## Presence-only GitHub environment check

Use `gh api` or `gh variable list` / `gh secret list` to check names only. Never echo secret bodies.

Required vars:

- `AZURE_CLIENT_ID`
- `AZURE_TENANT_ID`
- `AZURE_SUBSCRIPTION_ID`
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
- `AZURE_GITHUB_ACTIONS_PRINCIPAL_ID`
- `TF_BACKEND_RESOURCE_GROUP`
- `TF_BACKEND_STORAGE_ACCOUNT`
- `TF_BACKEND_CONTAINER`
- `TF_BACKEND_KEY`
- `FABRO_DEPLOY_GITHUB_APP_ID`
- `FABRO_DEPLOY_GITHUB_APP_CLIENT_ID`
- `FABRO_DEPLOY_GITHUB_APP_SLUG`
- `FABRO_DEPLOY_GITHUB_ALLOWED_USERNAME`

Required secrets for headless install:

- `FABRO_DEPLOY_GITHUB_TOKEN`
- `FABRO_DEPLOY_GITHUB_APP_CLIENT_SECRET`
- `FABRO_DEPLOY_GITHUB_APP_PRIVATE_KEY`

Required secret after install:

- `FABRO_DEPLOY_DEV_TOKEN`

## Deployment phases

1. Bootstrap applies `terraform/bootstrap/github_actions` once.
2. Operator applies `terraform/environments/sandbox` once with `-var='fabro_server_enabled=false'`.
3. GitHub Actions deploy builds images, applies Terraform with immutable image refs, waits for ACA revision readiness, reads `/health`, completes install if needed, and validates.
4. First install persists runtime config to `/storage` and emits the dev token.
5. Day-two deploys reuse `/storage/settings.toml` and go straight to validation.

## Triage by failed step

- `Validate Azure deploy inputs`: missing GitHub environment vars.
- `Validate GitHub App deploy inputs`: partial headless GitHub App config; complete all vars/secrets or remove all App values and use manual handoff.
- `Auto-complete install when needed`: inspect whether install token extraction, GitHub token repo access, secret-write access, or an install endpoint failed.
- `Authenticated API check`: dev token missing or invalid for server.
- `Run Azure smoke workflow`: first verify `fabro auth login --server "$FABRO_SERVER_URL" --dev-token "$FABRO_DEPLOY_RUNTIME_DEV_TOKEN"` ran, then inspect sandbox creation errors.
- `Verify required workflow images`: missing ACR image or image outside target ACR.

## Manual validation

```bash
curl -fsSL "$FABRO_SERVER_URL/health"

curl -fsSL \
  -H "Authorization: Bearer $FABRO_DEPLOY_DEV_TOKEN" \
  "$FABRO_SERVER_URL/api/v1/models"

fabro auth login \
  --server "$FABRO_SERVER_URL" \
  --dev-token "$FABRO_DEPLOY_DEV_TOKEN"

fabro run workflow.toml \
  --server "$FABRO_SERVER_URL/api/v1"
```

Use the stable ACA base URL for login and the `/api/v1` URL for `fabro run`.
