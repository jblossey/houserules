//! `houserules capture`: the file it writes (header, output, `lossy`), the line
//! it prints, the exit status it mirrors, the git facts it records, and every
//! setup error with its exact line. Each test runs the compiled binary; the
//! captured commands run through `sh`, so the same test text runs on Windows
//! with Git for Windows (`houserules.platform-gated-tests`).

use std::fs;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use tempfile::TempDir;

/// The keys of the header, in the order that `capture --help` lists them.
const HEADER_KEYS: [&str; 10] = [
    "schema", "argv", "cwd", "start", "end", "exit", "signal", "head", "dirty", "lossy",
];

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// `houserules capture --out <out> <extra>... -- <command>...` run in `dir`.
fn capture(dir: &Path, out: &Path, extra: &[&str], command: &[&str]) -> Output {
    houserules()
        .arg("capture")
        .arg("--out")
        .arg(out)
        .args(extra)
        .arg("--")
        .args(command)
        .current_dir(dir)
        .output()
        .expect("run capture")
}

/// A capture file split into its header and its output.
struct Written {
    header: Map<String, Value>,
    output: Vec<u8>,
}

/// Reads the capture at `path`: the first line is the header, the bytes after
/// the first LF are the output.
fn read_capture(path: &Path) -> Written {
    let bytes = fs::read(path).expect("read the capture");
    let (first, output) = match bytes.iter().position(|byte| *byte == b'\n') {
        Some(at) => (&bytes[..at], bytes[at + 1..].to_vec()),
        None => (&bytes[..], Vec::new()),
    };
    let header = serde_json::from_slice(first).expect("the first line is one JSON object");
    Written { header, output }
}

/// Runs `capture` of `command` in a fresh directory with no git repository
/// and returns the directory, the capture path, the process output, and the
/// parsed file.
fn run_plain(command: &[&str]) -> (TempDir, std::path::PathBuf, Output, Written) {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let output = capture(dir.path(), &out, &[], command);
    let written = read_capture(&out);
    (dir, out, output, written)
}

/// The text of a UTF-8 output.
fn text(output: &[u8]) -> &str {
    std::str::from_utf8(output).expect("utf8 output")
}

