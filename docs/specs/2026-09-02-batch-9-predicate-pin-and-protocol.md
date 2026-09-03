# Batch 9: the shared predicate, the scenario pin, and the protocol wording

Date: 2026-09-02. Status: approved by the owner on 2026-09-02, as written.
Driver: backlog batch 9 — HR-031 (ruled 2026-09-02: unify), HR-032
(batch 8 T5 review), HR-033, HR-034 (batch 8 branch review). Branch
`batch-9` from `58e2d3f`.
Process: implementer on sonnet, task review on opus, branch review on
fable; one tier up after two endpoint drops; strictly sequential; the
audit `--ids` on every round are the dispatch's Knowledge ids. Workspace
`.superpowers/sdd/2026-09-02-batch-9/` with the ledger `progress.md`.

## 1. Goal

Close the two residual code gaps (one idPrefix predicate everywhere; the
seeded scenario copies pinned) and write the two lessons three batches
have paid for into the shipped templates and protocol, with one shared
evaluation run.

## 2. Out of scope

- Publishing, adoption; wider stamp validation beyond idPrefix; any
  restructuring beyond the named sentences.
- No new dependency; the payload stays on built-ins and POSIX shell.

## 3. Facts

- The owner ruled HR-031 on 2026-09-02: the flag's regex
  (`/^[A-Z][A-Z0-9]{0,7}$/`) validates idPrefix in both the `--id-prefix`
  check and the stamp guard — one shared predicate, one message.
- No gate pins `.claude/evals/{dependency-add,docs-edit,seeded-violations}.json`
  to their template sources; batch 8 hand-synced the dependency-add pair.
  `record.json` differs by design (root holds run sets; the template
  ships the seed).
- Ruling 18's first enforcement (batch 8 T4) cost a Critical, a
  sanctioned amend, and a fix round; neither the rule body nor the
  template bullet names the decisive case (a recapture against pre-fix
  code after the fix exists).
- Task-brief boilerplate carries a hand-filled `--ids` line nothing pins
  to the Knowledge list (batch 8 T5); the retrieval protocol implies the
  equality but never states it. Controller guard catches have no
  mandated ledger line (three batches of evidence).
- `implementer.md` changes and retrieval-protocol changes in
  `template/tools/kb.mjs` both require a scenario re-run
  (process.evals-rerun body); T3 and T4 share one run (T5).

## 4. Tasks, in order, one Conventional Commit each

### T1 `fix(cli): one idPrefix predicate for the flag and the stamp` (HR-031)

- Extract the flag's regex into one shared predicate (one constant or
  helper, doc-commented) used by the `--id-prefix` check and the stamp
  guard; one message shape for both rejections. A hand-edited stamp can
  no longer carry a prefix `init` would refuse.
- Tests, RED first (natural): a stamp with `idPrefix: "wi"` (and one
  with `"TOOLONGPREFIX"`) passes today and flows onward; after, one
  usage-error line, exit 2. The HR-027 tests move to the shared message
  where applicable; the flag tests stay green.
- Live run: scratch corrupt-and-restore through `init` and `update` with
  a lowercase prefix; split gates; tree untouched on failure (the T4
  order holds).

### T2 `test(tests): pin the seeded eval scenario copies` (HR-032)

- A sync test beside the deliverables-schema pin covers the three
  scenario copies byte-exact (no designed difference exists for them);
  `record.json` excluded by design, stated in the block comment.
- The copies are in sync today: disclosed mutation, `mode: "mutation"`,
  per quality.pin-copies-byte-exact.

### T3 `fix(template): the timing-key example lives in the tdd bullet` (HR-033)

- `template/.claude/agents/implementer.md`, the tdd/mode line: add the
  decisive example — a RED re-captured against pre-fix code after the
  fix exists is `reconstructed`; `natural` is chronology, not code
  state. One sentence, generic, root synced via `update --dir .`.
- Data-only; gates as evidence. Expected-red co-change until T5.

### T4 `fix(template): the protocol states --ids IS the Knowledge list` (HR-034)

- Orchestrating skill (template + root), dispatch section: the audit
  `--ids` value IS the dispatch's Knowledge list, generated from it,
  never typed separately — for round 0 and every re-review alike.
- One skill sentence: every guard-caught controller slip gets a ledger
  line when it happens (process.gate-shell-chains evidence).
- The retrieval protocol text in `template/tools/kb.mjs` states the same
  `--ids` equality (its step 3 already names the audit command). Root
  copies synced. Data-only; gates as evidence. Expected-red co-change
  until T5 (the retrieval-protocol change is invisible to the check but
  the rule body demands the run — T5 covers it).

### T5 `docs(evals): fourth evaluation record` (process.evals-rerun)

- After T4: every scenario in a detached scratch worktree at the branch
  head per process.eval-fixture-procedure; implementer scenarios on
  sonnet, seeded-violations through task-reviewer on opus; judge every
  expected_behavior; append the run set keyed by both template blob ids;
  worktree removed; `eval-` artifacts in the batch workspace.

## 5. Reviews and finish

Per batch: task review (opus) with the full
`Knowledge:`/`Backlog:`/`--ids` block on every round (the ids are the
Knowledge list); every finding fixed or a reasoned backlog item; branch
review (fable) with `--workspace`; retrospective applied; ticks; one to
five clean commits; commitlint and the audit over the range; CLI ff-only
merge. Every controller shell sequence gated; guard catches logged as
they happen.

## 6. Code-health scan (files this batch touches)

- `bin/houserules.mjs`: two idPrefix validators become one shared
  predicate — extract, do not duplicate; the message unifies.
- `tests/dogfood.test.mjs`: the scenario pin sits beside the two
  existing pins and reuses their style; byte-exact, no normalizer.
- `template/.claude/agents/implementer.md` and the orchestrating skill:
  one added sentence each, no restructuring.
- `template/tools/kb.mjs`: the retrieval protocol gains one clause; the
  generated skill re-renders identically in targets.
