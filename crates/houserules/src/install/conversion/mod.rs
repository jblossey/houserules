//! The conversion of a 1.x install to the 2.0 layout (spec section 5).
//!
//! `update` runs [`convert`] on a tree whose `.houserules.json` has no
//! `schema` key. The conversion has two halves. [`plan`] is a pure function
//! from the tree that [`read_tree`] reads and the packs of the binary to a
//! [`Plan`]: it writes nothing, and it refuses a tree it cannot map without
//! losing an entry before any write. `d3` decides step 8 for the two D3 files.
//! The writer in [`convert`] then applies the plan step by step, in the order
//! of the spec, and prints a line for what it did.
//!
//! The steps run in this order: the overrides document (the result of steps
//! 1 to 3), the marker (step 1), the packs (step 5), the move of the entries
//! out of the topic and archive files (steps 2 and 3), the baselines, the
//! Claude files, and the D3 files (steps 6 to 8), and the `schema` key of the
//! marker (step 9) last. Every step tests its own state, so a run that
//! stopped after any write finishes at the next run:
//!
//! - the overrides document holds the decision for every id of a pack that is
//!   not installed yet, and it is written before the marker loses its
//!   `overrides` key and before a pack or an entry changes;
//! - an entry leaves its file only after the pack that ships its id is
//!   installed, and the plan decides only the ids of packs that are not
//!   installed yet. So an entry that an earlier run moved is never read as an
//!   entry that the project deleted, although the baselines stay until step 6;
//! - a pack is pinned before its areas and its copy are written, and the pin
//!   records the areas that the run is about to add. Step 5's state covers a
//!   pack with no pin and a pack whose pin has no copy or a wrong one (spec
//!   C20), so a run that stopped between the three writes finishes the pack,
//!   and an area that the stopped run added stays the pack's and never
//!   becomes the project's own;
//! - the entries that a pack lists under `optIn` are offered before the copy
//!   that ships them (spec 3.7): the exclusion is written first, then the
//!   marker holds the pin and the record in `optInOffered` in one write. A
//!   stop before the marker write leaves the exclusion, and the next run
//!   records the id; a stop after it leaves the record, and the next run
//!   offers nothing again;
//! - the `schema` key makes the tree a 2.0 install, so no step runs again.
//!
//! The conversion steps of batch 34 follow (`steps`): step 1, the JSON tracker
//! of a `backlog/` directory, runs here before the `schema` key; the others run
//! on every `update`.
//!
//! A tree whose `knowledge/` directory does not exist or is empty is not
//! converted (spec C21). Under step 2 every entry of it would be absent with a
//! recorded baseline, so the conversion would exclude all of them, which an
//! accidental deletion should not state. The refusal comes from [`read_tree`],
//! the first read, so the conversion starts only on a tree whose directory
//! holds an entry, and an interrupted run never leaves one without. A topic
//! file that is absent while `knowledge/` holds files is a deleted topic: its
//! entries become exclusions. An absent `knowledge/areas.json` in such a tree
//! is written by step 5, before the first pack, with no area; each pack then
//! adds the areas it declares and records them in its pin. When `ownedPaths`
//! lists that file, the conversion stops before any write.

mod d3;
mod plan;
pub(super) mod steps;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::archive::{json_file_names, list_archive_json_files, read_json, write_json};
use crate::baseline;
use crate::pack::areas::{add_pack_areas, areas_to_add};
use crate::pack::opt_in;
use crate::packs::{PackCopy, pack_sha256};
use crate::rules::RESERVED;

use crate::tracker::{Adapter, TrackerConfig};

use super::ci_workflow::Workflow;
use super::hand_steps;
use super::marker::{INSTALL_SCHEMA, Marker, Pin};
use super::{
    AREAS_FILE, SCHEMA_FILE, join_components, payload_content, read_if_present, write_if_changed,
    write_into,
};
use d3::{Action, Decision};
use plan::{CopyState, OverridesFile, Plan, TopicFile, Tree};

pub(crate) use d3::known_contents;

