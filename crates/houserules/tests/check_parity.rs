//! `check-knowledge` parity tests (HR-054 task 4, batch 16 phase 1): the
//! frozen fixture corpus's four check slices (`mini`, `mini-bad`,
//! `mini-stale`, `root`), byte-compared verbatim against the compiled
//! binary, and the CLI's load-failure-vs-check-finding boundary (spec
//! §6's CLI-failure-path deviation and the eager-glob-validation
//! deviation, docs/specs/2026-09-04-batch-15-tier2-spec.md).

mod common;

use std::fs;

use common::{FrozenWorktree, copy_dir_recursive, houserules, repo_root};

/// One frozen `tests/corpus/check/<slice>.json` capture: `stdout`,
/// `stderr`, and `exit`, read directly from the corpus file rather than
/// hand-copied, so a corpus regeneration is the only way this test's
/// expectation can drift.
struct CorpusCheck {
    stdout: String,
    stderr: String,
    exit: i32,
}

fn corpus_check(slice: &str) -> CorpusCheck {
    let text = fs::read_to_string(repo_root().join(format!("tests/corpus/check/{slice}.json")))
        .expect("read corpus check slice");
    let value: serde_json::Value = serde_json::from_str(&text).expect("parse corpus check slice");
    CorpusCheck {
        stdout: value["stdout"].as_str().expect("stdout").to_string(),
        stderr: value["stderr"].as_str().expect("stderr").to_string(),
        exit: value["exit"].as_i64().expect("exit") as i32,
    }
}

fn assert_matches_corpus(dir: &std::path::Path, slice: &str) {
    let expected = corpus_check(slice);
    let output = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir)
        .output()
        .expect("run check-knowledge");
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        expected.stdout,
        "{slice}: stdout diverged from the frozen corpus"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        expected.stderr,
        "{slice}: stderr diverged from the frozen corpus"
    );
    assert_eq!(
        output.status.code(),
        Some(expected.exit),
        "{slice}: exit code diverged from the frozen corpus"
    );
}

/// Parity gate 1 (brief item 1 of 4): the `mini` fixture is a clean base --
/// `check-knowledge` there must print `knowledge: ok` and exit 0, exactly
/// as `tests/corpus/check/mini.json` pins.
#[test]
fn check_knowledge_matches_the_frozen_mini_corpus() {
    assert_matches_corpus(&repo_root().join("tests/corpus/fixtures/mini"), "mini");
}

/// Parity gate 1, slice 2 of 4: `mini-bad` seeds all nine
/// `template/tools/lib/json-store.mjs` validator message shapes plus every
/// `checkBase`/`checkShape` per-entry shape (id prefix, duplicate id,
/// standing, `see`, `verify`, check-shape field and regex errors, and the
/// topic/file-name mismatch) -- `tests/corpus/check/mini-bad.json` pins
/// the full 20-message stderr, verbatim and in order, and exit 1.
#[test]
fn check_knowledge_matches_the_frozen_mini_bad_corpus() {
    assert_matches_corpus(
        &repo_root().join("tests/corpus/fixtures/mini-bad"),
        "mini-bad",
    );
}

/// Parity gate 1, slice 3 of 4: `mini-stale` is a base that passes the
/// first (schema/shape) stage cleanly, so `check_base` reaches its
/// post-early-return checks -- a stale generated file, a missing one, a
/// stray file in `.claude/rules`, and both a byte and a line budget
/// overrun -- pinned by `tests/corpus/check/mini-stale.json`.
#[test]
fn check_knowledge_matches_the_frozen_mini_stale_corpus() {
    assert_matches_corpus(
        &repo_root().join("tests/corpus/fixtures/mini-stale"),
        "mini-stale",
    );
}

/// Parity gate 1, slice 4 of 4: this repository's own knowledge base, at
/// the frozen sha, is clean -- `tests/corpus/check/root.json` pins
/// `knowledge: ok`, exit 0, the same as the `mini` slice but exercising
/// this repository's real, larger knowledge base end to end.
#[test]
fn check_knowledge_matches_the_frozen_root_corpus() {
    let worktree = FrozenWorktree::checkout(&repo_root(), read_frozen_sha());
    assert_matches_corpus(&worktree.path, "root");
}

