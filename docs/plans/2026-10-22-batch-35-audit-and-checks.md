# Batch 35 plan: audit and its checks

> **For agentic workers:** dispatch through `.claude/agents/` (`implementer`, `task-reviewer`, `branch-reviewer`) per `.claude/skills/orchestrating/SKILL.md`. Steps use checkbox (`- [ ]`) syntax.

**Revision 3** folds the 19 findings of the second plan review (`plan-review-r1.json`, no Critical finding: the plan-review loop closes, D3); revision 2 folded the 17 of the first (`plan-review.json`).

**Goal:** a check can run an external tool through a runner; `audit` and `stats` gain the evidence that issue 68 asks for and the content checks of HR-175, HR-176, HR-138, and HR-152; workspaces leave git; the pin checks read Cargo and mise; the batch-34 follow-ups close.

**Architecture:** `rules/audit.rs` keeps the audit engine; each check type moves into its own module under `rules/runner/`, behind one row builder, and the new `runner` type joins them with its `sarif` and `exit-code` parsers. The marker gains `runners` and `workspaces`; the schemas gain the new check and row fields; `stats` reads overrides, sanctions, and reversal lines. The seeded CI workflow checks out the PR head on a pull request.

**Tech stack:** Rust (the `houserules` crate), `serde`/`serde_json`, regress 0.12.0, `url` 2.5.8 (a direct dependency from Task 3's fix round; already in `Cargo.lock` through `axoupdater`), cargo only. No new crate enters the lock file (spec 3.5).

**Spec:** `docs/specs/2026-10-21-batch-35-audit-and-checks.md` (revision 6, approved under design.md 201: the fifth design review, `spec-review-r4.json`, holds no Critical finding; rulings 199 to 204; calls C2 to C14). Each task reads the spec sections it names; a brief carries them by pointer (`process.brief-carries-the-spec`). Branch: `batch-35`. Workspace: `/home/jblossey/projects/houserules/.superpowers/sdd/2026-10-21-batch-35/` (absolute in every dispatch).

**Tier: standard** (design.md 172(2)): one task review, at most one re-review; a branch review on opus. **Critical tasks** (173): T2 to T7 and T9 change the CLI's commands, flags, or output (T9: the help output), a schema, or an install's data on disk. For each: the task's spec cases become tests first, committed with one recorded RED run at BASE, then made to pass; the evidence is that RED run and the whole suite green at HEAD; one task review on opus and never a re-review; the controller verifies each fix by re-running the reviewer's reproduction (`process.fix-round-verification-record`): a Critical or Important finding goes to a fresh implementer, a Minor one the controller fixes. **The RED run of a critical task** records every new test: each test of new behavior fails, and each test the brief names as a pin of unchanged behavior passes and is recorded as a pin, with its reason (for example: the marker keeps every key today). A test whose new behavior is an exact line asserts the line, so it fails at BASE even where BASE already exits with the same code. T1 is a refactor with no output change: standard, with `process.tdd`'s disclosed-mutation proof (Task 1 step 2). T8 changes texts: standard. **Eval run:** yes (T8 changes the implementer and task-reviewer templates and the `docs-edit` scenario, `process.evals-rerun`). **Plan review:** fresh reviewers until a round holds no Critical finding (decided under design.md 204, D3); round 1 (`plan-review.json`) held one, a plan error (P1), and round 2 (`plan-review-r1.json`) held none.

**Implementer models (`process.implementer-model-per-task`):** sonnet for every task. T8 was planned on haiku; the plan review showed that spec 11 fixes only part of its text word for word and the rest needs judgment (P15), so it fails the "spec fixes the exact output" test of a haiku task. The ledger row of each dispatch records the model and this reason.

**Order and parallelism (`process.parallel-on-disjoint-files`; at most three agents at once):**

| Wave | Tasks | Why |
|---|---|---|
| 1 | T1 | every check task builds on the runner port |
| 2 | T2 | the shapes and the marker keys every later task reads |
| 3 | T3 ∥ T6 | T3 needs T2's `runner` type and `Marker::runner`; T6 needs T2's `Marker::workspaces`; disjoint files |
| 4 | T5 ∥ T8 | T5 needs T3 (`rules/audit.rs`, `rules/runner/`); T8 needs T6's workspace wording; disjoint files |
| 5 | T4 ∥ T7 | T4 needs T2's row fields and T5's `sanctioned` field; T7 needs T5 (`template/packs/security-hygiene.json`, `packs.rs`); disjoint files |
| 6 | T9 | help texts and documents state every behavior |

`crates/houserules/src/main.rs` changes in T4 (the `--window` flag and the `Stats` help) and T9 (every other help text) only, never in two tasks of one wave. Files the controller owns at integration: `crates/houserules/payload.stamp`, `.houserules.json`, the root copies that `update --dir .` writes, `backlog/`, and `knowledge/`. Each task still commits its own stamp and root copies (its `check-commit` gate needs them); when the cherry-pick of the second task of a wave conflicts on them, the controller re-runs `cargo run --quiet --bin payload-stamp-gate -- --write` and `./target/debug/houserules update --dir .` on that commit and amends it.

Each parallel implementer works in its own worktree from the same BASE, writes scratch only under `<scratchpad>/task-<N>/`, and does not touch the files the controller regenerates. The controller integrates by cherry-pick, one task at a time, runs the gates after each, writes the old-to-new commit map to the ledger, and removes the task's worktree and branch after integration.

## Cost measurement (172(6), `process.batches-measure-cost`)

The ledger records each dispatch's start, close, tokens, and tool uses; `efficiency-windows.json` holds one UTC window per phase (spec, plan, each wave with its reviews and fix rounds, evals, live run, branch review), and `efficiency.json` comes from `python3 -I .superpowers/sdd/2026-10-10-batch-33/efficiency.py <workspace>` before the branch review. Design review: five rounds (spec-review.json to spec-review-r4.json).

## Global constraints

- Use cargo for every package operation. No new crate enters `Cargo.lock` (spec 3.5: own SARIF types; `url` 2.5.8, already locked, becomes a direct dependency in Task 3's fix round); a task that needs another stops and says why (`security-hygiene.exact-pins`, `security-hygiene.dependency-vetting`).
- Every user-facing line is the spec's exact text; a test pins each line byte for byte (`quality.user-facing-text-is-contract`).
- A task that adds or tightens a check (T2, T3, T5, T6, T7) walks spec 13's rows for its check and states each accepted red in its report (`process.gates-cover-generated-commits`).
- A task that changes a pinned golden regenerates only that golden: run `cargo run --quiet --bin gen-goldens`, keep the owned golden's diff, and `git checkout` every other file it rewrote; the report shows that the owned golden's diff is only the spec's change.
- Commits: Conventional Commits, header and body lines at most 100 characters, no co-author or tool-attribution line, a `Work-Item:` trailer for each item the commit delivers; run `./target/debug/houserules check-commit --from origin/main` after every commit (the hook skips it).
- Use the tree binary (`cargo build`, then `./target/debug/houserules` with `HOUSERULES_SKIP_SELF_UPDATE=1`) for every gate and for `update` and `render` (`houserules.controller-gates-use-the-tree-binary`).
- `template/` is the source: edit there, then `./target/debug/houserules update --dir .` and `cargo run --quiet --bin payload-stamp-gate -- --write` in the same commit (`houserules.template-is-the-source`).
- A kit pack's content change moves its version by class, once per change, through `KIT_PACK_VERSIONS` and its checksum test (`packs.rs`); never a hand-typed hash.
- Copy every pattern, glob list, and exact line from its named source file (`pin-patterns-v4-pack.json`; batch 34's `branch-review-r1.json`; the spec); never retype one.
- Windows: the fake tool runs through `sh` on every system (spec 3.6; on Windows `Command::new("bash")` finds the WSL launcher); a test that compares a printed path builds it component by component (`houserules.path-pins-mirror-the-code`); a test that reads back a file git checked out commits a `.gitattributes` of `* -text` first. A test that needs a symlink (HR-224) or runs a `.sh` program directly (review focus 3) is `#[cfg(unix)]`, with a comment naming the platform constraint (`houserules.platform-gated-tests`); its Windows half, where one exists, runs the same input through `sh`. Before closing, run the suite once with `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=true cargo test --locked --no-fail-fast` and once plainly.
- Read houserules output through files: write it to the workspace and read the file.
- Put `CARGO_TARGET_DIR` of any scratch clone under `~/.cache` (`/tmp` is a 7.6 GB RAM disk; `houserules.clones-need-their-own-cargo-target`).
- Task gates: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo clippy --target x86_64-pc-windows-gnu --all-targets --locked -- -D warnings`, `cargo deny check`, shellcheck, `check-knowledge`, `check-backlog`, `render --check`, `residue-gate`, `vacuous-exception-gate`, `payload-stamp-gate`, `dist-generate-check`, `find-shell-tool-refs`, `check-commit --from <merge base>`, and CI's PR audit `mise run audit -- --base <merge base> --head HEAD` (`houserules.task-gates-mirror-ci`).
- ASD-STE100 style for docs, comments, commit messages, and reports; comments state current constraints only (`writing-style.code-comments`).

## Corpus runs (`houserules.plan-semantics-run-on-the-corpus`)

- The tests and sources that hold a token the batch changes (`install@2.1.0`, `.houserules/step/`, `pack@1.1.0`, `PACK_SCHEMA_MINOR`, `resolved`): `git grep` over the tracked tree, saved as `token-enumeration.txt`; each task's Files list takes its token's hits from that file (a hit about the step's output folder `.houserules/step/` itself, not the ignore line, stays).

- Pin patterns (spec 8.1): `pin-patterns-v4.tsv` over `cargo-pin-cases/` (`pin-patterns-v4-regress.txt`), over the 373 registry manifests (`pin-patterns-v4-registry.txt`: 316 hits, the same lines as version 3), and the timings (`pin-patterns-v4-timing.txt`). The pack text is `pin-patterns-v4-pack.json`, proven to decode to the TSV rows; its tool-pins `files` list is the 13 mise globs of `security-hygiene.dependency-vetting`'s `if` list, in order (`tool-pins-files-proof.py`, output `tool-pins-files-proof.txt`).
- `stats --window` order and the `Reversal:` form (spec 4, 9 (8)): over the 31 workspaces of `.superpowers/sdd/` (`corpus-plan-runs.txt`): every folder name has a `YYYY-MM-DD-` prefix; same-date folders sort by name (`2026-10-10-batch-32` before `-batch-33`); at the run, the spec 9 (8) form counted 9 lines in batch 34 and 17 in batch 35 (this ledger grows), 0 elsewhere; the other lines that hold `Reversal:` hold it inside a sentence and must not count.
- The template at each release tag holds no file the new pin checks or the widened durable-sha scope read, except an empty `.claude/evals/record.json` (`spec-review-r3-probe-template-tags.txt`).

## Review focus

Five inputs the spec implies and no case names, each pinned by a test in the owning task:

1. A tool that prints valid SARIF with no `results` key or an empty `runs` array (a clean run of some tools): the row passes with the spec's pass evidence `no findings in <n> calls` (T3 test `sarif_without_results_passes`).
2. A SARIF `artifactLocation.uri` that is percent-encoded (`src/a%20b.rs`) or a `file://` URI under the install root: the finding's `path` is decoded and root-relative with `/` separators (T3 test `sarif_uri_is_decoded_and_made_root_relative`).
3. A `runners` command whose program is a relative path with a space in a folder name (`tools/my lint/run.sh`): it resolves against the install root and runs with no shell (T3 test `runner_program_with_a_space_resolves_against_the_install_root`).
4. A `.gitignore` whose `.houserules/step/` line ends in CRLF or trailing spaces: `update` replaces it with `.houserules/` and keeps the file's line ending (T6 test `update_replaces_a_crlf_step_line`).
5. `stats --window 1` over two folders of one date whose names differ only in the batch number: the spec's order (name order) decides, and a folder with no date prefix sorts first (T4 test `window_orders_same_date_folders_by_name`).

## Code-health scan (`process.code-health-scan`)

Measured at plan revision 1 of batch 35 (`codehealth-measure.txt`): `rules/audit.rs` 3726 lines (1482 of code, 2244 of tests from line 1483), `rules/stats.rs` 1674, `rules/check.rs` 3240, `rules/proposals.rs` 1363, `rules/trailers.rs` 1022, `rules/check_shape.rs` 221, `report_claims/doc.rs` 419, `install/mod.rs` 2055, `install/marker.rs` 1609, `install/ci_workflow.rs` 638, `packs.rs` 1523; read too: the four schemas, the migrating-knowledge skill, the seeded pack entries.

- `rules/audit.rs` mixes four responsibilities: git plumbing (`run_git` to `removed_lines`, lines 91 to 408), glob and regex helpers (`glob_list`, `match_any`, `filter_matching`, `compile_check_regex`, 409 to 451), the six check types (`Violation`, `CommitsCheck`, `CoChangeCheck`, the report-field helpers, `AuditContext`, `run_report_field_check`, and the 136-line `run_check` match, 452 to 1004), and the audit engine. A new type would grow the match further (divergent change). T1 moves the git plumbing into `rules/git.rs`, the glob and regex helpers into `rules/runner/mod.rs`, and each check type into its own module under `rules/runner/` behind one row builder; the engine stays.
- `run_check` builds rows through closures (`row`, `pass`, `violate`) that each arm captures; T1 replaces them with one `RowBuilder` type.
- `run_report_field_check` declares two nested functions inside a branch (`report_files_changed`, `matching_file`); T5 lifts them to module level when it adds `non_empty` and `covers_ids`.
- `install/mod.rs` hard-codes one ignore line in `seed_gitignore` and its three printed lines; T6 replaces it with the `.houserules/` line and the replacement step of spec 7, in the same function.
- `install/marker.rs` exposes one accessor pair per key; T2 adds the `runners` and `workspaces` accessors in the same form, no generic key layer (speculative generality).
- `rules/stats.rs` and `rules/proposals.rs`: no smell blocks the change; T4 adds fields to `Usage` and `RuleRow` and the window filter in `resolve_sources`.
- `packs.rs`: no smell; T5 and T7 move versions through the existing pinning test.
- Out of scope, already tracked: HR-124 and HR-205 (`report_claims.rs`, batch 36), HR-164 (`stats.rs`, 2.x), HR-206 (`pack/`), HR-225 (the JSON parser, 2.x), HR-217 (the GitHub run of the seeded workflow, batch 38; spec 15).

---

### Task 1: the runner port and the git module (standard, refactor)

Spec: 3.4; case 3. Code-health items 1 and 2.

**Files:** create `crates/houserules/src/rules/runner/{mod.rs,grep_absent.rs,commits.rs,co_change.rs,diff_append_only.rs,report_field.rs,durable_sha.rs}` and `crates/houserules/src/rules/git.rs`; modify `rules/audit.rs`, `rules/mod.rs`, `rules/check_commit.rs`, `rules/durable_sha.rs` (its `use` of `git_diff`, `range`), `rules/trailers.rs` (its `use` of `Commit`), and every other `use` of a moved item; the unit tests of `audit.rs` move with the code they test; create `crates/houserules/tests/runner_pins.rs` and `crates/houserules/tests/fixtures/runner-pins/`.

**Interfaces (produces):**
- `rules::runner::RowBuilder<'a> { entry: &'a Entry, level: CheckLevel }` with `pass(&self, evidence: String) -> Value`, `violate(&self, evidence: String) -> Value` (`fail` or `warn` by level), `row(&self, result: &str, evidence: String) -> Value`, and `with(&self, row: Value, key: &str, value: Value) -> Value` (adds `findings` or `sanctioned` in later tasks).
- `rules::runner::run(entry: &Entry, check: &CheckDef, ctx: &AuditContext) -> Result<Value, String>`: one match on `CheckType`, one arm per module, each module exposing `run(check: &CheckDef, ctx: &AuditContext, rows: &RowBuilder) -> Result<Value, String>`.
- `rules::runner::AuditContext` (`pub(super)`): the same fields and cache methods; its methods call `rules::git`. `Violation` moves to `runner/commits.rs`; `CommitsCheck` and `CoChangeCheck` keep their names and signatures for `check_commit.rs`.
- `rules::git::{run_git, rev, range, git_diff, changed_files, tree_files, show_file, Commit, commits_in, changed_files_for, paths_changed_against_any_parent, removed_lines}` keep their signatures; T3 and T5 add their git helpers here.
- `rules::runner::{glob_list, match_any, filter_matching, compile_check_regex}` keep their signatures.

- [ ] **Step 1: Write the pins** in `tests/runner_pins.rs` (spec case 3): a test-built scratch repository with fixed author, committer, and dates holds one entry of each type whose rows the existing `tests/goldens/audit` set does not pin (`diff-append-only`, `durable-sha`), and its `.houserules.json` names a local `defaultBranch`; the durable-sha evidence names the branch tip's sha, so every scratch commit has fixed content, author, committer, and dates (`GIT_AUTHOR_DATE`, `GIT_COMMITTER_DATE`), which makes every sha reproducible; the test compares each row's JSON with a committed expected file under `crates/houserules/tests/fixtures/runner-pins/`. Add a test that pins the output of `check-commit` and of the commit-msg hook for a fixed range of that repository against committed expected bytes, each on its own (the hook runs `check-commit`, so comparing the two with each other could not bite).
- [ ] **Step 2: Prove the pins bite** (`process.tdd`, disclosed mutation): run them at BASE (pass), then make a scratch edit at BASE that changes one evidence string of each pinned type, run them, record each failure verbatim, and revert the edit. The report carries both runs.
- [ ] **Step 3: Move the code** with no behavior change: the four groups to their destinations, `RowBuilder` in place of the closures. No test changes its expectation.
- [ ] **Step 4: Run the suite and every task gate.** All goldens, the new pins, and the `check-commit` pin pass unchanged.
- [ ] **Step 5: Commit** `refactor(audit): run each check type through its own runner module` with `Work-Item: HR-183`.

### Task 2: the check shapes, the schemas, and the marker keys (critical)

Spec: 3.1, 3.3 (the `check-knowledge` lines of `runners`), 7 (the `workspaces` value line), 10; cases 2 (the malformed-entry lines in `check-knowledge`), 10 (the restamp line and a second `update` that changes nothing), 14 (the schemas and the override; the kit packs at 1.2.0 are T5's).

**Files:** modify `rules/check_shape.rs` (`CheckType::Runner`; fields `tool: Option<String>`, `non_empty: Option<bool>`, `covers_ids: Option<bool>`), `rules/check.rs` (`check_fields("runner") => ["tool"]`; `non_empty` and `covers_ids` read only by `report-field`, as `field` and `per_commit` are read by one type each today: `check_shape` checks only the fields a type requires and refuses no other; the `runners` and `workspaces` lines), `rules/runner/mod.rs` (the `Runner` arm returns `Err("the runner check type is not built yet")` until T3 replaces it), `template/knowledge/schema.json` and its root copy, `crates/houserules/schemas/pack-1.json` (`$defs.check`, minor 2), `packs.rs` (`PACK_SCHEMA_MINOR` 2 and its doc; the test `the_embedded_pack_schema_names_minor_1_and_holds_the_check_of_the_knowledge_schema` takes minor 2 and its name says `minor_2`), `crates/houserules/schemas/overrides-1.json`, `template/.claude/schemas/deliverables.json` and its root copy (`auditRow`: `sanctioned`, `findings`, `override`), `install/marker.rs` (`INSTALL_SCHEMA` `houserules/install@2.2.0`, the accessors, the restamp step), `install/mod.rs` (the restamp step's place in `update`; its module doc's `houserules/install@2.1.0`), `install/pack_moves.rs` and `pack/opt_in.rs` where they name `install@2.1.0`; the existing tests that pin `install@2.1.0` (`token-enumeration.txt`): `crates/houserules/tests/{conversion.rs,dogfood.rs,init_install.rs,tracker_json.rs}`; new tests `crates/houserules/tests/{check_shapes_35.rs,marker_35.rs}`.

**Interfaces (produces):**
- `Marker::runner_lines(&self) -> Vec<String>`: one line per malformed `runners` entry (spec 3.3), in key order, plus the `runners must be an object` line; `check-knowledge` prints them all.
- `Marker::runner(&self, tool: &str) -> Result<Option<RunnerDef>, String>`: reads only that entry (`Ok(None)` when the key or the entry is absent; `Err` with the entry's first malformed-entry line); `audit` calls it only for a triggered runner check. `RunnerDef { command: Vec<String>, parser: Parser, install: String }`; `Parser::{Sarif, ExitCode}`.
- `Marker::workspaces(&self) -> Result<Option<String>, String>` (`Ok(None)` when the key is not set; the `Err` is the malformed-value line of spec 7). `DEFAULT_WORKSPACES: &str = ".houserules/work"`.
- `update` prints `wrote .houserules.json (houserules/install@2.2.0)` once, from a step that tests its own state (5.148).

- [ ] **Step 1: Write the failing tests:** one per malformed-entry line and the `runners must be an object` line, several malformed entries giving one line each; the malformed `workspaces` line (absolute, `..`, empty, not a string), byte for byte; `runner_check_needs_tool`; `check_knowledge_names_an_undefined_runner_tool` (spec 3.3's first line); case 14's schema half (a pack copy at `houserules/pack@1.2.0` with each new field, a `runner` check included, validates; an override at `houserules/overrides@1.1.0` with each new field validates); `review_row_with_override_validates`, `audit_row_with_findings_and_sanctioned_validates`; `update_restamps_2_1_0_to_2_2_0_once` (the line, then a second `update` that prints no marker line); the existing `install@2.1.0` pins move to 2.2.0. Pin of unchanged behavior, named in the RED record: `init_and_update_keep_runners_and_workspaces` (the marker keeps every key today).
- [ ] **Step 2: Record the RED at BASE** (`cargo test --locked --test check_shapes_35 --test marker_35`, and the four changed test files).
- [ ] **Step 3: Implement the shapes, the schemas, the accessors, and the restamp.**
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(audit): ...` then `feat(audit): ...` with `Work-Item: HR-183`, `HR-188`, `HR-175`, `HR-138`, `HR-213`.

### Task 3: the runner check (critical)

Spec: 3.1 to 3.6, C13; cases 1, 2 (the `audit` half and the CI shape). Review focus 1 to 3.

**Files:** create `crates/houserules/src/rules/runner/{external.rs,sarif.rs}`, `tests/fixtures/runner/fake-sarif.sh` (mode 755, `* -text` in `tests/fixtures/runner/.gitattributes`); modify `rules/runner/mod.rs` (the `Runner` arm), `rules/git.rs` (the tree test's helpers: the checked-out commit, `git diff --quiet <head> -- <paths>`), `rules/audit.rs` (exit 2 with no rows on a setup error); tests `crates/houserules/tests/runner_check.rs`.

**Interfaces:**
- Consumes: T1's `RowBuilder`, `AuditContext`, `rules::git`; T2's `Marker::runner`, `RunnerDef`, `Parser`.
- Produces: `rules::runner::external::SetupError(String)` (the exact line; `audit` prints it and exits 2); `rules::runner::sarif::{Log, findings(log: &Log, root: &Path) -> Vec<Finding>}`; `Finding { path: Option<String>, line: Option<u64>, rule: String, message: String }`, serialized as the row's `findings`.

- [ ] **Step 1: Write the failing tests:** every sentence of spec case 1 and of case 2's `audit` half as its own test (`runner_fails_on_findings_with_path_line_rule_message`, `runner_passes_on_a_clean_run`, `untriggered_runner_starts_no_process`, `two_hundred_fifty_files_make_two_calls`, `result_with_no_level_and_no_rule_counts_as_warning`, `semgrep_shape_note_default_does_not_count`, `semgrep_shape_error_default_counts`, `override_to_none_drops_results`, `kind_pass_does_not_count`, `result_with_no_location_has_null_path`, `sarif_path_outside_the_root_is_kept_as_given`, `exit_code_tool_exiting_3_fails_with_its_stderr_line`, `exit_code_tool_with_empty_stderr_fails_with_exited_n`, `undefined_tool_exits_2_with_its_line`, `malformed_runner_entry_exits_2_with_its_line`, `missing_program_exits_2_with_the_install_line`, `head_not_checked_out_exits_2_with_the_tree_line`, `changed_triggering_file_exits_2_with_the_tree_line`, one test per SARIF setup-error line of spec 3.3, `sarif_pass_evidence_counts_calls`, `exit_code_pass_evidence_counts_calls`, `result_without_rule_id_takes_the_indexed_rule_or_the_token`, `note_result_is_not_in_findings`), the CI shape of case 2 (`ci_shape_pr_head_checkout_runs_a_tree_runner`: a scratch repository whose base moved after the PR branch, a checkout of the PR head as `ref:` gives it, then `audit --base <base> --head <PR head>` with a `scope: "tree"` runner check gives rows), and the three review-focus tests. `fake-sarif.sh` prints the SARIF file its first argument names, exits with the status its second argument names, and appends one line per call to a marker file, so a test counts the calls.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Implement** the trigger, the batching (200 files per call), the tree test, the parsers, and the rows of spec 3.2; spawn with `std::process::Command` and argv entries only. Prove no `not built yet` arm survives (`grep -rn "not built yet" crates/houserules/src` is empty).
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(audit): ...` then `feat(audit): ...` with `Work-Item: HR-183`.

### Task 4: `stats`: overrides, cost, the window, sanctions, reversals (critical)

Spec: 4, 6 (the `stats` bullet), 9 (8), C2, 12 (`stats --help`); cases 4, 6 (the `stats` half), 15. Review focus 5.

**Files:** modify `rules/stats.rs` (`cmd_stats`, the window), `rules/cost.rs` (`Workspace` gains `reversals`; `AdherenceRow` gains `judged`, `override`, and `sanctioned`, so every counted row carries what the new keys read; the per-review cost reads the review file's bytes here, beside the model costs), `rules/proposals.rs`, `main.rs` (`Stats`: `--window <n>` and its help), `tests/goldens/stats/*.json` (regenerated per the global constraint), `crates/houserules/tests/stats_workspaces.rs` and `crates/houserules/tests/stats_models.rs` where they pin the changed keys; tests `crates/houserules/tests/stats_35.rs` and its fixture workspaces under `tests/fixtures/stats-35/`.

**Interfaces:**
- Consumes: T2's deliverables-schema fields (`override`, `sanctioned`); T5's `sanctioned` row field.
- Produces: `rules::stats::cmd_stats(dir: Option<PathBuf>, workspaces: Vec<PathBuf>, window: Option<usize>) -> ExitCode`; `Usage` gains `verdicts`, `overrides`, `cost: Option<u64>` (bytes; `None` for a rule with no judged rows), `sanctioned`; `RuleRow::override_rate(&self) -> Option<f64>`; the top-level key `sanctioned` (`id`, `workspace`, `task`, `ref`) and, per workspace, `reversals`.

- [ ] **Step 1: Write the failing tests:** spec case 4 sentence by sentence (`retire`'s `cost` is `null` for a deterministic rule); case 6's `stats` half (a review row that keeps `sanctioned` and a pre-35 audit row with the evidence suffix count under `sanctioned`, not `violations` or `fails`); case 15 (`Reversal:`, `- Reversal:`, `  * Reversal:` count; `the ledger has no Reversal: line` does not); `mechanize_needs_override_rate_at_most_0_2_or_null`; `window_zero_prints_its_line_and_exits_2` (`--window must be 1 or more`); `stats_help_names_the_window`; review focus 5. Pins of unchanged behavior, named in the RED record: the existing `stats` keys and proposals of a workspace with no override, sanction, or `Reversal:` line.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Implement.** The window filters the named workspaces in `resolve_sources` before any read. Cost per review is its file's bytes divided by its judged rows in integer division (rounded down); a rule's cost is the sum over the reviews that judged it.
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(stats): ...` then `feat(stats): ...` with `Work-Item: HR-188`, `HR-152`, `HR-219`.

### Task 5: report-field content, the `self_audit` comparison, sanctions, and reference counts (critical)

Spec: 5, 6, 8.5, C3, C12, 202; cases 5, 6 (the `audit` half), 9 (HR-221), 14 (the kit packs at 1.2.0).

**Files:** modify `rules/runner/report_field.rs` (`non_empty`, `covers_ids`, the `--workspace` evidence and modes), `rules/audit.rs` and `cmd_audit` (the comparison after the reference verdict; `--sanctioned` on a judged rule; the `sanctioned` field on every sanctioned row; `--ids` into the context), `rules/runner/mod.rs` (`AuditContext` gains `ids: &[String]`), `rules/git.rs` (resolve a recorded sha: `git rev-parse --verify <sha>^{commit}`), `rules/trailers.rs` (`Verdict` counts `checked`, `passed`, `skipped`, and its doc comment), `template/packs/process.json` (`process.knowledge-first`: `non_empty`, `covers_ids`; the bodies of `process.deliverables-json` and `process.knowledge-earns-its-place`; schema id `houserules/pack@1.2.0`; version 2.0.0), `template/packs/security-hygiene.json` (`security-hygiene.verify-current-docs`: `non_empty`; schema id `@1.2.0`; version 4.0.0), `packs.rs` (`KIT_PACK_VERSIONS` rows), `tests/goldens/audit/*` (the `references` key, regenerated per the global constraint), `install/pack_moves.rs`, `pack/opt_in.rs`, and `crates/houserules/tests/opt_in.rs` where they name `pack@1.1.0`; `crates/houserules/tests/{trailers.rs,every_commit_entry.rs}` (`every_commit_entry.rs` pins `pack@1.1.0`; `token-enumeration.txt`); tests `crates/houserules/tests/report_field_35.rs`.

**Interfaces:**
- Consumes: T1's `RowBuilder::with`; T2's shapes; T3's `rules/audit.rs` state.
- Produces: the row field `sanctioned: "<ref>"`; the `references` object `{checked, passed, skipped, failures}`.

- [ ] **Step 1: Write the failing tests:** case 5 (the kit entries give `warn` rows and `audit` exits 0; the same inputs on a `fail`-level check give `fail` rows; `set in 1 of 2 reports`; `--workspace` `non_empty` per report and `covers_ids needs --report`); case 6's `audit` half (a matching copy with `reference_fail` and a `--sanctioned` row passes; recorded short shas lengthened by a repack still compare; `self_audit differs at <path>`; the not-compared evidence on another base; the comparing row and its summary counts left out; a draft passes); `sanctioned_judged_rule_is_not_stale`; HR-221 (one passing and one failing trailer give `checked` 2, `passed` 1); case 14's pack half (the kit's `process` and `security-hygiene` packs validate at 1.2.0); the pack-version pins. Pins of unchanged behavior, named in the RED record: a draft `self_audit: null` passes; a `--report` row of a set field passes; a deterministic sanctioned fail keeps its evidence suffix and exit.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Implement.** Lift the two nested functions of `run_report_field_check` to module level first (code-health item 3). The comparison runs in `cmd_audit` after the reference verdict (spec 6), then sets the comparing row and the summary counts. `process.knowledge-earns-its-place` names `stats --window 5` (spec 12).
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(audit): ...` then `feat(audit): ...` with `Work-Item: HR-175`, `HR-176`, `HR-138`, `HR-221`.

### Task 6: workspaces, the ignore line, `update` and `init`, the CI checkout, and report-claims paths (critical)

Spec: 7, 8.2, 8.4, 8.6, 3.2 (the CI bullet), C8, C14, 203; cases 7, 9 (HR-216, HR-220, HR-222, HR-224), 10 (the ignore-line and workflow lines). Review focus 4. The CI shape of case 2 is T3's: it needs the runner check.

**Files:** modify `install/mod.rs` (`seed_gitignore` and the replacement step; HR-216's refusal; HR-220; its module doc and the doc of `seed_gitignore`, which name the `.houserules/step/` line), `install/ci_workflow.rs` (`hand_step_for_kept`: the PR-head hand step for every tracker), `install/hand_steps.rs` (`ci_lacks_the_head_checkout(path: &str) -> String`, spec 3.2's exact text), `docs/upgrading-to-2.md` (a batch-35 section that holds the new hand step, so `hand_steps.rs`'s test `the_upgrade_guide_shows_every_hand_step` passes; T9 extends the section), `template/.github/workflows/knowledge.yml` (`ref:` on the checkout), `.github/workflows/ci.yml` (`ref:` on the checkout of the `checks` job), `rules/check.rs` (the ignored-`verify` line), `rules/check_marker_keys.rs` (Task 2's module of the marker-key lines: the unignored-default-location line, with the trailing-`/` test), `report_claims/doc.rs` (the `.houserules/step/` skip; the symlink verdict); the existing tests that pin the `.houserules/step/` line: `crates/houserules/tests/{install.rs,minimal_default.rs}`; new tests `crates/houserules/tests/{workspaces_35.rs,update_35.rs}`, and `crates/houserules/tests/check_report_claims.rs`.

**Interfaces:**
- Consumes: T2's `Marker::workspaces`, `DEFAULT_WORKSPACES`.
- Produces: `with: ref: ${{ github.event.pull_request.head.sha }}` on the `actions/checkout` step of the template and of `ci.yml`'s `checks` job (`ci-checkout-ref-docs.txt`); `ci_workflow::hand_step_for_kept(adapter: Option<Adapter>, path: &str, current: &[u8]) -> Vec<String>`, the tracker's step first, then the PR-head step.

- [ ] **Step 1: Write the failing tests:** case 7 sentence by sentence (the default-location line for an existing, unignored folder; no line for an absent folder or a set, tracked location; the fresh clone with `.houserules/` and no folder passes); the reproduction first (a reference-workflow install with a gitignored workspace and an entry that verifies a file in it fails `check-knowledge` at BASE with the missing-path line); case 9's HR-216, HR-220, HR-222, and HR-224 sentences; the three ignore lines of spec 7 byte for byte; `update_rewrites_an_unmodified_seeded_workflow_with_the_ref`; `update_keeps_a_modified_seeded_workflow_and_prints_its_hand_step` (a JSON-tracker install whose kept workflow lacks the `ref:` prints `kept .github/workflows/knowledge.yml (locally modified)` and then `hand step: .github/workflows/knowledge.yml lacks the PR-head checkout; add ref: ${{ github.event.pull_request.head.sha }} under with: of its actions/checkout step`; a GitHub-tracker install whose kept workflow lacks both its tracker lines and the `ref:` prints both steps, the tracker's first); `every_seeded_workflow_form_carries_the_ref` over `ci_workflow::template_for` for each adapter and the subdirectory form; actionlint passes on each form; review focus 4. The existing `.houserules/step/` ignore-line pins move to `.houserules/` (`token-enumeration.txt`). Pins of unchanged behavior, named in the RED record: a cited missing file under the workspaces location is named as today; `update` in a 1.x tree with `knowledge/` and no marker gives today's `--id-prefix` refusal. After the task: `cargo test --locked --test check_parity --test pack` passes unchanged (no `.houserules/work` folder exists there, so the default-location line stays silent).
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(install): ...` then `feat(install): ...` with `Work-Item: HR-213`, `HR-216`, `HR-220`, `HR-222`, `HR-224`, `HR-183`. The `ci.yml` change is `ci:` in its own commit (`houserules.commit-type-under-the-crate-path-decides-a-release`).

### Task 7: the pin checks and the durable-sha scope (critical)

Spec: 8.1, 8.3, C4, C11; cases 8, 9 (HR-218), 10 (the pin reds).

**Files:** modify `template/packs/security-hygiene.json` (`security-hygiene.exact-pins`: `flags`, `pattern`, body; the two new entries; version 5.0.0), `template/packs/knowledge-base.json` (the durable-sha `files`; version 3.0.0), `packs.rs` (`KIT_PACK_VERSIONS`, the sorted entry list and its doc), `crates/houserules/tests/exact_pins_check_pattern.rs`; create `crates/houserules/tests/fixtures/pin-cases/` (a committed copy of the workspace's `cargo-pin-cases/`), `crates/houserules/tests/pin_checks.rs`.

**Interfaces:** Consumes T5's `security-hygiene` 4.0.0 and its schema id. Produces the entry ids `security-hygiene.tool-pins-are-exact` and `security-hygiene.tool-versions-pins-are-exact`.

- [ ] **Step 1: Write the failing tests:** every case file through `audit` (each must-fail case at its own line, each must-pass case clean), the existing package.json cases of `exact_pins_check_pattern.rs` unchanged, the cost test of 8.1 (`exact-pins` on a generated 3000-line package.json and a 3000-line Cargo.toml of features finishes within 2 s in a debug build), `a_plan_citing_an_interim_sha_fails` (HR-218), the pack-version pins. Pins of unchanged behavior, named in the RED record: the existing package.json cases and the Cargo must-pass cases pass at BASE. Copy each check from `pin-patterns-v4-pack.json` (`pattern`, `flags`, `files`) byte for byte.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Implement.** The pack body of `security-hygiene.exact-pins` loses its sentence on bare Cargo ranges; the controller changes the same sentence in `knowledge/overrides.json` at integration.
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(packs): ...` then `feat(packs)!: ...` with `Work-Item: HR-214`, `HR-218`.

### Task 8: the agent templates, the skills, and the eval scenario (standard)

Spec: 4 (the orchestrating skill's override sentence), 6 (the template lines on `self_audit` and `sanctioned`), 7 (the readers), 11 (every sentence verbatim), 12 (the skills, `houserules-step`, the `--window 5` texts).

**Files:** `template/workflows/reference/.claude/agents/{implementer.md,task-reviewer.md,branch-reviewer.md,spec-reviewer.md}` (`branch-reviewer.md` names `stats --window 5`), `template/workflows/reference/.claude/skills/{orchestrating,planning,designing,finishing-a-feature}/SKILL.md` (the orchestrating skill names `stats --window 5`), `template/workflows/reference/.claude/evals/docs-edit.json`, `template/.claude/skills/houserules-step/SKILL.md` (its audit step: a runner check needs the change committed; its line on `init` and `.houserules/step/` names `.houserules/`), and their root copies through `update --dir .`; `crates/houserules/tests/workflow_texts.rs`.

- [ ] **Step 1: RED:** `workflow_texts.rs` gains one assertion per sentence of spec 11 and the workspace wording of spec 7; record that they fail at BASE.
- [ ] **Step 2: Write the texts.**
- [ ] **Step 3: Suite and gates.** The task's own audit fails `process.evals-rerun` by design until the eval run (controller step 4): book it with `--sanctioned process.evals-rerun=<controller step 4 of this plan>` in the self_audit run and say so in the report.
- [ ] **Step 4: Commit** `docs(workflow): ...` with `Work-Item: HR-215`, `HR-219`, `HR-176`, `HR-138`, `HR-213`, `HR-188`.

### Task 9: the help texts and the documents (critical: help output)

Spec: 12 (every `--help` text but `stats`, README, the upgrade guide, `template/docs/README.md`, the migrating-knowledge skill), 7 (the hand step), 3.3 (no network by default), 8.5.

**Files:** `crates/houserules/src/main.rs` (the help of `Audit`, `CheckKnowledge`, `CheckReportClaims`, `Init`, `Update`), `README.md` (the CLI table, a runner section, the `.houserules/` lines, the "Measure process cost" section), `docs/upgrading-to-2.md` (the batch-35 section: the seeded-workflow hand step, the workspaces move, the kept-schema merges, the pin reds), `template/docs/README.md`, `template/.claude/skills/migrating-knowledge/SKILL.md` (the runner section; its CI-gate section for other hosts checks out the PR head) and its root copy; tests `crates/houserules/tests/help_35.rs`.

- [ ] **Step 1: Write the failing tests:** one assertion per help text that spec 12 names, on the statement it must carry (runners, the exit 2 of a setup error, and the reference counts `checked`, `passed`, `skipped` of `audit`; the runner and marker lines of `check-knowledge`; the workspace-path and step-path lines of `check-report-claims`; the ignore line, the HR-216 refusal, the marker restamp, and the kept `runners` and `workspaces` keys of `update`; the `.houserules/` line and HR-220's kept tracker pattern of `init`). Commit them as `test(cli): ...` and record the RED at BASE.
- [ ] **Step 2: Write the help texts and the sections; check every command and flag against the tree binary's `--help`.** The hand-step test of batch 33 covers each new hand step's text.
- [ ] **Step 3: Suite and gates, then commit** the help texts as `feat(cli): ...` and the documents as `docs: ...`, with `Work-Item: HR-183`, `HR-213`, `HR-188`, `HR-221`.

## Controller steps

1. **Before T1:** confirm this plan's review closed with no Critical finding; record each round in the ledger. Close the plan window of `efficiency-windows.json`.
2. **After T2 integrates:** set `"workspaces": ".superpowers/sdd"` in this repository's `.houserules.json`; add the local entry `houserules.comments-name-no-review-history` (spec 9 (3); the pattern copied from batch 34's `branch-review-r1.json`, retrospective, `writing-style.code-comments`) to `knowledge/houserules.json`; add C7's sentence to the local `process.batches-measure-cost`; note on HR-204 that item 8 is delivered by that entry; `render`; commit `docs(knowledge): ...` with `Work-Item: HR-219`, `HR-204`.
3. **After T7 integrates:** drop the bare-Cargo-range sentence from the body of `security-hygiene.exact-pins` in `knowledge/overrides.json` (spec 8.1); `render`; commit.
4. **After T8:** the eval run (every scenario through the changed templates; `record.json` appended; spec case 11). In this repository, replace `houserules` in each brief with the tree binary's path (T8's orchestrating sentence).
5. **Live run:** spec case 12 by a dispatched implementer with a live-run brief, in a local scratch repository (no GitHub repository). This repository's `update` with the tree binary, then a second `update` that changes nothing; `stats --window 5`. Spec case 13 in a scratch worktree of this repository: a commit that adds `// fix round 2` to a Rust file, `audit --ids writing-style.code-comments,houserules.comments-name-no-review-history`, output saved as `live-case-13.json`; the worktree and its branch are deleted afterwards.
6. **Batch close:** `stats --window 5` over the workspaces to `stats.json`; `efficiency.json`; the branch review on opus; its fix wave; the knowledge commit of non-owner-gated proposals; `houserules set <id> status=done batch=35` for the 16 items; the model observations for the owner.
7. **Finish:** aggregate into one to five clean commits, each with a `Work-Item:` trailer for every item it delivers; push, PR, CI green, the ff-only merge from the CLI (design.md 198); no release (PR #83 stays held until batch 38).

## Spec acceptance cases → tasks

| Case | Task | Case | Task |
|---|---|---|---|
| 1 | T3 | 9 | T5 (HR-221), T6 (HR-216, HR-220, HR-222, HR-224), T7 (HR-218) |
| 2 | T2 (`check-knowledge` lines), T3 (`audit`), T6 (CI shape) | 10 | T2 (restamp), T6 (ignore line, workflow), T7 (pin reds) |
| 3 | T1 | 11 | controller step 4 |
| 4 | T4 | 12 | controller step 5 |
| 5 | T5 | 13 | controller steps 2 and 5 |
| 6 | T4, T5 | 14 | T2 (schemas, override), T5 (kit packs) |
| 7 | T6 | 15 | T4 |
| 8 | T7 | | |
