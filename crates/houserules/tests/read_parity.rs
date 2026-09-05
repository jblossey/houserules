//! The knowledge read-command parity tests (batch 17 T4): the frozen
//! fixture corpus's `knowledge/` slices (`topics`/`index`/`standing`/`get`/
//! `for`, on both the frozen worktree and the `mini` fixture),
//! byte-compared verbatim against the compiled binary, plus the CLI's
//! usage-error arms these five commands add to the flat surface.

mod common;

use std::fs;
use std::path::Path;

use common::{FrozenWorktree, copy_dir_recursive, houserules, repo_root};

/// One frozen `tests/corpus/knowledge/<relative>` capture: `stdout`,
/// `stderr`, and `exit`, read directly from the corpus file so a corpus
/// regeneration is the only way this test's expectation can drift.
struct CorpusRun {
    stdout: String,
    stderr: String,
    exit: i32,
}

fn corpus_run(relative: &str) -> CorpusRun {
    let text = fs::read_to_string(repo_root().join(format!("tests/corpus/knowledge/{relative}")))
        .unwrap_or_else(|error| panic!("read tests/corpus/knowledge/{relative}: {error}"));
    let value: serde_json::Value =
        serde_json::from_str(&text).expect("parse corpus knowledge slice");
    CorpusRun {
        stdout: value["stdout"].as_str().expect("stdout").to_string(),
        stderr: value["stderr"].as_str().expect("stderr").to_string(),
        exit: value["exit"].as_i64().expect("exit") as i32,
    }
}

fn assert_run_matches(args: &[&str], dir: &Path, slice: &str) {
    let expected = corpus_run(slice);
    let output = houserules()
        .args(args)
        .args(["--dir"])
        .arg(dir)
        .output()
        .unwrap_or_else(|error| panic!("run houserules {args:?}: {error}"));
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

fn read_frozen_sha() -> String {
    let text = fs::read_to_string(repo_root().join("tests/corpus/manifest.json"))
        .expect("read corpus manifest");
    let manifest: serde_json::Value = serde_json::from_str(&text).expect("parse corpus manifest");
    manifest["frozen_sha"]
        .as_str()
        .expect("manifest.frozen_sha is a string")
        .to_string()
}

/// Parity gate, the frozen worktree ("the live tree" as of the corpus
/// freeze): `topics`, `index` (bare and `--standing`), `standing`, `get
/// houserules.template-is-the-source`, `for tools/kb.mjs` (bare and
/// `--full`).
#[test]
fn topics_matches_the_frozen_corpus_on_the_worktree() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(&["topics"], &worktree.path, "topics.json");
}

#[test]
fn index_matches_the_frozen_corpus_on_the_worktree() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(&["index"], &worktree.path, "index.json");
}

#[test]
fn index_standing_matches_the_frozen_corpus_on_the_worktree() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(
        &["index", "--standing"],
        &worktree.path,
        "index-standing.json",
    );
}

#[test]
fn standing_matches_the_frozen_corpus_on_the_worktree() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(&["standing"], &worktree.path, "standing.json");
}

#[test]
fn get_matches_the_frozen_corpus_on_the_worktree() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(
        &["get", "houserules.template-is-the-source"],
        &worktree.path,
        "get-houserules-template-is-the-source.json",
    );
}

#[test]
fn for_matches_the_frozen_corpus_on_the_worktree() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(
        &["for", "tools/kb.mjs"],
        &worktree.path,
        "for-tools-kb-mjs.json",
    );
}

#[test]
fn for_full_matches_the_frozen_corpus_on_the_worktree() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(
        &["for", "tools/kb.mjs", "--full"],
        &worktree.path,
        "for-tools-kb-mjs-full.json",
    );
}

/// Parity gate, the `mini` fixture -- the same commands, over a fresh copy
/// of the small synthetic knowledge base `tests/corpus/fixtures/mini`
/// commits.
fn mini_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    dir
}

#[test]
fn topics_matches_the_frozen_corpus_on_the_mini_fixture() {
    assert_run_matches(&["topics"], mini_copy().path(), "mini/topics.json");
}

