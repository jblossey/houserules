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
never touched again. Runnable today from a local clone via the compiled
binary (`cargo install --path crates/houserules`, then `houserules init`);
the JS form retired at batch 18 when the payload flipped to the binary, and
distribution channels land at phase 4 (HR-048).

The two strongest reasons:

1. **The project repo is the only place everything works.** Generated rules
   and the preloaded knowledge skill must be project files (plugin cannot
   contribute them); CI gates run without Claude Code; the hook runs before
   any install. Vendoring is not a compromise — it is the only layout in
   which every consumer (harness, CI, hook, subagent) finds what it needs.
2. **The manifest split solves update-vs-ownership.** Kit-owned files
   (`tools/`, agent templates, orchestrating/finishing skills, hook) stay
   upgradable byte-for-byte; seed-once files (schemas, topics, backlog,
   evals, CI workflow, AGENTS.md, CLAUDE.md, settings) belong to the project
   from the first write. `houserules files` prints the split; `.houserules.json`
   records the installed version.

Harness portability comes free: the payload is plain JSON, markdown, and
POSIX shell; the `houserules` binary reads and writes it (amended at
batch 18 T6, docs/specs/2026-09-05-batch-18-phase3.md §1 —
`houserules.payload-runs-on-builtins` carries the full contract and its
history). Another harness can read `knowledge/*.json` directly or shell
out to the `houserules` binary; nothing depends on Claude Code except the
`.claude/` conventions, which other tools ignore harmlessly.

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
   and the tests. Done in a45961e on 2026-08-31.
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
   in `package.json`, the headers, the README sentence. Done in 6fc4244 on
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
   `vitest.config.mts`. (a) is backlog item HR-001; (b) was done in 6fc4244
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
   repository installs its own kit: `init --dir . --id-prefix HR`, then
   a repository-specific `AGENTS.md` (with `CLAUDE.md` as its one-line
   `@AGENTS.md` Claude Code pointer, batch 27 T4, HR-136), areas
   (`template`, `cli`, `tests`, `docs`), topics, and a backlog that
   holds the open work items. The CLIs
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
   `init`/`update` between releases); deferring. Done in 3b7d0f3 on
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
   (anthropics/claude-code#26794). Done in 757f127 on 2026-08-31.
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
    points at the existing release commit (c44e7c4) so the grammar is
    uniform from the first release, and `houserules-v0.2.0-alpha`
    stays as history. Unblocks HR-048's channels and HR-049. Source:
    owner, 2026-09-04.
    Supersession by 5.71 was REJECTED at 5.72: plain `v<version>` stands.
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
33. **Shipped prose that states a measurement is a claim — ruled
    2026-09-06.** process.claims-match-artifacts' body extends: a
    shipped doc or comment sentence stating a measurement cites a
    retained capture at a durable path, exactly as a report
    sentence does, and a fix round's sweep re-opens every doc
    sentence the round touched. The batch's dominant failure
    class (T4 install.rs, T5 mapping table, T6 frozen-sha
    comment); the claims checker cannot see shipped files, so the
    manual sweep is the net. Source: owner, 2026-09-06, batch 18
    report.
34. **TDD mode labels are measurements — ruled 2026-09-06.**
    process.tdd's body gains: a mutation proof's RED shows the
    test FAILING under the mutation (a capture showing passes
    proves nothing), and a post-commit recapture is
    `reconstructed` regardless of what also ran earlier. Both
    shapes were round-0 criticals at batch 18 (T2, T1). Source:
    owner, 2026-09-06, batch 18 report.
35. **make-corpus.mjs interim exception confirmed — ruled
    2026-09-06.** The owner confirms decision 30's recorded
    exception: tools/make-corpus.mjs stays the one Node dev tool
    until it retires with the corpus and bin/houserules.mjs at
    Tier-2 phase 5, where the mandatory no-Node sweep runs.
    Source: owner, 2026-09-06, batch 18 report.
36. **Batch 18 accepted; phase 4 gated next — ruled 2026-09-06.**
    Tier-2 phase 3 accepted as merged (main c82fe1e). The next
    batch gates phase 4: HR-048 delivery channels, HR-049's
    pinned tag, macOS signing — unblocking HR-063/064. Source:
    owner, 2026-09-06, batch 18 report.
37. **Phase-4 gate rulings — ruled 2026-09-06.** The batch 19
    spec approved as drafted. Release pipeline: cargo-dist
    (0.32.0 at research; active through 2026, absorbed Astral's
    fork upstream) over taiki-e's quieter action and a
    hand-rolled matrix — pinned exactly at adoption with the
    full vet. macOS binaries ship unsigned with the documented
    quarantine note; signing revisits at 1.0. Source: owner,
    2026-09-06, batch 19 gate.
38. **Command fields are literal — ruled 2026-09-07.**
    process.claims-match-artifacts' body gains: a command field
    is the literal command copied from the terminal — never
    retyped, never annotated inline; labels and notes go beside
    the field; a reviewer paste-runs command fields, and a field
    that cannot paste-run is a finding by itself. The class
    failed four review rounds at batch 19 (T1 r1/r2, T2 r1/r3);
    HR-071's checker lint catches the mechanical shapes, the
    rule names the discipline. Source: owner, 2026-09-07,
    batch 19 report.
39. **A flip closure sweeps the retired form — ruled
    2026-09-07.** process.closure-claims-carry-enumeration's
    body gains: a migration or flip closure names its zero-hit
    sweep of the RETIRED form (the grep pattern and its
    variants), rerunnable at HEAD; the enumeration is of the old
    form's residue, not the new form's presence. Ruled with the
    full context of the batch-18 T6 and batch-19 T2 failures —
    presence enumerations confirm known work, only residue
    sweeps falsify closure. Source: owner, 2026-09-07, batch 19
    report.
40. **Batch 19 accepted — ruled 2026-09-07.** Phase 4 accepted
    as merged (main 253746a) under the two-step reality; the
    step-two queue stands (HR-068 token → first release →
    runbook step 6). The controller selects the next batch and
    presents its gate. Source: owner, 2026-09-07, batch 19
    report.
41. **Phase-5 gate — ruled 2026-09-07.** The batch 20 spec
    approved as drafted: the complete JS retirement closing
    HR-047 (every test mapped ported-or-retired under a closure
    enumeration; the parity net retires with its purpose
    complete), HR-066/071 shipping the checker through the
    binary with the paste-run lint, HR-067's skill clauses, one
    shared evals rerun, and the §5.24+§5.31 closing sweep with
    the permanent residue gate. Source: owner, 2026-09-07,
    batch 20 gate.
42. **The corpus boundary correction — ruled 2026-09-07.** The
    batch-20 spec's replacement claim was wrong: the goldens
    cover check/render/read-for only. The remaining corpus
    slices leave the frozen contract per the batch-18 §3
    criterion — re-baselined as reviewed Rust-generated goldens
    via gen-goldens as T3's first act, the parity suites
    flipping to golden-reading in the same commit as the corpus
    deletion; a reading test never outlives, nor precedes the
    replacement of, the artifact it reads. Found by the T1 r2
    review's deletion-derivation (22 cargo tests broke under
    the trial deletion). Source: controller under the approved
    spec's own criterion, 2026-09-07.
