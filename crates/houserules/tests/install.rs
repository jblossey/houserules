//! `init` and `files` CLI-level tests (HR-047 phase-3 slice, batch 18 T3,
//! docs/specs/2026-09-05-batch-18-phase3.md §§1-2): the payload now lives
//! inside the binary (`rust-embed`, `debug-embed` -- `install.rs`'s own
//! module doc has the vetting and configuration account), so these tests
//! run the compiled binary as a real subprocess against real scratch git
//! repositories, following `check_commit.rs`'s structural pattern (its own
//! doc explains why each `tests/*.rs` file keeps its own small copy of
//! these helpers rather than sharing `mod common;`). `update` is T4's; no
//! test here exercises it. The JS-vs-Rust byte-identity diff and the
//! release-build embed spot check are live-run proofs
//! (`.superpowers/sdd/2026-09-05-batch-18/t3-evidence/`), not `cargo test`
//! cases: they need a real `node` invocation and a real `--release`
//! rebuild respectively, neither of which belongs in this suite.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A `Command` for the compiled `houserules` binary under test.
fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// This checkout's repository root, resolved at compile time so it is
/// correct regardless of the test runner's working directory --
/// `tests/common/mod.rs::repo_root`'s own copy, duplicated here rather than
/// pulled in via `mod common;` for the same reason `check_commit.rs` gives
/// (its own module doc): this file needs none of that module's other
/// helpers, and importing it whole would warn the rest of it as dead code.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// A fresh scratch directory with `git init` already run -- the minimal
/// fixture `install`'s own `.git`-existence check accepts. `tempfile`'s
/// `TempDir` removes itself on drop (`houserules.tests-clean-scratch-dirs`'s
/// sanctioned Rust form: a `Drop`-owning guard, minted at the same site
/// that creates it).
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

/// Every `KIT_OWNED` path -- `install.rs`'s own copy, which this file
/// cannot import (a bare CLI binary crate, no `pub` library surface):
/// `tools/kb.mjs`, `tools/backlog.mjs`, `tools/lib/cli.mjs`, and
/// `tools/lib/json-store.mjs` left this list at batch 20 T3 (HR-047,
/// docs/specs/2026-09-07-batch-20-phase5.md §2), joining `RETIRED`.
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

/// Every `SEED_ONCE` path, `bin/houserules.mjs`'s own array, same
/// live-verified source as `KIT_OWNED`.
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
    "CLAUDE.md",
];

/// `env!("CARGO_PKG_VERSION")` at THIS test binary's own compile time --
/// the same value `install::kit_version` bakes in for the binary under
/// test (that function's own doc has the account: batch 20 T3, HR-047,
/// retired the earlier package.json-reading form both copies used).
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[test]
fn files_prints_the_kit_owned_and_seed_once_lists_as_json() {
    let output = houserules().arg("files").output().expect("run files");
    assert!(output.status.success());
    let expected = serde_json::json!({"kitOwned": KIT_OWNED, "seedOnce": SEED_ONCE});
    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("files prints JSON");
    assert_eq!(actual, expected);
    // Byte-exact against `emit`'s own format (two-space indent, trailing
    // newline), not just structurally equal -- `houserules files` has no
    // `--dir` and no filesystem input, so its output is deterministic
    // enough to pin verbatim.
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
    );
}

#[test]
fn init_bare_dir_flag_exits_2_with_claps_value_required_message() {
    // `argv_closure.rs`'s own class, pinned here rather than added to that
    // file's `COMMANDS` list: unlike every command that list covers, `init`'s
    // `--dir` is a real, load-bearing option in the frozen JS too (`files`'s
    // sibling test above pins the one JS-parity-relevant `--dir` divergence
    // this command has), so its argv edges belong beside its own behavior.
    // Fix round 1, issue 2: measured live, a bare trailing `--dir` (nothing
    // follows it) never reaches `tools/lib/cli.mjs`'s `parseArgs` as a
    // flag at all -- `opts.dir` stays `undefined`, so `node
    // bin/houserules.mjs init --dir` from a scratch git repo exits 0 and
    // seeds that repo, the CURRENT directory, exactly as a bare `init`
    // would. clap's own stricter "a value is required" refusal is a ruled
    // divergence (the batch's argv-closure ruling), not an unnoticed one.
    let output = houserules()
        .args(["init", "--dir"])
        .output()
        .expect("run init --dir");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: a value is required for '--dir <DIR>' but none was supplied"),
        "got {stderr:?}"
    );
}

