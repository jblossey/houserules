//! `init` writes the 2.0 install (batch 33, spec sections 3, 7, and 9): the
//! marker `houserules/install@2.0.0`, the five kit packs with their pins,
//! no topic file, and the Claude Code files only with `--target claude`.
//! Each case runs the compiled binary against a scratch git repository.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

mod tree_snapshot;

use tree_snapshot::{assert_same_tree, snapshot};

/// A `Command` for the compiled `houserules` binary under test. This file
/// keeps its own copy of the helper, as every `tests/*.rs` file does.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// This checkout's repository root.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// A fresh scratch directory with `git init` already run.
fn scratch_git_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    dir
}

/// Runs `init --dir <dir>` with `extra` arguments.
fn init(dir: &Path, extra: &[&str]) -> Output {
    houserules()
        .args(["init", "--dir"])
        .arg(dir)
        .args(extra)
        .output()
        .expect("run init")
}

/// Runs `init`, requires exit 0, and returns the stdout lines.
fn init_lines(dir: &Path, extra: &[&str]) -> Vec<String> {
    let output = init(dir, extra);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    lines_of(output.stdout)
}

/// The lines of captured stdout.
fn lines_of(stdout: Vec<u8>) -> Vec<String> {
    String::from_utf8(stdout)
        .expect("utf8 stdout")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Requires `file` below `dir` to hold exactly `expected`. A failure names
/// the file and prints no content.
fn assert_file_holds(dir: &Path, file: &str, expected: &[u8]) {
    assert!(
        fs::read(dir.join(file)).expect("read the file") == expected,
        "{file} changed"
    );
}

/// The marker of the install at `dir`, parsed.
fn marker(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join(".houserules.json")).expect("read marker"))
        .expect("parse the marker")
}

/// The keys of a JSON object, in file order.
fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// The five kit packs under `template/packs/`.
const PACKS: [&str; 5] = [
    "knowledge-base",
    "process",
    "quality",
    "security-hygiene",
    "writing-style",
];

/// The Claude Code files that `init` leaves out without `--target claude`.
const CLAUDE_FILES: [&str; 4] = [
    "tools/claude-session-start.sh",
    ".claude/agents/implementer.md",
    ".claude/agents/task-reviewer.md",
    ".claude/agents/branch-reviewer.md",
];

/// The `KIT_OWNED` files that every install gets.
const OTHER_KIT_OWNED: [&str; 4] = [
    ".githooks/commit-msg",
    ".claude/skills/orchestrating/SKILL.md",
    ".claude/skills/finishing-a-feature/SKILL.md",
    ".claude/skills/migrating-knowledge/SKILL.md",
];

/// The one line `init` prints for a Claude Code user, when the Claude
/// renderer is off.
const ENABLE_LINE: &str =
    "for Claude Code: run houserules init --target claude in the install root";

/// The SHA-256 of a pack's canonical form: compact JSON of the parsed value
/// with every object's keys sorted (spec L3), computed here without the
/// binary's own function.
fn canonical_sha256(pack: &str) -> String {
    fn sorted(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                keys.sort();
                let mut out = Map::new();
                for key in keys {
                    out.insert(key.clone(), sorted(&map[key]));
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
            other => other.clone(),
        }
    }
    let text = fs::read_to_string(
        repo_root()
            .join("template/packs")
            .join(format!("{pack}.json")),
    )
    .expect("read the template pack");
    let parsed: Value = serde_json::from_str(&text).expect("parse the template pack");
    let bytes = serde_json::to_vec(&sorted(&parsed)).expect("serialize");
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The version of a template pack.
fn pack_version(pack: &str) -> String {
    let text = fs::read_to_string(
        repo_root()
            .join("template/packs")
            .join(format!("{pack}.json")),
    )
    .expect("read the template pack");
    let parsed: Value = serde_json::from_str(&text).expect("parse the template pack");
    parsed["version"].as_str().expect("a version").to_string()
}

/// The areas that a fresh `init` records in the pin of `pack`: the seeded
/// areas of `knowledge/areas.json`, each in the pin of the first pack that
/// declares it.
fn seeded_areas(pack: &str) -> Vec<&'static str> {
    match pack {
        "knowledge-base" => vec!["docs"],
        "process" => vec!["global", "process"],
        "quality" => vec!["tools"],
        _ => Vec::new(),
    }
}

