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
| Design | superpowers:brainstorming when installed, else write it by hand | spec in `docs/specs/`; user approval |
| Plan | superpowers:writing-plans when installed, else write it by hand | plan in `docs/plans/`; code-health scan of the touched files (`process.code-health-scan`); user approval when asked |
| Build | superpowers:subagent-driven-development when installed, else dispatch tasks yourself | the dispatch protocol below; strictly sequential |
| Verify | — | live run before any PR or deploy spend (`process.live-run-before-ci`) |
| Finish | project skill `finishing-a-feature` | backlog ticked, one to five clean commits, PR, checks, ff-only merge |
| Rollout, acceptance | — | user acceptance; the acceptance record and new rulings ride the next branch |

In a repository that builds `houserules` from source (the houserules kit repository and its forks), when the current branch changes generated output, run every controller gate through the tree binary (`cargo run --release --quiet --bin houserules --`) instead of the installed one; reinstall after merge with `cargo install --path crates/houserules --bin houserules`. Elsewhere, an adopter's installed `houserules` has no such tree to build from and is already the released build.

## Dispatch protocol

- Templates: `implementer` (sonnet), `task-reviewer` (opus), `branch-reviewer` (fable). Name the model on every dispatch; the template value is the default, not a substitute for naming it. Reviews always run on a mightier model than the implementer they review (`process.model-policy`).
- Never dispatch two subagents at once.
- Derive a dispatch's commit span with `git rev-list --count BASE..HEAD`; never hand-type a count into a dispatch or its ledger row.
- An implementer dispatch is the task brief plus these lines:
  - `BASE: <sha>` — the commit before the task.
  - `Backlog: <ids the task delivers>`
  - `Knowledge: <ids>` — `houserules for <the brief's files>` plus the procedure ids the task needs. Five to ten ids.
  - `REPORT_FILE: <workspace>/task-<N>-report.json`
- A reviewer dispatch adds `BASE`, `HEAD`, the same `Backlog:` and `Knowledge:` lines, `REPORT_FILE`, `REVIEW_FILE: <workspace>/task-<N>-review.json` (re-review: `task-<N>-review-r<R>.json`), `AUDIT_JSON: <workspace>/task-<N>-audit.json` (re-review: `-r<R>`), and the audit command's `--ids <the Knowledge ids>`.
- The audit `--ids` value is the dispatch's `Knowledge:` list, generated from it, never typed separately — true for round 0 and every re-review alike.
- A re-review dispatch carries the identical `Knowledge:`/`Backlog:`/`--ids` block as the round-0 dispatch. A narrowed block narrows the audit package silently.
- A fix-round dispatch names `FIX_BASE`; the fix-diff audit goes into the report's `fix_rounds` entry, and `self_audit` stays the `BASE..HEAD` audit (`process.deliverables-json`).
- A fix-round dispatch quotes a finding's premise only with the review's own named verification run beside it; a premise with no such run gets no free pass — the dispatch tells the implementer to measure it first, before acting on it (HR-067: a batch 18 T6 fix round relayed an unverified hook-history premise as fact, and the correction it produced was itself false).
- The branch review dispatch names `WORKSPACE`, `BASE` (the merge base), `HEAD`, the plan and spec paths, and `REVIEW_FILE: <workspace>/branch-review.json`, through `branch-reviewer`; its audit runs `--workspace <WORKSPACE>` in place of `--report`.
- A brief names no version number for a tool, action, or package (`security-hygiene.exact-pins`); it names the verification the implementer runs and records in `docs_verified`, and shows placeholders such as `jdx/mise-action@<current major>`.
- A brief names every test, gate, and file its spec task lists, verbatim or by pointer (`process.brief-carries-the-spec`); an implementer who cannot satisfy one flags it instead of dropping it.
- A brief's claim about a file's current contents is measured at brief-writing time — grepped or read, never restated from memory or an earlier document (HR-067: a batch 19 T2 brief asserted a second knowledge copy that did not exist, and the implementer paid the disproof).
- When a batch edits an agent template or skill, the dispatch message carries the changed instruction verbatim: templates load at session start, so the running session's copy is stale until a restart.

## Handling reviews

- A review that fails `houserules validate` (an `open` row included) or is not at `REVIEW_FILE` with its `AUDIT_JSON` is incomplete: re-dispatch it. `houserules stats` reads only those files.
- A branch review's audit runs `--workspace <WORKSPACE>`: its `report-field` rows are judged from every task report, not skipped.
- Severity of an adherence failure: standing rule is Critical; area rule is Important; `warn` is Minor.
- Every finding is fixed (`process.no-tech-debt`). A deferral is a backlog item with a reason, named in the finding.
- Ledger line per task: `Adherence: <pass>/<fail>/<warn>; judged fails: <ids or none>`.
- A fix round closes in one of two recorded forms: a re-review deliverable in the workspace (the default for executable changes or any critical finding), or a controller verification note in the ledger naming each finding and the artifact proving its fix (sufficient for report-only or prose rounds) — `process.fix-round-verification-record`.
- A plan's factual claims about where things live cite the enumeration that found them; the branch review re-derives them like report claims.
- Batch close: before the branch review dispatches, confirm every `task-*-report.json` in the workspace has a ledger row stating its close commit and round count.
- Log every controller slip that forces a re-run, an amend, or a correction — gate-caught or not — to the batch workspace ledger when it happens (`process.gate-shell-chains`).
- On a mid-batch branch rebuild, write the old-to-new sha map to its own workspace file, cite it in the batch report, and never edit closed-task deliverables (`process.main-wins-backlog-collisions`).
- Retrospective proposals from the branch review: apply every proposal that does not change a `standing` entry in one `docs(knowledge): ...` commit before finishing; list standing-rule proposals in the batch report for the user's ruling.

## Template evaluation

- A change to `.claude/agents/implementer.md`, `.claude/agents/task-reviewer.md`, or `.claude/evals/*.json` needs a run of every scenario in `.claude/evals/` before the branch review (`process.evals-rerun`); the audit fails until `.claude/evals/record.json` changes with them.
- Run each scenario in a detached scratch worktree at the branch head: implementer scenarios through `implementer` (sonnet) with the scenario's `query` as the brief file and its `knowledge` as the `Knowledge:` line; `seeded-violations` through `task-reviewer` (opus) on a fixture built from its `setup`. Workspace artifacts carry the `eval-` prefix; keep nothing from a worktree.
- Judge every `expected_behavior` line from the report or review. Append one run set to `.claude/evals/record.json`: `date`, `templates` (`git rev-parse HEAD:<path>` for both templates), `runs` (`scenario`, `agent`, `model`, `pass`, `of`, `notes`).

## Rulings

Write every ruling to its home file in the same turn (`process.rulings-to-file`), then `houserules render`. A ruling that lives only in chat is lost at compaction.
