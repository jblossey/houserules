//! `defer` on the JSON backlog (spec 3.1): files a new item that carries a
//! deferral, or marks an existing item as one.
//!
//! The `deferral` object (`reason`, `origin`, `files`) is an optional field
//! of the backlog schema's `item`. A new item goes to
//! `backlog/items/<section>.json` (`general` by default) with the next id
//! above every number the active items and `backlog/archive/` hold. Every
//! write goes through `emit`, like `set`, so the file keeps the form that
//! `check-backlog` compares against.

use serde_json::{Value, json};

use super::commands::{CommandError, rewrite_item};
use super::load::{LoadedBacklog, read_json_value};
use crate::archive::{list_archive_json_files, read_json, write_json};
use crate::tracker::{DeferRequest, DeferTarget, check_defer_request, lines};

/// The highest item number that the id shape `<prefix>-\d{3}` holds.
const LAST_ITEM_NUMBER: u32 = 999;

/// Runs `defer` for `request` on the backlog `b` of an install whose item ids
/// carry `prefix`, and returns the id of the item it filed or marked. The
/// request is checked by `tracker::check_defer_request`, the one checker that
/// every adapter calls.
pub(crate) fn defer(
    b: &LoadedBacklog,
    prefix: &str,
    request: &DeferRequest,
) -> Result<String, CommandError> {
    let target = check_defer_request(request).map_err(CommandError)?;
    let deferral = request.deferral();
    match target {
        DeferTarget::Item(id) => mark_item(b, id, deferral),
        DeferTarget::New { title, section } => file_item(b, prefix, title, section, deferral),
    }
}

/// Marks the active item `id` as a deferral: its object gets the `deferral`
/// field, after the fields it already has.
fn mark_item(b: &LoadedBacklog, id: &str, deferral: Value) -> Result<String, CommandError> {
    let (_, item) = b
        .items
        .iter()
        .find(|(item_id, _)| item_id == id)
        .ok_or_else(|| CommandError(lines::unknown_item(id)))?;
    rewrite_item(b, item, id, |map| {
        map.insert("deferral".to_string(), deferral);
    })?;
    Ok(id.to_string())
}

/// Files a new deferral item titled `title` in `section`, which the request
/// check has found to hold only letters, digits, and hyphens, and returns its
/// id.
fn file_item(
    b: &LoadedBacklog,
    prefix: &str,
    title: &str,
    section: &str,
    deferral: Value,
) -> Result<String, CommandError> {
    let number = highest_item_number(b, prefix)? + 1;
    if number > LAST_ITEM_NUMBER {
        return Err(CommandError(format!(
            "defer: no item id above {prefix}-{LAST_ITEM_NUMBER} is left"
        )));
    }
    let id = format!("{prefix}-{number:03}");
    let reason = deferral["reason"].clone();
    let item = json!({
        "id": id,
        "type": "chore",
        "milestone": null,
        "status": "open",
        "title": title,
        "body": [reason],
        "batch": null,
        "deferral": deferral,
    });
    let path = b
        .root
        .join("backlog")
        .join("items")
        .join(format!("{section}.json"));
    let file = if path.exists() {
        let mut file = read_json_value(&path).map_err(|error| CommandError(error.to_string()))?;
        file.get_mut("items")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| CommandError(format!("{}: \"items\" is not an array", path.display())))?
            .push(item);
        file
    } else {
        json!({
            "section": section,
            "heading": section,
            "title": section,
            "spec": null,
            "items": [item],
        })
    };
    write_json(&path, &file).map_err(CommandError)?;
    Ok(id)
}

