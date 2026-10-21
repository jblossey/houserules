//! The contents that the kit shipped, and step 8 of the 1.x conversion: D3,
//! first time (batch 33 spec section 5).
//!
//! The binary carries every content that a 1.x release shipped of the files
//! that the kit writes and the project may have kept, and the content of each
//! at the batch-33 main (`crates/houserules/released/`, `index.json`):
//! [`known_contents`] reads them. A file whose bytes equal a known content,
//! after the id-prefix rewrite of the install, is the kit's (batch 34 spec
//! 4.7). It also carries every text of a pack entry that a 1.x release shipped
//! ([`released_entries`]), the base from which the conversion decides which
//! fields of an entry the project changed.
//!
//! Step 8 compares the copy of the knowledge schema and of the seeded CI
//! workflow on disk with the known contents and decides one [`Action`] for each
//! file. The knowledge schema is compared without `$defs.area`, because a 1.x
//! project extended that enum with its own areas, and 2.0 reads the areas from
//! `knowledge/areas.json`. Later runs of `update` deliver both files through
//! the path-baseline sync (`install::sync`).

use std::collections::BTreeMap;

use rust_embed::RustEmbed;
use serde_json::Value;

use crate::emit::emit;

use super::super::hand_steps;
use super::super::with_id_prefix;
use super::plan::owned_path_is_absent;

/// The released contents of the kit, embedded from
/// `crates/houserules/released/`.
#[derive(RustEmbed)]
#[folder = "released/"]
struct Released;

/// One distinct text of a pack entry that a 1.x release shipped.
#[derive(Debug, Clone)]
pub(super) struct ReleasedEntry {
    /// The 1.x releases whose topic files held this text, without the `v`.
    pub(super) versions: Vec<String>,
    /// The entry.
    pub(super) entry: Value,
}

/// Every text of every pack entry that a 1.x release shipped, by entry id. The
/// embedded file is a build input, so a defect of it is a build defect, and a
/// test reads it.
pub(super) fn released_entries() -> BTreeMap<String, Vec<ReleasedEntry>> {
    let file = Released::get("entries.json").expect("the released entries are embedded");
    let all: BTreeMap<String, Vec<Value>> =
        serde_json::from_slice(&file.data).expect("the released entries are valid JSON");
    all.into_iter()
        .map(|(id, texts)| {
            let texts = texts
                .into_iter()
                .map(|text| ReleasedEntry {
                    versions: text["versions"]
                        .as_array()
                        .expect("the versions of a released entry")
                        .iter()
                        .map(|version| version.as_str().expect("a version").to_string())
                        .collect(),
                    entry: text["entry"].clone(),
                })
                .collect();
            (id, texts)
        })
        .collect()
}

/// The path of the knowledge schema below the install root.
pub(super) const SCHEMA_PATH: &str = "knowledge/schema.json";

/// The path of the seeded CI workflow below the git top level, in the form of
/// an install at the top level.
pub(super) const WORKFLOW_PATH: &str = ".github/workflows/knowledge.yml";

/// What step 8 does with one D3 file.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Action {
    /// The CI workflow does not exist: there is nothing to convert.
    Absent,
    /// The project owns the path: the file stays as found.
    Owned,
    /// The file holds the content of 2.0 already: only the baseline is
    /// recorded.
    Baseline,
    /// The file holds a released content, or it is the absent knowledge
    /// schema: it is written with the content of 2.0 and baselined.
    Replace,
    /// The knowledge schema loses `$defs.area` and the `$ref` of its area
    /// property. The file gets these bytes and no baseline.
    Edit(Vec<u8>),
    /// The file stays as found.
    Keep,
}

/// One decision of step 8: the action, and the hand step that the project has
/// left.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Decision {
    pub(super) action: Action,
    pub(super) hand_step: Option<String>,
}

impl Decision {
    /// A decision with no hand step.
    fn of(action: Action) -> Self {
        Self {
            action,
            hand_step: None,
        }
    }

    /// A decision that leaves `hand_step` to the project.
    fn with_hand_step(action: Action, hand_step: String) -> Self {
        Self {
            action,
            hand_step: Some(hand_step),
        }
    }
}

