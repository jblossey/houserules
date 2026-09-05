//! The backlog command surface's pure logic -- `tools/backlog.mjs`'s
//! `checkBacklog`, `cmdGet`, `cmdList`, `cmdBatch`, and `cmdSet`, ported
//! (batch 17 T2). Every function here takes an already-loaded
//! `LoadedBacklog` and returns its result value or a `CommandError`; the
//! `cli` module resolves `--dir`, loads, calls these, and turns the
//! result into stdout/stderr text and an `ExitCode`, the same split
//! `rules::check`'s `check_base`/`cmd_check_knowledge` and `rules::render`'s
//! `render`/`cmd_render` already use.
//!
//! See `load.rs`'s module doc for why every value here is a raw
//! `serde_json::Value`, never a `backlog::model` typed struct.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

pub(crate) use crate::emit::emit;

use super::load::{LoadedBacklog, read_json_value};
use super::model::ItemStatus;

/// One backlog command's usage or runtime failure: printed as a single
/// stderr line, exit 2. Three arms, all rendering identically: (1)
/// `template/tools/lib/cli.mjs`'s `UsageError`, caught by
/// `tools/backlog.mjs`'s `main` (unknown id, bad `field=value`, missing
/// args); (2) `set_item`'s mid-command re-read failure -- the items file
/// disappeared or turned invalid between load and write, a narrow TOCTOU
/// window, via `read_json_value`'s own `LoadError`; (3) `set_item`'s
/// target item vanishing from that same re-read -- edited out of its file
/// in that same window -- which the frozen JS never guards at all
/// (`Object.assign(undefined, changes)` throws an uncaught `TypeError`).
/// Spec §6's crash-path deviation is why (3) is a named `CommandError`
/// arm, not a panic: it is exactly the class of crash the deviation rules
/// against reproducing (task-2-review.json, issue 10 -- an earlier cut of
/// (3) was a `.expect(...)` panic, and this doc comment claimed only (1)
/// and (2)).
#[derive(Debug)]
pub(crate) struct CommandError(pub String);

/// Valid `set status=<value>` values, in the schema's own declared order --
/// `tools/backlog.mjs`'s `STATUSES` constant, expressed here as
/// `model::ItemStatus`'s own variants (schema-pinned by `model.rs`'s build
/// test, `item_status_is_pinned`) rather than a second hand-typed array
/// that could drift from `backlog/schema.json`'s `$defs/status` enum
/// unnoticed. This is the one place `set_item` validates a raw CLI string
/// against a closed set of values, so it is `model::ItemStatus`'s one
/// caller and the only type `backlog::model` still holds: the spec §3
/// data-layer rule (typed models only for a path that never re-serializes
/// its data back to the source and can accept a parse failure) has no
/// other consumer to give -- `get`/`set` need each item's own on-disk key
/// order, and `check-backlog` needs to tolerate malformed data, neither of
/// which a typed struct can do -- so every other type that file once
/// declared is deleted, not dormant (see `load.rs`'s module doc for the
/// full reasoning).
const STATUS_VALUES: [ItemStatus; 4] = [
    ItemStatus::Open,
    ItemStatus::Partial,
    ItemStatus::Done,
    ItemStatus::Dropped,
];

/// `status`'s lowercase schema name, via `ItemStatus`'s own `Serialize`
/// impl (`#[serde(rename_all = "lowercase")]`) -- avoids a second
/// hand-written name table that could drift out of step with the derive.
fn status_name(status: ItemStatus) -> String {
    match serde_json::to_value(status).expect("ItemStatus always serializes") {
        Value::String(name) => name,
        other => unreachable!("ItemStatus serializes as a JSON string, got {other:?}"),
    }
}

