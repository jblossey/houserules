//! Which paths git ignores in the working tree of an install (batch 35, spec 7
//! and 8.6). `check-knowledge` asks about the `verify` paths of its entries and
//! about the default location of the workspaces; `check-report-claims` asks
//! about the paths that a spec or an item cites. One question serves all three:
//! [`ignored_among`].
//!
//! git refuses a path beyond a symbolic link, and its refusal ends the whole
//! call. The question therefore never reaches git with such a path. A path
//! through a symlink is judged at the target of the link when the target lies
//! inside the install root, and at the link itself when it does not: a
//! workspace that the worktrees share lies outside the root, and its link is
//! the ignored entry.
//!
//! git refuses a path inside a submodule in the same way. Such a path belongs
//! to the repository of the submodule and no ignore line of this one covers it,
//! so it is no member of the answer, and the other paths keep their verdicts.
//! Any other failure of git is a named fault ([`IgnoreFault`]): the question
//! gives no verdict then, and the caller says so.

use std::collections::{BTreeSet, VecDeque};
use std::fmt;
use std::io::Write;
use std::path::{Component, Path};
use std::process::{Command, Stdio};

use super::render::{LocateError, git_location_in};

/// The mode that the index records for a submodule.
const SUBMODULE_MODE: &str = "160000";

/// Why [`ignored_among`] gave no answer. Each variant holds the one line that
/// names the fault, with the text of git.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IgnoreFault {
    /// The directory lies outside a git work tree: git has no ignore verdict
    /// there.
    NotAWorkTree(String),
    /// git could not run, or it failed inside a work tree.
    Git(String),
}

impl fmt::Display for IgnoreFault {
    /// The one line of the fault.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (Self::NotAWorkTree(line) | Self::Git(line)) = self;
        formatter.write_str(line)
    }
}

/// The members of `paths` that git ignores in the working tree at `root`. Each
/// path is relative to `root`, `/`-separated, and may end in `/` to ask about
/// a directory: the usual ignore line `dir/` matches a directory only, and git
/// cannot tell that an absent path is one. A path that git cannot be asked
/// about is no member and takes no other verdict with it: an empty path, an
/// absolute path, a path that climbs out of `root` with `..`, and a path inside
/// a submodule (the submodule itself can be asked about). A tracked file is no
/// member, whatever an ignore line says. A directory outside a git work tree
/// is [`IgnoreFault::NotAWorkTree`], and any other failure to run git, or a git
/// that fails, is [`IgnoreFault::Git`].
pub(crate) fn ignored_among(root: &Path, paths: &[&str]) -> Result<BTreeSet<String>, IgnoreFault> {
    let real_root = root.canonicalize().ok();
    let asked: Vec<(&str, String)> = paths
        .iter()
        .filter_map(|path| {
            git_subject(root, real_root.as_deref(), path).map(|subject| (*path, subject))
        })
        .collect();
    if asked.is_empty() {
        return Ok(BTreeSet::new());
    }
    let subjects: Vec<&str> = asked.iter().map(|(_, subject)| subject.as_str()).collect();
    let ignored = match check_ignore(root, &subjects) {
        Ok(ignored) => ignored,
        Err(failure) => ignored_after_a_failure(root, &subjects, failure)?,
    };
    Ok(asked
        .into_iter()
        .filter(|(_, subject)| ignored.contains(subject))
        .map(|(path, _)| path.to_string())
        .collect())
}

/// The members of `subjects` that git ignores, after `check_ignore` failed
/// with the line `failure`. A directory outside a work tree is the fault
/// [`IgnoreFault::NotAWorkTree`]. A subject inside a submodule is the one cause
/// that this recovers from: the subject is no member, and the others are asked
/// again without it. Any other cause keeps `failure` as [`IgnoreFault::Git`].
fn ignored_after_a_failure(
    root: &Path,
    subjects: &[&str],
    failure: String,
) -> Result<BTreeSet<String>, IgnoreFault> {
    if matches!(git_location_in(root), Err(LocateError::NotInRepository(_))) {
        return Err(IgnoreFault::NotAWorkTree(failure));
    }
    let Some(submodules) = submodule_paths(root) else {
        return Err(IgnoreFault::Git(failure));
    };
    let beyond_submodules: Vec<&str> = subjects
        .iter()
        .copied()
        .filter(|subject| !is_inside_a_submodule(&submodules, subject))
        .collect();
    if beyond_submodules.len() == subjects.len() {
        return Err(IgnoreFault::Git(failure));
    }
    if beyond_submodules.is_empty() {
        return Ok(BTreeSet::new());
    }
    check_ignore(root, &beyond_submodules).map_err(IgnoreFault::Git)
}

