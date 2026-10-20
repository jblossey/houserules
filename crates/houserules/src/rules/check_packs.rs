//! The pack-layer checks of `check-knowledge`: what a project's pack copies,
//! `overrides.json`, and `areas.json` must hold on top of what the schema
//! says.
//!
//! The checks, each as findings that name the file, and the id when the
//! finding has one (a pin finding names a pack, and the `CLAUDE.md` finding
//! names no id):
//!
//! - the pins: each copy under `knowledge/packs/` matches the `sha256` that
//!   `.houserules.json` `packs.<name>` records for it, and every copy has a
//!   pin and every pin a copy (`pin_findings`). The pins come from
//!   `install::marker::read_pins_for_gate`, which the kit-owned citation lint
//!   reads too (`kit_citations`);
//! - the validity of `overrides.json` (spec L5): an override or an
//!   exclusion of an id that no installed pack ships, of a local id, an
//!   override and an exclusion of one id, and two overrides of one id
//!   (`override_findings`);
//! - one id, one source: an id that two sources ship, and an id that is
//!   active in one source and archived in another (`duplicate_findings`,
//!   `archived_and_active_findings`);
//! - the area of every active entry is a key of `areas.json`, and on an
//!   install that keeps the 1.x `$defs.area` enum, a pack area that the
//!   enum lacks names the enum as the cause (`area_findings`);
//! - the dead-glob check leaves out the areas that a pack added and that
//!   still hold the pack's globs (`areas_at_their_pack_globs`);
//! - a `see` link is dangling only when no source holds its id: an excluded,
//!   inactive, or archived id still resolves (`see_is_held`,
//!   `pack_see_findings`);
//! - the `CLAUDE.md` import: a `CLAUDE.md` that does not hold `@AGENTS.md`
//!   keeps the standing rules from Claude Code (`claude_import_finding`).
//!
//! `override_listing` is no check: it lists every override and exclusion
//! for the output of `check-knowledge`, and never fails the gate.
//!
//! The `CLAUDE.md` import is the whitespace-delimited token `@AGENTS.md`,
//! the token that Claude Code reads as an import
//! (code.claude.com/docs/en/memory, "Import additional files"). The check
//! pairs no delimiters and parses no Markdown
//! (`quality.lint-the-token-not-the-document`). So it takes the token inside
//! a code span or a fenced block as an import, although Claude Code skips
//! one there (`a_token_inside_a_code_span_counts_as_the_import`), and it
//! takes no path that spells the file another way, such as `@./AGENTS.md`
//! (`a_path_that_spells_the_file_another_way_is_no_import`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

use crate::install::marker::read_pins_for_gate;
use crate::packs::{pack_sha256, topic_of};
use crate::root::INSTALL_MARKER;

use super::model::{Base, Source};

/// The file that holds the overrides and the exclusions.
const OVERRIDES_FILE: &str = "knowledge/overrides.json";

/// The file that is the one list of areas.
const AREAS_FILE: &str = "knowledge/areas.json";

/// The finding for a `CLAUDE.md` that does not import `AGENTS.md`.
const CLAUDE_IMPORT_LINE: &str =
    "CLAUDE.md: the standing rules do not reach Claude Code; add @AGENTS.md to CLAUDE.md";

/// The pack-layer findings that need no valid render: the pins, the
/// validity of `overrides.json`, one id one source, and an id that is both
/// active and archived. `archive_files` maps each archived id to the first
/// archive file that holds it (`check::check_archive`).
pub(crate) fn pack_findings(base: &Base, archive_files: &BTreeMap<String, String>) -> Vec<String> {
    let mut findings = pin_findings(base);
    findings.extend(override_findings(base));
    findings.extend(duplicate_findings(base));
    findings.extend(archived_and_active_findings(base, archive_files));
    findings
}

/// The finding for a copy that differs from its pin.
pub(crate) fn pin_mismatch_line(pack: &str) -> String {
    format!(
        "knowledge/packs/{pack}.json: the pack \"{pack}\" differs from its pin in {INSTALL_MARKER}; \
         put your change in {OVERRIDES_FILE} and restore the copy"
    )
}

/// One finding for each copy that fails its pin or has none, in pack
/// order, then one for each pin that has no copy.
fn pin_findings(base: &Base) -> Vec<String> {
    let pins = match read_pins_for_gate(&base.root) {
        Ok(pins) => pins,
        Err(finding) => return vec![finding],
    };
    let mut findings = Vec::new();
    for copy in &base.packs {
        match pins.get(&copy.name) {
            None => findings.push(format!(
                "knowledge/packs/{0}.json: the pack \"{0}\" has no pin in {INSTALL_MARKER}",
                copy.name
            )),
            Some(pin) if pin.sha256 != pack_sha256(&copy.raw) => {
                findings.push(pin_mismatch_line(&copy.name));
            }
            Some(_) => {}
        }
    }
    for name in pins.keys() {
        if !base.packs.iter().any(|copy| copy.name == *name) {
            findings.push(format!(
                "{INSTALL_MARKER}: the pin for the pack \"{name}\" has no copy at \
                 knowledge/packs/{name}.json"
            ));
        }
    }
    findings
}

