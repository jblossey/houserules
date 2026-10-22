//! `stats` for overrides, cost, the window, sanctions, and reversals (spec 4,
//! 6, and 9 (8); cases 4, 6, and 15). Each case runs the compiled binary over
//! a workspace that a test writes or over a committed fixture under
//! `tests/fixtures/stats-35/`.

mod install_shape;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// The committed fixture workspace `name`, built component by component
/// (`houserules.path-pins-mirror-the-code`).
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("stats-35")
        .join(name)
}

/// One entry of the knowledge base: a judged rule has no `check`, a
/// deterministic rule has one.
fn entry(id: &str, deterministic: bool) -> Value {
    let mut entry = json!({
        "id": id, "kind": "rule", "area": "global", "standing": false,
        "summary": "A rule.", "source": {"date": "2026-10-09", "by": "review"},
    });
    if deterministic {
        entry["check"] =
            json!({"type": "report-field", "level": "fail", "if": "**", "field": "live_run"});
    }
    entry
}

/// A repository root that holds a loadable knowledge base with the judged
/// rules `j.one`, `j.two`, `j.three`, and `j.unused` and the deterministic
/// rules `d.one`, `d.two`, and `d.three`. `stats` loads the base before it
/// reads any workspace.
fn knowledge_root() -> TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    let knowledge = root.path().join("knowledge");
    fs::create_dir_all(&knowledge).expect("create knowledge/");
    fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
    fs::write(knowledge.join("areas.json"), "{}").expect("write areas.json");
    install_shape::mark_2_0(root.path());
    let entries: Vec<Value> = [
        ("j.one", false),
        ("j.two", false),
        ("j.three", false),
        ("j.unused", false),
        ("d.one", true),
        ("d.two", true),
        ("d.three", true),
    ]
    .iter()
    .map(|(id, deterministic)| entry(id, *deterministic))
    .collect();
    fs::write(
        knowledge.join("test.json"),
        serde_json::to_string(&json!({"entries": entries})).expect("serialize entries"),
    )
    .expect("write test.json");
    root
}

/// Runs `houserules stats <workspaces...> <flags...> --dir <root>`.
fn stats(root: &Path, workspaces: &[PathBuf], flags: &[&str]) -> Output {
    houserules()
        .arg("stats")
        .args(workspaces)
        .args(flags)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run stats")
}

/// The parsed JSON a successful `stats` run prints.
fn stats_json(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

/// The `rules` row of `id`.
fn rule<'a>(value: &'a Value, id: &str) -> &'a Value {
    value["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .find(|row| row["id"] == json!(id))
        .unwrap_or_else(|| panic!("no rules row for {id}"))
}

/// The proposal of `action` about `id`, when `value` holds one.
fn proposal<'a>(value: &'a Value, action: &str, id: &str) -> Option<&'a Value> {
    value["proposals"]
        .as_array()
        .expect("proposals array")
        .iter()
        .find(|row| row["action"] == json!(action) && row["id"] == json!(id))
}

/// The `path` of every `workspaces` row, in order.
fn paths(value: &Value) -> Vec<String> {
    value["workspaces"]
        .as_array()
        .expect("workspaces array")
        .iter()
        .map(|row| row["path"].as_str().expect("path").to_string())
        .collect()
}

/// The display form of the paths `dirs`, as `stats` prints a workspace path.
fn displayed(dirs: &[&PathBuf]) -> Vec<String> {
    dirs.iter().map(|dir| dir.display().to_string()).collect()
}

/// Writes `value` to `name` in `dir` and returns the size of the file.
fn write_json(dir: &Path, name: &str, value: &Value) -> u64 {
    let text = serde_json::to_string_pretty(value).expect("serialize");
    fs::write(dir.join(name), &text).expect("write the deliverable");
    text.len() as u64
}

/// A workspace directory `name` under `parent` that holds one report.
fn workspace_with_report(parent: &Path, name: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).expect("create the workspace");
    write_json(
        &dir,
        "task-1-report.json",
        &json!({"kind": "task-report", "knowledge_used": []}),
    );
    dir
}

/// A workspace directory `name` under `parent` that holds the task reviews
/// `reviews`, named `task-1-review.json`, `task-2-review.json`, and so on.
fn workspace_with_reviews(parent: &Path, name: &str, reviews: &[Value]) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).expect("create the workspace");
    for (index, review) in reviews.iter().enumerate() {
        write_json(&dir, &format!("task-{}-review.json", index + 1), review);
    }
    dir
}

/// Copies the files of the directory `from` into the new directory `to`.
fn copy_files(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create the copy");
    for entry in fs::read_dir(from).expect("read the fixture") {
        let entry = entry.expect("fixture entry");
        fs::copy(entry.path(), to.join(entry.file_name())).expect("copy a fixture file");
    }
}

/// A task review with the `rule_adherence` rows `rows` and the `issues`.
fn review(rows: &[Value], issues: &[Value]) -> Value {
    json!({
        "kind": "task-review", "model": "opus",
        "rule_adherence": rows, "issues": issues,
    })
}

