//! The files of the `openspec/` folder at one commit (spec 3.1).
//!
//! [`Tree::read`] lists the folder with `git ls-tree -r -z` and reads a file
//! with `git cat-file blob`, both through `review::git`: a commit names every
//! input, and the work tree is never read
//! (`houserules.commit-bound-documents-read-the-commit`). The listing is
//! NUL-separated and so is never quoted, its pathspec is literal, and
//! `review::git` runs git with the C locale and clears the variables that
//! reshape a pathspec (`houserules.parsed-git-text-pins-every-setting`). The
//! install root is the working directory of the call, so an install below the
//! git top level reads its own `openspec/` folder.
//!
//! A symlink or a submodule is a path of the tree and no file of a change. At a
//! commit, a link holds only the path of its target and a submodule holds no
//! content, while OpenSpec reads through a file link: spec 3.1 names this
//! departure and its reversal.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::review::git;

/// The folder of an OpenSpec root, from the install root.
const ROOT: &str = "openspec";

/// One tracked path below the folder.
#[derive(Debug)]
struct Entry {
    /// Whether the path is a regular file: no symlink and no submodule.
    regular: bool,
    /// The object name of the blob.
    object: String,
}

/// The files below `openspec/` of the tree of one commit.
#[derive(Debug)]
pub(super) struct Tree {
    /// The install root.
    root: PathBuf,
    /// The tracked paths below the folder, from the install root.
    entries: BTreeMap<String, Entry>,
}

impl Tree {
    /// The tree of `commit` for the install at `root`. The `Err` is the line of
    /// git for a commit that it does not have.
    pub(super) fn read(root: &Path, commit: &str) -> Result<Self, String> {
        let listing = git::run_bytes(
            root,
            &[
                "ls-tree",
                "-r",
                "-z",
                commit,
                "--",
                &format!(":(literal){ROOT}"),
            ],
        )?;
        let listing = String::from_utf8_lossy(&listing);
        let prefix = format!("{ROOT}/");
        let mut entries = BTreeMap::new();
        for line in listing.split('\0').filter(|line| !line.is_empty()) {
            let Some((meta, path)) = line.split_once('\t') else {
                continue;
            };
            if !path.starts_with(&prefix) {
                continue;
            }
            let mut fields = meta.split(' ');
            let (mode, kind, object) = (fields.next(), fields.next(), fields.next());
            entries.insert(
                path.to_string(),
                Entry {
                    regular: kind == Some("blob")
                        && mode.is_some_and(|mode| mode.starts_with("100")),
                    object: object.unwrap_or_default().to_string(),
                },
            );
        }
        Ok(Self {
            root: root.to_path_buf(),
            entries,
        })
    }

