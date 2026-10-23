//! The reference workflow package (batch 34, spec 4.1 to 4.3): `init
//! --workflow reference` installs its files, pins them in `.houserules.json`,
//! and checks its requirements; `update` moves the pins with the binary and
//! keeps what the project changed. Each case runs the compiled binary
//! against a scratch git repository. The package lives under
//! `template/workflows/reference/`; the tests that read it there do not
//! install anything.

mod tracker_support;
mod tree_snapshot;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Map, Value, json};
use tempfile::TempDir;
use tracker_support::{
    assert_output, edit_marker, read_json, repo_root, run, run_ok, scratch_git_repo, sha256_hex,
    stderr, stdout, write_json,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The refusal of `init --workflow reference` with no tracker (spec 4.3).
const WORKFLOW_NEEDS_TRACKER: &str = "the reference workflow needs a tracker; run houserules init --workflow reference --tracker json|github|beads";

/// The line of `check-knowledge` for an install with the reference workflow
/// and no tracker (spec 3.10).
const WORKFLOW_NEEDS_TRACKER_CHECK: &str =
    "the reference workflow needs a tracker; run houserules init --tracker json|github|beads";

/// The line of `check-knowledge` for an install with the process pack and no
/// tracker (spec 3.10).
const PACK_NEEDS_TRACKER_CHECK: &str =
    "pack process needs a tracker; run houserules init --tracker json|github|beads";

/// The line that `init --workflow reference` prints for the packs that hold
/// the rules the workflow requires and the install lacks.
const REQUIRED_PACKS_LINE: &str = "the reference workflow requires rules from the packs process, security-hygiene, writing-style; run houserules init --workflow reference --add-required-packs to add them";

/// The packs that provide the 29 required ids, with `knowledge-base` among
/// them, in name order.
const REQUIRED_PACKS: [&str; 4] = [
    "knowledge-base",
    "process",
    "security-hygiene",
    "writing-style",
];

/// The id of the every-commit entry, which no workflow file may cite.
const EVERY_COMMIT_ENTRY: &str = "process.every-commit-names-a-work-item";

/// The text of `agentsBlock` in the manifest (spec 4.1).
const AGENTS_BLOCK: &str = "This install runs the reference workflow. Read `.claude/skills/orchestrating/SKILL.md` at the start of a session and before any handoff. If your harness can hand a task to another agent the way `.claude/agents/*.md` describes, run handoffs in parallel only when the plan marks their file sets disjoint, each in its own worktree; a harness with no subagents does the work directly, in its own session.";

/// The project data file that `init --workflow reference` writes empty.
const RECORD: &str = ".claude/evals/record.json";

/// The version of the binary under test.
fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// The directory of the package in the source tree.
fn package_dir() -> PathBuf {
    repo_root().join("template/workflows/reference")
}

/// The manifest of the package, as the source tree holds it.
fn manifest() -> Value {
    let text = fs::read_to_string(package_dir().join("workflow.json")).expect("read workflow.json");
    serde_json::from_str(&text).expect("workflow.json is JSON")
}

/// The install paths that the manifest lists.
fn manifest_files() -> Vec<String> {
    manifest()["files"]
        .as_array()
        .expect("files is an array")
        .iter()
        .map(|file| file.as_str().expect("a path").to_string())
        .collect()
}

/// The path `relative` below `dir`, built one component at a time.
fn path_of(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The lines that `output` printed on stdout.
fn lines(output: &std::process::Output) -> Vec<String> {
    stdout(output).lines().map(str::to_string).collect()
}

/// A scratch install with the JSON tracker and the reference workflow, and
/// no pack beyond `knowledge-base`.
fn workflow_install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(
        dir.path(),
        &["init", "--workflow", "reference", "--tracker", "json"],
    );
    dir
}

/// A scratch install with the JSON tracker, the reference workflow, and the
/// packs that provide the required ids.
fn complete_workflow_install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(
        dir.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--add-required-packs",
        ],
    );
    dir
}

/// Spec 4.3 and live case 11: with no tracker installed and none asked for,
/// `init --workflow reference` refuses before it writes anything.
#[test]
fn init_workflow_reference_without_a_tracker_refuses_and_changes_nothing() {
    let dir = scratch_git_repo();
    let output = run(dir.path(), &["init", "--workflow", "reference"]);
    assert_output(&output, 2, "", &format!("{WORKFLOW_NEEDS_TRACKER}\n"));
    assert_eq!(snapshot(dir.path()).len(), 0);

    run_ok(dir.path(), &["init"]);
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["init", "--workflow", "reference"]);
    assert_output(&output, 2, "", &format!("{WORKFLOW_NEEDS_TRACKER}\n"));
    assert_same_tree(&before, &snapshot(dir.path()), "the refused init");
}

