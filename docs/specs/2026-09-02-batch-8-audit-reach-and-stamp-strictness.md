# Batch 8: audit reach, stamp strictness, and the eval polish

Date: 2026-09-02. Status: approved by the owner on 2026-09-02, as written
(HR-029 stays a designed trap with a judged criterion).
Driver: backlog batch 8 — HR-025, HR-026 (batch 7 reviews), HR-027, HR-028
(batch 7 task 1 reviewer notes, owner-included at the gate), HR-029,
HR-030 (batch 7 branch review). Branch `batch-8` from `e2d2c2b`.
Process: implementer on sonnet, task review on opus, branch review on
fable; one tier up after two endpoint drops; strictly sequential.
Workspace `.superpowers/sdd/2026-09-02-batch-8/` with the ledger
`progress.md`.

## 1. Goal

Give every check an audit loading path and pin the hand-synced schema
copies, tighten the stamp to the strictness the owner chose, and polish
the eval suite — the scenario ambiguity and the template's one-sentence
field list — paying for the template edits with one shared run.

## 2. Out of scope

- Publishing, adoption, extending globToRegExp, wider stamp schema
  validation beyond `idPrefix` (HR-027's scope is the one field).
- No new dependency; the payload stays on built-ins and POSIX shell.

## 3. Facts

- The two `deliverables.json` copies are SEED_ONCE and hand-synced; they
  differ only in the backlogId pattern (`HR-` vs `WI-`) by design. Batch 7
  synced five fields on diligence alone; they are provably identical
  today except that pattern.
- The audit package builder (`template/tools/kb.mjs:709`) admits standing
  entries and area-matched rule/invariant kinds only, so the report-field
  check on `houserules.live-run-recipe` (kind procedure, area cli) joins
  a package only via `--ids`; the batch 7 branch audit silently lacked
  the row. `knowledge-base.rules-need-a-loading-path`'s body documents
  the interim state and names HR-026.
- `readJsonObject` accepts any plain object, so a stamp holding an
  `idPrefix` that is a number passes the guard and flows into id
  generation.
- `install` rewrites every KIT_OWNED file before it reads the stamp, so a
  corrupt `.houserules.json` exits 2 with the target already updated
  (pre-existing since batch 4).
- The dependency-add eval query names "the project's tooling package",
  which collides with `houserules.payload-runs-on-builtins` in this
  repository; the batch 7 run navigated it with a deviation disclosure,
  but the judge cannot tell a designed trap from an accidental ambiguity.
- `implementer.md`'s Report section is one long sentence after the batch
  7 contract fields landed; each change re-inflates it.
- A change to `.claude/evals/*.json` or `implementer.md` triggers
  `process.evals-rerun`; T5 and T6 both do, so one shared run (T7) covers
  the batch and each interim audit carries the expected fail row,
  disclosed in concerns (the batch 7 pattern, now in the rule's body).

## 4. Tasks, in order, one Conventional Commit each

### T1 `test(tests): pin the SEED_ONCE deliverables schema copies` (HR-025)

- A test compares `.claude/schemas/deliverables.json` with
  `template/.claude/schemas/deliverables.json` modulo the one designed
  difference (the backlogId pattern), so any other drift fails the suite.
- The copies are in sync today, so no natural RED exists: a disclosed
  mutation (introduce a one-field drift, RED, restore byte-identical,
  GREEN) proves the test can fail; `mode: "mutation"`.

### T2 `fix(tools): every check gets an audit loading path` (HR-026)

- The package builder admits any area-matched entry that carries a
  `check` (extend the one condition at `template/tools/kb.mjs:709`; no
  second list). Standing entries and rule/invariant kinds behave as
  before.
- Tests, RED first (natural): a checked procedure entry in a matched
  area joins the package; an unchecked procedure still does not.
- Update `knowledge-base.rules-need-a-loading-path`'s body in the same
  commit: the checked-procedure clause drops its "until HR-026" interim
  wording and states the new rule (a check loads via `standing`, a
  matched area, or `--ids`). Root-only entry; render with the change.
- Root sync via `update --dir .`. Consequence the reviewer verifies: the
  batch's own later audits (T3/T4 touch `bin/**`, area cli) now carry
  the live_run report-field row without `--ids`.

### T3 `fix(cli): the stamp validates idPrefix as a string` (HR-027)

- After `readJsonObject`, the stamp read requires `idPrefix`, when
  present, to be a non-empty string; anything else raises
  `UsageError('<path>: invalid idPrefix')` — one stderr line, exit 2.
  `readJsonObject` itself stays generic (two call sites, one contract).
- Tests, RED first (natural): a stamp with a numeric `idPrefix` flows
  into id generation today; after, one line and exit 2. A stamp without
  `idPrefix` keeps the default (unchanged).
- Live run: scratch init; doctor the stamp's idPrefix to a number; init
  → one line, exit 2; restore; ok; both gates.

### T4 `fix(cli): install reads the stamp before writing` (HR-028)

- `install` validates the stamp (existence, object shape, `idPrefix`)
  before it writes any KIT_OWNED file; a corrupt stamp exits 2 with the
  target untouched.
- Tests, RED first (natural): today a corrupt stamp exits 2 with the
  target already updated; after, the target's files are byte-identical
  to before the failed run.
- Live run: scratch init; corrupt the stamp; snapshot the tree; init →
  exit 2 and the snapshot matches; restore; ok; both gates.

### T5 `chore(evals): the dependency-add collision is a judged criterion` (HR-029)

- Keep the query as the designed trap and add one `expected_behavior`
  line to `.claude/evals/dependency-add.json`: the implementer must
  place the dependency outside the zero-dependency payload and disclose
  the collision as a deviation — the navigation batch 7's run performed
  becomes an explicit judged criterion.
- Data-only: gates as evidence. Triggers `process.evals-rerun` (T7).

### T6 `chore(template): implementer.md lists one report field per line` (HR-030)

- The Report section's field list becomes one line per field; wording
  unchanged — structure only. Root sync via `update --dir .`; dogfood
  parity is the gate. Data-only: gates plus a diff-stat proof that no
  word changed (only whitespace/line structure). Triggers
  `process.evals-rerun` (T7).

### T7 `docs(evals): third evaluation record` (process.evals-rerun)

- After T6: every scenario in a detached scratch worktree at the branch
  head, per `process.eval-fixture-procedure` for the seeded fixture;
  implementer scenarios on sonnet, seeded-violations through
  task-reviewer on opus; judge every `expected_behavior` including T5's
  new criterion; append the run set keyed by both template blob ids;
  worktree removed; `eval-` artifacts in the batch workspace.

## 5. Reviews and finish

Per batch: task review (opus) with the full
`Knowledge:`/`Backlog:`/`--ids` block on every round; every finding fixed
or a reasoned backlog item; branch review (fable) with `--workspace`;
retrospective applied; ticks; one to five clean commits; commitlint and
the audit over the range; CLI ff-only merge. The hook gates both harness
trailers; evidence-producing shell sequences are `&&`-chained
(process.gate-shell-chains).

## 6. Code-health scan (files this batch touches)

- `template/tools/kb.mjs`: the package condition is one boolean — extend
  it, do not fork it; the builder stays in one place.
- `bin/houserules.mjs`: `install`'s write-then-validate order inverts;
  keep one validation path (the shared helper plus the one idPrefix
  check), no second stamp parser.
- `tests/`: the schema-copy comparison belongs beside
  `tests/dogfood.test.mjs`'s parity concerns; reuse its style, not a new
  harness.
- `.claude/evals/dependency-add.json`: one added line; the scenario's
  shape is otherwise stable.
- `template/.claude/agents/implementer.md`: structure-only edit; the
  diff must show moved words, not changed ones.
