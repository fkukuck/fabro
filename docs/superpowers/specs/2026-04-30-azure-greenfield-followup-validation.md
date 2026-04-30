# Azure greenfield follow-up validation

Date: 2026-04-30
Worktree: `/Users/ipt/repos/fabro-fkukuck/.worktrees/azure-greenfield-stabilization`
Branch: `opencode/azure-greenfield-stabilization`

## Goal

Revalidate the greenfield bring-up path and the steady-state repeat-deploy path for the Azure deployment work completed in Tasks 1-4, without creating commits or pushing any branch state.

## Fresh evidence collected

### Local Terraform validation

- `terraform/bootstrap/github_actions`
  - `terraform init -backend=false`: succeeded.
  - `terraform validate`: succeeded.
- `terraform/environments/sandbox`
  - `terraform validate`: succeeded.

This proves both Terraform roots are syntactically valid in the current worktree.

### Local install-flow application tests

- `apps/fabro-web`
  - `bun test app/install-app.test.tsx`: passed (`17 pass`, `0 fail`).
- repository root
  - `ulimit -n 4096 && cargo nextest run -p fabro-server install`: passed (`64 tests run: 64 passed, 476 skipped`).

This provides fresh local coverage for the install wizard and server-side install/deploy state handling that underpins the Azure bring-up flow.

### GitHub and Azure environment inspection

- `gh repo view --json nameWithOwner,defaultBranchRef` reported `fabro-sh/fabro` with default branch `main`.
- `gh workflow list --json id,name,path,state` on `fabro-sh/fabro` did not list `.github/workflows/deploy-azure.yml` on the default branch.
- `gh api repos/fabro-sh/fabro/environments` listed `nightly`, `release`, `staging - docs`, and `staging - docs/public`; there was no `production` environment.
- `az account show --output json` succeeded and confirmed a live Azure login in subscription `97200cb2-456d-4471-876a-55f0a2bd8d54`.
- `az group list --query "[].name" -o tsv` showed only `NetworkWatcherRG` and `fkukuck-fabro-tfstate`.
- `az acr list --query "[?contains(name, 'fabro')].[name,loginServer,resourceGroup]" -o tsv` returned no ACRs.
- `az containerapp list --query "[?contains(name, 'fabro')].[name,resourceGroup,properties.configuration.ingress.fqdn]" -o tsv` returned no Fabro container apps.
- `az resource list --resource-group "fkukuck-fabro-tfstate" --query "[].{type:type,name:name}" -o tsv` showed only the backend storage account `fkukuckfabrotfstate`.

## Greenfield path

The bootstrap backend appears to exist, but there is no evidence in the current Azure subscription of the shared sandbox environment having been applied yet: no Fabro resource group, no ACR, and no Container App were visible from the logged-in account.

Because no local `terraform.tfvars` files or equivalent secure input values were present in this worktree, I did not have the environment-specific variables needed to run a responsible greenfield `terraform plan` or `terraform apply`. I also did not have the concrete remote-backend configuration values and target environment inputs needed to initialize and exercise the real shared sandbox environment from this session.

## Repeat-deploy path

I could not revalidate the repeat-deploy GitHub Actions path end-to-end for the current branch state because:

- `gh workflow list` for the remote default branch did not show `.github/workflows/deploy-azure.yml`,
- the GitHub `production` environment does not exist remotely,
- the current task explicitly forbids pushing the branch, and
- the current worktree contains uncommitted local changes that cannot be consumed by GitHub Actions without a push.

That means there is no safe way in this session to trigger GitHub Actions against the exact code under test, nor to observe a live Azure deployment that already uses this branch's uncommitted changes.

## Responsible conclusion

Task 5 can be advanced to the point of:

- confirming the Terraform roots validate locally,
- confirming the install-flow tests pass locally,
- confirming Azure login works,
- confirming the backend resource group exists,
- documenting that the actual greenfield environment apply and repeat-deploy workflow validation are still blocked by missing environment-specific inputs, the absence of a visible remote `production` environment, and the no-push constraint.

The remaining required evidence for full Task 5 completion is:

1. Real environment values (`terraform.tfvars` or equivalent secure inputs).
2. A provisioned GitHub `production` environment with the documented variables and secrets.
3. A remote branch or merged branch that contains the code being validated, so GitHub Actions can execute the exact workflow and Terraform changes under test.
4. A visible GitHub Actions deploy path for that branch, together with the required GitHub `production` environment configuration.
5. A live Azure environment apply plus post-deploy checks (`/health`, authenticated `/api/v1/models`, and the Azure smoke workflow).