/// Spec 4.1, 4.3, and live case 11: with a tracker, `init --workflow
/// reference` writes every file of the manifest and an empty `record.json`,
/// and pins each file by the SHA-256 of what it wrote.
#[test]
fn init_workflow_reference_writes_the_package_and_pins_each_file() {
    let dir = scratch_git_repo();
    let output = run_ok(
        dir.path(),
        &["init", "--workflow", "reference", "--tracker", "json"],
    );
    let printed = lines(&output);
    let files = manifest_files();
    assert_eq!(
        printed
            .iter()
            .filter(|line| *line == "wrote .claude/schemas/deliverables.json")
            .count(),
        1,
        "the schema is written once, with the pattern of the tracker: {printed:?}"
    );
    for file in &files {
        assert!(
            printed.contains(&format!("wrote {file}")),
            "no wrote line for {file} in {printed:?}"
        );
        let installed = fs::read(path_of(dir.path(), file)).expect("read an installed file");
        let source = fs::read(path_of(&package_dir(), file)).expect("read the package file");
        assert_eq!(installed, source, "{file} differs from the package");
    }
    assert!(printed.contains(&format!("wrote {RECORD}")), "{printed:?}");
    assert_eq!(
        fs::read(path_of(dir.path(), RECORD)).expect("read record.json"),
        fs::read(path_of(&package_dir(), RECORD)).expect("read the package record.json")
    );
    assert_eq!(read_json(dir.path(), RECORD), json!([]));
    assert!(
        printed.contains(&format!("workflow: reference {}", version())),
        "{printed:?}"
    );

    let marker = read_json(dir.path(), ".houserules.json");
    let pins = &marker["workflow"];
    assert_eq!(pins["name"], json!("reference"));
    assert_eq!(pins["version"], json!(version()));
    let pinned: BTreeSet<&String> = pins["files"]
        .as_object()
        .expect("files is an object")
        .keys()
        .collect();
    assert_eq!(pinned, files.iter().collect::<BTreeSet<_>>());
    for file in &files {
        let bytes = fs::read(path_of(dir.path(), file)).expect("read an installed file");
        assert_eq!(
            pins["files"][file],
            json!(sha256_hex(&bytes)),
            "pin of {file}"
        );
    }
    assert_eq!(
        pins["models"],
        json!({
            "implementer": "sonnet",
            "task-reviewer": "opus",
            "branch-reviewer": "opus",
            "spec-reviewer": "opus",
        })
    );
    assert!(
        !dir.path().join(".claude/settings.json").exists(),
        "the hook entry needs the Claude renderer"
    );
    assert!(!dir.path().join("tools/claude-session-start.sh").exists());
}

/// Spec 4.3: the workflow writes its files whatever the render targets, and
/// a moved agent template carries no `model:` line (spec 4.1: one source for
/// each role's model).
#[test]
fn an_installed_agent_template_carries_no_model_line() {
    let dir = workflow_install();
    for file in manifest_files() {
        if !file.starts_with(".claude/agents/") {
            continue;
        }
        let text = fs::read_to_string(path_of(dir.path(), &file)).expect("read an agent template");
        let frontmatter = text.split("\n---\n").next().expect("a front matter");
        assert!(
            !frontmatter.lines().any(|line| line.starts_with("model:")),
            "{file} carries a model line"
        );
    }
}

/// Spec 4.3 and live case 11: without `--add-required-packs`, `init` prints
/// the packs that hold the required rules and adds none.
#[test]
fn init_workflow_reference_prints_the_packs_that_hold_the_required_ids() {
    let dir = scratch_git_repo();
    let output = run_ok(
        dir.path(),
        &["init", "--workflow", "reference", "--tracker", "json"],
    );
    assert!(
        lines(&output).contains(&REQUIRED_PACKS_LINE.to_string()),
        "{}",
        stdout(&output)
    );
    let marker = read_json(dir.path(), ".houserules.json");
    let packs: Vec<&String> = marker["packs"].as_object().expect("packs").keys().collect();
    assert_eq!(packs, ["knowledge-base"]);
}

/// Spec 4.3, 3.7, and live case 11: with `--add-required-packs`, `init` adds
/// the packs that hold the required rules, offers the opt-in entry of the
/// process pack once, and every gate passes while its exclusion is in place.
#[test]
fn init_with_add_required_packs_installs_the_packs_and_every_gate_passes() {
    let dir = scratch_git_repo();
    let output = run_ok(
        dir.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--add-required-packs",
        ],
    );
    let printed = lines(&output);
    assert!(
        !printed.contains(&REQUIRED_PACKS_LINE.to_string()),
        "{printed:?}"
    );
    let offer = format!(
        "{EVERY_COMMIT_ENTRY} is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion"
    );
    assert!(printed.contains(&offer), "no offer line in {printed:?}");
    let marker = read_json(dir.path(), ".houserules.json");
    let packs: Vec<&String> = marker["packs"].as_object().expect("packs").keys().collect();
    assert_eq!(packs, REQUIRED_PACKS);
    assert_eq!(marker["optInOffered"], json!([EVERY_COMMIT_ENTRY]));
    let overrides = read_json(dir.path(), "knowledge/overrides.json");
    assert_eq!(
        overrides["exclusions"],
        json!([{"id": EVERY_COMMIT_ENTRY, "reason": "opt-in"}])
    );

    for args in [
        vec!["check-knowledge"],
        vec!["check-backlog"],
        vec!["render", "--check"],
    ] {
        let gate = run(dir.path(), &args);
        assert_eq!(
            gate.status.code(),
            Some(0),
            "{args:?}: {}{}",
            stdout(&gate),
            stderr(&gate)
        );
    }
}

/// `--add-required-packs` adds the packs of a workflow, so it needs one.
#[test]
fn add_required_packs_needs_the_workflow_flag() {
    let dir = scratch_git_repo();
    let output = run(dir.path(), &["init", "--add-required-packs"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("--workflow"),
        "got {}",
        stderr(&output)
    );
    assert_eq!(snapshot(dir.path()).len(), 0);
}

/// Spec 3.3 and live case 11: `init --tracker json` and then `init
/// --workflow reference --tracker json` both exit 0 on a JSON install.
#[test]
fn init_workflow_on_an_install_with_the_same_tracker_exits_0() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    run_ok(
        dir.path(),
        &["init", "--workflow", "reference", "--tracker", "json"],
    );
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "json"}));
    assert_eq!(marker["workflow"]["name"], json!("reference"));
}

