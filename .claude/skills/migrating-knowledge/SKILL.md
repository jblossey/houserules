---
name: migrating-knowledge
description: Use to migrate existing knowledge — AGENTS.md or CLAUDE.md prose, docs, wikis, code comments, PR templates, lint and CI configs — into `knowledge/*.json`, after `init` in a codebase that already has it, or when adopting houserules in an existing project.
---

# Migrating existing knowledge

`init` seeds the `knowledge-base` pack, which holds the rules for writing
entries, and an empty set of project topics. `houserules pack add <name>`
adds the other kit packs: `process`, `quality`, `security-hygiene`, and
`writing-style`. A pack's entries are `standing` and load every session, or
case-dependent: scoped to an area, loading only when a matching file is
read. Every project-specific rule you already live by is still scattered across
AGENTS.md, docs, code, past conversations, and whatever only lives in an
agent's own memory. This skill moves it in. Knowledge that exists only
in memory is the most fragile source: write it down explicitly, the same
turn you notice it, before it is lost to the next compaction.

## When

Run the full migration pass after `init` in a codebase that already carries
knowledge — not on a fresh project, which starts with nothing to migrate.
Two sections below run regardless, fresh project or migration alike, right
after `init`: "Elicit your own operating discipline" and "Generate your own
CI gate" — a fresh project has no existing knowledge to move, but it still
needs its own ruled discipline and its own CI gate. Work the rest topic by
topic and gate after each one (see Gates below); do not migrate everything
before the first check.

## If `init` or `update` kept your own AGENTS.md

`AGENTS.md` is seed-once: neither `init` nor `update` replaces a file
that is already there, and your own text in it stays as it is. `init`
reports `kept AGENTS.md`. `render`, which `init` and `update` run, adds
the houserules block to the end of the file (or rewrites the block where
it already is), and prints `AGENTS.md: written` when it changes the
file. Merge the two starter sections by hand: run
`houserules init --dir <an empty scratch git repo>` somewhere else, open
the `AGENTS.md` it writes there, and copy its `## Knowledge base` and
`## Workflow` sections into your own `AGENTS.md` wherever they fit your
file's own structure. Keep the rest of your file as it is.

If you use Claude Code and have no `CLAUDE.md` yet, add one containing
only `@AGENTS.md` so Claude Code loads the same instructions; skip this
on any other harness, or if your `CLAUDE.md` already points at your
`AGENTS.md` some other way.

If instead you have your own `CLAUDE.md` with real content and no
`AGENTS.md`, `init` (a first-time install) or `update` (an install seeded
before this release) seeds a fresh, generic `AGENTS.md` that knows
nothing about your existing rules and leaves your `CLAUDE.md` exactly as
it is (also seed-once) -- see README's `Updating` section for the exact
`wrote AGENTS.md` line that names this state. Move your `CLAUDE.md`
content into `AGENTS.md` yourself, the same way the rest of this skill
migrates any other source, then replace `CLAUDE.md` with a single
`@AGENTS.md` line.

## Inventory

Knowledge hides in:

- **AGENTS.md** (or a project's own `CLAUDE.md` or other
  project-instructions file) — prose rules, workflow steps, warnings.
- **`docs/`, wikis, READMEs** — design records, decisions, how-tos.
- **Code comments** that state a constraint, not just what the code does.
- **PR and review templates** — the checklist a reviewer runs by hand.
- **Lint and CI configs** — a rule enforced in YAML or a linter plugin is
  still a rule; the entry documents it, the config still enforces it.
- **Past conversations and an agent's own memory** — a fact or
  preference the project has been operating on that was never written to a
  tracked file. Nothing outlives the session until it is.

List the sources before you write the first entry. An inventory that admits
what is left beats a partial migration that looks finished.

## Classify

Pick one `kind` per entry (`knowledge/schema.json`):

- **rule** — an instruction: do this, never do that.
- **invariant** — a property that always holds; a fact, not a command.
- **gotcha** — a non-obvious trap, and how to avoid it.
- **procedure** — an ordered sequence of steps toward one outcome.
- **decision** — a choice made among alternatives, with its reasoning.
- **history** — what happened, and when; link it with `see` from the entry
  whose summary would otherwise need a date.

When a paragraph mixes an instruction with its backstory, split it: the
instruction becomes a rule or procedure; the backstory becomes `body`, or
its own `history` entry when it is long enough to want one.

