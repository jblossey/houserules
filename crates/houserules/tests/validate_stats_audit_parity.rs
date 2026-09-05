//! `validate`, `stats`, and `audit` parity tests (batch 17 T3, docs/specs/
//! 2026-09-04-batch-15-tier2-spec.md §5 phase 2): BYTE parity for
//! `validate` (all four frozen corpus slices) and `stats` (both frozen
//! corpus slices) against `tools/kb.mjs`'s captured output; FIELD-identical
//! JSON (spec §4) for `audit` against both frozen audit slices, compared as
//! parsed `serde_json::Value` rather than raw bytes -- a JSON object's key
//! order is not part of its value (this crate's `preserve_order` feature
//! backs `Value::Object` with an `IndexMap`, whose `PartialEq` is itself
//! order-independent), so this is the correct comparison for "same fields,
//! same values, same row order" without demanding a byte-identical
//! `area_files` key order the frozen JS's own insertion-order artifact
//! never promised either (`rules::audit`'s own module doc has the fuller
//! account of that specific field).
//!
//! Also pins the ruled clap argv deviation (spec §6) for these three
//! commands, the same pattern `backlog_parity.rs`'s own such tests use.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use common::{FrozenWorktree, houserules, repo_root};

fn read_frozen_sha() -> String {
    let text = fs::read_to_string(repo_root().join("tests/corpus/manifest.json"))
        .expect("read corpus manifest");
    let manifest: Value = serde_json::from_str(&text).expect("parse corpus manifest");
    manifest["frozen_sha"]
        .as_str()
        .expect("manifest.frozen_sha is a string")
        .to_string()
}

/// One frozen `{command, cwd, stdout, stderr, exit}` capture, read directly
/// from the corpus file rather than hand-copied.
struct CorpusCapture {
    stdout: String,
    stderr: String,
    exit: i32,
}

fn corpus_capture(relative: &str) -> CorpusCapture {
    let path = repo_root().join(relative);
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let value: Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    CorpusCapture {
        stdout: value["stdout"].as_str().expect("stdout").to_string(),
        stderr: value["stderr"].as_str().expect("stderr").to_string(),
        exit: value["exit"].as_i64().expect("exit") as i32,
    }
}

/// Replaces every occurrence of `from`'s displayed path in `text` with
/// `placeholder` -- the inverse of `tools/make-corpus.mjs`'s own
/// `redactPath`, applied to this binary's own output before comparing it
/// against the corpus's already-redacted bytes (`kb.mjs validate` echoes
/// the caller's resolved absolute path into each result's `file` field;
/// the placeholder keeps the comparison host-independent).
fn redact(text: &str, from: &Path, placeholder: &str) -> String {
    text.replace(&from.display().to_string(), placeholder)
}

fn sorted_json_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    names
}

// ---- validate: byte parity, all four corpus slices ---------------------------------

fn assert_validate_matches_corpus(
    worktree: &Path,
    files: &[PathBuf],
    corpus_slice: &str,
    redact_from: Option<(&Path, &str)>,
) {
    let mut cmd = houserules();
    cmd.arg("validate");
    for file in files {
        cmd.arg(file);
    }
    cmd.current_dir(worktree);
    let output = cmd.output().expect("run validate");
    let expected = corpus_capture(&format!("tests/corpus/validate/{corpus_slice}"));

    let mut stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let mut stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    if let Some((from, placeholder)) = redact_from {
        stdout = redact(&stdout, from, placeholder);
        stderr = redact(&stderr, from, placeholder);
    }
    assert_eq!(stdout, expected.stdout, "{corpus_slice}: stdout diverged");
    assert_eq!(stderr, expected.stderr, "{corpus_slice}: stderr diverged");
    assert_eq!(
        output.status.code(),
        Some(expected.exit),
        "{corpus_slice}: exit code diverged"
    );
}

/// Parity gate, slice 1 of 4 (passing): every `.json` file in the archived
/// batch-14 workspace fixture, validated together.
#[test]
fn validate_matches_the_frozen_batch14_workspace_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/batch14-workspace");
    let files: Vec<PathBuf> = sorted_json_names(&fixtures)
        .into_iter()
        .map(|name| fixtures.join(name))
        .collect();
    assert_validate_matches_corpus(
        &worktree.path,
        &files,
        "batch14-workspace.json",
        Some((&fixtures, "<fixtures>/batch14-workspace")),
    );
}

/// Parity gate, slice 2 of 4 (passing, single file): the same fixture's
/// `task-1-report.json` alone.
#[test]
fn validate_matches_the_frozen_task_1_report_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/batch14-workspace");
    assert_validate_matches_corpus(
        &worktree.path,
        &[fixtures.join("task-1-report.json")],
        "task-1-report.json",
        Some((&fixtures, "<fixtures>/batch14-workspace")),
    );
}

/// Parity gate, slice 3 of 4 (failing): a report with two schema
/// violations.
#[test]
fn validate_matches_the_frozen_invalid_deliverable_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/invalid-deliverable");
    assert_validate_matches_corpus(
        &worktree.path,
        &[fixtures.join("bad-report.json")],
        "invalid-deliverable.json",
        Some((&fixtures, "<fixtures>/invalid-deliverable")),
    );
}

