//! The path-baseline sync: `update` keeps the files it owns at the content of
//! the running kit, and never overwrites a change the adopter made.
//!
//! The files are the `KIT_OWNED` paths, and the D3 files once
//! `.houserules.json` baselines them: `knowledge/schema.json`, the deliverables
//! schema, the seeded CI workflow of the install, and, where the install has the
//! JSON tracker and its `backlog/` directory, `backlog/schema.json`
//! (`ci_workflow`'s own module doc has the workflow's path and its baseline key
//! in the top-level pathspec form of `git`). A file's baseline is the hash of
//! the content the kit last wrote (`baseline::classify` has the decision rule).
//! A file whose bytes equal a content that the kit shipped before is the kit's
//! as well (spec 4.7, C35): its bytes stand for the baseline. Each file gets one
//! outcome:
//!
//! - missing: written, and listed as `wrote <file>`; the seeded CI workflow is
//!   the exception, which stays absent (spec 10.7);
//! - at its baseline: written with the kit's content and listed when its
//!   bytes change; a file whose bytes would not change is neither written
//!   nor listed, so a second `update` prints no `wrote` line. Either way the
//!   baseline is restamped, and a script keeps its executable bit;
//! - modified: kept, and listed as `kept <file> (locally modified)`;
//! - listed in the project's owned paths: left as found, with no line.
//!
//! The `backlogId` pattern of the deliverables schema is a setting of the
//! install, not a part of the kit's content (spec 3.9): the content that the
//! sync writes holds the pattern of the installed adapter, the pattern of the
//! kit's schema (any token) when the install has no tracker, or the pattern
//! that the file holds when the installed adapter cannot name its pattern
//! (C38). So a pattern that `init --tracker` wrote is no change by the project.
//!
//! While `renderers` lacks `claude`, the sync skips the Claude Code files
//! that `init` leaves out (`CLAUDE_FILES`).

use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::baseline::{self, Status};
use crate::tracker::{self, Adapter};

use super::ci_workflow::{self, Workflow};
use super::{
    CLAUDE_FILES, DELIVERABLES_FILE, KIT_OWNED, SCHEMA_FILE, TRACKER_SCHEMA, baseline_hash,
    is_overridden, join_components, known_contents, payload_content, read_if_present,
    write_if_changed,
};

/// What decides the `backlogId` pattern of the deliverables schema that the
/// sync writes (spec 3.9).
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BacklogIdPattern {
    /// The install has no tracker: the schema of the kit stands, whose pattern
    /// accepts any token.
    Kit,
    /// The installed adapter named this pattern.
    Named(String),
    /// The installed adapter cannot name its pattern (C38): the pattern that
    /// the file holds stays.
    Held,
}

/// What one sync run reads.
pub(super) struct Plan<'a> {
    /// The install root.
    pub(super) target: &'a Path,
    /// The id prefix that payload files are rewritten with.
    pub(super) prefix: &'a str,
    /// The paths the project owns: the sync never rewrites one.
    pub(super) owned: &'a [String],
    /// `true` when the Claude renderer is enabled.
    pub(super) claude_enabled: bool,
    /// `true` when the install has the JSON tracker and its `backlog/`
    /// directory, so that the tracker's schema is a D3 file of the sync.
    pub(super) json_tracker: bool,
    /// The adapter of the install, when it has a tracker.
    pub(super) adapter: Option<Adapter>,
    /// What decides the `backlogId` pattern of the deliverables schema.
    pub(super) backlog_id_pattern: &'a BacklogIdPattern,
    /// The directory of the repository's top level, where the workflow file
    /// lies.
    pub(super) top_level: &'a Path,
    /// The seeded workflow file of the install, when the install holds one
    /// (`Workflow::held_by_install`).
    pub(super) workflow: Option<&'a Workflow>,
}

