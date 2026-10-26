//! The published `rule-package`, `audit-result`, and `capture` documents: each
//! carries its `schema` key first and validates against its committed
//! schema; the committed schema files have the shape the generator gives
//! them; and the manifest `schemas/published.json` fails a changed, a
//! missing, and an unlisted file. The tests run the compiled `houserules`
//! binary; the schemas are the files of this checkout.
//!
//! A test whose doc comment calls it a pin asserts the order of the keys that
//! `audit` prints for its rows and its summary; every other test asserts the
//! `schema` key or a schema file.

mod schema_support;
mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};
use tempfile::TempDir;

use schema_support::{
    BASE_URL, META_SCHEMA, assert_valid, manifest_problems, read_json, schema_files,
};
use tracker_support::{
    commit_all, copy_dir, edit_marker, git, houserules, repo_root, run, run_ok, scratch_git_repo,
    stderr, stdout, write_json,
};

/// A path that the knowledge base of this repository resolves to an area.
const A_PATH: &str = "crates/houserules/src/main.rs";

/// The `schemas/` directory of this checkout.
fn schemas_dir() -> PathBuf {
    repo_root().join("schemas")
}

/// The keys of the JSON object `value`, in the order the text gave them.
fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// The parsed stdout of `houserules for <args>` over this checkout.
fn for_output(args: &[&str]) -> Value {
    let mut full = vec!["for"];
    full.extend_from_slice(args);
    let output = run_ok(&repo_root(), &full);
    serde_json::from_str(&stdout(&output)).expect("for prints one JSON document")
}

#[test]
fn for_prints_its_schema_key_first_and_validates() {
    let package = for_output(&[A_PATH]);
    assert_eq!(keys(&package)[0], "schema");
    assert_eq!(package["schema"], "houserules/rule-package@1.0.0");
    assert_valid(&package, "schemas/rule-package/1.0.0.json");
}

#[test]
fn for_full_prints_its_schema_key_first_and_validates() {
    let package = for_output(&[A_PATH, "--full"]);
    assert_eq!(keys(&package)[0], "schema");
    assert_eq!(package["schema"], "houserules/rule-package@1.0.0");
    let entries = package["entries"].as_array().expect("an entries array");
    assert!(
        entries.iter().any(|entry| entry.get("body").is_some()),
        "--full prints whole entries"
    );
    assert_valid(&package, "schemas/rule-package/1.0.0.json");
}

#[test]
fn a_capture_header_validates() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("run.txt");
    let output = houserules()
        .arg("capture")
        .arg("--out")
        .arg(&out)
        .args(["--", "sh", "-c", "echo hello"])
        .current_dir(dir.path())
        .output()
        .expect("run capture");
    assert!(output.status.success(), "{output:?}");
    let bytes = fs::read(&out).expect("read the capture");
    let first_line = bytes.split(|byte| *byte == b'\n').next().expect("a line");
    let header: Value = serde_json::from_slice(first_line).expect("the header is JSON");
    assert_eq!(keys(&header)[0], "schema");
    assert_eq!(header["schema"], "houserules/capture@1.0.0");
    assert_valid(&header, "schemas/capture/1.0.0.json");
}

// ---- the audit-result document ----------------------------------------------------

/// The schema file of the audit result, relative to the repository root.
const AUDIT_SCHEMA: &str = "schemas/audit-result/1.0.0.json";

/// The id of the entry whose `runner` check reads the SARIF of the fixture's
/// fake tool: its row holds `findings`.
const LINT: &str = "team.lint";

/// A kit entry without a check: its row is judged.
const JUDGED: &str = "process.tdd";

/// A kit entry whose check passes on the fixture's feature commit.
const PASSING: &str = "process.no-tech-debt";

/// The kit entry whose check reads the `self_audit` of a report.
const DELIVERABLES: &str = "process.deliverables-json";

/// A scratch install for the audit tests: the kit with the process pack, the
/// entry `team.lint` whose `runner` check reads SARIF from a fake tool, and a
/// feature commit with the subject `wip`, which the `commits` check of
/// `process.conventional-commits` fails.
struct AuditInstall {
    dir: TempDir,
    /// The commit that holds the kit: the base of every audit.
    base: String,
    /// A folder outside the repository for the reports and `--json` files.
    out: TempDir,
}

