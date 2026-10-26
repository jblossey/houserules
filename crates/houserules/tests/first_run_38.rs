//! The two first-run fixes of batch 38 (design.md 289, spec 20, acceptance case 15; HR-280).
//!
//! 1. The `next:` line of `init` and `update` starts with `git add <paths> && ` when the index
//!    below the install root holds a file and the command wrote a file that git neither tracks
//!    nor ignores. `<paths>` are the first path components of those files, sorted. The dead-glob
//!    gate of `check-knowledge` is unchanged: once the index holds files, the files that git
//!    tracks decide, so a gate run before the `git add` still fails.
//! 2. `check-backlog` reports an item of `backlog/items/` whose `batch` no record of
//!    `backlog/batches.json` or `backlog/archive/batches.json` holds.
//!
//! 3. The line comes from the files that the command wrote, and the command opens no file outside
//!    them: `init` and `update` end with exit 0 where an untracked symlink points at a FIFO.
//!
//! Each case runs the compiled binary against a scratch git repository. The binary keeps the
//! environment of the test run, so the configuration that the autocrlf suite injects reaches it.
//! The scratch repository sets its own `core.excludesFile`, so no ignore file of the host changes
//! what git ignores, and its fixture git calls run through `with_default_git`
//! (`houserules.pinned-git-text-runs-hermetic`). A case that runs the printed line runs each of its
//! commands in order in the install root, as the shell runs them with `&&`, and requires exit 0.
//! `the_lines_of_these_tests_are_in_the_spec` shows that the spec holds each line.

mod tracker_support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    edit_marker, flat, houserules, printed, read_json, repo_root, stderr, stdout, text_below,
    with_default_git, write_json,
};

/// The null device, which a repository-local `core.excludesFile` names to keep the ignore file of
/// the host out of the fixture.
const NULL_DEVICE: &str = "/dev/null";

/// The spec of the batch, below the repository root.
const SPEC: &str = "docs/specs/2026-10-25-batch-38-openspec-and-release.md";

/// The `next:` line without the `git add` prefix (spec 20.1).
const PLAIN_LINE: &str = "next: houserules check-knowledge && houserules check-backlog";

/// The `next:` line with the `git add` prefix, `<paths>` still open (spec 20.1).
const GIT_ADD_FORM: &str =
    "next: git add <paths> && houserules check-knowledge && houserules check-backlog";

/// The finding of an unknown batch, its placeholders still open (spec 20.2).
const UNKNOWN_BATCH_FORM: &str =
    "<item file> <id>: batch <n> is not in backlog/batches.json or backlog/archive/batches.json";

/// The first path components of the files that a plain `init` writes: every file the kit seeds,
/// sorted, with the `.gitignore` that the command writes and the marker.
const INIT_PATHS: &str =
    ".claude .gitignore .houserules.json AGENTS.md CLAUDE.md docs knowledge tools";

/// The line of `check-knowledge` that a dead glob prints after `init`, in a repository whose index
/// holds a file (the dead-glob gate, which spec 20.1 leaves as it is).
const DEAD_GLOB_LINE: &str =
    "knowledge/areas.json.tools.paths: \"tools/**\" matches no tracked file (git ls-files)";

/// The `next:` line with `paths` for `<paths>`.
fn git_add_line(paths: &str) -> String {
    GIT_ADD_FORM.replace("<paths>", paths)
}

/// The finding of spec 20.2 for the item `id` of the file `file`, whose batch is `batch`.
fn unknown_batch_line(file: &str, id: &str, batch: u64) -> String {
    UNKNOWN_BATCH_FORM
        .replace("<item file>", file)
        .replace("<id>", id)
        .replace("<n>", &batch.to_string())
}

/// Runs `houserules <args> --dir <dir>` with the environment of the test run.
fn run(dir: &Path, args: &[&str]) -> Output {
    houserules()
        .args(args)
        .arg("--dir")
        .arg(dir)
        .output()
        .unwrap_or_else(|error| panic!("run houserules {args:?}: {error}"))
}

/// [`run`] with a requirement of exit 0.
fn run_ok(dir: &Path, args: &[&str]) -> Output {
    let output = run(dir, args);
    assert!(
        output.status.success(),
        "houserules {args:?} failed: {}",
        printed(&output)
    );
    output
}