/// A judged row of a review with the `result`.
fn judged_row(id: &str, result: &str) -> Value {
    json!({
        "id": id, "kind": "rule", "mode": "judged", "level": null,
        "result": result, "evidence": "e",
    })
}

/// A judged row with a person's override to `standing`.
fn overridden_row(id: &str, result: &str, standing: &str) -> Value {
    let mut row = judged_row(id, result);
    row["override"] = json!({"result": standing, "by": "owner", "reason": "r"});
    row
}

/// A finding that cites the rule `rule`.
fn finding(rule: &str, what: &str) -> Value {
    json!({
        "severity": "minor", "file": "src/a.rs", "what": what, "why": "w", "fix": "f",
        "rule": rule,
    })
}

// ---- overrides, verdicts, and cost (spec 4, case 4) ---------------------------------

/// A judged rule gets its `verdicts` (judged rows with a `pass` or `fail`
/// result), its `overrides`, its `override_rate`, and its `cost`: for each
/// review that judged it, the review file's bytes divided by the review's
/// judged rows and rounded down, summed. A task review, a re-review, and the
/// branch review all count.
#[test]
fn rules_rows_hold_verdicts_overrides_override_rate_and_cost() {
    let root = knowledge_root();
    let workspace = fixture("overrides");
    let size = |name: &str| {
        fs::metadata(workspace.join(name))
            .expect("fixture size")
            .len()
    };
    let (first, second, branch) = (
        size("task-1-review.json"),
        size("task-2-review.json"),
        size("branch-review.json"),
    );
    assert_ne!(first % 3, 0, "the fixture must exercise the rounding down");
    let value = stats_json(&stats(root.path(), &[workspace], &[]));

    let one = rule(&value, "j.one");
    assert_eq!(one["verdicts"], json!(3));
    assert_eq!(one["overrides"], json!(0));
    assert_eq!(one["override_rate"], json!(0.0));
    assert_eq!(one["cost"], json!(first / 3 + second / 2 + branch));

    let two = rule(&value, "j.two");
    assert_eq!(two["verdicts"], json!(2));
    assert_eq!(two["overrides"], json!(1));
    assert_eq!(two["override_rate"], json!(0.5));
    assert_eq!(two["cost"], json!(first / 3 + second / 2));

    let three = rule(&value, "j.three");
    assert_eq!(three["verdicts"], json!(0), "a skipped row is no verdict");
    assert_eq!(three["overrides"], json!(0));
    assert_eq!(three["override_rate"], Value::Null);
    assert_eq!(
        three["cost"],
        json!(first / 3),
        "a skipped row is still judged"
    );
}

/// The `override_rate` is `null` at zero verdicts and the `cost` is `null`
/// for a rule with no judged row; a deterministic rule never has one, even
/// when a review copies its audit row.
#[test]
fn a_rule_with_no_verdict_or_judged_row_gets_null_rate_and_null_cost() {
    let root = knowledge_root();
    let value = stats_json(&stats(root.path(), &[fixture("overrides")], &[]));
    let copied = rule(&value, "d.one");
    assert_eq!(copied["verdicts"], json!(0));
    assert_eq!(copied["overrides"], json!(0));
    assert_eq!(copied["override_rate"], Value::Null);
    assert_eq!(copied["cost"], Value::Null);
    let unused = rule(&value, "j.unused");
    assert_eq!(unused["verdicts"], json!(0));
    assert_eq!(unused["override_rate"], Value::Null);
    assert_eq!(unused["cost"], Value::Null);
}

/// A workspace with no override, sanction, or reversal gives each new key
/// its empty value: `sanctioned` is an empty list at the top and `0` per
/// rule, `reversals` is `0` per workspace.
#[test]
fn a_workspace_with_no_new_input_gets_the_empty_new_keys() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let workspace = workspace_with_report(scratch.path(), "batch-1");
    let value = stats_json(&stats(root.path(), &[workspace], &[]));
    assert_eq!(value["sanctioned"], json!([]));
    assert_eq!(value["workspaces"][0]["reversals"], json!(0));
    assert_eq!(rule(&value, "j.unused")["sanctioned"], json!(0));
}

/// An override on a row that is no verdict is no override of a verdict, so
/// the rate stays at most 1.
#[test]
fn an_override_on_a_row_that_is_no_verdict_is_not_counted() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let workspace = workspace_with_reviews(
        scratch.path(),
        "batch-1",
        &[review(&[overridden_row("j.one", "skipped", "pass")], &[])],
    );
    let value = stats_json(&stats(root.path(), &[workspace], &[]));
    let one = rule(&value, "j.one");
    assert_eq!(one["verdicts"], json!(0));
    assert_eq!(one["overrides"], json!(0));
    assert_eq!(one["override_rate"], Value::Null);
}

