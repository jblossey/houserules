//! An install with no tracker (batch 34, spec sections 3.3, 3.4, 3.8, and
//! 3.10): the lines and exit codes of spec 3.4's table, one test per cell,
//! the `get` routing that a knowledge read never needs a tracker for, the
//! refusals of `init --tracker` and `pack add process`, and the install
//! that `init` writes without a backlog. Each case runs the compiled binary
//! against a scratch git repository.

mod tracker_support;
mod tree_snapshot;

use std::fs;
use std::process::Command;

use serde_json::{Value, json};
use tracker_support::{
    ARCHIVE_LINE, AUDIT_LINE_ZERO, LINE_A, LINE_B, PACK_NEEDS_TRACKER, assert_output, commit_all,
    count_lines, git, hermetic_bd, houserules, install_with_backlog_dir,
    install_with_the_process_pack_and_a_backlog_dir, install_with_the_process_pack_and_no_tracker,
    install_without_backlog, read_json, run, run_ok, scratch_git_repo, stderr, stdout, write_json,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The refusal of `init --tracker github|beads` on an install that holds a
/// `backlog/` directory (spec 3.3).
const BACKLOG_HOLDS_JSON: &str = "backlog/ holds JSON tracker data; move it as the migrating-knowledge skill describes and remove backlog/, or run houserules init --tracker json";

/// The refusal of `init --tracker github` with no repository (spec 3.3).
const GITHUB_NEEDS_REPOSITORY: &str =
    "--tracker github needs a repository: pass --repo <owner>/<name>";

/// The commands that spec 3.4's second row sends to the tracker, with the
/// arguments that reach the tracker: each needs a tracker and prints line A
/// or line B with exit 2.
const TRACKER_COMMANDS: [&[&str]; 5] = [
    &["list"],
    &["batch", "1"],
    &["set", "WI-001", "status=done"],
    &["defer", "--title", "t", "--reason", "r", "--origin", "o"],
    &["get", "WI-001"],
];

/// A retired knowledge entry in a local topic file, for `archive` to move.
fn write_retired_entry(dir: &std::path::Path) {
    write_json(
        dir,
        "knowledge/fixture.json",
        &json!({
            "topic": "fixture",
            "title": "Fixture",
            "entries": [{
                "id": "fixture.old",
                "kind": "gotcha",
                "area": "docs",
                "summary": "A retired fixture entry.",
                "body": ["Fixture body."],
                "tags": [],
                "source": {"date": "2026-10-20", "by": "controller"},
                "status": "retired",
            }],
        }),
    );
}

/// Spec 3.4, `check-backlog`, no `backlog/` directory: line A on stdout,
/// exit 0.
#[test]
fn check_backlog_without_tracker_prints_line_a_exit_0() {
    let dir = install_without_backlog();
    let output = run(dir.path(), &["check-backlog"]);
    assert_output(&output, 0, &format!("{LINE_A}\n"), "");
}

/// Spec 3.4, `check-backlog`, a `backlog/` directory exists: line B on
/// stderr, exit 1.
#[test]
fn check_backlog_with_backlog_dir_and_no_tracker_prints_line_b_exit_1() {
    let dir = install_with_backlog_dir();
    let output = run(dir.path(), &["check-backlog"]);
    assert_output(&output, 1, "", &format!("{LINE_B}\n"));
}

/// Spec 3.4, second row, no `backlog/` directory: `list`, `batch`, `set`,
/// `defer`, and `get` of a backlog id print line A and exit 2.
#[test]
fn commands_that_need_a_tracker_print_line_a_without_a_backlog_dir_exit_2() {
    let dir = install_without_backlog();
    for args in TRACKER_COMMANDS {
        let output = run(dir.path(), args);
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(2), String::new(), format!("{LINE_A}\n")),
            "houserules {args:?}"
        );
    }
}

/// `list` with a `backlog/` directory and no tracker: line B, exit 2.
#[test]
fn list_with_backlog_dir_and_no_tracker_prints_line_b_exit_2() {
    let dir = install_with_backlog_dir();
    let output = run(dir.path(), &["list"]);
    assert_output(&output, 2, "", &format!("{LINE_B}\n"));
}

/// Spec 3.4, second row, a `backlog/` directory exists: the same five
/// commands print line B and exit 2.
#[test]
fn commands_that_need_a_tracker_print_line_b_with_a_backlog_dir_exit_2() {
    let dir = install_with_backlog_dir();
    for args in TRACKER_COMMANDS {
        let output = run(dir.path(), args);
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(2), String::new(), format!("{LINE_B}\n")),
            "houserules {args:?}"
        );
    }
}

