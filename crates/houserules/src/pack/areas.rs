//! Areas under packs (spec section 6).
//!
//! `knowledge/areas.json` is the project's one list of areas. A pack
//! declares the areas its entries use, each with default globs.
//! [`add_pack_areas`] adds each declared area that `areas.json` lacks and
//! names each glob that matches no tracked file. [`seeded_areas_of`] names
//! the areas of an `areas.json` that `init` seeded from the kit and that came
//! with a pack, so that the pin records them. [`remove_pack_areas`]
//! takes out an area only when a pack added it, no remaining pack declares
//! it, and no active entry of any source uses it. `.houserules.json` keeps
//! the record of what each pack added: the `addedAreas` of its pin.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::archive::{read_json, write_json};
use crate::install::marker::Marker;
use crate::packs::PackCopy;
use crate::rules::{Base, compile, skipped_path_line, tracked_tree_files};

/// Reads `knowledge/areas.json` as a JSON object, with its path. A file
/// of another shape is one line that names it.
fn read_areas(root: &Path) -> Result<(PathBuf, Map<String, Value>), String> {
    let path = root.join("knowledge").join("areas.json");
    match read_json(&path)? {
        Value::Object(areas) => Ok((path, areas)),
        _ => Err(format!("{}: not a JSON object", path.display())),
    }
}

/// The globs of an area's JSON, or none when its `paths` is no array of
/// strings.
fn globs_of(area: &Value) -> Vec<&str> {
    area.get("paths")
        .and_then(Value::as_array)
        .map(|paths| paths.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default()
}

/// The areas that `pack` declares and `areas` lacks, with their globs, in the
/// order the pack declares them.
fn missing_areas<'a>(
    pack: &'a PackCopy,
    areas: &Map<String, Value>,
) -> Vec<&'a (String, Vec<String>)> {
    pack.areas
        .iter()
        .filter(|(area, _)| !areas.contains_key(area))
        .collect()
}

/// The names of the areas that [`add_pack_areas`] adds for `pack` now, in the
/// order it adds them. A caller that writes the `addedAreas` record of a pin
/// before it adds the areas asks for them first, so that a run which stops
/// between the two writes still leaves the record.
pub(crate) fn areas_to_add(root: &Path, pack: &PackCopy) -> Result<Vec<String>, String> {
    let (_, areas) = read_areas(root)?;
    Ok(missing_areas(pack, &areas)
        .into_iter()
        .map(|(area, _)| area.clone())
        .collect())
}

/// Adds each area that `pack` declares and `knowledge/areas.json` lacks,
/// with the pack's globs, and pushes its name onto `record` (the
/// `addedAreas` of the pack's pin). An area that exists keeps the
/// project's globs. Returns the lines to print: one for each area added,
/// one for each tracked path that is no UTF-8 and so leaves the scan, then
/// one for each added glob that matches no tracked file. That last line is
/// information: `check-knowledge` leaves the area out of its dead-glob check
/// while the area keeps the globs of the pack (spec C16). The tracked files
/// are the list that `check-knowledge` matches its dead globs against
/// (`rules::tracked_tree_files`), so the two never disagree on one tree.
///
/// Nothing is written when no area is missing. A pack glob that does not
/// compile, and an `areas.json` that is no JSON object, are refused before
/// any write: a glob that does not compile would break every later read of
/// `areas.json`.
pub(crate) fn add_pack_areas(
    root: &Path,
    pack: &PackCopy,
    record: &mut Vec<String>,
) -> Result<Vec<String>, String> {
    let (path, mut areas) = read_areas(root)?;
    let missing = missing_areas(pack, &areas);
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let mut compiled = Vec::new();
    for (area, globs) in &missing {
        for glob in globs {
            let matcher = compile(glob).map_err(|error| {
                format!(
                    "pack {} declares the glob \"{glob}\" for area \"{area}\", which does not compile: {error}",
                    pack.name
                )
            })?;
            compiled.push((area, glob, matcher));
        }
    }
    let (tree, _, skipped) = tracked_tree_files(root)?;
    let mut lines = Vec::new();
    for (area, globs) in &missing {
        areas.insert(area.clone(), json!({"paths": globs}));
        if !record.contains(area) {
            record.push(area.clone());
        }
        let listed: Vec<&str> = globs.iter().map(String::as_str).collect();
        lines.push(if globs.is_empty() {
            format!("added area \"{area}\" to knowledge/areas.json with no globs")
        } else {
            format!(
                "added area \"{area}\" to knowledge/areas.json with the globs {}",
                listed.join(", ")
            )
        });
    }
    lines.extend(skipped.iter().map(|path| skipped_path_line(path)));
    for (area, glob, matcher) in compiled {
        if !tree.iter().any(|tracked| matcher.is_match(tracked)) {
            lines.push(format!(
                "the glob \"{glob}\" of area \"{area}\" matches no tracked file; check-knowledge accepts it while the area keeps the globs of the pack"
            ));
        }
    }
    write_json(&path, &Value::Object(areas))?;
    Ok(lines)
}

