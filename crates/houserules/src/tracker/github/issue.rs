//! One row of the issues endpoint and what its labels say about it (spec
//! 3.5): the number and the id, the status, the kind of record, the item
//! type, and the batch. [`Issue`] reads the labels the way every hosted adapter
//! does (`HostedIssue`) and keeps what GitHub decides: the number, the
//! `houserules:<kind>` marker labels, the `status:partial` label, the
//! milestone, and the pull-request key.

use serde_json::Value;

use crate::backlog::ItemStatus;
use crate::tracker::hosted::{HostedIssue, Kind};
use crate::tracker::{TrackerError, block};

/// The label of an issue that is partly done.
pub(super) const PARTIAL: &str = "status:partial";

/// The reason that an id which names a pull request is no work item.
pub(super) const PULL_REQUEST: &str = "is a pull request, not an issue";

/// The number of `id`: `#<n>` or the bare `<n>`, with no leading zero.
pub(super) fn number_of(id: &str) -> Option<u64> {
    let digits = id.strip_prefix('#').unwrap_or(id);
    if digits.is_empty() || digits.starts_with('0') || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// The id of the issue `number`.
pub(super) fn id_of(number: u64) -> String {
    format!("#{number}")
}

/// One row of the issues endpoint, as far as the adapter reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Issue {
    /// The number of the issue in its repository.
    pub(super) number: u64,
    /// The title.
    pub(super) title: String,
    /// The body: the prose and the houserules block.
    pub(super) body: String,
    /// Whether the issue is closed.
    pub(super) closed: bool,
    /// Why the issue was closed (`completed`, `not_planned`, and so on).
    pub(super) close_reason: Option<String>,
    /// The names of the labels, in the order that GitHub prints them.
    pub(super) labels: Vec<String>,
    /// The title of the milestone.
    pub(super) milestone: Option<String>,
    /// Whether the row is a pull request, which the issues endpoint lists too.
    pub(super) pull_request: bool,
}

impl Issue {
    /// Reads one row of the issues endpoint. A row with no number is a named
    /// fault.
    pub(super) fn from_row(row: &Value) -> Result<Self, TrackerError> {
        let number = row["number"].as_u64().ok_or_else(|| {
            TrackerError::Command("gh returned an issue with no number".to_string())
        })?;
        let text = |key: &str| row[key].as_str().unwrap_or_default().to_string();
        let labels = row["labels"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|label| label["name"].as_str().or_else(|| label.as_str()))
            .map(str::to_string)
            .collect();
        Ok(Self {
            number,
            title: text("title"),
            body: text("body"),
            closed: row["state"].as_str() == Some("closed"),
            close_reason: row["state_reason"].as_str().map(str::to_string),
            labels,
            milestone: row["milestone"]["title"].as_str().map(str::to_string),
            pull_request: row.get("pull_request").is_some_and(|key| !key.is_null()),
        })
    }
}

