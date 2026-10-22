//! Report-field content, the `self_audit` comparison, and sanctions on judged
//! rules (batch 35, spec sections 5 and 6, acceptance cases 5 and 6).
//!
//! Every case runs the compiled binary against a scratch git repository that
//! `init --tracker json` seeded, so the entries under test are the kit's own:
//! `process.knowledge-first`, `process.deliverables-json`, and
//! `security-hygiene.verify-current-docs`. A few cases add the project's own
//! `fail`-level entries beside them. A report lives in a scratch folder
//! outside the repository, so that no commit of a case ever holds it.
//!
//! A test whose doc comment calls it a pin asserts behavior that the code
//! held before the change the file pins; every other test asserts the change.

mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    commit_all, git, read_json, run, run_ok, scratch_git_repo, stderr, stdout, write_json,
};

/// The kit entry whose check reads `knowledge_used`.
const KNOWLEDGE_FIRST: &str = "process.knowledge-first";

/// The kit entry whose check reads `docs_verified`.
const VERIFY_DOCS: &str = "security-hygiene.verify-current-docs";

/// The kit entry whose check reads `self_audit`.
const DELIVERABLES: &str = "process.deliverables-json";

/// The kit entry whose check reads the commit subjects.
const CONVENTIONAL: &str = "process.conventional-commits";

/// The project's own `fail`-level entry that requires a non-empty
/// `docs_verified`.
const TEAM_DOCS: &str = "team.docs-content";

/// The project's own `fail`-level entry that requires a non-empty
/// `knowledge_used` that covers `--ids`.
const TEAM_KNOWLEDGE: &str = "team.knowledge-content";

/// A judged rule of the writing-style pack: it has no check.
const JUDGED_RULE: &str = "writing-style.code-comments";

/// A scratch repository with the kit seeded, and a folder for the reports of
/// a case.
struct Scratch {
    repo: TempDir,
    reports: TempDir,
    /// The base commit of the audits: the commit that holds the kit.
    base: String,
}

impl Scratch {
    /// The repository root.
    fn root(&self) -> &Path {
        self.repo.path()
    }

    /// The path of the report file `name` in the report folder.
    fn report_path(&self, name: &str) -> PathBuf {
        self.reports.path().join(name)
    }

    /// Writes `report` to `name` in the report folder and returns the path.
    fn write_report(&self, name: &str, report: &Value) -> PathBuf {
        let path = self.report_path(name);
        fs::write(
            &path,
            serde_json::to_string_pretty(report).expect("serialize the report"),
        )
        .expect("write the report");
        path
    }

