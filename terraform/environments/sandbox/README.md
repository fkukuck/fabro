# Sandbox Environment

This Terraform root is the steady-state Azure environment for Fabro production deploys.

## Supported production flow

1. Run `terraform/bootstrap/github_actions` once to create the remote backend and the GitHub Actions OIDC identity.
2. Copy every bootstrap output into the GitHub `production` environment before continuing.
3. Initialize this root with the Azure Blob backend created by bootstrap.
4. Run one normal `terraform apply` with `fabro_server_enabled = false`. Do not use `-target`.
5. Trigger `.github/workflows/deploy-azure.yml` to build images, push them to ACR, apply Terraform, and verify steady state.
6. Complete the web install wizard with the GitHub token path and `Azure Container Instances`, then store the resulting dev token in the GitHub `production` environment as `FABRO_DEPLOY_DEV_TOKEN`.
7. Use the same workflow for all steady-state deploys.

Local Terraform state is no longer the supported production path for this environment.

## Inputs

- Set the Azure naming and network variables in `terraform.tfvars`.
- Set `github_actions_principal_id` to the bootstrap-created GitHub Actions principal object ID before the first manual apply. This is required for the supported CI-driven deployment path.
- Keep `fabro_server_enabled = false` until the first CI deploy is ready to publish a real immutable server image.
- The validated `fabro-server` shape for this Azure environment is `fabro_server_cpu = 2` and `fabro_server_memory = "4Gi"`.

## Runtime model

- The Container App mounts Azure Files at `/storage`.
- Fabro durable runtime data lives in the Terraform-managed Azure Blob container.
- Fabro runtime configuration is completed through the install wizard, not Terraform secret injection.
- The server image is expected to come from the environment ACR.

## Notes

- Storage account and ACR names must be globally unique.
- Keep the Container App at one replica.
- This root owns shared infrastructure and the live `fabro-server` image reference.
- Workflow-specific Azure sandbox image refs remain owned by the workflow repositories that use them.
- If you reuse an existing Azure resource group, keep this root's backend and GitHub environment variables pointed at that same environment.
- For forked testing or a second deployment, create a separate backend state key and resource group instead of sharing this root across unrelated environments.
