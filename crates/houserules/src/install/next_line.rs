//! The `next:` line of `init` and `update` (batch 38, spec 20.1).
//!
//! The line names the two gates to run. When the index below the install root
//! holds a file, `check-knowledge` reads the files that git tracks
//! (`tracked_tree_files`), so the files that the command wrote are dead globs
//! until `git add` stages them. The line then starts with `git add <paths> && `,
//! where `<paths>` are the first path components of the files that the command
//! wrote and that git neither tracks nor ignores, relative to the install root
//! and sorted. `git add` refuses an ignored path, so an ignored file is no path
//! of the line.
//!
//! A command opens [`Writes`] before its first write and asks it for the line
//! after its last. The files that the command wrote are the ones in the record
//! of its writes (`written`), which every write of `init` and `update` goes
//! through. The line opens no other file and reads the bytes of none: one
//! `git ls-files` call, restricted to the written paths, says which of them git
//! neither tracks nor ignores.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::rules::{tracked_files, untracked_among};
use crate::shell;
use crate::written::Recording;

/// The two gates that the line names, which follow the `git add` prefix.
const GATES: &str = "houserules check-knowledge && houserules check-backlog";

/// The record of the files that a command writes, for the `next:` line that the
/// command prints after it.
pub(super) struct Writes(Recording);

impl Writes {
    /// Opens the record. A command calls it before its first write.
    pub(super) fn open() -> Self {
        Self(Recording::open())
    }

    /// The whole `next:` line, read after the command wrote: with the `git add`
    /// prefix when the index below `root` holds a file and the command wrote at
    /// least one file below `root` that git neither tracks nor ignores, else the
    /// plain line. A path that a POSIX shell needs quoted is quoted
    /// (`shell::quote`).
    pub(super) fn line(self, root: &Path) -> String {
        let written = below_root(root, &self.0.finish());
        let paths = if written.is_empty() || !index_holds_a_file(root) {
            BTreeSet::new()
        } else {
            first_components_of_untracked(root, &written)
        };
        if paths.is_empty() {
            return format!("next: {GATES}");
        }
        let quoted: Vec<String> = paths.iter().map(|path| shell::quote(path)).collect();
        format!("next: git add {} && {GATES}", quoted.join(" "))
    }
}

/// `true` when the index below `root` holds a file, as the dead-glob gate reads
/// it (`tracked_files`). With no file in it, the gate walks the work tree, and
/// the line stays plain.
fn index_holds_a_file(root: &Path) -> bool {
    tracked_files(root).is_some_and(|(files, _skipped)| !files.is_empty())
}

/// The first path components, sorted and distinct, of those `written` files that
/// git neither tracks nor ignores. Empty when git cannot list the files.
fn first_components_of_untracked(root: &Path, written: &[String]) -> BTreeSet<String> {
    untracked_among(root, written)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|path| path.split('/').next().map(str::to_string))
        .collect()
}

/// The paths of `written` below `root`, as sorted, distinct, `/`-separated paths
/// relative to `root`. A path outside `root` is dropped, and so is one that has
/// no UTF-8 name: a printed `git add` line cannot name it.
fn below_root(root: &Path, written: &[PathBuf]) -> Vec<String> {
    let relative: BTreeSet<String> = written
        .iter()
        .filter_map(|path| relative_to(root, path))
        .collect();
    relative.into_iter().collect()
}

