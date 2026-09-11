---
paths:
  - "docs/**"
  - "README.md"
  - "CLAUDE.md"
  - "knowledge/**"
  - "backlog/**"
  - ".claude/**"
---
Generated from knowledge/ by houserules render. Do not edit.

# Docs rules

## Rules

- [houserules.readme-mirrors-kit-owned] When `KIT_OWNED` gains or loses a path, update README's agent-layer bullet and ownership table in the same commit.
- [knowledge-base.cite-durable-refs] Cite a commit in a permanent file only if it is or will be reachable from main; name in-branch events by batch/task/round; restate interim SHAs at aggregation.
- [knowledge-base.ids-are-permanent] Never rename a merged entry id. Add a new entry and link the old one with `see`.
- [knowledge-base.rules-need-a-loading-path] Give every rule, invariant, or checked entry a loading path: `standing: true`, or an area whose globs match the files it governs.
- [knowledge-base.state-only-the-source] State only what the source states; verify polarity, counts, and mechanisms against the code before filing an entry or shipping teaching prose.
- [knowledge-base.summary-is-the-rule] The `summary` states the rule in one sentence; `body` carries why, how, exceptions; examples in docs and skills model this split. No time-sensitive phrasing.
- [process.evals-rerun] Re-run every `.claude/evals/` scenario when the implementer or task-reviewer template or a scenario changes; append the run to `.claude/evals/record.json`.

Detail: houserules get <id>