/// Waits for `child` at most `limit`, then returns its output; a child still
/// running at the limit is killed and the test fails.
fn finish_within(mut child: Child, limit: Duration) -> Output {
    let deadline = Instant::now() + limit;
    loop {
        if child.try_wait().expect("poll the child").is_some() {
            return child.wait_with_output().expect("collect the output");
        }
        if Instant::now() > deadline {
            child.kill().expect("kill the child");
            panic!("capture did not end within {limit:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Runs `git` in `dir` with a fixed identity; fails the test on a git error.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.email=test@test.invalid",
            "-c",
            "user.name=Test",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A git repository with one commit, a tracked `a.txt`, and an empty
/// `captures/` folder for the capture files.
fn repo_with_one_commit() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    fs::write(dir.path().join("a.txt"), "tracked\n").expect("write a.txt");
    git(dir.path(), &["add", "a.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "seed"]);
    fs::create_dir(dir.path().join("captures")).expect("create captures/");
    dir
}

/// The names in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read the directory")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Asserts the stderr, exit status 2, and empty stdout of a setup error.
fn assert_setup_error(output: &Output, line: &str) {
    assert_eq!(text(&output.stderr), format!("{line}\n"), "stderr");
    assert_eq!(output.status.code(), Some(2), "exit status");
    assert_eq!(text(&output.stdout), "", "stdout");
}

// ---- the file --------------------------------------------------------------------------

#[test]
fn the_header_holds_the_ten_keys_in_order() {
    let (dir, out, output, written) = run_plain(&["sh", "-c", "echo out; echo err >&2"]);
    assert_eq!(text(&output.stderr), "", "capture prints nothing on stderr");
    let keys: Vec<&str> = written.header.keys().map(String::as_str).collect();
    assert_eq!(keys, HEADER_KEYS);
    assert_eq!(written.header["schema"], "houserules/capture@1.0.0");
    assert_eq!(
        written.header["argv"],
        serde_json::json!(["sh", "-c", "echo out; echo err >&2"])
    );
    assert_eq!(written.header["exit"], 0);
    assert_eq!(written.header["signal"], Value::Null);
    assert_eq!(written.header["lossy"], 0);
    let cwd = Path::new(written.header["cwd"].as_str().expect("cwd is a string"));
    assert!(cwd.is_absolute(), "cwd {cwd:?} is absolute");
    assert_eq!(
        cwd.canonicalize().expect("canonicalize cwd"),
        dir.path().canonicalize().expect("canonicalize dir")
    );
    assert!(out.is_file());
}

#[test]
fn both_streams_land_in_the_order_the_command_wrote_them() {
    let (_dir, _out, _output, written) = run_plain(&[
        "sh",
        "-c",
        "echo one; echo two >&2; echo three; echo four >&2",
    ]);
    assert_eq!(text(&written.output), "one\ntwo\nthree\nfour\n");
}

#[test]
fn the_times_are_rfc_3339_utc_with_nine_digits_and_start_is_not_after_end() {
    let (_dir, _out, _output, written) = run_plain(&["sh", "-c", "exit 0"]);
    for key in ["start", "end"] {
        let value = written.header[key].as_str().expect("a string");
        let shape_ok = value.len() == 30
            && value.ends_with('Z')
            && value.as_bytes()[10] == b'T'
            && value.as_bytes()[19] == b'.'
            && value.bytes().enumerate().all(|(at, byte)| match at {
                4 | 7 => byte == b'-',
                10 => byte == b'T',
                13 | 16 => byte == b':',
                19 => byte == b'.',
                29 => byte == b'Z',
                _ => byte.is_ascii_digit(),
            });
        assert!(shape_ok, "{key} is {value:?}");
    }
    assert!(
        written.header["start"].as_str() <= written.header["end"].as_str(),
        "{:?} is after {:?}",
        written.header["start"],
        written.header["end"]
    );
}

#[test]
fn the_output_gets_no_added_newline() {
    let (_dir, _out, _output, written) = run_plain(&["sh", "-c", "printf abc"]);
    assert_eq!(written.output, b"abc");
}

#[test]
fn an_empty_output_is_a_header_alone() {
    let (_dir, out, _output, written) = run_plain(&["sh", "-c", "exit 0"]);
    assert_eq!(written.output, b"");
    let bytes = fs::read(&out).expect("read the capture");
    assert!(
        !bytes.contains(&b'\n'),
        "the file holds a LF: {:?}",
        String::from_utf8_lossy(&bytes)
    );
}

#[test]
fn arguments_reach_the_command_without_a_shell() {
    let (_dir, _out, _output, written) = run_plain(&[
        "sh",
        "-c",
        "printf '%s|' \"$@\"",
        "sh",
        "a b",
        "it's",
        "$HOME",
        "",
    ]);
    assert_eq!(text(&written.output), "a b|it's|$HOME||");
    assert_eq!(
        written.header["argv"][5], "it's",
        "the header records the arguments as given"
    );
}

// The Cygwin runtime under Git for Windows' `sh` expands a glob in its own
// arguments when a native program starts it (the `glob` option of
// https://cygwin.com/cygwin-ug-net/using-cygwinenv.html), so the case holds on
// Unix only. The file `a` gives both patterns a match; a shell would expand them.
#[cfg(unix)]
#[test]
fn a_glob_reaches_the_command_as_written() {
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("a"), "").expect("write a");
    let out = dir.path().join("run.txt");
    capture(dir.path(), &out, &[], &["echo", "*", "?"]);
    assert_eq!(text(&read_capture(&out).output), "* ?\n");
}

#[test]
fn the_env_form_sets_a_variable_for_the_command() {
    let (_dir, _out, _output, written) = run_plain(&["env", "X=1", "sh", "-c", "echo $X"]);
    assert_eq!(text(&written.output), "1\n");
}

#[test]
fn a_large_output_on_both_streams_is_kept_whole() {
    let (_dir, _out, _output, written) = run_plain(&[
        "sh",
        "-c",
        "yes x | head -n 100000; yes y | head -n 100000 >&2",
    ]);
    assert_eq!(written.output.len(), 400_000);
    assert!(written.output.starts_with(b"x\nx\n"));
    assert!(written.output.ends_with(b"y\ny\n"));
    assert_eq!(written.header["exit"], 0);
}

// ---- the exit status and the printed line ----------------------------------------------

#[test]
fn capture_mirrors_the_exit_code_and_prints_one_line() {
    let (_dir, out, output, written) = run_plain(&["sh", "-c", "echo hi; exit 3"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(text(&output.stdout), format!("{}: exit 3\n", out.display()));
    assert_eq!(written.header["exit"], 3);
    assert_eq!(written.header["signal"], Value::Null);
    assert_eq!(text(&written.output), "hi\n");
}

#[test]
fn a_command_that_exits_2_gives_2_and_leaves_its_file() {
    let (_dir, out, output, written) = run_plain(&["sh", "-c", "echo bad >&2; exit 2"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(text(&output.stdout), format!("{}: exit 2\n", out.display()));
    assert_eq!(text(&output.stderr), "");
    assert_eq!(written.header["exit"], 2);
    assert_eq!(text(&written.output), "bad\n");
}

// A signal ends a process on Unix only.
#[cfg(unix)]
#[test]
fn a_signal_gives_a_null_exit_and_128_plus_the_signal() {
    let (_dir, out, output, written) = run_plain(&["sh", "-c", "kill -TERM $$"]);
    assert_eq!(output.status.code(), Some(143));
    assert_eq!(
        text(&output.stdout),
        format!("{}: ended by signal 15\n", out.display())
    );
    assert_eq!(written.header["exit"], Value::Null);
    assert_eq!(written.header["signal"], 15);
}

// An exit code above 255 exists on Windows only; `cmd` is the native shell
// that can exit with one.
#[cfg(windows)]
#[test]
fn a_windows_exit_code_above_255_passes_whole() {
    let (_dir, out, output, written) = run_plain(&["cmd", "/C", "exit 300"]);
    assert_eq!(output.status.code(), Some(300));
    assert_eq!(
        text(&output.stdout),
        format!("{}: exit 300\n", out.display())
    );
    assert_eq!(written.header["exit"], 300);
}

#[test]
fn a_relative_out_path_names_the_file_in_the_current_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = capture(
        dir.path(),
        Path::new("rel.txt"),
        &[],
        &["sh", "-c", "echo r"],
    );
    assert_eq!(text(&output.stdout), "rel.txt: exit 0\n");
    assert_eq!(
        text(&read_capture(&dir.path().join("rel.txt")).output),
        "r\n"
    );
}

// ---- bytes that are not UTF-8 ----------------------------------------------------------

#[test]
fn a_byte_that_is_not_utf8_becomes_one_replacement_and_counts_in_lossy() {
    let (_dir, _out, _output, written) = run_plain(&["sh", "-c", "printf 'a\\377b'"]);
    assert_eq!(text(&written.output), "a\u{fffd}b");
    assert_eq!(written.header["lossy"], 1);
}

#[test]
fn a_cut_utf8_sequence_is_one_replacement() {
    let (_dir, _out, _output, written) = run_plain(&["sh", "-c", "printf 'ab\\342\\202'"]);
    assert_eq!(text(&written.output), "ab\u{fffd}");
    assert_eq!(written.header["lossy"], 1);
}

#[test]
fn a_lone_cr_and_valid_utf8_stay_as_written() {
    let (_dir, _out, _output, written) =
        run_plain(&["sh", "-c", "printf 'a\\rb\\342\\202\\254\\r\\n'"]);
    assert_eq!(text(&written.output), "a\rb\u{20ac}\r\n");
    assert_eq!(written.header["lossy"], 0);
}

// ---- git: head and dirty ---------------------------------------------------------------

#[test]
fn head_and_dirty_are_null_outside_a_work_tree() {
    let (_dir, _out, _output, written) = run_plain(&["sh", "-c", "exit 0"]);
    assert_eq!(written.header["head"], Value::Null);
    assert_eq!(written.header["dirty"], Value::Null);
}

#[test]
fn head_and_dirty_are_null_before_the_first_commit() {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let out = dir.path().join("run.txt");
    capture(dir.path(), &out, &[], &["sh", "-c", "exit 0"]);
    let written = read_capture(&out);
    assert_eq!(written.header["head"], Value::Null);
    assert_eq!(written.header["dirty"], Value::Null);
}

#[test]
fn a_clean_work_tree_gives_the_full_sha_and_dirty_false() {
    let repo = repo_with_one_commit();
    let out = repo.path().join("captures").join("run.txt");
    capture(repo.path(), &out, &[], &["sh", "-c", "exit 0"]);
    let written = read_capture(&out);
    let sha = git(repo.path(), &["rev-parse", "HEAD"]);
    assert_eq!(written.header["head"], sha.trim());
    assert_eq!(written.header["head"].as_str().map(str::len), Some(40));
    assert_eq!(written.header["dirty"], false);
}

#[test]
fn an_untracked_file_makes_the_work_tree_dirty() {
    let repo = repo_with_one_commit();
    fs::write(repo.path().join("new.txt"), "untracked\n").expect("write new.txt");
    let out = repo.path().join("captures").join("run.txt");
    capture(repo.path(), &out, &[], &["sh", "-c", "exit 0"]);
    assert_eq!(read_capture(&out).header["dirty"], true);
}

#[test]
fn an_untracked_file_counts_whatever_the_git_config_says() {
    let repo = repo_with_one_commit();
    git(repo.path(), &["config", "status.showUntrackedFiles", "no"]);
    fs::write(repo.path().join("new.txt"), "untracked\n").expect("write new.txt");
    let out = repo.path().join("captures").join("run.txt");
    capture(repo.path(), &out, &[], &["sh", "-c", "exit 0"]);
    assert_eq!(read_capture(&out).header["dirty"], true);
}

#[test]
fn a_changed_tracked_file_makes_the_work_tree_dirty() {
    let repo = repo_with_one_commit();
    fs::write(repo.path().join("a.txt"), "changed\n").expect("change a.txt");
    let out = repo.path().join("captures").join("run.txt");
    capture(repo.path(), &out, &[], &["sh", "-c", "exit 0"]);
    assert_eq!(read_capture(&out).header["dirty"], true);
}

#[test]
fn the_capture_file_itself_never_counts_as_a_change() {
    let repo = repo_with_one_commit();
    let out = repo.path().join("captures").join("run.txt");
    capture(repo.path(), &out, &[], &["sh", "-c", "exit 0"]);
    let second = repo.path().join("captures").join("again.txt");
    capture(repo.path(), &second, &[], &["sh", "-c", "exit 0"]);
    assert_eq!(
        read_capture(&out).header["dirty"],
        false,
        "the first run saw a clean tree"
    );
    assert_eq!(
        read_capture(&second).header["dirty"],
        true,
        "the first capture is an untracked file for the second run"
    );
}

#[test]
fn head_and_dirty_are_null_inside_the_git_directory() {
    let repo = repo_with_one_commit();
    let out = repo.path().join("captures").join("run.txt");
    capture(
        &repo.path().join(".git"),
        &out,
        &[],
        &["sh", "-c", "exit 0"],
    );
    let written = read_capture(&out);
    assert_eq!(written.header["head"], Value::Null);
    assert_eq!(written.header["dirty"], Value::Null);
}

#[test]
fn head_and_dirty_are_null_in_a_bare_repository() {
    let repo = repo_with_one_commit();
    let source = repo.path().to_str().expect("utf8 path");
    git(repo.path(), &["clone", "-q", "--bare", source, "bare.git"]);
    let out = repo.path().join("captures").join("run.txt");
    capture(
        &repo.path().join("bare.git"),
        &out,
        &[],
        &["sh", "-c", "exit 0"],
    );
    let written = read_capture(&out);
    assert_eq!(written.header["head"], Value::Null);
    assert_eq!(written.header["dirty"], Value::Null);
}

#[test]
fn head_and_dirty_are_both_null_when_git_status_fails() {
    let repo = repo_with_one_commit();
    fs::write(repo.path().join(".git").join("index"), "not an index").expect("corrupt the index");
    let out = repo.path().join("captures").join("run.txt");
    capture(repo.path(), &out, &[], &["sh", "-c", "exit 0"]);
    let written = read_capture(&out);
    assert_eq!(written.header["head"], Value::Null);
    assert_eq!(written.header["dirty"], Value::Null);
}

// ---- stdin ------------------------------------------------------------------------------

#[test]
fn stdin_of_the_command_is_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let mut child = houserules()
        .arg("capture")
        .arg("--out")
        .arg(&out)
        .args(["--", "sh", "-c", "cat"])
        .current_dir(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn capture");
    let held_open = child.stdin.take();
    let output = finish_within(child, Duration::from_secs(30));
    drop(held_open);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(read_capture(&out).output, b"");
}

// ---- --force and the setup errors --------------------------------------------------------

#[test]
fn an_existing_file_stops_the_run_and_stays_unchanged() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let marker = dir.path().join("marker");
    fs::write(&out, "keep me").expect("write the existing file");
    let output = capture(dir.path(), &out, &[], &["sh", "-c", "echo ran > marker"]);
    assert_setup_error(
        &output,
        &format!(
            "capture: {} exists; pass --force to replace it",
            out.display()
        ),
    );
    assert_eq!(fs::read(&out).expect("read"), b"keep me");
    assert!(!marker.exists(), "the command ran");
    assert_eq!(names_in(dir.path()), ["run.txt"]);
}

#[test]
fn force_replaces_an_existing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    fs::write(&out, "old").expect("write the existing file");
    let output = capture(dir.path(), &out, &["--force"], &["sh", "-c", "echo new"]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(text(&read_capture(&out).output), "new\n");
    assert_eq!(names_in(dir.path()), ["run.txt"]);
}

#[test]
fn a_file_that_appears_while_the_command_runs_is_not_replaced() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let output = capture(dir.path(), &out, &[], &["sh", "-c", "echo raced > run.txt"]);
    assert_setup_error(
        &output,
        &format!(
            "capture: {} exists; pass --force to replace it",
            out.display()
        ),
    );
    assert_eq!(fs::read(&out).expect("read"), b"raced\n");
    assert_eq!(names_in(dir.path()), ["run.txt"]);
}

#[test]
fn force_replaces_a_file_that_appears_while_the_command_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let output = capture(
        dir.path(),
        &out,
        &["--force"],
        &["sh", "-c", "echo raced > run.txt"],
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(read_capture(&out).header["exit"], 0);
    assert_eq!(names_in(dir.path()), ["run.txt"]);
}

// A link at `--out` exists on Unix only (creating one on Windows needs a privilege).
#[cfg(unix)]
#[test]
fn a_dangling_link_at_out_stops_the_run_before_the_command() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let marker = dir.path().join("marker");
    std::os::unix::fs::symlink("nowhere", &out).expect("create the link");
    let output = capture(dir.path(), &out, &[], &["sh", "-c", "echo ran > marker"]);
    assert_setup_error(
        &output,
        &format!(
            "capture: {} exists; pass --force to replace it",
            out.display()
        ),
    );
    assert!(!marker.exists(), "the command ran");
    assert_eq!(names_in(dir.path()), ["run.txt"]);
    assert_eq!(
        fs::read_link(&out).expect("still a link"),
        Path::new("nowhere")
    );
}

/// `houserules capture --out <out> <extra>... -- true` run by `sh` under `umask`.
#[cfg(unix)]
fn capture_under_umask(dir: &Path, umask: &str, out: &Path, extra: &[&str]) -> Output {
    Command::new("sh")
        .arg("-c")
        .arg("umask \"$1\" && shift && exec \"$0\" capture --out \"$@\" -- true")
        .arg(env!("CARGO_BIN_EXE_houserules"))
        .arg(umask)
        .arg(out)
        .args(extra)
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .current_dir(dir)
        .output()
        .expect("run sh")
}

// File modes exist on Unix only. A capture gets the mode of a file that a shell
// redirect creates: 0666 less the umask of the process.
#[cfg(unix)]
#[test]
fn the_capture_gets_the_mode_of_a_plain_file_creation() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    for (umask, mode) in [("022", 0o644), ("002", 0o664), ("077", 0o600)] {
        let out = dir.path().join(format!("run-{umask}.txt"));
        let output = capture_under_umask(dir.path(), umask, &out, &[]);
        assert_eq!(output.status.code(), Some(0), "umask {umask}");
        let found = fs::metadata(&out).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(found, mode, "umask {umask}: {found:o}");
    }
}

#[cfg(unix)]
#[test]
fn force_gives_the_replacement_the_mode_of_a_plain_file_creation() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    fs::write(&out, "old").expect("write the existing file");
    fs::set_permissions(&out, fs::Permissions::from_mode(0o600)).expect("narrow the file");
    let output = capture_under_umask(dir.path(), "022", &out, &["--force"]);
    assert_eq!(output.status.code(), Some(0));
    let found = fs::metadata(&out).expect("metadata").permissions().mode() & 0o777;
    assert_eq!(found, 0o644, "{found:o}");
}

#[test]
fn a_missing_directory_stops_the_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("missing");
    let out = missing.join("run.txt");
    let marker = dir.path().join("marker");
    let output = capture(dir.path(), &out, &[], &["sh", "-c", "echo ran > marker"]);
    assert_setup_error(
        &output,
        &format!("capture: {} does not exist", missing.display()),
    );
    assert!(!marker.exists(), "the command ran");
    assert!(!missing.exists());
}

#[test]
fn a_missing_relative_directory_is_named_as_written() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = capture(
        dir.path(),
        Path::new("sub/run.txt"),
        &[],
        &["sh", "-c", "exit 0"],
    );
    assert_setup_error(&output, "capture: sub does not exist");
}

