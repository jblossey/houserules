//! The kit's `knowledge-base.summary-is-the-rule` check, run the way an adopter
//! runs it: a scratch install from `houserules init`, one commit on top, and
//! `houserules audit` over the range. The check is a `warn` row that names a
//! changed knowledge file holding time-sensitive wording. The check reads the
//! knowledge base and its kit source only, never product data that sits under
//! another directory named `knowledge`. The pattern spells each word with one
//! bracketed letter, so the knowledge file that holds the pattern never matches
//! it.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// The knowledge entry whose check the tests run.
const RULE: &str = "knowledge-base.summary-is-the-rule";

/// The wording the check names, as a writer types it: every spelling that the
/// tracked knowledge files or a reviewer's probe showed, a curly apostrophe and
/// a bare "today" included.
const PHRASES: [&str; 8] = [
    "Currently",
    "for now",
    "at the moment",
    "as of today",
    "today's",
    "Today\u{2019}s",
    "The list today",
    "until HR-048 ships a distribution channel",
];

/// A `Command` for the compiled `houserules` binary under test.
/// Sets `HOUSERULES_SKIP_SELF_UPDATE` so `update`'s self-update phase never runs
/// (`tests/update.rs` has the full account).
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// Runs `git` in `root`, panics with its stderr on failure, and returns its stdout.
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

/// Commits every change under `root` and returns the new commit id.
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
            "-m",
            message,
        ],
    );
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

/// A scratch repository holding the kit that `houserules init` installs, in one
/// base commit. Returns the directory and the base commit id.
fn install() -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let base = commit(dir.path(), "chore: init");
    (dir, base)
}

/// Replaces the first `from` in `file` (relative to `root`) with `to`.
fn replace_in(root: &Path, file: &str, from: &str, to: &str) {
    let path = root.join(file);
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {file}: {error}"));
    assert!(text.contains(from), "{file} holds no {from:?}");
    fs::write(&path, text.replacen(from, to, 1)).expect("write the edited file");
}

/// The audit row of `RULE` for the range from `base` to `HEAD` in `root`.
fn rule_row(root: &Path, base: &str) -> Value {
    let output = houserules()
        .args(["audit", "--base", base, "--ids", RULE, "--dir"])
        .arg(root)
        .output()
        .expect("run audit");
    assert!(
        output.status.success(),
        "audit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let audit: Value = serde_json::from_slice(&output.stdout).expect("audit prints JSON");
    audit["rules"]
        .as_array()
        .expect("rules is an array")
        .iter()
        .find(|row| row["id"] == RULE)
        .unwrap_or_else(|| panic!("no audit row for {RULE}"))
        .clone()
}

#[test]
fn a_time_sensitive_phrase_in_a_changed_knowledge_file_draws_a_warn_row() {
    for phrase in PHRASES {
        let (dir, base) = install();
        replace_in(
            dir.path(),
            "knowledge/process.json",
            "\"summary\": \"Test-driven",
            &format!("\"summary\": \"{phrase} test-driven"),
        );
        commit(dir.path(), "docs: add a phrase");

        let row = rule_row(dir.path(), &base);

        assert_eq!(row["mode"], "deterministic", "{phrase}: {row}");
        assert_eq!(row["level"], "warn", "{phrase}: {row}");
        assert_eq!(row["result"], "warn", "{phrase}: {row}");
        let evidence = row["evidence"].as_str().expect("evidence is a string");
        assert!(
            evidence.starts_with("knowledge/process.json:"),
            "{phrase}: {row}"
        );
    }
}

#[test]
fn the_check_reaches_the_knowledge_files_of_a_kit_source_directory() {
    let (dir, base) = install();
    fs::create_dir_all(dir.path().join("template/knowledge")).expect("create the directory");
    fs::write(
        dir.path().join("template/knowledge/process.json"),
        "{\"summary\": \"Do it for now.\"}\n",
    )
    .expect("write the kit source file");
    commit(dir.path(), "docs: add a kit source file");

    let row = rule_row(dir.path(), &base);

    assert_eq!(row["result"], "warn", "{row}");
    let evidence = row["evidence"].as_str().expect("evidence is a string");
    assert!(
        evidence.starts_with("template/knowledge/process.json:"),
        "{row}"
    );
}

#[test]
fn a_changed_json_file_under_another_knowledge_directory_passes_the_check() {
    let (dir, base) = install();
    fs::create_dir_all(dir.path().join("src/assistant/knowledge")).expect("create the directory");
    fs::write(
        dir.path().join("src/assistant/knowledge/faq.json"),
        "{\"answer\": \"We are currently closed\"}\n",
    )
    .expect("write the product data file");
    commit(dir.path(), "docs: add product data");

    let row = rule_row(dir.path(), &base);

    assert_eq!(row["result"], "pass", "{row}");
    assert_eq!(row["evidence"], "0 files checked", "{row}");
}

#[test]
fn a_change_without_such_phrasing_passes_even_in_the_file_that_holds_the_pattern() {
    let (dir, base) = install();
    replace_in(
        dir.path(),
        "knowledge/knowledge-base.json",
        "\"authoring\"",
        "\"authoring\",\n        \"wording\"",
    );
    commit(dir.path(), "docs: tag the entry");

    let row = rule_row(dir.path(), &base);

    assert_eq!(row["mode"], "deterministic", "{row}");
    assert_eq!(row["result"], "pass", "{row}");
    assert_eq!(row["evidence"], "1 files checked", "{row}");
}