/// The areas of `pack` that `knowledge/areas.json` holds with the globs of
/// the pack, in the order the pack declares them, except the ones in
/// `claimed`. `init` asks it for an `areas.json` that it seeded from the kit
/// in the same run: such an area came with the first pack that declares it,
/// so that pack's pin records it as added, and `claimed` holds the areas of
/// the packs before it. An area whose globs differ from the pack's is not in
/// the answer.
pub(crate) fn seeded_areas_of(
    root: &Path,
    pack: &PackCopy,
    claimed: &BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let (_, areas) = read_areas(root)?;
    Ok(pack
        .areas
        .iter()
        .filter(|(area, globs)| {
            !claimed.contains(area)
                && areas
                    .get(area)
                    .is_some_and(|definition| globs_of(definition) == *globs)
        })
        .map(|(area, _)| area.clone())
        .collect())
}

/// Takes out of `knowledge/areas.json` the areas that the pin of `removed`
/// records as added, where spec section 6 allows it, and keeps the records
/// of the pins right.
///
/// `remaining` holds every pack that stays installed. `removed` leaves
/// when `remaining` holds no pack of its name (`pack remove`); when
/// `remaining` holds a newer copy of the same pack (`pack upgrade`), that
/// copy and its entries stay. For each recorded area:
///
/// - the pack's own newer copy still declares it: the record stays;
/// - another remaining pack declares it: the record moves to the pin of
///   the first such pack in name order, so the area stays pack-added;
/// - no remaining pack declares it, and an active entry of `base` uses it
///   (the entries of a leaving `removed` do not count): the area stays and
///   the record ends, so the project owns the area from then on;
/// - otherwise the area is removed and one line prints it with its globs.
///
/// The pin of `removed` keeps only the areas whose record stays. An area
/// that is not in the pin's `addedAreas` is never touched, so an area the
/// project defined itself is safe.
pub(crate) fn remove_pack_areas(
    root: &Path,
    removed: &PackCopy,
    remaining: &[PackCopy],
    base: &Base,
) -> Result<Vec<String>, String> {
    let mut marker = Marker::read(root)?;
    let mut pins = marker.pins()?;
    let Some(recorded) = pins.get(&removed.name).map(|pin| pin.added_areas.clone()) else {
        return Ok(Vec::new());
    };
    if recorded.is_empty() {
        return Ok(Vec::new());
    }
    let (path, mut areas) = read_areas(root)?;
    let leaving = !remaining.iter().any(|pack| pack.name == removed.name);
    let mut by_name: Vec<&PackCopy> = remaining.iter().collect();
    by_name.sort_by(|left, right| left.name.cmp(&right.name));
    let declares = |pack: &PackCopy, area: &str| pack.areas.iter().any(|(name, _)| name == area);
    let in_use = |area: &str| {
        base.entries
            .values()
            .any(|entry| entry.area == area && !(leaving && entry.source.name() == removed.name))
    };
    let before = pins.clone();
    let mut kept = Vec::new();
    let mut lines = Vec::new();
    let mut removed_any = false;
    for area in recorded {
        if by_name
            .iter()
            .any(|pack| pack.name == removed.name && declares(pack, &area))
        {
            kept.push(area);
            continue;
        }
        if let Some(owner) = by_name.iter().find(|pack| declares(pack, &area)) {
            if let Some(pin) = pins.get_mut(&owner.name)
                && !pin.added_areas.contains(&area)
            {
                pin.added_areas.push(area);
            }
            continue;
        }
        if in_use(&area) {
            continue;
        }
        if let Some(definition) = areas.shift_remove(&area) {
            let globs = globs_of(&definition);
            lines.push(if globs.is_empty() {
                format!("removed area \"{area}\" from knowledge/areas.json; it had no globs")
            } else {
                format!(
                    "removed area \"{area}\" from knowledge/areas.json; its globs were {}",
                    globs.join(", ")
                )
            });
            removed_any = true;
        }
    }
    if let Some(pin) = pins.get_mut(&removed.name) {
        pin.added_areas = kept;
    }
    if removed_any {
        write_json(&path, &Value::Object(areas))?;
    }
    if pins != before {
        marker.set_pins(&pins);
        marker.write()?;
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    /// A pack that declares each of `areas`, with nothing else in it.
    fn pack_declaring(areas: &[(&str, &[&str])]) -> PackCopy {
        PackCopy {
            name: "alpha".to_string(),
            version: "1.0.0".to_string(),
            schema: "houserules/pack@1.0.0".to_string(),
            topics: Vec::new(),
            areas: areas
                .iter()
                .map(|(area, globs)| {
                    (
                        area.to_string(),
                        globs.iter().map(|glob| glob.to_string()).collect(),
                    )
                })
                .collect(),
            entries: Vec::new(),
            raw: json!({}),
        }
    }

    /// A directory whose `knowledge/areas.json` holds `areas`.
    fn root_with_areas(areas: &Value) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("knowledge")).expect("create knowledge/");
        fs::write(
            dir.path().join("knowledge/areas.json"),
            serde_json::to_string(areas).expect("serialize"),
        )
        .expect("write areas.json");
        dir
    }

    /// An area that the file holds with the globs of the pack comes with the
    /// pack, in the order the pack declares them; an area the file lacks
    /// does not.
    #[test]
    fn a_seeded_area_with_the_globs_of_the_pack_comes_with_it() {
        let dir = root_with_areas(&json!({
            "tools": {"paths": ["tools/**"]},
            "docs": {"paths": ["docs/**"]},
        }));
        let pack = pack_declaring(&[
            ("docs", &["docs/**"]),
            ("missing", &["m/**"]),
            ("tools", &["tools/**"]),
        ]);

        assert_eq!(
            seeded_areas_of(dir.path(), &pack, &BTreeSet::new()),
            Ok(vec!["docs".to_string(), "tools".to_string()])
        );
    }

    /// An area that an earlier pack claimed stays with that pack.
    #[test]
    fn a_claimed_area_stays_with_the_pack_that_claimed_it() {
        let dir = root_with_areas(&json!({"docs": {"paths": ["docs/**"]}}));
        let pack = pack_declaring(&[("docs", &["docs/**"])]);

        assert_eq!(
            seeded_areas_of(dir.path(), &pack, &BTreeSet::from(["docs".to_string()])),
            Ok(Vec::new())
        );
    }

    /// An area whose globs differ from the pack's, in content, count, or
    /// order, is the project's own.
    #[test]
    fn a_seeded_area_with_other_globs_is_the_projects_own() {
        let dir = root_with_areas(&json!({
            "docs": {"paths": ["documents/**"]},
            "tools": {"paths": ["tools/**", "bin/**"]},
            "src": {"paths": ["b/**", "a/**"]},
        }));
        let pack = pack_declaring(&[
            ("docs", &["docs/**"]),
            ("tools", &["tools/**"]),
            ("src", &["a/**", "b/**"]),
        ]);

        assert_eq!(
            seeded_areas_of(dir.path(), &pack, &BTreeSet::new()),
            Ok(Vec::new())
        );
    }
}
