//! Reads a JSON deliverable file leniently (never through a typed schema
//! model) and lists a workspace directory's deliverables by kind, shared
//! by `stats.rs` and `audit.rs`.
//!
//! Both callers aggregate across a workspace of agent-authored JSON
//! files (`task-*-audit*.json`, `task-*-report.json`,
//! `task-*-review*.json`, `spec-review.json`, `spec-review-r<n>.json`,
//! `branch-review.json`; the constants below list them once) that need not be
//! schema-valid at read time --
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
/// (`task-*-report.json`), `reviews` (`task-*-review*.json`), and
/// `spec_reviews` (`spec-review.json` and `spec-review-r<n>.json`).
pub(super) struct WorkspaceFiles {
    pub audits: Vec<String>,
    pub reports: Vec<String>,
    pub reviews: Vec<String>,
    pub spec_reviews: Vec<String>,
}

/// The task-id segment shared by every task deliverable filename pattern
/// below, declared once as `.claude/schemas/deliverables.json`'s `$defs.taskId`
/// pattern (both copies) -- `task_id_shape_is_declared_identically_in_both_
/// schema_copies` pins the two to this literal. Digits, with an optional
/// trailing lowercase letter for a split task (`1`, `3a`).
const TASK_ID_SHAPE: &str = r"\d+[a-z]?";

/// One kind of deliverable file in a workspace: its name as a person writes
/// it, and the pattern that matches it. `STATS_READS` lists every kind, and
/// both the classification of a filename and every message that names the
/// files read it, so the two cannot disagree.
pub(super) struct DeliverableFile {
    /// The name as a person writes it; `*` stands for any part that varies,
    /// and `<n>` for a round number. A name with neither is the exact
    /// filename.
    pub glob: &'static str,
    /// The anchored pattern that matches the name, built on demand because the
    /// task kinds embed `TASK_ID_SHAPE`.
    pattern: fn() -> String,
}

impl DeliverableFile {
    /// `true` when `name` is a file of this kind.
    fn names(&self, name: &str) -> bool {
        matches(name, &(self.pattern)())
    }
}

/// `task-<id>-audit*.json`, with `<id>` built from `TASK_ID_SHAPE`.
pub(super) const AUDIT: DeliverableFile = DeliverableFile {
    glob: "task-*-audit*.json",
    pattern: audit_pattern,
};

/// `task-<id>-review*.json`, with `<id>` built from `TASK_ID_SHAPE`.
pub(super) const REVIEW: DeliverableFile = DeliverableFile {
    glob: "task-*-review*.json",
    pattern: review_pattern,
};

/// `task-<id>-report.json`, with `<id>` built from `TASK_ID_SHAPE`. A report
/// never carries a round suffix, unlike an audit or a review, so nothing
/// follows the marker word.
pub(super) const REPORT: DeliverableFile = DeliverableFile {
    glob: "task-*-report.json",
    pattern: report_pattern,
};

/// `spec-review.json`: the first design review of a spec, which has no task
/// id. A probe capture named after a spec review (`spec-review-probe-*.json`)
/// is evidence, not a deliverable.
pub(super) const SPEC_REVIEW: DeliverableFile = DeliverableFile {
    glob: "spec-review.json",
    pattern: spec_review_pattern,
};

/// `spec-review-r<n>.json`: the design re-review of round `<n>`, with `<n>`
/// digits only, so a probe capture such as `spec-review-r2-probe-list.json`
/// is no deliverable.
pub(super) const SPEC_REVIEW_ROUND: DeliverableFile = DeliverableFile {
    glob: "spec-review-r<n>.json",
    pattern: spec_review_round_pattern,
};

/// The exact name of the deliverable of a whole branch. `workspace_files`
/// does not list it: it carries no task id, and its name is fixed.
pub(super) const BRANCH_REVIEW_FILE: &str = "branch-review.json";

/// The deliverable of a whole branch, named `BRANCH_REVIEW_FILE`.
pub(super) const BRANCH_REVIEW: DeliverableFile = DeliverableFile {
    glob: BRANCH_REVIEW_FILE,
    pattern: branch_review_pattern,
};

/// Every deliverable file that `stats` reads from a workspace, in the order
/// its messages name them.
pub(super) const STATS_READS: [&DeliverableFile; 6] = [
    &AUDIT,
    &REVIEW,
    &REPORT,
    &SPEC_REVIEW,
    &SPEC_REVIEW_ROUND,
    &BRANCH_REVIEW,
];

/// The globs of `STATS_READS` as one phrase for a message: `a, b, and c`.
pub(super) fn stats_reads_phrase() -> String {
    let globs: Vec<&str> = STATS_READS.iter().map(|file| file.glob).collect();
    match globs.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{}, and {last}", rest.join(", ")),
        Some((only, _)) => (*only).to_string(),
        None => String::new(),
    }
}