/// `mechanize` fires as today and only when the override rate is at most
/// 0.2 or `null`; its evidence gains `override_rate`.
#[test]
fn mechanize_needs_override_rate_at_most_0_2_or_null() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let mechanize = |name: &str, first: &[Value], second: &[Value]| -> Option<Value> {
        let parent = scratch.path().join(name);
        let one = workspace_with_reviews(&parent, "2026-10-01-a", first);
        let two = workspace_with_reviews(&parent, "2026-10-02-b", second);
        let value = stats_json(&stats(root.path(), &[one, two], &[]));
        proposal(&value, "mechanize", "j.one").cloned()
    };

    // One override in five verdicts: the rate is 0.2, so the rule is proposed.
    let at_the_limit = mechanize(
        "limit",
        &[
            review(&[overridden_row("j.one", "fail", "pass")], &[]),
            review(&[judged_row("j.one", "pass")], &[]),
            review(&[judged_row("j.one", "pass")], &[]),
        ],
        &[
            review(&[judged_row("j.one", "fail")], &[]),
            review(&[judged_row("j.one", "pass")], &[]),
        ],
    )
    .expect("mechanize fires at the rate 0.2");
    assert_eq!(at_the_limit["evidence"]["override_rate"], json!(0.2));
    assert_eq!(at_the_limit["evidence"]["flagged_workspaces"], json!(2));

    // One override in four verdicts: the rate is 0.25, so the rule is not proposed.
    assert_eq!(
        mechanize(
            "above",
            &[
                review(&[overridden_row("j.one", "fail", "pass")], &[]),
                review(&[judged_row("j.one", "pass")], &[]),
            ],
            &[
                review(&[judged_row("j.one", "fail")], &[]),
                review(&[judged_row("j.one", "pass")], &[]),
            ],
        ),
        None,
        "mechanize is withheld at the rate 0.25"
    );

    // Findings only: no verdict, so the rate is null and the rule is proposed.
    let no_verdict = mechanize(
        "null",
        &[review(&[], &[finding("j.one", "first")])],
        &[review(&[], &[finding("j.one", "second")])],
    )
    .expect("mechanize fires at a null rate");
    assert_eq!(no_verdict["evidence"]["override_rate"], Value::Null);
}

/// The `samples` of a `mechanize` proposal are up to three findings that cite
/// the rule, the most recent workspace first and each workspace in file
/// order. A finding that cites another rule is no sample.
#[test]
fn mechanize_samples_are_the_three_newest_findings_of_the_rule() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let workspace = |name: &str| {
        workspace_with_reviews(
            scratch.path(),
            name,
            &[review(
                &[judged_row("j.one", "fail")],
                &[
                    finding("j.one", &format!("{name} first")),
                    finding("j.two", &format!("{name} other rule")),
                    finding("j.one", &format!("{name} second")),
                ],
            )],
        )
    };
    let (second, third, first) = (
        workspace("2026-10-02-b"),
        workspace("2026-10-03-c"),
        workspace("2026-10-01-a"),
    );
    let value = stats_json(&stats(root.path(), &[second, third, first], &[]));
    let proposal = proposal(&value, "mechanize", "j.one").expect("mechanize fires");
    assert_eq!(
        proposal["evidence"]["samples"],
        json!([
            finding("j.one", "2026-10-03-c first"),
            finding("j.one", "2026-10-03-c second"),
            finding("j.one", "2026-10-02-b first"),
        ])
    );
}

/// A workspace directory `name` under `parent` whose re-review cites `j.one`
/// in its `new_breakage` and its `issues`, and whose spec review cites it in
/// its `findings`; with `issues`, a task review and the branch review cite it
/// in their `issues` too.
fn workspace_citing_j_one(parent: &Path, name: &str, issues: bool) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).expect("create the workspace");
    if issues {
        write_json(
            &dir,
            "task-1-review.json",
            &review(&[], &[finding("j.one", &format!("{name} task issue"))]),
        );
        write_json(
            &dir,
            "branch-review.json",
            &json!({
                "kind": "branch-review", "model": "opus", "rule_adherence": [],
                "issues": [finding("j.one", &format!("{name} branch issue"))],
            }),
        );
    }
    write_json(
        &dir,
        "task-1-review-r1.json",
        &json!({
            "kind": "re-review", "model": "opus", "rule_adherence": [],
            "new_breakage": [finding("j.one", &format!("{name} new breakage"))],
            "issues": [finding("j.one", &format!("{name} re-review issue"))],
        }),
    );
    write_json(
        &dir,
        "spec-review.json",
        &json!({
            "kind": "spec-review", "model": "opus",
            "findings": [finding("j.one", &format!("{name} spec finding"))],
        }),
    );
    dir
}

