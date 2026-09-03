# houserules runbook

Operational steps for maintaining this repository. Each section covers
one recurring task.

## After a release-please merge, restamp the kit version

release-please opens a PR that bumps `package.json`'s version. The merge
does not update `.houserules.json`. The stale stamp fails
`tests/dogfood.test.mjs` on main, because that test pins the stamp to
the running version.

After you merge a release-please PR, restamp the kit:

1. Run `mise exec -- node bin/houserules.mjs update --dir .`. The command
   prints a drift line, for example `kit 0.1.0 -> 0.2.0-alpha`.
2. Commit the restamped `.houserules.json`:
   `chore(release): restamp the kit version`.
3. Push the commit to main.

Run this step every time, right after the merge. A skipped restamp
breaks main until the next one.
