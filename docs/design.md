# lorekit design: packaging the knowledge-management setup

Date: 2026-08-30. Status: implemented. Source: the TagPilot knowledge-management
setup (tag-pilot branch `knowledge-management`, PR #42), extracted into a
standalone, project-agnostic kit.

## 1. Goal

Make TagPilot's knowledge-management setup — knowledge base, backlog, two CLIs,
deliverables contract, agent layer, render pipeline, CI gates — applicable to
any repository, with a clean way to apply it to new projects and to update
existing ones, while each project keeps its own data.

## 2. Research record (2026-08-30, against current docs)

### Option A: Claude Code plugin

Verified against https://code.claude.com/docs/en/plugins.md,
plugins-reference.md, hooks.md, sub-agents.md, discover-plugins.md:

- A plugin carries skills, agents, hooks, slash commands, MCP/LSP servers;
  manifest `.claude-plugin/plugin.json`; skills namespace as
  `plugin-name:skill-name`. Plugin agents do support `skills:`, `model:`,
  `disallowedTools:` frontmatter. Hooks can run bundled scripts via
  `${CLAUDE_PLUGIN_ROOT}` and require workspace trust.
- Install per project via `extraKnownMarketplaces` + `enabledPlugins` in
  `.claude/settings.json`; git-based marketplaces work; updates propagate
  through the marketplace auto-updater.
- **A plugin cannot contribute `.claude/rules/*.md`.** Path-scoped rules
  (`paths:` frontmatter) load only from the project repo (and only on Read,
  not on Write — tracked upstream as anthropics/claude-code#23478, which is
  why standing rules also live in `CLAUDE.md`-adjacent generated files and
  in the preloaded skill).
- The superpowers plugin (github.com/obra/superpowers-marketplace) stays
  usable beyond one harness because its payload is plain markdown skills; the
  lesson is "plain files portable everywhere", not "plugin required".

Why rejected as the primary mechanism: everything load-bearing in this setup
must live in the project repo anyway. The generated `.claude/rules/*.md` and
the generated, per-project `project-knowledge` skill cannot come from a
plugin; CI runs `kb check`/`audit` on a runner that has no Claude Code, so
the CLIs must be in-repo; the SessionStart hook runs a project script. A
plugin could ship only the static agent templates and the orchestrating
skill — the least project-bound tenth of the setup — while splitting the
machinery across two update channels.

### Option B: installable npm package (`npx lorekit init`)

Standard scaffolder pattern; the tooling is already Node. Two variants:
runtime dependency (project imports the CLIs from `node_modules`) or
scaffold-and-vendor (files are copied in). A runtime dependency fights two
properties inherited from the source design: the tools must run with Node
built-ins only, before any install step (the SessionStart hook fires in
fresh clones), and the project must stay self-contained (a clone with no
`npm install` still has working `tools/kb.sh`).

### Option C: template applied by copying / install script

Maximally simple and harness-portable, but with no update story: once copied,
machinery fixes never reach adopters.

## 3. Decision

**B+C hybrid: a scaffold-and-vendor CLI with an ownership manifest.**
`lorekit init` copies the payload into the project; `lorekit update`
overwrites only kit-owned machinery; per-project data is seeded once and
never touched again. Runnable today from a local clone
(`node ~/projects/lorekit/bin/lorekit.mjs init`), publishable later as
`npx lorekit init` without structural change (the `bin` field is wired).

The two strongest reasons:

1. **The project repo is the only place everything works.** Generated rules
   and the preloaded knowledge skill must be project files (plugin cannot
   contribute them); CI gates run without Claude Code; the hook runs before
   any install. Vendoring is not a compromise — it is the only layout in
   which every consumer (harness, CI, hook, subagent) finds what it needs.
2. **The manifest split solves update-vs-ownership.** Kit-owned files
   (`tools/`, agent templates, orchestrating/finishing skills, hook) stay
   upgradable byte-for-byte; seed-once files (schemas, topics, backlog,
   evals, CI workflow, CLAUDE.md, settings) belong to the project from the
   first write. `lorekit files` prints the split; `.lorekit.json` records
   the installed version.

Harness portability comes free: the payload is plain JSON, markdown, and
POSIX shell + Node scripts. Another harness can read `knowledge/*.json`
directly or shell out to `tools/kb.sh`; nothing depends on Claude Code
except the `.claude/` conventions, which other tools ignore harmlessly.

A plugin remains a possible **later complement** (publishing the orchestrating
skill and agent templates for teams that want central updates), recorded here
as considered and deliberately not built now (YAGNI; two update channels).

## 4. What ships, what was left out

Shipped as seed data (generic): the entry/area/check data model and schemas;
kb + backlog CLIs with their test suites; the JSON deliverables contract and
validate/audit machinery (field `a19` renamed `dependency_vetting`); the
render pipeline (standing rules, area rules, `project-knowledge` skill); the
implementer/task-reviewer/branch-reviewer contracts with the
mightier-reviewer model policy and fix-round contract; the orchestrating and
finishing-a-feature skills; the SessionStart hook; three eval scenarios; a
plain-Node CI workflow; and these standing rules: ask-when-missing,
backlog-drives-work, brainstorm-first, conventional-commits, ff-only-merges,
knowledge-first, rulings-to-file, sequential-agents, tdd (scoped to
executable code, gates for data/docs), no-tech-debt (backlog-only deferrals,
TODO check), code-health-scan, deliverables-json, model-policy,
live-run-before-ci, the security-hygiene family (dependency vetting, exact
pins, no co-author, no focused tests, verify current docs),
quality.principles, and the writing-style family (ASD-STE100 principles,
code comments, doc comments). Plus the non-standing knowledge-base authoring
rules (summary-is-the-rule, state-only-the-source, ids-are-permanent).

Left out (TagPilot-specific, by design): the `architecture.*` invariants;
domain topics (catalog-db, metadata-writes, sidecar, watcher, webview, api,
deploy, infra, live-run recipes, ...); coverage floor values and the coverage
topic; deploy/infra rules; opinionated toolchain picks (oxc-only,
no-ci-only-logic/mise wiring, nx, commitlint hook, gitleaks config); the
`no-direct-push-main` and `security-prompts-and-credentials` rules (they
encode TagPilot's cost and desktop context); TagPilot's Rust eval scenario.
A "preset" mechanism for such opinions was considered and skipped (YAGNI —
a project records its picks as ordinary entries; a second adopter with the
same picks would justify a preset).

Adaptations made during extraction: the knowledge schema's `area` enum and
`areas.json` are seed data the project extends (the tests exercise an
extended enum); backlog id prefix is parameterized (`--id-prefix`, default
`WI-`, rewritten in the seeded schemas at init); the shell wrappers prefer
`node` on PATH and fall back to `mise exec`; the CI workflow uses
actions/checkout@v7 and actions/setup-node@v7 (verified current 2026-08-30);
`milestone` and backlog section-name patterns were loosened to generic ones.

## 5. Decisions for the owner

Each item records the ruling with its date, or stays marked open.

1. **Name — ruled 2026-08-31: rename to `houserules`.** `lorekit` was
   unclaimed on the npm registry (checked 2026-08-30 and 2026-08-31) but is
   taken in the wild: lorekit.io ("Persistent memory for your AI agents",
   MIT, aimed at the same Claude Code, Cursor, and Codex users), the GitHub
   organization `lorekit`, matluz1/lorekit (an MCP tabletop-RPG engine),
   lorekit.app, and lorekit.ai. A search for the name would never surface
   this kit, and adopters would confuse it with lorekit.io. `houserules`
   was free on npm on 2026-08-31; its nearest neighbours are the npm package
   `house-rules` (an input-validation library, last modified 2022) and
   board-game house-rule trackers — no AI-agent product. Rejected
   candidates: `praxiskit` (free, less self-explaining), `agentlore`
   (collides with a Claude Code session-log product), and `canonkit`,
   `groundrules`, `codelore`, `kbkit`, `lorebook`, `repolore`, `codecanon`
   (taken on npm). The rename covers the package and bin name, the stamp
   file (`.lorekit.json` → `.houserules.json`), the README, this record,
   and the tests; it is the first work item after the decisions round.
2. **License — ruled 2026-08-31: MIT.** The source project tag-pilot is
   `UNLICENSED`/private with the same owner, and the payload contains no
   third-party material (the superpowers plugin is only named, in
   `knowledge/process.json`), so the choice was free. MIT is on every
   allowlist. Because `init`/`update` vendor the payload into adopters'
   repositories, the kit-owned `.mjs` and `.sh` files carry a two-line
   SPDX/copyright header so that copies carry the notice by construction;
   the markdown agents and skills get no header (prompt tokens), and the
   README states that files produced by `init` are the adopter's under the
   same terms. Rejected: MIT-0 (cleaner for vendoring, but absent from some
   corporate SPDX allowlists), Apache-2.0 (NOTICE handling is heavy for
   vendored files), staying UNLICENSED (only coherent for a local-only
   clone). To do: LICENSE file ("2026 Jannis Blossey"), `"license": "MIT"`
   in `package.json`, the headers, the README sentence.
3. **Publishing — open, deferred by the owner on 2026-08-31.** No remote,
   no npm publish, no marketplace registration was performed. Options, in
   effort order: keep using the local clone; push to GitHub as
   `jblossey/houserules` (the name was free on 2026-08-31) and run via
   `npx github:jblossey/houserules#<tag> init`; publish to npm as
   `houserules`. Findings from the 2026-08-31 check that apply to every
   published path: (a) npm installs the `bin` as a symlink in
   `node_modules/.bin`, Node resolves `import.meta.url` through the symlink
   but keeps `process.argv[1]` as the `.bin` path, so the entry guard in
   `bin/lorekit.mjs` never matches and the CLI exits 0 without output —
   verified live with `npm exec --package=git+file://<this repo>`; the fix
   is `realpathSync(process.argv[1])` in the guard (same idiom in
   `tools/kb.mjs` and `tools/backlog.mjs`) with a regression test that runs
   the bin through a symlink; (b) `package.json` has no `files` field, so a
   git or npm install also carries `tests/`, `docs/`, `mise.toml`, and
   `vitest.config.mts`. Both are scheduled with the rename, independent of
   this ruling.
4. **Adoption path for tag-pilot itself — open, deferred by the owner on
   2026-08-31 until PR #42 is merged.** TagPilot already runs the source of
   this setup and is fine as-is. Until the ruling, tag-pilot stays the
   upstream of every kit-owned file: diff its `tools/`, `.claude/agents/`,
   and `.claude/skills/` against `template/` before every release and port
   the drift (first item: the `process.evals-rerun` rule with its
   `co-change` check and `.claude/evals/record.json`, tag-pilot `fc3241b`).
   Measured drift on 2026-08-31: `kb.mjs` 10 lines, `backlog.mjs` 2, the
   wrappers 2–3 each, the agents 10–16 each, `finishing-a-feature` 28,
   `cli.mjs` and `json-store.mjs` 0. If tag-pilot becomes a consumer: run
   `update` semantics manually (adopt the kit-owned files), keep its own
   knowledge/backlog data, and reconcile renames — agent and skill names
   (`tagpilot-*` → unprefixed, `tagpilot-orchestrating` → `orchestrating`,
   `tagpilot-knowledge` → `project-knowledge`), the `a19` →
   `dependency_vetting` report field, and its `TP-`/`E01` id patterns (the
   loosened patterns accept both). Ids are permanent there; keep TagPilot's
   entry ids as they are. That migration is tag-pilot work, done there.
5. **Dogfooding — ruled 2026-08-31: yes, full.** After the rename, this
   repository installs its own kit: `init --dir . --id-prefix HR`, then a
   repository-specific `CLAUDE.md`, areas (`template`, `cli`, `tests`,
   `docs`), topics, and a backlog that holds the open work items. The CLIs
   resolve their data from the git root of the cwd, so the root
   `knowledge/` and `backlog/` do not collide with `template/` (the seed
   payload, read only by `init` and the tests). The 12 kit-owned files
   exist twice — in `template/` (the source) and at the root (the
   installed copy); `update --dir .` syncs them and a parity test pins the
   root copies to `template/` byte for byte. A standing rule here says:
   edit `template/`, then run `update --dir .`; never hand-edit the root
   copies or the generated files. Coverage globs list only
   `template/tools/*` and `bin/`, so the copies do not distort coverage.
   Rejected: no dogfooding (rules by instruction only, nothing exercises
   `init`/`update` between releases); deferring.
6. **SessionStart matcher — ruled 2026-08-31: `startup|resume|clear|fork`
   for the `start` ritual; the `compact` entry stays.** Verified against the
   current hooks reference on 2026-08-31: `source` has five values —
   `startup`, `resume`, `clear` ("context was reset while maintaining the
   same session"), `compact`, `fork` ("a session was forked from another
   session"). Forked sessions report `fork` since Claude Code v2.1.214
   (before: `resume`), so tag-pilot's four-value matcher misses them on the
   installed 2.1.251. `clear` stays because the ritual is most needed right
   after a context reset, and the `start` mode prints one line. Rejected:
   the unchanged four-value matcher; dropping `clear`. Note: settings are
   seed-once and `init` merges by exact matcher string, so the new value
   reaches new installs only; existing adopters edit one line. Known
   upstream quirk, irrelevant here: in VS Code `/clear` reports `startup`
   (anthropics/claude-code#26794).
