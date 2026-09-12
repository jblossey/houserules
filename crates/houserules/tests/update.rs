//! `update` CLI-level tests. Follows `install.rs`'s own structural pattern
//! (a real subprocess against a real scratch git repository, its own small
//! copy of the shared helpers) for the same reason that file gives
//! (`check_commit.rs`'s module doc): this file needs none of `install.rs`'s
//! other test-only helpers, so a `mod` share would only add dead-code
//! warnings there. The deletion mechanism itself (`RETIRED`) is exercised
//! at the unit level in `install.rs`'s own tests, which inject their own
//! list directly at the `delete_retired` call site; these CLI-level tests
//! cover the two production shapes instead: no deletion when the retired
//! paths are absent (a fresh `init` never seeds them), and
//! `update_deletes_retired_shell_tools_from_an_old_install` below,
//! `RETIRED`'s production use against an old install that still has them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// A `Command` for the compiled `houserules` binary under test --
/// `install.rs`'s own copy of this helper.
fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// This checkout's repository root -- `install.rs`'s own copy.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// A fresh scratch directory with `git init` already run -- `install.rs`'s
/// own copy. `tempfile`'s `TempDir` removes itself on drop
/// (`houserules.tests-clean-scratch-dirs`'s sanctioned Rust form).
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

/// A scratch repository already seeded by a fresh `houserules init` --
/// `update`'s own tests all start from this, since `update` presupposes an
/// already-`init`ed install. The binary does not crash on a target that
/// was never `init`ed.
/// `update_over_a_never_init_ed_target_is_a_named_error_exit_2` below pins
/// it as one named stderr line naming `<target>/knowledge/schema.json`,
/// and exit 2 -- the exact `io::Error` text and path separator are
/// platform-specific (that test's own doc has the measured Unix/Windows
/// split), so the pin derives them rather than hardcoding either. `node
/// bin/houserules.mjs update` dumps a 26-line stack trace for the same
/// missing file instead. `install.rs`'s own "Failure paths" doc section
/// carries the same account, since a target repository's `update` failure
/// modes belong there too.
fn seeded_repo() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "seed init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

/// `update` over a target that was never `init`ed is a named error, exit 2
/// -- NOT a reproduced crash (`houserules.crash-paths-are-named`),
/// `seeded_repo`'s own doc has the full account. `update` still writes
/// every `KIT_OWNED` file before it fails: the marker-absent default seeds
/// cleanly (`update_renders_none_when_the_marker_file_is_missing` already
/// pins that), and only the render step, which needs the `SEED_ONCE`
/// knowledge base `init` alone seeds, has nothing to read.
///
/// The expected stderr line is DERIVED, not hardcoded: this test performs
/// the identical fallible call the binary's failing arm makes --
/// `rules::model::load_base`'s own `fs::read_to_string` on the same
/// `root.join("knowledge").join("schema.json")` path (`install.rs`'s own
/// "Failure paths" doc section has the fuller account) -- against the same
/// path in the same directory, and formats the captured `io::Error`
/// exactly as `rules::model::LoadError::Io`'s `Display` does (`"{path}:
/// {source}"`).
///
/// On Windows, the real failure is `...\knowledge\schema.json: The system
/// cannot find the path specified. (os error 3)`, not the
/// `.../knowledge/schema.json: No such file or directory (os error 2)` a
/// hardcoded Unix message would assume -- two independent divergences at once.
/// The path separator differs because `PathBuf::join` inserts the platform's
/// own `MAIN_SEPARATOR` (`\` on Windows, `/` on Unix) between components it
/// joins itself, but a separator typed literally inside one string argument (a
/// single `.join("knowledge/schema.json")` call) is never normalized -- this
/// test joins `"knowledge"` and `"schema.json"` as two separate calls,
/// matching production's own two `.join()` calls byte-for-byte. The message
/// and code differ because a missing PARENT directory is a different Windows
/// error than a missing leaf file: `ERROR_PATH_NOT_FOUND` (3, "The system
/// cannot find the path specified.") applies here since `knowledge/` itself
/// does not exist, where `ERROR_FILE_NOT_FOUND` (2, "The system cannot find
/// the file specified.") would apply if only `schema.json` were missing
/// (Microsoft's own System Error Codes reference, WinError.h, entries 2 and
/// 3). Deriving the path and the error text from a real syscall on this
/// platform, this run, keeps the assertion exact everywhere with zero `cfg`.
#[test]
fn update_over_a_never_init_ed_target_is_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");

    let schema_path = dir.path().join("knowledge").join("schema.json");
    let read_error = fs::read_to_string(&schema_path)
        .expect_err("knowledge/schema.json must still be absent: update never seeds it");
    let expected = format!("{}: {read_error}\n", schema_path.display());
    assert_eq!(stderr, expected);
}

/// Every `KIT_OWNED` path -- `install.rs`'s own copy. `tools/kb.mjs`,
/// `tools/backlog.mjs`, `tools/lib/cli.mjs`, and `tools/lib/json-store.mjs`
/// are not in this list -- they joined `RETIRED` instead.
const KIT_OWNED: &[&str] = &[
    "tools/claude-session-start.sh",
    ".githooks/commit-msg",
    ".claude/agents/implementer.md",
    ".claude/agents/task-reviewer.md",
    ".claude/agents/branch-reviewer.md",
    ".claude/skills/orchestrating/SKILL.md",
    ".claude/skills/finishing-a-feature/SKILL.md",
    ".claude/skills/migrating-knowledge/SKILL.md",
];

