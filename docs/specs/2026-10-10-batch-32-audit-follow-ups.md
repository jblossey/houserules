# Batch 32: the adopter lead-time audit, minor-release half

Items: HR-167, HR-168, HR-169, HR-170. Owner rulings: design.md 5.94 (the split, build now, a minor release) and 5.95 (every major-version item waits for one 2.0.0 release after this one). Release: 1.4.0.

## 1. Source

An adopter audited its batch 3 under houserules 1.2.0: 11 tasks, 23 review rounds, 95 findings, 30 of them critical, 31% of 2,680 judged audit rows boilerplate. Release 1.3.0 already covers severity by impact, the bounded review loop, risk tiers, and parallel agents. This batch builds the rest of what needs no schema or surface change. Batch 33 (2.0.0) takes evidence by reference (HR-171, HR-057, HR-158) and a linter check type (HR-173).

## 2. Tier

Full: the batch changes `template/**` (`houserules.template-changes-run-full-tier`) and the `audit` command (`houserules.cli-changes-run-full-tier`). At most two re-reviews per task, a branch review, and an eval run (the implementer and task-reviewer templates change).

## 3. Design

### 3.1 Controller rules stay out of task audits (HR-168)

Measurement. Over this repository's 266 task reviews (7,806 judged rows), every kit standing rule sits in a glob-less area (10 in `global`, 23 in `process`). The approach proposed to the owner, filling in a row when no changed file matches the rule's area patterns, would remove no row. The corpus run (`houserules.plan-semantics-run-on-the-corpus`) disproved it before the plan.

The same run shows where the boilerplate is. Seven kit standing rules govern acts only the controller performs. Task reviews judged them 1,745 times with 0 fails:

| Rule | Judged in task reviews | Fails |
|---|---|---|
| `process.brainstorm-first` | 260 | 0 |
| `process.backlog-drives-work` | 260 | 0 |
| `process.code-health-scan` | 260 | 0 |
| `process.model-policy` | 260 | 0 |
| `process.live-run-before-ci` | 260 | 0 |
| `quality.well-maintained-libraries` | 131 | 0 |
| `process.owner-rulings-need-owner-supersession` | 54 | 0 |

Rules that task reviews do catch stay in: `process.rulings-to-file` (7 fails), `process.tdd` (11), `process.claims-match-artifacts` (51).

Design:

- An entry tagged `controller` governs the controller's acts: specs, plans, dispatch, rulings, merges, releases. A task diff cannot break it.
- A task audit is `houserules audit` with `--report`. In a task audit, an entry tagged `controller` joins the package only when `--ids` names it. The branch audit (`--workspace`) and a plain audit (neither flag) keep it.
- The kit tags the seven rules above. This repository also tags its own `process.ff-only-merges`. `houserules.controller-gates-use-the-tree-binary` already carries the tag.
- In a batch without a branch review, the controller runs the branch-range audit before Finish and judges its `controller` rows in the ledger.
- No schema change, no new flag: a tag is a free string, as `full-tier` and `ruled-keep` are. The `--report` help text and README state the rule.
- Adopter path: `update` rewrites a kit entry at its baseline, tag included. An entry the adopter changed or overrode keeps the adopter's copy; the adopter adds the tag by hand if wanted. An adopter entry that already carries `controller` with another meaning leaves task audits too; the release note says so.

### 3.2 Template defects and several workspaces (HR-167)

- `implementer.md`: the self-audit lives only in the report's `self_audit`. The implementer writes no `task-*` file other than `REPORT_FILE`; probe and capture output goes to non-`task-*` names. This mirrors `task-reviewer.md`.
- `implementer.md`: the `exit` field carries a command's exit status. A command never appends `; echo "EXIT=$?"` or any other echo of its status.
- `branch-reviewer.md` and the orchestrating skill: a batch may have more than one workspace. The dispatch names every one. The branch reviewer reads the deliverables of all of them and runs the workspace audit once per workspace, each writing `<that workspace>/branch-audit.json`. `stats` already takes several workspaces.

