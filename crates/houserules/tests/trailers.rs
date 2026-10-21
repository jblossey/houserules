//! Trailers, deliverable deferrals, and the every-commit check (batch 34,
//! spec sections 3.4 and 3.7): `audit` resolves each `Work-Item:` and
//! `Deferred:` trailer of its range, and each `issues[].backlog` id of the
//! reviews it reads, through the installed tracker; a `commits` check with
//! `require_trailer`, `exempt_authors`, and `exempt_subjects` fails a commit
//! that names no work item, in `check-commit` and in the commit-msg hook's
//! message-file mode. The check runs here as an entry of the project's own,
//! with the fields of the process pack's every-commit entry. Each case runs
//! the compiled binary against a scratch git repository with the JSON
//! tracker. The end-to-end case of a pull-request trailer needs the GitHub
//! adapter and lives with it; the resolution of that line is a unit test in
//! `rules/trailers.rs`.
//!
//! Tests that assert behavior this batch adds fail at the commit before it
//! (the RED of design.md 173(2)). A test that asserts behavior that did not
//! change says so in its doc comment: it is a pin of unchanged behavior.

mod tracker_support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    AUDIT_LINE_ZERO, JSON_UNREACHABLE, LINE_B, assert_output, commit_all, git,
    install_with_backlog_dir, install_without_backlog, read_json, run, run_ok, scratch_git_repo,
    stderr, stdout, write_json,
};

/// The id of the project's own entry that holds the every-commit check.
const ENTRY: &str = "team.every-commit-names-a-work-item";

/// The audit line of an install with no tracker and `n` references.
fn audit_line(n: usize) -> String {
    format!("skipped {n} work-item references: no tracker installed (houserules init --tracker)")
}

/// The line for a `Work-Item` trailer that the tracker does not hold.
fn work_item_unresolved(sha: &str, id: &str) -> String {
    format!("{sha}: Work-Item {id} does not resolve in the json tracker")
}

/// The line for a `Deferred` trailer that names no filed deferral.
fn deferred_unfiled(sha: &str, id: &str) -> String {
    format!("{sha}: Deferred {id} is not a filed deferral with its reason in the json tracker")
}

/// The line for a review issue that names no filed deferral.
fn issue_unfiled(file: &str, id: &str) -> String {
    format!("{file}: {id} is not a filed deferral with its reason in the json tracker")
}

