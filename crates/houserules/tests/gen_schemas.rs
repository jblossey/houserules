//! The `gen-schemas` dev binary: `--check` against the committed schema
//! files, the default write, and `--record` for the manifest
//! `schemas/published.json`, which never changes the row of a file that a
//! release tag holds. These tests run apart from `published_schemas.rs`
//! because `CARGO_BIN_EXE_gen-schemas` exists only once the binary does.

mod tracker_support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    commit_all, copy_dir, git, repo_root, scratch_git_repo, sha256_hex, with_default_git,
};

/// The schema files that the registry of `gen-schemas` publishes, relative to
/// `schemas/`.
const FILES: [&str; 6] = [
    "audit-result/1.0.0.json",
    "capture/1.0.0.json",
    "change-record/1.0.0.json",
    "evidence/1.0.0.json",
    "route/1.0.0.json",
    "rule-package/1.0.0.json",
];

/// Creates the lightweight tag `name` at HEAD of the repository `repo`, with
/// signing off whatever the user configured.
fn tag(repo: &Path, name: &str) {
    git(repo, &["-c", "tag.gpgsign=false", "tag", name]);
}

/// The command `gen-schemas <args> --dir <root>` against the scratch root
/// `root`.
fn gen_schemas_command(root: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gen-schemas"));
    command.args(args).arg("--dir").arg(root);
    command
}

/// Runs `gen-schemas <args>` against the scratch root `root`.
fn gen_schemas(root: &Path, args: &[&str]) -> Output {
    gen_schemas_command(root, args)
        .output()
        .expect("run gen-schemas")
}

/// A directory that refuses a new file. Dropping it restores the mode, so
/// that the scratch root can be removed.
#[cfg(unix)]
struct LockedDirectory {
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl Drop for LockedDirectory {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        // A failed restore leaves the scratch root to the OS temp cleaner.
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o755));
    }
}

/// Makes the directory `path` refuse a new file. Returns the guard that
/// undoes it and the error that `fs::write` gives for a file in `path`, or
/// `None` when the process can write there anyway (root): then the case
/// cannot be set up, and the test ends.
#[cfg(unix)]
fn lock_directory(path: &Path) -> Option<(LockedDirectory, std::io::Error)> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o555)).expect("lock the directory");
    let locked = LockedDirectory {
        path: path.to_path_buf(),
    };
    let probe = path.join("probe");
    match fs::write(&probe, "") {
        Ok(()) => {
            fs::remove_file(&probe).expect("remove the probe");
            None
        }
        Err(error) => Some((locked, error)),
    }
}

/// The stdout of `output` as text.
fn out(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("utf8 stdout")
}

/// The stderr of `output` as text.
fn err(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("utf8 stderr")
}

/// A scratch root that holds a copy of this checkout's `schemas/`.
fn root_with_schemas() -> TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    copy_dir(&repo_root().join("schemas"), &root.path().join("schemas"));
    root
}

/// A scratch git repository that holds a committed copy of `schemas/`.
fn repo_with_schemas() -> TempDir {
    let repo = scratch_git_repo();
    copy_dir(&repo_root().join("schemas"), &repo.path().join("schemas"));
    commit_all(repo.path(), "chore: schemas");
    repo
}

/// The manifest of `root` as parsed JSON.
fn manifest(root: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(root.join("schemas/published.json")).expect("read"))
        .expect("parse the manifest")
}

/// The manifest that a correct `--record` writes for this checkout's schemas.
fn recorded_manifest() -> Value {
    let rows: Vec<Value> = FILES
        .iter()
        .map(|file| {
            let bytes = fs::read(repo_root().join("schemas").join(file)).expect("read");
            json!({ "path": file, "sha256": sha256_hex(&bytes) })
        })
        .collect();
    json!({ "files": rows })
}

/// Sets the digest of the manifest row for `file` in `root` to `digest`.
fn set_row_digest(root: &Path, file: &str, digest: &str) {
    let path = root.join("schemas/published.json");
    let mut value = manifest(root);
    for row in value["files"].as_array_mut().expect("rows") {
        if row["path"] == file {
            row["sha256"] = json!(digest);
        }
    }
    fs::write(path, format!("{value:#}\n")).expect("write the manifest");
}

#[test]
fn check_passes_on_the_committed_schemas() {
    let output = Command::new(env!("CARGO_BIN_EXE_gen-schemas"))
        .arg("--check")
        .output()
        .expect("run gen-schemas");
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (Some(0), String::new(), String::new())
    );
}

#[test]
fn check_names_a_changed_and_a_missing_file() {
    let root = root_with_schemas();
    let changed = root.path().join("schemas/capture/1.0.0.json");
    let mut text = fs::read_to_string(&changed).expect("read the schema");
    text.push('\n');
    fs::write(&changed, text).expect("change the schema");
    fs::remove_file(root.path().join("schemas/rule-package/1.0.0.json")).expect("remove");
    let output = gen_schemas(root.path(), &["--check"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            "schemas/capture/1.0.0.json: differs from the generated schema\n\
             schemas/rule-package/1.0.0.json: missing\n"
                .to_string()
        )
    );
}

