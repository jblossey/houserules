# Batch 28 plan — the issue batch and the HR-098 repair

Spec: docs/specs/2026-10-01-batch-28-issue-batch.md (approved
2026-10-01, section 6). Branch: batch-28. Pipeline: standard
(implementer sonnet → task-reviewer opus per task; branch-reviewer
fable at the end; strictly sequential). Workspace:
`.superpowers/sdd/2026-10-01-batch-28/`.

Goal: six issue fixes and one repair land as six tasks; no task changes
the CLI surface or a schema constraint.

## Global constraints

- No subcommand, no flag, no schema constraint change
  (`houserules.1-0-surface-is-frozen`).
- Every kit edit lands in `template/`. After it: `update --dir .`
  through the tree binary, then
  `cargo run --bin payload-stamp-gate -- --write`
  (`houserules.template-is-the-source`, `houserules.payload-stamp-gate`).
- T2 to T5 change `implementer.md` or `task-reviewer.md`, the two
  templates the `process.evals-rerun` co-change check watches. Each of
  their task audits declares that rule with `--sanctioned`, citing spec
  section 1. The controller runs the one shared rerun after T6.
- Every new message string below is contract text: a test pins it, and
  the implementer sweeps README and `docs/` for a sentence that states
  the old behavior (`quality.user-facing-text-is-contract`).
- A test that mints a scratch directory uses `tempfile::TempDir`
  (`houserules.tests-clean-scratch-dirs`); a test that pins a printed
  path builds it by component (`houserules.path-pins-mirror-the-code`).

## Code-health scan (process.code-health-scan)

- `crates/houserules/src/report_claims.rs` (2889 lines): one file holds
  every check and its tests. Smell: size. Targeted fix: T4 and T5 each
  add their check as a child module with its own tests
  (`report_claims/natural_red.rs`, `report_claims/cited_text.rs`). The
  parent grows only by the `mod` lines, the two wiring calls in
  `check_report_claims`, and the Limits bullets. The file keeps its
  path, because the implementer template cites it.
- `crates/houserules/src/rules/audit.rs` (2878 lines): the
  `CheckType::ReportField` arm is about 70 lines inside one match, with
  two nested functions. Smell: long arm. Targeted fix: T3 first moves
  the arm into its own function (a pure move, tests unchanged), then
  changes the single-report branch.
- `crates/houserules/src/rules/stats.rs` (343 lines): three loops call
  `read_deliverable_value` the same way. Smell: repetition. Targeted
  fix: T2 adds one local reader that appends the remedy; the three
  loops call it. The shared reader in `deliverable.rs` keeps its text:
  `validate` and `audit` use it too.
- `crates/houserules/src/install.rs` (1334 lines): `seed` and `update`
  carry the same refusal block. Smell: duplication. Targeted fix: T2
  moves it into one helper that both call.
- `crates/houserules/src/rules/render.rs` (`repo_root_from_cwd`): the
  error is the first line of git's stderr. Smell: another tool's text
  as this binary's error. Targeted fix: T2, the named line.
- `crates/houserules/src/rules/check.rs` (2625 lines): `rules/` does not
  import `install` (measured: no `crate::install` use under `rules/`).
  T6 keeps that boundary: the command wrapper passes the kit-owned list
  into the lint.
- `template/.claude/agents/implementer.md` (67 lines): step 4 of the
  closing section is one dense paragraph. T5's two sentences land as
  their own numbered step before it, not inside it.
- `.github/workflows/dependabot-auto-merge.yml`: the header comment
  states a mechanism the live proof refuted. T1 rewrites it with the
  step; no stale rationale stays.

## Review focus

Inputs the spec implies and a person will meet. Each line has its test
in the owning task.

1. A report that lists no commit, or a RED whose redirect is not the
   plain `> <file> 2>&1` form (T4): the check stays silent.
2. A cited sentence whose only backticked token is the citation's own
   path or basename, and a reversed range such as `file.rs:20-10` (T5):
   no finding from the new rule.
3. An adopter who deleted a whole seeded topic file, and an id followed
   by punctuation such as `` `process.tdd`, `` (T6): the id is still
   recognized and checked.
4. A report with the `self_audit` key absent, not null (T3): the row
   still fails; only an explicit null is the draft state.
5. A target that is a linked git worktree, where `.git` is a file, and
   a target two levels below the top level (T2): the worktree root
   installs; the nested directory gets the new refusal.

## Tasks

### T1 — the auto-merge repair (HR-098)

Files: `.github/workflows/dependabot-auto-merge.yml`,
`.github/dependabot.yml`.

- The workflow: one new step before the merge step, under the same
  minor-or-patch condition, with `shell: bash`
  (`houserules.actions-default-shell-lacks-pipefail`). It reads the
  review decision with `gh pr view --json reviewDecision` and runs
  `gh pr review --approve "$PR_URL"` when the decision is not
  `APPROVED`. `GH_TOKEN` is `DEPENDABOT_AUTOMERGE_TOKEN`. No new
  `uses:` line. The header comment states: the token approves, then
  enables the merge; an enabled auto-merge waits for the approval
  ruleset.
