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

`update` also rewrites the agent templates and eval scenarios of the reference workflow when you use them (see [Trackers and the reference workflow](#trackers-and-the-reference-workflow)). An `audit` over the migration commit then fails `process.evals-rerun`, because your `.claude/evals/record.json` did not change with them. The kit release carries the eval run of its templates. To clear the red, run your `.claude/evals/` scenarios and append the run to `.claude/evals/record.json`. `--sanctioned process.evals-rerun=<ref>` does not clear it. The row keeps its `fail` and is marked sanctioned. `audit` still exits 1, and a CI job that runs it stays red.

## The knowledge layer

### Packs

houserules 1.x seeded the kit's rules into your topic files. houserules 2.0 ships them as five packs: `knowledge-base`, `process`, `quality`, `security-hygiene`, and `writing-style`. A pack is one versioned file of entries. Your own entries stay in `knowledge/<topic>.json`. They are your local pack.

After `update`:

- `knowledge/packs/<pack>.json` holds one copy of each installed pack.
- `.houserules.json` pins each copy under `packs`: its `version`, its `source`, its `sha256`, and the areas it added (`addedAreas`). `source` is `kit` for a pack that the binary carries.
- A seeded entry that you never changed has left your topic file. A topic file that held nothing else is gone.
- An entry that you changed is now an override. An entry that you deleted is now an exclusion. An entry that you archived is now an override that sets `status` to `retired`.
- The entry ids of the 1.x `overrides` key of `.houserules.json` moved to `knowledge/overrides.json`: a listed entry that differs from the pack entry became an override, and a listed entry that you deleted became an exclusion.
- The id-keyed `baselines` of `.houserules.json` are gone. The path baselines stay, except for the files that the reference workflow pins: the pin of a workflow file replaces its baseline, and `update` prints a line such as `removed 5 baselines from .houserules.json; the pins of the workflow replace them`.

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
- `pack add process` refuses on an install that has no tracker: `pack process needs a tracker; run houserules init --tracker json|github|beads`.
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
- A path can be a kit-owned file, a seeded file (a tracker data file, `AGENTS.md`, `docs/README.md`), a file of the reference workflow, a schema, the CI workflow, or `.claude/settings.json`.
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

- `houserules init --target claude` enables it. It adds `claude` to `renderers` in `.houserules.json` and seeds the Claude Code files that are missing: `tools/claude-session-start.sh` and the compaction hook entry in `.claude/settings.json`. Where the install runs the reference workflow, it also adds the hook entry of the workflow. It keeps every file that exists. `render` then writes `.claude/rules/<area>.md` and `.claude/skills/project-knowledge/SKILL.md`. The agent templates in `.claude/agents/` belong to the reference workflow.
- Without `--target claude`, `init` installs none of these files, and `update` writes none of them.
- The conversion writes `renderers` as `["claude"]`, so a converted install keeps what it had. `update` never adds a renderer.
- Claude Code reads the standing rules of the `AGENTS.md` block through the `@AGENTS.md` import of `CLAUDE.md`.

### standing-rules.md is gone

`.claude/rules/standing-rules.md` no longer exists. `update` removes it. The standing rules now live in the block of the root `AGENTS.md`, once. The `project-knowledge` skill no longer lists them.

Claude Code reads `AGENTS.md` only through `CLAUDE.md`. When your `CLAUDE.md` lacks the line `@AGENTS.md`, `update` prints a hand step ([Hand step: the CLAUDE.md import](#hand-step-the-claudemd-import)), and `check-knowledge` fails: `CLAUDE.md: the standing rules do not reach Claude Code; add @AGENTS.md to CLAUDE.md`.

### The startup hook entry is gone

The compaction entry is the only core session-start hook. `update` removes the `SessionStart` entry that has the matcher `startup|resume|clear|fork` and the command `${CLAUDE_PROJECT_DIR}/tools/claude-session-start.sh start` from `.claude/settings.json`, where the file holds it exactly as houserules seeded it. It prints one line:

```text
edited .claude/settings.json: removed the seeded startup hook entry; the kit seeds the compaction entry alone
```

The session ritual moved to the reference workflow, and `tools/claude-session-start.sh` prints nothing for `start`. An entry that you changed stays, and so does a `.claude/settings.json` that `ownedPaths` lists; `update` prints no line for either.

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

## Trackers and the reference workflow

houserules 2.0 moves two parts out of the core. Your work items live in a tracker that you choose. The agent templates and the lifecycle skills live in the reference workflow, an optional package. A plain `init` installs neither.

`update` converts an install of 1.x in one run. On an unmodified install of 1.4.0 that has a `backlog/` directory, the move of the `process` pack prints the opt-in line of the every-commit entry (see [Deferrals, trailers, and the opt-in](#deferrals-trailers-and-the-opt-in)):

```text
process.every-commit-names-a-work-item is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion
```

After the pack moves, `update` prints these lines. The numbers in them depend on your install:

```text
wrote .claude/agents/implementer.md
wrote .claude/agents/task-reviewer.md
wrote .claude/agents/branch-reviewer.md
wrote .claude/agents/spec-reviewer.md
wrote .claude/skills/orchestrating/SKILL.md
wrote .claude/skills/finishing-a-feature/SKILL.md
wrote .claude/skills/designing/SKILL.md
wrote .claude/skills/planning/SKILL.md
wrote .claude/evals/dependency-add.json
wrote .claude/evals/docs-edit.json
wrote .claude/evals/seeded-violations.json
wrote .githooks/commit-msg
wrote tools/reference-session-start.sh
removed 6 baselines from .houserules.json; the pins of the workflow replace them
merged .claude/settings.json (reference workflow SessionStart hook added)
edited .claude/settings.json: removed the seeded startup hook entry; the kit seeds the compaction entry alone
wrote tools/claude-session-start.sh
wrote .claude/skills/migrating-knowledge/SKILL.md
wrote .claude/skills/houserules-step/SKILL.md
wrote tools/houserules-step.sh
wrote .claude/schemas/deliverables.json
wrote backlog/schema.json
```

In that run, `update` also gave the install the JSON tracker, wrote the `backlogId` pattern into the deliverables schema, and excluded the every-commit entry. The sections below explain each part. A second `update` changes nothing.

### Trackers

The `tracker` key of `.houserules.json` names the tracker. An install with no `tracker` key has no tracker.

```json
"tracker": { "adapter": "json" }
```

The other two forms are `{"adapter": "github", "repo": "<owner>/<name>"}` and `{"adapter": "beads"}`.

- The conversion gives the JSON tracker to an install of 1.x that has a `backlog/` directory. Your data stays where it is.
- An install of 1.x with no `backlog/` directory gets no tracker, because there is no data to adopt. When `update` adopts the reference workflow there, it prints the [tracker hand step](#hand-step-the-reference-workflow-needs-a-tracker). `check-knowledge` fails until the install has a tracker:

  ```text
  pack process needs a tracker; run houserules init --tracker json|github|beads
  the reference workflow needs a tracker; run houserules init --tracker json|github|beads
  ```

- An install of 2.0 that has a `backlog/` directory and no `tracker` key keeps the line `backlog/ exists but no tracker is installed; run houserules init --tracker json` until you run `houserules init --tracker json`.

Choose a tracker with `init`:

```sh
houserules init --tracker json
houserules init --tracker github --repo <owner>/<name>
houserules init --tracker beads
```

`--repo` is optional for GitHub. Without it, `init` reads the `origin` remote. Every command that sets `tracker` also writes the `backlogId` pattern of the deliverables schema for the tracker, and the seeded CI workflow for it. `update` does both too.

An install keeps its tracker. `init --tracker` with the same tracker changes nothing for the tracker and exits 0. These cases refuse, exit 2, and write nothing:

| Case | Line |
|---|---|
| Another tracker is installed | `this install already has the <adapter> tracker; houserules changes no tracker` |
| `github` or `beads` on an install with a `backlog/` directory | `backlog/ holds JSON tracker data; move it as the migrating-knowledge skill describes and remove backlog/, or run houserules init --tracker json` |
| `github` with no `--repo` and no GitHub `origin` | `--tracker github needs a repository: pass --repo <owner>/<name>` |
| `beads` with no `bd` or no Beads store | `--tracker beads needs an initialized Beads store: run bd init (or bd bootstrap in a clone), then houserules init --tracker beads` |

No command moves data from one tracker to another. The `migrating-knowledge` skill has one section for each tracker with the setup and the field mapping.

**JSON.** The tracker keeps the `backlog/` files and the schema of 1.x. The commands that you used in 1.x print what they printed.

**GitHub Issues.** The tracker runs the `gh` CLI. houserules never reads, takes, or stores a token: `gh` keeps its own login. An item is an issue with a `type:<t>` label. `check-backlog` names the `gh label create <label>` command for each `houserules:` label that your repository lacks. The seeded CI workflow gets the job-level permissions `contents: read` and `issues: read`, and the step-level environment `GH_TOKEN: ${{ github.token }}` on the steps that run `check-backlog` and `audit`.

**Beads.** The tracker runs the `bd` CLI. houserules sets `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` on every `bd` it starts, in a project and in its CI, so an adopter never sends bd usage events through houserules. A user's own `bd` calls keep bd's own setting. The seeded CI workflow of a Beads install installs `bd` at a pinned release and runs `bd bootstrap`, because a fresh clone has no Beads store. `update` reads the issue prefix from `bd` again and rewrites the `backlogId` pattern when it changed. Where `bd` cannot be reached, `update` keeps the pattern, prints the unreachable line on stderr, and finishes.

`update` keeps a CI workflow that you changed, and prints a [hand step](#hand-step-a-ci-workflow-that-lacks-the-lines-of-your-tracker) when the workflow lacks the lines of your tracker.

#### With no tracker

A command that needs a tracker prints a line when the install has none. Line A is `no tracker installed; run houserules init --tracker json|github|beads to add one`. Line B is `backlog/ exists but no tracker is installed; run houserules init --tracker json`.

| Command | No `backlog/` directory | A `backlog/` directory exists |
|---|---|---|
| `check-backlog` | line A, exit 0 | line B, exit 1 |
| `list`, `batch`, `set`, `defer`, `get` of a work-item id | line A, exit 2 | line B, exit 2 |
| `archive` | moves the knowledge entries, prints `archive: skipped the backlog half: ` and line A, exit 0 | moves nothing, line B, exit 1 |
| `audit` | resolves no trailer, prints `skipped <n> work-item references: no tracker installed (houserules init --tracker)`, and exits as its other checks decide | line B, exit 1 |

`get` of a knowledge id never needs a tracker. `pack add process` needs one: `pack process needs a tracker; run houserules init --tracker json|github|beads`, exit 2.

#### A tracker that cannot be reached

An installed tracker that cannot be reached is never a pass. The gates, `check-backlog` and `audit`, exit 1. `list`, `batch`, `get`, `set`, `defer`, and `archive` exit 2. All print one line:

```text
tracker <adapter> cannot be reached: <cause>; <setup step>
```

| Tracker | Cause | Setup step |
|---|---|---|
| JSON | `backlog/ is missing` | `restore backlog/ from version control` |
| GitHub | `gh is not installed` | `install gh` |
| GitHub | `gh is not logged in` | `run gh auth login` |
| GitHub | `gh cannot see <owner>/<name>` | `check tracker.repo in .houserules.json and gh auth status` |
| Beads | `bd is not installed` | `install bd` |
| Beads | `no Beads store` | `run bd bootstrap in a clone, or bd init for a new store` |
| GitHub or Beads, any other failure | the first line of the error of the tool | `run the command by hand to see the full error` |

### Deferrals, trailers, and the opt-in

`houserules defer` files work that waits. `--title` files a new item: a chore, open, with the reason as its body. A title has at most 120 characters. `--item <id>` marks an existing item as a deferral. Both forms name a `--reason` and an `--origin`, repeat `--file <path>` for each file the deferral concerns, and print the id on one line. `--section <s>` names the section of a new item. On the JSON tracker it is the name of the items file. `list --deferred` shows each deferral with its `reason`, `origin`, and `files`.

A commit names the work item it delivers in a `Work-Item:` trailer, and a commit that files a deferral names it in a `Deferred:` trailer:

```text
fix(parser): read a quoted key

Work-Item: WI-012
Deferred: WI-015 the split changes the public API
```

- A commit that delivers several items writes one `Work-Item:` trailer for each.
- Trailers follow the trailer rules of git: the `Key: value` lines of the last paragraph. houserules reads no trailer from the body of a pull request.
- `audit` resolves every trailer of its range through the installed tracker. An id that matches no id shape of the tracker fails without a call to the tracker.
- A `Deferred:` trailer passes when it names a reason and the item carries a deferral with a reason.
- `audit --report` and `audit --workspace` also resolve each `issues[].backlog` id of the reviews and reports they read.

Each failure prints one line, and `audit` exits 1:

```text
<short-sha>: Work-Item <id> does not resolve in the <adapter> tracker
<short-sha>: Deferred <id> names no reason
<short-sha>: Deferred <id> is not a filed deferral with its reason in the <adapter> tracker
<file>: <id> is not a filed deferral with its reason in the <adapter> tracker
```

With no tracker, `audit` resolves no trailer and prints the `skipped` line of the table above.

The JSON result of `audit` holds the same verdict. `references.failures` lists each failure line, `references.resolved` and `references.skipped` count the references, and `summary.reference_fail` counts the failures. Line B and the line of a tracker that cannot be reached are failures too. `summary.fail` counts only the rule rows. The `--json` file holds the same result as stdout.

The process pack ships `process.every-commit-names-a-work-item`. The entry fails a commit that has no `Work-Item:` trailer. These commits pass: a commit by `dependabot[bot]`, `release-please[bot]`, or `github-actions[bot]`, and a commit whose subject starts `chore(<scope>): release `. A hook that reads the message before git knows the author applies the subject exemption only. A merge commit is checked like any other commit, and `git revert` writes no trailer. Add one with `git revert -e`.

The entry is opt-in. `optIn` in a pack manifest lists such entries. Every command that brings an opt-in entry onto your install writes an exclusion for it in `knowledge/overrides.json` with the reason `opt-in`. Five paths do this: `pack add`, `pack upgrade`, `init --add-required-packs`, an `update` that moves the pin of a pack, and the conversion of an install of 1.x. The command records the id in `optInOffered` in `.houserules.json` and prints once:

```text
process.every-commit-names-a-work-item is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion
```

Delete the exclusion to opt in. A later command never writes the exclusion again for an id in `optInOffered`, so the entry stays active. `pack remove` drops the exclusion and the record of an entry that you did not opt in to, so a later `pack add` offers it again. For an entry that you opted in to, a later `pack add` prints `<id> is active: this install opted in before`.

### The reference workflow

The reference workflow is a package of the files that run a change through design, plan, build, review, and finish. `houserules files` lists them as `workflowFiles`:

- four agent templates: `implementer`, `task-reviewer`, `branch-reviewer`, and the new `spec-reviewer`, in `.claude/agents/`
- four skills: `orchestrating`, `finishing-a-feature`, and the new `designing` and `planning`
- three eval scenarios in `.claude/evals/`
- the commit-msg hook `.githooks/commit-msg`
- the session-start script `tools/reference-session-start.sh`

`.claude/evals/record.json`, your record of eval runs, is workflow data. `init --workflow reference` writes it empty, and nothing pins, replaces, or reports it.

The workflow writes all its files whatever the render targets, because they are plain Markdown and scripts. Only the hook entry for `tools/reference-session-start.sh` in `.claude/settings.json` needs the Claude renderer. `init --workflow reference` adds it where `renderers` holds `claude`, and a later `init --target claude` adds it too.

**Install.** `init --workflow reference` needs a tracker. With none installed and no `--tracker`, it refuses and exits 2:

```text
the reference workflow needs a tracker; run houserules init --workflow reference --tracker json|github|beads
```

`init --workflow reference --tracker json` installs both. The workflow cites rules of the `process`, `security-hygiene`, and `writing-style` packs. `init` names the packs that your install lacks, and `--add-required-packs` adds them.

**Pins.** `.houserules.json` gains the key `workflow` with the `name`, the `version` of the binary, `files` (the SHA-256 of each file as the kit wrote it), and `models` (the model of each role: `implementer` `sonnet`, and `task-reviewer`, `branch-reviewer`, and `spec-reviewer` `opus`). `update` keeps your values in `models` and adds a missing role. The templates carry no `model:` line, and the `orchestrating` skill names the model of the role in every dispatch.

`update` moves the pins with the binary:

- It writes a file whose bytes equal its pin, and moves the pin.
- It keeps a file whose bytes differ from its pin, and prints `kept <file> (locally modified; reference workflow <version>)` on every run until you merge the change by hand. A file whose bytes you merged to equal the new content gets its pin moved, with no line.
- It never writes a path that `ownedPaths` lists, and it prints nothing for it.
- A workflow path with no pin belongs to the project. `update` never writes it. When a binary adds a path that your project already holds, `update` prints the [hand step](#hand-step-a-file-at-a-workflow-path-that-is-not-the-kits) once, and then prints nothing for that path.

**Conversion.** When an install of 1.x holds files of the kit at the workflow paths and has no `workflow` key, `update` adopts the reference workflow at the version of the binary:

- A file that is the kit's is pinned at its recorded baseline, else at a content that the kit shipped and that the file equals. A file that you changed has a recorded baseline too. `update` pins it there, keeps it, and prints the `kept` line.
- A file that is absent is written and pinned.
- A file at a workflow path that is not the kit's is left as it is, gets no pin, and gets the [hand step](#hand-step-a-file-at-a-workflow-path-that-is-not-the-kits).
- A path that `ownedPaths` lists is left as it is, with no pin and no line.
- A required rule that `knowledge/overrides.json` excludes gets the [hand step](#hand-step-a-required-rule-that-you-excluded).
- An install with no tracker gets the [tracker hand step](#hand-step-the-reference-workflow-needs-a-tracker).

An install with no kit file at a workflow path never adopts the workflow. A project's own `.githooks/commit-msg` or `.claude/agents/implementer.md` on a minimal install stays yours: `update` adopts nothing and prints nothing for it. `init --workflow reference` then keeps both files and prints the hand step once.

**Session start.** `tools/claude-session-start.sh` keeps only the re-injection of `houserules standing` after a compaction. The ritual that 1.x printed at every start moved to `tools/reference-session-start.sh`, with its own hook entry. Where the script is the kit's, `update` replaces it. Where you changed it, `update` keeps it, writes the new script and its hook entry, and prints the [ritual hand step](#hand-step-the-ritual-line-of-the-session-start-script). Where `ownedPaths` lists the script or `.claude/settings.json`, `update` leaves both as they are.

**Requirements.** `check-knowledge` checks that each rule the workflow cites comes from one installed pack that your overrides do not exclude. It fails with one line for each rule, exit 1:

```text
workflow reference requires <id>: no installed pack provides it; run houserules pack add <pack>
workflow reference requires <id>, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it
workflow reference requires <id>: add it to the local pack
```

The third line is for a rule that no pack of the kit ships. The kit-citation check covers the files of the workflow while the workflow is installed.

**The `AGENTS.md` block.** `render` writes the workflow's pointer for every harness into the houserules block of `AGENTS.md`: the reference to the `orchestrating` skill, and the rule that a harness with no subagents does the work directly, in its own session. It also writes the start and end commands of your tracker.

### The houserules step

Every install holds a core skill, `.claude/skills/houserules-step/SKILL.md`, and a core script, `tools/houserules-step.sh`, that any workflow can call. The script writes its output to `.houserules/step/`, so an agent reads it with its file-read tool:

- `tools/houserules-step.sh before <path>...` writes the rule package of the paths to `.houserules/step/rules.json` and prints the path of the file.
- `tools/houserules-step.sh after <base>` runs `houserules audit --base <base>`, writes `.houserules/step/audit.json`, and prints the path of the file and the exit status of the audit. With a report as the second argument, it also runs `houserules validate` on it.

`init` adds `.houserules/step/` to `.gitignore`, and the script writes a `.gitignore` of `*` into that directory before its first output. The skill and the script name no workflow.

### The minimal default

`init` with no flag installs the core and nothing else. It writes:

- `.houserules.json`
- `knowledge/schema.json`, `knowledge/areas.json`, and `knowledge/packs/knowledge-base.json`
- `.claude/skills/migrating-knowledge/SKILL.md`, `.claude/skills/houserules-step/SKILL.md`, and `tools/houserules-step.sh`
- the line `.houserules/step/` in `.gitignore`
- `.claude/schemas/deliverables.json`
- `AGENTS.md` with its block, each directory `AGENTS.md` that `render` writes for the installed packs (none for the `knowledge-base` pack alone), `CLAUDE.md`, and `docs/README.md`
- `.github/workflows/knowledge.yml`, on a GitHub-hosted origin

It writes no tracker, no workflow, no `backlog/` directory, and no eval file. Every gate passes on that install. `check-knowledge` prints `knowledge: ok`, and `check-backlog` prints line A and exits 0. A second `update` writes nothing under `backlog/` and no eval file.

The conversion does not reduce an install of 1.x to the minimal default. It adopts the tracker and the workflow files that it finds.

### Files that moved

`houserules files` lists seven classes in 2.0. These paths changed class:

| Path | Class in 1.x | Class in 2.0 |
|---|---|---|
| `.claude/agents/implementer.md`, `.claude/agents/task-reviewer.md`, `.claude/agents/branch-reviewer.md` | kit-owned | workflow files |
| `.claude/skills/orchestrating/SKILL.md`, `.claude/skills/finishing-a-feature/SKILL.md`, `.githooks/commit-msg` | kit-owned | workflow files |
| `.claude/evals/dependency-add.json`, `.claude/evals/docs-edit.json`, `.claude/evals/seeded-violations.json` | seed-once | workflow files |
| `.claude/evals/record.json` | seed-once | workflow data |
| `backlog/batches.json`, `backlog/amendments.json`, `backlog/decisions.json`, `backlog/parked.json`, `backlog/items/general.json` | seed-once | tracker files, seed-once |
| `backlog/schema.json` | seed-once | tracker files, kept current through a baseline |
| `.claude/schemas/deliverables.json` | seed-once | kept current through a baseline |
| `tools/claude-session-start.sh` | kit-owned | kit-owned, with the compaction re-injection only |

These files are new: `.claude/skills/houserules-step/SKILL.md` and `tools/houserules-step.sh` (kit-owned), and `.claude/agents/spec-reviewer.md`, `.claude/skills/designing/SKILL.md`, `.claude/skills/planning/SKILL.md`, and `tools/reference-session-start.sh` (workflow files).

The invariant: an install holds the files of the parts it installed, and `update` restores only those. `update` restores and delivers the tracker files only where `tracker.adapter` is `json`. It delivers the deliverables schema and `backlog/schema.json` where they are the kit's, and keeps a copy that you changed, with a [hand step](#hand-step-a-schema-of-your-own).

### The model field

Every task report, review, re-review, branch review, and spec review carries `model`: the `Model:` value of the dispatch (a model alias or a model id), `human`, or `unknown`. `validate` requires it. A file with no `model` fails with `<file>: missing "model"`, so reports and reviews that you wrote before the upgrade fail until you add the field. `stats` groups cost and findings by `model` under the new key `models`, and counts a deliverable with no `model` in the row `pre-2.0`. A `model` that names one family (`fable`, `haiku`, `opus`, or `sonnet`) as a whole word counts in the row of that family, so the alias `opus` and the id `claude-opus-5-5` share the row `opus`. Any other value keeps its own row.

A new kind, `spec-review`, holds the findings of a design review. `stats` counts its findings by severity in the same rows as the findings of a review. A task report may also list `gates_run` in each of its `fix_rounds`.

### Two tiers

houserules 1.x had three tiers: light, standard, and full. 2.0 has two, light and standard. The kit texts that named the full tier now name a critical task:

- A critical task changes the commands, flags, or output of a command-line interface, a schema, authentication or security code, or the data that an install keeps on disk.
- The plan writes the cases of a critical task as tests before any code. The author of the change commits them first, with one recorded run that shows them failing at the base commit, and then makes them pass. The evidence is that run and the whole suite passing at the head.
- A critical task gets one task review and never a re-review.
- The light tier applies when no task is critical, where 1.x asked that no `full-tier` entry applied.

The `full-tier` tag is gone. No kit text reads it, so an entry of your own that carries the tag no longer changes anything. Delete the tag, or keep it as a plain label.

### The reds that an upgrade causes by design

The migration commit, or the first commits after it, can fail these checks. Each red is correct, and each has a remedy:

- **`audit` of the migration commit fails `process.evals-rerun`.** `update` rewrote agent templates or eval scenarios, and `.claude/evals/record.json` did not change with them. The row carries the evidence `... without .claude/evals/record.json`. To clear the red, run your `.claude/evals/` scenarios and append the run to `.claude/evals/record.json`. `--sanctioned process.evals-rerun=<ref>` does not clear it. The row keeps its `fail` and is marked sanctioned. `audit` still exits 1, and a CI job that runs it stays red.
- **`check-knowledge` fails for a required rule that you excluded.** The files of the workflow cite a rule that `knowledge/overrides.json` excludes. `update` prints the [hand step](#hand-step-a-required-rule-that-you-excluded), and `check-knowledge` prints the same line until you remove the exclusion. To change the rule, add an override for it.
- **`check-knowledge` fails while the install has the process pack or the workflow and no tracker.** Run `houserules init --tracker json|github|beads`.
- **`validate` fails a report or review with no `model`.** Add the field.
- **`git revert` fails the every-commit entry, once you opted in to it.** Add a `Work-Item:` trailer with `git revert -e`.

The every-commit entry causes no red on upgrade: the conversion excludes it, so no commit fails for a missing `Work-Item:` trailer until you delete the exclusion (see [Deferrals, trailers, and the opt-in](#deferrals-trailers-and-the-opt-in)).

## Hand steps

`update` does what it can. For the rest, it prints `hand step: ` and a text. Each text below is the line as `update` prints it. A word in angle brackets, such as `<path>`, stands for a value from your project.

Some steps ask you to compare a file with the one that `houserules init` writes. Make that reference install once, outside your repository:

```sh
git init -q ../kit-reference
git -C ../kit-reference remote add origin https://github.com/example/reference.git
houserules init --dir ../kit-reference
```

The GitHub origin makes `init` write the CI workflow too. To compare a file of your tracker, run `houserules init --tracker <name> --dir ../kit-reference` as well, with `json` or `github` for `<name>`: it writes `backlog/schema.json` for the JSON tracker, the `backlogId` pattern, and the CI workflow of that tracker. Delete `../kit-reference` when you are done.

The Beads tracker needs a store first, because `init --tracker beads` refuses without one. `bd init` links an origin remote to the store and contacts it, so it runs before the `remote add` line. For Beads, make the reference install with this block instead:

```sh
git init -q ../kit-reference
(cd ../kit-reference && bd init)
git -C ../kit-reference remote add origin https://github.com/example/reference.git
houserules init --dir ../kit-reference
houserules init --tracker beads --dir ../kit-reference
```

If `../kit-reference` exists already, delete it and make it again with this block.

### Hand step: a reserved topic name

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

### Hand step: the CLAUDE.md import

```text
hand step: add @AGENTS.md to CLAUDE.md: without the import, the standing rules no longer reach Claude Code
```

Add a line with `@AGENTS.md` to `CLAUDE.md`. 1.x gave Claude Code the standing rules in `.claude/rules/standing-rules.md`. 2.0 gives them in the block of `AGENTS.md`, and Claude Code reads that file only through the import. `check-knowledge` fails until the line is there.

### Hand step: a schema that you own, with the area enum

```text
hand step: remove $defs.area from knowledge/schema.json; your areas live in knowledge/areas.json. Then replace the $ref of its area property with <area-property>
```

`knowledge/schema.json` is in `ownedPaths`, so `update` does not rewrite it. It still holds the area enum of 1.x. Do both edits:

1. Delete the object `$defs.area`. It is the list of area names.
2. In `$defs.entry.properties.area`, replace `{ "$ref": "#/$defs/area" }` with the text that `update` printed in place of `<area-property>`. For the schema of the kit it is `{"type":"string","minLength":1}`.

With the enum in place, `check-knowledge` names the enum as the cause when a pack adds an area that the enum lacks.

### Hand step: a schema with other changes

```text
hand step: knowledge/schema.json holds changes beyond the area enum, so houserules update removed $defs.area and the $ref of its area property and nothing else; compare the file with the schema that houserules init writes, and merge the rest by hand
```

`update` already made the two edits. Your other changes are still in the file. Compare it with the schema of the kit and merge by hand:

```sh
diff knowledge/schema.json ../kit-reference/knowledge/schema.json
```

### Hand step: a different schema without an area enum

```text
hand step: knowledge/schema.json differs from the schema that houserules init writes and holds no area enum to remove, so houserules update kept it; compare the two files, and merge the rest by hand
```

`update` kept the file as it is. Compare it with the schema of the kit, as in the step above, and merge the differences you want.

### Hand step: a schema that is not a JSON object

```text
hand step: knowledge/schema.json is no JSON object, so houserules update kept it; replace it with the schema that houserules init writes
```

Copy `../kit-reference/knowledge/schema.json` over `knowledge/schema.json`.

### Hand step: a CI workflow of your own

```text
hand step: <path> differs from every workflow that houserules 1.x released, so houserules update kept it; compare it with the workflow that houserules init writes, and merge the rest by hand
```

`<path>` is the workflow file, `.github/workflows/knowledge.yml` for an install at the top level of the repository. `update` replaces a workflow that equals one of the workflows of houserules 1.x. It keeps yours. Compare it with the workflow of the kit and merge by hand:

```sh
diff .github/workflows/knowledge.yml ../kit-reference/.github/workflows/knowledge.yml
```

To keep your workflow for good, list its path in `ownedPaths`.

### Hand step: two packs ship one id

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

### Hand step: a file at a workflow path that is not the kit's

```text
hand step: <file> is not the kit's; the reference workflow leaves it to the project
```

`update` found files of the kit at the workflow paths of your install and adopted the reference workflow. The file at `<file>` is not one of them: it equals no content that the kit shipped. `update` leaves it as it is and prints this line once for the path. To use the file of the kit, delete yours and run `houserules update`. To keep yours for good, list its path in `ownedPaths`.

### Hand step: a required rule that you excluded

```text
hand step: workflow reference requires <id>, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it
```

The files of the workflow cite the rule `<id>`, and `knowledge/overrides.json` excludes it. Remove the exclusion. To change the rule, add an override for it. `check-knowledge` fails with the same line until you do.

### Hand step: the reference workflow needs a tracker

```text
hand step: the reference workflow and the process pack need a tracker; run houserules init --tracker json|github|beads
```

The install has no `tracker` key. The conversion gives the JSON tracker to an install of houserules 1.x that has a `backlog/` directory, and to no other install. Run `houserules init --tracker json` to keep the backlog files, or name another tracker. `check-knowledge` names the missing tracker until you do.

### Hand step: the ritual line of the session-start script

```text
hand step: tools/claude-session-start.sh holds the reference workflow's ritual line; move it to tools/reference-session-start.sh
```

The reference workflow prints the session ritual from its own script, `tools/reference-session-start.sh`, which `update` wrote and added to `.claude/settings.json`. Your `tools/claude-session-start.sh` is not the kit's, so `update` kept it, and it still prints the ritual. The ritual prints twice until you move the line. Delete the ritual line from it, or delete the file and run `houserules update`. The script of the kit keeps only the standing rules after a compaction.

### Hand step: a schema of your own

```text
hand step: <path> differs from every content of it that houserules shipped, so houserules update kept it; compare it with the file that houserules init writes, and merge the rest by hand
```

`<path>` is `.claude/schemas/deliverables.json`, or `backlog/schema.json` with the JSON tracker. `update` replaces a schema that equals a content that the kit shipped, and keeps yours. Compare it with the schema of the kit and merge by hand:

```sh
diff .claude/schemas/deliverables.json ../kit-reference/.claude/schemas/deliverables.json
```

The `backlogId` pattern of the deliverables schema belongs to your tracker, so it differs from the file of the reference install. `update` writes the pattern of the tracker of the install, and the pattern that accepts any token, `^[A-Za-z0-9#._-]+$`, while the install has no tracker. To keep your schema for good, list its path in `ownedPaths`.

### Hand step: a CI workflow that lacks the lines of your tracker

The seeded CI workflow depends on the tracker. `update` keeps a workflow that you changed. When the workflow lacks what the tracker needs, `update` prints one of these lines on every run.

The GitHub tracker:

```text
hand step: <path> lacks the lines that the github tracker needs; add permissions: {contents: read, issues: read} to the gates job, and env: {GH_TOKEN: ${{ github.token }}} to each step that runs houserules check-backlog or houserules audit
```

Add the permissions block to the job and the token to the two steps. `gh` reads the issues with that token.

The Beads tracker:

```text
hand step: <path> lacks the steps that the beads tracker needs; add a step that installs bd <bd-version> from its GitHub release, and a step that runs bd bootstrap --yes, before the step that runs houserules check-backlog
```

Copy the two steps from the workflow that `houserules init` writes for the Beads tracker. They download `bd`, check its checksum, and bootstrap the store from the data of the origin.

## The commit-msg hook

The commit-msg hook is a file of the reference workflow, so a minimal install has none. The hook finds your install root from the path of its own file: the parent of the directory that holds it. A copy in `.git/hooks` has the parent `.git`, which holds no `.houserules.json`. That copy now skips the check with one line, and the commit goes through:

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
