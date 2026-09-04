//! The knowledge base render and check draw from: `knowledge/schema.json`,
//! `knowledge/areas.json`, every topic file under `knowledge/`, and the
//! entries they declare.
//!
//! Ports the slice of `tools/kb.mjs`'s `loadBase` the render and check
//! surfaces need (docs/specs/2026-09-04-batch-15-tier2-spec.md §3, HR-054
//! tasks 3 and 4). `load_base` mirrors `loadBase`'s own tolerance exactly:
//! a knowledge file must exist and parse as JSON, but its *shape* is never
//! enforced here -- neither a topic file whose `entries` array holds a
//! malformed item (missing `id`, wrong type, `null`), nor `areas.json`
//! being something other than an object of `{paths: [...]}` values (fix
//! round 1, finding 1); both still load. `checkBase` (`check.rs`) is the
//! only surface that reports shape problems, as CHECK FINDINGS (exit 1),
//! never as a load failure (exit 2). `schema.json`'s content is now used
//! (`checkBase` validates every knowledge file against it); `renderAll`
//! still never reads it, but `load_base` requires it to parse, exactly
//! like `tools/kb.mjs`'s `loadBase`, which calls
//! `readJson(join(dir, 'schema.json'))` unconditionally.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::glob::{GlobError, compile};

/// One area's glob membership, as declared under its key in
/// `knowledge/areas.json`. A path with no matching glob in any area still
/// belongs to `global`, which conventionally declares no paths of its own
/// (`renderAll` and `areaFiles` both special-case it).
#[derive(Deserialize)]
pub(crate) struct AreaDef {
    #[serde(default)]
    pub paths: Vec<String>,
}

/// One knowledge entry's render-relevant fields, extracted leniently from
/// its topic file's raw JSON (see the module doc): a field absent or of the
/// wrong JSON type falls back to its default rather than failing the load,
/// the same tolerance `tools/kb.mjs`'s `loadBase` gives every field it
/// spreads onto an indexed entry (`{...item, topic: topic.name}`, no shape
/// check beyond `typeof item.id === 'string'`).
#[derive(Clone)]
pub(crate) struct Entry {
    pub id: String,
    pub kind: String,
    pub area: String,
    pub standing: bool,
    pub summary: String,
}

/// One topic file's render-relevant metadata: its file slug (`name`),
/// display `title`, and how many entries it declares (`entry_count`,
/// listed in the knowledge skill's `## Topics` section regardless of
/// which of those entries are the standing or area-file kinds, and
/// regardless of whether every entry is itself well-formed -- it is the
/// raw `entries` array's length, matching `tools/kb.mjs`'s
/// `topicEntryCount`).
pub(crate) struct TopicMeta {
    pub name: String,
    pub title: String,
    pub entry_count: usize,
}

/// The loaded knowledge base: every area (in `areas.json`'s declared
/// order — `renderAll`'s area-file iteration and the `render`/`render
/// --check` stdout it drives depend on that order surviving the load), every
/// entry indexed by id (the first occurrence across topic files wins, the
/// same as `tools/kb.mjs`'s `loadBase`), each topic's render metadata, and
/// the raw JSON `checkBase` (`check.rs`) validates: the parsed
/// `schema.json` content, `areas.json`'s content before it is narrowed to
/// `AreaDef`, and every topic file's full parsed content paired with its
/// repo-relative path and file-stem name.
pub(crate) struct Base {
    pub root: PathBuf,
    pub areas: Vec<(String, AreaDef)>,
    pub entries: HashMap<String, Entry>,
    pub topics: Vec<TopicMeta>,
    pub schema: Value,
    pub areas_raw: Value,
    pub topic_files: Vec<(String, String, Value)>,
}

