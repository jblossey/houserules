//! The flat CLI's unified `get` command: resolves an id by shape
//! (`HR-031` is a backlog item, `process.tdd` a knowledge entry). Lives
//! at the crate root, like `emit`, because it is the one command that
//! spans both feature modules -- `rules` and `backlog` never depend on
//! each other (see `emit.rs`'s own module doc for why that boundary
//! matters), and a `get` that called from inside either one would break
//! it.
//!
//! Dispatch is per id, not two separate batch lookups merged afterward:
//! a domain is loaded only once a request actually needs it (lazily,
//! and at most once per invocation), and the first id that fails to
//! resolve -- in the order given, whichever domain it is in -- is where
//! lookup stops. One consequence, disclosed rather than engineered
//! around: an EMPTY id list here reports its usage error without
//! needing either the backlog or the knowledge base to exist -- there
//! is no id to decide a domain from, so neither loads.

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

use crate::backlog::{self, LoadedBacklog};
use crate::emit::emit;
use crate::rules::{self, Base};

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
/// parked item (`^PP-\d+-\d{2}$`), the closed set this binary treats as a
/// backlog id. `item_prefix` is the project's own stamped
/// `.houserules.json` `idPrefix` (`install::stamped_id_prefix`, `HR` in
/// this repository, `WI` by default): only the item shape is
/// prefix-parameterized, since `backlog/schema.json` fixes the amendment
/// and parked-item shapes to `A-`/`PP-` regardless of a project's own item
/// prefix. Every other shape resolves against the knowledge base instead:
/// a knowledge entry id always looks like `^[a-z0-9-]+\.[a-z0-9-]+$`
/// (`knowledge/schema.json`), lowercase with a dot, so the two
/// vocabularies never actually collide in practice -- but this function
/// only ever checks the backlog shapes, since an id this function rejects
/// still fails cleanly against the knowledge base with a named
/// `unknown id "..."` error.
///
/// The three shapes are checked independently -- each an `if` that only
/// ever returns on a genuine match, never on a structural prefix match
/// alone -- rather than the first matching literal prefix deciding the
/// whole result: `item_prefix` is adopter-chosen (`is_id_prefix` in
/// `install.rs` allows a single-letter prefix, `A` or `P` included), so a
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

/// Marks a record resolved from `backlog/archive/` or `knowledge/archive/`
/// (`crate::archive::find_archived_backlog_item`/
/// `find_archived_knowledge_entry`) with `"archived": true` -- the designed
/// token an archived id's `get` output carries (`quality.absence-is-
/// designed`'s own principle, applied to a record's active/archived state
/// rather than an absent field: a reader must see this at a glance, never
/// infer it from the record's own `status` field, which a plain backlog
/// item does not even carry).
fn label_archived(mut record: Value) -> Value {
    if let Value::Object(map) = &mut record {
        map.insert("archived".to_string(), Value::Bool(true));
    }
    record
}

/// Runs `get`: resolves each of `ids` by shape (`is_backlog_id`, against
/// the project's own stamped item prefix, `install::stamped_id_prefix`),
/// loading the backlog or the knowledge base at most once each, lazily,
/// and prints the results as one JSON array in the order given -- a
/// "needs at least one id" usage error when `ids` is empty; see the
/// module doc for why that check runs before either domain loads.
pub(crate) fn cmd_get(dir: Option<PathBuf>, ids: Vec<String>) -> ExitCode {
    if ids.is_empty() {
        eprintln!("get needs at least one id");
        return ExitCode::from(2);
    }
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let item_prefix = match crate::install::stamped_id_prefix(&root) {
        Ok(prefix) => prefix,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    let mut loaded_backlog: Option<LoadedBacklog> = None;
    let mut loaded_knowledge: Option<Base> = None;
    let mut values = Vec::with_capacity(ids.len());
    for id in &ids {
        let one = std::slice::from_ref(id);
        let result = if is_backlog_id(id, &item_prefix) {
            if loaded_backlog.is_none() {
                match backlog::load_backlog(&root) {
                    Ok(loaded) => loaded_backlog = Some(loaded),
                    Err(error) => {
                        eprintln!("{error}");
                        return ExitCode::from(2);
                    }
                }
            }
            let loaded = loaded_backlog.as_ref().expect("just loaded above");
            match backlog::get_items(loaded, one) {
                Ok(values) => Ok(values),
                Err(error) => match crate::archive::find_archived_any(&root, id) {
                    Ok(Some(record)) => Ok(vec![label_archived(record)]),
                    Ok(None) => Err(error.0),
                    Err(message) => Err(message),
                },
            }
        } else {
            if loaded_knowledge.is_none() {
                match rules::load_base(&root) {
                    Ok(base) => loaded_knowledge = Some(base),
                    Err(error) => {
                        eprintln!("{error}");
                        return ExitCode::from(2);
                    }
                }
            }
            let base = loaded_knowledge.as_ref().expect("just loaded above");
            match rules::get_entries(base, one) {
                Ok(values) => Ok(values),
                Err(active_message) => match crate::archive::find_archived_any(&root, id) {
                    Ok(Some(record)) => Ok(vec![label_archived(record)]),
                    Ok(None) => Err(active_message),
                    Err(message) => Err(message),
                },
            }
        };
        match result {
            Ok(mut resolved) => values.push(resolved.remove(0)),
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(2);
            }
        }
    }
    print!("{}", emit(&Value::Array(values)));
    ExitCode::SUCCESS
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
    /// (`install::is_id_prefix` allows it): its own 3-digit item ids must
    /// not shadow the fixed 2-digit amendment shape, proving the
    /// independent-branch design load-bearing rather than an early return
    /// on the first literal prefix match.
    #[test]
    fn the_amendment_shape_survives_a_colliding_single_letter_item_prefix() {
        assert!(is_backlog_id("A-001", "A"));
        assert!(is_backlog_id("A-01", "A"));
    }
}
