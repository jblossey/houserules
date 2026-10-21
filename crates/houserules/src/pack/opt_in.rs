//! The opt-in of a pack entry (batch 34, spec 3.7).
//!
//! A pack lists the ids of its opt-in entries under `optIn`. An opt-in entry
//! is inactive on every install until the project turns it on. Every path that
//! brings such an id onto an install ([`offer`]) writes an exclusion with the
//! reason `opt-in` to `knowledge/overrides.json` once, and records the id in
//! `.houserules.json` `optInOffered`. The project opts in by deleting the
//! exclusion. A later [`offer`] never writes the exclusion again for an id in
//! `optInOffered`, so a project that deleted it has opted in for good.
//!
//! [`withdraw`] runs on `pack remove`: it drops each offer that the project
//! did not take, with its exclusion, so a later `pack add` offers the entry
//! again. An id that the project opted in to stays in `optInOffered`, and
//! [`active_lines`] says so on the next `pack add`. [`withdraw_orphans`] runs
//! on `pack upgrade` and on the pin move of `update`: it drops each untaken
//! offer whose entry no installed pack ships any more.
//!
//! # What a stop leaves
//!
//! A command that writes the copy of a pack writes the exclusion first
//! ([`offer`] or [`exclude`]), so no stop of it leaves an opt-in entry live
//! without the project's deletion of the exclusion
//! (`houserules.migrations-finish-after-a-stop`). [`offer`] records the id in
//! the marker that its caller writes, so a run that stopped after the
//! exclusion finds it in place, records the id, and prints the line. After a
//! stop at any write the next run finishes the work of `pack add`,
//! `pack upgrade`, the pin move of `update`, the 1.x conversion of `update`,
//! and `pack remove`:
//!
//! - `pack add` writes the exclusion, then the copy, then the one marker that
//!   holds the pin and the record. A stop at the exclusion leaves no pack; a
//!   stop at the copy leaves the exclusion and no pack, and the next `pack
//!   add` finishes; a stop at the marker leaves the copy and the exclusion
//!   with no pin, the entry excluded, and `pack remove` then `pack add`
//!   finishes. Tests: `a_pack_add_that_stops_at_the_exclusion_leaves_no_entry_live`,
//!   `a_pack_add_that_stops_at_the_copy_leaves_no_entry_live`, and
//!   `a_pack_add_that_stops_at_the_marker_leaves_no_entry_live` in
//!   `tests/opt_in.rs`.
//! - `pack upgrade` writes the exclusion of each new `optIn` id, the copy, the
//!   marker with the new pin and the record, the one-id-one-source step, the
//!   removal of dropped areas, and last the withdrawal of the orphaned offers.
//!   A stop before the marker leaves the old pin, and each new entry is
//!   excluded or not shipped yet, never live; the same `pack upgrade`
//!   finishes. The withdrawal tests the file and the
//!   installed packs, so a stop between its two writes is finished by the
//!   next `pack upgrade`. Tests: `a_pack_upgrade_that_stops_at_the_exclusion_leaves_no_entry_live`,
//!   `a_pack_upgrade_that_stops_at_the_copy_leaves_no_entry_live`,
//!   `a_pack_upgrade_that_stops_at_the_marker_leaves_no_entry_live`, and
//!   `a_pack_upgrade_that_stops_at_the_withdrawal_of_an_offer_finishes_at_the_next_upgrade`
//!   in `tests/opt_in.rs`.
//! - The pin move of `update` writes the exclusion, the copy, the withdrawal
//!   of the orphaned offers, and last the marker that holds the record and
//!   moves the pin. A stop leaves the pin at the old version, and the next
//!   `update` moves it again. Test: `a_move_writes_the_exclusion_before_it_writes_the_copy`
//!   in `install/pack_moves.rs`.
//! - The 1.x conversion of `update` writes the exclusion of each opt-in entry
//!   of a kit pack it installs, then one marker that holds the pin and the
//!   record, then the areas, and last the copy. A stop before the marker leaves
//!   the exclusion and no pin, and the next `update` records the id and prints
//!   the line; a stop after it leaves the pin and the record with no copy, and
//!   the next `update` writes the copy and prints no second line. Tests:
//!   `a_conversion_that_stops_at_the_exclusion_finishes_with_one_offer_line`,
//!   `a_conversion_that_stops_at_the_marker_after_the_exclusion_finishes_with_one_offer_line`,
//!   and `a_conversion_that_stops_at_the_copy_finishes_without_a_second_offer_line`
//!   in `tests/conversion_34.rs`.
//! - `pack remove` is the one command that can leave an entry active:
//!   [`withdraw`] writes the marker, then removes the exclusion, and the copy
//!   goes after that, so a stop between the exclusion and the copy leaves the
//!   pack installed with its entry active. The project asked for the removal,
//!   and the next `pack remove` finishes it; a stop between the marker and the
//!   exclusion leaves an exclusion with the reason `opt-in` that the next run
//!   removes. Test: `withdraw_finishes_a_run_that_stopped_after_the_marker`
//!   in this file.