/// The one line of `output` that starts with `next: `.
fn next_line(output: &Output) -> String {
    let text = stdout(output);
    let mut lines = text.lines().filter(|line| line.starts_with("next: "));
    let line = lines
        .next()
        .unwrap_or_else(|| panic!("no next line in {text}"));
    assert!(lines.next().is_none(), "two next lines in {text}");
    line.to_string()
}

/// Runs each command of the printed `line` in `dir`, in order, and requires exit 0 for each. The
/// commands run without a shell: a printed path here is a plain word.
fn run_the_printed_line(dir: &Path, line: &str) {
    let commands = line.strip_prefix("next: ").expect("a next line");
    for command in commands.split(" && ") {
        let words: Vec<&str> = command.split_whitespace().collect();
        let (program, args) = words.split_first().expect("a command");
        let mut process = match *program {
            "git" => Command::new("git"),
            "houserules" => houserules(),
            other => panic!("the line runs {other}"),
        };
        let output = process
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap_or_else(|error| panic!("run `{command}`: {error}"));
        assert!(
            output.status.success(),
            "`{command}` exited {:?}: {}",
            output.status.code(),
            printed(&output)
        );
    }
}

/// Runs `git <args>` in `dir` with a fixed identity, no signing, and none of the host's git
/// configuration, so that no global ignore file hides a file from `git add -A`. It returns the
/// stdout, and a failure fails the test with the stderr of git.
fn git(dir: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    let output = with_default_git(&mut command)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A fresh scratch directory with `git init` already run, and a `core.excludesFile` of its own at
/// the null device: a repository-local value overrides the global one, so an ignore file of the
/// host hides no file from the binary or from `git add`.
fn scratch_git_repo() -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["config", "core.excludesFile", NULL_DEVICE]);
    dir
}

/// Commits every change in `dir`, an empty commit included.
fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            message,
        ],
    );
}

/// A scratch repository with one commit that holds `README.md`.
fn repo_with_a_commit() -> TempDir {
    let repo = scratch_git_repo();
    fs::write(repo.path().join("README.md"), "text\n").expect("write README.md");
    commit_all(repo.path(), "chore: start the project");
    repo
}

/// [`repo_with_a_commit`] after `init`, with the install committed.
fn committed_install() -> TempDir {
    let repo = repo_with_a_commit();
    run_ok(repo.path(), &["init"]);
    commit_all(repo.path(), "chore: houserules init");
    repo
}

/// Takes the tracked kit script out of the repository: it is gone from the index and from the
/// work tree, in a commit, so that `update` writes it again as a file that git does not track.
fn drop_the_script_in_a_commit(repo: &Path) {
    git(repo, &["rm", "-q", "tools/houserules-step.sh"]);
    commit_all(repo, "chore: drop the step script");
}

// ---- the next line -------------------------------------------------------------------

/// Spec 20.1 and acceptance case 15: in a repository with one commit, `init` prints the `git add`
/// form, and running the printed line exits 0 for both checks.
#[test]
fn init_after_a_first_commit_prints_the_git_add_line_and_the_printed_line_runs_green() {
    let repo = repo_with_a_commit();

    let line = next_line(&run_ok(repo.path(), &["init"]));

    assert_eq!(line, git_add_line(INIT_PATHS));
    run_the_printed_line(repo.path(), &line);
}

/// The dead-glob gate keeps its reading (design.md 289): once the index holds a file, the files that
/// git tracks decide, so `check-knowledge` fails after `init` until the kit files are added.
#[test]
fn the_dead_glob_gate_still_fails_after_init_until_the_kit_files_are_added() {
    let repo = repo_with_a_commit();
    run_ok(repo.path(), &["init"]);

    let before = run(repo.path(), &["check-knowledge"]);
    assert_eq!(before.status.code(), Some(1));
    assert_eq!(printed(&before), format!("{DEAD_GLOB_LINE}\n"));

    git(repo.path(), &["add", "-A"]);
    assert_eq!(
        run(repo.path(), &["check-knowledge"]).status.code(),
        Some(0)
    );
}

