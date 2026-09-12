//! Integration test for `template/.githooks/commit-msg`. The hook's own
//! doc comment states its contract: the trailer gate always runs;
//! `houserules check-commit` runs only after a quiet `houserules
//! check-commit --help` probe succeeds, so an older or absent binary
//! degrades to the trailer gate alone instead of blocking every commit.
//! Every case here spawns the real POSIX-shell hook against a scratch
//! temp directory, on a hermetic `PATH` (any directory already carrying a
//! `houserules` executable removed) with, where the case needs one, a
//! fake `houserules` script placed on that `PATH`.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The shipped commit-msg hook under test -- built directly from
/// `CARGO_MANIFEST_DIR`, deliberately never through `Path::canonicalize`
/// (the `repo_root()` every sibling file in this crate keeps its own
/// copy of): this path becomes an `sh` command-line argument
/// (`run_hook`, below), and `canonicalize`'s own current docs state that
/// on Windows it "converts the path to use extended length path syntax
/// ... [which] may be incompatible with other applications ... passed to
/// the application on the command-line" -- Git-for-Windows' `sh` cannot
/// open a `\\?\`-prefixed path, failing every case here with exit 127 and
/// stderr opening `/usr/bin/bash:`. The leading `..` components stay
/// unresolved in the returned path; both Windows' and POSIX's own file
/// APIs resolve `..` while opening a file, so `sh` (and every other
/// non-Rust reader) still finds it.
fn hook_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/.githooks/commit-msg")
}

/// `true` when `dir` holds a file named `houserules` (`houserules.exe` on
/// Windows too). Deliberately coarser than a full executable-bit check: a
/// portable executable-bit check has no std-only form, and adding a crate for
/// this test-only concern is out of scope. Over-removing a `PATH` entry that
/// happens to hold a non-executable file named `houserules` costs nothing here
/// -- the goal is only a `PATH` no real `houserules` binary can leak through.
fn dir_has_houserules(dir: &Path) -> bool {
    dir.join("houserules").is_file() || (cfg!(windows) && dir.join("houserules.exe").is_file())
}

/// This process's own `PATH`, with every directory holding a `houserules`
/// executable removed -- the hermetic base every `run_hook` case builds
/// its own `PATH` from. `grep`/`head`/`sh` still resolve normally: only
/// directories that themselves carry a `houserules` entry are dropped.
fn path_without_houserules() -> OsString {
    let path = env::var_os("PATH").unwrap_or_default();
    let kept: Vec<PathBuf> = env::split_paths(&path)
        .filter(|dir| !dir_has_houserules(dir))
        .collect();
    env::join_paths(kept).expect("join PATH entries")
}

/// Runs the shipped commit-msg hook against `message` from a fresh
/// scratch directory (`tempfile::tempdir`, removed on drop --
/// `houserules.tests-clean-scratch-dirs`'s sanctioned Rust form), with
/// `extra_path_dir` (when given) prepended to `path_without_houserules`'s
/// hermetic base as the child's `PATH`. Returns the exit status code and
/// stderr regardless of whether the hook succeeds.
fn run_hook(message: &str, extra_path_dir: Option<&Path>) -> (Option<i32>, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let msg_file = dir.path().join("MSG");
    fs::write(&msg_file, message).expect("write MSG");

    let base = path_without_houserules();
    let path = match extra_path_dir {
        Some(extra) => {
            let mut dirs = vec![extra.to_path_buf()];
            dirs.extend(env::split_paths(&base));
            env::join_paths(dirs).expect("join PATH entries")
        }
        None => base,
    };

    let output = Command::new("sh")
        .arg(hook_path())
        .arg(&msg_file)
        .current_dir(dir.path())
        .env("PATH", path)
        .output()
        .expect("run commit-msg hook");
    (
        output.status.code(),
        String::from_utf8(output.stderr).expect("utf8 stderr"),
    )
}

