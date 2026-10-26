---
name: planning
description: Use when an approved spec needs its plan: tasks with declared file sets, the tier, critical tasks, technical-debt candidates, and the code-health scan
---

# Planning a batch

You are the controller. A plan turns an approved spec into tasks that an implementer can run without asking you. Plan only from an approved spec; a plan written before its approved spec is void.

Write the output of every `houserules` command to a file in the batch workspace, a folder of the `workspaces` location (default `.houserules/work/`), and read the file with your file-read tool. Treat a `<<` marker or a "more items" note as a lost read and read the source file.

## 1. Pick the tier and mark the critical tasks

Pick the tier from the change's risk (`process.risk-tiers`, and the Tiers section of the `orchestrating` skill). Record the tier and the criterion that selected it in the plan.

Mark each critical task: a task whose change alters the CLI's commands, flags, or output, a schema, authentication or security code, or the data on disk of an install. For a critical task, the plan turns the spec's cases for the task into tests that the implementer commits first, with one recorded run that shows them failing at `BASE`. The evidence is that run and the whole suite passing at `HEAD`. The whole-suite half is the integration run of the person who runs the work.

## 2. Fold in technical debt

Run `houserules list --deferred` into a file. Fold in every open deferral whose files overlap the planned files of a task and that passes the convenience criterion: folding it in does not turn a non-critical task into a critical task. The plan lists each folded candidate with its overlapping files, and each overlapping deferral that you leave out with the reason.

## 3. Scan the code health

Read the files that the batch will change (`process.code-health-scan`). Write a Code health section that names the smells and antipatterns you found, what the plan fixes, and what goes to the tracker and why. A clean scan states what you checked.

## 4. Write the tasks

Each task names:

- The work item ids it delivers (`Backlog:`). The implementer writes a `Work-Item:` trailer for each id on every commit.
- Its files: the declared file set, and the interfaces it consumes and produces.
- The spec sections, tests, and gates it must satisfy, by pointer or verbatim. An implementer who cannot satisfy one flags it.
- Its steps in test-first order.

A task that adds or tightens a check in `check-knowledge`, `check-backlog`, or `audit` also carries a paper walk: a fresh install, an install on each released kit under the new binary, an install with kept or overridden kit-owned files, and each author class of commits. The report of the task states each accepted red.

Mark tasks parallel only when their declared file sets are disjoint and no task needs another's output (`process.parallel-on-disjoint-files`). Name the files that you regenerate at integration.

The plan may name a file that two tasks share, such as a command-line entry point whose changes merge by keeping both sides. Two tasks whose only shared file is that file may run at once. You resolve the conflict of the later cherry-pick by keeping both sides and run the gates. Any other resolution is a fix round for the later task.

Order the tasks in waves when some tasks need others. A wave orders the tasks; it is no barrier. A task starts as soon as every task it needs is integrated. A task that consumes only an interface that no open finding changes starts from an interim base that holds the commits of the task it needs. You integrate the commits of the consuming task alone, after the task it needs closes.

A gate command that the harness refuses to an agent is a controller-run integration gate: the plan lists it, and you run it after each integration and record it in the ledger.

Besides the full suite, the plan marks each suite that only you run at integration, for example a slow suite or a suite for another platform. A task's own run skips every such suite.

A task that changes the implementer template, the task-reviewer template, or an eval scenario needs the eval run of the Template evaluation section of the `orchestrating` skill. Say so in the plan.

## 5. Approve

Write the plan to `docs/plans/`. Each factual claim in it about where things live cites the enumeration that found it. Take the plan to the project's owner or decider when they ask for it.

## 6. Keep the plan true

A decision that makes a sentence of the plan false adds a revision note to the plan in the same turn. At batch close, re-read the declared file set of each task (its Files line) and the plan's Global constraints, when it has them, against the tree. Add a revision note for each sentence that no longer holds.
