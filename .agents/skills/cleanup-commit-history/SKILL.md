---
name: cleanup-commit-history
description: Rewrite a fork or feature branch history into a small, readable sequence of durable commits. Use when asked to clean up commit history, rebuild a branch series, squash/drop noisy commits, prepare an ipt fork branch against origin/main or upstream/main, or provide safe commands for replacing and force-pushing a rewritten branch.
---

# Cleanup Commit History

## Objective

Rewrite the current branch history into a small, coherent commit series that preserves the branch tip behavior the user still wants while removing accidental churn.

Prefer durable product commits over chronological noise. Drop commits whose effects were reverted, overwritten, superseded, or turned out to be dead ends. Remove workflow, planning, scratch, and non-product artifacts unless the user explicitly says to keep them.

## Safety Rules

- Work in an isolated git worktree.
- Create a backup ref before rewriting anything.
- Do not rewrite or replace the user's real branch until they approve it.
- Do not keep merge commits in the rewritten series.
- Preserve the original branch tip content unless an intentional content difference is explicitly called out.
- If a directory or artifact class is supposed to disappear entirely, verify it does not survive in the rewritten branch tip.
- Never claim success without fresh verification output.

## Analysis Phase

1. Identify the branch being cleaned up and the upstream base, usually `origin/main`.
2. Inspect the merge base.
3. Inspect the full commit range from merge base to branch tip.
4. Inspect the aggregate diff versus the base.
5. Group the desired surviving changes into a small number of meaningful commits.
6. Decide which commits or artifact classes should be dropped because they are reverted, superseded, scratch-only, or non-product churn.

## Rewrite Phase

Rebuild the series intentionally. Do not preserve "fix the previous fix" commits as separate commits unless each fix represents a durable standalone reason to remain in the fork.

For every surviving commit:

- Use a strong subject line.
- Add a body explaining why the change exists.
- Explain why the fork should continue carrying it on top of upstream.
- Keep the commit scoped to one durable concern.

## Verification Phase

Before presenting the result:

1. Compare the rewritten branch tip against the original branch tip.
2. Call out every intentional content difference.
3. Run the relevant repo verification commands.
4. Report the actual command results.
5. Check that dropped artifact classes are absent from the rewritten tip.

## Final Response

Return:

1. The rewritten commit list in order.
2. Any intentional content changes versus the original branch tip.
3. Any remaining risk or uncertainty.
4. The exact commands the user should run to move the real branch and force-push it safely after approval.

Keep the original branch untouched unless the user explicitly approves replacing it.
