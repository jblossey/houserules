//! D4 (spec section 5): this binary writes install schema major 2, so a
//! command that reads the install refuses a tree whose install schema is
//! missing or older. It prints one named line and exits 2. `update`, `init`,
//! and `files` run on such a tree. A tree whose install schema is newer is
//! refused by every command but `files` (spec C22); `update` refuses it
//! after its self-update phase and before its repo phase, so it can install
//! the release that the line names. A tree whose install schema is no install
//! schema id is refused by every command but `files`, `init` and `update`
//! included (spec C23). The tests run the compiled binary on a real 1.4.0
//! install (`v1_4_0::seed`).

mod tree_snapshot;
mod v1_4_0;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched on and no install receipt to find:
/// `AXOUPDATER_CONFIG_PATH` names `receipts`, a directory with no receipt, so
/// the phase prints its guidance line and reaches no network. `axoupdater`
/// reads `AXOUPDATER_CONFIG_WORKING_DIR` before it, so the command drops it.
fn houserules_with_self_update(receipts: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command
        .env_remove("HOUSERULES_SKIP_SELF_UPDATE")
        .env_remove("HOUSERULES_SELF_UPDATED")
        .env_remove("CI")
        .env_remove("AXOUPDATER_CONFIG_WORKING_DIR")
        .env("AXOUPDATER_CONFIG_PATH", receipts);
    command
}

/// A fresh scratch directory with `git init` already run.
fn scratch_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    dir
}

/// A scratch repository holding a real 1.4.0 install.
fn install_1_4_0() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    v1_4_0::seed(dir.path(), None);
    dir
}

/// The marker path, built the way the binary builds it.
fn marker_path(dir: &Path) -> PathBuf {
    dir.join(".houserules.json")
}

/// D4's line for the marker of `dir`: the install is a 1.x install.
fn unconverted_line(dir: &Path) -> String {
    format!(
        "{}: this is an install of houserules 1.x; run houserules update to convert it\n",
        marker_path(dir).display()
    )
}

/// Runs `houserules <args> --dir <dir>`.
fn run(dir: &Path, args: &[&str]) -> Output {
    houserules()
        .args(args)
        .arg("--dir")
        .arg(dir)
        .output()
        .expect("run houserules")
}

/// Every command that D4 covers, each with a valid argument shape. The list
/// is spec section 5's list; `update`, `init`, and `files` are not in it.
const COVERED: &[&[&str]] = &[
    &["check-knowledge"],
    &["check-backlog"],
    &["check-commit", "--from", "HEAD"],
    &["audit", "--base", "HEAD"],
    &["validate", "report.json"],
    &["render", "--check"],
    &["render"],
    &["get", "process.tdd"],
    &["for", "src/main.rs"],
    &["index"],
    &["topics"],
    &["standing"],
    &["list"],
    &["batch", "1"],
    &["set", "WI-001", "status=done"],
    &["stats", "workspace"],
    &["archive"],
    &["check-report-claims", "report.json"],
    &["pack", "list"],
    &["pack", "add", "process"],
    &["pack", "remove", "process"],
    &["pack", "upgrade", "process"],
];

#[test]
fn every_command_that_reads_the_install_prints_d4s_line_on_a_1_x_tree() {
    let dir = install_1_4_0();
    let line = unconverted_line(dir.path());
    for args in COVERED {
        let output = run(dir.path(), args);
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            line,
            "houserules {args:?}: stderr"
        );
        assert_eq!(output.status.code(), Some(2), "houserules {args:?}: exit");
        assert!(
            output.stdout.is_empty(),
            "houserules {args:?}: stdout {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn a_refused_command_changes_no_file_of_the_1_x_tree() {
    let dir = install_1_4_0();
    let before = fs::read(marker_path(dir.path())).expect("read the marker");
    let knowledge = fs::read(dir.path().join("knowledge/process.json")).expect("read a topic");
    for args in COVERED {
        run(dir.path(), args);
    }
    assert_eq!(
        fs::read(marker_path(dir.path())).expect("read the marker"),
        before
    );
    assert_eq!(
        fs::read(dir.path().join("knowledge/process.json")).expect("read a topic"),
        knowledge
    );
    assert!(!dir.path().join("knowledge/packs").exists());
}

/// Runs `houserules files` with `dir` as the working directory.
fn files_in(dir: &Path) -> Output {
    houserules()
        .arg("files")
        .current_dir(dir)
        .output()
        .expect("run files")
}

#[test]
fn files_runs_on_a_1_x_tree() {
    let neutral = tempfile::tempdir().expect("tempdir");
    let expected = files_in(neutral.path());
    assert_eq!(expected.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&expected.stdout).contains("kitOwned"),
        "{:?}",
        String::from_utf8_lossy(&expected.stdout)
    );
    let dir = install_1_4_0();

    let output = files_in(dir.path());

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, expected.stdout);
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn init_refuses_a_1_x_tree_with_its_own_line_and_changes_nothing() {
    let dir = install_1_4_0();
    let before = fs::read(marker_path(dir.path())).expect("read the marker");
    let output = run(dir.path(), &["init"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        unconverted_line(dir.path())
    );
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        fs::read(marker_path(dir.path())).expect("read the marker"),
        before
    );
}

