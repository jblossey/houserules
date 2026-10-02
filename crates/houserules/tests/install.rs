//! `init` and `files` CLI-level tests: the payload lives inside the binary
//! (`rust-embed`, `debug-embed` -- `install.rs`'s own module doc has the
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

/// Every `KIT_OWNED` path -- `install.rs`'s own copy, which this file
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

/// Every `SEED_ONCE` path -- `install.rs`'s own array, duplicated here
/// for the same reason `KIT_OWNED` above is.
const SEED_ONCE: &[&str] = &[
    "knowledge/schema.json",
    "knowledge/areas.json",
    "knowledge/process.json",
    "knowledge/quality.json",
    "knowledge/security-hygiene.json",
    "knowledge/writing-style.json",
    "knowledge/knowledge-base.json",
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

/// Every `GITHUB_HOSTED_SEED_ONCE` path -- `install.rs`'s own array,
/// duplicated here for the same reason `KIT_OWNED` above is.
const GITHUB_HOSTED_SEED_ONCE: &[&str] = &[".github/workflows/knowledge.yml"];

/// `env!("CARGO_PKG_VERSION")` at THIS test binary's own compile time --
/// the same value `install::kit_version` bakes in for the binary under
/// test (that function's own doc has the account).
fn kit_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[test]
fn files_prints_the_kit_owned_and_seed_once_lists_as_json() {
    let output = houserules().arg("files").output().expect("run files");
    assert!(output.status.success());
    let expected = serde_json::json!({
        "kitOwned": KIT_OWNED,
        "seedOnce": SEED_ONCE,
        "githubHostedSeedOnce": GITHUB_HOSTED_SEED_ONCE,
    });
    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("files prints JSON");
    assert_eq!(actual, expected);
    // Byte-exact against `emit`'s own format (two-space indent, trailing
    // newline), not just structurally equal -- `houserules files` has no
    // `--dir` and no filesystem input, so its output is deterministic
    // enough to pin verbatim.
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        format!("{}\n", serde_json::to_string_pretty(&expected).unwrap())
    );
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

    let mut expected_lines: Vec<String> = KIT_OWNED
        .iter()
        .chain(SEED_ONCE)
        .map(|file| format!("wrote {file}"))
        .collect();
    // `scratch_git_repo` configures no `origin` remote, so the
    // GitHub-hosted-only file is skipped, not written -- a fresh scratch
    // repository is never GitHub-hosted.
    for file in GITHUB_HOSTED_SEED_ONCE {
        expected_lines.push(format!(
            "skipped {file} (origin is not GitHub-hosted; see the migrating-knowledge skill to add your own CI gate)"
        ));
    }
    expected_lines.push("wrote .claude/settings.json".to_string());
    expected_lines.push(".claude/rules/standing-rules.md: written".to_string());
    expected_lines.push(".claude/rules/docs.md: written".to_string());
    expected_lines.push(".claude/rules/tools.md: written".to_string());
    expected_lines.push(".claude/skills/project-knowledge/SKILL.md: written".to_string());
    expected_lines.push(format!("houserules: initialized {}", dir.path().display()));
    expected_lines.push("next: houserules check-knowledge && houserules check-backlog".to_string());
    let actual_lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(actual_lines, expected_lines);

    for file in KIT_OWNED.iter().chain(SEED_ONCE) {
        assert!(dir.path().join(file).is_file(), "{file} was not written");
    }
    for file in GITHUB_HOSTED_SEED_ONCE {
        assert!(
            !dir.path().join(file).exists(),
            "{file} was written into a non-GitHub-hosted target"
        );
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
/// writes it, and its content is the template's own, unchanged.
#[test]
fn init_into_a_fresh_repo_seeds_agents_md_byte_identical_to_the_template() {
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

    let seeded = fs::read(dir.path().join("AGENTS.md")).expect("read seeded AGENTS.md");
    let template =
        fs::read(repo_root().join("template/AGENTS.md")).expect("read template/AGENTS.md");
    assert_eq!(seeded, template);
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
        .args(["init", "--dir"])
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
        .args(["init", "--dir"])
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
        .args(["init", "--dir"])
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
    assert_baselines_cover_every_kit_owned_file(&marker);
}

/// `init` and `update` both stamp a baseline for every `KIT_OWNED` path (a
/// fresh install's own content trivially matches the payload it was just
/// written from), plus one for every kit-shipped knowledge entry --
/// `install.rs`'s own `baseline` module has the full account. This checks
/// the `KIT_OWNED` half, common to every stamping test in both files;
/// knowledge-entry baselines are pinned by the entry-upsert tests instead,
/// since their exact set depends on `template/knowledge/`'s own content.
fn assert_baselines_cover_every_kit_owned_file(marker: &serde_json::Value) {
    let baselines = marker["baselines"]
        .as_object()
        .unwrap_or_else(|| panic!("baselines is not an object in {marker:?}"));
    for file in KIT_OWNED {
        assert!(
            baselines.contains_key(*file),
            "baselines missing {file} in {baselines:?}"
        );
    }
    assert!(
        baselines.len() > KIT_OWNED.len(),
        "expected knowledge-entry baselines alongside the KIT_OWNED ones, got {baselines:?}"
    );
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

/// The refusal text for a target inside a repository but not at its top
/// level, split around the two paths it names.
const INSIDE_REPOSITORY_PREFIX: &str = " is inside the git repository at ";
const INSIDE_REPOSITORY_SUFFIX: &str =
    ", not its top level; houserules installs at the repository root\n";

/// Splits the subdirectory refusal in `stderr` into the target and the top
/// level it prints. The binary prints both in resolved form: the target
/// with symlinks resolved (a spelling that may differ from the one given to
/// `--dir`), and the top level as a prefix of that printed target.
fn parse_subdirectory_refusal(stderr: &str) -> (&str, &str) {
    stderr
        .strip_suffix(INSIDE_REPOSITORY_SUFFIX)
        .and_then(|named_paths| named_paths.split_once(INSIDE_REPOSITORY_PREFIX))
        .unwrap_or_else(|| panic!("not the subdirectory refusal: {stderr:?}"))
}

/// Asserts `stderr` is the subdirectory refusal for `target`, naming
/// `top_level` as the directory above it that holds `.git`. The printed
/// target and top level are compared with `target` and `top_level`
/// canonically, because the binary prints the resolved spelling and a temp
/// path may resolve through a symlink (macOS). The printed top level must
/// be a prefix of the printed target, so the line names a directory the
/// target lies beneath.
fn assert_subdirectory_refusal(stderr: &str, target: &Path, top_level: &Path) {
    let (printed_target, printed_top_level) = parse_subdirectory_refusal(stderr);
    assert_eq!(
        Path::new(printed_target)
            .canonicalize()
            .expect("canonicalize the printed target"),
        target.canonicalize().expect("canonicalize target"),
        "the line names the target"
    );
    assert!(
        Path::new(printed_target).starts_with(printed_top_level)
            && printed_target != printed_top_level,
        "the target lies beneath the printed top level: {stderr:?}"
    );
    assert_eq!(
        Path::new(printed_top_level)
            .canonicalize()
            .expect("canonicalize the printed top level"),
        top_level.canonicalize().expect("canonicalize top level"),
        "the line names the directory that holds .git"
    );
}

#[test]
fn init_refuses_a_subdirectory_naming_the_top_level() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(&sub)
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_subdirectory_refusal(
        &String::from_utf8(output.stderr).expect("utf8 stderr"),
        &sub,
        repo.path(),
    );
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(&sub).unwrap().count(), 0);
}

#[test]
fn init_refuses_a_nested_subdirectory_naming_the_top_level() {
    let repo = scratch_git_repo();
    let nested = repo.path().join("a").join("b");
    fs::create_dir_all(&nested).expect("create nested subdirectory");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(&nested)
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_subdirectory_refusal(
        &String::from_utf8(output.stderr).expect("utf8 stderr"),
        &nested,
        repo.path(),
    );
}

/// Runs `init --dir <target>` with `envs` set in the child, and asserts
/// the refusal that names `top_level` as the directory that holds `.git`
/// above `target`. The line prints the resolved target and the resolved
/// `top_level`, a prefix of it, so no environment variable or git setting
/// changes the directories it names.
fn assert_init_refuses_naming(top_level: &Path, target: &Path, envs: &[(&str, &Path)]) {
    let mut command = houserules();
    command.args(["init", "--dir"]).arg(target);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output().expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_subdirectory_refusal(
        &String::from_utf8(output.stderr).expect("utf8 stderr"),
        target,
        top_level,
    );
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(target).unwrap().count(), 0);
}

