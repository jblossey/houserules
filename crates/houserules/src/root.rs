//! The shared `--dir`-or-git-root resolution every command's own
//! load-first wrapper needs -- `resolve_root`, one function in place of
//! a near-identical match/eprintln/exit-2 block repeated per command
//! file. A single function here keeps the error message and the exit
//! code in exactly one place, where a test can pin them, instead of one
//! copy per caller that a change would have to visit everywhere to keep
//! in sync. Lives at the crate root, like `emit` and `get`, for the
//! identical reason: `rules` and `backlog` both need it, and neither
//! should depend on the other for it (`emit.rs`'s own module doc has the
//! fuller account of that boundary) -- this module itself is the one
//! place that imports `rules::repo_root_from_cwd` for the git-plumbing
//! half of the resolution, so neither `backlog` nor any other caller
//! needs its own dependency on `rules` for that.
//!
//! Callers split two ways after resolving. Most load their one domain (a
//! `Base` or a `LoadedBacklog`) in the same breath, so a resolution
//! failure and a load failure share one error path. Three defer: `get`
//! checks its own arity before deciding which domain a given id even
//! needs (`get`'s own module doc has the reasoning); `archive` and
//! `check-report-claims` never load a `Base` or a `LoadedBacklog` at
//! all, since each reads (and, for `archive`, writes) its own files
//! directly instead.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::rules::repo_root_from_cwd;

/// Resolves `dir`, falling back to the enclosing git repository's top
/// level. A resolution failure (no enclosing repository, for instance)
/// prints one named stderr line and yields exit 2 -- the CLI-failure-path
/// convention every command in this binary follows
/// (`houserules.crash-paths-are-named`).
pub(crate) fn resolve_root(dir: Option<PathBuf>) -> Result<PathBuf, ExitCode> {
    match dir {
        Some(path) => Ok(path),
        None => repo_root_from_cwd().map_err(|error| {
            eprintln!("{error}");
            ExitCode::from(2)
        }),
    }
}
