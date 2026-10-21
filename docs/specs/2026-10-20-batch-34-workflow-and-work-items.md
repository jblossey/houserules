# Batch 34: the workflow and work items

**Status: APPROVED by the owner on 2026-10-20 (design.md 187), revision 4, with the rulings 179 to 186 on Q1 to Q8 and the controller calls C1 to C27.** Drafted from the approved meta-spec (`docs/specs/2026-10-15-release-2-0-0-meta-spec.md`), its rulings design.md 5.102 to 5.161, and the rulings 172 to 178 that came after it. Revision 2 folded in the 37 findings of the first design review (`spec-review.json`, 3 Critical); revision 3 folds in the 33 findings of the second (`spec-review-r1.json`, 2 Critical); revision 4 folds in the 25 findings of the third (`spec-review-r2.json`, no Critical), where the review loop ended (design.md 178). Section 12 holds the questions only the owner can rule. Section 13 lists the calls the controller made, for the owner to check at the spec gate.

Items: HR-180, HR-181, HR-194, HR-144, HR-129, HR-153, HR-165, HR-166, HR-150, HR-196, HR-172, HR-198, HR-199. Folded technical debt: HR-203 items 3 and 4 (section 9.3). Workspace: `.superpowers/sdd/2026-10-20-batch-34/` (ledger `progress.md`).

Tier: standard (design.md 172(2)). The tasks that change the CLI's commands, flags, or output, a schema, or the data on disk of an install are critical tasks (173): the spec's cases become tests before any code, one recorded RED run at BASE, the whole suite green at HEAD, and one task review on opus with no re-review. A branch review on opus (5.158(4)). One eval run covers the batch: the implementer and task-reviewer templates change (`process.evals-rerun`).

## 0. What rulings 172 to 178 change in the meta-spec

The meta-spec predates the purge of the full tier. This spec follows the later rulings:

- 172 supersedes 5.154, 5.156, 5.157, 5.158(1)-(3) and (5), 5.160(1), (3), and (4), and 170(2) where it names full-tier batches. So: no full tier exists; this batch runs the standard tier; no reference figures are measured, because the escalation statement of 5.157 is gone. Meta-spec section 4 (batch 34), O20 to O24a, and the agent-template row of 6.2 read with this.
- What stands: the default is the standard tier and the light tier stays (5.155); the branch reviewer's default model is opus (5.158(4)); the design review runs in the standard tier (5.159), repeated with a fresh reviewer until a round holds no Critical finding (178); the design reviewer's default model is opus (5.160(2)).
- 172(6), every batch measures its cost, stays this repository's rule; the kit does not ship it. Its entry `process.full-tier-measures-cost` names the full tier, so this batch adds `process.batches-measure-cost` with the same rule and retires the old id with a `see` link (`knowledge-base.ids-are-permanent`).
- HR-198 delivers 172 and 173 for the kit: two tiers and the critical-task check.
- The meta-spec carries a pointer to this section (section 4 and before O24a).

## 1. What the batch delivers

