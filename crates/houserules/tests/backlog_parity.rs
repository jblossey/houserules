//! Backlog command parity tests (batch 17 T2): the frozen fixture corpus's
//! five backlog slices (`list --open`, `get HR-052`, `batch 14`, `check`,
//! `set`), byte-compared verbatim against the compiled binary, plus the
//! CLI's usage-error and load-failure arms (`tools/backlog.mjs`'s `main`,
//! ported).

mod common;

use std::fs;
use std::path::Path;

use common::{FrozenWorktree, copy_dir_recursive, houserules, repo_root};
use serde_json::Value;

/// One frozen `tests/corpus/backlog/<slice>.json` capture: `stdout`,
/// `stderr`, and `exit`, read directly from the corpus file so a corpus
/// regeneration is the only way this test's expectation can drift.
struct CorpusRun {
    stdout: String,
    stderr: String,
    exit: i32,
}

fn corpus_run(relative: &str) -> CorpusRun {
    let text = fs::read_to_string(repo_root().join(format!("tests/corpus/backlog/{relative}")))
        .unwrap_or_else(|error| panic!("read tests/corpus/backlog/{relative}: {error}"));
    let value: serde_json::Value = serde_json::from_str(&text).expect("parse corpus backlog slice");
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

/// Parity gate, slice 1 of 4 against the frozen worktree: `list --open`.
#[test]
fn list_open_matches_the_frozen_corpus() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(&["list", "--open"], &worktree.path, "list-open.json");
}

/// Parity gate, slice 2 of 4: `get HR-052`.
#[test]
fn get_hr_052_matches_the_frozen_corpus() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(&["get", "HR-052"], &worktree.path, "get-hr-052.json");
}

/// Parity gate, slice 3 of 4: `batch 14`.
#[test]
fn batch_14_matches_the_frozen_corpus() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(&["batch", "14"], &worktree.path, "batch-14.json");
}

/// Parity gate, slice 4 of 4: `check-backlog` against a clean backlog --
/// `backlog: ok`, exit 0, matching `tools/corpus/backlog/check.json`
/// (recorded from `tools/backlog.mjs check`).
#[test]
fn check_backlog_matches_the_frozen_corpus() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    assert_run_matches(&["check-backlog"], &worktree.path, "check.json");
}

/// Parity gate, `set` slice: run on a fresh copy of the `mini` fixture (
/// never the live tree), then byte-compare both the captured stdout and
/// the WRITTEN FILE against the frozen corpus -- the write path is the
/// first Rust code that mutates a repository file (brief, "the set-slice
/// byte-compare is the gate"). The fixture's two items exercise both write
/// shapes at once: `HR-901`'s `status` updates an existing key in place
/// and its `batch` is a brand new trailing key; `HR-902` is untouched and
/// must still round-trip byte-for-byte.
#[test]
fn set_matches_the_frozen_corpus_stdout_and_written_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());

    let expected = corpus_run("set/mini/command.json");
    let output = houserules()
        .args(["set", "HR-901", "status=done", "batch=2", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run houserules set");
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        expected.stdout,
        "set: stdout diverged from the frozen corpus"
    );
    assert_eq!(output.stderr, b"", "set: no stderr on success");
    assert_eq!(output.status.code(), Some(expected.exit));

    let written = fs::read(dir.path().join("backlog/items/misc.json")).expect("read written file");
    let expected_bytes =
        fs::read(repo_root().join("tests/corpus/backlog/set/mini/backlog/items/misc.json"))
            .expect("read frozen written-file corpus");
    assert_eq!(
        written, expected_bytes,
        "set: written backlog/items/misc.json diverged from the frozen corpus byte-for-byte"
    );
}

/// `get` with no ids: `main`'s own usage error, checked after the backlog
/// has loaded -- `UsageError('get needs at least one id')`, exit 2.
#[test]
fn get_with_no_ids_prints_the_frozen_usage_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["get", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run get");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"get needs at least one id\n");
}

