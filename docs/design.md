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

## 5. Open decisions for the owner

1. **Name.** `lorekit` — checked unclaimed on the npm registry on
   2026-08-30; rename before publishing if desired.
2. **License.** Currently `UNLICENSED`/private. Decide (MIT/Apache-2.0
   would suit a kit meant for reuse) and add a LICENSE file.
3. **Publishing.** No remote, no npm publish, no marketplace registration
   was performed. Options, in effort order: keep using the local clone; push
   to GitHub and run via `npx github:<user>/lorekit init`; publish to npm.
4. **Adoption path for tag-pilot itself.** TagPilot already runs the source
   of this setup and is fine as-is. If it should become a lorekit consumer:
   run `lorekit update` semantics manually (adopt the kit-owned files),
   keep its own knowledge/backlog data, and reconcile renames — agent and
   skill names (`tagpilot-*` → unprefixed, `tagpilot-knowledge` →
   `project-knowledge`), the `a19` → `dependency_vetting` report field, and
   its `TP-`/`E01` id patterns (lorekit's loosened patterns accept both).
   Ids are permanent there; keep TagPilot's entry ids as they are.
5. **SessionStart `clear` matcher.** The seeded hook fires on
   `startup|resume|clear` and `compact`, matching tag-pilot's proven config.