/// The marker `init` writes: the 2.0 keys in the layout's order, one pin for
/// each kit pack, no entry baseline, and no `overrides` or `ownedPaths` key.
#[test]
fn init_writes_the_2_0_marker() {
    let dir = scratch_git_repo();
    init_lines(dir.path(), &[]);

    let marker = marker(dir.path());
    assert_eq!(
        keys(&marker),
        [
            "schema",
            "version",
            "idPrefix",
            "packs",
            "renderers",
            "baselines"
        ]
    );
    assert_eq!(marker["schema"], json!("houserules/install@2.0.0"));
    assert_eq!(marker["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(marker["idPrefix"], json!("WI"));
    assert_eq!(marker["renderers"], json!([]));
    let expected_packs: Value = Value::Object(
        PACKS
            .iter()
            .map(|pack| {
                (
                    pack.to_string(),
                    json!({
                        "version": pack_version(pack),
                        "source": "kit",
                        "sha256": canonical_sha256(pack),
                        "addedAreas": seeded_areas(pack),
                    }),
                )
            })
            .collect(),
    );
    assert_eq!(marker["packs"], expected_packs);
    let mut baselines = keys(&marker["baselines"]);
    baselines.sort();
    let mut expected: Vec<String> = OTHER_KIT_OWNED
        .iter()
        .chain(["knowledge/schema.json"].iter())
        .map(|file| file.to_string())
        .collect();
    expected.sort();
    assert_eq!(baselines, expected);
}

/// A fresh `init` is followed by `check-knowledge` with no finding (spec 16,
/// C16): the seeded `tools/**` glob matches no file while the Claude files
/// stay out, and the areas that `init` seeds are the areas that the kit
/// packs added, each recorded in the pin of the first pack that declares it.
#[test]
fn a_fresh_init_passes_check_knowledge_and_records_the_seeded_areas_in_the_pins() {
    for extra in [&[][..], &["--target", "claude"][..]] {
        let dir = scratch_git_repo();
        init_lines(dir.path(), extra);

        let checked = houserules()
            .arg("check-knowledge")
            .current_dir(dir.path())
            .output()
            .expect("run check-knowledge");

        assert_eq!(
            (
                checked.status.code(),
                String::from_utf8_lossy(&checked.stdout).into_owned(),
                String::from_utf8_lossy(&checked.stderr).into_owned()
            ),
            (Some(0), "knowledge: ok\n".to_string(), String::new()),
            "init {extra:?}"
        );
        let marker = marker(dir.path());
        for pack in PACKS {
            assert_eq!(
                marker["packs"][pack]["addedAreas"],
                json!(seeded_areas(pack)),
                "init {extra:?}: {pack}"
            );
        }
    }
}

/// An `areas.json` that the project already holds is the project's own:
/// `init` records no area of it as added by a pack, whatever globs it holds.
#[test]
fn init_records_no_area_of_an_areas_file_the_project_already_holds() {
    let dir = scratch_git_repo();
    fs::create_dir_all(dir.path().join("knowledge")).expect("create knowledge/");
    fs::write(
        dir.path().join("knowledge/areas.json"),
        "{\n  \"global\": {\"paths\": []},\n  \"process\": {\"paths\": []},\n  \"docs\": {\"paths\": [\"docs/**\", \"AGENTS.md\", \"knowledge/**\", \"backlog/**\", \".claude/**\"]},\n  \"tools\": {\"paths\": [\"tools/**\"]}\n}\n",
    )
    .expect("write areas.json");

    init_lines(dir.path(), &[]);

    let marker = marker(dir.path());
    for pack in PACKS {
        assert_eq!(marker["packs"][pack]["addedAreas"], json!([]), "{pack}");
    }
}

/// Each pack copy is the template's pack file, byte for byte (emit form,
/// HR-162), and `init` writes none of the five 1.x topic files.
#[test]
fn init_writes_the_five_pack_copies_and_no_topic_file() {
    let dir = scratch_git_repo();
    let lines = init_lines(dir.path(), &[]);

    for pack in PACKS {
        let copy = format!("knowledge/packs/{pack}.json");
        assert_eq!(
            fs::read(dir.path().join(&copy)).expect("read the copy"),
            fs::read(
                repo_root()
                    .join("template/packs")
                    .join(format!("{pack}.json"))
            )
            .expect("read the template pack"),
            "{copy}"
        );
        assert!(lines.contains(&format!("wrote {copy}")), "{lines:?}");
        assert!(
            !dir.path().join(format!("knowledge/{pack}.json")).exists(),
            "knowledge/{pack}.json is a 1.x topic file"
        );
    }
}

/// Without `--target claude`, `init` leaves out the agent templates, the
/// session-start script, and the hook entry (spec 7, C9), prints the one
/// line that enables them, and installs everything else as in 1.4.0.
#[test]
fn init_without_target_claude_leaves_out_the_claude_files() {
    let dir = scratch_git_repo();
    let lines = init_lines(dir.path(), &[]);

    for file in CLAUDE_FILES {
        assert!(!dir.path().join(file).exists(), "{file} was written");
        assert!(!lines.iter().any(|line| line.contains(file)), "{lines:?}");
    }
    assert!(
        !dir.path().join(".claude/settings.json").exists(),
        "the hook entry needs the Claude renderer"
    );
    for file in OTHER_KIT_OWNED {
        assert!(dir.path().join(file).is_file(), "{file} was not written");
    }
    for file in [
        "knowledge/schema.json",
        "knowledge/areas.json",
        "backlog/schema.json",
        ".claude/schemas/deliverables.json",
        ".claude/evals/record.json",
        "AGENTS.md",
        "CLAUDE.md",
    ] {
        assert!(dir.path().join(file).is_file(), "{file} was not written");
    }
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.as_str() == ENABLE_LINE)
            .count(),
        1,
        "{lines:?}"
    );
}

