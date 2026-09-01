# Batch 3: exact gates and a sharper agent layer

Date: 2026-09-01. Status: approved by the owner on 2026-09-01, as written (all five tasks).
Driver: backlog batch 3 — HR-005, HR-009, HR-006 (with ruling 7,
`docs/design.md` §5.8), HR-008, HR-007; all filed by the batch 2 retrospective
on 2026-09-01. Branch `batch-3` from `3cc86e6`.
Process: implementer on sonnet, task review on opus, branch review on fable
(`process.model-policy`), strictly sequential; when an endpoint drops twice,
the implementer moves one tier up and its reviewer with it (ruling of
2026-09-01, batch 2). Workspace `.superpowers/sdd/2026-09-01-batch-3/`
(git-ignored) with the ledger `progress.md`.

## 1. Goal

Make the kit's gates say what the rules say — the audit enforces the commit
body limit, diffs the branch against its merge base, and judges report fields
over a whole workspace — and remove the last noise and gaps the batch 2
reviews found in `init` and in the agent templates.

## 2. Out of scope

- HR-003 (evals-rerun port): blocked on tag-pilot PR #42. The kit has no
  eval re-run rule yet, so T5's template edits carry no eval run; the branch
  review reads the templates.
- Publishing (§5.3) and tag-pilot adoption (§5.4): deferred rulings.
- New dependencies: none. `template/` stays on Node built-ins and POSIX shell
  (`houserules.payload-runs-on-builtins`).

## 3. Facts verified on 2026-09-01

- `bin/houserules.mjs` `install` runs `execFileSync(process.execPath,
  [<target>/tools/kb.mjs, 'render'], { cwd, encoding })` with the default
  stdio; on failure Node echoes the child's stderr to the parent's stderr and
  throws. `tests/init.test.mjs` "lets a render failure on broken project data
  propagate as a real error" asserts `toThrow()` only, so the child's one
  usage-error line lands in the suite output.
- `template/tools/kb.mjs`: `changedFiles` and `removedLines` run `git diff
  … base..head` (two dots, the tip-to-tip diff); `commitsIn` runs `git log
  base..head` (correct). The `commits` check tests `subject` and
  `body_absent` only; `checkShape` demands one of the two. The `report-field`
  check reads one `ctx.report` and returns `skipped` without `--report`.
  `stats(dir)` already lists `task-\d+-report.json`, `task-*-audit*.json`,
  `task-*-review*.json`.
- Schemas: `knowledge/schema.json` `$defs.check` is closed
  (`additionalProperties: false`) with the keys type, level, files, pattern,
  flags, scope, subject, body_absent, if, then, field.
  `.claude/schemas/deliverables.json` `$defs.run` is closed with `command`
  and `output`; `taskReport.files_changed` exists. Both schemas are
  seed-once: the root copy and the `template/` source are edited together.
  Agent templates and skills are kit-owned: edit `template/`, then
  `node bin/houserules.mjs update --dir .`.
- `process.conventional-commits` (both `process.json` files) carries the
  check `{type: commits, level: fail, subject: <pattern>}`; ruling 7 adds
  `body_line_max: 100` once the check exists.
- git 2.x: `git diff A...B` diffs from the merge base of A and B to B; the
  `--merge-base` flag is the newer spelling of the same thing.

## 4. Tasks, in order, one Conventional Commit each

### T1 `fix(cli): report a render failure in init as one usage error` (HR-005)

- `install`: spawn `render` with `stdio: ['ignore', 'pipe', 'pipe']`; on
  failure throw `UsageError` whose message is the child's stderr (trimmed;
  `error.stderr`), so `main` prints one `error` line and returns 2. Extract
  the spawn into a small `renderIn(target)` helper (code-health, §6).
- Tests, RED first: the broken-data init test becomes `main(['init', '--dir',
  dir], io)` returns 2, `io.stderr` names `knowledge/process.json` and
  matches `/invalid JSON/`, one line, no stack trace; the success path still
  prints the render summary. The full suite output no longer contains the
  child's line (state it in the report).
- Live run: scratch `git init`; `init --dir <scratch>` twice — once with a
  broken `knowledge/process.json` written before the second run (one line,
  exit 2), once after repair (ok, both checks).

### T2 `fix(tools): audit the diff from the merge base, not tip to tip` (HR-009)

- `changedFiles` and `removedLines` use `base...head` (three dots) through
  one `range(base, head)` helper; `commitsIn` keeps `base..head` (commits are
  already right). Doc comments say why.
- Tests, RED first: a temp repo where `main` gains a file after the branch
  was cut; `audit --base main --head branch` lists only the branch's files,
  and a `grep-absent` or `diff-append-only` rule on main's new file is not
  triggered. Existing audit tests stay green.
- Live run: this repository — `tools/kb.sh audit --base 9a15f95 --head HEAD`
  lists the same files before and after (linear history); the scratch repo
  from the test shape shows the difference (record both).

### T3 `feat(tools): enforce a commit body line limit in the commits check` (HR-006, ruling 7)

