# houserules design: packaging the knowledge-management setup

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

### Option B: installable npm package (`npx houserules init`)

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
`houserules init` copies the payload into the project; `houserules update`
overwrites only kit-owned machinery; per-project data is seeded once and
never touched again. Runnable today from a local clone
(`node ~/projects/houserules/bin/houserules.mjs init`), publishable later as
`npx houserules init` without structural change (the `bin` field is wired).

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
   first write. `houserules files` prints the split; `.houserules.json` records
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
   and the tests. Done in 7db7db9 on 2026-08-31.
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
   in `package.json`, the headers, the README sentence. Done in a1d31ac on
   2026-08-31.
3. **Publishing — open, deferred by the owner on 2026-08-31.** No remote,
   no npm publish, no marketplace registration was performed. Options, in
   effort order: keep using the local clone; push to GitHub as
   `jblossey/houserules` (the name was free on 2026-08-31) and run via
   `npx github:jblossey/houserules#<tag> init`; publish to npm as
   `houserules`. Findings from the 2026-08-31 check that apply to every
   published path: (a) npm installs the `bin` as a symlink in
   `node_modules/.bin`, Node resolves `import.meta.url` through the symlink
   but keeps `process.argv[1]` as the `.bin` path, so the entry guard in
   `bin/houserules.mjs` never matches and the CLI exits 0 without output —
   verified live with `npm exec --package=git+file://<this repo>`; the fix
   is `realpathSync(process.argv[1])` in the guard (same idiom in
   `tools/kb.mjs` and `tools/backlog.mjs`) with a regression test that runs
   the bin through a symlink; (b) `package.json` has no `files` field, so a
   git or npm install also carries `tests/`, `docs/`, `mise.toml`, and
   `vitest.config.mts`. (a) is backlog item HR-001; (b) was done in a1d31ac
   on 2026-08-31 (`files`: bin, template, README.md, LICENSE).
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
   Update 2026-09-01: PR #42 is merged and the trigger fired; the owner
   deferred again. New trigger: the first houserules release or publish
   (raised together with decision 3). The drift check of 2026-09-01 found
   one tag-pilot novelty since the port base — fc3241b, the evals-rerun
   rule — which batch 4 ports as HR-003; every other difference is a
   houserules improvement tag-pilot lacks. The tag-pilot-only scenario
   rust-test-near-coverage stays unported by design.
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
   `init`/`update` between releases); deferring. Done in 3bc142e on
   2026-08-31; the backlog lives in `backlog/` from then on.
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
   (anthropics/claude-code#26794). Done in d356fba on 2026-08-31.
7. **Package manager — ruled 2026-09-01: pnpm only; the workspace is set
   up like tag-pilot's.** The owner's instruction of 2026-09-01: use pnpm,
   never npm or npx, and set the workspace up as tag-pilot does. tag-pilot
   on 2026-09-01: `mise.toml` pins `node = "24.18.1"` and
   `pnpm = "11.18.0"` and defines the tasks `setup`, `test`, `lint`, and
   `audit`; `pnpm-lock.yaml` is the only lockfile; `pnpm-workspace.yaml`
   holds the pnpm settings; `.githooks/commit-msg` runs commitlint through
   `mise exec -- pnpm exec commitlint --edit`, activated by
   `git config core.hooksPath .githooks` in the `setup` task; CI installs
   with `pnpm install --frozen-lockfile` after `jdx/mise-action`. Applied
   here as HR-004 in batch 2; the batch spec fixes the file list; done on
   2026-09-01. The
   payload in `template/` has no package manager (§2, option B), so the
   ruling changes nothing there. Probe of 2026-09-01 in a scratch project
   (`pnpm add --save-exact git+file://<this clone>`): pnpm writes
   `node_modules/.bin/houserules` as a shell shim that execs the real path
   under `node_modules/.pnpm/`, and links `node_modules/houserules` as a
   symlink; `node node_modules/houserules/bin/houserules.mjs files` through
   that symlink prints nothing and exits 0 — the HR-001 defect, independent
   of the package manager.
8. **Commit body line limit in the audit — ruled 2026-09-01: yes.** The
   batch 2 branch review found two controller commits, made before the
   commit-msg hook existed, with body lines of 190 and 227 characters that
   the audit's `commits` check passed, because the check tests the subject
   only. HR-006 adds the optional `body_line_max` key to the check (schema,
   `kb.mjs`, tests); the same task sets `body_line_max: 100` on the standing
   entry `process.conventional-commits` in `knowledge/process.json` and
   `template/knowledge/process.json`. Rejected: the commit-msg hook as the
   only enforcement (it covers only commits made after it is installed, and
   only in this repository).
9. **Rulings-to-file body — ruled 2026-09-01: the dispatch-deviation bullet
   is added, as the batch 2 branch review proposed.** The ruling that moved a
   fix round to an opus implementer sat only in the git-ignored ledger until
   the branch review found it. The body of the standing entry
   `process.rulings-to-file` already named the home files (backlog entry,
   knowledge entry, batch spec) and said that a ledger note never substitutes
   for the tracked write; it now also makes a dispatch deviation from the
   spec's process line (model, order, gate) a ruling to amend in that spec
   line in the same turn. The summary is unchanged. Applied in
   `knowledge/process.json` and `template/knowledge/process.json` so every
   new installation carries it.
10. **TDD summary names the disclosed-mutation proof — ruled 2026-09-01:
    yes, in shortened form.** The batch 3 branch review proposed promoting
    the body's already-correct-behavior clause into the `process.tdd`
    summary after task 1 added a coverage-keeping test with no RED. The
    proposed sentence exceeds the schema's 160-character summary cap, so the
    summary now reads "Test-driven development for every executable change:
    the failing test first, or a disclosed-mutation proof for
    already-correct behavior; verbatim RED and GREEN." and the body keeps
    the detail. Applied in `knowledge/process.json` and
    `template/knowledge/process.json`.
11. **Test-state hygiene in quality.principles — ruled 2026-09-01: yes.**
    The body of `quality.principles` gains the batch 3 retrospective's
    bullet: tests assert behavior and can fail; global and module state
    (mocks, prototypes) is cleaned up after each test; an auto-restore
    setting covers only spies, so a module-mock factory's mock gets its own
    explicit afterEach reset. The summary is unchanged. Applied in
    `knowledge/quality.json` and `template/knowledge/quality.json`; the
    gotcha `houserules.vitest-restore-mocks-scope` records the Vitest
    specifics.
12. **Rulings-to-file summary names deferrals — ruled 2026-09-01: yes, in
    shortened form.** The HR-010 deferral ruling sat only in the git-ignored
    ledger until the batch 3 branch review caught it. The summary of
    `process.rulings-to-file` now reads "Every ruling goes to its home file
    in the same turn — a deferral's backlog item included. A ledger and the
    chat are not home files; neither survives compaction." (the wording
    quoted at the gate measured 161 characters against the 160 cap; one
    word shorter ships). Applied in `knowledge/process.json` and
    `template/knowledge/process.json`.
