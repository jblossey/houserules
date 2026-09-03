# Batch 12: the release hotfix

Date: 2026-09-03. Status: approved in outline at the hotfix gate,
2026-09-03 (HR-044 alone; restamp + regex widening + the
release-runbook decision in this spec). Driver: HR-044. Branch
`batch-12` from `9d0bb10`. Main is RED; this batch turns it green.
Process: implementer on sonnet, task review on opus, branch review on
fable over the one-task diff; the audit `--ids` are the dispatch's
Knowledge ids. Workspace `.superpowers/sdd/2026-09-03-batch-12/`.

## 1. Goal

Main green after the first release cycle: the dogfood stamp follows
the released version, the init test accepts pre-release versions, and
the post-release restamp step is documented so the next release does
not repeat this.

## 2. Out of scope

- Any npm publish machinery; any change to release-please config or
  the workflow; HR-045 (the OSS files batch).
- No CI softening: the dogfood stamp check stays exact.

## 3. Facts

- The 0.2.0-alpha release commit (bd2b754) bumped package.json;
  nobody ran `update --dir .`, so root `.houserules.json` still says
  0.1.0 and tests/dogfood.test.mjs fails (stamp must equal VERSION).
- tests/init.test.mjs asserts the stamped version matches
  `/^\d+\.\d+\.\d+$/` — pre-release-blind, wrong under design.md
  §5.19 (alpha versions are the norm).
- CI runs 33789904843 and 33782255156 carry the failures.

## 4. The task, one to two Conventional Commits

### T1 `fix(tests): the first release cycle turns main green` (HR-044)

- Run `mise exec -- node bin/houserules.mjs update --dir .`: the
  restamp is the drift line's first production use — capture
  `kit 0.1.0 -> 0.2.0-alpha` verbatim as live evidence. Commit the
  restamped `.houserules.json` (plus any synced kit-owned drift the
  run reveals; expected none).
- Widen the init test's version assertion to full semver with an
  optional pre-release suffix (e.g.
  `/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/`); RED first per the timing
  key against the current stamp state where a natural capture exists,
  else disclosed mutation.
- RULING (this spec): the post-release restamp is a documented
  maintainer step, not a CI tolerance. Create `docs/runbook.md` (the
  repository's real one) with one section: after merging a
  release-please PR, run `update --dir .`, commit
  `chore(release): restamp the kit version`, push. STE-100.
- Full suite green is the acceptance; both check gates; lint.

## 5. Reviews and finish

Task review (opus), branch review (fable) with `--workspace`; one to
two clean commits (no aggregation needed at this size); ff-only merge
from the CLI; push to origin; verify CI green on main.