/// The index decides, not the history: a repository with staged files and no commit gets the `git
/// add` form too.
#[test]
fn init_with_a_staged_file_and_no_commit_prints_the_git_add_line() {
    let repo = scratch_git_repo();
    fs::write(repo.path().join("README.md"), "text\n").expect("write README.md");
    git(repo.path(), &["add", "README.md"]);

    let line = next_line(&run_ok(repo.path(), &["init"]));

    assert_eq!(line, git_add_line(INIT_PATHS));
    run_the_printed_line(repo.path(), &line);
}

/// Acceptance case 15: in a repository with no commit and an empty index, `init` prints the old
/// line, and running it exits 0 for both checks, because the gate then reads the file walk.
#[test]
fn init_in_a_repository_with_no_commit_prints_the_old_line_and_the_printed_line_runs_green() {
    let repo = scratch_git_repo();

    let line = next_line(&run_ok(repo.path(), &["init"]));

    assert_eq!(line, PLAIN_LINE);
    run_the_printed_line(repo.path(), &line);
}

/// The index decides, not the history: a commit that holds no file leaves the index empty, so
/// `init` prints the old line.
#[test]
fn init_after_a_commit_that_holds_no_file_prints_the_old_line() {
    let repo = scratch_git_repo();
    commit_all(repo.path(), "chore: an empty start");

    let line = next_line(&run_ok(repo.path(), &["init"]));

    assert_eq!(line, PLAIN_LINE);
    run_the_printed_line(repo.path(), &line);
}

/// A file that git tracks is no path of the line (the `.gitignore` that `init` changes), and a file
/// that git ignores is none either: `git add` refuses an ignored path, so the line would stop. The
/// printed line runs green with `CLAUDE.md` ignored.
#[test]
fn init_leaves_a_tracked_file_and_an_ignored_file_out_of_the_line() {
    let repo = scratch_git_repo();
    fs::write(repo.path().join("README.md"), "text\n").expect("write README.md");
    fs::write(repo.path().join(".gitignore"), "CLAUDE.md\n").expect("write .gitignore");
    commit_all(repo.path(), "chore: start the project");

    let line = next_line(&run_ok(repo.path(), &["init"]));

    assert_eq!(
        line,
        git_add_line(".claude .houserules.json AGENTS.md docs knowledge tools")
    );
    run_the_printed_line(repo.path(), &line);
}

/// A subdirectory install: the paths are relative to the install root, the index is the one below
/// it, and the printed line runs in the install root.
#[test]
fn a_subdirectory_install_names_its_paths_relative_to_the_install_root() {
    let repo = scratch_git_repo();
    fs::create_dir_all(repo.path().join("sub")).expect("create sub");
    fs::write(repo.path().join("sub/keep.txt"), "text\n").expect("write sub/keep.txt");
    fs::write(repo.path().join("top.txt"), "text\n").expect("write top.txt");
    commit_all(repo.path(), "chore: start the project");
    let root = repo.path().join("sub");

    let line = next_line(&run_ok(&root, &["init"]));

    assert_eq!(line, git_add_line(INIT_PATHS));
    run_the_printed_line(&root, &line);
}

/// A subdirectory install whose root holds no tracked file has an empty index below it, as the
/// dead-glob gate reads it, so `init` prints the old line even where the repository has commits.
#[test]
fn a_subdirectory_install_with_no_tracked_file_below_its_root_prints_the_old_line() {
    let repo = scratch_git_repo();
    fs::write(repo.path().join("top.txt"), "text\n").expect("write top.txt");
    commit_all(repo.path(), "chore: start the project");
    let root = repo.path().join("sub");
    fs::create_dir_all(&root).expect("create sub");

    let line = next_line(&run_ok(&root, &["init"]));

    assert_eq!(line, PLAIN_LINE);
    run_the_printed_line(&root, &line);
}

/// Spec 20.1 for `update`: a kit file that `update` writes again and that git does not track is a
/// path of the line, and the printed line runs green.
#[test]
fn update_prints_the_git_add_line_for_a_file_it_writes_that_git_does_not_track() {
    let repo = committed_install();
    drop_the_script_in_a_commit(repo.path());

    let line = next_line(&run_ok(repo.path(), &["update"]));

    assert_eq!(line, git_add_line("tools"));
    assert!(repo.path().join("tools/houserules-step.sh").is_file());
    run_the_printed_line(repo.path(), &line);
}

