//! `update` CLI-level tests. Follows `install.rs`'s own structural pattern
//! (a real subprocess against a real scratch git repository) for the same
//! reason that file gives (`check_commit.rs`'s module doc). The shared
//! helpers `repo_root`, `scratch_git_repo`, and `sha256_hex` come from
//! `tracker_support`, whose `dead_code` allowance covers the helpers this
//! file does not call. `houserules()` stays beside its callers, as the doc
//! of that helper in `tracker_support` says every `tests/*.rs` file keeps
//! it. The deletion mechanism itself (`RETIRED`) is
//! exercised at the unit level in `install/mod.rs`'s own tests, which
//! inject their own list directly at the `delete_retired` call site; these
//! CLI-level tests cover the two production shapes instead: no deletion
//! when the retired paths are absent (a fresh `init` never seeds them), and
//! `update_deletes_retired_shell_tools_from_an_old_install` below,
//! `RETIRED`'s production use against an old install that still has them.

mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use tracker_support::{quoted, repo_root, resolved, scratch_git_repo, sha256_hex};

/// A `Command` for the compiled `houserules` binary under test --
/// `install.rs`'s own copy of this helper. Sets
/// `HOUSERULES_SKIP_SELF_UPDATE` so `update`'s new self-update phase
/// (`selfupdate.rs`) never runs here: its own network call belongs
/// nowhere in `cargo test` (security-hygiene), and every `assert_eq!`
/// against this file's exact stderr predates that phase and would break
/// under its extra line the moment a real install receipt is present.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// A scratch repository that a fresh `houserules init --target claude`
/// installed (`init_seeded_repo`): `update`'s own tests start from this unless
/// they say otherwise, since `update` presupposes an already-`init`ed
/// install. The binary does not crash on a target that was never `init`ed.
/// `update_over_a_never_init_ed_target_is_a_named_error_exit_2` below pins
/// it as one named stderr line naming `<target>/knowledge/schema.json`, and
/// exit 2 -- the exact `io::Error` text and path separator are
/// platform-specific (that test's own doc has the measured Unix/Windows
/// split), so the pin derives them rather than hardcoding either. `node
/// bin/houserules.mjs update` dumps a 26-line stack trace for the same
/// missing file instead. `install/mod.rs`'s own "Failure paths" doc section
/// carries the same account, since a target repository's `update` failure
/// modes belong there too. The conversion of a 1.x install has its own tests
/// in `conversion.rs`.
fn seeded_repo() -> tempfile::TempDir {
    init_seeded_repo()
}

/// The install of a fresh `houserules init --target claude`: a 2.0 install,
/// with every kit-owned file.
fn init_seeded_repo() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
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
/// `seeded_repo`'s own doc has the full account. The directory holds no
/// `.houserules.json` and no `knowledge/`, at or above it, so `update` prints
/// the line for a tree with no install (HR-216), whose remedy names the target
/// that `--dir` gave, before it writes any file.
#[test]
fn update_over_a_never_init_ed_target_is_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        format!(
            "no houserules install here (no .houserules.json); run houserules init --dir {}\n",
            quoted(dir.path())
        )
    );
    assert!(
        !dir.path().join("knowledge").exists(),
        "update never seeds knowledge/"
    );
}

/// Every `KIT_OWNED` path -- `install/mod.rs`'s own copy. `tools/kb.mjs`,
/// `tools/backlog.mjs`, `tools/lib/cli.mjs`, and `tools/lib/json-store.mjs`
/// are not in this list -- they joined `RETIRED` instead.
const KIT_OWNED: &[&str] = &[
    "tools/claude-session-start.sh",
    ".claude/skills/migrating-knowledge/SKILL.md",
    ".claude/skills/houserules-step/SKILL.md",
    "tools/houserules-step.sh",
];

/// Every `SEED_ONCE` path -- `install/mod.rs`'s own copy.
const SEED_ONCE: &[&str] = &[
    "knowledge/schema.json",
    "knowledge/areas.json",
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
    "docs/README.md",
    "AGENTS.md",
    "CLAUDE.md",
];

