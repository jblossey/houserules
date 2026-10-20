//! `init` and `files` CLI-level tests: the payload lives inside the binary
//! (`rust-embed`, `debug-embed` -- `install/mod.rs`'s own module doc has the
//! configuration account), so these tests run the compiled binary as a real
//! subprocess against real scratch git repositories, following
//! `check_commit.rs`'s structural pattern (its own doc explains why each
//! `tests/*.rs` file keeps its own small copy of these helpers rather than
//! sharing `mod common;`). No test here exercises `update`. The JS-vs-Rust
//! byte-identity diff and the release-build embed spot check are live-run
//! proofs, not `cargo test` cases: they need a real `node` invocation and a
//! real `--release` rebuild respectively, neither of which belongs in this
//! suite.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A `Command` for the compiled `houserules` binary under test. Sets
/// `HOUSERULES_SKIP_SELF_UPDATE` so `update`'s self-update phase
/// (`selfupdate.rs`) never runs here -- `update.rs`'s own copy of this
/// helper has the full account.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// This checkout's repository root, resolved at compile time so it is
/// correct regardless of the test runner's working directory --
/// `tests/common/mod.rs::repo_root`'s own copy, duplicated here rather than
/// pulled in via `mod common;` for the same reason `check_commit.rs` gives
/// (its own module doc): this file needs none of that module's other
/// helpers, and importing it whole would warn the rest of it as dead code.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// A fresh scratch directory with `git init` already run -- the minimal
/// fixture `install`'s own `.git`-existence check accepts. `tempfile`'s
/// `TempDir` removes itself on drop (`houserules.tests-clean-scratch-dirs`'s
/// sanctioned Rust form: a `Drop`-owning guard, minted at the same site
/// that creates it).
fn scratch_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    dir
}

/// Every `KIT_OWNED` path -- `install/mod.rs`'s own copy, which this file
/// cannot import (a bare CLI binary crate, no `pub` library surface):
/// `tools/kb.mjs`, `tools/backlog.mjs`, `tools/lib/cli.mjs`, and
/// `tools/lib/json-store.mjs` are not in this list -- they joined
/// `RETIRED` instead.
const KIT_OWNED: &[&str] = &[
    "tools/claude-session-start.sh",
    ".githooks/commit-msg",
    ".claude/agents/implementer.md",
    ".claude/agents/task-reviewer.md",
    ".claude/agents/branch-reviewer.md",
    ".claude/skills/orchestrating/SKILL.md",
    ".claude/skills/finishing-a-feature/SKILL.md",
    ".claude/skills/migrating-knowledge/SKILL.md",
];

/// Every `SEED_ONCE` path -- `install/mod.rs`'s own array, duplicated here
/// for the same reason `KIT_OWNED` above is.
const SEED_ONCE: &[&str] = &[
    "knowledge/schema.json",
    "knowledge/areas.json",
    "backlog/schema.json",
    "backlog/amendments.json",
    "backlog/batches.json",
    "backlog/decisions.json",
    "backlog/parked.json",
    "backlog/items/general.json",
    ".claude/schemas/deliverables.json",
    ".claude/evals/dependency-add.json",
    ".claude/evals/docs-edit.json",
    ".claude/evals/record.json",
    ".claude/evals/seeded-violations.json",
    "docs/README.md",
    "AGENTS.md",
    "CLAUDE.md",
];

/// The CI workflow, the one D3 file that only a GitHub-hosted target gets.
const GITHUB_HOSTED_SEED_ONCE: &[&str] = &[".github/workflows/knowledge.yml"];

/// The `KIT_OWNED` files that only the Claude renderer needs --
/// `install/mod.rs`'s own `CLAUDE_FILES`. `init` leaves them out without
/// `--target claude`.
const CLAUDE_FILES: &[&str] = &[
    "tools/claude-session-start.sh",
    ".claude/agents/implementer.md",
    ".claude/agents/task-reviewer.md",
    ".claude/agents/branch-reviewer.md",
];

/// The five topic files of a 1.x install: `init` writes the kit's rules as
/// packs, so it seeds none of them.
const TOPIC_FILES: &[&str] = &[
    "knowledge/process.json",
    "knowledge/quality.json",
    "knowledge/security-hygiene.json",
    "knowledge/writing-style.json",
    "knowledge/knowledge-base.json",
];

/// The five kit packs, in the order `init` installs them.
const PACKS: &[&str] = &[
    "knowledge-base",
    "process",
    "quality",
    "security-hygiene",
    "writing-style",
];

/// The `SEED_ONCE` files that `init` seeds: every one.
fn seeded_once() -> impl Iterator<Item = &'static &'static str> {
    SEED_ONCE.iter()
}

/// The line `init` prints when the Claude renderer is off.
const ENABLE_CLAUDE_LINE: &str =
    "for Claude Code: run houserules init --target claude in the install root";

/// `env!("CARGO_PKG_VERSION")` at THIS test binary's own compile time --
/// the same value `install::kit_version` bakes in for the binary under
/// test (that function's own doc has the account).
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Runs `houserules files` and returns its stdout and the parsed JSON.
fn files_output() -> (String, serde_json::Value) {
    let output = houserules().arg("files").output().expect("run files");
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed = serde_json::from_str(&text).expect("files prints JSON");
    (text, parsed)
}

