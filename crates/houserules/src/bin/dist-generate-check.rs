//! HR-119: a real CI gate for the create-release pairing
//! `dist_workspace_config.rs` already pins by substring.
//! `houserules.release-workflow-is-generated`'s own rule states the
//! discipline (`.github/workflows/release.yml` is generated from
//! `dist-workspace.toml`; `dist generate --check` is the gate) -- this
//! binary is that gate, wired into `mise.toml`'s `lint` task (the
//! `checks` job's ubuntu-only leg; `cargo-dist` is scoped `os = ["linux"]`
//! in `mise.toml`'s own `[tools]` table, so the `rust` job's macOS/Windows
//! legs never need it installed at all).
//!
//! # Why a bare `dist generate --check` is not the gate
//!
//! `dist-workspace.toml`'s own `[dist] allow-dirty = ["ci"]` makes a plain
//! `dist generate --check` tolerate ANY drift in the generated CI output
//! (`release.yml` included) -- that is the setting's whole purpose for a
//! human running `dist plan`/`dist generate` day to day, but it also makes
//! the check vacuous as a GATE: a hand-edited `release.yml` still exits 0.
//! This binary runs the check against a SCRATCH COPY with `allow-dirty`
//! cleared to `[]` instead, in a temp directory -- the real, tracked
//! `dist-workspace.toml` in the working tree is never mutated.
//!
//! # The Dependabot interplay (HR-092)
//!
//! Clearing `allow-dirty` on its own creates a NEW false-positive:
//! Dependabot bumps an action's pin directly inside `release.yml` (its
//! own `uses: <action>@<sha>` lines), never `dist-workspace.toml`'s
//! `[dist.github-action-commits]` seed table alongside it
//! (`houserules.actions-pinned-by-sha`). A fresh regen from the
//! now-stale seed would then reintroduce the OLD sha, diffing against
//! every occurrence Dependabot already moved -- a gate failure on
//! exactly the automated maintenance this repository wants, not drift to
//! flag. Verified live (this binary's own module doc is the disclosure
//! the design demands): a synthetic Dependabot-shaped bump of
//! `actions/checkout`'s sha in a scratch `release.yml`, with
//! `dist-workspace.toml` left untouched, flips a cleared-`allow-dirty`
//! `dist generate --check` from exit 0 to exit 255; re-seeding the
//! scratch table's `actions/checkout` entry from the bumped `release.yml`
//! before checking restores exit 0.
//!
//! `rewrite_scratch_config` performs exactly that reseed: every
//! `[dist.github-action-commits]` line whose action `release.yml`'s own
//! `uses:` lines still reference gets its `sha` replaced with whatever
//! sha `release.yml` currently uses for that action -- its trailing `#`
//! comment (the human-readable version this repository's own convention
//! keeps) is left as written, since `release.yml` itself never carries
//! that comment for `dist generate --check` to catch drift in. A pin the
//! table names that `release.yml` no longer references at all (a
//! `cargo-dist-version` bump that stopped using it) is left exactly as
//! recorded -- an unrecognized case this binary declines to guess about;
//! `dist generate --check`'s own diff against the untouched line is the
//! right way to surface it.
//!
//! Structural drift -- anything else: an added or removed step, a changed
//! job, a stale non-pin setting -- is NOT reseeded, so it still fails the
//! cleared-`allow-dirty` check exactly as it should.
//!
//! The reseed is one-directional, disclosed rather than fixed here: it
//! makes the scratch copy's table follow `release.yml`, so the TRACKED
//! `dist-workspace.toml`'s own four seeded sha values are never compared
//! against anything by this gate -- a corrupted or hand-typed sha there
//! (a zeroed `actions/attest`, say) still exits 0, since the scratch
//! copy's reseeded value is what `dist generate --check` actually sees.
//! Verified live: t4-evidence/hr119-zeroed-attest-probe.sh. HR-126 tracks
//! a vacuousness check for this seed table (this direction, and an
//! entry naming an action `dist` no longer emits at all); out of this
//! gate's own scope.
//!
//! # The mise/dist-workspace pin, checked before any of that
//!
//! `mise.toml`'s own `[tools] cargo-dist` entry and `dist-workspace.toml`'s
//! `cargo-dist-version` are two independent pins of the same tool
//! (`security-hygiene.exact-pins`); this binary runs whichever `dist`
//! mise.toml resolves onto `PATH`, so a pin drifting away from the other
//! would silently regenerate with the WRONG cargo-dist version -- a
//! result neither a pass nor a fail actually verified. Checked first,
//! before the scratch copy is even built: a mismatch is a named failure
//! naming both versions, never a generate-and-compare run against a tool
//! version nobody confirmed matches the config.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

