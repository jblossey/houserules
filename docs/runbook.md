# houserules runbook

Operational steps for maintaining this repository. Each section covers
one recurring task.

## release-please's release-type: rust at crates/houserules (HR-073)

`.github/workflows/release-please.yml` runs `googleapis/
release-please-action@v5.0.0` on every push to `main` (`on: push:
branches: [main]`) — not only when a release is being cut. That action
resolves release-please `^17.6.0`; `release-please-config.json` declares
the `v17.11.2` schema.

Through batch 20 T4, `packages["."].release-type` there still named
`"node"`, with no `package-name` override set. Verified against the
pinned `v17.11.2` source (not a config-string reading alone, per
process.wiring-checks-run-the-resolution): `BaseStrategy.
buildReleasePullRequest` calls `getBranchComponent()` to name the
release branch. Unlike its sibling `getComponent()`,
`getBranchComponent()` is NOT guarded by `includeComponentInTag` — it
always calls `getDefaultComponent()`, which falls through to
`this.packageName ?? getDefaultPackageName()` since no `package-name` is
configured. The Node strategy's `getDefaultPackageName()` reads
`package.json` through `getPkgJsonContents()`; when that file is
missing, `getPkgJsonContents()` catches the resulting `FileNotFoundError`
and throws `MissingRequiredFileError` naming `package.json`, `'node'`,
and this repository. Batch 20 T3 (HR-047) retired `package.json`, so the
next push to `main` after that batch merged — not a later release
attempt — would have run `buildReleasePullRequest` against a repository
with no `package.json` and thrown before any PR opened. HR-068 (the
tag-push token gap) does not shield this: that gap is about
`release.yml` never starting after a tag pushes, strictly later in the
pipeline than this failure.

**Closed pre-merge, batch 20 T5 (HR-073, ruled design.md §5.43).**
`packages["."]` moved to `packages["crates/houserules"]` with
`release-type: "rust"`. Re-derived against the pinned `v17.11.2` source,
not carried over from the `node` shape:

- **The crate's own `Cargo.toml` becomes the package manifest.**
  `Rust.getDefaultPackageName()` (`src/strategies/rust.ts:137-141`) reads
  it through `getPackageManifest()` (`:148-153`), which resolves
  `addPath('Cargo.toml')` against the package's own path — no
  `package.json` anywhere in the call path.
- **The tag stays plain.** `include-component-in-tag: false` short-
  circuits `getComponent()` to `''` before it ever calls
  `getDefaultComponent()` (`src/strategies/base.ts:178-183`), and
  `buildReleasePullRequest`'s `TagName` construction
  (`base.ts:299-304`) receives `undefined` for the component whenever
  `includeComponentInTag` is false — unaffected by which path the
  package config names. `dist plan --tag=v0.2.0-alpha` still exits 0
  (`.superpowers/sdd/2026-09-07-batch-20/t5-evidence/
  dist-plan-v0.2.0-alpha.log`).
- **`extra-files` needed a leading `/` on every entry.**
  `BaseStrategy.addPath` (`base.ts:770-784`) prefixes a non-absolute
  file with the package's own path unless that package sits at
  `ROOT_PROJECT_PATH` (`"."`) — true for every extra-file, string or
  object, since `extraFileUpdates` (`base.ts:411-520`) calls it
  unconditionally. Moving the package off root turned `README.md` and
  `template/.github/workflows/knowledge.yml` into paths under
  `crates/houserules/` that do not exist; `github.ts:850-861`'s
  `buildChangeSet` treats a missing `createIfMissing: false` file as a
  silent no-op, never a failure, so this class of break has no error to
  notice it by. Both entries now read `/README.md` and
  `/template/.github/workflows/knowledge.yml`. The redundant
  `crates/houserules/Cargo.toml` `toml`/`jsonpath` extra-file entry is
  gone too: the `Rust` strategy already emits that same update natively
  (`rust.ts:112-120`), and unprefixed it would have doubled the path
  the same way.
