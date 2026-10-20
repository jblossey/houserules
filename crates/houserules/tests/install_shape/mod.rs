//! The install shape that a command reads. D4 (spec section 5) refuses a tree
//! whose install schema is missing or older, so a test that builds a tree by
//! hand and runs a command on it first makes the tree a 2.0 install with
//! [`mark_2_0`]. Every test file that needs it includes this module, as
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
