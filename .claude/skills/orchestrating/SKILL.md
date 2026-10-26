---
name: orchestrating
description: Use when starting a session in this repository, starting or resuming a batch, or dispatching any subagent
---

# Orchestrating a batch

You are the controller. Subagents get knowledge through their templates; you get it through this skill, the standing rules, and `houserules`. Every read command prints JSON. Write each output to a file in the batch workspace, and read the file with your file-read tool (`houserules list --open > <workspace>/open.json`). Treat a `<<` marker or a "more items" note as a lost read and read the source file.

The batch workspace is the folder `<YYYY-MM-DD>-batch-<n>` of the `workspaces` location (default `.houserules/work/`). A project sets another location with the `workspaces` key of `.houserules.json`. The folder holds the deliverables of the batch and its ledger, `progress.md`. Git ignores the default location; `houserules check-knowledge` fails an entry whose `verify` path names a file in any ignored location, the default included: commit a copy under a tracked path and verify that.

## Session ritual (session start, resume, and after compaction)

1. `git status --short && git log --oneline -15`
2. Run the start command of the Session section in the houserules block of `AGENTS.md`. Then run `houserules list --batch <n>` for the in-progress batch, or `houserules list --open` when none is in progress. At the end of a session, run the end command of the same section.
3. If a plan is in flight: read the ledger `progress.md` in the batch workspace. Trust the ledger and `git log` over memory.
4. `houserules index --area process`; `houserules get` what the next step needs. The process index lists every process rule, standing or not: it is how a non-standing process rule reaches you (`knowledge-base.rules-need-a-loading-path`).
5. Re-read the spec and plan in flight before the next dispatch.

## Batch lifecycle

| Phase | Skill | Repo gate |
|---|---|---|
| Select items | — | `houserules list --open`; record the batch (the paragraph below the table gives its record) and schedule its items (`houserules set <id> batch=<n>`) |
| Design | `designing` | spec in `docs/specs/`; design review by the `spec-reviewer` (standard tier); approval from the project's owner or decider |
| Plan | `planning` | plan in `docs/plans/`; code-health scan of the touched files (`process.code-health-scan`); technical-debt candidates; approval when asked |
| Build | — | the dispatch protocol below; close the phase's cost window when the phase ends, where the project keeps one (Batch close) |
| Verify | — | live run before any pull or merge request or deploy spend (`process.live-run-before-ci`); a printed command or line that changes after the live run runs again before the branch review; close the phase's cost window when the phase ends, where the project keeps one (Batch close) |
| Finish | `finishing-a-feature` | backlog ticked, one to five clean commits, the evidence bundle (`houserules bundle write`) in the description of the pull or merge request, checks, merged per the project's own discipline |
| Rollout, acceptance | — | acceptance from the project's owner or decider; the acceptance record and new rulings ride the next branch |

The batch record keeps the batch's `number`, `summary`, `kickoff`, and `status`. The tracker decides where it lives:

- JSON: an entry of `backlog/batches.json`.
- GitHub: an issue labelled `houserules:batch` and titled `Batch <n>: <summary>`. A fenced `houserules` block at the end of its body holds `kickoff` and `status`, for example `status: {"state":"in-progress","text":""}`. The issue is open while the batch is planned or in progress, and closed when it is done.
- Beads: an issue of the custom type `houserules-batch`, with the same title and block.

No `houserules` command writes a GitHub or Beads batch record: create and edit the issue with `gh` or `bd`. `houserules set <id> batch=<n>` schedules an item on every tracker.

## Tiers

The change's risk picks the tier at plan time (`process.risk-tiers`). You pick between two tiers, light and standard. Every tier runs Select items, Verify (the live run), Finish, and Rollout and acceptance. A lighter tier changes only what its pipeline column names. The re-review cap per task differs by tier.

