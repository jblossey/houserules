# Batch 34 plan: the workflow and work items

> **For agentic workers:** dispatch through `.claude/agents/` (`implementer`, `task-reviewer`, `branch-reviewer`) per `.claude/skills/orchestrating/SKILL.md`. Steps use checkbox (`- [ ]`) syntax.

**Goal:** every backlog function runs through one tracker contract with JSON, GitHub Issues, and Beads adapters; the agent templates and lifecycle skills become an installable reference workflow; a plain `init` is minimal and passes every gate.

**Architecture:** a `tracker` module defines the contract as a Rust trait; the existing `backlog` module becomes the JSON adapter behind it, and two new adapters run the `gh` and `bd` CLIs. A `workflow` module embeds `template/workflows/reference/` with a manifest, pins its files in `.houserules.json`, and checks its required rule ids. `update` gains provenance-keyed conversion steps; `audit` resolves `Work-Item:` and `Deferred:` trailers through the contract.

**Tech stack:** Rust (the `houserules` crate), `serde_json`, `rust-embed`, the `gh` and `bd` CLIs as subprocesses, cargo only.

**Spec:** `docs/specs/2026-10-20-batch-34-workflow-and-work-items.md` (approved 2026-10-20, design.md 187, revision 4, rulings 179 to 186, calls C1 to C27). Each task reads the spec sections it names; a brief carries them by pointer (`process.brief-carries-the-spec`). Branch: `batch-34`. Workspace: `/home/jblossey/projects/houserules/.superpowers/sdd/2026-10-20-batch-34/` (absolute in every dispatch).

