//! `check-commit` CLI-level tests: the usage errors and exit codes
//! `cmd_check_commit` itself owns (`rules::check_commit`'s own module doc
//! has the command's full account; its unit tests there cover the
//! finding-generation logic these tests do not re-derive). `check-commit`
//! has no frozen-JS predecessor at all -- unlike every command
//! `argv_closure.rs` and the other `*_parity.rs` files pin, there is
//! nothing to compare it against, so this file follows their structural
//! pattern (a scratch git repo, the compiled binary run as a real
//! subprocess) without claiming any parity contract.

use std::fs;
use std::path::Path;
use std::process::Command;

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of `common::houserules`, not `mod common;` itself: this
/// file needs none of that module's other helpers (`FrozenWorktree`,
/// `copy_dir_recursive`, `repo_root`), and pulling the module in just for
/// this one function makes the rest warn as dead code for this binary
/// specifically, since each file under `tests/` compiles as its own crate
/// (`argv_closure.rs`'s own doc names this exact tension for the same
/// reason).
fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// The vendored knowledge schema's path, resolved at compile time from the
/// crate's own manifest directory (`crates/houserules`) so it does not
/// depend on the test binary's working directory.
fn vendored_schema_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json")
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

fn write_file(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

fn commit(root: &Path, message: &str) -> String {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            message,
        ],
    );
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

/// A scratch git repo whose knowledge base declares one `commits`-type
/// check (`process.conventional-commits`'s own subject pattern) -- the
/// smallest fixture `load_base` accepts (this crate's `check.rs`/
/// `audit.rs` own module docs explain why a minimal, standalone fixture is
/// simpler here than a schema-shape probe of the real vendored one).
fn make_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    write_file(
        root,
        "knowledge/schema.json",
        &fs::read_to_string(vendored_schema_path()).expect("read the vendored knowledge schema"),
    );
    write_file(
        root,
        "knowledge/areas.json",
        r#"{"global": {"paths": []}, "process": {"paths": []}, "docs": {"paths": []}, "tools": {"paths": []}}"#,
    );
    write_file(
        root,
        "knowledge/process.json",
        r#"{
            "$schema": "./schema.json", "topic": "process", "title": "process title",
            "entries": [{
                "id": "process.conventional-commits", "kind": "rule", "area": "process",
                "standing": true, "summary": "Conventional commits.", "body": ["x"], "tags": [],
                "source": {"date": "2026-08-29", "by": "user"},
                "check": {
                    "type": "commits", "level": "fail",
                    "subject": "^(feat|fix|chore|docs|test): .+"
                }
            }]
        }"#,
    );
    write_file(root, "CLAUDE.md", "# Test\n");
    commit(root, "chore: init");
    dir
}

#[test]
fn neither_a_message_file_nor_from_is_a_named_usage_error_exit_2() {
    let dir = make_repo();
    let output = houserules()
        .args(["check-commit"])
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        output.stderr,
        b"check-commit needs a message file or --from <ref>\n"
    );
}

#[test]
fn both_a_message_file_and_from_is_a_named_usage_error_exit_2() {
    let dir = make_repo();
    let msg = dir.path().join("MSG");
    fs::write(&msg, "feat: x\n").unwrap();
    let output = houserules()
        .arg("check-commit")
        .arg(&msg)
        .args(["--from", "HEAD"])
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        output.stderr,
        b"check-commit takes a message file or --from, not both\n"
    );
}

#[test]
fn to_without_from_is_a_named_usage_error_exit_2() {
    let dir = make_repo();
    let msg = dir.path().join("MSG");
    fs::write(&msg, "feat: x\n").unwrap();
    let output = houserules()
        .arg("check-commit")
        .arg(&msg)
        .args(["--to", "HEAD"])
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"check-commit's --to needs --from\n");
}

#[test]
fn a_clean_message_file_prints_ok_and_exits_0() {
    let dir = make_repo();
    let msg = dir.path().join("MSG");
    fs::write(&msg, "feat: a clean subject\n").unwrap();
    let output = houserules()
        .arg("check-commit")
        .arg(&msg)
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"check-commit: ok\n");
    assert_eq!(output.stderr, b"");
}

#[test]
fn a_bad_message_file_prints_one_finding_and_exits_1() {
    let dir = make_repo();
    let msg = dir.path().join("MSG");
    fs::write(&msg, "bad subject\n").unwrap();
    let output = houserules()
        .arg("check-commit")
        .arg(&msg)
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        output.stderr,
        b"process.conventional-commits: commit \"bad subject\" does not match ^(feat|fix|chore|docs|test): .+\n"
    );
}

#[test]
fn a_clean_range_prints_ok_and_exits_0() {
    let dir = make_repo();
    let base_sha = commit(dir.path(), "chore: base");
    commit(dir.path(), "feat: good");
    let output = houserules()
        .args(["check-commit", "--from", &base_sha])
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"check-commit: ok\n");
}

#[test]
fn a_bad_range_prints_one_finding_and_exits_1() {
    let dir = make_repo();
    let base_sha = commit(dir.path(), "chore: base");
    commit(dir.path(), "bad subject");
    let output = houserules()
        .args(["check-commit", "--from", &base_sha, "--to", "HEAD"])
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        output.stderr,
        b"process.conventional-commits: commit \"bad subject\" does not match ^(feat|fix|chore|docs|test): .+\n"
    );
}

#[test]
fn an_unresolvable_from_ref_is_a_named_error_exit_2() {
    let dir = make_repo();
    let output = houserules()
        .args(["check-commit", "--from", "nope"])
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"bad ref \"nope\"\n");
}

#[test]
fn an_unreadable_message_file_is_a_named_error_exit_2() {
    let dir = make_repo();
    let missing = dir.path().join("no-such-file");
    let output = houserules()
        .arg("check-commit")
        .arg(&missing)
        .current_dir(dir.path())
        .output()
        .expect("run check-commit");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with(&format!("{}: ", missing.display())),
        "{stderr}"
    );
}