/// Joins `lines` the way a command prints them: one per line, each ended.
fn printed(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// Removes the process pack from the install at `dir` when it holds one, so
/// that the commits of a test meet no entry of it. An install without the
/// pack is left as it is.
fn without_the_process_pack(dir: &Path) {
    let _ = run(dir, &["pack", "remove", "process"]);
}

/// A scratch install with the JSON tracker, two work items, and a base
/// commit: `WI-001` is a plain open item, `WI-002` a filed deferral with its
/// reason. The install holds no process pack, so no entry of the pack judges
/// the commits of a test. Returns the directory and the base commit for
/// `audit --base`.
fn json_install() -> (TempDir, String) {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    without_the_process_pack(dir.path());
    let filed = run_ok(
        dir.path(),
        &[
            "defer",
            "--title",
            "Cache the lookups",
            "--reason",
            "Needs a design.",
            "--origin",
            "task 4 review",
        ],
    );
    assert_eq!(stdout(&filed), "WI-002\n", "the deferral takes WI-002");
    let base = commit_all(dir.path(), "chore: install the kit");
    (dir, base)
}

/// Makes an empty commit by `author` with `subject`, and `trailers` as the
/// last paragraph when there are any. Returns the short sha. An empty commit
/// changes no path, so a `--report` audit of the range triggers no
/// `report-field` check.
fn commit_as(dir: &Path, author: &str, subject: &str, trailers: &[&str]) -> String {
    let mut args = vec![
        "commit".to_string(),
        "-q".to_string(),
        "--no-verify".to_string(),
        "--allow-empty".to_string(),
        "--author".to_string(),
        format!("{author} <author@example.com>"),
        "-m".to_string(),
        subject.to_string(),
    ];
    if !trailers.is_empty() {
        args.push("-m".to_string());
        args.push(trailers.join("\n"));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    git(dir, &refs);
    short_head(dir)
}

/// The short sha of `HEAD`.
fn short_head(dir: &Path) -> String {
    git(dir, &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_string()
}

/// An empty human commit with `subject` and `trailers`.
fn commit_with(dir: &Path, subject: &str, trailers: &[&str]) -> String {
    commit_as(dir, "A Human", subject, trailers)
}

/// Runs `audit --base <base>` with `extra` arguments.
fn audit(dir: &Path, base: &str, extra: &[&str]) -> std::process::Output {
    let mut args = vec!["audit", "--base", base];
    args.extend_from_slice(extra);
    run(dir, &args)
}

/// Pin of unchanged behavior: an audit of a range with no trailer prints
/// nothing on stderr and exits 0 on an install with a reachable tracker.
#[test]
fn audit_of_a_range_with_no_trailer_prints_no_line() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &[]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

/// Spec 3.7: trailers that resolve pass: a work item, an aggregated commit
/// with two `Work-Item:` trailers, and a `Deferred:` trailer that names a
/// filed deferral with a reason. Pin of unchanged behavior at the commit
/// before this batch, which read no trailer; the test guards the verdict.
#[test]
fn audit_passes_trailers_that_resolve() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-001"]);
    commit_with(
        dir.path(),
        "feat: aggregate two items",
        &["Work-Item: WI-001", "Work-Item: WI-002"],
    );
    commit_with(
        dir.path(),
        "chore: park the cache",
        &["Deferred: WI-002 The cache needs a design."],
    );
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

/// Spec 3.7: a `Work-Item:` trailer that the tracker does not hold prints
/// the not-resolve line with the commit's short sha and the adapter, and
/// the audit exits 1. The audit's own JSON stays on stdout.
#[test]
fn audit_names_a_work_item_that_does_not_resolve() {
    let (dir, base) = json_install();
    let sha = commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-099"]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[work_item_unresolved(&sha, "WI-099")])
    );
    assert_eq!(output.status.code(), Some(1));
    serde_json::from_str::<Value>(&stdout(&output)).expect("stdout is the audit JSON");
}

/// Spec 3.7: a commit may carry several `Work-Item:` trailers, and the audit
/// resolves each: one line for each that fails, in the order of the message.
#[test]
fn audit_resolves_each_of_several_work_item_trailers_on_one_commit() {
    let (dir, base) = json_install();
    let sha = commit_with(
        dir.path(),
        "feat: aggregate three items",
        &[
            "Work-Item: WI-098",
            "Work-Item: WI-001",
            "Work-Item: WI-099",
        ],
    );
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[
            work_item_unresolved(&sha, "WI-098"),
            work_item_unresolved(&sha, "WI-099"),
        ])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Spec 3.7: the audit resolves every trailer of every commit of its range.
/// The lines come in the order of `git log`, the order `check-commit`
/// prints, newest commit first.
#[test]
fn audit_prints_the_lines_of_a_range_newest_commit_first() {
    let (dir, base) = json_install();
    let first = commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-097"]);
    commit_with(dir.path(), "feat: add the cache", &["Work-Item: WI-001"]);
    let third = commit_with(dir.path(), "feat: add the index", &["Work-Item: WI-099"]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[
            work_item_unresolved(&third, "WI-099"),
            work_item_unresolved(&first, "WI-097"),
        ])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Spec 3.7: a `Deferred:` trailer passes only when the item carries the
/// deferral marker with a reason. `WI-001` exists and carries no marker.
#[test]
fn audit_names_a_deferred_trailer_whose_item_is_no_filed_deferral() {
    let (dir, base) = json_install();
    let sha = commit_with(
        dir.path(),
        "chore: park the seed item",
        &["Deferred: WI-001 Waits for the design."],
    );
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[deferred_unfiled(&sha, "WI-001")])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Spec 3.7: a `Deferred:` trailer for an id that the tracker does not hold
/// is no filed deferral either.
#[test]
fn audit_names_a_deferred_trailer_for_an_unknown_item() {
    let (dir, base) = json_install();
    let sha = commit_with(
        dir.path(),
        "chore: park the cache",
        &["Deferred: WI-099 Waits for the design."],
    );
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[deferred_unfiled(&sha, "WI-099")])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Spec 3.7: a `Deferred:` trailer with no reason fails with its own line,
/// even when the item is a filed deferral.
#[test]
fn audit_names_a_deferred_trailer_with_no_reason() {
    let (dir, base) = json_install();
    let sha = commit_with(dir.path(), "chore: park the cache", &["Deferred: WI-002"]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[format!("{sha}: Deferred WI-002 names no reason")])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Spec 3.7: the two texts of a `Deferred:` trailer are not compared. The
/// reason of the trailer differs from the reason of the item and passes. It
/// passes at the commit before this batch, which reads no trailer: a guard
/// against a verdict that compares the texts.
#[test]
fn audit_does_not_compare_the_reason_of_a_deferred_trailer_with_the_item() {
    let (dir, base) = json_install();
    commit_with(
        dir.path(),
        "chore: park the cache",
        &["Deferred: WI-002 A reason in other words."],
    );
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

/// Spec 3.7: only the last paragraph of a message holds trailers, by git's
/// own rule. A `Work-Item:` line in an earlier paragraph is prose. It passes
/// at the commit before this batch, which reads no trailer: a guard against
/// a reader that scans the whole body.
#[test]
fn audit_reads_only_the_last_paragraph_for_trailers() {
    let (dir, base) = json_install();
    git(
        dir.path(),
        &[
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            "feat: add the lookup",
            "-m",
            "Work-Item: WI-099",
            "-m",
            "A closing paragraph with no trailer.",
        ],
    );
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

/// Spec 3.7: git's trailer rules take the key without regard to its case,
/// so `work-item:` names a work item as `Work-Item:` does.
#[test]
fn audit_reads_the_trailer_key_without_regard_to_case() {
    let (dir, base) = json_install();
    let sha = commit_with(dir.path(), "feat: add the lookup", &["work-item: WI-099"]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[work_item_unresolved(&sha, "WI-099")])
    );
}

/// Review focus 2: a trailer block with CRLF line endings and trailing
/// spaces and tabs, as a Windows commit writes it, parses as the same
/// trailers: each id is the bare id, so the ones that exist resolve and the
/// one that does not is named without the whitespace.
#[test]
fn trailers_parse_crlf_and_trailing_space() {
    let (dir, base) = json_install();
    let messages = tempfile::tempdir().expect("tempdir");
    let message = messages.path().join("message.txt");
    fs::write(
        &message,
        "feat: add the lookup\r\n\r\nA body line.\r\n\r\nWork-Item: WI-001  \r\nWork-Item: WI-002\t\r\nWork-Item: WI-099 \r\n",
    )
    .expect("write the message");
    git(
        dir.path(),
        &[
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "--cleanup=verbatim",
            "-F",
            message.to_str().expect("utf-8 path"),
        ],
    );
    let sha = short_head(dir.path());
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[work_item_unresolved(&sha, "WI-099")])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Makes git of the repository `dir` recognize `separators` as the trailer
/// separators (`trailer.separators`), as a team that writes `Fix #12` does.
fn with_repository_trailer_separators(dir: &Path, separators: &str) {
    git(dir, &["config", "trailer.separators", separators]);
}

/// Spec 3.7 (C32): trailers parse under git's default separator `:`, whatever
/// the repository's `trailer.separators`. Under `#:` git prints the trailer
/// `Work-Item: WI-099` as `Work-Item# WI-099`, and the audit must still read
/// it: the same range exits 1 with the not-resolve line, as it does under no
/// configuration.
#[test]
fn audit_reads_trailers_under_a_repository_trailer_separators() {
    let (dir, base) = json_install();
    with_repository_trailer_separators(dir.path(), "#:");
    let sha = commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-099"]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[work_item_unresolved(&sha, "WI-099")])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Spec 3.7 (C32): a line with another separator is no trailer, whatever the
/// repository's `trailer.separators`: `Work-Item# WI-099` names nothing, so
/// the audit has no reference to resolve and exits 0. It passes before the
/// fix, because the old reader dropped the line: it guards the fix against a
/// reader that takes the separator of the repository.
#[test]
fn audit_reads_no_trailer_that_uses_another_separator() {
    let (dir, base) = json_install();
    with_repository_trailer_separators(dir.path(), "#:");
    commit_with(dir.path(), "feat: add the lookup", &["Work-Item# WI-099"]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

/// Spec 3.7: an id of no declared shape fails with the not-resolve line and
/// reaches no backend. The backlog is gone, so any call to the tracker would
/// print the unreachable line instead; the shape-less ids print only the
/// not-resolve lines. An id of the right shape does reach the tracker, and
/// the unreachable tracker is a gate failure, exit 1.
#[test]
fn audit_gives_an_id_of_no_declared_shape_the_not_resolve_line_and_no_backend_call() {
    let (dir, base) = json_install();
    let sha = commit_with(
        dir.path(),
        "feat: add the lookup",
        &["Work-Item: --help", "Deferred: -x Waits for the design."],
    );
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(
        stderr(&output),
        printed(&[
            work_item_unresolved(&sha, "--help"),
            deferred_unfiled(&sha, "-x"),
        ])
    );
    assert_eq!(output.status.code(), Some(1));

    commit_with(dir.path(), "feat: add the cache", &["Work-Item: WI-001"]);
    let reached = audit(dir.path(), &base, &[]);
    assert_eq!(stderr(&reached), format!("{JSON_UNREACHABLE}\n"));
    assert_eq!(reached.status.code(), Some(1));
}

/// The `--help` of `command` with its whitespace folded to single spaces, so
/// that the wrapping of the help text decides nothing.
fn help_of(command: &str) -> String {
    let output = tracker_support::houserules()
        .args([command, "--help"])
        .output()
        .expect("run --help");
    assert_eq!(output.status.code(), Some(0), "{command} --help exits 0");
    stdout(&output)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `quality.user-facing-text-is-contract`: the help of `audit` names every
/// input that it reads and every reason that it exits 1. It reads the
/// trailers of the range and the `issues[].backlog` ids of the files of
/// `--report` and `--workspace`, which holds reports and reviews.
#[test]
fn audit_help_names_the_references_it_resolves_and_the_files_it_reads() {
    let help = help_of("audit");
    for phrase in [
        "resolves each `Work-Item:` and `Deferred:` trailer of the range's commits",
        "each `issues[].backlog` id of the deliverables it reads (`--report` and `--workspace`)",
        "through the installed tracker",
        "prints one line on stderr for each reference that fails",
        "exits 1 when one does, when a member's check fails, or when the tracker cannot be reached",
        "With no tracker it resolves nothing",
        "The audit also resolves the `issues[].backlog` ids of the file.",
        "A directory of `task-*-report.json` and `task-*-review*.json` files.",
        "the audit resolves the `issues[].backlog` ids of every file",
    ] {
        assert!(
            help.contains(phrase),
            "audit --help lacks {phrase:?}: {help}"
        );
    }
}

/// `quality.user-facing-text-is-contract`: the help of `check-commit` names
/// the trailer rule, the one input that the message-file mode lacks, and the
/// exit 1.
#[test]
fn check_commit_help_names_the_trailer_rule_and_its_exit() {
    let help = help_of("check-commit");
    for phrase in [
        "A `commits` check with `require_trailer` also reads the trailers of each message",
        "fails a message that has none of that key",
        "For a message file only `exempt_subjects` applies, since no author exists yet.",
        "It exits 1 when a `fail`-level check finds a violation",
    ] {
        assert!(
            help.contains(phrase),
            "check-commit --help lacks {phrase:?}: {help}"
        );
    }
}

/// Writes a review with `issues` that name `ids` as their backlog items, to
/// `name` below `dir`.
fn write_review(dir: &Path, name: &str, ids: &[&str]) {
    let issues: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({
                "severity": "minor",
                "file": "src/lib.rs",
                "what": "A finding.",
                "why": "A reason.",
                "fix": "A fix.",
                "backlog": id,
            })
        })
        .collect();
    write_json(dir, name, &json!({"kind": "task-review", "issues": issues}));
}

/// Spec 3.7 (F13): `audit --report` resolves each `issues[].backlog` id of
/// the report: a filed deferral with its reason passes, an item with no
/// marker and an unknown item each print the file, the id, and the adapter.
#[test]
fn audit_report_resolves_each_backlog_id_of_the_issues() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &[]);
    let reviews = tempfile::tempdir().expect("tempdir");
    write_review(
        reviews.path(),
        "task-1-review.json",
        &["WI-002", "WI-001", "WI-099"],
    );
    let file = reviews.path().join("task-1-review.json");
    let shown = file.display().to_string();
    let output = audit(dir.path(), &base, &["--report", &shown]);
    assert_eq!(
        stderr(&output),
        printed(&[
            issue_unfiled(&shown, "WI-001"),
            issue_unfiled(&shown, "WI-099")
        ])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// Spec 3.7 (F13): with the ids fixed, the same audit exits 0. It passes at
/// the commit before this batch, which reads no review: a guard against a
/// reader that fails a filed deferral.
#[test]
fn audit_report_passes_when_every_backlog_id_is_a_filed_deferral() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &[]);
    let reviews = tempfile::tempdir().expect("tempdir");
    write_review(reviews.path(), "task-1-review.json", &["WI-002"]);
    let file = reviews.path().join("task-1-review.json");
    let output = audit(
        dir.path(),
        &base,
        &["--report", file.to_str().expect("utf-8 path")],
    );
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
}

/// Spec 3.7 (F13): `audit --workspace` resolves the ids of every review it
/// reads, and names the file by its name in the workspace.
#[test]
fn audit_workspace_resolves_each_backlog_id_of_every_review() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &[]);
    let workspace = tempfile::tempdir().expect("tempdir");
    write_review(workspace.path(), "task-1-review.json", &["WI-002"]);
    write_review(workspace.path(), "task-2-review.json", &["WI-099"]);
    let shown = workspace.path().display().to_string();
    let output = audit(dir.path(), &base, &["--workspace", &shown]);
    assert_eq!(
        stderr(&output),
        printed(&[issue_unfiled("task-2-review.json", "WI-099")])
    );
    assert_eq!(output.status.code(), Some(1));
}

/// An install with no tracker, no `backlog/` directory, and no process pack.
fn install_with_no_tracker() -> TempDir {
    let dir = install_without_backlog();
    without_the_process_pack(dir.path());
    dir
}

/// Spec 3.4 and 3.9: with no tracker the audit resolves nothing and counts
/// every reference it read: each `Work-Item:` and `Deferred:` trailer of the
/// range and each `issues[].backlog` id of the review. The count is on
/// stderr, and the exit code is the other checks'.
#[test]
fn audit_without_a_tracker_counts_every_reference_it_skipped() {
    let dir = install_with_no_tracker();
    let base = commit_all(dir.path(), "chore: install the kit");
    commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-001"]);
    commit_with(
        dir.path(),
        "feat: aggregate two items",
        &["Work-Item: WI-002", "Work-Item: WI-003"],
    );
    commit_with(
        dir.path(),
        "chore: park the cache",
        &["Deferred: WI-004 Needs a design."],
    );
    let reviews = tempfile::tempdir().expect("tempdir");
    write_review(reviews.path(), "task-1-review.json", &["WI-005", "WI-006"]);
    let file = reviews.path().join("task-1-review.json");
    let output = audit(
        dir.path(),
        &base,
        &["--report", file.to_str().expect("utf-8 path")],
    );
    assert_eq!(stderr(&output), format!("{}\n", audit_line(6)));
    assert_eq!(output.status.code(), Some(0));
}

/// Pin of unchanged behavior: with no tracker and no reference the count is
/// zero and the line still prints.
#[test]
fn audit_without_a_tracker_and_without_a_reference_counts_zero() {
    let dir = install_with_no_tracker();
    let base = commit_all(dir.path(), "chore: install the kit");
    commit_with(dir.path(), "feat: add the lookup", &[]);
    let output = audit(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), format!("{AUDIT_LINE_ZERO}\n"));
    assert_eq!(output.status.code(), Some(0));
}

// ---- the verdict on the references, in the audit's JSON ----

/// The line for a `Deferred` trailer that names no reason.
fn deferred_without_reason(sha: &str, id: &str) -> String {
    format!("{sha}: Deferred {id} names no reason")
}

/// Runs `audit --base <base> --json <file>` with `extra` arguments. Returns
/// the output and the audit result that stdout printed. The `--json` file
/// must hold the same value.
fn audit_result(dir: &Path, base: &str, extra: &[&str]) -> (std::process::Output, Value) {
    let target = tempfile::tempdir().expect("tempdir");
    let file = target.path().join("audit.json");
    let mut args = vec!["--json", file.to_str().expect("utf-8 path")];
    args.extend_from_slice(extra);
    let output = audit(dir, base, &args);
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("stdout is the audit JSON");
    let written: Value =
        serde_json::from_str(&fs::read_to_string(&file).expect("the audit writes the --json file"))
            .expect("the --json file is JSON");
    assert_eq!(
        written, printed,
        "the --json file holds the result of stdout"
    );
    (output, printed)
}

/// Asserts the verdict that `result` holds on the references: the whole
/// `references` object, and the failures that the summary counts. The rule
/// rows decide nothing here, so `summary.fail` is 0.
fn assert_verdict(result: &Value, references: &Value, reference_fail: usize) {
    assert_eq!(&result["references"], references, "the references object");
    assert_eq!(result["summary"]["reference_fail"], json!(reference_fail));
    assert_eq!(result["summary"]["fail"], json!(0), "no rule row fails");
}

/// The audit's JSON holds the verdict that stderr and the exit code carry:
/// a failing `Work-Item:` trailer and two failing `Deferred:` trailers show
/// in `references.failures`, newest commit first, and the references that
/// the tracker judged count as resolved. The stderr lines and the exit code
/// stay as they were.
#[test]
fn audit_json_holds_each_failing_trailer_and_counts_the_references() {
    let (dir, base) = json_install();
    let first = commit_with(
        dir.path(),
        "feat: add the lookup",
        &["Work-Item: WI-099", "Work-Item: WI-001"],
    );
    let second = commit_with(
        dir.path(),
        "chore: park the cache",
        &["Deferred: WI-001 Needs a design.", "Deferred: WI-002"],
    );
    let lines = [
        deferred_unfiled(&second, "WI-001"),
        deferred_without_reason(&second, "WI-002"),
        work_item_unresolved(&first, "WI-099"),
    ];
    let (output, result) = audit_result(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), printed(&lines));
    assert_eq!(output.status.code(), Some(1));
    assert_verdict(
        &result,
        &json!({"resolved": 4, "skipped": 0, "failures": lines}),
        3,
    );
}

/// The audit's JSON holds an `issues[].backlog` id that is no filed
/// deferral, with the file that names it: `--report` reads one file.
#[test]
fn audit_json_holds_an_unfiled_backlog_id() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &[]);
    let reviews = tempfile::tempdir().expect("tempdir");
    write_review(
        reviews.path(),
        "task-1-review.json",
        &["WI-002", "WI-001", "WI-099"],
    );
    let shown = reviews
        .path()
        .join("task-1-review.json")
        .display()
        .to_string();
    let lines = [
        issue_unfiled(&shown, "WI-001"),
        issue_unfiled(&shown, "WI-099"),
    ];
    let (output, result) = audit_result(dir.path(), &base, &["--report", &shown]);
    assert_eq!(stderr(&output), printed(&lines));
    assert_eq!(output.status.code(), Some(1));
    assert_verdict(
        &result,
        &json!({"resolved": 3, "skipped": 0, "failures": lines}),
        2,
    );
}

