---
name: migrating-knowledge
description: Use to migrate existing knowledge — AGENTS.md or CLAUDE.md prose, docs, wikis, code comments, pull or merge request templates, lint and CI configs — into `knowledge/*.json`, after `init` in a codebase that already has it, or when adopting houserules in an existing project.
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
needs its own ruled discipline and its own CI gate. A project that wires the
review layer answers "Elicit the review settings" as well. Work the rest topic by
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
- **Pull or merge request templates and review templates** — the checklist a
  reviewer runs by hand.
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
- **Commit attribution** - may a commit or the description of a pull or merge
  request carry a co-author, session, or tool-attribution trailer? Record it as
  a `security-hygiene.*` rule; `houserules check-commit` enforces only the
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

`init` writes no CI file, and `update` writes none again. Right after `init`,
fresh project or migration alike, ask which host the project uses (`github` or
`gitlab`; a self-hosted GitHub counts as `github`, and a self-managed GitLab as
`gitlab`), and wire the gates into the pipeline of the project. The jobs run
`houserules check-knowledge`, `houserules check-backlog`, and, on a pull or
merge request, `houserules audit --base <the request's base sha> --head <the
request's head sha>`, on every push and every pull or merge request. They also
run the review layer (see "Wire the review layer" below). `houserules ci` prints
or writes the wiring, and it never edits a CI file that it did not create.
`houserules ci print` gives the first two of three ways, and `houserules ci write`
gives the third:

- **Reference.** `houserules ci print --host github` or `--host gitlab` prints
  the lines that call the unit of this release, and writes nothing. The unit is
  a reusable workflow on GitHub, pinned by the commit sha of the release tag:
  the output names the `git ls-remote` command that resolves the sha. It is a
  file for `include: remote` on GitLab, with an `integrity` hash. Add the lines
  to the pipeline file of the project.
- **Inline.** With `--mode inline`, `houserules ci print` prints the jobs of the
  unit with the inputs of the install filled in. Use it for a project that keeps the
  jobs in its own pipeline file, for a self-hosted GitHub (it cannot call a
  workflow on github.com by default), and for a GitLab that cannot reach
  github.com.
- **A file of its own.** `houserules ci write --host github --sha <sha>`, with
  the commit of the release tag as `<sha>`, or `houserules ci write --host
  gitlab`, creates a new file that holds the same lines and adds its path to
  `review.ci.files`. `--host github --mode inline` needs no `--sha`. It never
  writes into an existing file. `houserules ci write --template` writes the
  template of the pull or merge request, and only where the host reads none
  yet.

`--host` defaults to the host of the `origin` remote when that is github.com or
gitlab.com. Another origin needs the flag. Ask which file of the project's own
pipeline wires houserules, and add its path to `review.ci.files` in
`.houserules.json`: `ci write` adds its own file, and `ci print` ends with the
line that says so. A change to a file of `review.ci.files` then routes to
`senior-review`.

For any other host, write the steps by hand. Install in the CI image the
`houserules` release that `.houserules.json` records in its `version` field,
then run the same three steps. The shell installer of release `<version>` is
`https://github.com/jblossey/houserules/releases/download/v<version>/houserules-installer.sh`.
Do not install the latest release: it can run a check that your kit files do
not satisfy until you run `houserules update` and commit the result. Name the
job and its trigger once written, so the wiring is a claim someone can grep
against. Prove it with a real run before trusting it, not a syntax check alone.

Check out the head of the pull or merge request, not a merge commit, before the
audit when the project uses `runner` checks: `audit` needs the working tree to
be the tree at `--head`. The jobs of the unit do this. Add the install step of
each tool of `runners` to the job as well: pass it to the `setup` input of the
unit, or add it to the job by hand.

## Wire the review layer

The review layer gives each pull or merge request one report and each merge one
record. Each command below is a function of the layer. A command reads the
repository and files. No command talks to the host beyond the tracker that the
install names: the GitHub tracker runs gh, as audit does. The jobs of the unit
talk to the host. Wire the output that a function writes, not the command
alone.

### The evidence packet

`houserules evidence --base <ref> --out <file>` writes the reviewer packet as
Markdown. The packet starts with the uncovered surface: the changed lines that
no check with `covers: true` read. It lists the intent, the failed, warned,
proven, and judged rules, the size of the diff, and the route. With `--audit
<file>`, the result of `audit --json` for the same range, it runs no second
audit. With `--json <file>` it also writes the document
`houserules/evidence@1.0.0`. It exits 0 whatever the audit finds. The jobs of the
unit post the packet as the one comment of the install.

### The route

`houserules route --base <ref> --json <file>` decides `auto-merge-eligible`,
`owner-review`, or `senior-review`. `houserules route --base <ref> --no-audit
--json <file>` computes the floor, which runs no project tool: wire it into a
job that runs no project code, and apply at least the floor. Both commands exit
0 for every route. A project with no `review.route` setting gets `owner-review`
for each change, and `senior-review` for a change to a rule source.

### The evidence bundle

