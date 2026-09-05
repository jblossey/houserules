//! Loads `backlog/*.json` into memory -- `tools/backlog.mjs`'s
//! `loadBacklog`, ported (batch 17 T2).
//!
//! Every file loads as raw `serde_json::Value` (the crate-wide
//! `preserve_order` feature backs each object with an insertion-order map,
//! so a value read here and later re-serialized keeps its on-disk key
//! order), never through `backlog::model`'s typed schema structs. This is
//! a deliberate choice, not an oversight -- see this crate's `backlog`
//! module doc for the two concrete reasons: `get`/`set` must reproduce
//! each item's *own* on-disk key order byte-for-byte, which the struct's
//! fixed declaration order cannot (a real backlog item's fields are not
//! declaration-ordered -- verified against this repository's own
//! `HR-052`), and `check-backlog` must tolerate a malformed item the way
//! `rules::model`'s own load does for knowledge entries (`load_base`'s
//! module doc), reporting it as a check finding rather than refusing to
//! load.
//!
//! `load_backlog` mirrors `loadBacklog`'s own tolerance exactly: a
//! malformed item (not an object, or with a non-string `id`) is silently
//! excluded from `LoadedBacklog::items`, the same as JS's `item &&
//! typeof item.id === 'string'` guard; a section file whose `items` field
//! is missing or not an array contributes no items, matching `loadBacklog`'s
//! `Array.isArray(section.items) ? section.items : []`. `check_backlog`
//! (`commands.rs`) is the surface that reports these shapes as findings,
//! using each section's raw, unfiltered content instead.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// A failure while loading the backlog from disk: an unreadable file, or
/// one that failed to parse as JSON -- `template/tools/lib/json-store.mjs`'s
/// `readJson`, the same two failure shapes `rules::model::LoadError`
/// carries for the knowledge base (backlog has no glob-bearing file, so no
/// third variant). Named with the offending path, matching `readJson`'s own
/// message text.
#[derive(Debug)]
pub(crate) enum LoadError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            LoadError::Json { path, source } => {
                write!(f, "{}: invalid JSON ({source})", path.display())
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// Reads `path` and parses it as JSON, naming the file in either failure --
/// `readJson`, ported (see `LoadError`'s doc).
pub(crate) fn read_json_value(path: &Path) -> Result<Value, LoadError> {
    let text = fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| LoadError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// One `backlog/items/*.json` file: its repo-relative path, file-stem
/// name, and full raw content (`$schema`/`section`/`heading`/`title`/
/// `spec`/`items`) -- `loadBacklog`'s per-file spread (`{file, name,
/// ...readJson(...)}`), read once and shared by `check_backlog` (schema
/// validation over the raw content) and `load_backlog`'s own item-indexing
/// pass below.
#[derive(Debug)]
pub(crate) struct Section {
    pub file: String,
    pub name: String,
    pub content: Value,
}

/// The loaded backlog: every file's raw content, the section list, and
/// every item indexed by id -- `loadBacklog`'s return value. An item's
/// `Value` already carries the `section`/`file` fields `loadBacklog`
/// attaches (`{...item, section: section.section, file: section.file}`),
/// appended after its own on-disk keys, so `commands::cmd_get` can return
/// it verbatim and `commands::cmd_set` can read `file` back off it to find
/// which items file to rewrite.
#[derive(Debug)]
pub(crate) struct LoadedBacklog {
    pub root: PathBuf,
    pub schema: Value,
    pub amendments: Value,
    pub batches: Value,
    pub decisions: Value,
    pub parked: Value,
    pub sections: Vec<Section>,
    pub items: Vec<(String, Value)>,
}

/// Loads every backlog file under `root`, indexing items by id with their
/// section and file -- `loadBacklog`, ported (see the module doc for the
/// raw-`Value` design and its tolerance for malformed items).
pub(crate) fn load_backlog(root: &Path) -> Result<LoadedBacklog, LoadError> {
    let dir = root.join("backlog");
    let schema = read_json_value(&dir.join("schema.json"))?;
    let amendments = read_json_value(&dir.join("amendments.json"))?;
    let batches = read_json_value(&dir.join("batches.json"))?;
    let decisions = read_json_value(&dir.join("decisions.json"))?;
    let parked = read_json_value(&dir.join("parked.json"))?;

    let items_dir = dir.join("items");
    let mut names: Vec<String> = fs::read_dir(&items_dir)
        .map_err(|source| LoadError::Io {
            path: items_dir.clone(),
            source,
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();

    let mut sections = Vec::with_capacity(names.len());
    let mut items: Vec<(String, Value)> = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    for name in names {
        let file = format!("backlog/items/{name}");
        let content = read_json_value(&items_dir.join(&name))?;
        let section_field = content
            .get("section")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if let Some(raw_items) = content.get("items").and_then(Value::as_array) {
            for item in raw_items {
                let Some(id) = item.get("id").and_then(Value::as_str) else {
                    continue;
                };
                if !seen_ids.insert(id.to_string()) {
                    continue;
                }
                let mut augmented = item.clone();
                if let Value::Object(map) = &mut augmented {
                    map.insert("section".to_string(), Value::String(section_field.clone()));
                    map.insert("file".to_string(), Value::String(file.clone()));
                }
                items.push((id.to_string(), augmented));
            }
        }
        let name_stem = name.strip_suffix(".json").unwrap_or(&name).to_string();
        sections.push(Section {
            file,
            name: name_stem,
            content,
        });
    }

    Ok(LoadedBacklog {
        root: root.to_path_buf(),
        schema,
        amendments,
        batches,
        decisions,
        parked,
        sections,
        items,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::super::test_support::{default_items, item, make_repo, write};
    use super::*;

    /// tests/backlog.test.mjs, describe('loadBacklog and checkBacklog'):
    /// "loads every file and indexes items with their section".
    #[test]
    fn loads_every_file_and_indexes_items_with_their_section() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        let (_, wi002) = b.items.iter().find(|(id, _)| id == "WI-002").unwrap();
        assert_eq!(wi002.get("section").and_then(Value::as_str), Some("E01"));
        assert_eq!(
            b.sections
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            vec!["E01"]
        );
    }

    /// tests/backlog.test.mjs: "treats a section with no items array as
    /// having none" -- `loadBacklog`'s `Array.isArray(section.items) ?
    /// section.items : []` false branch.
    #[test]
    fn treats_a_section_with_no_items_array_as_having_none() {
        let dir = make_repo(default_items());
        write(
            dir.path(),
            "backlog/items/E02.json",
            &json!({"section": "E02", "heading": "h", "title": "t", "spec": ""}),
        );
        let b = load_backlog(dir.path()).expect("loads");
        let e02 = b.sections.iter().find(|s| s.name == "E02").unwrap();
        assert!(e02.content.get("items").is_none());
        assert!(b.items.iter().any(|(id, _)| id == "WI-001"));
    }

    /// A malformed item (non-string id) is excluded from the index, but its
    /// section's raw content still carries it whole, for `check_backlog` to
    /// report -- `loadBacklog`'s `typeof item.id === 'string'` guard.
    #[test]
    fn excludes_a_malformed_item_from_the_index_but_keeps_it_in_the_raw_section() {
        let dir = make_repo(vec![item(json!({})), json!({"id": 5, "type": "feat"})]);
        let b = load_backlog(dir.path()).expect("loads");
        assert_eq!(b.items.len(), 1);
        let section = &b.sections[0];
        assert_eq!(
            section
                .content
                .get("items")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2)
        );
    }

    /// A duplicate id across items keeps only the first occurrence in the
    /// index -- `loadBacklog`'s `!items.has(item.id)` guard.
    #[test]
    fn keeps_only_the_first_occurrence_of_a_duplicate_id() {
        let dir = make_repo(vec![
            item(json!({"title": "first"})),
            item(json!({"title": "second"})),
        ]);
        let b = load_backlog(dir.path()).expect("loads");
        assert_eq!(b.items.len(), 1);
        assert_eq!(
            b.items[0].1.get("title").and_then(Value::as_str),
            Some("first")
        );
    }

    /// A missing `backlog/schema.json` is a load failure naming the file --
    /// `readJson`'s unreadable-file message, matching `rules::model`'s
    /// `LoadError::Io` for the knowledge base.
    #[test]
    fn a_missing_schema_file_is_a_load_error_naming_the_path() {
        let dir = make_repo(default_items());
        fs::remove_file(dir.path().join("backlog/schema.json")).unwrap();
        let error = load_backlog(dir.path()).expect_err("missing schema.json");
        assert!(error.to_string().contains("schema.json"), "{error}");
    }

    /// Invalid JSON in an items file is a load failure naming the file --
    /// `readJson`'s parse-failure message.
    #[test]
    fn invalid_json_in_an_items_file_is_a_load_error() {
        let dir = make_repo(default_items());
        fs::write(dir.path().join("backlog/items/E01.json"), "{").unwrap();
        let error = load_backlog(dir.path()).expect_err("invalid JSON");
        assert!(
            error.to_string().contains("E01.json: invalid JSON"),
            "{error}"
        );
    }
}