/// The commands of the `SessionStart` entries of the `.claude/settings.json`
/// below `dir`, in file order.
fn session_start_commands(dir: &Path) -> Vec<String> {
    read_json(dir, ".claude/settings.json")["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart is an array")
        .iter()
        .flat_map(|entry| entry["hooks"].as_array().expect("an entry holds hooks"))
        .map(|hook| hook["command"].as_str().expect("a command").to_string())
        .collect()
}

/// Resolves the `SessionStart` entry that names the reference session-start
/// script the way Claude Code does (the variable `CLAUDE_PROJECT_DIR` is the
/// install root) and runs the script that it names. Requires one such entry
/// and an installed script, and returns what the script printed.
fn run_the_reference_session_start_hook(dir: &Path) -> Output {
    let commands: Vec<String> = session_start_commands(dir)
        .into_iter()
        .filter(|command| command.contains("reference-session-start.sh"))
        .collect();
    assert_eq!(
        commands.len(),
        1,
        "one SessionStart entry runs the script: {:?}",
        session_start_commands(dir)
    );
    let relative = commands[0]
        .strip_prefix("${CLAUDE_PROJECT_DIR}/")
        .unwrap_or_else(|| panic!("{} does not start at the project dir", commands[0]));
    assert_eq!(relative, "tools/reference-session-start.sh");
    assert!(
        path_of(dir, relative).is_file(),
        "the entry names {relative}, which the install lacks"
    );
    // On Windows, `Command::new` searches the system directory before PATH,
    // so `bash` resolves to the WSL launcher, which exits 1 when no
    // distribution is installed; `sh` resolves to the Git for Windows bash.
    let shell = if cfg!(windows) { "sh" } else { "bash" };
    let output = Command::new(shell)
        .arg(relative)
        .current_dir(dir)
        .output()
        .expect("run the hook script");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).starts_with("Session ritual: "),
        "got {}",
        stdout(&output)
    );
    output
}

/// The matcher of each `SessionStart` entry of `.claude/settings.json`, with
/// `None` for an entry that has none.
fn session_start_matchers(dir: &Path) -> Vec<Option<String>> {
    read_json(dir, ".claude/settings.json")["hooks"]["SessionStart"]
        .as_array()
        .expect("SessionStart is an array")
        .iter()
        .map(|entry| entry["matcher"].as_str().map(str::to_string))
        .collect()
}

/// The matcher of the one entry of the core session-start script (C40).
fn core_matchers() -> Vec<Option<String>> {
    vec![Some("compact".to_string())]
}

/// Spec 4.1, 4.3 and live case 11: `init --workflow reference --target claude`
/// writes the hook entry of the reference session-start script beside the
/// core entries, and the entry resolves to the installed script and runs it.
#[test]
fn init_workflow_with_target_claude_writes_the_hook_entry_and_it_runs() {
    let dir = scratch_git_repo();
    run_ok(
        dir.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--target",
            "claude",
        ],
    );
    run_the_reference_session_start_hook(dir.path());
    let matchers = session_start_matchers(dir.path());
    assert_eq!(matchers[..1], core_matchers(), "{matchers:?}");
    assert_eq!(matchers.len(), 2, "{matchers:?}");
}

/// Spec 4.3 and live case 11: without the Claude renderer the install has no
/// `.claude/settings.json` and so no hook entry; a later `init --target
/// claude` adds the entry of the reference session-start script, which
/// resolves to the installed script and runs it; a second one changes no byte.
#[test]
fn init_target_claude_later_adds_the_hook_entry() {
    let dir = workflow_install();
    assert!(!dir.path().join(".claude/settings.json").exists());
    run_ok(dir.path(), &["init", "--target", "claude"]);
    run_the_reference_session_start_hook(dir.path());
    let matchers = session_start_matchers(dir.path());
    assert_eq!(matchers[..1], core_matchers(), "{matchers:?}");
    assert_eq!(matchers.len(), 2, "{matchers:?}");
    let before = snapshot(dir.path());
    run_ok(dir.path(), &["init", "--target", "claude"]);
    assert_same_tree(&before, &snapshot(dir.path()), "the second init");
}

/// Spec 4.3: `init --workflow reference` on an install that has the Claude
/// renderer adds the hook entry too, and a rerun of the same command adds it
/// once.
#[test]
fn init_workflow_on_a_claude_install_adds_the_hook_entry_once() {
    let dir = scratch_git_repo();
    run_ok(
        dir.path(),
        &["init", "--tracker", "json", "--target", "claude"],
    );
    assert!(
        !session_start_commands(dir.path())
            .iter()
            .any(|command| command.contains("reference-session-start.sh"))
    );
    let workflow = ["init", "--workflow", "reference", "--tracker", "json"];
    run_ok(dir.path(), &workflow);
    run_the_reference_session_start_hook(dir.path());
    let before = snapshot(dir.path());
    run_ok(dir.path(), &workflow);
    assert_same_tree(&before, &snapshot(dir.path()), "the second init");
    run_the_reference_session_start_hook(dir.path());
}