/// The pattern of `AUDIT`.
fn audit_pattern() -> String {
    format!(r"^task-{TASK_ID_SHAPE}-audit.*\.json$")
}

/// The pattern of `REPORT`.
fn report_pattern() -> String {
    format!(r"^task-{TASK_ID_SHAPE}-report\.json$")
}

/// The pattern of `REVIEW`.
fn review_pattern() -> String {
    format!(r"^task-{TASK_ID_SHAPE}-review.*\.json$")
}

/// The pattern of `SPEC_REVIEW`.
fn spec_review_pattern() -> String {
    r"^spec-review\.json$".to_string()
}

/// The pattern of `SPEC_REVIEW_ROUND`.
fn spec_review_round_pattern() -> String {
    r"^spec-review-r[0-9]+\.json$".to_string()
}

/// The pattern of `BRANCH_REVIEW`.
fn branch_review_pattern() -> String {
    r"^branch-review\.json$".to_string()
}

/// `true` when `name` matches `pattern` anywhere -- every pattern this
/// module compiles is anchored (`^...$`), so this is a whole-string
/// match. Uses `regress` (already this crate's ECMAScript-regex engine,
/// `check.rs`'s `validate`/`regex_validity_message`) rather than
/// hand-rolling the static patterns above, so a future pattern
/// change gets the same regex semantics everywhere in this crate.
fn matches(name: &str, pattern: &str) -> bool {
    Regex::new(pattern)
        .unwrap_or_else(|error| panic!("{pattern:?} is a static, known-valid pattern: {error}"))
        .find(name)
        .is_some()
}

/// The sorted members of `names` that are files of any of `files`.
fn names_of_kinds(names: &[String], files: &[&DeliverableFile]) -> Vec<String> {
    let mut found: Vec<String> = names
        .iter()
        .filter(|name| files.iter().any(|file| file.names(name)))
        .cloned()
        .collect();
    found.sort();
    found
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
    Ok(WorkspaceFiles {
        audits: names_of_kinds(&names, &[&AUDIT]),
        reports: names_of_kinds(&names, &[&REPORT]),
        reviews: names_of_kinds(&names, &[&REVIEW]),
        spec_reviews: names_of_kinds(&names, &[&SPEC_REVIEW, &SPEC_REVIEW_ROUND]),
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

    /// Every listed kind matches the names its own glob stands for: the task
    /// kinds with a task id, and with and without a round suffix after the
    /// marker word, and a round number for `<n>`. The glob and the pattern
    /// come from the one entry, so a kind whose pattern drifts from its glob
    /// fails here.
    #[test]
    fn every_listed_file_matches_the_names_its_glob_stands_for() {
        for file in STATS_READS {
            for tail in ["", "-r1"] {
                let named = file
                    .glob
                    .replacen("task-*", "task-1", 1)
                    .replace("<n>", "1");
                let sample = match named.split_once('*') {
                    Some((head, rest)) => format!("{head}{tail}{rest}"),
                    None if tail.is_empty() => named,
                    None => continue,
                };
                assert!(file.names(&sample), "{} should match {sample}", file.glob);
            }
        }
    }

    /// The phrase of the globs keeps the order of `STATS_READS` and joins the
    /// last two with `, and`.
    #[test]
    fn stats_reads_phrase_names_every_listed_glob_in_order() {
        assert_eq!(
            stats_reads_phrase(),
            "task-*-audit*.json, task-*-review*.json, task-*-report.json, \
             spec-review.json, spec-review-r<n>.json, and branch-review.json"
        );
    }

    /// The workspace fixture mixes `task-1-audit.json`,
    /// `task-2-audit-r1.json`, `task-1-report.json`, a lettered
    /// `task-3a-report.json` (plus a decoy `task-1-report.md`),
    /// `task-2-review.json`, the spec reviews `spec-review.json` and
    /// `spec-review-r1.json` (plus the decoys `spec-review.txt`,
    /// `review-spec.json`, `spec-review-r.json`, and the probe captures
    /// `spec-review-probe-x.json` and `spec-review-r2-probe-y.json`), an
    /// unrelated file, and a branch-level
    /// deliverable `branch-fix-1-report.json` that must land in none of
    /// the four lists -- this pins `workspace_files`' own
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
            "spec-review.json",
            "spec-review-r1.json",
            "spec-review.txt",
            "review-spec.json",
            "spec-review-r.json",
            "spec-review-probe-x.json",
            "spec-review-r2-probe-y.json",
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
        assert_eq!(
            files.spec_reviews,
            vec!["spec-review-r1.json", "spec-review.json"]
        );
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
