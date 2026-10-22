//! The workspaces leave git (spec 7; case 7; HR-213).
//! `init` writes the `.houserules/` ignore line, `check-knowledge` names a
//! `verify` path that git ignores and an unignored default workspaces folder,
//! and a fresh clone of an install that ignores its workspaces passes. Each
//! case runs the compiled binary against a scratch install that
//! `houserules init` made.

mod tracker_support;

use std::fs;
use std::path::Path;

use serde_json::json;
use tempfile::TempDir;
use tracker_support::{
    add_submodule, commit_all, edit_marker, git, run, run_ok, scratch_git_repo, stderr, stdout,
    write_json,
};

/// The line of `check-knowledge` for the default location of the workspaces
/// when the folder exists and git does not ignore it (spec 7).
const DEFAULT_LOCATION_LINE: &str = ".houserules.json: workspaces path \".houserules/work\" is not ignored by git; add .houserules/ to .gitignore";

/// A file of a batch workspace at the default location.
const WORKSPACE_FILE: &str = ".houserules/work/2026-10-21-batch-1/capture.txt";

/// The start of the line of `check-knowledge` for the `verify` paths when git
/// fails inside a work tree: the ignore test did not run, and the line names
/// the failure of git (spec 7). The rest of the line is the text of git.
const VERIFY_FAULT_START: &str =
    "knowledge: verify paths not tested against .gitignore: git check-ignore failed: fatal: ";

/// The start of the line of `check-knowledge` for the default location of the
/// workspaces when git fails inside a work tree.
const DEFAULT_LOCATION_FAULT_START: &str = ".houserules.json: workspaces path \".houserules/work\" not tested against .gitignore: git check-ignore failed: fatal: ";

/// The line of `check-knowledge` for a `verify` path that git ignores (spec 7).
fn ignored_line(path: &str) -> String {
    format!(
        "knowledge/team.json team.evidence: verify path \"{path}\" is ignored by git; commit a copy under a tracked path and verify that"
    )
}

/// The line of `check-knowledge` for a `verify` path that does not exist.
fn missing_line(path: &str) -> String {
    format!("knowledge/team.json team.evidence: verify path \"{path}\" does not exist")
}

/// A scratch install that `houserules init` made.
fn install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    dir
}

/// Writes `.gitignore` of `dir` whole, so that a case does not depend on the
/// line that `init` wrote.
fn write_gitignore(dir: &Path, text: &str) {
    fs::write(dir.join(".gitignore"), text).expect("write .gitignore");
}

/// Writes `relative` below `dir` with `text`, creating its directories.
fn write(dir: &Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("create parents");
    fs::write(path, text).expect("write the file");
}

/// Writes the local topic file `knowledge/team.json` with the entry
/// `team.evidence`, which verifies `paths`, and renders the install.
fn add_evidence(dir: &Path, paths: &[&str]) {
    write_json(
        dir,
        "knowledge/team.json",
        &json!({
            "$schema": "./schema.json",
            "topic": "team",
            "title": "Team rules",
            "entries": [{
                "id": "team.evidence",
                "kind": "gotcha",
                "area": "global",
                "summary": "The capture shows the behavior.",
                "body": ["Read the capture."],
                "tags": ["evidence"],
                "source": {"date": "2026-10-22", "by": "user"},
                "verify": paths,
            }],
        }),
    );
    run_ok(dir, &["render"]);
}

/// The exit code and the stderr lines of `check-knowledge`.
fn check_knowledge(dir: &Path) -> (i32, Vec<String>) {
    let output = run(dir, &["check-knowledge"]);
    (
        output.status.code().expect("an exit code"),
        stderr(&output).lines().map(str::to_string).collect(),
    )
}

/// Requires `check-knowledge` to fail with exactly the stderr lines `expected`.
fn assert_fails_with(dir: &Path, expected: &[String], context: &str) {
    let (code, lines) = check_knowledge(dir);
    assert_eq!((code, lines.as_slice()), (1, expected), "{context}");
}