/// A stop between the writes of `init --workflow reference --target claude`
/// leaves a tree that the next run finishes (`houserules.migrations-finish-
/// after-a-stop`): the hook entry is written before the `workflow` key, so a
/// stop after it leaves the entry and no pins, and the rerun adds no second
/// entry; a stop that lost the entry leaves the pins, and a plain `init` puts
/// the entry back.
#[test]
fn a_stop_around_the_hook_entry_leaves_a_tree_that_the_next_init_finishes() {
    let dir = scratch_git_repo();
    let workflow = [
        "init",
        "--workflow",
        "reference",
        "--tracker",
        "json",
        "--target",
        "claude",
    ];
    run_ok(dir.path(), &workflow);
    let pinned = read_json(dir.path(), ".houserules.json")["workflow"].clone();

    edit_marker(dir.path(), |marker| {
        marker.remove("workflow");
    });
    run_ok(dir.path(), &workflow);
    run_the_reference_session_start_hook(dir.path());
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["workflow"],
        pinned
    );

    let mut settings = read_json(dir.path(), ".claude/settings.json");
    let entries = settings["hooks"]["SessionStart"]
        .as_array_mut()
        .expect("SessionStart is an array");
    entries.truncate(1);
    write_json(dir.path(), ".claude/settings.json", &settings);
    run_ok(dir.path(), &["init"]);
    run_the_reference_session_start_hook(dir.path());
}

/// A `.claude/settings.json` of the project keeps its entries and its other
/// hooks; the hook entry of the reference session-start script goes after
/// them.
#[test]
fn the_hook_entry_merges_into_a_settings_file_of_the_project() {
    let dir = scratch_git_repo();
    let own = json!({
        "permissions": {"allow": ["Bash(ls)"]},
        "hooks": {
            "SessionStart": [
                {"matcher": "startup", "hooks": [{"type": "command", "command": "echo ours"}]}
            ],
            "Stop": [{"hooks": [{"type": "command", "command": "echo stop"}]}],
        },
    });
    write_json(dir.path(), ".claude/settings.json", &own);
    run_ok(
        dir.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--target",
            "claude",
        ],
    );
    run_the_reference_session_start_hook(dir.path());
    let settings = read_json(dir.path(), ".claude/settings.json");
    assert_eq!(settings["permissions"], own["permissions"]);
    assert_eq!(settings["hooks"]["Stop"], own["hooks"]["Stop"]);
    let commands = session_start_commands(dir.path());
    assert_eq!(commands[0], "echo ours", "{commands:?}");
    assert_eq!(commands.len(), 3, "{commands:?}");
}

/// A path in `ownedPaths` is the project's: with `.claude/settings.json`
/// owned, `init` leaves it as found (absent stays absent); with the script
/// owned, `init` writes no script and no entry that names it.
#[test]
fn the_hook_entry_is_left_out_where_the_project_owns_the_settings_or_the_script() {
    let owned_settings = scratch_git_repo();
    run_ok(owned_settings.path(), &["init", "--tracker", "json"]);
    edit_marker(owned_settings.path(), |marker| {
        marker.insert("ownedPaths".to_string(), json!([".claude/settings.json"]));
    });
    run_ok(
        owned_settings.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--target",
            "claude",
        ],
    );
    assert!(
        !owned_settings.path().join(".claude/settings.json").exists(),
        "an owned settings file stays absent"
    );

    let owned_script = scratch_git_repo();
    run_ok(owned_script.path(), &["init", "--tracker", "json"]);
    edit_marker(owned_script.path(), |marker| {
        marker.insert(
            "ownedPaths".to_string(),
            json!(["tools/reference-session-start.sh"]),
        );
    });
    run_ok(
        owned_script.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--target",
            "claude",
        ],
    );
    assert!(
        !owned_script
            .path()
            .join("tools/reference-session-start.sh")
            .exists()
    );
    assert!(
        !session_start_commands(owned_script.path())
            .iter()
            .any(|command| command.contains("reference-session-start.sh")),
        "{:?}",
        session_start_commands(owned_script.path())
    );
}

/// Spec 4.3 and live case 12: a required id that no installed pack provides
/// gets the line that names the workflow, the id, and the pack to add.
#[test]
fn check_knowledge_names_each_requirement_that_no_installed_pack_provides() {
    let dir = workflow_install();
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(1));
    let errors = stderr(&output);
    for (id, pack) in [
        ("process.tdd", "process"),
        ("security-hygiene.exact-pins", "security-hygiene"),
        ("writing-style.principles", "writing-style"),
    ] {
        let line = format!(
            "workflow reference requires {id}: no installed pack provides it; run houserules pack add {pack}"
        );
        assert!(
            errors.lines().any(|found| found == line),
            "no {line:?} in {errors}"
        );
    }
    assert!(
        !errors.contains("workflow reference requires knowledge-base."),
        "the installed pack provides its ids: {errors}"
    );
}

/// Spec 4.3 and live case 12: a required id that `knowledge/overrides.json`
/// excludes gets the line that names the exclusion (design.md 5.112: no
/// waiver).
#[test]
fn check_knowledge_names_a_requirement_that_the_project_excludes() {
    let dir = complete_workflow_install();
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "exclusions": [
                {"id": EVERY_COMMIT_ENTRY, "reason": "opt-in"},
                {"id": "process.tdd", "reason": "not for us"},
            ],
        }),
    );
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(1));
    let line = "workflow reference requires process.tdd, which knowledge/overrides.json excludes; remove the exclusion, and override the entry to change it";
    assert!(
        stderr(&output).lines().any(|found| found == line),
        "no {line:?} in {}",
        stderr(&output)
    );
}

/// Spec 3.10: an install with the workflow and the process pack and no
/// tracker gets one line for each, exit 1.
#[test]
fn check_knowledge_names_a_missing_tracker_for_the_workflow_and_the_process_pack() {
    let dir = complete_workflow_install();
    edit_marker(dir.path(), |marker| {
        marker.remove("tracker");
    });
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(1));
    let errors = stderr(&output);
    for line in [PACK_NEEDS_TRACKER_CHECK, WORKFLOW_NEEDS_TRACKER_CHECK] {
        assert!(
            errors.lines().any(|found| found == line),
            "no {line:?} in {errors}"
        );
    }
}

