//! The knowledge-base read commands: `topics`, `index`, `for`, and
//! `standing` -- `tools/kb.mjs`'s `cmdTopics`/`cmdIndex`/`cmdFor`/
//! `cmdStanding`, ported (batch 17 T4, docs/specs/2026-09-04-batch-15-
//! tier2-spec.md §5 phase 2). `cmdGet`'s port, [`get_entries`], lives here
//! too, but carries no `cmd_get` CLI wrapper of its own: the flat surface's
//! `get` (spec §3) resolves an id by SHAPE between a backlog item and a
//! knowledge entry, so its dispatch lives at the crate root (`crate::get`,
//! next to `crate::emit`) rather than in either feature module -- see that
//! file's own doc for why.
//!
//! Every command here reads [`Base::raw_entries`], never [`Base::entries`]:
//! the spec §3 data-layer rule already established for `backlog`'s own
//! `get`/`set` applies just as much here, for two concrete reasons
//! `Entry`'s reduced field set cannot answer. First, `get` and `for --full`
//! must print an entry's ENTIRE original JSON, in its own on-disk key
//! order -- `Entry` keeps only `id`/`kind`/`area`/`standing`/`summary`/
//! `check`, dropping `body`/`tags`/`source`/`see`/`verify` outright, and
//! reconstructing an object through any typed struct would re-order a
//! hand-edited file's own key layout on print. Second, `index`'s
//! `--topic`/`--tag` filters read fields `Entry` does not carry at all.
//! [`Base::raw_entries`]'s own doc has the loader-side half of this
//! reasoning.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::emit::emit;

use super::check::falsy;
use super::glob::{areas_for, strip_dot};
use super::model::{Base, load_base};
use super::render::{AREA_FILE_KINDS, RULE_KINDS};

/// The command a `for` result points readers at, to see the full standing
/// set -- `tools/kb.mjs`'s `STANDING_COMMAND`. Kept at its frozen, literal
/// value for byte parity: the flat surface's own `houserules standing`
/// rewrite is a separate, later, mechanical phase-3 task (spec §3's "no
/// shims" bullet), not this port's job.
const STANDING_COMMAND: &str = "tools/kb.sh standing";

/// Entry kinds `for` includes by area membership alone (no `verify` match
/// needed) -- `tools/kb.mjs`'s `FOR_KINDS`, `AREA_FILE_KINDS` plus
/// `procedure`.
fn is_for_kind(kind: &str) -> bool {
    AREA_FILE_KINDS.contains(&kind) || kind == "procedure"
}

/// `entry.get("id")` as a plain `&str`, `""` for a missing or non-string
/// id -- every sort in this module orders by this, matching `tools/kb.mjs`'s
/// `byId` (`a.id < b.id`, a plain string comparison).
fn entry_id(entry: &Value) -> &str {
    entry.get("id").and_then(Value::as_str).unwrap_or("")
}

fn sort_by_id(entries: &mut [Value]) {
    entries.sort_by(|a, b| entry_id(a).cmp(entry_id(b)));
}

/// Copies `entry[key]` into `map` under the same name, or omits the key
/// entirely when `entry` does not carry it. This is `JSON.stringify`'s own
/// treatment of an object literal's `undefined`-valued property: `e.kind`
/// reads `undefined` for an entry missing `kind`, and `JSON.stringify`
/// drops an `undefined`-valued key rather than printing it -- a bare
/// `unwrap_or(Value::Null)` instead prints the key with a JSON `null`
/// value, a byte-shape divergence on malformed data, not a tolerance
/// (batch 17 T4 fix round 1, review issue 3: unlike `has_tag`'s and
/// `for_result`'s malformed-data decisions below, this one is parity, not
/// a ruling -- indexRow's own missing-key behavior is exactly this).
fn copy_present(map: &mut serde_json::Map<String, Value>, entry: &Value, key: &str) {
    if let Some(value) = entry.get(key) {
        map.insert(key.to_string(), value.clone());
    }
}

/// `{id, kind, area, standing, summary}` -- `tools/kb.mjs`'s `indexRow`.
/// `standing` is `Boolean(e.standing)`, JS truthiness coerced to a real
/// JSON boolean, always present regardless of the raw field; the other
/// four fields are omitted, not null, when `entry` lacks them
/// (`copy_present`'s own doc).
fn index_row(entry: &Value) -> Value {
    let mut map = serde_json::Map::new();
    copy_present(&mut map, entry, "id");
    copy_present(&mut map, entry, "kind");
    copy_present(&mut map, entry, "area");
    map.insert(
        "standing".to_string(),
        Value::Bool(!falsy(entry.get("standing"))),
    );
    copy_present(&mut map, entry, "summary");
    Value::Object(map)
}

