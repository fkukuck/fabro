We maintain a fork of `origin/main` on branch `ipt`.

Starting at the merge base with `origin/main`, analyze every commit on `ipt` and rewrite the branch history into a small, readable sequence of durable commits.

Goals:
- Keep only meaningful fork-specific deltas that we still intentionally want to carry on top of upstream.
- Squash related commits into coherent units.
- Drop commits whose effects were reverted, overwritten, superseded, or turned out to be dead ends.
- Remove workflow/planning artifacts and similar non-product churn unless I explicitly say to keep them.
- Preserve the actual behavior and files we still need at the branch tip.
- Give every surviving commit a strong subject line and a body that explains why the change exists and why it should remain in the fork.
- Do not keep merge commits.

Process requirements:
- Work in an isolated git worktree.
- Create a backup ref before rewriting anything.
- Inspect the merge base, full commit range, and overall diff versus `origin/main` before deciding on the new series.
- Rebuild the history intentionally rather than preserving accidental "fix the previous fix" noise.
- Prefer a small number of meaningful commits over a long chain of follow-up corrections.
- If a directory or artifact class is supposed to disappear entirely, make sure it does not survive anywhere in the rewritten branch tip.
- Leave the original branch untouched until I approve replacing it.

Verification requirements:
- Compare the rewritten branch tip against the original branch tip and call out any intentional content differences.
- Run the relevant verification commands for the repo and report the actual results.
- Do not claim success without fresh verification output.

Perform the task.

Return:
1. The rewritten commit list in order.
2. Any intentional content changes versus the original branch tip.
3. Any remaining risk or uncertainty.
4. The exact commands I should run to move the real branch and force-push it safely after approval.
