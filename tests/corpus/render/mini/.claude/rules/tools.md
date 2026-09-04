---
paths:
  - "mini-tools/**"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Tools rules

## Rules

- [mini.build-cache] Clear the mini-tools build cache before every fixture regeneration run.
- [mini.no-todo] Never leave a TODO marker under mini-tools/.

## Invariants

- [mini.tool-timeout] Every mini-tools command times out after five seconds.

## Gotchas

- [mini.stale-lockfile] A stale mini-tools lockfile makes the fixture regenerate with drift.

Detail: tools/kb.sh get <id>