/// Line B is a failure entry of the JSON: an install with no tracker and a
/// `backlog/` directory resolves no reference, so both references count as
/// skipped.
#[test]
fn audit_json_holds_line_b() {
    let dir = install_with_backlog_dir();
    without_the_process_pack(dir.path());
    let base = commit_all(dir.path(), "chore: install the kit");
    commit_with(
        dir.path(),
        "feat: add the lookup",
        &["Work-Item: WI-001", "Work-Item: WI-002"],
    );
    let (output, result) = audit_result(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), format!("{LINE_B}\n"));
    assert_eq!(output.status.code(), Some(1));
    assert_verdict(
        &result,
        &json!({"resolved": 0, "skipped": 2, "failures": [LINE_B]}),
        1,
    );
}

/// The line of a tracker that cannot be reached is a failure entry of the
/// JSON, and no reference counts as resolved. The audit still prints its
/// JSON and exits 1.
#[test]
fn audit_json_holds_an_unreachable_tracker() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-001"]);
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    let (output, result) = audit_result(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), format!("{JSON_UNREACHABLE}\n"));
    assert_eq!(output.status.code(), Some(1));
    assert_verdict(
        &result,
        &json!({"resolved": 0, "skipped": 1, "failures": [JSON_UNREACHABLE]}),
        1,
    );
}

