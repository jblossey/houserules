//! The conversion of a 1.x install to the 2.0 layout through `update`
//! (spec section 5), end to end: the compiled binary runs on trees that the
//! `v1.4.0` binary seeded (`v1_4_0::seed`). The decisions of each step are
//! unit tests in `src/install/conversion/`; the corpus run over this
//! repository is the unit test `this_repository_at_the_base_of_batch_33_task_7_converts_as_the_corpus_run_says`.

mod locked_file;
mod tree_snapshot;
mod v1_4_0;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The five topics of a 1.4.0 install, which are the five kit packs of 2.0.
const TOPICS: [&str; 5] = [
    "knowledge-base",
    "process",
    "quality",
    "security-hygiene",
    "writing-style",
];

/// The CI workflow of an install at the git top level.
const WORKFLOW: &str = ".github/workflows/knowledge.yml";

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// Runs `git <args>` in `dir`, which must succeed.
fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fresh scratch directory with `git init` already run.
fn scratch_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q"]);
    dir
}

/// A scratch repository that a real 1.4.0 `init` seeded. With `github`, the
/// repository has a GitHub `origin` first, so `init` seeds the CI workflow.
fn install_1_4_0(github: bool) -> tempfile::TempDir {
    install_1_4_0_with_prefix(github, None)
}

/// [`install_1_4_0`] whose `init` got `--id-prefix <prefix>` when `prefix` is
/// given.
fn install_1_4_0_with_prefix(github: bool, prefix: Option<&str>) -> tempfile::TempDir {
    let dir = scratch_git_repo();
    if github {
        git(
            dir.path(),
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/example/project.git",
            ],
        );
    }
    v1_4_0::seed(dir.path(), prefix);
    dir
}

/// Runs `houserules <args> --dir <dir>`.
fn run(dir: &Path, args: &[&str]) -> Output {
    houserules()
        .args(args)
        .arg("--dir")
        .arg(dir)
        .output()
        .expect("run houserules")
}

/// Runs `houserules update --dir <dir>` and requires exit 0. Returns the
/// lines of stdout.
fn update(dir: &Path) -> Vec<String> {
    let output = run(dir, &["update"]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "update failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8 stdout")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Requires exit 0 from `houserules <args> --dir <dir>`.
fn passes(dir: &Path, args: &[&str]) {
    let output = run(dir, args);
    assert_eq!(
        output.status.code(),
        Some(0),
        "houserules {args:?} failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// `check-knowledge`, `check-backlog`, and `render --check`: the three gates
/// that read the tree and that a converted tree must pass. `audit`,
/// `check-commit`, and `validate` need a range or a report and do not run.
/// The conversion gives the `backlog/` directory of 1.x the JSON tracker (batch
/// 34, spec 4.7 step 1), so the three gates pass at once.
fn check_knowledge_check_backlog_and_render_check_pass(dir: &Path) {
    passes(dir, &["check-knowledge"]);
    passes(dir, &["check-backlog"]);
    passes(dir, &["render", "--check"]);
}

/// Commits every file of `dir`.
fn commit_all(dir: &Path) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "state"]);
}

/// `git status --porcelain` of `dir`.
fn porcelain(dir: &Path) -> String {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .expect("run git status");
    String::from_utf8(output.stdout).expect("utf8")
}

/// Reads the JSON file `relative` below `dir`.
fn read_json(dir: &Path, relative: &str) -> Value {
    let path = relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part));
    serde_json::from_str(&fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!("read {}: {error}", path.display());
    }))
    .expect("parse JSON")
}

/// Writes `value` as the JSON file `relative` below `dir`.
fn write_json(dir: &Path, relative: &str, value: &Value) {
    let path = relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part));
    fs::create_dir_all(path.parent().expect("a parent")).expect("create directories");
    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(value).expect("serialize")
        ),
    )
    .expect("write JSON");
}

/// The path `relative` below `dir`, built one component at a time.
fn path_of(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// Copies the directory `from` into `to` with every file below it.
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create the destination");
    for entry in fs::read_dir(from).expect("read the source") {
        let entry = entry.expect("read an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy a file");
        }
    }
}