/// `env!("CARGO_PKG_VERSION")` at THIS test binary's own compile time --
/// `install/mod.rs`'s own copy of `kit_version` (that function's own doc has
/// the account).
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// A file whose bytes would not change is neither written nor listed, so
/// the first `update` right after `init` prints no `wrote` line, and an
/// adopter file stays as it is.
#[test]
fn update_right_after_init_writes_and_lists_no_file_and_leaves_adopter_files_alone() {
    let dir = init_seeded_repo();
    fs::write(dir.path().join("adopter-notes.md"), b"mine").expect("write adopter file");

    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "stderr: {stderr}");

    let mut expected_lines: Vec<String> = vec!["render: up to date".to_string()];
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
        dir.path().join(".claude/skills/houserules-step/SKILL.md"),
        b"corrupted",
    )
    .expect("corrupt .claude/skills/houserules-step/SKILL.md");

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
        stdout.contains("kept .claude/skills/houserules-step/SKILL.md (locally modified)\n"),
        "got:\n{stdout}"
    );
    assert!(
        !stdout.contains("wrote .claude/skills/houserules-step/SKILL.md\n"),
        "got:\n{stdout}"
    );

    assert_eq!(
        fs::read(dir.path().join(".claude/skills/houserules-step/SKILL.md")).unwrap(),
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
    let file = ".claude/skills/houserules-step/SKILL.md";
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

/// A `KIT_OWNED` file already holding the running kit's exact content, but
/// with a recorded baseline stamped to something else entirely, is neither
/// kept-and-reported nor left stale: `update` restamps its baseline to the
/// running content's hash, and neither writes nor lists the file, since its
/// bytes would not change; it is never `kept ... (locally modified)`. This is
/// the dogfood shape a ruled wording change produces when a project's own
/// copy is hand-synced to a new kit release without an intervening
/// `update` restamping first.
#[test]
fn update_restamps_a_kit_owned_file_already_at_the_running_content_despite_a_stale_baseline() {
    let dir = seeded_repo();
    let file = ".claude/skills/houserules-step/SKILL.md";
    let running_content = fs::read(repo_root().join("template").join(file)).unwrap();
    set_marker_baseline(
        dir.path(),
        file,
        &sha256_hex(b"content from an older kit release\n"),
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
    assert!(
        !stdout.contains(&format!("kept {file} (locally modified)\n")),
        "got:\n{stdout}"
    );
    assert!(!stdout.contains(file), "got:\n{stdout}");

    assert_eq!(
        fs::read(dir.path().join(file)).unwrap(),
        running_content,
        "update changed content that already matched the running kit"
    );
    let marker_path = dir.path().join(".houserules.json");
    let restamped: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    assert_eq!(
        restamped["baselines"][file],
        serde_json::json!(sha256_hex(&running_content)),
        "the stale baseline was never restamped to the running content's hash"
    );
}

/// A path in `ownedPaths` silences reporting for a locally modified `KIT_OWNED` file
/// entirely: no report line, and the adopter's own content is kept exactly
/// as a plain modified file's would be, but with nothing printed about it.
#[test]
fn update_silences_a_locally_modified_kit_owned_file_listed_in_owned_paths() {
    let dir = seeded_repo();
    let file = ".claude/skills/houserules-step/SKILL.md";
    fs::write(dir.path().join(file), b"adopter owns this now").expect("corrupt file");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["ownedPaths"] = serde_json::json!([file]);
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
        restamped["ownedPaths"],
        serde_json::json!([file]),
        "update did not preserve the ownedPaths field"
    );
}

/// A `KIT_OWNED` file the adopter deleted outright, with no override, is
/// kit machinery the adopter has not claimed: `update` restores it, the
/// same outcome an at-baseline file gets, rather than merely reporting it
/// gone.
#[test]
fn update_restores_a_deleted_kit_owned_file() {
    let dir = seeded_repo();
    let file = ".claude/skills/houserules-step/SKILL.md";
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
    let file = "tools/houserules-step.sh";
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

/// A path in `ownedPaths` on a deleted `KIT_OWNED` file is the adopter's own
/// declaration that they deleted it on purpose: `update` leaves it absent
/// and reports nothing.
#[test]
fn update_leaves_an_owned_deleted_kit_owned_file_absent() {
    let dir = seeded_repo();
    let file = ".claude/skills/houserules-step/SKILL.md";
    fs::remove_file(dir.path().join(file)).expect("delete file");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["ownedPaths"] = serde_json::json!([file]);
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

/// Reads `dir`'s `.houserules.json`, applies `edit`, and writes it back in
/// emit form.
fn edit_marker(dir: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = dir.join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read the marker"))
            .expect("parse the marker");
    edit(&mut marker);
    fs::write(
        &path,
        format!("{}\n", serde_json::to_string_pretty(&marker).unwrap()),
    )
    .expect("write the marker");
}

/// Runs `update --dir <dir>`, requires exit 0, and returns the stdout lines.
fn update_lines(dir: &Path) -> Vec<String> {
    let output = houserules()
        .args(["update", "--dir"])
        .arg(dir)
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8 stdout")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Marks `dir` as a 2.0 install: the `schema` key `init` writes.
fn mark_as_2_0(marker: &mut serde_json::Value) {
    marker["schema"] = serde_json::json!("houserules/install@2.0.0");
}

/// A second `update` changes nothing and prints no `wrote` line.
#[test]
fn a_second_update_prints_no_wrote_line() {
    let dir = init_seeded_repo();
    update_lines(dir.path());
    let second = update_lines(dir.path());
    let wrote: Vec<&String> = second
        .iter()
        .filter(|line| line.starts_with("wrote "))
        .collect();
    assert!(wrote.is_empty(), "got: {second:?}");
}

/// A file in `ownedPaths` of a 2.0 install is never rewritten, present or
/// absent, and `update` prints no line about it. A modified file that is not
/// listed is kept and listed, as before.
#[test]
fn update_never_rewrites_a_file_in_owned_paths_of_a_2_0_install() {
    let dir = seeded_repo();
    let modified = "tools/houserules-step.sh";
    let deleted = ".claude/skills/migrating-knowledge/SKILL.md";
    let unlisted = ".claude/skills/houserules-step/SKILL.md";
    fs::write(dir.path().join(modified), b"the adopter's hook").expect("write the hook");
    fs::remove_file(dir.path().join(deleted)).expect("delete the skill");
    fs::write(dir.path().join(unlisted), b"edited").expect("write the agent");
    edit_marker(dir.path(), |marker| {
        mark_as_2_0(marker);
        marker["ownedPaths"] = serde_json::json!([modified, deleted]);
    });

    let lines = update_lines(dir.path());

    assert_eq!(
        fs::read(dir.path().join(modified)).unwrap(),
        b"the adopter's hook"
    );
    assert!(!dir.path().join(deleted).exists());
    assert!(
        !lines.iter().any(|line| line.contains(modified)),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains(deleted)),
        "{lines:?}"
    );
    assert!(
        lines.contains(&format!("kept {unlisted} (locally modified)")),
        "{lines:?}"
    );
}

/// The Claude Code files `init` leaves out without `--target claude`.
const CLAUDE_FILES: [&str; 1] = ["tools/claude-session-start.sh"];

/// While `renderers` lacks `claude`, the sync writes none of the Claude
/// files; a missing `renderers` key reads as `["claude"]`, the value the
/// 1.x conversion writes, so the sync restores them.
#[test]
fn update_writes_the_claude_files_only_while_renderers_holds_claude() {
    let cases: [(Option<serde_json::Value>, bool); 4] = [
        (Some(serde_json::json!([])), false),
        (Some(serde_json::json!(["other"])), false),
        (Some(serde_json::json!(["claude"])), true),
        (None, true),
    ];
    for (renderers, restored) in cases {
        let dir = seeded_repo();
        for file in CLAUDE_FILES {
            fs::remove_file(dir.path().join(file)).expect("delete a Claude file");
        }
        edit_marker(dir.path(), |marker| {
            mark_as_2_0(marker);
            if let Some(renderers) = &renderers {
                marker["renderers"] = renderers.clone();
            }
        });

        let lines = update_lines(dir.path());

        for file in CLAUDE_FILES {
            assert_eq!(
                dir.path().join(file).exists(),
                restored,
                "{file} with renderers {renderers:?}"
            );
            assert_eq!(
                lines.contains(&format!("wrote {file}")),
                restored,
                "{file} with renderers {renderers:?}: {lines:?}"
            );
        }
        assert!(
            dir.path().join("tools/houserules-step.sh").is_file(),
            "the sync keeps every other kit-owned file"
        );
    }
}

/// The path of the seeded CI workflow and its baseline key (spec section 8:
/// git's top-level pathspec form).
const WORKFLOW: &str = ".github/workflows/knowledge.yml";
const WORKFLOW_KEY: &str = ":/.github/workflows/knowledge.yml";

/// The template's content of `file`.
fn template_bytes(file: &str) -> Vec<u8> {
    fs::read(repo_root().join("template").join(file)).expect("read the template file")
}

/// What an older release wrote to `file`: the template with one more blank
/// line at its end. The bytes differ, and a schema stays a valid schema.
fn older_release_bytes(file: &str) -> Vec<u8> {
    let current = String::from_utf8(template_bytes(file)).expect("the template is UTF-8");
    format!("{current}\n").into_bytes()
}

/// One scenario of the D3 sync: a seeded install marked as 2.0, `file`
/// holding the older release's content, `baseline` recorded under `key`
/// when it is `Some`, and `file` listed in `ownedPaths` when `owned`. Runs
/// `update` and returns its lines and the content of `file` after it.
fn d3_scenario(
    file: &str,
    key: &str,
    baseline: Option<&[u8]>,
    owned: bool,
) -> (Vec<String>, Vec<u8>) {
    let dir = seeded_repo();
    let path = dir.path().join(file);
    fs::create_dir_all(path.parent().unwrap()).expect("create the directory");
    fs::write(&path, older_release_bytes(file)).expect("write the older content");
    edit_marker(dir.path(), |marker| {
        mark_as_2_0(marker);
        match baseline {
            Some(baseline) => marker["baselines"][key] = serde_json::json!(sha256_hex(baseline)),
            None => {
                marker["baselines"]
                    .as_object_mut()
                    .expect("baselines")
                    .remove(key);
            }
        }
        if owned {
            marker["ownedPaths"] = serde_json::json!([file]);
        }
    });
    let lines = update_lines(dir.path());
    (lines, fs::read(&path).expect("read the file back"))
}

/// `knowledge/schema.json` and the seeded workflow are covered by the sync
/// once they are baselined, and not before. At its baseline the file is
/// written with the running kit's content; with no baseline it is left as
/// found, with no line; with a baseline it no longer matches it is kept and
/// listed; in `ownedPaths` it is never written.
#[test]
fn update_syncs_the_schema_and_the_workflow_once_they_are_baselined() {
    for (file, key) in [
        ("knowledge/schema.json", "knowledge/schema.json"),
        (WORKFLOW, WORKFLOW_KEY),
    ] {
        let older = older_release_bytes(file);
        let wrote = format!("wrote {file}");
        let kept = format!("kept {file} (locally modified)");

        let (lines, after) = d3_scenario(file, key, Some(&older), false);
        assert!(lines.contains(&wrote), "{file} at its baseline: {lines:?}");
        assert_eq!(after, template_bytes(file), "{file} at its baseline");

        let (lines, after) = d3_scenario(file, key, None, false);
        assert!(
            !lines.iter().any(|line| line.contains(file)),
            "{file} with no baseline: {lines:?}"
        );
        assert_eq!(after, older, "{file} with no baseline");

        let (lines, after) = d3_scenario(file, key, Some(b"what the kit once wrote"), false);
        assert!(lines.contains(&kept), "{file} modified: {lines:?}");
        assert_eq!(after, older, "{file} modified");

        let (lines, after) = d3_scenario(file, key, Some(&older), true);
        assert!(
            !lines.iter().any(|line| line.contains(file)),
            "{file} owned: {lines:?}"
        );
        assert_eq!(after, older, "{file} owned");
    }
}

/// The baseline of a D3 file that the sync writes is the hash of the
/// content it wrote.
#[test]
fn update_restamps_the_baseline_of_a_d3_file_it_writes() {
    let dir = seeded_repo();
    let older = older_release_bytes(WORKFLOW);
    fs::create_dir_all(dir.path().join(".github/workflows")).expect("create the directory");
    fs::write(dir.path().join(WORKFLOW), &older).expect("write the older content");
    edit_marker(dir.path(), |marker| {
        mark_as_2_0(marker);
        marker["baselines"][WORKFLOW_KEY] = serde_json::json!(sha256_hex(&older));
    });

    update_lines(dir.path());

    let marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join(".houserules.json")).unwrap())
            .unwrap();
    assert_eq!(
        marker["baselines"][WORKFLOW_KEY],
        serde_json::json!(sha256_hex(&template_bytes(WORKFLOW)))
    );
}

/// A baselined workflow that the project deleted stays absent (spec 10.7):
/// `update` writes none and prints nothing about it. Another D3 file that is
/// missing is restored, like a kit-owned file.
#[test]
fn update_does_not_restore_a_deleted_workflow() {
    let dir = seeded_repo();
    edit_marker(dir.path(), |marker| {
        mark_as_2_0(marker);
        marker["baselines"][WORKFLOW_KEY] = serde_json::json!(sha256_hex(b"once written"));
    });

    let lines = update_lines(dir.path());

    assert!(
        !lines.iter().any(|line| line.contains(WORKFLOW)),
        "{lines:?}"
    );
    assert!(!dir.path().join(WORKFLOW).exists());
}

/// A kit-owned script whose bytes already match keeps its executable bit
/// without a line: `update` repairs the mode and still lists nothing.
#[cfg(unix)]
#[test]
fn update_repairs_a_lost_executable_bit_without_a_line() {
    use std::os::unix::fs::PermissionsExt;

    let dir = seeded_repo();
    let hook = dir.path().join("tools/houserules-step.sh");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o644)).expect("chmod the hook");

    let lines = update_lines(dir.path());

    let mode = fs::metadata(&hook).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("tools/houserules-step.sh")),
        "{lines:?}"
    );
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

