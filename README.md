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
  a `finishing-a-feature` skill, a `migrating-knowledge` skill, a
  SessionStart hook, a commit-msg hook that gates harness trailers, and
  eval scenarios.
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
# or, once published: pnpm dlx houserules init
tools/kb.sh check && tools/backlog.sh check
git add -A && git commit -m 'chore: install houserules knowledge setup'
```

`init` seeds everything, runs `render`, and stamps `.houserules.json`. Restart
Claude Code once after the first install (the first `.claude/agents/` file
and the new hook need a fresh session). Set `git config core.hooksPath
.githooks` to activate the commit-msg trailer gate.

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
- `.githooks/commit-msg` is kit-owned: `init` replaces any hook of that name
  your project already has. Set `git config core.hooksPath .githooks` to
  activate its commit-msg trailer gate.
- Existing files under `knowledge/`, `backlog/`, evals, or the workflow are
  kept as they are.

Then move your real rules in: add topics as `knowledge/<topic>.json`, extend
the `area` enum in `knowledge/schema.json` together with the globs in
`knowledge/areas.json`, replace the example backlog item, and run
`tools/kb.sh render`. The `migrating-knowledge` skill walks that move step
by step, from inventory to entries to gates.

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
| `.claude/skills/orchestrating`, `finishing-a-feature`, `migrating-knowledge` | `.claude/schemas/deliverables.json`, evals |
| `.githooks/commit-msg` | `.github/workflows/knowledge.yml`, `CLAUDE.md`, settings |

`update` replaces `.githooks/commit-msg` unconditionally, since it is
kit-owned; set `git config core.hooksPath .githooks` once (if you have not
already) to activate its commit-msg trailer gate.

Do not hand-edit kit-owned files or the generated `.claude/rules/*.md` and
`.claude/skills/project-knowledge/SKILL.md` — edits are lost on the next
`update` or `render`.

`update` also reports version drift: it prints one line naming the version
your stamp had and the version the run just synced to, `kit <old> -> <new>`
(an unchanged version prints the same shape with both sides equal). A stamp
from before the version field prints `kit none -> <new>`. When houserules
ships a new release, adopt it this way:

1. Bump the `houserules` dependency to the new release and install it
   (`pnpm install`, or your package manager's equivalent).
2. Run `pnpm exec houserules update --dir .` (or your package manager's
   exec equivalent).
3. Review the diff (`git diff`). The drift line is your check: it confirms
   the version you moved from and the version you landed on.

## Daily commands

```sh
tools/kb.sh topics | index --topic process | get <id> | for <path> | standing
tools/kb.sh render | check | audit --base origin/main | validate <report.json>
tools/backlog.sh list --open | get WI-001 | batch 1 | set WI-001 status=done batch=1 | check
```

## Development (this repository)

```sh
mise run setup    # pnpm install, activates the commit-msg hook (trailer gate + commitlint)
mise run test     # vitest with coverage
mise run lint     # shellcheck, kb check, backlog check
```

Package operations go through pnpm only (`pnpm add --save-exact …`); tool
versions are pinned in `mise.toml`. This repository runs its own kit (id
prefix `HR`): `template/` is the source, the root `tools/`, `.claude/agents/`,
and `.claude/skills/` are the installed copy. Edit `template/`, then run
`node bin/houserules.mjs update --dir .`; `tests/dogfood.test.mjs` pins the
copies to their sources. `knowledge/` and `backlog/` at the root are this
repository's own rules and work items.

## License

MIT — see [LICENSE](LICENSE). The files that `init` and `update` write into
your project are yours under the same terms; the kit-owned scripts carry an
SPDX header, so vendored copies keep the notice. Not published to npm or any
plugin marketplace yet (owner decision pending, `docs/design.md` §5.3).