/// The lowercase hex SHA-256 digest of `content`.
fn sha256_hex(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Edits the `entries` of the topic file `relative` below `dir`.
fn edit_entries(dir: &Path, relative: &str, edit: impl FnOnce(&mut Vec<Value>)) {
    let mut file = read_json(dir, relative);
    let entries = file["entries"].as_array_mut().expect("entries");
    edit(entries);
    write_json(dir, relative, &file);
}

/// Removes the entry `id` from the topic file `relative` and returns it.
fn take_entry(dir: &Path, relative: &str, id: &str) -> Value {
    let mut taken = None;
    edit_entries(dir, relative, |entries| {
        let at = entries
            .iter()
            .position(|entry| entry["id"] == json!(id))
            .unwrap_or_else(|| panic!("{relative} holds {id}"));
        taken = Some(entries.remove(at));
    });
    taken.expect("an entry")
}

/// Edits the marker of `dir`.
fn edit_marker(dir: &Path, edit: impl FnOnce(&mut Value)) {
    let mut marker = read_json(dir, ".houserules.json");
    edit(&mut marker);
    write_json(dir, ".houserules.json", &marker);
}

/// D4's line for the marker of `dir`.
fn unconverted_line(dir: &Path) -> String {
    format!(
        "{}: this is an install of houserules 1.x; run houserules update to convert it\n",
        dir.join(".houserules.json").display()
    )
}

/// No `update` line announces a change: the output of an `update` that
/// changes nothing is the render line and the three closing lines.
fn assert_no_change_lines(lines: &[String]) {
    for line in lines {
        assert!(
            !(line.starts_with("wrote ")
                || line.starts_with("removed ")
                || line.starts_with("updated ")
                || line.starts_with("moved ")
                || line.starts_with("kept ")
                || line.starts_with("edited ")
                || line.starts_with("hand step: ")
                || line.ends_with(": written")),
            "update printed a change line: {line}\nall lines: {lines:?}"
        );
    }
}

/// Step 7 removes `.claude/rules/standing-rules.md`, and the render that ends
/// the same `update` writes no file for the standing rules again (spec
/// section 7): they reach Claude Code through the block of `AGENTS.md`. No
/// line of `update` names the file as written.
#[test]
fn update_leaves_no_standing_rules_file_after_it_converts_an_install() {
    let dir = install_1_4_0(false);
    let standing_rules = path_of(dir.path(), ".claude/rules/standing-rules.md");
    assert!(standing_rules.is_file(), "the 1.4.0 install holds the file");

    let lines = update(dir.path());

    assert!(!standing_rules.exists(), "the file is back after update");
    assert!(
        !lines
            .iter()
            .any(|line| line.contains("standing-rules.md") && line.ends_with(": written")),
        "update wrote the file: {lines:?}"
    );
    assert!(
        lines.contains(&"AGENTS.md: written".to_string()),
        "the block of AGENTS.md carries the standing rules now: {lines:?}"
    );
    passes(dir.path(), &["render", "--check"]);
}

/// The one exclusion that the conversion writes for an unmodified install: the
/// offer of the opt-in entry of the process pack (spec 3.7).
fn opt_in_exclusion() -> Value {
    json!({"id": "process.every-commit-names-a-work-item", "reason": "opt-in"})
}

/// An unmodified 1.4.0 install converts with no override and no exclusion of a
/// project entry: the overrides document holds the exclusion of the opt-in
/// entry alone (spec 3.7), and every topic file is a pack.
#[test]
fn an_unmodified_1_4_0_install_converts_with_no_override_and_only_the_opt_in_exclusion() {
    let dir = install_1_4_0(true);
    let lines = update(dir.path());

    assert_eq!(
        read_json(dir.path(), "knowledge/overrides.json"),
        json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [],
            "exclusions": [opt_in_exclusion()],
        })
    );
    for topic in TOPICS {
        assert!(
            !path_of(dir.path(), &format!("knowledge/{topic}.json")).exists(),
            "{topic}.json is a pack now"
        );
        assert!(path_of(dir.path(), &format!("knowledge/packs/{topic}.json")).is_file());
    }
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["schema"], json!("houserules/install@2.2.0"));
    assert_eq!(marker["renderers"], json!(["claude"]));
    assert!(marker.get("overrides").is_none());
    assert!(marker.get("ownedPaths").is_none());
    let mut packs: Vec<&String> = marker["packs"].as_object().expect("packs").keys().collect();
    packs.sort();
    assert_eq!(packs, TOPICS);
    for pin in marker["packs"].as_object().expect("packs").values() {
        assert_eq!(pin["source"], json!("kit"));
        assert_eq!(pin["addedAreas"], json!([]));
    }
    let mut baselines: Vec<&String> = marker["baselines"]
        .as_object()
        .expect("baselines")
        .keys()
        .collect();
    baselines.sort();
    assert_eq!(
        baselines,
        [
            ".claude/schemas/deliverables.json",
            ".claude/skills/houserules-step/SKILL.md",
            ".claude/skills/migrating-knowledge/SKILL.md",
            ":/.github/workflows/knowledge.yml",
            "backlog/schema.json",
            "knowledge/schema.json",
            "tools/claude-session-start.sh",
            "tools/houserules-step.sh",
        ],
        "only path baselines stay, the two schemas of batch 34 join them, and the pins of the workflow replace the baselines of its files"
    );
    assert_eq!(marker["tracker"], json!({"adapter": "json"}));
    assert_eq!(marker["workflow"]["name"], json!("reference"));
    assert!(
        lines.contains(
            &"removed 58 entry baselines from .houserules.json; the pins of the packs replace them"
                .to_string()
        ),
        "{lines:?}"
    );
    assert!(
        lines.contains(
            &"converted .houserules.json to the install schema houserules/install@2.2.0"
                .to_string()
        ),
        "{lines:?}"
    );
    check_knowledge_check_backlog_and_render_check_pass(dir.path());

    commit_all(dir.path());
    let second = update(dir.path());
    assert_no_change_lines(&second);
    assert_eq!(porcelain(dir.path()), "", "a second update changes no file");
}

#[test]
fn a_1_x_workflow_that_the_kit_released_is_replaced_and_baselined() {
    let dir = install_1_4_0(true);
    let released = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("released/knowledge-workflow-1.0.0.yml"),
    )
    .expect("read a released workflow");
    fs::write(path_of(dir.path(), WORKFLOW), &released).expect("write the old workflow");
    let kit = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../template")
            .join(WORKFLOW),
    )
    .expect("read the kit workflow");

    let lines = update(dir.path());

    assert_eq!(fs::read(path_of(dir.path(), WORKFLOW)).unwrap(), kit);
    assert!(lines.contains(&format!("wrote {WORKFLOW}")), "{lines:?}");
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(
        marker["baselines"][format!(":/{WORKFLOW}")],
        json!(sha256_hex(&kit))
    );
}

#[test]
fn a_1_x_workflow_the_project_changed_is_kept_and_listed_as_a_hand_step() {
    let dir = install_1_4_0(true);
    let mut changed = fs::read(path_of(dir.path(), WORKFLOW)).expect("read the workflow");
    changed.extend_from_slice(b"# our own step\n");
    fs::write(path_of(dir.path(), WORKFLOW), &changed).expect("write the workflow");

    let lines = update(dir.path());

    assert_eq!(fs::read(path_of(dir.path(), WORKFLOW)).unwrap(), changed);
    assert!(
        lines.contains(&format!(
            "hand step: {WORKFLOW} differs from every workflow that houserules 1.x released, so houserules update kept it; compare it with the workflow that houserules init writes, and merge the rest by hand"
        )),
        "{lines:?}"
    );
    let marker = read_json(dir.path(), ".houserules.json");
    assert!(
        marker["baselines"].get(format!(":/{WORKFLOW}")).is_none(),
        "a kept workflow gets no baseline"
    );
}

