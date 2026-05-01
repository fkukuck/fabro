# GitHub Actions Bootstrap

1. Copy `terraform.tfvars.example` to `terraform.tfvars` and fill in real values.
2. Run `terraform init`.
3. Run `terraform apply`.
4. Record these outputs in the GitHub `production` environment:
   - `AZURE_CLIENT_ID` = `terraform output -raw github_actions_client_id`
   - `AZURE_TENANT_ID` = `terraform output -raw tenant_id`
   - `AZURE_SUBSCRIPTION_ID` = `terraform output -raw subscription_id`
   - `AZURE_GITHUB_ACTIONS_PRINCIPAL_ID` = `terraform output -raw github_actions_principal_id`
   - `TF_BACKEND_RESOURCE_GROUP` = `terraform output -raw backend_resource_group_name`
   - `TF_BACKEND_STORAGE_ACCOUNT` = `terraform output -raw backend_storage_account_name`
   - `TF_BACKEND_CONTAINER` = `terraform output -raw backend_container_name`
   - `TF_BACKEND_KEY` = `terraform output -raw backend_state_key`

All of these values are required for the supported Azure deploy workflow. In particular,
`AZURE_GITHUB_ACTIONS_PRINCIPAL_ID` must be copied into the GitHub environment before the
first `terraform/environments/sandbox` apply so Terraform can grant CI access during
greenfield bring-up.

This root is intentionally bootstrap-only. The steady-state environment lives in `terraform/environments/sandbox`.

If you need a fork or a second Azure environment, create a separate backend state key and GitHub environment for it.
Do not reuse this bootstrap state for two independent deployments unless they are intentionally sharing the same Azure environment.
