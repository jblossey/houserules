//! `init` and `update` after the CI wiring became opt-in (batch 37, spec 10.7
//! and 10.3; cases 8 and 9; HR-243, HR-245). `init` writes no CI file and prints
//! one line; `update` writes none again, prints the opt-in line once for an
//! install that holds the seeded workflow, reads a kept copy of that workflow as
//! YAML, and prints the pin line for a file that `houserules ci write` created
//! with another version.
//!
//! `init` seeds no workflow now, so each case about the seeded workflow starts
//! from the one that an earlier release seeded (`seed_workflow`). A case that a
//! stop interrupts finishes at the next run (`houserules.migrations-finish-after-a-stop`).
//! Each case runs the compiled binary against a scratch repository.

mod help_support;
mod locked_file;
mod tracker_support;
mod tree_snapshot;

use std::fs;
use std::path::{Path, PathBuf};

use help_support::{help_of, help_of_args, omits, states};
use serde_json::json;
use tempfile::TempDir;
use tracker_support::{
    add_submodule, commit_all, edit_marker, git, read_json, run, run_ok, scratch_git_repo,
    seed_workflow, stdout,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The seeded CI workflow of an install at the git top level.
const WORKFLOW: &str = ".github/workflows/knowledge.yml";

/// The seeded CI workflow of an install in the folder `sub`.
const SUB_WORKFLOW: &str = ".github/workflows/knowledge-sub.yml";

/// The first line of a file that `houserules ci write` created (spec 10.3).
const CI_WRITTEN_BY: &str = "# Written by houserules ci write; rewrite it with houserules ci write --force and the flags that wrote it.";

/// The pin line of `update` for a file of `ci write` of another version (spec 10.3).
const PIN_LINE: &str = "update: <path> pins houserules <old>; this binary is <new>: run houserules ci write --force with the flags that wrote it (a GitHub reference file takes the --sha of v<new>)";

/// The version of the binary under test.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The opt-in line of `update` for the workflow at `path` (spec 10.7).
fn opt_in_line(path: &str) -> String {
    format!(
        "update: {path} runs the houserules gates; to add the review layer, run houserules ci print --host github"
    )
}

/// The pin line of `update` for the file at `path` that pins `old` (spec 10.3).
fn pin_line(path: &str, old: &str) -> String {
    PIN_LINE
        .replace("<path>", path)
        .replace("<old>", old)
        .replace("<new>", VERSION)
}

/// The file `relative` below `dir`, built component by component
/// (`houserules.path-pins-mirror-the-code`).
fn file(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// A scratch repository, with an `origin` remote at `origin` when there is one.
fn repository(origin: Option<&str>) -> TempDir {
    let dir = scratch_git_repo();
    if let Some(url) = origin {
        git(dir.path(), &["remote", "add", "origin", url]);
    }
    dir
}

/// A scratch repository with a plain install and the workflow that an earlier
/// release seeded.
fn seeded() -> TempDir {
    let dir = repository(None);
    run_ok(dir.path(), &["init"]);
    seed_workflow(dir.path(), dir.path());
    dir
}

/// The stdout lines of `update` on the install at `root`, which must exit 0.
fn update_lines(root: &Path) -> Vec<String> {
    stdout(&run_ok(root, &["update"]))
        .lines()
        .map(str::to_string)
        .collect()
}

/// The line of `init` that says it wrote no CI file, for the host text `host`
/// (spec 10.7).
fn no_ci_file_line(host: &str) -> String {
    format!(
        "init: houserules wrote no CI file; to run the gates and the review layer in your pipeline, run houserules ci print --host {host}"
    )
}

/// The stdout lines of `output`.
fn lines_of(output: &std::process::Output) -> Vec<String> {
    stdout(output).lines().map(str::to_string).collect()
}

/// The lines of `lines` that are hand steps.
fn hand_steps(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line.starts_with("hand step: "))
        .cloned()
        .collect()
}

// ---- the help: init and files, update ----

/// The words of the help that 10.7 makes false are gone, and the words that
/// replace them are there: `init --dir`, and the last sentence of `files`
/// (`quality.user-facing-text-is-contract`).
#[test]
fn the_help_of_init_and_files_states_that_init_writes_no_ci_file() {
    let dir_help = "The install root; defaults to the current directory. Unlike every read command's `--dir` above, this is not resolved against an enclosing install -- the directory itself is the root. It must exist and lie in a git repository: the top level, or a directory below it for a subdirectory install. A new install root (one with no `.houserules.json` yet) is refused when it nests with another install of the repository; a directory that holds its own `.houserules.json` is no new root";
    let old_dir_help = [
        "which writes its CI workflow at the top level",
        "would write another install's workflow file",
    ];
    states("init --help", &help_of("init"), &[dir_help]);
    omits("init --help", &help_of("init"), &old_dir_help);

    let files = "A seeded CI workflow is a D3 file; init writes none (houserules ci prints or writes the CI wiring)";
    let old_files = ["that only a GitHub-hosted target gets"];
    states("files --help", &help_of("files"), &[files]);
    states("houserules --help", &help_of_args(&["--help"]), &[files]);
    omits("files --help", &help_of("files"), &old_files);
    omits("houserules --help", &help_of_args(&["--help"]), &old_files);
}

/// The help of `update` states what `update` does with the CI wiring.
#[test]
fn the_help_of_update_states_the_reading_of_a_kept_workflow_and_the_two_lines() {
    let sentences = [
        "The gates job of a kept copy is each job with a step that runs `houserules check-knowledge`, read as YAML whatever its name and indentation, and the markers are read per job: a `permissions` mapping with `contents: write` is asked to replace it with `contents: read`; a copy with no such job gets one hand step, and so does a file that is not valid YAML, with the parser's line.",
        "`update` writes no CI workflow: one that the project deleted stays absent.",
        "An install that holds the seeded workflow gets one line, once, that names `houserules ci print --host github` for the review layer, and `reviewLayerOffered` in `.houserules.json` records it.",
        "For each tracked file of the repository that `houserules ci write` created with another version than the binary's, it prints one line that names `houserules ci write --force` with the flags that wrote the file.",
        "A GitHub reference file takes the `--sha` of the new release.",
    ];
    states("update --help", &help_of("update"), &sentences);
    states("houserules --help", &help_of_args(&["--help"]), &sentences);
}

// ---- case 8 and 9: update and the seeded workflow ----

/// Case 9: `update` on an install that holds the seeded workflow keeps it (no
/// new job), prints the opt-in line once, and records `reviewLayerOffered`. A
/// second `update` prints no such line and changes nothing. `check-knowledge`
/// accepts the key.
#[test]
fn update_prints_the_opt_in_line_once_and_records_it() {
    let dir = seeded();
    let workflow = fs::read(file(dir.path(), WORKFLOW)).expect("read the workflow");

    let first = update_lines(dir.path());

    assert_eq!(
        first
            .iter()
            .filter(|line| **line == opt_in_line(WORKFLOW))
            .count(),
        1,
        "{first:?}"
    );
    assert!(
        !first.iter().any(|line| line.starts_with("wrote ")),
        "{first:?}"
    );
    assert_eq!(
        fs::read(file(dir.path(), WORKFLOW)).unwrap(),
        workflow,
        "update adds no job"
    );
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["reviewLayerOffered"],
        json!(true)
    );
    run_ok(dir.path(), &["check-knowledge"]);

    let after_first = snapshot(dir.path());
    let second = update_lines(dir.path());

    assert!(
        !second.iter().any(|line| line.starts_with("update: ")),
        "{second:?}"
    );
    assert_same_tree(&after_first, &snapshot(dir.path()), "a second update");
}

