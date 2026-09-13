//! HR-081: attributes a `template/**`-only change to the `crates/houserules`
//! release without any release-please plugin surface. `crates/houserules/
//! payload.stamp` records a sha256 digest of `template/`'s current tracked
//! file set (paths and content); this gate recomputes that digest from the
//! working tree and fails when it disagrees with the recorded one.
//!
//! `template/` is embedded into the shipped `houserules` binary at compile
//! time (`houserules.payload-embeds-checkout-bytes`, `install.rs`'s
//! `Payload`), so any change under it is a real product change, and
//! release-please's own commit-to-package attribution
//! (`CommitSplit.split`, keyed on a commit's changed file paths) only ever
//! counts a commit that touches `crates/houserules/**`. A template-only
//! commit touches nothing there and would cut no release. This gate closes
//! that gap at the working tree: a contributor whose tree holds a
//! `template/` change with no matching `payload.stamp` regeneration fails
//! `mise run lint` (wired into its chain alongside `residue-gate`) before
//! the change can land.
//!
//! That is a TREE-LEVEL guarantee, not a per-commit one. This gate reads
//! only the current working tree, so a branch that SPLITS the `template/`
//! edit and the `payload.stamp` regeneration into two separate commits
//! still passes here at the branch tip, even though `CommitSplit` reads
//! each commit's own changed paths -- a commit that touches only
//! `template/` still cuts no release on its own, whatever a later commit
//! in the same branch repairs. Enforcing "every commit touching
//! `template/` also touches `crates/houserules/payload.stamp`" per commit
//! is `houserules check-commit`'s job instead (a `co-change`-type
//! knowledge check, evaluated per commit over the PR's full range in CI --
//! `check_commit.rs`'s own module doc has that mechanism's account), not
//! this gate's.
//!
//! `--write` recomputes the digest and rewrites `payload.stamp` with it;
//! run with no arguments, the gate only compares and never writes.

use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// `payload.stamp`'s own path, relative to the repository root: inside
/// `crates/houserules/` so `CommitSplit`'s prefix match sees it, never
/// inside `template/` itself so it never ships to adopters.
const STAMP_PATH: &str = "crates/houserules/payload.stamp";

/// The directory this gate watches: the one payload `install.rs` embeds.
const PAYLOAD_DIR: &str = "template";

/// This checkout's repository root, resolved at compile time -- this
/// file's own copy of the pattern every `src/bin/*.rs` file in this crate
/// keeps independently (`residue-gate.rs`'s own doc explains why: a file
/// needing none of another bin's helpers still warns the rest of that
/// module dead if pulled in just for this one function).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Every path `git ls-files -z -- <dir>` names under `root`, repo-relative
/// POSIX paths, sorted. `-z` avoids `core.quotePath`'s escaping of a
/// non-ASCII path (`residue-gate.rs::tracked_files`'s own doc has the full
/// account); this gate has no legitimate reason to skip a tracked payload
/// path for being non-UTF-8, so an undecodable entry is a named panic
/// here rather than a silent drop -- `template/` ships as UTF-8 text and
/// portable binary assets, never a path a git checkout cannot decode.
fn tracked_files_under(root: &Path, dir: &str) -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", dir])
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("run git ls-files -- {dir}: {error}"));
    assert!(
        output.status.success(),
        "git ls-files -- {dir} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut files: Vec<String> = output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            String::from_utf8(entry.to_vec())
                .unwrap_or_else(|_| panic!("tracked path under {dir} is not valid UTF-8"))
        })
        .collect();
    files.sort();
    files
}