/// Spec section 16: a 1.4.0 install with a modified kit entry, an
/// `overrides`-listed id, a deleted entry, an archived seeded entry with a
/// recorded baseline, an `ownedPaths`-listed topic file that was deleted (a
/// topic no kit-owned file cites) and an archived entry of that topic, a
/// project-only id in a kit topic file, and a schema with its own areas.
/// `check-knowledge`, `check-backlog`, and `render --check` print D4's line
/// before `update`; `update` converts it; the same three gates pass; a second
/// `update` prints no change.
#[test]
fn a_1_4_0_install_in_every_state_converts_and_passes_check_knowledge_check_backlog_and_render_check()
 {
    let dir = install_1_4_0(false);
    let root = dir.path();
    // A modified kit entry.
    edit_entries(root, "knowledge/process.json", |entries| {
        let entry = entries
            .iter_mut()
            .find(|entry| entry["id"] == json!("process.conventional-commits"))
            .expect("process.conventional-commits");
        entry["summary"] = json!("Write the test first, always.");
    });
    // An `overrides`-listed id, modified.
    edit_entries(root, "knowledge/writing-style.json", |entries| {
        let entry = entries
            .iter_mut()
            .find(|entry| entry["id"] == json!("writing-style.principles"))
            .expect("writing-style.principles");
        entry["body"] = json!(["Our own style rule."]);
    });
    // A deleted entry.
    take_entry(
        root,
        "knowledge/security-hygiene.json",
        "security-hygiene.no-focused-tests",
    );
    // An archived seeded entry with a recorded baseline.
    let mut archived = take_entry(
        root,
        "knowledge/process.json",
        "process.skills-for-procedures",
    );
    archived["status"] = json!("retired");
    let process_title = read_json(root, "knowledge/process.json")["title"].clone();
    write_json(
        root,
        "knowledge/archive/process.json",
        &json!({"topic": "process", "title": process_title, "entries": [archived]}),
    );
    // A topic file in `ownedPaths` that was deleted, and an archived entry of
    // that topic.
    let mut quality_archived = take_entry(root, "knowledge/quality.json", "quality.principles");
    quality_archived["status"] = json!("retired");
    let quality_title = read_json(root, "knowledge/quality.json")["title"].clone();
    fs::remove_file(path_of(root, "knowledge/quality.json")).expect("delete quality.json");
    write_json(
        root,
        "knowledge/archive/quality.json",
        &json!({"topic": "quality", "title": quality_title, "entries": [quality_archived.clone()]}),
    );
    // Project-only ids in a kit topic file, and a schema with its own areas.
    // The first stays in the area `tools`, which only the deleted topic used
    // among the kit packs, so the generated file of that area stays current.
    edit_entries(root, "knowledge/writing-style.json", |entries| {
        let mut own = entries[0].clone();
        own["id"] = json!("writing-style.our-own");
        own["area"] = json!("tools");
        own["standing"] = json!(false);
        entries.push(own.clone());
        own["id"] = json!("writing-style.our-backend");
        own["area"] = json!("backend");
        entries.push(own);
    });
    let mut schema = read_json(root, "knowledge/schema.json");
    schema["$defs"]["area"]["enum"]
        .as_array_mut()
        .expect("the area enum")
        .push(json!("backend"));
    write_json(root, "knowledge/schema.json", &schema);
    let mut areas = read_json(root, "knowledge/areas.json");
    areas["backend"] = json!({"paths": ["backend/**"]});
    write_json(root, "knowledge/areas.json", &areas);
    fs::create_dir_all(path_of(root, "backend")).expect("create backend/");
    fs::write(path_of(root, "backend/main.rs"), "fn main() {}\n").expect("write backend/main.rs");
    edit_marker(root, |marker| {
        marker["overrides"] = json!(["writing-style.principles", "knowledge/quality.json"]);
    });
    commit_all(root);

    for gate in [
        ["check-knowledge"].as_slice(),
        ["check-backlog"].as_slice(),
        ["render", "--check"].as_slice(),
    ] {
        let output = run(root, gate);
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            unconverted_line(root),
            "{gate:?}"
        );
        assert_eq!(output.status.code(), Some(2), "{gate:?}");
    }

    let lines = update(root);

    assert!(
        lines.contains(&"wrote knowledge/overrides.json (3 overrides, 1 exclusion)".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"knowledge/quality.json is in ownedPaths, so the conversion leaves it as it is and does not install the kit pack quality; run houserules pack add quality to install it".to_string()),
        "{lines:?}"
    );
    let overrides = read_json(root, "knowledge/overrides.json");
    assert_eq!(
        overrides,
        json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [
                {"id": "process.conventional-commits", "fields": {"summary": "Write the test first, always."}},
                {"id": "process.skills-for-procedures", "fields": {"status": "retired"}},
                {"id": "writing-style.principles", "fields": {"body": ["Our own style rule."]}},
            ],
            "exclusions": [
                {"id": "security-hygiene.no-focused-tests"},
                opt_in_exclusion(),
            ],
        })
    );
    let marker = read_json(root, ".houserules.json");
    assert_eq!(marker["ownedPaths"], json!(["knowledge/quality.json"]));
    assert!(marker.get("overrides").is_none());
    let mut packs: Vec<&String> = marker["packs"].as_object().expect("packs").keys().collect();
    packs.sort();
    assert_eq!(
        packs,
        [
            "knowledge-base",
            "process",
            "security-hygiene",
            "writing-style"
        ],
        "no pack for the owned topic"
    );
    assert!(!path_of(root, "knowledge/quality.json").exists());
    assert!(!path_of(root, "knowledge/packs/quality.json").exists());
    assert_eq!(
        read_json(root, "knowledge/archive/quality.json")["entries"],
        json!([quality_archived]),
        "the archived entry of the owned topic stays archived"
    );
    assert!(!path_of(root, "knowledge/archive/process.json").exists());
    let writing_style = read_json(root, "knowledge/writing-style.json");
    let ids: Vec<&str> = writing_style["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(ids, ["writing-style.our-own", "writing-style.our-backend"]);
    assert!(
        read_json(root, "knowledge/schema.json")["$defs"]
            .get("area")
            .is_none(),
        "the schema lost the area enum"
    );
    check_knowledge_check_backlog_and_render_check_pass(root);

    commit_all(root);
    let second = update(root);
    assert_no_change_lines(&second);
    assert_eq!(porcelain(root), "", "a second update changes no file");
}

/// A converted override of a modified 1.x entry carries only the fields that
/// the project changed from the text that its 1.x release shipped. A 1.4.0
/// install whose `process.tdd` summary alone is edited gets an override of
/// `summary` alone, so the fields that the kit changed after 1.4.0 (the body
/// of the entry) reach it.
#[test]
fn a_modified_summary_converts_to_an_override_of_the_summary_alone() {
    let dir = install_1_4_0(false);
    edit_entries(dir.path(), "knowledge/process.json", |entries| {
        let entry = entries
            .iter_mut()
            .find(|entry| entry["id"] == json!("process.tdd"))
            .expect("process.tdd");
        entry["summary"] = json!("Write the test first, always.");
    });
    commit_all(dir.path());

    update(dir.path());

    assert_eq!(
        read_json(dir.path(), "knowledge/overrides.json"),
        json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [
                {"id": "process.tdd", "fields": {"summary": "Write the test first, always."}}
            ],
            "exclusions": [opt_in_exclusion()],
        })
    );
}

#[test]
fn a_1_x_topic_named_overrides_stops_the_conversion_before_any_write() {
    let dir = install_1_4_0(false);
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({"topic": "overrides", "title": "Overrides", "entries": []}),
    );
    commit_all(dir.path());

    let output = run(dir.path(), &["update"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "hand step: knowledge/overrides.json is a topic file of houserules 1.x, and houserules 2.0 reserves the name overrides for its own file; move its entries to a topic file with another name, then run houserules update again\n"
    );
    assert_eq!(porcelain(dir.path()), "", "the stop changes no file");
}

#[test]
fn a_claude_md_without_the_import_gets_the_hand_step_and_fails_check_knowledge_until_it_has_it() {
    let dir = install_1_4_0(false);
    fs::write(path_of(dir.path(), "CLAUDE.md"), "Our own notes.\n").expect("write CLAUDE.md");

    let lines = update(dir.path());

    assert!(
        lines.contains(&"hand step: add @AGENTS.md to CLAUDE.md: without the import, the standing rules no longer reach Claude Code".to_string()),
        "{lines:?}"
    );
    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(
            "CLAUDE.md: the standing rules do not reach Claude Code; add @AGENTS.md to CLAUDE.md"
        ),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    fs::write(
        path_of(dir.path(), "CLAUDE.md"),
        "Our own notes.\n\n@AGENTS.md\n",
    )
    .expect("add the import");
    // The converted tree has the process pack and no tracker, which
    // `check-knowledge` names (spec 3.10); the project adopts the JSON tracker.
    passes(dir.path(), &["init", "--tracker", "json"]);

    passes(dir.path(), &["check-knowledge"]);
}

