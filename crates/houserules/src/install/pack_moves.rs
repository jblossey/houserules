//! The pack moves of `update` on a 2.0 install (spec section 5).
//!
//! `update` moves the pin of each kit pack to the version of the binary,
//! rewrites its copy, keeps `knowledge/overrides.json`, and applies the area
//! rules of spec section 6, as `pack upgrade` does for an upstream pack. A
//! pin is a kit pin by its `source`, never by the name of its pack, so an
//! upstream pack that carries a kit name is no subject of this step. The step
//! fetches nothing: it reads the packs the binary carries.
//!
//! Three states keep a pack as it is:
//!
//! - the new version ships an id that another installed pack ships: the
//!   kit pack stays at its old pin and copy, and a hand step names the id and
//!   both packs (the next `update` tests the state again);
//! - the copy fails its `sha256` at the version of the pin: the copy stays,
//!   and the line of `check-knowledge` says to put the change in
//!   `knowledge/overrides.json`;
//! - the pack has no pin, or its pin has another source: the project removed
//!   it, or it is an upstream pack, and `update` never installs a pack.
//!
//! A kit pin with no copy gets its copy at the version of the binary, so
//! deleting a hand-edited copy and running `update` restores it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use crate::archive::write_json;
use crate::pack::areas::{add_pack_areas, areas_to_add, remove_pack_areas};
use crate::pack::opt_in;
use crate::pack::source::PackSource;
use crate::packs::{PackCopy, kit_packs, pack_sha256};
use crate::rules::{load_base, pin_mismatch_line};

use super::hand_steps;
use super::join_components;
use super::marker::{Marker, Pin};

/// The ids that `pack` ships.
fn ids_of(pack: &PackCopy) -> BTreeSet<&str> {
    pack.entries
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect()
}

/// Moves the kit pins of the install at `root` as this module describes, and
/// prints one line for each pin it moves and each copy it restores or
/// refuses to rewrite. The marker on disk is the one the step reads and
/// writes. Returns the hand steps, in the order the step found them.
pub(super) fn move_kit_packs(root: &Path) -> Result<Vec<String>, String> {
    let mut hand_steps = Vec::new();
    for kit in kit_packs() {
        let marker = Marker::read(root)?;
        let mut pins = marker.pins()?;
        let Some(pin) = pins.remove(&kit.name) else {
            continue;
        };
        if pin.source != "kit" {
            continue;
        }
        let base = load_base(root).map_err(|error| error.to_string())?;
        let copy = base.packs.iter().find(|pack| pack.name == kit.name);
        if let Some(copy) = copy
            && pin.version == kit.version
        {
            if pin.sha256 != pack_sha256(&copy.raw) {
                println!("{}", pin_mismatch_line(&kit.name));
            }
            continue;
        }
        if pin.version != kit.version
            && let Some(blocked) = shares_an_id(&kit, &pin.version, &base.packs, &pins)
        {
            hand_steps.extend(blocked);
            continue;
        }
        move_pack(root, marker, &kit, &pin.version, copy, &base.packs)?;
    }
    Ok(hand_steps)
}

/// The hand steps for each id that the version of the binary of `kit` ships
/// and another installed pack ships, or `None` when there is none. `old` is
/// the version of the pin, and `pins` holds the pins of the other packs: the
/// source of a pin decides the command that drops the id from its pack.
fn shares_an_id(
    kit: &PackCopy,
    old: &str,
    installed: &[PackCopy],
    pins: &BTreeMap<String, Pin>,
) -> Option<Vec<String>> {
    let new_ids = ids_of(kit);
    let steps: Vec<String> = installed
        .iter()
        .filter(|other| other.name != kit.name)
        .flat_map(|other| {
            let source = pins
                .get(&other.name)
                .and_then(|pin| PackSource::parse(&pin.source).ok());
            let shared: Vec<String> = ids_of(other)
                .intersection(&new_ids)
                .map(|id| {
                    hand_steps::kit_pack_shares_an_id(
                        &kit.name,
                        &kit.version,
                        old,
                        id,
                        &other.name,
                        source.as_ref(),
                    )
                })
                .collect();
            shared
        })
        .collect();
    (!steps.is_empty()).then_some(steps)
}

