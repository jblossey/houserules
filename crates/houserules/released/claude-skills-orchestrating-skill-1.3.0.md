---
name: orchestrating
description: Use when starting a session in this repository, starting or resuming a batch, or dispatching any subagent
---

# Orchestrating a batch

You are the controller. Subagents get knowledge through their templates; you get it through this skill, the standing rules, and `houserules`. Every read command prints JSON; read every output below as JSON.

## Session ritual (session start, resume, and after compaction)

1. `git status --short && git log --oneline -15`
2. `houserules list --batch <n>` for the in-progress batch, or `houserules list --open` when none is in progress.
3. If a plan is in flight: read the batch workspace ledger. Trust the ledger and `git log` over memory.
4. `houserules index --area process`; `houserules get` what the next step needs.
5. Re-read the spec and plan in flight before the next dispatch.

## Batch lifecycle

| Phase | Skill | Repo gate |
|---|---|---|
| Select items | — | `houserules list --open`; record the batch in `backlog/batches.json` and schedule its items (`set <id> batch=<n>`) |
| Design | superpowers:brainstorming when installed, else write it by hand | spec in `docs/specs/`; approval from the project's owner or decider |
| Plan | superpowers:writing-plans when installed, else write it by hand | plan in `docs/plans/`; code-health scan of the touched files (`process.code-health-scan`); approval when asked |
| Build | superpowers:subagent-driven-development when installed, else dispatch tasks yourself | the dispatch protocol below |
| Verify | — | live run before any PR or deploy spend (`process.live-run-before-ci`) |
| Finish | project skill `finishing-a-feature` | backlog ticked, one to five clean commits, PR, checks, merged per the project's own discipline |
| Rollout, acceptance | — | acceptance from the project's owner or decider; the acceptance record and new rulings ride the next branch |

## Tiers

The change's risk picks the tier at plan time (`process.risk-tiers`). The lifecycle table above is the full tier. Every tier runs Select items, Verify (the live run), Finish, and Rollout and acceptance. A lighter tier changes only what its pipeline column names: light uses the backlog item's body as the spec and drops the plan document and the branch review; standard uses a short spec and drops the branch review for a one-task batch. The re-review cap per task differs by tier.

| Tier | Selected when | Pipeline |
|---|---|---|
| light | one task; docs, comments, config values, or a dependency bump; no executable behavior change; no `full-tier` entry applies | the backlog item's body is the spec (the project's owner or decider still approves it); implementer; one task review; controller live run. No plan document, no branch review. |
| standard | every change that is neither light nor full | short spec and approval from the project's owner or decider; plan; implementer; one task review and at most one re-review; controller live run; branch review only for a batch of two or more tasks |
| full | `houserules for --full <planned files>` returns an entry tagged `full-tier`, or the change alters a public contract, a schema, authentication or security code, or a data migration | the whole lifecycle above; at most two re-reviews per task |

- Mark a high-risk area with an entry in that area tagged `full-tier`. A tag is a free string of the knowledge schema: no schema change, no new command.
- A change that meets a full criterion runs the full tier, whatever else it meets. One tier covers the batch: the highest tier that any of its changes needs.
- The plan records the tier and the criterion that selected it. A light change has no plan: the dispatch brief records both.
- A tier moves up during a batch, never down. A light task with an Important or Critical finding moves up to standard (`process.bounded-review-loop`).
- The task reviewer checks the tier. A wrong tier is a finding.

## Parallel and sequential dispatch

Dispatch agents in parallel only when the plan marks their tasks parallel (`process.parallel-on-disjoint-files`). Dispatch every other agent sequentially: never start a second one before the last one closes.

- Mark tasks parallel in the plan when their declared file sets are disjoint and no task needs another's output. You own the files that many tasks regenerate: the generated rule files and skill (`houserules render`), `backlog/`, lockfiles, and any other file that a build or sync step regenerates. A parallel task does not touch them. You regenerate them at integration.
- Give each parallel implementer its own git worktree, on its own branch from the same `BASE`.
- Name each parallel agent's scratch root in its dispatch, `<scratchpad>/task-<N>/`, and tell it to write scratch files nowhere else. Every agent of one session gets the same scratchpad path, and two agents that write one file name there overwrite each other without an error.
- Integrate a finished task onto the batch branch one at a time (cherry-pick). Run the gates after each. Write the old-to-new commit map to the ledger. A conflict or a red gate at integration is a fix round for the later task.
- Reviews of different tasks may run at the same time: reviewers do not write to the tree. The review of a task starts after its implementer closes.
- Run at most three agents at once unless the plan sets a lower limit. The ledger records each dispatch's start and close.

