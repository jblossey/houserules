# Batch 36 plan: evidence and report claims

> **For agentic workers:** dispatch each task through the `implementer` template; review through `task-reviewer`. Steps use checkbox (`- [ ]`) syntax.

**Revision 2** folds the 18 findings of the plan review (`plan-review.json`, no Critical finding: the plan-review loop closes, as in batch 35) and the decisions D1 to D4 taken under design.md 216 (`delegated-decisions.md`).

**Revision note at batch close** (branch review, issue 3): five sentences below were made false during the batch and stay as written for the record. (1) After Task 3 the `Cargo.lock` diff adds nine packages, not the five listed: `bitflags`, `defmt`, `defmt-macros`, and `defmt-parser` come with them (delegated decision D5). (2) The skipped-report golden does not change: `git diff --stat ce0fc0fd HEAD -- tests/goldens` at the batch head prints nothing (Task 4, concern 1). (3) Task 4 regenerates no validate golden of the batch-14 fixtures; the sentence that says it does contradicts the one that says they stay. (4) A 2.x `update` never prints the `schema_kept` hand step; a kept schema gets the line `kept .claude/schemas/deliverables.json (locally modified)`, or with no baseline the hand step of spec 12 (D9). (5) Task 2's fix round also changed `main.rs` (its crate doc), beside the main.rs tasks T3, T5, T6, T7, and T11.

**Goal:** command output leaves task reports: `capture` records it with a header, a report cites each capture by path and hash, `report-facts` writes the facts, and `check-report-claims` checks the captures, excerpts, recorded citations, comma lists, and claim evidence; the batch-35 deferrals close.

**Architecture:** a new module `crate::capture` owns the capture file (run, header, read); `crate::shell` quotes for a POSIX shell; `crate::report_facts` fills a report. `report_claims.rs` splits into submodules (narrative, citations, runs, evidence) before its checks change. Every `git ls-files` listing moves into `rules::git_paths`. The deliverables schema's `run`, `self_review`, `citations`, and `reran` change in both copies; `validate` reads fix-round captures.

**Tech stack:** Rust (the `houserules` crate), `serde_json`, regress 0.12.0, `sha2` through `crate::baseline::hash`, and three direct dependencies by owner ruling: `jiff` (D9; `--no-default-features --features std`, which brings `jiff-core`, `jiff-static`, `portable-atomic`, and `portable-atomic-util` into `Cargo.lock`, D1), `shlex` (design.md 209; locked 2.0.1), `tempfile` (design.md 213; moves from the dev dependencies at 3.27.0). Cargo only.

**Spec:** `docs/specs/2026-10-22-batch-36-evidence-and-report-claims.md` (revision 4, approved under design.md 208: the third design review, `spec-review-r2.json`, holds no Critical finding; rulings 209 to 215). Each task reads the spec sections it names; a brief carries them by pointer (`process.brief-carries-the-spec`). Branch: `batch-36`. Workspace: `/home/jblossey/projects/houserules/.superpowers/sdd/2026-10-22-batch-36/` (absolute in every dispatch).

**Tier: standard** (design.md 172(2)): one task review, at most one re-review; a branch review on opus. **Critical tasks** (173): T3 to T9 and T11 change the CLI's commands, flags, or output, a schema, or an install's data on disk (T11: help output). For each: the task's spec cases become tests first, committed with one recorded RED run at BASE, then made to pass; the evidence is that RED run and the whole suite green at HEAD; one task review on opus and never a re-review; the controller verifies each fix by re-running the reviewer's reproduction (`process.fix-round-verification-record`): a Critical or Important finding goes to a fresh implementer, a Minor one the controller fixes. The RED run records every new test: each test of new behavior fails, and each pin of unchanged behavior passes and is recorded as a pin with its reason. T1 and T2 are refactors with no output change, except T1's one stated delta (spec 8.1): standard, with `process.tdd`'s disclosed-mutation proof. T10 changes tests and comments only: standard. **Eval run:** yes (T11 changes the implementer and task-reviewer templates and the `dependency-add` scenario, `process.evals-rerun`). **Plan review:** fresh reviewers until a round holds no Critical finding, as in batch 35.

**Implementer models (`process.implementer-model-per-task`):** sonnet for every task; no task's text is fixed word for word by the spec alone.

**The batch's own reports (spec 12):** every task report of batch 36 uses the old form (`command`, `output`, `exit`). The implementer and the task reviewer run on it, with the batch-BASE binary `~/.cache/hr-b36/base/houserules` (built at ce0fc0f by controller step 1): `validate --dir ~/.cache/hr-b36/base-tree` (a detached worktree at ce0fc0f, for the old schema), and `check-report-claims --dir <a checkout at the task head>` (the task's worktree; the reviewer uses the worktree while it exists, else a detached worktree at the task's HEAD), the tree the report's claims describe (spec 12 as amended at plan revision 2). `audit` runs with the branch binary of the task's tree. Each dispatch names the binary and both `--dir` values.

**Order and parallelism (`process.parallel-on-disjoint-files`; at most three agents at once):**

| Wave | Tasks | Why |
|---|---|---|
| 1 | T1 ∥ T3 ∥ T9 | T1 moves the listings; T3 adds `capture` and `shell`; T9 changes the seeded workflow; disjoint files |
| 2 | T2 ∥ T4 ∥ T10 | T2 splits `report_claims.rs` after T1's swap; T4 needs T3's `capture::read`; T10 touches only tests and comments; disjoint files |
| 3 | T5 ∥ T8 | T5 needs T3 and T4; T8 needs T3's `shell::quote`; disjoint files |
| 4 | T6 | the run checks; needs T2, T3, T4 |
| 5 | T7 | the claim checks; needs T6 (`report_claims.rs`, `main.rs`) |
| 6 | T11 | the texts, templates, packs, and help lines state every behavior |

