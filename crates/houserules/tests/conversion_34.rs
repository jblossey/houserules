//! The conversion steps of batch 34 through `update` (spec 4.7), end to end:
//! the JSON tracker of a 1.x install, the adoption of the reference workflow
//! by provenance, the split of the session-start script, the delivery of the
//! two schemas, the known contents that make a kit-owned file the kit's
//! (C35), and the refresh of the Beads issue prefix (spec 3.3, C37, C38).
//! `tests/v1_4_0` seeds the 1.x installs with the binary of the `v1.4.0`
//! release. A batch-33 install is a fresh install of this binary with the
//! kit-owned files and baselines that batch 33 wrote laid over it, from the
//! contents at `fcc342d`, the batch-33 merge on main. The populations are
//! those of live cases 14, 15, and 16 (first half) of the spec.

mod locked_file;
mod tracker_support;
mod tree_snapshot;
mod v1_4_0;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Map, Value, json};

use tracker_support::{
    LINE_B, commit_all, edit_marker, git, install_with_backlog_dir, install_without_backlog,
    read_json, repo_root, run, run_ok, scratch_git_repo, sha256_hex, stderr, stdout, write_json,
};

/// The version the workflow pins: the version of the binary.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The batch-33 merge on main: the contents of a batch-33 install.
const BATCH_33: &str = "fcc342d";

/// The session-start script of the claude renderer.
const SESSION_SCRIPT: &str = "tools/claude-session-start.sh";

/// The session-start script of the reference workflow.
const REFERENCE_SCRIPT: &str = "tools/reference-session-start.sh";

/// The command of the startup hook entry that releases before this one seeded:
/// the entry that C40 removes where it equals the kit's.
const STARTUP_COMMAND: &str = "${CLAUDE_PROJECT_DIR}/tools/claude-session-start.sh start";

/// The command of the compaction hook entry, the one core hook entry.
const COMPACT_COMMAND: &str = "${CLAUDE_PROJECT_DIR}/tools/claude-session-start.sh compact";

/// The command of the hook entry of the reference workflow.
const REFERENCE_COMMAND: &str = "${CLAUDE_PROJECT_DIR}/tools/reference-session-start.sh";

/// The line that `update` prints when it removes the startup hook entry.
const STARTUP_REMOVED: &str = "edited .claude/settings.json: removed the seeded startup hook entry; the kit seeds the compaction entry alone";

/// The skill of the core that the reference workflow does not own.
const MIGRATING_SKILL: &str = ".claude/skills/migrating-knowledge/SKILL.md";

/// The deliverables schema, a D3 file of a 2.0 install.
const DELIVERABLES: &str = ".claude/schemas/deliverables.json";

/// The schema of the JSON tracker.
const BACKLOG_SCHEMA: &str = "backlog/schema.json";

/// The `backlogId` pattern of an install with no tracker (spec 3.9): any token.
const NO_TRACKER_PATTERN: &str = "^[A-Za-z0-9#._-]+$";

/// The hand step of step 2 for an install that has no tracker.
const NEEDS_TRACKER_STEP: &str = "hand step: the reference workflow and the process pack need a tracker; run houserules init --tracker json|github|beads";

/// The path `relative` below `dir`, built one component at a time.
fn path_of(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The bytes of `template/<path>` at the git reference `reference` of this
/// repository, or `None` when the path does not exist there.
fn kit_content_at(reference: &str, path: &str) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("show")
        .arg(format!("{reference}:template/{path}"))
        .current_dir(repo_root())
        .output()
        .expect("run git show");
    output.status.success().then_some(output.stdout)
}

/// The bytes of the file of the reference workflow at the install path `path`,
/// as the binary under test embeds it.
fn package_content(path: &str) -> Vec<u8> {
    fs::read(repo_root().join("template/workflows/reference").join(path))
        .unwrap_or_else(|error| panic!("read the package file {path}: {error}"))
}

/// The bytes of the core file `path` of `template/`.
fn core_content(path: &str) -> Vec<u8> {
    fs::read(repo_root().join("template").join(path))
        .unwrap_or_else(|error| panic!("read template/{path}: {error}"))
}

/// The install paths of the files that the manifest of the reference
/// workflow lists.
fn manifest_files() -> Vec<String> {
    let manifest = read_json_file(&repo_root().join("template/workflows/reference/workflow.json"));
    manifest["files"]
        .as_array()
        .expect("the manifest lists its files")
        .iter()
        .map(|file| file.as_str().expect("a path").to_string())
        .collect()
}

/// Reads and parses the JSON file at `path`.
fn read_json_file(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|error| panic!("{path:?}: {error}")))
        .expect("parse JSON")
}

/// Writes `content` to `relative` below `dir`, creating its directories.
fn write_file(dir: &Path, relative: &str, content: &[u8]) {
    let path = path_of(dir, relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("create directories");
    fs::write(path, content).expect("write a file");
}

/// Reads `relative` below `dir`.
fn read_file(dir: &Path, relative: &str) -> Vec<u8> {
    fs::read(path_of(dir, relative)).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// A scratch repository that a real 1.4.0 `init` seeded, with a GitHub
/// `origin` first so that `init` seeds the CI workflow.
fn install_1_4_0() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/project.git",
        ],
    );
    v1_4_0::seed(dir.path(), None);
    commit_all(dir.path(), "state");
    dir
}

/// Runs `houserules update --dir <dir>`.
fn update_output(dir: &Path) -> Output {
    run(dir, &["update"])
}

/// Runs `update`, requires exit 0, and returns the lines of stdout.
fn update(dir: &Path) -> Vec<String> {
    let output = update_output(dir);
    assert_eq!(
        output.status.code(),
        Some(0),
        "update failed:\n{}{}",
        stdout(&output),
        stderr(&output)
    );
    stdout(&output).lines().map(str::to_string).collect()
}

/// Requires exit 0 from `houserules <args> --dir <dir>`.
fn passes(dir: &Path, args: &[&str]) {
    let output = run(dir, args);
    assert_eq!(
        output.status.code(),
        Some(0),
        "houserules {args:?} failed:\n{}{}",
        stdout(&output),
        stderr(&output)
    );
}

/// `git status --porcelain` of `dir`.
fn porcelain(dir: &Path) -> String {
    git(dir, &["status", "--porcelain"])
}

/// No line of `lines` announces a change, but the lines of `allowed`, which
/// the project keeps seeing until it merges by hand.
fn assert_no_change_lines(lines: &[String], allowed: &[String]) {
    for line in lines {
        let change = line.starts_with("wrote ")
            || line.starts_with("removed ")
            || line.starts_with("updated ")
            || line.starts_with("moved ")
            || line.starts_with("kept ")
            || line.starts_with("merged ")
            || line.starts_with("edited ")
            || line.starts_with("hand step: ")
            || line.ends_with(": written");
        assert!(
            !change || allowed.contains(line),
            "update printed a change line: {line}\nall lines: {lines:?}"
        );
    }
}

/// The commands of the `SessionStart` hook entries of `.claude/settings.json`.
fn session_start_commands(dir: &Path) -> Vec<String> {
    let settings = read_json(dir, ".claude/settings.json");
    settings["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart entries")
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().expect("hooks").iter())
        .map(|hook| hook["command"].as_str().expect("a command").to_string())
        .collect()
}

/// The deliverables schema that `init --tracker <adapter>` writes for the
/// default id prefix on a scratch install, through the binary.
fn deliverables_schema_of_init(adapter: &str) -> Vec<u8> {
    let scratch = scratch_git_repo();
    run_ok(scratch.path(), &["init", "--tracker", adapter]);
    read_file(scratch.path(), DELIVERABLES)
}

/// The `pattern` of `$defs.backlogId` in the deliverables schema of `dir`.
fn backlog_id_pattern(dir: &Path) -> String {
    read_json(dir, DELIVERABLES)["$defs"]["backlogId"]["pattern"]
        .as_str()
        .expect("a pattern")
        .to_string()
}

/// The paths that `.houserules.json` of `dir` both baselines and pins in
/// `workflow.files`: none, because the pin is the one provenance record of a path
/// of the workflow.
fn paths_with_a_baseline_and_a_pin(dir: &Path) -> Vec<String> {
    let marker = read_json(dir, ".houserules.json");
    let pins = marker["workflow"]["files"]
        .as_object()
        .expect("the pins of the workflow");
    marker["baselines"]
        .as_object()
        .expect("baselines")
        .keys()
        .filter(|path| pins.contains_key(path.as_str()))
        .cloned()
        .collect()
}

/// The baseline that `.houserules.json` of `dir` records for `key`.
fn baseline(dir: &Path, key: &str) -> Value {
    read_json(dir, ".houserules.json")["baselines"][key].clone()
}

