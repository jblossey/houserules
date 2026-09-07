# houserules runbook

Operational steps for maintaining this repository. Each section covers
one recurring task.

## Cutting a release

The pipeline (docs/specs/2026-09-06-batch-19-phase4.md §2, T1): a plain
`v<version>` tag drives cargo-dist's generated
`.github/workflows/release.yml`. `dist-workspace.toml` is that file's
source (`houserules.release-workflow-is-generated`) — edit the config
and run `dist generate`; never hand-edit the generated workflow. One
release runs:

1. **Merge the release-please PR.** release-please keeps a PR open
   against `main` that bumps `package.json`'s version,
   `crates/houserules/Cargo.toml`'s version in lockstep (the `toml`
   `extra-files` entry), and — from this batch — every pinned install
   URL in README.md and the seeded `template/.github/workflows/
   knowledge.yml`'s own install step (the `x-release-please-version`/
   `-start-version`/`-end` annotations; HR-049's measured mechanism,
   `.superpowers/sdd/2026-09-06-batch-19/t2-evidence/
   generic-updater-check-green.log` and
   `generic-updater-check-knowledge-yml-green.log`). Merging this PR is
   the release trigger, and it lands the seeded workflow's own pin
   already current for the release it is about to trigger.
2. **Meet HR-068 before merging, not after.** `release-please-action`
   tags and releases on the default `GITHUB_TOKEN`, and GitHub does not
   start a workflow from a tag that token pushes
   (`houserules.default-token-tags-start-no-workflows`) — `release.yml`
   never runs unless this is resolved first. HR-068 names the choice: a
   PAT or GitHub App installation token on `release-please-action`'s
   `token:` input, or a `github-script` step that calls
   `workflow_dispatch` on `release.yml` directly. OWNER-ATTENDED: a
   secrets-management and workflow-ownership decision, not a technical
   unknown — meet it here, before merging step 1's PR for real.
3. **The tag fires the pipeline.** The merge pushes the plain
   `v<version>` tag (`include-component-in-tag: false`, docs/design.md
   §5.20); with HR-068 resolved, that tag starts `release.yml`: five
   target builds (linux x64/arm64 musl, macOS x64/arm64, Windows x64), a
   sha256 checksum per archive plus a unified `sha256.sum`, the shell
   and powershell installers, all uploaded to the GitHub Release the tag
   names.
4. **The pins are already current.** Step 1's merge carried the new
   README and knowledge.yml pins in the same commit; nothing further to
   do for them here. The seeded workflow ships each future release
   already pointing at itself, since the embed is compile-time
   (`houserules.template-is-the-source`) and this file updates before
   the tag that triggers that release's own build.
5. **Restamp this repository's own kit version.** Separate from the
   adopter-facing pins above: this repository runs its own kit
   (`houserules.template-is-the-source`), so its own `.houserules.json`
   stamp also needs the new version. Follow the restamp procedure below
   — referenced here, not repeated.
6. **Three acts fall due once the pipeline is proven.** The first real
   release with assets is step two of the spec's two-step reality
   (docs/specs/2026-09-06-batch-19-phase4.md §7). Once the host/create/
   announce jobs succeed against it:
   - **HR-069's revert**: `pr-run-mode` back to `"plan"` in
     `dist-workspace.toml`, dropping the `swatinem/rust-cache@v2` step
     it drags in, at the next config-touching task.
   - **The README install-block re-walk**: re-run the shell installer,
     mise, and direct-download blocks against the real release and
     replace each PENDING note with a real result (HR-064/HR-049).
   - **A seeded-repository CI run**: seed a fresh repository with
     `houserules init`, open a PR against it, and confirm the install
     step actually installs the binary and the gate passes end to end
     (HR-063).

### Owner-attended external acts

Each of these publishes houserules to a channel that needs a human with
write access to a registry or tap this repository does not own; none of
them run automatically. Do each once release assets exist (step 3
above), then again only when the registration itself needs an update
(most track new releases on their own once set up).

| Act | Needs | Where |
|---|---|---|
| mise registry short-name PR | A PR to `jdx/mise` adding `houserules -> ubi:jblossey/houserules` (or `github:jblossey/houserules`, pending HR-070) to `registry.toml`, so `mise use houserules` works with no backend prefix | github.com/jdx/mise |
| Homebrew tap | A `jblossey/homebrew-houserules` tap repository carrying a formula (cargo-dist can generate one; not enabled this batch — `dist-workspace.toml`'s `installers` carries only `shell`/`powershell`) | A new repository under the `jblossey` account |
| asdf plugin | An `asdf-houserules` plugin repository implementing asdf's plugin API against the same release archives | A new repository under the `jblossey` account |
| Scoop / winget | A Scoop manifest in a bucket repository, and/or a winget manifest PR against `microsoft/winget-pkgs`, both pointing at the Windows archive | A Scoop bucket repository; a PR to `microsoft/winget-pkgs` |

## After a release-please merge, restamp the kit version

release-please opens a PR that bumps `package.json`'s version. The merge
does not update `.houserules.json`. The stale stamp fails
`tests/dogfood.test.mjs` on main, because that test pins the stamp to
the running version.

After you merge a release-please PR, restamp the kit:

1. Run `houserules update --dir .`. The command prints a drift line, for
   example `kit 0.1.0 -> 0.2.0-alpha`.
2. Commit the restamped `.houserules.json`:
   `chore(release): restamp the kit version`.
3. Push the commit to main.

Run this step every time, right after the merge. A skipped restamp
breaks main until the next one.
