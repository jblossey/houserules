---
paths:
  - "crates/**"
  - "Cargo.toml"
  - "Cargo.lock"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Rust rules

## Rules

- [houserules.crash-paths-are-named] Where the frozen JS crashed or a glob/regex fails to compile, the binary reports one named error or finding — never a reproduced crash, never a silent default.

## Gotchas

- [houserules.glob-union-matcher] globMatch in tools/kb.mjs is a union: matchesGlob for the full vocabulary, globToRegExp only for `**`/`*` with dot-segments; combining both matches neither.

Detail: tools/kb.sh get <id>