/// The whole output of `files`, spelled out: four classes, each path in one
/// of them (spec section 15: the pack copies and the D3 files are classes of
/// their own).
#[test]
fn files_prints_the_file_classes_of_the_kit_as_json() {
    let expected = serde_json::json!({
        "kitOwned": [
            "tools/claude-session-start.sh",
            ".githooks/commit-msg",
            ".claude/agents/implementer.md",
            ".claude/agents/task-reviewer.md",
            ".claude/agents/branch-reviewer.md",
            ".claude/skills/orchestrating/SKILL.md",
            ".claude/skills/finishing-a-feature/SKILL.md",
            ".claude/skills/migrating-knowledge/SKILL.md",
        ],
        "seedOnce": [
            "knowledge/areas.json",
            "backlog/schema.json",
            "backlog/amendments.json",
            "backlog/batches.json",
            "backlog/decisions.json",
            "backlog/parked.json",
            "backlog/items/general.json",
            ".claude/schemas/deliverables.json",
            ".claude/evals/dependency-add.json",
            ".claude/evals/docs-edit.json",
            ".claude/evals/record.json",
            ".claude/evals/seeded-violations.json",
            "docs/README.md",
            "AGENTS.md",
            "CLAUDE.md",
        ],
        "packCopies": [
            "knowledge/packs/knowledge-base.json",
            "knowledge/packs/process.json",
            "knowledge/packs/quality.json",
            "knowledge/packs/security-hygiene.json",
            "knowledge/packs/writing-style.json",
        ],
        "d3Files": [
            "knowledge/schema.json",
            ".github/workflows/knowledge.yml",
        ],
    });

    let (text, actual) = files_output();

    assert_eq!(actual, expected);
    // Byte-exact against `emit`'s own format (two-space indent, trailing
    // newline), not just structurally equal -- `houserules files` has no
    // `--dir` and no filesystem input, so its output is deterministic
    // enough to pin verbatim.
    assert_eq!(
        text,
        format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
    );
}

/// A path is a file of one class: it is kit-owned, or seeded once, or a copy
/// of a kit pack, or a D3 file.
#[test]
fn each_path_that_files_prints_is_in_one_class_only() {
    let (_, actual) = files_output();
    let mut seen = std::collections::BTreeMap::new();
    for (class, paths) in actual.as_object().expect("files prints an object") {
        for path in paths.as_array().expect("a class is an array") {
            let path = path.as_str().expect("a path is a string").to_string();
            if let Some(first) = seen.insert(path.clone(), class.clone()) {
                panic!("{path} is in the classes {first} and {class}");
            }
        }
    }
}

#[test]
fn init_bare_dir_flag_exits_2_with_claps_value_required_message() {
    // `argv_closure.rs`'s own class, pinned here rather than added to that
    // file's `COMMANDS` list: unlike every command that list covers, `init`'s
    // `--dir` is a real, load-bearing option in the frozen JS too (`files`'s
    // sibling test above pins the one JS-parity-relevant `--dir` divergence
    // this command has), so its argv edges belong beside its own behavior.
    // A bare trailing `--dir` (nothing follows it) never reaches
    // `tools/lib/cli.mjs`'s `parseArgs` as a flag at all -- `opts.dir`
    // stays `undefined`, so `node bin/houserules.mjs init --dir` from a
    // scratch git repo exits 0 and seeds that repo, the CURRENT
    // directory, exactly as a bare `init` would. clap's own stricter "a
    // value is required" refusal is a ruled divergence, not an unnoticed
    // one.
    let output = houserules()
        .args(["init", "--dir"])
        .output()
        .expect("run init --dir");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: a value is required for '--dir <DIR>' but none was supplied"),
        "got {stderr:?}"
    );
}

#[test]
fn init_duplicated_dir_flag_exits_2_with_claps_cannot_be_used_multiple_times_message() {
    // `node bin/houserules.mjs init --dir <a> --dir <b>` exits 0 and
    // seeds <b> only -- `parseArgs` consumes each `--dir` occurrence in
    // order, the second overwriting the first in `opts.dir`, so the last
    // one silently wins. clap's own "cannot be used multiple times"
    // refusal is the same ruled divergence the bare-flag test above
    // names, not an unnoticed one.
    let output = houserules()
        .args(["init", "--dir", "a", "--dir", "b"])
        .output()
        .expect("run init --dir a --dir b");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(
        stderr.starts_with("error: the argument '--dir <DIR>' cannot be used multiple times"),
        "got {stderr:?}"
    );
}

#[test]
fn files_takes_no_dir_flag() {
    // `bin/houserules.mjs`'s own `files` arm reads no `--dir` at all
    // (`main`'s `case 'files'` never touches `opts.dir`); this binary's
    // `Files` variant carries no such field, so clap itself rejects one.
    let output = houserules()
        .args(["files", "--dir", "x"])
        .output()
        .expect("run files --dir x");
    assert_eq!(output.status.code(), Some(2));
}

