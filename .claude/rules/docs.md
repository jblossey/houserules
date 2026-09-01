---
paths:
  - "docs/**"
  - "README.md"
  - "CLAUDE.md"
  - "knowledge/**"
  - "backlog/**"
  - ".claude/**"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Docs rules

## Rules

- [knowledge-base.ids-are-permanent] Never rename a merged entry id. Add a new entry and link the old one with `see`.
- [knowledge-base.state-only-the-source] An entry states only what its source states; verify polarity and numbers against the code before filing it; never add an instruction the source did not give.
- [knowledge-base.summary-is-the-rule] An entry's `summary` states the rule or fact in one sentence; `body` states why, how, and the exceptions. No time-sensitive phrasing in a summary.
- [process.evals-rerun] Re-run every `.claude/evals/` scenario when the implementer or task-reviewer template or a scenario changes; append the run to `.claude/evals/record.json`.

Detail: tools/kb.sh get <id>
