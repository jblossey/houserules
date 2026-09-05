//! The `stats` command: aggregates rule violations and unused injected ids
//! across a workspace's JSON deliverables -- `tools/kb.mjs`'s `stats`,
//! ported byte-for-byte (batch 17 T3, docs/specs/2026-09-04-batch-15-tier2-
//! spec.md §5 phase 2). See `deliverable.rs`'s module doc for why every
//! read here is a tolerant `serde_json::Value`, never a typed deliverable
//! model.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::emit::emit;

use super::deliverable::{array_field, read_deliverable_value, workspace_files};
use super::model::load_base;

/// Records that task `task` triggered (or violated) rule `id`, deduplicating
/// repeats -- `tools/kb.mjs`'s `statsHit`.
fn stats_hit(map: &mut BTreeMap<String, BTreeSet<String>>, id: &str, task: &str) {
    map.entry(id.to_string())
        .or_default()
        .insert(task.to_string());
}

/// The task label between `task-` and the first following `-` in a
/// deliverable filename -- `tools/kb.mjs`'s `statsTask`
/// (`name.match(/^task-([^-]+)/)[1]`). `workspace_files` already restricts
/// its callers to names starting `task-<something>`, so the fallback (the
/// whole name) is unreached in practice; it exists so this never panics on
/// an unexpected shape, matching `quality.principles`' preference for a
/// checked result over a crash where the JS itself would throw
/// (`null[1]` on a failed `.match()`).
fn stats_task(name: &str) -> &str {
    match name.strip_prefix("task-") {
        Some(rest) => match rest.find('-') {
            Some(0) | None => name,
            Some(end) => &rest[..end],
        },
        None => name,
    }
}

/// Renders `tasks` as the sorted `Vec<&str>` -> JSON array `stats` reports
/// under `tasks` -- `tools/kb.mjs`'s `statsTasks` (`[...set].toSorted()`);
/// `BTreeSet` already iterates sorted, so this is just the JSON shape.
fn tasks_json(tasks: &BTreeSet<String>) -> Value {
    Value::Array(tasks.iter().cloned().map(Value::String).collect())
}

/// Aggregates rule violations and unused injected ids across a workspace's
/// JSON deliverables: `task-*-audit*.json` for injected ids and
/// deterministic failures, `task-*-review*.json` for judged failures,
/// `task-<n>-report.json` for the ids a report cites as used --
/// `tools/kb.mjs`'s `stats`.
pub(super) fn stats(dir: &Path) -> Result<Value, String> {
    let files = workspace_files(dir)?;
    let mut violations: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut injected: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for name in &files.audits {
        let data = read_deliverable_value(&dir.join(name))?;
        let task = stats_task(name).to_string();
        let ids = array_field(&data, "ids").map_err(|error| format!("{name}: {error}"))?;
        for id in ids.into_iter().filter_map(Value::as_str) {
            stats_hit(&mut injected, id, &task);
        }
        let rules = array_field(&data, "rules").map_err(|error| format!("{name}: {error}"))?;
        for rule in rules {
            if rule.get("result").and_then(Value::as_str) == Some("fail")
                && let Some(id) = rule.get("id").and_then(Value::as_str)
            {
                stats_hit(&mut violations, id, &task);
            }
        }
    }
    for name in &files.reviews {
        let data = read_deliverable_value(&dir.join(name))?;
        let task = stats_task(name).to_string();
        let rule_adherence =
            array_field(&data, "rule_adherence").map_err(|error| format!("{name}: {error}"))?;
        for row in rule_adherence {
            if row.get("mode").and_then(Value::as_str) == Some("judged")
                && row.get("result").and_then(Value::as_str) == Some("fail")
                && let Some(id) = row.get("id").and_then(Value::as_str)
            {
                stats_hit(&mut violations, id, &task);
            }
        }
    }
    let mut cited: HashSet<String> = HashSet::new();
    for name in &files.reports {
        let data = read_deliverable_value(&dir.join(name))?;
        let knowledge_used =
            array_field(&data, "knowledge_used").map_err(|error| format!("{name}: {error}"))?;
        for id in knowledge_used.into_iter().filter_map(Value::as_str) {
            cited.insert(id.to_string());
        }
    }

    let violations_json: Vec<Value> = violations
        .iter()
        .map(|(id, tasks)| json!({"id": id, "count": tasks.len(), "tasks": tasks_json(tasks)}))
        .collect();
    let unused_ids_json: Vec<Value> = injected
        .iter()
        .filter(|(id, _)| !cited.contains(id.as_str()))
        .map(|(id, tasks)| json!({"id": id, "tasks": tasks_json(tasks)}))
        .collect();
    let audit_tasks: HashSet<&str> = files.audits.iter().map(|name| stats_task(name)).collect();

    Ok(json!({
        "violations": violations_json,
        "unused_ids": unused_ids_json,
        "audits": {"files": files.audits.len(), "tasks": audit_tasks.len()},
        "reviews": {"files": files.reviews.len()},
    }))
}