/// Path, relative to the repository root, `dist-workspace.toml` lives at.
const CONFIG_PATH: &str = "dist-workspace.toml";

/// Path, relative to the repository root, the generated release workflow
/// lives at.
const WORKFLOW_PATH: &str = ".github/workflows/release.yml";

/// A fresh, empty directory under the OS temp root, removed by its own
/// `Drop` -- `tempfile` (this crate's `[dev-dependencies]`) is unavailable
/// to a `src/bin/*.rs` target's production code (`gen-goldens.rs`'s own
/// `ScratchDir` states the identical reasoning; this is that same
/// hand-rolled copy, kept independently per this package's no-library-
/// target convention).
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(prefix: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("mkdir {}: {error}", dir.display()));
        ScratchDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// This checkout's repository root, resolved at compile time -- every
/// other `src/bin/*.rs` file's own copy of this helper.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Every git-tracked path under `root`, repo-relative POSIX paths, via
/// `git ls-files -z` -- `-z` avoids `core.quotePath`'s escaping of a
/// non-ASCII path (`payload-stamp-gate.rs::tracked_files_under`'s own doc
/// has the full account); this gate has no legitimate reason to skip a
/// tracked path for being non-UTF-8, so an undecodable entry is a named
/// panic here rather than a silent drop.
fn tracked_files(root: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("run git ls-files: {error}"));
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
        .stdout
        .split(|&byte| byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            String::from_utf8(entry.to_vec())
                .unwrap_or_else(|_| panic!("tracked path is not valid UTF-8"))
        })
        .collect()
}

/// Copies every one of `root`'s git-tracked files into `dest`, creating
/// parent directories as needed -- a full, disposable working copy
/// `dist generate` can run against without ever touching `root` itself.
fn copy_tracked_tree(root: &Path, dest: &Path) {
    for relative in tracked_files(root) {
        let src = root.join(&relative);
        let dst = dest.join(&relative);
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|error| panic!("mkdir {}: {error}", parent.display()));
        }
        fs::copy(&src, &dst)
            .unwrap_or_else(|error| panic!("copy {} -> {}: {error}", src.display(), dst.display()));
    }
}

/// Every `<action>@<40-hex-sha>` pair `release_workflow`'s own `uses:`
/// lines reference right now, keyed by action name (a later occurrence of
/// the same action overwrites an earlier one; a `release.yml` where two
/// occurrences of the same action have drifted apart is exactly the
/// genuine anomaly `dist generate --check`'s own byte diff should still
/// catch, not something this scan smooths over).
fn used_action_shas(release_workflow: &str) -> HashMap<String, String> {
    let mut current = HashMap::new();
    for line in release_workflow.lines() {
        let trimmed = line.trim_start();
        let after_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let Some(after_uses) = after_dash.strip_prefix("uses: ") else {
            continue;
        };
        let Some((action, sha)) = after_uses.split_once('@') else {
            continue;
        };
        let sha = sha.trim();
        if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
            current.insert(action.to_string(), sha.to_string());
        }
    }
    current
}

/// The `[dist.github-action-commits]` table's own line range within
/// `lines`: the header's own index, and the exclusive end index (the next
/// `[`-headed line, or `lines.len()`) -- `dist_workspace_config.rs`'s own
/// `dist_table_text` is the identical hand-rolled table-scoping
/// technique, against a different table; a `toml` crate dependency is not
/// worth adding for this one table.
fn github_action_commits_table_range(lines: &[&str]) -> (usize, usize) {
    let start = lines
        .iter()
        .position(|line| line.trim() == "[dist.github-action-commits]")
        .expect("dist-workspace.toml has a [dist.github-action-commits] table header");
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map_or(lines.len(), |offset| start + 1 + offset);
    (start, end)
}