/// Every content of the install path `path` that a 1.x release shipped, and
/// the content that the batch-33 main holds, once each, with the id-prefix
/// rewrite of `prefix`. A path that the index does not name has none. The
/// content of the running kit is no part of the list: the caller holds it. The
/// embedded index names the files; a defect of it is a build defect, and a test
/// reads every content.
pub(crate) fn known_contents(path: &str, prefix: &str) -> Result<Vec<Vec<u8>>, String> {
    let index = Released::get("index.json").expect("the released index is embedded");
    let index: Value =
        serde_json::from_slice(&index.data).expect("the embedded released index is valid JSON");
    let Some(rows) = index[path].as_array() else {
        return Ok(Vec::new());
    };
    rows.iter()
        .map(|row| {
            let name = row["file"].as_str().expect("a released file name");
            let content = Released::get(name)
                .unwrap_or_else(|| panic!("{name} is embedded"))
                .data
                .into_owned();
            with_id_prefix(path, content, prefix)
        })
        .collect()
}

/// `value` without `$defs.area`.
fn without_area_enum(mut value: Value) -> Value {
    if let Some(defs) = value.get_mut("$defs").and_then(Value::as_object_mut) {
        defs.shift_remove("area");
    }
    value
}

/// `true` when the schema `value` holds `$defs.area`.
fn has_area_enum(value: &Value) -> bool {
    value.pointer("/$defs/area").is_some()
}

/// The path of the `area` property of an entry in the knowledge schema.
const AREA_PROPERTY_POINTER: &str = "/$defs/entry/properties/area";

/// The schema `value` without `$defs.area` and with the `$ref` of its area
/// property replaced by `area_property`, the definition of the schema of 2.0.
/// The property stays as it is when it holds no `$ref`.
fn with_plain_area(value: Value, area_property: &Value) -> Value {
    let mut value = without_area_enum(value);
    if let Some(area) = value.pointer_mut(AREA_PROPERTY_POINTER)
        && area.get("$ref").is_some()
    {
        *area = area_property.clone();
    }
    value
}

/// The decision for the knowledge schema. `disk` is the file, `owned` is
/// `true` when `ownedPaths` lists it, `kit` is the schema of 2.0 with the
/// id-prefix rewrite applied, and `prefix` is the id prefix of the install.
/// An absent schema is put back, since the conversion loads the knowledge
/// base with it; an absent schema that `ownedPaths` lists is one line, and the
/// conversion stops before it writes. An unreadable released index or content
/// is one line.
pub(super) fn decide_schema(
    disk: Option<&[u8]>,
    owned: bool,
    kit: &[u8],
    prefix: &str,
) -> Result<Decision, String> {
    let Some(disk) = disk else {
        return if owned {
            Err(owned_path_is_absent(SCHEMA_PATH))
        } else {
            Ok(Decision::of(Action::Replace))
        };
    };
    let parsed: Option<Value> = serde_json::from_slice(disk)
        .ok()
        .filter(|value: &Value| value.is_object());
    let kit_value: Value =
        serde_json::from_slice(kit).expect("the knowledge schema of the kit is valid JSON");
    let area_property = kit_value
        .pointer(AREA_PROPERTY_POINTER)
        .cloned()
        .expect("the knowledge schema of the kit defines the area property");
    if owned {
        let property = area_property.to_string();
        return Ok(match &parsed {
            Some(value) if !has_area_enum(value) => Decision::of(Action::Owned),
            _ => Decision::with_hand_step(
                Action::Owned,
                hand_steps::schema_owned_with_area_enum(&property),
            ),
        });
    }
    if disk == kit {
        return Ok(Decision::of(Action::Baseline));
    }
    let Some(parsed) = parsed else {
        return Ok(Decision::with_hand_step(
            Action::Keep,
            hand_steps::schema_unreadable(),
        ));
    };
    let disk_without_enum = without_area_enum(parsed.clone());
    for released in known_contents(SCHEMA_PATH, prefix)? {
        if let Ok(released) = serde_json::from_slice::<Value>(&released)
            && without_area_enum(released) == disk_without_enum
        {
            return Ok(Decision::of(Action::Replace));
        }
    }
    let edited = emit(&with_plain_area(parsed.clone(), &area_property)).into_bytes();
    if serde_json::from_slice::<Value>(&edited).ok() == Some(parsed) {
        return Ok(Decision::with_hand_step(
            Action::Keep,
            hand_steps::schema_differs(),
        ));
    }
    Ok(Decision::with_hand_step(
        Action::Edit(edited),
        hand_steps::schema_with_other_changes(),
    ))
}