/// With `--target claude`, `init` writes every kit-owned file, merges the
/// hook entry, records `claude`, and prints no enable line.
#[test]
fn init_target_claude_writes_every_claude_file_and_the_hook_entry() {
    let dir = scratch_git_repo();
    let lines = init_lines(dir.path(), &["--target", "claude"]);

    for file in CLAUDE_FILES.iter().chain(OTHER_KIT_OWNED.iter()) {
        assert_eq!(
            fs::read(dir.path().join(file)).expect("read the file"),
            fs::read(repo_root().join("template").join(file)).expect("read the template"),
            "{file}"
        );
        assert!(lines.contains(&format!("wrote {file}")), "{lines:?}");
    }
    let settings: Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).expect("read settings"),
    )
    .expect("parse settings");
    let matchers: Vec<&str> = settings["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart is an array")
        .iter()
        .map(|entry| entry["matcher"].as_str().expect("a matcher"))
        .collect();
    assert_eq!(matchers, ["compact", "startup|resume|clear|fork"]);
    let marker = marker(dir.path());
    assert_eq!(marker["renderers"], json!(["claude"]));
    let baselines = marker["baselines"].as_object().expect("baselines");
    for file in CLAUDE_FILES {
        assert!(baselines.contains_key(file), "baselines miss {file}");
    }
    assert!(!lines.iter().any(|line| line == ENABLE_LINE), "{lines:?}");
}

/// `init --target claude` on an existing install adds `claude` to
/// `renderers`, seeds the Claude files that are missing, keeps every file
/// that exists (a modified agent template, a modified skill, a modified file
/// that `ownedPaths` lists, and the other keys of `.claude/settings.json`
/// included), keeps the baseline of each file it kept, and renders.
#[test]
fn init_target_claude_on_an_existing_install_seeds_what_is_missing_and_keeps_the_rest() {
    let dir = scratch_git_repo();
    init_lines(dir.path(), &[]);
    let changed_skill = ".claude/skills/orchestrating/SKILL.md";
    let owned_hook = ".githooks/commit-msg";
    fs::write(dir.path().join(changed_skill), b"the project's own skill").expect("write the skill");
    fs::write(dir.path().join(owned_hook), b"the project's own hook").expect("write the hook");
    let mut stamp = marker(dir.path());
    stamp["ownedPaths"] = json!([owned_hook]);
    write_marker(dir.path(), &stamp);
    let baselines = marker(dir.path())["baselines"].clone();
    let agent = ".claude/agents/implementer.md";
    fs::create_dir_all(dir.path().join(".claude/agents")).expect("create the directory");
    fs::write(dir.path().join(agent), b"the project's own agent").expect("write the agent");
    fs::write(
        dir.path().join(".claude/settings.json"),
        "{\n  \"permissions\": {\"allow\": [\"Bash(git status)\"]}\n}\n",
    )
    .expect("write settings");
    let skill = dir.path().join(".claude/skills/project-knowledge/SKILL.md");
    assert!(
        !skill.exists(),
        "the Claude renderer is off before the target"
    );

    let lines = init_lines(dir.path(), &["--target", "claude"]);

    assert_eq!(marker(dir.path())["renderers"], json!(["claude"]));
    assert_file_holds(dir.path(), agent, b"the project's own agent");
    assert!(lines.contains(&format!("kept {agent}")), "{lines:?}");
    for file in [
        "tools/claude-session-start.sh",
        ".claude/agents/task-reviewer.md",
    ] {
        assert!(lines.contains(&format!("wrote {file}")), "{lines:?}");
    }
    let settings: Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).expect("read settings"),
    )
    .expect("parse settings");
    assert_eq!(
        settings["permissions"],
        json!({"allow": ["Bash(git status)"]})
    );
    assert_eq!(
        settings["hooks"]["SessionStart"]
            .as_array()
            .expect("SessionStart")
            .len(),
        2
    );
    assert!(skill.is_file(), "init did not render");
    assert!(
        lines.contains(&".claude/skills/project-knowledge/SKILL.md: written".to_string()),
        "{lines:?}"
    );
    for (file, content) in [
        (changed_skill, &b"the project's own skill"[..]),
        (owned_hook, &b"the project's own hook"[..]),
    ] {
        assert_file_holds(dir.path(), file, content);
        assert!(lines.contains(&format!("kept {file}")), "{lines:?}");
        assert!(!lines.contains(&format!("wrote {file}")), "{lines:?}");
        assert_eq!(
            marker(dir.path())["baselines"][file],
            baselines[file],
            "{file}"
        );
    }
}