/// A scratch repository with the install of batch 33: the kit-owned files and
/// the baselines that batch 33 wrote, from the contents at `BATCH_33`. A plain
/// install holds the skills and the commit-msg hook and no agent template; with
/// `claude` it also holds the agent templates, the session-start script, and
/// its hook entries. Both hold the seeded backlog files, the evals, and no
/// `tracker` key, which batch 34 introduces.
fn batch_33_install(claude: bool) -> tempfile::TempDir {
    let dir = scratch_git_repo();
    let mut init = vec!["init", "--tracker", "json"];
    if claude {
        init.extend(["--target", "claude"]);
    }
    run_ok(dir.path(), &init);
    for pack in ["process", "quality", "security-hygiene", "writing-style"] {
        run_ok(dir.path(), &["pack", "add", pack]);
    }
    let mut baselines = Map::new();
    let mut kit_owned = vec![
        ".githooks/commit-msg",
        ".claude/skills/orchestrating/SKILL.md",
        ".claude/skills/finishing-a-feature/SKILL.md",
    ];
    if claude {
        kit_owned.extend([
            SESSION_SCRIPT,
            ".claude/agents/implementer.md",
            ".claude/agents/task-reviewer.md",
            ".claude/agents/branch-reviewer.md",
        ]);
    }
    for path in kit_owned {
        let content =
            kit_content_at(BATCH_33, path).unwrap_or_else(|| panic!("{path} exists at {BATCH_33}"));
        write_file(dir.path(), path, &content);
        baselines.insert(path.to_string(), json!(sha256_hex(&content)));
    }
    for path in [
        ".claude/evals/dependency-add.json",
        ".claude/evals/docs-edit.json",
        ".claude/evals/seeded-violations.json",
    ] {
        let content =
            kit_content_at(BATCH_33, path).unwrap_or_else(|| panic!("{path} exists at {BATCH_33}"));
        write_file(dir.path(), path, &content);
    }
    write_file(dir.path(), ".claude/evals/record.json", b"{}\n");
    if claude {
        add_the_startup_entry(dir.path());
    }
    edit_marker(dir.path(), |marker| {
        marker.remove("tracker");
        marker.insert("schema".to_string(), json!("houserules/install@2.0.0"));
        let recorded = marker
            .entry("baselines")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("baselines");
        recorded.extend(baselines);
    });
    commit_all(dir.path(), "batch 33");
    dir
}

/// The startup entry that every release before this one seeded into
/// `.claude/settings.json`, after the compaction entry.
fn startup_entry() -> Value {
    json!({
        "matcher": "startup|resume|clear|fork",
        "hooks": [{"type": "command", "command": STARTUP_COMMAND}]
    })
}

/// Adds the startup entry to `.claude/settings.json` of `dir`: the file of a
/// batch-33 install holds it, and a fresh `init` of this binary does not seed it.
fn add_the_startup_entry(dir: &Path) {
    let mut settings = read_json(dir, ".claude/settings.json");
    settings["hooks"]["SessionStart"]
        .as_array_mut()
        .expect("SessionStart entries")
        .push(startup_entry());
    write_json(dir, ".claude/settings.json", &settings);
}

// --- Step 1: the JSON tracker, inside the 1.x conversion only --------------

/// Spec 4.7 step 1: a 1.x install with a `backlog/` directory gets
/// `{"adapter": "json"}` in the conversion, so `check-backlog` passes at once
/// and `init --tracker json` has nothing left to do.
#[test]
fn a_1_x_install_with_a_backlog_directory_converts_to_the_json_tracker() {
    let dir = install_1_4_0();

    update(dir.path());

    assert_eq!(
        read_json(dir.path(), ".houserules.json")["tracker"],
        json!({"adapter": "json"})
    );
    passes(dir.path(), &["check-backlog"]);
    let again = run_ok(dir.path(), &["init", "--tracker", "json"]);
    assert!(
        !stdout(&again).contains("wrote backlog/"),
        "{}",
        stdout(&again)
    );
}

/// Spec 3.3, 3.9, 4.7 steps 1, 2 and 4: a 1.x install with no `backlog/`
/// directory gets no tracker. The adoption of the workflow prints the tracker
/// hand step, and `check-knowledge` names the missing tracker of the process
/// pack and of the workflow (spec 3.10) until the project runs the command that
/// the hand step names. The deliverables schema that `update` delivers holds
/// the `backlogId` pattern of an install with no tracker, which accepts any
/// token; `init --tracker json` then writes the pattern of the JSON tracker.
#[test]
fn a_1_x_install_without_a_backlog_directory_gets_no_tracker_and_the_tracker_hand_step() {
    let dir = install_1_4_0();
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    commit_all(dir.path(), "no backlog");

    let lines = update(dir.path());

    assert!(read_json(dir.path(), ".houserules.json")["tracker"].is_null());
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.as_str() == NEEDS_TRACKER_STEP)
            .count(),
        1,
        "{lines:?}"
    );
    assert_eq!(backlog_id_pattern(dir.path()), NO_TRACKER_PATTERN);
    assert_eq!(
        read_file(dir.path(), DELIVERABLES),
        core_content(DELIVERABLES),
        "the schema of an install with no tracker is the kit's, byte for byte"
    );
    let check = run(dir.path(), &["check-knowledge"]);
    assert_eq!(check.status.code(), Some(1), "{}", stderr(&check));
    let report = format!("{}{}", stdout(&check), stderr(&check));
    for line in [
        "pack process needs a tracker; run houserules init --tracker json|github|beads",
        "the reference workflow needs a tracker; run houserules init --tracker json|github|beads",
    ] {
        assert!(report.lines().any(|found| found == line), "{report}");
    }
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    assert_eq!(
        read_file(dir.path(), DELIVERABLES),
        deliverables_schema_of_init("json"),
        "the command of the hand step writes the pattern of the JSON tracker"
    );
    assert_ne!(backlog_id_pattern(dir.path()), NO_TRACKER_PATTERN);
    commit_all(dir.path(), "tracker");
    passes(dir.path(), &["check-knowledge"]);
    let second = update(dir.path());
    assert!(!second.iter().any(|line| line == NEEDS_TRACKER_STEP));
}

/// Spec 3.9, population of case 15: a batch-33 install with no tracker holds the
/// deliverables schema of batch 33 at its baseline, with the JSON-shaped
/// `backlogId` pattern. `update` brings the schema to the content of the kit,
/// whose pattern accepts any token, because the install has no tracker to name
/// one. A second `update` changes nothing.
#[test]
fn a_batch_33_install_with_no_tracker_gets_the_no_tracker_pattern() {
    let dir = batch_33_install(false);
    let root = dir.path();
    let batch_33 = kit_content_at(BATCH_33, DELIVERABLES).expect("the batch-33 schema");
    write_file(root, DELIVERABLES, &batch_33);
    edit_marker(root, |marker| {
        marker["baselines"]
            .as_object_mut()
            .expect("baselines")
            .insert(DELIVERABLES.to_string(), json!(sha256_hex(&batch_33)));
    });
    commit_all(root, "a batch-33 schema");
    assert!(read_json(root, ".houserules.json")["tracker"].is_null());
    assert_ne!(backlog_id_pattern(root), NO_TRACKER_PATTERN);

    let lines = update(root);

    assert_eq!(backlog_id_pattern(root), NO_TRACKER_PATTERN);
    assert_eq!(read_file(root, DELIVERABLES), core_content(DELIVERABLES));
    assert_eq!(
        baseline(root, DELIVERABLES),
        json!(sha256_hex(&core_content(DELIVERABLES)))
    );
    assert!(
        lines.contains(&format!("wrote {DELIVERABLES}")),
        "{lines:?}"
    );
    commit_all(root, "update");
    assert_no_change_lines(&update(root), &[]);
    assert_eq!(porcelain(root), "", "a second update changes no file");
}

/// Spec 3.3, population of case 15: a 2.0 install with a `backlog/` directory
/// and no `tracker` key keeps line B. Step 1 runs inside the 1.x conversion
/// only, so `update` writes no tracker key, however often it runs. The
/// behavior is the one of batch 33: this test pins it.
#[test]
fn a_2_0_install_with_a_backlog_directory_and_no_tracker_key_keeps_line_b() {
    let dir = install_with_backlog_dir();
    commit_all(dir.path(), "state");

    for _ in 0..2 {
        update(dir.path());
        assert!(read_json(dir.path(), ".houserules.json")["tracker"].is_null());
    }

    let check = run(dir.path(), &["check-backlog"]);
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(stderr(&check), format!("{LINE_B}\n"));
}

