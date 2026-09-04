---
paths:
  - "tools/**"
  - ".github/**"
  - "mise.toml"
---
Generated from knowledge/ by tools/kb.sh render. Do not edit.

# Tools rules

## Gotchas

- [houserules.default-token-tags-start-no-workflows] A tag or commit pushed with the default GITHUB_TOKEN triggers no workflow; a release hand-off needs a PAT/App token or an explicit workflow_dispatch hop.

Detail: tools/kb.sh get <id>
