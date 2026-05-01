# Sandbox Environment

This Terraform root is the steady-state Azure environment for Fabro production deploys.

## Supported production flow

1. Run `terraform/bootstrap/github_actions` once to create the remote backend and the GitHub Actions managed identity.
2. Copy every bootstrap output into the GitHub `production` environment before continuing. This
   GitHub environment handoff is for the deploy workflow; it does not replace setting this
   root's Terraform inputs for the first operator-run apply.
3. Initialize this root with the Azure Blob backend created by bootstrap.
4. Run one normal `terraform apply` with `fabro_server_enabled = false`. Do not use `-target`.
5. Create the GitHub App manually before the first browser-login-ready deploy.
6. Store the required GitHub App variables `FABRO_DEPLOY_GITHUB_APP_ID`, `FABRO_DEPLOY_GITHUB_APP_CLIENT_ID`, `FABRO_DEPLOY_GITHUB_APP_SLUG`, and `FABRO_DEPLOY_GITHUB_ALLOWED_USERNAME`, the required GitHub App secrets `FABRO_DEPLOY_GITHUB_APP_CLIENT_SECRET` and `FABRO_DEPLOY_GITHUB_APP_PRIVATE_KEY`, the optional secret `FABRO_DEPLOY_GITHUB_APP_WEBHOOK_SECRET`, and the workflow GitHub token `FABRO_DEPLOY_GITHUB_TOKEN` in the GitHub `production` environment.
7. Trigger `.github/workflows/deploy-azure.yml`. If the app values are present, the workflow completes install and persists `FABRO_DEPLOY_DEV_TOKEN`. If they are missing, the workflow stops after pre-filling the non-GitHub install steps so you can complete GitHub App linkage manually, store the emitted `FABRO_DEPLOY_DEV_TOKEN` in the GitHub `production` environment, and rerun the workflow for authenticated validation.
8. Replace the LLM provider configuration manually after bootstrap because the workflow uses a dummy OpenAI key only to leave install mode.
9. Use the same `.github/workflows/deploy-azure.yml` workflow for all steady-state deploys, and rerun it from the current Azure-ready branch or ref whenever you need to ship a new version.

Local Terraform state is no longer the supported production path for this environment.

## Inputs

- Set the Azure naming and network variables in `terraform.tfvars`.
- Set `github_actions_principal_id` to the same bootstrap-created GitHub Actions managed identity principal ID that you copied into the GitHub `production` environment as `AZURE_GITHUB_ACTIONS_PRINCIPAL_ID` before the first manual apply.
- The first manual apply must happen before GitHub Actions takes over so Terraform can create the `github_actions_access` role assignments with the higher-privilege operator identity.
- Keep `fabro_server_enabled = false` until the first CI deploy is ready to publish a real immutable server image.
- The validated `fabro-server` shape for this Azure environment is `fabro_server_cpu = 2` and `fabro_server_memory = "4Gi"`.

## Runtime model

- The Container App mounts Azure Files at `/storage`.
- Fabro durable runtime data lives in the Terraform-managed Azure Blob container.
- Fabro runtime configuration is completed through the install wizard, not Terraform secret injection.
- The server image is expected to come from the environment ACR.
- The deploy workflow branches on `/health` before validation: install mode with the GitHub App values present triggers workflow-managed install completion, persists `FABRO_DEPLOY_DEV_TOKEN`, sets runtime auth state, and runs authenticated validation in the same run. Install mode without those values stops at the manual GitHub linkage handoff after the non-GitHub steps are prefilled. A plain healthy response skips install and continues directly. Authenticated validation only runs when a dev token is already available; otherwise those authenticated checks remain skipped.
- The deploy workflow uses the pre-existing GitHub App values from the GitHub `production` environment to complete browser-login-ready installs. `FABRO_DEPLOY_GITHUB_TOKEN` remains a workflow-only GitHub credential for repository access checks and for persisting `FABRO_DEPLOY_DEV_TOKEN`; Fabro does not persist it as the runtime GitHub credential. If the app values are absent on a greenfield install, the workflow stops at the manual GitHub linkage handoff point.

## Notes

- Storage account and ACR names must be globally unique.
- Keep the Container App at one replica.
- This root owns shared infrastructure and the live `fabro-server` image reference.
- Workflow-specific Azure sandbox image refs remain owned by the workflow repositories that use them.
- If you reuse an existing Azure resource group, keep this root's backend and GitHub environment variables pointed at that same environment.
- For forked testing or a second deployment, use a separate backend state key, GitHub environment, and Azure resource group instead of sharing this root across unrelated environments.
