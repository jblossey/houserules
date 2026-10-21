//! The Beads adapter (spec 3.5): the work items of an install are the issues
//! of a Beads store, read and written through the `bd` CLI (`runner`).
//!
//! # Mapping
//!
//! The Beads column of the mapping table of spec 3.5 is the contract. An issue
//! that has a `type:<t>` label and is of no custom record type is an item. An
//! issue of the custom type `houserules-batch`, `houserules-amendment`, or
//! `houserules-parked` (a group or an item) is a record that `list`, `batch`,
//! and the contract checks read. An issue of the type `houserules-section` or
//! `houserules-decision` is read by none of them; the native type `decision` is
//! a type of items and stays apart from the record. `get` prints a record that
//! is none of these, and any other issue, with the item shape and a `null`
//! type, section, and milestone, so a `Work-Item:` trailer may name it. `set`
//! and `defer --item` write items only.
//!
//! The layout of the records, which the conformance suite pins (C30), is the
//! layout of the GitHub adapter, built by the code that the two share
//! (`hosted`), with the custom types of the store in place of the
//! `houserules:<kind>` labels; `batch <n>` lists the items that carry
//! `batch:<n>` (C29). The fields that no issue field holds are in a fenced block
//! at the end of the description (`block`). The `see` list is the `related`
//! dependencies of the issue, in id order, because Beads keeps no order for
//! them. A deferral is the label `houserules:deferred` and the `deferral` field
//! of the block, and a `discovered-from` dependency when its origin is an issue
//! of the store.
//!
//! # Reads and writes
//!
//! Every read is one `bd list --json --all --limit 0` (`bd list` stops at 50
//! rows without the limit and leaves out closed issues without `--all`), kept
//! for the life of the adapter and dropped by a write. It holds the issues that
//! `bd list` lists: the infrastructure beads, gates, templates, and wisps that
//! `bd list` hides are no work items. `list` and `batch` build the JSON shapes
//! of the 1.4.0 backlog and run the code of the JSON adapter on them, so a
//! filter, a row, and an error read the same on both; `get` prints the record
//! of one issue in the same shapes. Rows come back in ascending id order, as
//! text, as `bd list --sort id` sorts them. A write is a `bd update`, `bd
//! reopen`, `bd close`, `bd create`, or `bd dep add` call, with each id after
//! `--`. `archive` moves nothing and calls no backend: the issues of a store
//! stay where they are.
//!
//! # The issue prefix
//!
//! The store keeps its issue prefix (`bd config get issue_prefix`); the
//! marker does not. The adapter reads it only for `backlog_id_pattern` (spec
//! 3.9), so building the adapter, and every read of the knowledge base, makes
//! no call to `bd`. `init --tracker beads` reads the prefix once, before any
//! write ([`prefix_for_init`]), and builds the adapter with it. An adapter built
//! without the prefix asks the store on its first `backlog_id_pattern`. A store
//! that cannot name the prefix is the error of that call, never a default
//! pattern: the pattern is written only from a prefix that `bd` named (C37). An
//! id has the shape of a Beads id whatever its prefix (`issue::is_issue_id`),
//! and the store answers for the prefix.

mod check;
mod defer;
mod edit;
mod issue;
mod runner;
#[cfg(test)]
mod test_support;

use std::cell::OnceCell;
use std::path::Path;
use std::rc::Rc;

use serde_json::{Value, json};

use super::hosted::{IssueCache, backlog_view, item_or_unknown, records_of, resolved};
use super::{
    DeferRequest, DeferTarget, ListOpts, Resolution, Tracker, TrackerError, changes_map,
    check_defer_request, lines, parse_set,
};
use crate::backlog::{self, CommandError};
use check::{Config, check_issues};
use edit::Edit;
use issue::{Issue, is_issue_id};
use runner::{BdError, Runner};

/// The release of `bd` that `mise.toml` pins and the seeded CI workflow
/// installs. A test reads `mise.toml` and requires the two to agree.
pub(crate) const BD_VERSION: &str = "1.3.1";

