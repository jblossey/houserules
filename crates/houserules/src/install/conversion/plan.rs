//! The pure part of the 1.x conversion (spec section 5, steps 0 to 7): a
//! function from a 1.x tree and the packs of the binary to the layout of a
//! 2.0 install. It reads no file and writes none, so a caller can refuse
//! before it writes, and the merge-base audit of batch 37 can map a 1.x merge
//! base with it.
//!
//! The function assigns each entry to a pack by its id, against the id lists
//! of the binary's packs. It reads no entry text to choose a pack. It reads
//! the text of an entry only to decide whether the project changed it
//! (`is_changed`), which carries the meaning of `baseline::classify`:
//! Modified. Each step acts on its own state of the tree and on no other, so
//! a run that an interruption cut short finishes at the next run (the writer
//! in `conversion` has the order of the writes).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use crate::baseline::{self, Status};
use crate::pack::one_source::{OverridesEdit, differing_fields, overrides_errors, shipped_entries};
use crate::packs::{PackCopy, is_entry_id, pack_sha256, topic_of};
use crate::rules::imports_agents_md;

use super::super::AREAS_FILE;
use super::super::hand_steps;
use super::d3::{ReleasedEntry, released_entries};

/// A local topic file or an archive file, parsed.
pub(super) struct TopicFile {
    /// The path below the install root, `/`-separated.
    pub(super) path: String,
    /// The parsed file.
    pub(super) content: Value,
}

/// The state of `knowledge/overrides.json` in the tree.
pub(super) enum OverridesFile {
    /// The file does not exist.
    Absent,
    /// An overrides document: an earlier run of the conversion wrote it.
    Document(Value),
    /// Any other file of that name: a topic file of houserules 1.x.
    Topic,
}

/// What a `.json` path under `knowledge/packs/` holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CopyState {
    /// A file that parses as JSON, with the `sha256` of its canonical form
    /// (`packs::pack_sha256`): the form that a pin records.
    Json(String),
    /// A directory, or a file that is no JSON.
    Foreign,
}

/// What the conversion reads of a 1.x tree.
pub(super) struct Tree {
    /// The values of the `overrides` key of the marker, or `None` when the
    /// marker has no such key.
    pub(super) overrides: Option<Vec<String>>,
    /// The values of the `ownedPaths` key of the marker.
    pub(super) owned_paths: Vec<String>,
    /// The `baselines` of the marker.
    pub(super) baselines: Map<String, Value>,
    /// The `version` of the marker: the release of 1.x that last ran the
    /// install, whose text of an entry is the base of what the project changed.
    pub(super) version: Option<String>,
    /// The `sha256` that the marker pins for each pack, by pack name.
    pub(super) pinned: BTreeMap<String, String>,
    /// What each `.json` path under `knowledge/packs/` holds, by pack name.
    pub(super) copies: BTreeMap<String, CopyState>,
    /// `true` when `knowledge/areas.json` exists.
    pub(super) areas_file_exists: bool,
    /// The local topic files, sorted by path.
    pub(super) topic_files: Vec<TopicFile>,
    /// The files of `knowledge/archive/`, sorted by path.
    pub(super) archive_files: Vec<TopicFile>,
    /// The state of `knowledge/overrides.json`.
    pub(super) overrides_file: OverridesFile,
    /// The text of `CLAUDE.md`, or `None` when the file does not exist.
    pub(super) claude_md: Option<String>,
    /// `true` when `.claude/rules/standing-rules.md` exists.
    pub(super) standing_rules: bool,
}

/// Where an id of a kit pack goes.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Home {
    /// The kit pack ships the entry as it is: nothing is written for the id.
    Pack,
    /// An override carries these fields.
    Override(Map<String, Value>),
    /// An exclusion drops the entry.
    Exclusion,
    /// An override with `status: retired` and these fields (an archived
    /// entry).
    Retired(Map<String, Value>),
    /// An earlier run wrote the override or the exclusion of the id. The
    /// document keeps it.
    Handled,
}

/// A topic that `ownedPaths` keeps local (step 4).
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Covered {
    /// The topic name.
    pub(super) topic: String,
    /// The kit pack that serves the topic and stays out of the install.
    pub(super) pack: String,
}

/// What the conversion does with a tree.
pub(super) struct Plan {
    /// Step 0: the hand step that stops the conversion before any write.
    pub(super) stop: Option<String>,
    /// `ownedPaths` after step 1: the paths of the tree, then the paths of
    /// the `overrides` key that the list lacks.
    pub(super) owned_paths: Vec<String>,
    /// The ids of the `overrides` key that no kit pack ships. They change
    /// nothing in 2.0.
    pub(super) ignored_ids: Vec<String>,
    /// Where each id of a kit pack goes (steps 1 to 3). Only the ids of the
    /// packs in `install_packs` have a home: a pack that the tree has
    /// installed already was decided by an earlier run, whose overrides
    /// document holds the result.
    pub(super) homes: BTreeMap<String, Home>,
    /// `knowledge/overrides.json` when the conversion changes it.
    pub(super) overrides_doc: Option<Value>,
    /// The local topic files that lose entries, with their new content.
    pub(super) topic_writes: Vec<(String, Value)>,
    /// The local topic files that the move empties (L9).
    pub(super) topic_removals: Vec<String>,
    /// The archive files that lose entries, with their new content.
    pub(super) archive_writes: Vec<(String, Value)>,
    /// The archive files that the move empties.
    pub(super) archive_removals: Vec<String>,
    /// The topics that `ownedPaths` keeps local (step 4).
    pub(super) covered: Vec<Covered>,
    /// The kit packs that step 5 installs, in name order.
    pub(super) install_packs: Vec<String>,
    /// The baseline keys that step 6 removes: every key that is an id.
    pub(super) id_baselines: Vec<String>,
    /// Step 7: `.claude/rules/standing-rules.md` goes.
    pub(super) remove_standing_rules: bool,
    /// Step 7: `CLAUDE.md` does not import `AGENTS.md`.
    pub(super) claude_md_hand_step: bool,
}

