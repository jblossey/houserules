//! `render`/`render --check` parity tests (HR-054 task 3, batch 16 phase
//! 1): the frozen fixture corpus byte-compare (mini and root bases) and
//! the `render --check` stdout/exit contract, exercised against the
//! compiled binary the same way `tools/kb.sh render` runs.

mod common;

use std::fs;

use common::{FrozenWorktree, copy_dir_recursive, houserules, list_generated_files, repo_root};

/// Tests (brief item 2): renders against `tests/corpus/fixtures/mini/`
/// and byte-compares every produced file against `tests/corpus/render/mini/**`
/// — the exact file set both ways, the exact bytes.
///
/// The fixture's own checked-in `.claude/rules/*.md` and skill file are
/// already current (they are the source `renderAll` produced them from in
/// the first place), so this deletes them before rendering: the byte
/// compare below proves this run's output matches, not that the fixture's
/// pre-existing files happened to already match.
#[test]
fn renders_byte_identical_to_the_frozen_mini_corpus() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    fs::remove_dir_all(dir.path().join(".claude/rules")).expect("remove .claude/rules");
    fs::remove_file(dir.path().join(".claude/skills/project-knowledge/SKILL.md"))
        .expect("remove the knowledge skill");

    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render");
    assert_eq!(output.status.code(), Some(0), "render exit code");

    let corpus_dir = repo_root().join("tests/corpus/render/mini");
    let expected_files = list_generated_files(&corpus_dir);
    assert_eq!(
        list_generated_files(dir.path()),
        expected_files,
        "mini render produced a different file set than the frozen corpus"
    );
    for relative in &expected_files {
        let expected = fs::read(corpus_dir.join(relative)).expect("read frozen mini corpus file");
        let actual = fs::read(dir.path().join(relative)).expect("read rendered mini file");
        assert_eq!(
            actual, expected,
            "{relative} diverged from the frozen mini corpus"
        );
    }
}

/// Tests (brief item 3): reads the frozen sha from
/// `tests/corpus/manifest.json`, renders against a detached worktree at
/// that sha, and byte-compares every produced file against
/// `tests/corpus/render/root/**`. Also proves the ordering trap (parity
/// trap 1): `areas.json` declares `docs` before `cli` before `template`
/// before `tests` (`tools` has no rule/invariant/gotcha entries and is
/// skipped) — alphabetical order would list `cli` before `docs`, so this
/// is the frozen root base proving the declared order survives, alongside
/// the mini base's own byte-identical round trip above.
///
/// `tools/kb.sh check` gates every commit on CI (`mise run lint`), so the
/// frozen worktree's own checked-in `.claude/rules/*.md` and the skill
/// file are already up to date — `render --check` there reports no stale
/// files at all. The ordering proof needs a stale listing to read, so this
/// deletes those generated files before checking: everything renders as
/// new, in `render_all`'s order.
#[test]
fn renders_byte_identical_to_the_frozen_root_corpus_and_preserves_area_order() {
    let repo_root = repo_root();
    let manifest_text =
        fs::read_to_string(repo_root.join("tests/corpus/manifest.json")).expect("read manifest");
    let manifest: serde_json::Value = serde_json::from_str(&manifest_text).expect("parse manifest");
    let sha = manifest["frozen_sha"]
        .as_str()
        .expect("manifest.frozen_sha is a string");

    let worktree = FrozenWorktree::checkout(&repo_root, sha);
    fs::remove_dir_all(worktree.path.join(".claude/rules")).expect("remove .claude/rules");
    fs::remove_file(
        worktree
            .path
            .join(".claude/skills/project-knowledge/SKILL.md"),
    )
    .expect("remove the knowledge skill");

    // Stale listing order, captured before anything is written, is the
    // area order proof: render_all's Vec order, not an alphabetized one.
    let check_output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run render --check");
    assert_eq!(
        check_output.status.code(),
        Some(1),
        "render --check exit code while stale"
    );
    let stderr = String::from_utf8(check_output.stderr).expect("utf8 stderr");
    let stale_order: Vec<&str> = stderr
        .lines()
        .map(|line| line.trim_end_matches(": would change"))
        .collect();
    assert_eq!(
        stale_order,
        vec![
            ".claude/rules/standing-rules.md",
            ".claude/rules/docs.md",
            ".claude/rules/cli.md",
            ".claude/rules/template.md",
            ".claude/rules/tests.md",
            ".claude/skills/project-knowledge/SKILL.md",
        ],
        "area file order must follow areas.json's declared order, not alphabetical"
    );

    let output = houserules()
        .args(["render", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run render");
    assert_eq!(output.status.code(), Some(0), "render exit code");

    let corpus_dir = repo_root.join("tests/corpus/render/root");
    let expected_files = list_generated_files(&corpus_dir);
    assert_eq!(
        list_generated_files(&worktree.path),
        expected_files,
        "root render produced a different file set than the frozen corpus"
    );
    for relative in &expected_files {
        let expected = fs::read(corpus_dir.join(relative)).expect("read frozen root corpus file");
        let actual = fs::read(worktree.path.join(relative)).expect("read rendered root file");
        assert_eq!(
            actual, expected,
            "{relative} diverged from the frozen root corpus"
        );
    }
}

/// Tests (brief item 4): `render --check` against a base where nothing is
/// stale, and against a copy with one generated file dirtied — ported from
/// tests/kb.test.mjs, describe('main (render, check)')'s render/render
/// --check portion (the `check` command assertions in that JS test belong
/// to `checkBase`, not this task's surface, and are left for the task that
/// ports it). The mini fixture's checked-in generated files are already
/// current (the same reason the root worktree needs pruning above), so
/// "nothing is stale" is the fixture's natural starting state.
#[test]
fn render_check_reports_up_to_date_then_stale_once_dirtied_then_render_repairs_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());

    let output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render --check");
    assert_eq!(output.status.code(), Some(0), "nothing stale yet");
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "render: up to date\n"
    );

    fs::write(
        dir.path().join(".claude/rules/standing-rules.md"),
        "dirtied\n",
    )
    .expect("dirty a generated file");
    let output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render --check");
    assert_eq!(output.status.code(), Some(1), "one file dirtied");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr, ".claude/rules/standing-rules.md: would change\n");

    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert_eq!(stdout, ".claude/rules/standing-rules.md: written\n");

    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "render: up to date\n",
        "render repaired the dirtied file"
    );
}

