# GitHub Actions Bootstrap

This root creates:

- the backend resource group, storage account, and storage container
- the GitHub Actions user-assigned managed identity
- the GitHub OIDC federated credential on that identity
- backend storage RBAC for Terraform state access

Use this as the bootstrap companion to the canonical guide in
`docs/public/administration/deploy-azure.mdx`.

## Apply the bootstrap root

1. Copy `terraform.tfvars.example` to the untracked local file `terraform.tfvars` and fill in real values.
2. Run `terraform init`.
3. Run `terraform apply`.

Do not commit `terraform.tfvars`. The checked-in contract is the example file plus the GitHub
environment handoff below; live values are environment state and must stay local.

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
`AZURE_GITHUB_ACTIONS_PRINCIPAL_ID` must be copied into the GitHub environment as the CI
handoff value that the deploy workflow uses during greenfield bring-up. The first operator-run
`terraform/environments/sandbox` apply also needs the same principal ID set separately as that
root's `github_actions_principal_id` Terraform input.

This root is intentionally bootstrap-only. The steady-state environment lives in
`terraform/environments/sandbox`.

After bootstrap handoff, operators must run `.github/workflows/deploy-azure.yml`
from the Azure-ready branch or ref that contains the deploy workflow and Azure
deployment changes. Forks that keep `main` as an upstream mirror must not assume
`main` is the deployable branch.

If you need a fork or a second Azure environment, give that independent deployment its own
backend state key, GitHub environment, and Azure resource group. Do not reuse this bootstrap
state across independent deployments unless they are intentionally sharing the same Azure
environment.