- The Dependabot file: a `groups` block under the `github-actions`
  entry, group `codeql-action`, pattern `github/codeql-action*`.
- Docs to verify and record in `docs_verified`: the `groups` reference
  (the pattern against the `/init` and `/analyze` names); the
  fine-grained permission that submitting a review needs; the
  `gh pr view` and `gh pr review` flags.
- Paper walk in the report (`process.gates-cover-generated-commits`):
  the five author classes of spec section 3.
- No unit test exists for a workflow. Evidence: `mise run lint` and a
  YAML parse of both files. The live proof is acceptance step 5 of the
  spec.

### T2 — refusals name the way out (HR-145 parts 1 and 3, HR-147)

Files: `crates/houserules/src/rules/stats.rs`,
`crates/houserules/src/rules/render.rs`, `crates/houserules/src/root.rs`,
`crates/houserules/src/install.rs`,
`template/.claude/agents/task-reviewer.md`.

- `stats`: an unreadable deliverable's error ends with the remedy:
  `stats reads every task-*-audit*.json, task-*-review*.json, and
  task-*-report.json in the workspace as a deliverable; rename the file
  or repair it`. Exit 2 stays.
- No repository: `repo_root_from_cwd` returns
  `<cwd> is not inside a git repository; pass --dir <repository root>`
  when git reports no repository. Any other git failure keeps its own
  line. `resolve_root` prints it; exit 2 stays.
- Subdirectory target: one helper for `seed` and `update`. A target
  with `.git` proceeds. A target inside a repository refuses with
  `<target> is inside the git repository at <top level>, not its top
  level; houserules installs at the repository root`. A target outside
  any repository keeps `<target> is not a git repository (run git init
  first)`.
- Template: one sentence in the task-reviewer's Read-only section.
  Probe output goes to a `.txt` file, never to a `task-*` name.
- Tests, failing first: `stats_names_the_remedy_for_a_non_json_audit`
  (and the review and report twins);
  `resolve_root_names_the_dir_remedy_outside_a_repository`;
  `init_refuses_a_subdirectory_naming_the_top_level`;
  `init_refuses_a_nested_subdirectory_naming_the_top_level`;
  `init_keeps_the_git_init_advice_outside_any_repository`;
  `init_accepts_a_linked_worktree_root`; the `update` twin of the
  subdirectory test.
- Live run: a scratch repository with a subdirectory; a
  `git archive | tar` copy with and without `--dir .`; a workspace
  with a non-JSON `task-1-audit.json`.

### T3 — the audit passes the draft state (HR-145 part 2)

Files: `crates/houserules/src/rules/audit.rs`, `knowledge/process.json`,
`template/knowledge/process.json`,
`template/.claude/agents/implementer.md`.

- Step 1, a pure move: the `ReportField` arm becomes its own function.
  The suite passes unchanged.
- Step 2: in the single-report branch, when the field is `self_audit`
  and the report holds the key with an explicit null, the row is the
  same pass row a set field gets (`report field self_audit is set`
  today; the implementer keeps one text for both states and pins it).
  An absent key still fails. The `--workspace` branch is unchanged.
- Knowledge: in `process.deliverables-json`, both copies, the sentence
  "the check below fails until it does" becomes the new contract: the
  audit accepts a null `self_audit` in a single-report run; `validate`
  rejects it at `DONE`. Then `render` and `update --dir .`.
- Template: the `self_audit` bullet and step 1 of the closing section
  say that the first audit of the draft passes this row.
- Tests, failing first:
  `report_field_passes_a_null_self_audit_in_a_single_report_run`;
  `report_field_fails_an_absent_self_audit_key`;
  `report_field_fails_a_null_self_audit_in_a_workspace_run`;
  `report_field_still_fails_a_null_value_of_another_field`; and
  `validate_rejects_done_with_a_null_self_audit` when no test pins that
  guard yet.
- Live run: the reproduction of the triage, in a scratch install: a
  valid report with `self_audit: null`, then `audit --report` on it.

### T4 — the checker proves the `natural` label (HR-142, HR-143)

Files: `crates/houserules/src/report_claims.rs` (the `mod` line, the
wiring call, two Limits bullets), new
`crates/houserules/src/report_claims/natural_red.rs`,
`template/.claude/agents/implementer.md`.

- The check: for each `tdd[i]` with `mode` `natural`, take the capture
  path from `red.command` with the same redirect pattern
  `check_redirected_captures` uses. Compare the file's modification
  time with the committer time (`git log -1 --format=%ct`) of the
  newest commit among `commits[]` and `fix_rounds[].commits[]`. A newer
  capture is the finding
  `tdd[<i>]: mode "natural", but the RED capture "<path>" is newer than
  the newest listed commit <sha>; label the entry "reconstructed"`,
  with both times in the text.
- Silent cases: no redirect; a redirect in another form; a missing
  capture file; no listed commit; a sha that does not resolve.