use std::path::Path;

use serde_json::{Value, json};

use crate::archive::{read_json, write_json};
use crate::install::marker::Marker;
use crate::packs::PackCopy;

use super::one_source::{OverridesEdit, shipped_entries};

/// The reason of an exclusion that [`offer`] writes.
const REASON: &str = "opt-in";

/// The path of `knowledge/overrides.json`, built one component at a time.
fn overrides_path(root: &Path) -> std::path::PathBuf {
    root.join("knowledge").join("overrides.json")
}

/// The parsed `knowledge/overrides.json`, or `null` when the file does not
/// exist. Any other failure is the operating system's own line.
fn read_overrides(root: &Path) -> Result<Value, String> {
    let path = overrides_path(root);
    if !path.exists() {
        return Ok(Value::Null);
    }
    read_json(&path)
}

/// The `exclusions` array of `overrides`, or none when it has none.
fn exclusions(overrides: &Value) -> &[Value] {
    overrides
        .get("exclusions")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// `true` when `overrides` excludes `id` for any reason.
fn excludes(overrides: &Value, id: &str) -> bool {
    exclusions(overrides)
        .iter()
        .any(|exclusion| exclusion.get("id").and_then(Value::as_str) == Some(id))
}

/// `true` when `overrides` holds the exclusion that [`offer`] writes for
/// `id`: the id, with the reason `opt-in`, and no override of the id beside
/// it. An exclusion with another reason, or an override of the entry, is the
/// project's own.
fn holds_an_offer(overrides: &Value, id: &str) -> bool {
    let is_offer = |exclusion: &Value| {
        exclusion.get("id").and_then(Value::as_str) == Some(id)
            && exclusion.get("reason").and_then(Value::as_str) == Some(REASON)
    };
    let overridden = overrides
        .get("overrides")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| item.get("id").and_then(Value::as_str) == Some(id))
        });
    !overridden && exclusions(overrides).iter().any(is_offer)
}

/// The `optIn` ids of `pack` whose exclusion in `overrides` is still the one
/// that [`offer`] wrote: the offers that the project did not take.
pub(super) fn untaken_offers(overrides: &Value, pack: &PackCopy) -> Vec<String> {
    pack.opt_in()
        .into_iter()
        .filter(|id| holds_an_offer(overrides, id))
        .collect()
}

/// The line that says an offered entry is excluded until the project deletes
/// its exclusion.
fn offer_line(id: &str) -> String {
    format!(
        "{id} is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion"
    )
}

/// The line of spec 3.7 for an `optIn` id that the install opted in to
/// before.
fn active_line(id: &str) -> String {
    format!("{id} is active: this install opted in before")
}

