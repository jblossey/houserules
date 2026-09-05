//! Reads a JSON deliverable file leniently (never through a typed schema
//! model) and lists a workspace directory's deliverables by kind --
//! `tools/kb.mjs`'s `readDeliverable` and `workspaceFiles`, shared by
//! `stats.rs` and `audit.rs` (batch 17 T3, docs/specs/
//! 2026-09-04-batch-15-tier2-spec.md §5 phase 2).
//!
//! Both callers aggregate across a workspace of agent-authored JSON files
//! (`task-*-audit*.json`, `task-<n>-report.json`, `task-*-review*.json`)
//! that need not be schema-valid at read time -- `stats`' own `?? []`
//! fallbacks tolerate a report missing `knowledge_used`, a review missing
//! `rule_adherence`, or an audit file missing `ids`/`rules` entirely
//! (`tests/kb.test.mjs`'s "tolerates ..." cases, ported to
//! `stats.rs`/`audit.rs`'s own tests). A typed, `deny_unknown_fields`
//! parse of, say, the full `TaskReport` shape would fail every one of
//! those cases outright, diverging from the frozen JS the moment any
//! *other* field the aggregation never reads is missing or malformed --
//! this is the data-layer rule's own worked example (spec §3: "a malformed
//! report in a workspace: what does the JS audit/stats DO with it? match
//! that"). So both readers stay on raw `serde_json::Value`, exactly as the
//! frozen JS does for *absence*.
//!
//! "Exactly as the frozen JS does" stops at absence, though (fix round 1,
//! issue 7, task-3-review.json): the JS's `data.rules ?? []` substitutes
//! an empty array only for `null`/`undefined`; a *present* `rules` of the
//! wrong type (an object, say) reaches `for (const rule of data.rules)`
//! and throws `TypeError: ... is not iterable` -- a crash, not a silent
//! empty result. `array_field` below distinguishes the two states so a
//! caller can match the frozen JS's real behavior: empty for absence,
//! a named finding (spec §6's crash-path ruling) for a wrongly-typed
//! present value, never silence for the second state.

use std::fs;
use std::path::Path;

use regress::Regex;
use serde_json::Value;

/// Reads `field` from `data` as a JSON array, `Ok(&[])` for a missing key
/// or an explicit `null` (`tools/kb.mjs`'s own `?? []` fallback), or a
/// named error for a present value of any other type -- see the module
/// doc for why this, not a blanket `.and_then(Value::as_array)` that
/// silently treats "wrongly typed" the same as "absent", is the correct
/// port of the frozen JS's own behavior on this data.
pub(super) fn array_field<'a>(data: &'a Value, field: &str) -> Result<Vec<&'a Value>, String> {
    match data.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => Ok(items.iter().collect()),
        Some(_) => Err(format!("{field} is not an array")),
    }
}

/// Reads `path` and parses it as JSON, naming the file in either failure --
/// `tools/kb.mjs`'s `readDeliverable` (itself `template/tools/lib/
/// json-store.mjs`'s `readJson`, wrapped so both a missing file and
/// invalid JSON report the same way). The frozen JS distinguishes a
/// `UsageError` (invalid JSON) from a plain `Error` (unreadable file) only
/// so `main`'s generic `UsageError` catch can choose between "print one
/// line, exit 2" and "let a real bug propagate as a stack trace" -- this
/// binary's own CLI-failure-path convention (docs/specs/
/// 2026-09-04-batch-15-tier2-spec.md §6) already turns *both* into one
/// named stderr line and exit 2 (`validate_deliverable.rs`/`audit.rs`/
/// `stats.rs`'s own `cmd_*` wrappers), so this function needs only the one
/// `Result` arm, not the JS's two exception types.
pub(super) fn read_deliverable_value(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{}: invalid JSON ({error})", path.display()))
}

/// One workspace directory's deliverable filenames by kind, each sorted --
/// `tools/kb.mjs`'s `workspaceFiles`: `audits` (`task-*-audit*.json`),
/// `reports` (`task-<n>-report.json`), and `reviews` (`task-*-review*.json`).
pub(super) struct WorkspaceFiles {
    pub audits: Vec<String>,
    pub reports: Vec<String>,
    pub reviews: Vec<String>,
}

/// `true` when `name` matches `pattern` anywhere -- every pattern this
/// module compiles is anchored (`^...$`), so this is a whole-string match.
/// Uses `regress` (already this crate's ECMAScript-regex engine, `check.rs`'s
/// `validate`/`regex_validity_message`) rather than hand-rolling the three
/// static patterns below, so the `\d`/`.+`/`.` semantics stay exactly the
/// frozen JS's own `RegExp` behavior.
fn matches(name: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .unwrap_or_else(|error| panic!("{pattern:?} is a static, known-valid pattern: {error}"))
        .find(name)
        .is_some()
}

/// Lists `dir`'s deliverable filenames by kind (see `WorkspaceFiles`), or a
/// named error if `dir` cannot be read -- `workspaceFiles`'s own
/// `readdirSync` failure arm (`error.message`, wrapped as a `UsageError` on
/// the JS side; see `read_deliverable_value`'s doc for why this binary
/// collapses that distinction).
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
        .filter(|name| matches(name, r"^task-\d+-report\.json$"))
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

    /// tests/kb.test.mjs, describe('stats'): the workspace fixture mixes
    /// `task-1-audit.json`, `task-2-audit-r1.json`, `task-1-report.json`
    /// (plus a decoy `task-1-report.md`), and `task-2-review.json` -- this
    /// pins `workspace_files`' own classification directly.
    #[test]
    fn classifies_audits_reports_and_reviews_ignoring_unrelated_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in [
            "task-1-audit.json",
            "task-2-audit-r1.json",
            "task-1-report.json",
            "task-1-report.md",
            "task-2-review.json",
            "unrelated.txt",
        ] {
            fs::write(dir.path().join(name), "{}").expect("write fixture");
        }
        let files = workspace_files(dir.path()).expect("list workspace files");
        assert_eq!(
            files.audits,
            vec!["task-1-audit.json", "task-2-audit-r1.json"]
        );
        assert_eq!(files.reports, vec!["task-1-report.json"]);
        assert_eq!(files.reviews, vec!["task-2-review.json"]);
    }
}
