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
- **One dependency-free `houserules` binary**: read commands print JSON;
  `check-knowledge`/`check-backlog` gate lint; `render` generates the
  markdown the harness loads; `audit` checks a git range against its rule
  package; `validate` checks agent deliverables; `stats` aggregates a
  batch's audits.
- **Generated harness files** (`houserules render`): standing rules
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
- **A CI gate** (`.github/workflows/knowledge.yml`): `check-knowledge`,
  `check-backlog`, and a PR audit, through the `houserules` binary alone.

## Install

houserules ships one dependency-free binary: no local clone, no Rust
toolchain, no Node. Install it through any of these channels, then run
`houserules init` in your project.

### The shell installer

<!-- x-release-please-start-version -->
```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/jblossey/houserules/releases/download/v0.2.0-alpha/houserules-installer.sh | sh
```
<!-- x-release-please-end -->

cargo-dist's own installer (the same [release pipeline](docs/runbook.md#cutting-a-release)
publishes it): it picks your platform's archive, checks its sha256, and
installs to `$CARGO_HOME/bin` (falling back to `~/.cargo/bin`), adding
that directory to `PATH` itself (`.profile`, `.zshrc`/`.zshenv`, fish's
`conf.d`, or `$GITHUB_PATH` under CI — no shell restart needed there).

PENDING the first release with assets (HR-048): `jblossey/houserules`
carries one release so far (`houserules-v0.2.0-alpha`, 2026-09-03), and
it ships zero assets; the tag this URL names has no release of
its own. Either way, the URL 404s today. Verified live
2026-09-07 (the releases API listing,
`.superpowers/sdd/2026-09-06-batch-19/t2-evidence/releases-api-listing.json`;
the 404,
`.superpowers/sdd/2026-09-06-batch-19/t2-evidence/curl-installer-404.log`).
Re-run this block once the pipeline cuts a release with assets, and
again once HR-049 re-verifies live (the two-step reality,
docs/specs/2026-09-06-batch-19-phase4.md §7).

### mise

```sh
mise use ubi:jblossey/houserules
```

Resolves the latest GitHub release's archive for your platform through
mise's `ubi` backend, with no registry entry needed; `mise use` with no
`@version` defaults to `@latest` and records that spec in `mise.toml`.
mise's own current docs mark the `ubi` backend deprecated in favor of
`github:owner/repo`, removed in mise 2027.1.0 — `ubi` still resolves and
installs today (HR-070 tracks the migration).

PENDING the first release with assets: the one release that exists
ships no asset for mise's `ubi` backend to resolve. mise's own
diagnostic names a different symptom, `no versions found ... matching
date filter`, not an absent release. Verified live 2026-09-07,
`.superpowers/sdd/2026-09-06-batch-19/t2-evidence/mise-ubi-control-run.log`.

### Direct download