/// The `samples` of the `mechanize` proposal about `j.one` over the two
/// workspaces `workspace_citing_j_one` writes under a new directory of
/// `scratch`.
fn j_one_samples(scratch: &Path, issues: bool) -> Value {
    let root = knowledge_root();
    let parent = scratch.join(if issues { "issues" } else { "no-issues" });
    let older = workspace_citing_j_one(&parent, "2026-10-01-a", issues);
    let newer = workspace_citing_j_one(&parent, "2026-10-02-b", issues);
    let value = stats_json(&stats(root.path(), &[older, newer], &[]));
    let mechanize = proposal(&value, "mechanize", "j.one").expect("mechanize fires");
    assert_eq!(mechanize["evidence"]["flagged_workspaces"], json!(2));
    mechanize["evidence"]["samples"].clone()
}

/// The `samples` hold entries of the `issues` of task reviews and of the
/// branch review only, the most recent workspace first. A re-review's
/// `new_breakage` and `issues` and a spec review's `findings` are no sample.
#[test]
fn mechanize_samples_hold_the_issues_of_task_and_branch_reviews_only() {
    let scratch = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        j_one_samples(scratch.path(), true),
        json!([
            finding("j.one", "2026-10-02-b task issue"),
            finding("j.one", "2026-10-02-b branch issue"),
            finding("j.one", "2026-10-01-a task issue"),
        ])
    );
}

/// Workspaces whose only citations are a re-review's findings and a spec
/// review's findings still flag `mechanize`, with no sample: the `findings`
/// count of the evidence and its `samples` have different populations.
#[test]
fn a_rule_cited_only_by_re_reviews_and_spec_reviews_has_no_samples() {
    let scratch = tempfile::tempdir().expect("tempdir");
    assert_eq!(j_one_samples(scratch.path(), false), json!([]));
}

/// The samples of one workspace follow the order of the files, whichever model
/// wrote each review: the review files are read by name and `branch-review.json`
/// after them, and the model of a review decides nothing.
#[test]
fn mechanize_samples_of_one_workspace_follow_the_files_not_the_models() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let dir = scratch.path().join("2026-10-02-b");
    fs::create_dir_all(&dir).expect("create the workspace");
    for (name, kind, model) in [
        ("task-1-review.json", "task-review", "sonnet"),
        ("task-2-review.json", "task-review", "opus"),
        ("branch-review.json", "branch-review", "haiku"),
    ] {
        write_json(
            &dir,
            name,
            &json!({
                "kind": kind, "model": model, "rule_adherence": [],
                "issues": [finding("j.one", name)],
            }),
        );
    }
    let other = workspace_with_reviews(
        scratch.path(),
        "2026-10-01-a",
        &[review(&[], &[finding("j.one", "older")])],
    );
    let value = stats_json(&stats(root.path(), &[other, dir], &[]));
    assert_eq!(
        proposal(&value, "mechanize", "j.one").expect("mechanize fires")["evidence"]["samples"],
        json!([
            finding("j.one", "task-1-review.json"),
            finding("j.one", "task-2-review.json"),
            finding("j.one", "branch-review.json"),
        ])
    );
}

/// The `retire` evidence gains `cost`: the sum over the reviews that judged
/// the rule, and `null` for a rule with no judged row (a deterministic rule).
#[test]
fn retire_evidence_carries_cost() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let audit = json!({
        "ids": ["j.one", "d.one"],
        "rules": [
            {"id": "d.one", "mode": "deterministic", "level": "fail", "result": "pass",
             "evidence": "e"},
            {"id": "j.one", "mode": "judged", "level": null, "result": "open", "evidence": ""},
        ],
    });
    let mut review_bytes = 0;
    let mut workspaces = Vec::new();
    for name in ["2026-10-01-a", "2026-10-02-b", "2026-10-03-c"] {
        let dir = scratch.path().join(name);
        fs::create_dir_all(&dir).expect("create the workspace");
        write_json(&dir, "task-1-audit.json", &audit);
        review_bytes += write_json(
            &dir,
            "task-1-review.json",
            &review(&[judged_row("j.one", "pass")], &[]),
        );
        workspaces.push(dir);
    }
    let value = stats_json(&stats(root.path(), &workspaces, &[]));
    assert_eq!(
        proposal(&value, "retire", "j.one").expect("retire j.one")["evidence"],
        json!({"injected": 3, "fails": 0, "findings": 0, "cited": 0, "cost": review_bytes})
    );
    assert_eq!(
        proposal(&value, "retire", "d.one").expect("retire d.one")["evidence"],
        json!({"injected": 3, "fails": 0, "findings": 0, "cited": 0, "cost": null})
    );
}