// --- Step 2: the adoption of the reference workflow ------------------------

/// Spec 16, case 14: a 1.4.0 install through `update` gets the JSON tracker,
/// the reference workflow with each file pinned at the content it was written
/// with, the split session-start script, and both schemas delivered. Every
/// gate passes, and a second `update` changes nothing.
#[test]
fn a_1_4_0_install_converts_with_the_tracker_the_workflow_the_split_script_and_both_schemas() {
    let dir = install_1_4_0();
    let root = dir.path();

    let lines = update(root);

    let marker = read_json(root, ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "json"}));
    let workflow = &marker["workflow"];
    assert_eq!(workflow["name"], json!("reference"));
    assert_eq!(workflow["version"], json!(VERSION));
    let manifest = manifest_files();
    let mut pinned: Vec<String> = workflow["files"]
        .as_object()
        .expect("the pins")
        .keys()
        .cloned()
        .collect();
    pinned.sort();
    let mut listed = manifest.clone();
    listed.sort();
    assert_eq!(pinned, listed, "every file of the manifest is pinned");
    let wrote: BTreeSet<&str> = lines
        .iter()
        .filter_map(|line| line.strip_prefix("wrote "))
        .collect();
    for path in &manifest {
        let source = package_content(path);
        assert_eq!(read_file(root, path), source, "{path}");
        assert_eq!(
            workflow["files"][path],
            json!(sha256_hex(&source)),
            "the pin of {path}"
        );
        let moved = kit_content_at("v1.4.0", path).is_none_or(|old| old != source);
        assert_eq!(wrote.contains(path.as_str()), moved, "the line of {path}");
    }
    assert!(
        !lines.iter().any(|line| line.starts_with("hand step: ")),
        "{lines:?}"
    );

    assert_eq!(
        read_file(root, SESSION_SCRIPT),
        core_content(SESSION_SCRIPT)
    );
    assert!(
        !String::from_utf8_lossy(&read_file(root, SESSION_SCRIPT)).contains("Session ritual"),
        "the core script holds no ritual line"
    );
    assert_eq!(
        session_start_commands(root),
        [COMPACT_COMMAND, REFERENCE_COMMAND],
        "the compaction entry and the entry of the workflow"
    );
    assert!(lines.contains(
        &"merged .claude/settings.json (reference workflow SessionStart hook added)".to_string()
    ));

    let deliverables = read_file(root, DELIVERABLES);
    assert_eq!(deliverables, deliverables_schema_of_init("json"));
    assert_eq!(
        baseline(root, DELIVERABLES),
        json!(sha256_hex(&deliverables))
    );
    assert!(
        lines.contains(&format!("wrote {DELIVERABLES}")),
        "{lines:?}"
    );
    let backlog = read_file(root, BACKLOG_SCHEMA);
    assert_eq!(backlog, core_content(BACKLOG_SCHEMA));
    assert_eq!(baseline(root, BACKLOG_SCHEMA), json!(sha256_hex(&backlog)));
    assert!(
        lines.contains(&format!("wrote {BACKLOG_SCHEMA}")),
        "{lines:?}"
    );

    assert_eq!(
        paths_with_a_baseline_and_a_pin(root),
        Vec::<String>::new(),
        "the pins replace the baselines of the workflow paths"
    );

    passes(root, &["check-knowledge"]);
    passes(root, &["check-backlog"]);
    passes(root, &["render", "--check"]);

    commit_all(root, "update");
    let second = update(root);
    assert_no_change_lines(&second, &[]);
    assert_eq!(porcelain(root), "", "a second update changes no file");
}

/// Spec 4.7 step 2: the adoption pins each file of the workflow, so the baseline
/// of a pinned path goes, as the 1.x conversion removes the baselines that the
/// pack pins replace. An install that an earlier build adopted, and that still
/// holds such a baseline, loses it at the next `update`.
#[test]
fn update_drops_the_baseline_of_a_path_that_the_workflow_pins() {
    let dir = install_1_4_0();
    let root = dir.path();
    update(root);
    let reviewer = ".claude/agents/task-reviewer.md";
    edit_marker(root, |marker| {
        marker["baselines"]
            .as_object_mut()
            .expect("baselines")
            .insert(
                reviewer.to_string(),
                json!(sha256_hex(&package_content(reviewer))),
            );
    });
    commit_all(root, "a stale baseline");
    assert_eq!(paths_with_a_baseline_and_a_pin(root), [reviewer]);

    update(root);

    assert_eq!(paths_with_a_baseline_and_a_pin(root), Vec::<String>::new());
    commit_all(root, "update");
    assert_no_change_lines(&update(root), &[]);
    assert_eq!(porcelain(root), "", "a second update changes no file");
}

/// The command that installs the workflow into a batch-33 install holds
/// baselines for the three paths that batch 33 shipped: `init --workflow` pins
/// them and drops the baselines.
#[test]
fn init_workflow_drops_the_baselines_of_the_paths_it_pins() {
    let dir = batch_33_install(false);
    let root = dir.path();
    assert!(!baseline(root, ".githooks/commit-msg").is_null());

    run_ok(
        root,
        &["init", "--workflow", "reference", "--tracker", "json"],
    );

    assert!(read_json(root, ".houserules.json")["workflow"].is_object());
    assert_eq!(paths_with_a_baseline_and_a_pin(root), Vec::<String>::new());
}

/// Spec 4.7 step 2, case 14: the modified file of a 1.4.0 install stays
/// pinned at its recorded baseline, which is the 1.4.0 content. `update`
/// keeps it and prints the kept line on every run, until the project merges.
#[test]
fn a_modified_workflow_file_is_pinned_at_its_baseline_and_kept_on_every_update() {
    let dir = install_1_4_0();
    let root = dir.path();
    let reviewer = ".claude/agents/task-reviewer.md";
    let mut changed = read_file(root, reviewer);
    changed.extend_from_slice(b"\nOur own rule.\n");
    write_file(root, reviewer, &changed);
    commit_all(root, "our rule");
    let kept = format!("kept {reviewer} (locally modified; reference workflow {VERSION})");

    let first = update(root);
    commit_all(root, "update");
    let second = update(root);

    assert!(first.contains(&kept), "{first:?}");
    assert!(second.contains(&kept), "{second:?}");
    assert_eq!(read_file(root, reviewer), changed);
    let released = kit_content_at("v1.4.0", reviewer).expect("the 1.4.0 template");
    assert_eq!(
        read_json(root, ".houserules.json")["workflow"]["files"][reviewer],
        json!(sha256_hex(&released))
    );
    assert_no_change_lines(&second, std::slice::from_ref(&kept));
    assert_eq!(porcelain(root), "", "a second update changes no file");
}

/// Spec 4.7 step 2: an install without a baseline for a workflow file (a
/// release before 1.4.0 recorded none) pins the file at the known content that
/// it equals, so `update` writes the new content. A file that equals no known
/// content is the project's own: it stays, gets the `null` pin, and its hand
/// step is printed once.
#[test]
fn a_workflow_file_without_a_baseline_is_pinned_at_the_release_content_it_equals() {
    let dir = install_1_4_0();
    let root = dir.path();
    let from_1_2_0 = ".claude/skills/orchestrating/SKILL.md";
    let from_1_0_0 = ".githooks/commit-msg";
    let own = ".claude/agents/task-reviewer.md";
    write_file(
        root,
        from_1_2_0,
        &kit_content_at("v1.2.0", from_1_2_0).expect("the 1.2.0 template"),
    );
    write_file(
        root,
        from_1_0_0,
        &kit_content_at("v1.0.0", from_1_0_0).expect("the 1.0.0 template"),
    );
    write_file(root, own, b"Our own reviewer.\n");
    edit_marker(root, |marker| {
        let baselines = marker["baselines"].as_object_mut().expect("baselines");
        for path in [from_1_2_0, from_1_0_0, own] {
            baselines.remove(path);
        }
    });
    commit_all(root, "an older install");

    let first = update(root);
    commit_all(root, "update");
    let second = update(root);

    for path in [from_1_2_0, from_1_0_0] {
        assert_eq!(read_file(root, path), package_content(path), "{path}");
        assert!(first.contains(&format!("wrote {path}")), "{first:?}");
    }
    assert_eq!(read_file(root, own), b"Our own reviewer.\n");
    let line = format!(
        "hand step: {own} is not the kit's; the reference workflow leaves it to the project"
    );
    assert_eq!(first.iter().filter(|found| **found == line).count(), 1);
    assert!(!second.contains(&line), "{second:?}");
    assert_eq!(
        read_json(root, ".houserules.json")["workflow"]["files"][own],
        Value::Null
    );
    assert_no_change_lines(&second, &[]);
}

