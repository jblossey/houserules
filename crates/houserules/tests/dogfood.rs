//! Integration tests for the kit's own manifest. This repository runs its
//! own kit (`houserules.template-is-the-source`): the root copy of every
//! `KIT_OWNED` file must stay byte-identical to its `template/` source,
//! every `SEED_ONCE`/`.claude/evals/` scenario copy the same, and the
//! generated `.claude/rules/*.md`/skill files must stay fresh relative to
//! `knowledge/`. `quality.pin-copies-byte-exact` governs every comparison
//! here: plain byte equality except where a named transform (the id-prefix
//! rewrite) is the production behavior being pinned.
//!
//! The Rust binary exposes no library API for its own `KIT_OWNED`/
//! `SEED_ONCE` constants (a bare CLI, no `pub` crate surface), so this
//! file drives the same set from the binary's one CLI-exposed form,
//! `houserules files` (`install::cmd_files`), the way `crates/houserules/
//! tests/install.rs`'s own `files_prints_the_kit_owned_and_seed_once_
//! lists_as_json` already pins that command's exact output shape.
//! `RETIRED` has no CLI-exposed form at all, so this file keeps its own
//! small copy, cross-checked against `install.rs`'s own `RETIRED` via its
//! existing `retired_holds_the_shell_tools_and_the_js_engines_they_fronted`
//! unit test rather than duplicating a second pin of the same fact here.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of the pattern `install.rs`/`check_commit.rs` each keep
/// independently (their own docs explain why: a file needing none of
/// `mod common;`'s other helpers still warns the rest of that module dead
/// if pulled in just for this one function).
fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// This checkout's repository root, resolved at compile time so it is
/// correct regardless of the test runner's working directory --
/// `tests/common/mod.rs::repo_root`'s own copy, duplicated here for the
/// same reason as `houserules()` above.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Paths the kit does not ship that `update` deletes from an install if
/// present -- `install.rs`'s own private `RETIRED`, whose exact contents
/// that module's own `retired_holds_the_shell_tools_and_the_js_engines_
/// they_fronted` test already pins; duplicated here (not exported by the
/// binary, and the CLI has no subcommand that echoes it) only so this
/// file's own assertions below can name the paths without hand-writing
/// them a second time inline.
const RETIRED: &[&str] = &[
    "tools/kb.sh",
    "tools/backlog.sh",
    "tools/kb.mjs",
    "tools/backlog.mjs",
    "tools/lib/cli.mjs",
    "tools/lib/json-store.mjs",
];

/// Runs `houserules files` and returns its `kitOwned`/`seedOnce` arrays as
/// owned strings -- the one CLI-exposed form of the binary's own manifest.
fn kit_files() -> (Vec<String>, Vec<String>) {
    let output = houserules().arg("files").output().expect("run files");
    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("files prints JSON");
    let strings = |key: &str| -> Vec<String> {
        value[key]
            .as_array()
            .unwrap_or_else(|| panic!("{key} is an array"))
            .iter()
            .map(|entry| entry.as_str().expect("array entry is a string").to_string())
            .collect()
    };
    (strings("kitOwned"), strings("seedOnce"))
}