/// A converted install, then `pack remove process`, and the entries of the
/// pack copied into the local pack: no step re-adds the pack or turns the
/// local entries back into pack entries.
#[test]
fn update_after_pack_remove_and_a_local_copy_of_the_entries_changes_nothing() {
    let dir = install_1_4_0(false);
    update(dir.path());
    let entries = read_json(dir.path(), "knowledge/packs/process.json")["entries"].clone();
    passes(dir.path(), &["pack", "remove", "process"]);
    write_json(
        dir.path(),
        "knowledge/process.json",
        &json!({"topic": "process", "title": "Process", "entries": entries}),
    );
    passes(dir.path(), &["render"]);
    commit_all(dir.path());
    let before = tree_snapshot::snapshot(dir.path());

    let lines = update(dir.path());

    assert_no_change_lines(&lines);
    tree_snapshot::assert_same_tree(
        &before,
        &tree_snapshot::snapshot(dir.path()),
        "update after pack remove",
    );
}

#[test]
fn update_converts_a_hand_written_local_entry_with_a_pack_id_into_an_override() {
    let dir = scratch_git_repo();
    passes(dir.path(), &["init", "--tracker", "json"]);
    passes(dir.path(), &["pack", "add", "process"]);
    let mut entry = read_json(dir.path(), "knowledge/packs/process.json")["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == json!("process.tdd"))
        .expect("process.tdd")
        .clone();
    entry["summary"] = json!("Write the test first, always.");
    write_json(
        dir.path(),
        "knowledge/process.json",
        &json!({"topic": "process", "title": "Process", "entries": [entry]}),
    );
    let before = run(dir.path(), &["check-knowledge"]);
    assert_eq!(before.status.code(), Some(1));
    let before_text = String::from_utf8_lossy(&before.stderr).into_owned();
    assert!(
        before_text.contains("process.tdd")
            && before_text.contains("knowledge/process.json")
            && before_text.contains("knowledge/packs/process.json"),
        "{before_text}"
    );

    let lines = update(dir.path());

    assert!(
        lines.contains(&"moved process.tdd from knowledge/process.json to knowledge/overrides.json: pack process ships it, and the override sets summary".to_string()),
        "{lines:?}"
    );
    assert!(!path_of(dir.path(), "knowledge/process.json").exists());
    assert_eq!(
        read_json(dir.path(), "knowledge/overrides.json")["overrides"],
        json!([{"id": "process.tdd", "fields": {"summary": "Write the test first, always."}}])
    );
    passes(dir.path(), &["check-knowledge"]);
}

#[test]
fn a_tree_with_knowledge_and_no_marker_needs_the_id_prefix_flag() {
    let dir = install_1_4_0(false);
    fs::remove_file(path_of(dir.path(), ".houserules.json")).expect("delete the marker");
    commit_all(dir.path());
    let before = tree_snapshot::snapshot(dir.path());

    let refused = run(dir.path(), &["update"]);

    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(refused.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr),
        format!(
            "{}: not found, but {} exists; run houserules update --id-prefix <prefix> with the id prefix of your backlog (WI for WI-001), so that update stamps the right one\n",
            dir.path().join(".houserules.json").display(),
            dir.path().join("knowledge").display()
        )
    );
    tree_snapshot::assert_same_tree(&before, &tree_snapshot::snapshot(dir.path()), "the refusal");

    let output = run(dir.path(), &["update", "--id-prefix", "HR"]);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["idPrefix"], json!("HR"));
    assert_eq!(marker["schema"], json!("houserules/install@2.2.0"));
}

/// This repository's own marker and knowledge files at the base of batch 33
/// Task 7 (`tests/fixtures/conversion/repo/`): 34 overridden ids, four owned
/// paths, a schema that differs from the released one in its area enum, and
/// 53 project-only ids. `update` converts them with no kept line and no hand
/// step, and a second `update` changes nothing.
#[test]
fn this_repositorys_marker_and_knowledge_at_the_base_of_batch_33_task_7_convert_without_a_hand_step()
 {
    let dir = scratch_git_repo();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("conversion")
        .join("repo");
    fs::copy(
        fixture.join(".houserules.json"),
        dir.path().join(".houserules.json"),
    )
    .expect("copy the marker");
    copy_dir(&fixture.join("knowledge"), &dir.path().join("knowledge"));

    let lines = update(dir.path());

    assert!(
        lines.contains(&"wrote knowledge/overrides.json (35 overrides, 0 exclusions)".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(
            &"moved 4 paths from overrides to ownedPaths in .houserules.json".to_string()
        ),
        "{lines:?}"
    );
    assert!(
        lines.contains(
            &"removed 49 entry baselines from .houserules.json; the pins of the packs replace them"
                .to_string()
        ),
        "{lines:?}"
    );
    let kept: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("kept ") || line.starts_with("hand step: "))
        .collect();
    assert!(kept.is_empty(), "{kept:?}");
    let schema = read_json(dir.path(), "knowledge/schema.json");
    assert!(
        schema["$defs"].get("area").is_none(),
        "the schema lost the area enum"
    );

    assert_no_change_lines(&update(dir.path()));
}

/// D4 tells a 1.x install without `knowledge/schema.json` to run `update`
/// (spec C21), so `update` converts it: the file comes back, and the tree
/// equals the tree of the conversion of an install that kept its schema.
#[test]
fn a_1_4_0_install_without_its_knowledge_schema_converts_and_gets_the_file_back() {
    let control = install_1_4_0(false);
    update(control.path());
    let dir = install_1_4_0(false);
    fs::remove_file(path_of(dir.path(), "knowledge/schema.json")).expect("delete the schema");

    let lines = update(dir.path());

    assert!(
        lines.contains(&"wrote knowledge/schema.json".to_string()),
        "{lines:?}"
    );
    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        "the conversion of an install without its schema",
    );
    check_knowledge_check_backlog_and_render_check_pass(dir.path());
}

/// A 1.x install without `knowledge/schema.json` whose marker lists the file
/// as the project's own: `update` cannot put it back (spec C21). It stops
/// before any write with one line that names the file. The 1.x marker spells
/// the list `overrides`, an interrupted conversion spells it `ownedPaths`.
#[test]
fn update_refuses_an_absent_knowledge_schema_that_the_marker_lists_as_owned_before_any_write() {
    for key in ["overrides", "ownedPaths"] {
        let dir = install_1_4_0(false);
        fs::remove_file(path_of(dir.path(), "knowledge/schema.json")).expect("delete the schema");
        edit_marker(dir.path(), |marker| {
            marker[key] = json!(["knowledge/schema.json"]);
        });
        let before = tree_snapshot::snapshot(dir.path());

        let output = run(dir.path(), &["update"]);

        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "knowledge/schema.json: this file does not exist, and .houserules.json lists it in ownedPaths (the key is overrides in a 1.x marker), so update cannot restore it; restore it, or remove it from ownedPaths, then run houserules update again\n",
            "{key}"
        );
        assert_eq!(output.status.code(), Some(2), "{key}: {output:?}");
        assert!(output.stdout.is_empty(), "{key}: {output:?}");
        tree_snapshot::assert_same_tree(
            &before,
            &tree_snapshot::snapshot(dir.path()),
            &format!("update with the schema listed in {key}"),
        );
    }
}