/// Every `SEED_ONCE` path -- `install.rs`'s own copy.
const SEED_ONCE: &[&str] = &[
    "knowledge/schema.json",
    "knowledge/areas.json",
    "knowledge/process.json",
    "knowledge/quality.json",
    "knowledge/security-hygiene.json",
    "knowledge/writing-style.json",
    "knowledge/knowledge-base.json",
    "backlog/schema.json",
    "backlog/amendments.json",
    "backlog/batches.json",
    "backlog/decisions.json",
    "backlog/parked.json",
    "backlog/items/general.json",
    ".claude/schemas/deliverables.json",
    ".claude/evals/dependency-add.json",
    ".claude/evals/docs-edit.json",
    ".claude/evals/record.json",
    ".claude/evals/seeded-violations.json",
    ".github/workflows/knowledge.yml",
    "docs/README.md",
    "CLAUDE.md",
];

/// The lowercase hex SHA-256 digest of `content` -- this test file's own
/// copy of `install::baseline::hash`, computed independently with the same
/// `sha2` crate so a hand-stamped baseline in these tests matches exactly
/// what the binary under test would compute for the same bytes.
fn sha256_hex(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `env!("CARGO_PKG_VERSION")` at THIS test binary's own compile time --
/// `install.rs`'s own copy of `kit_version` (that function's own doc has
/// the account).
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[test]
fn update_syncs_every_unmodified_kit_owned_file_and_leaves_adopter_files_alone() {
    let dir = seeded_repo();
    fs::write(dir.path().join("adopter-notes.md"), b"mine").expect("write adopter file");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "stderr: {stderr}");

    let mut expected_lines: Vec<String> = KIT_OWNED
        .iter()
        .map(|file| format!("wrote {file}"))
        .collect();
    expected_lines.push("render: up to date".to_string());
    let version = kit_version();
    expected_lines.push(format!("kit {version} -> {version}"));
    expected_lines.push(format!("houserules: updated {}", dir.path().display()));
    expected_lines.push("next: houserules check-knowledge && houserules check-backlog".to_string());
    let actual_lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(actual_lines, expected_lines);

    assert_eq!(
        fs::read(dir.path().join("adopter-notes.md")).expect("read adopter file"),
        b"mine",
        "update touched an adopter-owned file"
    );
}

/// A `KIT_OWNED` file the adopter has edited since the last `update`
/// diverges from its recorded baseline: `update` keeps the adopter's own
/// content, reports it once, and does NOT resync it -- the behavior this
/// module's own "update and the ownership baseline" doc section describes,
/// replacing the unconditional overwrite an earlier release always ran.
#[test]
fn update_keeps_a_locally_modified_kit_owned_file_and_reports_it_once() {
    let dir = seeded_repo();
    fs::write(
        dir.path().join(".claude/agents/implementer.md"),
        b"corrupted",
    )
    .expect("corrupt .claude/agents/implementer.md");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("kept .claude/agents/implementer.md (locally modified)\n"),
        "got:\n{stdout}"
    );
    assert!(
        !stdout.contains("wrote .claude/agents/implementer.md\n"),
        "got:\n{stdout}"
    );

    assert_eq!(
        fs::read(dir.path().join(".claude/agents/implementer.md")).unwrap(),
        b"corrupted",
        "update overwrote the adopter's own change"
    );
}

/// A `KIT_OWNED` file the adopter has NOT changed since the kit last wrote
/// it -- still at its recorded baseline -- gets replaced with whatever the
/// running kit ships now, even when that differs from both the adopter's
/// current content and the running binary's own `template/` copy. A real
/// version bump is simulated by hand: the file on disk is set to one
/// string, its recorded baseline stamped to that same string's hash (so
/// `update` reads it as untouched), proving the replacement is driven by
/// the baseline comparison, not by a byte-for-byte match with the payload.
#[test]
fn update_replaces_an_at_baseline_kit_owned_file_with_the_running_kits_content() {
    let dir = seeded_repo();
    let file = ".claude/agents/implementer.md";
    let old_kit_content = b"content from an older kit release\n";
    fs::write(dir.path().join(file), old_kit_content).expect("write old kit content");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["baselines"][file] = serde_json::json!(sha256_hex(old_kit_content));
    fs::write(&marker_path, serde_json::to_string_pretty(&marker).unwrap()).unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("wrote {file}\n")),
        "got:\n{stdout}"
    );

    let running_content = fs::read(repo_root().join("template").join(file)).unwrap();
    assert_eq!(
        fs::read(dir.path().join(file)).unwrap(),
        running_content,
        "an at-baseline file was not replaced with the running kit's content"
    );

    let restamped: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    assert_eq!(
        restamped["baselines"][file],
        serde_json::json!(sha256_hex(&running_content))
    );
}