A task may start before its wave when every task it needs has integrated and its files are disjoint from every running task's files: T2 needs only T1, so it starts while wave 1's T3 and T9 close their fix rounds (controller ruling at integration, ledger 2026-10-23).

`crates/houserules/src/main.rs` changes in T3 (the `Capture` subcommand), T5 (`ReportFacts`), T6 and T7 (the `check-report-claims` about), and T11 (the other help lines), never in two tasks of one wave. `Cargo.toml` and `Cargo.lock` change in T3 only. Files the controller owns at integration: `crates/houserules/payload.stamp`, `.houserules.json`, the root copies that `update --dir .` writes, `backlog/`, and `knowledge/`. Each task still commits its own stamp and root copies (its `check-commit` gate needs them); when the cherry-pick of a later task of a wave conflicts on them, the controller re-runs `cargo run --quiet --bin payload-stamp-gate -- --write` and `./target/debug/houserules update --dir .` on that commit and amends it.

Each parallel implementer works in its own worktree from the same BASE, writes scratch only under `<scratchpad>/task-<N>/`, and does not touch the files the controller regenerates. The controller integrates by cherry-pick, one task at a time, runs the gates after each, writes the old-to-new commit map to the ledger, and removes the task's worktree and branch after integration.

## Cost measurement (172(6), `process.batches-measure-cost`)

The ledger records each dispatch's start, close, tokens, and tool uses; `efficiency-windows.json` holds one UTC window per phase (spec, plan, each wave with its reviews and fix rounds, evals, live run, branch review); `efficiency.json` comes from `python3 -I .superpowers/sdd/2026-10-10-batch-33/efficiency.py <workspace>` before the branch review, after every window is closed. Design review: three rounds (`spec-review.json` to `spec-review-r2.json`).

## Global constraints

- Use cargo for every package operation. T3 adds `jiff` (`cargo add jiff@=<version> --no-default-features --features std`), `shlex`, and `tempfile` with `cargo add <crate>@=<version>`, after it vets each, and each of the four crates that `jiff` brings, and records them in `dependency_vetting` (`security-hygiene.dependency-vetting`, `security-hygiene.exact-pins`); `shlex` at the locked 2.0.1 and `tempfile` at 3.27.0 add no crate to `Cargo.lock`. No other direct dependency, and no other crate in `Cargo.lock`: after T3, `git diff <BASE> -- Cargo.lock | grep '^+name'` lists only `jiff`, `jiff-core`, `jiff-static`, `portable-atomic`, `portable-atomic-util`. A task that needs another stops and says why.
- Verify every API of `jiff`, `shlex`, `tempfile`, `std::io::pipe`, `str::Utf8Chunks`, and clap's trailing-argument form against current docs and record each in `docs_verified` (`security-hygiene.verify-current-docs`; the workspace's `library-research-*.txt` are a start, not the record).
- Every user-facing line is the spec's exact text; a test pins each line byte for byte (`quality.user-facing-text-is-contract`). Copy each line from the spec file; never retype it.
- A task that adds or tightens a check or a gate of spec 12's table (T1, T4, T6, T7, T8, T9) walks spec 12's rows for its check and states each accepted red in its report (`process.gates-cover-generated-commits`).
- A task that changes a pinned golden regenerates only that golden: run `cargo run --quiet --bin gen-goldens`, keep the owned golden's diff, and `git checkout` every other file it rewrote; the report shows that the owned golden's diff is only the spec's change.
- `tests/fixtures/batch14-workspace/` stays byte for byte (`houserules.corpus-batch14-fixtures-are-committed`; `residue-gate` pins it): its reports keep the old form; T4 regenerates the validate goldens that read them, and new-form fixtures go to new folders.
- Commits: Conventional Commits, header and body lines at most 100 characters, no co-author or tool-attribution line, a `Work-Item:` trailer for each item the commit delivers; run `./target/debug/houserules check-commit --from origin/main` after every commit (the hook skips it).
- Use the tree binary (`cargo build`, then `./target/debug/houserules` with `HOUSERULES_SKIP_SELF_UPDATE=1`) for every gate and for `update` and `render` (`houserules.controller-gates-use-the-tree-binary`), except `validate` and `check-report-claims` on the task's own report (the batch-BASE binary, above).
- `template/` is the source: edit there, then `./target/debug/houserules update --dir .` and `cargo run --quiet --bin payload-stamp-gate -- --write` in the same commit (`houserules.template-is-the-source`).
- A kit pack's content change moves its version by class, once per change, through `KIT_PACK_VERSIONS` and its checksum test (`packs.rs`); never a hand-typed hash.
- Windows: a test that compares a printed path builds it component by component (`houserules.path-pins-mirror-the-code`); a test that reads back a file git checked out commits a `.gitattributes` of `* -text` first; signals, non-UTF-8 arguments, and read-only directories are `#[cfg(unix)]`; a test that creates a file or directory with a non-UTF-8 name is `#[cfg(target_os = "linux")]` (APFS on macOS refuses such a name), or seeds the name through git's index (`git hash-object -w` and `git update-index --cacheinfo`, the pattern of `seed_undecodable_tracked_path` in `rules/check.rs`); each gate carries a comment naming the platform constraint (`houserules.platform-gated-tests`); a test that expects a printed path builds it through the same quoting as the code (`shlex::try_quote` in an integration test), since a Windows temp path holds a backslash that `shlex` quotes; a child process in a test runs through `sh` (on Windows `Command::new("bash")` finds the WSL launcher). Before closing, run the suite once with `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=true cargo test --locked --no-fail-fast` and once plainly; when the harness refuses the autocrlf run, name it in `concerns`, and the controller runs it at integration (spec 8.6 (1)).
- Read houserules output through files: write it to the workspace and read the file.
- Put `CARGO_TARGET_DIR` of any scratch clone under `~/.cache` (`/tmp` is a 7.6 GB RAM disk; `houserules.clones-need-their-own-cargo-target`).
- Task gates: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, `cargo clippy --target x86_64-pc-windows-gnu --all-targets --locked -- -D warnings`, `cargo deny check`, shellcheck, `check-knowledge`, `check-backlog`, `render --check`, `residue-gate`, `vacuous-exception-gate`, `payload-stamp-gate`, `dist-generate-check`, `find-shell-tool-refs`, `check-commit --from <merge base>`, and CI's PR audit `mise run audit -- --base <merge base> --head HEAD` (`houserules.task-gates-mirror-ci`).
- ASD-STE100 style for docs, comments, commit messages, and reports; comments state current constraints only (`writing-style.code-comments`; `houserules.comments-name-no-review-history` warns on history phrases).

