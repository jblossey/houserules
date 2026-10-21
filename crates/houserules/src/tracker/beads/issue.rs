//! One row of `bd list --json` and what its labels, status, and type say about
//! it (spec 3.5): the id, the status, the kind of record, the item type, the
//! batch, and the `see` list. [`Issue`] reads the labels the way every hosted
//! adapter does (`HostedIssue`) and keeps what Beads decides: the id shape, the
//! custom types that mark a record, the custom status `partial`, the close
//! reason, and the `related` dependencies.

use serde_json::{Value, json};

use crate::backlog::ItemStatus;
use crate::tracker::TrackerError;
use crate::tracker::hosted::{HostedIssue, Kind};

/// The custom status of an issue that is partly done.
pub(super) const PARTIAL: &str = "partial";

/// The status of a closed issue.
const CLOSED: &str = "closed";

/// The close reason that reads as `dropped`; any other reads as `done`.
pub(super) const DROPPED: &str = "dropped";

/// The dependency type that `see` maps to.
const RELATED: &str = "related";

/// `true` when `id` has the shape of a Beads id: an issue prefix of letters,
/// digits, `_`, and `-`, a hyphen, and a lowercase base-36 hash, then the
/// numbers of the child levels (`<prefix>-<hash>.1.2`). The prefix is the
/// store's, which this function does not know: an id of another prefix has the
/// shape, and the store answers that it holds no such issue. An argument for
/// the tool, such as `--help`, has no such shape.
pub(super) fn is_issue_id(id: &str) -> bool {
    let Some((prefix, rest)) = id.rsplit_once('-') else {
        return false;
    };
    let mut levels = rest.split('.');
    let hash = levels.next().unwrap_or_default();
    !prefix.is_empty()
        && !prefix.starts_with('-')
        && !prefix.ends_with('-')
        && prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        && !hash.is_empty()
        && hash
            .chars()
            .all(|c| c.is_ascii_digit() || c.is_ascii_lowercase())
        && levels.all(|level| !level.is_empty() && level.chars().all(|c| c.is_ascii_digit()))
}

/// One row of `bd list --json`, as far as the adapter reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Issue {
    /// The id of the issue, such as `hr-hff` or `hr-hff.1`.
    pub(super) id: String,
    /// The title.
    pub(super) title: String,
    /// The description: the prose and the houserules block.
    pub(super) description: String,
    /// The status of `bd`: a built-in one or a custom one.
    pub(super) status: String,
    /// The native or custom type of `bd`.
    pub(super) issue_type: String,
    /// Why the issue was closed, when it was closed with a reason.
    pub(super) close_reason: Option<String>,
    /// The labels, in the order that `bd` prints them.
    pub(super) labels: Vec<String>,
    /// The targets of the `related` dependencies of the issue, in id order.
    pub(super) related: Vec<String>,
}

impl Issue {
    /// Reads one row of `bd list --json`. A row with no id is a named fault.
    pub(super) fn from_row(row: &Value) -> Result<Self, TrackerError> {
        let id = row["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| TrackerError::Command("bd returned an issue with no id".to_string()))?
            .to_string();
        let text = |key: &str| row[key].as_str().unwrap_or_default().to_string();
        let labels = row["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let mut related: Vec<String> = row["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|edge| {
                edge["type"].as_str() == Some(RELATED)
                    && edge["issue_id"].as_str().is_none_or(|from| from == id)
            })
            .filter_map(|edge| edge["depends_on_id"].as_str())
            .map(str::to_string)
            .collect();
        related.sort();
        related.dedup();
        Ok(Self {
            title: text("title"),
            description: text("description"),
            status: text("status"),
            issue_type: text("issue_type"),
            close_reason: row["close_reason"].as_str().map(str::to_string),
            labels,
            related,
            id,
        })
    }
}

impl HostedIssue for Issue {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn text(&self) -> &str {
        &self.description
    }

    fn labels(&self) -> &[String] {
        &self.labels
    }

    fn closed(&self) -> bool {
        self.status == CLOSED
    }