/// The sha256 digest of `dir`'s current tracked-file state: every tracked
/// path's name and content, NUL-separated, in sorted path order, hashed
/// together. Sorted order and the NUL separators make the digest
/// independent of `git ls-files`'s own ordering and immune to a
/// path/content boundary collision; any content change, addition, or
/// removal under `dir` changes it.
fn payload_digest(root: &Path, dir: &str) -> String {
    let mut hasher = Sha256::new();
    for path in tracked_files_under(root, dir) {
        let content = fs::read(root.join(&path))
            .unwrap_or_else(|error| panic!("read tracked path {path}: {error}"));
        hasher.update(path.as_bytes());
        hasher.update([0u8]);
        hasher.update(&content);
        hasher.update([0u8]);
    }
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// `payload.stamp`'s own recorded digest, trimmed -- `None` when the file
/// does not exist or is not valid UTF-8, both treated as "no stamp yet"
/// rather than a crash, since a missing stamp is the normal state for a
/// contributor who has not run `--write` yet.
fn read_stamp(root: &Path) -> Option<String> {
    fs::read_to_string(root.join(STAMP_PATH))
        .ok()
        .map(|content| content.trim().to_string())
}

/// Writes `digest` to `payload.stamp`, followed by one trailing newline --
/// the exact byte `read_stamp`'s own `.trim()` strips back off, so a
/// write-then-read round-trips the bare hex digest with no surrounding
/// whitespace on either side.
fn write_stamp(root: &Path, digest: &str) {
    fs::write(root.join(STAMP_PATH), format!("{digest}\n"))
        .unwrap_or_else(|error| panic!("write {STAMP_PATH}: {error}"));
}

/// The gate's three possible verdicts, `main`'s own decision extracted so
/// each is a direct unit test rather than a full process spawn (and a real
/// or seeded `template/`/`payload.stamp` pair) per case.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Verdict {
    /// `payload.stamp`'s recorded digest agrees with `template/`'s current
    /// one: nothing to do.
    Match,
    /// A stamp exists but disagrees with the current digest: `template/`
    /// changed since the stamp was last regenerated.
    Stale { recorded: String, current: String },
    /// No stamp exists yet (or it is unreadable): the normal state before
    /// a payload's first `--write`.
    Missing { current: String },
}

/// Decides `root`'s verdict: `template/`'s current digest, compared
/// against `payload.stamp`'s own recorded one (or its absence). The one
/// decision `main` reports on the non-`--write` path, extracted so
/// `Match`/`Stale`/`Missing` are each a direct unit test instead of a full
/// process spawn per case.
fn decide(root: &Path) -> Verdict {
    let current = payload_digest(root, PAYLOAD_DIR);
    match read_stamp(root) {
        Some(recorded) if recorded == current => Verdict::Match,
        Some(recorded) => Verdict::Stale { recorded, current },
        None => Verdict::Missing { current },
    }
}

