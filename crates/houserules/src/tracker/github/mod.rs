//! The GitHub adapter (spec 3.5): the work items of an install are the issues
//! of one repository, read and written through the `gh` CLI (`super::gh`).
//!
//! # Mapping
//!
//! The mapping table of spec 3.5 is the contract. An issue that has a
//! `type:<t>` label and no `houserules:<kind>` label is an item. An issue that
//! has a `houserules:<kind>` label is a record: `batch`, `amendment`, and
//! `parked` (a group or an item) are read. `list`, `batch`, and the contract
//! checks read items and those records. A `section` or `decision` record is
//! read by none of them, and `get` prints it, and any other issue, with the
//! item shape and a `null` type, section, and milestone, so a `Work-Item:`
//! trailer may name it. `set` and `defer --item` write items only. A row whose
//! `pull_request` key is not null is a pull request: it is dropped
//! everywhere, and an id that names one is no issue. Only the repository of
//! the tracker is read: an id of another repository matches no shape.
//!
//! The layout of the records, which the conformance suite pins (C30):
//!
//! - an amendment keeps `type`, `label`, and `date` in its block; its text is
//!   the body, and it is `done` when its issue is closed;
//! - a parked group has the labels `houserules:parked` and `batch:<n>` and
//!   `group: true` in its block; its body is the intro of the group, and `get`
//!   prints it in the form of `backlog/parked.json`, with the parked items of
//!   its batch;
//! - a parked item has the label `houserules:parked` and the `batch:<n>` label
//!   of its group; its text is the body, and `resolved` sits in its block;
//! - a batch record is titled `Batch <n>: <summary>` and keeps `kickoff` and
//!   `status` in its block. `batch <n>` lists the items that carry
//!   `batch:<n>` (C29), as `list --batch <n>` does, while the JSON adapter
//!   lists the `items` of its `batches.json` record.
//!
//! # Reads and writes
//!
//! Every read is one pass over `gh api --paginate
//! repos/<repo>/issues?state=all&per_page=100`, kept for the life of the
//! adapter and dropped by a write. `list` and `batch` build the JSON shapes of
//! the 1.4.0 backlog and run the code of the JSON adapter on them, so a
//! filter, a row, and an error read the same on both; `get` prints the record
//! of one issue in the same shapes. Rows come back in ascending id order. Each
//! write creates the label or milestone it needs. `archive` moves nothing and
//! calls no backend: the issues of a repository stay where they are.

mod check;
mod defer;
mod edit;
mod issue;
#[cfg(test)]
mod test_support;

use std::collections::BTreeSet;
use std::rc::Rc;

use serde_json::{Value, json};

use super::gh::{Runner, parse_rows};
use super::hosted::{IssueCache, backlog_view, item_or_unknown, records_of, resolved};
use super::{
    DeferRequest, DeferTarget, ListOpts, Resolution, Tracker, TrackerError, changes_map,
    check_defer_request, lines, parse_set,
};
use crate::backlog::{self, CommandError};
use check::check_issues;
use edit::Edit;
use issue::{Issue, PULL_REQUEST, id_of, number_of};

/// What the tracker says about the issue `number` of `issues` (spec 3.5): any
/// issue is a work item, whatever its labels, except a pull request.
fn resolution(issues: &[Issue], number: u64) -> Resolution {
    match issues.iter().find(|issue| issue.number == number) {
        None => Resolution::NotFound,
        Some(issue) if issue.pull_request => Resolution::NotAnItem {
            reason: PULL_REQUEST.to_string(),
        },
        Some(issue) => Resolution::Found(resolved(issue)),
    }
}

/// The tracker whose work items are the issues of `repo`.
pub(super) struct GithubTracker {
    repo: String,
    gh: Runner,
    issues: IssueCache<Issue>,
}

impl GithubTracker {
    /// The adapter for `repo` (`<owner>/<name>`) that runs `gh`. Reads
    /// nothing: the issues load when an operation needs them.
    pub(super) fn new(repo: String, gh: Runner) -> Self {
        Self {
            repo,
            gh,
            issues: IssueCache::new(),
        }
    }

