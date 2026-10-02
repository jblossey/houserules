# Batch 30 — a Dependabot PR that falls behind main gets its rebase

Items: HR-156. HR-098 closes with it, on the proof that section 5
names.
Kickoff: the owner's direction of 2026-10-02 (design.md 5.89).
The batch runs before batch 29. Its branch also carries the batch-28
rollout record.

The owner ruled sections 2.1 and 2.2 in the brainstorm of 2026-10-02.
Section 7 is the gate for the written spec as a whole.

## 1. The defect (verified 2026-10-02)

- The `main` ruleset requires an up-to-date branch
  (`strict_required_status_checks_policy: true`). A push to main while
  a Dependabot PR is open leaves the PR behind. Its enabled auto-merge
  then waits (`houserules.auto-merge-waits-for-every-ruleset-rule`).
- PR #48 showed it. The workflow approved it and enabled auto-merge 13
  seconds after it opened; every check passed; the release commit of
  1.2.0 reached main first. The compare API read `behind_by=1`. The PR
  waited for almost two hours, until the owner's account asked
  Dependabot to recreate it (section 8).
- GitHub's documentation promises a Dependabot rebase only for
  conflicts ("By default, Dependabot automatically rebases pull
  requests to resolve any conflicts", read 2026-10-02). PR #32 was
  behind main from 2026-09-21, and the weekly runs of 2026-09-25 and
  2026-10-02 left it untouched.
- What Dependabot did after a merge of one of its own PRs, one
  observation each: PR #32 (github-actions) stayed behind after PR #31
  (cargo) merged; PRs #32 and #43 were rebased about two minutes after
  PR #48 (github-actions, like both) merged (section 8).
- A rebase by any actor other than Dependabot is no repair. The same
  documentation says that Dependabot stops rebasing a PR "once extra
  commits have been pushed to it".

## 2. Design

### 2.1 The mechanism (ruled: a rebase request, one PR per push)

A new workflow, `.github/workflows/dependabot-rebase.yml`, runs on
every push to `main`. It has one job with one script step:

1. List the open PRs, from the repository's own list and not from the
   search index (section 8).
2. Keep the PRs whose author is Dependabot and that have auto-merge
   enabled.
3. In ascending PR number: ask the compare API how far the head
   commit is behind `main`; skip a behind PR that has a failed check
   at its current head (section 8). Stop at the first behind PR that
   is not skipped.
4. Comment `@dependabot rebase` on that PR. With no such PR, do
   nothing and pass.

The chain after the comment uses only what exists. Dependabot rebases
its branch. The `dependabot-auto-merge` workflow runs on the
`synchronize` event: it approves the PR if it is not approved, and runs
its merge step again. The checks run, and the PR merges. That merge is a push to main, so this workflow runs again and
serves the next PR.

One PR per push is the ruled choice: k behind PRs cost k check rounds
in series. A request to all of them at once costs about k(k+1)/2
rounds, because each merge puts the others behind again.

Details:

- The step reads "behind" from the compare API
  (`repos/<repository>/compare/main...<head commit id>`, field
  `behind_by`), not from `mergeStateStatus`. It names the head by its
  commit id, not by its branch: a commit id needs no escaping in a URL
  (section 8). GitHub computes that
  field lazily: on 2026-10-02, PRs #32 and #43 read `UNKNOWN` while
  PR #48 read `BEHIND`.
- The job carries the same repository guard as `dependabot-auto-merge`
  (`github.repository == 'jblossey/houserules'`), so a fork runs
  nothing.
- The workflow sets a `concurrency` group and does not cancel a run in
  progress. GitHub keeps at most one pending run per group: a newer
  pending run replaces an older one, which then shows as cancelled.
  No PR is lost, because each run reads the live state.
- The step names `shell: bash`
  (`houserules.actions-default-shell-lacks-pipefail`).
- A failed read fails the job. The step posts no comment after a
  failed read.

### 2.2 The token (ruled: the auto-merge token, in the Actions store)

The step makes its one comment as the owner, with
`DEPENDABOT_AUTOMERGE_TOKEN` (Contents and Pull requests, read and
write, this repository only). Its reads use the workflow's own token
under the read permissions that the workflow declares (section 8).
GitHub's reference lists `@dependabot rebase` and does not say whose
comments Dependabot obeys. The owner's comment is the case that is not
in doubt; a comment from the Actions bot (`GITHUB_TOKEN`) is the
doubtful case, and the owner ruled it out.

The store a run reads depends on who starts the run, not on the
event: a run that Dependabot starts reads Dependabot secrets, and
every other run reads Actions secrets. Every push to main has the
owner as its actor (section 3), so this workflow reads the Actions
store. Verified on PR #31: the push that an auto-merged Dependabot PR
makes has the owner as its actor too. The secret therefore exists in
both stores under one name.

An empty secret is a named error, not a failed `gh` call: the step
prints `::error::` with the secret's name and the command that sets
it, and fails.

The step uses no action and no checkout: `gh` and the two tokens are
enough. So the batch adds no dependency and no pin.

## 3. The walk (`process.gates-cover-generated-commits`)

Who pushes to `main`, and what the new workflow does on each push:

| Push | Actor | Result |
|---|---|---|
| The owner's fast-forward merge of a batch branch | owner | Runs; asks for one rebase if a PR qualifies |
| The auto-merge of a Dependabot PR | owner (the token enabled it) | Runs; serves the next PR in the chain |
| The owner's merge of the release-please PR | owner | Runs; same as the first row |
| A tag push | — | Does not run: the trigger names the `main` branch only |
| A push made with `GITHUB_TOKEN` | Actions bot | Starts no run. No workflow in the tree pushes a commit to main |
| A push by Dependabot itself | Dependabot | Cannot occur today: Dependabot has no merge command. Such a run would read the Dependabot store, which holds the same secret |

States a run can end in: passed with one comment; passed with none;
failed (the accepted reds below); cancelled while pending, when a newer
push replaced it in the concurrency group.

Resources the workflow causes: one PR comment per run at most; the
rebase and the force-push that Dependabot makes on that request, or
Dependabot's reply comment when it does not rebase; one check round
on the rebased head. No other automation writes the request comment.
`dependabot-auto-merge` owns the approval.

Accepted reds:

- **The secret is not in the Actions store.** Every run fails with the
  named error until the owner sets it. The owner sets it before the
  merge (section 6).
- **GitHub or the API is unreachable.** The run fails. The next push
  to main repeats the request; a re-run of the failed job does too.
- **A read returns a value the step does not know**: a compare value
  that is no number, a check state outside the five that `gh` prints,
  or a count of checks that is no number. The run fails with a named
  error and posts no comment.

## 4. Limits, stated

- **A push that lands in the seconds before auto-merge is enabled.**
  The run sees a PR without auto-merge and skips it. The next push to
  main picks it up. PR #48 had auto-merge 13 seconds after it opened.
- **Head-of-line.** The workflow always asks for the lowest-numbered
  behind PR that is not red. If Dependabot does not rebase it (the
  documentation names a PR with extra commits and a PR older than 30
  days), younger PRs wait behind it until the owner closes or merges
  it. The stuck PR stays visible: open, auto-merge on, one rebase
  comment per push to main.
- **A red PR gets no request.** A PR whose checks failed at its
  current head is skipped, so it cannot hold the line (section 8). If
  a rebase would turn it green, the owner asks for that rebase.
- **A second push before Dependabot acts** repeats the comment on the
  same PR. The cost is the repeated comment.
- **What counts as red.** The step reads the check state as `gh`
  groups it. `gh` puts the conclusions `STALE` and `STARTUP_FAILURE`
  into its pending group, so a PR with only such checks is not red and
  gets its request.
- **Major updates** carry no auto-merge and get no request. They stay
  with the code owner, as ruled in HR-098.
- **The author value.** The step selects the PRs whose author `gh`
  prints as `app/dependabot` (measured). If `gh` ever printed another
  value, the step would select nothing and pass.
- **The first real request is the first proof of the check read.**
  The docs map REST endpoints to permission scopes; `gh` reads check
  state through GraphQL. The read runs only for a behind PR with
  auto-merge on, so no run before that one exercises it.

## 5. Tests and proof

Tests, in `dependabot_config.rs` (section 8), through the shared
workflow reader:

- Shape pins: the trigger is a push to `main` and nothing else; the
  repository guard; the secret's name; `shell: bash`; the concurrency
  group with no cancel.
- The step's own script, run under the shell Actions uses, against a
  stub `gh` (the pattern of the approve-script test):
  - no open Dependabot PR: no comment, pass;
  - one PR, auto-merge on, behind: one comment on it, with the exact
    text `@dependabot rebase`;
  - two such PRs: one comment, on the lower number;
  - a behind PR without auto-merge before a behind PR with it: the
    comment goes to the second;
  - auto-merge on, not behind: no comment;
  - the list call fails, or a compare call fails: the step fails and
    posts no comment;
  - an empty token: the named error, no `gh` call.

  Added after approval (section 8): a PR by another author is not
  selected; a behind PR with a failed check is skipped with one
  printed line; pending and absent checks do not skip; a failed or
  unknown check read fails the step; an up-to-date PR costs no check
  read; every read carries the workflow token, and the comment alone
  carries the owner's token; the exact permission set, the top-level
  key set, and a table of forbidden shapes with the harmless edits
  that must stay green.

Live proof before the PR (`process.live-run-before-ci`): the step's
script runs on this machine against the real repository, with a `gh`
wrapper that passes every read through and records the comment call
without sending it. During the task it selected PR #48; since PR #48
merged it selects nothing, which is right for that state (section 8).

Rollout proof, after the merge. The first run on main proves that the
secret reaches the step (the guard passes), and proves the list call
and the projection inside Actions. The first run that finds a behind
PR with auto-merge on proves the rest: the check read, and the request
with the owner's token. It comments on that PR, Dependabot rebases it,
and the PR merges with no owner action. That run closes HR-156 and
HR-098.

## 6. Owner acts

1. **Before the merge:** store the token in the Actions store:
   `gh secret set DEPENDABOT_AUTOMERGE_TOKEN`. A fine-grained token
   cannot be read back. If its value is not at hand, regenerate the
   token and set it in both stores (`--app dependabot` for the
   second).
2. **The merge of the batch PR**, as for every batch.

## 7. Approval

Approved as written by the owner on 2026-10-02. The plan follows.

## 8. Corrections after approval (controller record)

- **Section 2.2, the secret stores.** The approved text said that a
  workflow that a push starts reads Actions secrets. GitHub's page
  says the store follows the actor: a push that Dependabot starts
  reads Dependabot secrets. The task review found it; section 2.2 now
  states the actor rule. The design does not change, because every
  push to main has the owner as its actor.
- **Section 2.1, the compare call.** The approved text named the head
  branch in the compare URL. The task review showed that `gh api`
  cuts a URL at `#`, so a branch name with that character would be
  compared as another ref and read as up to date. The controller
  ruled: the call names the head commit id. If wrong, the cost is one
  changed field in the list call.
- **Section 5, the tests' home.** The approved text said "a new file".
  The plan ruled the tests into `dependabot_config.rs`: the shared
  workflow reader admits no user that leaves one of its helpers
  unused.
- **A follow-up outside the batch.** `mise run lint` prints five
  cargo-deny warnings for duplicate crate versions on every run; the
  task review found a report that called the output clean. HR-157
  files the warn level for a ruling.
- **Section 1 and section 5, PR #48.** The approved text named PR #48
  as the rollout proof. During the build, the owner's account
  commented `@dependabot recreate` on PRs #48, #43, and #32. PR #48
  merged at 10:39:38 UTC on 2026-10-02 with the approval and the
  auto-merge that the workflow had set: the second half of the chain
  in section 2.1 is thereby observed. The rollout proof is now the
  first run on main that finds a behind PR with auto-merge on. The
  first run after the merge still proves that the secret reaches the
  step, and proves the list call and the projection inside Actions.
- **Section 1, two PRs of one weekly run.** The approved text said the
  first merge leaves the second behind. After PR #48 merged, Dependabot
  rebased PRs #32 and #43 about two minutes later (122 and 128
  seconds), with no conflict to resolve. That is one observation, not
  a rule. The other observations: PR #32 stayed behind after PR #31
  (another ecosystem) merged; the release commit, a push by the owner,
  left PR #48 behind for almost two hours; the batch-28 push, which
  changed `.github/dependabot.yml`, was followed by rebases of PRs #32
  and #43 two minutes later. Where Dependabot and this workflow both
  ask for the same rebase, the cost is one repeated check round.