fn read_frozen_sha() -> &'static str {
    // `tests/corpus/manifest.json`'s `frozen_sha` is a73a8c6b1c511217ce...
    // (verified against the manifest at write time); reading it here
    // (rather than hardcoding it a second time) keeps this test honest
    // about which sha it checks out if the corpus is ever regenerated.
    static SHA: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SHA.get_or_init(|| {
        let text = fs::read_to_string(repo_root().join("tests/corpus/manifest.json"))
            .expect("read corpus manifest");
        let manifest: serde_json::Value =
            serde_json::from_str(&text).expect("parse corpus manifest");
        manifest["frozen_sha"]
            .as_str()
            .expect("manifest.frozen_sha is a string")
            .to_string()
    })
    .as_str()
}

/// Interaction (brief, "T3's eager glob validation"): `check-knowledge`
/// reuses `load_base`, so a malformed `areas.json` glob fails at load
/// time, one named line, exit 2 -- distinct from a base that loads fine
/// and fails the check itself (`mini-bad`/`mini-stale` above, both exit 1
/// with the ported `checkBase` stderr messages). This is also the
/// boundary test the brief's parity gate 3 asks for: a base that fails to
/// load side by side with a base that loads and fails the check.
#[test]
fn check_knowledge_fails_on_a_malformed_area_glob_with_a_named_error() {
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
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge");

    assert_eq!(output.status.code(), Some(2), "malformed-glob exit code");
    assert_eq!(output.stdout, b"", "no stdout on the load-failure path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(
        stderr.contains("a[z-a]b"),
        "the error names the offending glob, got: {stderr:?}"
    );
}

/// Load-failure error arm 2 of 2: `check-knowledge --dir` at a directory
/// with no `knowledge/` at all fails the same way `render` does (both
/// call `load_base`), one named line naming `schema.json`, exit 2.
#[test]
fn check_knowledge_missing_knowledge_directory_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");

    let output = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge");

    assert_eq!(output.status.code(), Some(2), "load_base failure exit code");
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(
        stderr.contains("schema.json"),
        "the error names the missing file, got: {stderr:?}"
    );
}

/// Load-failure error arm, git-resolution leg: `check-knowledge` with
/// `--dir` omitted outside any git repository falls back to
/// `repo_root_from_cwd` (shared with `render`), fails the same way.
#[test]
fn check_knowledge_outside_a_git_repository_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");

    let output = houserules()
        .arg("check-knowledge")
        .current_dir(dir.path())
        .output()
        .expect("run check-knowledge");

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

/// tests/kb.test.mjs, `describe('main (render, check)')`: "render --check
/// exits 1 while stale, render writes, check reports and passes" -- ported
/// whole (both the render half and the check half were one interleaved
/// `it()` on the JS side; deleted from tests/kb.test.mjs in the same
/// commit as this port).
#[test]
fn render_check_exits_1_while_stale_render_writes_check_reports_and_passes() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    fs::remove_dir_all(dir.path().join(".claude/rules")).expect("remove .claude/rules");
    fs::remove_file(dir.path().join(".claude/skills/project-knowledge/SKILL.md"))
        .expect("remove the knowledge skill");

    let output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render --check");
    assert_eq!(output.status.code(), Some(1), "render --check while stale");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains(".claude/rules/standing-rules.md: would change\n"),
        "got: {stderr:?}"
    );

    let output = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge while stale");
    assert_eq!(output.status.code(), Some(1), "check-knowledge while stale");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.contains("generated file is out of date"),
        "got: {stderr:?}"
    );

    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render");
    assert_eq!(output.status.code(), Some(0), "render writes");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains(".claude/rules/standing-rules.md: written\n"),
        "got: {stdout:?}"
    );

    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render again");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "render: up to date\n"
    );

    let output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run render --check once clean");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "render: up to date\n"
    );

    let output = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge once clean");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "knowledge: ok\n"
    );
}