/// Spec 4.7 step 2: a workflow path in `ownedPaths` (the `overrides` key of a
/// 1.x marker) is left as found, gets no pin, and gets no line.
#[test]
fn an_owned_workflow_path_is_left_as_found_with_no_pin_and_no_line() {
    let dir = install_1_4_0();
    let root = dir.path();
    let owned = ".claude/agents/implementer.md";
    let before = read_file(root, owned);
    edit_marker(root, |marker| {
        marker.insert("overrides".to_string(), json!([owned]));
    });
    commit_all(root, "owned");

    let lines = update(root);

    assert_eq!(read_file(root, owned), before);
    let pins = read_json(root, ".houserules.json")["workflow"]["files"].clone();
    assert!(pins.get(owned).is_none(), "{pins}");
    assert!(pins.as_object().is_some_and(|pins| !pins.is_empty()));
    for line in &lines {
        assert!(!line.contains(owned), "{line}");
    }
}

/// Spec 4.7 step 2: a required id that `knowledge/overrides.json` excludes
/// gets the hand step, and `check-knowledge` fails with the line of spec 4.3
/// until the project removes the exclusion.
#[test]
fn an_excluded_required_id_gets_the_hand_step_and_fails_check_knowledge() {
    let dir = install_1_4_0();
    let root = dir.path();
    fs::remove_file(path_of(root, "knowledge/process.json")).expect("delete the topic file");
    commit_all(root, "a deleted topic");
    let line = |prefix: &str| {
        format!(
            "{prefix}workflow reference requires process.tdd, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it"
        )
    };

    let lines = update(root);

    assert!(lines.contains(&line("hand step: ")), "{lines:?}");
    let check = run(root, &["check-knowledge"]);
    assert_eq!(check.status.code(), Some(1));
    let report = format!("{}{}", stdout(&check), stderr(&check));
    assert!(report.lines().any(|found| found == line("")), "{report}");
}

/// Spec 16, case 14, the red that an upgrade causes by design: the update
/// commit changes agent templates and eval scenarios and not the scenario
/// record, so the audit of that commit fails the row of `process.evals-rerun`.
/// The remedy is the sanction, which keeps the true result of the row and names
/// its reference.
#[test]
fn the_audit_of_the_update_commit_fails_the_evals_rerun_row_until_it_is_sanctioned() {
    let dir = install_1_4_0();
    let root = dir.path();
    let base = git(root, &["rev-parse", "HEAD"]).trim().to_string();
    update(root);
    commit_all(root, "update");
    let evals_row = |output: &Output| {
        let audit: Value = serde_json::from_str(&stdout(output)).expect("the audit prints JSON");
        let row = audit["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .find(|row| row["id"] == json!("process.evals-rerun"))
            .expect("the row of process.evals-rerun")
            .clone();
        (row, audit["summary"].clone())
    };

    let (row, _) = evals_row(&run(root, &["audit", "--base", &base]));
    assert_eq!(row["result"], json!("fail"));
    assert!(
        row["evidence"]
            .as_str()
            .expect("evidence")
            .ends_with("without .claude/evals/record.json"),
        "{row}"
    );

    let (row, summary) = evals_row(&run(
        root,
        &[
            "audit",
            "--base",
            &base,
            "--sanctioned",
            "process.evals-rerun=the-eval-run",
        ],
    ));
    assert_eq!(row["result"], json!("fail"));
    assert!(
        row["evidence"]
            .as_str()
            .expect("evidence")
            .ends_with("(sanctioned: the-eval-run)"),
        "{row}"
    );
    assert_eq!(summary["sanctioned_fail"], json!(1));
}

/// Spec 16, case 16, first half: on a minimal 2.0 install, a project's own
/// commit-msg hook and implementer template are not adopted. `update` twice
/// writes no `workflow` key, changes neither file, and prints no line that
/// names them. Batch 33 behaves so: this test pins it.
#[test]
fn a_projects_own_workflow_path_files_on_a_minimal_install_are_never_adopted() {
    let dir = install_without_backlog();
    let root = dir.path();
    write_file(root, ".githooks/commit-msg", b"#!/bin/sh\nexit 0\n");
    write_file(root, ".claude/agents/implementer.md", b"Our implementer.\n");
    commit_all(root, "our files");

    for _ in 0..2 {
        let output = update_output(root);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let report = format!("{}{}", stdout(&output), stderr(&output));
        for path in [".githooks/commit-msg", ".claude/agents/implementer.md"] {
            assert!(!report.contains(path), "{path} in {report}");
        }
        assert!(!report.contains("hand step: "), "{report}");
    }

    assert!(read_json(root, ".houserules.json")["workflow"].is_null());
    assert_eq!(
        read_file(root, ".githooks/commit-msg"),
        b"#!/bin/sh\nexit 0\n"
    );
    assert_eq!(
        read_file(root, ".claude/agents/implementer.md"),
        b"Our implementer.\n"
    );
}

/// Spec 4.7 step 2, case 15: a plain batch-33 install holds the skills and the
/// hook and no agent template. `update` adopts the workflow, writes the
/// absent files, pins each, and prints the tracker hand step, because the
/// install has a `backlog/` directory and no tracker key.
#[test]
fn a_plain_batch_33_install_is_adopted_and_gets_the_tracker_hand_step() {
    let dir = batch_33_install(false);
    let root = dir.path();
    assert!(!path_of(root, ".claude/agents/implementer.md").exists());

    let lines = update(root);

    let pins = read_json(root, ".houserules.json")["workflow"]["files"].clone();
    for path in manifest_files() {
        assert_eq!(read_file(root, &path), package_content(&path), "{path}");
        assert_eq!(pins[&path], json!(sha256_hex(&package_content(&path))));
    }
    assert!(
        lines.contains(&"wrote .claude/agents/implementer.md".to_string()),
        "{lines:?}"
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.as_str() == NEEDS_TRACKER_STEP)
            .count(),
        1,
        "{lines:?}"
    );
    assert!(read_json(root, ".houserules.json")["tracker"].is_null());
    assert!(
        session_start_commands_exist(root).is_none(),
        "a plain install has no settings hook entry"
    );
    let check = run(root, &["check-backlog"]);
    assert_eq!(stderr(&check), format!("{LINE_B}\n"));
}

/// The hook commands of `.claude/settings.json`, or `None` when the install
/// has no such file.
fn session_start_commands_exist(dir: &Path) -> Option<Vec<String>> {
    path_of(dir, ".claude/settings.json")
        .is_file()
        .then(|| session_start_commands(dir))
}

// --- Step 3: the split of the session-start script ------------------------

/// Spec 4.7 step 3, case 15: a batch-33 install with `--target claude` holds
/// the script with the ritual line at its baseline. `update` replaces it with
/// the core script, writes the reference script, and adds its hook entry
/// without a hand step.
#[test]
fn a_batch_33_claude_install_gets_the_core_script_the_reference_script_and_its_hook_entry() {
    let dir = batch_33_install(true);
    let root = dir.path();
    assert!(
        String::from_utf8_lossy(&read_file(root, SESSION_SCRIPT)).contains("Session ritual"),
        "the batch-33 script holds the ritual line"
    );

    let lines = update(root);

    assert_eq!(
        read_file(root, SESSION_SCRIPT),
        core_content(SESSION_SCRIPT)
    );
    assert_eq!(
        read_file(root, REFERENCE_SCRIPT),
        package_content(REFERENCE_SCRIPT)
    );
    assert_eq!(
        session_start_commands(root)
            .iter()
            .filter(|command| command.contains("reference-session-start.sh"))
            .count(),
        1
    );
    assert!(
        lines.contains(&format!("wrote {SESSION_SCRIPT}")),
        "{lines:?}"
    );
    assert!(
        lines.contains(&format!("wrote {REFERENCE_SCRIPT}")),
        "{lines:?}"
    );
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("holds the reference workflow's ritual line")),
        "{lines:?}"
    );
}

/// The hand step of spec 4.7 step 3.
const RITUAL_STEP: &str = "hand step: tools/claude-session-start.sh holds the reference workflow's ritual line; move it to tools/reference-session-start.sh";

