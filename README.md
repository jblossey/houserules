# houserules

A reusable knowledge-management and agent-workflow kit for repositories that
are developed with AI agents (Claude Code first, any harness that reads plain
files second). Extracted from the TagPilot project's knowledge-management
setup; the design record lives in [docs/design.md](docs/design.md).

houserules installs, into your project's own repository:

- **A knowledge base** (`knowledge/*.json`): addressable entries (`id`,
  `kind`, `area`, `summary`, `body`, `tags`, `source`, `see`, `verify`,
  optional deterministic `check`), one JSON file per topic, validated by a
  schema your project owns.
- **Two dependency-free CLIs** (`tools/kb.sh`, `tools/backlog.sh`, Node
  built-ins only): read commands print JSON; `check` gates lint; `render`
  generates the markdown the harness loads; `audit` checks a git range
  against its rule package; `validate` checks agent deliverables; `stats`
  aggregates a batch's audits.
- **Generated harness files** (`tools/kb.sh render`): standing rules
  (`.claude/rules/standing-rules.md`), path-scoped area rules
  (`.claude/rules/<area>.md`), and a preloaded `project-knowledge` skill.
- **An agent layer**: three agent templates (`implementer`, `task-reviewer`,
  `branch-reviewer`) with rule-adherence audits and a JSON deliverables
  contract (`.claude/schemas/deliverables.json`), an `orchestrating` skill,
  a `finishing-a-feature` skill, a SessionStart hook, and eval scenarios.
- **Seed rules**: a generic standing-rule set (TDD, conventional commits,
  ff-only merges, sequential agents, no tech debt, dependency vetting, exact
  pins, doc comments, ASD-STE100 writing style, and more). Your project adds
  its own topics, areas, and rules on top.
- **A CI gate** (`.github/workflows/knowledge.yml`): `kb check`,
  `backlog check`, and a PR audit, with plain Node — no other toolchain.

## Apply to a fresh project

```sh
mkdir my-project && cd my-project && git init
node ~/projects/houserules/bin/houserules.mjs init
# or, once published: npx houserules init
tools/kb.sh check && tools/backlog.sh check
git add -A && git commit -m 'chore: install houserules knowledge setup'
```

`init` seeds everything, runs `render`, and stamps `.houserules.json`. Restart
Claude Code once after the first install (the first `.claude/agents/` file
and the new hook need a fresh session).

## Apply to an existing project

```sh
cd my-project
node ~/projects/houserules/bin/houserules.mjs init --id-prefix ABC
```

- `--id-prefix ABC` sets your backlog id prefix (`ABC-001`); default `WI`.
- An existing `CLAUDE.md` is never touched: `init` reports `kept CLAUDE.md`.
  Copy the `## Knowledge base` and `## Workflow` sections from
  `template/CLAUDE.md` into yours by hand.
- An existing `.claude/settings.json` is merged: the two SessionStart hook
  entries (`startup|resume|clear|fork` for the session ritual, `compact` for
  the standing rules) are appended only if their matchers are absent.
  Nothing else in your settings is touched.
- Existing files under `knowledge/`, `backlog/`, evals, or the workflow are
  kept as they are.

Then move your real rules in: add topics as `knowledge/<topic>.json`, extend
the `area` enum in `knowledge/schema.json` together with the globs in
`knowledge/areas.json`, replace the example backlog item, and run
`tools/kb.sh render`.

## Update an existing installation

```sh
node ~/projects/houserules/bin/houserules.mjs update
```

`update` overwrites only kit-owned machinery and re-renders; it never touches
project data. `node bin/houserules.mjs files` prints the ownership manifest:

| kit-owned (update overwrites) | seed-once (yours after init) |
|---|---|
| `tools/` CLIs, wrappers, session hook | `knowledge/` schema, areas, topics |
| `.claude/agents/*.md` | `backlog/` schema and data |
| `.claude/skills/orchestrating`, `finishing-a-feature` | `.claude/schemas/deliverables.json`, evals |
| | `.github/workflows/knowledge.yml`, `CLAUDE.md`, settings |

Do not hand-edit kit-owned files or the generated `.claude/rules/*.md` and
`.claude/skills/project-knowledge/SKILL.md` — edits are lost on the next
`update` or `render`.

## Daily commands

```sh
tools/kb.sh topics | index --topic process | get <id> | for <path> | standing
tools/kb.sh render | check | audit --base origin/main | validate <report.json>
tools/backlog.sh list --open | get WI-001 | batch 1 | set WI-001 status=done batch=1 | check
```

## Development (this repository)

```sh
mise exec -- npm test        # vitest with coverage
```

License: not yet decided by the owner; the package is marked UNLICENSED
until then. Not published to npm or any plugin marketplace.
