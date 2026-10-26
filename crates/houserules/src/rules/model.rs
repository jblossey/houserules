//! The merged knowledge base that render, check, audit, and the read
//! commands draw from.
//!
//! `load_base` reads three kinds of source under `knowledge/` and merges
//! them into one [`Base`]:
//!
//! - the local topic files, `knowledge/<topic>.json`: the project's own
//!   entries;
//! - the pack copies, `knowledge/packs/<pack>.json`: one pinned copy for
//!   each installed pack;
//! - `knowledge/overrides.json`: the fields the project replaces in a pack
//!   entry, and the pack entries it excludes.
//!
//! [`RESERVED`] names the files directly under `knowledge/` that are no
//! topic files. `schema.json` and `areas.json` must exist; `load_base`
//! requires both to parse. `check_base` (`check.rs`) validates every
//! knowledge file against `schema.json`; the renderers never read it.
//!
//! The merge. An override replaces whole fields of one pack entry, and a
//! field set to `null` is removed. An exclusion drops every pack entry
//! with its id. A pack entry whose effective `status` is `retired` or
//! `superseded` (`archive::is_inactive_status`) leaves the active index
//! too: it stays in its copy, and `Base::inactive` keeps it for `get`. An
//! absent status is active. One id in two active sources: the local entry
//! wins over a pack entry, and the first pack in name order wins over
//! another pack; `Base::duplicates` records each pair. An override never
//! changes a local entry. Only a pack entry leaves on its status here:
//! `archive` moves a local entry with an inactive status.
//!
//! Tolerance. A local topic file and `areas.json` must parse as JSON, but
//! the loader never enforces their *shape*: a topic file whose `entries`
//! array holds a malformed item (missing `id`, wrong type, `null`) still
//! loads, and so does an `areas.json` that is no object of `{paths:
//! [...]}` values. `check_base` is the only surface that reports those
//! shape problems, as check findings (exit 1), never as a load failure
//! (exit 2). A pack copy and `overrides.json` differ: each must pass the
//! binary's embedded schema, and a violation is a load error that names
//! the file and the field, because the merge needs the fields it reads.
//! Each pack name has one copy file, `knowledge/packs/<name>.json`: two
//! copies that declare one name, and a copy whose `name` differs from its
//! file stem, are load errors.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Map, Value};

use crate::archive::is_inactive_status;
use crate::packs::{OVERRIDES_SCHEMA, PackCopy, parse_pack, topic_of};

use super::check::{falsy, validate};
use super::check_shape::CheckDef;
use super::glob::{GlobError, compile};

/// The files directly under `knowledge/` that are not topic files.
pub(crate) const RESERVED: [&str; 3] = ["schema.json", "areas.json", "overrides.json"];

/// Where an entry of the merged base comes from.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Source {
    /// A topic file of the project, `knowledge/<topic>.json`.
    Local,
    /// The pack copy of this name, `knowledge/packs/<name>.json`.
    Pack(String),
}

impl Source {
    /// The name `stats` reports for the source: `local`, or the pack name.
    /// A pack is never named `local`, so the two never collide.
    pub(crate) fn name(&self) -> &str {
        match self {
            Source::Local => "local",
            Source::Pack(name) => name,
        }
    }
}

/// One area's glob membership, as declared under its key in
/// `knowledge/areas.json`. A path with no matching glob in any area still
/// belongs to `global`, which conventionally declares no paths of its own
/// (the renderers and `area_files` both special-case it).
#[derive(Deserialize)]
pub(crate) struct AreaDef {
    #[serde(default)]
    pub paths: Vec<String>,
    /// `false` when the area sets `"agentsMd": false`: the area gets no
    /// directory `AGENTS.md` block. An absent key is `true`.
    #[serde(default = "agents_md_default", rename = "agentsMd")]
    pub agents_md: bool,
}

/// The value of `AreaDef::agents_md` for an area without the key.
fn agents_md_default() -> bool {
    true
}

/// One knowledge entry's render-relevant fields, extracted leniently from
/// its raw JSON with any override applied (see the module doc): a field
/// absent or of the wrong JSON type falls back to its default rather than
/// failing the load.
///
/// The three states an entry's raw `check` JSON field takes once
/// "malformed" is told apart from "absent": a plain `Option<CheckDef>`
/// could not distinguish them, so `audit` would treat an invalid `check`
/// (an unknown `type`, a missing required field, or a non-object truthy
/// value) exactly like no `check` at all, silently downgrading a
/// checked, possibly *standing*, entry to a judged row. `audit.rs`'s own
/// row-building is `CheckField`'s one production consumer, and reports
/// `Malformed` as a named, exit-2 error
/// (`houserules.crash-paths-are-named`). `check-knowledge` (`check.rs`)
/// already reports a malformed `check` as its own check finding via the
/// untyped `check_shape` function there, independent of this field, and
/// is unaffected by it either way.
#[derive(Clone)]
pub(crate) enum CheckField {
    /// No `check` key, or one whose value is falsy in the JS sense
    /// (`null`, `false`, `0`, `""`) -- `super::check::falsy`.
    Absent,
    /// A JS-truthy `check` value that is not a JSON object, or an object
    /// that fails to parse as a `CheckDef`.
    Malformed,
    /// A `check` object that parsed cleanly. Boxed: `Absent`/`Malformed`
    /// carry no data, so an unboxed `CheckDef` (`check_shape::CheckDef`'s
    /// several `Option<Glob>`/`Option<String>` fields add up) would make
    /// every `Entry` pay for the largest variant's size regardless of
    /// which one it holds.
    Valid(Box<CheckDef>),
}

#[derive(Clone)]
pub(crate) struct Entry {
    pub id: String,
    pub kind: String,
    pub area: String,
    pub standing: bool,
    pub summary: String,
    /// The entry's `check` field, classified by `CheckField`. See that
    /// type's own doc for the load-time tolerance this buys and why it
    /// still names a production finding rather than reproducing a crash.
    pub check: CheckField,
    /// The source whose entry the base keeps for this id.
    pub source: Source,
}

/// One topic's render-relevant metadata: its slug (`name`), display
/// `title`, and how many entries it holds (`entry_count`, listed in the
/// knowledge skill's `## Topics` section regardless of the kind of those
/// entries and regardless of whether each is well-formed).
///
/// A topic merges every source that serves it. Its title is the local
/// topic file's when that file has one, else the title of the first pack
/// in name order that declares the topic. Its count is the length of the
/// local file's raw `entries` array plus the entries of every pack copy
/// under that topic, an excluded entry and an inactive pack entry left out.
pub(crate) struct TopicMeta {
    pub name: String,
    pub title: String,
    pub entry_count: usize,
}