/// The highest number among the ids `<prefix>-<digits>` of the active items
/// and of `backlog/archive/`, or 0 when there are none.
fn highest_item_number(b: &LoadedBacklog, prefix: &str) -> Result<u32, CommandError> {
    let number_of = |id: &str| -> Option<u32> {
        let digits = id.strip_prefix(prefix)?.strip_prefix('-')?;
        if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        digits.parse().ok()
    };
    let mut highest = b
        .items
        .iter()
        .filter_map(|(id, _)| number_of(id))
        .max()
        .unwrap_or(0);
    let dir = b.root.join("backlog").join("archive");
    let names = list_archive_json_files(&dir)
        .map_err(CommandError)?
        .unwrap_or_default();
    for name in names.iter().filter(|name| *name != "batches.json") {
        let file = read_json(&dir.join(name)).map_err(CommandError)?;
        let archived = file
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("id").and_then(Value::as_str))
            .filter_map(number_of)
            .max()
            .unwrap_or(0);
        highest = highest.max(archived);
    }
    Ok(highest)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::load::load_backlog;
    use super::super::test_support::{default_items, item, make_repo, write};
    use super::*;

    /// A request that files a new item titled `title`.
    fn filing(title: &str) -> DeferRequest {
        DeferRequest {
            item: None,
            title: Some(title.to_string()),
            reason: "Needs a design.".to_string(),
            origin: "batch 1".to_string(),
            files: Vec::new(),
            section: Some("E01".to_string()),
        }
    }

    /// The new id is one above the highest active number, and an archived
    /// number counts too.
    #[test]
    fn the_new_id_is_above_the_active_items_and_the_archive() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        assert_eq!(defer(&b, "WI", &filing("A")).unwrap(), "WI-003");

        write(
            dir.path(),
            "backlog/archive/E01.json",
            &json!({"section": "E01", "heading": "h", "title": "t", "spec": "", "items": [
                item(json!({"id": "WI-040", "status": "done"})),
            ]}),
        );
        write(
            dir.path(),
            "backlog/archive/batches.json",
            &json!({"batches": [{"number": 99}]}),
        );
        let b = load_backlog(dir.path()).expect("reloads");
        assert_eq!(defer(&b, "WI", &filing("B")).unwrap(), "WI-041");
    }

    /// An id of another prefix does not count.
    #[test]
    fn an_id_of_another_prefix_does_not_count() {
        let dir = make_repo(vec![item(json!({"id": "OTHER-500"}))]);
        let b = load_backlog(dir.path()).expect("loads");
        assert_eq!(defer(&b, "WI", &filing("A")).unwrap(), "WI-001");
    }

    /// The item space ends at 999.
    #[test]
    fn the_item_space_ends_at_999() {
        let dir = make_repo(vec![item(json!({"id": "WI-999"}))]);
        let b = load_backlog(dir.path()).expect("loads");
        let error = defer(&b, "WI", &filing("A")).unwrap_err();
        assert_eq!(error.0, "defer: no item id above WI-999 is left");
    }

    /// A section with a character outside letters, digits, and hyphens is
    /// refused, so no title writes outside `backlog/items/`.
    #[test]
    fn a_section_that_could_leave_the_items_directory_is_refused() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        for section in ["../x", "a/b", "", "a b", "é"] {
            let mut request = filing("A");
            request.section = Some(section.to_string());
            let error = defer(&b, "WI", &request).unwrap_err();
            assert!(
                error.0.starts_with("defer: the section"),
                "{section:?}: {}",
                error.0
            );
        }
    }

    /// The title limit counts characters, and an empty title is refused.
    #[test]
    fn the_title_limit_counts_characters() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        assert!(defer(&b, "WI", &filing(&"é".repeat(120))).is_ok());
        let error = defer(&b, "WI", &filing(&"é".repeat(121))).unwrap_err();
        assert_eq!(error.0, lines::title_too_long(121));
        assert_eq!(
            defer(&b, "WI", &filing(" ")).unwrap_err().0,
            "defer: the title is empty"
        );
    }

    /// A deferral with no files stores an empty list, and backslashes in a
    /// file become slashes.
    #[test]
    fn files_are_stored_with_forward_slashes() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        let mut request = filing("A");
        request.files = vec!["a\\b.rs".to_string(), "c/d.rs".to_string()];
        defer(&b, "WI", &request).unwrap();
        let saved: Value = serde_json::from_str(
            &fs::read_to_string(dir.path().join("backlog/items/E01.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            saved["items"][2]["deferral"]["files"],
            json!(["a/b.rs", "c/d.rs"])
        );
    }

    /// Marking an item that the backlog does not hold is a named error.
    #[test]
    fn marking_an_unknown_item_is_a_named_error() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        let mut request = filing("A");
        request.title = None;
        request.item = Some("WI-404".to_string());
        assert_eq!(
            defer(&b, "WI", &request).unwrap_err().0,
            "unknown item \"WI-404\""
        );
    }

    /// A reason or an origin that says nothing is refused before any write.
    #[test]
    fn an_empty_reason_or_origin_is_refused() {
        let dir = make_repo(default_items());
        let b = load_backlog(dir.path()).expect("loads");
        let mut request = filing("A");
        request.reason = "  ".to_string();
        assert_eq!(
            defer(&b, "WI", &request).unwrap_err().0,
            "defer: the reason is empty"
        );
        let mut request = filing("A");
        request.origin = String::new();
        assert_eq!(
            defer(&b, "WI", &request).unwrap_err().0,
            "defer: the origin is empty"
        );
    }
}
