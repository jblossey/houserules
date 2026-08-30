---
name: finishing-a-feature
description: Use when a feature branch is complete, tests and lint are green, and the branch is ready to merge to main. This repository forbids GitHub merge buttons, merge commits, squash merges, and co-author lines; merges are fast-forward-only via CLI.
---

# Finishing a Feature

## Overview

This repository requires clean aggregated commits, fast-forward-only (ff-only)
merges done from the CLI, and no co-author lines. When interactive rebase is
not available, commit aggregation uses `git reset --soft` instead.

## When to Use

- The feature branch's work is done and the project's test and lint gates pass
  (`tools/kb.sh check` and `tools/backlog.sh check` included).
- The branch needs to go from "done" to "merged into main."

Do not start this procedure on a branch that is not yet green — fix tests
and lint first.

## Procedure

1. **Verify the branch is green.** Run the project's full test and lint gates,
   including `tools/kb.sh check` and `tools/backlog.sh check`.

2. **Update the backlog.** Run `tools/backlog.sh set <id> status=done
   batch=<n>` for every delivered item, set the batch's `status` in
   `backlog/batches.json` (a direct edit), and run `tools/backlog.sh check`.
   Commit this update.

3. **Aggregate commits.** When interactive rebase is unavailable, use a
   soft reset instead:
   ```sh
   git reset --soft $(git merge-base main HEAD)
   ```
   Then build 1-5 clean, logical Conventional Commits from the staged
   result (`git restore --staged .` and re-stage per logical group as
   needed). Never include a co-author line, in this or any commit.

4. **Push and open a PR.**
   ```sh
   git push -f -u origin <branch>
   gh pr create --fill
   ```

5. **Wait for CI.**
   ```sh
   gh pr checks --watch
   ```
   All checks must pass before continuing.

6. **Merge fast-forward from the CLI.** Never use the GitHub merge button.
   ```sh
   git switch main && git pull --ff-only
   git merge --ff-only <branch>
   git push origin main
   ```
   GitHub marks the PR merged once the commits reach main.

7. **Delete the branch.**
   ```sh
   git push origin --delete <branch>
   git branch -d <branch>
   ```

## Common Mistakes

- **Main moved since branching.** `git merge --ff-only` will fail. Rebase
  first (`git rebase main`), re-verify green, then aggregate commits as in
  step 3.
- **Force-pushing main.** Force-push (`-f`) is only ever for feature
  branches. Never force-push main.
- **Adding a co-author line.** This repository never uses co-author trailers,
  in aggregated commits or anywhere else in this procedure.
- **Merging via the GitHub UI.** The merge button, merge commits, and
  squash merges are all forbidden here — always merge ff-only from the CLI.
