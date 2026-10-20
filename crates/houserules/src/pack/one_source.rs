//! One id, one source (spec L8 and L9).
//!
//! A local entry whose id an installed pack ships is the state this step
//! converts, on every `pack add`, `pack upgrade`, and `update`:
//!
//! - a local entry that equals the pack entry goes;
//! - a local entry that differs becomes an override that carries each
//!   differing field, and a field that the local entry lacks is set to
//!   `null`;
//! - when `knowledge/overrides.json` already holds an override or an
//!   exclusion of the id, the local entry replaces it, and one printed line
//!   names each item it replaced;
//! - an archived local entry becomes an override with `status: retired`
//!   and its other differing fields, and leaves the archive, so the id is
//!   never active and archived at once;
//! - a topic file goes in the step that empties it: it held at least one
//!   entry, and an installed pack ships every id in it (L9). A file with
//!   `"entries": []` stays. An archive file that the step empties goes too.
//!
//! [`plan`] decides everything and writes nothing, so a caller can refuse
//! before it writes any other file; [`Plan::apply`] writes. [`one_source`]
//! is both, for a caller that has installed its packs already.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::archive::{list_archive_json_files, read_json, write_json};
use crate::packs::{OVERRIDES_SCHEMA, PackCopy};
use crate::rules::{Base, validate};

/// The `schema` of an `overrides.json` that this step creates.
const OVERRIDES_SCHEMA_ID: &str = "houserules/overrides@1.0.0";

/// What the step decided: the files to write, the files to remove, and the
/// lines to print.
pub(super) struct Plan {
    writes: Vec<(PathBuf, Value)>,
    removals: Vec<PathBuf>,
    lines: Vec<String>,
}

impl Plan {
    /// The lines to print, in the order the step decided them.
    pub(super) fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Writes each file in emit form, then removes each emptied file.
    pub(super) fn apply(&self) -> Result<(), String> {
        for (path, value) in &self.writes {
            write_json(path, value)?;
        }
        for path in &self.removals {
            fs::remove_file(path).map_err(|error| format!("{}: {error}", path.display()))?;
        }
        Ok(())
    }
}

/// Applies the step to the base `base` read from `root`: plans it, writes
/// it, and returns the lines to print. `base.packs` holds the installed
/// packs.
pub(crate) fn one_source(root: &Path, base: &Base) -> Result<Vec<String>, String> {
    let plan = plan(root, &base.topic_files, &base.packs, &base.overrides_raw)?;
    plan.apply()?;
    Ok(plan.lines)
}

/// The id of each entry that an installed pack ships, with the pack and the
/// entry. The first pack in name order keeps an id that two packs ship.
pub(crate) fn shipped_entries(packs: &[PackCopy]) -> BTreeMap<&str, (&str, &Value)> {
    let mut by_name: Vec<&PackCopy> = packs.iter().collect();
    by_name.sort_by(|left, right| left.name.cmp(&right.name));
    let mut shipped = BTreeMap::new();
    for pack in by_name {
        for entry in &pack.entries {
            if let Some(id) = entry.get("id").and_then(Value::as_str) {
                shipped.entry(id).or_insert((pack.name.as_str(), entry));
            }
        }
    }
    shipped
}

/// The id of `item`, the pack that ships that id, and the pack's entry, or
/// `None` when `item` has no string id or no installed pack ships it.
fn find_shipped<'item, 'pack>(
    item: &'item Value,
    shipped: &BTreeMap<&'pack str, (&'pack str, &'pack Value)>,
) -> Option<(&'item str, &'pack str, &'pack Value)> {
    let id = item.get("id")?.as_str()?;
    let (pack, entry) = shipped.get(id)?;
    Some((id, pack, entry))
}

