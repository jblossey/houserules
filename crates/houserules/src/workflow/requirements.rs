//! The requirements of a workflow (spec 4.3): the rule ids that its files
//! cite. A requirement installs nothing. `check-knowledge` asks that each
//! required id comes from an installed, non-excluded entry (a local entry
//! counts), and `init --workflow` says which packs of the kit hold the ids
//! that the install lacks.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::install::hand_steps;
use crate::install::marker::{Marker, Pin};
use crate::pack::TRACKER_PACK;
use crate::packs::kit_packs;
use crate::rules::Base;
use crate::tracker::lines::pack_needs_tracker;

use super::{Manifest, needs_tracker_at_check, pins, reference};

/// Each id that a pack of the kit ships, with the first pack by name that
/// ships it.
fn kit_providers() -> BTreeMap<String, String> {
    let mut providers = BTreeMap::new();
    for pack in kit_packs() {
        for entry in &pack.entries {
            if let Some(id) = entry.get("id").and_then(Value::as_str) {
                providers
                    .entry(id.to_string())
                    .or_insert_with(|| pack.name.clone());
            }
        }
    }
    providers
}

/// One line for each id that `manifest` requires and `base` does not
/// provide, in id order, by the first state that applies (spec 4.3):
///
/// - the project excludes the id in `knowledge/overrides.json`;
/// - a pack of the kit ships the id and the install lacks the pack;
/// - no pack of the kit ships the id, so the project provides it itself.
///
/// An id that a retired or superseded entry holds is left out: the
/// kit-citation lint names that case with its own line.
pub(crate) fn unmet(base: &Base, manifest: &Manifest) -> Vec<String> {
    let providers = kit_providers();
    let workflow = &manifest.name;
    let mut ids: Vec<&String> = manifest.requires.iter().collect();
    ids.sort();
    ids.into_iter()
        .filter(|id| !base.entries.contains_key(*id) && !base.inactive.contains_key(*id))
        .map(|id| {
            if base.excluded.contains_key(id) {
                hand_steps::workflow_excludes_a_required_id(workflow, id)
            } else if let Some(pack) = providers.get(id) {
                format!(
                    "workflow {workflow} requires {id}: no installed pack provides it; run houserules pack add {pack}"
                )
            } else {
                format!("workflow {workflow} requires {id}: add it to the local pack")
            }
        })
        .collect()
}

/// The ids that `manifest` requires and that the project excludes in
/// `knowledge/overrides.json`, in id order. The adoption of a workflow names
/// each one in a hand step (spec 4.7, step 2).
pub(crate) fn excluded_ids<'a>(base: &Base, manifest: &'a Manifest) -> Vec<&'a String> {
    let mut ids: Vec<&String> = manifest
        .requires
        .iter()
        .filter(|id| base.excluded.contains_key(*id))
        .collect();
    ids.sort();
    ids
}

/// The findings of `check-knowledge` about the installed workflow and the
/// packs that need a tracker (spec 3.10, 4.3). The process pack with no
/// tracker gets its line; the workflow gets the line of each unmet
/// requirement, and a line of its own when the install has no tracker. An
/// install with neither gets nothing.
///
/// A marker that cannot give its tracker or its workflow gives nothing here:
/// the pin check, the kit-citation lint, and the render check each name the
/// defect of the marker.
pub(crate) fn findings(base: &Base) -> Vec<String> {
    let Ok(marker) = Marker::read_or_default_for_gate(&base.root, "WI") else {
        return Vec::new();
    };
    let has_tracker = matches!(marker.tracker(), Ok(Some(_)));
    let mut findings = Vec::new();
    if !has_tracker && base.packs.iter().any(|pack| pack.name == TRACKER_PACK) {
        findings.push(pack_needs_tracker(TRACKER_PACK));
    }
    if matches!(pins::read(&marker), Ok(Some(_))) {
        let manifest = reference();
        findings.extend(unmet(base, &manifest));
        if !has_tracker {
            findings.push(needs_tracker_at_check(&manifest));
        }
    }
    findings
}

/// The packs of the kit that ship an id of `manifest.requires` and that
/// `installed` lacks, sorted by name.
pub(crate) fn packs_to_add(manifest: &Manifest, installed: &BTreeMap<String, Pin>) -> Vec<String> {
    let providers = kit_providers();
    let packs: BTreeSet<&String> = manifest
        .requires
        .iter()
        .filter_map(|id| providers.get(id))
        .filter(|pack| !installed.contains_key(*pack))
        .collect();
    packs.into_iter().cloned().collect()
}