/// With `GIT_WORK_TREE` at a directory that does not contain the target,
/// git answers with that directory as the top level (`GIT_WORK_TREE=<other>
/// git rev-parse --show-toplevel` run in `<repo>/sub` prints `<other>`); the
/// refusal still names the directory above the target that holds `.git`.
#[test]
fn init_names_the_ancestor_that_holds_git_under_a_foreign_git_work_tree() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let foreign = tempfile::tempdir().expect("tempdir");
    assert_init_refuses_naming(repo.path(), &sub, &[("GIT_WORK_TREE", foreign.path())]);
}

/// With `GIT_DIR` set, git answers with the directory it runs in as the
/// top level (`GIT_DIR=<repo>/.git git rev-parse --show-toplevel` run in
/// `<repo>/sub` prints `<repo>/sub`).
#[test]
fn init_names_the_ancestor_that_holds_git_under_git_dir() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    assert_init_refuses_naming(repo.path(), &sub, &[("GIT_DIR", &repo.path().join(".git"))]);
}

/// With `GIT_WORK_TREE` at the target itself, git answers with the target
/// as its own top level (`GIT_WORK_TREE=<repo>/sub git rev-parse
/// --show-toplevel` run in `<repo>/sub` prints `<repo>/sub`).
#[test]
fn init_names_the_ancestor_that_holds_git_when_git_work_tree_is_the_target() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    assert_init_refuses_naming(repo.path(), &sub, &[("GIT_WORK_TREE", &sub)]);
}

