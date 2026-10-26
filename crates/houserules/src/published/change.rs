//! The kind of a changed path and the line ranges of its hunks: the forms
//! that the `evidence` and `change-record` documents name for the uncovered
//! surface of a change (spec 4.2 and 5.9). Neither type is a document of its
//! own, so no file under `schemas/` describes them alone.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What happened to one changed path. The change analysis gives each path
/// one kind (spec 4.2). Serialized, a kind is its lower-case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ChangeKind {
    /// Added or edited text: the path has hunks.
    Text,
    /// A deleted file that held lines.
    Deleted,
    /// A rename with no edit: the path has no hunk.
    Renamed,
    /// An empty file that was added or deleted.
    Empty,
    /// A binary file.
    Binary,
    /// A change of the file mode only.
    Mode,
    /// A symbolic link (mode `120000`).
    Symlink,
    /// A submodule (mode `160000`).
    Submodule,
}

/// The lines that one hunk of a changed path touches. Serialized, a range is
/// one of two objects: `{"start": s, "end": e}` or `{"after": n, "deleted":
/// k}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum LineRange {
    /// The lines `start` to `end` of the file, counted from 1. For a deleted
    /// file they are the lines of the old file.
    Lines {
        /// The first line.
        start: u64,
        /// The last line.
        end: u64,
    },
    /// A hunk that only deletes lines: `deleted` lines went away after line
    /// `after` of the new file. `after` is 0 when the deletion is at the top.
    DeletedAfter {
        /// The line of the new file after which the lines went away.
        after: u64,
        /// The number of lines that went away.
        deleted: u64,
    },
}

/// Whether an empty file was added or deleted. An empty file has no hunk and
/// no rename, so its kind alone does not tell the two apart. Serialized, it is
/// its lower-case name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EmptyChange {
    /// The range adds the empty file.
    Added,
    /// The range deletes the empty file.
    Deleted,
}

/// A changed path that no covering check read: the element of the
/// `uncovered.paths` list of the evidence document (spec 5.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct UncoveredPath {
    /// The path from the install root. A deleted path is its old path.
    pub(crate) path: String,
    /// The path before a rename, or `null`.
    pub(crate) old_path: Option<String>,
    /// What happened to the path.
    pub(crate) kind: ChangeKind,
    /// The lines that the hunks of the path touch, in the order of the diff.
    pub(crate) ranges: Vec<LineRange>,
    /// For the kind `empty` only: whether the range adds or deletes the file.
    /// The key is absent for every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "EmptyChange")]
    pub(crate) empty: Option<EmptyChange>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_serializes_in_lower_case() {
        for (kind, name) in [
            (ChangeKind::Text, "text"),
            (ChangeKind::Deleted, "deleted"),
            (ChangeKind::Renamed, "renamed"),
            (ChangeKind::Empty, "empty"),
            (ChangeKind::Binary, "binary"),
            (ChangeKind::Mode, "mode"),
            (ChangeKind::Symlink, "symlink"),
            (ChangeKind::Submodule, "submodule"),
        ] {
            let text = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(text, format!("\"{name}\""));
            let back: ChangeKind = serde_json::from_str(&text).expect("deserialize");
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn a_line_range_serializes_in_the_two_forms_of_the_evidence_document() {
        let lines = LineRange::Lines { start: 2, end: 5 };
        let after = LineRange::DeletedAfter {
            after: 4,
            deleted: 2,
        };

        assert_eq!(
            serde_json::to_string(&lines).expect("serialize"),
            r#"{"start":2,"end":5}"#
        );
        assert_eq!(
            serde_json::to_string(&after).expect("serialize"),
            r#"{"after":4,"deleted":2}"#
        );
        for range in [lines, after] {
            let text = serde_json::to_string(&range).expect("serialize");
            let back: LineRange = serde_json::from_str(&text).expect("deserialize");
            assert_eq!(back, range);
        }
    }

    #[test]
    fn an_uncovered_path_keeps_its_four_keys_in_order_with_null_for_no_old_path() {
        let path = UncoveredPath {
            path: "src/a.rs".to_string(),
            old_path: None,
            kind: ChangeKind::Text,
            ranges: vec![LineRange::Lines { start: 1, end: 1 }],
            empty: None,
        };
        let renamed = UncoveredPath {
            path: "src/b.rs".to_string(),
            old_path: Some("src/old.rs".to_string()),
            kind: ChangeKind::Renamed,
            ranges: Vec::new(),
            empty: None,
        };

        assert_eq!(
            serde_json::to_string(&path).expect("serialize"),
            r#"{"path":"src/a.rs","old_path":null,"kind":"text","ranges":[{"start":1,"end":1}]}"#
        );
        assert_eq!(
            serde_json::to_string(&renamed).expect("serialize"),
            r#"{"path":"src/b.rs","old_path":"src/old.rs","kind":"renamed","ranges":[]}"#
        );
        for value in [path, renamed] {
            let text = serde_json::to_string(&value).expect("serialize");
            let back: UncoveredPath = serde_json::from_str(&text).expect("deserialize");
            assert_eq!(back, value);
        }
    }

    #[test]
    fn an_empty_path_names_whether_the_file_was_added_or_deleted_in_a_fifth_key() {
        for (change, word) in [
            (EmptyChange::Added, "added"),
            (EmptyChange::Deleted, "deleted"),
        ] {
            let path = UncoveredPath {
                path: "src/blank.rs".to_string(),
                old_path: None,
                kind: ChangeKind::Empty,
                ranges: Vec::new(),
                empty: Some(change),
            };

            let text = serde_json::to_string(&path).expect("serialize");

            assert_eq!(
                text,
                format!(
                    r#"{{"path":"src/blank.rs","old_path":null,"kind":"empty","ranges":[],"empty":"{word}"}}"#
                )
            );
            let back: UncoveredPath = serde_json::from_str(&text).expect("deserialize");
            assert_eq!(back, path);
        }
    }

    #[test]
    fn a_path_of_another_kind_reads_back_with_no_empty_key() {
        let text = r#"{"path":"a","old_path":null,"kind":"text","ranges":[]}"#;

        let path: UncoveredPath = serde_json::from_str(text).expect("deserialize");

        assert_eq!(path.empty, None);
    }
}
