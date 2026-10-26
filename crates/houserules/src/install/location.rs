//! Where an install root sits in its git repository, and the other installs
//! of that repository (spec section 8, C12).
//!
//! `init` and `update` install in the directory they are given: the git top
//! level, or a directory below it. [`locate`] finds the top level by the
//! filesystem alone: no git setting (`GIT_DIR`, `GIT_WORK_TREE`,
//! `core.worktree`) changes which directory it names. [`other_installs`]
//! lists the other `.houserules.json` files of the repository, and
//! [`refuse_overlap`] refuses a new install that nests with one of them. `init`
//! calls it for a root with no marker yet only: an existing install is no new
//! root.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::node_path::resolve_like_node;
use crate::root::INSTALL_MARKER;
use crate::rules::{ListingFailure, listed_by_name};

/// An install root's place in its git repository.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Placement {
    /// The repository's top-level directory.
    pub(super) top_level: PathBuf,
    /// The root's path below the top level, one component each; empty for an
    /// install at the top level.
    pub(super) components: Vec<String>,
}

impl Placement {
    /// `true` for an install at the git top level.
    pub(super) fn is_top_level(&self) -> bool {
        self.components.is_empty()
    }

    /// The root's path below the top level, `/`-separated; empty at the top
    /// level.
    pub(super) fn path(&self) -> String {
        self.components.join("/")
    }

    /// The root's directory, built one component at a time
    /// (`houserules.path-pins-mirror-the-code`).
    pub(super) fn root(&self) -> PathBuf {
        root_of(&self.top_level, &self.components)
    }
}

/// The directory below `top_level` with the path `components`, built one
/// component at a time.
fn root_of(top_level: &Path, components: &[String]) -> PathBuf {
    components
        .iter()
        .fold(top_level.to_path_buf(), |path, part| path.join(part))
}

/// The nearest directory above `dir`, nearest first, that holds a `.git`
/// entry: a directory in a repository's top level, or the file in a linked
/// worktree's. `dir` itself is not a candidate. The ancestors are those of
/// the path as passed, so the result is a prefix of `dir`; the caller
/// decides which spelling of the path is walked. No git setting (`GIT_DIR`,
/// `GIT_WORK_TREE`, `core.worktree`) changes which directory it names.
fn nearest_ancestor_holding_git(dir: &Path) -> Option<&Path> {
    dir.ancestors()
        .skip(1)
        .find(|ancestor| ancestor.join(".git").exists())
}

