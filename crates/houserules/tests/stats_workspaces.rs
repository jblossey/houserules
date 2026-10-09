//! `stats` over one or several workspace directories, run through the
//! compiled binary: the variadic positional, the named errors for a
//! missing path and for a file given as a workspace, the zero row for a
//! directory with no deliverables, and one normalized label for a path a
//! shell glob hands over with a trailing separator.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of `common::houserules`, not `mod common;` itself: this
/// file needs none of that module's other helpers, and pulling the module
/// in for this one function makes the rest warn as dead code for this
/// binary (`check_commit.rs`'s own copy has the account). Sets
/// `HOUSERULES_SKIP_SELF_UPDATE` so `update`'s self-update phase never
/// runs here.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// A repository root holding a loadable, empty knowledge base: `stats`
/// loads the base before it reads any workspace.
fn scratch_root() -> TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    let knowledge = root.path().join("knowledge");
    fs::create_dir_all(&knowledge).expect("create knowledge/");
    fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
    fs::write(knowledge.join("areas.json"), "{}").expect("write areas.json");
    fs::write(
        knowledge.join("test.json"),
        serde_json::to_string(&json!({"entries": [{
            "id": "a.rule", "kind": "rule", "area": "global", "standing": false,
            "summary": "A rule.",
        }]}))
        .expect("serialize entries"),
    )
    .expect("write test.json");
    root
}

/// Runs `houserules stats <workspaces...> --dir <root>`.
fn stats(root: &Path, workspaces: &[PathBuf]) -> Output {
    houserules()
        .arg("stats")
        .args(workspaces)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run stats")
}

/// The parsed JSON a successful `stats` run prints.
fn stats_json(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

/// A workspace directory `name` under `parent` holding one failing audit
/// and one report for task 1.
fn workspace_with_task_1(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).expect("create workspace");
    fs::write(
        dir.join("task-1-audit.json"),
        serde_json::to_string(&json!({
            "ids": ["a.rule"],
            "rules": [{"id": "a.rule", "mode": "deterministic", "level": "fail",
                       "result": "fail", "evidence": ""}],
        }))
        .expect("serialize audit"),
    )
    .expect("write audit");
    fs::write(
        dir.join("task-1-report.json"),
        serde_json::to_string(&json!({"kind": "task-report", "knowledge_used": ["a.rule"]}))
            .expect("serialize report"),
    )
    .expect("write report");
    dir
}

/// `stats` with no workspace is a usage error that names the positional.
#[test]
fn stats_without_a_workspace_is_a_usage_error_naming_the_positional() {
    let root = scratch_root();
    let output = stats(root.path(), &[]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("<WORKSPACE>..."), "{stderr}");
}

/// `stats --help` names one or more workspaces.
#[test]
fn stats_help_names_one_or_more_workspaces() {
    let output = houserules()
        .args(["stats", "--help"])
        .output()
        .expect("run stats --help");
    let help = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        help.contains("Usage: houserules stats [OPTIONS] <WORKSPACE>..."),
        "{help}"
    );
    assert!(help.contains("One or more workspace directories"), "{help}");
}

/// A missing workspace is one named line on stderr, exit 2, no stdout
/// (`houserules.crash-paths-are-named`). The line starts with the path
/// as the caller gave it, built the way the binary builds it
/// (`houserules.path-pins-mirror-the-code`).
#[test]
fn a_missing_workspace_is_one_named_error_with_exit_2() {
    let root = scratch_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let good = workspace_with_task_1(scratch.path(), "good");
    let missing = scratch.path().join("missing");
    let output = stats(root.path(), &[good, missing.clone()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.starts_with(&format!("{}: ", missing.display())),
        "{stderr}"
    );
}

/// A file given as a workspace is the same shape of error.
#[test]
fn a_file_given_as_a_workspace_is_one_named_error_with_exit_2() {
    let root = scratch_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let file = scratch.path().join("notes.txt");
    fs::write(&file, "not a directory").expect("write file");
    let output = stats(root.path(), std::slice::from_ref(&file));
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.starts_with(&format!("{}: ", file.display())),
        "{stderr}"
    );
}

/// A directory with no deliverables among the workspaces is a zero row,
/// not an error; the other workspace keeps its numbers.
#[test]
fn an_empty_workspace_among_others_is_a_zero_row_not_an_error() {
    let root = scratch_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let full = workspace_with_task_1(scratch.path(), "full");
    let empty = scratch.path().join("empty");
    fs::create_dir_all(&empty).expect("create empty workspace");
    let value = stats_json(&stats(root.path(), &[full, empty.clone()]));

    let rows = value["workspaces"].as_array().expect("workspaces array");
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[1],
        json!({
            "path": empty.display().to_string(),
            "tasks": 0, "reviews": 0, "re_reviews": 0, "fix_rounds": 0, "report_bytes": 0,
            "findings": {"critical": 0, "important": 0, "minor": 0, "other": 0},
            "targets": {"code": 0, "deliverable": 0, "prose": 0, "other": 0},
        })
    );
    assert_eq!(value["cost"]["totals"]["tasks"], json!(1));
}

/// Only empty workspaces: zero tasks, so every per-task mean is the
/// designed token `none` (`quality.absence-is-designed`).
#[test]
fn only_empty_workspaces_report_none_for_every_per_task_mean() {
    let root = scratch_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let empty = scratch.path().join("empty");
    fs::create_dir_all(&empty).expect("create empty workspace");
    let value = stats_json(&stats(root.path(), &[empty]));
    assert_eq!(
        value["cost"]["per_task"],
        json!({
            "reviews": "none", "re_reviews": "none", "fix_rounds": "none",
            "report_bytes": "none",
            "findings": {"critical": "none", "important": "none", "minor": "none", "other": "none"},
            "targets": {"code": "none", "deliverable": "none", "prose": "none", "other": "none"},
        })
    );
}

/// A shell glob hands each directory over with a trailing separator; the
/// task label prefix is the directory name, never empty.
#[test]
fn a_trailing_separator_still_yields_the_directory_name_as_the_label() {
    let root = scratch_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let first = workspace_with_task_1(scratch.path(), "first");
    let second = workspace_with_task_1(scratch.path(), "second");
    let value = stats_json(&stats(root.path(), &[first.join(""), second.join("")]));
    assert_eq!(
        value["violations"],
        json!([{"id": "a.rule", "count": 2, "tasks": ["first/1", "second/1"]}])
    );
}

/// The same workspace given twice is read once.
#[test]
fn the_same_workspace_given_twice_is_read_once() {
    let root = scratch_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let only = workspace_with_task_1(scratch.path(), "only");
    let once = stats_json(&stats(root.path(), std::slice::from_ref(&only)));
    let twice = stats_json(&stats(root.path(), &[only.clone(), only.join(".")]));
    assert_eq!(twice["workspaces"].as_array().map(Vec::len), Some(1));
    assert_eq!(twice["violations"], once["violations"]);
    assert_eq!(twice["audits"], once["audits"]);
}