/// Rewrites one `[dist.github-action-commits]` table line's `sha` from
/// `current_shas`, keeping its own indentation, action name, and trailing
/// `# ` comment exactly as written -- `None` when `line` does not name an
/// action `current_shas` carries (not a `"action" = "sha" # comment`
/// line at all, or an action `release.yml` no longer references), the
/// module doc's own "left exactly as recorded" case.
fn reseed_line(line: &str, current_shas: &HashMap<String, String>) -> Option<String> {
    let trimmed = line.trim_start();
    let after_quote = trimmed.strip_prefix('"')?;
    let quote_end = after_quote.find('"')?;
    let action = &after_quote[..quote_end];
    let sha = current_shas.get(action)?;
    let comment_index = line.find(" # ")?;
    let indent = &line[..line.len() - trimmed.len()];
    Some(format!(
        "{indent}\"{action}\" = \"{sha}\"{}",
        &line[comment_index..]
    ))
}

/// Builds the scratch copy's own `dist-workspace.toml` content from the
/// real, tracked `config`: `allow-dirty = ["ci"]` cleared to `[]`, and
/// every `[dist.github-action-commits]` entry `release_workflow` still
/// references reseeded to its current sha (module doc, "The Dependabot
/// interplay"). Panics naming the missing line if `config` no longer
/// carries the exact `allow-dirty = ["ci"]` line this rewrite targets --
/// a structural change here needs a human, not a silent no-op.
fn rewrite_scratch_config(config: &str, release_workflow: &str) -> String {
    let mut lines: Vec<String> = config.lines().map(str::to_string).collect();

    let allow_dirty_index = lines
        .iter()
        .position(|line| line.trim() == "allow-dirty = [\"ci\"]")
        .expect("dist-workspace.toml's own allow-dirty = [\"ci\"] line was not found to clear");
    lines[allow_dirty_index] = "allow-dirty = []".to_string();

    let current_shas = used_action_shas(release_workflow);
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (table_start, table_end) = github_action_commits_table_range(&borrowed);
    for line in &mut lines[table_start + 1..table_end] {
        if let Some(rewritten) = reseed_line(line, &current_shas) {
            *line = rewritten;
        }
    }

    let mut joined = lines.join("\n");
    joined.push('\n');
    joined
}

/// `mise.toml`'s own pinned `cargo-dist` version -- the `[tools]` table's
/// `cargo-dist = { version = "X", ... }` entry.
fn mise_cargo_dist_version(mise_toml: &str) -> String {
    let line = mise_toml
        .lines()
        .find(|line| line.trim_start().starts_with("cargo-dist = "))
        .expect("mise.toml has a cargo-dist entry in [tools]");
    let after_key = line
        .split_once("version = \"")
        .expect("mise.toml's cargo-dist entry has a version = \"...\" field")
        .1;
    let end = after_key
        .find('"')
        .expect("mise.toml's cargo-dist version has a closing quote");
    after_key[..end].to_string()
}

/// `dist-workspace.toml`'s own pinned `cargo-dist-version`.
fn dist_workspace_cargo_dist_version(dist_workspace_toml: &str) -> String {
    let line = dist_workspace_toml
        .lines()
        .find(|line| line.trim_start().starts_with("cargo-dist-version = "))
        .expect("dist-workspace.toml has a cargo-dist-version line");
    let after_key = line
        .split_once('"')
        .expect("dist-workspace.toml's cargo-dist-version has an opening quote")
        .1;
    let end = after_key
        .find('"')
        .expect("dist-workspace.toml's cargo-dist-version has a closing quote");
    after_key[..end].to_string()
}

/// Runs `dist generate --check` inside `scratch_root` (the scratch copy's
/// own top level, so relative paths inside it resolve the same way a real
/// checkout's would), returning its exit status. `dist` resolves on
/// `PATH` -- `mise.toml`'s own `[tools]` table scopes it `os = ["linux"]`
/// so the `checks` job's `mise run lint` step (this binary's own caller)
/// always has it, with no separate install step this binary needs to run
/// itself.
fn run_dist_generate_check(scratch_root: &Path) -> std::process::ExitStatus {
    Command::new("dist")
        .args(["generate", "--check"])
        .current_dir(scratch_root)
        .status()
        .unwrap_or_else(|error| panic!("run dist generate --check: {error}"))
}