- `knowledge/schema.json` and `template/knowledge/schema.json`: `$defs.check`
  gains `body_line_max` (integer, minimum 1). `kb.mjs`: `CHECK_FIELDS.commits`
  stays `[]` (that table lists the keys a check type requires unconditionally;
  `body_line_max` is optional — ruling of 2026-09-01 on the task 3 review, the
  spec first said the table lists it); `checkShape` accepts a commits check
  that has any of `subject`, `body_absent`, `body_line_max` (message updated);
  the runner fails a commit with a body line longer than the limit (`commit
  "<subject>" has a body line over <n> characters`).
- `knowledge/process.json` and `template/knowledge/process.json`:
  `process.conventional-commits.check.body_line_max: 100` (ruling 7). Render.
- Tests, RED first: a 100-character body line passes, a 101-character line
  fails with the message; `checkShape` accepts `body_line_max` alone and
  rejects `0`; the seeded rule set (`template/knowledge`) still passes
  `checkBase`.
- Live run: scratch `init`; a commit with a 120-character body line →
  `tools/kb.sh audit --base HEAD~1` shows the fail row; a wrapped commit
  passes. In this repository `tools/kb.sh audit --base 9a15f95 --head HEAD`
  stays clean (all bodies were wrapped at finishing).

### T4 `feat(tools): audit report fields over a workspace of task reports` (HR-008)

- `audit` accepts `--workspace <dir>` (exclusive with `--report`; both →
  `UsageError`). It reads every `task-\d+-report.json` through the same
  listing `stats` uses (one `workspaceFiles(dir)` helper, code-health §6).
  A `report-field` check with a workspace: for each report whose
  `files_changed` matches `if`, the field must be set; the row fails naming
  the first report that lacks it (`task-2-report.json lacks dependency_vetting
  (triggered by package.json)`); when no report is triggered the evidence is
  `not triggered by any report`; a report without `files_changed` (a required
  field) is malformed and fails the row naming it (`<name> lacks
  files_changed`) — ruling of 2026-09-01 on the task 4 review; with neither
  `--report` nor `--workspace` the row stays `skipped` as today.
- Templates (kit-owned, then `update --dir .`): `branch-reviewer.md` runs the
  audit with `--workspace <WORKSPACE>` and its "skipped row" sentence states
  the new meaning (a skipped report-field row means the workspace was not
  passed); the orchestrating skill's branch review line names it.
- Tests, RED first: a workspace with two reports (one triggered and set, one
  triggered and missing) → fail naming the second; both set → pass; none
  triggered → the evidence above; `--report` unchanged; `--report` plus
  `--workspace` → usage error.
- Live run: `tools/kb.sh audit --base 9a15f95 --head 7ba8ebe --workspace
  .superpowers/sdd/2026-09-01-batch-2` — the four report-field rows are
  judged (pass), not skipped.

### T5 `docs(template): sharpen the deliverables schema and the agent templates` (HR-007)

- `.claude/schemas/deliverables.json` (root and `template/`): `$defs.run`
  gains optional `exit` (integer). `implementer.md` Report section: `tests`
  entries carry `exit` whenever the exit code is evidence (a rejected commit,
  a usage error) and `output` stays verbatim; `coverage` reads "one measure
  per target whenever the project has a coverage gate and the full suite
  ran, so the batch keeps a baseline; `null` only when the suite did not
  run". Orchestrating skill, dispatch protocol: "A brief names no version
  number for a tool, action, or package (`security-hygiene.exact-pins`); it
  names the verification the implementer runs and records in
  `docs_verified`, and shows placeholders such as
  `jdx/mise-action@<current major>`."
- Tests, RED first: `tools/kb.sh validate` accepts a report whose run has
  `exit: 2` and rejects `exit: "2"`; the dogfood parity test covers the
  template copies after `update --dir .`.
- Live run: scratch `init` seeds the new schema and templates; a report with
  `exit` validates there.
- Note for the owner: agent templates load at session start; the new
  wording applies from the next session.

## 5. Reviews and finish

After each task: task review with `AUDIT_JSON`; every finding fixed, Minor
included, or deferred as a backlog item with a reason. After T5: branch
review over `3cc86e6..HEAD` with `--workspace` (T4 makes it real) and the
retrospective. Then `finishing-a-feature` without its push and PR steps (no
remote): backlog ticked, one to five clean commits, commitlint over the range,
ff-only merge into `main` from the CLI, in a `&&`-only chain with no
destructive step behind an unverified command.

## 6. Code-health scan (files this batch touches)

- `bin/houserules.mjs` `install` (about 60 lines: file writes, settings
  merge, stamp, render): T1 extracts `renderIn(target)`; no further split.
- `template/tools/kb.mjs`: `changedFiles` and `removedLines` each build the
  git range (T2: one helper); `audit` builds its context inline (T4 adds the
  workspace reports through a helper shared with `stats`, which today lists
  the same files on its own — duplication removed); `checkShape`'s commits
  message hard-codes two keys (T3 updates it). 854 lines: long, but one
  concern per function; no split in this batch.
- `tests/kb.test.mjs` (1645 lines): organized by `describe`; new tests join
  the existing audit and check blocks. No split now.
- Schemas are closed objects: every new key is an explicit schema edit in
  two copies (root seed and template); T3 and T5 name both.
- No smell in `template/.claude/agents/*.md` beyond the wording T5 fixes.
