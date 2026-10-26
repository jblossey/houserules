# houserules

[![CI](https://github.com/jblossey/houserules/actions/workflows/ci.yml/badge.svg)](https://github.com/jblossey/houserules/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jblossey/houserules?label=release)](https://github.com/jblossey/houserules/releases)

A CLI that makes your AI coding agents adhere to your rules, audits each change, and proves your intent is obeyed.

AI coding agents forget a project's rules between sessions, and nobody can review every line they write. With houserules, you write the rules down once, and an audit checks each change against them. Each rule, gotcha, or decision of your project is an [entry](#concepts) in a JSON file under `knowledge/`. An entry has an id and an [area](#concepts), the paths that it applies to. Before an agent changes a file, `houserules for <path>` gives it the rules of that file. `houserules render` writes the same rules into `AGENTS.md`, the instruction file that many coding agents read. After the change, `houserules audit` runs the [check](#concepts) of each rule against the commits, on your machine or in CI, and fails the change when a check fails, whether a person or an agent wrote it.

houserules is one binary and plain files in your repository. It needs no server, database, account, or hosted service. The knowledge stays readable JSON without the binary.

That is the core. Everything else is an optional layer that you add when you need it: shared rule sets ([packs](#shared-rule-sets)), a store of work items ([tracker](#work-items)), a [review layer](#reviewing-a-change) that links each change to its intent and keeps a record of each merge, and a [workflow for a team of agents](#a-workflow-for-a-team-of-agents). You can stop after the [quick start](#quick-start).

A hand-written `AGENTS.md` costs nothing to start, but nothing checks it. houserules gives each rule an id and a schema, generates the instruction files from one source, and ties a rule to a check that can fail a change. It works beside other tools:

- A tool that copies one rules file into the formats of several agents shares the text of the rules. houserules writes `AGENTS.md` and the files of Claude Code, and it adds a check to a rule.
- A linter or a code scanner, such as Semgrep, parses the code. The checks of houserules match patterns, paths, diffs, and commit messages, and a check can run such a tool as a [runner](#checks-that-run-your-tools).
- A spec tool, such as OpenSpec, plans a change. houserules supplies the rules of each change and [links a commit to its plan](#linking-a-change-to-its-intent).
- A tool that records the session of an agent keeps what the agent did. houserules keeps what each check found and a record of each merge.
- The branch protection of your host blocks a merge. houserules gives it a check to require, and it blocks nothing itself.

It fits a repository where agents change code every day and every agent, tool, and session must follow the same rules. For one person, one agent, and a short `AGENTS.md`, houserules is likely more than you need. [Limits](#limits) lists what it does not do.

## Table of contents

- [Install](#install)
- [Quick start](#quick-start)
- [Rules and checks](#rules-and-checks)
- [Integration](#integration)
- [Linking a change to its intent](#linking-a-change-to-its-intent)
- [Reviewing a change](#reviewing-a-change)
- [A workflow for a team of agents](#a-workflow-for-a-team-of-agents)
- [Limits](#limits)
- [Concepts](#concepts)
- [Documentation](#documentation)
- [Stability](#stability)
- [Contributing and support](#contributing-and-support)
- [License](#license)

## Install

houserules is one binary for Linux (x64 and arm64), macOS (x64 and arm64), and Windows (x64). It needs no Rust toolchain and no Node runtime.

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/jblossey/houserules/releases/latest/download/houserules-installer.sh | sh
```

The installer puts the binary in `~/.local/bin` and adds that folder to your `PATH`. On Windows, run the PowerShell installer: `powershell -c "irm https://github.com/jblossey/houserules/releases/latest/download/houserules-installer.ps1 | iex"`.

With [mise](https://mise.jdx.dev), run `mise use github:jblossey/houserules`. Each [release](https://github.com/jblossey/houserules/releases) also holds an archive for each platform and its SHA-256 checksum. The macOS binaries carry no signature: when macOS blocks a binary that a browser downloaded, run `xattr -d com.apple.quarantine <path to houserules>`, or run the binary once and then click Open Anyway under Privacy & Security in System Settings. To build from the source, clone the repository and run `cargo install --locked --path crates/houserules`.

In CI, install an exact release: replace `latest/download` with `download/v<version>` in the installer URL, or run `mise use github:jblossey/houserules@<version>`.

### Requirements

- **git.** `route`, `evidence`, `bundle check`, and `record write` read the attributes of a commit with `git --attr-source`, so they need git 2.41 or later. `audit` needs it only when an `intent-required` check must find an intent in the changed paths. With an older git, the command stops with `<command>: git <version> is older than 2.41, which --attr-source needs`.
- **`gh`** for the GitHub tracker, and **`bd`** for the Beads tracker. The JSON tracker needs no tool.

### Updating

```sh
houserules update
```

`update` brings the kit files of your repository to the version of the binary and keeps each file that you changed. Installed through the shell installer, it first replaces the binary itself. On an interactive terminal, the other commands look for a newer release at most once a day; set `HOUSERULES_NO_UPDATE_CHECK=1` to turn the check off. That check and the self-update are the only network calls of the binary; the GitHub tracker runs `gh`, the Beads tracker runs `bd`, and a pack from a git repository comes through `git`. [docs/updating.md](docs/updating.md) has the details and the steps to uninstall.

## Quick start

The quick start runs in a new scratch repository, so it changes no file of yours. Run the blocks one after the other in one terminal. In the output, words in angle brackets stand for a value of your run.

```sh
mkdir houserules-demo && cd houserules-demo
git init -q -b main
mkdir src && printf 'def greet(name):\n    return "Hello, " + name\n' > src/app.py
git add -A && git commit -q -m "feat: greet a user"
houserules init
```

```text
wrote .claude/skills/migrating-knowledge/SKILL.md
wrote .claude/skills/houserules-step/SKILL.md
wrote tools/houserules-step.sh
wrote knowledge/schema.json
wrote knowledge/areas.json
wrote .claude/schemas/deliverables.json
wrote docs/README.md
wrote AGENTS.md
wrote CLAUDE.md
wrote knowledge/packs/knowledge-base.json
wrote .gitignore
AGENTS.md: written
houserules: initialized <repository>
for Claude Code: run houserules init --target claude in the install root
init: houserules wrote no CI file; to run the gates and the review layer in your pipeline, run houserules ci print --host github or gitlab
next: <the commands to run next>
```

`init` writes the [kit](#concepts) files and keeps every file that exists. Its last line names the commands to run next, which depend on the state of your repository. Commit the files before the first check: `check-knowledge` matches the globs of each area against the files that git tracks, and a glob that matches no tracked file is a finding. [Setting up your repository](#setting-up-your-repository) says what each file is for.

```sh
git add -A && git commit -q -m "chore: install houserules"
houserules check-knowledge
```

```text
knowledge: ok
```

Now add an area for `src/` and two entries. The block writes `knowledge/areas.json` again, with the four areas that `init` wrote and a fifth, `app`; in your own repository, add the `app` key to the file that you have. The entries go into a topic file of your own, `knowledge/project.json`: a rule whose `check` fails a change that adds a `TODO` under `src/`, and a decision, which records a choice and has no check:

```sh
cat > knowledge/areas.json <<'EOF'
{
  "global": { "paths": [] },
  "process": { "paths": [] },
  "docs": { "paths": ["docs/**", "AGENTS.md", "knowledge/**", "backlog/**", ".claude/**"] },
  "tools": { "paths": ["tools/**"] },
  "app": { "paths": ["src/**"] }
}
EOF
cat > knowledge/project.json <<'EOF'
{
  "topic": "project",
  "title": "Rules of this project",
  "entries": [
    {
      "id": "project.no-todo-comments",
      "kind": "rule",
      "area": "app",
      "summary": "File open work as a work item; leave no TODO comment in the code.",
      "body": ["A TODO comment has no owner and no date, and nobody plans it."],
      "tags": ["code"],
      "source": { "date": "2026-10-26", "by": "user" },
      "check": { "type": "grep-absent", "level": "fail", "files": ["src/**"], "pattern": "TODO", "scope": "changed", "covers": true }
    },
    {
      "id": "project.plain-python",
      "kind": "decision",
      "area": "app",
      "summary": "The app is plain Python with no web framework.",
      "body": ["The app answers in one process and needs no server."],
      "tags": ["code"],
      "source": { "date": "2026-10-26", "by": "user" }
    }
  ]
}
EOF
houserules render
houserules check-knowledge
cat src/AGENTS.md
```

```text
AGENTS.md: written
src/AGENTS.md: written
knowledge: ok
<!-- houserules:begin -->
Generated from knowledge/ by houserules render. Do not edit.

## App rules

### Rules

- [project.no-todo-comments] File open work as a work item; leave no TODO comment in the code.

Detail: houserules get <id>
<!-- houserules:end -->
```

An entry needs `id`, `kind`, `area`, `summary`, `body`, `tags`, and `source`; the `check` is optional. `source.by` is `user`, `review`, `controller`, or `docs`, and `pattern` is a regular expression. `"covers": true` counts the files that the check reads as checked, which [Reviewing a change](#reviewing-a-change) uses. `knowledge/schema.json` holds the schema of an entry and of each check. `render` wrote the rule into `src/AGENTS.md`, beside the code that it governs, and left the decision out.

An agent that changes `src/app.py` asks for the [rule package](#concepts) of the file. Any agent can run the command, whatever instruction files its tool reads:

```sh
houserules for src/app.py
```

```json
{
  "schema": "houserules/rule-package@1.0.0",
  "paths": [
    "src/app.py"
  ],
  "areas": [
    "app",
    "global"
  ],
  "entries": [
    {
      "id": "project.no-todo-comments",
      "kind": "rule",
      "area": "app",
      "standing": false,
      "summary": "File open work as a work item; leave no TODO comment in the code."
    }
  ],
  "standing": "houserules standing"
}
```

Commit the rule. Then make a change that breaks it, and audit the last commit. The audit result goes to `.houserules/`, the folder for scratch output that `init` added to `.gitignore`:

```sh
git add -A && git commit -q -m "docs: add the first project rule"
printf '# TODO: say goodbye too\n' >> src/app.py
git commit -q -am "feat: plan a farewell"
mkdir -p .houserules
houserules audit --base HEAD~1 > .houserules/audit.json
echo "audit exited $?"
sed -n '/"rules": \[/,/^  \],/p' .houserules/audit.json
```

```text
skipped 0 work-item references: no tracker installed (houserules init --tracker)
audit exited 1
  "rules": [
    {
      "id": "project.no-todo-comments",
      "kind": "rule",
      "mode": "deterministic",
      "level": "fail",
      "result": "fail",
      "evidence": "src/app.py:3 matches TODO"
    }
  ],
```

`audit` prints a JSON document: the changed files and their areas, one [audit row](#concepts) for each rule of the change, a `summary` that counts the results, and the work-item `references`. It exits 1 when a row or a reference fails, so a CI job that runs it fails, and 2 when it cannot run, for example for a ref that is no commit. The first line says that the install has no tracker, so `audit` checks no `Work-Item:` trailer. Fix the change and audit again:

```sh
git checkout -q HEAD~1 -- src/app.py
printf 'def farewell(name):\n    return "Goodbye, " + name\n' >> src/app.py
git commit -q --amend -am "feat: say goodbye"
houserules audit --base HEAD~1 > .houserules/audit.json && echo "audit passed"
```

```text
skipped 0 work-item references: no tracker installed (houserules init --tracker)
audit passed
```

People read the same knowledge. `topics` lists the topics, `index` lists the entries of a topic, an area, a tag, or a kind, and `get <id>` prints an entry in full, with its body and its source:

```sh
houserules index --kind decision
```

```json
[
  {
    "id": "project.plain-python",
    "kind": "decision",
    "area": "app",
    "standing": false,
    "summary": "The app is plain Python with no web framework."
  }
]
```

### Setting up your repository

In a repository of your own, run `houserules init` at its top. It writes these files and keeps each one that exists:

- `.houserules.json`, the record of the install, with the version of the kit and the pin of each pack (`init` prints no `wrote` line for it), and the line `.houserules/` in `.gitignore`.
- `knowledge/schema.json`, `knowledge/areas.json`, and `knowledge/packs/knowledge-base.json`: the schema of an entry, your areas, and the copy of the `knowledge-base` pack.
- `AGENTS.md`, which gets the block of `render` at its end, and `CLAUDE.md`, which holds the line `@AGENTS.md`. A `CLAUDE.md` that exists stays as it is: add the line to it, or `check-knowledge` fails.
- Two procedures in Markdown that any agent can read: `.claude/skills/migrating-knowledge/SKILL.md` moves the rules of an existing `AGENTS.md`, `CLAUDE.md`, or other docs into entries, and `.claude/skills/houserules-step/SKILL.md` runs `for` before a change and `audit` after it, with `tools/houserules-step.sh`.
- `.claude/schemas/deliverables.json`, the schema of the JSON reports that `houserules validate` checks, and `docs/README.md`, a note on how to take over a kit file.

It writes no CI file, no git hook, and no git ref, and the files of Claude Code come only with `--target claude`. Running it again writes only what is missing. Review the files with `git status`, commit them, and run `houserules check-knowledge`. [docs/ownership.md](docs/ownership.md) says which files `update` keeps current and which are yours. To remove houserules, delete these files and the houserules block of each `AGENTS.md`; [docs/updating.md](docs/updating.md#uninstalling) lists every step.

In a monorepo, `houserules init --dir <folder>` installs in a folder. A repository can hold several installs that do not nest, each with its own `knowledge/` and `.houserules.json`, and `ci print` gives each one its own `dir` and `key` ([docs/subdirectory-install.md](docs/subdirectory-install.md)). One install can also give each team folder an area, with its own rules and its own route.

## Rules and checks

An entry is one of six kinds: `rule`, `invariant`, `gotcha`, `procedure`, `decision`, or `history`. `for` gives an agent the rules, invariants, gotchas, and procedures of the areas of its paths; you read the decisions and the history with `index` and `get`. A [standing rule](#concepts) (`"standing": true`, of the area `global` or `process`) applies to every change: `houserules standing` lists it, and `render` writes it into the root `AGENTS.md`. Only the `summary` of an entry goes into the instruction files, so keep the rule in that one sentence and the reasons in `body`.

A `check` makes a rule enforceable. Its `level` is `fail` or `warn`, and its `type` is one of nine:

| Type | The row fails when | Example `check` |
|---|---|---|
| `grep-absent` | a pattern matches a line of the files in scope: the changed files, or the whole tree | `{ "type": "grep-absent", "level": "fail", "files": ["src/**"], "pattern": "TODO", "scope": "changed" }` |
| `commits` | a commit message breaks a rule on its subject, its body, or a required trailer | `{ "type": "commits", "level": "fail", "subject": "^[a-z]+: ", "require_trailer": "Work-Item" }` |
| `co-change` | a change to the `if` files comes without a change to the `then` files | `{ "type": "co-change", "level": "fail", "if": ["src/**"], "then": ["CHANGELOG.md"] }` |
| `diff-append-only` | a change removes a line from the files | `{ "type": "diff-append-only", "level": "fail", "files": ["docs/decisions.md"] }` |
| `durable-sha` | an added line cites a commit that the default branch does not reach | `{ "type": "durable-sha", "level": "warn", "files": ["docs/**"] }` |
| `runner` | a tool of your project reports a finding (see below) | `{ "type": "runner", "level": "fail", "tool": "shellcheck", "files": ["**/*.sh"] }` |
| `report-field` | the JSON report of an agent lacks a value for a field | `{ "type": "report-field", "level": "warn", "if": ["src/**"], "field": "docs_verified", "non_empty": true }` |
| `intent-required` | a change to the `if` files names no intent | `{ "type": "intent-required", "level": "fail", "if": ["src/**"] }` |
| `intent-resolves` | an `Intent:` trailer names an intent that does not exist | `{ "type": "intent-resolves", "level": "fail" }` |

A rule without a check is [judged](#concepts): `audit` lists it with the result `open`, for a person or an agent to decide. `get <id>` prints the `check` of an entry, and each audit row has the `mode` `deterministic` or `judged`.

- A check that flags code it should not read: narrow its `files` or its `pattern`, or set its `level` to `warn`. A warned row does not fail the audit.
- A failure that a plan accepts for now is a [sanction](#concepts): `audit --sanctioned <rule>=<ref>` books it with a reference, the row keeps its `fail`, and the audit still exits 1.
- To drop a rule for good, delete its entry, or exclude it when it comes from a pack (see [Shared rule sets](#shared-rule-sets)).
- `houserules check-commit` runs the `commits` checks on a commit message before the commit exists.

### Checks that run your tools

A `runner` check runs a tool that your project defines in the `runners` key of `.houserules.json`, such as shellcheck, Semgrep, or ast-grep. It fails the row on what the tool reports: a non-zero exit, or a finding of its SARIF output. The kit ships no tool. [docs/runners.md](docs/runners.md) shows the setup.

### Shared rule sets

A [pack](#concepts) is one versioned file of entries, with their checks, and of the areas that they need. It holds no tool and no CI file: a `runner` check of a pack needs the tool in the `runners` key of each repository. The kit ships five packs: `knowledge-base` (how to write entries; `init` installs it), `process`, `quality`, `security-hygiene`, and `writing-style`. A team shares its own rules the same way: it tags a pack file in a git repository, and each repository pins the tag.

```sh
houserules pack list                            # the installed packs, and the kit packs you lack
houserules pack add quality                     # a kit pack
houserules pack add git+<url>@<tag>#<path>      # a pack file of a git repository, at an exact tag
houserules pack upgrade <name> <tag>            # move that pack to another tag
```

Each command acts on one repository; houserules has no command that moves many repositories at once. To move many, run `pack upgrade` in each one, for example from a script that opens a pull or merge request. Nobody edits a pack copy in `knowledge/packs/`. A repository changes or drops a pack entry in `knowledge/overrides.json`, and `check-knowledge` prints one line for each [override and exclusion](#concepts), such as `exclusion <id>: <reason>`, so the log of each CI run shows them. houserules has no lock that stops a repository from excluding an entry. The review layer sends a change to `knowledge/` to `senior-review`, and a branch protection rule of your host can require that review. The files in [template/packs/](template/packs) show the shape of a pack, and [docs/upgrading-to-2.md](docs/upgrading-to-2.md#packs) gives the commands in full.

## Integration

### Coding agents

`render` writes a block into the root `AGENTS.md`: the standing rules, and one section for each area that names its globs and `houserules for <path>`. When the globs of an area share one folder, it writes the rules of the area into the `AGENTS.md` of that folder. It writes only between its `<!-- houserules:begin -->` and `<!-- houserules:end -->` lines and keeps the rest of each file. Codex, Cursor, and the coding agent of GitHub Copilot read `AGENTS.md`, and [agents.md](https://agents.md) lists more tools; each tool decides how it reads the `AGENTS.md` of a folder. `CLAUDE.md` holds the line `@AGENTS.md`, so Claude Code reads the same text. The one value of `--target` is `claude`: `houserules init --target claude` adds the files of Claude Code: one rule file for each area in `.claude/rules/`, which Claude Code loads when it reads or edits a matching file, a `project-knowledge` skill, and a hook that prints the standing rules after Claude Code compacts its context. houserules writes no file in the rule format of another tool, such as `.cursor/rules/` or `.github/copilot-instructions.md`. A [harness](#concepts) that reads no `AGENTS.md` gets the rules from `houserules for <path>`. `check-knowledge` fails while a generated file is out of date. [docs/upgrading-to-2.md](docs/upgrading-to-2.md#render-targets) describes each generated file.

### CI

`init` writes no CI file. `houserules ci print --host github` or `--host gitlab` prints the lines that call the review jobs of your release, and writes nothing; on GitHub, the lines call a reusable workflow at the commit of the release tag. `houserules ci write` writes the same lines into a new file of its own, such as `.github/workflows/houserules-review.yml`. [docs/review-layer.md](docs/review-layer.md) shows the output. The jobs run on each pull or merge request and on each push to the default branch:

- `gates` runs `check-knowledge`, `check-backlog`, `audit`, and `bundle check` with a read-only token. It checks out the head of the request, so it audits the request with the rules of the request itself.
- `floor` computes the route of the request with a binary that the request did not build, and it runs no check: a change to a rule source routes to `senior-review`.
- Two jobs hold the write scopes, and neither runs houserules or the code of the repository: `report` comments the reviewer summary on the request, sets a commit status, and sets a label with the route; `record-push` pushes the record that `record` writes for each merge to the notes ref `refs/notes/houserules`. On a request from a fork, `report` posts nothing.

`gates` fails on a failed rule, and on a request whose description holds no valid [evidence bundle](#concepts). `houserules bundle write` writes that section from the captures of your test runs, `houserules ci write --template` writes a template for the description, and `review.bundle.exemptAuthors` and `review.bundle.exemptBranches` in `.houserules.json` exempt authors, such as a dependency bot, and branches. Make the gates job a required check in the branch protection of your host, so that a red audit blocks the merge: houserules itself merges nothing and blocks nothing. Both units take the inputs `dir`, `key`, `setup`, `validation`, `install`, `floor-install`, and `tracker`. `setup` runs a command before the audit, such as the install of your runner tools. Another CI system runs `houserules audit --base <ref>` as a step. The [upgrade guide](docs/upgrading-to-2.md#the-three-ways-of-wiring) describes the three ways to wire the jobs.

### Work items

`houserules init --tracker json`, `github`, or `beads` installs a [tracker](#concepts): JSON files in `backlog/`, the issues of a GitHub repository through `gh`, or a Beads store through `bd`. The commands `list`, `get`, `set`, `defer`, and `check-backlog` work the same on each. A commit names its [work item](#concepts) in a `Work-Item: <id>` [trailer](#concepts), and `audit` fails a trailer whose item does not exist. [docs/trackers.md](docs/trackers.md) covers the trackers, deferrals, and trailers.

## Linking a change to its intent

A commit names the reason for its change, its [intent](#concepts), in an `Intent: <source>:<ref>` trailer that its author, a person or an agent, writes. The source is `work-item`, `file` (a plan or spec file in the repository), or `openspec` (a change of [OpenSpec](https://github.com/Fission-AI/OpenSpec)). The next block adds a plan, and a change that names the plan in its trailer:

```sh
mkdir -p plans
printf '# Say goodbye in German\n\nUsers in Germany asked for a farewell in their language.\n\n- [x] Add the German farewell\n- [ ] Translate the greeting\n' > plans/farewell.md
printf 'def abschied(name):\n    return "Auf Wiedersehen, " + name\n' >> src/app.py
git add -A
git commit -q -m "feat: say goodbye in German" -m "Intent: file:plans/farewell.md"
houserules evidence --base HEAD~1 > .houserules/packet.md
sed -n '/^### Intent/,/^Tasks:/p' .houserules/packet.md
```

````text
### Intent (1)

`file plans/farewell.md`: `Say goodbye in German`
Why:
```
Users in Germany asked for a farewell in their language.
```
Tasks: 1 of 2 done
````

`evidence` writes the reviewer summary of a change, the [evidence packet](#concepts). For each intent it prints the title, the reason, and the task count of the plan; it does not compare the diff with the plan. Nothing requires the trailer until you add an entry with an `intent-required` check: `audit` then fails a change to the `if` files whose commits name no intent. An `intent-resolves` check fails a trailer whose intent does not exist. After a squash merge, the record of the merge reads the squash commit, so keep the trailers in its message.

OpenSpec plans a change: its proposal, specs, design, and tasks. houserules supplies the rules of the project and the checks of each change. houserules reads the `openspec/` folder and edits no OpenSpec file; an `openspec:` trailer still resolves after `openspec archive` moves the change. A block that OpenSpec or another tool keeps in `AGENTS.md` stays as it is, since `render` writes only between its own markers. When git tracks OpenSpec changes, `render` adds a section to its block that tells agents to run `houserules for` before a change and `houserules audit` before they archive it. The reference workflow plans changes too, so a project that plans with OpenSpec needs no reference workflow, and `check-knowledge` warns about an install that holds both ([Plan with one workflow](docs/openspec.md#plan-with-one-workflow)). [docs/openspec.md](docs/openspec.md) shows the setup, a spec requirement beside a rule, the two checks with an example entry, and the guidance block for `openspec/config.yaml`.

## Reviewing a change

The review layer answers three questions about a change: what changed, what no check read, and who must review it. The CI jobs that `ci print` wires run it, and each of its commands also runs on your machine. The packet of the last section starts with the [uncovered surface](#concepts): the changed lines that no check with `"covers": true` read.

```sh
sed -n '/^### Uncovered/,/^Coverage/p' .houserules/packet.md
```

```text
### Uncovered surface (6 of 8 changed lines)

- `plans/farewell.md`: lines 1-6

Coverage is per file: a file is covered when a check that declares covers: true read it.
```

The rule of the quick start covers `src/`, so only the plan is uncovered; no entry of the kit sets `covers`. After the intent, the packet lists the failed, warned, and passed (proven) rules, the judged rules, the size of the diff, and the [route](#concepts). The route is the highest route that any of these reasons gives:

- A change to a rule source (`knowledge/`, `.houserules.json`, or a CI file of houserules) gets `senior-review`.
- A failed row gets `owner-review`, and so does a warned row unless `review.route.warn` names another route.
- `review.route` in `.houserules.json` gives an area, or the paths of a glob, a route of its own, such as `"areas": { "app": { "route": "senior-review" } }`. Every other change gets `review.route.default`, which is `owner-review` unless you set it.
- `auto-merge-eligible` holds only while the size of the diff and its uncovered share stay within the `maxDiffLines` and `maxUncoveredShare` of its entry.

houserules names no reviewer and reads no `CODEOWNERS` file. The route is a label and a commit status, and your host decides who reviews.

Each merge gets a [change record](#concepts): a git note with the commits, the hashes of the rules, the audit rows, the intents, the route, and the approvers. The CI jobs write and push it; on your machine, `record write` writes it to your clone:

```sh
houserules record write --base HEAD~1 --head HEAD
houserules record list
houserules record show HEAD | sed -n 3p
```

```text
<sha> owner-review 0 reviewers - unaudited
Record <sha> · range <sha>..<sha> (merge base <sha>) · rules <hash> · route owner-review
```

`record list` prints one line for each record: its commit, its route, the number of approvers that the CI job read from the merged request, `sampled` or `-` for the post-merge sample, and the verdict of `record audit`, `unaudited` while there is none. `record show` prints the record as Markdown, with the packet, the sample, the reviewers, and the post-merge audit, and checks its rule hashes against git. A clone fetches the records with `git fetch origin refs/notes/houserules:refs/notes/houserules`. `houserules stats --records <range>` counts the records of a range and those with a reviewer, for each route and each area. [docs/review-layer.md](docs/review-layer.md) lists the commands and the `review` settings, and [docs/evidence.md](docs/evidence.md) covers `houserules capture`, which saves the output of a test run under a header with its exit status and its commit.

## A workflow for a team of agents

`houserules init --workflow reference --tracker json --add-required-packs` installs an optional process for a project that hands tasks to agents: four agent templates for Claude Code (an implementer and three reviewers), four skills that take a change from its spec to its merge, a commit-msg hook, and the packs whose rules the process cites. Turn the hook on with `git config core.hooksPath .githooks`, or call `sh .githooks/commit-msg "$1"` from your hook manager. Each agent audits its own change and writes a JSON report that `houserules validate` and `houserules check-report-claims` check. Without the workflow, houserules works with the process that you have. [docs/reference-workflow.md](docs/reference-workflow.md) describes the workflow.

`houserules stats` reads the reports, reviews, and audits of the agents and counts, for each entry, the workspaces that loaded it, its failed rows, and the findings that cite it. It proposes changes from the numbers, such as `retire` for an entry that is not standing, that at least three workspaces loaded, and that never failed, drew no finding, and no report cited. [docs/process-cost.md](docs/process-cost.md) describes each number.

## Limits

- houserules enforces only what a check tests. `audit` lists a rule without a check as `open`, and no command lists the rules that carry a check.
- Its own checks match patterns, paths, diffs, and commit messages, and they read no meaning of code. A `runner` check brings a tool that does.
- It blocks nothing by itself. A red audit stops a merge only when your host requires the job, and a sanction keeps the audit red.
- In CI, a request is audited with its own rules, so a request that changes `knowledge/` changes the rules that judge it. Its route, `senior-review`, shows the change, and a review that your host requires holds it.
- A repository can override or exclude any entry of a pack, and no lock stops it. `check-knowledge` prints each override and exclusion.
- It signs nothing. A capture records a command, its exit status, and its commit, and the hash that a report records shows a later edit of the file, but not whether the run took place as the capture says. The notes ref of the change records is an ordinary git ref: the push rules of your host decide who can rewrite it.
- A change record names its commits by sha, and their author is what git records. houserules does not tell an agent's commit from a person's, and it keeps no prompt and no session, only a trailer that points at one.

## Concepts

- **area**: a name and a list of path globs in `knowledge/areas.json`. An entry belongs to one area and applies to the files that its globs match. The entries of the area `global` apply to every file.
- **audit row**: the result of one rule for a change: `pass`, `fail`, `warn`, `skipped`, or `open` for a judged rule.
- **capture**: the output of a command in a file under a one-line JSON header with the command, its exit status, and its commit. `houserules capture` writes it.
- **change record**: the git note that `record write` keeps for each merge, under `refs/notes/houserules`.
- **check**: the machine test of an entry, one of the nine types of [Rules and checks](#rules-and-checks).
- **entry**: one rule, invariant, gotcha, procedure, decision, or history item, with a permanent id `<topic>.<name>`.
- **evidence bundle**: a section of the description of a pull or merge request: its purpose, its work items, and the captures of its verification runs. `bundle write` writes it, and `bundle check` checks it.
- **evidence packet**: the reviewer summary that `evidence` writes as Markdown.
- **harness**: the tool that runs a coding agent, such as Claude Code.
- **install root**: the folder that holds `.houserules.json`: the top of the repository, or the folder that `init --dir` names.
- **intent**: the reason for a change: a work item, a file, or an OpenSpec change, named in an `Intent:` trailer.
- **judged**: a rule with no check. A person or an agent decides it.
- **kit**: the files that the binary carries and that `init` and `update` write. `update` keeps a kit file that you changed.
- **override, exclusion**: a change to a pack entry, or its removal, in `knowledge/overrides.json`.
- **pack**: a versioned file of entries, copied to `knowledge/packs/` and pinned in `.houserules.json` by version, source, and checksum.
- **render**: the writing of the generated files from `knowledge/`: the `AGENTS.md` blocks, and with `--target claude` the files in `.claude/`.
- **route**: who must review a change: `auto-merge-eligible`, `owner-review`, or `senior-review`. It is a label and a commit status, and your host decides who reviews.
- **rule package**: the entries that a set of paths pulls in, which `for` prints and `audit` checks.
- **runner**: a tool of your project that a `runner` check starts.
- **sanction**: a failure that `audit --sanctioned <rule>=<ref>` books with a reference. The row stays `fail`.
- **standing rule**: an entry with `"standing": true`. It applies to every change and loads in every session.
- **topic**: a group of entries: one file `knowledge/<topic>.json`, and the first part of each id.
- **tracker**: the store of work items: JSON files, GitHub issues, or Beads.
- **trailer**: a `Key: value` line at the end of a commit message, such as `Work-Item: WI-001` or `Intent: file:plans/login.md`.
- **uncovered surface**: the changed lines that no check with `"covers": true` read.
- **work item**: a task in the tracker, with an id such as `WI-001`.

## Documentation

- [Updating and uninstalling](docs/updating.md)
- [A subdirectory install](docs/subdirectory-install.md): an install in a folder of a monorepo
- [Trackers](docs/trackers.md): work items, deferrals, and trailers
- [The houserules step and evidence](docs/evidence.md): the helper script, captures, and the checks of a report
- [Runners](docs/runners.md): checks that run your own tools
- [The review layer](docs/review-layer.md): the reviewer summary, routes, records, and CI
- [Ownership model](docs/ownership.md): the files that `update` writes, and the files that are yours
- [The reference workflow](docs/reference-workflow.md): the optional process for a team of agents
- [Measure process cost](docs/process-cost.md): `houserules stats` over the reports of agents
- [CLI reference](docs/cli.md): every command; `houserules <command> --help` prints its flags
- [houserules with OpenSpec](docs/openspec.md)
- [Upgrading to houserules 2.0](docs/upgrading-to-2.md): packs, overrides, and each hand step of `update`

## Stability

houserules follows semantic versioning. A change that breaks a command, a flag, or a schema comes in a new major version, and the [changelog](CHANGELOG.md) names it. The JSON documents of `for`, `audit`, `capture`, `evidence`, `route`, and `record` name their schema and its version, such as `houserules/audit-result@1.0.0`. The schemas live in [schemas/](schemas), and a published version of a schema never changes.

## Contributing and support

Ask how to use houserules in the [Q&A discussions](https://github.com/jblossey/houserules/discussions/new?category=q-a), and report a bug or ask for a change in an [issue](https://github.com/jblossey/houserules/issues/new/choose). Report a security problem in private, as [SECURITY.md](SECURITY.md) describes. You can contribute a change: [CONTRIBUTING.md](CONTRIBUTING.md) explains the setup, the commit rules, and the gates that each change must pass. This repository runs houserules on itself, and its own rules are in [knowledge/](knowledge).

## License

[MIT](LICENSE). The scripts that the kit writes into `tools/` carry an SPDX header, so a copy keeps the notice.