/// Parity gate, slice 4 of 4 (failing, `skipped > 0`): the parked
/// `self_audit.summary.skipped` report-field message (batch 16 T4 r1
/// re-review), frozen for the first time in this batch's T1.
#[test]
fn validate_matches_the_frozen_skipped_report_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/skipped-report");
    assert_validate_matches_corpus(
        &worktree.path,
        &[fixtures.join("skipped-report.json")],
        "skipped-report.json",
        Some((&fixtures, "<fixtures>/skipped-report")),
    );
}

// ---- stats: byte parity, both corpus slices -----------------------------------------

fn assert_stats_matches_corpus(worktree: &Path, workspace: &Path, corpus_slice: &str) {
    let output = houserules()
        .args(["stats"])
        .arg(workspace)
        .current_dir(worktree)
        .output()
        .expect("run stats");
    let expected = corpus_capture(&format!("tests/corpus/stats/{corpus_slice}"));
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        expected.stdout,
        "{corpus_slice}: stdout diverged"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        expected.stderr,
        "{corpus_slice}: stderr diverged"
    );
    assert_eq!(
        output.status.code(),
        Some(expected.exit),
        "{corpus_slice}: exit code diverged"
    );
}

/// Parity gate, slice 1 of 2: the archived batch-14 workspace, which holds
/// no `task-*-audit*.json` at all -- this slice exercises only `stats`'
/// reviews/reports input paths.
#[test]
fn stats_matches_the_frozen_batch14_workspace_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/batch14-workspace");
    assert_stats_matches_corpus(&worktree.path, &fixtures, "batch14-workspace.json");
}

/// Parity gate, slice 2 of 2: `stats-workspace`, which carries one
/// `task-1-audit.json` (two injected ids, one failing rule) and one
/// `task-1-report.json` citing exactly one of those ids -- the slice that
/// makes the audits input path and the `unused_ids` cited-vs-injected
/// cross-reference observable.
#[test]
fn stats_matches_the_frozen_stats_workspace_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/stats-workspace");
    assert_stats_matches_corpus(&worktree.path, &fixtures, "stats-workspace.json");
}

// ---- audit: field-identical JSON, both frozen audit slices --------------------------

fn assert_audit_matches_corpus(
    worktree: &Path,
    base: &str,
    head: &str,
    ids: &str,
    corpus_slice: &str,
    expected_exit: i32,
) {
    let output = houserules()
        .args(["audit", "--base", base, "--head", head, "--ids", ids])
        .current_dir(worktree)
        .output()
        .expect("run audit");
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "{corpus_slice}: exit code diverged"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let actual: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| {
        panic!("{corpus_slice}: parse this binary's own stdout: {error}\n{stdout}")
    });
    let expected = corpus_capture(&format!("tests/corpus/audit/{corpus_slice}"));
    let expected_value: Value =
        serde_json::from_str(&expected.stdout).expect("parse the frozen corpus stdout");
    assert_eq!(
        actual, expected_value,
        "{corpus_slice}: field-identical JSON diverged"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        expected.stderr,
        "{corpus_slice}: stderr diverged"
    );
}

/// Field-identical gate, slice 1 of 2 (failing range): the range whose
/// `--ids` names `process.deliverables-json` and finds a task report's
/// terminal status without a filled `self_audit`.
#[test]
fn audit_matches_the_frozen_validate_terminal_report_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_audit_matches_corpus(
        &worktree.path,
        "a13117540cc1480b00d9b57907d3ad4b02767b1c",
        "1537d89ad000d7376160c30fb06edc604ce4352c",
        "houserules.template-is-the-source,process.tdd,process.deliverables-json,quality.principles,writing-style.doc-comments",
        "validate-terminal-report.json",
        1,
    );
}

/// Field-identical gate, slice 2 of 2 (clean range).
#[test]
fn audit_matches_the_frozen_knowledge_retrospective_slice() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_audit_matches_corpus(
        &worktree.path,
        "c290a29526aa30c080cc9bfbdd7753b746e6e22d",
        "779300045991aa4349c2b6774c181aec36af7cb7",
        "houserules.template-is-the-source,process.deliverables-json,writing-style.principles,quality.principles,knowledge-base.state-only-the-source",
        "knowledge-retrospective.json",
        0,
    );
}

// ---- interaction: a base that fails to load side by side with the deliverable-facing
// commands' own contract (the brief's parity gate 3 pattern) --------------------------