/// The topic file with the entry `team.lint`.
fn team_topic() -> Value {
    json!({
        "$schema": "./schema.json",
        "topic": "team",
        "title": "Team rules",
        "entries": [{
            "id": LINT, "kind": "rule", "area": "global", "standing": true,
            "summary": "Lint the source.", "body": ["Run the linter."], "tags": [],
            "source": {"date": "2026-10-24", "by": "controller"},
            "check": {
                "type": "runner", "level": "fail", "tool": "fake",
                "files": "src/**", "scope": "changed",
            },
        }],
    })
}

/// The SARIF document that the fake tool prints: one finding in `src/a.rs`.
fn sarif() -> Value {
    json!({
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {"name": "fake"}},
            "results": [{
                "ruleId": "no-todo", "level": "error", "message": {"text": "remove the todo"},
                "locations": [{"physicalLocation": {
                    "artifactLocation": {"uri": "src/a.rs"}, "region": {"startLine": 1},
                }}],
            }],
        }],
    })
}

/// The scratch install of [`AuditInstall`].
fn audit_install() -> AuditInstall {
    let dir = scratch_git_repo();
    let root = dir.path();
    run_ok(root, &["init", "--tracker", "json"]);
    run_ok(root, &["pack", "add", "process"]);
    write_json(root, "knowledge/team.json", &team_topic());
    edit_marker(root, |marker| {
        marker.insert(
            "runners".to_string(),
            json!({"fake": {
                "command": ["sh", "tools/fake-sarif.sh", "out/result.sarif", "0", "out/calls.txt"],
                "parser": "sarif",
                "install": "install the fake tool",
            }}),
        );
    });
    let tool = fs::read_to_string(
        repo_root().join("crates/houserules/tests/fixtures/runner/fake-sarif.sh"),
    )
    .expect("read the fake tool");
    fs::create_dir_all(root.join("tools")).expect("create tools/");
    fs::write(root.join("tools/fake-sarif.sh"), tool).expect("write the fake tool");
    write_json(root, "out/result.sarif", &sarif());
    fs::write(root.join(".git/info/exclude"), "out/\n").expect("exclude out/");
    let base = commit_all(root, "chore: install the kit");
    fs::create_dir_all(root.join("src")).expect("create src/");
    fs::write(root.join("src/a.rs"), "fn a() {}\n").expect("write src/a.rs");
    commit_all(root, "wip");
    AuditInstall {
        dir,
        base,
        out: tempfile::tempdir().expect("tempdir"),
    }
}

impl AuditInstall {
    /// The repository root.
    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Runs `audit --base <base>` with `extra` arguments.
    fn run_audit(&self, extra: &[&str]) -> Output {
        let mut args = vec!["audit", "--base", self.base.as_str()];
        args.extend_from_slice(extra);
        run(self.root(), &args)
    }

    /// Runs `audit --base <base>` with `extra` arguments and parses stdout.
    fn audit(&self, extra: &[&str]) -> Value {
        let output = self.run_audit(extra);
        parse_audit(&output)
    }

    /// Writes `report` into the folder outside the repository and returns the
    /// path as text.
    fn write_report(&self, report: &Value) -> String {
        let path = self.out.path().join("report.json");
        fs::write(&path, report.to_string()).expect("write the report");
        path.to_str().expect("a UTF-8 path").to_string()
    }
}

/// The JSON document that `output`, the run of `audit`, printed.
fn parse_audit(output: &Output) -> Value {
    serde_json::from_str(&stdout(output)).unwrap_or_else(|error| {
        panic!(
            "audit prints JSON ({error}): stdout {}, stderr {}",
            stdout(output),
            stderr(output)
        )
    })
}

/// The row of `result` for the entry `id`.
fn row<'a>(result: &'a Value, id: &str) -> &'a Value {
    result["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("the result has no row for {id}"))
}

/// A task report with a draft `self_audit`.
fn draft_report() -> Value {
    json!({"kind": "task-report", "self_audit": null})
}

