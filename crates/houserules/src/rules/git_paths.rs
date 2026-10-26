//! The install-root view of the path lists git prints.
//!
//! Every path a gate matches is relative to the install root, and a path
//! outside the root is out of scope. git prints `diff --name-only` paths
//! from the git top level, and `ls-tree` paths from the directory it runs
//! in. The helpers in `git.rs` ask git for top-level paths, NUL-separated
//! (`-z`: git never quotes a name then), and [`InstallPrefix::map`] is the
//! one function that turns such a list into install-root paths. An install
//! at the git top level has an empty prefix, and every path maps to
//! itself.
//!
//! This module is also the one module that runs `git ls-files`. The binary
//! holds four listings: [`tracked_files`] (the index below the working
//! directory), [`tracked_from_top_level`] (the index of the whole repository),
//! [`listed_by_name`] (the files of one name, tracked or untracked and not
//! ignored), and [`submodule_paths`] (the submodules of the index). Each
//! caller keeps its own handling of a failure and of a path that is no UTF-8.
//! A lint test (`tests/ls_files_home.rs`) fails when another module passes the
//! `ls-files` argument to git.

use std::io;
use std::path::Path;
use std::process::Command;

use super::render::git_location_in;

/// The mode that the index records for a submodule.
const SUBMODULE_MODE: &str = "160000";

/// The install root's path below the git top level, as git prints it:
/// `/`-separated with a trailing `/` (`sub/`), empty for an install at the
/// top level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InstallPrefix(String);

impl InstallPrefix {
    /// Reads the prefix of `root` from git. A failure is git's own named
    /// line (`houserules.crash-paths-are-named`).
    pub(crate) fn of(root: &Path) -> Result<Self, String> {
        git_location_in(root)
            .map(|location| Self(location.prefix))
            .map_err(|error| error.to_string())
    }

    /// `true` for an install below the git top level.
    pub(super) fn is_subdirectory(&self) -> bool {
        !self.0.is_empty()
    }

    /// The prefix as git prints it: `sub/`, empty at the top level.
    pub(crate) fn as_str(&self) -> &str {
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
    pub(crate) fn map(&self, nul_separated: &str) -> Vec<String> {
        nul_separated
            .split('\0')
            .filter(|path| !path.is_empty())
            .filter_map(|path| path.strip_prefix(self.0.as_str()))
            .map(str::to_string)
            .collect()
    }
}

/// Runs `git ls-files -z` at `root`, decoding each NUL-separated entry on
/// its own. It returns `None` when `root` is not a git working tree or git
/// itself is not on `PATH`, and it never panics
/// (`bin/residue-gate.rs`'s own `tracked_files` panics on the identical
/// command, but that dev-only tool always runs inside this checkout's
/// own git history, where the command cannot fail this way). Otherwise it
/// returns the tracked paths that decode as UTF-8 and, as a second list,
/// every tracked path that does not, rendered lossily for display.
///
/// `-z` is not optional: without it, git's own `core.quotePath` (on by
/// default) double-quotes and octal-escapes any path byte outside
/// printable ASCII, so a tracked `docs/café.md` comes back as the
/// 12-character-escaped literal `"docs/caf\303\251.md"` -- a string no
/// glob matches -- and a naive `.lines()` split has the identical failure
/// mode for an embedded newline. `-z` disables that quoting and
/// NUL-terminates each entry instead of newline-terminating it.
///
/// Decoding happens per entry, not once over the whole buffer: a single
/// `String::from_utf8` over the joined output would turn ONE tracked path
/// with non-UTF-8 bytes (a latin-1 filename, say) into a `None` for the
/// ENTIRE result, silently losing every other, perfectly valid path.
/// Splitting on NUL first and decoding each piece keeps every valid entry
/// in the returned list (`-z`'s own trailing NUL leaves one empty final
/// piece, filtered) and collects every undecodable one, as its own raw
/// bytes rendered lossily for display, into the second list instead of
/// discarding it.
pub(crate) fn tracked_files(root: &Path) -> Option<(Vec<String>, Vec<String>)> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for entry in output.stdout.split(|&byte| byte == 0) {
        if entry.is_empty() {
            continue;
        }
        match std::str::from_utf8(entry) {
            Ok(path) => files.push(path.to_string()),
            Err(_) => skipped.push(String::from_utf8_lossy(entry).into_owned()),
        }
    }
    Some((files, skipped))
}