/// Every filter `index` accepts -- `tools/kb.mjs`'s loosely-typed `opts`
/// object as `filterEntries` reads it. `standing` (`index --standing`, no
/// value) is a plain flag; every other field is `None` unless the caller
/// gave that flag a value.
pub(crate) struct IndexOpts {
    pub area: Option<String>,
    pub topic: Option<String>,
    pub tag: Option<String>,
    pub kind: Option<String>,
    pub standing: bool,
}

/// `Ok(true)` when `entry`'s `tags` array contains `tag`; `Err(id)` when
/// `entry`'s `tags` field is missing or not an array -- `tools/kb.mjs`'s
/// `e.tags.includes(...)` crashes uncaught (`TypeError: e.tags.includes is
/// not a function`) on exactly that shape, verified live. Named rather
/// than silent (batch 17 T4 fix round 1, review issue 3's crash-path
/// decision for this instance): `for_result`'s own malformed-`verify`
/// decision below is the same call for the same reason -- a crash the
/// frozen JS reaches on this data is reported, not reproduced and not
/// swallowed (spec §6's crash-path ruling), so `index --tag` on a base
/// carrying a malformed entry is consistent with `for` on one, not a
/// softer, entry-skipping answer for one read command and a hard failure
/// for the other.
fn has_tag<'a>(entry: &'a Value, tag: &str) -> Result<bool, &'a str> {
    match entry.get("tags") {
        Some(Value::Array(tags)) => Ok(tags.iter().any(|t| t.as_str() == Some(tag))),
        _ => Err(entry_id(entry)),
    }
}

/// Every loaded entry (raw, unsorted) matching every filter `opts` sets,
/// then sorted by id -- `tools/kb.mjs`'s `filterEntries`. `Err` only from
/// the `--tag` filter, and only for an entry that survives every filter
/// applied before it (`has_tag`'s own doc) -- matching JS's own filter
/// order and short-circuiting exactly: an entry a `--tag` filter never
/// reaches (excluded already by `--area`/`--topic`) never has its `tags`
/// field read at all.
fn filter_entries(base: &Base, opts: &IndexOpts) -> Result<Vec<Value>, String> {
    let mut entries: Vec<Value> = base.raw_entries.values().cloned().collect();
    if let Some(area) = &opts.area {
        entries.retain(|e| e.get("area").and_then(Value::as_str) == Some(area.as_str()));
    }
    if let Some(topic) = &opts.topic {
        entries.retain(|e| e.get("topic").and_then(Value::as_str) == Some(topic.as_str()));
    }
    if let Some(tag) = &opts.tag {
        let mut tagged = Vec::with_capacity(entries.len());
        for entry in entries {
            match has_tag(&entry, tag) {
                Ok(true) => tagged.push(entry),
                Ok(false) => {}
                Err(id) => {
                    return Err(format!(
                        "{id}: tags is not an array; cannot filter by tag \"{tag}\""
                    ));
                }
            }
        }
        entries = tagged;
    }
    if let Some(kind) = &opts.kind {
        entries.retain(|e| e.get("kind").and_then(Value::as_str) == Some(kind.as_str()));
    }
    if opts.standing {
        entries.retain(|e| !falsy(e.get("standing")));
    }
    sort_by_id(&mut entries);
    Ok(entries)
}

/// Index rows for entries matching every given filter, sorted by id --
/// `tools/kb.mjs`'s `cmdIndex`. `Err` only from `filter_entries`'s own
/// `--tag` malformed-data case.
pub(crate) fn index_entries(base: &Base, opts: &IndexOpts) -> Result<Vec<Value>, String> {
    Ok(filter_entries(base, opts)?.iter().map(index_row).collect())
}

/// `{topic, entries, title}` per loaded topic file, in load order --
/// `tools/kb.mjs`'s `cmdTopics`.
pub(crate) fn topic_rows(base: &Base) -> Vec<Value> {
    base.topics
        .iter()
        .map(|t| json!({"topic": t.name, "entries": t.entry_count, "title": t.title}))
        .collect()
}