/// The copy of `result` that a report holds as `self_audit`: the summary and
/// the deterministic rows.
fn copy_of(result: &Value) -> Value {
    let rows: Vec<&Value> = result["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .filter(|row| row["mode"] == "deterministic")
        .collect();
    json!({"summary": result["summary"], "rows": rows})
}

/// The draft report with `copy` as its `self_audit`.
fn report_with(copy: Value) -> Value {
    let mut report = draft_report();
    report["self_audit"] = copy;
    report
}

/// The copy of `result` whose first row holds another evidence line, so that a
/// comparison with a fresh run differs at `self_audit.rows[0].evidence`.
fn stale_copy_of(result: &Value) -> Value {
    let mut copy = copy_of(result);
    copy["rows"][0]["evidence"] = json!("another line");
    copy
}

#[test]
fn audit_prints_its_schema_key_first_and_validates() {
    let install = audit_install();
    let result = install.audit(&[]);
    assert_eq!(
        keys(&result),
        [
            "schema",
            "base",
            "head",
            "ids",
            "changed_files",
            "areas",
            "area_files",
            "rules",
            "summary",
            "references"
        ]
    );
    assert_eq!(result["schema"], "houserules/audit-result@1.0.0");
    assert_eq!(
        row(&result, LINT)["findings"].as_array().map(Vec::len),
        Some(1),
        "the fixture gives the schema a row with findings to check"
    );
    assert_valid(&result, AUDIT_SCHEMA);
}

#[test]
fn audit_writes_the_same_document_to_stdout_and_to_json() {
    let install = audit_install();
    let file = install.out.path().join("audit.json");
    let output = install.run_audit(&["--json", file.to_str().expect("a UTF-8 path")]);
    let written = fs::read_to_string(&file).expect("read the --json file");
    assert_eq!(written, stdout(&output));
    let document: Value = serde_json::from_str(&written).expect("the file holds JSON");
    assert_eq!(keys(&document)[0], "schema");
    assert_valid(&document, AUDIT_SCHEMA);
}

#[test]
fn a_report_run_validates_with_a_draft_a_matching_copy_and_a_stale_copy() {
    let install = audit_install();
    let draft = install.write_report(&draft_report());
    let first = install.audit(&["--report", &draft]);
    assert_eq!(keys(&first)[0], "schema");
    assert_eq!(row(&first, DELIVERABLES)["result"], "pass");
    assert_valid(&first, AUDIT_SCHEMA);

    let matching = install.write_report(&report_with(copy_of(&first)));
    let second = install.audit(&["--report", &matching]);
    assert_eq!(
        second["rules"], first["rules"],
        "a matching copy moves no row"
    );
    assert_valid(&second, AUDIT_SCHEMA);

    let stale = install.write_report(&report_with(stale_copy_of(&first)));
    let third = install.audit(&["--report", &stale]);
    let moved = row(&third, DELIVERABLES);
    assert_eq!(moved["result"], "fail");
    assert_eq!(
        moved["evidence"],
        "self_audit differs at self_audit.rows[0].evidence"
    );
    assert_eq!(
        third["summary"]["fail"].as_u64(),
        first["summary"]["fail"].as_u64().map(|fail| fail + 1),
        "the summary counts the moved row"
    );
    assert_valid(&third, AUDIT_SCHEMA);
}

#[test]
fn a_sanctioned_run_validates_and_marks_the_booked_rows() {
    let install = audit_install();
    let result = install.audit(&[
        "--sanctioned",
        "team.lint=interim",
        "--sanctioned",
        "process.tdd=interim",
    ]);
    assert_eq!(row(&result, LINT)["result"], "fail");
    assert_eq!(row(&result, LINT)["sanctioned"], "interim");
    assert!(
        row(&result, LINT)["evidence"]
            .as_str()
            .is_some_and(|evidence| evidence.ends_with("(sanctioned: interim)"))
    );
    assert_eq!(row(&result, JUDGED)["sanctioned"], "interim");
    assert_eq!(result["summary"]["sanctioned_fail"], 1);
    assert_valid(&result, AUDIT_SCHEMA);
}

#[test]
fn a_stale_booking_prints_stale_sanctions_between_summary_and_references_and_validates() {
    let install = audit_install();
    let result = install.audit(&["--sanctioned", "process.no-tech-debt=interim"]);
    assert_eq!(
        result["stale_sanctions"],
        json!(["--sanctioned process.no-tech-debt=interim: row result is \"pass\", not fail"])
    );
    assert_eq!(row(&result, PASSING)["result"], "pass");
    let top = keys(&result);
    assert_eq!(
        top[top.len() - 3..],
        ["summary", "stale_sanctions", "references"]
    );
    assert_valid(&result, AUDIT_SCHEMA);
}

#[test]
fn a_booking_on_the_row_that_the_comparison_moves_validates() {
    let install = audit_install();
    let draft = install.write_report(&draft_report());
    let first = install.audit(&["--report", &draft]);
    let stale = install.write_report(&report_with(stale_copy_of(&first)));
    let result = install.audit(&[
        "--report",
        &stale,
        "--sanctioned",
        "process.deliverables-json=interim",
    ]);
    let moved = row(&result, DELIVERABLES);
    assert_eq!(moved["result"], "fail");
    assert_eq!(moved["sanctioned"], "interim");
    assert!(
        result.get("stale_sanctions").is_none(),
        "the booking is judged again against the moved row"
    );
    assert_eq!(result["summary"]["sanctioned_fail"], 1);
    assert_valid(&result, AUDIT_SCHEMA);
}

#[test]
fn an_empty_range_prints_empty_range_and_validates() {
    let install = audit_install();
    let head = git(install.root(), &["rev-parse", "HEAD"])
        .trim()
        .to_string();
    let output = run(install.root(), &["audit", "--base", &head]);
    let result = parse_audit(&output);
    assert_eq!(result["summary"]["empty_range"], true);
    assert_eq!(
        row(&result, PASSING)["evidence"],
        "empty range: 0 files checked"
    );
    assert_valid(&result, AUDIT_SCHEMA);
}

/// Pin of unchanged behavior: the keys of a row follow the order `id`, `kind`,
/// `mode`, `level`, `result`, `evidence`, `findings`, `sanctioned`, and the keys
/// of a summary follow `base`, `head`, the counts, `sanctioned_fail`, and
/// `reference_fail`. A reordered key changes the bytes of every golden.
#[test]
fn audit_keeps_the_key_order_of_its_rows_and_its_summary() {
    let install = audit_install();
    let result = install.audit(&["--sanctioned", "team.lint=interim"]);
    assert_eq!(
        keys(row(&result, LINT)),
        [
            "id",
            "kind",
            "mode",
            "level",
            "result",
            "evidence",
            "findings",
            "sanctioned"
        ]
    );
    assert_eq!(
        keys(row(&result, JUDGED)),
        ["id", "kind", "mode", "level", "result", "evidence"]
    );
    assert_eq!(
        keys(&result["summary"]),
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
    assert_eq!(
        keys(&result["references"]),
        ["checked", "passed", "skipped", "failures"]
    );
}

#[test]
fn every_committed_schema_has_its_id_its_meta_schema_and_its_schema_const() {
    let files = schema_files(&schemas_dir());
    for expected in [
        "audit-result/1.0.0.json",
        "capture/1.0.0.json",
        "change-record/1.0.0.json",
        "evidence/1.0.0.json",
        "route/1.0.0.json",
        "rule-package/1.0.0.json",
    ] {
        assert!(
            files.iter().any(|file| file == expected),
            "{expected} is missing from {files:?}"
        );
    }
    for file in &files {
        let schema = read_json(&schemas_dir().join(file));
        let (name, version) = file
            .strip_suffix(".json")
            .and_then(|stem| stem.split_once('/'))
            .unwrap_or_else(|| panic!("{file} is no <name>/<version>.json path"));
        assert_eq!(schema["$id"], format!("{BASE_URL}/{file}"), "{file}: $id");
        assert_eq!(schema["$schema"], META_SCHEMA, "{file}: $schema");
        assert_eq!(
            schema["properties"]["schema"]["const"],
            format!("houserules/{name}@{version}"),
            "{file}: the const of the schema key"
        );
        assert_eq!(
            keys(&schema["properties"])[0],
            "schema",
            "{file}: the schema key is the first property"
        );
    }
}

#[test]
fn the_committed_schemas_describe_serialization() {
    let capture = read_json(&schemas_dir().join("capture/1.0.0.json"));
    assert_eq!(
        capture["required"],
        json!([
            "schema", "argv", "cwd", "start", "end", "exit", "signal", "head", "dirty", "lossy"
        ]),
        "a serialized Option field is always present, so the header requires it"
    );
    let package = read_json(&schemas_dir().join("rule-package/1.0.0.json"));
    assert_eq!(
        package["required"],
        json!(["schema", "paths", "areas", "entries", "standing"])
    );
    let route = read_json(&schemas_dir().join("route/1.0.0.json"));
    assert_eq!(
        route["required"],
        json!([
            "schema", "install", "head", "route", "floor", "inputs", "reasons"
        ]),
        "the route document always prints these keys"
    );
    assert_eq!(
        route["$defs"]["Inputs"]["required"],
        json!([
            "failed",
            "warned",
            "areas",
            "no_area",
            "paths",
            "diff_lines",
            "uncovered_share",
            "score",
            "rule_sources_changed"
        ]),
        "a floor prints a null uncovered share, and a run with no score a null score, so both are required"
    );
    assert_eq!(
        route["$defs"]["Reason"]["required"],
        json!([
            "rule",
            "input",
            "value",
            "threshold",
            "route",
            "paths",
            "more_paths",
            "decisive"
        ]),
        "a reason prints a null value and a null threshold when the rule has none"
    );
    let evidence = read_json(&schemas_dir().join("evidence/1.0.0.json"));
    assert_eq!(
        evidence["required"],
        json!([
            "schema",
            "install",
            "base",
            "head",
            "merge_base",
            "intent",
            "rules",
            "diff",
            "uncovered",
            "failed",
            "warned",
            "proven",
            "judged",
            "skipped",
            "work_items",
            "route"
        ]),
        "the evidence document always prints these keys; merge_base is null for a range from the empty tree"
    );
    assert_eq!(
        evidence["$defs"]["Rules"]["required"],
        json!(["merge_base_hash", "head_hash", "packs"])
    );
    assert_eq!(
        evidence["$defs"]["JudgedRow"]["required"],
        json!(["id", "verdict", "evidence"]),
        "a judged rule with no review row prints a null verdict and a null evidence"
    );
    assert_eq!(
        evidence["$defs"]["UncoveredPath"]["required"],
        json!(["path", "old_path", "kind", "ranges"]),
        "the empty key belongs to the kind empty alone"
    );
    let record = read_json(&schemas_dir().join("change-record/1.0.0.json"));
    assert_eq!(
        record["required"],
        json!([
            "schema",
            "install",
            "key",
            "base",
            "head",
            "merge_base",
            "written_by",
            "host",
            "rules",
            "audit",
            "intent",
            "route",
            "sample",
            "reviewers",
            "reviewers_pending",
            "post_merge_audit",
            "truncated"
        ]),
        "the minimal record prints these keys; commits, evidence, bundle, links, and reviewers_more are the optional ones"
    );
    assert_eq!(
        record["$defs"]["Rules"]["required"],
        json!([
            "merge_base_hash",
            "merge_base_rules",
            "head_hash",
            "audited_with",
            "packs"
        ])
    );
    assert_eq!(
        record["$defs"]["Rules"]["properties"]["merge_base_rules"]["enum"],
        json!(["present", "absent"])
    );
    assert_eq!(record["$defs"]["Audit"]["required"], json!(["summary"]));
    assert_eq!(
        record["$defs"]["Audit"]["properties"]["references"]["$ref"],
        json!("#/$defs/References"),
        "the reference summary is the type of `audit --json`, failure lines included"
    );
    assert_eq!(
        record["$defs"]["References"]["required"],
        json!(["checked", "passed", "skipped", "failures"])
    );
    assert!(
        record["$defs"]["RecordEvidence"]["properties"]["work_items_more"].is_object(),
        "a cut of the evidence's work items leaves a count"
    );
    assert!(
        !record["$defs"]["RecordEvidence"]["required"]
            .as_array()
            .expect("required keys")
            .contains(&json!("work_items_more")),
        "an evidence that no cut touched has no such key"
    );
    assert_eq!(
        record["$defs"]["UncoveredPaths"]["anyOf"][1]["propertyNames"],
        json!({"enum": [
            "text", "deleted", "renamed", "empty", "binary", "mode", "symlink", "submodule"
        ]}),
        "a count map is keyed by the kinds of spec 4.2, in their order"
    );
    assert_eq!(
        record["$defs"]["PostMergeAudit"]["required"],
        json!(["verdict", "by", "note"]),
        "an audit with no note prints a null note"
    );
    assert_eq!(
        record["$defs"]["RecordRun"]["required"],
        json!(["argv", "exit", "head", "sha256"]),
        "a header that does not parse leaves an empty argv and null facts"
    );
    let audit = read_json(&schemas_dir().join("audit-result/1.0.0.json"));
    assert_eq!(
        audit["required"],
        json!([
            "schema",
            "base",
            "head",
            "ids",
            "changed_files",
            "areas",
            "area_files",
            "rules",
            "summary",
            "references"
        ]),
        "the audit always prints these keys; stale_sanctions is the one optional top-level key"
    );
    assert_eq!(
        audit["$defs"]["AuditRow"]["required"],
        json!(["id", "kind", "mode", "level", "result", "evidence"]),
        "a judged row prints a null level, so the row requires it; findings and sanctioned are optional"
    );
    assert_eq!(
        audit["$defs"]["Summary"]["required"],
        json!([
            "base",
            "head",
            "deterministic",
            "pass",
            "fail",
            "warn",
            "skipped",
            "judged",
            "reference_fail"
        ]),
        "empty_range and sanctioned_fail are the optional summary keys"
    );
    assert_eq!(
        audit["$defs"]["References"]["required"],
        json!(["checked", "passed", "skipped", "failures"])
    );
}

#[test]
fn the_schema_names_the_evidence_that_a_judged_row_prints() {
    let install = audit_install();
    let result = install.audit(&[]);
    let token = row(&result, JUDGED)["evidence"]
        .as_str()
        .expect("a judged row prints its evidence as text");
    let schema = read_json(&schemas_dir().join("audit-result/1.0.0.json"));
    let description = schema["$defs"]["AuditRow"]["properties"]["evidence"]["description"]
        .as_str()
        .expect("the evidence of a row has a description");
    assert!(
        description.contains(token),
        "the description {description:?} names the token {token:?} of a judged row"
    );
}

/// The change-record schema gives `reviewers_pending` both meanings of spec
/// 8.2: a later job has not set the reviewers, or the approvers could not be
/// read and stay unknown.
#[test]
fn the_change_record_schema_gives_reviewers_pending_both_meanings() {
    let schema = read_json(&schemas_dir().join("change-record/1.0.0.json"));
    assert_eq!(
        schema["properties"]["reviewers_pending"]["description"],
        json!(
            "`true` when the record holds no reviewers yet: a later job has not set them (spec 10.4), or the approvers could not be read and stay unknown (spec 10.1 and 10.4)."
        )
    );
}

/// Appends every `description` string below `value` to `found`, with the path
/// of the keys that lead to it.
fn collect_descriptions(value: &Value, path: &str, found: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let child_path = format!("{path}/{key}");
                match child {
                    Value::String(text) if key == "description" => {
                        found.push((child_path, text.clone()));
                    }
                    _ => collect_descriptions(child, &child_path, found),
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_descriptions(item, &format!("{path}/{index}"), found);
            }
        }
        _ => {}
    }
}