- **Section 2.1, the approval after a rebase.** The approved text said
  the push dismisses the approval and the workflow approves again. On
  PR #48 the timeline shows no dismissal and no second review after
  Dependabot's force-push; the PR merged on the first approval. The
  workflow's approve step covers both cases: it approves only a PR
  that is not approved.
- **Section 2.1, the list call (branch review).** `gh pr list` with
  an author filter reads GitHub's search index. A merged PR keeps its
  auto-merge request, and its old head reads as behind, so a stale
  index entry right after a merge would get the comment. The lag of
  the index is not measured. The controller ruled: the step lists
  without a filter flag, which reads the repository's own list, and
  selects the author in the shell. If wrong, the cost is one more
  projected field.
- **Section 2.1 and section 4, a red PR (owner ruling, 2026-10-02, at
  the branch review).** The branch review showed a cause of
  head-of-line that the approved limits did not name: a PR whose
  checks fail is selected on every push, costs one failing check
  round each time, and starves younger PRs. The owner ruled: the step
  skips a PR whose checks have failed at its current head.
- **Section 2.1, the concurrency group (branch review).** The approved
  text said two pushes close together run one after the other. GitHub
  keeps one pending run per group; section 2.1 and section 3 now say
  so.
- **The commit type.** The aggregated commit that carries the workflow
  is `ci(dependabot)`. Its tests live under the crate's path, and a
  `feat` or `fix` there would make release-please propose a release
  for a change that ships nothing to an adopter.
