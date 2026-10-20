# houserules

[![CI](https://github.com/jblossey/houserules/actions/workflows/ci.yml/badge.svg)](https://github.com/jblossey/houserules/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jblossey/houserules?label=release)](https://github.com/jblossey/houserules/releases)

Knowledge base, backlog, and agent workflow for repositories built with
AI coding agents.

houserules gives a repository developed with AI coding agents a knowledge
base, a backlog, and an agent workflow that keeps both current. Point
Claude Code (or any harness that reads plain files) at your project;
houserules installs the machinery, and your project supplies the rules.

Every rule, decision, and gotcha your team has learned lives in
`knowledge/*.json`, addressable by id. Every unit of work traces to a
backlog item. Three agent templates carry the rules into every change and
audit the result against them, so the rules stay enforced instead of
drifting into a wiki nobody reads.

## Table of contents

- [Why](#why)
- [Install](#install)
- [Quick start](#quick-start)
- [Adding to an existing project](#adding-to-an-existing-project)
- [What it installs](#what-it-installs)
- [Packs](#packs)
- [Ownership model](#ownership-model)
- [The agent workflow](#the-agent-workflow)
- [Measure process cost](#measure-process-cost)
- [CLI reference](#cli-reference)
- [Stability](#stability)
- [Contributing and support](#contributing-and-support)
- [License](#license)

## Why

Agent sessions forget; repositories don't. houserules turns what agents
learn in your codebase — the pitfalls, conventions, and decisions — into
addressable rules that load into every future session. Each rule is
targeted and audited, so the set stays small and cheap in tokens: agents
stop repeating known mistakes without re-reading history, and review
rounds shrink. Once the recurring pitfalls are captured, you add your own
rules and the same machinery enforces them — the codebase converges on
the style you rule, not the style that happens.

## Install

houserules ships one dependency-free binary: no local clone, no Rust
toolchain, no Node. Install it through any of these channels, then run
`houserules init` in your project.

### The shell installer

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/jblossey/houserules/releases/latest/download/houserules-installer.sh | sh
```

The installer picks your platform's archive, verifies its sha256, and
installs to `~/.local/bin`, adding that directory to `PATH` itself. The
URL always resolves to the newest release — it never needs a version
edit.

### mise

```sh
mise use github:jblossey/houserules
```

Resolves the newest release's archive for your platform through
[mise](https://mise.jdx.dev)'s `github` backend and records the tool in
your `mise.toml`.

### Direct download

| Target | Archive |
|---|---|
| Apple Silicon macOS | [houserules-aarch64-apple-darwin.tar.xz](https://github.com/jblossey/houserules/releases/latest/download/houserules-aarch64-apple-darwin.tar.xz) |
| Intel macOS | [houserules-x86_64-apple-darwin.tar.xz](https://github.com/jblossey/houserules/releases/latest/download/houserules-x86_64-apple-darwin.tar.xz) |
| x64 Windows | [houserules-x86_64-pc-windows-msvc.zip](https://github.com/jblossey/houserules/releases/latest/download/houserules-x86_64-pc-windows-msvc.zip) |
| ARM64 Linux (musl) | [houserules-aarch64-unknown-linux-musl.tar.xz](https://github.com/jblossey/houserules/releases/latest/download/houserules-aarch64-unknown-linux-musl.tar.xz) |
| x64 Linux (musl) | [houserules-x86_64-unknown-linux-musl.tar.xz](https://github.com/jblossey/houserules/releases/latest/download/houserules-x86_64-unknown-linux-musl.tar.xz) |

Each archive has a `.sha256` checksum beside it (append `.sha256` to the
archive URL). Extract the archive and put the `houserules` binary on
`PATH`. An exact older version stays downloadable from its own release
page.

macOS binaries are unsigned. Clear the quarantine flag once:

```sh
xattr -d com.apple.quarantine /path/to/houserules
```

Or use the GUI path: run the binary once and let macOS block it, then
open System Settings, click Privacy & Security, scroll to the
blocked-software notice, and click Open Anyway.

### Updating

```sh
houserules update
```

Installed through the shell installer and run from where it installed,
`update` checks GitHub for a newer release, replaces the binary itself,
and re-runs before touching your repository — nothing else to install
by hand. Run from anywhere else — mise, a direct download, or a
shell-installer copy invoked from a different location — it prints one
line naming what to do instead, and updates your repository on the
binary you already have.

Either way, the repository sync writes the files houserules owns —
keeping any you changed — and re-renders the generated files. It moves
each kit pack to the version of the binary and keeps your overrides and
exclusions. When a pack ships the id of one of your own entries, it moves that entry
into an override (one id, one source) and removes a topic file left empty. It prints the version
drift it just synced, `kit <old> -> <new>`. It also backfills any
seed-once file a later kit release adds that your install never
received: an install seeded before this release gets a fresh `AGENTS.md`
this way (`wrote AGENTS.md`) — generic, and yours to fill in with the
rules your own `CLAUDE.md` already carries; the `migrating-knowledge`
skill walks that merge. Review the diff (`git diff`) afterward; the
drift line confirms the version you moved from and the version you
landed on. Whenever the binary check completes, it also warns if another
`houserules` sits elsewhere on your `PATH` — not the copy it just
checked, updated or not.

An install of houserules 1.x converts to the 2.0 layout in the same run.
[docs/upgrading-to-2.md](docs/upgrading-to-2.md) lists what changes and
each hand step that `update` can print.

The knowledge schema, `knowledge/schema.json`, and the CI workflow,
`.github/workflows/knowledge.yml`, are a kind of file that `houserules
files` lists as `d3Files`. `init` writes each once and records a
baseline for it. `update` keeps each current through that baseline: it
rewrites a copy that still equals its baseline, keeps and lists a copy
that you changed, and leaves a copy that you list in `ownedPaths` as it
is. The workflow installs the release that `.houserules.json` records.
`update` replaces a workflow of houserules 1.x that equals the workflow
of a release. It keeps a workflow with steps of your own and prints a
hand step. Copy the install step from the kit's
[`template/.github/workflows/knowledge.yml`](https://github.com/jblossey/houserules/blob/main/template/.github/workflows/knowledge.yml)
into it, or compare the two files.

Set `HOUSERULES_SKIP_SELF_UPDATE` (to any value) to update only the
repository and skip the binary check for one run; `update` skips it
automatically in CI.

On an interactive terminal, every other command also checks, at most
once a day, whether a newer release exists, and names it in one line.
Set `CI` or `HOUSERULES_NO_UPDATE_CHECK` (to any value) to turn that off.

### Uninstalling

Remove the binary through the channel you installed with:

- **Shell installer or direct download**: delete the `houserules` binary
  from where it was installed — `~/.local/bin` for the shell installer,
  wherever you placed it for a direct download. An older shell-installer
  install sits in `~/.cargo/bin`.
- **mise**: run `mise uninstall houserules` and remove the tool's entry
  from your `mise.toml`.

Everything houserules put into a project is plain files tracked by your
repository — there is no hidden state. To remove the kit from a project,
delete the files it manages (`houserules files` lists them) plus the
seeded `knowledge/` and `backlog/` data, or keep the knowledge files:
they are readable JSON and useful without the binary.

## Quick start

```sh
mkdir my-project && cd my-project && git init
houserules init
houserules check-knowledge && houserules check-backlog
git add -A && git commit -m 'chore: install houserules knowledge setup'
```

`init` writes the kit-owned machinery, installs the five kit packs
(your starting rules), and seeds your backlog and schemas (plus a CI
workflow, on a GitHub-hosted origin). It then renders the harness files
and stamps `.houserules.json` with the binary's version. Look at what
you got:

```sh
houserules topics
```

```json
[
  { "topic": "knowledge-base",   "entries": 7,  "title": "Authoring knowledge entries" },
  { "topic": "process",          "entries": 38, "title": "How work runs: batches, dispatch, reviews, rulings" },
  { "topic": "quality",          "entries": 6,  "title": "Quality principles" },
  { "topic": "security-hygiene", "entries": 4,  "title": "Dependency, commit, and test hygiene" },
  { "topic": "writing-style",    "entries": 5,  "title": "Writing style for docs, comments, commits, reports" }
]
```

```sh
houserules get process.tdd   # one entry, in full
```

Set `git config core.hooksPath .githooks` to activate the commit-msg
Conventional Commits gate.

For Claude Code, add the Claude files:

```sh
houserules init --target claude
```

This seeds the agent templates, the session-start script and its hook
entry in `.claude/settings.json`, and enables the Claude renderer, which
writes the area rules under `.claude/rules/` and the `project-knowledge`
skill. Restart Claude Code once afterward (the first `.claude/agents/`
file and the new hook need a fresh session). Other harnesses need
nothing more: the standing rules sit in the `houserules` block of
`AGENTS.md`.

## Adding to an existing project

```sh
cd my-project
houserules init --id-prefix ABC
```

- `--id-prefix ABC` sets your backlog id prefix (`ABC-001`); default `WI`.
- An existing `AGENTS.md` keeps its text: `init` reports `kept
  AGENTS.md`. `render` then adds the `houserules` block at its end, between
  `<!-- houserules:begin -->` and `<!-- houserules:end -->`. Move the
  block where you want it; it stays there. Copy the `## Knowledge base`
  and `## Workflow` sections from the seeded template into yours by hand;
  the `migrating-knowledge` skill walks the merge. An existing `CLAUDE.md`
  is kept the same way — point it at your `AGENTS.md` with a single
  `@AGENTS.md` line if you use Claude Code. `check-knowledge` fails a
  `CLAUDE.md` that lacks the line.
- With `--target claude`, an existing `.claude/settings.json` is merged:
  the two SessionStart hook entries are appended only if their matchers
  are absent. Nothing else in your settings is touched.
- `.githooks/commit-msg` is kit-owned, but `init` keeps a hook of that
  name that your project already has, and says so (`kept
  .githooks/commit-msg`).
- Existing files under `knowledge/`, `backlog/`, evals, or the workflow
  are kept as they are.

Then move your real rules in: add your entries as topic files
`knowledge/<topic>.json` (an id starts with its topic name), add the
areas they use to `knowledge/areas.json` with their globs, replace the
example backlog item, and run `houserules render`. To change a rule of
the kit, override or exclude it in `knowledge/overrides.json` (see
[Packs](#packs)). The `migrating-knowledge` skill walks the move step by
step, from inventory to entries to gates — including eliciting your
project's own merge, attribution, and parallelism discipline, which
houserules leaves to your ruling instead of shipping fixed.

### A subdirectory install

A repository can hold the install in a subdirectory, for example a
service in a monorepo:

```sh
houserules init --dir services/api
```

- The install root is the directory you give. `.claude/` and
  `.githooks/` live in the install root, so launch Claude Code there.
- Every other command works from anywhere inside the install. Its default
  root is the nearest directory at or above the current one that holds
  `.houserules.json`, no higher than the git top level.
- Every path a gate matches is relative to the install root, and a
  changed path outside it is out of scope. `check-commit` checks only the
  commits that touch the install root.
- The CI workflow stays at the git top level, as
  `.github/workflows/knowledge-<folder>.yml`, where `<folder>` is the
  install root's path with each `/` replaced by `-` (`knowledge-services-api.yml`
  here). It carries a `working-directory` and a `paths` filter for the
  subdirectory. A repository can hold several installs, each with its own
  workflow; `init` refuses a root that nests with another install or
  reuses a workflow file name.
- houserules never sets `core.hooksPath`. Activate the commit-msg hook
  with `git config core.hooksPath services/api/.githooks`, or call it
  from your hook manager in one line: `sh services/api/.githooks/commit-msg
  "$1"`.

## What it installs

- **A knowledge base** (`knowledge/`): addressable entries (`id`,
  `kind`, `area`, `summary`, `body`, `tags`, `source`, `see`, `verify`,
  optional deterministic `check`). The kit's entries come as five packs
  in `knowledge/packs/`. Your own entries live in one JSON file per
  topic, `knowledge/<topic>.json`. Your changes to pack entries live in
  `knowledge/overrides.json`, and your areas in `knowledge/areas.json`.
  A schema validates the entries, and `update` keeps it current.
- **A backlog** (`backlog/`): typed work items, one JSON file per
  section, driving every change.
- **Project instructions** (`AGENTS.md`): the cross-harness instruction
  file every agent reads, plus a one-line `CLAUDE.md` (`@AGENTS.md`) that
  bridges Claude Code to the same text.
- **The `houserules` binary**: read commands print JSON;
  `check-knowledge`/`check-backlog` gate lint; `render` generates the
  markdown the harness loads; `audit` checks a git range against its
  rule package; `validate` checks agent deliverables.
- **Generated harness files** (`houserules render`): the standing rules
  and one section for each area in a marked block of `AGENTS.md`; the
  rules of an area whose globs share one directory in that directory's
  `AGENTS.md`; and, for Claude Code (`houserules init --target claude`),
  path-scoped area rules (`.claude/rules/<area>.md`) and a preloaded
  `project-knowledge` skill.
- **An agent layer**: an `orchestrating` skill, a `finishing-a-feature`
  skill, a `migrating-knowledge` skill, a commit-msg hook that gates
  Conventional Commits, a JSON deliverables contract, and eval
  scenarios. With `--target claude`, also three agent templates
  (`implementer`, `task-reviewer`, `branch-reviewer`) with rule-adherence
  audits, and a SessionStart hook.
- **Seed rules**: five packs with a generic standing-rule set (TDD,
  conventional commits, no tech debt, dependency vetting, exact pins,
  doc comments, ASD-STE100 writing style, and more). Your project adds
  its own topics, areas, and rules on top, and overrides or excludes a
  rule of the kit.
- **A CI gate** (`.github/workflows/knowledge.yml`), on a GitHub-hosted
  origin: `check-knowledge`, `check-backlog`, and a PR audit, each a
  `houserules` command. The gate first installs the release that
  `.houserules.json` records, so it runs the binary your kit files came
  from. It reads the version from the line that starts with two spaces
  and `"version": "<text>"`, the layout `houserules` writes. When
  `.houserules.json` is missing or has no such line (a minified or
  re-indented stamp, or a null version), the gate installs the latest
  release and prints a notice that says so; run `houserules update` to
  write the stamp, or to name the error in it. On other hosts `init`
  prints a skip note and the `migrating-knowledge` skill covers
  generating the equivalent gate.

## Packs

The kit's rules ship as five packs: `knowledge-base`, `process`,
`quality`, `security-hygiene`, and `writing-style`. A pack is a
versioned set of entries. `knowledge/packs/<pack>.json` holds one copy of
each installed pack, and `.houserules.json` pins each copy by version,
source, and checksum. Your own entries stay in your topic files.

```sh
houserules pack list                         # installed packs, and kit packs you lack
houserules pack add <name>                   # a kit pack
houserules pack add git+<url>@<tag>#<path>   # an upstream pack at an exact tag
houserules pack add <file>                   # a pack file inside your install
houserules pack upgrade <name> <tag>         # move an upstream pack to a new tag
houserules pack upgrade <name>               # copy a file pack again
houserules pack remove <name>                # remove a pack
```

An upstream pack comes through your own `git` command, so any host and
your git credentials work. The tag must be a tag, never a branch or a
commit: a pin is exact. `update` moves the kit packs to the version of
the binary, and `pack upgrade` moves an upstream pack.

Never edit a pack copy: `check-knowledge` fails a copy that differs from
its pin. Put your changes in `knowledge/overrides.json` instead:

```json
{
  "schema": "houserules/overrides@1.0.0",
  "overrides": [
    {
      "id": "quality.principles",
      "fields": { "standing": null }
    }
  ],
  "exclusions": [
    {
      "id": "security-hygiene.no-focused-tests",
      "reason": "This project has no test runner that supports focused tests."
    }
  ]
}
```

An override replaces whole fields of one pack entry, and a field set to
`null` removes it. An exclusion drops an entry. `check-knowledge` lists
both, and `get <id>` marks an overridden or excluded entry. `update` and
`pack upgrade` keep your overrides and exclusions; when a pack ships the
id of one of your own entries, they move that entry into an override.
An id has one source: `pack add`
refuses a pack that ships an id of another installed pack.

## Ownership model

Every path houserules writes falls into one of four classes, and
`houserules files` prints the exact manifest of each as JSON:

- **Kit-owned** (`kitOwned`) — `update` writes a missing file and
  rewrites one you did not change on every run. It keeps and reports a
  file you changed. Never hand-edit it.
- **Seed-once** (`seedOnce`) — `init` writes it only if it is absent,
  then leaves it alone. It is yours from the first write on.
  `.claude/settings.json` is the one exception: with `--target claude`,
  `init` merges its two SessionStart hook entries into an existing file
  instead of skipping it.
- **Kept current through a baseline** (`d3Files`) — the knowledge schema
  and the CI workflow. `init` writes each once and baselines it, and
  `update` keeps it at the content of the kit unless you changed it. The
  CI workflow is written only when your `origin` resolves to GitHub;
  `update` backfills it if your origin becomes GitHub-hosted later.
- **Pack copies** (`packCopies`) — `knowledge/packs/<pack>.json`.
  `update` moves each kit pack to the version of the binary and rewrites
  its copy. You change a pack entry through `knowledge/overrides.json`,
  never in the copy.
- **Yours** — everything else: your knowledge entries, your backlog
  items, your project code. houserules never touches it.

To keep a file for good, list its path in `ownedPaths` in
`.houserules.json`: `update` leaves a listed file as it finds it and
prints no line for it, `init` never seeds it, and `check-knowledge`
does not check a listed kit-owned file for citations. The files and
blocks that `render` generates follow the knowledge base, not
`ownedPaths`: `render` still writes the houserules block of a listed
`AGENTS.md`. To change a pack
entry, use `knowledge/overrides.json` (see [Packs](#packs)).

`.claude/rules/*.md` and `.claude/skills/project-knowledge/SKILL.md` are
generated by `houserules render` from `knowledge/`, and so is the
`houserules` block of each `AGENTS.md` that `render` writes. The next
`render` replaces a hand edit inside a generated file or block.

| Class | Files |
|---|---|
| kit-owned | `tools/claude-session-start.sh` and `.claude/agents/*.md` (both with `--target claude`), the `orchestrating`, `finishing-a-feature`, and `migrating-knowledge` skills in `.claude/skills/`, `.githooks/commit-msg` |
| seed-once | `knowledge/areas.json`; the `backlog/` schema and data; `.claude/schemas/deliverables.json` and the evals; `docs/README.md`, `AGENTS.md`, `CLAUDE.md` |
| kept current through a baseline | `knowledge/schema.json`, `.github/workflows/knowledge.yml` |
| pack copies | `knowledge/packs/<pack>.json`, one for each installed pack |

## The agent workflow

Three agent templates, installed with `--target claude` and each on a
different model tier, carry the rules into every change:

- **implementer** (the cheapest model that fits the task): implements
  one task from a brief, test-driven, and writes a JSON report.
- **task-reviewer** (a stronger model than the implementer it reviews):
  reviews one task's diff for spec compliance, code quality, and rule
  adherence.
- **branch-reviewer** (the strongest model): reviews the whole branch
  before merge and proposes knowledge-base improvements drawn from the
  batch's reviews.

An `orchestrating` skill drives the batch lifecycle — brainstorm or
spec, a decision gate from the project's owner or decider, plan,
dispatch, live run, finish, rollout. Every dispatched agent runs
`houserules audit` against its own diff and records the result in its
JSON report, so rule adherence is checked, not just asserted.

## Measure process cost

`houserules stats` reads the deliverables that agents write into a batch
workspace and prints one JSON document. Give it one or more workspace
directories:

```sh
houserules stats .superpowers/sdd/*/
```

With several workspaces, a task label reads `<workspace>/<task>`. The
output keeps the keys `violations`, `unused_ids`, `audits`, and
`reviews`. It adds these keys:

- `workspaces`: one row per workspace with its tasks, reviews, fix
  rounds, report bytes, and findings by severity and by target.
- `cost`: the sums of those rows and the mean per task.
- `rules`: one row per active knowledge entry. `source` names where the
  entry comes from: `local`, or the name of the pack that ships it.
  `workspaces` counts the workspaces whose audit or review rated it, and
  `injected` the workspaces whose audit injected it. `rows` counts the
  deterministic audit rows and the judged review rows that rated it, and
  `fails` those that failed. `findings` and `targets` count the findings
  that cite it, by severity and by target. `cited` counts the reports
  that cited it.
- `packs`: one row for `local` and one for each installed pack, with
  the sums of its source. `entries` counts its active entries and
  `standing` the standing ones. `judged_rows` counts the review rows that
  rated its judged entries. `violations` counts the tasks that
  `violations` lists for its entries.
- `proposals`: the changes the numbers support. Each row names an
  `action`, the entry `id`, the `reason`, and the `evidence`. The five
  actions are `demote`, `retire`, `mechanize`, `narrow`, and `budget`. A
  row with `"owner_gate": true` is for the project's owner to decide.
  `narrow` counts a finding as soft only when it is minor or hits a
  deliverable, so a finding on Markdown, `docs/`, `knowledge/`, or
  `backlog/` text counts as a contract finding. Tag an entry
  `ruled-keep` when you decide to keep it, and `stats` stops proposing
  `demote`, `retire`, `narrow`, or `mechanize` for it; the entry still
  counts toward the standing budget.

One `proposals` row, abbreviated:

```json
{
  "action": "narrow",
  "id": "process.claims-match-artifacts",
  "owner_gate": true,
  "reason": "4 of 4 findings are minor or hit a deliverable or prose. Narrow the rule to contract surfaces.",
  "evidence": {"findings": 4, "soft_findings": 4}
}
```

## CLI reference

Working with knowledge:

| Command | What it does |
|---|---|
| `topics` | Lists every knowledge topic with its entry count and title |
| `index` | Lists knowledge-entry index rows, optionally filtered |
| `get <id>` | Prints items by id: a backlog item, amendment, parked item, or knowledge entry |
| `for <path>` | Prints the rule package a set of changed paths pulls in |
| `standing` | Lists the standing rules |
| `render` | Rewrites stale generated harness files (`--check` lists them instead) |
| `pack <list\|add\|upgrade\|remove>` | Lists, adds, upgrades, and removes the knowledge packs of an install |

Working with the backlog:

| Command | What it does |
|---|---|
| `list` | Lists backlog items, optionally filtered |
| `batch <n>` | Prints one development batch's summary and item rows |
| `set <id> k=v` | Applies field assignments to a backlog item |
| `archive` | Sweeps done/dropped items, done batches, and retired or superseded entries of your topic files into `archive/` mirrors; a pack entry stays in its pack copy |

Gates:

| Command | What it does |
|---|---|
| `check-knowledge` | Validates the knowledge base, its packs against their pins, its overrides and exclusions, generated-file freshness, and that kit-owned files cite only entries it holds |
| `check-backlog` | Validates the backlog |
| `check-commit` | Runs commit-message checks against a message file or git range |
| `audit` | Builds a git range's rule package and runs every deterministic check; a task audit (`--report`) leaves out an entry tagged `controller` unless `--ids` names it |
| `validate` | Validates deliverable JSON files against the schema |
| `check-report-claims` | Cross-checks a report's claims against the artifacts it cites |
| `stats <workspace>...` | Aggregates rule violations, process cost, and knowledge-base proposals across one or more workspaces |

Installing:

| Command | What it does |
|---|---|
| `init` | Seeds the kit into a git repository from the embedded payload; `--target claude` adds the Claude Code files, and `--dir <subdir>` installs in a subdirectory |
| `update` | Syncs kit-owned files, moves the kit packs, converts an install of houserules 1.x, removes retired files, reports version drift |
| `files` | Prints the four file classes of the kit as JSON: kit-owned, seed-once, pack copies, and the files kept current through a baseline |

Run any command with `--help` for its flags.

## Stability

The CLI surface (the 21 subcommands above and their flags) and the
schema constraint surface are frozen: a change to either is a breaking
change and ships only with a major version bump.
Within a major version, `update` never rewrites your project's own data
files, except that it moves an entry whose id a pack ships into an
override (one id, one source). The full contract lives in [docs/design.md](docs/design.md).

## Contributing and support

Questions and bug reports go to
[GitHub issues](https://github.com/jblossey/houserules/issues). Pull
requests are welcome; [CONTRIBUTING.md](CONTRIBUTING.md) covers the
development workflow, the batch process, and the gates a change must
pass.

This repository runs its own kit (id prefix `HR`): `template/` is the
source of everything the binary installs, and the root `.claude/`,
`tools/`, and `.githooks/` are the installed copy. Edit `template/`,
then run `houserules update --dir .`; tests pin the copies to their
sources.

```sh
mise run setup    # hooks + the binary on PATH
mise run lint     # the full lint chain
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

## License

MIT — see [LICENSE](LICENSE). The files that `init` and `update` write
into your project are yours under the same terms; the kit-owned scripts
carry an SPDX header, so vendored copies keep the notice. Distribution
is binary-only: GitHub Releases, mise's `github:` backend, and the shell
installer.
