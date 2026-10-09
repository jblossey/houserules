//! Test fixtures shared by the `rules::` modules that read a knowledge
//! base: a loadable base on disk, an entry to put in it, and a valid
//! `check`. `stats.rs` and `proposals.rs` use them; a module that needs a
//! base in a test imports from here and from no sibling.

use std::fs;

use serde_json::{Value, json};
use tempfile::TempDir;

use super::model::{Base, load_base};

/// A loadable knowledge base on disk holding `entries` in one topic file.
/// `schema.json` and `areas.json` are empty objects: `load_base` only
/// needs them to parse. The returned guard removes the directory on drop
/// (`houserules.tests-clean-scratch-dirs`).
pub(super) fn base_with(entries: &[Value]) -> (TempDir, Base) {
    let dir = tempfile::tempdir().expect("tempdir");
    let knowledge = dir.path().join("knowledge");
    fs::create_dir_all(&knowledge).expect("create knowledge/");
    fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
    fs::write(knowledge.join("areas.json"), "{}").expect("write areas.json");
    fs::write(
        knowledge.join("test.json"),
        serde_json::to_string(&json!({"entries": entries})).expect("serialize entries"),
    )
    .expect("write test.json");
    let base = load_base(dir.path()).expect("load base");
    (dir, base)
}

/// A knowledge entry with the fields the rows read; `overrides` wins.
pub(super) fn entry(id: &str, overrides: Value) -> Value {
    let mut entry = json!({
        "id": id, "kind": "rule", "area": "global", "standing": false,
        "summary": "A rule.", "source": {"date": "2026-10-09", "by": "review"},
    });
    if let (Value::Object(map), Value::Object(extra)) = (&mut entry, overrides) {
        map.extend(extra);
    }
    entry
}

/// A `check` object `load_base` classifies as a valid check.
pub(super) fn valid_check() -> Value {
    json!({"type": "report-field", "level": "fail", "if": "**", "field": "live_run"})
}