/// The line of `update` for a 1.x install whose `knowledge/` directory does not
/// exist or is empty (spec C21). `prefix` is the `idPrefix` of the marker.
fn absent_knowledge_line(prefix: &str) -> String {
    format!(
        "knowledge/: this directory does not exist or is empty, so update cannot tell which entries the project deleted and cannot convert this install of houserules 1.x; restore it from version control and run houserules update again, or remove .houserules.json and run houserules init --id-prefix {prefix} for a fresh install\n"
    )
}

/// The line of `update` for a path that `ownedPaths` lists, that does not
/// exist, and that the conversion must write (spec C21).
fn owned_path_is_absent_line(path: &str) -> String {
    format!(
        "{path}: this file does not exist, and .houserules.json lists it in ownedPaths (the key is overrides in a 1.x marker), so update cannot restore it; restore it, or remove it from ownedPaths, then run houserules update again\n"
    )
}

/// Deletes the `knowledge/` directory of `dir`.
fn remove_knowledge(dir: &Path) {
    fs::remove_dir_all(path_of(dir, "knowledge")).expect("delete knowledge/");
}

/// Deletes `knowledge/areas.json`, the one file of a 1.x `knowledge/` that the
/// other files do not need.
fn remove_areas_file(dir: &Path) {
    fs::remove_file(path_of(dir, "knowledge/areas.json")).expect("delete areas.json");
}

/// Requires the refusal of `houserules <args> --dir <dir>` for a `knowledge/`
/// directory that does not exist or is empty (spec C21): the line with the
/// marker's `prefix`, exit 2, no output on stdout, and a tree that is as it was.
/// `context` names the case. Returns the line that the run printed.
fn assert_update_refuses_a_knowledge_directory_without_entries(
    dir: &Path,
    args: &[&str],
    prefix: &str,
    context: &str,
) -> String {
    let before = tree_snapshot::snapshot(dir);

    let output = run(dir, args);

    let line = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(line, absent_knowledge_line(prefix), "{context}");
    assert_eq!(output.status.code(), Some(2), "{context}: {output:?}");
    assert!(output.stdout.is_empty(), "{context}: {output:?}");
    tree_snapshot::assert_same_tree(
        &before,
        &tree_snapshot::snapshot(dir),
        &format!("{context}: the tree changed"),
    );
    line
}

/// One state of a refusal test: how the install was made, the edit of its
/// marker, the command, and the prefix that the line names.
struct RefusalCase {
    /// Names the case in a failed assertion.
    context: &'static str,
    /// The `--id-prefix` of the 1.4.0 `init`, when the case has one.
    install_prefix: Option<&'static str>,
    /// Edits the marker before the run, when the case does.
    edit: Option<fn(&mut Value)>,
    /// The arguments of the run.
    args: &'static [&'static str],
    /// The `idPrefix` of the marker, which the line names.
    prefix: &'static str,
}

/// Requires the refusal of every case after `knowledge/` lost its entries. With
/// `empty`, the directory stays as an empty directory; otherwise it is gone.
fn assert_refusals(empty: bool, cases: Vec<RefusalCase>) {
    for case in cases {
        let dir = install_1_4_0_with_prefix(false, case.install_prefix);
        if let Some(edit) = case.edit {
            edit_marker(dir.path(), edit);
        }
        remove_knowledge(dir.path());
        if empty {
            fs::create_dir(path_of(dir.path(), "knowledge")).expect("create knowledge/");
        }
        assert_update_refuses_a_knowledge_directory_without_entries(
            dir.path(),
            case.args,
            case.prefix,
            case.context,
        );
    }
}

/// A 1.x install whose `knowledge/` directory does not exist is not converted
/// (spec C21): under step 2 every entry of the kit is deleted, a statement that
/// an accidental deletion should not make. `update` stops before any write with
/// one line that names `knowledge/` and both ways out, and exits 2. The way out
/// through `init` names `--id-prefix` with the `idPrefix` of the marker, which
/// wins over a flag. The refusal holds for a default marker, for a marker with a
/// prefix of its own, for a marker that lists paths the project owns under
/// `overrides`, for a marker that an interrupted conversion left with
/// `ownedPaths`, and with `--id-prefix`.
#[test]
fn update_refuses_a_1_x_install_without_its_knowledge_directory_before_any_write() {
    assert_refusals(
        false,
        vec![
            RefusalCase {
                context: "a default marker",
                install_prefix: None,
                edit: None,
                args: &["update"],
                prefix: "WI",
            },
            RefusalCase {
                context: "a marker with its own prefix",
                install_prefix: Some("XY"),
                edit: None,
                args: &["update"],
                prefix: "XY",
            },
            RefusalCase {
                context: "overrides",
                install_prefix: None,
                edit: Some(|marker| {
                    marker["overrides"] = json!(["process.tdd", "knowledge/schema.json"]);
                }),
                args: &["update"],
                prefix: "WI",
            },
            RefusalCase {
                context: "ownedPaths",
                install_prefix: None,
                edit: Some(|marker| marker["ownedPaths"] = json!(["knowledge/schema.json"])),
                args: &["update"],
                prefix: "WI",
            },
            RefusalCase {
                context: "an id prefix flag, which the marker's prefix outranks",
                install_prefix: None,
                edit: None,
                args: &["update", "--id-prefix", "ZZ"],
                prefix: "WI",
            },
        ],
    );
}

/// An empty `knowledge/` directory holds no entry to read, so it is refused the
/// same way as an absent one (spec C21): the same line, exit 2, and no write.
#[test]
fn update_refuses_a_1_x_install_with_an_empty_knowledge_directory_before_any_write() {
    assert_refusals(
        true,
        vec![
            RefusalCase {
                context: "an empty directory",
                install_prefix: None,
                edit: None,
                args: &["update"],
                prefix: "WI",
            },
            RefusalCase {
                context: "an empty directory and a marker with its own prefix",
                install_prefix: Some("XY"),
                edit: None,
                args: &["update"],
                prefix: "XY",
            },
        ],
    );
}

/// The refusal is for a `knowledge/` that holds no entry. A file named
/// `knowledge` is no directory: `update` stops before any write with the line
/// of the operating system for the directory it cannot read, which names the
/// path, as it did before the refusal existed.
#[test]
fn update_names_a_file_in_place_of_the_knowledge_directory_with_the_error_of_the_system() {
    let dir = install_1_4_0(false);
    remove_knowledge(dir.path());
    let path = path_of(dir.path(), "knowledge");
    fs::write(&path, "").expect("write a file knowledge");
    let before = tree_snapshot::snapshot(dir.path());

    let output = run(dir.path(), &["update"]);

    let error = fs::read_dir(&path).expect_err("a file is no directory");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{}: {error}\n", path.display())
    );
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    tree_snapshot::assert_same_tree(
        &before,
        &tree_snapshot::snapshot(dir.path()),
        "the tree changed",
    );
}