/// The entries of `content["entries"]`, or none when it holds no array.
fn entries_of(content: &Value) -> &[Value] {
    content
        .get("entries")
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The string `id` of an entry.
fn id_of(entry: &Value) -> Option<&str> {
    entry.get("id").and_then(Value::as_str)
}

/// The canonical bytes one entry hashes to: its compact JSON, in the key
/// order of the entry. A baseline of 1.x is the hash of these bytes.
fn canonical_entry_bytes(entry: &Value) -> Vec<u8> {
    serde_json::to_vec(entry).expect("a JSON Value always serializes")
}

/// `true` when the project changed `entry`: the meaning of `Modified` in
/// `baseline::classify`. The entry differs from its recorded baseline and
/// from `shipped`, the entry of the pack; with no baseline, from `shipped`
/// alone. An entry that the project never changed is unchanged even when the
/// text of the kit moved since its release, and an entry that equals a text
/// that a release shipped (`released`) is the kit's whatever the baselines say.
fn is_changed(
    entry: &Value,
    recorded: Option<&str>,
    shipped: &Value,
    released: &[ReleasedEntry],
) -> bool {
    baseline::classify(
        recorded,
        Some(&canonical_entry_bytes(entry)),
        &canonical_entry_bytes(shipped),
        false,
    ) == Status::Modified
        && !released.iter().any(|text| text.entry == *entry)
}

/// The text of an entry that its 1.x release shipped, from `texts`, every text
/// that a release shipped: the text whose canonical bytes hash to the baseline
/// that the marker recorded for the id, else the text of the release that the
/// marker names. `None` when no release shipped the id, or the marker names
/// neither, so no base exists.
fn released_base<'a>(
    texts: &'a [ReleasedEntry],
    recorded: Option<&str>,
    version: Option<&str>,
) -> Option<&'a Value> {
    recorded
        .and_then(|hash| {
            texts
                .iter()
                .find(|text| baseline::hash(&canonical_entry_bytes(&text.entry)) == hash)
        })
        .or_else(|| {
            version.and_then(|version| {
                texts
                    .iter()
                    .find(|text| text.versions.iter().any(|found| found == version))
            })
        })
        .map(|text| &text.entry)
}

/// The fields of the override for the entry `entry` of the project: the fields
/// that differ from `shipped`, the entry of the pack, and that the project
/// changed from `released`, the text that its release shipped. A field that the
/// kit changed after that release, and that the project left alone, reaches the
/// install through the pack. With no released text, every field that differs
/// from the pack is the project's.
fn changed_fields(entry: &Value, released: Option<&Value>, shipped: &Value) -> Map<String, Value> {
    let mut fields = differing_fields(entry, shipped);
    if let Some(released) = released {
        let by_the_project = differing_fields(entry, released);
        fields.retain(|key, _| by_the_project.contains_key(key));
    }
    fields
}

/// The ids of the `overrides` and `exclusions` that an overrides document
/// names.
fn ids_in_document(document: &Value) -> BTreeSet<String> {
    ["overrides", "exclusions"]
        .iter()
        .filter_map(|key| document.get(key).and_then(Value::as_array))
        .flatten()
        .filter_map(id_of)
        .map(str::to_string)
        .collect()
}

/// `Home::Override` for `fields`, or `Home::Pack` when no field differs: the
/// entry equals the entry of the pack.
fn override_or_pack(fields: Map<String, Value>) -> Home {
    if fields.is_empty() {
        Home::Pack
    } else {
        Home::Override(fields)
    }
}

/// The fields of the override that retires an archived entry: `status:
/// retired`, then `fields` but a status of their own.
fn retired_fields(fields: Map<String, Value>) -> Map<String, Value> {
    let mut retired = Map::new();
    retired.insert("status".to_string(), json!("retired"));
    retired.extend(fields.into_iter().filter(|(key, _)| key != "status"));
    retired
}

/// The topics that a pack of `packs` serves and that `owned` lists as a
/// topic file (step 4), with the pack that serves each.
fn covered_topics(packs: &[PackCopy], owned: &[String]) -> Vec<Covered> {
    let mut covered = Vec::new();
    for pack in packs {
        for (topic, _) in &pack.topics {
            if owned.contains(&format!("knowledge/{topic}.json")) {
                covered.push(Covered {
                    topic: topic.clone(),
                    pack: pack.name.clone(),
                });
            }
        }
    }
    covered
}

/// `true` when step 5 has nothing left to do for the kit pack `pack`: the
/// marker pins it, and the copy at its path matches the `sha256` of the pin.
/// A pack with no pin, or with a pin whose copy is absent or does not match,
/// is the state of step 5 (spec C20), whichever write of an interrupted run
/// left it.
///
/// A pack with no pin whose copy path holds anything but the copy of the
/// binary (another file, or a directory) is one line and no plan: an
/// interrupted run writes the pin before the copy, so no run of this binary
/// leaves that state, and the file is the project's.
fn is_installed(tree: &Tree, pack: &PackCopy) -> Result<bool, String> {
    let copy = tree.copies.get(&pack.name);
    match tree.pinned.get(&pack.name) {
        Some(pinned) => Ok(matches!(copy, Some(CopyState::Json(sha)) if sha == pinned)),
        None => match copy {
            None => Ok(false),
            Some(CopyState::Json(sha)) if *sha == pack_sha256(&pack.raw) => Ok(false),
            Some(_) => Err(format!(
                "knowledge/packs/{0}.json: this path holds something other than the copy of the kit pack {0} that houserules {1} writes, and .houserules.json has no pin for the pack; move it away, then run houserules update again",
                pack.name,
                env!("CARGO_PKG_VERSION")
            )),
        },
    }
}