/// `get` with an unknown id: `cmdGet`'s `UsageError('unknown id "...")`,
/// exit 2.
#[test]
fn get_with_an_unknown_id_prints_the_frozen_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["get", "HR-999999", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run get");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "unknown id \"HR-999999\"\n"
    );
}

/// `batch` with no arguments: `main`'s `UsageError('batch needs one
/// number')`, exit 2.
#[test]
fn batch_with_no_arguments_prints_the_frozen_usage_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["batch", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run batch");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"batch needs one number\n");
}

/// `batch x`: `cmdBatch`'s `UsageError('batch needs a number')`, exit 2.
#[test]
fn batch_with_a_non_numeric_argument_prints_the_frozen_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["batch", "x", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run batch x");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"batch needs a number\n");
}

/// `batch 999999`: `cmdBatch`'s `UsageError('unknown batch "..."')`, exit 2.
#[test]
fn batch_with_an_unknown_number_prints_the_frozen_message_and_exits_2() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["batch", "999999", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run batch 999999");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "unknown batch \"999999\"\n"
    );
}

/// `set` with no arguments at all: `cmdSet`'s combined "needs <id> and at
/// least one field=value" usage error, exit 2.
#[test]
fn set_with_no_arguments_prints_the_frozen_usage_message_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    let output = houserules()
        .args(["set", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run set");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        output.stderr,
        b"set needs <id> and at least one field=value\n"
    );
}

/// `set` on an unknown item: `cmdSet`'s `UsageError('unknown item "...")`.
#[test]
fn set_on_an_unknown_item_prints_the_frozen_message_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    let output = houserules()
        .args(["set", "HR-404", "status=done", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run set HR-404");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "unknown item \"HR-404\"\n"
    );
}

/// `set` with a bad `key=value` pair: three shapes, each pinned to the
/// frozen JS's exact message -- an invalid status, a non-numeric batch, and
/// an unrecognized field name.
#[test]
fn set_with_a_bad_key_value_pair_prints_the_frozen_message_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());

    let output = houserules()
        .args(["set", "HR-901", "status=nope", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run set status=nope");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "status must be one of open, partial, done, dropped\n"
    );

    let output = houserules()
        .args(["set", "HR-901", "batch=x", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run set batch=x");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"batch must be a positive integer\n");

    let output = houserules()
        .args(["set", "HR-901", "title=x", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run set title=x");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "unknown field \"title\"\n"
    );
}

/// A missing `backlog/schema.json` is a load failure, one named line
/// naming the file, exit 2 -- `tools/backlog.mjs`'s `readJson` raising a
/// plain (uncaught) `Error`, per the CLI-failure-path deviation this whole
/// port follows (spec §6): a crash-class JS failure becomes a named
/// stderr line here, never a stack trace.
#[test]
fn a_missing_backlog_directory_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(stderr.contains("schema.json"), "got: {stderr:?}");
}

/// Invalid JSON in an items file: `check-backlog`'s (or any command's)
/// load fails the same way, naming the file -- `readJson`'s
/// `UsageError('${path}: invalid JSON (...)')`, ported as a load failure.
#[test]
fn invalid_json_in_an_items_file_prints_a_named_error_and_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    fs::write(dir.path().join("backlog/items/misc.json"), "{").expect("corrupt misc.json");

    let output = houserules()
        .args(["list", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run list");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "exactly one named error line");
    assert!(
        stderr.contains("misc.json: invalid JSON"),
        "got: {stderr:?}"
    );
}

/// Copies the `mini` fixture into a fresh scratch dir and `git init`s it,
/// so `--dir` (or the default git-root resolution) sees a self-contained
/// repository -- mirrors `tests/backlog.test.mjs`'s own `withMiniFixtureRepo`
/// (its module doc explains why a bare copy, with no `.git` of its own,
/// is not enough).
fn mini_copy() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir_recursive(&repo_root().join("tests/corpus/fixtures/mini"), dir.path());
    std::process::Command::new("git")
        .args(["init", "--quiet"])
        .arg(dir.path())
        .status()
        .expect("git init the mini copy");
    dir
}

/// Reads the JSON file at `path`, applies `edit`, and writes it back
/// pretty-printed -- the small helper every fixture-mutation test below
/// uses to seed one specific condition onto a fresh `mini` copy.
fn edit_json(path: &Path, edit: impl FnOnce(&mut Value)) {
    let mut value: Value =
        serde_json::from_str(&fs::read_to_string(path).expect("read fixture json"))
            .expect("parse fixture json");
    edit(&mut value);
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(&value).unwrap()),
    )
    .expect("write fixture json");
}