/// `validate`/`stats`/`audit` all resolve their repository root the same
/// way `render`/`check-knowledge` do (`--dir`, or the enclosing git
/// repository's top level via `repo_root_from_cwd`), and load the
/// knowledge base there before dispatching -- see `rules::stats::cmd_stats`'s
/// own doc for why this replicates `tools/kb.mjs`'s own unconditional
/// `loadBase` call. A missing `knowledge/` directory is therefore the same
/// one-line, exit-2 CLI failure for all three, not only for `render`/
/// `check-knowledge`.
#[test]
fn validate_stats_and_audit_fail_the_same_way_render_does_on_a_missing_knowledge_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    common::copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    fs::remove_dir_all(dir.path().join("knowledge")).expect("remove knowledge/");

    for (args, positional) in [
        (vec!["validate"], vec!["report.json"]),
        (vec!["stats"], vec!["."]),
        (vec!["audit", "--base", "HEAD"], vec![]),
    ] {
        let mut cmd = houserules();
        cmd.args(&args);
        cmd.args(&positional);
        cmd.current_dir(dir.path());
        let output = cmd.output().expect("run command");
        assert_eq!(output.status.code(), Some(2), "{args:?}: exit code");
        assert_eq!(output.stdout, b"", "{args:?}: no stdout on the error path");
        let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
        assert_eq!(
            stderr.lines().count(),
            1,
            "{args:?}: exactly one named error line"
        );
    }

    // Fix round 1, issue 9 (task-3-review.json): `validate` with ZERO
    // positional arguments too -- the one argument shape the table above
    // never reaches, since its own `validate` case always supplies
    // `report.json`. `tools/kb.mjs`'s `main` loads the knowledge base
    // before dispatching to `validate`'s own arity check, so a missing
    // `knowledge/` must report the KNOWLEDGE error even with no files
    // given, not "validate needs at least one file" (which the earlier
    // cut's reversed check order produced instead).
    let output = houserules()
        .arg("validate")
        .current_dir(dir.path())
        .output()
        .expect("run validate with no files and no knowledge/");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert_ne!(
        stderr, "validate needs at least one file\n",
        "a missing knowledge/ must be reported before the arity check runs, matching main's own load-then-dispatch order"
    );
}

// ---- ruled argv deviation (spec §6): the binary parses argv with clap, so a flag the
// frozen JS's parseArgs silently ignored or coerced to a bare true is a named clap
// usage error at exit 2 instead. See backlog_parity.rs's own such block for the pattern
// and its rationale (reconstructed, not mutation or natural: both sides are third-party
// behavior, clap's argument validation and parseArgs' own ambiguity).

/// `audit --base` with no value: JS coerces the bare flag to `true`
/// (`typeof opts.base === 'string' ? opts.base : undefined`), so `baseRef`
/// is `undefined` and `audit` reports its own "needs --base" usage error,
/// exit 2 -- the same exit code the binary's clap-native "a value is
/// required" error gives, but for a different reason and with different
/// wording, so this pins the clap wording actually shown.
#[test]
fn audit_base_with_no_value_exits_2_with_claps_own_message() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--base"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: a value is required for '--base <BASE>' but none was supplied"),
        "got: {stderr:?}"
    );
}

/// `audit --base X --base Y`: JS's last value wins (an object property
/// assigned twice), so the audit runs against `Y` alone, exit 0/1; the
/// binary refuses the duplicate, exit 2.
#[test]
fn audit_with_a_duplicated_base_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--base", "HEAD"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --base HEAD");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--base <BASE>' cannot be used multiple times"),
        "got: {stderr:?}"
    );
}

// ---- batch 17 T4 (HR-059's fold-in list): the same duplicate-value-flag
// class as `--base`/`--json` above, for `--head`/`--ids`/`--report`/
// `--workspace` -- the four shapes T3's own enumeration (task-3-fix2-
// enumerate-argv.sh) omitted. Each verified live first with an INVALID
// first value and a VALID second one, so JS's own last-value-wins reading
// genuinely succeeds (exit 0) rather than merely failing for an unrelated
// reason that happens to also be exit 2 (`--ids`/`--report`/`--workspace`
// each looked identical to a real divergence on a first pass using two
// equally-bogus values, since JS's own business logic then fails on the
// surviving bogus one too -- only a valid survivor tells the two shapes
// apart).

/// `audit --base HEAD --head HEAD --head HEAD`: JS's last value wins, exit
/// 0 (an empty, valid range); the binary refuses the duplicate, exit 2.
#[test]
fn audit_with_a_duplicated_head_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args([
            "audit", "--base", "HEAD", "--head", "HEAD", "--head", "HEAD",
        ])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --head HEAD --head HEAD");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--head <HEAD>' cannot be used multiple times"),
        "got: {stderr:?}"
    );
}

/// `audit --ids a.bogus --ids process.tdd`: JS's last value wins -- a real
/// id, so the audit runs and exits 0; the binary refuses the duplicate,
/// exit 2.
#[test]
fn audit_with_a_duplicated_ids_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args([
            "audit",
            "--base",
            "HEAD",
            "--ids",
            "a.bogus",
            "--ids",
            "process.tdd",
        ])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --ids a.bogus --ids process.tdd");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--ids <IDS>' cannot be used multiple times"),
        "got: {stderr:?}"
    );
}

/// `audit --report bogus.json --report <real report>`: JS's last value
/// wins -- a real, readable file, so the audit runs and exits 0; the
/// binary refuses the duplicate, exit 2.
#[test]
fn audit_with_a_duplicated_report_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let report = repo_root().join("tests/corpus/fixtures/batch14-workspace/task-1-report.json");
    let output = houserules()
        .args([
            "audit",
            "--base",
            "HEAD",
            "--report",
            "bogus.json",
            "--report",
        ])
        .arg(&report)
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --report bogus.json --report <real report>");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--report <REPORT>' cannot be used multiple times"),
        "got: {stderr:?}"
    );
}

/// `audit --workspace bogus-dir --workspace <real workspace>`: JS's last
/// value wins -- a real, readable directory, so the audit runs and exits
/// 0; the binary refuses the duplicate, exit 2.
#[test]
fn audit_with_a_duplicated_workspace_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let workspace = repo_root().join("tests/corpus/fixtures/batch14-workspace");
    let output = houserules()
        .args([
            "audit",
            "--base",
            "HEAD",
            "--workspace",
            "bogus-dir",
            "--workspace",
        ])
        .arg(&workspace)
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --workspace bogus-dir --workspace <real workspace>");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with(
            "error: the argument '--workspace <WORKSPACE>' cannot be used multiple times"
        ),
        "got: {stderr:?}"
    );
}