#[test]
fn index_matches_the_frozen_corpus_on_the_mini_fixture() {
    assert_run_matches(&["index"], mini_copy().path(), "mini/index.json");
}

#[test]
fn index_standing_matches_the_frozen_corpus_on_the_mini_fixture() {
    assert_run_matches(
        &["index", "--standing"],
        mini_copy().path(),
        "mini/index-standing.json",
    );
}

#[test]
fn standing_matches_the_frozen_corpus_on_the_mini_fixture() {
    assert_run_matches(&["standing"], mini_copy().path(), "mini/standing.json");
}

#[test]
fn get_matches_the_frozen_corpus_on_the_mini_fixture() {
    assert_run_matches(
        &["get", "mini.build-cache"],
        mini_copy().path(),
        "mini/get-mini-build-cache.json",
    );
}

#[test]
fn for_matches_the_frozen_corpus_on_the_mini_fixture() {
    assert_run_matches(
        &["for", "mini-tools/build.sh"],
        mini_copy().path(),
        "mini/for-mini-tools-build-sh.json",
    );
}

#[test]
fn for_full_matches_the_frozen_corpus_on_the_mini_fixture() {
    assert_run_matches(
        &["for", "mini-tools/build.sh", "--full"],
        mini_copy().path(),
        "mini/for-mini-tools-build-sh-full.json",
    );
}

// ---- usage errors and load-failure propagation, the five commands' own
// contribution to the flat surface (`get`'s own usage-error arms are
// pinned in backlog_parity.rs already -- one shared dispatcher now serves
// both id shapes, so nothing here repeats them).

