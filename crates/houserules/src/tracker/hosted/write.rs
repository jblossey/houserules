//! The parts of the writes of `set` and `defer` that both hosted adapters plan
//! the same way: what a set of changes asks for, which labels make a batch or a
//! milestone the only one, and the text and labels of a deferral. The calls that
//! carry a write out belong to the adapter.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{DEFERRED, HostedIssue};
use crate::backlog::ItemStatus;
use crate::tracker::{Change, block};

/// What the changes of one `set` ask for, the last change of a field winning.
#[derive(Debug, Default, PartialEq, Eq)]
pub(in crate::tracker) struct Wanted {
    /// The status, when a `status=<s>` change asks for one.
    pub(in crate::tracker) status: Option<ItemStatus>,
    /// The batch number, when a `batch=<n>` change asks for one.
    pub(in crate::tracker) batch: Option<i64>,
    /// The milestone, when a `milestone=<m>` change asks for one.
    pub(in crate::tracker) milestone: Option<String>,
}

impl Wanted {
    /// What `changes` ask for.
    pub(in crate::tracker) fn of(changes: &[Change]) -> Self {
        let mut wanted = Self::default();
        for change in changes {
            match change {
                Change::Status(status) => wanted.status = Some(*status),
                Change::Batch(batch) => wanted.batch = Some(*batch),
                Change::Milestone(milestone) => wanted.milestone = Some(milestone.clone()),
            }
        }
        wanted
    }
}

/// What makes `<family><value>` the only label of its family (`batch:`,
/// `milestone:`) that `issue` carries: the labels of the family to remove, and
/// the label to add when the issue lacks it.
pub(in crate::tracker) fn label_family_change<T: HostedIssue>(
    issue: &T,
    family: &str,
    value: &str,
) -> (Vec<String>, Option<String>) {
    let label = format!("{family}{value}");
    let remove = issue
        .labels()
        .iter()
        .filter(|name| name.starts_with(family) && **name != label)
        .cloned()
        .collect();
    let add = (!issue.has_label(&label)).then_some(label);
    (remove, add)
}

/// The text of an issue whose text is `text` once it carries `deferral`: the
/// prose stays, and the `deferral` field of the block is the new one.
pub(in crate::tracker) fn deferred_text(text: &str, deferral: Value) -> String {
    let (prose, mut fields) = block::parse(text);
    fields.insert("deferral".to_string(), deferral);
    block::render(&prose, &fields)
}

/// The text of a new deferral: the `reason` as prose, and the `deferral` field.
pub(in crate::tracker) fn filed_deferral_text(reason: &str, deferral: Value) -> String {
    block::render(
        reason,
        &BTreeMap::from([("deferral".to_string(), deferral)]),
    )
}

/// The labels of a new deferral filed in `section`, which the request check
/// has found to hold only letters, digits, and hyphens.
pub(in crate::tracker) fn deferral_labels(section: &str) -> [String; 3] {
    [
        "type:chore".to_string(),
        DEFERRED.to_string(),
        format!("section:{section}"),
    ]
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::test_support::issue;
    use super::*;
    use crate::tracker::parse_set;

    /// The last change of a field wins, and a field no change names stays
    /// unasked.
    #[test]
    fn the_last_change_of_a_field_wins() {
        let changes = parse_set(&[
            "status=open".to_string(),
            "batch=1".to_string(),
            "status=done".to_string(),
            "batch=2".to_string(),
        ])
        .expect("changes");
        assert_eq!(
            Wanted::of(&changes),
            Wanted {
                status: Some(ItemStatus::Done),
                batch: Some(2),
                milestone: None,
            }
        );
        assert_eq!(Wanted::of(&[]), Wanted::default());
    }

    /// A family keeps one label: the others go, a label the issue has asks for
    /// nothing, and a label of another family or of the same text inside a tag
    /// stays.
    #[test]
    fn a_family_keeps_one_label() {
        let carrying = issue("hr-1", &["batch:1", "tag:batch:x", "milestone:v1"], "");
        assert_eq!(
            label_family_change(&carrying, "batch:", "2"),
            (vec!["batch:1".to_string()], Some("batch:2".to_string()))
        );
        assert_eq!(
            label_family_change(&carrying, "batch:", "1"),
            (Vec::new(), None)
        );
        assert_eq!(
            label_family_change(&carrying, "milestone:", "v2"),
            (
                vec!["milestone:v1".to_string()],
                Some("milestone:v2".to_string())
            )
        );
    }

    /// A deferral keeps the prose of the issue and replaces its `deferral`
    /// field; a new deferral has its reason as prose.
    #[test]
    fn a_deferral_text_holds_the_prose_and_the_deferral_field() {
        let deferral = json!({"reason": "Why.", "origin": "batch 1", "files": []});
        let marked = deferred_text(
            "Prose.\n\n```houserules\ndeferral: {\"reason\":\"Old.\"}\nlabel: Core\n```",
            deferral.clone(),
        );
        let (prose, fields) = block::parse(&marked);
        assert_eq!(prose, "Prose.");
        assert_eq!(fields["deferral"], deferral);
        assert_eq!(fields["label"], json!("Core"));
        let filed = filed_deferral_text("Why.", deferral.clone());
        let (prose, fields) = block::parse(&filed);
        assert_eq!(prose, "Why.");
        assert_eq!(fields["deferral"], deferral);
        assert_eq!(fields.len(), 1);
    }

    /// A new deferral is a chore of its section that carries the marker.
    #[test]
    fn a_new_deferral_is_a_chore_of_its_section() {
        assert_eq!(
            deferral_labels("E09"),
            ["type:chore", "houserules:deferred", "section:E09"]
        );
    }
}