/// The file that holds the standing rules of 1.x for Claude Code. The Claude
/// renderer of 2.0 no longer writes it.
const STANDING_RULES: &str = ".claude/rules/standing-rules.md";

/// The prefix of `schema` ids of an overrides document.
const OVERRIDES_SCHEMA_PREFIX: &str = "houserules/overrides@";

/// What the conversion needs to know about the install.
pub(super) struct Context<'a> {
    /// The install root.
    pub(super) root: &'a Path,
    /// The directory of the git top level.
    pub(super) top_level: &'a Path,
    /// The id prefix of the install.
    pub(super) prefix: &'a str,
    /// The seeded CI workflow of the install, when the install holds one
    /// (`Workflow::held_by_install`).
    pub(super) workflow: Option<&'a Workflow>,
}

/// The `knowledge/` directory of the install, built one component at a time.
fn knowledge_dir(root: &Path) -> std::path::PathBuf {
    root.join("knowledge")
}

/// `true` when `path` exists. A path whose existence cannot be told is one
/// line with the operating system's own text.
fn exists(path: &Path) -> Result<bool, String> {
    path.try_exists()
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// `true` when the directory `dir` holds at least one entry, of any kind. A
/// directory that does not exist holds none. Any other reason `dir` cannot be
/// read is one line with the operating system's own text.
fn holds_an_entry(dir: &Path) -> Result<bool, String> {
    let named = |error: std::io::Error| format!("{}: {error}", dir.display());
    match fs::read_dir(dir) {
        Ok(mut entries) => Ok(entries.next().transpose().map_err(named)?.is_some()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(named(error)),
    }
}

/// What the path `path` of a pack copy holds. A directory, and a file that is
/// no JSON, are [`CopyState::Foreign`]; a file that cannot be read is one line
/// with the operating system's own text.
fn read_copy(path: &Path) -> Result<CopyState, String> {
    let named = |error: std::io::Error| format!("{}: {error}", path.display());
    if !fs::metadata(path).map_err(named)?.is_file() {
        return Ok(CopyState::Foreign);
    }
    let bytes = fs::read(path).map_err(named)?;
    Ok(match serde_json::from_slice::<Value>(&bytes) {
        Ok(raw) => CopyState::Json(pack_sha256(&raw)),
        Err(_) => CopyState::Foreign,
    })
}

/// The line that stops the conversion of a tree whose `knowledge/` directory
/// does not exist or is empty (spec C21). It names the directory and both ways
/// out. The way out through `init` keeps the id prefix of the install: `prefix`
/// is the `idPrefix` of the marker.
fn absent_knowledge_line(prefix: &str) -> String {
    format!(
        "knowledge/: this directory does not exist or is empty, so update cannot tell which entries the project deleted and cannot convert this install of houserules 1.x; restore it from version control and run houserules update again, or remove .houserules.json and run houserules init --id-prefix {prefix} for a fresh install"
    )
}

/// Reads the files of the 1.x tree that the plan needs, and nothing is
/// written. A tree whose `knowledge/` directory does not exist or is empty is
/// the line of [`absent_knowledge_line`] for `prefix`. An unreadable file or
/// directory, or a topic or archive file that is no JSON, is one line that
/// names it.
fn read_tree(root: &Path, marker: &Marker, prefix: &str) -> Result<Tree, String> {
    let knowledge = knowledge_dir(root);
    if !holds_an_entry(&knowledge)? {
        return Err(absent_knowledge_line(prefix));
    }
    let mut topic_files = Vec::new();
    for name in json_file_names(&knowledge)? {
        if RESERVED.contains(&name.as_str()) {
            continue;
        }
        topic_files.push(TopicFile {
            content: read_json(&knowledge.join(&name))?,
            path: format!("knowledge/{name}"),
        });
    }
    let archive = knowledge.join("archive");
    let mut archive_files = Vec::new();
    for name in list_archive_json_files(&archive)?.unwrap_or_default() {
        archive_files.push(TopicFile {
            content: read_json(&archive.join(&name))?,
            path: format!("knowledge/archive/{name}"),
        });
    }
    let overrides_file = match read_if_present(&knowledge.join("overrides.json"))? {
        None => OverridesFile::Absent,
        Some(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(document)
                if document
                    .get("schema")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.starts_with(OVERRIDES_SCHEMA_PREFIX)) =>
            {
                OverridesFile::Document(document)
            }
            _ => OverridesFile::Topic,
        },
    };
    let packs = knowledge.join("packs");
    let mut copies = BTreeMap::new();
    if packs.is_dir() {
        for name in json_file_names(&packs)? {
            if let Some(pack) = name.strip_suffix(".json") {
                copies.insert(pack.to_string(), read_copy(&packs.join(&name))?);
            }
        }
    }
    let claude_md = read_if_present(&root.join("CLAUDE.md"))?
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());
    Ok(Tree {
        overrides: marker.overrides(),
        owned_paths: marker.owned_paths(),
        baselines: marker.baselines(),
        version: marker.version().map(str::to_string),
        pinned: marker
            .pins()?
            .into_iter()
            .map(|(name, pin)| (name, pin.sha256))
            .collect(),
        copies,
        areas_file_exists: exists(&knowledge.join("areas.json"))?,
        topic_files,
        archive_files,
        overrides_file,
        claude_md,
        standing_rules: join_components(root, STANDING_RULES)
            .try_exists()
            .map_err(|error| format!("{}: {error}", root.join(STANDING_RULES).display()))?,
    })
}

