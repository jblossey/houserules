# Batch 2: pnpm workspace like tag-pilot, symlink-safe CLI entry, clean JSON errors

Date: 2026-09-01. Status: approved by the owner on 2026-09-01, as written (commitlint included).
Driver: backlog batch 2 — HR-004 (owner ruling of 2026-09-01, `docs/design.md`
§5.7), HR-001, HR-002. Branch `batch-2` from `9a15f95`; the ruling commit
`3eef303` is its first commit.
Process: the first batch through the agent layer — implementer on sonnet, task
review on opus, branch review on fable (`process.model-policy`), strictly
sequential. Ruling of 2026-09-01: task 3 fix round 1 ran on an opus implementer
after four consecutive sonnet endpoint drops; its re-review ran on fable. Workspace: `.superpowers/sdd/2026-09-01-batch-2/` (git-ignored)
with the ledger `progress.md`.

## 1. Goal

Run this repository on pnpm the way tag-pilot does, then close the two CLI
defects found in batch 0. After the batch: `mise run setup`, `test`, `lint`,
and `audit` work; no npm artefact remains; the CLI works through any symlinked
path; a malformed knowledge or backlog file yields one usage-error line.

## 2. Out of scope

- HR-003 (evals-rerun port): blocked on tag-pilot PR #42.
- Publishing (§5.3) and tag-pilot adoption (§5.4): deferred rulings.
- `template/` gains no package manager, dependency, or install step
  (`houserules.payload-runs-on-builtins`); consumers keep their own tooling.
  Kit-owned files change only in T2 and T3.
- Historical records keep their npm wording: `docs/design.md` §2–§5.3 and the
  batch 0 spec.

## 3. Facts verified on 2026-09-01

- tag-pilot: `mise.toml` pins `node = "24.18.1"`, `pnpm = "11.18.0"`,
  `shellcheck = "0.11.0"` (and more) and defines the tasks `setup`, `test`,
  `lint` (linters, `shellcheck tools/*.sh`, `tools/kb.sh check`,
  `tools/backlog.sh check`), and `audit`; `pnpm-workspace.yaml` holds
  `packages`, `allowBuilds`, `overrides`; `.githooks/commit-msg` is
  `mise exec -- pnpm exec commitlint --edit "$1"`, activated by
  `git config core.hooksPath .githooks` in `setup`; `commitlint.config.mjs`
  extends `@commitlint/config-conventional`; CI runs `jdx/mise-action@v2`,
  `pnpm install --frozen-lockfile`, `mise run lint`, `mise run test`, and a
  `commitlint` job on pull requests; `.gitignore` lists `.superpowers/`.
- pnpm 11 (current docs): settings live in `pnpm-workspace.yaml` in camelCase
  (`.npmrc` holds auth and registry only); `minimumReleaseAge` defaults to
  1440 minutes; `strictDepBuilds` defaults to true (install fails on an
  unreviewed build script; `allowBuilds` is the review); `pnpm import` writes
  `pnpm-lock.yaml` from `package-lock.json` and keeps the locked versions;
  `--frozen-lockfile` is the CI default; `pnpm add --save-exact` (`-E`) pins
  exactly; `pnpm init` writes a `devEngines.packageManager` block with
  `onFail: download` — not used here, mise pins pnpm.
- Our tree: vitest 4.1.11 and @vitest/coverage-v8 4.1.11 were published on
  2026-08-18 (older than the cooldown). The only install script in
  `package-lock.json` is `fsevents` 2.3.3 (darwin-only, absent on Linux).
  shellcheck 0.11.0 passes on `template/tools/*.sh`.
- pnpm probe (scratch project, `pnpm add --save-exact git+file://<this
  clone>`): `pnpm exec houserules files` and `pnpm dlx git+file://<clone>
  files` print the manifest (the shim execs the real path under
  `node_modules/.pnpm/`); `node node_modules/houserules/bin/houserules.mjs
  files` through the package symlink prints nothing and exits 0 (HR-001).
  mise strips inactive tool paths: in a directory without `mise.toml`, run
  tools as `mise x node@24.18.1 pnpm@11.18.0 -- <command>`.

## 4. Tasks, in order, one Conventional Commit each

### T1 `chore: run the repository on pnpm, set the workspace up like tag-pilot` (HR-004)

- Pins: `mise use --pin node@24.18.1 pnpm@11.18.0 shellcheck@0.11.0` — the
  CLI writes `mise.toml`, never a hand-written version. Tasks in `mise.toml`:
  `setup` = `pnpm install`, `git config core.hooksPath .githooks`; `test` =
  `pnpm test`; `lint` = `shellcheck tools/*.sh template/tools/*.sh
  .githooks/commit-msg`, `tools/kb.sh check`, `tools/backlog.sh check`;
  `audit` = `tools/kb.sh audit`.
- Lockfile: `pnpm import`, `git rm package-lock.json`, `pnpm install`; then
  `pnpm install --frozen-lockfile` passes. `pnpm-workspace.yaml`: `saveExact:
  true` (the one deviation from tag-pilot: it makes
  `security-hygiene.exact-pins` the default); `allowBuilds` only for build
  scripts pnpm reports, each with a one-line reason in the task report (on
  Linux: none expected). No `packageManager` or `devEngines` field.
- commitlint: `pnpm add -D --save-exact @commitlint/cli
  @commitlint/config-conventional` (dependency vetting in the report);
  `commitlint.config.mjs` and `.githooks/commit-msg` as in tag-pilot, the hook
  executable. config-conventional's defaults (header and body lines at most
  100 characters; subject not in sentence, start, pascal, or upper case) match
  `process.conventional-commits`: verify against the current README, override
  nothing. The implementer runs `mise run setup` before its first commit, so
  the hook checks every commit of this batch.
