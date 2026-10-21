//! The JSON adapter: the `backlog/*.json` files of the install behind the
//! tracker contract (spec 3.5). It wraps the code of the 1.4.0 backlog
//! (`crate::backlog`) and the backlog half of `archive`, and prints what that
//! code printed. Each operation loads the backlog fresh, so a command sees
//! the files as they are when it runs.
//!
//! The adapter's backend is the `backlog/` directory: with the directory
//! missing, every operation is [`TrackerError::Unreachable`]. A directory
//! with a missing or invalid file is a load failure, named by file, as in
//! 1.4.0.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{DeferRequest, Deferral, ListOpts, Resolution, Resolved, Tracker, TrackerError, lines};
use crate::backlog::{self, CommandError, LoadedBacklog};

/// The tracker whose items live in `backlog/*.json` below `root`.
pub(super) struct JsonTracker {
    root: PathBuf,
    prefix: String,
}

impl JsonTracker {
    /// The adapter for the install at `root` whose item ids carry `prefix`.
    /// Reads nothing: the backlog loads when an operation needs it.
    pub(super) fn new(root: &Path, prefix: String) -> Self {
        Self {
            root: root.to_path_buf(),
            prefix,
        }
    }

    /// The error of a `backlog/` directory that does not exist.
    fn unreachable() -> TrackerError {
        TrackerError::Unreachable {
            adapter: "json",
            cause: "backlog/ is missing".to_string(),
            setup: "restore backlog/ from version control".to_string(),
        }
    }

    /// Fails with [`TrackerError::Unreachable`] when `backlog/` is missing.
    fn reach(&self) -> Result<(), TrackerError> {
        if self.root.join("backlog").is_dir() {
            Ok(())
        } else {
            Err(Self::unreachable())
        }
    }

    /// Loads the whole backlog.
    fn load(&self) -> Result<LoadedBacklog, TrackerError> {
        self.reach()?;
        backlog::load_backlog(&self.root).map_err(|error| TrackerError::Command(error.to_string()))
    }

    /// The stored record of `id`: an active item, an amendment, a parked
    /// item, or an archived item (`archived: true` attached). `None` when the
    /// backlog holds none.
    fn record(&self, b: &LoadedBacklog, id: &str) -> Result<Option<Value>, TrackerError> {
        if let Ok(mut found) = backlog::get_items(b, std::slice::from_ref(&id.to_string())) {
            return Ok(Some(found.remove(0)));
        }
        crate::archive::find_archived_backlog_item(&self.root, id)
            .map(|archived| archived.map(crate::archive::label_archived))
            .map_err(TrackerError::Command)
    }
}

/// One or more ASCII digits, exactly `len` of them when given, at least one
/// otherwise -- the digit-run shape every backlog id pattern below is built
/// from (`backlog/schema.json`'s own `\d{3}`/`\d{2}`/`\d+` fragments).
fn is_digit_run(s: &str, len: Option<usize>) -> bool {
    if s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    len.is_none_or(|len| s.len() == len)
}

