//! Pins `release-please-config.json`'s package shape: the one package
//! sits at `"crates/houserules"` (`release-type: "rust"`).
//! `Rust.getDefaultPackageName` (`src/strategies/rust.ts:137-141`) reads
//! that crate's own `Cargo.toml` through `getPackageManifest()`
//! (`:148-153`), which resolves `addPath('Cargo.toml')` against the
//! package's own path -- no `package.json` anywhere in the call path.
//! This file pins only the config shape that derivation depends on,
//! cheaply, with no release-please run.
//!
//! Every source line this file cites is v17.6.0, the release-please
//! version `.github/workflows/release-please.yml`'s pinned action
//! bundles (its `dist/index.js` reports `exports.VERSION = '17.6.0'`).
//! `schema_names_the_bundled_release_please_version` cross-checks the
//! config's `$schema` against that workflow's own pinned action sha so
//! the two cannot drift apart silently.
//!
//! `include-component-in-tag` stays `false`: the owner ruled the plain
//! `v<version>` tag shape (design.md 5.20) and reaffirmed it after a
//! since-rejected reversal (design.md 5.72). `BaseStrategy` defaults it
//! to `true` (`base.ts:152`), and losing it would not fail loudly:
//! `getComponent` (`base.ts:178-183`) would then return the package's
//! own component instead of `''`, so `buildReleasePullRequest`
//! (`base.ts:299-304`) would tag the next release `houserules-v<version>`
//! instead of the plain `v<version>` every release since
//! `houserules-v0.2.0-alpha` has used. GitHub's own tag-filter glob
//! matches either shape (`**` absorbs any prefix), so this would not
//! surface as a CI failure either.
//!
//! `include-component-in-tag: false` is exactly the constraint under
//! which `getComponent()` always returns `''`
//! (`src/strategies/base.ts:178-183`), which is why HR-081 (a
//! `template/**`-only commit still bumping `crates/houserules`) cannot
//! be reached through `linked-versions` or any other component-matching
//! plugin: every strategy's component is the same empty string, so
//! nothing distinguishes one package from another. HR-081 is instead
//! reached with no release-please plugin surface at all: `crates/
//! houserules/payload.stamp` records a digest of `template/`'s tracked
//! file state, `cargo run --bin payload-stamp-gate` (wired into `mise
//! run lint`, `crates/houserules/src/bin/payload-stamp-gate.rs`) fails
//! whenever `template/` and the stamp disagree, and `--write`
//! regenerates it. A contributor who changes `template/` without
//! regenerating the stamp fails that gate before the change can land, so
//! every human- or agent-authored commit that touches `template/**` also
//! touches `crates/houserules/payload.stamp` by the time it reaches
//! `main` -- the one path `CommitSplit`'s own, unmodified prefix match
//! (`src/util/commit-split.ts`) already attributes to `crates/
//! houserules`. `release-please`'s own generated commits touch no path
//! under `template/**` at all: the seeded `template/.github/workflows/
//! knowledge.yml` installer now pins `releases/latest/download`, with no
//! `extra-files` entry left under `template/` for a release commit to
//! rewrite (branch review, batch 24, issue 1) -- this invariant needs no
//! bot-commit carve-out.
//!
//! `cargo-workspace` (`src/plugins/cargo-workspace.ts:345-350`) writes
//! the workspace-root `Cargo.lock` directly (`path: 'Cargo.lock'`,
//! unscoped by any package's own path), unlike `Rust.buildUpdates`'s
//! built-in lock update (`rust.ts:124-128`), which resolves under the
//! package's own path and silently no-ops for a crate that sits under a
//! workspace root (HR-082).
//!
//! `BaseStrategy.addPath` (`src/strategies/base.ts`) prefixes every
//! non-absolute extra-file path with the package's own path unless that
//! package sits at the repository root (`ROOT_PROJECT_PATH`, `"."`) or
//! the path itself starts with `/`. With the package off root, an
//! extra-file path lacking a leading `/` resolves under
//! `crates/houserules/` instead of the repository root -- a silent no-op
//! release-please only logs, never fails on (`github.ts`'s
//! `buildChangeSet` continues past a missing `createIfMissing: false`
//! file). The extra-files test below pins the leading-`/` requirement,
//! and two more tests pin the settings whose own loss would be just as
//! silent: `changelog-path` (would fork a second, empty changelog, not
//! fail) and the prerelease keys (design.md 5.67: `-alpha` drops from
//! 0.3.0 onward, since a prerelease version is excluded from GitHub's
//! `releases/latest` alias every documented install path uses, spec 2a).

