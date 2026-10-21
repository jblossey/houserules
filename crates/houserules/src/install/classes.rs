//! The file classes of the kit: which path `init` and `update` treat how.
//! Every path is in one class (`houserules files` prints the classes; spec
//! 4.2):
//!
//! - [`KIT_OWNED`]: machinery that houserules owns. `init` writes it, and
//!   `update` overwrites each file that the project has not changed.
//! - [`SEED_ONCE`]: project data that `init` seeds once and `update` never
//!   overwrites.
//! - [`D3_FILES`]: files that `init` seeds once and `update` keeps at the
//!   content of the kit through a baseline, unless the project changed or
//!   owns them.
//! - [`TRACKER_DATA`] and [`TRACKER_SCHEMA`]: the files of the JSON tracker,
//!   which only `init --tracker json` writes and `update` restores where the
//!   install has that tracker.
//! - The files of a workflow (`workflow`'s own module doc names them):
//!   `init --workflow` writes them, and `update` moves them through the pins
//!   of the workflow. `houserules files` prints them as `workflowFiles` and
//!   `workflowData`.
//!
//! [`PREFIXED`] lists the seeded files that carry the backlog id prefix, and
//! [`RETIRED`] the paths that the kit no longer ships.

use super::ci_workflow;

/// Machinery files houserules owns: `init` writes them, and `update`
/// overwrites each one the adopter has not changed (`install`'s own module
/// doc, "`update` and the ownership baseline", has the keep and override
/// cases). `tests/install.rs`'s own copy pins this list, so the two cannot
/// silently drift apart. The step skill and script work in any workflow, so
/// every install gets them; the files of a workflow come with the workflow.
pub(super) const KIT_OWNED: &[&str] = &[
    CLAUDE_SESSION_START,
    ".claude/skills/migrating-knowledge/SKILL.md",
    ".claude/skills/houserules-step/SKILL.md",
    "tools/houserules-step.sh",
];

/// The session-start script of the Claude Code renderer. It holds the
/// re-injection of the standing rules after a compaction. A script of 1.x also
/// holds the ritual line, which the reference workflow moved to its own script
/// (spec 4.7, step 3).
pub(super) const CLAUDE_SESSION_START: &str = "tools/claude-session-start.sh";

/// The `KIT_OWNED` files that only the Claude Code renderer needs: the
/// session-start script runs from the hook entry of `.claude/settings.json`.
/// An install whose `renderers` lacks `claude` has none of them, and the sync
/// restores none.
pub(super) const CLAUDE_FILES: &[&str] = &[CLAUDE_SESSION_START];

/// The kit packs that a plain `init` installs into a new install (spec 4.6).
/// The other kit packs come with `pack add`, or with `init --workflow
/// --add-required-packs` for the packs that a workflow requires.
pub(super) const DEFAULT_PACKS: &[&str] = &["knowledge-base"];

/// Project-data files `init` seeds once, and `update` never overwrites once
/// they exist: an adopter's own edits to `AGENTS.md` or `CLAUDE.md` are never
/// kit-owned content. `update` still BACKFILLS a `SEED_ONCE` path
/// that is entirely absent (`install`'s own module doc, "`update`", has the
/// full account) -- a later kit release can add a new path to this list, and
/// an install seeded by an older release never had a chance to receive it.
/// A subset of these paths -- `knowledge_topic_files`, in the conversion --
/// is the 1.x layout's topic files: `init` seeds none of them (the kit's
/// rules come as packs), and `update` reconciles them entry by entry on a
/// 1.x tree only.
pub(super) const SEED_ONCE: &[&str] = &[
    SCHEMA_FILE,
    AREAS_FILE,
    DELIVERABLES_FILE,
    "docs/README.md",
    "AGENTS.md",
    "CLAUDE.md",
];

/// The five data files of the JSON tracker. `init --tracker json` seeds each
/// once, and `update` writes each one that is absent from an install that
/// has a `backlog/` directory and the JSON tracker; a file that exists is
/// project data and stays.
pub(super) const TRACKER_DATA: &[&str] = &[
    "backlog/batches.json",
    "backlog/amendments.json",
    "backlog/decisions.json",
    "backlog/parked.json",
    "backlog/items/general.json",
];

