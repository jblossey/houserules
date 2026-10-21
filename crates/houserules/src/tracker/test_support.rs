//! Test doubles of the tracker contract, for the tests of the code that
//! routes between the knowledge base and a tracker and of the code that every
//! adapter shares at `init --tracker`. Test-only
//! (`#[cfg(test)]` on the `mod test_support` declaration in `mod.rs`).

use serde_json::Value;

use super::{DeferRequest, ListOpts, Resolution, Resolved, Tracker, TrackerError, lines};

/// A tracker that panics on every operation that would reach a backend,
/// and whose `matches_id` answers `claims`: the double of "a tracker that
/// would fail if called".
pub(crate) struct NeverCalled {
    /// What `matches_id` answers for every id.
    pub(crate) claims: bool,
}

/// The panic of an operation that no test expects.
fn reached(operation: &str) -> ! {
    panic!("the test double was asked to {operation}, which reaches a backend")
}

impl Tracker for NeverCalled {
    fn name(&self) -> &'static str {
        "never-called"
    }

    fn matches_id(&self, _id: &str) -> bool {
        self.claims
    }

    fn resolve(&self, _id: &str) -> Result<Resolution, TrackerError> {
        reached("resolve")
    }

    fn list(&self, _opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
        reached("list")
    }

    fn batch(&self, _number: &str) -> Result<Value, TrackerError> {
        reached("batch")
    }

    fn get(&self, _ids: &[String]) -> Result<Vec<Value>, TrackerError> {
        reached("get")
    }

    fn set(&mut self, _id: &str, _assignments: &[String]) -> Result<Value, TrackerError> {
        reached("set")
    }

    fn defer(&mut self, _request: &DeferRequest) -> Result<String, TrackerError> {
        reached("defer")
    }

    fn archive_backlog_half(&mut self, _lines: &mut Vec<String>) -> Result<(), TrackerError> {
        reached("archive")
    }

    fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
        reached("check")
    }

    fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
        reached("describe its pattern")
    }
}

/// What `Claims::resolve` answers.
#[derive(Clone, Copy)]
pub(crate) enum Says {
    /// The id names a work item.
    Found,
    /// The tracker holds nothing under the id.
    NotFound,
    /// The id names a record that is no work item.
    NotAnItem,
}

/// A tracker that claims every id, resolves an id as `says`, and holds no
/// record for `get`. Every other operation panics.
pub(crate) struct Claims {
    /// The answer of `resolve`.
    pub(crate) says: Says,
}

impl Tracker for Claims {
    fn name(&self) -> &'static str {
        "claims"
    }

    fn matches_id(&self, _id: &str) -> bool {
        true
    }

    fn resolve(&self, _id: &str) -> Result<Resolution, TrackerError> {
        Ok(match self.says {
            Says::Found => Resolution::Found(Resolved { deferral: None }),
            Says::NotFound => Resolution::NotFound,
            Says::NotAnItem => Resolution::NotAnItem {
                reason: "is a record, not an item".to_string(),
            },
        })
    }

    fn list(&self, _opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
        reached("list")
    }

    fn batch(&self, _number: &str) -> Result<Value, TrackerError> {
        reached("batch")
    }

    fn get(&self, ids: &[String]) -> Result<Vec<Value>, TrackerError> {
        Err(TrackerError::NotFound(lines::unknown_id(&ids[0])))
    }

    fn set(&mut self, _id: &str, _assignments: &[String]) -> Result<Value, TrackerError> {
        reached("set")
    }

    fn defer(&mut self, _request: &DeferRequest) -> Result<String, TrackerError> {
        reached("defer")
    }

    fn archive_backlog_half(&mut self, _lines: &mut Vec<String>) -> Result<(), TrackerError> {
        reached("archive")
    }

    fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
        reached("check")
    }

    fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
        reached("describe its pattern")
    }
}

/// A tracker that answers only `backlog_id_pattern`: the double of "an adapter
/// other than JSON" for the code that every adapter shares at
/// `init --tracker`. Every other operation panics.
pub(crate) struct PatternOnly {
    /// The `backlogId` pattern the double reports; `None` for a backend that
    /// cannot name one, which fails with [`PatternOnly::unreachable`].
    pub(crate) pattern: Option<&'static str>,
}

impl PatternOnly {
    /// The error of a double that has no pattern: its backend cannot be reached.
    pub(crate) fn unreachable() -> TrackerError {
        TrackerError::Unreachable {
            adapter: "pattern-only",
            cause: "the backend is down".to_string(),
            setup: "bring it up".to_string(),
        }
    }
}

impl Tracker for PatternOnly {
    fn name(&self) -> &'static str {
        "pattern-only"
    }

    fn matches_id(&self, _id: &str) -> bool {
        reached("match an id")
    }

    fn resolve(&self, _id: &str) -> Result<Resolution, TrackerError> {
        reached("resolve")
    }

    fn list(&self, _opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
        reached("list")
    }

    fn batch(&self, _number: &str) -> Result<Value, TrackerError> {
        reached("batch")
    }

    fn get(&self, _ids: &[String]) -> Result<Vec<Value>, TrackerError> {
        reached("get")
    }

    fn set(&mut self, _id: &str, _assignments: &[String]) -> Result<Value, TrackerError> {
        reached("set")
    }

    fn defer(&mut self, _request: &DeferRequest) -> Result<String, TrackerError> {
        reached("defer")
    }

    fn archive_backlog_half(&mut self, _lines: &mut Vec<String>) -> Result<(), TrackerError> {
        reached("archive")
    }

    fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
        reached("check")
    }

    fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
        self.pattern
            .map(str::to_string)
            .ok_or_else(Self::unreachable)
    }
}