### 3.3 A loading path for non-standing process rules (HR-169)

A non-standing entry in the glob-less `process` area already has two loading paths: the controller reads every process entry's summary at each session start (the ritual's `houserules index --area process`), and an agent receives the ids on its `Knowledge:` line. The kit names these paths:

- `knowledge-base.rules-need-a-loading-path` (body) names the ritual's process index and the `Knowledge:` line as the loading paths of a non-standing process entry.
- A process rule is standing only when every session must carry it. A rule only the controller can break also gets the `controller` tag, which removes it from a task audit's package, standing or not (3.1; amended in 7).
- The orchestrating skill's ritual says why it lists the process index. The `migrating-knowledge` skill tells an adopter to keep a migrated process rule non-standing unless every session needs it, and to tag controller rules.

### 3.4 Findings name their caller and their check (HR-170)

- `task-reviewer.md`: a finding whose fix adds a capability names the caller that needs it, in `fix`. The implementer may contest a fix with no caller. The controller then drops the finding or files it.
- `task-reviewer.md`: a Minor finding's `fix` names the check that proves the fix, as a command and its expected result. For a Minor-only round, the controller runs that check and cites it in its verification note.
- `branch-reviewer.md`: re-run the check of at least one controller verification note per closed Minor-only round and report the result. A note whose check fails is a finding.
- `process.bounded-review-loop` and `process.fix-round-verification-record` state the reviewer-named check. `process.no-tech-debt` keeps "fix every finding": a dropped capability finding has no caller to fix for, and the controller records the drop in its note.
- `seeded-violations` eval: the fixture also commits a history-narrating code comment (`writing-style.code-comments`, a standing rule). Expected: filed at Minor, its `fix` naming a check.
- No schema change: the caller and the check go in the existing `fix` text.

## 4. Release and adopter path

- 1.4.0 through release-please: `feat` commits, no `!`, no surface change. No command, flag, or schema changes. `audit --report` drops a tagged entry from the package, which an adopter sees only through the kit's own tags.
- Migration: `houserules self-update && houserules update`. A second `update` changes nothing. The release note names the `controller` tag, the per-workspace branch audit, and the template sentences an adopter with a local fork should copy.
- The live run proves the path on a 1.3.0 install.

## 5. Out of scope

Evidence by reference, inline caps, and the checker's parsing gaps (HR-171, batch 33). A linter check type (HR-173, batch 33). Narrowing `process.rulings-to-file` (HR-172, owner ruling first).

## 6. Approval

The owner ruled the proposals and the split on 2026-10-10 (5.94): "file them with that batch split and implement batch 32 immediately". The mechanism of 3.1 replaces the area-pattern test from the proposal after the corpus run in 3.1 disproved that test. The goal and the surface (no schema change, no new flag) are unchanged, and the batch report names the change for the owner.

## 7. Changes after approval

- 3.3, Task 2: the sentence "standing only when every session and every task audit must carry it" contradicted 3.1, where seven rules are both standing and tagged `controller`. The shipped text says "standing only when every session must carry it"; the tag removes a rule from a task audit's package, standing or not. Accepted by the controller, 2026-10-10.
- 3.1, Task 2: the orchestrating skill's `Knowledge:` bullet keeps `controller` entries off a dispatch's line, because an id on that line reaches `--ids` and would bring the entry back into the task audit. Accepted by the controller, 2026-10-10.
- 3.4, Task 2: the branch reviewer re-runs the check of at least one finding in every Minor-only round's controller note. The orchestrating skill's check-running sentence covers every controller note; a mixed round keeps its closing rule. Accepted by the controller, 2026-10-10.
- 3.4, Task 2 fix round 1: a contested no-caller finding has three outcomes, not two. The contest holds: the controller drops the finding and records the reason in the ledger. Defer: a backlog item that the finding names. The contest fails: the finding stands, and the implementer fixes it in the next round. The task-2 review required the third arm (writing-style.instructions-cover-the-state-space). Accepted by the controller, 2026-10-10.