/// Runs the `stats` subcommand: resolves `root` (`--dir`, or the enclosing
/// git repository's top level) and loads the knowledge base there before
/// dispatching -- `tools/kb.mjs`'s `main` calls `loadBase(repoRoot(cwd))`
/// unconditionally ahead of its command `switch`, for every command
/// including `stats`, even though `stats` itself never reads the result;
/// replicated here for parity's sake (a repository whose knowledge base
/// fails to load fails `stats` too in the frozen JS, not only `audit`).
/// Then runs `stats` over `workspace` and prints its JSON result.
pub(crate) fn cmd_stats(dir: Option<PathBuf>, workspace: PathBuf) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    if let Err(error) = load_base(&root) {
        eprintln!("{error}");
        return ExitCode::from(2);
    }
    match stats(&workspace) {
        Ok(value) => {
            print!("{}", emit(&value));
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    fn stats_rules(result: &str) -> Value {
        json!([{
            "id": "a.rule", "kind": "rule", "mode": "deterministic",
            "level": "fail", "result": result, "evidence": "",
        }])
    }

    /// tests/kb.test.mjs, describe('stats'): "aggregates violations, unused
    /// ids, and file counts from a workspace of JSON deliverables".
    #[test]
    fn aggregates_violations_unused_ids_and_file_counts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(
            root.join("task-1-audit.json"),
            serde_json::to_string(&json!({"ids": ["a.rule", "c.d"], "rules": stats_rules("fail")}))
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("task-2-audit-r1.json"),
            serde_json::to_string(&json!({"ids": ["a.rule"], "rules": stats_rules("pass")}))
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("task-1-report.json"),
            serde_json::to_string(
                &json!({"kind": "task-report", "knowledge_used": ["a.rule", "b.c"]}),
            )
            .unwrap(),
        )
        .unwrap();
        // The old markdown-report contract; stats must ignore it entirely.
        fs::write(
            root.join("task-1-report.md"),
            "# r\n\nKnowledge used: a.rule, b.c\n",
        )
        .unwrap();
        fs::write(
            root.join("task-2-review.json"),
            serde_json::to_string(&json!({
                "kind": "task-review",
                "rule_adherence": [
                    {"id": "x.y", "mode": "judged", "result": "fail", "evidence": "ev"},
                    {"id": "a.rule", "mode": "deterministic", "result": "pass", "evidence": "ok"},
                ],
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(root.join("unrelated.txt"), "").unwrap();

        assert_eq!(
            stats(root).unwrap(),
            json!({
                "violations": [
                    {"id": "a.rule", "count": 1, "tasks": ["1"]},
                    {"id": "x.y", "count": 1, "tasks": ["2"]},
                ],
                "unused_ids": [{"id": "c.d", "tasks": ["1"]}],
                "audits": {"files": 2, "tasks": 2},
                "reviews": {"files": 1},
            })
        );

        let empty_dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            stats(empty_dir.path()).unwrap(),
            json!({
                "violations": [],
                "unused_ids": [],
                "audits": {"files": 0, "tasks": 0},
                "reviews": {"files": 0},
            })
        );
    }

    /// tests/kb.test.mjs, describe('stats'): "tolerates an audit file with
    /// no ids or rules" -- the `?? []` fallback for a stats file or a
    /// hand-written one.
    #[test]
    fn tolerates_an_audit_file_with_no_ids_or_rules() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("task-9-audit.json"), "{}").unwrap();
        assert_eq!(
            stats(dir.path()).unwrap(),
            json!({
                "violations": [],
                "unused_ids": [],
                "audits": {"files": 1, "tasks": 1},
                "reviews": {"files": 0},
            })
        );
    }

    /// tests/kb.test.mjs, describe('stats'): "tolerates a review with no
    /// rule_adherence and a report with no knowledge_used".
    #[test]
    fn tolerates_a_review_with_no_rule_adherence_and_a_report_with_no_knowledge_used() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-review.json"),
            serde_json::to_string(&json!({"kind": "task-review"})).unwrap(),
        )
        .unwrap();
        fs::write(
            dir.path().join("task-1-report.json"),
            serde_json::to_string(&json!({"kind": "task-report"})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            stats(dir.path()).unwrap(),
            json!({
                "violations": [],
                "unused_ids": [],
                "audits": {"files": 0, "tasks": 0},
                "reviews": {"files": 1},
            })
        );
    }

    /// tests/kb.test.mjs, describe('stats'): "raises a UsageError naming a
    /// malformed deliverable file, instead of crashing".
    #[test]
    fn reports_a_malformed_deliverable_file_naming_it_instead_of_crashing() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("task-3-audit.json"), "{\"ids\": [").unwrap();
        let error = stats(dir.path()).unwrap_err();
        assert!(error.contains("task-3-audit.json"), "{error}");
    }

    /// Fix round 1, issue 7 (task-3-review.json): a present-but-wrongly-
    /// typed `rules` field is a named finding, not silence. The frozen JS
    /// crashes here (`for (const rule of data.rules)` on an object throws
    /// `TypeError: ... is not iterable`, the reviewer's own measured
    /// reproduction); this binary instead names the file and the field.
    #[test]
    fn reports_a_wrongly_typed_rules_field_naming_the_file_instead_of_silently_skipping_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-audit.json"),
            serde_json::to_string(&json!({"ids": [], "rules": {"a": 1}})).unwrap(),
        )
        .unwrap();
        let error = stats(dir.path()).unwrap_err();
        assert!(error.contains("task-1-audit.json"), "{error}");
        assert!(error.contains("rules"), "{error}");
    }

    /// Fix round 1, issue 7: the same treatment for `ids`, the sibling
    /// field on the same `task-*-audit*.json` shape.
    #[test]
    fn reports_a_wrongly_typed_ids_field_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-audit.json"),
            serde_json::to_string(&json!({"ids": {"a": 1}, "rules": []})).unwrap(),
        )
        .unwrap();
        let error = stats(dir.path()).unwrap_err();
        assert!(error.contains("task-1-audit.json"), "{error}");
        assert!(error.contains("ids"), "{error}");
    }

    /// Fix round 1, issue 7: `rule_adherence`, the review-side sibling.
    #[test]
    fn reports_a_wrongly_typed_rule_adherence_field_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-review.json"),
            serde_json::to_string(&json!({"kind": "task-review", "rule_adherence": {"a": 1}}))
                .unwrap(),
        )
        .unwrap();
        let error = stats(dir.path()).unwrap_err();
        assert!(error.contains("task-1-review.json"), "{error}");
        assert!(error.contains("rule_adherence"), "{error}");
    }

    /// Fix round 1, issue 7: `knowledge_used`, the report-side sibling.
    #[test]
    fn reports_a_wrongly_typed_knowledge_used_field_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-report.json"),
            serde_json::to_string(&json!({"kind": "task-report", "knowledge_used": {"a": 1}}))
                .unwrap(),
        )
        .unwrap();
        let error = stats(dir.path()).unwrap_err();
        assert!(error.contains("task-1-report.json"), "{error}");
        assert!(error.contains("knowledge_used"), "{error}");
    }
}
