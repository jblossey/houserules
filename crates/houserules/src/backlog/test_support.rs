//! Fixture builders shared by `load.rs`'s and `commands.rs`' own
//! `#[cfg(test)] mod tests` (task-2-review.json, issue 9): both modules
//! load a backlog to exercise different parts of it, and a fixture that
//! drifts between two copies would let one module's tests silently stop
//! covering the shape the other module's tests assume. Test-only
//! (`#[cfg(test)]` on the `mod test_support` declaration in `mod.rs`), so
//! this file compiles into nothing outside `cargo test`.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};

/// Writes `content` as pretty JSON with a trailing newline at
/// `root.join(relative)`, creating parent directories as needed -- every
/// fixture file this module's builders write goes through this one path.
pub(super) fn write(root: &Path, relative: &str, content: &Value) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(content).unwrap()),
    )
    .unwrap();
}

/// A minimal, schema-valid backlog item, shallow-merged with `overrides` --
/// Rust port of `tests/backlog.test.mjs`'s `item()` helper.
pub(super) fn item(overrides: Value) -> Value {
    let mut base = json!({
        "id": "WI-001",
        "type": "feat",
        "milestone": "M0",
        "status": "open",
        "title": "First item.",
        "body": ["Body one.", "Body two."],
    });
    if let (Value::Object(base_map), Value::Object(over_map)) = (&mut base, overrides) {
        for (key, value) in over_map {
            base_map.insert(key, value);
        }
    }
    base
}

/// The vendored backlog schema, read once so every fixture repo carries the
/// real `$defs` -- Rust port of `tests/backlog.test.mjs`'s module-level
/// `SCHEMA` constant.
pub(super) fn vendored_schema() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/backlog/schema.json");
    serde_json::from_str(&fs::read_to_string(path).expect("read the vendored backlog schema"))
        .expect("parse the vendored backlog schema")
}

/// `tests/backlog.test.mjs`'s module-level `DEFAULT_ITEMS`: the `read
/// commands`/`set` tests assert against these literals directly, never
/// against `LoadedBacklog::items`, so a bug in `load_backlog` cannot hide
/// behind a fixture that mirrors the same bug.
pub(super) fn default_items() -> Vec<Value> {
    vec![
        item(json!({})),
        item(json!({
            "id": "WI-002", "status": "done", "batch": 1, "title": "Second item:",
            "milestone": null, "see": ["WI-001", "A-01"],
        })),
    ]
}

/// `tests/backlog.test.mjs`'s module-level `DEFAULT_AMENDMENT`.
pub(super) fn default_amendment() -> Value {
    json!({
        "id": "A-01", "type": "constraint", "status": "done",
        "text": ["Latest stable versions."],
    })
}

/// A minimal backlog under a fresh temp root, one items file (`E01`,
/// carrying `items`), one amendment, one batch, one decision, and one
/// parked group -- Rust port of `tests/backlog.test.mjs`'s `makeRepo`.
pub(super) fn make_repo(items: Vec<Value>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    write(root, "backlog/schema.json", &vendored_schema());
    write(
        root,
        "backlog/amendments.json",
        &json!({"heading": "A. Amendments", "amendments": [default_amendment()]}),
    );
    write(
        root,
        "backlog/items/E01.json",
        &json!({
            "section": "E01", "heading": "E01. Product scope (S1)",
            "title": "Product scope", "spec": "S1", "items": items,
        }),
    );
    write(
        root,
        "backlog/batches.json",
        &json!({"heading": "Batch planning", "intro": [], "table_header": [], "batches": [{
            "number": 1, "items": ["WI-002"], "summary": "WI-002 -- second",
            "kickoff": "user 2026-08-01", "status": {"state": "done", "text": "done -- merged"},
        }]}),
    );
    write(
        root,
        "backlog/decisions.json",
        &json!({"preamble": "# Backlog", "decisions": [{"date": "2026-08-01", "text": "Markdown only."}], "notes": ["A note."]}),
    );
    write(
        root,
        "backlog/parked.json",
        &json!({"groups": [{"batch": 29, "intro": "Batch 29 parked polish", "items": [{"id": "PP-29-01", "text": "A parked item."}]}]}),
    );
    dir
}
