//! The OpenSpec root of an install, as `render` and the warning of two planning workflows read it
//! (batch 38, spec 6.1 and 6.2).
//!
//! [`root_of`] tells what the `openspec/` folder of an install is, by the paths that git tracks below
//! the install root, so that a clone and the work tree agree. Where git cannot list the tree, it reads
//! the files of the work tree instead, as the search for `AGENTS.md` files does. The classifier is
//! [`root_kind`] of the `openspec` intent adapter, which reads the same folder at a commit: a folder
//! `openspec/specs/` or `openspec/changes/` with no store metadata is a planning shape. The text of
//! `openspec/config.yaml`, else of `openspec/config.yml`, is read only for a root with no planning
//! shape and only where that path listing holds the file. Bytes of it that are no UTF-8 decode
//! lossily, as the adapter decodes them.
//!
//! [`SECTION`] is the section that the root block of `AGENTS.md` holds while the root has a planning
//! shape. [`two_workflows_line`] is the warning of an install that runs the reference workflow and
//! holds an OpenSpec root.

use std::fs;
use std::io;
use std::path::Path;

use crate::install::marker::read_render_inputs_for_gate;
use crate::review::intent::openspec::{RootKind, root_kind};

use super::super::tracked_files;
use super::RenderError;

/// The section of the root block that names the split between OpenSpec and houserules (spec 6.1).
pub(super) const SECTION: &str = "## OpenSpec

This project plans its changes with OpenSpec: a change's proposal, specs, design, and tasks live in `openspec/changes/<id>/`. Write a design decision into the change's `design.md`, not into a separate spec. houserules supplies the rules and the checks of each change: run `houserules for <path>` before you change a file, and `houserules audit --base <the base branch>` before you archive a change.";

/// The folder of an OpenSpec root, below the install root.
const FOLDER: &str = "openspec";

/// The files that hold the configuration of an OpenSpec root, in the order OpenSpec reads them.
const CONFIGS: [&str; 2] = ["openspec/config.yaml", "openspec/config.yml"];

/// The files below `openspec/` that git tracks below `root`, as `/`-separated paths from `root`.
/// Where git cannot list the tree, the files of the work tree below `openspec/`.
fn openspec_paths(root: &Path) -> Result<Vec<String>, RenderError> {
    match tracked_files(root) {
        Some((tracked, _undecodable)) => Ok(tracked
            .into_iter()
            .filter(|path| path.starts_with("openspec/"))
            .collect()),
        None => {
            let mut found = Vec::new();
            walk(root, FOLDER, &mut found)?;
            Ok(found)
        }
    }
}

/// The files below the directory `relative` of `root`, found by a walk that follows no symbolic link.
/// A directory that is not there holds none. A name that is no UTF-8 is left out, as git's listing
/// leaves out such a path.
fn walk(root: &Path, relative: &str, found: &mut Vec<String>) -> Result<(), RenderError> {
    let directory = relative
        .split('/')
        .fold(root.to_path_buf(), |path, component| path.join(component));
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(RenderError::io(relative, &error)),
    };
    for entry in entries {
        let entry = entry.map_err(|error| RenderError::io(relative, &error))?;
        let raw = entry.file_name();
        let Some(name) = raw.to_str() else {
            continue;
        };
        let path = format!("{relative}/{name}");
        let kind = entry
            .file_type()
            .map_err(|error| RenderError::io(&path, &error))?;
        if kind.is_dir() {
            walk(root, &path, found)?;
        } else {
            found.push(path);
        }
    }
    Ok(())
}

/// The text of the file `path` below `root`, with a replacement character for each byte that is no
/// UTF-8, as the adapter reads a file of a tree. `None` when the file does not exist; an error of
/// the operating system is the one line that names the file.
fn read_lossy(root: &Path, path: &str) -> Result<Option<String>, RenderError> {
    match fs::read(root.join(path)) {
        Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RenderError::io(path, &error)),
    }
}

/// The text of the configuration of the OpenSpec root below `root`: `openspec/config.yaml`, else
/// `openspec/config.yml`, each only when `paths` lists it. A file that is not there holds none; a file
/// that the operating system cannot read is the one line that names it.
fn config_text(root: &Path, paths: &[String]) -> Result<Option<String>, RenderError> {
    for name in CONFIGS {
        if paths.iter().any(|path| path == name)
            && let Some(text) = read_lossy(root, name)?
        {
            return Ok(Some(text));
        }
    }
    Ok(None)
}