/// Case 9: a subdirectory install names its own workflow file in the line.
#[test]
fn update_names_the_own_workflow_of_a_subdirectory_install_in_the_opt_in_line() {
    let dir = repository(None);
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).expect("create sub/");
    run_ok(&sub, &["init"]);
    seed_workflow(dir.path(), &sub);

    let lines = update_lines(&sub);

    assert!(lines.contains(&opt_in_line(SUB_WORKFLOW)), "{lines:?}");
    assert_eq!(
        read_json(&sub, ".houserules.json")["reviewLayerOffered"],
        json!(true)
    );
}

/// Case 9: an install with no seeded workflow gets no opt-in line, and the key
/// stays absent.
#[test]
fn update_without_a_seeded_workflow_prints_no_opt_in_line_and_records_nothing() {
    let dir = repository(Some("https://github.com/acme/widgets.git"));
    run_ok(dir.path(), &["init"]);

    let lines = update_lines(dir.path());

    assert!(
        !lines.iter().any(|line| line.starts_with("update: ")),
        "{lines:?}"
    );
    assert!(
        read_json(dir.path(), ".houserules.json")
            .get("reviewLayerOffered")
            .is_none()
    );
    assert!(!file(dir.path(), ".github").exists());
}

/// Case 8: `update` on an install whose seeded workflow was deleted writes none
/// and prints nothing about it, and a second `update` changes nothing.
#[test]
fn update_with_the_seeded_workflow_deleted_writes_none_and_prints_nothing() {
    let dir = seeded();
    fs::remove_file(file(dir.path(), WORKFLOW)).expect("delete the workflow");

    let first = update_lines(dir.path());

    assert!(
        !first.iter().any(|line| line.contains("knowledge.yml")),
        "{first:?}"
    );
    assert!(
        !first.iter().any(|line| line.starts_with("update: ")),
        "{first:?}"
    );
    assert!(
        !file(dir.path(), WORKFLOW).exists(),
        "update wrote the workflow"
    );
    assert!(
        read_json(dir.path(), ".houserules.json")
            .get("reviewLayerOffered")
            .is_none()
    );
    let after_first = snapshot(dir.path());
    update_lines(dir.path());
    assert_same_tree(&after_first, &snapshot(dir.path()), "a second update");
}