/// A failure while loading the knowledge base from disk: an unreadable
/// file, JSON that failed to parse, or an area glob that failed to
/// compile, named with the offending path (and, for `Glob`, the offending
/// glob itself, from the wrapped `GlobError`).
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
    Glob {
        path: PathBuf,
        source: GlobError,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            LoadError::Json { path, source } => {
                write!(f, "{}: invalid JSON ({source})", path.display())
            }
            LoadError::Glob { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {}

/// Reads `path` and parses it as JSON, naming the file in any read or
/// parse failure -- the one place `load_base` and `load_areas` both read a
/// knowledge file from disk, matching `tools/lib/json-store.mjs`'s
/// `readJson` (a missing or unreadable file and invalid JSON are both
/// possible on the JS side too; `checkBase`'s own shape validation runs
/// only after this succeeds).
fn read_json_value(path: &Path) -> Result<Value, LoadError> {
    let text = fs::read_to_string(path).map_err(|source| LoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| LoadError::Json {
        path: path.to_path_buf(),
        source,
    })
}

/// Extracts a string field from a raw JSON object leniently: absent, or
/// present with a non-string value, both fall back to `""` rather than
/// failing -- the same tolerance `tools/kb.mjs`'s plain property access
/// gives a field it never destructures with a required shape.
fn string_field(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Loads every knowledge topic file under `root`, indexing entries by id.
/// Topic files are `knowledge/*.json` other than `schema.json` and
/// `areas.json`, read in filename order. `schema.json` must exist and
/// parse as JSON (see the module doc).
pub(crate) fn load_base(root: &Path) -> Result<Base, LoadError> {
    let dir = root.join("knowledge");
    let schema = read_json_value(&dir.join("schema.json"))?;
    let areas_path = dir.join("areas.json");
    let areas_raw = read_json_value(&areas_path)?;
    let areas = build_areas(&areas_raw, &areas_path)?;

    let mut names: Vec<String> = fs::read_dir(&dir)
        .map_err(|source| LoadError::Io {
            path: dir.clone(),
            source,
        })?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json") && name != "schema.json" && name != "areas.json")
        .collect();
    names.sort();

    let mut entries: HashMap<String, Entry> = HashMap::new();
    let mut topics = Vec::with_capacity(names.len());
    let mut topic_files = Vec::with_capacity(names.len());
    for name in names {
        let path = dir.join(&name);
        let content = read_json_value(&path)?;
        let title = string_field(&content, "title");
        let raw_entries: Vec<Value> = content
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for item in &raw_entries {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            entries.entry(id.to_string()).or_insert_with(|| Entry {
                id: id.to_string(),
                kind: string_field(item, "kind"),
                area: string_field(item, "area"),
                standing: item
                    .get("standing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                summary: string_field(item, "summary"),
            });
        }
        let name_stem = name.strip_suffix(".json").unwrap_or(&name).to_string();
        topics.push(TopicMeta {
            name: name_stem.clone(),
            title,
            entry_count: raw_entries.len(),
        });
        topic_files.push((format!("knowledge/{name}"), name_stem, content));
    }

    Ok(Base {
        root: root.to_path_buf(),
        areas,
        entries,
        topics,
        schema,
        areas_raw,
        topic_files,
    })
}

/// Builds the typed area list from `areas.json`'s already-parsed raw
/// content, preserving its declared key order (the `serde_json::Map`
/// `preserve_order` feature backs `raw` with an `IndexMap` instead of the
/// default `BTreeMap`, which would silently alphabetize the areas and
/// reorder every consumer of this list). Never fails on a malformed shape
/// -- neither `raw` being something other than an object, nor one area's
/// value failing to deserialize into `AreaDef` -- because `tools/kb.mjs`'s
/// `loadBase` does not either: a shape problem is `checkBase`'s schema
/// validator's finding to report (exit 1), not a load failure (exit 2)
/// (fix round 1, finding 1; the report's `implemented` claimed this
/// invariant for topic entries only, task-4-review.json issue 1 caught
/// that it did not yet hold for `areas.json`). A malformed area is simply
/// excluded from the returned list -- `check.rs`'s `check_base` runs its
/// schema validation directly against `raw` (`base.areas_raw`), not
/// against this typed list, so the exclusion never hides a finding; it
/// only means `render_all` cannot render that one area, which is moot
/// whenever the shape is bad, since `check_base` never reaches `render_all`
/// on an unclean first stage, and `render`'s own contract for a malformed
/// `areas.json` was never pinned by a test.
///
/// The one load failure this still raises is the sanctioned one (fix
/// round 1, finding 2 and finding 4): a glob that fails to compile, for
/// every area whose shape DID type-check. `tools/kb.mjs`'s own
/// `loadAreas` performs no such check, since `matchesGlob`/`globToRegExp`
/// never raise; this is a deliberate strengthening the ruling's "malformed
/// globs are named errors" line sanctions, not a parity claim. `path`
/// names `areas.json` in that error.
fn build_areas(raw: &Value, path: &Path) -> Result<Vec<(String, AreaDef)>, LoadError> {
    let Some(map) = raw.as_object() else {
        return Ok(Vec::new());
    };
    let mut areas = Vec::with_capacity(map.len());
    for (name, value) in map {
        let Ok(def) = serde_json::from_value::<AreaDef>(value.clone()) else {
            continue;
        };
        for glob in &def.paths {
            compile(glob).map_err(|source| LoadError::Glob {
                path: path.to_path_buf(),
                source,
            })?;
        }
        areas.push((name.clone(), def));
    }
    Ok(areas)
}

/// Reads and parses `knowledge/areas.json` at `path`, then builds its typed
/// area list (see `build_areas`). Kept as its own entry point -- distinct
/// from `load_base`, which also needs the raw parsed value `build_areas`
/// consumes -- for the matcher's own tests (`glob.rs`), which need only the
/// typed list; production code reaches `build_areas` through `load_base`
/// instead, so this has no production caller (`#[allow(dead_code)]`,
/// matching `glob.rs`'s own precedent for test-only entry points).
#[allow(dead_code)]
pub(crate) fn load_areas(path: &Path) -> Result<Vec<(String, AreaDef)>, LoadError> {
    let raw = read_json_value(path)?;
    build_areas(&raw, path)
}