| Tier | Selected when | Pipeline |
|---|---|---|
| light | one task; docs, comments, config values, or a dependency bump; no executable behavior change; no task is critical | the backlog item's body is the spec (the project's owner or decider still approves it); implementer; one task review; controller live run. No design review, no plan document, no branch review. |
| standard | every change that is not light | short spec with a design review, and approval from the project's owner or decider; plan; implementer; one task review and at most one re-review; controller live run; branch review only for a batch of two or more tasks |

A critical task is a task whose change alters the CLI's commands, flags, or output, a schema, authentication or security code, or the data on disk of an install. It runs inside the standard tier with its own evidence:

- The plan writes the spec's cases for the task as tests before any code.
- The implementer commits those tests first, with one recorded run that shows them failing at `BASE`, then makes them pass. It still works test-first for every further change. The evidence is that run and the whole suite passing at `HEAD`.
- The task gets one task review on the task reviewer's configured model (default opus). It gets no re-review of its original findings.
- You verify each fix by re-running the reviewer's own reproduction, and you record the check in the ledger (`process.fix-round-verification-record`). A Critical or Important finding goes to a fresh implementer. You fix a Minor finding yourself.
- A fix round that fixes a Critical or Important finding, or that changes files beyond the files its findings name, then gets one review of its fix diff. That review files only new breakage. Only a new Critical or Important finding of it opens a further round, which closes by your check, with no second fix-diff review (`process.risk-tiers`).

The tier rules:

- The plan records the tier and the criterion that selected it. A light change has no plan: the dispatch brief records both.
- One tier covers the batch: the highest tier that any of its changes needs. A critical task follows its own evidence rule inside that tier.
- A tier moves up during a batch, never down. A light task with an Important or Critical finding moves up to standard (`process.bounded-review-loop`).
- The task reviewer checks the tier. A wrong tier is a finding.

## Parallel and sequential dispatch

Dispatch agents in parallel only when the plan marks their tasks parallel (`process.parallel-on-disjoint-files`). Dispatch every other agent sequentially: never start a second one before the last one closes.

- Mark tasks parallel in the plan when their declared file sets are disjoint and no task needs another's output. You own the files that many tasks regenerate: the generated rule files and skill (`houserules render`), `backlog/`, lockfiles, and any other file that a build or sync step regenerates. A parallel task does not touch them. You regenerate them at integration.
- Give each parallel implementer its own git worktree, on its own branch from the same `BASE`.
- Open the brief of a dispatch into a worktree with a first act. The implementer runs `git merge-base --is-ancestor <BASE> HEAD` in its worktree. When the command exits 0, the worktree holds `<BASE>`, and the implementer goes on. When the command exits 1, the implementer runs `git reset --hard <BASE>` and records the reset in `concerns`. When the command exits with any other status, the implementer stops and reports BLOCKED with the output. In a fix round, `<BASE>` is `FIX_BASE`. Leave the first act out of a dispatch into a shared checkout, where a reset discards work.
- Name each parallel agent's scratch root in its dispatch, `<scratchpad>/task-<N>/`, and tell it to write scratch files nowhere else. Every agent of one session gets the same scratchpad path, and two agents that write one file name there overwrite each other without an error.
- Integrate a finished task onto the batch branch one at a time (cherry-pick). Run the gates after each. Write the old-to-new commit map to the ledger. A conflict or a red gate at integration is a fix round for the later task.
- A gate command that the harness refuses to an agent is a controller-run integration gate: the plan lists it, and you run it after each integration and record it in the ledger.
- Run each controller-run integration gate through `houserules capture`, and put any injected configuration in the argv, so that the capture shows it: `houserules capture --out <workspace>/<name>.txt -- env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=true <gate command>`.
- After you integrate a task, remove its worktree and its branch: `git worktree remove <path>`, then `git branch -D <branch>` (the cherry-pick leaves the branch unmerged, so `-d` refuses). A worktree that stays makes your harness load its copies of the skills and templates as duplicates.
- Reviews of different tasks may run at the same time: reviewers do not write to the tree. The review of a task starts after its implementer closes.
- Run at most three agents at once unless the plan sets a lower limit. The ledger records each dispatch's start and close.