/// Reads the issue prefix of the store that `runner` reaches.
fn read_prefix(runner: &Runner) -> Result<String, BdError> {
    let reply = runner.json(["config", "get", "issue_prefix", "--json"])?;
    let prefix = reply["value"].as_str().unwrap_or_default().trim();
    let fit = !prefix.is_empty()
        && !prefix.starts_with('-')
        && !prefix.ends_with('-')
        && prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    if fit {
        Ok(prefix.to_string())
    } else {
        Err(BdError::Output(format!(
            "the issue prefix \"{prefix}\", which is empty or holds a character outside letters, digits, underscores, and hyphens"
        )))
    }
}

/// The issue prefix of the Beads store of the install at `root` for
/// `init --tracker beads`, as `bd` reports it. A missing `bd` and a missing
/// store are the refusal of spec 3.3; any other failure is the line of an
/// unreachable tracker (spec 3.4).
pub(crate) fn prefix_for_init(root: &Path) -> Result<String, TrackerError> {
    read_prefix(&Runner::from_env(root)).map_err(|error| match error {
        BdError::NotInstalled | BdError::NoStore => {
            TrackerError::Command(lines::BEADS_NEEDS_STORE.to_string())
        }
        other => other.into_tracker_error(),
    })
}

/// The pattern of the ids of a store with the issue prefix `prefix` (spec 3.9):
/// the prefix, a hyphen, a lowercase base-36 hash, and child levels.
fn id_pattern(prefix: &str) -> String {
    format!("^{prefix}-[0-9a-z]+(\\.[0-9]+)*$")
}

/// What the tracker says about the issue `id` of `issues` (spec 3.5): any issue
/// is a work item, whatever its labels.
fn resolution(issues: &[Issue], id: &str) -> Resolution {
    issues
        .iter()
        .find(|issue| issue.id == id)
        .map_or(Resolution::NotFound, |issue| {
            Resolution::Found(resolved(issue))
        })
}

/// The tracker whose work items are the issues of the Beads store of an
/// install.
pub(super) struct BeadsTracker {
    bd: Runner,
    issues: IssueCache<Issue>,
    prefix: OnceCell<String>,
}

impl BeadsTracker {
    /// The adapter for the install at `root`, with the issue prefix `prefix`
    /// when `bd` has named it already. Reads nothing: the issues load when an
    /// operation needs them.
    pub(super) fn new(root: &Path, prefix: Option<&str>) -> Self {
        Self {
            bd: Runner::from_env(root),
            issues: IssueCache::new(),
            prefix: prefix.map_or_else(OnceCell::new, |prefix| OnceCell::from(prefix.to_string())),
        }
    }