// ---- restored gate: check-backlog's warn/ok, exit-1, and schema-early-
// return arms (task-2-review.json, issue 1: the vitest port dropped these
// three `main`-describe assertions). Reconstructed, not mutation or
// natural: cli.rs's printing logic was already correct and unchanged by
// this fix round (verified live against both engines on fresh mini copies
// before writing each assertion below), and no mutation of it is captured
// here -- the gap this round closes is the missing test, not a behavior
// change.

/// `check-backlog` over a done item with no batch: the `warn: <text>` line
/// prints before `backlog: ok`, exit 0 -- `tools/backlog.mjs`'s `main`,
/// `'check'` case, the `for (const warning of warnings) io.out(...)` loop.
#[test]
fn check_backlog_prints_a_warning_then_ok_for_a_done_item_without_a_batch() {
    let dir = mini_copy();
    edit_json(&dir.path().join("backlog/items/misc.json"), |v| {
        v["items"][1].as_object_mut().unwrap().remove("batch");
    });
    let output = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        output.stdout,
        b"warn: backlog/items/misc.json HR-902: done without a batch\nbacklog: ok\n"
    );
    assert_eq!(output.stderr, b"");
}

/// `check-backlog` over a dangling batch item: the error prints on
/// stderr, exit 1, no stdout -- `tools/backlog.mjs`'s `main`, `'check'`
/// case's `io.err(...); return 1;` arm.
#[test]
fn check_backlog_prints_errors_on_stderr_and_exits_1_for_a_dangling_batch_item() {
    let dir = mini_copy();
    edit_json(&dir.path().join("backlog/batches.json"), |v| {
        v["batches"][0]["items"] = serde_json::json!(["HR-999"]);
    });
    let output = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        output.stderr,
        b"backlog/batches.json batch 1: item \"HR-999\" does not exist\n"
    );
}

/// `check-backlog` over an invalid item `type` plus an unknown field: both
/// schema findings print on stderr, exit 1, and NO stage-two finding
/// appears even though the same fixture also makes `HR-902`
/// done-without-a-batch -- `checkBacklog`'s `if (errors.any) return
/// { errors: errors.list, warnings }` early return, ported to
/// `check_backlog`'s own `if !errors.is_empty() { return (errors,
/// warnings); }`.
#[test]
fn check_backlog_prints_schema_errors_and_exits_1_before_stage_two_runs() {
    let dir = mini_copy();
    edit_json(&dir.path().join("backlog/items/misc.json"), |v| {
        let items = v["items"].as_array_mut().unwrap();
        items[0]["type"] = serde_json::json!("nope");
        items[0]["bogus_key"] = serde_json::json!(true);
        items[1].as_object_mut().unwrap().remove("batch");
    });
    let output = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"", "no warning: stage two never ran");
    assert_eq!(
        output.stderr,
        b"backlog/items/misc.json.items[0].type: must be one of \"feat\", \"nfr\", \"constraint\", \"decision\", \"process\", \"bug\", \"fix\", \"chore\", \"research\", \"question\", \"risk\", \"test\"\nbacklog/items/misc.json.items[0]: unknown field \"bogus_key\"\n"
    );
}

