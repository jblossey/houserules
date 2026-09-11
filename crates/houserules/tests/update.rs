//! `update` CLI-level tests (HR-047 phase-3 slice, batch 18 T4,
//! docs/specs/2026-09-05-batch-18-phase3.md §1). Follows `install.rs`'s own
//! structural pattern (a real subprocess against a real scratch git
//! repository, its own small copy of the shared helpers) for the same
//! reason that file gives (`check_commit.rs`'s module doc): this file needs
//! none of `install.rs`'s other test-only helpers, so a `mod` share would
//! only add dead-code warnings there. The deletion mechanism itself
//! (`RETIRED`) is exercised at the unit level in `install.rs`'s own tests,
//! which inject their own list directly at the `delete_retired` call site;
//! these CLI-level tests cover the two production shapes instead: no
//! deletion when the retired paths are absent (a fresh, post-T5 `init`
//! never seeds them), and `update_deletes_retired_shell_tools_from_an_old_
//! install` below, `RETIRED`'s first production use, batch 18 T5.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
/// already-`init`ed install. Measured live on both engines for this task
/// (captures retained at
/// `.superpowers/sdd/2026-09-05-batch-18/t4-evidence/fr3-bin-never-init.out`
/// and `fr3-js-never-init.out`), the binary does not crash on a target
/// that was never `init`ed.
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
/// -- NOT a reproduced crash (`houserules.crash-paths-are-named`) --
/// measured live on both engines for this task (`seeded_repo`'s own doc
/// has the full account and the retained captures). `update` still writes
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
/// A hardcoded Unix message broke this on Windows (batch-18 PR #6,
/// windows-latest, run 34056836063/job 101550164150): the real failure was
/// `...\knowledge\schema.json: The system cannot find the path specified.
/// (os error 3)`, not the `.../knowledge/schema.json: No such file or
/// directory (os error 2)` the old hardcoded string assumed -- two
/// independent divergences at once. The path separator differs because
/// `PathBuf::join` inserts the platform's own `MAIN_SEPARATOR` (`\` on
/// Windows, `/` on Unix) between components it joins itself, but a
/// separator typed literally inside one string argument (the old
/// `.join("knowledge/schema.json")`) is never normalized -- this test now
/// joins `"knowledge"` and `"schema.json"` as two separate calls, matching
/// production's own two `.join()` calls byte-for-byte. The message and
/// code differ because a missing PARENT directory is a different Windows
/// error than a missing leaf file: `ERROR_PATH_NOT_FOUND` (3, "The system
/// cannot find the path specified.") applies here since `knowledge/`
/// itself does not exist, where `ERROR_FILE_NOT_FOUND` (2, "The system
/// cannot find the file specified.") would apply if only `schema.json`
/// were missing (Microsoft's own System Error Codes reference, WinError.h,
/// entries 2 and 3). Deriving the path and the error text from a real
/// syscall on this platform, this run, keeps the assertion exact
/// everywhere with zero `cfg`.
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
/// left this list at batch 20 T3 (HR-047, docs/specs/2026-09-07-batch-20-
/// phase5.md §2), joining `RETIRED`.
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

/// `env!("CARGO_PKG_VERSION")` at THIS test binary's own compile time --
/// `install.rs`'s own copy of `kit_version` (that function's own doc has
/// the account of batch 20 T3's retirement of the earlier package.json-
/// reading form).
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[test]
fn update_syncs_every_kit_owned_file_reports_the_drift_line_and_leaves_adopter_files_alone() {
    let dir = seeded_repo();
    // Corrupt a KIT_OWNED file and an unrelated adopter file the same way,
    // so the assertions below can tell "update overwrote this" from "update
    // never touched this" by content alone. `.claude/agents/implementer.md`,
    // not `tools/kb.mjs`: batch 20 T3 (HR-047) moved the four JS engines
    // from `KIT_OWNED` to `RETIRED`, so a fresh `seeded_repo()` no longer
    // carries `tools/kb.mjs` at all -- an old install still holding one is
    // its own, separate scenario, covered by `update_deletes_retired_
    // js_engines_from_an_old_install` below.
    fs::write(
        dir.path().join(".claude/agents/implementer.md"),
        b"corrupted",
    )
    .expect("corrupt .claude/agents/implementer.md");
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

    let restored = fs::read(dir.path().join(".claude/agents/implementer.md"))
        .expect("read .claude/agents/implementer.md after update");
    let template = fs::read(repo_root().join("template/.claude/agents/implementer.md"))
        .expect("read template");
    assert_eq!(
        restored, template,
        ".claude/agents/implementer.md was not resynced"
    );
    assert_eq!(
        fs::read(dir.path().join("adopter-notes.md")).expect("read adopter file"),
        b"mine",
        "update touched an adopter-owned file"
    );
}

/// Batch 20 T3 (HR-047, docs/specs/2026-09-07-batch-20-phase5.md §2): the
/// four JS engines the shell wrappers used to front (`update_deletes_
/// retired_shell_tools_from_an_old_install`'s own doc explains why
/// `tools/kb.sh`/`tools/backlog.sh` get their own, separate test) are
/// `RETIRED` too now -- an install seeded before this batch still carries
/// them, and `update` deletes all four in the same run.
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

/// A post-T5 `init` never seeds `RETIRED`'s two paths (they left
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

/// Batch 18 T5: `RETIRED`'s first production use. An install seeded before
/// this task (or by the still-frozen `node bin/houserules.mjs init`, which
/// still writes both) carries `tools/kb.sh` and `tools/backlog.sh`; the
/// next `update` deletes both, reports each `removed <path>` in `RETIRED`'s
/// own call order, and still resyncs every current `KIT_OWNED` file
/// alongside them in the same run.
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
    assert_eq!(
        marker,
        serde_json::json!({"version": version, "idPrefix": "WI"})
    );
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
    assert_eq!(
        marker,
        serde_json::json!({"version": version, "idPrefix": "FOO"})
    );
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
    assert_eq!(
        marker,
        serde_json::json!({"version": version, "idPrefix": "ZED"})
    );
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

/// Every invalid `.houserules.json` shape below was measured against the
/// real `node bin/houserules.mjs update`, not assumed from `init`'s own
/// account, per this task's brief. This task's `live_run` entries hold all
/// ten runs (JS and the binary, over each of the five shapes below);
/// `install.rs`'s own "Failure paths" doc section cites that coverage.
/// All five print one named stderr line (`<marker path>: <suffix>`), exit
/// 2, with no `wrote <file>` line ahead of it on either engine. Seeds its
/// own scratch repo so the expected message's marker path and the actual
/// one always agree.
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