    /// Writes `content` to `relative` below the repository root and commits
    /// it with `message`. Returns the full sha.
    fn commit_file(&self, relative: &str, content: &str, message: &str) -> String {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().expect("a parent folder")).expect("create folders");
        fs::write(path, content).expect("write the file");
        commit_all(self.root(), message)
    }

    /// The full sha of `HEAD`, which is the commit that `scratch` makes after
    /// its base: it changes `src/a.rs`. Call it before any other commit.
    fn head_of_the_first_change(&self) -> String {
        git(self.root(), &["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Runs `audit --base <base> --report <report>` with `extra` arguments.
    /// Returns the output and the parsed JSON result.
    fn audit_report(&self, base: &str, report: &Path, extra: &[&str]) -> (Output, Value) {
        let report = report.to_str().expect("a utf-8 path");
        let mut args = vec!["audit", "--base", base, "--report", report];
        args.extend_from_slice(extra);
        self.audit(&args)
    }

    /// Runs `audit --base <base> --workspace <folder>` with `extra` arguments.
    fn audit_workspace(&self, base: &str, extra: &[&str]) -> (Output, Value) {
        let folder = self.reports.path().to_str().expect("a utf-8 path");
        let mut args = vec!["audit", "--base", base, "--workspace", folder];
        args.extend_from_slice(extra);
        self.audit(&args)
    }

    /// Runs `houserules` with `args` in the repository and parses the JSON
    /// result of stdout.
    fn audit(&self, args: &[&str]) -> (Output, Value) {
        let output = run(self.root(), args);
        let result = serde_json::from_str(&stdout(&output)).unwrap_or_else(|error| {
            panic!(
                "audit prints JSON ({error}): stdout {}, stderr {}",
                stdout(&output),
                stderr(&output)
            )
        });
        (output, result)
    }
}

/// The two entries of the project that check the content of a report field at
/// level `fail`, with the options of spec 5.
fn team_topic() -> Value {
    json!({
        "topic": "team",
        "title": "Team rules",
        "entries": [
            {
                "id": TEAM_DOCS, "kind": "rule", "area": "global", "standing": true,
                "summary": "The report names the docs that the author verified.",
                "body": ["Record each docs source in `docs_verified`."], "tags": [],
                "source": {"date": "2026-10-22", "by": "controller"},
                "check": {
                    "type": "report-field", "level": "fail", "if": "**/*.rs",
                    "field": "docs_verified", "non_empty": true,
                },
            },
            {
                "id": TEAM_KNOWLEDGE, "kind": "rule", "area": "global", "standing": true,
                "summary": "The report cites every id of the dispatch.",
                "body": ["Record each id in `knowledge_used`."], "tags": [],
                "source": {"date": "2026-10-22", "by": "controller"},
                "check": {
                    "type": "report-field", "level": "fail", "if": "**",
                    "field": "knowledge_used", "non_empty": true, "covers_ids": true,
                },
            },
        ],
    })
}

/// A scratch repository with the kit seeded by `init --tracker json`, the four
/// packs that `init` leaves out, and the `team_topic` entries when `with_team`
/// is set. One commit holds them all (the base). Then one commit changes
/// `src/a.rs`.
fn scratch(with_team: bool) -> Scratch {
    scratch_with(|root| {
        if with_team {
            write_json(root, "knowledge/team.json", &team_topic());
        }
    })
}

/// A scratch repository as `scratch` makes it, with the files that `prepare`
/// writes below the root held by the base commit.
fn scratch_with(prepare: impl FnOnce(&Path)) -> Scratch {
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init", "--tracker", "json"]);
    for pack in ["process", "quality", "security-hygiene", "writing-style"] {
        run_ok(repo.path(), &["pack", "add", pack]);
    }
    prepare(repo.path());
    let base = commit_all(repo.path(), "chore: install the kit");
    let scratch = Scratch {
        repo,
        reports: tempfile::tempdir().expect("tempdir"),
        base,
    };
    scratch.commit_file("src/a.rs", "fn a() {}\n", "feat: add the lookup");
    scratch
}

/// A report that is a draft of the self-audit and holds a value for every
/// field that the kit checks.
fn draft(knowledge_used: Value, docs_verified: Value) -> Value {
    json!({
        "kind": "task-report",
        "knowledge_used": knowledge_used,
        "docs_verified": docs_verified,
        "self_audit": null,
    })
}

/// The row of `result` for the entry `id`.
fn row<'a>(result: &'a Value, id: &str) -> &'a Value {
    result["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|row| row["id"] == json!(id))
        .unwrap_or_else(|| panic!("the result has no row for {id}"))
}

/// A deterministic row of a `report-field` check as the audit prints it.
fn field_row(id: &str, level: &str, result: &str, evidence: &str) -> Value {
    json!({
        "id": id, "kind": "rule", "mode": "deterministic", "level": level,
        "result": result, "evidence": evidence,
    })
}

/// The copy of `result` that a task report holds as `self_audit`: the summary
/// and the deterministic rows of the audit.
fn copy_of(result: &Value) -> Value {
    let rows: Vec<Value> = result["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .filter(|row| row["mode"] == json!("deterministic"))
        .cloned()
        .collect();
    json!({"summary": result["summary"], "rows": rows})
}

/// The report `base` with `copy` as its `self_audit`.
fn with_copy(base: &Value, copy: &Value) -> Value {
    let mut report = base.clone();
    report["self_audit"] = copy.clone();
    report
}

/// The index of the row `id` in the `rows` array of `copy`.
fn row_index(copy: &Value, id: &str) -> usize {
    copy["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .position(|row| row["id"] == json!(id))
        .unwrap_or_else(|| panic!("the copy has no row for {id}"))
}

// ---- spec 5: non_empty and covers_ids on the kit entries (case 5) ----

/// Case 5: an empty `docs_verified` is a `warn` row of the kit entry, which
/// fails no audit.
#[test]
fn an_empty_docs_verified_is_a_warn_row_and_the_audit_exits_0() {
    let scratch = scratch(false);
    let report = scratch.write_report("report.json", &draft(json!(["process.tdd"]), json!([])));
    let (output, result) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&result, VERIFY_DOCS),
        &field_row(
            VERIFY_DOCS,
            "warn",
            "warn",
            "report field docs_verified is empty"
        )
    );
    assert_eq!(result["summary"]["fail"], json!(0));
    assert_eq!(result["summary"]["warn"], json!(1));
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Case 5: a `knowledge_used` that lacks an id of `--ids` is a `warn` row
/// that names the first missing id, in the order of `--ids`.
#[test]
fn a_knowledge_used_that_lacks_an_id_of_ids_is_a_warn_row() {
    let scratch = scratch(false);
    let report = scratch.write_report(
        "report.json",
        &draft(json!(["process.tdd", "quality.principles"]), json!(["x"])),
    );
    let (output, result) = scratch.audit_report(
        &scratch.base,
        &report,
        &[
            "--ids",
            "process.tdd,writing-style.principles,quality.no-compat-softening,quality.principles",
        ],
    );
    assert_eq!(
        row(&result, KNOWLEDGE_FIRST),
        &field_row(
            KNOWLEDGE_FIRST,
            "warn",
            "warn",
            "report field knowledge_used lacks writing-style.principles of --ids"
        )
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Case 5: an empty `knowledge_used` is a `warn` row of the kit entry, and
/// the empty value is named before any id of `--ids`.
#[test]
fn an_empty_knowledge_used_is_a_warn_row() {
    let scratch = scratch(false);
    let report = scratch.write_report("report.json", &draft(json!([]), json!(["x"])));
    let (output, result) = scratch.audit_report(&scratch.base, &report, &["--ids", "process.tdd"]);
    assert_eq!(
        row(&result, KNOWLEDGE_FIRST),
        &field_row(
            KNOWLEDGE_FIRST,
            "warn",
            "warn",
            "report field knowledge_used is empty"
        )
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Pin of unchanged behavior: a `--report` row of a field that is set passes
/// with the evidence it had before. A `knowledge_used` that covers `--ids`
/// and a `docs_verified` with an entry are such fields.
#[test]
fn a_set_field_that_covers_the_ids_still_passes() {
    let scratch = scratch(false);
    let report = scratch.write_report(
        "report.json",
        &draft(json!(["process.tdd", "quality.principles"]), json!(["x"])),
    );
    let (output, result) = scratch.audit_report(
        &scratch.base,
        &report,
        &["--ids", "process.tdd,quality.principles"],
    );
    assert_eq!(
        row(&result, KNOWLEDGE_FIRST),
        &field_row(
            KNOWLEDGE_FIRST,
            "warn",
            "pass",
            "report field knowledge_used is set"
        )
    );
    assert_eq!(
        row(&result, VERIFY_DOCS),
        &field_row(
            VERIFY_DOCS,
            "warn",
            "pass",
            "report field docs_verified is set"
        )
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Pin of unchanged behavior: without `--ids`, `covers_ids` passes. The kit
/// entry holds the option, and a one-id `knowledge_used` still passes.
#[test]
fn covers_ids_passes_without_ids() {
    let scratch = scratch(false);
    let report = scratch.write_report("report.json", &draft(json!(["process.tdd"]), json!(["x"])));
    let (_, result) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&result, KNOWLEDGE_FIRST),
        &field_row(
            KNOWLEDGE_FIRST,
            "warn",
            "pass",
            "report field knowledge_used is set"
        )
    );
}

/// Case 5: the same inputs on checks of level `fail` give `fail` rows with
/// the same evidence, and the audit exits 1.
#[test]
fn the_same_inputs_on_a_fail_level_check_give_fail_rows() {
    let scratch = scratch(true);
    let report = scratch.write_report("report.json", &draft(json!(["process.tdd"]), json!([])));
    let (output, result) = scratch.audit_report(
        &scratch.base,
        &report,
        &["--ids", "process.tdd,quality.principles"],
    );
    assert_eq!(
        row(&result, TEAM_DOCS),
        &field_row(
            TEAM_DOCS,
            "fail",
            "fail",
            "report field docs_verified is empty"
        )
    );
    assert_eq!(
        row(&result, TEAM_KNOWLEDGE),
        &field_row(
            TEAM_KNOWLEDGE,
            "fail",
            "fail",
            "report field knowledge_used lacks quality.principles of --ids"
        )
    );
    assert_eq!(result["summary"]["fail"], json!(2));
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}

// ---- spec 5: the --workspace evidence and modes (case 5) ----

/// Writes the reports `task-1-report.json` and `task-2-report.json` with the
/// given `files_changed`, `knowledge_used`, and `docs_verified`.
fn write_two_reports(
    scratch: &Scratch,
    first: (&[&str], Value, Value),
    second: (&[&str], Value, Value),
) {
    for (name, (files, knowledge_used, docs_verified)) in [
        ("task-1-report.json", first),
        ("task-2-report.json", second),
    ] {
        scratch.write_report(
            name,
            &json!({
                "kind": "task-report",
                "files_changed": files,
                "knowledge_used": knowledge_used,
                "docs_verified": docs_verified,
                "self_audit": null,
            }),
        );
    }
}

/// HR-176 and case 5: a `--workspace` row counts the reports that the check
/// triggered and the reports that the audit read. One of two reports names a
/// Rust file, so one triggers the check.
#[test]
fn a_workspace_row_says_set_in_triggered_of_read_reports() {
    let scratch = scratch(false);
    write_two_reports(
        &scratch,
        (&["src/a.rs"], json!(["process.tdd"]), json!(["x"])),
        (&["docs/notes.md"], json!(["process.tdd"]), json!(null)),
    );
    let (_, result) = scratch.audit_workspace(&scratch.base, &[]);
    assert_eq!(
        row(&result, VERIFY_DOCS),
        &field_row(
            VERIFY_DOCS,
            "warn",
            "pass",
            "report field docs_verified is set in 1 of 2 reports"
        )
    );
}

/// Spec 6: in a `--workspace` run `non_empty` applies to each report that the
/// check triggers, and the first report whose value is empty fails the row
/// with its name.
#[test]
fn a_workspace_non_empty_check_names_the_first_report_with_an_empty_value() {
    let scratch = scratch(false);
    write_two_reports(
        &scratch,
        (&["src/a.rs"], json!(["process.tdd"]), json!(["x"])),
        (&["src/b.rs"], json!([]), json!([])),
    );
    let (_, result) = scratch.audit_workspace(&scratch.base, &[]);
    assert_eq!(
        row(&result, VERIFY_DOCS),
        &field_row(
            VERIFY_DOCS,
            "warn",
            "warn",
            "task-2-report.json: report field docs_verified is empty"
        )
    );
    assert_eq!(
        row(&result, KNOWLEDGE_FIRST),
        &field_row(
            KNOWLEDGE_FIRST,
            "warn",
            "warn",
            "task-2-report.json: report field knowledge_used is empty"
        )
    );
}

/// Spec 6, C12: `covers_ids` does not run in a `--workspace` run, because each
/// report has its own `--ids`. The row passes with the count line of spec 5 and
/// `; covers_ids needs --report` after it, so that HR-176's count stays on every
/// row, whatever `--ids` holds.
#[test]
fn a_workspace_covers_ids_check_passes_with_its_own_evidence() {
    let scratch = scratch(false);
    write_two_reports(
        &scratch,
        (&["src/a.rs"], json!(["process.tdd"]), json!(["x"])),
        (&["src/b.rs"], json!(["process.tdd"]), json!(["x"])),
    );
    let (_, result) =
        scratch.audit_workspace(&scratch.base, &["--ids", "process.tdd,quality.principles"]);
    assert_eq!(
        row(&result, KNOWLEDGE_FIRST),
        &field_row(
            KNOWLEDGE_FIRST,
            "warn",
            "pass",
            "report field knowledge_used is set in 2 of 2 reports; covers_ids needs --report"
        )
    );
}

// ---- spec 6: the self_audit comparison (case 6) ----

/// A scratch repository whose range holds a commit with a subject that
/// breaks the conventional-commits check and a commit with a `Work-Item:`
/// trailer that no tracker item answers. The first audit of that range, run
/// with `--sanctioned process.conventional-commits=interim`, has a sanctioned
/// fail row and `reference_fail` 1.
fn scratch_with_a_sanctioned_fail_and_a_failed_reference() -> Scratch {
    let scratch = scratch(false);
    commit_all(scratch.root(), "bad subject line");
    commit_all(scratch.root(), "feat: add the cache\n\nWork-Item: WI-099");
    scratch
}

/// The `--sanctioned` argument of the cases that book the bad subject.
const SANCTION: [&str; 2] = ["--sanctioned", "process.conventional-commits=interim"];

/// Pin of unchanged behavior: a draft report (`self_audit: null`) passes the
/// check of `self_audit` in a `--report` run.
#[test]
fn a_draft_self_audit_passes() {
    let scratch = scratch(false);
    let report = scratch.write_report("report.json", &draft(json!(["process.tdd"]), json!(["x"])));
    let (output, result) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&result, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "fail",
            "pass",
            "report field self_audit is present"
        )
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Case 6: a report whose `self_audit` is the copy of the audit of the same
/// range, with its copied `reference_fail` and a `--sanctioned` row, passes
/// the comparison: the second audit equals the first in every row and count.
/// The sanctioned row carries the structured `sanctioned` field (spec 6).
#[test]
fn a_matching_copy_with_a_failed_reference_and_a_sanctioned_row_passes() {
    let scratch = scratch_with_a_sanctioned_fail_and_a_failed_reference();
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &SANCTION);
    assert_eq!(first["summary"]["reference_fail"], json!(1));
    assert_eq!(first["summary"]["sanctioned_fail"], json!(1));
    assert_eq!(row(&first, CONVENTIONAL)["sanctioned"], json!("interim"));

    scratch.write_report("report.json", &with_copy(&base, &copy_of(&first)));
    let (output, second) = scratch.audit_report(&scratch.base, &report, &SANCTION);
    assert_eq!(second["rules"], first["rules"], "no row moved");
    assert_eq!(second["summary"], first["summary"], "no count moved");
    assert_eq!(
        row(&second, DELIVERABLES)["evidence"],
        json!("report field self_audit is present")
    );
    assert_eq!(
        output.status.code(),
        Some(1),
        "the sanctioned fail and the failed reference still fail the run"
    );
}

/// Case 6: a row of the copy that differs from the run fails the check with
/// the JSON path of the difference. The copy stands for the hand-reworded row
/// of batch 27. The check moves from `pass` to `fail` in the summary.
#[test]
fn a_row_that_differs_fails_with_its_path() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (first_output, first) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(first_output.status.code(), Some(0));

    let mut copy = copy_of(&first);
    let index = row_index(&copy, CONVENTIONAL);
    copy["rows"][index]["evidence"] = json!("2 commits checked, all fine");
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (output, second) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&second, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "fail",
            "fail",
            &format!("self_audit differs at self_audit.rows[{index}].evidence")
        )
    );
    assert_eq!(
        second["summary"]["pass"],
        json!(first["summary"]["pass"].as_u64().unwrap() - 1)
    );
    assert_eq!(second["summary"]["fail"], json!(1));
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}

/// Case 6: a count of the summary that differs is named by its key. The
/// comparison runs after the reference verdict, so `reference_fail` is part
/// of the compared set: a copy with a stale count fails.
#[test]
fn a_summary_count_that_differs_is_named_by_its_key() {
    let scratch = scratch_with_a_sanctioned_fail_and_a_failed_reference();
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &SANCTION);

    let mut stale = copy_of(&first);
    stale["summary"]["reference_fail"] = json!(0);
    scratch.write_report("report.json", &with_copy(&base, &stale));
    let (_, second) = scratch.audit_report(&scratch.base, &report, &SANCTION);
    assert_eq!(
        row(&second, DELIVERABLES)["evidence"],
        json!("self_audit differs at self_audit.summary.reference_fail")
    );

    let mut lacking = copy_of(&first);
    lacking["summary"]
        .as_object_mut()
        .expect("summary")
        .remove("reference_fail");
    scratch.write_report("report.json", &with_copy(&base, &lacking));
    let (_, third) = scratch.audit_report(&scratch.base, &report, &SANCTION);
    assert_eq!(
        row(&third, DELIVERABLES)["evidence"],
        json!("self_audit differs at self_audit.summary.reference_fail")
    );
}

/// Case 6: a copy that lacks a row of the run, or holds one more, fails with
/// the path of the first row that has no partner.
#[test]
fn a_copy_with_a_missing_or_an_extra_row_fails_at_the_row() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    let rows = copy_of(&first)["rows"].as_array().expect("rows").len();

    let mut shorter = copy_of(&first);
    shorter["rows"].as_array_mut().expect("rows").pop();
    scratch.write_report("report.json", &with_copy(&base, &shorter));
    let (_, second) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&second, DELIVERABLES)["evidence"],
        json!(format!(
            "self_audit differs at self_audit.rows[{}]",
            rows - 1
        ))
    );

    let mut longer = copy_of(&first);
    let extra = longer["rows"][0].clone();
    longer["rows"].as_array_mut().expect("rows").push(extra);
    scratch.write_report("report.json", &with_copy(&base, &longer));
    let (_, third) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&third, DELIVERABLES)["evidence"],
        json!(format!("self_audit differs at self_audit.rows[{rows}]"))
    );
}

