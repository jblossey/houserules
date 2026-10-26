# Batch 38 plan: OpenSpec, spec and session links, and the 2.0.0 release work

houserules reads an OpenSpec change as the intent of a change, takes OpenSpec's own validation result as a capture, links commits to specs and agent sessions by trailers, and offers two optional checks over them. `render` tells an agent where OpenSpec plans and where houserules rules, and two planning workflows draw a warning. The surface freeze of 1.x retires for its successor. The batch ends release-ready: the upgrade proof, the issue comments, and the README, which the owner reviews last.

> **For agentic workers:** dispatch each task through the `implementer` template; review through `task-reviewer`. Steps use checkbox (`- [ ]`) syntax.

**Revision 1.** Spec: `docs/specs/2026-10-25-batch-38-openspec-and-release.md` (revision 3 and section 18; approved under design.md 265 and 275). Rulings 266 to 277 shape it. Branch `batch-38`, rebased on `batch-37`; development starts after batch 37 merges, on `main`, and the controller rechecks every path of this plan at that rebase (revision note). Workspace: `/home/jblossey/projects/houserules/.superpowers/sdd/2026-10-25-batch-38/` (absolute in every dispatch).

**Revision 2** folds the 20 findings of the plan review (`plan-review.json`: 0 Critical, 12 Important, 8 Minor). The review held no Critical finding, so the plan-review loop ends. P18 corrected the tracker at once: `backlog/batches.json` no longer lists HR-217 under batch 38 (f918988b on `main`). P11's durable source exists: `coexistence-openspec-files/` with `coexistence-openspec-files.list` and `.sha256` (22 files, equal to the scratch repository's OpenSpec commit `c67feb6`).

**Revision notes during development** (a decision that makes a plan sentence false adds a note here in the same turn):
- design.md 278: no cap on parallel agents for batch 38. The heading "Order and parallelism (`process.parallel-on-disjoint-files`; at most three agents at once)" no longer holds its cap; the waves, the disjointness rule, and one-at-a-time integration hold. A wave's reviews may run beside the next wave's implementers when their files are disjoint.
- Rebase onto `main` (2026-10-26): batch 37 merged at b9dd0d0e; `batch-38` is rebased onto it with no conflict, so "rebased on `batch-37`" now reads "rebased on `main`". Revision 2's tracker fix now cites f918988b on `main`, the aggregated batch-37 commit that holds it (`backlog/batches.json` lists HR-209, HR-190, HR-212, HR-223, and HR-238 for batch 38). The controller rechecked every path and line anchor of this plan at the rebase (`rebase-path-recheck.txt`): every file exists and every anchor holds; `review/intent.rs` now has 1,826 lines, not 1,802.
- Task 6 review (finding 4, Minor): the durable test pins the swap (the successor in, the retired entry out), not the absolute standing count; "the standing count 25" in Task 6 Step 2 is the task's recorded run, not a permanent test. Finding 3 scopes the README check to the `## Stability` section and the issue form's surface-impact field: Task 10 keeps those two places free of a freeze statement, and a kept other meaning of the words is no red.
- design.md 280 (faster task loops): the Global constraints' Task gates line holds for the controller's integration run; a task's run and a fix round replace `cargo test --locked` with the tests of the files they change and skip the autocrlf suite (`houserules.task-gates-mirror-ci`); a fix round adds each finding's Check line. A reading the spec leaves open that a test would pin goes to the controller before the test is written. Ledger rows carry UTC times.
- Task 1 review: T1 keeps the existing `review::intent::resolve_named(root: &Path, commit: &str, named: &IntentRef) -> Result<Intent, String>` in place of `resolve_named_at`: no named path reads the adapter configuration, so a `config` argument would go unused; T4's `intent-resolves` calls it at `--head` and at the merge base. `found_in` carries `cfg_attr(not(test), expect(dead_code))` until T4 adds its caller, and T4 removes the attribute. T1 also changes `review/mod.rs` (`intent` becomes `pub(crate)`, for T2's `root_kind`), the unknown-source line pins of `tests/bundle.rs`, `tests/evidence.rs`, `tests/record.rs`, and `tests/review_config.rs`, the `--intent` help of `evidence`, `record write`, and `bundle write`, `README.md` (the source list), and the migrating-knowledge skill's source list. Spec 3.1 and 3.3 are amended from the Task 1 review, round 0. T2 starts from an interim base that holds T1's round-0 commits, because it consumes only `root_kind`, which no finding changes; the controller integrates T2's own commits after T1 closes.
- Task 2's brief gave it the three git spawns of `rules/check.rs` (the Code health section names Task 4): Task 2 routed them, so Task 4 routes only `rules/audit.rs` and `rules/trailers.rs`. Task 2 also adds `rules/render/openspec.rs` and a re-export in `rules/mod.rs`. Task 3 starts from an interim base that holds Task 1 complete and Task 2's round-0 commits; the restamp lines of `install/mod.rs` are now 39, 180, and 182.
- Task 8's file set (the controller's autocrlf run at the wave-4 base, the head after the eval record: `t8-autocrlf-before.txt`, 83 `LF will be replaced by CRLF` lines): the helpers that stage the warned paths in `tests/validate_stats_audit_parity.rs` (48 lines), `tests/several_installs.rs` (19), and the unit tests that stage `openspec/`, `a/`, `sub/`, and `top.txt` fixtures (16; their modules under `crates/houserules/src/`), the shared helpers of `crates/houserules/tests/tracker_support/mod.rs` and `tests/common/mod.rs` when a staging form lives there, `.gitattributes` if the paths need it, and the new `crates/houserules/tests/git_spawns.rs`. Task 9's files (`docs/openspec.md`, `docs/upgrading-to-2.md`, the migrating-knowledge skill and its root copy, a new `tests/openspec_docs.rs`) are disjoint from them, so Tasks 8 and 9 run in parallel after Tasks 4 and 5 integrate. Task 9 also carries Task 3's and Task 4's notes: the skill's `evidence@1.0.0` token and check-type list, the guide's `pack@1.4.0` and `overrides@1.3.0` rows, the hand step of a kept schema, and the example entries.
- design.md 283: batch 38 gains Task 11 (HR-260; spec 19, case 14): the faster loop in the reference workflow. It changes the implementer and task-reviewer templates, so "Eval run: no" no longer holds: the controller runs every scenario of `.claude/evals/` after Task 11's integration and before the branch review (`process.evals-rerun`). Task 11 starts beside Task 3 from the batch-38 head; its files are disjoint from every open task.
- design.md 285: batch 38 gains Task 13 after the branch review: the README rewrite from six first-time readers (findings and design in the batch workspace, `readme-rewrite-design.md`). It changes `README.md`, `crates/houserules/Cargo.toml` (`description` only), new pages under `docs/` for the moved reference, and the tests that read the README (`tests/readme_funnel.rs` and every test that reads a README section). Standard, not critical: it changes no command, flag, schema, or output. The six personas rerun on the result; the HR-212 drafts' README section names are rechecked.
- design.md 289: batch 38 gains Task 14 (critical, 173): the two first-run fixes of spec 20. Files: `crates/houserules/src/install/mod.rs` (the two `next:` lines and a helper), `crates/houserules/src/backlog/commands.rs` (`check_backlog`), their tests, and the upgrade guide's reds (`docs/upgrading-to-2.md`); disjoint from Task 13b except for a README block that prints `init`'s output, which the controller re-pins at integration.