A project may rule its own parallelism discipline (elicited by the `migrating-knowledge` skill's "Elicit your own operating discipline" step, recorded as its own knowledge entry); that ruling replaces this default. A line from a kit before 1.3.0 that says to hand off or dispatch tasks sequentially by default states the old kit default, not a project ruling. `houserules update` never rewrites such a line, so remove the sentence by hand. It sits in `AGENTS.md` outside the houserules block, or, in an install seeded by the 1.0.0 kit, in `CLAUDE.md`. While the workflow is installed, the houserules block of `AGENTS.md` carries the current rule: `run handoffs in parallel only when the plan marks their file sets disjoint, each in its own worktree`. The 1.0.0 line in `CLAUDE.md` also carries the dispatch-template and the `Knowledge:` and `BASE:` instructions: delete only its sentence `Dispatch agent work sequentially by default, until the project rules its own parallelism discipline.`, or move your own text into `AGENTS.md` and leave `CLAUDE.md` as the one line `@AGENTS.md`.

## Dispatch protocol

- Templates: `implementer`, `task-reviewer`, `branch-reviewer`, and `spec-reviewer`; `houserules init --workflow reference` installs them. Each role's model is a configuration value: `workflow.models` in `.houserules.json` holds it, and the defaults are sonnet for the implementer and opus for the other three roles. A template carries no `model:` line, so a dispatch that names no model runs on your own session's model. Read the role's value, pass it to the dispatch, and check that the dispatch names one before you send it. Reviews run on a mightier model than the implementer they review by default (`process.model-policy` is a recommendation the project may override).
- Every dispatch carries a `Model: <alias>` line, an implementer dispatch included. A report from a dispatch with no `Model:` line records `unknown` as its model.
- Derive a dispatch's commit span with `git rev-list --count BASE..HEAD`; never hand-type a count into a dispatch or its ledger row.
- Paste every sha of a dispatch from the output of git rev-parse in the same turn; never retype one.
- An implementer dispatch is the task brief plus these lines:
  - `BASE: <sha>` — the commit before the task.
  - `Backlog: <ids the task delivers>` — the implementer writes one `Work-Item: <id>` trailer per id on every commit.
  - `Tier: <light|standard> (<criterion>)` — the tier and the criterion that selected it (`process.risk-tiers`). A critical task says so in the criterion, for example `Tier: standard (critical task: it changes a schema)`.
  - `Model: <the role's model>` — the model the agent records in the `model` field of its deliverable.
  - `Knowledge: <ids>` — `houserules for <the brief's files>` plus the procedure ids the task needs. Five to ten ids. Leave an entry tagged `controller` off the line unless the task needs it: a task audit keeps it in the package when the line names it (`knowledge-base.controller-rules-are-tagged`).
  - `REPORT_FILE: <workspace>/task-<N>-report.json`
- A reviewer dispatch adds `BASE`, `HEAD`, the same `Backlog:`, `Tier:`, and `Knowledge:` lines, its own `Model:`, `REPORT_FILE`, `REVIEW_FILE: <workspace>/task-<N>-review.json` (re-review: `task-<N>-review-r<R>.json`), `AUDIT_JSON: <workspace>/task-<N>-audit.json` (re-review: `-r<R>`), and the audit command's `--ids <the Knowledge ids>`.
- A spec-review dispatch names `TARGET: <spec path>`, `BASE`, `HEAD`, the batch's `Backlog:` ids, `Tier:`, `Model:`, `Knowledge:` (the four design-phase ids and the ids the spec rests on), and `REVIEW_FILE: <workspace>/spec-review.json`. A fresh review after a Critical finding writes `spec-review-r<R>.json` and names the earlier reviews as `PRIOR:`.
- The audit `--ids` value is the dispatch's `Knowledge:` list, generated from it, never typed separately — true for round 0 and every re-review alike.
- A re-review dispatch carries the identical `Knowledge:`/`Backlog:`/`--ids` block as the round-0 dispatch, and the task's current `Tier:` line. A narrowed block narrows the audit package silently.
- A fix-round dispatch names `FIX_BASE`; the fix-diff audit goes into the report's `fix_rounds` entry, and `self_audit` stays the `BASE..HEAD` audit (`process.deliverables-json`).
- A parallel task's fix round commits in the task's own worktree on its `FIX_BASE`, because a worktree-isolated agent cannot run git against the shared checkout. You cherry-pick that commit, run the gates, and extend the ledger's commit map.
- A fix-round dispatch quotes a finding's premise only with the review's own named verification run beside it; a premise with no such run gets no free pass — the dispatch tells the implementer to measure it first, before acting on it. Relaying an unverified premise as fact produces a correction that can itself be false.
- The branch review dispatch names `WORKSPACE` (every workspace of the batch, when it has several), `BASE` (the merge base), `HEAD`, `Model:`, the plan and spec paths, and `REVIEW_FILE: <workspace>/branch-review.json`, through `branch-reviewer`; its audit runs `--workspace <WORKSPACE>` in place of `--report`, once per workspace, each run writing `<that workspace>/branch-audit.json`.
- A brief names no version number for a tool, action, or package (`security-hygiene.exact-pins`); it names the verification the implementer runs and records in `docs_verified`, and shows placeholders such as `jdx/mise-action@<current major>`.
- A brief names every test, gate, and file its spec task lists, verbatim or by pointer (`process.brief-carries-the-spec`); an implementer who cannot satisfy one flags it instead of dropping it.
- A brief that forbids tree changes names who files its deferrals: you, by default. The implementer then lists each deferral in `concerns` and runs no `houserules defer`.
- A brief for a task that adds or tightens a check in `check-knowledge`, `check-backlog`, or `audit` carries a paper walk: a fresh install, an install on each released kit under the new binary, an install with kept or overridden kit-owned files, and each author class of commits. The report states each accepted red.
- A brief's claim about a file's current contents is measured at brief-writing time — grepped or read, never restated from memory or an earlier document; an unmeasured claim about a knowledge copy or a file's shape is exactly the kind of thing that can be flatly wrong, and the implementer pays the disproof. A brief that dictates a message reads it once for each state that prints it. A brief that dictates an enumeration derives it from the subject, never from a file name. A brief that dictates a mechanism that a reviewer proposed has run it against the reviewer's own counter-example first.
- When a batch edits an agent template or skill, the dispatch message carries the changed instruction verbatim: templates load at session start, so the running session's copy is stale until a restart.
- A controller commit made while a task is open (a concurrent backlog filing, a rulings-to-file write) touches only files disjoint from the open task's own files; record it in the ledger before the next reviewer dispatch, and name it in that dispatch.

## Handling reviews

- A review that fails `houserules validate` (an `open` row included) or is not at `REVIEW_FILE` with its `AUDIT_JSON` is incomplete: re-dispatch it. `houserules stats` reads only files at those names: every `task-*-audit*.json`, `task-*-review*.json`, `task-*-report.json`, `branch-review.json`, `spec-review.json`, and `spec-review-r<n>.json` in a workspace.
- A branch review's audit runs `--workspace <WORKSPACE>`: its `report-field` rows are judged from every task report, not skipped.
- When you or the project's owner or decider rule against a reviewer's verdict on a judged row, record the ruling on that row of the review file as `override`: `{ "result": "pass" | "fail", "by": "<person or role>", "reason": "<text>" }`. The reviewer's `result` stays on the row. You write the `override`; a reviewer never writes one. `houserules stats` reads it into the `override_rate` of the rule.
- Severity follows impact, not the rule's class (`process.severity-by-impact`). Critical: wrong behavior on a shipped contract, a security exposure, data loss, a gate that passes when it must fail, or fabricated evidence. Important: a missed requirement, incorrect or fragile behavior, maintainability damage you would block a merge over, or a misstated claim that the merge decision relies on. Minor: everything else, narrative report prose included.
- Every finding is fixed (`process.no-tech-debt`). A deferral is a backlog item with a reason, named in the finding.
- Bound the review loop (`process.bounded-review-loop`). A fix round that holds only Minor fixes closes with a controller verification note, never with a re-review. A re-review reads the fix diff and the original findings only, and only a new Critical or Important finding opens a further round. The cap on re-reviews per task is the tier's: light 0, standard 1. A critical task gets no re-review of its original findings, and one review of its fix diff after a fix round of the kind that `process.risk-tiers` names; the task reviewer writes it as `task-<N>-review-r<R>.json`. A branch-review fix wave has the cap of the batch's tier. At the cap, the open non-critical findings become one backlog item with the reason, and an open Critical finding goes to the project's owner or decider. A light task with an Important or Critical finding moves up to the standard tier and gets its one re-review.
- A fix round closes in one of two recorded forms: a re-review deliverable in the workspace (the default for executable changes or any critical finding), or a controller verification note in the ledger naming each finding and the artifact proving its fix (sufficient for a round that holds only Minor fixes, whatever the kind of change, and for report-only or prose rounds) — `process.fix-round-verification-record`. In a note, run the check that each Minor finding names (name one yourself when a finding names none) and cite its result. Confirm that the round's `gates_run` lists the whole gate set of the task, not only the gates the round touched. When an implementer contests a finding because its `fix` adds a capability with no caller, rule in one of three ways. The contest holds: drop the finding and record the reason in the ledger. Defer: file a backlog item that the finding names. The contest fails: the finding stands, and the implementer fixes it in the next round.
- Ledger line per task: `Adherence: <pass>/<fail>/<warn>; judged fails: <ids or none>`.
- A plan's factual claims about where things live cite the enumeration that found them; the branch review re-derives them like report claims. In a batch without a branch review, you re-derive them before Finish.
- Log every controller slip that forces a re-run, an amend, or a correction — gate-caught or not — to the batch workspace ledger when it happens (`process.gate-shell-chains`).
- On a mid-batch branch rebuild, keep the old branch under a new name, write the old-to-new sha map to its own workspace file, cite both in the batch report, and never edit closed-task deliverables (`process.main-wins-backlog-collisions`).

## Batch close

- A project that measures the cost of its batches keeps `<workspace>/efficiency-windows.json`: one window (`phase`, `start`, and `end`, in UTC) for each phase. Close the window of a phase when the phase ends, as the Build and Verify rows say. A window rebuilt from commit dates says so in its phase name.
- Before the branch review dispatches, or before Finish in a batch without one, re-read the declared file set of each task (its Files line) and the plan's Global constraints, when it has them, against the tree. Add a revision note to the plan for each sentence that no longer holds (the `planning` skill, "Keep the plan true"). A batch with no plan has nothing to re-read.
- Before the branch review dispatches, or before Finish in a batch without one, confirm every `task-*-report.json` in each workspace has a ledger row stating its close commit and round count.
- In a batch without a branch review, run `houserules audit --base <merge base> --workspace <WORKSPACE>` before Finish, once per workspace, and judge its `controller` rows in the ledger: a task audit leaves them out (`knowledge-base.controller-rules-are-tagged`).
- Run `houserules stats --window 5 <WORKSPACE>...` with every batch workspace, this batch's included, and save the output as `<workspace>/stats.json` (`process.knowledge-earns-its-place`). The flag keeps the five most recent of the workspaces you name, and every workspace when fewer than five exist. Run it before the branch review dispatches, or before Finish in a batch without one: the branch reviewer reads that file. A batch with no branch review reads the `proposals` key itself. Once a backlog item cites a stats file, never overwrite that file: save a later run under a name that states its window, `stats-batches-<a>-<b>.json`.
- Apply every `proposals` row with `owner_gate: false` of the batch-close run in one `docs(knowledge): ...` commit, and apply the retrospective proposals of the branch review in the same commit, except a proposal that adds or changes a standing entry or changes an entry whose `source.by` is `user`. Make that commit after the branch review closes, or before Finish in a batch without one, so that a row which only the batch-close run draws is applied too. `retire` sets `status: retired` and, for a local entry, runs `houserules archive`. A pack entry stays in its pack copy: a `retire`, `demote`, or `narrow` proposal for it is an override in `knowledge/overrides.json`, and `retire` is an override with `status: retired`. `demote` drops `standing` and gives the entry an area. `narrow` rewrites the entry's scope. `mechanize` files a backlog item for the check.
- List every `owner_gate: true` row, and every retrospective proposal that the previous bullet excepts, in the batch report for the project's owner or decider to rule on.
- When the project's owner or decider rules to keep an entry that a proposal names, add the tag `ruled-keep` to the entry, so the loop stops proposing it (`process.knowledge-earns-its-place`). A `ruled-keep` entry gets no `demote`, `retire`, `narrow`, or `mechanize` proposal. It still counts toward the standing budget.
- A pack entry that the project retires, demotes, rewrites, or tags `ruled-keep` is an override in `knowledge/overrides.json`, and `houserules update` keeps the override.
- A new retrospective entry defaults to a non-standing area entry. A new standing entry names the standing entry it replaces, or the budget headroom from the latest `stats` run.

## Template evaluation

- A change to `.claude/agents/implementer.md`, `.claude/agents/task-reviewer.md` (both installed by `houserules init --workflow reference`), or `.claude/evals/*.json` needs a run of every scenario in `.claude/evals/` before the branch review, or before Finish in a batch without one (`process.evals-rerun`); the audit fails until `.claude/evals/record.json` changes with them.
- Run each scenario in a detached scratch worktree at the branch head: implementer scenarios through `implementer` with the scenario's `query` as the brief file, a `Backlog:` line that names one item of the scratch tree, and its `knowledge` as the `Knowledge:` line; `seeded-violations` through `task-reviewer` on a fixture built from its `setup`. Each dispatch names its role's configured model. Workspace artifacts carry the `eval-` prefix; keep nothing from a worktree.
- Each scenario brief names the command that runs the suite with the project's pinned tools: the test task of the project's task runner. When the project has no such task, the brief names the test command of the plan's gate list, or, in a batch with no plan, the test command that the project documents.
- When the branch changes the houserules binary, replace `houserules` in the brief with the path of the binary built from the branch.
- Judge every `expected_behavior` line from the report or review. Append one run set to `.claude/evals/record.json`: `date`, `templates` (`git rev-parse HEAD:<path>` for both templates), `runs` (`scenario`, `agent`, `model`, `pass`, `of`, `notes`).
- An empty `.claude/evals/record.json` is the state of a fresh install. The first run set you append is the baseline; it is due at the first batch that changes one of the two templates or a scenario.
- Commit a `houserules update` on its own, outside a batch branch. An update can change the two templates without an eval run of the batch, and the audit of a batch branch that holds such a change fails `process.evals-rerun`. The kit release carries its own eval run.

## Rulings

Write every ruling to its home file in the same turn (`process.rulings-to-file`), then `houserules render`. A ruling that lives only in chat is lost at compaction. A decision that makes a sentence of the plan false adds a revision note to the plan in the same turn (the `planning` skill, "Keep the plan true"). Record each decision that the owner or a review overturns as a `Reversal:` line in the ledger (`process.reversals-are-recorded`).
