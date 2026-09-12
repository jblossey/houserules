//! The `validate` command: schema-validates one or more deliverable JSON
//! files against `.claude/schemas/deliverables.json`, plus the two
//! task-report invariants the schema's shape rules cannot express.
//!
//! This validates a deliverable file's *shape* directly against the raw
//! `serde_json::Value`, using the generic JSON-Schema-subset engine
//! (`super::validate`, `check.rs`) rather than a typed
//! `TaskReport`/`TaskReview`/`ReReview`/`BranchReview` parse, which is not
//! this command's validation path and would duplicate the schema engine's
//! semantics a second time in the type system (`quality.principles`: one
//! write path). There is consequently no typed deliverable model layer
//! for this crate to carry.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::emit::emit;
use crate::node_path::resolve_like_node;

use super::deliverable::read_deliverable_value;
use super::model::load_base;

/// Path, relative to the repo root, of the agent-deliverables JSON Schema.
const DELIVERABLES_SCHEMA: &str = ".claude/schemas/deliverables.json";

/// Maps a deliverable's `kind` field to its definition name in
/// `DELIVERABLES_SCHEMA`.
const DELIVERABLE_KINDS: [(&str, &str); 4] = [
    ("task-report", "taskReport"),
    ("task-review", "taskReview"),
    ("re-review", "reReview"),
    ("branch-review", "branchReview"),
];

/// `task-report` statuses that claim the task is genuinely finished.
const TERMINAL_STATUSES: [&str; 2] = ["DONE", "DONE_WITH_CONCERNS"];

/// One validated deliverable: the file it read, the deliverable `kind` it
/// found, and every schema/invariant violation (empty when valid).
#[derive(Debug)]
pub(super) struct ValidatedDeliverable {
    pub file: PathBuf,
    pub kind: String,
    pub errors: Vec<String>,
}

/// Checks the two task-report invariants the schema's shape rules cannot
/// express: a terminal `status` (DONE or DONE_WITH_CONCERNS) needs a
/// filled `self_audit`, and that audit's `summary.skipped` must be 0 -- a
/// nonzero count means the audit ran without `--report` and skipped every
/// report-field check, so its rows are not trustworthy. BLOCKED and
/// NEEDS_CONTEXT reports are exempt.
fn check_task_report_audit(value: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(status) = value.get("status").and_then(Value::as_str) else {
        return;
    };
    if !TERMINAL_STATUSES.contains(&status) {
        return;
    }
    check_fix_round_audits(value, path, errors);
    let self_audit = value.get("self_audit");
    if matches!(self_audit, Some(Value::Null)) {
        errors.push(format!(
            "{path}: status \"{status}\" needs a non-null self_audit"
        ));
        return;
    }
    let skipped = self_audit
        .and_then(|sa| sa.get("summary"))
        .and_then(|summary| summary.get("skipped"));
    if let Some(Value::Number(skipped)) = skipped
        && skipped.as_f64().is_some_and(|n| n > 0.0)
    {
        errors.push(format!(
            "{path}: self_audit.summary.skipped is {skipped}; re-run audit with --report"
        ));
    }
}

/// Parses the first JSON value at or after `output`'s first `{` and
/// returns its `summary.skipped` count when that value is a number
/// greater than zero -- `None` for output carrying no `{` at all (a
/// cargo/vitest run's prose), no `summary.skipped`, or one that is zero.
///
/// Two tolerances, both required by `output` being a verbatim command
/// capture (`process.evidence-outlives-the-session`'s invariant), not a
/// hand-trimmed excerpt:
/// - Leading bytes: `output.find('{')` skips past any prose before the
///   JSON starts (a shell prompt echo, a blank line).
/// - Trailing bytes: `Deserializer::from_str(..).into_iter().next()`
///   reads only the FIRST top-level value and tolerates bytes after it,
///   unlike `serde_json::from_str`, which rejects anything but trailing
///   whitespace -- a trailing newline or a second line of shell noise
///   must not turn a real match into a parse failure.
///
/// Limits (honest, not exhaustive): a capture whose PROSE itself contains
/// a `{` before the real audit JSON starts is not scanned past that
/// point. `find` returns the first occurrence unconditionally, so the
/// parse is attempted from the prose's own brace; when that slice is not
/// valid JSON the whole function returns `None` -- a silent miss, the
/// same shape a plain leading line would cause, just for a leading line
/// that happens to contain `{` --
/// `accepts_a_fix_round_test_whose_prose_contains_a_brace_before_the_json`
/// pins this residual gap rather than leaving it unmeasured. Retrying at
/// each subsequent `{` until one parses would close it, but no observed
/// capture in this project's own history needs it; adding the retry loop
/// now would be speculative complexity YAGNI already rules against.
fn parse_audit_summary_skipped(output: &str) -> Option<serde_json::Number> {
    let start = output.find('{')?;
    let value = serde_json::Deserializer::from_str(&output[start..])
        .into_iter::<Value>()
        .next()?
        .ok()?;
    match value.get("summary")?.get("skipped")? {
        Value::Number(n) if n.as_f64().is_some_and(|f| f > 0.0) => Some(n.clone()),
        _ => None,
    }
}