/// `audit --bogus --base HEAD`: JS ignores the unrecognized flag and
/// audits normally; the binary reports it as an unexpected argument, exit
/// 2.
#[test]
fn audit_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--bogus", "--base", "HEAD"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `audit extra --base HEAD`: `audit` takes no positional argument at all
/// in the frozen JS, so a stray one is silently parsed into `positional`
/// and never read -- JS audits normally, exit 0/1; the binary reports it
/// as an unexpected argument, exit 2.
#[test]
fn audit_with_an_unexpected_positional_exits_2_where_js_silently_ignored_it() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "extra", "--base", "HEAD"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit extra");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: unexpected argument 'extra' found"),
        "got: {stderr:?}"
    );
}

/// `validate --bogus report.json` (the unknown flag BEFORE the file): both
/// engines exit 2, but not for the same reason, and not because either one
/// recognizes `--bogus` (fix round 2, task-3-review-r1.json finding 5 --
/// this test previously claimed JS "ignored it and succeeded", which is
/// only true when the flag comes AFTER the file;
/// `validate_with_an_unknown_flag_after_the_file_exits_2_where_js_ignored_it_and_succeeded`
/// below is that case). JS's own `parseArgs`-style loop treats an
/// unrecognized `--flag` as consuming the very next token as its value
/// (the same "unexpected positional consumed as a flag's value" class
/// `list_with_an_unexpected_positional_exits_2_where_js_consumed_it_as_a_flag_value`
/// pins for backlog): `--bogus report.json` leaves zero positional files,
/// so JS's own arity check fires ("validate needs at least one file"). The
/// binary's clap surface takes `--bogus` as an unexpected argument the
/// moment it appears, regardless of position, so it also exits 2, with a
/// different message.
#[test]
fn validate_with_an_unknown_flag_before_the_file_exits_2_in_both_engines_for_different_reasons() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/batch14-workspace");
    let output = houserules()
        .args(["validate", "--bogus"])
        .arg(fixtures.join("task-1-report.json"))
        .current_dir(&worktree.path)
        .output()
        .expect("run validate --bogus report.json");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `validate report.json --bogus` (the unknown flag AFTER the file): JS
/// ignores the unrecognized flag (already having captured `report.json`
/// as its one positional) and validates the file, exit 0; the binary's
/// clap surface reports `--bogus` as an unexpected argument regardless of
/// position, exit 2 -- the real divergence finding 5's own audit missed,
/// since the sibling test above puts the flag first, where JS also exits
/// 2 for an unrelated reason (fix round 2, task-3-review-r1.json finding
/// 5; verified live against the frozen worktree).
#[test]
fn validate_with_an_unknown_flag_after_the_file_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let fixtures = repo_root().join("tests/corpus/fixtures/batch14-workspace");
    let output = houserules()
        .arg("validate")
        .arg(fixtures.join("task-1-report.json"))
        .arg("--bogus")
        .current_dir(&worktree.path)
        .output()
        .expect("run validate report.json --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

// ---- CLI-dispatch coverage (tests/kb.test.mjs, describe('main (audit, stats)') and
// describe('main (validate)')): behavior only observable by actually running the
// compiled binary -- argv forwarding, printed output, exit codes, and file writes --
// so these live here rather than as unit tests on the pure audit/validate_deliverable/
// stats functions (matching backlog_parity.rs's own split for the backlog module's
// cmd_* layer, which carries no unit tests of its own for the same reason).

/// tests/kb.test.mjs, describe('main (audit, stats)'): "audit exits 1 on a
/// failure, 0 when clean, 2 without --base ..." -- the `--base`-missing arm.
#[test]
fn audit_needs_base_prints_the_frozen_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .arg("audit")
        .current_dir(&worktree.path)
        .output()
        .expect("run audit");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"audit needs --base <ref>\n");
}

/// tests/kb.test.mjs, describe('main (audit, stats)'): "trims the values in
/// --ids" and "forwards ... --json from the CLI to audit" (the --json file
/// write half): a comma-and-space-separated --ids value trims to the bare
/// ids in the JSON output, and the file --json names holds the identical
/// JSON `emit` also printed to stdout.
#[test]
fn audit_trims_the_values_in_ids_and_writes_the_json_file() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let json_path = worktree.path.join("out.json");
    let output = houserules()
        .args([
            "audit",
            "--base",
            "c290a29526aa30c080cc9bfbdd7753b746e6e22d",
            "--head",
            "779300045991aa4349c2b6774c181aec36af7cb7",
            "--ids",
            "process.tdd, quality.principles",
        ])
        .arg("--json")
        .arg(&json_path)
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --ids with spaces");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout: Value = serde_json::from_slice(&output.stdout).expect("parse stdout");
    assert_eq!(
        stdout["ids"],
        serde_json::json!(["process.tdd", "quality.principles"])
    );
    let file_text = fs::read_to_string(&json_path).expect("read --json output");
    let file_value: Value = serde_json::from_str(&file_text).expect("parse --json output");
    assert_eq!(file_value, stdout, "the --json file must match stdout");
}

