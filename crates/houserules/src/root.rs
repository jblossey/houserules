//! The shared `--dir`-or-git-root resolution every command's own load-first
//! wrapper needs -- `resolve_root` (batch 17 T4 fix round 1, review issue
//! 7), extracted from what had become eight near-identical copies of the
//! same match/eprintln/exit-2 block, one per command file
//! (`rules::render`'s `cmd_render`, `rules::check`'s `cmd_check_knowledge`,
//! `rules::validate_deliverable`'s `cmd_validate`, `rules::audit`'s
//! `cmd_audit`, `rules::stats`'s `cmd_stats`, `rules::read`'s `load`,
//! `backlog::cli`'s `load`, and `crate::get`'s `cmd_get`). A change to the
//! error message or the exit code once needed six other edits to match; a
//! test could not compare them, since nothing called out that they were
//! meant to agree. Lives at the crate root, like `emit` and `get`, for the
//! identical reason: `rules` and `backlog` both need it, and neither
//! should depend on the other for it (`emit.rs`'s own module doc has the
//! fuller account of that boundary) -- `backlog` already reached into
//! `rules::repo_root_from_cwd` directly for the git-plumbing half of this
//! (`backlog/cli.rs`'s own doc), so this only consolidates the wrapping
//! logic around it, not that existing, narrower cross-module reach.
//!
//! `crate::get::cmd_get` is this function's one caller that does NOT load
//! immediately after resolving the root: the unified `get` checks its own
//! arity first (docs/specs/2026-09-04-batch-15-tier2-spec.md §3, ruled at
//! the batch 17 T4 review -- `get`'s own module doc has the reasoning).
//! Every other caller resolves the root and loads its domain in the same
//! breath, matching the frozen JS's own `loadBase(repoRoot(cwd))`/
//! `loadBacklog(repoRoot(cwd))` call ahead of every dispatch.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::rules::repo_root_from_cwd;

/// Resolves `dir`, falling back to the enclosing git repository's top
/// level. A resolution failure (no enclosing repository, for instance)
/// prints one named stderr line and yields exit 2 -- the CLI-failure-path
/// convention every command in this binary follows (spec §6).
pub(crate) fn resolve_root(dir: Option<PathBuf>) -> Result<PathBuf, ExitCode> {
    match dir {
        Some(path) => Ok(path),
        None => repo_root_from_cwd().map_err(|error| {
            eprintln!("{error}");
            ExitCode::from(2)
        }),
    }
}
