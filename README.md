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
`knowledge/*.json`, addressable by id. Your work items live in the tracker
that you choose: JSON files, GitHub Issues, or Beads. A commit names the
item it delivers. An optional reference workflow adds agent templates that
carry the rules into every change and audit the result against them, so the
rules stay enforced instead of drifting into a wiki nobody reads.

## Table of contents

- [Why](#why)
- [Install](#install)
- [Quick start](#quick-start)
- [Adding to an existing project](#adding-to-an-existing-project)
- [What it installs](#what-it-installs)
- [Packs](#packs)
- [Trackers](#trackers)
- [The houserules step](#the-houserules-step)
- [Ownership model](#ownership-model)
- [The reference workflow](#the-reference-workflow)
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
exclusions. Where the install runs the reference workflow, it moves the files
of the workflow with the binary and keeps a file that you changed.
When a pack ships the id of one of your own entries, it moves that entry
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

The knowledge schema (`knowledge/schema.json`), the deliverables schema
(`.claude/schemas/deliverables.json`), and the CI workflow
(`.github/workflows/knowledge.yml`) are a kind of file that `houserules
files` lists as `d3Files`. `init` writes each once and records a
baseline for it. `update` keeps each current through that baseline: it
rewrites a copy that still equals its baseline, keeps and lists a copy
that you changed, and leaves a copy that you list in `ownedPaths` as it
is. The CI workflow installs the release that `.houserules.json` records.
`update` replaces a CI workflow of houserules 1.x that equals the workflow
of a release. It keeps a CI workflow with steps of your own and prints a
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
seeded `knowledge/` data and the `backlog/` data of a JSON tracker, or keep
the knowledge files: they are readable JSON and useful without the binary.

## Quick start

```sh
mkdir my-project && cd my-project && git init
houserules init
houserules check-knowledge && houserules check-backlog
git add -A && git commit -m 'chore: install houserules knowledge setup'
```

`init` installs the minimal default: the `knowledge-base` pack (your
starting rules), the schemas, `AGENTS.md` and `CLAUDE.md`, the
`migrating-knowledge` and `houserules-step` skills, and a CI workflow on a
GitHub-hosted origin. It installs no tracker and no workflow. It then
renders the harness files and stamps `.houserules.json` with the binary's
version. Both gates pass on this install:

```text
knowledge: ok
no tracker installed; run houserules init --tracker json|github|beads to add one
```

`check-backlog` has no tracker to check, so it prints its line and exits 0.
Look at what you got:

```sh
houserules topics
```

```json
[
  {
    "topic": "knowledge-base",
    "entries": 7,
    "title": "Authoring knowledge entries"
  }
]
```

```sh
houserules get knowledge-base.ids-are-permanent   # one entry, in full
```

Add more of the kit when you need it:

```sh
houserules pack add quality                                  # one more pack of starting rules
houserules init --tracker json                               # a tracker for your work items
houserules init --workflow reference --add-required-packs    # agent templates and skills
```

The reference workflow installs a commit-msg hook that gates Conventional
Commits. Set `git config core.hooksPath .githooks` to activate it.

For Claude Code, add the Claude files:

```sh
houserules init --target claude
```

This seeds the session-start script and its hook entry in
`.claude/settings.json`, and enables the Claude renderer, which writes the
area rules under `.claude/rules/` and the `project-knowledge` skill.
Restart Claude Code once afterward (a new hook or a new `.claude/agents/`
file needs a fresh session). Other harnesses need nothing more: the
standing rules sit in the `houserules` block of `AGENTS.md`.

## Adding to an existing project

```sh
cd my-project
houserules init --id-prefix ABC
```

- `--id-prefix ABC` sets the id prefix of the JSON tracker (`ABC-001`); default `WI`.
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
  `init` adds a hook entry only if the file lacks it. Nothing else in your
  settings is touched.
- `init --workflow reference` keeps a file that your project already has at
  a path of the workflow, such as `.githooks/commit-msg` or an agent
  template. It prints `hand step: <file> is not the kit's; the reference
  workflow leaves it to the project`, and the file stays yours.
- Existing files under `knowledge/` and `backlog/` are kept as they are.

Then move your real rules in: add your entries as topic files
`knowledge/<topic>.json` (an id starts with its topic name), add the
areas they use to `knowledge/areas.json` with their globs, choose a
tracker for your work items (see [Trackers](#trackers)), and run
`houserules render`. To change a rule of
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

- The install root is the directory you give. `.claude/` lives in the
  install root, so launch Claude Code there. `.githooks/` lives there too,
  where the install runs the reference workflow.
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
  workflow; `init` refuses a new root that nests with another install or
  reuses a workflow file name. A root that already holds its
  `.houserules.json` is no new root: `init` there runs no such check.
- houserules never sets `core.hooksPath`. Where the install runs the
  reference workflow, activate its commit-msg hook with `git config
  core.hooksPath services/api/.githooks`, or call it from your hook manager
  in one line: `sh services/api/.githooks/commit-msg "$1"`.

## What it installs

A plain `init` installs the core. A tracker, the reference workflow, and
the Claude Code files are opt-in.

- **A knowledge base** (`knowledge/`): addressable entries (`id`,
  `kind`, `area`, `summary`, `body`, `tags`, `source`, `see`, `verify`,
  optional deterministic `check`). The kit's entries come as five packs
  in `knowledge/packs/`, and a plain `init` installs the `knowledge-base`
  pack. Your own entries live in one JSON file per
  topic, `knowledge/<topic>.json`. Your changes to pack entries live in
  `knowledge/overrides.json`, and your areas in `knowledge/areas.json`.
  A schema validates the entries, and `update` keeps it current.
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
- **Skills and a report format**: a `migrating-knowledge` skill, a
  `houserules-step` skill with the script `tools/houserules-step.sh` (see
  [The houserules step](#the-houserules-step)), and a JSON deliverables
  contract, `.claude/schemas/deliverables.json`, that every report and
  review follows.
- **A tracker** (`houserules init --tracker`): the store of your work
  items, in JSON files, GitHub Issues, or Beads. See
  [Trackers](#trackers).
- **The reference workflow** (`houserules init --workflow reference`): four
  agent templates (`implementer`, `task-reviewer`, `branch-reviewer`,
  `spec-reviewer`) with rule-adherence audits; the `orchestrating`,
  `finishing-a-feature`, `designing`, and `planning` skills; a commit-msg
  hook that gates Conventional Commits; eval scenarios; and a session-start
  script. See [The reference workflow](#the-reference-workflow).
- **Claude Code files** (`houserules init --target claude`): a
  session-start script that prints the standing rules again after a
  compaction, its hook entry in `.claude/settings.json`, and the Claude
  renderer.
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
  write the stamp, or to name the error in it. The steps that run the
  gates follow your tracker: the GitHub form adds the `issues: read`
  permission and a `GH_TOKEN`, and the Beads form installs `bd` at a pinned
  release and bootstraps the store. On other hosts `init`
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

The `process` pack names the work items of your tracker, so `pack add
process` needs one. Without a tracker it refuses: `pack process needs a
tracker; run houserules init --tracker json|github|beads`.

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

## Trackers

A tracker holds your work items. `init` installs none until you choose one:

```sh
houserules init --tracker json     # the backlog/*.json files of your repository
houserules init --tracker github   # the issues of a GitHub repository, through gh
houserules init --tracker beads    # a Beads store, through bd
```

- **JSON** keeps typed items, batches, amendments, and parked polish in
  `backlog/`, with a schema. `--id-prefix` sets the item id prefix.
- **GitHub Issues** runs the `gh` CLI. `--repo <owner>/<name>` names the
  repository; without it, `init` reads the `origin` remote. houserules never
  reads or stores a token: `gh` keeps its own login. An item is an issue
  with a `type:<t>` label. `check-backlog` names each `houserules:` label
  that your repository lacks.
- **Beads** runs the `bd` CLI. Run `bd init` first, or `bd bootstrap` in a
  clone: `init --tracker beads` refuses without a store. houserules sets
  `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` on every `bd` it starts, so an
  adopter never sends bd usage events through houserules. A user's own `bd`
  calls keep bd's own setting. The seeded CI workflow of a Beads install
  installs `bd` and bootstraps the store.

The work-item commands are the same on every tracker: `list`, `batch`,
`get`, `set`, `defer`, `check-backlog`, and `archive`. An install keeps its
tracker, so `init --tracker` with another tracker refuses. `github` and
`beads` also refuse an install that has a `backlog/` directory. The
`migrating-knowledge` skill describes how to move the data. Every tracker
writes the `backlogId` pattern of the deliverables schema for its id shapes.

With no tracker, `list`, `batch`, `set`, `defer`, and `get` of a work item
print one line that says so and exit 2. `check-backlog` prints that line and
exits 0. Where a `backlog/` directory exists, `list`, `batch`, `set`,
`defer`, and `get` of a work item print
`backlog/ exists but no tracker is installed; run houserules init --tracker json`
and exit 2, and `check-backlog` and `audit` print it and exit 1. A tracker
that the gates cannot reach is never a pass: `check-backlog` and `audit`
print `tracker <adapter> cannot be reached: <cause>; <setup step>` and exit 1.

### Deferrals

`defer` files work that waits. `--title` files a new item (a chore, open,
with the reason as its body). `--item <id>` marks an existing item as a
deferral.

```sh
houserules defer --title "Split the parser module" \
  --reason "The split changes the public API and waits for a major version." \
  --origin "review of the parser change" \
  --file src/parser.rs --file src/lexer.rs
```

```text
WI-002
```

`list --deferred` shows each deferral with its `reason`, `origin`, and
`files`. A title has at most 120 characters.

### Trailers

A commit names the work item it delivers in a `Work-Item:` trailer. A
commit that files a deferral names it in a `Deferred:` trailer:

```text
fix(parser): read a quoted key

Work-Item: WI-012
Deferred: WI-015 the split changes the public API
```

A commit that delivers several items writes one `Work-Item:` trailer for
each. `audit` resolves every trailer of its range through the installed
tracker. A `Work-Item:` id must name an item. A `Deferred:` id must name a
filed deferral with a reason. Each failure prints one line, and `audit`
exits 1:

```text
<short-sha>: Work-Item WI-099 does not resolve in the json tracker
```

With no tracker, `audit` resolves nothing. It prints `skipped <n> work-item
references: no tracker installed (houserules init --tracker)`. Where a
`backlog/` directory exists, it prints the `backlog/ exists` line of the
section [Trackers](#trackers) and exits 1.

The JSON result of `audit` holds the same verdict. `references.failures`
lists each failure line, `references.resolved` and `references.skipped` count
the references, and `summary.reference_fail` counts the failures. The
`backlog/ exists` line and the line of a tracker that cannot be reached are
failures too. `summary.fail` counts only the rule rows.

The process pack ships `process.every-commit-names-a-work-item`, which fails
a commit that has no `Work-Item:` trailer. It is an opt-in entry. When
a command brings the process pack onto an install, houserules excludes the
entry in `knowledge/overrides.json` with the reason `opt-in`, and prints
`process.every-commit-names-a-work-item is an opt-in entry;
knowledge/overrides.json excludes it until you delete its exclusion`. Delete
the exclusion to opt in. The install then keeps the entry, whatever a later
command does.

## The houserules step

Every install holds the `houserules-step` skill and the script
`tools/houserules-step.sh`. Any workflow, or none, calls the script around
a change. The script runs the `houserules` on your `PATH`:

```sh
tools/houserules-step.sh before src/parser.rs     # the rules for the files you will change
tools/houserules-step.sh after main report.json   # the audit since main, and the report check
```

`before` runs `houserules for` and writes the rule package to
`.houserules/step/rules.json`. `after <base>` runs `houserules audit --base
<base>` and writes the result to `.houserules/step/audit.json`. With a
report as the second argument, it also runs `houserules validate` and
writes `.houserules/step/validate.json`. The script prints the path of each
output file, and `after` prints each exit status, so an agent reads the
output with its file-read tool. Paths and refs are relative to the install
root. `.houserules/step/` holds scratch output, and `init` adds it to
`.gitignore`.

## Ownership model

Every path houserules writes falls into one of seven classes, and
`houserules files` prints the exact manifest of each as JSON:

- **Kit-owned** (`kitOwned`) — `update` writes a missing file and
  rewrites one you did not change on every run. It keeps and reports a
  file you changed. Never hand-edit it.
- **Seed-once** (`seedOnce`) — `init` writes it only if it is absent,
  then leaves it alone. It is yours from the first write on.
  `.claude/settings.json` is the one exception: with `--target claude`,
  `init` merges its hook entries into an existing file instead of
  skipping it.
- **Kept current through a baseline** (`d3Files`) — the knowledge schema,
  the deliverables schema, and the CI workflow. `init` writes each once and
  baselines it, and `update` keeps it at the content of the kit unless you
  changed it. The CI workflow is written only when your `origin` resolves
  to GitHub; `update` backfills it if your origin becomes GitHub-hosted
  later.
- **Tracker files** (`trackerFiles`) — the JSON tracker: its five data
  files are seed-once, and `backlog/schema.json` is kept current through a
  baseline. `init --tracker json` writes them, and `update` restores or
  delivers them only where the tracker is `json`.
- **Workflow files** (`workflowFiles`) — the files of the reference
  workflow. `init --workflow reference` writes them and pins each one in
  `.houserules.json` under `workflow`. `update` writes a file whose bytes
  equal its pin and moves the pin with the binary. It keeps a file that you
  changed and prints `kept <file> (locally modified; reference workflow
  <version>)` on every run, until you merge the change by hand. A file at a
  workflow path that the kit did not write is yours and gets no pin.
- **Workflow data** (`workflowData`) — `.claude/evals/record.json`, your
  record of eval runs. `init --workflow reference` writes it empty, and
  nothing pins or replaces it.
- **Pack copies** (`packCopies`) — `knowledge/packs/<pack>.json`.
  `update` moves each kit pack to the version of the binary and rewrites
  its copy. You change a pack entry through `knowledge/overrides.json`,
  never in the copy.
- **Yours** — everything else: your knowledge entries, your work items
  in a tracker you keep, your project code. houserules never touches it.

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
| kit-owned (`kitOwned`) | `tools/claude-session-start.sh` (with `--target claude`), `.claude/skills/migrating-knowledge/SKILL.md`, `.claude/skills/houserules-step/SKILL.md`, `tools/houserules-step.sh` |
| seed-once (`seedOnce`) | `knowledge/areas.json`, `docs/README.md`, `AGENTS.md`, `CLAUDE.md` |
| kept current through a baseline (`d3Files`) | `knowledge/schema.json`, `.claude/schemas/deliverables.json`, `.github/workflows/knowledge.yml` (GitHub-hosted origin only) |
| tracker files (`trackerFiles`) | `backlog/batches.json`, `backlog/amendments.json`, `backlog/decisions.json`, `backlog/parked.json`, `backlog/items/general.json`, `backlog/schema.json` |
| workflow files (`workflowFiles`) | `.claude/agents/implementer.md`, `.claude/agents/task-reviewer.md`, `.claude/agents/branch-reviewer.md`, `.claude/agents/spec-reviewer.md`, `.claude/skills/orchestrating/SKILL.md`, `.claude/skills/finishing-a-feature/SKILL.md`, `.claude/skills/designing/SKILL.md`, `.claude/skills/planning/SKILL.md`, `.claude/evals/dependency-add.json`, `.claude/evals/docs-edit.json`, `.claude/evals/seeded-violations.json`, `.githooks/commit-msg`, `tools/reference-session-start.sh` |
| workflow data (`workflowData`) | `.claude/evals/record.json` |
| pack copies (`packCopies`) | `knowledge/packs/knowledge-base.json`, `knowledge/packs/process.json`, `knowledge/packs/quality.json`, `knowledge/packs/security-hygiene.json`, `knowledge/packs/writing-style.json` |

## The reference workflow

The reference workflow is an optional set of lifecycle files for a project
that hands tasks to agents. It needs a tracker and the rules that its files
cite:

```sh
houserules init --workflow reference --tracker json --add-required-packs
```

Without a tracker, `init` refuses: `the reference workflow needs a tracker;
run houserules init --workflow reference --tracker json|github|beads`.
Without `--add-required-packs`, `init` names the packs that hold the rules
the workflow cites and adds none. `check-knowledge` fails while a rule that
the workflow requires is missing or excluded.

Four agent templates, each on a configured model, carry the rules into
every change:

- **implementer** (default model `sonnet`): implements one task from a
  brief, test-driven, and writes a JSON report.
- **task-reviewer** (default `opus`): reviews one task's diff for spec
  compliance, code quality, and rule adherence.
- **branch-reviewer** (default `opus`): reviews the whole branch before
  merge and proposes knowledge-base improvements drawn from the batch's
  reviews.
- **spec-reviewer** (default `opus`): attacks a spec before a human reads
  it and files its findings in a `spec-review` deliverable.

The model of each role sits in `workflow.models` in `.houserules.json`.
The templates carry no `model:` line. The `orchestrating` skill names the
configured model in every dispatch, and each report and review records it
in its `model` field.

Four skills drive the lifecycle. `designing` drafts a spec from what the
project already holds, has it reviewed, and takes it to the project's owner
or decider. `planning` turns the approved spec into tasks with declared
file sets. `orchestrating` dispatches the agents, runs the live run, and
drives the batch to its finish. `finishing-a-feature` merges the branch.
The risk of the change picks one of two tiers, light or standard. A task is
critical when it changes the commands, flags, or output of a command-line
interface, a schema, authentication or security code, or the data that an
install keeps on disk: its tests come first, and it gets one review. Every
dispatched agent runs `houserules audit` against its own diff and records
the result in its JSON report, so rule adherence is checked, not just
asserted.

The workflow also installs a commit-msg hook that gates Conventional
Commits, three eval scenarios with their record, and
`tools/reference-session-start.sh`, which prints the session ritual. The
block that `render` writes into `AGENTS.md` points every harness at the
`orchestrating` skill. A harness with no subagents does the work directly,
in its own session.

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
- `models`: one row per model that wrote a deliverable, sorted by row
  name. A `model` value that names one family (`fable`, `haiku`, `opus`,
  or `sonnet`) as a whole word counts in the row of that family: the
  alias `opus` and the id `claude-opus-5-5` share the row `opus`. A value
  that names no family keeps its own row, as `human` and `unknown` do. A
  row has `model` in place of `path` and the other fields of a
  `workspaces` row. A deliverable with no `model` field counts in the
  row `pre-2.0`.
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

One `models` row, for a workspace whose only deliverable is a task review
that `claude-opus-5-5` wrote, with one important finding on code and one
minor finding on prose. The row is named after the family, `opus`:

```json
{
  "model": "opus",
  "tasks": 0,
  "reviews": 1,
  "re_reviews": 0,
  "fix_rounds": 0,
  "report_bytes": 0,
  "findings": {
    "critical": 0,
    "important": 1,
    "minor": 1,
    "other": 0
  },
  "targets": {
    "code": 1,
    "deliverable": 0,
    "prose": 1,
    "other": 0
  }
}
```

One `proposals` row, for a workspace whose only deliverable is a task review
with four minor findings on prose that cite `process.claims-match-artifacts`:

```json
{
  "action": "narrow",
  "id": "process.claims-match-artifacts",
  "owner_gate": true,
  "reason": "4 of 4 findings are minor or hit a deliverable. Narrow the rule to contract surfaces.",
  "evidence": {
    "findings": 4,
    "soft_findings": 4
  }
}
```

## CLI reference

Working with knowledge:

| Command | What it does |
|---|---|
| `topics` | Lists every knowledge topic with its entry count and title |
| `index` | Lists knowledge-entry index rows, optionally filtered |
| `get <id>` | Prints items by id: a knowledge entry, or a work item of the installed tracker |
| `for <path>` | Prints the rule package a set of changed paths pulls in |
| `standing` | Lists the standing rules |
| `render` | Rewrites stale generated harness files (`--check` lists them instead) |
| `pack <list\|add\|upgrade\|remove>` | Lists, adds, upgrades, and removes the knowledge packs of an install |

Working with work items:

| Command | What it does |
|---|---|
| `list` | Lists the work items of the installed tracker, optionally filtered; `--deferred` lists the deferrals with their reasons |
| `batch <n>` | Prints one development batch's summary and item rows |
| `set <id> k=v` | Applies field assignments (`status`, `batch`, `milestone`) to a work item |
| `defer` | Files a deferral: `--title` files a new item, and `--item <id>` marks an existing one; both name a reason and an origin |
| `archive` | Sweeps done/dropped items and done batches of the JSON tracker, and retired or superseded entries of your topic files, into `archive/` mirrors; a pack entry stays in its pack copy |

Gates:

| Command | What it does |
|---|---|
| `check-knowledge` | Validates the knowledge base, its packs against their pins, its overrides and exclusions, generated-file freshness, and that kit-owned files cite only entries it holds |
| `check-backlog` | Validates the work items of the installed tracker |
| `check-commit` | Runs commit-message checks against a message file or git range |
| `audit` | Builds a git range's rule package and runs every deterministic check, and resolves each `Work-Item:` and `Deferred:` trailer of the range through the installed tracker; a task audit (`--report`) leaves out an entry tagged `controller` unless `--ids` names it |
| `validate` | Validates deliverable JSON files against the schema, which requires a `model` field |
| `check-report-claims` | Cross-checks a report's claims against the artifacts it cites; with `--doc <path>` or `--item <id>` it checks a spec or a backlog item for cited workspace paths that do not exist |
| `stats <workspace>...` | Aggregates rule violations, process cost, and knowledge-base proposals across one or more workspaces |

Installing:

| Command | What it does |
|---|---|
| `init` | Seeds the minimal default into a git repository from the embedded payload; `--tracker json\|github\|beads` (with `--repo` for GitHub) installs a tracker, `--workflow reference` installs the reference workflow (`--add-required-packs` adds the packs it needs), `--target claude` adds the Claude Code files, and `--dir <subdir>` installs in a subdirectory |
| `update` | Syncs kit-owned files, moves the kit packs and the workflow files, converts an install of houserules 1.x, removes retired files, reports version drift |
| `files` | Prints the file classes of the kit as JSON: kit-owned, seed-once, pack copies, kept current through a baseline, tracker files, workflow files, and workflow data |

Run any command with `--help` for its flags.

## Stability

The CLI surface (the subcommands above and their flags) and the
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