## Architecture

- **The adapter.** `crates/houserules/src/review/intent/openspec.rs` (a submodule of `review/intent.rs`) reads an OpenSpec change from the tree at a given commit through `review::git` and runs no tool. `review/intent.rs` dispatches to it as a third `Adapter`. The root classifier is one function that the adapter (tree paths at a commit) and the render split and warning (tracked paths of the work tree) share.
- **Links.** One reader of the range's trailers (`Intent:` and the session keys), through `rules::trailers::parse`, serves `evidence`, `record write`, and the two checks; T3 widens `parse` to `pub(crate)`.
- **Checks.** The two check types live in a new `rules/intent_checks.rs`, beside `durable_sha.rs`; `check_shape.rs` and `check.rs` register their shapes and lines, and `rules/runner/mod.rs` dispatches them (its exhaustive `match check.kind`, lines 241 to 249).
- **No library target.** An integration test under `crates/houserules/tests/` reaches the binary's output only; a check of an internal function is a `#[cfg(test)]` unit test in its module.
- **No dependency is added** (spec 13). Patterns go through `regress`, YAML through `saphyr`, globs through `rules::glob::compile`, git through `rules::git::command` or `review::git`.

## Tier, critical tasks, models

**Tier: standard** (design.md 172(2)): a branch review closes the batch. **Critical tasks** (173): T1, T2, T3, T4, and T5. Each changes the CLI's output (T1's adapter lines and fields, T2's render section and warning, T5's `--validation` flags and lines), a schema (T3's published 1.1.0 documents and install schema 2.4.0, T4's knowledge schema), or an install's data on disk (T3's restamp). For each, the task's spec cases become tests first, committed with one recorded RED run at BASE, then made to pass; one task review on opus; a fix round that fixes a Critical or Important finding, or changes files beyond its findings' files, gets one fix-diff review in the re-review form (design.md 260, as `process.risk-tiers` ships it). T6 to T10 are standard: T6 changes knowledge entries and pointers, T7 changes tests only; T8 changes tests and may change `.gitattributes`; T9 and T10 change docs and their tests.

**Eval run:** no. No task changes the implementer or task-reviewer template or a scenario (`process.evals-rerun`); a task that finds it must stops and flags it.

**Implementer models (`process.implementer-model-per-task`, design.md 224, 225; HR-242):** T7 goes to haiku at level L1: batch 37's haiku task (T15, L1) closed with an Important finding, so the level stays L1. HR-248 fixes the exact result: one `pack_pin` helper in `tests/tracker_support/mod.rs`, six callers, no expectation change, and no task consumes it. Every other task goes to sonnet (L3 or L4). The ledger rows name the model, the level, and the reason; at batch close HR-242 gets the data point.

## Order and parallelism (`process.parallel-on-disjoint-files`; at most three agents at once)

