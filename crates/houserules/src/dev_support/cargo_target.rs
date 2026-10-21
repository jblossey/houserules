//! The target directory of this package, as cargo reports it. The two dev
//! bins that run a binary they just built (`vacuous-exception-gate.rs` and
//! `gen-goldens.rs`) use it to find that binary, and
//! `tests/dev_support_cargo_target.rs` tests it.
//!
//! This is the one helper that `src/bin/*.rs` files share. The package has
//! no library target, so each of them includes this file with `#[path]`;
//! every other helper of a bin stays a copy of its own. One definition
//! means that a fix to the lookup is made once.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The `cargo metadata` command that reports the target directory of the
/// package in `root`. It runs in `root`, as the `cargo build` of its
/// caller does, so a relative `CARGO_TARGET_DIR` resolves against `root`
/// in both.
pub fn metadata_command(root: &Path) -> Command {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let mut command = Command::new(cargo);
    command
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root);
    command
}

/// Runs `command`, a `cargo metadata` command, and returns the
/// `target_directory` of its output: the directory where cargo places the
/// artifacts, wherever `CARGO_TARGET_DIR`, `CARGO_BUILD_TARGET_DIR`, or
/// `build.target-dir` puts it.
pub fn target_directory_of(mut command: Command) -> PathBuf {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("run cargo metadata: {error}"));
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("parse the cargo metadata output: {error}"));
    let directory = metadata["target_directory"]
        .as_str()
        .unwrap_or_else(|| panic!("the cargo metadata output has no target_directory"));
    PathBuf::from(directory)
}
