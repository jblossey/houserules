# houserules runbook

Operational steps for maintaining this repository. Each section covers
one recurring task.

## release-please configuration (crates/houserules)

`.github/workflows/release-please.yml` runs
`googleapis/release-please-action@v5.0.0` on every push to `main`, not
only when a
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
since-rejected reversal at §5.71). The one `extra-files` entry,
`/.houserules.json`'s `json`/`$.version` update, anchors to the
repository root with a leading `/`, since the package sits off root; it
folds this repository's own kit-version restamp into the release PR
itself, so the PR's own `crates/houserules/Cargo.toml` bump and
`.houserules.json`'s `version` field land in the same commit (see
`houserules.post-release-restamp` below). `changelog-path:
"/CHANGELOG.md"` keeps release notes in the existing
root changelog instead of starting a second one under
`crates/houserules/`. The `cargo-workspace` plugin keeps the
workspace-root `Cargo.lock` in sync; a residue in the package's own
recorded lock entry is not cosmetic, since this repository's own CI
passes `--locked` (`.github/workflows/ci.yml:45-46`,
`mise.toml:15,26`): a stale line fails `cargo test --locked`/`cargo
check --locked` outright on `main`, before anything gets a chance to
self-heal it. `cargo-dist`'s own release build passes no such flag, so
the release itself still succeeds either way; HR-082 tracks closing the
CI-facing gap.
`crates/houserules/tests/release_please_config.rs` pins
every one of these settings against the
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
release-please's own release commit touches no path under `template/`:
the seeded `template/.github/workflows/knowledge.yml` installer pins
`releases/latest/download`, needing no per-release rewrite, so
`extra-files` carries no entry there (branch review batch 24, issue 1) —
this gate and `check-commit`'s co-change enforcement need no bot-commit
carve-out.

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
   own native update), appending to the root `CHANGELOG.md`, and
   rewriting `.houserules.json`'s `version` field to match (the
   `extra-files` entry, above) — this repository's own kit-version
   restamp rides the same PR as the adopter-facing bump, so the PR
   itself carries a matching `Cargo.toml`/`.houserules.json` pair. The
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
   (`houserules.default-token-tags-start-no-workflows`). If the release
   PR's own checks sit at `action_required` instead of running, approve
   the runs once from the PR's checks tab; this first cut is
   owner-attended either way.

   **Do not delete `refs/tags/v0.2.0-alpha` before `v0.3.0` exists.**
   `backfillReleasesFromTags` (release-please's own tag-based release
   lookup, run when no matching GitHub Release object is found) resolves
   the plain `v0.2.0-alpha` tag as this repository's current release
   anchor under `include-component-in-tag: false`; deleting it removes
   that anchor. `release-please-config.json`'s own `bootstrap-sha` gives
   release-please a stopping point if the anchor is gone, so a deletion
   would not stall the pipeline or walk the whole repository history —
   but the resulting commit window would not match today's
   tag-anchored one exactly (verified live: the configured
   `bootstrap-sha` sits several commits earlier than the tag). `v0.3.0`
   shipped 2026-09-13 and is the anchor now; the `v0.2.0-alpha` remains
   stay as history per the owner's ruling (design.md 5.67).
4. **The tag builds and uploads into the release step 3 already made.**
   `release.yml` builds five targets (linux x64/arm64 musl, macOS
   x64/arm64, Windows x64), a sha256 checksum per archive plus a unified
   `sha256.sum`, and the shell and PowerShell installers. It then uploads
   all of it into the tag's GitHub Release with `gh release upload`, then
   undrafts the release with `gh release edit --draft=false` (a no-op:
   step 3's release already publishes, never drafts).
   `dist-workspace.toml`'s `create-release = false` is what makes this an
   upload instead of a second creation.

   WARNING: v0.3.0's own first live cut ran before this setting existed.
   Its host job called `gh release create` against the tag
   release-please had already released, and collided ("a release with
   the same tag name already exists", HR-118, design.md 5.77).

   Every release from the one that carries `create-release = false`
   onward uploads into release-please's release instead of fighting it
   for creation. `0.3.0` carries no prerelease suffix, so the release is
   not marked a prerelease — the condition every documented install
   path's `releases/latest/download` alias needs, to resolve to it
   (docs/specs/2026-09-12-batch-24-repo-and-setup.md §2a).
5. **Check for baseline drift, not a version restamp.** The release PR
   (step 1) already carried `.houserules.json`'s `version` field to
   `0.3.0`, so `crates/houserules/tests/dogfood.rs`'s
   `houserules_json_stamps_the_installed_version_and_the_hr_id_prefix`
   test passes on the release PR itself, before merge. What can still
   drift is `baselines`: the per-file hashes `houserules update --dir .`
   stamps for every `KIT_OWNED` file and kit-shipped knowledge entry.
   Follow the restamp procedure below to check and, if needed, refresh
   them — referenced here, not repeated.
6. **Every mechanism above is live-proven; nothing remains due.**
   Before `v0.3.0`, the proofs were the pinned release-please source,
   `release_please_config.rs`'s config-shape pins (its module doc:
   "this file pins only the config shape ... with no release-please
   run"), and T1's commit-split and package-shape simulations in the
   batch workspace
   (`.superpowers/sdd/2026-09-12-batch-24/t1-evidence/`). The `v0.3.0`
   cut supplied the real runs (batch 24, 2026-09-13): the
   host/create/announce jobs succeeded; the README install-block
   re-walk (the shell installer, mise, and direct-download channels)
   ran live against the real, now-`latest` release (HR-064, HR-070);
   and a seeded-repository CI run closed the loop — a fresh
   `houserules init` repository's PR gate installed the binary through
   the latest-alias installer inside the runner and passed end to end
   (HR-063, captures in the batch workspace's `t3b-evidence/`).

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

## After a release-please merge, check for baseline drift

release-please's PR bumps `crates/houserules/Cargo.toml`'s version (the
`rust` release-type's own native update, HR-073) and, in the same PR,
rewrites `.houserules.json`'s `version` field to match (the
`/.houserules.json` `extra-files` entry). The version half of the restamp
therefore already rides the PR and needs no post-merge step;
`crates/houserules/tests/dogfood.rs`'s
`houserules_json_stamps_the_installed_version_and_the_hr_id_prefix` test
passes on the release PR itself, before merge, by construction.

What the PR does NOT update is `baselines`: the per-file hashes
`houserules update --dir .` stamps for every `KIT_OWNED` file and
kit-shipped knowledge entry. Those drift only if the kit's own payload
changed since the last stamp, independent of the version bump.

After you merge a release-please PR, check for that drift:

1. Run `houserules update --dir .`. The command prints a drift line, for
   example `kit 0.3.0 -> 0.3.0`; a version drift here would mean the
   extra-files entry above did not fire and needs investigating.
2. If `.houserules.json` changed (a `baselines` hash, ordinarily),
   commit it: `chore(release): restamp the kit baselines`.
3. Push the commit to main.

An unstamped baseline drift does not fail `dogfood.rs`'s version test
(that test pins only `version` and `idPrefix`), but it does leave
`.houserules.json` stale against the tree's real kit-owned content until
the next `update` run notices it.
