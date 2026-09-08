---
paths:
  - "crates/**"
  - "Cargo.toml"
  - "Cargo.lock"
---
Generated from knowledge/ by houserules render. Do not edit.

# Rust rules

## Rules

- [houserules.crash-paths-are-named] Where the frozen JS crashed or a glob/regex fails to compile, the binary reports one named error or finding — never a reproduced crash, never a silent default.

## Gotchas

- [houserules.glob-union-matcher] RETIRED at batch 20 T3: tools/kb.mjs's own globMatch union is gone; crates/houserules/src/rules/glob.rs's globset engine is the only matcher left.

Detail: houserules get <id>