/// tests/kb.test.mjs, describe('main (audit, stats)'): "resolves --report
/// and --json against the given cwd, not process.cwd()". `--dir` is
/// unset, so the repository root resolves from `--base`/`--head` alone via
/// `repo_root_from_cwd`; `--report`/`--json`'s relative paths must instead
/// resolve against the process's actual working directory, a `sub`
/// directory of the worktree, not its root.
#[test]
fn resolves_report_and_json_against_the_given_cwd_not_the_worktree_root() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let sub = worktree.path.join("sub");
    fs::create_dir(&sub).expect("mkdir sub");
    fs::write(sub.join("rel.json"), "{}").expect("write rel.json");
    let output = houserules()
        .args([
            "audit",
            "--base",
            "c290a29526aa30c080cc9bfbdd7753b746e6e22d",
            "--head",
            "779300045991aa4349c2b6774c181aec36af7cb7",
            "--report",
            "rel.json",
            "--json",
            "out.json",
        ])
        .current_dir(&sub)
        .output()
        .expect("run audit with relative --report/--json");
    assert_ne!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _: Value = serde_json::from_slice(&output.stdout).expect("stdout is valid JSON");
    assert!(
        sub.join("out.json").exists(),
        "--json must resolve against the given cwd"
    );
    assert!(
        !worktree.path.join("out.json").exists(),
        "--json must not resolve against the worktree root"
    );
}

/// tests/kb.test.mjs, describe('main (audit, stats)'): "reports a usage
/// error, not a stack trace, when base and head share no merge base"
/// (HR-009, Minor #1).
#[test]
fn audit_reports_a_usage_error_not_a_stack_trace_when_base_and_head_share_no_merge_base() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    common::copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), root);
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    };
    let git_output = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).expect("utf8 git output")
    };
    let commit_args = [
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t.t",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--no-verify",
    ];
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&[&commit_args[..], &["-m", "chore: init"]].concat());
    git(&["checkout", "-q", "--orphan", "orphan"]);
    git(&["rm", "-rf", "-q", "."]);
    fs::write(root.join("orphan.txt"), "x\n").expect("write orphan.txt");
    git(&["add", "-A"]);
    git(&[&commit_args[..], &["-m", "chore: orphan commit"]].concat());
    git(&["checkout", "-q", "main"]);
    let main_sha = git_output(&["rev-parse", "--short", "main"])
        .trim()
        .to_string();
    let orphan_sha = git_output(&["rev-parse", "--short", "orphan"])
        .trim()
        .to_string();

    let output = houserules()
        .args(["audit", "--base", "main", "--head", "orphan"])
        .current_dir(root)
        .output()
        .expect("run audit across an orphan branch");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        format!("no merge base between \"{main_sha}\" and \"{orphan_sha}\"\n")
    );
}

/// tests/kb.test.mjs, describe('main (audit, stats)'): the last of the
/// four `stats` assertions bundled into "audit exits 1 on a failure, 0
/// when clean, 2 without --base; stats needs one dir" -- a malformed
/// deliverable file in the workspace is a named error at the CLI boundary
/// too, not only from `stats` itself. Misattributed to the *arity* arm
/// ("stats needs one dir") through fix round 1 (task-3-review.json, issue
/// 5); `stats_with_zero_positional_arguments_exits_2` below is that arm's
/// own, separate test.
#[test]
fn stats_reports_a_malformed_deliverable_file_naming_it_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let dir = tempfile::tempdir().expect("tempdir");
    fs::write(dir.path().join("task-3-audit.json"), "{\"ids\": [").expect("write fixture");
    let output = houserules()
        .arg("stats")
        .arg(dir.path())
        .current_dir(&worktree.path)
        .output()
        .expect("run stats");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("task-3-audit.json"), "got: {stderr:?}");
}

/// tests/kb.test.mjs, describe('main (validate)'): "prints results and
/// exits 1 when any file has errors, 0 when all are valid" and "... 2"
/// (`validate needs at least one file`).
#[test]
fn validate_needs_at_least_one_file_prints_the_frozen_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .arg("validate")
        .current_dir(&worktree.path)
        .output()
        .expect("run validate");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"validate needs at least one file\n");
}

#[test]
fn validate_exits_1_when_any_file_has_errors_0_when_all_are_valid() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let good = repo_root().join("tests/corpus/fixtures/batch14-workspace/task-1-report.json");
    let bad = repo_root().join("tests/corpus/fixtures/invalid-deliverable/bad-report.json");

    let output = houserules()
        .arg("validate")
        .arg(&good)
        .arg(&bad)
        .current_dir(&worktree.path)
        .output()
        .expect("run validate good bad");
    assert_eq!(output.status.code(), Some(1));
    let results: Value = serde_json::from_slice(&output.stdout).expect("parse stdout");
    let results = results.as_array().expect("a JSON array");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["errors"], serde_json::json!([]));
    assert!(!results[1]["errors"].as_array().unwrap().is_empty());

    let output_ok = houserules()
        .arg("validate")
        .arg(&good)
        .current_dir(&worktree.path)
        .output()
        .expect("run validate good");
    assert_eq!(output_ok.status.code(), Some(0));
}