/// `AGENTS.md` gets the identical backfill treatment as any other missing
/// `SEED_ONCE` path: absent with no override means "never arrived", not
/// "deleted on purpose".
#[test]
fn update_backfills_a_missing_agents_md_file() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join("AGENTS.md")).expect("remove AGENTS.md");

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
    assert!(stdout.contains("wrote AGENTS.md\n"), "got:\n{stdout}");

    let backfilled = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    let template = fs::read_to_string(repo_root().join("template/AGENTS.md")).unwrap();
    let block = backfilled
        .strip_prefix(&template)
        .expect("the text of the seed comes first, byte for byte");
    assert!(
        block.starts_with("<!-- houserules:begin -->\n"),
        "the render of update appends the block: {block:?}"
    );
    assert!(block.ends_with("\n<!-- houserules:end -->\n"), "{block:?}");
    assert!(
        stdout.contains("AGENTS.md: written\n"),
        "the render of update reports the block, got:\n{stdout}"
    );
}

/// A present `AGENTS.md` is `SEED_ONCE`: `init` seeds it, and `update`
/// never rewrites its text once it exists, exactly like `CLAUDE.md`. The
/// render that ends `update` appends the houserules block to a file with no
/// markers and leaves the project's text byte for byte.
#[test]
fn update_keeps_the_project_text_of_an_existing_agents_md_file() {
    let dir = seeded_repo();
    assert!(
        dir.path().join("AGENTS.md").is_file(),
        "init did not seed AGENTS.md"
    );
    fs::write(dir.path().join("AGENTS.md"), b"project notes").expect("edit AGENTS.md");

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
        !stdout.contains("wrote AGENTS.md") && !stdout.contains("kept AGENTS.md"),
        "the sync reported AGENTS.md in:\n{stdout}"
    );
    assert!(
        stdout.contains("AGENTS.md: written\n"),
        "the render appends the block, got:\n{stdout}"
    );
    let written = fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
    assert!(
        written.starts_with("project notes\n<!-- houserules:begin -->\n"),
        "got {written:?}"
    );
}

