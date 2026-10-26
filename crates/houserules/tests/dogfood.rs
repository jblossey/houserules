//! Integration tests for the kit's own manifest. This repository runs its
//! own kit (`houserules.template-is-the-source`): the root copy of every
//! `KIT_OWNED` file must stay byte-identical to its `template/` source,
//! every eval scenario copy of the reference workflow the same, and the
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
//! small copy, cross-checked against `install/mod.rs`'s own `RETIRED` via its
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
/// Sets `HOUSERULES_SKIP_SELF_UPDATE` so `update`'s self-update phase
/// (`selfupdate.rs`) never runs here -- `tests/update.rs`'s own copy of
/// this helper has the full account.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
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
/// present -- `install/mod.rs`'s own private `RETIRED`, whose exact contents
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

/// The file classes that `houserules files` prints.
struct KitFiles {
    kit_owned: Vec<String>,
    seed_once: Vec<String>,
    pack_copies: Vec<String>,
    d3_files: Vec<String>,
    tracker_files: Vec<String>,
    workflow_files: Vec<String>,
    workflow_data: Vec<String>,
}

/// Runs `houserules files` and returns its classes as owned strings -- the
/// one CLI-exposed form of the binary's own manifest.
fn kit_files() -> KitFiles {
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
    KitFiles {
        kit_owned: strings("kitOwned"),
        seed_once: strings("seedOnce"),
        pack_copies: strings("packCopies"),
        d3_files: strings("d3Files"),
        tracker_files: strings("trackerFiles"),
        workflow_files: strings("workflowFiles"),
        workflow_data: strings("workflowData"),
    }
}

/// Every file under `root`, as `/`-joined paths relative to `root`, sorted --
/// Node's own `fs.readdirSync(dir, { recursive: true, withFileTypes: true })`
/// behavior, reimplemented with `std::fs` alone (no new crate: `walkdir` is
/// already a transitive dependency of `rust-embed` but not one this crate
/// declares directly). Built from explicit `/`-joined segments, not
/// `Path::display()`, so the result is byte-identical on every OS in the 3-OS
/// CI matrix regardless of the native path separator; `fs::read_dir` does not
/// filter dot-entries (confirmed by `install/mod.rs`'s own
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

/// The directory of the reference workflow in `template/`.
const WORKFLOW_DIR: &str = "workflows/reference";

/// The eval scenarios of the workflow: its files under `.claude/evals/`. The
/// data file `record.json` is no workflow file, since the root copy
/// accumulates run sets across evaluations while the package ships only the
/// empty record.
fn eval_scenarios(workflow_files: &[String]) -> Vec<String> {
    workflow_files
        .iter()
        .filter(|file| file.starts_with(".claude/evals/"))
        .cloned()
        .collect()
}

/// The classes -- kit-owned machinery, seed-once project data, the D3 files,
/// the pack copies, and the tracker files -- name their known members, and
/// the kit-owned and seed-once lists never overlap.
#[test]
fn kit_owned_manifest_separates_from_seed_once_and_has_no_overlap() {
    let files = kit_files();
    for expected in [
        "tools/claude-session-start.sh",
        ".claude/skills/migrating-knowledge/SKILL.md",
        ".claude/skills/houserules-step/SKILL.md",
        "tools/houserules-step.sh",
    ] {
        assert!(
            files.kit_owned.iter().any(|file| file == expected),
            "kitOwned is missing {expected}"
        );
    }
    for moved in [
        ".claude/agents/implementer.md",
        ".claude/skills/orchestrating/SKILL.md",
        ".githooks/commit-msg",
    ] {
        assert!(
            files.workflow_files.iter().any(|file| file == moved),
            "workflowFiles is missing {moved}"
        );
        assert!(
            !files.kit_owned.iter().any(|file| file == moved),
            "kitOwned still holds {moved}"
        );
    }
    assert_eq!(files.workflow_data, [".claude/evals/record.json"]);
    for expected in ["knowledge/areas.json", "AGENTS.md", "CLAUDE.md"] {
        assert!(
            files.seed_once.iter().any(|file| file == expected),
            "seedOnce is missing {expected}"
        );
    }
    let overlap: Vec<&String> = files
        .kit_owned
        .iter()
        .filter(|file| files.seed_once.contains(file))
        .collect();
    assert!(
        overlap.is_empty(),
        "kitOwned and seedOnce overlap: {overlap:?}"
    );
    assert_eq!(
        files.d3_files,
        [
            "knowledge/schema.json",
            ".claude/schemas/deliverables.json",
            ".github/workflows/knowledge.yml"
        ]
    );
    assert!(
        files
            .tracker_files
            .contains(&"backlog/schema.json".to_string()),
        "trackerFiles is missing backlog/schema.json"
    );
    let other_classes: Vec<&String> = files
        .kit_owned
        .iter()
        .chain(&files.seed_once)
        .filter(|file| {
            files.d3_files.contains(file)
                || files.pack_copies.contains(file)
                || files.tracker_files.contains(file)
                || files.workflow_files.contains(file)
                || files.workflow_data.contains(file)
        })
        .collect();
    assert!(
        other_classes.is_empty(),
        "a file of another class is in kitOwned or seedOnce too: {other_classes:?}"
    );
}