/// The items of `raw[key]`, or none when `raw` has no such array.
fn items<'a>(raw: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    raw.get(key).and_then(Value::as_array).into_iter().flatten()
}

/// The id of an override, an exclusion, or an entry.
fn id_of(item: &Value) -> Option<&str> {
    item.get("id").and_then(Value::as_str)
}

/// The finding for an override or exclusion (`kind`) of `id` that changes
/// nothing: a local id, or an id that no installed pack ships.
fn unshipped_finding(
    kind: &str,
    id: &str,
    base: &Base,
    shipped: &BTreeSet<&str>,
) -> Option<String> {
    let at = format!("{OVERRIDES_FILE} {id}");
    if base
        .entries
        .get(id)
        .is_some_and(|entry| entry.source == Source::Local)
    {
        let rule = if kind == "override" {
            "an override changes a pack entry only"
        } else {
            "an exclusion drops a pack entry only"
        };
        return Some(format!("{at}: {kind} of a local id; {rule}"));
    }
    (!shipped.contains(id)).then(|| format!("{at}: {kind} of an id that no installed pack ships"))
}

/// The findings of spec L5, in file order: overrides first, then
/// exclusions.
fn override_findings(base: &Base) -> Vec<String> {
    let shipped: BTreeSet<&str> = base
        .packs
        .iter()
        .flat_map(|pack| pack.entries.iter())
        .filter_map(id_of)
        .collect();
    let mut findings = Vec::new();
    let mut overrides_per_id: BTreeMap<&str, usize> = BTreeMap::new();
    for id in items(&base.overrides_raw, "overrides").filter_map(id_of) {
        findings.extend(unshipped_finding("override", id, base, &shipped));
        let count = overrides_per_id.entry(id).or_default();
        *count += 1;
        if *count == 2 {
            findings.push(format!(
                "{OVERRIDES_FILE} {id}: two overrides name the same id"
            ));
        }
    }
    for id in items(&base.overrides_raw, "exclusions").filter_map(id_of) {
        findings.extend(unshipped_finding("exclusion", id, base, &shipped));
        if overrides_per_id.contains_key(id) {
            findings.push(format!(
                "{OVERRIDES_FILE} {id}: an override and an exclusion name the same id"
            ));
        }
    }
    findings
}