/// The fields of `local` that differ from `shipped`, as an override carries
/// them: a field that `local` has and `shipped` lacks or holds another
/// value for takes the local value, in the order of `local`; a field that
/// only `shipped` has is set to `null`, in the order of `shipped`. `id` is
/// never a field of an override.
pub(crate) fn differing_fields(local: &Value, shipped: &Value) -> Map<String, Value> {
    let mut fields = Map::new();
    let (Some(local), Some(shipped)) = (local.as_object(), shipped.as_object()) else {
        return fields;
    };
    for (key, value) in local {
        if key != "id" && shipped.get(key) != Some(value) {
            fields.insert(key.clone(), value.clone());
        }
    }
    for key in shipped.keys() {
        if key != "id" && !local.contains_key(key) {
            fields.insert(key.clone(), Value::Null);
        }
    }
    fields
}

/// `knowledge/overrides.json` as a step edits it: the one-id-one-source step
/// here, and the conversion of a 1.x install (`install::conversion`).
pub(crate) struct OverridesEdit {
    doc: Value,
    changed: bool,
}

impl OverridesEdit {
    /// Starts from the parsed file, or from a new document when the file
    /// does not exist (`raw` is `null`).
    pub(crate) fn new(raw: &Value) -> Self {
        Self {
            doc: if raw.is_null() {
                json!({"schema": OVERRIDES_SCHEMA_ID, "overrides": [], "exclusions": []})
            } else {
                raw.clone()
            },
            changed: false,
        }
    }

    /// Removes every override and every exclusion of `id`. Returns one line
    /// for each item removed.
    pub(crate) fn replace_items_of(&mut self, id: &str) -> Vec<String> {
        let mut lines = Vec::new();
        for (key, label) in [("overrides", "override"), ("exclusions", "exclusion")] {
            let Some(items) = self.doc.get_mut(key).and_then(Value::as_array_mut) else {
                continue;
            };
            let before = items.len();
            items.retain(|item| item.get("id").and_then(Value::as_str) != Some(id));
            for _ in items.len()..before {
                lines.push(format!(
                    "replaced the {label} of {id} in knowledge/overrides.json with the local entry"
                ));
            }
            if items.len() != before {
                self.changed = true;
            }
        }
        lines
    }

    /// Appends an override of `id` that sets `fields`.
    pub(crate) fn add_override(&mut self, id: &str, fields: Map<String, Value>) {
        self.push("overrides", json!({"id": id, "fields": fields}));
    }

    /// Appends an exclusion of `id`.
    pub(crate) fn add_exclusion(&mut self, id: &str) {
        self.push("exclusions", json!({"id": id}));
    }

    /// Appends `item` to the array under `key`, which a new array holds when
    /// the document has none.
    fn push(&mut self, key: &str, item: Value) {
        match self.doc.get_mut(key).and_then(Value::as_array_mut) {
            Some(items) => items.push(item),
            None => self.doc[key] = json!([item]),
        }
        self.changed = true;
    }

    /// The document when an edit changed it, else `None`.
    pub(crate) fn into_changed(self) -> Option<Value> {
        self.changed.then_some(self.doc)
    }
}

/// The lines in which `doc` breaks the embedded overrides schema; none when
/// it holds.
pub(crate) fn overrides_errors(doc: &Value) -> Vec<String> {
    let schema: Value = serde_json::from_str(OVERRIDES_SCHEMA)
        .expect("the embedded overrides schema is valid JSON");
    let mut errors = Vec::new();
    validate(
        doc,
        &schema,
        "knowledge/overrides.json",
        &mut errors,
        &schema,
    );
    errors
}

/// A local entry that an installed pack ships.
struct Moved<'a> {
    id: &'a str,
    pack: &'a str,
    fields: Map<String, Value>,
}

