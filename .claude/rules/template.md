---
paths:
  - "template/**"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Template rules

## Invariants

- [houserules.payload-runs-on-builtins] The payload in `template/` runs on Node built-ins and POSIX shell only: no dependency, no build step; the CLIs work in a fresh clone before any install.

## Gotchas

- [houserules.glob-union-matcher] globMatch in tools/kb.mjs is a union: matchesGlob for the full vocabulary, globToRegExp only for `**`/`*` with dot-segments; combining both matches neither.

Detail: tools/kb.sh get <id>
