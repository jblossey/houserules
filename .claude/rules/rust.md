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
- [houserules.kit-lints-read-kit-vocabulary] A check over kit-owned text takes its vocabulary from the embedded payload; a name the adopter chooses never turns kit text into a finding.
- [houserules.platform-gated-tests] Gate platform-specific code with #[cfg]: a std::os::unix or std::os::windows use compiles only on that platform; CI builds all targets on three OSes.
- [houserules.task-gates-mirror-ci] A task-end gate run includes every check CI runs on push or pull request; a local gate list that omits a CI check is stale.
- [quality.exclusive-pins-are-mutation-proven] A test that pins 'this and nothing else' compares the whole structure and proves each forbidden shape with a mutation that turns it red.
- [quality.gates-derive-their-scope] A permanent gate derives its scope (tracked files minus declared, reasoned exclusions) and fails loudly on unreadable input; a hand-typed list is ungated.
- [quality.lint-the-token-not-the-document] A lint whose subject is a delimited token matches the token and its delimiters with one pattern; it pairs no delimiters and models no document around it.

## Gotchas

- [houserules.commit-type-under-the-crate-path-decides-a-release] A feat or fix commit that touches crates/houserules/** makes release-please propose a release; a CI change or its tests use ci, test, refactor, or chore.
- [houserules.path-pins-mirror-the-code] A test that pins a path the binary prints builds it component by component: Path::join keeps an embedded slash, and Windows alone shows the divergence.

Detail: houserules get <id>