/// A review adds one share to the `cost` of a rule, whatever number of its
/// rows judge the rule. The share is the review file's bytes divided by all
/// the judged rows of the review, rounded down. A rule that the review judges
/// in two rows still counts two rows and two verdicts.
#[test]
fn a_review_that_judges_a_rule_in_two_rows_costs_the_rule_one_share() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let workspace = workspace_with_reviews(
        scratch.path(),
        "batch-1",
        &[
            review(
                &[
                    judged_row("j.one", "pass"),
                    judged_row("j.one", "fail"),
                    judged_row("j.two", "pass"),
                ],
                &[],
            ),
            review(&[judged_row("j.one", "pass")], &[]),
        ],
    );
    let size = |name: &str| {
        fs::metadata(workspace.join(name))
            .expect("review size")
            .len()
    };
    let (dup, single) = (size("task-1-review.json"), size("task-2-review.json"));
    let value = stats_json(&stats(root.path(), &[workspace], &[]));

    let one = rule(&value, "j.one");
    assert_eq!(one["cost"], json!(dup / 3 + single));
    assert_eq!(one["rows"], json!(3), "each judged row is still a row");
    assert_eq!(
        one["verdicts"],
        json!(3),
        "each judged row is still a verdict"
    );
    assert_eq!(rule(&value, "j.two")["cost"], json!(dup / 3));
}

// ---- the window (spec 4, case 4, review focus 5) ------------------------------------

/// `--window 3` over five workspaces reads the three newest by the date
/// prefix of the folder name, and keeps the order the caller gave.
#[test]
fn window_reads_the_n_newest_workspaces_by_their_date_prefix() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let dirs: Vec<PathBuf> = [
        "2026-10-03-batch-3",
        "2026-10-01-batch-1",
        "2026-10-05-batch-5",
        "2026-10-02-batch-2",
        "2026-10-04-batch-4",
    ]
    .iter()
    .map(|name| workspace_with_report(scratch.path(), name))
    .collect();
    let value = stats_json(&stats(root.path(), &dirs, &["--window", "3"]));
    assert_eq!(paths(&value), displayed(&[&dirs[0], &dirs[2], &dirs[4]]));
    assert_eq!(value["cost"]["totals"]["tasks"], json!(3));
}

/// A window above the count reads every workspace.
#[test]
fn window_above_the_count_reads_all() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let dirs: Vec<PathBuf> = ["2026-10-02-batch-2", "2026-10-01-batch-1"]
        .iter()
        .map(|name| workspace_with_report(scratch.path(), name))
        .collect();
    let value = stats_json(&stats(root.path(), &dirs, &["--window", "9"]));
    assert_eq!(paths(&value), displayed(&[&dirs[0], &dirs[1]]));
}

/// With no flag `stats` reads every workspace in the order given.
#[test]
fn no_window_flag_reads_every_workspace_in_the_order_given() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let dirs: Vec<PathBuf> = [
        "2026-10-01-batch-1",
        "2026-10-03-batch-3",
        "2026-10-02-batch-2",
    ]
    .iter()
    .map(|name| workspace_with_report(scratch.path(), name))
    .collect();
    let value = stats_json(&stats(root.path(), &dirs, &[]));
    assert_eq!(paths(&value), displayed(&[&dirs[0], &dirs[1], &dirs[2]]));
}

/// Two folders of one date whose names differ only in the batch number: the
/// name decides, whatever order the caller gives. A folder with no date
/// prefix sorts before every dated one, and undated folders sort by name.
#[test]
fn window_orders_same_date_folders_by_name() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let batch_32 = workspace_with_report(scratch.path(), "2026-10-10-batch-32");
    let batch_33 = workspace_with_report(scratch.path(), "2026-10-10-batch-33");
    for dirs in [
        [batch_32.clone(), batch_33.clone()],
        [batch_33.clone(), batch_32.clone()],
    ] {
        let value = stats_json(&stats(root.path(), &dirs, &["--window", "1"]));
        assert_eq!(paths(&value), displayed(&[&batch_33]));
    }

    let notes = workspace_with_report(scratch.path(), "notes");
    let older = workspace_with_report(scratch.path(), "2026-01-01-batch-1");
    let value = stats_json(&stats(
        root.path(),
        &[older.clone(), notes.clone()],
        &["--window", "1"],
    ));
    assert_eq!(paths(&value), displayed(&[&older]), "undated sorts first");

    let zulu = workspace_with_report(scratch.path(), "zulu");
    let alpha = workspace_with_report(scratch.path(), "alpha");
    let value = stats_json(&stats(
        root.path(),
        &[alpha.clone(), older.clone(), zulu.clone()],
        &["--window", "2"],
    ));
    assert_eq!(
        paths(&value),
        displayed(&[&older, &zulu]),
        "undated folders sort by name"
    );
}

/// The window filters the named workspaces before any read: a malformed file
/// in a workspace outside the window is never read.
#[test]
fn window_filters_before_any_read() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let old = workspace_with_report(scratch.path(), "2026-10-01-batch-1");
    fs::write(old.join("task-1-report.json"), "not json").expect("write a malformed report");
    let new = workspace_with_report(scratch.path(), "2026-10-02-batch-2");
    let dirs = [old, new.clone()];
    assert_eq!(
        stats(root.path(), &dirs, &[]).status.code(),
        Some(2),
        "without the window the malformed report stops the run"
    );
    let value = stats_json(&stats(root.path(), &dirs, &["--window", "1"]));
    assert_eq!(paths(&value), displayed(&[&new]));
}