/// Case 6: recorded short shas that a repack lengthened still compare. The
/// repository prints 12-character shas, the copy holds the 7-character
/// prefixes that an earlier run printed, and the check compares the commits
/// the shas name. An equal copy passes; a copy whose row differs fails at the
/// row, not with the not-compared evidence.
#[test]
fn recorded_short_shas_that_a_repack_lengthened_still_compare() {
    let scratch = scratch(false);
    git(scratch.root(), &["config", "core.abbrev", "12"]);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    let long_base = first["summary"]["base"].as_str().expect("base").to_string();
    let long_head = first["summary"]["head"].as_str().expect("head").to_string();
    assert_eq!(long_base.len(), 12, "the repository prints 12 characters");

    let mut copy = copy_of(&first);
    copy["summary"]["base"] = json!(long_base[..7]);
    copy["summary"]["head"] = json!(long_head[..7]);
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (_, equal) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&equal, DELIVERABLES)["evidence"],
        json!("report field self_audit is present")
    );
    assert_eq!(row(&equal, DELIVERABLES)["result"], json!("pass"));

    let index = row_index(&copy, CONVENTIONAL);
    copy["rows"][index]["result"] = json!("fail");
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (_, differing) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&differing, DELIVERABLES)["evidence"],
        json!(format!(
            "self_audit differs at self_audit.rows[{index}].result"
        ))
    );
}

