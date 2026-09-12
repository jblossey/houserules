# Batch 23 spec: the template cluster

Status: approved by the owner, 2026-09-12 (design.md 5.65),
as drafted.
Items: HR-078, HR-080, HR-090, HR-094, HR-095, HR-096, HR-103
(selected 2026-09-12, design.md 5.64) — three batches' worth of
template and process lessons landing in the shipped kit under
ONE shared evals rerun.
NOT in scope: the step-two release set (owner-attended, HR-068
first); HR-100/101/102/104/107 (code items, no template
surface); HR-072, HR-081/082, HR-088; the parked owner items.

## 1. Goal

Every lesson batches 20-22 recorded against the agent templates,
the orchestrating skill, and the kit's teaching prose ships to
adopters. The implementer and task-reviewer templates change, so
the whole `.claude/evals/` scenario set reruns once, before the
branch review (`process.evals-rerun`).

## 2. The template edits (HR-078, HR-080, HR-090, HR-094, HR-103)

All edits land in `template/.claude/...`, then `houserules
update --dir .` regenerates the root copies (KIT_OWNED). Drafted
sentences below ship as written unless the owner amends them.

**Implementer template** (`agents/implementer.md`):
- HR-078: "Edit a report by writing the new content to a temp
  file, running `houserules validate` on it, and moving it into
  place only when validation passes; never edit REPORT_FILE in
  place." (Origin: a jq slip briefly emptied a report.)
- HR-080: the closing-act false-positive branch names its
  destination: "report the vehicle upstream — an issue at
  github.com/jblossey/houserules." (Ruled 5.51.)
- HR-094(2) + HR-103(3), merged (same lesson, two batches):
  "On resume after an interruption, at the end of every fix
  round, and before the report ships: rewrite `implemented`,
  `self_review`, and `concerns` from the tree at HEAD,
  re-running each measurement. A sentence carried from a draft,
  a review, or a dispatch is re-derived before it lands."
- HR-103(4): "The rules your task exists to enforce count as
  relied-on; list them in `knowledge_used`."
- HR-090: the audit-invocation teaching gains `--sanctioned
  <rule>=<ref>` beside the existing flags.

**Task-reviewer template** (`agents/task-reviewer.md`):
- HR-094(3): "A closure claim's enumeration derives its pattern
  from the claim's SUBJECT via a spelling-discovery probe over
  the corpus; reject a closure keyed to one artifact's name."
- HR-090: the audit-invocation teaching gains `--sanctioned`.

**Branch-reviewer template** (`agents/branch-reviewer.md`):
- HR-103(2): after the workspace audit, compare the report
  count the audit's evidence names against the workspace's
  `task-*-report.json` listing; a mismatch invalidates every
  report-field row.
- HR-090: the audit-invocation teaching gains `--sanctioned`.

**Orchestrating skill** (`skills/orchestrating/SKILL.md`):
- HR-094(1): when the current branch changes generated output,
  run every controller gate through the tree binary (`cargo run
  --release --quiet --bin houserules --`); reinstall after
  merge.
- HR-103(5), batch-close checklist: every `task-*-report.json`
  in the workspace has a ledger row stating its close commit
  and round count before the branch review dispatches.

**HR-090's sweep widening**: the retained batch-21 sweep
(`sweep-audit-invocation-sentences.sh`) patterns widen so a
flag-only teaching sentence is caught; the widened sweep reruns
to zero over the tracked tree as the item's closure enumeration
and is retained in the batch workspace.

## 3. The task-id shape, declared once (HR-103(1))

`backlog/schema.json`'s deliverables side — the task-id shape
(`^\d+[a-z]?$` or as measured from the corpus) declares once in
`.claude/schemas/deliverables.json`, and the three filename
patterns in `rules/deliverable.rs` derive from it (or pin it via
a parity test that fails when any of the three diverges from
the declared shape). TDD: the parity test RED against a
deliberately diverged pattern (disclosed mutation), GREEN at
HEAD. Both schema copies (root + template).

Amendment (controller-ruled at T1 review, recorded for the batch
report): the corpus measure excludes pre-batch-18 ad-hoc
evidence names; the declared shape governs deliverables, and 11
historical batch-17 root-level audit files are deliberately
outside it — houserules stats on that workspace reads 14 audits
at HEAD (was 25), the excluded files being evidence, not
deliverables.

## 4. The two record riders (HR-095, HR-096)

- HR-095: one sentence in `report_claims.rs`'s Limits section:
  a narrative names an ephemeral path's class and cites the
  literal in a retained workspace capture; no checker escape.
  Comment-only; the batch-22 sweep scripts stay zero.
- HR-096: the nine pre-batch-21 "verified live at commit X"
  markers whose SHAs no clone resolves restate to event
  locators (batch/task/round), preserving each historical
  claim. Enumeration: the retained batch-21 sweep
  (`sweep-unreachable-shas.sh`), rerun before and after; nine
  before, zero unreachable after.

## 5. The evals rerun

After the template edits and before the branch review: every
scenario in `.claude/evals/` runs per the orchestrating skill's
Template evaluation section (implementer scenarios on sonnet,
seeded-violations through the task-reviewer on opus, detached
scratch worktrees, `eval-` prefixed artifacts); one run set
appends to `.claude/evals/record.json` with the template blob
ids at HEAD. The audit fails until record.json changes with the
templates — by design.

## 6. Task shape

- T1: HR-103(1) task-id shape + parity test; HR-095; HR-096
  (code + records; sonnet implementer, opus review).
- T2: every template/skill edit (§2) + `update --dir .` + the
  HR-090 sweep widening and rerun (sonnet implementer, opus
  review). Dispatches carry the changed instructions verbatim —
  running sessions hold stale copies until restart.
- Then the controller-run evals rerun (§5), the batch live run
  (fresh init in a scratch repo; the regenerated templates
  present and byte-identical to template/), the fable branch
  review, aggregation to 1-5 commits, the finish protocol.
- No spec-level open questions: the drafted sentences above are
  the proposals; amend at the gate or approve as drafted.