/// Spec 3.10: the process pack alone, with no workflow, gets only the line of
/// the pack.
#[test]
fn check_knowledge_names_a_missing_tracker_for_the_process_pack_alone() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    run_ok(dir.path(), &["pack", "add", "process"]);
    edit_marker(dir.path(), |marker| {
        marker.remove("tracker");
    });
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(1));
    let errors = stderr(&output);
    assert!(
        errors
            .lines()
            .any(|found| found == PACK_NEEDS_TRACKER_CHECK)
    );
    assert!(!errors.contains(WORKFLOW_NEEDS_TRACKER_CHECK), "{errors}");
}

/// Spec 4.3: the kit-citation lint covers a workflow file that the kit wrote:
/// a citation of an id the base lacks is a finding that names the file.
#[test]
fn the_citation_lint_covers_the_workflow_files_the_kit_pinned() {
    let dir = complete_workflow_install();
    let file = path_of(dir.path(), ".claude/agents/implementer.md");
    let mut text = fs::read_to_string(&file).expect("read the template");
    text.push_str("\nSee `process.gone` too.\n");
    fs::write(&file, text).expect("edit the template");
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(1));
    let errors = stderr(&output);
    assert!(
        errors.contains(".claude/agents/implementer.md") && errors.contains("process.gone"),
        "{errors}"
    );
}

/// A workflow path whose file the project wrote, with no pin, is the
/// project's: the lint reads none of it.
#[test]
fn the_citation_lint_leaves_a_project_own_file_at_a_workflow_path_alone() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    fs::create_dir_all(dir.path().join(".claude/agents")).expect("create the directory");
    fs::write(
        dir.path().join(".claude/agents/implementer.md"),
        "Our own agent cites `process.gone`.\n",
    )
    .expect("write the project's file");
    run_ok(
        dir.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--add-required-packs",
        ],
    );
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
}

/// A second `init --workflow reference` writes no file: every workflow file
/// that exists is kept, no file is reported as the project's, and no byte of
/// the tree changes.
#[test]
fn a_second_init_workflow_keeps_every_file_and_changes_no_byte() {
    let dir = complete_workflow_install();
    let before = snapshot(dir.path());
    let output = run_ok(
        dir.path(),
        &[
            "init",
            "--workflow",
            "reference",
            "--tracker",
            "json",
            "--add-required-packs",
        ],
    );
    assert_same_tree(&before, &snapshot(dir.path()), "the second init");
    let printed = lines(&output);
    for file in manifest_files() {
        assert!(printed.contains(&format!("kept {file}")), "{printed:?}");
    }
    assert!(
        !printed
            .iter()
            .any(|line| line.starts_with("wrote ") || line.starts_with("hand step: ")),
        "{printed:?}"
    );
}

/// A stop between the files and the `workflow` key leaves a tree that the
/// next `init --workflow reference` finishes: the files equal the kit's, so
/// they are adopted and pinned, and none is reported as the project's.
#[test]
fn init_workflow_finishes_an_install_that_stopped_before_the_pins() {
    let dir = complete_workflow_install();
    let pinned = read_json(dir.path(), ".houserules.json")["workflow"].clone();
    edit_marker(dir.path(), |marker| {
        marker.remove("workflow");
    });
    let output = run_ok(
        dir.path(),
        &["init", "--workflow", "reference", "--tracker", "json"],
    );
    assert!(
        !lines(&output)
            .iter()
            .any(|line| line.starts_with("hand step: ")),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["workflow"],
        pinned
    );
}

/// Spec 4.3: `update` on a workflow install writes nothing the second time.
#[test]
fn update_twice_on_a_workflow_install_changes_nothing() {
    let dir = complete_workflow_install();
    run_ok(dir.path(), &["update"]);
    let after_first = snapshot(dir.path());
    let output = run_ok(dir.path(), &["update"]);
    assert_same_tree(&after_first, &snapshot(dir.path()), "the second update");
    assert!(
        !lines(&output).iter().any(|line| line.starts_with("kept ")),
        "{}",
        stdout(&output)
    );
}

/// Spec 4.3 and live case 13: `update` keeps a workflow file that the
/// project changed and prints the kept line on every run; a second run
/// prints the line again and changes nothing.
#[test]
fn a_locally_edited_workflow_file_is_kept_and_named_on_every_update() {
    let dir = complete_workflow_install();
    let file = ".claude/agents/implementer.md";
    let path = path_of(dir.path(), file);
    let mut text = fs::read_to_string(&path).expect("read the template");
    text.push_str("\nProject addition.\n");
    fs::write(&path, &text).expect("edit the template");
    let kept = format!(
        "kept {file} (locally modified; reference workflow {})",
        version()
    );

    let first = run_ok(dir.path(), &["update"]);
    assert!(lines(&first).contains(&kept), "{}", stdout(&first));
    let after_first = snapshot(dir.path());
    let second = run_ok(dir.path(), &["update"]);
    assert!(lines(&second).contains(&kept), "{}", stdout(&second));
    assert_same_tree(&after_first, &snapshot(dir.path()), "the second update");
    assert_eq!(fs::read_to_string(&path).expect("read the file"), text);
}

