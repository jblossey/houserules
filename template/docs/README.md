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
  eval scenario, a workflow, `CLAUDE.md`, and the rest). Listing one
  silences every report for that file -- modified, deleted, or a
  missing file's backfill -- and `update` never writes, replaces, or
  recreates it again, whatever the kit ships for it next.
- A **knowledge-entry id** governs one entry inside a knowledge-topic
  file that is otherwise still reconciled normally: listing one
  silences that entry's own modified or deleted report, without
  touching any other entry in the same file.

Delete a path or id from `overrides` to hand ownership back to the
kit; the next `update` treats it as a plain, unowned divergence again.