/// A command that sets the tracker brings a seeded workflow to the form of the
/// adapter, and writes none that the project deleted.
#[test]
fn a_tracker_command_writes_no_deleted_workflow() {
    let dir = repository(Some("https://github.com/acme/widgets.git"));
    run_ok(dir.path(), &["init"]);
    seed_workflow(dir.path(), dir.path());
    fs::remove_file(file(dir.path(), WORKFLOW)).expect("delete the workflow");

    let output = run_ok(dir.path(), &["init", "--tracker", "github"]);

    assert!(
        !stdout(&output).contains("knowledge.yml"),
        "{}",
        stdout(&output)
    );
    assert!(
        !file(dir.path(), WORKFLOW).exists(),
        "init wrote the workflow"
    );
}

/// Spec 10.7: `init` prints its line only when the install holds no seeded
/// workflow after the run. A tracker command rewrites the workflow that the
/// install holds, so it prints the line of that write and not the line that
/// says no CI file was written.
#[test]
fn init_with_a_tracker_on_a_seeded_install_prints_the_wrote_line_and_not_the_no_ci_file_line() {
    let dir = repository(Some("https://github.com/acme/widgets.git"));
    run_ok(dir.path(), &["init"]);
    seed_workflow(dir.path(), dir.path());
    commit_all(dir.path(), "chore: seed the workflow");

    let lines = lines_of(&run_ok(dir.path(), &["init", "--tracker", "github"]));

    assert!(lines.contains(&format!("wrote {WORKFLOW}")), "{lines:?}");
    assert!(!lines.contains(&no_ci_file_line("github")), "{lines:?}");
    assert!(file(dir.path(), WORKFLOW).is_file());
}

/// Spec 10.7: `init` on an install that holds the seeded workflow prints no
/// line about a CI file, and leaves the workflow where it is.
#[test]
fn init_on_a_seeded_install_prints_no_line_that_says_it_wrote_no_ci_file() {
    let dir = repository(Some("https://github.com/acme/widgets.git"));
    run_ok(dir.path(), &["init"]);
    seed_workflow(dir.path(), dir.path());
    let before = snapshot(dir.path());

    let lines = lines_of(&run_ok(dir.path(), &["init"]));

    assert!(
        !lines
            .iter()
            .any(|line| line.starts_with("init: houserules wrote no CI file")),
        "{lines:?}"
    );
    assert_same_tree(&before, &snapshot(dir.path()), "a second init");
}

