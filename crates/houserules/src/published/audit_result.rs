//! The `audit-result` document: the output of `houserules audit`, on stdout
//! and in the file of `--json`.
//!
//! The type is its own, apart from the deliverables types, because a judged
//! row holds the result `open`, which the deliverables schema does not allow
//! in a report. The order of the fields is the order of the keys that `audit`
//! prints. A key that `audit` leaves out is an `Option` that the serializer
//! skips when it is `None`; its `schemars(with)` names the type of the key
//! when present, so that the schema does not allow `null` where the document
//! never prints one.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Published;

/// The result of `houserules audit` for a range of commits: the rules of the
/// package with one row each, the counts of the rows, and the verdict on the
/// work-item references of the range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(title = "Audit result")]
pub(crate) struct AuditResult {
    /// The schema id, `houserules/audit-result@1.0.0`.
    pub(crate) schema: String,
    /// The short sha of the commit that `--base` names.
    pub(crate) base: String,
    /// The short sha of the commit that `--head` names, `HEAD` by default.
    pub(crate) head: String,
    /// The ids that `--ids` adds to the package, in the order given.
    pub(crate) ids: Vec<String>,
    /// The files that the range adds, copies, modifies, or renames, as paths
    /// from the install root, in the order of the diff.
    pub(crate) changed_files: Vec<String>,
    /// The areas that the changed files touch, sorted.
    pub(crate) areas: Vec<String>,
    /// The changed files of each area. `global` comes first; every other area
    /// follows in the order in which a changed file first matches it.
    pub(crate) area_files: IndexMap<String, Vec<String>>,
    /// One row for each rule of the package, sorted by id.
    pub(crate) rules: Vec<AuditRow>,
    /// The counts of the rows.
    pub(crate) summary: Summary,
    /// One line for each `--sanctioned` booking on a deterministic row that
    /// did not fail. Absent when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<String>")]
    pub(crate) stale_sanctions: Option<Vec<String>>,
    /// The verdict on the work-item references of the range.
    pub(crate) references: References,
}

impl Published for AuditResult {
    const NAME: &'static str = "audit-result";
    const VERSION: &'static str = "1.0.0";
}

/// The row of one rule in an audit result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct AuditRow {
    /// The id of the knowledge entry.
    pub(crate) id: String,
    /// The kind of the entry.
    pub(crate) kind: String,
    /// Whether the audit ran a check for the rule or leaves the rule to a
    /// reviewer.
    pub(crate) mode: RowMode,
    /// The level of the check, or `null` for a judged row.
    pub(crate) level: Option<RowLevel>,
    /// What the check found, or `open` for a judged row.
    pub(crate) result: RowResult,
    /// The line that names what the check found, or `—` for a judged row.
    pub(crate) evidence: String,
    /// The results of a `runner` check with a SARIF tool that count, in the
    /// order of the calls. Only the row of such a check holds the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<Finding>")]
    pub(crate) findings: Option<Vec<Finding>>,
    /// The reference of the `--sanctioned` booking on the row. Only a judged
    /// row and a failed deterministic row hold the key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub(crate) sanctioned: Option<String>,
}

/// Whether the audit ran a check for a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RowMode {
    /// The rule has a check, and the audit ran it.
    Deterministic,
    /// The rule has no check; a reviewer judges it.
    Judged,
}

/// The level of a check: what a violation of the check counts as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RowLevel {
    /// A violation fails the audit.
    Fail,
    /// A violation warns and fails nothing.
    Warn,
}

impl RowLevel {
    /// The result of a violated check of this level: `fail` for a `fail`
    /// check and `warn` for a `warn` check.
    pub(crate) fn violation(self) -> RowResult {
        match self {
            RowLevel::Fail => RowResult::Fail,
            RowLevel::Warn => RowResult::Warn,
        }
    }
}

/// The result of one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RowResult {
    /// The check holds.
    Pass,
    /// A check of level `fail` is violated.
    Fail,
    /// A check of level `warn` is violated.
    Warn,
    /// The check did not run.
    Skipped,
    /// A judged row: a reviewer decides.
    Open,
}

