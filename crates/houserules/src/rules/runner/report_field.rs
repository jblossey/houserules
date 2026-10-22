//! The `report-field` check: a report that the changed files trigger must
//! hold a value for the check's `field`, and the check's options can ask for
//! more than a value (`non_empty`, `covers_ids`). The same module compares a
//! report's `self_audit` with a fresh run of the audit (`compare_self_audit`).

use std::path::Path;

use serde_json::{Map, Value};

use crate::rules::check_shape::{CheckDef, Glob};
use crate::rules::git::full_commit_sha;

use super::{AuditContext, RowBuilder, filter_matching, match_any};

/// The value at a dot-separated `field` path in `data`, indexing into
/// either an object's keys or an array's numeric-string indices.
fn field_value<'a>(data: &'a Value, field: &str) -> Option<&'a Value> {
    field.split('.').try_fold(data, |node, key| match node {
        Value::Object(map) => map.get(key),
        Value::Array(items) => key.parse::<usize>().ok().and_then(|index| items.get(index)),
        _ => None,
    })
}

/// `true` when `field_value` is present and not `null`.
fn has_field(data: &Value, field: &str) -> bool {
    !matches!(field_value(data, field), None | Some(Value::Null))
}

/// `true` when `value` is an empty string, array, or object. Any other value,
/// a number or a boolean included, holds something.
fn is_empty_value(value: &Value) -> bool {
    match value {
        Value::String(text) => text.is_empty(),
        Value::Array(items) => items.is_empty(),
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

/// The first id of `ids` that `value` does not hold, or `None` when it holds
/// them all. A value that is no array holds no id.
fn first_missing_id<'a>(value: &Value, ids: &'a [String]) -> Option<&'a String> {
    ids.iter().find(|id| {
        !value
            .as_array()
            .is_some_and(|items| items.iter().any(|item| item.as_str() == Some(id.as_str())))
    })
}

/// The one report field that holds the audit's own output. An implementer
/// writes the draft report with `self_audit: null`, runs the audit, and
/// copies the audit's printed `summary` and deterministic rows into the
/// field. `houserules validate` guards the field's final state: it rejects a
/// `DONE` or `DONE_WITH_CONCERNS` report whose `self_audit` is `null`
/// (`check_task_report_audit` in `validate_deliverable.rs`).
const SELF_AUDIT_FIELD: &str = "self_audit";

/// `true` when `field` is `self_audit` and `report` holds that key, filled or
/// as an explicit JSON `null` (the draft state, before the implementer has
/// copied the audit's rows into it). A report that lacks the key is not a
/// draft, and no other field has a draft state.
fn self_audit_is_present(report: &Value, field: &str) -> bool {
    field == SELF_AUDIT_FIELD && field_value(report, field).is_some()
}

