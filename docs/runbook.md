# houserules runbook

Operational steps for maintaining this repository. Each section covers
one recurring task.

## release-please configuration (crates/houserules)

`.github/workflows/release-please.yml` runs `googleapis/
release-please-action@v5.0.0` on every push to `main`, not only when a
release is being cut, and authenticates with the `RELEASE_PLEASE_TOKEN`
repository secret (a fine-grained PAT scoped to Contents, Pull requests,
and Issues, each Read and write, this repository only), falling back to
the default `GITHUB_TOKEN` when that secret is absent. The PAT is what
lets the merge in "Cutting a release" below actually start the release:
a tag pushed by the default `GITHUB_TOKEN` starts no workflow
(`houserules.default-token-tags-start-no-workflows`).

`release-please-config.json` configures one package, `crates/houserules`
(`release-type: "rust"`, reading that crate's own `Cargo.toml` as the
version source). `include-component-in-tag: false` keeps every release
tag plain (`v<version>`, docs/design.md §5.20, reaffirmed §5.72 after a
since-rejected reversal at §5.71). Every `extra-files` entry anchors to
the repository root with a leading `/`, since the package sits off root.
`changelog-path: "/CHANGELOG.md"` keeps release notes in the existing
root changelog instead of starting a second one under
`crates/houserules/`. The `cargo-workspace` plugin keeps the
workspace-root `Cargo.lock` in sync; a residue in the package's own
recorded lock entry is not cosmetic, since this repository's own CI
passes `--locked` (`.github/workflows/ci.yml:45-46`,
`mise.toml:15,26`): a stale line fails `cargo test --locked`/`cargo
check --locked` outright on `main`, before anything gets a chance to
self-heal it. `cargo-dist`'s own release build passes no such flag, so
the release itself still succeeds either way; HR-082 tracks closing the
CI-facing gap. `crates/houserules/tests/
release_please_config.rs` pins every one of these settings against the
pinned release-please source and fails loudly if any of them regresses.

A commit touching only `template/` touches no path under
`crates/houserules/**`, so release-please's own commit-to-package
attribution would otherwise see no reason to release it — even though
`template/` compiles into the shipped binary
(`houserules.payload-embeds-checkout-bytes`) and is therefore a real
product change (HR-081). `houserules.payload-stamp-gate` closes that gap
with no release-please plugin surface: `crates/houserules/payload.stamp`
records a digest of `template/`'s tracked files, `mise run lint` fails
whenever the working tree disagrees with it, and `houserules
check-commit` enforces the same pairing commit by commit in CI. After
changing anything under `template/`, run `cargo run --quiet --bin
payload-stamp-gate -- --write` and commit the regenerated
`payload.stamp` in the same commit as the `template/` change.

## Cutting a release

The pipeline (docs/specs/2026-09-12-batch-24-repo-and-setup.md §2a, T1):
release-please proposes the version from conventional commits, merging
its PR pushes a plain `v<version>` tag, and that tag drives cargo-dist's
generated `.github/workflows/release.yml`. `dist-workspace.toml` is that
file's source (`houserules.release-workflow-is-generated`) — edit the
config and run `dist generate`; never hand-edit the generated workflow.
One release runs:

1. **release-please keeps a PR open against `main`**, bumping
   `crates/houserules/Cargo.toml`'s version (the `rust` release-type's
   own native update) and appending to the root `CHANGELOG.md`. The
   `RELEASE_PLEASE_TOKEN` PAT (release-please configuration, above)
   authenticates the workflow that keeps this PR current.
2. **This release's version is pinned by a `Release-As: 0.3.0` commit
   footer**, already on this branch (a `chore(release): re-aim the next
   release at 0.3.0` commit) — release-please's own one-shot re-aim,
   scoped to the commit that carries it. A persistent `release-as`
   config key would keep proposing the same version after every later
   merge, a trap release-please's own manifest-releaser docs name
   explicitly; the config carries no such key, so a future release needs
   no config edit to move past 0.3.0.