/// An override silences reporting for a locally modified `KIT_OWNED` file
/// entirely: no report line, and the adopter's own content is kept exactly
/// as a plain modified file's would be, but with nothing printed about it.
#[test]
fn update_silences_a_locally_modified_kit_owned_file_listed_in_overrides() {
    let dir = seeded_repo();
    let file = ".claude/agents/implementer.md";
    fs::write(dir.path().join(file), b"adopter owns this now").expect("corrupt file");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["overrides"] = serde_json::json!([file]);
    fs::write(&marker_path, serde_json::to_string_pretty(&marker).unwrap()).unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(file), "got:\n{stdout}");
    assert_eq!(
        fs::read(dir.path().join(file)).unwrap(),
        b"adopter owns this now"
    );

    let restamped: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    assert_eq!(
        restamped["overrides"],
        serde_json::json!([file]),
        "update did not preserve the overrides field"
    );
}

/// A `KIT_OWNED` file the adopter deleted outright, with no override, is
/// kit machinery the adopter has not claimed: `update` restores it, the
/// same outcome an at-baseline file gets, rather than merely reporting it
/// gone.
#[test]
fn update_restores_a_deleted_kit_owned_file() {
    let dir = seeded_repo();
    let file = ".claude/agents/implementer.md";
    fs::remove_file(dir.path().join(file)).expect("delete file");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("wrote {file}\n")),
        "got:\n{stdout}"
    );
    assert!(!stdout.contains("skipped"), "got:\n{stdout}");

    let restored = fs::read(dir.path().join(file)).expect("file was restored");
    let template = fs::read(repo_root().join("template").join(file)).unwrap();
    assert_eq!(restored, template);
}

/// A `KIT_OWNED` path that fails to read for a reason OTHER than absence is
/// a named error, exit 2 -- never silently classified as at-baseline. Only
/// `io::ErrorKind::NotFound` means "restore it"; every other read failure
/// is reported instead. Unix-only (`houserules.platform-gated-tests`): the
/// reproduction needs a real permission distinction between reading and
/// writing the same file, which POSIX mode bits give directly and Windows
/// does not.
///
/// A directory standing in for the file is not a genuine reproduction
/// here: a directory read failure looks similar-shaped, since a
/// directory also fails the subsequent write attempt. A file the owner
/// can WRITE but not READ is the reproduction that actually
/// discriminates: a read failure that is not `io::ErrorKind::NotFound`
/// must exit 2 and leave the file's content untouched, never fall
/// through to a write -- write-only permission is enough for
/// `fs::write`'s open call, so treating the read failure as "safe to
/// overwrite" would silently replace the file's content with the
/// payload's, with no error and no report naming what happened.
#[cfg(unix)]
#[test]
fn update_reports_a_named_error_instead_of_silently_overwriting_an_unreadable_kit_owned_file() {
    use std::os::unix::fs::PermissionsExt;

    let dir = seeded_repo();
    let file = ".githooks/commit-msg";
    let path = dir.path().join(file);
    let original = fs::read(&path).expect("read the seeded file before restricting it");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o200))
        .expect("chmod the file write-only, unreadable even to its own owner");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");

    // Restore a readable mode so the assertion below can read the file back.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("restore a readable mode");

    assert_eq!(
        output.status.code(),
        Some(2),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with(&format!("{}: ", path.display())),
        "got {stderr:?}"
    );
    assert_eq!(
        fs::read(&path).expect("read the file back"),
        original,
        "update silently overwrote a file it could not read"
    );
}

/// An override on a deleted `KIT_OWNED` file's path is the adopter's own
/// declaration that they deleted it on purpose: `update` leaves it absent
/// and reports nothing.
#[test]
fn update_leaves_an_overridden_deleted_kit_owned_file_absent() {
    let dir = seeded_repo();
    let file = ".claude/agents/implementer.md";
    fs::remove_file(dir.path().join(file)).expect("delete file");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["overrides"] = serde_json::json!([file]);
    fs::write(&marker_path, serde_json::to_string_pretty(&marker).unwrap()).unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(file), "got:\n{stdout}");
    assert!(
        !dir.path().join(file).exists(),
        "update restored an overridden file"
    );
}

/// The one knowledge entry the entry-upsert tests below all exercise.
const TEST_ENTRY_ID: &str = "process.ask-when-missing";
const TEST_ENTRY_TOPIC_FILE: &str = "knowledge/process.json";

/// Reads `dir`'s `topic_file` and returns its `entries` array, in on-disk
/// order.
fn read_topic_entries(dir: &Path, topic_file: &str) -> Vec<serde_json::Value> {
    let content = fs::read_to_string(dir.join(topic_file)).expect("read topic file");
    let value: serde_json::Value = serde_json::from_str(&content).expect("parse topic file");
    value["entries"]
        .as_array()
        .expect("entries is an array")
        .clone()
}

/// Rewrites `dir`'s `topic_file` with `entries` as its new `entries` array,
/// in the same pretty-printed, trailing-newline shape `update` itself
/// writes, so a later byte comparison against `update`'s own output stays
/// meaningful.
fn write_topic_entries(dir: &Path, topic_file: &str, entries: Vec<serde_json::Value>) {
    let path = dir.join(topic_file);
    let mut value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    value["entries"] = serde_json::Value::Array(entries);
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
    )
    .expect("write topic file");
}

