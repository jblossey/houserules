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
- [quality.gates-derive-their-scope] A permanent gate derives its scope (tracked files minus declared, reasoned exclusions) and fails loudly on unreadable input; a hand-typed list is ungated.

## Gotchas

- [houserules.glob-union-matcher] RETIRED at batch 20 T3: tools/kb.mjs's own globMatch union is gone; crates/houserules/src/rules/glob.rs's globset engine is the only matcher left.
- [houserules.path-pins-mirror-the-code] A test that pins a path the binary prints builds it component by component: Path::join keeps an embedded slash, and Windows alone shows the divergence.

Detail: houserules get <id>