/// Spec 3.4, `archive`, no `backlog/` directory: the knowledge half runs,
/// the archive line prints, exit 0.
#[test]
fn archive_without_tracker_moves_knowledge_and_prints_the_archive_line() {
    let dir = install_without_backlog();
    write_retired_entry(dir.path());
    let output = run(dir.path(), &["archive"]);
    assert_output(
        &output,
        0,
        &format!(
            "{ARCHIVE_LINE}\narchived fixture.old -> knowledge/archive/fixture.json\narchive: 1 moved\n"
        ),
        "",
    );
    let moved = read_json(dir.path(), "knowledge/archive/fixture.json");
    assert_eq!(moved["entries"][0]["id"], json!("fixture.old"));
    let kept = read_json(dir.path(), "knowledge/fixture.json");
    assert_eq!(kept["entries"], json!([]));
}

/// Spec 3.4, `archive`, a `backlog/` directory exists: it moves nothing,
/// prints line B, and exits 1.
#[test]
fn archive_with_backlog_dir_and_no_tracker_moves_nothing_and_prints_line_b_exit_1() {
    let dir = install_with_backlog_dir();
    write_retired_entry(dir.path());
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["archive"]);
    assert_output(&output, 1, "", &format!("{LINE_B}\n"));
    assert_same_tree(&before, &snapshot(dir.path()), "archive with line B");
}

/// A scratch install with two commits whose second changes a document, and
/// the first commit's sha, for `audit --base`.
fn audited_install(dir: &std::path::Path) -> String {
    let base = commit_all(dir, "chore: install the kit");
    fs::write(dir.join("docs/note.md"), "A note.\n").expect("write a document");
    commit_all(dir, "docs: add a note");
    base
}

/// Spec 3.4, `audit`, no `backlog/` directory: the audit line prints on
/// every run, `skipped 0` included, on stderr, so stdout stays the audit's
/// own JSON; the exit code is the other checks'.
#[test]
fn audit_without_tracker_prints_skipped_0() {
    let dir = install_without_backlog();
    let base = audited_install(dir.path());
    let output = run(dir.path(), &["audit", "--base", &base]);
    assert_eq!(stderr(&output), format!("{AUDIT_LINE_ZERO}\n"));
    assert_eq!(output.status.code(), Some(0));
    let result: Value = serde_json::from_str(&stdout(&output)).expect("stdout is the audit JSON");
    assert!(result.get("summary").is_some(), "got {result}");
}

/// Spec 3.4, `audit`, a `backlog/` directory exists: line B and exit 1; the
/// audit's own JSON still prints on stdout.
#[test]
fn audit_with_backlog_dir_and_no_tracker_prints_line_b_exit_1() {
    let dir = install_with_backlog_dir();
    let base = audited_install(dir.path());
    let output = run(dir.path(), &["audit", "--base", &base]);
    assert_eq!(stderr(&output), format!("{LINE_B}\n"));
    assert_eq!(output.status.code(), Some(1));
    serde_json::from_str::<Value>(&stdout(&output)).expect("stdout is the audit JSON");
}

/// Pin of unchanged behavior: a knowledge read never needs a tracker
/// (spec 3.8). `get process.tdd` prints the entry with no tracker, with and
/// without a `backlog/` directory.
#[test]
fn get_of_a_knowledge_id_never_needs_a_tracker() {
    for dir in [
        install_with_the_process_pack_and_no_tracker(),
        install_with_the_process_pack_and_a_backlog_dir(),
    ] {
        let output = run(dir.path(), &["get", "process.tdd"]);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        let entries: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
        assert_eq!(entries[0]["id"], json!("process.tdd"));
    }
}

/// Pin of unchanged behavior: an unknown id of a loaded topic prints the
/// knowledge base's not-found line, not line A (spec 3.4, live case 2).
#[test]
fn get_of_an_unknown_knowledge_id_prints_the_not_found_line_not_line_a() {
    let dir = install_with_the_process_pack_and_no_tracker();
    let output = run(dir.path(), &["get", "process.nonexistent"]);
    assert_output(&output, 2, "", "unknown id \"process.nonexistent\"\n");
}