| Wave | Tasks | Why |
|---|---|---|
| 1 | T1 ∥ T6 ∥ T7 | T1 the adapter (`review/intent*`, `install/marker/review.rs`); T6 the freeze (`knowledge/houserules.json`, the sweep's pointers); T7 HR-248 (seven test files); disjoint |
| 2 | T2 ∥ T3 | T2 render and warning (needs T1's root classifier; `rules/render/*`, `install/mod.rs`, which holds `InitWorkflow` (line 1226), `rules/check.rs` for the warning call only); T3 links, sessions, schemas 1.1.0, install 2.4.0, HR-251 (needs T1; `review/*`, `published/*`, `install/marker*`, `main.rs`, `rules/trailers.rs` for `parse`'s visibility); disjoint: the three module-doc lines of `install/mod.rs` that state the restamp (39, 178, 180) are a controller commit at T3's integration |
| 3 | T4 ∥ T5 | T4 the two checks (needs T1, T3; `rules/intent_checks.rs`, `check_shape.rs`, `check.rs`, `audit.rs`, `trailers.rs`, the knowledge schema); T5 the validation capture and the CI input (needs T1, T3; `review/evidence.rs`, `review/record/*`, both units); disjoint |
| 4 | T8 ∥ T9 | T8 HR-249 and HR-252's grep test (needs every code task); T9 `docs/openspec.md`, the guide, the skill (needs every command's text); parallel only after the controller writes T8's file set into a revision note (from the autocrlf run at the wave-4 BASE) and finds it disjoint from T9's; else T8, then T9 |
| 5 | T10 | The README, last (spec 18.1) |

`rules/check.rs` changes in T2 (one call that prints the 6.2 line) and T4, in different waves. `review/evidence.rs` and `review/record/*` change in T3 and T5, in different waves. `install/marker/review.rs` changes in T1 (`openspec`) and T3 (`review.sessions.trailers`), in different waves. `rules/trailers.rs` changes in T3 (visibility) and T4 (its spawns), in different waves. `review/intent.rs` changes in T1 and T3. `main.rs` changes in T3 only (the `update` help that names the stamp, lines 790 and 791): each command's flags live in its `clap::Args` struct. `knowledge/schema.json` and its template change in T4 only.

The controller owns at integration: `crates/houserules/payload.stamp`, `.houserules.json` (but T3 commits its restamp, P2), the root copies that `update --dir .` writes, `backlog/`, and `knowledge/` other than T4's schema and T6's two entries. Each task still commits its own stamp and root copies for its `check-commit` gate; the controller regenerates them on a cherry-pick conflict (`payload-stamp-gate -- --write`, `update --dir .`) and records it.

## Global constraints

- **Dependencies:** none added; a task that needs one stops and says why (spec 13).
- **Current docs:** verify every API of `regress`, `saphyr`, `globset`, clap's `Args`, git's trailer and notes commands, GitHub Actions and GitLab CI inputs (T5), and OpenSpec 1.15.0 behavior against the saved sources and the v1.15.0 source clone at `~/.cache/hr-b38/openspec-repo` (read as data); record each in `docs_verified` (`security-hygiene.verify-current-docs`). No task runs OpenSpec's binary (spec case 10); the expected values come from the saved outputs `openspec-1.15.0-outputs/` and the probe captures.
- **Exact text:** every user-facing line is the spec's exact text, copied from the spec file, pinned whole by a test (`quality.user-facing-text-is-contract`).
- **Untrusted text:** every value read from a change, a trailer, or a capture prints through `review::text`.
- **Git:** every git call of a source file under `crates/houserules/src/` (but `src/bin/`) that a task changes goes through `rules::git::command` or `review::git` (HR-252's part; spec 13); a task adds no direct `Command::new("git")` there. The dev binaries (`src/bin/*`) and the integration tests are exceptions: with no library target they cannot reach the runner, and `rules/git.rs` depends on `rules::git_paths`, so a `#[path]` include does not compile standalone; HR-252 keeps them with that reason. T7 is outside this constraint.
- **Determinism:** the same inputs give the same bytes; a test runs each changed command twice.
- **Hermetic git in tests:** `LC_ALL=C`, no injected configuration (`tracker_support::with_default_git`), explicit `core.abbrev` where text is pinned (`houserules.pinned-git-text-runs-hermetic`).
- **Windows:** path pins built component by component; platform gates with a comment naming the constraint; the autocrlf run before closing (the controller runs it when refused).
- **Template is the source:** edit `template/`, then `./target/debug/houserules update --dir .` and `cargo run --quiet --bin payload-stamp-gate -- --write` in the same commit; a pack's content change moves its version by class through `KIT_PACK_VERSIONS`.
- **Commits:** Conventional Commits, lines at most 100 characters, a `Work-Item:` trailer for each item a commit delivers, no co-author or tool line; `check-commit --from origin/main` after every commit.
- **Tree binary** for every gate, `update`, and `render`, with `HOUSERULES_SKIP_SELF_UPDATE=1`.
- **Task gates:** `cargo fmt --check`; `cargo clippy --all-targets --locked -- -D warnings`; `cargo test --locked`; the windows-gnu clippy; `cargo deny check`; shellcheck; `check-knowledge`, `check-backlog`, `render --check`; `residue-gate`, `vacuous-exception-gate`, `payload-stamp-gate`, `dist-generate-check`, `find-shell-tool-refs`; `gen-schemas -- --check`; `check-commit --from <merge base>`; CI's PR audit `mise run audit -- --base <merge base> --head HEAD`. A refused gate goes to `concerns`; the controller runs it at integration.
- **Grounding** (design.md 263): every brief carries the grounding line; no live CI proof is part of any task before the 2.0.0 release.

## Corpus runs (`houserules.plan-semantics-run-on-the-corpus`)

- The adapter's rules: `probe-openspec-corpus.txt`, `probe-openspec-adapter-rules.txt`, `probe-openspec-parity.txt` (30 active changes against OpenSpec 1.15.0's own outputs), `spec-review-probe-openspec-edges.txt`, `spec-review-r1-probe-openspec-edges.txt`, `spec-review-r1-probe-prose-deltas.txt`, `probe-commonmark-divergence.txt`.
- The issue rule of spec 9.1: `enumerate-issues.py` ran at plan time (`enumerate-issues-plan.txt`): 19 issues named, all open; 6 numbers that are no issue (`#2` to `#8` but `#7`); no open issue that no item names.
- Globs and `regress`: `spec-review-r1-probe-globset-regress.txt`. Coexistence: `probe-openspec-coexistence.txt`. Upgrade seeding: `spec-review-r1-probe-upgrade-seeding.txt`.