/// Runs `houserules validate <args>` in `cwd` and returns the resulting
/// single result's own `file` field -- the three tests below all compare
/// two such calls against EACH OTHER rather than against an independently
/// hand-built expectation (CI round 2, issue 1's own lesson): a `file`
/// field this binary itself produced through `resolve_like_node` is the
/// only oracle guaranteed to agree with what the SAME function produces
/// for a different but equivalent input, on whatever this machine's OS
/// naturally does -- a hand-built expectation using `std::fs::
/// canonicalize` directly reintroduces exactly the class of platform
/// quirk (Windows' own 8.3-short-name expansion, distinct from and in
/// addition to the `\\?\` prefix `resolve_like_node`'s own doc covers)
/// that broke this suite once already.
fn validate_file_field(args: &[&str], cwd: &Path) -> String {
    let output = houserules()
        .arg("validate")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|error| panic!("run houserules validate {args:?} in {cwd:?}: {error}"));
    assert_eq!(
        output.status.code(),
        Some(0),
        "validate {args:?} in {cwd:?}: stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results: Value = serde_json::from_slice(&output.stdout).expect("parse stdout");
    results[0]["file"]
        .as_str()
        .unwrap_or_else(|| panic!("validate {args:?} in {cwd:?}: no file field in {results}"))
        .to_string()
}

/// tests/kb.test.mjs, describe('main (validate)'): "resolves a relative
/// path against the given cwd, not process.cwd()" -- the JS test calls
/// `main(['validate', 'rel.json'], io, dir)` directly, `dir` a plain
/// string never itself touched by a real `chdir`/`getcwd`; this port
/// instead spawns the compiled binary as a real, separate process (the
/// only way to exercise a CLI at all). Verifies the same property a
/// different way: an explicit absolute argument built from the identical
/// `sub` (resolved through `resolve_like_node`'s own absolute-path
/// branch, which never queries the current directory at all) must name
/// the same file as the relative argument resolved against `sub` as cwd
/// -- true only if the relative resolution actually used `sub`, not this
/// test's own unrelated process cwd.
#[test]
fn validate_resolves_a_relative_path_against_the_given_cwd() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let sub = worktree.path.join("sub");
    fs::create_dir(&sub).expect("mkdir sub");
    let source = repo_root().join("tests/corpus/fixtures/batch14-workspace/task-1-report.json");
    fs::copy(&source, sub.join("rel.json")).expect("copy fixture");

    let via_cwd = validate_file_field(&["rel.json"], &sub);
    let absolute = sub.join("rel.json");
    let via_absolute =
        validate_file_field(&[absolute.to_str().expect("utf8 path")], &worktree.path);
    assert_eq!(
        via_cwd, via_absolute,
        "a relative path must resolve against the given cwd (sub), not the test's own cwd"
    );
}

/// CI fix round 1, issue 1 (macOS: `validate_resolves_a_relative_path_
/// against_the_given_cwd` failed with "got /private/var/... vs expected
/// /var/..."): the dispatched diagnosis was that the binary wrongly
/// canonicalizes a symlinked cwd where the real JS CLI does not -- probed
/// live before trusting it (`tools/kb.sh validate` run through an
/// identically-symlinked cwd, real git worktree, real `PWD` set by the
/// shell's own `cd`), and found the opposite: `process.cwd()` resolves
/// the symlink away in JS's own real entry point too (it, like every
/// `getcwd(3)`-based query, is specified to; only a shell's own `$PWD` --
/// which the JS CLI's `main` never reads -- would preserve it, and this
/// binary reading it instead would make it diverge FROM parity, not
/// restore it). This test pins the verified truth for the binary instead:
/// visiting the real directory and visiting it through a symlink must
/// report the identical file -- true only if the symlinked cwd resolves
/// to the real one, matching real JS, not the symlink's own name.
/// CI round 2, issue 1: rewritten to compare two calls against each
/// other rather than against a `std::fs::canonicalize` built expectation,
/// which also broke Windows CI a second, unrelated way (its own 8.3-
/// short-name expansion, `validate_file_field`'s own doc has the reason).
#[test]
fn validate_resolves_a_symlinked_cwd_to_the_real_directory() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let real = worktree.path.join("real");
    fs::create_dir(&real).expect("mkdir real");
    let source = repo_root().join("tests/corpus/fixtures/batch14-workspace/task-1-report.json");
    fs::copy(&source, real.join("rel.json")).expect("copy fixture");
    let symlinked = worktree.path.join("via-symlink");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&real, &symlinked).expect("symlink real as via-symlink");
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(&real, &symlinked).expect("symlink real as via-symlink");

    let via_real = validate_file_field(&["rel.json"], &real);
    let via_symlink = validate_file_field(&["rel.json"], &symlinked);
    assert_eq!(
        via_real, via_symlink,
        "a symlinked cwd must resolve to the same real directory as visiting it directly, \
         matching the real JS CLI run through the same symlink"
    );
}