    /// Every tracked path below the folder, sorted: files, symlinks, and
    /// submodules.
    pub(super) fn paths(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    /// The tracked paths whose folder is `folder`, from the install root, with
    /// their entries, in order.
    fn below<'a>(
        &'a self,
        folder: &str,
    ) -> impl Iterator<Item = (&'a String, &'a Entry)> + use<'a> {
        let prefix = format!("{folder}/");
        self.entries
            .range(prefix.clone()..)
            .take_while(move |(path, _)| path.starts_with(&prefix))
    }

    /// Whether `path` is a tracked path of the tree: a file, a symlink, or a
    /// submodule.
    pub(super) fn has_path(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    /// Whether a tracked path is below `folder`.
    pub(super) fn has_folder(&self, folder: &str) -> bool {
        self.below(folder).next().is_some()
    }

    /// The regular files below `folder`, at any depth, as paths from the
    /// install root, sorted.
    pub(super) fn files_below(&self, folder: &str) -> Vec<String> {
        self.below(folder)
            .filter(|(_, entry)| entry.regular)
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// The distinct names of the entries directly in `folder`, sorted.
    pub(super) fn child_names(&self, folder: &str) -> Vec<String> {
        let skip = folder.len() + 1;
        self.below(folder)
            .filter_map(|(path, _)| path[skip..].split('/').next())
            .map(str::to_string)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    /// The text of the regular file `path` of the tree, with a replacement
    /// character for each byte that is no UTF-8; `None` when the tree holds no
    /// regular file there. The `Err` is the line of git.
    pub(super) fn text(&self, path: &str) -> Result<Option<String>, String> {
        let Some(entry) = self.entries.get(path).filter(|entry| entry.regular) else {
            return Ok(None);
        };
        let content = git::run_bytes(&self.root, &["cat-file", "blob", &entry.object])?;
        Ok(Some(String::from_utf8_lossy(&content).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::review::git::scratch::{self, Repo};

    /// A repository with the files that the tests read.
    fn repo() -> Repo {
        let repo = Repo::new();
        repo.write("openspec/config.yaml", "schema: spec-driven\n");
        repo.write("openspec/changes/a/proposal.md", "# A\n");
        repo.write("openspec/changes/a/specs/cap/spec.md", "spec\n");
        repo.write("openspec/changes/archive/2026-01-02-b/proposal.md", "# B\n");
        repo.write("openspec/changes/.hidden/proposal.md", "# Hidden\n");
        repo.write("openspec-other/x.md", "not below the folder\n");
        repo.write("docs/y.md", "not below the folder\n");
        repo.commit("base");
        repo
    }

    /// The tree of `repo` at `commit`, read from the top.
    fn tree(repo: &Repo, commit: &str) -> Tree {
        Tree::read(repo.root(), commit).expect("read the tree")
    }

    #[test]
    fn the_tree_holds_every_file_below_openspec_and_nothing_else() {
        let repo = repo();
        let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();

        assert_eq!(
            tree(&repo, &head).paths(),
            [
                "openspec/changes/.hidden/proposal.md",
                "openspec/changes/a/proposal.md",
                "openspec/changes/a/specs/cap/spec.md",
                "openspec/changes/archive/2026-01-02-b/proposal.md",
                "openspec/config.yaml",
            ]
        );
    }

    #[test]
    fn a_folder_exists_when_a_file_is_below_it() {
        let repo = repo();
        let tree = tree(&repo, "HEAD");

        assert!(tree.has_folder("openspec/changes/a"));
        assert!(tree.has_folder("openspec/changes/archive"));
        assert!(tree.has_folder("openspec/changes/a/specs"));
        assert!(!tree.has_folder("openspec/changes/b"));
        assert!(!tree.has_folder("openspec/changes/a/proposal.md"));
        assert!(!tree.has_folder("openspec/chan"));
    }

    #[test]
    fn the_children_of_a_folder_are_its_distinct_names_sorted() {
        let repo = repo();
        let tree = tree(&repo, "HEAD");

        assert_eq!(
            tree.child_names("openspec/changes"),
            [".hidden", "a", "archive"]
        );
        assert_eq!(
            tree.child_names("openspec/changes/a"),
            ["proposal.md", "specs"]
        );
        assert_eq!(tree.child_names("openspec/missing"), Vec::<String>::new());
    }

    #[test]
    fn the_files_below_a_folder_are_sorted_paths_from_the_install_root() {
        let repo = repo();
        let tree = tree(&repo, "HEAD");

        assert_eq!(
            tree.files_below("openspec/changes/a"),
            [
                "openspec/changes/a/proposal.md",
                "openspec/changes/a/specs/cap/spec.md",
            ]
        );
        assert_eq!(
            tree.files_below("openspec/changes/aa"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_file_reads_as_text_at_the_commit() {
        let repo = repo();
        let first = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        repo.write("openspec/changes/a/proposal.md", "# Changed\n");
        repo.commit("change");
        repo.write("openspec/changes/a/proposal.md", "# Dirty\n");

        assert_eq!(
            tree(&repo, &first)
                .text("openspec/changes/a/proposal.md")
                .expect("read"),
            Some("# A\n".to_string())
        );
        assert_eq!(
            tree(&repo, "HEAD")
                .text("openspec/changes/a/proposal.md")
                .expect("read"),
            Some("# Changed\n".to_string())
        );
    }

    #[test]
    fn a_file_that_the_tree_does_not_hold_has_no_text() {
        let repo = repo();
        let tree = tree(&repo, "HEAD");

        assert_eq!(tree.text("openspec/changes/a/tasks.md"), Ok(None));
        assert_eq!(tree.text("openspec/changes/a"), Ok(None));
    }

    #[test]
    fn text_that_is_not_utf_8_reads_with_replacement_characters() {
        let repo = repo();
        repo.write_bytes("openspec/changes/a/tasks.md", b"- [x] caf\xe9\n");
        repo.commit("latin one");

        assert_eq!(
            tree(&repo, "HEAD")
                .text("openspec/changes/a/tasks.md")
                .expect("read"),
            Some("- [x] caf\u{fffd}\n".to_string())
        );
    }

    #[test]
    fn a_name_with_spaces_and_non_ascii_characters_reads_as_it_is() {
        let repo = Repo::new();
        repo.write("openspec/changes/é x/proposal.md", "# E\n");
        repo.commit("base");

        assert_eq!(
            tree(&repo, "HEAD").paths(),
            ["openspec/changes/é x/proposal.md"]
        );
    }

    #[test]
    fn a_tree_with_no_openspec_folder_holds_nothing() {
        let repo = Repo::new();
        repo.write("docs/y.md", "text\n");
        repo.write("openspec", "a file, not a folder\n");
        repo.commit("base");

        assert_eq!(tree(&repo, "HEAD").paths(), Vec::<String>::new());
    }

    #[test]
    fn a_subdirectory_install_reads_its_own_folder() {
        let repo = Repo::new();
        repo.write("openspec/changes/above/proposal.md", "# Above\n");
        repo.write("sub/openspec/changes/below/proposal.md", "# Below\n");
        repo.commit("base");

        let tree = Tree::read(&repo.root().join("sub"), "HEAD").expect("read the tree");

        assert_eq!(tree.paths(), ["openspec/changes/below/proposal.md"]);
        assert_eq!(
            tree.text("openspec/changes/below/proposal.md"),
            Ok(Some("# Below\n".to_string()))
        );
    }

    /// A symlink is a tracked path, but no regular file: it is no artifact and
    /// has no text. Symlinks need unix.
    #[cfg(unix)]
    #[test]
    fn a_symlink_is_a_path_and_no_file() {
        let repo = repo();
        std::os::unix::fs::symlink(
            "proposal.md",
            repo.root().join("openspec/changes/a/link.md"),
        )
        .expect("symlink");
        repo.commit("link");
        let tree = tree(&repo, "HEAD");

        assert!(
            tree.paths()
                .contains(&"openspec/changes/a/link.md".to_string())
        );
        assert!(
            !tree
                .files_below("openspec/changes/a")
                .contains(&"openspec/changes/a/link.md".to_string())
        );
        assert_eq!(tree.text("openspec/changes/a/link.md"), Ok(None));
    }

    #[test]
    fn a_commit_that_git_does_not_have_is_an_error_line() {
        let repo = repo();

        let line = Tree::read(repo.root(), &"0".repeat(40)).expect_err("no such commit");

        assert!(!line.is_empty());
        assert!(!line.contains('\n'), "{line}");
    }

    // ---- the settings that change git's text (houserules.parsed-git-text-pins-every-setting) --

    /// The test that the tests of an environment variable run again as a child
    /// process (`git::scratch::rerun_test`): it prints the paths of the tree
    /// that the variables `HR_RERUN_*` name, and does nothing when they are
    /// absent.
    #[test]
    fn rerun_prints_the_tree_of_its_environment() {
        let Some(root) = std::env::var_os("HR_RERUN_ROOT") else {
            return;
        };
        let commit = std::env::var("HR_RERUN_COMMIT").expect("HR_RERUN_COMMIT");
        let tree = Tree::read(Path::new(&root), &commit).expect("read the tree");
        println!("TREE {:?}", tree.paths());
    }

    /// The paths of the tree of `repo` that a child process reads with `envs`
    /// in its environment, as debug text.
    fn read_with_environment(repo: &Repo, envs: &[(&str, &str)]) -> String {
        let root = repo.root().to_str().expect("a UTF-8 path");
        let mut all = vec![("HR_RERUN_ROOT", root), ("HR_RERUN_COMMIT", "HEAD")];
        all.extend_from_slice(envs);
        let printed = scratch::rerun_test(
            "review::intent::openspec::tree::tests::rerun_prints_the_tree_of_its_environment",
            &all,
        );
        printed
            .lines()
            .find_map(|line| line.split_once("TREE ").map(|(_, tree)| tree))
            .expect("the child printed the tree")
            .to_string()
    }

    /// The global pathspec settings and a quoting setting of the environment
    /// change nothing: the listing is NUL-separated and its pathspec is
    /// literal.
    #[test]
    fn global_pathspec_settings_and_quoting_in_the_environment_change_nothing() {
        let repo = repo();
        repo.write("openspec/changes/é x/proposal.md", "# E\n");
        repo.commit("a name that git may quote");
        let plain = read_with_environment(&repo, &[]);
        assert!(plain.contains("é x"), "{plain}");

        for envs in [
            vec![("GIT_ICASE_PATHSPECS", "1")],
            vec![("GIT_LITERAL_PATHSPECS", "1")],
            vec![
                ("GIT_CONFIG_COUNT", "1"),
                ("GIT_CONFIG_KEY_0", "core.quotepath"),
                ("GIT_CONFIG_VALUE_0", "true"),
            ],
        ] {
            assert_eq!(read_with_environment(&repo, &envs), plain, "{envs:?}");
        }
    }
}