#[test]
fn init_duplicated_dir_flag_exits_2_with_claps_cannot_be_used_multiple_times_message() {
    // Fix round 1, issue 2: measured live, `node bin/houserules.mjs init
    // --dir <a> --dir <b>` exits 0 and seeds <b> only -- `parseArgs`
    // consumes each `--dir` occurrence in order, the second overwriting
    // the first in `opts.dir`, so the last one silently wins. clap's own
    // "cannot be used multiple times" refusal is the same ruled divergence
    // the bare-flag test above names, not an unnoticed one.
    let output = houserules()
        .args(["init", "--dir", "a", "--dir", "b"])
        .output()
        .expect("run init --dir a --dir b");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--dir <DIR>' cannot be used multiple times"),
        "got {stderr:?}"
    );
}

#[test]
fn files_takes_no_dir_flag() {
    // `bin/houserules.mjs`'s own `files` arm reads no `--dir` at all
    // (`main`'s `case 'files'` never touches `opts.dir`); this binary's
    // `Files` variant carries no such field, so clap itself rejects one.
    let output = houserules()
        .args(["files", "--dir", "x"])
        .output()
        .expect("run files --dir x");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn init_into_a_fresh_repo_writes_every_kit_owned_and_seed_once_file_then_renders() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "stderr: {stderr}");

    let mut expected_lines: Vec<String> = KIT_OWNED
        .iter()
        .chain(SEED_ONCE)
        .map(|file| format!("wrote {file}"))
        .collect();
    expected_lines.push("wrote .claude/settings.json".to_string());
    expected_lines.push(".claude/rules/standing-rules.md: written".to_string());
    expected_lines.push(".claude/rules/docs.md: written".to_string());
    expected_lines.push(".claude/skills/project-knowledge/SKILL.md: written".to_string());
    expected_lines.push(format!("houserules: initialized {}", dir.path().display()));
    expected_lines.push("next: houserules check-knowledge && houserules check-backlog".to_string());
    let actual_lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(actual_lines, expected_lines);

    for file in KIT_OWNED.iter().chain(SEED_ONCE) {
        assert!(dir.path().join(file).is_file(), "{file} was not written");
    }
    assert!(dir.path().join(".claude/settings.json").is_file());

    // Ports tests/init.test.mjs:226's "seeds a start hook for startup,
    // resume, clear, and fork sessions, and a compact hook" (batch 20 T1
    // fix round 2, review r2 new_breakage 4): the fresh-seed arm, with no
    // pre-existing settings.json to merge into, had no cargo assertion on
    // TEMPLATE_MATCHERS -- only the three merge-scenario tests below did.
    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).expect("read settings.json"),
    )
    .expect("parse settings.json");
    let matchers: Vec<&str> = settings["hooks"]["SessionStart"]
        .as_array()
        .expect("hooks.SessionStart is an array")
        .iter()
        .map(|entry| entry["matcher"].as_str().expect("matcher is a string"))
        .collect();
    assert_eq!(matchers, TEMPLATE_MATCHERS);
}

/// Ports `tests/init.test.mjs`'s `describe('init')`, "defaults the target
/// to the given cwd" (batch 20 T1 fix round 1, HR-047; review finding 4):
/// with `--dir` omitted entirely, `cmd_init` resolves `dir.as_deref().
/// unwrap_or_else(|| Path::new("."))` against the PROCESS's own working
/// directory, not an ancestor -- the arm `crates/houserules/tests/
/// install.rs`'s other tests never exercise, since every other test here
/// passes `--dir` explicitly. `Command::current_dir` is this file's own
/// equivalent of the JS test's `main(['init'], capture(), dir)`, whose
/// third argument overrides `process.cwd()` the same way.
#[test]
fn init_with_no_dir_flag_seeds_the_current_working_directory() {
    let dir = scratch_git_repo();
    let output = houserules()
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path().join(".githooks/commit-msg").is_file());
}