/// The line that stops the conversion when `ownedPaths` lists `path`, the
/// path does not exist, and the conversion must write it: `update` leaves an
/// owned path as it finds it, so it cannot create the file (spec C21). A 1.x
/// marker spells the list `overrides`; the conversion moves it to
/// `ownedPaths`.
pub(super) fn owned_path_is_absent(path: &str) -> String {
    format!(
        "{path}: this file does not exist, and .houserules.json lists it in ownedPaths (the key is overrides in a 1.x marker), so update cannot restore it; restore it, or remove it from ownedPaths, then run houserules update again"
    )
}

/// Refuses an absent `knowledge/areas.json` that `ownedPaths` lists while step
/// 5 installs a pack, which writes the file. The conversion stops before any
/// write.
fn refuse_owned_absent_areas_file(tree: &Tree, plan: &Plan) -> Result<(), String> {
    let owned = plan.owned_paths.iter().any(|path| path == AREAS_FILE);
    if owned && !tree.areas_file_exists && !plan.install_packs.is_empty() {
        return Err(owned_path_is_absent(AREAS_FILE));
    }
    Ok(())
}

/// Plans the conversion of `tree` against the packs of the binary. A tree
/// that the conversion cannot map without losing an entry is one line and no
/// plan: an id in two local topic files, an id that is active and archived
/// at once, a copy path of an unpinned kit pack that holds a file of the
/// project, or an absent `knowledge/areas.json` that `ownedPaths` lists while
/// a pack is installed (`refuse_owned_absent_areas_file`).
pub(super) fn plan(tree: &Tree, packs: &[PackCopy]) -> Result<Plan, String> {
    let mut plan = Plan {
        stop: None,
        owned_paths: tree.owned_paths.clone(),
        ignored_ids: Vec::new(),
        homes: BTreeMap::new(),
        overrides_doc: None,
        topic_writes: Vec::new(),
        topic_removals: Vec::new(),
        archive_writes: Vec::new(),
        archive_removals: Vec::new(),
        covered: Vec::new(),
        install_packs: Vec::new(),
        id_baselines: Vec::new(),
        remove_standing_rules: false,
        claude_md_hand_step: false,
    };
    if matches!(tree.overrides_file, OverridesFile::Topic) {
        plan.stop = Some(hand_steps::reserved_topic());
        return Ok(plan);
    }

    // A value of `overrides` is an id when it has the shape of an entry id, and
    // a path otherwise: every path that release 1.x listed holds a slash or an
    // upper-case letter (`packs::is_entry_id`'s test names the cases).
    let (listed_ids, listed_paths): (Vec<&String>, Vec<&String>) = tree
        .overrides
        .iter()
        .flatten()
        .partition(|value| is_entry_id(value));
    for path in listed_paths {
        if !plan.owned_paths.contains(path) {
            plan.owned_paths.push(path.clone());
        }
    }

    plan.covered = covered_topics(packs, &plan.owned_paths);
    let covered: BTreeSet<&str> = plan.covered.iter().map(|c| c.topic.as_str()).collect();
    let covered_packs: BTreeSet<&str> = plan.covered.iter().map(|c| c.pack.as_str()).collect();
    for pack in packs {
        if !covered_packs.contains(pack.name.as_str()) && !is_installed(tree, pack)? {
            plan.install_packs.push(pack.name.clone());
        }
    }
    let shipped = shipped_entries(packs);
    let is_moved = |id: &str| shipped.contains_key(id) && !covered.contains(topic_of(id));

    let mut local: BTreeMap<&str, (&str, &Value)> = BTreeMap::new();
    for file in &tree.topic_files {
        for entry in entries_of(&file.content) {
            let Some(id) = id_of(entry).filter(|id| is_moved(id)) else {
                continue;
            };
            if let Some((first, _)) = local.insert(id, (file.path.as_str(), entry)) {
                return Err(if first == file.path {
                    format!(
                        "{id} is twice in {first}; keep one entry, then run houserules update again"
                    )
                } else {
                    format!(
                        "{id} is in {first} and {}; keep one entry, then run houserules update again",
                        file.path
                    )
                });
            }
        }
    }
    let mut archived: BTreeMap<&str, &Value> = BTreeMap::new();
    for file in &tree.archive_files {
        for entry in entries_of(&file.content) {
            let Some(id) = id_of(entry).filter(|id| is_moved(id)) else {
                continue;
            };
            if let Some((active, _)) = local.get(id) {
                return Err(format!(
                    "{id} is active in {active} and archived in {}; remove one of the two, then run houserules update again",
                    file.path
                ));
            }
            archived.entry(id).or_insert(entry);
        }
    }

    plan.ignored_ids = listed_ids
        .iter()
        .filter(|id| !shipped.contains_key(id.as_str()))
        .map(|id| id.to_string())
        .collect();
    let listed: BTreeSet<&str> = listed_ids.iter().map(|id| id.as_str()).collect();
    let document = match &tree.overrides_file {
        OverridesFile::Document(document) => document.clone(),
        _ => Value::Null,
    };
    let handled = ids_in_document(&document);

    let released = released_entries();
    for (id, (pack_name, shipped_entry)) in &shipped {
        if covered.contains(topic_of(id))
            || !plan.install_packs.iter().any(|name| name == pack_name)
        {
            continue;
        }
        let recorded = tree.baselines.get(*id).and_then(Value::as_str);
        let texts = released.get(*id).map_or(&[][..], Vec::as_slice);
        let base = released_base(texts, recorded, tree.version.as_deref());
        let home = if let Some(entry) = archived.get(id) {
            let fields = if listed.contains(id) || is_changed(entry, recorded, shipped_entry, texts)
            {
                changed_fields(entry, base, shipped_entry)
            } else {
                Map::new()
            };
            Home::Retired(retired_fields(fields))
        } else if let Some((_, entry)) = local.get(id) {
            if listed.contains(id) {
                override_or_pack(changed_fields(entry, base, shipped_entry))
            } else if handled.contains(*id) {
                Home::Handled
            } else if is_changed(entry, recorded, shipped_entry, texts) {
                override_or_pack(changed_fields(entry, base, shipped_entry))
            } else {
                Home::Pack
            }
        } else if listed.contains(id) || (!handled.contains(*id) && recorded.is_some()) {
            Home::Exclusion
        } else if handled.contains(*id) {
            Home::Handled
        } else {
            Home::Pack
        };
        plan.homes.insert(id.to_string(), home);
    }

    let mut edit = OverridesEdit::new(&document);
    for (id, home) in &plan.homes {
        match home {
            Home::Override(fields) | Home::Retired(fields) => {
                edit.replace_items_of(id);
                edit.add_override(id, fields.clone());
            }
            Home::Exclusion => {
                edit.replace_items_of(id);
                edit.add_exclusion(id);
            }
            Home::Pack | Home::Handled => {}
        }
    }
    plan.overrides_doc = edit.into_changed();
    if let Some(doc) = &plan.overrides_doc {
        let errors = overrides_errors(doc);
        if !errors.is_empty() {
            return Err(format!(
                "the conversion cannot write knowledge/overrides.json: {}",
                errors.join("; ")
            ));
        }
    }

    refuse_owned_absent_areas_file(tree, &plan)?;

    (plan.topic_writes, plan.topic_removals) = without_moved(&tree.topic_files, &is_moved);
    (plan.archive_writes, plan.archive_removals) = without_moved(&tree.archive_files, &is_moved);

    plan.id_baselines = tree
        .baselines
        .keys()
        .filter(|key| is_entry_id(key))
        .cloned()
        .collect();
    plan.remove_standing_rules = tree.standing_rules;
    plan.claude_md_hand_step = tree
        .claude_md
        .as_deref()
        .is_some_and(|text| !imports_agents_md(text));
    Ok(plan)
}

