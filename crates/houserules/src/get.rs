//! The flat CLI's unified `get` command: routes each id between the
//! knowledge base and the tracker of the install (spec 3.8). Lives at the
//! crate root, like `emit`, because it is the one command that spans both
//! feature modules -- `rules` and `backlog` never depend on each other (see
//! `emit.rs`'s own module doc for why that boundary matters), and a `get`
//! that called from inside either one would break it.
//!
//! The routing, per id:
//!
//! - A knowledge-shaped id (`<topic>.<rest>`, `knowledge/schema.json`'s id
//!   pattern) whose first part is a loaded topic goes to the knowledge base.
//!   The tracker is asked about it only when the id also matches one of the
//!   adapter's declared id shapes (`Tracker::matches_id`, which reaches no
//!   backend); an id that both resolve fails.
//! - Every other id goes to the tracker. With no tracker, the answer is line
//!   A or line B of spec 3.4. An unresolved knowledge-shaped id whose first
//!   part names a kit pack that the install lacks gets the pack line, in
//!   place of line A or B, or after the tracker's not-found line.
//!
//! So a knowledge read never needs the tracker, and an unreachable tracker
//! never stops `get process.tdd`. Dispatch is per id: the knowledge base
//! loads once, lazily, for the first knowledge-shaped id, and the tracker is
//! built once, lazily, for the first id that needs it. The first id that
//! fails to resolve -- in the order given, whichever domain it is in -- is
//! where lookup stops. One consequence, disclosed rather than engineered
//! around: an EMPTY id list reports its usage error without needing the
//! knowledge base or the tracker to exist -- there is no id to route.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::Value;

use crate::archive::{find_archived_knowledge_entry, label_archived};
use crate::emit::emit;
use crate::packs::kit_packs;
use crate::rules::{self, Base};
use crate::tracker::{self, Lookup, Resolution, Tracker, TrackerError, lines};

/// The first part of `id` when `id` is knowledge-shaped: `<topic>.<rest>`,
/// each part of lowercase letters, digits, and hyphens
/// (`knowledge/schema.json`'s id pattern).
fn knowledge_topic(id: &str) -> Option<&str> {
    let slug = |text: &str| {
        !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    id.split_once('.')
        .filter(|(topic, rest)| slug(topic) && slug(rest))
        .map(|(topic, _)| topic)
}

/// Resolves the ids of one `get` call, loading what an id needs at most once.
struct Router {
    root: PathBuf,
    base: Option<Base>,
    tracker: Option<Result<Lookup, TrackerError>>,
}

impl Router {
    /// A router for the install at `root` that has loaded nothing yet.
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            base: None,
            tracker: None,
        }
    }

    /// The knowledge base, loaded on the first call. A load failure is the
    /// line to print.
    fn base(&mut self) -> Result<&Base, String> {
        if self.base.is_none() {
            let base = rules::load_base(&self.root).map_err(|error| error.to_string())?;
            self.base = Some(base);
        }
        Ok(self.base.as_ref().expect("loaded above"))
    }

    /// What the install holds as a tracker, built on the first call.
    fn tracker(&mut self) -> &Result<Lookup, TrackerError> {
        let root = &self.root;
        self.tracker.get_or_insert_with(|| tracker::lookup(root))
    }

    /// The tracker that may claim a knowledge id, when the install has one
    /// that can be built. An install with none, and a tracker that cannot be
    /// built, claim nothing: a knowledge read never needs the tracker.
    fn claiming_tracker(&mut self) -> Option<&dyn Tracker> {
        match self.tracker() {
            Ok(Lookup::Installed(tracker)) => Some(tracker.as_ref()),
            Ok(Lookup::Absent(_)) | Err(_) => None,
        }
    }

    /// Resolves `id`, or says why not as the lines to print on stderr.
    fn resolve(&mut self, id: &str) -> Result<Value, Vec<String>> {
        if let Some(topic) = knowledge_topic(id) {
            let loaded = self
                .base()
                .map_err(|line| vec![line])?
                .topics
                .iter()
                .any(|loaded| loaded.name == topic);
            if loaded {
                return self.resolve_in_knowledge(id);
            }
            // An archived entry keeps resolving when its topic file is gone
            // (`knowledge-base.ids-are-permanent`).
            match find_archived_knowledge_entry(&self.root, id) {
                Ok(Some(record)) => return Ok(label_archived(record)),
                Ok(None) => {}
                Err(message) => return Err(vec![message]),
            }
        }
        self.resolve_in_tracker(id)
    }

    /// Resolves `id`, whose first part is a loaded topic, in the knowledge
    /// base, and refuses it when the tracker resolves it too.
    fn resolve_in_knowledge(&mut self, id: &str) -> Result<Value, Vec<String>> {
        self.base().map_err(|line| vec![line])?;
        let base = self.base.as_ref().expect("loaded above");
        let record = read_knowledge(base, &self.root, id);
        let claims = self
            .claiming_tracker()
            .is_some_and(|tracker| tracker.matches_id(id));
        match (record, claims) {
            (Ok(record), false) => Ok(record),
            (Ok(record), true) => {
                let tracker = self.claiming_tracker().expect("claims above");
                match tracker.resolve(id) {
                    Ok(Resolution::Found(_)) => {
                        Err(vec![lines::resolves_in_both(id, tracker.name())])
                    }
                    Ok(_) => Ok(record),
                    Err(error) => Err(vec![error.to_string()]),
                }
            }
            (Err(message), false) => Err(vec![message]),
            (Err(_), true) => self.resolve_in_tracker(id),
        }
    }

    /// Resolves `id` in the tracker. With no tracker the answer is line A or
    /// line B, unless the id names a kit pack that the install lacks.
    fn resolve_in_tracker(&mut self, id: &str) -> Result<Value, Vec<String>> {
        let pack_line = self.pack_line(id);
        match self.tracker() {
            Ok(Lookup::Installed(tracker)) => {
                match tracker.get(std::slice::from_ref(&id.to_string())) {
                    Ok(mut records) => Ok(records.remove(0)),
                    Err(TrackerError::NotFound(message)) => {
                        let mut printed = vec![message];
                        printed.extend(pack_line);
                        Err(printed)
                    }
                    Err(error) => Err(vec![error.to_string()]),
                }
            }
            Ok(Lookup::Absent(state)) => {
                Err(vec![pack_line.unwrap_or_else(|| state.line().to_string())])
            }
            Err(error) => Err(vec![error.to_string()]),
        }
    }

    /// The pack line for `id` when its first part names a kit pack that the
    /// install lacks: it needs the knowledge base, which a knowledge-shaped
    /// id has loaded by now.
    fn pack_line(&self, id: &str) -> Option<String> {
        let topic = knowledge_topic(id)?;
        let base = self.base.as_ref()?;
        let carried = kit_packs().iter().any(|pack| pack.name == topic);
        let installed = base.packs.iter().any(|pack| pack.name == topic);
        (carried && !installed).then(|| lines::pack_not_installed(id, topic))
    }
}