/// A file that `update` writes again and that the index holds is no path of the line.
#[test]
fn update_prints_the_old_line_for_a_file_it_writes_that_git_tracks() {
    let repo = committed_install();
    fs::remove_file(repo.path().join("tools/houserules-step.sh")).expect("remove the script");

    let line = next_line(&run_ok(repo.path(), &["update"]));

    assert_eq!(line, PLAIN_LINE);
    assert!(repo.path().join("tools/houserules-step.sh").is_file());
    assert_eq!(git(repo.path(), &["status", "--porcelain"]), "");
}

/// An `update` that writes no file prints the old line.
#[test]
fn update_that_writes_no_file_prints_the_old_line() {
    let repo = committed_install();

    let line = next_line(&run_ok(repo.path(), &["update"]));

    assert_eq!(line, PLAIN_LINE);
    assert_eq!(git(repo.path(), &["status", "--porcelain"]), "");
}

/// The line names the files that the command wrote, not every file that git does not track: the kit
/// files of an install that nobody added are all untracked, and an `update` that only restamps the
/// marker names the marker alone.
#[test]
fn update_names_the_files_it_wrote_and_not_every_untracked_file() {
    let repo = repo_with_a_commit();
    run_ok(repo.path(), &["init"]);
    edit_marker(repo.path(), |marker| {
        marker.insert("version".to_string(), json!("0.0.1"));
    });

    let line = next_line(&run_ok(repo.path(), &["update"]));

    assert_eq!(line, git_add_line(".houserules.json"));
}

/// The same inputs give the same line (the Determinism constraint of the plan): two identical
/// repositories print the same line for `init` and for `update`.
#[test]
fn the_next_line_is_the_same_in_two_identical_repositories() {
    let mut lines = Vec::new();
    for _ in 0..2 {
        let repo = repo_with_a_commit();
        let init = next_line(&run_ok(repo.path(), &["init"]));
        commit_all(repo.path(), "chore: houserules init");
        drop_the_script_in_a_commit(repo.path());
        let update = next_line(&run_ok(repo.path(), &["update"]));
        lines.push((init, update));
    }

    assert_eq!(lines[0], lines[1]);
    assert_eq!(lines[0], (git_add_line(INIT_PATHS), git_add_line("tools")));
}

/// `init --tracker json` names the `backlog` folder that the tracker install wrote, besides the
/// files of a plain `init`.
#[test]
fn init_with_the_json_tracker_names_the_backlog_that_it_wrote() {
    let repo = repo_with_a_commit();

    let line = next_line(&run_ok(repo.path(), &["init", "--tracker", "json"]));

    assert_eq!(
        line,
        git_add_line(
            ".claude .gitignore .houserules.json AGENTS.md CLAUDE.md backlog docs knowledge tools"
        )
    );
    run_the_printed_line(repo.path(), &line);
}

/// `init --workflow reference` names the `.githooks` folder that the workflow install wrote, and
/// the packs that it added are under `knowledge`, which the line names already.
#[test]
fn init_with_the_reference_workflow_names_the_hooks_that_it_wrote() {
    let repo = repo_with_a_commit();

    let line = next_line(&run_ok(
        repo.path(),
        &[
            "init",
            "--tracker",
            "json",
            "--workflow",
            "reference",
            "--add-required-packs",
        ],
    ));

    assert_eq!(
        line,
        git_add_line(
            ".claude .githooks .gitignore .houserules.json AGENTS.md CLAUDE.md backlog docs knowledge tools"
        )
    );
    run_the_printed_line(repo.path(), &line);
}

/// The line names what the command wrote: an `update` that finds every file as the kit has it
/// writes none, so the line stays plain although the files of the install are still untracked.
#[test]
fn an_update_that_rewrites_no_file_prints_the_old_line_while_the_install_is_untracked() {
    let repo = repo_with_a_commit();
    run_ok(repo.path(), &["init"]);

    let line = next_line(&run_ok(repo.path(), &["update"]));

    assert_eq!(line, PLAIN_LINE);
}