/// `init` on an existing install keeps a kit-owned file the project changed,
/// with or without `--target claude`: the bytes stay, the line is
/// `kept <file>`, and the baseline stays at what it was, so `update` still
/// sees the change.
#[test]
fn init_keeps_a_kit_owned_file_the_project_changed_and_its_baseline() {
    for extra in [&[][..], &["--target", "claude"][..]] {
        let dir = scratch_git_repo();
        init_lines(dir.path(), &[]);
        let skill = ".claude/skills/orchestrating/SKILL.md";
        fs::write(dir.path().join(skill), b"the project's own skill").expect("write the skill");
        let baseline = marker(dir.path())["baselines"][skill].clone();
        assert!(baseline.is_string(), "init baselines {skill}");

        let lines = init_lines(dir.path(), extra);

        assert_file_holds(dir.path(), skill, b"the project's own skill");
        assert!(
            lines.contains(&format!("kept {skill}")),
            "init {extra:?}: {lines:?}"
        );
        assert!(
            !lines.contains(&format!("wrote {skill}")),
            "init {extra:?}: {lines:?}"
        );
        assert_eq!(
            marker(dir.path())["baselines"][skill],
            baseline,
            "init {extra:?}"
        );
    }
}

/// `init` never touches a path in `ownedPaths`. A file the project changed
/// stays as it is and is listed as `kept`; a file the project deleted stays
/// absent and is not listed. The paths cover each write of `init` outside the
/// pack copies: a kit-owned file, a seed-once file, the CI workflow, and the
/// hook entry of `.claude/settings.json`.
#[test]
fn init_never_touches_a_path_the_project_owns() {
    let dir = scratch_git_repo();
    let status = Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/example/example.git",
        ])
        .current_dir(dir.path())
        .status()
        .expect("add the remote");
    assert!(status.success(), "adding the remote failed");
    init_lines(dir.path(), &["--target", "claude"]);
    let changed_hook = ".githooks/commit-msg";
    let own_settings = ".claude/settings.json";
    let deleted = [
        ".claude/skills/migrating-knowledge/SKILL.md",
        "docs/README.md",
        ".github/workflows/knowledge.yml",
    ];
    fs::write(dir.path().join(changed_hook), b"the project's own hook").expect("write the hook");
    fs::write(dir.path().join(own_settings), "{\"permissions\": {}}\n").expect("write settings");
    for file in deleted {
        fs::remove_file(dir.path().join(file)).expect("remove an owned file");
    }
    let mut stamp = marker(dir.path());
    stamp["ownedPaths"] = json!([
        changed_hook,
        own_settings,
        deleted[0],
        deleted[1],
        deleted[2]
    ]);
    write_marker(dir.path(), &stamp);
    let stamped = marker(dir.path());
    let before = without_marker(snapshot(dir.path()));

    let lines = init_lines(dir.path(), &["--target", "claude"]);

    assert_same_tree(
        &before,
        &without_marker(snapshot(dir.path())),
        "init over owned paths",
    );
    let after = marker(dir.path());
    assert_eq!(after["ownedPaths"], stamped["ownedPaths"]);
    assert_eq!(after["baselines"], stamped["baselines"]);
    for file in [changed_hook, own_settings] {
        assert!(lines.contains(&format!("kept {file}")), "{lines:?}");
    }
    for file in deleted {
        assert!(
            !lines.iter().any(|line| line.contains(file)),
            "{file}: {lines:?}"
        );
    }
}