13. **Fix-round and reconstruction clauses in process.tdd — ruled
    2026-09-02: yes.** The batch 4 reviews found an untested behavior
    narrowing shipped behind a wrong claim (task 2) and a post-commit
    reconstruction presented as a natural cycle (task 4). The body of
    `process.tdd` now says a fix round that narrows or widens matching
    behavior carries its own RED or counter-example test, and a cycle
    captured after the commit says so and presents the runs in their real
    order. Applied in `knowledge/process.json` and
    `template/knowledge/process.json`; the summary is unchanged.
14. **Accepted deviations are rulings — ruled 2026-09-02: yes.** The
    batch 4 task 4 verify-path deferral lived only in the git-ignored ledger
    and report until the review forced the spec amendment. The body of
    `process.rulings-to-file` now says an accepted implementer deviation is
    a ruling, homed (a spec amendment or a backlog item) in the turn it is
    accepted, before the next dispatch. Applied in `knowledge/process.json`
    and `template/knowledge/process.json`; the summary is unchanged.
15. **KIT_OWNED-anchored body for houserules.template-is-the-source — ruled
    2026-09-02: yes.** The entry's body enumerated the installed skills and
    went stale the moment batch 5 added `migrating-knowledge` to
    `KIT_OWNED`. The body now names the authority instead of the list: the
    root copies of every `KIT_OWNED` path — `tools/`, `.claude/agents/`,
    and the kit skills — are the installed copy. Applied in
    `knowledge/houserules.json` (root-only topic); the summary is
    unchanged. Source: batch 5 branch review, stale_entries.
