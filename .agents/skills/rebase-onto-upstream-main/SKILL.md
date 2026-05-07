---
name: rebase-onto-upstream-main
description: Rebase this Fabro fork or Azure branch onto upstream/main or origin/main in an isolated worktree. Use when asked to update a diverged fork branch, resolve rebase conflicts non-interactively, compare upstream changes against fork changes, read changelogs since the merge base, or preserve Azure/ipt fork functionality while aligning with upstream Fabro.
---

# Rebase Onto Upstream Main

## Objective

Rebase the current Fabro fork branch onto the latest upstream mainline while preserving intentional fork behavior and dropping fork changes that upstream now covers.

Prefer upstream implementations when they solve the same problem. Keep Azure and ipt.ch fork functionality working under the updated upstream architecture.

## Safety Rules

- Work in a separate git worktree. Do not perform the rebase in the user's current working tree.
- Create a backup ref for the original branch tip before starting the rebase.
- Use non-interactive git commands. Set `GIT_EDITOR=true` or equivalent for commands that might open an editor.
- Do not overwrite, reset, or delete the user's original branch unless they explicitly approve it.
- Do not force-push unless the user explicitly asks.
- Treat unrelated dirty worktree changes as user-owned. Preserve them and avoid operating on them.

## Workflow

1. Identify the current branch, remotes, and intended base.
   - Prefer `upstream/main` when it exists and is fresh.
   - Use `origin/main` only when the repo's mirror/main policy or user request makes that the intended base.
2. Verify remotes and refs with `git remote -v`, `git branch --show-current`, `git status --short`, and `git rev-parse`.
3. Create a backup ref, for example `refs/backup/<branch>-before-upstream-rebase-<date-or-sha>`.
4. Create an isolated worktree from the branch tip, for example under `../fabro-rebase-<branch>`.
5. Find the merge base between the branch and the target mainline.
6. Determine the merge-base commit date with `git show -s --format=%cI <merge-base>`.
7. Read `docs/public/changelog/*.mdx` entries dated on or after the merge-base date. Use them to understand upstream behavior changes before resolving conflicts.
8. Inspect upstream changes since divergence:
   - `git log --oneline <merge-base>..<target-mainline>`
   - `git diff <merge-base>..<target-mainline>`
9. Inspect fork changes since divergence:
   - `git log --oneline <merge-base>..HEAD`
   - `git diff <merge-base>..HEAD`
10. Compare both sides for overlap. If upstream now includes a feature, fix, refactor, or API shape the fork previously carried, align with upstream's version instead of preserving duplicate fork code.
11. Rebase the worktree branch onto the target mainline non-interactively.
12. Resolve conflicts deliberately:
    - Preserve Azure Container Apps server hosting and Azure Container Instances workflow sandbox support.
    - Preserve ipt.ch branding.
    - Adapt fork code to upstream's new names, APIs, architecture, tests, docs, and generated artifacts.
    - Prefer repo-native patterns and current upstream style over older fork patterns.
13. Continue the rebase until complete, or abort only if the conflict state cannot be resolved safely.

## Verification

After the rebase completes:

1. Review the final diff versus the target mainline.
2. Review the final diff versus the original branch tip and call out intentional behavior differences.
3. Run focused verification for touched areas.
4. For broad Rust changes, prefer:
   - `cargo build --workspace`
   - `cargo nextest run --workspace`
   - `cargo +nightly-2026-04-14 fmt --check --all`
   - `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`
5. For web changes, run relevant commands in `apps/fabro-web`, such as `bun run typecheck`, `bun test`, or `bun run build`.
6. If validation is too expensive or blocked, state exactly what did and did not run.

## Final Response

Report:

1. The worktree path and rebased branch/ref.
2. The target mainline and merge-base commit/date used.
3. The changelog files consulted.
4. Key upstream changes that affected conflict resolution.
5. What fork behavior was preserved, changed, or dropped because upstream now covers it.
6. Verification commands and actual outcomes.
7. Exact safe commands for replacing the user's real branch only if they want to proceed.