- Template: the `tdd` bullet gains the capture sentence of the spec;
  the Report section gains the HR-143 sentence, verbatim from the spec.
- Tests, failing first, each with a scratch repository and file times
  set explicitly: `natural_red_older_than_the_commit_passes`;
  `natural_red_newer_than_the_newest_commit_fails`;
  `reconstructed_and_mutation_entries_are_ignored`;
  `a_red_without_a_redirect_is_silent`;
  `a_missing_capture_file_is_silent`; `a_report_without_commits_is_silent`;
  `an_append_redirect_is_silent`.
- Corpus run: the check over every retained deliverable
  (`.superpowers/sdd/**/task-*-report.json` and
  `tests/fixtures/batch14-workspace/`); the count goes in the report.
- Live run: `check-report-claims` on a real report of this batch.

### T5 — cited lines are compared at HEAD (HR-141)

Files: `crates/houserules/src/report_claims.rs` (the `mod` line, the
wiring call, Limits bullets), new
`crates/houserules/src/report_claims/cited_text.rs`,
`template/.claude/agents/implementer.md`.

- The rule: for each citation with a line or a range, in the four
  narrative fields, take the sentence that holds it. Collect its
  backticked fragments; drop the citation's own token, its path, and
  its basename. With at least one fragment left, one of them must occur
  in the cited line or range of the file at HEAD. Otherwise the finding
  is `<label>: cites "<token>", but none of <fragments> occurs there;
  the line reads "<text>"`.
- Silent cases: no fragment left; no line number; a reversed range; a
  file that does not resolve.
- Corpus gate, before the wiring call lands: run the rule over the
  retained corpus and record the count and each hit. Zero false
  positives: wire it in. Otherwise narrow the trigger, record the new
  count, and name what stays uncaught in the Limits list. The report
  carries both counts.
- Template: a new numbered step before the closing act, with the two
  sentences of the spec.
- Tests, failing first: `a_cited_line_holding_the_fragment_passes`;
  `a_drifted_line_fails_and_quotes_the_line`;
  `a_range_passes_when_any_line_holds_the_fragment`;
  `a_sentence_without_a_fragment_is_silent`;
  `the_citations_own_path_is_not_a_fragment`;
  `a_reversed_range_is_silent`; `a_fragment_in_the_next_sentence_does_not_count`.
- Live run: `check-report-claims` on a report with a drifted citation
  made for the run, and on a real report of this batch.

### T6 — kit-owned citations resolve; the empty record is defined (HR-146, HR-145 part 4)

Files: `crates/houserules/src/rules/check.rs`,
`crates/houserules/src/install.rs` (an accessor for the kit-owned
list), the `check-knowledge` command wrapper,
`template/.claude/skills/migrating-knowledge/SKILL.md`,
`template/.claude/agents/branch-reviewer.md`,
`template/.claude/skills/orchestrating/SKILL.md`.

- The lint: for each kit-owned file present under the root, find every
  backticked `<topic>.<slug>` token, trailing punctuation outside the
  backticks ignored. A token counts as an id when its prefix is a
  loaded topic or a topic the embedded payload seeds. Each id must
  resolve to a live entry. A miss is
  `<file>: cites "<id>", which is not in the knowledge base; restore
  the entry`.
- The kit-side test: every id the kit-owned files under `template/`
  cite resolves in `template/knowledge/`.
- Skill sentence, at the prune step of migrating-knowledge: before you
  delete a seeded entry, search the kit-owned files for its id.
- Empty record: the branch-reviewer sentence and the orchestrating
  sentence of spec part 4.
- Tests, failing first:
  `check_knowledge_passes_when_every_cited_id_resolves`;
  `check_knowledge_fails_a_kit_owned_citation_of_a_deleted_entry`;
  `a_token_without_a_topic_prefix_is_not_an_id`;
  `an_id_followed_by_punctuation_is_still_checked`;
  `a_deleted_seeded_topic_still_fails_its_cited_ids`;
  `an_absent_kit_owned_file_is_skipped`;
  `shipped_kit_owned_files_cite_only_shipped_ids`.
- Live run: a scratch install; delete `process.evals-rerun`; `render`;
  `check-knowledge` fails with the two files named; restore; it passes.

## After T6

1. The shared eval rerun: every scenario in `.claude/evals/`, in a
   detached scratch worktree at the branch head, per the orchestrating
   skill; one run set appended to `.claude/evals/record.json`.
2. The branch-range audit with `--workspace`; the evals row must pass.
3. The branch review; its findings fixed; the retrospective proposals
   applied in one `docs(knowledge)` commit.
4. The finish per `.claude/skills/finishing-a-feature/SKILL.md`. The PR
   body carries the closing keywords for issues 35, 36, 37, 39, and 40
   and names the new lint for the release note. No attribution line.

## Gates per task

fmt, clippy -D warnings, cargo test --locked, tree-binary
check-knowledge/check-backlog/render --check, check-commit --from
merge-base, mise run lint. Branch end: finishing-a-feature, 8 checks,
ff-only merge, main push before branch deletion.