/// Spec 10.7: an install whose seeded workflow was deleted holds none after
/// the run, so `init` prints its line there, also when a tracker command runs.
#[test]
fn init_on_an_install_whose_seeded_workflow_was_deleted_prints_its_line() {
    let dir = repository(Some("https://github.com/acme/widgets.git"));
    run_ok(dir.path(), &["init"]);
    seed_workflow(dir.path(), dir.path());
    fs::remove_file(file(dir.path(), WORKFLOW)).expect("delete the workflow");

    for arguments in [&["init"][..], &["init", "--tracker", "github"][..]] {
        let lines = lines_of(&run_ok(dir.path(), arguments));

        assert!(
            lines.contains(&no_ci_file_line("github")),
            "{arguments:?}: {lines:?}"
        );
    }
}

/// Spec 10.7 and design.md 245: `init` and `update` refuse no install root
/// for a reason that only a seeded workflow has. A folder name that a GitHub
/// path filter reads as a pattern gives no workflow file name, and an install
/// that holds no workflow needs none, so both commands succeed there and write
/// nothing below `.github`.
#[test]
fn init_and_update_accept_a_root_that_no_workflow_file_name_could_hold() {
    // Windows file names cannot hold `*` or `?`.
    let folders: &[&str] = if cfg!(unix) {
        &["docs[v2]", "a+b", "a!b", "a]b", "a*b", "a?b"]
    } else {
        &["docs[v2]", "a+b", "a!b", "a]b"]
    };
    for folder in folders {
        let dir = repository(Some("https://gitlab.com/acme/widgets.git"));
        let root = dir.path().join(folder);
        fs::create_dir(&root).expect("create the install folder");

        let init = lines_of(&run_ok(&root, &["init"]));
        let update = lines_of(&run_ok(&root, &["update"]));

        assert!(
            init.contains(&no_ci_file_line("gitlab")),
            "{folder}: {init:?}"
        );
        assert!(
            update
                .iter()
                .any(|line| line.starts_with("houserules: updated ")),
            "{folder}: {update:?}"
        );
        assert!(
            !file(dir.path(), ".github").exists(),
            "{folder}: a workflow was written"
        );
    }
}

/// `update` leaves a workflow that `ownedPaths` lists as it found it, with no
/// line: the opt-in line is a line about that file.
#[test]
fn update_prints_no_opt_in_line_for_an_owned_workflow() {
    let dir = seeded();
    edit_marker(dir.path(), |marker| {
        marker.insert("ownedPaths".to_string(), json!([WORKFLOW]));
    });

    let lines = update_lines(dir.path());

    assert!(
        !lines.iter().any(|line| line.contains("knowledge.yml")),
        "{lines:?}"
    );
    assert!(
        read_json(dir.path(), ".houserules.json")
            .get("reviewLayerOffered")
            .is_none()
    );
}

/// An install that offered the review layer before prints no line again.
#[test]
fn update_prints_no_opt_in_line_when_the_marker_records_the_offer() {
    let dir = seeded();
    edit_marker(dir.path(), |marker| {
        marker.insert("reviewLayerOffered".to_string(), json!(true));
    });

    let lines = update_lines(dir.path());

    assert!(
        !lines.iter().any(|line| line.starts_with("update: ")),
        "{lines:?}"
    );
}

/// The write of the marker ends `update` and records the offer
/// (`houserules.migrations-finish-after-a-stop`): a run that stops at that
/// write prints no opt-in line and records nothing, and the next run prints the
/// line and leaves the tree of a run that no stop interrupted.
#[test]
fn an_update_that_stops_at_the_marker_prints_the_opt_in_line_at_the_next_update() {
    let control = seeded();
    update_lines(control.path());

    let dir = seeded();
    let Some(lock) = locked_file::lock(&file(dir.path(), ".houserules.json")) else {
        return;
    };
    let failed = run(dir.path(), &["update"]);
    drop(lock);
    assert_eq!(failed.status.code(), Some(2), "{failed:?}");
    assert!(
        !stdout(&failed).contains("update: "),
        "the stopped run printed a line: {}",
        stdout(&failed)
    );

    let lines = update_lines(dir.path());

    assert!(lines.contains(&opt_in_line(WORKFLOW)), "{lines:?}");
    assert_same_tree(
        &snapshot(control.path()),
        &snapshot(dir.path()),
        "the update that stopped at the marker",
    );
}

