# Batch 4: the evals-rerun port, and polish

Date: 2026-09-01. Status: approved by the owner on 2026-09-01, as written (T5 included).
Driver: backlog batch 4 — HR-010, HR-011, HR-012, HR-003 (unblocked: tag-pilot
PR #42 is merged). Branch `batch-4` from `4043d75`.
Process: implementer on sonnet, task review on opus, branch review on fable
(`process.model-policy`), strictly sequential; one tier up after two endpoint
drops. Workspace `.superpowers/sdd/2026-09-01-batch-4/` (git-ignored) with the
ledger `progress.md`. Decision 4 stays deferred (new trigger: the first
release or publish; §5.4 updated 2026-09-01).

## 1. Goal

Port tag-pilot's evaluation re-run rule so the agent templates get a
regression suite with a tracked record, and close the three polish items the
batch 3 reviews filed: the stamp file's raw SyntaxError, the lost git stderr,
and the two dispatch-protocol gaps.

## 2. Out of scope

- tag-pilot adoption (deferred, §5.4) and publishing (§5.3).
- Backporting houserules improvements into tag-pilot (their work, their repo).
- The tag-pilot-only scenario `rust-test-near-coverage` (never ported, by
  design).
- New dependencies: none; `template/` stays on built-ins.

## 3. Facts verified on 2026-09-01

- tag-pilot fc3241b is the only novelty since the port base: the entry
  `process.evals-rerun` (area `docs`, routed through `.claude/**`; check
  `co-change`: a change to the implementer or task-reviewer template or to
  `.claude/evals/*.json` without a change to `.claude/evals/record.json`
  fails the audit), a "Template evaluation" section in the orchestrating
  skill (detached scratch worktree at the branch head; implementer scenarios
  through the implementer on sonnet with the scenario's `query` as the brief
  and its `knowledge` as the Knowledge line; `seeded-violations` through the
  task-reviewer on opus on a fixture from its `setup`; judge every
  `expected_behavior`; append one run set to `record.json` with `date`,
  `templates` blob ids via `git rev-parse HEAD:<path>`, `runs`), and one
  branch-reviewer sentence (compare the record's `templates` blob ids with
  HEAD; a mismatch is a critical finding under `process.evals-rerun`).
- Our `kb.mjs` and both schema copies already carry the `co-change` check
  type. Our three seeded scenarios match tag-pilot's; the template ships no
  `record.json` yet, and `.claude/evals/record.json` is in neither ownership
  list of `bin/houserules.mjs`.
- `bin/houserules.mjs` `install` still parses the stamp with raw
  `JSON.parse(readFileSync(markerPath))`; the test "propagates an unreadable
  marker, which is a defect and not a usage error" pins that behavior and is
  today the only cover of `main`'s non-UsageError rethrow branch.
- `gitDiff` (template/tools/kb.mjs) discards the child's stderr and labels
  every failure `no merge base between "<base>" and "<head>"`.
- The root knowledge holds `process.brief-carries-the-spec` (batch 3); the
  kit seed does not yet.

## 4. Tasks, in order, one Conventional Commit each

### T1 `fix(cli): report an unreadable stamp file as one usage error` (HR-010)

- `install` reads the stamp with `readJson` from
  `template/tools/lib/json-store.mjs` (already imported from that module):
  invalid JSON in `.houserules.json` becomes `UsageError`
  (`<path>: invalid JSON (...)`), one stderr line, exit 2.
- Tests, RED first: the pinning test becomes "reports an unreadable stamp as
  one usage error" (returns 2; stderr names `.houserules.json` and matches
  `/invalid JSON/`; one line; no `    at `). Constraint: `main`'s rethrow
  branch keeps a covering test — find a real non-UsageError path through
  `main`, or a mocked one with the mock disclosed (the file already mocks
  `node:child_process`); state the choice in the report.
- Live run: scratch `init`; corrupt `.houserules.json`; `init --dir
  <scratch>` again → one line, `EXIT: 2`; restore; ok.

### T2 `fix(tools): carry the git stderr in the diff failure` (HR-011)

- `gitDiff` pipes the child's stderr. On failure: when the stderr names the
  missing merge base, the message stays
  `no merge base between "<base>" and "<head>"`; otherwise the `UsageError`
  carries the stderr's first line (fallback: the error message). Export
  `gitDiff` with JSDoc so both branches are unit-testable; sync the root
  copy.
