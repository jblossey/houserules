//! Where a directory sits in its git repository: the one resolution that
//! `houserules render`, `check-knowledge`, and `check-backlog` run before
//! they load their base. `root::resolve_root` is the caller.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::rules::git;

/// Where a directory sits in its git repository.
#[derive(Debug)]
pub(crate) struct GitLocation {
    /// The repository's top-level directory (`git rev-parse
    /// --show-toplevel`), with the platform's own separator: git prints
    /// `/` on Windows too.
    pub top_level: PathBuf,
    /// The directory's own path below the top level, as git prints it:
    /// `/`-separated with a trailing `/` (`sub/x/`), empty at the top
    /// level (`git rev-parse --show-prefix`).
    pub prefix: String,
}

/// Locates the current working directory in its enclosing git repository,
/// the same resolution `houserules render` (and `houserules
/// check-knowledge` / `houserules check-backlog`, via `cmd_check_knowledge`
/// in `check.rs` and the `backlog` module's CLI wrappers) performs before
/// loading its base. Crate-visible, not `rules`-private: the `backlog`
/// module needs the identical resolution and would otherwise duplicate it.
/// `root::resolve_root` is the one caller; it prints the error this
/// returns.
///
/// Every failure arm is one line that names what failed:
/// - the working directory cannot be read: `cannot read the current
///   directory: <OS error>`;
/// - `git` cannot run: `cannot run git: <OS error>`;
/// - git finds no repository above the directory (see
///   `GIT_NO_REPOSITORY_FOUND`): this binary's own line, `<cwd> is not
///   inside a git repository; pass --dir <install root>`. The remedy
///   belongs here: the directory is the working directory, not a `--dir`;
/// - any other git failure keeps git's first non-empty stderr line, which
///   holds the recorded one-line error contract: git can print more than
///   one line.
pub(crate) fn git_location_from_cwd() -> io::Result<GitLocation> {
    let cwd = std::env::current_dir().map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read the current directory: {error}"),
        )
    })?;
    git_location_in(&cwd).map_err(|error| match error {
        LocateError::NotInRepository(dir) => io::Error::other(format!(
            "{} is not inside a git repository; pass --dir <install root>",
            dir.display()
        )),
        LocateError::Failed(error) => error,
    })
}

/// Why `git_location_in` could not place a directory.
#[derive(Debug)]
pub(in crate::rules) enum LocateError {
    /// No git repository holds the directory.
    NotInRepository(PathBuf),
    /// Any other failure, as the one line to print.
    Failed(io::Error),
}

impl fmt::Display for LocateError {
    /// One line. `NotInRepository` names the directory and no remedy: a
    /// caller that took the directory from `--dir` must not be told to pass
    /// it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInRepository(dir) => {
                write!(
                    formatter,
                    "{} is not inside a git repository",
                    dir.display()
                )
            }
            Self::Failed(error) => write!(formatter, "{error}"),
        }
    }
}

/// The start of the first stderr line `git rev-parse --show-toplevel
/// --show-prefix` prints when its upward search for a repository finds
/// none. Both
/// wordings of that search start this way: `LC_ALL=C git rev-parse
/// --show-toplevel` outside a repository prints `fatal: not a git
/// repository (or any parent up to mount point /)` then a second line,
/// `Stopping at filesystem boundary ...`, and with
/// `GIT_DISCOVERY_ACROSS_FILESYSTEM=1` it prints `fatal: not a git
/// repository (or any of the parent directories): .git` (git 2.55.0). A
/// `GIT_DIR` that names a missing directory prints `fatal: not a git
/// repository: '<dir>'` instead, which does not start this way: the
/// directory may sit inside a repository, so that line stays git's own.
const GIT_NO_REPOSITORY_FOUND: &str = "fatal: not a git repository (or any";

/// `git rev-parse --show-toplevel --show-prefix` run in `dir`, from the
/// command of the git runner, which sets `LC_ALL=C`: without it git translates
/// its messages (`LC_ALL=de_DE.UTF-8` prints `Schwerwiegend: Kein
/// Git-Repository ...`), and the check against `GIT_NO_REPOSITORY_FOUND` reads
/// English text.
fn git_rev_parse_location(dir: &Path) -> Command {
    let mut command = git::command(dir);
    command.args(["rev-parse", "--show-toplevel", "--show-prefix"]);
    command
}