/// A window of 0 is refused with its line, and `stats` prints nothing else.
#[test]
fn window_zero_prints_its_line_and_exits_2() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let workspace = workspace_with_report(scratch.path(), "2026-10-01-batch-1");
    let output = stats(root.path(), &[workspace], &["--window", "0"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        "--window must be 1 or more\n"
    );
}

/// `stats --help` names the window flag and what the new keys count. Clap
/// wraps the help, so the check reads it with its whitespace collapsed.
#[test]
fn stats_help_names_the_window() {
    let output = houserules()
        .args(["stats", "--help"])
        .output()
        .expect("run stats --help");
    let help = String::from_utf8(output.stdout).expect("utf8 stdout");
    let help = help.split_whitespace().collect::<Vec<_>>().join(" ");
    for statement in [
        "--window <N>",
        "Reads only the N most recent of the workspaces",
        "by the YYYY-MM-DD prefix of its folder name, then by name",
        "a folder with no date prefix is the oldest",
        "N above the number of workspaces reads all of them",
        "N must be 1 or more",
        "`verdicts`, `overrides`, the `override_rate`, and the `cost`: \
         the sum of each judging review's share of its bytes",
        "A fail that a sanction accepts counts under `sanctioned`, not under `violations`",
        "counts `reversals`: the `Reversal:` lines of the `progress.md` in that workspace",
    ] {
        assert!(help.contains(statement), "{statement}\n{help}");
    }
}

// ---- sanctions (spec 6, case 6) -----------------------------------------------------

/// A review row that keeps `sanctioned` and an audit row whose evidence ends
/// with `(sanctioned: <ref>)` count under `sanctioned`, in the rule's count
/// and in the top-level list, and never under `violations` or the rule's
/// `fails`. An audit row that has both forms counts once. A reviewer's copy
/// of a deterministic row counts nowhere twice.
#[test]
fn sanctioned_fails_count_apart_from_violations() {
    let root = knowledge_root();
    let workspace = fixture("sanctions");
    let label = workspace.display().to_string();
    let value = stats_json(&stats(root.path(), std::slice::from_ref(&workspace), &[]));

    assert_eq!(
        value["violations"],
        json!([
            {"id": "d.three", "count": 1, "tasks": ["1"]},
            {"id": "j.two", "count": 1, "tasks": ["1"]},
        ])
    );
    assert_eq!(
        value["sanctioned"],
        json!([
            {"id": "d.one", "workspace": label, "task": "1", "ref": "controller step 4 (T8)"},
            {"id": "d.two", "workspace": label, "task": "1", "ref": "plan, controller step 4"},
            {"id": "j.one", "workspace": label, "task": "1", "ref": "owner ruling 12"},
        ])
    );
    for (id, fails, sanctioned) in [
        ("d.one", 0, 1),
        ("d.two", 0, 1),
        ("d.three", 1, 0),
        ("j.one", 0, 1),
        ("j.two", 1, 0),
    ] {
        let row = rule(&value, id);
        assert_eq!(row["fails"], json!(fails), "{id} fails");
        assert_eq!(row["sanctioned"], json!(sanctioned), "{id} sanctioned");
    }
    assert_eq!(
        rule(&value, "j.one")["rows"],
        json!(1),
        "a sanctioned row is still a row"
    );
}

/// With several workspaces the `task` of a sanctioned entry carries the
/// workspace label, as the `tasks` of `violations` do, and the `workspace` is
/// the path as given.
#[test]
fn sanctioned_entries_name_their_workspace_and_task() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let first = scratch.path().join("2026-10-01-batch-1");
    let second = scratch.path().join("2026-10-02-batch-2");
    copy_files(&fixture("sanctions"), &first);
    copy_files(&fixture("sanctions"), &second);
    let value = stats_json(&stats(root.path(), &[second.clone(), first.clone()], &[]));
    let entries: Vec<&Value> = value["sanctioned"]
        .as_array()
        .expect("sanctioned list")
        .iter()
        .filter(|entry| entry["id"] == json!("d.one"))
        .collect();
    assert_eq!(
        entries,
        [
            &json!({
                "id": "d.one", "workspace": first.display().to_string(),
                "task": "2026-10-01-batch-1/1", "ref": "controller step 4 (T8)",
            }),
            &json!({
                "id": "d.one", "workspace": second.display().to_string(),
                "task": "2026-10-02-batch-2/1", "ref": "controller step 4 (T8)",
            }),
        ]
    );
    assert_eq!(rule(&value, "d.one")["sanctioned"], json!(2));
}