/// `seeded_repo`'s own `init` wrote no CI workflow; a plain `update` leaves it
/// missing and silent, the same convention `RETIRED`'s absence gets.
#[test]
fn update_silently_leaves_the_workflow_absent_without_a_github_origin() {
    let dir = seeded_repo();
    assert!(!dir.path().join(WORKFLOW).exists(), "{WORKFLOW} was seeded");

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
        !stdout.contains(WORKFLOW),
        "update reported {WORKFLOW} in:\n{stdout}"
    );
    assert!(
        !dir.path().join(WORKFLOW).exists(),
        "{WORKFLOW} was backfilled"
    );
}

/// A target that becomes GitHub-hosted after its original `init` (an
/// `origin` remote added later) gets no workflow from the next `update`: it
/// writes no CI file (spec 10.7).
#[test]
fn update_writes_no_workflow_once_the_origin_becomes_github_hosted() {
    let dir = seeded_repo();
    assert!(!dir.path().join(WORKFLOW).exists(), "{WORKFLOW} was seeded");
    let status = Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/jblossey/houserules.git",
        ])
        .current_dir(dir.path())
        .status()
        .expect("run git remote add");
    assert!(status.success(), "git remote add failed");

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
        !stdout.contains(WORKFLOW),
        "update reported {WORKFLOW} in:\n{stdout}"
    );
    assert!(
        !dir.path().join(WORKFLOW).exists(),
        "{WORKFLOW} was written"
    );
}