fn main() -> ExitCode {
    let write = std::env::args().any(|arg| arg == "--write");
    let root = repo_root();

    if write {
        let digest = payload_digest(&root, PAYLOAD_DIR);
        write_stamp(&root, &digest);
        println!("{STAMP_PATH} written: {digest}");
        return ExitCode::SUCCESS;
    }

    match decide(&root) {
        Verdict::Match => ExitCode::SUCCESS,
        Verdict::Stale { recorded, current } => {
            eprintln!(
                "payload-stamp-gate: {STAMP_PATH} is stale.\n  recorded: {recorded}\n  current:  {current}\n{PAYLOAD_DIR}/ changed since {STAMP_PATH} was last regenerated (HR-081: this is how a template-only change reaches the crates/houserules release). Run `cargo run --quiet --bin payload-stamp-gate -- --write` and commit the result alongside your {PAYLOAD_DIR}/ change."
            );
            ExitCode::FAILURE
        }
        Verdict::Missing { current } => {
            eprintln!(
                "payload-stamp-gate: {STAMP_PATH} is missing or unreadable.\n  current digest: {current}\nRun `cargo run --quiet --bin payload-stamp-gate -- --write` and commit the result."
            );
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(root: &Path) {
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .output()
            .expect("git init");
    }

    fn stage_all(root: &Path) {
        Command::new("git")
            .args(["add", "-A"])
            .current_dir(root)
            .output()
            .expect("git add -A");
    }

    /// The digest changes when a tracked payload file's content changes,
    /// even though the file set is unchanged -- a content-only edit is
    /// exactly the case a residue of `template/`'s own change history
    /// would miss if the digest keyed on paths alone.
    #[test]
    fn payload_digest_changes_when_a_tracked_files_content_changes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        fs::create_dir_all(root.join("template")).expect("mkdir template");
        fs::write(root.join("template/CLAUDE.md"), "one\n").expect("write");
        stage_all(root);
        let before = payload_digest(root, "template");

        fs::write(root.join("template/CLAUDE.md"), "two\n").expect("rewrite");
        stage_all(root);
        let after = payload_digest(root, "template");

        assert_ne!(before, after);
    }

    /// The digest changes when a file is added under the payload
    /// directory, even with every existing file's content unchanged.
    #[test]
    fn payload_digest_changes_when_a_file_is_added() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        fs::create_dir_all(root.join("template")).expect("mkdir template");
        fs::write(root.join("template/CLAUDE.md"), "one\n").expect("write");
        stage_all(root);
        let before = payload_digest(root, "template");

        fs::write(root.join("template/README.md"), "new\n").expect("write new file");
        stage_all(root);
        let after = payload_digest(root, "template");

        assert_ne!(before, after);
    }

    /// The digest is insensitive to `git ls-files`' own listing order:
    /// two directory trees with the identical path/content pairs, staged
    /// in a different order, produce the identical digest -- `sort()` in
    /// `tracked_files_under` is what this test proves. Sorted order is
    /// only half of what makes the comparison meaningful across two
    /// different git checkouts (a contributor's clone and CI's): the
    /// other half is `.gitattributes`'s repository-wide `* -text`, which
    /// keeps every checkout's bytes byte-for-byte identical in the first
    /// place (`houserules.payload-embeds-checkout-bytes`), so there is
    /// nothing left for the sorted digest to disagree on.
    #[test]
    fn payload_digest_is_independent_of_staging_order() {
        let dir_a = tempfile::tempdir().expect("tempdir");
        let root_a = dir_a.path();
        init_repo(root_a);
        fs::create_dir_all(root_a.join("template")).expect("mkdir");
        fs::write(root_a.join("template/a.md"), "a\n").expect("write a");
        stage_all(root_a);
        fs::write(root_a.join("template/b.md"), "b\n").expect("write b");
        stage_all(root_a);

        let dir_b = tempfile::tempdir().expect("tempdir");
        let root_b = dir_b.path();
        init_repo(root_b);
        fs::create_dir_all(root_b.join("template")).expect("mkdir");
        fs::write(root_b.join("template/b.md"), "b\n").expect("write b first");
        fs::write(root_b.join("template/a.md"), "a\n").expect("write a second");
        stage_all(root_b);

        assert_eq!(
            payload_digest(root_a, "template"),
            payload_digest(root_b, "template")
        );
    }

    /// A path/content boundary collision (`"ab"` + `""` vs `"a"` + `"b"`)
    /// must not produce the same digest: the NUL separators between and
    /// after each field are what rule this out.
    #[test]
    fn payload_digest_does_not_collide_across_a_path_content_boundary() {
        let dir_a = tempfile::tempdir().expect("tempdir");
        let root_a = dir_a.path();
        init_repo(root_a);
        fs::create_dir_all(root_a.join("template")).expect("mkdir");
        fs::write(root_a.join("template/ab"), "").expect("write ab");
        stage_all(root_a);

        let dir_b = tempfile::tempdir().expect("tempdir");
        let root_b = dir_b.path();
        init_repo(root_b);
        fs::create_dir_all(root_b.join("template")).expect("mkdir");
        fs::write(root_b.join("template/a"), "b").expect("write a");
        stage_all(root_b);

        assert_ne!(
            payload_digest(root_a, "template"),
            payload_digest(root_b, "template")
        );
    }

    /// `read_stamp` reports `None`, not a panic, for a repository that has
    /// never run `--write` -- the normal state before a payload's first
    /// stamp.
    #[test]
    fn read_stamp_is_none_when_the_stamp_file_does_not_exist() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(read_stamp(dir.path()), None);
    }

    /// `write_stamp` then `read_stamp` round-trips the exact digest,
    /// trimmed of the trailing newline `write_stamp` adds.
    #[test]
    fn write_stamp_then_read_stamp_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("crates/houserules")).expect("mkdir");
        write_stamp(root, "deadbeef");
        assert_eq!(read_stamp(root), Some("deadbeef".to_string()));
    }

    // ---- decide ----

    fn seed_template(root: &Path, content: &str) {
        fs::create_dir_all(root.join("crates/houserules")).expect("mkdir crates/houserules");
        fs::create_dir_all(root.join("template")).expect("mkdir template");
        fs::write(root.join("template/CLAUDE.md"), content).expect("write template file");
        stage_all(root);
    }

    #[test]
    fn decide_reports_match_when_the_recorded_stamp_agrees_with_the_current_digest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        seed_template(root, "one\n");
        let digest = payload_digest(root, PAYLOAD_DIR);
        write_stamp(root, &digest);
        assert_eq!(decide(root), Verdict::Match);
    }

    #[test]
    fn decide_reports_stale_naming_both_digests_when_the_stamp_disagrees() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        seed_template(root, "one\n");
        write_stamp(root, "not-the-real-digest");
        let current = payload_digest(root, PAYLOAD_DIR);
        assert_eq!(
            decide(root),
            Verdict::Stale {
                recorded: "not-the-real-digest".to_string(),
                current,
            }
        );
    }

    #[test]
    fn decide_reports_missing_naming_the_current_digest_when_no_stamp_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        init_repo(root);
        seed_template(root, "one\n");
        let current = payload_digest(root, PAYLOAD_DIR);
        assert_eq!(decide(root), Verdict::Missing { current });
    }
}
