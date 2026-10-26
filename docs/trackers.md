# Trackers

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
  calls keep bd's own setting. The CI jobs of a Beads install install `bd`
  and bootstrap the store.

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

## Deferrals

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

## Trailers

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

The JSON result of `audit` holds the same verdict. `references.failures` lists
each failure line, `references.checked` counts the references that the tracker
judged, `references.passed` counts those among them that passed,
`references.skipped` counts those that the audit did not judge (no tracker, a
tracker that cannot be reached, or a tracker that answered with an error), and
`summary.reference_fail` counts the failures. The `backlog/ exists` line and
the line of a tracker that cannot be reached are failures too. `summary.fail`
counts only the rule rows.

The process pack ships `process.every-commit-names-a-work-item`, which fails
a commit that has no `Work-Item:` trailer. It is an opt-in entry. When
a command brings the process pack onto an install, houserules excludes the
entry in `knowledge/overrides.json` with the reason `opt-in`, and prints
`process.every-commit-names-a-work-item is an opt-in entry;
knowledge/overrides.json excludes it until you delete its exclusion`. Delete
the exclusion to opt in. The install then keeps the entry, whatever a later
command does.
