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
        move_pack(root, marker, &kit, &pin.version, copy)?;
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
/// gains the ones the new version adds.
///
/// The writes come in this order, and the pin moves to the new version last
/// (spec C20):
///
/// 1. the pin, still at the old version, with the areas that the move is about
///    to add in its `addedAreas`;
/// 2. the areas that the new version adds;
/// 3. the new copy;
/// 4. the removal of the areas that the new version drops, and of their record;
/// 5. the version and the `sha256` of the pin.
///
/// A run that stops after any write leaves a pin at the old version, which the
/// next run moves again: the record names every area that an earlier run
/// added, and the removal tests the pin's record against the new copy, so each
/// write is safe to repeat.
fn move_pack(
    root: &Path,
    mut marker: Marker,
    kit: &PackCopy,
    old: &str,
    previous: Option<&PackCopy>,
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
    let mut pins = marker.pins()?;
    let pin = pins
        .get_mut(&kit.name)
        .expect("the pin of the pack stays while the move runs");
    pin.version = kit.version.clone();
    pin.sha256 = pack_sha256(&kit.raw);
    marker.set_pins(&pins);
    marker.write()?;

    if previous.is_some() {
        println!("moved pack {} from {old} to {}", kit.name, kit.version);
    } else {
        println!("wrote {relative} ({})", kit.version);
    }
    for line in area_removals.into_iter().chain(area_additions) {
        println!("{line}");
    }
    Ok(())
}
