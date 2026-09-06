---
paths:
  - "tools/**"
  - ".github/**"
  - "mise.toml"
---
Generated from knowledge/ by houserules render. Do not edit.

# Tools rules

## Rules

- [houserules.dev-tools-are-rust-native] Dev tooling is a cargo bin under crates/houserules/src/bin/; never a new Node script.

## Gotchas

- [houserules.default-token-tags-start-no-workflows] A tag or commit pushed with the default GITHUB_TOKEN triggers no workflow; a release hand-off needs a PAT/App token or an explicit workflow_dispatch hop.

Detail: houserules get <id>
