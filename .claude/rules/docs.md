---
paths:
  - "docs/**"
  - "README.md"
  - "AGENTS.md"
  - "knowledge/**"
  - "backlog/**"
  - ".claude/**"
---
Generated from knowledge/ by houserules render. Do not edit.

# Docs rules

## Rules

- [houserules.plan-semantics-run-on-the-corpus] A classifier, threshold, or matching rule that a plan fixes in words runs over the real corpus before the plan is approved; the plan cites the run.
- [houserules.readme-mirrors-kit-owned] When `KIT_OWNED` gains or loses a path, update the ownership table of docs/ownership.md, and any README list of kit files, in the same commit.
- [knowledge-base.cite-durable-refs] Cite a commit in a permanent file only if it is or will be reachable from main; name in-branch events by batch/task/round; restate interim SHAs at aggregation.
- [knowledge-base.controller-rules-are-tagged] Tag an entry `controller` when only the acts of the person who runs the work can break it; a task audit leaves it out, the branch audit judges it.
- [knowledge-base.ids-are-permanent] Never rename a merged entry id. Add a new entry and link the old one with `see`.
- [knowledge-base.rules-need-a-loading-path] Give every rule, invariant, or checked entry a loading path: `standing: true`, or an area whose globs match the files it governs.
- [knowledge-base.state-only-the-source] State only what the source states; verify polarity, counts, and mechanisms against the code before filing an entry or shipping teaching prose.
- [knowledge-base.summary-is-the-rule] The `summary` states the rule in one sentence; `body` carries why, how, exceptions; examples in docs and skills model this split. No time-sensitive phrasing.
- [process.evals-rerun] Re-run every scenario when the template of the author of the change or of the task review, or a scenario, changes; append the run to the scenario record.
- [writing-style.readme-is-an-adopter-funnel] A README is an adopter funnel: value, install, runnable usage, then depth; keep out process history, provenance narration, and claims that rot with releases.

## Gotchas

- [knowledge-base.context-proxy-output-is-lossy] A context proxy can shorten or drop houserules output with no visible sign; the source files and the gates are the truth, not the transcript.
- [process.pointer-sweep-unfiltered] When a file moves or is deleted, sweep for its old path with an unfiltered, case-insensitive grep over the whole tree; an extension filter misses pointers.

Detail: houserules get <id>