impl HostedIssue for Issue {
    fn id(&self) -> String {
        id_of(self.number)
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn text(&self) -> &str {
        &self.body
    }

    fn labels(&self) -> &[String] {
        &self.labels
    }

    fn closed(&self) -> bool {
        self.closed
    }

    /// A closed issue is `done` unless it was closed as not planned or as a
    /// duplicate; an open one is `partial` with the partial label (spec 3.5).
    fn item_status(&self) -> ItemStatus {
        if self.closed {
            match self.close_reason.as_deref() {
                Some("not_planned" | "duplicate") => ItemStatus::Dropped,
                _ => ItemStatus::Done,
            }
        } else if self.has_label(PARTIAL) {
            ItemStatus::Partial
        } else {
            ItemStatus::Open
        }
    }

    /// The `houserules:<kind>` label that the issue carries: `amendment`,
    /// `parked`, `batch`, and, for a record that no command reads, `section`
    /// or `decision`.
    fn marked_kind(&self) -> Option<Kind> {
        let has = |kind: &str| self.has_label(&format!("houserules:{kind}"));
        if has("amendment") {
            Some(Kind::Amendment)
        } else if has("parked") {
            Some(Kind::Parked)
        } else if has("batch") {
            Some(Kind::Batch)
        } else if has("section") || has("decision") {
            Some(Kind::Other)
        } else {
            None
        }
    }

    fn milestone(&self) -> Option<&str> {
        self.milestone.as_deref()
    }

    /// The `see` field of the block.
    fn see(&self) -> Option<Value> {
        block::parse(&self.body).1.get("see").cloned()
    }

    fn answers_to(&self, id: &str) -> bool {
        number_of(id) == Some(self.number)
    }

    fn is_pull_request(&self) -> bool {
        self.pull_request
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::test_support::issue;
    use super::*;

    /// `#<n>` and the bare `<n>` are ids; nothing else is.
    #[test]
    fn an_id_is_a_number_with_or_without_the_hash() {
        for (id, number) in [("#12", Some(12)), ("12", Some(12)), ("#1", Some(1))] {
            assert_eq!(number_of(id), number, "{id}");
        }
        for id in [
            "",
            "#",
            "0",
            "#0",
            "012",
            "#012",
            "-1",
            "+1",
            "1.5",
            "a",
            "#a",
            "12abc",
            "acme/other#12",
            "other#12",
            "#12/x",
            " 12",
            "12 ",
            "99999999999999999999999",
        ] {
            assert_eq!(number_of(id), None, "{id:?}");
        }
    }

    /// Spec 3.5: the status of an issue from its state, its close reason, and
    /// its partial label.
    #[test]
    fn a_status_follows_the_state_the_close_reason_and_the_partial_label() {
        let mut open = issue(1, &[], "");
        assert_eq!(open.item_status(), ItemStatus::Open);
        open.labels.push(PARTIAL.to_string());
        assert_eq!(open.item_status(), ItemStatus::Partial);
        for (reason, status) in [
            (None, ItemStatus::Done),
            (Some("completed"), ItemStatus::Done),
            (Some("reopened"), ItemStatus::Done),
            (Some("not_planned"), ItemStatus::Dropped),
            (Some("duplicate"), ItemStatus::Dropped),
        ] {
            open.closed = true;
            open.close_reason = reason.map(str::to_string);
            assert_eq!(open.item_status(), status, "{reason:?}");
        }
    }

    /// Spec 3.5, C30: an issue with the label `houserules:parked` is a parked
    /// group when its block holds `group: true`, and a parked item otherwise.
    #[test]
    fn a_parked_issue_is_a_group_when_its_block_holds_group_true() {
        let kind = |body: &str| issue(1, &["houserules:parked", "batch:1"], body).kind();
        assert_eq!(
            kind("Intro.\n\n```houserules\ngroup: true\n```"),
            Kind::Group
        );
        for body in [
            "Text.",
            "Text.\n\n```houserules\ngroup: false\n```",
            "Text.\n\n```houserules\ngroup: \"true\"\n```",
            "Text.\n\n```houserules\nresolved: {\"batch\":2,\"note\":\"n\"}\n```",
        ] {
            assert_eq!(kind(body), Kind::Parked, "{body:?}");
        }
        let block = "Intro.\n\n```houserules\ngroup: true\n```";
        assert_eq!(
            issue(1, &["type:chore"], block).kind(),
            Kind::Item,
            "the block of an issue with no parked label is no group"
        );
    }

    /// Spec 3.5: the `houserules:<kind>` label names the record; a section or
    /// decision record is read by no command; a pull request is no item.
    #[test]
    fn the_marker_label_names_the_record() {
        let kind = |label: &str| issue(1, &[label, "type:chore"], "").kind();
        assert_eq!(kind("houserules:amendment"), Kind::Amendment);
        assert_eq!(kind("houserules:batch"), Kind::Batch);
        assert_eq!(kind("houserules:section"), Kind::Other);
        assert_eq!(kind("houserules:decision"), Kind::Other);
        assert_eq!(kind("houserules:deferred"), Kind::Item);
        let mut pull_request = issue(2, &["type:chore"], "");
        pull_request.pull_request = true;
        assert!(!pull_request.is_item());
    }

    /// Spec 3.5: the `see` list sits in the block, the milestone in its own
    /// field, and an id names the issue by its number, with or without the hash.
    #[test]
    fn the_see_list_the_milestone_and_the_id_come_from_the_issue() {
        let text = "Text.\n\n```houserules\nsee: [\"#1\", \"A-01\"]\n```";
        let mut item = issue(4, &["type:chore"], text);
        item.milestone = Some("v1".to_string());
        assert_eq!(item.see(), Some(json!(["#1", "A-01"])));
        assert_eq!(item.milestone(), Some("v1"));
        assert_eq!(issue(5, &[], "").see(), None);
        assert!(item.answers_to("#4") && item.answers_to("4"));
        assert!(!item.answers_to("#5") && !item.answers_to("04"));
        assert_eq!(item.id(), "#4");
    }

    /// Spec 3.5: a row of the issues endpoint reads with object labels or
    /// string labels, a milestone title, and the pull-request key.
    #[test]
    fn a_row_reads_its_labels_its_milestone_and_the_pull_request_key() {
        let row = json!({
            "number": 7, "title": "T", "body": null, "state": "closed",
            "state_reason": "not_planned",
            "labels": [{"name": "type:feat"}, "batch:3"],
            "milestone": {"title": "v1", "number": 2},
            "pull_request": {"url": "u"},
        });
        let issue = Issue::from_row(&row).expect("a row");
        assert_eq!(
            issue,
            Issue {
                number: 7,
                title: "T".to_string(),
                body: String::new(),
                closed: true,
                close_reason: Some("not_planned".to_string()),
                labels: vec!["type:feat".to_string(), "batch:3".to_string()],
                milestone: Some("v1".to_string()),
                pull_request: true,
            }
        );
        let plain = Issue::from_row(&json!({"number": 8, "pull_request": null})).expect("a row");
        assert!(
            !plain.pull_request,
            "a null pull_request key is no pull request"
        );
        assert_eq!(
            Issue::from_row(&json!({"title": "no number"})),
            Err(TrackerError::Command(
                "gh returned an issue with no number".to_string()
            ))
        );
    }
}