/// The decision for the CI workflow at `path`. `disk` is the file, `owned` is
/// `true` when `ownedPaths` lists it, `kit` is the workflow of 2.0 for the
/// install, and `prefix` is the id prefix of the install.
pub(super) fn decide_workflow(
    disk: Option<&[u8]>,
    owned: bool,
    kit: &[u8],
    path: &str,
    prefix: &str,
) -> Result<Decision, String> {
    let Some(disk) = disk else {
        return Ok(Decision::of(Action::Absent));
    };
    if owned {
        return Ok(Decision::of(Action::Owned));
    }
    if disk == kit {
        return Ok(Decision::of(Action::Baseline));
    }
    if known_contents(WORKFLOW_PATH, prefix)?
        .iter()
        .any(|released| released == disk)
    {
        return Ok(Decision::of(Action::Replace));
    }
    Ok(Decision::with_hand_step(
        Action::Keep,
        hand_steps::workflow_differs(path),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::Path;

    use serde_json::json;

    fn released(name: &str) -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("released")
                .join(name),
        )
        .expect("read a released file")
    }

    fn kit_schema() -> Vec<u8> {
        fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json"))
            .expect("read the kit schema")
    }

    fn kit_workflow() -> Vec<u8> {
        fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../template/.github/workflows/knowledge.yml"),
        )
        .expect("read the kit workflow")
    }

    /// The released schema as a value, to edit in a test.
    fn released_schema() -> Value {
        serde_json::from_slice(&released("knowledge-schema-1.0.0.json")).expect("parse")
    }

    /// The kit schema holds no area enum, and its area property is the plain
    /// string that the edit of a schema with other changes takes over.
    #[test]
    fn the_kit_schema_holds_a_plain_area_property_and_no_area_enum() {
        let kit: Value = serde_json::from_slice(&kit_schema()).expect("parse");

        assert_eq!(
            kit.pointer(AREA_PROPERTY_POINTER),
            Some(&json!({"type": "string", "minLength": 1}))
        );
        assert!(!has_area_enum(&kit));
    }

    /// A schema that equals a released content is replaced.
    #[test]
    fn a_released_schema_is_replaced() {
        let disk = released("knowledge-schema-1.0.0.json");

        let decision = decide_schema(Some(&disk), false, &kit_schema(), "WI").unwrap();

        assert_eq!(decision, Decision::of(Action::Replace));
    }

    /// A schema whose area enum holds the areas of the project is a match:
    /// the comparison leaves `$defs.area` out.
    #[test]
    fn a_released_schema_with_its_own_areas_is_replaced() {
        let mut schema = released_schema();
        schema["$defs"]["area"]["enum"]
            .as_array_mut()
            .unwrap()
            .push(json!("backend"));
        let disk = emit(&schema).into_bytes();

        let decision = decide_schema(Some(&disk), false, &kit_schema(), "WI").unwrap();

        assert_eq!(decision, Decision::of(Action::Replace));
    }

    /// A match ignores the formatting of the file.
    #[test]
    fn a_reformatted_released_schema_is_replaced() {
        let disk = serde_json::to_vec(&released_schema()).unwrap();

        let decision = decide_schema(Some(&disk), false, &kit_schema(), "WI").unwrap();

        assert_eq!(decision, Decision::of(Action::Replace));
    }

    /// A schema with other changes loses `$defs.area` and the `$ref` of its
    /// area property and nothing else; the rest is a hand step.
    #[test]
    fn a_schema_with_other_changes_loses_only_the_area_enum_and_its_ref() {
        let mut schema = released_schema();
        schema["$defs"]["entry"]["properties"]["summary"]["maxLength"] = json!(200);
        let disk = emit(&schema).into_bytes();

        let decision = decide_schema(Some(&disk), false, &kit_schema(), "WI").unwrap();

        let Action::Edit(bytes) = &decision.action else {
            panic!("expected an edit: {decision:?}");
        };
        let mut expected = schema.clone();
        expected["$defs"]
            .as_object_mut()
            .unwrap()
            .shift_remove("area");
        expected["$defs"]["entry"]["properties"]["area"] =
            json!({"type": "string", "minLength": 1});
        assert_eq!(serde_json::from_slice::<Value>(bytes).unwrap(), expected);
        assert_eq!(
            decision.hand_step,
            Some(hand_steps::schema_with_other_changes())
        );
    }

    /// A schema in `ownedPaths` stays as found. The project removes
    /// `$defs.area` by hand while the file holds it, and has nothing to do
    /// once it does not.
    #[test]
    fn an_owned_schema_stays_and_gets_a_hand_step_while_it_holds_the_area_enum() {
        let disk = released("knowledge-schema-1.0.0.json");
        let property = json!({"type": "string", "minLength": 1}).to_string();

        let with_enum = decide_schema(Some(&disk), true, &kit_schema(), "WI").unwrap();
        let without_enum = decide_schema(Some(&kit_schema()), true, &kit_schema(), "WI").unwrap();

        assert_eq!(
            with_enum,
            Decision::with_hand_step(
                Action::Owned,
                hand_steps::schema_owned_with_area_enum(&property)
            )
        );
        assert_eq!(without_enum, Decision::of(Action::Owned));
    }

    /// A schema that is already the kit's is baselined, which is the state
    /// that a rerun finds after an interruption.
    #[test]
    fn the_kit_schema_is_only_baselined() {
        let decision = decide_schema(Some(&kit_schema()), false, &kit_schema(), "WI").unwrap();

        assert_eq!(decision, Decision::of(Action::Baseline));
    }

    /// A schema that does not exist is put back (spec C21): the conversion
    /// needs it to load the knowledge base. A schema that is no JSON object is
    /// kept with a hand step.
    #[test]
    fn an_absent_schema_is_restored_and_a_broken_one_is_kept() {
        assert_eq!(
            decide_schema(None, false, &kit_schema(), "WI").unwrap(),
            Decision::of(Action::Replace)
        );
        assert_eq!(
            decide_schema(Some(b"[1]"), false, &kit_schema(), "WI").unwrap(),
            Decision::with_hand_step(Action::Keep, hand_steps::schema_unreadable())
        );
    }

    /// A schema that `ownedPaths` lists and that does not exist stops the
    /// conversion with one line that names the file and the key (spec C21).
    #[test]
    fn an_owned_schema_that_is_absent_stops_the_conversion_with_one_line() {
        assert_eq!(
            decide_schema(None, true, &kit_schema(), "WI"),
            Err("knowledge/schema.json: this file does not exist, and .houserules.json lists it in ownedPaths (the key is overrides in a 1.x marker), so update cannot restore it; restore it, or remove it from ownedPaths, then run houserules update again".to_string())
        );
    }

    /// A schema that already lost the area enum and differs from the kit's is
    /// kept: no edit applies.
    #[test]
    fn a_schema_with_nothing_left_to_remove_is_kept_with_its_own_hand_step() {
        let mut schema: Value = serde_json::from_slice(&kit_schema()).unwrap();
        schema["title"] = json!("Mine");
        let disk = emit(&schema).into_bytes();

        let decision = decide_schema(Some(&disk), false, &kit_schema(), "WI").unwrap();

        assert_eq!(
            decision,
            Decision::with_hand_step(Action::Keep, hand_steps::schema_differs())
        );
    }

    /// Each released workflow is replaced. A workflow of the kit is only
    /// baselined. Any other workflow is kept with a hand step. An owned
    /// workflow stays, without a line.
    #[test]
    fn a_released_workflow_is_replaced_and_any_other_is_kept() {
        let kit = kit_workflow();
        for name in [
            "knowledge-workflow-1.0.0.yml",
            "knowledge-workflow-1.2.0.yml",
        ] {
            let disk = released(name);
            let decision = decide_workflow(Some(&disk), false, &kit, WORKFLOW_PATH, "WI").unwrap();
            let expected = if disk == kit {
                Decision::of(Action::Baseline)
            } else {
                Decision::of(Action::Replace)
            };
            assert_eq!(decision, expected, "{name}");
        }
        let mut changed = released("knowledge-workflow-1.2.0.yml");
        changed.extend_from_slice(b"# mine\n");
        assert_eq!(
            decide_workflow(Some(&changed), false, &kit, WORKFLOW_PATH, "WI").unwrap(),
            Decision::with_hand_step(Action::Keep, hand_steps::workflow_differs(WORKFLOW_PATH))
        );
        assert_eq!(
            decide_workflow(Some(&changed), true, &kit, WORKFLOW_PATH, "WI").unwrap(),
            Decision::of(Action::Owned)
        );
        assert_eq!(
            decide_workflow(None, false, &kit, WORKFLOW_PATH, "WI").unwrap(),
            Decision::of(Action::Absent)
        );
    }

    /// Every row of the index names an embedded file, so `known_contents` reads
    /// one content for each row of each path.
    #[test]
    fn every_row_of_the_index_is_a_known_content() {
        let index: Value = serde_json::from_slice(&released("index.json")).expect("parse");
        for (path, rows) in index.as_object().expect("an object") {
            let contents = known_contents(path, "WI").expect("known contents");
            assert_eq!(
                contents.len(),
                rows.as_array().expect("rows").len(),
                "{path}"
            );
        }
    }

    /// A path that the index does not name has no known content: the kit never
    /// shipped it before the running release.
    #[test]
    fn a_path_the_index_does_not_name_has_no_known_content() {
        assert_eq!(
            known_contents("tools/houserules-step.sh", "WI"),
            Ok(Vec::new())
        );
    }

    /// The batch-33 content joins the releases, once each when two tags hold the
    /// same bytes: the deliverables schema has the content of every 1.x release
    /// and the content of the batch-33 main.
    #[test]
    fn the_batch_33_content_is_known_beside_the_released_ones() {
        assert_eq!(
            known_contents(".claude/schemas/deliverables.json", "WI")
                .expect("known contents")
                .len(),
            2
        );
        assert_eq!(
            known_contents("tools/claude-session-start.sh", "WI")
                .expect("known contents")
                .len(),
            1
        );
    }

    /// The id-prefix rewrite applies to the files that carry the prefix, as it
    /// does to the kit's own content, and to no other file.
    #[test]
    fn known_contents_carry_the_id_prefix_of_the_install() {
        let schema = ".claude/schemas/deliverables.json";
        let default = known_contents(schema, "WI").expect("known contents");
        let renamed = known_contents(schema, "HR").expect("known contents");
        assert_eq!(default.len(), renamed.len());
        for (default, renamed) in default.iter().zip(&renamed) {
            let default = String::from_utf8_lossy(default);
            assert!(default.contains("WI-"));
            assert_eq!(
                String::from_utf8_lossy(renamed),
                default.replace("WI-", "HR-")
            );
        }
        let hook = ".githooks/commit-msg";
        assert_eq!(
            known_contents(hook, "WI").expect("known contents"),
            known_contents(hook, "HR").expect("known contents")
        );
    }

    /// The released entries hold each distinct text of an entry once, with the
    /// versions that shipped it, and each text is the entry of its id.
    #[test]
    fn released_entries_hold_each_text_with_the_versions_that_shipped_it() {
        let entries = released_entries();
        let texts = &entries["process.tdd"];
        assert!(
            texts
                .iter()
                .any(|text| text.versions.iter().any(|version| version == "1.4.0")),
            "{texts:?}"
        );
        for (id, texts) in &entries {
            for (at, text) in texts.iter().enumerate() {
                assert_eq!(text.entry["id"], json!(id));
                assert!(!text.versions.is_empty(), "{id}");
                assert!(
                    texts[at + 1..]
                        .iter()
                        .all(|other| other.entry != text.entry),
                    "{id} holds one text twice"
                );
            }
        }
    }
}
