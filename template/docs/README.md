# Project docs

This directory holds this project's own documentation: specs, plans,
design notes, and other reference material for people working in this
repository.

Houserules ships no starter content beyond this file. Add real docs as
the project needs them.

## Taking ownership of a kit-shipped file or knowledge entry

`houserules update` keeps kit-owned files and kit-shipped knowledge
entries in sync with the running kit, but never overwrites one this
project has changed: a file or entry that no longer matches the hash
`.houserules.json`'s `baselines` map recorded for it is kept as-is and
reported once.

`.houserules.json`'s `overrides` list takes two different kinds of
entry, and each governs its own case:

```json
{
  "overrides": ["tools/claude-session-start.sh", "process.ask-when-missing"]
}
```

- A **file path** governs the whole file, whichever kind it is: a
  kit-owned file, an entire knowledge-topic file under `knowledge/`,
  or any other file houserules seeds (a backlog file, a schema, an
  eval scenario, a workflow, `AGENTS.md`, and the rest). Listing one
  silences every report for that file -- modified, deleted, or a
  missing file's backfill -- and `update` never writes, replaces, or
  recreates it again, whatever the kit ships for it next. A kit-owned
  file listed here is also out of the citation check that "Entries
  that kit-owned files cite" below describes.
- A **knowledge-entry id** governs one entry inside a knowledge-topic
  file that is otherwise still reconciled normally: listing one
  silences that entry's own modified or deleted report, without
  touching any other entry in the same file. An entry `houserules
  archive` moves out of the active set looks deleted to `update`:
  list its id here after the sweep, or every later `update` reports
  it once per run. Then run `houserules check-knowledge`. If a
  kit-owned file cites the entry, the check reports that file: list
  each reported file here too, or restore the entry.

Delete a path or id from `overrides` to hand ownership back to the
kit; the next `update` treats it as a plain, unowned divergence again.
A kit-owned file that you delete from `overrides` is checked for
citations again.

## Pruning the knowledge base at batch close

At every batch close, `houserules stats <WORKSPACE>...` over the five most
recent batch workspaces prints a `proposals` key. Each row proposes to
`demote`, `retire`, `mechanize`, or `narrow` an entry, or names the budget
of the standing set. The `orchestrating` skill applies the rows with
`owner_gate: false` and sends the other rows to the project's owner or
decider. When the owner or decider rules to keep an entry that a row
names, tag the entry `ruled-keep`: it then gets no `demote`, `retire`,
`narrow`, or `mechanize` row, and it still counts toward the budget. List
a kit-shipped entry that the project retires, demotes, rewrites, or tags
in `overrides` (see above), so `houserules update` stays silent about
it.

## Entries that kit-owned files cite

Kit-owned files cite entries of the knowledge topics that the kit
seeds. `houserules check-knowledge` reports each cited entry that the
knowledge base does not hold, and it names the citing file.

A kit-owned file listed in `overrides` is not checked for the
knowledge ids it cites.

A seeded entry that a kit-owned file cites cannot be deleted or
archived while that file is checked: restore the entry, or list each
citing file in `overrides`. The report prints both ways. Listing the
files keeps the deletion or the archive.
