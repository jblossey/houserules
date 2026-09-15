---
paths:
  - "template/**"
---
Generated from knowledge/ by houserules render. Do not edit.

# Template rules

## Rules

- [writing-style.instructions-cover-the-state-space] An instruction keyed to one value of an enumerated field names the value it displaces; test the sentence against every other value before shipping.

## Invariants

- [houserules.agents-md-is-canonical] AGENTS.md is the cross-harness instruction surface; CLAUDE.md is a one-line pointer to it; every seeded text stays harness-neutral.
- [houserules.payload-runs-on-builtins] The vendored payload runs on the `houserules` binary and POSIX shell only: every shipped reference invokes it directly; no file needs Node, npm, or an install.

Detail: houserules get <id>
