//! `.houserules.json` `defaultBranch` (HR-131): the `durable-sha` check of
//! `audit` judges reachability against the default branch, and
//! `defaultBranch` names it for a repository whose default branch is
//! neither `main` nor `master` and that has no `origin/HEAD`. Each case runs
//! the compiled binary in a scratch repository whose only branch is
//! `trunk`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// The file of the spec of batch 38, below `docs/specs/`.
const SPEC_FILE: &str = "2026-10-25-batch-38-openspec-and-release.md";

/// The line that every reader of the marker prints for an empty `defaultBranch` (spec 14 and
/// case 13).
const EMPTY_DEFAULT_BRANCH: &str = ".houserules.json: defaultBranch must be a non-empty string";

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of `common::houserules`, for the reason
/// `tests/check_commit.rs` gives for its own copy.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// Runs `git` in `root`, panicking with its stderr on failure. Returns
/// stdout.
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
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

fn write_file(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, content).expect("write file");
}

/// Stages everything under `root` and commits it with `message`; returns
/// the new commit's sha.
fn commit_all(root: &Path, message: &str) -> String {
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

/// The text of a 2.0 `.houserules.json` (D4 refuses any other) with the keys
/// of `extra`, each preceded by a comma, after the install `schema`.
fn marker_with(extra: &str) -> String {
    format!("{{\"schema\": \"houserules/install@2.0.0\"{extra}}}\n")
}

/// A repository whose only branch is `trunk`. It holds an install with
/// `marker` as its `.houserules.json` and one standing `durable-sha` rule
/// over `docs/**`, and a second commit that adds `docs/x.md`. Returns the
/// repository and the sha of the first commit.
fn trunk_repo(marker: &str) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().expect("tempdir");
    let root = repo.path();
    git(root, &["init", "-q", "-b", "trunk"]);
    let schema = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json");
    write_file(
        root,
        "knowledge/schema.json",
        &fs::read_to_string(schema).expect("read the vendored schema"),
    );
    write_file(
        root,
        "knowledge/areas.json",
        r#"{"global":{"paths":[]},"process":{"paths":[]},"docs":{"paths":["docs/**"]}}"#,
    );
    let topic = serde_json::json!({
        "$schema": "./schema.json", "topic": "knowledge-base", "title": "knowledge-base title",
        "entries": [{
            "id": "knowledge-base.cite-durable-refs", "kind": "rule", "area": "docs",
            "standing": true, "summary": "Cite durable refs.", "body": ["Body."],
            "tags": ["test"], "source": {"date": "2026-08-29", "by": "user"},
            "check": {"type": "durable-sha", "level": "fail", "files": "docs/**"},
        }],
    });
    write_file(
        root,
        "knowledge/knowledge-base.json",
        &serde_json::to_string(&topic).expect("serialize the topic"),
    );
    write_file(root, ".houserules.json", marker);
    let base = commit_all(root, "chore: base");
    write_file(root, "docs/x.md", "no citation here\n");
    commit_all(root, "docs: add a page");
    (repo, base)
}

fn audit(root: &Path, base: &str) -> Output {
    houserules()
        .args(["audit", "--base", base])
        .current_dir(root)
        .output()
        .expect("run audit")
}

/// The one line `audit` prints when no default-branch candidate exists:
/// it names the key that fixes it.
const NOT_FOUND_LINE: &str = "durable-sha: cannot resolve the default branch. No origin/HEAD, \
     origin/main, origin/master, main, or master exists. Set \"defaultBranch\" in \
     .houserules.json.\n";

/// With no key and no `main` or `master`, the check names the key.
#[test]
fn audit_names_the_key_when_no_default_branch_resolves() {
    let (repo, base) = trunk_repo(&marker_with(""));

    let output = audit(repo.path(), &base);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(String::from_utf8_lossy(&output.stderr), NOT_FOUND_LINE);
}

/// `defaultBranch` names `trunk`: the check resolves it and passes.
#[test]
fn audit_judges_reachability_against_the_configured_branch() {
    let (repo, base) = trunk_repo(&marker_with(r#", "defaultBranch": "trunk""#));
    let trunk_tip = git(repo.path(), &["rev-parse", "trunk"]).trim().to_string();

    let output = audit(repo.path(), &base);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).expect("audit JSON");
    assert_eq!(result["rules"][0]["result"], "pass");
    assert_eq!(
        result["rules"][0]["evidence"],
        format!(
            "1 file(s) checked for in-branch sha citations (against the default branch, \
             {trunk_tip})"
        )
    );
}

/// HR-251: an empty `defaultBranch` names no branch, and a value that is no string
/// names none either: `audit` stops with the line of the marker (exit 2) for either,
/// the line that every other reader of the marker prints, and no `durable-sha` line.
#[test]
fn audit_names_a_default_branch_that_is_no_branch_name() {
    for value in ["\"\"", "5"] {
        let (repo, base) = trunk_repo(&marker_with(&format!(", \"defaultBranch\": {value}")));

        let output = audit(repo.path(), &base);

        assert_eq!(output.status.code(), Some(2), "{value}");
        assert_eq!(output.stdout, b"", "{value}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.ends_with(&format!("{EMPTY_DEFAULT_BRANCH}\n")),
            "{value}: {stderr}"
        );
        assert!(!stderr.contains("durable-sha"), "{value}: {stderr}");
    }
}

/// HR-251: the commands that read the marker through its validated reader refuse an
/// empty `defaultBranch` with the line of the marker and exit 2: `evidence` names the
/// file as a gate does.
#[test]
fn evidence_refuses_an_empty_default_branch_with_the_line_of_the_marker() {
    let (repo, base) = trunk_repo(&marker_with(", \"defaultBranch\": \"\""));

    let output = houserules()
        .args(["evidence", "--base", &base])
        .current_dir(repo.path())
        .output()
        .expect("run evidence");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{EMPTY_DEFAULT_BRANCH}\n")
    );
}

/// The line of this file that the spec of batch 38 gives as a literal is in the spec
/// (`quality.user-facing-text-is-contract`).
#[test]
fn the_spec_lines_of_these_tests_are_in_the_spec() {
    let spec = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/specs")
            .join(SPEC_FILE),
    )
    .expect("read the spec");

    assert!(
        spec.contains(&format!("`{EMPTY_DEFAULT_BRANCH}`")),
        "{EMPTY_DEFAULT_BRANCH}"
    );
}

/// A configured branch that does not exist is one named line, not a fall
/// back to another branch.
#[test]
fn audit_names_a_configured_branch_that_does_not_exist() {
    let (repo, base) = trunk_repo(&marker_with(r#", "defaultBranch": "develop""#));

    let output = audit(repo.path(), &base);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "durable-sha: defaultBranch \"develop\" in .houserules.json names no branch. \
         Neither origin/develop nor develop exists.\n"
    );
}
