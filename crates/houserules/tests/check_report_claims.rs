//! `check-report-claims` CLI-level tests: the usage errors, exit codes,
//! and root/report-path resolution the subcommand wrapper itself owns
//! (`report_claims.rs`'s own module doc indexes the limits of the checks; the
//! unit tests of `report_claims` and its submodules cover the
//! finding-generation logic these tests do not re-derive). Like
//! `check_commit.rs`, this command has no frozen-JS predecessor in the flat
//! surface -- it was ported from a dev-only `src/bin/` target with no CLI of
//! its own beyond a positional and a raw `cwd` -- so this file follows that
//! file's structural pattern (a scratch git repo, the compiled binary run as a
//! real subprocess) without claiming any parity contract, and this command
//! joins neither `argv_closure.rs`'s `COMMANDS` list nor any `_parity.rs` file.

mod install_shape;
mod tracker_support;

use std::path::Path;
use std::process::Command;

use tracker_support::sha256_hex;

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of `common::houserules`, not `mod common;` itself:
/// this file needs none of that module's other helpers (`FrozenWorktree`,
/// `copy_dir_recursive`, `repo_root`), for the same reason `check_commit.
/// rs`'s own copy gives.
/// Sets `HOUSERULES_SKIP_SELF_UPDATE` so `update`'s self-update phase
/// (`selfupdate.rs`) never runs here -- `tests/update.rs`'s own copy of
/// this helper has the full account.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
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
    install_shape::mark_2_0(dir.path());
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
    install_shape::mark_2_0(dir.path());
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
/// falls back to `git_location_from_cwd`, fails the same way every other
/// flat-surface command's own git-resolution leg does (`check_parity.rs`'s
/// `check_knowledge_outside_a_git_repository_prints_a_named_error_and_exits_2`,
/// mirrored here) -- one named, non-empty stderr line, exit 2. The exact
/// text is pinned once for every such command, by `check_parity.rs`'s
/// `resolve_root_names_the_dir_remedy_outside_a_repository`.
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

// ---- the run checks: captures that the binary wrote (batch 36, spec 6, checks 1 to 3) ----

/// The binary under test, as the argument of a command that `capture` runs.
/// A test builds no expected path from it: `report-facts` fills each `command`
/// from the header of the capture, so a quoted Windows path needs no
/// hand-quoting here (`houserules.platform-gated-tests`).
const BINARY: &str = env!("CARGO_BIN_EXE_houserules");

/// Runs `capture` in `dir` of `command`, writing the capture to `out` (relative
/// to `dir`, whose folder exists), and returns the exit code that `capture`
/// ends with, which is the code of the command.
fn capture(dir: &Path, out: &str, command: &[&str]) -> i32 {
    let output = houserules()
        .current_dir(dir)
        .args(["capture", "--out", out, "--"])
        .args(command)
        .output()
        .expect("run capture");
    output.status.code().expect("capture ends with a code")
}

/// Runs `report-facts` over `report.json` in `dir` with `base`, and fails the
/// test unless it exits 0.
fn fill_facts(dir: &Path, base: &str) {
    let output = houserules()
        .args(["report-facts", "report.json", "--base", base])
        .current_dir(dir)
        .output()
        .expect("run report-facts");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs `check-report-claims report.json` in `dir`.
fn check_claims(dir: &Path) -> std::process::Output {
    houserules()
        .args(["check-report-claims", "report.json"])
        .current_dir(dir)
        .output()
        .expect("run check-report-claims")
}

/// Pin of the case 4 sentence "passes a report that `report-facts` filled", and
/// check 1 on a changed byte: a report whose run `report-facts` filled from a
/// capture of the binary passes, and the same report fails with the hash line
/// once a byte of the capture changed. The old checks do not see the new form,
/// so the first half passes at the base too.
#[test]
fn a_report_that_report_facts_filled_passes_and_a_changed_capture_fails() {
    let (dir, _head) = init_scratch_repo();
    std::fs::create_dir_all(dir.path().join("evidence")).unwrap();
    assert_eq!(
        capture(dir.path(), "evidence/run.txt", &[BINARY, "--version"]),
        0
    );
    std::fs::write(
        dir.path().join("report.json"),
        r#"{"kind": "task-report", "tests": [{"capture": "evidence/run.txt"}]}"#,
    )
    .unwrap();
    fill_facts(dir.path(), "HEAD");

    let output = check_claims(dir.path());
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"report.json: no claim mismatches found\n");
    assert_eq!(output.stderr, b"");

    let recorded = {
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("report.json")).unwrap())
                .unwrap();
        report["tests"][0]["sha256"].as_str().unwrap().to_string()
    };
    let mut bytes = std::fs::read(dir.path().join("evidence/run.txt")).unwrap();
    bytes.extend_from_slice(b"x");
    std::fs::write(dir.path().join("evidence/run.txt"), &bytes).unwrap();
    let actual = sha256_hex(&bytes);
    let output = check_claims(dir.path());
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "tests[0]: capture evidence/run.txt has sha256 {actual}, but the report records {recorded}; re-read the capture, then remove sha256 and run report-facts\n"
        )
    );
}

