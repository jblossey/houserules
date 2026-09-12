//! Shared helpers for the `houserules` binary's integration tests: a
//! handle on the compiled binary, this checkout's repository
//! root, a portable recursive directory copy (no external `cp -r`, so the
//! Windows leg of the CI matrix behaves the same as Linux and macOS), the
//! generated-file listing the parity tests compare, the frozen sha those
//! same tests check out a worktree at, and a detached git worktree at
//! that sha, always removed on drop.
//!
//! `tests/common/mod.rs` (not `tests/common.rs`) is Cargo's convention for
//! a module shared between integration test binaries without becoming a
//! test binary of its own.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// The frozen sha every parity test's own `FrozenWorktree::checkout` call
/// resolves against. `check_parity.rs` and `render_parity.rs` check out a
/// worktree at this commit because it is the point-in-time content their own
/// goldens were captured against. The commit this constant names must carry no
/// local, machine-specific path in any file `render_all` generates or
/// `check-knowledge` reads: a worktree checked out at a commit that does would
/// leak that path into the checked-in goldens compared against it. Changing
/// this value requires re-running `cargo run --quiet --bin gen-goldens` and
/// reviewing the regenerated goldens: every golden `gen-goldens` captures from
/// a worktree checked out at this sha depends on it. The
/// `mini`/`mini-bad`/`mini-stale` fixture goldens `gen-goldens` captures
/// straight from `tests/fixtures/**` do not.
pub const FROZEN_SHA: &str = "5f14727b4adeeb347a8d1f0c8f98d929f62bc7f4";

/// Serializes `git worktree add`/`remove` across this test binary's
/// threads. `git worktree` mutates shared metadata under
/// `.git/worktrees/`, and the default parallel test runner can start
/// several `FrozenWorktree::checkout` calls at once: without this lock, a
/// concurrent `git worktree add` intermittently fails with `fatal: failed
/// to read .git/worktrees/.../commondir: Success`. Each worktree still
/// gets its own temp path and lives independently once added; only the
/// two git subprocess calls that touch the shared metadata need to run
/// one at a time.
static WORKTREE_LOCK: Mutex<()> = Mutex::new(());

/// A `Command` for the compiled `houserules` binary under test.
pub fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// This checkout's repository root, resolved at compile time from the
/// crate's manifest directory (`crates/houserules`) so it is correct
/// regardless of the test runner's working directory. Canonicalized:
/// `CARGO_MANIFEST_DIR` joined with `../..` keeps those two literal
/// components rather than collapsing them, and the binary's own
/// `resolve_like_node` collapses `..` textually like Node's `path.resolve`
/// does (`validate_deliverable.rs`'s own doc) -- left uncollapsed here,
/// this path would not byte-match what the binary echoes back for an
/// already-absolute argument built from it, breaking every corpus test
/// that redacts this value out of the binary's own output before
/// comparing.
pub fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Copies every file under `src` into `dst`, creating directories as needed.
pub fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create destination directory");
    for entry in fs::read_dir(src).expect("read source directory") {
        let entry = entry.expect("read directory entry");
        let dest_path = dst.join(entry.file_name());
        let file_type = entry.file_type().expect("read file type");
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path);
        } else {
            fs::copy(entry.path(), &dest_path).expect("copy file");
        }
    }
}

/// Every file `render_all` can produce under `root`: every
/// `.claude/rules/*.md` file, sorted, then the knowledge skill. Applying
/// this same order to both the golden directory and a freshly rendered
/// one makes the two `list_generated_files` calls comparable. The skill
/// path is checked against disk, not assumed -- listing every file under
/// `.claude/skills/project-knowledge` (not just asserting `SKILL.md`'s
/// existence) also catches a stray file render did not produce, the
/// exact file set for the one file that carries the whole retrieval
/// protocol. A caller comparing two `list_generated_files` results still
/// gets a clear mismatch instead of a later, unrelated `fs::read` panic.
///
/// Used by `render_parity.rs` only: `tests/common/mod.rs` compiles fresh
/// into every integration-test binary that declares `mod common;` (Cargo's
/// convention for a module shared without becoming its own test binary),
/// so a helper only some binaries call reads as dead code from the ones
/// that do not -- `check_parity.rs` is the second such binary and needs
/// the module's other four helpers, not this one.
#[allow(dead_code)]
pub fn list_generated_files(root: &Path) -> Vec<String> {
    let rules_dir = root.join(".claude/rules");
    let mut files: Vec<String> = fs::read_dir(&rules_dir)
        .expect("read .claude/rules")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().into_string().expect("utf8 filename"))
        .filter(|name| name.ends_with(".md"))
        .map(|name| format!(".claude/rules/{name}"))
        .collect();
    files.sort();

    let skill_dir = root.join(".claude/skills/project-knowledge");
    let mut skill_files: Vec<String> = fs::read_dir(&skill_dir)
        .unwrap_or_else(|error| {
            panic!("read {}: {error}", skill_dir.display());
        })
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().into_string().expect("utf8 filename"))
        .map(|name| format!(".claude/skills/project-knowledge/{name}"))
        .collect();
    skill_files.sort();
    files.extend(skill_files);
    files
}

