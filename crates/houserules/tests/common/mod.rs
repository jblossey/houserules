//! Shared helpers for the `houserules` binary's integration tests (HR-054
//! task 3): a handle on the compiled binary, this checkout's repository
//! root, a portable recursive directory copy (no external `cp -r`, so the
//! Windows leg of the CI matrix behaves the same as Linux and macOS), the
//! generated-file listing the corpus parity tests compare, and a detached
//! git worktree at a frozen sha, always removed on drop.
//!
//! `tests/common/mod.rs` (not `tests/common.rs`) is Cargo's convention for
//! a module shared between integration test binaries without becoming a
//! test binary of its own.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// Serializes `git worktree add`/`remove` across this test binary's
/// threads. `git worktree` mutates shared metadata under `.git/worktrees/`,
/// and the default parallel test runner can start several
/// `FrozenWorktree::checkout` calls at once (batch 17 T2's
/// `backlog_parity.rs` alone mints one per corpus slice and per
/// worktree-backed error-arm test): without this lock, a concurrent `git
/// worktree add` intermittently fails outright -- reproduced live by
/// temporarily removing the lock and running `--test-threads=16` eight
/// times (3 of 8 failed; task-2-fix1-worktree-race-without-lock.txt
/// retains one failure, `fatal: failed to read
/// .git/worktrees/.../commondir: Success`), then confirmed the lock as
/// shown here removes the race (five more runs, all green). Each worktree
/// still gets its own temp path and lives independently once added; only
/// the two git subprocess calls that touch the shared metadata need to
/// run one at a time.
static WORKTREE_LOCK: Mutex<()> = Mutex::new(());

/// A `Command` for the compiled `houserules` binary under test.
pub fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// This checkout's repository root, resolved at compile time from the
/// crate's manifest directory (`crates/houserules`) so it is correct
/// regardless of the test runner's working directory.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

/// Every file `render_all` can produce under `root`, in the same order
/// `tools/make-corpus.mjs`'s `renderedPaths` lists them: every
/// `.claude/rules/*.md` file, sorted, then the knowledge skill. Fix round
/// 1, finding 5: the skill path is checked against disk, not assumed --
/// listing every file under `.claude/skills/project-knowledge` (not just
/// asserting `SKILL.md`'s existence) also catches a stray file render did
/// not produce, matching the review's "exact file set" requirement for
/// the one file that carries the whole retrieval protocol. A caller
/// comparing two `list_generated_files` results still gets a clear
/// mismatch instead of a later, unrelated `fs::read` panic.
///
/// Used by `render_parity.rs` only: `tests/common/mod.rs` compiles fresh
/// into every integration-test binary that declares `mod common;` (Cargo's
/// convention for a module shared without becoming its own test binary),
/// so a helper only some binaries call reads as dead code from the ones
/// that do not -- `check_parity.rs` (HR-054 task 4) is the second such
/// binary and needs the module's other four helpers, not this one.
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
    /// mints a temp directory and immediately removes it, the same
    /// mkdtemp-then-remove approach `tools/make-corpus.mjs`'s
    /// `withFrozenWorktree` uses.
    ///
    /// `-c core.autocrlf=false -c core.eol=lf` (CI fix round: the windows
    /// leg of the rust matrix) pin the checkout to the frozen sha's own
    /// LF bytes regardless of the runner's ambient git config. The frozen
    /// sha (a73a8c6) predates this repository's `.gitattributes`, so
    /// nothing there marks its files `text`; a windows runner's
    /// `core.autocrlf=true` (Git for Windows' common default, the same as
    /// setting `text=auto` on every file plus `core.eol=crlf`, per `git
    /// help config`) CRLF-translated the checked-out `.claude/rules/*.md` and
    /// the skill on checkout, so `check-knowledge`'s fresh LF render
    /// disagreed with the CRLF bytes already on disk -- a false
    /// "generated file is out of date" finding, exit 1, empty stdout
    /// where the frozen corpus records `knowledge: ok\n`, exit 0.
    /// Command-line `-c` overrides outrank every config file (`git help
    /// git`), so these two win over the runner's system/global setting
    /// however it was set, and `git worktree add`'s own checkout (`git
    /// help worktree`: "Create a worktree ... and checkout <commit-ish>
    /// into it") is an ordinary checkout, subject to the same config.
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
    /// Removes the worktree. Fix round 1, finding 8: a `Drop` must not
    /// panic while unwinding (a panicking assertion in the test using this
    /// worktree is already unwinding when this runs), so a failed removal
    /// is reported on stderr, naming the leaked path so a developer can
    /// run `git worktree prune`, instead of the prior silent `let _ = ...`
    /// that let a leak accumulate with no signal.
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
