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
- [houserules.platform-gated-tests] Gate platform-specific code with #[cfg]: a std::os::unix or std::os::windows use compiles only on that platform; CI builds all targets on three OSes.
- [houserules.task-gates-mirror-ci] A task-end gate run includes every check CI runs on push; a local gate list that omits a CI check is stale.
- [quality.gates-derive-their-scope] A permanent gate derives its scope (tracked files minus declared, reasoned exclusions) and fails loudly on unreadable input; a hand-typed list is ungated.

## Gotchas

- [houserules.path-pins-mirror-the-code] A test that pins a path the binary prints builds it component by component: Path::join keeps an embedded slash, and Windows alone shows the divergence.

Detail: houserules get <id>