/// The entries of `content["entries"]`, or none when it holds no array.
fn entries_of(content: &Value) -> &[Value] {
    content
        .get("entries")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Decides the step for the local topic files `topic_files` (each a
/// repository-relative path, the topic name, and the parsed file, as
/// `Base::topic_files` holds them), the archive files under `root`, and the
/// parsed `overrides.json` (`null` when absent), against the installed
/// packs `packs`. Writes nothing.
///
/// Refuses, with one line and no change, an id that is in two local topic
/// files, an id that is active in a topic file and archived at once, and a
/// result that `overrides.json` would not accept.
pub(super) fn plan(
    root: &Path,
    topic_files: &[(String, String, Value)],
    packs: &[PackCopy],
    overrides: &Value,
) -> Result<Plan, String> {
    let shipped = shipped_entries(packs);
    let knowledge = root.join("knowledge");
    let mut edit = OverridesEdit::new(overrides);
    let mut writes = Vec::new();
    let mut removals = Vec::new();
    let mut lines = Vec::new();
    let mut active_in: BTreeMap<&str, &str> = BTreeMap::new();

    for (file, name, content) in topic_files {
        let items = entries_of(content);
        let mut kept = Vec::new();
        let mut moved = Vec::new();
        for item in items {
            match find_shipped(item, &shipped) {
                Some((id, pack, entry)) => {
                    if let Some(first) = active_in.insert(id, file) {
                        return Err(format!(
                            "{id} is in {first} and {file}; keep one entry, then run the command again"
                        ));
                    }
                    moved.push(Moved {
                        id,
                        pack,
                        fields: differing_fields(item, entry),
                    });
                }
                None => kept.push(item.clone()),
            }
        }
        if moved.is_empty() {
            continue;
        }
        for entry in moved {
            lines.push(if entry.fields.is_empty() {
                format!(
                    "dropped {} from {file}: pack {} ships the same entry",
                    entry.id, entry.pack
                )
            } else {
                let names: Vec<&str> = entry.fields.keys().map(String::as_str).collect();
                format!(
                    "moved {} from {file} to knowledge/overrides.json: pack {} ships it, and the override sets {}",
                    entry.id,
                    entry.pack,
                    names.join(", ")
                )
            });
            lines.extend(edit.replace_items_of(entry.id));
            if !entry.fields.is_empty() {
                edit.add_override(entry.id, entry.fields);
            }
        }
        let path = knowledge.join(format!("{name}.json"));
        if kept.is_empty() {
            lines.push(format!(
                "removed {file}: installed packs ship every entry in it"
            ));
            removals.push(path);
        } else {
            let mut rewritten = content.clone();
            rewritten["entries"] = Value::Array(kept);
            writes.push((path, rewritten));
        }
    }

    let archive = knowledge.join("archive");
    for name in list_archive_json_files(&archive)?.unwrap_or_default() {
        let file = format!("knowledge/archive/{name}");
        let path = archive.join(&name);
        let content = read_json(&path)?;
        let mut kept = Vec::new();
        let mut moved = Vec::new();
        for item in entries_of(&content) {
            match find_shipped(item, &shipped) {
                Some((id, pack, entry)) => {
                    if let Some(active) = active_in.get(id) {
                        return Err(format!(
                            "{id} is active in {active} and archived in {file}; remove one of the two, then run the command again"
                        ));
                    }
                    let mut fields = Map::new();
                    fields.insert("status".to_string(), json!("retired"));
                    for (key, value) in differing_fields(item, entry) {
                        if key != "status" {
                            fields.insert(key, value);
                        }
                    }
                    moved.push(Moved { id, pack, fields });
                }
                None => kept.push(item.clone()),
            }
        }
        if moved.is_empty() {
            continue;
        }
        for entry in moved {
            lines.push(format!(
                "retired {} from {file} into knowledge/overrides.json: pack {} ships it",
                entry.id, entry.pack
            ));
            lines.extend(edit.replace_items_of(entry.id));
            edit.add_override(entry.id, entry.fields);
        }
        if kept.is_empty() {
            lines.push(format!("removed {file}: it holds no entry now"));
            removals.push(path);
        } else {
            let mut rewritten = content.clone();
            rewritten["entries"] = Value::Array(kept);
            writes.push((path, rewritten));
        }
    }

    if let Some(doc) = edit.into_changed() {
        let errors = overrides_errors(&doc);
        if !errors.is_empty() {
            return Err(format!(
                "the one-id-one-source step cannot write knowledge/overrides.json: {}",
                errors.join("; ")
            ));
        }
        writes.push((knowledge.join("overrides.json"), doc));
    }
    Ok(Plan {
        writes,
        removals,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::rules::load_base;

    fn entry(id: &str, summary: &str) -> Value {
        json!({
            "id": id, "kind": "rule", "area": "global", "standing": true,
            "summary": summary, "body": ["Body."], "tags": [],
            "source": {"date": "2026-10-19", "by": "controller"},
        })
    }

    /// An entry that equals its pack entry carries no field. A field with
    /// another value, a field only the local entry has, and a field only the
    /// pack entry has (set to `null`) are the fields of an override, in the
    /// order of the local entry and then of the pack entry. `id` is never
    /// one.
    #[test]
    fn differing_fields_name_each_difference_in_order() {
        let shipped = entry("team.a", "Shipped");
        assert_eq!(differing_fields(&shipped, &shipped), Map::new());

        let mut local = shipped.clone();
        local["summary"] = json!("Local");
        local["id"] = json!("other.id");
        local["status"] = json!("retired");
        local
            .as_object_mut()
            .expect("an object")
            .shift_remove("standing");
        let fields = differing_fields(&local, &shipped);

        assert_eq!(
            Value::Object(fields.clone()),
            json!({"summary": "Local", "status": "retired", "standing": null})
        );
        let order: Vec<&String> = fields.keys().collect();
        assert_eq!(order, ["summary", "status", "standing"]);
    }

    /// Two equal values with another key order are equal: a hand reformat
    /// of an entry is no difference.
    #[test]
    fn a_reordered_entry_is_no_difference() {
        let shipped = entry("team.a", "Shipped");
        let mut reordered = Map::new();
        for key in [
            "source", "tags", "body", "summary", "standing", "area", "kind", "id",
        ] {
            reordered.insert(key.to_string(), shipped[key].clone());
        }

        assert_eq!(
            differing_fields(&Value::Object(reordered), &shipped),
            Map::new()
        );
    }

    /// The first pack in name order keeps an id that two packs ship.
    #[test]
    fn the_first_pack_in_name_order_keeps_a_shared_id() {
        let pack = |name: &str, summary: &str| PackCopy {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            schema: "houserules/pack@1.0.0".to_string(),
            topics: vec![],
            areas: vec![],
            entries: vec![entry("x.same", summary)],
            raw: Value::Null,
        };
        let packs = [pack("b", "From b"), pack("a", "From a")];

        let shipped = shipped_entries(&packs);

        assert_eq!(shipped["x.same"].0, "a");
        assert_eq!(shipped["x.same"].1["summary"], json!("From a"));
    }

    /// `one_source` plans and writes: the equal entry goes, its emptied
    /// file goes, and the lines come back.
    #[test]
    fn one_source_writes_the_plan_and_returns_its_lines() {
        let root = tempfile::tempdir().expect("tempdir");
        let knowledge = root.path().join("knowledge");
        fs::create_dir_all(knowledge.join("packs")).expect("create packs/");
        fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
        fs::write(knowledge.join("areas.json"), "{}").expect("write areas.json");
        let shipped = entry("team.alpha", "A");
        write_json(
            &knowledge.join("packs").join("team.json"),
            &json!({
                "name": "team", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {"team": {"title": "Team"}},
                "areas": {"global": {"paths": []}},
                "entries": [shipped.clone()],
            }),
        )
        .expect("write the pack copy");
        write_json(
            &knowledge.join("team.json"),
            &json!({"topic": "team", "title": "Team", "entries": [shipped]}),
        )
        .expect("write the topic file");
        let base = load_base(root.path()).expect("load the base");

        let lines = one_source(root.path(), &base).expect("the step");

        assert_eq!(
            lines,
            [
                "dropped team.alpha from knowledge/team.json: pack team ships the same entry",
                "removed knowledge/team.json: installed packs ship every entry in it",
            ]
        );
        assert!(!knowledge.join("team.json").exists());
        assert!(!knowledge.join("overrides.json").exists());
    }
}