/// Scans every `fix_rounds[].tests[].output` for an embedded audit result
/// carrying a nonzero `summary.skipped`, the same threshold
/// `check_task_report_audit` already applies to the top-level audit: a
/// fix round whose OWN audit test ran without `--report` must also fail
/// validation, not only a top-level `self_audit` in that state.
fn check_fix_round_audits(value: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(fix_rounds) = value.get("fix_rounds").and_then(Value::as_array) else {
        return;
    };
    for (round_index, round) in fix_rounds.iter().enumerate() {
        let Some(tests) = round.get("tests").and_then(Value::as_array) else {
            continue;
        };
        for (test_index, test) in tests.iter().enumerate() {
            let Some(output) = test.get("output").and_then(Value::as_str) else {
                continue;
            };
            if let Some(skipped) = parse_audit_summary_skipped(output) {
                errors.push(format!(
                    "{path}.fix_rounds[{round_index}].tests[{test_index}].output: audit summary skipped is {skipped}; re-run audit with --report"
                ));
            }
        }
    }
}

/// Validates one deliverable file at `path` against the definition its
/// `kind` names in `root`'s `DELIVERABLES_SCHEMA`, plus
/// `check_task_report_audit` for a `task-report`. `path` is used verbatim
/// as the returned `file` and as every error message's location prefix --
/// callers resolve it to an absolute path first (`cmd_validate` does,
/// against `cwd`).
fn validate_deliverable(root: &Path, path: &Path) -> Result<ValidatedDeliverable, String> {
    let schema = read_deliverable_value(&root.join(DELIVERABLES_SCHEMA))?;
    let value = read_deliverable_value(path)?;
    let path_display = path.display().to_string();

    let kind_str = value.get("kind").and_then(Value::as_str);
    let def = kind_str.and_then(|kind| {
        DELIVERABLE_KINDS
            .iter()
            .find(|(candidate, _)| *candidate == kind)
            .map(|(_, def)| *def)
    });
    let Some(def) = def else {
        let kind_repr = match value.get("kind") {
            Some(kind_value) => {
                serde_json::to_string(kind_value).unwrap_or_else(|_| "null".to_string())
            }
            None => "undefined".to_string(),
        };
        return Err(format!(
            "{path_display}: unknown deliverable kind {kind_repr}"
        ));
    };

    let mut errors = Vec::new();
    let reference = json!({"$ref": format!("#/$defs/{def}")});
    super::validate(&value, &reference, &path_display, &mut errors, &schema);
    if def == "taskReport" {
        check_task_report_audit(&value, &path_display, &mut errors);
    }
    Ok(ValidatedDeliverable {
        file: path.to_path_buf(),
        kind: kind_str
            .expect("a recognized kind is always a string")
            .to_string(),
        errors,
    })
}