#[test]
fn a_tree_with_no_marker_gets_the_line_that_names_id_prefix() {
    let dir = scratch_git_repo();
    fs::create_dir(dir.path().join("knowledge")).expect("create knowledge");
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: not found; run houserules update --id-prefix <prefix> to convert an install of houserules 1.x, or houserules init to start a new one\n",
            marker_path(dir.path()).display()
        )
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_marker_with_an_older_install_schema_major_gets_the_line() {
    let dir = scratch_git_repo();
    fs::write(
        marker_path(dir.path()),
        r#"{"schema": "houserules/install@1.0.0", "idPrefix": "WI"}"#,
    )
    .expect("write the marker");
    let output = run(dir.path(), &["standing"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        unconverted_line(dir.path())
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_malformed_install_schema_is_one_named_line() {
    let dir = scratch_git_repo();
    fs::write(
        marker_path(dir.path()),
        r#"{"schema": "nonsense", "idPrefix": "WI"}"#,
    )
    .expect("write the marker");
    let output = run(dir.path(), &["standing"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: schema \"nonsense\" is not houserules/install@<major>.<minor>.<patch>\n",
            marker_path(dir.path()).display()
        )
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_marker_that_is_no_json_object_is_one_named_line() {
    let dir = scratch_git_repo();
    fs::write(marker_path(dir.path()), "[]").expect("write the marker");
    let output = run(dir.path(), &["standing"]);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{}: not a JSON object\n", marker_path(dir.path()).display())
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_2_0_install_passes_d4() {
    let dir = scratch_git_repo();
    let init = run(dir.path(), &["init"]);
    assert_eq!(init.status.code(), Some(0), "{init:?}");
    let output = run(dir.path(), &["standing"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

/// The install schema id that a future houserules would stamp.
const NEWER_SCHEMA: &str = "houserules/install@3.0.0";

/// A scratch repository whose marker carries `schema` as its install schema.
fn install_with_schema(schema: &Value) -> tempfile::TempDir {
    let dir = scratch_git_repo();
    fs::write(
        marker_path(dir.path()),
        format!("{{\"schema\": {schema}, \"idPrefix\": \"WI\"}}"),
    )
    .expect("write the marker");
    dir
}

/// A scratch repository with a 2.0 install that `init` made, whose marker
/// then carries `schema` as its install schema.
fn init_install_with_schema(schema: &Value) -> tempfile::TempDir {
    let dir = scratch_git_repo();
    let init = run(dir.path(), &["init"]);
    assert_eq!(init.status.code(), Some(0), "{init:?}");
    let path = marker_path(dir.path());
    let mut marker: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read the marker"))
            .expect("parse the marker");
    marker["schema"] = schema.clone();
    fs::write(
        &path,
        serde_json::to_string_pretty(&marker).expect("serialize the marker"),
    )
    .expect("write the marker");
    dir
}

/// A scratch repository whose marker carries `NEWER_SCHEMA`.
fn install_with_a_newer_schema() -> tempfile::TempDir {
    install_with_schema(&json!(NEWER_SCHEMA))
}

/// D4's line for the marker of `dir` when its install schema is newer than
/// this binary writes (spec C22).
fn newer_line(dir: &Path) -> String {
    format!(
        "{}: install schema {NEWER_SCHEMA:?} is newer than houserules {} reads; install a houserules release that reads install schema major 3\n",
        marker_path(dir).display(),
        env!("CARGO_PKG_VERSION")
    )
}

#[test]
fn every_command_that_reads_the_install_refuses_a_newer_install_schema() {
    let dir = install_with_a_newer_schema();
    let line = newer_line(dir.path());
    for args in COVERED {
        let output = run(dir.path(), args);
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            line,
            "houserules {args:?}: stderr"
        );
        assert_eq!(output.status.code(), Some(2), "houserules {args:?}: exit");
        assert!(
            output.stdout.is_empty(),
            "houserules {args:?}: stdout {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

#[test]
fn update_and_init_refuse_a_newer_install_schema_and_change_nothing() {
    let dir = install_with_a_newer_schema();
    let before = tree_snapshot::snapshot(dir.path());
    for args in [["update"], ["init"]] {
        let output = run(dir.path(), &args);
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            newer_line(dir.path()),
            "houserules {args:?}: stderr"
        );
        assert_eq!(output.status.code(), Some(2), "houserules {args:?}: exit");
        assert!(
            output.stdout.is_empty(),
            "houserules {args:?}: stdout {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
        tree_snapshot::assert_same_tree(
            &before,
            &tree_snapshot::snapshot(dir.path()),
            &format!("houserules {args:?} on a newer install schema"),
        );
    }
}

/// Runs `update` on `dir` with its self-update phase on, and requires the
/// order of the phases (spec C22): the line that opens the phase, its guidance
/// line, and then the refusal `line`. The refusal comes with exit 2, nothing
/// on stdout, and no change to the tree.
fn assert_update_refuses_after_its_self_update_phase(dir: &Path, line: &str) {
    let receipts = tempfile::tempdir().expect("tempdir");
    let before = tree_snapshot::snapshot(dir);

    let output = houserules_with_self_update(receipts.path())
        .arg("update")
        .arg("--dir")
        .arg(dir)
        .output()
        .expect("run houserules update");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 3, "{stderr}");
    assert_eq!(lines[0], "houserules: checking for a newer release...");
    assert!(
        lines[1].starts_with("houserules: no shell-installer receipt found;"),
        "{stderr}"
    );
    assert_eq!(format!("{}\n", lines[2]), line);
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    tree_snapshot::assert_same_tree(
        &before,
        &tree_snapshot::snapshot(dir),
        "update after its self-update phase",
    );
}

/// `update` runs its self-update phase first, so that it can install the
/// release which the line asks for, and refuses a newer install schema before
/// its repo phase (spec C22).
#[test]
fn update_runs_its_self_update_phase_before_it_refuses_a_newer_install_schema() {
    let dir = install_with_a_newer_schema();

    assert_update_refuses_after_its_self_update_phase(dir.path(), &newer_line(dir.path()));
}

/// Install schema values that are no install schema id: a string without the
/// prefix, a value that is no string, and an id without its minor and patch.
fn malformed_schemas() -> [Value; 3] {
    [json!("nonsense"), json!(2), json!("houserules/install@3")]
}

/// `init` and `update` refuse an install schema that is no install schema id
/// with the line that the reading commands print, and exit 2: they keep the
/// value no more than they rewrite it (spec C23). `update` refuses before its
/// repo phase, as for a newer install schema.
#[test]
fn init_and_update_refuse_a_malformed_install_schema_with_the_line_of_the_reading_commands() {
    for schema in malformed_schemas() {
        let dir = init_install_with_schema(&schema);
        let before = tree_snapshot::snapshot(dir.path());
        let reading = run(dir.path(), &["standing"]);
        let marker = marker_path(dir.path()).display().to_string();
        let line = match schema.as_str() {
            Some(id) => format!(
                "{marker}: schema {id:?} is not houserules/install@<major>.<minor>.<patch>\n"
            ),
            None => format!("{marker}: schema must be a string\n"),
        };
        assert_eq!(
            String::from_utf8_lossy(&reading.stderr),
            line,
            "{schema}: standing"
        );
        assert_eq!(reading.status.code(), Some(2), "{schema}: {reading:?}");

        for args in [["update"], ["init"]] {
            let output = run(dir.path(), &args);

            assert_eq!(
                String::from_utf8_lossy(&output.stderr),
                String::from_utf8_lossy(&reading.stderr),
                "houserules {args:?} with schema {schema}: stderr"
            );
            assert_eq!(
                output.status.code(),
                Some(2),
                "houserules {args:?} with schema {schema}: exit"
            );
            assert!(
                output.stdout.is_empty(),
                "houserules {args:?} with schema {schema}: stdout {:?}",
                String::from_utf8_lossy(&output.stdout)
            );
            tree_snapshot::assert_same_tree(
                &before,
                &tree_snapshot::snapshot(dir.path()),
                &format!("houserules {args:?} with schema {schema}"),
            );
        }
    }
}

/// `update` refuses a malformed install schema after its self-update phase,
/// as it refuses a newer one (spec C22, C23).
#[test]
fn update_runs_its_self_update_phase_before_it_refuses_a_malformed_install_schema() {
    for schema in malformed_schemas() {
        let dir = init_install_with_schema(&schema);
        let reading = run(dir.path(), &["standing"]);

        assert_update_refuses_after_its_self_update_phase(
            dir.path(),
            &String::from_utf8_lossy(&reading.stderr),
        );
    }
}

#[test]
fn files_runs_on_a_tree_with_a_newer_install_schema() {
    let neutral = tempfile::tempdir().expect("tempdir");
    let expected = files_in(neutral.path());
    let dir = install_with_a_newer_schema();

    let output = files_in(dir.path());

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(output.stdout, expected.stdout);
    assert!(output.stderr.is_empty(), "{output:?}");
}