/// The paths of the submodules in the index of the work tree at `root`, as
/// `/`-separated paths below `root`; `None` when git cannot list the index.
fn submodule_paths(root: &Path) -> Option<Vec<String>> {
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

/// `true` when `subject` names a path below one of `submodules`. The submodule
/// itself, with or without a trailing `/`, is an ordinary path to git.
fn is_inside_a_submodule(submodules: &[String], subject: &str) -> bool {
    submodules.iter().any(|submodule| {
        subject
            .strip_prefix(submodule.as_str())
            .and_then(|below| below.strip_prefix('/'))
            .is_some_and(|below| !below.is_empty())
    })
}

/// The path that git is asked about for `path`, or `None` when git cannot be
/// asked: the root itself, for one. The module doc has the rule for a path
/// through a symlink, which holds for a directory ask of the link (`link/`) too.
fn git_subject(root: &Path, real_root: Option<&Path>, path: &str) -> Option<String> {
    if path.contains('\0') {
        return None;
    }
    let mut pending = VecDeque::new();
    for part in Path::new(path).components() {
        match part {
            Component::Normal(name) => pending.push_back(name.to_str()?.to_string()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    if pending.is_empty() {
        return None;
    }
    let mut resolved: Vec<String> = Vec::new();
    let mut whole = true;
    let directory_ask = path.ends_with('/');
    while let Some(part) = pending.pop_front() {
        let candidate = resolved
            .iter()
            .fold(root.to_path_buf(), |parent, name| parent.join(name))
            .join(&part);
        // git refuses `link/` as it refuses `link/x`: both lie beyond the link.
        let beyond_a_symlink = (!pending.is_empty() || directory_ask)
            && candidate
                .symlink_metadata()
                .is_ok_and(|metadata| metadata.file_type().is_symlink());
        if !beyond_a_symlink {
            resolved.push(part);
            continue;
        }
        match components_below(real_root, &candidate) {
            Some(target) => resolved = target,
            None => {
                resolved.push(part);
                whole = false;
                break;
            }
        }
    }
    if resolved.is_empty() {
        return None;
    }
    let mut subject = resolved.join("/");
    if whole && directory_ask {
        subject.push('/');
    }
    Some(subject)
}

/// The names of the path that the symlink `link` resolves to, below
/// `real_root`, or `None` when the link has no target, leaves the root, or
/// resolves to a name that is no UTF-8.
fn components_below(real_root: Option<&Path>, link: &Path) -> Option<Vec<String>> {
    let target = link.canonicalize().ok()?;
    target
        .strip_prefix(real_root?)
        .ok()?
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str().map(str::to_string),
            _ => None,
        })
        .collect()
}

/// The members of `subjects` that `git check-ignore` reports at `root`, or the
/// one line that names the failure to run git or its failure.
fn check_ignore(root: &Path, subjects: &[&str]) -> Result<BTreeSet<String>, String> {
    let mut child = Command::new("git")
        .args(["check-ignore", "-z", "--stdin"])
        .current_dir(root)
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("git check-ignore could not run: {error}"))?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let input = subjects.join("\0") + "\0";
    // Git answers while it reads, so a writer thread keeps a long list from
    // filling both pipes at once. A write that fails means git has already
    // stopped; its exit status below names the failure.
    let output = std::thread::scope(|scope| {
        scope.spawn(move || stdin.write_all(input.as_bytes()));
        child.wait_with_output()
    })
    .map_err(|error| format!("git check-ignore could not run: {error}"))?;
    // Exit 0: some path is ignored. Exit 1: none is. Anything else is a fault.
    if !matches!(output.status.code(), Some(0 | 1)) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let headline = stderr
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
            .map_or_else(|| output.status.to_string(), str::to_string);
        return Err(format!("git check-ignore failed: {headline}"));
    }
    Ok(output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| String::from_utf8_lossy(entry).into_owned())
        .collect())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use super::*;

    /// Runs `git <args>` at `dir` and requires success.
    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t.t"])
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// A repository whose `.gitignore` holds `ignore`.
    fn repository(ignore: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join(".gitignore"), ignore).expect("write .gitignore");
        dir
    }

    /// The members of `asked` that `ignored_among` reports at `dir`.
    fn ignored(dir: &Path, asked: &[&str]) -> Vec<String> {
        ignored_among(dir, asked)
            .expect("git answers")
            .into_iter()
            .collect()
    }

    /// A path that git cannot be asked about is no member, and git does not
    /// run: a directory that is no repository is no error then.
    #[test]
    fn a_path_that_git_cannot_be_asked_about_is_no_member_and_starts_no_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        let absolute = dir.path().join("x.txt");
        let absolute = absolute.to_str().expect("a UTF-8 path");
        for path in [
            "",
            ".",
            "./",
            "/abs/x.txt",
            "../x.txt",
            "a/../x.txt",
            absolute,
        ] {
            assert_eq!(
                ignored_among(dir.path(), &[path]),
                Ok(BTreeSet::new()),
                "{path:?}"
            );
        }
    }

    /// A directory outside a git work tree is the fault `NotAWorkTree`, with
    /// the text of git, never a clean answer.
    #[test]
    fn a_directory_outside_a_work_tree_is_the_fault_not_a_work_tree() {
        let dir = tempfile::tempdir().expect("tempdir");

        let fault = ignored_among(dir.path(), &["a/b.txt"]).expect_err("git fails");

        let IgnoreFault::NotAWorkTree(line) = fault else {
            panic!("expected NotAWorkTree, got {fault:?}");
        };
        assert!(
            line.starts_with("git check-ignore failed: fatal: "),
            "{line}"
        );
    }

    /// A failure of git inside a work tree, with no path inside a submodule to
    /// blame, is the fault `Git` with the first line of git.
    #[test]
    fn a_git_failure_inside_a_work_tree_is_the_fault_git() {
        let dir = repository("ws/\n");
        fs::write(dir.path().join(".git/index"), "no index\n").expect("overwrite the index");

        let fault = ignored_among(dir.path(), &["ws/a.txt"]).expect_err("git fails");

        let IgnoreFault::Git(line) = fault else {
            panic!("expected Git, got {fault:?}");
        };
        assert!(
            line.starts_with("git check-ignore failed: fatal: ") && !line.contains('\n'),
            "{line}"
        );
    }

    /// An ignored path is a member, in the spelling that the caller gave. A
    /// path that no line ignores, and a tracked file that a line matches, are
    /// not. A trailing `/` asks about a directory that need not exist.
    #[test]
    fn an_ignored_path_is_a_member_and_a_tracked_or_unlisted_one_is_not() {
        let dir = repository("ws/\n*.log\n");
        fs::write(dir.path().join("kept.log"), "log\n").expect("write kept.log");
        git(dir.path(), &["add", "-f", "kept.log"]);

        let found = ignored(
            dir.path(),
            &[
                "ws/x.txt",
                "./ws/y.txt",
                "ws/",
                "ws",
                "src/a.rs",
                "a.log",
                "kept.log",
            ],
        );

        assert_eq!(found, ["./ws/y.txt", "a.log", "ws/", "ws/x.txt"]);
    }

    /// A path inside a submodule is a path that git cannot be asked about:
    /// `git check-ignore` refuses it and ends the whole call. It is no member,
    /// and it takes the verdict of no other path with it, in either order. The
    /// submodule itself is an ordinary path.
    #[test]
    fn a_path_inside_a_submodule_is_no_member_and_hides_no_other_verdict() {
        let upstream = repository("");
        fs::write(upstream.path().join("s.txt"), "s\n").expect("write s.txt");
        git(upstream.path(), &["add", "s.txt"]);
        git(upstream.path(), &["commit", "-q", "-m", "s"]);
        let dir = repository("ws/\nvendor/ignored.txt\n");
        git(
            dir.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                upstream.path().to_str().expect("a UTF-8 path"),
                "vendor",
            ],
        );

        for asked in [["ws/a.txt", "vendor/s.txt"], ["vendor/s.txt", "ws/a.txt"]] {
            assert_eq!(ignored(dir.path(), &asked), ["ws/a.txt"], "{asked:?}");
        }
        assert_eq!(
            ignored(
                dir.path(),
                &["vendor", "vendor/", "vendor/s.txt", "vendor/sub/"]
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            ignored(dir.path(), &["vendor/s.txt"]),
            Vec::<String>::new(),
            "a submodule path alone"
        );
    }

    /// An install in a subdirectory of the repository asks about paths below its
    /// own root: the submodule is found by its path below that root.
    #[test]
    fn a_submodule_below_an_install_in_a_subdirectory_is_found_by_its_path_below_the_root() {
        let upstream = repository("");
        fs::write(upstream.path().join("s.txt"), "s\n").expect("write s.txt");
        git(upstream.path(), &["add", "s.txt"]);
        git(upstream.path(), &["commit", "-q", "-m", "s"]);
        let dir = repository("ws/\n");
        let install = dir.path().join("inst");
        fs::create_dir(&install).expect("create inst/");
        git(
            dir.path(),
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "-q",
                upstream.path().to_str().expect("a UTF-8 path"),
                "inst/vendor",
            ],
        );

        let found = ignored(&install, &["vendor/s.txt", "ws/a.txt"]);

        assert_eq!(found, ["ws/a.txt"]);
    }

    /// A symlink is a platform feature of Unix here
    /// (`houserules.platform-gated-tests`).
    #[cfg(unix)]
    mod symlinks {
        use std::os::unix::fs::symlink;

        use super::*;

        /// A repository that ignores `ws/`, with the symlink `link` at the
        /// root to `target`.
        fn linked(target: &Path) -> tempfile::TempDir {
            let dir = repository("ws/\n");
            symlink(target, dir.path().join("link")).expect("create the symlink");
            dir
        }

        /// A symlink to a folder outside the root is judged at the link: the
        /// link is a path that git can name, the paths beyond it are not.
        #[test]
        fn a_path_through_a_symlink_that_leaves_the_root_is_judged_at_the_link() {
            let outside = tempfile::tempdir().expect("tempdir");
            let dir = repository("ws/\n");
            fs::create_dir(dir.path().join("ws")).expect("create ws/");
            symlink(outside.path(), dir.path().join("ws/shared")).expect("create the symlink");
            symlink(outside.path(), dir.path().join("open")).expect("create the symlink");

            let found = ignored(dir.path(), &["ws/shared/a/b.txt", "open/a/b.txt"]);

            assert_eq!(found, ["ws/shared/a/b.txt"]);
        }

        /// A symlink whose target is gone is judged at the link too.
        #[test]
        fn a_path_through_a_dangling_symlink_is_judged_at_the_link() {
            let gone = tempfile::tempdir().expect("tempdir");
            let dir = repository("ws/\n");
            fs::create_dir(dir.path().join("ws")).expect("create ws/");
            symlink(gone.path().join("removed"), dir.path().join("ws/dangling"))
                .expect("create the symlink");

            let found = ignored(dir.path(), &["ws/dangling/a.txt"]);

            assert_eq!(found, ["ws/dangling/a.txt"]);
        }

        /// A symlink to a folder inside the root is judged at its target: the
        /// link itself is not ignored, and the target is.
        #[test]
        fn a_path_through_a_symlink_inside_the_root_is_judged_at_its_target() {
            let dir = repository("ws/\n");
            fs::create_dir(dir.path().join("ws")).expect("create ws/");
            fs::create_dir(dir.path().join("src")).expect("create src/");
            let ws = dir.path().join("ws");
            symlink(&ws, dir.path().join("to-ws")).expect("create the symlink");
            symlink(dir.path().join("src"), dir.path().join("to-src")).expect("create the symlink");
            symlink(dir.path().join("to-ws"), dir.path().join("twice"))
                .expect("create the symlink");

            let found = ignored(
                dir.path(),
                &["to-ws/a.txt", "to-src/a.txt", "twice/sub/a.txt"],
            );

            assert_eq!(found, ["to-ws/a.txt", "twice/sub/a.txt"]);
        }

        /// A directory ask (a trailing `/`) of a symlink is judged like the path
        /// beyond the link, because git refuses `link/` as beyond a symlink: at
        /// the target when the target lies inside the root, at the link itself
        /// (asked without the `/`) when it does not or when it is gone.
        #[test]
        fn a_directory_ask_of_a_symlink_is_judged_like_the_path_beyond_it() {
            let outside = tempfile::tempdir().expect("tempdir");
            let gone = tempfile::tempdir().expect("tempdir");
            let dir = repository("ws/\n");
            fs::create_dir(dir.path().join("ws")).expect("create ws/");
            fs::create_dir(dir.path().join("src")).expect("create src/");
            symlink(outside.path(), dir.path().join("ws/shared")).expect("create the symlink");
            symlink(gone.path().join("removed"), dir.path().join("ws/dangling"))
                .expect("create the symlink");
            symlink(dir.path().join("ws"), dir.path().join("to-ws")).expect("create the symlink");
            symlink(dir.path().join("src"), dir.path().join("to-src")).expect("create the symlink");
            symlink(outside.path(), dir.path().join("open")).expect("create the symlink");

            let found = ignored(
                dir.path(),
                &["ws/shared/", "ws/dangling/", "to-ws/", "to-src/", "open/"],
            );

            assert_eq!(found, ["to-ws/", "ws/dangling/", "ws/shared/"]);
        }

        /// A symlink as the last component is the entry that git names.
        #[test]
        fn a_symlink_as_the_last_component_is_judged_as_the_entry() {
            let outside = tempfile::tempdir().expect("tempdir");
            let dir = linked(outside.path());
            fs::write(dir.path().join(".gitignore"), "link\n").expect("write .gitignore");

            let found = ignored(dir.path(), &["link"]);

            assert_eq!(found, ["link"]);
        }
    }
}
