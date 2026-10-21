//! The writes of `defer` (spec 3.1 and 3.5): marking an issue that is an item
//! as a deferral, and filing a new item that carries one. A deferral whose
//! origin is the id of an issue that the store holds also gets a
//! `discovered-from` dependency on it.

use super::issue::is_issue_id;
use super::{BeadsTracker, Edit};
use crate::tracker::hosted::{DEFERRED, deferral_labels, deferred_text, filed_deferral_text};
use crate::tracker::{DeferRequest, TrackerError};

impl BeadsTracker {
    /// The id of the issue that `origin` names, when the store holds one: the
    /// text has the shape of an id and an issue has it. Any other origin is
    /// text, and it gets no link.
    fn linked_origin(&self, origin: &str) -> Result<Option<String>, TrackerError> {
        let origin = origin.trim();
        if !is_issue_id(origin) {
            return Ok(None);
        }
        Ok(self
            .issues()?
            .iter()
            .any(|issue| issue.id == origin)
            .then(|| origin.to_string()))
    }

    /// Links the deferral `id` to the issue it was discovered from.
    fn link_origin(&self, id: &str, origin: Option<String>) -> Result<(), TrackerError> {
        if let Some(origin) = origin.filter(|origin| origin != id) {
            self.call([
                "dep",
                "add",
                "--type",
                "discovered-from",
                "--",
                id,
                origin.as_str(),
            ])?;
        }
        Ok(())
    }

    /// Marks the issue `id` as a deferral and returns its id.
    pub(super) fn mark_deferred(
        &self,
        id: &str,
        request: &DeferRequest,
    ) -> Result<String, TrackerError> {
        let issue = self.item_named(id)?;
        let origin = self.linked_origin(&request.origin)?;
        let description = deferred_text(&issue.description, request.deferral());
        let edit = Edit {
            add_labels: vec![DEFERRED.to_string()],
            ..Edit::default()
        };
        self.issues.forget();
        self.update_issue(&issue.id, &edit, Some(&description))?;
        self.link_origin(&issue.id, origin)?;
        Ok(issue.id)
    }

    /// Files a new deferral titled `title` in `section`, which the request
    /// check has found to hold only letters, digits, and hyphens, and returns
    /// its id.
    pub(super) fn file_deferred(
        &self,
        title: &str,
        section: &str,
        request: &DeferRequest,
    ) -> Result<String, TrackerError> {
        let origin = self.linked_origin(&request.origin)?;
        let labels = deferral_labels(section).join(",");
        let body = filed_deferral_text(&request.reason, request.deferral());
        self.issues.forget();
        let created = self
            .bd
            .json([
                "create",
                format!("--title={title}").as_str(),
                "--type",
                "chore",
                "--labels",
                labels.as_str(),
                format!("--description={body}").as_str(),
                "--json",
            ])
            .map_err(|error| error.into_tracker_error())?;
        let id = created["id"]
            .as_str()
            .filter(|id| is_issue_id(id))
            .ok_or_else(|| TrackerError::Command("bd create printed no issue id".to_string()))?
            .to_string();
        self.link_origin(&id, origin)?;
        Ok(id)
    }
}