/// Case 6: a re-review run from another base does not compare. The row passes
/// and its evidence adds the two ranges.
#[test]
fn a_run_from_another_base_prints_the_not_compared_evidence() {
    let scratch = scratch(false);
    let first_change = scratch.head_of_the_first_change();
    scratch.commit_file("src/b.rs", "fn b() {}\n", "feat: add the cache");
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);

    scratch.write_report("report.json", &with_copy(&base, &copy_of(&first)));
    let (output, other) = scratch.audit_report(&first_change, &report, &[]);
    let recorded = format!(
        "{}..{}",
        first["summary"]["base"].as_str().expect("base"),
        first["summary"]["head"].as_str().expect("head")
    );
    let this_run = format!(
        "{}..{}",
        other["summary"]["base"].as_str().expect("base"),
        other["summary"]["head"].as_str().expect("head")
    );
    assert_ne!(recorded, this_run);
    assert_eq!(
        row(&other, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "fail",
            "pass",
            &format!(
                "report field self_audit is present; self_audit covers {recorded}; \
                 this run covers {this_run}: not compared"
            )
        )
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Case 6: a recorded sha that names no commit gives the not-compared
/// evidence too, with the recorded text as written.
#[test]
fn a_recorded_sha_that_names_no_commit_prints_the_not_compared_evidence() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);

    let mut copy = copy_of(&first);
    copy["summary"]["base"] = json!("deadbee");
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (output, second) = scratch.audit_report(&scratch.base, &report, &[]);
    let head = first["summary"]["head"].as_str().expect("head");
    let range = format!(
        "{}..{head}",
        first["summary"]["base"].as_str().expect("base")
    );
    assert_eq!(
        row(&second, DELIVERABLES)["evidence"],
        json!(format!(
            "report field self_audit is present; self_audit covers deadbee..{head}; \
             this run covers {range}: not compared"
        ))
    );
    assert_eq!(row(&second, DELIVERABLES)["result"], json!("pass"));
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Case 6, `quality.absence-is-designed`: a copy whose summary holds no sha, or
/// holds text that is no sha, prints the designed token `none` for the missing
/// one and the text as written for the other. The sha text `--help` never
/// reaches `git` as an option.
#[test]
fn a_copy_without_a_sha_prints_the_designed_token_for_it() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    let this_run = format!(
        "{}..{}",
        first["summary"]["base"].as_str().expect("base"),
        first["summary"]["head"].as_str().expect("head")
    );

    let mut copy = copy_of(&first);
    copy["summary"]
        .as_object_mut()
        .expect("summary")
        .remove("base");
    copy["summary"]["head"] = json!("--help");
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (output, second) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&second, DELIVERABLES)["evidence"],
        json!(format!(
            "report field self_audit is present; self_audit covers none..--help; \
             this run covers {this_run}: not compared"
        ))
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Case 6: when the comparing check has level `warn`, a difference gives a
/// `warn` row, the `warn` count takes the row from the `pass` count, and the
/// audit exits 0.
#[test]
fn a_comparing_check_of_level_warn_gives_a_warn_row() {
    let scratch = scratch_with(|root| {
        let mut overrides = read_json(root, "knowledge/overrides.json");
        overrides["schema"] = json!("houserules/overrides@1.1.0");
        overrides["overrides"]
            .as_array_mut()
            .expect("the overrides")
            .push(json!({"id": DELIVERABLES, "fields": {"check": {
                "type": "report-field", "level": "warn", "if": "**", "field": "self_audit",
            }}}));
        write_json(root, "knowledge/overrides.json", &overrides);
    });
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(first["summary"]["warn"], json!(0));

    let mut copy = copy_of(&first);
    let index = row_index(&copy, CONVENTIONAL);
    copy["rows"][index]["evidence"] = json!("hand-reworded");
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (output, second) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&second, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "warn",
            "warn",
            &format!("self_audit differs at self_audit.rows[{index}].evidence")
        )
    );
    assert_eq!(second["summary"]["warn"], json!(1));
    assert_eq!(second["summary"]["fail"], json!(0));
    assert_eq!(
        second["summary"]["pass"].as_u64(),
        first["summary"]["pass"].as_u64().map(|pass| pass - 1)
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Case 6: the compared set leaves out the comparing row and the summary
/// counts it moves. A failed run is copied back into the report, as an author
/// does who fixes the stale copy by copying again, and the next run passes.
#[test]
fn a_failed_run_copied_back_into_the_report_passes_the_next_run() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);

    let mut stale = copy_of(&first);
    let index = row_index(&stale, CONVENTIONAL);
    stale["rows"][index]["evidence"] = json!("hand-reworded");
    scratch.write_report("report.json", &with_copy(&base, &stale));
    let (_, failed) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(row(&failed, DELIVERABLES)["result"], json!("fail"));
    assert_eq!(failed["summary"]["fail"], json!(1));

    scratch.write_report("report.json", &with_copy(&base, &copy_of(&failed)));
    let (output, converged) = scratch.audit_report(&scratch.base, &report, &[]);
    assert_eq!(
        row(&converged, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "fail",
            "pass",
            "report field self_audit is present"
        )
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Audits the scratch repository with `SANCTION` and a draft report, puts the
/// copy of that audit into the report with `edit` applied to it, and audits
/// again with `SANCTION` and `extra`. Returns the output and the result of the
/// second audit.
fn audit_with_an_edited_copy(
    scratch: &Scratch,
    edit: impl FnOnce(&mut Value),
    extra: &[&str],
) -> (Output, Value) {
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &SANCTION);
    let mut copy = copy_of(&first);
    edit(&mut copy);
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let mut args = SANCTION.to_vec();
    args.extend_from_slice(extra);
    scratch.audit_report(&scratch.base, &report, &args)
}

