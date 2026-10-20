# Upgrading to houserules 2.0

Upgrade an install of houserules 1.x with one command:

```sh
houserules update
```

`update` installs the newest binary where the install channel supports it, converts your install to the 2.0 layout, and prints one summary: the files it wrote, the files it kept, and the hand steps that remain. A second `update` prints no change.

Upgrade in this order:

1. Finish the work that runs. Commit it.
2. Run `houserules update`.
3. Read the summary. Do each hand step that it prints (see [Hand steps](#hand-steps)).
4. Run `houserules check-knowledge` and `houserules check-backlog`.
5. Commit the migration on its own, outside a batch branch.

`update` also rewrites the kit's agent templates (`.claude/agents/implementer.md`, `.claude/agents/task-reviewer.md`) when you use them. An `audit` over the migration commit then fails `process.evals-rerun`, because your `.claude/evals/record.json` did not change with them. The kit release carries the eval run of its templates. Run your `.claude/evals/` scenarios and append the run to `.claude/evals/record.json` when you want your own record, or pass `--sanctioned process.evals-rerun=<ref>` to that one `audit`: the row keeps its `fail` and is marked sanctioned.

## The knowledge layer

### Packs

houserules 1.x seeded the kit's rules into your topic files. houserules 2.0 ships them as five packs: `knowledge-base`, `process`, `quality`, `security-hygiene`, and `writing-style`. A pack is one versioned file of entries. Your own entries stay in `knowledge/<topic>.json`. They are your local pack.

After `update`:

- `knowledge/packs/<pack>.json` holds one copy of each installed pack.
- `.houserules.json` pins each copy under `packs`: its `version`, its `source`, its `sha256`, and the areas it added (`addedAreas`). `source` is `kit` for a pack that the binary carries.
- A seeded entry that you never changed has left your topic file. A topic file that held nothing else is gone.
- An entry that you changed is now an override. An entry that you deleted is now an exclusion. An entry that you archived is now an override that sets `status` to `retired`.
- The entry ids of the 1.x `overrides` key of `.houserules.json` moved to `knowledge/overrides.json`: a listed entry that differs from the pack entry became an override, and a listed entry that you deleted became an exclusion.
- The id-keyed `baselines` of `.houserules.json` are gone. The path baselines stay.

Manage packs with `houserules pack`:

```sh
houserules pack list                                       # installed packs, and kit packs you lack
houserules pack add <name>                                 # a kit pack, at the version of the binary
houserules pack add git+<url>@<tag>#<path>                 # an upstream pack, at an exact tag
houserules pack add <file>                                 # a pack file inside your install
houserules pack upgrade <name> <tag>                       # move an upstream pack to a new tag
houserules pack upgrade <name>                             # copy a file pack again
houserules pack remove <name>                              # delete a pack's copy and pin
```

Rules for packs:

- A kit pack moves with `houserules update`. `update` moves the pin of a kit pack to the version of the binary, rewrites its copy, and keeps your overrides and exclusions. `pack upgrade` refuses a kit pack.
- Never edit a copy in `knowledge/packs/`. `check-knowledge` fails a copy that differs from its pin: `knowledge/packs/<pack>.json: the pack "<pack>" differs from its pin in .houserules.json; put your change in knowledge/overrides.json and restore the copy`. Deleting the copy of a kit pack and running `update` restores it.
- An id has one source. When a pack ships an id that one of your topic files holds, `update` and `pack add` turn your entry into an override, or drop it when it equals the pack entry. `pack add` refuses a pack that ships an id of another installed pack.
- `pack add process` refuses on an install that has no `backlog/` directory.
- Every `pack` command that changes a file then runs `render`.

### Overrides

`knowledge/overrides.json` holds your changes to pack entries:

```json
{
  "schema": "houserules/overrides@1.0.0",
  "overrides": [
    {
      "id": "process.model-policy",
      "fields": {
        "summary": "Run every review on the strongest model that the budget allows.",
        "standing": true
      }
    },
    {
      "id": "quality.principles",
      "fields": { "standing": null }
    }
  ],
  "exclusions": [
    {
      "id": "security-hygiene.no-focused-tests",
      "reason": "This project has no test runner that supports focused tests."
    }
  ]
}
```

- An override replaces whole fields of one entry. It can name any field of the knowledge schema except `id`. It replaces an array whole: give the full `tags` list.
- A field set to `null` removes the field. Only a field that the schema allows to be absent, such as `standing`, can go.
- To retire a pack entry, override its `status` with `retired` (or `superseded`). `for`, `standing`, `index`, `audit`, and `render` then leave the entry out, and `get` still prints it. `houserules archive` moves entries of your topic files only.
- `get <id>` prints the effective entry and an `overridden` field that lists the replaced fields.
- `check-knowledge` prints every override and exclusion. The list does not fail the gate.
- `check-knowledge` fails an override or exclusion of an id that no installed pack ships, of an id of your local pack, or of an id that has two overrides or an override and an exclusion. The line names the id and the case.
- Delete an override to take the pack's text back.

The name `overrides` is reserved: a topic file cannot carry it.

### Exclusions

An exclusion drops one pack entry from your install. The optional `reason` explains why.

- `for`, `standing`, `index`, `audit`, and `render` leave an excluded entry out. `get` prints it with an `excluded` field that holds the reason, or `true` when there is none.
- A `see` link to an excluded id is not dangling.
- A kit-owned file that cites an excluded id fails `check-knowledge`: `<file>: cites "<id>", which knowledge/overrides.json excludes; remove the exclusion, or replace it with an override that carries your own text`. You can also list the file in `ownedPaths`.
- `pack remove` refuses while `overrides.json` names an id of that pack. Remove those overrides and exclusions first.

### ownedPaths

`ownedPaths` in `.houserules.json` lists the files that you own. The file paths of the 1.x `overrides` key moved there, and the key is gone.

```json
{
  "ownedPaths": ["tools/claude-session-start.sh", ".github/workflows/knowledge.yml"]
}
```

- `update` never writes, replaces, or recreates a listed path, whatever the kit ships next. It prints no line for it.
- `init` never seeds a listed path.
- `check-knowledge` does not check a listed kit-owned file for citations.
- A path can be a kit-owned file, a seeded file (a backlog file, `AGENTS.md`, `docs/README.md`, an eval scenario), the knowledge schema, the CI workflow, or `.claude/settings.json`.
- The files that `render` generates follow the knowledge base, not `ownedPaths`.
- A `knowledge/<topic>.json` in `ownedPaths` keeps all its entries local. The conversion installs no pack for that topic and prints one line: `knowledge/<topic>.json is in ownedPaths, so the conversion leaves it as it is and does not install the kit pack <pack>; run houserules pack add <pack> to install it`.

### Areas

`knowledge/areas.json` is the one list of areas, and your project owns it. The `area` enum is gone from `knowledge/schema.json`. `check-knowledge` checks the area of every active entry against the keys of `areas.json`. An archived entry keeps its area as plain text.

```json
{
  "docs": { "paths": ["docs/**", "AGENTS.md", "knowledge/**"] },
  "template": { "paths": ["template/**"], "agentsMd": false }
}
```

- `paths` holds the globs of the area. `"agentsMd": false` turns off the directory `AGENTS.md` block of the area (see [Render targets](#render-targets)).
- `pack add`, and an `update` that brings a new pack version, add each area that the pack declares and `areas.json` lacks, with the globs of the pack. They record the area in the `addedAreas` of the pin. An area that already exists keeps your globs.
- `pack remove`, and an `update` in which a pack drops an area, remove an area only when a pack added it, no remaining pack declares it, and no active entry uses it.
- The conversion records each area that it adds, and no area that existed before.
- `check-knowledge` does not fail a glob that matches no file while a pack added the area and the globs still equal the pack's globs. `pack add` and `update` name such a glob.

### Render targets

`render` writes through one renderer for each target.

**The AGENTS.md renderer is always on.**

- It owns only the block between `<!-- houserules:begin -->` and `<!-- houserules:end -->`. A file with no markers gets the block at its end. Once the markers exist, the block stays where you put it. After `update`, expect this block at the end of your `AGENTS.md`.
- The block of the root `AGENTS.md` holds the standing rules and one section for each area. A section names the globs of the area and the command `houserules for <path>`. It never carries the rules of an area.
- An area whose globs share one directory below the install root gets a block with its rules in the `AGENTS.md` of that directory. An area with `"agentsMd": false` gets none.
- `render` removes a block that it no longer writes. It deletes the file only when nothing but whitespace is left. `render --check` fails while such a block is left.
- A marker with no pair, or two blocks in one file, is one line that names the file.

**The Claude renderer is opt-in.**

- `houserules init --target claude` enables it. It adds `claude` to `renderers` in `.houserules.json` and seeds the Claude Code files that are missing: `.claude/agents/*.md`, `tools/claude-session-start.sh`, and the hook entry in `.claude/settings.json`. It keeps every file that exists. `render` then writes `.claude/rules/<area>.md` and `.claude/skills/project-knowledge/SKILL.md`.
- Without `--target claude`, `init` installs none of these files, and `update` writes none of them.
- The conversion writes `renderers` as `["claude"]`, so a converted install keeps what it had. `update` never adds a renderer.
- Claude Code reads the standing rules of the `AGENTS.md` block through the `@AGENTS.md` import of `CLAUDE.md`.

### standing-rules.md is gone

`.claude/rules/standing-rules.md` no longer exists. `update` removes it. The standing rules now live in the block of the root `AGENTS.md`, once. The `project-knowledge` skill no longer lists them.

Claude Code reads `AGENTS.md` only through `CLAUDE.md`. When your `CLAUDE.md` lacks the line `@AGENTS.md`, `update` prints a hand step ([Hand step: the CLAUDE.md import](#hand-step-the-claudemd-import)), and `check-knowledge` fails: `CLAUDE.md: the standing rules do not reach Claude Code; add @AGENTS.md to CLAUDE.md`.

### Gates stop on an install of 1.x

A 2.0 binary refuses an install of houserules 1.x. On a tree whose `.houserules.json` has no `schema` key, every command but `update` and `files` prints one line and exits 2. That includes `init`, which changes nothing, because writing a 2.0 install over a 1.x tree would skip the conversion.

```text
<path>/.houserules.json: this is an install of houserules 1.x; run houserules update to convert it
```

The fix is one command: `houserules update`.

The CI workflow of an install seeded by houserules 0.3.0, 1.0.0, or 1.1.0 installs the newest release. From the release of 2.0, its gates exit 2 with this line until you run `update` and commit the result. A workflow seeded by 1.2.0 or later installs the version that `.houserules.json` records, so it keeps the 1.x binary until `update` restamps the file.

A tree with a `knowledge/` directory and no `.houserules.json` gets a different line:

```text
<path>/.houserules.json: not found; run houserules update --id-prefix <prefix> to convert an install of houserules 1.x, or houserules init to start a new one
```

`update` refuses such a tree without `--id-prefix`, so that it never stamps the default prefix on a project with its own. Give the backlog id prefix of your project.

A binary that is older than the install refuses too. When the `schema` of `.houserules.json` is newer than the binary writes, every command but `files` prints a line that names the release it needs. `update` installs that release first.

### Hand steps

`update` does what it can. For the rest, it prints `hand step: ` and a text. Each text below is the line as `update` prints it. A word in angle brackets, such as `<path>`, stands for a value from your project.

Some steps ask you to compare a file with the one that `houserules init` writes. Make that reference install once, outside your repository:

```sh
git init -q ../kit-reference
git -C ../kit-reference remote add origin https://github.com/example/reference.git
houserules init --dir ../kit-reference
```

The GitHub origin makes `init` write the CI workflow too. Delete `../kit-reference` when you are done.

#### Hand step: a reserved topic name

`update` stops before it changes a file, and exits 2:

```text
hand step: knowledge/overrides.json is a topic file of houserules 1.x, and houserules 2.0 reserves the name overrides for its own file; move its entries to a topic file with another name, then run houserules update again
```

Your 1.x install has a topic named `overrides`. 2.0 keeps `knowledge/overrides.json` for your overrides and exclusions.

1. Choose a new topic name, for example `project-overrides`.
2. Rename the file to `knowledge/project-overrides.json`. Set its `topic` field to the new name.
3. Change the prefix of every id in it from `overrides.` to the new name. An id starts with the name of its topic file.
4. Change each `see` link, `verify` path, and citation of the old ids in your project.
5. Run `houserules update` again.

#### Hand step: the CLAUDE.md import

```text
hand step: add @AGENTS.md to CLAUDE.md: without the import, the standing rules no longer reach Claude Code
```

Add a line with `@AGENTS.md` to `CLAUDE.md`. 1.x gave Claude Code the standing rules in `.claude/rules/standing-rules.md`. 2.0 gives them in the block of `AGENTS.md`, and Claude Code reads that file only through the import. `check-knowledge` fails until the line is there.

#### Hand step: a schema that you own, with the area enum

```text
hand step: remove $defs.area from knowledge/schema.json; your areas live in knowledge/areas.json. Then replace the $ref of its area property with <area-property>
```

`knowledge/schema.json` is in `ownedPaths`, so `update` does not rewrite it. It still holds the area enum of 1.x. Do both edits:

1. Delete the object `$defs.area`. It is the list of area names.
2. In `$defs.entry.properties.area`, replace `{ "$ref": "#/$defs/area" }` with the text that `update` printed in place of `<area-property>`. For the schema of the kit it is `{"type":"string","minLength":1}`.

With the enum in place, `check-knowledge` names the enum as the cause when a pack adds an area that the enum lacks.

#### Hand step: a schema with other changes

```text
hand step: knowledge/schema.json holds changes beyond the area enum, so houserules update removed $defs.area and the $ref of its area property and nothing else; compare the file with the schema that houserules init writes, and merge the rest by hand
```

`update` already made the two edits. Your other changes are still in the file. Compare it with the schema of the kit and merge by hand:

```sh
diff knowledge/schema.json ../kit-reference/knowledge/schema.json
```

#### Hand step: a different schema without an area enum

```text
hand step: knowledge/schema.json differs from the schema that houserules init writes and holds no area enum to remove, so houserules update kept it; compare the two files, and merge the rest by hand
```

`update` kept the file as it is. Compare it with the schema of the kit, as in the step above, and merge the differences you want.

#### Hand step: a schema that is not a JSON object

```text
hand step: knowledge/schema.json is no JSON object, so houserules update kept it; replace it with the schema that houserules init writes
```

Copy `../kit-reference/knowledge/schema.json` over `knowledge/schema.json`.

#### Hand step: a CI workflow of your own

```text
hand step: <path> differs from every workflow that houserules 1.x released, so houserules update kept it; compare it with the workflow that houserules init writes, and merge the rest by hand
```

`<path>` is the workflow file, `.github/workflows/knowledge.yml` for an install at the top level of the repository. `update` replaces a workflow that equals one of the workflows of houserules 1.x. It keeps yours. Compare it with the workflow of the kit and merge by hand:

```sh
diff .github/workflows/knowledge.yml ../kit-reference/.github/workflows/knowledge.yml
```

To keep your workflow for good, list its path in `ownedPaths`.

#### Hand step: two packs ship one id

A new version of a kit pack ships an id that an installed upstream pack ships too. `update` keeps the kit pack at its old version and copy. The text depends on how you installed the other pack.

An upstream pack from a git source:

```text
hand step: pack <kit> <new> ships <id>, which the installed pack <other> ships too, so houserules update kept <kit> at <old>; move <other> to a version that drops the id with houserules pack upgrade <other> <tag>, or run houserules pack remove <other>, then run houserules update again
```

Run `houserules pack upgrade <other> <tag>` with a tag whose version drops the id, or remove the pack.

An upstream pack from a file:

```text
hand step: pack <kit> <new> ships <id>, which the installed pack <other> ships too, so houserules update kept <kit> at <old>; edit <path> to drop the id and run houserules pack upgrade <other>, or run houserules pack remove <other>, then run houserules update again
```

Edit the file at `<path>` to drop the id and run `houserules pack upgrade <other>`, or remove the pack.

A pack with no upstream source to move:

```text
hand step: pack <kit> <new> ships <id>, which the installed pack <other> ships too, so houserules update kept <kit> at <old>; run houserules pack remove <other>, then run houserules update again
```

Remove the other pack.

In each case, run `houserules update` again. It tests the state again and moves the pin of the kit pack.

## The commit-msg hook

The commit-msg hook finds your install root from the path of its own file: the parent of the directory that holds it. A copy in `.git/hooks` has the parent `.git`, which holds no `.houserules.json`. That copy now skips the check with one line, and the commit goes through:

```text
commit-msg: no .houserules.json in the parent of the hook directory; check-commit skipped
```

Activate the hook where it lives. For an install at the top level of the repository, run:

```sh
git config core.hooksPath .githooks
```

For an install in a subdirectory, run:

```sh
git config core.hooksPath <subdir>/.githooks
```

houserules never sets `core.hooksPath` for you.

If a hook manager already owns your hooks, call the hook from the `commit-msg` hook of the manager in one line. The manager passes the path of the commit message file:

```sh
sh <subdir>/.githooks/commit-msg "$1"
```

Use `.githooks/commit-msg` for an install at the top level.