/// `git_location_from_cwd`'s body, for any directory `dir`. git prints the
/// top level on the first line and the prefix on the second; the second
/// line is empty at the top level.
pub(in crate::rules) fn git_location_in(dir: &Path) -> Result<GitLocation, LocateError> {
    let output = git_rev_parse_location(dir).output().map_err(|error| {
        LocateError::Failed(io::Error::new(
            error.kind(),
            format!("cannot run git: {error}"),
        ))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first_line = stderr.lines().map(str::trim).find(|line| !line.is_empty());
        if first_line.is_some_and(|line| line.starts_with(GIT_NO_REPOSITORY_FOUND)) {
            return Err(LocateError::NotInRepository(dir.to_path_buf()));
        }
        return Err(LocateError::Failed(io::Error::other(
            first_line
                .unwrap_or("git rev-parse --show-toplevel --show-prefix failed")
                .to_string(),
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    Ok(GitLocation {
        top_level: Path::new(lines.next().unwrap_or_default().trim())
            .components()
            .collect(),
        prefix: lines.next().unwrap_or_default().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    /// A fresh scratch directory with `git init` already run.
    fn scratch_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = git::command(dir.path())
            .args(["init", "-q"])
            .status()
            .expect("run git init");
        assert!(status.success(), "git init failed");
        dir
    }

    /// Outside any repository the error names the directory, in place of
    /// git's own `fatal:` text, and names no remedy: the caller knows
    /// whether the directory came from `--dir`.
    #[test]
    fn git_location_in_names_the_directory_outside_a_repository() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = git_location_in(dir.path()).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("{} is not inside a git repository", dir.path().display())
        );
    }

    /// A git failure that is not "no repository" keeps git's own line:
    /// `git rev-parse --show-toplevel` fails in a bare repository, which
    /// has no work tree.
    #[test]
    fn git_location_in_keeps_the_git_line_for_another_git_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = git::command(dir.path())
            .args(["init", "-q", "--bare"])
            .status()
            .expect("run git init --bare");
        assert!(status.success(), "git init --bare failed");
        let message = git_location_in(dir.path()).unwrap_err().to_string();
        assert!(message.starts_with("fatal: "), "{message}");
        assert!(!message.contains("--dir"), "{message}");
    }

    /// The git command runs with `LC_ALL=C` on every host: the end-to-end
    /// German-locale test in `tests/check_parity.rs` proves the effect
    /// only where the `de_DE.UTF-8` locale is installed, and this test
    /// pins the cause everywhere.
    #[test]
    fn git_rev_parse_location_runs_git_with_lc_all_c() {
        let command = git_rev_parse_location(Path::new("."));
        let lc_all = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("LC_ALL"));
        assert_eq!(
            lc_all,
            Some((
                std::ffi::OsStr::new("LC_ALL"),
                Some(std::ffi::OsStr::new("C"))
            ))
        );
    }

    /// The resolution starts from the command of the git runner, which reads the
    /// objects as committed (`GIT_NO_REPLACE_OBJECTS`) and runs in the directory.
    #[test]
    fn git_rev_parse_location_starts_from_the_command_of_the_git_runner() {
        let command = git_rev_parse_location(Path::new("."));
        let replace = command
            .get_envs()
            .find(|(key, _)| *key == std::ffi::OsStr::new("GIT_NO_REPLACE_OBJECTS"));
        assert_eq!(
            replace,
            Some((
                std::ffi::OsStr::new("GIT_NO_REPLACE_OBJECTS"),
                Some(std::ffi::OsStr::new("1"))
            ))
        );
        assert_eq!(command.get_current_dir(), Some(Path::new(".")));
    }

    /// Inside a repository the top level and the directory's own path
    /// below it resolve from a subdirectory. The prefix keeps a space and
    /// a non-ASCII letter as git prints them, and ends in `/`.
    #[test]
    fn git_location_in_resolves_the_top_level_and_the_prefix_from_a_subdirectory() {
        let repo = scratch_git_repo();
        let sub = repo.path().join("a").join("b sä");
        fs::create_dir_all(&sub).expect("create subdirectory");
        let location = git_location_in(&sub).expect("resolve inside a repository");
        assert_eq!(
            location
                .top_level
                .canonicalize()
                .expect("canonicalize resolved root"),
            repo.path().canonicalize().expect("canonicalize repository")
        );
        assert_eq!(location.prefix, "a/b sä/");
    }

    /// The top level uses the platform's own separator, so a path that a
    /// line joins to it does not mix `/` and `\\` on Windows.
    #[test]
    fn git_location_in_gives_the_top_level_with_the_platform_separator() {
        let repo = scratch_git_repo();
        let location = git_location_in(repo.path()).expect("resolve inside a repository");
        let shown = location.top_level.display().to_string();
        let foreign = if cfg!(windows) { '/' } else { '\\' };
        assert!(!shown.contains(foreign), "{shown}");
    }

    /// At the top level the prefix is empty: git prints an empty second
    /// line there.
    #[test]
    fn git_location_in_gives_an_empty_prefix_at_the_top_level() {
        let repo = scratch_git_repo();
        let location = git_location_in(repo.path()).expect("resolve inside a repository");
        assert_eq!(location.prefix, "");
    }
}