- **Section 2.1, the order in step 3 (controller ruling).** The step
  reads the check state only for a PR that the compare call shows
  behind. The selection is the same; an up-to-date PR costs no extra
  read.
- **Section 2.2, two tokens (controller ruling, fix wave).** The
  approved text gave the owner's token to the whole step. The token
  carries Contents and Pull requests only. Whether it can read check
  state is not documented, and an empty answer would read as "no
  checks", which would switch off the red skip in silence. So the
  reads use the workflow's own token under declared read permissions
  (`contents`, `pull-requests`, `checks`, `statuses`), and the owner's
  token makes the comment only. The owner's ruling holds for the call
  that needs the owner's account. If wrong, the cost is three more
  permission lines.
- **The branch was rebuilt on main twice**, after PR #48 merged and
  after PRs #32 and #43 merged. Neither rebuild had a conflict. The
  maps of old to new commit ids are in the batch workspace
  (`sha-map-rebuild-on-main.txt`, `sha-map-rebuild-on-main-2.txt`).
- **The branch review's proposals.** Four knowledge entries are added
  with the batch (`quality.exclusive-pins-are-mutation-proven`,
  `quality.selecting-steps-walk-the-states-of-what-they-select`,
  `houserules.gh-filtered-lists-read-the-search-index`,
  `houserules.commit-type-under-the-crate-path-decides-a-release`).
  HR-153 takes the rest for batch 29: two clauses for the kit-shipped
  entry `knowledge-base.state-only-the-source`, three clauses for
  standing entries, which only the owner rules, and four template
  defects.

## 9. Outcome (controller record, 2026-10-02)

- Built: `.github/workflows/dependabot-rebase.yml`, its tests in
  `crates/houserules/tests/dependabot_config.rs` (the shape pins, a
  table of forbidden shapes with the harmless edits that stay green,
  and the step's own script against a stub `gh`), the header of
  `dependabot-auto-merge.yml`, and six knowledge entries added or
  changed.
- Reviewed: one task review with two fix rounds, the branch review,
  and one fix wave with three rounds. Section 8 lists every change to
  the approved text.
- Proven before the merge: the step's script, run on this machine
  against the real repository through a wrapper that forwards reads
  and sends nothing. While PR #48 was behind, it selected PR #48.
  With no PR open, it selects nothing.
- Not proven before the merge, by nature: any run inside Actions. The
  first run on main proves that the secret reaches the step, and
  proves the list call. The first run that finds a behind PR with
  auto-merge on proves the check read under the declared permissions
  and the request with the owner's token.
- HR-156 stays partial and HR-098 stays partial until that second
  run. The record of it rides the next branch.
- Open with the owner: the Actions secret (section 6); HR-157 (the
  cargo-deny warn level); the clauses and template defects in HR-153.
