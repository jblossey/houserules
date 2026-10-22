//! The install schema `houserules/install@2.2.0` (spec 7, 10; case 10), the
//! step of `update` that restamps a 2.1.0 marker, and the keys `runners` and
//! `workspaces`, whose values every command that writes the marker keeps.
//! Each case runs the compiled binary against a scratch install that
//! `houserules init` made.

mod locked_file;
mod tracker_support;
mod tree_snapshot;

use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{count_lines, edit_marker, read_json, run, run_ok, scratch_git_repo, stdout};
use tree_snapshot::{assert_same_tree, snapshot};

/// The install schema of the batch before.
const OLD_SCHEMA: &str = "houserules/install@2.1.0";

/// The install schema that `update` stamps.
const NEW_SCHEMA: &str = "houserules/install@2.2.0";

/// The line `update` prints once for the restamp (spec 10).
const RESTAMP_LINE: &str = "wrote .houserules.json (houserules/install@2.2.0)";

/// A scratch install that `init` made, with its marker set back to the install
/// schema 2.1.0: the install of a project that last ran `update` in batch 34.
fn install_at_2_1_0() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    edit_marker(dir.path(), |marker| {
        marker.insert("schema".to_string(), json!(OLD_SCHEMA));
    });
    dir
}

/// The keys of the marker of the install at `dir`, in file order.
fn marker_keys(dir: &Path) -> Vec<String> {
    read_json(dir, ".houserules.json")
        .as_object()
        .expect("the marker is an object")
        .keys()
        .cloned()
        .collect()
}

/// A `runners` object and a `workspaces` value that the project wrote.
fn project_keys() -> (Value, Value) {
    (
        json!({
            "semgrep": {
                "command": ["semgrep", "scan", "--sarif"],
                "parser": "sarif",
                "install": "pip install semgrep==1.2.3",
            },
        }),
        json!(".superpowers/sdd"),
    )
}

/// Case 10: `update` restamps a 2.1.0 marker to 2.2.0 and prints the line once;
/// the `schema` key keeps its place; a second `update` prints no marker line
/// and changes nothing.
#[test]
fn update_restamps_2_1_0_to_2_2_0_once() {
    let dir = install_at_2_1_0();
    let keys = marker_keys(dir.path());

    let first = run_ok(dir.path(), &["update"]);

    assert_eq!(count_lines(&stdout(&first), RESTAMP_LINE), 1, "{first:?}");
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["schema"],
        json!(NEW_SCHEMA)
    );
    assert_eq!(marker_keys(dir.path()), keys);

    let after_first = snapshot(dir.path());
    let second = run_ok(dir.path(), &["update"]);

    assert!(
        !stdout(&second)
            .lines()
            .any(|line| line.starts_with("wrote .houserules.json")),
        "{second:?}"
    );
    assert_same_tree(&after_first, &snapshot(dir.path()), "the second update");
}

/// A marker that already holds 2.2.0 gets no line: the step tests its own
/// state.
#[test]
fn update_prints_no_restamp_line_for_a_marker_at_2_2_0() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);

    let output = run_ok(dir.path(), &["update"]);

    assert_eq!(count_lines(&stdout(&output), RESTAMP_LINE), 0, "{output:?}");
}

/// `houserules.migrations-finish-after-a-stop`: the write of the marker stops
/// the run, so the marker keeps 2.1.0 and the run prints no restamp line; the
/// next `update` restamps and leaves the tree of a run that no fault stopped.
#[test]
fn an_update_that_stops_at_the_marker_finishes_the_restamp_at_the_next_update() {
    let control = install_at_2_1_0();
    run_ok(control.path(), &["update"]);

    let dir = install_at_2_1_0();
    let Some(lock) = locked_file::lock(&dir.path().join(".houserules.json")) else {
        return;
    };
    let failed = run(dir.path(), &["update"]);
    drop(lock);
    assert_eq!(failed.status.code(), Some(2), "{failed:?}");
    assert_eq!(count_lines(&stdout(&failed), RESTAMP_LINE), 0, "{failed:?}");
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["schema"],
        json!(OLD_SCHEMA)
    );

    let rerun = run_ok(dir.path(), &["update"]);

    assert_eq!(count_lines(&stdout(&rerun), RESTAMP_LINE), 1, "{rerun:?}");
    assert_same_tree(
        &snapshot(control.path()),
        &snapshot(dir.path()),
        "the update that stopped at the marker",
    );
}

/// Pin of unchanged behavior: the marker is a map, so `init`, `init --workflow`,
/// `update`, and `pack add` keep `runners` and `workspaces` as found, in their
/// place, a malformed value included (the gate `check-knowledge` names a
/// defect; no writer refuses it). This passes at the base commit and guards
/// the keys against a writer that rebuilds the marker.
#[test]
fn init_and_update_keep_runners_and_workspaces() {
    let (runners, workspaces) = project_keys();
    for (runners, workspaces) in [
        (runners, workspaces),
        (json!(["not", "an", "object"]), json!("/an/absolute/path")),
    ] {
        let dir = scratch_git_repo();
        run_ok(dir.path(), &["init"]);
        edit_marker(dir.path(), |marker| {
            marker.insert("runners".to_string(), runners.clone());
            marker.insert("workspaces".to_string(), workspaces.clone());
        });

        for command in [
            vec!["init"],
            vec!["init", "--tracker", "json", "--workflow", "reference"],
            vec!["update"],
            vec!["pack", "add", "quality"],
        ] {
            run_ok(dir.path(), &command);
            let marker = read_json(dir.path(), ".houserules.json");
            assert_eq!(marker["runners"], runners, "{command:?}");
            assert_eq!(marker["workspaces"], workspaces, "{command:?}");
            let keys = marker_keys(dir.path());
            let position = |key: &str| keys.iter().position(|found| found == key);
            assert!(position("runners") < position("workspaces"), "{command:?}");
        }
    }
}