1. The work-item contract (HR-180; 5.103, 5.104, 5.128, 5.132, 5.136, 5.138, 5.139, 5.144, 5.145, 5.149, 5.150): one core contract for every backlog function, three adapters (JSON, GitHub Issues, Beads), one conformance suite, the `Work-Item:` and `Deferred:` trailers, and no default tracker.
2. The reference workflow (HR-181; 5.111, 5.113, 5.114, 5.119, 5.120, 5.125, 5.131, 5.135, 5.140, 5.141, 5.142): a workflow package with a manifest, required ids, and per-file pins, installed by `init --workflow reference`; the `model` field of every report and review.
3. The minimal default (HR-182's acceptance; interaction 10): `init` installs the core set and the `knowledge-base` pack, no tracker, and no workflow, and that install passes every gate. With 5.152's rewording (section 7), HR-182 closes in this batch.
4. Two tiers and the critical-task check (HR-198), the design review and the `spec-reviewer` template (HR-144, 178), technical-debt candidates in the design and plan steps (HR-194), and the superpowers map (HR-199).
5. The rewrite set of meta-spec 6.4, with the role rewording of 5.152, and the standing-entry changes the owner rules (section 8).
6. The riders: HR-129, HR-153, HR-165, HR-166, HR-150; reading houserules output through files (HR-196); HR-203 items 3 and 4.
7. The schema delivery batch 33 left to this batch (batch-33 spec C6): the backlog and deliverables schemas reach every install through `update`.
8. This repository on the new layout: the JSON tracker, the reference workflow, and, at Finish, the every-commit `Work-Item:` entry (5.138).

## 2. Facts the design rests on

The workspace files are under `.superpowers/sdd/2026-10-20-batch-34/`; each is the output of the command or script it names.

- F1. Kit files today (`fact-files.json`, `fact-files.cmd.txt`): 8 kit-owned files, 15 seed-once files (6 under `backlog/`, 4 eval files, the deliverables schema, AGENTS.md, CLAUDE.md, `docs/README.md`, `knowledge/areas.json`), 5 pack copies, 2 D3 files (the knowledge schema and the CI workflow).
- F2. The workflow files cite 25 seeded ids (`fact-required-ids.py`, its output `fact-required-ids.txt`): the count 5.125 measured. The migrating-knowledge skill, which stays core, cites five `knowledge-base` ids only.
- F3. The rewrite set of meta-spec 6.4 on batch-33 main (`python3 -I f4-rewrite-set-v4.py .`, output `f4-rewrite-set-v4.txt`; `rewrite-ids-v4.txt`): 36 entries, the meta-spec's 34 plus `knowledge-base.cite-durable-refs` and `knowledge-base.context-proxy-output-is-lossy`, which batch 33 added. Non-entry lines: template AGENTS.md 8, template docs/README.md 7, the migrating-knowledge skill 23, `rules/render/claude.rs` 4, `rules/render/mod.rs` 1.
- F4. Beads (`fact-beads.txt`, `internal/types/types.go` on its main branch, fetched 2026-10-20): `close_reason` is a JSON field of an issue, so `bd close --reason` keeps the reason readable through `--json` (M2's first open point). Statuses: open, in_progress, blocked, deferred, closed, pinned, hooked. Native types include bug, feature, task, epic, chore, decision. Issues carry `labels`, `dependencies`, `external_ref`, `spec_id`, `defer_until`. The JSON shapes of `bd list` and `bd show` (M2's second point) are measured against the pinned release in the task that builds the adapter.
- F5. `gh` 2.102.0 is installed here; `bd` is not. `gh issue list` caps its result at `--limit` (default 30) and has no paging flag; `gh api --paginate` pages. Its issue JSON gives `stateReason`, `labels`, `milestone`, and `body`; GitHub's close reasons are completed, not planned, and duplicate. `gh` has no milestone command; `gh api repos/<repo>/milestones` creates one (`spec-review-probe-gh-fields.txt`, `spec-review-probe-gh-ids-labels.txt`). This repository has 23 issues, 7 with no label; the issues endpoint returns 82 rows, 59 of them pull requests, and 53 closed rows have no close reason (`fact-gh-counts.txt`).
- F6. The superpowers plugin 7.0.0 ships 15 skills (`fact-superpowers-skills.txt`).
- F7. Kit texts that name the full tier (`fact-full-tier.py`, `fact-full-tier.txt`): 16 lines, the orchestrating skill lines 32, 36, 37, 38, 40, 41, 66, 88, and `template/packs/process.json` lines 829, 831, 832, 833, 835, 836 (`process.risk-tiers`), 891 (`process.bounded-review-loop`), and 920 (`process.knowledge-earns-its-place`). `houserules for --full` prints whole entries; it has nothing to do with tiers.
- F8. The `commits` check has `subject`, `body_absent`, and `body_line_max`; `per_commit` belongs to `co-change` (`check_commit.rs:7`). This repository's local `security-hygiene.no-coauthor` (`knowledge/security-hygiene.json`) matches a trailer line by a `body_absent` pattern; no kit check reads a trailer.
- F9. The agent templates' frontmatter names models: implementer `sonnet`, task-reviewer `opus`, branch-reviewer `fable`.
- F10. `init --target claude` installs the agent templates today; a plain batch-33 `init` writes the orchestrating, finishing-a-feature, and migrating-knowledge skills, the commit-msg hook, the evals, and a directory `AGENTS.md` per area that render writes (`spec-review-probe-init-files.txt`).
- F11. `update` writes every absent seed-once file, so on an install without `backlog/` it writes the six backlog files (`spec-review-probe-no-backlog.txt`).
- F12. Release commits (`fact-release-authors.txt`): `c44e7c4` (0.2.0-alpha) is authored by `github-actions[bot]`; the six from 0.3.0 to 1.4.0 by `Jannis Blossey`, because `release-please.yml` runs with the `RELEASE_PLEASE_TOKEN` PAT. Every release commit's subject starts `chore(main): release ` (`c44e7c4`: `chore(main): release houserules 0.2.0-alpha`). Dependabot commits are authored by `dependabot[bot]` (`spec-review-probe-commit-authors.txt`).
- F13. The deliverables schema names backlog ids in `issues[].backlog` (a finding's deferral) and `taskReport.backlog` (the items a task delivers); `backlogId` is `^(WI-[0-9]+|PP-[0-9]+-[0-9]+)$` with the stamped prefix (`spec-review-probe-deliverables-backlogid.txt`).
- F14. Claude Code resolves a subagent's model from the model the dispatch passes, then the template's `model:` frontmatter, then `CLAUDE_CODE_SUBAGENT_MODEL`, then the main conversation's model; before v2.1.251 the environment variable came first (sub-agents docs, "Choose a model", as re-read by the third review, `spec-review-r2.json` T10; `spec-review-r1-probe-agent-model.txt`).
- F15. This repository's `.claude/schemas/deliverables.json` equals the batch-33 template content after the `HR` rewrite, which differs from v1.4.0's; its `backlog/schema.json` equals v1.4.0's (`spec-review-r1-probe-repo-schemas.txt`).

## 3. The work-item contract (HR-180)

### 3.1 The contract

The core defines one contract (a Rust trait) that every backlog command calls; no command reads `backlog/` directly. Its operations are the 1.4.0 backlog functions plus the deferral of 5.103 and 5.128:

- read: `list` with its filters (`--open`, `--status`, `--type`, `--batch`, `--milestone`, `--section`, and the new `--deferred`), `batch <n>`, `get <id>` for the 1.4.0 kinds (items, amendments, parked items), and `resolve <id>` (exists, and the deferral's reason) for the gates (C31);
- write: `set <id> field=value...`, the backlog half of `archive`, and `defer`;
- check: the contract checks of 5.139 (duplicate ids, `see` references, batch members, duplicate batches, at most one batch in progress, done without a batch as a warning, amendment and parked references, every deferral has its reason) and the adapter's own checks;
- describe: the adapter's name, its id shapes (a pattern each), and its session-start and session-end commands for the AGENTS.md block (5.138).

`defer` has two forms:

- `houserules defer --title <t> --reason <r> --origin <ref> [--file <path>]... [--section <s>]` files a new deferral and prints its id on one line. On JSON the item goes to `backlog/items/<s>.json` (default `general`), with the id one above the highest number over the items and `backlog/archive/`, `type: chore`, `status: open`, `milestone: null`, `batch: null`, and `body: [<reason>]`; a new items file gets `section: <s>`, `heading: <s>`, `title: <s>`, and `spec: null`. A title longer than 120 characters is refused: `defer: the title is <n> characters; the limit is 120`, exit 2.
- `houserules defer --item <id> --reason <r> --origin <ref> [--file <path>]...` marks an existing item as a deferral and prints its id.

### 3.2 Equivalence across adapters

`list`, `batch`, and `get` print JSON, as at 1.4.0. Every command prints on every adapter what it prints on the JSON adapter, compared as parsed JSON: each id mapped through the test's seed map, the adapter-only field `file` removed, and rows compared as a set keyed by the mapped id (5.104, 5.132(1)). Row order: the JSON adapter keeps its file order, as at 1.4.0; the hosted adapters return rows in ascending id order; a test pins each.

`list --deferred` adds the `deferral` object (`reason`, `origin`, `files`) to each row.

### 3.3 Which tracker an install has

- `.houserules.json` gains `tracker`: `{"adapter": "json"}`, `{"adapter": "github", "repo": "<owner>/<name>"}`, or `{"adapter": "beads"}`. No key means no tracker.
- `init` installs none unless `--tracker json|github|beads` names one (5.138). `--tracker json` writes the JSON adapter's files (section 4.2, `trackerFiles`). `--tracker github` takes the repository from `--repo <owner>/<name>`, else from the `origin` remote, else refuses: `--tracker github needs a repository: pass --repo <owner>/<name>`, exit 2. `--tracker beads` writes nothing under `backlog/`: Beads keeps its own data (`bd init` is the project's step; the migrating-knowledge skill says so).
- Every command that sets `tracker` also writes the `backlogId` pattern of the deliverables schema for the adapter (3.9) and the seeded CI workflow for it (4.8).
- `init --tracker <a>` on an install that has the same tracker does nothing for the tracker, exit 0, so `init --workflow reference --tracker json` works on a JSON install. On an install with another tracker it refuses: `this install already has the <adapter> tracker; houserules changes no tracker`, exit 2.
- `init --tracker beads` reads the issue prefix from `bd` (for the `backlogId` pattern, 3.9); with `bd` missing or no store it refuses: `--tracker beads needs an initialized Beads store: run bd init (or bd bootstrap in a clone), then houserules init --tracker beads`, exit 2. `init` reads the prefix once, before any write, as `init --tracker github` resolves its repository; a refused `init --tracker` writes nothing on every adapter, and a pattern is written only from a prefix that `bd` named in the same command: a failed read is the unreachable line of 3.4, exit 2, with the tracker key and the schema unwritten (C37). `update` re-reads the prefix where `bd` is reachable and rewrites the pattern when it changed. Where `bd` cannot be reached, `update` keeps the pattern, prints the unreachable line of 3.4 on stderr, and finishes with its usual exit (C38).
- `init --tracker github` or `beads` on an install that has a `backlog/` directory refuses: `backlog/ holds JSON tracker data; move it as the migrating-knowledge skill describes and remove backlog/, or run houserules init --tracker json`, exit 2.
- The 1.x conversion gives an install with a `backlog/` directory `{"adapter": "json"}` (5.138(1); section 4.7 step 1). A 1.x install with no `backlog/` gets no tracker: 5.149(2) presumes the conversion keeps a tracker, but with no data there is nothing to adopt, so `update` prints the tracker hand step of 4.7 instead (a departure, stated). A 2.0 install with a `backlog/` directory and no `tracker` key keeps 5.144(2)'s line B until the project runs `init --tracker json`; this repository does so in this batch (section 10).

### 3.4 With no tracker, or an unreachable one

The lines of 5.138, 5.144, 5.145, 5.149, and 5.150, exact (each a test).

- Line A: `no tracker installed; run houserules init --tracker json|github|beads to add one`
- Line B: `backlog/ exists but no tracker is installed; run houserules init --tracker json`
- The archive line: `archive: skipped the backlog half: no tracker installed; run houserules init --tracker json|github|beads to add one`
- The audit line, printed on every `audit` of an install with no tracker, `skipped 0` included (5.149(1)): `skipped <n> work-item references: no tracker installed (houserules init --tracker)`

| Command | No `backlog/` directory | A `backlog/` directory exists |
|---|---|---|
| `check-backlog` | line A, exit 0 | line B, exit 1 |
| `list`, `batch`, `set`, `defer`, and `get` of an id that 3.8 sends to the tracker | line A, exit 2 | line B, exit 2 |
| `archive` | moves the knowledge entries, prints the archive line, exit 0 | moves nothing, line B, exit 1 |
| `audit` | resolves no trailer and no deliverable deferral, prints the audit line, exit as its other checks decide | line B, exit 1 |

For `get` of an unresolved knowledge-shaped id whose pack the binary carries but the install lacks, the pack line of 3.8 replaces line A or line B (5.150); exit 2.

An installed tracker that cannot be reached is never a pass (5.103(2)). Every command that needs it prints `tracker <adapter> cannot be reached: <cause>; <setup step>`: the gates (`check-backlog`, `audit`) exit 1; `list`, `batch`, `get`, `set`, `defer`, and `archive` exit 2. The causes and setup steps, exact: JSON `backlog/ is missing` / `restore backlog/ from version control`; GitHub `gh is not installed` / `install gh`, `gh is not logged in` / `run gh auth login`, `gh cannot see <owner>/<name>` / `check tracker.repo in .houserules.json and gh auth status`; Beads `bd is not installed` / `install bd`, `no Beads store` / `run bd bootstrap in a clone, or bd init for a new store`; for every other failure of either tool, the tool's first stderr line / `run the command by hand to see the full error`; for `bd`, which prints warnings before its error, the first stderr line that is not part of a `warning` block, else the first line, else the exit status (C36). The Beads steps follow its sync docs (a fresh clone runs `bd bootstrap`), read but not run here; the task records them against the pinned release in `docs_verified`.

### 3.5 The adapters

JSON is today's files and code behind the contract, unchanged in behavior: the item, amendment (`A-nn`), and parked (`PP-n-nn`) id shapes, its four file checks (schema, emitter round trip, section equals file name, archive mirror; 5.139), and `archive` moving done records to `backlog/archive/`. `defer` writes the `deferral` object, an optional field the backlog schema gains.

GitHub Issues runs `gh`: reads through `gh api --paginate repos/<repo>/issues?state=all&per_page=100`, writes through `gh issue` and `gh api`. houserules never reads, takes, or stores a token value; `gh` keeps its own authentication (5.136). Beads runs `bd` with `--json`. A missing tool is a setup error that names the tool and its install step (5.107). Each write creates what it needs and that does not exist: a label (`gh label create`, `bd label`), a GitHub milestone (`gh api repos/<repo>/milestones -f title=<m>`). So `set` and `defer` never fail on a new batch or milestone.

Which records an adapter holds: on GitHub, the adapter drops every row that has a `pull_request` key and reads only `tracker.repo`; a trailer that names a pull request fails with `<short-sha>: Work-Item #<n> is a pull request, not an issue, in the github tracker`, and an `<owner>/<repo>#<n>` id is not resolved. An item is an issue with a `type:<t>` label; a record is an issue with a `houserules:<kind>` label. `list`, `batch`, and the contract checks read only those. `resolve` and `get` of any other issue print the 1.4.0 item shape with `type`, `section`, and `milestone` null, so a `Work-Item:` trailer may name any issue of the repository (HR-180's acceptance). On Beads, the same rule with the `type:<t>` label and the custom record types.

Read direction: every backend state maps to one JSON status. Beads `in_progress`, `blocked`, `hooked`, `pinned`, and `deferred` read as `open`; the custom status `partial` is configured as `partial:active` (so `bd ready` shows it). GitHub closed with a null or `reopened` close reason reads as `done`. An issue with two `type:` labels fails the adapter check: `#<n> has two type labels`; an issue with no `section:` label reads as section null.

Rejected native means, each with its best case: GitHub sub-issues for batch membership (a native tree) and the native issue type (one field, not a label) lose because issue types exist only for organizations and labels work alike on both backends; Beads epics with parent-child links for batches and the native milestone type lose for the same reason; Beads' native `deferred` status with `defer_until` would hide a deferral from `bd ready`, against 6.2, which wants open deferrals found.

The houserules block is a fenced block (` ```houserules `) at the end of an issue body or description, holding the fields that have no native home as `key: value` lines (a list as JSON). The adapter writes it and reads it; nothing else in the body counts.

| JSON backlog | GitHub Issues | Beads |
|---|---|---|
| item id `<prefix>-nnn` | `#<n>` (input also takes the bare `<n>`, since `#` starts a shell comment) | the Beads id |
| `title` | issue title | title |
| `body` (paragraph list) | the issue body before the houserules block, paragraphs split on blank lines | description, the same split |
| `type` | label `type:<t>` | label `type:<t>`, and the native type feat→feature, bug→bug, fix→bug, chore→chore, decision→decision, any other→task |
| `status` open / partial / done / dropped | open / open with label `status:partial` / closed as completed / closed as not planned (read: duplicate too) | open / custom status `partial` / closed / closed with close reason `dropped` |
| `milestone` | the GitHub milestone | label `milestone:<m>` |
| `batch` | label `batch:<n>` | label `batch:<n>` |
| `section` (the items file) | label `section:<s>` | label `section:<s>` |
| `see` | block `see:` | `related` dependencies |
| `rulings` (date, by, text) | block `rulings:` | block `rulings:` |
| `tags` | labels `tag:<t>` | labels `tag:<t>` |
| `label`, `extra_lines` | block `label:`, `extra_lines:` | the same |
| `deferral` (`reason`, `origin`, `files`) | label `houserules:deferred` and block `deferral:` | label `houserules:deferred`, block `deferral:`, and a `discovered-from` dependency when the origin is a Beads id |
| section metadata (heading, title, spec of an items file) | an issue labelled `houserules:section`, fields in its block | custom type `houserules-section` |
| batch record (`number`, `summary`, `kickoff`, `status`) | an issue labelled `houserules:batch`, titled `Batch <n>: <summary>`, the rest in its block; open while planned or in progress | custom type `houserules-batch`, the same fields |
| amendment, decision, parked group and item | issues labelled `houserules:amendment`, `houserules:decision`, `houserules:parked` | custom types `houserules-amendment`, `houserules-decision`, `houserules-parked` (never the native `decision`, which an item of type decision uses) |
| `archive` (backlog half) | moves nothing; prints `archive: the github tracker keeps closed items in place` | the same with `beads` |

Record layout (C30). The adapters read this layout, and the conformance suite pins it. An amendment keeps `type`, `label`, and `date` in its block, its text is the body, and it is `done` when its issue is closed. A parked group is an issue labelled `houserules:parked` and `batch:<n>` whose block holds `group: true`; its body is the group's intro. A parked item is an issue labelled `houserules:parked` and the `batch:<n>` of its group; its text is the body and `resolved` sits in its block. A batch record keeps `kickoff` and `status` in its block. A section or decision record is read by no `list`, `batch`, or contract check; `get` prints it with the item shape like any other issue. `set` and `defer --item` write items only: an id that names a record, or an issue with no `type:` label, gets the JSON adapter's `unknown item "<id>"`, exit 2, before any write. On Beads the same layout with the custom types.

Batch membership (C29). The JSON adapter is unchanged: `batch <n>` lists the `items` of its `batches.json` record, which `set <id> batch=<n>` does not change. On GitHub and Beads the batch record has no items field: `batch <n>` lists the items that carry `batch:<n>`. This is the one exception to 3.2's equivalence; `list --batch <n>` prints the same rows on every adapter.

The contract's `set` takes `status`, `batch`, and `milestone` on every adapter; the JSON adapter writes `milestone` to the item (live case 9 and "never fail on a new batch or milestone" above). One parser and one validator for the inputs of `set` and `defer` serve every adapter.

GitHub's adapter check fails, naming `gh label create <label>`, when a `houserules:<kind>` label is missing; Beads' fails, naming the `bd config` step, when a custom type or status is missing. The implementer pins the `bd` release and records the JSON shapes of `bd list` and `bd show` in `docs_verified`.

### 3.6 The conformance suite

One suite runs against every adapter (5.132(1)), one test per row of the mapping table and per contract check. The JSON adapter runs in-process. Beads runs against a real `bd` of the pinned release in a scratch directory; a Beads test fails, never skips, when `bd` is missing, and `mise.toml` pins `bd` so that the CI jobs on all three operating systems install it (`security-hygiene.dependency-vetting` applies; the task records the platforms Beads releases for, and a platform without a release gets the suite through a stated job). GitHub Issues runs against a fake `gh`: a test executable that serves the `gh` JSON shapes from a scratch store. CI never writes to a GitHub repository; the live run proves the adapter against a real one: a private scratch repository the controller creates, named in the batch report with a reminder to delete it (design.md 180).

### 3.7 Trailers, deliverable deferrals, and `audit`

- `Work-Item: <id>` names a work item of a commit, and a commit may carry several (an aggregated commit spans several items). `Deferred: <id> <reason>` names a deferral the commit makes (5.103, 5.138). Trailers parse by git's trailer rules (`git interpret-trailers --parse`: the last paragraph's `Key: value` lines). houserules reads neither from a PR body.
- `audit` matches each trailer id against the adapter's declared id shapes (3.1) first; an id that matches none fails with the not-resolve line below and reaches no backend. Every backend call passes ids as separate arguments with no shell, after `--` where the tool takes options.
- A `Deferred:` trailer passes when its reason text is not empty and the tracker item carries the deferral marker with a non-empty reason; the two texts are not compared. A trailer with no reason fails: `<short-sha>: Deferred <id> names no reason`.
- `audit` resolves every trailer of every commit in its range through the installed adapter. Lines: `<short-sha>: Work-Item <id> does not resolve in the <adapter> tracker`; `<short-sha>: Deferred <id> is not a filed deferral with its reason in the <adapter> tracker`. Exit 1.
- `audit --report` and `--workspace` resolve every `issues[].backlog` id of the reviews and reports they read (F13): each is a filed deferral with its reason, else `<file>: <id> is not a filed deferral with its reason in the <adapter> tracker`, exit 1. With no tracker these ids count in the audit line of 3.4, and `validate` accepts any id shape (3.9): the two commands agree that a tracker-less install leaves ids unchecked.
- Requiring a `Work-Item:` on every commit is opt-in (5.138): the process pack ships `process.every-commit-names-a-work-item`, a `commits` check with new fields: `require_trailer` (a string, the trailer key: `Work-Item`), `exempt_authors` (a list of names: `dependabot[bot]`, `release-please[bot]`, `github-actions[bot]`), and `exempt_subjects` (a list of patterns: `^chore\(.+\): release `). An author matches by name (`git log --format=%an`). The subject exemption exists because release-please's commits are authored by whoever owns its token (F12): a PAT makes them a human's, so an author list alone fails every release commit made that way. It extends 5.138(3)'s author list (design.md 186, Q8). In the commit-msg hook's message-file mode there is no author yet, so the author exemption does not apply there; the subject exemption does. The line: `<short-sha>: no Work-Item trailer (process.every-commit-names-a-work-item)`; in message-file mode, which has no commit yet, `no Work-Item trailer (process.every-commit-names-a-work-item)` (C32). Trailers parse under git's default separator `:`, whatever the repository's `trailer.separators`.
- The opt-in. A pack manifest lists such ids under `optIn`. Every path that brings an `optIn` id onto an install (`pack add`, `pack upgrade`, `update`'s pin move, `init --add-required-packs`, the 1.x conversion) writes an exclusion with the reason `opt-in` once, and records the id in `.houserules.json` `optInOffered`, and prints `<id> is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion` (C32). A later command never writes the exclusion again for an id in `optInOffered`, so a project that deletes the exclusion has opted in for good. `pack remove` drops from `optInOffered`, with its exclusion, every id the project had not opted in to, so a later `pack add` offers it again; an id the project opted in to stays, and a later `pack add` prints `<id> is active: this install opted in before`. An `optIn` id is inactive on every install until its project opts in.
- The paper walk of the new gate (`process.gates-cover-generated-commits`), for every install that opts in: a human commit names its item; an aggregated commit names every item it delivers; a batch-close or docs commit names the batch's items it records; release-please's commits pass by subject, dependabot's by author; `git revert` writes no trailer, so the reverting author adds one (`git revert -e`), a red the upgrade guide names; a merge commit is checked as any other (an ff-only repository has none). The workflow's texts teach the trailer on every commit (4.1). For this repository, section 10.
- Schema changes, each an addition and a minor version (D2): the knowledge schema's `check` gains `require_trailer`, `exempt_authors`, `exempt_subjects`, in the knowledge schema and the embedded pack schema; the pack schema gains `optIn` (a list of ids); the install schema gains `tracker`, `workflow`, and `optInOffered`; the backlog schema gains the item's `deferral`; the deliverables schema gains `model` (required, a tightening the 2.0.0 major covers), the `spec-review` kind, and `fix_rounds[].gates_run`. The plan names each new version.

### 3.8 `get` routing (5.139, 5.150)

A knowledge read never needs the tracker. A knowledge-shaped id (`<topic>.<rest>`) whose first part is a loaded topic goes to the knowledge base, and `get` asks the adapter about it only when the id also matches one of the adapter's declared id shapes (3.1); an id that resolves in both fails: `<id> resolves both in the knowledge base and in the <adapter> tracker`, exit 2. Every other id goes to the adapter. An unresolved knowledge-shaped id whose first part names a pack the binary carries but the install lacks prints `<id>: pack <pack> is not installed; run houserules pack add <pack>`: with no tracker in place of line A or B, with a tracker after the adapter's not-found line. `get` exits non-zero. A test runs `get process.tdd` with an unreachable fake `gh` and expects the entry.

### 3.9 The id pattern of the deliverables schema (5.132(3))

`backlogId` accepts exactly the installed adapter's shapes: JSON the stamped prefix, `A-nn`, and `PP-n-nn`; GitHub `#<n>`; Beads the configured issue prefix. With no tracker it accepts any token of `^[A-Za-z0-9#._-]+$`, and `audit` counts such ids as skipped (3.4). Every command that sets `tracker` writes the pattern (3.3), and `update` writes it from `tracker.adapter` when it delivers the schema (4.7 step 4).

### 3.10 Other outputs

- The session commands that render writes into the AGENTS.md block: JSON `houserules list --open` at start and `houserules check-backlog` at end; GitHub `gh issue list --label houserules:batch --state open` and `houserules check-backlog`; Beads `bd ready` and `bd dolt push`.
- `pack add process` with no tracker: `pack process needs a tracker; run houserules init --tracker json|github|beads`, exit 2 (5.149). It replaces batch 33's line (`pack/mod.rs:369`).
- The session block also says: "Write houserules output to a file and read the file; treat a `<<` marker or a 'more items' note as a lost read" (6.3), because the session commands print long output.
- An install that holds the process pack or the workflow and no tracker (4.7, R4 of the second review): `check-knowledge` prints `pack process needs a tracker; run houserules init --tracker json|github|beads` and, with the workflow, `the reference workflow needs a tracker; run houserules init --tracker json|github|beads`, exit 1.

## 4. The reference workflow (HR-181)

### 4.1 The package

- A workflow package lives in the source tree at `template/workflows/<name>/`: a `workflow.json` manifest and the files at their install paths. The binary embeds it.
- The manifest holds `name`, `version` (the binary's version: the reference workflow ships "versioned with" the binary, 5.140(1)), `schema` (`houserules/workflow@1.0.0`, 5.117), `requires` (the rule ids its files cite), `files` (each install path), `models` (each role's default: implementer `sonnet`, task-reviewer `opus`, branch-reviewer `opus`, spec-reviewer `opus`; 5.119, 5.158(4), 5.160(2)), and `agentsBlock` (the text render writes into the AGENTS.md block while the workflow is installed).
- `agentsBlock` is the workflow's pointer for every harness: "This install runs the reference workflow. Read `.claude/skills/orchestrating/SKILL.md` at the start of a session and before any handoff. If your harness can hand a task to another agent the way `.claude/agents/*.md` describes, run handoffs in parallel only when the plan marks their file sets disjoint, each in its own worktree; a harness with no subagents does the work directly, in its own session." The rewrite of section 7 removes the template AGENTS.md lines that say this today; the block carries them where the workflow is installed.
- The reference workflow's files: four agent templates (implementer, task-reviewer, branch-reviewer, the new spec-reviewer), the orchestrating and finishing-a-feature skills, the new designing and planning skills (section 6.4), the three eval scenarios, the commit-msg hook, and `tools/reference-session-start.sh` with its hook entry.
- `.claude/evals/record.json` is project data: `init --workflow reference` writes it empty, and nothing pins, replaces, or reports it (5.142).
- A test asserts that `requires` equals the set of seeded ids the workflow's files cite (5.125, `fact-required-ids.py`); it is 25 today and grows with this batch's texts. `process.conventional-commits` is not required.
- One source for each role's model (5.119): the templates carry no `model:` frontmatter line; `.houserules.json` `workflow.models` holds the value; the orchestrating skill names it on every dispatch and checks that each dispatch names one, because a dispatch that names none runs on the controller's own model (F14). Rejected: `update` writing the frontmatter from `workflow.models` as a fallback; its best case is a safe default for a dispatch that forgets the model, but it makes a project's model change a locally modified file on every pin move. A test asserts no shipped template has a `model:` line.
- The trailer: the implementer template and the finishing-a-feature skill write a `Work-Item:` trailer for each id of the brief's `Backlog:` line on every commit, and the orchestrating skill names those ids in every dispatch. The texts never cite `process.every-commit-names-a-work-item`: an `optIn` id is excluded by default, and a cited excluded id would fail the requirement check and the kit-citation lint (5.112 allows no waiver). A test asserts that no workflow file cites an `optIn` id. The eval scenarios expect the trailer.

### 4.2 Files and their classes

The invariant: an install holds the files of the parts it installed, and `update` restores only those. `houserules files` lists each path in one class:

- `kitOwned`: `tools/claude-session-start.sh` (written only where the Claude renderer is on), `.claude/skills/migrating-knowledge/SKILL.md`, `.claude/skills/houserules-step/SKILL.md`, and `tools/houserules-step.sh` (4.4a).
- `seedOnce`: `knowledge/areas.json`, `AGENTS.md`, `CLAUDE.md`, `docs/README.md`.
- `d3Files`: `knowledge/schema.json`, `.claude/schemas/deliverables.json`, `.github/workflows/knowledge.yml` (GitHub-hosted only).
- `trackerFiles`: the five JSON backlog data files (`backlog/batches.json`, `backlog/amendments.json`, `backlog/decisions.json`, `backlog/parked.json`, `backlog/items/general.json`) as seed-once, and `backlog/schema.json` as a D3 file; written by `init --tracker json`, and restored or delivered by `update` only where `tracker.adapter` is `json`.
- `workflowFiles`: every file of the installed workflow's manifest.
- `workflowData`: `.claude/evals/record.json`.
- `packCopies`: as today.

### 4.3 Install, pins, and `update` (5.140)

- `.houserules.json` gains `workflow`: `name`, `version`, `files` (one SHA-256 per file: the content the kit last wrote, the 1.4.0 baseline mechanism), and `models` (copied from the manifest; project-owned: `update` keeps the project's values and adds a missing role).
- The workflow writes all its files whatever the render targets: they are plain Markdown and scripts any harness reads. Only the hook entry in `.claude/settings.json` needs the Claude renderer: written where `renderers` holds `claude`, and added by a later `init --target claude`.
- `init --workflow reference` needs a tracker (5.138): with none installed and no `--tracker`, it refuses: `the reference workflow needs a tracker; run houserules init --workflow reference --tracker json|github|beads`, exit 2. It prints the packs that provide its required ids, and adds them only with `--add-required-packs` (M7c).
- A workflow path with no pin belongs to the project: `update` never writes it and, after the one report of 4.7 step 2, prints nothing for it. `init --workflow reference` over a project's own file at a workflow path, and a new binary whose workflow adds a path the project already holds, treat it as 4.7 step 2 does. `workflow.files` maps such a path to `null` once its line is printed: `update` prints the step-2 line for a manifest path that `files` does not hold and whose file is not the kit's, then records the `null`; a path that `files` maps to `null` gets no line (C33). A kept file whose bytes the project merged to equal the binary's content gets its pin moved, with no line.
- `update` moves the pin with the binary: it writes each file whose bytes equal its pin and moves the pin; it keeps a file whose bytes differ from its pin and prints `kept <file> (locally modified; reference workflow <version>)` on every run until the project merges by hand; it never writes a path in `ownedPaths` and prints nothing for it.
- `check-knowledge` checks every required id comes from exactly one installed, non-excluded source (the local pack counts). Its lines, one per state, exit 1: not provided, `workflow reference requires <id>: no installed pack provides it; run houserules pack add <pack>`; excluded, `workflow reference requires <id>, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it` (5.112); provided by no pack the binary carries, `workflow reference requires <id>: add it to the local pack`. The kit-citation lint covers the workflow's files where the workflow is installed.

### 4.4 What stays core

The migrating-knowledge skill (5.114), `check-commit`, the report format and the deliverables schema (5.124), and the Claude renderer's compaction re-injection of `houserules standing` in `tools/claude-session-start.sh` (5.131). `--target claude` installs that script and its hook entry, and no agent template (F10). The hook entry is the compaction entry alone: the seeded `startup|resume|clear|fork` entry and the script's `start` mode go, because the ritual moved to the workflow; `update` removes that entry from `.claude/settings.json` where it equals the kit's (C40). The `project-knowledge` skill keeps `for`, `get`, and the topic list; its dispatch steps (`Knowledge:`, `REPORT_FILE`, `audit --base`, "as a dispatched subagent") move into the workflow's agent templates; its description becomes "Use before reading or changing any file in this repository" (5.141).

### 4.4a The workflow-agnostic step (issue 61, design.md 184)

Every install gets a core skill, `.claude/skills/houserules-step/SKILL.md`, and a core hook script, `tools/houserules-step.sh`, that any workflow can call:

- `tools/houserules-step.sh before <path>...` writes the rule package of the paths (`houserules for`) to `.houserules/step/rules.json` and prints that file's path, so an agent reads it with its file-read tool (6.3).
- `tools/houserules-step.sh after <base>` runs `houserules audit --base <base> --json .houserules/step/audit.json` and prints the file's path and the audit's exit status; with a report path as a second argument it also runs `houserules validate` on it.
- The skill names the four steps (read the rules for the files to change, implement, audit, write the report in the core report format) and the two hook calls. It names no workflow and no plugin.
- `.houserules/step/` is scratch output; `init` adds it to `.gitignore`. The script also writes `.houserules/step/.gitignore` holding `*` before its first output, so an install that `update` gave the step keeps the output out of `git status` too (C34).

No recipe for another workflow ships (184(2)).

### 4.5 The `model` field (5.120) and the `spec-review` kind

- Every task report, review, re-review, branch review, and spec review carries `model`: the configured model that its task names (a model alias or a model id), `human`, or `unknown`. `validate` requires it. The dispatch tells the agent the configured model to record. `stats` groups cost and findings by `model` under a new key `models` (one row per model family: an alias and every id of the family haiku, sonnet, opus, or fable share the row named after the family, and any other value keeps its own row, C48; each row holds the `cost` fields and the finding counts by severity); a deliverable without the field counts in the row `pre-2.0`.
- The `spec-review` kind (HR-144): required `kind` (`spec-review`), `model`, `target` (the spec's path), `base`, `head`, `strengths` (prose list), `findings`, and `verdict` (`approve` or `needs-fixes`); optional `scope`, `facts` (each `id`, `verdict` confirmed|wrong|partial, `probe`), `controller_calls` (each `id`, `verdict` sound|unsound|needs-owner, `why`), `coverage`, `questions_for_owner` (each `id`, `question`, `recommendation`), and `prior` (each `id`, `verdict` addressed|partial|not-addressed, `where`). A finding has `id`, `severity` (`critical`, `important`, `minor`, the schema's and `stats`' lowercase vocabulary), `section`, `what`, `evidence`, `fix`. `stats` reads spec reviews from `spec-review.json` and `spec-review-r<n>.json` in a workspace (C28) and counts their findings by severity and their cost in the `workspaces` and `models` rows, as a review.

### 4.6 The minimal default (HR-182, interaction 10)

`init` with no flag writes exactly: `.houserules.json`, `knowledge/schema.json`, `knowledge/areas.json`, `knowledge/packs/knowledge-base.json`, `.claude/skills/migrating-knowledge/SKILL.md`, `.claude/skills/houserules-step/SKILL.md`, `tools/houserules-step.sh`, the `.houserules/step/` line in `.gitignore`, `.claude/schemas/deliverables.json`, `AGENTS.md` with its block, each directory `AGENTS.md` that render writes for the installed packs (none for the `knowledge-base` pack alone), `CLAUDE.md`, `docs/README.md`, and `.github/workflows/knowledge.yml` on a GitHub-hosted target. A unit test pins the code's constant to this list. Every gate passes: `check-knowledge`, `check-backlog` (line A, exit 0), `audit`, `validate`, `render --check`, and `check-commit` (it prints that it checked nothing, 5.125). `update` twice writes nothing under `backlog/` and no eval file.

### 4.7 Conversion steps (5.148)

The invariant: a conversion step acts only on state the kit wrote, and never writes a file the project owns or changed. A file is the kit's when `.houserules.json` `baselines` holds its path, or its bytes equal a known kit content after the id-prefix rewrite. The known contents are every released content (5.142) and the batch-33 main content of each file (F15). The same provenance holds for a `kitOwned` file: `init` and `update` replace one whose bytes equal a known kit content, as they replace one at its baseline (C35). `update` runs these steps on every run, each keyed on its own state, after the batch-33 steps:

1. Inside the 1.x conversion only (5.138(1)): a `backlog/` directory gets `{"adapter": "json"}`. A 1.x install with no `backlog/` gets no tracker.
2. One or more workflow files that are the kit's, and no `workflow` key: adopt the reference workflow at its embedded version. Each kit file is pinned at its recorded path baseline, else at the known content it equals. An absent workflow file is written and pinned. A workflow path that holds a file that is not the kit's is the project's own: it is left as found, gets no pin, and is reported once: `hand step: <file> is not the kit's; the reference workflow leaves it to the project`. A path in `ownedPaths` is left as found, with no pin and no line. For each required id that `knowledge/overrides.json` excludes, a hand step: `hand step: workflow reference requires <id>, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it` (5.112). Where the install has no tracker after step 1: `hand step: the reference workflow and the process pack need a tracker; run houserules init --tracker json|github|beads` (and 3.10's `check-knowledge` lines until then). A 2.0 install with no kit workflow file never adopts the workflow.
3. `tools/claude-session-start.sh` holding the 1.x ritual line: where the script is the kit's, replace it with the core script, write `tools/reference-session-start.sh` where the workflow is installed, and add its hook entry to `.claude/settings.json` where `renderers` holds `claude` and the file is not in `ownedPaths`; where the project changed it, keep it, write `tools/reference-session-start.sh` and its hook entry as in the kit's branch (the hand step needs them to complete; the ritual prints once, C46), and print `hand step: tools/claude-session-start.sh holds the reference workflow's ritual line; move it to tools/reference-session-start.sh` (C39); where it or `.claude/settings.json` is in `ownedPaths`, leave both as found, with no line.
4. The backlog and deliverables schemas (batch-33 spec C6): delivered as D3 files where they are the kit's (the known contents above); a modified copy is kept and listed as a hand step; the `backlogId` pattern is written from `tracker.adapter` (3.9). The backlog schema only where `tracker.adapter` is `json`.
5. The `project-knowledge` skill: `render` rewrites it (generated).

A second `update` changes nothing.

### 4.8 The seeded CI workflow per adapter

The invariant: a seeded CI gate reaches the installed tracker, in valid workflow syntax. Every command that sets `tracker`, and `update`, write the CI workflow for the adapter, for `knowledge.yml` and each subdirectory install's `knowledge-<folder>.yml` (5.168), as D3 files: GitHub adds a job-level `permissions: {contents: read, issues: read}` and a step-level `env: {GH_TOKEN: ${{ github.token }}}` to the steps that run `check-backlog` and `audit`; Beads adds a step that installs `bd` at the pinned release and runs `bd bootstrap`; JSON and no tracker keep today's steps. A CI file that `update` keeps as modified gets a hand step naming the lines to add. This departs from 5.144(3), which kept the `check-backlog` step unconditional so that a later tracker needs no edit to a seed-once file: the CI file is a D3 file that `update` keeps current (batch 33), and a hosted tracker needs a token and a tool the step cannot have without an edit.

## 5. Two tiers and the critical-task check (HR-198, 172, 173)

- The 16 lines of F7 are rewritten: `process.risk-tiers`, `process.bounded-review-loop`, `process.knowledge-earns-its-place`, the orchestrating skill's Tiers section, and the task reviewer's tier check name two tiers, light and standard (5.155). The agent picks between them. The light criterion's "no `full-tier` entry applies" becomes "no task is critical": 5.155(2) keeps the light tier as it is, but its criterion names the tag that 172(1) removes, and 173(1)'s critical task is the closest test of the same risk (a departure, stated). The dispatch line becomes `Tier: <light|standard> (<criterion>)`.
- The `full-tier` tag is dropped (172(1)): no kit text names it, and the upgrade guide says that an adopter's own `full-tier` tags no longer change anything.
- A critical task is one whose change alters the CLI's commands, flags, or output, a schema, authentication or security code, or the data on disk of an install (173(1)). The plan writes the spec's cases for it as tests before any code; the implementer commits them first with one recorded RED run at BASE, then makes them pass, still test-first for every further change; the evidence is that RED run and the whole suite at HEAD. One task review by the task reviewer's configured model (default opus), never a re-review; the person who runs the batch checks each fix by re-running the reviewer's reproduction and records it (`process.fix-round-verification-record`): a Critical or Important finding goes to a fresh implementer, a Minor one is fixed directly.
- `process.tdd` states the critical task's evidence clause. The implementer and task-reviewer templates carry the critical-task rules. Kit texts name a role's configured model with its default, never a vendor model alone.
- This repository's local `process.no-full-tier` and `process.critical-tasks-test-first` retire when the process pack carries the rule (one id, one source).

## 6. Design, plan, and the superpowers map

### 6.1 The design review (HR-144, 5.159, 5.160(2), 178)

- A `spec-reviewer` agent template (configured model, default opus) attacks a spec's decisions before the human sees it: it reruns the spec's probes, checks every cited fact and ruling, and files its findings in a `spec-review` deliverable (4.5), a new kind of the deliverables schema (an addition, minor under D2).
- The designing skill runs it for every standard-tier batch. After a review that holds a Critical finding, the author fixes every finding and a fresh reviewer reviews the spec again; the loop repeats until a review holds no Critical finding (design.md 178). Important and Minor findings are folded into the spec without a further review; then the spec goes to the human.
- `validate` and `stats` cover the new kind.
- The design-phase rules of issue 38's part 3 ship as four non-standing process-pack entries that the spec-reviewer template cites (design.md 182): a rejecting or departing decision carries a captured probe, the rejected option's best case, and a reversal condition; each supporting claim is labelled run, read, or assumed, and an assumed claim never alone rejects; a spec lists the concepts it takes from its sources, with their status and a check against the goal; an overturned decision is a `Reversal:` ledger line, counted at the retrospective.

### 6.2 Technical-debt candidates (HR-194, 5.127, 5.128)

At the spec and plan steps of a standard-tier batch, the designing and planning skills run `houserules list --deferred` and fold in every open deferral whose files overlap the planned files and that passes the convenience criterion of 5.128. The criterion (design.md 185, amending 5.128): folding the candidate in does not turn a non-critical task into a critical task (173(1)). The spec lists each folded candidate with its overlapping files, and each overlapping deferral left out with the reason.

### 6.3 Reading houserules output through files (HR-196, 5.137)

The workflow's templates and skills tell an agent to write houserules output to a file in the workspace (`houserules get <ids> > <workspace>/knowledge-<task>.json`) and read the file with its file-read tool, and to treat a `<<` marker or a "more items" note as a lost read and read the source file. The migrating-knowledge skill gives the same advice. No proxy is changed or configured (the owner's constraint of 5.137).

### 6.4 The superpowers map (HR-199, 5.161)

The kit names no superpowers skill and no superpowers plugin: a case-insensitive grep for `superpowers` over `template/` (the workflow included) and the binary's embedded texts finds nothing. The released contents in `crates/houserules/released/` are outside the grep: they keep the bytes of earlier releases, which the provenance rule of 4.7 compares and never writes (C42). Today the orchestrating skill names three skills and `process.brainstorm-first` names "the superpowers brainstorming and spec skills"; all go. `process.brainstorm-first` keeps its summary; only its first body line, which names the plugin's skills, is rewritten to name the designing skill; the rule does not change. Each plugin skill (F6) and what covers it:

| Plugin skill | Covered by |
|---|---|
| brainstorming | the new designing skill: it drafts the spec from what the project holds (the item, its rulings, earlier specs) and asks the human only what they leave open |
| writing-plans | the new planning skill: tasks, file sets, the code-health scan, technical-debt candidates (6.2), tiers and critical tasks |
| executing-plans | the orchestrating skill, and for a harness with no subagents the workflow's `agentsBlock` sentence "a harness with no subagents does the work directly, in its own session" (4.1) |
| subagent-driven-development | the orchestrating skill's dispatch protocol and the implementer template |
| dispatching-parallel-agents, using-git-worktrees | the orchestrating skill's parallel section (`process.parallel-on-disjoint-files`), with worktree removal after integration (HR-203 item 3) |
| test-driven-development | `process.tdd` and the implementer template |
| verification-before-completion | `process.live-run-before-ci`, `process.claims-match-artifacts`, the implementer's closing act, `check-report-claims` |
| requesting-code-review, receiving-code-review | the task-reviewer, branch-reviewer, and spec-reviewer templates; the orchestrating skill's review handling; `process.review-findings-are-claims-too` |
| finishing-a-development-branch | the finishing-a-feature skill |
| using-superpowers | the workflow's `agentsBlock` (4.1) and the reference session-start ritual |
| systematic-debugging | not covered (Q3) |
| writing-skills | not covered: `process.skills-for-procedures` says when a procedure becomes a skill, not how to write one (Q3) |
| diagnosing-superpowers | not covered: it analyses a session transcript, which no kit part does (Q3) |

## 7. The rewrite of shipped texts (meta-spec 6.4, 5.149, 5.152)

- The 36 entries of F3 and the non-entry lines of F3 are rewritten so each names no file or role a minimal install lacks, and a process-pack entry names the installed tracker, not the JSON backlog (5.149). The rule of an entry does not change. The template AGENTS.md lines that point at the workflow move into the workflow's `agentsBlock` (4.1).
- Inside the process pack, `process.no-tech-debt` and `process.bounded-review-loop` also take role-neutral words (the author of the change, the person who decides, the record), and keep their requirement (5.152). The reference-workflow templates keep naming the roles.
- `knowledge-base.controller-rules-are-tagged` moves its `verify` path off the orchestrating skill.
- Every rewritten pack entry bumps its pack's version; `update` moves this repository's pins.
- The classes, spellings, and exclusions are the script's; a new hit is read before it counts.

## 8. Standing entries the owner rules (D10, 5.147)

- `process.rulings-to-file`: HR-172 narrows it (a decision inside the spec's intent goes to the brief or report; only a decision that changes what the spec states amends the spec), and HR-153 adds a clause that pulls the other way (a brief amendment that adds or drops a test, a file, or a gate against the plan is a ruling for the spec's outcomes section). HR-204 item 5 mechanizes the rule as ruled in 177 (a batch-close warn for 'for the owner' ledger lines without a spec entry). One ruling covers all three (Q1). Ruled 2026-10-20 (design.md 179): the narrowing ships, HR-153's clause is refused, and HR-204 item 5 warns only for a decision that changes what the spec states.
- HR-153's other clauses, nine on six standing entries (Q5): `process.claims-match-artifacts` (two: a fix round that re-runs earlier evidence says so, and a line citation into a regenerated file is re-derived; a documentation claim quotes every bearing passage, and a duration is recomputed), `process.closure-claims-carry-enumeration` (a corpus is selected by content; an exception list is a closure with its own script), `process.gates-cover-generated-commits` (a new shipped check is walked through the installed populations and run over the template at each release tag), `writing-style.code-comments` (three: a comment on measured or third-party behavior names an openable source; the controller files the knowledge entry before the dispatch; a doc comment written in a fix round is swept for the round's vocabulary), `writing-style.doc-comments` (a new caller re-opens the callee's doc comment), and `process.tdd` (a RED against a stub of drafted code is a `mutation` entry).
  Ruled 2026-10-20 (design.md 183): the eight approved clauses join their entries' bodies; the controller-filing clause is refused.
- HR-153's clauses for `knowledge-base.state-only-the-source`, which is not standing, ship.

## 9. Riders

### 9.1 Template and code riders

- HR-129: (1) an issue that cites a rule flips that rule's `rule_adherence` row to fail in the same review; (2) `fix_rounds[].gates_run`, an optional field of the deliverables schema; (3) a brief that forbids tree changes names who files deferrals; (4) a fix round records the full gate set, and a shrunken list is a re-review finding.
- HR-153's template defects: the paper walk for a new or tightened gate; the three brief-claim measurements; the branch reviewer's read-only comparison of BASE with the remote default branch and its review of unlisted commits; finishing-a-feature fetches and rebuilds on a moved main; a branch rebuilt on a moved main and a loaded template older than HEAD. Its two defects that assume severity by rule class (the fix-round default for a prose-only round that closes a finding critical by rule class; a finding whose severity comes from the rule class states the size of the repair) close as superseded: `process.severity-by-impact` says a rule's class never sets a severity, and the task-reviewer template and the orchestrating skill say the same. HR-153's record states this. Its long-output report question goes to HR-171 in batch 36, which delivers evidence by reference.
- HR-165: (1) README's sample `stats` output is a frozen slice, asserted equal; (2) rustdoc `missing_docs` for the crate's public items (this also delivers HR-204 item 9); (3) `check-report-claims --doc <path>` reads a spec by path, and `--item <id>` reads a backlog item through the contract: a workspace path the text cites must exist (`<doc>: cites <path>, which does not exist`, exit 1); the second half, a cited `stats` run names the file that holds its window, is deferred to HR-205, because its detection rule must run over the corpus before a plan fixes it (`houserules.plan-semantics-run-on-the-corpus`); with no tracker, `--item` prints line A and exits 2; (4) to (7) the implementer, task-reviewer, and finishing-a-feature texts of its body. Point 6's shipped wording is "run `houserules check-commit --from <merge base>` before the merge, even when the commit-msg hook skipped it"; the clause about this repository's tree binary stays in this repository's local entry.
- HR-166: the until-clause pattern in both copies with the new phrase; the two texts; the CLAUDE.md line of the orchestrating skill; its fourth point (a closed batch's report) is noted in this batch's record, and no closed deliverable is edited.
- HR-150: the closing `check-report-claims` entry has no redirect, or a capture no earlier entry names; after a fix to the report, the citation step runs again, then the closing run.

### 9.2 Templates

The task-reviewer and implementer templates change, so one eval run covers the batch, before the branch review.

### 9.3 Folded technical debt (HR-194, applied to this batch)

Enumeration: `open-unscheduled.txt` (`houserules list --open`, batch null). No item carries the deferral marker yet (it ships here), so the controller read each item for a deferral origin and file overlap:

- Folded: HR-203 item 3 (the orchestrating skill removes an implementer's worktree and branch after integration) and item 4 (an upgrade acceptance case lists the adopter-side reds a kit change causes; applied in section 16).
- Delivered by a rider: HR-204 item 9 (rustc `missing_docs`) by HR-165 point 2.
- Left out: HR-203 items 1, 2, 5 (`audit --workspace`, `stats`, the vacuous-exception gate: batch-35 files); HR-202 and HR-204's other items (checks on the runner of batch 35; HR-204 item 6, a tdd row's RED shows a failure, must accept the critical-task evidence of section 5, and its item 5 follows Q1); HR-149 and HR-157 (gates, no overlap); HR-088 (after 2.0.0, Q1 of 5.122); HR-123, HR-124, HR-128 (`check-report-claims` classes: batch 36); HR-130 (a `check-knowledge` lint for a loading path: a check, batch 35); HR-055, HR-058, HR-121, HR-126, HR-127, HR-155, HR-200, HR-201 (no overlap).
- This repository marks its open deferrals with `defer --item` in this batch (section 10), so that 6.2 runs on real data from batch 35 on.

## 10. This repository (meta-spec section 8)

- `init --tracker json` writes `tracker: {"adapter": "json"}` and the `backlogId` pattern (its `backlog/` files are kept). `update` with the tree binary moves the pack pins, adopts the reference workflow (its workflow files hold path baselines, so step 2 pins them at those), splits the session-start script, and delivers the two schemas (the deliverables schema is the batch-33 content, a known content, F15); a second `update` prints no change (5.148).
- `process.full-tier-measures-cost` retires with a `see` link to the new `process.batches-measure-cost` (section 0).
- The JSON tracker stays through the release (5.132(4)).
- The open items whose bodies name a review cap, a deferral, or a stated reason for later get the marker through `defer --item`; the plan enumerates them by body text and lists each.
- The every-commit `Work-Item:` entry (5.138): the pack version that ships it reaches this repository with the exclusion of 3.7. At Finish, the aggregation writes a `Work-Item:` trailer for every item each aggregated commit delivers, and then the batch-close commit deletes the exclusion. So no commit made before the opt-in meets the gate, and every later commit does. The held release PR's commit passes by its subject (3.7, F12). The branch audit before Finish books no red for it.

## 11. Documents

`docs/upgrading-to-2.md` gains the batch-34 section: trackers and `--tracker`, the trailers and the opt-in, the workflow package and its pins, the minimal default, the moved files and their classes, the `model` field, the two tiers and the dropped `full-tier` tag, each new hand step (the hand-step test of batch 33 covers them), and the reds an upgrade causes by design (section 16, case 14). The migrating-knowledge skill gains one section per adapter for moving `backlog/` data (a guide, not a command, 5.138), each adapter's setup (5.103), the advice of 6.3, and the plugin skills the kit leaves uncovered (Q3). README covers `--tracker`, `--workflow`, `defer`, and the minimal default; its ownership table and agent-layer bullet follow `files` (`houserules.readme-mirrors-kit-owned`).

## 12. Questions for the owner

One at a time; each ruling goes to design.md and this spec in the same turn.

- **Q1. `process.rulings-to-file`** (HR-172, HR-153's clause, HR-204 item 5). Ruled 2026-10-20 (design.md 179): "Narrow, refuse clause". Your ruling 177(5) said no rule is narrowed; that ruled the stats proposals, while 5.147(2) schedules HR-172's narrowing for this spec review, so it is open. Recommended: take HR-172's narrowing and refuse HR-153's clause; HR-204 item 5 then warns only for a decision that changes what the spec states. A brief that adds or drops a test, a file, or a gate inside the spec's intent is a decision for the brief and the report, where the branch review reads it.
- **Q2. The live proof of the GitHub adapter.** Ruled 2026-10-20 (design.md 180): the controller creates private scratch repositories as the live run needs and reminds the owner to delete each one. CI proves it against a fake `gh`. The live run needs a GitHub repository it may write issues, labels, and milestones to. Recommended: a scratch repository you create (for example `houserules-tracker-sandbox`) that the live run fills and empties; the alternative is the fake only.
- **Q3. Three plugin skills the kit does not cover** Ruled 2026-10-20 (design.md 181): leave all three uncovered. (6.4): systematic-debugging, writing-skills, diagnosing-superpowers. Recommended: leave all three uncovered and say so in the migrating-knowledge skill; none is a lifecycle phase, and a project that wants them keeps the plugin, which nothing in the kit asks for or breaks.
- **Q4. Issue 38's design-phase rules.** Ruled 2026-10-20 (design.md 182): ship all four. Its part 3: a rejecting decision carries a probe, the rejected option's best case, and a reversal condition; each claim is labelled run, read, or assumed; a spec lists the concepts it takes from its sources; an overturned decision is a `Reversal:` ledger line. Recommended: ship them as non-standing process-pack entries that the spec-reviewer template cites (they join the required ids).
- **Q5. HR-153's other standing clauses** (section 8): nine clauses on six standing entries, each yours to accept or refuse. Ruled 2026-10-20 (design.md 183): eight approved; the code-comments clause "the controller files the knowledge entry before the dispatch" refused.
- **Q6. Issue 61's points 1, 2, and 5.** Ruled 2026-10-20 (design.md 184): core step, no recipes (section 4.4a). Issue 61 asks for one workflow-agnostic step (for, implement, audit, write the report) as a small skill and a hook any workflow can call, and recipes for Superpowers and BMAD; its acceptance says a Superpowers project runs the step with no other houserules files. No ruling disposes of them, and a Superpowers recipe collides with 5.161 (the kit names no superpowers skill). Recommended: ship the step as a core skill `houserules-step` (every install gets it, as the migrating-knowledge skill) and a core hook script, and drop the recipes: the step is the integration point any workflow calls, and naming other workflows rots with their releases.
- **Q7. HR-194's convenience criterion with two tiers.** Ruled 2026-10-20 (design.md 185): doesn't make a task critical. 5.128's "does not raise the batch's tier" no longer separates candidates. Recommended: read it as "does not make a non-critical task critical" (173(1)), so a candidate never adds the critical-task cost to a task that had none.
- **Q8. How release-please's commits pass the every-commit entry.** Ruled 2026-10-20 (design.md 186): also exempt by subject. 5.138(3) exempts them by bot author, but a release-please run with a PAT, as here, authors them as the PAT's owner (F12). Recommended: exempt them by subject as well (`exempt_subjects`, default `^chore\(.+\): release `), keeping the author list for dependabot.

## 13. Controller calls

- C1. The contract is a Rust trait; the commands call it; `defer` (two forms, with `--section`) and `list --deferred` are its new CLI surface; `get` keeps the 1.4.0 kinds (3.1).
- C2. The `tracker` key's shape, `--repo`, the two `init --tracker` refusals, and that every command setting `tracker` writes the `backlogId` pattern and the CI workflow (3.3).
- C3. The GitHub adapter runs `gh`, and the Beads adapter runs `bd`: 5.136 forbids houserules taking a token, which a Rust GitHub client needs, and Beads has no other interface (3.5).
- C4. The mapping table of 3.5, the houserules block, the item set of a hosted tracker, creation on write, and the archive behavior of the two hosted adapters.
- C5. The suite: a real `bd` pinned by mise on every CI system, a fake `gh`, and equivalence as parsed JSON (3.2, 3.6).
- C6. Every exact line of sections 3 and 4.
- C7. The opt-in mechanism: `optIn`, the one-time exclusion, `optInOffered`, and the `pack remove` and re-add behavior (3.7); the `commits` fields `require_trailer`, `exempt_authors` with `github-actions[bot]` added, and `exempt_subjects` (design.md 186).
- C8. The workflow package's layout, manifest, and `agentsBlock` (4.1), versioned with the binary (5.140(1)); `workflow` with `models` in `.houserules.json`; no `model:` frontmatter (F14); the trailer on every commit, never citing an `optIn` id.
- C9. `--add-required-packs` (4.3).
- C10. The designing and planning skills as new workflow files (6.4).
- C11. The `models` key of `stats`, its `pre-2.0` row, and the fields of the `spec-review` kind (4.5).
- C12. `backlogId` accepts the installed adapter's shapes, and any token with no tracker (3.9).
- C13. HR-153's long-output question goes to HR-171 in batch 36; its two rule-class defects close as superseded by `process.severity-by-impact` (9.1).
- C14. HR-166's fourth point closes by a note in this batch's record (9.1).
- C15. The file classes of 4.2.
- C16. The workflow writes its files whatever the render targets; only its settings hook entry needs the Claude renderer (4.3).
- C17. The provenance rule of 4.7: known contents (released and batch-33) and baselines decide what is the kit's; step 1 only inside the 1.x conversion.
- C18. The seeded CI per adapter (4.8).
- C19. The `full-tier` tag is dropped with an upgrade-guide line; `process.full-tier-measures-cost` is replaced by `process.batches-measure-cost`, this repository's rule only (0, 5).
- C20. `process.brainstorm-first` keeps its summary; only its first body line changes (6.4).
- C21. This repository opts in to the every-commit entry at Finish, after aggregation, and adopts the JSON tracker by `init --tracker json` (10).
- C22. GitHub ids accept the bare number on input; pull requests and other repositories never resolve (3.5).
- C23. The read-direction mappings and the rejected native means (3.5).
- C24. The `Deferred:` verdict and the id-shape gate before any backend call (3.7).
- C25. The four stated departures: 5.149(1) kept (the audit line at n = 0 too), 5.149(2) (no tracker for a 1.x install with no data), 5.144(3) (the CI file per adapter), 5.155(2) (the light criterion) (3.3, 3.4, 4.8, 5).
- C26. A pinless workflow path belongs to the project (4.3).
- C27. HR-165(3)'s stats-window half deferred to HR-205.
- C28. `stats` reads a spec review only from `spec-review.json` and `spec-review-r<n>.json`: a probe capture named `<review stem>-probe-<slug>.json` is evidence, not a review (4.5; found by Task 5 over this batch's workspace).
- C29. Batch membership on hosted adapters follows the `batch:<n>` label; JSON `batch <n>` is unchanged (3.5; found by the Task 2 review).
- C30. The record layout the adapters read, the parked group issue, `set`/`defer --item` on items only, and `milestone` in the contract's `set` (3.5; found by the Task 2 review).
- C31. `resolve` returns existence and the deferral's reason only: no gate reads status, title, origin, or files (3.1; found by Task 4).
- C32. The offer line, the message-file form of the no-trailer line, and parsing under git's default trailer separator (3.7; found by the Task 4 review).
- C33. A workflow path that holds the project's own file maps to `null` in `workflow.files` once its line is printed, so a path that a new binary's manifest adds is reported once (4.3; found by the Task 6 review).
- C34. The step script keeps its output directory out of `git status` with its own `.gitignore` (4.4a; found by the Task 6 review).
- C35. A `kitOwned` file whose bytes equal a known kit content is the kit's: `init` and `update` replace it (4.7; found by the Task 6 review, a fresh minimal install over a 1.4.0 skill).
- C36. The cause of another `bd` failure skips bd's leading warning block (3.4; found by Task 3 against bd 1.3.1).
- Ruling 188 (owner, design.md 188): houserules sets `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` on every `bd` it starts; the Beads adapter docs say so (3.5; found by the Task 3 review).
- Ruling 189 (owner, design.md 189): `process.skills-for-procedures` drops "Author it with the writing-skills skill when available"; live case 20's guard matches every plugin skill name of the 6.4 table (6.4; found by the Task 9 review).
- C39. Step 3 writes the reference script and its hook entry in the changed branch too (4.7; found by the Task 7 review).
- C40. The core session-start hook is the compaction entry alone; `update` removes the seeded startup entry where it equals the kit's (4.4; found by the Task 7 review).
- C41. A kit pack's version moves once per change within a release cycle, by the class the `KIT_PACK_VERSIONS` doc names (a stricter check is a major) (found by the Task 9 re-review).
- C42. The known contents of `crates/houserules/released/` are outside 6.4's grep (6.4; found by the Task 8 review).
- C43. Live case 12's third line (provided by no carried pack) cannot appear with the shipped manifest, because a kit pack ships every required id. The unit test `an_id_that_no_pack_of_the_kit_ships_asks_for_the_local_pack` covers it with a synthetic id; the live case asserts the other two lines (16; found by the live run).
- C44. Live case 24's GitHub run passes only after the 2.0.0 release: the seeded workflow installs the release that `version` in `.houserules.json` names, and v1.4.0 does not read a 2.0 install. The run moves to the 2.0.0 rollout acceptance, as HR-217 in batch 38 (4.8, 16; found by the live run).
- C45. `--sanctioned` marks a fail row and keeps the audit's exit 1. Only a recorded eval run clears the `process.evals-rerun` red of an update commit; the upgrade guide says so (case 14; found by the live run).
- C46. With C40, step 3's changed branch leaves one session-start entry, so the ritual prints once (4.7; found by the live run).
- C47. The lint task's windows-gnu step runs clippy with `-D warnings`, as the `rust` job does on windows-latest; a compile check let a dead-code warning pass (found by the live run).
- C48. A deliverable's `model` holds the configured value, an alias or an id, and `stats` keys its `models` rows by family, so one model has one row whatever the spelling (4.5; found by the branch review, issue 2; the template wording waits for HR-215).
- C38. `update` with an unreachable `bd` keeps the pattern, names the cause with the unreachable line, and finishes (3.3; found while briefing Task 7).
- C37. `init --tracker` refuses a failed precondition before any write on every adapter; the `backlogId` pattern is written only from a prefix read in the same command (3.3, 3.9; found by the Task 3 review).

## 14. Meta-spec rows

HR-180, HR-181, HR-194, HR-144, HR-129, HR-153, HR-165, HR-166, HR-150, HR-196, HR-172, HR-198, HR-199.

## 15. Functions kept

Each meta-spec 6.2 row whose "Touched by" names a batch-34 item, and how this batch keeps it:

- Knowledge reads: `get` routing never needs the tracker for a knowledge id (3.8).
- `check-knowledge`: gains the workflow requirement lines (4.3) and the tracker lines of 3.10; the kit-citation lint covers installed workflow files.
- `render`: the AGENTS.md block gains the adapter's session commands (3.10) and the workflow's `agentsBlock` (4.1); the `project-knowledge` skill loses its dispatch steps (4.4).
- `audit`: resolves trailers and deliverable deferrals through the contract, with the no-tracker line (3.4, 3.7).
- `check-commit` and the commit-msg hook: the every-commit entry's fields (3.7); the hook in the workflow package, `check-commit` core (4.1, 4.4).
- `validate`: requires `model`, accepts `spec-review` (4.5), and the adapter's `backlogId` (3.9).
- `check-report-claims`: gains `--doc` and `--item` (9.1, HR-165).
- `stats`: the `models` key and spec reviews (4.5).
- Backlog reads (`list`, `batch`, `get`): through the contract on every adapter, equal by 3.2; the lines of 3.4.
- Backlog writes (`set`): through the contract; creation on write on hosted adapters (3.5).
- `check-backlog`: contract checks on every adapter, the JSON file checks on JSON (3.1, 3.5); 3.4 with no tracker.
- Deferral tracking: `defer`, `list --deferred`, the `Deferred:` trailer, and the deliverable gate (3.1, 3.7); the process pack needs a tracker (3.10).
- Batch records, amendments, decisions, parked polish: records of the contract, mapped per adapter (3.5).
- `archive`: the knowledge half core; the backlog half through the contract (3.4, 3.5).
- `init`: the minimal default, `--tracker`, `--workflow`, `--add-required-packs` (3.3, 4.3, 4.6).
- `update`: the workflow pins and the conversion steps (4.3, 4.7).
- `files`: the classes of 4.2.
- The `idPrefix` pattern of `backlogId`: per adapter (3.9).
- Agent templates and skills: the workflow package (4.1), the riders (9), the tiers (5), the design review (6.1).
- The migrating-knowledge skill: core, with the adapter sections (11).
- Evals and `process.evals-rerun`: in the workflow package; `record.json` project data (4.1, 4.2).
- The session-start hook: split (4.4, 4.7 step 3).
- AGENTS.md and CLAUDE.md: seed-once; the block as in `render` above.
- The seeded CI workflow: per adapter (4.8).
- The seeded entries: the rewrite of section 7, the owner's standing rulings (section 8).
- `update --id-prefix`: kept for the JSON adapter (3.9).
- The `.houserules.json` keys: `tracker`, `workflow`, `optInOffered` added; the rest as batch 33 left them.
- `update` keeps a modified file: for the workflow files (4.3, 4.7 step 2).
- Backlog records: mapped per adapter (3.5).
- `get` routing: 3.8.
- The deliverables schema's kinds: `spec-review` added, `model` required, `fix_rounds[].gates_run` (4.5, 9.1); delivered by `update` (4.7 step 4).
- The kit-owned files: the classes of 4.2.
- The seed-once backlog, report, and eval files: `trackerFiles`, `d3Files`, `workflowFiles`, `workflowData` (4.2).
- `docs/README.md`: seed-once; README's ownership table follows `files` (11).

The plan re-derives this list with the batch-33 script `f5-inventory-coverage.py`.

## 16. Acceptance and the live run

Each case runs with the tree binary in a scratch repository, its capture in the workspace (`live-<NN>-<slug>.txt`). Each names what its capture must show.

1. A minimal `init`: exactly the files of 4.6 as written there; every gate exits 0; `check-backlog` prints line A; `update` twice writes nothing under `backlog/` and no eval file.
2. The table of 3.4: each cell's line and exit, with and without a `backlog/` directory, and the `get` pack-line combination; `get process.nonexistent` with no tracker prints the knowledge base's not-found line, not line A; `audit` with no trailers prints `skipped 0 ...`.
3. `init --tracker json`: the `trackerFiles`; `list --open`, `batch`, `get`, `set`, `check-backlog` print what 1.4.0 prints on the same data; `validate` accepts a report naming a JSON id; `init --tracker github` on that install refuses with the line of 3.3.
4. `defer --title ... --file a --file b` prints an id; `list --deferred` shows its row with the `deferral` object; `defer --item <id>` marks an existing item; a 121-character title is refused; `check-backlog` passes.
5. `audit` over a range with `Work-Item:` and `Deferred:` trailers, one aggregated commit with two `Work-Item:` trailers, and a review whose `issues[].backlog` names an unfiled id: each line of 3.7 for the bad ones, a `Deferred:` trailer with no reason, and a `Work-Item: --help` trailer that fails resolution with no backend call recorded, exit 1; with the bad ones fixed, exit 0.
6. The every-commit entry: a new install with the process pack has it excluded (`optInOffered` holds it); with the exclusion deleted, `check-commit` fails a human `feat` commit with no trailer (the line of 3.7), passes a `dependabot[bot]` commit, and passes a `chore(main): release 2.0.0` commit authored by a human (design.md 186); a batch-33 install through `update` gets the exclusion; a second `update` after the project deleted it writes no exclusion; `pack remove process` then `pack add process` prints the active line.
7. `get` routing: a loaded topic id, a backlog id, an id in both (the line of 3.8), an id of a pack not installed, with and without a tracker; `get process.tdd` with an unreachable fake `gh` prints the entry.
8. The Beads adapter with the pinned `bd`: `list`, `batch`, `get`, `set`, `defer`, `check-backlog` on seeded data equal the JSON outputs by 3.2; an item of type `decision` and a decision record stay apart; an `in_progress` issue reads as open; the adapter check names the `bd config` step on a store that lacks the custom status `partial`; `init --tracker beads` with `bd` missing, and with no store, prints the refusal of 3.3.
9. HR-180's acceptance with the GitHub adapter (the repository of Q2, or the fake, as ruled): a repository with no `backlog/` passes `audit`; an unresolved id fails `audit` with the id and the adapter in the line; `get 12` and `get '#12'` both resolve; an issue with no houserules label resolves as a trailer target; a trailer naming a pull request fails with its line; `set <id> milestone=<new>` creates the milestone; a closed issue with no close reason reads as done.
10. An unreachable tracker: each cause of 3.4 per adapter prints its line byte for byte, and exits 1 from `check-backlog` and 2 from `list`.
11. `init --workflow reference` without a tracker refuses (the line of 4.3); with `--tracker json` it writes the workflow files, an empty `record.json`, and the `agentsBlock` in AGENTS.md; with `--add-required-packs` every gate exits 0, `check-knowledge` included while the process pack's `optIn` exclusion is in place; `init --tracker json` then `init --workflow reference --tracker json` on a JSON install both exit 0; without `--target claude` no `.claude/settings.json` hook entry; `init --target claude` later adds it.
12. Each unmet-requirement line of 4.3: not provided, excluded, and provided by no carried pack (the third by its unit test, C43).
13. A locally edited workflow file through `update` with a binary whose version, and so its workflow's, is higher (built from a scratch clone whose crate version is bumped): the file unchanged and the kept line printed; a second `update` prints the kept line again and changes nothing.
14. A 1.4.0 install (seeded by the v1.4.0 binary) through `update`: the JSON tracker, the adopted workflow (unmodified files pinned at their baselines, one modified file kept), the split session-start script, both schemas delivered, the hand steps; a second `update` changes nothing. The reds an upgrade causes by design, each with its remedy, asserted: the adopter's `audit` of the update commit fails `process.evals-rerun` (remedy: run the evals; `--sanctioned process.evals-rerun=<ref>` marks the row and keeps the exit, C45); an excluded required id fails `check-knowledge` (remedy: the line of 4.3). The every-commit entry causes none: it arrives excluded.
15. Populations through the steps of 4.7, each step's line or silence asserted: a batch-33 plain install (skills and hook present, no agent template); a batch-33 install with `--target claude` (the reference session-start hook entry added to `.claude/settings.json`); one with a modified session-start script; one with the script and `.claude/settings.json` in `ownedPaths`; a 1.x install with the process pack and the workflow and no `backlog/` (the tracker hand step, then the `check-knowledge` lines of 3.10); a 2.0 install with a `backlog/` directory and no `tracker` key (line B, no tracker written).
16. A minimal 2.0 install with a project-own `.githooks/commit-msg` and `.claude/agents/implementer.md`: `update` twice adopts nothing, writes nothing, and prints no kept line. Then `init --workflow reference --tracker json` keeps both files, prints the step-2 line once, and `update` twice prints nothing for them.
17. `validate` requires `model`; `stats` prints the `models` key with a `pre-2.0` row for older deliverables; a `spec-review` deliverable with the fields of 4.5, saved as `spec-review.json`, validates, and `stats` counts its findings under `critical`, `important`, and `minor`, with `other` at 0.
18. `pack add process` with no tracker refuses (the line of 3.10).
19. This repository: `init --tracker json`, `update --dir .`, and every lint step of `mise.toml`, the windows-gnu check included; both schemas delivered; a second `update` changes nothing.
20. The kit names no superpowers skill: the case-insensitive grep of 6.4 prints nothing.
21. The kit names no full tier: `fact-full-tier.py`, extended to take the root copies of template files as input (`.claude/skills`, `.claude/agents`, `knowledge/packs`) and to skip this repository's local and archived entries by id, prints `hits: 0` over `template/` and over those copies.
22. HR-199's acceptance: in a scratch install of the reference workflow with no superpowers plugin, one small item runs every lifecycle phase (design with its review, plan, one dispatched task with its review, live run, finish); the capture lists each phase and its artifact. With the plugin installed, nothing in the kit names it (case 20).
23. A workflow install's `.houserules.json` `workflow.models` changed for the implementer: the orchestrating skill's dispatch names the changed model (the capture shows the dispatch line), and no template carries a `model:` line.
24. The seeded CI per adapter (4.8): for each of JSON, GitHub, and Beads, `init --tracker <a>` writes the workflow of 4.8, and each file passes `actionlint`; a GitHub run of the workflow on the repository of Q2 passes `check-backlog` and `audit` (at the 2.0.0 rollout, C44) (with Q2 ruled as the fake only, `actionlint` is the check); the Beads steps run in a fresh clone of a scratch repository whose `refs/dolt/data` holds seeded data, and `check-backlog` passes there.

25. Issue 61's acceptance: a minimal install (no tracker, no workflow) runs the step: `tools/houserules-step.sh before <file>` writes the rule package, a change is made, `tools/houserules-step.sh after <base> <report>` audits and validates; no file outside the core set is needed. `grep -ri superpowers` over the step's two files prints nothing.

The eval run covers the changed templates: dependency-add, docs-edit, and seeded-violations; their expectations include the `Work-Item:` trailer where the entry is active (4.1).

## 17. Out of scope

The per-PR review tiers of HR-186 (batch 37). The runner port and new checks (batch 35, HR-202, HR-204 except item 9). Evidence by reference (batch 36, HR-171). A data-moving command between trackers (5.138: a guide only).
