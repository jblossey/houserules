# Batch 10: scratch-dir hygiene, the status tie, and the widened ledger bullet

Date: 2026-09-03. Status: approved by the owner on 2026-09-03, as written.
Driver: backlog batch 10 — HR-035 (batch 9 T2 review, ENOSPC census),
HR-036, HR-037 (batch 9 branch review, template defects). Branch
`batch-10` from `35ac91b`.
Process: implementer on sonnet, task review on opus, branch review on
fable; one tier up after two endpoint drops; strictly sequential; the
audit `--ids` on every round are the dispatch's Knowledge ids. Workspace
`.superpowers/sdd/2026-09-03-batch-10/` with the ledger `progress.md`.

## 1. Goal

Stop the test suite from leaking its scratch directories and give the
rule a loading path; close the two template defects the batch 9 review
filed, with one shared evaluation run.

## 2. Out of scope

- Publishing, adoption; any change to the eval scenarios themselves;
  any restructuring beyond the named sentences.
- No new dependency; the payload stays on built-ins and POSIX shell.

## 3. Facts

- tests/ has 21 `mkdtempSync` call sites across seven files and none
  removes its directory; at the batch 9 T2 review /tmp held 1714
  orphaned `houserules-target-*` trees plus 89 `kb-*` and 15 `ws-*`,
  and a transient inode exhaustion broke one suite run (HR-035).
- The HR-035 rider (batch 9 branch review): land the knowledge entry
  `houserules.tests-clean-scratch-dirs` with the retrofit, so the rule
  has a loading path the moment the call sites are fixed. Root-only
  entry, like quality.pin-copies-byte-exact: the seed has no tests
  area, and seeding a tests-area entry broke fresh-install gates once.
- implementer.md defines DONE_WITH_CONCERNS only for the final chat
  message; batch 9 produced two reports whose file says DONE while the
  answer and the ledger say DONE_WITH_CONCERNS (HR-036).
- The ledger bullet shipped in batch 9 binds only slips "that a gate or
  && chain catches"; the gap showed the day it shipped — an amend
  caught by inspection went unlogged, and a second formatting slip in
  the finish repeated the class (HR-037).
- An implementer.md change requires a scenario re-run
  (process.evals-rerun); T2 triggers it and T4 pays it. The rule body
  now carries the merge-block clause: the branch does not merge before
  the record commit lands. No scenario covers the orchestrating skill,
  so T3 adds no run of its own.

## 4. Tasks, in order, one Conventional Commit each

### T1 `test(tests): the suite cleans its mkdtemp scratch dirs` (HR-035)

- One shared fixture helper under tests/ that mints a scratch directory
  and registers its removal at the call site
  (`onTestFinished`/`afterEach` with
  `rmSync(dir, { recursive: true, force: true })`); all 21 call sites
  across the seven test files move to it. Test behavior unchanged.
- TDD per the timing key: prove the cleanup with a verbatim RED and
  GREEN or a disclosed-mutation proof; the decisive live-run evidence
  is a before/after count — a full suite run adds zero new scratch
  directories to the temp root.
- Same task: add `houserules.tests-clean-scratch-dirs` to root
  `knowledge/houserules.json` (kind rule, area tests; the body names
  the census and states the suite leaves the temp root as it found
  it), run `tools/kb.sh render`, and prove the loading path
  (`tools/kb.sh for tests/<a file>` lists the id). Root-only; no
  template counterpart.

### T2 `fix(template): the report status ties to a non-empty concerns list` (HR-036)

- `template/.claude/agents/implementer.md`, the `status` bullet: one
  sentence — set `status` to DONE_WITH_CONCERNS whenever `concerns` is
  non-empty; the final answer message repeats the report's status
  verbatim. Generic, no repo token; no restructuring of the per-line
  field list. Root synced via `update --dir .`, byte-identical.
- Data-only; gates as evidence. Expected-red co-change until T4,
  disclosed in concerns.

### T3 `fix(template): the ledger bullet covers every controller slip` (HR-037)

- Orchestrating skill (template + root), Handling reviews: widen the
  bullet to every controller slip that forces a re-run, an amend, or a
  correction — gate-caught or not — logged when it happens. One
  sentence edit, no restructuring. Root synced via `update --dir .`.
- Data-only; gates as evidence. No scenario covers the skill; T4's
  record still follows (T2's trigger stands).

### T4 `docs(evals): fifth evaluation record` (process.evals-rerun)

- After T3: every scenario in a detached scratch worktree at the branch
  head per process.eval-fixture-procedure; implementer scenarios on
  sonnet, seeded-violations through task-reviewer on opus; judge every
  expected_behavior; append the run set keyed by both template blob
  ids; worktree removed; `eval-` artifacts in the batch workspace. The
  dependency-add scenario also exercises T2's sentence: its collision
  disclosure belongs in `concerns`, so the report's status must read
  DONE_WITH_CONCERNS under the new tie.

## 5. Reviews and finish

Per batch: task review (opus) with the full
`Knowledge:`/`Backlog:`/`--ids` block on every round (the ids are the
Knowledge list); every finding fixed or a reasoned backlog item; branch
review (fable) with `--workspace`; retrospective applied; ticks; one to
five clean commits; commitlint and the audit over the range; CLI
ff-only merge. Every controller shell sequence gated; every controller
slip logged as it happens, gate-caught or not (T3's own standard,
applied from the start of the batch).

## 6. Code-health scan (files this batch touches)

- tests/*.test.mjs: 21 duplicated mkdtemp idioms become one helper —
  extract, do not duplicate; no test asserts less than before.
- `template/.claude/agents/implementer.md` and the orchestrating skill:
  one sentence each, no restructuring.
- `knowledge/houserules.json`: one new entry with a loading path; the
  summary states the rule in one sentence, ≤160 characters.