// ---- restored gate: `list`'s filters beyond `--open` (task-2-review.json
// assessment: confirmed matching the JS but never pinned by a CLI-level
// test). Reconstructed, same reasoning as the block above: list_items'
// filtering was already correct and unchanged by this fix round, and no
// mutation of it is captured here -- the gap this round closes is the
// missing CLI-level test, not a behavior change.

/// `list --status`/`--section`/`--type`/`--milestone`/`--batch` over the
/// `mini` fixture, each matching the frozen JS exactly (verified live
/// before writing these assertions).
#[test]
fn list_filters_beyond_open_match_the_frozen_corpus_on_the_mini_fixture() {
    let dir = mini_copy();
    let hr901 = serde_json::json!({
        "id": "HR-901", "status": "open", "milestone": null, "batch": null,
        "title": "Write the mini-tools smoke test",
    });
    let hr902 = serde_json::json!({
        "id": "HR-902", "status": "done", "milestone": null, "batch": 1,
        "title": "Ship the mini-tools smoke test",
    });

    for (args, expected) in [
        (vec!["list", "--status", "done"], vec![hr902.clone()]),
        (
            vec!["list", "--section", "misc"],
            vec![hr901.clone(), hr902.clone()],
        ),
        (vec!["list", "--type", "feat"], vec![hr902.clone()]),
        (
            vec!["list", "--milestone", "-"],
            vec![hr901.clone(), hr902.clone()],
        ),
        (vec!["list", "--batch", "1"], vec![hr902.clone()]),
    ] {
        let output = houserules()
            .args(&args)
            .args(["--dir"])
            .arg(dir.path())
            .output()
            .unwrap_or_else(|error| panic!("run houserules {args:?}: {error}"));
        assert_eq!(output.status.code(), Some(0), "{args:?}");
        let stdout: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{args:?}: invalid JSON stdout ({error}): {output:?}"));
        assert_eq!(stdout, Value::Array(expected), "{args:?}");
    }
}

// ---- ruled argv deviation (spec §6, dc6d8c6; task-2-review.json issue 2):
// the binary parses argv with clap, so a flag the frozen JS's `parseArgs`
// silently ignored, coerced to a bare `true`, or let a duplicate override
// is a named clap usage error at exit 2 instead. Each test below names the
// JS's own (success-path) answer, reproduced live against the frozen
// worktree before pinning the clap one. Reconstructed, not mutation or
// natural: both sides of the comparison are third-party behavior (clap's
// argument validation, `parseArgs`'s own ambiguity), not this crate's own
// logic, so no first-party mutation demonstrates the same divergence.

/// `list --bogus`: JS ignores the unrecognized flag and lists (exit 0);
/// the binary reports it as an unexpected argument, exit 2.
#[test]
fn list_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["list", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run list --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `list --status` with no value: JS treats the bare flag as `true` and
/// lists nothing (`opts.status` never matches a real status), exit 0; the
/// binary reports the missing value, exit 2.
#[test]
fn list_with_a_value_less_flag_exits_2_where_js_coerced_it_to_true() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["list", "--status", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run list --status");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with(
            "error: a value is required for '--status <STATUS>' but none was supplied"
        ),
        "got: {stderr:?}"
    );
}

/// `list --status open --status done`: JS's last `--status` value wins
/// (an object property assigned twice), exit 0; the binary refuses the
/// duplicate, exit 2.
#[test]
fn list_with_a_duplicated_flag_exits_2_where_js_let_the_last_value_win() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["list", "--status", "open", "--status", "done", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run list --status open --status done");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: the argument '--status <STATUS>' cannot be used multiple times"),
        "got: {stderr:?}"
    );
}

/// `list --open extra`: JS's `parseArgs` consumes `extra` as `--open`'s
/// own value (still truthy) and lists, exit 0; the binary reports `extra`
/// as an unexpected positional, exit 2.
#[test]
fn list_with_an_unexpected_positional_exits_2_where_js_consumed_it_as_a_flag_value() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["list", "--open", "extra", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run list --open extra");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument 'extra' found"),
        "got: {stderr:?}"
    );
}