/// Sets `path`'s Unix execute bits; a no-op on Windows, where `sh`
/// (Git-for-Windows/MSYS -- the same shell every test here already
/// depends on to run the shebang hook itself) resolves a shebang script
/// on `PATH` without a Unix execute-bit concept of its own.
fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Writes an executable fake `houserules` script into a fresh scratch
/// directory: a `check-commit --help` probe call exits 0 (the fake
/// understands the subcommand), and a real `check-commit <file>` call
/// exits `code`, writing `output` to stderr first when given. Returns the
/// directory, suitable as `run_hook`'s `extra_path_dir`.
fn fake_houserules(code: i32, output: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("houserules");
    let echo = output
        .map(|text| format!("echo '{text}' >&2\n"))
        .unwrap_or_default();
    fs::write(
        &script,
        format!(
            "#!/usr/bin/env sh\n\
             if [ \"$1\" = \"check-commit\" ] && [ \"$2\" = \"--help\" ]; then\n  exit 0\nfi\n\
             {echo}exit {code}\n"
        ),
    )
    .expect("write fake houserules");
    make_executable(&script);
    dir
}

/// Writes an executable fake `houserules` script that simulates an older
/// binary with no `check-commit` subcommand: a `check-commit --help`
/// probe call exits 2 (clap's own "unrecognized subcommand" exit); a real
/// `check-commit <file>` call -- one the hook must never make once the
/// probe has already failed -- prints a distinctive line and exits 1, so
/// a hook that calls it anyway fails the test loudly instead of
/// coincidentally passing. Returns the directory, suitable as
/// `run_hook`'s `extra_path_dir`.
fn fake_houserules_rejecting_check_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("houserules");
    fs::write(
        &script,
        "#!/usr/bin/env sh\n\
         if [ \"$1\" = \"check-commit\" ] && [ \"$2\" = \"--help\" ]; then\n  exit 2\nfi\n\
         echo \"should not have been invoked for real\" >&2\nexit 1\n",
    )
    .expect("write fake houserules");
    make_executable(&script);
    dir
}

#[test]
fn accepts_a_trailer_free_message_with_no_output_when_houserules_is_absent_from_path() {
    let (code, stderr) = run_hook("feat: a clean subject\n", None);
    assert_eq!(code, Some(0));
    assert_eq!(stderr, "");
}

#[test]
fn rejects_a_co_authored_by_trailer_with_one_line_naming_it_before_any_probe() {
    let (code, stderr) = run_hook("feat: x\n\nCo-Authored-By: Someone <a@b.com>\n", None);
    assert_eq!(code, Some(1));
    let lines: Vec<&str> = stderr.trim().split('\n').collect();
    assert_eq!(lines.len(), 1, "{stderr}");
    assert!(stderr.contains("Co-Authored-By"), "{stderr}");
}

#[test]
fn rejects_a_claude_session_trailer_with_one_line_naming_it_before_any_probe() {
    let (code, stderr) = run_hook("feat: x\n\nClaude-Session: https://example.test/s\n", None);
    assert_eq!(code, Some(1));
    let lines: Vec<&str> = stderr.trim().split('\n').collect();
    assert_eq!(lines.len(), 1, "{stderr}");
    assert!(stderr.contains("Claude-Session"), "{stderr}");
}

#[test]
fn execs_houserules_check_commit_when_the_binary_is_on_path_inheriting_its_exit_and_stderr() {
    let path_dir = fake_houserules(1, Some("process.conventional-commits: bad subject"));
    let (code, stderr) = run_hook("bad subject\n", Some(path_dir.path()));
    assert_eq!(code, Some(1));
    assert_eq!(stderr.trim(), "process.conventional-commits: bad subject");
}

#[test]
fn passes_check_commit_and_the_message_files_own_path_as_argv() {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("houserules");
    fs::write(&script, "#!/usr/bin/env sh\necho \"$1 $2\" >&2\nexit 0\n")
        .expect("write fake houserules");
    make_executable(&script);

    let (_code, stderr) = run_hook("feat: x\n", Some(dir.path()));
    let mut parts = stderr.trim().split(' ');
    let subcommand = parts.next().unwrap_or_default();
    let message_path = parts.next().unwrap_or_default();
    assert_eq!(subcommand, "check-commit");
    assert!(message_path.ends_with("MSG"), "{message_path}");
}

#[test]
fn degrades_to_the_trailer_gate_alone_when_houserules_is_absent_from_path() {
    let (code, stderr) = run_hook("bad subject\n", None);
    assert_eq!(code, Some(0));
    assert_eq!(stderr, "");
}

#[test]
fn degrades_to_the_trailer_gate_alone_when_an_on_path_houserules_rejects_the_check_commit_subcommand()
 {
    let path_dir = fake_houserules_rejecting_check_commit();
    let (code, stderr) = run_hook("bad subject\n", Some(path_dir.path()));
    assert_eq!(code, Some(0));
    assert_eq!(stderr, "");
}