#[test]
fn a_published_description_holds_no_line_wrap() {
    let mut wrapped = Vec::new();
    for file in schema_files(&schemas_dir()) {
        let mut found = Vec::new();
        collect_descriptions(&read_json(&schemas_dir().join(&file)), "", &mut found);
        assert!(!found.is_empty(), "{file} describes nothing");
        for (path, text) in found {
            if text.split("\n\n").any(|paragraph| paragraph.contains('\n')) {
                wrapped.push(format!("{file}{path}: {text:?}"));
            }
        }
    }
    assert_eq!(
        wrapped,
        Vec::<String>::new(),
        "each description holds paragraphs of one line"
    );
}

#[test]
fn the_manifest_lists_every_schema_file_with_its_digest() {
    assert_eq!(manifest_problems(&schemas_dir()), Vec::<String>::new());
}

/// A scratch copy of `schemas/`.
fn schemas_copy() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&schemas_dir(), dir.path());
    dir
}

#[test]
fn the_manifest_fails_a_changed_file() {
    let copy = schemas_copy();
    let changed = copy.path().join("capture/1.0.0.json");
    let mut text = fs::read_to_string(&changed).expect("read the schema");
    text.push('\n');
    fs::write(&changed, text).expect("change the schema");
    assert_eq!(
        manifest_problems(copy.path()),
        vec!["capture/1.0.0.json: its bytes differ from the manifest".to_string()]
    );
}

