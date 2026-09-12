//! `houserules archive`: moves retired records out of the active backlog
//! and knowledge base into their `archive/` mirrors.
//!
//! Three record classes qualify: backlog items whose `status` is `done` or
//! `dropped` (`backlog/items/<file>.json` -> `backlog/archive/<file>.json`),
//! batch entries in `backlog/batches.json` whose `status.state` is `done`
//! (-> `backlog/archive/batches.json`), and knowledge entries whose
//! `status` is `superseded` or `retired` (`knowledge/schema.json`'s
//! optional entry field; absent means `active`) (`knowledge/<topic>.json`
//! -> `knowledge/archive/<topic>.json`). `backlog/decisions.json` is never
//! read or written here: rulings are the permanent record.
//!
//! Every write goes through [`crate::emit::emit`], the one shared JSON
//! serializer, so an archive file's formatting always matches the file it
//! was moved from. The sweep is idempotent: a record already moved carries
//! no more trace in its active file, so a second run finds nothing to move
//! for it.
//!
//! This module owns no reader of its own for the active set -- `get`
//! (`crate::get`) is the one caller of [`find_archived_backlog_item`] and
//! [`find_archived_knowledge_entry`], the fallback lookups that let an
//! archived id keep resolving (`knowledge-base.ids-are-permanent`) once
//! `houserules archive` has moved it out of the base `rules::load_base`/
//! `backlog::load_backlog` read.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::Value;

use crate::emit::emit;

/// Reads `path` and parses it as JSON, naming `display_name` in either
/// failure. This is the one place the module reads a JSON file from disk.
/// It matches `rules::model`'s and `backlog::load`'s own
/// `read_json_value`. Each module keeps its own copy, because a shared
/// one would cross the `rules`/`backlog` modular-install boundary this
/// crate-root module already sits outside of.
///
/// `pub(crate)`: both `check_archive` passes (`rules::check`,
/// `backlog::commands`) call this directly. Each names the repository-
/// relative form it already computes (`format!("knowledge/archive/
/// {name}")`). A check-gate finding then matches the one relative-path
/// format every other finding in the same run uses.
///
/// `read_json`, below, names `path` itself. Its callers (`cmd_archive`,
/// the two `find_archived_*` lookups) have no shorter name to report.
pub(crate) fn read_json_as(path: &Path, display_name: &str) -> Result<Value, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{display_name}: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("{display_name}: invalid JSON ({error})"))
}

/// Reads `path` and parses it as JSON, naming `path` itself in either
/// failure -- `read_json_as`'s own doc has the full account of the two
/// forms and why each exists.
pub(crate) fn read_json(path: &Path) -> Result<Value, String> {
    read_json_as(path, &path.display().to_string())
}

/// Serializes `value` through the shared emitter and writes it to `path`,
/// creating `path`'s parent directory (`backlog/archive/` or
/// `knowledge/archive/`, on a repository's first sweep) if it does not yet
/// exist.
fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
    }
    fs::write(path, emit(value)).map_err(|error| format!("{}: {error}", path.display()))
}

/// Collects every `.json` file name from an already-opened `read_dir`,
/// sorted. The one step shared by `json_file_names` and
/// `list_archive_json_files`: each opens `dir` itself, so each reports its
/// own required-or-tolerant policy on the OPEN failure, and both reuse
/// this for everything after that open succeeds. An unreadable directory
/// entry, or a name that is not valid Unicode, is a named error here
/// (`houserules.crash-paths-are-named`).
fn collect_json_names(read_dir: fs::ReadDir, dir: &Path) -> Result<Vec<String>, String> {
    let mut names: Vec<String> = read_dir
        .map(|entry| entry.map_err(|error| format!("{}: {error}", dir.display())))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    Ok(names)
}

/// Every `.json` file name directly under `dir`, sorted. `dir` must exist:
/// this is the required-directory form `archive_backlog_items` and
/// `archive_knowledge_entries` use for `backlog/items/` and `knowledge/`.
/// A missing or otherwise unreadable `dir` is the original OS error
/// `fs::read_dir` returned, never a reconstruction of one
/// (`houserules.crash-paths-are-named`): the operator needs the real
/// errno text, and absence is not tolerated here the way it is for an
/// archive directory (`list_archive_json_files`, below).
fn json_file_names(dir: &Path) -> Result<Vec<String>, String> {
    let read_dir = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    collect_json_names(read_dir, dir)
}