- **The changelog stays at the repository root.** Left unconfigured,
  the `Rust` strategy's changelog update targets `addPath(this.
  changelogPath)` (`rust.ts:37-44`, default `CHANGELOG.md`), which
  would create a second, empty changelog under `crates/houserules/`
  instead of continuing the root `CHANGELOG.md`'s real release history.
  `changelog-path: "/CHANGELOG.md"` keeps it at the existing file, via
  the same leading-`/` root anchor as the `extra-files` fix.
- **A real, permanent gap this ruling accepts, tracked as HR-081, not
  blessed as correct:** moving the package path off `"."` also moves
  commit attribution. `CommitSplit.split()` (`src/util/
  commit-split.ts:78-108`) only attributes a commit to a configured
  `packagePaths` entry when one of the commit's touched files sits
  under that path (`:99`); the special case that assigns every commit
  to a package regardless of what it touched applies only to
  `ROOT_PROJECT_PATH` itself (`commit-split.ts:63`,
  `manifest.ts:700-701`). From this release onward, release-please
  considers a commit toward `crates/houserules`'s next version only if
  it touches `crates/houserules/**`. That is not the same boundary as
  "shipped code": `install.rs:232-234`'s `#[derive(RustEmbed)]
  #[folder = "../../template/"]` compiles the whole repository-root
  `template/` tree into the released binary, so a commit that changes
  only `template/`, `README.md`, or another root-level path changes
  shipped code and now produces no release PR at all. Measured over
  `v0.2.0-alpha..6aa19e0` (batch 20 T5 fix round 1 review): 82 of 119
  commits touch no path under `crates/houserules/`, and 7 of 27
  `feat`/`fix` commits are unattributed under the new package path —
  four of them (`7460c77 feat(template)`, `6ac1aaa fix(template)`,
  `719d3f5 fix(tools)`, `101555e fix(template)`) change the embedded
  payload directly, enumerated by what each commit actually touches,
  not by subject scope — a subject-scoped read of this same range
  missed `719d3f5` (`fix(tools): validate rejects incomplete terminal
  reports`, no `crates/houserules/` path, two `template/` files)
  (`.superpowers/sdd/2026-09-07-batch-20/t5-evidence/
  hr081-template-payload-commits.sh`,
  `hr081-template-payload-commits.log`).
  Reversing the package path is not the fix: `release-type: rust` at
  `"."` would take the workspace branch and push a `CargoToml` update
  for the root manifest, which `cargo-toml.ts:37-40` throws on ("is not
  a package manifest") since this workspace's root `Cargo.toml` carries
  no `[package]` section. HR-081 tracks the gap and carries the
  candidate remedies without choosing one.
- **Schema conformance, checked locally against the pinned v17.11.2
  schema** (no `ajv-cli`/Node validator survives batch 20 T3's
  retirement of that toolchain):
  `.superpowers/sdd/2026-09-07-batch-20/t5-evidence/
  config-schema-conformance-check.py` and its captured
  `config-schema-conformance-check.log`.
- **`Cargo.lock`'s own recorded version for the local package needs no
  extra-files entry.** The `Rust` strategy already tries to bump it
  (`rust.ts:123-126`), but at `addPath('Cargo.lock')` under this
  package's own path — `crates/houserules/Cargo.lock`, which does not
  exist, since this repository's real `Cargo.lock` sits at the
  workspace root. That update silently no-ops the same way the
  `extra-files` entries above used to. Left as is: any subsequent
  `cargo build`/`check`/`test` invocation resolves the local package's
  entry in `Cargo.lock` from `Cargo.toml` directly and rewrites it,
  verified live by bumping the crate's version, running `cargo check`,
  and observing exactly that line change, then reverting both files
  (`.superpowers/sdd/2026-09-07-batch-20/t5-evidence/
  cargo-check-version-bump-self-heal.log`,
  `cargo-lock-after-self-heal.diff`). That rewrite needs no `--locked`
  gate to object: a bounded, rerunnable sweep
  (`.superpowers/sdd/2026-09-07-batch-20/t5-evidence/
  ci-lock-flag-sweep.sh`, `ci-lock-flag-sweep.log`) finds zero
  `--locked`/`--frozen`/`--offline` flags across this repository's own
  CI surface (`.github/`, `mise.toml`, `dist-workspace.toml`) and
  across cargo-dist v0.32.0's own build path (`cargo-dist/src/build/
  cargo.rs`, the function that constructs the `cargo build`/`check`
  command `dist build` runs, plus its sibling files in that module) —
  the only `--locked` in cargo-dist v0.32.0's Rust source is the
  from-git `cargo install ... --locked cargo-dist` command, emitted
  twice by `DistInstallStrategy::GitBranch`'s `dash`/`powershell`
  methods (`backend/ci/mod.rs:131`, `:143`) — a strategy this
  repository's config never selects (`release.yml`'s own "Install
  dist" step uses the plain curl/irm `Installer` strategy instead).
  `CARGO_PKG_VERSION` and `dist`'s own package-version resolution read
  `Cargo.toml` directly and never consult `Cargo.lock` for a local
  workspace member, so a stale line there is cosmetic to those three
  verified consumers specifically — not a general claim: a `--locked`
  consumer would fail outright, reproduced live by bumping the crate's
  version in place and reverting
  (`.superpowers/sdd/2026-09-07-batch-20/t5-evidence/
  cargo-check-locked-fails.log`): `cargo check --locked` errors with
  "cannot update the lock file ... because --locked was passed". No
  consumer in this repository's own
  pipeline runs `--locked` today, so this is a latent residue, not an
  active break, but HR-073's own tick is not its owner: HR-082 tracks
  it.

**What stays unproven until the first live release (queued at HR-068
step two, the two-step reality in docs/specs/
2026-09-06-batch-19-phase4.md §7):** every check above is a schema or
source-code proof, not a release-please run. Whether `Rust.
buildUpdates()` actually opens a mergeable PR against `main` with this
exact config — the version bump, the changelog entry, every extra-file
edit landing together — needs release-please's own GitHub App
credentials and a real repository, neither available in this checkout.
That first run is the live proof this section's local verification
cannot substitute for.

## Cutting a release

The pipeline (docs/specs/2026-09-06-batch-19-phase4.md §2, T1): a plain
`v<version>` tag drives cargo-dist's generated
`.github/workflows/release.yml`. `dist-workspace.toml` is that file's
source (`houserules.release-workflow-is-generated`) — edit the config
and run `dist generate`; never hand-edit the generated workflow. One
release runs:

1. **Merge the release-please PR.** release-please keeps a PR open
   against `main` that bumps `crates/houserules/Cargo.toml`'s version
   (the `rust` release-type's own native update, HR-073) and every
   pinned install URL in README.md and the seeded `template/.github/
   workflows/knowledge.yml`'s own install step (the
   `x-release-please-version`/`-start-version`/`-end` annotations;
   HR-049's measured mechanism, `.superpowers/sdd/2026-09-06-batch-19/
   t2-evidence/generic-updater-check-green.log` and
   `generic-updater-check-knowledge-yml-green.log`). Merging this PR is
   the release trigger, and it lands the seeded workflow's own pin
   already current for the release it is about to trigger. This step
   presupposes the release-type fix above landed already (HR-073,
   closed on this branch pre-merge); release-please could not open this
   PR at all otherwise. Whether it actually does, with this exact
   config, is this release's own live proof — the section above names
   what stays unverified until it runs.
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
   (docs/specs/2026-09-06-batch-19-phase4.md §7). HR-073's own live
   proof lands earlier than these three, on this procedure's first run
   of step 1 above: a release-please PR opening under the `rust`
   release-type with no `MissingRequiredFileError` or other config-shape
   failure is the run the section above's local schema and source checks
   could not substitute for. Once the host/create/announce jobs succeed
   against the resulting release:
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