/// Spec 4.7 step 3: a script that the project changed and that still holds the
/// ritual line stays as it is. `update` prints the kept line and the hand step
/// on every run, adds the hook entry of the reference script, and writes that
/// script.
#[test]
fn a_session_start_script_the_project_changed_is_kept_with_the_hand_step() {
    let dir = install_1_4_0();
    let root = dir.path();
    let mut changed = read_file(root, SESSION_SCRIPT);
    changed.extend_from_slice(b"# our own line\n");
    write_file(root, SESSION_SCRIPT, &changed);
    commit_all(root, "our script");
    let kept = format!("kept {SESSION_SCRIPT} (locally modified)");

    let first = update(root);
    commit_all(root, "update");
    let second = update(root);

    for lines in [&first, &second] {
        assert!(lines.contains(&kept), "{lines:?}");
        assert!(lines.contains(&RITUAL_STEP.to_string()), "{lines:?}");
    }
    assert_eq!(read_file(root, SESSION_SCRIPT), changed);
    assert_eq!(
        read_file(root, REFERENCE_SCRIPT),
        package_content(REFERENCE_SCRIPT)
    );
    assert!(
        session_start_commands(root)
            .iter()
            .any(|command| command.contains("reference-session-start.sh"))
    );
    assert_eq!(porcelain(root), "", "a second update changes no file");
}

/// Spec 4.7 step 3: with the script and `.claude/settings.json` in
/// `ownedPaths` (the `overrides` key of a 1.x marker), `update` leaves both as
/// found and prints no line for either.
#[test]
fn an_owned_session_start_script_and_settings_file_stay_as_found_without_a_line() {
    let dir = install_1_4_0();
    let root = dir.path();
    let script = read_file(root, SESSION_SCRIPT);
    let settings = read_file(root, ".claude/settings.json");
    edit_marker(root, |marker| {
        marker.insert(
            "overrides".to_string(),
            json!([SESSION_SCRIPT, ".claude/settings.json"]),
        );
    });
    commit_all(root, "owned");

    let lines = update(root);

    assert_eq!(read_file(root, SESSION_SCRIPT), script);
    assert_eq!(read_file(root, ".claude/settings.json"), settings);
    for line in &lines {
        assert!(
            !line.contains(SESSION_SCRIPT) && !line.contains(".claude/settings.json"),
            "{line}"
        );
    }
    assert!(!lines.contains(&RITUAL_STEP.to_string()));
}

// --- Step 4: the delivery of the two schemas -------------------------------

/// Spec 4.7 step 4: a deliverables schema of the batch-33 main content (a known
/// content) on an install with the JSON tracker and no baseline is delivered:
/// `update` writes the schema with the pattern of the adapter and baselines it.
#[test]
fn a_batch_33_deliverables_schema_is_delivered_with_the_pattern_of_the_adapter() {
    let dir = batch_33_install(false);
    let root = dir.path();
    edit_marker(root, |marker| {
        marker.insert("tracker".to_string(), json!({"adapter": "json"}));
    });
    write_file(
        root,
        DELIVERABLES,
        &kit_content_at(BATCH_33, DELIVERABLES).expect("the batch-33 schema"),
    );
    edit_marker(root, |marker| {
        marker["baselines"]
            .as_object_mut()
            .expect("baselines")
            .remove(DELIVERABLES);
    });
    commit_all(root, "a batch-33 schema");

    let lines = update(root);

    let delivered = read_file(root, DELIVERABLES);
    assert_eq!(delivered, deliverables_schema_of_init("json"));
    assert_eq!(baseline(root, DELIVERABLES), json!(sha256_hex(&delivered)));
    assert!(
        lines.contains(&format!("wrote {DELIVERABLES}")),
        "{lines:?}"
    );
    commit_all(root, "update");
    assert_no_change_lines(&update(root), &[]);
}

/// Spec 4.7 step 4: a schema that equals no known content is the project's:
/// `update` keeps it, gets no baseline for it, and lists a hand step on every
/// run. The backlog schema is a file of the JSON tracker only.
#[test]
fn a_modified_schema_is_kept_and_listed_as_a_hand_step() {
    let dir = install_1_4_0();
    let root = dir.path();
    for path in [DELIVERABLES, BACKLOG_SCHEMA] {
        let mut schema = read_json(root, path);
        schema["title"] = json!("Our own schema");
        let mut text = serde_json::to_string_pretty(&schema).expect("serialize");
        text.push('\n');
        write_file(root, path, text.as_bytes());
    }
    let before = (
        read_file(root, DELIVERABLES),
        read_file(root, BACKLOG_SCHEMA),
    );
    commit_all(root, "our schemas");

    let first = update(root);
    commit_all(root, "update");
    let second = update(root);

    for path in [DELIVERABLES, BACKLOG_SCHEMA] {
        let step = format!(
            "hand step: {path} differs from every content of it that houserules shipped, so houserules update kept it; compare it with the file that houserules init writes, and merge the rest by hand"
        );
        assert!(first.contains(&step), "{first:?}");
        assert!(second.contains(&step), "{second:?}");
        assert!(baseline(root, path).is_null(), "{path} has a baseline");
        assert!(!first.contains(&format!("wrote {path}")), "{first:?}");
    }
    assert_eq!(
        (
            read_file(root, DELIVERABLES),
            read_file(root, BACKLOG_SCHEMA)
        ),
        before
    );
}

/// Spec 4.7 step 4: an owned schema stays as found, with no line.
#[test]
fn an_owned_schema_is_left_alone_without_a_line() {
    let dir = install_1_4_0();
    let root = dir.path();
    edit_marker(root, |marker| {
        marker.insert(
            "overrides".to_string(),
            json!([DELIVERABLES, BACKLOG_SCHEMA]),
        );
    });
    let before = (
        read_file(root, DELIVERABLES),
        read_file(root, BACKLOG_SCHEMA),
    );
    commit_all(root, "owned");

    let lines = update(root);

    assert_eq!(
        (
            read_file(root, DELIVERABLES),
            read_file(root, BACKLOG_SCHEMA)
        ),
        before
    );
    for line in &lines {
        assert!(
            !line.contains(DELIVERABLES) && !line.contains(BACKLOG_SCHEMA),
            "{line}"
        );
    }
}

// --- C40: the compaction entry is the only core session-start hook ---------

/// C40 (spec 4.4): the startup entry of a 1.4.0 install equals the kit's, so
/// `update` removes it once and keeps the compaction entry and the entry of the
/// workflow. A second `update` changes nothing.
#[test]
fn update_removes_the_startup_hook_entry_that_equals_the_kits() {
    let dir = install_1_4_0();
    let root = dir.path();
    assert!(session_start_commands(root).contains(&STARTUP_COMMAND.to_string()));

    let lines = update(root);

    assert_eq!(
        session_start_commands(root),
        [COMPACT_COMMAND, REFERENCE_COMMAND]
    );
    assert_eq!(
        lines
            .iter()
            .filter(|line| **line == STARTUP_REMOVED)
            .count(),
        1,
        "{lines:?}"
    );
    commit_all(root, "update");
    assert_no_change_lines(&update(root), &[]);
    assert_eq!(porcelain(root), "", "a second update changes no file");
}

/// C40: a batch-33 install with `--target claude` holds the startup entry and no
/// reference entry. `update` removes the startup entry and adds the entry of the
/// workflow.
#[test]
fn a_batch_33_claude_install_loses_the_startup_hook_entry() {
    let dir = batch_33_install(true);
    let root = dir.path();
    assert!(session_start_commands(root).contains(&STARTUP_COMMAND.to_string()));

    let lines = update(root);

    assert_eq!(
        session_start_commands(root),
        [COMPACT_COMMAND, REFERENCE_COMMAND]
    );
    assert!(lines.contains(&STARTUP_REMOVED.to_string()), "{lines:?}");
}

/// C40: a startup entry that the project changed stays, with no line. The
/// entries that differ from the kit's by one more hook, and by another command,
/// are both the project's.
#[test]
fn a_startup_hook_entry_that_the_project_changed_stays_without_a_line() {
    for change in [
        json!([
            {"type": "command", "command": STARTUP_COMMAND},
            {"type": "command", "command": "${CLAUDE_PROJECT_DIR}/tools/our-hook.sh"}
        ]),
        json!([{"type": "command", "command": "${CLAUDE_PROJECT_DIR}/tools/our-hook.sh"}]),
    ] {
        let dir = install_1_4_0();
        let root = dir.path();
        let mut settings = read_json(root, ".claude/settings.json");
        let entries = settings["hooks"]["SessionStart"]
            .as_array_mut()
            .expect("SessionStart entries");
        let startup = entries
            .iter_mut()
            .find(|entry| entry["matcher"] == json!("startup|resume|clear|fork"))
            .expect("the startup entry");
        startup["hooks"] = change.clone();
        write_json(root, ".claude/settings.json", &settings);
        commit_all(root, "our hook");

        let lines = update(root);

        let kept = read_json(root, ".claude/settings.json")["hooks"]["SessionStart"]
            .as_array()
            .expect("SessionStart entries")
            .iter()
            .find(|entry| entry["matcher"] == json!("startup|resume|clear|fork"))
            .expect("the startup entry stays")["hooks"]
            .clone();
        assert_eq!(kept, change);
        assert!(!lines.contains(&STARTUP_REMOVED.to_string()), "{lines:?}");
        assert!(
            !lines
                .iter()
                .any(|line| line.contains(".claude/settings.json")
                    && !line.contains("reference workflow SessionStart hook added")),
            "{lines:?}"
        );
    }
}