16. **Anchored, widened trailer check — ruled 2026-09-02: yes.** The
    `security-hygiene.no-coauthor` check grepped commit bodies for an
    unanchored substring, so honest prose naming the trailer turned the
    batch 6 branch audit red and forced a reword in task 4's self-review.
    The check now anchors to real trailer lines and covers the session
    trailer the harness also injects; the summary says both. Applied in
    `knowledge/security-hygiene.json` and
    `template/knowledge/security-hygiene.json`. Source: batch 6 branch
    review, violated_rules and template_defects 3.
17. **Before/after captures in process.tdd — ruled 2026-09-02: yes.**
    Batch 6 twice fixed a change where no assertion can change (an
    output-noise fix, a report-only round) and both re-reviews accepted
    disclosed before/after captures as the honest evidence. The body of
    `process.tdd` now names that form: captures of the observable
    difference, presented in the order they ran. Applied in
    `knowledge/process.json` and `template/knowledge/process.json`; the
    summary is unchanged. Source: batch 6 branch review, violated_rules.
18. **The timing key in process.tdd — ruled 2026-09-02: yes.** Batch 7
    shipped the required `mode` enum, and three reviewers spent rounds
    converging on what `natural` means before the rule said it. The body
    of `process.tdd` now states the timing key: the label follows when
    the shown run happened, not the failure's flavor — `natural` only
    when the shown RED ran pre-commit; a post-hoc recapture is
    `reconstructed` and says so; a disclosed-mutation proof is
    `mutation`. Applied in `knowledge/process.json` and
    `template/knowledge/process.json`; the summary is unchanged. Source:
    batch 7 branch review, violated_rules.
19. **Releases — ruled 2026-09-03: semver via release-please; alpha
    pre-release first; upstream after alpha state.** Versioning follows
    semver and changelogs are generated from the conventional commits;
    the tool is release-please. The first release is a pre-release
    under the `alpha` dist-tag. The GitHub upstream is created right
    after all tasks needed to reach alpha state are done; until then
    decision 3's constraint stands (no remote, no `gh repo create`, no
    `npm publish`). Adopters get releases through the update path.
    Backlog: HR-039 (release machinery), HR-038 (adopter update path).
    Source: owner, 2026-09-03.
20. **Release tag format — ruled 2026-09-04: plain `v<version>`.**
    Tier-2 spec ruling 1. Every binary-delivery channel (mise ubi,
    asdf, taps) defaults to the `v*` grammar; ruled at the cheapest
    moment (one release, zero external consumers).
    `include-component-in-tag` goes false in the release-please config
    at the next config-touching task; the alias tag `v0.2.0-alpha`
    points at the existing release commit (bd2b754) so the grammar is
    uniform from the first release, and `houserules-v0.2.0-alpha`
    stays as history. Unblocks HR-048's channels and HR-049. Source:
    owner, 2026-09-04.
21. **Tier-2 implementation language — ruled 2026-09-04: Rust.**
    Tier-2 spec ruling 2, chosen over Go with the trade-offs on the
    table: type-level correctness (serde models the deliverables and
    knowledge schemas exactly), smaller binaries, no GC — accepting
    slower compiles and a cross-compilation story (cargo-dist or
    cross/zig) that the spec settles. The workload (JSON, globs, git
    subprocesses, regex, markdown render) is correctness-bound, not
    performance-bound; the parity gates hold either way. Source:
    owner, 2026-09-04.