/// Requires `check-knowledge` to pass.
fn assert_passes(dir: &Path, context: &str) {
    let output = run(dir, &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (Some(0), "knowledge: ok\n".to_string(), String::new()),
        "{context}"
    );
}

/// Commits the state of `dir` and clones it into a new directory, as CI does.
/// A `.gitattributes` of `* -text` goes in first, so that a checkout with
/// `core.autocrlf` set leaves the files as they were written.
fn commit_and_clone(dir: &Path) -> TempDir {
    write(dir, ".gitattributes", "* -text\n");
    commit_all(dir, "chore: install");
    let clone = tempfile::tempdir().expect("tempdir");
    let source = dir.to_str().expect("a UTF-8 path");
    let target = clone.path().to_str().expect("a UTF-8 path");
    git(dir, &["clone", "-q", source, target]);
    clone
}

// ---- the ignore line that init writes ----

/// Spec 7: `init` prints `wrote .gitignore` for a new file,
/// `merged .gitignore (.houserules/ added)` when it appends the line, and
/// `kept .gitignore (.houserules/ already present)` when the file holds it.
#[test]
fn init_prints_the_ignore_line_message_of_each_state_of_gitignore() {
    let fresh = scratch_git_repo();
    let output = run_ok(fresh.path(), &["init"]);
    assert!(
        stdout(&output)
            .lines()
            .any(|line| line == "wrote .gitignore"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        fs::read_to_string(fresh.path().join(".gitignore")).expect("read .gitignore"),
        ".houserules/\n"
    );

    let lacking = scratch_git_repo();
    write_gitignore(lacking.path(), "target/\n");
    let output = run_ok(lacking.path(), &["init"]);
    assert!(
        stdout(&output)
            .lines()
            .any(|line| line == "merged .gitignore (.houserules/ added)"),
        "{}",
        stdout(&output)
    );

    let output = run_ok(lacking.path(), &["init"]);
    assert!(
        stdout(&output)
            .lines()
            .any(|line| line == "kept .gitignore (.houserules/ already present)"),
        "{}",
        stdout(&output)
    );
}

// ---- the default location of the workspaces ----

/// Spec 7: the folder `.houserules/work` exists, the marker sets no
/// `workspaces` key, and git does not ignore the folder: `check-knowledge`
/// names it.
#[test]
fn an_unignored_default_location_is_named() {
    let dir = install();
    write_gitignore(dir.path(), "target/\n");
    write(dir.path(), WORKSPACE_FILE, "capture\n");

    assert_fails_with(
        dir.path(),
        &[DEFAULT_LOCATION_LINE.to_string()],
        "an existing, unignored default location",
    );
}

/// Spec 7: an install whose `.gitignore` lost `.houserules/` and kept only the
/// older line `.houserules/step/` is named too: that line ignores the output
/// of the step and not the workspaces.
#[test]
fn the_older_step_line_does_not_ignore_the_default_location() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/step/\n");
    write(dir.path(), WORKSPACE_FILE, "capture\n");

    assert_fails_with(
        dir.path(),
        &[DEFAULT_LOCATION_LINE.to_string()],
        "the step line does not ignore the workspaces",
    );
}

/// An absent folder holds nothing git could track, so a fresh clone, and an
/// install that never made a workspace, get no line.
#[test]
fn an_absent_default_location_gets_no_line() {
    let dir = install();
    write_gitignore(dir.path(), "target/\n");

    assert_passes(dir.path(), "no folder .houserules/work");
}

/// An ignored default location gets no line, with the folder present, whether
/// the line ignores `.houserules/` or only the folder as a directory pattern.
#[test]
fn an_ignored_default_location_gets_no_line() {
    for ignore in [
        ".houserules/\n",
        ".houserules/work/\n",
        ".houserules/work\n",
    ] {
        let dir = install();
        write_gitignore(dir.path(), ignore);
        write(dir.path(), WORKSPACE_FILE, "capture\n");

        assert_passes(dir.path(), ignore);
    }
}