/// Every file under `root`, as `/`-joined paths relative to `root`, sorted --
/// Node's own `fs.readdirSync(dir, { recursive: true, withFileTypes: true })`
/// behavior, reimplemented with `std::fs` alone (no new crate: `walkdir` is
/// already a transitive dependency of `rust-embed` but not one this crate
/// declares directly). Built from explicit `/`-joined segments, not
/// `Path::display()`, so the result is byte-identical on every OS in the 3-OS
/// CI matrix regardless of the native path separator; `fs::read_dir` does not
/// filter dot-entries (confirmed by `install.rs`'s own
/// `the_embedded_payload_carries_every_kit_owned_and_seed_once_path` test
/// doc), matching Node's own `recursive: true` behavior this pins against.
fn list_files_recursive(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, prefix: &[String], out: &mut Vec<String>) {
        for entry in
            fs::read_dir(dir).unwrap_or_else(|error| panic!("read_dir {}: {error}", dir.display()))
        {
            let entry = entry.expect("read directory entry");
            let name = entry.file_name().into_string().expect("utf8 filename");
            let mut next = prefix.to_vec();
            next.push(name);
            let file_type = entry.file_type().expect("read file type");
            if file_type.is_dir() {
                walk(&entry.path(), &next, out);
            } else {
                out.push(next.join("/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &[], &mut out);
    out.sort();
    out
}

/// `SEED_ONCE` paths under `.claude/evals/` excluding `record.json`:
/// `record.json` is excluded by design, since the root copy accumulates
/// run sets across evaluations while the template ships only the seed
/// record.
fn eval_scenarios(seed_once: &[String]) -> Vec<String> {
    seed_once
        .iter()
        .filter(|file| {
            file.starts_with(".claude/evals/") && file.as_str() != ".claude/evals/record.json"
        })
        .cloned()
        .collect()
}

/// The two lists -- kit-owned machinery and seed-once project data --
/// name their known members and never overlap.
#[test]
fn kit_owned_manifest_separates_from_seed_once_and_has_no_overlap() {
    let (kit_owned, seed_once) = kit_files();
    for expected in [
        "tools/claude-session-start.sh",
        ".claude/agents/implementer.md",
        ".claude/skills/orchestrating/SKILL.md",
        ".claude/skills/migrating-knowledge/SKILL.md",
        ".githooks/commit-msg",
    ] {
        assert!(
            kit_owned.iter().any(|file| file == expected),
            "kitOwned is missing {expected}"
        );
    }
    for expected in [
        "knowledge/schema.json",
        "backlog/schema.json",
        ".claude/schemas/deliverables.json",
        ".claude/evals/record.json",
        "CLAUDE.md",
    ] {
        assert!(
            seed_once.iter().any(|file| file == expected),
            "seedOnce is missing {expected}"
        );
    }
    let overlap: Vec<&String> = kit_owned
        .iter()
        .filter(|file| seed_once.contains(file))
        .collect();
    assert!(
        overlap.is_empty(),
        "kitOwned and seedOnce overlap: {overlap:?}"
    );
}

/// Every file under `template/` is exactly `kitOwned` (minus `RETIRED`,
/// which this binary's own `KIT_OWNED` never carries, so the filter is a
/// structural no-op today) plus `seedOnce` plus `.claude/settings.json`
/// (seeded or merged specially, never plain-copied).
#[test]
fn kit_owned_and_seed_once_account_for_every_template_file() {
    let (kit_owned, seed_once) = kit_files();
    let mut expected: Vec<String> = kit_owned
        .iter()
        .filter(|file| !RETIRED.contains(&file.as_str()))
        .cloned()
        .collect();
    expected.extend(seed_once.iter().cloned());
    expected.push(".claude/settings.json".to_string());
    expected.sort();

    let actual = list_files_recursive(&repo_root().join("template"));
    assert_eq!(actual, expected);
}

/// Every live `KIT_OWNED` root copy in this checkout is byte-identical to
/// its `template/` source. A hand edit to either side fails here and is
/// lost on the next `houserules update`.
#[test]
fn root_kit_owned_files_equal_their_template_source_byte_for_byte() {
    let (kit_owned, _seed_once) = kit_files();
    let root = repo_root();
    for file in kit_owned
        .iter()
        .filter(|file| !RETIRED.contains(&file.as_str()))
    {
        let root_path = root.join(file);
        assert!(
            root_path.is_file(),
            "{file} is missing at the repository root"
        );
        let root_bytes =
            fs::read(&root_path).unwrap_or_else(|error| panic!("read {file}: {error}"));
        let template_bytes = fs::read(root.join("template").join(file))
            .unwrap_or_else(|error| panic!("read template/{file}: {error}"));
        assert_eq!(
            root_bytes, template_bytes,
            "{file} diverged from template/{file}"
        );
    }
}

/// `install.rs`'s own `retired_holds_the_shell_tools_and_the_js_engines_
/// they_fronted` already pins `RETIRED`'s exact contents
/// (`houserules.template-is-the-source`'s own knowledge entry names it);
/// this test confirms neither retired path lingers in this checkout's own
/// worktree, root or template.
#[test]
fn retired_paths_are_absent_from_both_root_and_template() {
    let root = repo_root();
    for file in RETIRED {
        assert!(
            !root.join(file).exists(),
            "{file} is still present at the repository root"
        );
        assert!(
            !root.join("template").join(file).exists(),
            "template/{file} is still present"
        );
    }
}

/// This repository's own `.houserules.json` names `env!("CARGO_PKG_VERSION")`
/// and the `HR` backlog id prefix -- proof this repository dogfoods its own
/// `init`/`update` output rather than a hand-written stamp. `baselines` is not
/// pinned here byte-for-byte: it holds one hash per `KIT_OWNED` file and
/// kit-shipped knowledge entry, which grows with the payload itself, so this
/// only checks that `update` has stamped one at all; the two `install.rs` unit
/// tests inject their own list directly and cover the stamping mechanism's own
/// behavior.
///
/// `overrides` IS pinned exactly, because these three paths are load-
/// bearing: without them, `update --dir .` here would backfill each one.
/// `backlog/items/general.json` is absent because this repository's own
/// backlog items live at `backlog/items/kit.json` instead. `.github/
/// workflows/knowledge.yml` is absent because this repository's CI is its
/// own workflow set (`ci.yml` and the rest), not the generic seeded gate.
/// `docs/README.md` is absent because this directory already holds this
/// repository's real specs, plans, and design notes in place of the
/// generic starter file.
#[test]
fn houserules_json_stamps_the_installed_version_and_the_hr_id_prefix() {
    let root = repo_root();
    let stamp: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".houserules.json")).expect("read .houserules.json"),
    )
    .expect("parse .houserules.json");
    assert_eq!(
        stamp["version"],
        serde_json::json!(env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(stamp["idPrefix"], serde_json::json!("HR"));
    assert!(stamp["baselines"].is_object(), "got {stamp}");
    assert_eq!(
        stamp["overrides"],
        serde_json::json!([
            "backlog/items/general.json",
            ".github/workflows/knowledge.yml",
            "docs/README.md",
        ]),
        "got {stamp}"
    );
}

/// `.claude/schemas/deliverables.json` is `SEED_ONCE` -- `update` never
/// writes it, so root and template are hand-synced except for the one
/// designed difference, `init`'s id-prefix rewrite
/// (`WI-` -> `<idPrefix>-`). `quality.pin-copies-byte-exact`: the
/// assertion applies that exact production transform to the template side
/// rather than dropping the differing text, so the two cannot silently
/// drift apart in any other way.
#[test]
fn deliverables_schema_equals_template_source_with_the_id_prefix_rewrite() {
    let root = repo_root();
    let stamp: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".houserules.json")).expect("read .houserules.json"),
    )
    .expect("parse .houserules.json");
    let id_prefix = stamp["idPrefix"].as_str().expect("idPrefix is a string");
    const SCHEMA_PATH: &str = ".claude/schemas/deliverables.json";
    let root_schema = fs::read_to_string(root.join(SCHEMA_PATH)).expect("read root schema");
    let template_schema =
        fs::read_to_string(root.join("template").join(SCHEMA_PATH)).expect("read template schema");
    assert_eq!(
        root_schema,
        template_schema.replace("WI-", &format!("{id_prefix}-"))
    );
}

