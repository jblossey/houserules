//! Test fixtures shared by the `rules::` modules that read a knowledge
//! base: a loadable base on disk, an entry to put in it, and a valid
//! `check`. `stats.rs` and `proposals.rs` use them; a module that needs a
//! base in a test imports from here and from no sibling.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};
use tempfile::TempDir;

use crate::packs::pack_sha256;

use super::model::{Base, load_base};

/// A loadable knowledge base on disk holding `entries` in one topic file.
/// `schema.json` and `areas.json` are empty objects: `load_base` only
/// needs them to parse. The returned guard removes the directory on drop
/// (`houserules.tests-clean-scratch-dirs`).
pub(super) fn base_with(entries: &[Value]) -> (TempDir, Base) {
    base_with_packs(entries, &[], &[])
}

/// A loadable knowledge base on disk like `base_with`, plus one pack copy
/// for each `(name, entries)` pair, and an exclusion in `overrides.json`
/// for each id of `excluded`. A pack serves one topic of its own name; its
/// entries get the `body` and `tags` that the pack schema requires when
/// they lack them.
pub(super) fn base_with_packs(
    entries: &[Value],
    packs: &[(&str, &[Value])],
    excluded: &[&str],
) -> (TempDir, Base) {
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
    for (name, pack_entries) in packs {
        write_pack_copy(dir.path(), name, &[name], pack_entries);
    }
    if !excluded.is_empty() {
        let exclusions: Vec<Value> = excluded.iter().map(|id| json!({"id": id})).collect();
        fs::write(
            knowledge.join("overrides.json"),
            serde_json::to_string(&json!({
                "schema": "houserules/overrides@1.0.0", "exclusions": exclusions,
            }))
            .expect("serialize overrides"),
        )
        .expect("write overrides.json");
    }
    let base = load_base(dir.path()).expect("load base");
    (dir, base)
}

/// Writes the pack copy `knowledge/packs/<name>.json` under `root`: the
/// pack serves each of `topics` and declares no area. Each entry gets the
/// `body` and `tags` that the pack schema requires when it lacks them.
pub(super) fn write_pack_copy(root: &Path, name: &str, topics: &[&str], entries: &[Value]) {
    let served: Map<String, Value> = topics
        .iter()
        .map(|topic| (topic.to_string(), json!({"title": "A pack."})))
        .collect();
    let complete: Vec<Value> = entries.iter().map(complete_pack_entry).collect();
    let packs_dir = root.join("knowledge/packs");
    fs::create_dir_all(&packs_dir).expect("create knowledge/packs/");
    fs::write(
        packs_dir.join(format!("{name}.json")),
        serde_json::to_string(&json!({
            "name": name, "version": "1.0.0", "schema": "houserules/pack@1.0.0",
            "topics": served, "areas": {}, "entries": complete,
        }))
        .expect("serialize a pack"),
    )
    .expect("write a pack copy");
}

/// Writes `.houserules.json` under `root` with one pin for each copy under
/// `knowledge/packs/`: the `sha256` of the parsed copy, as `pack add` pins
/// it.
pub(super) fn pin_every_copy(root: &Path) {
    let mut pins = Map::new();
    let mut files: Vec<_> = fs::read_dir(root.join("knowledge/packs"))
        .expect("read knowledge/packs")
        .map(|file| file.expect("a directory entry").path())
        .collect();
    files.sort();
    for file in files {
        let raw: Value =
            serde_json::from_str(&fs::read_to_string(&file).expect("read a pack copy"))
                .expect("parse a pack copy");
        pins.insert(
            raw["name"].as_str().expect("a pack name").to_string(),
            json!({
                "version": raw["version"], "source": "kit",
                "sha256": pack_sha256(&raw), "addedAreas": [],
            }),
        );
    }
    fs::write(
        root.join(".houserules.json"),
        serde_json::to_string(&json!({"packs": pins})).expect("serialize the pins"),
    )
    .expect("write .houserules.json");
}

/// `entry` with the `body` and `tags` that the pack schema requires,
/// unless it carries them.
fn complete_pack_entry(entry: &Value) -> Value {
    let mut complete = entry.clone();
    if let Value::Object(map) = &mut complete {
        map.entry("body").or_insert_with(|| json!(["A body."]));
        map.entry("tags").or_insert_with(|| json!([]));
    }
    complete
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
