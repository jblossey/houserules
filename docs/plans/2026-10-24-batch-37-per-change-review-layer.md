# Batch 37 plan: the per-change review layer

Each pull or merge request gets one report, and each merge gets one record. `evidence` writes the reviewer packet, with the uncovered surface first and the intent of the change in it. `route` decides who must review, with a tool-free floor. `bundle` checks the request's evidence before review. `record` keeps a git note per merge. The published documents carry versioned, generated schemas. Two published CI units and `houserules ci` wire all of this into GitHub or GitLab without editing an adopter's CI file.

> **For agentic workers:** dispatch each task through the `implementer` template; review through `task-reviewer`. Steps use checkbox (`- [ ]`) syntax.

**Revision 2** folds the 26 findings of the plan review (`plan-review.json`: 1 Critical, 16 Important, 9 Minor) and the owner's rulings 251 (the CI units at the root `ci/`, QP-1) and 252 (Zlib allowed, QP-2). Two findings corrected the spec under the review rule: 10.2's `issues: read` is static on `gates` and `record` (P13), and 10.8's `setup` installs the tree binary (P12). The Critical finding sent revision 2 to a fresh plan review.

**Revision 3** folds the 15 findings of the second plan review (`plan-review-r1.json`: 1 Critical, 6 Important, 8 Minor; round-1 P2 and P8 partial) and the owner's ruling 253 (the GitHub unit takes its version as an input, QP-3 and QP-4). The Critical finding sent revision 3 to a fresh plan review.

**Revision 4** folds the 11 findings of the third plan review (`plan-review-r2.json`: 0 Critical, 3 Important, 8 Minor; round-2 P34 and round-1 P2 partial, carried in P45). The review held no Critical finding, so the plan-review loop ends, as in batches 35 and 36. P42 corrected spec 10.2 and 10.6 under the review rule: `version` is needed only for a `release` install, and the minimum check runs only for `release` (minimum `2.0.0`).

**Revision notes during development** (HR-241 (5): a decision that makes a plan sentence false adds a note here in the same turn):
- T16 (task-16-review.json issue 6): Task 16's Files line also holds `crates/houserules/tests/pack_texts.rs` (one `CORE_READINGS` row: the core-text scan flags "deferral" in the verbatim schema description), and its commit subject names five defects, because 11.6 belongs to Task 10. A later task that edits `CORE_READINGS` rebases on its length.
- T16 (task-16-review.json issue 2): the eval briefs of controller step 5 run the suite with `mise run test`, the task runner's test task that brings the tool pins; the task gate `cargo test --locked` of the Global constraints stays the task-end gate.
- T1 (task-1-review.json issue 2): the entry schema of `rule-package` does not require `standing` (spec 9.2 amended).
- T2 (task-2-review.json issue 7): `compare_self_audit` compares the JSON of the typed result with the report's copy and sets the verdicts on the typed rows. Task 2 also changes `crates/houserules/Cargo.toml` and `Cargo.lock`: `indexmap` takes its `serde` feature, which the typed `area_files` map needs; no crate enters the lockfile (controller ruling inside spec 12). Its Files line also holds `tests/gen_schemas.rs`, `tests/validate_36.rs`, `tests/fixtures/stats-35/old-form-expected/audit.json`, and the module doc of `rules/mod.rs`. `rules::audit::audit` returns a result with empty `references`; `cmd_audit` adds the verdict (T4 and T6 read the judged result through `AuditSource`).
- T3 (task-3-review.json issue 1): score thresholds (`ownerScore`, `seniorScore`) are non-negative numbers with no upper bound, as spec 6.1 lists the faults. Reversed by the T6 review (issue 3): a score is a number from 0 to 1 (5.1), so both thresholds are too (spec 6.1 amended); T6's fix round changes the check in `install/marker/review.rs`.
- T4 (task-4-review.json issues 1 to 4, 6; design.md 255): `--attr-source` needs git 2.41, not 2.40 (spec 4.2 and case 2 amended; the Git global constraint, the T4 interface and tests, and the T11 image pin say 2.41). A CI file of 4.1 is a changed rule source when it is one at either end of the range. Spec 4.2's argv adds `--diff-algorithm=myers`, `--inter-hunk-context=0`, and `-c core.attributesFile=`, every other setting that changes the parsed text is pinned with a test, every other `git diff` of the analysis carries the options that apply (`--no-relative` included), and a local `.git/info/attributes` is a named input. An empty-tree range audits every path of `--head` (spec 4.1 amended). T4 also changes `rules/{mod,audit,check_shape,model,git_paths}.rs` and `rules/runner/external.rs` (the in-process audit result, the covering globs, and a visibility change). One function names the seeded workflow file: the controller removes T4's copy at integration, after T13.
- T4 (task-4-review-r1.json): T4 also changes `rules/git.rs` (one git runner and one commit resolver for the audit and the review modules, the empty-tree range) and splits `audit_range` out of `rules/audit.rs`. The diff argv of `review/diff.rs` holds pins beyond the options that spec 4.2 lists (`-l1000`, `--indent-heuristic`, `--ignore-submodules=none`, `--submodule=short`, `-c core.bigFileThreshold=512m`, `--no-replace-objects`, and the environment of `review::git::command`), each from the 74-row enumeration with its test, under 4.2's "every other git setting ... pinned in the same way". `audit` on the CLI refuses git's empty tree as a base: T9's `record write` audits a root push in process, and T11's `record` job runs no `audit --json` on a root push (spec 10.1 amended).
- T5 (task-5-review.json issues 1, 2, 4): spec 4.9 carries the adapter's lines and the named work item with no tracker. `review::intent::resolve` returns `Result<Vec<Intent>, IntentError>`, with `IntentError::Named(String)` (the adapter line of the named intent, which T8 prints after `intent <source>:<ref>: `; T7 exits 2 on an `--intent` whose source is no adapter and on `Failed`, and prints any other `Named` line in the intent's block, as 5.1 and 5.3 say) and `IntentError::Failed(String)` (a marker, tracker, or git failure; the caller exits 2 with it). `resolve` takes the config that `Marker::review` checked and repeats none of its checks.
- T6 (task-6-review.json; design.md 257): spec 6.1 to 6.3 amended: `check-knowledge` fails a `default` or `warn` route of `auto-merge-eligible` and a score threshold outside 0 to 1; a path entry at `auto-merge-eligible` keeps its two limits, with two lines; the `default` reason needs no changed rule source either; a limit line rounds the share up and the limit down; `--score` outside 0 to 1 exits 2 with its line (route and evidence). Spec 6.4 stands as written: one reason object for every rule that was evaluated, one that found nothing included. T6 also changes `install/marker/review.rs`. Each command task (T6, T7, T8, T12, T9) makes its own submodule `pub(crate)` in `review/mod.rs`, and each task that registers a schema (T6, T7, T9) also edits `tests/gen_schemas.rs` and `tests/published_schemas.rs`.
- T7 (task-7-review.json): spec 5.1, 5.9, 8.2, and case 1 amended: the `unresolved` key of a named intent that does not resolve, in the order of 5.8; the `empty` key of an empty path; `rules.packs` from the marker at `--head`, read from git, each `{name, version, sha256}`; the `--score` line with no threshold; the 3,000-id case with long ids. T7 changes T4's `published/change.rs`, `review/change.rs`, `review/diff.rs`, and the `changed` helper of `review/route.rs`'s tests (`EmptyChange`). `review::evidence::build` also takes `packs: Vec<PackPin>`, and T9's `record write` passes it. T7's fix round replaces T5's `IntentError`: `review::intent::resolve` returns `Result<Vec<Intent>, String>` from one call; a named intent that does not resolve is its entry in adapter order, then ref, with `unresolved` holding the adapter's line; `Err` is a failure that is not the named intent's own, and the caller exits 2 with it. T8's `bundle check` prints the `unresolved` line of that entry after `intent <source>:<ref>: `, and every `Intent` literal of T8 and T9 names `unresolved`. The T5 note's `IntentError` sentence is superseded.
- T11 (task-11-review.json; design.md 258, 259): spec 10.1 amended. The install key maps each character outside `A-Za-z0-9_-` to `-`; T12's `ci print` and `ci write` compute it by that rule (one function, shared with the tests of the units) and refuse two installs that share a key with the line of 10.1. A force push to the default branch writes no record, with the notice of 10.1. Both units build a `default-branch` floor binary with no file of the request's tree in reach (working directory and every ancestor of the source outside the request checkout, the toolchain pinned), and the GitLab jobs inherit no `default:` keyword that runs a command, a service, or a cache; `ci_support` asserts both, and T12's inline forms reuse those assertions. T12's production key function replaces the test helper `ci_support::install_key`, and the unit tests call it. Spec 7.1 states the fence rule of the units' `hr_intent`: T8's `bundle check` reads sections by it, with one test that feeds the same descriptions to `hr_intent` and to `bundle check`. A force push's `record-push` job pushes nothing (spec 10.1).
- T14 (task-14-review.json issue 2): spec 7.4 gives the request template's section with placeholders, byte for byte; T14's template and T12's `ci write --template` and `ci print --template` copy it, and their tests compare with the block read from the spec file.
- T13 (task-13-review.json; design.md 256): spec 10.7 amended. `init` prints its line only when the install holds no seeded workflow after the run; the no-host text is the literal `github or gitlab`; a needed scope with a value other than `read` gets a replace step. T13 also creates `install/ci_files.rs` and `install/ci_workflow/reader.rs` and adds three hand steps to the guide. `arraydeque` 0.5.1 enters under `saphyr` (design.md 256). `knowledge/houserules.json` entry `houserules.seeded-repo-live-proof` goes to T17's carry-overs.
- T8 (task-8-review.json issues 1 and 5): spec 7.3 amended: a tracker that cannot be reached gives `bundle check: <n> work items not resolved: <the tracker's line>`, exit 0 for this part (4.9), and the exit-2 lines on stderr carry the `bundle check: ` prefix too. The produced interfaces differ from the Interfaces block of Task 8: `Bundle.intent` is `Option<String>` (the text after `Intent:`, which T9 parses with `IntentRef::parse`); `BundleRun { header_line }` with `header() -> Option<Header>`; `sections` gives the zero-based line number of each heading. The fix round also edits `hr_intent` in both units (T11's files): its awk runs under `LC_ALL=C`, so `[[:space:]]` is the ASCII set of `bundle check`.
- T12 (task-12-review.json issues 1 and 2): spec 10.3 amended: the closing line of `ci print` goes to stderr, and the GitHub forms' push trigger names the default branch resolved as `durable-sha` resolves it (by name), or exit 2 with a named line. `rules/durable_sha.rs` gives the name resolution to `ci` (one resolver, not a copy).
- design.md 260: a critical task's fix round that fixes a Critical or Important finding, or changes files beyond its findings' files, gets one task-reviewer review of its fix diff (`task-<N>-review-r1.json`, new breakage only), from the T12 fix round on. The Tier paragraph's "one review" holds for the original findings.
- design.md 261: Task 17's Files line also holds the bodies of `process.risk-tiers` and `process.bounded-review-loop` in `template/packs/process.json`, copied from the two overrides of `knowledge/overrides.json`; the controller drops those overrides at T17's integration after `houserules update --dir .`.
- design.md 262 and 263: controller step 7 is done (Pages enabled with the Actions source). Step 9 runs no live CI proof before the 2.0.0 release: neither the GitHub proof (case 13) nor the GitLab proof (case 14, Q3); HR-253 holds both, with the live items of the T11 fix round. The PR's own CI still gates the merge. Q4: ff-only from the CLI after green CI, no release. Every brief carries the grounding line of 263(3).
- T9 (task-9-review.json issues 4 and 5): spec 8.2, 8.4, and 10.4 amended. The minimal record cuts each intent ref to 150 bytes and keeps `rules` without `packs`, so it fits `maxBytes` less 4,096 at every cap; a byte cap counts a string without its quotes; step 7 also cuts the evidence's `work_items`; the reviewers of 10.4 stay within 3,072 bytes (GitLab usernames may hold emoji), which the GitLab `record-push` jq program enforces with `utf8bytelength` (T11's unit; the T9 fix round edits it). The T9 fix round also gives `notes::read` a range-membership fallback for a cut or minimal record (spec 8.1).
- T17 (report concern 1): spec 7.4's summary of `process.evidence-before-review` had 225 characters, beyond the schemas' 160-character cap; spec 7.4 now gives the 155-character summary that T17 ships, and Task 17's step "the seed rule's summary verbatim" reads it.
- T10 (task-10-review.json issue 2): spec 8.3 amended: in a shallow clone, a range of `record list` or `stats --records` whose end the clone lacks, or whose walk reaches a shallow commit, is exit 2 with a named line; `bad range` stays for a range that git cannot parse.
- T10 (task-10-review-r1.json new_breakage[0]): spec 8.3 states the shallow-range rule by the symmetric difference of the two ends, a missing full object name, and a relative revision whose base resolves; an abbreviation that does not resolve stays `bad range`. A third T10 round closes by the controller's check (design.md 260(3)).
- T17 (task-17-review-r1.json new_breakage[0]): spec 10.3 amended: the first line of a `ci write` file and the `update` line name `ci write --force` with the flags that wrote the file, and the update line names the `--sha` of the new release for a GitHub reference file. T17's second fix round changes `CI_WRITTEN_BY_LINE` (install/marker/review.rs), the update line (install/ci_files.rs), the `update` help, and their pins, beyond its Files line.
- Live run (live-run-report.json, concerns 1 and 2): spec 6.3's area lines show the name as a code span (4.8, as the binary prints it), and spec 10.4 states the push with no merged request: `record` writes its note without `--reviewers-pending`. A live-run fix task changes the GitLab unit's `record` job and its shell test.
- T18 review (task-18-review.json assessment): the GitHub record job read a failed listing of the commit's pull requests, a failed request read, or a failed reviews read as no request or no approvers. Spec 10.1 now gives `--reviewers-pending` and a notice for each; task 19 changes the GitHub unit.
- T19 review (task-19-review.json issue 0): spec 8.2 gives `reviewers_pending: true` both meanings (a later job has not set the reviewers; the approvers could not be read) and the `record show` line; T19's fix round changes `record show`, the `--reviewers-pending` help, and the change-record schema description with its Rust doc comments, beyond its Files line.
- Batch close (controller step 8): the plan's 17 tasks grew to 20: T18 and T19 from the live run and its review (the GitLab and GitHub record jobs, spec 10.1, 10.4, 8.2) and T20, the branch-review fix wave. Each task's Files line was exceeded where its review notes above say so; no other Files sentence holds a path the tree lacks. Of the Global constraints, the `expect(dead_code` grep printed nothing after wave 9; every other constraint held. Controller steps 7 and 9 changed by design.md 262 and 263 (Pages enabled; no live CI proof; Q3 and Q4 answered).
- T1 and T15 reviews: both reports left out CI's PR audit gate. Every later brief names it in its first-act block with the command and the rule for a red from BASE.

