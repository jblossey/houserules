//! `stats` by model (spec 4.5, case 17): the `models` key, its `pre-2.0` row
//! for a deliverable with no `model`, the spec reviews that `stats` reads as
//! reviews, the remedy that names every file `stats` reads, and the sample
//! `models` block of README (HR-165 (1)). Each case runs the compiled binary
//! over a workspace that the test writes.

mod tracker_support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    install_with_the_process_pack_and_no_tracker, install_without_backlog, repo_root, run, stderr,
    stdout,
};

/// The remedy that `stats` appends to the error for a file it cannot read as
/// a deliverable. It names every file `stats` reads, `spec-review.json` and
/// `spec-review-r<n>.json` included.
const REMEDY: &str = "stats reads every task-*-audit*.json, task-*-review*.json, \
    task-*-report.json, spec-review.json, spec-review-r<n>.json, and branch-review.json \
    in the workspace as a deliverable; rename the file or repair it";

/// Writes `value` to `name` in `workspace` and returns the file's size.
fn write_deliverable(workspace: &Path, name: &str, value: &Value) -> u64 {
    let text = serde_json::to_string_pretty(value).expect("serialize");
    fs::write(workspace.join(name), &text).expect("write the deliverable");
    text.len() as u64
}

/// A finding row of a task review, a branch review, or a re-review.
fn issue(severity: &str, file: &str) -> Value {
    json!({
        "severity": severity, "file": file, "what": "w", "why": "w", "fix": "f",
    })
}

/// A finding row of a spec review: it names a spec section, not a file.
fn spec_finding(severity: &str) -> Value {
    json!({
        "id": "S1", "severity": severity, "section": "3.4",
        "what": "w", "evidence": "e", "fix": "f",
    })
}

/// The sizes of the two report files that `workspace` holds.
struct ReportSizes {
    with_model: u64,
    without_model: u64,
}

/// A workspace with deliverables from three models and from an older kit:
///
/// - `claude-sonnet-5-5` wrote the report of task 1, which has one fix round;
/// - `claude-opus-5-5` wrote the review of task 1 (a critical finding on code,
///   a minor one on prose), the spec review (a critical, an important, and two
///   minor findings), and the branch review (an important finding on code);
/// - `human` wrote the re-review of task 2 (a minor finding on prose);
/// - the report and the review of task 2 carry no `model`, so they count in
///   `pre-2.0` (the review has an important finding on code).
fn mixed_workspace() -> (TempDir, ReportSizes) {
    let workspace = tempfile::tempdir().expect("tempdir");
    let path = workspace.path();
    let with_model = write_deliverable(
        path,
        "task-1-report.json",
        &json!({
            "kind": "task-report", "model": "claude-sonnet-5-5",
            "knowledge_used": [], "fix_rounds": [{"round": 1}],
        }),
    );
    write_deliverable(
        path,
        "task-1-review.json",
        &json!({
            "kind": "task-review", "model": "claude-opus-5-5", "rule_adherence": [],
            "issues": [issue("critical", "crates/a.rs"), issue("minor", "README.md")],
        }),
    );
    let without_model = write_deliverable(
        path,
        "task-2-report.json",
        &json!({"kind": "task-report", "knowledge_used": []}),
    );
    write_deliverable(
        path,
        "task-2-review.json",
        &json!({
            "kind": "task-review", "rule_adherence": [],
            "issues": [issue("important", "crates/b.rs")],
        }),
    );
    write_deliverable(
        path,
        "task-2-review-r1.json",
        &json!({
            "kind": "re-review", "model": "human", "rule_adherence": [],
            "new_breakage": [issue("minor", "docs/c.md")],
        }),
    );
    write_deliverable(
        path,
        "spec-review.json",
        &json!({
            "kind": "spec-review", "model": "claude-opus-5-5",
            "findings": [
                spec_finding("critical"),
                spec_finding("important"),
                spec_finding("minor"),
                spec_finding("minor"),
            ],
        }),
    );
    write_deliverable(
        path,
        "branch-review.json",
        &json!({
            "kind": "branch-review", "model": "claude-opus-5-5", "rule_adherence": [],
            "issues": [issue("important", "crates/d.rs")],
        }),
    );
    (
        workspace,
        ReportSizes {
            with_model,
            without_model,
        },
    )
}