## Write entries

- Before you write an entry, check `houserules index --topic <t>` or
  `houserules standing` for a seeded rule that already says it. Drop the
  duplicate instead of filing it — the kit packs carry rules for
  conventional commits, TDD, exact pins, and more (`houserules pack list`
  lists the packs, and `houserules pack add <name>` installs one).
- Before you delete a seeded entry, search the kit-owned files for its id
  (`grep -rn '<id>' .claude/agents .claude/skills`; `.claude/agents` exists after
  `houserules init --workflow reference`):
  `houserules check-knowledge` fails when a kit-owned file cites an id the
  knowledge base no longer holds.
- `summary` states the rule in one sentence, ≤ 160 characters, with no
  time-sensitive phrasing (`knowledge-base.summary-is-the-rule`). Dates
  belong in `source.date`.
- `body` carries why, how, and the exceptions the source gave.
- `tags` are lowercase, hyphenated words for retrieval; a schema-required
  field, so give every entry at least one.
- `source` names `date` and `by` (`user`, `review`, `controller`, or
  `docs`); migrated knowledge is usually `by: "docs"` with a `ref` pointing
  at the original location.
- `verify` lists paths that exist in the repository today — the file or
  test that would show the rule broken.
- `check` is optional: add one only where the rule is mechanically
  checkable. It needs `type` and `level` (`fail` or `warn`); see
  `$defs.check.type` in `knowledge/schema.json` for the current list of
  types (`grep-absent`, `commits`, `co-change`, `diff-append-only`,
  `report-field`, `durable-sha`, `runner`). Most migrated rules stay
  text-only.
- State only what the source states (`knowledge-base.state-only-the-source`).
  A paragraph that hints at a rule without stating it is not yet an entry —
  ask, or leave it for the next pass.
- Choose the id with care before you file it: once an entry is merged, its
  id is permanent (`knowledge-base.ids-are-permanent`); a rename adds a new
  entry and links the old one with `see`.

## Areas

An entry's `area` scopes which files load its rule. Before you file the
first entry for a new area, add the area and its globs to
`knowledge/areas.json`, in the same commit — `houserules check-knowledge`
fails an entry whose area is no key of that file, and an area whose globs
match no file loads its rule for no file.

Mark a migrated non-negotiable `standing: true`; it loads in every session
instead of only when a matching file is read. `houserules check-knowledge` allows
it only for kind `rule` or `invariant` in area `global` or `process` —
leave everything else area-scoped so it loads with its files.

Keep a migrated process rule non-standing unless every session needs it.
A non-standing `process` entry still loads: the person who runs the work
reads it at `houserules index --area process`, and an agent receives its id
in the task brief (`knowledge-base.rules-need-a-loading-path`). Tag a rule
that only the acts of the person who runs the work can break `controller`:
a task audit leaves it out of its package, and the branch audit judges it
(`knowledge-base.controller-rules-are-tagged`).

## Migrate topic by topic

Work one `knowledge/<topic>.json` at a time:

1. Pick a source: one AGENTS.md section, one doc, one comment cluster.
2. For each paragraph, write an entry, or park it when it is not worth one
   yet. With a tracker, file the park as a work item in it; without one,
   keep it as a note in `docs/`. The text ends with the trigger
   (`<the rule>. Trigger: <what makes it worth an entry>`), so the park is
   re-openable later. The JSON tracker also keeps hand-edited parks, which
   the section on trackers below describes.
3. Delete the migrated prose — an AGENTS.md paragraph, a doc section that
   only restated the rule. Keep anything that enforces or executes: a
   lint or CI config, a code comment that still explains its code. The
   entry documents it; the config or the comment still does the work.
4. Gate (below), then move to the next topic.

When the pass is done, AGENTS.md keeps only project identity and the two
sections `init` seeded (`## Knowledge base`, `## Workflow`) — everything
else has become an entry, a park, or a deletion, except a lint or CI
config or a comment that still does the work its entry now documents.
`CLAUDE.md` stays a plain `@AGENTS.md` pointer if you use Claude Code;
nothing else belongs there.

## Gates

After every topic:

```sh
houserules render
houserules check-knowledge
```

A project with a tracker also runs the check of its tracker: the
houserules block of `AGENTS.md` lists the command. Fix every failure before
the next topic. The next branch audit checks the
migrated entries the same way it checks any other change.