// ---- no file that the command did not write is opened --------------------------------

/// The longest that one command of the tests below may run. A run ends in well under a second. A
/// run that opens a FIFO that has no writer never ends.
#[cfg(unix)]
const LIMIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Makes a FIFO at `path`. A process that opens it for reading blocks until another process opens
/// it for writing.
#[cfg(unix)]
fn make_fifo(path: &Path) {
    use std::os::unix::ffi::OsStrExt;

    let name = std::ffi::CString::new(path.as_os_str().as_bytes()).expect("a path with no NUL");
    // SAFETY: `name` is a NUL-terminated string that lives through the call.
    let status = unsafe { libc::mkfifo(name.as_ptr(), 0o600) };
    assert_eq!(
        status,
        0,
        "mkfifo {}: {}",
        path.display(),
        std::io::Error::last_os_error()
    );
}

/// Adds the untracked symlink `pipe.link` to `repo`, which points at a FIFO in `elsewhere`. The
/// FIFO is outside the repository, so only the symlink is a file of the work tree. A process that
/// opens `pipe.link` blocks.
#[cfg(unix)]
fn link_to_a_fifo(repo: &Path, elsewhere: &Path) {
    let fifo = elsewhere.join("the.fifo");
    make_fifo(&fifo);
    std::os::unix::fs::symlink(&fifo, repo.join("pipe.link")).expect("create the symlink");
}

