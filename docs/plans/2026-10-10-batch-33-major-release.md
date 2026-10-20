# Batch 33 implementation plan: the 2.0.0 major release

**Status: SUPERSEDED on 2026-10-15** by the owner's re-scope to discussion 59 (design.md 5.97). No task of this draft runs. **Status: DRAFT, not approved.** The spec is a draft too. The owner ruled spec decisions D1 to D11 on 2026-10-10 (design.md 5.96); each task's `Depends on:` line names the ruling it follows. Q1 is ruled: HR-088 and HR-053 stay out. No task starts before the owner approves the spec.

**Goal:** Ship 2.0.0: evidence by reference, a check type that runs a project command, report fields that tests can check, and an upgrade path that carries every install from 1.x to 2.0.0.

**Spec:** `docs/specs/2026-10-10-batch-33-major-release.md`. Executors read both.

**Tier:** full. Criteria: `crates/houserules/src/**` changes (`houserules.cli-changes-run-full-tier`), `template/**` changes (`houserules.template-changes-run-full-tier`), and a public contract and schema change. Cap: two re-reviews per task.

**Workspace:** `.superpowers/sdd/2026-10-10-batch-33/` (ledger `progress.md`).

## Global constraints

- Release 2.0.0 through release-please. A commit that breaks the 1.x surface carries `!` and a `BREAKING CHANGE:` footer.
- The 2.0 surface: the 1.x subcommands plus `capture`. Update `the_1_0_cli_surface_names_exactly_these_20_subcommands` and `no_subcommand_carries_a_long_about_beyond_its_short_one` with the new count in the task that adds `capture`; rename the first test for the 2.0 surface (`knowledge-base.ids-are-permanent` does not cover test names).
- Both copies of each schema file stay identical apart from the id prefix (the existing parity tests).
- Edit the kit in `template/`, then `./target/debug/houserules update --dir .` (after `cargo build`) and `render` with the tree binary (`houserules.template-is-the-source`, `houserules.controller-gates-use-the-tree-binary`); regenerate `crates/houserules/payload.stamp` with `cargo run --bin payload-stamp-gate -- --write`.
- Seeded text stays harness-neutral (`houserules.agents-md-is-canonical`). Writing style: ASD-STE100 (`writing-style.principles`).
- Task-end gates mirror CI (`houserules.task-gates-mirror-ci`): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `mise run lint`, `./target/debug/houserules check-knowledge`, `./target/debug/houserules check-backlog`. After every commit: `./target/debug/houserules check-commit --from <BASE>` (the commit-msg hook skips the check when no `houserules` is on PATH).
- No new dependency without the vetting record (`security-hygiene.dependency-vetting`), installed with `cargo add <crate>@=<version>` (`security-hygiene.exact-pins`). T1 adds `jiff` for the capture timestamps (spec D9, ruled); the crate has no date or time dependency today. `sha2` is present for the hash.

## Review focus

1. A command whose output is not UTF-8 (a Windows code page, a binary blob): `capture` records `encoding: lossy` with the count and never fails the run for it.
2. A command that never starts (a typo, a missing binary): `capture` writes a header with `exit: 127` and the error text, and exits 127.
3. A 1.x report read by the 2.0 binary: `validate` and `check-report-claims` print one named line, and `stats` counts it with 2.0 reports in one run.
4. A `command` check on a dirty working tree or a tree whose HEAD is not `--head`: the row is `skipped` with the reason, never a pass or a fail from the wrong tree.
5. `update` on an install whose adopter changed a schema file or an eval: the file is kept, the summary names the hand step, and a second `update` prints nothing new.

## Code-health scan

From `m2-code-health.txt` (production lines, before the `#[cfg(test)]` module):

- `crates/houserules/src/report_claims.rs`: 1,804 production lines, 46 functions, 15 `unwrap`/`expect`. T2 retires four checks and splits the rest into `report_claims/` submodules: one per check family and one for the shared capture resolver. Read each of the 15 `unwrap`/`expect` sites; a site on a regex literal stays; any other one becomes a named error (`houserules.crash-paths-are-named`).
- `crates/houserules/src/rules/audit.rs`: `audit()` is 222 lines and `run_check()` 168. T3 adds a check arm: put the `command` arm in its own function next to `run_report_field_check` and keep `run_check` a dispatch. T4 adds the report write: a separate function, not more lines in `audit()`.
- `crates/houserules/src/rules/check.rs`: `check_base()` is 235 lines. T3 adds shape rules for the new check fields: add them to the existing per-type shape function, not to `check_base()`.
- `crates/houserules/src/install.rs`: `upsert_topic_entries()` 131 lines, `update()` 107. T5 adds file baselines for the schemas, evals, and workflow: reuse the `KIT_OWNED` path through `baseline::classify`; do not add a third reconciliation loop.
- `crates/houserules/src/rules/stats.rs` and `proposals.rs`: no long function. T6 changes text and adds one list.
- No other smell found in the touched files.

