---
name: migrating-knowledge
description: Use to migrate existing knowledge — CLAUDE.md prose, docs, wikis, code comments, PR templates, lint and CI configs — into `knowledge/*.json`, after `init` in a codebase that already has it, or when adopting houserules in an existing project.
---

# Migrating existing knowledge

`init` seeds a generic standing-rule set (process, quality,
security-hygiene, writing-style, knowledge-base) and an empty set of
project topics. Every project-specific rule you already live by is still
scattered across CLAUDE.md, docs, and code. This skill moves it in.

## When

Run this after `init` in a codebase that already carries knowledge — not on
a fresh project, which starts with nothing to migrate. Work topic by topic
and gate after each one (see Gates below); do not migrate everything before
the first check.

## Inventory

Knowledge hides in:

- **CLAUDE.md** and any project-instructions file — prose rules, workflow
  steps, warnings.
- **`docs/`, wikis, READMEs** — design records, decisions, how-tos.
- **Code comments** that state a constraint, not just what the code does.
- **PR and review templates** — the checklist a reviewer runs by hand.
- **Lint and CI configs** — a rule enforced in YAML or a linter plugin is
  still a rule; the entry documents it, the config still enforces it.

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

- Before you write an entry, check `tools/kb.sh index --topic <t>` or
  `tools/kb.sh standing` for a seeded rule that already says it. Drop the
  duplicate instead of filing it — the seed already carries 23 standing
  rules (conventional commits, TDD, exact pins, ff-only merges, and more).
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
instead of only when a matching file is read. `tools/kb.sh check` allows
it only for kind `rule` or `invariant` in area `global` or `process` —
leave everything else area-scoped so it loads with its files.

## Migrate topic by topic

Work one `knowledge/<topic>.json` at a time:

1. Pick a source: one CLAUDE.md section, one doc, one comment cluster.
2. For each paragraph, write an entry, or park it when it is not worth one
   yet. Parks are hand-edited into a group in `backlog/parked.json` — no
   CLI creates one:
   ```json
   { "batch": <n>, "intro": "...", "items": [
     { "id": "PP-<batch>-<nn>", "text": "<the rule>. Trigger: <what makes it worth an entry>" }
   ] }
   ```
   The text ends with the trigger, so the park is re-openable later.
3. Delete the migrated prose — a CLAUDE.md paragraph, a doc section that
   only restated the rule. Keep anything that enforces or executes: a
   lint or CI config, a code comment that still explains its code. The
   entry documents it; the config or the comment still does the work.
4. Gate (below), then move to the next topic.

When the pass is done, CLAUDE.md keeps only project identity and the two
sections `init` seeded (`## Knowledge base`, `## Workflow`) — everything
else has become an entry, a park, or a deletion, except a lint or CI
config or a comment that still does the work its entry now documents.

## Gates

After every topic:

```sh
tools/kb.sh render
tools/kb.sh check
tools/backlog.sh check
```

Fix every failure before the next topic. The next branch audit checks the
migrated entries the same way it checks any other change.

## A worked example

Before, in CLAUDE.md:

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
  "source": { "date": "2026-09-02", "by": "docs", "ref": "CLAUDE.md, migrated" },
  "verify": ["src/payments/"]
}
```

One paragraph, one entry: the summary states the rule alone, the body
carries both reasons the source gave plus the fallback, and `verify`
points at the code the rule protects. The entry is `standing: true`: a
review-before-merge rule is a non-negotiable, and area `process` carries
no file globs, so only `standing` gives the entry a loading path. Delete
the CLAUDE.md paragraph once `tools/kb.sh render` and both checks pass.
