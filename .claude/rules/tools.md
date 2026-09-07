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
- [houserules.release-workflow-is-generated] release.yml is generated from dist-workspace.toml; never hand-edit it — edit the config and run `dist generate` (`dist generate --check` is the gate).
- [process.wiring-checks-run-the-resolution] A trigger, pin, or wiring check runs the resolution end to end - feed the produced value to its consumer; a string, glob, or schema match proves nothing.

## Gotchas

- [houserules.actions-default-shell-lacks-pipefail] An Actions run step with no shell key runs bash -e without pipefail: a failing left side of a pipe passes silently; name shell: bash for -eo pipefail.
- [houserules.default-token-tags-start-no-workflows] A tag or commit pushed with the default GITHUB_TOKEN triggers no workflow; a release hand-off needs a PAT/App token or an explicit workflow_dispatch hop.

Detail: houserules get <id>