/// A range whose references all resolve shows zero failures, and a range
/// with no reference shows zero of each count.
#[test]
fn audit_json_of_a_clean_range_shows_zero_failures() {
    let (dir, base) = json_install();
    commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-001"]);
    commit_with(
        dir.path(),
        "chore: park the cache",
        &["Deferred: WI-002 The cache needs a design."],
    );
    let (output, result) = audit_result(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), "");
    assert_eq!(output.status.code(), Some(0));
    assert_verdict(
        &result,
        &json!({"resolved": 2, "skipped": 0, "failures": []}),
        0,
    );

    let (quiet, none) = audit_result(dir.path(), "HEAD", &[]);
    assert_eq!(stderr(&quiet), "");
    assert_eq!(quiet.status.code(), Some(0));
    assert_verdict(
        &none,
        &json!({"resolved": 0, "skipped": 0, "failures": []}),
        0,
    );
}

/// With no tracker and no `backlog/` directory, the JSON counts the
/// references that the audit did not resolve, and the skipped line stays
/// out of `failures`: it fails nothing.
#[test]
fn audit_json_without_a_tracker_counts_the_references_it_skipped() {
    let dir = install_with_no_tracker();
    let base = commit_all(dir.path(), "chore: install the kit");
    commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-001"]);
    commit_with(
        dir.path(),
        "chore: park the cache",
        &["Deferred: WI-002 Needs a design.", "Work-Item: WI-003"],
    );
    let (output, result) = audit_result(dir.path(), &base, &[]);
    assert_eq!(stderr(&output), format!("{}\n", audit_line(3)));
    assert_eq!(output.status.code(), Some(0));
    assert_verdict(
        &result,
        &json!({"resolved": 0, "skipped": 3, "failures": []}),
        0,
    );
}