/// Judges one `report-field` check: with `--report`, whether that report
/// holds a value for `check.field` that meets the options of the check; with
/// `--workspace`, whether every report whose `files_changed` triggers the
/// check does; with neither, the row is `skipped`. A `--report` run also
/// accepts the draft state of `self_audit` (see `self_audit_is_present`): a
/// `null` and a filled field get the same row, `report field self_audit is
/// present`. A `--workspace` run does not accept the draft state.
pub(super) fn run(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<Value, String> {
    let trigger = filter_matching(ctx.changed, &check.if_changed)?;
    if trigger.is_empty() {
        return Ok(rows.pass("not triggered".to_string()));
    }
    if let Some(report) = ctx.report {
        return Ok(judge_report(check, ctx, rows, report, &trigger[0]));
    }
    if let Some(reports) = ctx.reports {
        return judge_reports(check, rows, reports);
    }
    Ok(rows.row("skipped", "no --report given".to_string()))
}

/// The row of a `--report` run. A filled `self_audit` marks the row for the
/// comparison of `compare_self_audit`. Any other field must hold a value
/// (`trigger` names the changed file that triggered the check), and then the
/// options apply in this order: `non_empty`, then `covers_ids`. `covers_ids`
/// passes when the audit has no `--ids`.
fn judge_report(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
    report: &Value,
    trigger: &str,
) -> Value {
    let field = check.field.as_deref().unwrap_or_default();
    if self_audit_is_present(report, field) {
        if has_field(report, field) {
            ctx.note_self_audit_row(&rows.entry.id);
        }
        return rows.pass(format!("report field {field} is present"));
    }
    let Some(value) = field_value(report, field).filter(|value| !value.is_null()) else {
        return rows.violate(format!(
            "report lacks a value for {field} (triggered by {trigger})"
        ));
    };
    if check.non_empty == Some(true) && is_empty_value(value) {
        return rows.violate(format!("report field {field} is empty"));
    }
    if check.covers_ids == Some(true)
        && let Some(missing) = first_missing_id(value, ctx.ids)
    {
        return rows.violate(format!("report field {field} lacks {missing} of --ids"));
    }
    rows.pass(format!("report field {field} is set"))
}

/// The `files_changed` paths of one report: the strings of its array, and
/// none when the key is no array.
fn report_files_changed(data: &Value) -> Vec<&str> {
    data.get("files_changed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// The first of a report's `files_changed` matching `globs`, or `None` when
/// none do. A malformed glob is an `Err` (`?`), not a false "did not match".
fn matching_file<'a>(data: &'a Value, globs: &Option<Glob>) -> Result<Option<&'a str>, String> {
    for file in report_files_changed(data) {
        if match_any(file, globs)? {
            return Ok(Some(file));
        }
    }
    Ok(None)
}

/// The row of a `--workspace` run over `reports`. A report whose
/// `files_changed` is no array of strings is named as lacking it; the shape
/// is just as unusable for glob-matching as a missing key, so it is no
/// element-by-element filter. Each report that the check triggers must hold a
/// value, and with `non_empty` a value that is not empty; the first report
/// that breaks either fails the row. `covers_ids` does not run here, because
/// each report has its own `--ids`: a passing row of a check that holds it adds
/// `; covers_ids needs --report` to the report counts.
fn judge_reports(
    check: &CheckDef,
    rows: &RowBuilder,
    reports: &[(String, Value)],
) -> Result<Value, String> {
    let field = check.field.as_deref().unwrap_or_default();
    let has_valid_files_changed = |data: &Value| matches!(data.get("files_changed"), Some(Value::Array(items)) if items.iter().all(Value::is_string));
    if let Some((name, _)) = reports
        .iter()
        .find(|(_, data)| !has_valid_files_changed(data))
    {
        return Ok(rows.violate(format!("{name} lacks files_changed")));
    }
    let mut hits: Vec<&(String, Value)> = Vec::new();
    for entry in reports {
        if matching_file(&entry.1, &check.if_changed)?.is_some() {
            hits.push(entry);
        }
    }
    if hits.is_empty() {
        return Ok(rows.pass("not triggered by any report".to_string()));
    }
    for (name, data) in &hits {
        if !has_field(data, field) {
            let file = matching_file(data, &check.if_changed)?.unwrap_or_default();
            return Ok(rows.violate(format!(
                "{name} lacks a value for {field} (triggered by {file})"
            )));
        }
        if check.non_empty == Some(true) && field_value(data, field).is_some_and(is_empty_value) {
            return Ok(rows.violate(format!("{name}: report field {field} is empty")));
        }
    }
    let counts = format!(
        "report field {field} is set in {} of {} reports",
        hits.len(),
        reports.len()
    );
    if check.covers_ids == Some(true) {
        return Ok(rows.pass(format!("{counts}; covers_ids needs --report")));
    }
    Ok(rows.pass(counts))
}

/// The keys of an audit summary that the comparison leaves out: `base` and
/// `head`, which it reads as commits, not as text.
const SUMMARY_KEYS_LEFT_OUT: [&str; 2] = ["base", "head"];

/// The keys of an audit summary that count the rows by result.
const RESULT_COUNT_KEYS: [&str; 4] = ["pass", "fail", "warn", "skipped"];

/// The key of an audit summary that counts the booked `fail` rows. The audit
/// prints it only above zero.
const SANCTIONED_COUNT_KEY: &str = "sanctioned_fail";

/// A comparing row as the counts of a summary see it.
struct CountedRow<'a> {
    /// The `result` of the row.
    result: &'a str,
    /// `true` when the row is a `fail` that carries `sanctioned`.
    booked_fail: bool,
}

/// The rows of `comparing` among `rows`, which is an array of audit rows. Any
/// other value holds none.
fn counted_rows<'a>(rows: &'a Value, comparing: &[String]) -> Vec<CountedRow<'a>> {
    rows.as_array()
        .into_iter()
        .flatten()
        .filter(|row| is_row_of(row, comparing))
        .filter_map(|row| {
            let result = row.get("result")?.as_str()?;
            let booked = row.get("sanctioned").is_some_and(Value::is_string);
            Some(CountedRow {
                result,
                booked_fail: result == "fail" && booked,
            })
        })
        .collect()
}