**Spec:** `docs/specs/2026-10-24-batch-37-per-change-review-layer.md` (revision 6, approved under design.md 248; section 12 amended by design.md 249 and 250; the unit paths and 10.2 by design.md 251 and 253; 10.2 and 10.8 by the first plan review). Each task reads the spec sections it names; a brief carries them by pointer (`process.brief-carries-the-spec`). Branch: `batch-37`. Workspace: `/home/jblossey/projects/houserules/.superpowers/sdd/2026-10-24-batch-37/` (absolute in every dispatch).

**Architecture:**
- **Published types.** `crate::published` holds every type that a published document names (`Serialize`, `Deserialize`, `schemars::JsonSchema`): the documents, the intent contract, and the change kinds and line ranges of spec 5.9. It depends only on `serde`, `serde_json`, `schemars`, and `indexmap`, so `src/bin/gen-schemas.rs` includes it with `#[path = "../published/mod.rs"]`; the crate has no library target, and `gen-goldens` includes `dev_support/cargo_target.rs` the same way. Every item of `crate::published` is used by the main binary; a value that only the generator reads (the Pages base URL, the generator settings) lives in `gen-schemas.rs`.
- **Review modules.** `crate::review` holds the change analysis (spec 4), the intent adapters, and one submodule per command (`route`, `evidence`, `bundle`, `record`, `ci`). T1 adds `mod review;` to `main.rs` and creates `review/mod.rs` with every submodule declared (`change`, `diff`, `rules_hash`, `text`, `git`, `intent`, `route`, `evidence`, `bundle`, `ci`, `record`), each file a stub that holds only its module doc, so no later task edits `review/mod.rs` and T4 does not edit `main.rs`. Each command defines its arguments as a `clap::Args` struct in its own module, so `main.rs` gains one variant and one match arm per command.
- **Config.** The `.houserules.json` `review` key is read in `install/marker/review.rs`, beside `marker/runners.rs`. That module also holds the two constants that several tasks read: `ADAPTERS` (the intent source names) and `CI_WRITTEN_BY_LINE` (`ci write`'s first line, spec 10.3).
- **CI units.** The GitHub unit's one source is `.github/workflows/houserules-review.yml`, with a required `version` input and a hand-kept minimum version (design.md 253); the GitLab unit's is `ci/gitlab-review.yml` at the repository root, with a release-bumped version line (design.md 251). The binary embeds both with `include_bytes!`, outside `template/` and the kit payload.

**Tech stack:** Rust (the `houserules` crate), `serde_json`, `globset`, `sha2` through `crate::baseline::hash`, git through `rules::git`. New dependencies by owner ruling: `schemars` with `preserve_order` and `indexmap2` (design.md 237; T1), `saphyr` (design.md 238; T13), `base64` 0.23.1 (design.md 250; T12), and the dev-dependency `jsonschema` without its default features (design.md 249; T1). The Zlib license of `foldhash`, which `jsonschema` and `saphyr` bring, is allowed (design.md 252; T1). Cargo only.

**Tier: standard** (design.md 172(2)): one task review, at most one re-review for a non-critical task; a branch review. **Critical tasks** (173): T1, T2, T3, T6 to T13, T16, and T17. Each changes the CLI's commands, flags, or output, a schema, security code (T11's token handling), or an install's data on disk (T16: the deliverables schema; T17: the process pack). For each critical task, the task's spec cases become tests first, committed with one recorded RED run at BASE, then made to pass. Reading of `process.risk-tiers` for this plan: when a task's tests need a dependency that the task adds (T1, T12, T13), the RED base is the task's own dependency commit, and the controller names that commit in the task review's dispatch. The evidence is that RED run and the whole suite green at HEAD. A critical task gets one task review on opus and never a re-review. The controller verifies each fix by re-running the reviewer's reproduction (`process.fix-round-verification-record`). A Critical or Important finding goes to a fresh implementer; the controller fixes a Minor one. The RED run records every new test: each test of new behavior fails, and each pin of unchanged behavior passes and is recorded as a pin with its reason. T4, T5, T14, and T15 are standard. T4 and T5 are internal modules that no CLI output reads until T6. T14 is this repository's CI. T15 changes tests only. **Eval run:** yes, because T16 changes the task-reviewer template (`process.evals-rerun`; spec case 11). **Plan review:** fresh reviewers until a round holds no Critical finding, as in batches 35 and 36.

**Implementer models (`process.implementer-model-per-task`, design.md 224, 225):**
- **T15 goes to haiku at level L1.** The spec fixes its exact result: each inline digest becomes a call to `tracker_support::sha256_hex`, and no test expectation changes. It touches five files, and no other task consumes an interface from it.
- **Every other task goes to sonnet.** Each one either consumes or produces an interface (L3) or is critical (L4).

The ledger row of T15's dispatch names the model, the level, and that reason. Its close line states the findings by severity, the fix rounds, the re-reviews, and the tokens and tool uses. At batch close, HR-242 gets the data point.

## Order and parallelism (`process.parallel-on-disjoint-files`; at most three agents at once)

