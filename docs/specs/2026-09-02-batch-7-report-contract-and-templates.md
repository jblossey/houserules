# Batch 7: the report contract, template tightenings, and one shared eval run

Date: 2026-09-02. Status: approved by the owner on 2026-09-02 with one
amendment: `live_run` (T3) and `mode` (T4) are required fields, not
optional; `verdict.text` (T5) stays optional.
Driver: backlog batch 7 — HR-022, HR-024 (batch 6 reviews), HR-016, HR-017
(batch 4 retrospective), HR-021, HR-023 (batch 5/6 retrospectives).
Branch `batch-7` from `16f68d5`.
Process: implementer on sonnet, task review on opus, branch review on fable;
one tier up after two endpoint drops; strictly sequential. Workspace
`.superpowers/sdd/2026-09-02-batch-7/` with the ledger `progress.md`.

## 1. Goal

Close the two small crash/vacuity gaps the batch 6 reviews found, then give
the report contract the three structured fields four batches of prose have
been paraphrasing (live_run, tdd mode, verdict.text), tighten the re-review
dispatch protocol, and pay for the template edits with one shared
evaluation run.

## 2. Out of scope

- Publishing, adoption, extending globToRegExp's vocabulary
  (houserules.glob-union-matcher documents the boundary).
- No new dependency; the payload stays on built-ins and POSIX shell.

## 3. Facts

- `install` reads the stamp with `existsSync(markerPath) ?
  readJson(markerPath) : { idPrefix: prefix }` and then touches
  `marker.idPrefix`; a `.houserules.json` holding `null` crashes with a raw
  stack at `bin/houserules.mjs:185` (verified live at batch 6 HEAD).
  `mergeSettings` got the plain-object guard in batch 6; the two sites
  now want one shared guard.
- An audit with no commits in range succeeds with vacuous deterministic
  rows ('0 commits checked', 'not triggered') that read as positive
  evidence; batch 6's report-only round needed hand annotation.
- Live-run evidence lives in prose inside `tests`; a missing scratch
  recipe is findable only by reading every entry (batch 4 task 2, batch 6
  task 4). The deliverables schema has `$defs.run` to reuse.
- A tdd cycle cannot say how it was produced; batch 4 task 4 spent two
  report rounds wording natural vs mutation vs reconstruction, and
  process.tdd now names all three forms plus before/after captures.
- `verdict.open` is a bare textList; batch 5's r1 used it for a status
  sentence plus new-breakage restatements, so 'findings-remain' misread.
- The batch 6 task 1 r1 re-review dispatch omitted `Backlog:` and
  `--ids`, narrowing that audit's package; nothing forces the block.
- Changing `implementer.md` or `task-reviewer.md` triggers
  `process.evals-rerun`: the audit fails until `.claude/evals/record.json`
  changes with them. T3–T6 all touch a template, so one run after T6
  covers the batch.

## 4. Tasks, in order, one Conventional Commit each

### T1 `fix(cli): reject a non-object stamp file as a usage error` (HR-022)

- One shared plain-object guard (a small named helper) serves both the
  stamp read and `mergeSettings`; `null`, a string, a number, a boolean,
  or an array in `.houserules.json` raises
  `UsageError('<path>: not a JSON object')` — one stderr line, exit 2.
- Tests, RED first: `null` in `.houserules.json` (today a raw stack);
  the existing HR-010 stamp tests and all batch 6 settings tests stay
  green through the helper extraction.
- Live run: scratch `init` ok; corrupt the stamp with `null`; `init` →
  one line naming the file, exit 2; restore; ok; both check gates.

### T2 `fix(tools): stamp an empty-range audit as vacuous` (HR-024)

- In `template/tools/kb.mjs`: when the range holds no commits, the audit
  summary gains `"empty_range": true` and every deterministic row's
  evidence is prefixed `empty range:`. Populated ranges are unchanged.
- `auditSummary` in both deliverables schema copies gains optional
  `empty_range`, enum `[true]`, so a copied `self_audit` summary
  validates; the key exists only when true. Scope confirmed at the task
  review (T2 review, spec_compliance extra 1).
- Tests, RED first: a base==head audit asserts the flag and one prefixed
  evidence string; an existing populated-range test stays green. Root
  sync via `update --dir .`.

### T3 `feat(template): a structured live_run field in task reports` (HR-016)