/// A second `init`, with or without `--target claude`, writes and lists no
/// file, and changes no byte of the tree (R2-2).
#[test]
fn a_second_init_prints_no_wrote_line_and_changes_no_file() {
    for extra in [&[][..], &["--target", "claude"][..]] {
        let dir = scratch_git_repo();
        init_lines(dir.path(), extra);
        let before = snapshot(dir.path());

        let lines = init_lines(dir.path(), extra);

        assert_same_tree(&before, &snapshot(dir.path()), &format!("init {extra:?}"));
        let wrote: Vec<&String> = lines
            .iter()
            .filter(|line| line.starts_with("wrote "))
            .collect();
        assert!(wrote.is_empty(), "init {extra:?}: {lines:?}");
    }
}

/// `init` then `update`: every file of the tree is byte-identical (HR-162),
/// and `update` prints no `wrote` line.
#[test]
fn init_then_update_leaves_every_file_byte_identical() {
    for extra in [&[][..], &["--target", "claude"][..]] {
        let dir = scratch_git_repo();
        init_lines(dir.path(), extra);
        let before = snapshot(dir.path());

        let output = houserules()
            .args(["update", "--dir"])
            .arg(dir.path())
            .output()
            .expect("run update");
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        assert_same_tree(
            &before,
            &snapshot(dir.path()),
            &format!("init {extra:?} then update"),
        );
        let lines = lines_of(output.stdout);
        assert!(
            !lines.iter().any(|line| line.starts_with("wrote ")),
            "{lines:?}"
        );
    }
}

/// `init` never overwrites a pack copy that exists. On an existing install, a
/// copy with one more line at its end stays as it is, its pin stays, and no
/// line names the copy: `init` does not look at the packs there (spec C18).
#[test]
fn init_leaves_a_pack_copy_of_an_existing_install_as_it_is() {
    let dir = scratch_git_repo();
    init_lines(dir.path(), &[]);
    let copy = dir.path().join("knowledge/packs/quality.json");
    let edited = [fs::read(&copy).expect("read the copy"), b"\n".to_vec()].concat();
    fs::write(&copy, &edited).expect("hand-edit the copy");
    let pins = marker(dir.path())["packs"].clone();

    let lines = init_lines(dir.path(), &[]);

    assert_eq!(fs::read(&copy).unwrap(), edited);
    assert_eq!(marker(dir.path())["packs"], pins);
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("knowledge/packs/quality.json")),
        "{lines:?}"
    );
}

/// On a new install, where no marker records a pack, a pack copy that exists
/// is kept: its bytes stay and `init` lists it as `kept`.
#[test]
fn init_keeps_a_pack_copy_that_exists_on_a_new_install() {
    let dir = scratch_git_repo();
    let copy = dir.path().join("knowledge/packs/quality.json");
    let edited = [
        fs::read(repo_root().join("template/packs/quality.json")).expect("read the template pack"),
        b"\n".to_vec(),
    ]
    .concat();
    fs::create_dir_all(copy.parent().expect("a parent")).expect("create knowledge/packs/");
    fs::write(&copy, &edited).expect("write the copy");

    let lines = init_lines(dir.path(), &[]);

    assert_eq!(fs::read(&copy).unwrap(), edited);
    assert!(
        lines.contains(&"kept knowledge/packs/quality.json".to_string()),
        "{lines:?}"
    );
}

/// An install whose `.houserules.json` has no `schema` key is a 1.x
/// install: `init` refuses it with one line and changes nothing.
#[test]
fn init_refuses_a_1_x_install_and_changes_nothing() {
    let dir = scratch_git_repo();
    let marker_path = dir.path().join(".houserules.json");
    fs::write(
        &marker_path,
        "{\n  \"idPrefix\": \"WI\",\n  \"version\": \"1.4.0\",\n  \"baselines\": {}\n}\n",
    )
    .expect("write a 1.x marker");
    let before = snapshot(dir.path());

    let output = init(dir.path(), &[]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        format!(
            "{}: this is an install of houserules 1.x; run houserules update to convert it\n",
            marker_path.display()
        )
    );
    assert_same_tree(&before, &snapshot(dir.path()), "the refusal");
}