/// Every `.json` file name directly under an `archive/` directory, sorted,
/// or `None` when that directory does not exist yet -- most repositories
/// have archived nothing. Any other reason `dir` cannot be opened is a
/// named error, never confused with clean absence. Shared by both
/// `find_archived_*` lookups below and both `check_archive` passes
/// (`rules::check`, `backlog::commands`), so the tolerant-absence-but-
/// loud-corruption policy cannot drift between the three archive-
/// directory readers.
pub(crate) fn list_archive_json_files(dir: &Path) -> Result<Option<Vec<String>>, String> {
    let read_dir = match fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", dir.display())),
    };
    collect_json_names(read_dir, dir).map(Some)
}

/// Appends `moved` to `archive_path`'s `array_key` array, creating the file
/// from `template` (the active file's own content, its `array_key` array
/// replaced with an empty one) the first time a record from this file is
/// archived. Every other field -- `section`/`heading`/`title`/`spec` for a
/// backlog items file, `heading`/`intro`/`table_header` for
/// `batches.json`, `topic`/`title` for a knowledge topic file -- carries
/// over from `template` unchanged, once, on creation only; a pre-existing
/// archive file keeps its own copy of those fields and is never
/// overwritten by `template`'s.
fn append_to_archive(
    archive_path: &Path,
    template: &Value,
    array_key: &str,
    moved: Vec<Value>,
) -> Result<(), String> {
    let mut archive_content = if archive_path.exists() {
        read_json(archive_path)?
    } else {
        let mut fresh = template.clone();
        if let Value::Object(map) = &mut fresh {
            map.insert(array_key.to_string(), Value::Array(Vec::new()));
        }
        fresh
    };
    match archive_content.get_mut(array_key) {
        Some(Value::Array(array)) => array.extend(moved),
        _ => {
            return Err(format!(
                "{}: \"{array_key}\" is not an array",
                archive_path.display()
            ));
        }
    }
    write_json(archive_path, &archive_content)
}

/// Partitions `array`'s elements by `is_moved` into `(moved, kept)`.
/// Preserves each side's own relative order. `Iterator::partition` itself
/// returns `(matched, unmatched)`. This function's own name and doc pin
/// that order to `(moved, kept)`, so every call site's `let (moved, keep)
/// = partition(...)` destructures in the order `is_moved`'s own name
/// promises, rather than relying on each caller to remember which half
/// comes first.
fn partition(array: Vec<Value>, is_moved: impl Fn(&Value) -> bool) -> (Vec<Value>, Vec<Value>) {
    array.into_iter().partition(is_moved)
}

/// A record's own on-disk id string, or `"?"` for one with no string `id`.
/// Only ever reached for a malformed record: a schema-valid backlog or
/// knowledge base never carries one. `check-backlog`/`check-knowledge` are
/// where a missing id is a reported finding, not this sweep's job to
/// invent one.
fn record_id(value: &Value) -> &str {
    value.get("id").and_then(Value::as_str).unwrap_or("?")
}

/// A record's own on-disk `field` value as a plain integer string, or the
/// same `"?"` token `record_id` uses when the field is missing or not an
/// integer -- one designed absent token per malformed-field class in this
/// module, not two.
fn record_number(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_i64)
        .map_or_else(|| "?".to_string(), |n| n.to_string())
}

/// Moves every `done`/`dropped` item out of each `backlog/items/*.json`
/// file into its `backlog/archive/<same name>.json` mirror. Returns the
/// number moved; appends one `archived <id> -> backlog/archive/<file>`
/// line per move to `lines`, in file-name then in-file order.
fn archive_backlog_items(root: &Path, lines: &mut Vec<String>) -> Result<usize, String> {
    let items_dir = root.join("backlog/items");
    let archive_dir = root.join("backlog/archive");
    let mut moved_count = 0;
    for name in json_file_names(&items_dir)? {
        let path = items_dir.join(&name);
        let mut content = read_json(&path)?;
        let Some(items) = content.get("items").and_then(Value::as_array).cloned() else {
            continue;
        };
        let (moved, keep) = partition(items, |item| {
            matches!(
                item.get("status").and_then(Value::as_str),
                Some("done") | Some("dropped")
            )
        });
        if moved.is_empty() {
            continue;
        }
        let template = content.clone();
        if let Value::Object(map) = &mut content {
            map.insert("items".to_string(), Value::Array(keep));
        }
        write_json(&path, &content)?;
        append_to_archive(&archive_dir.join(&name), &template, "items", moved.clone())?;
        for item in &moved {
            lines.push(format!(
                "archived {} -> backlog/archive/{name}",
                record_id(item)
            ));
        }
        moved_count += moved.len();
    }
    Ok(moved_count)
}

