//! The compression-marker gate, end to end through the compiled binary: a
//! marker written into a knowledge file, a backlog file, and a deliverable
//! fails `check-knowledge`, `check-backlog`, and `validate`, each with its
//! file and field, and the same files without the marker pass. The gate's
//! own cases (what a marker is, keys, delimiters) are unit tests in
//! `src/rules/markers.rs`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off (`tests/update.rs`'s own copy
/// of this helper has the account).
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// A scratch repository that `houserules init --tracker json` installed,
/// with one local topic file `knowledge/team.json` that holds one entry.
/// `tempfile`'s `TempDir` removes itself on drop.
fn seeded_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    let init = run(dir.path(), &["init", "--tracker", "json"]);
    assert_eq!(init.status.code(), Some(0), "{init:?}");
    let topic = json!({
        "topic": "team",
        "title": "Team",
        "entries": [{
            "id": "team.local-rule", "kind": "rule", "area": "global", "standing": false,
            "summary": "A rule of the team.", "body": ["Why and how."], "tags": ["test"],
            "source": {"date": "2026-10-19", "by": "user"},
        }],
    });
    fs::write(
        dir.path().join("knowledge/team.json"),
        format!("{}\n", serde_json::to_string_pretty(&topic).unwrap()),
    )
    .expect("write the topic file");
    let render = run(dir.path(), &["render"]);
    assert_eq!(render.status.code(), Some(0), "{render:?}");
    dir
}

/// A compression marker, built here so that no tracked file holds one.
fn marker() -> String {
    format!("{}x{}", "<<ccr:", ">>")
}

/// The whole finding line for the field at `at`.
fn finding(at: &str) -> String {
    format!(
        "{at}: holds a compression marker (headroom ccr); a context proxy changed this text, so \
         restore the original"
    )
}

/// Runs `houserules <args> --dir <root>`.
fn run(root: &Path, args: &[&str]) -> Output {
    houserules()
        .args(args)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run houserules")
}

fn stderr_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::to_string)
        .collect()
}

/// A marker in a topic file's string value fails `check-knowledge` with the
/// file and the field path; the same file without it passes.
#[test]
fn check_knowledge_rejects_a_marker_in_a_knowledge_file() {
    let dir = seeded_repo();
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "knowledge: ok\n");
    assert_eq!(output.status.code(), Some(0));

    let path = dir.path().join("knowledge/team.json");
    let original = fs::read_to_string(&path).expect("read the topic file");
    let mut topic: Value = serde_json::from_str(&original).expect("parse the topic file");
    topic["entries"][0]["body"][0] = json!(marker());
    fs::write(&path, serde_json::to_string_pretty(&topic).unwrap()).expect("write");
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(1));
    let lines = stderr_lines(&output);
    assert!(
        lines.contains(&finding("knowledge/team.json.entries[0].body[0]")),
        "the finding is missing from: {lines:#?}"
    );
    assert!(
        lines.iter().all(|line| !line.contains(&marker())),
        "a finding prints the marker: {lines:#?}"
    );

    fs::write(&path, original).expect("restore the topic file");
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(0));
}

/// A marker in a backlog item fails `check-backlog` with the file and the
/// field path; the seeded backlog passes.
#[test]
fn check_backlog_rejects_a_marker_in_a_backlog_file() {
    let dir = seeded_repo();
    let output = run(dir.path(), &["check-backlog"]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "backlog: ok\n");
    assert_eq!(output.status.code(), Some(0));

    let path = dir.path().join("backlog/items/general.json");
    let mut section: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read the items file"))
            .expect("parse the items file");
    section["items"][0]["title"] = json!(marker());
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&section).unwrap()),
    )
    .expect("write the items file");
    let output = run(dir.path(), &["check-backlog"]);
    assert_eq!(output.status.code(), Some(1));
    let lines = stderr_lines(&output);
    assert!(
        lines.contains(&finding("backlog/items/general.json.items[0].title")),
        "the finding is missing from: {lines:#?}"
    );
}

/// A marker in a deliverable fails `validate` with the file and the field
/// path, in the JSON that `validate` prints.
#[test]
fn validate_rejects_a_marker_in_a_deliverable() {
    let dir = seeded_repo();
    let report = dir.path().join("review.json");
    fs::write(
        &report,
        serde_json::to_string(&json!({"kind": "branch-review", "assessment": marker()})).unwrap(),
    )
    .expect("write the deliverable");
    let output = run(
        dir.path(),
        &["validate", report.to_str().expect("a UTF-8 path")],
    );
    assert_eq!(output.status.code(), Some(1));
    let results: Value = serde_json::from_slice(&output.stdout).expect("validate prints JSON");
    let errors: Vec<&str> = results[0]["errors"]
        .as_array()
        .expect("errors array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let expected = finding(&format!("{}.assessment", report.display()));
    assert!(
        errors.contains(&expected.as_str()),
        "the finding is missing from: {errors:#?}"
    );
}