/// C40: with `.claude/settings.json` in `ownedPaths` (the `overrides` key of a
/// 1.x marker), the startup entry stays and `update` prints no line for the file.
#[test]
fn an_owned_settings_file_keeps_the_startup_hook_entry_without_a_line() {
    let dir = install_1_4_0();
    let root = dir.path();
    edit_marker(root, |marker| {
        marker.insert("overrides".to_string(), json!([".claude/settings.json"]));
    });
    commit_all(root, "owned");

    let lines = update(root);

    assert!(session_start_commands(root).contains(&STARTUP_COMMAND.to_string()));
    assert!(
        !lines
            .iter()
            .any(|line| line.contains(".claude/settings.json")),
        "{lines:?}"
    );
}

// --- Steps 2 to 4: a stop after any write finishes at the next update ------

/// What stops `update` at one write (`houserules.migrations-finish-after-a-stop`).
#[derive(Clone, Copy)]
enum Obstacle<'a> {
    /// The file at the path exists and is read-only, so the write to it fails and
    /// the writes before it succeed. A user who may write anywhere (`root`) is not
    /// stopped, and the case ends at once.
    ReadOnlyFile(&'a str),
    /// A directory takes the place of the file that `update` creates: the run
    /// stops when it reaches the path, after the writes before it.
    DirectoryInPlaceOfFile(&'a str),
}

/// Interrupts `update` on the install that `build` makes, at the write that
/// `obstacle` stops, and finishes it with one more `update`. `left` asserts what
/// the stop left behind, so that the case proves where the run stopped. The
/// finished tree equals the tree of a run that nothing stopped (spec 4.7, the
/// invariant of the steps).
fn assert_finishes_after_a_stop_at(
    build: impl Fn() -> tempfile::TempDir,
    obstacle: Obstacle<'_>,
    left: impl FnOnce(&Path),
) {
    let control = build();
    update(control.path());

    let dir = build();
    let lock = match obstacle {
        Obstacle::ReadOnlyFile(path) => {
            let Some(lock) = locked_file::lock(&path_of(dir.path(), path)) else {
                return;
            };
            Some(lock)
        }
        Obstacle::DirectoryInPlaceOfFile(path) => {
            fs::create_dir_all(path_of(dir.path(), path)).expect("place the directory");
            None
        }
    };
    let failed = update_output(dir.path());
    drop(lock);
    if let Obstacle::DirectoryInPlaceOfFile(path) = obstacle {
        fs::remove_dir(path_of(dir.path(), path)).expect("remove the directory");
    }
    assert_eq!(failed.status.code(), Some(2), "{}", stderr(&failed));
    left(dir.path());
    update(dir.path());

    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        "the update that a stop interrupted",
    );
}

/// `true` when the marker of `dir` has a `workflow` key.
fn workflow_is_pinned(dir: &Path) -> bool {
    read_json(dir, ".houserules.json")["workflow"].is_object()
}

/// A batch-33 install with `--target claude` and no record of eval runs, so that
/// the adoption writes the data file of the workflow.
fn batch_33_install_without_the_eval_record() -> tempfile::TempDir {
    let dir = batch_33_install(true);
    fs::remove_file(path_of(dir.path(), ".claude/evals/record.json")).expect("remove the record");
    dir
}

/// A batch-33 install with `--target claude` that one `update` adopted, and that
/// then holds the startup hook entry again: the next `update` has only the
/// settings cleanup of C40 to write.
fn adopted_install_with_the_startup_entry() -> tempfile::TempDir {
    let dir = batch_33_install(true);
    update(dir.path());
    add_the_startup_entry(dir.path());
    commit_all(dir.path(), "the startup entry");
    dir
}

/// A batch-33 install that one `update` adopted, whose deliverables schema is
/// the batch-33 content again and has no baseline: the next `update` has only
/// the baseline of step 4 to write.
fn adopted_install_with_an_undelivered_schema() -> tempfile::TempDir {
    let dir = batch_33_install(false);
    update(dir.path());
    write_file(
        dir.path(),
        DELIVERABLES,
        &kit_content_at(BATCH_33, DELIVERABLES).expect("the batch-33 schema"),
    );
    edit_marker(dir.path(), |marker| {
        marker["baselines"]
            .as_object_mut()
            .expect("baselines")
            .remove(DELIVERABLES);
    });
    commit_all(dir.path(), "an undelivered schema");
    dir
}

/// The write of a file of the workflow in the middle of the adoption stops the
/// run after the files before it. The marker holds no pin yet, and the next
/// `update` adopts again. Of the files that a batch-33 install holds, only the
/// agent templates differ from the package.
#[test]
fn an_update_that_stops_at_a_file_of_the_workflow_finishes_at_the_next_update() {
    let reviewer = ".claude/agents/task-reviewer.md";
    assert_finishes_after_a_stop_at(
        || batch_33_install(true),
        Obstacle::ReadOnlyFile(reviewer),
        |dir| {
            assert!(!workflow_is_pinned(dir));
            let written = ".claude/agents/implementer.md";
            assert_eq!(read_file(dir, written), package_content(written));
            assert_ne!(read_file(dir, reviewer), package_content(reviewer));
        },
    );
}

/// The reference script is the last file of the workflow, and absent from a
/// batch-33 install. The run stops when it reaches the script, with every other
/// file written and no pin.
#[test]
fn an_update_that_stops_at_the_last_file_of_the_workflow_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        || batch_33_install(true),
        Obstacle::DirectoryInPlaceOfFile(REFERENCE_SCRIPT),
        |dir| {
            assert!(!workflow_is_pinned(dir));
            let hook = ".githooks/commit-msg";
            assert_eq!(read_file(dir, hook), package_content(hook));
        },
    );
}

/// The data file of the workflow follows its files. The run stops at it with all
/// files written and no pin.
#[test]
fn an_update_that_stops_at_the_eval_record_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        batch_33_install_without_the_eval_record,
        Obstacle::DirectoryInPlaceOfFile(".claude/evals/record.json"),
        |dir| {
            assert!(!workflow_is_pinned(dir));
            assert_eq!(
                read_file(dir, REFERENCE_SCRIPT),
                package_content(REFERENCE_SCRIPT)
            );
        },
    );
}

/// The first write of the marker is the pin of the adoption. The run stops at it
/// with every file written.
#[test]
fn an_update_that_stops_at_the_pin_of_the_adoption_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        || batch_33_install(true),
        Obstacle::ReadOnlyFile(".houserules.json"),
        |dir| {
            assert!(!workflow_is_pinned(dir));
            assert_eq!(
                read_file(dir, REFERENCE_SCRIPT),
                package_content(REFERENCE_SCRIPT)
            );
        },
    );
}

/// The merge of the hook entry into `.claude/settings.json` follows the pin. The
/// run stops at it with the pin written and the hook entry missing.
#[test]
fn an_update_that_stops_at_the_hook_entry_of_the_workflow_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        || batch_33_install(true),
        Obstacle::ReadOnlyFile(".claude/settings.json"),
        |dir| {
            assert!(workflow_is_pinned(dir));
            assert!(
                !session_start_commands(dir).contains(&REFERENCE_COMMAND.to_string()),
                "the hook entry is not merged yet"
            );
        },
    );
}

/// The cleanup of C40 is the one write to `.claude/settings.json` of an install
/// that the adoption finished. The run stops at it with the startup entry in
/// place.
#[test]
fn an_update_that_stops_at_the_startup_hook_cleanup_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        adopted_install_with_the_startup_entry,
        Obstacle::ReadOnlyFile(".claude/settings.json"),
        |dir| {
            assert!(session_start_commands(dir).contains(&STARTUP_COMMAND.to_string()));
        },
    );
}

/// The baseline of step 4 is the one write to the marker of an install that the
/// adoption finished. The run stops at it with the schema unchanged and no
/// baseline.
#[test]
fn an_update_that_stops_at_the_baseline_of_a_schema_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        adopted_install_with_an_undelivered_schema,
        Obstacle::ReadOnlyFile(".houserules.json"),
        |dir| {
            assert!(baseline(dir, DELIVERABLES).is_null());
            assert_eq!(
                read_file(dir, DELIVERABLES),
                kit_content_at(BATCH_33, DELIVERABLES).expect("the batch-33 schema")
            );
        },
    );
}