#[test]
fn check_names_a_schema_file_that_cannot_be_read() {
    let root = root_with_schemas();
    let path = root
        .path()
        .join("schemas")
        .join("capture")
        .join("1.0.0.json");
    fs::remove_file(&path).expect("remove the schema");
    fs::create_dir(&path).expect("a directory in its place");
    let reason = fs::read(&path).expect_err("a directory is no readable file");
    let output = gen_schemas(root.path(), &["--check"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            format!("schemas/capture/1.0.0.json: cannot be read: {reason}\n")
        )
    );
}

#[test]
fn record_stops_at_a_manifest_that_cannot_be_read() {
    let repo = repo_with_schemas();
    let manifest = repo.path().join("schemas").join("published.json");
    fs::remove_file(&manifest).expect("remove the manifest");
    fs::create_dir(&manifest).expect("a directory in its place");
    let reason = fs::read_to_string(&manifest).expect_err("a directory is no readable file");
    let output = gen_schemas(repo.path(), &["--record"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            format!("gen-schemas: read {}: {reason}\n", manifest.display())
        )
    );
}

/// A manifest that is no JSON stops `--record` before any write.
#[test]
fn record_stops_at_a_manifest_that_is_no_json() {
    let repo = repo_with_schemas();
    let manifest = repo.path().join("schemas").join("published.json");
    fs::write(&manifest, "not json\n").expect("write the manifest");
    let reason = serde_json::from_str::<Value>("not json\n").expect_err("no JSON");
    let output = gen_schemas(repo.path(), &["--record"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            format!("gen-schemas: parse {}: {reason}\n", manifest.display())
        )
    );
    assert_eq!(
        fs::read_to_string(&manifest).expect("read the manifest"),
        "not json\n",
        "the manifest keeps its bytes"
    );
}

// A directory that refuses a new file exists on Unix (`Permissions::set_mode`).
#[cfg(unix)]
#[test]
fn a_schema_file_that_cannot_be_written_stops_the_run() {
    let root = root_with_schemas();
    let directory = root.path().join("schemas").join("capture");
    fs::remove_file(directory.join("1.0.0.json")).expect("remove the schema");
    let Some((_locked, refusal)) = lock_directory(&directory) else {
        return;
    };
    let output = gen_schemas(root.path(), &[]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            format!(
                "gen-schemas: write {}: {refusal}\n",
                directory.join("1.0.0.json").display()
            )
        )
    );
}

// A directory that refuses a new file exists on Unix (`Permissions::set_mode`).
#[cfg(unix)]
#[test]
fn record_stops_at_a_manifest_that_cannot_be_written() {
    let repo = repo_with_schemas();
    let schemas = repo.path().join("schemas");
    fs::remove_file(schemas.join("published.json")).expect("remove the manifest");
    let Some((_locked, refusal)) = lock_directory(&schemas) else {
        return;
    };
    let output = gen_schemas(repo.path(), &["--record"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            format!(
                "gen-schemas: write {}: {refusal}\n",
                schemas.join("published.json").display()
            )
        ),
        "no row counts as recorded when the manifest stays unwritten"
    );
}

// Windows searches the parent's `PATH` after the child's, so an empty `PATH`
// does not hide git there. Unix searches the child's `PATH` alone.
#[cfg(unix)]
#[test]
fn record_names_a_git_that_cannot_be_run() {
    let root = root_with_schemas();
    let no_programs = tempfile::tempdir().expect("tempdir");
    let refusal = Command::new("git")
        .env("PATH", no_programs.path())
        .output()
        .expect_err("no git on an empty PATH");
    let output = gen_schemas_command(root.path(), &["--record"])
        .env("PATH", no_programs.path())
        .output()
        .expect("run gen-schemas");
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            format!("gen-schemas: run git tag: {refusal}\n")
        )
    );
}

#[test]
fn record_names_a_tag_listing_that_git_refuses() {
    let root = tempfile::tempdir().expect("tempdir");
    let listing = with_default_git(
        Command::new("git")
            .args(["tag", "--list", "--sort=refname", "v*"])
            .current_dir(root.path()),
    )
    .output()
    .expect("run git");
    assert!(
        !listing.status.success(),
        "{} is inside a git work tree",
        root.path().display()
    );
    let refusal = String::from_utf8_lossy(&listing.stderr).trim().to_string();
    let output = with_default_git(&mut gen_schemas_command(root.path(), &["--record"]))
        .output()
        .expect("run gen-schemas");
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            format!("gen-schemas: git tag --list failed: {refusal}\n")
        )
    );
}