/// The counts of the items in an overrides document, for its line.
fn document_counts(document: &Value) -> (usize, usize) {
    let count = |key: &str| {
        document
            .get(key)
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    };
    (count("overrides"), count("exclusions"))
}

/// Converts the 1.x install at `ctx.root` to the 2.0 layout and returns the
/// hand steps that the project has left. `marker` is the marker of the
/// install, read before; the conversion changes it and writes it. `kit` holds
/// the packs of the binary. Every file the conversion reads is read before it
/// writes the first one. A refusal is one line, and step 0 stops the
/// conversion with the hand step of `hand_steps::reserved_topic`.
pub(super) fn convert(
    ctx: &Context<'_>,
    marker: &mut Marker,
    kit: &[PackCopy],
) -> Result<Vec<String>, String> {
    let tree = read_tree(ctx.root, marker, ctx.prefix)?;
    let plan = plan::plan(&tree, kit)?;
    if let Some(stop) = &plan.stop {
        return Err(format!("{}{stop}", hand_steps::PREFIX));
    }
    if !plan.install_packs.is_empty() {
        areas_file_is_an_object(ctx.root)?;
    }
    let schema_kit = payload_content(SCHEMA_FILE, ctx.prefix)?;
    let schema_disk = read_if_present(&join_components(ctx.root, SCHEMA_FILE))?;
    let owned = |path: &str| plan.owned_paths.iter().any(|owned| owned == path);
    let schema = d3::decide_schema(
        schema_disk.as_deref(),
        owned(SCHEMA_FILE),
        &schema_kit,
        ctx.prefix,
    )?;
    let workflow = ctx
        .workflow
        .map(|workflow| {
            let disk = read_if_present(&join_components(ctx.top_level, &workflow.path))?;
            d3::decide_workflow(
                disk.as_deref(),
                owned(&workflow.path),
                &workflow.content,
                &workflow.path,
                ctx.prefix,
            )
            .map(|decision| (workflow, decision))
        })
        .transpose()?;

    let mut hand_steps = Vec::new();
    write_overrides_document(ctx, &plan)?;
    step_1_marker(marker, &tree, &plan)?;
    for covered in &plan.covered {
        println!(
            "knowledge/{0}.json is in ownedPaths, so the conversion leaves it as it is and does not install the kit pack {1}; run houserules pack add {1} to install it",
            covered.topic, covered.pack
        );
    }
    install_packs(ctx, marker, &plan, kit)?;
    move_entries(ctx, &plan)?;

    let mut baselines = marker.baselines();
    for key in &plan.id_baselines {
        baselines.shift_remove(key);
    }
    if !plan.id_baselines.is_empty() {
        println!(
            "removed {} entry baselines from .houserules.json; the pins of the packs replace them",
            plan.id_baselines.len()
        );
    }
    if plan.remove_standing_rules {
        let path = join_components(ctx.root, STANDING_RULES);
        fs::remove_file(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        println!("removed {STANDING_RULES}");
    }
    marker.set_renderers(&["claude"]);
    adopt_the_json_tracker(ctx.root, marker)?;
    if plan.claude_md_hand_step {
        hand_steps.push(hand_steps::claude_md_import());
    }
    apply_d3(
        ctx.root,
        SCHEMA_FILE,
        SCHEMA_FILE,
        &schema_kit,
        &schema,
        &mut baselines,
        &mut hand_steps,
    )?;
    if let Some((workflow, decision)) = &workflow {
        apply_d3(
            ctx.top_level,
            &workflow.path,
            &workflow.key,
            &workflow.content,
            decision,
            &mut baselines,
            &mut hand_steps,
        )?;
    }
    marker.set_baselines(baselines);
    marker.write()?;

    marker.stamp_schema();
    marker.write()?;
    println!(
        "converted .houserules.json to the install schema {}",
        INSTALL_SCHEMA
    );
    Ok(hand_steps)
}

/// Step 1 of the conversion steps of batch 34 (spec 4.7): a 1.x install with a
/// `backlog/` directory gets the JSON tracker, because the directory is the
/// data of that tracker. An install with no such directory gets none: there is
/// no data to adopt, and `update` prints the tracker hand step instead
/// (`steps`). The step runs in the conversion only, and before its completion
/// marker, the `schema` key, so a run that stopped finishes at the next run. A
/// marker that has a `tracker` key keeps it.
fn adopt_the_json_tracker(root: &Path, marker: &mut Marker) -> Result<(), String> {
    if root.join("backlog").is_dir() && marker.tracker()?.is_none() {
        marker.set_tracker(&TrackerConfig {
            adapter: Adapter::Json,
            repo: None,
        });
    }
    Ok(())
}

/// Refuses a `knowledge/areas.json` that is no JSON object before the
/// conversion writes a file: step 5 adds the areas of the packs to it. An
/// absent file passes: step 5 creates it.
fn areas_file_is_an_object(root: &Path) -> Result<(), String> {
    let path = knowledge_dir(root).join("areas.json");
    if !exists(&path)? {
        return Ok(());
    }
    match read_json(&path)? {
        Value::Object(_) => Ok(()),
        _ => Err(format!("{}: not a JSON object", path.display())),
    }
}

/// Steps 1 to 3, first write: `knowledge/overrides.json`, before the marker
/// loses its `overrides` key and before an entry leaves its file.
fn write_overrides_document(ctx: &Context<'_>, plan: &Plan) -> Result<(), String> {
    let Some(document) = &plan.overrides_doc else {
        return Ok(());
    };
    write_json(&knowledge_dir(ctx.root).join("overrides.json"), document)?;
    let (overrides, exclusions) = document_counts(document);
    println!(
        "wrote knowledge/overrides.json ({}, {})",
        counted(overrides, "override"),
        counted(exclusions, "exclusion")
    );
    Ok(())
}

/// `count` and `noun`, with the noun in the plural unless `count` is 1.
fn counted(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
}

/// Step 1: moves the paths of the `overrides` key to `ownedPaths` and
/// removes the key. It acts only while the marker has the key.
fn step_1_marker(marker: &mut Marker, tree: &Tree, plan: &Plan) -> Result<(), String> {
    if tree.overrides.is_none() {
        return Ok(());
    }
    if !plan.owned_paths.is_empty() {
        marker.set_owned_paths(&plan.owned_paths);
    }
    marker.remove_overrides();
    marker.write()?;
    let moved = plan.owned_paths.len() - tree.owned_paths.len();
    if moved > 0 {
        println!("moved {moved} paths from overrides to ownedPaths in .houserules.json");
    }
    if !plan.ignored_ids.is_empty() {
        println!(
            "dropped {} from overrides in .houserules.json: no kit pack ships them",
            plan.ignored_ids.join(", ")
        );
    }
    Ok(())
}

/// Steps 2 and 3: takes the moved entries out of the topic files and the
/// archive files, and removes each file that the move empties.
fn move_entries(ctx: &Context<'_>, plan: &Plan) -> Result<(), String> {
    for (path, content) in plan.topic_writes.iter().chain(&plan.archive_writes) {
        write_json(&join_components(ctx.root, path), content)?;
        println!("updated {path}: the entries of the kit packs left it");
    }
    for path in plan.topic_removals.iter().chain(&plan.archive_removals) {
        let file = join_components(ctx.root, path);
        fs::remove_file(&file).map_err(|error| format!("{}: {error}", file.display()))?;
        println!("removed {path}: the entries of the kit packs left it empty");
    }
    Ok(())
}

/// Step 5, before the first pack: creates an absent `knowledge/areas.json`
/// with no area, so that each pack adds the areas it declares and records
/// them in its pin, as `init` seeds the file of an existing 2.0 install. The
/// file holds no area of the project, so a run that stops after this write
/// finishes as an uninterrupted run does.
fn seed_absent_areas_file(root: &Path) -> Result<(), String> {
    let path = join_components(root, AREAS_FILE);
    if exists(&path)? {
        return Ok(());
    }
    write_json(&path, &json!({}))?;
    println!("wrote {AREAS_FILE}");
    Ok(())
}

/// Step 5: installs each kit pack that the plan names. The exclusion of each
/// opt-in entry of the pack comes first (`opt_in::offer`, spec 3.7), so that no
/// stop leaves the entry live. Then the pin is written, with the areas that the
/// pack is about to add in its `addedAreas` and the record of the offer in
/// `optInOffered`, in one write of the marker; the offer lines print after that
/// write. Then the areas are added to `knowledge/areas.json`, and the copy is
/// written. A run that stops after any write leaves a pin that the plan reads
/// as step 5's state again (a pin whose copy is absent), and the next run adds
/// no area that the record already names and offers no entry that the marker
/// records. A run that stops before the marker write finds the exclusion in
/// place, records the id, and prints the line. A pin that an earlier run wrote
/// keeps its record.
fn install_packs(
    ctx: &Context<'_>,
    marker: &mut Marker,
    plan: &Plan,
    kit: &[PackCopy],
) -> Result<(), String> {
    if !plan.install_packs.is_empty() {
        seed_absent_areas_file(ctx.root)?;
    }
    let mut pins = marker.pins()?;
    for name in &plan.install_packs {
        let pack = kit
            .iter()
            .find(|pack| &pack.name == name)
            .expect("the plan names packs of the binary");
        let mut added_areas = pins
            .get(name)
            .map(|pin| pin.added_areas.clone())
            .unwrap_or_default();
        for area in areas_to_add(ctx.root, pack)? {
            if !added_areas.contains(&area) {
                added_areas.push(area);
            }
        }
        pins.insert(
            name.clone(),
            Pin {
                version: pack.version.clone(),
                source: "kit".to_string(),
                sha256: pack_sha256(&pack.raw),
                added_areas: added_areas.clone(),
                commit: None,
            },
        );
        marker.set_pins(&pins);
        let offered = opt_in::offer(ctx.root, marker, pack)?;
        marker.write()?;
        for line in offered {
            println!("{line}");
        }
        let area_lines = add_pack_areas(ctx.root, pack, &mut added_areas)?;
        let relative = format!("knowledge/packs/{name}.json");
        write_json(&join_components(ctx.root, &relative), &pack.raw)?;
        println!("wrote {relative} ({})", pack.version);
        for line in area_lines {
            println!("{line}");
        }
    }
    Ok(())
}

/// Step 8: applies `decision` to the D3 file `path` below `base`, whose
/// baseline key is `key` and whose content of 2.0 is `kit`.
fn apply_d3(
    base: &Path,
    path: &str,
    key: &str,
    kit: &[u8],
    decision: &Decision,
    baselines: &mut Map<String, Value>,
    hand_steps: &mut Vec<String>,
) -> Result<(), String> {
    match &decision.action {
        Action::Absent | Action::Owned | Action::Keep => {}
        Action::Baseline => {
            baselines.insert(key.to_string(), json!(baseline::hash(kit)));
        }
        Action::Replace => {
            if write_if_changed(base, path, kit)? {
                println!("wrote {path}");
            }
            baselines.insert(key.to_string(), json!(baseline::hash(kit)));
        }
        Action::Edit(bytes) => {
            write_into(base, path, bytes)?;
            println!("edited {path}: removed $defs.area and the $ref of its area property");
        }
    }
    if let Some(step) = &decision.hand_step {
        hand_steps.push(step.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use crate::packs::kit_packs;
    use plan::Home;

    /// The overrides that the conversion writes for this repository at the
    /// commit that the conversion fixture holds, each with its sorted field names. The 34
    /// rows below `knowledge-base.cite-durable-refs` hold the fields that the
    /// project changed from the text of its release, 1.4.0. A field that the
    /// kit changed after that release and that the project left alone reaches
    /// the install through the pack. `knowledge-base.cite-durable-refs` is a
    /// project-only id of the 1.x tree that a kit pack of 2.0 ships. No
    /// release shipped it, so step 2 compares it with the pack entry, as L8
    /// does. Its `check` differs too: the pack entry reads more paths than the
    /// project's check.
    const EXPECTED_OVERRIDES: [(&str, &[&str]); 35] = [
        (
            "knowledge-base.cite-durable-refs",
            &["body", "check", "source", "summary", "verify"],
        ),
        ("knowledge-base.ids-are-permanent", &["body"]),
        (
            "knowledge-base.rules-need-a-loading-path",
            &["body", "source", "standing", "tags", "verify"],
        ),
        ("knowledge-base.state-only-the-source", &["body"]),
        ("process.ask-when-missing", &["body", "summary"]),
        ("process.brainstorm-first", &["body", "summary"]),
        ("process.brief-carries-the-spec", &["body", "source"]),
        ("process.claims-match-artifacts", &["body", "source"]),
        (
            "process.closure-claims-carry-enumeration",
            &["body", "source"],
        ),
        (
            "process.contract-refresh-sweep",
            &["body", "kind", "source", "standing"],
        ),
        ("process.eval-fixture-procedure", &["body", "source"]),
        ("process.evals-rerun", &["body", "source"]),
        ("process.evidence-outlives-the-session", &["body", "source"]),
        (
            "process.fix-proofs-extend-the-reviewers-run",
            &["body", "source", "tags"],
        ),
        ("process.fix-round-verification-record", &["body", "source"]),
        (
            "process.gate-shell-chains",
            &["body", "kind", "source", "standing"],
        ),
        ("process.main-wins-backlog-collisions", &["body", "source"]),
        ("process.model-policy", &["body", "summary"]),
        ("process.owner-content-mid-batch", &["body", "source"]),
        (
            "process.owner-rulings-need-owner-supersession",
            &["body", "source", "tags"],
        ),
        (
            "process.review-findings-are-claims-too",
            &["body", "source"],
        ),
        ("process.rulings-to-file", &["body"]),
        ("process.skills-for-procedures", &["standing"]),
        ("process.suggestions-are-unverified", &["body", "source"]),
        ("process.tdd", &["body"]),
        (
            "process.verify-in-persisted-state",
            &["body", "source", "standing", "tags"],
        ),
        (
            "process.wiring-checks-run-the-resolution",
            &["area", "body", "source", "standing", "tags"],
        ),
        (
            "quality.gates-derive-their-scope",
            &["area", "body", "source", "standing", "tags"],
        ),
        ("quality.no-compat-softening", &["source"]),
        (
            "quality.pin-copies-byte-exact",
            &["area", "body", "source", "standing", "tags", "verify"],
        ),
        ("quality.principles", &["body"]),
        ("quality.well-maintained-libraries", &["body", "source"]),
        ("security-hygiene.exact-pins", &["body", "summary"]),
        ("writing-style.doc-comments", &["body"]),
        (
            "writing-style.instructions-cover-the-state-space",
            &["area", "body", "source", "standing", "tags"],
        ),
    ];

    /// The copy of this repository's `.houserules.json` and `knowledge/` at
    /// the commit that the fixture holds, a 1.x install with 34 overridden ids.
    fn repository_fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("conversion")
            .join("repo")
    }

    /// The number of entries in `content`.
    fn entry_count(content: &Value) -> usize {
        content
            .get("entries")
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    }

    /// The entries that stay in the local topic files after the plan: the
    /// files it rewrites hold their new entries, the files it removes hold
    /// none, and every other file keeps its entries.
    fn local_entries_after(tree: &Tree, plan: &Plan) -> usize {
        tree.topic_files
            .iter()
            .map(|file| {
                if plan.topic_removals.contains(&file.path) {
                    0
                } else if let Some((_, content)) = plan
                    .topic_writes
                    .iter()
                    .find(|(path, _)| *path == file.path)
                {
                    entry_count(content)
                } else {
                    entry_count(&file.content)
                }
            })
            .sum()
    }

    /// The corpus run (`plan-corpus-conversion.py`) over this repository at
    /// the commit that the conversion fixture holds: 34 overrides from the overrides of the
    /// marker, 24 pack entries, no exclusion, no retired entry, the topic file
    /// `writing-style.json` gone, and 4 paths in `ownedPaths`. The tree holds
    /// 53 project-only ids. One of them, `knowledge-base.cite-durable-refs`,
    /// is an id that the binary's packs ship: it becomes the 35th override and
    /// empties `knowledge-base.json`. The other 52 stay local.
    #[test]
    fn this_repository_at_the_base_of_batch_33_task_7_converts_as_the_corpus_run_says() {
        let root = repository_fixture();
        let marker = Marker::read(&root).expect("read the fixture marker");
        let tree =
            read_tree(&root, &marker, &marker.id_prefix_or("WI")).expect("read the fixture tree");

        let plan = plan::plan(&tree, &kit_packs()).expect("a plan");

        let document = plan.overrides_doc.clone().expect("an overrides document");
        let mut written: Vec<(String, Vec<String>)> = document["overrides"]
            .as_array()
            .expect("overrides")
            .iter()
            .map(|item| {
                let mut fields: Vec<String> = item["fields"]
                    .as_object()
                    .expect("fields")
                    .keys()
                    .cloned()
                    .collect();
                fields.sort();
                (item["id"].as_str().expect("an id").to_string(), fields)
            })
            .collect();
        written.sort();
        let expected: Vec<(String, Vec<String>)> = EXPECTED_OVERRIDES
            .iter()
            .map(|(id, fields)| {
                (
                    id.to_string(),
                    fields.iter().map(|field| field.to_string()).collect(),
                )
            })
            .collect();
        assert_eq!(written, expected);
        assert_eq!(document["exclusions"], json!([]));

        let before: usize = tree
            .topic_files
            .iter()
            .map(|file| entry_count(&file.content))
            .sum();
        let kept_local = local_entries_after(&tree, &plan);
        let moved = before - kept_local;
        assert_eq!(
            (moved - written.len(), written.len(), kept_local),
            (24, 35, 52),
            "pack entries, overrides, and local ids"
        );
        assert_eq!(
            plan.topic_removals,
            [
                "knowledge/knowledge-base.json",
                "knowledge/writing-style.json"
            ]
        );
        let retired_or_excluded = plan
            .homes
            .values()
            .filter(|home| matches!(home, Home::Retired(_) | Home::Exclusion))
            .count();
        assert_eq!(retired_or_excluded, 0);
        assert_eq!(
            plan.owned_paths,
            [
                "backlog/items/general.json",
                ".github/workflows/knowledge.yml",
                "docs/README.md",
                ".githooks/commit-msg"
            ]
        );
        assert_eq!(plan.id_baselines.len(), 49);
        assert_eq!(plan.install_packs.len(), 5);
        assert!(plan.covered.is_empty() && plan.ignored_ids.is_empty());
    }
}