/// Runs `stats` over `workspace` in `install` and returns the parsed output.
fn stats(install: &Path, workspace: &Path) -> Value {
    let output = run(
        install,
        &["stats", workspace.to_str().expect("a UTF-8 path")],
    );
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    serde_json::from_str(&stdout(&output)).expect("stats prints JSON")
}

/// Spec 4.5 and case 17: `models` holds one row per model family, sorted by
/// row name, with the cost fields and the findings by severity; the id
/// `claude-opus-5-5` counts in the row `opus`; a deliverable with no `model`
/// counts in the row `pre-2.0`; a spec review counts as a review.
#[test]
fn stats_prints_models_with_a_pre_2_0_row_and_counts_a_spec_review() {
    let install = install_without_backlog();
    let (workspace, sizes) = mixed_workspace();
    let value = stats(install.path(), workspace.path());
    let expected = json!([
        {
            "model": "human",
            "tasks": 0, "reviews": 1, "re_reviews": 1, "fix_rounds": 0, "report_bytes": 0,
            "findings": {"critical": 0, "important": 0, "minor": 1, "other": 0},
            "targets": {"code": 0, "deliverable": 0, "prose": 1, "other": 0},
        },
        {
            "model": "opus",
            "tasks": 0, "reviews": 3, "re_reviews": 0, "fix_rounds": 0, "report_bytes": 0,
            "findings": {"critical": 2, "important": 2, "minor": 3, "other": 0},
            "targets": {"code": 2, "deliverable": 0, "prose": 1, "other": 4},
        },
        {
            "model": "pre-2.0",
            "tasks": 1, "reviews": 1, "re_reviews": 0, "fix_rounds": 0,
            "report_bytes": sizes.without_model,
            "findings": {"critical": 0, "important": 1, "minor": 0, "other": 0},
            "targets": {"code": 1, "deliverable": 0, "prose": 0, "other": 0},
        },
        {
            "model": "sonnet",
            "tasks": 1, "reviews": 0, "re_reviews": 0, "fix_rounds": 1,
            "report_bytes": sizes.with_model,
            "findings": {"critical": 0, "important": 0, "minor": 0, "other": 0},
            "targets": {"code": 0, "deliverable": 0, "prose": 0, "other": 0},
        },
    ]);
    assert_eq!(value["models"], expected);
}

/// A workspace that records one model family under several spellings, and
/// the values that keep their own rows:
///
/// - the reports of `sonnet` and `claude-sonnet-5-5` (the second has a fix
///   round);
/// - the reviews of `opus`, `claude-opus-5-5`, and `claude-opus-5-5[1m]`, each
///   with one finding;
/// - the report of `gpt-5`, the review of `human`, the report of `unknown`, and
///   a report with no `model`.
///
/// Returns the workspace and the sizes of the two reports of the sonnet row.
fn spellings_workspace() -> (TempDir, u64) {
    let workspace = tempfile::tempdir().expect("tempdir");
    let path = workspace.path();
    let alias = write_deliverable(
        path,
        "task-1-report.json",
        &json!({"kind": "task-report", "model": "sonnet", "knowledge_used": []}),
    );
    let id = write_deliverable(
        path,
        "task-2-report.json",
        &json!({
            "kind": "task-report", "model": "claude-sonnet-5-5",
            "knowledge_used": [], "fix_rounds": [{"round": 1}],
        }),
    );
    for (name, model, severity, file) in [
        ("task-1-review.json", "opus", "critical", "crates/a.rs"),
        (
            "task-2-review.json",
            "claude-opus-5-5",
            "important",
            "crates/b.rs",
        ),
        (
            "task-3-review.json",
            "claude-opus-5-5[1m]",
            "minor",
            "README.md",
        ),
    ] {
        write_deliverable(
            path,
            name,
            &json!({
                "kind": "task-review", "model": model, "rule_adherence": [],
                "issues": [issue(severity, file)],
            }),
        );
    }
    write_deliverable(
        path,
        "task-3-report.json",
        &json!({"kind": "task-report", "model": "gpt-5", "knowledge_used": []}),
    );
    write_deliverable(
        path,
        "task-4-review.json",
        &json!({"kind": "task-review", "model": "human", "rule_adherence": [], "issues": []}),
    );
    write_deliverable(
        path,
        "task-4-report.json",
        &json!({"kind": "task-report", "model": "unknown", "knowledge_used": []}),
    );
    write_deliverable(
        path,
        "task-5-report.json",
        &json!({"kind": "task-report", "knowledge_used": []}),
    );
    (workspace, alias + id)
}

