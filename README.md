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
- [Ownership model](#ownership-model)
- [The agent workflow](#the-agent-workflow)
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

Either way, the repository sync overwrites only kit-owned machinery and
re-renders — never project data — and prints the version drift it just
synced, `kit <old> -> <new>`. It also backfills any seed-once file a
later kit release adds that your install never received: an install
seeded before this release gets a fresh `AGENTS.md` this way (`wrote
AGENTS.md`) — generic, and yours to fill in with the rules your own
`CLAUDE.md` already carries; the `migrating-knowledge` skill walks that
merge. Review the diff (`git diff`) afterward; the drift line confirms
the version you moved from and the version you landed on. Whenever the
binary check completes, it also warns if another `houserules` sits
elsewhere on your `PATH` — not the copy it just checked, updated or not.

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

`init` writes the kit-owned machinery, seeds your starting knowledge
topics, backlog, and schemas (plus a CI workflow, on a GitHub-hosted
origin), then renders the harness files and stamps `.houserules.json`
with the binary's version. Look at what you got:

```sh
houserules topics
```

```json
[
  { "topic": "knowledge-base",   "entries": 4,  "title": "Authoring knowledge entries" },
  { "topic": "process",          "entries": 33, "title": "How work runs: batches, dispatch, reviews, rulings" },
  { "topic": "quality",          "entries": 6,  "title": "Quality principles" },
  { "topic": "security-hygiene", "entries": 4,  "title": "Dependency, commit, and test hygiene" },
  { "topic": "writing-style",    "entries": 5,  "title": "Writing style for docs, comments, commits, reports" }
]
```

```sh
houserules get process.tdd   # one entry, in full
```

Restart Claude Code once after the first install (the first
`.claude/agents/` file and the new hook need a fresh session). Set
`git config core.hooksPath .githooks` to activate the commit-msg
Conventional Commits gate.

## Adding to an existing project

```sh
cd my-project
houserules init --id-prefix ABC
```

- `--id-prefix ABC` sets your backlog id prefix (`ABC-001`); default `WI`.
- An existing `AGENTS.md` is never touched: `init` reports `kept
  AGENTS.md`. Copy the `## Knowledge base` and `## Workflow` sections
  from the seeded template into yours by hand; the `migrating-knowledge`
  skill walks the merge. An existing `CLAUDE.md` is kept the same way —
  point it at your `AGENTS.md` with a single `@AGENTS.md` line if you use
  Claude Code.
- An existing `.claude/settings.json` is merged: the two SessionStart
  hook entries are appended only if their matchers are absent. Nothing
  else in your settings is touched.
- `.githooks/commit-msg` is kit-owned: `init` replaces any hook of that
  name your project already has.
- Existing files under `knowledge/`, `backlog/`, evals, or the workflow
  are kept as they are.

Then move your real rules in: add topics as `knowledge/<topic>.json`,
extend the `area` enum in `knowledge/schema.json` together with the
globs in `knowledge/areas.json`, replace the example backlog item, and
run `houserules render`. The `migrating-knowledge` skill walks that move
step by step, from inventory to entries to gates — including eliciting
your project's own merge, attribution, and parallelism discipline, which
houserules leaves to your ruling instead of shipping fixed.

## What it installs

- **A knowledge base** (`knowledge/*.json`): addressable entries (`id`,
  `kind`, `area`, `summary`, `body`, `tags`, `source`, `see`, `verify`,
  optional deterministic `check`), one JSON file per topic, validated by
  a schema your project owns.
- **A backlog** (`backlog/`): typed work items, one JSON file per
  section, driving every change.
- **Project instructions** (`AGENTS.md`): the cross-harness instruction
  file every agent reads, plus a one-line `CLAUDE.md` (`@AGENTS.md`) that
  bridges Claude Code to the same text.
- **The `houserules` binary**: read commands print JSON;
  `check-knowledge`/`check-backlog` gate lint; `render` generates the
  markdown the harness loads; `audit` checks a git range against its
  rule package; `validate` checks agent deliverables.
- **Generated harness files** (`houserules render`): standing rules
  (`.claude/rules/standing-rules.md`), path-scoped area rules
  (`.claude/rules/<area>.md`), and a preloaded `project-knowledge`
  skill.
- **An agent layer**: three agent templates (`implementer`,
  `task-reviewer`, `branch-reviewer`) with rule-adherence audits and a
  JSON deliverables contract, an `orchestrating` skill, a
  `finishing-a-feature` skill, a `migrating-knowledge` skill, a
  SessionStart hook, a commit-msg hook that gates Conventional Commits,
  and eval scenarios.
- **Seed rules**: a generic standing-rule set (TDD, conventional
  commits, no tech debt, dependency vetting, exact pins, doc comments,
  ASD-STE100 writing style, and more). Your project adds its own topics,
  areas, and rules on top.
- **A CI gate** (`.github/workflows/knowledge.yml`), on a GitHub-hosted
  origin: `check-knowledge`, `check-backlog`, and a PR audit, through
  the `houserules` binary alone. On other hosts `init` prints a skip
  note and the `migrating-knowledge` skill covers generating the
  equivalent gate.

## Ownership model

Every path houserules writes falls into one of three buckets:

- **Kit-owned** — `update` overwrites it on every run. Never hand-edit
  it. `.claude/rules/*.md` and
  `.claude/skills/project-knowledge/SKILL.md` are generated by
  `houserules render` from `knowledge/`, so they behave the same way.
- **Seed-once** — `init` writes it only if it is absent, then leaves it
  alone. It is yours from the first write on. `.claude/settings.json`
  is the one exception: `init` merges its two SessionStart hook entries
  into an existing file instead of skipping it. The CI workflow is
  seed-once with one extra gate: written only when your `origin`
  resolves to GitHub; `update` backfills it if your origin becomes
  GitHub-hosted later.
- **Yours** — everything else: your knowledge entries, your backlog
  items, your project code. houserules never touches it.

`houserules files` prints the exact manifest:

| kit-owned (update overwrites) | seed-once (yours after init) |
|---|---|
| `tools/claude-session-start.sh` | `knowledge/` schema, areas, topics |
| `.claude/agents/*.md` | `backlog/` schema and data |
| `.claude/skills/orchestrating`, `finishing-a-feature`, `migrating-knowledge` | `.claude/schemas/deliverables.json`, evals |
| `.githooks/commit-msg` | `AGENTS.md`, `CLAUDE.md` pointer, settings |

## The agent workflow

Three agent templates, each on a different model tier, carry the rules
into every change:

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

Working with the backlog:

| Command | What it does |
|---|---|
| `list` | Lists backlog items, optionally filtered |
| `batch <n>` | Prints one development batch's summary and item rows |
| `set <id> k=v` | Applies field assignments to a backlog item |
| `archive` | Sweeps done/dropped items, batches, and retired entries into `archive/` mirrors |

Gates:

| Command | What it does |
|---|---|
| `check-knowledge` | Validates the knowledge base and generated-file freshness |
| `check-backlog` | Validates the backlog |
| `check-commit` | Runs commit-message checks against a message file or git range |
| `audit` | Builds a git range's rule package and runs every deterministic check |
| `validate` | Validates deliverable JSON files against the schema |
| `check-report-claims` | Cross-checks a report's claims against the artifacts it cites |
| `stats` | Aggregates rule violations across a workspace's deliverables |

Installing:

| Command | What it does |
|---|---|
| `init` | Seeds the kit into a git repository from the embedded payload |
| `update` | Syncs kit-owned files, removes retired ones, reports version drift |
| `files` | Prints the kit-owned and seed-once manifests |

Run any command with `--help` for its flags.

## Stability

houserules is at 1.0. The CLI surface (the twenty subcommands above and
their flags) and the schema constraint surface are frozen: a change to
either is a breaking change and ships only with a major version bump.
Within a major version, `update` never rewrites your project's own data
files. The full contract lives in [docs/design.md](docs/design.md).

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