/// The lines `init` prints after the kit-owned and seed-once files, on a
/// repository with no `origin`: the skipped workflow, then (when `claude`)
/// the settings file, then the render lines: the blocks of the AGENTS.md
/// renderer, and with `claude` the files of the Claude renderer.
fn expected_tail_lines(claude: bool) -> Vec<String> {
    // `scratch_git_repo` configures no `origin` remote, so the
    // GitHub-hosted-only file is skipped, not written -- a fresh scratch
    // repository is never GitHub-hosted.
    let mut lines: Vec<String> = GITHUB_HOSTED_SEED_ONCE
        .iter()
        .map(|file| {
            format!(
                "skipped {file} (origin is not GitHub-hosted; see the migrating-knowledge skill to add your own CI gate)"
            )
        })
        .collect();
    if claude {
        lines.push("wrote .claude/settings.json".to_string());
    }
    lines.extend(["AGENTS.md: written", "tools/AGENTS.md: written"].map(str::to_string));
    if claude {
        lines.extend(
            [
                ".claude/rules/docs.md: written",
                ".claude/rules/tools.md: written",
                ".claude/skills/project-knowledge/SKILL.md: written",
            ]
            .map(str::to_string),
        );
    }
    lines
}

/// Without `--target claude`, a fresh `init` writes the kit-owned files that
/// every install gets, the seed-once files, and the five kit packs, then
/// renders and prints the line that enables Claude Code. Every printed line
/// is pinned whole.
#[test]
fn init_into_a_fresh_repo_writes_every_kit_owned_and_seed_once_file_then_renders() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "stderr: {stderr}");

    let kit_owned = || KIT_OWNED.iter().filter(|file| !CLAUDE_FILES.contains(file));
    let mut expected_lines: Vec<String> = kit_owned()
        .chain(seeded_once())
        .map(|file| format!("wrote {file}"))
        .collect();
    expected_lines.extend(
        PACKS
            .iter()
            .map(|pack| format!("wrote knowledge/packs/{pack}.json")),
    );
    expected_lines.extend(expected_tail_lines(false));
    expected_lines.push(format!("houserules: initialized {}", dir.path().display()));
    expected_lines.push(ENABLE_CLAUDE_LINE.to_string());
    expected_lines.push("next: houserules check-knowledge && houserules check-backlog".to_string());
    let actual_lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(actual_lines, expected_lines);

    for file in kit_owned().chain(seeded_once()) {
        assert!(dir.path().join(file).is_file(), "{file} was not written");
    }
    for file in CLAUDE_FILES.iter().chain(TOPIC_FILES) {
        assert!(!dir.path().join(file).exists(), "{file} was written");
    }
    for file in GITHUB_HOSTED_SEED_ONCE {
        assert!(
            !dir.path().join(file).exists(),
            "{file} was written into a non-GitHub-hosted target"
        );
    }
    assert!(
        !dir.path().join(".claude/settings.json").exists(),
        "the hook entry needs the Claude renderer"
    );
}

/// With `--target claude`, a fresh `init` also writes the Claude Code files
/// and the settings file, and prints no enable line. Every printed line is
/// pinned whole.
#[test]
fn init_target_claude_into_a_fresh_repo_writes_every_kit_owned_and_seed_once_file_then_renders() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "stderr: {stderr}");

    let mut expected_lines: Vec<String> = KIT_OWNED
        .iter()
        .chain(seeded_once())
        .map(|file| format!("wrote {file}"))
        .collect();
    expected_lines.extend(
        PACKS
            .iter()
            .map(|pack| format!("wrote knowledge/packs/{pack}.json")),
    );
    expected_lines.extend(expected_tail_lines(true));
    expected_lines.push(format!("houserules: initialized {}", dir.path().display()));
    expected_lines.push("next: houserules check-knowledge && houserules check-backlog".to_string());
    let actual_lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(actual_lines, expected_lines);

    for file in KIT_OWNED.iter().chain(seeded_once()) {
        assert!(dir.path().join(file).is_file(), "{file} was not written");
    }
    assert!(dir.path().join(".claude/settings.json").is_file());

    // The fresh-seed arm, with no pre-existing settings.json to merge
    // into, has no other assertion on TEMPLATE_MATCHERS -- only the
    // merge-scenario tests below do.
    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).expect("read settings.json"),
    )
    .expect("parse settings.json");
    let matchers: Vec<&str> = settings["hooks"]["SessionStart"]
        .as_array()
        .expect("hooks.SessionStart is an array")
        .iter()
        .map(|entry| entry["matcher"].as_str().expect("matcher is a string"))
        .collect();
    assert_eq!(matchers, TEMPLATE_MATCHERS);
}

/// `AGENTS.md` is the cross-harness instruction file `SEED_ONCE` carries
/// (`houserules.template-is-the-source`'s own copy is embedded byte for
/// byte, `payload_content` rewrites no prefix into it): a fresh `init`
/// writes it with the template's own text, and the render that ends `init`
/// appends the houserules block at the end of the file (design 5.115).
#[test]
fn init_into_a_fresh_repo_seeds_agents_md_with_the_template_text_and_the_block_after_it() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let seeded = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read seeded AGENTS.md");
    let template = fs::read_to_string(repo_root().join("template/AGENTS.md"))
        .expect("read template/AGENTS.md");
    let block = seeded
        .strip_prefix(&template)
        .expect("the text of the seed comes first, byte for byte");
    assert!(
        block.starts_with(
            "<!-- houserules:begin -->\nGenerated from knowledge/ by houserules render. Do not edit.\n"
        ),
        "the block follows the seed: {block:?}"
    );
    assert!(block.ends_with("\n<!-- houserules:end -->\n"), "{block:?}");
}