fn main() -> ExitCode {
    let root = repo_root();

    let mise_toml = fs::read_to_string(root.join("mise.toml"))
        .unwrap_or_else(|error| panic!("read mise.toml: {error}"));
    let config = fs::read_to_string(root.join(CONFIG_PATH))
        .unwrap_or_else(|error| panic!("read {CONFIG_PATH}: {error}"));
    let mise_version = mise_cargo_dist_version(&mise_toml);
    let workspace_version = dist_workspace_cargo_dist_version(&config);
    if mise_version != workspace_version {
        eprintln!(
            "dist-generate-check: mise.toml's cargo-dist ({mise_version}) and \
             {CONFIG_PATH}'s cargo-dist-version ({workspace_version}) have drifted apart -- \
             this gate runs whichever `dist` mise.toml pins, which may not be the version \
             {CONFIG_PATH} itself targets. Bump both to the same pin."
        );
        return ExitCode::FAILURE;
    }

    let scratch = ScratchDir::new("dist-generate-check");
    copy_tracked_tree(&root, scratch.path());

    let scratch_config_path = scratch.path().join(CONFIG_PATH);
    let release_workflow = fs::read_to_string(root.join(WORKFLOW_PATH))
        .unwrap_or_else(|error| panic!("read {WORKFLOW_PATH}: {error}"));
    let rewritten = rewrite_scratch_config(&config, &release_workflow);
    fs::write(&scratch_config_path, rewritten)
        .unwrap_or_else(|error| panic!("write {}: {error}", scratch_config_path.display()));

    let status = run_dist_generate_check(scratch.path());
    if status.success() {
        println!(
            "dist-generate-check: {WORKFLOW_PATH} matches a fresh `dist generate` from \
             {CONFIG_PATH} (Dependabot action-pin drift tolerated)"
        );
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "dist-generate-check: {WORKFLOW_PATH} does not match a fresh `dist generate` from \
             {CONFIG_PATH} (Dependabot action-pin drift already tolerated above -- see dist's \
             own diff). Run `dist generate` and commit the result, per \
             houserules.release-workflow-is-generated."
        );
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- used_action_shas ----

    #[test]
    fn used_action_shas_reads_every_uses_line_regardless_of_leading_dash() {
        let workflow = "\
      - uses: actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803
        uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a
";
        let current = used_action_shas(workflow);
        assert_eq!(
            current.get("actions/checkout").map(String::as_str),
            Some("d23441a48e516b6c34aea4fa41551a30e30af803")
        );
        assert_eq!(
            current.get("actions/upload-artifact").map(String::as_str),
            Some("043fb46d1a93c77aae656e7c1c64a875d1fc6a0a")
        );
        assert_eq!(current.len(), 2);
    }

    #[test]
    fn used_action_shas_ignores_a_tag_reference_and_a_short_hex_string() {
        let workflow = "\
      - uses: actions/checkout@v6
      - uses: actions/checkout@abc123
";
        assert!(used_action_shas(workflow).is_empty());
    }

    #[test]
    fn used_action_shas_keeps_the_latest_occurrence_when_two_diverge() {
        let workflow = "\
      - uses: actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803
      - uses: actions/checkout@ffffffffffffffffffffffffffffffffffffffff
";
        assert_eq!(
            used_action_shas(workflow)
                .get("actions/checkout")
                .map(String::as_str),
            Some("ffffffffffffffffffffffffffffffffffffffff")
        );
    }

    // ---- mise_cargo_dist_version / dist_workspace_cargo_dist_version ----

    #[test]
    fn mise_cargo_dist_version_reads_the_inline_table_entry() {
        let mise_toml =
            "[tools]\ncargo-dist = { version = \"0.32.0\", os = [\"linux\"] }\nrust = \"1.98.1\"\n";
        assert_eq!(mise_cargo_dist_version(mise_toml), "0.32.0");
    }

    #[test]
    fn dist_workspace_cargo_dist_version_reads_the_plain_string_line() {
        let config = "[dist]\ncargo-dist-version = \"0.32.0\"\nci = \"github\"\n";
        assert_eq!(dist_workspace_cargo_dist_version(config), "0.32.0");
    }

    // ---- github_action_commits_table_range ----

    #[test]
    fn github_action_commits_table_range_finds_the_header_through_eof_when_it_is_last() {
        let text = "[workspace]\nmembers = []\n\n[dist.github-action-commits]\n\"a\" = \"1\"\n\"b\" = \"2\"\n";
        let lines: Vec<&str> = text.lines().collect();
        let (start, end) = github_action_commits_table_range(&lines);
        assert_eq!(lines[start], "[dist.github-action-commits]");
        assert_eq!(end, lines.len());
        assert_eq!(&lines[start + 1..end], ["\"a\" = \"1\"", "\"b\" = \"2\""]);
    }

    #[test]
    fn github_action_commits_table_range_stops_at_the_next_header() {
        let text = "[dist.github-action-commits]\n\"a\" = \"1\"\n\n[dist.other]\nx = 1\n";
        let lines: Vec<&str> = text.lines().collect();
        let (start, end) = github_action_commits_table_range(&lines);
        assert_eq!(&lines[start + 1..end], ["\"a\" = \"1\"", ""]);
    }

    // ---- reseed_line ----

    #[test]
    fn reseed_line_replaces_the_sha_and_keeps_the_comment_when_the_action_is_known() {
        let mut current = HashMap::new();
        current.insert(
            "actions/checkout".to_string(),
            "ffffffffffffffffffffffffffffffffffffffff".to_string(),
        );
        let line = "\"actions/checkout\" = \"d23441a48e516b6c34aea4fa41551a30e30af803\" # v6.1.0";
        assert_eq!(
            reseed_line(line, &current),
            Some(
                "\"actions/checkout\" = \"ffffffffffffffffffffffffffffffffffffffff\" # v6.1.0"
                    .to_string()
            )
        );
    }

    #[test]
    fn reseed_line_is_none_for_an_action_release_yml_no_longer_uses() {
        let current = HashMap::new();
        let line = "\"actions/checkout\" = \"d23441a48e516b6c34aea4fa41551a30e30af803\" # v6.1.0";
        assert_eq!(reseed_line(line, &current), None);
    }

    #[test]
    fn reseed_line_is_none_for_a_non_table_row() {
        let mut current = HashMap::new();
        current.insert("actions/checkout".to_string(), "f".repeat(40));
        assert_eq!(reseed_line("[dist.github-action-commits]", &current), None);
    }

    // ---- rewrite_scratch_config ----

    const SAMPLE_CONFIG: &str = "\
[workspace]
members = [\"cargo:.\"]

[dist]
allow-dirty = [\"ci\"]

[dist.github-action-commits]
\"actions/checkout\" = \"d23441a48e516b6c34aea4fa41551a30e30af803\" # v6.1.0
\"actions/attest\" = \"1e69f48acb82d1966a394da916b4c1698aa569d6\" # v4.2.2
";

    #[test]
    fn rewrite_scratch_config_clears_allow_dirty() {
        let rewritten = rewrite_scratch_config(SAMPLE_CONFIG, "");
        assert!(rewritten.lines().any(|line| line == "allow-dirty = []"));
        assert!(!rewritten.contains("allow-dirty = [\"ci\"]"));
    }

    #[test]
    fn rewrite_scratch_config_reseeds_only_the_action_the_workflow_still_uses() {
        let release_workflow = "\
      - uses: actions/checkout@ffffffffffffffffffffffffffffffffffffffff
";
        let rewritten = rewrite_scratch_config(SAMPLE_CONFIG, release_workflow);
        assert!(rewritten.contains(
            "\"actions/checkout\" = \"ffffffffffffffffffffffffffffffffffffffff\" # v6.1.0"
        ));
        // actions/attest is untouched: release_workflow above never uses it.
        assert!(rewritten.contains(
            "\"actions/attest\" = \"1e69f48acb82d1966a394da916b4c1698aa569d6\" # v4.2.2"
        ));
    }

    #[test]
    #[should_panic(expected = "allow-dirty")]
    fn rewrite_scratch_config_panics_when_the_allow_dirty_line_is_gone() {
        let config = "[dist]\n[dist.github-action-commits]\n";
        rewrite_scratch_config(config, "");
    }
}
