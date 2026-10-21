//! The released contents of the kit (`crates/houserules/released/`), which the
//! conversion compares an install with (spec 4.7, batch 33 spec section 5,
//! step 8). `index.json` names, for each install path, every distinct content
//! that a `v1.*` release shipped or that the batch-33 main (`BATCH_33`) holds,
//! and `entries.json` every distinct text of a pack entry that a release shipped
//! (the base of the fields a project changed). A content is the bytes of
//! `template/<path>` at every tag and commit that the index names for it, and
//! every `v1.*` tag that holds the path is named, so a release that changes a
//! file fails here until its content joins the index.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// The batch-33 merge on main: the contents of an install of batch 33.
const BATCH_33: &str = "fcc342d";

/// The install paths of the index: the kit-owned files of 1.x, the files that
/// batch 34 moves to the D3 class, and the files of the reference workflow.
const PATHS: [&str; 15] = [
    ".claude/agents/branch-reviewer.md",
    ".claude/agents/implementer.md",
    ".claude/agents/task-reviewer.md",
    ".claude/evals/dependency-add.json",
    ".claude/evals/docs-edit.json",
    ".claude/evals/seeded-violations.json",
    ".claude/schemas/deliverables.json",
    ".claude/skills/finishing-a-feature/SKILL.md",
    ".claude/skills/migrating-knowledge/SKILL.md",
    ".claude/skills/orchestrating/SKILL.md",
    ".github/workflows/knowledge.yml",
    ".githooks/commit-msg",
    "backlog/schema.json",
    "knowledge/schema.json",
    "tools/claude-session-start.sh",
];

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

/// Parses the JSON file `name` of the released directory.
fn released_json(name: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(released_dir().join(name))
            .unwrap_or_else(|error| panic!("read {name}: {error}")),
    )
    .unwrap_or_else(|error| panic!("parse {name}: {error}"))
}

/// The index of the released contents, parsed.
fn index() -> Value {
    released_json("index.json")
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

/// The bytes of `template/<path>` at the git reference `reference`, or `None`
/// when the reference holds no such path.
fn bytes_at(reference: &str, path: &str) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("show")
        .arg(format!("{reference}:template/{path}"))
        .current_dir(repo_root())
        .output()
        .expect("run git show");
    output.status.success().then_some(output.stdout)
}

/// The commits that a row names; a row of a release alone names none.
fn commits_of(row: &Value) -> Vec<String> {
    row.get("commits")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .map(|commit| commit.as_str().expect("a commit").to_string())
        .collect()
}

/// The versions and the commits that a row names, as references of git.
fn references(row: &Value) -> Vec<String> {
    row["versions"]
        .as_array()
        .expect("versions")
        .iter()
        .map(|version| format!("v{}", version.as_str().expect("a version")))
        .chain(commits_of(row))
        .collect()
}

/// The index names exactly the install paths of `PATHS`.
#[test]
fn the_index_names_the_install_paths_of_the_kit_owned_and_moved_files() {
    let index = index();
    let mut named: Vec<&str> = index
        .as_object()
        .expect("the index is an object")
        .keys()
        .map(String::as_str)
        .collect();
    named.sort_unstable();
    let mut expected = PATHS.to_vec();
    expected.sort_unstable();
    assert_eq!(named, expected);
}

/// Each file of the index holds the bytes of its installed path at every
/// release and every commit that the index names for it.
#[test]
fn every_released_content_equals_the_file_at_each_reference_that_names_it() {
    let index = index();
    for (installed, rows) in index.as_object().expect("the index is an object") {
        for row in rows.as_array().expect("rows") {
            let file = row["file"].as_str().expect("a file name");
            let content = fs::read(released_dir().join(file)).expect("read a released file");
            for reference in references(row) {
                assert!(
                    bytes_at(&reference, installed).as_deref() == Some(content.as_slice()),
                    "{file} differs from {installed} at {reference}"
                );
            }
        }
    }
}

/// Every `v1.*` release that holds a path is named once for it, and no tag
/// that lacks the path is named, so no release ships a content that the
/// conversion would call an adopter change. The batch-33 commit is named once
/// for each path that it holds.
#[test]
fn every_release_that_holds_a_path_is_named_once_for_it() {
    let tags = release_versions();
    assert!(!tags.is_empty(), "the checkout holds no v1.* tag");
    let index = index();
    for (installed, rows) in index.as_object().expect("the index is an object") {
        let mut named: Vec<String> = Vec::new();
        let mut commits: Vec<String> = Vec::new();
        for row in rows.as_array().expect("rows") {
            for version in row["versions"].as_array().expect("versions") {
                named.push(version.as_str().expect("a version").to_string());
            }
            commits.extend(commits_of(row));
        }
        let listed: BTreeSet<String> = named.iter().cloned().collect();
        assert_eq!(
            named.len(),
            listed.len(),
            "{installed} names a version twice"
        );
        let held: BTreeSet<String> = tags
            .iter()
            .filter(|tag| bytes_at(&format!("v{tag}"), installed).is_some())
            .cloned()
            .collect();
        assert_eq!(listed, held, "{installed}");
        let expected_commits: Vec<String> = bytes_at(BATCH_33, installed)
            .map(|_| BATCH_33.to_string())
            .into_iter()
            .collect();
        assert_eq!(commits, expected_commits, "{installed}");
    }
}

/// No file of the directory is unnamed, and no two files hold the same bytes.
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
        .filter(|name| name != "index.json" && name != "entries.json")
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

/// `entries.json` holds, for each pack entry that a release shipped, each
/// distinct text with the versions that shipped it: every entry of every topic
/// file at every `v1.*` tag is in the file with its tag, and no id of the file
/// is one that no tag ships.
#[test]
fn entries_json_holds_every_text_of_every_entry_that_a_release_shipped() {
    let entries = released_json("entries.json");
    let entries = entries.as_object().expect("entries.json is an object");
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    for tag in release_versions() {
        let listing = Command::new("git")
            .args(["ls-tree", "-r", "--name-only"])
            .arg(format!("v{tag}"))
            .arg("template/knowledge")
            .current_dir(repo_root())
            .output()
            .expect("run git ls-tree");
        for path in String::from_utf8(listing.stdout).expect("utf8").lines() {
            let name = path.rsplit('/').next().expect("a name");
            if !path.ends_with(".json") || name == "schema.json" || name == "areas.json" {
                continue;
            }
            let output = Command::new("git")
                .arg("show")
                .arg(format!("v{tag}:{path}"))
                .current_dir(repo_root())
                .output()
                .expect("run git show");
            let topic: Value = serde_json::from_slice(&output.stdout).expect("a topic file");
            for entry in topic["entries"].as_array().expect("entries") {
                let id = entry["id"].as_str().expect("an id");
                let texts = entries
                    .get(id)
                    .and_then(Value::as_array)
                    .unwrap_or_else(|| panic!("entries.json lacks {id}"));
                let held = texts.iter().any(|text| {
                    text["entry"] == *entry
                        && text["versions"]
                            .as_array()
                            .expect("versions")
                            .iter()
                            .any(|version| version == tag.as_str())
                });
                assert!(held, "entries.json lacks the text of {id} at v{tag}");
                *seen.entry(id.to_string()).or_default() += 1;
            }
        }
    }
    for id in entries.keys() {
        assert!(
            seen.contains_key(id),
            "entries.json holds {id}, which no tag ships"
        );
    }
}