/// The stored entries (raw JSON, `topic` field included, per
/// [`Base::raw_entries`]) for the given ids, in the order given --
/// `tools/kb.mjs`'s `cmdGet`. Fails on the first unknown id, matching
/// `Array.prototype.map`'s throw-on-first-error behavior; the crate-root
/// `get` command is this function's only caller, reached only for an id
/// shaped like a knowledge entry.
pub(crate) fn get_entries(base: &Base, ids: &[String]) -> Result<Vec<Value>, String> {
    ids.iter()
        .map(|id| {
            base.raw_entries
                .get(id)
                .cloned()
                .ok_or_else(|| format!("unknown id \"{id}\""))
        })
        .collect()
}

/// The rule package one or more changed paths pull in -- `tools/kb.mjs`'s
/// `cmdFor`: every entry whose kind is rule-shaped (`is_for_kind`) AND
/// whose area one of `paths` resolves to (`areas_for`), plus every entry
/// whose own `verify` names one of `paths` directly, regardless of area or
/// kind. `full` prints each matching entry whole (`get`'s own shape);
/// otherwise each is reduced through `index_row`.
///
/// `Err` from two causes. First, a declared area glob failing to compile
/// -- unreachable in practice, since `model::load_base` already compiles
/// every one eagerly at load time (`model.rs`'s own doc), but propagated
/// as a named error rather than `.expect()`-panicking on that guarantee,
/// matching `audit.rs`'s own `area_files`/`areas_for` call sites. Second,
/// an entry's `verify` array holding a non-string element REACHED BEFORE
/// ANY MATCH: `tools/kb.mjs`'s `stripDot` crashes uncaught on it
/// (`path.replace is not a function`, verified live), the exact
/// malformed-data class spec §6's crash-path ruling covers (`check.rs`'s
/// own `check_base` already reports a non-string `verify` entry as a
/// named finding for the same reason, there via `path.join`'s divergent
/// tolerance rather than a crash) -- reported here as a named error
/// rather than reproduced as a crash or silently excluded (batch 17 T4
/// fix round 1, review issue 3).
///
/// The two short-circuits this reproduces, both required for parity, not
/// only the outer one (fix round 2, review new_breakage 1: fix round 1's
/// own doc claimed both but the code only had the first). Outer: `verify`
/// is read at all only when `area_match` is false for that entry, matching
/// JS's own `||` -- an entry `for` already includes by area never has its
/// `verify` field read, malformed or not, since JS's own evaluation never
/// reaches it either. Inner: the loop over `items` stops at the FIRST
/// element whose stripped path is in `wanted`, matching
/// `Array.prototype.some`'s own element-level short-circuit -- a
/// malformed element AFTER a match is never type-checked, verified live
/// with `verify: ["<matched path>", 123]` on a `decision`-kind entry
/// (`tools/kb.sh for` exits 0 and includes the entry; the pre-fix binary
/// wrongly errored on the trailing `123`, since it kept scanning every
/// element regardless of an earlier match).
pub(crate) fn for_result(base: &Base, paths: &[String], full: bool) -> Result<Value, String> {
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let areas = areas_for(&path_refs, &base.areas).map_err(|error| error.to_string())?;
    let wanted: HashSet<&str> = paths.iter().map(|p| strip_dot(p)).collect();

    let mut entries: Vec<Value> = Vec::new();
    for entry in base.raw_entries.values() {
        let kind = entry.get("kind").and_then(Value::as_str).unwrap_or("");
        let area = entry.get("area").and_then(Value::as_str).unwrap_or("");
        let area_match = is_for_kind(kind) && areas.iter().any(|a| a == area);
        let included = if area_match {
            true
        } else if let Some(items) = entry.get("verify").and_then(Value::as_array) {
            let mut matched = false;
            for item in items {
                let Some(verify_path) = item.as_str() else {
                    return Err(format!(
                        "{}: verify entry {item} is not a string",
                        entry_id(entry)
                    ));
                };
                if wanted.contains(strip_dot(verify_path)) {
                    matched = true;
                    break;
                }
            }
            matched
        } else {
            false
        };
        if included {
            entries.push(entry.clone());
        }
    }
    sort_by_id(&mut entries);

    let entries_value = if full {
        Value::Array(entries)
    } else {
        Value::Array(entries.iter().map(index_row).collect())
    };
    Ok(json!({
        "paths": paths.iter().map(|p| strip_dot(p)).collect::<Vec<_>>(),
        "areas": areas,
        "entries": entries_value,
        "standing": STANDING_COMMAND,
    }))
}

