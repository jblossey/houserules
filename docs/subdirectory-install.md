# A subdirectory install

A repository can hold the install in a subdirectory, for example a
service in a monorepo:

```sh
houserules init --dir services/api
```

- The install root is the directory you give. `.claude/` lives in the
  install root, so launch Claude Code there. `.githooks/` lives there too,
  where the install runs the reference workflow.
- Every other command works from anywhere inside the install. Its default
  root is the nearest directory at or above the current one that holds
  `.houserules.json`, no higher than the git top level.
- Every path a gate matches is relative to the install root, and a
  changed path outside it is out of scope. `check-commit` checks only the
  commits that touch the install root.
- A CI workflow that a 1.x release seeded stays at the git top level, as
  `.github/workflows/knowledge-<folder>.yml`, where `<folder>` is the
  install root's path with each `/` replaced by `-` (`knowledge-services-api.yml`
  here). It carries a `working-directory` and a `paths` filter for the
  subdirectory. A repository can hold several installs; `init` refuses a new
  root that nests with another install. A root that already holds its
  `.houserules.json` is no new root: `init` there runs no such check.
  `houserules ci print` and `ci write` give each install its own key, and
  they refuse two installs of one repository that share a key.
- houserules never sets `core.hooksPath`. Where the install runs the
  reference workflow, activate its commit-msg hook with `git config
  core.hooksPath services/api/.githooks`, or call it from your hook manager
  in one line: `sh services/api/.githooks/commit-msg "$1"`.