/// `true` when `id` matches one of `backlog/schema.json`'s three id shapes
/// -- an item (`^<item_prefix>-\d{3}$`), an amendment (`^A-\d{2}$`), or a
/// parked item (`^PP-\d+-\d{2}$`), the closed set this adapter treats as a
/// backlog id. `item_prefix` is the project's own stamped
/// `.houserules.json` `idPrefix` (`HR` in this repository, `WI` by
/// default): only the item shape is prefix-parameterized, since
/// `backlog/schema.json` fixes the amendment and parked-item shapes to `A-`
/// and `PP-` regardless of a project's own item prefix.
///
/// The three shapes are checked independently -- each an `if` that only
/// ever returns on a genuine match, never on a structural prefix match
/// alone -- rather than the first matching literal prefix deciding the
/// whole result: `item_prefix` is adopter-chosen (`is_id_prefix` in
/// `install/marker.rs` allows a single-letter prefix, `A` or `P` included), so a
/// project's own item shape can share a leading literal with the fixed
/// `A-`/`PP-` shapes. Returning early on the item prefix's own digit-count
/// mismatch would then wrongly refuse an id the fixed shape it collides
/// with should still accept.
fn is_backlog_id(id: &str, item_prefix: &str) -> bool {
    if let Some(rest) = id
        .strip_prefix(item_prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        && is_digit_run(rest, Some(3))
    {
        return true;
    }
    if let Some(rest) = id.strip_prefix("A-")
        && is_digit_run(rest, Some(2))
    {
        return true;
    }
    if let Some(rest) = id.strip_prefix("PP-")
        && let Some((num, suffix)) = rest.split_once('-')
        && is_digit_run(num, None)
        && is_digit_run(suffix, Some(2))
    {
        return true;
    }
    false
}

/// What `resolve` reports for a stored `record`: its deferral marker. A
/// marker with no string `reason` carries an empty reason, which no gate
/// accepts.
fn resolved(record: &Value) -> Resolved {
    let deferral = record
        .get("deferral")
        .and_then(Value::as_object)
        .map(|map| Deferral {
            reason: map
                .get("reason")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        });
    Resolved { deferral }
}

/// A usage fault or load failure of the backlog code as a tracker error.
fn command(error: CommandError) -> TrackerError {
    TrackerError::Command(error.0)
}

impl Tracker for JsonTracker {
    fn name(&self) -> &'static str {
        "json"
    }

    fn matches_id(&self, id: &str) -> bool {
        is_backlog_id(id, &self.prefix)
    }

    fn resolve(&self, id: &str) -> Result<Resolution, TrackerError> {
        let b = self.load()?;
        Ok(match self.record(&b, id)? {
            Some(record) => Resolution::Found(resolved(&record)),
            None => Resolution::NotFound,
        })
    }

    fn list(&self, opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
        Ok(backlog::list_items(&self.load()?, opts))
    }

    fn batch(&self, number: &str) -> Result<Value, TrackerError> {
        backlog::batch_record(&self.load()?, number).map_err(command)
    }

    fn get(&self, ids: &[String]) -> Result<Vec<Value>, TrackerError> {
        let b = self.load()?;
        ids.iter()
            .map(|id| match self.record(&b, id)? {
                Some(record) => Ok(record),
                None => Err(TrackerError::NotFound(lines::unknown_id(id))),
            })
            .collect()
    }

    fn set(&mut self, id: &str, assignments: &[String]) -> Result<Value, TrackerError> {
        let b = self.load()?;
        let changes = backlog::set_item(&b, id, assignments).map_err(command)?;
        Ok(json!({"id": id, "changes": changes}))
    }

    fn defer(&mut self, request: &DeferRequest) -> Result<String, TrackerError> {
        let b = self.load()?;
        backlog::defer(&b, &self.prefix, request).map_err(command)
    }

    fn archive_backlog_half(&mut self, lines: &mut Vec<String>) -> Result<(), TrackerError> {
        self.reach()?;
        crate::archive::archive_backlog_half(&self.root, lines)
            .map(|_| ())
            .map_err(TrackerError::Command)
    }

    fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
        Ok(backlog::check_backlog(&self.load()?))
    }

    fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
        Ok(format!(
            "^({}-[0-9]+|A-[0-9]+|PP-[0-9]+-[0-9]+)$",
            self.prefix
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_every_backlog_id_shape_and_rejects_everything_else() {
        for id in ["HR-031", "HR-999", "A-01", "A-99", "PP-29-01", "PP-104-07"] {
            assert!(is_backlog_id(id, "HR"), "{id} should be a backlog id");
        }
        for id in [
            "process.tdd",
            "houserules.pnpm-only",
            "HR-99",
            "HR-9999",
            "HR-abc",
            "A-1",
            "PP-29-1",
            "PP-1",
            "",
        ] {
            assert!(!is_backlog_id(id, "HR"), "{id} should not be a backlog id");
        }
    }

    /// The item shape (`^<prefix>-\d{3}$`) follows the project's own
    /// stamped `idPrefix`, not a hardcoded `HR-` -- `WI-001` (the default
    /// `init` seeds) is a backlog id under the default `WI` prefix and not
    /// under `HR`, and vice versa for an `HR-` id under a non-`HR`
    /// project.
    #[test]
    fn resolves_the_item_shape_against_the_projects_own_stamped_prefix() {
        assert!(is_backlog_id("WI-001", "WI"));
        assert!(!is_backlog_id("WI-001", "HR"));
        assert!(is_backlog_id("ZZ-042", "ZZ"));
        assert!(!is_backlog_id("HR-042", "ZZ"));
    }

    /// The amendment (`A-`) and parked-item (`PP-`) shapes stay
    /// kit-fixed (`backlog/schema.json`) regardless of the project's own
    /// item prefix -- unaffected by `WI`/`ZZ` above.
    #[test]
    fn the_amendment_and_parked_item_shapes_ignore_the_projects_item_prefix() {
        for prefix in ["HR", "WI", "ZZ"] {
            assert!(is_backlog_id("A-01", prefix));
            assert!(is_backlog_id("PP-29-01", prefix));
        }
    }

    /// A project could validly stamp a single-letter `idPrefix` of `A`
    /// (`install::marker::is_id_prefix` allows it): its own 3-digit item ids must
    /// not shadow the fixed 2-digit amendment shape, proving the
    /// independent-branch design load-bearing rather than an early return
    /// on the first literal prefix match.
    #[test]
    fn the_amendment_shape_survives_a_colliding_single_letter_item_prefix() {
        assert!(is_backlog_id("A-001", "A"));
        assert!(is_backlog_id("A-01", "A"));
    }

    /// Spec 3.9: the pattern holds the stamped prefix, the amendment shape,
    /// and the parked shape.
    #[test]
    fn the_backlog_id_pattern_holds_the_prefix_and_the_two_fixed_shapes() {
        let tracker = JsonTracker::new(Path::new("."), "HR".to_string());
        assert_eq!(
            tracker.backlog_id_pattern(),
            Ok("^(HR-[0-9]+|A-[0-9]+|PP-[0-9]+-[0-9]+)$".to_string())
        );
    }

    /// `resolve` reads the deferral marker of a record: the reason of an item
    /// that carries one, an empty reason for a marker with none, and no
    /// marker for an item, an amendment, or a parked item that has none.
    #[test]
    fn resolved_reads_the_deferral_marker_of_a_record() {
        let marked = resolved(&json!({
            "id": "WI-001", "status": "done", "title": "Ship it.",
            "deferral": {"reason": "Waits.", "origin": "batch 3", "files": ["a.rs", 7]},
        }));
        assert_eq!(
            marked.deferral.map(|deferral| deferral.reason),
            Some("Waits.".to_string())
        );
        let unreasoned = resolved(&json!({"id": "WI-002", "deferral": {"reason": 3}}));
        assert_eq!(
            unreasoned.deferral.map(|deferral| deferral.reason),
            Some(String::new())
        );
        for unmarked in [
            json!({"id": "WI-003", "status": "open", "title": "T"}),
            json!({"id": "A-01", "status": "open", "label": "L", "text": ["T"]}),
            json!({"id": "PP-1-01", "text": "Polish.", "batch": 1}),
            json!({"id": "WI-004", "deferral": "Waits."}),
        ] {
            assert!(resolved(&unmarked).deferral.is_none(), "{unmarked}");
        }
    }
}