    /// Runs `bd` with `args`, and names a failure as an unreachable tracker.
    fn call<I, S>(&self, args: I) -> Result<String, TrackerError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.bd.run(args).map_err(BdError::into_tracker_error)
    }

    /// Every issue of the store in ascending id order, as text. The first call
    /// reads them; later calls share the read until a write drops it.
    fn issues(&self) -> Result<Rc<Vec<Issue>>, TrackerError> {
        self.issues.get_or_read(|| {
            let reply = self
                .bd
                .json(["list", "--json", "--all", "--limit", "0"])
                .map_err(BdError::into_tracker_error)?;
            let Value::Array(rows) = reply else {
                return Err(
                    BdError::Output("a list that is no array".to_string()).into_tracker_error()
                );
            };
            let mut issues = rows
                .iter()
                .map(Issue::from_row)
                .collect::<Result<Vec<_>, _>>()?;
            issues.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(issues)
        })
    }

    /// The `config` of the store, from `bd info --json`.
    fn config(&self) -> Result<Config, TrackerError> {
        let reply = self
            .bd
            .json(["info", "--json"])
            .map_err(BdError::into_tracker_error)?;
        Ok(reply["config"].as_object().cloned().unwrap_or_default())
    }

    /// The issue prefix of the store: the one the adapter was built with, else
    /// the one `bd` names on the first call. A store that cannot name it is the
    /// error of the call, and the next call asks again.
    fn prefix(&self) -> Result<&str, TrackerError> {
        if let Some(prefix) = self.prefix.get() {
            return Ok(prefix);
        }
        let read = read_prefix(&self.bd).map_err(BdError::into_tracker_error)?;
        Ok(self.prefix.get_or_init(|| read))
    }

    /// The issue that `id` names: an id that has no shape of a Beads id and an
    /// id that no issue has are each a [`TrackerError::NotFound`] with the line
    /// that `missing` builds for the id as the caller gave it. An id with no
    /// shape reaches no backend.
    fn issue_named(&self, id: &str, missing: fn(&str) -> String) -> Result<Issue, TrackerError> {
        let not_found = || TrackerError::NotFound(missing(id));
        if !is_issue_id(id) {
            return Err(not_found());
        }
        self.issues()?
            .iter()
            .find(|issue| issue.id == id)
            .cloned()
            .ok_or_else(not_found)
    }

    /// The issue that `id` names, which `set` and `defer --item` may write: an
    /// item (spec 3.5, C30). A record, and an issue with no `type:` label, are
    /// no item: each is a [`TrackerError::NotFound`] with the line of the JSON
    /// adapter, `unknown item "<id>"`, so no write reaches `bd`.
    fn item_named(&self, id: &str) -> Result<Issue, TrackerError> {
        item_or_unknown(self.issue_named(id, lines::unknown_item)?, id)
    }
}