/// A location that the project sets with `workspaces` is the project's choice,
/// tracked or ignored: it gets no line.
#[test]
fn a_location_set_with_workspaces_gets_no_line() {
    let dir = install();
    write_gitignore(dir.path(), "target/\n");
    edit_marker(dir.path(), |marker| {
        marker.insert("workspaces".to_string(), json!(".superpowers/sdd"));
    });
    write(
        dir.path(),
        ".superpowers/sdd/2026-10-21-batch-1/a.txt",
        "a\n",
    );
    write(dir.path(), WORKSPACE_FILE, "capture\n");

    assert_passes(dir.path(), "a tracked location set with workspaces");

    write_gitignore(dir.path(), ".superpowers/\n");
    assert_passes(dir.path(), "an ignored location set with workspaces");
}

/// A fresh clone of an install whose `.gitignore` holds `.houserules/`, with no
/// `workspaces` key and no folder `.houserules/work`, passes.
#[test]
fn a_fresh_clone_that_ignores_the_default_location_passes() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    write(dir.path(), WORKSPACE_FILE, "capture\n");
    let clone = commit_and_clone(dir.path());

    assert!(!clone.path().join(".houserules/work").exists());
    assert_passes(clone.path(), "a fresh clone");
}

// ---- the verify paths ----

/// Spec case 7: a reference-workflow install that ignores its workspaces and an
/// entry that verifies a file in one. In a fresh clone the file is absent. The
/// line says that git ignores the path, in place of the line about the missing
/// path.
#[test]
fn a_fresh_clone_names_a_verify_path_into_an_ignored_workspace_as_ignored() {
    let dir = scratch_git_repo();
    run_ok(
        dir.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--add-required-packs",
        ],
    );
    write_gitignore(dir.path(), ".houserules/\n");
    write(dir.path(), WORKSPACE_FILE, "capture\n");
    add_evidence(dir.path(), &[WORKSPACE_FILE]);
    let clone = commit_and_clone(dir.path());

    assert_fails_with(
        clone.path(),
        &[ignored_line(WORKSPACE_FILE)],
        "a fresh clone: the file is absent, and the line is the ignored line alone",
    );
}

/// Spec 7: on the machine that holds the workspace the file exists, and git
/// still ignores it: the line is the same.
#[test]
fn a_verify_path_into_an_ignored_workspace_that_exists_is_named() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    write(dir.path(), WORKSPACE_FILE, "capture\n");
    add_evidence(dir.path(), &[WORKSPACE_FILE]);

    assert_fails_with(
        dir.path(),
        &[ignored_line(WORKSPACE_FILE)],
        "the file exists and git ignores it",
    );
}

/// Spec 7: a committed copy under a tracked path passes, and a file that an
/// ignore pattern matches but git tracks is no ignored path.
#[test]
fn a_committed_copy_and_a_tracked_file_that_a_pattern_matches_pass() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n*.log\n");
    write(dir.path(), "tests/fixtures/capture.txt", "capture\n");
    write(dir.path(), "tests/fixtures/run.log", "log\n");
    git(dir.path(), &["add", "-f", "tests/fixtures/run.log"]);
    add_evidence(
        dir.path(),
        &["tests/fixtures/capture.txt", "tests/fixtures/run.log"],
    );
    commit_all(dir.path(), "chore: install");

    assert_passes(dir.path(), "a committed copy and a tracked file");
}

/// A `verify` path that does not exist and that git does not ignore gets the
/// line about the missing path.
#[test]
fn a_missing_verify_path_that_git_does_not_ignore_keeps_the_missing_line() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    add_evidence(dir.path(), &["tests/fixtures/absent.txt"]);

    assert_fails_with(
        dir.path(),
        &[missing_line("tests/fixtures/absent.txt")],
        "a missing path that is not ignored",
    );
}

/// Spec 7: outside a git work tree the ignore test does not run, so a file that
/// a `.gitignore` names still passes when it exists.
#[test]
fn outside_a_git_work_tree_the_ignore_test_does_not_run() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    write(dir.path(), WORKSPACE_FILE, "capture\n");
    add_evidence(dir.path(), &[WORKSPACE_FILE]);
    fs::remove_dir_all(dir.path().join(".git")).expect("remove .git");

    assert_passes(dir.path(), "no work tree");
}