/// With `--dir` omitted entirely, `cmd_init` resolves
/// `dir.as_deref().unwrap_or_else(|| Path::new("."))` against the
/// PROCESS's own working directory, not an ancestor -- the arm this
/// file's other tests never exercise, since every other test here passes
/// `--dir` explicitly. `Command::current_dir` sets that working
/// directory for the child process.
#[test]
fn init_with_no_dir_flag_seeds_the_current_working_directory() {
    let dir = scratch_git_repo();
    let output = houserules()
        .arg("init")
        .current_dir(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path().join(".githooks/commit-msg").is_file());
}

/// `tools/kb.sh` and `tools/backlog.sh` are not in `KIT_OWNED`, so a
/// fresh `init` does not seed either -- the flat `houserules` command
/// surface is the only entry point a freshly seeded project gets.
#[test]
fn init_no_longer_seeds_the_retired_shell_tools() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(!stdout.contains("kb.sh"), "got:\n{stdout}");
    assert!(!dir.path().join("tools/kb.sh").exists());
    assert!(!dir.path().join("tools/backlog.sh").exists());
}

/// `tools/kb.mjs`, `tools/backlog.mjs`, `tools/lib/cli.mjs`, and
/// `tools/lib/json-store.mjs` are not in `KIT_OWNED` either -- a fresh
/// `init` does not seed any of them.
#[test]
fn init_no_longer_seeds_the_retired_js_engines() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    for file in [
        "tools/kb.mjs",
        "tools/backlog.mjs",
        "tools/lib/cli.mjs",
        "tools/lib/json-store.mjs",
    ] {
        assert!(!stdout.contains(file), "got:\n{stdout}");
        assert!(!dir.path().join(file).exists(), "{file} was seeded");
    }
}

#[test]
fn init_writes_kit_owned_content_byte_identical_to_the_template_source() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for file in KIT_OWNED {
        let expected =
            fs::read(repo_root().join("template").join(file)).expect("read template source");
        let actual = fs::read(dir.path().join(file)).expect("read seeded file");
        assert_eq!(actual, expected, "{file} diverged from template/{file}");
    }
}

#[cfg(unix)]
#[test]
fn init_marks_shell_scripts_and_the_git_hook_executable() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for file in ["tools/claude-session-start.sh", ".githooks/commit-msg"] {
        let mode = fs::metadata(dir.path().join(file))
            .unwrap_or_else(|error| panic!("stat {file}: {error}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755, "{file} mode is {mode:o}");
    }
    // A KIT_OWNED file the JS writer never chmods stays at the plain,
    // non-executable mode `writeFileSync` produces.
    let plain_mode = fs::metadata(dir.path().join(".claude/agents/implementer.md"))
        .expect("stat .claude/agents/implementer.md")
        .permissions()
        .mode();
    assert_eq!(plain_mode & 0o111, 0);
}

#[test]
fn init_stamps_the_marker_with_the_kit_version_and_the_default_id_prefix() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let marker: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".houserules.json")).expect("read marker"),
    )
    .expect("parse marker");
    assert_eq!(marker["version"], serde_json::json!(kit_version()));
    assert_eq!(marker["idPrefix"], serde_json::json!("WI"));
    assert!(marker.get("overrides").is_none(), "got {marker:?}");
    assert_baselines_are_the_paths_init_wrote(&marker);
}

/// `init` stamps a baseline for every `KIT_OWNED` path it wrote (a fresh
/// install's own content trivially matches the payload it was just written
/// from) and for `knowledge/schema.json`, and for no knowledge entry: the
/// kit's rules come as packs, whose pins replace the per-entry baselines. The
/// seeded workflow is baselined too, but only when `init` writes it, which
/// needs a GitHub-hosted origin.
fn assert_baselines_are_the_paths_init_wrote(marker: &serde_json::Value) {
    let baselines = marker["baselines"]
        .as_object()
        .unwrap_or_else(|| panic!("baselines is not an object in {marker:?}"));
    let mut expected: Vec<&str> = KIT_OWNED.to_vec();
    expected.push("knowledge/schema.json");
    expected.sort_unstable();
    let mut actual: Vec<&str> = baselines.keys().map(String::as_str).collect();
    actual.sort_unstable();
    assert_eq!(actual, expected);
}

#[test]
fn init_rewrites_the_id_prefix_in_prefixed_seed_files_but_not_the_marker_content_rules() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .arg("--id-prefix")
        .arg("FOO")
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    for file in [
        "backlog/schema.json",
        "backlog/items/general.json",
        ".claude/schemas/deliverables.json",
    ] {
        let text = fs::read_to_string(dir.path().join(file)).expect("read prefixed file");
        assert!(text.contains("FOO-"), "{file} missing FOO- prefix");
        assert!(!text.contains("WI-"), "{file} still carries WI-");
    }
    let general = fs::read_to_string(dir.path().join("backlog/items/general.json")).unwrap();
    assert!(general.contains("\"FOO-001\""));
}