/// The canonical bytes one knowledge entry hashes to -- this test file's own
/// copy of `install::canonical_entry_bytes`: a plain `serde_json::to_vec`,
/// deterministic for a given parsed `Value` because `serde_json`'s
/// `preserve_order` feature (this crate's own `Cargo.toml`) keeps an
/// object's key order exactly as parsed.
fn canonical_entry_bytes(value: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("a JSON Value always serializes")
}

/// Sets `dir`'s `.houserules.json` `baselines.<key>` to `hash`.
fn set_marker_baseline(dir: &Path, key: &str, hash: &str) {
    let marker_path = dir.join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["baselines"][key] = serde_json::json!(hash);
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();
}

/// An entry the adopter has NOT changed since the kit last wrote it gets
/// replaced with whatever the running kit ships now, even when that differs
/// from the entry's current content -- the same hand-stamped-baseline proof
/// `update_replaces_an_at_baseline_kit_owned_file_with_the_running_kits_
/// content` runs for a whole `KIT_OWNED` file, here at one entry's
/// granularity: the entry on disk is set to an "older release" shape, its
/// recorded baseline stamped to that shape's own hash, and `update` is
/// proven to replace it with the real, running `template/` content rather
/// than leaving the older shape in place.
#[test]
fn update_replaces_an_at_baseline_knowledge_entry_with_the_running_kits_content() {
    let dir = seeded_repo();
    let mut entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    let index = entries
        .iter()
        .position(|entry| entry["id"] == TEST_ENTRY_ID)
        .expect("test entry present in a fresh seed");
    let running_entry = entries[index].clone();

    let mut old_release_entry = running_entry.clone();
    old_release_entry["summary"] = serde_json::json!("an older release's summary text");
    entries[index] = old_release_entry.clone();
    write_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE, entries);
    set_marker_baseline(
        dir.path(),
        TEST_ENTRY_ID,
        &sha256_hex(&canonical_entry_bytes(&old_release_entry)),
    );

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(TEST_ENTRY_ID), "got:\n{stdout}");
    assert!(
        stdout.contains(&format!(
            "updated {TEST_ENTRY_TOPIC_FILE} (1 entry changed)\n"
        )),
        "got:\n{stdout}"
    );

    let entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    let entry = entries
        .iter()
        .find(|entry| entry["id"] == TEST_ENTRY_ID)
        .expect("entry still present");
    assert_eq!(
        entry, &running_entry,
        "an at-baseline entry was not replaced with the running kit's content"
    );

    let marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join(".houserules.json")).unwrap())
            .unwrap();
    assert_eq!(
        marker["baselines"][TEST_ENTRY_ID],
        serde_json::json!(sha256_hex(&canonical_entry_bytes(&running_entry)))
    );
}

/// An entry the adopter edited diverges from its recorded baseline:
/// `update` keeps the adopter's own content and reports it once, the same
/// contract a `KIT_OWNED` file gets.
#[test]
fn update_keeps_a_locally_modified_knowledge_entry_and_reports_it_once() {
    let dir = seeded_repo();
    let mut entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    let index = entries
        .iter()
        .position(|entry| entry["id"] == TEST_ENTRY_ID)
        .expect("test entry present");
    entries[index]["summary"] = serde_json::json!("the adopter's own wording");
    let modified = entries.clone();
    write_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE, entries);

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("kept {TEST_ENTRY_ID} (locally modified)\n")),
        "got:\n{stdout}"
    );

    assert_eq!(
        read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE),
        modified
    );
}

/// An entry the adopter deleted outright -- present at baseline, then
/// removed from the file entirely -- is left absent and reported once;
/// nothing restores it.
#[test]
fn update_respects_an_adopter_deleted_knowledge_entry_and_reports_it_once() {
    let dir = seeded_repo();
    let mut entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    entries.retain(|entry| entry["id"] != TEST_ENTRY_ID);
    write_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE, entries);

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("skipped {TEST_ENTRY_ID} (deleted)\n")),
        "got:\n{stdout}"
    );

    let entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    assert!(!entries.iter().any(|entry| entry["id"] == TEST_ENTRY_ID));
}

/// An override on a deleted entry's id silences the report entirely; the
/// entry stays absent either way.
#[test]
fn update_silences_an_adopter_deleted_knowledge_entry_listed_in_overrides() {
    let dir = seeded_repo();
    let mut entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    entries.retain(|entry| entry["id"] != TEST_ENTRY_ID);
    write_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE, entries);

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["overrides"] = serde_json::json!([TEST_ENTRY_ID]);
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(TEST_ENTRY_ID), "got:\n{stdout}");
    assert!(
        !read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE)
            .iter()
            .any(|entry| entry["id"] == TEST_ENTRY_ID)
    );
}

