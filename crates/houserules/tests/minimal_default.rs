//! The minimal default (batch 34, spec 4.6; HR-182): a plain `init` installs
//! the core set and the `knowledge-base` pack, with no tracker and no
//! workflow, and every gate passes on that install. Each case runs the
//! compiled binary against a scratch git repository.

mod tracker_support;
mod tree_snapshot;

use std::fs;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    AUDIT_LINE_ZERO, LINE_A, assert_output, commit_all, git, read_json, run, run_ok,
    scratch_git_repo, stderr, stdout,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The files of a plain `init` on a repository with no GitHub origin, as
/// spec 4.6 lists them: the marker, the knowledge schema and areas, the one
/// pack, the two core skills and the step script, the `.gitignore` line, the
/// deliverables schema, `AGENTS.md`, `CLAUDE.md`, and `docs/README.md`.
const MINIMAL_FILES: [&str; 12] = [
    ".claude/schemas/deliverables.json",
    ".claude/skills/houserules-step/SKILL.md",
    ".claude/skills/migrating-knowledge/SKILL.md",
    ".gitignore",
    ".houserules.json",
    "AGENTS.md",
    "CLAUDE.md",
    "docs/README.md",
    "knowledge/areas.json",
    "knowledge/packs/knowledge-base.json",
    "knowledge/schema.json",
    "tools/houserules-step.sh",
];

/// The line that `check-commit` prints when the base holds no `commits`
/// check (design.md 5.125): a run that checked nothing does not read as a
/// pass.
const CHECKED_NOTHING: &str = "check-commit: no commits check is installed; checked nothing";

/// The line of the `.gitignore` that keeps the scratch output of the step and
/// the batch workspaces out of the repository (spec 4.4a; batch 35, spec 7).
const IGNORE_LINE: &str = ".houserules/";

/// A scratch install that a plain `init` made.
fn plain_install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    dir
}

/// The paths of the files under `dir`, sorted.
fn files_of(dir: &std::path::Path) -> Vec<String> {
    snapshot(dir).into_keys().collect()
}

/// Spec 4.6 and live case 1: a plain `init` writes exactly the files of the
/// list, and no other. No directory `AGENTS.md` exists, because the
/// `knowledge-base` pack alone gives no area a directory block.
#[test]
fn a_plain_init_writes_exactly_the_files_of_the_minimal_default() {
    let dir = plain_install();
    assert_eq!(files_of(dir.path()), MINIMAL_FILES);
}

/// Spec 4.6: on a target with a GitHub origin, the CI workflow joins the list.
#[test]
fn a_plain_init_on_a_github_hosted_origin_also_writes_the_ci_workflow() {
    let dir = scratch_git_repo();
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/name.git",
        ],
    );
    run_ok(dir.path(), &["init"]);
    let mut expected: Vec<String> = MINIMAL_FILES.iter().map(|file| file.to_string()).collect();
    expected.push(".github/workflows/knowledge.yml".to_string());
    expected.sort();
    assert_eq!(files_of(dir.path()), expected);
}

/// Spec 4.6: the only pack that a plain `init` installs is `knowledge-base`,
/// and the marker records no tracker and no workflow.
#[test]
fn a_plain_init_pins_the_knowledge_base_pack_alone_and_records_no_tracker_or_workflow() {
    let dir = plain_install();
    let marker = read_json(dir.path(), ".houserules.json");
    let packs: Vec<&String> = marker["packs"]
        .as_object()
        .expect("packs is an object")
        .keys()
        .collect();
    assert_eq!(packs, ["knowledge-base"]);
    assert!(marker.get("tracker").is_none(), "got {marker}");
    assert!(marker.get("workflow").is_none(), "got {marker}");
}

/// Spec 4.4a and batch 35, spec 7: `init` adds `.houserules/`, the folder of
/// the step and the workspaces, to a new `.gitignore`.
#[test]
fn a_plain_init_writes_the_ignore_line_to_a_new_gitignore() {
    let dir = plain_install();
    assert_eq!(
        fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore"),
        format!("{IGNORE_LINE}\n")
    );
}

/// Spec 4.4a: `init` appends the line to a `.gitignore` that lacks it, and
/// keeps every line the project wrote, also when the file ends with no
/// newline.
#[test]
fn init_appends_the_ignore_line_to_an_existing_gitignore() {
    let dir = scratch_git_repo();
    fs::write(dir.path().join(".gitignore"), "target/\n*.log").expect("write .gitignore");
    run_ok(dir.path(), &["init"]);
    assert_eq!(
        fs::read_to_string(dir.path().join(".gitignore")).expect("read .gitignore"),
        format!("target/\n*.log\n{IGNORE_LINE}\n")
    );
}