/// Batch 18 T5 (spec §1): `tools/kb.sh` and `tools/backlog.sh` left
/// `KIT_OWNED`, so a fresh `init` no longer seeds either -- the flat
/// `houserules` command surface is the only entry point a freshly seeded
/// project gets.
#[test]
fn init_no_longer_seeds_the_retired_shell_tools() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("kb.sh"), "got:\n{stdout}");
    assert!(!dir.path().join("tools/kb.sh").exists());
    assert!(!dir.path().join("tools/backlog.sh").exists());
}

/// Batch 20 T3 (HR-047, docs/specs/2026-09-07-batch-20-phase5.md §2): the
/// four JS engines the shell wrappers above used to front left `KIT_OWNED`
/// too, in the same commit -- a fresh `init` no longer seeds any of them.
#[test]
fn init_no_longer_seeds_the_retired_js_engines() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    for file in [
        "tools/kb.mjs",
        "tools/backlog.mjs",
        "tools/lib/cli.mjs",
        "tools/lib/json-store.mjs",
    ] {
        assert!(!stdout.contains(file), "got:\n{stdout}");
        assert!(!dir.path().join(file).exists(), "{file} was seeded");
    }
}

#[test]
fn init_writes_kit_owned_content_byte_identical_to_the_template_source() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for file in KIT_OWNED {
        let expected =
            fs::read(repo_root().join("template").join(file)).expect("read template source");
        let actual = fs::read(dir.path().join(file)).expect("read seeded file");
        assert_eq!(actual, expected, "{file} diverged from template/{file}");
    }
}

#[cfg(unix)]
#[test]
fn init_marks_shell_scripts_and_the_git_hook_executable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for file in ["tools/claude-session-start.sh", ".githooks/commit-msg"] {
        let mode = fs::metadata(dir.path().join(file))
            .unwrap_or_else(|error| panic!("stat {file}: {error}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "{file} mode is {mode:o}");
    }
    // A KIT_OWNED file the JS writer never chmods stays at the plain,
    // non-executable mode `writeFileSync` produces.
    let plain_mode = fs::metadata(dir.path().join(".claude/agents/implementer.md"))
        .expect("stat .claude/agents/implementer.md")
        .permissions()
        .mode();
    assert_eq!(plain_mode & 0o111, 0);
}

#[test]
fn init_stamps_the_marker_with_the_kit_version_and_the_default_id_prefix() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let marker: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".houserules.json")).expect("read marker"),
    )
    .expect("parse marker");
    assert_eq!(
        marker,
        serde_json::json!({"version": kit_version(), "idPrefix": "WI"})
    );
}

#[test]
fn init_rewrites_the_id_prefix_in_prefixed_seed_files_but_not_the_marker_content_rules() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .arg("--id-prefix")
        .arg("FOO")
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for file in [
        "backlog/schema.json",
        "backlog/items/general.json",
        ".claude/schemas/deliverables.json",
    ] {
        let text = fs::read_to_string(dir.path().join(file)).expect("read prefixed file");
        assert!(text.contains("FOO-"), "{file} missing FOO- prefix");
        assert!(!text.contains("WI-"), "{file} still carries WI-");
    }
    let general = fs::read_to_string(dir.path().join("backlog/items/general.json")).unwrap();
    assert!(general.contains("\"FOO-001\""));
}

#[test]
fn init_into_a_missing_git_repo_is_a_named_usage_error_exit_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{} is not a git repository (run git init first)\n",
            dir.path().display()
        )
    );
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn init_rejects_a_malformed_id_prefix_flag_exit_2() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .args(["--id-prefix", "lowercase"])
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "id-prefix must be 1-8 characters, A-Z then A-Z0-9\n"
    );
    // Only the fixture's own `.git` predates the rejected call; nothing else
    // was written.
    let entries: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from(".git")]);
}