/// `{id, summary}` for every standing rule, then every standing invariant,
/// each group sorted by id -- `tools/kb.mjs`'s `cmdStanding`
/// (`standingEntries`, inlined: it and `render::standing_lines` need
/// different output shapes from the same filter-and-order logic, and nothing
/// else needs a shared name for it). Omits `id`/`summary` rather than
/// printing it null when an entry lacks it (`copy_present`'s own doc).
pub(crate) fn standing_rows(base: &Base) -> Vec<Value> {
    RULE_KINDS
        .iter()
        .flat_map(|kind| {
            let mut group: Vec<&Value> = base
                .raw_entries
                .values()
                .filter(|e| {
                    !falsy(e.get("standing"))
                        && e.get("kind").and_then(Value::as_str) == Some(*kind)
                })
                .collect();
            group.sort_by(|a, b| entry_id(a).cmp(entry_id(b)));
            group
        })
        .map(|e| {
            let mut map = serde_json::Map::new();
            copy_present(&mut map, e, "id");
            copy_present(&mut map, e, "summary");
            Value::Object(map)
        })
        .collect()
}

/// Resolves `dir` (via `crate::root::resolve_root`) and loads the
/// knowledge base there -- shared by every `cmd_*` wrapper below, since
/// this file alone carries four of them (`check.rs`/`render.rs` only ever
/// needed this pattern inline for one command each; `backlog::cli`'s own
/// `load` is the same idea for that module).
fn load(dir: Option<PathBuf>) -> Result<Base, ExitCode> {
    let root = crate::root::resolve_root(dir)?;
    load_base(&root).map_err(|error| {
        eprintln!("{error}");
        ExitCode::from(2)
    })
}

/// Runs `topics`: prints one row per loaded topic file.
pub(crate) fn cmd_topics(dir: Option<PathBuf>) -> ExitCode {
    let base = match load(dir) {
        Ok(base) => base,
        Err(code) => return code,
    };
    print!("{}", emit(&Value::Array(topic_rows(&base))));
    ExitCode::SUCCESS
}