/// The first way out of the refusal: the project restores `knowledge/` from
/// version control, and `update` then converts the install as it converts
/// one that never lost the directory.
#[test]
fn update_converts_the_install_after_knowledge_comes_back_from_version_control() {
    let control = install_1_4_0(false);
    update(control.path());
    let dir = install_1_4_0(false);
    commit_all(dir.path());
    remove_knowledge(dir.path());
    assert_update_refuses_a_knowledge_directory_without_entries(
        dir.path(),
        &["update"],
        "WI",
        "update",
    );

    // Windows runners set core.autocrlf; the checkout must give back the stored bytes.
    git(
        dir.path(),
        &["-c", "core.autocrlf=false", "checkout", "--", "knowledge"],
    );
    update(dir.path());

    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        "the conversion of an install whose knowledge/ came back",
    );
    check_knowledge_check_backlog_and_render_check_pass(dir.path());
}

/// The arguments of the fresh-install command that `line` prints: the words
/// from `init` up to `for a fresh install`.
fn fresh_install_arguments(line: &str) -> Vec<String> {
    let start = line.find("houserules init").expect("the line names init") + "houserules ".len();
    let end = line
        .find(" for a fresh install")
        .expect("the line ends the remedy");
    line[start..end]
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

/// The second way out of the refusal: the project removes `.houserules.json`
/// and runs the command that the line prints, exactly as printed. `init` then
/// writes a fresh install of the 2.0 layout, and its marker keeps the prefix of
/// the marker that was removed. It replaces the migrating-knowledge skill of
/// 1.4.0, which equals a content that the kit shipped (spec C35), so the skill
/// cites no rule that the minimal install lacks. The printed command alone
/// leaves a tree where `check-knowledge` and `render --check` pass.
/// `check-backlog` prints line B (spec 3.4) and exits 1, because the
/// `backlog/` directory of 1.x has no tracker in that install; the remedy that
/// line names is `init --tracker json`.
#[test]
fn init_as_the_line_prints_it_gives_a_fresh_install_with_the_old_prefix() {
    for (install_prefix, prefix) in [(None, "WI"), (Some("XY"), "XY")] {
        let dir = install_1_4_0_with_prefix(false, install_prefix);
        remove_knowledge(dir.path());
        let line = assert_update_refuses_a_knowledge_directory_without_entries(
            dir.path(),
            &["update"],
            prefix,
            "update",
        );
        let arguments = fresh_install_arguments(&line);
        assert_eq!(arguments, ["init", "--id-prefix", prefix]);

        fs::remove_file(path_of(dir.path(), ".houserules.json")).expect("delete the marker");
        let words: Vec<&str> = arguments.iter().map(String::as_str).collect();
        passes(dir.path(), &words);

        let marker = read_json(dir.path(), ".houserules.json");
        assert_eq!(marker["schema"], json!("houserules/install@2.2.0"));
        assert_eq!(marker["idPrefix"], json!(prefix));
        assert!(marker.get("overrides").is_none());
        let skill = ".claude/skills/migrating-knowledge/SKILL.md";
        assert_eq!(
            fs::read(path_of(dir.path(), skill)).expect("read the skill"),
            fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../template")
                    .join(skill)
            )
            .expect("read the kit skill"),
            "init replaced the skill of 1.4.0"
        );

        passes(dir.path(), &["check-knowledge"]);
        passes(dir.path(), &["render", "--check"]);
        let backlog = run(dir.path(), &["check-backlog"]);
        assert_eq!(backlog.status.code(), Some(1));
        assert_eq!(
            String::from_utf8_lossy(&backlog.stderr),
            "backlog/ exists but no tracker is installed; run houserules init --tracker json\n"
        );
    }
}

/// Spec step 2 and HR-151(3): a topic file that is absent while `knowledge/`
/// exists was deleted, so each entry of its kit pack becomes an exclusion.
#[test]
fn a_deleted_topic_file_converts_to_an_exclusion_for_each_of_its_entries() {
    let dir = install_1_4_0(false);
    let mut ids: Vec<String> = read_json(dir.path(), "knowledge/writing-style.json")["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id").to_string())
        .collect();
    ids.sort();
    fs::remove_file(path_of(dir.path(), "knowledge/writing-style.json"))
        .expect("delete writing-style.json");

    let lines = update(dir.path());

    let mut exclusions: Vec<Value> = ids.iter().map(|id| json!({"id": id})).collect();
    assert!(
        lines.contains(&format!(
            "wrote knowledge/overrides.json (0 overrides, {} exclusions)",
            ids.len()
        )),
        "{lines:?}"
    );
    exclusions.push(opt_in_exclusion());
    assert_eq!(
        read_json(dir.path(), "knowledge/overrides.json")["exclusions"],
        json!(exclusions)
    );
}

/// The `addedAreas` of each pin of the marker in `dir`, by pack name.
fn added_areas_by_pack(dir: &Path) -> Value {
    let marker = read_json(dir, ".houserules.json");
    let pins = marker["packs"].as_object().expect("the pins");
    Value::Object(
        pins.iter()
            .map(|(name, pin)| (name.clone(), pin["addedAreas"].clone()))
            .collect(),
    )
}

/// A 1.x install whose `knowledge/` holds files but no `areas.json` converts
/// (spec C21): step 5 writes the file, and each kit pack adds the areas it
/// declares and records them in its pin, as a fresh `init` records them. The
/// control is the same install whose `areas.json` holds an empty object. After
/// the conversion the two trees are equal, the marker included, and a second
/// `update` changes nothing.
#[test]
fn a_1_4_0_install_without_its_areas_file_converts_with_the_areas_of_the_packs() {
    let control = install_1_4_0(false);
    write_json(control.path(), "knowledge/areas.json", &json!({}));
    update(control.path());
    let dir = install_1_4_0(false);
    remove_areas_file(dir.path());

    let lines = update(dir.path());

    assert!(
        lines.contains(&"wrote knowledge/areas.json".to_string()),
        "{lines:?}"
    );
    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        "the conversion of an install without areas.json",
    );
    assert_eq!(
        read_json(dir.path(), "knowledge/areas.json"),
        read_json(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template"),
            "knowledge/areas.json"
        ),
        "areas.json"
    );
    assert_eq!(
        added_areas_by_pack(dir.path()),
        json!({
            "knowledge-base": ["docs"],
            "process": ["global", "process"],
            "quality": ["tools"],
            "security-hygiene": [],
            "writing-style": [],
        }),
        "addedAreas"
    );
    check_knowledge_check_backlog_and_render_check_pass(dir.path());
    commit_all(dir.path());
    assert_no_change_lines(&update(dir.path()));
}

/// A 1.x install without `knowledge/areas.json` whose marker lists the file as
/// the project's own: `update` cannot write it (spec C21). It stops before any
/// write with the line of the owned absent path, which names the file. The 1.x
/// marker spells the list `overrides`, an interrupted conversion spells it
/// `ownedPaths`.
#[test]
fn update_refuses_an_absent_areas_file_that_the_marker_lists_as_owned_before_any_write() {
    for key in ["overrides", "ownedPaths"] {
        let dir = install_1_4_0(false);
        remove_areas_file(dir.path());
        edit_marker(dir.path(), |marker| {
            marker[key] = json!(["knowledge/areas.json"]);
        });
        let before = tree_snapshot::snapshot(dir.path());

        let output = run(dir.path(), &["update"]);

        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            owned_path_is_absent_line("knowledge/areas.json"),
            "{key}"
        );
        assert_eq!(output.status.code(), Some(2), "{key}: {output:?}");
        assert!(output.stdout.is_empty(), "{key}: {output:?}");
        tree_snapshot::assert_same_tree(
            &before,
            &tree_snapshot::snapshot(dir.path()),
            &format!("update with areas.json listed in {key}"),
        );
    }
}

