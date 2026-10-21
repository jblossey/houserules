//! The `workflow` key of `.houserules.json`: which workflow an install runs,
//! at which version, the SHA-256 of each file the kit last wrote, and the
//! model of each role.
//!
//! The pins decide what `update` does with a workflow file
//! (`workflow`'s own module doc): it writes a file whose bytes equal the pin,
//! and keeps a file whose bytes differ. A path that holds the project's own
//! file maps to `null` once the kit has reported it. Every path in `files` is
//! a forward-slash path, the form of the manifest, whatever the platform.
//! `models` belongs to the project: `update` adds a role the manifest names
//! and `models` lacks, and changes no value and removes no role.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::install::marker::Marker;

/// The keys of the `workflow` object, in the order that a write puts them.
const KEYS: [&str; 4] = ["name", "version", "files", "models"];

/// The pins of the workflow that an install runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkflowPin {
    /// The workflow name, `reference`.
    pub(crate) name: String,
    /// The version of the binary that last moved the pins.
    pub(crate) version: String,
    /// The SHA-256 of the content that the kit last wrote to each workflow
    /// file, by install path. A path that holds the project's own file maps
    /// to `None`: the kit reported it once and leaves it alone (spec 4.3).
    pub(crate) files: BTreeMap<String, Option<String>>,
    /// The model of each role, as the project last set it.
    pub(crate) models: BTreeMap<String, String>,
}

/// `true` when `text` is a SHA-256 in hex: 64 hex digits.
fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The pins of the install that `marker` holds, or `None` when the marker has
/// no `workflow` key. A key of another shape is one line that names the
/// marker and the field, and the path of a file pin that is neither a SHA-256
/// string nor `null`.
pub(crate) fn read(marker: &Marker) -> Result<Option<WorkflowPin>, String> {
    let Some(value) = marker.workflow_value() else {
        return Ok(None);
    };
    let at = format!("{}: workflow", marker.file_name());
    let Some(map) = value.as_object() else {
        return Err(format!("{at} must be an object"));
    };
    if let Some(key) = map.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(format!("{at}: unknown key {key:?}"));
    }
    let text = |key: &str| -> Result<String, String> {
        match map.get(key) {
            Some(Value::String(text)) => Ok(text.clone()),
            Some(_) => Err(format!("{at}.{key} must be a string")),
            None => Err(format!("{at}: missing \"{key}\"")),
        }
    };
    let files = || -> Result<BTreeMap<String, Option<String>>, String> {
        let Some(Value::Object(entries)) = map.get("files") else {
            return Err(format!(
                "{at}.files must be an object of SHA-256 strings or null"
            ));
        };
        entries
            .iter()
            .map(|(path, value)| match value {
                Value::Null => Ok((path.clone(), None)),
                Value::String(hash) if is_sha256(hash) => Ok((path.clone(), Some(hash.clone()))),
                _ => Err(format!(
                    "{at}.files[{path:?}] must be a SHA-256 string or null"
                )),
            })
            .collect()
    };
    let models = || -> Result<BTreeMap<String, String>, String> {
        let Some(Value::Object(entries)) = map.get("models") else {
            return Err(format!("{at}.models must be an object of strings"));
        };
        entries
            .iter()
            .map(|(role, value)| match value {
                Value::String(model) => Ok((role.clone(), model.clone())),
                _ => Err(format!("{at}.models must be an object of strings")),
            })
            .collect()
    };
    Ok(Some(WorkflowPin {
        name: text("name")?,
        version: text("version")?,
        files: files()?,
        models: models()?,
    }))
}

/// Sets the `workflow` key of `marker` to `pin`. The key keeps its place when
/// it exists and goes last when it does not.
pub(crate) fn write(marker: &mut Marker, pin: &WorkflowPin) {
    let mut map = Map::new();
    map.insert("name".to_string(), json!(pin.name));
    map.insert("version".to_string(), json!(pin.version));
    map.insert("files".to_string(), json!(pin.files));
    map.insert("models".to_string(), json!(pin.models));
    marker.set_workflow_value(Value::Object(map));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::baseline;

    /// A fresh marker with no file: `Marker::read_or_default` of an absent
    /// marker.
    fn marker_with(workflow: Option<Value>) -> (tempfile::TempDir, Marker) {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut marker = Marker::read_or_default(dir.path(), "WI").expect("a default marker");
        if let Some(value) = workflow {
            marker.set_workflow_value(value);
        }
        (dir, marker)
    }

    fn pin() -> WorkflowPin {
        WorkflowPin {
            name: "reference".to_string(),
            version: "2.0.0".to_string(),
            files: BTreeMap::from([
                (
                    ".claude/agents/implementer.md".to_string(),
                    Some(baseline::hash(b"kit")),
                ),
                (".claude/agents/task-reviewer.md".to_string(), None),
            ]),
            models: BTreeMap::from([("implementer".to_string(), "sonnet".to_string())]),
        }
    }

    /// A marker with no `workflow` key holds no pins.
    #[test]
    fn a_marker_without_the_key_holds_no_pins() {
        let (_dir, marker) = marker_with(None);
        assert_eq!(read(&marker), Ok(None));
    }

    /// A write reads back the same pins, and the object keeps the order of
    /// the keys that the spec names.
    #[test]
    fn a_write_reads_back_and_keeps_the_key_order() {
        let (_dir, mut marker) = marker_with(None);
        write(&mut marker, &pin());
        assert_eq!(read(&marker), Ok(Some(pin())));
        let keys: Vec<&String> = marker
            .workflow_value()
            .and_then(Value::as_object)
            .expect("an object")
            .keys()
            .collect();
        assert_eq!(keys, KEYS);
    }

    /// Each fault of shape is one line that names the marker and the field.
    #[test]
    fn a_key_of_the_wrong_shape_is_one_named_line() {
        let cases = [
            (json!("reference"), "workflow must be an object"),
            (
                json!({"name": "reference", "version": "1", "files": {}, "models": {}, "x": 1}),
                "workflow: unknown key \"x\"",
            ),
            (
                json!({"version": "1", "files": {}, "models": {}}),
                "workflow: missing \"name\"",
            ),
            (
                json!({"name": 5, "version": "1", "files": {}, "models": {}}),
                "workflow.name must be a string",
            ),
            (
                json!({"name": "r", "version": "1", "files": [], "models": {}}),
                "workflow.files must be an object of SHA-256 strings or null",
            ),
            (
                json!({"name": "r", "version": "1", "files": {"a.md": 1}, "models": {}}),
                "workflow.files[\"a.md\"] must be a SHA-256 string or null",
            ),
            (
                json!({"name": "r", "version": "1", "files": {"a.md": "ab"}, "models": {}}),
                "workflow.files[\"a.md\"] must be a SHA-256 string or null",
            ),
            (
                json!({"name": "r", "version": "1", "files": {}, "models": {"a": 1}}),
                "workflow.models must be an object of strings",
            ),
        ];
        for (value, expected) in cases {
            let (dir, marker) = marker_with(Some(value.clone()));
            let named = format!(
                "{}: {expected}",
                dir.path().join(".houserules.json").display()
            );
            assert_eq!(read(&marker), Err(named), "{value}");
        }
    }
}