// A program that does not exist cannot start. The expected error comes from the
// same `spawn` call as the code's: Windows resolves a bare name itself and gives
// its own `program not found`, not the error 2 of Unix (`resolve_exe` in
// library/std/src/sys/process/windows.rs of the Rust sources).
#[test]
fn a_program_that_cannot_start_stops_the_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let refusal = Command::new("no-such-program-for-capture")
        .spawn()
        .expect_err("no such program");
    let output = capture(dir.path(), &out, &[], &["no-such-program-for-capture"]);
    assert_setup_error(
        &output,
        &format!("capture: could not start no-such-program-for-capture: {refusal}"),
    );
    assert!(names_in(dir.path()).is_empty(), "files left behind");
}

// `Permissions::set_mode` and a directory that refuses a new file exist on Unix.
#[cfg(unix)]
#[test]
fn an_unwritable_directory_stops_the_run() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let locked = dir.path().join("locked");
    fs::create_dir(&locked).expect("create locked/");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("lock the directory");
    let refusal = fs::File::create(locked.join("probe")).err();
    let Some(refusal) = refusal else {
        // The process can write into a read-only directory (root): nothing to test.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
        return;
    };
    let out = locked.join("run.txt");
    let marker = dir.path().join("marker");
    let output = capture(dir.path(), &out, &[], &["sh", "-c", "echo ran > marker"]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
    assert_setup_error(
        &output,
        &format!("capture: could not write {}: {refusal}", out.display()),
    );
    assert!(!marker.exists(), "the command ran");
    assert!(names_in(&locked).is_empty());
}

