//! The install shape that a command reads. D4 (spec section 5) refuses a tree
//! whose install schema is missing or older, so a test that builds a tree by
//! hand and runs a command on it first makes the tree a 2.0 install with
//! [`mark_2_0`]; a test that runs a backlog command on it also gives it the
//! JSON tracker with [`mark_json_tracker`]. Every test file that needs it includes this module, as
//! `tests/workflow_reader/mod.rs` is shared, and keeps its own other helpers.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

/// Makes the tree under `dir` a 2.0 install: its `.houserules.json` holds
/// the install `schema` key. A marker that exists keeps every other key; an
/// absent marker is created with the key alone.
pub fn mark_2_0(dir: &Path) {
    let path = dir.join(".houserules.json");
    let mut marker: Map<String, Value> = match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).expect("the marker is a JSON object"),
        Err(_) => Map::new(),
    };
    marker.insert("schema".to_string(), json!("houserules/install@2.0.0"));
    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&Value::Object(marker)).expect("serialize the marker")
        ),
    )
    .expect("write the marker");
}

/// Gives the install under `dir` the JSON tracker: its `.houserules.json`
/// holds `tracker: {"adapter": "json"}`. The backlog commands need a tracker
/// (batch 34, spec 3.4); a marker that exists keeps every other key. Only the
/// test files that run a backlog command call it, so the others read it as
/// dead code.
#[allow(dead_code)]
pub fn mark_json_tracker(dir: &Path) {
    let path = dir.join(".houserules.json");
    let mut marker: Map<String, Value> =
        serde_json::from_str(&fs::read_to_string(&path).expect("read the marker"))
            .expect("the marker is a JSON object");
    marker.insert("tracker".to_string(), json!({"adapter": "json"}));
    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&Value::Object(marker)).expect("serialize the marker")
        ),
    )
    .expect("write the marker");
}
