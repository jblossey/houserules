//! The flat CLI's unified `get` command (batch 17 T4, docs/specs/
//! 2026-09-04-batch-15-tier2-spec.md §3): "`get` (resolves by id shape:
//! `HR-031` is a backlog item, `process.tdd` a knowledge entry)". The
//! frozen source ships this as two separate scripts, each with its own
//! `get` -- `tools/kb.mjs`'s `cmdGet` (a knowledge entry) and
//! `tools/backlog.mjs`'s `cmdGet` (a backlog item, amendment, or parked
//! item) -- but this binary has one flat command surface, so the two
//! cannot both be named `get`; this file is the one place that decides
//! which of them a given id means. Lives at the crate root, like `emit`,
//! because it is the one command that spans both feature modules -- `rules`
//! and `backlog` never depend on each other (see `emit.rs`'s own module doc
//! for why that boundary matters), and a `get` that called from inside
//! either one would break it.
//!
//! Dispatch is per id, not two separate batch lookups merged afterward: a
//! domain is loaded only once a request actually needs it (lazily, and at
//! most once per invocation), and the first id that fails to resolve -- in
//! the order given, whichever domain it is in -- is where lookup stops,
//! matching each frozen `cmdGet`'s own `Array.prototype.map` fail-fast
//! order. One consequence, disclosed rather than engineered around: unlike
//! either single-domain script, which loads its sole domain unconditionally
//! before checking for at least one id, an EMPTY id list here reports its
//! usage error without needing either the backlog or the knowledge base to
//! exist -- there is no id to decide a domain from, so neither loads. No
//! committed corpus slice exercises `get` with no arguments against a
//! broken repository, so this is a disclosed judgment call, not a pinned
//! parity gate.

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
/// -- an item (`^HR-\d{3}$`), an amendment (`^A-\d{2}$`), or a parked item
/// (`^PP-\d+-\d{2}$`), the closed, disjoint set this binary treats as a
/// backlog id. Every other shape resolves against the knowledge base
/// instead: a knowledge entry id always looks like
/// `^[a-z0-9-]+\.[a-z0-9-]+$` (`knowledge/schema.json`), lowercase with a
/// dot, so the two vocabularies never actually collide in practice -- but
/// this function only ever checks the backlog shapes, since an id this
/// function rejects reaching the knowledge base and failing there with
/// `unknown id "..."` is exactly the frozen `cmdGet`'s own message either
/// side would give it anyway.
fn is_backlog_id(id: &str) -> bool {
    if let Some(rest) = id.strip_prefix("HR-") {
        return is_digit_run(rest, Some(3));
    }
    if let Some(rest) = id.strip_prefix("A-") {
        return is_digit_run(rest, Some(2));
    }
    if let Some(rest) = id.strip_prefix("PP-") {
        return match rest.split_once('-') {
            Some((num, suffix)) => is_digit_run(num, None) && is_digit_run(suffix, Some(2)),
            None => false,
        };
    }
    false
}

/// Runs `get`: resolves each of `ids` by shape (`is_backlog_id`), loading
/// the backlog or the knowledge base at most once each, lazily, and prints
/// the results as one JSON array in the order given -- `main`'s own "needs
/// at least one id" usage error (shared, byte-identical text, by both
/// frozen `cmdGet`s) when `ids` is empty; see the module doc for why that
/// check runs before either domain loads here, unlike the frozen scripts'
/// own load-then-check order.
pub(crate) fn cmd_get(dir: Option<PathBuf>, ids: Vec<String>) -> ExitCode {
    if ids.is_empty() {
        eprintln!("get needs at least one id");
        return ExitCode::from(2);
    }
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };

    let mut loaded_backlog: Option<LoadedBacklog> = None;
    let mut loaded_knowledge: Option<Base> = None;
    let mut values = Vec::with_capacity(ids.len());
    for id in &ids {
        let one = std::slice::from_ref(id);
        let result = if is_backlog_id(id) {
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
            backlog::get_items(loaded, one).map_err(|error| error.0)
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
            rules::get_entries(base, one)
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
            assert!(is_backlog_id(id), "{id} should be a backlog id");
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
            assert!(!is_backlog_id(id), "{id} should not be a backlog id");
        }
    }
}