/// With `core.worktree` in the repository's config naming the target, git
/// answers with the target as its own top level (`git rev-parse
/// --show-toplevel` run in `<repo>/sub` prints `<repo>/sub`).
#[test]
fn init_names_the_ancestor_that_holds_git_under_core_worktree() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let status = Command::new("git")
        .args(["config", "core.worktree"])
        .arg(&sub)
        .current_dir(repo.path())
        .status()
        .expect("run git config");
    assert!(status.success(), "git config core.worktree failed");
    assert_init_refuses_naming(repo.path(), &sub, &[]);
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

/// A subdirectory of a linked worktree names the worktree root, the
/// nearest directory that holds a `.git` entry, not the main repository.
#[test]
fn init_refuses_a_subdirectory_of_a_linked_worktree_naming_the_worktree_root() {
    let (_main, worktree) = repository_with_a_linked_worktree();
    let sub = worktree.join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    assert_init_refuses_naming(&worktree, &sub, &[]);
}

/// `--dir` and the working directory resolve to an absolute target before
/// the refusal is built, so a relative `--dir sub`, `--dir .`, and no
/// `--dir` at all each name the repository root.
#[test]
fn init_resolves_a_relative_target_before_naming_the_top_level() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    for (cwd, args) in [
        (repo.path(), &["init", "--dir", "sub"][..]),
        (&sub, &["init", "--dir", "."][..]),
        (&sub, &["init"][..]),
    ] {
        let output = houserules()
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("run init");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
        let (printed_target, printed_top_level) = parse_subdirectory_refusal(&stderr);
        assert_eq!(
            Path::new(printed_target)
                .canonicalize()
                .expect("canonicalize target"),
            sub.canonicalize().expect("canonicalize subdirectory"),
            "{args:?}: the target is absolute"
        );
        assert!(
            Path::new(printed_target).starts_with(printed_top_level),
            "{args:?}: {stderr:?}"
        );
        assert_eq!(
            Path::new(printed_top_level)
                .canonicalize()
                .expect("canonicalize top level"),
            repo.path().canonicalize().expect("canonicalize repository"),
            "{args:?}"
        );
    }
}

/// Creates `link`, a symlink to `original`.
#[cfg(unix)]
fn symlink(original: &Path, link: &Path) {
    std::os::unix::fs::symlink(original, link).expect("create symlink");
}

/// Runs `init --dir <target>` with no extra environment and asserts the
/// refusal that names `top_level`, printed in resolved form: the printed
/// target is not the spelling given in `target`.
#[cfg(unix)]
fn assert_init_refuses_a_symlinked_target(top_level: &Path, target: &Path) {
    let output = houserules()
        .args(["init", "--dir"])
        .arg(target)
        .output()
        .expect("run init");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_subdirectory_refusal(&stderr, target, top_level);
    let (printed_target, _) = parse_subdirectory_refusal(&stderr);
    assert_ne!(
        printed_target,
        target.display().to_string(),
        "the line prints the resolved spelling, not the symlink"
    );
}

/// A symlink to a repository subdirectory is inside the repository: git
/// itself answers with the repository root for it (`git rev-parse
/// --show-toplevel` run from the link prints `<repo>`). The refusal names
/// that root, never the `git init` advice, which would create a nested
/// repository.
#[cfg(unix)]
#[test]
fn init_names_the_repository_for_a_symlinked_subdirectory_target() {
    let repo = scratch_git_repo();
    let sub = repo.path().join("sub");
    fs::create_dir(&sub).expect("create subdirectory");
    let links = tempfile::tempdir().expect("tempdir");
    let link = links.path().join("link");
    symlink(&sub, &link);
    assert_init_refuses_a_symlinked_target(repo.path(), &link);
    // Nothing was written into the rejected target.
    assert_eq!(fs::read_dir(&sub).unwrap().count(), 0);
}