// A non-UTF-8 argument exists on Unix only.
#[cfg(unix)]
#[test]
fn an_argument_that_is_not_utf8_stops_the_run() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let output = houserules()
        .arg("capture")
        .arg("--out")
        .arg(&out)
        .arg("--")
        .arg("echo")
        .arg(std::ffi::OsStr::from_bytes(b"a\xffb"))
        .current_dir(dir.path())
        .output()
        .expect("run capture");
    assert_setup_error(&output, "capture: argument 1 is not UTF-8");
    assert!(names_in(dir.path()).is_empty());
}

// A non-UTF-8 argument exists on Unix only; the check of the arguments comes
// first, so an existing file does not hide it.
#[cfg(unix)]
#[test]
fn the_arguments_are_tested_before_the_file() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    fs::write(&out, "keep me").expect("write the existing file");
    let output = houserules()
        .arg("capture")
        .arg("--out")
        .arg(&out)
        .arg("--")
        .arg(std::ffi::OsStr::from_bytes(b"\xff"))
        .current_dir(dir.path())
        .output()
        .expect("run capture");
    assert_setup_error(&output, "capture: argument 0 is not UTF-8");
    assert_eq!(fs::read(&out).expect("read"), b"keep me");
}

// A non-UTF-8 `--out` path exists on Unix only. The path is never created, so
// a file system that refuses such a name (APFS) is not involved.
#[cfg(unix)]
#[test]
fn an_out_path_that_is_not_utf8_stops_the_run() {
    use std::os::unix::ffi::OsStrExt;

    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join(std::ffi::OsStr::from_bytes(b"run\xff.txt"));
    let output = houserules()
        .arg("capture")
        .arg("--out")
        .arg(&out)
        .args(["--", "sh", "-c", "exit 0"])
        .current_dir(dir.path())
        .output()
        .expect("run capture");
    assert_setup_error(&output, "capture: the --out path is not UTF-8");
    assert!(names_in(dir.path()).is_empty());
}

