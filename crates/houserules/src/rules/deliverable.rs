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

/// The task-id segment shared by every deliverable filename pattern below,
/// declared once as `.claude/schemas/deliverables.json`'s `$defs.taskId`
/// pattern (both copies) -- `task_id_shape_is_declared_identically_in_both_
/// schema_copies` pins the two to this literal. Digits, with an optional
/// trailing lowercase letter for a split task (`1`, `3a`).
const TASK_ID_SHAPE: &str = r"\d+[a-z]?";

/// The `task-<id>-audit*.json` filename pattern, with `<id>` built from
/// `TASK_ID_SHAPE`.
fn audit_pattern() -> String {
    format!(r"^task-{TASK_ID_SHAPE}-audit.*\.json$")
}

/// The `task-<id>-report.json` filename pattern, with `<id>` built from
/// `TASK_ID_SHAPE`. A report never carries a round suffix, unlike an audit
/// or a review, so nothing follows the marker word.
fn report_pattern() -> String {
    format!(r"^task-{TASK_ID_SHAPE}-report\.json$")
}

/// The `task-<id>-review*.json` filename pattern, with `<id>` built from
/// `TASK_ID_SHAPE`.
fn review_pattern() -> String {
    format!(r"^task-{TASK_ID_SHAPE}-review.*\.json$")
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
        .filter(|name| matches(name, &audit_pattern()))
        .cloned()
        .collect();
    let mut reports: Vec<String> = names
        .iter()
        .filter(|name| matches(name, &report_pattern()))
        .cloned()
        .collect();
    let mut reviews: Vec<String> = names
        .iter()
        .filter(|name| matches(name, &review_pattern()))
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
    use std::path::PathBuf;

    use serde_json::Value;

    use super::*;

    /// The repo-root and template copies of the deliverables schema, each
    /// paired with its repository-relative literal --
    /// `houserules.template-is-the-source` keeps both in lockstep, and a
    /// failure message names the file the way this repository names it,
    /// not the `../..`-joined `PathBuf` a maintainer would have to read
    /// backwards (`houserules.path-pins-mirror-the-code`).
    fn schema_paths() -> [(&'static str, PathBuf); 2] {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        [
            (
                ".claude/schemas/deliverables.json",
                crate_root.join("../../.claude/schemas/deliverables.json"),
            ),
            (
                "template/.claude/schemas/deliverables.json",
                crate_root.join("../../template/.claude/schemas/deliverables.json"),
            ),
        ]
    }

    /// Pins `TASK_ID_SHAPE` to `.claude/schemas/deliverables.json`'s
    /// declared `$defs.taskId.pattern` in both copies: a schema edit that
    /// drifts from the Rust constant fails here.
    #[test]
    fn task_id_shape_is_declared_identically_in_both_schema_copies() {
        let expected = format!("^{TASK_ID_SHAPE}$");
        for (label, path) in schema_paths() {
            let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{label}: {error}"));
            let schema: Value =
                serde_json::from_str(&text).unwrap_or_else(|error| panic!("{label}: {error}"));
            let pattern = schema["$defs"]["taskId"]["pattern"]
                .as_str()
                .unwrap_or_else(|| panic!("{label}: $defs.taskId.pattern missing"));
            assert_eq!(pattern, expected, "{label}");
        }
    }

    /// Every one of the three filename patterns must accept a bare digit
    /// and a digit+letter split id (`1`, `3a`) and reject a two-letter
    /// id, a non-digit id, an embedded dash, and a bare word (`3ab`,
    /// `abc`, `3-a`, `round2`). To reproduce a divergence, temporarily
    /// hardcode a narrower id shape in one of `audit_pattern`/
    /// `report_pattern`/`review_pattern`: exactly that pattern's rows
    /// turn red, then revert it.
    #[test]
    fn filename_patterns_agree_on_the_declared_task_id_shape() {
        let accepted_ids = ["1", "12", "3a", "10b"];
        let rejected_ids = ["3ab", "abc", "3-a", "round2"];
        let patterns = [
            ("audit", audit_pattern()),
            ("report", report_pattern()),
            ("review", review_pattern()),
        ];
        for (marker, pattern) in patterns {
            for id in accepted_ids {
                let name = format!("task-{id}-{marker}.json");
                assert!(matches(&name, &pattern), "{marker}: {name} should match");
            }
            for id in rejected_ids {
                let name = format!("task-{id}-{marker}.json");
                assert!(
                    !matches(&name, &pattern),
                    "{marker}: {name} should not match"
                );
            }
        }
    }

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
    /// branch-decoy-rejecting edges of its patterns. Three retained,
    /// ad-hoc evidence names that predate and fall outside the declared
    /// task-id shape (`task-1-final-full-task-audit.json`,
    /// `task-3-live-audit-js.json`, `task-1-fixround1-fixdiff-audit.json`)
    /// are asserted OUT of `audits`: they name real, retained evidence
    /// files, deliberately excluded from the shape and never renamed.
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
            "task-1-final-full-task-audit.json",
            "task-3-live-audit-js.json",
            "task-1-fixround1-fixdiff-audit.json",
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
        for name in [
            "task-1-final-full-task-audit.json",
            "task-3-live-audit-js.json",
            "task-1-fixround1-fixdiff-audit.json",
        ] {
            assert!(
                !files.audits.contains(&name.to_string()),
                "{name} predates the declared task-id shape and must stay out of audits"
            );
        }
    }
}
