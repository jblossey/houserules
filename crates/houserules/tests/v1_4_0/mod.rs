//! A real 1.4.0 install, for the tests that exercise what `update` converts.
//!
//! The helper builds the `houserules` binary of the `v1.4.0` release tag once
//! and keeps it under `CARGO_TARGET_TMPDIR`, the directory that Cargo gives
//! integration tests inside the target directory: a second test run reuses
//! the binary, and `cargo clean` removes it. Each build runs in its own
//! directory and moves the finished binary into place only when no binary
//! is there yet, so two test processes never share a half-built file. The
//! source is a detached `git worktree` of the tag, removed after the build
//! whether it succeeds or fails.
//!
//! [`seed`] then runs that binary's `init` in a scratch git repository. The
//! tree is what a real 1.4.0 user has: the kit's five topic files with one
//! baseline for each entry, no packs, and a `.houserules.json` with no
//! `schema` key. Every test file that needs the shape includes this module,
//! as `tests/workflow_reader/mod.rs` is shared, and keeps its own other
//! helpers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The release tag whose binary seeds the fixtures.
const TAG: &str = "v1.4.0";

/// The path of the cached binary, once the first caller has built it.
static BINARY: OnceLock<PathBuf> = OnceLock::new();

/// This checkout's repository root, built from the manifest directory.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The directory that holds the cached binary and the build directories.
fn cache_dir() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("houserules-v1.4.0")
}

/// The file name of a `houserules` executable on this platform.
fn executable() -> &'static str {
    if cfg!(windows) {
        "houserules.exe"
    } else {
        "houserules"
    }
}

/// Runs `command`, which must succeed; a failure panics with `what` and the
/// output of the command.
fn run_checked(command: &mut Command, what: &str) {
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("{what}: {error}"));
    assert!(
        output.status.success(),
        "{what} failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A build directory with the worktree of the tag in it. Dropping it removes
/// the worktree and the directory, so a failed build leaves nothing behind
/// but the report of the failure. A removal that fails prints a warning, as
/// `common::FrozenWorktree` does: a `Drop` must not panic.
struct BuildDir {
    work: PathBuf,
    source: PathBuf,
}

impl Drop for BuildDir {
    fn drop(&mut self) {
        let removed = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.source)
            .current_dir(repo_root())
            .output();
        match removed {
            Ok(output) if output.status.success() => {}
            _ => eprintln!(
                "warning: could not remove the worktree {}; run `git worktree prune`",
                self.source.display()
            ),
        }
        if let Err(error) = fs::remove_dir_all(&self.work) {
            eprintln!("warning: could not remove {}: {error}", self.work.display());
        }
    }
}

/// Builds the binary of the tag in a directory of its own and moves it to
/// `cached` unless a binary is there already.
fn build(cached: &Path) {
    let work = cache_dir().join(format!("build-{}", std::process::id()));
    fs::create_dir_all(&work).expect("create the build directory");
    let directory = BuildDir {
        source: work.join("source"),
        work,
    };
    run_checked(
        Command::new("git")
            .args(["worktree", "add", "--detach", "--quiet"])
            .arg(&directory.source)
            .arg(TAG)
            .current_dir(repo_root()),
        &format!("git worktree add {TAG}"),
    );
    let target = directory.work.join("target");
    run_checked(
        Command::new(env!("CARGO"))
            .args(["build", "--locked", "--quiet", "--bin", "houserules"])
            .arg("--target-dir")
            .arg(&target)
            .current_dir(&directory.source),
        &format!("cargo build of {TAG}"),
    );
    let built = target.join("debug").join(executable());
    let staged = directory.work.join(executable());
    fs::copy(&built, &staged).expect("copy the built binary");
    if !cached.is_file() {
        fs::rename(&staged, cached).expect("move the binary into the cache");
    }
}

/// The path of the 1.4.0 binary. The first call builds it when the cache
/// holds none.
fn binary() -> &'static Path {
    BINARY.get_or_init(|| {
        let cached = cache_dir().join(executable());
        if !cached.is_file() {
            build(&cached);
        }
        cached
    })
}

/// Seeds the git repository `dir` as a 1.4.0 `init` does, with the backlog
/// id prefix `id_prefix` when it is `Some`. A test that wants the CI
/// workflow adds a GitHub `origin` to `dir` before it calls this function.
pub fn seed(dir: &Path, id_prefix: Option<&str>) {
    let mut init = Command::new(binary());
    init.env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .arg("init")
        .arg("--dir")
        .arg(dir);
    if let Some(prefix) = id_prefix {
        init.args(["--id-prefix", prefix]);
    }
    run_checked(&mut init, "the 1.4.0 init");
}
