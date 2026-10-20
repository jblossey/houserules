//! The install-root view of the path lists git prints.
//!
//! Every path a gate matches is relative to the install root, and a path
//! outside the root is out of scope. git prints `diff --name-only` paths
//! from the git top level, and `ls-tree` paths from the directory it runs
//! in. The helpers in `audit.rs` ask git for top-level paths, NUL-separated
//! (`-z`: git never quotes a name then), and [`InstallPrefix::map`] is the
//! one function that turns such a list into install-root paths. An install
//! at the git top level has an empty prefix, and every path maps to
//! itself.

use std::path::Path;

use super::render::git_location_in;

/// The install root's path below the git top level, as git prints it:
/// `/`-separated with a trailing `/` (`sub/`), empty for an install at the
/// top level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InstallPrefix(String);

impl InstallPrefix {
    /// Reads the prefix of `root` from git. A failure is git's own named
    /// line (`houserules.crash-paths-are-named`).
    pub(super) fn of(root: &Path) -> Result<Self, String> {
        git_location_in(root)
            .map(|location| Self(location.prefix))
            .map_err(|error| error.to_string())
    }

    /// `true` for an install below the git top level.
    pub(super) fn is_subdirectory(&self) -> bool {
        !self.0.is_empty()
    }

    /// The prefix as git prints it: `sub/`, empty at the top level.
    pub(super) fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` when `nul_separated`, NUL-separated top-level paths git
    /// printed, names at least one path and none of them is below the
    /// install root. A list with no path at all is not "only outside": a
    /// commit that changes no path (an empty commit) is no outside commit.
    pub(super) fn changes_only_outside(&self, nul_separated: &str) -> bool {
        let mut paths = nul_separated
            .split('\0')
            .filter(|path| !path.is_empty())
            .peekable();
        paths.peek().is_some() && paths.all(|path| !path.starts_with(self.0.as_str()))
    }

    /// Turns `nul_separated`, the NUL-separated top-level paths git printed,
    /// into install-root paths. A path outside the install root is dropped.
    /// The prefix ends in `/`, so `sub2/x` is outside the install `sub/`.
    pub(super) fn map(&self, nul_separated: &str) -> Vec<String> {
        nul_separated
            .split('\0')
            .filter(|path| !path.is_empty())
            .filter_map(|path| path.strip_prefix(self.0.as_str()))
            .map(str::to_string)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn prefix(value: &str) -> InstallPrefix {
        InstallPrefix(value.to_string())
    }

    /// An install at the git top level maps every path to itself.
    #[test]
    fn an_install_at_the_top_level_maps_every_path_to_itself() {
        assert_eq!(
            prefix("").map("a.txt\0sub/b.txt\0"),
            vec!["a.txt".to_string(), "sub/b.txt".to_string()]
        );
    }

    /// A subdirectory install strips its prefix and drops every path
    /// outside it.
    #[test]
    fn a_subdirectory_install_strips_its_prefix_and_drops_outside_paths() {
        assert_eq!(
            prefix("sub/").map("other/y.txt\0sub/src/x.rs\0top.txt\0sub/docs/a.md\0"),
            vec!["src/x.rs".to_string(), "docs/a.md".to_string()]
        );
    }

    /// A directory whose name starts with the install's folder name is
    /// outside the install: the prefix ends in `/`.
    #[test]
    fn a_sibling_whose_name_starts_like_the_install_folder_is_outside() {
        assert_eq!(
            prefix("sub/").map("sub2/x.rs\0sub.rs\0sub/x.rs\0"),
            vec!["x.rs".to_string()]
        );
    }

    /// A name keeps every byte git printed: a space, a non-ASCII letter,
    /// and a newline stay as they are.
    #[test]
    fn a_name_keeps_every_byte_git_printed() {
        assert_eq!(
            prefix("sub dir/sä/").map("sub dir/sä/a b.rs\0sub dir/sä/ü\nx.rs\0"),
            vec!["a b.rs".to_string(), "ü\nx.rs".to_string()]
        );
    }

    /// An empty listing maps to nothing, and so does one with only outside
    /// paths.
    #[test]
    fn a_listing_without_an_install_path_maps_to_nothing() {
        assert_eq!(prefix("sub/").map(""), Vec::<String>::new());
        assert_eq!(prefix("sub/").map("other/x.rs\0"), Vec::<String>::new());
    }

    /// A list is "only outside" when it holds a path and every path is
    /// outside the install. An empty list, and one with a path below the
    /// install, are not; neither is a sibling whose name starts like the
    /// install folder, which is outside.
    #[test]
    fn changes_only_outside_needs_a_path_and_no_path_below_the_install() {
        let install = prefix("sub/");
        assert!(install.changes_only_outside("other/y.txt\0sub2/x.rs\0"));
        assert!(!install.changes_only_outside(""));
        assert!(!install.changes_only_outside("\0"));
        assert!(!install.changes_only_outside("other/y.txt\0sub/x.rs\0"));
        assert!(!prefix("").changes_only_outside("other/y.txt\0"));
    }

    /// Only an empty prefix is an install at the top level.
    #[test]
    fn only_an_empty_prefix_is_an_install_at_the_top_level() {
        assert!(!prefix("").is_subdirectory());
        assert!(prefix("sub/").is_subdirectory());
        assert_eq!(prefix("sub dir/sä/").as_str(), "sub dir/sä/");
    }

    /// A fresh scratch directory with `git init` already run.
    fn scratch_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .expect("run git init");
        assert!(status.success(), "git init failed");
        dir
    }

    /// The prefix of a subdirectory is git's own prefix of it; at the top
    /// level it is empty.
    #[test]
    fn of_reads_git_s_prefix_for_a_subdirectory_and_an_empty_one_at_the_top_level() {
        let repo = scratch_repo();
        let sub = repo.path().join("sub dir").join("sä");
        std::fs::create_dir_all(&sub).expect("create the subdirectory");

        assert_eq!(
            InstallPrefix::of(&sub).expect("prefix"),
            prefix("sub dir/sä/")
        );
        assert_eq!(InstallPrefix::of(repo.path()).expect("prefix"), prefix(""));
    }

    /// Outside any repository the error is one line that names the
    /// directory. It names no `--dir` remedy: the directory is the one the
    /// caller already passed.
    #[test]
    fn of_names_the_directory_outside_a_repository() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = InstallPrefix::of(dir.path()).unwrap_err();
        assert_eq!(
            error,
            format!("{} is not inside a git repository", dir.path().display())
        );
    }
}