/// A path in `ownedPaths` on the still-absent workflow means "deleted on
/// purpose" even once the origin becomes GitHub-hosted: `update` leaves it
/// absent and prints nothing about it, the same as any other overridden path.
#[test]
fn update_does_not_write_an_owned_workflow() {
    let dir = seeded_repo();
    let status = Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/jblossey/houserules.git",
        ])
        .current_dir(dir.path())
        .status()
        .expect("run git remote add");
    assert!(status.success(), "git remote add failed");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["ownedPaths"] = serde_json::json!([WORKFLOW]);
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
        !stdout.contains(WORKFLOW),
        "update reported {WORKFLOW} in:\n{stdout}"
    );
    assert!(!dir.path().join(WORKFLOW).exists());
}

/// A path in `ownedPaths` on a missing `SEED_ONCE` path means "deleted on purpose":
/// `update` leaves it absent and prints nothing about it.
#[test]
fn update_does_not_backfill_an_owned_seed_once_file() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join("docs/README.md")).expect("remove docs/README.md");

    let marker_path = dir.path().join(".houserules.json");
    let mut marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).unwrap()).unwrap();
    marker["ownedPaths"] = serde_json::json!(["docs/README.md"]);
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

/// The backfill of the JSON tracker's data files rewrites `PREFIXED` payload
/// content with the install's own stamped `idPrefix`, never the `--id-prefix`
/// flag's value (which defaults to `WI`): backfilling into a non-`WI` install
/// must not hand it ids `check-backlog` then rejects.
#[test]
fn update_backfills_a_prefixed_file_with_the_installs_own_stamped_prefix() {
    let dir = scratch_git_repo();
    let init = houserules()
        .args(["init", "--tracker", "json", "--dir"])
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
/// file: `.claude/skills/migrating-knowledge/SKILL.md`, left exactly as `seeded_repo`
/// wrote it, matches the payload and is stamped, neither written nor listed
/// since its bytes would not change (`Status::AtBaseline`);
/// `.claude/skills/houserules-step/SKILL.md`, hand-corrupted
/// below, diverges from the payload and is kept, reported once, and left
/// unstamped for a later run to reconcile (`Status::Modified`). Nothing on
/// disk is overwritten by the divergent file; both outcomes coexist in one
/// `update` invocation, since neither file has a recorded baseline yet.
#[test]
fn update_migration_run_reconciles_kit_owned_files_with_no_baselines_recorded() {
    let dir = seeded_repo();
    let unmodified_file = ".claude/skills/migrating-knowledge/SKILL.md";
    let modified_file = ".claude/skills/houserules-step/SKILL.md";
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
    assert!(!stdout.contains(unmodified_file), "got:\n{stdout}");
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
/// neither present and reports no deletion (`install/mod.rs`'s own "Deletion"
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

/// Asserts `stderr` is the refusal of a directory below the git top level
/// that holds no install, naming `target` and `top_level` in resolved form
/// (every symlink followed, built one component at a time). This file keeps
/// its own helpers because `mod common;` would make that module's other
/// helpers warn as dead code in this binary (`check_commit.rs`'s own
/// `houserules` helper doc has the account).
fn assert_subdirectory_refusal(stderr: &str, target: &Path, top_level: &Path) {
    let root = resolved(target);
    let top = resolved(top_level);
    assert_eq!(
        stderr,
        format!(
            "{}: this directory is inside the git repository at {} and holds no .houserules.json; run houserules init --dir {} to install there\n",
            root.display(),
            top.display(),
            quoted(&root)
        )
    );
}

/// A target inside a repository but not at its top level, with no install,
/// is not an install root: `update` refuses it and names the enclosing top level.
#[test]
fn update_refuses_a_subdirectory_that_holds_no_install() {
    let repo = seeded_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let output = houserules()
        .args(["update", "--dir"])
        .arg(&sub)
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_subdirectory_refusal(
        &String::from_utf8(output.stderr).expect("utf8 stderr"),
        &sub,
        repo.path(),
    );
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(&sub).unwrap().count(), 0);
}

/// With `GIT_WORK_TREE` at a directory that does not contain the target,
/// git answers with that directory as the top level (`GIT_WORK_TREE=<other>
/// git rev-parse --show-toplevel` run in `<repo>/sub` prints `<other>`); the
/// refusal still names the directory above the target that holds `.git`.
#[test]
fn update_names_the_ancestor_that_holds_git_under_a_foreign_git_work_tree() {
    let repo = seeded_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let foreign = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["update", "--dir"])
        .arg(&sub)
        .env("GIT_WORK_TREE", foreign.path())
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_subdirectory_refusal(
        &String::from_utf8(output.stderr).expect("utf8 stderr"),
        &sub,
        repo.path(),
    );
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(&sub).unwrap().count(), 0);
}