/// Every file under `template/` is exactly `kitOwned` (minus `RETIRED`,
/// which this binary's own `KIT_OWNED` never carries, so the filter is a
/// structural no-op today) plus `seedOnce` plus the D3 files plus the tracker
/// files plus `.claude/settings.json` (seeded or merged specially, never
/// plain-copied) plus the pack files, which `packCopies` names by their path
/// in an install (`knowledge/packs/<name>.json`) and
/// `template/packs/<name>.json` holds, plus the reference workflow: its
/// manifest and each workflow file and data file at its install path below
/// `template/workflows/reference/`.
#[test]
fn kit_owned_and_seed_once_account_for_every_template_file() {
    let files = kit_files();
    let mut expected: Vec<String> = files
        .kit_owned
        .iter()
        .filter(|file| !RETIRED.contains(&file.as_str()))
        .cloned()
        .collect();
    expected.extend(files.seed_once.iter().cloned());
    expected.extend(files.d3_files.iter().cloned());
    expected.extend(files.tracker_files.iter().cloned());
    expected.push(".claude/settings.json".to_string());
    expected.extend(files.pack_copies.iter().map(|copy| {
        copy.strip_prefix("knowledge/")
            .unwrap_or_else(|| panic!("{copy} lies below knowledge/"))
            .to_string()
    }));
    expected.push(format!("{WORKFLOW_DIR}/workflow.json"));
    expected.extend(
        files
            .workflow_files
            .iter()
            .chain(&files.workflow_data)
            .map(|file| format!("{WORKFLOW_DIR}/{file}")),
    );
    expected.sort();
    expected.dedup();

    let actual = list_files_recursive(&repo_root().join("template"));
    assert_eq!(actual, expected);
}

/// Every live `KIT_OWNED` root copy in this checkout is byte-identical to
/// its `template/` source, except a path this repo's own `.houserules.json`
/// `ownedPaths` list declares its own (`.githooks/commit-msg`: this
/// repository keeps its stricter, trailer-gating hook while the template's
/// own copy drops that gate -- T2 review ruling 5.81). A hand edit to a
/// non-owned path fails here; `houserules update` keeps the locally
/// modified copy and prints `kept <file> (locally modified)`.
#[test]
fn root_kit_owned_files_equal_their_template_source_byte_for_byte() {
    let files = kit_files();
    let root = repo_root();
    let stamp: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".houserules.json")).expect("read .houserules.json"),
    )
    .expect("parse .houserules.json");
    let owned: Vec<&str> = stamp["ownedPaths"]
        .as_array()
        .expect("ownedPaths is an array")
        .iter()
        .map(|v| v.as_str().expect("ownedPaths entry is a string"))
        .collect();
    for file in files
        .kit_owned
        .iter()
        .filter(|file| !RETIRED.contains(&file.as_str()))
        .filter(|file| !owned.contains(&file.as_str()))
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

/// `install/mod.rs`'s own `retired_holds_the_shell_tools_and_the_js_engines_
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

