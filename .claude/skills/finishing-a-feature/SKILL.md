---
name: finishing-a-feature
description: Use when a feature branch is complete, tests and lint are green, and the branch is ready to merge to main. Merges are fast-forward-only via CLI and commits carry no co-author lines by default, unless the project has ruled its own merge and attribution discipline.
---

# Finishing a Feature

## Overview

This procedure produces clean aggregated commits and merges them from the
CLI. It ships with two defaults a project may override by ruling its own
discipline instead (elicited by the `migrating-knowledge` skill's "Elicit
your own operating discipline" step, recorded as its own knowledge entry):
fast-forward-only (ff-only) merges, and no co-author lines. When
interactive rebase is not available, commit aggregation uses `git reset
--soft` instead.

Write the output of every `houserules` command to a file in the batch
workspace, a folder of the `workspaces` location (default
`.houserules/work/`), and read the file with your file-read tool. Treat a
`<<` marker or a "more items" note as a lost read and read the source file.

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
   batch=<n>` for every delivered item. Set the `status` state of the batch
   record to `done`, and run `houserules check-backlog`. The record lives in
   the tracker: on the JSON tracker, edit its entry in `backlog/batches.json`;
   on the GitHub tracker, edit the `status` in the `houserules` block of the
   issue labelled `houserules:batch` and close it; on the Beads tracker, do
   the same on the issue of the custom type `houserules-batch`. The
   `orchestrating` skill gives the layout. Commit the files that this update
   changed.

3. **Sweep the archive.** Confirm every batch entry you are about to sweep
   has its acceptance ruling homed (a decision record in the tracker or the
   batch's own ledger) — `houserules archive` does not check this itself;
   it trusts the operator to run the sweep only after rulings land, which
   is why this step comes after step 2. Then, in order:
   ```sh
   houserules archive
   houserules render
   houserules check-knowledge
   houserules check-backlog
   ```
   `archive` moves every knowledge entry the ruling above just retired into
   `knowledge/archive/` and, on the JSON tracker, every done item and batch
   into `backlog/archive/`; the GitHub and Beads trackers keep closed items
   in place. Running it is safe even when nothing qualifies yet. `render`
   regenerates the houserules blocks of the `AGENTS.md` files and, in an
   install with the Claude renderer, `.claude/rules/*.md` and the
   `project-knowledge` skill: a sweep that moves a rendered (standing or
   area-file) knowledge entry leaves those generated files stale, and
   `check-knowledge` fails on that drift if you skip this step. If either
   check still fails after `render`, the finding is real — fix it (or
   revert the sweep) before continuing; do not proceed to step 4 with a
   red gate. Commit the sweep and the regenerated files together, with the
   backlog update or in the branch that closes next — never skip the sweep
   itself outright.

4. **Aggregate commits.** Fetch first: `git fetch origin main`. If
   `git merge-base --is-ancestor origin/main HEAD` fails, main moved:
   rebuild the branch on `origin/main`, re-verify green, and only then
   aggregate. The rebuild keeps the old branch under a new name and writes
   the old-to-new sha map to its own workspace file (the sha-map rule of
   the `orchestrating` skill):
   ```sh
   git branch <branch>-pre-rebuild
   git rebase origin/main
   git log --reverse --format=%H origin/main..<branch>-pre-rebuild > <workspace>/old-shas.txt
   git log --reverse --format=%H origin/main..<branch> > <workspace>/new-shas.txt
   paste <workspace>/old-shas.txt <workspace>/new-shas.txt > <workspace>/sha-map.txt
   ```
   If a rebase drops a commit that `origin/main` already holds, the lists
   differ in length: pair the rest by subject. When interactive rebase is
   unavailable, aggregate with a soft reset instead:
   ```sh
   git reset --soft $(git merge-base origin/main HEAD)
   ```
   Then build 1-5 clean, logical Conventional Commits from the staged
   result (`git restore --staged .` and re-stage per logical group as
   needed). Write one `Work-Item: <id>` trailer for each item that an
   aggregated commit delivers, in the last paragraph of its message, one
   trailer per line. Never include a co-author line, in this or any commit,
   unless the project has ruled its own attribution convention.

4b. **Sweep interim SHAs.** Grep the aggregated diff for commit-SHA
   citations (`git diff origin/main...HEAD | grep -oE '\b[0-9a-f]{7,40}\b'`,
   judged in context): a SHA that will not survive the aggregation
   restates to a durable locator or to the aggregated commit.

4c. **Check the commits.** Run `houserules check-commit --from <merge base>`
   before the merge, even when the commit-msg hook skipped it. The merge
   base is the output of `git merge-base origin/main HEAD`. The hook skips
   the check when `houserules check-commit --help` fails (no `houserules` on
   `PATH`, or one without the `check-commit` subcommand), when it cannot
   resolve the install root of the hook, and when the parent of the hook
   directory holds no `.houserules.json`.

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

7. **Merge from the CLI, per the project's own discipline.** The default
   here is fast-forward-only, never a UI merge button:
   ```sh
   git switch main && git pull --ff-only
   git merge --ff-only <branch>
   git push origin main
   ```
   A project that has ruled a different merge discipline (a merge commit,
   a squash) follows that ruling instead. GitHub marks the PR merged once
   the commits reach main.

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

- **Main moved since branching.** `git merge --ff-only` will fail. Run
  `git fetch origin main`, rebuild the branch with `git rebase origin/main`
  (step 4), re-verify green, then aggregate commits as in step 4.
- **Force-pushing main.** Force-push (`-f`) is only ever for feature
  branches. Never force-push main.
- **Adding a co-author line without a ruling that allows it.** The default
  here is no co-author trailers, in aggregated commits or anywhere else in
  this procedure; change it only by ruling the project's own attribution
  convention first. The `Work-Item:` and `Deferred:` trailers are no
  co-author lines.
- **Merging via a UI button without a ruling that allows it.** The default
  here is always ff-only from the CLI; change it only by ruling the
  project's own merge discipline first.
