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
   evals, CI workflow, CLAUDE.md, settings) belong to the project from the
   first write. `houserules files` prints the split; `.houserules.json` records
   the installed version.

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