/// Writes the copy of `kit`, moves its pin from `old` to the version of the
/// binary, and applies the area rules for the old copy `previous` (none when
/// the copy was missing). The pin keeps the areas that the pack added and
/// gains the ones the new version adds. `installed` holds the copies the
/// install had before the move, the old copy of `kit` among them.
///
/// The writes come in this order, and the pin moves to the new version last
/// (spec C20):
///
/// 1. the pin, still at the old version, with the areas that the move is about
///    to add in its `addedAreas`;
/// 2. the exclusion of each opt-in entry that the new version lists
///    (`knowledge/overrides.json`), before the copy that ships the entry;
/// 3. the areas that the new version adds;
/// 4. the new copy;
/// 5. the removal of the areas that the new version drops, and of their record;
/// 6. the withdrawal of each untaken offer whose entry no installed pack ships
///    any more: the record, then the exclusion;
/// 7. one write of the marker that holds the record of the offer and moves the
///    version and the `sha256` of the pin.
///
/// No stop leaves an opt-in entry live: the exclusion comes before the copy.
/// The pin never moves without the record of the offer: a run that stops after
/// the exclusion is written finds the pin at the old version, offers again,
/// and finds the exclusion in place.
///
/// A run that stops after any write leaves a pin at the old version, which the
/// next run moves again: the record names every area that an earlier run
/// added, and the removal tests the pin's record against the new copy, so each
/// write is safe to repeat. The withdrawal tests the exclusions against the
/// installed packs, so it finishes after a stop too.
fn move_pack(
    root: &Path,
    mut marker: Marker,
    kit: &PackCopy,
    old: &str,
    previous: Option<&PackCopy>,
    installed: &[PackCopy],
) -> Result<(), String> {
    let mut pins = marker.pins()?;
    let pin = pins
        .get_mut(&kit.name)
        .expect("the caller found the pin of the pack");
    let mut record = pin.added_areas.clone();
    for area in areas_to_add(root, kit)? {
        if !record.contains(&area) {
            record.push(area);
        }
    }
    pin.added_areas = record.clone();
    marker.set_pins(&pins);
    marker.write()?;
    opt_in::exclude(root, &marker, kit)?;
    let area_additions = add_pack_areas(root, kit, &mut record)?;
    let relative = format!("knowledge/packs/{}.json", kit.name);
    write_json(&join_components(root, &relative), &kit.raw)?;

    let mut area_removals = Vec::new();
    if let Some(previous) = previous {
        let base = load_base(root).map_err(|error| error.to_string())?;
        area_removals = remove_pack_areas(root, previous, &base.packs, &base)?;
    }
    // The removal of areas wrote the marker on its own: read it again.
    let mut marker = Marker::read(root)?;
    let mut after_the_move: Vec<PackCopy> = installed
        .iter()
        .filter(|pack| pack.name != kit.name)
        .cloned()
        .collect();
    after_the_move.push(kit.clone());
    opt_in::withdraw_orphans(root, &mut marker, &after_the_move)?;
    let mut pins = marker.pins()?;
    let pin = pins
        .get_mut(&kit.name)
        .expect("the pin of the pack stays while the move runs");
    pin.version = kit.version.clone();
    pin.sha256 = pack_sha256(&kit.raw);
    marker.set_pins(&pins);
    let offered = opt_in::offer(root, &mut marker, kit)?;
    marker.write()?;

    if previous.is_some() {
        println!("moved pack {} from {old} to {}", kit.name, kit.version);
    } else {
        println!("wrote {relative} ({})", kit.version);
    }
    for line in area_removals
        .into_iter()
        .chain(area_additions)
        .chain(offered)
    {
        println!("{line}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::packs::parse_pack;

    /// The id that the kit pack lists under `optIn`.
    const OPT_IN: &str = "demo.one";

    /// A kit pack `demo` at 1.1.0 that ships `demo.one` and `demo.two` and
    /// lists `opt_in` under `optIn`.
    fn kit(opt_in: &[&str]) -> PackCopy {
        let entry = |id: &str| {
            json!({
                "id": id, "kind": "rule", "area": "global", "summary": "A rule.",
                "body": ["Body."], "tags": [],
                "source": {"date": "2026-10-20", "by": "docs"},
            })
        };
        parse_pack(
            json!({
                "name": "demo", "version": "1.1.0", "schema": "houserules/pack@1.2.0",
                "topics": {"demo": {"title": "Demo"}}, "areas": {"global": {"paths": []}},
                "entries": [entry("demo.one"), entry("demo.two")], "optIn": opt_in,
            }),
            "demo.json",
        )
        .expect("a valid pack")
    }

    /// A scratch install whose marker pins the kit pack `demo` at 1.0.0, with
    /// the area that the pack declares already in `areas.json`, and no copy
    /// of the pack.
    fn install_with_the_pin_at_1_0_0() -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        write_json(
            &root.join(".houserules.json"),
            &json!({
                "schema": "houserules/install@2.2.0",
                "packs": {"demo": {
                    "version": "1.0.0", "source": "kit", "sha256": "0", "addedAreas": []
                }},
            }),
        )
        .expect("write the marker");
        write_json(
            &root.join("knowledge/areas.json"),
            &json!({"global": {"paths": []}}),
        )
        .expect("write areas.json");
        dir
    }

    /// The marker of `root`, as JSON.
    fn marker_json(root: &Path) -> Value {
        serde_json::from_str(
            &fs::read_to_string(root.join(".houserules.json")).expect("read the marker"),
        )
        .expect("parse the marker")
    }

    /// Moves the pin of `kit` from 1.0.0 in `root`, the way `update` does for
    /// a kit pin whose copy is missing, and answers what the move answers.
    fn try_to_move_the_pin(root: &Path, kit: &PackCopy) -> Result<(), String> {
        let marker = Marker::read(root).expect("read the marker");
        move_pack(root, marker, kit, "1.0.0", None, &[])
    }

    /// Moves the pin of `kit` from 1.0.0 in `root` and requires the move to
    /// finish.
    fn move_the_pin(root: &Path, kit: &PackCopy) {
        try_to_move_the_pin(root, kit).expect("the move");
    }

    /// The move writes the exclusion of an opt-in entry before the copy that
    /// ships the entry, so a stop at the exclusion leaves no live entry: a
    /// directory in place of `overrides.json` makes the exclusion fail with
    /// the operating system's own error, and the move writes no copy and
    /// leaves the pin at the old version.
    #[test]
    fn a_move_writes_the_exclusion_before_it_writes_the_copy() {
        let install = install_with_the_pin_at_1_0_0();
        fs::create_dir(install.path().join("knowledge/overrides.json"))
            .expect("create a directory in place of overrides.json");
        let error = try_to_move_the_pin(install.path(), &kit(&[OPT_IN]))
            .expect_err("the exclusion cannot be written");
        assert!(error.contains("overrides.json"), "{error}");
        assert!(!install.path().join("knowledge/packs/demo.json").exists());
        assert_eq!(
            marker_json(install.path())["packs"]["demo"]["version"],
            json!("1.0.0")
        );
    }

    /// A kit pack whose new version no longer ships an opt-in entry takes its
    /// untaken offer with it: the exclusion goes, and so does the id in the
    /// record. The exclusion of the project's own stays.
    #[test]
    fn a_move_withdraws_an_untaken_offer_that_the_new_version_drops() {
        let install = install_with_the_pin_at_1_0_0();
        let gone = json!({"id": "demo.gone", "reason": "opt-in"});
        let own = json!({"id": "other.rule", "reason": "Not for us."});
        write_json(
            &install.path().join("knowledge/overrides.json"),
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [],
                "exclusions": [gone, own],
            }),
        )
        .expect("write overrides.json");
        let mut marker = marker_json(install.path());
        marker["optInOffered"] = json!(["demo.gone"]);
        write_json(&install.path().join(".houserules.json"), &marker).expect("write the marker");

        move_the_pin(install.path(), &kit(&[]));

        let overrides: Value = serde_json::from_str(
            &fs::read_to_string(install.path().join("knowledge/overrides.json"))
                .expect("read overrides.json"),
        )
        .expect("parse overrides.json");
        assert_eq!(overrides["exclusions"], json!([own]));
        let marker = marker_json(install.path());
        assert_eq!(marker["optInOffered"], json!([]));
        assert_eq!(marker["packs"]["demo"]["version"], json!("1.1.0"));
    }

    /// The move offers the opt-in entries that the new version lists, and the
    /// one write of the marker holds both the record of the offer and the new
    /// version of the pin, so the pin never moves without the record.
    #[test]
    fn a_move_offers_the_opt_in_entries_in_the_write_that_moves_the_pin() {
        let install = install_with_the_pin_at_1_0_0();
        let kit = kit(&[OPT_IN]);
        move_the_pin(install.path(), &kit);

        let marker = marker_json(install.path());
        assert_eq!(marker["packs"]["demo"]["version"], json!("1.1.0"));
        assert_eq!(
            marker["packs"]["demo"]["sha256"],
            json!(pack_sha256(&kit.raw))
        );
        assert_eq!(marker["optInOffered"], json!([OPT_IN]));
        let overrides: Value = serde_json::from_str(
            &fs::read_to_string(install.path().join("knowledge/overrides.json"))
                .expect("read overrides.json"),
        )
        .expect("parse overrides.json");
        assert_eq!(
            overrides["exclusions"],
            json!([{"id": OPT_IN, "reason": "opt-in"}])
        );
    }

    /// A project that deleted the exclusion has opted in for good: a later
    /// move writes no exclusion and leaves the record as it was.
    #[test]
    fn a_move_never_offers_an_entry_the_marker_records() {
        let install = install_with_the_pin_at_1_0_0();
        let mut marker = marker_json(install.path());
        marker["optInOffered"] = json!([OPT_IN]);
        write_json(&install.path().join(".houserules.json"), &marker).expect("write the marker");

        move_the_pin(install.path(), &kit(&[OPT_IN]));
        assert!(!install.path().join("knowledge/overrides.json").exists());
        let marker = marker_json(install.path());
        assert_eq!(marker["optInOffered"], json!([OPT_IN]));
        assert_eq!(marker["packs"]["demo"]["version"], json!("1.1.0"));
    }

    /// Pin of unchanged behavior: a kit pack with no `optIn` list moves with
    /// no exclusion, no `overrides.json`, and no record in the marker.
    #[test]
    fn a_move_of_a_pack_with_no_opt_in_list_writes_no_exclusion_and_no_record() {
        let install = install_with_the_pin_at_1_0_0();
        move_the_pin(install.path(), &kit(&[]));
        assert!(!install.path().join("knowledge/overrides.json").exists());
        assert_eq!(marker_json(install.path()).get("optInOffered"), None);
    }
}