/// For each `optIn` id of `pack` that `marker` records as offered before, the
/// line that says the entry is active: the project opted in to it, so the
/// command that brings the pack back writes no exclusion. `pack add` prints
/// these lines; no other path does.
pub(super) fn active_lines(marker: &Marker, pack: &PackCopy) -> Vec<String> {
    let offered = marker.opt_in_offered();
    pack.opt_in()
        .iter()
        .filter(|id| offered.contains(id))
        .map(|id| active_line(id))
        .collect()
}

/// `overrides` with an exclusion of each of `ids`, each with the reason
/// `opt-in`. `ids` is not empty.
fn with_exclusions(overrides: &Value, ids: &[&String]) -> Value {
    let mut edit = OverridesEdit::new(overrides);
    for id in ids {
        edit.add_exclusion(id);
    }
    let mut document = edit.into_changed().expect("an exclusion was added");
    let items = document["exclusions"]
        .as_array_mut()
        .expect("the edit left an exclusions array");
    let first_added = items.len() - ids.len();
    for item in &mut items[first_added..] {
        item["reason"] = json!(REASON);
    }
    document
}

/// The `optIn` ids of `pack` that `marker` does not record yet, in the order
/// of `optIn`: the ids that an offer makes now.
fn unrecorded(marker: &Marker, pack: &PackCopy) -> Vec<String> {
    let recorded = marker.opt_in_offered();
    pack.opt_in()
        .into_iter()
        .filter(|id| !recorded.contains(id))
        .collect()
}

/// `overrides` with an exclusion of each of `ids` that it does not exclude
/// yet, or `None` when it excludes every one.
fn with_missing_exclusions(overrides: &Value, ids: &[String]) -> Option<Value> {
    let missing: Vec<&String> = ids.iter().filter(|id| !excludes(overrides, id)).collect();
    (!missing.is_empty()).then(|| with_exclusions(overrides, &missing))
}

/// The plan of [`exclude`]: `overrides` as [`exclude`] leaves it for `pack`,
/// built in memory and written nowhere, or `None` when it needs no exclusion.
/// A command that checks every refusal before it writes plans its other
/// writes to `knowledge/overrides.json` on this document, so that none of them
/// writes an exclusion away.
pub(super) fn with_offered_exclusions(
    overrides: &Value,
    marker: &Marker,
    pack: &PackCopy,
) -> Option<Value> {
    with_missing_exclusions(overrides, &unrecorded(marker, pack))
}

/// Writes the exclusion with the reason `opt-in` of each of `ids` to
/// `knowledge/overrides.json`, for each id the file does not exclude yet.
fn write_exclusions(root: &Path, ids: &[String]) -> Result<(), String> {
    let overrides = read_overrides(root)?;
    match with_missing_exclusions(&overrides, ids) {
        Some(document) => write_json(&overrides_path(root), &document),
        None => Ok(()),
    }
}

/// Writes the exclusion of each `optIn` id of `pack` that `marker` does not
/// record yet, and nothing else: the id stays unrecorded, so [`offer`] still
/// prints its line and records it. A command that writes the copy of `pack`
/// calls this before the write that makes the entries live, so that no stop
/// leaves an opt-in entry active.
pub(crate) fn exclude(root: &Path, marker: &Marker, pack: &PackCopy) -> Result<(), String> {
    write_exclusions(root, &unrecorded(marker, pack))
}

/// Offers each `optIn` id of `pack` that `marker` does not record yet: writes
/// its exclusion with the reason `opt-in` to `knowledge/overrides.json` when
/// the file holds none for the id, then records the id in `marker`, which the
/// caller writes. An id that `marker` records is left alone, whatever
/// `overrides.json` holds, so a project that deleted the exclusion keeps the
/// entry. Returns one line for each id it offered, in the order of `optIn`.
/// A caller that writes the copy of `pack` calls this, or [`exclude`], before
/// that write.
pub(crate) fn offer(
    root: &Path,
    marker: &mut Marker,
    pack: &PackCopy,
) -> Result<Vec<String>, String> {
    let new = unrecorded(marker, pack);
    if new.is_empty() {
        return Ok(Vec::new());
    }
    write_exclusions(root, &new)?;
    let lines = new.iter().map(|id| offer_line(id)).collect();
    let mut offered = marker.opt_in_offered();
    offered.extend(new);
    marker.set_opt_in_offered(&offered);
    Ok(lines)
}