/// The schema of the JSON tracker's data, a D3 file of an install that has
/// the JSON tracker: `init --tracker json` baselines it when it writes it,
/// and `update` then keeps it at the content of the kit.
pub(super) const TRACKER_SCHEMA: &str = "backlog/schema.json";

/// The knowledge schema, a D3 file: the sync covers it once
/// `.houserules.json` baselines it.
pub(super) const SCHEMA_FILE: &str = "knowledge/schema.json";

/// The deliverables schema, a D3 file: the sync covers it once
/// `.houserules.json` baselines it. The `backlogId` pattern of the file belongs
/// to the tracker of the install, not to the kit (spec 3.9), so the content that
/// the sync writes holds the pattern of the adapter (`sync`'s own module doc).
pub(super) const DELIVERABLES_FILE: &str = ".claude/schemas/deliverables.json";

/// The one list of areas, which the project owns.
pub(super) const AREAS_FILE: &str = "knowledge/areas.json";

/// The D3 files: `init` seeds each once, and `update` then keeps it at the
/// content of the kit through a baseline, unless the project changed it or
/// owns it (`sync`'s own module doc has the account).
///
/// The CI workflow is the one D3 file that only a GitHub-hosted target gets:
/// the gate itself assumes a GitHub Actions runner and a GitHub PR event, so
/// writing it into a project hosted elsewhere would ship a workflow file
/// nothing ever runs. `seed` (`init`) writes it when the origin is GitHub
/// hosted (`origin_is_github_hosted`), keeps a file that exists and reports
/// it, and skips the write with a line that says why for any other origin.
/// `update` backfills a still-absent workflow the same way, but only once the
/// origin has become GitHub-hosted; skipping it for another origin stays
/// silent there, matching `update`'s own convention of reporting only what it
/// changes. A project not on GitHub gets its own CI gate from the
/// `migrating-knowledge` skill's own instruction instead.
pub(super) const D3_FILES: &[&str] = &[SCHEMA_FILE, DELIVERABLES_FILE, ci_workflow::TEMPLATE_PATH];

/// Seed files that carry the backlog id prefix; `--id-prefix` rewrites
/// them.
pub(super) const PREFIXED: &[&str] = &[
    "backlog/schema.json",
    "backlog/items/general.json",
    ".claude/schemas/deliverables.json",
];

/// Paths the kit does not ship, that `update` deletes from an install if
/// present (`install`'s own module doc, "Deletion", has the full account of
/// the mechanism).
pub(super) const RETIRED: &[&str] = &[
    "tools/kb.sh",
    "tools/backlog.sh",
    "tools/kb.mjs",
    "tools/backlog.mjs",
    "tools/lib/cli.mjs",
    "tools/lib/json-store.mjs",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Spec 4.6: the files that a plain `init` writes through the classes are
    /// the kit-owned files that need no Claude renderer plus the seed-once
    /// files. The marker, the pack copy, the `.gitignore` line, and the
    /// generated block are no class members.
    #[test]
    fn a_plain_init_writes_the_kit_owned_files_without_the_claude_ones_and_the_seed_once_files() {
        let mut written: Vec<&str> = KIT_OWNED
            .iter()
            .copied()
            .filter(|file| !CLAUDE_FILES.contains(file))
            .chain(SEED_ONCE.iter().copied())
            .collect();
        written.sort_unstable();
        assert_eq!(
            written,
            [
                ".claude/schemas/deliverables.json",
                ".claude/skills/houserules-step/SKILL.md",
                ".claude/skills/migrating-knowledge/SKILL.md",
                "AGENTS.md",
                "CLAUDE.md",
                "docs/README.md",
                "knowledge/areas.json",
                "knowledge/schema.json",
                "tools/houserules-step.sh",
            ]
        );
    }

    /// Spec 4.2: the session-start script is the one kit-owned file that only
    /// the Claude renderer needs.
    #[test]
    fn the_claude_files_are_the_session_start_script_alone() {
        assert_eq!(CLAUDE_FILES, ["tools/claude-session-start.sh"]);
    }
}