/// An edit of the `summary` of a copy.
type SummaryEdit = fn(&mut Value);

/// Case 6: a copy whose summary counts contradict its own rows and the run
/// fails at the count. The comparison takes the result of the comparing row out
/// of each side's count and compares the four counts like any other key. The
/// first case is the review's reproduction: the summary moves the sanctioned
/// fail into `pass`, and every row of the copy is the true row.
#[test]
fn a_copy_whose_summary_counts_contradict_the_run_differs_at_the_count() {
    let scratch = scratch(false);
    commit_all(scratch.root(), "bad subject line");
    let edits: [(&str, SummaryEdit); 4] = [
        ("pass", |summary| {
            let fail = summary["fail"].as_u64().expect("fail");
            summary["pass"] = json!(summary["pass"].as_u64().expect("pass") + fail);
            summary["fail"] = json!(0);
        }),
        ("fail", |summary| summary["fail"] = json!(0)),
        ("warn", |summary| summary["warn"] = json!(1)),
        ("skipped", |summary| summary["skipped"] = json!(1)),
    ];
    for (key, edit) in edits {
        let (output, result) =
            audit_with_an_edited_copy(&scratch, |copy| edit(&mut copy["summary"]), &[]);
        assert_eq!(
            row(&result, DELIVERABLES),
            &field_row(
                DELIVERABLES,
                "fail",
                "fail",
                &format!("self_audit differs at self_audit.summary.{key}")
            ),
            "a copy that changes {key}"
        );
        assert_eq!(output.status.code(), Some(1), "a copy that changes {key}");
    }
}