#[test]
fn init_into_a_missing_git_repo_is_a_named_usage_error_exit_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{} is not a git repository (run git init first)\n",
            dir.path().display()
        )
    );
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn init_keeps_the_git_init_advice_outside_any_repository() {
    let outside = tempfile::tempdir().expect("tempdir");
    let nested = outside.path().join("a").join("b");
    fs::create_dir_all(&nested).expect("create nested directory");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(&nested)
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{} is not a git repository (run git init first)\n",
            nested.display()
        )
    );
}

/// A scratch repository with one commit and a linked worktree `wt` in it,
/// whose root holds `.git` as a file, not a directory. Returns the main
/// repository and the worktree path.
fn repository_with_a_linked_worktree() -> (tempfile::TempDir, PathBuf) {
    let main = scratch_git_repo();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=test",
                "-c",
                "user.email=test@example.invalid",
            ])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(main.path())
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["commit", "-q", "--allow-empty", "-m", "initial"]);
    let worktree = main.path().join("wt");
    git(&[
        "worktree",
        "add",
        "-q",
        "--detach",
        worktree.to_str().expect("utf8 path"),
    ]);
    assert!(
        worktree.join(".git").is_file(),
        "a linked worktree's .git is a file"
    );
    (main, worktree)
}

/// A linked worktree's root holds `.git` as a file, not a directory; it
/// is a top level and installs.
#[test]
fn init_accepts_a_linked_worktree_root() {
    let (_main, worktree) = repository_with_a_linked_worktree();

    let output = houserules()
        .args(["init", "--dir"])
        .arg(&worktree)
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(worktree.join("knowledge/schema.json").is_file());
}

/// Runs `init` with `args` in `cwd` and requires exit 0.
fn init_in(cwd: &Path, args: &[&str]) {
    let output = houserules()
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "init {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `--dir` and the working directory resolve to an absolute target before
/// the install root is placed, so a relative `--dir sub`, `--dir .`, and no
/// `--dir` at all each install in `sub`, below the repository top level.
#[test]
fn init_resolves_a_relative_target_to_the_subdirectory_install() {
    for (from_sub, args) in [
        (false, &["init", "--dir", "sub"][..]),
        (true, &["init", "--dir", "."][..]),
        (true, &["init"][..]),
    ] {
        let repo = scratch_git_repo();
        let sub = repo.path().join("sub");
        fs::create_dir(&sub).expect("create subdirectory");

        init_in(if from_sub { &sub } else { repo.path() }, args);

        assert!(sub.join(".houserules.json").is_file(), "{args:?}");
        assert!(sub.join("knowledge/schema.json").is_file(), "{args:?}");
        assert!(
            !repo.path().join(".houserules.json").exists(),
            "{args:?}: the install is not at the top level"
        );
    }
}

/// A subdirectory of a linked worktree installs below the worktree root, the
/// nearest directory that holds a `.git` entry, not below the main
/// repository.
#[test]
fn init_installs_in_a_subdirectory_of_a_linked_worktree() {
    let (main, worktree) = repository_with_a_linked_worktree();
    let sub = worktree.join("sub");
    fs::create_dir(&sub).expect("create subdirectory");

    init_in(
        main.path(),
        &["init", "--dir", sub.to_str().expect("utf8 path")],
    );

    assert!(sub.join(".houserules.json").is_file());
    assert!(!worktree.join(".houserules.json").exists());
    assert!(!main.path().join(".houserules.json").exists());
}

/// Creates `link`, a symlink to `original`.
#[cfg(unix)]
fn symlink(original: &Path, link: &Path) {
    std::os::unix::fs::symlink(original, link).expect("create symlink");
}

/// A symlink to a repository subdirectory, to a nested one, and a
/// subdirectory below a symlinked parent of the repository each install in
/// the real directory: the install root is placed below the top level by
/// its resolved path.
#[cfg(unix)]
#[test]
fn init_installs_in_the_subdirectory_that_a_symlink_names() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    let nested = repo.path().join("a").join("b");
    fs::create_dir(&sub).expect("create subdirectory");
    fs::create_dir_all(&nested).expect("create nested subdirectory");
    let links = tempfile::tempdir().expect("tempdir");
    symlink(&sub, &links.path().join("to-sub"));
    symlink(&nested, &links.path().join("to-nested"));

    init_in(
        repo.path(),
        &[
            "init",
            "--dir",
            links.path().join("to-sub").to_str().expect("utf8"),
        ],
    );
    init_in(
        repo.path(),
        &[
            "init",
            "--dir",
            links.path().join("to-nested").to_str().expect("utf8"),
        ],
    );

    assert!(sub.join(".houserules.json").is_file());
    assert!(nested.join(".houserules.json").is_file());
    assert!(!repo.path().join(".houserules.json").exists());
}

/// A subdirectory reached through a symlinked directory above the
/// repository installs in the real subdirectory.
#[cfg(unix)]
#[test]
fn init_installs_in_a_subdirectory_below_a_symlinked_parent() {
    let outer = tempfile::tempdir().expect("tempdir");
    let repo = outer.path().join("repo");
    let status = Command::new("git")
        .args(["init", "-q"])
        .arg(&repo)
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    let sub = repo.join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let links = tempfile::tempdir().expect("tempdir");
    let parent_link = links.path().join("parent-link");
    symlink(outer.path(), &parent_link);
    let through_link = parent_link.join("repo").join("sub");

    init_in(
        outer.path(),
        &["init", "--dir", through_link.to_str().expect("utf8")],
    );

    assert!(sub.join(".houserules.json").is_file());
    assert!(!repo.join(".houserules.json").exists());
}