// A directory with a non-UTF-8 name needs a file system that accepts one:
// Linux does, APFS on macOS refuses (`houserules.platform-gated-tests`).
#[cfg(target_os = "linux")]
#[test]
fn a_current_directory_that_is_not_utf8_stops_the_run() {
    use std::os::unix::ffi::OsStrExt;

    let root = tempfile::tempdir().expect("tempdir");
    let odd = root.path().join(std::ffi::OsStr::from_bytes(b"d\xff"));
    fs::create_dir(&odd).expect("create the directory");
    let output = capture(&odd, Path::new("run.txt"), &[], &["sh", "-c", "exit 0"]);
    assert_setup_error(&output, "capture: the current directory is not UTF-8");
    assert!(names_in(&odd).is_empty());
}

// The same file system constraint; the current directory is tested before the
// directory of `--out`.
#[cfg(target_os = "linux")]
#[test]
fn the_current_directory_is_tested_before_the_out_directory() {
    use std::os::unix::ffi::OsStrExt;

    let root = tempfile::tempdir().expect("tempdir");
    let odd = root.path().join(std::ffi::OsStr::from_bytes(b"d\xff"));
    fs::create_dir(&odd).expect("create the directory");
    let output = capture(
        &odd,
        Path::new("missing/run.txt"),
        &[],
        &["sh", "-c", "exit 0"],
    );
    assert_setup_error(&output, "capture: the current directory is not UTF-8");
}