/// Spec 4.3: `update` writes a workflow file back that the project deleted.
#[test]
fn update_writes_back_a_workflow_file_the_project_deleted() {
    let dir = complete_workflow_install();
    let file = ".claude/skills/orchestrating/SKILL.md";
    let path = path_of(dir.path(), file);
    let original = fs::read(&path).expect("read the skill");
    fs::remove_file(&path).expect("delete the skill");
    let output = run_ok(dir.path(), &["update"]);
    assert!(
        lines(&output).contains(&format!("wrote {file}")),
        "{}",
        stdout(&output)
    );
    assert_eq!(fs::read(&path).expect("read the skill"), original);
}

/// Review focus 3: the pins of the workflow key forward-slash paths, the
/// form of the manifest, whatever the platform.
#[test]
fn workflow_pins_key_forward_slash_paths() {
    let dir = workflow_install();
    let marker = read_json(dir.path(), ".houserules.json");
    let keys: Vec<&String> = marker["workflow"]["files"]
        .as_object()
        .expect("files is an object")
        .keys()
        .collect();
    assert!(!keys.is_empty());
    for key in keys {
        assert!(!key.contains('\\'), "{key} holds a backslash");
        assert!(manifest_files().contains(key), "{key} is no manifest path");
    }
}

/// Review focus 5: `update` adds the model role that `workflow.models` lacks
/// from the manifest, keeps the value the project set, keeps a role the
/// manifest does not know, and prints no line for any of it.
#[test]
fn update_adds_missing_model_roles_and_keeps_unknown() {
    let dir = workflow_install();
    edit_marker(dir.path(), |marker| {
        let mut models = Map::new();
        models.insert("implementer".to_string(), json!("haiku"));
        models.insert("designer".to_string(), json!("custom-model"));
        marker["workflow"]["models"] = Value::Object(models);
    });
    let output = run_ok(dir.path(), &["update"]);
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(
        marker["workflow"]["models"],
        json!({
            "implementer": "haiku",
            "designer": "custom-model",
            "task-reviewer": "opus",
            "branch-reviewer": "opus",
            "spec-reviewer": "opus",
        })
    );
    for line in lines(&output) {
        assert!(
            !line.contains("haiku") && !line.contains("designer") && !line.contains("models"),
            "a line names the models: {line}"
        );
    }
}

/// The project-owned files at the workflow paths of live cases 16.
fn project_own_workflow_files(dir: &Path) {
    for (file, text) in [
        (".githooks/commit-msg", "#!/bin/sh\necho our hook\n"),
        (".claude/agents/implementer.md", "Our own implementer.\n"),
    ] {
        let path = path_of(dir, file);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
        fs::write(path, text).expect("write the project's file");
    }
}

/// Live case 16, first half: a minimal install with the project's own files
/// at workflow paths. `update` twice adopts nothing, writes nothing, and
/// prints no kept line.
#[test]
fn update_on_a_minimal_install_adopts_no_project_own_file_at_a_workflow_path() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    project_own_workflow_files(dir.path());
    let before = snapshot(dir.path());
    for _ in 0..2 {
        let output = run_ok(dir.path(), &["update"]);
        assert!(
            !lines(&output).iter().any(|line| line.starts_with("kept ")),
            "{}",
            stdout(&output)
        );
    }
    let marker = read_json(dir.path(), ".houserules.json");
    assert!(marker.get("workflow").is_none(), "got {marker}");
    let after = snapshot(dir.path());
    for file in [".githooks/commit-msg", ".claude/agents/implementer.md"] {
        assert_eq!(after.get(file), before.get(file), "{file} changed");
    }
}

/// Live case 16, second half, and spec 4.3 (C33): `init --workflow reference`
/// over the project's own files keeps both, prints the line of spec 4.7 step 2
/// once for each, records `null` for each in `workflow.files`, and two
/// updates print nothing for them.
#[test]
fn init_workflow_keeps_a_project_own_file_reports_it_once_and_records_null_for_it() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    project_own_workflow_files(dir.path());
    let before = snapshot(dir.path());
    let output = run_ok(
        dir.path(),
        &["init", "--workflow", "reference", "--tracker", "json"],
    );
    let printed = lines(&output);
    for file in [".githooks/commit-msg", ".claude/agents/implementer.md"] {
        let line = format!(
            "hand step: {file} is not the kit's; the reference workflow leaves it to the project"
        );
        assert_eq!(
            printed.iter().filter(|found| **found == line).count(),
            1,
            "{file} in {printed:?}"
        );
        assert_eq!(
            snapshot(dir.path()).get(file),
            before.get(file),
            "{file} changed"
        );
    }
    let marker = read_json(dir.path(), ".houserules.json");
    let pinned = marker["workflow"]["files"].as_object().expect("files");
    assert_eq!(pinned.get(".githooks/commit-msg"), Some(&Value::Null));
    assert_eq!(
        pinned.get(".claude/agents/implementer.md"),
        Some(&Value::Null)
    );

    for _ in 0..2 {
        let output = run_ok(dir.path(), &["update"]);
        for line in lines(&output) {
            assert!(
                !line.contains(".githooks/commit-msg")
                    && !line.contains(".claude/agents/implementer.md"),
                "an update names a project-own file: {line}"
            );
        }
    }
    let after = snapshot(dir.path());
    for file in [".githooks/commit-msg", ".claude/agents/implementer.md"] {
        assert_eq!(after.get(file), before.get(file), "{file} changed");
    }
}