/// Check 1 on a run that holds only its `capture`: one line for each key it
/// lacks, and the command that the lines name cures all three. The test follows
/// the printed remedy: after `report-facts`, the report passes.
#[test]
fn a_run_that_lacks_its_facts_gets_a_line_for_each_key_and_report_facts_cures_it() {
    let (dir, _head) = init_scratch_repo();
    std::fs::create_dir_all(dir.path().join("evidence")).unwrap();
    assert_eq!(
        capture(dir.path(), "evidence/run.txt", &[BINARY, "--version"]),
        0
    );
    std::fs::write(
        dir.path().join("report.json"),
        r#"{"kind": "task-report", "tests": [{"capture": "evidence/run.txt"}]}"#,
    )
    .unwrap();

    let output = check_claims(dir.path());
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "tests[0]: no sha256; run houserules report-facts\n\
         tests[0]: no command; run houserules report-facts\n\
         tests[0]: no exit; run houserules report-facts\n"
    );

    fill_facts(dir.path(), "HEAD");
    let output = check_claims(dir.path());
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"report.json: no claim mismatches found\n");
    assert_eq!(output.stderr, b"");
}

/// Check 3 on captures that the binary wrote: a `natural` RED captured before
/// the commit passes, and the same RED with a header that says it ended in 2099
/// fails with the line of the spec. The RED is the binary without a command,
/// which exits 2. The test reads the committer time of the newest listed commit
/// from git and computes the expected seconds from it.
#[test]
fn a_natural_red_captured_before_the_commit_passes_and_a_later_one_fails() {
    let (dir, base) = init_scratch_repo();
    let root = dir.path();
    std::fs::create_dir_all(root.join("evidence")).unwrap();
    assert_eq!(
        capture(root, "evidence/red.txt", &[BINARY, "check-report-claims"]),
        2
    );
    assert_eq!(
        capture(root, "evidence/green.txt", &[BINARY, "--version"]),
        0
    );
    git(
        root,
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
            "second",
        ],
    );
    let draft = r#"{
        "kind": "task-report",
        "tests": [],
        "tdd": [{
            "test": "t",
            "mode": "natural",
            "red": {"capture": "evidence/red.txt"},
            "green": {"capture": "evidence/green.txt"}
        }]
    }"#;
    std::fs::write(root.join("report.json"), draft).unwrap();
    fill_facts(root, &base);
    let output = check_claims(root);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The header of the RED now says that it ended in 2099, after the commit.
    let red = std::fs::read_to_string(root.join("evidence/red.txt")).unwrap();
    let (header_line, rest) = red.split_once('\n').unwrap_or((red.as_str(), ""));
    let mut header: serde_json::Value = serde_json::from_str(header_line).unwrap();
    header["end"] = serde_json::json!("2099-01-01T00:00:00.000000000Z");
    let mut changed = header.to_string();
    if !rest.is_empty() {
        changed.push('\n');
        changed.push_str(rest);
    }
    std::fs::write(root.join("evidence/red.txt"), changed).unwrap();
    std::fs::write(root.join("report.json"), draft).unwrap();
    fill_facts(root, &base);
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("report.json")).unwrap()).unwrap();
    let listed = report["commits"][0]["sha"].as_str().unwrap();
    let committed = git(root, &["log", "-1", "--format=%ct %cI", listed]);
    let (seconds, iso) = committed.trim().split_once(' ').unwrap();
    let after = 4_070_908_800_i64 - seconds.parse::<i64>().unwrap();
    let output = check_claims(root);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "tdd[0]: mode \"natural\", but the RED capture \"evidence/red.txt\" ended {after} s after the newest listed commit {listed} (committed {iso}); label the entry \"reconstructed\"\n"
        )
    );
}

/// Case 4 and spec 12: a run of the old form fails with the remedy line, also
/// in an install that kept the old deliverables schema. The command reads no
/// schema, so the stand-in below stays unread; it states the install the test
/// stands for.
#[test]
fn a_run_of_the_old_form_fails_also_where_the_install_kept_the_old_schema() {
    let (dir, head) = init_scratch_repo();
    std::fs::create_dir_all(dir.path().join(".claude/schemas")).unwrap();
    std::fs::write(
        dir.path().join(".claude/schemas/deliverables.json"),
        r#"{"$defs": {"run": {"required": ["command", "output"]}}}"#,
    )
    .unwrap();
    let mut report: serde_json::Value = serde_json::from_str(&base_report_json(&head)).unwrap();
    report["tests"] = serde_json::json!([
        {"command": "cargo test", "output": "ok\n", "exit": 0}
    ]);
    std::fs::write(
        dir.path().join("report.json"),
        serde_json::to_string(&report).unwrap(),
    )
    .unwrap();
    let output = check_claims(dir.path());
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "tests[0]: no capture; run the command through houserules capture\n"
    );
}