<!-- x-release-please-start-version -->
| Target | Archive |
|---|---|
| Apple Silicon macOS | [houserules-aarch64-apple-darwin.tar.xz](https://github.com/jblossey/houserules/releases/download/v0.2.0-alpha/houserules-aarch64-apple-darwin.tar.xz) |
| Intel macOS | [houserules-x86_64-apple-darwin.tar.xz](https://github.com/jblossey/houserules/releases/download/v0.2.0-alpha/houserules-x86_64-apple-darwin.tar.xz) |
| x64 Windows | [houserules-x86_64-pc-windows-msvc.zip](https://github.com/jblossey/houserules/releases/download/v0.2.0-alpha/houserules-x86_64-pc-windows-msvc.zip) |
| ARM64 Linux (musl) | [houserules-aarch64-unknown-linux-musl.tar.xz](https://github.com/jblossey/houserules/releases/download/v0.2.0-alpha/houserules-aarch64-unknown-linux-musl.tar.xz) |
| x64 Linux (musl) | [houserules-x86_64-unknown-linux-musl.tar.xz](https://github.com/jblossey/houserules/releases/download/v0.2.0-alpha/houserules-x86_64-unknown-linux-musl.tar.xz) |
<!-- x-release-please-end -->

Each archive carries a `.sha256` checksum beside it (append `.sha256` to
the archive's own URL); extract the archive and put the `houserules`
binary on `PATH` yourself.

macOS ships unsigned (docs/specs/2026-09-06-batch-19-phase4.md §3's
ruling: zero cost, revisit at 1.0). A browser download sets the
quarantine flag, and Gatekeeper then refuses to run an unsigned binary;
clear it once:

```sh
xattr -d com.apple.quarantine /path/to/houserules
```

Verified against the documented `xattr -d <attribute> <file>` form
(ss64.com/mac/xattr.html, checked 2026-09-07); this block cannot run
live in this Linux development environment — no macOS host is available
here, a permanent constraint, not a pending-release one.

PENDING the first release with assets: all five listed archives 404
today. Verified live 2026-09-07,
`.superpowers/sdd/2026-09-06-batch-19/t2-evidence/direct-download-404.log`.

## Quick start

```sh
mkdir my-project && cd my-project && git init
houserules init
houserules check-knowledge && houserules check-backlog
git add -A && git commit -m 'chore: install houserules knowledge setup'
```

`init` writes the kit-owned machinery, seeds your starting knowledge
topics, backlog, schemas, and CI workflow, then runs `render` and stamps
`.houserules.json` with the binary's own version — no drift to correct
right after. `houserules update --dir .` stays safe and idempotent to
run any time later; it brings the generated files to whatever binary you
currently have on `PATH` and drops any kit file that binary has since
retired. Look at what you got:

```sh
houserules topics            # the seeded topics: process, quality, ...
houserules get process.tdd   # one entry, in full
```

Restart Claude Code once after the first install (the first
`.claude/agents/` file and the new hook need a fresh session). Set `git
config core.hooksPath .githooks` to activate the commit-msg trailer gate.

## Adding to an existing project

```sh
cd my-project
houserules init --id-prefix ABC
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
`houserules render`. The `migrating-knowledge` skill walks that move step
by step, from inventory to entries to gates.

## Ownership model

Every path houserules writes falls into one of three buckets:

- **Kit-owned** — `update` overwrites it on every run. Never hand-edit it;
  edits are lost on the next `update` or `render`. `.claude/rules/*.md` and
  `.claude/skills/project-knowledge/SKILL.md` are generated by `houserules
  render` from `knowledge/`, so they behave the same way.
- **Seed-once** — `init` writes it only if it is absent, then leaves it
  alone. It is yours from the first write on. `.claude/settings.json` is
  the one exception: `init` merges its two SessionStart hook entries into
  an existing file instead of skipping it; `update` never touches the
  file either way.
- **Yours** — everything else: your knowledge entries, your backlog items,
  your project code. houserules never touches it.

`houserules files` prints the exact manifest:

| kit-owned (update overwrites) | seed-once (yours after init) |
|---|---|
| `tools/claude-session-start.sh` | `knowledge/` schema, areas, topics |
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

`houserules audit --base <ref>` checks a change against the rule package
its files touch. Every dispatched agent runs it against its own diff and
records the result in its JSON report
(`.claude/schemas/deliverables.json`), so rule adherence is checked, not
just asserted.

## Updating an installation

```sh
houserules update
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

1. Install the new release through whichever [Install](#install) channel
   you used originally (the shell installer, `mise`, or a fresh direct
   download over the old binary).
2. Run `houserules update --dir .`.
3. Review the diff (`git diff`). The drift line is your check: it confirms
   the version you moved from and the version you landed on.

## Daily commands

```sh
houserules topics | index --topic process | get <id> | for <path> | standing
houserules render | check-knowledge | audit --base origin/main | validate <report.json>
houserules list --open | get WI-001 | batch 1 | set WI-001 status=done batch=1 | check-backlog
```

## Development (this repository)

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full contributor workflow.

```sh
mise run setup    # activates the commit-msg hook (trailer gate + check-commit)
mise run lint     # shellcheck, cargo deny, check-knowledge, check-backlog, render --check
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Package operations go through cargo only (`cargo add <crate>@=<version>`);
tool versions are pinned in `mise.toml`. This repository runs its own kit (id
prefix `HR`): `template/` is the source, the root `tools/`, `.claude/agents/`,
and `.claude/skills/` are the installed copy. Edit `template/`, then run
`houserules update --dir .` (`mise run houserules -- update --dir .`);
`crates/houserules/tests/dogfood.rs` pins the copies to their sources.
`knowledge/` and `backlog/` at the root are this repository's own rules
and work items.

## License

MIT — see [LICENSE](LICENSE). The files that `init` and `update` write into
your project are yours under the same terms; the kit-owned scripts carry an
SPDX header, so vendored copies keep the notice. Distribution is
binary-only (GitHub Releases, mise via ubi, a curl-to-sh installer);
houserules will not publish to npm or any plugin marketplace (ruled
2026-09-04, docs/design.md §5.22).