/// A detached git worktree at a frozen sha, created under a fresh temp
/// path and removed with `git worktree remove --force` on drop — a Rust
/// `Drop` is this test's finally block, so a panicking assertion still
/// cleans up.
pub struct FrozenWorktree {
    repo_root: PathBuf,
    pub path: PathBuf,
}

impl FrozenWorktree {
    /// Checks out `sha` as a new detached worktree of the repository at
    /// `repo_root`. `git worktree add` refuses an existing path, so this
    /// mints a temp directory and immediately removes it before checkout.
    ///
    /// `-c core.autocrlf=false -c core.eol=lf` pin the checkout to LF
    /// bytes regardless of the runner's ambient git config. The frozen
    /// sha's own `.gitattributes` already marks `.claude/rules/**` and
    /// `.claude/skills/project-knowledge/**` `-text`, which disables
    /// translation on its own; these two flags guard a future re-pin
    /// whose tree might lack those rules -- without them, a windows
    /// runner's `core.autocrlf=true` (Git for Windows' common default,
    /// the same as setting `text=auto` on every file plus
    /// `core.eol=crlf`, per `git help config`) would CRLF-translate the
    /// checked-out `.claude/rules/*.md` and the skill on checkout, so
    /// `check-knowledge`'s fresh LF render would disagree with the CRLF
    /// bytes already on disk -- a false "generated file is out of date"
    /// finding, exit 1, empty stdout where the frozen corpus records
    /// `knowledge: ok\n`, exit 0. Command-line `-c` overrides outrank
    /// every config file (`git help git`), so these two win over the
    /// runner's system/global setting however it was set, and `git
    /// worktree add`'s own checkout (`git help worktree`: "Create a
    /// worktree ... and checkout <commit-ish> into it") is an ordinary
    /// checkout, subject to the same config.
    pub fn checkout(repo_root: &Path, sha: &str) -> Self {
        let holder = tempfile::tempdir().expect("tempdir");
        let path = holder.path().to_path_buf();
        drop(holder);
        let status = {
            let _guard = WORKTREE_LOCK
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            Command::new("git")
                .args([
                    "-c",
                    "core.autocrlf=false",
                    "-c",
                    "core.eol=lf",
                    "worktree",
                    "add",
                    "--detach",
                    "--quiet",
                ])
                .arg(&path)
                .arg(sha)
                .current_dir(repo_root)
                .status()
                .expect("run git worktree add")
        };
        assert!(status.success(), "git worktree add {sha} failed");
        FrozenWorktree {
            repo_root: repo_root.to_path_buf(),
            path,
        }
    }
}

impl Drop for FrozenWorktree {
    /// Removes the worktree. A `Drop` must not panic while unwinding (a
    /// panicking assertion in the test using this worktree is already
    /// unwinding when this runs), so a failed removal is reported on
    /// stderr, naming the leaked path so a developer can run `git
    /// worktree prune`.
    fn drop(&mut self) {
        let guard = WORKTREE_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let result = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .current_dir(&self.repo_root)
            .status();
        drop(guard);
        match result {
            Ok(status) if status.success() => {}
            Ok(status) => eprintln!(
                "warning: git worktree remove --force {} exited {status}; run `git worktree prune` in {}",
                self.path.display(),
                self.repo_root.display(),
            ),
            Err(error) => eprintln!(
                "warning: could not run git worktree remove --force {}: {error}; run `git worktree prune` in {}",
                self.path.display(),
                self.repo_root.display(),
            ),
        }
    }
}