22. **npm retires — ruled 2026-09-04.** Tier-2 spec ruling 3, closing
    decision 3's npm part: distribution is binary-only (mise ubi,
    asdf, taps, a curl installer); nothing was ever published to npm
    and nothing will be under this ruling. package.json remains for
    this repository's own dev tooling; the README's post-publish npm
    form goes away at the rewrite. An esbuild-style wrapper package
    (postinstall downloads the platform binary) stays purely additive
    later if demand appears. Source: owner, 2026-09-04.
    Superseded in part by §5.23: at full retirement package.json
    leaves the tree with the rest of the JS dev tooling.
23. **The Tier-2 surface and runtime — ruled 2026-09-04.** Four
    rulings from the batch 15 spec gate: (a) NO SHIMS — `tools/kb.sh`
    and `tools/backlog.sh` are deleted, every shipped reference
    invokes the binary directly, and `update` gains KIT_OWNED
    deletion; (b) FLAT commands, no kb/backlog namespaces, with
    per-module checks (`check-knowledge`, `check-backlog`) because
    (c) houserules becomes MODULAR — adopters will choose feature
    sets (backlog-only, rules without backlog); the crate boundaries
    prepare it, the feature itself is HR-053; (d) the DEV TOOLING
    migrates too — cargo test replaces vitest phase by phase, a
    built-in `check-commit` replaces the hook's commitlint probe,
    and pnpm, package.json, and node_modules leave the tree at
    retirement. Nothing JS survives phase 5. The full design lives in
    docs/specs/2026-09-04-batch-15-tier2-spec.md. Source: owner,
    2026-09-04.
24. **Post-port repository sweep — ruled 2026-09-04.** Tier-2 spec
    ruling 8: the migration closes with an obligation to scan the
    full repository and correct every rule, knowledge entry, backlog
    item, and doc (README included) to the new setup — flat
    `houserules` commands, Rust/cargo tooling, binary distribution.
    Standing entries that encode the old world amend or retire under
    owner rulings recorded here. Historical records (CHANGELOG,
    decision rows, past specs, eval records) keep their wording. The
    sweep is the closing task of phase 5 with a mechanical grep gate;
    the spec's §5 carries the detail. Source: owner, 2026-09-04.
25. **Glob vocabulary for the Rust port — ruled 2026-09-04.** Raised
    by the batch 16 T3 review: the Rust matcher had silently
    replaced the JS two-engine union (node matchesGlob OR the custom
    globToRegExp) and diverged on extglob, nested braces, one
    dot-segment case, and two panicking malformed-class globs. The
    owner rules: the globset crate becomes the single matching
    engine (well-maintained library over custom code). Every
    divergence from the frozen union is pinned by a counterexample
    cargo test asserting the chosen answer; malformed globs are
    named errors, never panics; extglob does not exist in the
    vocabulary. This is the one further sanctioned exception to the
    spec's §7 parity rule, beside the flat command surface. No glob
    in this repository or the corpus uses the affected vocabulary
    (all 59 are `**`, `*`, or literals). Source: owner, 2026-09-04,
    mid batch 16.
26. **The five batch-16 parity deviations — ruled 2026-09-04.**
    Confirmed as a set at the batch 16 report: (a) CLI failure
    paths print one named error line and exit 2 where the JS dumped
    a node stack trace with exit 1; (b) areas.json globs validate
    eagerly at load with a named error where the JS silently
    mismatched until match time; (c) regex validity verdicts match
    the JS exactly through a real ECMAScript engine (regress), with
    only the untestable V8 reason wording diverging; (d) coverage
    floors ratchet per ported file in a second vitest run while the
    global floors keep their pre-port values; (e) where the JS
    crashed with uncaught errors on malformed data (non-string
    verify, uncompilable schema pattern, unsupported $ref), the
    binary reports named findings with exit 1. Each was raised by a
    batch 16 review, homed as a spec §6 bullet when accepted, and
    is now owner-ruled; the spec's §6 carries the detail. Source:
    owner, 2026-09-04, batch 16 report.