/// The knowledge record of `id`: the active entry, else the archived entry
/// labelled `archived`. A miss is the line of the active lookup, and an
/// unreadable archive is its own named line.
fn read_knowledge(base: &Base, root: &Path, id: &str) -> Result<Value, String> {
    match rules::get_entries(base, std::slice::from_ref(&id.to_string())) {
        Ok(mut values) => Ok(values.remove(0)),
        Err(active_message) => match find_archived_knowledge_entry(root, id) {
            Ok(Some(record)) => Ok(label_archived(record)),
            Ok(None) => Err(active_message),
            Err(message) => Err(message),
        },
    }
}

/// Runs `get`: routes each of `ids` (see the module doc) and prints the
/// results as one JSON array in the order given -- a "needs at least one id"
/// usage error when `ids` is empty; see the module doc for why that check
/// runs before anything loads.
pub(crate) fn cmd_get(dir: Option<PathBuf>, ids: Vec<String>) -> ExitCode {
    if ids.is_empty() {
        eprintln!("get needs at least one id");
        return ExitCode::from(2);
    }
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let mut router = Router::new(root);
    let mut values = Vec::with_capacity(ids.len());
    for id in &ids {
        match router.resolve(id) {
            Ok(value) => values.push(value),
            Err(printed) => {
                for line in printed {
                    eprintln!("{line}");
                }
                return ExitCode::from(2);
            }
        }
    }
    print!("{}", emit(&Value::Array(values)));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::tracker::NoTracker;
    use crate::tracker::test_support::{Claims, NeverCalled, Says};

    /// A knowledge base with one topic `test` and the entry `test.one`, on
    /// disk under a scratch directory.
    fn knowledge_install() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let knowledge = dir.path().join("knowledge");
        fs::create_dir_all(&knowledge).expect("create knowledge/");
        fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
        fs::write(knowledge.join("areas.json"), "{}").expect("write areas.json");
        fs::write(
            knowledge.join("test.json"),
            serde_json::to_string(&json!({"topic": "test", "entries": [
                {"id": "test.one", "kind": "rule", "area": "global", "summary": "One.",
                 "body": ["B."], "tags": [], "source": {"date": "2026-10-20", "by": "review"}},
            ]}))
            .expect("serialize"),
        )
        .expect("write test.json");
        dir
    }

    /// A router over `dir` whose tracker is the `lookup` given.
    fn router(dir: &Path, lookup: Result<Lookup, TrackerError>) -> Router {
        let mut router = Router::new(dir.to_path_buf());
        router.tracker = Some(lookup);
        router
    }

    #[test]
    fn a_topic_is_the_first_part_of_a_knowledge_shaped_id() {
        assert_eq!(knowledge_topic("process.tdd"), Some("process"));
        assert_eq!(
            knowledge_topic("knowledge-base.x-1"),
            Some("knowledge-base")
        );
        for id in [
            "WI-001",
            "A-01",
            "PP-1-01",
            "",
            ".x",
            "x.",
            "Process.tdd",
            "a.b.c",
            "bd-a3f.1.2",
        ] {
            assert_eq!(knowledge_topic(id), None, "{id:?}");
        }
    }

    /// Spec 3.8: `get` of a knowledge id with a loaded topic never calls the
    /// tracker, though the tracker is installed.
    #[test]
    fn a_knowledge_id_never_calls_a_tracker_that_does_not_claim_it() {
        let dir = knowledge_install();
        let mut router = router(
            dir.path(),
            Ok(Lookup::Installed(Box::new(NeverCalled { claims: false }))),
        );
        let record = router.resolve("test.one").expect("the entry");
        assert_eq!(record["id"], json!("test.one"));
    }

    /// Spec 3.8: an id that both the knowledge base and the tracker resolve
    /// fails with the line of the spec.
    #[test]
    fn an_id_that_resolves_in_both_fails() {
        let dir = knowledge_install();
        let mut router = router(
            dir.path(),
            Ok(Lookup::Installed(Box::new(Claims { says: Says::Found }))),
        );
        assert_eq!(
            router.resolve("test.one"),
            Err(vec![
                "test.one resolves both in the knowledge base and in the claims tracker"
                    .to_string()
            ])
        );
    }

    /// A tracker that claims the shape but holds nothing under the id, or
    /// holds a record that is no work item, leaves the knowledge answer alone.
    #[test]
    fn a_claiming_tracker_that_holds_no_work_item_leaves_the_knowledge_answer() {
        let dir = knowledge_install();
        for says in [Says::NotFound, Says::NotAnItem] {
            let mut router = router(dir.path(), Ok(Lookup::Installed(Box::new(Claims { says }))));
            assert!(router.resolve("test.one").is_ok());
        }
    }

    /// A tracker that cannot be built claims nothing, so a knowledge read
    /// still works.
    #[test]
    fn a_tracker_that_cannot_be_built_does_not_stop_a_knowledge_read() {
        let dir = knowledge_install();
        let mut router = router(
            dir.path(),
            Err(TrackerError::Command("not built".to_string())),
        );
        assert!(router.resolve("test.one").is_ok());
    }

    /// An id of an unloaded topic goes to the tracker; with none it gets
    /// line A or line B, or the pack line for a kit pack the install lacks.
    #[test]
    fn an_id_of_no_loaded_topic_gets_the_line_of_an_install_without_a_tracker() {
        let dir = knowledge_install();
        let mut bare = router(dir.path(), Ok(Lookup::Absent(NoTracker::Bare)));
        assert_eq!(
            bare.resolve("WI-001"),
            Err(vec![lines::NO_TRACKER.to_string()])
        );
        assert_eq!(
            bare.resolve("other.thing"),
            Err(vec![lines::NO_TRACKER.to_string()])
        );
        let mut with_dir = router(dir.path(), Ok(Lookup::Absent(NoTracker::BacklogDir)));
        assert_eq!(
            with_dir.resolve("WI-001"),
            Err(vec![lines::BACKLOG_WITHOUT_TRACKER.to_string()])
        );
        assert_eq!(
            with_dir.resolve("process.tdd"),
            Err(vec![
                "process.tdd: pack process is not installed; run houserules pack add process"
                    .to_string()
            ])
        );
    }

    /// A tracker that does not hold the id answers with its own line; the
    /// pack line follows it for a kit pack the install lacks.
    #[test]
    fn the_pack_line_follows_the_not_found_line_of_a_tracker() {
        let dir = knowledge_install();
        let mut router = router(
            dir.path(),
            Ok(Lookup::Installed(Box::new(Claims {
                says: Says::NotFound,
            }))),
        );
        assert_eq!(
            router.resolve("quality.anything"),
            Err(vec![
                "unknown id \"quality.anything\"".to_string(),
                "quality.anything: pack quality is not installed; run houserules pack add quality"
                    .to_string()
            ])
        );
    }

    /// An unknown id of a loaded topic is the knowledge base's own line, not
    /// line A.
    #[test]
    fn an_unknown_id_of_a_loaded_topic_is_the_knowledge_not_found_line() {
        let dir = knowledge_install();
        let mut router = router(dir.path(), Ok(Lookup::Absent(NoTracker::Bare)));
        assert_eq!(
            router.resolve("test.missing"),
            Err(vec!["unknown id \"test.missing\"".to_string()])
        );
    }
}