## Code health (`process.code-health-scan`; `plan-code-health.txt`)

Scanned the planned source files at the batch-38 head (`git ls-files` under `review/`, `published/`, `rules/render/`, `install/`: `plan-enum-src.txt`, 53 files; plus `rules/{model,check,audit,trailers,check_shape}.rs`, `install/marker.rs`). Found and planned:
- Direct git spawns in planned files (HR-252's part; per file in the review's `plan-review-probe-git-spawns.txt`): `install/mod.rs` 3 (T2), `rules/render/location.rs` 3 (T2), `rules/check.rs` 3 (T4), `rules/audit.rs` 1 (T4), `rules/trailers.rs` 2 (T4); every other planned source file holds none. Exceptions named with their reason (the Git constraint): `src/bin/gen-schemas.rs` 2, `src/bin/residue-gate.rs` 7, and the integration tests. The runner's own spawn is `rules/git.rs:36`; `review/git.rs`'s four are `#[cfg(test)]` helpers in a file no task changes. The crate holds 71 at the batch-37 head; HR-252 keeps the rest.
- History narration (HR-244): `rules/check.rs:366` and `:3111` (`used to fall through`), and `src/bin/residue-gate.rs` (T4 takes all three, so the item closes).
- Size: `rules/check.rs` 3,350 lines and `rules/audit.rs` 2,930; the two checks go to a new module (`rules/intent_checks.rs`) instead of growing them. `review/intent.rs` 1,802 lines; the adapter goes to its own submodule.
- No `TODO`, `FIXME`, or lasting `allow(dead_code)` in the planned files. To the tracker: none new.

## Technical debt (spec 14; `plan-deferred.json`, 22 open deferrals)

Folded: HR-251 (T3), HR-252's part (T2, T4, T8's grep test), HR-244 (T4), HR-248 (T7), HR-249 (T8). Left out, each with spec 14's reason: HR-231, HR-124, HR-250, HR-253, HR-254, HR-121, HR-164, HR-205, HR-206, HR-225, HR-233, HR-234, HR-240; HR-217 (HR-253, 267(4)); HR-236, HR-241, HR-243 close with batch 37.

---

### Task 1: the `openspec` intent adapter (critical)

Spec: 3 (all), 0 (the 4.9 reading at a commit); cases 1, 2. Backlog: HR-209.

