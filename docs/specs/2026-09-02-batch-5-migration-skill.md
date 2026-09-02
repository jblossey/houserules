# Batch 5: the knowledge-migration skill, and the settings read

Date: 2026-09-02. Status: approved by the owner on 2026-09-02, as written.
Driver: backlog batch 5 — HR-013 (owner request of 2026-09-01), HR-014
(batch 4 deferral). Branch `batch-5` from `2a535f6`.
Process: implementer on sonnet, task review on opus, branch review on fable;
one tier up after two endpoint drops; strictly sequential. Workspace
`.superpowers/sdd/2026-09-02-batch-5/` with the ledger `progress.md`.

## 1. Goal

Ship the skill that walks a pre-existing codebase from scattered knowledge
(CLAUDE.md prose, docs, ad-hoc rules files, comments) into `knowledge/*.json`
entries in the houserules style, and finish the bin's last inconsistent
project-data read.

## 2. Out of scope

- HR-015 to HR-018 (batch 6 candidates), HR-003 follow-ups, publishing,
  adoption.
- No new dependency; the skill is markdown in the payload.

## 3. Facts

- Kit-owned skills live in `template/.claude/skills/<name>/SKILL.md`, ship
  through `KIT_OWNED` in `bin/houserules.mjs`, and are dogfooded at the root
  (`tests/dogfood.test.mjs` iterates `KIT_OWNED`, so parity is automatic
  once the list grows; the `files` manifest and init assertions move,
  RED first).
- The knowledge style the skill teaches is already written down:
  `knowledge/schema.json` (kinds, areas, summary ≤ 160, source, verify,
  check) and the `knowledge-base.*` rules (summary-is-the-rule,
  state-only-the-source, ids-are-permanent).
- `mergeSettings` in `bin/houserules.mjs` still hand-parses
  `.claude/settings.json` with its own wording (`not readable JSON`); the
  stamp path reads through `readJson` since batch 4. The existing test
  asserts exit 2 and `toContain('settings.json')` only.

## 4. Tasks, in order, one Conventional Commit each

### T1 `feat(template): a skill that migrates existing knowledge into the kit` (HR-013)

- Create `template/.claude/skills/migrating-knowledge/SKILL.md` (kit-owned):
  frontmatter `name: migrating-knowledge`, a `description` that triggers on
  "migrate knowledge", "existing CLAUDE.md", "adopt houserules in an
  existing project". Sections:
  1. **When**: after `init` in a codebase that already has knowledge.
  2. **Inventory**: where knowledge hides — CLAUDE.md prose, `docs/`,
     wikis and READMEs, code comments that state constraints, PR and
     review templates, lint and CI configs that encode rules.
  3. **Classify**: one line per kind — rule, invariant, gotcha, procedure,
     decision, history — with the schema's semantics.
  4. **Write entries**: summary is the rule (≤ 160, no time-sensitive
     phrasing); body why/how/exceptions; `source` with date and by;
     `verify` paths that exist; a `check` where the rule is deterministic
     (grep-absent, commits, co-change, report-field). State only what the
     source states; never invent an instruction.
  5. **Areas**: extend the enum in `knowledge/schema.json` and the globs in
     `knowledge/areas.json` together.
  6. **Migrate topic by topic**: one topic file per pass; park what is not
     worth an entry (`backlog/parked.json` with a reason); delete the
     migrated originals — knowledge lives once; CLAUDE.md keeps only
     identity and the two seeded sections.
  7. **Gates**: `tools/kb.sh render`, `tools/kb.sh check`,
     `tools/backlog.sh check`; the next branch audit carries the new rules.
  8. **A worked example**: one CLAUDE.md paragraph transformed into one
     complete entry, before/after.
- `bin/houserules.mjs`: `KIT_OWNED` gains the skill path (RED first: the
  `files` manifest and init assertions move; dogfood parity is automatic).
- README: the "Apply to an existing project" section points to the skill
  (one sentence).
- Live run: scratch `init` ships the skill; then a mini-migration in the
  scratch repository follows the skill's own steps — take a two-rule
  CLAUDE.md paragraph, produce one entry, extend an area, park one
  leftover, delete the original, both gates green; each command its own
  report entry with the scratch path and exit.
- TDD note: markdown plus an ownership-list change; the list change is
  RED-first; the prose has no natural RED (say so; the live-run
  mini-migration is the evidence).

### T2 `fix(cli): read settings.json through readJson` (HR-014)

- `mergeSettings` reads with `readJson`: invalid JSON in
  `.claude/settings.json` becomes `UsageError('<path>: invalid JSON (...)')`
  (one line, exit 2); an unreadable file (a permissions or directory
  defect) propagates as a plain Error — the recorded semantics decision,
  consistent with the stamp and with `readJson`'s contract.
- Tests, RED first: the existing settings test's message expectation moves
  from `not readable JSON` to the `readJson` wording (it already asserts
  exit 2 and the file name); a companion pins the plain-Error defect path
  (the batch 4 `EISDIR` idiom).
- Live run: scratch `init`; corrupt `.claude/settings.json`; `init` again →
  one line naming the file, exit 2; restore; ok.

## 5. Reviews and finish

Per batch: task review (opus) with `AUDIT_JSON`; every finding fixed or a
reasoned backlog item; branch review (fable) with `--workspace`; the
retrospective applied; ticks; one to five clean commits; commitlint and the
audit over the range; CLI ff-only merge. T1 changes no implementer or
task-reviewer template and no scenario, so `process.evals-rerun` demands no
record change (the audit enforces it either way).

## 6. Code-health scan (files this batch touches)

- `bin/houserules.mjs`: `mergeSettings` is the last hand parse of project
  data (T2 removes it); the ownership lists absorb one more path (T1).
- `template/.claude/skills/`: three skills today, each one file; the new
  one follows the same one-file shape. No smell in the touched tests.