/// Every path of the index of the repository that holds `root`, whatever the
/// directory `root` is: `/`-separated, from the git top level, in the order git
/// prints them (`ls-files --full-name -- :/`). A subdirectory install sees the
/// files outside its own root too. A path that is no UTF-8 is left out: the
/// callers read files that houserules wrote, and the path of such a file is a
/// JSON string. A failure is one line: the error of the operating system when
/// git does not run, else git's own first line.
pub(crate) fn tracked_from_top_level(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--full-name", "--", ":/"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("git ls-files failed");
        return Err(format!(
            "cannot list the tracked files of the repository at {}: {line}",
            root.display()
        ));
    }
    Ok(output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| std::str::from_utf8(entry).ok())
        .map(str::to_string)
        .collect())
}

/// Why [`listed_by_name`] gave no listing.
#[derive(Debug)]
pub(crate) enum ListingFailure {
    /// git could not run; holds the error of the operating system.
    NotRun(io::Error),
    /// git ran and failed; holds the first non-empty line of its error
    /// output, trimmed, or `git ls-files failed` when it printed none.
    Failed(String),
}

/// The files named `name` at any depth below `dir` that git lists, tracked
/// or untracked and not ignored, as the raw bytes of each path below `dir`
/// (`/`-separated, in the order git prints). git lists a submodule as one
/// path and none of its files. The caller decodes the bytes and decides
/// what a [`ListingFailure`] means.
pub(crate) fn listed_by_name(dir: &Path, name: &str) -> Result<Vec<Vec<u8>>, ListingFailure> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ])
        .arg(format!(":(glob)**/{name}"))
        .current_dir(dir)
        .output()
        .map_err(ListingFailure::NotRun)?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("git ls-files failed");
        return Err(ListingFailure::Failed(line.to_string()));
    }
    Ok(output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(<[u8]>::to_vec)
        .collect())
}