/// The ids of the orphaned offers in `overrides`: exclusions that [`offer`]
/// wrote, with no override beside them, for an id that no pack of `packs`
/// ships. Each id comes once, in the order of the file.
pub(super) fn orphaned_offers(overrides: &Value, packs: &[PackCopy]) -> Vec<String> {
    let shipped = shipped_entries(packs);
    let mut orphaned: Vec<String> = Vec::new();
    for exclusion in exclusions(overrides) {
        let Some(id) = exclusion.get("id").and_then(Value::as_str) else {
            continue;
        };
        if !shipped.contains_key(id)
            && holds_an_offer(overrides, id)
            && !orphaned.iter().any(|seen| seen == id)
        {
            orphaned.push(id.to_string());
        }
    }
    orphaned
}

/// Drops each of `ids` from `optInOffered` and writes `marker`, then removes
/// the exclusion of each from `overrides`, the parsed `knowledge/overrides.json`.
/// `ids` is not empty. The marker is written first, so that a stop between the
/// two writes leaves exclusions that the next run finds again.
fn withdraw_ids(
    root: &Path,
    marker: &mut Marker,
    overrides: Value,
    ids: &[String],
) -> Result<(), String> {
    if ids.is_empty() {
        return Ok(());
    }
    let kept: Vec<String> = marker
        .opt_in_offered()
        .into_iter()
        .filter(|id| !ids.contains(id))
        .collect();
    marker.set_opt_in_offered(&kept);
    marker.write()?;
    let mut document = overrides;
    if let Some(items) = document.get_mut("exclusions").and_then(Value::as_array_mut) {
        items.retain(|item| {
            let id = item.get("id").and_then(Value::as_str).unwrap_or_default();
            !ids.iter().any(|withdrawn| withdrawn == id)
        });
    }
    write_json(&overrides_path(root), &document)
}

/// Withdraws the offers of `pack` that the project did not take, for
/// `pack remove`: for each `optIn` id whose exclusion is still the one that
/// [`offer`] wrote, drops the id from `optInOffered` and writes the marker,
/// then removes the exclusion. An id whose exclusion the project deleted stays
/// in `optInOffered`: the project opted in. `marker` is the marker on disk and
/// is written here, before the exclusions go, so that a stop between the two
/// writes leaves an exclusion that the next run removes.
pub(crate) fn withdraw(root: &Path, marker: &mut Marker, pack: &PackCopy) -> Result<(), String> {
    let overrides = read_overrides(root)?;
    let withdrawn = untaken_offers(&overrides, pack);
    withdraw_ids(root, marker, overrides, &withdrawn)
}

