---
paths:
  - "template/**"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Template rules

## Rules

- [writing-style.instructions-cover-the-state-space] An instruction keyed to one value of an enumerated field names the value it displaces; test the sentence against every other value before shipping.

## Invariants

- [houserules.payload-runs-on-builtins] The payload in `template/` runs on Node built-ins and POSIX shell only: no dependency, no build step; the CLIs work in a fresh clone before any install.

Detail: tools/kb.sh get <id>