/// The paths of the submodules in the index of the work tree at `root`, as
/// `/`-separated paths below `root`; `None` when git cannot list the index.
pub(crate) fn submodule_paths(root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "--stage", "-z"])
        .current_dir(root)
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|&byte| byte == 0)
            .filter_map(|entry| {
                let (record, path) = std::str::from_utf8(entry).ok()?.split_once('\t')?;
                (record.split(' ').next() == Some(SUBMODULE_MODE)).then(|| path.to_string())
            })
            .collect(),
    )
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

    /// The listing by name holds the matches at any depth, tracked or
    /// untracked, and leaves out an ignored match, a tracked file of another
    /// name, and a name that only ends like the one asked for.
    #[test]
    fn listed_by_name_lists_tracked_and_untracked_matches_and_leaves_out_ignored_ones() {
        let repo = scratch_repo();
        let root = repo.path();
        for (path, text) in [
            ("AGENTS.md", "top\n"),
            ("a/AGENTS.md", "tracked\n"),
            ("b/c/AGENTS.md", "untracked\n"),
            ("ignored/AGENTS.md", "ignored\n"),
            ("a/NOT-AGENTS.md", "other name\n"),
            (".gitignore", "ignored/\n"),
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the folder");
            std::fs::write(&path, text).expect("write the file");
        }
        let status = Command::new("git")
            .args(["add", "AGENTS.md", "a"])
            .current_dir(root)
            .status()
            .expect("run git add");
        assert!(status.success(), "git add failed");

        let mut listed: Vec<String> = listed_by_name(root, "AGENTS.md")
            .expect("a listing")
            .iter()
            .map(|raw| String::from_utf8_lossy(raw).into_owned())
            .collect();
        listed.sort();

        assert_eq!(listed, ["AGENTS.md", "a/AGENTS.md", "b/c/AGENTS.md"]);
    }

    /// A directory that is no work tree is a failure that holds the first
    /// line of git's own message. The message follows the host locale, so
    /// the test reads the expected line from git in the same directory.
    #[test]
    fn listed_by_name_fails_with_the_first_line_of_gits_message_outside_a_work_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let message = Command::new("git")
            .arg("ls-files")
            .current_dir(dir.path())
            .output()
            .expect("run git ls-files")
            .stderr;
        let expected = String::from_utf8_lossy(&message)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .expect("git names the failure")
            .to_string();

        let failure = listed_by_name(dir.path(), "AGENTS.md").expect_err("a failure");

        assert!(
            matches!(&failure, ListingFailure::Failed(line) if *line == expected),
            "{failure:?}"
        );
    }

    /// The listing from the top level holds every tracked path of the
    /// repository, whatever the directory it runs in: a subdirectory sees the
    /// paths outside itself too, and an untracked file is no entry.
    #[test]
    fn tracked_from_top_level_lists_the_whole_index_from_a_subdirectory() {
        let repo = scratch_repo();
        let root = repo.path();
        for (path, text) in [
            ("top.txt", "top\n"),
            ("sub/in.txt", "in\n"),
            ("other/out.txt", "out\n"),
            ("untracked.txt", "untracked\n"),
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the folder");
            std::fs::write(&path, text).expect("write the file");
        }
        let status = Command::new("git")
            .args(["add", "top.txt", "sub", "other"])
            .current_dir(root)
            .status()
            .expect("run git add");
        assert!(status.success(), "git add failed");

        let mut listed = tracked_from_top_level(&root.join("sub")).expect("a listing");
        listed.sort();

        assert_eq!(listed, ["other/out.txt", "sub/in.txt", "top.txt"]);
        let mut from_top = tracked_from_top_level(root).expect("a listing");
        from_top.sort();
        assert_eq!(from_top, listed);
    }

    /// A path that is no UTF-8 is left out; the others stay. The entry is seeded
    /// through the index, because not every file system takes the name.
    #[cfg(unix)]
    #[test]
    fn tracked_from_top_level_leaves_out_a_path_that_is_no_utf8() {
        use std::os::unix::ffi::OsStrExt;
        use std::process::Stdio;

        let repo = scratch_repo();
        let root = repo.path();
        let blob = Command::new("git")
            .args(["hash-object", "-w", "--stdin"])
            .current_dir(root)
            .stdin(Stdio::null())
            .output()
            .expect("run git hash-object");
        assert!(blob.status.success(), "git hash-object failed");
        let blob = String::from_utf8(blob.stdout).expect("an object id");
        for raw in [&b"caf\xe9.txt"[..], b"plain.txt"] {
            let status = Command::new("git")
                .args([
                    "update-index",
                    "--add",
                    "--cacheinfo",
                    "100644",
                    blob.trim(),
                ])
                .arg(std::ffi::OsStr::from_bytes(raw))
                .current_dir(root)
                .status()
                .expect("run git update-index");
            assert!(status.success(), "git update-index failed");
        }

        assert_eq!(
            tracked_from_top_level(root).expect("a listing"),
            ["plain.txt"]
        );
    }

    /// A directory that is no work tree is a failure that holds the first line
    /// of git's own message. The message follows the host locale, so the test
    /// reads the expected line from git in the same directory.
    #[test]
    fn tracked_from_top_level_fails_with_the_first_line_of_gits_message_outside_a_work_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let message = Command::new("git")
            .arg("ls-files")
            .current_dir(dir.path())
            .output()
            .expect("run git ls-files")
            .stderr;
        let expected = String::from_utf8_lossy(&message)
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .expect("git names the failure")
            .to_string();

        let error = tracked_from_top_level(dir.path()).expect_err("a failure");

        assert_eq!(
            error,
            format!(
                "cannot list the tracked files of the repository at {}: {expected}",
                dir.path().display()
            )
        );
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
