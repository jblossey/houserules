---
name: migrating-knowledge
description: Use to migrate existing knowledge — AGENTS.md or CLAUDE.md prose, docs, wikis, code comments, PR templates, lint and CI configs — into `knowledge/*.json`, after `init` in a codebase that already has it, or when adopting houserules in an existing project.
---

# Migrating existing knowledge

`init` seeds a generic rule set (process, quality, security-hygiene,
writing-style, knowledge-base) and an empty set of project topics. Most of
the seed is `standing` and loads every session; some of it is
case-dependent instead, scoped to an area and loading only when a matching
file is read — both kinds are already there, not just the standing ones.
Every project-specific rule you already live by is still scattered across
AGENTS.md, docs, code, past conversations, and whatever only lives in an
agent's own memory. This skill moves it in. Knowledge that exists only
in memory is the most fragile source: write it down explicitly, the same
turn you notice it (`process.keep-knowledge-current`), before it is lost
to the next compaction.

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

`AGENTS.md` is seed-once: neither `init` nor `update` overwrites a file
that is already there. `init` reports `kept AGENTS.md`; `update` leaves
it untouched and prints no line for it at all -- absence of any AGENTS.md
line in `update`'s output means it was already there, not that it was
skipped some other way. Merge the two starter sections by hand: run
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
  duplicate instead of filing it — the seed already carries 33 standing
  rules (conventional commits, TDD, exact pins, and more).
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
  `report-field`). Most migrated rules stay text-only.
- State only what the source states (`knowledge-base.state-only-the-source`).
  A paragraph that hints at a rule without stating it is not yet an entry —
  ask, or leave it for the next pass.
- Choose the id with care before you file it: once an entry is merged, its
  id is permanent (`knowledge-base.ids-are-permanent`); a rename adds a new
  entry and links the old one with `see`.

## Areas

An entry's `area` scopes which files load its rule. Before you file the
first entry for a new area, extend `$defs.area.enum` in
`knowledge/schema.json` and the matching globs in `knowledge/areas.json`
together, in the same commit — one without the other fails validation or
never loads.

Mark a migrated non-negotiable `standing: true`; it loads in every session
instead of only when a matching file is read. `houserules check-knowledge` allows
it only for kind `rule` or `invariant` in area `global` or `process` —
leave everything else area-scoped so it loads with its files.

## Migrate topic by topic

Work one `knowledge/<topic>.json` at a time:

1. Pick a source: one AGENTS.md section, one doc, one comment cluster.
2. For each paragraph, write an entry, or park it when it is not worth one
   yet. Parks are hand-edited into a group in `backlog/parked.json` — no
   CLI creates one:
   ```json
   { "batch": <n>, "intro": "...", "items": [
     { "id": "PP-<batch>-<nn>", "text": "<the rule>. Trigger: <what makes it worth an entry>" }
   ] }
   ```
   The text ends with the trigger, so the park is re-openable later.
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
houserules check-backlog
```

Fix every failure before the next topic. The next branch audit checks the
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
  merge queue? Record it as a `process.*` rule (`standing: true`);
  `finishing-a-feature`'s merge step follows the shipped fast-forward-only
  default until this entry says otherwise.
- **Commit attribution** - may a commit or PR description carry a
  co-author, session, or tool-attribution trailer? Record it as a
  `security-hygiene.*` rule; the seeded commit-msg hook enforces only
  Conventional Commits until a `commits`-type check on this entry adds an
  attribution gate of your own.
- **Agent parallelism** - may more than one agent dispatch run at once, or
  does every dispatch wait for the last one to close? Record it as a
  `process.*` rule; the `orchestrating` skill defaults to sequential
  dispatch until this entry rules otherwise.

Answer all three before the first real batch. A project that never rules
them is choosing the shipped defaults (ff-only CLI merges, no attribution
trailers, sequential dispatch) by omission, not by decision - ruling them
explicitly, even to keep the default, is what lets a later reader tell the
two apart.

## Generate your own CI gate

`init` seeds `.github/workflows/knowledge.yml` only for a GitHub-hosted
`origin`; every other host gets a printed skip note and nothing written,
since a GitHub Actions workflow runs nowhere else. Generate the equivalent
gate for your own host, right after `init`, fresh project or migration
alike: install `houserules` in
the CI image, then run the same three steps the seeded workflow runs —
`houserules check-knowledge`, `houserules check-backlog`, and, on a
pull/merge request, `houserules audit --base <the request's base sha>
--head <the request's head sha>` — on every push and pull/merge request.
Name the job and its trigger once written, so the wiring is a claim
someone can grep against (`process.wiring-checks-run-the-resolution`), and
prove it with a real run before trusting it, not a syntax check alone.

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
  "tags": ["payments", "review"],
  "source": { "date": "2026-09-02", "by": "docs", "ref": "AGENTS.md, migrated" },
  "verify": ["src/payments/"]
}
```

One paragraph, one entry: the summary states the rule alone, the body
carries both reasons the source gave plus the fallback, and `verify`
points at the code the rule protects. The entry is `standing: true`: a
review-before-merge rule is a non-negotiable, and area `process` carries
no file globs, so only `standing` gives the entry a loading path. Delete
the AGENTS.md paragraph once `houserules render` and both checks pass.
