//! Helpers that the unit tests of the Beads adapter share.

use super::issue::Issue;

/// An open issue titled `Issue <id>` with the type `issue_type`, the labels
/// `labels`, and `description`.
pub(super) fn issue(id: &str, issue_type: &str, labels: &[&str], description: &str) -> Issue {
    Issue {
        id: id.to_string(),
        title: format!("Issue {id}"),
        description: description.to_string(),
        status: "open".to_string(),
        issue_type: issue_type.to_string(),
        close_reason: None,
        labels: labels.iter().map(|label| label.to_string()).collect(),
        related: Vec::new(),
    }
}
