# Sandbox Environment

This Terraform root is the steady-state Azure environment for Fabro production deploys.

## Supported production flow

1. Run `terraform/bootstrap/github_actions` once to create the remote backend and the GitHub Actions OIDC identity.
2. Initialize this root with the Azure Blob backend created by bootstrap.
3. Run this environment manually once with `fabro_server_enabled = false`.
4. Trigger `.github/workflows/deploy-azure.yml` to build images, push them to ACR, and deploy `fabro-server`.
5. Complete the web install wizard with the GitHub token path, then store the resulting dev token in the GitHub `production` environment as `FABRO_DEPLOY_DEV_TOKEN`.
6. Use the same workflow for all steady-state deploys.

Local Terraform state is no longer the supported production path for this environment.

## Inputs

- Set the Azure naming and network variables in `terraform.tfvars`.
- Set `github_actions_principal_id` to the bootstrap-created service principal object ID if you want Terraform to grant CI access during the first manual apply.
- Keep `fabro_server_enabled = false` until the first CI deploy is ready to publish a real immutable server image.

## Runtime model

- The Container App mounts Azure Files at `/storage`.
- Fabro runtime configuration is completed through the install wizard, not Terraform secret injection.
- The server image is expected to come from the environment ACR.
- Azure sandbox runtime settings are persisted by the install flow and reused across redeploys.

## Notes

- Storage account and ACR names must be globally unique.
- Keep the Container App at one replica.
- This root owns shared infrastructure and the live `fabro-server` image reference.
- Workflow-specific Azure sandbox image refs remain owned by the workflow repositories that use them.
