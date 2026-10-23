//! Helpers that the tests of the help texts and of the documents share
//! (`help_35.rs` and `help_36.rs`): the text of a `--help` and of a repository
//! file with every run of white space folded to one space, and the assertions
//! that list every statement a text lacks or holds.
//!
//! Clap and the Markdown files wrap their lines, so a statement is compared on
//! the folded text. Clap drops the period that ends the last sentence of a
//! command's description, so no statement ends on it.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `text` with every run of white space read as one space.
pub fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text that `houserules <args>` prints on stdout, folded. The command
/// exits 0.
pub fn help_of_args(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_houserules"))
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .args(args)
        .output()
        .expect("run --help");
    assert_eq!(output.status.code(), Some(0), "{args:?} exits 0");
    folded(&String::from_utf8(output.stdout).expect("utf8 stdout"))
}

/// The text that `houserules <command> --help` prints, folded.
pub fn help_of(command: &str) -> String {
    help_of_args(&[command, "--help"])
}

/// The file `relative` below the repository root, built component by component
/// (`houserules.path-pins-mirror-the-code`).
pub fn repo_file(relative: &str) -> PathBuf {
    relative.split('/').fold(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".."),
        |path, part| path.join(part),
    )
}

/// The text of the file `relative` below the repository root.
pub fn read(relative: &str) -> String {
    fs::read_to_string(repo_file(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

/// Fails with every statement of `statements` that `text` lacks.
pub fn states(surface: &str, text: &str, statements: &[&str]) {
    let missing: Vec<&str> = statements
        .iter()
        .copied()
        .filter(|statement| !text.contains(statement))
        .collect();
    assert!(
        missing.is_empty(),
        "{surface} lacks {} statement(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
}

/// Fails with every statement of `statements` that `text` holds.
pub fn omits(surface: &str, text: &str, statements: &[&str]) {
    let found: Vec<&str> = statements
        .iter()
        .copied()
        .filter(|statement| text.contains(statement))
        .collect();
    assert!(
        found.is_empty(),
        "{surface} holds {} stale statement(s):\n{}",
        found.len(),
        found.join("\n")
    );
}
