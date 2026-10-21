//! What the hosted adapters share (spec 3.5): the GitHub adapter and the Beads
//! adapter keep the work items in the issues of a backend, and each reads an
//! issue through [`HostedIssue`]. The record shapes of the 1.4.0 backlog (C30),
//! the contract checks of `check-backlog`, the planning of the writes that both
//! adapters make the same way, and the cache of the issues of one adapter live
//! here once. An adapter keeps only what its backend decides: how a row becomes
//! an issue, which marker names a record, where the milestone and the `see`
//! list sit, and the calls that read and write.

mod cache;
mod check;
mod records;
#[cfg(test)]
mod test_support;
mod write;

use serde_json::Value;

use crate::backlog::ItemStatus;
use crate::tracker::{TrackerError, block, lines};

pub(super) use cache::IssueCache;
pub(super) use check::check_contract;
pub(super) use records::{backlog_view, records_of, resolved};
pub(super) use write::{
    Wanted, deferral_labels, deferred_text, filed_deferral_text, label_family_change,
};

/// The label of an issue that holds a deferral.
pub(super) const DEFERRED: &str = "houserules:deferred";

/// The kinds of issue that the hosted adapters read differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// A work item: an issue with a `type:<t>` label and no record marker.
    Item,
    /// An amendment record.
    Amendment,
    /// A parked item: the issue holds the text of one item of a parked group.
    Parked,
    /// A parked group: the issue holds the intro of a batch's parked items.
    Group,
    /// A batch record.
    Batch,
    /// A section or decision record, or an issue with no marker of the
    /// adapter: read with the item shape and a `null` type.
    Other,
}

/// The issue `issue`, which `id` named, as the item that `set` and `defer
/// --item` may write (spec 3.5, C30). A record, and an issue with no `type:`
/// label, are no item: each is a [`TrackerError::NotFound`] with the line of the
/// JSON adapter, `unknown item "<id>"`, so that no write reaches the backend.
pub(super) fn item_or_unknown<T: HostedIssue>(issue: T, id: &str) -> Result<T, TrackerError> {
    if issue.kind() == Kind::Item {
        Ok(issue)
    } else {
        Err(TrackerError::NotFound(lines::unknown_item(id)))
    }
}

/// An issue of a hosted tracker, as far as the record shapes and the contract
/// checks read it. The adapter implements the accessors; the provided methods
/// read the labels and the block the same way for every adapter.
pub(super) trait HostedIssue {
    /// The id that the tracker prints for the issue.
    fn id(&self) -> String;

    /// The title.
    fn title(&self) -> &str;

    /// The text that holds the prose and the houserules block: the body of a
    /// GitHub issue, the description of a Beads issue.
    fn text(&self) -> &str;

    /// The labels, in the order that the backend prints them.
    fn labels(&self) -> &[String];

    /// `true` when the issue is closed.
    fn closed(&self) -> bool;

    /// The JSON status that the state of the issue maps to (spec 3.5).
    fn item_status(&self) -> ItemStatus;

    /// The kind that the marker of the backend names: `Amendment`, `Parked`,
    /// `Batch`, or `Other` for a record that no command reads; `None` for an
    /// issue with no marker. A GitHub issue carries the marker as a
    /// `houserules:<kind>` label, a Beads issue as a custom type.
    fn marked_kind(&self) -> Option<Kind>;

    /// The milestone of the issue.
    fn milestone(&self) -> Option<&str>;

    /// The `see` field of the item record: the ids that the issue points to.
    fn see(&self) -> Option<Value>;

    /// `true` when `id`, as a person writes it, names this issue.
    fn answers_to(&self, id: &str) -> bool;

    /// `true` for a row that the backend lists among its issues and that is no
    /// issue: a GitHub pull request. Every shared function leaves such a row
    /// out.
    fn is_pull_request(&self) -> bool {
        false
    }

    /// `true` when the issue carries `label`.
    fn has_label(&self, label: &str) -> bool {
        self.labels().iter().any(|name| name == label)
    }

    /// The values of the labels that start with `prefix`, in label order.
    fn label_values<'a>(&'a self, prefix: &'a str) -> impl Iterator<Item = &'a str> {
        self.labels()
            .iter()
            .filter_map(move |label| label.strip_prefix(prefix))
    }

    /// The item type, when the issue has a `type:<t>` label.
    fn item_type(&self) -> Option<&str> {
        self.label_values("type:").next()
    }

    /// The batch number of a `batch:<n>` label.
    fn batch(&self) -> Option<u64> {
        self.label_values("batch:")
            .find_map(|value| value.parse().ok().filter(|batch| *batch > 0))
    }

    /// What kind of record the issue is. The marker of the backend decides; a
    /// parked issue is a group when its block holds `group: true`; an issue
    /// with no marker is an item when it has a `type:<t>` label.
    fn kind(&self) -> Kind {
        match self.marked_kind() {
            Some(Kind::Parked) if self.is_group() => Kind::Group,
            Some(kind) => kind,
            None if self.item_type().is_some() => Kind::Item,
            None => Kind::Other,
        }
    }

    /// `true` when the block of the issue holds `group: true`.
    fn is_group(&self) -> bool {
        block::parse(self.text()).1.get("group") == Some(&Value::Bool(true))
    }

    /// `true` when the issue is an item: a `type:<t>` label, no record marker,
    /// and no pull request.
    fn is_item(&self) -> bool {
        !self.is_pull_request() && self.kind() == Kind::Item
    }
}