/// A second `init` writes nothing: the line is there, and the file stays as
/// it is.
#[test]
fn a_second_init_keeps_a_gitignore_that_holds_the_ignore_line() {
    let dir = plain_install();
    let before = snapshot(dir.path());
    run_ok(dir.path(), &["init"]);
    assert_same_tree(&before, &snapshot(dir.path()), "the second init");
}

/// A project that lists `.gitignore` in `ownedPaths` owns it: `init` leaves
/// it alone.
#[test]
fn init_never_writes_a_gitignore_that_the_project_owns() {
    let dir = plain_install();
    fs::remove_file(dir.path().join(".gitignore")).expect("remove .gitignore");
    tracker_support::edit_marker(dir.path(), |marker| {
        marker.insert("ownedPaths".to_string(), json!([".gitignore"]));
    });
    run_ok(dir.path(), &["init"]);
    assert!(!dir.path().join(".gitignore").exists());
}

/// A validating `BLOCKED` task report, which keeps `self_audit` null so the
/// sample needs no audit rows.
fn blocked_report() -> Value {
    json!({
        "kind": "task-report", "model": "claude-opus-5-5", "task": 1, "backlog": [],
        "status": "BLOCKED", "implemented": "x",
        "commits": [], "tests": [], "live_run": [], "tdd": [],
        "files_changed": [], "docs_verified": [], "dependency_vetting": null,
        "coverage": [], "self_audit": null, "self_review": [], "concerns": [],
        "knowledge_used": [],
    })
}

/// Spec 4.6 and live case 1: every gate passes on a minimal install.
/// `check-backlog` prints line A and exits 0, `audit` prints its count of
/// skipped references and exits 0, and `check-commit` prints that it checked
/// nothing.
#[test]
fn every_gate_passes_on_a_minimal_install() {
    let dir = plain_install();
    let base = commit_all(dir.path(), "chore: install the kit");
    fs::write(dir.path().join("docs/note.md"), "A note.\n").expect("write a document");
    commit_all(dir.path(), "docs: add a note");
    let report = dir.path().join("report.json");
    fs::write(
        &report,
        serde_json::to_string_pretty(&blocked_report()).expect("serialize"),
    )
    .expect("write the report");

    assert_output(
        &run(dir.path(), &["check-knowledge"]),
        0,
        "knowledge: ok\n",
        "",
    );
    assert_output(
        &run(dir.path(), &["check-backlog"]),
        0,
        &format!("{LINE_A}\n"),
        "",
    );
    assert_output(
        &run(dir.path(), &["render", "--check"]),
        0,
        "render: up to date\n",
        "",
    );
    let audit = run(dir.path(), &["audit", "--base", &base]);
    assert_eq!(
        (audit.status.code(), stderr(&audit)),
        (Some(0), format!("{AUDIT_LINE_ZERO}\n"))
    );
    let validate = run(
        dir.path(),
        &["validate", report.to_str().expect("a UTF-8 path")],
    );
    assert_eq!(validate.status.code(), Some(0), "{}", stdout(&validate));
    assert_output(
        &run(dir.path(), &["check-commit", "--from", &base]),
        0,
        &format!("{CHECKED_NOTHING}\n"),
        "",
    );
}

/// Design.md 5.125: `check-commit` on the message file of the commit-msg
/// hook prints the same line when no `commits` check is installed.
#[test]
fn check_commit_on_a_message_file_prints_that_it_checked_nothing() {
    let dir = plain_install();
    let message = dir.path().join("MSG");
    fs::write(&message, "feat: anything\n").expect("write the message");
    let output = run(
        dir.path(),
        &["check-commit", message.to_str().expect("a UTF-8 path")],
    );
    assert_output(&output, 0, &format!("{CHECKED_NOTHING}\n"), "");
}

/// Spec 4.6 and live case 1: `update` twice on a minimal install writes
/// nothing under `backlog/` and no eval file, and the second run changes no
/// file at all.
#[test]
fn update_twice_writes_nothing_under_backlog_and_no_eval_file() {
    let dir = plain_install();
    run_ok(dir.path(), &["update"]);
    let after_first = snapshot(dir.path());
    run_ok(dir.path(), &["update"]);
    assert_same_tree(&after_first, &snapshot(dir.path()), "the second update");
    let written: Vec<String> = after_first
        .keys()
        .filter(|path| path.starts_with("backlog/") || path.starts_with(".claude/evals/"))
        .cloned()
        .collect();
    assert_eq!(written, Vec::<String>::new());
    assert_eq!(files_of(dir.path()), MINIMAL_FILES);
}