/// `stats` groups the `model` of a deliverable by family: the alias and the
/// ids of one family count in one row, named after the family, so a dispatch
/// that names the alias and one that names an id never split a model in two.
/// A value of no family keeps its own row, as `human`, `unknown`, and `pre-2.0`
/// do.
#[test]
fn stats_counts_the_alias_and_the_ids_of_one_family_in_one_row() {
    let install = install_without_backlog();
    let (workspace, sonnet_bytes) = spellings_workspace();
    let value = stats(install.path(), workspace.path());
    let rows = value["models"].as_array().expect("models is an array");
    let names: Vec<&str> = rows
        .iter()
        .map(|row| row["model"].as_str().expect("a model name"))
        .collect();
    assert_eq!(
        names,
        ["gpt-5", "human", "opus", "pre-2.0", "sonnet", "unknown"]
    );
    let row = |name: &str| rows.iter().find(|row| row["model"] == json!(name));
    assert_eq!(
        row("opus"),
        Some(&json!({
            "model": "opus",
            "tasks": 0, "reviews": 3, "re_reviews": 0, "fix_rounds": 0, "report_bytes": 0,
            "findings": {"critical": 1, "important": 1, "minor": 1, "other": 0},
            "targets": {"code": 2, "deliverable": 0, "prose": 1, "other": 0},
        }))
    );
    assert_eq!(
        row("sonnet"),
        Some(&json!({
            "model": "sonnet",
            "tasks": 2, "reviews": 0, "re_reviews": 0, "fix_rounds": 1,
            "report_bytes": sonnet_bytes,
            "findings": {"critical": 0, "important": 0, "minor": 0, "other": 0},
            "targets": {"code": 0, "deliverable": 0, "prose": 0, "other": 0},
        }))
    );
}

/// Spec 4.5: the `workspaces` row counts the spec review as a review, with
/// its findings by severity, as the `models` rows do.
#[test]
fn the_workspace_row_counts_the_spec_review_and_its_findings() {
    let install = install_without_backlog();
    let (workspace, _) = mixed_workspace();
    let value = stats(install.path(), workspace.path());
    let row = &value["workspaces"][0];
    assert_eq!(row["reviews"], json!(5));
    assert_eq!(row["re_reviews"], json!(1));
    assert_eq!(
        row["findings"],
        json!({"critical": 2, "important": 3, "minor": 4, "other": 0})
    );
}

/// The model rows partition the deliverables: each cost field of the rows sums
/// to the same field of `cost.totals`.
#[test]
fn the_model_rows_sum_to_the_cost_totals() {
    let install = install_without_backlog();
    let (workspace, _) = mixed_workspace();
    let value = stats(install.path(), workspace.path());
    let rows = value["models"].as_array().expect("models is an array");
    let sum = |field: &[&str]| -> u64 {
        rows.iter()
            .map(|row| {
                field
                    .iter()
                    .fold(row, |value, key| &value[*key])
                    .as_u64()
                    .expect("a count")
            })
            .sum()
    };
    let total = |field: &[&str]| -> u64 {
        field
            .iter()
            .fold(&value["cost"]["totals"], |value, key| &value[*key])
            .as_u64()
            .expect("a count")
    };
    for field in [
        vec!["tasks"],
        vec!["reviews"],
        vec!["re_reviews"],
        vec!["fix_rounds"],
        vec!["report_bytes"],
        vec!["findings", "critical"],
        vec!["findings", "important"],
        vec!["findings", "minor"],
        vec!["findings", "other"],
        vec!["targets", "code"],
        vec!["targets", "deliverable"],
        vec!["targets", "prose"],
        vec!["targets", "other"],
    ] {
        assert_eq!(sum(&field), total(&field), "{field:?}");
    }
}