/// [`run_ok`] that ends the command and fails the test when it does not end within [`LIMIT`]. The
/// output goes to files outside the repository, so that the test reads it after the end of the
/// command and no pipe can fill.
#[cfg(unix)]
fn run_ok_within_the_limit(dir: &Path, args: &[&str]) -> Output {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let logs = tempfile::tempdir().expect("tempdir");
    let (out_path, err_path) = (logs.path().join("stdout"), logs.path().join("stderr"));
    let mut child = houserules()
        .args(args)
        .arg("--dir")
        .arg(dir)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&out_path).expect("create the stdout file"))
        .stderr(fs::File::create(&err_path).expect("create the stderr file"))
        .spawn()
        .unwrap_or_else(|error| panic!("start houserules {args:?}: {error}"));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait for houserules") {
            break status;
        }
        if start.elapsed() > LIMIT {
            child.kill().expect("end houserules");
            child.wait().expect("collect houserules");
            panic!(
                "houserules {args:?} did not end within {LIMIT:?}: it opens a file it did not write"
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let output = Output {
        status,
        stdout: fs::read(&out_path).expect("read the stdout file"),
        stderr: fs::read(&err_path).expect("read the stderr file"),
    };
    assert!(
        output.status.success(),
        "houserules {args:?} failed: {}",
        printed(&output)
    );
    output
}

/// The line comes from the files that `init` wrote, and `init` opens no other file: an untracked
/// symlink that points at a FIFO blocks any read, and the line holds the paths of the files that
/// `init` wrote alone.
#[cfg(unix)]
#[test]
fn init_opens_no_untracked_file_that_it_did_not_write() {
    let repo = repo_with_a_commit();
    let elsewhere = tempfile::tempdir().expect("tempdir");
    link_to_a_fifo(repo.path(), elsewhere.path());

    let line = next_line(&run_ok_within_the_limit(repo.path(), &["init"]));

    assert_eq!(line, git_add_line(INIT_PATHS));
}

/// The same for `update`: it ends where an untracked symlink points at a FIFO, and the line holds
/// the one path that `update` wrote.
#[cfg(unix)]
#[test]
fn update_opens_no_untracked_file_that_it_did_not_write() {
    let repo = committed_install();
    drop_the_script_in_a_commit(repo.path());
    let elsewhere = tempfile::tempdir().expect("tempdir");
    link_to_a_fifo(repo.path(), elsewhere.path());

    let line = next_line(&run_ok_within_the_limit(repo.path(), &["update"]));

    assert_eq!(line, git_add_line("tools"));
}

// ---- an unknown batch ----------------------------------------------------------------

/// A scratch repository with the JSON tracker installed.
fn json_tracker_install() -> TempDir {
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init", "--tracker", "json"]);
    repo
}

/// A batch record of the shape `backlog/batches.json` holds.
fn batch_record(number: u64) -> Value {
    json!({
        "number": number,
        "items": [],
        "summary": "A batch",
        "kickoff": "kickoff",
        "status": {"state": "done", "text": "done"},
    })
}

/// Writes `records` as the batch records of the file `relative` (`backlog/batches.json` or
/// `backlog/archive/batches.json`), in the form of the seeded file.
fn write_batches(repo: &Path, relative: &str, records: &[Value]) {
    let mut file = read_json(repo, "backlog/batches.json");
    file["batches"] = Value::Array(records.to_vec());
    write_json(repo, relative, &file);
}

/// An item of an items file, as the seeded file holds one, with `batch` and `see` where given.
fn item(id: &str, batch: Option<u64>, see: &[&str]) -> Value {
    let mut item = json!({
        "id": id,
        "type": "chore",
        "milestone": null,
        "status": "open",
        "title": format!("The item {id}."),
        "body": ["A body."],
    });
    if let Some(batch) = batch {
        item["batch"] = json!(batch);
    }
    if !see.is_empty() {
        item["see"] = json!(see);
    }
    item
}

/// Writes the items file `backlog/items/<section>.json` with `items`.
fn write_items(repo: &Path, section: &str, items: &[Value]) {
    write_json(
        repo,
        &format!("backlog/items/{section}.json"),
        &json!({
            "$schema": "../schema.json",
            "section": section,
            "heading": section,
            "title": format!("The items of {section}"),
            "spec": "",
            "items": items,
        }),
    );
}

/// Spec 20.2 and acceptance case 15: the item that the first-run recipe ticks, on a fresh JSON
/// tracker, names a batch that no record holds. `check-backlog` reports it and exits 1, where
/// `batch 1` already refuses the batch.
#[test]
fn check_backlog_reports_an_item_whose_batch_no_record_holds() {
    let repo = json_tracker_install();
    run_ok(repo.path(), &["set", "WI-001", "status=done", "batch=1"]);

    let checked = run(repo.path(), &["check-backlog"]);

    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(stdout(&checked), "");
    assert_eq!(
        stderr(&checked),
        format!(
            "{}\n",
            unknown_batch_line("backlog/items/general.json", "WI-001", 1)
        )
    );
    let batch = run(repo.path(), &["batch", "1"]);
    assert_eq!(batch.status.code(), Some(2));
    assert_eq!(stderr(&batch), "unknown batch \"1\"\n");
}

/// Spec 20.3: the same item passes when `backlog/batches.json` holds its batch.
#[test]
fn check_backlog_passes_an_item_whose_batch_a_record_holds() {
    let repo = json_tracker_install();
    run_ok(repo.path(), &["set", "WI-001", "status=done", "batch=1"]);
    write_batches(repo.path(), "backlog/batches.json", &[batch_record(1)]);

    let checked = run(repo.path(), &["check-backlog"]);

    assert_eq!(printed(&checked), "backlog: ok\n");
    assert_eq!(checked.status.code(), Some(0));
    assert_eq!(run(repo.path(), &["batch", "1"]).status.code(), Some(0));
}

/// Spec 20.3: the same item passes when only `backlog/archive/batches.json` holds its batch.
#[test]
fn check_backlog_passes_an_item_whose_batch_an_archived_record_holds() {
    let repo = json_tracker_install();
    run_ok(repo.path(), &["set", "WI-001", "status=done", "batch=1"]);
    write_batches(
        repo.path(),
        "backlog/archive/batches.json",
        &[batch_record(1)],
    );

    let checked = run(repo.path(), &["check-backlog"]);

    assert_eq!(printed(&checked), "backlog: ok\n");
    assert_eq!(checked.status.code(), Some(0));
}

/// The command reports each item that names an unknown batch, and no other item: an item with no
/// batch, an item of a known batch, and an item of an archived batch pass. The lines follow the
/// order of the items files and of the items, and an item's batch line follows its `see` finding.
/// The whole output is the pin, and a second run prints the same bytes.
#[test]
fn check_backlog_reports_each_item_with_an_unknown_batch_and_no_other() {
    let repo = json_tracker_install();
    write_batches(repo.path(), "backlog/batches.json", &[batch_record(2)]);
    write_batches(
        repo.path(),
        "backlog/archive/batches.json",
        &[batch_record(3)],
    );
    write_items(
        repo.path(),
        "general",
        &[
            item("WI-001", None, &[]),
            item("WI-002", Some(2), &[]),
            item("WI-003", Some(3), &[]),
            item("WI-004", Some(9), &["WI-999"]),
        ],
    );
    write_items(
        repo.path(),
        "later",
        &[item("WI-005", Some(8), &[]), item("WI-006", Some(9), &[])],
    );
    let expected = [
        "backlog/items/general.json WI-004: see \"WI-999\" does not exist".to_string(),
        unknown_batch_line("backlog/items/general.json", "WI-004", 9),
        unknown_batch_line("backlog/items/later.json", "WI-005", 8),
        unknown_batch_line("backlog/items/later.json", "WI-006", 9),
    ]
    .join("\n")
        + "\n";

    let first = run(repo.path(), &["check-backlog"]);
    let second = run(repo.path(), &["check-backlog"]);

    assert_eq!(first.status.code(), Some(1));
    assert_eq!(stdout(&first), "");
    assert_eq!(stderr(&first), expected);
    assert_eq!((first.stdout, first.stderr), (second.stdout, second.stderr));
}

// ---- the seeded text names the batch record --------------------------------------------

/// The part of `text` that comes before `status=done batch=<n>`. A text with no such tick fails the
/// test.
fn before_the_tick(text: &str) -> &str {
    let tick = text
        .find("status=done batch=<n>")
        .unwrap_or_else(|| panic!("the text names no tick: {text}"));
    &text[..tick]
}

/// The seeded item of the JSON tracker tells the reader to record the batch in
/// `backlog/batches.json` before the tick, and the steps as printed end with `check-backlog` at
/// exit 0 (`quality.user-facing-text-is-contract`).
#[test]
fn the_seeded_item_names_the_batch_record_before_the_tick_and_its_steps_end_green() {
    let repo = json_tracker_install();
    let seeded = read_json(repo.path(), "backlog/items/general.json");
    let text = seeded["items"][0]["body"]
        .as_array()
        .expect("a body")
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" ");

    assert!(
        before_the_tick(&text).contains("`backlog/batches.json`"),
        "{text}"
    );
    write_batches(repo.path(), "backlog/batches.json", &[batch_record(1)]);
    run_ok(repo.path(), &["set", "WI-001", "status=done", "batch=1"]);
    let checked = run(repo.path(), &["check-backlog"]);
    assert_eq!(printed(&checked), "backlog: ok\n");
    assert_eq!(checked.status.code(), Some(0));
}

