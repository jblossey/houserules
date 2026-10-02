---
paths:
  - "tools/**"
  - ".github/**"
  - "mise.toml"
---
Generated from knowledge/ by houserules render. Do not edit.

# Tools rules

## Rules

- [houserules.actions-pinned-by-sha] Pin every workflow `uses:` to a full commit SHA, version tag as a comment; the Actions policy rejects a tag ref; release.yml pins live in dist-workspace.toml.
- [houserules.dev-tools-are-rust-native] Dev tooling is a cargo bin under crates/houserules/src/bin/; never a new Node script.
- [houserules.release-workflow-is-generated] release.yml is generated from dist-workspace.toml; never hand-edit it — edit the config and run `dist generate` (`dist generate --check` is the gate).
- [process.wiring-checks-run-the-resolution] A trigger, pin, or wiring check runs the resolution end to end - feed the produced value to its consumer; a string, glob, or schema match proves nothing.
- [quality.selecting-steps-walk-the-states-of-what-they-select] Walk a step that selects a resource and acts on it through each state the resource can have when read: gone, merged, failing, edited, stale in the listing.

## Invariants

- [houserules.release-please-owns-the-release-object] release-please is the only tool that creates a GitHub Release here; dist uploads into it (create-release = false) and never calls gh release create.

## Gotchas

- [houserules.actions-default-shell-lacks-pipefail] An Actions run step with no shell key runs bash -e without pipefail: a failing left side of a pipe passes silently; name shell: bash for -eo pipefail.
- [houserules.auto-merge-waits-for-every-ruleset-rule] An enabled auto-merge waits for every ruleset requirement, approval and up-to-date branch included; a bypass actor's exemption does not apply to it.
- [houserules.c-toolchain-crates-need-ci-mingw] A dependency with a C build script (a `-sys` crate) breaks the windows-gnu cross check unless ci.yml installs a mingw-w64 C compiler first.
- [houserules.codeql-action-steps-share-one-version] The `github/codeql-action` `init` and `analyze` steps must run one version; a bump of one step alone fails `analyze` on the configuration-version mismatch.
- [houserules.default-token-tags-start-no-workflows] A tag or commit pushed with the default GITHUB_TOKEN triggers no workflow; a release hand-off needs a PAT/App token or an explicit workflow_dispatch hop.
- [houserules.gh-filtered-lists-read-the-search-index] `gh pr list` with `--app` or `--author` reads GitHub's search index, not the pull request list; a step that acts on a result lists with no filter flag.
- [houserules.main-takes-only-checked-commits] The main ruleset rejects pushes whose commits lack the 8 required checks: even a one-line tick rides a branch and PR; only a green PR head ff-merges clean.
- [houserules.mise-cooldown-on-release-day] mise's minimum_release_age (default 24h) keeps a repository's first-ever release from resolving via @latest for a day; later releases fall back unaffected.
- [houserules.release-footer-survives-aggregation] A Release-As footer must survive aggregation: keep its empty commit or restate it on a crates-touching commit; no other body line starts 'Release-As:'.
- [houserules.secret-store-follows-the-triggering-actor] A run reads Dependabot secrets when Dependabot starts it, a push included, and Actions secrets when another actor does; a token both use lives in both stores.

Detail: houserules get <id>