/// `git check-ignore` refuses a path inside a submodule and ends the whole
/// call, so the check never asks it about one. Such a path is no ignored path,
/// and it takes the verdict of no other path with it, in either order.
#[test]
fn a_verify_path_in_a_submodule_does_not_hide_an_ignored_one() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    add_submodule(dir.path(), "vendor", "s.txt");
    commit_all(dir.path(), "chore: install");
    write(dir.path(), WORKSPACE_FILE, "capture\n");

    add_evidence(dir.path(), &["vendor/s.txt"]);
    assert_passes(dir.path(), "a path in a submodule alone");

    for paths in [
        [WORKSPACE_FILE, "vendor/s.txt"],
        ["vendor/s.txt", WORKSPACE_FILE],
    ] {
        add_evidence(dir.path(), &paths);
        assert_fails_with(
            dir.path(),
            &[ignored_line(WORKSPACE_FILE)],
            &format!("an ignored path beside a path in a submodule: {paths:?}"),
        );
    }
}

/// Overwrites the index of the repository at `dir` with text that is no index:
/// every git command that reads the index then fails inside the work tree.
fn corrupt_the_index(dir: &Path) {
    fs::write(dir.join(".git/index"), "no index\n").expect("overwrite the index");
}

/// Spec 7: the ignore test is skipped outside a git work tree and nowhere
/// else. A git that fails inside a work tree gives one line that says what was
/// not tested, with the text of git; the `verify` path is not read as unignored.
#[test]
fn a_git_fault_inside_a_work_tree_is_named_for_the_verify_paths() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    write(dir.path(), "tests/fixtures/capture.txt", "capture\n");
    add_evidence(dir.path(), &["tests/fixtures/capture.txt"]);
    commit_all(dir.path(), "chore: install");
    corrupt_the_index(dir.path());

    let (code, lines) = check_knowledge(dir.path());

    assert_eq!(code, 1, "{lines:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with(VERIFY_FAULT_START), "{lines:?}");
}

/// The same fault gives one line for the default location of the workspaces,
/// where the folder exists, and no line where the folder is absent: a test
/// that asks git nothing runs no git.
#[test]
fn a_git_fault_inside_a_work_tree_is_named_for_the_default_location() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    commit_all(dir.path(), "chore: install");
    corrupt_the_index(dir.path());
    assert_passes(dir.path(), "no folder and no verify path: git is not asked");

    write(dir.path(), WORKSPACE_FILE, "capture\n");
    let (code, lines) = check_knowledge(dir.path());

    assert_eq!(code, 1, "{lines:?}");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(
        lines[0].starts_with(DEFAULT_LOCATION_FAULT_START),
        "{lines:?}"
    );
}

/// A workspace that the project shares through a symlink is a real
/// layout. A symlink is a platform feature of Unix here
/// (`houserules.platform-gated-tests`). git refuses a path beyond a symlink,
/// so the check asks git about the link: the link is ignored by
/// `.houserules/`, and the `verify` path through it is named, not lost to
/// git's refusal.
#[cfg(unix)]
#[test]
fn a_verify_path_through_a_symlink_into_an_ignored_folder_is_named() {
    let dir = install();
    write_gitignore(dir.path(), ".houserules/\n");
    let shared = tempfile::tempdir().expect("tempdir");
    write(shared.path(), "2026-10-21-batch-1/capture.txt", "capture\n");
    fs::create_dir_all(dir.path().join(".houserules")).expect("create .houserules");
    std::os::unix::fs::symlink(shared.path(), dir.path().join(".houserules/work"))
        .expect("symlink the shared workspaces");
    add_evidence(dir.path(), &[WORKSPACE_FILE]);

    assert_fails_with(
        dir.path(),
        &[ignored_line(WORKSPACE_FILE)],
        "a path through a symlink",
    );
}