/// The install has no `areas.json`. Step 5 writes it before the first pin, so
/// the first write to the marker, the pin of the first kit pack, stops the run
/// with the file in place and no area in it.
#[test]
fn an_update_that_stops_at_the_pin_of_the_first_kit_pack_of_an_install_without_areas_finishes_at_the_next_update()
 {
    assert_finishes_after_a_stop_at(".houserules.json", remove_areas_file, |_| {});
}

/// The install has no `areas.json`. The write of the copy of the first kit pack
/// stops the run after the file, the pin, and the area of the pack are written.
#[test]
fn an_update_that_stops_at_the_copy_of_the_first_kit_pack_of_an_install_without_areas_finishes_at_the_next_update()
 {
    assert_finishes_after_a_stop_at(
        "knowledge/packs/knowledge-base.json",
        |dir| {
            remove_areas_file(dir);
            write_compact_copy(dir, "knowledge-base");
        },
        |dir| delete_copy(dir, "knowledge-base"),
    );
}

/// A run that stops after the packs are written finishes at the next run: the
/// tree equals the tree of an uninterrupted conversion. The stop is a
/// directory where `.claude/rules/standing-rules.md` lies, which step 7 cannot
/// remove.
#[test]
fn an_update_that_stops_after_step_5_finishes_at_the_next_update() {
    let control = install_1_4_0(false);
    update(control.path());

    let dir = install_1_4_0(false);
    let rules = path_of(dir.path(), ".claude/rules/standing-rules.md");
    let kept = fs::read(&rules).expect("read the standing rules");
    fs::remove_file(&rules).expect("remove the file");
    fs::create_dir(&rules).expect("create a directory in its place");

    let failed = run(dir.path(), &["update"]);

    assert_eq!(failed.status.code(), Some(2), "{failed:?}");
    let marker = read_json(dir.path(), ".houserules.json");
    assert!(
        marker.get("schema").is_none(),
        "the schema key is written last"
    );
    assert_eq!(marker["packs"].as_object().expect("packs").len(), 5);
    assert!(path_of(dir.path(), "knowledge/packs/process.json").is_file());
    assert!(!path_of(dir.path(), "knowledge/process.json").exists());

    fs::remove_dir(&rules).expect("remove the directory");
    fs::write(&rules, kept).expect("restore the file");
    update(dir.path());

    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        "the finished conversion",
    );
}

/// Writes a `knowledge/overrides.json` that holds no override and no exclusion.
/// The offer of the opt-in entry then rewrites a file that exists, so it needs
/// no new file in `knowledge/`.
#[cfg(unix)]
fn seed_an_empty_overrides_document(dir: &Path) {
    write_json(
        dir,
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [],
            "exclusions": []
        }),
    );
}

/// The same stop between the pins and the move of the entries, where the
/// topic files still hold the entries of the packs and the baselines still
/// stand. The next run must not read the entries as deleted ones, and the
/// tree equals the tree of an uninterrupted conversion. The stop is a
/// `knowledge/` directory that cannot lose a file. The install holds its
/// overrides document already (`seed_an_empty_overrides_document`), so that the
/// first write that the directory refuses is the removal of a topic file. A
/// user with the right to write anywhere (`root`) cannot stop the run this
/// way, so the test ends.
#[cfg(unix)]
#[test]
fn an_update_that_stops_before_the_entries_move_finishes_at_the_next_update() {
    use std::os::unix::fs::PermissionsExt;

    let control = install_1_4_0(false);
    seed_an_empty_overrides_document(control.path());
    update(control.path());

    let dir = install_1_4_0(false);
    seed_an_empty_overrides_document(dir.path());
    let knowledge = path_of(dir.path(), "knowledge");
    fs::create_dir(knowledge.join("packs")).expect("create packs/");
    fs::set_permissions(&knowledge, fs::Permissions::from_mode(0o555)).expect("lock knowledge/");

    let failed = run(dir.path(), &["update"]);

    fs::set_permissions(&knowledge, fs::Permissions::from_mode(0o755)).expect("unlock knowledge/");
    if failed.status.success() {
        return;
    }
    assert_eq!(failed.status.code(), Some(2), "{failed:?}");
    let marker = read_json(dir.path(), ".houserules.json");
    assert!(marker.get("schema").is_none());
    assert_eq!(marker["packs"].as_object().expect("packs").len(), 5);
    assert!(
        path_of(dir.path(), "knowledge/process.json").is_file(),
        "the entries are still in their topic file"
    );

    update(dir.path());

    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        "the finished conversion",
    );
}

/// The kit pack `name` as the binary carries it.
fn kit_pack(name: &str) -> Value {
    serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../template/packs")
                .join(format!("{name}.json")),
        )
        .expect("read the kit pack"),
    )
    .expect("parse the kit pack")
}

/// Writes the kit pack `name` at its copy path in the compact form of
/// `serde_json`: the pack of the binary, in bytes that `update` would not
/// write.
fn write_compact_copy(dir: &Path, name: &str) {
    let path = path_of(dir, &format!("knowledge/packs/{name}.json"));
    fs::create_dir_all(path.parent().expect("a parent")).expect("create knowledge/packs/");
    fs::write(
        path,
        serde_json::to_string(&kit_pack(name)).expect("serialize"),
    )
    .expect("write the copy");
}

/// Removes the area `area` from `knowledge/areas.json`, so that the kit pack
/// which declares it adds it again.
fn drop_area(dir: &Path, area: &str) {
    let mut areas = read_json(dir, "knowledge/areas.json");
    areas
        .as_object_mut()
        .expect("areas")
        .remove(area)
        .unwrap_or_else(|| panic!("areas.json holds {area}"));
    write_json(dir, "knowledge/areas.json", &areas);
}

/// Interrupts `update` at the write to the file `locked` (below the install
/// root), and finishes the conversion with one more `update`. `prepare`
/// changes the fresh 1.4.0 install of the case and of its control, and `after`
/// changes the stopped tree before the run that finishes it. A run that no
/// fault stopped leaves the tree that the finished run must reach (spec C20):
/// the pins, the `addedAreas`, and every other file included. The case ends
/// at once for a user with the right to write anywhere (`root`), whom a
/// read-only file does not stop.
fn assert_finishes_after_a_stop_at(
    locked: &str,
    prepare: impl Fn(&Path),
    after: impl FnOnce(&Path),
) {
    let control = install_1_4_0(false);
    prepare(control.path());
    update(control.path());

    let dir = install_1_4_0(false);
    prepare(dir.path());
    let Some(lock) = locked_file::lock(&path_of(dir.path(), locked)) else {
        return;
    };
    let failed = run(dir.path(), &["update"]);
    drop(lock);
    assert_eq!(
        failed.status.code(),
        Some(2),
        "the write to {locked} stops the run: {failed:?}"
    );
    assert!(
        read_json(dir.path(), ".houserules.json")
            .get("schema")
            .is_none(),
        "the schema key is written last"
    );
    after(dir.path());
    update(dir.path());

    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        &format!("the conversion that stopped at the write to {locked}"),
    );
}