// A removed current directory exists on Unix only: `getcwd` fails there.
#[cfg(unix)]
#[test]
fn a_removed_current_directory_stops_the_run_with_the_operating_systems_error() {
    let root = tempfile::tempdir().expect("tempdir");
    let output = Command::new("sh")
        .arg("-c")
        .arg("mkdir gone && cd gone && rmdir ../gone && exec \"$0\" capture --out run.txt -- true")
        .arg(env!("CARGO_BIN_EXE_houserules"))
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .current_dir(root.path())
        .output()
        .expect("run sh");
    let stderr = text(&output.stderr);
    assert!(
        stderr.starts_with("capture: could not read the current directory: "),
        "{stderr:?}"
    );
    assert!(stderr.ends_with(")\n"), "{stderr:?}");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_missing_command_is_a_usage_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    for args in [
        vec!["capture", "--out", out.to_str().expect("utf8 path")],
        vec!["capture", "--out", out.to_str().expect("utf8 path"), "--"],
    ] {
        let output = houserules()
            .args(&args)
            .current_dir(dir.path())
            .output()
            .expect("run capture");
        let stderr = text(&output.stderr);
        assert!(
            stderr.contains("required arguments were not provided") && stderr.contains("<COMMAND>"),
            "{args:?}: {stderr:?}"
        );
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
    assert!(names_in(dir.path()).is_empty());
}

#[test]
fn the_command_must_follow_a_double_dash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["capture", "--out", "run.txt", "true"])
        .current_dir(dir.path())
        .output()
        .expect("run capture");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        text(&output.stderr).contains("unexpected argument"),
        "{:?}",
        text(&output.stderr)
    );
    assert!(names_in(dir.path()).is_empty());
}

