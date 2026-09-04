# Batch 14: the hygiene sweep

Date: 2026-09-04. Status: approved by the owner on 2026-09-04, as written.
Driver: backlog batch 14 — HR-041 (batches 11/12 branch reviews),
HR-043 (batch 11 branch review), HR-050 (batch 13 branch review).
Branch `batch-14` from `4a7aeb4`.
Process: implementer on sonnet, task review on opus, branch review on
fable; one tier up after two endpoint drops; strictly sequential; the
audit `--ids` on every round are the dispatch's Knowledge ids.
Workspace `.superpowers/sdd/2026-09-04-batch-14/`. The finish
re-fetches and rebases onto main before the merge (standing owner
instruction).

## 1. Goal

The process tooling that will build the Tier-2 rewrite gets sharp
first: validate stops silently passing incomplete reports, reviewer
prescriptions state invariants, implementer evidence stays verbatim,
and one evaluation run covers both template changes.

## 2. Out of scope

- HR-047/048 (the Tier-2 spec follows this batch); HR-049 (waits on
  the tag-format ruling); HR-042 (waits on the owner's standing-check
  ruling).
- No dependency; the payload stays on built-ins.

## 3. Facts

- A task report with a terminal status and self_audit null passes
  validate (batch 11 T3's interrupted finalization); one with
  skipped>0 rows passes too (batch 12's fix-round audit without
  --report). Both cost review rounds that a gate would have caught.
- Batch 11 T2's r0 prescription named only the observed instance and
  the empty-string case survived a fix round; the r1 ruling charged
  the residual to the prescription's wording (HR-043).
- Batch 13 T2's captures appended a synthetic exit=0 line inside
  output, and its fix_rounds[0].findings listed four of five findings
  — both invisible to the tools (HR-050).
- implementer.md and task-reviewer.md changes trigger
  process.evals-rerun; T2 triggers it and T3 pays with the seventh
  record. The orchestrating-skill step (sha-map on rebuild) rides
  free.

## 4. Tasks, in order, one Conventional Commit each

### T1 `fix(tools): validate rejects incomplete terminal reports` (HR-041)

- `template/tools/kb.mjs` validate: a task-report whose `status` is
  DONE or DONE_WITH_CONCERNS fails validation when `self_audit` is
  null, and when `self_audit.summary.skipped` is greater than 0 (the
  message names the `--report` flag). BLOCKED/NEEDS_CONTEXT reports
  and the write-then-fill flow stay valid.
- Root sync via `update --dir .`. TDD: natural RED first for both new
  rejections (doctored report fixtures); the existing validate tests
  stay green; full suite and lint.
- Live run: validate against doctored copies of a real report (null
  self_audit; a skipped>0 self_audit; a BLOCKED report with null
  self_audit passing), per-command captures.

### T2 `fix(template): the template sentences from two retrospectives` (HR-043 + HR-050)

- `template/.claude/agents/task-reviewer.md`: (a) findings guidance —
  state each fix as the invariant to hold plus the observed instance;
  (b) re-review checklist — confirm the fix round's findings list
  matches the review's findings one for one before ruling
  all-addressed. One sentence each, no restructuring.
- `template/.claude/agents/implementer.md`, live_run guidance: output
  carries the command's stdout/stderr verbatim and nothing else; the
  exit field alone carries the exit status. One sentence.
- `template/.claude/skills/orchestrating/SKILL.md`: one rebuild step —
  on a mid-batch branch rebuild, write the old-to-new sha map to its
  own workspace file, cite it in the batch report, never edit
  closed-task deliverables.
- Each sentence checked against its state space
  (writing-style.instructions-cover-the-state-space). Root copies
  synced byte-identical. Data-only; expected-red co-change until T3,
  disclosed in concerns.

### T3 `docs(evals): seventh evaluation record` (process.evals-rerun)

- After T2: every scenario in a detached scratch worktree at the
  branch head per process.eval-fixture-procedure; the record keyed by
  both template blob ids; worktree removed; `eval-` artifacts in the
  batch workspace.

## 5. Reviews and finish

Per batch: task review (opus) with the full block on every round;
every finding fixed or a reasoned backlog item; branch review (fable)
with `--workspace`; retrospective applied; ticks; one to five clean
commits; fetch + rebase onto main, then ff-only merge from the CLI;
push; CI green.

## 6. Code-health scan (files this batch touches)

- `template/tools/kb.mjs` validate: two new rejection branches beside
  the existing shape checks — extend, no restructuring; messages in
  the existing usage-error voice.
- The two agent templates and the orchestrating skill: one sentence
  each in place.
- `tests/`: new validate cases beside the existing ones, minting
  scratch dirs through the shared helper.