/// `get` with no ids, in a repository holding NEITHER `knowledge/` nor
/// `backlog/`: the distinguishing input for `get`'s own arity-first
/// ordering (docs/specs/2026-09-04-batch-15-tier2-spec.md §3, ruled at the
/// batch 17 T4 review, batch 17 fix round 1, review issue 6). Both frozen
/// scripts load their sole domain unconditionally before their own arity
/// check, so each throws an uncaught `ENOENT` stack trace here (`node`'s
/// own exit 1 for an uncaught exception, verified live: `tools/kb.sh get`
/// and `tools/backlog.sh get` both fail this way, naming their own
/// `<domain>/schema.json`); `get`'s own domain depends on the ids given,
/// so with none it cannot pick one to load at all, and checks arity first
/// instead -- `get needs at least one id`, exit 2, needing neither
/// directory to exist.
#[test]
fn get_with_no_ids_in_a_repository_with_neither_domain_prints_the_usage_message_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .arg(dir.path())
        .status()
        .expect("git init");
    let output = houserules()
        .args(["get", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run get");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"get needs at least one id\n");
}

/// `for` with no paths: `main`'s own "needs at least one path" usage error,
/// checked after the base has loaded -- `UsageError('for needs at least
/// one path')`, exit 2.
#[test]
fn for_with_no_paths_prints_the_frozen_usage_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["for", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run for");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"for needs at least one path\n");
}

/// A missing `knowledge/schema.json` propagates as a named, one-line
/// error, exit 2 -- the same load-failure path `render`/`check-knowledge`
/// already pin, exercised here through `topics` (any of the five read
/// commands shares the identical `load_base` call).
#[test]
fn a_missing_knowledge_directory_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["topics", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run topics");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(stderr.contains("schema.json"), "got: {stderr:?}");
}

/// Batch 17 T4 fix round 1, review issue 4: invalid JSON in a knowledge
/// file is a named, one-line error, exit 2 -- kb.test.mjs's own dropped
/// `describe('main (read commands)')` case ("reports invalid JSON in a
/// knowledge file as a usage error, not a stack trace") had no Rust
/// replacement until now; `load_base` propagates the same
/// `LoadError::Json` for every read command, exercised here through
/// `topics` like the missing-`schema.json` case above.
#[test]
fn invalid_json_in_a_knowledge_file_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    fs::write(dir.path().join("knowledge/mini.json"), "{").expect("corrupt mini.json");

    let output = houserules()
        .args(["topics", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run topics");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(
        stderr.contains("mini.json: invalid JSON"),
        "got: {stderr:?}"
    );
}

// ---- the ruled clap argv deviation (spec §6), for the four commands this
// task adds -- the same pattern backlog_parity.rs's and
// validate_stats_audit_parity.rs's own such tests use. Batch 17 T4 fix
// round 1, review issue 1: these nine shapes were the ones argv_closure.rs's
// own module doc claimed were already pinned here; they were not, until now.

/// `index --bogus`: JS ignores the unrecognized flag and lists (exit 0);
/// the binary reports it as an unexpected argument, exit 2.
#[test]
fn index_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["index", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run index --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `topics --bogus`: JS ignores the unrecognized flag and lists (exit 0);
/// the binary reports it as an unexpected argument, exit 2.
#[test]
fn topics_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["topics", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run topics --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `standing --bogus`: JS ignores the unrecognized flag and lists (exit 0);
/// the binary reports it as an unexpected argument, exit 2.
#[test]
fn standing_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["standing", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run standing --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `for tools/kb.mjs --bogus`: JS ignores the unrecognized flag and prints
/// the rule package (exit 0); the binary reports it as an unexpected
/// argument, exit 2.
#[test]
fn for_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["for", "tools/kb.mjs", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run for tools/kb.mjs --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `topics extra`: JS ignores the unexpected positional and lists (exit
/// 0); the binary reports it as an unexpected argument, exit 2.
#[test]
fn topics_with_an_unexpected_positional_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["topics", "extra", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run topics extra");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument 'extra' found"),
        "got: {stderr:?}"
    );
}

/// `standing extra`: JS ignores the unexpected positional and lists (exit
/// 0); the binary reports it as an unexpected argument, exit 2.
#[test]
fn standing_with_an_unexpected_positional_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["standing", "extra", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run standing extra");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument 'extra' found"),
        "got: {stderr:?}"
    );
}

/// `index extra`: JS ignores the unexpected positional and lists (exit 0);
/// the binary reports it as an unexpected argument, exit 2.
#[test]
fn index_with_an_unexpected_positional_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["index", "extra", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run index extra");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument 'extra' found"),
        "got: {stderr:?}"
    );
}

/// `index --area process --area global`: JS's last value wins (an object
/// property assigned twice), so `index` filters by `global` alone, exit 0;
/// the binary refuses the duplicate, exit 2.
#[test]
fn index_with_a_duplicated_area_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["index", "--area", "process", "--area", "global", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run index --area process --area global");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: the argument '--area <AREA>' cannot be used multiple times"),
        "got: {stderr:?}"
    );
}

/// `for tools/kb.mjs --full --full`: JS's `parseArgs` sets the same
/// `opts.full = true` twice, still `true`, exit 0; the binary refuses the
/// duplicate, exit 2.
#[test]
fn for_with_a_duplicated_full_flag_exits_2_where_js_let_the_repeat_be_a_no_op() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["for", "tools/kb.mjs", "--full", "--full", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run for tools/kb.mjs --full --full");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: the argument '--full' cannot be used multiple times"),
        "got: {stderr:?}"
    );
}

// ---- HR-056: the bare-invocation pin.

/// Bare `houserules`, no subcommand and no flag: `arg_required_else_help`
/// prints clap's own help to stderr and exits 2 -- matching the frozen
/// `tools/kb.sh`/`tools/backlog.sh` contract (batch 16 branch review, issue
/// 4: both print a usage line and fail with no command), not clap's
/// all-`Option` default of a silent, empty success.
#[test]
fn bare_invocation_prints_help_on_stderr_and_exits_2() {
    let output = houserules().output().expect("run houserules bare");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("The `houserules` command line"),
        "got: {stderr:?}"
    );
    assert!(
        stderr.contains("Usage: houserules [COMMAND]"),
        "got: {stderr:?}"
    );
}