/// `quality.user-facing-text-is-contract`: the help of `audit` says where
/// the verdict on the references sits in the JSON result, and that the
/// `--json` file holds the same result as stdout.
#[test]
fn audit_help_names_where_the_verdict_sits() {
    let help = help_of("audit");
    for phrase in [
        "The JSON result holds the same verdict:",
        "`references.failures` lists each failure line",
        "`references.resolved` and `references.skipped` count the references",
        "`summary.reference_fail` counts the failures",
        "The line that asks for a tracker, and the line of a tracker that cannot be reached, are failures too",
        "Also writes the JSON result that stdout prints to this file",
    ] {
        assert!(
            help.contains(phrase),
            "audit --help lacks {phrase:?}: {help}"
        );
    }
}

// ---- the every-commit check ----

/// The lines that `check-knowledge` prints about `knowledge/team.json`. An
/// install without the process pack has findings of other files (the kit
/// skills cite the pack), so a test of the entry reads the lines about its
/// own file.
fn findings_about_team_json(dir: &Path) -> Vec<String> {
    let output = run(dir, &["check-knowledge"]);
    format!("{}{}", stdout(&output), stderr(&output))
        .lines()
        .filter(|line| line.starts_with("knowledge/team.json"))
        .map(str::to_string)
        .collect()
}