// ---- the claim checks (batch 36, spec 6, checks 4 to 11) ----

/// Checks 4 to 11 of spec 6 and the rest of case 4. Each test runs the compiled
/// binary in a scratch repository and compares the whole of stderr, so a line
/// that the spec words differently, a line in another order, and a line that is
/// missing all fail. The captures come from the `capture` command of the same
/// binary; a script runs through `sh`, so the text of a test runs on Windows
/// with Git for Windows too (`houserules.platform-gated-tests`).
mod claim_checks {
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::{
        BINARY, capture, check_claims, fill_facts, houserules, init_scratch_repo, install_shape,
    };
    use crate::tracker_support::{assert_output, commit_all, sha256_hex};

    /// The success line for a report named `report.json`.
    const CLEAN: &str = "report.json: no claim mismatches found\n";

    /// The lines of `src/a.txt`, the file that most cases cite.
    const A_TXT: &str = "alpha\nbeta\ngamma\n";

    /// The end of an ephemeral-path line, after the quoted path.
    const EPHEMERAL: &str = "an ephemeral path that will not exist once the session ends";

    /// Writes `contents` to `relative` below `dir` and creates the parents.
    fn write_file(dir: &Path, relative: &str, contents: &str) {
        let path = dir.join(relative);
        fs::create_dir_all(path.parent().expect("a parent directory")).expect("create parents");
        fs::write(path, contents).expect("write the file");
    }

    /// Writes `report` as `report.json` below `dir`.
    fn write_report(dir: &Path, report: &Value) {
        write_file(
            dir,
            "report.json",
            &serde_json::to_string_pretty(report).expect("serialize the report"),
        );
    }

    /// A scratch install with one commit. `.gitattributes` stops every
    /// line-ending conversion, so a file that a test reads back holds the bytes
    /// it wrote.
    fn scratch_install() -> tempfile::TempDir {
        let (dir, _head) = init_scratch_repo();
        write_file(dir.path(), ".gitattributes", "* -text\n");
        dir
    }

    /// Runs `capture` in `dir` of `sh -c <script> <arguments>...`; the capture
    /// goes to `out`, relative to `dir`, whose folder is created. The script
    /// must exit 0.
    fn capture_script(dir: &Path, out: &str, script: &str, arguments: &[&str]) {
        fs::create_dir_all(dir.join(out).parent().expect("a parent directory"))
            .expect("create the folder of the capture");
        let mut command = vec!["sh", "-c", script];
        command.extend_from_slice(arguments);
        assert_eq!(capture(dir, out, &command), 0, "capture of {script}");
    }

    /// The finding of a hash that differs, for the citation `index` of `shown`.
    fn hash_line(index: usize, shown: &str, actual: &str, recorded: &str) -> String {
        format!(
            "citations[{index}]: {shown} has sha256 {actual} at HEAD, but the report records {recorded}; re-open the lines at HEAD, correct line and end, then remove sha256 and run report-facts\n"
        )
    }

    // ---- check 4: recorded citations ----

    /// Case 4: a recorded citation passes after `report-facts`; an edit of the
    /// working tree that no commit holds changes nothing, because the check
    /// reads the blob at HEAD; a later commit that edits a cited line fails with
    /// the hash line.
    #[test]
    fn a_cited_line_edited_by_a_later_commit_fails_on_the_hash() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", A_TXT);
        commit_all(root, "feat: add a");
        write_report(
            root,
            &json!({"kind": "task-report", "citations": [
                {"path": "src/a.txt", "line": 1, "end": 2, "anchor": "alpha"}
            ]}),
        );
        fill_facts(root, "HEAD");
        assert_output(&check_claims(root), 0, CLEAN, "");

        write_file(root, "src/a.txt", "alpha\nBETA\ngamma\n");
        assert_output(&check_claims(root), 0, CLEAN, "");