/// This repository's own `.houserules.json` names `env!("CARGO_PKG_VERSION")`,
/// the `HR` backlog id prefix, and the 2.0 install schema -- proof this
/// repository dogfoods its own `init`/`update` output rather than a
/// hand-written stamp. `baselines` is not pinned here byte-for-byte: it holds
/// one hash per `KIT_OWNED` path and per D3 file, which grows with the
/// payload itself, so this only checks that `update` has stamped one at all.
///
/// `ownedPaths` IS pinned exactly. Its entries are load-bearing: without
/// them, `update --dir .` here would backfill or rewrite each one.
/// `backlog/items/general.json` is absent because this repository's own
/// backlog items live at `backlog/items/kit.json` instead. `.github/
/// workflows/knowledge.yml` is absent because this repository's CI is its
/// own workflow set (`ci.yml` and the rest), not the generic seeded gate.
/// `docs/README.md` is absent because this directory already holds this
/// repository's real specs, plans, and design notes in place of the
/// generic starter file. `.githooks/commit-msg` keeps this repository's
/// trailer gate. The entries this repository words in its own, fuller body
/// are overrides in `knowledge/overrides.json`, which
/// `check-knowledge` lists.
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
    assert_eq!(
        stamp["schema"],
        serde_json::json!("houserules/install@2.4.0")
    );
    assert!(stamp["baselines"].is_object(), "got {stamp}");
    assert!(stamp.get("overrides").is_none(), "got {stamp}");
    assert_eq!(
        stamp["ownedPaths"],
        serde_json::json!([
            "backlog/items/general.json",
            ".github/workflows/knowledge.yml",
            "docs/README.md",
            ".githooks/commit-msg",
        ]),
        "got {stamp}"
    );
}

/// `.claude/schemas/deliverables.json` is a D3 file: `update --dir .` keeps
/// this repository's copy at the content of the kit. The one designed
/// difference is the `backlogId` pattern: the template holds the default of an
/// install with no tracker (spec 3.9), and `init --tracker <adapter>` and
/// `update` write the pattern of the adapter.
/// `quality.pin-copies-byte-exact`: the assertion compares this repository's
/// schema with the schema that `init` writes, through the binary, on a scratch
/// install that has this repository's `idPrefix` and tracker. The production
/// transform makes the difference, so the comparison allows none.
#[test]
fn deliverables_schema_equals_template_source_with_the_id_prefix_rewrite() {
    let root = repo_root();
    let stamp: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".houserules.json")).expect("read .houserules.json"),
    )
    .expect("parse .houserules.json");
    let id_prefix = stamp["idPrefix"].as_str().expect("idPrefix is a string");
    let adapter = stamp["tracker"]["adapter"]
        .as_str()
        .expect("this repository has a tracker");
    const SCHEMA_PATH: &str = ".claude/schemas/deliverables.json";
    let scratch = tempfile::tempdir().expect("tempdir");
    let repository = Command::new("git")
        .args(["init", "-q"])
        .current_dir(scratch.path())
        .status()
        .expect("run git init");
    assert!(repository.success(), "git init failed");
    let mut init = houserules();
    init.args(["init", "--id-prefix", id_prefix, "--tracker", adapter])
        .arg("--dir")
        .arg(scratch.path());
    if let Some(repo) = stamp["tracker"]["repo"].as_str() {
        init.args(["--repo", repo]);
    }
    let output = init.output().expect("run init");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let root_schema = fs::read_to_string(root.join(SCHEMA_PATH)).expect("read root schema");
    let written =
        fs::read_to_string(scratch.path().join(SCHEMA_PATH)).expect("read the written schema");
    assert_eq!(root_schema, written);
}

/// The filter this file's own byte-parity test below relies on is never
/// vacuously empty.
#[test]
fn the_workflow_derives_at_least_one_eval_scenario() {
    assert!(!eval_scenarios(&kit_files().workflow_files).is_empty());
}

/// Unlike the deliverables schema above, `.claude/evals/*.json` entries
/// are not `PREFIXED` -- `init --workflow` copies them unchanged, so the pin
/// here is plain byte equality, no transform. The source of a scenario is its
/// path below `template/workflows/reference/`.
#[test]
fn eval_scenario_copies_equal_the_package_source_byte_for_byte() {
    let root = repo_root();
    for file in eval_scenarios(&kit_files().workflow_files) {
        let root_bytes =
            fs::read(root.join(&file)).unwrap_or_else(|error| panic!("read {file}: {error}"));
        let source_bytes = fs::read(root.join("template").join(WORKFLOW_DIR).join(&file))
            .unwrap_or_else(|error| panic!("read the package copy of {file}: {error}"));
        assert_eq!(
            root_bytes, source_bytes,
            "{file} diverged from its package source"
        );
    }
}

