# Batch 32 implementation plan: the adopter lead-time audit, minor-release half

**Goal:** Ship HR-167, HR-168, HR-169, and HR-170 as 1.4.0.

**Spec:** `docs/specs/2026-10-10-batch-32-audit-follow-ups.md`. Executors read both.

**Tier:** full. Criteria: `template/**` changes (`houserules.template-changes-run-full-tier`) and `crates/houserules/src/**` changes (`houserules.cli-changes-run-full-tier`). Cap: two re-reviews per task.

**Workspace:** `.superpowers/sdd/2026-10-10-batch-32/` (ledger `progress.md`).

## Global constraints

- No subcommand, no flag, no schema file change (`houserules.1-0-surface-is-frozen`). Release: 1.4.0 through release-please (`feat` commits, no `!`).
- No standing entry's `summary` changes. A new kit entry is not standing.
- Edit the kit in `template/`, then `./target/debug/houserules update --dir .` (after `cargo build`) and `render` with the tree binary (`houserules.template-is-the-source`, `houserules.controller-gates-use-the-tree-binary`); regenerate `crates/houserules/payload.stamp` with `cargo run --bin payload-stamp-gate -- --write` (`houserules.payload-stamp-gate`).
- Seeded text stays harness-neutral (`houserules.agents-md-is-canonical`). Writing style: ASD-STE100 (`writing-style.principles`).
- Task-end gates mirror CI (`houserules.task-gates-mirror-ci`): `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `mise run lint`, `./target/debug/houserules check-knowledge`, `./target/debug/houserules check-backlog`.

## Review focus

1. A task audit with `--report` whose `--ids` names a `controller` entry: the entry stays in the package.
2. The branch audit (`--workspace`) and a plain audit: `controller` entries stay in the package.
3. A non-standing `controller` entry in a touched area or `global`: a task audit leaves it out unless `--ids` names it.
4. An adopter whose copy of a tagged kit entry is modified or overridden: `update` keeps the adopter's copy and says so once, as for any entry.
5. A template sentence keyed to one value (a Minor finding, a Minor-only round): read it against Critical and Important too (`writing-style.instructions-cover-the-state-space`).

## Code-health scan

- `crates/houserules/src/rules/audit.rs` (3,046 lines): a large file. Package selection is one loop in `audit()` (lines 896-916). The change adds one predicate there. Name it (`is_controller_entry`) and keep it next to the loop; do not split the file in this batch. Read the tag from `base.raw_entries`, as `proposals.rs` `is_ruled_keep` does (lines 185-193). Two tag readers then exist: extract one shared helper `has_tag(base, id, tag)` into `crates/houserules/src/rules/model.rs` or the module both already import, and use it in both places (DRY).
- `crates/houserules/src/main.rs`: the `--report` doc comment (lines 203-205) is the `--help` text; it changes with the behavior.
- `template/.claude/agents/implementer.md`: the `tests` and `live_run` bullets (lines 44-45) repeat the command-form rule. State the exit-field sentence once, in the paragraph at line 33, and keep the bullets as they are.
- `template/.claude/skills/orchestrating/SKILL.md`: the review-handling bullets at lines 88-89 overlap `process.fix-round-verification-record`. Add the reviewer-named check to line 89 only; do not repeat it in line 88.
- No other smell found in the touched files.

## Parallelism

T1 and T2 are parallel: their file sets are disjoint and neither needs the other's output. Each runs in its own worktree from the same `BASE`. At most two agents at once. The controller owns `backlog/`, the ledger, and the integration. T2 owns the generated rule files and skill, because only T2 changes knowledge. T1 does not run `render` or `update`.

---

## Task 1: `audit` leaves `controller` entries out of a task audit (HR-168, code)

**Files:**
- Modify: `crates/houserules/src/rules/audit.rs` (package loop, module doc, unit tests)
- Modify: `crates/houserules/src/rules/proposals.rs` (use the shared tag helper)
- Modify: the module that gets the shared helper (`crates/houserules/src/rules/model.rs` unless a better existing home is found; name it in the report)
- Modify: `crates/houserules/src/main.rs` (the `--report` doc comment)
- Modify: `README.md` (the `audit` description: one sentence on the `controller` tag)
- Test: unit tests in `audit.rs` `mod tests` (reuse `audit_entries()` and `audit_opts()`)

Not touched: `template/**`, `knowledge/**`, `.claude/**`, `crates/houserules/payload.stamp`, `crates/houserules/tests/dogfood.rs`.

**Behavior (spec 3.1):** when `opts.report` is `Some` (a task audit), an entry whose raw `tags` hold the exact string `controller` joins the package only when `opts.ids` names it. This holds for a standing entry, an entry in a touched area, and an entry with a check alike. With `--workspace`, or with neither flag, the package is unchanged.

- [ ] **Step 1: Failing tests.** In `audit.rs` `mod tests`, add a standing entry tagged `controller` (and one non-standing `controller` entry in `global`) to a fixture. Tests:
  - `a_task_audit_leaves_a_standing_controller_entry_out_of_its_package` (`--report` given: id absent from `ids`/`rules`);
  - `a_task_audit_keeps_a_controller_entry_that_ids_names`;
  - `a_branch_audit_keeps_controller_entries` (`--workspace` given);
  - `a_plain_audit_keeps_controller_entries` (neither flag);
  - `a_task_audit_leaves_a_global_controller_entry_out`.
  Run `cargo test -p houserules --bin houserules controller_entr > <ws>/t1-red.txt 2>&1` and keep the RED capture.
- [ ] **Step 2: Implement.** The shared tag helper, then the predicate in the package loop and after the `--ids` loop as needed (`--ids` always wins). Update the module doc's package description.
- [ ] **Step 3: GREEN.** The same command into `<ws>/t1-green.txt`.
- [ ] **Step 4: Help text and README.** The `--report` doc comment gains: "A task audit: an entry tagged `controller` joins the package only when `--ids` names it." README's `audit` text says the same in one sentence. Run `cargo test --bin houserules no_subcommand_carries_a_long_about_beyond_its_short_one` and any help golden test.
- [ ] **Step 5: Gates** (global constraints). Live run: `./target/debug/houserules audit --base <BASE>~5 --head <BASE> --report .superpowers/sdd/2026-10-09-batch-31/task-1-report.json` and the same range with `--workspace .superpowers/sdd/2026-10-09-batch-31` on this repository (read only). The root knowledge has `houserules.controller-gates-use-the-tree-binary` tagged `controller` today: show it absent from the first and present in the second. Each run captured `> <file> 2>&1` with its exit.
- [ ] **Step 6: Commit** `feat(cli): a task audit leaves entries tagged controller out of its package`.

## Task 2: kit text and knowledge (HR-167, HR-168 tags, HR-169, HR-170)

**Files:** `template/.claude/agents/implementer.md`, `template/.claude/agents/task-reviewer.md`, `template/.claude/agents/branch-reviewer.md`, `template/.claude/skills/orchestrating/SKILL.md`, `template/.claude/skills/migrating-knowledge/SKILL.md`, `template/knowledge/process.json`, `template/knowledge/quality.json`, `template/knowledge/knowledge-base.json`, `template/.claude/evals/seeded-violations.json`; their root copies through `update`; `knowledge/*.json` (this repository's own entries); `.claude/rules/*.md` and `.claude/skills/project-knowledge/SKILL.md` through `render`; `crates/houserules/payload.stamp`; `crates/houserules/tests/dogfood.rs` only if a pin breaks (name it in the report).

Not touched: `crates/houserules/src/**`, `README.md`, `backlog/`.

- [ ] **Step 1: Tags (spec 3.1).** Add the tag `controller` to the `tags` of `process.brainstorm-first`, `process.backlog-drives-work`, `process.code-health-scan`, `process.model-policy`, `process.live-run-before-ci`, `process.owner-rulings-need-owner-supersession` (`template/knowledge/process.json`) and `quality.well-maintained-libraries` (`template/knowledge/quality.json`). Change no summary.
- [ ] **Step 2: A new entry.** In `template/knowledge/knowledge-base.json`, add a non-standing rule `knowledge-base.controller-rules-are-tagged`, area as the file's other authoring entries (`docs`), summary: "Tag an entry `controller` when only the controller's acts can break it; a task audit leaves it out, the branch audit judges it." Body: the definition of a controller act (specs, plans, dispatch, rulings, merges, releases); `--ids` brings it back; the branch audit and a plain audit keep it; in a batch without a branch review the controller judges its `controller` rows of the branch-range audit before Finish; a rule that task reviews catch stays untagged. Source `by: controller`, date 2026-10-10, ref the spec.
- [ ] **Step 3: Loading paths (spec 3.3).** `knowledge-base.rules-need-a-loading-path` body: a non-standing `process` entry loads through the session ritual's `houserules index --area process` (the controller) and through the `Knowledge:` line (an agent); a process rule is standing only when every session and every task audit must carry it; a controller-only rule gets the `controller` tag. Keep the summary. Orchestrating skill ritual step 4: say that the process index loads every non-standing process rule for the controller. `migrating-knowledge` skill: where it decides `standing`, add both sentences.
- [ ] **Step 4: Implementer template (spec 3.2).** After line 33's sentence on `exit`: "Never append an echo of the exit status (`; echo \"EXIT=$?\"`) to a command." In the Report or self-audit section: "Your self-audit lives only in the report's `self_audit`. Write no `task-*` file other than `REPORT_FILE`: `houserules stats` reads every `task-*-audit*.json`, `task-*-review*.json`, and `task-*-report.json` in the workspace as a deliverable. Name probe and capture files without the `task-` prefix." Also add to the fix-round contract: a finding whose fix adds a capability with no caller may be contested in `concerns`; the controller rules.
- [ ] **Step 5: Task-reviewer template (spec 3.4).** In Calibration, after the `fix` sentence: "A `fix` that adds a capability names the caller that needs it. A Minor finding's `fix` also names the check that proves it: a command and its expected result." Keep every other sentence.
- [ ] **Step 6: Branch-reviewer template (spec 3.2, 3.4).** `WORKSPACE` may name several directories; read every deliverable in each; run the workspace audit once per workspace with `--json <that workspace>/branch-audit.json`. The `controller` rows of the branch audit are judged here; a task audit left them out. Re-run the named check of at least one controller verification note per Minor-only round and report the result; a failing check is a finding.
- [ ] **Step 7: Orchestrating skill (spec 3.1-3.4).** The branch review dispatch names every workspace of the batch. Line 89's controller note: for a Minor-only round, run the check each Minor finding names and cite the result. A finding contested for having no caller: drop it or file it, and record the drop in the note. A batch without a branch review: run `houserules audit --base <merge base> --workspace <WORKSPACE>` before Finish and judge its `controller` rows in the ledger. Name `knowledge-base.controller-rules-are-tagged` where the skill cites loading rules.
- [ ] **Step 8: Knowledge entries for the loop (spec 3.4).** `process.bounded-review-loop` and `process.fix-round-verification-record` bodies: the controller note for a Minor-only round runs the reviewer-named check. `process.no-tech-debt` body: a capability finding with no caller is dropped by the controller with a recorded reason, not fixed. No summary changes.
- [ ] **Step 9: Eval scenario.** `template/.claude/evals/seeded-violations.json`: the setup also gives the new module a comment that narrates history (`// added for batch 3`), and the query's `Knowledge:` adds `writing-style.code-comments`. New expected line: "The history-narrating comment is filed under writing-style.code-comments at minor, and its fix names a check (a command and its expected result)". Copy to `.claude/evals/seeded-violations.json` (both copies stay byte-equal; dogfood test).
- [ ] **Step 10: This repository's own entries.** Tag `process.ff-only-merges` `controller` in `knowledge/process.json`. Our `.houserules.json` overrides `process.brainstorm-first`, `process.model-policy`, `process.owner-rulings-need-owner-supersession`, and `quality.well-maintained-libraries`: `update` does not rewrite those, so add the tag to the root copies by hand. Confirm with `./target/debug/houserules index --tag controller` that nine entries carry it.
- [ ] **Step 11: Propagate and gates.** `cargo build`, `./target/debug/houserules update --dir .`, `render`, the payload stamp, then the global gates and `render --check`. Unfiltered grep over the tree for `EXIT=` and `one workspace` (`process.pointer-sweep-unfiltered`); each hit is correct or fixed. Live run: a scratch `git init`, `./target/debug/houserules init --dir <scratch>`, then `check-knowledge`, `check-backlog`, and `index --tag controller` inside it, each captured `> <file> 2>&1` with its exit.
- [ ] **Step 12: Commit** `feat(template): controller rules leave task audits; findings name their caller and their check`.

## Task 3: eval run (controller)

After T1 and T2 integrate: run every `.claude/evals/` scenario per the orchestrating skill's Template evaluation section (implementer scenarios on sonnet, `seeded-violations` on opus), judge every expected line, and append one run set to `.claude/evals/record.json` (both copies when the dogfood test requires it).

## Task 4: live run (controller)

1. Adopter path: build `main`'s 1.3.0 binary in a detached worktree, `init` a scratch repository with it, commit, then run the branch binary's `update --dir <scratch>`. Expect the seven tagged kit entries rewritten, no `kept` line, `check-knowledge` and `check-backlog` green; a second `update` changes nothing.
2. A task audit and a branch audit over the same range of this repository: the `controller` rows are gone from the first and present in the second.
3. Measure: judged rows in the batch's own task audits, with the controller rows left out.

Then batch close (stats, the branch review, the loop's proposals), Finish, and the PR.