43. **HR-073 — release-please goes rust — ruled 2026-09-08.**
    The config's `release-type: node` reads the package.json
    Tier 2 retired and throws MissingRequiredFileError on the
    first push to main after batch 20 merges (v17.11.2 source,
    getBranchComponent → getPkgJsonContents). Ruled: switch to
    `release-type: rust` at crates/houserules on the batch-20
    branch before the merge — plain v-tags kept, extra-files
    re-derived — validated locally as far as possible (config
    schema, the pinned action's source, dist plan); the live
    proof stays the owner-attended first release (HR-068 step
    two). Source: owner, 2026-09-08, batch 20 report.
44. **Batch 20's T3 standing amendments adopted — ruled
    2026-09-08.** Both standing-entry changes the retirement
    forced are adopted as applied in-tree:
    `houserules.pnpm-only` re-grounds on cargo (`cargo add
    <crate>@=<version>`) with the prior pnpm ruling recorded as
    history, and `houserules.template-is-the-source` moves its
    byte-pin's named home to crates/houserules/tests/dogfood.rs
    with the frozen bin/houserules.mjs stated retired; ids
    permanent, policies unchanged. Source: owner, 2026-09-08,
    batch 20 report.
45. **The cli area's home — ruled 2026-09-08.** The cli
    knowledge area points at `crates/houserules/src/**` (was
    `bin/**`, dead after the JS entrypoint retired);
    quality.absence-is-designed and houserules.live-run-recipe
    load there. HR-074 files the dead-glob gate that catches
    this class. Source: owner, 2026-09-08, batch 20 report.
46. **quality.well-maintained-libraries's sweep bullet — ruled
    2026-09-08.** Body[2] restated past tense per the T4
    review's draft: the mandatory whole-codebase sweep ran at
    batch 20 T4 (§5.24), named one candidate, adopted none
    under spec §8; completed with the §5.47 outcome. Source:
    owner, 2026-09-08, batch 20 report.
47. **walkdir adopted — ruled 2026-09-08.** The owner weighed
    maturity (BurntSushi, 607M downloads, finished scope) over
    the include_dir staleness precedent: walkdir enters
    exact-pinned in a future batch and replaces the duplicated
    dev-bin walkers. HR-079 carries the work. Source: owner,
    2026-09-08, batch 20 report.
48. **security-hygiene.exact-pins's examples — ruled
    2026-09-08.** The root copy's summary reorders its CLI
    examples cargo-first (`cargo add <crate>@=<version>`,
    `pnpm add --save-exact`); the template copy stays
    ecosystem-agnostic as shipped. Found by the T4 sweep,
    applied under this ruling. Source: owner, 2026-09-08,
    batch 20 report.
49. **claims-match gains the commit-message bullet — ruled
    2026-09-08.** A commit message body is a claim like a
    report sentence: counts recomputed from the artifact before
    committing; a post-commit error is recorded in the report
    for aggregation to carry, never amended, never silent.
    Both knowledge copies. Source: owner, 2026-09-08, batch 20
    report.
50. **closure-claims gains the executable-enumeration bullet —
    ruled 2026-09-08.** The enumeration artifact is executable:
    a script whose own run prints the counts the claim cites,
    not a static dump or a hand-asserted mapping. Both
    knowledge copies. Source: owner, 2026-09-08, batch 20
    report.
51. **The escape hatch's upstream destination — ruled
    2026-09-08.** The seeded closing-act false-positive branch
    names where upstream is: an issue at
    github.com/jblossey/houserules. Template edit; rides the
    next template-touching batch with HR-078 (HR-080 carries
    it). Source: owner, 2026-09-08, batch 20 report.
52. **Source-comment citations — ruled 2026-09-08.** A
    long-lived source comment states the rerunnable command and
    its measured counts inline, self-contained; a
    batch-workspace path may ride as provenance, never as the
    only evidence — a clone carries no workspace. Bullet on
    process.evidence-outlives-the-session, both copies; the
    shipped Limits bullets already satisfy it (commands and
    counts inline), so no code edit is owed. Source: owner,
    2026-09-08, batch 20 report.
53. **Batch 20 accepted — ruled 2026-09-08.** Tier-2 phase 5
    accepted as merged (main b947e78): the repository and kit
    are one static binary plus POSIX shell; HR-047 closes the
    Tier-2 rewrite; release-please live-proven under the §5.43
    rust config on the merge push itself. The controller
    selects the next batch and presents its gate; the step-two
    release set stays owner-attended (HR-068 first). Source:
    owner, 2026-09-08, batch 20 acceptance.
54. **Batch-21 gate — ruled 2026-09-08.** The spec approved as
    drafted: the gate cluster (HR-074 dead-glob, HR-075
    ephemeral-path, HR-076 emitter round-trip, HR-077 audit
    sanctioned-fail annotation, HR-086 resolved as a textList
    minLength in the deliverables schema, both copies) plus the
    §5.47 walkdir adoption (HR-079). Each gate proves itself on
    a seeded instance of its own batch-20 incident. Source:
    owner, 2026-09-08, batch 21 gate.
55. **HR-086's scoping ratified — ruled 2026-09-11.** The
    textList minLength ships on seven of the nine fields; the
    two retrospective tasks lists keep the old floor (measured:
    84 of 90 real entries are one-character task references; a
    shared floor rejects real data; the shredded-prose incident
    class is fully covered by the seven). The spec gains the
    recording amendment. Source: owner, 2026-09-11, batch 21
    report.
56. **The dead-glob gate's spec account — ruled 2026-09-11.**
    The batch-21 spec §2 gains the four-part amendment: the
    git-tracked primary derivation, the two documented
    fallbacks, the zero-to-one cliff, and the undecodable-entry
    skip — the gate's whole behavior lives in the spec, not
    only in doc comments. Source: owner, 2026-09-11, batch 21
    report.
57. **Ephemeral paths in narrative — ruled 2026-09-11.**
    Cite-into-artifact is the standing answer: a report that
    must discuss an ephemeral path names its class in prose and
    cites the literal in a retained workspace capture; no
    checker escape (an escape is what drift hides behind). One
    Limits sentence in report_claims.rs records the pattern on
    the next implementation branch. Source: owner, 2026-09-11,
    batch 21 report.
58. **Batch-21 standing bullets — ruled 2026-09-11.** Four
    bullets applied: claims-match gains re-derive-after-any-
    world-change and observed-vs-inferred; closure-claims
    gains subject-derived-patterns-with-a-spelling-probe;
    doc-comments gains doc-behavior-statements-are-claims.
    Both knowledge copies where shared; each cites its
    measured batch-21 incident. Source: owner, 2026-09-11,
    batch 21 report.
59. **Record dispositions — ruled 2026-09-11 ("Fix both").**
    HR-091/HR-092 attribute to batch 21's window with a
    disclosed side-PR note (the check-backlog warns retire);
    HR-096 files the restatement of the nine pre-batch-21
    unreachable SHA markers to event locators per
    cite-durable-refs, riding a future batch. Source: owner,
    2026-09-11, batch 21 report.
60. **Batch 21 accepted; pause — ruled 2026-09-11.** The gates
    batch accepted as merged (main 19b75c4). No batch 22
    selection now: the backlog waits, the step-two release set
    stays owner-attended (HR-068 first), and the template
    cluster (HR-078/080/090/094 + riders HR-095/096) stands as
    the recorded candidate for whenever work resumes. Source:
    owner, 2026-09-11, batch 21 acceptance.
61. **Batch 22 selected and its spec approved — ruled
    2026-09-11.** The pause lifts: batch 22 is the stewardship
    trio HR-105/098/099 plus HR-089 as a rider ("New trio +
    HR-089"); the template cluster moves to recorded candidate
    for batch 23. The spec (docs/specs/2026-09-11-batch-22-
    stewardship.md) is approved as drafted with three design
    rulings folded in: the knowledge schema gains an optional
    status field (active default | superseded | retired) as
    the archive sweep's retirement signal; both in-code docs
    standing rules revise to the drafted wordings (no history
    narration in code); sha2 is adopted for the update
    baselines with entry-level upsert for SEED_ONCE knowledge
    topics. Source: owner, 2026-09-11, batch-22 shaping.
62. **doc-comments wording trimmed to the summary cap — ruled
    2026-09-11.** The 5.61 doc-comments wording measures 166
    characters against the knowledge schema's 160-character
    summary cap (found live by T3a's gate run). Ruled: trim,
    not a cap raise and not a body split. The shipped summary
    is "Document every exported symbol with the language's
    doc-comment convention: the current contract. Name things
    so the code reads without comments." — dropping only
    "concise and complete", which writing-style.principles
    already demands. The cap stays at 160 for all adopters.
    Source: owner, 2026-09-11, batch-22 T3a escalation.
63. **Batch-22 standing rulings — ruled 2026-09-12.** Four
    applied: claims-match gains the five drift classes (rewritten
    commands, unmeasured counts, stream-index citations, false
    universals, unchecked transcriptions) plus re-derive-at-final-
    HEAD; closure-claims gains corpus-plus-pattern derivation with
    a retained spelling probe; code-comments gains survey-derived
    sweep verification; doc-comments gains same-commit cross-
    reference re-resolution. Plus one new standing entry,
    process.suggestions-are-unverified (another agent's suggested
    wording is a draft, not evidence). The deterministic
    citation-lines check files as HR-107. Source: owner,
    2026-09-12, batch 22 report.
64. **Batch 22 accepted; batch 23 is the template cluster —
    ruled 2026-09-12.** The stewardship batch accepted as merged
    (main 885d5a3, 18 checks green on the third run; the two
    .gitattributes rounds included). Batch 23 selected: the
    template cluster HR-078/080/090/094 with riders HR-095/096
    and HR-103's five template defects, all under one shared
    evals rerun. The owner-attended release set (HR-068 first)
    stays parked for the owner. Source: owner, 2026-09-12,
    batch 22 acceptance.
65. **Batch-23 spec approved; the release surface is broken;
    batch 24 is the repo-and-setup batch — ruled 2026-09-12.**
    The template-cluster spec is approved as drafted (every
    template sentence verbatim). In the same ruling the owner
    records that the GitHub releases page shows neither the
    source zip nor the per-platform binaries and that the
    documented installation methods do not work (HR-108), and
    directs that the batch after batch 23 makes the whole
    repository and the setup around it right. Source: owner,
    2026-09-12, batch-23 spec gate.
66. **Batch 23 accepted; batch 24 is the repo-and-setup batch —
    ruled 2026-09-12.** The template cluster accepted as merged
    (main 296781b, all eight required checks green on the first
    push). Batch 24 proceeds per HR-108's direction: the
    releases page (no source zip, no binaries), the broken
    installation methods, and the whole setup around the
    repository, absorbing HR-048/068/081/082 and the runbook
    step-two set (HR-049/063/064/069/070); the owner-attended
    decisions in that set (HR-068's token choice first) surface
    as gate questions. Source: owner, 2026-09-12, batch 23
    acceptance.
67. **Batch-24 gate rulings — ruled 2026-09-12.** Four: the
    release trigger is a fine-grained PAT in a repository
    Actions secret (the Dependabot-token precedent); every
    documented install path pins to the releases/latest
    download alias so a release never forces a README edit
    (HR-049 closes by design change — nothing version-bearing
    remains to follow); the version is 0.3.0 with the -alpha
    suffix dropped (the latest alias excludes prereleases, and
    0.x already signals pre-1.0 — supersedes release PR #18's
    1.0.0-alpha); the 0.2.0-alpha remains stay as history.
    Source: owner, 2026-09-12, batch-24 spec gate.
68. **Batch 25 directed: the v1-readiness batch — ruled
    2026-09-12.** After batch 24, batch 25 makes the repository
    v1-ready: everything the controller deems necessary for v1,
    plus a complete owner review of every default the template
    ships — delivered as a complete list/spreadsheet where the
    owner ticks each item template-appropriate or not, requests
    specific changes, and adds items still missing (HR-113; the
    row set derives from template/** itself). Source: owner,
    2026-09-12, during batch 24.
69. **Batch-25 sweep addition: the gap rows — ruled
    2026-09-12.** The template-defaults review list (5.68,
    HR-113) also enumerates what is used but NOT shipped: this
    repository's own machinery versus template/, and the
    tag-pilot checkout's evolved state versus template/ —
    tag-pilot read strictly read-only, per its standing rule.
    Each gap row carries the same tick/change/add affordances.
    Source: owner, 2026-09-12, during batch 24.
70. **Agent effort levels pinned — ruled 2026-09-12.** The
    controller session runs at effort high (settings.json
    effortLevel, both copies), task and branch reviews at high,
    implementers at xhigh — pinned in the agent templates'
    frontmatter (effort:, verified against Claude Code's
    current sub-agents and settings docs) and recorded as a
    process.model-policy body bullet in both knowledge copies.
    The template edits to implementer.md and task-reviewer.md
    trigger process.evals-rerun; the rerun is booked before
    this batch's branch review. Source: owner, 2026-09-12,
    during batch 24.
71. **HR-081's tag-format ripple: include-component-in-tag
    reverts to true — ruled 2026-09-12, superseding 5.20 on
    complete facts.** 5.20's actual rationale (verbatim):
    "Every binary-delivery channel (mise ubi, asdf, taps)
    defaults to the `v*` grammar; ruled at the cheapest moment
    (one release, zero external consumers)." This is a
    channel-default argument, not a README-URL argument.
    BaseStrategy.getComponent (src/strategies/base.ts:178-183,
    the release-please version googleapis/
    release-please-action@45996ed bundles) returns '' whenever
    include-component-in-tag is false, before it ever reads
    either package's configured component, so the
    linked-versions plugin HR-081 needs (crates/houserules
    linked to a root package that sees every commit unfiltered)
    can never match while it stays false: reaching HR-081 at
    all requires reversing 5.20's tag shape.
    Channel by channel, verified against each tool's current
    docs and the tree's own present state: mise's old `ubi:`
    backend is the one channel spec 2a's releases/latest/
    download/<asset> alias (and HR-070's move to mise's
    `github:` backend) dissolves outright. The README does not
    dissolve yet: README.md:73-77 and :118-126 still pin
    `releases/download/v0.2.0-alpha/...` today, inside
    release-please's own `x-release-please-start-version`
    blocks (`/README.md` is in `extra-files`), so an
    unretired block would be rewritten to `v0.3.0` -- a tag
    that will not exist, since the real tag is
    `houserules-v0.3.0`. The amended plan removes that
    interval: T3a retires the blocks and their extra-files
    entry ON THE BATCH BRANCH, before any release PR
    regenerates against the merged main, so no wrong-shape tag
    is ever written into the README; the README already says
    today's URLs 404
    (README.md:85-95), so this changes which wrong tag a
    404'ing URL names, not whether it 404s. asdf and
    a Homebrew/Scoop tap are HR-048's own open deliverables and
    do NOT require the plain grammar: mise's `github:` backend
    itself takes a documented `version_prefix` tool option
    (`version_prefix = "houserules-v"`) for exactly this shape;
    an asdf plugin's `bin/list-all`/`bin/download` are the
    plugin author's own shell scripts with no format the asdf
    core enforces; a Homebrew formula can `url ... , tag:
    "houserules-v0.3.0"` with an explicit `version` field when
    auto-detection does not infer one. The flip costs each of
    these three not-yet-built channels one explicit option or a
    few lines of parsing, not a blocked or reduced channel --
    stated plainly since HR-048 has not built any of them yet
    and each is cheaper to write against the tag shape that will
    actually exist than to write against one this repository is
    reversing.
    The `v0.2.0-alpha` alias tag 5.20 minted stays as the
    one-time history it already is; no future release mints a
    matching plain-`v` alias. The README's own plain-v URLs stop
    being a reason to mint one the moment HR-049's re-walk lands
    (mise's `github:` backend and asdf/the tap, once built, take
    `houserules-v<version>` directly) -- until then the README
    names whichever wrong tag the most recent release-please run
    last wrote into it, exactly as it 404s today.
    `true` is also BaseStrategy's own default and restores the
    `houserules-v<version>` shape the first real release
    (`houserules-v0.2.0-alpha`) already used; release.yml's
    tag-push glob matches either shape unaffected.
    HR-048's own body is updated in this same turn to drop its
    now-reversed "plain v<version>" prerequisite line. Source:
    controller under batch-24 T1 fix round 2's own ruled
    latitude, 2026-09-12.
72. **The 5.71 supersession REJECTED; 5.20 reaffirmed — ruled
    2026-09-12.** The owner keeps the plain `v<version>` tag
    shape. 5.71's channel analysis stands as recorded fact, but
    the ruling is reversed: releases tag `v0.3.0`, and T1
    reopens to deliver HR-081's template attribution WITHOUT
    the component in the tag — the linked-versions mechanism is
    known inert under that constraint, so a different mechanism
    is required. Source: owner, 2026-09-12, the batch-24
    checkpoint.
73. **The kit-version restamp folds into the release PR, not a
    post-merge step — ruled 2026-09-13.** The branch review
    (batch 24) walked the release PR release-please's own config
    would open through this branch's own gates and found it red
    by construction, two ways: the seeded `template/.github/
    workflows/knowledge.yml`'s `x-release-please-version`
    extra-files entry collides with `houserules.payload-stamp-
    gate` and `check-commit`'s per-commit co-change on every
    release commit, and `.houserules.json`'s version stamp lags
    `crates/houserules/Cargo.toml`'s bump on the release PR
    itself, failing `dogfood.rs`'s version test before anyone can
    merge. Fix: the knowledge.yml entry retires (its installer
    URL moves to `releases/latest/download`, needing no
    per-release rewrite), and `extra-files` gains one replacement
    entry, `/.houserules.json` (`json`/`$.version`), so the
    version restamp rides the release PR instead of a separate
    post-merge commit. Final state: `extra-files` carries exactly
    that one entry. `houserules.post-release-restamp` and
    docs/runbook.md's step 5 now cover baseline drift only.
    Source: controller under the branch-fix task's own ruled
    latitude (spec-compliant per docs/specs/2026-09-12-batch-24-
    repo-and-setup.md §2's "merges green unattended" goal),
    2026-09-13, branch review issues 1-2. RATIFIED by the owner
    2026-09-13 at the batch-24 report, with release PR #23's
    live diff as the proof (the version rides the PR; the PR is
    green unattended).
74. **HR-081's mechanism ratified: the payload-stamp witness —
    ruled 2026-09-13.** Template attribution works by
    construction: `crates/houserules/payload.stamp` carries a
    SHA-256 digest over `git ls-files -z -- template`, the
    `payload-stamp-gate` bin verifies it in lint (`--write`
    regenerates), `check-commit`'s opt-in `per_commit` co-change
    check pairs every `template/**` commit with the stamp (a
    parent-tree guard exempts pre-mechanism history), so every
    template change touches `crates/**` and release-please
    attributes it with no config extension. Chosen after the
    5.72 rejection made linked-versions inert and the
    single-root-package alternative failed at source (the
    bundled CargoToml updater throws on a `[package]`-less
    manifest). The close-gate proof (HR-081): the first
    template-only change on main after v0.3.0 must bump a
    release PR. Source: owner, 2026-09-13, the batch-24 report.
75. **Three standing rules adopted from the batch-24
    retrospective — ruled 2026-09-13.** The owner adopted all
    three branch-review proposals as standing:
    `process.gates-cover-generated-commits` (walk every commit
    author class — bots and generated commits included —
    through a new gate on paper before it ships),
    `process.fix-proofs-extend-the-reviewers-run` (a fix
    round's proof extends the review's own reproduction, never
    a narrower fresh one), and
    `process.owner-rulings-need-owner-supersession` (a
    controller with latitude proposes a reversal of an owner
    ruling and builds nothing on it until the owner rules).
    Entries live in `knowledge/process.json` with
    `standing: true`. Source: owner, 2026-09-13, the batch-24
    report.
76. **Batch 24 accepted; T2 runs immediately — ruled
    2026-09-13.** The owner accepted the batch's built-and-
    merged state (main eeb5708; first live hop verified;
    release PR #23 green unattended) and directed the T2
    release now: PR #23 merges ff-only from the CLI as the
    watched live release — tag v0.3.0 → dist → five archives +
    checksums + installer, marked latest. HR-048 and HR-068
    close on that chain; T3b's live install verification and
    the batch's ticks/sweep follow on the close branch. Source:
    owner, 2026-09-13, the batch-24 report.
77. **T2's create-collision recovery: delete the asset-less
    release object and re-run the failed host job — ruled
    2026-09-13.** The live chain broke at its last hop:
    release-please (on the PAT) created release v0.3.0 on the
    tag push, then dist's host job failed at `gh release
    create` ("a release with the same tag name already
    exists"). Both tools created the release. The owner ruled
    the pipeline-authored recovery over a manual asset upload:
    the release object was deleted (tag v0.3.0 kept), the host
    job re-run, and dist recreated the release with all 16
    assets; `releases/latest/download/...` verified 200 live.
    The permanent fix is HR-118: `create-release = false` in
    dist-workspace.toml (proven against pinned dist 0.32.0 to
    regenerate the host step into `gh release upload` + `gh
    release edit`, cooperating with release-please's published
    release), riding the batch-24 close branch. Source: owner,
    2026-09-13, T2.
78. **process.gates-cover-generated-commits extended to
    generated resources — ruled 2026-09-13.** The owner
    extended the standing rule adopted at 5.75 the same day:
    the pre-ship walk covers every generated RESOURCE the
    automations write (releases, tags, PRs) with one named
    owner per resource, not only the commit author classes.
    Proof of the gap: the T2 create-collision (5.77) — two
    automations both created the v0.3.0 release object — hit
    live hours after the commit-scoped rule shipped. The entry
    in `knowledge/process.json` carries the trimmed summary and
    the extension bullet. Source: owner, 2026-09-13, the
    batch-24 close report.
79. **Batch-25 gate rulings — ruled 2026-09-14.** Four rulings
    settle the v1-readiness spec (docs/specs/2026-09-14-batch-
    25-v1-readiness.md): (1) the template-defaults review is an
    interactive claude.ai artifact page that is NOT published
    publicly — private by default on the owner's account, the
    link never shared, the repository holding the durable
    record of every decision; (2) riders HR-101 and HR-102 ride
    the batch, HR-072 and HR-088 stay deferred — seventeen
    items; (3) the 1.0.0 cut rides the batch close on a
    Release-As footer, watched end to end; (4) macOS signing
    stays deferred-documented (HR-121 books it on adopter
    friction), discharging 5.37's revisit-at-1.0 obligation;
    the documented bypass wording amended 2026-09-15 (owner
    ruling at the batch-25 report): the current path is run
    once and let macOS block it, then System Settings >
    Privacy & Security > Open Anyway - Apple removed the older
    right-click-Open in macOS Sequoia.
    Source: owner, 2026-09-14, the batch-25 spec gate.
80. **The template-defaults review ruled - 2026-09-14.** The
    owner reviewed all 258 rows on the private page (150 keep,
    86 drop, 15 change; the row-level record is
    docs/specs/2026-09-14-batch-25-review-decisions.md, which
    carries every cross-cutting directive verbatim). The big
    strokes: the ask-the-user pattern becomes ask-the-
    owner/decider across every template file; no concrete
    houserules-repo reference may appear in any template file;
    template wording de-Rusts (cargo examples become generic);
    the seeded CI workflow leaves the payload - init/migrate
    elicits the project's host and generates the gate;
    ff-only-merges, no-coauthor, sequential-agents, and the
    effortLevel pin leave the template's fixed rules (the
    init/migrate flow asks each project's own discipline);
    model-policy becomes a recommendation; standing flips for
    contract-refresh-sweep, gate-shell-chains,
    skills-for-procedures, and keep-knowledge-current (the last
    standing in this repository too); Keep on a GAP row means
    ADOPT (owner-confirmed) - eight repo rules enter the
    template de-referenced; 46 of the 49 tag-pilot rows are dropped
    (three keep-ticks stand, ruled at 5.81 after a controller
    mis-aggregation recorded them as drops). The page's
    decisions database and its rows file are purged after T2
    consumes them (owner directive: tag-pilot sensitivity);
    tracked files quote no tag-pilot content, not even entry
    names. Rider (controller, 2026-09-15, under the granted
    design latitude, accepted with the built state at 5.83):
    the CI-workflow directive was implemented as a CONDITIONAL
    SEED, not a payload removal - the workflow file stays in
    template/, init seeds it only for a GitHub-hosted origin
    and prints a skip note elsewhere, update backfills a later
    GitHub origin, and the migrating-knowledge skill instructs
    generating the host's own gate. Source: owner, 2026-09-14,
    the review read-back and the three confirmation rulings.
81. **Two T2-review reconciliations - ruled 2026-09-15.** (1) The
    three tag-pilot rows the owner ticked Keep (tests-clean-temp-
    dirs, pointer-sweep-unfiltered, report-evidence-verbatim) ARE
    adopted into the template, de-referenced - the drop-all
    statement in 5.80 and the addendum was the controller's
    mis-aggregation of the read-back, not the owner's tick; their
    generic content is published by this ruling, while the other
    46 rows' identifiers stay workspace-only. (2) The seeded
    commit-msg hook DROPS its attribution-trailer gate: the
    template hook keeps only the Conventional-Commits checks and
    attribution enforcement is fully project-ruled. THIS
    repository keeps its own trailer-gating hook through the
    overrides model (security-hygiene.no-coauthor is standing
    here), the same pattern as its other retained rules. Rider
    (controller, same checkpoint): .houserules.json's overrides
    also pin 16 knowledge-entry ids - this repository keeps its
    own referenced, batch-cited versions of the entries the
    template de-referenced under directive 2, and the overrides
    list is what holds them against future template updates; the
    count is the audit's cross-check against the file. Source:
    owner, 2026-09-15, the T2 review checkpoint.
82. **The v1 surface freeze pinned — derived 2026-09-15.** The
    CLI command set and both schema copies' constraint surface
    are the 1.0 contract (spec §3, HR-113). The 20 frozen
    `Command` variants `crates/houserules/src/main.rs`
    defines, captured verbatim by `houserules --help`:
    `render`: Writes every stale generated knowledge file, or
    lists them with `--check`; `check-knowledge`: Validates
    the knowledge base: schema, cross-entry invariants, and
    every generated file's freshness and budget; `get`: Prints
    one or more items by id, each resolved by its own shape: a
    backlog item (`<idPrefix>-\d{3}`, the project's own
    stamped prefix), an amendment or parked item (`A-\d{2}`,
    `PP-\d+-\d{2}`, both kit-fixed regardless of idPrefix), or
    otherwise a knowledge entry; `list`: Lists backlog items,
    optionally filtered; `batch`: Prints one development
    batch's summary and item rows; `set`: Applies
    `field=value` assignments to a backlog item and rewrites
    its file; `check-backlog`: Validates the backlog: schema,
    cross-file invariants; `audit`: Builds the rule package
    for a git range and runs every member's deterministic
    check; `validate`: Validates one or more deliverable JSON
    files against `.claude/schemas/deliverables.json`;
    `stats`: Aggregates rule violations and unused injected
    ids across a workspace directory's JSON deliverables;
    `index`: Lists knowledge-entry index rows, optionally
    filtered; `for`: Prints the rule package one or more
    changed paths pull in: their areas' rule-shaped entries,
    plus every entry whose `verify` names one of the paths;
    `topics`: Lists every loaded knowledge topic's name, entry
    count, and title; `standing`: Lists the standing rules,
    rules before invariants; `check-commit`: Runs every
    `commits`-type knowledge check against a not-yet-committed
    message or a git range; `init`: Seeds the kit into a
    target git repository from the embedded payload; `files`:
    Prints the kit-owned and seed-once file lists the embedded
    payload defines; `update`: Syncs an already-`init`ed
    target's `KIT_OWNED` files from the embedded payload,
    deletes any retired kit file still present, and reports
    the stamped-to-running version drift;
    `check-report-claims`: Cross-checks one deliverable
    report's claims against the artifacts and git history it
    cites: redirected captures, truncation markers, listed
    commit shas, self-audit narrative, and the bounded
    no-execution paste-run lint over every captured command
    field; `archive`: Moves done/dropped backlog items, done
    batch entries, and superseded/retired knowledge entries
    into their `archive/` mirrors. The
    `no_subcommand_carries_a_long_about_beyond_its_short_one`
    test pins the count; a sibling test,
    `the_1_0_cli_surface_names_exactly_these_20_subcommands`,
    pins the names themselves (batch 25 T5 review: a renamed
    variant keeps the count-only test green,
    disclosed-mutation-proven by temporarily renaming
    `Files`). Schemas: `knowledge/schema.json`,
    `template/knowledge/schema.json`, `backlog/schema.json`,
    `template/backlog/schema.json`,
    `.claude/schemas/deliverables.json`, and
    `template/.claude/schemas/deliverables.json`, pinned as of
    this freeze (batch-25 T5) to the blob ids `git rev-parse
    HEAD:<path>` returns: 8a9987062e5b, 2a433a5f7b29,
    d4c195bc437c, c8dfca6cd118, d78e5a0fa50c, 0bb189143e34, in
    that order -- content hashes, so they carry their own
    identity with no commit sha needed. Post-1.0, a change to
    either surface is a breaking change: an owner ruling and a
    major-version bump, not a patch or minor release
    (`houserules.1-0-surface-is-frozen`). Source: spec
    docs/specs/2026-09-14-batch-25-v1-readiness.md §3,
    approved by the owner 2026-09-14; derived from the tree at
    T5, 2026-09-15.

83. **Batch 25 accepted; the close and the 1.0.0 cut proceed -
    ruled 2026-09-15.** The owner accepted the batch's built
    state (all six tasks closed on batch-25) and directed the
    full close: ticks and sweep, the empty Release-As: 1.0.0
    footer commit riding the final branch (exactly one
    ^Release-As: hit verified at aggregation), the fable branch
    review, the ff-only merge, and the watched v1.0.0 cut on
    the proven chain - the owner merges the release PR when it
    opens. Source: owner, 2026-09-15, the batch-25 report.
84. **Batch 26 (the README rework) runs owner-mode - ruled
    2026-09-15.** For this batch only, the owner suspended the
    spec gate and the subagent pipeline: the controller
    researches README practice, derives the ideal structure
    before consulting the current file, presents the comparison,
    triages the changes with the owner, and performs the rework
    itself. Triage rulings: strip all maintainer-evidence
    narration from README (provenance lives in specs and
    knowledge); the convergence thesis becomes a short
    plain-claims Why section; add a ToC, a CLI reference table
    of the 20 frozen subcommands, a Stability section stating
    the 5.82 contract, and a Contributing & support section;
    the Updating section folds under Install. Source: owner,
    2026-09-15, the README triage.85. **The install-and-update UX cluster is booked; the README
    gains an Uninstall section - ruled 2026-09-15.** Mid-batch
    26 the owner directed four additions. Three are booked as
    open backlog items for a future spec: `houserules update`
    also updates the installed binary (HR-133); the binary
    prints a small update-available header on every command
    once a newer release exists (HR-134); and the install path
    question - whether `~/.cargo/bin` fits a toolchain-neutral
    binary - awaits an owner ruling before any move (HR-135).
    The fourth lands in this batch: README states how to
    uninstall houserules, per install channel (HR-132 scope
    addition). Source: owner, 2026-09-15, mid-batch messages.
86. **Batch 27 directed: the install-and-update cluster plus the
    agent-agnostic kit - ruled 2026-09-15.** The owner scheduled
    HR-133 (update self-updates the binary), HR-134 (the
    update-available header), HR-135, and the new HR-136 (the kit
    works flawlessly with Codex and other harnesses that do not
    know Claude structures). HR-135 is ruled: `~/.local/bin` is
    the correct install path for a toolchain-neutral binary; the
    move carries a migration story for existing `~/.cargo/bin`
    installs. Source: owner, 2026-09-15, the batch-27 direction.
87. **Batch 28 directed: the issue batch, with the HR-098 repair
    folded in - ruled 2026-10-01.** The owner directed the open
    GitHub issues 35 to 40 into the backlog (HR-141 to HR-146)
    and their fixes into one batch; each fix starts with its own
    brainstorm. HR-098's live proof read negative (an enabled
    auto-merge waits for the approval ruleset; the admin bypass
    does not apply to it), and its repair rides the same batch.
    The repair is ruled: the auto-merge workflow approves a minor
    or patch Dependabot PR with the owner's token before it
    enables the merge, and a Dependabot group moves the
    `github/codeql-action` pair in one PR. Issue 41 (HR-147, the
    kit installs into a subdirectory) arrived mid-brainstorm and
    is ruled: batch 28 corrects the refusal text only; the
    subdirectory install stays open for its own spec. Issue 38
    (HR-144, the design-phase review gate) is ruled out of batch
    28: it forms batch 29 with HR-140, so review coverage and
    review cost get one design, with the surface ruling for the
    new deliverable kind taken there. The owner approved the
    written batch-28 spec the same day; HR-129 (the
    template-cluster riders) joins batch 29. Source: owner,
    2026-10-01, the batch-28 brainstorm and spec gate.
88. **Batch 28 checkpoint: the lint ships; the seeded CI installs
    the stamped version - ruled 2026-10-02.** The new citation
    lint of `check-knowledge` fails in every install of 0.3.0 to
    1.1.0 until `houserules update` runs, because the seeded CI
    installs the latest binary and those releases shipped a
    citation of an id the kit never seeded. The owner ruled:
    ship the lint as built, state the red first in the release
    note, and change the seeded workflow in this batch so that it
    installs the version `.houserules.json` records (HR-154).
    Two more rulings at the same checkpoint: the citation of
    `knowledge-base.cite-durable-refs` stays removed from the
    finishing-a-feature skill, and the entry is not seeded;
    issue 35 stays open, because its checker rule did not ship
    (HR-141). Source: owner, 2026-10-02, the batch-28 checkpoint.
89. **Batch 28 accepted; the rollout rulings - ruled 2026-10-02.**
    The owner reviewed and merged PR #46 and released 1.2.0.
    Four rulings followed at the rollout. (1) The controller
    edits the 1.2.0 release note with the text the owner
    approved: the `check-knowledge` red first, then the install
    step of the release tag. (2) The controller runs the
    seeded-repository proof on the owner's account; the run
    closes HR-154. (3) PR #48 shows the HR-098 repair working
    and still waiting, because it fell behind main: observe
    Dependabot's weekly run of 2026-10-09. (4) Ruling 3 is
    superseded the same day, on evidence the controller found
    after it asked: two weekly runs left PR #32 behind main
    untouched. The fix for a Dependabot PR that falls behind
    (HR-156) is designed now, as batch 30, before batch 29;
    PR #48 stays untouched and is its proof. Source: owner,
    2026-10-02, the batch-28 rollout. Later the same day the
    owner's account asked Dependabot to recreate PR #48, and it
    merged; the batch-30 spec, section 8, names the proof that
    replaces it.
90. **Batch 31 directed: process cost - ruled 2026-10-09.** Users
    report that houserules prolongs feature work. The analysis of
    2026-10-09 measured three causes over 26 batch workspaces:
    review effort lands on the process's own deliverables, the
    knowledge base only grows, and one pipeline serves every
    change. The owner directed batch 31 to build a risk-tiered
    pipeline (HR-159), severity by impact and a bounded review
    loop (HR-140, out of batch 29), and a mechanism that makes
    the knowledge base self-optimizing (HR-160), and to ship the
    cost analysis to adopters so that their setups self-optimize
    too (HR-161). An adopter updates after the release with as
    little friction as possible. Generated report facts (HR-158)
    are filed, not built. Numbered 90: item 89 rides the
    unmerged batch-30 branch. Source: owner, 2026-10-09.
91. **Batch 31: `stats` reads several workspaces; the release is a
    minor - ruled 2026-10-09.** `houserules stats` takes one or
    more workspace positionals (`<WORKSPACE>...`). The owner ruled
    that widening a positional's arity is not a surface change
    under 5.82: no subcommand or flag is added, removed, or
    renamed, and every existing call behaves the same. Batch 31
    ships as 1.3.0. Source: owner, 2026-10-09, the batch-31
    brainstorm.
92. **Batch 31: agents run in parallel on disjoint files - ruled
    2026-10-09.** At the spec gate the owner directed the parallel
    dispatch the cost analysis proposed into batch 31: tasks whose
    file sets are disjoint run at the same time, each in its own
    worktree, and the controller integrates them one at a time.
    The ruling supersedes `process.sequential-agents` (owner seed,
    2026-08-30) and the kit's sequential default. Source: owner,
    2026-10-09, the batch-31 spec gate.
93. **Batch 31: two loop defects from the first live run - ruled
    2026-10-09.** The first `stats` run over this repository's five
    most recent workspaces gave four `narrow` proposals whose
    findings sit on shipped README, runbook, skill, and knowledge
    text: in a repository whose docs are its product, prose is a
    contract surface. The owner ruled: `narrow` counts a finding as
    soft only when it is Minor or targets a deliverable. The run
    also re-proposes every standing demotion at every batch close.
    The owner ruled: an entry tagged `ruled-keep` gets no `demote`,
    `retire`, `narrow`, or `mechanize` proposal; it still counts
    toward the standing budget. Source: owner, 2026-10-09, the
    batch-31 live run.
94. **The adopter lead-time audit: six proposals, two batches -
    ruled 2026-10-10.** An adopter's audit of its batch 3 under
    houserules 1.2.0 measured 95 review findings, 30 of them
    critical, and 31% of 2,680 judged audit rows as boilerplate.
    Release 1.3.0 already covers severity by impact, the bounded
    review loop, risk tiers, and parallel agents. The owner ruled
    the rest into backlog items with this split. Batch 32 ships as
    a minor release now: template defects and several workspaces
    (HR-167), controller rules out of task audits (HR-168), a
    loading path for non-standing process rules (HR-169), and
    findings that name their caller and their check (HR-170).
    Batch 33 ships as 2.0.0: evidence by reference with a capture
    command and inline-output caps (HR-171, with HR-057 and
    HR-158), and a check type that runs a project linter
    (HR-173). The plan-sync narrowing of
    `process.rulings-to-file` (HR-172) waits for its own owner
    ruling. Source: owner, 2026-10-10.
95. **One major release after 1.4.0 - ruled 2026-10-10.** After
    batch 32 ships as 1.4.0, every backlog item and GitHub issue
    that needs a major version goes into one release, 2.0.0, with
    a proper upgrade path for adopters. Batch 33 is that release:
    its brainstorm collects the major-version items from the
    backlog and the GitHub issues, and its spec designs the
    upgrade path. Source: owner, 2026-10-10.
96. **Batch 33 (2.0.0): the eleven preparation decisions - ruled
    2026-10-10.** The owner ruled the decisions of the batch-33 spec,
    section 8. D1: 2.0.0 holds the major-version items, the upgrade
    path, and the items that change the same files. D2: from 2.0.0,
    an addition (a subcommand, a flag, an optional field, an enum
    value) is a minor change, because `update` then delivers the
    schema files; removing, renaming, and tightening stay major. This
    amends 82 from the 2.0.0 release on. D3: `update` delivers the
    schema files, the eval scenarios, and the seeded CI workflow
    through baselines, like the kit-owned files. D4: a 2.0.0 binary
    on a tree that is not migrated fails every gate with one named
    line and exit 2. D5: HR-129 and HR-153 move from batch 29 to
    batch 33; batch 29 keeps HR-144 and ships after 2.0.0 as a
    minor. D6: the kit seeds `knowledge-base.cite-durable-refs`
    (issue 50, HR-178) together with HR-131's repair, a
    `defaultBranch` key in `.houserules.json`; this supersedes the
    "not seeded" ruling of 88. D7: the subdirectory install (issue
    41, HR-147) gets its own batch after 2.0.0 (batch 34). D8: batch
    30 (PR #49) is resolved before batch 33 starts; the owner stores
    the token in the Actions store first. D9: the capture
    timestamps use `jiff`. D10: the pending standing-rule rulings
    (HR-042, HR-172, HR-153's clauses, the loop proposals) are taken
    at the spec review. D11: no GitLab CI gate; on a host other than
    GitHub, `init` prints a note, and the migrating-knowledge skill
    says what to set up and how. HR-177 is dropped. Q1: HR-088
    and HR-053 stay out of 2.0.0; each ships after it as a minor.
    Source: owner, 2026-10-10, the batch-33 preparation.
97. **Batch 33 re-scoped to the rule-to-change verification
    contract - ruled 2026-10-15.** The owner sharpens the scope of
    houserules: per change, houserules says which rules apply,
    which are proven, which are only judged, what is unverified,
    and who must review; other concerns connect through adapters
    (discussion 59, issues 58 and 60 to 73). This scope replaces
    the batch-33 spec and plan of 2026-10-10; both stay in the tree
    as superseded drafts. The 2.0.0 cut, the place of each item
    that batch 33 lists, and which rulings of 96 carry over are
    ruled one at a time; until then, each ruling of 96 stands.
    Source: owner, 2026-10-15.
98. **2.0.0 holds all of discussion 59 and resolves every open
    issue - ruled 2026-10-15.** Release 2.0.0 (batch 33) holds every
    issue of discussion 59, the five cuts and the additions alike:
    issues 58 and 60 to 73 (HR-179 to HR-193). It also resolves
    every other open GitHub issue: 35 (HR-141), 38 (HR-144), 41
    (HR-147), and 50 (HR-178). This supersedes D5's placement of
    HR-144 in batch 29 and D7's batch 34 for HR-147; batch 34 is
    removed, and batch 29 holds no item. Source: owner, 2026-10-15.
99. **2.0.0 ships as several batches on main under one meta-spec -
    ruled 2026-10-15.** The 2.0.0 work runs as several batches; the
    owner names three batch specs. Each batch has its own spec,
    plan, review, and ff-only merge to main. The release PR that
    release-please opens stays unmerged until the last batch is on
    main; no 1.x release ships from main in between. One meta-spec
    covers the whole release: every item and issue of 98 and the
    upgrade path. Each batch spec is checked against the meta-spec,
    so that every item and issue is covered by exactly one batch.
    Source: owner, 2026-10-15. Amended by 100 and 101: "three" was
    not an order.
100. **Nothing is lost in 2.0.0 - ruled 2026-10-15.** Every function
    houserules has at 1.4.0 remains at 2.0.0, unless the owner rules
    otherwise for that function. An issue's proposal that would drop
    or weaken a function is not such a ruling: the meta-spec lists
    the function, and the owner rules it. The meta-spec carries the
    inventory of current functions and the fate of each. Tracking
    unresolved minor issues (technical debt) is one of them: the
    owner named it crucial to houserules' usability. Source: owner,
    2026-10-15.
101. **2.0.0 ships in five batches - set 2026-10-15.** The owner
    left the number of batches to the controller ("three batches was
    not an order"). The controller set five, one theme and one file
    set each, in dependency order: the knowledge layer; the workflow
    and work items; audit and its checks; evidence and report claims;
    the per-change review layer. The rows of each batch are the
    owner's to rule (meta-spec M1). Source: controller, by the
    owner's delegation, 2026-10-15.
102. **The rows of the five 2.0.0 batches - ruled 2026-10-15.** The
    owner approved the split of meta-spec sections 3 and 4 (M1).
    Batch 33: HR-182, HR-184, HR-191, HR-147, HR-178, HR-131, HR-162,
    HR-151. Batch 34: HR-180, HR-181, HR-144, HR-129, HR-153, HR-165,
    HR-166, HR-150. Batch 35: HR-183, HR-173, HR-163, HR-175, HR-139,
    HR-176, HR-138, HR-152, HR-164, HR-188. Batch 36: HR-171, HR-057,
    HR-158, HR-174, HR-148, HR-072, HR-141. Batch 37: HR-193, HR-179,
    HR-185, HR-190, HR-186, HR-192, HR-189, HR-187. Source: owner,
    2026-10-15.
103. **Deferrals and technical debt under the work-item contract -
    ruled 2026-10-15.** The work-item contract of HR-180 (issue 60)
    has a "file a deferral" operation that each tracker adapter
    implements: a title, the reason, the origin (the finding or the
    review file), and a deferral marker (a label or a type). The gates
    check that each deferral a review or a report names resolves to a
    filed item with its reason. Two parts close the gaps of a tracker
    whose data lives outside the git tree (Beads keeps it in Dolt
    under `refs/dolt/data`, apart from source branches): (1) the change
    names each deferral in a commit trailer (for example `Deferred:
    <id> <reason>`), so the PR shows it, it travels with the branch,
    and the gate checks that each named id resolves with its reason;
    (2) when the gate cannot reach the tracker's data, it fails with
    one line that names the adapter and the setup step, never a pass.
    The migrating-knowledge skill describes the setup for each
    adapter: for Beads, installing `bd`, the custom types and
    statuses, and fetching the data in CI. Meta-spec M2. Source:
    owner, 2026-10-15.
104. **The backlog is a port with adapters - ruled 2026-10-15.** The
    work-item contract of HR-180 follows a hexagonal design. The core
    defines the contract: every backlog function the CLI has at 1.4.0
    (`list` and its filters, `batch`, `set`, `get` of item, amendment,
    and parked ids, `check-backlog` and its checks, the backlog half of
    `archive`, the records for items, batches, amendments, decisions,
    and parked polish) plus the deferral operation of 103. The JSON
    backlog is not a core unit: it is one adapter that fulfills the
    contract, like the Beads and GitHub Issues adapters. Whatever the
    backend, every command behaves as it behaves on the JSON backlog
    today. This is point 3 of issue 60 without loss of any capability;
    point 5 (removing the commands) does not apply. The knowledge half
    of `archive` stays in the core. Meta-spec M3. Source: owner,
    2026-10-15.
105. **Packs are plugins - ruled 2026-10-15.** Packs (HR-182, issue 62)
    are a plugin system, like linter plugins: a project adds or removes
    a pack, and after installation it excludes or ignores individual
    entries of a pack. A project's own rules have the same shape: a
    pack that is not published but written and maintained locally,
    beside the packs it adds. Issue 62's point 4 (packs are seed-once,
    `update` never overwrites them) does not apply: a kit fix reaches a
    project the way a plugin upgrade does, and the project's
    exclusions and its local pack stay its own. Upstream packs from
    other repositories (HR-191, issue 71) are published packs of the
    same system. Meta-spec M4. Source: owner, 2026-10-15.
106. **A pack entry is configured by override - ruled 2026-10-15.**
    An override may change any field of a pack entry except its id:
    level, standing, areas, summary, body, check, and tags. Excluding
    the entry is the way to drop it. `check-knowledge` lists every
    override and exclusion (issue 71 point 2). This keeps the 1.4.0
    ability to edit a seeded entry in place, and it carries what the
    `stats` loop writes to `overrides` today (retire, demote, narrow,
    `ruled-keep`). Meta-spec M4b. Source: owner, 2026-10-15.
107. **Checks run through a runner port - ruled 2026-10-15.** The
    check runner contract of HR-183 (issue 63: a command, its
    arguments, and an output parser for exit code, SARIF, or JSON) is
    a port. The six built-in check types (`grep-absent`, `commits`,
    `co-change`, `diff-append-only`, `report-field`, `durable-sha`)
    are runners that fulfill it and run inside the binary with no
    other tool installed. External tools (Semgrep, ast-grep, a SARIF
    linter, commitlint) are runners a project or a pack chooses. The
    kit's own packs keep the built-in runners, so `check-commit` and
    the commit-msg hook behave as at 1.4.0. Issue 63's point 3 does
    not apply, and its point 4 is replaced: a built-in stays where a
    tool exists. A missing external tool is a setup error that names
    the tool and its install step, never a rule failure (point 5).
    Meta-spec M5. Source: owner, 2026-10-15.
108. **Render targets are a port - ruled 2026-10-15.** `render` (HR-184,
    issue 64) writes through a port with one renderer per target. The
    AGENTS.md renderer is always on: the standing rules and one
    section per area that names the area's globs; an area whose globs
    share one directory prefix also gets that directory's AGENTS.md.
    The Claude renderer keeps the 1.4.0 output exactly
    (`.claude/rules/*.md` with glob loading through `paths:`, and the
    `project-knowledge` skill); `init --target claude` enables it,
    `update` keeps it where an install has it, and `init` prints one
    line that tells a Claude Code user how to enable it. `render
    --check` and the generated-file budget cover every enabled
    renderer. A renderer for another harness needs no core change.
    Meta-spec M6. Source: owner, 2026-10-15.
109. **Packs are self-contained - ruled 2026-10-15.** A pack declares
    no dependency on another pack: pack hierarchies bring circular
    dependencies and bloat chains. A pack that needs what another pack
    holds packages it itself. The proposal of meta-spec M7 (a pack
    declares the packs it needs) is rejected. Source: owner,
    2026-10-15.
110. **Workflows are not packs; an id has one source - ruled
    2026-10-15.** Two installed packs never ship the same id: duplicate
    ids become a maintenance problem when two packs bundle different
    versions of the same content. The proposal of meta-spec M7b
    (identical duplicates allowed) is rejected. A workflow (the
    reference workflow of HR-181) gets its own mechanism, distinct
    from packs: it can name the packs it references without
    dependency chains, maintenance burden, or supply-chain risk.
    Source: owner, 2026-10-15.
111. **A workflow requires rule ids; only the root installs packs -
    ruled 2026-10-15.** A workflow (HR-181) is its own package kind:
    agents, skills, evals, and the report contract, with no rules. Its
    manifest lists the rule ids it requires (as an OSGi bundle requires
    capabilities); the reference workflow requires the 20 ids it cites.
    Only the project root installs packs: `.houserules.json` lists each
    pack once with an exact version and a checksum (as Terraform's lock
    file pins one provider version with hashes). Packs require nothing
    and nothing requires a workflow, so the graph stays one level deep.
    A requirement never installs anything: `check-knowledge` checks
    that each required id comes from exactly one installed pack (the
    local pack counts) and is not excluded, and an unmet requirement
    prints one line naming the workflow, the id, and a pack that
    provides it. `init --workflow reference` prints the packs that meet
    the requirements and adds them to the root manifest only on an
    explicit flag; from then on the root owns them. A project may meet
    a requirement from its local pack. Evidence: Terraform, OSGi, Dev
    Containers, npm, ESLint, pre-commit docs (batch-33 ledger,
    V-plugin-patterns). Meta-spec M7c. Source: owner, 2026-10-15.
112. **No waiver of a workflow requirement - ruled 2026-10-15.** A
    waiver would duplicate the exclusion structure and make the system
    inconsistent. A project that excludes or deleted a rule that an
    installed workflow requires provides it itself: it replaces the
    exclusion with an override of its own (106). Until it does,
    `check-knowledge` fails with the unmet-requirement line of 111.
    For a 1.x install, `update` cannot write that content: it lists
    each such id as a hand step in its summary. This supersedes the
    1.4.0 behavior where an `overrides` entry silenced the citation of
    a deleted seeded entry. Meta-spec M8. Source: owner, 2026-10-15.
113. **The 1.x install converts in `update` - ruled 2026-10-15.**
    `update` from 1.x converts an install: the agent templates, skills,
    evals, and report schema become the reference-workflow package with
    its required ids; unchanged seeded entries become pack entries,
    pinned with version and checksum in `.houserules.json`; changed
    seeded entries become overrides (106); deleted or
    `overrides`-listed seeded entries become exclusions (with the hand
    step of 112 where a workflow requires the id); the project's own
    entries move into its local pack; `backlog/` runs under the JSON
    adapter; `.claude/rules/` and the `project-knowledge` skill run
    under the Claude renderer; hooks, the CI workflow, AGENTS.md, and
    CLAUDE.md stay. `update` prints one summary, and a second `update`
    changes nothing. A new `init` gets the minimal default. Meta-spec
    M8. Source: owner, 2026-10-15.
114. **The commit-msg hook belongs to the reference workflow;
    migrating-knowledge is core - ruled 2026-10-15.** Some adopters do
    not want to enforce what the commit-msg hook mandates, so
    `.githooks/commit-msg` ships in the reference-workflow package,
    not in the core. `check-commit` stays a core command that any
    project or CI can run. In the 1.x conversion (113) the hook joins
    the reference-workflow package. The migrating-knowledge skill is
    core: every install gets it. Source: owner, 2026-10-15.
115. **`render` owns a marked block of AGENTS.md - ruled 2026-10-15.**
    In each AGENTS.md the AGENTS.md renderer (108) writes to, it owns
    only the block between `<!-- houserules:begin -->` and
    `<!-- houserules:end -->` and never touches text outside it. A
    file with no markers (at `init` or in the 1.x conversion) gets the
    block appended at its end; once the markers exist, the block stays
    where the project put it. A missing end marker or two blocks in one
    file make `render` and `render --check` print one line that names
    the file, never a guess. `render --check` and the generated-file
    budget read only the block. The Claude renderer keeps writing whole
    generated files under `.claude/rules/`. Meta-spec M9. Source:
    owner, 2026-10-15.
116. **Commands never call a host API - ruled 2026-10-15.** The 2.0.0
    commands (`evidence`, the review-tier decision, `record write`, and
    the rest) produce files, Markdown, JSON, and exit codes; a change
    record goes to a git ref, which is git, not a host API. Only the
    seeded GitHub workflow talks to GitHub: it posts the PR comment,
    sets the commit status and label, and pushes the record ref. D11
    stands for other hosts: `init` prints its note, and the
    migrating-knowledge skill gets one section per batch-37 function
    that says which command output to wire up and how. A later host
    integration needs no core change. Meta-spec M10. Source: owner,
    2026-10-15.
117. **Published documents carry their own schema version - ruled
    2026-10-15.** Every document houserules publishes (the rule package
    of `for`, the `audit` result, the evidence packet, the change
    record; HR-187, issue 67) carries its schema id and version, for
    example `"schema": "houserules/audit-result@1.2.0"`, and each schema
    is published at a stable URL. Each schema has its own semver under
    D2: an added optional field is a minor change; a removal or a
    tightening is a major change. The CLI's major version rises only
    when a schema it emits by default takes a major change. The MCP
    server serves the same documents and versions.
    `houserules.1-0-surface-is-frozen` is rewritten to name the
    published schemas and these rules; the wording of that standing
    entry is ruled at the batch-37 spec review. This replaces the one
    version line of the meta-spec draft. Meta-spec M11. Source: owner,
    2026-10-15.
118. **Issue 35's line check is exact, not heuristic - ruled
    2026-10-15.** Part 2 of issue 35 (HR-141) ships in batch 36 as an
    exact check: a citation that a claim rests on uses a recorded form
    that a tool fills in when the report is written (the path, the line
    or range, and the text or hash of that line at that moment), and
    `check-report-claims` compares the recorded text with HEAD, so a
    mismatch is drift by construction. A citation in free prose keeps
    the 1.4.0 checks (the file resolves; the line number fits the
    file). The implementer template asks for the recorded form. This
    meets the zero-false-positive bar of the batch-28 spec
    (`docs/specs/2026-10-01-batch-28-issue-batch.md`, section HR-141)
    without a heuristic; the unwired batch-28 module is not wired.
    Issue 35 closes with part 1 (shipped in batch 28) and this part 2.
    Meta-spec M12. Source: owner, 2026-10-15; citation corrected by
    the owner on 2026-10-16 (meta-spec review F13).
119. **Reviewer models are workflow configuration - ruled 2026-10-15.**
    The model of each reference-workflow role (implementer, task
    reviewer, branch reviewer, and the spec reviewer of HR-144) is a
    value in the workflow's configuration, not text in a template.
    The defaults are the 1.4.0 models; model names depend on the
    harness, so a project on another harness sets its own.
    `process.model-policy` keeps its text and stays the default. The
    workflow recommends no particular model family for any role:
    issue 61's recommendation of a different family for the branch
    review is not adopted. Meta-spec M13. Source: owner, 2026-10-15.
120. **Every report and review names its model - ruled 2026-10-15.**
    The 2.0.0 deliverables schema makes `model` a required field of
    every report and review. Its value is the model id, `human` (a
    person wrote it), or `unknown` (the harness does not expose the
    model): a designed token, never an absent or guessed value
    (`quality.absence-is-designed`). The reference workflow fills it
    from its configuration (119): each dispatch passes the model it
    chose, and the agent copies it. `stats` groups its cost and finding
    figures by model; reports from before 2.0 have no field and form
    their own group. `stats` reads reports leniently, so mixed
    workspaces still aggregate. Delivered with HR-181 in batch 34.
    Meta-spec M13b. Source: owner, 2026-10-15.
121. **HR-173 is the exit-code runner; doc-comments splits - ruled
    2026-10-15.** HR-173 ships in batch 35 as the exit-code runner of
    the runner port (HR-183, 107), with migrating-knowledge recipes for
    rustdoc `missing_docs`, ruff `D`, and eslint `jsdoc`; it is no
    separate check type. `writing-style.doc-comments` holds six
    clauses, and a linter proves only the first (every exported symbol
    has a doc comment in the language's form). The batch-35 spec
    splits the entry: `writing-style.doc-comments` keeps its id as the
    presence rule, backed by the linter and proven; a new entry holds
    the content and naming clauses, stays judged, and links back with
    `see` (`knowledge-base.ids-are-permanent`). HR-173's acceptance
    becomes: the presence rule leaves the judged set; the content rule
    stays in it. In the 1.x conversion an install's change to the old
    entry becomes an override on the presence entry, and the batch-35
    spec names the hand step for the new entry. Meta-spec M14. Source:
    owner, 2026-10-15.
122. **The 2026-10-10 rulings under the new scope - confirmed
    2026-10-15.** The owner confirmed the meta-spec's carry-over table
    of 96: D1 is superseded by 98 (its same-files rule stays a batching
    rule); D2 stands (117 applies it per published schema); D3 stands
    for the schema files and the seeded CI workflow, with the eval
    scenarios in the reference-workflow package (111) and pack entries
    upgraded as plugins (105); D4 stands; D5 stands (batch 34) and its
    batch-29 part is superseded by 98; D6 stands (batch 33, in the
    `knowledge-base` pack); D7 is superseded by 98; D8 is done; D9
    stands (batch 36); D10 stands, each ruling taken at the spec review
    of the batch that holds its row; D11 stands, extended by 116; Q1
    stands. Source: owner, 2026-10-15.
123. **The 1.x conversion of `overrides` - ruled 2026-10-15, amends
    113.** In 1.4.0 `overrides` declares that the adopter owns an id or
    a path (`baseline.rs`, `Status::Overridden`); it is not a deletion.
    The conversion maps each entry once: (1) a listed seeded entry that
    is present becomes an override of the pack entry carrying the
    project's own text (106); where it equals the kit's text it becomes
    the pack entry; (2) a listed seeded entry that is absent becomes an
    exclusion, with the hand step of 112 where a workflow requires the
    id; (3) a listed file path stays project-owned: `update` never
    rewrites it, in 2.0 as in 1.4.0, a file that moves into the
    reference-workflow package (the commit-msg hook) included. The
    clause of 113 that maps "`overrides`-listed seeded entries" to
    exclusions is replaced by this item. Meta-spec review F1. Source:
    owner, 2026-10-15.
124. **The report format is core - ruled 2026-10-16, amends 111 and
    113.** The deliverables schema, the report and review kinds (the
    `spec-review` kind of HR-144 and the required `model` field of 120
    included), and every command and check that reads them are core:
    `validate`, `check-report-claims`, `stats`, `audit --report` and
    `--workspace`, and the `report-field` check type that four seeded
    entries use (`process.knowledge-first`, `process.deliverables-json`,
    `security-hygiene.dependency-vetting`,
    `security-hygiene.verify-current-docs`). A workflow fills the report
    format and does not own it: its templates write reports and its
    evals test them. In 111, "the report contract" reads "templates
    that fill the core report format"; in 113, the report schema stays
    core and `update` keeps delivering it (D3, 122). The Claude
    renderer's `project-knowledge` skill keeps its 1.4.0 text, because
    everything it names stays core. This matches issue 61, point 1,
    which puts "write the deliverable report" into the
    workflow-agnostic step. Meta-spec review F2. Source: owner,
    2026-10-16.
125. **The reference workflow requires 25 ids - ruled 2026-10-16,
    amends 111.** The reference workflow's manifest lists the 25
    seeded ids that its files cite: the three agent templates, the
    orchestrating and finishing-a-feature skills, the evals, and the
    commit-msg hook (enumeration: the batch-33 workspace script
    `f3-required-ids.py`). In 111, "the 20 ids it cites" reads "the 25
    ids its files cite". A test asserts that the manifest lists exactly
    the ids the package's files cite. The workflow does not require
    `process.conventional-commits`: the hook runs whatever `commits`
    entries the project installs (114, 107). When no `commits` entry
    is installed, `check-commit` prints that it checked nothing and
    exits 0, so a check that ran nothing does not read as a pass.
    Delivered with HR-181. Meta-spec review F3. Source: owner,
    2026-10-16.
126. **Installed packs live in the tree - ruled 2026-10-16.** `pack
    add` and `update` copy each installed pack's entries into the
    repository in the form `emit` writes (HR-162) and record its
    version and checksum in `.houserules.json`; every command reads the
    copies on disk, as in 1.4.0. The packs the binary carries are only
    the source that `pack add` and `update` copy from. A copied pack
    whose content no longer matches its checksum fails
    `check-knowledge` with a line that names the pack and points to an
    override (106). A pin that differs from the binary's own pack
    version changes nothing by itself; a copied pack whose schema
    version is newer than the binary reads (117) fails each command
    with a line that names the houserules version it needs. `update`
    moves the pins of the kit's packs to the versions the binary
    carries, rewrites their copies, keeps overrides and exclusions, and
    prints one line per moved pin; a pack the binary does not carry
    moves only when the project runs `pack add` again with a new
    version. HR-193 reads the merge-base rules (`knowledge/`, the
    copied packs, `.houserules.json`) from git at the merge base and
    checks their checksums, with no network and no old binary. The
    batch-33 spec names the directory layout. Invariant: a pin names
    content that every gate can reproduce at any commit, with or
    without a network. Meta-spec review F6. Source: owner, 2026-10-16.
127. **The spec and plan steps pick up technical-debt candidates -
    ruled 2026-10-16, amends 102.** Owner, verbatim: "In the reference
    workflow, I want the spec/plan steps - when a new feature or a
    bigger rework is worked on - to check the tech debt items in the
    backlog and automatically include convenient candidates for tech
    debt reduction." The technical-debt items are the open items that
    carry the deferral marker of 103. The reference workflow's spec and
    plan steps for a new feature or a bigger rework query them through
    the backlog contract (104), so the step works on every adapter, and
    fold the convenient candidates into the batch. Filed as HR-194 in
    batch 34 with HR-181, the reference workflow it changes. What makes
    a candidate convenient is a separate owner ruling. Source: owner,
    2026-10-16.
128. **A convenient technical-debt candidate - ruled 2026-10-16,
    amends 103 and 127.** The step of 127 runs at the spec and plan
    steps of a standard-tier or full-tier batch (`process.risk-tiers`);
    a light change has no plan. A candidate is convenient when it is an
    open item with the deferral marker whose files overlap the files
    the batch plans to change, and folding it in does not raise the
    batch's tier. The deferral operation of 103 records the files its
    origin names (a review finding carries a `file` field), so a
    deferral has a title, the reason, the origin, the files, and the
    deferral marker. The spec lists each candidate it folds in with the
    overlapping files, and each overlapping item it leaves out with the
    reason (the tier would rise). The owner's approval of the spec
    stays the gate. Source: owner, 2026-10-16.
129. **Each rule reaches a harness from one rendered source - ruled
    2026-10-16, amends 108.** For the standing rules that source is the
    AGENTS.md renderer's block; Claude Code reads it through the
    CLAUDE.md bridge (`@AGENTS.md`). The Claude renderer no longer
    writes `.claude/rules/standing-rules.md`, and the 1.x conversion
    removes it; the Claude renderer keeps the glob-loaded area files
    and the `project-knowledge` skill. In 108, "keeps the 1.4.0 output
    exactly" reads "keeps the 1.4.0 output except
    `standing-rules.md`, with the same rule text reaching Claude Code";
    issue 64's acceptance line "a Claude Code project with `--target
    claude` gets the same files as today" reads the same way. Area
    rules follow the same invariant: the batch-33 spec checks current
    Claude Code docs for which AGENTS.md files Claude Code loads and
    puts each area's rules in exactly one source per harness. The
    generated-file budget counts the standing block once. Meta-spec
    review F8. Source: owner, 2026-10-16.
130. **The override fields are the schema's fields - ruled 2026-10-16,
    amends 106.** An override may change every entry field except
    `id`: `kind`, `area`, `standing`, `status`, `summary`, `body`,
    `tags`, `source`, `see`, `verify`, and `check` (the
    `template/knowledge/schema.json` `$defs.entry` properties). An
    override replaces a whole field: to change only a check's `level`,
    it restates that entry's whole `check`. The override carries what
    the batch-close proposals do to a pack entry today: `retire` sets
    `status`, `demote` drops `standing` and sets `area`, `narrow`
    rewrites the text, `ruled-keep` adds a tag. The controller applies
    them as overrides, never by editing the pack copy (126). In 106,
    the field list "level, standing, areas, summary, body, check, and
    tags" and the clause "what the `stats` loop writes to `overrides`
    today" are replaced by this item: `stats` writes no file, and in
    1.4.0 the controller edits the entry and lists a kit-shipped one in
    `overrides`. Invariant: a ruling's field names are the schema's
    field names. Meta-spec review F9. Source: owner, 2026-10-16.
131. **The session-start hook splits - ruled 2026-10-16, amends 113.**
    The 1.4.0 session-start hook (`tools/claude-session-start.sh`,
    wired in `.claude/settings.json`) does two jobs. Its re-injection
    of the standing rules after a compaction (`houserules standing`)
    is core and ships with the Claude renderer, so every Claude Code
    install keeps it. Its ritual line (git status, the batch, the
    ledger, the `orchestrating` skill) moves to the reference-workflow
    package, which adds its own session-start entry. In 113, "hooks
    ... stay" reads: the compaction re-injection stays with the Claude
    renderer; the ritual line and the commit-msg hook (114) move to
    the reference-workflow package; a hook path listed in `overrides`
    stays project-owned (123). The re-injection is a deliberate repeat
    at the recent end of the context, not a rendered source, so 129
    does not count it. Invariant: the spec restates each ruling as
    ruled, and a conflict between rulings goes to the owner. Meta-spec
    review F10. Source: owner, 2026-10-16.
132. **Four consequences of the backlog port - ruled 2026-10-16,
    extends 104.** (1) One conformance suite of the backlog contract
    runs against every adapter, so equivalence with the JSON backlog is
    a test result, not a claim. (2) Where a backend lacks a native
    field or state (for example `partial`, `dropped`, a batch, an
    amendment, a parked group), the adapter maps it to that backend's
    own means (labels, custom types and statuses, milestones, epics),
    and the suite proves the mapping. (3) The deliverables schema
    accepts the adapter's id shape. (4) This repository keeps the JSON
    adapter through the 2.0.0 release, so every HR id cited in the
    specs and design.md keeps resolving. The controller derived these
    from 104 and the meta-spec first printed them inside the ruled M3
    paragraph; the owner now rules them. Meta-spec review F14. Source:
    owner, 2026-10-16.
133. **The install layout carries its own schema version - ruled
    2026-10-16, amends D4 (96, 122) and extends 117.** From 2.0.0,
    `.houserules.json` carries `"schema": "houserules/install@<semver>"`
    in the convention of 117: the install layout's own version, apart
    from the CLI version. `init` writes it, the 1.x conversion writes
    it as its last step, and it stays in every install. `update` runs a
    conversion only when the tree's install schema has a lower major
    than the binary writes; a second `update` then changes nothing. An
    added optional key is a minor change and needs no conversion (D2).
    D4 reads: a binary that writes install schema major 2, on a tree
    whose install schema is missing or has a lower major, prints one
    named line and exits 2 from every gate. A 1.x tree has no `schema`
    key. The `version` stamp keeps its job: the seeded CI installs that
    binary. Release automation sets only the CLI version, so every
    batch tests the conversion and D4 with the tree binary. Meta-spec
    O1, review F7. Source: owner, 2026-10-16.
134. **The area model under packs - ruled 2026-10-16, extends 105, 108,
    and 126.** (1) `knowledge/areas.json` is the one list of areas, and
    the project owns it. The `area` enum leaves the knowledge schema,
    and `check-knowledge` checks every active entry's area against the
    keys of `areas.json`. (2) A pack declares the areas its entries use,
    each with default globs. When `pack add`, or an `update` that brings
    a new pack version, introduces a missing area, houserules adds it
    with the pack's globs and prints one line for it. An area that
    already exists keeps the project's globs. `.houserules.json` records
    each area that a pack added. (3) After `pack remove`, or after an
    `update` in which a pack drops an area, houserules removes an area
    only when a pack added it, no remaining pack declares it, and no
    active entry of any source uses it. It prints each removed area
    with its globs, so a project that edited them can restore them, and
    `render` removes that area's directory AGENTS.md. An area the
    project defined itself is never removed. (4) Archived entries do
    not keep an area alive: an archived entry keeps its area as plain
    text, and `check-knowledge` no longer checks it against
    `areas.json`. (5) The dead-glob gate stays strict: when `pack add`
    or `update` writes a glob that matches no tracked file, the command
    names that glob in its output, and the project fixes it before it
    commits. (6) A key in `areas.json` turns off an area's directory
    AGENTS.md (108). This repository sets it for `template`, so that
    this repository's rules never reach the kit's shipped seed
    `template/AGENTS.md`; `cli` and `tests` get their own directory
    files. Points (3) and the add-on-update half of (2) are the owner's
    own requirement. Meta-spec O2, review F5a. Source: owner,
    2026-10-16.
135. **`.claude/` and the git hooks of a subdirectory install - ruled
    2026-10-16, extends 113 and 114.** (1) By default every command
    uses the nearest directory at or above the current directory that
    holds `.houserules.json`, searching no higher than the git top
    level; when none exists, the git top level, as in 1.x. `init --dir
    <subdir>` installs there. (2) `.claude/` lives in the install root,
    and the install docs say to launch Claude Code there: Claude Code
    reads `.claude/settings.json` (and so the session-start hook) only
    from the launch directory, and loads `.claude/agents/` only from
    the launch directory and its parents (code.claude.com docs, fetched
    2026-10-16). `${CLAUDE_PROJECT_DIR}` then names the install root.
    (3) `.githooks/` lives in the install root (in the reference-workflow
    package, 114). houserules never sets `core.hooksPath`. Git runs a
    commit-msg hook from the top level of the working tree, so the hook
    passes `--dir` with the install root, read from its own file path.
    The docs give two activations: `git config core.hooksPath
    <subdir>/.githooks` when the repository has no other hooks, or a
    one-line call from the existing hook manager. (4) In a subdirectory
    install, `check-commit` checks only the commits that touch the
    install root: the hook reads the staged files, and `--from` reads
    each commit's changed paths. It prints one line that counts the
    skipped commits, so a skip never looks like a pass. An install at
    the git top level checks every commit, as in 1.x. The seeded CI
    workflow stays at the git top level (meta-spec 5.18). Meta-spec O3,
    review F11. Source: owner, 2026-10-16.
136. **A tracker adapter reaches its own backend - ruled 2026-10-16,
    resolves a conflict between 104 and 116.** 116 governs the
    change-review functions it names (`evidence`, the review-tier
    decision, `record write`, and the other batch-37 outputs); they stay
    host-free. A tracker adapter of 104 reaches its own backend, because
    that is its job: only the GitHub Issues adapter, and any later
    hosted-tracker adapter, talks to the host; the core and every other
    command stay host-free. When an adapter cannot reach its tracker,
    the gate fails with one line that names the adapter and the setup
    step (103). How the adapter authenticates is for the batch-34 spec;
    houserules never takes or stores a token value itself. Meta-spec
    O4a. Source: owner, 2026-10-16.
137. **houserules stays correct over a lossy transport - ruled
    2026-10-16.** A context proxy (measured: headroom-ai 0.41.0) can
    shorten or drop houserules output on its way to the model with no
    visible sign. houserules never depends on a proxy's settings or
    behavior, and the fix changes or configures no proxy (owner
    constraint). Principle: no gate depends on what the model saw,
    damage is caught where it would be written, and rules reach agents
    through files. (1) HR-195, batch 33: `check-knowledge`,
    `check-backlog`, and `validate` reject a compression marker in a
    knowledge file, a backlog file, or a deliverable, naming the file
    and field; the patterns are a list in houserules, starting with
    headroom's markers taken from its source at spec time; a gotcha
    entry in the default `knowledge-base` pack states what was measured
    and that the source files and gates are the truth, and recommends
    no proxy setting. (2) HR-196, batch 34: the reference workflow's
    templates and skills tell an agent to save houserules output to a
    file and read it with its file-read tool, and to treat a marker or
    a "more items" note as a lost read and read the source file; the
    migrating-knowledge skill says the same. (3) HR-197, batch 36: a
    capture is written to a file by the command that produced it and
    the report cites the file; `check-report-claims` requires every
    quoted excerpt to be a byte-exact substring of its cited capture.
    Evidence: `.superpowers/sdd/2026-10-10-batch-33/headroom-2026-10-16/`.
    Meta-spec O7. Source: owner, 2026-10-16.
138. **The default tracker and issue 60's other points - ruled
    2026-10-16, extends 104 and 136.** (1) No default tracker: `init`
    installs none unless `--tracker json|github|beads` is given; on an
    install with no tracker the backlog commands exit with one line
    that names `--tracker`; `init --workflow reference` without
    `--tracker` refuses with one line that names the three choices,
    because the reference workflow needs a tracker. The 1.x conversion
    keeps an existing `backlog/` as the JSON adapter, and this
    repository keeps JSON (132). (2) Point 1: the trace is a commit
    trailer, `Work-Item: <id>`; houserules does not read the PR body,
    which lives on the host, not in the tracker (116, 136); the seeded
    GitHub workflow may check it host-side. (3) Point 4: core `audit`
    resolves every `Work-Item:` trailer it finds and fails an id that
    does not resolve, naming the id and the adapter. Requiring an id on
    every commit is a `commits`-type entry a project chooses; it ships
    in the process pack with an exemption list of bot authors
    (dependabot and release-please pass), and this repository installs
    it from batch 34. (4) Point 6: each adapter declares its
    session-start and session-end commands, and `render` writes them
    into the AGENTS.md block. (5) Point 7: the migrating-knowledge
    skill gets one section per adapter for moving `backlog/` data with
    the mapping of 132; a guide, not a command, because the contract
    has no create operation besides the deferral of 103. Meta-spec O4.
    Source: owner, 2026-10-16.
139. **`get` and `check-backlog` under a non-file adapter - ruled
    2026-10-16, extends 104 and 132.** (1) `get` sends an id with the
    knowledge shape (`^[a-z0-9-]+\.[a-z0-9-]+$`) whose first part is a
    loaded topic to the knowledge base; every other id goes to the
    installed tracker adapter, which resolves its own shapes (JSON:
    `<idPrefix>-nnn`, `A-nn`, `PP-n-nn`; GitHub Issues: an issue number
    such as `#123`; Beads: its hash ids, a child id such as `bd-a1b2.1`
    included, because `bd-a1b2` is no topic). An id that resolves in
    both fails with one line that names both. (2) `A-nn` and `PP-n-nn`
    are the JSON adapter's id shapes, no longer kit-fixed; this
    repository keeps them (132). Under another adapter, amendments and
    parked items are records in the backend with its own ids, mapped as
    132 rules; the contract carries a record's kind, not its id shape.
    (3) `check-backlog` splits. Contract checks run through the
    contract on every adapter and the conformance suite proves each:
    duplicate ids, `see` references that resolve, batch members that
    exist, duplicate batches, at most one batch in progress, the "done
    without a batch" warning, the amendment and parked references, and
    the deferral check of 103. Adapter checks are each adapter's own:
    JSON keeps the file schema, the emitter round trip, "section equals
    the file name", and the archive mirror; GitHub Issues and Beads
    check that the labels, types, and statuses the 132 mapping needs
    exist, and fail with the named setup step. Meta-spec O5. Source:
    owner, 2026-10-16.
140. **Workflow-package upgrades - ruled 2026-10-16, extends 111 and
    126.** (1) A workflow package has a name and a version;
    `.houserules.json` pins both and one SHA-256 per file, the baseline
    mechanism of 1.4.0. The reference workflow ships inside the binary,
    versioned with it, so `update` moves its pin, as 126 rules for the
    kit packs. (2) `update` writes each workflow file that still
    matches its pin and moves that pin. (3) A file whose content
    differs from its pin is kept, as 1.4.0 keeps a modified kit-owned
    file: `update` writes nothing to it and prints one line naming the
    file and the new package version, so the adopter merges by hand. A
    path listed in `overrides` stays project-owned and is not reported
    (123). A pack copy, unlike a workflow file, fails when edited (126),
    because a pack entry has field overrides and a workflow file has
    none. (4) After an upgrade, `check-knowledge` checks the new
    version's required rule ids (111) and names a missing one.
    Meta-spec O6, review F5g. Source: owner, 2026-10-16.
141. **The `project-knowledge` skill - ruled 2026-10-16, amends 124
    and 129.** The Claude renderer's `project-knowledge` skill no longer
    keeps its 1.4.0 text. (1) Batch 33: its standing section goes, so
    the standing rules reach Claude Code from the AGENTS.md block alone
    (129); custom subagents load CLAUDE.md and AGENTS.md at startup
    unless they set `omitClaudeMd` (code.claude.com sub-agents docs,
    fetched 2026-10-16). The implementer-template line "The standing
    rules in your preloaded `project-knowledge` skill bind every
    change" is rewritten to point at AGENTS.md. (2) Batch 33: its
    retrieval protocol tells the agent to save houserules output to a
    file and read it with its file-read tool (137). (3) Batch 34: the
    dispatch steps (`Knowledge:`, `REPORT_FILE`, `audit --base
    <BASE>`, "as a dispatched subagent") move from the core skill into
    the reference-workflow package's agent templates; the core skill
    keeps the generic protocol (`for`, `get`) and the topic list, and
    its description becomes "Use before reading or changing any file in
    this repository". 124's premise that everything the skill names
    stays core, and 129's keeping of the skill unchanged, are amended
    to this. Meta-spec O8, re-review R1. Source: owner, 2026-10-16.
142. **An unmodified 1.x copy of the D3 files - ruled 2026-10-16,
    extends D3 (96, 122) and 140.** No 1.x install records a baseline
    for the three schemas, the seeded CI workflow, or the eval files.
    The binary carries every released content of those files (14
    across 0.3.0 and 1.0.0 to 1.4.0, counted from the git tags on
    2026-10-16). For each file the 1.x conversion applies the install's
    id-prefix rewrite to each released content and compares the result
    with the copy on disk: a match is an untouched file, which `update`
    replaces with the 2.0 file and baselines; any other content is an
    adopter change, which `update` keeps and lists as a hand step (D3).
    `.claude/evals/record.json` is the project's own data, a log the
    adopter appends to: it moves with the workflow package, and
    `update` never pins, replaces, or reports it. The batch-37 proof
    shows an unchanged 1.0.0 install whose schemas are replaced and a
    modified schema that is kept and listed. Meta-spec O9, re-review
    R2. Source: owner, 2026-10-16.
143. **An adopter-modified knowledge schema loses the `area` enum -
    ruled 2026-10-16, extends 134 and 142.** In 1.4.0 a project adds an
    area by editing `$defs.area.enum` in the seed-once
    `knowledge/schema.json`, and `check-knowledge` requires
    `knowledge/areas.json` to hold the same names, so each enum name is
    already an `areas.json` key. (1) The 1.x conversion ignores
    `$defs.area` when it compares an on-disk knowledge schema with the
    released contents (142). A schema that then matches changed only
    its areas: `update` replaces it with the 2.0 schema and baselines
    it, and no area is lost. This repository's schema takes this path:
    it differs from the template only by four enum names. (2) A schema
    with other adopter changes keeps them: the conversion removes only
    `$defs.area` and the `area` property's `$ref` to it, so the
    property takes the 2.0 schema's area definition, and prints the
    edit in its summary. The remaining differences are listed as a
    hand step (D3). (3) A schema path listed in `overrides` is not
    rewritten (123(3)). `update` lists the hand step "remove
    `$defs.area` from `knowledge/schema.json`; your areas live in
    `knowledge/areas.json`", and until the project does so,
    `check-knowledge` names the enum as the cause when a pack adds an
    area that the enum lacks. Meta-spec O10, re-review R3. Source:
    owner, 2026-10-16.
144. **The seeded CI's `check-backlog` step on an install with no
    tracker - ruled 2026-10-16, extends 138.** (1) On an install with
    no tracker, `check-backlog` exits 0 and prints one line: no tracker
    is installed, so it has nothing to check, and `--tracker` adds one.
    A lint gate with nothing to lint passes. The read and write
    commands (`list`, `batch`, `set`, and `get` of a backlog id) exit
    non-zero with the `--tracker` line, because they cannot do what was
    asked. The ruling as approved also named `archive` among them; that
    conflicts with the core knowledge half of `archive` and is
    meta-spec O11a. (2) `check-backlog` fails when no tracker is
    installed but a `backlog/` directory exists, with one line that
    names `--tracker json`, so the gate cannot pass over a lost tracker
    setting. (3) The seeded CI workflow keeps its unconditional
    `houserules check-backlog` step, so a tracker added later is
    checked with no edit to the seed-once file. Meta-spec O11,
    re-review R4. Source: owner, 2026-10-16.
145. **`archive` on an install with no tracker - ruled 2026-10-16,
    amends 144.** (1) On an install with no tracker and no `backlog/`
    directory, `archive` moves the superseded and retired knowledge
    entries, skips the backlog half, prints one line that says so and
    names `--tracker`, and exits 0. (2) On an install with no tracker
    and a `backlog/` directory, `archive` fails before it moves
    anything, with the line `check-backlog` prints there (144(2),
    naming `--tracker json`). (3) 144(1) is corrected: the commands
    that exit non-zero on an install with no tracker are `list`,
    `batch`, `set`, and `get` of a backlog id; `archive` is not among
    them. Meta-spec O11a. Source: owner, 2026-10-16.
146. **117 beside D2 - ruled 2026-10-16, clarifies 117 and 122.**
    (1) 117's "only" governs the published documents alone: the schema
    of each published document (the rule package of `for`, the `audit`
    result, the evidence packet, the change record) has its own semver,
    and its major change raises the CLI's major only when the CLI emits
    that schema version by default; a new major that the CLI emits only
    on request is a minor CLI change. (2) D2 governs the rest: removing,
    renaming, or tightening a subcommand or flag, or a schema the CLI
    reads from the project (the knowledge schema, the JSON adapter's
    backlog schema, the deliverables schema, and the install layout
    `houserules/install@`, 133) raises the CLI's major; an addition to
    any of them is minor. (3) The rewrite of
    `houserules.1-0-surface-is-frozen` at the batch-37 spec review names
    the three surfaces (the CLI, the read schemas, and the published
    documents); the owner rules its wording there (117). Meta-spec O12,
    re-review R6. Source: owner, 2026-10-16.
147. **Four open items outside the release - ruled 2026-10-16,
    places D10's rulings and amends Q1 (96, 122).** (1) HR-042 (the
    `security-hygiene.dependency-vetting` trigger on a `uses:` action
    reference) joins batch 35: it ships as a `security-hygiene` pack
    version through the runner port, and its standing-entry ruling is
    taken at the batch-35 spec review (D10). (2) HR-172 (narrowing
    `process.rulings-to-file`) joins batch 34; its standing-entry
    ruling is taken at the batch-34 spec review, with HR-153's standing
    clauses. (3) HR-053 is dropped. 2.0 delivers its in-scope parts
    through HR-180 (the rules without a backlog: no default tracker,
    138), HR-182 (optional packs, 105), and HR-181 (an optional
    workflow, 111). Its remaining part, a backlog-only install, is out
    of scope: owner, verbatim, "tracker without a knowledge base is no
    longer in scope. People can use beads for that. That is no longer
    houserules main focus." This replaces Q1's "ships after 2.0.0 as a
    minor" for HR-053; Q1 still covers HR-088. (4) HR-055 stays after
    2.0.0 as a minor, rescoped to curated starter packs (for example
    docs style and code quality) that the binary carries and a project
    adds with `pack add`; the default stays `knowledge-base` alone
    (issue 62), and an added pack is a minor change (D2). The release
    holds 47 items: batches 33 to 37 hold 9, 11, 11, 8, and 8.
    Meta-spec O13, re-review R14. Source: owner, 2026-10-16.
148. **Conversion steps run on every `update` - ruled 2026-10-16,
    amends 133.** (1) Each conversion step tests its own state on disk
    and in `.houserules.json` (for example: the commit-msg hook is not
    yet under the workflow package's pins), acts only while that state
    holds, and does nothing otherwise. `update` runs every step on
    every run, so a step that a later 2.x binary adds reaches an
    install already at install schema major 2, and a second `update`
    still changes nothing. This follows 1.4.0's reconciliation, which
    compares each item with the payload on every run and needs no
    separate migration step (`install.rs:89-97`). (2) The install
    schema major keeps its other jobs: the 1.x conversion writes major
    2 as its last step, and D4 fires on a missing or lower major. 133's
    "`update` runs a conversion only when the tree's install schema has
    a lower major" is amended: the major no longer decides whether the
    steps run. (3) Each batch from 34 to 37 runs `update` on this
    repository with the tree binary and commits the result, and its
    live run shows a second `update` printing no change. Meta-spec O14,
    re-review S2. Source: owner, 2026-10-16.
149. **Tracker-dependent gates on an install with no tracker - ruled
    2026-10-16, extends 103, 138, and 144.** (1) On an install with no
    tracker, `audit` does not resolve `Work-Item:` or `Deferred:`
    trailers: it prints one line that names how many it skipped and
    `--tracker`, and does not fail on them. No tracker is the default a
    project chose; 103(2)'s "never a pass" stays for an installed
    tracker that cannot be reached. As in 144(2), `audit` fails with
    one line naming `--tracker json` when no tracker is installed and a
    `backlog/` directory exists. (2) The process pack declares that it
    needs a tracker: `pack add process` on an install with no tracker
    refuses with one line that names `--tracker`, as `init --workflow
    reference` does (138(1)). The process pack keeps its deferral rules
    (`process.no-tech-debt`, `process.backlog-drives-work`,
    `process.bounded-review-loop`), reworded in batch 34 only to name
    the installed tracker. The 1.x conversion always has a tracker,
    because it keeps `backlog/` as the JSON adapter (138(1)). A project
    that wants some process rules without a tracker adds a tracker or
    copies those entries into its local pack. (3) The other packs
    presume no tracker: batch 34's rewrite rewords each entry outside
    the process pack that names a backlog item (for example
    `quality.principles`), and the rewrite set of meta-spec 6.4 is
    re-derived on this basis (re-review S5). Meta-spec O15, re-review
    S3. Source: owner, 2026-10-16.
150. **`get` of an id from a pack that is not installed - ruled
    2026-10-16, extends 139 and 144.** (1) Routing stays as 139(1)
    rules it: a knowledge-shaped id whose first part is a loaded topic
    goes to the knowledge base, and every other id goes to the tracker
    when one is installed. (2) An id that stays unresolved, has the
    knowledge shape, and whose first part names a pack the binary
    carries but the install lacks gets one line that names the pack and
    `houserules pack add <pack>`. With no tracker, this line replaces
    the `--tracker` line of 144(1); with a tracker, it follows the
    adapter's not-found line. `get` exits non-zero in both cases. (3)
    Only the names of the packs the binary carries trigger the line, so
    a Beads id such as `bd-a1b2.1` never gets it. Meta-spec O16,
    re-review S11. Source: owner, 2026-10-16.
151. **HR-193's `audit` when the merge base has a 1.x layout - ruled
    2026-10-16, extends 126 and 133.** (1) When the merge base has no
    install schema, `audit` reads its rules with the 1.x conversion of
    113 and 123, in memory: it writes nothing, uses no network, and
    needs no old binary (126). It prints one line saying that the merge
    base has a 1.x layout and was converted in memory. (2) D4 applies
    to the checked tree (HEAD) only, never to the merge-base read; a
    1.x HEAD under a 2.0 binary still fails with D4's line. (3) The
    upgrade pull request changes rules, so `audit` checks it against
    the converted merge-base rules and against HEAD's rules and reports
    both results (issue 73); it touches protected paths, so it gets the
    highest tier and goes to human review. (4) The batch-37 live run
    includes an upgrade pull request on an install from each released
    tag, with a 1.x merge base, and shows both audit results. Meta-spec
    O17, re-review S1. Source: owner, 2026-10-16.
152. **How far batch 34 rewords the process pack's deferral rules - ruled
    2026-10-19, clarifies 149(2).** (1) The "only" of 149(2) limits
    the tracker change: the process pack's deferral rules keep their
    requirement (every finding is fixed, or deferred with a reason as
    an item in the installed tracker), and of their tracker wording,
    only the name of the tracker changes. (2) The role rewording of
    meta-spec 6.4 applies to `process.no-tech-debt` and
    `process.bounded-review-loop` as to every other entry: the
    implementer, the controller, and the ledger become role-neutral
    words such as "the author of the change", "the person who decides",
    and "the record". No rule changes. (3) The reference-workflow
    templates keep naming the roles. 149(2) is clarified, not reversed.
    Meta-spec O18, re-review T4. Source: owner, 2026-10-19.
153. **Which text an unchanged entry takes in the in-memory merge-base
    read - ruled 2026-10-19, extends 151.** (1) The in-memory 1.x
    conversion of 151(1) maps the layout only: which pack or local pack
    holds each entry, overrides, exclusions, and the area model. It
    reads every entry's text from the merge base and pins nothing to
    the binary's packs. (2) So the merge-base audit applies the 1.x
    rules as the merge base holds them, and the HEAD audit applies the
    2.0 rules; where the two differ, the audit shows it, and 151(3)
    sends the pull request to the highest tier and human review. (3)
    113 still governs the conversion that `update` writes to disk.
    Meta-spec O19, re-review T5. Source: owner, 2026-10-19.
154. **The reference workflow's tiers: a fast default, and the full tier
    as an explicit escalation - ruled 2026-10-19, amends the tiers of
    `process.risk-tiers` (batch 31).** The owner, verbatim: "my
    adopters complain that the full mode still takes a whole day for
    tasks that \"normally tak 1.5 hours\" while that might be
    overexaggerated the feeling of these adopters must somehow be
    addressed." (1) The reference workflow has a sensible, fast default
    tier. (2) The full tier becomes an explicit escalation: it runs only
    when the human explicitly allows it. The agent never selects it by
    itself. (3) When it proposes the escalation, the agent that talks to
    the human states what the full tier means and its consequences: the
    extensive token usage and the time it takes. (4) The full tier is
    lightened sensibly, because the current full tier is too extensive.
    (5) It ships in 2.0.0 as HR-198, in batch 34 with the reference
    workflow (HR-181). The default tier's pipeline, the fate of the full
    criteria, the content of the escalation statement, and the lightened
    full pipeline are meta-spec O20 to O24. Source: owner, 2026-10-19.
155. **The default tier - ruled 2026-10-19, extends 154.** The owner,
    verbatim: "No, I said the full tier needs making faster. Not the
    default tier." (1) The default tier is the standard tier of
    `process.risk-tiers`, unchanged: a short spec with the decision
    gate, a plan, an implementer per task, one task review and at most
    one re-review, the live run, and a branch review only for a batch of
    two or more tasks. (2) The light tier stays as it is, and the agent
    picks between light and standard by itself. (3) The agent never
    picks the full tier (154(2)). (4) Only the full tier is lightened
    (154(4), meta-spec O23). The controller's proposal to merge the spec
    and the plan and to drop the standard tier's branch review is
    rejected. Meta-spec O20. Source: owner, 2026-10-19.
156. **The former full criteria under the escalation - ruled 2026-10-19,
    extends 154 and 155.** (1) The full criteria (an entry tagged
    `full-tier` applies, a public contract, a schema, authentication or
    security code, a data migration) no longer select the tier. A change
    that meets one obliges the agent to propose the escalation at the
    first point where it shows the human the batch's tier, the spec
    gate; the proposal names the criterion and carries the escalation
    statement (meta-spec O22). (2) When the human declines, the batch
    runs the standard tier; the plan records the criterion that was met
    and the human's refusal, and a standard tier after a declined
    escalation is not a finding for the task reviewer. (3) A criterion
    that shows only during the batch (a task turns out to touch a
    schema) makes the agent propose the escalation before its next
    dispatch; the batch keeps its tier until the human answers. (4) The
    agent may propose the escalation for another reason it names, and
    the human may ask for the full tier at any time. (5) The one
    automatic move stays: a light task with an Important or Critical
    finding moves up to the standard tier, never to full. Meta-spec O21.
    Source: owner, 2026-10-19.
157. **The escalation statement - ruled 2026-10-19, extends 154 and
    156.** The proposal of an escalation is a short statement in plain
    words with five parts. (1) What the full tier means: the steps it
    adds over the standard tier (as meta-spec O23 rules them), each with
    the model it runs on. (2) Why: the criterion that was met, or the
    reason the agent names. (3) The cost for this batch, labelled as an
    estimate: the added agent runs, by count and model, and the expected
    extra time and tokens as a range relative to the standard tier. The
    figures come from the project's own earlier batches when its records
    hold them: the ledger records each agent run's start and close time,
    and its token count where the harness reports one. Until then the
    agent uses the kit's reference figures, which batch 34 measures on
    this repository (one standard run and one full run) and states in the
    orchestrating skill. (4) What declining means: the batch runs the
    standard tier, and the statement names the steps it skips. (5) The
    human's answer is recorded verbatim in the plan beside the
    criterion, and in the ledger for a proposal made during the batch.
    Meta-spec O22. Source: owner, 2026-10-19.
158. **The lightened full tier - ruled 2026-10-19, extends 154, amends
    119.** The owner, verbatim: "cut the branch reviewer to opus. the
    rest accepted as recommended". (1) The full tier keeps a full
    written spec, but the design is shown once, as a whole, and the
    human approves it once; no approval after each section. The agent
    asks the human only about decisions that are the human's to make.
    (2) The re-review cap is one per task, as in the standard tier, and
    one for a branch-review fix wave. (3) The branch review runs for
    every full batch, a one-task batch included. (4) The branch
    reviewer's default model is opus, in every tier: 119 keeps the
    1.4.0 defaults except this one, so the standard tier's branch review
    (a batch of two or more tasks) runs on opus too. The owner chose
    this knowing that it changes the default tier.
    `process.model-policy` still holds: opus is mightier than the sonnet
    implementer. (5) The escalation statement of 157 names the full
    spec and the branch review on opus as what the full tier adds.
    Meta-spec O23. Source: owner, 2026-10-19.
159. **HR-144's design review in both tiers - ruled 2026-10-19, extends
    154, 155, and 158.** The owner, verbatim: "I want the design review
    for both but the standard tier only gets a design re-review if
    critical issues are found. Everything else gets folded in directly
    after the design review and proceeds to plan." (1) HR-144's design
    review runs in the standard and the full tier, before the human
    sees the spec. (2) In the standard tier a design re-review runs only
    when the design review finds a Critical issue. Every other finding
    is folded into the spec directly after the review, and the batch
    proceeds to the plan. The controller reads "proceeds to plan" as
    keeping the standard tier's decision gate (155(1)): the human
    approves the spec with the findings folded in, then the plan
    starts. (3) The full tier's design re-review, the design reviewer's
    model, and the tier of this repository's batches 33 to 37 are
    meta-spec O24a. Meta-spec O24. Source: owner, 2026-10-19.
160. **The rest of O24 - ruled 2026-10-19, extends 158 and 159.** (1)
    In the full tier a design re-review runs for a Critical or an
    Important finding, at most once (the cap of 158(2)); a Minor finding
    is folded in directly, as in the standard tier. The escalation
    statement names this as the full tier's design-review difference.
    (2) The design reviewer's default model is opus, as the branch
    reviewer's (158(4)); under 119 a project sets its own. (3) This
    repository's batches 33 and 34 run the full tier under the installed
    rules, because the new tiers ship in batch 34; batch 34's live run
    measures the reference figures of 157(3), one standard run and one
    full run. (4) Batches 35 to 37 run under the new rules: each meets a
    full criterion, so at each spec gate the controller proposes the
    escalation with the statement of 157, and the owner decides per
    batch. Meta-spec O24a. Source: owner, 2026-10-19.
161. **The reference workflow is independent of the superpowers plugin -
    ruled 2026-10-19.** The owner, verbatim: "As an additional item for
    the reference workflow I want us to become independent of the
    superpowers plugin. That means: adopters are not blocked from using
    superpowers together with houserules but it doesn't add any value
    because the houserules reference workflow covers all areas of what
    superpowers normally adds to our workflow." The trigger: the
    plugin's brainstorming skill opens with an open question, which
    fits a start from scratch but irritates on a project that already
    holds its design; the owner calls this a structural issue of running
    both. (1) The reference workflow covers every area the plugin adds
    to the workflow, so the plugin adds no value beside it. (2) Nothing
    blocks an adopter from installing the plugin with houserules. (3)
    The kit names no superpowers skill: the orchestrating skill's
    "superpowers:<skill> when installed" rows and the superpowers
    sentence of `process.brainstorm-first` go. (4) It ships in 2.0.0 as
    HR-199, in batch 34 with the reference workflow (HR-181), placed by
    the controller under 101; batch 34's spec maps each skill of the
    plugin to the reference-workflow part that covers it, and the owner
    rules any skill the map leaves uncovered. Source: owner, 2026-10-19.
162. **The batch-33 on-disk layout - ruled 2026-10-19, fills 126.** The
    owner, verbatim: "Let an independen reviewer check if this layout
    provokes any problems. If not, it is approved." and, after the
    review and its scoped re-review, "I approve layout v3". The layout
    is the controller's proposal v3
    (`.superpowers/sdd/2026-10-10-batch-33/layout-proposal.md`, reviews
    `layout-review.json` and `layout-review-r1.json`). (1) Local entries
    stay in `knowledge/<topic>.json`; the 1.x conversion moves the kit's
    entries to `knowledge/packs/<pack>.json`, one pinned copy per pack
    in emit form (name, version, schema id and version, a title per
    topic, declared areas with default globs, entries). (2)
    `knowledge/overrides.json` is new and project-owned: overrides by id
    and field (null removes a field) and exclusions by id with an
    optional reason. (3) `.houserules.json` gains `schema`, `packs`
    (version, source, sha256 of the canonical parsed JSON, and
    `addedAreas`), `ownedPaths`, and an optional `defaultBranch`; it
    keeps `version`, `idPrefix`, and path-only `baselines`. The 1.x
    `overrides` list splits: ids to `overrides.json`, file paths to
    `ownedPaths`. (4) One constant reserves `schema.json`, `areas.json`,
    and `overrides.json`; a 1.x topic named `overrides` stops the
    conversion with one hand step. (5) A one-id-one-source step runs on
    every `update` and `pack add`: equal text drops the local copy,
    other text becomes an override of the differing fields, and it
    replaces an existing exclusion or override of that id with one
    printed line each. (6) `update` removes a topic file only when it
    holds at least one entry and an installed pack ships every entry's
    id. (7) A retired pack entry is an override and stays in its pack
    copy. (8) A topic's title is the local file's, else the pack's. The
    batch-33 spec states the layout and the review's 13
    `spec_must_state` items. Source: owner, 2026-10-19.
163. **Area rule text per harness - ruled 2026-10-19, fills 129 for
    areas.** The owner chose option B of batch-33 spec Q1: "B". Fact:
    with the CLAUDE.md bridge (`@AGENTS.md`), Claude Code reads the
    root AGENTS.md and the `.claude/rules/` files, and no directory
    AGENTS.md (code.claude.com/docs/en/memory, fetched 2026-10-19). (1)
    The root AGENTS.md block never carries an area's rule text. Its
    section per area names the area's globs and the command
    `houserules for <path>`. (2) An area whose globs share one
    directory prefix, and that does not turn it off (134(6)), gets its
    rule text in that directory's AGENTS.md block. (3) Claude Code gets
    each area's text from `.claude/rules/<area>.md` alone, when the
    Claude renderer is enabled. (4) An area whose globs span more than
    one directory reaches a harness other than Claude Code through
    `houserules for` only. The root block has one shape, whatever
    renderers are enabled. Source: owner, 2026-10-19.
164. **The source and fetch of an upstream pack - ruled 2026-10-19,
    fills 126 for HR-191.** The owner, verbatim: "I approve of Q2 as you
    recommended it. With B as a backlog item." (1) `pack add
    git+<url>@<tag>#<path>` fetches one pack file at that tag through
    the user's own `git` command: any host, private repositories with
    the user's git credentials, and no new library. (2) `pack add
    <file>` copies a pack file the project already holds (a vendored
    file or a submodule). (3) The pin records the source, the commit
    the tag resolved to, and the content checksum (162), so a moved tag
    cannot change a pinned pack. (4) Every gate still reads only the
    copies in the tree (126). (5) An HTTPS source (`pack add
    https://.../pack.json`, which needs an HTTP client library and a
    token for private hosts) is a backlog item, HR-200, not scheduled.
    Source: owner, 2026-10-19.
165. **The version scheme of the kit's packs - ruled 2026-10-19.** The
    owner chose option A of batch-33 spec Q3: "A". (1) Each kit pack
    carries its own semver, starting at 1.0.0, apart from the CLI
    version. (2) A pack's version rises only when its content changes:
    major for a removed entry or a stricter check, minor for an added
    entry, patch for a wording change. (3) A test pins each kit pack's
    content checksum to its version, so a content change without a
    version change fails CI. (4) So `update` moves, and reports, only
    the pins of the packs that changed (126). Source: owner, 2026-10-19.
166. **How a project upgrades an upstream pack - ruled 2026-10-19,
    supersedes the re-add clause of 126.** The batch-33 spec review
    (finding S5) found that 126 ("moves only when the project runs
    `pack add` again with a new version") and 162 (`pack add` refuses
    an installed name) leave an upstream pack no upgrade path. The
    owner chose option B: "B". (1) A separate command,
    `houserules pack upgrade <name> <tag>`, moves the pin of an
    installed upstream pack to a new tag of the same source. (2)
    `pack add` stays refuse-only for an installed name. (3) The
    upgrade keeps the project's overrides and exclusions and runs the
    same checks as `pack add`. (4) The kit's packs still move only
    through `update` (126, 165). Source: owner, 2026-10-19.
167. **A directory AGENTS.md whose block `render` no longer writes -
    ruled 2026-10-19, reconciles 134(3) with 115.** The batch-33 spec
    review (finding S12) found that 134(3) (`render` removes a removed
    area's directory AGENTS.md) collides with 115 (`render` never
    touches text outside its block). The owner chose option A: "A".
    (1) `render` removes every directory AGENTS.md block it no longer
    writes: the block of a removed area, of an area set to
    `"agentsMd": false`, and of an area whose globs no longer share
    that directory prefix. (2) After it removes a block, `render`
    deletes the file only when nothing but whitespace is left. A file
    with the project's own text keeps that text. (3) `render --check`
    fails on such a leftover block. Source: owner, 2026-10-19.
168. **Several installs in one repository - ruled 2026-10-19, extends
    135.** The batch-33 spec review (finding S7) found that the spec
    named no path, baseline, or owner for the seeded CI workflow of a
    subdirectory install, and no behavior for a second install in the
    same repository. The owner chose option B: "B". (1) One repository
    may hold several houserules installs in 2.0.0. (2) Each
    subdirectory install gets its own seeded CI workflow,
    `.github/workflows/knowledge-<folder>.yml` at the git top level,
    and `update` tracks and rewrites it through a baseline (D3). (3) An
    install at the git top level keeps `knowledge.yml`. Source: owner,
    2026-10-19.
169. **The batch-33 spec is approved - 2026-10-19.** The owner,
    verbatim: "approved". The approval covers
    `docs/specs/2026-10-19-batch-33-knowledge-layer.md` as it stood at the
    batch-33 spec gate, after the design review and two scoped re-reviews
    (`spec-review.json`, `spec-review-r1.json`, `spec-review-r2.json`;
    every finding fixed, the last round without a further review), and
    the controller calls C1 to C15 of its section 13. Source: owner,
    2026-10-19.
170. **Four agents at once in batch 33; full-tier batches measure their
    cost - ruled 2026-10-19.** The owner, verbatim: "You are allowed to
    use up to 4 agents at once for this batch. When using full tier, i
    want you to measure the token and time efficiency we currently have
    so we are able to compare gains and losses through the progression
    of our changes. Plan is approved." (1) Batch 33 runs up to four
    agents at once (the orchestrating skill's default is three). (2)
    Every full-tier batch, from batch 33 on, measures its token and time
    cost per phase and per dispatch (`process.full-tier-measures-cost`):
    the controller's model usage, each dispatch's tokens, tool uses, and
    duration, and wall-clock time. Batch 33 is the baseline, with every
    dispatch of batches 28 to 33 that the transcripts hold
    (`.superpowers/sdd/2026-10-10-batch-33/efficiency-baseline.json`).
    (3) The batch-33 plan is approved. Source: owner, 2026-10-19.
171. **The rustls advisories are fixed on main before batch 33 - ruled
    2026-10-19.** `cargo deny` failed on main (RUSTSEC-2026-0346 to
    -0349, rustls 0.23.45, a transitive dependency of axoupdater), so
    every lint run was red. The owner chose option A: "approved as
    recommended". A branch from main carries only `cargo update -p
    rustls` (0.23.46), goes through a PR, and merges fast-forward once
    CI is green; `batch-33` then rebases onto it. Source: owner,
    2026-10-19.
172. **The full tier is purged - ruled 2026-10-20, supersedes 154, 156,
    157, 158 (1)-(3) and (5), 160 (1), (3) and (4), and 170 (2) where it
    names full-tier batches.** The owner, verbatim: "Make sure you only
    escalate to full tier where absolutely necessary. You're spending a
    lot of time and tokens on things that should go faster and
    smoother." and then "Purge the full tier completely for us as well
    as for the reference workflow. Instead, for critical tasks, find
    other a different, faster, more efficient way to assert quality and
    correctness." (1) No tier named full exists, in this repository's
    process and in the reference workflow. No batch, task, or proposal
    selects or escalates to it. The tiers are light and standard (155,
    159 for the design review). (2) This repository runs the standard
    tier from batch 33's Task 9 on: one task review, at most one
    re-review, and a branch review for a batch of two or more tasks, on
    opus (158(4)). (3) A critical task gets a faster, more efficient
    check of quality and correctness in place of the full tier's review
    rounds. The controller proposes the check, and the owner rules it
    (open). (4) The entries that selected the full tier
    (`houserules.cli-changes-run-full-tier`,
    `houserules.template-changes-run-full-tier`) are retired; the
    controller's interim `process.full-tier-only-when-necessary` is
    replaced by `process.no-full-tier`. (5) The kit side ships with
    HR-198 in batch 34. (6) Controller call, for the owner to check:
    every batch measures its cost as 170(2) describes
    (`process.full-tier-measures-cost`), so later batches compare with
    batch 33. Source: owner, 2026-10-20.
173. **A critical task: tests first and one review - ruled 2026-10-20,
    fills 172(3).** The owner chose "Tests first + one review" of the
    controller's four options. (1) A critical task is one whose change
    alters the CLI's commands, flags, or output, a schema,
    authentication or security code, or the data on disk of an install.
    (2) The plan turns the spec's cases for the task into tests before
    any code. The implementer commits them first and records one run
    that shows them failing at BASE. (3) The implementer makes them
    pass. The implementer still works test-first for every further
    change (`process.tdd`); only the evidence shrinks: that one RED run
    and the whole suite passing at HEAD, with no recorded RED and GREEN
    for every further step and no disclosed-mutation proof. (4) One task review on opus, and
    never a re-review. The controller checks each fix by re-running the
    reviewer's own reproduction and records it in the ledger
    (`process.fix-round-verification-record`): a Critical or Important
    finding goes to a fresh implementer, a Minor one the controller
    fixes itself. (5) Every other task runs the light or the standard
    tier unchanged. (6) This repository applies it from its next
    critical task; the kit side ships with HR-198 in batch 34. Source:
    owner, 2026-10-20.
174. **The newer-format line, and the batch-33 controller calls -
    ruled 2026-10-20.** The controller put the open line question (A or
    B) and controller calls C16 to C25 and 172(6) to the owner in one
    message, with A recommended. The owner, verbatim: "approved". (1)
    Option A: the line for a pack copy or an install marker that a newer
    houserules wrote names the file's format id, the running houserules
    version, and the format major to install a release for:
    `<path>: pack schema "<id>" is newer than houserules <version>
    reads; install a houserules release that reads pack schema major
    <major>`, and the same form for `install schema` (C22). No built-in
    release table. (2) Calls C16 to C25 of the batch-33 spec and 172(6)
    stand as built. (3) The five README corrections of batch 33's Task 9
    stay. Source: owner, 2026-10-20.
175. **How 2.0.0 is marked for release-please - ruled 2026-10-20.** The
    batch-33 branch review found no breaking-change marker on the
    branch, so release-please would propose 1.5.0. The owner chose "feat!
    now (Recommended)" of two options. At aggregation, the batch-33
    commit that makes every reading command refuse a 1.x install (D4)
    gets the type `feat!` and a `BREAKING CHANGE:` footer that names the
    refusal and its fix, `houserules update`. No `Release-As` commit.
    Source: owner, 2026-10-20.
176. **The batch-33 controller calls C26 and C27 - ruled 2026-10-20.**
    The batch-33 branch review found two controller calls that the spec
    did not file for the owner: C26 (a kit-named pack copy with no pin
    names its source as `an unrecorded source`) and C27 (one reader
    validates every pin of `.houserules.json` whole, with one line per
    defect). The controller filed both in spec section 13 and put them
    to the owner with the merge request. The owner, verbatim: "You may
    merge, file the two calls". Both calls stand as built. Source:
    owner, 2026-10-20.
177. **The batch-33 stats proposals - ruled 2026-10-20.** The controller
    put the 43 owner-gated proposals of the batch-33 close run
    (`.superpowers/sdd/2026-10-10-batch-33/stats.json`) to the owner on a
    review page, one card each with the controller's call. The owner
    ruled every card; the rulings are copied in the batch ledger.
    (1) Demote 13 standing entries: `houserules.controller-gates-use-the-tree-binary`
    (area cli), `houserules.pnpm-only` (area rust),
    `process.backlog-drives-work`, `process.brainstorm-first`,
    `process.code-health-scan`, `process.ff-only-merges`,
    `process.keep-knowledge-current`, `process.live-run-before-ci`,
    `process.model-policy`, `process.owner-rulings-need-owner-supersession`
    (area process), `process.deliverables-json`, `process.knowledge-first`
    (area global, so that their report-field checks still join every
    audit package), and `security-hygiene.no-focused-tests` (area tests).
    The owner approved `process.code-health-scan` on a condition,
    verbatim: "Can only be demoted if the writing plans step is in the
    repo and not only part of the superpowers plugin we still use." The
    condition holds: the Plan row of the lifecycle table in
    `.claude/skills/orchestrating/SKILL.md` (and its `template/` source)
    names the scan and its id. Standing drops from 38 to 25, the budget.
    (2) The other 14 demotions are refused: the entries stay standing.
    (3) No entry gets the `ruled-keep` tag yet. The owner, verbatim: "do
    not put in the ruled-keep tag yet. I want to see how the changes
    develop over the next implementation rounds. Just apply the
    changes." (4) Mechanize nine judged rules (HR-204), with the
    controller's proposed checks; for `writing-style.code-comments` the
    owner, verbatim: "approved but do not weaken the rule or replace it
    by the deterministic check. the rule is broader than what a
    deterministic check alone can offer." (5) No rule is narrowed. For
    `process.claims-match-artifacts`,
    `process.closure-claims-carry-enumeration`, and
    `writing-style.doc-comments`, the owner, verbatim: "propose
    mechanization solution instead of narrowing"; HR-204 items 1, 2, and
    9 are those solutions. The narrowing of `process.evidence-outlives-the-session`,
    `process.tdd`, and `quality.principles` is refused. Source: owner,
    2026-10-20.
178. **The design review repeats until no Critical finding is left -
    ruled 2026-10-20, clarifies 5.159.** The owner, verbatim: "If the
    reviewer returns with critical reviews, you have to fix these and
    then dispatch another reviewer again. This repeats until no
    criticals are left." (1) After each design review that holds a
    Critical finding, the author fixes every finding, and a fresh
    reviewer reviews the spec again. (2) The loop has no cap: it ends
    with the first review that holds no Critical finding. (3) Important
    and Minor findings are folded into the spec without a further review
    (5.159). (4) It holds for this repository's batches and for the
    reference workflow's designing skill (HR-144). Source: owner,
    2026-10-20.
179. **`process.rulings-to-file` narrows - ruled 2026-10-20 (batch-34
    spec Q1, HR-172).** The owner chose "Narrow, refuse clause
    (Recommended)". (1) Only a decision that changes what the spec states
    amends the spec in the same turn; a decision inside the spec's intent
    is recorded in the task brief or the task report, where the branch
    review reads it. (2) HR-153's clause (a brief amendment that adds or
    drops a test, a file, or a gate against the plan is a ruling for the
    spec's outcomes section) is refused. (3) HR-204 item 5's batch-close
    warn fires only for a decision that changes what the spec states.
    177(5) ruled the stats proposals and does not cover this item, which
    5.147(2) scheduled for this spec review. Source: owner, 2026-10-20.
180. **Scratch GitHub repositories for live runs - ruled 2026-10-20
    (batch-34 spec Q2).** The owner, verbatim: "You are allowed to
    create scratch repos as long as they are private and as long as you
    remind me to delete them." (1) The controller may create a GitHub
    repository for a live run, such as the GitHub tracker's proof and a
    run of the seeded CI; it is private. (2) Every report that follows
    its creation names each such repository and reminds the owner to
    delete it, until the owner says it is gone; houserules never deletes
    a repository itself. (3) Batch 34's live run proves the GitHub
    adapter and the seeded CI on such a repository. Source: owner,
    2026-10-20.
181. **Three plugin skills stay uncovered - ruled 2026-10-20 (batch-34
    spec Q3, HR-199, 5.161(4)).** The owner chose "Leave all three
    uncovered (Recommended)". The reference workflow covers no
    counterpart of the superpowers plugin's systematic-debugging,
    writing-skills, and diagnosing-superpowers skills: none is a
    lifecycle phase. The migrating-knowledge skill says so; a project
    that wants them keeps the plugin, which nothing in the kit asks for
    or breaks. Source: owner, 2026-10-20.
182. **Issue 38's design-phase rules ship - ruled 2026-10-20 (batch-34
    spec Q4, HR-144).** The owner chose "Ship all four (Recommended)".
    The process pack gains four non-standing entries that the
    spec-reviewer template cites, so they join the reference workflow's
    required ids: (a) a decision that rejects a library or departs from
    the source research or an approved spec carries a captured probe,
    the rejected option's best case, and a reversal condition; (b) each
    supporting claim is labelled run, read, or assumed, and an assumed
    claim is never the sole reason for a rejection; (c) a spec lists
    every concept it takes from its sources, with the source's status
    and a check against the goal; (d) every decision the owner or a
    reviewer overturns is a `Reversal:` ledger line, counted at the
    retrospective. Source: owner, 2026-10-20.
183. **HR-153's clauses for standing rules - ruled 2026-10-20 (batch-34
    spec Q5).** The owner ruled the nine clauses on a review page; the
    rulings are copied in `.superpowers/sdd/2026-10-20-batch-34/clause-rulings.json`.
    Approved, each joining its entry's body: `process.claims-match-artifacts`
    (a fix round that re-runs or re-pins earlier evidence says so, and a
    line citation into a regenerated evidence file is re-derived; a
    documentation claim quotes every bearing passage and says which one
    the measurement supports, and a duration is recomputed from its two
    timestamps), `process.closure-claims-carry-enumeration` (a corpus of
    deliverables is selected by content, never by file name; an
    exception list is a closure with its own script),
    `process.gates-cover-generated-commits` (a check added to a shipped
    gate is walked through the installed populations and run over the
    template at each release tag), `writing-style.code-comments` (a
    comment on measured or third-party behavior names an openable
    source; a doc comment written in a fix round is swept for the
    round's vocabulary, and the fix-round dispatch names the sweep),
    `writing-style.doc-comments` (a new caller re-opens the callee's doc
    comment), and `process.tdd` (a RED against a stub of an
    implementation already drafted is a `mutation` entry). Refused: the
    code-comments clause that the controller files the knowledge entry
    before the dispatch. Source: owner, 2026-10-20.
184. **Issue 61's workflow-agnostic step is core - ruled 2026-10-20
    (batch-34 spec Q6, HR-181).** The owner chose "Core step, no recipes
    (Recommended)". (1) The step (read the rules for the files to
    change, implement, audit, write the report) ships as a core skill,
    `houserules-step`, that every install gets, and a core hook script
    any workflow can call. (2) No recipe for another workflow ships:
    issue 61's point 5 is not adopted, because naming other workflows
    rots with their releases and a Superpowers recipe would break 5.161.
    Source: owner, 2026-10-20.
185. **HR-194's convenience criterion with two tiers - ruled 2026-10-20
    (batch-34 spec Q7, amends 5.128).** The owner chose "Doesn't make a
    task critical (Recommended)". An open deferral is a convenient
    candidate when its files overlap the batch's planned files and
    folding it in does not turn a non-critical task into a critical task
    (173(1)). This replaces 5.128's "does not raise the batch's tier",
    which no longer separates candidates once only the light and the
    standard tier exist (172). Source: owner, 2026-10-20.
186. **Release commits pass the every-commit entry by subject - ruled
    2026-10-20 (batch-34 spec Q8, extends 5.138(3)).** The owner chose
    "Also exempt by subject (Recommended)". The every-commit `Work-Item:`
    entry exempts a commit whose subject matches `exempt_subjects`
    (default `^chore\(.+\): release `) beside the bot-author list of
    5.138(3), because release-please's commits are authored by whoever
    owns its token: six of this repository's seven release commits are
    authored by the PAT's owner. Dependabot stays exempt by author.
    Source: owner, 2026-10-20.
187. **The batch-34 spec is approved - ruled 2026-10-20.** The owner,
    verbatim: "approved. write the plan, let it be reviewed until no
    critical issues are left, then proceed to the implementation
    autonomously for this batch." (1) The spec
    `docs/specs/2026-10-20-batch-34-workflow-and-work-items.md`
    (revision 4, with rulings 179 to 186) is approved with its
    controller calls C1 to C27 and the hook commands of its section
    4.4a. (2) The batch-34 plan is reviewed as 178 rules for a spec: a
    fresh reviewer after each round that holds a Critical finding,
    until a round holds none. (3) The controller then runs the batch's
    implementation without a further owner gate: the tasks, their
    reviews, the eval run, the live run, and the branch review. The
    merge to main still waits for the owner's word. Source: owner,
    2026-10-20.
188. **houserules turns bd's usage metrics off for the bd calls it
    makes - ruled 2026-10-20 (batch-34 Task 3 review, issue 6).** The
    owner chose "Turn them off (Recommended)". bd 1.3.1 queues a usage
    event (command name, bd version, platform, a machine-derived id)
    for its vendor's endpoint on every call unless metrics are off.
    houserules sets `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` on
    every `bd` it starts, in a project and in its CI, so an adopter
    never sends events through houserules; the Beads adapter docs say
    so. A user's own `bd` calls keep bd's own setting. Source: owner,
    2026-10-20.
189. **process.skills-for-procedures drops its plugin-skill hint - ruled
    2026-10-21 (batch-34 Task 9 review).** The owner chose "Drop the
    sentence (Recommended)". The summary's sentence "Author it with the
    writing-skills skill when available" names a skill of the
    superpowers plugin, which spec 6.4 forbids; the rule stays: a
    repeatable procedure becomes a project skill. The test that guards
    live case 20 matches every plugin skill name of spec 6.4's table,
    not only the plugin's name. Source: owner, 2026-10-21.
190. **Batch 34 may merge to main without a further owner word; no
    release - ruled 2026-10-21.** The owner, verbatim: "permission
    granted. make sure you adhere to how we planned through the specs
    and branches and that you don't release a version." (1) The
    controller merges batch 34 to main, fast-forward only from the CLI,
    once the planned lifecycle has run: every task and its review, the
    controller steps of the plan, the eval run, the live run, the
    branch review and its fix wave, the batch-close knowledge commit,
    the aggregation into one to five commits, and a pull request whose
    checks pass on all three systems. No step is skipped or reordered to
    reach the merge. (2) No version is released: release PR #83 stays
    unmerged until batch 37 is on main (5.99), no tag is pushed, and no
    release workflow is run. Source: owner, 2026-10-21.
191. **OpenSpec support ships in 2.0.0; Spec Kit follows in a minor
    release - ruled 2026-10-21 (HR-209).** The owner, verbatim: "I want
    support for openspec in v2.0.0 and spec kit can follow as a minor
    release later. I want to shift to quick answers to reduce time."
    (1) HR-209 covers OpenSpec and is part of 2.0.0; the levels of
    compatibility and the batch it lands in are still to be ruled.
    (2) Spec Kit support is HR-210, a minor release after 2.0.0.
    Source: owner, 2026-10-21.
192. **2.0.0 ships its breaking core and OpenSpec; the rest of batches
    35 to 37 moves to 2.x minors - ruled 2026-10-21, amends 5.98, 5.101,
    and 5.102.** The owner chose "Yes (Recommended)" for: "Ship 2.0.0
    with the core above (HR-138, HR-152, HR-175, HR-171/197/057, HR-187
    schema IDs, HR-176) plus OpenSpec, and move every other item of
    batches 35-37 to 2.x minors", after a plain-language table of every
    item's content. (1) Batch 35 is the 2.0 contract batch: HR-138,
    HR-152, HR-175, HR-176, HR-171, HR-197, HR-057, and HR-187's schema
    ids (versioned schemas for the documents 2.0 emits). (2) Batch 36 is
    OpenSpec (HR-209) and carries the upgrade-path live proof that batch
    37 held; it is the last batch of 2.0.0, and release PR #83 stays held
    until it is on main. Batch 37 is dropped. (3) The other items keep
    their bodies, leave their batch, and take milestone `2.x`: HR-042,
    HR-139, HR-163, HR-164, HR-173, HR-183, HR-188, HR-072, HR-141,
    HR-148, HR-158, HR-174, HR-179, HR-185, HR-186, HR-189, HR-190,
    HR-192, HR-193, and HR-211 (HR-187's MCP server and recipes). Nothing
    is lost (5.100): each stays open in the backlog. The GitHub issues of
    the moved items stay open past 2.0.0. Source: owner, 2026-10-21.
193. **The 2.0.0 cut, revised by the owner - ruled 2026-10-21, amends
    192.** The owner, verbatim: "HR-188, 183, 158, 141 (because it is in
    progress), 148, 185, 179, 186, 190 (because it fits openspec), 192 must
    be v2.0.0. The rest is accepted as recommended as the new cut. Upon
    release of 2.0.0 i want the github issues to be commented on with what
    part is covered and what follows." (1) 2.0.0 holds 192's core
    (HR-138, HR-152, HR-175, HR-176, HR-171, HR-197, HR-057, HR-187's
    versioned schemas), OpenSpec (HR-209), and HR-188, HR-183, HR-158,
    HR-141, HR-148, HR-185, HR-179, HR-186, HR-190, HR-192. (2) 2.x keeps
    HR-042, HR-139, HR-163, HR-164, HR-173, HR-072, HR-174, HR-189,
    HR-193, and HR-211. (3) At the 2.0.0 release, each GitHub issue that a
    2.0.0 or 2.x item names gets a comment stating which part 2.0.0
    covers and which part follows, and in which item (HR-212). (4) The
    controller lays the 2.0.0 items into batches by theme (5.101's
    method): 35 audit and checks, 36 evidence and report claims, 37 the
    per-change review layer, 38 OpenSpec with HR-190 and the
    upgrade-path live proof, the last batch of 2.0.0; release PR #83 stays
    held until batch 38 is on main. Source: owner, 2026-10-21.
194. **HR-173 joins 2.0.0 by itself when HR-183 needs it - ruled
    2026-10-21.** The owner, verbatim: "If it can't be tested without the
    runner, the runner should automatically join v2.0.0." When batch 35's
    spec or plan finds that HR-183's runner contract cannot be tested
    without the exit-code runner of HR-173, HR-173 moves into batch 35
    with no further owner ruling; the spec records the finding and its
    probe. Otherwise HR-173 stays 2.x. Source: owner, 2026-10-21.
195. **An adopter's batch workspaces are gitignored from the start -
    ruled 2026-10-21.** The owner, verbatim: "workspaces should be
    gitignored from the start in adopters." Context: adopters report that
    the workspaces grow large and that check-knowledge in CI fails a
    `verify` path into a gitignored workspace (HR-213, batch 35). The kit
    gives the workspaces a gitignored home from the first install, the way
    this repository keeps `.superpowers/` out of git; batch 35's spec names
    the location, how `init` and `update` write the ignore line, how an
    install that tracks its workspaces today moves, and what a `verify`
    path into a workspace becomes. Source: owner, 2026-10-21.
196. **Batches 30 to 34 are accepted; the batch-34 scratch repository is
    deleted - ruled 2026-10-21.** The owner, verbatim: "batches are
    accepted and repo is deleted." The acceptance rulings of batches 30,
    31, 32, 33, and 34 are homed here, so the archive sweep of the
    finishing skill (step 3) runs on this branch. The private scratch
    repository jblossey/houserules-b34-live of the batch-34 live run no
    longer exists (process.scratch-repos-are-private-and-reported).
    Source: owner, 2026-10-21.
197. **The batch-34 rulings sheet - ruled 2026-10-21.** The owner ruled every
    card of the batch-34 ruling sheet with the controller's recommendation
    (33 cards on the sheet, the last one in chat: "Confirm the decline").
    (1) A commit that records a ruling or an acceptance names the work items
    it concerns; a ruling about no item files one first
    (process.ruling-commits-name-their-items). (2) The twelve standing rules
    the stats run proposed to demote stay standing. (3) Eight of the nine
    mechanize rows keep the checks HR-204 already approved;
    process.gates-cover-generated-commits folds into HR-219 as the planning
    skill's paper-walk row. (4) The eight narrow rows are declined. (5) The
    three narrow rows the controller left unapplied stay declined. (6) Every
    entry the rulings keep (23) carries the tag ruled-keep, set in its own
    file or, for a pack entry, through knowledge/overrides.json. (7) The
    corrected body of houserules.template-is-the-source is approved.
    Source: owner, 2026-10-21, the ruling sheet
    https://claude.ai/artifact/JQEoWYpwuDZpnEvAdR7p2i and the chat.
198. **Batch 35 may merge to main without a further owner word; no
    release - ruled 2026-10-21.** The owner, verbatim, to the question
    whether ruling 190's leave also covers batch 35: "yes and start".
    Ruling 190 (1) and (2) apply to batch 35 word for word: the controller
    merges fast-forward only from the CLI once batch 35's planned
    lifecycle has run, with no step skipped or reordered, and releases no
    version (release PR #83 stays held until batch 38, design.md 193). The
    owner still approves batch 35's spec at its gate. Source: owner,
    2026-10-21.
199. **Batch 35's design questions - ruled 2026-10-21.** Asked after the
    first design review (spec-review.json), as quick answers. (1) Tool
    arguments: "Project only (Recommended)": the project's
    `.houserules.json` holds each tool's whole command line; a pack's check
    names only the tool key, the parser, and the files, so no text from a
    pack reaches a process's argv. (2) HR-173: "Parser in HR-183
    (Recommended)": the exit-code parser ships with HR-183 in batch 35;
    HR-173 is rescoped to its rustdoc, ruff, and eslint recipes and the
    split of `writing-style.doc-comments`, and stays 2.x. (3) The JSON
    parser of issue 63: "Defer to 2.x (Recommended)": filed as a 2.x item;
    at the release issue 63 gets the comment of 193(3) and stays open.
    (4) The default workspace home: ".houserules/work/ (Recommended)",
    under the one ignored `.houserules/` directory, with a location a
    project can set. (5) SARIF parsing, verbatim: "check if it had no
    release because it is considered stable or because it is stale. If
    stale, i want you to research an alternative that is well-maintained
    and well-adopted. If there is none, only then we own the minimal
    types." (6) The six judged rules of HR-219 (1)
    (houserules.crash-paths-are-named, knowledge-base.state-only-the-source,
    quality.exclusive-pins-are-mutation-proven,
    quality.gates-derive-their-scope, quality.user-facing-text-is-contract,
    writing-style.instructions-cover-the-state-space): "ruled-keep all six
    (Recommended)": each stays judged, with a per-row reason in the spec,
    and carries the tag ruled-keep. Source: owner, 2026-10-21.
200. **Batch 35's second-review questions - ruled 2026-10-22.** (1) The
    parser and the install line of an external tool: "Project's runners
    entry (Recommended)": the project's `.houserules.json` `runners` entry
    holds a tool's command, its parser, and its install line together; a
    pack's check names only the tool key, the files, and the scope. This
    amends 199(1), which named the parser in the pack's check. (2) The
    check-report-claims checks of HR-219 part 4 (commit-body counts,
    closure words, scratchpad paths): "HR-204 in batch 36 (Recommended)":
    HR-204, with its items 1 to 10, joins batch 36 (evidence and report
    claims). Source: owner, 2026-10-22.
201. **The batch-35 spec is approved once its design review holds no
    Critical finding - ruled 2026-10-22.** The owner, verbatim: "once the
    reviews find no criticals anymore, the spec is approved from my side
    and you can proceed." When a design review of
    `docs/specs/2026-10-21-batch-35-audit-and-checks.md` holds no Critical
    finding, the controller folds that review's Important and Minor
    findings into the spec (designing skill, step 3.3), marks the spec
    approved under this ruling, and proceeds to the plan without a
    further owner gate. A finding that needs an owner ruling still goes
    to the owner. Source: owner, 2026-10-22.
202. **HR-175's report checks stay warnings - ruled 2026-10-22.** The new
    `non_empty` option on `security-hygiene.verify-current-docs` and the
    new `covers_ids` option on `process.knowledge-first` keep their
    entries' level warn: "Stay warnings (Recommended)". A violation is a
    warn row with its evidence lines, which the task reviewer reads; it
    fails no audit and is no upgrade red. Source: owner, 2026-10-22.
203. **Runner checks in CI judge the PR head - ruled 2026-10-22.** On a
    pull request, the seeded `knowledge.yml` and this repository's `ci.yml`
    check out the merge commit but audit `--head` the PR head, so the tree
    test of a runner check would exit 2 on every PR. The owner chose
    "Check out the PR head (Recommended)": both workflows check out
    `github.event.pull_request.head.sha`, so every row of the audit judges
    one tree. The seeded file reaches installs through `update`'s
    baseline. Source: owner, 2026-10-22.
204. **The controller decides alone until 08:00 local on 2026-10-22 -
    ruled 2026-10-22.** The owner, verbatim: "I want you to work
    autonomously until i come back at 8am. If during that time you would
    have needed decisions from me, decide those for me and provide me with
    a complete page of these decisions in the morning when i come back so
    i can veto in case some diverge from how i want things to be decided.
    We will accept that you possibly have to rewrite some things in case
    of diverging decisions." Each decision that would have gone to the
    owner is taken by the controller, written to its home file as usual,
    marked "decided under 204", and listed with its options and reasons in
    `.superpowers/sdd/2026-10-21-batch-35/delegated-decisions.md`, which
    becomes the morning page. A veto reopens the decision; the rework it
    causes is accepted. The owner's limits, verbatim: "These decisions do
    not include critical steps like releasing a version, spending money,
    or interacting with external systems beyond what i have granted you
    sensibly in the past. Your privileges are scoped to the current work
    in houserules only. You may not autonomously change specs except if a
    review finds issues with it (like we already operate) and you may in
    no case autonomously change the meta-spec." So: no release, no spend,
    no external action beyond the earlier grants (the ff-only merge of
    198, private scratch repositories); a spec changes only to fold a
    review's findings; the meta-spec never changes under 204, and a
    finding that would amend it goes to the morning page as an open
    question. Source: owner, 2026-10-22.
205. **The batch-35 live run installs Semgrep in a throwaway venv - ruled
    2026-10-22.** Spec case 12 needs a real Semgrep and none is installed.
    The owner chose "Throwaway venv (Recommended)": `pip install
    semgrep==<exact version>` into a venv under `~/.cache`, run with
    `--metrics off` and a local rule file, and delete the venv after. The
    download from PyPI is the only outward action; nothing in the
    repository changes. Source: owner, 2026-10-22.
206. **The owner keeps the batch-35 night decisions - ruled 2026-10-22.**
    Asked before the merge whether the decisions taken alone under ruling
    204 (D1 to D11 of `.superpowers/sdd/2026-10-21-batch-35/delegated-decisions.md`)
    and the five meta-spec edits of spec revisions 4 and 5 (M1) stand, the
    owner chose "Keep all (Recommended)". Each stands as recorded; D7 (the
    `url` and `percent-encoding` crates as direct dependencies) is now the
    owner's choice under `quality.well-maintained-libraries`. Source: owner,
    2026-10-22.
207. **Batch 35 accepted; the controller decides its close proposals and
    deferrals - ruled 2026-10-22.** The owner, verbatim: "Accepted. Decide
    the stats proposals and deferrals for me. Proceed". The controller's
    decisions, each reviewable by the owner:
    (1) The four owner-gated `mechanize` proposals of the batch-35
    `stats.json` (`process.fix-proofs-extend-the-reviewers-run`,
    `process.gates-cover-generated-commits`,
    `process.refused-commands-hand-back`, `process.rulings-to-file`): keep
    each judged and tag it `ruled-keep`. Whether a proof extends a
    reviewer's run, whether a paper walk covers every author class, whether
    a refused step was taken another way (a harness event, not in the
    diff), and whether a ruling reached its home file all need a reader;
    HR-204 (batch 36) mechanizes the parts of report claims that a check
    can read. (2) `narrow process.rulings-to-file`: declined with (1).
    (3) The standing budget (26 of 25): demote
    `process.every-commit-names-a-work-item` to area `global`. It caught
    nothing in the window, and `check-commit`, the commit-msg hook, and
    CI's check-commit job enforce its check whether or not it is standing;
    area `global` keeps it in every audit package. (4) The two `narrow`
    proposals with owner_gate false (`houserules.task-gates-mirror-ci`,
    `quality.absence-is-designed`): no change; each entry already confines
    itself to its contract surface. (5) The deferrals: HR-226, HR-227,
    HR-228, HR-229, HR-230, and HR-232 join batch 36; HR-231 (mechanize
    four judged rules, each needing its own design and corpus run) moves
    to 2.x. Source: owner, 2026-10-22.
208. **Batch 36: approval and merge leave as for batch 35 - ruled
    2026-10-22.** (1) "Yes, as 201 (Recommended)": the batch-36 spec is
    approved once a fresh whole-meta-spec design review holds no Critical
    finding; the controller folds that review's findings and proceeds to
    the plan; owner-only questions still go to the owner. (2) "Yes, as 198
    (Recommended)": after the full lifecycle, CI green, and the owner's word
    on any decision the controller took alone, the controller merges
    ff-only from the CLI; no release, and release PR #83 stays held until
    batch 38. Source: owner, 2026-10-22.
209. **Batch 36: the quoting library - ruled 2026-10-22.** "shlex 2.0.1
    (Recommended)": `shlex` quotes the recorded command of a capture and
    every path that a printed remedy puts into a command (HR-171, HR-227;
    spec question Q1). The locked 2.0.1 adds no crate to `Cargo.lock`;
    the plan vets it and adds it as a direct dependency with `cargo add
    shlex@=<version>`. Source: owner, 2026-10-22.
210. **Batch 36: HR-204 item 5 moves to 2.x - ruled 2026-10-22.** "Move
    to 2.x (Recommended)": the batch-close warning for a ledger line that
    says "for the owner" with no spec entry leaves batch 36 as HR-233
    (milestone 2.x). The kit defines no ledger line form and no spec
    section numbers, so a deterministic check would read one repository's
    prose habits; `process.rulings-to-file` stays judged, as ruling 179
    already limits the item (spec question Q2). Source: owner, 2026-10-22.
211. **Batch 36: the natural-RED predicate of HR-204 item 6 - ruled
    2026-10-22.** "Exit code only (Recommended)": `check-report-claims`
    fails a `natural` RED whose capture exited 0. This replaces the words
    `FAILED` and `panicked` of 177(4): they are cargo's, and they fail a
    RED that is a compile error, the usual first RED of a function that
    does not exist yet (13 of the retained natural REDs; spec question
    Q-C6, `rederive-f3-f5.txt` in the batch-36 workspace). Source: owner,
    2026-10-22.
212. **Batch 36: three report-claims checks retire, HR-072 drops - ruled
    2026-10-22.** "Retire all three, drop HR-072 (Recommended)": the
    redirected-capture check, the truncation-marker check, and the
    paste-run lint of `check-report-claims` retire with the inline output
    they read; the hash check, the byte-exact excerpt check, and the
    command taken from the capture header replace them. This rules the
    meta-spec fate "kept; checks grow" of `check-report-claims` (5.100)
    for these three checks. HR-072 (narrow the paste-run lint, 2.x)
    is dropped: its subject is gone (spec question Q-C12). Source: owner,
    2026-10-22.
213. **Batch 36: the atomic file replace - ruled 2026-10-22.**
    "tempfile 3.27.0 (Recommended)": `capture` and `report-facts` write
    through `tempfile::NamedTempFile` in the target's directory and
    `persist` it (`persist_noclobber` where `capture` must not replace an
    existing file). `tempfile` moves from the dev dependencies to the
    dependencies at its pinned version (spec review finding F13;
    `library-research-tempfile.txt` in the batch-36 workspace). Source:
    owner, 2026-10-22.
214. **Batch 36: the report-facts subcommand - ruled 2026-10-22.**
    "Approve as report-facts (Recommended)": HR-158 ships as the new flat
    subcommand `houserules report-facts <REPORT> --base <ref> [--fix-base
    <ref>] [--audit <file>]`, which fills a task report's fact fields from
    git and from captures and never replaces a recorded hash
    (`houserules.1-0-surface-is-frozen`; spec question Q-RF1). Source:
    owner, 2026-10-22.
215. **Batch 36: the home of the ls-files listings - ruled 2026-10-23.**
    "All in rules::git_paths (Recommended)": the tracked-file listing
    (today in `rules/check.rs`, with a copy in `report_claims.rs`), the
    glob listing (today in `install/location.rs` and
    `rules/render/agents_md.rs`), and the index listing (today in
    `rules/ignored.rs`) move into `rules::git_paths`; the lint test of
    HR-204 item 7 names that module and the three gate binaries. This
    supersedes the module names of 177(4) for item 7 (spec question
    Q-C8). Source: owner, 2026-10-23.
216. **Second night delegation - ruled 2026-10-23.** The owner, verbatim:
    "Continue autonomously like last night. You can continue autonomously
    either until the clock strikes 8am when i take over the rulings again
    or until you reach the spec stage of batch 38, whichever comes first.
    Just like last night, give me an overview of the rulings you took on
    my behalf when the time comes for me to take over the decisions again
    so i can see if anything needs a veto." The limits of 204 apply ("like
    last night"): no release, no spending, no external action beyond the
    grants of the past; privileges scoped to the current work in
    houserules; a spec changes only to fold a review finding; the
    meta-spec never changes. The delegation ends at 08:00 local time
    (CEST) on 2026-10-23, or when batch 38 reaches its spec stage. Every
    decision taken under it goes to a page for the owner's veto. (2) To
    the question whether batch 36 may merge before the owner's word on the
    decisions taken alone (208(2)): "Merge, veto later (Recommended)": the
    controller ff-merges batch 36 from the CLI once its lifecycle and CI
    are green, with no release; a veto after the merge is fixed in a
    follow-up commit or in batch 37, which then starts from main. Source:
    owner, 2026-10-23.
217. **Batch 36: two check-report-claims lines beyond spec 6 - ruled
    2026-10-23.** "Reviewer's wording (Recommended)": a report run that
    lacks `sha256`, `command`, or `exit` gets one line per missing key,
    `<label>: no <key>; run houserules report-facts`; a natural RED whose
    capture header `end` does not parse gets `tdd[<i>]: mode "natural",
    but the RED capture "<path>" has an end that is no RFC 3339 time (<the
    jiff error>)`. Both go into spec 6 (Task 6 review, question Q1 of the
    night page). Source: owner, 2026-10-23.
218. **The second night's decisions approved - ruled 2026-10-23.** The
    owner, verbatim: "your nightly rulings are approved." Decisions D1 to
    D15 taken under 216 (`delegated-decisions.md` in the batch-36
    workspace; the night page) stand with no veto. Source: owner,
    2026-10-23.
219. **Batch 36: three check-report-claims readings of Task 7 - ruled
    2026-10-23.** (1) "Add the line (Recommended)": checks 5 and 6 print
    `<label>: <path> could not be read (<the OS error>)` for a failed
    lookup that is not NotFound; only NotFound reads as `does not exist`.
    (2) "JSON path label (Recommended)": check 10 labels a commit body
    with the commit's JSON path, its sha as listed, and `body`
    (`commits[<i>] <sha> body`, `fix_rounds[<r>].commits[<i>] <sha>
    body`). (3) "Count if a capture (Recommended)": in a commit body a
    bare file name that names a capture at the install root counts; every
    other citation is skipped, a bare file name that names no capture at
    the install root included. All three go into spec 6 (Task 7 review).
    Source: owner, 2026-10-23.
220. **The surface freeze ends at the 2.0.0 release - ruled
    2026-10-23.** The owner, verbatim: "After the 2.0.0 release, the
    freeze does not apply anymore." To the question what then holds:
    "Retire at 2.0.0 (Recommended)": `houserules.1-0-surface-is-frozen`
    is retired when 2.0.0 ships, and the freeze of 82 ends with it.
    Semver still applies: a breaking CLI or schema change is a `!`
    commit, release-please bumps the major version, and no owner ruling
    is needed per change. Until the release the freeze holds as before.
    The retirement is HR-238 (batch 38). Source: owner, 2026-10-23.
221. **A breaking change after 2.0.0 still needs an owner ruling - ruled
    2026-10-23.** The owner, verbatim: "If a change would prompt a major
    release, i.e. a breaking change, I still need to rule about it." This
    replaces the clause of 220 that no owner ruling is needed per change.
    From the 2.0.0 release on, every change that would prompt a major
    release (a `!` commit, a `BREAKING CHANGE` footer) needs the owner's
    ruling before it merges, whatever surface it touches. HR-238 retires
    `houserules.1-0-surface-is-frozen` and adds a standing successor entry
    that states this rule and names the entry it replaces. Source: owner,
    2026-10-23.
222. **Batch 36: the `env` example of the implementer template - ruled
    2026-10-23.** "Plain houserules (Recommended)": spec 7's example
    reads `-- env HOUSERULES_SKIP_SELF_UPDATE=1 houserules
    check-knowledge`, the installed binary any adopter has, in place of
    this repository's `./target/debug/houserules` (Task 11 review).
    Source: owner, 2026-10-23.
223. **Batch 36: the branch review's GitHub finding and three knowledge
    proposals - ruled 2026-10-23.** (1) "Fix both now (Recommended)": the
    fix wave gives a kept GitHub-form CI copy one hand step for its
    permissions and one for its env, each only for a marker it lacks;
    this fixes the branch review's finding (a copy with `issues: read`
    and no `contents: read` got no step) and HR-235 (a copy lacking only
    the env was asked for `issues: read`), which joins batch 36; spec 8.5
    and case 9 state the lines. (2) "Keep standing (Recommended)":
    `process.conventional-commits` stays standing and gets the tag
    `ruled-keep` (stats proposed a demote). (3) "Keep judged":
    `writing-style.readme-is-an-adopter-funnel` stays judged and gets
    `ruled-keep` (stats proposed a mechanize). (4) "Apply the edit
    (Recommended)": the body of `process.reversals-are-recorded` states
    that each reversal is its own list item starting with `Reversal:`,
    the form `houserules stats` counts. Source: owner, 2026-10-23.
224. **The haiku trial - ruled 2026-10-23.** The owner, verbatim: "Do as
    you recommend. I want you to give haiku increasingly difficult tasks
    during the next implementations and track how it performs. The
    intention here is to gauge if - through houserule's setup - models
    that you normally wouldn't use because they are not mighty enough are
    enabled to perform better through having clear, targeted rules." From
    batch 37 on, every batch gives haiku at least one implementer task,
    starting at a simple task and one level harder after each clean close;
    a haiku task gets the same brief, knowledge, gates, and opus review as
    a sonnet task. `process.implementer-model-per-task` holds the levels,
    the moves, and the record; HR-242 collects one data point per batch.
    Source: owner, 2026-10-23.
225. **The gate of the haiku trial - ruled 2026-10-23.** "4 points + early
    stop (Recommended)": after every fourth haiku data point in HR-242,
    and at once after any Critical finding on a haiku task, the owner
    rules on the data table (haiku against the sonnet tasks of the same
    level: findings by severity, fix rounds, re-reviews, tokens, time):
    continue, change the levels or the moves, stop, or make haiku the
    default up to a level. A plan may give haiku two tasks per batch when
    it holds two at the current level. Source: owner, 2026-10-23.
226. **Batch 36: two findings of the fix-wave re-review - ruled
    2026-10-23.** (1) "Fix now (Recommended)", corrected by "A replace step
    (Recommended)": the job's own `permissions` key decides the form of a
    permissions hand step of a kept CI copy: no key, a block is added; a
    mapping block that lacks a scope, the scope is added to it; a scalar
    `read-all` holds every read scope and gets no step; a scalar
    `write-all` gets a step that replaces it with the read-only block
    (my first option counted `write-all` as holding the token, which
    contradicts spec 8.5's goal; the owner ruled the correction). Spec 8.5
    states the lines. (2) "Reviewer's wording (Recommended)": the body[0]
    of `process.reversals-are-recorded` reads "Write each reversal as its
    own list item of the record that starts with `Reversal:` and names the
    decision that was overturned; `houserules stats` counts only a line
    that starts with `Reversal:`, and the branch reviewer of the reference
    workflow also counts a `Reversal:` inside a longer entry.", true for a
    minimal install too; the test exclusion of the 5.223 sentence goes.
    Source: owner, 2026-10-23.
227. **Batch 36 accepted; its owner-gated close proposals accepted - ruled
    2026-10-24.** The owner, verbatim: "all accepted." (answering the four
    points of `.superpowers/sdd/2026-10-22-batch-36/batch-report.md`).
    (1) Batch 36 is accepted; the archive sweep moves its items and its
    batch record. (2) The six proposals for standing entries or entries
    with `source.by: user`: a body line each for
    `writing-style.doc-comments`, `quality.principles`,
    `process.rulings-to-file`, and `security-hygiene.dependency-vetting`
    (the pack entries through `knowledge/overrides.json`; the kit packs
    stay unchanged), and a body line for
    `process.refused-commands-hand-back` naming a shell glob or a variable
    as another way. The warn check for history phrases of
    `writing-style.code-comments` goes on
    `houserules.comments-name-no-review-history`, not on the judged rule:
    an entry with a check gets a deterministic row in place of a judged one
    (batch-35 spec 9 (3)). Its phrases, ruled the same day by "Unambiguous
    only (Recommended)": `now (read-only|quoted|shared|gone)` and
    `confirmed on this box`; `no longer` and `used to` stay judged, because
    most of their matches on main state a current contract (30 and 5
    comment lines of crates/; 2 of the 5 `used to` lines are current).
    (3) The three `narrow` proposals stay "no
    change". (4) No veto of O3: reading the knowledge JSON when
    `houserules get` was refused stands as a read, not as the refused
    command run another way. Source: owner, 2026-10-24.
228. **The duplicate line of `quality.principles`; batch 37 starts -
    ruled 2026-10-24.** The owner, verbatim: "yes, delete the duplicate
    and start batch 37". This repository's override of
    `quality.principles` drops the short "Tests assert behavior ..." line;
    the longer line that holds the same sentence and the auto-restore
    clause stays. Batch 37 (the per-change review layer, meta-spec
    section 4) starts with its spec. Source: owner, 2026-10-24.
229. **Batch 37: what counts as coverage - ruled 2026-10-24.** To the
    question of the first design review (F1, Q-A) what counts as covered
    for the evidence packet's uncovered surface and the route: "Checks
    opt in (Recommended)". A check counts as coverage only when it
    declares it with a new optional field of the knowledge schema
    (`covers: true`, default false, a minor addition); a file is covered
    when a check that declares it passed or warned on that file. Until a
    project marks a check, every changed hunk is uncovered. Source: owner,
    2026-10-24.
230. **Batch 37: the name of the per-change requirement - ruled
    2026-10-24.** "route (Recommended)": the command `houserules route`,
    the config key `review.route`, and issue 66's values
    `auto-merge-eligible`, `owner-review`, and `senior-review`; "tier"
    stays with the pipeline tiers of `process.risk-tiers` (meta-spec
    5.20). Source: owner, 2026-10-24.
231. **Batch 37: where the evidence bundle lives - ruled 2026-10-24.**
    "PR description (Recommended)": a section of the PR body that
    `houserules bundle write` writes from captures and `houserules bundle
    check` checks in CI; no file in the tree and no deliverables-schema
    change (issue 58's first option). Source: owner, 2026-10-24.
232. **Batch 37: the stable URL of the published schemas - ruled
    2026-10-24.** "GitHub Pages": the schemas are served by GitHub Pages of
    this repository, not as raw files on `main`. The spec states the
    publishing workflow and the hand step that enables Pages. Source:
    owner, 2026-10-24.
233. **Batch 37: pull requests from forks - ruled 2026-10-24.** "Summary
    only (Recommended)": a fork PR runs every check; its evidence packet
    goes to the job's step summary, and nothing is posted to the PR; the
    seeded workflow never uses `pull_request_target`. Source: owner,
    2026-10-24.
234. **Batch 37: this repository's own CI - ruled 2026-10-24.** "Yes, from
    batch 37 (Recommended)": `.github/workflows/ci.yml` gains the bundle
    check, the evidence comment, the route label and status, and the
    change record on `main`, so batch 37's own PR is the first live proof
    on GitHub; Dependabot and release-please are exempt from the bundle;
    the record job holds `contents: write` for the notes ref. Source:
    owner, 2026-10-24.
235. **Batch 37: the new surface under the freeze - ruled 2026-10-24.**
    "Yes, one ruling (Recommended)": batch 37 adds every subcommand and
    flag that its spec lists (`bundle write`, `bundle check`, `route`,
    `record write`, `record show`, `record list`, `record audit`,
    `stats --records`, and the flags of `evidence`, `route`, `bundle`, and
    `record`) as additions to the 2.0.0 major. Source: owner, 2026-10-24.
236. **Batch 37: the home of the published-document semver rules -
    ruled 2026-10-24.** "Non-standing entry (Recommended)": the rules of
    5.117 and 5.146 for the published documents go to a new non-standing
    entry of the `cli` area; the rewrite of the frozen entry that 5.117
    and 5.146(3) put at this review does not happen, because 220 and 221
    retire that entry (HR-238). Source: owner, 2026-10-24.
237. **Batch 37: how the published schemas are made - ruled 2026-10-24.**
    "Generated with schemars": the schemas are derived with `schemars`
    from Rust types, so the output code of `audit`, `for`, and the new
    documents moves to typed structs (a refactor that folds into the spec,
    `quality.well-maintained-libraries`), not hand-written files with a
    `jsonschema` conformance test. Source: owner, 2026-10-24.
238. **Batch 37: the YAML reader of the CI workflow copies - ruled
    2026-10-24.** "saphyr": `install/ci_workflow.rs` reads a kept workflow
    with the `saphyr` parser instead of its text tests (HR-243); houserules
    still writes no YAML. Source: owner, 2026-10-24.
239. **Batch 37: the rollout of the bundle check - ruled 2026-10-24.**
    "Fail from the start (Recommended)": the seeded workflow's bundle
    check fails a PR without a bundle from the first `update`; the upgrade
    guide and `update`'s output state the red and its remedy (`bundle
    write`, or the bot in `exemptAuthors`). Source: owner, 2026-10-24.
240. **Batch 37: the spec approval - ruled 2026-10-24.** "I approve the
    spec": after the design-review loop, the owner reads the final spec
    and approves it before the plan starts (not the rule of 208). The
    merge mode is not ruled yet; the controller asks before the merge.
    Source: owner, 2026-10-24.
241. **Batch 37: the post-merge sample under a fast-forward merge - ruled
    2026-10-24.** To the second design review's QO-1: "State the limit
    (Recommended)". Under a fast-forward merge the record's key is the
    author's own head commit, so the draw is known before the merge; the
    spec, the packet's documentation, and the migrating-knowledge skill
    state that the sample is unsteerable only under a host merge that
    writes a new commit (squash, merge commit, or rebase on the host). No
    salt in 2.0.0. Source: owner, 2026-10-24.
242. **Batch 37: the route's trust boundary - ruled 2026-10-24.** To the
    second design review's QO-2: "Separate tool-free job". The seeded
    workflow computes the route's tool-free floors (rule sources, the
    configured area and path routes, paths in no area, and the diff size)
    in a job that runs no project tool, and the route that the report job
    applies is at least that floor; the report job accepts only the three
    route values. Source: owner, 2026-10-24.
243. **Batch 37: one report for a change, whatever the spec workflow -
    ruled 2026-10-24.** The owner asked before approving the batch-37
    spec: "if we would start using openspec, does the review workflow
    properly integrate with it? ... That should not yield two different
    reports at the end. Houserules should enable every kind of workflow
    to integrate and produce one coherent report at the end." To the
    question when the packet and the record get the section a spec
    adapter fills: "Contract now, OpenSpec in 38, and the contract must
    also allow to use github spec-kit". Batch 37 adds an `intent` section
    to the evidence packet, the change record, and the bundle, and a
    spec-adapter contract that a plan file or a work item fills; batch 38's
    OpenSpec adapter (HR-209, with HR-190) fills it from the change folder;
    the contract must fit GitHub Spec Kit's feature folders too, so that
    HR-210 adds an adapter and no schema change. Source: owner, 2026-10-24.
244. **Batch 37: GitLab is seeded too - ruled 2026-10-24, reverses D11
    of 5.96.** The owner asked before approving the batch-37 spec: "what
    we do should work independent of whether someone is on github or
    gitlab (managed or self-hosted). Have we made sure, that gitlab users
    are properly supported and that we have no hard-wiring to github for
    our adopters?" To the question how far batch 37 supports GitLab:
    "Seed GitLab CI too (Recommended)". The kit seeds a GitLab form beside
    the GitHub one: a `.gitlab-ci.yml` include with the same jobs, a
    merge-request template, and the host taken from the origin URL or
    from `init --host`, so a self-managed GitHub or GitLab instance is
    covered too; the posting jobs use a project access token that the
    adopter creates; a live proof runs in a private scratch GitLab project
    that the owner grants at the live run. The texts become host-neutral
    (pull or merge request), and no seeded file or command hard-codes
    `github.com`. Source: owner, 2026-10-24.
245. **Batch 37: CI wiring is opt-in and minimally invasive - ruled
    2026-10-24, revises 239, 244, and D3's default seeding.** The owner:
    "I am thinking about the case when projects have their own github
    actions or gitlab ci setup pre-existing. Most projects won't want a
    tool to write in their CI config. We need to be as minimally invasive
    as possible. What do you propose?" To the proposal: "Yes, as proposed
    (Recommended)". (1) houserules never edits a CI file it did not
    create, and writes none unless asked. (2) This repository publishes
    two versioned units with each release: a GitHub reusable workflow and
    a GitLab file for `include: remote`, each holding the gates, floor,
    report, record, and record-push jobs; an adopter references one with
    a pinned version. (3) `houserules ci print --host github|gitlab`
    prints the snippet or the jobs for an existing pipeline and writes
    nothing; `houserules ci write` creates a new file of its own on
    request and never touches an existing one. (4) `init` writes no CI
    file by default and prints one line on how to wire CI. (5) An
    existing install keeps its seeded `knowledge.yml`, which `update`
    keeps maintaining for the gates; `update` does not add the review
    layer to it and prints once how to opt in. 239's bundle check fails
    from the start for a project that opted in. 244's GitLab support is
    the published include, `ci print`, and `ci write`, not a seeded
    `.gitlab-ci.yml`. Nothing is lost (5.100): each function stays, only
    the defaults change. Source: owner, 2026-10-24.
246. **Batch 37: three answers to the third design review - ruled
    2026-10-24.** (1) QR-1, "Personal token fallback (Recommended)": on
    gitlab.com, where project access tokens need Premium or Ultimate, a
    project may give `HOUSERULES_TOKEN` a personal access token of a
    dedicated member with the Developer role; a project access token
    stays the preferred form, and the skill and the upgrade guide state
    that a personal token reaches every project its user can reach.
    (2) QR-2, "Only on request (Recommended)": the pull or merge request
    template is written only by `houserules ci write --template`; `init`
    and `update` never write one (this narrows HR-179's seed-once template
    and 244's seeded merge-request template under 245). (3) QR-3, "Accept
    (Recommended)": no release before 2.0.0 has `route --no-audit`, so
    batch 37's own pull request shows the floor's mechanics only, and this
    repository proves 242's floor from the 2.0.0 release (batch 38); from
    then on the floor job installs a released binary or builds the default
    branch's commit, never the request's, and a missing floor means at
    least `owner-review`. Source: owner, 2026-10-24.
247. **Batch 37: two answers to the fourth design review - ruled
    2026-10-24.** (1) QS-1, "Allow, with condition (Recommended)": the
    floor job may also take a binary that the runner image provides
    (`floor-install: runner:<path>`), an extension of 246(3), only where
    the runner is fresh per job (a GitHub-hosted runner, a GitLab docker
    executor) or the runner user cannot write the path; the spec, the
    migrating-knowledge skill, and the upgrade guide state the condition.
    (2) QS-2, "Required --sha flag (Recommended)": `houserules ci write
    --host github --mode reference` requires `--sha <release commit sha>`,
    which the `git ls-remote` line that `ci print` prints resolves; every
    command stays host-free. Source: owner, 2026-10-24.
248. **Batch 37: spec revision 6 approved - ruled 2026-10-24.** The owner was
    asked to approve spec revision 6, including the correction of the
    freeze entry's command count from 20 to 29, and answered "I approve
    the spec (Recommended)". Under 240 the plan starts
    from `docs/specs/2026-10-24-batch-37-per-change-review-layer.md` at
    revision 6. The approval covers the fact correction of
    `houserules.1-0-surface-is-frozen`: its body and its test both name
    29 commands. Source: owner, 2026-10-24.
249. **Batch 37: a JSON Schema validator for the tests - ruled 2026-10-24.**
    At plan time: cases 1, 2, 5, and 6 validate each published document
    against its generated schema, the in-crate validator of
    `rules/check.rs` lacks `anyOf`, `oneOf`, and `const`, which schemars
    emits, and spec 12 named no validator. The owner, to "Which validator
    should the tests use?": "jsonschema dev-dep (Recommended)". The
    `jsonschema` crate is a dev-dependency only (it does not ship in the
    binary), added with `cargo add --dev jsonschema@=<version>`, vetted
    and pinned exactly in the task that first needs it. Source: owner,
    2026-10-24.
250. **Batch 37: a base64 encoder for `ci print` - ruled 2026-10-24.** At
    plan time: `ci print` prints the GitLab include's `integrity` as
    `sha256-<base64>` (spec 10.2), and spec 12 named no encoder. The
    owner, to "Which should T12 use?": "base64 crate, locked
    (Recommended)". `base64` becomes a direct dependency at the version
    that `Cargo.lock` already holds through other crates (0.23.1 at plan
    time), so no crate enters the lockfile; T12 vets it. Source: owner,
    2026-10-24.
251. **Batch 37: the CI units live outside the kit payload - ruled
    2026-10-24.** The plan review (QP-1, `plan-review.json` P1) found that
    release-please's bump of the units' version lines under `template/ci/`
    changes `template/**` without regenerating `payload.stamp`, so the
    payload-stamp gate would fail every release PR
    (`plan-review-probe-stamp.txt`). The owner, to "Where should the units
    live?": "Root ci/ dir (Recommended)". The unit sources are
    `ci/github-review.yml` and `ci/gitlab-review.yml` at the repository
    root, embedded with `include_bytes!`; the GitLab include URL is
    `https://raw.githubusercontent.com/jblossey/houserules/v<version>/ci/gitlab-review.yml`.
    Spec 10.2 and every path of the units are amended. Source: owner,
    2026-10-24.
252. **Batch 37: the Zlib license is allowed - ruled 2026-10-24.** The plan
    review (QP-2, `plan-review.json` P3) found that `jsonschema` and
    `saphyr` both bring `foldhash` 0.2.0, licensed Zlib, which `deny.toml`
    does not allow (`plan-review-probe-deny.txt`,
    `plan-review-probe-deny-saphyr.txt`). The owner, to "Allow Zlib?":
    "Allow Zlib (Recommended)". T1 adds "Zlib" to `deny.toml`'s allow list
    with a comment that states why, in the form of the MPL-2.0 comment.
    Source: owner, 2026-10-24.
253. **Batch 37: the GitHub unit takes its version as an input - ruled
    2026-10-25.** The second plan review (`plan-review-r1.json` P27, P32,
    QP-3, QP-4) found that release-please's token cannot change a workflow
    file (the default token has no Workflows permission, and
    `RELEASE_PLEASE_TOKEN` is scoped to Contents, Pull requests, and
    Issues), and that Dependabot's action-pin updates of
    `.github/workflows/houserules-review.yml` would break its byte-identical
    copy under `ci/` every week. The owner, to "Which form should the
    GitHub unit take?": "Version as input (Recommended)".
    `.github/workflows/houserules-review.yml` is the GitHub unit's only
    source: no copy under `ci/` and no release version line. The caller
    passes `version:` beside its `@<sha> # v<version>` line, and `ci print`
    and `ci write` fill both. The unit holds a minimum version, raised by
    hand, and refuses an older binary with one line. The GitLab unit keeps
    `ci/gitlab-review.yml` and its release-bumped version line. This
    narrows 251 for GitHub. Source: owner, 2026-10-25.
254. **Batch 37: two inactive lock-only crates accepted - ruled
    2026-10-25.** The T1 task review (`task-1-review.json` issue 4) found
    two crates that the test-only `jsonschema` (249) brings with no recent
    activity: `num-cmp` 0.1.0 (one release, 2017; last push 2020) and
    `version_check` 0.9.5 (last release and push July 2024). Neither
    reaches the shipped binary (`cargo tree -e normal --target all`). The
    owner, to "Accept them?": "Accept as finished (Recommended)". Both
    enter as finished, test-only crates under 249; T1's vetting rows cite
    this ruling; the check is repeated if `jsonschema` drops or replaces
    them. Source: owner, 2026-10-25.
255. **Batch 37: a local `.git/info/attributes` is a named input - ruled
    2026-10-25.** The T4 task review (`task-4-review.json` issue 4) found
    that no git option turns off `$GIT_DIR/info/attributes`: neither
    `--attr-source` nor an empty `core.attributesFile` stops it, so a local
    `*.rs -diff` line there turns a text path binary in the diff of spec
    4.2 (`review4-git-config-probe-output.txt`). The owner, to "How should
    the review commands treat it?": "Named input (Recommended)". Spec 4.2
    names the repository's own `.git/info/attributes` as an input of a
    local run; a CI clone holds none, so CI stays the authority. No command
    refuses it. Source: owner, 2026-10-25.
256. **Batch 37: arraydeque accepted under saphyr - ruled 2026-10-25.**
    The T13 task review (`task-13-review.json` issue 4) found that
    `saphyr-parser` 0.2.2, which `saphyr` (238) needs, requires
    `arraydeque` 0.5.1 as a normal dependency, so it ships in the binary;
    its last release is from 2023-02-05 (`review13-probe-deps.txt`). The
    owner, to "Accept it?": "Accept as finished (Recommended)".
    `arraydeque` enters as a finished, single-purpose crate under 238;
    T13's vetting row cites this ruling; the check is repeated if `saphyr`
    drops or replaces it. Source: owner, 2026-10-25.
257. **Batch 37: no default route of auto-merge-eligible - ruled
    2026-10-25.** The T6 task review (`task-6-review.json`, spec item 4)
    found that a `review.route.default` of `auto-merge-eligible` makes a
    change in an area with no route of its own, or in no area,
    auto-merge-eligible with no limit: a 3,000-line change with no covering
    check routed `auto-merge-eligible` (`t6-review-probes-out.txt` P2). The
    owner, to "How should the default work?": "Refuse it (Recommended)".
    `check-knowledge` fails a `review.route.default` or `review.route.warn`
    of `auto-merge-eligible`, so a change is auto-merge-eligible only
    through an area or a path entry that names both limits (spec 6.1).
    Source: owner, 2026-10-25.
258. **Batch 37: the install key maps every other character to `-` -
    ruled 2026-10-25.** The T11 task review (`task-11-review.json` issue 5)
    found that spec 10.1's key (the install path with each `/` replaced by
    `-`) can hold a dot, which a GitLab environment name refuses, so the
    GitLab unit's key input refuses the spec's key of an install such as
    `packages/app.v2`. The owner, to "How should the key be formed?": "Map
    to - and refuse clashes (Recommended)". The key is `root` for a
    top-level install, else the install path with each character outside
    `A-Za-z0-9_-` replaced by `-`. `ci print` and `ci write` refuse an
    install whose key equals the key of another install of the repository,
    with one line that names both paths. Spec 10.1 and the T12 interface
    are amended. Source: owner, 2026-10-25.
259. **Batch 37: a force push to the default branch writes no record -
    ruled 2026-10-25.** The T11 task review (`task-11-review.json` issue 8)
    found that both record jobs fall back to git's empty tree when the
    push's `before` commit is not in the clone (a force push), so they
    write a record over the whole history; spec 10.1 names the empty tree
    only for a `before` of all zeros. The owner, to "What should a force
    push record?": "No record, a notice (Recommended)". The record job
    prints one notice that names the missing `before` commit and writes no
    record for that push. Spec 10.1 is amended. Source: owner, 2026-10-25.
260. **A fix-diff review for a critical task - ruled 2026-10-25, amends
    173(4).** The controller found that a critical task's fix round got no
    review of its own diff: the controller's check proves each named
    finding fixed, but not that the fix broke nothing new, and the one
    re-review of this batch (T5, standard) found three new problems. The
    owner, to the controller's recommendation: "recommendation approved.
    apply it". (1) A critical task still gets one task review and no
    re-review of its original findings. (2) A fix round that fixes a
    Critical or Important finding, or that changes files beyond the files
    its findings name, then gets one task-reviewer review scoped to its
    fix diff, which files only new breakage. (3) Only a new Critical or
    Important finding of that review opens a further round; that round
    closes by the controller's check, with no second fix-diff review.
    (4) This repository applies it from the T12 fix round of batch 37,
    through overrides of `process.risk-tiers` and
    `process.bounded-review-loop` in `knowledge/overrides.json`. Source:
    owner, 2026-10-25.
261. **The kit ships ruling 260 in 2.0.0 - ruled 2026-10-25.** The
    controller asked whether the kit ships ruling 260 to adopters too.
    The owner, to "Should the kit ship it to adopters too?": "Fold into
    T17 (Recommended)". (1) Task 17 of batch 37, which edits the process
    pack, changes `process.risk-tiers` and `process.bounded-review-loop`
    in `template/packs/process.json` to the text of the two overrides in
    `knowledge/overrides.json`. (2) At T17's integration the controller
    runs `houserules update --dir .` and drops the two overrides, so this
    repository reads the shipped text. Source: owner, 2026-10-25.
262. **Batch 37: Pages, the GitLab proof, and the merge mode are
    delegated - ruled 2026-10-25, answers Q2 to Q4 of the batch-37 spec.**
    The owner, verbatim: "We can't setup a private gitlab project so we
    need to rely on what we have for that one. Can you enable github pages
    with my gh cli? Then i would delegate those three things to you." (1)
    Q2: the controller enabled GitHub Pages on `jblossey/houserules` with
    `gh api -X POST repos/jblossey/houserules/pages -f build_type=workflow`
    (source "GitHub Actions"; the `github-pages` environment deploys from
    `main`). Controller step 7 is done. (2) Q3: no gitlab.com project.
    Case 14's live run does not run in batch 37 (263); the GitLab unit rests on
    its local evidence (the `ci_units` and `ci_command` tests: the parsed
    unit and every trust rule of 10.1, the shell sandbox runs of the
    unit's functions, the pinned image's tools, and the GitLab forms of
    `ci print` and `ci write`). HR-253 holds the live run, with what only
    it can show. (3) Q4: the controller's recommendation stands: ff-only
    from the CLI after the planned lifecycle and green CI, no release.
    (4) The acceptance of the batch stays with the owner. Source: owner,
    2026-10-25.
263. **No live CI proofs until 2.0.0 ships; ground every step on current
    sources - ruled 2026-10-25.** The owner, verbatim: "I want you to skip
    the live CI proofs for now. Make sure you ground your implementations,
    specs and plans on current docs and knowledge." and "At least until v2
    is shipped. After that, we can pick the live proofs back up." (1) No
    live run on a CI host is a proof or a gate until 2.0.0 is released:
    case 13 (GitHub) and case 14 (GitLab) of the batch-37 spec do not run,
    and no later batch of 2.0.0 adds one. A pull request's own CI still
    runs and still gates its merge; the review unit's jobs on batch 37's
    pull request are not captured as case 13. The local live run
    (`process.live-run-before-ci`) still runs. (2) HR-253 holds both
    proofs and the live items of the T11 fix round; its trigger is the
    2.0.0 release. (3) Every implementation, spec, and plan is grounded on
    the current docs of each library, tool, and framework and on the
    current knowledge entries: the override of
    `security-hygiene.verify-current-docs` in `knowledge/overrides.json`
    states it, and every brief carries it. Source: owner, 2026-10-25.
264. **Batch 37's acceptance is delegated - ruled 2026-10-25, amends
    262(4).** The owner, verbatim: "I also want to delegate the Batch 37
    acceptance to you." (1) The controller accepts batch 37 after its
    merge and rollout, by the batch's acceptance cases and its gates, and
    writes the acceptance record that rides the next branch. (2) The
    release of 2.0.0 stays with the owner (ruling 204). Source: owner,
    2026-10-25.
265. **Batch 38 starts its spec now; the owner answers its questions,
    then the controller takes over - ruled 2026-10-25.** The owner,
    verbatim: "I would like to start batch-38 spec now. I will answer the
    spec's open questions. When we have all answers in and have arrived at
    a properly reviewed spec that needs no re-reviews anymore, you can take
    over as suggested." (1) The controller drafts the batch-38 spec now,
    beside batch 37's development. (2) The owner answers every open
    question of the spec, library choices included. (3) Once every answer
    is in and the design review loop has closed (a review with no Critical
    finding, its Important and Minor findings folded in, no further review
    due), the controller takes over: it approves the spec, rules what the
    spec leaves to it, and accepts batch 38 after its merge and rollout.
    (4) A meta-spec change, the reversal of an owner ruling, the release of
    2.0.0, and the public issue comments of HR-212 stay with the owner
    (ruling 204, `process.owner-rulings-need-owner-supersession`). Source:
    owner, 2026-10-25.
266. **OpenSpec compatibility: an adapter and a documented hookup -
    ruled 2026-10-25 (HR-209, batch-38 spec Q1).** The owner chose
    "Adapter + documented hookup (Recommended)". (1) `evidence`, `record`,
    `route`, and `bundle check` read an OpenSpec change through an
    `openspec` intent adapter (title, why, tasks, requirement deltas) and
    an `openspec validate --json` output file that a caller passes; no
    command runs OpenSpec (5.116, C13). (2) The README and the docs show
    the `openspec/config.yaml` lines that put the houserules step into
    OpenSpec's apply and archive phases; houserules never edits that file.
    (3) This supersedes 184(2)'s "no recipe for another workflow ships"
    for that documented OpenSpec hookup only; no other workflow gets a
    recipe. Source: owner, 2026-10-25. Controller correction, the same
    day: the option text named `route` among the readers, but `route`
    takes no intent input (batch-37 spec 6.2); the adapter reaches
    `evidence`, `record`, and `bundle check`. The owner's choice is
    unchanged.
267. **Batch 38's open questions Q2 to Q5 - ruled 2026-10-25.** The owner
    chose the recommendation of each. (1) Q2: issue 70's two optional rules
    are two check types of the knowledge schema, `intent-required` and
    `intent-resolves`, that `audit` evaluates. (2) Q3: the session trailers
    are a configurable list, `review.sessions.trailers`, default
    `["Entire-Checkpoint"]`. (3) Q4: the OpenSpec adapter reads Markdown by
    OpenSpec 1.15.0's own patterns through `regress`; no Markdown library
    is added. (4) Q5: HR-217 leaves batch 38 and runs with HR-253's
    post-release proofs (263). With Q1 (266), every open question of the
    batch-38 spec is answered. Source: owner, 2026-10-25.
268. **The controller may run OpenSpec's binary - ruled 2026-10-25.** The
    owner, verbatim: "If you need the openspec binary i grant you
    permission to run it." (1) The controller may download OpenSpec's
    released standalone binary, check it against the release's
    `SHA256SUMS`, and run it in scratch folders under `~/.cache` for the
    batch-38 probes, with telemetry off (`OPENSPEC_TELEMETRY=0`,
    `DO_NOT_TRACK=1`, `CI=1`) and its own `HOME`. (2) No test and no gate
    of houserules depends on it (5.116, C13). Source: owner, 2026-10-25.
269. **This repository moves to OpenSpec and drops Superpowers (HR-239) -
    ruled 2026-10-25.** The owner answered "Yes" to the controller's
    question whether HR-239 should record that this repository migrates
    to OpenSpec and drops Superpowers. (1) HR-239's open question "whether
    this repository migrates too" is answered: it does, and this
    repository stops using the Superpowers plugin's skills. (2) HR-239
    stays a 2.x item that needs its brainstorm and spec; this ruling sets
    their starting point and schedules nothing. Source: owner, 2026-10-25.
270. **An agent learns the split of OpenSpec and houserules; two
    planning workflows draw a warning - ruled 2026-10-25 (batch-38 spec,
    extends 266).** The controller found that nothing tells an agent in a
    project with both tools who does what, and that houserules' reference
    workflow and OpenSpec are two rival planning lifecycles. The owner
    chose the recommendation of each question. (1) "Yes, rendered when
    openspec/ exists": `render` adds a short OpenSpec section to
    houserules' own block of `AGENTS.md` when the install's tree holds an
    OpenSpec root: OpenSpec plans changes in `openspec/changes/`, a design
    decision goes to the change's `design.md`, and houserules supplies
    the rules and checks. houserules edits only its own block. (2) "Warn
    and advise core-only": `init --workflow reference` and
    `check-knowledge` print one warning line when an install runs the
    reference workflow and the project holds an OpenSpec root; nothing is
    blocked; the docs advise houserules without the reference workflow
    beside OpenSpec until HR-239. Source: owner, 2026-10-25.
271. **The upgrade proof shows what 2.0.0 does; its merge-base half goes
    to HR-193 - ruled 2026-10-25, amends meta-spec 7 and 5.151(4).** The
    batch-38 design review (spec-review.json F12) found that meta-spec 7
    asks the proof to audit an upgrade against the converted 1.x
    merge-base rules, which is HR-193's in-memory conversion, moved to 2.x
    by 192. The owner chose "What 2.0.0 does; rest to HR-193
    (Recommended)". (1) The batch-38 proof shows, per 1.x release: `update`,
    every gate, `audit` against the head's rules, D4's line on a 1.x head,
    and `route` senior-review for the rule-source change. (2) The proof
    against converted merge-base rules joins HR-193 (2.x). (3) Meta-spec 7
    carries this amendment by the owner's ruling. Source: owner,
    2026-10-25.
272. **No HR-190 seed rule in 2.0.0 - ruled 2026-10-25, amends meta-spec
    section 4.** The batch-38 design review (F26) found that meta-spec 4
    plans an HR-190 seed rule while the spec ships the two intent checks
    in no pack. The owner chose "No seed rule, docs example
    (Recommended)". (1) No pack entry uses `intent-required` or
    `intent-resolves`; a project opts in with its own entry. (2)
    `docs/openspec.md` and the migrating-knowledge skill show an example
    entry for each check. (3) Meta-spec 4's HR-190 seed rule is dropped by
    this ruling. Source: owner, 2026-10-25.
273. **The batch-38 spec is approved by the controller - 2026-10-25, under
    265.** Every open question is ruled (266, 267, 270, 271, 272); the
    design review (`spec-review.json`: 0 Critical, 17 Important, 11 Minor)
    is folded into revision 2 of
    `docs/specs/2026-10-25-batch-38-openspec-and-release.md`, and the loop
    is closed (no Critical finding). By 265(3) the controller approves
    revision 2 and plans from it. Source: controller, by the owner's
    delegation of 265.
274. **A second design review of the batch-38 spec - ruled 2026-10-25,
    suspends 273.** The owner, verbatim: "That's a lot of important
    findings. I want you to do another spec review round." (1) A fresh
    `spec-reviewer` reviews revision 2 whole (`spec-review-r1.json`), with
    the first review as `PRIOR`. (2) The controller's approval of 273 waits
    until that round's findings are folded in and the loop closes by the
    designing skill (no Critical finding); 265's handover applies then.
    Source: owner, 2026-10-25.
275. **Store-pointer OpenSpec roots, and the end of the batch-38 review
    loop - ruled 2026-10-26.** The second design review
    (`spec-review-r1.json`: 0 Critical, 11 Important, 14 Minor) asked
    what an install whose `openspec/` holds only a store pointer gets
    (R8). The owner chose "Warning yes, AGENTS.md note no (Recommended)":
    (1) the two-workflow warning of 270(2) fires for a store-pointer root
    too; (2) the `AGENTS.md` section of 270(1) is not rendered for it,
    because its text would be false there, and `docs/openspec.md` says so.
    To "should a third reviewer check the spec before I approve it?", the
    owner chose "No, fold and approve": (3) the controller folds the
    findings of `spec-review-r1.json` into revision 3 and approves it
    under 265, which ends 274's suspension of 273. Source: owner,
    2026-10-26.
276. **Batch 38 merges as batch 37 does - ruled 2026-10-26.** The owner,
    verbatim: "yes, same merge mode for batch 38", to the controller's
    question whether batch 38 merges ff-only from the CLI after green CI,
    with no release (262(3)). (1) The controller merges batch 38 by that
    mode without a further owner word. (2) The README review (HR-223 (5)),
    the 2.0.0 release, and the posting of HR-212's comments stay with the
    owner. Source: owner, 2026-10-26.
277. **The README review is the last gate before the release - ruled
    2026-10-26.** The owner, verbatim: "Make sure after the readme review
    we are release-ready." (1) Batch 38 orders its work so that the
    owner's README review (HR-223 (5)) is its last open step: every task,
    the upgrade proof, the live run, the branch review and its fixes, the
    knowledge and backlog close, the drafts of HR-212, and the draft
    release note are done, and the batch-38 pull request is open with
    green CI, before the owner reviews the README on that pull request.
    (2) After the review, the controller applies the owner's changes,
    waits for green CI, merges (276), checks the post-merge state of
    batch-38 spec section 18, and reports the release ready: the owner
    then only cuts the release (merges release PR #83) and posts the
    HR-212 comments. Source: owner, 2026-10-26.