/// Spec 3.8 and 3.4: an unresolved knowledge-shaped id whose pack the
/// binary carries and the install lacks prints the pack line, with no
/// tracker in place of line A or line B, exit 2.
#[test]
fn get_of_an_uninstalled_packs_id_prints_the_pack_line_instead_of_line_a_or_b() {
    let pack_line = "process.tdd: pack process is not installed; run houserules pack add process\n";
    for dir in [install_without_backlog(), install_with_backlog_dir()] {
        let output = run(dir.path(), &["get", "process.tdd"]);
        assert_output(&output, 2, "", pack_line);
    }
}

/// Spec 3.10: `pack add process` with no tracker refuses with the line of
/// the spec and changes nothing.
#[test]
fn pack_add_process_without_tracker_refuses() {
    let dir = install_without_backlog();
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["pack", "add", "process"]);
    assert_output(&output, 2, "", &format!("{PACK_NEEDS_TRACKER}\n"));
    assert_same_tree(&before, &snapshot(dir.path()), "pack add process");
}

/// Spec 3.10: with the JSON tracker installed, `pack add process` adds the
/// pack. Pin of unchanged behavior for an install that holds a backlog.
#[test]
fn pack_add_process_with_a_json_tracker_installs_the_pack() {
    let dir = install_without_backlog();
    tracker_support::declare_json_tracker(dir.path());
    fs::create_dir_all(dir.path().join("backlog")).expect("create backlog/");
    let output = run(dir.path(), &["pack", "add", "process"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(dir.path().join("knowledge/packs/process.json").is_file());
}

/// The `pattern` of the `backlogId` definition of the deliverables schema of
/// the install at `dir`.
fn backlog_id_pattern(dir: &std::path::Path) -> String {
    let schema = read_json(dir, ".claude/schemas/deliverables.json");
    schema["$defs"]["backlogId"]["pattern"]
        .as_str()
        .expect("the pattern is a string")
        .to_string()
}

/// Asserts that `init --tracker` printed the `wrote` line of the deliverables
/// schema once: the schema is written once, with the `backlogId` pattern of
/// the tracker that the same command installs. The CI workflow of a
/// GitHub-hosted repository is written once too.
fn assert_written_once(lines: &str) {
    for file in [
        ".claude/schemas/deliverables.json",
        ".github/workflows/knowledge.yml",
    ] {
        assert_eq!(
            count_lines(lines, &format!("wrote {file}")),
            1,
            "the `wrote {file}` line, once, in {lines}"
        );
    }
}

/// Spec 3.3 and 3.9: `init --tracker json` writes the deliverables schema
/// once, with the `backlogId` pattern of the JSON adapter for the id prefix of
/// the install (`WI` by default).
#[test]
fn init_tracker_json_writes_the_schema_once_with_the_pattern_of_the_adapter() {
    for (args, pattern) in [
        (
            vec!["init", "--tracker", "json"],
            "^(WI-[0-9]+|A-[0-9]+|PP-[0-9]+-[0-9]+)$",
        ),
        (
            vec!["init", "--tracker", "json", "--id-prefix", "HR"],
            "^(HR-[0-9]+|A-[0-9]+|PP-[0-9]+-[0-9]+)$",
        ),
    ] {
        let dir = scratch_git_repo();
        let output = run(dir.path(), &args);
        assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
        assert_eq!(
            count_lines(&stdout(&output), "wrote .claude/schemas/deliverables.json"),
            1,
            "{args:?}: {}",
            stdout(&output)
        );
        assert_eq!(backlog_id_pattern(dir.path()), pattern, "{args:?}");
    }
}

/// Spec 3.3 and 3.9: `init --tracker github` takes the repository from
/// `--repo`, else from the `origin` remote; it writes the `tracker` key with
/// the repository, the `backlogId` pattern of the adapter (`#<n>`), and the
/// seeded CI workflow, and it writes no `backlog/` file. A second run
/// changes nothing.
#[test]
fn init_tracker_github_writes_the_pattern_and_the_ci_workflow() {
    let remote = scratch_git_repo();
    git(
        remote.path(),
        &["remote", "add", "origin", "git@github.com:acme/widgets.git"],
    );
    let output = run(remote.path(), &["init", "--tracker", "github"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let lines = stdout(&output);
    for expected in [
        "wrote .claude/schemas/deliverables.json",
        "wrote .github/workflows/knowledge.yml",
        "tracker: github (acme/widgets)",
    ] {
        assert!(
            lines.lines().any(|line| line == expected),
            "no {expected:?} in {lines}"
        );
    }
    assert_written_once(&lines);
    let marker = read_json(remote.path(), ".houserules.json");
    assert_eq!(
        marker["tracker"],
        json!({"adapter": "github", "repo": "acme/widgets"})
    );
    assert_eq!(backlog_id_pattern(remote.path()), "^#[0-9]+$");
    assert!(
        remote
            .path()
            .join(".github/workflows/knowledge.yml")
            .is_file()
    );
    assert!(!remote.path().join("backlog").exists());
    let before = snapshot(remote.path());
    let again = run(remote.path(), &["init", "--tracker", "github"]);
    assert_eq!(again.status.code(), Some(0), "stderr: {}", stderr(&again));
    assert_same_tree(
        &before,
        &snapshot(remote.path()),
        "a second init --tracker github",
    );

    let flagged = scratch_git_repo();
    let output = run(
        flagged.path(),
        &["init", "--tracker", "github", "--repo", "acme/gadgets"],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let marker = read_json(flagged.path(), ".houserules.json");
    assert_eq!(
        marker["tracker"],
        json!({"adapter": "github", "repo": "acme/gadgets"})
    );
    assert_eq!(backlog_id_pattern(flagged.path()), "^#[0-9]+$");
}

/// Spec 3.3: with no `--repo` and no `origin` remote, `init --tracker
/// github` refuses, exit 2, and writes nothing.
#[test]
fn init_tracker_github_without_a_repository_refuses() {
    let dir = scratch_git_repo();
    let output = run(dir.path(), &["init", "--tracker", "github"]);
    assert_output(&output, 2, "", &format!("{GITHUB_NEEDS_REPOSITORY}\n"));
    assert!(!dir.path().join(".houserules.json").exists());
}

/// Spec 3.3: `init --tracker github` on an install that has a `backlog/`
/// directory refuses, exit 2, and changes nothing.
#[test]
fn init_tracker_github_with_backlog_dir_refuses() {
    let dir = install_with_backlog_dir();
    let before = snapshot(dir.path());
    let output = run(
        dir.path(),
        &["init", "--tracker", "github", "--repo", "acme/widgets"],
    );
    assert_output(&output, 2, "", &format!("{BACKLOG_HOLDS_JSON}\n"));
    assert_same_tree(&before, &snapshot(dir.path()), "init --tracker github");
}

/// Spec 3.3: the same refusal for `init --tracker beads`.
#[test]
fn init_tracker_beads_with_backlog_dir_refuses() {
    let dir = install_with_backlog_dir();
    let output = run(dir.path(), &["init", "--tracker", "beads"]);
    assert_output(&output, 2, "", &format!("{BACKLOG_HOLDS_JSON}\n"));
}

/// Runs `init --tracker beads` in `dir` with `bd` from `PATH` (or the program
/// that `HOUSERULES_BD` names), in the `bd` environment below `home`.
fn init_beads(dir: &std::path::Path, home: &std::path::Path) -> std::process::Output {
    let mut command = houserules();
    command
        .args(["init", "--tracker", "beads"])
        .arg("--dir")
        .arg(dir);
    hermetic_bd(&mut command, home);
    command.output().expect("run houserules init")
}

/// Spec 3.3 and 3.9: `init --tracker beads` reads the issue prefix from `bd`
/// and writes the `tracker` key, the `backlogId` pattern of that prefix, and
/// the seeded CI workflow of a GitHub-hosted repository. It writes no
/// `backlog/` file. A second run changes nothing of what the kit writes. The
/// prefix has a hyphen, which the pattern keeps. This test needs the `bd` that
/// `mise.toml` pins and fails without it.
#[test]
fn init_tracker_beads_writes_the_pattern_and_the_ci_workflow() {
    let dir = scratch_git_repo();
    let home = dir.path().join(".git/bd-home");
    let mut bd = Command::new(std::env::var_os("HOUSERULES_BD").unwrap_or_else(|| "bd".into()));
    bd.args([
        "init",
        "--non-interactive",
        "--prefix",
        "my-proj",
        "--quiet",
        "--skip-agents",
        "--skip-hooks",
    ])
    .current_dir(dir.path());
    hermetic_bd(&mut bd, &home);
    let created = bd
        .output()
        .expect("bd is not installed: run mise install, which pins it in mise.toml");
    assert!(created.status.success(), "bd init: {}", stderr(&created));
    // The remote comes after `bd init`: `bd init` runs `git ls-remote` against
    // an `origin` that it finds, and asks for the host key of a real server.
    git(
        dir.path(),
        &["remote", "add", "origin", "git@github.com:acme/widgets.git"],
    );

    let output = init_beads(dir.path(), &home);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let lines = stdout(&output);
    for expected in [
        "wrote .claude/schemas/deliverables.json",
        "wrote .github/workflows/knowledge.yml",
        "tracker: beads",
    ] {
        assert!(
            lines.lines().any(|line| line == expected),
            "no {expected:?} in {lines}"
        );
    }
    assert_written_once(&lines);
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "beads"}));
    assert_eq!(
        backlog_id_pattern(dir.path()),
        "^my-proj-[0-9a-z]+(\\.[0-9]+)*$"
    );
    assert!(dir.path().join(".github/workflows/knowledge.yml").is_file());
    assert!(!dir.path().join("backlog").exists());

    let kit_files = |dir: &std::path::Path| {
        let mut tree = snapshot(dir);
        tree.retain(|path, _| !path.starts_with(".beads"));
        tree
    };
    let before = kit_files(dir.path());
    let again = init_beads(dir.path(), &home);
    assert_eq!(again.status.code(), Some(0), "stderr: {}", stderr(&again));
    assert_same_tree(
        &before,
        &kit_files(dir.path()),
        "a second init --tracker beads",
    );
}

/// The text that `houserules <args> --help` prints.
fn help_of(args: &[&str]) -> String {
    let output = houserules()
        .args(args)
        .arg("--help")
        .output()
        .expect("run houserules --help");
    assert!(output.status.success(), "{}", stderr(&output));
    stdout(&output)
}

/// `get --help` names the id shape of each of the three trackers, because each
/// tracker resolves its own shapes (spec 3.8).
#[test]
fn get_help_names_the_id_shapes_of_the_three_trackers() {
    let help = help_of(&["get"]);
    for shape in [
        "for the JSON tracker an item `<idPrefix>-\\d{3}`",
        "for the GitHub tracker an issue `#<n>`",
        "for the Beads tracker an issue `<prefix>-<hash>`",
    ] {
        assert!(help.contains(shape), "no {shape:?} in {help}");
    }
}

/// `check-backlog --help` names what the gate checks on each of the three
/// trackers, because each adapter adds its own checks to the contract's
/// (spec 3.5).
#[test]
fn check_backlog_help_names_the_checks_of_the_three_trackers() {
    let help = help_of(&["check-backlog"]);
    for check in [
        "on every tracker the contract's checks",
        "for the JSON tracker",
        "for the GitHub tracker the `houserules:` labels",
        "for the Beads tracker the custom types and the `partial` status",
    ] {
        assert!(help.contains(check), "no {check:?} in {help}");
    }
}

/// `init --help` tells the person who picks `--tracker beads` that the Beads
/// store must exist, and which command makes it (spec 3.3).
#[test]
fn init_help_names_the_bd_init_step_of_the_beads_tracker() {
    let help = help_of(&["init"]);
    for step in ["`bd init`", "`bd bootstrap`"] {
        assert!(help.contains(step), "no {step:?} in {help}");
    }
}

/// `--repo` belongs to the GitHub adapter: with another adapter it is a
/// usage error, exit 2, and nothing is written.
#[test]
fn init_repo_flag_with_another_adapter_is_refused() {
    let dir = scratch_git_repo();
    let output = run(
        dir.path(),
        &["init", "--tracker", "json", "--repo", "acme/widgets"],
    );
    assert_output(&output, 2, "", "--repo applies to --tracker github only\n");
    assert!(!dir.path().join(".houserules.json").exists());
}

/// `--repo` must name `<owner>/<name>`.
#[test]
fn init_repo_flag_that_is_no_owner_and_name_is_refused() {
    let dir = scratch_git_repo();
    let output = run(
        dir.path(),
        &["init", "--tracker", "github", "--repo", "widgets"],
    );
    assert_output(
        &output,
        2,
        "",
        "--repo \"widgets\" must be <owner>/<name>\n",
    );
    assert!(!dir.path().join(".houserules.json").exists());
}

/// The install that `init` writes holds no `backlog/` directory: backlog
/// files belong to the JSON tracker (spec 4.2, `trackerFiles`).
#[test]
fn a_plain_init_writes_no_backlog_file() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    assert!(!dir.path().join("backlog").exists());
    let marker = read_json(dir.path(), ".houserules.json");
    assert!(marker.get("tracker").is_none(), "got {marker}");
}