/// Spec 4.3 (C33): a new binary whose workflow adds a path that the project
/// already holds. The pins of the older binary lack the path, so the first
/// `update` prints the line of spec 4.7 step 2 once and records `null`; the
/// second prints nothing for it; the file stays as found.
#[test]
fn update_reports_a_path_the_pins_lack_once_and_records_null() {
    let dir = complete_workflow_install();
    let file = ".claude/evals/docs-edit.json";
    let path = path_of(dir.path(), file);
    fs::write(&path, "{\"ours\": true}\n").expect("write the project's file");
    edit_marker(dir.path(), |marker| {
        marker["workflow"]["files"]
            .as_object_mut()
            .expect("files is an object")
            .remove(file);
    });
    let line = format!(
        "hand step: {file} is not the kit's; the reference workflow leaves it to the project"
    );

    let first = run_ok(dir.path(), &["update"]);
    assert_eq!(
        lines(&first).iter().filter(|found| **found == line).count(),
        1,
        "{}",
        stdout(&first)
    );
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["workflow"]["files"][file], Value::Null);

    let after_first = snapshot(dir.path());
    let second = run_ok(dir.path(), &["update"]);
    assert!(
        !stdout(&second).contains(file),
        "the second update names the path: {}",
        stdout(&second)
    );
    assert_same_tree(&after_first, &snapshot(dir.path()), "the second update");
    assert_eq!(
        fs::read_to_string(&path).expect("read the file"),
        "{\"ours\": true}\n"
    );
}

/// A path in `ownedPaths` is the project's: `update` leaves it as found, with
/// no line, even though the workflow pins it.
#[test]
fn update_leaves_an_owned_workflow_path_as_found_with_no_line() {
    let dir = complete_workflow_install();
    let file = ".githooks/commit-msg";
    edit_marker(dir.path(), |marker| {
        marker.insert("ownedPaths".to_string(), json!([file]));
    });
    let path = path_of(dir.path(), file);
    fs::write(&path, "#!/bin/sh\necho owned\n").expect("write the owned hook");
    let output = run_ok(dir.path(), &["update"]);
    assert!(
        !stdout(&output).contains(file),
        "a line names the owned path: {}",
        stdout(&output)
    );
    assert_eq!(
        fs::read_to_string(&path).expect("read the hook"),
        "#!/bin/sh\necho owned\n"
    );
}

/// The ids that the packs of the source tree ship, with the pack that ships
/// each.
fn pack_ids() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for entry in fs::read_dir(repo_root().join("template/packs")).expect("read template/packs") {
        let path = entry.expect("a directory entry").path();
        let pack: Value =
            serde_json::from_str(&fs::read_to_string(path).expect("read a pack")).expect("JSON");
        for item in pack["entries"].as_array().expect("entries") {
            ids.insert(item["id"].as_str().expect("an id").to_string());
        }
    }
    ids
}

/// The text of every file that the manifest lists, joined.
fn workflow_texts() -> Vec<(String, String)> {
    manifest_files()
        .into_iter()
        .map(|file| {
            let text = fs::read_to_string(path_of(&package_dir(), &file))
                .unwrap_or_else(|error| panic!("read {file}: {error}"));
            (file, text)
        })
        .collect()
}

/// `true` when `text` cites `id` the way the kit-citation lint and the
/// requirement script read a citation: the id stands alone between characters
/// that cannot belong to an id.
fn cites(text: &str, id: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(offset) = text[from..].find(id) {
        let start = from + offset;
        let end = start + id.len();
        let before_ok = start == 0 || {
            let before = bytes[start - 1];
            !(before.is_ascii_alphanumeric() || before == b'_' || before == b'.' || before == b'-')
        };
        let after_ok = end == bytes.len() || {
            let after = bytes[end];
            !(after.is_ascii_alphanumeric() || after == b'_' || after == b'-')
        };
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

/// Spec 4.1 (5.125): `requires` equals the set of seeded ids that the files
/// of the package cite. `process.conventional-commits` is not required.
#[test]
fn requires_equals_the_ids_the_workflow_files_cite() {
    let ids = pack_ids();
    let texts = workflow_texts();
    let cited: BTreeSet<String> = ids
        .iter()
        .filter(|id| texts.iter().any(|(_, text)| cites(text, id)))
        .cloned()
        .collect();
    let requires: BTreeSet<String> = manifest()["requires"]
        .as_array()
        .expect("requires is an array")
        .iter()
        .map(|id| id.as_str().expect("an id").to_string())
        .collect();
    assert_eq!(requires, cited);
    assert!(!requires.contains("process.conventional-commits"));
    assert_eq!(requires.len(), 29);
}

/// Spec 4.1: no file of the package cites an id that a pack lists under
/// `optIn`: such an id is excluded by default, and a cited excluded id would
/// fail the requirement check and the citation lint.
#[test]
fn no_workflow_file_cites_an_opt_in_id() {
    let mut opt_in = BTreeSet::new();
    for entry in fs::read_dir(repo_root().join("template/packs")).expect("read template/packs") {
        let path = entry.expect("a directory entry").path();
        let pack: Value =
            serde_json::from_str(&fs::read_to_string(path).expect("read a pack")).expect("JSON");
        for id in pack["optIn"].as_array().into_iter().flatten() {
            opt_in.insert(id.as_str().expect("an id").to_string());
        }
    }
    assert!(opt_in.contains(EVERY_COMMIT_ENTRY), "{opt_in:?}");
    for (file, text) in workflow_texts() {
        for id in &opt_in {
            assert!(!cites(&text, id), "{file} cites the opt-in id {id}");
        }
    }
}

/// Spec 4.1 (F14): no shipped template carries a `model:` line; the value of
/// each role lives in `workflow.models`.
#[test]
fn no_template_carries_a_model_line() {
    fn walk(dir: &Path, offenders: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("read a directory") {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                walk(&path, offenders);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                let text = fs::read_to_string(&path).expect("read a template");
                if text.lines().any(|line| line.starts_with("model:")) {
                    offenders.push(path.display().to_string());
                }
            }
        }
    }
    let mut offenders = Vec::new();
    walk(&repo_root().join("template"), &mut offenders);
    assert_eq!(offenders, Vec::<String>::new());
}

/// Spec 4.1: the manifest names the package, its schema, the default model of
/// each role, and the `agentsBlock` text; its `files` list every file of the
/// package tree except the manifest and the project data file.
#[test]
fn the_manifest_describes_the_package_tree() {
    let manifest = manifest();
    assert_eq!(manifest["name"], json!("reference"));
    assert_eq!(manifest["schema"], json!("houserules/workflow@1.0.0"));
    assert_eq!(
        manifest["models"],
        json!({
            "implementer": "sonnet",
            "task-reviewer": "opus",
            "branch-reviewer": "opus",
            "spec-reviewer": "opus",
        })
    );
    assert_eq!(manifest["agentsBlock"], json!(AGENTS_BLOCK));

    fn walk(dir: &Path, prefix: &str, found: &mut BTreeSet<String>) {
        for entry in fs::read_dir(dir).expect("read a directory") {
            let entry = entry.expect("a directory entry");
            let name = entry.file_name().into_string().expect("utf-8 name");
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                walk(&entry.path(), &relative, found);
            } else {
                found.insert(relative);
            }
        }
    }
    let mut tree = BTreeSet::new();
    walk(&package_dir(), "", &mut tree);
    tree.remove("workflow.json");
    tree.remove(RECORD);
    assert_eq!(
        tree,
        manifest_files().into_iter().collect::<BTreeSet<_>>(),
        "the manifest lists the files of the package tree"
    );
}

