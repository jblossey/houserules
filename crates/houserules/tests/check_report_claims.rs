//! `check-report-claims` CLI-level tests: the usage errors, exit codes,
//! and root/report-path resolution the subcommand wrapper itself owns
//! (`report_claims.rs`'s own module doc has the checks' full account;
//! its unit tests there cover the finding-generation logic these tests
//! do not re-derive). Like `check_commit.rs`, this command has no
//! frozen-JS predecessor in the flat surface -- it was ported from a
//! dev-only `src/bin/` target with no CLI of its own beyond a positional
//! and a raw `cwd` -- so this file follows that file's structural
//! pattern (a scratch git repo, the compiled binary run as a real
//! subprocess) without claiming any parity contract, and this command
//! joins neither `argv_closure.rs`'s `COMMANDS` list nor any
//! `_parity.rs` file.

use std::path::Path;
use std::process::Command;

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of `common::houserules`, not `mod common;` itself:
/// this file needs none of that module's other helpers (`FrozenWorktree`,
/// `copy_dir_recursive`, `repo_root`), for the same reason `check_commit.
/// rs`'s own copy gives.
fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// Runs `git` in `root`, panicking with its stderr on failure.
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A scratch git repo with one commit, so a report naming it as its own
/// `self_audit`/`commits` head resolves.
fn init_scratch_repo() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.email=test@test.invalid",
            "-c",
            "user.name=Test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "seed",
        ],
    );
    let head = git(dir.path(), &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_string();
    (dir, head)
}

/// A minimal, otherwise-clean `task-report` naming `head` as its one
/// commit in both `self_audit` and `commits` -- `report_claims.rs`'s own
/// `base_report` test fixture, reconstructed here since this file
/// compiles as its own crate and cannot import that module's `#[cfg(test)]`
/// helpers.
fn base_report_json(head: &str) -> String {
    format!(
        r#"{{
            "kind": "task-report",
            "self_audit": {{
                "summary": {{"base": "{head}", "head": "{head}", "deterministic": 0, "pass": 0, "fail": 0, "warn": 0, "skipped": 0, "judged": 0}},
                "rows": []
            }},
            "implemented": "nothing checkable here",
            "commits": [{{"sha": "{head}", "subject": "seed"}}],
            "self_review": [],
            "tests": [],
            "live_run": [],
            "tdd": [],
            "fix_rounds": []
        }}"#
    )
}

/// No `REPORT_PATH` positional: clap's own required-argument usage error,
/// exit 2 -- the missing-argument case this command's own module doc
/// notes now belongs to clap rather than a hand-written `usage:` line
/// (`report_claims.rs`'s port notes).
#[test]
fn no_report_path_is_claps_own_required_argument_error_exit_2() {
    let (dir, _head) = init_scratch_repo();
    let output = houserules()
        .arg("check-report-claims")
        .current_dir(dir.path())
        .output()
        .expect("run check-report-claims");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("the following required arguments were not provided"),
        "got: {stderr:?}"
    );
    assert!(stderr.contains("REPORT_PATH"), "got: {stderr:?}");
}

/// A clean report: exit 0, the as-typed report path echoed in the
/// success line, nothing on stderr.
#[test]
fn a_clean_report_prints_no_claim_mismatches_and_exits_0() {
    let (dir, head) = init_scratch_repo();
    std::fs::write(dir.path().join("report.json"), base_report_json(&head)).unwrap();
    let output = houserules()
        .args(["check-report-claims", "report.json"])
        .current_dir(dir.path())
        .output()
        .expect("run check-report-claims");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"report.json: no claim mismatches found\n");
    assert_eq!(output.stderr, b"");
}

/// A report whose only commit is a fabricated sha: exit 1, one named
/// mismatch line on stderr, nothing on stdout.
#[test]
fn a_report_with_a_mismatch_prints_one_line_and_exits_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.email=test@test.invalid",
            "-c",
            "user.name=Test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "seed",
        ],
    );
    std::fs::write(
        dir.path().join("report.json"),
        base_report_json("deadbeef1"),
    )
    .unwrap();
    let output = houserules()
        .args(["check-report-claims", "report.json"])
        .current_dir(dir.path())
        .output()
        .expect("run check-report-claims");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "got: {stderr:?}");
    assert!(
        stderr.contains("deadbeef1"),
        "the mismatch names the bad sha, got: {stderr:?}"
    );
}

/// A report carrying the paste-run lint's placeholder shape: exit 1,
/// naming the placeholder -- the same lint `report_claims.rs`'s own unit
/// tests exercise directly, proven once more end to end through the real
/// compiled binary.
#[test]
fn a_report_with_an_unpasteable_command_field_prints_the_lint_finding_and_exits_1() {
    let (dir, head) = init_scratch_repo();
    let mut report: serde_json::Value = serde_json::from_str(&base_report_json(&head)).unwrap();
    report["tests"] = serde_json::json!([
        {"command": "houserules check-report-claims <REPORT_FILE>", "output": ""}
    ]);
    std::fs::write(
        dir.path().join("report.json"),
        serde_json::to_string(&report).unwrap(),
    )
    .unwrap();
    let output = houserules()
        .args(["check-report-claims", "report.json"])
        .current_dir(dir.path())
        .output()
        .expect("run check-report-claims");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(
        stderr,
        "tests[0]: command carries the placeholder \"<REPORT_FILE>\" and cannot paste-run as written\n"
    );
}

/// `--dir` resolves the artifact/git root independently of `report_path`,
/// which always resolves against the real current directory: a report
/// living outside the `--dir` root still loads and still checks its
/// commit shas against that other root.
#[test]
fn dir_resolves_the_root_independently_of_the_report_paths_own_location() {
    let (root_dir, head) = init_scratch_repo();
    let report_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        report_dir.path().join("report.json"),
        base_report_json(&head),
    )
    .unwrap();
    let output = houserules()
        .args([
            "check-report-claims",
            "report.json",
            "--dir",
            root_dir.path().to_str().unwrap(),
        ])
        .current_dir(report_dir.path())
        .output()
        .expect("run check-report-claims");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"report.json: no claim mismatches found\n");
}

/// No `--dir` and no enclosing git repository: `crate::root::resolve_root`
/// falls back to `repo_root_from_cwd`, fails the same way every other
/// flat-surface command's own git-resolution leg does (`check_parity.rs`'s
/// `check_knowledge_outside_a_git_repository_prints_a_named_error_and_exits_2`,
/// mirrored here) -- one named, non-empty stderr line, exit 2. Not an
/// exact-text pin: the line is git's own stderr, whose wording this crate
/// does not own.
#[test]
fn outside_a_git_repository_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("report.json"), "{}").unwrap();
    let output = houserules()
        .args(["check-report-claims", "report.json"])
        .current_dir(dir.path())
        .output()
        .expect("run check-report-claims");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "got: {stderr:?}");
    assert!(!stderr.trim().is_empty(), "the error line is not empty");
}

/// A report path that does not exist: exit 2, one named line citing the
/// path.
#[test]
fn a_missing_report_file_is_a_named_error_exit_2() {
    let (dir, _head) = init_scratch_repo();
    let output = houserules()
        .args(["check-report-claims", "does-not-exist.json"])
        .current_dir(dir.path())
        .output()
        .expect("run check-report-claims");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("does-not-exist.json"), "got: {stderr:?}");
}