/// The first write to the marker is the pin of the first kit pack.
#[test]
fn an_update_that_stops_at_the_pin_of_the_first_kit_pack_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(".houserules.json", |_| {}, |_| {});
}

/// The project has no area `tools`, which the third kit pack adds. The packs
/// before it are installed when the write to `areas.json` stops the run.
#[test]
fn an_update_that_stops_at_the_areas_of_the_third_kit_pack_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        "knowledge/areas.json",
        |dir| drop_area(dir, "tools"),
        |_| {},
    );
}

/// Deletes the copy of the kit pack `name`: the file the write of the copy
/// stopped at held the pack of the binary in another form only to let the
/// write fail, and an interrupted run leaves no such file.
fn delete_copy(dir: &Path, name: &str) {
    fs::remove_file(path_of(dir, &format!("knowledge/packs/{name}.json")))
        .expect("delete the copy");
}

/// The project has no area `docs`, which the first kit pack adds. The write of
/// the copy stops the run after the pin and the area are written.
#[test]
fn an_update_that_stops_at_the_copy_of_the_first_kit_pack_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        "knowledge/packs/knowledge-base.json",
        |dir| {
            drop_area(dir, "docs");
            write_compact_copy(dir, "knowledge-base");
        },
        |dir| delete_copy(dir, "knowledge-base"),
    );
}

/// The same stop at the copy of the third kit pack, which adds the area
/// `tools` after the packs before it are installed.
#[test]
fn an_update_that_stops_at_the_copy_of_the_third_kit_pack_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        "knowledge/packs/quality.json",
        |dir| {
            drop_area(dir, "tools");
            write_compact_copy(dir, "quality");
        },
        |dir| delete_copy(dir, "quality"),
    );
}

/// The marker lists a modified entry in `overrides`, so the conversion writes
/// `knowledge/overrides.json` first. The write to the marker stops the run
/// before the marker loses its `overrides` key.
#[test]
fn an_update_that_stops_at_the_marker_after_the_overrides_document_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        ".houserules.json",
        |dir| {
            edit_entries(dir, "knowledge/writing-style.json", |entries| {
                let entry = entries
                    .iter_mut()
                    .find(|entry| entry["id"] == json!("writing-style.principles"))
                    .expect("writing-style.principles");
                entry["body"] = json!(["Our own style rule."]);
            });
            edit_marker(dir, |marker| {
                marker["overrides"] = json!(["writing-style.principles"]);
            });
        },
        |_| {},
    );
}

/// The write of the knowledge schema, the first D3 file, stops the run after
/// the packs, the entries, and the standing rules are done.
#[test]
fn an_update_that_stops_at_the_knowledge_schema_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at("knowledge/schema.json", |_| {}, |_| {});
}

/// A kit pack with no pin whose copy path holds the copy of the binary, in
/// the form of the writer or in another form, gets its pin: the state that a
/// stop between the copy and the pin left in an earlier build (spec C20).
#[test]
fn an_unpinned_kit_pack_whose_copy_is_the_binarys_copy_gets_its_pin() {
    let control = install_1_4_0(false);
    update(control.path());
    for emit_form in [false, true] {
        let dir = install_1_4_0(false);
        write_compact_copy(dir.path(), "knowledge-base");
        if emit_form {
            let copy = read_json(dir.path(), "knowledge/packs/knowledge-base.json");
            write_json(dir.path(), "knowledge/packs/knowledge-base.json", &copy);
        }

        update(dir.path());

        tree_snapshot::assert_same_tree(
            &tree_snapshot::snapshot(control.path()),
            &tree_snapshot::snapshot(dir.path()),
            &format!("an unpinned copy (written in emit form: {emit_form})"),
        );
    }
}

/// A kit pack whose pin was written and whose copy is absent, or does not
/// match the `sha256` of its pin, gets its copy (spec C20): the state that a
/// stop between the pin and the copy leaves.
#[test]
fn a_pinned_kit_pack_whose_copy_is_absent_or_wrong_gets_its_copy() {
    let control = install_1_4_0(false);
    update(control.path());
    let pin = read_json(control.path(), ".houserules.json")["packs"]["knowledge-base"].clone();
    for wrong_copy in [false, true] {
        let dir = install_1_4_0(false);
        edit_marker(dir.path(), |marker| {
            marker["packs"] = json!({"knowledge-base": pin});
        });
        if wrong_copy {
            let mut copy = kit_pack("knowledge-base");
            copy["entries"][0]["summary"] = json!("A hand edit.");
            write_json(dir.path(), "knowledge/packs/knowledge-base.json", &copy);
        }

        update(dir.path());

        tree_snapshot::assert_same_tree(
            &tree_snapshot::snapshot(control.path()),
            &tree_snapshot::snapshot(dir.path()),
            &format!("a pin with a copy that is wrong: {wrong_copy}"),
        );
    }
}

/// A kit pack with no pin whose copy path holds another file, or a directory,
/// is not the copy of the binary: the conversion stops before any write with
/// one line that names the path (spec C20).
#[test]
fn a_foreign_file_or_directory_at_the_copy_path_of_an_unpinned_kit_pack_stops_the_conversion() {
    for (kind, setup) in [
        (
            "a file with other content",
            (|path: &Path| fs::write(path, "{\"mine\": true}\n").expect("write a file"))
                as fn(&Path),
        ),
        ("a file that is no JSON", |path: &Path| {
            fs::write(path, "not json").expect("write a file");
        }),
        ("a directory", |path: &Path| {
            fs::create_dir(path).expect("create a directory");
            fs::write(path.join("keep.txt"), "kept").expect("write a file");
        }),
    ] {
        let dir = install_1_4_0(false);
        let copy = path_of(dir.path(), "knowledge/packs/process.json");
        fs::create_dir_all(copy.parent().expect("a parent")).expect("create knowledge/packs/");
        setup(&copy);
        let before = tree_snapshot::snapshot(dir.path());

        let output = run(dir.path(), &["update"]);

        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!(
                "knowledge/packs/process.json: this path holds something other than the copy of the kit pack process that houserules {} writes, and .houserules.json has no pin for the pack; move it away, then run houserules update again\n",
                env!("CARGO_PKG_VERSION")
            ),
            "{kind}"
        );
        assert_eq!(output.status.code(), Some(2), "{kind}: {output:?}");
        assert!(output.stdout.is_empty(), "{kind}: {output:?}");
        tree_snapshot::assert_same_tree(
            &before,
            &tree_snapshot::snapshot(dir.path()),
            &format!("update with {kind} at the copy path"),
        );
    }
}
