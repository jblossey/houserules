# houserules with OpenSpec

OpenSpec plans a change: its proposal, its specs, its design, and its tasks. houserules supplies the rules of the project and the checks of each change. This page shows how the two meet.

This page was checked against OpenSpec 1.15.0. houserules never edits an OpenSpec file. The adapter that reads a change reads files from the tree at a commit and runs no OpenSpec tool.

## What houserules reads

houserules reads the `openspec/` folder of the install root. It reads no OpenSpec folder above the install root.

- The folder has a planning shape when `openspec/specs/` or `openspec/changes/` is a folder that holds no `.openspec-store/store.yaml`.
- Without a planning shape, a `store:` key in `openspec/config.yaml` (or `openspec/config.yml`) makes the folder a store pointer. houserules does not follow a store.

A change is a folder `openspec/changes/<id>/`. An archived change is a folder `openspec/changes/archive/<YYYY-MM-DD>-<id>/`. A folder whose name starts with `.` is no change. The reference of a change is the id that OpenSpec prints: the folder name.

houserules finds a change by the paths that the range changes under its folder. A change that the range archives resolves to its archived folder. A change with no folder at the head, because the range deleted or renamed it, gives no intent. By default `review.intent.sources` is `["work-item"]`. Set it to include `openspec`, in `.houserules.json`, so that the packet finds a change by those paths:

```json
{ "review": { "intent": { "sources": ["work-item", "openspec"] } } }
```

