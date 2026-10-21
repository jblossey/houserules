//! `src/dev_support/cargo_target.rs`: the lookup of the target directory
//! that `vacuous-exception-gate.rs` and `gen-goldens.rs` use to find the
//! binary they just built. The module is included with `#[path]` in both
//! bins and here, so its tests run once, in this file.
//!
//! Each test runs the real `cargo metadata` with exactly one source of the
//! target directory set, so the tests run the resolution end to end.

#[path = "../src/dev_support/cargo_target.rs"]
mod cargo_target;

use std::path::{Path, PathBuf};
use std::process::Command;

use cargo_target::{metadata_command, target_directory_of};

/// This checkout's repository root, resolved at compile time -- this
/// file's own copy of the pattern every `tests/*.rs` file in this crate
/// keeps independently (`install.rs`'s own doc explains why).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// A `cargo metadata` command in the repository root without
/// `CARGO_TARGET_DIR` and `CARGO_BUILD_TARGET_DIR` in its environment. A
/// `build.target-dir` value in a cargo config file stays. That is enough:
/// each test sets one source that outranks a config file (an environment
/// variable, or the `--config` argument), so the config file of the
/// developer plays no part.
fn metadata_command_without_target_dir_env() -> Command {
    let mut command = metadata_command(&repo_root());
    command
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_BUILD_TARGET_DIR");
    command
}

/// An absolute `CARGO_TARGET_DIR` names the target directory as it
/// stands.
#[test]
fn target_directory_follows_an_absolute_cargo_target_dir() {
    let outside = std::env::temp_dir().join("cargo-target-outside");
    let mut command = metadata_command_without_target_dir_env();
    command.env("CARGO_TARGET_DIR", &outside);
    assert_eq!(target_directory_of(command), outside);
}

/// `CARGO_BUILD_TARGET_DIR` moves the target directory too.
#[test]
fn target_directory_follows_cargo_build_target_dir() {
    let outside = std::env::temp_dir().join("cargo-build-target-outside");
    let mut command = metadata_command_without_target_dir_env();
    command.env("CARGO_BUILD_TARGET_DIR", &outside);
    assert_eq!(target_directory_of(command), outside);
}

/// So does the `build.target-dir` value that `--config` sets. The path is
/// quoted as a JSON string, whose escapes are a subset of the escapes of a
/// TOML basic string.
#[test]
fn target_directory_follows_the_build_target_dir_config() {
    let outside = std::env::temp_dir().join("cargo-config-target-outside");
    let quoted = serde_json::to_string(outside.to_str().expect("the temp path is UTF-8"))
        .expect("quote the path as a string");
    let mut command = metadata_command_without_target_dir_env();
    command
        .arg("--config")
        .arg(format!("build.target-dir={quoted}"));
    assert_eq!(target_directory_of(command), outside);
}

/// A relative `CARGO_TARGET_DIR` resolves against the working directory of
/// the command, the repository root: the directory that holds the root
/// `Cargo.toml`.
#[test]
fn target_directory_resolves_a_relative_cargo_target_dir_in_the_root() {
    let mut command = metadata_command_without_target_dir_env();
    command.env("CARGO_TARGET_DIR", "relative-target");
    let directory = target_directory_of(command);
    assert!(directory.is_absolute(), "{}", directory.display());
    assert!(
        directory.ends_with("relative-target"),
        "{}",
        directory.display()
    );
    let parent = directory
        .parent()
        .expect("the target directory has a parent");
    assert!(parent.join("Cargo.toml").is_file(), "{}", parent.display());
}