/// A symlink to a repository root holds `.git` through the link, so it is
/// a top level and installs into the repository.
#[cfg(unix)]
#[test]
fn init_accepts_a_symlink_to_the_repository_root() {
    let repo = scratch_git_repo();
    let links = tempfile::tempdir().expect("tempdir");
    let link = links.path().join("link");
    symlink(repo.path(), &link);
    let output = houserules()
        .args(["init", "--dir"])
        .arg(&link)
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(repo.path().join("knowledge/schema.json").is_file());
}

/// The repository root reached through a symlinked parent directory
/// installs.
#[cfg(unix)]
#[test]
fn init_accepts_the_repository_root_below_a_symlinked_parent() {
    let outer = tempfile::tempdir().expect("tempdir");
    let repo = outer.path().join("repo");
    let status = Command::new("git")
        .args(["init", "-q"])
        .arg(&repo)
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    let links = tempfile::tempdir().expect("tempdir");
    let parent_link = links.path().join("parent-link");
    symlink(outer.path(), &parent_link);
    let output = houserules()
        .args(["init", "--dir"])
        .arg(parent_link.join("repo"))
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(repo.join("knowledge/schema.json").is_file());
}

/// A target that is a file, not a directory, keeps the `git init` advice
/// even inside a repository, and the advice prints the target as passed.
#[test]
fn init_keeps_the_git_init_advice_for_a_target_that_is_a_file() {
    let repo = scratch_git_repo();
    let file = repo.path().join("f");
    fs::write(&file, "").expect("create file");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(&file)
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{} is not a git repository (run git init first)\n",
            file.display()
        )
    );
}

/// A target that does not exist yet keeps the `git init` advice, even
/// inside a repository: the directory is not there to be inside anything.
#[test]
fn init_keeps_the_git_init_advice_for_a_target_that_does_not_exist_yet() {
    let repo = scratch_git_repo();
    let outside = tempfile::tempdir().expect("tempdir");
    for absent in [repo.path().join("absent"), outside.path().join("absent")] {
        let output = houserules()
            .args(["init", "--dir"])
            .arg(&absent)
            .output()
            .expect("run init");
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!(
                "{} is not a git repository (run git init first)\n",
                absent.display()
            )
        );
        assert!(!absent.exists(), "nothing was created");
    }
}

#[test]
fn init_rejects_a_malformed_id_prefix_flag_exit_2() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .args(["--id-prefix", "lowercase"])
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "id-prefix must be 1-8 characters, A-Z then A-Z0-9\n"
    );
    // Only the fixture's own `.git` predates the rejected call; nothing else
    // was written.
    let entries: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from(".git")]);
}

/// A pre-existing `knowledge/process.json` holding invalid JSON is a
/// project topic file: `init` seeds no topic file and does not read this one.
/// The first step that reads its content is the load of the knowledge base
/// that `render_and_report` runs (`rules::model::load_base`), which fails here
/// with its named-JSON-error shape. No earlier `install.rs` test reaches this
/// arm: every other error-path test here fails before any file write, at the
/// target's `.git` check, the id-prefix flag, or a pre-existing
/// `.houserules.json` -- none of them plants broken data under `knowledge/`
/// first.
///
/// The expected path is built one component at a time, the way
/// `rules::model::load_base` builds the name it prints: joining `"knowledge"`
/// onto the root, then the topic file name onto that. A single
/// `join("knowledge/process.json")` diverges from it on Windows alone:
/// `Path::join` inserts the platform's own separator only between components
/// it joins itself, so a literal `/` embedded in one string argument survives
/// untouched, while the code's own two-step join always uses the platform
/// separator.
#[test]
fn init_reports_a_render_failure_on_broken_project_data_as_one_usage_error() {
    let dir = scratch_git_repo();
    let knowledge_dir = dir.path().join("knowledge");
    fs::create_dir_all(&knowledge_dir).expect("mkdir knowledge");
    let broken_path = knowledge_dir.join("process.json");
    fs::write(&broken_path, "{").expect("write broken process.json");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    let expected_prefix = format!("{}: invalid JSON (", broken_path.display());
    assert!(
        stderr.starts_with(&expected_prefix),
        "expected prefix {expected_prefix:?}, got {stderr:?}"
    );
    assert_eq!(stderr.trim().split('\n').count(), 1, "got {stderr:?}");
}

#[test]
fn init_reports_an_unparsable_pre_existing_marker_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    fs::write(dir.path().join(".houserules.json"), "not json").expect("write marker");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).unwrap();
    let marker_path = dir.path().join(".houserules.json");
    assert!(
        stderr.starts_with(&format!("{}: invalid JSON (", marker_path.display())),
        "got {stderr:?}"
    );
}

