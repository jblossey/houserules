//! Helpers that `no_tracker.rs`, `tracker_conformance.rs`, and
//! `tracker_json.rs` share: the binary under test, scratch repositories, the
//! install shapes the tracker cases start from, the environment of a hermetic
//! `bd`, and the exact lines of spec sections 3.3, 3.4, 3.8, and 3.10. Every
//! test file that needs them includes this module, as `install_shape/mod.rs`
//! is shared.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Map, Value, json};

/// Line A of spec 3.4: no tracker and no `backlog/` directory.
pub const LINE_A: &str =
    "no tracker installed; run houserules init --tracker json|github|beads to add one";

/// Line B of spec 3.4: no tracker and a `backlog/` directory.
pub const LINE_B: &str =
    "backlog/ exists but no tracker is installed; run houserules init --tracker json";

/// The archive line of spec 3.4.
pub const ARCHIVE_LINE: &str = "archive: skipped the backlog half: no tracker installed; run houserules init --tracker json|github|beads to add one";

/// The audit line of spec 3.4 with a zero count.
pub const AUDIT_LINE_ZERO: &str =
    "skipped 0 work-item references: no tracker installed (houserules init --tracker)";

/// The line of an unreachable JSON tracker (spec 3.4).
pub const JSON_UNREACHABLE: &str =
    "tracker json cannot be reached: backlog/ is missing; restore backlog/ from version control";

/// The refusal of `pack add process` with no tracker (spec 3.10).
pub const PACK_NEEDS_TRACKER: &str =
    "pack process needs a tracker; run houserules init --tracker json|github|beads";

/// A `Command` for the compiled `houserules` binary under test. This file
/// keeps the helper beside its callers, as every `tests/*.rs` file does.
pub fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// This checkout's repository root.
pub fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Runs `git <args>` in `dir` with a fixed identity and no signing, and
/// returns its stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
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

/// Runs `actionlint` on `file` and requires a clean exit. A missing tool fails
/// the test and names the step that installs it.
pub fn assert_actionlint_passes(file: &Path) {
    let output = Command::new("actionlint")
        .arg(file)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "actionlint is not installed ({error}): run mise install, which pins it in mise.toml"
            )
        });
    assert!(
        output.status.success(),
        "actionlint rejects {}:\n{}{}",
        file.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The lowercase hex SHA-256 digest of `content`.
pub fn sha256_hex(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A fresh scratch directory with `git init` already run.
pub fn scratch_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q"]);
    dir
}

/// Adds the submodule `path` to the repository at `dir`. The repository of the
/// submodule holds `file` in its first commit. The call allows the `file`
/// protocol for itself alone: git refuses it by default for a submodule.
pub fn add_submodule(dir: &Path, path: &str, file: &str) {
    let upstream = scratch_git_repo();
    fs::write(upstream.path().join(file), "text\n").expect("write the submodule file");
    commit_all(upstream.path(), "chore: submodule");
    let source = upstream.path().to_str().expect("a UTF-8 path");
    git(
        dir,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            source,
            path,
        ],
    );
}

/// Commits every change in `dir` and returns the new commit's sha.
pub fn commit_all(dir: &Path, message: &str) -> String {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            message,
        ],
    );
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

/// Runs `houserules <args> --dir <dir>`.
pub fn run(dir: &Path, args: &[&str]) -> Output {
    houserules()
        .args(args)
        .arg("--dir")
        .arg(dir)
        .output()
        .unwrap_or_else(|error| panic!("run houserules {args:?}: {error}"))
}

/// Runs `houserules <args> --dir <dir>` and requires exit 0.
pub fn run_ok(dir: &Path, args: &[&str]) -> Output {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "houserules {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// The stdout of `output` as text.
pub fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

/// The stderr of `output` as text.
pub fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf8 stderr")
}

/// The number of lines of `text` that equal `wanted`.
pub fn count_lines(text: &str, wanted: &str) -> usize {
    text.lines().filter(|line| *line == wanted).count()
}

/// Asserts the exit code, stdout, and stderr of `output` whole.
pub fn assert_output(output: &Output, code: i32, expected_stdout: &str, expected_stderr: &str) {
    assert_eq!(
        (output.status.code(), stdout(output), stderr(output)),
        (
            Some(code),
            expected_stdout.to_string(),
            expected_stderr.to_string()
        ),
        "exit code, stdout, and stderr"
    );
}

