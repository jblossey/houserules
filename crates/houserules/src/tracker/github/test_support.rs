//! The issue builder that the unit tests of the adapter share. Test-only
//! (`#[cfg(test)]` on the `mod test_support` declaration in `mod.rs`).

use super::issue::Issue;

/// An issue with `number`, `labels`, and `body`, open.
pub(super) fn issue(number: u64, labels: &[&str], body: &str) -> Issue {
    Issue {
        number,
        title: format!("Issue {number}"),
        body: body.to_string(),
        closed: false,
        close_reason: None,
        labels: labels.iter().map(|label| label.to_string()).collect(),
        milestone: None,
        pull_request: false,
    }
}