#[test]
fn init_reports_a_pre_existing_markers_bad_id_prefix_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    fs::write(
        dir.path().join(".houserules.json"),
        r#"{"idPrefix":"lowercase"}"#,
    )
    .expect("write marker");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let marker_path = dir.path().join(".houserules.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: idPrefix must be 1-8 characters, A-Z then A-Z0-9\n",
            marker_path.display()
        )
    );
}

#[test]
fn init_reports_a_pre_existing_markers_empty_version_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    fs::write(dir.path().join(".houserules.json"), r#"{"version":""}"#).expect("write marker");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let marker_path = dir.path().join(".houserules.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: version must be a non-empty string\n",
            marker_path.display()
        )
    );
}

/// A second `init` seeds what is missing and keeps what exists. It keeps
/// every seed-once file, the settings file, and every kit-owned file, a
/// changed one included, so it prints no `wrote` line. It does not look at
/// the packs of an existing install, so it prints no line for a pack copy
/// (spec C18).
#[test]
fn a_second_init_keeps_every_file_that_exists_a_changed_kit_owned_file_included() {
    let dir = scratch_git_repo();
    let first = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("first init");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    let changed = ".githooks/commit-msg";
    fs::write(dir.path().join(changed), b"the adopter's own hook").expect("change the hook");

    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("second init");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success());

    for file in seeded_once() {
        assert!(
            stdout.contains(&format!("kept {file}\n")),
            "expected `kept {file}` in:\n{stdout}"
        );
    }
    for pack in PACKS {
        assert!(
            !stdout.contains(&format!("knowledge/packs/{pack}.json")),
            "a second init names the pack copy {pack} in:\n{stdout}"
        );
    }
    // No `origin` remote is configured, so both the first and second
    // `init` skip the GitHub-hosted-only file the same way.
    for file in GITHUB_HOSTED_SEED_ONCE {
        assert!(
            stdout.contains(&format!(
                "skipped {file} (origin is not GitHub-hosted; see the migrating-knowledge skill to add your own CI gate)\n"
            )),
            "expected `skipped {file}` in:\n{stdout}"
        );
    }
    for file in KIT_OWNED {
        assert!(
            stdout.contains(&format!("kept {file}\n")),
            "expected `kept {file}` in:\n{stdout}"
        );
    }
    let wrote: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("wrote "))
        .collect();
    assert!(wrote.is_empty(), "got {wrote:?}");
    assert!(stdout.contains("kept .claude/settings.json (hooks already present)\n"));
    assert!(
        fs::read(dir.path().join(changed)).expect("read the hook") == b"the adopter's own hook",
        "{changed} changed"
    );
}

/// `init` over a target that already carries a knowledge topic file of its
/// own -- the documented path for adopting the kit into a codebase with
/// existing knowledge -- keeps that file exactly as found. It installs the
/// kit's packs beside it and stamps a baseline for no knowledge entry: the
/// pins of the packs replace the per-entry baselines. The next `update` does
/// not touch the file either.
#[test]
fn init_over_a_pre_existing_topic_file_keeps_it_and_stamps_no_entry_baseline() {
    let dir = scratch_git_repo();
    fs::create_dir_all(dir.path().join("knowledge")).expect("mkdir knowledge");
    let adopters_own_file = r#"{"$schema": "./schema.json", "topic": "process", "title": "Adopter's own", "entries": []}"#;
    fs::write(dir.path().join("knowledge/process.json"), adopters_own_file)
        .expect("write adopter's own process.json");

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("wrote knowledge/packs/process.json\n"),
        "got:\n{stdout}"
    );

    let marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join(".houserules.json")).unwrap())
            .unwrap();
    let baselines = marker["baselines"].as_object().expect("baselines object");
    assert!(
        !baselines.keys().any(|key| key.starts_with("process.")),
        "init stamped a baseline for an entry: {baselines:?}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("knowledge/process.json")).unwrap(),
        adopters_own_file,
        "init modified a pre-existing topic file it should only have kept"
    );

    let update_output = houserules()
        .args(["update", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run update");
    assert!(
        update_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&update_output.stderr)
    );
    let update_stdout = String::from_utf8(update_output.stdout).expect("utf8 stdout");
    assert!(
        !update_stdout.contains("knowledge/process.json"),
        "got:\n{update_stdout}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("knowledge/process.json")).unwrap(),
        adopters_own_file,
        "update modified a topic file of the project"
    );
}

#[test]
fn init_merges_session_start_hooks_into_a_pre_existing_settings_file() {
    let dir = scratch_git_repo();
    fs::create_dir_all(dir.path().join(".claude")).expect("mkdir .claude");
    fs::write(
        dir.path().join(".claude/settings.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "hooks": {"SessionStart": [{"matcher": "custom", "hooks": []}]},
            "otherField": true,
        }))
        .unwrap(),
    )
    .expect("write settings.json");

    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let template: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo_root().join("template/.claude/settings.json")).unwrap(),
    )
    .unwrap();
    let template_matchers: Vec<&str> = template["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();

    let merged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(merged["otherField"], serde_json::json!(true));
    let merged_entries = merged["hooks"]["SessionStart"].as_array().unwrap();
    let merged_matchers: Vec<&str> = merged_entries
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();
    assert_eq!(merged_matchers[0], "custom");
    for matcher in template_matchers {
        assert!(
            merged_matchers.contains(&matcher),
            "{matcher} missing from merged SessionStart: {merged_matchers:?}"
        );
    }
}