## Corpus runs (`houserules.plan-semantics-run-on-the-corpus`)

- Report fixtures that the schema change breaks (spec 4): `token-enumeration.txt`: 26 tracked files hold a `task-report` kind. The validate goldens (`tests/goldens/validate/*`) and the five parity tests of `validate_stats_audit_parity.rs` (lines 625, 1041, 1141, 1181, 1237) run `validate` inside a worktree at the frozen commit 5f14727, so they read that commit's old schema (`plan-review-probe-frozen-validate.txt`): the batch-14 and `task-1-report` goldens and the five tests stay unchanged and are pins of unchanged behavior; the `skipped-report` golden changes because the fix-round invariant reads captures (T4). The new form is pinned against the tree schema in new tests (T4). `tests/fixtures/batch14-workspace/` stays byte for byte.
- The comma-list rule (spec 6.9) over the narrative fields: `corpus-comma-rule-r2.txt` (9 lists; all 8 one-word lists read in full; the spaced count list read as one citation). T7 copies the rule from spec 6.9.
- The commit-body count rule (spec 6.10): `corpus-commit-body-captures.txt` (0 findings of 18).
- The trigger words (spec 6.5): `corpus-reports.txt` and `spec-review-probe-closure-words.txt` (472 closure items, 168 pin items; the classified sample of 30 is C7's baseline).
- The `ls-files` callers (spec 8.1): `fact-ls-files-callers.txt` and `spec-review-r2-probe-ls-files.txt` (eight `"ls-files"` literals, all git arguments).
- The remedy lines (spec 8.3): `fact-remedy-lines.txt` and `spec-review-probe-remedy-paths.txt`; the `bd config set` pins: `spec-review-r1-probe-bd-config.txt`.
- The evidence-form residue (spec 7): `residue-evidence-form-classified.txt`.

## Review focus

Five inputs the spec implies and no case names, each pinned by a test in the owning task:

1. A command whose output is empty: the capture is the header line alone, with no LF after it and no output bytes; `read` gives an empty output, and an excerpt can match nothing (T3 test `an_empty_output_is_a_header_alone`; T6 test `an_excerpt_of_an_empty_capture_fails`).
2. A command that writes a lone CR, or output that ends in the middle of a multi-byte UTF-8 sequence: the CR stays; the cut sequence becomes one U+FFFD and counts 1 in `lossy` (T3 test `a_cut_utf8_sequence_is_one_replacement`).
3. A report whose `citations` names the same lines twice with two anchors: each entry is judged on its own; neither overwrites the other's hash (T5 test `two_citations_of_one_range_keep_their_own_hashes`).
4. A capture path with a space or a non-ASCII character in a folder name, cited relative to the install root: `report-facts` and `check-report-claims` resolve it, and `command` quotes the path when the argv held it (T5 test `a_capture_path_with_a_space_resolves`).
5. A `--base` that is HEAD itself (a task with no commit yet): `commits` and `files_changed` become empty arrays, and the command still exits 0 (T5 test `a_base_at_head_gives_empty_facts`).

## Code-health scan (`process.code-health-scan`)

Measured at plan revision 1 (`codehealth-measure.txt`): `report_claims.rs` 3111 lines (1882 of code, tests from 1883), `report_claims/natural_red.rs` 809 (tests from 215), `report_claims/doc.rs` 420, `rules/validate_deliverable.rs` 1056, `rules/check.rs` 3301, `rules/check_packs.rs` 1146, `rules/git_paths.rs` 184, `rules/ignored.rs` 554, `rules/render/agents_md.rs` 591, `install/location.rs` 567, `install/mod.rs` 2206, `install/ci_workflow.rs` 708, `install/hand_steps.rs` 266, `tracker/beads/check.rs` 265, `main.rs` 1606, `packs.rs` 1646; read too: the deliverables schema, the implementer and task-reviewer templates, the houserules-step skill and script.

- `report_claims.rs` holds eleven checks, their parsers, and 1228 lines of tests in one file (a large module with divergent change: this batch changes eight of the checks). T2 splits it, with no behavior change, into `report_claims/narrative.rs` (narrative collection, the sha, ratio, and ephemeral checks), `report_claims/citations.rs` (`find_citations`, the line, count, and sweep checks), and `report_claims/runs.rs` (run collection and the three checks that T6 retires); `report_claims.rs` keeps the orchestration, `ClaimsSource`, and `cmd_check_report_claims`. Tests move with the code they test.
- Duplicate code: the tracked-file listing exists twice (`check.rs` `git_ls_files`, `report_claims.rs` `list_tracked_files`) and the glob listing twice (`install/location.rs`, `rules/render/agents_md.rs`). T1 moves every listing into `rules::git_paths` (design.md 215).
- `natural_red.rs` reads file modification times and git-tracked status to approximate when a RED ran; the capture header records the end time, so T6 replaces that logic, and the tracked-file skip and its limits go.
- `validate_deliverable.rs` `parse_audit_summary_skipped` scans a string for the first `{`; T4 keeps the scan on the capture's output, and its limit stays documented.
- `install/ci_workflow.rs` builds each form by marker strings; T9 adds two markers and their hand steps in the same form (`hand_step_for_kept`), no new layer.
- `install/mod.rs`, `tracker/beads/check.rs`: no smell blocks the change; T8 routes two printed values through `crate::shell::quote`.
- Out of scope, already tracked: HR-124 and HR-205 (`report_claims.rs`; spec 14), HR-121, HR-164, HR-206, HR-225, HR-231, HR-233 (2.x or no planned file), HR-217 (batch 38).

---

### Task 1: every `ls-files` listing in `rules::git_paths` (standard, refactor and lint)

Spec: 8.1; case 6. Backlog: HR-204. Code-health item 2.

**Files:** modify `crates/houserules/src/rules/git_paths.rs` (the three listings), `rules/check.rs` (`tracked_tree_files` calls the moved listing), `rules/ignored.rs` (`submodule_paths` moves out), `install/location.rs` (`other_installs` calls the glob listing), `rules/render/agents_md.rs` (`git_listing` calls it), `rules/mod.rs` (the `pub(crate)` re-exports), `report_claims.rs` (`list_tracked_files` calls the tracked listing; the module doc's first paragraph); create `crates/houserules/tests/ls_files_home.rs`.

**Interfaces (produces):**
- `rules::git_paths::tracked_files(root: &Path) -> Option<(Vec<String>, Vec<String>)>`: today's `git_ls_files` (listed paths, undecodable entries rendered lossily), unchanged.
- `rules::git_paths::listed_by_name(dir: &Path, name: &str) -> Result<Vec<Vec<u8>>, ListingFailure>`: `git ls-files -z --cached --others --exclude-standard -- :(glob)**/<name>`, the NUL-separated entries as raw bytes; `ListingFailure::{NotRun(std::io::Error), Failed(String)}` (`Failed` holds git's first stderr line). Each caller keeps its own decoding and failure handling.
- `rules::git_paths::submodule_paths(root: &Path) -> Option<Vec<String>>`: moved as it is.
- All three `pub(crate)` through `rules/mod.rs`.

- [ ] **Step 1: Write the lint test** `ls_files_home.rs` (spec 8.1): it reads every `.rs` file under `crates/houserules/src` (derived by a directory walk, `quality.gates-derive-their-scope`) and fails when a file other than `rules/git_paths.rs` holds the literal `"ls-files"`, quotes included; the three gate binaries are exempt, each with its reason in the test. Run it at BASE: it fails, naming `rules/check.rs`, `rules/ignored.rs`, `install/location.rs`, `rules/render/agents_md.rs`, and `report_claims.rs`. Record that run.
- [ ] **Step 2: Write the delta test** (spec 8.1, case 6): a unit test in `report_claims.rs` over a scratch repository with a tracked file whose name is not UTF-8 (`#[cfg(unix)]`): `list_tracked_files` leaves the file out. It fails at BASE (the name is decoded lossily). Record that run.
- [ ] **Step 3: Move the listings** with no other behavior change; `report_claims.rs` maps `tracked_files` to its list by dropping the second vector, and its module doc says that it uses `rules::git_paths`. The existing tests of `other_installs`, `git_listing`, `submodule_paths`, and `tracked_tree_files` pass unchanged.
- [ ] **Step 4: Prove the lint bites** (`process.tdd`, disclosed mutation): add `.args(["ls-files"])` to a scratch copy of one other module, run the lint, record the failure, revert.
- [ ] **Step 5: Run the suite and every task gate; commit** `refactor(rules): list git paths in one module` with `Work-Item: HR-204`.

### Task 2: split `report_claims.rs` (standard, refactor)

Spec: 10 (the split). Backlog: HR-141. Code-health item 1. Needs T1.

**Files:** modify `crates/houserules/src/report_claims.rs`; create `report_claims/{narrative.rs,citations.rs,runs.rs}`; the module doc's limits move with the checks they describe.

**Interfaces (produces):** `report_claims::narrative::{collect_narrative, check_narrative_shas_resolve, check_self_audit_narrative, check_ephemeral_paths, check_self_audit_head_is_current}`; `report_claims::citations::{Citation, find_citations, NarrativeField, collect_narrative_fields, load_citation, check_citation_lines, check_narrative_number_claims, check_narrative_sweep_coverage, collect_files_changed}`; `report_claims::runs::{Run, collect_runs, check_redirected_captures, check_truncation_markers, check_paste_run_lint}`; each `pub(super)` with its current signature. The shared helpers (`resolve_against`, `read_utf8_lossy`, `line_count`, `collect_listed_commit_shas`, `resolve_commit`, `is_ancestor`, `newest_listed_commit`, `list_tracked_files`, `resolve_citation_path`, the word helpers) stay in `report_claims.rs` as `pub(super)`, so `natural_red.rs`'s `super::` paths stay valid; `redirect_capture_pattern`, `redirect_capture`, `quote_mask`, and `find_placeholder` move to `runs.rs` with their checks. `check_report_claims` imports the moved checks with `use` and calls them unqualified, in today's order, so `HELP_CHECKS` in `main.rs` stays as it is.

- [ ] **Step 1: Prove the pins bite** (disclosed mutation): at BASE, change one finding line of each of three checks (one per new module), run `cargo test --locked report_claims` and `--test check_report_claims`, record each failure, revert.
- [ ] **Step 2: Move the code and its tests**; no test changes its expectation; the about test `the_check_report_claims_about_names_every_check` passes with `main.rs` unchanged. `report_claims.rs`'s module doc keeps a Limits index that names each submodule whose doc holds the limits of its checks.
- [ ] **Step 3: Run the suite and every task gate; commit** `refactor(report-claims): split the checks into submodules` with `Work-Item: HR-141`.

### Task 3: `capture` and shell quoting (critical)

Spec: 3, 9; cases 1, 2; review focus 1, 2. Backlog: HR-171, HR-197.

**Files:** create `crates/houserules/src/capture.rs` (run, write, read), `crates/houserules/src/shell.rs`; modify `main.rs` (`mod capture; mod shell;`, the `Capture` subcommand and its help, which lists the header keys of spec 3), `crates/houserules/Cargo.toml` and `Cargo.lock` (`jiff`, `shlex`, `tempfile` to `[dependencies]`), `crates/houserules/tests/check_parity.rs` (`capture` in `NOT_ROOT_RESOLVING`), and the `main.rs` pins of the subcommand count; create `crates/houserules/tests/capture.rs`.

**Interfaces (produces):**
- `capture::SCHEMA: &str = "houserules/capture@1.0.0"`.
- `capture::Header { schema: String, argv: Vec<String>, cwd: String, start: String, end: String, exit: Option<i32>, signal: Option<i32>, head: Option<String>, dirty: Option<bool>, lossy: u64 }` (serde, fields in spec 3's order, `schema` first).
- `capture::Capture { header: Header, bytes: Vec<u8> }` with `output(&self) -> &[u8]` (the bytes after the first LF; empty when the file is the header alone) and `sha256(&self) -> String` (`crate::baseline::hash` of `bytes`).
- `capture::read(path: &Path) -> Result<Capture, ReadError>`; `ReadError::{Missing, Unreadable(String), NoCapture(String)}`; `Unreadable` holds the OS error of spec 5's and check 1's `could not be read` line (D2); the `NoCapture` reasons are spec 3's four texts verbatim.
- `capture::cmd_capture(out: PathBuf, force: bool, command: Vec<OsString>) -> ExitCode` for a setup error (exit 2); after the file is written it ends the process with `std::process::exit(code)`, the command's code or 128 plus the signal (spec 3, D3).
- `shell::quote(value: &str) -> String` (`shlex::try_quote`; a value that needs no quoting is returned as it is) and `shell::join(argv: &[String]) -> String` (`shlex::try_join`); an argument holds no NUL (the OS refuses one), and the doc says so where the code relies on it.

- [ ] **Step 1: Write the tests** of cases 1 and 2 and review focus 1 and 2 in `tests/capture.rs`, through the built binary and `sh`: the header keys in order (parse the first line as JSON and compare the key list), both streams in order, the exit mirror, the printed line, the signal case (`#[cfg(unix)]`), U+FFFD and `lossy`, `head: null` and `dirty: null` outside a work tree and before the first commit, `dirty: false` and `true`, stdin at end of file, the `env` form, every setup error's exact line with exit 2 and no command started (a marker file stays absent) and the existing file unchanged, `--force`; a Windows test (`#[cfg(windows)]`) of a command that exits 300 (`capture` exits 300, D3); the non-UTF-8 current directory case is `#[cfg(target_os = "linux")]`. The unit tests of `shell::join` (`["sh", "-c", "a b"]` gives `sh -c 'a b'`) and of `read`'s four reasons and `Unreadable` live in `src/shell.rs` and `src/capture.rs` under `#[cfg(test)]` (the crate has no library target); the RED commit adds both modules with `unimplemented!()` bodies, so those tests fail at BASE plus the stubs. Record one RED run: the binary tests at BASE (the subcommand does not exist) and the unit tests on the stubs.
- [ ] **Step 2: Vet and add the three dependencies and the four crates `jiff` brings** (global constraints), and verify their APIs; check the `Cargo.lock` diff names only the five new crates.
- [ ] **Step 3: Implement** spec 3 in the stated order: setup tests, HEAD and dirty, the temporary files, one `std::io::pipe` for both streams, `Utf8Chunks` for the lossy count, `jiff` with `{:.9}`, `persist_noclobber` or `persist`.
- [ ] **Step 4: Run the suite and every task gate; commit** `feat(capture): record a command's output under a header` with `Work-Item: HR-171` and `Work-Item: HR-197`.

### Task 4: the report form and `validate` (critical)

Spec: 4, 12 (the reds of `validate`); case 5; the upgrade part of case 10 for the deliverables schema. Backlog: HR-171, HR-057, HR-141, HR-204. Needs T3.

**Files:** modify `template/.claude/schemas/deliverables.json` and its root copy (through `update --dir .`), `crates/houserules/src/rules/validate_deliverable.rs` (`check_fix_round_audits` reads each fix-round capture through `crate::capture::read`, resolved against `root` with `Path::join`; the new line of spec 4), `crates/houserules/tests/deliverables_model.rs`, `tests/goldens/validate/skipped-report.json` (regenerated: the old-form fixture's fix-round `output` is no longer read, so its skipped-audit error leaves the golden, by design; the fixture stays old-form); the batch-14 and `task-1-report` validate goldens and the five frozen parity tests stay unchanged (pins); create `tests/fixtures/report-36/` (a valid new-form report with its captures, written by T3's binary) and `crates/houserules/tests/validate_36.rs`, which validates against a scratch root that holds the new schema: the new invariant on a fix-round audit capture with `"skipped": 2`, and the forms of case 5.

**Interfaces (produces):** the schema definitions `run`, `selfReviewItem`, `citation`, and `fixRound.findings[].reran` exactly as spec 4 states, with descriptions; the `validate` line `<file>.fix_rounds[<i>].tests[<j>].capture: audit summary skipped is <n>; re-run audit with --report`.

- [ ] **Step 1: Write the tests** of case 5 and the schema half of case 10 in `validate_36.rs`. The upgrade test starts from the batch-35 state: `init` a scratch install, write `git show ce0fc0f:template/.claude/schemas/deliverables.json` (with the install's `backlogId` pattern) over its copy, set the copy's baseline in `.houserules.json` to that content, run `update`, and assert the new schema; then repeat with a modified copy and assert the `schema_kept` hand step, and that a new-form report fails `validate` against the kept schema (a red of spec 12); an in-flight old-form report fails `validate` against the new schema (a red of spec 12). Add a before-and-after test: `stats` and `audit --workspace` over a copy of a batch-35 workspace fixture (`tests/fixtures/stats-35/`) print the same result with the BASE binary and the branch binary (case 10). Record one RED run at BASE, and the pins of the frozen goldens and parity tests.
- [ ] **Step 2: Change both schema copies and `validate`**; regenerate the three goldens per the global constraint and show that each diff is the spec's change.
- [ ] **Step 3: Run the suite and every task gate; commit** `feat(deliverables)!: cite runs by capture, record citations and evidence` with the four `Work-Item:` trailers.

### Task 5: `report-facts` and the core report step (critical)

Spec: 5, 7 (the core `houserules-step` skill's step 4 and the script); case 3, case 11a (the core path); review focus 3, 4, 5. Backlog: HR-158, HR-141. Needs T3, T4.

**Files:** create `crates/houserules/src/report_facts.rs`; modify `main.rs` (`mod report_facts;`, the `ReportFacts` subcommand and its help, and the subcommand-count pins), `crates/houserules/tests/check_parity.rs` (`report-facts` in `ROOT_RESOLVING_COMMANDS`), `rules/git_paths.rs` and `rules/mod.rs` (`InstallPrefix` and its `of` and `map` `pub(crate)`), `template/.claude/skills/houserules-step/SKILL.md` (step 4, spec 7's text verbatim), `template/tools/houserules-step.sh` (`after <base> <report>`: the audit with `--report`, then `report-facts`, then `validate`), and their root copies; `crates/houserules/tests/houserules_step.rs` (the script's new run); create `crates/houserules/tests/report_facts.rs`.

**Interfaces:**
- Consumes `capture::read`, `Capture::sha256`, `Header`, `shell::join`, `rules::git_paths::InstallPrefix` (made `pub(crate)` for `files_changed`), `crate::emit::emit`.
- Produces `report_facts::cmd_report_facts(dir: Option<PathBuf>, report: PathBuf, base: String, fix_base: Option<String>, audit: Option<PathBuf>) -> ExitCode` (it resolves the install root before it reads `<REPORT>`, spec 5), and `report_facts::judge_citation(root: &Path, path: &str, line: u64, end: Option<u64>, anchor: &str) -> Result<CitedLines, CitationFault>`, with `CitedLines { bytes: Vec<u8>, sha256: String }`: it reads `git show HEAD:./<path>` at the install root after the blob and `..` tests, cuts the cited bytes per spec 4's line rule, and tests the anchor (in the range, once in the file). `CitationFault::{NotTracked, EndBeforeLine, TooFewLines { lines, needed }, AnchorNotInRange, AnchorRepeated { count }}` map one to one onto spec 5's and check 4's shared lines; each `Display` takes the citation's index and copies the spec's line. T7 consumes `judge_citation` and adds only check 4's hash-mismatch line.

- [ ] **Step 1: Write the tests** of case 3 and review focus 3 to 5 in `report_facts.rs` (a scratch repository with fixed dates; captures written by T3's binary; a subdirectory install with a top-level file of the same name; a CRLF file; the anchor `let` twice), and the core path of case 11a in `houserules_step.rs`. Record one RED run at BASE.
- [ ] **Step 2: Implement** spec 5: the fill order (commits, files_changed, fix-round commits, self_audit, runs, citations), every finding checked before any write, the write through `NamedTempFile::persist`.
- [ ] **Step 3: Change the skill's step 4 and the script**; shellcheck the script.
- [ ] **Step 4: Run the suite and every task gate; commit** `feat(report-facts): fill a task report's facts from git and captures` with `Work-Item: HR-158` and `Work-Item: HR-141`.

### Task 6: the run checks of `check-report-claims` (critical)

Spec: 6 checks 1 to 3 and the retirement paragraph (design.md 211, 212); the run half of case 4. Backlog: HR-171, HR-197, HR-204. Needs T2, T3, T4.

**Files:** modify `report_claims.rs` (`check_report_claims`'s calls), `report_claims/runs.rs` (run collection reads `capture`, `sha256`, `command`, `exit`, `excerpt`; checks 1 and 2; the three retired checks and their limits leave), `report_claims/natural_red.rs` (check 3 on the header's `end` and `exit`; the modification-time and tracked-file logic leaves), `main.rs` (the `check-report-claims` about names the new checks and drops the retired ones), `crates/houserules/tests/check_report_claims.rs` (its fixtures move to the new form; the retired checks' tests leave).

**Interfaces:** consumes `capture::read`, `Capture::{output, sha256}`, `Header`, `shell::join`, and `jiff::Timestamp` (parse the header's `end`; `as_second` for the whole-second comparison of check 3). Produces `runs::RunRef { label: String, capture: Option<String>, sha256: Option<String>, command: Option<String>, exit: Option<Option<i32>>, excerpt: Option<String> }` and `runs::collect_runs(report: &Value) -> Vec<RunRef>` (checks 1 to 3 read it).

- [ ] **Step 1: Write the tests** of check 1's seven lines (the old-form run included, also on a report written under a kept old schema, a red of spec 12), check 2, check 3's two lines (and a `mutation` RED that exited 0 passes), and review focus 1's excerpt case. Record one RED run at BASE.
- [ ] **Step 2: Implement**; remove the retired checks with their tests and limits bullets; the about test passes with the new pairing.
- [ ] **Step 3: Run the suite and every task gate; commit** `feat(report-claims)!: check runs against their captures` with the three trailers.

### Task 7: the claim checks of `check-report-claims` (critical)

Spec: 6 checks 4 to 11; the rest of case 4. Backlog: HR-141, HR-148, HR-204. Needs T5 (`judge_citation`), T6.

**Files:** modify `report_claims.rs` (the calls), `report_claims/citations.rs` (check 4 through `report_facts::judge_citation`; the comma lists of check 9, copied from spec 6.9; check 10's commit bodies, which count only a citation that `capture::read` reads; checks 10 and 11 read a capture's output only), `report_claims/narrative.rs` (the narrative reads `self_review[].claim`; check 7 walks every string of the report but the excluded fields), create `report_claims/evidence.rs` (checks 5 and 6), `main.rs` (the about), `crates/houserules/tests/check_report_claims.rs`.

**Interfaces:** consumes `report_facts::judge_citation`, `CitationFault`, and `capture::read` (checks 4, 10, 11). Produces nothing later tasks read.

- [ ] **Step 1: Write the tests** of every line of checks 4 to 7, the comma-list cases of check 9 (spec 6.9's five examples), check 10's commit-body cases, check 11's header case, and the passes of case 4. Record one RED run at BASE.
- [ ] **Step 2: Implement**; the module doc's limits gain the limits of the new checks (the trigger words of C7, the spaced-list rule of C9).
- [ ] **Step 3: Run the suite and every task gate; commit** `feat(report-claims): check recorded citations, lists, and claim evidence` with the three trailers.

### Task 8: regex lines for pack copies and overrides, and quoted values (critical)

Spec: 8.2, 8.3 (but the `update --help` sentence, which T11 writes); cases 7, 8 (but the help sentence). Backlog: HR-226, HR-227. Needs T3 (`shell::quote`).

**Files:** modify `crates/houserules/src/rules/check_packs.rs` (`pack_check_findings` and the override path give the regex lines), `install/mod.rs` (the two `--dir` remedies), `tracker/beads/check.rs` (each `bd config set` list), `template/.claude/skills/migrating-knowledge/SKILL.md` and its root copy (the quoted `bd config set` lines), `crates/houserules/tests/tracker_conformance.rs` (its pins of those lines), `crates/houserules/tests/update.rs` (lines 121 and 1538, `assert_subdirectory_refusal`) and `crates/houserules/tests/several_installs.rs` (line 587), whose pins of the remedy path build the expected line through `shlex::try_quote`; create `crates/houserules/tests/regex_and_quoting_36.rs`.

- [ ] **Step 1: Write the tests** of case 7 (a pack copy and an override with `(`; a fresh `init` with every kit pack passes) and case 8 (both `--dir` lines with a path that holds a space and with a relative plain path `sub`, so the plain case needs no quoting on any platform; the two-entry Beads list and a one-entry list). Walk spec 12's regex row. Record one RED run at BASE.
- [ ] **Step 2: Implement; run the suite and every task gate; commit** `fix: test every check pattern and quote printed values` with `Work-Item: HR-226` and `Work-Item: HR-227`.

### Task 9: a read-only token in the seeded CI workflow (critical)

Spec: 8.5; case 9, the workflow half of case 10. Backlog: HR-229.

**Files:** modify `template/.github/workflows/knowledge.yml` (`permissions:` with `contents: read`; `persist-credentials: false` under `with:`), `crates/houserules/src/install/ci_workflow.rs` (the GitHub form extends the block; the Beads form drops the `persist-credentials` line; the two markers in `hand_step_for_kept`), `install/hand_steps.rs` (`ci_lacks_contents_read(path)`, `ci_lacks_persisted_credentials_off(path)`, with spec 8.5's texts, and their `printed_texts` cases), `docs/upgrading-to-2.md` (a batch-36 section that shows both hand steps; T11 extends it); tests in `ci_workflow.rs` and `crates/houserules/tests/ci_workflow_36.rs`.

- [ ] **Step 1: Write the tests** of case 9 (each tracker form and the subdirectory form; the per-marker hand steps, a kept GitHub-form copy with and without its lines gets no `contents: read` step; actionlint on each form through `mise exec -- actionlint`) and the workflow half of case 10. Record one RED run at BASE.
- [ ] **Step 2: Implement; run the suite and every task gate; commit** `fix(ci): run the seeded workflow with a read-only token` with `Work-Item: HR-229`.

### Task 10: one `sha256_hex` and the six comments (standard)

Spec: 8.4, 8.7; cases 11, 11a (HR-228). Backlog: HR-228, HR-232.

**Files:** modify `crates/houserules/tests/conversion.rs` and `crates/houserules/tests/update.rs` (include `tracker_support` and call its `sha256_hex`; drop their copies), `crates/houserules/src/update_check.rs`, `crates/houserules/tests/update_check.rs`, `crates/houserules/src/bin/vacuous-exception-gate.rs` (the six comments, re-derived with the warn check's pattern).

- [ ] **Step 1: Record** the warn check's hits on the three files at BASE (`audit --ids houserules.comments-name-no-review-history` over a commit that touches them, or the pattern run directly; six hits) and `grep -n "fn sha256_hex"` over the two test files (two hits).
- [ ] **Step 2: Change the tests and the comments**; run the suite (no expectation changes) and every task gate; record the hits again (zero) and the grep (none).
- [ ] **Step 3: Commit** `chore: share one sha256_hex and drop review history from comments` with `Work-Item: HR-228` and `Work-Item: HR-232`.

### Task 11: the agent templates, the skills, the packs, the help texts, and the documents (critical: help output)

Spec: 7 (but the core step, T5), 8.3 (the `update --help` sentence), 8.6, 11; cases 11a (HR-230), 12 (by the controller). Backlog: HR-171, HR-141, HR-204, HR-230, HR-227.

**Files:** `template/workflows/reference/.claude/agents/{implementer.md,task-reviewer.md,branch-reviewer.md}`, `template/workflows/reference/.claude/skills/{orchestrating,planning,designing}/SKILL.md`, `template/workflows/reference/.claude/evals/dependency-add.json`, `template/packs/process.json` (three bodies and one summary of spec 7, and the Limits pointer of `process.claims-match-artifacts` repointed to the submodules' limits; patch version), the Limits pointer of `implementer.md` step 5 likewise, `crates/houserules/src/packs.rs` (`KIT_PACK_VERSIONS`), `crates/houserules/src/main.rs` (the help of `Validate` and `Update`, and the `update` line of `houserules --help`), `README.md` (the CLI table, an evidence section), `docs/upgrading-to-2.md` (the batch-36 section: the report form, closed workspaces, the reds of spec 12 with their remedies), and their root copies through `update --dir .`; `crates/houserules/tests/workflow_texts.rs` and `crates/houserules/tests/help_36.rs`.

- [ ] **Step 1: Write the tests**: each changed text holds its new sentence (one assertion per text of spec 7 and 8.6, case 11a), and each changed help line is pinned. Record one RED run at BASE.
- [ ] **Step 2: Change the texts and the help**; move the process pack's version and checksum; run `update --dir .`.
- [ ] **Step 3: Run the suite and every task gate**: the task audit and the PR-audit gate run with `--sanctioned process.evals-rerun=plan-controller-step-4` (the eval record lands at controller step 4, spec 19 case 12), and the report states it: its `self_audit` shows the row `fail` with `sanctioned` set and `summary.sanctioned_fail` 1. Add the kept-implementer red of spec 12 as a test (an install whose `implementer.md` `update` keeps as modified gets its hand step). **Commit** `docs: teach evidence by capture in the templates, skills, and guides` with the five trailers.

## Controller steps

1. **Before T1:** confirm this plan's review closed with no Critical finding; record each round in the ledger; close the plan window of `efficiency-windows.json`. Build the batch-BASE binary and tree: `git worktree add --detach ~/.cache/hr-b36/base-tree ce0fc0f`, then `CARGO_TARGET_DIR=~/.cache/hr-b36/target cargo build --locked --manifest-path ~/.cache/hr-b36/base-tree/Cargo.toml` and copy the binary to `~/.cache/hr-b36/base/houserules`; record its `--version` and sha in the ledger.
2. **After each integration:** the gates, and the runs the harness refused to an agent (the autocrlf suite, shellcheck), recorded in the ledger (spec 8.6 (1)).
3. **After T11 integrates:** reword this repository's override of `process.evidence-outlives-the-session` (`knowledge/overrides.json`, `body[2]`) and the body of `houserules.live-run-recipe` (`knowledge/houserules.json`) to captures (spec 7); `render`; commit `docs(knowledge): ...` with `Work-Item: HR-171`.
4. **After T11:** the eval run (every scenario through the changed templates and the branch binary; `record.json` appended and committed before the branch review; spec case 12); the scenarios' reports use the new form. The `process.evals-rerun` red of an update commit (spec 12) is this repository's own case: T11's sanctioned row, cleared by this commit; the ledger records it with its remedy.
5. **Live run:** spec case 13 by a dispatched implementer with a live-run brief, in a scratch repository under `~/.cache/hr-b36/live/` (no GitHub repository), with the branch binary: a task cycle through `capture`, `report-facts`, and `check-report-claims`, then each tamper of case 4; this repository's `update` with the tree binary, then a second `update` that changes nothing.
6. **Batch close:** `stats --window 5` over the workspaces to `stats.json`; `efficiency.json`; the branch review on opus; its fix wave; the knowledge commit of non-owner-gated proposals; `houserules set <id> status=done batch=36` for the 13 items; remove `~/.cache/hr-b36/base-tree` with `git worktree remove`.
7. **Finish:** aggregate into one to five clean commits, each with a `Work-Item:` trailer for every item it delivers; push, PR, CI green, the ff-only merge from the CLI (design.md 208); no release (PR #83 stays held until batch 38).

## Spec acceptance cases → tasks

| Case | Task | Case | Task |
|---|---|---|---|
| 1 | T3 | 8 | T8, T11 (help sentence) |
| 2 | T3 | 9 | T9 |
| 3 | T5 | 10 | T4 (schema, the validate reds, `stats` and `audit --workspace` before and after), T6 (check 1 on a kept schema), T9 (workflow), T11 (pack patch, guide, the kept-implementer red); controller steps 4 (the evals red) and 5 (`update` twice) |
| 4 | T6 (runs), T7 (claims) | 11 | T10 |
| 5 | T4 | 11a | T10 (HR-228), T11 (HR-230), T5 (core path) |
| 6 | T1 | 12 | controller step 4 |
| 7 | T8 | 13 | controller step 5 |