    /// Runs `gh` with `args`, and names a failure as an unreachable tracker.
    fn call<I, S>(&self, args: I) -> Result<String, TrackerError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.gh
            .run(args)
            .map_err(|error| error.into_tracker_error(&self.repo))
    }

    /// The rows that `gh api --paginate` returns for `collection` of the
    /// repository, with `query`.
    fn rows(&self, collection: &str, query: &str) -> Result<Vec<Value>, TrackerError> {
        let endpoint = format!("repos/{}/{collection}?{query}", self.repo);
        parse_rows(&self.call(["api", "--paginate", endpoint.as_str()])?)
    }

    /// Every issue of the repository, pull requests included, in ascending
    /// number order. The first call reads them; later calls share the read
    /// until a write drops it.
    fn issues(&self) -> Result<Rc<Vec<Issue>>, TrackerError> {
        self.issues.get_or_read(|| {
            let mut issues = self
                .rows("issues", "state=all&per_page=100")?
                .iter()
                .map(Issue::from_row)
                .collect::<Result<Vec<_>, _>>()?;
            issues.sort_by_key(|issue| issue.number);
            Ok(issues)
        })
    }

    /// The names of the labels of the repository.
    fn label_names(&self) -> Result<BTreeSet<String>, TrackerError> {
        Ok(self
            .rows("labels", "per_page=100")?
            .iter()
            .filter_map(|row| row["name"].as_str())
            .map(str::to_string)
            .collect())
    }

    /// The issue that `id` names, which is no pull request: an id with no
    /// shape, a number that no issue has, and a pull request are each a
    /// [`TrackerError::NotFound`] with the line that `missing` builds for the
    /// id as the caller gave it.
    fn issue_named(&self, id: &str, missing: fn(&str) -> String) -> Result<Issue, TrackerError> {
        let not_found = |line: String| TrackerError::NotFound(line);
        let number = number_of(id).ok_or_else(|| not_found(missing(id)))?;
        let issues = self.issues()?;
        let issue = issues
            .iter()
            .find(|issue| issue.number == number)
            .ok_or_else(|| not_found(missing(id)))?;
        if issue.pull_request {
            return Err(not_found(format!("{} {PULL_REQUEST}", id_of(number))));
        }
        Ok(issue.clone())
    }

    /// The issue that `id` names, which `set` and `defer --item` may write:
    /// an item (spec 3.5, C30). A record, and an issue with no `type:` label,
    /// are no item: each is a [`TrackerError::NotFound`] with the line of the
    /// JSON adapter, `unknown item "<id>"`, so no write reaches `gh`.
    fn item_named(&self, id: &str) -> Result<Issue, TrackerError> {
        item_or_unknown(self.issue_named(id, lines::unknown_item)?, id)
    }
}

impl Tracker for GithubTracker {
    fn name(&self) -> &'static str {
        "github"
    }

    fn matches_id(&self, id: &str) -> bool {
        number_of(id).is_some()
    }

    fn resolve(&self, id: &str) -> Result<Resolution, TrackerError> {
        let Some(number) = number_of(id) else {
            return Ok(Resolution::NotFound);
        };
        Ok(resolution(&self.issues()?, number))
    }

    fn list(&self, opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
        Ok(backlog::list_items(&backlog_view(&self.issues()?), opts))
    }

    fn batch(&self, number: &str) -> Result<Value, TrackerError> {
        backlog::batch_record(&backlog_view(&self.issues()?), number)
            .map_err(|CommandError(line)| TrackerError::Command(line))
    }

    fn get(&self, ids: &[String]) -> Result<Vec<Value>, TrackerError> {
        records_of(
            ids,
            |id| self.issue_named(id, lines::unknown_id),
            || self.issues(),
        )
    }

    fn set(&mut self, id: &str, assignments: &[String]) -> Result<Value, TrackerError> {
        let issue = self.item_named(id)?;
        let changes = parse_set(assignments).map_err(TrackerError::Command)?;
        self.apply(&issue, &Edit::plan(&issue, &changes))?;
        Ok(json!({"id": id_of(issue.number), "changes": changes_map(&changes)}))
    }

    fn defer(&mut self, request: &DeferRequest) -> Result<String, TrackerError> {
        let target = check_defer_request(request).map_err(TrackerError::Command)?;
        let deferral = request.deferral();
        match target {
            DeferTarget::Item(id) => self.mark_deferred(id, deferral),
            DeferTarget::New { title, section } => {
                self.file_deferred(title, section, &request.reason, deferral)
            }
        }
    }

    fn archive_backlog_half(&mut self, lines: &mut Vec<String>) -> Result<(), TrackerError> {
        lines.push(lines::archive_in_place(self.name()));
        Ok(())
    }

    fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
        let issues = self.issues()?;
        let labels = self.label_names()?;
        Ok(check_issues(&self.repo, &issues, &labels))
    }

    fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
        Ok("^#[0-9]+$".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::issue;
    use super::*;

    /// Spec 3.5 and 3.7: every issue is a work item that a trailer may name,
    /// whatever its labels, except a pull request; a deferral carries its
    /// reason (C31).
    #[test]
    fn any_issue_but_a_pull_request_resolves_as_a_work_item() {
        let mut pull_request = issue(2, &["type:feat"], "");
        pull_request.pull_request = true;
        let block = "Why.\n\n```houserules\ndeferral: {\"reason\":\"Why.\",\"origin\":\"batch 2\",\"files\":[\"a.rs\"]}\n```";
        let mut deferred = issue(3, &["houserules:deferred"], block);
        deferred.closed = true;
        let issues = [issue(1, &[], ""), pull_request, deferred];
        assert!(matches!(resolution(&issues, 9), Resolution::NotFound));
        assert!(matches!(
            resolution(&issues, 2),
            Resolution::NotAnItem { .. }
        ));
        let Resolution::Found(plain) = resolution(&issues, 1) else {
            panic!("an issue with no label resolves");
        };
        assert!(plain.deferral.is_none());
        let Resolution::Found(found) = resolution(&issues, 3) else {
            panic!("a deferral resolves");
        };
        assert_eq!(found.deferral.expect("a deferral").reason, "Why.");
    }
}
