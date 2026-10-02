# Batch 28 — the issue batch and the HR-098 repair

Items: HR-098 (Dependabot auto-merge, the repair), HR-141 (issue 35),
HR-142 (issue 36), HR-143 (issue 37), HR-145 (issue 39), HR-146
(issue 40), HR-147 (issue 41, the refusal text only).
Kickoff: the owner's direction of 2026-10-01 (design.md 5.87).

Not in this batch: HR-144 (issue 38, the design-phase review gate). It
forms batch 29 with HR-140 (design.md 5.87).

Every design in section 3 was ruled or approved by the owner in the
brainstorm of 2026-10-01, one item at a time. Section 6 is the gate for
the written spec as a whole.

## 1. Constraints that shape every task

- **The 1.0 surface freeze holds** (`houserules.1-0-surface-is-frozen`,
  design.md 5.82). A task adds no subcommand, no flag, and no schema
  constraint change unless its own section records an owner ruling.
- **The template is the source** (`houserules.template-is-the-source`).
  Every kit edit lands in `template/`; `houserules update --dir .`
  through the tree binary syncs the root copies.
- **One shared eval rerun** (`process.evals-rerun`, the ledgered
  economy). Several tasks edit the agent templates. The batch books one
  rerun of every `.claude/evals/` scenario at the final template state,
  before the branch review. Each interim task audit declares the
  co-change fail with the `--sanctioned` flag and cites this section as
  its reference. The branch-range audit after the record commit is the
  row that must pass.

## 2. Triage digest (verified 2026-10-01)