/// Runs `index`: prints every entry matching `opts`, sorted by id, or a
/// named error naming the entry when a `--tag` filter reaches one whose
/// `tags` field is malformed (`filter_entries`'s own doc).
pub(crate) fn cmd_index(dir: Option<PathBuf>, opts: IndexOpts) -> ExitCode {
    let base = match load(dir) {
        Ok(base) => base,
        Err(code) => return code,
    };
    match index_entries(&base, &opts) {
        Ok(rows) => {
            print!("{}", emit(&Value::Array(rows)));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Runs `for`: prints the rule package `paths` pulls in, or `main`'s own
/// "needs at least one path" usage error when `paths` is empty (checked
/// after the base has loaded, matching the frozen JS's own order -- every
/// command's usage checks run after `loadBase`, never before).
pub(crate) fn cmd_for(dir: Option<PathBuf>, paths: Vec<String>, full: bool) -> ExitCode {
    let base = match load(dir) {
        Ok(base) => base,
        Err(code) => return code,
    };
    if paths.is_empty() {
        eprintln!("for needs at least one path");
        return ExitCode::from(2);
    }
    match for_result(&base, &paths, full) {
        Ok(value) => {
            print!("{}", emit(&value));
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

/// Runs `standing`: prints every standing rule, then every standing
/// invariant.
pub(crate) fn cmd_standing(dir: Option<PathBuf>) -> ExitCode {
    let base = match load(dir) {
        Ok(base) => base,
        Err(code) => return code,
    };
    print!("{}", emit(&Value::Array(standing_rows(&base))));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::json;

    use super::*;

    /// A standing `process.sequential` rule, with every field a caller
    /// might override -- Rust port of `tests/kb.test.mjs`'s `entry()`.
    fn entry(overrides: Value) -> Value {
        let mut base = json!({
            "id": "process.sequential",
            "kind": "rule",
            "area": "process",
            "standing": true,
            "summary": "Run agents sequentially.",
            "body": ["One at a time."],
            "tags": ["dispatch"],
            "source": {"date": "2026-08-29", "by": "user"},
        });
        if let (Value::Object(base_map), Value::Object(over_map)) = (&mut base, overrides) {
            for (key, value) in over_map {
                base_map.insert(key, value);
            }
        }
        base
    }

    /// Groups `entries` by their id prefix and writes each group as its own
    /// topic file -- Rust port of `tests/kb.test.mjs`'s `writeTopics`.
    fn write_topics(root: &Path, entries: &[Value]) {
        let mut by_topic: std::collections::BTreeMap<String, Vec<Value>> =
            std::collections::BTreeMap::new();
        for e in entries {
            let id = e["id"].as_str().expect("entry id is a string");
            let topic = id.split('.').next().expect("entry id has a topic prefix");
            by_topic
                .entry(topic.to_string())
                .or_default()
                .push(e.clone());
        }
        for (topic, topic_entries) in by_topic {
            fs::write(
                root.join(format!("knowledge/{topic}.json")),
                serde_json::to_string(&json!({
                    "$schema": "./schema.json", "topic": topic,
                    "title": format!("{topic} title"), "entries": topic_entries,
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }

    /// A knowledge base under `root`: a project-extended seed schema, a
    /// minimal `areas.json` covering `process`/`rust`/`global`, and
    /// `entries` split into topic files -- Rust port of
    /// `tests/kb.test.mjs`'s `makeRepo` (its git init/commit are dropped,
    /// like `check.rs`'s own `make_repo`: `--dir` bypasses git resolution).
    fn make_repo(root: &Path, entries: &[Value]) {
        fs::create_dir_all(root.join("knowledge")).unwrap();
        let mut schema: Value = serde_json::from_str(
            &fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json"),
            )
            .unwrap(),
        )
        .unwrap();
        schema["$defs"]["area"]["enum"] = json!(["global", "process", "rust", "docs"]);
        fs::write(
            root.join("knowledge/schema.json"),
            serde_json::to_string(&schema).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&json!({
                "global": {"paths": []},
                "process": {"paths": []},
                "rust": {"paths": ["crates/**", "Cargo.toml"]},
                "docs": {"paths": ["docs/**", "CLAUDE.md"]},
            }))
            .unwrap(),
        )
        .unwrap();
        write_topics(root, entries);
    }

    /// `tests/kb.test.mjs`, `describe('read commands')`'s own fixture
    /// entries: a standing rule and a standing invariant in `process`, a
    /// non-standing gotcha and a `history`-kind entry in `rust`.
    fn fixture_entries() -> Vec<Value> {
        vec![
            entry(json!({})),
            entry(json!({
                "id": "process.ask", "kind": "invariant", "summary": "Ask when unsure.",
                "tags": ["dispatch", "users"], "see": ["process.sequential"],
                "verify": ["CLAUDE.md"],
                "check": {"type": "commits", "level": "warn", "subject": "^x"},
            })),
            entry(json!({
                "id": "rust.clean", "area": "rust", "standing": false, "kind": "gotcha",
                "summary": "Clean before retry.", "body": [],
                "source": {"date": "2026-08-01", "by": "review", "ref": "TP-226"},
            })),
            entry(json!({
                "id": "rust.history", "area": "rust", "standing": false, "kind": "history",
                "summary": "Batch 19 measured 96.01.",
            })),
        ]
    }

    /// tests/kb.test.mjs, describe('read commands'): "topics lists name,
    /// count, title".
    #[test]
    fn topics_lists_name_count_title() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(dir.path(), &fixture_entries());
        let base = load_base(dir.path()).expect("loads");
        assert_eq!(
            topic_rows(&base),
            vec![
                json!({"topic": "process", "entries": 2, "title": "process title"}),
                json!({"topic": "rust", "entries": 2, "title": "rust title"}),
            ]
        );
    }

    fn row(id: &str, kind: &str, area: &str, standing: bool, summary: &str) -> Value {
        json!({"id": id, "kind": kind, "area": area, "standing": standing, "summary": summary})
    }

    fn no_filters() -> IndexOpts {
        IndexOpts {
            area: None,
            topic: None,
            tag: None,
            kind: None,
            standing: false,
        }
    }

    /// tests/kb.test.mjs, describe('read commands'): "index filters by
    /// area, topic, tag, kind, standing and sorts by id".
    #[test]
    fn index_filters_by_area_topic_tag_kind_standing_and_sorts_by_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(dir.path(), &fixture_entries());
        let base = load_base(dir.path()).expect("loads");

        assert_eq!(
            index_entries(&base, &no_filters()).unwrap(),
            vec![
                row(
                    "process.ask",
                    "invariant",
                    "process",
                    true,
                    "Ask when unsure."
                ),
                row(
                    "process.sequential",
                    "rule",
                    "process",
                    true,
                    "Run agents sequentially.",
                ),
                row("rust.clean", "gotcha", "rust", false, "Clean before retry."),
                row(
                    "rust.history",
                    "history",
                    "rust",
                    false,
                    "Batch 19 measured 96.01.",
                ),
            ]
        );
        assert_eq!(
            index_entries(
                &base,
                &IndexOpts {
                    area: Some("rust".to_string()),
                    ..no_filters()
                }
            )
            .unwrap(),
            vec![
                row("rust.clean", "gotcha", "rust", false, "Clean before retry."),
                row(
                    "rust.history",
                    "history",
                    "rust",
                    false,
                    "Batch 19 measured 96.01.",
                ),
            ]
        );
        assert_eq!(
            index_entries(
                &base,
                &IndexOpts {
                    topic: Some("process".to_string()),
                    tag: Some("users".to_string()),
                    ..no_filters()
                }
            )
            .unwrap(),
            vec![row(
                "process.ask",
                "invariant",
                "process",
                true,
                "Ask when unsure."
            )]
        );
        assert_eq!(
            index_entries(
                &base,
                &IndexOpts {
                    kind: Some("gotcha".to_string()),
                    ..no_filters()
                }
            )
            .unwrap(),
            vec![row(
                "rust.clean",
                "gotcha",
                "rust",
                false,
                "Clean before retry."
            )]
        );
        assert_eq!(
            index_entries(
                &base,
                &IndexOpts {
                    standing: true,
                    ..no_filters()
                }
            )
            .unwrap(),
            vec![
                row(
                    "process.ask",
                    "invariant",
                    "process",
                    true,
                    "Ask when unsure."
                ),
                row(
                    "process.sequential",
                    "rule",
                    "process",
                    true,
                    "Run agents sequentially.",
                ),
            ]
        );
    }

    /// Batch 17 T4 fix round 1, review issue 3 (byte-shape divergence, not
    /// a tolerance ruling): an entry missing `kind`/`area`/`summary`
    /// prints an `index` row that OMITS those keys, matching
    /// `JSON.stringify`'s own drop of an `undefined`-valued property --
    /// not `"kind": null`, which `unwrap_or(Value::Null)` printed before
    /// this fix. `standing` stays present regardless (`Boolean(undefined)`
    /// is a real `false`, never omitted).
    #[test]
    fn index_row_omits_a_missing_field_instead_of_printing_it_null() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[json!({"id": "malformed.bare", "standing": false})],
        );
        let base = load_base(dir.path()).expect("loads");
        assert_eq!(
            index_entries(&base, &no_filters()).unwrap(),
            vec![json!({"id": "malformed.bare", "standing": false})]
        );
    }

    /// Batch 17 T4 fix round 1, review issue 3: `index --tag` on an entry
    /// whose `tags` field is missing or not an array is a named error
    /// naming that entry, not a silently excluded one -- the frozen JS's
    /// `e.tags.includes(...)` crashes uncaught on exactly this shape,
    /// verified live, and spec §6's crash-path ruling covers it the same
    /// way `for`'s own malformed-`verify` case below does. An entry the
    /// `--tag` filter never reaches (already excluded by an earlier
    /// filter) is never checked, matching JS's own filter order.
    #[test]
    fn index_tag_filter_reports_a_named_error_for_an_entry_with_malformed_tags() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[
                entry(json!({"id": "process.tagged"})),
                json!({"id": "malformed.no-tags", "kind": "rule", "area": "global", "standing": false, "summary": "x"}),
            ],
        );
        let base = load_base(dir.path()).expect("loads");
        let error = index_entries(
            &base,
            &IndexOpts {
                tag: Some("anything".to_string()),
                ..no_filters()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            "malformed.no-tags: tags is not an array; cannot filter by tag \"anything\""
        );

        // An entry the tag filter never reaches (excluded by --area first)
        // is never checked, so its own malformed tags never surface.
        let scoped = index_entries(
            &base,
            &IndexOpts {
                area: Some("process".to_string()),
                tag: Some("dispatch".to_string()),
                ..no_filters()
            },
        )
        .unwrap();
        assert_eq!(scoped.len(), 1);
    }

    /// tests/kb.test.mjs, describe('read commands'): "get returns the
    /// stored entries plus topic, in the order of the ids given, and
    /// rejects unknown ids".
    #[test]
    fn get_returns_stored_entries_plus_topic_in_order_and_rejects_unknown_ids() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entries = fixture_entries();
        make_repo(dir.path(), &entries);
        let base = load_base(dir.path()).expect("loads");

        let mut expected_ask = entries[1].clone();
        expected_ask["topic"] = json!("process");
        assert_eq!(
            get_entries(&base, &["process.ask".to_string()]).unwrap(),
            vec![expected_ask.clone()]
        );

        let mut expected_clean = entries[2].clone();
        expected_clean["topic"] = json!("rust");
        assert_eq!(
            get_entries(
                &base,
                &["rust.clean".to_string(), "process.ask".to_string()]
            )
            .unwrap(),
            vec![expected_clean, expected_ask]
        );

        let error = get_entries(&base, &["nope.x".to_string()]).unwrap_err();
        assert_eq!(error, "unknown id \"nope.x\"");
    }

    /// tests/kb.test.mjs, describe('read commands'): "for resolves areas
    /// and lists rule, invariant, gotcha entries only".
    #[test]
    fn for_resolves_areas_and_lists_rule_invariant_gotcha_entries_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let entries = fixture_entries();
        make_repo(dir.path(), &entries);
        let base = load_base(dir.path()).expect("loads");

        assert_eq!(
            for_result(&base, &["crates/a/src/x.rs".to_string()], false).unwrap(),
            json!({
                "paths": ["crates/a/src/x.rs"],
                "areas": ["global", "rust"],
                "entries": [row("rust.clean", "gotcha", "rust", false, "Clean before retry.")],
                // The literal, not the constant: a change to STANDING_COMMAND's
                // own value must still fail this test (mirrors task-4-review.json
                // fix round 1, finding 7, on the JS side of this same case).
                "standing": "tools/kb.sh standing",
            })
        );
        assert_eq!(
            for_result(&base, &["README.md".to_string()], false).unwrap(),
            json!({"paths": ["README.md"], "areas": ["global"], "entries": [], "standing": STANDING_COMMAND})
        );
        let mut expected_clean = entries[2].clone();
        expected_clean["topic"] = json!("rust");
        assert_eq!(
            for_result(&base, &["crates/a/src/x.rs".to_string()], true).unwrap(),
            json!({
                "paths": ["crates/a/src/x.rs"], "areas": ["global", "rust"],
                "entries": [expected_clean], "standing": STANDING_COMMAND,
            })
        );
    }

    /// tests/kb.test.mjs, describe('read commands'): "for includes
    /// procedures and entries whose verify names a path".
    #[test]
    fn for_includes_procedures_and_entries_whose_verify_names_a_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[
                entry(json!({
                    "id": "rust.procedure", "area": "rust", "standing": false, "kind": "procedure",
                    "summary": "Run the sidecar smoke.", "body": [],
                })),
                entry(json!({
                    "id": "docs.verify-only", "area": "docs", "standing": false, "kind": "decision",
                    "summary": "Keep the crate layout.", "body": [],
                    "verify": ["./crates/a/src/x.rs"],
                })),
            ],
        );
        let base = load_base(dir.path()).expect("loads");
        assert_eq!(
            for_result(&base, &["crates/a/src/x.rs".to_string()], false)
                .unwrap()
                .get("entries")
                .cloned()
                .unwrap(),
            json!([
                row(
                    "docs.verify-only",
                    "decision",
                    "docs",
                    false,
                    "Keep the crate layout."
                ),
                row(
                    "rust.procedure",
                    "procedure",
                    "rust",
                    false,
                    "Run the sidecar smoke."
                ),
            ])
        );
        assert_eq!(
            for_result(&base, &["docs/other.md".to_string()], false)
                .unwrap()
                .get("entries")
                .cloned()
                .unwrap(),
            json!([])
        );
    }

    /// Batch 17 T4 fix round 1, review issue 3: an entry's `verify` array
    /// holding a non-string element is a named error naming that entry --
    /// the frozen JS's `stripDot` crashes uncaught on it
    /// (`path.replace is not a function`, verified live), and spec §6's
    /// crash-path ruling covers this malformed-data class (`check.rs`'s
    /// own `check_base` already names a non-string `verify` entry as a
    /// finding for the same class, there via a divergent tolerance rather
    /// than a crash). The entry's own `kind` (`decision`) is not
    /// `for`-eligible by area, so its `verify` is the only route to a
    /// match and is reached.
    #[test]
    fn for_reports_a_named_error_for_a_non_string_verify_entry() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[json!({
                "id": "malformed.bad-verify", "kind": "decision", "area": "global",
                "standing": false, "summary": "y", "verify": [123],
            })],
        );
        let base = load_base(dir.path()).expect("loads");
        let error = for_result(&base, &["anything.txt".to_string()], false).unwrap_err();
        assert_eq!(
            error,
            "malformed.bad-verify: verify entry 123 is not a string"
        );
    }

    /// Batch 17 T4 fix round 2, review new_breakage 1: the verify loop
    /// stops at the FIRST matching element, like `Array.prototype.some`'s
    /// own element-level short-circuit -- a malformed element AFTER a
    /// match is never type-checked. `README.md` matches first; the
    /// trailing `123` must never be reached, so this must succeed, not
    /// error.
    #[test]
    fn for_matches_a_verify_entry_before_a_later_malformed_one_without_checking_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[json!({
                "id": "malformed.match-then-bad-verify", "kind": "decision", "area": "global",
                "standing": false, "summary": "y", "verify": ["README.md", 123],
            })],
        );
        let base = load_base(dir.path()).expect("loads");
        let result = for_result(&base, &["README.md".to_string()], false).unwrap();
        assert_eq!(
            result.get("entries").cloned().unwrap(),
            json!([row(
                "malformed.match-then-bad-verify",
                "decision",
                "global",
                false,
                "y"
            )])
        );
    }

    /// Batch 17 T4 fix round 1, review issue 3: `for`'s own area-then-
    /// verify check short-circuits exactly like the frozen JS's `||` --
    /// an entry `for` already includes by area (a `rule` in `global`,
    /// which every path resolves to) never has its `verify` field read,
    /// so a malformed one there does not surface. Without this, the fix
    /// above would wrongly turn every `for` call into an error whenever
    /// ANY entry anywhere carried a malformed `verify`, matched or not.
    #[test]
    fn for_does_not_check_verify_when_the_entry_already_matches_by_area() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[json!({
                "id": "malformed.area-match-bad-verify", "kind": "rule", "area": "global",
                "standing": false, "summary": "z", "verify": [123],
            })],
        );
        let base = load_base(dir.path()).expect("loads");
        let result = for_result(&base, &["anything.txt".to_string()], false).unwrap();
        assert_eq!(
            result.get("entries").cloned().unwrap(),
            json!([{
                "id": "malformed.area-match-bad-verify", "kind": "rule", "area": "global",
                "standing": false, "summary": "z",
            }])
        );
    }

    /// tests/kb.test.mjs, describe('read commands'): "includes a
    /// non-standing global rule in for, for any path".
    #[test]
    fn includes_a_non_standing_global_rule_in_for_for_any_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[entry(json!({
                "id": "global.always", "area": "global", "standing": false,
                "summary": "Applies everywhere.",
            }))],
        );
        let base = load_base(dir.path()).expect("loads");
        let expected = json!([row(
            "global.always",
            "rule",
            "global",
            false,
            "Applies everywhere."
        )]);
        for path in ["anything/at/all.rs", "unrelated/other.txt"] {
            assert_eq!(
                for_result(&base, &[path.to_string()], false)
                    .unwrap()
                    .get("entries")
                    .cloned()
                    .unwrap(),
                expected
            );
        }
    }

    /// tests/kb.test.mjs, describe('read commands'): "standing lists rules
    /// before invariants".
    #[test]
    fn standing_lists_rules_before_invariants() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(dir.path(), &fixture_entries());
        let base = load_base(dir.path()).expect("loads");
        assert_eq!(
            standing_rows(&base),
            vec![
                json!({"id": "process.sequential", "summary": "Run agents sequentially."}),
                json!({"id": "process.ask", "summary": "Ask when unsure."}),
            ]
        );
    }

    /// Batch 17 T4 fix round 1, review issue 3: a standing entry missing
    /// `summary` prints a `standing` row that OMITS the key, matching
    /// `JSON.stringify`'s own drop of an `undefined`-valued property --
    /// the same fix `index_row_omits_a_missing_field_instead_of_printing_it_null`
    /// pins for `index`.
    #[test]
    fn standing_omits_a_missing_field_instead_of_printing_it_null() {
        let dir = tempfile::tempdir().expect("tempdir");
        make_repo(
            dir.path(),
            &[json!({"id": "malformed.bare", "kind": "rule", "standing": true})],
        );
        let base = load_base(dir.path()).expect("loads");
        assert_eq!(standing_rows(&base), vec![json!({"id": "malformed.bare"})]);
    }
}