/// The root block of the `AGENTS.md` of `dir`.
fn agents_block(dir: &Path) -> String {
    let text = fs::read_to_string(dir.join("AGENTS.md")).expect("read AGENTS.md");
    let start = text
        .find("<!-- houserules:begin -->")
        .expect("a begin marker");
    let end = text.find("<!-- houserules:end -->").expect("an end marker");
    text[start..end].to_string()
}

/// Spec 3.10: the block of an install with the JSON tracker names the session
/// commands and the advice of spec 6.3, and no workflow text while the
/// workflow is not installed.
#[test]
fn the_block_names_the_session_commands_and_the_advice_of_the_tracker() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    let block = agents_block(dir.path());
    let section = "## Session\n\n- Start: run `houserules list --open`.\n- End: run `houserules check-backlog`.\n- Write houserules output to a file and read the file; treat a `<<` marker or a \"more items\" note as a lost read.\n";
    assert!(block.contains(section), "no session section in {block}");
    assert!(!block.contains("## Reference workflow"), "{block}");
    assert!(!block.contains("reference workflow"), "{block}");
}

/// Spec 3.10: each adapter names its own session commands; `render` builds no
/// tracker for it.
#[test]
fn the_block_names_the_session_commands_of_each_adapter() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    for (marker_tracker, start, end) in [
        (
            json!({"adapter": "github", "repo": "owner/name"}),
            "gh issue list --label houserules:batch --state open",
            "houserules check-backlog",
        ),
        (json!({"adapter": "beads"}), "bd ready", "bd dolt push"),
    ] {
        edit_marker(dir.path(), |marker| {
            marker.insert("tracker".to_string(), marker_tracker.clone());
        });
        run_ok(dir.path(), &["render"]);
        let block = agents_block(dir.path());
        assert!(
            block.contains(&format!("- Start: run `{start}`.\n- End: run `{end}`.\n")),
            "{block}"
        );
    }
}

/// Spec 3.10: an install with no tracker has no session section.
#[test]
fn a_block_without_a_tracker_has_no_session_section() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    let block = agents_block(dir.path());
    assert!(!block.contains("## Session"), "{block}");
    assert!(!block.contains("lost read"), "{block}");
}

/// Spec 4.1: the block carries `agentsBlock` as the workflow section while the
/// workflow is installed, and `update` leaves it there.
#[test]
fn the_block_carries_the_workflow_pointer_while_the_workflow_is_installed() {
    let dir = workflow_install();
    let section = format!("## Reference workflow\n\n{AGENTS_BLOCK}\n");
    assert!(agents_block(dir.path()).contains(&section));
    run_ok(dir.path(), &["update"]);
    assert!(agents_block(dir.path()).contains(&section));
}

/// Spec 4.4 and 5.141: the `project-knowledge` skill keeps `for`, `get`, and
/// the topic list; the dispatch steps moved to the agent templates, and the
/// description no longer says "dispatched subagent".
#[test]
fn the_project_knowledge_skill_has_no_dispatch_step() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--target", "claude"]);
    let skill = fs::read_to_string(dir.path().join(".claude/skills/project-knowledge/SKILL.md"))
        .expect("read the skill");
    assert!(
        skill.contains("description: Use before reading or changing any file in this repository\n"),
        "{skill}"
    );
    assert!(skill.contains("houserules for"), "{skill}");
    assert!(skill.contains("houserules get"), "{skill}");
    assert!(skill.contains("## Topics"), "{skill}");
    for dispatch_word in [
        "dispatched subagent",
        "Knowledge:",
        "REPORT_FILE",
        "audit --base",
    ] {
        assert!(
            !skill.contains(dispatch_word),
            "the skill keeps the dispatch step {dispatch_word:?}: {skill}"
        );
    }
}