## Elicit your own operating discipline

The template drops three rules as houserules-repo opinion, not universal:
merge discipline, commit attribution, and agent parallelism. Nothing
enforces a default for any of the three until your project rules one -
every template sentence that says a discipline is "elicited" or "ruled"
means this step. Ask three questions, record each answer as its own
knowledge entry, and cite that entry wherever a skill or template file
names the project's own ruling:

- **Merge discipline** - how does a finished branch reach main: fast-forward
  only from the CLI, a merge commit, a squash merge, or your host's own
  merge queue? Record it as a `process.*` rule (`standing: true`); the
  merge step of the reference workflow follows the shipped fast-forward-only
  default until this entry says otherwise.
- **Commit attribution** - may a commit or PR description carry a
  co-author, session, or tool-attribution trailer? Record it as a
  `security-hygiene.*` rule; `houserules check-commit` enforces only the
  `commits` checks of the installed packs (the Conventional Commits check
  of the `process` pack, for one) until a `commits`-type check on this
  entry adds an attribution gate of your own.
- **Agent parallelism** - may more than one agent run at once, or does
  every handoff wait for the last one to close? Record it as a `process.*`
  rule; the reference workflow defaults to parallel handoffs on tasks the
  plan marks as disjoint file sets, each in its own worktree, and to
  sequential handoffs otherwise, until this entry rules otherwise.

Answer all three before the first change you plan. A project that never
rules them is choosing the shipped defaults (ff-only CLI merges, no
attribution trailers, parallel handoffs on disjoint file sets and
sequential handoffs otherwise) by omission, not by decision - ruling them
explicitly, even to keep the default, is what lets a later reader tell
the two apart.

## Generate your own CI gate

`init` seeds `.github/workflows/knowledge.yml` only for a GitHub-hosted
`origin`; every other host gets a printed skip note and nothing written,
since a GitHub Actions workflow runs nowhere else. Generate the equivalent
gate for your own host, right after `init`, fresh project or migration
alike: install in the CI image the `houserules` release that
`.houserules.json` records in its `version` field, then run the same three
steps the seeded workflow runs — `houserules check-knowledge`,
`houserules check-backlog`, and, on a pull/merge request, `houserules audit
--base <the request's base sha> --head <the request's head sha>` — on every
push and pull/merge request.
The shell installer of release `<version>` is
`https://github.com/jblossey/houserules/releases/download/v<version>/houserules-installer.sh`.
Do not install the latest release: it can run a check that your kit files
do not satisfy until you run `houserules update` and commit the result.
Name the job and its trigger once written, so the wiring is a claim
someone can grep against. Prove it with a real run before trusting it, not
a syntax check alone.

Check out the head of the pull or merge request, not a merge commit, before
the audit when the project uses `runner` checks: `audit` needs the working
tree to be the tree at `--head`. The seeded GitHub workflow does this with
`ref: ${{ github.event.pull_request.head.sha }}` on its checkout step. Add the
install step of each tool of `runners` to the job as well; the seeded workflow
installs none.

## Run an external tool with a runner check

A `runner` check runs a tool that your project names, such as Semgrep or
ast-grep, over the files of a change. The project defines the tool in the
`runners` key of `.houserules.json`: its command, its parser, and one install
line. The check names only the tool key, the files, and the scope:

```json
{
  "runners": {
    "semgrep": {
      "command": ["semgrep", "scan", "--sarif", "--metrics", "off", "--disable-version-check", "--config", "tools/semgrep-rules.yml"],
      "parser": "sarif",
      "install": "python3 -m pip install semgrep==<exact version>"
    }
  }
}
```

```json
{ "type": "runner", "level": "fail", "tool": "semgrep", "files": ["src/**"], "scope": "changed" }
```

Run the tool with no network access by default:

- Use rule files in the repository (`--config <file>`), never a registry
  config: Semgrep sends usage metrics when it pulls rules from the registry.