/// An entry the install never had -- absent from the file, with no baseline
/// ever recorded for its id -- is written for the first time and stamped,
/// silently: the same shape a genuinely new entry a later kit release adds
/// would take, simulated here by removing both the entry and its baseline
/// from an already-seeded install.
#[test]
fn update_writes_a_knowledge_entry_the_install_never_had() {
    let dir = seeded_repo();
    let mut entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    let index = entries
        .iter()
        .position(|entry| entry["id"] == TEST_ENTRY_ID)
        .expect("test entry present");
    let expected_entry = entries.remove(index);
    write_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE, entries);

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["baselines"]
        .as_object_mut()
        .unwrap()
        .remove(TEST_ENTRY_ID);
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(TEST_ENTRY_ID), "got:\n{stdout}");
    assert!(
        stdout.contains(&format!(
            "updated {TEST_ENTRY_TOPIC_FILE} (1 entry changed)\n"
        )),
        "got:\n{stdout}"
    );

    let entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    assert!(entries.contains(&expected_entry), "got {entries:?}");

    let marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    assert_eq!(
        marker["baselines"][TEST_ENTRY_ID],
        serde_json::json!(sha256_hex(&canonical_entry_bytes(&expected_entry)))
    );
}

/// A `SEED_ONCE` path a later kit release adds is missing from an install
/// seeded by an earlier one. `update` backfills it, since absence with no
/// override means "never arrived", not "deleted on purpose".
#[test]
fn update_backfills_a_missing_seed_once_file() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join("docs/README.md")).expect("remove docs/README.md");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("wrote docs/README.md\n"), "got:\n{stdout}");

    let backfilled = fs::read(dir.path().join("docs/README.md")).unwrap();
    let template = fs::read(repo_root().join("template/docs/README.md")).unwrap();
    assert_eq!(backfilled, template);
}

/// An override on a missing `SEED_ONCE` path means "deleted on purpose":
/// `update` leaves it absent and prints nothing about it.
#[test]
fn update_does_not_backfill_an_overridden_seed_once_file() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join("docs/README.md")).expect("remove docs/README.md");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["overrides"] = serde_json::json!(["docs/README.md"]);
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains("docs/README.md"), "got:\n{stdout}");
    assert!(!dir.path().join("docs/README.md").exists());
}

/// A knowledge-topic file the adopter deleted outright is backfilled in
/// full, the same "never arrived" contract every other missing `SEED_ONCE`
/// path gets -- never a half-populated husk that reports every one of its
/// entries deleted.
#[test]
fn update_backfills_a_missing_knowledge_topic_file_in_full() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join(TEST_ENTRY_TOPIC_FILE)).expect("remove topic file");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("wrote {TEST_ENTRY_TOPIC_FILE}\n")),
        "got:\n{stdout}"
    );
    assert!(!stdout.contains("skipped"), "got:\n{stdout}");
    assert!(!stdout.contains("deleted"), "got:\n{stdout}");

    let backfilled = fs::read(dir.path().join(TEST_ENTRY_TOPIC_FILE)).unwrap();
    let template = fs::read(repo_root().join("template").join(TEST_ENTRY_TOPIC_FILE)).unwrap();
    assert_eq!(backfilled, template);

    let entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    assert!(
        entries.iter().any(|entry| entry["id"] == TEST_ENTRY_ID),
        "the backfilled file is missing its own kit entries: {entries:?}"
    );

    let marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join(".houserules.json")).unwrap())
            .unwrap();
    assert!(
        marker["baselines"][TEST_ENTRY_ID].is_string(),
        "the backfilled file's entries were not stamped: {marker}"
    );
}

/// An override on a knowledge-topic file's own path governs the whole file,
/// not just one entry id: the file stays exactly as the adopter left it --
/// absent, in this case -- and update prints nothing about it.
#[test]
fn update_leaves_an_overridden_missing_knowledge_topic_file_absent() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join(TEST_ENTRY_TOPIC_FILE)).expect("remove topic file");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["overrides"] = serde_json::json!([TEST_ENTRY_TOPIC_FILE]);
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(TEST_ENTRY_TOPIC_FILE), "got:\n{stdout}");
    assert!(!dir.path().join(TEST_ENTRY_TOPIC_FILE).exists());
}

/// An override on a knowledge-topic file's own path also governs a file
/// that is PRESENT but has locally modified entries: the whole file is left
/// exactly as found, and no per-entry report line fires for it either.
#[test]
fn update_leaves_an_overridden_present_knowledge_topic_file_untouched() {
    let dir = seeded_repo();
    let mut entries = read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE);
    let index = entries
        .iter()
        .position(|entry| entry["id"] == TEST_ENTRY_ID)
        .expect("test entry present");
    entries[index]["summary"] = serde_json::json!("the adopter's own wording");
    write_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE, entries.clone());

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["overrides"] = serde_json::json!([TEST_ENTRY_TOPIC_FILE]);
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(TEST_ENTRY_ID), "got:\n{stdout}");
    assert!(!stdout.contains(TEST_ENTRY_TOPIC_FILE), "got:\n{stdout}");
    assert_eq!(
        read_topic_entries(dir.path(), TEST_ENTRY_TOPIC_FILE),
        entries
    );
}

