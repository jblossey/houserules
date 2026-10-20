//! The released 1.x contents of the two D3 files (`crates/houserules/released/`),
//! which the conversion compares an install's copies with (spec section 5,
//! step 8). Each content is a file of the index, read back from the `v1.*`
//! release tags: a content is the bytes of `template/<path>` at every tag the
//! index names for it, and every `v1.*` tag is named, so a release that
//! changes either file fails here until its content joins the index.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// This checkout's repository root.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The directory of the released contents.
fn released_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("released")
}

/// The index of the released contents, parsed.
fn index() -> Value {
    serde_json::from_str(
        &fs::read_to_string(released_dir().join("index.json")).expect("read the index"),
    )
    .expect("parse the index")
}

/// The `v1.*` release tags of the repository, as versions without the `v`.
fn release_versions() -> BTreeSet<String> {
    let output = Command::new("git")
        .args(["tag", "--list", "v1.*"])
        .current_dir(repo_root())
        .output()
        .expect("run git tag");
    assert!(output.status.success(), "git tag failed");
    String::from_utf8(output.stdout)
        .expect("utf8 tags")
        .lines()
        .map(|tag| tag.trim_start_matches('v').to_string())
        .collect()
}

/// The bytes of `template/<path>` at the tag `v<version>`.
fn bytes_at_tag(version: &str, path: &str) -> Vec<u8> {
    let output = Command::new("git")
        .arg("show")
        .arg(format!("v{version}:template/{path}"))
        .current_dir(repo_root())
        .output()
        .expect("run git show");
    assert!(
        output.status.success(),
        "git show v{version}:template/{path} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

/// Each file of the index holds the bytes of its installed path at every
/// release the index names for it.
#[test]
fn every_released_content_equals_the_file_at_each_release_that_names_it() {
    let index = index();
    for (installed, rows) in index.as_object().expect("the index is an object") {
        for row in rows.as_array().expect("rows") {
            let file = row["file"].as_str().expect("a file name");
            let content = fs::read(released_dir().join(file)).expect("read a released file");
            for version in row["versions"].as_array().expect("versions") {
                let version = version.as_str().expect("a version");
                assert!(
                    bytes_at_tag(version, installed) == content,
                    "{file} differs from {installed} at v{version}"
                );
            }
        }
    }
}

/// Every `v1.*` release is named once for each D3 file, so no release
/// ships a content that the conversion would call an adopter change.
#[test]
fn every_release_tag_is_named_once_for_each_d3_file() {
    let expected = release_versions();
    assert!(!expected.is_empty(), "the checkout holds no v1.* tag");
    let index = index();
    for (installed, rows) in index.as_object().expect("the index is an object") {
        let mut named: Vec<String> = rows
            .as_array()
            .expect("rows")
            .iter()
            .flat_map(|row| row["versions"].as_array().expect("versions").iter())
            .map(|version| version.as_str().expect("a version").to_string())
            .collect();
        named.sort();
        let listed: BTreeSet<String> = named.iter().cloned().collect();
        assert_eq!(
            named.len(),
            listed.len(),
            "{installed} names a version twice"
        );
        assert_eq!(listed, expected, "{installed}");
    }
}

/// The two distinct contents of the workflow and the one of the schema are
/// the ones the index files hold: no file of the directory is unnamed, and
/// no two files hold the same bytes.
#[test]
fn the_released_directory_holds_each_distinct_content_once() {
    let index = index();
    let named: BTreeSet<String> = index
        .as_object()
        .expect("the index is an object")
        .values()
        .flat_map(|rows| rows.as_array().expect("rows").iter())
        .map(|row| row["file"].as_str().expect("a file name").to_string())
        .collect();
    let on_disk: BTreeSet<String> = fs::read_dir(released_dir())
        .expect("read the directory")
        .map(|entry| {
            entry
                .expect("read an entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 name")
        })
        .filter(|name| name != "index.json")
        .collect();
    assert_eq!(named, on_disk);
    let mut contents: Vec<Vec<u8>> = named
        .iter()
        .map(|file| fs::read(released_dir().join(file)).expect("read a released file"))
        .collect();
    contents.sort();
    contents.dedup();
    assert_eq!(contents.len(), named.len(), "two files hold one content");
}