/// A workspace with no deliverable has no model row: `models` is an empty
/// list, never a row of zeros.
#[test]
fn an_empty_workspace_has_no_model_rows() {
    let install = install_without_backlog();
    let workspace = tempfile::tempdir().expect("tempdir");
    let value = stats(install.path(), workspace.path());
    assert_eq!(value["models"], json!([]));
}

/// A `model` that is no string is a named error with the file, as a field of
/// the wrong type is elsewhere in `stats`; it is never read as `pre-2.0`.
#[test]
fn a_model_that_is_no_non_empty_string_is_a_named_error() {
    let install = install_without_backlog();
    for bad in [json!(7), json!(""), json!(["claude"])] {
        let workspace = tempfile::tempdir().expect("tempdir");
        write_deliverable(
            workspace.path(),
            "task-1-report.json",
            &json!({"kind": "task-report", "model": bad, "knowledge_used": []}),
        );
        let output = run(
            install.path(),
            &["stats", workspace.path().to_str().expect("a UTF-8 path")],
        );
        assert_eq!(output.status.code(), Some(2), "{bad}");
        assert_eq!(
            stderr(&output),
            "task-1-report.json: model is not a non-empty string\n",
            "{bad}"
        );
    }
}

/// Spec 4.5: `spec-review-r<n>.json` is a deliverable file, so one that is no
/// JSON stops `stats` with the remedy, which names the file's pattern.
#[test]
fn an_unreadable_spec_review_gets_the_remedy_that_names_its_pattern() {
    let install = install_without_backlog();
    let workspace = tempfile::tempdir().expect("tempdir");
    let broken = workspace.path().join("spec-review-r1.json");
    fs::write(&broken, "not json").expect("write");
    let output = run(
        install.path(),
        &["stats", workspace.path().to_str().expect("a UTF-8 path")],
    );
    assert_eq!(output.status.code(), Some(2));
    let error = stderr(&output);
    assert!(error.contains("spec-review-r1.json"), "{error}");
    assert!(error.ends_with(&format!(". {REMEDY}\n")), "{error}");
}

/// Spec 4.5 and C28: `stats` reads a spec review only from `spec-review.json`
/// and `spec-review-r<n>.json`. A probe capture that is named after a review is
/// evidence, not a deliverable: a file that is no JSON stops nothing, and a
/// capture of a JSON array counts as no review.
#[test]
fn a_probe_capture_named_after_a_spec_review_is_no_deliverable() {
    let install = install_without_backlog();
    let workspace = tempfile::tempdir().expect("tempdir");
    write_deliverable(
        workspace.path(),
        "spec-review.json",
        &json!({
            "kind": "spec-review", "model": "claude-opus-5-5",
            "findings": [spec_finding("minor")],
        }),
    );
    for capture in [
        "spec-review-probe-gh.json",
        "spec-review-r2-probe-list-open.json",
    ] {
        fs::write(workspace.path().join(capture), "not json").expect("write");
    }
    fs::write(
        workspace.path().join("spec-review-r1-probe-list.json"),
        "[{\"id\": \"S1\"}]",
    )
    .expect("write");
    let value = stats(install.path(), workspace.path());
    assert_eq!(value["workspaces"][0]["reviews"], json!(1));
    let models = value["models"].as_array().expect("models is an array");
    assert_eq!(models.len(), 1, "{models:?}");
    assert_eq!(models[0]["model"], json!("opus"));
    assert_eq!(models[0]["reviews"], json!(1));
}

/// A workspace whose only files are probe captures of a spec review has no
/// deliverable: no review and no model row.
#[test]
fn probe_captures_alone_leave_the_workspace_without_a_review() {
    let install = install_without_backlog();
    let workspace = tempfile::tempdir().expect("tempdir");
    fs::write(
        workspace.path().join("spec-review-probe-gh.json"),
        "not json",
    )
    .expect("write");
    fs::write(
        workspace.path().join("spec-review-r1-probe-list.json"),
        "[{\"id\": \"S1\"}]",
    )
    .expect("write");
    let value = stats(install.path(), workspace.path());
    assert_eq!(value["workspaces"][0]["reviews"], json!(0));
    assert_eq!(value["models"], json!([]));
}