/// A kit pack declares areas that the project's own `knowledge/areas.json`
/// may lack: `init` adds each one with the pack's globs and records it in
/// the `addedAreas` of the first pack that adds it (spec 6).
#[test]
fn init_adds_the_areas_a_pack_declares_and_records_them_in_the_pins() {
    let dir = scratch_git_repo();
    fs::create_dir_all(dir.path().join("knowledge")).expect("create knowledge/");
    fs::write(
        dir.path().join("knowledge/areas.json"),
        "{\n  \"global\": {\"paths\": []}\n}\n",
    )
    .expect("write areas.json");

    let lines = init_lines(dir.path(), &[]);

    let areas: Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("knowledge/areas.json")).expect("read areas.json"),
    )
    .expect("parse areas.json");
    assert_eq!(keys(&areas), ["global", "docs", "process", "tools"]);
    assert_eq!(areas["tools"], json!({"paths": ["tools/**"]}));
    let marker = marker(dir.path());
    let added = |pack: &str| marker["packs"][pack]["addedAreas"].clone();
    assert_eq!(added("knowledge-base"), json!(["docs"]));
    assert_eq!(added("process"), json!(["process"]));
    assert_eq!(added("quality"), json!(["tools"]));
    assert_eq!(added("security-hygiene"), json!([]));
    assert_eq!(added("writing-style"), json!([]));
    assert!(
        lines.contains(
            &"added area \"tools\" to knowledge/areas.json with the globs tools/**".to_string()
        ),
        "{lines:?}"
    );
}

/// `tree` without the marker, which a test that edits it in another layout
/// compares by key instead.
fn without_marker(mut tree: tree_snapshot::Tree) -> tree_snapshot::Tree {
    tree.remove(".houserules.json");
    tree
}

/// Runs `pack remove <name>` in the install at `dir` and requires exit 0.
fn remove_pack(dir: &Path, name: &str) {
    let output = houserules()
        .args(["pack", "remove", name, "--dir"])
        .arg(dir)
        .output()
        .expect("run pack remove");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Requires that `lines` show no pack installed and no area added: no
/// `wrote` line for a file below `knowledge/` and no `added area` line.
fn assert_installs_no_pack_and_adds_no_area(lines: &[String]) {
    let offenders: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("wrote knowledge/") || line.starts_with("added area "))
        .collect();
    assert!(offenders.is_empty(), "{offenders:?}");
}

/// A fresh install from which `pack remove quality` took the pack out: its
/// copy and pin are gone, and so is its area `tools`. When `owned` is not
/// empty, the marker lists those paths in `ownedPaths`.
fn install_without_the_quality_pack(owned: &[&str]) -> TempDir {
    let dir = scratch_git_repo();
    init_lines(dir.path(), &[]);
    remove_pack(dir.path(), "quality");
    if !owned.is_empty() {
        let mut stamp = marker(dir.path());
        stamp["ownedPaths"] = json!(owned);
        write_marker(dir.path(), &stamp);
    }
    dir
}

/// Writes `stamp` as the marker of the install at `dir`.
fn write_marker(dir: &Path, stamp: &Value) {
    fs::write(
        dir.join(".houserules.json"),
        format!("{}\n", serde_json::to_string_pretty(stamp).unwrap()),
    )
    .expect("write the marker");
}

/// A 2.0 marker without a `renderers` key reads as `["claude"]`, so `init`
/// leaves the key out, with or without `--target claude`, and keeps the
/// Claude files.
#[test]
fn init_leaves_an_absent_renderers_key_absent() {
    for extra in [&[][..], &["--target", "claude"][..]] {
        let dir = scratch_git_repo();
        init_lines(dir.path(), &["--target", "claude"]);
        let mut stamp = marker(dir.path());
        stamp
            .as_object_mut()
            .expect("an object")
            .remove("renderers");
        write_marker(dir.path(), &stamp);

        let lines = init_lines(dir.path(), extra);

        assert!(
            marker(dir.path()).get("renderers").is_none(),
            "init {extra:?}"
        );
        assert!(
            lines.contains(&"kept .claude/agents/implementer.md".to_string()),
            "init {extra:?}: {lines:?}"
        );
        assert!(!lines.iter().any(|line| line == ENABLE_LINE), "{lines:?}");
    }
}

/// A `packs` key of another shape is one named line and changes nothing.
#[test]
fn init_names_a_malformed_packs_key_and_changes_nothing() {
    let dir = scratch_git_repo();
    init_lines(dir.path(), &[]);
    let mut stamp = marker(dir.path());
    stamp["packs"] = json!([]);
    write_marker(dir.path(), &stamp);
    let before = snapshot(dir.path());

    let output = init(dir.path(), &[]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        format!(
            "{}: packs must be an object\n",
            dir.path().join(".houserules.json").display()
        )
    );
    assert_same_tree(&before, &snapshot(dir.path()), "the refusal");
}

