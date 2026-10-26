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

The CI workflow of an install seeded by houserules 0.3.0, 1.0.0, or 1.1.0 installs the newest release. From the release of 2.0, its gates exit 2 with this line until you run `update` and commit the result. A workflow seeded by 1.2.0 or later installs the version that `.houserules.json` records, so it keeps the 1.x binary until `update` restamps the file. When `.houserules.json` records no readable `version`, a workflow of 1.2.0 or later installs the newest release too, and its gates exit 2 with this line.

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

`--repo` is optional for GitHub. Without it, `init` reads the `origin` remote. Every command that sets `tracker` also writes the `backlogId` pattern of the deliverables schema for the tracker, and brings the seeded CI workflow of an install that holds one to the form of the tracker. It writes no workflow for an install that holds none. `update` does both too.

An install keeps its tracker. `init --tracker` with the same tracker changes nothing for the tracker and exits 0. These cases refuse, exit 2, and write nothing:

| Case | Line |
|---|---|
| Another tracker is installed | `this install already has the <adapter> tracker; houserules changes no tracker` |
| `github` or `beads` on an install with a `backlog/` directory | `backlog/ holds JSON tracker data; move it as the migrating-knowledge skill describes and remove backlog/, or run houserules init --tracker json` |
| `github` with no `--repo` and no GitHub `origin` | `--tracker github needs a repository: pass --repo <owner>/<name>` |
| `beads` with no `bd` or no Beads store | `--tracker beads needs an initialized Beads store: run bd init (or bd bootstrap in a clone), then houserules init --tracker beads` |

No command moves data from one tracker to another. The `migrating-knowledge` skill has one section for each tracker with the setup and the field mapping.

**JSON.** The tracker keeps the `backlog/` files and the schema of 1.x. The commands that you used in 1.x print what they printed.

**GitHub Issues.** The tracker runs the `gh` CLI. houserules never reads, takes, or stores a token: `gh` keeps its own login. An item is an issue with a `type:<t>` label. `check-backlog` names the `gh label create <label>` command for each `houserules:` label that your repository lacks. The seeded CI workflow of an install that holds one gets the job-level permissions `contents: read` and `issues: read`, and the step-level environment `GH_TOKEN: ${{ github.token }}` on the steps that run `check-backlog` and `audit`.

**Beads.** The tracker runs the `bd` CLI. houserules sets `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` on every `bd` it starts, in a project and in its CI, so an adopter never sends bd usage events through houserules. A user's own `bd` calls keep bd's own setting. The seeded CI workflow of an install that holds one with the Beads tracker installs `bd` at a pinned release and runs `bd bootstrap`, because a fresh clone has no Beads store. `update` reads the issue prefix from `bd` again and rewrites the `backlogId` pattern when it changed. Where `bd` cannot be reached, `update` keeps the pattern, prints the unreachable line on stderr, and finishes.

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
- Trailers follow the trailer rules of git: the `Key: value` lines of the last paragraph. houserules reads no trailer from the body of a pull or merge request.
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

The JSON result of `audit` holds the same verdict. `references.failures` lists each failure line, `references.checked` counts the references that the tracker judged, `references.passed` counts those among them that passed, `references.skipped` counts those that the audit did not judge (no tracker, a tracker that cannot be reached, or a tracker that answered with an error), and `summary.reference_fail` counts the failures. Line B and the line of a tracker that cannot be reached are failures too. `summary.fail` counts only the rule rows. The `--json` file holds the same result as stdout.

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

`init` adds `.houserules/` to `.gitignore`, so the output of the script and the batch workspaces (`.houserules/work/` by default) stay out of git, and the script writes a `.gitignore` of `*` into `.houserules/step/` before its first output. The skill and the script name no workflow.

### The minimal default

`init` with no flag installs the core and nothing else. It writes:

- `.houserules.json`
- `knowledge/schema.json`, `knowledge/areas.json`, and `knowledge/packs/knowledge-base.json`
- `.claude/skills/migrating-knowledge/SKILL.md`, `.claude/skills/houserules-step/SKILL.md`, and `tools/houserules-step.sh`
- the line `.houserules/` in `.gitignore`
- `.claude/schemas/deliverables.json`
- `AGENTS.md` with its block, each directory `AGENTS.md` that `render` writes for the installed packs (none for the `knowledge-base` pack alone), `CLAUDE.md`, and `docs/README.md`

It writes no tracker, no workflow, no CI file, no `backlog/` directory, and no eval file. Every gate passes on that install. `check-knowledge` prints `knowledge: ok`, and `check-backlog` prints line A and exits 0. A second `update` writes nothing under `backlog/` and no eval file.

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

The invariant: an install holds the files of the parts it installed, and `update` restores only those. `update` restores and delivers the tracker files only where `tracker.adapter` is `json`. It delivers the deliverables schema and `backlog/schema.json` where they are the kit's, and keeps a copy that you changed. A copy that has a baseline gets the line `kept <path> (locally modified)`; a copy that has none, and that differs from every content that the kit shipped, gets the [hand step of a schema of your own](#hand-step-a-schema-of-your-own).

### The model field

Every task report, review, re-review, branch review, and spec review carries `model`: the `Model:` value of the dispatch (a model alias or a model id), `human`, or `unknown`. `validate` requires it. A file with no `model` fails with `<file>: missing "model"`, so reports and reviews that you wrote before the upgrade fail until you add the field. `stats` groups cost and findings by `model` under the new key `models`, and counts a deliverable with no `model` in the row `pre-2.0`. A `model` that names one family (`fable`, `haiku`, `opus`, or `sonnet`) as a whole word counts in the row of that family, so the alias `opus` and the id `claude-opus-5-5` share the row `opus`. Any other value keeps its own row.

A new kind, `spec-review`, holds the findings of a design review. `stats` counts its findings by severity in the same rows as the findings of a review. A task report may also list `gates_run` in each of its `fix_rounds`.

### Two tiers

houserules 1.x had three tiers: light, standard, and full. 2.0 has two, light and standard. The kit texts that named the full tier now name a critical task:

- A critical task changes the commands, flags, or output of a command-line interface, a schema, authentication or security code, or the data that an install keeps on disk.
- The plan writes the cases of a critical task as tests before any code. The author of the change commits them first, with one recorded run that shows them failing at the base commit, and then makes them pass. The evidence is that run and the whole suite passing at the head.
- A critical task gets one task review and no re-review of its original findings. A fix round that fixes a Critical or Important finding, or that changes files beyond the files its findings name, then gets one review of its fix diff.
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

