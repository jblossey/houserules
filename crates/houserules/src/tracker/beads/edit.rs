//! The writes of `set` (spec 3.5): what a set of changes asks of an issue, and
//! the `bd` calls that carry it out. A label needs no step of its own in Beads:
//! the first issue that carries it creates it, and so a batch or a milestone
//! that is new never fails a write.

use super::BeadsTracker;
use super::issue::{DROPPED, Issue, PARTIAL};
use crate::backlog::ItemStatus;
use crate::tracker::hosted::{HostedIssue, Wanted, label_family_change};
use crate::tracker::{Change, TrackerError};

/// The status that `bd update` sets for `open`.
const OPEN: &str = "open";

/// The close reason of an issue that is `done`.
const COMPLETED: &str = "completed";

/// The calls that bring an issue to the state `changes` ask for.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Edit {
    /// The labels that `bd update` adds.
    pub(super) add_labels: Vec<String>,
    /// The labels that `bd update` removes.
    pub(super) remove_labels: Vec<String>,
    /// The status that `bd update` sets: `open` or the custom status `partial`.
    pub(super) status: Option<&'static str>,
    /// `bd reopen` runs first: the issue is closed and a status change is
    /// asked for. `bd close` keeps the reason of an issue that is closed, so a
    /// closed issue is reopened before it is closed with another reason.
    pub(super) reopen: bool,
    /// `bd close --reason` runs last.
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
            edit.plan_label(issue, "batch:", &batch.to_string());
        }
        if let Some(milestone) = wanted.milestone {
            edit.plan_label(issue, "milestone:", &milestone);
        }
        edit
    }

    /// Makes `<family><value>` the only label of its family (`batch:` or
    /// `milestone:`) that `issue` carries.
    fn plan_label(&mut self, issue: &Issue, family: &str, value: &str) {
        let (remove, add) = label_family_change(issue, family, value);
        self.remove_labels.extend(remove);
        self.add_labels.extend(add);
    }

    /// Adds what the status `wanted` asks of `issue`.
    fn plan_status(&mut self, issue: &Issue, wanted: ItemStatus) {
        if issue.item_status() == wanted {
            return;
        }
        self.reopen = issue.closed();
        match wanted {
            ItemStatus::Done => self.close_reason = Some(COMPLETED),
            ItemStatus::Dropped => self.close_reason = Some(DROPPED),
            ItemStatus::Partial => self.status = Some(PARTIAL),
            // `bd reopen` made a closed issue open.
            ItemStatus::Open => self.status = (!issue.closed()).then_some(OPEN),
        }
    }
}

impl BeadsTracker {
    /// Carries out `edit` on `issue`, and drops the read of the issues whatever
    /// the outcome: a call that failed may have changed the store. A reopen
    /// comes first, the update of labels and status next, a close last.
    pub(super) fn apply(&self, issue: &Issue, edit: &Edit) -> Result<(), TrackerError> {
        self.issues.forget();
        if edit.reopen {
            self.call(["reopen", "--", issue.id.as_str()])?;
        }
        self.update_issue(&issue.id, edit, None)?;
        if let Some(reason) = edit.close_reason {
            self.call(["close", "--reason", reason, "--", issue.id.as_str()])?;
        }
        Ok(())
    }

    /// Runs `bd update` for `id` with the labels and the status of `edit`, and
    /// `description` when given; makes no call when there is nothing to change.
    pub(super) fn update_issue(
        &self,
        id: &str,
        edit: &Edit,
        description: Option<&str>,
    ) -> Result<(), TrackerError> {
        if edit.add_labels.is_empty()
            && edit.remove_labels.is_empty()
            && edit.status.is_none()
            && description.is_none()
        {
            return Ok(());
        }
        let description = description.map(|text| format!("--description={text}"));
        let mut args: Vec<&str> = vec!["update"];
        for label in &edit.add_labels {
            args.extend(["--add-label", label.as_str()]);
        }
        for label in &edit.remove_labels {
            args.extend(["--remove-label", label.as_str()]);
        }
        if let Some(status) = edit.status {
            args.extend(["--status", status]);
        }
        if let Some(description) = &description {
            args.push(description.as_str());
        }
        args.extend(["--", id]);
        self.call(args)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::issue;
    use super::*;
    use crate::tracker::parse_set;

    /// The changes that a set of assignments asks for.
    fn assignments(items: &[&str]) -> Vec<Change> {
        parse_set(
            &items
                .iter()
                .map(|item| item.to_string())
                .collect::<Vec<_>>(),
        )
        .expect("assignments")
    }

    /// The change that a set of assignments asks of an issue: only what the
    /// issue does not already hold; a label replaces the label of its family.
    #[test]
    fn an_edit_asks_only_for_what_the_issue_lacks() {
        let mut partial = issue(
            "hr-3",
            "chore",
            &["type:chore", "batch:1", "milestone:v1", "tag:batch:x"],
            "",
        );
        partial.status = PARTIAL.to_string();
        assert_eq!(
            Edit::plan(
                &partial,
                &assignments(&["batch=1", "milestone=v1", "status=partial"])
            ),
            Edit::default()
        );
        assert_eq!(
            Edit::plan(
                &partial,
                &assignments(&["batch=2", "milestone=v2", "status=done"])
            ),
            Edit {
                add_labels: vec!["batch:2".to_string(), "milestone:v2".to_string()],
                remove_labels: vec!["batch:1".to_string(), "milestone:v1".to_string()],
                status: None,
                reopen: false,
                close_reason: Some("completed"),
            },
            "a tag that holds the text of a family is another label"
        );
        assert_eq!(
            Edit::plan(&partial, &assignments(&["status=open"])),
            Edit {
                status: Some("open"),
                ..Edit::default()
            }
        );
    }

    /// A closed issue is reopened before it gets another close reason or a
    /// status; `open` needs only the reopen; an issue that reads as `open`
    /// already asks for nothing.
    #[test]
    fn a_closed_issue_is_reopened_before_another_status() {
        let mut closed = issue("hr-4", "chore", &["type:chore"], "");
        closed.status = "closed".to_string();
        let plan = |wanted: &str| Edit::plan(&closed, &assignments(&[wanted]));
        assert_eq!(plan("status=done"), Edit::default());
        assert_eq!(
            plan("status=dropped"),
            Edit {
                reopen: true,
                close_reason: Some("dropped"),
                ..Edit::default()
            }
        );
        assert_eq!(
            plan("status=open"),
            Edit {
                reopen: true,
                ..Edit::default()
            }
        );
        assert_eq!(
            plan("status=partial"),
            Edit {
                reopen: true,
                status: Some("partial"),
                ..Edit::default()
            }
        );
        let mut working = issue("hr-5", "chore", &["type:chore"], "");
        working.status = "in_progress".to_string();
        assert_eq!(
            Edit::plan(&working, &assignments(&["status=open"])),
            Edit::default()
        );
    }
}
