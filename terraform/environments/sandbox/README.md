# Sandbox Environment

This Terraform root is the steady-state Azure environment for Fabro production deploys.

## Supported production flow

1. Run `terraform/bootstrap/github_actions` once to create the remote backend and the GitHub Actions managed identity.
2. Copy every bootstrap output into the GitHub `production` environment before continuing.
3. Initialize this root with the Azure Blob backend created by bootstrap.
4. Run one normal `terraform apply` with `fabro_server_enabled = false`. Do not use `-target`.
5. Store a GitHub `production` environment secret named `FABRO_DEPLOY_GITHUB_TOKEN` before the first run if you want the workflow-managed install path to succeed in that same run. Use a user-backed token or PAT. During workflow-managed install, the workflow uses that token as the server's installed GitHub token for the repo/token-based GitHub install step, for Fabro runtime GitHub access to the target repo, and persists the emitted `FABRO_DEPLOY_DEV_TOKEN` into the GitHub `production` environment secret.
6. Trigger `.github/workflows/deploy-azure.yml` to build images, push them to ACR, apply Terraform, and treat `/health` as the branch point: `{"status":"ok","mode":"install"}` means the workflow-managed install completes immediately, persists `FABRO_DEPLOY_DEV_TOKEN`, sets runtime auth state, and runs authenticated validation in the same run. Plain `{"status":"ok"}` means install is skipped and the workflow continues directly. Authenticated validation only runs when a dev token is already available; otherwise those authenticated checks remain skipped.
7. If you finish install manually instead, complete the web install wizard with the GitHub token path and `Azure Container Instances`, then store the resulting dev token in the GitHub `production` environment as `FABRO_DEPLOY_DEV_TOKEN`.
8. Replace the LLM provider configuration manually after bootstrap because the workflow uses a dummy OpenAI key only to leave install mode.
9. Use the same workflow for all steady-state deploys.

Local Terraform state is no longer the supported production path for this environment.

## Inputs

- Set the Azure naming and network variables in `terraform.tfvars`.
- Set `github_actions_principal_id` to the bootstrap-created GitHub Actions managed identity principal ID before the first manual apply.
- The first manual apply must happen before GitHub Actions takes over so Terraform can create the `github_actions_access` role assignments with the higher-privilege operator identity.
- Keep `fabro_server_enabled = false` until the first CI deploy is ready to publish a real immutable server image.
- The validated `fabro-server` shape for this Azure environment is `fabro_server_cpu = 2` and `fabro_server_memory = "4Gi"`.

## Runtime model

- The Container App mounts Azure Files at `/storage`.
- Fabro durable runtime data lives in the Terraform-managed Azure Blob container.
- Fabro runtime configuration is completed through the install wizard, not Terraform secret injection.
- The server image is expected to come from the environment ACR.
- The deploy workflow branches on `/health` before validation: install mode triggers workflow-managed install completion, persists `FABRO_DEPLOY_DEV_TOKEN`, sets runtime auth state, and runs authenticated validation in the same run. A plain healthy response skips install and continues directly. Authenticated validation only runs when a dev token is already available; otherwise those authenticated checks remain skipped.
- `FABRO_DEPLOY_GITHUB_TOKEN` must be a user-backed token or PAT that can satisfy `gh api user`, serve as the server's installed GitHub token for the repo/token-based GitHub install step, access the target repo for Fabro runtime GitHub operations, and update the GitHub `production` environment secret. GitHub App installation tokens are not valid for this path.

## Notes

- Storage account and ACR names must be globally unique.
- Keep the Container App at one replica.
- This root owns shared infrastructure and the live `fabro-server` image reference.
- Workflow-specific Azure sandbox image refs remain owned by the workflow repositories that use them.
- If you reuse an existing Azure resource group, keep this root's backend and GitHub environment variables pointed at that same environment.
- For forked testing or a second deployment, create a separate backend state key and resource group instead of sharing this root across unrelated environments.