// ---- case 9: the reader of a kept copy (HR-243) ----

/// A kept copy of the seeded workflow with the text `text`, which the baseline
/// does not hold: `update` keeps it and prints the hand steps of the reader.
fn kept_steps(text: &str) -> Vec<String> {
    let dir = seeded();
    fs::write(file(dir.path(), WORKFLOW), text).expect("change the workflow");
    commit_all(dir.path(), "chore: our workflow");
    let kept = format!("kept {WORKFLOW} (locally modified)");

    let first = update_lines(dir.path());
    assert!(first.contains(&kept), "{first:?}");
    let second = update_lines(dir.path());
    assert_eq!(hand_steps(&second), hand_steps(&first), "the second run");
    assert_eq!(
        fs::read_to_string(file(dir.path(), WORKFLOW)).unwrap(),
        text,
        "update keeps the copy"
    );
    hand_steps(&first)
}

/// A workflow with the jobs `jobs`, written at two spaces per level.
fn workflow_of(jobs: &str) -> String {
    format!("name: ours\non:\n  pull_request:\njobs:\n{jobs}")
}

/// A gates job named `name` at two spaces, with `permissions` (the lines under
/// the job, or empty), a checkout step that holds the `ref` input and drops the
/// persisted credential, and the step that runs the gates.
fn gates_job(name: &str, permissions: &str) -> String {
    format!(
        "  {name}:\n    runs-on: ubuntu-latest\n{permissions}    steps:\n      - uses: actions/checkout@v7\n        with:\n          ref: ${{{{ github.event.pull_request.head.sha }}}}\n          persist-credentials: false\n      - run: houserules check-knowledge\n"
    )
}

/// The block of `permissions` with the scope `contents: <value>`.
fn contents(value: &str) -> String {
    format!("    permissions:\n      contents: {value}\n")
}

/// The hand step for a gates job that grants `contents: write` (spec 10.7).
fn write_step(job: &str) -> String {
    format!(
        "hand step: {WORKFLOW} grants contents: write in the {job} job; replace it with contents: read"
    )
}

/// The hand step for a gates job with no `permissions` key (spec 8.5).
fn block_step() -> String {
    format!(
        "hand step: {WORKFLOW} lacks contents: read; add permissions: {{contents: read}} to the gates job"
    )
}

/// The hand step for a kept copy with no gates job (spec 10.7).
fn no_gates_job_step() -> String {
    format!(
        "hand step: {WORKFLOW} has no job that runs houserules check-knowledge; houserules update checks no permissions there"
    )
}

/// Case 9 and HR-243: a gates job with `contents: write` gets the replace step
/// that names the job; one with `contents: read` gets nothing.
#[test]
fn a_gates_job_with_contents_write_gets_the_replace_step() {
    assert_eq!(
        kept_steps(&workflow_of(&gates_job("gates", &contents("write")))),
        [write_step("gates")]
    );
    assert_eq!(
        kept_steps(&workflow_of(&gates_job("gates", &contents("read")))),
        Vec::<String>::new()
    );
}

/// The hand step for a gates job that holds the scope `scope` with the value
/// `value`, which is not `read` (spec 10.7).
fn sets_step(job: &str, scope: &str, value: &str) -> String {
    format!(
        "hand step: {WORKFLOW} sets {scope}: {value} in the {job} job; replace it with {scope}: read"
    )
}

/// Changes the one place `from` of the workflow of the install `dir` to `to`,
/// commits the change, and returns the hand steps of the next `update`.
fn edited_steps(dir: &Path, from: &str, to: &str) -> Vec<String> {
    let path = file(dir, WORKFLOW);
    let text = fs::read_to_string(&path).expect("read the workflow");
    assert_eq!(text.matches(from).count(), 1, "{from:?} stands once");
    fs::write(&path, text.replace(from, to)).expect("change the workflow");
    commit_all(dir, "chore: edit the workflow");
    hand_steps(&update_lines(dir))
}