/// `/^[1-9]\d*$/`: one or more ASCII digits, the first not `0` -- the shape
/// `set batch=<value>` requires.
fn is_positive_integer(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_digit() && first != '0' => chars.all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// `{id, status, milestone, batch, title}`, in that key order --
/// `tools/backlog.mjs`'s `listRow`. `milestone`/`batch` substitute `null`
/// for a missing key, matching `item.milestone ?? null` /
/// `item.batch ?? null`; the others are copied as-is (always present on a
/// schema-valid item, the only kind `cmd_list`/`cmd_batch` build rows
/// from).
fn list_row(item: &Value) -> Value {
    json!({
        "id": item.get("id").cloned().unwrap_or(Value::Null),
        "status": item.get("status").cloned().unwrap_or(Value::Null),
        "milestone": item.get("milestone").cloned().unwrap_or(Value::Null),
        "batch": item.get("batch").cloned().unwrap_or(Value::Null),
        "title": item.get("title").cloned().unwrap_or(Value::Null),
    })
}

/// Validates a loaded backlog against its schema and every cross-file
/// invariant -- `checkBacklog`, ported. Returns `(errors, warnings)`; an
/// empty `errors` with `check-backlog` printing `warn:` lines for each
/// warning, "backlog: ok", and exiting 0 matches a clean `tools/backlog.sh
/// check` run (`cli.rs`).
pub(crate) fn check_backlog(b: &LoadedBacklog) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    check_ref(
        &b.amendments,
        "amendmentsFile",
        "backlog/amendments.json",
        b,
        &mut errors,
    );
    check_ref(
        &b.batches,
        "batchesFile",
        "backlog/batches.json",
        b,
        &mut errors,
    );
    check_ref(
        &b.decisions,
        "decisionsFile",
        "backlog/decisions.json",
        b,
        &mut errors,
    );
    check_ref(
        &b.parked,
        "parkedFile",
        "backlog/parked.json",
        b,
        &mut errors,
    );
    for section in &b.sections {
        check_ref(&section.content, "itemsFile", &section.file, b, &mut errors);
        let content_section = section
            .content
            .get("section")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if content_section != section.name {
            errors.push(format!(
                "{}: section \"{content_section}\" must equal the file name \"{}\"",
                section.file, section.name
            ));
        }
    }
    if !errors.is_empty() {
        return (errors, warnings);
    }

    let mut ids: HashSet<String> = HashSet::new();
    for amendment in b
        .amendments
        .get("amendments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = amendment.get("id").and_then(Value::as_str) {
            ids.insert(id.to_string());
        }
    }
    for group in b
        .parked
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for parked in group
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(id) = parked.get("id").and_then(Value::as_str) {
                ids.insert(id.to_string());
            }
        }
    }

    let mut seen: HashMap<&str, &str> = HashMap::new();
    for section in &b.sections {
        for item in section
            .content
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
            if let Some(&prior) = seen.get(id) {
                errors.push(format!(
                    "{} {id}: duplicate id (also in {prior})",
                    section.file
                ));
            }
            seen.insert(id, &section.file);
            ids.insert(id.to_string());
        }
    }

    for section in &b.sections {
        for item in section
            .content
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
            for see_value in item
                .get("see")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(see_id) = see_value.as_str()
                    && !ids.contains(see_id)
                {
                    errors.push(format!(
                        "{} {id}: see \"{see_id}\" does not exist",
                        section.file
                    ));
                }
            }
            let status = item
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let batch_is_null = matches!(item.get("batch"), None | Some(Value::Null));
            if status == "done" && batch_is_null {
                warnings.push(format!("{} {id}: done without a batch", section.file));
            }
        }
    }

    let item_ids: HashSet<&str> = b.items.iter().map(|(id, _)| id.as_str()).collect();
    let mut numbers: HashSet<i64> = HashSet::new();
    let mut in_progress = 0u32;
    for batch in b
        .batches
        .get("batches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let number = batch
            .get("number")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        for id_value in batch
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(id) = id_value.as_str()
                && !item_ids.contains(id)
            {
                errors.push(format!(
                    "backlog/batches.json batch {number}: item \"{id}\" does not exist"
                ));
            }
        }
        if !numbers.insert(number) {
            errors.push(format!("backlog/batches.json: duplicate batch {number}"));
        }
        if batch
            .get("status")
            .and_then(|s| s.get("state"))
            .and_then(Value::as_str)
            == Some("in-progress")
        {
            in_progress += 1;
        }
    }
    if in_progress > 1 {
        errors.push(format!(
            "backlog/batches.json: {in_progress} batches in progress (at most one)"
        ));
    }

    (errors, warnings)
}