/// The filter this file's own byte-parity test below relies on is never
/// vacuously empty.
#[test]
fn seed_once_derives_at_least_one_eval_scenario() {
    let (_kit_owned, seed_once) = kit_files();
    assert!(!eval_scenarios(&seed_once).is_empty());
}

/// Unlike the deliverables schema above, `.claude/evals/*.json` entries
/// are not `PREFIXED` -- `init` copies them unchanged, so the pin here is
/// plain byte equality, no transform.
#[test]
fn seed_once_eval_scenario_copies_equal_template_source_byte_for_byte() {
    let (_kit_owned, seed_once) = kit_files();
    let root = repo_root();
    for file in eval_scenarios(&seed_once) {
        let root_bytes =
            fs::read(root.join(&file)).unwrap_or_else(|error| panic!("read {file}: {error}"));
        let template_bytes = fs::read(root.join("template").join(&file))
            .unwrap_or_else(|error| panic!("read template/{file}: {error}"));
        assert_eq!(
            root_bytes, template_bytes,
            "{file} diverged from template/{file}"
        );
    }
}

/// This repository's own `.claude/rules/*.md` and knowledge skill stay
/// fresh relative to `knowledge/`. Runs `houserules render --check`
/// directly against this checkout's live root, read-only: no worktree
/// indirection is needed, since the binary under test here already is
/// this repository's own rendering authority
/// (`houserules.template-is-the-source`), so a direct `--check` run
/// cannot mutate anything a worktree copy would otherwise be protecting.
#[test]
fn generated_files_are_fresh_relative_to_knowledge() {
    let output = houserules()
        .args(["render", "--check", "--dir"])
        .arg(repo_root())
        .output()
        .expect("run render --check");
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).expect("utf8 stdout"),
        "render: up to date\n"
    );
}