/// Spec 10.7: a scope that the mapping holds with another value than `read`
/// gets a step that names the scope, the value, and the job, and replaces the
/// value; no step adds a second key. When the project follows the step, the
/// next `update` prints no hand step. `contents: none` stands for the plain
/// forms.
#[test]
fn contents_none_gets_the_replace_step_and_the_next_update_accepts_the_fix() {
    let dir = seeded();

    let steps = edited_steps(
        dir.path(),
        "      contents: read\n",
        "      contents: none\n",
    );
    assert_eq!(steps, [sets_step("gates", "contents", "none")]);

    let followed = edited_steps(
        dir.path(),
        "      contents: none\n",
        "      contents: read\n",
    );
    assert_eq!(followed, Vec::<String>::new());
}

/// Spec 10.7: `issues: write` with the GitHub tracker is a scope held with
/// another value than `read`: it gets the replace step, not an add step, and
/// the next `update` prints no hand step once the project follows it.
#[test]
fn issues_write_gets_the_replace_step_and_the_next_update_accepts_the_fix() {
    let dir = repository(Some("https://github.com/acme/widgets.git"));
    run_ok(dir.path(), &["init", "--tracker", "github"]);
    seed_workflow(dir.path(), dir.path());
    update_lines(dir.path());
    commit_all(dir.path(), "chore: the form of the github tracker");

    let steps = edited_steps(dir.path(), "      issues: read\n", "      issues: write\n");
    assert_eq!(steps, [sets_step("gates", "issues", "write")]);

    let followed = edited_steps(dir.path(), "      issues: write\n", "      issues: read\n");
    assert_eq!(followed, Vec::<String>::new());
}

/// HR-243: a re-indented gates job is the same job. The job sits at four
/// spaces, and its keys at eight; a copy with `contents: read` gets no step, one
/// with `contents: write` gets the replace step, and one with no key gets the
/// block step.
#[test]
fn a_re_indented_gates_job_is_read_like_any_other() {
    let reindented = |permissions: &str| {
        format!(
            "name: ours\non:\n  pull_request:\njobs:\n    gates:\n        runs-on: ubuntu-latest\n{permissions}        steps:\n            - uses: actions/checkout@v7\n              with:\n                  ref: ${{{{ github.event.pull_request.head.sha }}}}\n                  persist-credentials: false\n            - run: houserules check-knowledge\n"
        )
    };

    assert_eq!(
        kept_steps(&reindented(
            "        permissions:\n            contents: read\n"
        )),
        Vec::<String>::new()
    );
    assert_eq!(
        kept_steps(&reindented(
            "        permissions:\n            contents: write\n"
        )),
        [write_step("gates")]
    );
    assert_eq!(kept_steps(&reindented("")), [block_step()]);
}

/// HR-243: a renamed gates job is read, and the replace step names it.
#[test]
fn a_renamed_gates_job_is_read_and_named() {
    assert_eq!(
        kept_steps(&workflow_of(&gates_job(
            "knowledge-gate",
            &contents("write")
        ))),
        [write_step("knowledge-gate")]
    );
}

/// HR-243: two gates jobs are each checked.
#[test]
fn two_gates_jobs_are_each_checked() {
    let jobs = format!(
        "{}{}",
        gates_job("first", &contents("write")),
        gates_job("second", &contents("write"))
    );
    assert_eq!(
        kept_steps(&workflow_of(&jobs)),
        [write_step("first"), write_step("second")]
    );

    let one_bad = format!(
        "{}{}",
        gates_job("first", &contents("read")),
        gates_job("second", &contents("write"))
    );
    assert_eq!(kept_steps(&workflow_of(&one_bad)), [write_step("second")]);
}

/// HR-243: a copy with no job that runs `houserules check-knowledge` gets one
/// step and no other.
#[test]
fn a_copy_with_no_gates_job_gets_one_step() {
    let lint =
        "  lint:\n    runs-on: ubuntu-latest\n    steps:\n      - run: houserules check-backlog\n";

    assert_eq!(kept_steps(&workflow_of(lint)), [no_gates_job_step()]);
    assert_eq!(kept_steps("name: ours\n"), [no_gates_job_step()]);
}