An `Intent: openspec:<id>` trailer names a change with no setting (see [Name a change in a commit](#name-a-change-in-a-commit)). The trailer resolves to the active change `<id>`, else to the archived change whose folder is named `<id>`, else to the archived change `<YYYY-MM-DD>-<id>` with the latest date.

For each change, the packet shows these fields. Each follows the rule that OpenSpec 1.15.0 uses for it:

| Field | Value |
|---|---|
| title | The first heading of `proposal.md` of the form `# <title>` or `# Change: <title>`. The id, when the file is missing, holds no such heading, or reads `proposal`. |
| why | The section of the first heading named `Why` (any level, any case, outside code fences) in `proposal.md`, up to the next heading of the same or a higher level. It stops at 1,000 characters. |
| tasks | The number of done tasks and of all tasks. Every line of a task file that is a Markdown task counts, and `x` or `X` marks a task done. The task file is `tasks.md`, unless the schema of the change names another. |
| requirements | The number of added, modified, removed, and renamed requirements in the `ADDED Requirements`, `MODIFIED Requirements`, `REMOVED Requirements`, and `RENAMED Requirements` sections of each file `spec.md` at least one folder below `specs/`. |
| validation | The result of `openspec validate` (see [The validation capture](#the-validation-capture)). |

Two departures: a proposal with no delta spec gives `requirements: null`, where `openspec show` counts the bullets of What Changes; and a schema outside the tree, such as a user-level schema, counts as unresolved, so the task count falls back to `tasks.md`.

For a change whose proposal holds a Why, whose `tasks.md` holds two tasks of which one is done, and whose delta spec adds one requirement, `houserules evidence` prints this section:

````text
### Intent (1)

`openspec add-login`: `Add login`
Why:
```
Users need to sign in before they reach their data.
```
Tasks: 1 of 2 done
Requirements: +1 ~0 -0 renamed 0 (total 1)
````

A named change that houserules cannot read prints an unresolved intent block with one of these lines:

- `no openspec/ folder at the head`
- `the OpenSpec root points at a store, which houserules does not read`
- `an OpenSpec change id is kebab-case: lowercase letters, digits, and single hyphens`
- `no OpenSpec change with that id at the head, active or archived`

## Name a change in a commit

A commit names the intent of its change in a trailer: `Intent: openspec:<id>` for an OpenSpec change, and `Intent: file:<path>` for a file. `evidence` and `record write` read the trailers of each commit of the range, and `audit` reads them for the two checks below. A trailer never stops a command. A value that is no `<source>:<ref>`, or whose source is no adapter, prints as an unresolved intent, and the command exits 0.

A trailer whose key `review.sessions.trailers` lists is a reference to a session of an agent tool. The default key is `Entire-Checkpoint`. The packet prints each such trailer in the line `Sessions: <sha7> <key>: <value>, ...`, or `Sessions: none`. For a commit with the trailer `Entire-Checkpoint: 01HXYZ`, it prints this line:

```text
Sessions: <sha7> Entire-Checkpoint: `01HXYZ`
```

houserules reads no checkpoint: the value is text.

## The validation capture

`openspec validate` checks a change. houserules takes its result as a capture, so that the result belongs to one commit. Run this command in a clean work tree, at the commit that you review:

```sh
houserules capture --out <file> -- sh -c 'openspec validate --all --json 2>/dev/null'
```

`houserules capture` writes the output of the run to `<file>`, under a header that names the commit and says whether the work tree was dirty. The exit of the run is no error: OpenSpec exits 1 when an item fails. Pass the capture to `evidence --validation <file>` and to `record write --validation <file>`.

Each `openspec` intent takes the item whose `type` is `change` and whose `id` equals the reference of the intent. A capability of the same id is another item. The packet prints one line under the intent, with `no issues`, `1 issue`, or `<n> issues` counting every issue of every level:

```text
Validation: `openspec` ok: `no issues`
Validation: `openspec` failed: `2 issues`
```

The run validates active changes only. An archived change has no item, so its validation is `null`. An intent with no item keeps `validation: null`.

A capture that cannot be used never stops a command. `evidence` and `record write` print one line on stderr, keep every validation `null`, and exit as without the flag:

```text
evidence: --validation <path>: <reason>
```

`record write` prints `record:` in place of `evidence:`. The `<reason>` is one of these:

- `cannot be read (<os error>)`
- `is no capture (<the line of capture's reader>)`
- `ran outside a git work tree`
- `ran at <head7>, not at <head7>`
- `ran in a dirty work tree`
- `holds no JSON (<serde line>)`
- `is no openspec validate document (version "1.0" with items)`

In CI, both units of the review layer take the `validation` input. It is the path, relative to the install, of the capture that your `setup` command writes with the command above. When the input is set and the file exists, the units pass `--validation <path>` to `evidence` and to `record write`. When the input is set and the file does not exist, a unit prints this notice and passes no flag. `<path>` is the input, joined to the folder of the install:

```text
houserules: the validation input names <path>, which does not exist, so this run passes no validation capture
```

## Two checks of intent

A requirement of OpenSpec and a rule of houserules answer two questions. A requirement in a delta spec says what the product does:

```markdown
### Requirement: Sign in
The system SHALL let a user sign in.
```

A rule says what every change must obey, and its check can fail a change. The `intent-required` entry below is such a rule: it asks each change to `src/` for its OpenSpec change.

Two check types of the knowledge schema judge the links of a change. No pack of the kit uses either one: a project opts in with an entry of its own. Put each entry into a topic file of your own, for example `knowledge/project.json`, and run `houserules render`.

`intent-required` fails a change that touches a path that `if` matches and names no intent. `sources` takes `file` and `openspec`, and defaults to both. A commit whose subject matches a regex of `exempt_subjects` contributes no path. The check passes when the range holds an `Intent:` trailer of `sources` that resolves, or changes a file of a change that an adapter of `sources` finds. When no changed path matches `if`, the check does not apply to the change.

```json
{
  "id": "project.changes-name-their-spec",
  "kind": "rule",
  "area": "global",
  "standing": true,
  "summary": "A change to src names its OpenSpec change in an Intent: trailer.",
  "body": [
    "A change that touches src/ is planned in an OpenSpec change.",
    "Name it with the trailer Intent: openspec:<id>, or change its folder in the same range."
  ],
  "tags": ["openspec"],
  "source": { "date": "2026-10-26", "by": "docs" },
  "check": {
    "type": "intent-required",
    "level": "fail",
    "if": ["src/**"],
    "sources": ["openspec"],
    "exempt_subjects": ["^chore\\(deps\\)", "^chore\\(main\\): release"]
  }
}
```

A failing row prints `no intent of <sources> for <n> changed paths that match the check's if`, with `1 changed path` for one path and `<sources>` printed as `file, openspec` for the default. A bump of a dependency and a release commit change paths too. Exempt them with `exempt_subjects`, as the entry does, or leave the lock files out of `if`. The `record` job after a squash merge sees one commit. A squash message that drops the `Intent:` trailers fails the recorded `intent-required` where the request passed. Keep the trailers in the squash message.

`intent-resolves` fails for each `Intent:` trailer of the range whose intent resolves neither at the head nor at the merge base. A trailer that names a change that the range removed passes. A failing row prints one line for each trailer: `<sha7> Intent: <source>:<ref>: <the adapter's line>`. A value with a control character, or one that starts with a double quote, prints as a JSON string, with its quotes and its `\u` escapes, so the line holds no control character and a printed value starts with a double quote exactly when it is a JSON string. A range with no `Intent:` trailer passes.

```json
{
  "id": "project.intents-resolve",
  "kind": "rule",
  "area": "global",
  "standing": true,
  "summary": "Each Intent: trailer names a spec that exists.",
  "body": [
    "A trailer that names a change or a file that the tree does not hold is a typo or a deleted spec."
  ],
  "tags": ["openspec"],
  "source": { "date": "2026-10-26", "by": "docs" },
  "check": { "type": "intent-resolves", "level": "fail" }
}
```

A knowledge schema that your project changed or owns lacks both types. See [the hand step of the upgrade guide](upgrading-to-2.md#hand-step-a-knowledge-schema-that-you-changed-or-own).

## Guidance for the agent

OpenSpec lets a project add advice for the agent that applies or archives a change, under `operations` in `openspec/config.yaml`. OpenSpec returns the advice when the agent runs `openspec instructions apply --change <id> --json` or `openspec instructions archive --change <id> --json`. Merge this block into the `operations` key of your own file:

```yaml
operations:
  apply:
    guidance:
      - Before you change a file, run houserules for <the files> and follow the rules it prints.
      - After the last task, run houserules audit --base <the base branch> and fix every failed row.
  archive:
    guidance:
      - Before you archive, run houserules audit --base <the base branch>; archive only when no row fails.
```

The agent replaces `<the files>` with the files that it changes and `<the base branch>` with the base branch of the project. With a store-selected OpenSpec root, OpenSpec reads the guidance from the store's config. OpenSpec treats the guidance as advice, not as a check: the enforcement stays with `houserules audit` and your CI. houserules reads no key of `openspec/config.yaml` beyond `schema` and `store`.

## The split in AGENTS.md

When git tracks a file under `openspec/specs/` or `openspec/changes/` of the install, `houserules render` writes this section into the houserules block of `AGENTS.md`, directly after `## Standing rules`:

```markdown
## OpenSpec

This project plans its changes with OpenSpec: a change's proposal, specs, design, and tasks live in `openspec/changes/<id>/`. Write a design decision into the change's `design.md`, not into a separate spec. houserules supplies the rules and the checks of each change: run `houserules for <path>` before you change a file, and `houserules audit --base <the base branch>` before you archive a change.
```

The section reads the tracked paths, so a clone and the work tree agree. After the first tracked file of an OpenSpec root, `check-knowledge` reports `AGENTS.md: generated file is out of date (run houserules render)` and `render --check` reports `AGENTS.md: would change`, until you run `houserules render`.

An install with a store-pointer root gets no section. When it runs the reference workflow, it gets the warning of [Plan with one workflow](#plan-with-one-workflow).

## Plan with one workflow

An install that runs the reference workflow (the `workflow` key of `.houserules.json`) and holds an OpenSpec root with a planning shape or a store pointer plans with two workflows. `init --workflow reference` and `check-knowledge` print this line on stderr and keep their exit code. `<version>` is the version of the binary:

```text
houserules: warning: this install runs the reference workflow and the project plans with OpenSpec (openspec/); plan changes with one of them. Beside OpenSpec, houserules needs no reference workflow: https://github.com/jblossey/houserules/blob/v<version>/docs/openspec.md
```

When the root has a planning shape, the two workflows give two instructions on where a design decision goes. The section of [The split in AGENTS.md](#the-split-in-agentsmd) tells the agent to write it into the `design.md` of the change. The `designing` skill of the reference workflow tells it to write the spec to `docs/specs/`.

### Remove the reference workflow

houserules 2.0 has no command that removes the reference workflow. To drop it, take these steps by hand.

**Step 1.** Delete each file that `houserules files` lists under `workflowFiles`:

```text
.claude/agents/implementer.md
.claude/agents/task-reviewer.md
.claude/agents/branch-reviewer.md
.claude/agents/spec-reviewer.md
.claude/skills/orchestrating/SKILL.md
.claude/skills/finishing-a-feature/SKILL.md
.claude/skills/designing/SKILL.md
.claude/skills/planning/SKILL.md
.claude/evals/dependency-add.json
.claude/evals/docs-edit.json
.claude/evals/seeded-violations.json
.githooks/commit-msg
tools/reference-session-start.sh
```

**Step 2.** Delete the `workflow` key of `.houserules.json`, with the object that it holds.

**Step 3.** When the Claude renderer is on, delete the `SessionStart` entry of `.claude/settings.json` whose command is `${CLAUDE_PROJECT_DIR}/tools/reference-session-start.sh`. `update` leaves that file as it is.

**Step 4.** Run `houserules update`. It writes `AGENTS.md` without the pointer of the workflow, and it writes none of the files back. `check-knowledge` then prints no warning.

Steps 1 and 2 go together. While the `workflow` key stays, `update` writes the files back. With the key gone, `update` adopts the workflow again while an unmodified file of it remains.

`.claude/evals/record.json` is your record of eval runs, and `update` never writes it. Keep it or delete it. The packs that the workflow required stay installed.