/// The `SEED_ONCE` backfill rewrites `PREFIXED` payload content with the
/// install's own stamped `idPrefix`, never the `--id-prefix` flag's value
/// (which defaults to `WI`): backfilling into a non-`WI` install must not
/// hand it ids `check-backlog` then rejects.
#[test]
fn update_backfills_a_prefixed_file_with_the_installs_own_stamped_prefix() {
    let dir = scratch_git_repo();
    let init = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .args(["--id-prefix", "FOO"])
        .output()
        .expect("run init");
    assert!(
        init.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    fs::remove_file(dir.path().join("backlog/items/general.json"))
        .expect("remove backlog/items/general.json");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update (no --id-prefix flag: must not default to WI)");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("wrote backlog/items/general.json\n"),
        "got:\n{stdout}"
    );

    let backfilled = fs::read_to_string(dir.path().join("backlog/items/general.json")).unwrap();
    assert!(backfilled.contains("\"FOO-001\""), "{backfilled}");
    assert!(!backfilled.contains("WI-"), "{backfilled}");

    let check_backlog = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert!(
        check_backlog.status.success(),
        "check-backlog failed after the backfill: {}",
        String::from_utf8_lossy(&check_backlog.stderr)
    );
}

/// An install stamped by the pre-baseline format -- `.houserules.json` with
/// no `baselines` field at all -- needs no separate migration code path:
/// `update` treats every item's unrecorded baseline as "compare directly
/// against the current payload". This one run proves both of `baseline::
/// classify`'s no-baseline arms at once, each on a different `KIT_OWNED`
/// file: `.claude/agents/task-reviewer.md`, left exactly as `seeded_repo`
/// wrote it, matches the payload and is silently written and stamped
/// (`Status::AtBaseline`); `.claude/agents/implementer.md`, hand-corrupted
/// below, diverges from the payload and is kept, reported once, and left
/// unstamped for a later run to reconcile (`Status::Modified`). Nothing on
/// disk is overwritten by the divergent file; both outcomes coexist in one
/// `update` invocation, since neither file has a recorded baseline yet.
#[test]
fn update_migration_run_reconciles_kit_owned_files_with_no_baselines_recorded() {
    let dir = seeded_repo();
    let unmodified_file = ".claude/agents/task-reviewer.md";
    let modified_file = ".claude/agents/implementer.md";
    fs::write(dir.path().join(modified_file), b"pre-baseline adopter edit")
        .expect("corrupt modified_file");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker
        .as_object_mut()
        .unwrap()
        .remove("baselines")
        .expect("seeded_repo stamps baselines");
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains(&format!("wrote {unmodified_file}\n")),
        "got:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("kept {modified_file} (locally modified)\n")),
        "got:\n{stdout}"
    );
    assert!(
        !stdout.contains(&format!("wrote {modified_file}\n")),
        "got:\n{stdout}"
    );

    assert_eq!(
        fs::read(dir.path().join(modified_file)).unwrap(),
        b"pre-baseline adopter edit",
        "the divergent file was overwritten during migration"
    );

    let restamped: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    let baselines = restamped["baselines"]
        .as_object()
        .expect("baselines object");
    assert!(baselines.contains_key(unmodified_file), "got {baselines:?}");
    assert!(
        !baselines.contains_key(modified_file),
        "a divergent file should stay unstamped until the adopter resolves it: got {baselines:?}"
    );
}

/// `tools/kb.mjs`, `tools/backlog.mjs`, `tools/lib/cli.mjs`, and
/// `tools/lib/json-store.mjs` (`update_deletes_retired_shell_tools_from_
/// an_old_install`'s own doc explains why `tools/kb.sh`/`tools/backlog.sh`
/// get their own, separate test) are `RETIRED` too -- an install seeded
/// before they retired still carries them, and `update` deletes all four
/// in the same run.
#[test]
fn update_deletes_retired_js_engines_from_an_old_install() {
    let dir = seeded_repo();
    fs::create_dir_all(dir.path().join("tools/lib")).expect("mkdir tools/lib");
    for file in [
        "tools/kb.mjs",
        "tools/backlog.mjs",
        "tools/lib/cli.mjs",
        "tools/lib/json-store.mjs",
    ] {
        fs::write(dir.path().join(file), b"old\n")
            .unwrap_or_else(|error| panic!("write a pre-T3 {file}: {error}"));
    }

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let removed: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("removed "))
        .collect();
    assert_eq!(
        removed,
        [
            "removed tools/kb.mjs",
            "removed tools/backlog.mjs",
            "removed tools/lib/cli.mjs",
            "removed tools/lib/json-store.mjs",
        ]
    );
    for file in [
        "tools/kb.mjs",
        "tools/backlog.mjs",
        "tools/lib/cli.mjs",
        "tools/lib/json-store.mjs",
    ] {
        assert!(!dir.path().join(file).exists(), "{file} still present");
    }
}

#[test]
fn update_leaves_seed_once_files_and_settings_untouched() {
    let dir = seeded_repo();
    fs::write(dir.path().join("CLAUDE.md"), b"project notes").expect("edit CLAUDE.md");
    fs::write(
        dir.path().join(".claude/settings.json"),
        b"{\"hooks\":{}}\n",
    )
    .expect("edit settings.json");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();

    for file in SEED_ONCE {
        assert!(
            !stdout.contains(file),
            "update reported {file}, a SEED_ONCE path, in:\n{stdout}"
        );
    }
    assert!(
        !stdout.contains("settings.json"),
        "update reported .claude/settings.json in:\n{stdout}"
    );
    assert_eq!(
        fs::read(dir.path().join("CLAUDE.md")).unwrap(),
        b"project notes"
    );
    assert_eq!(
        fs::read(dir.path().join(".claude/settings.json")).unwrap(),
        b"{\"hooks\":{}}\n"
    );
}

