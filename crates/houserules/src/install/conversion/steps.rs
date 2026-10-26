//! The conversion steps of batch 34 (spec 4.7), steps 2 to 4. `update` runs
//! them on every run of a 2.0 install and of an install that the 1.x conversion
//! just converted, after the steps of batch 33. Each step tests its own state,
//! acts only while that state holds, and writes no file that the project owns or
//! changed, so a run that stopped after any write finishes at the next run and a
//! second `update` changes nothing (`houserules.migrations-finish-after-a-stop`).
//! Step 1, the JSON tracker of a 1.x install with a `backlog/` directory, runs
//! inside the 1.x conversion (`convert`), before its completion marker. Step 5,
//! the `project-knowledge` skill, is the render that ends `update`.
//!
//! - Step 2 adopts the reference workflow into an install that has files of the
//!   kit at its workflow paths and no `workflow` key: [`workflow::adopt`] pins
//!   each kit file and writes the absent ones, and the hand steps name a file
//!   that is not the kit's, a required id that the project excludes, and the
//!   missing tracker.
//! - Step 3 splits the session-start script: where the script of the claude
//!   renderer holds the ritual line, the workflow's hook entry joins
//!   `.claude/settings.json`, and a script that the project changed gets the
//!   hand step. The sync that follows replaces a script that is the kit's with
//!   the core script, and the adoption wrote the reference script.
//! - The settings cleanup of C40 (spec 4.4) runs after step 3: the startup hook
//!   entry that the kit seeded before is removed from `.claude/settings.json`
//!   where it equals the kit's, because the compaction entry is the only core
//!   session-start hook.
//! - Step 4 delivers the deliverables schema and, with the JSON tracker, the
//!   backlog schema as D3 files: a copy that equals a content that the kit
//!   shipped gets a baseline at its own bytes, so that the sync writes the
//!   current content; any other copy is kept with a hand step.
//!
//! A path in `ownedPaths` is left as found by every step, with no line.

use serde_json::{Value, json};

use crate::baseline;
use crate::emit::emit;
use crate::tracker::Adapter;
use crate::workflow::{self, requirements};

use super::super::marker::Marker;
use super::super::sync::is_known_deliverables;
use super::super::{
    CLAUDE_SESSION_START, DELIVERABLES_FILE, SETTINGS_PATH, TRACKER_SCHEMA, hand_steps,
    is_overridden, join_components, payload_content, read_if_present, read_json_object,
    seed_workflow_hook,
};
use super::{Context, known_contents};

/// The text that marks the ritual line of the 1.x session-start script.
const RITUAL_MARKER: &str = "Session ritual:";

/// Runs steps 2 to 4 for the install at `ctx.root`, whose `marker` the caller
/// has read after the steps of batch 33. The steps print a line for what they
/// write, and change `marker` and write it. Returns the hand steps that the
/// project has left, without their prefix.
pub(in crate::install) fn run(
    ctx: &Context<'_>,
    marker: &mut Marker,
) -> Result<Vec<String>, String> {
    let mut hand_steps = Vec::new();
    adopt_the_workflow(ctx, marker, &mut hand_steps)?;
    split_the_session_start_script(ctx, marker, &mut hand_steps)?;
    remove_the_seeded_startup_hook(ctx, marker)?;
    deliver_the_schemas(ctx, marker, &mut hand_steps)?;
    Ok(hand_steps)
}

/// Step 2: adopts the reference workflow while the install has no `workflow`
/// key and at least one workflow path that it does not own holds a file of the
/// kit. `marker` is written once the pins are in it.
fn adopt_the_workflow(
    ctx: &Context<'_>,
    marker: &mut Marker,
    hand_steps: &mut Vec<String>,
) -> Result<(), String> {
    if workflow::pins::read(marker)?.is_some() {
        return Ok(());
    }
    let manifest = workflow::reference();
    let owned = marker.owned_paths();
    let mut holds_a_kit_file = false;
    for path in &manifest.files {
        if !is_overridden(&owned, path) && workflow::is_kit_file(ctx.root, &manifest, path, marker)?
        {
            holds_a_kit_file = true;
            break;
        }
    }
    if !holds_a_kit_file {
        return Ok(());
    }
    let outcome = workflow::adopt(ctx.root, marker, &manifest)?;
    marker.write()?;
    for line in outcome
        .lines
        .iter()
        .filter(|line| !line.starts_with(hand_steps::PREFIX))
    {
        println!("{line}");
    }
    hand_steps.extend(outcome.hand_steps);
    let base = crate::rules::load_base(ctx.root).map_err(|error| error.to_string())?;
    for id in requirements::excluded_ids(&base, &manifest) {
        hand_steps.push(hand_steps::workflow_excludes_a_required_id(
            &manifest.name,
            id,
        ));
    }
    if marker.tracker()?.is_none() {
        hand_steps.push(hand_steps::workflow_needs_a_tracker());
    }
    Ok(())
}