/// The merged knowledge base (see the module doc for the sources and the
/// merge): every area (in `areas.json`'s declared order -- the renderers'
/// area iteration and the `render`/`render --check` output it drives
/// depend on that order surviving the load), every active entry indexed by
/// id, each topic's render metadata, and the raw JSON `check_base`
/// (`check.rs`) validates.
///
/// `entries` and `raw_entries` hold the same ids: the entries that are
/// active after the merge. An excluded pack entry is in neither; it is in
/// `excluded`. A pack entry whose effective `status` is `retired` or
/// `superseded` is in neither; it is in `inactive`. `raw_entries` holds
/// each entry's ENTIRE effective JSON object -- every field `Entry`
/// keeps, plus every field it drops (`body`, `tags`, `source`, `see`,
/// `verify`, `check`'s own raw shape) -- in the object's own key order,
/// with `topic` appended: the local file's stem for a local entry, the
/// first part of the id for a pack entry. `get`, `for --full`, and
/// `index`'s `--topic`/`--tag` filters (`read.rs`) need this: `Entry`'s
/// reduced field set cannot answer a `--tag` filter, and reconstructing
/// an entry through a typed struct would reorder a hand-edited file's key
/// order on print.
///
/// The rest of the merge is public to the checks that read it:
///
/// - `topic_files`: each local topic file's repo-relative path, file stem,
///   and full parsed content. Pack copies and `overrides.json` are not in
///   it.
/// - `packs`: every pack copy, sorted by pack name.
/// - `overrides_raw`: the parsed `knowledge/overrides.json`, or `null`
///   when the file does not exist.
/// - `excluded`: each excluded pack entry by id, as `get` prints it -- the
///   pack entry, `topic`, then `excluded` holding the reason, or `true`
///   when the exclusion has none.
/// - `inactive`: each pack entry whose effective `status` is `retired` or
///   `superseded`, by id, as `get` prints it -- the effective entry, then
///   `topic`. An inactive pack entry is the entry the base keeps for its
///   id, so it still shadows the same id of a later pack.
/// - `overridden`: for each pack entry with an override, the field names
///   of the override, sorted. An inactive entry is listed too.
/// - `duplicates`: for each id that two sources ship, the id, the source
///   the base keeps, and the source it ignores, sorted. The kept entry may
///   be an inactive pack entry. Two local topic files with one id are not
///   listed: the first file in sorted order wins, and `check_base` reports
///   that pair from `topic_files`.
/// - `archived_ids`: every id in `knowledge/archive/*.json`. An archive
///   file that cannot be read or parsed adds no id; `check-knowledge`
///   reports it.
#[derive(Default)]
pub(crate) struct Base {
    pub root: PathBuf,
    pub areas: Vec<(String, AreaDef)>,
    pub entries: HashMap<String, Entry>,
    pub raw_entries: HashMap<String, Value>,
    pub topics: Vec<TopicMeta>,
    pub schema: Value,
    pub areas_raw: Value,
    pub topic_files: Vec<(String, String, Value)>,
    pub packs: Vec<PackCopy>,
    pub overrides_raw: Value,
    pub excluded: BTreeMap<String, Value>,
    pub inactive: BTreeMap<String, Value>,
    pub overridden: BTreeMap<String, Vec<String>>,
    pub duplicates: Vec<(String, Source, Source)>,
    pub archived_ids: BTreeSet<String>,
}

impl Base {
    /// The globs of the files that the check of the entry `id` reads, when the
    /// check declares `covers: true`. Empty for an unknown id, an entry with
    /// no check or a malformed one, and a check that does not declare it
    /// (`CheckDef::covered_globs`).
    pub(crate) fn covering_globs(&self, id: &str) -> Vec<String> {
        match self.entries.get(id).map(|entry| &entry.check) {
            Some(CheckField::Valid(check)) => check.covered_globs(),
            _ => Vec::new(),
        }
    }
}

/// `true` when the raw entry `id` carries `tag` in its `tags` array. Only
/// an exact tag counts; an unknown `id`, or a `tags` field that is no
/// array, counts as no tags.
pub(crate) fn raw_entry_has_tag(base: &Base, id: &str, tag: &str) -> bool {
    base.raw_entries
        .get(id)
        .and_then(|raw| raw.get("tags"))
        .and_then(Value::as_array)
        .is_some_and(|tags| tags.iter().any(|candidate| candidate.as_str() == Some(tag)))
}