/// Case 6: a copy that lacks the comparing row has nothing to take out of its
/// counts. Its summary still counts the row, so the count differs from the
/// run's count without the row.
#[test]
fn a_copy_that_lacks_the_comparing_row_differs_at_the_count() {
    let scratch = scratch(false);
    commit_all(scratch.root(), "bad subject line");
    let (_, result) = audit_with_an_edited_copy(
        &scratch,
        |copy| {
            let index = row_index(copy, DELIVERABLES);
            copy["rows"].as_array_mut().expect("rows").remove(index);
        },
        &[],
    );
    assert_eq!(
        row(&result, DELIVERABLES)["evidence"],
        json!("self_audit differs at self_audit.summary.pass")
    );
}

/// C12: a `--workspace` run does not compare. A report whose copy is stale
/// passes the check of `self_audit` there, and the row counts the reports.
#[test]
fn a_workspace_run_does_not_compare_the_self_audit() {
    let scratch = scratch(false);
    let stale = json!({
        "summary": {
            "base": "1111111", "head": "2222222", "deterministic": 1, "pass": 1, "fail": 0,
            "warn": 0, "skipped": 0, "judged": 0,
        },
        "rows": [],
    });
    scratch.write_report(
        "task-1-report.json",
        &json!({
            "kind": "task-report", "files_changed": ["src/a.rs"],
            "knowledge_used": ["process.tdd"], "docs_verified": ["x"], "self_audit": stale,
        }),
    );
    let (_, result) = scratch.audit_workspace(&scratch.base, &[]);
    assert_eq!(
        row(&result, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "fail",
            "pass",
            "report field self_audit is set in 1 of 1 reports"
        )
    );
}

