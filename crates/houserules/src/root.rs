//! The shared `--dir`-or-install-root resolution every command's own
//! load-first wrapper needs -- `resolve_root`, one function in place of
//! a near-identical match/eprintln/exit-2 block repeated per command
//! file. A single function here keeps the error message and the exit
//! code in exactly one place, where a test can pin them, instead of one
//! copy per caller that a change would have to visit everywhere to keep
//! in sync. Lives at the crate root, like `emit` and `get`, for the
//! identical reason: `rules` and `backlog` both need it, and neither
//! should depend on the other for it (`emit.rs`'s own module doc has the
//! fuller account of that boundary) -- this module itself is the one
//! place that imports `rules::git_location_from_cwd` for the git-plumbing
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

use crate::rules::{GitLocation, git_location_from_cwd};

/// The file that marks an install root.
pub(crate) const INSTALL_MARKER: &str = ".houserules.json";

/// Resolves `dir`, falling back to the nearest install root: the nearest
/// directory at or above the current directory that holds
/// `.houserules.json`, searched no higher than the git top level, else the
/// git top level. A resolution failure prints one named stderr line and
/// yields exit 2 -- the CLI-failure-path convention every command in this
/// binary follows (`houserules.crash-paths-are-named`). With no enclosing
/// repository the line is `<cwd> is not inside a git repository; pass
/// --dir <install root>`; the working directory is never a fallback.
pub(crate) fn resolve_root(dir: Option<PathBuf>) -> Result<PathBuf, ExitCode> {
    match dir {
        Some(path) => Ok(path),
        None => git_location_from_cwd()
            .map(|location| nearest_install_root(&location))
            .map_err(|error| {
                eprintln!("{error}");
                ExitCode::from(2)
            }),
    }
}

/// The deepest directory of `location`'s path from the git top level down
/// to the current directory that holds `.houserules.json`; the top level
/// when none does. The candidates grow from git's own prefix, one
/// component at a time, so the search never leaves the repository and
/// never compares paths.
fn nearest_install_root(location: &GitLocation) -> PathBuf {
    let candidates: Vec<PathBuf> = location
        .prefix
        .split('/')
        .filter(|component| !component.is_empty())
        .scan(location.top_level.clone(), |path, component| {
            path.push(component);
            Some(path.clone())
        })
        .collect();
    candidates
        .into_iter()
        .rev()
        .find(|candidate| candidate.join(INSTALL_MARKER).is_file())
        .unwrap_or_else(|| location.top_level.clone())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// A location whose top level is `top` and whose current directory is
    /// `prefix` below it.
    fn location(top: &Path, prefix: &str) -> GitLocation {
        GitLocation {
            top_level: top.to_path_buf(),
            prefix: prefix.to_string(),
        }
    }

    fn mark(dir: &Path) {
        fs::create_dir_all(dir).expect("create the directory");
        fs::write(dir.join(INSTALL_MARKER), "{}\n").expect("write the marker");
    }

    /// The nearest marker at or above the current directory wins.
    #[test]
    fn the_nearest_marker_below_the_top_level_wins() {
        let top = tempfile::tempdir().expect("tempdir");
        mark(top.path());
        mark(&top.path().join("sub"));
        fs::create_dir_all(top.path().join("sub").join("x")).expect("create x");

        let root = nearest_install_root(&location(top.path(), "sub/x/"));

        assert_eq!(root, top.path().join("sub"));
    }

    /// A marker in the current directory itself is the root.
    #[test]
    fn a_marker_in_the_current_directory_is_the_root() {
        let top = tempfile::tempdir().expect("tempdir");
        mark(top.path());
        mark(&top.path().join("sub").join("x"));

        let root = nearest_install_root(&location(top.path(), "sub/x/"));

        assert_eq!(root, top.path().join("sub").join("x"));
    }

    /// With no nearer marker the marker at the top level is the root.
    #[test]
    fn the_top_level_marker_is_the_root_when_no_nearer_marker_exists() {
        let top = tempfile::tempdir().expect("tempdir");
        mark(top.path());
        fs::create_dir_all(top.path().join("sub").join("x")).expect("create x");

        let root = nearest_install_root(&location(top.path(), "sub/x/"));

        assert_eq!(root, top.path());
    }

    /// With no marker anywhere the root is the top level.
    #[test]
    fn the_root_is_the_top_level_when_no_marker_exists() {
        let top = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(top.path().join("sub").join("x")).expect("create x");

        let root = nearest_install_root(&location(top.path(), "sub/x/"));

        assert_eq!(root, top.path());
    }

    /// A marker in a sibling directory belongs to another install.
    #[test]
    fn a_marker_in_a_sibling_directory_is_not_the_root() {
        let top = tempfile::tempdir().expect("tempdir");
        mark(&top.path().join("other"));
        fs::create_dir_all(top.path().join("sub").join("x")).expect("create x");

        let root = nearest_install_root(&location(top.path(), "sub/x/"));

        assert_eq!(root, top.path());
    }

    /// Run at the top level (an empty prefix), the root is the top level.
    #[test]
    fn an_empty_prefix_gives_the_top_level() {
        let top = tempfile::tempdir().expect("tempdir");
        mark(&top.path().join("sub"));

        let root = nearest_install_root(&location(top.path(), ""));

        assert_eq!(root, top.path());
    }
}