/// A kit-owned file that cannot be read, for a reason other than absence, is
/// one named line that keeps the operating system's own text, and `init`
/// leaves the file as it is: a file that its owner can write and cannot read
/// must not read as a missing one, or `init` would write over it. A process
/// that reads any file (a root user) cannot set the case up, so the test
/// returns there. The expected path is built the way the binary builds it,
/// with the slash of the relative name kept in one join.
#[cfg(unix)]
#[test]
fn init_names_a_kit_owned_file_that_cannot_be_read_and_leaves_it_alone() {
    use std::os::unix::fs::PermissionsExt;

    let dir = scratch_git_repo();
    let hook = dir.path().join(".githooks/commit-msg");
    fs::create_dir_all(hook.parent().expect("a parent")).expect("create .githooks/");
    fs::write(&hook, b"the project's own hook").expect("write the hook");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o200)).expect("drop the read bit");
    let Err(os_error) = fs::read(&hook) else {
        return;
    };

    let output = init(dir.path(), &[]);

    fs::set_permissions(&hook, fs::Permissions::from_mode(0o644)).expect("restore the mode");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 stderr"),
        format!("{}: {os_error}\n", hook.display())
    );
    assert_file_holds(
        dir.path(),
        ".githooks/commit-msg",
        b"the project's own hook",
    );
}

/// A `defaultBranch` that is no string is one named line and changes nothing,
/// for `init` and for `update`: the key is one that `audit` reads, and the
/// marker has one reader.
#[test]
fn init_and_update_name_a_default_branch_that_is_no_string_and_change_nothing() {
    for command in ["init", "update"] {
        let dir = scratch_git_repo();
        init_lines(dir.path(), &[]);
        let mut stamp = marker(dir.path());
        stamp["defaultBranch"] = json!(5);
        write_marker(dir.path(), &stamp);
        let before = snapshot(dir.path());

        let output = houserules()
            .args([command, "--dir"])
            .arg(dir.path())
            .output()
            .expect("run the command");

        assert_eq!(output.status.code(), Some(2), "{command}");
        assert_eq!(output.stdout, b"", "{command}");
        assert_eq!(
            String::from_utf8(output.stderr).expect("utf8 stderr"),
            format!(
                "{}: defaultBranch must be a string\n",
                dir.path().join(".houserules.json").display()
            ),
            "{command}"
        );
        assert_same_tree(&before, &snapshot(dir.path()), command);
    }
}

/// A pin with no copy is left for `update`: `init` neither writes the copy
/// nor lists it, since the pin may name another version than the binary's.
#[test]
fn init_leaves_a_pinned_pack_whose_copy_is_missing_to_update() {
    let dir = scratch_git_repo();
    init_lines(dir.path(), &[]);
    let copy = dir.path().join("knowledge/packs/quality.json");
    fs::remove_file(&copy).expect("remove the copy");

    let lines = init_lines(dir.path(), &[]);

    assert!(!copy.exists());
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("knowledge/packs/quality.json")),
        "{lines:?}"
    );
}

/// `init` on an existing install puts back no pack that `pack remove` took
/// out, also when the project lists `knowledge/areas.json` and the pack copy
/// in `ownedPaths` (spec C18): the tree, the pins, and the owned paths stay
/// as they were, and no line reports a pack or an area.
#[test]
fn init_puts_back_no_pack_that_pack_remove_took_out_when_the_project_owns_its_files() {
    let owned = ["knowledge/areas.json", "knowledge/packs/quality.json"];
    let dir = install_without_the_quality_pack(&owned);
    let stamped = marker(dir.path());
    let before = without_marker(snapshot(dir.path()));

    let lines = init_lines(dir.path(), &[]);

    assert_same_tree(
        &before,
        &without_marker(snapshot(dir.path())),
        "init over owned pack files",
    );
    let after = marker(dir.path());
    assert_eq!(after["packs"], stamped["packs"]);
    assert_eq!(after["ownedPaths"], json!(owned));
    assert_installs_no_pack_and_adds_no_area(&lines);
}

/// The same without `ownedPaths` (spec C18, first sentence): `init` installs
/// no pack and adds no area, so the tree and the marker stay byte for byte.
#[test]
fn init_puts_back_no_pack_that_pack_remove_took_out() {
    let dir = install_without_the_quality_pack(&[]);
    let before = snapshot(dir.path());

    let lines = init_lines(dir.path(), &[]);

    assert_same_tree(&before, &snapshot(dir.path()), "init after pack remove");
    assert!(
        marker(dir.path())["packs"].get("quality").is_none(),
        "the pin of quality is back"
    );
    assert_installs_no_pack_and_adds_no_area(&lines);
}