/// Writes `value` as `.claude/settings.json` under `dir`, creating
/// `.claude/` as needed -- shared by the `merge_settings` arm tests below.
fn write_settings(dir: &Path, value: &serde_json::Value) {
    fs::create_dir_all(dir.join(".claude")).expect("mkdir .claude");
    fs::write(
        dir.join(".claude/settings.json"),
        serde_json::to_string_pretty(value).unwrap(),
    )
    .expect("write settings.json");
}

/// The two matchers the embedded `.claude/settings.json` template's own
/// `SessionStart` array carries.
const TEMPLATE_MATCHERS: [&str; 2] = ["compact", "startup|resume|clear|fork"];

/// A JSON `null` at `hooks` seeds cleanly under the frozen JS. With
/// `.claude/settings.json = {"hooks": null}`, `node bin/houserules.mjs
/// init --dir <scratch>` exits 0, prints `merged .claude/settings.json
/// (SessionStart hooks added)`, and writes both template matchers into
/// `hooks.SessionStart` -- `settings.hooks ??= {}` replaces `null` the
/// same as a missing key, so this is not a crash arm.
#[test]
fn init_seeds_a_null_hooks_settings_file_exactly_as_the_js_does() {
    let dir = scratch_git_repo();
    write_settings(dir.path(), &serde_json::json!({"hooks": null}));

    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let merged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let merged_matchers: Vec<&str> = merged["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();
    assert_eq!(merged_matchers, TEMPLATE_MATCHERS);
}

/// A JSON `null` at `hooks.SessionStart` (with `hooks` itself a genuine
/// object) also seeds cleanly. With `.claude/settings.json = {"hooks":
/// {"SessionStart": null}}`, `node bin/houserules.mjs init` exits 0,
/// prints the same `merged` line, and writes both template matchers --
/// `settings.hooks.SessionStart ??= []` replaces `null` the same way.
#[test]
fn init_seeds_a_null_session_start_settings_file_exactly_as_the_js_does() {
    let dir = scratch_git_repo();
    write_settings(
        dir.path(),
        &serde_json::json!({"hooks": {"SessionStart": null}}),
    );

    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let merged: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();
    let merged_matchers: Vec<&str> = merged["hooks"]["SessionStart"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["matcher"].as_str().unwrap())
        .collect();
    assert_eq!(merged_matchers, TEMPLATE_MATCHERS);
}

/// A `hooks` value that is itself a JSON ARRAY is a genuine JS quirk, not
/// a crash. With `.claude/settings.json = {"hooks": []}`, `node
/// bin/houserules.mjs init` exits 0, prints the `merged` line (`changed`
/// is unconditionally `true` -- `settings.hooks.SessionStart ??= []`
/// lands a non-index property on the array, every template matcher is
/// pushed into it, and `JSON.stringify` then drops that property because
/// it serializes only an array's indexed elements), and the file reads
/// back byte-for-byte `{"hooks": []}` -- the array's own (empty)
/// elements untouched, no matcher visible anywhere.
/// `install::merge_settings`'s own doc has the full mechanism; this pins
/// its matched output.
#[test]
fn init_matches_the_js_silently_dropping_the_merge_when_hooks_is_an_array() {
    let dir = scratch_git_repo();
    write_settings(dir.path(), &serde_json::json!({"hooks": []}));

    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("merged .claude/settings.json (SessionStart hooks added)\n"));

    let merged =
        fs::read_to_string(dir.path().join(".claude/settings.json")).expect("read settings.json");
    assert_eq!(merged, "{\n  \"hooks\": []\n}\n");
}

/// `hooks` holding a bare number is the genuine JS crash shape and stays
/// a named error. With `.claude/settings.json = {"hooks": 5}`, `node
/// bin/houserules.mjs init` exits 1 with an uncaught `TypeError: Cannot
/// create property 'SessionStart' on number '5'` and a full stack trace
/// (ES modules run in strict mode, where assigning a property onto a
/// primitive throws) -- `houserules.crash-paths-are-named` converts that
/// crash into one named stderr line and exit 2, never the reproduced
/// trace.
#[test]
fn init_reports_a_scalar_hooks_value_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    write_settings(dir.path(), &serde_json::json!({"hooks": 5}));

    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let settings_path = dir.path().join(".claude/settings.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("{}: hooks is not an object\n", settings_path.display())
    );
}

/// `hooks.SessionStart` holding a bare number is the other genuine JS
/// crash shape. With `.claude/settings.json = {"hooks": {"SessionStart":
/// 5}}`, `node bin/houserules.mjs init` exits 1 with an uncaught
/// `TypeError: settings.hooks.SessionStart.map is not a function` --
/// named here too.
#[test]
fn init_reports_a_scalar_session_start_value_as_a_named_error_exit_2() {
    let dir = scratch_git_repo();
    write_settings(
        dir.path(),
        &serde_json::json!({"hooks": {"SessionStart": 5}}),
    );

    let output = houserules()
        .args(["init", "--target", "claude", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    let settings_path = dir.path().join(".claude/settings.json");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: hooks.SessionStart is not an array\n",
            settings_path.display()
        )
    );
}