#[test]
fn the_default_run_writes_every_schema_file_and_never_the_manifest() {
    let root = tempfile::tempdir().expect("tempdir");
    let first = gen_schemas(root.path(), &[]);
    assert_eq!(
        (first.status.code(), out(&first), err(&first)),
        (
            Some(0),
            "wrote schemas/audit-result/1.0.0.json\nwrote schemas/capture/1.0.0.json\n\
             wrote schemas/change-record/1.0.0.json\nwrote schemas/evidence/1.0.0.json\nwrote schemas/route/1.0.0.json\n\
             wrote schemas/rule-package/1.0.0.json\n"
                .to_string(),
            String::new()
        )
    );
    for file in FILES {
        assert_eq!(
            fs::read(root.path().join("schemas").join(file)).expect("a written schema"),
            fs::read(repo_root().join("schemas").join(file)).expect("a committed schema"),
            "{file} equals the committed file"
        );
    }
    assert!(!root.path().join("schemas/published.json").exists());
    let second = gen_schemas(root.path(), &[]);
    assert_eq!(
        (second.status.code(), out(&second), err(&second)),
        (Some(0), String::new(), String::new()),
        "a second run changes nothing and prints nothing"
    );
}

#[test]
fn record_adds_a_row_for_every_file_that_has_none() {
    let repo = repo_with_schemas();
    fs::remove_file(repo.path().join("schemas/published.json")).expect("remove the manifest");
    let output = gen_schemas(repo.path(), &["--record"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(0),
            "recorded schemas/audit-result/1.0.0.json\nrecorded schemas/capture/1.0.0.json\n\
             recorded schemas/change-record/1.0.0.json\nrecorded schemas/evidence/1.0.0.json\nrecorded schemas/route/1.0.0.json\n\
             recorded schemas/rule-package/1.0.0.json\n"
                .to_string(),
            String::new()
        )
    );
    assert_eq!(manifest(repo.path()), recorded_manifest());
    let again = gen_schemas(repo.path(), &["--record"]);
    assert_eq!(
        (again.status.code(), out(&again), err(&again)),
        (Some(0), String::new(), String::new()),
        "a second --record changes nothing and prints nothing"
    );
}

#[test]
fn record_replaces_the_row_of_a_file_that_no_release_tag_holds() {
    let repo = scratch_git_repo();
    git(
        repo.path(),
        &[
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            "chore: empty",
        ],
    );
    tag(repo.path(), "v0.9.0");
    copy_dir(&repo_root().join("schemas"), &repo.path().join("schemas"));
    commit_all(repo.path(), "chore: schemas");
    tag(repo.path(), "snapshot");
    set_row_digest(repo.path(), "capture/1.0.0.json", &"0".repeat(64));
    let output = gen_schemas(repo.path(), &["--record"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(0),
            "re-recorded schemas/capture/1.0.0.json\n".to_string(),
            String::new()
        ),
        "the tag v0.9.0 predates the file and the tag snapshot is no release tag"
    );
    assert_eq!(manifest(repo.path()), recorded_manifest());
}

#[test]
fn record_refuses_to_change_the_row_of_a_file_that_a_release_tag_holds() {
    let repo = repo_with_schemas();
    tag(repo.path(), "v1.0.0");
    set_row_digest(repo.path(), "capture/1.0.0.json", &"0".repeat(64));
    let before = fs::read(repo.path().join("schemas/published.json")).expect("read");
    let output = gen_schemas(repo.path(), &["--record"]);
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            "schemas/capture/1.0.0.json: release v1.0.0 holds this file, so its row stays; \
             give the change a new schema version\n"
                .to_string()
        )
    );
    assert_eq!(
        fs::read(repo.path().join("schemas/published.json")).expect("read"),
        before,
        "the manifest keeps its bytes"
    );
}

/// The tag that `--record` names is the first by name, whatever `tag.sort`
/// the user configured.
#[test]
fn record_names_the_first_release_tag_by_name() {
    let repo = repo_with_schemas();
    tag(repo.path(), "v1.0.0");
    tag(repo.path(), "v0.9.1");
    set_row_digest(repo.path(), "capture/1.0.0.json", &"0".repeat(64));
    let output = Command::new(env!("CARGO_BIN_EXE_gen-schemas"))
        .arg("--record")
        .arg("--dir")
        .arg(repo.path())
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "tag.sort")
        .env("GIT_CONFIG_VALUE_0", "-refname")
        .output()
        .expect("run gen-schemas");
    assert_eq!(
        (output.status.code(), out(&output), err(&output)),
        (
            Some(1),
            String::new(),
            "schemas/capture/1.0.0.json: release v0.9.1 holds this file, so its row stays; \
             give the change a new schema version\n"
                .to_string()
        )
    );
}

#[test]
fn an_unknown_argument_and_both_modes_are_usage_errors() {
    let root = tempfile::tempdir().expect("tempdir");
    for args in [&["--bogus"][..], &["--check", "--record"][..]] {
        let output = gen_schemas(root.path(), args);
        assert_eq!(
            (output.status.code(), out(&output), err(&output)),
            (
                Some(2),
                String::new(),
                "usage: gen-schemas [--check | --record] [--dir <root>]\n".to_string()
            ),
            "{args:?}"
        );
    }
}