/// The topic of the local entry `id`: the stem of its topic file.
fn local_topic<'a>(base: &'a Base, id: &'a str) -> &'a str {
    base.raw_entries
        .get(id)
        .and_then(|raw| raw.get("topic"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| topic_of(id))
}

/// The file that `source` keeps the entry `id` in.
fn source_file(base: &Base, id: &str, source: &Source) -> String {
    match source {
        Source::Local => format!("knowledge/{}.json", local_topic(base, id)),
        Source::Pack(name) => format!("knowledge/packs/{name}.json"),
    }
}

/// One finding for each pair of sources that ship one id, in the shape of
/// the 1.4.0 finding for two local files: the source that the base ignores
/// first, the one it keeps after `also in`. Whatever the status of the kept
/// entry, the pair is one source too many.
fn duplicate_findings(base: &Base) -> Vec<String> {
    base.duplicates
        .iter()
        .map(|(id, kept, ignored)| {
            format!(
                "{} {id}: duplicate id (also in {})",
                source_file(base, id, ignored),
                source_file(base, id, kept)
            )
        })
        .collect()
}

/// One finding for each active entry whose id is also archived, naming the
/// file of the active entry and the archive file. An id that an exclusion or
/// an inactive status keeps out of the active set may be archived.
fn archived_and_active_findings(
    base: &Base,
    archive_files: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut findings = Vec::new();
    for id in &base.archived_ids {
        // `check_archive` reports an archive file it cannot read again, and
        // then holds no file to name for the ids the base read from it.
        let (Some(entry), Some(archive_file)) = (base.entries.get(id), archive_files.get(id))
        else {
            continue;
        };
        findings.push(format!(
            "{} {id}: id is also archived in {archive_file}; an id is active or archived, never \
             both",
            source_file(base, id, &entry.source)
        ));
    }
    findings
}

/// The areas whose dead globs the gate leaves out (spec C16): an area that a
/// pin's `addedAreas` records and whose globs equal the globs of an installed
/// pack that declares it. A pack declares a convention that a new project may
/// not have files for yet. An area that the project wrote, and an area whose
/// globs the project changed, are not in the set. A marker that cannot give
/// its pins leaves the set empty: `pin_findings` names that fault in the same
/// run.
pub(crate) fn areas_at_their_pack_globs(base: &Base) -> BTreeSet<String> {
    let Ok(pins) = read_pins_for_gate(&base.root) else {
        return BTreeSet::new();
    };
    let recorded: BTreeSet<&str> = pins
        .values()
        .flat_map(|pin| pin.added_areas.iter().map(String::as_str))
        .collect();
    base.areas
        .iter()
        .filter(|(area, definition)| {
            recorded.contains(area.as_str())
                && base.packs.iter().any(|pack| {
                    pack.areas
                        .iter()
                        .any(|(name, globs)| name == area && *globs == definition.paths)
                })
        })
        .map(|(area, _)| area.clone())
        .collect()
}

/// One finding for each active entry whose `area` is no key of
/// `areas.json`, in id order, and for each area that a pack declares and
/// the `$defs.area` enum of an install that keeps it lacks. An
/// `areas.json` that is no object is the schema check's finding, so no
/// entry is reported for it. An entry with no area is the schema check's
/// finding too.
pub(crate) fn area_findings(base: &Base) -> Vec<String> {
    let mut findings = Vec::new();
    if let Some(areas) = base.areas_raw.as_object() {
        let mut ids: Vec<&String> = base.entries.keys().collect();
        ids.sort();
        for id in ids {
            let entry = &base.entries[id];
            if !entry.area.is_empty() && !areas.contains_key(&entry.area) {
                findings.push(format!(
                    "{} {id}: area \"{}\" is no key of {AREAS_FILE}",
                    source_file(base, id, &entry.source),
                    entry.area
                ));
            }
        }
    }
    if let Some(allowed) = base
        .schema
        .pointer("/$defs/area/enum")
        .and_then(Value::as_array)
    {
        for pack in &base.packs {
            for (area, _) in &pack.areas {
                if !allowed.iter().any(|name| name.as_str() == Some(area)) {
                    findings.push(format!(
                        "knowledge/schema.json: $defs.area lists no area \"{area}\", which the \
                         pack \"{}\" declares; add \"{area}\" to the enum, or remove $defs.area",
                        pack.name
                    ));
                }
            }
        }
    }
    findings
}

/// `true` when some source holds `id`: an active entry, an excluded or
/// inactive pack entry, or an archived entry. A `see` link to such an id is
/// not dangling.
pub(crate) fn see_is_held(base: &Base, id: &str) -> bool {
    base.entries.contains_key(id)
        || base.excluded.contains_key(id)
        || base.inactive.contains_key(id)
        || base.archived_ids.contains(id)
}

/// One finding for each `see` link of an active pack entry that no source
/// holds, naming the pack file. The links of a local entry are checked in
/// `check_base`, which keeps their 1.4.0 coercion of a non-string link.
pub(crate) fn pack_see_findings(base: &Base) -> Vec<String> {
    let mut findings = Vec::new();
    for pack in &base.packs {
        let source = Source::Pack(pack.name.clone());
        for item in &pack.entries {
            let Some(id) = id_of(item) else { continue };
            if base
                .entries
                .get(id)
                .is_none_or(|entry| entry.source != source)
            {
                continue;
            }
            for see in items(&base.raw_entries[id], "see").filter_map(Value::as_str) {
                if !see_is_held(base, see) {
                    findings.push(format!(
                        "{} {id}: see \"{see}\" does not exist",
                        source_file(base, id, &source)
                    ));
                }
            }
        }
    }
    findings
}

/// `true` when the text of a `CLAUDE.md` holds the import `@AGENTS.md`: the
/// whitespace-delimited token.
pub(crate) fn imports_agents_md(claude_md: &str) -> bool {
    claude_md
        .split_whitespace()
        .any(|token| token == "@AGENTS.md")
}

/// The finding for a `CLAUDE.md` in `root` that does not import
/// `AGENTS.md`, or for one that cannot be read. An absent `CLAUDE.md`
/// gives none.
pub(crate) fn claude_import_finding(root: &Path) -> Option<String> {
    match fs::read_to_string(root.join("CLAUDE.md")) {
        Ok(text) if imports_agents_md(&text) => None,
        Ok(_) => Some(CLAUDE_IMPORT_LINE.to_string()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => Some(format!("CLAUDE.md: {error}")),
    }
}

/// One line for each override and each exclusion of `overrides.json`, in
/// file order: `override <id>: <fields>` and `exclusion <id>[: <reason>]`.
pub(crate) fn override_listing(base: &Base) -> Vec<String> {
    let mut lines = Vec::new();
    for item in items(&base.overrides_raw, "overrides") {
        let Some(id) = id_of(item) else { continue };
        let fields: Vec<&str> = item
            .get("fields")
            .and_then(Value::as_object)
            .map(|fields| fields.keys().map(String::as_str).collect())
            .unwrap_or_default();
        lines.push(if fields.is_empty() {
            format!("override {id}")
        } else {
            format!("override {id}: {}", fields.join(", "))
        });
    }
    for item in items(&base.overrides_raw, "exclusions") {
        let Some(id) = id_of(item) else { continue };
        lines.push(match item.get("reason").and_then(Value::as_str) {
            Some(reason) => format!("exclusion {id}: {reason}"),
            None => format!("exclusion {id}"),
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::*;
    use crate::rules::model::load_base;
    use crate::rules::test_support::{
        base_with, base_with_packs, entry, pin_every_copy, write_pack_copy,
    };

    /// Writes `value` as pretty JSON at `path`, creating its directory.
    fn write_json(path: &Path, value: &Value) {
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
        fs::write(
            path,
            serde_json::to_string_pretty(value).expect("serialize"),
        )
        .expect("write a JSON file");
    }

    /// The tree's base, loaded again after a test changed a file.
    fn reload(dir: &TempDir) -> Base {
        load_base(dir.path()).expect("loads")
    }

    /// Writes `knowledge/overrides.json`.
    fn write_overrides(dir: &TempDir, overrides: Value, exclusions: Value) {
        write_json(
            &dir.path().join("knowledge/overrides.json"),
            &json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": overrides, "exclusions": exclusions,
            }),
        );
    }

    /// An override of `id` that replaces the summary.
    fn override_of(id: &str) -> Value {
        json!({"id": id, "fields": {"summary": "Changed."}})
    }

    fn no_archive() -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    /// A base with local entry `test.a` and pack `alpha` that ships
    /// `alpha.b`, `alpha.c`, pinned.
    fn alpha_tree() -> TempDir {
        let (dir, _) = base_with_packs(
            &[entry("test.a", json!({}))],
            &[(
                "alpha",
                &[entry("alpha.b", json!({})), entry("alpha.c", json!({}))],
            )],
            &[],
        );
        pin_every_copy(dir.path());
        dir
    }

    // -- the pins --

    /// A tree with no `knowledge/packs/` and no `packs` key, which is every
    /// tree before packs, has no pin finding.
    #[test]
    fn a_tree_with_no_packs_and_no_pins_is_silent() {
        let (_dir, base) = base_with(&[entry("test.a", json!({}))]);
        assert_eq!(pack_findings(&base, &no_archive()), Vec::<String>::new());
    }

    /// A pinned, unchanged copy is clean, and so is a pin whose version
    /// differs from the copy's: only the checksum decides.
    #[test]
    fn a_copy_that_matches_its_pin_is_clean_whatever_the_pinned_version() {
        let dir = alpha_tree();
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            Vec::<String>::new()
        );
        let mut marker: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join(".houserules.json")).unwrap())
                .unwrap();
        marker["packs"]["alpha"]["version"] = json!("0.1.0");
        write_json(&dir.path().join(".houserules.json"), &marker);
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            Vec::<String>::new()
        );
    }

    /// The pin hashes the parsed copy: CRLF line endings and another key
    /// order keep it.
    #[test]
    fn a_copy_with_other_line_endings_and_key_order_matches_its_pin() {
        let dir = alpha_tree();
        let path = dir.path().join("knowledge/packs/alpha.json");
        let Value::Object(map) =
            serde_json::from_str::<Value>(&fs::read_to_string(&path).unwrap()).unwrap()
        else {
            panic!("a pack copy is an object");
        };
        let reversed: Value = Value::Object(map.into_iter().rev().collect());
        fs::write(
            &path,
            serde_json::to_string_pretty(&reversed)
                .unwrap()
                .replace('\n', "\r\n"),
        )
        .unwrap();
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            Vec::<String>::new()
        );
    }

    /// A hand-edited copy fails its pin with one line that names the pack
    /// and says to put the change in `overrides.json`.
    #[test]
    fn a_hand_edited_copy_names_the_pack_and_points_to_overrides_json() {
        let dir = alpha_tree();
        let path = dir.path().join("knowledge/packs/alpha.json");
        let mut copy: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        copy["entries"][0]["summary"] = json!("Edited in place.");
        write_json(&path, &copy);
        let findings = pack_findings(&reload(&dir), &no_archive());
        assert_eq!(
            findings,
            [
                "knowledge/packs/alpha.json: the pack \"alpha\" differs from its pin in \
                 .houserules.json; put your change in knowledge/overrides.json and restore the \
                 copy"
            ]
        );
        assert_eq!(findings[0], pin_mismatch_line("alpha"));
    }

    /// A copy with no pin entry is one line, and so is a pin with no copy.
    #[test]
    fn a_copy_with_no_pin_and_a_pin_with_no_copy_are_one_line_each() {
        let dir = alpha_tree();
        let mut marker: Value =
            serde_json::from_str(&fs::read_to_string(dir.path().join(".houserules.json")).unwrap())
                .unwrap();
        let alpha = marker["packs"]["alpha"].clone();
        marker["packs"] = json!({"ghost": alpha});
        write_json(&dir.path().join(".houserules.json"), &marker);
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            [
                "knowledge/packs/alpha.json: the pack \"alpha\" has no pin in .houserules.json",
                ".houserules.json: the pin for the pack \"ghost\" has no copy at \
                 knowledge/packs/ghost.json",
            ]
        );
    }

    /// With copies and no `.houserules.json` at all, each copy has no pin.
    #[test]
    fn copies_with_no_marker_file_have_no_pins() {
        let dir = alpha_tree();
        fs::remove_file(dir.path().join(".houserules.json")).unwrap();
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            ["knowledge/packs/alpha.json: the pack \"alpha\" has no pin in .houserules.json"]
        );
    }

    /// A marker file that cannot give its pins is named by its relative
    /// name, never by its path: invalid JSON, a `packs` that is no object, a
    /// pin that is no object, a pin with an unknown key, and a pin with no
    /// string `version`, `source`, or `sha256`.
    #[test]
    fn a_marker_file_that_cannot_give_its_pins_is_named() {
        let dir = alpha_tree();
        let marker = dir.path().join(".houserules.json");
        fs::write(&marker, "{").unwrap();
        let findings = pack_findings(&reload(&dir), &no_archive());
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].starts_with(".houserules.json: invalid JSON ("),
            "{findings:?}"
        );
        let pin = json!({"version": "1.0.0", "source": "kit", "sha256": "abc"});
        let without = |key: &str| {
            let mut pin = pin.clone();
            pin.as_object_mut().expect("an object").remove(key);
            json!({"packs": {"alpha": pin}})
        };
        let with = |key: &str, value: Value| {
            let mut pin = pin.clone();
            pin[key] = value;
            json!({"packs": {"alpha": pin}})
        };
        for (content, expected) in [
            (json!([]), ".houserules.json: not a JSON object"),
            (
                json!({"packs": []}),
                ".houserules.json: packs must be an object",
            ),
            (
                json!({"packs": {"alpha": 5}}),
                ".houserules.json: packs.alpha must be an object",
            ),
            (
                without("version"),
                ".houserules.json: packs.alpha: missing \"version\"",
            ),
            (
                with("version", json!(1)),
                ".houserules.json: packs.alpha.version must be a string",
            ),
            (
                without("source"),
                ".houserules.json: packs.alpha: missing \"source\"",
            ),
            (
                with("source", json!(1)),
                ".houserules.json: packs.alpha.source must be a string",
            ),
            (
                without("sha256"),
                ".houserules.json: packs.alpha: missing \"sha256\"",
            ),
            (
                with("sha256", json!(5)),
                ".houserules.json: packs.alpha.sha256 must be a string",
            ),
            (
                with("extra", json!(1)),
                ".houserules.json: packs.alpha: unknown key \"extra\"",
            ),
        ] {
            write_json(&marker, &content);
            assert_eq!(
                pack_findings(&reload(&dir), &no_archive()),
                [expected],
                "{content}"
            );
        }
    }

    /// A copy that declares a newer minor of the pack schema major is read
    /// with the keys that its embedded schema names (spec C13), and the pin
    /// still hashes the whole copy: it passes.
    #[test]
    fn a_copy_with_a_newer_minor_and_unknown_keys_is_clean() {
        let dir = alpha_tree();
        let path = dir.path().join("knowledge/packs/alpha.json");
        let mut copy: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        copy["schema"] = json!("houserules/pack@1.9.0");
        copy["futureKey"] = json!({"anything": [1, 2]});
        copy["entries"][0]["futureField"] = json!("kept in the copy");
        write_json(&path, &copy);
        pin_every_copy(dir.path());
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            Vec::<String>::new()
        );
    }

    // -- the validity of overrides.json (spec L5) --

    /// An override and an exclusion of ids that the installed packs ship
    /// are clean.
    #[test]
    fn an_override_and_an_exclusion_of_pack_ids_are_clean() {
        let dir = alpha_tree();
        write_overrides(
            &dir,
            json!([override_of("alpha.b")]),
            json!([{"id": "alpha.c"}]),
        );
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            Vec::<String>::new()
        );
    }

    /// An override or an exclusion of an id that no installed pack ships:
    /// one line for each, naming the id and the case.
    #[test]
    fn an_override_or_an_exclusion_of_an_id_no_pack_ships_is_one_line() {
        let dir = alpha_tree();
        write_overrides(
            &dir,
            json!([override_of("ghost.x")]),
            json!([{"id": "ghost.y", "reason": "gone"}]),
        );
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            [
                "knowledge/overrides.json ghost.x: override of an id that no installed pack ships",
                "knowledge/overrides.json ghost.y: exclusion of an id that no installed pack \
                 ships",
            ]
        );
    }

    /// An override or an exclusion of a local id.
    #[test]
    fn an_override_or_an_exclusion_of_a_local_id_is_one_line() {
        let (dir, _) = base_with_packs(
            &[entry("test.a", json!({})), entry("test.z", json!({}))],
            &[("alpha", &[entry("alpha.b", json!({}))])],
            &[],
        );
        pin_every_copy(dir.path());
        write_overrides(
            &dir,
            json!([override_of("test.a")]),
            json!([{"id": "test.z"}]),
        );
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            [
                "knowledge/overrides.json test.a: override of a local id; an override changes a \
                 pack entry only",
                "knowledge/overrides.json test.z: exclusion of a local id; an exclusion drops a \
                 pack entry only",
            ]
        );
    }

    /// An override and an exclusion of one id, and two overrides of one
    /// id.
    #[test]
    fn an_override_with_an_exclusion_and_two_overrides_of_one_id_are_one_line_each() {
        let dir = alpha_tree();
        write_overrides(
            &dir,
            json!([
                override_of("alpha.b"),
                override_of("alpha.c"),
                override_of("alpha.c")
            ]),
            json!([{"id": "alpha.b"}]),
        );
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            [
                "knowledge/overrides.json alpha.c: two overrides name the same id",
                "knowledge/overrides.json alpha.b: an override and an exclusion name the same id",
            ]
        );
    }

    /// After `pack remove`, an override of the removed pack's id is the
    /// first case again.
    #[test]
    fn an_override_of_an_id_whose_pack_is_gone_fails() {
        let dir = alpha_tree();
        write_overrides(&dir, json!([override_of("alpha.b")]), json!([]));
        fs::remove_file(dir.path().join("knowledge/packs/alpha.json")).unwrap();
        fs::remove_file(dir.path().join(".houserules.json")).unwrap();
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            ["knowledge/overrides.json alpha.b: override of an id that no installed pack ships"]
        );
    }

    // -- one id, one source --

    /// A local entry and a pack entry with one id: one line that names the
    /// id and both files. The later source, which the base ignores, comes
    /// first, as the 1.4.0 line for two local files does.
    #[test]
    fn a_local_entry_and_a_pack_entry_with_one_id_are_one_line() {
        let (dir, _) = base_with_packs(
            &[entry("alpha.b", json!({}))],
            &[("alpha", &[entry("alpha.b", json!({}))])],
            &[],
        );
        pin_every_copy(dir.path());
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            ["knowledge/packs/alpha.json alpha.b: duplicate id (also in knowledge/test.json)"]
        );
    }

    /// Two packs that ship one id; the first pack in name order is the one
    /// the base keeps, even when that entry is retired.
    #[test]
    fn two_packs_with_one_id_are_one_line_whatever_the_kept_status() {
        let dir = alpha_tree();
        write_pack_copy(
            dir.path(),
            "beta",
            &["alpha", "beta"],
            &[
                entry("alpha.b", json!({"status": "retired"})),
                entry("beta.d", json!({})),
            ],
        );
        pin_every_copy(dir.path());
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            [
                "knowledge/packs/beta.json alpha.b: duplicate id (also in knowledge/packs/alpha.json)"
            ]
        );
        write_pack_copy(
            dir.path(),
            "aardvark",
            &["alpha"],
            &[entry("alpha.b", json!({"status": "retired"}))],
        );
        pin_every_copy(dir.path());
        assert_eq!(
            pack_findings(&reload(&dir), &no_archive()),
            [
                "knowledge/packs/alpha.json alpha.b: duplicate id (also in \
                 knowledge/packs/aardvark.json)",
                "knowledge/packs/beta.json alpha.b: duplicate id (also in \
                 knowledge/packs/aardvark.json)",
            ]
        );
    }

    // -- active and archived --

    /// Writes `knowledge/archive/<topic>.json` with one archived entry and
    /// returns the map `check_archive` would build.
    fn archive(dir: &TempDir, file_topic: &str, id: &str) -> BTreeMap<String, String> {
        write_json(
            &dir.path()
                .join(format!("knowledge/archive/{file_topic}.json")),
            &json!({
                "topic": file_topic, "title": "t",
                "entries": [entry(id, json!({"status": "retired"}))],
            }),
        );
        BTreeMap::from([(
            id.to_string(),
            format!("knowledge/archive/{file_topic}.json"),
        )])
    }

    /// A local entry that is also archived, and a pack entry that is also
    /// archived: one line each, naming the active file and the archive file.
    #[test]
    fn an_id_active_and_archived_is_one_line_naming_both_files() {
        let dir = alpha_tree();
        let files = archive(&dir, "test", "test.a");
        let mut more = archive(&dir, "alpha-old", "alpha.b");
        more.extend(files);
        assert_eq!(
            pack_findings(&reload(&dir), &more),
            [
                "knowledge/packs/alpha.json alpha.b: id is also archived in \
                 knowledge/archive/alpha-old.json; an id is active or archived, never both",
                "knowledge/test.json test.a: id is also archived in knowledge/archive/test.json; \
                 an id is active or archived, never both",
            ]
        );
    }

    /// A pack entry that an exclusion or a retired override takes out of
    /// the active set may be archived: no finding.
    #[test]
    fn an_excluded_or_retired_pack_entry_may_be_archived() {
        let dir = alpha_tree();
        write_overrides(
            &dir,
            json!([{"id": "alpha.b", "fields": {"status": "retired"}}]),
            json!([{"id": "alpha.c"}]),
        );
        let mut files = archive(&dir, "alpha", "alpha.b");
        files.extend(archive(&dir, "alpha-two", "alpha.c"));
        assert_eq!(pack_findings(&reload(&dir), &files), Vec::<String>::new());
    }

    // -- the area of an active entry --

    /// A base with `areas.json` holding `global` only.
    fn areas_tree() -> TempDir {
        let dir = alpha_tree();
        write_json(
            &dir.path().join("knowledge/areas.json"),
            &json!({"global": {"paths": []}}),
        );
        dir
    }

    /// An active entry of any source with an area that `areas.json` lacks
    /// is one line; the override applies first, and an archived entry is
    /// never checked.
    #[test]
    fn an_active_entry_whose_area_is_no_key_of_areas_json_is_one_line() {
        let (dir, _) = base_with_packs(
            &[entry("test.a", json!({"area": "ghost"}))],
            &[(
                "alpha",
                &[
                    entry("alpha.b", json!({"area": "other"})),
                    entry("alpha.c", json!({})),
                ],
            )],
            &[],
        );
        write_json(
            &dir.path().join("knowledge/areas.json"),
            &json!({"global": {"paths": []}}),
        );
        write_overrides(
            &dir,
            json!([
                {"id": "alpha.b", "fields": {"area": "global"}},
                {"id": "alpha.c", "fields": {"area": "third"}},
            ]),
            json!([]),
        );
        write_json(
            &dir.path().join("knowledge/archive/test.json"),
            &json!({"topic": "test", "title": "t",
                "entries": [entry("test.old", json!({"area": "unchecked"}))]}),
        );
        assert_eq!(
            area_findings(&reload(&dir)),
            [
                "knowledge/packs/alpha.json alpha.c: area \"third\" is no key of \
                 knowledge/areas.json",
                "knowledge/test.json test.a: area \"ghost\" is no key of knowledge/areas.json",
            ]
        );
    }

    /// An `areas.json` that is no object is reported by the schema check:
    /// the area check adds no line for every entry.
    #[test]
    fn the_area_check_is_silent_when_areas_json_is_no_object() {
        let dir = alpha_tree();
        write_json(&dir.path().join("knowledge/areas.json"), &json!(["global"]));
        assert_eq!(area_findings(&reload(&dir)), Vec::<String>::new());
    }

    /// On an install that keeps `$defs.area`, an area that a pack declares
    /// and the enum lacks names the enum as the cause, for each pack and
    /// area, in pack and area order. An install with no enum has none.
    #[test]
    fn a_pack_area_that_the_kept_enum_lacks_names_the_enum() {
        let dir = areas_tree();
        write_json(
            &dir.path().join("knowledge/packs/alpha.json"),
            &json!({
                "name": "alpha", "version": "1.0.0", "schema": "houserules/pack@1.0.0",
                "topics": {"alpha": {"title": "A pack."}},
                "areas": {"global": {"paths": []}, "docs": {"paths": ["docs/**"]}},
                "entries": [],
            }),
        );
        assert_eq!(area_findings(&reload(&dir)), Vec::<String>::new());
        write_json(
            &dir.path().join("knowledge/schema.json"),
            &json!({"$defs": {"area": {"enum": ["global", "process"]}}}),
        );
        assert_eq!(
            area_findings(&reload(&dir)),
            [
                "knowledge/schema.json: $defs.area lists no area \"docs\", which the pack \
                 \"alpha\" declares; add \"docs\" to the enum, or remove $defs.area"
            ]
        );
    }

    // -- see --

    /// An id that a source holds in any state resolves.
    #[test]
    fn a_see_link_resolves_to_an_active_excluded_inactive_or_archived_id() {
        let dir = alpha_tree();
        write_overrides(
            &dir,
            json!([{"id": "alpha.b", "fields": {"status": "retired"}}]),
            json!([{"id": "alpha.c"}]),
        );
        archive(&dir, "test", "test.old");
        let base = reload(&dir);
        for id in ["test.a", "alpha.b", "alpha.c", "test.old"] {
            assert!(see_is_held(&base, id), "{id}");
        }
        assert!(!see_is_held(&base, "ghost.x"));
    }

    /// A pack entry's `see` link to an id no source holds is one line that
    /// names the pack file; a link to an excluded id of the same pack is
    /// not dangling.
    #[test]
    fn a_pack_entry_that_links_to_an_id_no_source_holds_is_one_line() {
        let (dir, _) = base_with_packs(
            &[],
            &[(
                "alpha",
                &[
                    entry("alpha.b", json!({"see": ["alpha.c", "ghost.x"]})),
                    entry("alpha.c", json!({})),
                ],
            )],
            &["alpha.c"],
        );
        assert_eq!(
            pack_see_findings(&reload(&dir)),
            ["knowledge/packs/alpha.json alpha.b: see \"ghost.x\" does not exist"]
        );
    }

    // -- the CLAUDE.md import --

    /// Writes `CLAUDE.md` with `text` in a new directory and returns the
    /// finding for it.
    fn claude_finding(text: &str) -> Option<String> {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("CLAUDE.md"), text).unwrap();
        claude_import_finding(dir.path())
    }

    const CLAUDE_LINE: &str =
        "CLAUDE.md: the standing rules do not reach Claude Code; add @AGENTS.md to CLAUDE.md";

    /// An absent `CLAUDE.md` is never a finding.
    #[test]
    fn an_absent_claude_md_is_no_finding() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(claude_import_finding(dir.path()), None);
    }

    /// A `CLAUDE.md` that holds the import passes, wherever the token
    /// stands; one that does not is one line, whole.
    #[test]
    fn a_claude_md_without_the_import_is_one_line() {
        for text in [
            "@AGENTS.md",
            "@AGENTS.md\n",
            "# Notes\n\n@AGENTS.md\n\nMore.\n",
            "Intro.\n@AGENTS.md",
            "a\t@AGENTS.md\tb",
            "@AGENTS.md\r\nmore",
        ] {
            assert_eq!(claude_finding(text), None, "{text:?}");
        }
        for text in [
            "",
            "# Test\n",
            "Read AGENTS.md first.\n",
            "AGENTS.md\n",
            "@AGENTS.md.\n",
            "x@AGENTS.md\n",
            "@AGENTS.mdx\n",
            "@agents.md\n",
        ] {
            assert_eq!(
                claude_finding(text).as_deref(),
                Some(CLAUDE_LINE),
                "{text:?}"
            );
        }
    }

    /// The token inside a code span is no import for Claude Code, and the
    /// check, which parses no Markdown, takes it as one (a limit).
    #[test]
    fn a_token_inside_a_code_span_counts_as_the_import() {
        assert_eq!(claude_finding("Use ` @AGENTS.md ` here.\n"), None);
        assert_eq!(claude_finding("```\n@AGENTS.md\n```\n"), None);
    }

    /// Another spelling of the path is no import for this check (a limit).
    #[test]
    fn a_path_that_spells_the_file_another_way_is_no_import() {
        assert_eq!(
            claude_finding("@./AGENTS.md\n").as_deref(),
            Some(CLAUDE_LINE)
        );
    }

    /// A `CLAUDE.md` that cannot be read is named with the operating
    /// system's own error, not skipped.
    #[test]
    fn a_claude_md_that_cannot_be_read_is_named() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("CLAUDE.md")).unwrap();
        let finding = claude_import_finding(dir.path()).expect("a finding");
        assert!(finding.starts_with("CLAUDE.md: "), "{finding}");
        assert!(!finding.contains("standing rules"), "{finding}");
    }

    // -- the listing --

    /// Every override and exclusion is listed in file order; a tree with
    /// none lists none.
    #[test]
    fn the_listing_names_every_override_and_exclusion() {
        let dir = alpha_tree();
        assert_eq!(override_listing(&reload(&dir)), Vec::<String>::new());
        write_overrides(
            &dir,
            json!([
                {"id": "alpha.b", "fields": {"summary": "x", "standing": null}},
                override_of("ghost.z"),
            ]),
            json!([{"id": "alpha.c", "reason": "not for us"}, {"id": "ghost.y"}]),
        );
        assert_eq!(
            override_listing(&reload(&dir)),
            [
                "override alpha.b: summary, standing",
                "override ghost.z: summary",
                "exclusion alpha.c: not for us",
                "exclusion ghost.y",
            ]
        );
    }
}