/// HR-243: a file that does not parse gets one step with the parser's line, and
/// no other.
#[test]
fn a_copy_that_is_no_yaml_gets_one_step_with_the_parsers_line() {
    let tab = "jobs:\n  gates:\n\tsteps: []\n";

    assert_eq!(
        kept_steps(tab),
        [format!(
            "hand step: {WORKFLOW} is not valid YAML (tabs disallowed within this context (block indentation) at byte 16 line 3 column 2); houserules update checks no permissions there"
        )]
    );
}

/// Case 9: a copy whose other jobs hold `persist-credentials: false` or
/// `contents: read` still gets the steps of its own gates job: a marker of
/// another job is no marker of the gates job.
#[test]
fn a_marker_of_another_job_is_no_marker_of_the_gates_job() {
    let gates = "  gates:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v7\n        with:\n          ref: ${{ github.event.pull_request.head.sha }}\n      - run: houserules check-knowledge\n";
    let docs = "  docs:\n    runs-on: ubuntu-latest\n    permissions:\n      contents: read\n    steps:\n      - uses: actions/checkout@v7\n        with:\n          persist-credentials: false\n      - run: echo docs\n";

    assert_eq!(
        kept_steps(&workflow_of(&format!("{gates}{docs}"))),
        [
            block_step(),
            format!(
                "hand step: {WORKFLOW} lacks persist-credentials: false; add it under with: of its actions/checkout step"
            ),
        ]
    );
}

/// A comment is no marker: the copy that holds `contents: read` only in a
/// comment gets the block step.
#[test]
fn a_comment_is_no_marker() {
    let jobs = gates_job("gates", "    # permissions: {contents: read}\n");

    assert_eq!(kept_steps(&workflow_of(&jobs)), [block_step()]);
}

// ---- 10.3: the pin line ----

/// A repository whose install an earlier release seeded and whose tracked files
/// hold the file `path` with the text `text`.
fn with_a_tracked_file(path: &str, text: &str) -> TempDir {
    let dir = seeded();
    let target = file(dir.path(), path);
    fs::create_dir_all(target.parent().expect("a parent")).expect("create the folder");
    fs::write(&target, text).expect("write the file");
    commit_all(dir.path(), "chore: add the file");
    dir
}

/// The text of a file of `ci write` that pins `version`.
fn ci_write_file(version: &str) -> String {
    format!("{CI_WRITTEN_BY}\n# houserules {version}\nname: houserules review\n")
}

/// Case 9: a file that `ci write` created with an older version gets the pin
/// line on every run, and `update` writes nothing to it.
#[test]
fn update_prints_the_pin_line_for_a_file_of_an_older_ci_write() {
    let path = ".github/workflows/houserules-review.yml";
    let text = ci_write_file("0.9.0");
    let dir = with_a_tracked_file(path, &text);

    let first = update_lines(dir.path());

    assert_eq!(
        first
            .iter()
            .filter(|line| **line == pin_line(path, "0.9.0"))
            .count(),
        1,
        "{first:?}"
    );
    assert_eq!(fs::read_to_string(file(dir.path(), path)).unwrap(), text);
    let after_first = snapshot(dir.path());
    let second = update_lines(dir.path());
    assert!(second.contains(&pin_line(path, "0.9.0")), "{second:?}");
    assert_same_tree(&after_first, &snapshot(dir.path()), "a second update");
}

/// Case 9: a subdirectory install finds the file of `ci write` at the top level
/// of the repository, and the line names the path from the top level.
#[test]
fn update_in_a_subdirectory_install_finds_the_file_at_the_top_level() {
    let dir = repository(None);
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).expect("create sub/");
    run_ok(&sub, &["init"]);
    let path = ".gitlab/houserules-review.gitlab-ci.yml";
    let target = file(dir.path(), path);
    fs::create_dir_all(target.parent().expect("a parent")).expect("create the folder");
    fs::write(&target, ci_write_file("1.0.0")).expect("write the file");
    commit_all(dir.path(), "chore: add the file");

    let lines = update_lines(&sub);

    assert!(lines.contains(&pin_line(path, "1.0.0")), "{lines:?}");
}