use std::fs;
use std::path::{Path, PathBuf};

/// This checkout's repository root, resolved at compile time -- this
/// file's own copy of the pattern every `tests/*.rs` file in this crate
/// keeps independently (`install.rs`'s own doc explains why: a file
/// needing none of `mod common;`'s other helpers still warns the rest of
/// that module dead if pulled in just for this one function).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Parses `release-please-config.json` at the repository root.
fn config() -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(repo_root().join("release-please-config.json"))
            .expect("read release-please-config.json"),
    )
    .expect("parse release-please-config.json")
}

/// Parses `.release-please-manifest.json` at the repository root.
fn manifest() -> serde_json::Value {
    serde_json::from_str(
        &fs::read_to_string(repo_root().join(".release-please-manifest.json"))
            .expect("read .release-please-manifest.json"),
    )
    .expect("parse .release-please-manifest.json")
}

/// The manifest's one tracked path must equal the config's one package
/// path (`src/manifest.ts`'s `parseReleasedVersions` indexes released
/// versions by that same path) -- both name `crates/houserules`, the
/// crate whose `Cargo.toml` is the version source.
#[test]
fn manifest_key_matches_the_configs_package_path() {
    let manifest = manifest();
    let manifest_keys: Vec<&str> = manifest
        .as_object()
        .expect("manifest is a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(manifest_keys, ["crates/houserules"]);

    let config = config();
    let package_keys: Vec<&str> = config["packages"]
        .as_object()
        .expect("config.packages is a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(package_keys, ["crates/houserules"]);
}

/// The one package names the `rust` release-type (`src/factory.ts` maps
/// it to the `Rust` strategy) -- not `node`, which reads a `package.json`
/// absent from this repository.
#[test]
fn package_release_type_is_rust() {
    let config = config();
    assert_eq!(
        config["packages"]["crates/houserules"]["release-type"],
        "rust"
    );
}

/// `cargo-workspace`'s `postProcessCandidates` writes the workspace-root
/// `Cargo.lock` (`path: 'Cargo.lock'`, unscoped by any package's own
/// path), falling back to the one `rust`-typed candidate when no root
/// (`"."`) package exists (`src/plugins/cargo-workspace.ts:335-343`) --
/// exactly this repository's shape.
#[test]
fn cargo_workspace_plugin_fixes_the_lockfile() {
    let config = config();
    let plugins = config["plugins"].as_array().expect("plugins is an array");
    assert!(
        plugins
            .iter()
            .any(|plugin| plugin["type"] == "cargo-workspace"),
        "no cargo-workspace plugin entry: {plugins:?}"
    );
}

/// The config never carries a persistent `release-as`. `src/
/// commit.ts:262-292` records a `Release-As:` commit footer as a note
/// titled `RELEASE AS`, and `DefaultVersioningStrategy.
/// determineReleaseType` (`src/versioning-strategies/default.ts:74-82`)
/// returns that version outright for any commit carrying it -- a
/// one-shot override scoped to the commits that carry it, not a standing
/// config value. manifest-releaser.md:166-171 (fetched live) names the
/// config-key alternative's own trap: "once the release PR is merged you
/// should either remove [`release-as`] or update it to a higher
/// version. Otherwise subsequent `manifest-pr` runs will continue to use
/// this version". This repository's 0.3.0 re-aim (design.md 5.67) rides
/// a `chore(release): re-aim the next release at 0.3.0` commit's
/// `Release-As: 0.3.0` footer instead: it is an empty commit, and
/// `manifest.ts` always constructs `CommitSplit` with `includeEmpty:
/// true` (`manifest.ts:687-688`), which places a file-less commit onto
/// every configured package path (`commit-split.ts:109-118`) -- so
/// `crates/houserules`'s own commit scan sees it even though the commit
/// touches no file under `crates/houserules/`.
#[test]
fn release_as_is_absent_the_reaim_rides_a_commit_footer_instead() {
    let config = config();
    assert!(
        config.get("release-as").is_none(),
        "a persistent release-as would keep proposing 0.3.0 forever, per \
         manifest-releaser.md:166-171 -- remove it or update it after every merge"
    );
}

/// Neither the config root nor the one package pins a prerelease suffix
/// any more (design.md 5.67: `-alpha` drops from 0.3.0 onward). The
/// three keys are valid at the config root too (the v17.6.0 schema lists
/// all three as root properties) and a root value is inherited by the
/// package, so a root-level `"prerelease": true` would pin the suffix
/// while a package-only check stayed green.
#[test]
fn no_package_pins_a_prerelease_suffix() {
    let config = config();
    for key in ["versioning", "prerelease", "prerelease-type"] {
        assert!(
            config.get(key).is_none(),
            "the config root still sets {key:?}"
        );
        assert!(
            config["packages"]["crates/houserules"].get(key).is_none(),
            "crates/houserules still sets {key:?}"
        );
    }
}

/// The root `include-component-in-tag` stays `false` (design.md 5.20,
/// reaffirmed at 5.72 after a since-rejected reversal, design.md 5.71).
/// `BaseStrategy` defaults it to `true` (`base.ts:152`), and losing it
/// would not fail loudly: `getComponent` (`base.ts:178-183`) would then
/// return the package's own component instead of `''`, so
/// `buildReleasePullRequest` (`base.ts:299-304`) would tag the next
/// release `houserules-v<version>` instead of the plain `v<version>`
/// every release since `houserules-v0.2.0-alpha` has used. GitHub's own
/// tag-filter glob still starts every workflow either way -- `[0-9]+`
/// matches one or more digits, `.` matches itself, and `**`/`*` absorb
/// any prefix or suffix -- so `.github/workflows/release.yml`'s
/// `'**[0-9]+.[0-9]+.[0-9]+*'` matches a component-prefixed tag exactly
/// as it matches a plain one: this setting's loss would ship silently.
#[test]
fn include_component_in_tag_stays_false_at_the_config_root() {
    let config = config();
    assert_eq!(config["include-component-in-tag"], false);
}

/// Every `extra-files` entry, when present, is repository-root-relative
/// (a leading `/`, or an object whose own `path` carries one): the
/// package's `addPath` is not `ROOT_PROJECT_PATH`, so an entry without
/// one would resolve under `crates/houserules/` instead of the
/// repository root. `extra-files` itself may legitimately be empty --
/// nothing here requires a standing entry -- so an empty array trivially
/// satisfies this loop; `extra_files_contains_only_the_version_entry`
/// below pins this repository's own current, non-empty shape.
#[test]
fn extra_files_are_anchored_to_the_repository_root() {
    let config = config();
    let extra_files = config["extra-files"]
        .as_array()
        .expect("config.extra-files is a JSON array");
    for entry in extra_files {
        let path = match entry {
            serde_json::Value::String(path) => path.as_str(),
            serde_json::Value::Object(object) => object["path"]
                .as_str()
                .expect("extra-files object entry has a string path"),
            other => panic!("unexpected extra-files entry shape: {other:?}"),
        };
        assert!(
            path.starts_with('/'),
            "extra-files entry {path:?} is not anchored to the repository root"
        );
    }
}

/// `extra-files` carries exactly one entry: the `/.houserules.json`
/// `json`/`jsonpath` update that folds this repository's own kit-version
/// restamp into the release PR itself (`houserules.post-release-restamp`,
/// branch review batch 24 issue 2). `template/.github/workflows/
/// knowledge.yml`'s former entry is retired (issue 1: the seeded
/// installer now pins `releases/latest/download` and needs no per-release
/// rewrite), so this is the array's only member, not one of several.
#[test]
fn extra_files_contains_only_the_version_entry() {
    let config = config();
    assert_eq!(
        config["extra-files"],
        serde_json::json!([
            { "type": "json", "path": "/.houserules.json", "jsonpath": "$.version" }
        ])
    );
}

/// The config never names `package.json` -- as a `package-name`
/// override, an extra-file, or otherwise.
#[test]
fn config_names_no_retired_package_json() {
    let raw = fs::read_to_string(repo_root().join("release-please-config.json"))
        .expect("read release-please-config.json");
    assert!(
        !raw.contains("package.json"),
        "config still names package.json: {raw}"
    );
}

/// The package's `changelog-path` stays `/CHANGELOG.md`. Unset, `Rust`'s
/// changelog update (`rust.ts:37-45`) targets `addPath(this.
/// changelogPath)` with `createIfMissing: true` and the default
/// `CHANGELOG.md`, which resolves under the package's own path once it
/// sits off `ROOT_PROJECT_PATH` -- a new, empty `crates/houserules/
/// CHANGELOG.md` would spring up while the real changelog at the
/// repository root goes stale. That root file is not cosmetic: `dist
/// plan` packages it as `[misc]` into every release archive.
#[test]
fn package_changelog_path_stays_the_root_changelog() {
    let config = config();
    assert_eq!(
        config["packages"]["crates/houserules"]["changelog-path"],
        "/CHANGELOG.md"
    );
}

/// Both halves of the version-alignment pair are checked, not just the
/// config's own citation of itself. `PINNED_ACTION_SHA` is
/// `.github/workflows/release-please.yml`'s pinned
/// `googleapis/release-please-action` commit; its own `dist/index.js`
/// sets `exports.VERSION = '17.6.0'` at that exact sha (verified live).
/// Reading `$schema` alone cannot catch a bumped action pin: this test
/// also reads the workflow file itself and asserts it still pins
/// `PINNED_ACTION_SHA`, so bumping the pin without re-verifying and
/// updating both constants here fails this test instead of leaving
/// every citation in this file quietly wrong.
#[test]
fn schema_names_the_bundled_release_please_version() {
    const RELEASE_PLEASE_VERSION: &str = "v17.6.0";
    const PINNED_ACTION_SHA: &str = "45996ed1f6d02564a971a2fa1b5860e934307cf7";

    let config = config();
    let schema = config["$schema"]
        .as_str()
        .expect("$schema is a JSON string");
    assert!(
        schema.contains(RELEASE_PLEASE_VERSION),
        "$schema {schema:?} does not name {RELEASE_PLEASE_VERSION}"
    );

    let workflow = fs::read_to_string(repo_root().join(".github/workflows/release-please.yml"))
        .expect("read .github/workflows/release-please.yml");
    let pin = format!("googleapis/release-please-action@{PINNED_ACTION_SHA}");
    assert!(
        workflow.contains(&pin),
        "release-please.yml no longer pins {pin:?} -- re-verify the new sha's \
         dist/index.js exports.VERSION and update PINNED_ACTION_SHA, \
         RELEASE_PLEASE_VERSION and every source-line citation in this file together"
    );
}

/// Ports `GenericJson.updateContent` (`src/updaters/generic-json.ts`,
/// fetched live at v17.6.0) against this repository's REAL
/// `.houserules.json`, proving `extra_files_contains_only_the_version_
/// entry`'s `json`/`$.version` entry end to end
/// (`process.wiring-checks-run-the-resolution`): `$.version` is a single
/// top-level field, so JSONPath's own traversal needs no general engine
/// here -- direct field access is that jsonpath's exact resolution.
/// `VERSION_REGEX` (`(?<major>\d+)\.(?<minor>\d+)\.(?<patch>\d+)(-(?<
/// preRelease>[\w.]+))?(\+(?<build>[-\w.]+))?`) is ported with `regress`,
/// this crate's own ECMAScript-regex engine (`rules::check`,
/// `report_claims`, `rules::audit` already depend on it directly, so
/// this proof adds no dependency); only the whole match's span matters
/// here, since `updateContent` replaces group 0, not a named group.
/// `payload.value.replace(VERSION_REGEX, ...)` is JS's non-global
/// `.replace()`, which rewrites the FIRST match only -- `regress::find`
/// (not `find_iter`) is the same restriction.
#[test]
fn houserules_json_version_field_updates_via_the_ported_generic_json_updater() {
    let root = repo_root();
    let raw = fs::read_to_string(root.join(".houserules.json")).expect("read .houserules.json");
    let original: serde_json::Value = serde_json::from_str(&raw).expect("parse .houserules.json");

    let version_regex = regress::Regex::new(r"\d+\.\d+\.\d+(-[\w.]+)?(\+[-\w.]+)?")
        .expect("valid VERSION_REGEX port");
    let current_version = original["version"]
        .as_str()
        .expect(".houserules.json's version field is a string");
    let found = version_regex
        .find(current_version)
        .unwrap_or_else(|| panic!("{current_version:?} does not match VERSION_REGEX"));

    let mut updated_version = String::new();
    updated_version.push_str(&current_version[..found.start()]);
    updated_version.push_str("0.3.0");
    updated_version.push_str(&current_version[found.end()..]);

    let mut updated = original.clone();
    updated["version"] = serde_json::json!(updated_version);

    let mut expected = original.clone();
    expected["version"] = serde_json::json!("0.3.0");
    assert_eq!(
        updated, expected,
        "the ported update must rewrite $.version to 0.3.0 and touch nothing else"
    );
    assert_eq!(updated["idPrefix"], original["idPrefix"]);
    assert_eq!(updated["overrides"], original["overrides"]);
    assert_eq!(updated["baselines"], original["baselines"]);
}