/// What the `openspec/` folder of the install at `root` is (spec 6.1): a planning shape, a store
/// pointer, or neither. A configuration that the operating system cannot read, in a root with no
/// planning shape, is one named line.
pub(super) fn root_of(root: &Path) -> Result<RootKind, RenderError> {
    let paths = openspec_paths(root)?;
    if root_kind(&paths, None) == RootKind::Planning {
        return Ok(RootKind::Planning);
    }
    Ok(root_kind(&paths, config_text(root, &paths)?.as_deref()))
}

/// The warning of spec 6.2 for the binary of version `version`.
fn warning_line(version: &str) -> String {
    format!(
        "houserules: warning: this install runs the reference workflow and the project plans with OpenSpec (openspec/); plan changes with one of them. Beside OpenSpec, houserules needs no reference workflow: https://github.com/jblossey/houserules/blob/v{version}/docs/openspec.md"
    )
}

/// The warning of spec 6.2 for the install at `root`, or `None` when the install does not run the
/// reference workflow (no `workflow` key in `.houserules.json`) or its tree holds no OpenSpec root
/// (a planning shape or a store pointer). A marker or a configuration that cannot be read gives
/// `None` too: `render` and `check-knowledge` report that fault in their own line.
pub(crate) fn two_workflows_line(root: &Path) -> Option<String> {
    if !read_render_inputs_for_gate(root).ok()?.workflow {
        return None;
    }
    match root_of(root).ok()? {
        RootKind::Absent => None,
        RootKind::Planning | RootKind::StorePointer => {
            Some(warning_line(env!("CARGO_PKG_VERSION")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::git::scratch::{assert_no_line_ending_warning_under_autocrlf, fixture_git};
    use crate::rules::git::command;

    /// A fresh scratch directory with `git init` already run.
    fn scratch_git_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = command(dir.path())
            .args(["init", "-q"])
            .status()
            .expect("run git init");
        assert!(status.success(), "git init failed");
        dir
    }

    /// Writes `relative` (`/`-separated) below `root`, creating the folders above it.
    fn write(root: &Path, relative: &str, content: &[u8]) {
        let path = relative
            .split('/')
            .fold(root.to_path_buf(), |path, component| path.join(component));
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the folder");
        fs::write(path, content).expect("write the file");
    }

    /// Stages `relative` below `root`.
    fn track(root: &Path, relative: &str) {
        let staged = fixture_git(root, &["add", relative], &[]);
        assert!(staged.status.success(), "git add failed");
    }

    /// The warning is the line of spec 6.2 with the version of the binary in the link;
    /// `tests/openspec_coexistence.rs` shows that the spec holds this text.
    #[test]
    fn the_warning_names_the_version_of_the_binary() {
        assert_eq!(
            warning_line("2.0.0"),
            "houserules: warning: this install runs the reference workflow and the project plans with OpenSpec (openspec/); plan changes with one of them. Beside OpenSpec, houserules needs no reference workflow: https://github.com/jblossey/houserules/blob/v2.0.0/docs/openspec.md"
        );
    }

    /// Git tracks the planning shape: an untracked file draws none, a staged one does, and a
    /// config with a `store:` key is a pointer only while no tracked file gives a planning shape.
    #[test]
    fn the_root_follows_the_paths_that_git_tracks() {
        let repo = scratch_git_repo();
        assert_eq!(root_of(repo.path()).expect("a root"), RootKind::Absent);

        write(repo.path(), "openspec/specs/a/spec.md", b"text\n");
        assert_eq!(root_of(repo.path()).expect("a root"), RootKind::Absent);
        track(repo.path(), "openspec/specs/a/spec.md");
        assert_eq!(root_of(repo.path()).expect("a root"), RootKind::Planning);

        let pointer = scratch_git_repo();
        write(pointer.path(), "openspec/config.yaml", b"store: team\n");
        assert_eq!(root_of(pointer.path()).expect("a root"), RootKind::Absent);
        track(pointer.path(), "openspec/config.yaml");
        assert_eq!(
            root_of(pointer.path()).expect("a root"),
            RootKind::StorePointer
        );
        write(pointer.path(), "openspec/changes/c/proposal.md", b"text\n");
        track(pointer.path(), "openspec/changes/c/proposal.md");
        assert_eq!(root_of(pointer.path()).expect("a root"), RootKind::Planning);
    }

    /// Where git cannot list the tree, the work tree is read: every file below `openspec/` counts,
    /// a store folder is no planning shape, and a config is read where it lies.
    #[test]
    fn without_git_the_root_follows_the_work_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(root_of(dir.path()).expect("a root"), RootKind::Absent);

        write(dir.path(), "openspec/config.yml", b"store: team\n");
        assert_eq!(root_of(dir.path()).expect("a root"), RootKind::StorePointer);

        write(dir.path(), "openspec/specs/a/spec.md", b"text\n");
        assert_eq!(root_of(dir.path()).expect("a root"), RootKind::Planning);

        write(
            dir.path(),
            "openspec/specs/.openspec-store/store.yaml",
            b"name: team\n",
        );
        assert_eq!(root_of(dir.path()).expect("a root"), RootKind::StorePointer);
    }

    /// A configuration with bytes that are no UTF-8 is read lossily, as the adapter reads it from a
    /// tree: its `store:` key still makes a store pointer, and one with no such key holds none.
    #[test]
    fn a_configuration_that_is_no_utf8_is_read_lossily() {
        let pointer = scratch_git_repo();
        write(pointer.path(), "openspec/config.yaml", b"store: \xff\xfe\n");
        track(pointer.path(), "openspec/config.yaml");
        assert_eq!(
            root_of(pointer.path()).expect("a root"),
            RootKind::StorePointer
        );
        write(
            pointer.path(),
            ".houserules.json",
            b"{\"workflow\": {\"name\": \"reference\"}}\n",
        );
        assert_eq!(
            two_workflows_line(pointer.path()),
            Some(warning_line(env!("CARGO_PKG_VERSION")))
        );

        let plain = scratch_git_repo();
        write(plain.path(), "openspec/config.yaml", b"schema: \xff\n");
        track(plain.path(), "openspec/config.yaml");
        assert_eq!(root_of(plain.path()).expect("a root"), RootKind::Absent);
    }

    /// A tracked configuration that the operating system cannot read is one line that names the
    /// file, and only a root with no planning shape reads it. A folder in place of the file fails on
    /// every platform.
    #[test]
    fn a_configuration_that_the_operating_system_cannot_read_is_one_named_line() {
        let repo = scratch_git_repo();
        write(repo.path(), "openspec/config.yaml", b"store: team\n");
        track(repo.path(), "openspec/config.yaml");
        let config = repo.path().join("openspec").join("config.yaml");
        fs::remove_file(&config).expect("remove the file");
        fs::create_dir(&config).expect("create a folder in its place");

        let error = root_of(repo.path()).expect_err("a fault");
        assert!(
            error.to_string().starts_with("openspec/config.yaml: "),
            "{error}"
        );
        assert_eq!(two_workflows_line(repo.path()), None);

        write(repo.path(), "openspec/specs/a/spec.md", b"text\n");
        track(repo.path(), "openspec/specs/a/spec.md");
        assert_eq!(root_of(repo.path()).expect("a root"), RootKind::Planning);
    }

    /// The warning needs both the `workflow` key of the marker and an OpenSpec root.
    #[test]
    fn the_warning_needs_the_workflow_key_and_an_openspec_root() {
        let repo = scratch_git_repo();
        write(repo.path(), "openspec/specs/a/spec.md", b"text\n");
        track(repo.path(), "openspec/specs/a/spec.md");
        assert_eq!(two_workflows_line(repo.path()), None);

        write(
            repo.path(),
            ".houserules.json",
            b"{\"workflow\": {\"name\": \"reference\"}}\n",
        );
        assert_eq!(
            two_workflows_line(repo.path()),
            Some(warning_line(env!("CARGO_PKG_VERSION")))
        );

        let without_root = scratch_git_repo();
        write(
            without_root.path(),
            ".houserules.json",
            b"{\"workflow\": {\"name\": \"reference\"}}\n",
        );
        assert_eq!(two_workflows_line(without_root.path()), None);
    }

    /// The fixture of the test that stages OpenSpec files with `git add` prints no line-ending
    /// warning under the autocrlf suite, which injects `core.autocrlf=true`.
    #[test]
    fn staging_the_fixture_prints_no_line_ending_warning_under_autocrlf() {
        assert_no_line_ending_warning_under_autocrlf(&[
            "rules::render::openspec::tests::the_root_follows_the_paths_that_git_tracks",
        ]);
    }
}