/// A JSON-tracker install with no process pack and an entry of its own that
/// holds the check of the process pack's every-commit entry: it requires a
/// `Work-Item` trailer, and exempts the three bots by author name and a
/// release subject by pattern. `check-knowledge` accepts the install, as it
/// must for the entry of the pack. Returns the directory and a base commit.
fn install_with_the_every_commit_check() -> (TempDir, String) {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    without_the_process_pack(dir.path());
    write_json(
        dir.path(),
        "knowledge/team.json",
        &json!({
            "topic": "team",
            "title": "Team rules",
            "entries": [{
                "id": ENTRY,
                "kind": "rule",
                "area": "global",
                "standing": true,
                "summary": "Every commit names the work item it delivers in a `Work-Item:` trailer.",
                "body": ["Write one trailer for each work item that the commit delivers."],
                "tags": [],
                "source": {"date": "2026-10-20", "by": "controller"},
                "check": {
                    "type": "commits",
                    "level": "fail",
                    "require_trailer": "Work-Item",
                    "exempt_authors": [
                        "dependabot[bot]",
                        "release-please[bot]",
                        "github-actions[bot]"
                    ],
                    "exempt_subjects": ["^chore\\(.+\\): release "],
                },
            }],
        }),
    );
    assert_eq!(
        findings_about_team_json(dir.path()),
        Vec::<String>::new(),
        "check-knowledge accepts the entry"
    );
    let base = commit_all(dir.path(), "chore: install the kit");
    (dir, base)
}