/// `path` below `root` as a `/`-separated path, or `None` for a path that is
/// outside `root`, is `root` itself, or has a part other than a plain name.
fn relative_to(root: &Path, path: &Path) -> Option<String> {
    let parts: Vec<&str> = path
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect::<Option<_>>()?;
    (!parts.is_empty()).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::git::scratch::Repo;

    /// The line without the prefix.
    const PLAIN: &str = "next: houserules check-knowledge && houserules check-backlog";

    /// A repository whose index holds `README.md`.
    fn repo_with_a_commit() -> Repo {
        let repo = Repo::new();
        repo.write("README.md", "text\n");
        repo.commit("chore: start the project");
        repo
    }

    /// Writes `path` below the top of `repo` through the record of the writes, as a command does.
    fn write_recorded(repo: &Repo, path: &str) {
        let file = repo.root().join(path);
        std::fs::create_dir_all(file.parent().expect("a parent folder")).expect("create folders");
        crate::written::write(&file, "text\n").expect("write the file");
    }

    /// An empty index leaves the line plain, whatever the command writes.
    #[test]
    fn an_empty_index_gives_the_plain_line() {
        let repo = Repo::new();
        let writes = Writes::open();
        write_recorded(&repo, "new.txt");

        assert_eq!(writes.line(repo.root()), PLAIN);
    }

    /// A directory that is no work tree has no listing, so the line is plain.
    #[test]
    fn a_directory_that_is_no_work_tree_gives_the_plain_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let writes = Writes::open();
        crate::written::write(&dir.path().join("new.txt"), "text\n").expect("write the file");

        assert_eq!(writes.line(dir.path()), PLAIN);
    }

    /// A command that writes nothing leaves the line plain, although an untracked file is there.
    /// So does one that writes a file that the index holds.
    #[test]
    fn a_command_that_writes_no_untracked_file_gives_the_plain_line() {
        let repo = repo_with_a_commit();
        repo.write("kept.txt", "text\n");
        assert_eq!(Writes::open().line(repo.root()), PLAIN);

        let writes = Writes::open();
        write_recorded(&repo, "README.md");
        assert_eq!(writes.line(repo.root()), PLAIN);
    }

    /// The line names each first path component once, sorted by byte, and no file that the command
    /// did not write.
    #[test]
    fn the_line_names_the_first_component_of_each_file_written_once_and_sorted() {
        let repo = repo_with_a_commit();
        repo.write("kept.txt", "text\n");
        let writes = Writes::open();
        for path in ["b/x.txt", "b/deep/y.txt", "a.txt", ".hidden/z.txt", "B.md"] {
            write_recorded(&repo, path);
        }

        assert_eq!(
            writes.line(repo.root()),
            "next: git add .hidden B.md a.txt b && houserules check-knowledge && houserules check-backlog"
        );
    }

    /// A file written twice is one path.
    #[test]
    fn a_file_written_twice_is_one_path() {
        let repo = repo_with_a_commit();
        let writes = Writes::open();
        write_recorded(&repo, "twice.txt");
        write_recorded(&repo, "twice.txt");

        assert_eq!(
            writes.line(repo.root()),
            "next: git add twice.txt && houserules check-knowledge && houserules check-backlog"
        );
    }

    /// A file that git ignores is no path of the line: `git add` refuses it.
    #[test]
    fn a_written_file_that_git_ignores_is_no_path() {
        let repo = repo_with_a_commit();
        repo.write(".gitignore", "ignored.txt\n");
        repo.commit("chore: ignore a file");
        let writes = Writes::open();
        write_recorded(&repo, "ignored.txt");
        write_recorded(&repo, "kept.txt");

        assert_eq!(
            writes.line(repo.root()),
            "next: git add kept.txt && houserules check-knowledge && houserules check-backlog"
        );
    }

    /// The paths are relative to the install root, and a file written outside it is no path.
    #[test]
    fn a_file_outside_the_install_root_is_no_path() {
        let repo = Repo::new();
        repo.write("sub/keep.txt", "text\n");
        repo.commit("chore: start the project");
        let root = repo.root().join("sub");
        let writes = Writes::open();
        write_recorded(&repo, "top.txt");
        write_recorded(&repo, "sub/docs/in.txt");

        assert_eq!(
            writes.line(&root),
            "next: git add docs && houserules check-knowledge && houserules check-backlog"
        );
    }

    /// A path that a POSIX shell needs quoted is quoted.
    #[test]
    fn a_path_that_a_shell_needs_quoted_is_quoted() {
        let repo = repo_with_a_commit();
        let writes = Writes::open();
        write_recorded(&repo, "my notes/a.txt");

        assert_eq!(
            writes.line(repo.root()),
            "next: git add 'my notes' && houserules check-knowledge && houserules check-backlog"
        );
    }
}
