# Batch 6: dot-inclusive globs, honest audit evidence, hardened reads, hook gate

Date: 2026-09-02. Status: approved by the owner on 2026-09-02, as written
(T4 gates both harness trailers).
Driver: backlog batch 6 — HR-019, HR-018, HR-020 (batch 5 branch review),
HR-015 (batch 4 branch review). Branch `batch-6` from `c3e763a`.
Process: implementer on sonnet, task review on opus, branch review on fable;
one tier up after two endpoint drops; strictly sequential. Workspace
`.superpowers/sdd/2026-09-02-batch-6/` with the ledger `progress.md`.

## 1. Goal

Make the audit see every file it should (dot-segments under `**`), state its
co-change evidence honestly, turn the last crash-with-stack in the bin into a
usage error, and gate harness trailers at commit time for this repository and
every consumer.

## 2. Out of scope

- HR-016, HR-017, HR-021 (batch 7: they edit agent templates and share one
  evals-rerun), publishing, adoption.
- No new dependency anywhere; the payload stays on Node built-ins and POSIX
  shell (`houserules.payload-runs-on-builtins`).
- No automatic `git config core.hooksPath` in `init` (documented instead).

## 3. Facts

- `node:path` `matchesGlob` excludes dot-segments under `**`:
  `matchesGlob('template/.claude/skills/migrating-knowledge/SKILL.md',
  'template/**')` is false (verified live in the batch 5 branch review).
  Every batch 5 audit therefore ran without the `template` area, silently
  dropping `houserules.payload-runs-on-builtins` from the package.
- `template/tools/kb.mjs` calls `matchesGlob` twice: `areaFiles` (~line 106)
  and `matchAny` (~line 435, used by the `co-change`, `diff-append-only`,
  and file-scoped check runners). Both share the defect for `**` globs that
  must cross a dot-segment.
- The `co-change` runner (~line 538) picks `trigger[0]` from the `if`
  matches and the first `then` match as evidence; when the only changed
  `if` match is the `then` file itself, the evidence reads
  "record.json changed with record.json" (circular, HR-018).
- `mergeSettings` (`bin/houserules.mjs:104`) reads through `readJson`,
  which accepts any valid JSON; `settings.hooks ??= {}` then crashes with a
  plain `TypeError` stack when `.claude/settings.json` holds `null`, a
  string, or a number (HR-020, pre-existing).
- `.githooks/commit-msg` runs commitlint only; a harness-injected
  `Co-Authored-By` trailer reached a batch 4 commit and was caught by hand.
  The harness also injects `Claude-Session:`; the owner's standing position
  is no trailers of any kind (`security-hygiene.no-coauthor`, owner brief).
- The payload copier chmods `0o755` only for files ending in `.sh`
  (`bin/houserules.mjs:94`); a hook file has no extension.

## 4. Tasks, in order, one Conventional Commit each

### T1 `fix(tools): match dot-segments in area and check globs` (HR-019)

- Add one shared dot-inclusive matcher in `template/tools/kb.mjs` (built-ins
  only: keep `matchesGlob` and also match with dot-segments made visible, or
  a small glob-to-RegExp with `**` crossing separators, `*` within one
  segment, dotfiles included; the implementer picks and records the
  mechanism in `docs_verified` against the current Node 24 docs).
- Both call sites use it: `areaFiles` and `matchAny`. No other behavior
  moves.
- Tests, RED first: `areasFor(['template/.claude/agents/implementer.md'],
  areas)` includes `'template'`; a `matchAny`-level case where a `**` glob
  crosses a dot-segment (a `co-change` `then` such as `src/**` against
  `src/.config/x.json`); existing glob tests stay green.
- `mise exec -- node bin/houserules.mjs update --dir .` syncs the root copy;
  target repositories inherit the fix via `update`.
- Live run: in a scratch repo with an area glob `src/**` and a changed
  `src/.hidden/file`, `tools/kb.sh for src/.hidden/file` prints the area's
  rules and an audit includes the area package; before the fix it does not.

### T2 `fix(tools): name a record-only co-change plainly` (HR-018)

- In the `co-change` runner: when the only `if` match is the `then` path,
  the evidence reads `only <then> changed; the co-change is satisfied by
  definition`; otherwise the listed trigger excludes the `then` path.
  Pass/fail behavior unchanged.
- Tests, RED first: the record-only case asserts the new evidence line; a
  mixed case (template + record changed) asserts the trigger is the
  template, not the record.

### T3 `fix(cli): reject a non-object settings.json as a usage error` (HR-020)

- After `readJson`, `mergeSettings` requires a plain object: `null`, a
  string, a number, a boolean, or an array in `.claude/settings.json`
  raises `UsageError('<path>: not a JSON object')` — one stderr line,
  exit 2, same convention as every other project-data read.
- Tests, RED first: `null` crashes with a `TypeError` stack today; after,
  one line and exit 2. One more case for an array.
- Live run: scratch `init`, write `null` into `.claude/settings.json`,
  `init` again → one line naming the file, exit 2; restore; ok.

### T4 `feat(template): ship the commit-msg hook with a trailer gate` (HR-015)

- New `template/.githooks/commit-msg` (POSIX sh): reject any line matching
  `^(Co-Authored-By|Claude-Session):` case-insensitively — one error line
  naming the offending trailer, exit 1 — then run commitlint only when it
  is available (`command -v` guard), so a consumer without commitlint still
  gets the trailer gate.
- `KIT_OWNED` gains `.githooks/commit-msg`; the copier chmod condition
  covers hook files (not only `*.sh`), asserted by a mode test.
- This repository's root `.githooks/commit-msg` becomes the installed copy
  (dogfood parity via `tests/dogfood.test.mjs`); `core.hooksPath` is
  already `.githooks` here.
- README (consumer side): one sentence — set
  `git config core.hooksPath .githooks` to activate the gate.
- Tests, RED first: manifest/init assertions move for the new `KIT_OWNED`
  path; the mode test fails before the chmod condition moves.
- Live run: in a scratch repo with `core.hooksPath .githooks`, a commit
  with a `Co-Authored-By` trailer is rejected with one line; the same
  commit without it passes; a `Claude-Session:` trailer is rejected.

## 5. Reviews and finish

Per batch: task review (opus) with `AUDIT_JSON`; every finding fixed or a
reasoned backlog item; branch review (fable) with `--workspace`; the
retrospective applied; ticks; one to five clean commits; commitlint and the
audit over the range; CLI ff-only merge. No implementer or task-reviewer
template and no scenario changes, so `process.evals-rerun` demands no record
change (the audit enforces it either way). T1 lands first so the batch's own
audits run with the repaired matcher.

## 6. Code-health scan (files this batch touches)

- `template/tools/kb.mjs`: two `matchesGlob` call sites duplicate the
  matching idiom (T1 unifies them behind one helper); the `co-change`
  evidence builds strings inline (T2 touches only the wording).
- `bin/houserules.mjs`: the chmod special-case list grows; keep it one
  predicate, not a second list (T4). `mergeSettings` gains its last
  missing input class (T3); no hand parse remains.
- `.githooks/commit-msg`: two lines today, no shellcheck coverage; T4's
  template copy enters `mise run lint`'s shellcheck scope.