// --- The 1.x conversion offers the opt-in entries (spec 3.7) ---------------

/// The every-commit entry of the process pack: the one entry that a kit pack
/// lists under `optIn`.
const OPT_IN_ENTRY: &str = "process.every-commit-names-a-work-item";

/// The line that says an offered entry is excluded until the project deletes
/// its exclusion.
fn offer_line() -> String {
    format!(
        "{OPT_IN_ENTRY} is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion"
    )
}

/// How many of `lines` are the offer line of `OPT_IN_ENTRY`.
fn offer_lines_in(lines: &[String]) -> usize {
    lines.iter().filter(|line| **line == offer_line()).count()
}

/// The `exclusions` of `knowledge/overrides.json` of `dir`, or none when the
/// file does not exist.
fn exclusions_of(dir: &Path) -> Vec<Value> {
    if !path_of(dir, "knowledge/overrides.json").exists() {
        return Vec::new();
    }
    read_json(dir, "knowledge/overrides.json")["exclusions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The exclusion that an offer writes for `OPT_IN_ENTRY`.
fn opt_in_exclusion() -> Value {
    json!({"id": OPT_IN_ENTRY, "reason": "opt-in"})
}

/// The ids that the marker of `dir` records in `optInOffered`.
fn offered_ids(dir: &Path) -> Vec<String> {
    read_json(dir, ".houserules.json")["optInOffered"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .map(|id| id.as_str().expect("an id").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Runs `check-commit` on a message file that holds `message`, outside the
/// install, and returns the output.
fn check_commit_message(dir: &Path, message: &str) -> Output {
    let messages = tempfile::tempdir().expect("tempdir");
    let file = messages.path().join("COMMIT_EDITMSG");
    fs::write(&file, message).expect("write the message");
    run(dir, &["check-commit", file.to_str().expect("a utf-8 path")])
}

/// Spec 3.7 ("the 1.x conversion" is a path that brings an `optIn` id onto an
/// install) and live case 14 ("the every-commit entry causes none: it arrives
/// excluded"): a 1.4.0 install through `update` has the exclusion with the
/// reason `opt-in` and nothing else excluded, `optInOffered` holds the id, the
/// offer line prints once, and `check-commit` passes a message with no
/// trailer. A second `update` writes nothing and prints no offer line.
#[test]
fn a_1_4_0_install_converts_with_the_every_commit_entry_excluded_and_offered_once() {
    let dir = install_1_4_0();
    let root = dir.path();

    let lines = update(root);

    assert_eq!(offer_lines_in(&lines), 1, "{lines:?}");
    assert_eq!(exclusions_of(root), vec![opt_in_exclusion()]);
    assert_eq!(offered_ids(root), [OPT_IN_ENTRY]);
    let got = run_ok(root, &["get", OPT_IN_ENTRY]);
    let record: Value = serde_json::from_str(&stdout(&got)).expect("get prints JSON");
    assert_eq!(record[0]["excluded"], json!("opt-in"));
    passes(root, &["check-knowledge"]);
    passes(root, &["render", "--check"]);
    tracker_support::assert_output(
        &check_commit_message(root, "chore: x\n"),
        0,
        "check-commit: ok\n",
        "",
    );

    commit_all(root, "update");
    let second = update(root);
    assert_eq!(offer_lines_in(&second), 0, "{second:?}");
    assert_no_change_lines(&second, &[]);
    assert_eq!(porcelain(root), "", "a second update changes no file");
    assert_eq!(exclusions_of(root), vec![opt_in_exclusion()]);
    assert_eq!(offered_ids(root), [OPT_IN_ENTRY]);
}

/// Spec 3.7: a project that deleted the exclusion after the conversion has
/// opted in for good. The next `update` writes no exclusion, prints no offer
/// line, and leaves `optInOffered` as it was; `check-commit` then fails a
/// message with no trailer.
#[test]
fn a_project_that_opted_in_after_the_conversion_stays_opted_in() {
    let dir = install_1_4_0();
    let root = dir.path();
    update(root);
    let mut overrides = read_json(root, "knowledge/overrides.json");
    overrides["exclusions"] = json!([]);
    write_json(root, "knowledge/overrides.json", &overrides);
    run_ok(root, &["render"]);
    commit_all(root, "opt in");

    let second = update(root);

    assert_eq!(offer_lines_in(&second), 0, "{second:?}");
    assert_eq!(exclusions_of(root), Vec::<Value>::new());
    assert_eq!(offered_ids(root), [OPT_IN_ENTRY]);
    assert_eq!(porcelain(root), "", "the second update changes no file");
    tracker_support::assert_output(
        &check_commit_message(root, "chore: x\n"),
        1,
        "",
        &format!("no Work-Item trailer ({OPT_IN_ENTRY})\n"),
    );
}

/// Interrupts the conversion of the install that `build` makes at the write to
/// `locked`, and finishes it with one more `update`. `left` asserts what the
/// stop left behind and removes what only let the write fail. The case also
/// requires that no stop leaves the entry live (the copy of the process pack
/// never stands without the exclusion), that the offer line prints once over
/// the two runs, and that the finished tree equals the tree of a run that no
/// stop interrupted (`houserules.migrations-finish-after-a-stop`).
fn assert_the_offer_survives_a_stop_at(
    locked: &str,
    build: impl Fn() -> tempfile::TempDir,
    left: impl FnOnce(&Path),
) {
    let control = build();
    let control_lines = update(control.path());
    assert_eq!(offer_lines_in(&control_lines), 1, "{control_lines:?}");

    let dir = build();
    let Some(lock) = locked_file::lock(&path_of(dir.path(), locked)) else {
        return;
    };
    let failed = update_output(dir.path());
    drop(lock);
    assert_eq!(
        failed.status.code(),
        Some(2),
        "the write to {locked} stops the run: {}",
        stderr(&failed)
    );
    assert!(
        read_json(dir.path(), ".houserules.json")
            .get("schema")
            .is_none(),
        "the schema key is written last"
    );
    if path_of(dir.path(), "knowledge/packs/process.json").exists() {
        assert_eq!(
            exclusions_of(dir.path()),
            vec![opt_in_exclusion()],
            "the stopped tree has the copy of the process pack, so it has the exclusion"
        );
    }
    left(dir.path());
    let finished = update(dir.path());

    let failed_lines: Vec<String> = stdout(&failed).lines().map(str::to_string).collect();
    assert_eq!(
        offer_lines_in(&failed_lines) + offer_lines_in(&finished),
        1,
        "the offer line prints once over the two runs: {failed_lines:?} then {finished:?}"
    );
    assert_eq!(exclusions_of(dir.path()), vec![opt_in_exclusion()]);
    assert_eq!(offered_ids(dir.path()), [OPT_IN_ENTRY]);
    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        &format!("the conversion that stopped at the write to {locked}"),
    );
}

/// A 1.4.0 install whose `ownedPaths` lists the topic file of the
/// `knowledge-base` pack, so that the conversion leaves that pack out and the
/// process pack is the first pack it installs: the pin of the process pack is
/// the first write to the marker.
fn install_1_4_0_whose_first_pack_is_process() -> tempfile::TempDir {
    let dir = install_1_4_0();
    edit_marker(dir.path(), |marker| {
        marker.insert(
            "ownedPaths".to_string(),
            json!(["knowledge/knowledge-base.json"]),
        );
    });
    dir
}

/// The write of the exclusion stops the run: `knowledge/overrides.json` exists
/// and is read-only. The process pack is not installed, nothing is recorded,
/// and the next `update` offers the entry.
#[test]
fn a_conversion_that_stops_at_the_exclusion_finishes_with_one_offer_line() {
    assert_the_offer_survives_a_stop_at(
        "knowledge/overrides.json",
        || {
            let dir = install_1_4_0();
            write_json(
                dir.path(),
                "knowledge/overrides.json",
                &json!({
                    "schema": "houserules/overrides@1.0.0",
                    "overrides": [],
                    "exclusions": []
                }),
            );
            dir
        },
        |dir| {
            assert!(!path_of(dir, "knowledge/packs/process.json").exists());
            assert!(read_json(dir, ".houserules.json")["packs"]["process"].is_null());
            assert_eq!(offered_ids(dir), Vec::<String>::new());
            assert_eq!(exclusions_of(dir), Vec::<Value>::new());
        },
    );
}

/// The write of the pin stops the run after the exclusion is written and
/// before the marker holds the pin and the record: the stop that the brief of
/// this round names. The next `update` records the id, finds the exclusion in
/// place, and prints the line once.
#[test]
fn a_conversion_that_stops_at_the_marker_after_the_exclusion_finishes_with_one_offer_line() {
    assert_the_offer_survives_a_stop_at(
        ".houserules.json",
        install_1_4_0_whose_first_pack_is_process,
        |dir| {
            assert_eq!(exclusions_of(dir), vec![opt_in_exclusion()]);
            assert!(read_json(dir, ".houserules.json")["packs"]["process"].is_null());
            assert_eq!(offered_ids(dir), Vec::<String>::new());
            assert!(!path_of(dir, "knowledge/packs/process.json").exists());
        },
    );
}

/// The write of the copy stops the run after the marker holds the pin and the
/// record. The next `update` finds the pin without a copy, writes the copy, and
/// prints no second offer line: the first run printed it when it recorded the
/// id.
#[test]
fn a_conversion_that_stops_at_the_copy_finishes_without_a_second_offer_line() {
    assert_the_offer_survives_a_stop_at(
        "knowledge/packs/process.json",
        || {
            let dir = install_1_4_0();
            let path = path_of(dir.path(), "knowledge/packs/process.json");
            fs::create_dir_all(path.parent().expect("a parent")).expect("create knowledge/packs/");
            let pack = fs::read_to_string(repo_root().join("template/packs/process.json"))
                .expect("read the process pack");
            let compact: Value = serde_json::from_str(&pack).expect("parse the process pack");
            fs::write(path, serde_json::to_string(&compact).expect("serialize"))
                .expect("write the copy in a form that update would not write");
            dir
        },
        |dir| {
            assert_eq!(exclusions_of(dir), vec![opt_in_exclusion()]);
            assert!(read_json(dir, ".houserules.json")["packs"]["process"].is_object());
            assert_eq!(offered_ids(dir), [OPT_IN_ENTRY]);
            fs::remove_file(path_of(dir, "knowledge/packs/process.json"))
                .expect("delete the stand-in for the copy");
        },
    );
}

// --- C35: a kit-owned file that equals a known content is the kit's --------

/// C35 (spec 4.7): `init` replaces a kit-owned file whose bytes equal a release
/// content, as it replaces one at its baseline, and baselines the new file.
#[test]
fn init_replaces_a_kit_owned_file_that_equals_a_release_content() {
    let dir = scratch_git_repo();
    let old = kit_content_at("v1.0.0", MIGRATING_SKILL).expect("the 1.0.0 skill");
    write_file(dir.path(), MIGRATING_SKILL, &old);

    let output = run_ok(dir.path(), &["init"]);

    let lines = stdout(&output);
    assert!(
        lines
            .lines()
            .any(|line| line == format!("wrote {MIGRATING_SKILL}")),
        "{lines}"
    );
    assert_eq!(
        read_file(dir.path(), MIGRATING_SKILL),
        core_content(MIGRATING_SKILL)
    );
    assert_eq!(
        baseline(dir.path(), MIGRATING_SKILL),
        json!(sha256_hex(&core_content(MIGRATING_SKILL)))
    );
}

/// `init` keeps a kit-owned file that equals no known content and does not
/// baseline it. Batch 33 behaves so: this test pins it.
#[test]
fn init_keeps_a_kit_owned_file_that_equals_no_known_content() {
    let dir = scratch_git_repo();
    write_file(dir.path(), MIGRATING_SKILL, b"Our own skill.\n");

    let output = run_ok(dir.path(), &["init"]);

    assert!(
        stdout(&output)
            .lines()
            .any(|line| line == format!("kept {MIGRATING_SKILL}"))
    );
    assert_eq!(read_file(dir.path(), MIGRATING_SKILL), b"Our own skill.\n");
    assert!(baseline(dir.path(), MIGRATING_SKILL).is_null());
}

/// C35: `update` replaces a kit-owned file that equals a release content
/// although the marker records no baseline for it.
#[test]
fn update_replaces_a_kit_owned_file_that_equals_a_release_content_without_a_baseline() {
    let dir = install_without_backlog();
    let root = dir.path();
    write_file(
        root,
        MIGRATING_SKILL,
        &kit_content_at("v1.4.0", MIGRATING_SKILL).expect("the 1.4.0 skill"),
    );
    edit_marker(root, |marker| {
        marker["baselines"]
            .as_object_mut()
            .expect("baselines")
            .remove(MIGRATING_SKILL);
    });
    commit_all(root, "an old skill");

    let lines = update(root);

    assert!(
        lines.contains(&format!("wrote {MIGRATING_SKILL}")),
        "{lines:?}"
    );
    assert_eq!(
        read_file(root, MIGRATING_SKILL),
        core_content(MIGRATING_SKILL)
    );
}

/// `update` keeps a changed kit-owned file with no recorded baseline and lists
/// it. Batch 33 behaves so: this test pins it.
#[test]
fn update_keeps_a_changed_kit_owned_file_without_a_baseline() {
    let dir = install_without_backlog();
    let root = dir.path();
    write_file(root, MIGRATING_SKILL, b"Our own skill.\n");
    edit_marker(root, |marker| {
        marker["baselines"]
            .as_object_mut()
            .expect("baselines")
            .remove(MIGRATING_SKILL);
    });
    commit_all(root, "our skill");

    let lines = update(root);

    assert!(
        lines.contains(&format!("kept {MIGRATING_SKILL} (locally modified)")),
        "{lines:?}"
    );
    assert_eq!(read_file(root, MIGRATING_SKILL), b"Our own skill.\n");
}

// --- Beads: the prefix refresh of `update` (spec 3.3, C37, C38) ------------

/// A program that runs as `bd`: the shell script `body`, written below `dir`.
#[cfg(unix)]
fn script_bd(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(".git").join("stand-in-bd");
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write the stand-in for bd");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("make it executable");
    path
}

/// Runs `update` in `dir` with `program` as `bd`, in the `bd` environment below
/// `dir`.
#[cfg(unix)]
fn update_with_bd(dir: &Path, program: &Path) -> Output {
    let mut command = tracker_support::houserules();
    command
        .args(["update", "--dir"])
        .arg(dir)
        .env("HOUSERULES_BD", program);
    tracker_support::hermetic_bd(&mut command, &dir.join(".git/bd-home"));
    command.output().expect("run houserules update")
}

/// Spec 3.3: `update` re-reads the issue prefix where `bd` is reachable and
/// rewrites the pattern when it changed; a second `update` with the same
/// prefix writes nothing. Where `bd` cannot be reached, `update` keeps the
/// pattern, prints the unreachable line of spec 3.4 on stderr, and finishes with
/// exit 0 (C38).
#[cfg(unix)]
#[test]
fn update_refreshes_the_beads_pattern_and_keeps_it_when_bd_is_unreachable() {
    let dir = install_without_backlog();
    let root = dir.path();
    edit_marker(root, |marker| {
        marker.insert("tracker".to_string(), json!({"adapter": "beads"}));
    });
    commit_all(root, "beads");
    let neo = script_bd(root, r#"echo '{"key":"issue_prefix","value":"neo"}'"#);

    let first = update_with_bd(root, &neo);

    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert_eq!(backlog_id_pattern(root), "^neo-[0-9a-z]+(\\.[0-9]+)*$");
    assert!(
        stdout(&first)
            .lines()
            .any(|line| line == format!("wrote {DELIVERABLES}")),
        "{}",
        stdout(&first)
    );
    commit_all(root, "neo");
    let again = update_with_bd(root, &neo);
    assert_eq!(again.status.code(), Some(0));
    assert!(!stdout(&again).contains(DELIVERABLES), "{}", stdout(&again));
    assert_eq!(porcelain(root), "", "a second update changes no file");

    let old = script_bd(root, r#"echo '{"key":"issue_prefix","value":"old"}'"#);
    let changed = update_with_bd(root, &old);
    assert_eq!(changed.status.code(), Some(0), "{}", stderr(&changed));
    assert_eq!(backlog_id_pattern(root), "^old-[0-9a-z]+(\\.[0-9]+)*$");

    commit_all(root, "old");
    let broken = script_bd(root, r#"echo "Error: database is locked" >&2; exit 1"#);
    let kept = update_with_bd(root, &broken);
    assert_eq!(kept.status.code(), Some(0), "{}", stderr(&kept));
    assert_eq!(
        stderr(&kept),
        "tracker beads cannot be reached: Error: database is locked; run the command by hand to see the full error\n"
    );
    assert_eq!(backlog_id_pattern(root), "^old-[0-9a-z]+(\\.[0-9]+)*$");
    assert_eq!(porcelain(root), "", "an unreachable bd changes no file");
}