/// Finds `target`'s place in its git repository. `init` and `update` pass an
/// absolute `target`: `node_path::resolve_like_node` has already resolved
/// `--dir`, collapsing `..` and keeping symlinks. A `target` that holds a
/// `.git` entry, reached through any symlink in its path, is the top level.
/// Otherwise the top level is the nearest ancestor of the resolved path that
/// holds a `.git` entry, and the placement is the resolved path below it:
/// a symlink into a repository subdirectory is inside the repository by the
/// same rule. A `target` outside any repository, one that does not exist yet,
/// and one that cannot be resolved are refused with the `git init` advice,
/// which prints `target` as passed. A path component that is no UTF-8 is one
/// named line, since the placement keeps the path as text.
pub(super) fn locate(target: &Path) -> Result<Placement, String> {
    if target.join(".git").exists() {
        return Ok(Placement {
            top_level: target.to_path_buf(),
            components: Vec::new(),
        });
    }
    let resolved = target
        .canonicalize()
        .and_then(|canonical| resolve_like_node(&canonical))
        .ok()
        .filter(|resolved| resolved.is_dir());
    let Some((resolved, top_level)) = resolved.as_deref().and_then(|resolved| {
        nearest_ancestor_holding_git(resolved).map(|top_level| (resolved, top_level))
    }) else {
        return Err(format!(
            "{} is not a git repository (run git init first)",
            target.display()
        ));
    };
    let below = resolved
        .strip_prefix(top_level)
        .expect("an ancestor of a path is a prefix of it");
    let components = below
        .components()
        .map(|part| {
            part.as_os_str().to_str().map(str::to_string).ok_or_else(|| {
                format!(
                    "{}: the path holds a name that is no UTF-8; houserules reads the path of an install as UTF-8 text",
                    resolved.display()
                )
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Placement {
        top_level: top_level.to_path_buf(),
        components,
    })
}

/// The roots of the installs of the repository at `placement`, other than
/// the install at `placement` itself, each as its components below the top
/// level, sorted. An install is a `.houserules.json` that git lists, tracked
/// or untracked and not ignored, and that exists on disk. git lists a
/// submodule as one path and none of its files, so an install inside a
/// submodule belongs to that repository.
pub(super) fn other_installs(placement: &Placement) -> Result<Vec<Vec<String>>, String> {
    let markers =
        listed_by_name(&placement.top_level, INSTALL_MARKER).map_err(|failure| match failure {
            ListingFailure::NotRun(error) => format!("cannot run git: {error}"),
            ListingFailure::Failed(line) => format!(
                "cannot list the installs of the repository at {}: {line}",
                placement.top_level.display()
            ),
        })?;
    let mut roots = BTreeSet::new();
    for listed in &markers {
        let listed = std::str::from_utf8(listed).map_err(|_| {
            format!(
                "cannot list the installs of the repository at {}: git lists a path that is no UTF-8",
                placement.top_level.display()
            )
        })?;
        let mut components: Vec<String> = listed.split('/').map(str::to_string).collect();
        components.pop();
        if placement.top_level.join(listed).is_file() && components != placement.components {
            roots.insert(components);
        }
    }
    Ok(roots.into_iter().collect())
}

/// `true` when `ancestor` is a proper prefix of `path`, component by
/// component.
fn is_proper_ancestor(ancestor: &[String], path: &[String]) -> bool {
    ancestor.len() < path.len() && path.starts_with(ancestor)
}

/// Refuses, with one line that names the other install, an install root
/// that lies inside the root of one of `others` and one that holds the root of
/// one of them. `others` is sorted, so the first of them that clashes is the
/// one named. An install at the git top level lies above every other install.
pub(super) fn refuse_overlap(placement: &Placement, others: &[Vec<String>]) -> Result<(), String> {
    let own = placement.root();
    for other in others {
        let named = root_of(&placement.top_level, other).display().to_string();
        if is_proper_ancestor(other, &placement.components) {
            return Err(format!(
                "{}: the install root lies inside the install at {named}; houserules installs do not nest",
                own.display()
            ));
        }
        if is_proper_ancestor(&placement.components, other) {
            return Err(format!(
                "{}: the install root holds the install at {named}; houserules installs do not nest",
                own.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    /// A placement below `/repo` with the path `components`.
    fn placement(components: &[&str]) -> Placement {
        Placement {
            top_level: PathBuf::from("/repo"),
            components: components.iter().map(|part| part.to_string()).collect(),
        }
    }

    /// The root of an install with the path `components`, as the
    /// `others` of `refuse_overlap` hold it.
    fn root(components: &[&str]) -> Vec<String> {
        components.iter().map(|part| part.to_string()).collect()
    }

    /// A scratch git repository. `tempfile`'s `TempDir` removes itself on
    /// drop.
    fn git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .expect("run git init");
        assert!(status.success(), "git init failed");
        dir
    }

    /// `path` in the form [`locate`] resolves a target to: canonical, and
    /// on Windows without the `\\?\` prefix that `canonicalize` adds.
    fn resolved(path: &Path) -> PathBuf {
        resolve_like_node(&path.canonicalize().expect("canonicalize")).expect("resolve")
    }

    /// Writes an empty install marker below `dir`.
    fn mark(dir: &Path, relative: &str) {
        let path = dir.join(relative).join(INSTALL_MARKER);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
        fs::write(path, "{}\n").expect("write the marker");
    }

    /// A target that holds `.git` is the top level, with no component.
    #[test]
    fn a_target_that_holds_git_is_the_top_level() {
        let dir = git_repo();

        let placed = locate(dir.path()).expect("a placement");

        assert_eq!(
            placed,
            Placement {
                top_level: dir.path().to_path_buf(),
                components: Vec::new()
            }
        );
        assert!(placed.is_top_level());
        assert_eq!(placed.path(), "");
        assert_eq!(placed.root(), dir.path());
    }

    /// A directory below the top level is placed by its path below it, one
    /// component for each directory, whatever the names hold.
    #[test]
    fn a_directory_below_the_top_level_is_placed_by_its_path() {
        let dir = git_repo();
        let nested = dir.path().join("my dir").join("über");
        fs::create_dir_all(&nested).expect("create the directory");
        let top = resolved(dir.path());

        let placed = locate(&nested).expect("a placement");

        assert_eq!(placed.top_level, top);
        assert_eq!(placed.components, ["my dir", "über"]);
        assert_eq!(placed.path(), "my dir/über");
        assert!(!placed.is_top_level());
        assert_eq!(placed.root(), top.join("my dir").join("über"));
    }

    /// The nearest `.git` above the target is the top level: a nested
    /// repository or a linked worktree (`.git` as a file) wins over the
    /// repository around it.
    #[test]
    fn the_nearest_git_entry_above_the_target_is_the_top_level() {
        let dir = git_repo();
        let worktree = dir.path().join("wt");
        let target = worktree.join("sub");
        fs::create_dir_all(&target).expect("create the directory");
        fs::write(worktree.join(".git"), "gitdir: elsewhere\n").expect("write .git file");
        let worktree = resolved(&worktree);

        let placed = locate(&target).expect("a placement");

        assert_eq!(placed.top_level, worktree);
        assert_eq!(placed.components, ["sub"]);
    }

    /// A target outside any repository, one that does not exist, and a file
    /// get the `git init` advice, which prints the target as passed.
    #[test]
    fn a_target_outside_a_repository_gets_the_git_init_advice() {
        let outside = tempfile::tempdir().expect("tempdir");
        let repo = git_repo();
        let file = repo.path().join("f");
        fs::write(&file, "").expect("create the file");
        for target in [
            outside.path().to_path_buf(),
            repo.path().join("absent"),
            file,
        ] {
            assert_eq!(
                locate(&target),
                Err(format!(
                    "{} is not a git repository (run git init first)",
                    target.display()
                ))
            );
        }
    }

    /// A symlink to a repository subdirectory is placed by the path it
    /// resolves to.
    #[cfg(unix)]
    #[test]
    fn a_symlink_to_a_subdirectory_is_placed_by_its_resolved_path() {
        let dir = git_repo();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).expect("create the directory");
        let links = tempfile::tempdir().expect("tempdir");
        let link = links.path().join("link");
        std::os::unix::fs::symlink(&sub, &link).expect("create the symlink");

        let placed = locate(&link).expect("a placement");

        assert_eq!(
            placed.top_level,
            dir.path().canonicalize().expect("canonicalize")
        );
        assert_eq!(placed.components, ["sub"]);
    }

    /// A path component that is no UTF-8 is one named line.
    #[cfg(unix)]
    #[test]
    fn a_directory_name_that_is_no_utf8_is_one_named_line() {
        use std::os::unix::ffi::OsStrExt;

        let dir = git_repo();
        let name = std::ffi::OsStr::from_bytes(b"bad\xffname");
        let target = dir.path().join(name);
        if fs::create_dir(&target).is_err() {
            return;
        }
        let resolved = target.canonicalize().expect("canonicalize");

        let error = locate(&target).expect_err("a refusal");

        assert_eq!(
            error,
            format!(
                "{}: the path holds a name that is no UTF-8; houserules reads the path of an install as UTF-8 text",
                resolved.display()
            )
        );
    }

    /// The listing holds every other install that git lists: tracked and
    /// untracked, sorted, with the install at `placement` itself, an ignored
    /// install, and a tracked marker that is gone from the disk left out.
    #[test]
    fn other_installs_lists_the_other_markers_git_lists() {
        let dir = git_repo();
        mark(dir.path(), "b/c");
        mark(dir.path(), "a");
        mark(dir.path(), "own");
        mark(dir.path(), "ignored");
        mark(dir.path(), "gone");
        fs::write(dir.path().join(".gitignore"), "ignored/\n").expect("write .gitignore");
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(dir.path())
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["add", "a", "gone"]);
        fs::remove_file(dir.path().join("gone").join(INSTALL_MARKER)).expect("remove the marker");
        let own = Placement {
            top_level: dir.path().to_path_buf(),
            components: vec!["own".to_string()],
        };

        let others = other_installs(&own).expect("a listing");

        assert_eq!(others, [root(&["a"]), root(&["b", "c"])]);
    }

    /// A marker at the top level is an install with no component.
    #[test]
    fn other_installs_lists_an_install_at_the_top_level() {
        let dir = git_repo();
        mark(dir.path(), "");
        let sub = Placement {
            top_level: dir.path().to_path_buf(),
            components: vec!["sub".to_string()],
        };

        assert_eq!(other_installs(&sub), Ok(vec![Vec::<String>::new()]));
    }

    /// A directory that git cannot list is one named line.
    #[test]
    fn other_installs_names_a_failed_listing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let placed = Placement {
            top_level: dir.path().to_path_buf(),
            components: vec!["sub".to_string()],
        };

        let error = other_installs(&placed).expect_err("a refusal");

        assert!(
            error.starts_with(&format!(
                "cannot list the installs of the repository at {}: ",
                dir.path().display()
            )),
            "{error}"
        );
    }

    /// The directory of the install with the path `components`, below
    /// `/repo`, as a refusal line prints it.
    fn at(components: &[&str]) -> String {
        root_of(Path::new("/repo"), &root(components))
            .display()
            .to_string()
    }

    /// An install root below another install, and one above another install,
    /// are refused with one line that names the other; a sibling is not.
    #[test]
    fn a_nested_install_is_refused_and_a_sibling_is_not() {
        let others = [root(&["a"]), root(&["b", "c"])];

        assert_eq!(
            refuse_overlap(&placement(&["a", "d"]), &others),
            Err(format!(
                "{}: the install root lies inside the install at {}; houserules installs do not nest",
                at(&["a", "d"]),
                at(&["a"])
            ))
        );
        assert_eq!(
            refuse_overlap(&placement(&["b"]), &others),
            Err(format!(
                "{}: the install root holds the install at {}; houserules installs do not nest",
                at(&["b"]),
                at(&["b", "c"])
            ))
        );
        assert_eq!(refuse_overlap(&placement(&["e"]), &others), Ok(()));
        // A name that starts with another install's name is no nesting: the
        // paths compare component by component.
        assert_eq!(refuse_overlap(&placement(&["ab"]), &others), Ok(()));
        assert_eq!(refuse_overlap(&placement(&["ab", "x"]), &others), Ok(()));
        assert_eq!(
            refuse_overlap(&placement(&["a"]), &[root(&["ab", "x"])]),
            Ok(())
        );
        assert_eq!(refuse_overlap(&placement(&["a"]), &[]), Ok(()));
    }

    /// An install at the top level lies above every other install, and an
    /// install below a top-level install lies inside it.
    #[test]
    fn the_top_level_lies_above_every_other_install() {
        assert_eq!(
            refuse_overlap(&placement(&[]), &[root(&["a"])]),
            Err(format!(
                "{}: the install root holds the install at {}; houserules installs do not nest",
                at(&[]),
                at(&["a"])
            ))
        );
        assert_eq!(
            refuse_overlap(&placement(&["a"]), &[root(&[])]),
            Err(format!(
                "{}: the install root lies inside the install at {}; houserules installs do not nest",
                at(&["a"]),
                at(&[])
            ))
        );
    }

    /// Two roots whose paths make one name once each `/` becomes `-` do not
    /// clash: `init` writes no workflow file.
    #[test]
    fn two_roots_that_make_one_dashed_name_do_not_clash() {
        assert_eq!(
            refuse_overlap(&placement(&["b-c"]), &[root(&["b", "c"])]),
            Ok(())
        );
    }

    #[test]
    fn nearest_ancestor_holding_git_names_the_nearest_directory_with_a_git_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("a").join("b");
        fs::create_dir_all(&target).expect("create target");
        fs::create_dir(dir.path().join(".git")).expect("create .git directory");
        assert_eq!(nearest_ancestor_holding_git(&target), Some(dir.path()));
    }

    #[test]
    fn nearest_ancestor_holding_git_prefers_the_nearest_of_two_and_reads_a_git_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let middle = dir.path().join("a");
        let target = middle.join("b");
        fs::create_dir_all(&target).expect("create target");
        fs::create_dir(dir.path().join(".git")).expect("create .git directory");
        fs::write(middle.join(".git"), "gitdir: elsewhere\n").expect("write .git file");
        assert_eq!(
            nearest_ancestor_holding_git(&target),
            Some(middle.as_path())
        );
    }

    #[test]
    fn nearest_ancestor_holding_git_never_names_the_target_itself() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("a");
        fs::create_dir_all(target.join(".git")).expect("create target with .git");
        assert_eq!(nearest_ancestor_holding_git(&target), None);
    }

    #[test]
    fn nearest_ancestor_holding_git_is_none_when_no_ancestor_holds_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        let target = dir.path().join("a").join("b");
        fs::create_dir_all(&target).expect("create target");
        assert_eq!(nearest_ancestor_holding_git(&target), None);
    }
}