/// Ports `tests/init.test.mjs`'s `describe('init')`, "reports a render
/// failure on broken project data as one usage error" (batch 20 T1 fix
/// round 2, HR-047; review r2 new_breakage 3): a pre-existing `knowledge/
/// process.json` holding invalid JSON is a `SEED_ONCE` file already
/// present, so `seed`'s own write loop `kept`s it unmodified and
/// `render_and_report` (`install.rs`, the `seed` arm's own call site) is
/// the first and only step that ever reads its content -- proved by
/// mutation (round-2 review): neutering only this call site's `?` (`let _
/// = crate::rules::render_and_report(target);`) leaves the rest of this
/// suite green and turns this one test's exit-2 assertion into a silent
/// exit 0. No earlier `install.rs` test reaches this arm: every other
/// error-path test here fails before any file write, at the target's
/// `.git` check, the id-prefix flag, or a pre-existing `.houserules.json`
/// -- none of them plants broken data under `knowledge/` first.
#[test]
fn init_reports_a_render_failure_on_broken_project_data_as_one_usage_error() {
    let dir = scratch_git_repo();
    fs::create_dir_all(dir.path().join("knowledge")).expect("mkdir knowledge");
    fs::write(dir.path().join("knowledge/process.json"), "{").expect("write broken process.json");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let broken_path = dir.path().join("knowledge/process.json");
    assert!(
        stderr.starts_with(&format!("{}: invalid JSON (", broken_path.display())),
        "got {stderr:?}"
    );
    assert_eq!(stderr.trim().split('\n').count(), 1, "got {stderr:?}");
}

#[test]
fn init_reports_an_unparsable_pre_existing_marker_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    fs::write(dir.path().join(".houserules.json"), "not json").expect("write marker");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let marker_path = dir.path().join(".houserules.json");
    assert!(
        stderr.starts_with(&format!("{}: invalid JSON (", marker_path.display())),
        "got {stderr:?}"
    );
}

#[test]
fn init_reports_a_pre_existing_markers_bad_id_prefix_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    fs::write(
        dir.path().join(".houserules.json"),
        r#"{"idPrefix":"lowercase"}"#,
    )
    .expect("write marker");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let marker_path = dir.path().join(".houserules.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: idPrefix must be 1-8 characters, A-Z then A-Z0-9\n",
            marker_path.display()
        )
    );
}

#[test]
fn init_reports_a_pre_existing_markers_empty_version_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    fs::write(dir.path().join(".houserules.json"), r#"{"version":""}"#).expect("write marker");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let marker_path = dir.path().join(".houserules.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: version must be a non-empty string\n",
            marker_path.display()
        )
    );
}

#[test]
fn a_second_init_keeps_seed_once_files_and_settings_but_still_overwrites_kit_owned() {
    let dir = scratch_git_repo();
    let first = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("first init");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("second init");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success());

    for file in SEED_ONCE {
        assert!(
            stdout.contains(&format!("kept {file}\n")),
            "expected `kept {file}` in:\n{stdout}"
        );
    }
    for file in KIT_OWNED {
        assert!(
            stdout.contains(&format!("wrote {file}\n")),
            "expected `wrote {file}` in:\n{stdout}"
        );
    }
    assert!(stdout.contains("kept .claude/settings.json (hooks already present)\n"));
}

#[test]
fn init_merges_session_start_hooks_into_a_pre_existing_settings_file() {
    let dir = scratch_git_repo();
    fs::create_dir_all(dir.path().join(".claude")).expect("mkdir .claude");
    fs::write(
        dir.path().join(".claude/settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "hooks": {"SessionStart": [{"matcher": "custom", "hooks": []}]},
            "otherField": true,
        }))
        .unwrap(),
    )
    .expect("write settings.json");

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let template: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo_root().join("template/.claude/settings.json")).unwrap(),
    )
    .unwrap();
    let template_matchers: Vec<&str> = template["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();

    let merged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(merged["otherField"], serde_json::json!(true));
    let merged_entries = merged["hooks"]["SessionStart"].as_array().unwrap();
    let merged_matchers: Vec<&str> = merged_entries
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();
    assert_eq!(merged_matchers[0], "custom");
    for matcher in template_matchers {
        assert!(
            merged_matchers.contains(&matcher),
            "{matcher} missing from merged SessionStart: {merged_matchers:?}"
        );
    }
}

/// Writes `value` as `.claude/settings.json` under `dir`, creating
/// `.claude/` as needed -- shared by the `merge_settings` arm tests below.
fn write_settings(dir: &Path, value: &serde_json::Value) {
    fs::create_dir_all(dir.join(".claude")).expect("mkdir .claude");
    fs::write(
        dir.join(".claude/settings.json"),
        serde_json::to_string_pretty(value).unwrap(),
    )
    .expect("write settings.json");
}

/// The two matchers the embedded `.claude/settings.json` template's own
/// `SessionStart` array carries.
const TEMPLATE_MATCHERS: [&str; 2] = ["compact", "startup|resume|clear|fork"];