/// Withdraws the orphaned offers ([`orphaned_offers`]) that `packs`, the packs
/// installed once a move is done, leave behind: the new version of a pack no
/// longer ships the entry. The offer is houserules' own data, so `pack upgrade`
/// and the pin move of `update` remove it, as `pack remove` removes the offers
/// of the pack it removes. The record goes first and the exclusion second, as
/// in [`withdraw`]. The orphan test reads the file and `packs`, never the pack
/// that was installed before, so a run that stopped between the two writes is
/// finished by the next run of the command. `marker` is the marker on disk.
pub(crate) fn withdraw_orphans(
    root: &Path,
    marker: &mut Marker,
    packs: &[PackCopy],
) -> Result<(), String> {
    let overrides = read_overrides(root)?;
    let withdrawn = orphaned_offers(&overrides, packs);
    withdraw_ids(root, marker, overrides, &withdrawn)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::packs::parse_pack;

    /// The id that the demo pack lists under `optIn` in most tests.
    const ONE: &str = "demo.one";

    /// A pack `demo` that ships one entry for each id of `ids` and lists
    /// `opt_in` under `optIn`.
    fn pack(ids: &[&str], opt_in: &[&str]) -> PackCopy {
        let entries: Vec<Value> = ids
            .iter()
            .map(|id| {
                json!({
                    "id": id, "kind": "rule", "area": "global", "summary": "A rule.",
                    "body": ["Body."], "tags": [],
                    "source": {"date": "2026-10-20", "by": "docs"},
                })
            })
            .collect();
        parse_pack(
            json!({
                "name": "demo", "version": "1.0.0", "schema": "houserules/pack@1.1.0",
                "topics": {"demo": {"title": "Demo"}}, "areas": {"global": {"paths": []}},
                "entries": entries, "optIn": opt_in,
            }),
            "demo.json",
        )
        .expect("a valid pack")
    }

    /// A scratch install root with a marker that has no `optInOffered` key.
    fn root() -> TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("knowledge")).expect("create knowledge/");
        write(
            dir.path(),
            ".houserules.json",
            &json!({"schema": "houserules/install@2.1.0"}),
        );
        dir
    }

    fn write(root: &Path, relative: &str, value: &Value) {
        write_json(&root.join(relative), value).expect("write a JSON file");
    }

    fn marker(root: &Path) -> Marker {
        Marker::read(root).expect("read the marker")
    }

    /// The `exclusions` that `knowledge/overrides.json` holds, or none.
    fn exclusions_of(root: &Path) -> Vec<Value> {
        exclusions(&read_overrides(root).expect("read overrides.json")).to_vec()
    }

    fn offer_exclusion(id: &str) -> Value {
        json!({"id": id, "reason": "opt-in"})
    }

    /// The offer writes the exclusion with the reason `opt-in` to a new
    /// `overrides.json`, records the id in the marker it is given, and prints
    /// one line. It writes no marker: the caller does.
    #[test]
    fn offer_writes_the_exclusion_and_records_the_id_in_the_marker_it_is_given() {
        let root = root();
        let mut held = marker(root.path());
        let lines =
            offer(root.path(), &mut held, &pack(&[ONE, "demo.two"], &[ONE])).expect("the offer");
        assert_eq!(lines, [offer_line(ONE)]);
        assert_eq!(
            read_overrides(root.path()).expect("overrides.json"),
            json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [],
                "exclusions": [offer_exclusion(ONE)],
            })
        );
        assert_eq!(held.opt_in_offered(), [ONE]);
        assert_eq!(marker(root.path()).opt_in_offered(), Vec::<String>::new());
    }

    /// The exclusion joins the items of an `overrides.json` that exists: an
    /// override and the project's own exclusion stay, in their order.
    #[test]
    fn offer_keeps_every_item_of_an_existing_overrides_file() {
        let root = root();
        let own = json!({"id": "other.rule", "reason": "Not for us."});
        let document = json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": "other.two", "fields": {"summary": "S."}}],
            "exclusions": [own],
        });
        write(root.path(), "knowledge/overrides.json", &document);
        offer(root.path(), &mut marker(root.path()), &pack(&[ONE], &[ONE])).expect("the offer");
        let after = read_overrides(root.path()).expect("overrides.json");
        assert_eq!(after["overrides"], document["overrides"]);
        assert_eq!(after["exclusions"], json!([own, offer_exclusion(ONE)]));
    }

    /// An id that the marker records is the project's: it is never offered
    /// again, and no `overrides.json` appears.
    #[test]
    fn offer_skips_an_id_the_marker_records() {
        let root = root();
        let mut held = marker(root.path());
        held.set_opt_in_offered(&[ONE.to_string()]);
        let lines = offer(root.path(), &mut held, &pack(&[ONE], &[ONE])).expect("the offer");
        assert_eq!(lines, Vec::<String>::new());
        assert!(!overrides_path(root.path()).exists());
        assert_eq!(held.opt_in_offered(), [ONE]);
    }

    /// A run that stopped after it wrote the exclusion and before the caller
    /// wrote the marker leaves a tree that the next run finishes: the next
    /// offer finds the exclusion, writes no second one, and records the id.
    #[test]
    fn offer_finishes_a_run_that_stopped_after_the_exclusion() {
        let root = root();
        offer(root.path(), &mut marker(root.path()), &pack(&[ONE], &[ONE])).expect("a first run");
        let written = fs::read(overrides_path(root.path())).expect("read overrides.json");

        let mut next = marker(root.path());
        offer(root.path(), &mut next, &pack(&[ONE], &[ONE])).expect("the next run");
        assert_eq!(
            fs::read(overrides_path(root.path())).expect("read overrides.json"),
            written
        );
        assert_eq!(exclusions_of(root.path()), vec![offer_exclusion(ONE)]);
        assert_eq!(next.opt_in_offered(), [ONE]);
    }

    /// Each id that the marker does not record is offered, in the order of
    /// the pack; the ones it records keep their place in the record.
    #[test]
    fn offer_offers_each_new_id_once_in_the_order_of_the_pack() {
        let root = root();
        let mut held = marker(root.path());
        held.set_opt_in_offered(&["demo.b".to_string()]);
        let lines = offer(
            root.path(),
            &mut held,
            &pack(
                &["demo.a", "demo.b", "demo.c"],
                &["demo.c", "demo.b", "demo.a"],
            ),
        )
        .expect("the offer");
        assert_eq!(lines, [offer_line("demo.c"), offer_line("demo.a")]);
        assert_eq!(
            exclusions_of(root.path()),
            vec![offer_exclusion("demo.c"), offer_exclusion("demo.a")]
        );
        assert_eq!(held.opt_in_offered(), ["demo.b", "demo.c", "demo.a"]);
    }

    /// The withdrawal of an offer that the project did not take drops the
    /// record and the exclusion, and writes the marker itself.
    #[test]
    fn withdraw_drops_the_record_and_the_exclusion_of_an_untaken_offer() {
        let root = root();
        let demo = pack(&[ONE], &[ONE]);
        let mut held = marker(root.path());
        offer(root.path(), &mut held, &demo).expect("the offer");
        held.write().expect("write the marker");

        withdraw(root.path(), &mut marker(root.path()), &demo).expect("the withdrawal");
        assert_eq!(exclusions_of(root.path()), Vec::<Value>::new());
        assert_eq!(marker(root.path()).opt_in_offered(), Vec::<String>::new());
    }

    /// An offer that the project took, by deleting the exclusion, stays in the
    /// record.
    #[test]
    fn withdraw_keeps_the_record_of_an_offer_the_project_took() {
        let root = root();
        let demo = pack(&[ONE], &[ONE]);
        let mut held = marker(root.path());
        offer(root.path(), &mut held, &demo).expect("the offer");
        held.write().expect("write the marker");
        let mut document = read_overrides(root.path()).expect("overrides.json");
        document["exclusions"] = json!([]);
        write(root.path(), "knowledge/overrides.json", &document);

        withdraw(root.path(), &mut marker(root.path()), &demo).expect("the withdrawal");
        assert_eq!(marker(root.path()).opt_in_offered(), [ONE]);
    }

    /// An exclusion with another reason, or an exclusion with an override of
    /// the entry beside it, is the project's own: the withdrawal leaves it and
    /// the record alone.
    #[test]
    fn withdraw_leaves_an_exclusion_that_is_the_projects_own() {
        let demo = pack(&[ONE], &[ONE]);
        for document in [
            json!({
                "schema": "houserules/overrides@1.0.0",
                "exclusions": [{"id": ONE, "reason": "We decided."}],
            }),
            json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [{"id": ONE, "fields": {"summary": "S."}}],
                "exclusions": [offer_exclusion(ONE)],
            }),
            json!({"schema": "houserules/overrides@1.0.0", "exclusions": [{"id": ONE}]}),
        ] {
            let root = root();
            write(root.path(), "knowledge/overrides.json", &document);
            let mut held = marker(root.path());
            held.set_opt_in_offered(&[ONE.to_string()]);
            held.write().expect("write the marker");

            withdraw(root.path(), &mut marker(root.path()), &demo).expect("the withdrawal");
            assert_eq!(
                read_overrides(root.path()).expect("overrides.json"),
                document
            );
            assert_eq!(marker(root.path()).opt_in_offered(), [ONE]);
        }
    }

    /// A withdrawal that stopped after it wrote the marker and before it
    /// removed the exclusion leaves an exclusion with the reason `opt-in` and
    /// no record: the next withdrawal removes it.
    #[test]
    fn withdraw_finishes_a_run_that_stopped_after_the_marker() {
        let root = root();
        write(
            root.path(),
            "knowledge/overrides.json",
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "exclusions": [offer_exclusion(ONE), {"id": "other.rule"}],
            }),
        );
        withdraw(root.path(), &mut marker(root.path()), &pack(&[ONE], &[ONE])).expect("the run");
        assert_eq!(
            exclusions_of(root.path()),
            vec![json!({"id": "other.rule"})]
        );
    }

    /// `exclude` writes the exclusion that an offer needs and nothing else: it
    /// records no id in the marker. The later [`offer`] finds the exclusion in
    /// place, writes no second one, and records the id and prints the line.
    #[test]
    fn exclude_writes_the_exclusion_and_leaves_the_record_to_the_offer() {
        let root = root();
        let demo = pack(&[ONE, "demo.two"], &[ONE]);
        let held = marker(root.path());
        exclude(root.path(), &held, &demo).expect("the exclusion");
        assert_eq!(exclusions_of(root.path()), vec![offer_exclusion(ONE)]);
        assert_eq!(held.opt_in_offered(), Vec::<String>::new());
        let written = fs::read(overrides_path(root.path())).expect("read overrides.json");

        let mut next = marker(root.path());
        let lines = offer(root.path(), &mut next, &demo).expect("the offer");
        assert_eq!(lines, [offer_line(ONE)]);
        assert_eq!(
            fs::read(overrides_path(root.path())).expect("read overrides.json"),
            written
        );
        assert_eq!(next.opt_in_offered(), [ONE]);
    }

    /// An id that the marker records is the project's: `exclude` writes no
    /// exclusion for it and creates no `overrides.json`.
    #[test]
    fn exclude_skips_an_id_the_marker_records() {
        let root = root();
        let mut held = marker(root.path());
        held.set_opt_in_offered(&[ONE.to_string()]);
        exclude(root.path(), &held, &pack(&[ONE], &[ONE])).expect("the exclusion");
        assert!(!overrides_path(root.path()).exists());
    }

    /// `with_offered_exclusions` is the plan of the exclusions: the document
    /// that `exclude` would write, built in memory from the document it is
    /// given, so that a command can plan its other writes on top of it. It
    /// writes nothing, and answers `None` when no exclusion is missing.
    #[test]
    fn with_offered_exclusions_plans_the_document_and_writes_nothing() {
        let root = root();
        let demo = pack(&[ONE], &[ONE]);
        let held = marker(root.path());
        let planned = with_offered_exclusions(&Value::Null, &held, &demo).expect("a plan");
        assert_eq!(
            planned,
            json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [],
                "exclusions": [offer_exclusion(ONE)],
            })
        );
        assert!(!overrides_path(root.path()).exists());
        assert_eq!(with_offered_exclusions(&planned, &held, &demo), None);
        let mut recording = marker(root.path());
        recording.set_opt_in_offered(&[ONE.to_string()]);
        assert_eq!(
            with_offered_exclusions(&Value::Null, &recording, &demo),
            None
        );
    }

    /// An orphaned offer is an exclusion that [`offer`] wrote, with no override
    /// beside it, for an id that no installed pack ships. An exclusion for an
    /// id that a pack ships, one with another reason, and one with an override
    /// beside it are not.
    #[test]
    fn orphaned_offers_name_the_untaken_offers_that_no_installed_pack_ships() {
        let demo = pack(&[ONE], &[ONE]);
        let overrides = json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": "demo.kept", "fields": {"summary": "S."}}],
            "exclusions": [
                offer_exclusion("demo.gone"),
                offer_exclusion(ONE),
                {"id": "demo.own", "reason": "We decided."},
                {"id": "demo.bare"},
                offer_exclusion("demo.kept"),
                offer_exclusion("demo.gone"),
            ],
        });
        assert_eq!(orphaned_offers(&overrides, &[demo]), ["demo.gone"]);
        assert_eq!(
            orphaned_offers(&Value::Null, &[]),
            Vec::<String>::new(),
            "no file, no offer"
        );
    }

    /// The withdrawal of the orphaned offers drops each id from the record and
    /// writes the marker, then removes the exclusions. Every other exclusion
    /// stays.
    #[test]
    fn withdraw_orphans_drops_the_record_and_the_exclusion() {
        let root = root();
        write(
            root.path(),
            "knowledge/overrides.json",
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "exclusions": [offer_exclusion("demo.gone"), {"id": "other.rule"}],
            }),
        );
        let mut held = marker(root.path());
        held.set_opt_in_offered(&["demo.gone".to_string(), ONE.to_string()]);
        held.write().expect("write the marker");

        withdraw_orphans(root.path(), &mut held, &[pack(&[ONE], &[ONE])]).expect("the withdrawal");
        assert_eq!(
            exclusions_of(root.path()),
            vec![json!({"id": "other.rule"})]
        );
        assert_eq!(marker(root.path()).opt_in_offered(), [ONE]);
    }

    /// A withdrawal that stopped after it wrote the marker and before it
    /// removed the exclusion leaves an orphaned offer with no record: the next
    /// withdrawal removes it.
    #[test]
    fn withdraw_orphans_finishes_a_run_that_stopped_after_the_marker() {
        let root = root();
        write(
            root.path(),
            "knowledge/overrides.json",
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "exclusions": [offer_exclusion("demo.gone")],
            }),
        );
        withdraw_orphans(root.path(), &mut marker(root.path()), &[]).expect("the run");
        assert_eq!(exclusions_of(root.path()), Vec::<Value>::new());
    }

    /// With no orphaned offer the withdrawal writes no file.
    #[test]
    fn withdraw_orphans_writes_nothing_when_no_offer_is_orphaned() {
        let root = root();
        let demo = pack(&[ONE], &[ONE]);
        offer(root.path(), &mut marker(root.path()), &demo).expect("the offer");
        let written = fs::read(overrides_path(root.path())).expect("read overrides.json");
        withdraw_orphans(root.path(), &mut marker(root.path()), &[demo]).expect("the run");
        assert_eq!(
            fs::read(overrides_path(root.path())).expect("read overrides.json"),
            written
        );
    }

    /// `pack add` says that an entry is active for each `optIn` id of the pack
    /// that the marker records, and says nothing for any other id.
    #[test]
    fn active_lines_name_each_recorded_id_of_the_pack() {
        let root = root();
        let mut held = marker(root.path());
        held.set_opt_in_offered(&["demo.b".to_string(), "elsewhere.x".to_string()]);
        let demo = pack(&["demo.a", "demo.b"], &["demo.a", "demo.b"]);
        assert_eq!(active_lines(&held, &demo), [active_line("demo.b")]);
        assert_eq!(
            active_lines(&marker(root.path()), &demo),
            Vec::<String>::new()
        );
    }
}