| Wave | Tasks | Why |
|---|---|---|
| 1 | T1 ∥ T15 ∥ T16 | T1 adds `schemars` and `crate::published`; T15 touches five test files; T16 touches templates, skills, and the deliverables schema; disjoint files |
| 2 | T2 ∥ T3 | T2 types the audit result (needs T1); T3 adds `covers`, the `review` key, and install schema 2.3.0 (needs T1's `Route`); disjoint files (T3's `main.rs` edit is the `update` help; T2 touches no `main.rs` line) |
| 3 | T4 ∥ T13 | T4 the change analysis (needs T2, T3; no `main.rs` edit, T1 declared `review`); T13 `init`, `update`, and HR-243 with `saphyr` (needs T3's `CI_WRITTEN_BY_LINE`; its `main.rs` edit is the help that 10.7 makes false); both delete T3's `expect(dead_code)` on `CI_WRITTEN_BY_LINE`, an identical hunk that merges cleanly |
| 4 | T5 ∥ T6 ∥ T11 | T5 the intent adapters and T6 `route` fill their own stubs (both need T4; their attribute deletions follow the global constraint); T11 the CI units (needs T13's `saphyr` for its parse tests) |
| 5 | T7 ∥ T14 | T7 `evidence` (needs T5, T6); T14 this repository's wiring (needs T11, T1's `gen-schemas`); disjoint files |
| 6 | T8 | `bundle` (needs T4, T5; `main.rs`) |
| 7 | T12 | `ci` (needs T3, T11; `main.rs`) |
| 8 | T9 | `record write` and `record show` (needs T6, T7, T8; `main.rs`) |
| 9 | T10 ∥ T17 | T10 `record list`, `record audit`, `stats --records`, HR-241 (6) (needs T9; its text test in a new `tests/workflow_texts_37.rs`); T17 the kit texts and the wording gate (needs every command's text; `tests/workflow_texts.rs`); disjoint files |

A task may start before its wave when every task it needs has integrated and its files are disjoint from every running task's files (as in batch 36). The tasks that change `main.rs` never start early.

`crates/houserules/src/main.rs` changes in T1 (`mod published;`, `mod review;`), T3 (the `update` help's install schema), T13 (the help that 10.7 makes false), T6, T7, T8, T12, T9, and T10, never in two tasks of one wave. Two tests enumerate the CLI surface and move in each command task: the count test `no_subcommand_carries_a_long_about_beyond_its_short_one` (each task sets the count at its BASE plus one, in the planned order T6 25, T7 26, T8 27, T12 28, T9 29, and updates the count in its doc comment) and `the_cli_surface_names_exactly_these_24_subcommands`, to which each task adds its subcommand names in `--help` order. T6 renames that test to `the_cli_surface_names_exactly_these_subcommands` and drops the count from its doc, so no later task renames it. `crates/houserules/tests/check_parity.rs` changes in the same command tasks (each new subcommand with `--dir` in `ROOT_RESOLVING_COMMANDS`, resolving its root through `crate::root::resolve_root`; `bundle write`, which spec 7.2 gives no `--dir`, is named in the doc comment of `NOT_ROOT_RESOLVING`, T8). `crates/houserules/src/published/mod.rs` changes in T1, T2, T4, T6, T7, and T9, and `src/bin/gen-schemas.rs` and `schemas/published.json` in T1, T2, T6, T7, and T9, each in a different wave. `Cargo.toml` and `Cargo.lock` change in T1, T13, and T12, each in a different wave; these three tasks never start early, so no two of them run at once.

The controller owns these files at integration: `crates/houserules/payload.stamp`, `.houserules.json`, the root copies that `update --dir .` writes, `backlog/`, and `knowledge/` other than `knowledge/schema.json` (T3 changes the template and runs `update --dir .`) and T14's edit of `houserules.task-gates-mirror-ci` (its parity test needs it in the same commit). Each task still commits its own stamp and root copies, because its `check-commit` gate needs them. When the cherry-pick of a later task of a wave conflicts on them, the controller re-runs `cargo run --quiet --bin payload-stamp-gate -- --write` and `./target/debug/houserules update --dir .` on that commit and amends it.

Each parallel implementer works in its own worktree from the same BASE, writes scratch only under `<scratchpad>/task-<N>/`, and does not touch the files the controller regenerates. The implementer's first act runs `git merge-base --is-ancestor <BASE> HEAD`; when that fails, it runs `git reset --hard <BASE>` and records it in `concerns` (HR-241 (2), applied from wave 1 on). The controller integrates by cherry-pick, one task at a time. It runs the gates after each, writes the old-to-new commit map to the ledger, and removes the task's worktree and branch after integration.

## Cost measurement (172(6), `process.batches-measure-cost`)

The ledger records each dispatch's start, close, tokens, and tool uses. `efficiency-windows.json` holds one UTC window per phase: spec, plan, each wave with its reviews and fix rounds, evals, live run, the GitLab and GitHub proofs, and the branch review. `efficiency.json` comes from `python3 -I .superpowers/sdd/2026-10-10-batch-33/efficiency.py <workspace>` before the branch review, after every window is closed. Design review: four rounds (`spec-review.json` to `spec-review-r3.json`).

## Global constraints

- **Dependencies.** Use cargo for every package operation.
  - T1 adds `schemars` with `cargo add schemars@=<version> --features preserve_order,indexmap2` and `jsonschema` with `cargo add --dev jsonschema@=<version> --no-default-features` (no HTTP resolution: the tests read local files). T1 adds `"Zlib"` to `deny.toml`'s allow list with a comment that names `foldhash` and the crates that bring it, in the form of the MPL-2.0 comment (design.md 252).
  - T13 adds `saphyr` with `cargo add saphyr@=<version>`.
  - T12 adds `base64` with `cargo add base64@=0.23.1` (design.md 250; `Cargo.lock` also holds 0.22.1).
  - Each such task vets its crate and every crate that the `Cargo.lock` diff adds. It lists them with `git diff <BASE> -- Cargo.lock | grep '^+name'` and records each in `dependency_vetting`, a lock-only crate with its repository and issue-response record (`security-hygiene.dependency-vetting`, `security-hygiene.exact-pins`).
  - No other direct dependency. A task that needs another stops and says why.
- **Current docs.** Verify every API of `schemars`, `saphyr`, `jsonschema`, `base64`, clap's `Args` derive and flattening, git's `--attr-source` and `notes merge`, GitHub Actions (reusable workflows, `permissions`, job outputs, `concurrency`, `::add-mask::`, Pages deployment), GitLab CI (`spec:inputs`, `include:remote` with `integrity` and `inputs`, `environment` with `action: access`, dotenv reports, `needs` with `artifacts`), and release-please `extra-files` against current docs. Record each in `docs_verified` (`security-hygiene.verify-current-docs`). Spec 2's "Assumed, verified by the plan" facts are each verified by the task that relies on them, and the report names the source.
- **Git 2.41.** T4 verifies, from the current runner-image documentation, that the git of each CI runner (`ubuntu-latest`, `macos-latest`, `windows-latest`) is 2.41 or later, and records it. If one is older, T4 stops and flags it before any code. T11 pins the GitLab unit's default `image` by digest to one whose git is 2.41 or later, and records the version.
- **No dead code, no lasting allowance.** Every task's clippy gate passes at its own HEAD.
  - An item that a later task first calls carries `#[cfg_attr(not(test), expect(dead_code, reason = "called from T<n>"))]` and a unit test that uses it. rustc also reports a field that is never read, so a struct whose fields several tasks read keeps its attribute until its last reader.
  - Each task deletes every attribute that its change leaves unmet: an `expect` that is no longer met fails the build, so the clippy gate names each one, and none survives the batch. These deletions are the one edit a task may make outside its Files line, in `src/published/**`, `src/review/**`, and `src/install/marker/review.rs`, and the report lists each. When two tasks of one wave delete attributes in one file, the controller integrates both: an identical or a distant hunk merges cleanly, and an adjacent one is a mechanical conflict that the controller resolves and records in the ledger.
  - An item that its own task already reads (T3's lint reads `ADAPTERS`; T3's config parse reads `Route`) carries no attribute.
  - The one lasting allowance is `gen-schemas.rs`'s include of `crate::published`, which carries `#[allow(dead_code, reason = "...")]` with the reason that the module is shared with the main binary, which uses the rest. An item of `crate::published` that only `gen-schemas` reads moves into `gen-schemas.rs`.
  - After T10 and T17 integrate, `grep -rn 'expect(dead_code' crates/houserules/src` prints nothing; the controller records it (controller step 8).
- **Exact text.** Every user-facing line is the spec's exact text, and a test pins each line byte for byte (`quality.user-facing-text-is-contract`). Copy each line from the spec file; never retype it. User-facing texts say "pull or merge request" (spec 3).
- **Determinism.** Every command of this batch writes the same bytes for the same inputs: no time, no environment value, and no absolute path in an output (spec 5.8). A test runs each command twice and compares the bytes.
- **Untrusted text.** Every value from the repository, a tool, or a request description prints through `review::text` (spec 4.8; T4). No task writes its own escaping.
- **Gates of spec 13.** A task that adds or tightens a check or a gate of spec 13's tables (T3, T8, T11, T13, T14) walks the rows of its gate and states each accepted red in its report (`process.gates-cover-generated-commits`).
- **Goldens.** A task that changes a pinned golden regenerates only that golden. Run `cargo run --quiet --bin gen-goldens`, keep the owned golden's diff, and `git checkout` every other file it rewrote. The report shows that the owned golden's diff is only the spec's change (for T1 and T2: only the `schema` line, spec 9.2).
- **Commits.** Use Conventional Commits, header and body lines at most 100 characters, no co-author or tool-attribution line, and a `Work-Item:` trailer for each item the commit delivers. A `feat` or `fix` commit under `crates/houserules/**` proposes a release; CI and test-only changes use `ci`, `test`, `refactor`, or `chore` (`houserules.commit-type-under-the-crate-path-decides-a-release`). Run `./target/debug/houserules check-commit --from origin/main` after every commit (the hook skips it).
- **Tree binary.** Use the tree binary (`cargo build`, then `./target/debug/houserules` with `HOUSERULES_SKIP_SELF_UPDATE=1`) for every gate and for `update` and `render` (`houserules.controller-gates-use-the-tree-binary`). The deliverables schema's constraints do not change in this batch, so every task's `validate` and `check-report-claims` run with the branch binary.
- **Template is the source.** Edit in `template/`, then run `./target/debug/houserules update --dir .` and `cargo run --quiet --bin payload-stamp-gate -- --write` in the same commit (`houserules.template-is-the-source`). A kit pack's content change moves its version by class, once per change, through `KIT_PACK_VERSIONS` and its checksum test (`packs.rs`); never type a hash by hand.
- **Hermetic git in tests.** A test that pins text git prints runs git with `LC_ALL=C`, no injected configuration, and an explicit `core.abbrev` (`houserules.pinned-git-text-runs-hermetic`). A scratch repository copies `user.name` and `user.email` into its own config; no test reads a global git identity. A submodule fixture passes `-c protocol.file.allow=always`.
- **Windows.**
  - A test that compares a printed path builds it component by component (`houserules.path-pins-mirror-the-code`).
  - A test that reads back a file git checked out first commits a `.gitattributes` of `* -text`.
  - Gate tests to their platform, each gate with a comment that names the constraint (`houserules.platform-gated-tests`):
    - Signals, symlinks, and read-only directories: `#[cfg(unix)]`.
    - A file name that holds a newline, a tab, or non-UTF-8 bytes: `#[cfg(target_os = "linux")]`, or seed the name through git's index (`seed_undecodable_tracked_path` in `rules/check.rs`).
  - A child process in a test runs through `sh`.
  - Before closing, run the suite once with `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=true cargo test --locked --no-fail-fast` and once plainly. When the harness refuses the autocrlf run, name it in `concerns`, and the controller runs it at integration.
- **Where tests live.** The crate has no library target. A test of an internal type or function is a unit test in its module (`#[cfg(test)]`). A test of a command runs the built binary from `crates/houserules/tests/`. A test that runs `jq` is `#[cfg(unix)]`, with a comment: the units run `jq` on Linux images only. No test changes process-wide state (`std::env::set_var` and the like); a test that needs another `PATH` sets it on the child process.
- **RED runs.** A critical task's RED run shows each new test failing and each pin passing. A task that adds a dependency its tests need commits the dependency first, and that commit is the RED base. A new unit test whose RED is a compile error records the error, and the same capture runs the pins in their own test targets.
- **Reading houserules output.** Write it to the workspace and read the file.
- **Scratch builds.** Put `CARGO_TARGET_DIR` of any scratch clone under `~/.cache` (`houserules.clones-need-their-own-cargo-target`).
- **Task gates:**
  - `cargo fmt --check`
  - `cargo clippy --all-targets --locked -- -D warnings`
  - `cargo test --locked`
  - `cargo clippy --target x86_64-pc-windows-gnu --all-targets --locked -- -D warnings`
  - `cargo deny check`
  - shellcheck
  - `check-knowledge`, `check-backlog`, `render --check`
  - `residue-gate`, `vacuous-exception-gate`, `payload-stamp-gate`, `dist-generate-check`, `find-shell-tool-refs`
  - from T1 on: `cargo run --quiet --bin gen-schemas -- --check`
  - `check-commit --from <merge base>`
  - CI's PR audit: `mise run audit -- --base <merge base> --head HEAD` (`houserules.task-gates-mirror-ci`)
- **Style.** Write docs, comments, commit messages, and reports in ASD-STE100 style. Comments state current constraints only (`writing-style.code-comments`; `houserules.comments-name-no-review-history`).

## Corpus runs (`houserules.plan-semantics-run-on-the-corpus`)

- The change kinds and the hunk forms (spec 4.2): `probe-hunks.txt`, `spec-review-probe-git.txt`, `spec-review-r1-probe-delhunk.txt`, `spec-review-r3-probe-gitattributes.txt`. T4's fixtures reproduce each kind.
- Coverage (spec 4.4, design.md 229): `spec-review-probe-coverage.txt` (91 of 116 files by glob membership; 0 under `covers`).
- The rules hash (spec 4.5): `spec-review-r3-probe-ruleshash.txt`, `spec-review-r1-probe-lstree.txt`.
- The sample (spec 8.5): `spec-review-probe-sample.txt`.
- The minimal record (spec 8.4): `spec-review-r2-probe-minimal.txt`, `spec-review-r3-probe-minimal.txt`.
- The `file` intent adapter (spec 4.9) over this repository's specs and plans (31 plans with this one): `plan-probe-intent-files.txt` (`plan-probe-intent-files.py`, which prints each `why`'s first 24 characters).
  - Every file has a `# ` heading.
  - The first paragraph after a plan's heading varies: a `Spec:` line in 19 plans, the agentic-workers blockquote in 5 (batches 31 and 33 to 36), a `Written` line in 4, a `**Goal:**` line in 1 (batch 32's audit follow-ups), a superseded status in 1, and this plan's goal paragraph, which it puts first so batch 37's bundle can name it with `Intent: file:`.
  - No plan of this repository ticks its checkboxes: `tasks` runs from 0 of 0 (19 plans) to 0 of 45.
- The "pull request" wording (spec 21.10): `spec-review-r2-probe-hostwords.txt`. T17's grep runs over the same tree at its HEAD.

## Review focus

Five inputs that the spec implies and no case names, each pinned by a test in the owning task:

1. A request description with CRLF line ends (a GitHub body edited in a browser): `## Evidence bundle\r` is the heading, each line parses without its `\r`, and a `houserules-run` header line compares without the `\r` (T8 test `a_crlf_description_parses_like_lf`).
2. A changed file whose name holds a newline or a tab: the `-z --numstat` entry and the `-U0` text pair up on the same path, and the packet prints the name as a JSON string in a code span (T4 test `a_path_with_a_newline_pairs_its_hunks`, `#[cfg(target_os = "linux")]`).
3. A text file with no LF at its end: the `\ No newline at end of file` marker counts as no line, on the old and on the new side (T4 test `the_no_newline_marker_is_no_line`).
4. A configured tracker that cannot be reached while `evidence` runs: the `work-item` intents get `title: null`, the packet prints, and the command exits 0 (T5 test `an_unreachable_tracker_gives_a_null_title`).
5. A range whose head is an ancestor of its base (the merge base is the head): no changed path; `route` gives `review.route.default` or `owner-review` with the `default -> <route>` reason, and the packet prints `None.` (T6 test `a_head_behind_its_base_routes_by_default`; T7 test `a_head_behind_its_base_prints_none`).

## Code-health scan (`process.code-health-scan`)

Measured at plan time (`codehealth-measure.txt`): `rules/audit.rs` 2818 lines (tests from line 713), `rules/read.rs` 980, `rules/runner/*.rs` 2612 in nine files, `rules/check.rs` 3255, `rules/check_marker_keys.rs` 204, `rules/stats.rs` 1998, `rules/git.rs` 531, `install/mod.rs` 2209, `install/marker.rs` 1823, `install/ci_workflow.rs` 1255, `install/hand_steps.rs` 370, `capture.rs` 876, `main.rs` 1836, `packs.rs` 1646. Read too: the templates and skills of spec 7.5, 7.6, and 11, and `.github/workflows/ci.yml`.

- **Primitive obsession** in `audit`, `for`, and the runners: each builds its output as `serde_json::Value` by hand, with string keys at every call site. T1 and T2 move them to typed documents (spec 9.2), which the batch needs anyway for the generated schemas.
- **Text-matched YAML** in `install/ci_workflow.rs` (`job_permissions`): it reads a job's `permissions` by a text test, which a re-indented or renamed job defeats (HR-243). T13 replaces the reader with `saphyr` (spec 10.7).
- **Host hard-wiring**: `install/mod.rs` seeds a CI file only when the origin holds `github.com` (`origin_is_github_hosted`, spec 2). T13 removes the seeding (spec 10.7).
- **Large files that grow**: `main.rs` and `rules/stats.rs`.
  - Each new command keeps its arguments in its own module, so `main.rs` grows by a variant and an arm.
  - `stats --records` goes to a new `rules/review_coverage.rs`, so `stats.rs` gains only the call.
  - `marker.rs` gains its new keys in `marker/review.rs`, as `marker/runners.rs` does.
- **The in-crate schema validator** (`rules/check.rs`, `validate`) does not know `anyOf`, `oneOf`, or `const`. The tests of the published schemas use `jsonschema` (design.md 249), and `check-knowledge` keeps its validator.
- **Duplicate digests** in five test files (HR-236): T15.
- **Out of scope:**
  - HR-244's three history comments (`rules/check.rs` lines 355 and 3016, `bin/residue-gate.rs` line 338) sit in functions that T3 does not change: T3 edits `required_field_findings` (line 182) and `check_fields` (line 37). HR-244 stays out (spec 15).
  - HR-225, HR-231, HR-164, HR-217, HR-121, HR-124, HR-205, HR-206, HR-233, HR-234, HR-240: spec 15 gives each reason. `deferred-plan.json` is the plan-time `list --deferred`.

---

### Task 1: published document types, the `rule-package` and `capture` schemas, `gen-schemas` (critical)

Spec: 9.1 to 9.4, 9.2 (the `for` half); case 6 (the `for` and `capture` parts, the generator, and the manifest). Backlog: HR-187.

**Files:**
- `crates/houserules/Cargo.toml`, `Cargo.lock`, `deny.toml` (Zlib, design.md 252).
- Create:
  - `crates/houserules/src/published/{mod.rs,rule_package.rs,capture.rs,route.rs}`
  - `crates/houserules/src/bin/gen-schemas.rs`
  - `schemas/rule-package/1.0.0.json`, `schemas/capture/1.0.0.json`, `schemas/published.json`
  - `crates/houserules/tests/schema_support/mod.rs`, `crates/houserules/tests/published_schemas.rs`, and `crates/houserules/tests/gen_schemas.rs` (the tests that run the `gen-schemas` binary, apart, because `CARGO_BIN_EXE_gen-schemas` does not exist at the RED base)
  - `crates/houserules/src/review/mod.rs` and its stubs `src/review/{change,diff,rules_hash,text,git,intent,route,evidence,bundle,ci}.rs` and `src/review/record/mod.rs`, each holding only its module doc (Architecture)
- Modify:
  - `src/main.rs` (`mod published;`, `mod review;`)
  - `src/capture.rs` (`Header` moves to `published::capture`; `capture.rs` re-exports it)
  - `src/rules/read.rs` (`for_result` returns the typed `RulePackage`)
  - `tests/goldens/read-parity/` (only the `schema` line)

`schemas/` is at the repository root (the Pages site's `schemas/`, spec 9.6). `crates/houserules/schemas/`, which holds the read schemas `overrides-1.json` and `pack-1.json`, stays as it is.

**Interfaces (produces):**
- `trait Published: Serialize + JsonSchema { const NAME: &'static str; const VERSION: &'static str; }`.
- `fn id<T: Published>() -> String`, which gives `houserules/<name>@<version>`.
- In `gen-schemas.rs` only: `const BASE_URL: &str = "https://jblossey.github.io/houserules/schemas"`, `fn settings() -> schemars::generate::SchemaSettings` (`draft2020_12()`, `for_serialize()`), and `fn schema_for<T: Published>() -> serde_json::Value`, which sets `$id` to `<BASE_URL>/<name>/<version>.json` and describes the `schema` key as a `const` of `id::<T>()`. The tests validate documents against the committed files, and `gen-schemas --check` proves the files equal the generated ones.
- `published::rule_package::RulePackage`. A full entry stays a `serde_json::Value` whose schema requires `id`, `kind`, `area`, and `summary`, describes `standing` as a boolean, and allows every other key (spec 9.2 as amended by the T1 review).
- `published::capture::Header`, unchanged in field order.
- `published::route::Route`: `AutoMergeEligible`, `OwnerReview`, `SeniorReview`, serialized `auto-merge-eligible`, `owner-review`, `senior-review`. It is `Ord` in that order and has `fn as_str(self) -> &'static str`. T3 and T6 consume it.
- `gen-schemas`: `cargo run --quiet --bin gen-schemas` writes every registered schema file and never `schemas/published.json`; `-- --check` exits 1 with one line per file that differs or is missing. `-- --record` adds a manifest row (path and SHA-256) for each schema file that has none. It replaces the row of a listed file only when no release tag holds that file (`git tag --list 'v*'`, then `git cat-file -e <tag>:schemas/<path>` for each): within this batch, a fix that changes a 1.0.0 schema that an earlier task introduced re-records its row, and after a release the immutability test fails a changed published file (spec 9.4, 13). The registry is one list in `gen-schemas.rs` that later tasks append to; each task that adds a document runs `--record`.
- Test helper `schema_support::assert_valid(document: &serde_json::Value, schema_file: &str)` (repository-relative path), which validates with `jsonschema`. Later tasks' tests use it.

- [ ] **Step 1: Vet and add `schemars` and `jsonschema`, and allow Zlib** (global constraints). Commit `build: add schemars and jsonschema` with `Work-Item: HR-187`; this commit is the RED base.
- [ ] **Step 2: Write the tests** in `tests/published_schemas.rs`:
  - `for` and `for --full` print `schema` as their first key, and their output validates.
  - A capture header validates.
  - Each committed schema has `$id`, the 2020-12 `$schema`, and the `schema` const.
  - The manifest fails a changed, a missing, and an unlisted file under `schemas/` (three scratch copies).
  - `for --full` keeps each entry's key order (the `spec-review-r1-probe-keyorder.txt` pairs): a pin of unchanged behavior, which passes at the RED base.
  - In `tests/gen_schemas.rs`: `gen-schemas --check` passes at HEAD; `gen-schemas --record` refuses to change the row of a file that a release tag holds.

  Record one RED run at the dependency commit: each new-behavior test of `published_schemas.rs` fails on its own, the key-order pin passes, and `gen_schemas.rs`'s RED is its compile error.
- [ ] **Step 3: Implement** the types, the generator, and the manifest. Regenerate the read-parity goldens per the global constraint, and show that each changed file gains only the `schema` line.
- [ ] **Step 4: Prove `--check` bites** (disclosed mutation): rename a field of `RulePackage` in a scratch copy, run `gen-schemas --check`, record the failure, and revert.
- [ ] **Step 5: Run the suite and every task gate; commit** `feat(schemas): publish the rule-package and capture schemas` with `Work-Item: HR-187`.

### Task 2: the typed audit result (critical)

Spec: 9.1, 9.2 (the `audit` half); case 6 (the audit part). Backlog: HR-187. Needs T1.

**Files:**
- Create `src/published/audit_result.rs` and `schemas/audit-result/1.0.0.json`.
- Modify:
  - `src/published/mod.rs`, `src/bin/gen-schemas.rs`, `schemas/published.json`
  - `src/rules/audit.rs`: the module doc says why the result type is not the deliverables type (judged rows hold `open`).
  - `src/rules/runner/*.rs` (each runner returns a typed row)
  - `src/rules/runner/report_field.rs` (`compare_self_audit` compares typed rows)
  - `tests/goldens/audit/` and the `crates/houserules/tests/fixtures/runner-pins/` pins, each gaining only the `schema` line
  - `crates/houserules/tests/published_schemas.rs`

**Interfaces (produces):** `published::audit_result::{AuditResult, AuditRow, Summary}` with `Serialize`, `Deserialize`, and `JsonSchema`, holding every key that `audit` emits today, in today's order: the task derives the list from `rules/audit.rs` and `rules/runner/*.rs` and records it, an optional key as an `Option` with `skip_serializing_if` (a row's `sanctioned`, `rules/audit.rs` lines 431 and 440, which T6's route rule 2 reads). `rules::audit::audit` returns `AuditOutcome { result: AuditResult, .. }`, and `runner::run` returns `Result<AuditRow, String>`. T4 reads an `--audit` file into `AuditResult` and runs `audit` in process.

- [ ] **Step 1: Write the tests**:
  - `audit` stdout and `--json` print `schema` first and validate.
  - A `--report` run and a `--sanctioned` run validate.
  - A unit test: a result read back with `serde_json::from_str::<AuditResult>` serializes to the same bytes, a `--sanctioned` row with its `sanctioned` key included.
  - The goldens and the runner pins are pins of unchanged behavior, apart from the `schema` line.

  Record one RED run at BASE: the integration tests fail, the unit test's RED is its compile error, and the same capture runs the pins in their own targets (global constraints).
- [ ] **Step 2: Implement.** No behavior change beyond the `schema` key. Regenerate the audit goldens and the runner pins per the global constraint.
- [ ] **Step 3: Run the suite and every task gate; commit** `feat(audit): type the audit result and publish its schema` with `Work-Item: HR-187`.

### Task 3: `covers`, the `review` key, and install schema 2.3.0 (critical)

Spec: 4.4 (the `covers` field and its lint), 6.1, 8.4 (`maxBytes`), 10.7 (`reviewLayerOffered`); case 7; the schema parts of case 9. Backlog: HR-186, HR-185, HR-192, HR-245. Needs T1 (`Route`).

**Files:**
- Modify:
  - `template/knowledge/schema.json` and its root copy (`covers`, optional boolean, default `false`, in `$defs.check`)
  - `src/rules/check_shape.rs` (`CheckDef` gains `covers` in the form of `covers_ids`, and `full_sample` gains the key, so the schema-pin test `assert_object_pinned::<CheckDef>` holds)
  - `src/rules/check.rs` (`required_field_findings`: `covers` on a `commits` or `report-field` check is one line)
  - `src/rules/check_marker_keys.rs` (the `review` lint of spec 6.1, one line per fault, each naming the key)
  - `src/install/marker.rs` (`INSTALL_SCHEMA` `houserules/install@2.3.0`, `PREVIOUS_INSTALL_SCHEMA` 2.2.0)
  - every other pin of `install@2.2.0`, listed at plan time by `grep -rln 'install@2.2.0' crates/houserules` (`plan-r2-install-pins.txt`): `src/install/mod.rs`, `src/install/pack_moves.rs`, `src/main.rs` (the `update` help), `src/pack/opt_in.rs`, and the tests `conversion.rs`, `dogfood.rs`, `help_35.rs`, `init_install.rs`, `marker_35.rs`, `runner_check.rs`, `tracker_json.rs`. The task re-runs the grep at its BASE and records the list; each pin of the current schema moves to 2.3.0, and each pin that names a past schema on purpose stays, with the reason in the report.
- Create `src/install/marker/review.rs` and `crates/houserules/tests/review_config.rs`.

**Interfaces (produces):**
- `install::marker::review::ReviewConfig { diff_exclude, route: RouteConfig, bundle: BundleConfig, intent: IntentConfig, ci_files: Vec<String>, max_bytes: u64 }`. Each sub-struct holds spec 6.1's keys. Defaults: `intent.sources` `["work-item"]`, `max_bytes` 65,536.
- `Marker::review(&self) -> Result<ReviewConfig, Vec<String>>`, whose errors are spec 6.1's lines.
- `Marker::add_ci_file(&mut self, path: &str) -> bool`, which returns `true` when the path was added.
- `Marker::review_layer_offered(&self) -> bool` and `Marker::set_review_layer_offered(&mut self)`.
- `rules::check_shape::CheckDef.covers: Option<bool>`, read as `false` when absent (the form of `covers_ids`).
- `install::marker::review::ADAPTERS: &[&str] = &["work-item", "file"]`, the intent sources that the lint accepts. T5 implements one adapter per name, and its test fails a name with no adapter.
- `install::marker::review::CI_WRITTEN_BY_LINE: &str = "# Written by houserules ci write; rewrite it with houserules ci write --force."` (spec 10.3, copied from the spec file). T4 (the rule sources and the rules hash at a commit), T13 (`update`'s pin line in the work tree), and T12 (`ci write`) read it; no other copy exists.

- [ ] **Step 1: Write the tests** of case 7 (each broken key of 6.1, an unknown intent source, `covers` on a `commits` check, `maxBytes: 100`, each with its line) and the schema half of case 9:
  - A 2.2.0 install updates to 2.3.0.
  - A knowledge schema copy gains `covers`.
  - A marker with no `review` key reads the defaults.

  Walk spec 13's `check-knowledge` row. Record one RED run at BASE.
- [ ] **Step 2: Implement; run `update --dir .`; run the suite and every task gate; commit** `feat(config): declare covering checks and the review key` with the four trailers.

### Task 4: the change analysis (standard)

Spec: 4.1 to 4.8; the analysis parts of cases 1, 2, 5 (the rules hash in a subdirectory install, `review.ci.files` at its own commit, the git version line); review focus 2 and 3. Backlog: HR-185, HR-186, HR-192. Needs T2, T3.

**Files:**
- Fill T1's stubs `src/review/{change,diff,rules_hash,text,git}.rs`.
- Create:
  - `src/published/intent.rs` (the contract types of 4.9) and `src/published/change.rs` (`ChangeKind`, `LineRange`, and `UncoveredPath`, in the serialized forms of 5.9); neither is a document of its own, so no file under `schemas/`
- Modify `src/published/mod.rs`, and `src/install/marker/review.rs` (it deletes the `expect(dead_code)` of `CI_WRITTEN_BY_LINE`; T13 deletes the same line in the same wave).

The tests are unit tests in the new modules (global constraints).

**Interfaces (produces):**
- `published::change::ChangeKind` (`Text`, `Deleted`, `Renamed`, `Empty`, `Binary`, `Mode`, `Symlink`, `Submodule`, serialized in lower case).
- `published::change::LineRange` (`Lines { start, end }` serialized `{"start": s, "end": e}`, `DeletedAfter { after, deleted }` serialized `{"after": n, "deleted": k}`).
- `published::change::UncoveredPath { path, old_path: Option<String>, kind: ChangeKind, ranges: Vec<LineRange> }`, the element of 5.9's `uncovered.paths`.
- `review::change::ChangedPath { path, old_path: Option<String>, kind: ChangeKind, added: u64, deleted: u64, ranges: Vec<LineRange>, excluded: bool }`.
- `review::change::Analysis { install: String, base: String, head: String, merge_base: Option<String>, paths: Vec<ChangedPath>, diff: DiffSize, rules: RulesHashes, work_items: Vec<String>, areas: IndexMap<String, Vec<String>>, no_area: Vec<String>, rule_sources_changed: Vec<String> }`. `DiffSize` is `{ files, added, deleted, lines, excluded }`, and `RulesHashes` is `{ merge_base_hash: Option<String>, merge_base_rules: present | absent, head_hash }`.
- `review::change::analyze(root: &Path, base: &Base, config: &ReviewConfig, base_ref: &str, head_ref: &str) -> Result<Analysis, String>`. `rules::check_shape::CheckDef.covers` (T3) decides coverage. A `base_ref` equal to git's empty tree gives the whole history (4.1).
- `review::change::AuditSource` (`File(PathBuf)`, `InProcess`).
- `review::change::audit_rows(root: &Path, base: &Base, source: AuditSource, analysis: &Analysis) -> Result<AuditResult, String>`, with the range line of spec 4.
- `review::change::Coverage { uncovered: Vec<ChangedPath>, uncovered_lines: u64, total_lines: u64, share: f64 }`, from `coverage(analysis: &Analysis, audit: &AuditResult, base: &Base) -> Coverage` (4.4, per row).
- `review::rules_hash::rules_hash(root: &Path, install: &str, commit: &str) -> Result<Option<String>, String>` (4.5; the CI files read at `commit`).
- `review::rules_hash::ci_files_at(root: &Path, install: &str, commit: &str) -> Result<Vec<String>, String>`: the CI files of 4.1 at `commit`, as top-level paths. The scan for `ci write`'s files lists `git grep -l --full-name -F -e <CI_WRITTEN_BY_LINE> <commit> -- :/` and keeps each path whose first line at `commit` equals `CI_WRITTEN_BY_LINE` (T3). The rule sources of 4.2 read the same function at the merge base, when there is one, and at the head, and take the union.
- `review::text::code_span(value: &str) -> String` and `review::text::fenced(value: &str) -> String` (4.8).
- `review::git::attr_source_line(command: &str, version_text: &str) -> Option<String>`: a pure function that gives spec 4.2's line when `git --version`'s text names a version older than 2.41, else `None`. `review::git::require_attr_source(command: &str) -> Result<(), String>` runs `git --version` and calls it.
- `published::intent::{Intent, Tasks, Requirements, Validation}`: the fields of 4.9, in that order, with `JsonSchema`.

- [ ] **Step 1: Verify the runner images' git** (global constraints).
- [ ] **Step 2: Write the tests** in the modules' `#[cfg(test)]` blocks, over scratch repositories:
  - Each kind of 4.2: added, edited, a deletion-only hunk, deleted, pure rename, rename with edits, empty file, binary, mode only, symlink (`#[cfg(unix)]`), submodule.
  - The diff size with `diffExclude`.
  - `diff.noprefix`, `diff.relative`, and a request `.gitattributes` with `*.rs -diff` change nothing.
  - Coverage per row (a passing covering row covers, a failing one does not, a non-covering check never covers).
  - The rules hash: from the top level and from `sub/knowledge/` alike, `null` at an empty merge base, `no rule sources at <c7>` at the head, `review.ci.files` read at its own commit.
  - The work items in commit order without duplicates.
  - The areas map (a deleted path by its old path, a renamed path by both).
  - The rule-source paths, including a CI file of 4.1 by its top-level path in a subdirectory install.
  - `code_span` and `fenced` with backtick runs, `<!--`, and a control character.
  - A file whose first line is `CI_WRITTEN_BY_LINE` is a rule source and enters the rules hash at its own commit; the same file without that line is not; a file that gains the line in a later commit leaves the earlier commit's hash unchanged.
  - The old-git line: unit tests of `attr_source_line` over `git version 2.39.0`, `git version 2.40.4`, `git version 2.41.0`, `git version 2.56.0.windows.1`, and `git version 2.39.3 (Apple Git-146)`. T6 tests the line end to end with a `PATH` shim on the child process.
  - Review focus 2 and 3.

  Record the RED run (the module does not exist at BASE).
- [ ] **Step 3: Implement** spec 4 with the exact git argv of 4.2 and 4.5 (`LC_ALL=C`, `-c core.quotePath=false`). Pair `-z --numstat` with the `-U0` text by path.
- [ ] **Step 4: Run the suite and every task gate; commit** `feat(review): analyse a change for the review commands` with the three trailers.

### Task 5: the intent adapters (standard)

Spec: 4.9; case 4; review focus 4. Backlog: HR-246. Needs T4.

**Files:** fill the stub `src/review/intent.rs` (its tests are unit tests); delete each `expect(dead_code)` attribute that its change leaves unmet (global constraints); create the fixtures `crates/houserules/tests/fixtures/intent/openspec-change/` and `crates/houserules/tests/fixtures/intent/spec-kit-feature/`.

**Interfaces:** consumes `install::marker::review::ADAPTERS` (T3) and `published::intent::Intent` (T4). Produces:
- `review::intent::IntentRef { source: String, reference: String }` with `parse(line: &str) -> Result<IntentRef, String>`.
- `review::intent::resolve(root: &Path, analysis: &Analysis, config: &IntentConfig, named: Option<&IntentRef>) -> Result<Vec<Intent>, String>`. The intents come in adapter order, then by `ref`. A named intent that does not resolve is `Err` with the adapter's line, which `bundle check` prints after `intent <source>:<ref>: `.

- [ ] **Step 1: Write the tests** of case 4:
  - The `work-item` adapter with a tracker gives one intent per trailer with the title; with no tracker it gives none.
  - The `file` adapter over a plan file gives its heading, its first paragraph cut to 1,000 characters, and the checkbox counts with `-`, `*`, `+`, `[x]`, `[X]`, and `[ ]`. The adapter also runs over a changed path that matches `review.intent.files` and over `Intent: file:<path>`.
  - Each name of `ADAPTERS` has an adapter, and a name with none fails the test.
  - The contract-shape tests over the OpenSpec and Spec Kit fixture folders: each column of 4.9's table filled by hand into an `Intent` that serializes and reads back unchanged. T7 validates the same two intents against the committed evidence schema.
  - Review focus 4.

  Record the RED run.
- [ ] **Step 2: Implement; run the suite and every task gate; commit** `feat(review): read the intent of a change from work items and files` with `Work-Item: HR-246`.

### Task 6: `route` (critical)

Spec: 6.2 to 6.4, the `route` parts of 4; case 2 (the command part, without the `report` job logic, which T11 owns); review focus 5. Backlog: HR-186. Needs T4.

**Files:**
- Fill the stub `src/review/route.rs`; create `schemas/route/1.0.0.json` and `crates/houserules/tests/route.rs`; delete each `expect(dead_code)` attribute that its change leaves unmet (global constraints).
- Modify:
  - `src/published/route.rs` (the document types beside `Route`)
  - `src/published/mod.rs`, `src/bin/gen-schemas.rs`, `schemas/published.json`
  - `src/main.rs`: the `Route` variant; the count at its BASE plus one (25 in the planned order); `route` in the surface-name test, which T6 renames as the waves paragraph says
  - `crates/houserules/tests/check_parity.rs` (`route` in `ROOT_RESOLVING_COMMANDS`)

**Interfaces (produces):**
- `published::route::{RouteDocument, Inputs, Reason}`, as in 6.4.
- `review::route::decide(analysis: &Analysis, audit: Option<(&AuditResult, &Coverage)>, config: &RouteConfig, score: Option<f64>) -> Result<RouteDocument, String>`. `None` means `--no-audit`, which gives a floor document.
- `review::route::reason_lines(doc: &RouteDocument) -> Vec<String>`, the lines of 6.3 at the final route.
- `review::route::RouteArgs`, the clap struct of 6.3.

- [ ] **Step 1: Write the tests** of case 2's command part:
  - Each route rule, the floor (`--no-audit` with a missing runner tool), and each reason line verbatim.
  - 25 paths print 20 and `and 5 more`.
  - The document names every input and threshold, and it validates.
  - The `.gitattributes` case, and the old-git case end to end: a `PATH` set on the child process to a shim directory whose `git` prints `git version 2.39.0` for `--version` and passes every other call to the real git (`#[cfg(unix)]`).
  - `--score` with no threshold, and an `--audit` file of another range (exit 2 with spec 4's line).
  - Review focus 5 (the route half; T7 pins the packet's `None.` with `a_head_behind_its_base_prints_none`).

  Record one RED run at BASE.
- [ ] **Step 2: Implement; run the suite and every task gate; commit** `feat(route): decide who reviews a change` with `Work-Item: HR-186`.

### Task 7: `evidence` (critical)

Spec: 5; case 1; the packet half of case 4. Backlog: HR-185, HR-246. Needs T5, T6.

**Files:**
- Fill the stub `src/review/evidence.rs`; create `src/published/evidence.rs`, `schemas/evidence/1.0.0.json`, and `crates/houserules/tests/evidence.rs`.
- Modify:
  - `src/published/mod.rs`, `src/bin/gen-schemas.rs`, `schemas/published.json`
  - `src/main.rs`: the `Evidence` variant; the count at its BASE plus one (26 in the planned order); `evidence` in the surface-name test
  - each `expect(dead_code)` attribute that its change leaves unmet (global constraints; T7 is the last reader of `Analysis`'s merge base and rules hashes)
  - `crates/houserules/tests/check_parity.rs` (`evidence`)

**Interfaces (produces):**
- `published::evidence::Evidence`, the JSON of 5.9.
- `review::evidence::build(analysis: &Analysis, coverage: &Coverage, audit: &AuditResult, review: Option<&Value>, intents: Vec<Intent>, route: RouteDocument) -> Evidence`.
- `review::evidence::markdown(doc: &Evidence, install: &str) -> String`, holding 5.2 to 5.8 with the cut order of 5.8.
- `review::evidence::EvidenceArgs`.

- [ ] **Step 1: Write the tests** of case 1 (every line, including the 3,000-path and 3,000-id cuts and the 60,000-character cap) and the packet half of case 4. The JSON validates with both range forms. The two fixture intents of T5 (OpenSpec and Spec Kit), placed in an evidence document, validate against the committed evidence schema. Each exit-2 line of 5.1: an `--audit` file that does not parse, a `--review` file that does not parse as a review deliverable, an `--intent` whose source is no adapter, a `--score` outside 0 to 1, and an unwritable `--out` and `--json`. Review focus 5's packet half (`a_head_behind_its_base_prints_none`). Record one RED run at BASE.
- [ ] **Step 2: Implement; run the suite and every task gate; commit** `feat(evidence): write the reviewer packet of a change` with both trailers.

### Task 8: `bundle write` and `bundle check` (critical)

Spec: 7.1 to 7.3; case 3; review focus 1. Backlog: HR-179. Needs T4, T5.

**Files:**
- Fill the stub `src/review/bundle.rs`; create `crates/houserules/tests/bundle.rs`.
- Modify `src/main.rs` (the `Bundle` variant with `write` and `check`; the count at its BASE plus one, 27 in the planned order; `bundle` in the surface-name test) and `crates/houserules/tests/check_parity.rs` (`bundle check` in `ROOT_RESOLVING_COMMANDS`, as `pack`'s subcommands are listed; the doc comment of `NOT_ROOT_RESOLVING`, which holds top-level names only, states that `bundle write` takes no `--dir` (spec 7.2) and so has no entry in `ROOT_RESOLVING_COMMANDS`).

**Interfaces (produces):**
- `review::bundle::Bundle { purpose, intent: Option<IntentRef>, work_items: Vec<String>, tier, criterion, budget_reason, runs: Vec<BundleRun> }`, where `BundleRun { header_line: String, header: Header }`.
- `review::bundle::sections(body: &str) -> Vec<(usize, &str)>`, which applies 7.1's fence-aware section rule.
- `review::bundle::parse(section: &str) -> Bundle`. A missing line is an empty field; `check` reports it.
- `review::bundle::BundleArgs`.

T9 consumes `parse` for the record's `bundle` field.

- [ ] **Step 1: Write the tests** of case 3 (every line of 7.3, the exempt author and branch, `::error::x`, the fenced heading, the `.gitattributes` budget case), each exit-2 line of 7.2 (a capture that does not parse, neither `--purpose` nor an `--intent` whose adapter gives a why, an empty tier or criterion, no `--work-item`, no `--capture`, an unwritable `--out`), and review focus 1, with captures written by `houserules capture`. Walk spec 13's `bundle check` rows for both hosts. Record one RED run at BASE.
- [ ] **Step 2: Implement; run the suite and every task gate; commit** `feat(bundle): write and check the evidence bundle of a request` with `Work-Item: HR-179`.

### Task 9: `record write` and `record show` (critical)

Spec: 8.1 to 8.5, 4.5's `record show` check; case 5 (but `record list`, `record audit`, and `stats --records`, which T10 owns). Backlog: HR-192. Needs T6, T7, T8.

**Files:**
- Fill the stub `src/review/record/mod.rs`; create:
  - `src/review/record/{document.rs,budget.rs,notes.rs,sample.rs}`
  - `src/published/change_record.rs`, `schemas/change-record/1.0.0.json`
  - `crates/houserules/tests/record.rs`
- Modify:
  - `src/published/mod.rs`, `src/bin/gen-schemas.rs`, `schemas/published.json`
  - `src/main.rs`: the `Record` variant with `write` and `show`; the count at its BASE plus one (29 in the planned order); `record` in the surface-name test
  - `crates/houserules/tests/check_parity.rs` (`record write` and `record show`)

**Interfaces (produces):**
- `published::change_record::ChangeRecord`, as in 8.2.
- `review::record::notes::notes_ref(install: &str) -> String`.
- `review::record::notes::read(root: &Path, install: &str, sha: &str) -> Result<Option<ChangeRecord>, String>`, which finds the record keyed by `sha` or the record that lists it.
- `review::record::notes::write(root: &Path, install: &str, key: &str, bytes: &[u8]) -> Result<(), String>`.
- `review::record::budget::fit(record: ChangeRecord, max_bytes: u64, reserve: u64) -> ChangeRecord`, the cascade of 8.4.
- `review::record::sample::drawn(key: &str, rate: f64) -> bool`.
- `review::record::RecordArgs` with the subcommands `write` and `show`. T10 adds `list` and `audit`.

- [ ] **Step 1: Write the tests** of case 5's write and show parts:
  - The notes push to and fetch from a local bare remote; a clone without the ref prints the fetch line.
  - The hash mismatch; the subdirectory ref; `merge_base_rules: "absent"`; a root push; a shallow clone.
  - The re-run rule and another range.
  - Every budget case: 5,000 paths; 5,000 no-area paths; a 100,000-character purpose; with `maxBytes: 8192`, ten intent refs of 414 characters and 20 reviewer names of 255 characters with the reserve of `--reviewers-pending` filled; the minimal record with ten refs of multi-byte and of control characters and an audited key with a 1,000-byte note, under `maxBytes: 8192` with the reserve filled.
  - Case 4's last sentence: the packet's Intent section and the record's `intent` show the same values for one range.
  - `audit.error` with exit 0; the sample on a squash commit.
  - `record show` of a commit with no record: `record: no record for <sha7>`, exit 1.
  - The GitLab-shaped `jq` edit of 10.4 (25 approvers), run with the `jq` program that T11's unit holds, read through T11's handle (the variable `HOUSERULES_REVIEWERS_JQ` of the `record-push` job of `ci/gitlab-review.yml`, read with `saphyr`). T11 lands in wave 4, before this task.
  - A later `review.ci.files` change still passes `record show`.

  Record one RED run at BASE.
- [ ] **Step 2: Implement; run the suite and every task gate; commit** `feat(record): keep a change record per merge in git notes` with `Work-Item: HR-192`.

### Task 10: `record list`, `record audit`, `stats --records`, and the `Reversal:` warn line (critical)

Spec: 8.3 (`list`, `audit`, `stats --records`), 11.6; case 5 (those parts), case 10 (the warn line). Backlog: HR-192, HR-241. Needs T9.

**Files:**
- Create `src/rules/review_coverage.rs` and `crates/houserules/tests/record_list_audit.rs`.
- Modify:
  - `src/review/record/mod.rs` (`list` and `audit`)
  - `src/rules/stats.rs` (`--records` calls `review_coverage`; the warn line on stderr)
  - `src/main.rs` (`--records` on `Stats`; the count stays 29)
  - `template/workflows/reference/.claude/agents/branch-reviewer.md` (it names the warn line beside its own count) and its root copy
  - create `crates/houserules/tests/workflow_texts_37.rs` for the branch-reviewer sentence (T17 changes `workflow_texts.rs` in the same wave)
  - `crates/houserules/tests/check_parity.rs` (`record list` and `record audit`)

**Interfaces:** consumes T9's `notes::read`, `ChangeRecord`, and `budget`. Produces `rules::review_coverage::review_coverage(root: &Path, range: &str) -> Result<Value, String>`, the `review_coverage` key of 8.3.

- [ ] **Step 1: Write the tests**:
  - `record list` with and without `--sampled`, in its order.
  - `record audit` changes one key and no other byte; the second audit is exit 1; the cuts of `--by` and `--note`; the `maxBytes` refusal line.
  - `stats --records` with and without workspaces.
  - The warn line for an embedded `Reversal:`, which leaves the count of 226(2) unchanged.
  - The branch-reviewer sentence.

  Record one RED run at BASE.
- [ ] **Step 2: Implement; run `update --dir .`; run the suite and every task gate; commit** `feat(record): list and audit change records and count review coverage` with both trailers.

### Task 11: the published CI units (critical: write credentials)

Spec: 10.1, 10.2, 10.4 to 10.6, 13; case 8 (the unit parts); case 2's `report` logic. Backlog: HR-245, HR-179, HR-185, HR-186, HR-192. Needs T13 (`saphyr`).

**Files:**
- Create:
  - `.github/workflows/houserules-review.yml`, the GitHub unit's one source (design.md 253): a `version` input, needed only for a `release` install, and a minimum-version line set to `2.0.0`, which a person raises by hand; no release version line
  - `ci/gitlab-review.yml` (design.md 251), with its release version line
  - `crates/houserules/tests/ci_support/mod.rs`, the shared unit assertions
  - `crates/houserules/tests/ci_units.rs`, with fixtures under `crates/houserules/tests/fixtures/ci-units/` (route documents, request payloads, and a stub `gh` and `curl`)
- Modify:
  - `release-please-config.json`: one `extra-files` entry for the version line of `ci/gitlab-review.yml`
  - `crates/houserules/tests/release_please_config.rs`: `extra_files_contains_only_the_version_entry` (line 268) and its doc name the new entry and keep the invariant that no entry sits under `template/` or `.github/workflows/`

**Interfaces (produces):**
- The two units: their inputs as in 10.2 (GitHub's `version` among them); the job names `gates`, `floor`, `report`, `record`, `record-push`, which are `houserules-<key>-<job>` on GitLab; GitHub permissions as 10.2 states them since the first plan review (`issues: read` static on `gates` and `record`).
- Handles that tests read with `saphyr`, by job name:
  - the GitLab `record-push` job's variable `HOUSERULES_REVIEWERS_JQ`, the `jq` program that sets the reviewers (10.4); T9's test runs it;
  - in each unit's `report` job, the variable `HOUSERULES_ROUTE_SH`, a POSIX shell function `applied_route <gates-doc> <floor-doc> <own-head> <current-head>` that prints the applied route and its notice and exits 0; the job reads the current head with `gh` or `curl` and passes it, so the test passes it directly and runs the job script with the stub `gh` and `curl` on the child's `PATH` for the head-mismatch case.
- `ci_support` (test helper): `parse_unit(text: &str, host: Host) -> Unit` (through `saphyr`), and one assertion per trust rule of 10.1 and case 8 over a `Unit`: the job set and names, each GitHub job's exact `permissions`, the token environment on `report` and `record-push` only, no `set -x` and the trace variables unset in the write jobs, the GitHub mask, `report` and `record-push` with no houserules step and no project checkout, `floor` with no `setup`, no tool step, and no `install: none`, and no `run` script that names the request description in a `${{ }}` expression (C6; the description reaches a file through `jq` or the API read only). T12 runs the same assertions on each inline form.

No install path writes a unit file; both are outside the kit payload. Release-please bumps only `ci/gitlab-review.yml`, so `payload.stamp` and the workflow files stay unchanged; Dependabot's action-pin updates change `.github/workflows/houserules-review.yml` alone, which no copy mirrors (design.md 253).

- [ ] **Step 1: Write the tests** in `ci_units.rs` through `ci_support`:
  - actionlint on the GitHub unit through `mise exec -- actionlint`.
  - `saphyr` parses the GitLab unit into the job set of 10.1, with job names keyed by the install.
  - Every trust assertion of `ci_support` on both units.
  - `floor-install: runner:<path>` is among both units' inputs.
  - The `report` logic through `HOUSERULES_ROUTE_SH`: the higher of gates and floor, a refused fourth value, a missing or invalid floor gives at least `owner-review`, a gates-uploaded floor document is not read, the applied-route line is at most 120 characters, and a head mismatch gives its notice and no write (the stub `gh` and `curl` record every call; a mismatch makes no write call).
  - Two installs of one repository get distinct GitLab job names, markers, status contexts, labels, artifacts, and notes refs.
  - The units' `github.com` grep with spec 21.8's exceptions.
  - The GitLab unit's version line equals `CARGO_PKG_VERSION`. The GitHub unit holds no release version line; a `release` install without `version` fails with spec 10.2's line; a `release` binary older than the minimum fails with spec 10.6's line (the install step's check run with a stub `houserules --version`); `install: none`, `floor-install: default-branch`, and `runner:<path>` run no minimum check.
  - The commit classes that touch a unit (`process.gates-cover-generated-commits`): a scratch release-please commit (the GitLab version line and `Cargo.toml`) passes the payload-stamp gate and the version test; a scratch Dependabot commit that bumps one action pin in the GitHub unit passes every test. The report states each accepted red.

  Record one RED run at BASE.
- [ ] **Step 2: Implement** the units. Verify each GitHub and GitLab feature of the global constraints against current docs. Pin every action by sha (`houserules.actions-pinned-by-sha`). Pin the GitLab unit's default `image` by digest to an image with `git` 2.41 or later, `curl`, and `jq`, and record its git version (global constraints).
- [ ] **Step 3: Run the suite and every task gate; commit** `feat(ci): publish the review units for GitHub and GitLab` with the five trailers.

### Task 12: `houserules ci` (critical)

Spec: 10.2 (the `ci print` lines), 10.3, 7.4 (the template); case 8 (the `ci` parts). Backlog: HR-245, HR-179. Needs T3, T11.

**Files:**
- Fill the stub `src/review/ci.rs`; create `crates/houserules/tests/ci_command.rs`.
- Modify:
  - `src/main.rs` (the `Ci` variant with `print` and `write`; the count at its BASE plus one, 28 in the planned order; `ci` in the surface-name test)
  - `crates/houserules/tests/check_parity.rs` (`ci print` and `ci write`)
  - `crates/houserules/Cargo.toml` and `Cargo.lock` (`base64` 0.23.1, design.md 250: the lock gains no crate, and the houserules entry gains `"base64 0.23.1"`)
  - `src/install/marker/review.rs` (it deletes the `expect(dead_code)` of `Marker::add_ci_file`)

**Interfaces:** consumes `CI_WRITTEN_BY_LINE` and `Marker::add_ci_file` (T3), the unit files (T11, through `include_bytes!("../../../../.github/workflows/houserules-review.yml")` and `include_bytes!("../../../../ci/gitlab-review.yml")`), and `ci_support` (T11). The GitHub reference lines carry `version: <the binary's version>` beside `@<sha> # v<version>` (design.md 253). Produces `review::ci::CiArgs` and `review::ci::integrity(bytes: &[u8]) -> String`, which gives `sha256-<base64>`.

- [ ] **Step 1: Vet and add `base64`** (`cargo add base64@=0.23.1`), and check that `Cargo.lock` adds no crate. Commit `build: add base64` with `Work-Item: HR-245`; this commit is the RED base.
- [ ] **Step 2: Write the tests** of case 8's `ci` parts:
  - `print` with both hosts and both modes, including `integrity` against the embedded file and the `review.ci.files` line; GitHub's caller lines grant the union of 10.2 and pass `version`.
  - `print --template` prints the section of 7.4 and no `review.ci.files` line.
  - `write`: its two comment lines, the refusal of an existing file, `--force` only on its own file, the `review.ci.files` entry, `--sha` required for GitHub reference mode, and the GitLab include and `review.ci.files` lines.
  - `--template` in each location of 7.4, with the priority line.
  - `--host` with another origin and with no origin.
  - Each inline form through actionlint and through every `ci_support` assertion.
  - The command's `github.com` grep.

  Record one RED run at the dependency commit.
- [ ] **Step 3: Implement; run the suite and every task gate; commit** `feat(ci): print or write the review wiring for a project` with both trailers.

### Task 13: `init`, `update`, and the seeded workflow's reader (critical)

Spec: 10.7, 10.3 (the `update` pin line); case 8 (the `init` and deleted-workflow parts), case 9 (the workflow parts, HR-243). Backlog: HR-245, HR-243. Needs T3.

**Files:**
- Modify:
  - `crates/houserules/Cargo.toml` and `Cargo.lock` (`saphyr`)
  - `src/install/mod.rs` (no CI seeding; the `init` line; no re-write of an absent workflow; the opt-in line once; the pin line)
  - `src/install/ci_workflow.rs` (the `saphyr` reader per job)
  - `src/install/hand_steps.rs` (the `contents: write`, no-job, and invalid-YAML lines)
  - `src/install/marker/review.rs` (it deletes the `expect(dead_code)` of `CI_WRITTEN_BY_LINE`, `review_layer_offered`, and `set_review_layer_offered`; T4 deletes the first line in the same wave)
  - `src/main.rs`, the help that 10.7 makes false, with these texts, each pinned by a test in `update_37.rs`:
    - `Init`'s `--dir`: `The install root; defaults to the current directory. Unlike every read command's --dir above, this is not resolved against an enclosing install -- the directory itself is the root. It must exist and lie in a git repository: the top level, or a directory below it for a subdirectory install. A new install root (one with no .houserules.json yet) is refused when it nests with another install of the repository; a directory that holds its own .houserules.json is no new root.` (with the code spans of the current text)
    - `Files`: its last sentence becomes `A seeded CI workflow is a D3 file; init writes none (houserules ci prints or writes the CI wiring).`
  - `init`'s refusal of a new root that would write another install's workflow file goes with the seeding, and its tests with it; the nesting refusal stays.
  - `src/install/classes.rs`: the doc of `D3_FILES` states that `init` seeds no CI workflow and that `update` keeps an existing seeded one current.
  - `src/rules/git_paths.rs` (`tracked_from_top_level`) and `src/rules/mod.rs` (its re-export).
  - `crates/houserules/tests/init_install.rs` (the `init` cases of 21.8; after T15)
  - every other test that pins `init`'s seeded workflow (the task enumerates them with `grep -rln "knowledge.yml" crates/houserules/tests` and records the list)
  - `crates/houserules/tests/ci_workflow_36.rs`
- Create `crates/houserules/tests/update_37.rs`.

**Interfaces:** consumes `Marker::review_layer_offered`, `set_review_layer_offered`, and `CI_WRITTEN_BY_LINE` (T3). The pin line scans every tracked file of the repository, from its top level, for a first line equal to `CI_WRITTEN_BY_LINE`, through a new `rules::git_paths::tracked_from_top_level(root: &Path) -> Result<Vec<String>, String>` (`git ls-files -z --full-name -- :/`, top-level paths; `rules/git_paths.rs` holds every `ls-files` call, `tests/ls_files_home.rs`), so a subdirectory install finds a top-level `ci write` file. Produces nothing that later tasks read.

- [ ] **Step 1: Vet and add `saphyr`** (global constraints). Commit `build: add saphyr` with `Work-Item: HR-243`; this commit is the RED base.
- [ ] **Step 2: Write the tests**:
  - `init` on a GitHub, a GitLab, another-host, and a no-origin repository writes no CI file and prints its line.
  - `update` with the seeded workflow deleted writes none and prints nothing.
  - Case 9's workflow parts: the opt-in line once with `reviewLayerOffered`; each HR-243 form (re-indented, renamed, two jobs, none, `contents: write`, invalid YAML); a copy with `persist-credentials: false` or `contents: read` in other jobs; the pin line for an older `ci write` file, in a top-level and in a subdirectory install (the file at the top level); a second `update` changes nothing.

  Walk spec 13's upgrade reds. Record one RED run at the dependency commit.
- [ ] **Step 3: Implement; run the suite and every task gate; commit** `feat(install)!: write no CI file and read seeded workflows as YAML` with both trailers.

### Task 14: this repository's wiring (standard)

Spec: 9.6, 10.8; case 13's preparation. Backlog: HR-245, HR-187. Needs T11, T1.

**Files:**
- Create `.github/workflows/review.yml` and `.github/workflows/schemas.yml`.
- Modify:
  - `.github/workflows/ci.yml` (the `gen-schemas --check` step in `checks`)
  - `crates/houserules/tests/ci_gate_parity.rs` (its step count and `identify` know the new step)
  - `knowledge/houserules.json`: the body of `houserules.task-gates-mirror-ci` names `gen-schemas --check` (the parity test requires it), then `render`. This is the one `knowledge/` edit a task makes; no task of wave 5 touches the file.
  - `.github/PULL_REQUEST_TEMPLATE.md` (the bundle section of 7.1, by hand)

The controller adds the `review` key of 10.8 to `.houserules.json` at this task's integration (controller step 3).

`setup` follows spec 10.8 as amended by the plan review (P12): it installs the tree binary at the request's head on the PATH with `cargo install --locked --path crates/houserules --bin houserules`, under the Rust toolchain that `mise.toml` pins. The toolchain version is read from `mise.toml` at run time, never typed a second time; any installer that `setup` runs is pinned (HR-083). After `setup`, `houserules --version` prints the crate's version.

- [ ] **Step 1: Write the test** `crates/houserules/tests/repo_wiring.rs`:
  - actionlint passes on `review.yml` and `schemas.yml`.
  - `review.yml` calls `./.github/workflows/houserules-review.yml` with `install: none`, the `setup` above, and `floor-install: default-branch`, and no `version` (no job installs `release`), grants the union of 10.2's permissions, and triggers on `pull_request` with `edited` and on `push` to `main`.
  - The `setup` command, run with the user's `CARGO_HOME` (no download) and a scratch `CARGO_INSTALL_ROOT` and `CARGO_TARGET_DIR` on the child process, puts a `houserules` whose `--version` is the crate's in `<root>/bin`, which the child's `PATH` names. The test is `#[cfg(unix)]` and `#[ignore]`, with the reason that it builds the crate a second time; the task runs it once with `--ignored`, and the controller runs it at integration and records it.
  - `schemas.yml` deploys a site whose root holds `schemas/`, with `pages: write` and `id-token: write`, on `push` to `main` with `paths: schemas/**` and on `workflow_dispatch`.
  - `ci.yml`'s `checks` job runs `gen-schemas --check`.
  - Every action is pinned by sha.

  Record the RED run.
- [ ] **Step 2: Implement; run `render`; run the suite and every task gate; commit** `ci: wire the review layer and publish the schemas` with both trailers.

### Task 15: one `sha256_hex` in five test files (standard, haiku L1)

Spec: 1 (HR-236), 15. Backlog: HR-236.

**Files:** `crates/houserules/tests/{check_packs.rs,every_commit_entry.rs,init_install.rs,kit_citations.rs,pack_moves.rs}`.

- [ ] **Step 1: Record the inline digests** at BASE with `grep -n "Sha256::digest" crates/houserules/tests/{check_packs,every_commit_entry,init_install,kit_citations,pack_moves}.rs`. There are six (HR-236's body; `kit_citations.rs` holds two).
- [ ] **Step 2: Replace each digest** with a call to `tracker_support::sha256_hex` (`crates/houserules/tests/tracker_support/mod.rs:101`). Include the module as the other tests do (`mod tracker_support;`), and drop each `sha2` import that becomes unused. No test changes its expectation.
- [ ] **Step 3: Prove the tests still bite** (disclosed mutation): change one expected pin in a scratch copy of `pack_moves.rs`, run it, record the failure, and revert.
- [ ] **Step 4: Run the grep again** (zero hits), the suite, and every task gate. **Commit** `test: compute every pack pin digest with the shared helper` with `Work-Item: HR-236`.

### Task 16: the template defects of the batch-36 branch review (critical: a schema description)

Spec: 11.1 to 11.5; case 10 (the texts of 11). Backlog: HR-241.

**Files:**
- `template/.claude/schemas/deliverables.json` (the `issue.backlog` description, verbatim from HR-241 (1)) and its root copy.
- `template/workflows/reference/.claude/agents/{task-reviewer.md,branch-reviewer.md}` (one sentence each, HR-241 (1)).
- `template/workflows/reference/.claude/skills/orchestrating/SKILL.md` (11.2, 11.3, 11.4).
- `template/workflows/reference/.claude/skills/planning/SKILL.md` (11.5).
- Their root copies through `update --dir .`.
- `crates/houserules/tests/workflow_texts.rs`.

The description adds no constraint to the schema (`houserules.1-0-surface-is-frozen` freezes constraints), and a test shows that the schema validates the same reports as at BASE.

- [ ] **Step 1: Write the tests**:
  - Each text of 11.1 to 11.5 holds its new sentence.
  - The schema's description equals HR-241 (1)'s text.
  - The batch-36 workspace's task reports validate with the new schema as with the old one.

  Record one RED run at BASE.
- [ ] **Step 2: Change the texts; run `update --dir .`; run the suite and every task gate.** The audit and the PR-audit gate run with `--sanctioned process.evals-rerun=plan-controller-step-5`, because the eval record lands at controller step 5, and the report states it. **Commit** `docs: fix six kit template defects` with `Work-Item: HR-241`.

### Task 17: the kit texts and the wording gate (critical: the process pack)

Spec: 7.4 (the seed rule), 7.5, 7.6, 3 (the wording); case 10 (the wording grep and the upgrade-guide parts), case 9 (the process pack's minor version). Backlog: HR-179, HR-185, HR-186, HR-192, HR-245, HR-246. Needs T6 to T9 and T11 to T13 (every command's text). T10 runs beside it; the controller's gate run after both wave-9 integrations runs `host_wording.rs` over T10's texts too.

**Files:**
- `template/packs/process.json` (`process.evidence-before-review`, the summary verbatim from 7.4; a minor version), `template/packs/writing-style.json` (the README funnel's "whether pull requests are accepted" becomes "whether pull or merge requests are accepted"; a patch version), and `src/packs.rs` (`KIT_PACK_VERSIONS` and the checksums of both packs).
- `template/workflows/reference/.claude/skills/finishing-a-feature/SKILL.md` (7.5).
- `template/workflows/reference/.claude/skills/orchestrating/SKILL.md` (names the bundle step; after T16).
- `template/.claude/skills/migrating-knowledge/SKILL.md` (7.6, with every limit it lists).
- `README.md` (the CLI table and a review-layer section).
- `docs/upgrading-to-2.md` (the batch-37 section of 7.6).
- Their root copies through `update --dir .`.
- `crates/houserules/tests/workflow_texts.rs`, and a new `crates/houserules/tests/host_wording.rs`.

`host_wording.rs` derives its scope: every tracked file under `template/`, `README.md`, `docs/upgrading-to-2.md`, and the binary's help (`houserules --help` and each subcommand's `--help`). It fails on "pull request" without "merge request" except for spec 21.10's exceptions, each named in the test with its reason (`quality.gates-derive-their-scope`).

- [ ] **Step 1: Write the tests**:
  - The seed rule's summary verbatim, and the pack's version and checksum.
  - Each part of 7.6 has its own grep in the migrating-knowledge skill and the upgrade guide.
  - The finishing skill's `bundle write` step, and `gh pr create` and `glab mr create`.
  - The orchestrating skill's step.
  - The wording gate, proved by a disclosed mutation that adds "pull request" to a scratch copy of one skill.

  Record one RED run at BASE.
- [ ] **Step 2: Change the texts; move the pack version; run `update --dir .`; run the suite and every task gate; commit** `docs: teach the per-change review layer in the kit` with the six trailers.

## Controller steps

1. **Before T1:**
   - Confirm that this plan's review closed with no Critical finding, record each round in the ledger, and close the plan window of `efficiency-windows.json`.
   - Commit the plan.
   - Build the tree binary.
2. **After each integration:**
   - Run the gates and the runs that the harness refused to an agent (the autocrlf suite, shellcheck, actionlint where refused). Run each through `houserules capture` with any injected configuration in the argv (HR-241 (3)), and record each in the ledger.
   - Extend the commit map.
3. **At T14's integration:**
   - Add the `review` key of spec 10.8 to `.houserules.json` and run `check-knowledge`.
   - Add the entry `houserules.published-documents-carry-their-schema` of spec 9.5 to `knowledge/houserules.json` (area `cli`, not standing), then `render`. T14's commit already changed `houserules.task-gates-mirror-ci` in the same file; this step runs after T14 integrates.
   - Commit `docs(knowledge): ...` with `Work-Item: HR-187` and `Work-Item: HR-245`.
4. **After T9 (the last command variant):** set the body of `houserules.1-0-surface-is-frozen` to 29 `Command` variants (spec 0, design.md 248), `render`, and commit with `Work-Item: HR-187`.
5. **After T17: the eval run.**
   - Run every scenario of `.claude/evals/` through the changed templates and the branch binary. Each scenario brief names the project's test command as `mise run test` (HR-241 (4)).
   - Append `record.json` and commit it before the branch review (spec case 11). This clears T16's sanctioned row.
6. **Live run (spec 21.12):**
   - A dispatched implementer runs it with a live-run brief, in a scratch install under `~/.cache/hr-b37/live/` with a local bare remote and the branch binary.
   - Then this repository's `update` with the tree binary, and a second `update` that changes nothing.
7. **Pages (Q2 of spec 18):** before the merge, ask the owner whether the controller may enable GitHub Pages with the source "GitHub Actions" on `jblossey/houserules`, and act on the answer in that turn.
8. **Batch close:**
   - Record that `grep -rn 'expect(dead_code' crates/houserules/src` prints nothing (no interim allowance survives).
   - Run `stats --window 5` over the workspaces to `stats.json`, then produce `efficiency.json`.
   - Run the branch review (`branch-reviewer`, fable) and its fix wave.
   - Make the knowledge commit of the non-owner-gated proposals.
   - Run `houserules set <id> status=done batch=37` for HR-179, HR-185, HR-186, HR-187 (its 2.0.0 part; the body names HR-211 for the rest), HR-192, HR-236, HR-241, HR-243, HR-245, and HR-246.
   - Add HR-242's data point. Re-read this plan's Files lines and Global constraints, and add a revision note for each sentence that the batch made false (HR-241 (5)).
9. **Finish:**
   - Aggregate into one to five clean commits, each with a `Work-Item:` trailer for every item it delivers.
   - Write the bundle with `bundle write`, `Intent: file:docs/plans/2026-10-24-batch-37-per-change-review-layer.md`, from the captures of the final gate run at the head, into the PR description.
   - Push and open the PR. The GitHub proof (spec 21.13) runs on the PR, with the expected floor notice.
   - Run the GitLab proof (spec 21.14) with the include pointed at the pushed commit. Before it, ask the owner Q3 (the private scratch project and token) in its own turn, and remind the owner to delete both afterwards.
   - Before the merge, ask the owner Q4 (the merge mode) in its own turn. No release: PR #83 stays held until batch 38.
   - After the merge, fetch the notes ref, run `record show` on the last pushed commit, and check each Pages URL against its committed file (spec 21.13).

## Spec acceptance cases → tasks

| Case | Task | Case | Task |
|---|---|---|---|
| 1 | T4 (analysis), T7 | 8 | T11 (units), T12 (`ci`), T13 (`init`, `update`) |
| 2 | T4 (analysis), T6 (command), T11 (`report` logic) | 9 | T3 (schemas), T13 (workflow), T17 (pack) |
| 3 | T8 | 10 | T16 (11.1 to 11.5), T10 (11.6), T15 (HR-236), T17 (wording, guide), controller step 4 (29) |
| 4 | T5, T7 (packet), T9 (record) | 11 | controller step 5 |
| 5 | T4 (hash), T9, T10 | 12 | controller step 6 |
| 6 | T1, T2, and each document task's validation test | 13 | T14, controller steps 7 and 9 |
| 7 | T3 | 14 | controller step 9 |