/// A symlink to a repository subdirectory is inside the repository; the
/// refusal names the repository root, in resolved form.
#[cfg(unix)]
#[test]
fn update_names_the_repository_for_a_symlinked_subdirectory_target() {
    let repo = seeded_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let links = tempfile::tempdir().expect("tempdir");
    let link = links.path().join("link");
    std::os::unix::fs::symlink(&sub, &link).expect("create symlink");
    let output = houserules()
        .args(["update", "--dir"])
        .arg(&link)
        .output()
        .expect("run update");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_subdirectory_refusal(&stderr, &link, repo.path());
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(&sub).unwrap().count(), 0);
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
/// (`install/mod.rs`'s "update" doc section names this measured distinction
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

/// `ownedPaths` and `renderers` are hand-edited data of a 2.0 install: a
/// present value that is not an array of strings is one named error, as
/// `overrides` is. The `schema` key is one string.
#[test]
fn update_reports_a_malformed_2_0_key_as_a_named_error_exit_2() {
    assert_named_error_before_any_write(
        br#"{"ownedPaths":"tools/houserules-step.sh"}"#,
        "ownedPaths must be an array of strings\n",
    );
    assert_named_error_before_any_write(
        br#"{"ownedPaths":["tools/houserules-step.sh", 5]}"#,
        "ownedPaths must be an array of strings\n",
    );
    assert_named_error_before_any_write(
        br#"{"renderers":"claude"}"#,
        "renderers must be an array of strings\n",
    );
    assert_named_error_before_any_write(
        br#"{"renderers":["claude", 5]}"#,
        "renderers must be an array of strings\n",
    );
    assert_named_error_before_any_write(br#"{"schema":2}"#, "schema must be a string\n");
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

/// `HOUSERULES_SKIP_SELF_UPDATE` silences the self-update phase end to
/// end through the real binary, independently of `houserules()`'s own
/// default above -- this test builds its bare `Command` so it proves the
/// running binary reads this exact environment variable name itself,
/// never only the pure `selfupdate::phase_disabled` unit already covers.
/// No install receipt exists on a test-running machine by construction
/// (no test in this suite, or any other, ever runs the real shell
/// installer), so an unguarded phase would print its one no-receipt
/// guidance line first; this asserts that line's absence.
#[test]
fn update_skips_the_self_update_phase_silently_when_the_skip_env_var_is_set() {
    let dir = seeded_repo();
    let output = Command::new(env!("CARGO_BIN_EXE_houserules"))
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        !stderr.contains("houserules: no shell-installer receipt found"),
        "self-update phase ran despite the skip env var: {stderr:?}"
    );
}