/// A failure while loading the knowledge base from disk: an unreadable
/// file, JSON that failed to parse, an area glob that failed to compile,
/// a pack copy or `overrides.json` that breaks its embedded schema, or a
/// pack name without a copy file of its own, named with the offending path
/// (and, for `Glob`, the offending glob itself, from the wrapped
/// `GlobError`).
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
    /// The refusal of a pack copy, of `overrides.json`, or of a pack name
    /// that two copies declare or that differs from its file stem: one
    /// text that already names the file and the field or the names.
    Invalid(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { path, source } => write!(f, "{}: {source}", path.display()),
            LoadError::Json { path, source } => {
                write!(f, "{}: invalid JSON ({source})", path.display())
            }
            LoadError::Glob { path, source } => write!(f, "{}: {source}", path.display()),
            LoadError::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Reads `path` and parses it as JSON, naming the file in any read or
/// parse failure -- the one place `load_base` and `load_areas` both read
/// a knowledge file from disk (`check_base`'s own shape validation runs
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
/// failing.
fn string_field(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Classifies a raw `check` field into `CheckField::{Absent, Malformed,
/// Valid}` -- `Absent` for a missing key or a JS-falsy value, `Valid` for
/// an object that parses cleanly as a `CheckDef`, `Malformed` for
/// everything JS-truthy in between (a non-object truthy value, or an
/// object that fails to parse: an unknown `type`, a missing required
/// field). See `CheckField`'s own doc for why the distinction matters.
fn classify_check(raw: Option<&Value>) -> CheckField {
    if falsy(raw) {
        return CheckField::Absent;
    }
    match raw.filter(|value| value.is_object()) {
        Some(value) => serde_json::from_value::<CheckDef>(value.clone())
            .map(|check| CheckField::Valid(Box::new(check)))
            .unwrap_or(CheckField::Malformed),
        None => CheckField::Malformed,
    }
}

/// The names of the `.json` files directly under `dir`, sorted. A
/// directory entry that cannot be read is the operating system's own
/// error, named with `dir`. A name that is not valid Unicode cannot name a
/// topic or a pack, so it is skipped.
fn json_file_names(dir: &Path) -> Result<Vec<String>, LoadError> {
    let io_error = |source| LoadError::Io {
        path: dir.to_path_buf(),
        source,
    };
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_error)? {
        let entry = entry.map_err(io_error)?;
        if let Ok(name) = entry.file_name().into_string()
            && name.ends_with(".json")
        {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// One local topic file as read.
struct LocalTopic {
    /// The repo-relative path, `knowledge/<topic>.json`.
    file: String,
    /// The file stem: the topic name.
    name: String,
    content: Value,
}

impl LocalTopic {
    /// The items of the file's `entries` array, or none when the key is
    /// absent or holds no array.
    fn items(&self) -> &[Value] {
        array_of(&self.content, "entries")
    }
}

/// Reads the local topic files: every `knowledge/*.json` that `RESERVED`
/// does not name, in filename order.
fn read_local_topics(dir: &Path) -> Result<Vec<LocalTopic>, LoadError> {
    let mut topics = Vec::new();
    for name in json_file_names(dir)? {
        if RESERVED.contains(&name.as_str()) {
            continue;
        }
        let content = read_json_value(&dir.join(&name))?;
        topics.push(LocalTopic {
            file: format!("knowledge/{name}"),
            name: name.strip_suffix(".json").unwrap_or(&name).to_string(),
            content,
        });
    }
    Ok(topics)
}

/// One pack copy as read, with the file it came from.
struct PackFile {
    path: PathBuf,
    /// The file name without `.json`.
    stem: String,
    copy: PackCopy,
}

/// Reads the pack copies under `knowledge/packs/`, sorted by pack name.
/// An absent directory is no pack. A copy that `parse_pack` refuses is a
/// load error that names the file. One copy file belongs to each pack
/// name (`check_pack_names`).
fn read_pack_copies(dir: &Path) -> Result<Vec<PackCopy>, LoadError> {
    let packs_dir = dir.join("packs");
    let names = match json_file_names(&packs_dir) {
        Err(LoadError::Io { source, .. }) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        other => other?,
    };
    let mut files = Vec::with_capacity(names.len());
    for name in names {
        let path = packs_dir.join(&name);
        let raw = read_json_value(&path)?;
        let copy = parse_pack(raw, &path.display().to_string()).map_err(LoadError::Invalid)?;
        files.push(PackFile {
            path,
            stem: name.strip_suffix(".json").unwrap_or(&name).to_string(),
            copy,
        });
    }
    check_pack_names(&files)?;
    let mut packs: Vec<PackCopy> = files.into_iter().map(|file| file.copy).collect();
    packs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(packs)
}

/// Fails when a pack name has no copy file of its own, `<name>.json`. Two
/// copies that declare one name fail first, naming both files in file
/// order. Then a copy whose `name` differs from its file stem fails,
/// naming the file and both names.
fn check_pack_names(files: &[PackFile]) -> Result<(), LoadError> {
    let mut declared_in: HashMap<&str, &Path> = HashMap::new();
    for file in files {
        if let Some(first) = declared_in.insert(file.copy.name.as_str(), file.path.as_path()) {
            return Err(LoadError::Invalid(format!(
                "{}: pack name \"{}\" is also declared by {}",
                first.display(),
                file.copy.name,
                file.path.display()
            )));
        }
    }
    if let Some(file) = files.iter().find(|file| file.copy.name != file.stem) {
        return Err(LoadError::Invalid(format!(
            "{}: pack name \"{}\" does not match the file stem \"{}\"",
            file.path.display(),
            file.copy.name,
            file.stem
        )));
    }
    Ok(())
}

/// What `knowledge/overrides.json` holds, indexed by id.
struct Overrides {
    /// The parsed file, or `null` when it does not exist.
    raw: Value,
    /// The `fields` of the first override of each id.
    fields: HashMap<String, Map<String, Value>>,
    /// The reason of the first exclusion of each id, or `true` when the
    /// exclusion has none.
    exclusions: HashMap<String, Value>,
}

/// Reads `knowledge/overrides.json`. An absent file holds nothing. A file
/// that breaks the embedded overrides schema is a load error that names
/// the file and the field.
fn read_overrides(dir: &Path) -> Result<Overrides, LoadError> {
    let path = dir.join("overrides.json");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            return Ok(Overrides {
                raw: Value::Null,
                fields: HashMap::new(),
                exclusions: HashMap::new(),
            });
        }
        Err(source) => return Err(LoadError::Io { path, source }),
    };
    let raw: Value = serde_json::from_str(&text).map_err(|source| LoadError::Json {
        path: path.clone(),
        source,
    })?;
    let schema: Value = serde_json::from_str(OVERRIDES_SCHEMA)
        .expect("the embedded overrides schema is valid JSON");
    let mut errors = Vec::new();
    validate(
        &raw,
        &schema,
        &path.display().to_string(),
        &mut errors,
        &schema,
    );
    if !errors.is_empty() {
        return Err(LoadError::Invalid(errors.join("; ")));
    }
    let mut fields = HashMap::new();
    for item in array_of(&raw, "overrides") {
        let (Some(id), Some(Value::Object(replaced))) = (item["id"].as_str(), item.get("fields"))
        else {
            continue;
        };
        fields
            .entry(id.to_string())
            .or_insert_with(|| replaced.clone());
    }
    let mut exclusions = HashMap::new();
    for item in array_of(&raw, "exclusions") {
        let Some(id) = item["id"].as_str() else {
            continue;
        };
        let reason = item.get("reason").cloned().unwrap_or(Value::Bool(true));
        exclusions.entry(id.to_string()).or_insert(reason);
    }
    Ok(Overrides {
        raw,
        fields,
        exclusions,
    })
}

/// The items of `value[key]`, or none when the key is absent or holds no
/// array.
fn array_of<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Every id in `knowledge/archive/*.json`. A directory or file that cannot
/// be read or parsed adds no id: `check-knowledge` reports it
/// (`check::check_archive`).
fn read_archived_ids(dir: &Path) -> BTreeSet<String> {
    let archive_dir = dir.join("archive");
    let Ok(Some(names)) = crate::archive::list_archive_json_files(&archive_dir) else {
        return BTreeSet::new();
    };
    let mut ids = BTreeSet::new();
    for name in names {
        if let Ok(content) = crate::archive::read_json(&archive_dir.join(name)) {
            ids.extend(
                array_of(&content, "entries")
                    .iter()
                    .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                    .map(str::to_string),
            );
        }
    }
    ids
}

/// The entry-level result of the merge.
#[derive(Default)]
struct Merged {
    entries: HashMap<String, Entry>,
    raw_entries: HashMap<String, Value>,
    excluded: BTreeMap<String, Value>,
    inactive: BTreeMap<String, Value>,
    overridden: BTreeMap<String, Vec<String>>,
    duplicates: Vec<(String, Source, Source)>,
}

impl Merged {
    /// Adds the entry `item` of `source` under `topic`: its typed fields
    /// and its raw JSON with `topic` appended.
    fn insert(&mut self, id: &str, mut item: Value, topic: &str, source: Source) {
        if let Value::Object(map) = &mut item {
            map.insert("topic".to_string(), Value::String(topic.to_string()));
        }
        self.entries.insert(
            id.to_string(),
            Entry {
                id: id.to_string(),
                kind: string_field(&item, "kind"),
                area: string_field(&item, "area"),
                standing: item
                    .get("standing")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                summary: string_field(&item, "summary"),
                check: classify_check(item.get("check")),
                source,
            },
        );
        self.raw_entries.insert(id.to_string(), item);
    }
}

/// Applies an override's `fields` to a pack entry: each field takes the
/// override's value, in place when the entry has it and after the last
/// field when it lacks it, and a field set to `null` is removed.
fn apply_fields(entry: &mut Value, fields: &Map<String, Value>) {
    let Value::Object(map) = entry else {
        return;
    };
    for (field, value) in fields {
        if value.is_null() {
            map.shift_remove(field);
        } else {
            map.insert(field.clone(), value.clone());
        }
    }
}

/// The pack entry `item` as `get` prints an excluded entry: the entry,
/// `topic`, then `excluded` holding `reason`.
fn excluded_entry(item: &Value, topic: &str, reason: &Value) -> Value {
    let mut printed = item.clone();
    if let Value::Object(map) = &mut printed {
        map.insert("topic".to_string(), Value::String(topic.to_string()));
        map.insert("excluded".to_string(), reason.clone());
    }
    printed
}

/// `true` when the pack entry `item`, with the override `fields` of its id
/// applied, has an inactive status (`archive::is_inactive_status`). An
/// override that sets `status` decides: `null` removes the field, so the
/// entry is active again.
fn is_inactive(item: &Value, fields: Option<&Map<String, Value>>) -> bool {
    is_inactive_status(
        fields
            .and_then(|replaced| replaced.get("status"))
            .or_else(|| item.get("status"))
            .and_then(Value::as_str),
    )
}

/// Merges the entries of every source into one index by id. Local topic
/// files come first in filename order, then the pack copies in name order;
/// the first source that ships an id keeps it. An exclusion drops every
/// pack entry with its id, and an override applies to the pack entry that
/// the base keeps. A pack entry with an inactive status moves from the
/// index to `inactive`.
fn merge_entries(local: &[LocalTopic], packs: &[PackCopy], overrides: &Overrides) -> Merged {
    let mut merged = Merged::default();
    let mut inactive_ids = Vec::new();
    for topic in local {
        for item in topic.items() {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            if !merged.entries.contains_key(id) {
                merged.insert(id, item.clone(), &topic.name, Source::Local);
            }
        }
    }
    for pack in packs {
        let source = Source::Pack(pack.name.clone());
        for item in &pack.entries {
            let id = item["id"].as_str().expect("a parsed pack entry has an id");
            let topic = topic_of(id);
            if let Some(reason) = overrides.exclusions.get(id) {
                merged
                    .excluded
                    .entry(id.to_string())
                    .or_insert_with(|| excluded_entry(item, topic, reason));
            } else if let Some(kept) = merged.entries.get(id) {
                merged
                    .duplicates
                    .push((id.to_string(), kept.source.clone(), source.clone()));
            } else {
                let fields = overrides.fields.get(id);
                let mut effective = item.clone();
                if let Some(fields) = fields {
                    apply_fields(&mut effective, fields);
                    let mut names: Vec<String> = fields.keys().cloned().collect();
                    names.sort();
                    merged.overridden.insert(id.to_string(), names);
                }
                merged.insert(id, effective, topic, source.clone());
                if is_inactive(item, fields) {
                    inactive_ids.push(id.to_string());
                }
            }
        }
    }
    // An inactive entry stays in the index until every pack is read: it is
    // the entry the base keeps for its id, so it still shadows the same id
    // of a later pack.
    for id in inactive_ids {
        merged.entries.remove(&id);
        if let Some(raw) = merged.raw_entries.remove(&id) {
            merged.inactive.insert(id, raw);
        }
    }
    merged.duplicates.sort();
    merged
}

/// What `merge_topics` knows of one topic.
#[derive(Default)]
struct TopicParts {
    local_title: String,
    pack_title: Option<String>,
    entry_count: usize,
}

/// Merges the topics of every source, sorted by name (`TopicMeta`'s doc
/// has the rules). A topic that a pack declares is listed even when no
/// entry of it is left.
fn merge_topics(local: &[LocalTopic], packs: &[PackCopy], overrides: &Overrides) -> Vec<TopicMeta> {
    let mut parts: BTreeMap<String, TopicParts> = BTreeMap::new();
    for topic in local {
        let entry = parts.entry(topic.name.clone()).or_default();
        entry.local_title = string_field(&topic.content, "title");
        entry.entry_count += topic.items().len();
    }
    for pack in packs {
        for (name, title) in &pack.topics {
            parts
                .entry(name.clone())
                .or_default()
                .pack_title
                .get_or_insert_with(|| title.clone());
        }
        for item in &pack.entries {
            let id = item["id"].as_str().expect("a parsed pack entry has an id");
            if !overrides.exclusions.contains_key(id)
                && !is_inactive(item, overrides.fields.get(id))
            {
                parts
                    .entry(topic_of(id).to_string())
                    .or_default()
                    .entry_count += 1;
            }
        }
    }
    parts
        .into_iter()
        .map(|(name, parts)| TopicMeta {
            name,
            title: if parts.local_title.is_empty() {
                parts.pack_title.unwrap_or_default()
            } else {
                parts.local_title
            },
            entry_count: parts.entry_count,
        })
        .collect()
}

/// Loads the merged knowledge base under `root` (see the module doc).
/// `schema.json` and `areas.json` must exist and parse as JSON.
pub(crate) fn load_base(root: &Path) -> Result<Base, LoadError> {
    let dir = root.join("knowledge");
    let schema = read_json_value(&dir.join("schema.json"))?;
    let areas_path = dir.join("areas.json");
    let areas_raw = read_json_value(&areas_path)?;
    let areas = build_areas(&areas_raw, &areas_path)?;
    let local = read_local_topics(&dir)?;
    let packs = read_pack_copies(&dir)?;
    let overrides = read_overrides(&dir)?;

    let merged = merge_entries(&local, &packs, &overrides);
    let topics = merge_topics(&local, &packs, &overrides);
    let topic_files = local
        .into_iter()
        .map(|topic| (topic.file, topic.name, topic.content))
        .collect();
    Ok(Base {
        root: root.to_path_buf(),
        areas,
        entries: merged.entries,
        raw_entries: merged.raw_entries,
        topics,
        schema,
        areas_raw,
        topic_files,
        packs,
        overrides_raw: overrides.raw,
        excluded: merged.excluded,
        inactive: merged.inactive,
        overridden: merged.overridden,
        duplicates: merged.duplicates,
        archived_ids: read_archived_ids(&dir),
    })
}

/// Builds the typed area list from `areas.json`'s already-parsed raw
/// content, preserving its declared key order (the `serde_json::Map`
/// `preserve_order` feature backs `raw` with an `IndexMap` instead of the
/// default `BTreeMap`, which would silently alphabetize the areas and
/// reorder every consumer of this list). Never fails on a malformed
/// shape -- neither `raw` being something other than an object, nor one
/// area's value failing to deserialize into `AreaDef` -- since a shape
/// problem is `check_base`'s schema validator's finding to report (exit
/// 1), not a load failure (exit 2). A malformed area is simply excluded
/// from the returned list -- `check.rs`'s `check_base` runs its schema
/// validation directly against `raw` (`base.areas_raw`), not against
/// this typed list, so the exclusion never hides a finding; it only
/// means the renderers cannot render that one area, which is moot
/// whenever the shape is bad, since `check_base` never reaches the
/// generated-file checks on an unclean first stage.
///
/// The one load failure this still raises: a glob that fails to
/// compile, for every area whose shape DID type-check
/// (`houserules.crash-paths-are-named`). `path` names `areas.json` in
/// that error.
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

/// Reads and parses `knowledge/areas.json` at `path`, then builds its
/// typed area list (see `build_areas`). Kept as its own entry point --
/// distinct from `load_base`, which also needs the raw parsed value
/// `build_areas` consumes -- for the matcher's own tests (`glob.rs`),
/// which need only the typed list; production code reaches
/// `build_areas` through `load_base` instead, so this has no production
/// caller. `#[cfg(test)]`, not `#[allow(dead_code)]`: a genuinely
/// test-only function that says so compiles out of every non-test target
/// entirely, the same convention `backlog::test_support`'s own module
/// doc uses.
#[cfg(test)]
pub(crate) fn load_areas(path: &Path) -> Result<Vec<(String, AreaDef)>, LoadError> {
    let raw = read_json_value(path)?;
    build_areas(&raw, path)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    /// A repository root holding an empty `schema.json` and `areas.json`:
    /// the smallest tree `load_base` accepts. The returned guard removes
    /// the directory on drop (`quality.tests-clean-temp-dirs`).
    fn tree() -> TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        write(root.path(), &["knowledge", "schema.json"], &json!({}));
        write(root.path(), &["knowledge", "areas.json"], &json!({}));
        root
    }

    /// Writes `value` to the file at `components` under `root`, one
    /// component at a time (`houserules.path-pins-mirror-the-code`).
    fn write(root: &Path, components: &[&str], value: &Value) {
        let path = components
            .iter()
            .fold(root.to_path_buf(), |path, part| path.join(part));
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the parent");
        fs::write(&path, serde_json::to_string(value).expect("serialize")).expect("write a file");
    }

    /// A schema-valid entry of the given summary.
    fn entry(id: &str, summary: &str) -> Value {
        json!({
            "id": id, "kind": "rule", "area": "global", "standing": true,
            "summary": summary, "body": ["Body."], "tags": [],
            "source": {"date": "2026-10-19", "by": "controller"},
        })
    }

    /// Writes the local topic file `knowledge/<topic>.json`.
    fn local_topic(root: &Path, topic: &str, title: &str, entries: &[Value]) {
        write(
            root,
            &["knowledge", &format!("{topic}.json")],
            &json!({"topic": topic, "title": title, "entries": entries}),
        );
    }

    /// Writes the pack copy `knowledge/packs/<name>.json` that serves one
    /// topic of the same name.
    fn pack(root: &Path, name: &str, title: &str, entries: &[Value]) {
        write(
            root,
            &["knowledge", "packs", &format!("{name}.json")],
            &json!({
                "name": name, "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {name: {"title": title}},
                "areas": {"global": {"paths": []}},
                "entries": entries,
            }),
        );
    }

    /// Writes `knowledge/overrides.json`.
    fn overrides(root: &Path, overrides: Value, exclusions: Value) {
        write(
            root,
            &["knowledge", "overrides.json"],
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": overrides, "exclusions": exclusions,
            }),
        );
    }

    /// The merged base of a tree the test expects to load.
    fn load(root: &Path) -> Base {
        load_base(root).expect("the tree loads")
    }

    /// The reserved names are the three `knowledge/` files that are not
    /// topics, and no other name.
    #[test]
    fn the_reserved_names_are_the_three_files_that_are_not_topics() {
        assert_eq!(RESERVED, ["schema.json", "areas.json", "overrides.json"]);
    }

    /// A 1.x tree has no `knowledge/packs/` and no `overrides.json`: every
    /// merge field is empty and every entry is local.
    #[test]
    fn a_tree_without_packs_or_overrides_loads_as_the_one_x_shape() {
        let root = tree();
        local_topic(root.path(), "demo", "Demo", &[entry("demo.one", "One.")]);

        let base = load(root.path());

        assert!(base.packs.is_empty());
        assert_eq!(base.overrides_raw, Value::Null);
        assert!(base.excluded.is_empty());
        assert!(base.overridden.is_empty());
        assert!(base.duplicates.is_empty());
        assert!(base.archived_ids.is_empty());
        assert_eq!(base.entries["demo.one"].source, Source::Local);
        assert_eq!(base.topic_files.len(), 1);
    }

    /// `overrides.json` is no topic file, and a pack copy is no topic
    /// file: neither lands in `topic_files`.
    #[test]
    fn overrides_and_pack_copies_are_no_topic_files() {
        let root = tree();
        overrides(root.path(), json!([]), json!([]));
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "One.")]);

        let base = load(root.path());

        assert!(base.topic_files.is_empty());
        assert_eq!(
            base.overrides_raw["schema"],
            json!("houserules/overrides@1.0.0")
        );
    }

    /// A pack entry joins the base: typed fields, raw JSON with `topic`
    /// from the first part of its id, and its pack as source.
    #[test]
    fn a_pack_entry_joins_the_base_with_its_pack_as_source() {
        let root = tree();
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "One.")]);

        let base = load(root.path());

        assert_eq!(base.packs.len(), 1);
        assert_eq!(base.packs[0].name, "demo");
        let loaded = &base.entries["demo.one"];
        assert_eq!(loaded.source, Source::Pack("demo".to_string()));
        assert_eq!(loaded.summary, "One.");
        assert!(loaded.standing);
        let mut expected = entry("demo.one", "One.");
        expected["topic"] = json!("demo");
        assert_eq!(base.raw_entries["demo.one"], expected);
    }

    /// An override replaces whole fields and `null` removes one. The typed
    /// entry and the raw JSON both show the effective entry, and
    /// `overridden` lists the replaced field names, sorted.
    #[test]
    fn an_override_replaces_whole_fields_and_null_removes_one() {
        let root = tree();
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "One.")]);
        overrides(
            root.path(),
            json!([{"id": "demo.one", "fields": {
                "summary": "Replaced.", "standing": null, "tags": ["new"],
            }}]),
            json!([]),
        );

        let base = load(root.path());

        let loaded = &base.entries["demo.one"];
        assert_eq!(loaded.summary, "Replaced.");
        assert!(!loaded.standing);
        let raw = base.raw_entries["demo.one"].as_object().expect("an object");
        assert_eq!(raw["tags"], json!(["new"]));
        assert_eq!(
            raw.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "id", "kind", "area", "summary", "body", "tags", "source", "topic"
            ]
        );
        assert_eq!(
            base.overridden["demo.one"],
            [
                "standing".to_string(),
                "summary".to_string(),
                "tags".to_string()
            ]
        );
        assert_eq!(base.overridden.len(), 1);
    }

    /// An override sets a field that the pack entry lacks, after the fields
    /// it has.
    #[test]
    fn an_override_adds_a_field_the_pack_entry_lacks() {
        let root = tree();
        let mut bare = entry("demo.one", "One.");
        bare.as_object_mut().expect("an object").remove("standing");
        pack(root.path(), "demo", "Demo", &[bare]);
        overrides(
            root.path(),
            json!([{"id": "demo.one", "fields": {"see": ["demo.two"]}}]),
            json!([]),
        );

        let base = load(root.path());

        let raw = base.raw_entries["demo.one"].as_object().expect("an object");
        assert_eq!(raw["see"], json!(["demo.two"]));
        assert_eq!(
            raw.keys()
                .rev()
                .take(2)
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["topic", "see"]
        );
    }

    /// Of two overrides of one id, the first applies.
    #[test]
    fn the_first_of_two_overrides_of_one_id_applies() {
        let root = tree();
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "One.")]);
        overrides(
            root.path(),
            json!([
                {"id": "demo.one", "fields": {"summary": "First."}},
                {"id": "demo.one", "fields": {"summary": "Second.", "area": "rust"}},
            ]),
            json!([]),
        );

        let base = load(root.path());

        assert_eq!(base.entries["demo.one"].summary, "First.");
        assert_eq!(base.entries["demo.one"].area, "global");
        assert_eq!(base.overridden["demo.one"], ["summary".to_string()]);
    }

    /// An override changes pack entries only: a local entry keeps its text
    /// and is never listed as overridden.
    #[test]
    fn an_override_never_changes_a_local_entry() {
        let root = tree();
        local_topic(root.path(), "demo", "Demo", &[entry("demo.one", "Local.")]);
        overrides(
            root.path(),
            json!([{"id": "demo.one", "fields": {"summary": "Replaced."}}]),
            json!([]),
        );

        let base = load(root.path());

        assert_eq!(base.entries["demo.one"].summary, "Local.");
        assert!(base.overridden.is_empty());
    }

    /// An excluded entry is in no `entries` and no `raw_entries`. It stays
    /// in `excluded` as `get` prints it: the pack entry, its `topic`, then
    /// `excluded` with the reason.
    #[test]
    fn an_exclusion_removes_the_entry_and_keeps_it_for_get() {
        let root = tree();
        pack(
            root.path(),
            "demo",
            "Demo",
            &[entry("demo.one", "One."), entry("demo.two", "Two.")],
        );
        overrides(
            root.path(),
            json!([]),
            json!([{"id": "demo.one", "reason": "Not for us."}, {"id": "demo.two"}]),
        );

        let base = load(root.path());

        assert!(base.entries.is_empty());
        assert!(base.raw_entries.is_empty());
        let mut one = entry("demo.one", "One.");
        one["topic"] = json!("demo");
        one["excluded"] = json!("Not for us.");
        let mut two = entry("demo.two", "Two.");
        two["topic"] = json!("demo");
        two["excluded"] = json!(true);
        assert_eq!(base.excluded.len(), 2);
        assert_eq!(base.excluded["demo.one"], one);
        assert_eq!(base.excluded["demo.two"], two);
        let keys: Vec<&str> = base.excluded["demo.one"]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.last(), Some(&"excluded"));
    }

    /// An exclusion beats an override of the same id.
    #[test]
    fn an_exclusion_beats_an_override_of_the_same_id() {
        let root = tree();
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "One.")]);
        overrides(
            root.path(),
            json!([{"id": "demo.one", "fields": {"summary": "Replaced."}}]),
            json!([{"id": "demo.one"}]),
        );

        let base = load(root.path());

        assert!(base.entries.is_empty());
        assert!(base.overridden.is_empty());
        assert_eq!(base.excluded["demo.one"]["summary"], json!("One."));
    }

    /// An exclusion names the pack entry only: a local entry with the same
    /// id stays active.
    #[test]
    fn an_exclusion_leaves_a_local_entry_with_the_same_id_active() {
        let root = tree();
        local_topic(root.path(), "demo", "Demo", &[entry("demo.one", "Local.")]);
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "Pack.")]);
        overrides(root.path(), json!([]), json!([{"id": "demo.one"}]));

        let base = load(root.path());

        assert_eq!(base.entries["demo.one"].summary, "Local.");
        assert_eq!(base.entries["demo.one"].source, Source::Local);
        assert!(base.duplicates.is_empty());
    }

    /// A pack entry that an override sets to `status: retired` or
    /// `status: superseded` is in no `entries` and no `raw_entries`. It
    /// stays in `inactive` as `get` prints it: the effective entry, then
    /// `topic`. `overridden` still lists the replaced field.
    #[test]
    fn an_override_that_sets_an_inactive_status_takes_a_pack_entry_out_of_the_active_index() {
        for status in ["retired", "superseded"] {
            let root = tree();
            pack(
                root.path(),
                "demo",
                "Demo",
                &[entry("demo.one", "One."), entry("demo.two", "Two.")],
            );
            overrides(
                root.path(),
                json!([{"id": "demo.one", "fields": {"status": status}}]),
                json!([]),
            );

            let base = load(root.path());

            assert_eq!(
                base.entries.keys().map(String::as_str).collect::<Vec<_>>(),
                ["demo.two"],
                "{status}"
            );
            assert_eq!(
                base.raw_entries
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["demo.two"],
                "{status}"
            );
            let mut one = entry("demo.one", "One.");
            one["status"] = json!(status);
            one["topic"] = json!("demo");
            assert_eq!(base.inactive.len(), 1, "{status}");
            assert_eq!(base.inactive["demo.one"], one, "{status}");
            let keys: Vec<&str> = base.inactive["demo.one"]
                .as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(&keys[keys.len() - 2..], ["status", "topic"], "{status}");
            assert_eq!(
                base.overridden["demo.one"],
                ["status".to_string()],
                "{status}"
            );
        }
    }

    /// A pack copy that ships `status: retired` or `status: superseded`
    /// itself needs no override: its entry is out of the active index and
    /// in `inactive`, and no field is overridden.
    #[test]
    fn a_pack_entry_that_ships_an_inactive_status_is_out_of_the_active_index() {
        for status in ["retired", "superseded"] {
            let root = tree();
            let mut old = entry("demo.old", "Old.");
            old["status"] = json!(status);
            pack(
                root.path(),
                "demo",
                "Demo",
                &[old.clone(), entry("demo.kept", "Kept.")],
            );

            let base = load(root.path());

            assert_eq!(
                base.entries.keys().map(String::as_str).collect::<Vec<_>>(),
                ["demo.kept"],
                "{status}"
            );
            assert_eq!(
                base.raw_entries
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
                ["demo.kept"],
                "{status}"
            );
            old["topic"] = json!("demo");
            assert_eq!(base.inactive.len(), 1, "{status}");
            assert_eq!(base.inactive["demo.old"], old, "{status}");
            assert!(base.overridden.is_empty(), "{status}");
        }
    }

    /// The status that counts is the effective one. A pack entry with no
    /// status, or with `active`, stays in the index. One that ships
    /// `retired` or `superseded`, or gets either from an override, leaves
    /// it. An override that sets `status` to `active`, or removes it with
    /// `null`, brings either shipped status back.
    #[test]
    fn only_an_effective_status_of_retired_or_superseded_takes_a_pack_entry_out() {
        let root = tree();
        let with_status = |id: &str, status: &str| {
            let mut shipped = entry(id, "S.");
            shipped["status"] = json!(status);
            shipped
        };
        pack(
            root.path(),
            "demo",
            "Demo",
            &[
                entry("demo.plain", "S."),
                with_status("demo.explicit", "active"),
                with_status("demo.retired", "retired"),
                with_status("demo.superseded", "superseded"),
                with_status("demo.retired-back", "retired"),
                with_status("demo.retired-cleared", "retired"),
                with_status("demo.superseded-back", "superseded"),
                with_status("demo.superseded-cleared", "superseded"),
                entry("demo.set-retired", "S."),
                entry("demo.set-superseded", "S."),
                with_status("demo.swapped", "retired"),
            ],
        );
        overrides(
            root.path(),
            json!([
                {"id": "demo.retired-back", "fields": {"status": "active"}},
                {"id": "demo.retired-cleared", "fields": {"status": null}},
                {"id": "demo.superseded-back", "fields": {"status": "active"}},
                {"id": "demo.superseded-cleared", "fields": {"status": null}},
                {"id": "demo.set-retired", "fields": {"status": "retired"}},
                {"id": "demo.set-superseded", "fields": {"status": "superseded"}},
                {"id": "demo.swapped", "fields": {"status": "superseded"}},
            ]),
            json!([]),
        );

        let base = load(root.path());

        let mut active: Vec<&str> = base.entries.keys().map(String::as_str).collect();
        active.sort_unstable();
        assert_eq!(
            active,
            [
                "demo.explicit",
                "demo.plain",
                "demo.retired-back",
                "demo.retired-cleared",
                "demo.superseded-back",
                "demo.superseded-cleared",
            ]
        );
        let out: Vec<&str> = base.inactive.keys().map(String::as_str).collect();
        assert_eq!(
            out,
            [
                "demo.retired",
                "demo.set-retired",
                "demo.set-superseded",
                "demo.superseded",
                "demo.swapped",
            ]
        );
        assert_eq!(
            base.raw_entries["demo.retired-back"]["status"],
            json!("active")
        );
        assert!(
            base.raw_entries["demo.superseded-cleared"]
                .get("status")
                .is_none()
        );
        assert_eq!(base.inactive["demo.swapped"]["status"], json!("superseded"));
    }

    /// An inactive pack entry is still the entry the base keeps for its id:
    /// the same id in a later pack stays ignored and is recorded, and does
    /// not become active in its place.
    #[test]
    fn an_inactive_pack_entry_still_shadows_the_same_id_of_a_later_pack() {
        for status in ["retired", "superseded"] {
            let root = tree();
            for name in ["alpha", "zeta"] {
                write(
                    root.path(),
                    &["knowledge", "packs", &format!("{name}.json")],
                    &json!({
                        "name": name, "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                        "topics": {"demo": {"title": "Demo"}}, "areas": {},
                        "entries": [entry("demo.one", name)],
                    }),
                );
            }
            overrides(
                root.path(),
                json!([{"id": "demo.one", "fields": {"status": status}}]),
                json!([]),
            );

            let base = load(root.path());

            assert!(base.entries.is_empty(), "{status}");
            assert_eq!(
                base.inactive.get("demo.one").map(|entry| &entry["summary"]),
                Some(&json!("alpha")),
                "{status}"
            );
            assert_eq!(
                base.duplicates,
                [(
                    "demo.one".to_string(),
                    Source::Pack("alpha".to_string()),
                    Source::Pack("zeta".to_string())
                )],
                "{status}"
            );
        }
    }

    /// Only a pack entry leaves on an inactive status. A local entry with
    /// `retired` or `superseded` stays in the index until `archive` moves
    /// it.
    #[test]
    fn a_local_entry_with_an_inactive_status_stays_in_the_active_index() {
        for status in ["retired", "superseded"] {
            let root = tree();
            let mut old = entry("demo.old", "Old.");
            old["status"] = json!(status);
            local_topic(root.path(), "demo", "Demo", &[old]);

            let base = load(root.path());

            assert!(base.entries.contains_key("demo.old"), "{status}");
            assert!(base.raw_entries.contains_key("demo.old"), "{status}");
            assert!(base.inactive.is_empty(), "{status}");
        }
    }

    /// One id in a local file and a pack: the local entry wins and the pair
    /// is recorded as the id, the source kept, and the source ignored.
    #[test]
    fn the_local_entry_wins_over_a_pack_entry_and_the_pair_is_recorded() {
        let root = tree();
        local_topic(root.path(), "demo", "Demo", &[entry("demo.one", "Local.")]);
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "Pack.")]);

        let base = load(root.path());

        assert_eq!(base.entries["demo.one"].summary, "Local.");
        assert_eq!(base.raw_entries["demo.one"]["summary"], json!("Local."));
        assert_eq!(
            base.duplicates,
            [(
                "demo.one".to_string(),
                Source::Local,
                Source::Pack("demo".to_string())
            )]
        );
    }

    /// One id in two packs: the first pack in name order wins. The file
    /// order differs from the name order here: `alpha-b.json` sorts before
    /// `alpha.json`, since `-` sorts before `.`.
    #[test]
    fn the_first_pack_in_name_order_wins_over_another_pack() {
        let root = tree();
        write(
            root.path(),
            &["knowledge", "packs", "alpha-b.json"],
            &json!({
                "name": "alpha-b", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {"demo": {"title": "Demo"}}, "areas": {},
                "entries": [entry("demo.one", "Alpha-b.")],
            }),
        );
        write(
            root.path(),
            &["knowledge", "packs", "alpha.json"],
            &json!({
                "name": "alpha", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {"demo": {"title": "Demo"}}, "areas": {},
                "entries": [entry("demo.one", "Alpha.")],
            }),
        );

        let base = load(root.path());

        assert_eq!(base.entries["demo.one"].summary, "Alpha.");
        assert_eq!(
            base.packs
                .iter()
                .map(|pack| pack.name.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "alpha-b"]
        );
        assert_eq!(
            base.duplicates,
            [(
                "demo.one".to_string(),
                Source::Pack("alpha".to_string()),
                Source::Pack("alpha-b".to_string())
            )]
        );
    }

    /// One id in two local topic files keeps 1.x's answer, the first file
    /// in sorted order, and records no pair: `check-knowledge` reports it
    /// from the topic files.
    #[test]
    fn two_local_topic_files_with_one_id_keep_the_first_and_record_no_pair() {
        let root = tree();
        local_topic(root.path(), "a", "A", &[entry("demo.one", "From a.")]);
        local_topic(root.path(), "b", "B", &[entry("demo.one", "From b.")]);

        let base = load(root.path());

        assert_eq!(base.entries["demo.one"].summary, "From a.");
        assert!(base.duplicates.is_empty());
    }

    /// A topic lists every source: the local file's title wins, the first
    /// pack's title serves a topic with no local title, entries sum, an
    /// excluded entry does not count, and a topic that a pack declares
    /// with no entry is listed with a count of zero.
    #[test]
    fn topics_sum_every_source_and_take_the_local_title_else_the_first_packs() {
        let root = tree();
        local_topic(root.path(), "demo", "Local title", &[entry("demo.l", "L.")]);
        write(
            root.path(),
            &["knowledge", "packs", "p.json"],
            &json!({
                "name": "p", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {"demo": {"title": "Pack title"}, "empty": {"title": "Empty"}},
                "areas": {},
                "entries": [entry("demo.a", "A."), entry("demo.b", "B.")],
            }),
        );
        overrides(root.path(), json!([]), json!([{"id": "demo.b"}]));

        let base = load(root.path());

        let rows: Vec<(&str, &str, usize)> = base
            .topics
            .iter()
            .map(|topic| (topic.name.as_str(), topic.title.as_str(), topic.entry_count))
            .collect();
        assert_eq!(rows, [("demo", "Local title", 2), ("empty", "Empty", 0)]);
    }

    /// A topic count leaves out a pack entry whose effective status is
    /// `retired` or `superseded`, whether an override sets the status or
    /// the pack ships it, and counts a pack entry whose override clears the
    /// status. A topic whose every entry is inactive is still listed, with
    /// a count of zero.
    #[test]
    fn topics_leave_out_an_inactive_pack_entry() {
        let root = tree();
        let with_status = |id: &str, status: &str| {
            let mut shipped = entry(id, "S.");
            shipped["status"] = json!(status);
            shipped
        };
        pack(
            root.path(),
            "demo",
            "Demo",
            &[
                entry("demo.kept", "Kept."),
                entry("demo.overridden", "Overridden."),
                entry("demo.overridden-superseded", "Overridden."),
                with_status("demo.shipped", "retired"),
                with_status("demo.shipped-superseded", "superseded"),
                with_status("demo.cleared", "retired"),
                with_status("demo.cleared-superseded", "superseded"),
            ],
        );
        pack(root.path(), "only", "Only", &[entry("only.one", "One.")]);
        overrides(
            root.path(),
            json!([
                {"id": "demo.overridden", "fields": {"status": "retired"}},
                {"id": "demo.overridden-superseded", "fields": {"status": "superseded"}},
                {"id": "demo.cleared", "fields": {"status": null}},
                {"id": "demo.cleared-superseded", "fields": {"status": null}},
                {"id": "only.one", "fields": {"status": "superseded"}},
            ]),
            json!([]),
        );

        let base = load(root.path());

        let rows: Vec<(&str, usize)> = base
            .topics
            .iter()
            .map(|topic| (topic.name.as_str(), topic.entry_count))
            .collect();
        assert_eq!(rows, [("demo", 3), ("only", 0)]);
    }

    /// The ids of every archive file are `archived_ids`. A corrupt archive
    /// file is no load error: `check-knowledge` names it.
    #[test]
    fn archived_ids_hold_every_id_of_the_archive_files_and_skip_a_corrupt_file() {
        let root = tree();
        write(
            root.path(),
            &["knowledge", "archive", "demo.json"],
            &json!({"entries": [entry("demo.old", "Old."), {"no": "id"}]}),
        );
        let corrupt = root
            .path()
            .join("knowledge")
            .join("archive")
            .join("bad.json");
        fs::write(&corrupt, "{ not json").expect("write a corrupt archive file");

        let base = load(root.path());

        assert_eq!(
            base.archived_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["demo.old"]
        );
    }

    /// Files in `knowledge/packs/` that do not end in `.json` are no pack
    /// copies.
    #[test]
    fn a_file_in_the_packs_directory_that_is_not_json_is_ignored() {
        let root = tree();
        let packs = root.path().join("knowledge").join("packs");
        fs::create_dir_all(&packs).expect("create packs/");
        fs::write(packs.join("README.md"), "notes").expect("write a note");

        let base = load(root.path());

        assert!(base.packs.is_empty());
    }

    /// A `knowledge/packs` path that is not a directory is a load error
    /// that names it, with the operating system's own text.
    #[test]
    fn a_packs_path_that_is_not_a_directory_is_a_named_load_error() {
        let root = tree();
        let packs = root.path().join("knowledge").join("packs");
        fs::write(&packs, "oops").expect("write a file in place of the directory");

        let error = load_base(root.path()).err().expect("a load error");

        assert!(
            error
                .to_string()
                .starts_with(&format!("{}: ", packs.display())),
            "got {error}"
        );
    }

    /// An `overrides.json` that exists but cannot be read is a load error
    /// that names the file, with the operating system's own text. Only
    /// absence is no error.
    #[test]
    fn an_overrides_path_that_cannot_be_read_is_a_named_load_error() {
        let root = tree();
        let path = root.path().join("knowledge").join("overrides.json");
        fs::create_dir(&path).expect("create a directory in place of the file");

        let error = load_base(root.path()).err().expect("a load error");

        assert!(
            matches!(&error, LoadError::Io { path: named, .. } if *named == path),
            "got {error}"
        );
        assert!(
            error
                .to_string()
                .starts_with(&format!("{}: ", path.display())),
            "got {error}"
        );
    }

    /// A pack copy that breaks the pack schema is a load error: the line
    /// is the whole refusal of `parse_pack`, and it names the file.
    #[test]
    fn a_pack_copy_that_breaks_the_pack_schema_is_a_load_error_that_names_the_file() {
        let root = tree();
        write(
            root.path(),
            &["knowledge", "packs", "bad.json"],
            &json!({"name": "bad", "schema": "houserules/pack@1.0.0"}),
        );
        let path = root.path().join("knowledge").join("packs").join("bad.json");
        let named = path.display();

        let error = load_base(root.path()).err().expect("a load error");

        assert_eq!(
            error.to_string(),
            format!(
                "{named}: missing \"version\"; {named}: missing \"topics\"; \
                 {named}: missing \"areas\"; {named}: missing \"entries\""
            )
        );
    }

    /// Two pack copies that declare one name are a load error: one line
    /// that names the pack and both files, the first in file order first.
    /// One copy file and one pin key belong to each pack name.
    #[test]
    fn two_pack_copies_that_declare_one_name_are_a_load_error_that_names_both_files() {
        let root = tree();
        pack(root.path(), "demo", "Demo", &[entry("demo.one", "One.")]);
        write(
            root.path(),
            &["knowledge", "packs", "other.json"],
            &json!({
                "name": "demo", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {"demo": {"title": "Demo"}}, "areas": {},
                "entries": [entry("demo.two", "Two.")],
            }),
        );
        let packs = root.path().join("knowledge").join("packs");

        let error = load_base(root.path()).err().expect("a load error");

        assert_eq!(
            error.to_string(),
            format!(
                "{}: pack name \"demo\" is also declared by {}",
                packs.join("demo.json").display(),
                packs.join("other.json").display()
            )
        );
    }

    /// Two copies that declare one name are the error even when neither
    /// file stem is the name: the duplicate comes first.
    #[test]
    fn two_pack_copies_with_a_wrong_stem_and_one_name_report_the_duplicate() {
        let root = tree();
        for stem in ["a", "b"] {
            write(
                root.path(),
                &["knowledge", "packs", &format!("{stem}.json")],
                &json!({
                    "name": "demo", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                    "topics": {"demo": {"title": "Demo"}}, "areas": {},
                    "entries": [entry(&format!("demo.{stem}"), "S.")],
                }),
            );
        }
        let packs = root.path().join("knowledge").join("packs");

        let error = load_base(root.path()).err().expect("a load error");

        assert_eq!(
            error.to_string(),
            format!(
                "{}: pack name \"demo\" is also declared by {}",
                packs.join("a.json").display(),
                packs.join("b.json").display()
            )
        );
    }

    /// A pack copy whose `name` differs from its file stem is a load
    /// error: one line that names the file, the declared name, and the
    /// stem. The copy of a pack is `knowledge/packs/<name>.json`.
    #[test]
    fn a_pack_copy_whose_name_differs_from_its_file_stem_is_a_load_error() {
        let root = tree();
        write(
            root.path(),
            &["knowledge", "packs", "other.json"],
            &json!({
                "name": "demo", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {"demo": {"title": "Demo"}}, "areas": {},
                "entries": [entry("demo.one", "One.")],
            }),
        );
        let path = root
            .path()
            .join("knowledge")
            .join("packs")
            .join("other.json");

        let error = load_base(root.path()).err().expect("a load error");

        assert_eq!(
            error.to_string(),
            format!(
                "{}: pack name \"demo\" does not match the file stem \"other\"",
                path.display()
            )
        );
    }

    /// An `overrides.json` that breaks its schema is a load error that
    /// names the file and the field, and one that is not JSON names the
    /// file. Each line is the whole message.
    #[test]
    fn a_bad_overrides_file_is_a_load_error_that_names_the_file_and_the_field() {
        let root = tree();
        let path = root.path().join("knowledge").join("overrides.json");
        write(
            root.path(),
            &["knowledge", "overrides.json"],
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [{"id": "demo.one", "fields": {"standing": "yes"}}],
            }),
        );

        let error = load_base(root.path()).err().expect("a load error");

        assert_eq!(
            error.to_string(),
            format!(
                "{}.overrides[0].fields.standing: must be boolean or null",
                path.display()
            )
        );

        fs::write(&path, "{ not json").expect("write a corrupt file");
        let parse_error = serde_json::from_str::<Value>("{ not json").expect_err("not JSON");
        let error = load_base(root.path()).err().expect("a load error");
        assert_eq!(
            error.to_string(),
            format!("{}: invalid JSON ({parse_error})", path.display())
        );
    }

    #[test]
    fn covering_globs_names_the_files_of_a_check_that_declares_covers() {
        let root = tree();
        let covering = |id: &str, check: Value| {
            let mut entry = entry(id, "A rule.");
            entry["check"] = check;
            entry
        };
        local_topic(
            root.path(),
            "demo",
            "Demo",
            &[
                covering(
                    "demo.covering",
                    json!({"type": "grep-absent", "level": "fail", "files": ["src/**", "lib/*.rs"],
                           "pattern": "x", "scope": "tree", "covers": true}),
                ),
                covering(
                    "demo.plain",
                    json!({"type": "grep-absent", "level": "fail", "files": "src/**",
                           "pattern": "x", "scope": "tree"}),
                ),
                covering(
                    "demo.malformed",
                    json!({"type": "no-such-type", "level": "fail", "covers": true}),
                ),
                entry("demo.unchecked", "A rule with no check."),
            ],
        );
        let base = load_base(root.path()).expect("load the base");

        assert_eq!(base.covering_globs("demo.covering"), ["src/**", "lib/*.rs"]);
        for id in [
            "demo.plain",
            "demo.malformed",
            "demo.unchecked",
            "demo.unknown",
        ] {
            assert_eq!(base.covering_globs(id), Vec::<String>::new(), "{id}");
        }
    }
}