/// A fresh `init` never seeds `RETIRED`'s two paths (they are not in
/// `KIT_OWNED`), so a plain `update` over a freshly seeded install finds
/// neither present and reports no deletion (`install.rs`'s own "Deletion"
/// doc section); the mechanism itself is unit-tested there with an
/// injected list, and `update_deletes_retired_shell_tools_from_an_old_
/// install` below covers the shape where a `RETIRED` path IS present. This
/// only confirms the CLI-visible half of the absent case: no spurious
/// `removed <path>` line and no unexpected deletion.
#[test]
fn update_reports_no_deletions_when_the_retired_paths_are_absent() {
    let dir = seeded_repo();
    let before: Vec<_> = walk_relative(dir.path());

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.lines().any(|line| line.starts_with("removed ")),
        "unexpected deletion report in:\n{stdout}"
    );

    let after: Vec<_> = walk_relative(dir.path());
    assert_eq!(before, after, "update changed the set of files present");
}

/// An install seeded before `tools/kb.sh`/`tools/backlog.sh` retired (or
/// by the still-frozen `node bin/houserules.mjs init`, which still writes
/// both) carries them; the next `update` deletes both, reports each
/// `removed <path>` in `RETIRED`'s own call order, and still resyncs
/// every current `KIT_OWNED` file alongside them in the same run.
#[test]
fn update_deletes_retired_shell_tools_from_an_old_install() {
    let dir = seeded_repo();
    fs::write(
        dir.path().join("tools/kb.sh"),
        b"#!/usr/bin/env bash\nold\n",
    )
    .expect("write a pre-T5 tools/kb.sh");
    fs::write(
        dir.path().join("tools/backlog.sh"),
        b"#!/usr/bin/env bash\nold\n",
    )
    .expect("write a pre-T5 tools/backlog.sh");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let removed: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("removed "))
        .collect();
    assert_eq!(removed, ["removed tools/kb.sh", "removed tools/backlog.sh"]);

    assert!(!dir.path().join("tools/kb.sh").exists());
    assert!(!dir.path().join("tools/backlog.sh").exists());
    for file in KIT_OWNED {
        assert!(
            dir.path().join(file).is_file(),
            "{file} missing after update"
        );
    }
}

/// Every path under `root`, relative to `root`, sorted -- used to prove
/// `update` neither adds nor removes any unrelated file.
fn walk_relative(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("read_dir") {
            let entry = entry.expect("dir entry");
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
                continue;
            }
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                out.push(path.strip_prefix(root).unwrap().to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

#[test]
fn update_renders_none_when_the_marker_file_is_missing() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join(".houserules.json")).expect("remove marker");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let version = kit_version();
    assert!(
        stdout.contains(&format!("kit none -> {version}\n")),
        "got:\n{stdout}"
    );

    let marker: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".houserules.json")).expect("read marker"),
    )
    .expect("parse marker");
    assert_eq!(marker["version"], serde_json::json!(version));
    assert_eq!(marker["idPrefix"], serde_json::json!("WI"));
    assert!(marker["baselines"].is_object(), "got {marker:?}");
}

#[test]
fn update_renders_none_when_the_markers_version_field_is_absent_but_keeps_its_id_prefix() {
    let dir = seeded_repo();
    fs::write(dir.path().join(".houserules.json"), r#"{"idPrefix":"FOO"}"#).expect("write marker");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let version = kit_version();
    assert!(
        stdout.contains(&format!("kit none -> {version}\n")),
        "got:\n{stdout}"
    );

    let marker: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".houserules.json")).expect("read marker"),
    )
    .expect("parse marker");
    assert_eq!(marker["version"], serde_json::json!(version));
    assert_eq!(marker["idPrefix"], serde_json::json!("FOO"));
    assert!(marker["baselines"].is_object(), "got {marker:?}");
}

#[test]
fn update_reports_the_stamped_to_running_version_drift_when_a_version_is_already_stamped() {
    let dir = seeded_repo();
    fs::write(
        dir.path().join(".houserules.json"),
        r#"{"version":"0.0.1-old","idPrefix":"WI"}"#,
    )
    .expect("write marker");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let version = kit_version();
    assert!(
        stdout.contains(&format!("kit 0.0.1-old -> {version}\n")),
        "got:\n{stdout}"
    );
}

/// A marker that exists and carries a `version`, but no `idPrefix` key at
/// all, falls back to `--id-prefix` (or `WI`) for the restamped `idPrefix`
/// -- the same fallback `init`'s own missing-marker default already
/// exercises for the whole-file-absent case, here for the key-absent-in-a-
/// present-file case instead.
#[test]
fn update_defaults_the_id_prefix_from_the_flag_when_the_marker_has_no_id_prefix_key() {
    let dir = seeded_repo();
    fs::write(
        dir.path().join(".houserules.json"),
        r#"{"version":"1.2.3"}"#,
    )
    .expect("write marker");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .args(["--id-prefix", "ZED"])
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let version = kit_version();
    assert!(
        stdout.contains(&format!("kit 1.2.3 -> {version}\n")),
        "got:\n{stdout}"
    );

    let marker: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".houserules.json")).expect("read marker"),
    )
    .expect("parse marker");
    assert_eq!(marker["version"], serde_json::json!(version));
    assert_eq!(marker["idPrefix"], serde_json::json!("ZED"));
    assert!(marker["baselines"].is_object(), "got {marker:?}");
}