/// The remedy names the files the same way for every kind of unreadable
/// file: one list gives the text, so a file kind added to the list shows in
/// every message.
#[test]
fn the_remedy_is_the_same_for_every_unreadable_file() {
    let install = install_without_backlog();
    for name in [
        "task-1-audit.json",
        "task-1-review.json",
        "task-1-report.json",
        "spec-review.json",
        "spec-review-r2.json",
        "branch-review.json",
    ] {
        let workspace = tempfile::tempdir().expect("tempdir");
        fs::write(workspace.path().join(name), "not json").expect("write");
        let output = run(
            install.path(),
            &["stats", workspace.path().to_str().expect("a UTF-8 path")],
        );
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(
            stderr(&output).ends_with(&format!(". {REMEDY}\n")),
            "{name}: {}",
            stderr(&output)
        );
    }
}

/// The workspace of README's sample `models` row: one task review that a model
/// wrote, with an important finding on code and a minor one on prose.
fn models_sample_workspace() -> TempDir {
    let workspace = tempfile::tempdir().expect("tempdir");
    write_deliverable(
        workspace.path(),
        "task-1-review.json",
        &json!({
            "kind": "task-review", "model": "claude-opus-5-5", "rule_adherence": [],
            "issues": [issue("important", "crates/a.rs"), issue("minor", "README.md")],
        }),
    );
    workspace
}

/// The rule of README's sample `proposals` row. The kit's process pack ships
/// it.
const PROPOSED_RULE: &str = "process.claims-match-artifacts";

/// The workspace of README's sample `proposals` row: one task review with four
/// minor findings on prose, each of which cites `PROPOSED_RULE`.
fn proposals_sample_workspace() -> TempDir {
    let workspace = tempfile::tempdir().expect("tempdir");
    let issues: Vec<Value> = (0..4)
        .map(|_| {
            let mut finding = issue("minor", "README.md");
            finding["rule"] = json!(PROPOSED_RULE);
            finding
        })
        .collect();
    write_deliverable(
        workspace.path(),
        "task-1-review.json",
        &json!({
            "kind": "task-review", "model": "claude-opus-5-5", "rule_adherence": [],
            "issues": issues,
        }),
    );
    workspace
}

/// Asserts that README holds `slice` as a fenced JSON block, byte for byte.
fn assert_readme_holds(slice: &Value) {
    let slice = serde_json::to_string_pretty(slice).expect("serialize the slice");
    let expected = format!("```json\n{slice}\n```\n");
    let readme = fs::read_to_string(repo_root().join("README.md"))
        .expect("read README.md")
        .replace("\r\n", "\n");
    assert!(
        readme.contains(&expected),
        "README.md holds no block equal to the generated slice:\n{expected}"
    );
}

/// HR-165 (1): README's sample `models` row is the frozen slice of what
/// `stats` prints for the workspace of `models_sample_workspace`, byte for
/// byte.
#[test]
fn the_readme_stats_slice_equals_the_generated_slice() {
    let install = install_without_backlog();
    let workspace = models_sample_workspace();
    let value = stats(install.path(), workspace.path());
    assert_readme_holds(&value["models"][0]);
}

/// HR-165 (1): README's sample `proposals` row is the frozen slice of what
/// `stats` prints for the workspace of `proposals_sample_workspace`, byte for
/// byte. The slice is the `narrow` row. The standing budget of the kit's own
/// rules adds a `budget` row that the sample leaves out.
#[test]
fn the_readme_proposals_slice_equals_the_generated_slice() {
    let install = install_with_the_process_pack_and_no_tracker();
    let workspace = proposals_sample_workspace();
    let value = stats(install.path(), workspace.path());
    let rows = value["proposals"]
        .as_array()
        .expect("proposals is an array");
    let narrow = rows
        .iter()
        .find(|row| row["action"] == json!("narrow") && row["id"] == json!(PROPOSED_RULE))
        .unwrap_or_else(|| panic!("no narrow proposal for {PROPOSED_RULE}: {rows:?}"));
    assert_readme_holds(narrow);
}
