//! The writes of `defer` (spec 3.1 and 3.5): marking an issue that is an item
//! as a deferral, and filing a new item that carries one.

use serde_json::Value;

use super::issue::id_of;
use super::{Edit, GithubTracker};
use crate::tracker::TrackerError;
use crate::tracker::hosted::{DEFERRED, deferral_labels, deferred_text, filed_deferral_text};

impl GithubTracker {
    /// Marks the issue `id` as a deferral and returns its id.
    pub(super) fn mark_deferred(&self, id: &str, deferral: Value) -> Result<String, TrackerError> {
        let issue = self.item_named(id)?;
        let body = deferred_text(&issue.body, deferral);
        let add_labels = vec![DEFERRED.to_string()];
        self.issues.forget();
        self.ensure_labels(&add_labels)?;
        let edit = Edit {
            add_labels,
            ..Edit::default()
        };
        self.edit_issue(&issue.number.to_string(), &edit, Some(&body))?;
        Ok(id_of(issue.number))
    }

    /// Files a new deferral titled `title` in `section`, which the request
    /// check has found to hold only letters, digits, and hyphens, and returns
    /// its id.
    pub(super) fn file_deferred(
        &self,
        title: &str,
        section: &str,
        reason: &str,
        deferral: Value,
    ) -> Result<String, TrackerError> {
        let labels = deferral_labels(section);
        self.issues.forget();
        self.ensure_labels(&labels)?;
        let body = filed_deferral_text(reason, deferral);
        let mut args: Vec<&str> = vec![
            "issue",
            "create",
            "--repo",
            self.repo.as_str(),
            "--title",
            title,
            "--body",
            body.as_str(),
        ];
        for label in &labels {
            args.extend(["--label", label.as_str()]);
        }
        let printed = self.call(args)?;
        let number = printed
            .lines()
            .rev()
            .find_map(|line| {
                line.trim()
                    .rsplit('/')
                    .next()
                    .and_then(|last| last.parse().ok())
            })
            .ok_or_else(|| {
                TrackerError::Command("gh issue create printed no issue URL".to_string())
            })?;
        Ok(id_of(number))
    }
}