/// CI fix round 1, issue 1's other half: `std::path::absolute` keeps a
/// `..` component unresolved on POSIX by design (its own docs), where
/// Node's `path.resolve` always collapses it textually (verified live:
/// `path.resolve('/foo/../../baz')` is `/baz`). A relative path with a
/// `..` component used to echo back with the `..` still in it; now it
/// resolves the same way Node's own does -- visiting `sibling` through
/// `sub/../sibling` must report the identical file as visiting `sibling`
/// directly. CI round 2, issue 1: rewritten off `std::fs::canonicalize`
/// for the same reason as the two tests above.
#[test]
fn validate_collapses_a_relative_paths_dot_dot_components_like_node_does() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let sub = worktree.path.join("sub");
    let sibling = worktree.path.join("sibling");
    fs::create_dir(&sub).expect("mkdir sub");
    fs::create_dir(&sibling).expect("mkdir sibling");
    let source = repo_root().join("tests/corpus/fixtures/batch14-workspace/task-1-report.json");
    fs::copy(&source, sibling.join("rel.json")).expect("copy fixture");

    let via_dot_dot = validate_file_field(&["../sibling/rel.json"], &sub);
    let via_sibling = validate_file_field(&["rel.json"], &sibling);
    assert_eq!(
        via_dot_dot, via_sibling,
        "the .. must collapse to the same file as visiting sibling directly"
    );
}

// ---- fix round 1, issue 5: the ruled clap-argv deviation (spec §6), enumerated for
// audit/validate/stats the way T2 enumerated it for the backlog commands, then pinned
// for every divergence found -- not only a representative sample. Each test below names
// the JS's own answer for that exact argv shape (verified live against the frozen
// worktree) beside the clap answer it pins.

/// `audit --base HEAD --head` (bare, no value): JS's `parseArgs` coerces
/// the flag to `true`, so `typeof opts.head === 'string'` is false and
/// `headRef` defaults to `'HEAD'` -- the same as `--head` never having
/// been given at all. With `--base HEAD` too, base and head resolve to
/// the same commit (an empty, always-clean range), so JS exits 0; the
/// binary reports the missing value, exit 2.
#[test]
fn audit_bare_head_exits_2_where_js_coerced_it_to_the_default_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--head"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --head");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: a value is required for '--head <HEAD>' but none was supplied"),
        "got: {stderr:?}"
    );
}

/// `audit --base HEAD --ids` (bare, no value): JS's `typeof opts.ids ===
/// 'string' ? ... : []` gives an empty `ids` list, the same as omitting
/// `--ids` entirely, so JS exits 0 on this empty range; the binary reports
/// the missing value, exit 2.
#[test]
fn audit_bare_ids_exits_2_where_js_coerced_it_to_empty_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--ids"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --ids");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: a value is required for '--ids <IDS>' but none was supplied"),
        "got: {stderr:?}"
    );
}

/// `audit --base HEAD --report` (bare, no value): JS's `typeof opts.report
/// === 'string' ? ... : undefined` gives `report: undefined`, the same as
/// omitting `--report`, so JS exits 0 on this empty range; the binary
/// reports the missing value, exit 2.
#[test]
fn audit_bare_report_exits_2_where_js_coerced_it_to_undefined_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--report"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --report");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with(
            "error: a value is required for '--report <REPORT>' but none was supplied"
        ),
        "got: {stderr:?}"
    );
}

/// `stats <workspace> --bogus`: JS ignores the unrecognized flag and
/// stats the workspace normally, exit 0; the binary reports it as an
/// unexpected argument, exit 2.
#[test]
fn stats_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let dir = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .arg("stats")
        .arg(dir.path())
        .arg("--bogus")
        .current_dir(&worktree.path)
        .output()
        .expect("run stats --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// tests/kb.test.mjs, describe('main (audit, stats)'): the `stats needs
/// one dir` arm of "audit exits 1 on a failure, 0 when clean, 2 without
/// --base; stats needs one dir" -- `stats` with no workspace argument at
/// all. JS prints its own usage message; the binary's clap-native
/// "required argument" message differs in wording, but both exit 2 --
/// the ruled divergence class is JS *succeeding* where clap exits 2, which
/// this is not, so only the exit code is pinned here (fix round 1, issue
/// 5: this is the previously-lost vitest assertion, now its own test).
#[test]
fn stats_with_zero_positional_arguments_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .arg("stats")
        .current_dir(&worktree.path)
        .output()
        .expect("run stats");
    assert_eq!(output.status.code(), Some(2));
}

/// `stats a b` (two positionals): JS's own `positional.length !== 1`
/// guard also rejects this, exit 2 with the same "stats needs one
/// workspace directory" message as zero positionals; the binary's clap
/// surface takes a single positional, so a second one is an unexpected
/// argument, also exit 2 but with different wording -- pinned for the
/// same reason as the zero-positional case above.
#[test]
fn stats_with_two_positional_arguments_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["stats", "a", "b"])
        .current_dir(&worktree.path)
        .output()
        .expect("run stats a b");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: unexpected argument"),
        "got: {stderr:?}"
    );
}

// ---- fix round 2, finding 5 (task-3-review-r1.json): fix round 1's own enumeration missed
// four shapes the re-review's matrix over audit/validate/stats' full flag surface found --
// task-3-fix2-argv-enumeration.txt is that matrix, kept as a durable, checkable artifact
// rather than an unverifiable claim of completeness.

/// `audit --base HEAD --workspace` (bare, no value): JS's `typeof
/// opts.workspace === 'string' ? ... : undefined` gives `workspace:
/// undefined`, the same as omitting `--workspace` entirely, so JS exits 0
/// on this empty range; the binary reports the missing value, exit 2 --
/// the same bare-flag coercion class fix round 1 pinned for
/// --head/--ids/--report, missed for --workspace/--json.
#[test]
fn audit_bare_workspace_exits_2_where_js_coerced_it_to_undefined_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--workspace"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --workspace");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with(
            "error: a value is required for '--workspace <WORKSPACE>' but none was supplied"
        ),
        "got: {stderr:?}"
    );
}

