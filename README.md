# houserules

[![CI](https://github.com/jblossey/houserules/actions/workflows/ci.yml/badge.svg)](https://github.com/jblossey/houserules/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/jblossey/houserules?include_prereleases&label=release)](https://github.com/jblossey/houserules/releases)

houserules gives a repository developed with AI coding agents a knowledge
base, a backlog, and an agent workflow that keeps both current. Point
Claude Code (or any harness that reads plain files) at your project;
houserules installs the machinery, and your project supplies the rules.

Every rule, decision, and gotcha your team has learned lives in
`knowledge/*.json`, addressable by id. Every unit of work traces to a
backlog item. Three agent templates carry the rules into every change and
audit the result against them, so the rules stay enforced instead of
drifting into a wiki nobody reads.

houserules is set up in a way that makes your codebase incrementally
collect the rules and knowledge agents need in order to achieve a high
level of quality while at the same time minimizing token usage required
in each step.

The repo harness is based on the assumption that each repository has a
distinct and finite set of inherent knowledge required to achieve zero-
shot or close to zero-shot precision in iterations. With each targeted
and audited rule added, work throughout your code will inferentially
converge toward an almost optimally performing ai-native implementation
ground.

After a few iterations, claude will have accumulated enough rules
to not run into the same old pitfalls over and over again which will save
time and tokens throughout reviews and planning phases. After the
convergence phase (or already during the convergence), you'll be able
to add your own rules which claude will then iteratively enforce, shaping
the codebase in the exact style which you deem perfect.

## What it installs

- **A knowledge base** (`knowledge/*.json`): addressable entries (`id`,
  `kind`, `area`, `summary`, `body`, `tags`, `source`, `see`, `verify`,
  optional deterministic `check`), one JSON file per topic, validated by a
  schema your project owns.
- **A backlog** (`backlog/`): typed work items, one JSON file per section,
  driving every change.
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

## Install

houserules is not on npm yet. Until it is, install the pinned release
tag straight from GitHub:

```sh
pnpm add -D git+https://github.com/jblossey/houserules.git#houserules-v0.2.0-alpha
```

A git spec already pins the exact ref, so there is no `--save-exact` for
`pnpm add` to add. Once houserules is on npm:

```sh
pnpm add -D --save-exact houserules
```

## Quick start

```sh
mkdir my-project && cd my-project && git init
pnpm add -D git+https://github.com/jblossey/houserules.git#houserules-v0.2.0-alpha
pnpm exec houserules init
tools/kb.sh check && tools/backlog.sh check
git add -A && git commit -m 'chore: install houserules knowledge setup'
```

`pnpm add` writes `package.json` itself; no separate `pnpm init` is
needed. `init` writes the kit-owned machinery, seeds your starting
knowledge topics, backlog, schemas, and CI workflow, then runs `render`
and stamps `.houserules.json`. Look at what you got:

```sh
tools/kb.sh topics            # the seeded topics: process, quality, ...
tools/kb.sh get process.tdd   # one entry, in full
```

Restart Claude Code once after the first install (the first
`.claude/agents/` file and the new hook need a fresh session). Set `git
config core.hooksPath .githooks` to activate the commit-msg trailer gate.

## Adding to an existing project

```sh
cd my-project
pnpm add -D git+https://github.com/jblossey/houserules.git#houserules-v0.2.0-alpha
pnpm exec houserules init --id-prefix ABC
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

## Ownership model

Every path houserules writes falls into one of three buckets:

- **Kit-owned** — `update` overwrites it on every run. Never hand-edit it;
  edits are lost on the next `update` or `render`. `.claude/rules/*.md` and
  `.claude/skills/project-knowledge/SKILL.md` are generated by `tools/kb.sh
  render` from `knowledge/`, so they behave the same way.
- **Seed-once** — `init` writes it only if it is absent, then leaves it
  alone. It is yours from the first write on. `.claude/settings.json` is
  the one exception: `init` merges its two SessionStart hook entries into
  an existing file instead of skipping it; `update` never touches the
  file either way.
- **Yours** — everything else: your knowledge entries, your backlog items,
  your project code. houserules never touches it.

`pnpm exec houserules files` prints the exact manifest:

| kit-owned (update overwrites) | seed-once (yours after init) |
|---|---|
| `tools/` CLIs, wrappers, session hook | `knowledge/` schema, areas, topics |
| `.claude/agents/*.md` | `backlog/` schema and data |
| `.claude/skills/orchestrating`, `finishing-a-feature`, `migrating-knowledge` | `.claude/schemas/deliverables.json`, evals |
| `.githooks/commit-msg` | `.github/workflows/knowledge.yml`, `CLAUDE.md`, settings |

## The agent workflow

Three agent templates, each on a different model tier, carry the rules
into every change:

- **implementer** (the cheapest model that fits the task): implements one
  task from a brief, test-driven, and writes a JSON report.
- **task-reviewer** (a stronger model than the implementer it reviews):
  reviews one task's diff for spec compliance, code quality, and rule
  adherence.
- **branch-reviewer** (the strongest model): reviews the whole branch
  before merge and proposes knowledge-base improvements drawn from the
  batch's reviews.

An `orchestrating` skill drives the batch lifecycle — brainstorm or spec,
user gate, plan, sequential dispatch, live run, finish, rollout; a
`finishing-a-feature` skill handles the fast-forward merge; a
`migrating-knowledge` skill moves an existing project's rules into the
kit.

`tools/kb.sh audit --base <ref>` checks a change against the rule package
its files touch. Every dispatched agent runs it against its own diff and
records the result in its JSON report
(`.claude/schemas/deliverables.json`), so rule adherence is checked, not
just asserted.

## Updating an installation

```sh
pnpm exec houserules update
```

`update` overwrites only kit-owned machinery and re-renders; it never
touches project data.

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

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contributor workflow.

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