3. **Merge the release PR.** The merge pushes to `main`, which starts
   `release-please-action` again; that second run is the one that
   creates the plain `v0.3.0` tag (`include-component-in-tag: false`)
   and the GitHub Release, authenticated by the `RELEASE_PLEASE_TOKEN`
   PAT — that PAT authorship, not the merge itself, is what lets the
   tag start `release.yml`; a tag the default `GITHUB_TOKEN` created
   would start no workflow
   (`houserules.default-token-tags-start-no-workflows`).

   **Do not delete `refs/tags/v0.2.0-alpha` before `v0.3.0` exists.**
   `backfillReleasesFromTags` (release-please's own tag-based release
   lookup, run when no matching GitHub Release object is found) resolves
   the plain `v0.2.0-alpha` tag as this repository's current release
   anchor under `include-component-in-tag: false`; deleting it removes
   that anchor. `release-please-config.json`'s own `bootstrap-sha` gives
   release-please a stopping point if the anchor is gone, so a deletion
   would not stall the pipeline or walk the whole repository history —
   but the resulting commit window would not match today's
   tag-anchored one exactly (verified live: the configured `bootstrap-
   sha` sits several commits earlier than the tag). Keep the tag until
   `v0.3.0` ships and becomes the new anchor in its place.
4. **The tag builds and publishes the release.** `release.yml` builds
   five targets (linux x64/arm64 musl, macOS x64/arm64, Windows x64), a
   sha256 checksum per archive plus a unified `sha256.sum`, and the
   shell and PowerShell installers, then uploads all of it to the tag's
   GitHub Release. `0.3.0` carries no prerelease suffix, so the release
   is not marked a prerelease — the condition every documented install
   path's `releases/latest/download` alias needs, to resolve to it
   (docs/specs/2026-09-12-batch-24-repo-and-setup.md §2a).
5. **Restamp this repository's own kit version.** Separate from the
   adopter-facing release above: this repository runs its own kit
   (`houserules.template-is-the-source`), so its own `.houserules.json`
   stamp also needs the new version. Follow the restamp procedure below
   — referenced here, not repeated.
6. **Two acts fall due once this run succeeds for real (HR-063):** every
   mechanism above is proven from the pinned release-please source and
   `release_please_config.rs`'s own config-shape pins (its module doc:
   "this file pins only the config shape ... with no release-please
   run"), plus T1's own commit-split and package-shape simulations in
   the batch workspace
   (`.superpowers/sdd/2026-09-12-batch-24/t1-evidence/`) — not from an
   actual release-please run against this exact config. This
   repository's own `0.3.0` cut is that missing run. Once the
   host/create/announce jobs succeed against the resulting release:
   - **The README install-block re-walk**: re-run the shell installer,
     mise, and direct-download blocks against the real, now-`latest`
     release and replace each PENDING note with the result (HR-064,
     HR-070).
   - **A seeded-repository CI run**: seed a fresh repository with
     `houserules init`, open a PR against it, and confirm the install
     step actually installs the binary and the gate passes end to end
     (HR-063).

### Owner-attended external acts

Each of these publishes houserules to a channel that needs a human with
write access to a registry or tap this repository does not own; none of
them run automatically. Do each once release assets exist (step 4
above), then again only when the registration itself needs an update
(most track new releases on their own once set up).

| Act | Needs | Where |
|---|---|---|
| mise registry short-name PR | A PR to `jdx/mise` adding `houserules -> github:jblossey/houserules` to `registry.toml` (mise's `ubi` backend is deprecated in this backend's favor, HR-070), so `mise use houserules` works with no backend prefix | github.com/jdx/mise |
| Homebrew tap | A `jblossey/homebrew-houserules` tap repository carrying a formula (cargo-dist can generate one; not enabled this batch — `dist-workspace.toml`'s `installers` carries only `shell`/`powershell`) | A new repository under the `jblossey` account |
| asdf plugin | An `asdf-houserules` plugin repository implementing asdf's plugin API against the same release archives | A new repository under the `jblossey` account |
| Scoop / winget | A Scoop manifest in a bucket repository, and/or a winget manifest PR against `microsoft/winget-pkgs`, both pointing at the Windows archive | A Scoop bucket repository; a PR to `microsoft/winget-pkgs` |

## After a release-please merge, restamp the kit version

release-please opens a PR that bumps `crates/houserules/Cargo.toml`'s
version (the `rust` release-type's own native update, HR-073).
The merge does not update `.houserules.json`. The stale stamp fails
`crates/houserules/tests/dogfood.rs`'s `houserules_json_stamps_the_
installed_version_and_the_hr_id_prefix` test on main, because that test
pins the stamp to the running version (`CARGO_PKG_VERSION`).

After you merge a release-please PR, restamp the kit:

1. Run `houserules update --dir .`. The command prints a drift line, for
   example `kit 0.1.0 -> 0.2.0-alpha`.
2. Commit the restamped `.houserules.json`:
   `chore(release): restamp the kit version`.
3. Push the commit to main.

Run this step every time, right after the merge. A skipped restamp
breaks main until the next one.
