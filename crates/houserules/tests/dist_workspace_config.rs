//! Pins the permanent fix for HR-118: `dist-workspace.toml`'s `[dist]`
//! table carries `create-release = false`, and the generated
//! `.github/workflows/release.yml` host job uploads into an existing
//! GitHub Release instead of creating one.
//!
//! T2's live `v0.3.0` cut hit the collision this fix closes:
//! release-please (on its PAT) created the release on the tag push, then
//! dist's host job failed at `gh release create` ("a release with the same
//! tag name already exists") -- both tools tried to create the same
//! release (docs/design.md 5.77). cargo-dist 0.32.0's own docs
//! (`reference/config.html#create-release`, checked 2026-09-13) state the
//! fix directly: "If false, dist will assume a draft GitHub Release for the
//! current git tag already exists with the title/body you want ... upload
//! artifacts to it and undraft when complete." That cooperates with
//! release-please's own already-published (not draft) release: undrafting
//! a published release is a no-op.
//!
//! `release.yml` is generated from `dist-workspace.toml`
//! (`houserules.release-workflow-is-generated`); this file pins both
//! halves so a hand-edit to either one that loses the pairing fails here
//! instead of shipping a config whose generated workflow no one re-checked.

use std::fs;
use std::path::{Path, PathBuf};

/// This checkout's repository root, resolved at compile time -- this
/// file's own copy of the pattern every `tests/*.rs` file in this crate
/// keeps independently (`install.rs`'s own doc explains why: a file
/// needing none of `mod common;`'s other helpers still warns the rest of
/// that module dead if pulled in just for this one function).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The text between a `[dist]` table header and the next `[` table header
/// (or end of file) in `dist-workspace.toml` -- just enough table scoping
/// to prove `create-release = false` sits inside `[dist]` itself, not
/// merely somewhere in the file (a comment, or a differently-named table).
/// A hand-rolled scan is sufficient here: this file has one `[dist]`
/// header, and no other table in cargo-dist's schema names a
/// `create-release` key, so nothing beyond this substring match is at
/// stake -- not worth a `toml` crate dependency for one boolean.
fn dist_table_text(raw: &str) -> &str {
    let start = raw
        .find("\n[dist]\n")
        .expect("dist-workspace.toml has a [dist] table header")
        + 1;
    let after_header = &raw[start..];
    let body_start = after_header
        .find('\n')
        .expect("[dist] header has a newline after it")
        + 1;
    let body = &after_header[body_start..];
    let end = body.find("\n[").unwrap_or(body.len());
    &body[..end]
}

/// The `[dist]` table carries an uncommented `create-release = false`
/// line. Losing this line (or a stray `#` reintroducing it as a comment)
/// reopens the T2 collision: dist would default `create-release` back to
/// `true` and its host job would call `gh release create` against a tag
/// release-please already published.
#[test]
fn dist_workspace_sets_create_release_false() {
    let raw = fs::read_to_string(repo_root().join("dist-workspace.toml"))
        .expect("read dist-workspace.toml");
    let dist_table = dist_table_text(&raw);
    assert!(
        dist_table
            .lines()
            .any(|line| line.trim() == "create-release = false"),
        "[dist] table does not set create-release = false:\n{dist_table}"
    );
}

/// The generated host job uploads into release-please's already-created
/// release (`gh release upload`) and never calls `gh release create` --
/// the exact call that collided in T2. `dist-workspace.toml` is this
/// file's source (`houserules.release-workflow-is-generated`), but no CI
/// job or mise task runs `dist generate --check` today: a control probe
/// (a scratch clone, `dist generate --check` after a one-line hand-edit
/// to `release.yml`) exits 0 under this repository's own
/// `allow-dirty = ["ci"]`, and only exits 255 once allow-dirty is
/// cleared. This test substring-pins both halves of the pairing so a
/// drift between them fails `cargo test` even without that gate; HR-119
/// tracks wiring `dist generate --check` into a real, permanent CI step.
#[test]
fn release_workflow_host_step_uploads_not_creates() {
    let workflow = fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("read .github/workflows/release.yml");
    assert!(
        workflow.contains("gh release upload"),
        "release.yml's host job does not call gh release upload"
    );
    assert!(
        !workflow.contains("gh release create"),
        "release.yml's host job still calls gh release create, which collides \
         with the release release-please already published"
    );
}