#[test]
fn the_manifest_fails_a_missing_file() {
    let copy = schemas_copy();
    fs::remove_file(copy.path().join("rule-package/1.0.0.json")).expect("remove the schema");
    assert_eq!(
        manifest_problems(copy.path()),
        vec!["rule-package/1.0.0.json: listed but missing".to_string()]
    );
}

#[test]
fn the_manifest_names_a_listed_file_that_cannot_be_read() {
    let copy = schemas_copy();
    let listed = copy.path().join("capture").join("1.0.0.json");
    fs::remove_file(&listed).expect("remove the schema");
    fs::create_dir(&listed).expect("a directory in its place");
    let reason = fs::read(&listed).expect_err("a directory is no readable file");
    assert_eq!(
        manifest_problems(copy.path()),
        vec![format!("capture/1.0.0.json: cannot be read: {reason}")]
    );
}

#[test]
fn the_manifest_fails_an_unlisted_file() {
    let copy = schemas_copy();
    let unlisted = copy.path().join("unlisted/1.0.0.json");
    fs::create_dir_all(unlisted.parent().expect("a parent")).expect("create the directory");
    fs::write(&unlisted, "{}\n").expect("write the schema");
    assert_eq!(
        manifest_problems(copy.path()),
        vec!["unlisted/1.0.0.json: not listed in the manifest".to_string()]
    );
}