// ---- spec 6: sanctions (case 6) ----

/// Case 6: a `--sanctioned` booking on a judged rule is never stale. The
/// judged row stays open and carries the booking in `sanctioned`.
#[test]
fn a_sanctioned_judged_rule_carries_the_booking_and_is_not_stale() {
    let scratch = scratch(false);
    let report = scratch.write_report("report.json", &draft(json!(["process.tdd"]), json!(["x"])));
    let (output, result) = scratch.audit_report(
        &scratch.base,
        &report,
        &[
            "--ids",
            JUDGED_RULE,
            "--sanctioned",
            "writing-style.code-comments=interim",
        ],
    );
    assert_eq!(
        row(&result, JUDGED_RULE),
        &json!({
            "id": JUDGED_RULE, "kind": "rule", "mode": "judged", "level": null,
            "result": "open", "evidence": "\u{2014}", "sanctioned": "interim",
        })
    );
    assert!(result.get("stale_sanctions").is_none(), "{result}");
    assert!(result["summary"].get("sanctioned_fail").is_none());
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Pin of unchanged behavior: a sanctioned deterministic fail keeps its true
/// result, the evidence suffix of batch 32, and the exit status 1.
#[test]
fn a_sanctioned_deterministic_fail_keeps_its_evidence_suffix_and_exit() {
    let scratch = scratch(false);
    commit_all(scratch.root(), "bad subject line");
    let report = scratch.write_report("report.json", &draft(json!(["process.tdd"]), json!(["x"])));
    let (output, result) = scratch.audit_report(&scratch.base, &report, &SANCTION);
    let booked = row(&result, CONVENTIONAL);
    assert_eq!(booked["result"], json!("fail"));
    let evidence = booked["evidence"].as_str().expect("evidence");
    assert!(
        evidence.starts_with("commit \"bad subject line\" does not match "),
        "{evidence}"
    );
    assert!(evidence.ends_with(" (sanctioned: interim)"), "{evidence}");
    assert_eq!(result["summary"]["sanctioned_fail"], json!(1));
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}

/// Case 6: every sanctioned row, a deterministic fail included, carries the
/// structured `sanctioned` field beside its evidence suffix.
#[test]
fn a_sanctioned_deterministic_fail_carries_the_sanctioned_field() {
    let scratch = scratch(false);
    commit_all(scratch.root(), "bad subject line");
    let report = scratch.write_report("report.json", &draft(json!(["process.tdd"]), json!(["x"])));
    let (_, result) = scratch.audit_report(&scratch.base, &report, &SANCTION);
    assert_eq!(row(&result, CONVENTIONAL)["sanctioned"], json!("interim"));
    assert!(
        row(&result, VERIFY_DOCS).get("sanctioned").is_none(),
        "a row nobody booked has no sanctioned field"
    );
}

// ---- spec 6: a booking on the comparing row (case 6) ----

/// The `--sanctioned` argument that books the comparing row.
const BOOK_DELIVERABLES: [&str; 2] = ["--sanctioned", "process.deliverables-json=interim2"];

/// Sets the evidence of the copy's row for `CONVENTIONAL` to a text that no run
/// prints: the hand-reworded row of batch 27.
fn reword_the_conventional_row(copy: &mut Value) {
    let index = row_index(copy, CONVENTIONAL);
    copy["rows"][index]["evidence"] = json!("hand-reworded");
}

/// The keys of the `summary` of `result`, in the order the audit prints them.
fn summary_keys(result: &Value) -> Vec<&str> {
    result["summary"]
        .as_object()
        .expect("summary")
        .keys()
        .map(String::as_str)
        .collect()
}

/// Case 6, the review's reproduction (block B): a stale copy fails the comparing
/// row after the sanctioned annotation ran, and the row is booked. The booking
/// reflects the final result: the row carries `sanctioned` and the evidence
/// suffix, `sanctioned_fail` counts it beside the booked rule row, and no
/// `stale_sanctions` calls the booking stale.
#[test]
fn a_booked_comparing_row_that_the_comparison_fails_carries_its_booking() {
    let scratch = scratch(false);
    commit_all(scratch.root(), "bad subject line");
    let (output, result) =
        audit_with_an_edited_copy(&scratch, reword_the_conventional_row, &BOOK_DELIVERABLES);
    let index = row_index(&copy_of(&result), CONVENTIONAL);
    assert_eq!(
        row(&result, DELIVERABLES),
        &json!({
            "id": DELIVERABLES, "kind": "rule", "mode": "deterministic", "level": "fail",
            "result": "fail", "sanctioned": "interim2",
            "evidence": format!(
                "self_audit differs at self_audit.rows[{index}].evidence (sanctioned: interim2)"
            ),
        })
    );
    assert_eq!(result["summary"]["sanctioned_fail"], json!(2));
    assert!(result.get("stale_sanctions").is_none(), "{result}");
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}

/// Case 6: the comparing row is the only booked row. The summary prints
/// `sanctioned_fail` where it prints it when a rule row is booked: before
/// `reference_fail`.
#[test]
fn a_booked_comparing_row_alone_puts_sanctioned_fail_where_a_rule_row_does() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    let mut copy = copy_of(&first);
    reword_the_conventional_row(&mut copy);
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (output, result) = scratch.audit_report(&scratch.base, &report, &BOOK_DELIVERABLES);
    assert_eq!(row(&result, DELIVERABLES)["result"], json!("fail"));
    assert_eq!(row(&result, DELIVERABLES)["sanctioned"], json!("interim2"));
    assert_eq!(result["summary"]["sanctioned_fail"], json!(1));
    assert!(result.get("stale_sanctions").is_none(), "{result}");
    assert_eq!(
        summary_keys(&result),
        [
            "base",
            "head",
            "deterministic",
            "pass",
            "fail",
            "warn",
            "skipped",
            "judged",
            "sanctioned_fail",
            "reference_fail"
        ]
    );
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}