**Tier: standard** (design.md 172(2)): one task review, at most one re-review; a branch review on opus. **Critical tasks** (173): T1 to T7 change the CLI's commands, flags, or output, a schema, or the data on disk of an install. For each: the task's spec cases become tests first, the implementer commits them with one recorded RED run at BASE, then makes them pass test-first; evidence is that RED run and the whole suite green at HEAD; one task review on opus, never a re-review; the controller verifies each fix by re-running the reviewer's reproduction (`process.fix-round-verification-record`): Critical or Important to a fresh implementer, Minor fixed by the controller. T9 is critical too: it changes `knowledge-base.summary-is-the-rule`'s check pattern (HR-166 (1)), which changes what `check-knowledge` reports. T8 and T10 change texts and documents (T8 also refines `tools/reference-session-start.sh`, a hook script, not the CLI's output; its change still follows `process.tdd`): standard. **Eval run:** yes (T8 changes the implementer and task-reviewer templates, `process.evals-rerun`). **Plan review:** fresh reviewers until a round holds no Critical finding (design.md 187(2)); round 1 (`plan-review.json`) held none, and its 27 findings are folded into this revision.

**Order and parallelism (`process.parallel-on-disjoint-files`; at most three agents at once):**

| Wave | Tasks | Why |
|---|---|---|
| 1 | T1 | every tracker-facing task needs the contract and the marker keys |
| 2 | T2 ∥ T4 ∥ T5 | disjoint files (each task's Files list); each needs only T1's interfaces |
| 3 | T3 ∥ T6 | T3 needs T2's suite and registry; T6 needs T1; disjoint files |
| 4 | T7 ∥ T9 | T7 needs T3 (the `bd` pin) and T6 (workflow pins); T9 needs T6 (the process pack's new entry, landed with T6 per the Task 4 note); disjoint files |
| 5 | T8 | needs T6's moved files and T9's new entry ids |
| 6 | T10 | documents every behavior |

Files the controller owns at integration: `crates/houserules/payload.stamp`, `.houserules.json`, and the root copies `update --dir .` writes. Each task still commits its own stamp and root copies (its per-commit `check-commit` gate needs them); when a cherry-pick of the second task of a wave conflicts on them, the controller re-runs `cargo run --quiet --bin payload-stamp-gate -- --write` and `./target/debug/houserules update --dir .` on the cherry-picked commit and amends it. In wave 3, `mise.toml` is T3's: T6 runs shellcheck by hand on the moved hook's new path, names the one-line shellcheck change in its report, and the controller applies it when it cherry-picks T6.

Each parallel implementer works in its own worktree from the same BASE, writes scratch only under `<scratchpad>/task-<N>/`, and does not touch the files the controller regenerates (`houserules render` outputs, `backlog/`, `Cargo.lock` beyond its own `cargo add`). The controller integrates by cherry-pick, one task at a time, gates after each, and removes the task's worktree and `worktree-agent-*` branch after integration (HR-203 item 3).

## Cost measurement (172(6), `process.batches-measure-cost`)

The ledger records each dispatch's start, close, tokens, and tool uses; `efficiency.json` sums them per phase (design review, plan review, each task's implement and review, evals, live run, branch review). Design review so far: 3 rounds, 982,262 tokens, 3,367 s.

## Global constraints

- Use cargo for every package operation; add a crate with `cargo add <crate>@=<version>` and record the vetting in `dependency_vetting` (`security-hygiene.exact-pins`, `security-hygiene.dependency-vetting`). No new crate is planned; a task that needs one says why.
- Every user-facing line is the spec's exact text (sections 3 and 4); a test pins each line byte for byte.
- Commits: Conventional Commits, header and body lines at most 100 characters, no co-author or tool-attribution line; run `./target/debug/houserules check-commit --from origin/main` after every commit (the hook skips it).
- Use the tree binary (`cargo build`, then `./target/debug/houserules`) for every gate and for `update` and `render`.
- `template/` is the source: edit there, then `./target/debug/houserules update --dir .` and `cargo run --quiet --bin payload-stamp-gate -- --write` in the same commit (`houserules.template-is-the-source`).
- Windows: a test that compares a printed path builds the expected path in the form the binary prints (no `\\?\` prefix: `node_path::resolve_like_node`), component by component (`houserules.path-pins-mirror-the-code`); a test that reads back a file git checked out commits a `.gitattributes` of `* -text` first. Before closing, run the suite once with `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=true cargo test --locked --no-fail-fast` and once plainly; both pass.
- Read houserules output through files: write it to the workspace and read the file (5.137).
- Task gates: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`, the windows-gnu `cargo clippy --target x86_64-pc-windows-gnu --all-targets --locked -- -D warnings` (a `cargo check` until Task 11, C47), `cargo deny check`, shellcheck, `check-knowledge`, `check-backlog`, `render --check`, `residue-gate`, `vacuous-exception-gate`, `payload-stamp-gate`, `dist-generate-check`, `find-shell-tool-refs`, `check-commit --from <merge base>` (`houserules.task-gates-mirror-ci`).
- ASD-STE100 style for docs, comments, commit messages, and reports.

## Corpus runs (`houserules.plan-semantics-run-on-the-corpus`)

The matching rules this plan fixes, each run over the real corpus:

- `exempt_subjects` `^chore\(.+\): release ` over all 246 commits of main: 7 matches, every release commit from 0.2.0-alpha to 1.4.0, no other (`git log main --format="%h%x09%an%x09%s" | python3 -I corpus-exempt-subjects.py`, output `corpus-exempt-subjects.txt`). Bot authors on main: `dependabot[bot]` 10, `github-actions[bot]` 1.
- The rewrite set of spec section 7: 36 entries (`f4-rewrite-set-v4.txt`). The full-tier lines of spec section 5: 16 (`fact-full-tier.txt`). The required ids of spec 4.1: 25 (`fact-required-ids.txt`).
- GitHub item reads: 82 endpoint rows, 59 pull requests, 53 closed without reason (`fact-gh-counts.txt`): the adapter's pull-request drop and its read rows are tested against these shapes.
- Provenance (spec 4.7): this repository holds 9 baselines, the 8 kit-owned paths and the D3 knowledge schema; its deliverables schema equals the batch-33 content and its backlog schema equals v1.4.0's (`spec-review-r1-probe-repo-schemas.txt`). T7 runs the provenance function over this repository and saves each file's verdict before its own `update --dir .`, which is the act that converts this repository (Task 7 step 4).
- The open deferrals that get `defer --item` (spec 10): `python3 -I deferral-candidates.py open-items.json` over the 60 open items gives 18 hits; the controller read each (`deferral-verdicts.txt`): HR-121 (origin the batch-25 gate; files `dist-workspace.toml`, `.github/workflows/release.yml`), HR-124 (batch 25 T3a; `crates/houserules/src/report_claims.rs`), HR-164 (the batch-31 task-1 re-review cap; `crates/houserules/src/rules/stats.rs`), HR-205 (the batch-34 round-3 spec review; `crates/houserules/src/report_claims.rs`). The rest are scheduling rulings, features, or items this batch closes.
- Spec 15's function list: `plan-review-probe-62-rows.py` finds 34 meta-spec 6.2 rows touched by a batch-34 item and 34 spec-15 bullets covering them.

## Review focus

Five inputs the spec implies and no case names, each pinned by a test in the owning task:

1. A human-written ` ```houserules ` fence inside an issue body: the GitHub and Beads adapters read only the last fenced block of that tag, at the end of the body (T2 test `block_reads_only_the_trailing_houserules_fence`).
2. A trailer block with CRLF line endings or trailing spaces (a Windows commit): trailers parse the same (T4 test `trailers_parse_crlf_and_trailing_space`).
3. `defer --file` with a backslash path on Windows and a workflow pin key: stored and compared with forward slashes (T1 test `defer_stores_files_with_forward_slashes`; T6 test `workflow_pins_key_forward_slash_paths`).
4. A GitHub repository with more than 100 issues: every page is read (T2 test `github_reads_every_page_of_250_issues` against the fake `gh`).
5. `.houserules.json` `workflow.models` with a role missing and an unknown role: `update` adds the missing role from the manifest and keeps the unknown one unchanged, with no line (T6 test `update_adds_missing_model_roles_and_keeps_unknown`).

## Code-health scan (`process.code-health-scan`)

- `backlog/commands.rs` (1206 lines) reads files and computes in one place; T1 moves it behind the contract as the JSON adapter without changing its behavior, and the commands call the trait.
- `get.rs` routes by id shape inline; T1 replaces the routing with spec 3.8's rule in one function.
- `install/mod.rs` (1633 lines) mixes file-class constants with install logic, and `CLAUDE_FILES` ties agent templates to the Claude renderer; T1 and T6 move the classes into `install/classes.rs` and drop `CLAUDE_FILES`'s agent rows.
- `rules/audit.rs` (3419 lines) and `rules/check.rs` (3137 lines) are large; T4 puts trailer parsing in `rules/trailers.rs` and T6 puts the requirement check in `workflow/requirements.rs`, each called from the existing arm.
- `rules/stats.rs` names its deliverable files in two remedy strings and a matcher; T5 makes one list the source of all three.
- `packs.rs` hand-keeps version and hash rows; T4 and T9 bump rows through the existing pinning test, never by hand-typed hashes.
- Shared with batch 33: `install/conversion/*`, `rules/render/*`, `packs.rs`, `rules/kit_citations.rs`, the migrating-knowledge skill.

---

### Task 1: the tracker contract, the JSON adapter, and command routing (critical)

Spec: 3.1, 3.2 (JSON side), 3.3, 3.4, 3.5 (JSON paragraph), 3.7 (the schema version of the install marker), 3.8, 3.9 (the pattern write), 3.10 (pack line), 4.2 (`trackerFiles`). Live cases 2, 3, 4, 7, 10 (JSON rows), 18.

**Files:** create `crates/houserules/src/tracker/{mod.rs,json.rs,lines.rs}`, `crates/houserules/src/install/classes.rs`; rename `install/workflow.rs` to `install/ci_workflow.rs` (it is the seeded CI workflow; the crate's `workflow` module comes in T6) and add `ci_workflow::write_for(root: &Path, adapter: Option<Adapter>) -> Result<Vec<String>, String>` (today's file for every adapter; T7 fills each adapter); modify `backlog/*.rs`, `get.rs`, `archive.rs`, `main.rs` (`defer`, `list --deferred`, `init --tracker`, `--repo`, `--section`), `install/mod.rs`, `install/marker.rs` (`tracker`, `optInOffered`, the install schema `houserules/install@2.1.0`), `pack/mod.rs` (the process-pack line, :369), `template/backlog/schema.json` (`deferral`; the file carries no version id, so its version is the binary's release), `tests/backlog_parity.rs` and its harness (the frozen worktree's marker gains `tracker: {"adapter": "json"}`), every existing test that runs a backlog command on a marker with no tracker key; tests `crates/houserules/tests/{tracker_json.rs,no_tracker.rs}`.

**Interfaces (produces):**
- `tracker::Tracker` (trait): `name(&self) -> &'static str` (`"json"`, `"github"`, `"beads"`); `matches_id(&self, id: &str) -> bool`; `resolve(&self, id: &str) -> Result<Resolution, TrackerError>` with `tracker::Resolution::{Found(Resolved), NotFound, NotAnItem { reason: String }}` (T2 returns `NotAnItem { reason: "is a pull request, not an issue" }`; T4 prints the line); `list(&self, opts: &ListOpts) -> Result<Vec<Value>, TrackerError>`; `batch(&self, number: &str) -> Result<Value, TrackerError>`; `get(&self, ids: &[String]) -> Result<Vec<Value>, TrackerError>`; `set(&mut self, id: &str, assignments: &[String]) -> Result<Value, TrackerError>`; `defer(&mut self, request: &DeferRequest) -> Result<String, TrackerError>`; `archive_backlog_half(&mut self) -> Result<Vec<String>, TrackerError>` (lines to print); `check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError>` (errors, warnings); `backlog_id_pattern(&self) -> String`. `tracker::session_commands(adapter: Adapter) -> (&'static str, &'static str)` is a function of the adapter, so `render` never builds a tracker.
- `tracker::Resolved { status: String, title: String, deferral: Option<Deferral> }`; `tracker::Deferral { reason: String, origin: String, files: Vec<String> }`; `tracker::DeferRequest { item: Option<String>, title: Option<String>, reason: String, origin: String, files: Vec<String>, section: Option<String> }`.
- `tracker::TrackerError::{Unreachable { adapter: &'static str, cause: String, setup: String }, NotFound(String), Command(String)}`; `TrackerError::exit_code(&self, gate: bool) -> u8` (spec 3.4: gates 1, other commands 2).
- `tracker::installed(root: &Path, marker: &Marker) -> Result<Option<Box<dyn Tracker>>, TrackerError>` builds the adapter with no backend call (a backend is reached only by an operation); `tracker::register` (one match on `Adapter`, the only line T2 and T3 extend); `tracker::prepare_init(root: &Path, config: &TrackerConfig) -> Result<Vec<String>, TrackerError>` (the per-adapter init work and its lines: JSON writes `trackerFiles`; GitHub resolves the repository; T3 fills the Beads arm with its prefix read and refusal). `init --tracker` calls `prepare_init`, `write_backlog_id_pattern`, and `ci_workflow::write_for`, so T3 and T7 fill arms and never edit `init`.
- `tracker::lines::{NO_TRACKER, BACKLOG_WITHOUT_TRACKER, archive_skipped() -> String, audit_skipped(n: usize) -> String, unreachable(adapter, cause, setup) -> String, pack_needs_tracker(pack: &str) -> String}`: spec 3.4 and 3.10 exact.
- `Marker::tracker(&self) -> Option<TrackerConfig>`, `Marker::set_tracker(&mut self, config: &TrackerConfig)`, `Marker::opt_in_offered(&self) -> Vec<String>`, `Marker::set_opt_in_offered(&mut self, ids: &[String])`; `TrackerConfig { adapter: Adapter, repo: Option<String> }`; `Adapter::{Json, Github, Beads}`.
- `install::classes::{KIT_OWNED, SEED_ONCE, D3_FILES, TRACKER_DATA, TRACKER_SCHEMA}`; `tracker::write_backlog_id_pattern(root: &Path, tracker: &dyn Tracker) -> Result<bool, String>` (rewrites `backlogId` in `.claude/schemas/deliverables.json`; `false` when unchanged).

- [ ] **Step 1: Write the tests** in `tests/no_tracker.rs` and `tests/tracker_json.rs`. The new-behavior tests fail at BASE (the RED of 173(2)); the equivalence pins pass at BASE by design and are recorded in the same run, named as pins of unchanged behavior. The tests: one test per cell of spec 3.4's table and each line of 3.3 and 3.10 (names like `check_backlog_without_tracker_prints_line_a_exit_0`, `list_with_backlog_dir_and_no_tracker_prints_line_b_exit_2`, `archive_without_tracker_moves_knowledge_and_prints_the_archive_line`, `audit_without_tracker_prints_skipped_0`, `init_tracker_same_adapter_is_a_no_op`, `init_tracker_github_with_backlog_dir_refuses`, `pack_add_process_without_tracker_refuses`); JSON equivalence: the existing backlog goldens (`tests/goldens/backlog/**`, `tests/backlog_parity.rs`) pass with the tracker key in the frozen worktree (no second fixture set); `defer` (both forms, `--section`, the 120-character refusal, id rule, new items-file fields), `list --deferred` rows carry `deferral`; `get` routing of spec 3.8 including `get process.tdd` with a tracker that would fail if called (a test double that panics on any call); `defer_stores_files_with_forward_slashes`; `init --tracker json` writes the five data files and the schema and the `backlogId` pattern.
- [ ] **Step 2: Run them at BASE, record the run** (`cargo test --locked --test no_tracker --test tracker_json`): each new-behavior test fails (unknown flag, old line, or missing command); each pin passes.
- [ ] **Step 3: Build the contract and the JSON adapter.** `tracker/json.rs` wraps `LoadedBacklog`, `list_items`, `batch_record`, `get_items`, `set_item`, `check_backlog`, and the backlog half of `archive.rs` with no behavior change; every backlog command, `get`, and `archive` call `tracker::installed`. `init --tracker` writes `TrackerConfig` and the files of 4.2's `trackerFiles` (JSON only), then the pattern. `--tracker github` and `beads` write the key only in this task; their adapters arrive in T2 and T3 (`installed` returns `TrackerError::Command("the <adapter> adapter is not built yet")` until then; T2 and T3 replace those arms, and T3's step 4 proves none survives).
- [ ] **Step 4: Run the suite** (`cargo test --locked`), then every task gate.
- [ ] **Step 5: Commit** `test(tracker): ...` (the RED tests) then `feat(tracker): ...`.

**Task 1 as built (controller note after its review, `task-1-review.json`).** These signatures replace the Interfaces text above wherever they differ; later tasks consume these:
- `Tracker::archive_backlog_half(&mut self, lines: &mut Vec<String>) -> Result<(), TrackerError>`: the lines stay in `lines` when the sweep fails.
- `Tracker::set` returns `{"id": <id>, "changes": {<field>: <value>}}`; the command prints the 1.4.0 line from it for every adapter.
- `tracker::prepare_init(root: &Path, marker: &mut Marker, config: &TrackerConfig) -> Result<Vec<String>, TrackerError>` and `ci_workflow::write_for(root: &Path, marker: &mut Marker, adapter: Option<Adapter>) -> Result<Vec<String>, String>` take the in-memory marker.
- `prepare_init` runs the adapter's own arm, then one shared tail for every adapter: the tail builds the adapter through `register`, writes its `backlogId` pattern, and calls `write_for` (fix round 1 of Task 1). Task 2 and Task 3 fill their `register` and `prepare_init` arms only and get the pattern and the CI call from the tail; each adds `init_tracker_<adapter>_writes_the_pattern_and_the_ci_workflow`. Task 7 fills `write_for` per adapter and needs no call site.
- `Marker::tracker(&self) -> Result<Option<TrackerConfig>, String>`: a present, malformed key is a named fault (fix round 1 of Task 1).
- `Resolved` and `Deferral` carry a struct-level `expect(dead_code)` that stays fulfilled while any field is unread. Task 4 removes both expectations and reads each field in a tested path (`status`, `title`, `deferral.reason`, `origin`, `files`), or drops an unread field and says why in its report.
- No library target exists, and `tracker::write_backlog_id_pattern` is `pub(crate)`: an integration test under `tests/` reaches the pattern through the binary (`init --tracker json` on a scratch install, then read its deliverables schema).

### Task 2: the GitHub adapter, the houserules block, and the conformance suite (critical)

Spec: 3.2, 3.5 (GitHub column, block, read rows, pull requests, creation on write, rejected native means), 3.6 (fake `gh`), 3.7 (the PR line), 3.8 (`matches_id`). Live case 9.

**Files:** create `crates/houserules/src/tracker/{github.rs,block.rs,gh.rs}`, `crates/houserules/src/bin/fake-gh.rs`, `crates/houserules/tests/tracker_conformance.rs`; modify `tracker/mod.rs` (the `register` and `prepare_init` arms for `Github` only).

**Interfaces:**
- Consumes: T1's `Tracker`, `TrackerConfig`, `lines`.
- Produces: `tracker::block::{parse(body: &str) -> (String, BTreeMap<String, Value>), render(prose: &str, fields: &BTreeMap<String, Value>) -> String}` (the trailing fence only); `tracker::gh::Runner` (spawns `gh` from `PATH`, or the program named by `HOUSERULES_GH` for tests; arguments as separate argv entries, no shell); the conformance harness `tracker_conformance::Fixture { seed: Vec<Value>, id_map: BTreeMap<String, String> }` and `assert_equivalent(adapter_output: &Value, json_output: &Value, map: &BTreeMap<String, String>)` (spec 3.2: ids mapped, `file` removed, rows keyed by id). T3 adds its adapter to the same harness.

- [ ] **Step 1: Write the failing tests:** one conformance test per row of 3.5's mapping table and per contract check, parameterized by adapter (JSON and GitHub now); `block_reads_only_the_trailing_houserules_fence`; `github_reads_every_page_of_250_issues`; `github_drops_pull_requests` (the PR line of 3.5); `github_bare_number_and_hash_resolve_alike`; `github_closed_without_reason_reads_done`; `github_two_type_labels_fail_the_adapter_check`; `github_set_creates_label_and_milestone`; one test per unreachable cause of 3.4: `github_unreachable_gh_not_installed`, `github_unreachable_not_logged_in`, `github_unreachable_cannot_see_repository`, `github_unreachable_other_failure_prints_first_stderr_line`; `github_does_not_resolve_other_repositories`; `render_check_and_get_knowledge_never_call_gh` (an unreachable fake `gh`: `render --check` and `get process.tdd` exit 0). The JSON parameter of each conformance test passes at BASE by design (a pin); the GitHub parameter fails (the RED).
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Build `fake-gh`** (serves `gh api --paginate`, `gh issue create|edit|close`, `gh label create|list`, `gh api .../milestones`, `gh auth status` from a JSON store named by `FAKE_GH_STORE`), then `tracker/github.rs` to the mapping of 3.5. Confirm `dist-generate-check` and the release config exclude `fake-gh` from the shipped artifacts.
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(tracker): ...` then `feat(tracker): ...`.

### Task 3: the Beads adapter and `bd` in CI (critical)

Spec: 3.5 (Beads column, read rows, `partial:active`, rejected native means), 3.3 (`init --tracker beads`), 3.4 (Beads causes), 3.6. Live cases 8, 10 (Beads row).

**Files:** create `crates/houserules/src/tracker/beads.rs`; modify `tracker/mod.rs` (the `Beads` arms of `register` and `prepare_init` only), `tests/tracker_conformance.rs` (the Beads parameter), `mise.toml` (pin `bd`), `.github/workflows/ci.yml` (install `bd` through mise on all three systems).

**Interfaces:** consumes T1's contract and T2's harness; produces `tracker::beads::prefix(root: &Path) -> Result<String, TrackerError>` (T7 calls it for the CI pin and `update`'s pattern refresh); `tracker::beads::BD_VERSION: &str` (the pinned release, read from `mise.toml` by a test so the two cannot drift).

- [ ] **Step 1: Verify against the pinned release and record in `docs_verified`:** the JSON of `bd list --json` and `bd show --json`, `close_reason`, custom status configuration with a category, custom types, `bd bootstrap`, and the platforms `bd` releases for. Record each command and its output in the workspace.
- [ ] **Step 2: Write the failing tests:** the Beads parameter of every conformance test; `beads_in_progress_reads_open`; `beads_decision_item_and_decision_record_stay_apart`; `beads_missing_partial_status_names_the_bd_config_step`; `init_tracker_beads_without_bd_refuses`, `..._without_store_refuses`; one test per unreachable cause: `beads_unreachable_bd_not_installed`, `beads_unreachable_no_store`, `beads_unreachable_other_failure_prints_first_stderr_line`; `beads_tests_fail_when_bd_is_missing` (the harness fails, never skips); the end-to-end `audit_names_a_pull_request_trailer` with the fake `gh` (T2's adapter and T4's line, both landed by wave 3).
- [ ] **Step 3: Record the RED at BASE; build the adapter; add `bd` to `mise.toml` and the CI install step.**
- [ ] **Step 4: Suite and gates;** the CI matrix runs the Beads tests on Linux, macOS, and Windows (or the platform without a release gets the stated job of 3.6); `init --tracker github` and `init --tracker beads` then `list` reach their adapters, and `grep -rn 'is not built yet' crates/houserules/src` prints nothing.
- [ ] **Step 5: Commit** `test(tracker): ...`, `feat(tracker): ...`, `ci: install bd for the Beads conformance tests`.

### Task 4: trailers, deliverable deferrals, and the every-commit entry (critical)

Spec: 3.7 (every bullet), 3.4 (the audit line). Live cases 5, 6.

**Files:** create `crates/houserules/src/rules/trailers.rs`, `crates/houserules/src/pack/opt_in.rs`; modify `rules/mod.rs` (the new module), `rules/audit.rs` (the commits arm and `--report`/`--workspace` deferral resolution), `rules/check_shape.rs` (the new fields, beside `body_absent` and `per_commit`), `rules/check_commit.rs` (their evaluation), `template/knowledge/schema.json` and the embedded pack schema (`require_trailer`, `exempt_authors`, `exempt_subjects`; the pack schema's `optIn`), `pack/mod.rs` (`add`, `upgrade`, `remove` call `opt_in`), `install/pack_moves.rs` (the pin move calls `opt_in`), `template/packs/process.json` (`process.every-commit-names-a-work-item` and the manifest's `optIn`), `packs.rs` (the process row, and `PACK_SCHEMA_MINOR` 0 to 1: the pack schema becomes `houserules/pack@1.1.0`, pinned by a test); tests `crates/houserules/tests/{trailers.rs,opt_in.rs}`.

**Interfaces:**
- Consumes: T1's `Tracker::resolve`, `matches_id`, `Marker::opt_in_offered`.
- Produces: `rules::trailers::parse(message: &str) -> Vec<(String, String)>` (runs `git interpret-trailers --parse` on the message); `pack::opt_in::offer(root: &Path, marker: &mut Marker, pack: &PackCopy) -> Result<Vec<String>, String>` (writes each new `optIn` exclusion once, records `optInOffered`, returns the lines to print; T6 and T7 call it from `init --add-required-packs` and the 1.x conversion); `pack::opt_in::withdraw(root, marker, pack) -> Result<(), String>` (`pack remove`).
- The entry id `process.every-commit-names-a-work-item` (T9 and T8 must never cite it, spec 4.1).

- [ ] **Step 1: Write the failing tests:** every line of 3.7; several `Work-Item:` trailers on one commit; `Deferred:` with no reason; an id of no declared shape reaches no backend (a recording fake asserts zero calls); `issues[].backlog` resolution under `--report` and `--workspace`; author exemption by `%an`; subject exemption on a human-authored `chore(main): release 2.0.0`; message-file mode applies the subject exemption only; `trailers_parse_crlf_and_trailing_space`; the opt-in through `pack add`, `pack upgrade`, the pin move, `pack remove` then `pack add` (the active line), and a second `update` after the project deleted the exclusion.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Build the parse, the arms, the schema fields, the opt-in module, and the entry.** The knowledge schema file carries no version id; its version is the binary's release. `audit` prints `Resolution::NotAnItem`'s line from 3.5 with the commit's short sha (the process pack's version moves; regenerate the `packs.rs` row through its pinning test).
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(audit): ...` then `feat(audit): ...`.

### Task 5: the deliverables schema, `stats` by model, and `check-report-claims --doc/--item` (critical)

Spec: 4.5, 3.9 (the template's no-tracker pattern), 9.1 (HR-129 (2) `gates_run`; HR-165 (1), (2), (3) first half). Live cases 16, 17.

**Files:** modify `template/.claude/schemas/deliverables.json` and its root copy `.claude/schemas/deliverables.json` (hand-synced, as the parity test's doc says), `rules/deliverable.rs` (the one list of deliverable file patterns, `spec-review*.json` added; `audit` reads it unchanged), `rules/validate_deliverable.rs`, `rules/stats.rs` (its remedy strings read that list), `rules/cost.rs`, `tests/dogfood.rs` (the parity test `deliverables_schema_equals_template_source_with_the_id_prefix_rewrite` compares this repository's schema with the schema that `init --tracker <this repository's tracker.adapter>` writes on a scratch install with this repository's `idPrefix`, through the binary, and no expected-difference allowance), `report_claims.rs` and `report_claims/`, `main.rs` (`check-report-claims --doc`, `--item`), `#![warn(missing_docs)]` in `main.rs` (HR-165 (2), HR-204 item 9: the crate is binary-only and exposes one `pub` item, so the lint covers the crate root and that item; design.md 177's own condition, "where the crate exposes a library surface", is met by nothing more; HR-165 and HR-204 record this, and item 9's second half, the history-narration check over doc comments, goes with item 8 to batch 35), README's sample `stats` block and its slice test (HR-165 (1)); tests `crates/houserules/tests/{deliverables_model.rs,stats_models.rs,report_claims_docs.rs}`.

**Interfaces:**
- Consumes: T1's `tracker::installed` (for `--item`).
- Produces: the `spec-review` kind (fields exactly as spec 4.5), `model` required on every kind, `fix_rounds[].gates_run`; `stats` key `models` (rows: `model`, the cost fields, `findings` by `critical`/`important`/`minor`/`other`; deliverables without `model` in `pre-2.0`); `deliverable::WorkspaceFiles` gains `spec_reviews` (the one list of patterns lives in `rules/deliverable.rs`). The deliverables schema file carries no version id; its version is the binary's release.

- [ ] **Step 1: Write the failing tests:** `validate` refuses a report without `model`; accepts a `spec-review` with spec 4.5's fields and lowercase severities; refuses one with `Critical`; `stats` prints `models` with a `pre-2.0` row and counts a `spec-review.json`; `check-report-claims --doc docs/specs/x.md` names a cited workspace path that does not exist (the line of 9.1); `--item` with no tracker prints line A and exits 2; the README stats slice equals the generated slice.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Build.** The template schema's `backlogId` default becomes the no-tracker pattern of 3.9 (`^[A-Za-z0-9#._-]+$`); T1's writer rewrites it per adapter.
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(...)` then `feat(...)`.

### Task 6: the workflow package, the minimal default, and the AGENTS.md block (critical)

Spec: 4.1, 4.2, 4.3, 4.4, 4.4a, 4.6, 3.10 (session commands and the 6.3 advice in the block; the `check-knowledge` lines for the process pack or the workflow without a tracker), 5 (only the `Tier:` dispatch format in code, if any). Live cases 1, 11, 12, 13, 16 (the `init` half), 23, 25.

**Files:** create `crates/houserules/src/workflow/{mod.rs,manifest.rs,pins.rs,requirements.rs}` (the reference workflow; the seeded CI module is `install/ci_workflow.rs` since T1), `template/workflows/reference/workflow.json`, `template/.claude/skills/houserules-step/SKILL.md`, `template/tools/houserules-step.sh`, `template/workflows/reference/tools/reference-session-start.sh` (the 1.x ritual line moved out of the core script; T8 refines its text); move (`git mv`) into `template/workflows/reference/` the three agent templates, the orchestrating and finishing-a-feature skills, the three eval scenarios, and `template/.githooks/commit-msg`, at their install paths; modify `install/mod.rs`, `install/classes.rs`, `install/sync.rs`, `main.rs` (`init --workflow`, `--add-required-packs`; `files` output), `rules/check.rs` (calls `workflow::requirements`), `rules/kit_citations.rs` (workflow files when installed), `rules/render/agents_md.rs` (session commands, the 6.3 advice, `agentsBlock`), `rules/render/claude.rs` (the `project-knowledge` skill per 5.141), `rules/render/mod.rs` (its module doc line 14, F3), `rules/model.rs` (`Overrides` becomes `pub(crate)` for `requirements::unmet`), `.gitignore` seeding of `.houserules/step/`, `tests/dogfood.rs` (the template file-set test, the root kit-owned copies test, the orchestrating-skill path at :431), `tests/install.rs` (its own `KIT_OWNED` copy), `tests/init_install.rs`, `tests/update.rs`; tests `crates/houserules/tests/{workflow_install.rs,minimal_default.rs,houserules_step.rs}`.

**Interfaces:**
- Consumes: T1 (`tracker::installed`, classes), T4 (`pack::opt_in::offer` for `--add-required-packs`).
- The install schema stays `houserules/install@2.1.0` (T1 set it); the `workflow` key lands under it in the same batch.
- Produces: `workflow::Manifest { name: String, version: String, schema: String, requires: Vec<String>, files: Vec<String>, models: BTreeMap<String, String>, agents_block: String }`; `workflow::reference() -> Manifest` (embedded; `version` is `env!("CARGO_PKG_VERSION")`); `workflow::pins::{read(marker) -> Option<WorkflowPin>, write(marker, pin)}` with `WorkflowPin { name, version, files: BTreeMap<String, String>, models: BTreeMap<String, String> }` (keys forward-slash paths); `workflow::sync(root: &Path, marker: &mut Marker) -> Result<Vec<String>, String>` (the 4.3 lines); `workflow::requirements::unmet(base: &Base, manifest: &Manifest, overrides: &Overrides) -> Vec<String>` (the three 4.3 lines); `workflow::is_kit_file(root, path, marker) -> Result<bool, String>` (spec 4.7's provenance; T7 uses it).
- The workflow files after the move: `template/workflows/reference/.claude/agents/{implementer,task-reviewer,branch-reviewer}.md`, `.../.claude/skills/{orchestrating,finishing-a-feature}/SKILL.md`, `.../.claude/evals/{dependency-add,docs-edit,seeded-violations}.json`, `.../.githooks/commit-msg`. T8 adds `spec-reviewer.md`, `designing/SKILL.md`, `planning/SKILL.md` and their manifest rows.

- [ ] **Step 1: Write the failing tests:** case 1's file list from spec 4.6 (written in the test), and a unit test pinning the code's constant to it; every gate on a minimal install; `update` twice writes no `backlog/` and no eval file; `init --workflow reference` refusal line, with `--tracker json`, with `--add-required-packs`; no settings hook entry without `--target claude`, and `init --target claude` adds it; each unmet-requirement line; `update` on a locally edited workflow file prints the kept line twice and changes nothing; a pinless project file at a workflow path stays untouched after `init --workflow` and two updates (case 16's second half); `workflow_pins_key_forward_slash_paths`; `update_adds_missing_model_roles_and_keeps_unknown`; `requires` equals the cited ids (`fact-required-ids.py`'s rule, in Rust); no workflow file cites an `optIn` id; no template carries `model:`; the AGENTS.md block carries the adapter's session commands, the 6.3 advice, and `agentsBlock` only when the workflow is installed; the `project-knowledge` skill has no dispatch step and the new description; case 25's step run on a minimal install.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Move the files, write the manifest (`requires` from the rule; `models` and `agentsBlock` exactly as spec 4.1), and build the module.** `KIT_OWNED` becomes spec 4.2's list; the eval files and the moved files leave `SEED_ONCE` and `KIT_OWNED`; `CLAUDE_FILES` keeps only the session-start script.
- [ ] **Step 4: Suite and gates.**
- [ ] **Step 5: Commit** `test(workflow): ...` then `feat(workflow): ...`.

**Task 6 also lands the every-commit entry (controller note after the Task 4 review, `task-4-review.json`).** Task 4 built the trailer arms, the `commits` fields, and `pack::opt_in`, but its entry cannot land green before a plain `init` stops seeding the process pack: the entry is held as a commit on a branch of its own (`t4-entry`, deleted once the entry landed with Task 6) (template/packs/process.json: the entry and the manifest's `optIn` row; packs.rs: the process row). Task 6 cherry-picks it and owns: `template/packs/process.json`, the process row of `packs.rs` (through its pinning test), `tests/every_commit_entry.rs` (live case 6's first and last halves), and the expected-output updates of `tests/pack_moves.rs` and `tests/kit_citations.rs` that the pin move's exclusion causes. Every Task 6 `init` path that installs the process pack (`--add-required-packs` and any other) calls `pack::opt_in::offer`, writes the marker after it, and renders. Task 9 consumes the entry from Task 6.

**Task 6 as built (controller note after its review, `task-6-review.json`).** These replace the Interfaces text above where they differ; Tasks 7 and 8 consume these: `workflow::requirements::unmet(base: &Base, manifest: &Manifest) -> Vec<String>` (it reads `Base::excluded`; `Overrides` stays private); `workflow::is_kit_file(root: &Path, manifest: &Manifest, path: &str, marker: &Marker) -> Result<bool, String>`; `workflow::pins::read(marker) -> Result<Option<WorkflowPin>, String>`; `workflow.json` has no `version` key (the version is `env!("CARGO_PKG_VERSION")`); after the fix round `WorkflowPin.files` maps a project's own path to `null` (C33). The minimal default broke 114 tests outside the file list (`t6-red-minimal-packs-alone.txt`); the controller admits the files the task changed for that cause: tests `archive.rs`, `check_packs.rs`, `commit_msg_hook.rs`, `conversion.rs`, `kit_texts.rs`, `no_tracker.rs`, `render_targets.rs`, `several_installs.rs`, `stats_models.rs`, `tracker_conformance.rs`, `tracker_json.rs`, `tracker_support/mod.rs`, `update_check.rs`, the mini fixture and the render goldens; sources `rules/check_commit.rs` (spec 4.6's checked-nothing line), `pack/mod.rs`, `install/marker.rs`, `bin/residue-gate.rs`. Task 7 also owns: C35 (the known contents cover every 1.x content of each `kitOwned` file, and `init` and `update` replace a match), after which `conversion.rs` `init_as_the_line_prints_it_gives_a_fresh_install_with_the_old_prefix` runs only the printed lines; C33's `null` in 4.7 step 2; the `tests/dogfood.rs` pin that each root workflow copy equals its package source. Task 7 consumes the fallible `Tracker::backlog_id_pattern` of C37 for the `update` refresh.

**Task 3 as built (controller note after its review, `task-3-review.json`).** `tracker::beads::{prefix, prefix_for_init, BD_VERSION}`; the adapter lives in `tracker/beads/`. After its fix round: `Tracker::backlog_id_pattern` returns `Result<String, TrackerError>` (C37), the record builders shared by the GitHub and Beads adapters live in one tracker module, and the `bd` pin names `github:gastownhall/beads`. Every agent that may run `bd` runs it with an isolated `HOME` (and `USERPROFILE`), `BD_DISABLE_METRICS=1`, `DO_NOT_TRACK=1`, and no `origin` remote before `bd init`; the Task 7 brief and the live run carry the rule.

**Task 7 as built (controller note after its review, `task-7-review.json`).** The controller admits the files the task changed beyond its list, each for a stated cause: `install/classes.rs` (the deliverables schema in `d3Files`, spec 4.2), `tracker/mod.rs` (`replace_backlog_id_pattern` is `pub(crate)`), `workflow/requirements.rs` (the shared hand-step text and `excluded_ids`), and tests `install.rs`, `init_install.rs`, `released.rs`. `install/conversion/plan.rs` carries the freeze fix (the brief named it). Its fix round adds `main.rs` (the help of `update` and `init --tracker`), `template/.claude/settings.json` (C40), and `workflow/mod.rs` (`adopt` drops the baselines of the paths it pins).

### Task 7: conversion steps, known contents, schema delivery, the CI per adapter, and the session-start split (critical)

Spec: 4.7 (steps 1 to 5 and the invariant), 4.8, 3.3 (`update` refreshing the Beads prefix), 3.9 (`update` writes the pattern). Live cases 14, 15, 19 (the `update` half), 24.

**Files:** create `crates/houserules/src/install/conversion/steps.rs`; modify `install/conversion/{mod.rs,d3.rs}`, `crates/houserules/released/` (the index and the added contents), `install/ci_workflow.rs` (`write_for` per adapter, `knowledge.yml` and `knowledge-<folder>.yml`), `install/hand_steps.rs` (the new hand steps), `docs/upgrading-to-2.md` (the exact text of each new hand step, which the batch-33 hand-step test requires; T10 writes the prose around them), `mise.toml` and `.github/workflows/ci.yml` (`actionlint`, exact pin, vetting recorded), `template/.github/workflows/knowledge.yml` (and the per-adapter fragments it is built from), `template/tools/claude-session-start.sh` (core: the compaction re-injection only); tests `crates/houserules/tests/{conversion_34.rs,ci_per_adapter.rs}`.

**Interfaces:**
- Consumes: T1 (`write_backlog_id_pattern`, classes), T3 (`beads::prefix`, `BD_VERSION`), T4 (`opt_in::offer` in the 1.x conversion), T6 (`workflow::is_kit_file`, `workflow::sync`, the pin types).
- Produces: `install::conversion::steps::run(ctx: &Context, marker: &mut Marker) -> Result<Vec<String>, String>` (steps 1 to 4 of 4.7, after the batch-33 steps); `d3::known_contents(path: &str, prefix: &str) -> Result<Vec<Vec<u8>>, String>` (every released content of `path` plus its batch-33 main content); the released index lists, per path, each 1.x tag's content of every kit-owned and seed-once file this batch reclassifies, plus the batch-33 contents of the deliverables and backlog schemas (each extracted with `git show <tag>:template/<path>`; the script and its output go to the workspace).

- [ ] **Step 1: Write the failing tests:** each population of case 15 and case 14 (a 1.4.0 install seeded by the v1.4.0 binary from the existing `tests/v1_4_0` fixture) with each step's line or silence; the reds of case 14 with their remedies; case 16's first half (a project's own workflow-path files on a minimal install: nothing adopted, nothing written, no line); step 3's three branches (the kit's script, a changed one, an owned one) and the hook entry; step 4 on this repository's shape (a batch-33 deliverables schema: delivered); the Beads prefix refresh; the CI per adapter passes `actionlint` (the test fails, naming `mise install`, when `actionlint` is missing; CI installs it through mise) and holds the job-level permissions and step env of 4.8; a modified CI file gets the hand step.
- [ ] **Step 2: Record the RED at BASE.**
- [ ] **Step 3: Extract the released contents (record the script), build the steps, the CI writer, and the core session-start script.**
- [ ] **Step 4: Convert this repository.** Run `workflow::is_kit_file` over this repository and save each file's verdict to the workspace (the provenance corpus run); then `./target/debug/houserules update --dir .`, record its lines in the report, and commit the result with the task: this is the one act that converts this repository. Suite and gates.
- [ ] **Step 5: Commit** `test(install): ...` then `feat(install): ...`.

### Task 8: the reference workflow's texts (standard)

Spec: 4.1 (the trailer teaching; no `optIn` citation), 5 (two tiers, the critical task, the 16 lines of F7 in the workflow files), 6.1 (spec-reviewer, the loop of 178), 6.2 (with design.md 185), 6.3, 6.4 (designing and planning skills, no plugin name), 9.1 (HR-129 (1), (3), (4); HR-153's template defects as ruled; HR-165 (4) to (7) with point 6's adopter wording; HR-166 (3); HR-150), 9.3 (HR-203 items 3 and 4). Live cases 20, 21 (workflow half), 22.

**Files:** `template/workflows/reference/**` (`workflow.json`'s `requires` grows by the four ids of design.md 182, which the spec-reviewer cites; the moved files, `tools/reference-session-start.sh`, and new `spec-reviewer.md`, `designing/SKILL.md`, `planning/SKILL.md` with their `workflow.json` rows), the eval scenarios' expectations; then `./target/debug/houserules update --dir .` for the root copies.

- [ ] **Step 1:** Write a test (`tests/workflow_texts.rs`) that fails at BASE: the case-insensitive grep for `superpowers` over `template/` and the embedded texts prints nothing; `fact-full-tier.py`'s rule finds no hit in the workflow files; the spec-reviewer template names the four design-phase entries of design.md 182 by id; the implementer template writes a `Work-Item:` trailer per `Backlog:` id; no `model:` line.
- [ ] **Step 2: Rewrite the texts** to each cited section, in ASD-STE100 style, reading houserules output through files (6.3).
- [ ] **Step 3: Suite and gates, then commit** `docs(workflow): ...` (a `feat` only if executable code changes). T8's audit runs with `--sanctioned process.evals-rerun=<the eval run of controller step 3>`, because its root template changes reach `record.json` only at that run.

**Task 8 as built (controller note after its review, `task-8-review.json`).** The controller admits `tests/install.rs` (the three new workflow files in its list) and `tests/workflow_install.rs` (`requires` grows from 25 to 29), which the manifest rows force. Its fix round moves the shared guard lists of `tests/pack_texts.rs` and `tests/workflow_texts.rs` into one test-support module.

### Task 9: the kit's pack entries and core texts (standard)

Spec: 7 (the 36 entries of F3 and the non-entry lines, 5.152), 5 (the pack's 16 tier lines; the critical task in `process.risk-tiers` and `process.tdd`), 6.4 (`process.brainstorm-first` body[0]), 8 (design.md 179's narrowed `process.rulings-to-file`; design.md 183's eight clauses; `knowledge-base.state-only-the-source`'s clauses), 9.1 HR-166 (1) the until-clause pattern `(?:[^.;:]|\.\d){0,40}` in both copies of `knowledge-base.summary-is-the-rule`'s check with the phrase 'until 1.3.0 ships' added to its test's PHRASES, and (2) its two texts (the entry body and the test module doc); design.md 182's four entries (ids: `process.design-decisions-carry-probes`, `process.design-claims-are-labelled`, `process.specs-list-borrowed-concepts`, `process.reversals-are-recorded`), the migrating-knowledge skill (adapter sections, setups, 6.3 advice, design.md 181's uncovered skills), template AGENTS.md, CLAUDE.md, and docs/README.md lines of F3. Live cases 20, 21 (pack half).

**Files:** `template/packs/*.json`, `crates/houserules/src/packs.rs` (version rows through the pinning test), `knowledge/overrides.json` (the second copy of the until-clause pattern, :86), `tests/kit_time_sensitive_check.rs` (`PHRASES`), `tests/dogfood.rs` (`orchestrating_skill_carries_the_agents_md_parallel_line_verbatim` follows the line into `agentsBlock`), `template/.claude/skills/migrating-knowledge/SKILL.md`, `template/AGENTS.md`, `template/CLAUDE.md`, `template/docs/README.md`; then `update --dir .` and `render`.

- [ ] **Step 1:** Write a test (`tests/pack_texts.rs`) that fails at BASE: `f4-rewrite-set-v4.py`'s rule finds no rewrite-class hit in the 36 entries outside the kept classes; no pack line matches the full-tier rule; the four new ids exist, non-standing, in the process pack; `process.rulings-to-file`'s summary carries design.md 179(1).
- [ ] **Step 2: Rewrite the entries and texts;** bump each changed pack's version; regenerate the `packs.rs` rows through the pinning test.
- [ ] **Step 3: Suite and gates** (`check-knowledge` passes with the kit-citation lint over every installed text), **then commit** `docs(packs): ...`.

### Task 10: the documents (standard)

Spec: 11. Live: none; the hand-step test of batch 33 covers each new hand step.

**Files:** `docs/upgrading-to-2.md` (the batch-34 section), `README.md` (`--tracker`, `--workflow`, `defer`, the minimal default, the step, the ownership table and agent-layer bullet from `houserules files`, `houserules.readme-mirrors-kit-owned`).

- [ ] **Step 1:** No RED: a documents task. T7 already added each hand step's exact text, so the hand-step test passes at BASE.
- [ ] **Step 2: Write the sections; check every command and flag against the tree binary's `--help`.**
- [ ] **Step 3: Gates, then commit** `docs: document trackers, the reference workflow, and the minimal default`.

## Controller steps

1. **Before T1:** confirm this plan's review closed with no Critical finding (design.md 187(2)); record each round in the ledger.
1a. **After T1 integrates, before wave 2's BASE:** run `./target/debug/houserules init --tracker json --dir .` on this repository (it writes the `tracker` key and the `backlogId` pattern and keeps `backlog/`), record its output, and commit. From then on every gate here sees the JSON tracker.
1b. **From T7's integration on** (the new deliverables schema is delivered): every dispatch names the model the agent records in `model`.
2. **After T7 and T9 integrate:** T7's own `update --dir .` converted this repository (Task 7 step 4); run a second `update` (no change) and record it; retire `process.no-full-tier`, `process.critical-tasks-test-first`, and `process.full-tier-measures-cost` (the last with a `see` link to the new local `process.batches-measure-cost`); mark HR-121, HR-124, HR-164, HR-205, and HR-206 (origin batch 34 Task 4 fix round 1; files `crates/houserules/src/pack/mod.rs`, `crates/houserules/src/pack/opt_in.rs`) with `defer --item` (the origins, reasons, and files of the Corpus runs section); `render`; commit.
3. **After T8:** the eval run (every scenario; `record.json` appended).
4. **Live run:** spec section 16. A dispatched implementer with a live-run brief runs every case but 22 and 23; the controller runs cases 22 and 23 itself in a scratch install, because they need a design review and dispatches, which an implementer cannot start. The controller first creates a private scratch repository for cases 9 and 24 (`gh repo create <name> --private`), records it in the ledger, and every later report reminds the owner to delete it (design.md 180).
5. **Batch close:** `stats` over the last five workspaces to `stats.json`; the branch review on opus; its fix wave; the knowledge commit of non-owner-gated proposals; `houserules set <id> status=done batch=34` for the 13 items and HR-182; HR-153's record line for its two superseded defects; HR-166's note for its fourth point; HR-165's and HR-204's notes for the `missing_docs` scope.
6. **Finish:** aggregate into clean commits, each with a `Work-Item:` trailer for every item it delivers; then the batch-close commit deletes the `process.every-commit-names-a-work-item` exclusion (spec section 10). Push, PR, CI green, and the ff-only merge from the CLI follow without a further owner word, with one to five aggregated commits and no release (design.md 190; release PR #83 stays held until batch 38, design.md 193).

## Spec acceptance cases → tasks

| Case | Task | Case | Task |
|---|---|---|---|
| 1 | T6 | 14 | T7 |
| 2 | T1 | 15 | T7 |
| 3 | T1 | 16 | T6, T7 |
| 4 | T1 | 17 | T5 |
| 5 | T4 | 18 | T1 |
| 6 | T4 | 19 | controller step 2 |
| 7 | T1 | 20 | T8, T9 |
| 8 | T3 | 21 | T8, T9 |
| 9 | T2 | 22 | T8, controller live run |
| 10 | T1, T2, T3 | 23 | T8, controller live run |
| 11 | T6 | 24 | T7 |
| 12 | T6 | 25 | T6 |
| 13 | T6 | | |