        commit_all(root, "fix: edit line two");
        assert_output(
            &check_claims(root),
            1,
            "",
            &hash_line(
                0,
                "src/a.txt:1-2",
                &sha256_hex(b"alpha\nBETA"),
                &sha256_hex(b"alpha\nbeta"),
            ),
        );
    }

    /// Case 4: a later commit moves the cited line, so that the anchor leaves
    /// the range.
    #[test]
    fn a_cited_line_that_a_later_commit_moved_leaves_its_anchor_out_of_the_range() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", A_TXT);
        commit_all(root, "feat: add a");
        write_report(
            root,
            &json!({"kind": "task-report", "citations": [
                {"path": "src/a.txt", "line": 2, "anchor": "beta"}
            ]}),
        );
        fill_facts(root, "HEAD");
        assert_output(&check_claims(root), 0, CLEAN, "");

        write_file(root, "src/a.txt", "new\nalpha\nbeta\ngamma\n");
        commit_all(root, "fix: add a line on top");
        assert_output(
            &check_claims(root),
            1,
            "",
            "citations[0]: the anchor is not in src/a.txt:2 at HEAD; re-open the file at HEAD and correct line and end\n",
        );
    }

    /// Case 4: a recorded citation of an untracked path, of a directory, and of
    /// a path with a `..` segment is not tracked at HEAD.
    #[test]
    fn a_recorded_citation_of_an_untracked_path_a_directory_and_a_parent_path_is_not_tracked() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", A_TXT);
        commit_all(root, "feat: add a");
        write_file(root, "untracked.txt", "alpha\n");
        write_report(
            root,
            &json!({"kind": "task-report", "citations": [
                {"path": "untracked.txt", "line": 1, "anchor": "alpha"},
                {"path": "src", "line": 1, "anchor": "alpha"},
                {"path": "../a.txt", "line": 1, "anchor": "alpha"},
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "citations[0]: untracked.txt is not tracked at HEAD\n\
             citations[1]: src is not tracked at HEAD\n\
             citations[2]: ../a.txt is not tracked at HEAD\n",
        );
    }

    /// Case 4: `end` before `line`, a citation past the end of the file, and an
    /// anchor `let` that occurs twice in the file each give their line.
    #[test]
    fn an_end_before_the_line_a_line_past_the_end_and_a_repeated_anchor_are_findings() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", A_TXT);
        write_file(root, "src/dup.rs", "let x = 1;\nlet y = 2;\n");
        commit_all(root, "feat: add the files");
        write_report(
            root,
            &json!({"kind": "task-report", "citations": [
                {"path": "src/a.txt", "line": 3, "end": 2, "anchor": "gamma"},
                {"path": "src/a.txt", "line": 9, "anchor": "alpha"},
                {"path": "src/dup.rs", "line": 1, "anchor": "let"},
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "citations[0]: end 2 is before line 3\n\
             citations[1]: src/a.txt has 3 lines at HEAD, fewer than 9\n\
             citations[2]: the anchor occurs 2 times in src/dup.rs at HEAD; choose text that occurs once\n",
        );
    }

    /// Spec 6, check 4: a citation that holds no `sha256` yet passes, a recorded
    /// hash of another type than a string differs and shows as written, and an
    /// entry with a line of 0 is left to `validate`.
    #[test]
    fn a_citation_without_a_hash_passes_and_a_hash_that_is_no_string_differs() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", A_TXT);
        commit_all(root, "feat: add a");
        write_report(
            root,
            &json!({"kind": "task-report", "citations": [
                {"path": "src/a.txt", "line": 1, "anchor": "alpha"},
                {"path": "src/a.txt", "line": 1, "anchor": "alpha", "sha256": 5},
                {"path": "src/a.txt", "line": 0, "anchor": "alpha"},
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            &hash_line(1, "src/a.txt:1", &sha256_hex(b"alpha"), "5"),
        );
    }

    /// Case 4: in a subdirectory install with a file of the same name at the git
    /// top level, a recorded citation is judged against the file of the install.
    #[test]
    fn a_recorded_citation_in_a_subdirectory_install_is_judged_against_its_own_file() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "a.txt", "top level file\n");
        write_file(root, "sub/a.txt", "inner one\ninner two\n");
        let install = root.join("sub");
        install_shape::mark_2_0(&install);
        commit_all(root, "chore: seed");

        write_report(
            &install,
            &json!({"kind": "task-report", "citations": [
                {"path": "a.txt", "line": 1, "end": 2, "anchor": "inner one"}
            ]}),
        );
        fill_facts(&install, "HEAD");
        assert_output(&check_claims(&install), 0, CLEAN, "");

        write_report(
            &install,
            &json!({"kind": "task-report", "citations": [
                {"path": "a.txt", "line": 1, "anchor": "top level"}
            ]}),
        );
        assert_output(
            &check_claims(&install),
            1,
            "",
            "citations[0]: the anchor is not in a.txt:1 at HEAD; re-open the file at HEAD and correct line and end\n",
        );
    }

    // ---- check 5: self-review evidence ----

    /// Spec 6, check 5: an `evidence` path that does not exist.
    #[test]
    fn a_missing_evidence_path_is_a_finding() {
        let dir = scratch_install();
        let root = dir.path();
        write_report(
            root,
            &json!({"kind": "task-report", "self_review": [
                {"claim": "The run is clean", "evidence": ["nope.txt"]}
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "self_review[0].evidence[0]: nope.txt does not exist\n",
        );
    }

    /// Case 4 and spec 6, check 5: a claim with every, all, or no remaining
    /// names a capture in its evidence, and a source file is no capture. The
    /// words count whole and in any case; "at all" and "after all" do not count.
    #[test]
    fn a_closure_claim_needs_a_capture_in_its_evidence() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", A_TXT);
        capture_script(root, "evidence/run.txt", "echo ok", &[]);
        write_report(
            root,
            &json!({"kind": "task-report", "self_review": [
                {"claim": "Every call site was checked", "evidence": ["src/a.txt"]},
                {"claim": "All call sites were checked"},
                {"claim": "There is no remaining caller"},
                {"claim": "All call sites were checked", "evidence": ["evidence/run.txt"]},
                {"claim": "Not needed at all, and fine after all"},
                {"claim": "The overall install is a ball of wax"},
                {"claim": "Checked at all call sites"},
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "self_review[0]: claims \"Every\", but its evidence names no capture\n\
             self_review[1]: claims \"All\", but its evidence names no capture\n\
             self_review[2]: claims \"no remaining\", but its evidence names no capture\n",
        );
    }

    /// Case 4 and spec 6, check 5: a claim with pin, pins, or pinned names at
    /// least one existing path; a missing path adds its own line first.
    #[test]
    fn a_pin_claim_needs_an_existing_path_in_its_evidence() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", A_TXT);
        write_report(
            root,
            &json!({"kind": "task-report", "self_review": [
                {"claim": "The test pins the exit code"},
                {"claim": "The test pins the exit code", "evidence": ["src/a.txt"]},
                {"claim": "The output is pinned", "evidence": ["nope.txt"]},
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "self_review[0]: claims \"pins\", but its evidence names no existing path\n\
             self_review[2].evidence[0]: nope.txt does not exist\n\
             self_review[2]: claims \"pinned\", but its evidence names no existing path\n",
        );
    }

    /// Spec 6, check 5: a finding is one line, whatever the claim holds. The
    /// phrase no remaining is quoted as the claim spells it, with each run of
    /// white space (a line break, a carriage return and a tab, a line
    /// separator, a byte order mark, two spaces) as one space.
    #[test]
    fn a_closure_phrase_over_white_space_is_quoted_on_one_line() {
        let dir = scratch_install();
        let root = dir.path();
        write_report(
            root,
            &json!({"kind": "task-report", "self_review": [
                {"claim": "There is no\nremaining caller"},
                {"claim": "No  Remaining caller"},
                {"claim": "There is no\r\n\tremaining caller"},
                {"claim": "There is no\u{2028}remaining caller"},
                {"claim": "There is no\u{feff}remaining caller"},
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "self_review[0]: claims \"no remaining\", but its evidence names no capture\n\
             self_review[1]: claims \"No Remaining\", but its evidence names no capture\n\
             self_review[2]: claims \"no remaining\", but its evidence names no capture\n\
             self_review[3]: claims \"no remaining\", but its evidence names no capture\n\
             self_review[4]: claims \"no remaining\", but its evidence names no capture\n",
        );
    }

    /// Spec 6, check 5: a claim with a closure word and a pin word gets the
    /// closure line first.
    #[test]
    fn a_claim_with_a_closure_word_and_a_pin_word_gets_both_lines_closure_first() {
        let dir = scratch_install();
        let root = dir.path();
        write_report(
            root,
            &json!({"kind": "task-report", "self_review": [
                {"claim": "Every case is pinned"}
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "self_review[0]: claims \"Every\", but its evidence names no capture\n\
             self_review[0]: claims \"pinned\", but its evidence names no existing path\n",
        );
    }

    /// Case 4 and spec 6, check 8: the narrative checks read the `claim` of a
    /// `self_review` item, so a stale sha in a claim is a finding.
    #[test]
    fn a_stale_sha_in_a_self_review_claim_is_a_finding() {
        let dir = scratch_install();
        let root = dir.path();
        write_report(
            root,
            &json!({"kind": "task-report", "self_review": [
                {"claim": "see commit deadbeef1 for context"}
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "self_review[0].claim: \"deadbeef1\" looks like a commit sha but does not resolve to one\n",
        );
    }

    // ---- check 6: re-run probes ----

    /// Case 4 and spec 6, check 6: a `reran` path must exist and lie in the
    /// folder that holds the report or below it. A `..` segment cannot lead out
    /// of it. The report is given by its absolute path, so that the line names
    /// the folder as the test builds it.
    #[test]
    fn a_rerun_probe_must_exist_inside_the_folder_of_the_report() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "ws/probe.txt", "probe\n");
        write_file(root, "outside.txt", "outside\n");
        let report = root.join("ws").join("report.json");
        fs::write(
            &report,
            serde_json::to_string_pretty(&json!({"kind": "task-report", "fix_rounds": [{
                "round": 1,
                "findings": [
                    {"finding": "first", "fix": "fixed", "reran": "ws/probe.txt"},
                    {"finding": "second", "fix": "fixed", "reran": "ws/missing.txt"},
                    {"finding": "third", "fix": "fixed", "reran": "outside.txt"},
                    {"finding": "fourth", "fix": "fixed", "reran": "ws/../outside.txt"},
                    {"finding": "fifth", "fix": "fixed"},
                ],
            }]}))
            .expect("serialize the report"),
        )
        .expect("write the report");
        let output = houserules()
            .arg("check-report-claims")
            .arg(&report)
            .current_dir(root)
            .output()
            .expect("run check-report-claims");
        let workspace = root.join("ws");
        assert_output(
            &output,
            1,
            "",
            &format!(
                "fix_rounds[0].findings[1].reran: ws/missing.txt does not exist\n\
                 fix_rounds[0].findings[2].reran: outside.txt is not in the workspace {workspace}\n\
                 fix_rounds[0].findings[3].reran: ws/../outside.txt is not in the workspace {workspace}\n",
                workspace = workspace.display()
            ),
        );
    }

    // ---- checks 5 and 6: a path that the system refuses to look up ----

    /// Spec 6, checks 5 and 6 (design.md 219): only a path that is not found
    /// reads as `does not exist`. Any other failure of the lookup reads `could
    /// not be read` and quotes the error of the operating system. A path with
    /// a NUL byte fails the lookup on every platform and for every user; the
    /// expected error comes from the call that the check makes.
    #[test]
    fn a_path_that_fails_the_lookup_for_another_cause_than_absence_quotes_the_error() {
        let dir = scratch_install();
        let root = dir.path();
        let path = "a\0b.txt";
        let metadata_error = fs::metadata(root.join(path)).expect_err("a NUL byte names no file");
        let canonical_error =
            fs::canonicalize(root.join(path)).expect_err("a NUL byte names no file");
        write_report(
            root,
            &json!({"kind": "task-report",
                "self_review": [{"claim": "The test pins the exit code", "evidence": [path]}],
                "fix_rounds": [{"round": 1, "findings": [
                    {"finding": "first", "fix": "fixed", "reran": path},
                ]}],
            }),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            &format!(
                "self_review[0].evidence[0]: {path} could not be read ({metadata_error})\n\
                 self_review[0]: claims \"pins\", but its evidence names no existing path\n\
                 fix_rounds[0].findings[0].reran: {path} could not be read ({canonical_error})\n"
            ),
        );
    }

    /// Spec 6, checks 5 and 6 (design.md 219): a path below a folder that
    /// refuses the lookup (mode 000) reads `could not be read` with the error of
    /// the operating system, where the path is not found for another reader.
    /// A user who reads any folder (root) finds the path missing instead and
    /// cannot set the case up, so the test ends there. The expected error comes
    /// from the call that the check makes.
    #[cfg(unix)]
    #[test]
    fn a_path_below_a_folder_that_refuses_the_lookup_is_not_reported_as_absent() {
        use std::io::ErrorKind;
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_install();
        let root = dir.path();
        write_report(
            root,
            &json!({"kind": "task-report",
                "self_review": [{"claim": "The test pins the exit code", "evidence": ["locked/x.txt"]}],
                "fix_rounds": [{"round": 1, "findings": [
                    {"finding": "first", "fix": "fixed", "reran": "locked/x.txt"},
                ]}],
            }),
        );
        let locked = root.join("locked");
        fs::create_dir(&locked).expect("create the folder");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("lock the folder");
        let target = locked.join("x.txt");
        let refused =
            |error: std::io::Error| Some(error).filter(|e| e.kind() != ErrorKind::NotFound);
        let (Some(metadata_error), Some(canonical_error)) = (
            fs::metadata(&target).err().and_then(refused),
            fs::canonicalize(&target).err().and_then(refused),
        ) else {
            fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
            return;
        };
        let output = check_claims(root);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock the folder");
        assert_output(
            &output,
            1,
            "",
            &format!(
                "self_review[0].evidence[0]: locked/x.txt could not be read ({metadata_error})\n\
                 self_review[0]: claims \"pins\", but its evidence names no existing path\n\
                 fix_rounds[0].findings[0].reran: locked/x.txt could not be read ({canonical_error})\n"
            ),
        );
    }

    // ---- check 7: ephemeral paths ----

    /// Case 4 and spec 6, check 7: the check reads every string of the report,
    /// in the order of the report, and labels a finding with the JSON path of
    /// the string: a capture path under `/tmp/`, a claim that cites a
    /// `scratchpad/` path, and a `docs_verified` source under `/tmp/`. The
    /// capture does not exist, so check 1 names it first.
    #[test]
    fn an_ephemeral_path_in_any_string_of_the_report_is_a_finding() {
        let dir = scratch_install();
        let root = dir.path();
        write_report(
            root,
            &json!({
                "kind": "task-report",
                "docs_verified": [{"api": "serde", "source": "/tmp/docs/serde.txt"}],
                "self_review": [{"claim": "The probe sits in scratchpad/probe for now"}],
                "tests": [{"capture": "/tmp/claude/run.txt"}],
            }),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            &format!(
                "tests[0]: capture /tmp/claude/run.txt does not exist\n\
                 docs_verified[0].source: cites \"/tmp/docs/serde.txt\", {EPHEMERAL}\n\
                 self_review[0].claim: cites \"scratchpad/probe\", {EPHEMERAL}\n\
                 tests[0].capture: cites \"/tmp/claude/run.txt\", {EPHEMERAL}\n"
            ),
        );
    }

    /// Spec 6, check 7: the literal `command`, the `excerpt`, the `anchor` of a
    /// citation, the hashes, and the whole `self_audit` hold a scratch path
    /// without a finding. One string beside them is a finding, so that the test
    /// fails when the check reads nothing and when it reads too much.
    #[test]
    fn the_command_the_excerpt_the_anchor_the_hashes_and_the_self_audit_are_not_read() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/a.txt", "see /tmp/anchor here\n");
        commit_all(root, "feat: add a");
        capture_script(root, "evidence/run.txt", "echo /tmp/in-output", &[]);
        write_report(
            root,
            &json!({
                "kind": "task-report",
                "docs_verified": [{"api": "serde", "source": "/tmp/docs/serde.txt"}],
                "tests": [{"capture": "evidence/run.txt", "excerpt": "/tmp/in-output"}],
                "citations": [{"path": "src/a.txt", "line": 1, "anchor": "/tmp/anchor"}],
                "self_audit": {
                    "summary": {"note": "/tmp/audit-note"},
                    "rows": [{"evidence": "scratchpad/audit-row"}],
                },
            }),
        );
        fill_facts(root, "HEAD");
        let report: Value = serde_json::from_slice(&fs::read(root.join("report.json")).unwrap())
            .expect("parse the report");
        assert!(
            report["tests"][0]["command"]
                .as_str()
                .is_some_and(|command| command.contains("/tmp/in-output")),
            "the filled command holds the scratch path: {report}"
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            &format!("docs_verified[0].source: cites \"/tmp/docs/serde.txt\", {EPHEMERAL}\n"),
        );
    }

    // ---- check 9: comma lists of line numbers ----

    /// Case 4 and spec 6.9: every element of a comma or semicolon list is
    /// checked against a 10-line file. A tight element always continues the
    /// list. An element after one space continues it only when it ends the text
    /// or a separator, a closing bracket, or a sentence-ending period follows
    /// it, so `backlog_parity.rs:6, 42 total` reads one citation.
    #[test]
    fn every_element_of_a_list_of_line_numbers_is_checked() {
        let dir = scratch_install();
        let root = dir.path();
        let ten: String = (1..=10).map(|n| format!("line {n}\n")).collect();
        let twenty: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        for name in ["file.rs", "README.md", "backlog_parity.rs"] {
            write_file(root, name, &ten);
        }
        write_file(root, "big.rs", &twenty);
        commit_all(root, "feat: add the files");
        let cases = [
            (
                "file.rs:3,999 shows",
                Some(("file.rs:3,999", "file.rs", 10)),
            ),
            (
                "README.md:3,999:",
                Some(("README.md:3,999", "README.md", 10)),
            ),
            (
                "see README.md:3, 999.",
                Some(("README.md:3, 999", "README.md", 10)),
            ),
            (
                "file.rs:3;999 shows",
                Some(("file.rs:3;999", "file.rs", 10)),
            ),
            (
                "file.rs:2,5-999 shows",
                Some(("file.rs:2,5-999", "file.rs", 10)),
            ),
            ("backlog_parity.rs:6, 42 total", None),
            ("big.rs:12, 400 lines", None),
            ("file.rs:3,4 shows", None),
        ];
        for (text, finding) in cases {
            write_report(root, &json!({"kind": "task-report", "implemented": text}));
            let output = check_claims(root);
            match finding {
                Some((token, path, lines)) => assert_output(
                    &output,
                    1,
                    "",
                    &format!("implemented: cites \"{token}\", but {path} has only {lines} lines\n"),
                ),
                None => assert_output(&output, 0, CLEAN, ""),
            }
        }
    }

    // ---- checks 10 and 11: counts and sweeps beside a capture ----

    /// Case 4 and spec 6.10: a count in the body of a listed commit, beside a
    /// cited capture, is compared with the output of the capture. The line is
    /// labelled `commits[<i>] <sha> body`.
    #[test]
    fn a_commit_body_count_that_a_cited_capture_does_not_hold_is_a_finding() {
        let dir = scratch_install();
        let root = dir.path();
        capture_script(root, "evidence/run.txt", "echo 11 passed", &[]);
        let sha = commit_all(
            root,
            "feat: run the suite\n\nThe suite ran: 12 passed (evidence/run.txt).",
        );
        write_report(
            root,
            &json!({"kind": "task-report", "commits": [
                {"sha": sha, "subject": "feat: run the suite"}
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            &format!(
                "commits[0] {sha} body: \"12 passed\" cites \"evidence/run.txt\", which does not contain \"12\"\n"
            ),
        );
    }

    /// Case 4 and spec 6.10: in a commit body a citation counts only when it
    /// resolves to a capture. A source file and a bare file name that names no
    /// file at the install root are skipped, so the counts beside them pass.
    #[test]
    fn a_commit_body_count_beside_a_source_file_or_an_unresolved_name_passes() {
        let dir = scratch_install();
        let root = dir.path();
        write_file(root, "src/lib.rs", "fn main() {}\n");
        capture_script(root, "evidence/run.txt", "echo 11 passed", &[]);
        let source = commit_all(root, "fix: y\n\n7 passed beside src/lib.rs.");
        let bare = commit_all(root, "chore: z\n\nThe suite ran: 12 passed (run.txt).");
        write_report(
            root,
            &json!({"kind": "task-report", "commits": [
                {"sha": source, "subject": "fix: y"},
                {"sha": bare, "subject": "chore: z"},
            ]}),
        );
        assert_output(&check_claims(root), 0, CLEAN, "");
    }

    /// Spec 6.10: in a commit body a bare file name counts when it names a
    /// capture at the install root, as a path does. The suite ran 12 tests
    /// beside `run.txt`, whose output says 11.
    #[test]
    fn a_commit_body_count_beside_a_bare_name_that_names_a_capture_at_the_root_is_checked() {
        let dir = scratch_install();
        let root = dir.path();
        capture_script(root, "run.txt", "echo 11 passed", &[]);
        let sha = commit_all(root, "chore: z\n\nThe suite ran: 12 passed (run.txt).");
        write_report(
            root,
            &json!({"kind": "task-report", "commits": [
                {"sha": sha, "subject": "chore: z"}
            ]}),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            &format!(
                "commits[0] {sha} body: \"12 passed\" cites \"run.txt\", which does not contain \"12\"\n"
            ),
        );
    }

    /// Spec 6.10: a cited capture is read without its header line. The header
    /// of the capture holds the argument 7777, and the output does not, so the
    /// count of the narrative finds no match.
    #[test]
    fn a_narrative_count_is_compared_with_the_output_of_a_capture_and_not_its_header() {
        let dir = scratch_install();
        let root = dir.path();
        capture_script(root, "evidence/run.txt", "echo 11 passed", &["7777"]);
        write_report(
            root,
            &json!({
                "kind": "task-report",
                "implemented": "The suite ran: 7777 passed (evidence/run.txt).",
            }),
        );
        assert_output(
            &check_claims(root),
            1,
            "",
            "implemented: \"7777 passed\" cites \"evidence/run.txt\", which does not contain \"7777\"\n",
        );
    }

    /// Case 4 and spec 6.11: a sweep citation whose capture names the changed
    /// file only in its header fails; the same sweep passes when its output
    /// names the file.
    #[test]
    fn a_sweep_capture_that_names_the_changed_file_only_in_its_header_fails() {
        let dir = scratch_install();
        let root = dir.path();
        capture_script(
            root,
            "evidence/sweep.txt",
            "echo clean",
            &["src/changed.rs"],
        );
        let report = json!({
            "kind": "task-report",
            "files_changed": ["src/changed.rs"],
            "implemented": "The retained sweep (evidence/sweep.txt) is clean.",
        });
        write_report(root, &report);
        assert_output(
            &check_claims(root),
            1,
            "",
            "implemented: cites \"evidence/sweep.txt\" as a sweep, but its retained content does not name \"src/changed.rs\", a file this report's own files_changed lists\n",
        );

        fs::remove_file(root.join("evidence/sweep.txt")).expect("remove the capture");
        capture_script(root, "evidence/sweep.txt", "echo src/changed.rs", &[]);
        assert_output(&check_claims(root), 0, CLEAN, "");
    }

    // ---- captures outside the install root ----

    /// Case 4: a capture cited by an absolute path outside the install root
    /// passes check 1. The folder lies in the temporary directory, and check 7
    /// names a path that holds `/tmp/` or a `scratchpad/` segment, so the test
    /// builds its expectation from the path: the clean line, or the one line of
    /// check 7 and none of check 1.
    #[test]
    fn a_capture_cited_by_an_absolute_path_outside_the_install_root_passes() {
        let dir = scratch_install();
        let root = dir.path();
        let outside = tempfile::Builder::new()
            .prefix("claims-outside-")
            .tempdir()
            .expect("a folder outside the install root");
        let absolute = outside.path().join("run.txt");
        let absolute = absolute.to_str().expect("a UTF-8 path");
        assert_eq!(capture(root, absolute, &[BINARY, "--version"]), 0);
        write_report(
            root,
            &json!({"kind": "task-report", "tests": [{"capture": absolute}]}),
        );
        fill_facts(root, "HEAD");
        let ephemeral = (absolute.starts_with('/') && absolute.contains("/tmp/"))
            || absolute.contains("scratchpad/");
        if ephemeral {
            assert_output(
                &check_claims(root),
                1,
                "",
                &format!("tests[0].capture: cites \"{absolute}\", {EPHEMERAL}\n"),
            );
        } else {
            assert_output(&check_claims(root), 0, CLEAN, "");
        }
    }
}
