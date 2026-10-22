---
name: designing
description: Use when a standard-tier batch needs its design: draft the spec from what the project already holds, have it reviewed, and take it to the project's owner or decider
---

# Designing a batch

You are the controller. A design starts from what the project already holds: the work items of the batch, their rulings, and the earlier specs. Draft the spec from that material and ask the human only what it leaves open.

Write the output of every `houserules` command to a file in the batch workspace, a folder of the `workspaces` location (default `.houserules/work/`), and read the file with your file-read tool. Treat a `<<` marker or a "more items" note as a lost read and read the source file.

A light change needs no spec document: the work item's body is the spec (see the Tiers section of the `orchestrating` skill). This skill covers the standard tier.

## 1. Gather

1. Run `houserules get <ids>` for the items of the batch into a file. Read each body and each ruling in it.
2. Read the specs, the decisions, and the plans that the items cite, and the files that the batch will change.
3. List what the material leaves open: a decision that no ruling covers, two sources that disagree, a fact that nobody measured.

## 2. Draft the spec

Write the spec to `docs/specs/`. It holds:

- The items it delivers, and the outcome of each.
- The facts the design rests on. Label each run, read, or assumed, and cite its capture or its source (`process.design-claims-are-labelled`).
- Each decision. A decision that rejects a library, or departs from the source research or an approved spec, carries a captured probe, the best case of the rejected option, and a reversal condition (`process.design-decisions-carry-probes`).
- The concepts it takes from its sources, each with the source's status and a check against the goal (`process.specs-list-borrowed-concepts`).
- The technical-debt candidates. Run `houserules list --deferred` into a file. Fold in every open deferral whose files overlap the planned files and that passes the convenience criterion: folding it in does not turn a non-critical task into a critical task. List each folded candidate with its overlapping files, and each overlapping deferral that you leave out with the reason.
- The acceptance cases. Each names what its capture must show. An upgrade case lists the reds that the change causes by design on an install of the previous version, each with its remedy.
- The questions that only the human can rule, each with your recommendation.

Run `houserules check-report-claims --doc <the spec path>` and fix every workspace path that the spec cites and that does not exist.

## 3. Review the design

Before the human sees the spec, a fresh `spec-reviewer` attacks it. Dispatch it as the dispatch protocol of the `orchestrating` skill describes, with `REVIEW_FILE: <workspace>/spec-review.json`.

1. Read the review. Fix every finding in the spec.
2. A review that holds a Critical finding sends the spec to a fresh `spec-reviewer`, which reviews the whole spec again and writes `spec-review-r<R>.json`. Repeat until a review holds no Critical finding.
3. A review that holds no Critical finding ends the loop. Fold its Important and Minor findings into the spec without a further review.
4. Record each decision that the owner or a review overturns as a `Reversal:` line in the ledger (`process.reversals-are-recorded`).

## 4. The decision gate

Take the spec to the project's owner or decider. Ask one question at a time, each with your recommendation. Write every ruling to its home file in the same turn (`process.rulings-to-file`).

The plan starts after the approval, with the `planning` skill. A plan written before its approved spec is void: delete it and plan again from the spec.