/// Fix round 1, issue 1: a JSON `null` at `hooks` seeds cleanly under the
/// frozen JS. Measured live, `.claude/settings.json = {"hooks": null}`:
/// `node bin/houserules.mjs init --dir <scratch>` exits 0, prints `merged
/// .claude/settings.json (SessionStart hooks added)`, and writes both
/// template matchers into `hooks.SessionStart` -- `settings.hooks ??= {}`
/// replaces `null` the same as a missing key, so this is not a crash arm.
#[test]
fn init_seeds_a_null_hooks_settings_file_exactly_as_the_js_does() {
    let dir = scratch_git_repo();
    write_settings(dir.path(), &serde_json::json!({"hooks": null}));

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let merged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let merged_matchers: Vec<&str> = merged["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();
    assert_eq!(merged_matchers, TEMPLATE_MATCHERS);
}

/// Fix round 1, issue 1: a JSON `null` at `hooks.SessionStart` (with
/// `hooks` itself a genuine object) also seeds cleanly. Measured live,
/// `.claude/settings.json = {"hooks": {"SessionStart": null}}`: `node
/// bin/houserules.mjs init` exits 0, prints the same `merged` line, and
/// writes both template matchers -- `settings.hooks.SessionStart ??= []`
/// replaces `null` the same way.
#[test]
fn init_seeds_a_null_session_start_settings_file_exactly_as_the_js_does() {
    let dir = scratch_git_repo();
    write_settings(
        dir.path(),
        &serde_json::json!({"hooks": {"SessionStart": null}}),
    );

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let merged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let merged_matchers: Vec<&str> = merged["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();
    assert_eq!(merged_matchers, TEMPLATE_MATCHERS);
}

/// Fix round 1, issue 1: a `hooks` value that is itself a JSON ARRAY is a
/// genuine JS quirk, not a crash. Measured live,
/// `.claude/settings.json = {"hooks": []}`: `node bin/houserules.mjs
/// init` exits 0, prints the `merged` line (`changed` is unconditionally
/// `true` -- `settings.hooks.SessionStart ??= []` lands a non-index
/// property on the array, every template matcher is pushed into it, and
/// `JSON.stringify` then drops that property because it serializes only
/// an array's indexed elements), and the file reads back byte-for-byte
/// `{"hooks": []}` -- the array's own (empty) elements untouched, no
/// matcher visible anywhere. `install::merge_settings`'s own doc has the
/// full mechanism; this pins its measured, matched output.
#[test]
fn init_matches_the_js_silently_dropping_the_merge_when_hooks_is_an_array() {
    let dir = scratch_git_repo();
    write_settings(dir.path(), &serde_json::json!({"hooks": []}));

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let merged =
        fs::read_to_string(dir.path().join(".claude/settings.json")).expect("read settings.json");
    assert_eq!(merged, "{\n  \"hooks\": []\n}\n");
}

/// Fix round 1, issue 1: `hooks` holding a bare number is the genuine JS
/// crash shape and stays a named error. Measured live,
/// `.claude/settings.json = {"hooks": 5}`: `node bin/houserules.mjs init`
/// exits 1 with an uncaught `TypeError: Cannot create property
/// 'SessionStart' on number '5'` and a full stack trace (ES modules run
/// in strict mode, where assigning a property onto a primitive throws) --
/// `houserules.crash-paths-are-named` converts that crash into one named
/// stderr line and exit 2, never the reproduced trace.
#[test]
fn init_reports_a_scalar_hooks_value_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    write_settings(dir.path(), &serde_json::json!({"hooks": 5}));

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let settings_path = dir.path().join(".claude/settings.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("{}: hooks is not an object\n", settings_path.display())
    );
}

/// Fix round 1, issue 1: `hooks.SessionStart` holding a bare number is the
/// other genuine JS crash shape. Measured live,
/// `.claude/settings.json = {"hooks": {"SessionStart": 5}}`: `node
/// bin/houserules.mjs init` exits 1 with an uncaught `TypeError:
/// settings.hooks.SessionStart.map is not a function` -- named here too.
#[test]
fn init_reports_a_scalar_session_start_value_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    write_settings(
        dir.path(),
        &serde_json::json!({"hooks": {"SessionStart": 5}}),
    );

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let settings_path = dir.path().join(".claude/settings.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: hooks.SessionStart is not an array\n",
            settings_path.display()
        )
    );
}