/// The line of the every-commit check for the commit `sha`.
fn no_trailer(sha: &str) -> String {
    format!("{sha}: no Work-Item trailer ({ENTRY})")
}

/// Spec 3.7: `check-knowledge` compiles each pattern of `exempt_subjects`, so
/// a malformed one is a finding of the gate that names the field, never an
/// exit-2 stop of `check-commit` after `check-knowledge` passed.
#[test]
fn check_knowledge_names_a_malformed_exempt_subjects_pattern() {
    let (dir, base) = install_with_the_every_commit_check();
    let mut team = read_json(dir.path(), "knowledge/team.json");
    team["entries"][0]["check"]["exempt_subjects"] = json!(["^chore", "(unclosed"]);
    write_json(dir.path(), "knowledge/team.json", &team);
    assert_eq!(
        findings_about_team_json(dir.path()),
        [format!(
            "knowledge/team.json {ENTRY}: check exempt_subjects is not a valid regex (Invalid regular expression: /(unclosed/: Unterminated group)"
        )]
    );
    commit_with(dir.path(), "feat: add the lookup", &[]);
    let stopped = run(dir.path(), &["check-commit", "--from", &base]);
    assert_eq!(stopped.status.code(), Some(2), "check-commit stops too");
}

/// Spec 3.7 and live case 6: `check-commit` fails a human `feat` commit that
/// has no `Work-Item:` trailer, with the line of the spec, and passes a commit
/// that names its item.
#[test]
fn check_commit_fails_a_human_commit_with_no_work_item_trailer() {
    let (dir, base) = install_with_the_every_commit_check();
    let bad = commit_with(dir.path(), "feat: add the lookup", &[]);
    commit_with(dir.path(), "feat: add the cache", &["Work-Item: WI-001"]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(&output, 1, "", &format!("{}\n", no_trailer(&bad)));
}

/// Spec 3.7: the line is the same when the range holds several commits that
/// break the rule, newest first, and a commit with the trailer in the middle
/// of the range changes neither.
#[test]
fn check_commit_prints_one_line_for_each_commit_with_no_trailer() {
    let (dir, base) = install_with_the_every_commit_check();
    let first = commit_with(dir.path(), "feat: add the lookup", &[]);
    commit_with(dir.path(), "feat: add the cache", &["Work-Item: WI-001"]);
    let third = commit_with(dir.path(), "fix: repair the index", &[]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(
        &output,
        1,
        "",
        &printed(&[no_trailer(&third), no_trailer(&first)]),
    );
}

/// Spec 3.7 (C32): `check-commit` reads the trailers under git's default
/// separator, whatever the repository's `trailer.separators`. Under `#:` a
/// commit with `Work-Item: WI-001` passes, a commit with no trailer fails, and
/// a commit whose only line uses `#` names no work item and fails.
#[test]
fn check_commit_reads_trailers_under_a_repository_trailer_separators() {
    let (dir, base) = install_with_the_every_commit_check();
    with_repository_trailer_separators(dir.path(), "#:");
    commit_with(dir.path(), "feat: add the cache", &["Work-Item: WI-001"]);
    let bare = commit_with(dir.path(), "feat: add the index", &[]);
    let hashed = commit_with(dir.path(), "feat: add the lookup", &["Work-Item# WI-001"]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(
        &output,
        1,
        "",
        &printed(&[no_trailer(&hashed), no_trailer(&bare)]),
    );
}

/// Spec 3.7: a commit with a `Work-Item:` trailer, or one by an exempt
/// author, or one whose subject is exempt, passes. The authors are matched
/// by name (`%an`), and the subject exemption holds for a human author: a
/// release-please run with a personal token authors its commits as a human.
/// It passes at the commit before this batch, which reads no such field: a
/// guard against an exemption that is too narrow.
#[test]
fn check_commit_exempts_the_authors_and_the_subjects_of_the_entry() {
    let (dir, base) = install_with_the_every_commit_check();
    commit_with(dir.path(), "feat: add the cache", &["Work-Item: WI-001"]);
    commit_as(
        dir.path(),
        "dependabot[bot]",
        "chore(deps): bump serde",
        &[],
    );
    commit_as(
        dir.path(),
        "release-please[bot]",
        "chore(main): release 1.5.0",
        &[],
    );
    commit_as(
        dir.path(),
        "github-actions[bot]",
        "chore: update the lockfile",
        &[],
    );
    commit_with(dir.path(), "chore(main): release 2.0.0", &[]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(&output, 0, "check-commit: ok\n", "");
}

/// Spec 3.7: the author exemption matches the whole name. A human whose name
/// holds a bot's name is no bot, and a subject that holds the release text
/// anywhere but at its start is no release commit.
#[test]
fn check_commit_exempts_by_whole_author_name_and_anchored_subject() {
    let (dir, base) = install_with_the_every_commit_check();
    let by_name = commit_as(
        dir.path(),
        "not dependabot[bot]",
        "feat: add the lookup",
        &[],
    );
    let by_subject = commit_with(dir.path(), "feat: prepare chore(main): release 2.0.0", &[]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(
        &output,
        1,
        "",
        &printed(&[no_trailer(&by_subject), no_trailer(&by_name)]),
    );
}

/// Spec 3.7: in the message-file mode of the commit-msg hook there is no
/// author yet, so only the subject exemption applies. A message that has no
/// trailer fails, whoever the environment names as the author; a release
/// subject and a message with the trailer pass.
#[test]
fn check_commit_message_file_mode_applies_the_subject_exemption_only() {
    let (dir, _base) = install_with_the_every_commit_check();
    let messages = tempfile::tempdir().expect("tempdir");
    let check = |text: &str, author: &str| {
        let file = messages.path().join("COMMIT_EDITMSG");
        fs::write(&file, text).expect("write the message");
        let mut command = tracker_support::houserules();
        command
            .args(["check-commit", file.to_str().expect("utf-8 path")])
            .arg("--dir")
            .arg(dir.path())
            .env("GIT_AUTHOR_NAME", author);
        command.output().expect("run check-commit")
    };
    let failed = check("feat: add the lookup\n", "dependabot[bot]");
    assert_output(&failed, 1, "", &format!("no Work-Item trailer ({ENTRY})\n"));
    let release = check("chore(main): release 2.0.0\n", "A Human");
    assert_output(&release, 0, "check-commit: ok\n", "");
    let named = check("feat: add the lookup\n\nWork-Item: WI-001\n", "A Human");
    assert_output(&named, 0, "check-commit: ok\n", "");
}

/// A `commits` check with another trailer key: the fields work in any
/// `commits` check, and a warn-level check prints a warning and passes. The
/// check demands a `Closes` trailer of every commit, except an author named in
/// `exempt_authors` and a subject that matches `exempt_subjects`.
#[test]
fn a_local_commits_check_takes_the_trailer_key_of_its_own() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    without_the_process_pack(dir.path());
    write_json(
        dir.path(),
        "knowledge/team.json",
        &json!({
            "topic": "team",
            "title": "Team rules",
            "entries": [{
                "id": "team.every-commit-closes-something",
                "kind": "rule",
                "area": "docs",
                "summary": "Every commit closes something.",
                "body": ["Name the item that the commit closes."],
                "tags": [],
                "source": {"date": "2026-10-20", "by": "controller"},
                "check": {
                    "type": "commits",
                    "level": "warn",
                    "require_trailer": "Closes",
                    "exempt_authors": ["A Bot"],
                    "exempt_subjects": ["^docs"],
                },
            }],
        }),
    );
    assert_eq!(
        findings_about_team_json(dir.path()),
        Vec::<String>::new(),
        "check-knowledge accepts the entry"
    );
    let base = commit_all(dir.path(), "chore: install the kit");
    let bad = commit_with(dir.path(), "feat: add the lookup", &["Work-Item: WI-001"]);
    commit_with(dir.path(), "feat: add the cache", &["Closes: WI-001"]);
    commit_with(dir.path(), "docs: describe the cache", &[]);
    commit_as(dir.path(), "A Bot", "fix: repair the index", &[]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(
        &output,
        0,
        &format!(
            "warn: {bad}: no Closes trailer (team.every-commit-closes-something)\ncheck-commit: ok\n"
        ),
        "",
    );
}