/// Step 3: while the session-start script of the claude renderer holds the
/// ritual line of 1.x, adds the hook entry of the reference script where the
/// install pins the workflow and the renderer is on, and names a script that
/// the project changed in a hand step. The script and `.claude/settings.json`
/// stay as found when `ownedPaths` lists either.
fn split_the_session_start_script(
    ctx: &Context<'_>,
    marker: &mut Marker,
    hand_steps: &mut Vec<String>,
) -> Result<(), String> {
    let owned = marker.owned_paths();
    if is_overridden(&owned, CLAUDE_SESSION_START) || is_overridden(&owned, SETTINGS_PATH) {
        return Ok(());
    }
    let Some(current) = read_if_present(&join_components(ctx.root, CLAUDE_SESSION_START))? else {
        return Ok(());
    };
    if !String::from_utf8_lossy(&current).contains(RITUAL_MARKER) {
        return Ok(());
    }
    if marker.claude_enabled() {
        seed_workflow_hook(ctx.root, marker)?;
    }
    let at_baseline = marker
        .baselines()
        .get(CLAUDE_SESSION_START)
        .and_then(Value::as_str)
        .is_some_and(|recorded| recorded == baseline::hash(&current));
    let is_known = known_contents(CLAUDE_SESSION_START, ctx.prefix)?.contains(&current);
    if !at_baseline && !is_known {
        hand_steps.push(hand_steps::ritual_line_stays_in_the_session_script());
    }
    Ok(())
}

/// The `SessionStart` entry that the kit seeded before C40, whose script mode
/// printed the ritual and now prints nothing.
fn retired_startup_entry() -> Value {
    json!({
        "matcher": "startup|resume|clear|fork",
        "hooks": [{
            "type": "command",
            "command": "${CLAUDE_PROJECT_DIR}/tools/claude-session-start.sh start",
        }]
    })
}

/// The settings cleanup of C40: removes the entry of [`retired_startup_entry`]
/// from `.claude/settings.json` where the file holds it exactly, and prints one
/// line. An entry that the project changed stays, and so does a settings file
/// that `ownedPaths` lists; neither gets a line.
fn remove_the_seeded_startup_hook(ctx: &Context<'_>, marker: &Marker) -> Result<(), String> {
    if is_overridden(&marker.owned_paths(), SETTINGS_PATH) {
        return Ok(());
    }
    let path = join_components(ctx.root, SETTINGS_PATH);
    if read_if_present(&path)?.is_none() {
        return Ok(());
    }
    let mut settings = read_json_object(&path)?;
    let Some(entries) = settings
        .get_mut("hooks")
        .and_then(|hooks| hooks.get_mut("SessionStart"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(());
    };
    let seeded = retired_startup_entry();
    let before = entries.len();
    entries.retain(|entry| *entry != seeded);
    if entries.len() == before {
        return Ok(());
    }
    crate::written::write(&path, emit(&Value::Object(settings)))
        .map_err(|error| format!("{}: {error}", path.display()))?;
    println!(
        "edited {SETTINGS_PATH}: removed the seeded startup hook entry; the kit seeds the compaction entry alone"
    );
    Ok(())
}

/// Step 4: gives the deliverables schema, and with the JSON tracker the backlog
/// schema, a baseline at its own bytes when it equals a content that the kit
/// shipped, so that the sync of the same run brings it to the current content.
/// A copy that equals none is the project's: it gets no baseline, and the hand
/// step names it. An absent schema is the backfill's; a schema that has a
/// baseline is the sync's.
fn deliver_the_schemas(
    ctx: &Context<'_>,
    marker: &mut Marker,
    hand_steps: &mut Vec<String>,
) -> Result<(), String> {
    let owned = marker.owned_paths();
    let json = marker
        .tracker()?
        .is_some_and(|config| config.adapter == Adapter::Json);
    let mut schemas = vec![DELIVERABLES_FILE];
    if json {
        schemas.push(TRACKER_SCHEMA);
    }
    let mut baselines = marker.baselines();
    let mut stamped = false;
    for path in schemas {
        if is_overridden(&owned, path) || baselines.contains_key(path) {
            continue;
        }
        let Some(current) = read_if_present(&join_components(ctx.root, path))? else {
            continue;
        };
        let mut candidates = known_contents(path, ctx.prefix)?;
        candidates.push(payload_content(path, ctx.prefix)?);
        let is_known = if path == DELIVERABLES_FILE {
            is_known_deliverables(&current, &candidates)
        } else {
            candidates.contains(&current)
        };
        if is_known {
            baselines.insert(path.to_string(), json!(baseline::hash(&current)));
            stamped = true;
        } else {
            hand_steps.push(hand_steps::schema_kept(path));
        }
    }
    if stamped {
        marker.set_baselines(baselines);
        marker.write()?;
    }
    Ok(())
}