- Switch the metrics and every other call home off in the command (`--metrics off` and `--disable-version-check` for Semgrep: the version check asks Semgrep's servers on each run).
- Pin an exact version of the tool in the `install` line and in your CI.

Choose the parser by what the tool prints. A tool that prints SARIF 2.1.0 on
stdout (`semgrep scan --sarif`, `ast-grep scan --format sarif`) takes
`"parser": "sarif"`: each result is a finding, and the exit status does not
matter when stdout holds valid SARIF. A tool that exits with a non-zero status
when it finds something takes `"parser": "exit-code"`.
`houserules check-knowledge` names a check whose tool `runners` does not
define, and each malformed entry of `runners`. `houserules audit` runs a
triggered check only when the working tree is the tree at `--head`, and exits
2 with the install line of the tool when the program is not found.

## Read houserules output through files

Tool output can reach you shortened or with parts dropped, and the
transcript shows no sign of it (`knowledge-base.context-proxy-output-is-lossy`).
Write houserules output to a file and read the file with your file-read tool:
`houserules get <ids> > <folder>/knowledge-<task>.json`, then open the file.
The session commands of the houserules block print long output, so the same
holds for them. Never act on output that you cannot read whole:
treat a `<<` marker or a "more items" note as a lost read, and read the source file instead.

## Set up a tracker and move `backlog/` data

`init` installs no tracker. A project chooses one with
`houserules init --tracker json|github|beads`. The tracker holds the
project's work items, and the houserules block of `AGENTS.md` then names the
commands that open and close a session on it. A tracker that the gates cannot
reach is never a pass: the gate prints one line that names the tracker and
the setup step, and fails.

This section is a guide, not a command. No houserules command moves data
between trackers, because the tracker contract has no create operation
besides `houserules defer`. An install keeps its tracker once it has one, so
choose it before you move data. `init --tracker github` and
`init --tracker beads` refuse an install that has a `backlog/` directory:
move the data first, remove `backlog/` (commit the removal), then run `init`.

### The JSON tracker

Setup: `houserules init --tracker json` writes the data files of `backlog/`,
`backlog/schema.json`, and the `tracker` key of `.houserules.json`. An
install of release 1.x that already has a `backlog/` directory gets this
tracker from `houserules update`; there is nothing to move.

The tracker has no command that files a park. Hand-edit a group into
`backlog/parked.json`:

```json
{ "batch": <n>, "intro": "...", "items": [
  { "id": "PP-<batch>-<nn>", "text": "<the rule>. Trigger: <what makes it worth an entry>" }
] }
```

### GitHub Issues

Setup:

1. Install `gh` and run `gh auth login`. houserules never reads or stores a
   token; `gh` keeps its own login.
2. Create the labels that the adapter check names, one command each:
   `gh label create houserules:section --repo <owner>/<name>`, and the same
   for `houserules:batch`, `houserules:amendment`, `houserules:decision`,
   `houserules:parked`, and `houserules:deferred`.
   `houserules check-backlog` names each label that is missing.
3. Run `houserules init --tracker github`. It takes the repository from
   `--repo <owner>/<name>`, else from the `origin` remote.

Move the data from `backlog/`:

- An item becomes an issue. The title is the item title, and the body is
  the item paragraphs, separated by blank lines.
- Labels: `type:<type>`, `batch:<n>`, `section:<s>` (the name of the items
  file), and `tag:<t>` for each tag.
- Status: `open` stays open; `partial` stays open with the label
  `status:partial`; `done` closes as completed; `dropped` closes as not
  planned.
- A milestone is a GitHub milestone. `gh` has no milestone command: create
  one with `gh api repos/<owner>/<name>/milestones -f title=<m>`.
- Fields that no issue field holds (`see`, `rulings`, `label`,
  `extra_lines`, and a deferral's `deferral`) go in a fenced block at the
  end of the body. The fence is `houserules`, and each field is one
  `key: value` line, a list as JSON. A deferral also gets the label
  `houserules:deferred`.
- A batch, an amendment, a decision, and a parked group or item are issues
  with the label `houserules:batch`, `houserules:amendment`,
  `houserules:decision`, or `houserules:parked`. A batch is titled
  `Batch <n>: <summary>` and keeps `kickoff` and `status` in its block. An
  amendment keeps `type`, `label`, and `date` in its block, and its text is
  the body. A parked group also carries `batch:<n>` and `group: true` in its
  block; a parked item carries the `batch:<n>` of its group, its text is the
  body, and its `resolved` flag sits in its block.

Ids change: an issue number replaces each item id. Rewrite the `see` lists,
the `backlog` ids in your reports and reviews, and the ids you cite in
`Work-Item:` trailers. `houserules get <n>` reads an issue back. Run
`houserules check-backlog` when the move is done.

### Beads

Setup:

1. Install `bd`, and pin one release for every machine and for CI.
2. Start a store with `bd init`, in a project where `houserules init` has run.
   Where no `CLAUDE.md` exists, `bd init` writes one of its own, and
   `houserules init` keeps it: `houserules check-knowledge` then fails until
   that file holds `@AGENTS.md`. In a clone of a repository whose store
   exists, run `bd bootstrap` instead: the data lives in `refs/dolt/data`,
   apart from the source branches.
3. Add the custom status and the custom types that the adapter check
   names: `bd config set status.custom partial:active` and
   `bd config set types.custom houserules-section,houserules-batch,houserules-amendment,houserules-decision,houserules-parked`.
   `bd config set` replaces the whole list, so keep what the store already
   lists; `houserules check-backlog` prints the exact step.
4. Run `houserules init --tracker beads`. It reads the issue prefix from
   `bd`, and it refuses when `bd` or the store is missing.

houserules sets `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` on every `bd` it
starts, so an adopter never sends bd usage events through houserules. A user's
own `bd` calls keep bd's own setting.

A CI job needs the data too. The seeded CI workflow of a Beads install installs
`bd` at the pinned release and runs `bd bootstrap` in its fresh clone before
the gates run. A CI job of your own needs the same two steps. End a session
with `bd dolt push`, so the data travels.

Move the data from `backlog/`: create one issue per item, with the fields of
the GitHub list above.

- The native type follows the item type: `feat` is `feature`; `bug` and `fix`
  are `bug`; `chore` is `chore`; `decision` is `decision`; any other type is
  `task`. The label `type:<type>` carries the item type itself.
- Labels: `type:<type>`, `batch:<n>`, `section:<s>`, `milestone:<m>`, and
  `tag:<t>` for each tag.
- Status: `open` is open; `partial` is the custom status `partial`; `done` is
  closed; `dropped` is closed with the close reason `dropped`.
- A `see` entry is a `related` dependency. The block at the end of the
  description holds `rulings`, `label`, `extra_lines`, and `deferral`.
- A batch, an amendment, a decision, and a parked group or item are issues of
  the custom type `houserules-batch`, `houserules-amendment`,
  `houserules-decision`, or `houserules-parked`, with the same block as on
  GitHub. A section record is the custom type `houserules-section`. The
  native type `decision` stays for items of type `decision`.

Ids change here too: rewrite the `see` lists, the ids in reports, and the
`Work-Item:` trailers to the Beads ids. Run `houserules check-backlog` when
the move is done.

## What the reference workflow leaves to you

The reference workflow covers the phases of a change: design, plan, build,
review, live run, and finish. It covers nothing for
debugging, writing new skills, or analysing a past session, because none of the
three is a phase of that lifecycle. A project that wants help with them adds
its own skill or plugin. Nothing in the kit asks for it or breaks it.

## A worked example

Before, in AGENTS.md:

> Every change to `src/payments/` needs a review from someone on the
> payments team before merge — that code moves money, and the linter can't
> catch a sign error in a fee calculation. Ping #payments-oncall if no one
> has looked in a day.

After, in `knowledge/process.json`:

```json
{
  "id": "process.payments-review",
  "kind": "rule",
  "area": "process",
  "standing": true,
  "summary": "A change to `src/payments/` needs a payments-team review before merge.",
  "body": [
    "That code moves money, and the linter can't catch a sign error in a fee calculation.",
    "Ping #payments-oncall if no review lands within a day."
  ],
  "tags": ["payments", "review", "controller"],
  "source": { "date": "2026-09-02", "by": "docs", "ref": "AGENTS.md, migrated" },
  "verify": ["src/payments/"]
}
```

One paragraph, one entry: the summary states the rule alone, the body
carries both reasons the source gave plus the fallback, and `verify`
points at the code the rule protects. The entry is `standing: true`: a
review-before-merge rule is a non-negotiable that every session needs,
and area `process` carries no file globs, so a non-standing entry would
reach only the process index of the person who runs the work and a task
brief that names its id. The entry also carries the tag `controller`: a
merge is a coordinating act, so a task diff cannot break the rule, and a
task audit leaves it out. Delete the AGENTS.md paragraph once `houserules render` and both
checks pass.