#[test]
fn update_into_a_missing_git_repo_is_a_named_usage_error_exit_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{} is not a git repository (run git init first)\n",
            dir.path().display()
        )
    );
}

#[test]
fn update_rejects_a_malformed_id_prefix_flag_exit_2() {
    let dir = seeded_repo();
    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .args(["--id-prefix", "lowercase"])
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "id-prefix must be 1-8 characters, A-Z then A-Z0-9\n"
    );
}

/// Every invalid `.houserules.json` shape below prints one named stderr
/// line (`<marker path>: <suffix>`), exit 2, with no `wrote <file>` line
/// ahead of it. Seeds its own scratch repo so the expected message's
/// marker path and the actual one always agree.
fn assert_named_error_before_any_write(marker_content: &[u8], expected_stderr_suffix: &str) {
    let dir = seeded_repo();
    let marker_path = dir.path().join(".houserules.json");
    fs::write(&marker_path, marker_content).expect("write marker");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"", "update wrote output before failing");
    let stderr = String::from_utf8(output.stderr).unwrap();
    let expected_prefix = format!("{}: {expected_stderr_suffix}", marker_path.display());
    assert!(
        stderr.starts_with(&expected_prefix),
        "expected prefix {expected_prefix:?}, got {stderr:?}"
    );
}

#[test]
fn update_reports_an_unparsable_marker_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(b"not json", "invalid JSON (");
}

#[test]
fn update_reports_a_non_object_marker_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(br#"["a","b"]"#, "not a JSON object\n");
}

#[test]
fn update_reports_a_markers_bad_id_prefix_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(
        br#"{"idPrefix":"lowercase"}"#,
        "idPrefix must be 1-8 characters, A-Z then A-Z0-9\n",
    );
}

#[test]
fn update_reports_a_markers_empty_version_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(
        br#"{"version":""}"#,
        "version must be a non-empty string\n",
    );
}

/// A JSON `null` at `version` is present, not absent -- it fails the
/// non-empty-string check rather than reading as a missing stamp
/// (`install.rs`'s "update" doc section names this measured distinction
/// from the `none` drift arm).
#[test]
fn update_reports_a_markers_null_version_as_a_named_error_exit_2_not_as_none() {
    assert_named_error_before_any_write(
        br#"{"version":null}"#,
        "version must be a non-empty string\n",
    );
}

/// `overrides` is adopter-hand-edited data: a present value that is not an
/// array of strings is one named error, never a silent empty default -- the
/// same contract `idPrefix` and `version` already get.
#[test]
fn update_reports_a_markers_non_array_overrides_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(
        br#"{"overrides":"docs/README.md"}"#,
        "overrides must be an array of strings\n",
    );
}

/// An `overrides` array holding a non-string element is equally malformed.
#[test]
fn update_reports_a_markers_overrides_array_with_a_non_string_element_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(
        br#"{"overrides":["docs/README.md", 5]}"#,
        "overrides must be an array of strings\n",
    );
}

/// `baselines` is adopter-visible, hand-editable data too (documented at
/// `docs/README.md`): a present value that is not an object of string
/// values is one named error, never a silent empty default.
#[test]
fn update_reports_a_markers_non_object_baselines_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(
        br#"{"baselines":["a","b"]}"#,
        "baselines must be an object of strings\n",
    );
}

/// A `baselines` object holding a non-string value is equally malformed.
#[test]
fn update_reports_a_markers_baselines_object_with_a_non_string_value_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(
        br#"{"baselines":{"process.ask-when-missing": 5}}"#,
        "baselines must be an object of strings\n",
    );
}

/// `update` rewrites only the marker fields it owns (`version`, `idPrefix`,
/// `baselines`): any other key an adopter hand-added survives a restamp
/// untouched.
#[test]
fn update_preserves_an_unknown_marker_key_across_a_restamp() {
    let dir = seeded_repo();
    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["notes"] = serde_json::json!("adopter's own field");
    fs::write(
        &marker_path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .unwrap();

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let restamped: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    assert_eq!(
        restamped["notes"],
        serde_json::json!("adopter's own field"),
        "update dropped an unknown marker key"
    );
}

#[test]
fn update_bare_dir_flag_exits_2_with_claps_value_required_message() {
    let output = houserules()
        .args(["update", "--dir"])
        .output()
        .expect("run update --dir");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: a value is required for '--dir <DIR>' but none was supplied"),
        "got {stderr:?}"
    );
}

#[test]
fn update_duplicated_dir_flag_exits_2_with_claps_cannot_be_used_multiple_times_message() {
    let output = houserules()
        .args(["update", "--dir", "a", "--dir", "b"])
        .output()
        .expect("run update --dir a --dir b");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--dir <DIR>' cannot be used multiple times"),
        "got {stderr:?}"
    );
}