/// Moves every `status.state == "done"` batch out of `backlog/batches.json`
/// into `backlog/archive/batches.json`. Returns the number moved; appends
/// one `archived batch <number> -> backlog/archive/batches.json` line per
/// move, in the original array order.
///
/// Tests `status.state` alone; a batch's own acceptance ruling being
/// "homed" (recorded in `backlog/decisions.json` or the batch's own
/// ledger) is not checked here. That confirmation is the operator's job,
/// done before running this command -- the `finishing-a-feature` close-out
/// step that calls `archive` already runs only after rulings land.
fn archive_batches(root: &Path, lines: &mut Vec<String>) -> Result<usize, String> {
    let path = root.join("backlog/batches.json");
    let mut content = read_json(&path)?;
    let Some(batches) = content.get("batches").and_then(Value::as_array).cloned() else {
        return Ok(0);
    };
    let (moved, keep) = partition(batches, |batch| {
        batch
            .get("status")
            .and_then(|status| status.get("state"))
            .and_then(Value::as_str)
            == Some("done")
    });
    if moved.is_empty() {
        return Ok(0);
    }
    let template = content.clone();
    if let Value::Object(map) = &mut content {
        map.insert("batches".to_string(), Value::Array(keep));
    }
    write_json(&path, &content)?;
    append_to_archive(
        &root.join("backlog/archive/batches.json"),
        &template,
        "batches",
        moved.clone(),
    )?;
    for batch in &moved {
        let number = record_number(batch, "number");
        lines.push(format!(
            "archived batch {number} -> backlog/archive/batches.json"
        ));
    }
    Ok(moved.len())
}

/// Moves every `superseded`/`retired`-status entry out of each
/// `knowledge/<topic>.json` file into its `knowledge/archive/<topic>.json`
/// mirror; an absent `status` (or any value other than these two) stays
/// active. `schema.json` and `areas.json` are not topic files and are
/// never read here. Returns the number moved; appends one `archived <id>
/// -> knowledge/archive/<topic>.json` line per move, in file-name then
/// in-file order.
fn archive_knowledge_entries(root: &Path, lines: &mut Vec<String>) -> Result<usize, String> {
    let knowledge_dir = root.join("knowledge");
    let archive_dir = knowledge_dir.join("archive");
    let mut moved_count = 0;
    for name in json_file_names(&knowledge_dir)? {
        if name == "schema.json" || name == "areas.json" {
            continue;
        }
        let path = knowledge_dir.join(&name);
        let mut content = read_json(&path)?;
        let Some(entries) = content.get("entries").and_then(Value::as_array).cloned() else {
            continue;
        };
        let (moved, keep) = partition(entries, |entry| {
            matches!(
                entry.get("status").and_then(Value::as_str),
                Some("superseded") | Some("retired")
            )
        });
        if moved.is_empty() {
            continue;
        }
        let template = content.clone();
        if let Value::Object(map) = &mut content {
            map.insert("entries".to_string(), Value::Array(keep));
        }
        write_json(&path, &content)?;
        append_to_archive(
            &archive_dir.join(&name),
            &template,
            "entries",
            moved.clone(),
        )?;
        for entry in &moved {
            lines.push(format!(
                "archived {} -> knowledge/archive/{name}",
                record_id(entry)
            ));
        }
        moved_count += moved.len();
    }
    Ok(moved_count)
}

