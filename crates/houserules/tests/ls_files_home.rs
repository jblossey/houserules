//! Lint: the binary asks git for a path listing in one module only.
//!
//! The listings (`ls-files`) live in `rules/git_paths.rs`: a caller that
//! needs one calls that module and keeps its own handling of a failure. This
//! test reads every `.rs` file under `crates/houserules/src`, found by a
//! directory walk, and fails when a file other than that module holds the
//! string literal `"ls-files"`, quotes included. The literal is the
//! argument that a git command takes. A message that names the command
//! (`"git ls-files failed"`) is another literal and passes.
//!
//! The walk fails loudly on a directory or a file it cannot read
//! (`quality.gates-derive-their-scope`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use walkdir::WalkDir;

/// The one module that passes the `ls-files` argument to git, as a
/// `/`-separated path below `src`.
const HOME: &str = "rules/git_paths.rs";

/// The literal the lint looks for, quotes included.
const LITERAL: &str = "\"ls-files\"";

/// The files below `src` that may hold the literal, each with its reason.
/// Every one is a gate binary: its own binary has no access to the crate's
/// modules, so it cannot call `rules::git_paths`.
const EXEMPT: &[(&str, &str)] = &[
    (
        "bin/payload-stamp-gate.rs",
        "a gate binary: it cannot call the crate's modules",
    ),
    (
        "bin/dist-generate-check.rs",
        "a gate binary: it cannot call the crate's modules",
    ),
    (
        "bin/residue-gate.rs",
        "a gate binary: it cannot call the crate's modules",
    ),
];

/// Every `.rs` file below `src` with its text, as `/`-separated paths below
/// `src`, sorted by path.
fn source_files(src: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for entry in WalkDir::new(src).sort_by_file_name() {
        let entry = entry.unwrap_or_else(|error| panic!("walk {}: {error}", src.display()));
        if !entry.file_type().is_file() || entry.path().extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let below = entry
            .path()
            .strip_prefix(src)
            .expect("a walked path lies below its root");
        let relative = below
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let text = fs::read_to_string(entry.path())
            .unwrap_or_else(|error| panic!("read {}: {error}", entry.path().display()));
        files.push((relative, text));
    }
    files
}

/// The sources of the crate.
fn crate_sources() -> Vec<(String, String)> {
    source_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
}

/// A file other than the home and the exempt files that holds the literal
/// fails the lint, and the failure names it.
#[test]
fn no_source_file_but_the_home_holds_the_ls_files_literal() {
    let offenders: Vec<String> = crate_sources()
        .into_iter()
        .filter(|(path, text)| {
            text.contains(LITERAL)
                && path != HOME
                && !EXEMPT.iter().any(|(exempt, _)| exempt == path)
        })
        .map(|(path, _)| path)
        .collect();

    assert!(
        offenders.is_empty(),
        "these files pass the \"ls-files\" argument to git; list paths through {HOME} instead: \
         {offenders:?}"
    );
}

/// The lint cannot pass for want of a subject: the home holds the literal,
/// and every exemption still holds it (an exemption that holds none is
/// dead and goes).
#[test]
fn the_home_and_every_exemption_hold_the_literal() {
    let holders: BTreeSet<String> = crate_sources()
        .into_iter()
        .filter(|(_, text)| text.contains(LITERAL))
        .map(|(path, _)| path)
        .collect();

    assert!(
        holders.contains(HOME),
        "{HOME} holds no \"ls-files\" literal; the lint has no home to protect"
    );
    for (path, reason) in EXEMPT {
        assert!(
            holders.contains(*path),
            "{path} is exempt ({reason}) but holds no \"ls-files\" literal; drop the exemption"
        );
    }
}