/// How many of `counted` the summary count at `key` includes: the rows with
/// that result for a result count, the booked `fail` rows for
/// `SANCTIONED_COUNT_KEY`, and none for any other key.
fn rows_counted_under(key: &str, counted: &[CountedRow]) -> i64 {
    counted
        .iter()
        .filter(|row| {
            if RESULT_COUNT_KEYS.contains(&key) {
                row.result == key
            } else {
                key == SANCTIONED_COUNT_KEY && row.booked_fail
            }
        })
        .fold(0, |total, _| total + 1)
}

/// The text of the value at `key` in the summary `copy`, when it is a string.
fn recorded_sha<'a>(copy: &'a Value, key: &str) -> Option<&'a str> {
    copy.get("summary")?.get(key)?.as_str()
}

/// `true` when the recorded sha `recorded` and the sha `current` of this run
/// name one commit. A sha that names no commit, or more than one, is no
/// match.
fn same_commit(root: &Path, recorded: Option<&str>, current: &str) -> bool {
    let Some(recorded) = recorded.and_then(|sha| full_commit_sha(root, sha)) else {
        return false;
    };
    full_commit_sha(root, current).is_some_and(|sha| sha == recorded)
}

/// The path of the first difference between `recorded` and `current`, which
/// are both at `path`. Objects compare by key, in the order of `recorded`
/// first; arrays compare by position; a key or an element that only one side
/// holds is a difference at its own path.
fn first_difference(path: &str, recorded: &Value, current: &Value) -> Option<String> {
    match (recorded, current) {
        (Value::Object(recorded), Value::Object(current)) => {
            for (key, value) in recorded {
                let at = format!("{path}.{key}");
                match current.get(key) {
                    Some(other) => {
                        if let Some(found) = first_difference(&at, value, other) {
                            return Some(found);
                        }
                    }
                    None => return Some(at),
                }
            }
            current
                .keys()
                .find(|key| !recorded.contains_key(*key))
                .map(|key| format!("{path}.{key}"))
        }
        (Value::Array(recorded), Value::Array(current)) => {
            for index in 0..recorded.len().max(current.len()) {
                let at = format!("{path}[{index}]");
                match (recorded.get(index), current.get(index)) {
                    (Some(left), Some(right)) => {
                        if let Some(found) = first_difference(&at, left, right) {
                            return Some(found);
                        }
                    }
                    _ => return Some(at),
                }
            }
            None
        }
        _ => (recorded != current).then(|| path.to_string()),
    }
}

