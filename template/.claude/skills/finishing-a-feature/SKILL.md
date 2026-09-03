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

7. **After merging a release-please PR, or any merge that changes the
   houserules version, restamp the kit version.** Skip this step for
   every other merge. `.houserules.json` records the houserules version
   this project runs. That stamp goes stale when the version changes: a
   release does this in the kit repository itself; a houserules
   dependency bump does it elsewhere. In a project that installs the
   kit, install first — otherwise the command below still runs the old
   copy and reads no drift.
   ```sh
   pnpm install                                           # a project that installs the kit
   pnpm exec houserules update --dir .                    # a project that installs the kit
   mise exec -- node bin/houserules.mjs update --dir .    # the kit repository itself
   ```
   Read the drift line the command prints, `kit <old> -> <new>`. Equal
   sides mean nothing to commit. Different sides mean the stamp changed:
   ```sh
   git add .houserules.json
   git commit -m "chore(release): restamp the kit version"
   git push
   ```

8. **Delete the branch.**
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