/// The files of `files` that lose an entry whose id `is_moved` accepts: the
/// files that stay with their new content, and the files that the move
/// empties. A file with no entry to move is in neither list; a file that
/// holds an entry with no id or an id that stays does not go (L9).
fn without_moved(
    files: &[TopicFile],
    is_moved: &dyn Fn(&str) -> bool,
) -> (Vec<(String, Value)>, Vec<String>) {
    let mut writes = Vec::new();
    let mut removals = Vec::new();
    for file in files {
        let (kept, moved): (Vec<&Value>, Vec<&Value>) = entries_of(&file.content)
            .iter()
            .partition(|entry| !id_of(entry).is_some_and(is_moved));
        if moved.is_empty() {
            continue;
        }
        if kept.is_empty() {
            removals.push(file.path.clone());
        } else {
            let mut content = file.content.clone();
            content["entries"] = Value::Array(kept.into_iter().cloned().collect());
            writes.push((file.path.clone(), content));
        }
    }
    (writes, removals)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A schema-valid entry with the given id and summary.
    fn entry(id: &str, summary: &str) -> Value {
        json!({
            "id": id, "kind": "rule", "area": "global", "standing": true,
            "summary": summary, "body": ["Body."], "tags": ["test"],
            "source": {"date": "2026-10-01", "by": "user"},
        })
    }

    /// A pack of the binary that serves the topic `name` and ships the
    /// entries `ids`, each with the summary `Kit`.
    fn pack(name: &str, ids: &[&str]) -> PackCopy {
        PackCopy {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            schema: "houserules/pack@1.0.0".to_string(),
            topics: vec![(name.to_string(), name.to_string())],
            areas: Vec::new(),
            entries: ids.iter().map(|id| entry(id, "Kit")).collect(),
            raw: Value::Null,
        }
    }

    /// The hash that 1.x recorded as the baseline of `entry`.
    fn baseline_of(entry: &Value) -> Value {
        json!(baseline::hash(&canonical_entry_bytes(entry)))
    }

    /// A topic file at `path` that holds `entries`.
    fn file(path: &str, entries: Vec<Value>) -> TopicFile {
        TopicFile {
            path: path.to_string(),
            content: json!({"topic": "t", "title": "T", "entries": entries}),
        }
    }

    /// A tree with nothing in it: no marker key, no file.
    fn tree() -> Tree {
        Tree {
            overrides: None,
            owned_paths: Vec::new(),
            baselines: Map::new(),
            version: None,
            pinned: BTreeMap::new(),
            copies: BTreeMap::new(),
            areas_file_exists: true,
            topic_files: Vec::new(),
            archive_files: Vec::new(),
            overrides_file: OverridesFile::Absent,
            claude_md: None,
            standing_rules: false,
        }
    }

    fn packs() -> Vec<PackCopy> {
        vec![
            pack("alpha", &["alpha.one", "alpha.two"]),
            pack("beta", &["beta.one"]),
        ]
    }

    fn home(plan: &Plan, id: &str) -> Home {
        plan.homes[id].clone()
    }

    /// The `sha256` that the pin and the copy of a pack made by `pack()`
    /// carry when the project installed it.
    fn installed_sha() -> String {
        pack_sha256(&Value::Null)
    }

    /// Makes the tree hold the pack `name` as an uninterrupted step 5 leaves
    /// it: a pin and a copy that matches the pin.
    fn install(tree: &mut Tree, name: &str) {
        tree.pinned.insert(name.to_string(), installed_sha());
        tree.copies
            .insert(name.to_string(), CopyState::Json(installed_sha()));
    }

    /// Step 0: a 1.x topic file `knowledge/overrides.json` stops the
    /// conversion with one hand step, and the plan changes nothing.
    #[test]
    fn a_topic_named_overrides_stops_the_conversion_with_the_hand_step() {
        let mut tree = tree();
        tree.overrides_file = OverridesFile::Topic;
        tree.topic_files = vec![file(
            "knowledge/alpha.json",
            vec![entry("alpha.one", "Kit")],
        )];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(plan.stop, Some(hand_steps::reserved_topic()));
        assert!(plan.homes.is_empty());
        assert!(plan.topic_removals.is_empty());
        assert!(plan.install_packs.is_empty());
    }

    /// An overrides document that an earlier run wrote is no topic.
    #[test]
    fn an_overrides_document_of_an_earlier_run_does_not_stop_the_conversion() {
        let mut tree = tree();
        tree.overrides_file = OverridesFile::Document(json!({
            "schema": "houserules/overrides@1.0.0", "overrides": [], "exclusions": [],
        }));

        assert_eq!(plan(&tree, &packs()).expect("a plan").stop, None);
    }

    /// Step 1: a listed id that is present becomes an override with its
    /// differing fields, or nothing when it equals the entry of the pack. A
    /// listed id that is absent becomes an exclusion. Each path goes to
    /// `ownedPaths`, after the paths already there.
    #[test]
    fn step_1_splits_the_overrides_into_overrides_exclusions_and_owned_paths() {
        let mut tree = tree();
        tree.overrides = Some(vec![
            "alpha.one".to_string(),
            "alpha.two".to_string(),
            "beta.one".to_string(),
            "docs/README.md".to_string(),
            "AGENTS.md".to_string(),
        ]);
        tree.owned_paths = vec!["AGENTS.md".to_string()];
        tree.topic_files = vec![file(
            "knowledge/alpha.json",
            vec![entry("alpha.one", "Mine"), entry("alpha.two", "Kit")],
        )];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(
            home(&plan, "alpha.one"),
            Home::Override(json!({"summary": "Mine"}).as_object().unwrap().clone())
        );
        assert_eq!(home(&plan, "alpha.two"), Home::Pack);
        assert_eq!(home(&plan, "beta.one"), Home::Exclusion);
        assert_eq!(plan.owned_paths, ["AGENTS.md", "docs/README.md"]);
        assert_eq!(
            plan.overrides_doc,
            Some(json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [{"id": "alpha.one", "fields": {"summary": "Mine"}}],
                "exclusions": [{"id": "beta.one"}],
            }))
        );
    }

    /// A listed id that no kit pack ships has no effect in 2.0: the plan
    /// names it and writes nothing for it.
    #[test]
    fn step_1_ignores_a_listed_id_that_no_kit_pack_ships() {
        let mut tree = tree();
        tree.overrides = Some(vec!["team.gone".to_string()]);

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(plan.ignored_ids, ["team.gone"]);
        assert!(!plan.homes.contains_key("team.gone"));
        assert!(plan.overrides_doc.is_none());
    }

    /// Step 2: an entry that the project never changed is a pack entry,
    /// even when the text of the kit moved since its release; the move
    /// takes the entry out of the topic file, and a file that it empties
    /// goes (L9).
    #[test]
    fn step_2_makes_an_unchanged_entry_a_pack_entry_even_when_the_kit_text_moved() {
        let released = entry("alpha.one", "Released text");
        let mut tree = tree();
        tree.baselines
            .insert("alpha.one".to_string(), baseline_of(&released));
        tree.topic_files = vec![file("knowledge/alpha.json", vec![released])];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(home(&plan, "alpha.one"), Home::Pack);
        assert!(plan.overrides_doc.is_none());
        assert_eq!(plan.topic_removals, ["knowledge/alpha.json"]);
        assert!(plan.topic_writes.is_empty());
    }

    /// Step 2: a changed entry becomes an override with each field that
    /// differs from the entry of the pack.
    #[test]
    fn step_2_makes_a_changed_entry_an_override() {
        let released = entry("alpha.one", "Released text");
        let mut mine = released.clone();
        mine["summary"] = json!("Mine");
        let mut tree = tree();
        tree.baselines
            .insert("alpha.one".to_string(), baseline_of(&released));
        tree.topic_files = vec![file("knowledge/alpha.json", vec![mine])];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(
            home(&plan, "alpha.one"),
            Home::Override(json!({"summary": "Mine"}).as_object().unwrap().clone())
        );
    }

    /// Step 2: an entry with no baseline is compared with the entry of the
    /// pack, as L8 does: the same text is a pack entry, other text an
    /// override. This is the state of a project-only id that a 2.0 pack
    /// now ships.
    #[test]
    fn step_2_compares_an_entry_with_no_baseline_with_the_pack_entry() {
        let mut tree = tree();
        tree.topic_files = vec![file(
            "knowledge/alpha.json",
            vec![entry("alpha.one", "Kit"), entry("alpha.two", "Mine")],
        )];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(home(&plan, "alpha.one"), Home::Pack);
        assert_eq!(
            home(&plan, "alpha.two"),
            Home::Override(json!({"summary": "Mine"}).as_object().unwrap().clone())
        );
    }

    /// Step 2: an entry that is absent with a recorded baseline was deleted
    /// and becomes an exclusion. An entry that is absent with no baseline
    /// was never delivered and becomes a pack entry. A whole deleted topic
    /// converts to exclusions.
    #[test]
    fn step_2_makes_a_deleted_entry_an_exclusion_and_a_never_delivered_one_a_pack_entry() {
        let mut tree = tree();
        tree.baselines.insert(
            "alpha.one".to_string(),
            baseline_of(&entry("alpha.one", "Kit")),
        );

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(home(&plan, "alpha.one"), Home::Exclusion);
        assert_eq!(home(&plan, "alpha.two"), Home::Pack);
        assert_eq!(home(&plan, "beta.one"), Home::Pack);
    }

    /// Step 2 and spec HR-151(3): a topic file that is absent while the
    /// `knowledge/` directory exists is a deleted topic. Each entry of its
    /// pack with a recorded baseline becomes an exclusion, and the entries of
    /// a topic whose file exists are not read as deleted.
    #[test]
    fn a_whole_deleted_topic_converts_to_exclusions() {
        let mut tree = tree();
        for id in ["alpha.one", "alpha.two"] {
            tree.baselines
                .insert(id.to_string(), baseline_of(&entry(id, "Kit")));
        }
        tree.topic_files = vec![file("knowledge/beta.json", vec![entry("beta.one", "Kit")])];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(home(&plan, "alpha.one"), Home::Exclusion);
        assert_eq!(home(&plan, "alpha.two"), Home::Exclusion);
        assert_eq!(home(&plan, "beta.one"), Home::Pack);
    }

    /// Spec C21: `knowledge/areas.json` is absent, `ownedPaths` lists it, and
    /// step 5 installs a pack, which writes the file. The project owns the
    /// file, so the plan stops with the line of an owned absent path, which
    /// names it.
    #[test]
    fn an_owned_absent_areas_file_stops_the_plan_while_a_pack_is_installed() {
        let mut tree = tree();
        tree.areas_file_exists = false;
        tree.owned_paths = vec!["knowledge/areas.json".to_string()];

        let refusal = plan(&tree, &packs()).err();

        assert_eq!(refusal, Some(owned_path_is_absent("knowledge/areas.json")));
    }

    /// The same absent file stops nothing when no pack is installed: step 5
    /// writes no `knowledge/areas.json` then.
    #[test]
    fn an_owned_absent_areas_file_stops_nothing_when_every_pack_is_installed() {
        let mut tree = tree();
        tree.areas_file_exists = false;
        tree.owned_paths = vec!["knowledge/areas.json".to_string()];
        install(&mut tree, "alpha");
        install(&mut tree, "beta");

        let plan = plan(&tree, &packs()).expect("a plan");

        assert!(plan.install_packs.is_empty());
    }

    /// An absent `knowledge/areas.json` that the project does not own stops
    /// nothing: step 5 writes it.
    #[test]
    fn an_absent_areas_file_that_the_project_does_not_own_stops_nothing() {
        let mut tree = tree();
        tree.areas_file_exists = false;
        tree.owned_paths = vec!["knowledge/schema.json".to_string()];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(plan.install_packs, ["alpha", "beta"]);
    }

    /// Step 2 leaves an id that no kit pack ships in its file, and a file
    /// that keeps an entry stays with the entries that did not move. The
    /// topic keys of the file stay.
    #[test]
    fn step_2_keeps_a_project_only_entry_in_its_file() {
        let mut tree = tree();
        tree.topic_files = vec![file(
            "knowledge/alpha.json",
            vec![entry("alpha.one", "Kit"), entry("alpha.mine", "Mine")],
        )];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert!(plan.topic_removals.is_empty());
        assert_eq!(plan.topic_writes.len(), 1);
        let (path, content) = &plan.topic_writes[0];
        assert_eq!(path, "knowledge/alpha.json");
        assert_eq!(
            content,
            &json!({"topic": "t", "title": "T", "entries": [entry("alpha.mine", "Mine")]})
        );
    }

    /// L9: a file with `"entries": []` stays, and so does a file that holds
    /// an entry without an id.
    #[test]
    fn an_empty_file_and_a_file_with_an_entry_without_an_id_stay() {
        let mut tree = tree();
        tree.topic_files = vec![
            file("knowledge/empty.json", Vec::new()),
            file(
                "knowledge/odd.json",
                vec![json!({"summary": "No id"}), entry("alpha.one", "Kit")],
            ),
        ];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert!(plan.topic_removals.is_empty());
        assert_eq!(plan.topic_writes.len(), 1);
        assert_eq!(plan.topic_writes[0].0, "knowledge/odd.json");
    }

    /// Step 3: an archived entry of a kit pack becomes an override with
    /// `status: retired`, leaves the archive, and the id gets no exclusion.
    /// An unchanged entry carries the status only; a changed one carries its
    /// other differing fields too.
    #[test]
    fn step_3_retires_an_archived_entry_and_carries_its_changes() {
        let released = entry("alpha.one", "Released text");
        let mut changed = entry("alpha.two", "Mine");
        changed["status"] = json!("superseded");
        let mut tree = tree();
        tree.baselines
            .insert("alpha.one".to_string(), baseline_of(&released));
        tree.archive_files = vec![file(
            "knowledge/archive/alpha.json",
            vec![released, changed],
        )];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(
            home(&plan, "alpha.one"),
            Home::Retired(json!({"status": "retired"}).as_object().unwrap().clone())
        );
        assert_eq!(
            home(&plan, "alpha.two"),
            Home::Retired(
                json!({"status": "retired", "summary": "Mine"})
                    .as_object()
                    .unwrap()
                    .clone()
            )
        );
        assert_eq!(plan.archive_removals, ["knowledge/archive/alpha.json"]);
        assert_eq!(
            plan.overrides_doc.as_ref().unwrap()["exclusions"],
            json!([])
        );
    }

    /// Step 4: a topic file in `ownedPaths` keeps its entries local, installs
    /// no pack for the topic, and writes no exclusion for it, whether the file
    /// is present or absent. An archived entry of that topic stays in the
    /// archive. Another topic converts as always.
    #[test]
    fn step_4_keeps_an_owned_topic_local_and_installs_no_pack_for_it() {
        let mut tree = tree();
        tree.overrides = Some(vec![
            "knowledge/alpha.json".to_string(),
            "alpha.one".to_string(),
        ]);
        tree.baselines.insert(
            "alpha.two".to_string(),
            baseline_of(&entry("alpha.two", "Kit")),
        );
        tree.topic_files = vec![file(
            "knowledge/alpha.json",
            vec![entry("alpha.one", "Mine")],
        )];
        tree.archive_files = vec![file(
            "knowledge/archive/alpha.json",
            vec![entry("alpha.two", "Old")],
        )];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(
            plan.covered,
            [Covered {
                topic: "alpha".to_string(),
                pack: "alpha".to_string()
            }]
        );
        assert!(!plan.homes.contains_key("alpha.one"));
        assert!(!plan.homes.contains_key("alpha.two"));
        assert!(plan.topic_writes.is_empty() && plan.topic_removals.is_empty());
        assert!(plan.archive_writes.is_empty() && plan.archive_removals.is_empty());
        assert_eq!(plan.install_packs, ["beta"]);
        assert!(plan.overrides_doc.is_none());
    }

    /// Step 5 (spec C20): a kit pack that step 4 does not cover is installed
    /// when it has no pin, or when its pin has no copy or a copy that does not
    /// match the `sha256` of the pin. A pack with a pin and a copy that
    /// matches it is not.
    #[test]
    fn step_5_installs_each_kit_pack_with_no_pin_or_with_a_pin_whose_copy_is_absent_or_wrong() {
        let mut tree = tree();
        install(&mut tree, "alpha");

        assert_eq!(
            plan(&tree, &packs()).expect("a plan").install_packs,
            ["beta"]
        );

        tree.copies.remove("alpha");
        assert_eq!(
            plan(&tree, &packs()).expect("a plan").install_packs,
            ["alpha", "beta"]
        );

        tree.copies
            .insert("alpha".to_string(), CopyState::Json("other".to_string()));
        assert_eq!(
            plan(&tree, &packs()).expect("a plan").install_packs,
            ["alpha", "beta"]
        );

        tree.copies.insert("alpha".to_string(), CopyState::Foreign);
        assert_eq!(
            plan(&tree, &packs()).expect("a plan").install_packs,
            ["alpha", "beta"]
        );
    }

    /// Step 5 (spec C20): a kit pack with no pin whose copy path holds the
    /// copy of the binary is installed, as a pack with no copy is.
    #[test]
    fn step_5_installs_an_unpinned_kit_pack_whose_copy_is_the_copy_of_the_binary() {
        let mut tree = tree();
        tree.copies
            .insert("beta".to_string(), CopyState::Json(installed_sha()));

        assert_eq!(
            plan(&tree, &packs()).expect("a plan").install_packs,
            ["alpha", "beta"]
        );
    }

    /// Step 5 (spec C20): a kit pack with no pin whose copy path holds
    /// another file, or a directory, stops the conversion with one line that
    /// names the path. A pin changes the state: the copy of a pinned pack is
    /// the conversion's to write again.
    #[test]
    fn step_5_refuses_an_unpinned_kit_pack_whose_copy_path_holds_something_else() {
        for copy in [CopyState::Foreign, CopyState::Json("other".to_string())] {
            let mut tree = tree();
            tree.copies.insert("beta".to_string(), copy);

            assert_eq!(
                plan(&tree, &packs()).err(),
                Some(format!(
                    "knowledge/packs/beta.json: this path holds something other than the copy of the kit pack beta that houserules {} writes, and .houserules.json has no pin for the pack; move it away, then run houserules update again",
                    env!("CARGO_PKG_VERSION")
                ))
            );
        }
    }

    /// Step 6: every baseline that is an id goes, the kit ids and the other
    /// ids alike. A baseline of a path stays.
    #[test]
    fn step_6_removes_every_id_baseline_and_keeps_the_path_baselines() {
        let mut tree = tree();
        for key in [
            "alpha.one",
            "team.mine",
            ".githooks/commit-msg",
            ":/.github/workflows/knowledge.yml",
        ] {
            tree.baselines.insert(key.to_string(), json!("hash"));
        }

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(plan.id_baselines, ["alpha.one", "team.mine"]);
    }

    /// Step 7: `standing-rules.md` goes when it exists, and `CLAUDE.md`
    /// without the import gets the hand step. An absent `CLAUDE.md` gets
    /// none.
    #[test]
    fn step_7_removes_the_standing_rules_file_and_checks_the_claude_import() {
        let mut tree = tree();
        tree.standing_rules = true;
        tree.claude_md = Some("My own notes.\n".to_string());

        let plan = plan(&tree, &packs()).expect("a plan");

        assert!(plan.remove_standing_rules);
        assert!(plan.claude_md_hand_step);

        tree.claude_md = Some("@AGENTS.md\n".to_string());
        assert!(!super::plan(&tree, &packs()).unwrap().claude_md_hand_step);
        tree.claude_md = None;
        assert!(!super::plan(&tree, &packs()).unwrap().claude_md_hand_step);
        tree.standing_rules = false;
        assert!(!super::plan(&tree, &packs()).unwrap().remove_standing_rules);
    }

    /// A rerun after an interruption: the pack of the ids is installed
    /// already. The plan decides none of its ids, so an entry that an earlier
    /// run moved out is no deleted entry, and an entry that is left in a
    /// topic file still moves out.
    #[test]
    fn the_ids_of_an_installed_pack_get_no_home_and_their_entries_still_move_out() {
        let released = entry("alpha.one", "Released text");
        let mut tree = tree();
        install(&mut tree, "alpha");
        tree.baselines
            .insert("alpha.one".to_string(), baseline_of(&released));
        tree.baselines.insert(
            "alpha.two".to_string(),
            baseline_of(&entry("alpha.two", "Kit")),
        );
        tree.topic_files = vec![file("knowledge/alpha.json", vec![released])];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert!(!plan.homes.keys().any(|id| id.starts_with("alpha.")));
        assert!(plan.overrides_doc.is_none());
        assert_eq!(plan.topic_removals, ["knowledge/alpha.json"]);
    }

    /// A rerun after an interruption: the document of the earlier run holds
    /// the override of an id whose entry is still in its topic file. The
    /// document keeps the override, and the move only takes the entry out.
    #[test]
    fn a_rerun_keeps_the_override_that_an_earlier_run_wrote() {
        let document = json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": "alpha.one", "fields": {"summary": "Mine"}}],
            "exclusions": [{"id": "beta.one"}],
        });
        let mut tree = tree();
        tree.overrides_file = OverridesFile::Document(document);
        tree.topic_files = vec![file(
            "knowledge/alpha.json",
            vec![entry("alpha.one", "Mine")],
        )];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(home(&plan, "alpha.one"), Home::Handled);
        assert_eq!(home(&plan, "beta.one"), Home::Handled);
        assert!(plan.overrides_doc.is_none());
        assert_eq!(plan.topic_removals, ["knowledge/alpha.json"]);
    }

    /// An id in two local topic files cannot be mapped without losing an
    /// entry: the plan is one line that names both files.
    #[test]
    fn an_id_in_two_topic_files_is_refused_with_both_files_named() {
        let mut tree = tree();
        tree.topic_files = vec![
            file("knowledge/a.json", vec![entry("alpha.one", "Kit")]),
            file("knowledge/b.json", vec![entry("alpha.one", "Kit")]),
        ];

        let Err(refusal) = plan(&tree, &packs()) else {
            panic!("expected a refusal");
        };

        assert_eq!(
            refusal,
            "alpha.one is in knowledge/a.json and knowledge/b.json; keep one entry, then run houserules update again"
        );
    }

    /// An id that is active and archived at once is refused with both files
    /// named.
    #[test]
    fn an_id_active_and_archived_at_once_is_refused_with_both_files_named() {
        let mut tree = tree();
        tree.topic_files = vec![file("knowledge/a.json", vec![entry("alpha.one", "Kit")])];
        tree.archive_files = vec![file(
            "knowledge/archive/a.json",
            vec![entry("alpha.one", "Kit")],
        )];

        let Err(refusal) = plan(&tree, &packs()) else {
            panic!("expected a refusal");
        };

        assert_eq!(
            refusal,
            "alpha.one is active in knowledge/a.json and archived in knowledge/archive/a.json; remove one of the two, then run houserules update again"
        );
    }

    /// The plan reads no text to choose a pack: the pack that a topic file's
    /// name suggests does not decide where an entry goes, its id does.
    #[test]
    fn an_entry_goes_to_the_pack_that_ships_its_id_whatever_file_holds_it() {
        let mut tree = tree();
        tree.topic_files = vec![file("knowledge/misc.json", vec![entry("beta.one", "Kit")])];

        let plan = plan(&tree, &packs()).expect("a plan");

        assert_eq!(home(&plan, "beta.one"), Home::Pack);
        assert_eq!(plan.topic_removals, ["knowledge/misc.json"]);
    }

    /// The released 1.4.0 text of `process.tdd`, and a pack of the binary that
    /// ships the id with another text, so that every field of the 1.4.0 text
    /// differs from the pack's entry.
    fn released_tdd() -> (Value, Vec<PackCopy>) {
        let texts = released_entries().remove("process.tdd").expect("released");
        let text = texts
            .iter()
            .find(|text| text.versions.iter().any(|version| version == "1.4.0"))
            .expect("the 1.4.0 text");
        (text.entry.clone(), vec![pack("process", &["process.tdd"])])
    }

    /// An entry that the project changed becomes an override of the fields
    /// that it changed from the text of its release, not of every field that
    /// differs from the pack. The text of the release is the one whose hash
    /// the marker recorded.
    #[test]
    fn an_override_carries_only_the_fields_the_project_changed_from_its_release() {
        let (released, packs) = released_tdd();
        let mut mine = released.clone();
        mine["summary"] = json!("Mine");
        let mut tree = tree();
        tree.topic_files = vec![file("knowledge/process.json", vec![mine])];
        tree.baselines
            .insert("process.tdd".to_string(), baseline_of(&released));

        let plan = plan(&tree, &packs).expect("a plan");

        assert_eq!(
            home(&plan, "process.tdd"),
            Home::Override(json!({"summary": "Mine"}).as_object().unwrap().clone())
        );
    }

    /// With no baseline for the id, the text of the release that the marker
    /// names is the base of what the project changed.
    #[test]
    fn the_release_that_the_marker_names_is_the_base_when_no_baseline_exists() {
        let (released, packs) = released_tdd();
        let mut mine = released;
        mine["summary"] = json!("Mine");
        let mut tree = tree();
        tree.version = Some("1.4.0".to_string());
        tree.topic_files = vec![file("knowledge/process.json", vec![mine])];

        let plan = plan(&tree, &packs).expect("a plan");

        assert_eq!(
            home(&plan, "process.tdd"),
            Home::Override(json!({"summary": "Mine"}).as_object().unwrap().clone())
        );
    }

    /// An entry that equals a text that a release shipped is the kit's, whatever
    /// the baselines say: it becomes the entry of the pack, not an override.
    #[test]
    fn an_entry_that_equals_a_released_text_is_the_kits_without_a_baseline() {
        let (released, packs) = released_tdd();
        let mut tree = tree();
        tree.topic_files = vec![file("knowledge/process.json", vec![released])];

        let plan = plan(&tree, &packs).expect("a plan");

        assert_eq!(home(&plan, "process.tdd"), Home::Pack);
    }

    /// A listed id follows the same rule: the project owns the fields that it
    /// changed, and the pack supplies the rest.
    #[test]
    fn a_listed_id_carries_only_the_fields_the_project_changed_from_its_release() {
        let (released, packs) = released_tdd();
        let mut mine = released.clone();
        mine["tags"] = json!(["mine"]);
        let mut tree = tree();
        tree.overrides = Some(vec!["process.tdd".to_string()]);
        tree.version = Some("1.4.0".to_string());
        tree.topic_files = vec![file("knowledge/process.json", vec![mine])];

        let plan = plan(&tree, &packs).expect("a plan");

        assert_eq!(
            home(&plan, "process.tdd"),
            Home::Override(json!({"tags": ["mine"]}).as_object().unwrap().clone())
        );
    }
}