impl RowResult {
    /// The word that the document prints for the result.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RowResult::Pass => "pass",
            RowResult::Fail => "fail",
            RowResult::Warn => "warn",
            RowResult::Skipped => "skipped",
            RowResult::Open => "open",
        }
    }
}

/// One result of a `runner` check that counts, as the row lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Finding {
    /// The file, relative to the install root with `/` separators. A location
    /// outside the root keeps the fully resolved URI of its base when it has
    /// one, else the URI as the tool wrote it. `null` for a result with no
    /// location.
    pub(crate) path: Option<String>,
    /// The line in `path`; `null` with no path or no region.
    pub(crate) line: Option<u64>,
    /// The id of the rule, or `(no rule id)`.
    pub(crate) rule: String,
    /// The text of the message, or `(no message)`.
    pub(crate) message: String,
}

/// The counts of the rows of an audit result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Summary {
    /// The short sha of the commit that `--base` names.
    pub(crate) base: String,
    /// The short sha of the commit that `--head` names.
    pub(crate) head: String,
    /// The number of rows with the mode `deterministic`.
    pub(crate) deterministic: u64,
    /// The number of deterministic rows with the result `pass`.
    pub(crate) pass: u64,
    /// The number of deterministic rows with the result `fail`.
    pub(crate) fail: u64,
    /// The number of deterministic rows with the result `warn`.
    pub(crate) warn: u64,
    /// The number of deterministic rows with the result `skipped`.
    pub(crate) skipped: u64,
    /// The number of rows with the mode `judged`.
    pub(crate) judged: u64,
    /// `true` when the range holds no commit. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "bool")]
    pub(crate) empty_range: Option<bool>,
    /// The number of failed rows that hold `sanctioned`. Absent when there is
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub(crate) sanctioned_fail: Option<u64>,
    /// The number of lines in `references.failures`.
    pub(crate) reference_fail: u64,
}

