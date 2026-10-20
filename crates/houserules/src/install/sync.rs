//! The path-baseline sync: `update` keeps the files it owns at the content of
//! the running kit, and never overwrites a change the adopter made.
//!
//! The files are the `KIT_OWNED` paths, and the two D3 files once
//! `.houserules.json` baselines them: `knowledge/schema.json` and the
//! seeded CI workflow of the install (`workflow`'s own module doc has its path
//! and its baseline key in git's top-level pathspec form). A file's baseline is the hash
//! of the content the kit last wrote (`baseline::classify` has the decision
//! rule). Each file gets one outcome:
//!
//! - missing: written, and listed as `wrote <file>`;
//! - at its baseline: written with the kit's content and listed when its
//!   bytes change; a file whose bytes would not change is neither written
//!   nor listed, so a second `update` prints no `wrote` line. Either way the
//!   baseline is restamped, and a script keeps its executable bit;
//! - modified: kept, and listed as `kept <file> (locally modified)`;
//! - listed in the project's owned paths: left as found, with no line.
//!
//! While `renderers` lacks `claude`, the sync skips the Claude Code files
//! that `init` leaves out (`CLAUDE_FILES`).

use std::fs;
use std::io;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::baseline::{self, Status};

use super::workflow::Workflow;
use super::{
    CLAUDE_FILES, KIT_OWNED, SCHEMA_FILE, baseline_hash, is_overridden, payload_content,
    write_if_changed,
};

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
    /// The directory of the repository's top level, where the workflow file
    /// lies.
    pub(super) top_level: &'a Path,
    /// The workflow file of the install.
    pub(super) workflow: &'a Workflow,
}

/// Syncs every file of `plan` that the project does not own, restamps
/// `baselines`, and pushes one line for each file written or kept to
/// `lines`. The lines stay in `lines` when the sync fails, so the caller
/// prints what happened before the failure. An unreadable file is one named
/// error.
pub(super) fn sync_owned_files(
    plan: &Plan,
    baselines: &mut Map<String, Value>,
    lines: &mut Vec<String>,
) -> Result<(), String> {
    for &file in KIT_OWNED {
        if is_overridden(plan.owned, file) || (!plan.claude_enabled && CLAUDE_FILES.contains(&file))
        {
            continue;
        }
        let content = payload_content(file, plan.prefix)?;
        sync_file(plan.target, file, file, &content, baselines, lines)?;
    }
    let schema = payload_content(SCHEMA_FILE, plan.prefix)?;
    let workflow = plan.workflow;
    let d3_files = [
        (plan.target, SCHEMA_FILE, SCHEMA_FILE, &schema),
        (
            plan.top_level,
            workflow.path.as_str(),
            workflow.key.as_str(),
            &workflow.content,
        ),
    ];
    for (base, file, key, content) in d3_files {
        if baselines.contains_key(key) && !is_overridden(plan.owned, file) {
            sync_file(base, file, key, content, baselines, lines)?;
        }
    }
    Ok(())
}

/// Syncs the one file `path` below `base` against its baseline under `key`.
fn sync_file(
    base: &Path,
    path: &str,
    key: &str,
    content: &[u8],
    baselines: &mut Map<String, Value>,
    lines: &mut Vec<String>,
) -> Result<(), String> {
    let on_disk = base.join(path);
    match fs::read(&on_disk) {
        Ok(current) => match baseline::classify(
            baseline_hash(baselines, key),
            Some(&current),
            content,
            false,
        ) {
            Status::AtBaseline => {}
            Status::Modified => {
                lines.push(format!("kept {path} (locally modified)"));
                return Ok(());
            }
            Status::Overridden | Status::Deleted => {
                unreachable!(
                    "ownership is decided by the caller; a missing file is never classified"
                )
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("{}: {error}", on_disk.display())),
    }
    if write_if_changed(base, path, content)? {
        lines.push(format!("wrote {path}"));
    }
    baselines.insert(key.to_string(), json!(baseline::hash(content)));
    Ok(())
}
