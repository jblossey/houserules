---
paths:
  - "template/**"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Template rules

## Invariants

- [houserules.payload-runs-on-builtins] The payload in `template/` runs on Node built-ins and POSIX shell only: no dependency, no build step; the CLIs work in a fresh clone before any install.

Detail: tools/kb.sh get <id>