/// `audit --base HEAD --json` (bare, no value): JS's `typeof opts.json ===
/// 'string' ? ... : undefined` gives `json: undefined`, the same as
/// omitting `--json` (no file written), so JS exits 0 on this empty
/// range; the binary reports the missing value, exit 2.
#[test]
fn audit_bare_json_exits_2_where_js_coerced_it_to_undefined_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--json"])
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --json");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: a value is required for '--json <JSON>' but none was supplied"),
        "got: {stderr:?}"
    );
}

/// `audit --base HEAD --json a --json b`: JS's own scalar-flag assignment
/// lets the second `--json` overwrite the first, so it exits 0 and writes
/// only `b` (`a` is never created); the binary reports the duplicate
/// flag, exit 2, writing neither file -- the same "last value wins" class
/// fix round 1 pinned for a duplicated `--base`, missed for `--json`.
#[test]
fn audit_with_a_duplicated_json_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let a = worktree.path.join("a.json");
    let b = worktree.path.join("b.json");
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--json"])
        .arg(&a)
        .arg("--json")
        .arg(&b)
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --base HEAD --json a --json b");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--json <JSON>' cannot be used multiple times"),
        "got: {stderr:?}"
    );
    assert!(!a.exists(), "a.json must not be written");
    assert!(!b.exists(), "b.json must not be written");
}

// ---- fix round 1, issue 5: the two `main(audit, stats)` cases the brief's one-for-one
// vitest port left with no cargo counterpart at all (covered only at the audit()-unit
// level in audit.rs's own suite, per fix round 1's own concern 4).

/// tests/kb.test.mjs, describe('main (audit, stats)'): "forwards
/// --workspace from the CLI to audit" -- a fresh knowledge base (the
/// `mini` fixture plus one report-field-checked entry, mirroring
/// `audit.rs`'s own `report_field_entry` unit fixture) so the CLI's own
/// `--workspace` plumbing, not just `audit()`'s, is exercised.
#[test]
fn forwards_workspace_from_the_cli_to_audit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    common::copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), root);

    let knowledge_path = root.join("knowledge/mini.json");
    let mut knowledge: Value =
        serde_json::from_str(&fs::read_to_string(&knowledge_path).unwrap()).unwrap();
    knowledge["entries"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "mini.reportws", "kind": "rule", "area": "global", "standing": false,
            "summary": "Every triggered report carries dependency_vetting.",
            "body": ["Fixture entry for fix round 1's --workspace CLI-dispatch test."],
            "tags": ["fixture"],
            "source": {"date": "2026-09-04", "by": "docs", "ref": "HR-054 fixture corpus"},
            "check": {
                "type": "report-field", "level": "fail", "if": "**/package.json",
                "field": "dependency_vetting",
            },
        }));
    fs::write(&knowledge_path, serde_json::to_string(&knowledge).unwrap()).unwrap();

    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&[
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
        "chore: base",
    ]);
    let base_output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .expect("run git rev-parse");
    let base_sha = String::from_utf8(base_output.stdout)
        .unwrap()
        .trim()
        .to_string();
    fs::create_dir_all(root.join("tools")).expect("mkdir tools");
    fs::write(root.join("tools/package.json"), "{}\n").expect("write package.json");
    git(&["add", "-A"]);
    git(&[
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
        "feat: add a dependency",
    ]);

    let workspace = tempfile::tempdir().expect("tempdir");
    fs::write(
        workspace.path().join("task-1-report.json"),
        serde_json::to_string(&serde_json::json!({
            "kind": "task-report", "files_changed": ["tools/package.json"],
            "dependency_vetting": {"manifests": ["tools/package.json"], "dependencies": []},
        }))
        .unwrap(),
    )
    .expect("write workspace report");

    let output = houserules()
        .args(["audit", "--base", &base_sha, "--workspace"])
        .arg(workspace.path())
        .current_dir(root)
        .output()
        .expect("run audit --workspace");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).expect("parse stdout");
    assert_eq!(result["rules"][0]["id"], serde_json::json!("mini.reportws"));
    assert_eq!(result["rules"][0]["result"], serde_json::json!("pass"));
    assert_eq!(
        result["rules"][0]["evidence"],
        serde_json::json!("report field dependency_vetting is set in 1 reports")
    );
}

/// tests/kb.test.mjs, describe('main (audit, stats)'): "reports a usage
/// error from the CLI when --report and --workspace are both given" --
/// the same pairing `rejects_report_together_with_workspace` proves at
/// the `audit()`-unit level, re-run through the actual CLI dispatch.
#[test]
fn reports_a_usage_error_from_the_cli_when_report_and_workspace_are_both_given() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let report_path = worktree.path.join("report.json");
    fs::write(&report_path, "{}").expect("write report.json");
    let workspace = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["audit", "--base", "HEAD", "--report"])
        .arg(&report_path)
        .arg("--workspace")
        .arg(workspace.path())
        .current_dir(&worktree.path)
        .output()
        .expect("run audit --report --workspace");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        output.stderr,
        b"audit takes --report or --workspace, not both\n"
    );
}