27. **Rust toolchain pin form — ruled 2026-09-04: stable,
    freshest.** Raised at the batch 16 report from T2's disclosed
    choice (latest at 1.98.0, respecting mise's minimum_release_age
    quarantine, over the day-old stable 1.98.1). The owner rules
    the other way: bumps track stable's newest immediately; the
    quarantine bypass for the rust toolchain is deliberate. The
    exact-pins rule stands — the version is resolved by the CLI
    (`mise latest rust`) and pinned exactly; the pin moved to
    1.98.1 the same turn, components preserved, cargo gates green.
    Procedure: houserules.rust-toolchain-bumps-use-stable. Source:
    owner, 2026-09-04, batch 16 report.
28. **Batch 16 retrospective standing changes — ruled 2026-09-04.**
    All four approved at the batch report: process.rulings-to-file
    gains the code-comment-is-not-a-home-file bullet;
    quality.principles gains the language-engine bullet (validity
    verdicts come from an engine of that language, never a
    hand-rolled scan); two new standing rules land —
    process.evidence-outlives-the-session (cite evidence only at
    paths that outlive the session) and
    process.claims-match-artifacts (re-open every cited artifact
    before submitting a report). Applied in knowledge/ and
    template/knowledge/ (process.json, quality.json). Source:
    owner, 2026-09-04, batch 16 report.
29. **The three batch-17 rulings — confirmed 2026-09-05.** Ruled as
    a set at the batch 17 report: (a) the data-layer rule — typed
    serde models serve only paths where data is never re-serialized
    to its source file and a parse failure is acceptable; every
    path preserving an adopter's on-disk key order or diagnosing
    malformed input reads raw Value through tolerant loaders, and
    consumerless models are deleted, not kept dormant; (b) the
    unified get's arity-first ordering (its domain depends on the
    ids; fixed-domain commands stay load-first per JS parity);
    (c) the clap argv deviation — flags the JS silently swallowed
    are named usage errors at exit 2, every observable instance
    pinned by scripted enumeration. The spec's §3 and §6 carry the
    detail. Source: owner, 2026-09-05, batch 17 report.
30. **Batch 17 standing approvals + Rust-native dev tooling — ruled
    2026-09-05.** All five retrospective proposals approved with
    one correction: the tooling is Rust, not Node. Applied: the
    report-claims checker mandate on process.claims-match-artifacts
    (root copy; HR-061 ports the checker to a cargo bin and retires
    the interim script); the measurement clause on
    writing-style.code-comments (both copies); the exact-pins check
    extended to **/Cargo.toml with the bare-range residual noted
    (both copies); the sanctioned-forms bullet on
    houserules.tests-clean-scratch-dirs (the grep check parked
    unless violations recur); the new standing rules
    process.closure-claims-carry-enumeration and
    process.review-findings-are-claims-too (both copies). The
    owner's words: no Node tools in the codebase. Controller
    interpretation pending confirmation: make-corpus.mjs stays as
    the one interim exception (it drives the frozen JS and retires
    with the corpus at phase 5). Source: owner, 2026-09-05,
    batch 17 report.
31. **Well-maintained libraries over custom code, governed — ruled
    2026-09-05.** The preference existed in quality.principles
    (and tag-pilot); the owner adds the governance:
    quality.well-maintained-libraries lands standing in both
    copies. The orchestrator researches candidate libraries at
    SPEC time with maintenance evidence — never the implementers,
    never at plan time — and the owner rules the choice. Wherever
    the codebase is touched, custom code a library should replace
    is refactored in that batch, folded into the spec and plan;
    deferring the rewrite is not an option. After the Rust rewrite
    completes (no node/TypeScript/vitest traces), a mandatory
    whole-codebase sweep verifies the untouched remainder abides —
    riding the phase-5 repository sweep (§5.24). Enforced from
    this ruling forward; the batch 18 spec amends to comply.
    Source: owner, 2026-09-05, batch 18 spec gate.
32. **Spec before plan, strictly — ruled 2026-09-05.** A plan
    written before its approved spec is nil: deleted, and the
    planning phase redone from the spec — never re-anchored. Ruled
    when the batch 18 plan predated its spec; the plan was voided
    and rewritten from the approved spec. Applied as a
    process.brainstorm-first body bullet in both copies. Source:
    owner, 2026-09-05, batch 18 gate.