A project may rule its own parallelism discipline (elicited by the `migrating-knowledge` skill's "Elicit your own operating discipline" step, recorded as its own knowledge entry); that ruling replaces this default. An `AGENTS.md` or `CLAUDE.md` line from a kit before 1.3.0 that says to hand off or dispatch tasks sequentially by default states the old kit default, not a project ruling. `houserules update` never rewrites an existing `AGENTS.md` or `CLAUDE.md`, so replace that line by hand. The replacement line belongs in `AGENTS.md`: `If your harness can hand a task to another agent, run handoffs in parallel only when the plan marks their file sets disjoint, each in its own worktree; run every other handoff sequentially. The orchestrating skill holds the procedure.` An install seeded by the 1.0.0 kit holds the old line in `CLAUDE.md`, and `update` adds an `AGENTS.md` that already carries the new line: delete the old line from `CLAUDE.md`, so that the two files agree.

## Dispatch protocol

- Templates: `implementer` (sonnet), `task-reviewer` (opus), `branch-reviewer` (fable). Name the model on every dispatch; the template value is the default, not a substitute for naming it. Reviews run on a mightier model than the implementer they review by default (`process.model-policy` is a recommendation the project may override).
- Derive a dispatch's commit span with `git rev-list --count BASE..HEAD`; never hand-type a count into a dispatch or its ledger row.
- An implementer dispatch is the task brief plus these lines:
  - `BASE: <sha>` — the commit before the task.
  - `Backlog: <ids the task delivers>`
  - `Tier: <light|standard|full> (<criterion>)` — the tier and the criterion that selected it (`process.risk-tiers`).
  - `Knowledge: <ids>` — `houserules for <the brief's files>` plus the procedure ids the task needs. Five to ten ids.
  - `REPORT_FILE: <workspace>/task-<N>-report.json`
- A reviewer dispatch adds `BASE`, `HEAD`, the same `Backlog:`, `Tier:`, and `Knowledge:` lines, `REPORT_FILE`, `REVIEW_FILE: <workspace>/task-<N>-review.json` (re-review: `task-<N>-review-r<R>.json`), `AUDIT_JSON: <workspace>/task-<N>-audit.json` (re-review: `-r<R>`), and the audit command's `--ids <the Knowledge ids>`.
- The audit `--ids` value is the dispatch's `Knowledge:` list, generated from it, never typed separately — true for round 0 and every re-review alike.
- A re-review dispatch carries the identical `Knowledge:`/`Backlog:`/`--ids` block as the round-0 dispatch, and the task's current `Tier:` line. A narrowed block narrows the audit package silently.
- A fix-round dispatch names `FIX_BASE`; the fix-diff audit goes into the report's `fix_rounds` entry, and `self_audit` stays the `BASE..HEAD` audit (`process.deliverables-json`).
- A parallel task's fix round commits in the task's own worktree on its `FIX_BASE`, because a worktree-isolated agent cannot run git against the shared checkout. You cherry-pick that commit, run the gates, and extend the ledger's commit map.
- A fix-round dispatch quotes a finding's premise only with the review's own named verification run beside it; a premise with no such run gets no free pass — the dispatch tells the implementer to measure it first, before acting on it. Relaying an unverified premise as fact produces a correction that can itself be false.
- The branch review dispatch names `WORKSPACE`, `BASE` (the merge base), `HEAD`, the plan and spec paths, and `REVIEW_FILE: <workspace>/branch-review.json`, through `branch-reviewer`; its audit runs `--workspace <WORKSPACE>` in place of `--report`.
- A brief names no version number for a tool, action, or package (`security-hygiene.exact-pins`); it names the verification the implementer runs and records in `docs_verified`, and shows placeholders such as `jdx/mise-action@<current major>`.
- A brief names every test, gate, and file its spec task lists, verbatim or by pointer (`process.brief-carries-the-spec`); an implementer who cannot satisfy one flags it instead of dropping it.
- A brief's claim about a file's current contents is measured at brief-writing time — grepped or read, never restated from memory or an earlier document; an unmeasured claim about a knowledge copy or a file's shape is exactly the kind of thing that can be flatly wrong, and the implementer pays the disproof.
- When a batch edits an agent template or skill, the dispatch message carries the changed instruction verbatim: templates load at session start, so the running session's copy is stale until a restart.
- A controller commit made while a task is open (a concurrent backlog filing, a rulings-to-file write) touches only files disjoint from the open task's own files; record it in the ledger before the next reviewer dispatch, and name it in that dispatch.

## Handling reviews

- A review that fails `houserules validate` (an `open` row included) or is not at `REVIEW_FILE` with its `AUDIT_JSON` is incomplete: re-dispatch it. `houserules stats` reads only files at those names: every `task-*-audit*.json`, `task-*-review*.json`, `task-*-report.json`, and `branch-review.json` in a workspace.
- A branch review's audit runs `--workspace <WORKSPACE>`: its `report-field` rows are judged from every task report, not skipped.
- Severity follows impact, not the rule's class (`process.severity-by-impact`). Critical: wrong behavior on a shipped contract, a security exposure, data loss, a gate that passes when it must fail, or fabricated evidence. Important: a missed requirement, incorrect or fragile behavior, maintainability damage you would block a merge over, or a misstated claim that the merge decision relies on. Minor: everything else, narrative report prose included.
- Every finding is fixed (`process.no-tech-debt`). A deferral is a backlog item with a reason, named in the finding.
- Bound the review loop (`process.bounded-review-loop`). A fix round that holds only Minor fixes closes with a controller verification note, never with a re-review. A re-review reads the fix diff and the original findings only, and only a new Critical or Important finding opens a further round. The cap on re-reviews per task is the tier's: light 0, standard 1, full 2. A branch-review fix wave has the cap of the batch's tier. At the cap, the open non-critical findings become one backlog item with the reason, and an open Critical finding goes to the project's owner or decider. A light task with an Important or Critical finding moves up to the standard tier and gets its one re-review.
- A fix round closes in one of two recorded forms: a re-review deliverable in the workspace (the default for executable changes or any critical finding), or a controller verification note in the ledger naming each finding and the artifact proving its fix (sufficient for a round that holds only Minor fixes, whatever the kind of change, and for report-only or prose rounds) — `process.fix-round-verification-record`.
- Ledger line per task: `Adherence: <pass>/<fail>/<warn>; judged fails: <ids or none>`.
- A plan's factual claims about where things live cite the enumeration that found them; the branch review re-derives them like report claims. In a batch without a branch review, you re-derive them before Finish.
- Log every controller slip that forces a re-run, an amend, or a correction — gate-caught or not — to the batch workspace ledger when it happens (`process.gate-shell-chains`).
- On a mid-batch branch rebuild, write the old-to-new sha map to its own workspace file, cite it in the batch report, and never edit closed-task deliverables (`process.main-wins-backlog-collisions`).

## Batch close

- Before the branch review dispatches, or before Finish in a batch without one, confirm every `task-*-report.json` in the workspace has a ledger row stating its close commit and round count.
- Run `houserules stats <WORKSPACE>...` over the five most recent batch workspaces (every workspace when fewer than five exist), this batch's included, and save the output as `<workspace>/stats.json` (`process.knowledge-earns-its-place`). Run it before the branch review dispatches, or before Finish in a batch without one: the branch reviewer reads that file. A batch with no branch review reads the `proposals` key itself. Once a backlog item cites a stats file, never overwrite that file: save a later run under a name that states its window, `stats-batches-<a>-<b>.json`.
- Apply every `proposals` row with `owner_gate: false` of the batch-close run in one `docs(knowledge): ...` commit, and apply the retrospective proposals of the branch review in the same commit, except a proposal that adds or changes a standing entry or changes an entry whose `source.by` is `user`. Make that commit after the branch review closes, or before Finish in a batch without one, so that a row which only the batch-close run draws is applied too. `retire` sets `status: retired` and runs `houserules archive`. `demote` drops `standing` and gives the entry an area. `narrow` rewrites the entry's scope. `mechanize` files a backlog item for the check.
- List every `owner_gate: true` row, and every retrospective proposal that the previous bullet excepts, in the batch report for the project's owner or decider to rule on.
- When the project's owner or decider rules to keep an entry that a proposal names, add the tag `ruled-keep` to the entry, so the loop stops proposing it (`process.knowledge-earns-its-place`). A `ruled-keep` entry gets no `demote`, `retire`, `narrow`, or `mechanize` proposal. It still counts toward the standing budget.
- A kit-shipped entry that the project retires, demotes, rewrites, or tags `ruled-keep` goes into `.houserules.json` `overrides`, so `houserules update` stays silent about it.
- A new retrospective entry defaults to a non-standing area entry. A new standing entry names the standing entry it replaces, or the budget headroom from the latest `stats` run.

## Template evaluation

- A change to `.claude/agents/implementer.md`, `.claude/agents/task-reviewer.md`, or `.claude/evals/*.json` needs a run of every scenario in `.claude/evals/` before the branch review, or before Finish in a batch without one (`process.evals-rerun`); the audit fails until `.claude/evals/record.json` changes with them.
- Run each scenario in a detached scratch worktree at the branch head: implementer scenarios through `implementer` (sonnet) with the scenario's `query` as the brief file and its `knowledge` as the `Knowledge:` line; `seeded-violations` through `task-reviewer` (opus) on a fixture built from its `setup`. Workspace artifacts carry the `eval-` prefix; keep nothing from a worktree.
- Judge every `expected_behavior` line from the report or review. Append one run set to `.claude/evals/record.json`: `date`, `templates` (`git rev-parse HEAD:<path>` for both templates), `runs` (`scenario`, `agent`, `model`, `pass`, `of`, `notes`).
- An empty `.claude/evals/record.json` is the state of a fresh install. The first run set you append is the baseline; it is due at the first batch that changes one of the two templates or a scenario.
- Commit a `houserules update` on its own, outside a batch branch. An update can change the two templates without an eval run of the batch, and the audit of a batch branch that holds such a change fails `process.evals-rerun`. The kit release carries its own eval run.

## Rulings

Write every ruling to its home file in the same turn (`process.rulings-to-file`), then `houserules render`. A ruling that lives only in chat is lost at compaction.