- Tests, RED first: the orphan-branch test keeps its exact message; a new
  test reaches the other branch (for example an invalid pathspec through the
  `args` parameter → git's own `fatal:` line rides in the usage error).
- Live run: scratch orphan audit unchanged (one line, exit 2); the invalid
  input case shows git's own first line instead of a mislabel.

### T3 `docs(template): dispatch-protocol bullets and the brief rule in the seed` (HR-012)

- `template/.claude/skills/orchestrating/SKILL.md`, dispatch protocol, two
  bullets: a brief names every test, gate, and file its spec task lists,
  verbatim or by pointer (`process.brief-carries-the-spec`); when a batch
  edits an agent template, the dispatch message carries the changed
  instruction verbatim, because templates load at session start.
- `template/knowledge/process.json` gains `process.brief-carries-the-spec`
  (the root entry's text, a targeted insertion). `update --dir .`; render.
- Tests, RED first where a unit exists: the seed `checkBase` test is
  self-maintaining (it reads the directory) and must stay green; `tools/kb.sh
  validate`/`check` in a scratch `init` seeds the new entry (live evidence).

### T4 `feat(template): the evals-rerun rule with its co-change check and record` (HR-003)

- `template/knowledge/process.json` and `knowledge/process.json`:
  `process.evals-rerun`, adapted from tag-pilot fc3241b verbatim except the
  rename map (`implementer.md`, `task-reviewer.md`, unprefixed agents; read
  the entry in `~/projects/tag-pilot/knowledge/process.json` and keep its
  `check` object's shape exactly).
- `template/.claude/evals/record.json` seeded as `[]`; `SEED_ONCE` in
  `bin/houserules.mjs` gains `.claude/evals/record.json` (RED first: the
  ownership-list tests and `files` output change).
- `template/.claude/skills/orchestrating/SKILL.md`: the "Template
  evaluation" section (renamed). `template/.claude/agents/
  branch-reviewer.md`: the blob-id comparison sentence (renamed), appended
  to the rule-adherence paragraph. `update --dir .`.
- Verify the `docs` area of both `areas.json` copies routes `.claude/**` so
  the entry rides every template-touching audit without `--ids`; state the
  finding in the report.
- Live run: scratch `init` seeds the record and both checks pass; in this
  repository, `tools/kb.sh audit --base <batch base> --head HEAD` carries the
  new rule once `.claude/evals/record.json` exists at the root (T5 below
  writes it), and a probe change to a template without a record change fails
  the co-change in a scratch repository.

### T5 (controller, after T4's review clears): first evaluation run

Per the ported procedure: run the three scenarios at the branch head —
`docs-edit` and `dependency-add` through the `implementer` template
(sonnet), `seeded-violations` through the `task-reviewer` template (opus) —
in a detached scratch worktree; judge every `expected_behavior` line; append
the first run set to the root `.claude/evals/record.json` (`date`,
`templates` blob ids, `runs`) and commit it as `docs(evals): first
evaluation record`. Workspace artifacts carry the `eval-` prefix. This is
the port's live-run evidence and runs before the branch review. The
controller dispatches and judges; the record commit rides the branch.
The record commit also restores `.claude/evals/record.json` to the
`verify` list of `process.evals-rerun` in both process.json copies and
re-renders: T4 omitted the path because `kb check` asserts verify paths
exist and the root record is born here (ruling of 2026-09-02 on the task 4
review).

## 5. Reviews and finish

After each task: task review with `AUDIT_JSON`; every finding fixed or
deferred as a backlog item with a reason. After T5: branch review (fable)
over the whole branch with `--workspace`, including the record-vs-HEAD
blob-id check T4 adds to its own template. Then `finishing-a-feature`
without its push and PR steps (no remote): backlog ticked, one to five clean
commits, commitlint plus the audit over the range, ff-only merge from the
CLI, in an `&&`-only chain with no destructive step behind an unverified
command.

## 6. Code-health scan (files this batch touches)

- `bin/houserules.mjs`: the stamp read parses project data raw (T1 removes
  it); `mergeSettings` keeps its own parse-and-wrap — filed as HR-014 with
  its semantics decision (correction of 2026-09-01; the scan first called
  the stamp the last raw parse). The ownership lists are plain
  arrays; adding one seed file is a one-line change plus test updates.
- `template/tools/kb.mjs` `gitDiff`: the catch swallows the failure cause
  (T2); exporting it adds one symbol with JSDoc, in line with the file's
  other exported helpers.
- `template/.claude/skills/orchestrating/SKILL.md` grows two bullets and one
  section; still one screen per concern. `template/.claude/agents/
  branch-reviewer.md` gains one sentence in an existing paragraph.
- `tests/init.test.mjs` and `tests/kb.test.mjs` keep their existing fixture
  helpers; no new duplication is expected. No other smell found in the
  touched files.