/// The line that `init --workflow` prints for the packs it did not add:
/// `packs` is the result of `packs_to_add`.
pub(crate) fn add_packs_line(manifest: &Manifest, packs: &[String]) -> String {
    let name = &manifest.name;
    format!(
        "the {name} workflow requires rules from the packs {}; run houserules init --workflow {name} --add-required-packs to add them",
        packs.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::rules::load_base;
    use crate::workflow::reference;

    /// A pack copy `name` that ships the entries `ids`, written below `root`.
    fn write_pack(root: &std::path::Path, name: &str, ids: &[&str]) {
        let entries: Vec<Value> = ids
            .iter()
            .map(|id| {
                json!({
                    "id": id, "kind": "rule", "area": "global", "standing": false,
                    "summary": "A rule.", "body": ["A body."], "tags": [],
                    "source": {"date": "2026-10-20", "by": "docs"},
                })
            })
            .collect();
        let pack = json!({
            "name": name, "version": "1.0.0", "schema": "houserules/pack@1.0.0",
            "topics": {name: {"title": "A pack."}}, "areas": {}, "entries": entries,
        });
        let packs = root.join("knowledge/packs");
        fs::create_dir_all(&packs).expect("create knowledge/packs");
        fs::write(
            packs.join(format!("{name}.json")),
            serde_json::to_string(&pack).expect("serialize"),
        )
        .expect("write a pack copy");
    }

    /// A base that installs the pack copies `packs` (name, ids) and excludes
    /// the ids `excluded`.
    fn base_of(packs: &[(&str, &[&str])], excluded: &[&str]) -> (tempfile::TempDir, Base) {
        let dir = tempfile::tempdir().expect("tempdir");
        let knowledge = dir.path().join("knowledge");
        fs::create_dir_all(&knowledge).expect("create knowledge/");
        fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
        fs::write(knowledge.join("areas.json"), "{}").expect("write areas.json");
        for (name, ids) in packs {
            write_pack(dir.path(), name, ids);
        }
        if !excluded.is_empty() {
            let exclusions: Vec<Value> = excluded.iter().map(|id| json!({"id": id})).collect();
            fs::write(
                knowledge.join("overrides.json"),
                serde_json::to_string(&json!({
                    "schema": "houserules/overrides@1.0.0", "exclusions": exclusions,
                }))
                .expect("serialize"),
            )
            .expect("write overrides.json");
        }
        let base = load_base(dir.path()).expect("load the base");
        (dir, base)
    }

    /// A manifest that requires `ids`.
    fn requiring(ids: &[&str]) -> Manifest {
        let mut manifest = reference();
        manifest.requires = ids.iter().map(|id| id.to_string()).collect();
        manifest
    }

    /// A required id that an installed entry provides gives no line.
    #[test]
    fn a_provided_id_gives_no_line() {
        let (_dir, base) = base_of(&[("process", &["process.tdd"])], &[]);
        assert_eq!(
            unmet(&base, &requiring(&["process.tdd"])),
            Vec::<String>::new()
        );
    }

    /// Spec 4.3, state one: no installed pack provides an id that a pack of
    /// the kit ships. The line names the workflow, the id, and the pack.
    #[test]
    fn an_id_of_a_pack_the_install_lacks_names_the_pack_to_add() {
        let (_dir, base) = base_of(&[], &[]);
        assert_eq!(
            unmet(&base, &requiring(&["process.tdd"])),
            [
                "workflow reference requires process.tdd: no installed pack provides it; run houserules pack add process"
            ]
        );
    }

    /// Spec 4.3, state two: the project excludes the id. The exclusion wins
    /// over the pack hint, because the pack is installed.
    #[test]
    fn an_excluded_id_names_the_exclusion() {
        let (_dir, base) = base_of(
            &[("process", &["process.tdd", "process.risk-tiers"])],
            &["process.tdd"],
        );
        assert_eq!(
            unmet(&base, &requiring(&["process.tdd", "process.risk-tiers"])),
            [
                "workflow reference requires process.tdd, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it"
            ]
        );
    }

    /// Spec 4.3, state three: no pack of the kit ships the id, so the project
    /// provides it in its local pack.
    #[test]
    fn an_id_that_no_pack_of_the_kit_ships_asks_for_the_local_pack() {
        let (_dir, base) = base_of(&[], &[]);
        assert_eq!(
            unmet(&base, &requiring(&["team.house-rule"])),
            ["workflow reference requires team.house-rule: add it to the local pack"]
        );
    }

    /// The lines come in id order, whatever the order of `requires`.
    #[test]
    fn the_lines_come_in_id_order() {
        let (_dir, base) = base_of(&[], &[]);
        let lines = unmet(&base, &requiring(&["process.tdd", "process.risk-tiers"]));
        assert!(lines[0].contains("process.risk-tiers"), "{lines:?}");
        assert!(lines[1].contains("process.tdd"), "{lines:?}");
    }

    /// An id that a retired entry holds is the lint's line, not this one.
    #[test]
    fn a_retired_entry_gives_no_line_here() {
        let (dir, _base) = base_of(&[("process", &["process.tdd"])], &[]);
        fs::write(
            dir.path().join("knowledge/overrides.json"),
            serde_json::to_string(&json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [{"id": "process.tdd", "fields": {"status": "retired"}}],
            }))
            .expect("serialize"),
        )
        .expect("write overrides.json");
        let base = load_base(dir.path()).expect("load the base");
        assert_eq!(
            unmet(&base, &requiring(&["process.tdd"])),
            Vec::<String>::new()
        );
    }

    /// The packs to add are the kit packs that ship a required id and that
    /// the install lacks, once each, in name order.
    #[test]
    fn the_packs_to_add_are_the_kit_packs_the_install_lacks() {
        let installed = BTreeMap::from([(
            "knowledge-base".to_string(),
            Pin {
                version: "1.0.0".to_string(),
                source: "kit".to_string(),
                sha256: String::new(),
                added_areas: Vec::new(),
                commit: None,
            },
        )]);
        let manifest = reference();
        assert_eq!(
            packs_to_add(&manifest, &installed),
            ["process", "security-hygiene", "writing-style"]
        );
        assert_eq!(
            add_packs_line(&manifest, &packs_to_add(&manifest, &installed)),
            "the reference workflow requires rules from the packs process, security-hygiene, writing-style; run houserules init --workflow reference --add-required-packs to add them"
        );
    }
}