    /// A closed issue is `done` unless its close reason is `dropped`; the
    /// custom status `partial` is `partial`; every other status (`open`,
    /// `in_progress`, `blocked`, `hooked`, `pinned`, `deferred`) is `open`
    /// (spec 3.5).
    fn item_status(&self) -> ItemStatus {
        if self.closed() {
            if self.close_reason.as_deref() == Some(DROPPED) {
                ItemStatus::Dropped
            } else {
                ItemStatus::Done
            }
        } else if self.status == PARTIAL {
            ItemStatus::Partial
        } else {
            ItemStatus::Open
        }
    }

    /// The custom type of spec 3.5 that marks a record: `houserules-amendment`,
    /// `houserules-parked`, `houserules-batch`, and, for a record that no
    /// command reads, `houserules-section` or `houserules-decision`.
    fn marked_kind(&self) -> Option<Kind> {
        match self.issue_type.as_str() {
            "houserules-amendment" => Some(Kind::Amendment),
            "houserules-parked" => Some(Kind::Parked),
            "houserules-batch" => Some(Kind::Batch),
            "houserules-section" | "houserules-decision" => Some(Kind::Other),
            _ => None,
        }
    }

    /// The value of the `milestone:<m>` label.
    fn milestone(&self) -> Option<&str> {
        self.label_values("milestone:").next()
    }

    /// The `related` dependencies, in id order.
    fn see(&self) -> Option<Value> {
        (!self.related.is_empty()).then(|| json!(self.related))
    }