/// What the audit decided about the work-item references of the range: the
/// `Work-Item:` and `Deferred:` trailers of its commits and the backlog ids of
/// the deliverables it read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct References {
    /// The number of references that the tracker judged, passed or failed.
    pub(crate) checked: u64,
    /// The number of references among those checked that passed.
    pub(crate) passed: u64,
    /// The number of references that the audit did not judge: all of them when
    /// the install has no tracker, when the tracker cannot be reached, or when
    /// the tracker answered with an error.
    pub(crate) skipped: u64,
    /// One line for each failure, in the order that the audit prints them on
    /// stderr.
    pub(crate) failures: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A result that holds every optional key: `stale_sanctions` at the top,
    /// `empty_range` and `sanctioned_fail` in the summary, and `findings` and
    /// `sanctioned` in rows. The rows hold one value of each `mode`, `level`,
    /// and `result`.
    const FULL: &str = r#"{
  "schema": "houserules/audit-result@1.0.0",
  "base": "1111111",
  "head": "2222222",
  "ids": [
    "process.tdd"
  ],
  "changed_files": [
    "src/a.rs"
  ],
  "areas": [
    "global",
    "rust"
  ],
  "area_files": {
    "global": [],
    "rust": [
      "src/a.rs"
    ]
  },
  "rules": [
    {
      "id": "process.tdd",
      "kind": "rule",
      "mode": "judged",
      "level": null,
      "result": "open",
      "evidence": "—",
      "sanctioned": "interim"
    },
    {
      "id": "team.commits",
      "kind": "rule",
      "mode": "deterministic",
      "level": "fail",
      "result": "pass",
      "evidence": "1 commits checked"
    },
    {
      "id": "team.lint",
      "kind": "rule",
      "mode": "deterministic",
      "level": "fail",
      "result": "fail",
      "evidence": "src/a.rs:1 no-todo: remove the todo (sanctioned: interim)",
      "findings": [
        {
          "path": "src/a.rs",
          "line": 1,
          "rule": "no-todo",
          "message": "remove the todo"
        },
        {
          "path": null,
          "line": null,
          "rule": "(no rule id)",
          "message": "(no message)"
        }
      ],
      "sanctioned": "interim"
    },
    {
      "id": "team.style",
      "kind": "rule",
      "mode": "deterministic",
      "level": "warn",
      "result": "warn",
      "evidence": "src/a.rs:2 matches FORBIDDEN"
    },
    {
      "id": "team.report",
      "kind": "rule",
      "mode": "deterministic",
      "level": "fail",
      "result": "skipped",
      "evidence": "no --report given"
    }
  ],
  "summary": {
    "base": "1111111",
    "head": "2222222",
    "deterministic": 4,
    "pass": 1,
    "fail": 1,
    "warn": 1,
    "skipped": 1,
    "judged": 1,
    "empty_range": true,
    "sanctioned_fail": 1,
    "reference_fail": 1
  },
  "stale_sanctions": [
    "--sanctioned team.commits=interim: row result is \"pass\", not fail"
  ],
  "references": {
    "checked": 2,
    "passed": 1,
    "skipped": 0,
    "failures": [
      "HR-1: no such item"
    ]
  }
}"#;

    /// A result that holds none of the optional keys.
    const MINIMAL: &str = r#"{
  "schema": "houserules/audit-result@1.0.0",
  "base": "1111111",
  "head": "2222222",
  "ids": [],
  "changed_files": [],
  "areas": [
    "global"
  ],
  "area_files": {
    "global": []
  },
  "rules": [
    {
      "id": "team.commits",
      "kind": "rule",
      "mode": "deterministic",
      "level": "fail",
      "result": "pass",
      "evidence": "0 commits checked"
    }
  ],
  "summary": {
    "base": "1111111",
    "head": "2222222",
    "deterministic": 1,
    "pass": 1,
    "fail": 0,
    "warn": 0,
    "skipped": 0,
    "judged": 0,
    "reference_fail": 0
  },
  "references": {
    "checked": 0,
    "passed": 0,
    "skipped": 0,
    "failures": []
  }
}"#;

    /// Reads `text` as an audit result and writes it back in the form of
    /// `emit`.
    fn round_trip(text: &str) -> String {
        let result: AuditResult = serde_json::from_str(text).expect("an audit result");
        serde_json::to_string_pretty(&result).expect("an audit result serializes")
    }

    #[test]
    fn a_result_read_back_serializes_to_the_same_bytes_with_every_optional_key() {
        assert_eq!(round_trip(FULL), FULL);
    }

    #[test]
    fn a_result_read_back_keeps_every_optional_key_absent() {
        assert_eq!(round_trip(MINIMAL), MINIMAL);
    }

    #[test]
    fn a_result_read_back_keeps_an_empty_stale_sanctions_list() {
        let with_empty_list = MINIMAL.replace(
            "  \"references\": {",
            "  \"stale_sanctions\": [],\n  \"references\": {",
        );
        assert_ne!(with_empty_list, MINIMAL, "the replacement found its anchor");
        assert_eq!(round_trip(&with_empty_list), with_empty_list);
    }

    #[test]
    fn a_result_names_itself_with_the_word_that_the_document_prints() {
        for result in [
            RowResult::Pass,
            RowResult::Fail,
            RowResult::Warn,
            RowResult::Skipped,
            RowResult::Open,
        ] {
            assert_eq!(
                serde_json::to_value(result).expect("a result serializes"),
                serde_json::json!(result.as_str()),
            );
        }
    }

    #[test]
    fn a_booked_row_keeps_its_sanctioned_key_and_a_row_without_one_gains_none() {
        let result: AuditResult = serde_json::from_str(FULL).expect("an audit result");
        let booked: Vec<(&str, Option<&str>)> = result
            .rules
            .iter()
            .map(|row| (row.id.as_str(), row.sanctioned.as_deref()))
            .collect();
        assert_eq!(
            booked,
            [
                ("process.tdd", Some("interim")),
                ("team.commits", None),
                ("team.lint", Some("interim")),
                ("team.style", None),
                ("team.report", None),
            ]
        );
    }
}