/// Runs the `validate` subcommand: resolves `root` (`--dir`, or the
/// enclosing git repository's top level) and loads the knowledge base
/// there before dispatching -- see `stats::cmd_stats`'s doc for why this
/// runs even though `validate_deliverable` needs only `root`'s path, not
/// the loaded base's contents. Validates every file in `files` (each
/// absolutized against `cwd`) and prints the JSON results array.
pub(crate) fn cmd_validate(dir: Option<PathBuf>, files: Vec<PathBuf>) -> ExitCode {
    // Root resolution and `load_base` run before the arity check: on a
    // repository whose `knowledge/schema.json` is invalid, `validate`
    // with no files must print the schema load error, not "validate needs
    // at least one file" -- checking arity first would print the wrong
    // one, both exit 2.
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    if let Err(error) = load_base(&root) {
        eprintln!("{error}");
        return ExitCode::from(2);
    }
    if files.is_empty() {
        eprintln!("validate needs at least one file");
        return ExitCode::from(2);
    }

    let mut results = Vec::with_capacity(files.len());
    for file in &files {
        let absolute = match resolve_like_node(file) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{}: {error}", file.display());
                return ExitCode::from(2);
            }
        };
        match validate_deliverable(&root, &absolute) {
            Ok(result) => results.push(result),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        }
    }
    let any_errors = results.iter().any(|result| !result.errors.is_empty());
    let json_results: Vec<Value> = results
        .into_iter()
        .map(|result| {
            json!({
                "file": result.file.display().to_string(),
                "kind": result.kind,
                "errors": result.errors,
            })
        })
        .collect();
    print!("{}", emit(&Value::Array(json_results)));
    if any_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn template_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template")
    }

    /// A minimal repo root carrying only the vendored deliverables schema --
    /// `validate_deliverable` needs nothing else from `root`.
    fn schema_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join(DELIVERABLES_SCHEMA);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(
            template_root().join(".claude/schemas/deliverables.json"),
            &dest,
        )
        .unwrap();
        dir
    }

    fn report_sample() -> Value {
        json!({
            "kind": "task-report",
            "task": 1,
            "backlog": ["WI-001"],
            "status": "DONE",
            "implemented": "x",
            "commits": [{"sha": "abc1234", "subject": "feat: x"}],
            "tests": [{"command": "vitest", "output": "ok"}],
            "live_run": [{"command": "houserules init --dir scratch", "output": "ok", "exit": 0}],
            "tdd": [{
                "test": "t", "mode": "natural",
                "red": {"command": "c", "output": "FAIL"},
                "green": {"command": "c", "output": "PASS"},
            }],
            "files_changed": ["a.mjs"],
            "docs_verified": [],
            "dependency_vetting": null,
            "coverage": null,
            "self_audit": {
                "summary": {
                    "base": "abc1234", "head": "abc1235", "deterministic": 1,
                    "pass": 1, "fail": 0, "warn": 0, "skipped": 0, "judged": 0,
                },
                "rows": [{
                    "id": "process.sequential", "mode": "deterministic",
                    "result": "pass", "evidence": "x",
                }],
            },
            "self_review": [],
            "concerns": [],
            "knowledge_used": ["process.sequential"],
        })
    }

    fn write_report(root: &Path, value: &Value) -> PathBuf {
        let file = root.join("report.json");
        fs::write(&file, serde_json::to_string(value).unwrap()).unwrap();
        file
    }

    /// Validates a well-formed task report with no errors.
    #[test]
    fn validates_a_well_formed_task_report_with_no_errors() {
        let root = schema_root();
        let file = write_report(root.path(), &report_sample());
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(result.kind, "task-report");
        assert_eq!(result.errors, Vec::<String>::new());
    }

    /// Rejects a DONE report with a null self_audit.
    #[test]
    fn rejects_a_done_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}: status \"DONE\" needs a non-null self_audit",
                file.display()
            )]
        );
    }

    /// Rejects a DONE_WITH_CONCERNS report with a null self_audit.
    #[test]
    fn rejects_a_done_with_concerns_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("DONE_WITH_CONCERNS");
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}: status \"DONE_WITH_CONCERNS\" needs a non-null self_audit",
                file.display()
            )]
        );
    }

    /// Rejects a DONE report whose self_audit.summary.skipped is greater
    /// than 0.
    #[test]
    fn rejects_a_done_report_whose_self_audit_summary_skipped_is_greater_than_0() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"]["summary"]["skipped"] = json!(2);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}: self_audit.summary.skipped is 2; re-run audit with --report",
                file.display()
            )]
        );
    }

    /// A `+=` of a string onto a textList field appends one list item per
    /// character. 25 single-character `self_review` items must fail,
    /// naming the first shredded item's own minLength floor.
    #[test]
    fn rejects_a_self_review_shredded_into_one_character_per_item() {
        let root = schema_root();
        let mut report = report_sample();
        let shredded: Vec<Value> = "Re-read the whole change."
            .chars()
            .map(|c| json!(c.to_string()))
            .collect();
        assert_eq!(
            shredded.len(),
            25,
            "the incident's own probe carried 25 items"
        );
        report["self_review"] = json!(shredded);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.ends_with(".self_review[0]: shorter than 3")),
            "{:?}",
            result.errors
        );
    }

    /// Accepts a BLOCKED report with a null self_audit.
    #[test]
    fn accepts_a_blocked_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("BLOCKED");
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// Accepts a NEEDS_CONTEXT report with a null self_audit.
    #[test]
    fn accepts_a_needs_context_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("NEEDS_CONTEXT");
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// Accepts a BLOCKED report whose self_audit.summary.skipped is
    /// greater than 0 -- a non-terminal report's skipped audit rows are
    /// never inspected, pinning `check_task_report_audit`'s early return.
    #[test]
    fn accepts_a_blocked_report_whose_self_audit_summary_skipped_is_greater_than_0() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("BLOCKED");
        report["self_audit"]["summary"]["skipped"] = json!(2);
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// Rejects a task report without live_run.
    #[test]
    fn rejects_a_task_report_without_live_run() {
        let root = schema_root();
        let mut report = report_sample();
        report.as_object_mut().unwrap().remove("live_run");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}: missing \"live_run\"", file.display())]
        );
    }

    /// Rejects a live_run that is not an array.
    #[test]
    fn rejects_a_live_run_that_is_not_an_array() {
        let root = schema_root();
        let mut report = report_sample();
        report["live_run"] = json!("nope");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.live_run: must be array", file.display())]
        );
    }

    /// Rejects a live_run entry without a command.
    #[test]
    fn rejects_a_live_run_entry_without_a_command() {
        let root = schema_root();
        let mut report = report_sample();
        report["live_run"] = json!([{"output": "ok"}]);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.live_run[0]: missing \"command\"",
                file.display()
            )]
        );
    }

    /// Rejects a tdd cycle without mode.
    #[test]
    fn rejects_a_tdd_cycle_without_mode() {
        let root = schema_root();
        let mut report = report_sample();
        report["tdd"][0].as_object_mut().unwrap().remove("mode");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.tdd[0]: missing \"mode\"", file.display())]
        );
    }

    /// Rejects a tdd cycle with an unknown mode.
    #[test]
    fn rejects_a_tdd_cycle_with_an_unknown_mode() {
        let root = schema_root();
        let mut report = report_sample();
        report["tdd"][0]["mode"] = json!("guessed");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.tdd[0].mode: must be one of \"natural\", \"mutation\", \"reconstructed\"",
                file.display()
            )]
        );
    }

    /// Accepts a self_audit summary stamped empty_range: true.
    #[test]
    fn accepts_a_self_audit_summary_stamped_empty_range_true() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"] = json!({
            "summary": {
                "base": "abc1234", "head": "abc1234", "deterministic": 1,
                "pass": 1, "fail": 0, "warn": 0, "skipped": 0, "judged": 0,
                "empty_range": true,
            },
            "rows": [{
                "id": "process.sequential", "mode": "deterministic",
                "result": "pass", "evidence": "empty range: 0 commits checked",
            }],
        });
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// A fix round's own audit output can carry a skipped report-field
    /// check the same way the top-level `self_audit` can -- rejects it
    /// there too.
    #[test]
    fn rejects_a_fix_round_audit_output_with_a_nonzero_skipped_summary() {
        let root = schema_root();
        let mut report = report_sample();
        report["fix_rounds"] = json!([{
            "round": 1,
            "findings": [{"finding": "f", "fix": "x"}],
            "commits": [{"sha": "abc1236", "subject": "fix: x"}],
            "tests": [{
                "command": "houserules audit --base abc1234 --head abc1235",
                "output": serde_json::to_string(&json!({
                    "base": "abc1234", "head": "abc1235", "rules": [],
                    "summary": {
                        "base": "abc1234", "head": "abc1235", "deterministic": 1,
                        "pass": 0, "fail": 0, "warn": 0, "skipped": 3, "judged": 0,
                    },
                })).unwrap(),
            }],
        }]);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.fix_rounds[0].tests[0].output: audit summary skipped is 3; re-run audit with --report",
                file.display()
            )]
        );
    }

    /// A verbatim capture can carry one prose line before the audit JSON
    /// starts (a shell prompt echo, a leading blank line) --
    /// `parse_audit_summary_skipped` must still find the nonzero
    /// `skipped` past it, not only when the JSON is the very first byte.
    #[test]
    fn rejects_a_fix_round_audit_output_with_one_prose_line_before_the_json() {
        let root = schema_root();
        let mut report = report_sample();
        let audit_json = serde_json::to_string(&json!({
            "base": "abc1234", "head": "abc1235", "rules": [],
            "summary": {
                "base": "abc1234", "head": "abc1235", "deterministic": 1,
                "pass": 0, "fail": 0, "warn": 0, "skipped": 2, "judged": 0,
            },
        }))
        .unwrap();
        report["fix_rounds"] = json!([{
            "round": 1,
            "findings": [{"finding": "f", "fix": "x"}],
            "commits": [{"sha": "abc1236", "subject": "fix: x"}],
            "tests": [{
                "command": "houserules audit --base abc1234 --head abc1235",
                "output": format!("Running the audit...\n{audit_json}"),
            }],
        }]);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.fix_rounds[0].tests[0].output: audit summary skipped is 2; re-run audit with --report",
                file.display()
            )]
        );
    }

    /// A stray `{` inside the PROSE ahead of the real audit JSON -- not
    /// the JSON's own opening brace -- defeats the scan:
    /// `parse_audit_summary_skipped` finds this earlier, invalid brace
    /// first, fails to parse from it, and returns `None` without ever
    /// reaching the real, nonzero-skipped JSON later in the same
    /// capture. This is a limit the function's own doc names, measured
    /// here rather than left asserted only in prose.
    #[test]
    fn accepts_a_fix_round_test_whose_prose_contains_a_brace_before_the_json() {
        let root = schema_root();
        let mut report = report_sample();
        let audit_json = serde_json::to_string(&json!({
            "base": "abc1234", "head": "abc1235", "rules": [],
            "summary": {
                "base": "abc1234", "head": "abc1235", "deterministic": 1,
                "pass": 0, "fail": 0, "warn": 0, "skipped": 5, "judged": 0,
            },
        }))
        .unwrap();
        report["fix_rounds"] = json!([{
            "round": 1,
            "findings": [{"finding": "f", "fix": "x"}],
            "commits": [{"sha": "abc1236", "subject": "fix: x"}],
            "tests": [{
                "command": "houserules audit --base abc1234 --head abc1235",
                "output": format!("Compiling {{ not json }}\n{audit_json}"),
            }],
        }]);
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new(),
            "documents the residual gap: the stray brace in the prose is not skipped past"
        );
    }

    /// A fix round audit whose skipped count is 0 raises nothing --
    /// pins `parse_audit_summary_skipped`'s threshold, not merely its
    /// presence.
    #[test]
    fn accepts_a_fix_round_audit_output_with_a_zero_skipped_summary() {
        let root = schema_root();
        let mut report = report_sample();
        report["fix_rounds"] = json!([{
            "round": 1,
            "findings": [{"finding": "f", "fix": "x"}],
            "commits": [{"sha": "abc1236", "subject": "fix: x"}],
            "tests": [{
                "command": "houserules audit --base abc1234 --head abc1235 --report r.json",
                "output": serde_json::to_string(&json!({
                    "base": "abc1234", "head": "abc1235", "rules": [],
                    "summary": {
                        "base": "abc1234", "head": "abc1235", "deterministic": 1,
                        "pass": 1, "fail": 0, "warn": 0, "skipped": 0, "judged": 0,
                    },
                })).unwrap(),
            }],
        }]);
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// A fix round test whose output is not JSON at all (cargo/vitest
    /// prose, say) is inspected and quietly skipped, not misread as an
    /// audit summary.
    #[test]
    fn accepts_a_fix_round_test_whose_output_is_not_json() {
        let root = schema_root();
        let mut report = report_sample();
        report["fix_rounds"] = json!([{
            "round": 1,
            "findings": [{"finding": "f", "fix": "x"}],
            "commits": [{"sha": "abc1236", "subject": "fix: x"}],
            "tests": [{"command": "cargo test", "output": "running 1 test\ntest ok\n"}],
        }]);
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// Rejects a self_audit summary with empty_range: false.
    #[test]
    fn rejects_a_self_audit_summary_with_empty_range_false() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"] = json!({
            "summary": {
                "base": "abc1234", "head": "abc1235", "deterministic": 1,
                "pass": 1, "fail": 0, "warn": 0, "skipped": 0, "judged": 0,
                "empty_range": false,
            },
            "rows": [{
                "id": "process.sequential", "mode": "deterministic",
                "result": "pass", "evidence": "1 commits checked",
            }],
        });
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.self_audit.summary.empty_range: must be one of true",
                file.display()
            )]
        );
    }

    /// Reports a bad enum value and an unknown field.
    #[test]
    fn reports_a_bad_enum_value_and_an_unknown_field() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("MAYBE");
        report["extra"] = json!(1);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![
                format!(
                    "{}.status: must be one of \"DONE\", \"DONE_WITH_CONCERNS\", \"BLOCKED\", \"NEEDS_CONTEXT\"",
                    file.display()
                ),
                format!("{}: unknown field \"extra\"", file.display()),
            ]
        );
    }

    /// Accepts a run whose exit code is an integer.
    #[test]
    fn accepts_a_run_whose_exit_code_is_an_integer() {
        let root = schema_root();
        let mut report = report_sample();
        report["tests"] = json!([{"command": "vitest", "output": "ok", "exit": 2}]);
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// Rejects a run whose exit code is not an integer.
    #[test]
    fn rejects_a_run_whose_exit_code_is_not_an_integer() {
        let root = schema_root();
        let mut report = report_sample();
        report["tests"] = json!([{"command": "vitest", "output": "ok", "exit": "2"}]);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.tests[0].exit: must be integer", file.display())]
        );
    }

    /// Validates a task-review, rejecting a rule_adherence result the
    /// schema forbids.
    #[test]
    fn validates_a_task_review_rejecting_a_forbidden_rule_adherence_result() {
        let root = schema_root();
        let file = root.path().join("review.json");
        fs::write(
            &file,
            serde_json::to_string(&json!({
                "kind": "task-review", "task": 1, "base": "abc1234", "head": "abc1235",
                "spec_compliance": {"verdict": "compliant", "items": []},
                "rule_adherence": [{"id": "a.b", "mode": "judged", "result": "open", "evidence": "x"}],
                "strengths": [], "issues": [],
                "assessment": {"verdict": "approved", "text": "ok"},
            }))
            .unwrap(),
        )
        .unwrap();
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(result.kind, "task-review");
        assert_eq!(result.errors.len(), 1);
        assert!(
            result.errors[0].ends_with(
                "rule_adherence[0].result: must be one of \"pass\", \"fail\", \"warn\", \"skipped\""
            ),
            "{:?}",
            result.errors
        );
    }

    fn re_review_sample() -> Value {
        json!({
            "kind": "re-review", "task": 1, "round": 1, "fix_base": "abc1234", "head": "abc1235",
            "finding_verdicts": [{"finding": "f", "verdict": "addressed", "evidence": "a.mjs:1"}],
            "rule_adherence": [{"id": "process.sequential", "mode": "judged", "result": "pass", "evidence": "x"}],
            "new_breakage": [], "out_of_scope": [],
            "verdict": {"state": "all-addressed", "open": []},
        })
    }

    /// Accepts a re-review verdict with text.
    #[test]
    fn accepts_a_re_review_verdict_with_text() {
        let root = schema_root();
        let mut re_review = re_review_sample();
        re_review["verdict"]["text"] = json!("scheduled for task 7");
        let file = root.path().join("re-review.json");
        fs::write(&file, serde_json::to_string(&re_review).unwrap()).unwrap();
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// Accepts a re-review verdict without text.
    #[test]
    fn accepts_a_re_review_verdict_without_text() {
        let root = schema_root();
        let file = root.path().join("re-review.json");
        fs::write(&file, serde_json::to_string(&re_review_sample()).unwrap()).unwrap();
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// Rejects a re-review verdict.text of the wrong type.
    #[test]
    fn rejects_a_re_review_verdict_text_of_the_wrong_type() {
        let root = schema_root();
        let mut re_review = re_review_sample();
        re_review["verdict"]["text"] = json!(42);
        let file = root.path().join("re-review.json");
        fs::write(&file, serde_json::to_string(&re_review).unwrap()).unwrap();
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.verdict.text: must be string", file.display())]
        );
    }

    /// A minimal, schema-valid branch review: one retrospective
    /// `violated_rules` entry whose own `tasks` list names task numbers,
    /// not prose.
    fn branch_review_sample() -> Value {
        json!({
            "kind": "branch-review", "base": "abc1234", "head": "abc1235",
            "strengths": ["Clear commit history across the whole branch."],
            "issues": [], "rule_adherence": [],
            "recommendations": ["Ship the next batch at the same steady pace."],
            "retrospective": {
                "violated_rules": [{
                    "id": "process.tdd", "count": 1, "tasks": ["1"], "proposal": {},
                }],
                "uncovered_findings": [], "stale_entries": [], "unused_ids": [],
                "template_defects": [],
            },
            "assessment": {"ready": "yes", "text": "Ready to merge as is."},
        })
    }

    /// A single-character `tasks` reference is accepted, unlike a
    /// single-character prose field.
    #[test]
    fn accepts_a_well_formed_branch_review_with_no_errors() {
        let root = schema_root();
        let file = root.path().join("branch-review.json");
        fs::write(
            &file,
            serde_json::to_string(&branch_review_sample()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// The schema's `prose` type (`minLength: 3`, used by `self_review`,
    /// `concerns`, `strengths`, `out_of_scope`, `verdict.open`,
    /// `recommendations`) and its `text` type (`minLength: 1`, used by
    /// the retrospective `tasks` lists) enforce two different floors on
    /// purpose: a `tasks` entry legitimately holds a single-digit
    /// task-number reference ("1", "2"), which no single floor could gate
    /// without also gating real task numbers. This document carries a
    /// single-character `tasks` item (legitimate, unflagged) alongside a
    /// single-character `recommendations` item (illegitimate, flagged) to
    /// prove the split in one assertion.
    #[test]
    fn accepts_a_single_character_task_reference_while_rejecting_an_equally_short_recommendation() {
        let root = schema_root();
        let mut branch_review = branch_review_sample();
        branch_review["recommendations"] = json!(["ok"]);
        let file = root.path().join("branch-review.json");
        fs::write(&file, serde_json::to_string(&branch_review).unwrap()).unwrap();
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.recommendations[0]: shorter than 3",
                file.display()
            )]
        );
    }

    /// Rejects an unknown kind, a missing file, and invalid JSON as usage
    /// errors.
    #[test]
    fn rejects_an_unknown_kind_a_missing_file_and_invalid_json() {
        let root = schema_root();
        let memo = root.path().join("memo.json");
        fs::write(
            &memo,
            serde_json::to_string(&json!({"kind": "memo"})).unwrap(),
        )
        .unwrap();
        let error = validate_deliverable(root.path(), &memo).unwrap_err();
        assert!(
            error.contains("unknown deliverable kind \"memo\""),
            "{error}"
        );

        let missing = root.path().join("missing.json");
        assert!(validate_deliverable(root.path(), &missing).is_err());

        let broken = root.path().join("broken.json");
        fs::write(&broken, "{").unwrap();
        let error = validate_deliverable(root.path(), &broken).unwrap_err();
        assert!(error.contains("invalid JSON"), "{error}");
    }
}