- `$defs.taskReport` in both deliverables schema copies gains REQUIRED
  `live_run` (owner amendment at the gate): an array of `$defs.run`
  entries (command, output, exit). The array may be empty for a docs-only
  task whose live evidence is the gates; the task review judges
  sufficiency, mirroring process.tdd's data-only clause.
- `houserules.live-run-recipe` gains a `report-field` check at `warn` on
  `live_run` (`knowledge/houserules.json` — a root-only topic; the
  houserules topic ships no seed copy).
- `implementer.md` (template + root): the Report section names the field
  as required — the scratch recipe's entries go in `live_run`; `tests`
  keeps suite and gate runs.
- Tests, RED first: a report without `live_run` now fails validation;
  every existing task-report fixture gains the field; a wrong shape
  fails; the audit's report-field row warns when the field is missing —
  presence, even as an empty array, passes the deterministic row (the
  review judges an empty array's sufficiency). Corrected after the T3
  review: the shipped behaviour and the brief pass on empty.

### T4 `feat(template): a mode field on tdd cycles` (HR-017)

- `$defs.tddCycle` in both schema copies gains REQUIRED `mode` (owner
  amendment at the gate), enum `natural | mutation | reconstructed`:
  every cycle names its provenance explicitly.
- `implementer.md` (template + root): one sentence — name `mode` on every
  cycle; `natural` only for a genuine pre-commit RED.
- Tests, RED first: a cycle without `mode` now fails validation; every
  existing tdd fixture gains the field; an unknown mode is rejected.

### T5 `feat(template): verdict.text for re-review prose` (HR-021)

- `$defs.reReview.verdict` in both schema copies gains optional `text`.
- `task-reviewer.md` (template + root): `open` lists only prior findings
  still unaddressed; new breakage lives only in `new_breakage`; prose goes
  in `verdict.text`.
- Tests, RED first: validate fixtures with and without `text`; an `open`
  misuse cannot be schema-checked — the template sentence is the fix.

### T6 `fix(template): re-review dispatches carry the round-0 block` (HR-023)

- Orchestrating skill (template + root): the re-review dispatch bullet
  requires the identical `Knowledge:`/`Backlog:`/`--ids` block as the
  round-0 dispatch.
- `task-reviewer.md` (template + root): the reviewer refuses to run an
  audit without `--ids` when the dispatch's Knowledge list is non-empty,
  and says so in the review instead of running a narrowed package.
- Docs-only: no natural RED; the gates and render drift are the evidence
  (process.tdd data-only clause).

### T7 `docs(evals): second evaluation record` (process.evals-rerun)

- After T6: run every scenario in `.claude/evals/` in a detached scratch
  worktree at the branch head — implementer scenarios through
  `implementer` (sonnet), `seeded-violations` through `task-reviewer`
  (opus) — judge every `expected_behavior`, append one run set to
  `.claude/evals/record.json` (date, both template blob ids at HEAD,
  runs). Workspace artifacts carry the `eval-` prefix; the worktree is
  removed after. The audit's co-change fails until the record changes.

## 5. Reviews and finish

Per batch: task review (opus) with `AUDIT_JSON` and the full
`Knowledge:`/`Backlog:`/`--ids` block on every round; every finding fixed
or a reasoned backlog item; branch review (fable) with `--workspace`;
retrospective applied; ticks; one to five clean commits; commitlint and the
audit over the range; CLI ff-only merge. The commit-msg hook now gates both
harness trailers on every commit. At aggregation, the T1 and T2 commit
bodies' TDD wording is corrected to match the reports' disclosures (a
post-commit recapture and a reconstruction, not "natural"/"RED first") —
ruled at the T1 and T2 reviews, deferred to the aggregation both reviews
name.

## 6. Code-health scan (files this batch touches)

- `bin/houserules.mjs`: the plain-object guard exists once inline in
  `mergeSettings`; T1 extracts the shared helper instead of a second
  inline copy.
- `template/tools/kb.mjs`: the audit summary is built in one place; T2
  extends it there, no second summary shape.
- `.claude/schemas/deliverables.json`: four new fields this batch —
  `live_run` and `mode` required (owner amendment), `verdict.text` and
  `empty_range` (T2) optional — reuse `$defs.run` and closed enums; no
  new free-form objects.
- Agent templates and the orchestrating skill: each gains one to three
  sentences; no section restructuring.
