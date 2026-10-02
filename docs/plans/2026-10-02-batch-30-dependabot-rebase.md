# Batch 30 plan — the Dependabot rebase request

Spec: docs/specs/2026-10-02-batch-30-dependabot-rebase.md (approved
2026-10-02, section 7). Branch: batch-30. Pipeline: standard
(implementer sonnet → task-reviewer opus; branch-reviewer fable at the
end; strictly sequential). Workspace:
`.superpowers/sdd/2026-10-02-batch-30/`.

Goal: one new workflow asks Dependabot to rebase one behind PR per push
to main. One task. No CLI surface, no schema, no template, and no
dependency changes.

## Global constraints

- No subcommand, no flag, no schema constraint change
  (`houserules.1-0-surface-is-frozen`).
- Nothing under `template/` changes: the workflow belongs to this
  repository, not to the kit. So no payload stamp and no evals rerun.
- No `uses:` line in the new workflow: no action, no pin, no dependency
  (`houserules.actions-pinned-by-sha` has nothing to pin).
- The step names `shell: bash`
  (`houserules.actions-default-shell-lacks-pipefail`).
- Every string the step prints is contract text: a test pins it
  (`quality.user-facing-text-is-contract`).
- A test that mints a scratch directory uses `tempfile::TempDir`.

## Code-health scan (process.code-health-scan)

- `crates/houserules/tests/dependabot_config.rs` (285 lines): the
  script test builds its stub `gh`, scratch directory, and `PATH` inline
  (about 25 lines). The new script test needs the same three. Smell:
  duplication if copied. Targeted fix: the task moves that setup into
  one local helper that both script tests call.
- `crates/houserules/tests/workflow_reader/mod.rs` (158 lines): its
  module doc binds every user to use every helper, or `cargo clippy
  --all-targets -- -D warnings` fails on dead code. A third test file
  for the new workflow would have to use all eight helpers, `indent`
  included. No smell in the file; a constraint on where the new tests
  live (the ruling below).
  `workflow_steps` reads the steps of the one job; the new workflow has
  one job, so the helper fits. The helpers read step-level keys only;
  the new tests read three top-level blocks (`on:`, `concurrency:`,
  the job's `if:`) with `nested_under` and line reads.
- `.github/workflows/dependabot-auto-merge.yml` (46 lines): clean. Its
  header comment says where the token lives; after this batch the token
  lives in two stores. Targeted fix: the comment names both.
- `knowledge/houserules.json`: the entry
  `houserules.auto-merge-waits-for-every-ruleset-rule` states the
  defect and no repair. Targeted fix: the task adds the consequence.

Ruling (controller, against spec section 5, "a new file beside
`dependabot_config.rs`"): the new tests go into `dependabot_config.rs`.
The reader's rule forbids a third user that leaves a helper unused; a
test file that reads one workflow has no natural use for all eight, and
an `allow` would defeat that rule. The file's subject, the Dependabot
wiring, covers the new workflow. If wrong, the cost is one file move.

## Review focus

Inputs the spec implies and that are most likely to bite:

1. The compare read returns text that is no number (an API error body,
   `null`): the step must fail, not skip the PR. A bare `[ "$x" -gt 0 ]`
   inside `if` reads a non-number as false.
2. The list is empty: the step must pass and post nothing.
3. A behind PR without auto-merge has a lower number than a behind PR
   with it: the comment goes to the second.
4. A read fails after an earlier PR was judged up to date: no comment,
   non-zero exit.
5. The secret is empty: the named error, and no `gh` call at all.

Each is a case of the script test in task 1.

## Task 1 — the workflow, its tests, and its knowledge (HR-156)

Files:

- Create: `.github/workflows/dependabot-rebase.yml`
- Modify: `crates/houserules/tests/dependabot_config.rs`,
  `.github/workflows/dependabot-auto-merge.yml` (header comment only),
  `knowledge/houserules.json`, generated `.claude/rules/*.md`

Design: spec sections 2 to 5. The step's script, as a draft that the
implementer verifies against the current `gh` manual and the tests:

```bash
if [ -z "$GH_TOKEN" ]; then
  echo "::error::The Actions secret DEPENDABOT_AUTOMERGE_TOKEN is empty or not set. Set it with: gh secret set DEPENDABOT_AUTOMERGE_TOKEN"
  exit 1
fi
pulls=$(gh pr list --author 'app/dependabot' --state open --limit 100 --json number,headRefName,autoMergeRequest --jq '.[] | "\(.number) \(.headRefName) \(.autoMergeRequest != null)"')
sorted=$(sort -n <<< "$pulls")
while read -r number branch auto; do
  [ "$auto" = true ] || continue
  behind=$(gh api "repos/$GH_REPO/compare/main...$branch" --jq .behind_by)
  case $behind in
    '' | *[!0-9]*)
      echo "::error::The compare API returned '$behind' as behind_by for pull request #$number. Expected a number."
      exit 1
      ;;
  esac
  if [ "$behind" -gt 0 ]; then
    gh pr comment "$number" --body '@dependabot rebase'
    echo "Asked Dependabot to rebase pull request #$number: $behind behind main."
    exit 0
  fi
done <<< "$sorted"
echo "No Dependabot pull request with auto-merge is behind main."
```

The `--jq` expression only projects three fields per PR. Selection,
order, and the stop at the first behind PR are shell, so the stub-`gh`
test proves them. The projection itself is proven by the controller's
live run against the real repository.

Steps:

1. Tests first, failing because the workflow file does not exist:
   the shape pins and the script cases of spec section 5, plus the
   non-number case of the review focus.
2. The workflow.
3. The helper extraction in the test file (code-health scan).
4. The comment of `dependabot-auto-merge.yml`; the knowledge entries;
   `houserules render`.
5. Gates at HEAD; the report.

## After the task (controller)

1. Task review, fixes, then the branch review.
2. Live run before the PR (`process.live-run-before-ci`): the step's
   own script against the real repository, with a `gh` wrapper that
   forwards every read and records the comment call without sending
   it. Expected at plan time: it selects PR #48. Since PR #48 merged
   (spec section 8), it selects nothing. Also `shellcheck` over the
   extracted script (no gate reads it yet, HR-155).
3. Finish: tick nothing yet (HR-156 and HR-098 close on the rollout
   proof); aggregate; PR; the owner sets the Actions secret and merges.
4. Rollout proof (spec section 5): the first run on main proves that
   the secret reaches the step and proves the list call; the first run
   that finds a behind PR with auto-merge on proves the check read and
   the request. Then HR-156 and HR-098 are ticked
   and swept on the next branch.
