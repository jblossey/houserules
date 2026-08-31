# Batch 0: bootstrap — rename, license, hook matcher, dogfood

Date: 2026-08-31. Status: implemented on 2026-08-31 as backlog batch 1 —
commits 6721668 (T0), 7db7db9 (T1), d356fba (T2), a1d31ac (T3), 3bc142e (T4).
Approved by the owner on 2026-08-31 (the orchestrator implements; the
bootstrap exception of §6 accepted and recorded in backlog/decisions.json).
Driver: `docs/design.md` §5 (rulings 1, 2, 5, 6 of 2026-08-31). A backlog
does not exist yet; task 4 creates it.

## 1. Goal

Apply the four rulings that do not wait for external events, and install the
kit into this repository so that every later batch runs under the kit's own
rules and agent layer.

## 2. Out of scope

- The symlink entry-guard fix and the clean invalid-JSON error: executable
  code changes, done in batch 1 through the agent layer (see §6).
- The `process.evals-rerun` port: blocked until tag-pilot PR #42 is merged.
- Publishing (decision 3) and tag-pilot adoption (decision 4): deferred.

## 3. Tasks, in order, one Conventional Commit each

### T0 `docs(design): record the owner rulings of 2026-08-31`

Commit the §5 rewrite already on disk. Docs only; no test.

### T1 `refactor: rename lorekit to houserules`

- `git mv bin/lorekit.mjs bin/houserules.mjs`; `package.json` `name` and
  `bin`; `package-lock.json` through `npm install` (no version changes);
  vitest project name; `USAGE`; the stamp file `.lorekit.json` →
  `.houserules.json`; the two file-header comments in `template/tools/`;
  the provenance `"ref": "lorekit seed"` → `"houserules seed"` in every seed
  entry; the backlog seed sentence; README; `docs/design.md` prose (title,
  §3 command lines). The ruling text in §5.1 keeps `lorekit` as the former
  name.
- TDD: the tests change first (bin path, stamp file name) and fail (RED);
  the rename makes them pass (GREEN). Verbatim output goes into the commit
  body.
- Live run: `init --dir <scratch>` in a fresh `git init` repo, then
  `tools/kb.sh check && tools/backlog.sh check` there; the stamp file is
  `.houserules.json`.

### T2 `feat(template): session-start hook matches forked sessions`

- `template/.claude/settings.json`: matcher `startup|resume|clear` →
  `startup|resume|clear|fork`. README mention.
- TDD: a consumer test pins the seeded matcher (RED before, GREEN after).

### T3 `chore: mit license, spdx headers, package files whitelist`

- `LICENSE` (MIT text verified against the canonical source, "2026 Jannis
  Blossey"); `"license": "MIT"`; `private: true` stays until decision 3.
- Two header lines (`SPDX-License-Identifier: MIT`, copyright) in
  `bin/houserules.mjs`, `template/tools/*.mjs`, `template/tools/lib/*.mjs`,
  and after the shebang of `template/tools/*.sh`. No header in markdown or
  JSON payload. One README sentence: files that `init` and `update` write
  into a project are the project's under the same terms.
- `package.json` `files`: `bin`, `template`, `README.md`, `LICENSE`.
  Evidence: `npm pack --dry-run` lists no `tests/`, `docs/`, `mise.toml`,
  `vitest.config.mts`.
- Docs and data only; gates: `npm test`, the pack listing.

### T4 `chore: dogfood houserules in this repository`

- Parity test first: every path in `KIT_OWNED` exists at the root and is
  byte-identical to `template/` (RED: the root files are absent).
- `node bin/houserules.mjs init --dir . --id-prefix HR` (GREEN).
- `CLAUDE.md`: identity line and the two seeded sections; no knowledge in it.
- `knowledge/schema.json` area enum and `knowledge/areas.json`: `cli`
  (`bin/**`), `template` (`template/**`), `tests` (`tests/**`), `docs`
  (`docs/**`, `README.md`).
- `knowledge/houserules.json`, first entries: `houserules.template-is-the-
  source` (standing: edit `template/`, then `update --dir .`; never edit the
  root copies or generated files), `houserules.tag-pilot-read-only`
  (upstream reference only; diff before each release), `houserules.live-
  run-recipe` (procedure: scratch `git init`, `init`, both checks, the
  `git+file` `npm exec` check), `houserules.node-under-mise` (gotcha),
  `houserules.rename-map` (decision: names and fields relative to
  tag-pilot). Each entry states only what its source states.
- Backlog: `backlog/items/kit.json` with `HR-001` symlink entry-guard fix,
  `HR-002` invalid-JSON usage error, `HR-003` evals-rerun port (blocked on
  PR #42); `backlog/parked.json`: publishing, tag-pilot adoption, plugin
  complement, each with its reason; `backlog/batches.json`: batch 0 (this
  spec) and batch 1 (HR-001, HR-002); `backlog/decisions.json`: the six
  rulings by pointer to `docs/design.md` §5 and the bootstrap exception of
  §6 below.
- `tools/kb.sh render`; commit the generated `.claude/rules/*.md` and
  `.claude/skills/project-knowledge/SKILL.md`.
- Gates: `tools/kb.sh check && tools/backlog.sh check`, render drift zero,
  `npm test` green, `node bin/houserules.mjs update --dir .` is a no-op.
- The root `.github/workflows/knowledge.yml` is seeded and kept; it runs
  once a remote exists.

## 4. After batch 0

The owner restarts Claude Code once: the root `.claude/agents/`,
`.claude/skills/`, and the SessionStart hook load only in a fresh session.

## 5. Code-health scan (files this batch touches)

`bin/lorekit.mjs`: the entry guard compares `process.argv[1]` without
`realpathSync` (defect, HR-001, batch 1); `install()` mixes file writes,
settings merge, stamping, and rendering in one function (long function;
acceptable at 60 lines, no split now). `template/tools/kb.mjs`: `loadBase`
lets `JSON.parse` errors escape as stack traces (HR-002). Tests: the init
tests build targets by hand in several places (`makeTarget` covers it; no
change). No dependency is added; `dependency_vetting` is empty.

## 6. Bootstrap exception (needs the owner's ruling)

`process.model-policy` requires a mightier reviewer than the implementer,
through the `implementer`, `task-reviewer`, and `branch-reviewer`
templates. Those templates are not installed in this repository until T4
and a restart. Proposal: the orchestrator (Fable) implements batch 0
directly, with TDD evidence and live runs in each commit body, and records
this exception in `backlog/decisions.json`. From batch 1 on, every change
goes through the agent layer: implementer on the cheapest fitting model,
task review one tier up, branch review on Fable.