    fn answers_to(&self, id: &str) -> bool {
        self.id == id
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::issue;
    use super::*;

    /// A Beads id is a prefix, a hyphen, a hash, and child levels; a prefix may
    /// hold hyphens, underscores, and capitals.
    #[test]
    fn an_id_is_a_prefix_a_hash_and_child_levels() {
        for id in [
            "hr-1",
            "hr-hff",
            "hr-10",
            "hr-2.1",
            "hr-2.1.14",
            "my-proj-c3c",
            "my-proj-c3c.1",
            "My_Proj-m4g",
            "bd_-x-3a7",
            "a-yz1",
            // The store of this prefix answers that it holds none.
            "WI-001",
        ] {
            assert!(is_issue_id(id), "{id}");
        }
        for id in [
            "",
            "-",
            "hr",
            "hr-",
            "-1",
            "--help",
            "--version",
            "hr--1",
            "-hr-1",
            "hr-1.",
            "hr-1..2",
            "hr-.1",
            "hr-A1",
            "hr-1.a",
            "hr 1",
            "hr-1 ",
            " hr-1",
            "hr-1;ls",
            "x/hr-1",
            "hr-1\n",
            "HR_ 1",
            "#12",
            "12",
            "process.tdd",
            "quality.no-compat-softening",
        ] {
            assert!(!is_issue_id(id), "{id:?}");
        }
    }

    /// Spec 3.5: the status of an issue from its status and its close reason.
    #[test]
    fn a_status_follows_the_status_the_close_reason_and_the_custom_status() {
        let mut open = issue("hr-1", "task", &[], "");
        for status in [
            "open",
            "in_progress",
            "blocked",
            "hooked",
            "pinned",
            "deferred",
            "something-new",
        ] {
            open.status = status.to_string();
            assert_eq!(open.item_status(), ItemStatus::Open, "{status}");
        }
        open.status = PARTIAL.to_string();
        assert_eq!(open.item_status(), ItemStatus::Partial);
        open.status = CLOSED.to_string();
        for (reason, status) in [
            (None, ItemStatus::Done),
            (Some("completed"), ItemStatus::Done),
            (Some("Closed"), ItemStatus::Done),
            (Some("Won't fix"), ItemStatus::Done),
            (Some("dropped"), ItemStatus::Dropped),
        ] {
            open.close_reason = reason.map(str::to_string);
            assert_eq!(open.item_status(), status, "{reason:?}");
        }
    }

    /// Spec 3.5: the milestone is the value of a `milestone:` label, the `see`
    /// list is the `related` dependencies, and an id names the issue that has
    /// it and no other.
    #[test]
    fn the_milestone_the_see_list_and_the_id_come_from_the_issue() {
        let mut item = issue("hr-2", "feature", &["type:feat", "milestone:v1"], "");
        assert_eq!(item.milestone(), Some("v1"));
        assert_eq!(item.see(), None);
        item.related = vec!["hr-3".to_string(), "hr-9".to_string()];
        assert_eq!(item.see(), Some(json!(["hr-3", "hr-9"])));
        assert!(item.answers_to("hr-2") && !item.answers_to("#2") && !item.answers_to("hr-20"));
        assert_eq!(item.id(), "hr-2");
        assert!(!item.is_pull_request());
    }

    /// Spec 3.5, C30: the type of an issue decides its kind; a parked issue is
    /// a group when its block holds `group: true`, and a parked item otherwise.
    #[test]
    fn the_type_and_the_block_decide_the_kind() {
        let kind = |issue_type: &str, labels: &[&str], description: &str| {
            issue("hr-1", issue_type, labels, description).kind()
        };
        assert_eq!(kind("houserules-amendment", &[], ""), Kind::Amendment);
        assert_eq!(kind("houserules-batch", &[], ""), Kind::Batch);
        assert_eq!(kind("houserules-section", &[], ""), Kind::Other);
        assert_eq!(kind("houserules-decision", &[], ""), Kind::Other);
        assert_eq!(kind("decision", &["type:decision"], ""), Kind::Item);
        assert_eq!(kind("task", &["bug"], ""), Kind::Other);
        assert_eq!(
            kind(
                "houserules-parked",
                &["batch:1"],
                "Intro.\n\n```houserules\ngroup: true\n```"
            ),
            Kind::Group
        );
        for description in [
            "Text.",
            "Text.\n\n```houserules\ngroup: false\n```",
            "Text.\n\n```houserules\ngroup: \"true\"\n```",
            "Text.\n\n```houserules\nresolved: {\"batch\":2,\"note\":\"n\"}\n```",
        ] {
            assert_eq!(
                kind("houserules-parked", &["batch:1"], description),
                Kind::Parked,
                "{description:?}"
            );
        }
        assert_eq!(
            kind(
                "feature",
                &["type:feat"],
                "Item.\n\n```houserules\ngroup: true\n```"
            ),
            Kind::Item,
            "the block of an issue that is no parked record is no group"
        );
        assert_eq!(
            kind("houserules-batch", &["type:feat"], ""),
            Kind::Batch,
            "a record type wins over a type label"
        );
    }

    /// Spec 3.5: a row reads its labels, its close reason, and the targets of its
    /// `related` dependencies in id order; a dependency of another type, and a
    /// dependency of another issue, are no `see`.
    #[test]
    fn a_row_reads_its_labels_its_close_reason_and_its_related_dependencies() {
        let row = json!({
            "id": "hr-7", "title": "T", "description": "D", "status": "closed",
            "issue_type": "feature", "close_reason": "dropped",
            "labels": ["type:feat", "batch:3"],
            "dependencies": [
                {"issue_id": "hr-7", "depends_on_id": "hr-9", "type": "related"},
                {"issue_id": "hr-7", "depends_on_id": "hr-3", "type": "related"},
                {"issue_id": "hr-7", "depends_on_id": "hr-3", "type": "related"},
                {"issue_id": "hr-7", "depends_on_id": "hr-4", "type": "blocks"},
                {"issue_id": "hr-7", "depends_on_id": "hr-5", "type": "discovered-from"},
                {"issue_id": "hr-8", "depends_on_id": "hr-1", "type": "related"},
                {"issue_id": "hr-7", "depends_on_id": "external:proj:cap", "type": "related"},
            ],
        });
        assert_eq!(
            Issue::from_row(&row).expect("a row"),
            Issue {
                id: "hr-7".to_string(),
                title: "T".to_string(),
                description: "D".to_string(),
                status: "closed".to_string(),
                issue_type: "feature".to_string(),
                close_reason: Some("dropped".to_string()),
                labels: vec!["type:feat".to_string(), "batch:3".to_string()],
                related: vec![
                    "external:proj:cap".to_string(),
                    "hr-3".to_string(),
                    "hr-9".to_string()
                ],
            }
        );
        let plain = Issue::from_row(&json!({"id": "hr-8"})).expect("a row");
        assert_eq!(
            (plain.title.as_str(), plain.close_reason, plain.labels),
            ("", None, Vec::<String>::new())
        );
        assert_eq!(
            Issue::from_row(&json!({"title": "no id"})),
            Err(TrackerError::Command(
                "bd returned an issue with no id".to_string()
            ))
        );
    }
}