The `origin` remote lets `init --tracker github` read the repository. To compare a file of your tracker, run `houserules init --tracker <name> --dir ../kit-reference` as well, with `json` or `github` for `<name>`: it writes `backlog/schema.json` for the JSON tracker and the `backlogId` pattern. `init` writes no CI file, so the reference install holds no workflow to compare. Delete `../kit-reference` when you are done.

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
hand step: <path> differs from every workflow that houserules 1.x released, so houserules update kept it; compare it with the seeded workflow of this release, template/.github/workflows/knowledge.yml in the houserules repository, and merge the rest by hand
```

`<path>` is the workflow file, relative to the top level of the repository: `.github/workflows/knowledge.yml` for an install at the top level. `update` replaces a workflow that equals one of the workflows of houserules 1.x. It keeps yours. Compare it with the seeded workflow of this release, `template/.github/workflows/knowledge.yml` in the houserules repository, and merge by hand. That file is the form for an install at the top level of the repository with no tracker or the JSON tracker. The seeded workflow of another install differs from it:

- **The GitHub tracker:** the workflow also holds `issues: read` and the `GH_TOKEN` env. The hand step [a CI workflow that lacks the lines of your tracker](#hand-step-a-ci-workflow-that-lacks-the-lines-of-your-tracker) names them.
- **The Beads tracker:** the workflow also holds the steps that install `bd` and bootstrap the store, and it lacks `persist-credentials: false`. The same hand step names the steps. The hand step [a Beads CI workflow that drops its credential](#hand-step-a-beads-ci-workflow-that-drops-its-credential) names the input.
- **An install in a subdirectory:** the file is `.github/workflows/knowledge-<folder>.yml`, where `<folder>` is the path of the install below the top level with each `/` replaced by `-`. The `name` is `knowledge (<install path>)`, each of the two triggers has a `paths` filter on `<install path>/**`, and the job has `working-directory: <install path>` as the default of its steps.

The reference install holds no workflow, because `init` writes none, so fetch the file of your release (`<version>` is the release that you run). Run the command from the top level of the repository, with the path that the line names in place of `<path>`:

```sh
curl -fsSL https://raw.githubusercontent.com/jblossey/houserules/v<version>/template/.github/workflows/knowledge.yml | diff <path> -
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

`<path>` is `.claude/schemas/deliverables.json`, or `backlog/schema.json` with the JSON tracker. `update` replaces a schema that equals a content that the kit shipped, and keeps yours. It prints this step for a schema that has no baseline, as in an install converted from 1.x. A schema that has a baseline and differs from it gets `kept <path> (locally modified)` and no hand step. Compare your schema with the schema of the kit, and merge by hand:

```sh
diff .claude/schemas/deliverables.json ../kit-reference/.claude/schemas/deliverables.json
```

The `backlogId` pattern of the deliverables schema belongs to your tracker, so it differs from the file of the reference install. `update` writes the pattern of the tracker of the install, and the pattern that accepts any token, `^[A-Za-z0-9#._-]+$`, while the install has no tracker. To keep your schema for good, list its path in `ownedPaths`.

### Hand step: a CI workflow that lacks the lines of your tracker

The seeded CI workflow depends on the tracker. `update` keeps a workflow that you changed. When the workflow lacks what the tracker needs, `update` prints the lines below on every run, until the workflow holds each marker.

The GitHub tracker needs two things: the permissions `contents: read` and `issues: read` on the gates job, and the `GH_TOKEN` env on the two steps that run `houserules check-backlog` and `houserules audit`. `gh` reads the issues with that token. A workflow gets one line for its permissions and one for its env, each only when it lacks a marker of it, and none for a marker it holds. The line for the permissions follows the `permissions` key of the gates job and the markers that the workflow holds, so that the job never holds two `permissions` keys.

The GitHub tracker, for a gates job that holds no `permissions` key and a workflow that holds neither `contents: read` nor `issues: read`:

```text
hand step: <path> lacks the permissions that the github tracker needs; add permissions: {contents: read, issues: read} to the gates job
```

Add the block to the gates job:

```yaml
    permissions:
      contents: read
      issues: read
```

The GitHub tracker, for a workflow that holds `contents: read` and lacks `issues: read`. The seeded workflow of an install with no tracker, with the JSON tracker, or with the Beads tracker holds `contents: read`, so this is the line of a workflow that you kept when you moved to the GitHub tracker:

```text
hand step: <path> lacks issues: read, which the github tracker needs; add issues: read to the permissions of the gates job
```

Add `issues: read` to the `permissions` block that the job holds. Do not add a second `permissions` block.

```yaml
    permissions:
      contents: read
      issues: read
```

The GitHub tracker, for a workflow that holds `issues: read` and lacks `contents: read`:

```text
hand step: <path> lacks contents: read; add contents: read to the permissions of the gates job
```

Add `contents: read` to the `permissions` block that the job holds, with the same result as above.

The GitHub tracker, for a workflow that lacks the `GH_TOKEN` env. The line comes with one of the lines above or alone, for a workflow whose permissions are complete:

```text
hand step: <path> lacks the GH_TOKEN env that the github tracker needs; add env: {GH_TOKEN: ${{ github.token }}} to each step that runs houserules check-backlog or houserules audit
```

Add the env to the step that runs `houserules check-backlog` and to the step that runs `houserules audit`:

```yaml
      - run: houserules check-backlog
        env:
          GH_TOKEN: ${{ github.token }}
```

The Beads tracker:

```text
hand step: <path> lacks the steps that the beads tracker needs; add a step that installs bd <bd-version> from its GitHub release, and a step that runs bd bootstrap --yes, before the step that runs houserules check-backlog
```

Copy the two steps from the jobs that `houserules ci print --host github --mode inline` prints for an install with the Beads tracker. They download `bd`, check its checksum, and bootstrap the store from the data of the origin.

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

## Changes of install schema 2.2.0

Install schema `houserules/install@2.2.0` added the keys `runners` and `workspaces`. `update` brings `.gitignore` to hold the line `.houserules/`, which keeps the batch workspaces out of git too. The file decides what `update` does:

- A `.houserules/step/` line becomes `.houserules/` in its place, and `update` prints `merged .gitignore (.houserules/step/ replaced by .houserules/)`.
- A file that holds neither line gets `.houserules/` at its end, and `update` prints `merged .gitignore (.houserules/ added)`.
- A file that is absent is written with the line, and `update` prints `wrote .gitignore`.
- A file that holds `.houserules/` already stays, and `update` prints no line.
- A file that `ownedPaths` lists stays as found, and `update` prints no line: replace its `.houserules/step/` line with `.houserules/` by hand, or add the line where neither is present.

The keys `runners` and `workspaces`, which the project writes, keep their values. Do each hand step that `update` prints (see [Hand steps](#hand-steps)) and the hand steps below.

### Hand step: a CI workflow that does not check out the PR head

The seeded CI workflow checks out the head commit of a pull request, so that every step of the job, the audit included, judges the tree that lands. `update` rewrites a workflow that you did not change. It keeps a workflow that you changed, and prints this line on every run, after the hand step of your tracker, when `update` prints one, until the file holds the input:

```text
hand step: <path> lacks the PR-head checkout; add ref: ${{ github.event.pull_request.head.sha }} under with: of its actions/checkout step
```

Add the input under `with:` of the `actions/checkout` step:

```yaml
- uses: actions/checkout@v7
  with:
    ref: ${{ github.event.pull_request.head.sha }}
    fetch-depth: 0
```

On a push the expression is empty, and the checkout keeps the ref of the event.

### Hand step: workspaces that git tracks

The kit named no location for the batch workspaces before 2.2.0, so `update` cannot tell that your install tracks them, and it prints nothing. Since 2.2.0 the workspaces live in `.houserules/work/` by default, and git ignores that folder. If your install keeps its workspaces in a tracked folder, take these steps. Set `workspaces` in `.houserules.json` to the folder, or move the folder under `.houserules/work/`. For a folder outside `.houserules/`, add its line to `.gitignore` first, so that `check-knowledge` finds the folder ignored. Run `git rm -r --cached <folder>` and commit. For each `verify` path that `check-knowledge` then names, commit a copy under a tracked path (for example `tests/fixtures/`) and point `verify` at it.

`check-knowledge` names a `verify` path that git ignores, in place of the line for a missing path:

```text
knowledge/<topic>.json <id>: verify path "<path>" is ignored by git; commit a copy under a tracked path and verify that
```

A fresh clone lacks an ignored file, so a `verify` path into it fails the gates in CI. This is the reason for the copy.

### The schemas of your own

`update` rewrites the knowledge schema and the deliverables schema while each equals a content that houserules shipped. It keeps a copy that you changed, and prints `kept <path> (locally modified)` for it; a deliverables schema with no baseline gets the [hand step of a schema of your own](#hand-step-a-schema-of-your-own) instead. A kept copy lacks the fields of 2.2.0, and you merge them by hand. In `knowledge/schema.json`, add `"runner"` to the `type` enum of `$defs.check`, and add the properties `tool` (a string of at least one character), `non_empty` (a boolean), and `covers_ids` (a boolean). In `.claude/schemas/deliverables.json`, `$defs.auditRow` gains the properties `sanctioned`, `findings`, and `override`, and the schema gains the definitions `auditFinding` and `rowOverride`. Compare each file with the reference install (see [Hand steps](#hand-steps)):

```sh
diff knowledge/schema.json ../kit-reference/knowledge/schema.json
diff .claude/schemas/deliverables.json ../kit-reference/.claude/schemas/deliverables.json
```

### The reds of 2.2.0 by design

The migration commit, or the first commits after it, can fail these checks. Each red is correct, and each has a remedy:

- **`audit` of the update commit fails `process.evals-rerun`.** This holds for an install of the reference workflow, because `update` rewrote its templates. The remedy is the one in [the reds that an upgrade causes by design](#the-reds-that-an-upgrade-causes-by-design): run your scenarios and append the run to `.claude/evals/record.json`.
- **`check-knowledge` fails a `verify` path into an ignored folder.** See [the hand step for workspaces that git tracks](#hand-step-workspaces-that-git-tracks): commit a copy under a tracked path and point `verify` at it.
- **A schema of your own lacks the new fields.** `check-knowledge` fails a `runner` check against a knowledge schema that lacks the type, and `validate` fails an audit row that holds `sanctioned`, `findings`, or `override` against a deliverables schema that lacks them. See [the schemas of your own](#the-schemas-of-your-own).
- **A stale `self_audit` copy fails the report.** `audit --report` compares the copy with its own run over the same range. It compares nothing when the copy records another range, and a draft with `self_audit` set to `null` passes. When you change the report or the tree after the copy, the `self_audit` of the next task report whose copy is stale fails `process.deliverables-json`: run the audit again and copy `self_audit` again.
- **A change to a manifest that holds a pin that is not exact fails, whoever wrote the commit.** `security-hygiene.exact-pins` fails a change to a `Cargo.toml` that holds a dependency version that is not pinned with `=`: a bare version, which Cargo reads as a caret range, or a requirement that starts with `^`, `~`, `<`, or `>`, or that is `*`. Release-please and Dependabot commits are no exception. The remedy is to pin every dependency of that file exactly: run `cargo add <crate>@=<version>` for each. `security-hygiene.tool-pins-are-exact` and `security-hygiene.tool-versions-pins-are-exact` fail a change to a mise file or a `.tool-versions` file that holds a pin that is not a full version. Write the full version of each tool.

## A read-only token in the seeded CI workflow

The seeded CI workflow runs its gates job with a read-only token. Every form sets `permissions` with `contents: read` on the job. The GitHub form adds `issues: read` to that same block.

The JSON, GitHub, and no-tracker forms also set `persist-credentials: false` under `with:` of the `actions/checkout` step. No step of these forms runs `git` against the remote after the checkout. The checkout itself fetches the history (`fetch-depth: 0`). The Beads form keeps the persisted credential, which is now read-only: `bd bootstrap --yes` fetches the Dolt data from the remote after the checkout.

`update` rewrites a workflow that you did not change. It keeps a workflow that you changed. For a kept workflow, `update` prints one line for each of these markers that the file lacks, and none for a marker that the file holds. It prints the lines on every run, until the file holds the marker. The lines come in the order of the file: the hand step of your tracker, the read-only token, the PR-head checkout, and the persisted credential. A workflow that you kept when you changed the tracker holds the lines of its first form, so the lines of your new tracker can ask for less (`issues: read` and the env, without a new `permissions` block, for the GitHub tracker) or for the removal of a line (`persist-credentials: false` for the Beads tracker).

### Hand step: a CI workflow without the read-only token

```text
hand step: <path> lacks contents: read; add permissions: {contents: read} to the gates job
```

Add the block to the gates job:

```yaml
jobs:
  gates:
    runs-on: ubuntu-latest
    permissions:
      contents: read
```

`update` prints this line for the JSON tracker, the Beads tracker, and no tracker, when the gates job holds no `permissions` key. [A job that holds the key](#hand-step-a-ci-workflow-whose-gates-job-holds-a-permissions-key) gets another line. `update` prints no such line for the GitHub tracker: the [hand steps of the GitHub tracker](#hand-step-a-ci-workflow-that-lacks-the-lines-of-your-tracker) cover the block. They add the whole block, `issues: read` included, to a workflow that holds neither `contents: read` nor `issues: read`, and add the missing line to the block of a workflow that holds the other, so that the job never holds two `permissions` keys.

### Hand step: a CI workflow whose gates job holds a `permissions` key

The lines above add a `permissions` block to a gates job that holds none. When the gates job holds the key already, the line follows the key, so that the job never holds two `permissions` keys.

A `permissions` mapping that lacks `contents: read` gets this line, for every tracker. The mapping is a block, or a flow mapping such as `{pull-requests: read}`:

```text
hand step: <path> lacks contents: read; add contents: read to the permissions of the gates job
```

Add `contents: read` to the mapping that the job holds:

```yaml
    permissions:
      pull-requests: read
      contents: read
```

For the GitHub tracker, a mapping that holds neither `contents: read` nor `issues: read` gets this line:

```text
hand step: <path> lacks the permissions that the github tracker needs; add contents: read and issues: read to the permissions of the gates job
```

Add both lines to the mapping:

```yaml
    permissions:
      pull-requests: read
      contents: read
      issues: read
```

A mapping that holds one of the two scopes gets the line for the other scope, as in the lines of the GitHub tracker above.

The scalar `permissions: write-all` grants every scope to the job. It gets this line:

```text
hand step: <path> grants write-all; replace permissions: write-all with permissions: {contents: read} in the gates job
```

For the GitHub tracker, the line asks for both scopes:

```text
hand step: <path> grants write-all; replace permissions: write-all with permissions: {contents: read, issues: read} in the gates job
```

Replace the scalar with the block, for example for the GitHub tracker:

```yaml
    permissions:
      contents: read
      issues: read
```

The scalar `permissions: read-all` holds every read scope, so `update` prints no line for the permissions of a workflow that holds it. The other lines of the workflow, such as the `GH_TOKEN` env, still come when the workflow lacks them.

### Hand step: a CI workflow with `contents: write` or another scope that is not `read`, with no gates job, or with no valid YAML

`update` reads a workflow that you changed as YAML. The gates job is each job that has a step whose `run` holds `houserules check-knowledge`, whatever the name and the indentation of the job. `update` reads the markers of each such job by themselves: a marker of another job is no marker of the gates job. `<job>` is the name of the job under `jobs`.

A `permissions` mapping that grants `contents: write` gets this line:

```text
hand step: <path> grants contents: write in the <job> job; replace it with contents: read
```

Replace the scope:

```yaml
    permissions:
      contents: read
```

A `permissions` mapping can hold a scope that the gates job needs with another value than `read`. The scopes are `contents`, and `issues` for the GitHub tracker. The value can be `none`, or `write` for `issues`. The mapping gets this line, where `<scope>` and `<value>` are the key and the value that the copy holds:

```text
hand step: <path> sets <scope>: <value> in the <job> job; replace it with <scope>: read
```

Replace the value. Do not add a second key.

```yaml
    permissions:
      contents: read
      issues: read
```

A workflow with no job that runs `houserules check-knowledge` gets this line, and no other line for the workflow:

```text
hand step: <path> has no job that runs houserules check-knowledge; houserules update checks no permissions there
```

A file that the YAML parser rejects gets this line, and no other line for the workflow. `<parser-line>` is the parser's own text, with the place of the fault:

```text
hand step: <path> is not valid YAML (<parser-line>); houserules update checks no permissions there
```

Fix the file. `update` writes no YAML into your workflow.

### Hand step: a CI workflow that persists its credential

```text
hand step: <path> lacks persist-credentials: false; add it under with: of its actions/checkout step
```

Add the input under `with:` of the `actions/checkout` step:

```yaml
- uses: actions/checkout@v7
  with:
    ref: ${{ github.event.pull_request.head.sha }}
    fetch-depth: 0
    persist-credentials: false
```

`update` prints this line for the JSON tracker, the GitHub tracker, and no tracker. It prints no such line for the Beads tracker: the step that runs `bd bootstrap --yes` needs the persisted credential, and a [Beads workflow that holds the input](#hand-step-a-beads-ci-workflow-that-drops-its-credential) gets the opposite line.

### Hand step: a Beads CI workflow that drops its credential

```text
hand step: <path> holds persist-credentials: false, which the beads tracker cannot use; remove it from with: of its actions/checkout step, so that bd bootstrap can fetch from the remote
```

Remove the input from `with:` of the `actions/checkout` step:

```yaml
- uses: actions/checkout@v7
  with:
    ref: ${{ github.event.pull_request.head.sha }}
    fetch-depth: 0
```

`update` prints this line for the Beads tracker, on every run, until the file no longer holds the input. A workflow that you kept when you moved to the Beads tracker holds the input from its first form. The step that runs `bd bootstrap --yes` fetches from the remote after the checkout, so it needs the persisted credential.

## The report form

A task report cites its evidence by capture. `houserules capture` runs a
command and writes its output to a file under a header, and the report names
the file instead of holding the output. `houserules report-facts` fills the
facts of the report, and `houserules check-report-claims` checks the report
against the captures and against git. Your install gets the commands with the
binary. It gets the new report form with the deliverables schema, and the new
instructions with the agent templates and skills of the reference workflow.
`update` also moves the copy of the `process` pack to its next version
(`moved pack process from <old> to <new>`), because three entries and one
summary describe captures now.

An author writes a report in this order:

1. Run each test, gate, and live run through `houserules capture --out <file> -- <command>`, with `<file>` in the batch workspace.
2. Write each run as `{ "capture": "<path>" }`, each `self_review` item as `{ "claim", "evidence" }`, and each line that a claim rests on as an entry of `citations`.
3. Run `houserules report-facts <report> --base <ref>`, with `--audit <file>` for the result of `audit --json` and `--fix-base <ref>` in a fix round. It fills the commits, the changed files, the `sha256`, `command`, and `exit` of each run, the hash of each citation, and the self-audit.
4. Run `houserules validate <report>` and `houserules check-report-claims <report>`.

`tools/houserules-step.sh after <base> <report>` runs the audit with `--report`, then `report-facts`, then `validate`.

### What changed in the report

- `run` is `{ capture, sha256, command, exit, excerpt? }` and holds no other key. `output` left the schema: a report holds no command output. `capture` is a path, absolute or relative to the install root, so that a parallel agent in a worktree can cite a capture of the workspace. `excerpt` is optional, holds 1 to 1000 UTF-16 code units, and is a byte-exact part of the output of the capture.
- Each `self_review` item is an object, `{ "claim", "evidence" }`. A claim that says every, all, or no remaining lists a capture in `evidence`. A claim that says pin, pins, or pinned lists an existing path.
- `citations` is an optional array of `{ "path", "line", "end", "anchor", "sha256" }`. `anchor` is text of the cited lines that occurs once in the file at `HEAD`. `report-facts` fills `sha256` with the hash of the cited lines.
- A finding of a fix round may name `reran`: the path of the review probe that the round ran again.
- `check-report-claims` no longer runs three checks: the redirected-capture check, the truncation-marker check, and the paste-run lint. The hash, the byte-exact excerpt, and the `command` of the capture header cover their subjects.

### Closed workspaces need no conversion

A report of the old form fails `validate` against the new schema, and no gate validates a closed report again. `stats` and `audit --workspace` read the deliverables of a workspace as JSON without the schema, and print the same result before and after the upgrade. Leave the closed workspaces as they are.

### Hand step: a deliverables schema of your own

`update` rewrites the deliverables schema while it equals a content that houserules shipped. It keeps a copy that you changed. For a copy that has a baseline, `update` prints `kept .claude/schemas/deliverables.json (locally modified)` and no hand step; for a copy that has none, it prints the [hand step of a schema of your own](#hand-step-a-schema-of-your-own). A kept copy still requires `output` in a run, so every report in the new form fails `validate`. Merge these changes into the kept copy:

- Add the definition `sha256Hex`: a string that matches `^[0-9a-f]{64}$`.
- Change `$defs.run`: it requires `capture`, `sha256`, `command`, and `exit`, and it holds no `output`. `capture` and `command` are `text`, `sha256` is `sha256Hex`, `exit` is an integer or `null`, and `excerpt` is a string of 1 to 1000 UTF-16 code units (`minLength` 1, `maxLength` 1000).
- Add the definitions `selfReviewItem` (a required `claim` of type `prose`, and an optional `evidence` of type `textList`) and `citation` (a required `path`, `line`, and `anchor`, and an optional `end` and `sha256`; `line` and `end` are integers of at least 1, and `anchor` matches `\S{3}`).
- In `$defs.fixRound`, add the property `reran` (`text`) to the items of `findings`.
- In `$defs.taskReport`, add the property `citations` (an array of `citation`), and make `self_review` an array of `selfReviewItem`.

Compare the file with the reference install (see [Hand steps](#hand-steps)):

```sh
diff .claude/schemas/deliverables.json ../kit-reference/.claude/schemas/deliverables.json
```

### Hand step: an agent template that you changed

`update` keeps a template or a skill of the reference workflow that you changed, and prints `kept <file> (locally modified; reference workflow <version>)` on every run until you merge the change (see [The reference workflow](#the-reference-workflow)). Three files carry the new report form. The implementer template teaches the author to capture every run, to record citations, and to close the report with `report-facts` and `check-report-claims`; the task reviewer template gains the step that runs `check-report-claims` against a checkout at the task head; the `dependency-add` scenario expects a capture of the red and of the green run. The branch reviewer template and the `orchestrating`, `planning`, and `designing` skills gain a few sentences too. Make the reference install with the workflow, and compare each kept file with it:

```sh
git init -q ../kit-reference
houserules init --workflow reference --tracker json --add-required-packs --dir ../kit-reference
diff .claude/agents/implementer.md ../kit-reference/.claude/agents/implementer.md
diff .claude/agents/task-reviewer.md ../kit-reference/.claude/agents/task-reviewer.md
```

### The reds of the report form by design

The migration commit, or the first reports after it, can fail these checks. Each red is correct, and each has a remedy:

- **Every report in the new form fails `validate` against a deliverables schema that `update` kept.** The kept schema still requires `output`. Merge the changes of [the hand step for a deliverables schema of your own](#hand-step-a-deliverables-schema-of-your-own).
- **A report from an implementer template that `update` kept is in the old form,** which the new schema rejects. Merge the changes of [the hand step for an agent template that you changed](#hand-step-an-agent-template-that-you-changed).
- **A task report in flight across the upgrade fails `validate`,** and `check-report-claims` fails each of its runs with `<label>: no capture; run the command through houserules capture`, also on an install that kept the old schema. Rewrite the report in the new form.
- **`audit` of the update commit fails `process.evals-rerun`** on an install of the reference workflow, because `update` rewrote its templates. The remedy is the one in [the reds that an upgrade causes by design](#the-reds-that-an-upgrade-causes-by-design): run your scenarios and append the run to `.claude/evals/record.json`.
- **`check-knowledge` fails a pack copy or an override whose pattern does not compile.** It tests the patterns of a check in a pack copy and in an override as it does in a local entry, and prints `<file> <id>: check <field> is not a valid regex (<message>)`. Correct the pattern.

## The review layer

houserules 2.0 adds a per-change review layer. Each pull or merge request gets one report, and each merge gets one record. Five commands compute them: `evidence`, `route`, `bundle`, `record`, and `ci`. The layer is opt-in. A project that never wires it sees no new gate, and `update` changes only the files that the sections below name.

### What `init` and `update` write now

`init` writes no CI file. When the install holds no seeded workflow after the run, `init` prints one line, with `<host>` filled in from the `origin` remote (`github or gitlab` when the remote names no known host):

```text
init: houserules wrote no CI file; to run the gates and the review layer in your pipeline, run houserules ci print --host <host>
```

`update` writes no CI file either, and it does not write a deleted seeded workflow again. An install that holds the seeded `.github/workflows/knowledge.yml` keeps it. The workflow runs the gates only, and `update` keeps it current through its baseline as before. Once for each install, `update` prints this line and records `reviewLayerOffered: true` in `.houserules.json`:

```text
update: <path> runs the houserules gates; to add the review layer, run houserules ci print --host github
```

A file that `houserules ci write` created starts with the comment `# Written by houserules ci write; rewrite it with houserules ci write --force and the flags that wrote it.` and a second line with the version of the binary. When that version differs from the binary's, `update` prints `update: <path> pins houserules <old>; this binary is <new>: run houserules ci write --force with the flags that wrote it (a GitHub reference file takes the --sha of v<new>)`, and it writes nothing. Run the command again with the flags that you gave `ci write` the first time. For a GitHub reference file, give `--sha` the commit of the tag of the new release.

### The three ways of wiring

`houserules ci print` gives the first two of three ways, and `houserules ci write` gives the third. None of them edits a CI file that houserules did not create:

- **Reference.** `houserules ci print --host github` or `--host gitlab` prints the lines that call the unit of this release, and writes nothing. The unit is a reusable workflow on GitHub, pinned by the commit sha of the release tag: the output names the `git ls-remote` command that resolves the sha. It is a file for `include: remote` on GitLab, with an `integrity` hash. Add the lines to the pipeline file of the project.
- **Inline.** With `--mode inline`, `houserules ci print` prints the jobs of the unit with the inputs of the install filled in. Use it for a project that keeps the jobs in its own pipeline file, for a self-hosted GitHub (it cannot call a workflow on github.com by default), and for a GitLab that cannot reach github.com.
- **A file of its own.** `houserules ci write --host github --sha <sha>`, with the commit of the release tag as `<sha>`, or `houserules ci write --host gitlab`, creates a new file that holds the same lines and adds its path to `review.ci.files`. `--host github --mode inline` needs no `--sha`. It never writes into an existing file. `houserules ci write --template` writes the template of the pull or merge request, and only where the host reads none yet.

`--host` defaults to the host of the `origin` remote when that is github.com or gitlab.com. Another origin needs the flag. A line at the end of the output asks you to add the file that holds the lines to `review.ci.files` in `.houserules.json`, so that a change to it routes to `senior-review`.

### The commands of the layer

Each command reads the repository and files. No command talks to the host beyond the tracker that the install names: the GitHub tracker runs gh, as audit does. The jobs of the unit talk to the host.

- `houserules evidence --base <ref> --out <file>` writes the reviewer packet as Markdown. With `--audit <file>`, the result of `audit --json` for the same range, it runs no second audit.
- `houserules route --base <ref> --json <file>` decides `auto-merge-eligible`, `owner-review`, or `senior-review`. `houserules route --base <ref> --no-audit --json <file>` computes the floor, which runs no project tool. An install with no `review.route` setting gets `owner-review` for each change, and `senior-review` for a change to a rule source.
- `houserules bundle write` writes the evidence bundle section from the captures of the final gate run. `houserules bundle check --body <file> --base <ref> --head <ref>` checks the description of the request. Its exit status is the gate: 0 for a clean bundle, 1 when it prints a finding, 2 for a setup error.
- `houserules record write --base <ref> --head <ref> --out <file>` computes the record of a push to the default branch. The note lives under `refs/notes/houserules` (`refs/notes/houserules-sub/<install path>` for an install below the top level). A clone does not fetch it: `git fetch origin refs/notes/houserules:refs/notes/houserules`. Then `houserules record show <sha>` prints the record.

The rule `process.evidence-before-review` of the `process` pack (version `2.1.0`) asks for the bundle. The same version changes the bodies of `process.risk-tiers` and `process.bounded-review-loop`: a critical task gets one task review and no re-review of its original findings, and a fix round of the kind that `process.risk-tiers` names gets one review of its fix diff. The `writing-style` pack (version `1.0.2`) changes one word: its README rule asks whether pull or merge requests are accepted.

### The GitLab token

The job token of GitLab cannot post a note, set a commit status, set a label, read approvals, or push. The jobs `report` and `record-push` read the CI/CD variable `HOUSERULES_TOKEN`, which the project creates masked and hidden, with the environment scope `houserules-<key>` (`<key>` is `root` for an install at the top level). Only those two jobs name that environment, so the jobs that run houserules, `setup`, or the code of the request cannot read the token. Prefer a project access token (the scopes `api` and `write_repository`, the role Developer). On gitlab.com Free, project access tokens need Premium or Ultimate: use a personal access token of a dedicated member with the role Developer. A personal access token reaches every project that its user can reach. An access token expires after at most 365 days: rotate it. Until then `report` and `record-push` print their notice and post and push nothing, and the pipeline stays green.

### The limits of the review layer

- On GitHub, a label or a status that the job token sets starts no workflow run. A workflow that acts on the route listens to `workflow_run` of the review workflow, not to `labeled`.
- The post-merge sample is unsteerable only under a host merge that writes a new commit (a squash, a merge commit, or a rebase on the host). Under a fast-forward merge the key of the record is the head of the author, so the author knows the draw before the merge.
- A project that needs tools for its runner checks passes the install step to the `setup` input of the unit. The job that computes the floor runs no `setup`.
- A request runs its own edited CI definition on both hosts. A request that edits the wiring sets its own status and label. The mitigation is a required status from a protected-branch rule, plus the rule-source floor: a file in `review.ci.files` routes to `senior-review`. A request that edits a file of `review.ci.files` also controls its own run, which a required code-owner review of those files mitigates (a `CODEOWNERS` rule on GitHub, code owners with approval rules on GitLab, a Premium feature).
- `floor-install: runner:<path>` holds only on a fresh runner or an unwritable path: a GitHub-hosted runner, a GitLab docker executor, or a runner user that cannot write the path. On a reused runner the request could replace the file.
- On GitLab, a description edit starts no pipeline. Run the pipeline again to check the new description.

### Changes of install schema 2.3.0

`update` stamps `houserules/install@2.3.0` on a marker of `houserules/install@2.2.0` and prints `wrote .houserules.json (houserules/install@2.3.0)` once. It leaves a marker of any other schema as it is, `2.1.0` included. The keys `runners`, `workspaces`, and `review` keep their values. These keys and ids are new:

| Name | What it is |
|---|---|
| `houserules/install@2.3.0` | The install schema that holds the keys below. |
| `review` | The key of `.houserules.json` that configures the layer: `diffExclude`, `route`, `bundle`, `intent`, `ci`, and `record`. `check-knowledge` fails a fault in it with one line that names the key. |
| `reviewLayerOffered` | `true` once `update` has printed the opt-in line for the install. |
| `covers` | A field of the `check` of a knowledge entry. `covers: true` makes the check count as coverage of the files it reads. No entry of the kit sets it. `update` brings `knowledge/schema.json` to the version that names the field. |
| `houserules/pack@1.3.0` | The schema id of a pack copy whose check sets `covers`. |
| `houserules/overrides@1.2.0` | The schema id of a `knowledge/overrides.json` whose override sets `covers`. The overrides schema reads any minor of its major, so an older id does not fail. |

### The reds of the review layer by design

Each red is correct, and each has a remedy:

- **A project that wires the review layer:** its open requests fail `bundle check` until each carries a bundle. The remedy is `houserules bundle write` into the description, and on GitLab a new run. A request of a bot fails too unless `review.bundle.exemptAuthors` names the author or `review.bundle.exemptBranches` matches the branch.
- **An install that holds the seeded `knowledge.yml`:** no red. `update` prints the opt-in line once. An install whose seeded workflow was deleted gets no workflow and no line.
- **A fresh install:** no CI file and no red. `init` prints its line. The first request of the install has no rule sources at its merge base, so the packet and the record show `merge_base_rules: "absent"`.
- **A project with runner checks whose tools the job `record` does not get:** the record holds `audit.error` and no red. The remedy is the `setup` input of the unit.
- **A GitLab project without `HOUSERULES_TOKEN`, or with an expired one:** no comment, status, label, or record push, and one notice for each run. The remedy is to create or rotate the token. No red.
- **A check that you meant as coverage:** it is not coverage until its entry declares `covers: true`. No red.

## OpenSpec, links, and install schema 2.4.0

houserules 2.0 reads an OpenSpec change as the intent of a change. A commit links to a spec and to an agent session with a trailer. Two optional check types judge the links. [The OpenSpec page](openspec.md) shows the hookup. This section shows what an upgrade changes. No pack of the kit uses either check type, so neither becomes a gate until your project adds an entry of its own.

`evidence`, `record write`, and `audit` read the `Intent: <source>:<ref>` trailer and the session trailers of each commit of the range. A trailer never stops a command (see [Name a change in a commit](openspec.md#name-a-change-in-a-commit)).

### Changes of install schema 2.4.0

`update` stamps `houserules/install@2.4.0` on a marker of `houserules/install@2.2.0` or `houserules/install@2.3.0` and prints this line once:

```text
wrote .houserules.json (houserules/install@2.4.0)
```

It leaves a marker of any other schema as it is, `2.1.0` included. A new `init` and the conversion of an install of 1.x write `2.4.0` directly. The restamp changes the stamp and no other key. These names are new:

| Name | What it is |
|---|---|
| `houserules/install@2.4.0` | The install schema that holds the key and the source below. Its readers refuse an empty `defaultBranch`. |
| `review.sessions.trailers` | The key of `.houserules.json` that lists the trailer keys whose value refers to a session of an agent tool: an array of keys of letters, digits, and `-`. It defaults to `["Entire-Checkpoint"]`. A fault in it is one line from `check-knowledge`, exit 1: `.houserules.json: review.sessions.trailers must be an array of trailer keys (letters, digits, and -)`. |
| `openspec` | A value of `review.intent.sources`, for the adapter that reads an OpenSpec change. |
| `intent-required`, `intent-resolves` | Two check types of the knowledge schema. `intent-required` also takes the field `sources`. `update` brings `knowledge/schema.json` to the version that names them (see the hand step below). |
| `houserules/pack@1.4.0` | The schema id of a pack copy whose check has one of the two types or sets `sources`. |
| `houserules/overrides@1.3.0` | The schema id of a `knowledge/overrides.json` whose override sets one of them. The overrides schema reads any minor of its major, so an older id does not fail. |
| `houserules/evidence@1.1.0` | The document that `evidence --json` writes. It adds `sessions`. |
| `houserules/change-record@1.1.0` | The document that `record write` writes and `record show --json` prints. It adds `links.sessions` and `links.sessions_more`, and its `evidence` part leaves `sessions` out. |

The published files `evidence/1.0.0.json` and `change-record/1.0.0.json` stay at their URLs, byte for byte.

### The `validation` input of the CI units

Both units of the review layer, the GitHub workflow and the GitLab file, take a new input, the `validation` input. Its default is empty, so a pipeline that does not set it runs as before. Set it to the path, relative to the install, of a capture of `openspec validate --json` that your `setup` command writes (see [the validation capture](openspec.md#the-validation-capture)). When the input is set and the file exists, the units pass `--validation <path>` to `evidence` and to `record write`. When the input is set and the file does not exist, a unit prints this notice and passes no flag:

```text
houserules: the validation input names <path>, which does not exist, so this run passes no validation capture
```

### Hand step: a knowledge schema that you changed or own

A knowledge schema that lacks the two check types and the field `sources` fails an entry that uses them. `update` replaces `knowledge/schema.json` while it equals a content that houserules shipped, and prints `wrote knowledge/schema.json`. For any other schema, what it prints depends on your install.

**An install of 2.x, a schema that you changed.** `update` keeps the file and prints this line:

```text
kept knowledge/schema.json (locally modified)
```

**An install of 2.x, a schema that `ownedPaths` lists.** `update` leaves the file as it found it, and prints no line for it.

**An install of 1.x, a schema that you changed.** The conversion removes `$defs.area` and the `$ref` of its area property, and nothing else. It prints this line:

```text
hand step: knowledge/schema.json holds changes beyond the area enum, so houserules update removed $defs.area and the $ref of its area property and nothing else; compare the file with the schema that houserules init writes, and merge the rest by hand
```

**An install of 1.x, a schema that you own.** The `overrides` key of your marker lists `knowledge/schema.json`. The conversion moves the path to `ownedPaths` and does not rewrite the file. It prints this line:

```text
hand step: remove $defs.area from knowledge/schema.json; your areas live in knowledge/areas.json. Then replace the $ref of its area property with {"type":"string","minLength":1}
```

Each of these schemas lacks the two types and the field `sources`, and `check-knowledge` exits 1 for an entry that uses them. An entry that uses one of the types draws the first line below. An entry that sets `sources` draws the second line too:

```text
knowledge/<topic>.json.entries[<n>].check.type: must be one of "grep-absent", "commits", "co-change", "diff-append-only", "report-field", "durable-sha", "runner"
knowledge/<topic>.json.entries[<n>].check: unknown field "sources"
```

Each line names the topic file and the index of your entry. A schema of 1.x is older: it lacks the type `runner` and more fields of a check than `sources`. An entry that sets such a field draws one more line for it. The `intent-required` entry of [the OpenSpec page](openspec.md#two-checks-of-intent) sets `exempt_subjects` too, so it draws these lines on an install of 1.x:

```text
knowledge/<topic>.json.entries[<n>].check.type: must be one of "grep-absent", "commits", "co-change", "diff-append-only", "report-field", "durable-sha"
knowledge/<topic>.json.entries[<n>].check: unknown field "sources"
knowledge/<topic>.json.entries[<n>].check: unknown field "exempt_subjects"
```

**The remedy on an install of 2.x.** Make two edits to `knowledge/schema.json`:

1. In `$defs.check.properties.type.enum`, add `"intent-required"` and `"intent-resolves"`.
2. In `$defs.check.properties`, add this property:

```json
"sources": {
  "type": "array",
  "items": { "type": "string", "minLength": 1 },
  "uniqueItems": true
}
```

Then run `houserules check-knowledge`. To find other differences, compare the file with the schema of the kit (see [Hand steps](#hand-steps)):

```sh
diff knowledge/schema.json ../kit-reference/knowledge/schema.json
```

**The remedy on an install of 1.x.** The two edits are not enough: an entry that sets `exempt_subjects`, or another field that the schema of 1.x lacks, still draws a line for it. Compare the file with the schema of the kit, as in the command above. Merge the check types and the fields of a check that the kit adds, and keep your own changes. For a schema that you own, make the area edits of its line first (see [Hand step: a schema that you own, with the area enum](#hand-step-a-schema-that-you-own-with-the-area-enum)). For a schema that you changed, the merge is the one that its line asks for (see [Hand step: a schema with other changes](#hand-step-a-schema-with-other-changes)). Then run `houserules check-knowledge`.

### The faster task loop in the reference workflow

An install of the reference workflow gets the faster task loop of this release in four files and six pack entries. `update` writes each of these files while it equals its pin, and prints `kept <file> (locally modified; reference workflow <version>)` for a file that you changed (see [The reference workflow](#the-reference-workflow)):

- `.claude/agents/implementer.md`
- `.claude/agents/task-reviewer.md`
- `.claude/skills/orchestrating/SKILL.md`
- `.claude/skills/planning/SKILL.md`

An install that holds the earlier pack versions prints `moved pack process from 2.1.0 to 2.1.1` and `moved pack quality from 1.0.2 to 1.0.3`. The entries `process.ask-when-missing`, `process.tdd`, `process.risk-tiers`, `process.bounded-review-loop`, `process.parallel-on-disjoint-files`, and `quality.principles` gain or change body paragraphs. An override replaces a whole field, so a project that overrides the `body` of one of them keeps its own body. Delete the `body` field from the override to take the paragraphs of the kit, or merge them by hand.

The files and entries carry these rules:

- The implementer runs the focused test while it iterates, and the tests of its files before it commits. It runs no full suite and no suite that the plan marks as integration-only. The person who runs the work runs those suites once for each integration.
- A reading that the spec or the brief leaves open, and that a test would pin, is a question: the implementer stops, names the reading and its choices, and returns NEEDS_CONTEXT.
- The ledger stamps each dispatch and each close with its UTC time.
- The person who runs the work asks the owner in a plain message, never in a question tool that holds its turn, and keeps dispatching each task that needs no ruling.
- A wave orders the tasks and is no barrier: a task starts as soon as every task it needs is integrated.
- A plan may name one file that two tasks share. The person who runs the work resolves the later cherry-pick by keeping both sides.
- While a fix-diff review or a re-review runs, the person who runs the work may integrate the fix and run the integration gates.
- A test added after the recorded RED carries its own RED or a mutation row in the same report entry. A struct whose fields a reader fills names the production reader of each field in its doc comment.

### The reds of this section by design

Each red is correct, and each has a remedy:

- **Every gate exits 2, on an install of 1.x whose CI installs the newest release.** The workflow that houserules 0.3.0, 1.0.0, or 1.1.0 seeded installs the newest release, and so does a workflow of 1.2.0 or later on a `.houserules.json` with no readable `version`. From the release of 2.0, each gate of such a workflow exits 2 with the line of D4 until you run `update` and commit the result. The line is `<path>/.houserules.json: this is an install of houserules 1.x; run houserules update to convert it`. The remedy is one command, `houserules update` (see [Gates stop on an install of 1.x](#gates-stop-on-an-install-of-1x)).
- **`update` keeps a knowledge schema that you changed, on an install of 2.x.** It prints `kept knowledge/schema.json (locally modified)`. An entry that uses an intent check draws the lines of the [hand step](#hand-step-a-knowledge-schema-that-you-changed-or-own). The remedy is the two edits of that step.
- **`update` leaves a knowledge schema that `ownedPaths` lists, on an install of 2.x.** It prints no line for the file. An entry that uses an intent check draws the lines of the [hand step](#hand-step-a-knowledge-schema-that-you-changed-or-own). The remedy is the two edits of that step.
- **The conversion of an install of 1.x lists a knowledge schema that you changed as a hand step.** It prints a `hand step:` line. An entry that uses an intent check draws the lines of the [hand step](#hand-step-a-knowledge-schema-that-you-changed-or-own). The remedy is the merge of the schema of the kit that the line asks for. The two edits of that step are not enough.
- **The conversion of an install of 1.x lists a knowledge schema that you own as a hand step.** It prints a `hand step:` line with other words. An entry that uses an intent check draws the lines of the [hand step](#hand-step-a-knowledge-schema-that-you-changed-or-own). The remedy is the area edits that the line asks for, then the merge of the schema of the kit. The two edits of that step are not enough.
- **`update` keeps the kit files of an install that houserules 0.3.0 seeded.** The binary knows the kit files that houserules shipped from 1.0.0 on, so it reads an unchanged kit of 0.3.0 as files of your own. `update` keeps `.claude/schemas/deliverables.json` and prints the [hand step of a schema of your own](#hand-step-a-schema-of-your-own); until you merge it, every report in the new form fails `validate`. It keeps `.claude/evals/dependency-add.json` and prints the [hand step of a file that is not the kit's](#hand-step-a-file-at-a-workflow-path-that-is-not-the-kits). It removes only the area enum from `knowledge/schema.json` and prints the [hand step of a schema with other changes](#hand-step-a-schema-with-other-changes). The remedy is the three hand steps. For a kit that you did not change, copy `.claude/schemas/deliverables.json` and `knowledge/schema.json` of the reference install over yours, delete `.claude/evals/dependency-add.json`, and run `houserules update` again.
- **`update` keeps the seeded CI workflow of an install that has no `.houserules.json`.** Such an install has a `knowledge/` directory and no marker, and you convert it with `houserules update --id-prefix <prefix>` (see [Gates stop on an install of 1.x](#gates-stop-on-an-install-of-1x)). The conversion has no baseline for `.github/workflows/knowledge.yml`, so `update` leaves the file as it found it and prints no line and no hand step for it, even when it equals a workflow that houserules 1.x released. An install that has a `.houserules.json` gets `wrote .github/workflows/knowledge.yml` instead. The workflow of 1.x stays, and it lacks `permissions` with `contents: read`, the `ref: ${{ github.event.pull_request.head.sha }}` input of its checkout, and `persist-credentials: false`. No gate is red because of it. The remedy is to write the workflow of your release over the file: run the `curl` command of [the hand step for a CI workflow of your own](#hand-step-a-ci-workflow-of-your-own) with `-o <path>` in place of `| diff <path> -`. `update` leaves the new file as found on every later run, so compare it with the workflow of each release that you adopt.
- **`update` writes the install schema 2.4.0.** On an install of 2.2.0 or 2.3.0 it restamps the marker and prints `wrote .houserules.json (houserules/install@2.4.0)` once. The conversion of an install of 1.x prints `converted .houserules.json to the install schema houserules/install@2.4.0` instead. No remedy is needed.
- **A marker with an empty `defaultBranch` fails the commands that read it.** The restamp changes the stamp and no other key, and the readers of 2.4.0 refuse a value that an earlier build accepted. Every command that reads the marker prints `<path>/.houserules.json: defaultBranch must be a non-empty string` and exits 2. `check-knowledge` prints `.houserules.json: defaultBranch must be a non-empty string; kit-owned citations not checked` and exits 1. The remedy is to write the name of your default branch into the key, or to delete the key.
- **`audit` of the update commit fails `process.evals-rerun`** on an install of the reference workflow, because `update` rewrote the implementer template and more. The remedy is the one in [the reds that an upgrade causes by design](#the-reds-that-an-upgrade-causes-by-design): run your scenarios and append the run to `.claude/evals/record.json`.
- **An install that tracks an OpenSpec root fails `check-knowledge` until `render`.** After the first tracked file of the root, `check-knowledge` reports `AGENTS.md: generated file is out of date (run houserules render)` and `render --check` reports `AGENTS.md: would change`. The remedy is `houserules render` (see [The split in AGENTS.md](openspec.md#the-split-in-agentsmd)).
- **An install that runs the reference workflow and holds an OpenSpec root draws a warning** on stderr from `init --workflow reference` and from `check-knowledge`, with the exit code unchanged. The remedy is to plan with one workflow (see [Plan with one workflow](openspec.md#plan-with-one-workflow)).
- **A project that adds `intent-required`:** a bot or a release commit that changes a path that the entry's `if` matches fails it. The remedy is `exempt_subjects` or an `if` that leaves the lock files out. The `record` job after a squash merge sees one commit, so a squash message that drops the `Intent:` trailers fails the recorded check where the request passed. The remedy is to keep the trailers in the squash message.
- **`check-backlog` fails for an item whose batch no batch record holds.** On the JSON tracker, `check-backlog` reports each item of `backlog/items/` whose `batch` is a number that neither `backlog/batches.json` nor `backlog/archive/batches.json` holds. It prints one line for each such item on stderr and exits 1: `<item file> <id>: batch <n> is not in backlog/batches.json or backlog/archive/batches.json`. A backlog that ticked an item with `houserules set <id> batch=<n>` before the batch had a record draws the line. A new tracker draws it too, when its first item is ticked with `batch=1`. The remedy is to add a record for the batch to `backlog/batches.json`, or to set the item's `batch` to a batch that a record holds.