/// A copy of the `mini` fixture with a topic file that holds two entries
/// whose keys follow two different orders, as the topic files of this
/// repository do (`source` and `tags` occur in both orders).
fn install_with_two_key_orders() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    copy_dir(&repo_root().join("tests/fixtures/mini"), dir.path());
    let topic = r#"{
  "$schema": "./schema.json",
  "topic": "order",
  "title": "Entries with two key orders",
  "entries": [
    {
      "id": "order.source-first",
      "kind": "rule",
      "area": "tools",
      "summary": "An entry whose source key precedes its tags key.",
      "body": ["The keys follow the order of the file."],
      "source": { "date": "2026-10-24", "by": "docs", "ref": "fixture" },
      "tags": ["fixture"],
      "see": ["mini.build-cache"],
      "verify": ["mini-tools/build.sh"]
    },
    {
      "id": "order.tags-first",
      "kind": "rule",
      "area": "tools",
      "summary": "An entry whose tags key precedes its source key.",
      "body": ["The keys follow the order of the file."],
      "tags": ["fixture"],
      "source": { "date": "2026-10-24", "by": "docs", "ref": "fixture" },
      "verify": ["mini-tools/build.sh"],
      "see": ["mini.build-cache"]
    }
  ]
}
"#;
    fs::write(dir.path().join("knowledge/order.json"), topic).expect("write the topic file");
    dir
}

/// `for --full` prints each stored entry with its keys in the order of its
/// topic file, then `topic`. A package that sorts or re-orders the keys of an
/// entry fails this test.
#[test]
fn for_full_keeps_each_entrys_key_order() {
    let install = install_with_two_key_orders();
    let output = run_ok(install.path(), &["for", "mini-tools/build.sh", "--full"]);
    let package: Value = serde_json::from_str(&stdout(&output)).expect("JSON output");
    let entries = package["entries"].as_array().expect("an entries array");
    let find = |id: &str| -> &Value {
        entries
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap_or_else(|| panic!("{id} is missing from the package"))
    };
    let expected_source_first = [
        "id", "kind", "area", "summary", "body", "source", "tags", "see", "verify", "topic",
    ];
    let expected_tags_first = [
        "id", "kind", "area", "summary", "body", "tags", "source", "verify", "see", "topic",
    ];
    assert_eq!(keys(find("order.source-first")), expected_source_first);
    assert_eq!(keys(find("order.tags-first")), expected_tags_first);
}