- `.github/workflows/ci.yml` (dormant until a remote exists): job `checks` =
  checkout with `fetch-depth: 0`, `jdx/mise-action` at its current major,
  `pnpm install --frozen-lockfile`, `mise run lint`, `mise run test`, and on
  pull requests `mise run audit -- --base <base sha> --head <head sha>`; job
  `commitlint` on pull requests as in tag-pilot. Verify the action versions
  against current docs. The root `.github/workflows/knowledge.yml` (a
  seed-once copy, not kit-owned) is deleted: one toolchain and one workflow
  for this repository; the template copy still ships to consumers. Ruling of
  2026-09-01 on the task 1 review; the spec first kept the seeded gate.
- `.gitignore`: add `.superpowers/`.
- Docs and knowledge: README (`pnpm dlx houserules init` for the published
  path; the development section names `mise run setup`, `mise run test`,
  `mise run lint`, and the hook); `houserules.node-under-mise` (summary names
  pnpm; body gives the `mise x node@… pnpm@… --` form for scratch
  directories); `houserules.live-run-recipe` (the `npm exec` line becomes the
  pnpm probe of §3). Ids stay. `tools/kb.sh render`.
- Evidence: configuration and docs, no unit under test; the task report
  carries the live run — `mise run setup`, `mise run lint`, `mise run test`
  (110 tests, coverage gate), `pnpm install --frozen-lockfile`, one rejected
  commit (`Bad Header` → commitlint error) and one accepted, and `git grep -n
  "npm\|npx"` that lists only the historical records and evidence lines.

### T2 `fix(cli): resolve the entry path through symlinks before the main-module check` (HR-001)

- `template/tools/lib/cli.mjs`: export `isMainModule(importMetaUrl)` — true
  when `process.argv[1]` exists and its real path equals the real path of
  `importMetaUrl`; false when `argv[1]` is absent or not a file; JSDoc. Use
  it in `template/tools/kb.mjs`, `template/tools/backlog.mjs`, and
  `bin/houserules.mjs` (import `../template/tools/lib/cli.mjs`; the package
  ships both directories). `node bin/houserules.mjs update --dir .` syncs the
  root copies (the dogfood test pins them).
- Tests, RED first: unit tests for `isMainModule` (symlinked `argv[1]`,
  direct, foreign, absent); one end-to-end test per CLI (`bin/houserules.mjs
  files`, `kb.mjs topics`, `backlog.mjs list`) that runs the file through a
  symlink in a temp dir with `execFileSync(process.execPath, …)` and expects
  the JSON output.
- Live run: the §3 probe repeated in a scratch project at the fix commit: the
  symlinked path prints the manifest; `pnpm dlx` and `pnpm exec` still do;
  `init --dir <scratch>` plus both checks.
- The coverage gate stays green (lines 80, branches 99); the report states
  the measured numbers.

### T3 `fix(tools): report invalid json in knowledge and backlog files as a usage error` (HR-002)

- `template/tools/lib/json-store.mjs` `readJson`: on a parse error throw
  `UsageError` with `<path>: invalid JSON (<parse message>)`, using the
  existing class (if `cli.mjs` imports `json-store.mjs`, move `UsageError`
  to the module that avoids the cycle; ids and exports stay documented).
  `kb.mjs` and `backlog.mjs` already turn a `UsageError` into one `error:`
  line and a non-zero exit. `update --dir .`.
- Tests, RED first: a broken topic file → `main(['check'])` and `render`
  print the file and the parse message, no stack trace, the usage-error exit
  code; the same for a broken backlog file. `tests/init.test.mjs` "lets a
  render failure on broken project data propagate as a real error" stays
  valid unchanged.
- Live run: scratch `init`; break `knowledge/process.json`; `tools/kb.sh
  check` prints one error line and exits 2; repair; ok.

## 5. Reviews and finish

After each task: task review (opus) with `AUDIT_JSON`; every finding fixed,
Minor included, or deferred as a backlog item with a reason. After T3: branch
review (fable) over `9a15f95..HEAD` with the plan, this spec, and the
workspace; retrospective proposals applied in one `docs(knowledge)` commit,
standing-rule proposals listed for the owner. Then `finishing-a-feature`
without its push and PR steps (no remote; decision 3): backlog ticked, one to
five clean commits, ff-only merge into `main` from the CLI.

## 6. Code-health scan (files this batch touches)

- `bin/houserules.mjs`, `template/tools/kb.mjs`, `template/tools/backlog.mjs`:
  the same three-line entry guard in three files (duplication; T2 replaces it
  with one helper). `bin/houserules.mjs` surfaces the raw stderr of the
  `tools/kb.sh render` it spawns (T3 makes that one line).
- `template/tools/lib/json-store.mjs` `readJson`: `JSON.parse` errors escape
  unwrapped (T3).
- `mise.toml`: `node = "24"` is a floating pin (T1 pins it exactly).
- README and `knowledge/houserules.json` name npm in four places (T1).
- Tests: every CLI test calls `main` in-process; no test runs a file as an
  entry point — the gap that hid HR-001 (T2 adds child-process tests).
- No smell found in `tests/dogfood.test.mjs`, `vitest.config.mts`,
  `.gitignore`, `package.json` (one script, two devDependencies).

## 7. Open point for the gate

Include commitlint with the `.githooks/commit-msg` hook and the `commitlint`
CI job (recommended: it enforces `process.conventional-commits` mechanically,
as tag-pilot does; cost: two vetted devDependencies). The alternative is T1
without the hook and the job.