/// Syncs every file of `plan` that the project does not own, restamps
/// `baselines`, pushes one line for each file written or kept to `lines`, and
/// pushes the hand steps of a seeded CI workflow that the project changed and
/// that lacks what its tracker needs, or the PR-head checkout, to `hand_steps`
/// (`ci_workflow::hand_step_for_kept`). The lines stay in `lines`
/// when the sync fails, so the caller prints what happened before the failure.
/// An unreadable file is one named error.
pub(super) fn sync_owned_files(
    plan: &Plan,
    baselines: &mut Map<String, Value>,
    lines: &mut Vec<String>,
    hand_steps: &mut Vec<String>,
) -> Result<(), String> {
    for &file in KIT_OWNED {
        if is_overridden(plan.owned, file) || (!plan.claude_enabled && CLAUDE_FILES.contains(&file))
        {
            continue;
        }
        let content = payload_content(file, plan.prefix)?;
        let known = known_contents(file, plan.prefix)?;
        sync_file(plan.target, file, file, &content, &known, baselines, lines)?;
    }
    let schema = payload_content(SCHEMA_FILE, plan.prefix)?;
    let deliverables = deliverables_content(plan.target, plan.prefix, plan.backlog_id_pattern)?;
    let tracker_schema = payload_content(TRACKER_SCHEMA, plan.prefix)?;
    let mut d3_files = vec![
        (plan.target, SCHEMA_FILE, SCHEMA_FILE, &schema),
        (
            plan.target,
            DELIVERABLES_FILE,
            DELIVERABLES_FILE,
            &deliverables,
        ),
    ];
    if let Some(workflow) = plan.workflow {
        d3_files.push((
            plan.top_level,
            workflow.path.as_str(),
            workflow.key.as_str(),
            &workflow.content,
        ));
    }
    if plan.json_tracker {
        d3_files.push((plan.target, TRACKER_SCHEMA, TRACKER_SCHEMA, &tracker_schema));
    }
    for (base, file, key, content) in d3_files {
        if baselines.contains_key(key) && !is_overridden(plan.owned, file) {
            let kept = sync_file(base, file, key, content, &[], baselines, lines)?;
            if kept && plan.workflow.is_some_and(|workflow| workflow.key == key) {
                let current = fs::read(base.join(file))
                    .map_err(|error| format!("{}: {error}", base.join(file).display()))?;
                hand_steps.extend(ci_workflow::hand_step_for_kept(
                    plan.adapter,
                    file,
                    &current,
                ));
            }
        }
    }
    Ok(())
}