/// `validate(value, { $ref: "#/$defs/<def>" }, at, errors, b.schema)` --
/// `checkBacklog`'s own `check` closure, ported. Reuses `rules::validate`,
/// the same JSON-Schema-subset engine `check-knowledge` validates against
/// (`template/tools/lib/json-store.mjs`'s `validate`, the one function the
/// frozen `kb.mjs` and `backlog.mjs` both import).
fn check_ref(value: &Value, def: &str, at: &str, b: &LoadedBacklog, errors: &mut Vec<String>) {
    let ref_schema = json!({"$ref": format!("#/$defs/{def}")});
    crate::rules::validate(value, &ref_schema, at, errors, &b.schema);
}

/// The stored records (items with `section`/`file`, amendments, or parked
/// items with `batch`) for the given ids -- `cmdGet`, ported. Fails on the
/// first unknown id, matching `Array.prototype.map`'s throw-on-first
/// behavior.
pub(crate) fn get_items(b: &LoadedBacklog, ids: &[String]) -> Result<Vec<Value>, CommandError> {
    ids.iter()
        .map(|id| {
            if let Some((_, value)) = b.items.iter().find(|(item_id, _)| item_id == id) {
                return Ok(value.clone());
            }
            if let Some(amendment) = b
                .amendments
                .get("amendments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|a| a.get("id").and_then(Value::as_str) == Some(id.as_str()))
            {
                return Ok(amendment.clone());
            }
            for group in b
                .parked
                .get("groups")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(parked) = group
                    .get("items")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .find(|p| p.get("id").and_then(Value::as_str) == Some(id.as_str()))
                else {
                    continue;
                };
                let mut merged = parked.clone();
                if let Value::Object(map) = &mut merged {
                    map.insert(
                        "batch".to_string(),
                        group.get("batch").cloned().unwrap_or(Value::Null),
                    );
                }
                return Ok(merged);
            }
            Err(CommandError(format!("unknown id \"{id}\"")))
        })
        .collect()
}

/// Every filter `list` accepts -- `tools/backlog.mjs`'s `opts` object,
/// typed. Each field mirrors JS's own loose comparison exactly (see
/// `item_matches`'s doc), not a stricter Rust-native one.
pub(crate) struct ListOpts {
    pub open: bool,
    pub status: Option<String>,
    pub milestone: Option<String>,
    pub section: Option<String>,
    pub item_type: Option<String>,
    pub batch: Option<String>,
}

/// List rows for items matching every given filter -- `cmdList`, ported.
pub(crate) fn list_items(b: &LoadedBacklog, opts: &ListOpts) -> Vec<Value> {
    b.items
        .iter()
        .map(|(_, item)| item)
        .filter(|item| item_matches(item, opts))
        .map(list_row)
        .collect()
}

/// `true` when `item` passes every filter set on `opts` -- `cmdList`'s
/// chain of `items.filter(...)` calls, folded into one predicate.
/// `milestone`/`batch` reproduce JS's own coercions (`??`/`String()`)
/// exactly, including their type-sensitive edge: a non-string milestone or
/// a batch of some other JSON type (unreachable for schema-valid data)
/// never matches a CLI-supplied string filter, the same as JS's `===`.
fn item_matches(item: &Value, opts: &ListOpts) -> bool {
    if opts.open {
        let status = item
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if status != "open" && status != "partial" {
            return false;
        }
    }
    if let Some(want) = &opts.status
        && item.get("status").and_then(Value::as_str) != Some(want.as_str())
    {
        return false;
    }
    if let Some(want) = &opts.milestone
        && milestone_or_dash(item).as_deref() != Some(want.as_str())
    {
        return false;
    }
    if let Some(want) = &opts.section
        && item.get("section").and_then(Value::as_str) != Some(want.as_str())
    {
        return false;
    }
    if let Some(want) = &opts.item_type
        && item.get("type").and_then(Value::as_str) != Some(want.as_str())
    {
        return false;
    }
    if let Some(want) = &opts.batch
        && batch_as_string(item).as_deref() != Some(want.as_str())
    {
        return false;
    }
    true
}