/// `summary` as it enters the comparison: an object without the keys of
/// `SUMMARY_KEYS_LEFT_OUT`, with each of the `counted` comparing rows taken
/// out of the counts that include it. A count that is no integer, and a count
/// that no row accounts for, stay as they are. A `SANCTIONED_COUNT_KEY` that
/// falls to zero is left out, as the audit leaves it out at zero. Any other
/// value stays as it is.
fn summary_compared(summary: &Value, counted: &[CountedRow]) -> Value {
    let Value::Object(map) = summary else {
        return summary.clone();
    };
    let mut compared = Map::new();
    for (key, value) in map {
        if SUMMARY_KEYS_LEFT_OUT.contains(&key.as_str()) {
            continue;
        }
        let taken = rows_counted_under(key, counted);
        match value
            .as_i64()
            .filter(|_| taken > 0)
            .map(|count| count - taken)
        {
            Some(0) if key == SANCTIONED_COUNT_KEY => {}
            Some(count) => {
                compared.insert(key.clone(), Value::from(count));
            }
            None => {
                compared.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(compared)
}

/// The JSON path of the first difference between the `self_audit` copy and
/// the final output `result` of this run, or `None` when they agree. The
/// compared set is the summary (`summary_compared`: the counts less the rows of
/// `comparing` on each side, so a copy that lacks those rows keeps its counts)
/// and the rows without the rows of `comparing`. `result` is read as the audit
/// built it, before the comparison moves any row. The copy's rows are all
/// kept, so a judged row that someone wrote into the copy is a difference.
fn self_audit_difference(copy: &Value, result: &Value, comparing: &[String]) -> Option<String> {
    let recorded_summary = summary_compared(
        copy.get("summary").unwrap_or(&Value::Null),
        &counted_rows(copy.get("rows").unwrap_or(&Value::Null), comparing),
    );
    let current_summary = summary_compared(
        &result["summary"],
        &counted_rows(&result["rules"], comparing),
    );
    if let Some(path) = first_difference("self_audit.summary", &recorded_summary, &current_summary)
    {
        return Some(path);
    }
    let Some(Value::Array(recorded)) = copy.get("rows") else {
        return Some("self_audit.rows".to_string());
    };
    let recorded: Vec<(usize, &Value)> = recorded
        .iter()
        .enumerate()
        .filter(|(_, row)| !is_row_of(row, comparing))
        .collect();
    let current: Vec<&Value> = result["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row.get("mode").and_then(Value::as_str) == Some("deterministic"))
        .filter(|row| !is_row_of(row, comparing))
        .collect();
    let recorded_len = copy["rows"].as_array().map_or(0, Vec::len);
    for position in 0..recorded.len().max(current.len()) {
        match (recorded.get(position), current.get(position)) {
            (Some((index, left)), Some(right)) => {
                if let Some(path) =
                    first_difference(&format!("self_audit.rows[{index}]"), left, right)
                {
                    return Some(path);
                }
            }
            (Some((index, _)), None) => return Some(format!("self_audit.rows[{index}]")),
            _ => return Some(format!("self_audit.rows[{recorded_len}]")),
        }
    }
    None
}

/// `true` when `row` is the row of one of the entries in `ids`.
fn is_row_of(row: &Value, ids: &[String]) -> bool {
    row.get("id")
        .and_then(Value::as_str)
        .is_some_and(|id| ids.iter().any(|candidate| candidate == id))
}

/// Compares the `self_audit` of `report` with the final output `result` of
/// this run, and sets the verdict on the rows of `comparing` (the rows of the
/// checks that found a filled `self_audit`; spec 6).
///
/// The comparison happens only when the copy covers this run's range: each
/// recorded `base` and `head` must name the same commit as the sha of the run,
/// so that a short sha which a repack lengthened still matches. Otherwise each
/// row keeps its result and adds the two ranges and `not compared` to its
/// evidence. A difference fails the rows with `self_audit differs at <path>`;
/// they take the result of their level, and the summary counts move with them.
/// Returns `true` when a row now has the result `fail`.
pub(in crate::rules) fn compare_self_audit(
    root: &Path,
    report: &Value,
    comparing: &[String],
    result: &mut Value,
) -> bool {
    let Some(copy) = report.get(SELF_AUDIT_FIELD).filter(|copy| !copy.is_null()) else {
        return false;
    };
    if comparing.is_empty() {
        return false;
    }
    let run_base = result["summary"]["base"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let run_head = result["summary"]["head"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let covers_this_run = same_commit(root, recorded_sha(copy, "base"), &run_base)
        && same_commit(root, recorded_sha(copy, "head"), &run_head);
    if !covers_this_run {
        let range = |base: &str, head: &str| format!("{base}..{head}");
        let note = format!(
            "self_audit covers {}; this run covers {}: not compared",
            range(
                recorded_sha(copy, "base").unwrap_or("none"),
                recorded_sha(copy, "head").unwrap_or("none")
            ),
            range(&run_base, &run_head),
        );
        for row in rows_of(result, comparing) {
            let evidence = row["evidence"].as_str().unwrap_or_default().to_string();
            row["evidence"] = Value::String(format!("{evidence}; {note}"));
        }
        return false;
    }
    let Some(path) = self_audit_difference(copy, result, comparing) else {
        return false;
    };
    let mut failed = false;
    let mut moved: Vec<&'static str> = Vec::new();
    for row in rows_of(result, comparing) {
        let verdict = if row["level"] == "warn" {
            "warn"
        } else {
            "fail"
        };
        failed |= verdict == "fail";
        row["result"] = Value::String(verdict.to_string());
        row["evidence"] = Value::String(format!("self_audit differs at {path}"));
        moved.push(verdict);
    }
    for verdict in moved {
        let pass = result["summary"]["pass"].as_u64().unwrap_or(0);
        result["summary"]["pass"] = Value::from(pass.saturating_sub(1));
        let count = result["summary"][verdict].as_u64().unwrap_or(0);
        result["summary"][verdict] = Value::from(count + 1);
    }
    failed
}

/// The rows of `result` that belong to the entries in `ids`, for the
/// comparison to change.
fn rows_of<'a>(result: &'a mut Value, ids: &[String]) -> Vec<&'a mut Value> {
    result["rules"]
        .as_array_mut()
        .into_iter()
        .flatten()
        .filter(|row| is_row_of(row, ids))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A string, an array, and an object are empty when they hold nothing. A
    /// number or a boolean holds something, zero and `false` included.
    #[test]
    fn only_an_empty_string_array_or_object_is_empty() {
        for empty in [json!(""), json!([]), json!({})] {
            assert!(is_empty_value(&empty), "{empty}");
        }
        for held in [
            json!(" "),
            json!(["x"]),
            json!({"a": 1}),
            json!(0),
            json!(false),
        ] {
            assert!(!is_empty_value(&held), "{held}");
        }
    }

    /// The first missing id follows the order of `--ids`, ignores a non-string
    /// element, and finds every id missing in a value that is no array.
    #[test]
    fn the_first_missing_id_follows_the_order_of_ids() {
        let ids: Vec<String> = ["b", "a", "c"].map(str::to_string).to_vec();
        assert_eq!(first_missing_id(&json!(["a", "b", "c"]), &ids), None);
        assert_eq!(
            first_missing_id(&json!(["a"]), &ids).map(String::as_str),
            Some("b")
        );
        assert_eq!(
            first_missing_id(&json!(["b", 7, null, "c"]), &ids).map(String::as_str),
            Some("a")
        );
        assert_eq!(
            first_missing_id(&json!("a,b,c"), &ids).map(String::as_str),
            Some("b")
        );
        assert_eq!(first_missing_id(&json!([]), &[]), None);
    }

    /// The path of a difference names the key or the index, and a key or an
    /// element that only one side holds is a difference at its own path.
    #[test]
    fn the_first_difference_names_the_path_of_the_key_or_element() {
        let at = |recorded: Value, current: Value| first_difference("p", &recorded, &current);
        assert_eq!(
            at(json!({"a": [1, {"b": 2}]}), json!({"a": [1, {"b": 2}]})),
            None
        );
        assert_eq!(at(json!({"b": 1, "a": 2}), json!({"a": 2, "b": 1})), None);
        assert_eq!(
            at(json!({"a": [1, {"b": 2}]}), json!({"a": [1, {"b": 3}]})),
            Some("p.a[1].b".to_string())
        );
        assert_eq!(at(json!({"a": 1}), json!({})), Some("p.a".to_string()));
        assert_eq!(at(json!({}), json!({"a": 1})), Some("p.a".to_string()));
        assert_eq!(at(json!([1]), json!([1, 2])), Some("p[1]".to_string()));
        assert_eq!(at(json!([1, 2]), json!([1])), Some("p[1]".to_string()));
        assert_eq!(at(json!(1), json!("1")), Some("p".to_string()));
    }

    /// The final output of a run with three rows: a judged row, one
    /// deterministic row, and the row of the check that compares.
    fn result() -> Value {
        json!({
            "summary": {
                "base": "1111111", "head": "2222222", "deterministic": 2, "pass": 2, "fail": 0,
                "warn": 0, "skipped": 0, "judged": 1, "reference_fail": 0,
            },
            "rules": [
                {"id": "a.judged", "kind": "rule", "mode": "judged", "level": null,
                 "result": "open", "evidence": "-"},
                {"id": "a.other", "kind": "rule", "mode": "deterministic", "level": "fail",
                 "result": "pass", "evidence": "1 commits checked"},
                {"id": "a.self-audit", "kind": "rule", "mode": "deterministic", "level": "fail",
                 "result": "pass", "evidence": "report field self_audit is present"},
            ],
        })
    }

    /// The copy of `result()` as an author writes it: the summary and the
    /// deterministic rows.
    fn copy() -> Value {
        let run = result();
        json!({
            "summary": run["summary"],
            "rows": [run["rules"][1], run["rules"][2]],
        })
    }

    fn comparing() -> Vec<String> {
        vec!["a.self-audit".to_string()]
    }

    /// The compared set leaves out the row of the comparing check, the share
    /// of that row in the counts of the summary, and the shas. A copy of a
    /// failed run, a copy of a run that booked the failed row, a copy whose
    /// comparing row is a `warn`, and a copy with other shas of one commit all
    /// agree with the run.
    #[test]
    fn the_compared_set_leaves_out_the_comparing_row_its_counts_and_the_shas() {
        assert_eq!(
            self_audit_difference(&copy(), &result(), &comparing()),
            None
        );

        let mut failed = copy();
        failed["rows"][1]["result"] = json!("fail");
        failed["rows"][1]["evidence"] = json!("self_audit differs at self_audit.summary");
        failed["summary"]["pass"] = json!(1);
        failed["summary"]["fail"] = json!(1);
        failed["summary"]["base"] = json!("1111111aaaa");
        assert_eq!(
            self_audit_difference(&failed, &result(), &comparing()),
            None
        );

        let mut booked = failed.clone();
        booked["rows"][1]["sanctioned"] = json!("interim");
        booked["summary"]["sanctioned_fail"] = json!(1);
        assert_eq!(
            self_audit_difference(&booked, &result(), &comparing()),
            None,
            "the booked comparing row accounts for the one sanctioned fail"
        );

        let mut warned = copy();
        warned["rows"][1]["result"] = json!("warn");
        warned["summary"]["pass"] = json!(1);
        warned["summary"]["warn"] = json!(1);
        assert_eq!(
            self_audit_difference(&warned, &result(), &comparing()),
            None
        );
    }

    /// Every count of the summary that the comparing row does not account for,
    /// every other row, and every key of a row is compared. A copy that lacks
    /// the comparing row has nothing to take out of its counts. A judged row in
    /// the copy is a difference.
    #[test]
    fn the_compared_set_holds_the_other_counts_rows_and_keys() {
        let at = |copy: Value| self_audit_difference(&copy, &result(), &comparing());

        for (key, count) in [("pass", 3), ("fail", 1), ("warn", 1), ("skipped", 1)] {
            let mut counted = copy();
            counted["summary"][key] = json!(count);
            assert_eq!(
                at(counted),
                Some(format!("self_audit.summary.{key}")),
                "a copy that changes {key}"
            );
        }

        let mut lacking = copy();
        lacking["rows"].as_array_mut().expect("rows").pop();
        assert_eq!(
            at(lacking),
            Some("self_audit.summary.pass".to_string()),
            "a copy without the comparing row has nothing to take out"
        );

        let mut text = copy();
        text["summary"]["pass"] = json!("2");
        assert_eq!(at(text), Some("self_audit.summary.pass".to_string()));

        let mut zero = copy();
        zero["summary"]["sanctioned_fail"] = json!(0);
        assert_eq!(
            at(zero),
            Some("self_audit.summary.sanctioned_fail".to_string()),
            "no comparing row accounts for an explicit zero"
        );

        let mut judged = copy();
        judged["summary"]["judged"] = json!(0);
        assert_eq!(at(judged), Some("self_audit.summary.judged".to_string()));

        let mut reference = copy();
        reference["summary"]["reference_fail"] = json!(1);
        assert_eq!(
            at(reference),
            Some("self_audit.summary.reference_fail".to_string())
        );

        let mut evidence = copy();
        evidence["rows"][0]["evidence"] = json!("2 commits checked");
        assert_eq!(
            at(evidence),
            Some("self_audit.rows[0].evidence".to_string())
        );

        let mut sanctioned = copy();
        sanctioned["rows"][0]["sanctioned"] = json!("interim");
        assert_eq!(
            at(sanctioned),
            Some("self_audit.rows[0].sanctioned".to_string())
        );

        let mut extra_judged = copy();
        extra_judged["rows"]
            .as_array_mut()
            .expect("rows")
            .push(result()["rules"][0].clone());
        assert_eq!(at(extra_judged), Some("self_audit.rows[2]".to_string()));

        let mut no_rows = copy();
        no_rows.as_object_mut().expect("copy").remove("rows");
        assert_eq!(
            at(no_rows),
            Some("self_audit.summary.pass".to_string()),
            "a copy without rows lacks the comparing row too"
        );

        let mut counted_without_rows = copy();
        counted_without_rows
            .as_object_mut()
            .expect("copy")
            .remove("rows");
        counted_without_rows["summary"]["pass"] = json!(1);
        assert_eq!(
            at(counted_without_rows),
            Some("self_audit.rows".to_string()),
            "counts that agree leave the missing rows as the difference"
        );

        assert_eq!(
            at(json!("a string")),
            Some("self_audit.summary".to_string()),
            "a copy that is no object differs at its summary"
        );
    }
}