/// The process pack tells the reader to create the batch record before the tick, on each tracker.
/// The entry names no file: a pack entry names nothing that a minimal install lacks
/// (`pack_texts.rs`).
#[test]
fn the_process_pack_entry_names_the_batch_record_before_the_tick() {
    let pack: Value = serde_json::from_str(&text_below(repo_root(), "template/packs/process.json"))
        .expect("the pack is JSON");
    let entry = pack["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == "process.backlog-drives-work")
        .expect("the entry");
    let text = entry["body"][0].as_str().expect("a body");

    let before = before_the_tick(text);
    assert!(
        before.contains("Create the batch record of <n> first"),
        "{text}"
    );
    assert!(
        before.contains("the batch file of the JSON tracker"),
        "{text}"
    );
    assert!(
        before.contains("a batch issue on the GitHub and Beads trackers"),
        "{text}"
    );
}

// ---- the spec ------------------------------------------------------------------------

/// The spec holds the lines that these tests pin: the two forms of the `next:` line, and the finding
/// of an unknown batch, with the order of its placeholders.
#[test]
fn the_lines_of_these_tests_are_in_the_spec() {
    let spec = flat(&text_below(repo_root(), SPEC));

    for form in [PLAIN_LINE, GIT_ADD_FORM, UNKNOWN_BATCH_FORM] {
        assert!(
            spec.contains(&format!("`{form}`")),
            "the spec lacks: {form}"
        );
    }
}