/// Case 6: a comparing check of level `warn` turns the booked row into a `warn`
/// row, and a booking is for a `fail`. The stale line names the final result,
/// `warn`, and the row carries no booking. A stale booking fails the run.
#[test]
fn a_booked_comparing_row_of_level_warn_is_stale_with_its_final_result() {
    let scratch = scratch_with(|root| {
        let mut overrides = read_json(root, "knowledge/overrides.json");
        overrides["schema"] = json!("houserules/overrides@1.1.0");
        overrides["overrides"]
            .as_array_mut()
            .expect("the overrides")
            .push(json!({"id": DELIVERABLES, "fields": {"check": {
                "type": "report-field", "level": "warn", "if": "**", "field": "self_audit",
            }}}));
        write_json(root, "knowledge/overrides.json", &overrides);
    });
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    let mut copy = copy_of(&first);
    reword_the_conventional_row(&mut copy);
    scratch.write_report("report.json", &with_copy(&base, &copy));
    let (output, result) = scratch.audit_report(&scratch.base, &report, &BOOK_DELIVERABLES);
    let index = row_index(&copy, CONVENTIONAL);
    assert_eq!(
        row(&result, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "warn",
            "warn",
            &format!("self_audit differs at self_audit.rows[{index}].evidence")
        )
    );
    assert_eq!(
        result["stale_sanctions"],
        json!([
            "--sanctioned process.deliverables-json=interim2: row result is \"warn\", not fail"
        ])
    );
    assert!(result["summary"].get("sanctioned_fail").is_none());
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}

/// Pin of unchanged behavior: a booking on the comparing row of a copy that
/// agrees with the run stays stale, because the row passes. The row carries no
/// booking and the stale line names `pass`.
#[test]
fn a_booked_comparing_row_that_passes_stays_stale() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    scratch.write_report("report.json", &with_copy(&base, &copy_of(&first)));
    let (output, result) = scratch.audit_report(&scratch.base, &report, &BOOK_DELIVERABLES);
    assert_eq!(
        row(&result, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "fail",
            "pass",
            "report field self_audit is present"
        )
    );
    assert_eq!(
        result["stale_sanctions"],
        json!([
            "--sanctioned process.deliverables-json=interim2: row result is \"pass\", not fail"
        ])
    );
    assert!(result["summary"].get("sanctioned_fail").is_none());
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}

/// Pin of unchanged behavior: a failed run whose comparing row is booked is
/// copied back into the report, and the next run compares equal. The comparing
/// row passes then, so its booking is stale. The copy holds the booked row and
/// a `sanctioned_fail` of 1; the run before the booking holds neither, and the
/// comparing row accounts for both.
#[test]
fn a_booked_failed_run_copied_back_into_the_report_passes_the_next_run() {
    let scratch = scratch(false);
    let base = draft(json!(["process.tdd"]), json!(["x"]));
    let report = scratch.write_report("report.json", &base);
    let (_, first) = scratch.audit_report(&scratch.base, &report, &[]);
    let mut stale = copy_of(&first);
    reword_the_conventional_row(&mut stale);
    scratch.write_report("report.json", &with_copy(&base, &stale));
    let (_, failed) = scratch.audit_report(&scratch.base, &report, &BOOK_DELIVERABLES);
    assert_eq!(row(&failed, DELIVERABLES)["result"], json!("fail"));

    scratch.write_report("report.json", &with_copy(&base, &copy_of(&failed)));
    let (output, converged) = scratch.audit_report(&scratch.base, &report, &BOOK_DELIVERABLES);
    assert_eq!(
        row(&converged, DELIVERABLES),
        &field_row(
            DELIVERABLES,
            "fail",
            "pass",
            "report field self_audit is present"
        )
    );
    assert_eq!(
        converged["stale_sanctions"],
        json!([
            "--sanctioned process.deliverables-json=interim2: row result is \"pass\", not fail"
        ])
    );
    assert_eq!(output.status.code(), Some(1), "stderr: {}", stderr(&output));
}