impl Tracker for BeadsTracker {
    fn name(&self) -> &'static str {
        "beads"
    }

    fn matches_id(&self, id: &str) -> bool {
        is_issue_id(id)
    }

    fn resolve(&self, id: &str) -> Result<Resolution, TrackerError> {
        if !is_issue_id(id) {
            return Ok(Resolution::NotFound);
        }
        Ok(resolution(&self.issues()?, id))
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
        Ok(json!({"id": issue.id, "changes": changes_map(&changes)}))
    }

    fn defer(&mut self, request: &DeferRequest) -> Result<String, TrackerError> {
        match check_defer_request(request).map_err(TrackerError::Command)? {
            DeferTarget::Item(id) => self.mark_deferred(id, request),
            DeferTarget::New { title, section } => self.file_deferred(title, section, request),
        }
    }

    fn archive_backlog_half(&mut self, lines: &mut Vec<String>) -> Result<(), TrackerError> {
        lines.push(lines::archive_in_place(self.name()));
        Ok(())
    }

    fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
        let issues = self.issues()?;
        let config = self.config()?;
        Ok(check_issues(&issues, &config))
    }

    /// The pattern of the ids of the store's issue prefix. When `bd` cannot
    /// name the prefix, the call fails with the error of `bd` (C37).
    fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
        Ok(id_pattern(self.prefix()?))
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::issue;
    use super::*;

    /// The pin in `mise.toml` and `BD_VERSION` are one release.
    #[test]
    fn bd_version_is_the_release_that_mise_toml_pins() {
        let mise =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../mise.toml"))
                .expect("read mise.toml");
        let pinned = mise
            .lines()
            .find_map(|line| {
                line.strip_prefix("\"github:gastownhall/beads\" = \"")?
                    .strip_suffix('"')
            })
            .expect("mise.toml pins github:gastownhall/beads");
        assert_eq!(BD_VERSION, pinned);
    }

    /// Spec 3.9: the pattern holds the prefix, a hyphen, a lowercase base-36
    /// hash, and child levels; a prefix with a hyphen or a capital stays whole.
    #[test]
    fn the_id_pattern_holds_the_prefix_a_hash_and_child_levels() {
        assert_eq!(id_pattern("hr"), "^hr-[0-9a-z]+(\\.[0-9]+)*$");
        assert_eq!(id_pattern("My-Proj_2"), "^My-Proj_2-[0-9a-z]+(\\.[0-9]+)*$");
    }

    /// A tracker whose `bd` is a program that does not exist, so that any call
    /// to the backend is the error "bd is not installed", and whose prefix is
    /// known already: `prefix` when it is given, and "`bd` cannot name it" when
    /// it is not.
    fn tracker_without_bd(prefix: Option<&str>) -> BeadsTracker {
        let root = std::env::temp_dir();
        BeadsTracker {
            bd: Runner::with_program("houserules-no-such-bd-program", &root),
            issues: IssueCache::new(),
            prefix: prefix.map_or_else(OnceCell::new, |prefix| OnceCell::from(prefix.to_string())),
        }
    }

    /// Spec 3.7 and 3.8: an id of no declared shape is answered with no call to
    /// `bd`; an id of the shape is a question for the store, and a store that
    /// cannot be reached is the error.
    #[test]
    fn an_id_of_no_declared_shape_reaches_no_backend() {
        let tracker = tracker_without_bd(None);
        assert!(!tracker.matches_id("--help"));
        assert!(matches!(
            tracker.resolve("--help"),
            Ok(Resolution::NotFound)
        ));
        assert_eq!(
            tracker.get(&["--help".to_string()]),
            Err(TrackerError::NotFound("unknown id \"--help\"".to_string()))
        );
        assert!(tracker.matches_id("hr-1"));
        let unreachable = TrackerError::Unreachable {
            adapter: "beads",
            cause: "bd is not installed".to_string(),
            setup: "install bd".to_string(),
        };
        assert!(matches!(
            tracker.resolve("hr-1"),
            Err(error) if error == unreachable
        ));
    }

    /// Spec 3.9 and C37: the pattern of the ids holds the prefix of the store;
    /// when `bd` cannot name the prefix, the call fails with the error of `bd`
    /// and names no default pattern.
    #[test]
    fn the_pattern_holds_the_prefix_or_fails_with_the_error_of_bd() {
        assert_eq!(
            tracker_without_bd(Some("my-proj")).backlog_id_pattern(),
            Ok("^my-proj-[0-9a-z]+(\\.[0-9]+)*$".to_string())
        );
        assert_eq!(
            tracker_without_bd(None).backlog_id_pattern(),
            Err(TrackerError::Unreachable {
                adapter: "beads",
                cause: "bd is not installed".to_string(),
                setup: "install bd".to_string(),
            })
        );
    }

    /// An adapter built with the prefix that `init` read asks `bd` for nothing.
    #[test]
    fn an_adapter_built_with_the_prefix_asks_bd_for_nothing() {
        let tracker = BeadsTracker::new(&std::env::temp_dir(), Some("hr"));
        assert_eq!(
            tracker.backlog_id_pattern(),
            Ok("^hr-[0-9a-z]+(\\.[0-9]+)*$".to_string())
        );
    }

    /// Spec 3.5 and 3.7: every issue is a work item that a trailer may name,
    /// whatever its labels; a deferral carries its reason (C31).
    #[test]
    fn any_issue_resolves_as_a_work_item() {
        let issues = [
            issue("hr-1", "task", &[], ""),
            issue(
                "hr-3",
                "chore",
                &["houserules:deferred"],
                "Why.\n\n```houserules\ndeferral: {\"reason\":\"Why.\",\"origin\":\"batch 2\",\"files\":[]}\n```",
            ),
        ];
        assert!(matches!(resolution(&issues, "hr-9"), Resolution::NotFound));
        let Resolution::Found(plain) = resolution(&issues, "hr-1") else {
            panic!("an issue with no label resolves");
        };
        assert!(plain.deferral.is_none());
        let Resolution::Found(found) = resolution(&issues, "hr-3") else {
            panic!("a deferral resolves");
        };
        assert_eq!(found.deferral.expect("a deferral").reason, "Why.");
    }

    /// `archive` moves nothing, makes no call, and says so (spec 3.5).
    #[test]
    fn archive_moves_nothing_and_makes_no_call() {
        let mut tracker = tracker_without_bd(None);
        let mut lines = Vec::new();
        tracker
            .archive_backlog_half(&mut lines)
            .expect("no call to bd");
        assert_eq!(
            lines,
            ["archive: the beads tracker keeps closed items in place"]
        );
    }
}