/// Runs `houserules archive`: sweeps backlog items, batch entries, and
/// knowledge entries in that order (see the module doc for what qualifies
/// in each), printing one `archived ...` line per move followed by an
/// `archive: <n> moved` summary, then exits 0.
///
/// A batch's acceptance ruling being homed is the operator's own
/// confirmation, made before running this command; `archive_batches`'s own
/// doc has the full account.
///
/// Any read or write failure prints one named stderr line and exits 2
/// (`houserules.crash-paths-are-named`). Every move already made before
/// the failing step is still printed first -- a repository swept partway
/// through a failing step keeps whatever it already moved, and the
/// operator sees exactly what that was, not only that something failed.
/// The sweep is idempotent, so re-running it after the fix resumes rather
/// than duplicating those moves.
pub(crate) fn cmd_archive(dir: Option<PathBuf>) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let mut lines = Vec::new();
    let result = (|| -> Result<usize, String> {
        let items = archive_backlog_items(&root, &mut lines)?;
        let batches = archive_batches(&root, &mut lines)?;
        let entries = archive_knowledge_entries(&root, &mut lines)?;
        Ok(items + batches + entries)
    })();
    for line in &lines {
        println!("{line}");
    }
    match result {
        Ok(total) => {
            println!("archive: {total} moved");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Searches `backlog/archive/*.json` for `id`, returning it with
/// `section`/`file` appended the same way `backlog::load_backlog`'s own
/// item index augments an active one. Never `batches.json`: `get` only
/// ever resolves an item id, not a batch number.
///
/// `Ok(None)` when `backlog/archive` does not exist yet, or exists and was
/// searched in full with no match. `Err` the moment any archive file
/// cannot be read or parsed -- named by file, not silently treated as "no
/// match" (`houserules.crash-paths-are-named`): `get`'s one caller reports
/// this as the reason `id` did not resolve, rather than the misleading
/// "unknown id" a swallowed error would print.
pub(crate) fn find_archived_backlog_item(root: &Path, id: &str) -> Result<Option<Value>, String> {
    let dir = root.join("backlog/archive");
    let Some(names) = list_archive_json_files(&dir)? else {
        return Ok(None);
    };
    for name in names {
        if name == "batches.json" {
            continue;
        }
        let content = read_json(&dir.join(&name))?;
        let section = content
            .get("section")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let file = format!("backlog/archive/{name}");
        for item in content
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if item.get("id").and_then(Value::as_str) == Some(id) {
                let mut found = item.clone();
                if let Value::Object(map) = &mut found {
                    map.insert("section".to_string(), Value::String(section));
                    map.insert("file".to_string(), Value::String(file));
                }
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

/// Searches `knowledge/archive/*.json` for `id`, returning it with `topic`
/// appended the same way `rules::model::load_base`'s own `raw_entries`
/// augments an active entry.
///
/// `Ok(None)`/`Err` follow `find_archived_backlog_item`'s own contract
/// (see its doc): absence of the directory or of a match is `Ok(None)`; a
/// read or parse failure on any archive file is a named `Err`.
pub(crate) fn find_archived_knowledge_entry(
    root: &Path,
    id: &str,
) -> Result<Option<Value>, String> {
    let dir = root.join("knowledge/archive");
    let Some(names) = list_archive_json_files(&dir)? else {
        return Ok(None);
    };
    for name in names {
        let content = read_json(&dir.join(&name))?;
        let topic = name.strip_suffix(".json").unwrap_or(&name).to_string();
        for entry in content
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if entry.get("id").and_then(Value::as_str) == Some(id) {
                let mut found = entry.clone();
                if let Value::Object(map) = &mut found {
                    map.insert("topic".to_string(), Value::String(topic));
                }
                return Ok(Some(found));
            }
        }
    }
    Ok(None)
}

/// Tries both archive lookups for `id`, backlog first, and returns the
/// first match. `get`'s one fallback once `id` is unknown in the active
/// set. Trying both, regardless of which domain the active id-shape
/// matcher would route `id` to, is what lets an archived record resolve
/// even when the matcher itself misroutes the id -- a stamped
/// `--id-prefix` other than `HR-`/`A-`/`PP-` always routes to the
/// knowledge branch; fixing the matcher itself is a separate, filed
/// backlog item, not this function's job.
///
/// `Err` on the first read or parse failure either lookup hits.
/// `Ok(None)` only once both have genuinely found nothing.
pub(crate) fn find_archived_any(root: &Path, id: &str) -> Result<Option<Value>, String> {
    if let Some(item) = find_archived_backlog_item(root, id)? {
        return Ok(Some(item));
    }
    find_archived_knowledge_entry(root, id)
}