**Files:** create `crates/houserules/src/review/intent/openspec.rs`, `crates/houserules/tests/openspec_intent.rs`, and fixtures under `crates/houserules/tests/fixtures/openspec/` (the vendored corpus subset of case 2 with OpenSpec's MIT notice and the source tag; the edge fixtures of case 1). Modify `review/intent.rs` (the third adapter, the named `openspec:<id>`), `install/marker/review.rs` (`ADAPTERS` gains `openspec`; the unknown-source line of 3.6), and their tests.

**Interfaces (produces):**
- `review::intent::openspec::RootKind { Absent, Planning, StorePointer }` and `root_kind(paths: &[String], config: Option<&str>) -> RootKind`, where `paths` lists the tree's files under `openspec/` relative to the install root and `config` is the text of `openspec/config.yaml`, else `config.yml` (3.1). T2 calls it with tracked paths.
- `review::intent::resolve_named_at(root: &Path, commit: &str, config: &IntentConfig, named: &IntentRef) -> Result<Intent, String>`: one named intent resolved at `commit` (an unresolved one carries the adapter's line). T4's `intent-resolves` calls it at `--head` and at the merge base.
- `review::intent::found_in(root: &Path, analysis: &Analysis, config: &IntentConfig, sources: &[Adapter]) -> Result<Vec<Intent>, String>`: the intents that the adapters of `sources` find from the changed paths of `analysis` (no named intent). T4's `intent-required` calls it with the `IntentConfig` that the marker reader gives at `--head` and the `Analysis` of `review::change::analyze`.

**Oracles of case 1, per field:** titles, task counts, and delta counts from the saved outputs (`openspec-1.15.0-outputs/`, `spec-review-probe-openspec-edges.txt`, `spec-review-r1-probe-openspec-edges.txt`); `why` from section 3's rule and the `markdown-parser.ts` lines that spec 2 quotes (OpenSpec's binary prints no why); the extglob and brace-range fixtures from 3.3's literal rule; the lines of 3.5 for the fixtures only houserules defines.

- [ ] **Step 1: Write the tests** of case 1 (every fixture it lists, through `evidence --json`, each line of 3.5 whole) and case 2 (the ten vendored changes against `openspec-1.15.0-outputs/` for the active ones and a rerun of `probe-openspec-adapter-rules.py`, corrected to section 3, for the archived ones; save that rerun as `t1-archived-expected.txt`). Unit tests for each pattern of 3.4, for `root_kind`, and for `found_in`. Record two RED runs at BASE: `cargo test --locked --test openspec_intent` (cases 1 and 2 failing at run time) and the unit-test run (the compile error of the new module); the report cites both.
- [ ] **Step 2: Implement** section 3; `regress` for the patterns, `saphyr` for the YAML files, `rules::glob::compile` for `generates`.
- [ ] **Step 3: Run the suite and every task gate; commit** `feat(intent): read an OpenSpec change as the intent of a change` with `Work-Item: HR-209`.

### Task 2: the split in `AGENTS.md` and the two-workflow warning (critical)

Spec: 6.1, 6.2 (the removal list stays with T9); cases 10, 11 (but the removal steps, T9). Backlog: HR-209. Needs T1.

**Files:** `rules/render/agents_md.rs` and `rules/render/mod.rs` (the section after `## Standing rules`, by tracked paths through T1's `root_kind`), `rules/render/location.rs` (its three spawns), `install/mod.rs` (`InitWorkflow`, line 1226: the 6.2 line in `init --workflow reference`; its three spawns; not the three module-doc lines of the restamp), `rules/check.rs` (one call that prints the 6.2 line in `check-knowledge`), the coexistence fixture under `crates/houserules/tests/fixtures/openspec-coexistence/` (vendored from the workspace's `coexistence-openspec-files/`, checked against its `.sha256`, with OpenSpec's MIT notice and the source tag v1.15.0), `crates/houserules/tests/render_targets.rs`, and a new `crates/houserules/tests/openspec_coexistence.rs`.

**Interfaces:** consumes T1's `root_kind`. Produces the 6.1 section in `render`'s output and the 6.2 line on stderr; T9's tests read both from the binary's output and the spec file.

- [ ] **Step 1: Write the tests** of cases 10 and 11 (the section byte for byte, its absence, the two out-of-date lines, the warning with the binary's version and the kept exit code, a store-pointer root with the warning and no section, `init --target claude` and `update` leaving the OpenSpec files byte-identical). Order in each scratch install: case 11's out-of-date lines run after the fixture is tracked and before `houserules render`; case 10's passing gates run after `render`. Record one RED run at BASE.
- [ ] **Step 2: Implement; run `update --dir .`; run the suite and every task gate; commit** `feat(render): name OpenSpec's role in AGENTS.md and warn on two planning workflows` with `Work-Item: HR-209`.

### Task 3: trailers, sessions, schema versions 1.1.0, install 2.4.0, and HR-251 (critical)

Spec: 5.1 to 5.4, 5.7, 5.8, 14 (HR-251); cases 4, 6, 13 (HR-251). Backlog: HR-190, HR-251. Needs T1.

**Files:** create `crates/houserules/src/review/links.rs`; modify `review/mod.rs` (declare it), `review/evidence.rs` (the `Sessions:` line, trailer intents), `review/record/*` (`links.sessions`, `sessions_more`, step 7 and the minimal record of 8.4), `review/intent.rs` (named intents from trailers: `resolve`'s `named` becomes `&[IntentRef]`), `review/bundle.rs` (its `resolve_one`, bundle.rs:413, passes a one-entry slice), `rules/trailers.rs` and `rules/mod.rs` (`parse` and its module become `pub(crate)`; T4 routes the file's spawns), `published/evidence.rs`, `published/change_record.rs`, `src/bin/gen-schemas.rs`, `schemas/evidence/1.1.0.json`, `schemas/change-record/1.1.0.json`, `schemas/published.json` (the 1.0.0 files stay byte-identical), `install/marker.rs` (`install@2.4.0`, the restamp of 2.2.0 and 2.3.0, HR-251's line), `install/marker/review.rs` (`review.sessions.trailers` and its line), `main.rs` (the `update` help, lines 790 and 791) and `tests/help_35.rs` (its pin, line 247), the root `.houserules.json` restamped by `update --dir .` with `tests/dogfood.rs` (line 367) at 2.4.0, `tests/evidence.rs`, `tests/record.rs`, `tests/markers.rs`, `tests/ci_command.rs` (HR-251), `tests/gen_schemas.rs`, `tests/published_schemas.rs`. The stamp sites come from `git grep -n 'install@2\.3\.0'`, classified in the report: an assertion of the stamp a binary writes and a doc of the current contract move to 2.4.0; history (`docs/specs/`, `docs/plans/`, the batch-37 section of `docs/upgrading-to-2.md`) and a fixture that models a 2.3.0 marker stay. The three module-doc lines of `install/mod.rs` (39, 178, 180) are T2's file: the controller updates them at T3's integration.

**Readers of 1.0.0 records.** This repository's record job writes 1.0.0 notes from batch 37's merge on. `links.sessions` reads as empty and `links.sessions_more` as absent when a note lacks them (`#[serde(default)]`, as `work_items_more`); a test reads a 1.0.0 note fixture written by batch 37's code with `record show`, `record list`, and `record audit`.

**Paper walk** (HR-251 and `review.sessions.trailers` tighten the marker check that every command runs; `process.gates-cover-generated-commits`): enumerate the writers of `defaultBranch` in each released kit (`git grep` over the release tags) and in `update`; a fresh install; an install of each released kit under the new binary; an install whose marker holds an empty `defaultBranch` (what `update` does, the red, its remedy); each author class. The report states each accepted red.

**Interfaces (produces):** `review::links::read(root: &Path, base: &str, head: &str, keys: &[String]) -> Result<Links, String>` with `Links { intents: Vec<TrailerIntent>, sessions: Vec<Session> }`, `TrailerIntent { commit: String, value: String, named: Result<IntentRef, String> }`, `Session { commit: String, trailer: String, value: String }` (trailers by `rules::trailers::parse`, keys case-insensitive, commit order). T4 and T5 consume it.

- [ ] **Step 1: Write the tests** of cases 4 and 6 and HR-251's part of case 13; the record's `evidence` without `sessions`; the 1.0.0 note; the restamp lines; the `published.json` rows of both 1.1.0 files beside the unchanged 1.0.0 rows. Record one RED run at BASE.
- [ ] **Step 2: Implement; regenerate the schemas with `cargo run --quiet --bin gen-schemas -- --record` (a plain run writes no manifest row, and `--check` compares only generated files); run `update --dir .`; run the suite and every task gate; commit** `feat(evidence): link commits to specs and sessions by trailers` with `Work-Item: HR-190` and `Work-Item: HR-251` (HR-251 may take its own `fix` commit).

### Task 4: the checks `intent-required` and `intent-resolves` (critical)

Spec: 5.5, 5.6 (the check columns), 5.7 (the knowledge schema), 14 (HR-244, HR-252's part); cases 5, 13 (HR-244). Backlog: HR-190, HR-244, HR-252. Needs T1, T3.

**Files:** create `crates/houserules/src/rules/intent_checks.rs`; modify `rules/mod.rs` (declare it), `rules/runner/mod.rs` (the dispatch of the two types), `rules/check_shape.rs` (the two types and their fields), `rules/check.rs` (the lines of 5.5; HR-244's two comments; its three spawns), `rules/audit.rs` (evaluation; its spawn), `rules/trailers.rs` (its two spawns), `src/bin/residue-gate.rs` (HR-244's third comment; its seven spawns are a named exception of the Git constraint), `knowledge/schema.json` and `template/knowledge/schema.json` (and the root copy through `update`), `install/conversion/d3.rs` or the released-content table it reads, as the walk below requires, `tests/check_shapes_35.rs`, `tests/trailers.rs`, and a new `tests/intent_checks.rs`.

**Interfaces:** consumes T1's `resolve_named_at` and `found_in`, T3's `review::links::read` and `pub(crate)` `rules::trailers::parse`, the marker reader's `IntentConfig` at `--head`, and `review::change::analyze`.

**Paper walk** (a new check type; `process.gates-cover-generated-commits`): a fresh install; an install of each released kit under the new binary (the schema reaches it as released content of D3: walk `install/conversion/d3.rs` and state how an unmodified schema of each release is recognized and replaced, and how a modified one gets its hand step); an install with a kept or overridden schema; and each author class of the 5.6 table. The report states each accepted red.

- [ ] **Step 1: Write the tests** of case 5 and each `check-knowledge` line of 5.5 and 5.7; HR-244's grep. Record one RED run at BASE.
- [ ] **Step 2: Implement; run `update --dir .`; run the suite and every task gate; commit** `feat(check): add the intent-required and intent-resolves check types` with `Work-Item: HR-190`, and the HR-244 and HR-252 commits with their own trailers.

### Task 5: the validation capture and the units' `validation` input (critical)

Spec: 4 (all); case 3. Backlog: HR-209. Needs T1, T3.

**Files:** `review/evidence.rs` and its `Args` (`--validation`), `review/record/mod.rs` and `record write`'s `Args`, a new `review/validation.rs` (the capture reader and the 4.1 lines; `review/mod.rs` declares it), `.github/workflows/houserules-review.yml` and `ci/gitlab-review.yml` (the `validation` input of 4.3), `review/ci/forms.rs` (binds `validation` to its empty default in the GitHub and GitLab inline forms; an unbound input panics, forms.rs:341 and 383), `tests/ci_command.rs` (the inline forms), `crates/houserules/tests/ci_support/mod.rs`, `tests/ci_units.rs`, `tests/ci_units_shell/gates.rs` and `record.rs` (the units' scripts), `tests/evidence.rs`, `tests/record.rs`, `tests/check_parity.rs` if a flag changes its table.

- [ ] **Step 1: Write the tests** of case 3, each refusal line of 4.1 whole, the item match of 4.2 (a capability with the change's id), and the unit cases through `ci_support` (with the input set and the file present; absent, with the notice). Record one RED run at BASE.
- [ ] **Step 2: Implement** with `capture.rs`'s header reader and `serde_json`; pin every new action or image by sha or digest (`houserules.actions-pinned-by-sha`).
- [ ] **Step 3: Run the suite and every task gate; commit** `feat(evidence): read an OpenSpec validation capture bound to the head` with `Work-Item: HR-209`.

### Task 6: the freeze retires (HR-238)

Spec: 7; case 7. Backlog: HR-238.

**Files:** `knowledge/houserules.json` (the entry retired, the successor of 7.2 added; then `houserules archive` and `render`), `knowledge/archive/` as `archive` writes it, the rendered files, every live pointer the sweep classifies as one (among them `.github/ISSUE_TEMPLATE/2-change.yml:90` and `README.md:1103`; skills and templates under `template/` with their root copies; docs; code comments), a new `crates/houserules/tests/freeze_successor_38.rs` (case 7), and the workspace file `sweep-triage.md`. The 1.x conversion fixture (`crates/houserules/tests/fixtures/conversion/**`) is a fixture: kept.

- [ ] **Step 1:** capture the sweep of 7.3 (`git grep -n -i -e 1-0-surface-is-frozen -e freeze -e frozen`, unfiltered) as `t6-sweep.txt`; classify every hit in `sweep-triage.md`.
- [ ] **Step 2:** write the tests of case 7 (the retired entry, the standing successor with its exact summary, the standing count 25, no live pointer); record the failing run; change the entries and pointers; run `archive`, `render`, `update --dir .`.
- [ ] **Step 3: Run the suite and every task gate; commit** `docs(knowledge): retire the 1.x surface freeze for its successor` with `Work-Item: HR-238`.

### Task 7: one pack-pin helper (HR-248; haiku, L1)

Backlog: HR-248. **Files:** `crates/houserules/tests/tracker_support/mod.rs` (a `pack_pin` helper), `tests/check_packs.rs`, `tests/pack_moves.rs`, `tests/kit_citations.rs`, `tests/every_commit_entry.rs`, `tests/help_36.rs`, `tests/pack.rs`. `init_install.rs`'s `canonical_sha256` stays (its doc requires an independent sort).

- [ ] **Step 1:** add `pack_pin(pack: &serde_json::Value) -> String` (clone, sort all objects, serialize, `sha256_hex`) with a unit-style test in one caller; replace each copy; no expectation changes. `git grep -n 'sort_all_objects' -- crates/houserules/tests` lists only the helper.
- [ ] **Step 2: Run the suite and every task gate; commit** `test: pin every pack copy through one helper` with `Work-Item: HR-248`.

### Task 8: the autocrlf warnings and HR-252's grep test

Spec: 14 (HR-249, HR-252's test); case 13 (those parts). Backlog: HR-249, HR-252. Needs T1 to T5.

**Files:** declared before dispatch in a revision note: the controller runs the autocrlf suite at the wave-4 BASE (`t8-autocrlf-before.txt`) and lists the test helpers that stage the warned paths, and `.gitattributes` if needed; plus a new `crates/houserules/tests/git_spawns.rs`. It greps the source files of HR-252's part (`install/mod.rs`, `rules/render/location.rs`, `rules/check.rs`, `rules/audit.rs`, `rules/trailers.rs`, and the new files of T1, T3, and T5) for a direct `Command::new("git")` and finds none; its doc names the exceptions of the Git constraint with their reason and names no commit.

- [ ] **Step 1:** pin the warning count at 0 in a test that runs a staging helper under `core.autocrlf=true`; record it failing; fix the helpers (`.gitattributes` or the staging form, whichever the warning paths show); the autocrlf suite prints no `LF will be replaced by CRLF` line.
- [ ] **Step 2: Run the suite and every task gate; commit** `test: stage fixtures without line-ending warnings` with `Work-Item: HR-249`, and the grep test with `Work-Item: HR-252`.

### Task 9: `docs/openspec.md`, the upgrade guide, and the skill

Spec: 4.1 (the command), 5.5 (the examples), 6 (the page and the block), 6.2 (the removal list), 18.1 (the guide's batch-38 section and meta-spec 7's accepted reds); cases 8 (but the README), 11 (the removal steps). Backlog: HR-209, HR-190. Needs T1 to T5.

**Files:** create `docs/openspec.md`; modify `docs/upgrading-to-2.md`, `template/.claude/skills/migrating-knowledge/SKILL.md` and its root copy, `crates/houserules/tests/workflow_texts.rs` or a new `tests/openspec_docs.rs`.

- [ ] **Step 1: Write the tests:** the hookup block parses with `saphyr` and each command it names is a subcommand of the binary; the page names 1.15.0, the capture command of 4.1, both example entries (each passes `check-knowledge` in a scratch install), the store note, and every file of the reference workflow with the `workflow` key; following the removal steps as printed in a scratch install, a second `update` writes no file of the workflow back and the warning stops; the guide's batch-38 section names each red of 10.4. The tests read the 6.1 section and the 6.2 line from the binary's output in a scratch install and from the spec file. Record one RED run.
- [ ] **Step 2: Write the texts; run `update --dir .`; run the suite and every task gate; commit** `docs: document the OpenSpec hookup and the batch-38 upgrade` with both trailers.

### Task 10: the README (HR-223; last)

Spec: 8; case 8 (the README holds the block). Backlog: HR-223. Needs every other task.

**Files:** `README.md`, the README tests in `crates/houserules/tests/` (`workflow_texts.rs` and `help_*` as they pin it), and the workspace file `research-readme.md` (each primary source read, with its date).

- [ ] **Step 1:** read the sources of 8.1 and add each to `research-readme.md`; write the tests: the first screen's statements, the funnel order, the CLI reference against `--help`, the integration section with the block of section 6 (byte-equal to `docs/openspec.md`'s), each runnable block run as printed in a fresh install; record the failing run.
- [ ] **Step 2: Rewrite; run the suite and every task gate; commit** `docs: rewrite the README as an adopter funnel` with `Work-Item: HR-223`.

### Task 11: the faster loop in the reference workflow (HR-260; standard, not critical)

Spec: 19 (all), case 14. Backlog: HR-260. Needs nothing open.

**Files:** `template/workflows/reference/.claude/skills/orchestrating/SKILL.md`, `template/workflows/reference/.claude/skills/planning/SKILL.md`, `template/workflows/reference/.claude/agents/implementer.md`, `template/workflows/reference/.claude/agents/task-reviewer.md` (the targeted-test rule for its own runs, if it names the full suite), `template/packs/process.json`, `template/packs/quality.json`, `crates/houserules/src/packs.rs` (`KIT_PACK_VERSIONS` only), the root copies through `update --dir .`, `knowledge/overrides.json` (the duplicated fields dropped), and the kit-text tests (`tests/workflow_texts.rs`, `tests/pack_texts.rs`, the pack pins of `tests/check_packs.rs` and `tests/pack_moves.rs`). Disjoint from T3 (review/*, published/*, install/marker*, main.rs), T4, and T5; T9 later adds the upgrade guide's lines for it.

- [ ] **Step 1: Write the tests** of case 14: each rule of 19.1 to 19.8 as a whole sentence of its kit file (the task's wording), each pack's version move, `update` on an install of each released kit writing the new texts; record one RED run.
- [ ] **Step 2: Write the texts; move the pack versions by class through `KIT_PACK_VERSIONS`; run `update --dir .` and `payload-stamp-gate -- --write`; drop the overrides that the kit now carries; run the gates of design.md 280; commit** `docs: ship the faster task loop in the reference workflow` with `Work-Item: HR-260`. The eval run is the controller's.

## Controller steps

1. **Now (plan time):** `enumerate-issues.py` ran (`enumerate-issues-plan.txt`). Before T10: draft `issue-comments.md`, one comment per issue of the enumeration (spec 9.2), rerunning the enumeration at the batch head first.
2. **Before wave 2:** done at plan review: `coexistence-openspec-files/` (P11). **At T3's integration:** the three module-doc lines of `install/mod.rs` move to the 2.4.0 restamp. **After each integration:** the gates, the refused runs (autocrlf, the hook shellcheck), the commit map, the worktree removal.
3. **After T9's integration** (the last task that changes `src/` or `template/` before the README): the upgrade proof (spec 10, case 9) by a dispatched implementer with a live-run brief, in `~/.cache/hr-b38/upgrade/`: each release binary downloaded from this repository's releases and checked against its `.sha256`; the capture of 10.3 per release and variant.
4. **After T9's integration:** the live run of case 12 by a dispatched implementer, in a scratch install with a local bare remote and the branch binary. When the branch review's fix wave changes `src/` or `template/`, the controller reruns cases 9 and 12 at the new head before the owner's README review.
5. **After T10, the batch close:** `stats` over five workspaces; `efficiency.json`; the branch review (`branch-reviewer`, fable) and its fix wave; the knowledge commit; `set <id> status=done batch=38` for HR-209, HR-190, HR-238, HR-223, HR-212 (its drafts), HR-244, HR-248, HR-249, HR-251; HR-252 stays open with its remaining files; HR-217 is out of the batch (267(4); `batches.json` fixed at plan review); HR-242's data point; the plan's revision notes; `release-note.md` (18.1).
6. **Finish and release readiness (spec 18; design.md 276, 277):** one to five clean commits; the bundle; the PR; green CI on three operating systems; the owner's README review on the PR (each requested change applied and its examples rerun); the ff-only merge; the post-merge checks of 18.3; the report of 18.4. Release PR #83 stays with the owner.

## Spec acceptance cases → tasks

| Case | Task | Case | Task |
|---|---|---|---|
| 1, 2 | T1 | 8 | T9 (block), T10 (README) |
| 3 | T5 | 9 | controller step 3 |
| 4 | T3 | 10 | T2 |
| 5 | T4 | 11 | T2, T9 (removal) |
| 6 | T3 | 12 | controller step 4 |
| 7 | T6 | 13 | T3 (HR-251), T4 (HR-244), T7 (HR-248), T8 (HR-249, HR-252) |