/// Reads and parses the JSON file `relative` below `dir`.
pub fn read_json(dir: &Path, relative: &str) -> Value {
    let text = fs::read_to_string(dir.join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {relative}: {error}"))
}

/// Writes `value` as pretty JSON with a trailing newline to `relative`
/// below `dir`, creating parent directories.
pub fn write_json(dir: &Path, relative: &str, value: &Value) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("create parents");
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(value).expect("serialize")
        ),
    )
    .expect("write json");
}

/// Applies `edit` to the marker of the install at `dir`.
pub fn edit_marker(dir: &Path, edit: impl FnOnce(&mut Map<String, Value>)) {
    let mut marker = match read_json(dir, ".houserules.json") {
        Value::Object(map) => map,
        other => panic!("the marker is not an object: {other}"),
    };
    edit(&mut marker);
    write_json(dir, ".houserules.json", &Value::Object(marker));
}

/// Gives the install at `dir` the JSON tracker by editing its marker, the
/// way a project that has the 1.x backlog files would.
pub fn declare_json_tracker(dir: &Path) {
    edit_marker(dir, |marker| {
        marker.insert("tracker".to_string(), json!({"adapter": "json"}));
    });
}

/// Gives `command` the environment of a hermetic `bd` below `home`, which this
/// function creates. `bd` reads its user configuration, and writes its machine
/// id, its queue of usage events, and its Dolt data, below the home directory
/// of the person who runs it. `HOME`, `USERPROFILE`, and the native
/// configuration directories of Windows (`APPDATA`, `LOCALAPPDATA`) therefore
/// name `home`, so that a `bd` that this environment reaches, directly or
/// through `houserules`, touches no file outside `home` and the scenario. The
/// environment also turns usage metrics off, names an actor, puts the Dolt home
/// and the XDG configuration home below `home`, and removes `BEADS_DIR`, which
/// would name a store of the caller.
pub fn hermetic_bd(command: &mut Command, home: &Path) {
    fs::create_dir_all(home).unwrap_or_else(|error| panic!("create {}: {error}", home.display()));
    command
        .env("BD_DISABLE_METRICS", "1")
        .env("DO_NOT_TRACK", "1")
        .env("BEADS_ACTOR", "houserules-tests")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("APPDATA", home.join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env("DOLT_ROOT_PATH", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env_remove("BEADS_DIR");
}

/// A 2.0 install made by a plain `init`, with any `backlog/` directory
/// removed: the install of spec 3.4's first column, whichever files `init`
/// seeds. It holds the `knowledge-base` pack alone (spec 4.6).
pub fn install_without_backlog() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    let backlog = dir.path().join("backlog");
    if backlog.exists() {
        fs::remove_dir_all(backlog).expect("remove backlog/");
    }
    dir
}

/// An install that holds the process pack and no tracker, with no `backlog/`
/// directory: the state of spec 3.10. A plain `init` installs no process
/// pack, and `pack add process` needs a tracker, so the install takes the JSON
/// tracker first and the marker then loses it, as a project that deleted the
/// tracker data would.
pub fn install_with_the_process_pack_and_no_tracker() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    run_ok(dir.path(), &["pack", "add", "process"]);
    edit_marker(dir.path(), |marker| {
        marker.remove("tracker");
    });
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    dir
}

/// [`install_with_the_process_pack_and_no_tracker`] with a `backlog/`
/// directory: the install of spec 3.4's second column.
pub fn install_with_the_process_pack_and_a_backlog_dir() -> tempfile::TempDir {
    let dir = install_with_the_process_pack_and_no_tracker();
    fs::create_dir_all(dir.path().join("backlog")).expect("create backlog/");
    fs::write(dir.path().join("backlog/.gitkeep"), b"").expect("write backlog/.gitkeep");
    dir
}

/// The install of spec 3.4's second column: no tracker, and a `backlog/`
/// directory.
pub fn install_with_backlog_dir() -> tempfile::TempDir {
    let dir = install_without_backlog();
    fs::create_dir_all(dir.path().join("backlog")).expect("create backlog/");
    fs::write(dir.path().join("backlog/.gitkeep"), b"").expect("write backlog/.gitkeep");
    dir
}

/// Copies the `tests/fixtures/mini` install, which holds a JSON tracker
/// key and a small backlog, into a fresh git repository whose id prefix is
/// `HR`.
pub fn mini_install() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    copy_dir(&repo_root().join("tests/fixtures/mini"), dir.path());
    edit_marker(dir.path(), |marker| {
        marker.insert("idPrefix".to_string(), json!("HR"));
    });
    dir
}

/// Copies every file under `from` into `to`.
pub fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create destination");
    for entry in fs::read_dir(from).expect("read source") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("copy file");
        }
    }
}
