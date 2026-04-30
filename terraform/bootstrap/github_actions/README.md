# GitHub Actions Bootstrap

This root creates:

- the backend resource group, storage account, and storage container
- the GitHub Actions user-assigned managed identity
- the GitHub OIDC federated credential on that identity
- backend storage RBAC for Terraform state access

Use this as the bootstrap companion to the canonical guide in
`docs/public/administration/deploy-azure.mdx`.

## Apply the bootstrap root

1. Copy `terraform.tfvars.example` to `terraform.tfvars` and fill in real values.
2. Run `terraform init`.
3. Run `terraform apply`.

## GitHub environment handoff

Record these outputs in the GitHub `production` environment:

- `AZURE_CLIENT_ID`
- `AZURE_TENANT_ID`
- `AZURE_SUBSCRIPTION_ID`
- `AZURE_GITHUB_ACTIONS_PRINCIPAL_ID`
- `TF_BACKEND_RESOURCE_GROUP`
- `TF_BACKEND_STORAGE_ACCOUNT`
- `TF_BACKEND_CONTAINER`
- `TF_BACKEND_KEY`

All of these values are required for the supported Azure deploy workflow. In particular,
`AZURE_GITHUB_ACTIONS_PRINCIPAL_ID` must be copied into the GitHub environment before the
first `terraform/environments/sandbox` apply so Terraform can grant CI access during
greenfield bring-up.

This root is intentionally bootstrap-only. The steady-state environment lives in
`terraform/environments/sandbox`.

If you need a fork or a second Azure environment, create a separate backend state key and
GitHub environment for it. Do not reuse this bootstrap state for two independent deployments
unless they are intentionally sharing the same Azure environment.