/// Syncs the one file `path` below `base` against its baseline under `key`.
/// `known` holds the contents that the kit shipped before: a file whose bytes
/// equal one is at its baseline. Returns `true` when the file is kept because
/// the project changed it.
pub(super) fn sync_file(
    base: &Path,
    path: &str,
    key: &str,
    content: &[u8],
    known: &[Vec<u8>],
    baselines: &mut Map<String, Value>,
    lines: &mut Vec<String>,
) -> Result<bool, String> {
    let on_disk = base.join(path);
    match fs::read(&on_disk) {
        Ok(current) => {
            let known_hash = known.contains(&current).then(|| baseline::hash(&current));
            let recorded = known_hash
                .as_deref()
                .or_else(|| baseline_hash(baselines, key));
            match baseline::classify(recorded, Some(&current), content, false) {
                Status::AtBaseline => {}
                Status::Modified => {
                    lines.push(format!("kept {path} (locally modified)"));
                    return Ok(true);
                }
                Status::Overridden | Status::Deleted => {
                    unreachable!(
                        "ownership is decided by the caller; a missing file is never classified"
                    )
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{}: {error}", on_disk.display())),
    }
    if write_if_changed(base, path, content)? {
        lines.push(format!("wrote {path}"));
    }
    baselines.insert(key.to_string(), json!(baseline::hash(content)));
    Ok(false)
}

/// The `backlogId` pattern that the deliverables schema `text` holds, or
/// `None` for a text that is no schema with one.
fn backlog_id_pattern_of(text: &[u8]) -> Option<String> {
    let schema: Value = serde_json::from_slice(text).ok()?;
    schema
        .pointer("/$defs/backlogId/pattern")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// `schema`, a deliverables schema, with the `backlogId` pattern `pattern`; the
/// other bytes stay.
fn with_backlog_id_pattern(schema: &[u8], pattern: &str) -> Result<Vec<u8>, String> {
    let text =
        std::str::from_utf8(schema).map_err(|_| format!("{DELIVERABLES_FILE}: not valid UTF-8"))?;
    tracker::replace_backlog_id_pattern(text, pattern)
        .map(String::into_bytes)
        .map_err(|fault| format!("{DELIVERABLES_FILE}: {fault}"))
}

/// The content of the deliverables schema that the sync writes into the install
/// at `target` with the id prefix `prefix`: the kit's schema with the pattern
/// that `pattern` decides. The schema of the kit stands as it is for an install
/// with no tracker, and for a held pattern when the file is absent or has none.
pub(super) fn deliverables_content(
    target: &Path,
    prefix: &str,
    pattern: &BacklogIdPattern,
) -> Result<Vec<u8>, String> {
    let kit = payload_content(DELIVERABLES_FILE, prefix)?;
    let pattern = match pattern {
        BacklogIdPattern::Kit => None,
        BacklogIdPattern::Named(named) => Some(named.clone()),
        BacklogIdPattern::Held => read_if_present(&join_components(target, DELIVERABLES_FILE))?
            .as_deref()
            .and_then(backlog_id_pattern_of),
    };
    match pattern {
        Some(pattern) => with_backlog_id_pattern(&kit, &pattern),
        None => Ok(kit),
    }
}

/// `true` when the deliverables schema `current` equals one of `candidates`
/// apart from its `backlogId` pattern, which belongs to the install.
pub(super) fn is_known_deliverables(current: &[u8], candidates: &[Vec<u8>]) -> bool {
    match backlog_id_pattern_of(current) {
        Some(pattern) => candidates.iter().any(|candidate| {
            with_backlog_id_pattern(candidate, &pattern).is_ok_and(|aligned| aligned == current)
        }),
        None => candidates.iter().any(|candidate| candidate == current),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The deliverables schema of the kit with another `backlogId` pattern.
    fn schema_with(pattern: &str) -> Vec<u8> {
        with_backlog_id_pattern(
            &payload_content(DELIVERABLES_FILE, "WI").expect("the kit schema"),
            pattern,
        )
        .expect("a schema")
    }

    /// The content that the sync writes holds the pattern of the adapter when
    /// it named one, and the pattern of the kit when the install has no
    /// tracker, whatever the file holds. When the adapter cannot name its
    /// pattern, it holds the pattern of the file, else the pattern of the kit.
    #[test]
    fn the_deliverables_content_holds_the_pattern_that_the_plan_decides() {
        let dir = tempfile::tempdir().expect("tempdir");
        let kit = payload_content(DELIVERABLES_FILE, "WI").expect("the kit schema");
        assert_eq!(
            deliverables_content(dir.path(), "WI", &BacklogIdPattern::Held).expect("content"),
            kit,
            "no file and no pattern to hold"
        );
        let held = schema_with("^held$");
        crate::install::write_into(dir.path(), DELIVERABLES_FILE, &held).expect("write");
        assert_eq!(
            deliverables_content(dir.path(), "WI", &BacklogIdPattern::Held).expect("content"),
            held,
            "the adapter cannot name its pattern: the pattern of the file stays"
        );
        assert_eq!(
            deliverables_content(dir.path(), "WI", &BacklogIdPattern::Kit).expect("content"),
            kit,
            "no tracker: the pattern of the file does not stay"
        );
        assert_eq!(
            deliverables_content(
                dir.path(),
                "WI",
                &BacklogIdPattern::Named("^named$".to_string())
            )
            .expect("content"),
            schema_with("^named$"),
            "the adapter names a pattern"
        );
    }

    /// A schema is a known content apart from its `backlogId` pattern, which
    /// belongs to the install, and in no other part.
    #[test]
    fn a_deliverables_schema_is_known_apart_from_its_pattern_only() {
        let candidates = vec![schema_with("^older$")];
        assert!(is_known_deliverables(&schema_with("^mine$"), &candidates));
        let mut changed = serde_json::from_slice::<Value>(&schema_with("^mine$")).expect("JSON");
        changed["title"] = json!("Our own schema");
        let changed = serde_json::to_vec_pretty(&changed).expect("serialize");
        assert!(!is_known_deliverables(&changed, &candidates));
        assert!(!is_known_deliverables(b"[1]", &candidates));
        assert!(is_known_deliverables(b"[1]", &[b"[1]".to_vec()]));
    }

    /// A file whose bytes equal a known content is at its baseline although the
    /// marker records none: the sync writes the content and baselines it. Any
    /// other file with no baseline is kept.
    #[test]
    fn a_known_content_stands_for_the_baseline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = "tools/x.sh";
        let content = b"new\n".to_vec();
        let known = vec![b"old\n".to_vec()];
        crate::install::write_into(dir.path(), path, b"old\n").expect("write");
        let mut baselines = Map::new();
        let mut lines = Vec::new();

        let kept = sync_file(
            dir.path(),
            path,
            path,
            &content,
            &known,
            &mut baselines,
            &mut lines,
        )
        .expect("sync");

        assert!(!kept);
        assert_eq!(lines, [format!("wrote {path}")]);
        assert_eq!(baselines[path], json!(baseline::hash(&content)));
        assert_eq!(fs::read(dir.path().join(path)).expect("read"), content);

        crate::install::write_into(dir.path(), path, b"mine\n").expect("write");
        let mut baselines = Map::new();
        let mut lines = Vec::new();
        let kept = sync_file(
            dir.path(),
            path,
            path,
            &content,
            &known,
            &mut baselines,
            &mut lines,
        )
        .expect("sync");
        assert!(kept);
        assert_eq!(lines, [format!("kept {path} (locally modified)")]);
        assert!(baselines.is_empty());
    }
}