/// Case 9: no pin line for a file with the version of the binary, for a file
/// that starts with another line, for a file of `ci write` whose second line is
/// no version line, and for a file that git does not track. A file with CRLF
/// line ends is read like one with LF.
#[test]
fn update_prints_no_pin_line_unless_the_file_is_tracked_and_pins_another_version() {
    let cases = [
        ("current.yml", ci_write_file(VERSION), false),
        (
            "other-first-line.yml",
            "# Written by hand\n# houserules 0.9.0\n".to_string(),
            false,
        ),
        (
            "no-version-line.yml",
            format!("{CI_WRITTEN_BY}\nname: ours\n"),
            false,
        ),
        (
            "crlf.yml",
            ci_write_file("0.9.0").replace('\n', "\r\n"),
            true,
        ),
    ];
    let dir = seeded();
    for (name, text, _) in &cases {
        fs::write(file(dir.path(), name), text).expect("write the file");
    }
    commit_all(dir.path(), "chore: add the files");
    fs::write(file(dir.path(), "untracked.yml"), ci_write_file("0.9.0")).expect("write the file");

    let lines = update_lines(dir.path());

    let pins: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("update: ") && line.contains(" pins houserules "))
        .collect();
    assert_eq!(pins, [&pin_line("crlf.yml", "0.9.0")], "{lines:?}");
    for (name, _, pinned) in &cases {
        assert_eq!(
            lines.iter().any(|line| line.contains(name)),
            *pinned,
            "{name}: {lines:?}"
        );
    }
}

/// A tracked path that is no regular file has no version to read: a file that
/// the work tree lost, and a submodule. `update` exits 0 and prints no pin line.
#[test]
fn update_reads_no_version_from_a_lost_tracked_file_or_a_submodule() {
    let dir = seeded();
    fs::write(file(dir.path(), "lost.yml"), ci_write_file("0.9.0")).expect("write the file");
    add_submodule(dir.path(), "vendor", "file.txt");
    commit_all(dir.path(), "chore: add the file and the submodule");
    fs::remove_file(file(dir.path(), "lost.yml")).expect("lose the file");

    let lines = update_lines(dir.path());

    assert!(
        !lines.iter().any(|line| line.contains(" pins houserules ")),
        "{lines:?}"
    );
}

/// A tracked link is no regular file: `update` does not follow it, so a file of
/// `ci write` behind the link gets no pin line from the link.
#[cfg(unix)]
#[test]
fn update_does_not_follow_a_tracked_link() {
    let outside = tempfile::tempdir().expect("tempdir");
    let target = outside.path().join("review.yml");
    fs::write(&target, ci_write_file("0.9.0")).expect("write the target");
    let dir = seeded();
    std::os::unix::fs::symlink(&target, file(dir.path(), "link.yml")).expect("link the file");
    commit_all(dir.path(), "chore: add the link");

    let lines = update_lines(dir.path());

    assert!(
        !lines.iter().any(|line| line.contains(" pins houserules ")),
        "{lines:?}"
    );
}

/// A tracked file that cannot be read stops `update` before any write, with one
/// line that names the file and holds the error of the operating system.
#[cfg(unix)]
#[test]
fn update_names_a_tracked_file_it_cannot_read_and_writes_nothing() {
    use std::os::unix::fs::PermissionsExt;

    /// Gives the file its permissions back, so that the scratch directory goes.
    struct Restore(PathBuf, fs::Permissions);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, self.1.clone());
        }
    }

    let dir = seeded();
    // The binary names the top level as `--dir` gave it, so the expected path is built from the
    // same value (`houserules.path-pins-mirror-the-code`).
    let secret = file(dir.path(), "secret.yml");
    fs::write(&secret, ci_write_file("0.9.0")).expect("write the file");
    commit_all(dir.path(), "chore: add the file");
    let original = fs::metadata(&secret).expect("read the mode").permissions();
    let _restore = Restore(secret.clone(), original);
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).expect("lock the file");
    let Err(error) = fs::File::open(&secret) else {
        return;
    };
    let marker_before = fs::read(file(dir.path(), ".houserules.json")).expect("read the marker");

    let output = run(dir.path(), &["update"]);

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{}: {error}\n", secret.display())
    );
    assert_eq!(
        fs::read(file(dir.path(), ".houserules.json")).unwrap(),
        marker_before
    );
}
