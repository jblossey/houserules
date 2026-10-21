//! An issue that the unit tests of the shared code build. Test-only
//! (`#[cfg(test)]` on the `mod test_support` declaration in `mod.rs`).

use serde_json::Value;

use super::{HostedIssue, Kind};
use crate::backlog::ItemStatus;

/// An issue whose every accessor is a field, so that a test sets what the
/// shared code reads and nothing else.
pub(super) struct TestIssue {
    /// The id.
    pub(super) id: String,
    /// The title.
    pub(super) title: String,
    /// The prose and the block.
    pub(super) text: String,
    /// The labels.
    pub(super) labels: Vec<String>,
    /// Whether the issue is closed.
    pub(super) closed: bool,
    /// The status the issue maps to.
    pub(super) status: ItemStatus,
    /// The kind that the marker names.
    pub(super) marked: Option<Kind>,
    /// The milestone.
    pub(super) milestone: Option<String>,
    /// The `see` field.
    pub(super) see: Option<Value>,
    /// Whether the row is a pull request.
    pub(super) pull_request: bool,
}

impl HostedIssue for TestIssue {
    fn id(&self) -> String {
        self.id.clone()
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn text(&self) -> &str {
        &self.text
    }

    fn labels(&self) -> &[String] {
        &self.labels
    }

    fn closed(&self) -> bool {
        self.closed
    }

    fn item_status(&self) -> ItemStatus {
        self.status
    }

    fn marked_kind(&self) -> Option<Kind> {
        self.marked
    }

    fn milestone(&self) -> Option<&str> {
        self.milestone.as_deref()
    }

    fn see(&self) -> Option<Value> {
        self.see.clone()
    }

    fn answers_to(&self, id: &str) -> bool {
        self.id == id
    }

    fn is_pull_request(&self) -> bool {
        self.pull_request
    }
}

/// An open issue titled `Issue <id>` with the labels `labels`, `text`, and no
/// marker.
pub(super) fn issue(id: &str, labels: &[&str], text: &str) -> TestIssue {
    TestIssue {
        id: id.to_string(),
        title: format!("Issue {id}"),
        text: text.to_string(),
        labels: labels.iter().map(|label| label.to_string()).collect(),
        closed: false,
        status: ItemStatus::Open,
        marked: None,
        milestone: None,
        see: None,
        pull_request: false,
    }
}

/// `issue` with the marker `kind`.
pub(super) fn marked(kind: Kind, id: &str, labels: &[&str], text: &str) -> TestIssue {
    TestIssue {
        marked: Some(kind),
        ..issue(id, labels, text)
    }
}

/// `issue`, closed: it reads as `done`.
pub(super) fn closed(mut issue: TestIssue) -> TestIssue {
    issue.closed = true;
    issue.status = ItemStatus::Done;
    issue
}