`houserules bundle write` writes the bundle section from the captures of the
final gate run. `houserules bundle check --body <file> --base <ref> --head <ref>`
reads the description of the request from a file and prints a line for each
finding. Its exit status is the gate: 0 for a clean bundle, 1 when it prints a
finding, 2 for a setup error. Pass `--author` and `--head-ref` so that an exempt
author or branch passes. On GitHub the description is a field of the event
file. On GitLab read the request through the API: the predefined variable holds
only the first 2,700 characters.

### The change record

`houserules record write --base <ref> --head <ref> --out <file>` computes the
record of a push to the default branch. Without `--out` it writes a git note
under `refs/notes/houserules` (`refs/notes/houserules-sub/<install path>` for an
install below the top level of the repository). Only the jobs of the unit push
the ref. A clone does not fetch it:
`git fetch origin refs/notes/houserules:refs/notes/houserules`. Then
`houserules record show <sha>` prints the record and checks its rule hashes
against git.

### The intent of a change

The intent of a change is what it was meant to do. `review.intent.sources` lists
the adapters to run, `work-item` and `file`. `review.intent.files` lists the
globs of the files that the `file` adapter reads, for example
`docs/plans/*.md`. The `Intent: <source>:<ref>` line of the bundle names one
intent directly.

### The GitLab token

The job token of GitLab cannot post a note, set a commit status, set a label,
read approvals, or push. The jobs `report` and `record-push` read the CI/CD
variable `HOUSERULES_TOKEN`, which the project creates masked and hidden, with
the environment scope `houserules-<key>` (`<key>` is `root` for an install at
the top level). Only those two jobs name that environment, so the jobs that run
houserules, `setup`, or the code of the request cannot read the token. Prefer a
project access token (the scopes `api` and `write_repository`, the role
Developer). On gitlab.com Free, project access tokens need Premium or Ultimate:
use a personal access token of a dedicated member with the role Developer, and
tell the project that a personal access token reaches every project that its user
can reach. An access token expires after at most 365 days: rotate it. Until then
`report` and `record-push` print their notice and post and push nothing, and the
pipeline stays green.

### The limits of the review layer

State these limits to the project:

- On GitHub, a label or a status that the job token sets starts no workflow run.
  A workflow that acts on the route listens to `workflow_run` of the review
  workflow, not to `labeled`.
- The post-merge sample is unsteerable only under a host merge that writes a new
  commit (a squash, a merge commit, or a rebase on the host). Under a
  fast-forward merge the key of the record is the head of the author, so the
  author knows the draw before the merge.
- A project that needs tools for its runner checks passes the install step to the
  `setup` input of the unit. The job that computes the floor runs no `setup`.
- A request runs its own edited CI definition on both hosts. A request that edits
  the wiring sets its own status and label. The mitigation is a required status
  from a protected-branch rule, plus the rule-source floor: a file in
  `review.ci.files` routes to `senior-review`. A request that edits a file of
  `review.ci.files` also controls its own run, which a required code-owner review
  of those files mitigates (a `CODEOWNERS` rule on GitHub, code owners with
  approval rules on GitLab, a Premium feature).
- `floor-install: runner:<path>` holds only on a fresh runner or an unwritable
  path: a GitHub-hosted runner, a GitLab docker executor, or a runner user that
  cannot write the path. On a reused runner the request could replace the file.
- On GitLab, a description edit starts no pipeline. Run the pipeline again to
  check the new description.

## Elicit the review settings

Ask the project's owner for each setting, as you ask for the merge discipline
above. houserules ships no number. Write the answers to the `review` key of
`.houserules.json`; `houserules check-knowledge` checks the key:

- the diff budget of each risk tier (`review.bundle.budgets`), and the files that
  leave the diff size (`review.diffExclude`);
- the route thresholds: which areas and paths give `auto-merge-eligible` (each
  needs `maxDiffLines` and `maxUncoveredShare`), which give `senior-review`, and
  the score thresholds; and `review.route.sampleRate`, which an
  `auto-merge-eligible` area or path needs;
- the exempt authors and branches (`review.bundle.exemptAuthors` and
  `review.bundle.exemptBranches`), for example a dependency bot and a release
  tool;
- the intent sources and files (`review.intent.sources` and
  `review.intent.files`);
- which checks declare `covers: true`. A check is coverage only when its entry
  declares it, and no entry of the kit does. A change that no covering check
  reads lists every changed line as uncovered.

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
   `bd config set types.custom 'houserules-section,houserules-batch,houserules-amendment,houserules-decision,houserules-parked'`.
   `bd config set` replaces the whole list, so keep what the store already
   lists; `houserules check-backlog` prints the exact step, with a list of
   two or more entries in single quotes for a POSIX shell.
4. Run `houserules init --tracker beads`. It reads the issue prefix from
   `bd`, and it refuses when `bd` or the store is missing.

houserules sets `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` on every `bd` it
starts, so an adopter never sends bd usage events through houserules. A user's
own `bd` calls keep bd's own setting.

A CI job needs the data too. The jobs that `houserules ci print` wires for a
Beads install, and the workflow that an earlier release seeded, install `bd` at
the pinned release and run `bd bootstrap` in the fresh clone before the gates
run. A CI job of your own needs the same two steps. End a session
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
