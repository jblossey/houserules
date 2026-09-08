//! Pins HR-073's release-please rewiring (batch 20 T5, ruled design.md
//! §5.43): `release-please-config.json`'s one package moved from `"."`
//! (`release-type: "node"`, reading the `package.json` batch 20 T3
//! retired) to `"crates/houserules"` (`release-type: "rust"`, reading
//! that crate's own `Cargo.toml`). `docs/runbook.md`'s "release-please's
//! release-type" section carries the full source derivation
//! (`googleapis/release-please@v17.11.2`'s `Rust.getDefaultPackageName`
//! versus the retired `Node` strategy's `getPkgJsonContents`); this file
//! pins only the config shape that derivation depends on, cheaply, with
//! no release-please run.
//!
//! `BaseStrategy.addPath` (`src/strategies/base.ts`, v17.11.2) prefixes
//! every non-absolute extra-file path with the package's own path unless
//! that package sits at the repository root (`ROOT_PROJECT_PATH`, `"."`)
//! or the path itself starts with `/`. Moving the package off root turns
//! every extra-file path lacking a leading `/` into a path under
//! `crates/houserules/` that does not exist -- a silent no-op release-please
//! only logs, never fails on (`github.ts`'s `buildChangeSet` continues
//! past a missing `createIfMissing: false` file). The extra-files test
//! below pins the leading-`/` fix against a repeat of that regression, and
//! two more tests pin the settings whose own loss would be just as silent:
//! `include-component-in-tag` (would flip the tag shape, not fail) and
//! `changelog-path` (would fork a second, empty changelog, not fail).

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
/// path (`src/manifest.ts`'s `parseReleasedVersions`, v17.11.2, indexes
/// released versions by that same path) -- both name `crates/houserules`,
/// the crate whose `Cargo.toml` is now the version source.
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

/// The one package names the `rust` release-type (`src/factory.ts`,
/// v17.11.2, maps it to the `Rust` strategy) -- not `node`, which reads a
/// `package.json` this repository no longer has.
#[test]
fn package_release_type_is_rust() {
    let config = config();
    assert_eq!(
        config["packages"]["crates/houserules"]["release-type"],
        "rust"
    );
}

/// Every `extra-files` entry is repository-root-relative (a leading `/`,
/// or an object whose own `path` carries one): the package's `addPath`
/// no longer sits at `ROOT_PROJECT_PATH`, so an entry without one would
/// resolve under `crates/houserules/` instead of the repository root.
#[test]
fn extra_files_are_anchored_to_the_repository_root() {
    let config = config();
    let extra_files = config["extra-files"]
        .as_array()
        .expect("config.extra-files is a JSON array");
    assert!(
        !extra_files.is_empty(),
        "extra-files must not be emptied out"
    );
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

/// The config never names the `package.json` batch 20 T3 (HR-047)
/// retired -- as a `package-name` override, an extra-file, or otherwise.
#[test]
fn config_names_no_retired_package_json() {
    let raw = fs::read_to_string(repo_root().join("release-please-config.json"))
        .expect("read release-please-config.json");
    assert!(
        !raw.contains("package.json"),
        "config still names package.json: {raw}"
    );
}

/// The root `include-component-in-tag` stays `false`. `BaseStrategy`
/// defaults it to `true` (`base.ts:152`), and losing it would not fail
/// loudly: `getComponent` (`base.ts:178-183`) would then return the
/// package's own component instead of `''`, so `buildReleasePullRequest`
/// (`base.ts:299-304`) would tag the next release `houserules-v<version>`
/// instead of the plain `v<version>` every prior release used. GitHub's
/// own tag-filter glob still starts every workflow either way --
/// `[0-9]+` matches one or more digits, `.` matches itself, and `**`/`*`
/// absorb any prefix or suffix (GitHub Actions' filter pattern cheat
/// sheet, `v[12].[0-9]+.[0-9]+` matching `v1.10.1`/`v2.0.0` is the
/// documented example) -- so `.github/workflows/release.yml`'s
/// `'**[0-9]+.[0-9]+.[0-9]+*'` matches a component-prefixed tag exactly
/// as it matches a plain one. The divergence from every release
/// `v0.2.0-alpha` and earlier established would ship silently.
#[test]
fn include_component_in_tag_stays_false_at_the_config_root() {
    let config = config();
    assert_eq!(config["include-component-in-tag"], false);
}

/// The package's `changelog-path` stays `/CHANGELOG.md`. Unset, `Rust`'s
/// changelog update (`rust.ts:37-45`) targets `addPath(this.
/// changelogPath)` with `createIfMissing: true` and the default
/// `CHANGELOG.md`, which resolves under the package's own path once it
/// sits off `ROOT_PROJECT_PATH` -- a new, empty `crates/houserules/
/// CHANGELOG.md` would spring up while the real changelog at the
/// repository root goes stale. That root file is not cosmetic: `dist
/// plan` packages it as `[misc]` into every one of the five release
/// archives (`.superpowers/sdd/2026-09-07-batch-20/t5-evidence/
/// dist-plan-v0.2.0-alpha.log`).
#[test]
fn package_changelog_path_stays_the_root_changelog() {
    let config = config();
    assert_eq!(
        config["packages"]["crates/houserules"]["changelog-path"],
        "/CHANGELOG.md"
    );
}