/// The same for a symlink to a nested subdirectory.
#[cfg(unix)]
#[test]
fn init_names_the_repository_for_a_symlinked_nested_subdirectory_target() {
    let repo = scratch_git_repo();
    let nested = repo.path().join("a").join("b");
    fs::create_dir_all(&nested).expect("create nested subdirectory");
    let links = tempfile::tempdir().expect("tempdir");
    let link = links.path().join("link");
    symlink(&nested, &link);
    assert_init_refuses_a_symlinked_target(repo.path(), &link);
}

/// A subdirectory reached through a symlinked directory above the
/// repository names the repository root in resolved form.
#[cfg(unix)]
#[test]
fn init_names_the_repository_for_a_subdirectory_below_a_symlinked_parent() {
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
    assert_init_refuses_a_symlinked_target(&repo, &parent_link.join("repo").join("sub"));
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
/// `SEED_ONCE` file already present, so `seed`'s own write loop `kept`s it
/// unmodified; the first step that reads its content is the baseline stamp
/// `seed` runs for each `knowledge_topic_files` path (`install.rs`'s own
/// `stamp_topic_baselines`), which fails here with the same named-JSON-error
/// shape `render_and_report` would raise later if this step did not exist.
/// No earlier `install.rs` test reaches this arm: every other error-path
/// test here fails before any file write, at the target's `.git` check, the
/// id-prefix flag, or a pre-existing `.houserules.json` -- none of them
/// plants broken data under `knowledge/` first.
///
/// The expected path is built one component at a time, the way
/// `rules::model::load_base` builds the name it prints, and the way
/// `install.rs`'s own `join_components` now builds the path
/// `stamp_topic_baselines` reads: joining `"knowledge"` onto the root, then
/// each topic file name onto that. A single `join("knowledge/process.json")`
/// diverges from both on Windows alone: `Path::join` inserts the platform's
/// own separator only between components it joins itself, so a literal `/`
/// embedded in one string argument survives untouched, while the code's own
/// two-step join always uses the platform separator.
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

#[test]
fn a_second_init_keeps_seed_once_files_and_settings_but_still_overwrites_kit_owned() {
    let dir = scratch_git_repo();
    let first = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("first init");
    assert!(
        first.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("second init");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success());

    for file in SEED_ONCE {
        assert!(
            stdout.contains(&format!("kept {file}\n")),
            "expected `kept {file}` in:\n{stdout}"
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
            stdout.contains(&format!("wrote {file}\n")),
            "expected `wrote {file}` in:\n{stdout}"
        );
    }
    assert!(stdout.contains("kept .claude/settings.json (hooks already present)\n"));
}

/// `init` over a target that already carries a knowledge topic file of its
/// own -- the documented path for adopting the kit into a codebase with
/// existing knowledge -- keeps that file exactly as found (`kept`,
/// unchanged) and stamps a baseline for none of the kit entries it does not
/// contain: a baseline is a claim that specific content is on disk, and
/// none of the kit's own entries are. Those entries are not lost forever,
/// though -- the very next `update` writes every one of them, since an
/// entry `init` never stamped a baseline for is indistinguishable from one
/// that simply has not arrived at this install yet.
#[test]
fn init_over_a_pre_existing_topic_file_stamps_no_baseline_for_entries_it_never_wrote() {
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
        stdout.contains("kept knowledge/process.json\n"),
        "got:\n{stdout}"
    );

    let marker: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.path().join(".houserules.json")).unwrap())
            .unwrap();
    let baselines = marker["baselines"].as_object().expect("baselines object");
    assert!(
        !baselines.contains_key("process.ask-when-missing"),
        "init stamped a baseline for an entry it never wrote: {baselines:?}"
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
    assert!(!update_stdout.contains("skipped"), "got:\n{update_stdout}");

    let after_update: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("knowledge/process.json")).unwrap(),
    )
    .unwrap();
    let entries = after_update["entries"].as_array().unwrap();
    assert!(
        entries
            .iter()
            .any(|entry| entry["id"] == "process.ask-when-missing"),
        "the next update did not write the kit entries an adopter's own file never had: {entries:?}"
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
        .args(["init", "--dir"])
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
        .args(["init", "--dir"])
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
        .args(["init", "--dir"])
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
        .args(["init", "--dir"])
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
        .args(["init", "--dir"])
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
        .args(["init", "--dir"])
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
