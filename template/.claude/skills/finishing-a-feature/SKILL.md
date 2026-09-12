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
  (`houserules check-knowledge` and `houserules check-backlog` included).
- The branch needs to go from "done" to "merged into main."

Do not start this procedure on a branch that is not yet green — fix tests
and lint first.

## Procedure

1. **Verify the branch is green.** Run the project's full test and lint gates,
   including `houserules check-knowledge` and `houserules check-backlog`.

2. **Update the backlog.** Run `houserules set <id> status=done
   batch=<n>` for every delivered item, set the batch's `status` in
   `backlog/batches.json` (a direct edit), and run `houserules check-backlog`.
   Commit this update.

3. **Sweep the archive.** Confirm every batch entry you are about to sweep
   has its acceptance ruling homed (`backlog/decisions.json` or the
   batch's own ledger) — `houserules archive` does not check this itself;
   it trusts the operator to run the sweep only after rulings land, which
   is why this step comes after step 2. Then, in order:
   ```sh
   houserules archive
   houserules render
   houserules check-knowledge
   houserules check-backlog
   ```
   `archive` moves every item, batch, and knowledge entry the ruling above
   just retired into `backlog/archive/`/`knowledge/archive/`; running it
   is safe even when nothing qualifies yet. `render` regenerates
   `.claude/rules/*.md` and the `project-knowledge` skill: a sweep that
   moves a rendered (standing or area-file) knowledge entry leaves those
   generated files stale, and `check-knowledge` fails on that drift if you
   skip this step. If either check still fails after `render`, the finding
   is real — fix it (or revert the sweep) before continuing; do not
   proceed to step 4 with a red gate. Commit the sweep and the
   regenerated files together, with the backlog update or in the branch
   that closes next — never skip the sweep itself outright.

4. **Aggregate commits.** When interactive rebase is unavailable, use a
   soft reset instead:
   ```sh
   git reset --soft $(git merge-base main HEAD)
   ```
   Then build 1-5 clean, logical Conventional Commits from the staged
   result (`git restore --staged .` and re-stage per logical group as
   needed). Never include a co-author line, in this or any commit.

5. **Push and open a PR.**
   ```sh
   git push -f -u origin <branch>
   gh pr create --fill
   ```

6. **Wait for CI.**
   ```sh
   gh pr checks --watch
   ```
   All checks must pass before continuing.

7. **Merge fast-forward from the CLI.** Never use the GitHub merge button.
   ```sh
   git switch main && git pull --ff-only
   git merge --ff-only <branch>
   git push origin main
   ```
   GitHub marks the PR merged once the commits reach main.

8. **After merging a release-please PR, or any merge that changes the
   houserules version, restamp the kit version.** Skip this step for
   every other merge. `.houserules.json` records the houserules version
   this project runs. That stamp goes stale when the version changes: a
   release does this in the kit repository itself; a newer `houserules`
   binary on `PATH` does it elsewhere. In a project that installs the
   kit, install the new release first — through whichever channel you
   installed houserules with (the shell installer, mise, or a direct
   download) — otherwise the command below still runs the old binary
   and reads no drift.
   ```sh
   houserules update --dir .
   ```
   Read the drift line the command prints, `kit <old> -> <new>`. Equal
   sides mean nothing to commit. Different sides mean the stamp changed:
   ```sh
   git add .houserules.json
   git commit -m "chore(release): restamp the kit version"
   git push
   ```

9. **Delete the branch.**
   ```sh
   git push origin --delete <branch>
   git branch -d <branch>
   ```

## Common Mistakes

- **Main moved since branching.** `git merge --ff-only` will fail. Rebase
  first (`git rebase main`), re-verify green, then aggregate commits as in
  step 4.
- **Force-pushing main.** Force-push (`-f`) is only ever for feature
  branches. Never force-push main.
- **Adding a co-author line.** This repository never uses co-author trailers,
  in aggregated commits or anywhere else in this procedure.
- **Merging via the GitHub UI.** The merge button, merge commits, and
  squash merges are all forbidden here — always merge ff-only from the CLI.