/// Fix round 1, finding 4, error arm 1 of 3: `render` outside any git
/// repository, with `--dir` omitted, so `cmd_render` falls back to
/// `git rev-parse --show-toplevel` from the current directory and that
/// call fails. Pins the recorded failure-path contract (docs/specs/
/// 2026-09-04-batch-15-tier2-spec.md §6): one named error line to stderr,
/// exit 2 -- already-correct behavior from the prior round, proved here
/// with a disclosed mutation (see the report's tdd entry).
#[test]
fn render_outside_a_git_repository_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");

    let output = houserules()
        .arg("render")
        .current_dir(dir.path())
        .output()
        .expect("run render");

    assert_eq!(
        output.status.code(),
        Some(2),
        "git-root-resolution failure exit code"
    );
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(!stderr.trim().is_empty(), "the error line is not empty");
}

/// Fix round 1, finding 4, error arm 2 of 3: `render --dir` at a
/// directory with no `knowledge/` at all, so `load_base` fails reading
/// `knowledge/schema.json`. Pins the same contract as arm 1.
#[test]
fn render_missing_knowledge_directory_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");

    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render");

    assert_eq!(output.status.code(), Some(2), "load_base failure exit code");
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(
        stderr.contains("schema.json"),
        "the error names the missing file, got: {stderr:?}"
    );
}

/// Fix round 1, finding 4, error arm 3 of 3: a base that loads
/// successfully but whose `.claude` path is a plain file, not a
/// directory, so `render`'s `fs::create_dir_all(".claude/rules")` fails
/// with a portable (Linux/macOS/Windows) IO error. Pins the same contract
/// as arms 1 and 2. Non-check mode, since `--check` never writes.
#[test]
fn render_io_failure_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    fs::remove_dir_all(dir.path().join(".claude")).expect("remove .claude");
    fs::write(dir.path().join(".claude"), b"not a directory").expect("replace .claude with a file");

    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render");

    assert_eq!(output.status.code(), Some(2), "render io failure exit code");
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(!stderr.trim().is_empty(), "the error line is not empty");
}

/// Fix round 1, finding 4 (the schema.json divergence): a base with
/// `knowledge/areas.json` and a topic file but no `knowledge/schema.json`
/// fails `render` the same way `tools/kb.sh render` fails there, because
/// `loadBase` reads `schema.json` unconditionally. Reported through the
/// same named-error, exit-2 contract as the other three arms.
#[test]
fn render_fails_on_a_base_missing_schema_json_matching_the_js_requirement() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    fs::remove_file(dir.path().join("knowledge/schema.json")).expect("remove schema.json");

    let output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render --check");

    assert_eq!(
        output.status.code(),
        Some(2),
        "missing-schema.json exit code"
    );
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("schema.json"),
        "the error names the missing file, got: {stderr:?}"
    );
}

/// Fix round 1, finding 2 (the end-to-end half): a malformed glob in
/// `knowledge/areas.json` fails `render` with the named error, exit 2 --
/// `load_areas`'s eager validation (crates/houserules/src/rules/model.rs)
/// surfaces it through the same contract as the other error arms, instead
/// of only once a later phase's matcher first reaches it.
#[test]
fn render_fails_on_a_malformed_area_glob_with_a_named_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    let areas_path = dir.path().join("knowledge/areas.json");
    let areas_text = fs::read_to_string(&areas_path).expect("read areas.json");
    assert!(
        areas_text.contains("\"mini-tools/**\""),
        "fixture shape changed, update this test's injected glob"
    );
    let mangled = areas_text.replace("\"mini-tools/**\"", "\"mini-tools/**\", \"a[z-a]b\"");
    fs::write(&areas_path, mangled).expect("write a malformed glob into areas.json");

    let output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render --check");

    assert_eq!(output.status.code(), Some(2), "malformed-glob exit code");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("a[z-a]b"),
        "the error names the offending glob, got: {stderr:?}"
    );
}