/// The areas that the packs of `names` declare, as `init` records them in the
/// pins when it seeds an `areas.json`: the packs in name order, and each area
/// with the first pack that declares it.
fn areas_by_first_pack(names: &[&str]) -> Vec<(String, Vec<String>)> {
    let mut seen: Vec<String> = Vec::new();
    let mut by_pack = Vec::new();
    for name in names {
        let pack: Value = serde_json::from_str(
            &fs::read_to_string(
                repo_root()
                    .join("template/packs")
                    .join(format!("{name}.json")),
            )
            .expect("read the pack"),
        )
        .expect("parse the pack");
        let new: Vec<String> = keys(&pack["areas"])
            .into_iter()
            .filter(|area| !seen.contains(area))
            .collect();
        seen.extend(new.iter().cloned());
        by_pack.push((name.to_string(), new));
    }
    by_pack
}

/// An absent `knowledge/areas.json` on an existing install (spec C18): `init`
/// seeds it with only the areas that the installed packs declare, because the
/// render that ends `init` needs the file, and records each area in the pin
/// of the pack that declares it. It installs no pack. After `pack remove
/// quality`, the area `tools` of that pack is not in the new file.
#[test]
fn init_seeds_an_absent_areas_file_of_an_existing_install_with_the_areas_of_its_packs_only() {
    let dir = install_without_the_quality_pack(&[]);
    fs::remove_file(dir.path().join("knowledge/areas.json")).expect("remove areas.json");
    let before = snapshot(dir.path());

    let lines = init_lines(dir.path(), &[]);

    let installed: Vec<&str> = PACKS
        .into_iter()
        .filter(|pack| *pack != "quality")
        .collect();
    let expected = areas_by_first_pack(&installed);
    let areas: Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("knowledge/areas.json")).expect("read areas.json"),
    )
    .expect("parse areas.json");
    let mut seeded = keys(&areas);
    seeded.sort();
    let mut declared: Vec<String> = expected
        .iter()
        .flat_map(|(_, areas)| areas.iter().cloned())
        .collect();
    declared.sort();
    assert_eq!(seeded, declared);
    assert!(!seeded.contains(&"tools".to_string()), "{seeded:?}");
    let stamp = marker(dir.path());
    for (pack, added) in &expected {
        assert_eq!(
            stamp["packs"][pack]["addedAreas"],
            json!(added),
            "the record of {pack}"
        );
    }
    assert!(stamp["packs"].get("quality").is_none());
    assert!(
        lines.contains(&"wrote knowledge/areas.json".to_string()),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("wrote knowledge/packs/")),
        "{lines:?}"
    );
    let mut after = snapshot(dir.path());
    let mut untouched = before.clone();
    for changed in ["knowledge/areas.json", ".houserules.json"] {
        after.remove(changed);
        untouched.remove(changed);
    }
    assert_same_tree(&untouched, &after, "init seeds areas.json and nothing else");
}

/// When `ownedPaths` lists the absent `knowledge/areas.json`, `init` refuses
/// before it writes anything, with one line that names the file (spec C18).
#[test]
fn init_refuses_an_absent_owned_areas_file_before_any_write() {
    let dir = install_without_the_quality_pack(&["knowledge/areas.json"]);
    fs::remove_file(dir.path().join("knowledge/areas.json")).expect("remove areas.json");
    let before = snapshot(dir.path());

    let output = init(dir.path(), &["--target", "claude"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: this file is in ownedPaths and does not exist, so init cannot seed it; restore it, or remove it from ownedPaths\n",
            dir.path().join("knowledge").join("areas.json").display()
        )
    );
    assert_same_tree(&before, &snapshot(dir.path()), "the refusal");
}

/// `init --target claude` adds no hook entry to a `.claude/settings.json`
/// that `ownedPaths` lists, on an install that had no Claude renderer: the
/// file stays byte for byte, and the line says `kept`.
#[test]
fn init_target_claude_adds_no_hook_entry_to_an_owned_settings_file() {
    let dir = scratch_git_repo();
    init_lines(dir.path(), &[]);
    let own = "{\n  \"permissions\": {\"allow\": [\"Bash(ls)\"]}\n}\n";
    fs::create_dir_all(dir.path().join(".claude")).expect("create .claude/");
    fs::write(dir.path().join(".claude/settings.json"), own).expect("write settings");
    let mut stamp = marker(dir.path());
    stamp["ownedPaths"] = json!([".claude/settings.json"]);
    write_marker(dir.path(), &stamp);

    let lines = init_lines(dir.path(), &["--target", "claude"]);

    assert_file_holds(dir.path(), ".claude/settings.json", own.as_bytes());
    assert!(
        lines.contains(&"kept .claude/settings.json".to_string()),
        "{lines:?}"
    );
    assert_eq!(marker(dir.path())["renderers"], json!(["claude"]));
}
