# Project docs

This directory holds this project's own documentation: specs, plans,
design notes, and other reference material for people working in this
repository.

Houserules ships no starter content beyond this file. Add real docs as
the project needs them.

## Taking ownership of a kit-shipped file or knowledge entry

`houserules update` keeps kit-owned files and the packs of the kit in
sync with the running kit, but never overwrites a change this project
made. Two lists record what the project owns, each for its own kind of
thing:

- **Files**: `ownedPaths` in `.houserules.json`.
- **Entries of a pack**: `knowledge/overrides.json`.

### Files: `ownedPaths`

`update` rewrites a kit-owned file, the knowledge schema, and the CI
workflow only while the file still holds the bytes that `update` last
wrote (the hash under `baselines` in `.houserules.json`). It keeps a file
that the project changed, and lists it: `kept <file> (locally modified)`.

To keep a file for good, list its path in `ownedPaths`:

```json
{
  "ownedPaths": ["tools/houserules-step.sh", ".github/workflows/knowledge.yml"]
}
```

A listed path governs the whole file, whichever kind it is: a kit-owned
file, the knowledge schema, the CI workflow, or any other file houserules
seeds (a data file of the tracker, a file of the reference workflow, `AGENTS.md`, this file). `update`
never writes, replaces, or recreates a listed file again, whatever the kit
ships for it next, and it prints no line for it. `init` never seeds it.
The files and blocks that `render` generates follow the knowledge base, not
`ownedPaths`: `render` still writes the houserules block of a listed
`AGENTS.md` and prints `AGENTS.md: written`. A
kit-owned file listed here is also out of the citation check that
"Entries that kit-owned files cite" below describes.

Delete a path from `ownedPaths` to hand the file back to the kit. The next
`update` then treats a changed file as a plain divergence again: it keeps
the file and lists it.

### Entries: packs, overrides, and exclusions

The kit ships its knowledge entries as packs. `knowledge/packs/<pack>.json`
holds one copy of each installed pack, and `.houserules.json` pins each
copy under `packs`. `houserules pack list` lists them. The entries of
this project live in its own topic files in `knowledge/`.

Never edit a pack copy. `houserules check-knowledge` fails a copy that
differs from its pin, and the line names the pack. Put the change in
`knowledge/overrides.json`:

```json
{
  "schema": "houserules/overrides@1.0.0",
  "overrides": [
    {
      "id": "knowledge-base.summary-is-the-rule",
      "fields": { "tags": ["knowledge", "authoring", "ruled-keep"] }
    }
  ],
  "exclusions": [
    {
      "id": "security-hygiene.no-focused-tests",
      "reason": "This project's test runner has no focused tests."
    }
  ]
}
```

- An **override** replaces whole fields of one pack entry. It can name
  any field of the knowledge schema except `id`. It replaces an array
  whole: give the full `tags` list. A field set to `null` removes the
  field, when the schema allows the field to be absent.
- An **exclusion** drops one pack entry from this project. The optional
  `reason` says why.
- To retire a pack entry, override its `status` with `retired`.
  `houserules archive` moves the entries of the project's own topic files
  only. A retired pack entry stays in its pack copy.
- `check-knowledge` lists every override and exclusion. It fails an
  override or exclusion of an id that no installed pack ships, and an id
  that has two overrides, or an override and an exclusion.
- Delete an override or an exclusion to take the text of the pack back.
- After you change `knowledge/overrides.json`, run `houserules render`:
  an override or an exclusion can change the generated rules.

`houserules pack add <name>` installs a kit pack, `houserules pack remove
<name>` removes one, and `houserules pack upgrade <name> <tag>` moves a
pack from a git source to a new tag; `houserules pack upgrade <name>`
copies a pack from a file source again. `update` moves each kit pack to the
version of the running kit and rewrites its copy. It keeps
`knowledge/overrides.json`. When a new version of a pack drops an id that
the file names, `check-knowledge` names the id: delete that override or
exclusion.

## Pruning the knowledge base

When a body of work closes, `houserules stats --window 5 <WORKSPACE>...` over
the folders of deliverables reads the five most recent and prints a
`proposals` key. Each row
proposes to `demote`, `retire`, `mechanize`, or `narrow` an entry, or names
the budget of the standing set. The person who runs the work applies the
rows with `owner_gate: false` and sends the other rows to the project's
owner or decider. When the owner or decider rules to keep an entry that a row
names, tag the entry `ruled-keep`: it then gets no `demote`, `retire`,
`narrow`, or `mechanize` row, and it still counts toward the budget. Record
a change to a pack entry that the project retires (`status`), demotes
(`standing`), rewrites, or tags as an override in
`knowledge/overrides.json` (see above). Edit an entry of the project's own
topic files in place.

## Entries that kit-owned files cite

Kit-owned files cite entries of the packs that the kit ships.
`houserules check-knowledge` reports each cited entry that is not an
active entry of the knowledge base, and it names the citing file. The
report says what to do:

- The project excludes the entry, or an override sets its `status` to
  `retired` or `superseded`: remove the exclusion or the status, or
  replace it with an override that carries the project's own text.
- The pack is not installed: run `houserules pack add <pack>`.
- The installed pack does not ship the entry as an active entry: run
  `houserules update`.
- Another pack holds the name of the kit pack that ships the entry: the
  kit pack cannot be installed. Remove that pack with `houserules pack
  remove <pack>`, then run `houserules pack add <kit pack>`.
- No pack of the kit ships the entry: the file holds the text of an older
  kit. Run `houserules update`. When `update` prints `kept <file> (locally
  modified)`, remove the citation from the file, or list the file in
  `ownedPaths`.

A kit-owned file listed in `ownedPaths` is not checked for the knowledge
ids it cites.

A pack entry that a kit-owned file cites cannot stay excluded or retired
while that file is checked: restore the entry, or list each citing file
in `ownedPaths`. Listing the files keeps the exclusion or the status.
