//! Helpers that `stats_35.rs` and `validate_36.rs` share: the committed
//! fixture workspaces under `tests/fixtures/stats-35/` and the knowledge base
//! that `stats` and `audit --workspace` load before they read a workspace.
//! Every test file that needs them includes this module and the module
//! `install_shape`, as `tests/install_shape/mod.rs` is shared.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The committed fixture workspace `name`, built component by component
/// (`houserules.path-pins-mirror-the-code`).
pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("stats-35")
        .join(name)
}

/// One entry of the knowledge base: a judged rule has no `check`, a
/// deterministic rule requires the report field `live_run`.
fn entry(id: &str, deterministic: bool) -> Value {
    let mut entry = json!({
        "id": id, "kind": "rule", "area": "global", "standing": false,
        "summary": "A rule.", "source": {"date": "2026-10-09", "by": "review"},
    });
    if deterministic {
        entry["check"] =
            json!({"type": "report-field", "level": "fail", "if": "**", "field": "live_run"});
    }
    entry
}

/// Makes `dir` a 2.0 install with a loadable knowledge base that holds the
/// judged rules `j.one`, `j.two`, `j.three`, and `j.unused` and the
/// deterministic rules `d.one`, `d.two`, and `d.three`.
pub fn write_knowledge_base(dir: &Path) {
    let knowledge = dir.join("knowledge");
    fs::create_dir_all(&knowledge).expect("create knowledge/");
    fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
    fs::write(knowledge.join("areas.json"), "{}").expect("write areas.json");
    crate::install_shape::mark_2_0(dir);
    let entries: Vec<Value> = [
        ("j.one", false),
        ("j.two", false),
        ("j.three", false),
        ("j.unused", false),
        ("d.one", true),
        ("d.two", true),
        ("d.three", true),
    ]
    .iter()
    .map(|(id, deterministic)| entry(id, *deterministic))
    .collect();
    fs::write(
        knowledge.join("test.json"),
        serde_json::to_string(&json!({"entries": entries})).expect("serialize entries"),
    )
    .expect("write test.json");
}