/// `item.milestone ?? '-'`: a missing key or explicit `null` becomes
/// `"-"`, a present string keeps its value, and any other JSON type
/// (unreachable for schema-valid data) matches nothing.
fn milestone_or_dash(item: &Value) -> Option<String> {
    match item.get("milestone") {
        None | Some(Value::Null) => Some("-".to_string()),
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    }
}

/// `String(item.batch ?? '')`: a missing key or explicit `null` becomes
/// `""`, a number becomes its decimal form, and any other JSON type
/// (unreachable for schema-valid data) matches nothing.
fn batch_as_string(item: &Value) -> Option<String> {
    match item.get("batch") {
        None | Some(Value::Null) => Some(String::new()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// The batch record with its number, summary, kickoff, status, and item
/// rows -- `cmdBatch`, ported.
pub(crate) fn batch_record(b: &LoadedBacklog, number: &str) -> Result<Value, CommandError> {
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return Err(CommandError("batch needs a number".to_string()));
    }
    let target: i64 = number
        .parse()
        .map_err(|_| CommandError("batch needs a number".to_string()))?;
    let batch = b
        .batches
        .get("batches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|x| x.get("number").and_then(Value::as_i64) == Some(target))
        .ok_or_else(|| CommandError(format!("unknown batch \"{number}\"")))?;
    let items: Vec<Value> = batch
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|id_value| {
            let id = id_value.as_str()?;
            b.items
                .iter()
                .find(|(item_id, _)| item_id == id)
                .map(|(_, item)| list_row(item))
        })
        .collect();
    Ok(json!({
        "number": batch.get("number").cloned().unwrap_or(Value::Null),
        "summary": batch.get("summary").cloned().unwrap_or(Value::Null),
        "kickoff": batch.get("kickoff").cloned().unwrap_or(Value::Null),
        "status": batch.get("status").cloned().unwrap_or(Value::Null),
        "items": items,
    }))
}

/// `Value`'s JavaScript `ToString` for exactly the two shapes `set_item`'s
/// `changes` map ever holds -- a status string as-is, a batch number in
/// its decimal form -- for the `field=value` echo `set_item` returns.
fn display_change(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// Applies `field=value` assignments to an item's file on disk and reports
/// what changed -- `cmdSet`, ported. Re-reads the items file fresh from
/// disk (matching JS, not reusing `b`'s in-memory copy), merges `changes`
/// into the target item's own `serde_json::Map` (an `IndexMap` under the
/// crate-wide `preserve_order` feature: `.insert` updates an existing key
/// in place and appends a new one at the end, exactly `Object.assign`'s
/// own behavior), and writes the whole file back through `emit` -- see
/// `load.rs`'s module doc for why this, not a typed-struct round-trip
/// (`backlog::model` has no such type; the data-layer rule, spec §3, is
/// why), is what reproduces JS's write-formatting byte-for-byte.
pub(crate) fn set_item(
    b: &LoadedBacklog,
    id: Option<&str>,
    assignments: &[String],
) -> Result<String, CommandError> {
    let id = id.filter(|s| !s.is_empty());
    if id.is_none() || assignments.is_empty() {
        return Err(CommandError(
            "set needs <id> and at least one field=value".to_string(),
        ));
    }
    let id = id.expect("checked above");
    let (_, item) = b
        .items
        .iter()
        .find(|(item_id, _)| item_id == id)
        .ok_or_else(|| CommandError(format!("unknown item \"{id}\"")))?;

    let mut changes = serde_json::Map::new();
    for assignment in assignments {
        let mut parts = assignment.split('=');
        let field = parts.next().unwrap_or_default();
        let value = parts.next();
        match field {
            "status" => {
                let status = value
                    .and_then(|v| serde_json::from_value::<ItemStatus>(json!(v)).ok())
                    .ok_or_else(|| {
                        let names = STATUS_VALUES
                            .iter()
                            .map(|s| status_name(*s))
                            .collect::<Vec<_>>()
                            .join(", ");
                        CommandError(format!("status must be one of {names}"))
                    })?;
                changes.insert("status".to_string(), Value::String(status_name(status)));
            }
            "batch" => {
                let value = value.unwrap_or_default();
                if !is_positive_integer(value) {
                    return Err(CommandError("batch must be a positive integer".to_string()));
                }
                let number: i64 = value
                    .parse()
                    .map_err(|_| CommandError("batch must be a positive integer".to_string()))?;
                changes.insert("batch".to_string(), Value::Number(number.into()));
            }
            _ => return Err(CommandError(format!("unknown field \"{field}\""))),
        }
    }

    let file_relative = item
        .get("file")
        .and_then(Value::as_str)
        .expect("load_backlog always attaches the item's file path")
        .to_string();
    let path = b.root.join(&file_relative);
    let mut file_value = read_json_value(&path).map_err(|error| CommandError(error.to_string()))?;
    let target = file_value
        .get_mut("items")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .find(|entry| entry.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| {
            CommandError(format!(
                "{}: \"{id}\" no longer exists (changed since load)",
                path.display()
            ))
        })?;
    if let Value::Object(map) = target {
        for (key, value) in &changes {
            map.insert(key.clone(), value.clone());
        }
    }
    std::fs::write(&path, emit(&file_value))
        .map_err(|error| CommandError(format!("{}: {error}", path.display())))?;

    let rendered = changes
        .iter()
        .map(|(key, value)| format!("{key}={}", display_change(value)))
        .collect::<Vec<_>>()
        .join(" ");
    Ok(format!("{id}: {rendered}\n"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::super::load::load_backlog;
    use super::super::test_support::{default_amendment, default_items, item, make_repo, write};
    use super::*;

    fn list_row_fixture(
        id: &str,
        status: &str,
        milestone: Value,
        batch: Value,
        title: &str,
    ) -> Value {
        json!({"id": id, "status": status, "milestone": milestone, "batch": batch, "title": title})
    }

    /// tests/backlog.test.mjs, describe('read commands'): "get returns
    /// items with section and file, an amendment record, and a parked item
    /// with batch; rejects unknown ids".
    #[test]
    fn get_returns_items_amendments_and_parked_items_rejects_unknown_ids() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");

        let mut wi002 = default_items()[1].clone();
        if let Value::Object(map) = &mut wi002 {
            map.insert("section".to_string(), Value::String("E01".to_string()));
            map.insert(
                "file".to_string(),
                Value::String("backlog/items/E01.json".to_string()),
            );
        }
        assert_eq!(get_items(&b, &["WI-002".to_string()]).unwrap(), vec![wi002]);

        let mut wi001 = default_items()[0].clone();
        if let Value::Object(map) = &mut wi001 {
            map.insert("section".to_string(), Value::String("E01".to_string()));
            map.insert(
                "file".to_string(),
                Value::String("backlog/items/E01.json".to_string()),
            );
        }
        assert_eq!(get_items(&b, &["WI-001".to_string()]).unwrap(), vec![wi001]);

        assert_eq!(
            get_items(&b, &["A-01".to_string()]).unwrap(),
            vec![default_amendment()]
        );
        assert_eq!(
            get_items(&b, &["PP-29-01".to_string()]).unwrap(),
            vec![json!({"id": "PP-29-01", "text": "A parked item.", "batch": 29})]
        );
        assert!(get_items(&b, &["WI-999".to_string()]).is_err());
    }

    /// tests/backlog.test.mjs: "list filters and returns one row per item,
    /// with null for a missing milestone or batch".
    #[test]
    fn list_filters_and_returns_one_row_per_item() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        assert_eq!(
            list_items(&b, &no_opts_template()),
            vec![
                list_row_fixture("WI-001", "open", json!("M0"), Value::Null, "First item."),
                list_row_fixture("WI-002", "done", Value::Null, json!(1), "Second item:"),
            ]
        );

        let open_only = ListOpts {
            open: true,
            ..no_opts_template()
        };
        assert_eq!(
            list_items(&b, &open_only),
            vec![list_row_fixture(
                "WI-001",
                "open",
                json!("M0"),
                Value::Null,
                "First item."
            )]
        );

        let combined = ListOpts {
            open: false,
            status: Some("done".to_string()),
            milestone: Some("-".to_string()),
            section: Some("E01".to_string()),
            item_type: Some("feat".to_string()),
            batch: Some("1".to_string()),
        };
        assert_eq!(
            list_items(&b, &combined),
            vec![list_row_fixture(
                "WI-002",
                "done",
                Value::Null,
                json!(1),
                "Second item:"
            )]
        );

        let by_milestone = ListOpts {
            milestone: Some("M0".to_string()),
            ..no_opts_template()
        };
        assert_eq!(
            list_items(&b, &by_milestone),
            vec![list_row_fixture(
                "WI-001",
                "open",
                json!("M0"),
                Value::Null,
                "First item."
            )]
        );

        let no_such_batch = ListOpts {
            batch: Some("9".to_string()),
            ..no_opts_template()
        };
        assert_eq!(list_items(&b, &no_such_batch), Vec::<Value>::new());
    }

    fn no_opts_template() -> ListOpts {
        ListOpts {
            open: false,
            status: None,
            milestone: None,
            section: None,
            item_type: None,
            batch: None,
        }
    }

    /// tests/backlog.test.mjs: "batch returns the record and its item rows".
    #[test]
    fn batch_returns_the_record_and_its_item_rows() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        assert_eq!(
            batch_record(&b, "1").unwrap(),
            json!({
                "number": 1,
                "summary": "WI-002 -- second",
                "kickoff": "user 2026-08-01",
                "status": {"state": "done", "text": "done -- merged"},
                "items": [
                    {"id": "WI-002", "status": "done", "milestone": null, "batch": 1, "title": "Second item:"},
                ],
            })
        );
        let unknown = batch_record(&b, "7").unwrap_err();
        assert!(unknown.0.contains("unknown batch \"7\""), "{}", unknown.0);
        assert!(batch_record(&b, "x").is_err());
    }

    /// tests/backlog.test.mjs, describe('set'): "updates status and batch
    /// in the item file, keeping the file formatting".
    #[test]
    fn set_updates_status_and_batch_keeping_the_file_formatting() {
        let dir = make_repo(default_items());
        let root = dir.path();
        let b = load_backlog(root).expect("loads");
        let message = set_item(
            &b,
            Some("WI-001"),
            &["status=done".to_string(), "batch=3".to_string()],
        )
        .unwrap();
        assert_eq!(message, "WI-001: status=done batch=3\n");

        let text = fs::read_to_string(root.join("backlog/items/E01.json")).unwrap();
        assert!(text.ends_with('\n'));
        let saved: Value = serde_json::from_str(&text).unwrap();
        let saved_item = &saved["items"][0];
        assert_eq!(saved_item["status"], "done");
        assert_eq!(saved_item["batch"], 3);
        let keys: Vec<&String> = saved_item.as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            vec![
                "id",
                "type",
                "milestone",
                "status",
                "title",
                "body",
                "batch"
            ]
        );

        let b2 = load_backlog(root).expect("reloads");
        let status_error = set_item(&b2, Some("WI-001"), &["status=nope".to_string()]).unwrap_err();
        assert!(
            status_error.0.contains("status must be one of"),
            "{}",
            status_error.0
        );

        let batch_error = set_item(&b2, Some("WI-001"), &["batch=x".to_string()]).unwrap_err();
        assert!(
            batch_error.0.contains("batch must be a positive integer"),
            "{}",
            batch_error.0
        );

        // Not in the brief: covers the `value ?? ''` fallback in the batch
        // check -- an assignment with no `=` at all leaves `value` `None`.
        let batch_bare_error = set_item(&b2, Some("WI-001"), &["batch".to_string()]).unwrap_err();
        assert!(
            batch_bare_error
                .0
                .contains("batch must be a positive integer")
        );

        let unknown_field = set_item(&b2, Some("WI-001"), &["title=x".to_string()]).unwrap_err();
        assert!(
            unknown_field.0.contains("unknown field \"title\""),
            "{}",
            unknown_field.0
        );

        let no_assignments = set_item(&b2, Some("WI-001"), &[]).unwrap_err();
        assert!(no_assignments.0.contains("set needs"));

        let unknown_item = set_item(&b2, Some("WI-404"), &["status=done".to_string()]).unwrap_err();
        assert!(unknown_item.0.contains("unknown item"));
    }

    /// `emit`'s one known formatting boundary (task-2-review.json, issue
    /// 7): a JSON number already in the file, untouched by this `set`,
    /// keeps its own on-disk form through the round trip rather than
    /// JSON.stringify's lossy `f64` re-render -- `2.0` stays `2.0` (not
    /// `2`), and `12345678901234567890` (past `2^53`, still within `u64`)
    /// stays exact (not JS's rounded `12345678901234567000`). Verified
    /// live against the frozen JS before pinning Rust's own answer here
    /// (mode: reconstructed -- this pins `serde_json`'s own, unchanged
    /// number formatting, not logic this crate wrote, so no first-party
    /// mutation demonstrates the same divergence).
    #[test]
    fn set_preserves_a_pre_existing_items_own_number_form() {
        let dir = make_repo(vec![
            item(json!({})),
            item(json!({"id": "WI-002", "batch": 2.0})),
            item(json!({"id": "WI-003", "batch": 12_345_678_901_234_567_890_u64})),
        ]);
        let root = dir.path();
        let b = load_backlog(root).expect("loads");
        set_item(&b, Some("WI-001"), &["status=done".to_string()]).expect("set");

        let text = fs::read_to_string(root.join("backlog/items/E01.json")).unwrap();
        assert!(
            text.contains("\"batch\": 2.0"),
            "a pre-existing non-integer batch must keep its own form, got:\n{text}"
        );
        assert!(
            text.contains("\"batch\": 12345678901234567890"),
            "a pre-existing past-2^53 batch must stay exact, got:\n{text}"
        );
    }

    /// `set_item`'s target item vanishing from its own file in the narrow
    /// window between load and re-read -- a named `CommandError`, never a
    /// panic (task-2-review.json, issue 10; spec §6's crash-path
    /// deviation, since the frozen JS's `Object.assign(undefined, ...)`
    /// throws an uncaught `TypeError` here, the one crash class this port
    /// does not reproduce).
    #[test]
    fn set_item_reports_a_command_error_when_the_target_vanishes_before_the_rewrite() {
        let dir = make_repo(default_items());
        let root = dir.path();
        let b = load_backlog(root).expect("loads");
        // Simulate a concurrent edit: WI-001 is gone from its file by the
        // time set_item re-reads it, though `b` (loaded a moment ago)
        // still has it.
        write(
            root,
            "backlog/items/E01.json",
            &json!({"section": "E01", "heading": "h", "title": "t", "spec": "", "items": [
                item(json!({"id": "WI-002"})),
            ]}),
        );
        let error = set_item(&b, Some("WI-001"), &["status=done".to_string()]).unwrap_err();
        assert!(error.0.contains("WI-001"), "{}", error.0);
        assert!(error.0.contains("no longer exists"), "{}", error.0);
    }

    /// `checkBacklog`, ported: a schema error early-returns before stage
    /// two runs, then (once fixed) duplicate ids, dangling references, and
    /// batch problems all report together -- tests/backlog.test.mjs,
    /// describe('loadBacklog and checkBacklog'): "reports schema errors,
    /// duplicate ids, dangling references, and batch problems" (task-2-
    /// review.json, issue 1: the first cut of this port only carried the
    /// second phase below, dropping the schema-error/early-return proof).
    #[test]
    fn check_backlog_reports_a_schema_error_first_then_duplicate_ids_dangling_references_and_batch_problems()
     {
        let batches = json!({"heading": "h", "intro": [], "table_header": [], "batches": [
            {"number": 2, "items": ["WI-404"], "summary": "s", "kickoff": "", "status": {"state": "in-progress", "text": ""}},
            {"number": 2, "items": [], "summary": "s", "kickoff": "", "status": {"state": "in-progress", "text": ""}},
        ]});

        // Phase 1: WI-001 carries an invalid `type`. The schema error is
        // the ONLY error reported -- proving check_backlog's early return,
        // since WI-001 is ALSO a duplicate id (of the first item()) and
        // WI-003 ALSO dangles a `see` reference that stage two would
        // otherwise report alongside it.
        let dir = make_repo(vec![
            item(json!({})),
            item(json!({"id": "WI-001", "type": "nope"})),
            item(json!({"id": "WI-003", "see": ["WI-999"]})),
        ]);
        write(dir.path(), "backlog/batches.json", &batches);
        let b = load_backlog(dir.path()).expect("loads");
        let (errors, _warnings) = check_backlog(&b);
        assert_eq!(
            errors,
            vec![format!(
                "backlog/items/E01.json.items[1].type: must be one of {}",
                "\"feat\", \"nfr\", \"constraint\", \"decision\", \"process\", \"bug\", \
                 \"fix\", \"chore\", \"research\", \"question\", \"risk\", \"test\""
            )]
        );

        // Phase 2: the same fixture with WI-001's `type` fixed. Stage two
        // now runs and reports every cross-file problem the still-invalid
        // fixture carries.
        write(
            dir.path(),
            "backlog/items/E01.json",
            &json!({"section": "E01", "heading": "h", "title": "t", "spec": "", "items": [
                item(json!({})),
                item(json!({"id": "WI-001"})),
                item(json!({"id": "WI-003", "see": ["WI-999"]})),
            ]}),
        );
        let b = load_backlog(dir.path()).expect("loads");
        let (errors, _warnings) = check_backlog(&b);
        assert_eq!(
            errors,
            vec![
                "backlog/items/E01.json WI-001: duplicate id (also in backlog/items/E01.json)",
                "backlog/items/E01.json WI-003: see \"WI-999\" does not exist",
                "backlog/batches.json batch 2: item \"WI-404\" does not exist",
                "backlog/batches.json: duplicate batch 2",
                "backlog/batches.json: 2 batches in progress (at most one)",
            ]
        );
    }

    /// tests/backlog.test.mjs: "warns about done items without a batch and
    /// a section whose name differs from its file".
    #[test]
    fn warns_about_done_without_a_batch_and_reports_a_section_name_mismatch() {
        let dir = make_repo(vec![item(json!({"status": "done"}))]);
        let b = load_backlog(dir.path()).expect("loads");
        let (_errors, warnings) = check_backlog(&b);
        assert_eq!(
            warnings,
            vec!["backlog/items/E01.json WI-001: done without a batch"]
        );

        write(
            dir.path(),
            "backlog/items/E01.json",
            &json!({"section": "E02", "heading": "h", "title": "t", "spec": "", "items": []}),
        );
        let b2 = load_backlog(dir.path()).expect("loads");
        let (errors2, _warnings2) = check_backlog(&b2);
        assert_eq!(
            errors2,
            vec!["backlog/items/E01.json: section \"E02\" must equal the file name \"E01\""]
        );
    }
}