/// This repository runs the reference workflow (batch 34, spec section 10):
/// its `.houserules.json` pins the workflow files, and each root copy of a
/// pinned file equals the file of the package byte for byte
/// (`quality.pin-copies-byte-exact`). A path in `ownedPaths` is the project's
/// own and no copy, and a path pinned `null` holds a file of the project. The
/// test fails when the marker holds no `workflow` key, so it cannot pass with
/// nothing to compare.
#[test]
fn each_root_copy_of_a_pinned_workflow_file_equals_its_package_source_byte_for_byte() {
    let root = repo_root();
    let stamp: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join(".houserules.json")).expect("read .houserules.json"),
    )
    .expect("parse .houserules.json");
    let pins = stamp["workflow"]["files"].as_object().unwrap_or_else(|| {
        panic!(
            "this repository runs the reference workflow: .houserules.json holds no workflow pins"
        )
    });
    let owned: Vec<&str> = stamp["ownedPaths"]
        .as_array()
        .expect("ownedPaths is an array")
        .iter()
        .map(|v| v.as_str().expect("ownedPaths entry is a string"))
        .collect();
    let mut compared = 0;
    for (file, pin) in pins {
        if pin.is_null() || owned.contains(&file.as_str()) {
            continue;
        }
        let root_bytes =
            fs::read(root.join(file)).unwrap_or_else(|error| panic!("read {file}: {error}"));
        let source_bytes = fs::read(root.join("template").join(WORKFLOW_DIR).join(file))
            .unwrap_or_else(|error| panic!("read the package copy of {file}: {error}"));
        assert_eq!(
            root_bytes, source_bytes,
            "{file} diverged from its package source"
        );
        compared += 1;
    }
    assert!(compared > 0, "no pinned workflow file to compare");
}

/// The role of a model is configured in `.houserules.json` (`workflow.models`),
/// and no template names one (spec 4.1): the root copies of the agent
/// templates carry no `model:` line.
#[test]
fn the_root_agent_templates_carry_no_model_line() {
    let agents = repo_root().join(".claude/agents");
    let mut checked = 0;
    for entry in fs::read_dir(&agents).expect("read .claude/agents") {
        let path = entry.expect("read an entry").path();
        let text = fs::read_to_string(&path).expect("read an agent template");
        assert!(
            !text.lines().any(|line| line.starts_with("model:")),
            "{} names a model",
            path.display()
        );
        checked += 1;
    }
    assert!(checked > 0, "no agent template to check");
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

/// An install seeded before 1.3.0 keeps its old sequential-handoff line in
/// `AGENTS.md`, because `AGENTS.md` is `SEED_ONCE` and `update` never rewrites
/// it; an install seeded by 1.0.0 keeps that line in `CLAUDE.md` instead. The
/// seeded `AGENTS.md` no longer carries the parallel rule: the `agentsBlock` of
/// the reference workflow does, and `render` writes it into the `AGENTS.md`
/// block of the install that has the workflow. The orchestrating skill is the
/// kit-owned file that `update` does rewrite, so it carries the clause of that
/// rule that the replacement line needs. The two copies of the clause stay
/// equal byte for byte (`quality.pin-copies-byte-exact`).
#[test]
fn orchestrating_skill_carries_the_agents_md_parallel_line_verbatim() {
    let template = repo_root().join("template");
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(template.join(WORKFLOW_DIR).join("workflow.json"))
            .expect("read the workflow manifest"),
    )
    .expect("parse the workflow manifest");
    let block = manifest["agentsBlock"]
        .as_str()
        .expect("the manifest carries an agentsBlock");
    let start = block
        .find("run handoffs in parallel")
        .expect("the agentsBlock carries the parallel rule");
    let end_marker = "each in its own worktree";
    let end = block[start..]
        .find(end_marker)
        .map(|offset| start + offset + end_marker.len())
        .expect("the agentsBlock names the worktree rule");
    let clause = &block[start..end];
    let seed = fs::read_to_string(template.join("AGENTS.md")).expect("read template AGENTS.md");
    assert!(
        !seed.contains(clause),
        "the seeded AGENTS.md carries the parallel rule that the agentsBlock owns: {clause}"
    );
    let skill = fs::read_to_string(
        template
            .join(WORKFLOW_DIR)
            .join(".claude/skills/orchestrating/SKILL.md"),
    )
    .expect("read the package orchestrating skill");
    assert!(
        skill.contains(clause),
        "the orchestrating skill does not carry the agentsBlock clause verbatim: {clause}"
    );
}