// ---- help ---------------------------------------------------------------------------------

#[test]
fn help_lists_the_header_keys_in_order() {
    let output = houserules()
        .args(["capture", "--help"])
        .output()
        .expect("run capture --help");
    assert_eq!(output.status.code(), Some(0));
    let help = text(&output.stdout);
    let mut from = 0;
    for key in HEADER_KEYS {
        let needle = format!("`{key}`");
        let at = help[from..]
            .find(&needle)
            .unwrap_or_else(|| panic!("{needle} is missing after byte {from} of:\n{help}"));
        from += at + needle.len();
    }
}

#[test]
fn help_states_what_each_error_leaves_at_out_and_names_every_case() {
    let output = houserules()
        .args(["capture", "--help"])
        .output()
        .expect("run capture --help");
    let help = text(&output.stdout);
    for sentence in [
        "writes no file at `--out` (a file that exists stays as it was)",
        "the current directory cannot be read",
        "the operating system gives no exit status for the command",
    ] {
        assert!(
            help.contains(sentence),
            "{sentence:?} is missing from:\n{help}"
        );
    }
    assert!(!help.contains("leaves no file"), "{help}");
}

#[test]
fn capture_is_one_of_the_commands_listed_by_the_top_level_help() {
    let output = houserules().arg("--help").output().expect("run --help");
    let help = text(&output.stdout);
    assert!(
        help.lines()
            .any(|line| line.trim_start().starts_with("capture ")),
        "{help}"
    );
}