/// A rule whose only fail is sanctioned still draws no `retire` proposal: the
/// rule fired, and a booked interim fail does not make it unused.
#[test]
fn a_sanctioned_fail_still_blocks_a_retire() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut workspaces = Vec::new();
    for (name, result, evidence) in [
        ("2026-10-01-a", "fail", "e (sanctioned: owner ruling 12)"),
        ("2026-10-02-b", "pass", "e"),
        ("2026-10-03-c", "pass", "e"),
    ] {
        let dir = scratch.path().join(name);
        fs::create_dir_all(&dir).expect("create the workspace");
        write_json(
            &dir,
            "task-1-audit.json",
            &json!({
                "ids": ["d.one"],
                "rules": [{"id": "d.one", "mode": "deterministic", "level": "fail",
                           "result": result, "evidence": evidence}],
            }),
        );
        workspaces.push(dir);
    }
    let value = stats_json(&stats(root.path(), &workspaces, &[]));
    assert_eq!(proposal(&value, "retire", "d.one"), None);
}

/// A sanctioned fail fires the rule for every proposal, as a fail did before
/// the sanction existed: two workspaces whose only `j.one` rows are judged
/// fails earn `mechanize` whether or not a sanction accepts the fails. Only
/// the rule's `fails` and `violations` leave the sanctioned fails out.
#[test]
fn a_sanctioned_fail_flags_its_workspace_for_mechanize_like_any_fail() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let value_of = |parent: &str, sanctioned: bool| -> Value {
        let mut row = judged_row("j.one", "fail");
        if sanctioned {
            row["sanctioned"] = json!("owner ruling 9");
        }
        let workspaces: Vec<PathBuf> = ["2026-10-05-a", "2026-10-06-b"]
            .iter()
            .map(|name| {
                workspace_with_reviews(
                    &scratch.path().join(parent),
                    name,
                    &[review(std::slice::from_ref(&row), &[])],
                )
            })
            .collect();
        stats_json(&stats(root.path(), &workspaces, &[]))
    };
    let actions = |value: &Value| -> Vec<Value> {
        value["proposals"]
            .as_array()
            .expect("proposals array")
            .iter()
            .map(|row| row["action"].clone())
            .collect()
    };

    let unsanctioned = value_of("unsanctioned", false);
    let sanctioned = value_of("sanctioned", true);
    assert_eq!(actions(&unsanctioned), [json!("mechanize")]);
    assert_eq!(actions(&sanctioned), [json!("mechanize")]);
    let evidence = &proposal(&sanctioned, "mechanize", "j.one").expect("mechanize")["evidence"];
    assert_eq!(evidence["flagged_workspaces"], json!(2));
    assert_eq!(evidence["fails"], json!(0), "the rule's fails leave it out");
    let one = rule(&sanctioned, "j.one");
    assert_eq!(
        (one["fails"].clone(), one["sanctioned"].clone()),
        (json!(0), json!(2))
    );
    assert_eq!(sanctioned["violations"], json!([]));
}

/// A `sanctioned` or `override` field of the wrong type is one named error,
/// not a row that silently counts as unsanctioned or as no override
/// (`houserules.crash-paths-are-named`).
#[test]
fn a_wrongly_typed_override_or_sanctioned_field_is_a_named_error() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let mut bad_override = judged_row("j.one", "fail");
    bad_override["override"] = json!("yes");
    let mut bad_sanction = judged_row("j.two", "fail");
    bad_sanction["sanctioned"] = json!("");
    for (name, row, line) in [
        (
            "override",
            bad_override,
            "task-1-review.json: rule_adherence row \"j.one\": override is not an object\n",
        ),
        (
            "sanctioned",
            bad_sanction,
            "task-1-review.json: rule_adherence row \"j.two\": \
             sanctioned is not a non-empty string\n",
        ),
    ] {
        let workspace = workspace_with_reviews(scratch.path(), name, &[review(&[row], &[])]);
        let output = stats(root.path(), &[workspace], &[]);
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert_eq!(output.stdout, b"", "{name}: no stdout on the error path");
        assert_eq!(String::from_utf8(output.stderr).expect("utf8 stderr"), line);
    }
}

// ---- reversals (spec 9 (8), case 15) ------------------------------------------------

/// `Reversal:`, `- Reversal:`, `  * Reversal:`, and `+ Reversal:` lines of a
/// `progress.md` count under the workspace's `reversals`, with any leading
/// spaces. A line that holds `Reversal:` after other text, a lowercase word,
/// and a word with no colon do not.
#[test]
fn reversals_count_the_progress_lines_that_start_with_the_word() {
    let root = knowledge_root();
    let value = stats_json(&stats(root.path(), &[fixture("reversals")], &[]));
    assert_eq!(value["workspaces"][0]["reversals"], json!(5));
}

/// A workspace with no `progress.md` has `0` reversals, and each workspace
/// counts its own file. A ledger with CRLF line ends counts the same.
#[test]
fn each_workspace_counts_its_own_progress_file() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let with_file = workspace_with_report(scratch.path(), "2026-10-01-batch-1");
    fs::write(
        with_file.join("progress.md"),
        "Reversal: a\r\n- Reversal: b\r\nnot one\r\n",
    )
    .expect("write the ledger");
    let without_file = workspace_with_report(scratch.path(), "2026-10-02-batch-2");
    let value = stats_json(&stats(root.path(), &[with_file, without_file], &[]));
    assert_eq!(value["workspaces"][0]["reversals"], json!(2));
    assert_eq!(value["workspaces"][1]["reversals"], json!(0));
}

