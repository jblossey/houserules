//! Reads a JSON deliverable file leniently (never through a typed schema
//! model) and lists a workspace directory's deliverables by kind, shared
//! by `stats.rs` and `audit.rs`.
//!
//! Both callers aggregate across a workspace of agent-authored JSON
//! files (`task-*-audit*.json`, `task-*-report.json`,
//! `task-*-review*.json`) that need not be schema-valid at read time --
//! a report missing `knowledge_used`, a review missing
//! `rule_adherence`, or an audit file missing `ids`/`rules` entirely are
//! all tolerated. A typed, `deny_unknown_fields` parse of, say, the full
//! `TaskReport` shape would fail every one of those cases outright, the
//! moment any *other* field the aggregation never reads is missing or
//! malformed. So both readers stay on raw `serde_json::Value`.
//!
//! Absence and a wrongly-typed present value are still told apart:
//! `array_field` below returns an empty array for a missing key or an
//! explicit `null`, but a named finding for a *present* field of the
//! wrong type (an object where an array is expected, say)
//! (`houserules.crash-paths-are-named`) -- never silence for the second
//! state.

use std::fs;
use std::path::Path;

use regress::Regex;
use serde_json::Value;

/// Reads `field` from `data` as a JSON array, `Ok(&[])` for a missing
/// key or an explicit `null`, or a named error for a present value of
/// any other type -- see the module doc for why this, not a blanket
/// `.and_then(Value::as_array)` that silently treats "wrongly typed" the
/// same as "absent", is correct.
pub(super) fn array_field<'a>(data: &'a Value, field: &str) -> Result<Vec<&'a Value>, String> {
    match data.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items.iter().collect()),
        Some(_) => Err(format!("{field} is not an array")),
    }
}

/// Reads `path` and parses it as JSON, naming the file in either
/// failure: both a missing file and invalid JSON report the same way,
/// since this binary's own CLI-failure-path convention already turns
/// both into one named stderr line and exit 2
/// (`validate_deliverable.rs`/`audit.rs`/`stats.rs`'s own `cmd_*`
/// wrappers), so this function needs only the one `Result` arm.
pub(super) fn read_deliverable_value(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{}: invalid JSON ({error})", path.display()))
}

/// One workspace directory's deliverable filenames by kind, each
/// sorted: `audits` (`task-*-audit*.json`), `reports`
/// (`task-*-report.json`), and `reviews` (`task-*-review*.json`).
pub(super) struct WorkspaceFiles {
    pub audits: Vec<String>,
    pub reports: Vec<String>,
    pub reviews: Vec<String>,
}

/// `true` when `name` matches `pattern` anywhere -- every pattern this
/// module compiles is anchored (`^...$`), so this is a whole-string
/// match. Uses `regress` (already this crate's ECMAScript-regex engine,
/// `check.rs`'s `validate`/`regex_validity_message`) rather than
/// hand-rolling the three static patterns below, so a future pattern
/// change gets the same regex semantics everywhere in this crate.
fn matches(name: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .unwrap_or_else(|error| panic!("{pattern:?} is a static, known-valid pattern: {error}"))
        .find(name)
        .is_some()
}

/// Lists `dir`'s deliverable filenames by kind (see `WorkspaceFiles`), or
/// a named error if `dir` cannot be read.
pub(super) fn workspace_files(dir: &Path) -> Result<WorkspaceFiles, String> {
    let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", dir.display()))?;
        if let Ok(name) = entry.file_name().into_string() {
            names.push(name);
        }
    }
    let mut audits: Vec<String> = names
        .iter()
        .filter(|name| matches(name, r"^task-.+-audit.*\.json$"))
        .cloned()
        .collect();
    let mut reports: Vec<String> = names
        .iter()
        .filter(|name| matches(name, r"^task-.+-report\.json$"))
        .cloned()
        .collect();
    let mut reviews: Vec<String> = names
        .iter()
        .filter(|name| matches(name, r"^task-.+-review.*\.json$"))
        .cloned()
        .collect();
    audits.sort();
    reports.sort();
    reviews.sort();
    Ok(WorkspaceFiles {
        audits,
        reports,
        reviews,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_deliverable_value_reports_invalid_json_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.json");
        fs::write(&path, "not json").expect("write");
        let error = read_deliverable_value(&path).expect_err("invalid JSON");
        assert!(error.contains("invalid JSON"), "{error}");
        assert!(error.contains("bad.json"), "{error}");
    }

    #[test]
    fn read_deliverable_value_reports_a_missing_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("missing.json");
        let error = read_deliverable_value(&path).expect_err("missing file");
        assert!(error.contains("missing.json"), "{error}");
    }

    /// The workspace fixture mixes `task-1-audit.json`,
    /// `task-2-audit-r1.json`, `task-1-report.json`, a lettered
    /// `task-3a-report.json` (plus a decoy `task-1-report.md`),
    /// `task-2-review.json`, an unrelated file, and a branch-level
    /// deliverable `branch-fix-1-report.json` that must land in none of
    /// the three lists -- this pins `workspace_files`' own
    /// classification directly, at both the letter-accepting and the
    /// branch-decoy-rejecting edges of its patterns.
    #[test]
    fn classifies_audits_reports_and_reviews_ignoring_unrelated_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in [
            "task-1-audit.json",
            "task-2-audit-r1.json",
            "task-1-report.json",
            "task-3a-report.json",
            "task-1-report.md",
            "task-2-review.json",
            "unrelated.txt",
            "branch-fix-1-report.json",
        ] {
            fs::write(dir.path().join(name), "{}").expect("write fixture");
        }
        let files = workspace_files(dir.path()).expect("list workspace files");
        assert_eq!(
            files.audits,
            vec!["task-1-audit.json", "task-2-audit-r1.json"]
        );
        assert_eq!(
            files.reports,
            vec!["task-1-report.json", "task-3a-report.json"]
        );
        assert_eq!(files.reviews, vec!["task-2-review.json"]);
        assert!(
            !files
                .audits
                .contains(&"branch-fix-1-report.json".to_string())
        );
        assert!(
            !files
                .reports
                .contains(&"branch-fix-1-report.json".to_string())
        );
        assert!(
            !files
                .reviews
                .contains(&"branch-fix-1-report.json".to_string())
        );
    }
}