/// An unknown command: JS prints its own generic usage line (`usage:
/// backlog <get|list|batch|set|check> [options]`), exit 2; the binary
/// reports clap's own unrecognized-subcommand message naming the flat
/// surface, exit 2 -- the usage-line TEXT is the one ruled, disclosed
/// exception among these six (spec §7's flat-command-surface exception),
/// so only the exit code is pinned, not clap's wording.
#[test]
fn an_unknown_command_exits_2_with_clap_s_own_usage_naming_the_flat_surface() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unrecognized subcommand 'bogus'"),
        "got: {stderr:?}"
    );
}

/// `batch 99999999999999999999` (past `i64::MAX`): JS's `Number()` parses
/// it as a lossy double and reports `unknown batch "..."` (never finding a
/// match); the binary's `i64` parse fails first and reports `batch needs a
/// number`, the same message a non-numeric argument gets.
#[test]
fn batch_past_i64_reports_needs_a_number_where_js_reported_unknown_batch() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["batch", "99999999999999999999", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run batch past i64::MAX");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stderr, b"batch needs a number\n");
}

/// `get --foo`: JS's `parseArgs` consumes `--foo` as an option, leaving
/// `positional` empty, so `main`'s own "needs at least one id" usage error
/// fires (`get needs at least one id`, exit 2, the same message and path
/// as `get` with no arguments at all); the binary reports `--foo` itself
/// as an unexpected argument, exit 2. The seventh and last of spec §6's
/// enumerated argv instances (task-2-review-r1.json, new breakage 3).
#[test]
fn get_with_an_unknown_flag_exits_2_where_js_reported_needs_at_least_one_id() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["get", "--foo", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run get --foo");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--foo' found"),
        "got: {stderr:?}"
    );
}

// ---- batch 17 T4 fix round 2, review new_breakage 1: `batch` and `set`
// had no argv-deviation pins at all; argv_closure.rs's own closure sentence
// claimed this file covered them.

/// `batch 14 --bogus`: JS ignores the unrecognized flag and prints the
/// batch record (exit 0); the binary reports it as an unexpected
/// argument, exit 2.
#[test]
fn batch_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["batch", "14", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run batch 14 --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `set HR-052 status=open --bogus`: JS ignores the unrecognized flag and
/// writes the change (exit 0); the binary reports it as an unexpected
/// argument, exit 2, before ever touching the file.
#[test]
fn set_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["set", "HR-052", "status=open", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run set HR-052 status=open --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

// ---- the ruled clap argv deviation (spec §6), for `check-backlog` --
// batch 17 T4 fix round 2: argv_closure.rs's own closure sentence claimed
// `check-backlog` (and `check-knowledge`) were exempt from this class
// entirely; probing live (`tools/backlog.mjs check --bogus`/`extra`) found
// the same JS-ignores-it-and-succeeds divergence every other flat command
// has -- the exemption was as false as the one review new_breakage 1
// already named for `render`/`batch`/`set`.

/// `check-backlog --bogus`: JS ignores the unrecognized flag and checks
/// (exit 0, `backlog: ok`); the binary reports it as an unexpected
/// argument, exit 2.
#[test]
fn check_backlog_with_an_unknown_flag_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["check-backlog", "--bogus", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run check-backlog --bogus");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument '--bogus' found"),
        "got: {stderr:?}"
    );
}

/// `check-backlog extra`: JS ignores the unexpected positional and checks
/// (exit 0); the binary reports it as an unexpected argument, exit 2.
#[test]
fn check_backlog_with_an_unexpected_positional_exits_2_where_js_ignored_it_and_succeeded() {
    let worktree = FrozenWorktree::checkout(&repo_root(), &read_frozen_sha());
    let output = houserules()
        .args(["check-backlog", "extra", "--dir"])
        .arg(&worktree.path)
        .output()
        .expect("run check-backlog extra");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.starts_with("error: unexpected argument 'extra' found"),
        "got: {stderr:?}"
    );
}