/// A `progress.md` that cannot be read as text is one named error: the path,
/// then the operating system's own text (`houserules.crash-paths-are-named`).
#[test]
fn a_progress_file_that_is_no_text_is_a_named_error() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let workspace = workspace_with_report(scratch.path(), "2026-10-01-batch-1");
    let progress = workspace.join("progress.md");
    fs::write(&progress, [0xff, 0xfe, b'\n']).expect("write a file that is no UTF-8");
    let output = stats(root.path(), &[workspace], &[]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"", "no stdout on the error path");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.starts_with(&format!("{}: ", progress.display())),
        "{stderr}"
    );
}

// ---- pins of unchanged behavior -----------------------------------------------------

/// A workspace with no override, sanction, or reversal keeps every key and
/// proposal that `stats` printed before: the rows read without the new keys
/// equal the old rows.
#[test]
fn a_workspace_with_no_new_input_keeps_every_existing_key_and_proposal() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let dir = scratch.path().join("batch-1");
    fs::create_dir_all(&dir).expect("create the workspace");
    write_json(
        &dir,
        "task-1-audit.json",
        &json!({
            "ids": ["d.one", "j.one"],
            "rules": [
                {"id": "d.one", "mode": "deterministic", "level": "fail", "result": "fail",
                 "evidence": "e"},
                {"id": "j.one", "mode": "judged", "level": null, "result": "open",
                 "evidence": ""},
            ],
        }),
    );
    write_json(
        &dir,
        "task-1-report.json",
        &json!({"kind": "task-report", "knowledge_used": ["j.one"], "fix_rounds": []}),
    );
    write_json(
        &dir,
        "task-1-review.json",
        &review(
            &[
                json!({"id": "d.one", "mode": "deterministic", "level": "fail",
                       "result": "fail", "evidence": "e"}),
                judged_row("j.one", "pass"),
            ],
            &[
                json!({"severity": "important", "file": "src/a.rs", "what": "w", "why": "w",
                     "fix": "f", "rule": "j.one"}),
            ],
        ),
    );
    let report_bytes = fs::metadata(dir.join("task-1-report.json"))
        .expect("report size")
        .len();
    let value = stats_json(&stats(root.path(), std::slice::from_ref(&dir), &[]));

    assert_eq!(
        value["violations"],
        json!([{"id": "d.one", "count": 1, "tasks": ["1"]}])
    );
    assert_eq!(
        value["unused_ids"],
        json!([{"id": "d.one", "tasks": ["1"]}])
    );
    assert_eq!(value["audits"], json!({"files": 1, "tasks": 1}));
    assert_eq!(value["reviews"], json!({"files": 1}));
    assert_eq!(value["proposals"], json!([]));

    let old_rule_keys = [
        "id",
        "source",
        "standing",
        "mode",
        "workspaces",
        "injected",
        "rows",
        "fails",
        "findings",
        "targets",
        "cited",
    ];
    let old_row = |row: &Value| -> Value {
        let mut kept = serde_json::Map::new();
        for key in old_rule_keys {
            kept.insert(key.to_string(), row[key].clone());
        }
        Value::Object(kept)
    };
    assert_eq!(
        old_row(rule(&value, "d.one")),
        json!({
            "id": "d.one", "source": "local", "standing": false, "mode": "deterministic",
            "workspaces": 1, "injected": 1, "rows": 1, "fails": 1,
            "findings": {"critical": 0, "important": 0, "minor": 0, "other": 0},
            "targets": {"code": 0, "deliverable": 0, "prose": 0, "other": 0},
            "cited": 0,
        })
    );
    assert_eq!(
        old_row(rule(&value, "j.one")),
        json!({
            "id": "j.one", "source": "local", "standing": false, "mode": "judged",
            "workspaces": 1, "injected": 1, "rows": 1, "fails": 0,
            "findings": {"critical": 0, "important": 1, "minor": 0, "other": 0},
            "targets": {"code": 1, "deliverable": 0, "prose": 0, "other": 0},
            "cited": 1,
        })
    );

    let workspace_row = &value["workspaces"][0];
    for (key, expected) in [
        ("path", json!(dir.display().to_string())),
        ("tasks", json!(1)),
        ("reviews", json!(1)),
        ("re_reviews", json!(0)),
        ("fix_rounds", json!(0)),
        ("report_bytes", json!(report_bytes)),
        (
            "findings",
            json!({"critical": 0, "important": 1, "minor": 0, "other": 0}),
        ),
        (
            "targets",
            json!({"code": 1, "deliverable": 0, "prose": 0, "other": 0}),
        ),
    ] {
        assert_eq!(workspace_row[key], expected, "workspace row {key}");
    }
}