## Parallelism

- T1 and T3 are parallel: disjoint files (`capture.rs` and `main.rs` for T1; `audit.rs`, `check.rs`, `check_shape.rs`, the knowledge schemas for T3). The controller owns `main.rs`'s count tests if both touch it: T3 adds no subcommand, so it does not.
- T2 follows T1 (it reads the capture header through T1's parser). T4 follows T3 (both change `audit.rs`). T6 is parallel with T2 and T4: its files are disjoint. T5 follows T4: the D4 red is a check at the gates' entry in `main.rs`, which T4 changes too.
- T11 (the default branch and the seeded durable-sha rule) follows T5: both change `template/knowledge/` topic files.
- T7 (templates, skills, evals) follows T1 to T4: it states their final commands and fields. T8 (eval run) and T9 (docs) follow T7. T10 (live proof) is last.
- At most three agents at once. The controller owns `backlog/`, the ledger, `render` output, the payload stamp, and integration.

---

## Task 1: `houserules capture` (HR-171)

**Files:** create `crates/houserules/src/capture.rs`; modify `crates/houserules/src/main.rs` (the subcommand, its help, the two surface tests); create `crates/houserules/tests/capture.rs`; `README.md` (one command row).

**Interfaces produced:** `capture::Header { command, cwd, started, ended, exit, head, dirty, encoding }`; `capture::parse(path) -> Result<(Header, Body), String>`; `capture::sha256_of(path) -> Result<String, String>`. T2 consumes all three.

- [ ] **Step 1: Failing tests** in `tests/capture.rs`, each driving the built binary:
  - `capture_writes_the_header_and_the_output` (a command printing two lines; every header key present; body equal);
  - `capture_exits_with_the_commands_exit_code` (`sh -c 'exit 3'` on Unix, `cmd /C exit 3` on Windows, `#[cfg]`-gated per `houserules.platform-gated-tests`);
  - `capture_records_a_command_that_cannot_start` (exit 127, header `exit: 127`, the error text in the body);
  - `capture_marks_non_utf8_output_lossy` (a command writing `0xff 0xfe`; `encoding: lossy: 2 invalid bytes replaced`);
  - `capture_records_dirty_and_head` (a scratch repository with one commit, then an untracked file);
  - `capture_prints_the_report_entry_as_json` (stdout is one JSON line whose `sha256` matches the file);
  - `capture_cwd_is_relative_to_the_repository_root` (run from a subdirectory).
  RED captured `> <ws>/t1-red.txt 2>&1`.
- [ ] **Step 2: Implement** `capture.rs`: `started` and `ended` from `jiff::Timestamp::now()` rounded to whole seconds, written in its RFC 3339 `Display` form and read back with `str::parse` (spec D9; verify each call on docs.rs for the vetted version and record it in `docs_verified`). Spawn with `std::process::Command` (no shell), merge stdout and stderr in arrival order, write the header, then the body. One named error per failure path (cannot create `--out`, not inside a repository).
- [ ] **Step 3: GREEN** into `<ws>/t1-green.txt`.
- [ ] **Step 4: Surface tests.** Update the two `main.rs` surface tests to 21 subcommands with `capture` in the list. Help text: one short `about`, as every other subcommand.
- [ ] **Step 5: Gates, live run** (capture three real commands of this repository, one failing; show each file and the JSON line), **commit** `feat(cli)!: houserules capture runs a command and writes its evidence file` with a `BREAKING CHANGE:` footer naming the new subcommand.

## Task 2: evidence by reference in the schema and the checker (HR-171, HR-057, HR-174, HR-148)

**Depends on:** T1 merged into the batch branch.

**Files:** both `deliverables.json` copies; `crates/houserules/src/report_claims.rs` and new submodules under `crates/houserules/src/report_claims/`; `crates/houserules/src/rules/validate_deliverable.rs`; `crates/houserules/tests/check_report_claims.rs`; the schema parity tests that pin the `run` definition.

- [ ] **Step 1: Schema.** `run`: required `command`, `exit`, `capture`, `sha256`; optional `excerpt` with `maxLength: 4000`; `output` removed. Applies to `tests`, `live_run`, `tdd[].red`, `tdd[].green`, `fix_rounds[].tests`.
- [ ] **Step 2: Failing tests** for each new checker rule (spec 4.2): a missing capture; a hash mismatch; a header `command` or `exit` unlike the entry's; an excerpt not in the body; a `natural` RED whose `head` is outside the range or whose `ended` is later than the first descending commit; a 1.x report (an entry with `output`) gets the single named line. HR-174: a `docs_verified[].source` naming `/tmp/` or a `scratchpad/` segment is flagged. HR-148: `file.rs:23,24,999` flags line 999 past the end.
- [ ] **Step 3: Split, then change.** First move the existing checks into submodules with no behavior change (tests stay green: a pure move). Then delete `check_redirected_captures`, `check_truncation_markers`, `check_paste_run_lint`, and the capture-time arm of `natural_red`, with their tests, and add the new rules. The module doc's Limits list loses every bullet the deleted checks owned.
- [ ] **Step 4: `validate`** on a 1.x report prints the named line from spec 4.6 instead of the per-field errors.
- [ ] **Step 5: Corpus run** (`houserules.plan-semantics-run-on-the-corpus`): run the new checker over every retained 1.x report: each must produce exactly the single named line, nothing else. Capture the run.
- [ ] **Step 6: Gates, commit** `feat(schema)!: report evidence cites capture files by path and hash` with a `BREAKING CHANGE:` footer.

## Task 3: the `command` check type and report-field content checks (HR-173, HR-175, HR-139, HR-176)

**Files:** both `knowledge/schema.json` copies; `crates/houserules/src/rules/check_shape.rs`; `crates/houserules/src/rules/check.rs` (shape rules); `crates/houserules/src/rules/audit.rs` (the `command` arm, the report-field options, the evidence text); README (the check-type table).

- [ ] **Step 1: Schema.** `$defs/check/properties/type` gains `command`. New properties: `run` (array of strings, `minItems: 1`), `append_files` (boolean), `non_empty` (boolean), `covers_ids` (boolean), `requires` (array of regex strings). `check-knowledge` shape rules: `run` only with `command`; `non_empty`, `covers_ids`, `requires` only with `report-field`; `requires` only on a list field.
- [ ] **Step 2: Failing tests** in `audit.rs` `mod tests`: a passing and a failing `command` check (a script fixture); `append_files` appends only the matching changed files; `skipped` with a reason on a dirty matching file and on HEAD unlike `--head`; `non_empty` fails each of `null`, `""`, `[]`, `{}`; `covers_ids` names the missing id; `requires` names the unmatched pattern; the workspace evidence reads `set in <t> of <r> reports`.
- [ ] **Step 3: Implement**, each arm its own function (code-health scan).
- [ ] **Step 4: Kit entries.** `process.knowledge-first`: `non_empty: true`, `covers_ids: true`. `security-hygiene.dependency-vetting`: `non_empty: true`. `houserules.live-run-recipe` (this repository only): `requires` with the recipe's two commands (HR-139). `security-hygiene.verify-current-docs` unchanged (spec 4.4, corpus run m3).
- [ ] **Step 5: Corpus run.** Re-run m3's measurement through the real audit over the retained workspaces with `--workspace`; the counts must match m3 (1 `covers_ids` fail, 0 `non_empty` fails on the two entries). Capture it.
- [ ] **Step 6: Gates, live run, commit** `feat(schema)!: a command check type and report-field content checks`.

## Task 4: the audit writes a report's facts (HR-158, HR-138 (1))

**Depends on:** T3 merged (both change `audit.rs`).

**Files:** `crates/houserules/src/rules/audit.rs`; `crates/houserules/src/main.rs` (the `--report` help); README.

- [ ] **Step 1: Failing tests:** `audit --report <file>` writes `commits` (sha and subject of every commit in the range), `files_changed`, and `self_audit` (the summary and the deterministic rows of this run) into the file, and keeps every other field byte-equal; a second run with no new commit leaves the file byte-equal; a report path that does not exist is a named error.
- [ ] **Step 2: Implement** as one function that the `audit()` tail calls.
- [ ] **Step 3: Gates, live run, commit** `feat(cli)!: audit --report writes the report's commits, files, and self-audit`.

## Task 5: schema, eval, and workflow delivery (spec 5.1, HR-162, HR-151)

**Depends on:** T4 merged (both change `main.rs`). D3, ruled: baselines. D4, ruled: the named red (spec 5.2), in the same task because both read the stamp in `update` and the gates.

**Files:** `crates/houserules/src/install.rs`, `crates/houserules/src/baseline.rs`, `crates/houserules/src/main.rs` (the D4 check at each gate's entry), a test file for the D4 red, `crates/houserules/tests/update.rs`, `crates/houserules/tests/install.rs`; every `template/knowledge/*.json` topic file (HR-162: stored in emit form).

- [ ] **Step 1: Failing tests:** `update` on an install with an unchanged 1.4 schema rewrites it to 2.0 and stamps a baseline; a modified schema is kept and the summary names the hand step; the same for an eval file and the seeded workflow; `update` right after `init` leaves every knowledge file byte-equal (HR-162); an unstamped install's prefix remedy names `--id-prefix` (HR-151 (2)); an overridden deleted entry stays deleted after the next `update` (HR-151 (4)).
- [ ] **Step 1b: Failing tests for D4:** with a stamp below 2.0.0, `check-knowledge`, `check-backlog`, `audit`, `validate`, and `check-report-claims` each print the one named line of spec 5.2 and exit 2; with no stamp, the same.
- [ ] **Step 2: Implement** through `baseline::classify`, with the payload rendered for the install's `idPrefix` before hashing.
- [ ] **Step 3: Gates, commit** `feat(install)!: update delivers the schema files, evals, and CI workflow`.

## Task 11: the default branch and the seeded durable-sha rule (spec 5.3, HR-131, HR-178)

**Depends on:** T5 merged (both change `template/knowledge/` topic files); D6, ruled.

**Files:** `crates/houserules/src/rules/durable_sha.rs`; its tests; `template/knowledge/knowledge-base.json` (the seeded `knowledge-base.cite-durable-refs`); `knowledge/knowledge-base.json` through `update`; README (the `.houserules.json` keys).

- [ ] **Step 1: Failing tests:** a `defaultBranch` key names a branch that exists: the check reads it before every fallback; the key names a missing branch: one named error that names the key and the branch; no key and no fallback: the row is `skipped` with the line that names the key; a restamp by `update` keeps the key; a fresh `init` seeds the entry with `files` `backlog/**`, `knowledge/**`, `docs/specs/**`.
- [ ] **Step 2: Implement, gates, commit** `feat(cli): the durable-sha check reads defaultBranch; the kit seeds cite-durable-refs`.

## Task 6: `stats` across the release (spec 4.6, HR-152, HR-164)

**Depends on:** D1, ruled (b): HR-152 is in.

**Files:** `crates/houserules/src/rules/stats.rs`, `proposals.rs`, `cost.rs`, `README.md`, `crates/houserules/tests/stats_workspaces.rs`.

- [ ] **Step 1: Failing tests:** one run over a 1.4 fixture workspace and a 2.0 fixture workspace counts both; HR-152: a sanctioned fail row goes to a `sanctioned` list, not `violations`; HR-164 (1): the `budget` reason when `ruled_keep` exceeds the budget; HR-164 (2, 3): the README sample and the `is_soft` doc comment match the code.
- [ ] **Step 2: Implement, gates, commit.**

## Task 7: templates, skills, and knowledge text (spec 4.5)

**Depends on:** T1 to T4 merged. D1 (b) and D5 (a), ruled: the rider set is HR-129 (1, 3, 4), HR-153 (1-4), HR-165 (4-7), HR-166, HR-150. The standing-entry clauses of HR-153 wait for the spec review (D10).

**Files:** the three agent templates, the orchestrating, finishing-a-feature, and migrating-knowledge skills, the four eval scenarios, the knowledge entries the riders name; root copies through `update`; `render` output; the payload stamp.

- [ ] Every rider sentence from spec 4.5, each quoted in the dispatch brief from its backlog item.
- [ ] The migrating-knowledge skill's linter recipes (spec 4.3) and the upgrade pointer to `docs/upgrading-to-2.md`. Its "Generate your own CI gate" section is checked against the 2.0 commands (spec 5.4, D11).
- [ ] HR-166 (1): the until-clause pattern becomes `(?:[^.;:]|\.\d){0,40}` in both copies, with the phrase "until 1.3.0 ships" added to the test's phrases.
- [ ] Commit `feat(template)!: agents cite capture files; the riders of batches 25 to 32`.

## Task 8: eval run (controller)

After T7: run every `.claude/evals/` scenario per the orchestrating skill's Template evaluation section. Pass the changed template bodies verbatim in each dispatch: the session loaded the old ones. Append one run set to `.claude/evals/record.json`.

## Task 9: docs and the 2.0 surface record

**Files:** `docs/upgrading-to-2.md` (new; it tells an adopter with a hand-written CI gate to raise the pinned version after `update`, spec 5.4), `README.md`, `knowledge/` and `template/knowledge/` `houserules.1-0-surface-is-frozen` body (the 21 subcommands, the 2.0 schema blob ids, the additive rule of D2), `docs/design.md` (the 2.0 surface item), `docs/runbook.md` (the release step for a major).

## Task 10: live proof (controller; spec 4.6)

The matrix of spec 4.6: installs seeded at 1.0.0, 1.2.0, 1.4.0, each in four states, plus one unstamped install; each updated by the branch binary; the gates on a new-format report; a second `update` that changes nothing. Plus the D4 behavior on an un-migrated tree. Plus spec 5.3 on an install whose default branch is `trunk`: the seeded durable-sha row without the key (`skipped`, the named line) and with it (judged). Then batch close (stats, branch review, loop proposals), Finish, and the PR.
