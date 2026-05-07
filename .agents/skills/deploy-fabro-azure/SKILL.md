---
name: deploy-fabro-azure
description: Deploy or validate this Fabro fork on Azure using the repo's Azure Container Apps and Azure Container Instances guide. Use when asked to deploy Fabro "the Azure way", configure the GitHub production environment, run or diagnose .github/workflows/deploy-azure.yml, complete first install, validate Azure smoke checks, or recover a failed Azure deployment run.
---

# Deploy Fabro Azure

## Start Here

Use the canonical guide at `docs/public/administration/deploy-azure.mdx` as the source of truth. Load `references/azure-deploy-runbook.md` when executing the deployment, diagnosing a workflow run, or preparing a summary for the operator.

Never print secret values. Treat GitHub App client secrets, private keys, dev tokens, Terraform tfvars, and install tokens as credentials.

## Workflow

1. Identify the target repo, branch/ref, GitHub environment, Azure subscription, and whether this is greenfield install or day-two redeploy.
2. Run local preflight checks before changing cloud state: `az account show`, `gh auth status`, Terraform version, Docker buildx version, and git status.
3. Verify GitHub environment variable and secret presence with presence-only checks. For headless install, require the complete GitHub App set before workflow dispatch.
4. Confirm bootstrap state exists or apply `terraform/bootstrap/github_actions` once.
5. Confirm `terraform/environments/sandbox` was manually applied once with `fabro_server_enabled=false` and the bootstrap GitHub Actions principal ID.
6. Dispatch or inspect `.github/workflows/deploy-azure.yml` from the Azure-ready ref.
7. Use `/health` as the state machine:
   - plain `{"status":"ok"}` means configured deployment
   - `{"status":"ok","mode":"install"}` means install must complete before authenticated checks
8. If install is manual, complete the browser wizard, store `FABRO_DEPLOY_DEV_TOKEN` in the GitHub `production` environment, then rerun the workflow.
9. Validate post-deploy checks: `/health`, authenticated `/api/v1/models`, CLI auth login using the base URL, Azure smoke workflow using `/api/v1`, and required workflow image checks.
10. Summarize exact failures by phase and state whether Azure deployment, install, auth, smoke validation, or guide/operator setup failed.

## Auth Rules

Use the base Container App URL for browser access, `/health`, and CLI login:

```bash
fabro auth login --server "$FABRO_SERVER_URL" --dev-token "$FABRO_DEPLOY_DEV_TOKEN"
```

Use the API URL for API endpoints and workflow execution:

```bash
fabro run workflow.toml --server "$FABRO_SERVER_URL/api/v1"
```

Do not assume `FABRO_DEV_TOKEN` alone authenticates `fabro run --server` against an explicit remote HTTP(S) target. Persist CLI auth first with `fabro auth login`.

## Common Failures

- Missing GitHub App secret or PEM: fix GitHub environment secrets before first headless install. Partial App config should fail early.
- Bad install token from logs: parse only standalone token text or `[A-Za-z0-9_-]{40,}` after `/install?token=`.
- `curl` API auth passes but CLI smoke says `Authentication required`: run `fabro auth login --server "$FABRO_SERVER_URL" --dev-token "$FABRO_DEPLOY_DEV_TOKEN"` before smoke.
- Workflow-specific image failures: push the missing images to this environment's ACR and list full refs in `AZURE_REQUIRED_WORKFLOW_IMAGES`.
- Healthy server but skipped authenticated checks: store `FABRO_DEPLOY_DEV_TOKEN` in the GitHub environment and rerun.

## Completion Criteria

Call the deployment done only when `/health` is ok, install mode is gone, browser or dev-token CLI auth works, the deploy workflow passes authenticated API checks, and the Azure smoke workflow can launch a sandbox. If one of those checks is intentionally deferred, say exactly which gate remains.
