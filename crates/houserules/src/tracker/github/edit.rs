//! The writes of `set` (spec 3.5): what a set of changes asks of an issue,
//! and the `gh` calls that carry it out. Each write creates the label and the
//! milestone that it needs.

use super::GithubTracker;
use super::issue::{Issue, PARTIAL};
use crate::backlog::ItemStatus;
use crate::tracker::hosted::{HostedIssue, Wanted, label_family_change};
use crate::tracker::{Change, TrackerError};

/// The calls that bring `issue` to the state `changes` ask for.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Edit {
    /// The labels to add to the issue.
    pub(super) add_labels: Vec<String>,
    /// The labels to remove from the issue.
    pub(super) remove_labels: Vec<String>,
    /// The milestone to set, when the issue holds another.
    pub(super) milestone: Option<String>,
    /// `gh issue reopen` runs: the issue is closed and a status change is
    /// asked for.
    pub(super) reopen: bool,
    /// `gh issue close --reason` runs last.
    pub(super) close_reason: Option<&'static str>,
}

impl Edit {
    /// What `changes` ask of `issue`, the last change of a field winning. A
    /// change that the issue already holds asks for nothing.
    pub(super) fn plan(issue: &Issue, changes: &[Change]) -> Self {
        let wanted = Wanted::of(changes);
        let mut edit = Self::default();
        if let Some(status) = wanted.status {
            edit.plan_status(issue, status);
        }
        if let Some(batch) = wanted.batch {
            let (remove, add) = label_family_change(issue, "batch:", &batch.to_string());
            edit.remove_labels.extend(remove);
            edit.add_labels.extend(add);
        }
        if let Some(milestone) = wanted.milestone {
            edit.milestone =
                (issue.milestone.as_deref() != Some(milestone.as_str())).then_some(milestone);
        }
        edit
    }

    /// Adds what the status `wanted` asks of `issue`. An issue that is closed
    /// for another reason is reopened first: `gh issue close` leaves a closed
    /// issue as it is.
    fn plan_status(&mut self, issue: &Issue, wanted: ItemStatus) {
        let reason = match wanted {
            ItemStatus::Done => Some("completed"),
            ItemStatus::Dropped => Some("not planned"),
            ItemStatus::Open | ItemStatus::Partial => None,
        };
        let partial = wanted == ItemStatus::Partial;
        if !partial && issue.has_label(PARTIAL) {
            self.remove_labels.push(PARTIAL.to_string());
        }
        if partial && !issue.has_label(PARTIAL) {
            self.add_labels.push(PARTIAL.to_string());
        }
        if issue.item_status() == wanted || (partial && !issue.closed) {
            return;
        }
        self.reopen = issue.closed;
        self.close_reason = reason;
    }
}

impl GithubTracker {
    /// Creates each of `wanted` that the repository has not.
    pub(super) fn ensure_labels(&self, wanted: &[String]) -> Result<(), TrackerError> {
        if wanted.is_empty() {
            return Ok(());
        }
        let known = self.label_names()?;
        for label in wanted.iter().filter(|label| !known.contains(*label)) {
            self.call([
                "label",
                "create",
                label.as_str(),
                "--repo",
                self.repo.as_str(),
            ])?;
        }
        Ok(())
    }

    /// Creates the milestone `title` when the repository has not got it.
    fn ensure_milestone(&self, title: &str) -> Result<(), TrackerError> {
        let known = self.rows("milestones", "state=all&per_page=100")?;
        if known.iter().any(|row| row["title"].as_str() == Some(title)) {
            return Ok(());
        }
        let endpoint = format!("repos/{}/milestones", self.repo);
        self.call([
            "api",
            endpoint.as_str(),
            "-f",
            format!("title={title}").as_str(),
        ])?;
        Ok(())
    }

    /// Carries out `edit` on `issue`, and drops the read of the issues whatever
    /// the outcome: a call that failed may have changed the repository.
    /// Everything that the edit needs is created first: the labels, then the
    /// milestone. A reopen comes before a close.
    pub(super) fn apply(&self, issue: &Issue, edit: &Edit) -> Result<(), TrackerError> {
        self.issues.forget();
        self.ensure_labels(&edit.add_labels)?;
        if let Some(milestone) = &edit.milestone {
            self.ensure_milestone(milestone)?;
        }
        let number = issue.number.to_string();
        self.edit_issue(&number, edit, None)?;
        if edit.reopen {
            self.call([
                "issue",
                "reopen",
                "--repo",
                self.repo.as_str(),
                "--",
                number.as_str(),
            ])?;
        }
        if let Some(reason) = edit.close_reason {
            self.call([
                "issue",
                "close",
                "--repo",
                self.repo.as_str(),
                "--reason",
                reason,
                "--",
                number.as_str(),
            ])?;
        }
        Ok(())
    }

    /// Runs `gh issue edit` for `number` with `edit`'s labels and milestone,
    /// and `body` when given; makes no call when there is nothing to change.
    pub(super) fn edit_issue(
        &self,
        number: &str,
        edit: &Edit,
        body: Option<&str>,
    ) -> Result<(), TrackerError> {
        if edit.add_labels.is_empty()
            && edit.remove_labels.is_empty()
            && edit.milestone.is_none()
            && body.is_none()
        {
            return Ok(());
        }
        let mut args: Vec<&str> = vec!["issue", "edit", "--repo", self.repo.as_str()];
        for label in &edit.add_labels {
            args.extend(["--add-label", label.as_str()]);
        }
        for label in &edit.remove_labels {
            args.extend(["--remove-label", label.as_str()]);
        }
        if let Some(milestone) = &edit.milestone {
            args.extend(["--milestone", milestone.as_str()]);
        }
        if let Some(body) = body {
            args.extend(["--body", body]);
        }
        args.extend(["--", number]);
        self.call(args)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::issue;
    use super::*;
    use crate::tracker::parse_set;

    /// The change that a set of assignments asks of an issue: only what the
    /// issue does not already hold.
    #[test]
    fn an_edit_asks_only_for_what_the_issue_lacks() {
        let mut open = issue(3, &["type:chore", "batch:1", PARTIAL], "");
        open.milestone = Some("v1".to_string());
        let assignments = |items: &[&str]| -> Vec<Change> {
            parse_set(
                &items
                    .iter()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>(),
            )
            .expect("assignments")
        };
        assert_eq!(
            Edit::plan(
                &open,
                &assignments(&["batch=1", "milestone=v1", "status=partial"])
            ),
            Edit::default()
        );
        assert_eq!(
            Edit::plan(
                &open,
                &assignments(&["batch=2", "milestone=v2", "status=done"])
            ),
            Edit {
                add_labels: vec!["batch:2".to_string()],
                remove_labels: vec![PARTIAL.to_string(), "batch:1".to_string()],
                milestone: Some("v2".to_string()),
                reopen: false,
                close_reason: Some("completed"),
            }
        );
        let mut done = issue(4, &[], "");
        done.closed = true;
        assert_eq!(
            Edit::plan(&done, &assignments(&["status=dropped"])),
            Edit {
                reopen: true,
                close_reason: Some("not planned"),
                ..Edit::default()
            }
        );
        assert_eq!(
            Edit::plan(&done, &assignments(&["status=open"])),
            Edit {
                reopen: true,
                ..Edit::default()
            }
        );
        assert_eq!(
            Edit::plan(&done, &assignments(&["status=done"])),
            Edit::default()
        );
    }
}