- **HR-098.** PR #31 got auto-merge enabled by the owner's token and
  then waited three days for the owner's approving review. An enabled
  auto-merge waits for every ruleset requirement; the admin bypass does
  not apply to it. The `approval` ruleset has
  `require_last_push_approval: false`, and the owner is the sole code
  owner. PRs #33 and #34 fail CodeQL with `Loaded a configuration file
  for version '4.38.0', but running version '4.38.1'`: Dependabot bumps
  `github/codeql-action/init` and `/analyze` in separate PRs.
- **Issue 39.** A non-JSON file named `task-*-audit*.json`,
  `task-*-review*.json`, or `task-*-report.json` makes `stats` exit 2;
  a non-JSON `.json` file with another name is ignored. Without git,
  `check-knowledge` and `check-backlog` exit 2 with git's own
  `fatal: not a git repository`; both pass with `--dir .`.
- **Issue 40.** A fresh `init` seeds `process.evals-rerun`. With the
  entry deleted, `check-knowledge` still passes while the orchestrating
  skill and the branch-reviewer template cite the id.

## 3. Design

### HR-098 — the auto-merge repair (approved in the brainstorm)

- `.github/dependabot.yml`: the `github-actions` entry gains a group
  `codeql-action` with the pattern `github/codeql-action*`.
- `.github/workflows/dependabot-auto-merge.yml`: for a minor or patch
  update, a new step approves the PR with `DEPENDABOT_AUTOMERGE_TOKEN`
  when its review decision is not `APPROVED`. The existing step then
  enables auto-merge. A major update gets neither step. The header
  comment states the mechanism.
- Author classes: a Dependabot minor or patch PR gets the approval and
  the auto-merge; a Dependabot major gets nothing; an owner PR, a
  release-please PR, and a fork PR skip the job on the `user.login`
  condition. The workflow runs again on every push to the PR, so a
  dismissed stale review is submitted again.
- The implementer verifies against current docs: that the group pattern
  matches the `/init` and `/analyze` sub-paths, and the fine-grained
  token permission that submitting a review needs.
- Proof: the live run. HR-098 flips to done with the number of the
  first minor or patch Dependabot PR that merges with no owner action.
- Out of scope: PR #32 (the fetch-metadata major). The owner merges or
  closes it.

### HR-143 — a report-builder script validates its output (approved in the brainstorm)

- `template/.claude/agents/implementer.md`, Report section: the edit
  rule gains one sentence. "A script that builds or edits the report
  runs `houserules validate` on its output and exits non-zero when
  validation fails."
- No new check. The audit's `--report` run and the validate step
  already catch an invalid report; the sentence moves the catch earlier.

### HR-142 — the checker proves the `natural` label (approved in the brainstorm)

Ruled: the proof lives in `check-report-claims`, not in pasted
timestamps. The `tdd` entry has no field for a timestamp, and a new
field is a frozen-surface change.

- `crates/houserules/src/report_claims.rs`: one new check. For every
  `tdd` entry with `mode: natural` whose `red.command` redirects to a
  capture file, compare the file's mtime with the committer time of the
  newest commit the report lists (`commits[]` and
  `fix_rounds[].commits[]`). A capture newer than that commit is a
  finding. The finding names the entry, the file, both times, and the
  fix: label the entry `reconstructed`.
- Skips: an entry with no redirect, a missing capture file, and an
  unresolvable sha produce no finding in this check.
- The module doc's Limits list gains two bullets: the check does not
  see a RED captured between two commits of one task; an mtime holds
  only in the workspace that made the capture.
- `template/.claude/agents/implementer.md`, the `tdd` bullet: one
  sentence. Capture each RED with `> <file> 2>&1`;
  `check-report-claims` fails a `natural` entry whose capture is newer
  than the newest listed commit.
- No flag and no schema change.
- Tests, failing first: a natural entry with an older capture passes; a
  natural entry with a newer capture fails; `reconstructed` and
  `mutation` entries are ignored; each skip case stays silent. The task
  also runs the check over the retained report corpus and records the
  false-positive count.

### HR-141 — cited lines are re-opened at HEAD (approved in the brainstorm)

Ruled: the checker gains a heuristic rule, gated by a corpus run.

- `crates/houserules/src/report_claims.rs`, extending
  `check_citation_lines`: for a citation with a line or a line range,
  take the sentence that holds it. When that sentence holds backticked
  fragments (the citation's own path excluded), at least one fragment
  must occur in the cited line or range at HEAD. When none occurs, the
  finding names the citation, the fragments looked for, and the text at
  that line.
- Skips: a sentence with no backticked fragment; a citation without a
  line number; a file that does not resolve (the existing check reports
  it).
- Corpus gate: before the rule is wired in, the task runs it over every
  retained deliverable and records the count. With zero false positives
  the rule ships. Otherwise the task narrows the trigger, records the
  new count, and names what stays uncaught in the Limits list.
- `template/.claude/agents/implementer.md`, the closing act: two
  sentences. After the last edit, list every `file:line` citation in
  the report and re-open each cited line at HEAD. Copy each command
  field from the command you ran; never retype it.
- No flag and no schema change.
- Tests, failing first: a citation whose line holds the fragment
  passes; a drifted line fails; a range is honored; each skip case
  stays silent.
- Out of scope: citations in reviews (HR-128).

### HR-145 — the tooling defects of issue 39 (each part ruled in the brainstorm)

**Part 1, `stats` stays loud and names the remedy.** The abort on a
non-JSON deliverable stays (`quality.gates-derive-their-scope`).

- `crates/houserules/src/rules/stats.rs`: the error for an unreadable
  file gains the remedy. `stats` reads every `task-*-audit*.json`,
  `task-*-review*.json`, and `task-*-report.json` as a deliverable;
  rename or repair the file.
- `template/.claude/agents/task-reviewer.md`: one sentence. Probe
  output goes to a `.txt` file, never to a `task-*` name.
- No output-shape change and no exit-code change.

**Part 2, the audit passes the draft state.**

- `crates/houserules/src/rules/audit.rs`: in a single-report `--report`
  run, a `report-field` check on `self_audit` passes when the field is
  null. The row is byte-identical to the row a finished report gets, so
  the rows an implementer copies into `self_audit` stay true.
- A `--workspace` run stays strict: a null `self_audit` fails there.
- `houserules validate` keeps rejecting a report with status `DONE` or
  `DONE_WITH_CONCERNS` and a null `self_audit`. A test pins that guard
  beside the new behavior.
- `process.deliverables-json` in both knowledge copies: the body
  sentence "the check below fails until it does" is replaced with the
  new contract. The implementer template's `self_audit` wording follows.

**Part 3, a command outside git names the way out.**

- `crates/houserules/src/root.rs` (`resolve_root`, with
  `rules::repo_root_from_cwd`): with no `--dir` and no enclosing
  repository, the command prints its own line in place of git's raw
  `fatal:` text. The line says the directory is not inside a git
  repository and names the remedy: pass `--dir <repository root>`.
  Exit 2 stays. No fallback to the current directory.
- No template sentence.

**Part 4, the empty eval record is a defined state.** No seeded
baseline: a shipped run set would record runs the adopter never made.

- `template/.claude/agents/branch-reviewer.md`: an empty record is a
  finding only when the range changes a template or a scenario.
- `template/.claude/skills/orchestrating/SKILL.md`, Template
  evaluation: the first run set appended is the baseline. It is due at
  the first batch that changes a template or a scenario, an `update`
  that brings new templates included.

Tests, failing first, for parts 1 to 3: the remedy text in the `stats`
error; the draft-state pass, the strict `--workspace` fail, and the
`validate` guard; the named no-repository line and its exit code.

### HR-146 — a kit-owned file never cites a missing knowledge id (ruled in the brainstorm)

Triage corrected the issue: the kit ships `process.evals-rerun`. An
adopter may delete a seeded entry; the kit-owned files that cite it stay
out of the adopter's reach. Five of the eight kit-owned files cite 19
distinct seeded ids (enumeration: every backticked `<topic>.<slug>`
token in the `kitOwned` files that `houserules files` lists, with a
knowledge topic as its prefix).

Ruled: a lint plus one skill sentence.

- `check-knowledge` gains a lint. For every kit-owned file present in
  the repository, each backticked `<topic>.<slug>` token whose prefix
  is a knowledge topic must resolve to a live entry. A miss is a
  failure that names the file, the id, and the remedy: restore the
  entry. A token with no topic prefix (`verdict.text`, `source.date`)
  is not an id.
- A kit-side test proves the shipped state: every id the kit-owned
  files under `template/` cite resolves in `template/knowledge/`.
- `template/.claude/skills/migrating-knowledge/SKILL.md`: one sentence
  at the step where an adopter prunes seeded entries. Before you delete
  a seeded entry, search the kit-owned files for its id.
- No flag. An adopter project that deleted a cited entry gets a red
  `check-knowledge` after the update; that is the intended result
  (`quality.no-compat-softening`).
- Tests, failing first: a cited id that resolves passes; a deleted
  entry fails with the file and the id in the text; a token with no
  topic prefix stays silent.

### HR-147 — the subdirectory refusal names the real cause (ruled in the brainstorm)

Ruled: batch 28 corrects the text only. The subdirectory install stays
open as HR-147.

- `crates/houserules/src/install.rs` (both sites of `is not a git
  repository (run git init first)`): when the target is inside a git
  repository but is not its top level, the refusal names the enclosing
  top level and says the kit installs at the repository root. The
  `git init` advice stays for a target outside any repository.
- Tests, failing first: a subdirectory target gets the new text; a
  target outside any repository keeps the old text.
- HR-147 stays `open` after the batch.

## 4. Task order and pipeline

Tasks run strictly in sequence (`process.sequential-agents`):
implementer on sonnet, task-reviewer on opus, branch-reviewer on fable
(`process.model-policy`).

| Task | Items | Content |
|---|---|---|
| T1 | HR-098 | The approve step and the `codeql-action` group. |
| T2 | HR-145 parts 1 and 3, HR-147 | Refusals name the way out: the `stats` remedy text and the task-reviewer sentence, the no-repository line, the subdirectory refusal. |
| T3 | HR-145 part 2 | The audit passes the draft state; the entry body and the template wording follow. |
| T4 | HR-142, HR-143 | The `natural`-label check; the implementer sentences for the capture and for report-builder scripts. |
| T5 | HR-141 | The cited-line rule behind its corpus gate; the closing-act sentences. |
| T6 | HR-146, HR-145 part 4 | The dangling-id lint, the kit-side test, the migrating-knowledge sentence; the empty eval record as a defined state. |

After T6:

1. The shared eval rerun at the final template state, recorded in
   `.claude/evals/record.json` (section 1).
2. The live run (`process.live-run-before-ci`): the tree binary runs
   each changed command for real in a scratch install, including a
   scratch copy without git and a subdirectory target.
3. The branch review, then the finish per
   `.claude/skills/finishing-a-feature/SKILL.md`: one to five clean
   commits, a PR, the eight checks, a fast-forward merge.
4. Rollout: the release-please PR is owner-attended. No task changes
   the CLI surface or a schema constraint, so the release is a minor.
5. Acceptance: HR-098 closes on the first minor or patch Dependabot PR
   that merges with no owner action. HR-147 stays open.

T2 to T6 change Rust code under `process.tdd`. T1 changes a workflow
and a Dependabot configuration. That workflow runs only for a
Dependabot PR against main, so it cannot run for real before the merge
(an accepted exception to `process.live-run-before-ci`). Its pre-merge
evidence is the lint gates; its proof is the acceptance in step 5.

## 5. Open points folded into the gate

1. **HR-129 (the template-cluster riders) stays out.** Its four riders
   change the review templates and one adds a schema field. They belong
   with batch 29's design of the review mechanism. Recommendation:
   schedule HR-129 into batch 29.
2. **The issues close through the PR.** Recommendation: the PR body
   carries closing keywords for issues 35, 36, 37, 39, and 40. Issues
   38 and 41 stay open. The owner answers the reporter of issue 41.
3. **PR #32** (the fetch-metadata major) stays with the owner.
4. **The new lint can turn an adopter's gate red** (HR-146). The
   release note for the minor names it and the remedy.

## 6. Approval

Approved as written by the owner on 2026-10-01, the four open points
of section 5 included as recommended. HR-129 is scheduled into batch
29. The plan follows.

## 7. Outcomes and corrections (controller record, for the owner's review)

The build ran from section 3. Each point below is a controller ruling
that refined or departed from an approved design, a finding of the
branch review, or a correction of a fact this spec states. The owner
rules on them at the next checkpoint.

**For the owner to rule: the rollout red of the new lint (HR-154)**

- The seeded CI workflow installs the binary from the latest release.
  The finishing-a-feature skill of 0.3.0, 1.0.0, and 1.1.0 cites
  `knowledge-base.cite-durable-refs`, an id the kit never seeded.
  Under the new binary, `check-knowledge` therefore fails in every
  existing install until `houserules update` runs and its result is
  committed. The branch review reproduced it with an install of the
  1.1.0 kit files (workspace capture
  `branch-review-probes/v110-install-under-head-binary.txt`).
- An install whose finishing skill is locally modified needs more:
  `update` keeps the file, and the adopter removes the citation or
  lists the file in `overrides`.
- Section 5, point 4, and section 3, HR-146, name a narrower red: an
  adopter who deleted a cited entry. The owner approved on that
  premise.
- The lint is not softened. The release note names this red first,
  with the one-command remedy.

**Corrections of facts in this spec**

- Section 2 quotes one CodeQL message for PRs #33 and #34. The logs
  show `Loaded a configuration file for version '4.38.0', but running
  version '4.38.1'` on PR #33 and the same line with the two versions
  exchanged on PR #34.
- Section 3, HR-146, says an adopter cannot reach the kit-owned files.
  That is wrong. `update` overwrites an unmodified kit-owned file,
  keeps a locally modified one and reports it, and leaves an overridden
  one alone.
- Section 3, HR-146, says five of the eight kit-owned files cite 19
  seeded ids. The count of files and ids held at the merge base. The
  word "seeded" did not: one of the 19, `knowledge-base.cite-durable-refs`,
  was never seeded, at the merge base and at every release tag. Task 3
  of this batch added a sixth citing file. After the batch, five files
  cite 18 ids.

**HR-098**

- A second group, `codeql-action-security` with
  `applies-to: security-updates`, covers the class the first group
  misses. A Dependabot group applies to version updates only by
  default.
- Task 1 ships a test file, `crates/houserules/tests/dependabot_config.rs`,
  that pins both groups and runs the approve step's own script against
  a stub `gh`. `process.tdd` binds the change. This supersedes the
  sentence of section 4 that names the lint gates as the only
  pre-merge evidence.

**HR-147 and HR-145 part 3 (the refusals)**

- The subdirectory refusal does not ask git. It walks the ancestors of
  the target's resolved path for a `.git` entry. The line is then true
  under `GIT_DIR`, `GIT_WORK_TREE`, `core.worktree`, and a symlinked
  target. Two costs follow. A repository whose work tree exists only
  through `GIT_DIR` or `core.worktree` gets the `git init` text. On a
  path that runs through a symlink, the line prints the resolved
  spelling.
- Two more failure arms of the root resolution are named: a working
  directory that cannot be read, and a git that cannot be run. A
  missing `GIT_DIR` inside a repository keeps git's own line.

**HR-145 part 2 (the draft audit)**

- The row's pass text in a single-report run is `report field
  self_audit is present`, for a null and for a filled field. The text
  `is set` was false for a null field.
- The template names the statuses: `validate` rejects a null
  `self_audit` at `DONE` and `DONE_WITH_CONCERNS`, and accepts it at
  `BLOCKED` and `NEEDS_CONTEXT`.

**HR-142 (the `natural` label)**

- The finding reads `was modified <N> s after the newest listed commit
  <sha> (committed <T>)`, with `<T>` as git prints it. No calendar code
  and no date dependency were added.
- A capture file that git tracks is skipped: its modification time is
  the checkout's.
- The help of `check-report-claims` names every check the command
  runs. A test derives the list from the source.

**HR-143 (report builders)**

- The approved sentence gained a second one: "While `self_audit` is
  still `null`, the only error it lets pass is `needs a non-null
  self_audit`." Without it, a builder script fails on the draft it
  must produce.

**HR-141 (cited lines)**

- The checker rule did not ship. The corpus gate ran over the 166
  retained task reports. At the narrowest trigger the rule gives 12
  hits: 5 aged, 2 true, 5 false, with three judgment calls that move
  the false count between 3 and 6. The bar is zero false hits. A
  restriction to the `implemented` and `self_review` fields reaches
  zero false hits only with zero true hits, so it was not built. HR-141
  stays open for the checker arm and holds the counts.
- The template step shipped, with a failure branch the approved
  sentences did not have: "A cited line that does not show the claim:
  fix the citation or the sentence, then repeat this step."
- A defect of the existing check was fixed: both numbers of a cited
  range must fit the file. HR-148 files the comma-list form.

**HR-146 (kit-owned citations)**

- A citation is a backtick, an id-shaped token, and a backtick,
  adjacent. The lint models no markdown structure. A span that pads
  the id with a space is not extracted.
- The vocabulary is the kit's: a token is an id only when its prefix
  is a topic the kit seeds. A topic the adopter adds never turns kit
  text into a finding. Section 3 said "a knowledge topic"; the plan
  said a loaded or a seeded topic.
- The finding has two forms. For an id the kit seeds: restore the
  entry, or list each citing file in `overrides`. For an id the kit
  never seeded: run `update`, and for a file `update` keeps, edit the
  citation or list the file in `overrides`.
- A kit-owned file listed in `overrides` is not checked. The seeded
  `docs/README.md` states this in its `overrides` section. Existing
  installs keep their own copy of that file.
- The citation of `knowledge-base.cite-durable-refs` was removed from
  the finishing-a-feature skill. The alternative, to seed that entry,
  is a product decision and was not taken.
- An archived seeded entry counts as missing. The restore path puts it
  back as active and leaves the retired copy in the archive (HR-151).

**Help prose**

- The help text of `check-report-claims`, `check-knowledge`, and
  `init --dir` changed. No command, flag, or exit code changed.

**Knowledge added**

- `houserules.auto-merge-waits-for-every-ruleset-rule` and
  `houserules.codeql-action-steps-share-one-version`, both measured in
  the HR-098 triage.
- From the branch review's retrospective:
  `quality.lint-the-token-not-the-document`,
  `houserules.kit-lints-read-kit-vocabulary`, and body clauses on
  seven entries that are not standing. The clauses the review proposes
  for standing entries wait for the owner in HR-153.

**Follow-up items filed**

- HR-148 (comma-list citations), HR-149 (a dev gate under
  `CARGO_TARGET_DIR`), HR-150 (the closing act of the implementer
  template), HR-151 (side effects of the citation remedy), HR-152
  (`stats` counts a sanctioned fail), HR-153 (template and rule riders,
  batch 29), HR-154 (the seeded CI and the latest binary).

## 8. Checkpoint rulings and task 7 (owner, 2026-10-02; design.md 5.88)

The owner ruled at the checkpoint before the push:

- The citation lint ships as built. The release note states its red
  first, with the remedy `houserules update`.
- The seeded CI workflow installs the version `.houserules.json`
  records (HR-154). This batch builds it as task 7.
- The citation of `knowledge-base.cite-durable-refs` stays removed
  from the finishing-a-feature skill. The entry is not seeded.
- Issue 35 stays open. The PR closes issues 36, 37, 39, and 40. This
  supersedes section 5, point 2.

### Task 7 - the seeded CI installs the stamped version (HR-154)

- `template/.github/workflows/knowledge.yml`: the install step reads
  `version` from `.houserules.json` and downloads the installer of
  that release (`releases/download/v<version>/houserules-installer.sh`).
  The workflow text holds no version literal, so a release rewrites
  nothing in `template/`.
- A stamp with no `version` falls back to the latest release and
  prints a notice that names the reason. A recorded version with no
  release fails the step with a named line.
- The binary and the kit files of an install then move together: a
  new check reaches an install when it runs `houserules update` with
  the new binary and commits the result.
- The workflow is a seed-once file. An existing install keeps its own
  copy; the release note prints the changed step.
- The texts that state the old mechanism follow: `docs/runbook.md`,
  `README.md`, the migrating-knowledge skill's section on a CI gate
  for another host, and the doc comments that name `releases/latest`
  for this workflow.
- Tests, failing first: a pin of the step's shape, and a run of the
  step's own script against stubs for the three states (a recorded
  version, no version, a version with no release).
- Proof before the merge: the step's script runs for real against a
  published release, with the home directory pointed at a scratch
  directory. Proof after the release: the seeded-repository run of
  `houserules.seeded-repo-live-proof`, owner-attended.

### Task 7 outcome (controller record)

- The step reads the version with `sed`, not with `jq`. The invariant
  `houserules.payload-runs-on-builtins` stays as written: a new tool on
  an adopter's runner needs the owner; a POSIX read does not.
- The read matches the top-level `version` line in the two-space
  layout that houserules writes. A stamp in another layout (minified,
  re-indented) falls back to the latest release, and the notice names
  the layout the step reads.
- A version passes only as MAJOR.MINOR.PATCH with an optional semver
  prerelease. The value is checked before it reaches a URL, because a
  pull request can edit the stamp.
- The two workflow test files share one reader module.
- HR-154 stays partial. The runbook's release procedure names its two
  duties: the owner-attended seeded-repository run, and the release
  note that prints the changed step. HR-155 files the missing lint
  over a script embedded in a workflow.

## 9. Rollout (controller record, 2026-10-02; design.md 5.89)

- The owner reviewed and merged PR #46 and released 1.2.0.
- HR-154 is closed. The release note of 1.2.0 states the
  `check-knowledge` red first and prints the install step; the owner
  approved the text. The seeded-repository run is green: the step
  downloaded 1.2.0 with the stamp at 1.2.0, and 1.1.0 with the stamp
  edited to 1.1.0 while 1.2.0 was the latest release. The second run is
  an addition to the runbook's procedure: a run whose stamp names the
  latest release cannot show which release the step reads.
- HR-098 stays partial. PR #48 proves both parts of the repair (the
  approval with the owner's token; the two CodeQL steps in one PR, both
  `analyze` jobs green). It did not merge: the release commit reached
  main first, and the `main` ruleset requires an up-to-date branch.
- HR-156 files that stall. The owner first ruled to observe the weekly
  Dependabot run of 2026-10-09. The controller then read the timeline
  of PR #32, which it had not read before it asked: two weekly runs
  left that PR behind main untouched. The owner superseded the ruling:
  the fix is designed now, as batch 30.
