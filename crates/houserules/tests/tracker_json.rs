//! The JSON tracker (batch 34, spec sections 3.1 to 3.5, 3.8, 3.9, and 4.2):
//! `init --tracker json`, `defer` in both forms, `list --deferred`, the
//! routing of `get` between the knowledge base and the tracker, the line of
//! an unreachable JSON tracker, the `trackerFiles` class, and `update` of an
//! install with and without the JSON tracker. Each case runs the compiled
//! binary against a scratch git repository.
//!
//! The JSON adapter prints what the 1.4.0 backlog commands printed. The
//! goldens of `backlog_parity.rs` pin that equivalence on the frozen
//! worktree, whose marker now carries the tracker key; no second fixture set
//! exists.

mod tracker_support;
mod tree_snapshot;

use std::fs;

use serde_json::{Value, json};
use tracker_support::{
    JSON_UNREACHABLE, assert_output, commit_all, edit_marker, install_without_backlog,
    mini_install, read_json, run, run_ok, scratch_git_repo, stderr, stdout, write_json,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The five data files that `init --tracker json` writes, and the schema.
const TRACKER_FILES: [&str; 6] = [
    "backlog/batches.json",
    "backlog/amendments.json",
    "backlog/decisions.json",
    "backlog/parked.json",
    "backlog/items/general.json",
    "backlog/schema.json",
];

/// The `defer` arguments that file a deferral in the title form.
const DEFER_TITLE: [&str; 6] = [
    "defer",
    "--title",
    "Cache the lookups",
    "--reason",
    "Needs a design.",
    "--origin",
];

/// The `backlogId` pattern of the deliverables schema for `prefix`.
fn json_pattern(prefix: &str) -> String {
    format!("^({prefix}-[0-9]+|A-[0-9]+|PP-[0-9]+-[0-9]+)$")
}

/// The `backlogId` pattern that `dir`'s deliverables schema holds.
fn backlog_id_pattern(dir: &std::path::Path) -> String {
    let schema = read_json(dir, ".claude/schemas/deliverables.json");
    schema["$defs"]["backlogId"]["pattern"]
        .as_str()
        .expect("the pattern is a string")
        .to_string()
}

/// Runs `defer` in the title form with `extra` arguments after the origin.
fn defer_title(dir: &std::path::Path, origin: &str, extra: &[&str]) -> std::process::Output {
    let mut args: Vec<&str> = DEFER_TITLE.to_vec();
    args.push(origin);
    args.extend_from_slice(extra);
    run(dir, &args)
}

/// Writes a `deferral` object onto the item `id` of the mini backlog by hand,
/// the way a project that edits `backlog/items/misc.json` would.
fn store_deferral(dir: &std::path::Path, id: &str) {
    let mut misc = read_json(dir, "backlog/items/misc.json");
    let items = misc["items"].as_array_mut().expect("items");
    let item = items
        .iter_mut()
        .find(|item| item["id"] == json!(id))
        .expect("the item exists");
    item["deferral"] = json!({"reason": "r", "origin": "o", "files": []});
    write_json(dir, "backlog/items/misc.json", &misc);
}

/// Spec 3.3 and 4.2: `init --tracker json` writes the five data files, the
/// schema, and the `backlogId` pattern of the adapter; the backlog it
/// seeds passes `check-backlog`.
#[test]
fn init_tracker_json_writes_the_five_data_files_the_schema_and_the_pattern() {
    let dir = scratch_git_repo();
    let output = run_ok(dir.path(), &["init", "--tracker", "json"]);
    for file in TRACKER_FILES {
        assert!(dir.path().join(file).is_file(), "{file} is missing");
    }
    assert_eq!(backlog_id_pattern(dir.path()), json_pattern("WI"));
    assert!(
        stdout(&output).lines().any(|line| line == "tracker: json"),
        "got {}",
        stdout(&output)
    );
    let check = run_ok(dir.path(), &["check-backlog"]);
    assert_output(&check, 0, "backlog: ok\n", "");
}

/// Spec 3.3 and 3.7: the marker holds the tracker key, written last, and
/// the install schema that `init` stamps is `houserules/install@2.3.0`.
#[test]
fn init_tracker_json_stamps_the_tracker_key_and_the_install_schema_2_3_0() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "json"}));
    assert_eq!(marker["schema"], json!("houserules/install@2.3.0"));
    let keys: Vec<&String> = marker.as_object().expect("an object").keys().collect();
    assert_eq!(keys.last().map(|key| key.as_str()), Some("tracker"));
}

/// Spec 3.9: the pattern follows the stamped prefix, and the seeded items
/// carry it.
#[test]
fn init_tracker_json_with_an_id_prefix_writes_the_prefixed_pattern() {
    let dir = scratch_git_repo();
    run_ok(
        dir.path(),
        &["init", "--tracker", "json", "--id-prefix", "HR"],
    );
    assert_eq!(backlog_id_pattern(dir.path()), json_pattern("HR"));
    let general = read_json(dir.path(), "backlog/items/general.json");
    assert_eq!(general["items"][0]["id"], json!("HR-001"));
    run_ok(dir.path(), &["check-backlog"]);
}

/// Spec 3.3: `init --tracker json` on an install that has the same tracker
/// does nothing for the tracker, exit 0; the tree keeps its bytes.
#[test]
fn init_tracker_same_adapter_is_a_no_op() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["init", "--tracker", "json"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_same_tree(
        &before,
        &snapshot(dir.path()),
        "a second init --tracker json",
    );
}

/// Spec 3.3: on an install with the JSON tracker, `init --tracker github`
/// refuses with the line of the spec, exit 2, and changes nothing.
#[test]
fn init_tracker_another_adapter_refuses_and_changes_nothing() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    let before = snapshot(dir.path());
    let output = run(
        dir.path(),
        &["init", "--tracker", "github", "--repo", "acme/widgets"],
    );
    assert_output(
        &output,
        2,
        "",
        "this install already has the json tracker; houserules changes no tracker\n",
    );
    assert_same_tree(&before, &snapshot(dir.path()), "init --tracker github");
}

/// Spec 3.3: `init --tracker json` keeps the backlog files that exist,
/// whatever they hold, and writes the tracker key: an install that has
/// `backlog/` and no key adopts the JSON tracker with its data intact.
#[test]
fn init_tracker_json_keeps_the_backlog_files_of_an_install_that_has_them() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    let mut batches = read_json(dir.path(), "backlog/batches.json");
    batches["intro"] = json!(["The project's own intro."]);
    write_json(dir.path(), "backlog/batches.json", &batches);
    edit_marker(dir.path(), |marker| {
        marker.shift_remove("tracker");
    });
    let output = run(dir.path(), &["check-backlog"]);
    assert_eq!(output.status.code(), Some(1), "line B before the adoption");

    run_ok(dir.path(), &["init", "--tracker", "json"]);

    assert_eq!(
        read_json(dir.path(), "backlog/batches.json")["intro"],
        json!(["The project's own intro."])
    );
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "json"}));
}

/// Spec 3.3 and 3.4: the remedy that line A prints works on an install whose
/// repository tracks another `.houserules.json` below the install root (a
/// vendored project, a test fixture). The install is no new root, so `init
/// --tracker json` runs no nesting check, as `update` runs none: it exits 0
/// and writes the tracker, the six files, and the pattern.
#[test]
fn init_tracker_json_on_an_install_that_tracks_a_nested_marker_installs_the_tracker() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    write_json(dir.path(), "vendor/lib/.houserules.json", &json!({}));
    commit_all(dir.path(), "vendor a project that has its own install");
    let line_a = run(dir.path(), &["list"]);
    assert_eq!(line_a.status.code(), Some(2), "line A before the remedy");

    let output = run(dir.path(), &["init", "--tracker", "json"]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert_eq!(stderr(&output), "");
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["tracker"],
        json!({"adapter": "json"})
    );
    for file in TRACKER_FILES {
        assert!(dir.path().join(file).is_file(), "{file} is missing");
    }
    assert_eq!(backlog_id_pattern(dir.path()), json_pattern("WI"));
    let check = run_ok(dir.path(), &["check-backlog"]);
    assert_output(&check, 0, "backlog: ok\n", "");
}

/// `houserules.migrations-finish-after-a-stop`: the `tracker` key is the last
/// write of `init --tracker json`. A run that stopped after the tracker files
/// and before the pattern and the key leaves a tree that the next run
/// finishes: the key is written, the pattern is rewritten, and the data files
/// keep their bytes.
#[test]
fn init_tracker_json_finishes_a_run_that_stopped_before_the_pattern_and_the_key() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    let kit_schema = std::fs::read_to_string(
        tracker_support::repo_root().join("template/.claude/schemas/deliverables.json"),
    )
    .expect("read the template schema");
    fs::write(
        dir.path().join(".claude/schemas/deliverables.json"),
        kit_schema,
    )
    .expect("restore the pattern the stopped run had not written");
    edit_marker(dir.path(), |marker| {
        marker.shift_remove("tracker");
    });
    let batches = fs::read(dir.path().join("backlog/batches.json")).expect("read batches.json");

    run_ok(dir.path(), &["init", "--tracker", "json"]);

    assert_eq!(
        read_json(dir.path(), ".houserules.json")["tracker"],
        json!({"adapter": "json"})
    );
    assert_eq!(backlog_id_pattern(dir.path()), json_pattern("WI"));
    assert_eq!(
        fs::read(dir.path().join("backlog/batches.json")).expect("read batches.json"),
        batches
    );
    run_ok(dir.path(), &["check-backlog"]);
}

/// Spec 4.2: `houserules files` lists the five data files and the schema
/// in the class `trackerFiles`, and in no other class.
#[test]
fn files_lists_the_tracker_files_in_their_own_class() {
    let output = tracker_support::houserules()
        .arg("files")
        .output()
        .expect("run files");
    assert_eq!(output.status.code(), Some(0));
    let files: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(files["trackerFiles"], json!(TRACKER_FILES));
    for class in ["kitOwned", "seedOnce", "d3Files", "packCopies"] {
        let paths = files[class].as_array().expect("an array");
        let stray: Vec<&Value> = paths
            .iter()
            .filter(|path| {
                path.as_str()
                    .is_some_and(|path| TRACKER_FILES.contains(&path))
            })
            .collect();
        assert!(stray.is_empty(), "{class} lists {stray:?}");
    }
}

/// Spec 4.2: `update` of an install with no tracker writes no `backlog/`
/// file; the data files belong to the JSON tracker.
#[test]
fn update_without_a_json_tracker_writes_no_backlog_file() {
    let dir = install_without_backlog();
    run_ok(dir.path(), &["update"]);
    assert!(!dir.path().join("backlog").exists());
}

/// Spec 4.2: `update` of an install with the JSON tracker restores a data
/// file and the schema that the project deleted.
#[test]
fn update_with_a_json_tracker_restores_a_deleted_data_file_and_the_schema() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    fs::remove_file(dir.path().join("backlog/batches.json")).expect("delete batches.json");
    fs::remove_file(dir.path().join("backlog/schema.json")).expect("delete schema.json");
    let output = run_ok(dir.path(), &["update"]);
    for file in ["backlog/batches.json", "backlog/schema.json"] {
        assert!(dir.path().join(file).is_file(), "{file} is missing");
        assert!(
            stdout(&output)
                .lines()
                .any(|line| line == format!("wrote {file}")),
            "got {}",
            stdout(&output)
        );
    }
    run_ok(dir.path(), &["check-backlog"]);
}

/// An install with the JSON tracker and no `backlog/` directory is an
/// unreachable tracker (spec 3.4); `update` does not hide it by writing a
/// fresh backlog.
#[test]
fn update_with_a_json_tracker_does_not_recreate_a_missing_backlog_directory() {
    let dir = install_without_backlog();
    tracker_support::declare_json_tracker(dir.path());
    run_ok(dir.path(), &["update"]);
    assert!(!dir.path().join("backlog").exists());
}

/// Spec 3.4: each command that needs an unreachable JSON tracker prints the
/// line of the spec; the gate exits 1, the others exit 2, and nothing is
/// written.
#[test]
fn an_unreachable_json_tracker_prints_its_line_gates_exit_1_and_the_rest_exit_2() {
    let dir = mini_install();
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    let before = snapshot(dir.path());
    let line = format!("{JSON_UNREACHABLE}\n");
    let cells: [(&[&str], i32); 7] = [
        (&["check-backlog"], 1),
        (&["list"], 2),
        (&["batch", "1"], 2),
        (&["set", "HR-901", "status=done"], 2),
        (&["get", "HR-901"], 2),
        (&["archive"], 2),
        (
            &["defer", "--title", "t", "--reason", "r", "--origin", "o"],
            2,
        ),
    ];
    for (args, code) in cells {
        let output = run(dir.path(), args);
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(code), String::new(), line.clone()),
            "houserules {args:?}"
        );
    }
    assert_same_tree(&before, &snapshot(dir.path()), "an unreachable tracker");
}

/// Spec 3.8: `get` of a knowledge id never needs the tracker, so an
/// unreachable JSON tracker leaves it alone. Pin of unchanged behavior.
#[test]
fn get_of_a_knowledge_id_with_an_unreachable_json_tracker_prints_the_entry() {
    let dir = mini_install();
    fs::remove_dir_all(dir.path().join("backlog")).expect("remove backlog/");
    let output = run(dir.path(), &["get", "mini.build-cache"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let entries: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(entries[0]["id"], json!("mini.build-cache"));
}

/// Spec 3.8 and 3.4: an unresolved id of a pack the binary carries and the
/// install lacks prints the tracker's not-found line, then the pack line.
#[test]
fn get_of_an_uninstalled_packs_id_with_a_tracker_prints_the_not_found_line_then_the_pack_line() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    let output = run(dir.path(), &["get", "process.tdd"]);
    assert_output(
        &output,
        2,
        "",
        "unknown id \"process.tdd\"\nprocess.tdd: pack process is not installed; run houserules pack add process\n",
    );
}

/// Pin of unchanged behavior: `get` of an unknown backlog id prints the
/// not-found line, exit 2, with the JSON tracker installed.
#[test]
fn get_of_an_unknown_backlog_id_prints_the_not_found_line() {
    let dir = mini_install();
    let output = run(dir.path(), &["get", "HR-999"]);
    assert_output(&output, 2, "", "unknown id \"HR-999\"\n");
}

/// Spec 3.1: `defer --title` files a new item in the named section, prints
/// its id on one line, and the item holds the fields of the spec in the
/// order of the schema; the backlog still passes `check-backlog`.
#[test]
fn defer_title_files_an_item_in_the_named_section_and_prints_its_id() {
    let dir = mini_install();
    let output = defer_title(
        dir.path(),
        "batch 34 task 2",
        &[
            "--file",
            "src/a.rs",
            "--file",
            "src/b.rs",
            "--section",
            "misc",
        ],
    );
    assert_output(&output, 0, "HR-903\n", "");
    let misc = read_json(dir.path(), "backlog/items/misc.json");
    let filed = misc["items"][2].clone();
    assert_eq!(
        serde_json::to_string(&filed).expect("serialize"),
        concat!(
            r#"{"id":"HR-903","type":"chore","milestone":null,"status":"open","#,
            r#""title":"Cache the lookups","body":["Needs a design."],"batch":null,"#,
            r#""deferral":{"reason":"Needs a design.","origin":"batch 34 task 2","#,
            r#""files":["src/a.rs","src/b.rs"]}}"#
        )
    );
    let check = run_ok(dir.path(), &["check-backlog"]);
    assert_output(&check, 0, "backlog: ok\n", "");
}

/// Spec 3.1: with no `--section` the item goes to `backlog/items/general.json`,
/// and a new items file gets `section`, `heading`, `title`, and `spec: null`.
#[test]
fn defer_title_creates_the_general_items_file_with_the_fields_of_the_spec() {
    let dir = mini_install();
    let output = defer_title(dir.path(), "batch 34 task 2", &[]);
    assert_output(&output, 0, "HR-903\n", "");
    let general = read_json(dir.path(), "backlog/items/general.json");
    let keys: Vec<&String> = general.as_object().expect("an object").keys().collect();
    assert_eq!(keys, ["section", "heading", "title", "spec", "items"]);
    assert_eq!(general["section"], json!("general"));
    assert_eq!(general["heading"], json!("general"));
    assert_eq!(general["title"], json!("general"));
    assert_eq!(general["spec"], Value::Null);
    assert_eq!(general["items"][0]["id"], json!("HR-903"));
    assert_eq!(general["items"][0]["deferral"]["files"], json!([]));
    run_ok(dir.path(), &["check-backlog"]);
}

/// Spec 3.1: the new id is one above the highest number over the items and
/// `backlog/archive/`.
#[test]
fn defer_numbers_above_the_highest_id_of_the_items_and_the_archive() {
    let dir = mini_install();
    write_json(
        dir.path(),
        "backlog/archive/misc.json",
        &json!({
            "section": "misc",
            "heading": "Misc",
            "title": "Miscellaneous mini items",
            "spec": "",
            "items": [{
                "id": "HR-950",
                "type": "chore",
                "milestone": null,
                "status": "done",
                "title": "Archived.",
                "body": ["Archived."],
                "batch": 1,
            }],
        }),
    );
    let output = defer_title(dir.path(), "batch 34 task 2", &["--section", "misc"]);
    assert_output(&output, 0, "HR-951\n", "");
}

/// Spec 3.1: a title of more than 120 characters is refused, counted in
/// characters, with exit 2 and no write.
#[test]
fn defer_refuses_a_title_over_120_characters_and_writes_nothing() {
    let dir = mini_install();
    let before = snapshot(dir.path());
    let long = "ä".repeat(121);
    let output = run(
        dir.path(),
        &["defer", "--title", &long, "--reason", "r", "--origin", "o"],
    );
    assert_output(
        &output,
        2,
        "",
        "defer: the title is 121 characters; the limit is 120\n",
    );
    assert_same_tree(&before, &snapshot(dir.path()), "a refused title");
    let exact = "ä".repeat(120);
    let output = run(
        dir.path(),
        &["defer", "--title", &exact, "--reason", "r", "--origin", "o"],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
}

/// Spec 3.1: `defer --item` marks an existing item as a deferral, prints
/// its id, and leaves every other item as it was; the deferral goes after
/// the item's own fields.
#[test]
fn defer_item_marks_an_existing_item_and_prints_its_id() {
    let dir = mini_install();
    let output = run(
        dir.path(),
        &[
            "defer",
            "--item",
            "HR-901",
            "--reason",
            "Waits for T2.",
            "--origin",
            "batch 25",
            "--file",
            "src/a.rs",
        ],
    );
    assert_output(&output, 0, "HR-901\n", "");
    let misc = read_json(dir.path(), "backlog/items/misc.json");
    assert_eq!(
        misc["items"][0]["deferral"],
        json!({"reason": "Waits for T2.", "origin": "batch 25", "files": ["src/a.rs"]})
    );
    let keys: Vec<&String> = misc["items"][0]
        .as_object()
        .expect("an object")
        .keys()
        .collect();
    assert_eq!(keys.last().map(|key| key.as_str()), Some("deferral"));
    assert!(misc["items"][1].get("deferral").is_none());
    run_ok(dir.path(), &["check-backlog"]);
}

/// `defer --item` of an id that no item has is a named usage error.
#[test]
fn defer_item_of_an_unknown_item_is_refused() {
    let dir = mini_install();
    let before = snapshot(dir.path());
    let output = run(
        dir.path(),
        &[
            "defer", "--item", "HR-404", "--reason", "r", "--origin", "o",
        ],
    );
    assert_output(&output, 2, "", "unknown item \"HR-404\"\n");
    assert_same_tree(&before, &snapshot(dir.path()), "an unknown item");
}

/// Review focus 3: `--file` is stored and compared with forward slashes,
/// whatever the platform wrote.
#[test]
fn defer_stores_files_with_forward_slashes() {
    let dir = mini_install();
    let output = defer_title(
        dir.path(),
        "batch 34 task 2",
        &[
            "--file",
            "src\\lib\\a.rs",
            "--file",
            "docs/b.md",
            "--section",
            "misc",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let misc = read_json(dir.path(), "backlog/items/misc.json");
    assert_eq!(
        misc["items"][2]["deferral"]["files"],
        json!(["src/lib/a.rs", "docs/b.md"])
    );
    let output = run(
        dir.path(),
        &[
            "defer", "--item", "HR-901", "--reason", "r", "--origin", "o", "--file", "x\\y.rs",
        ],
    );
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let misc = read_json(dir.path(), "backlog/items/misc.json");
    assert_eq!(misc["items"][0]["deferral"]["files"], json!(["x/y.rs"]));
}

/// `defer` needs a reason and an origin that say something, and exactly one
/// of `--title` and `--item`: each fault exits 2 and writes nothing.
#[test]
fn defer_refuses_missing_and_conflicting_arguments_and_writes_nothing() {
    let valid = mini_install();
    run_ok(
        valid.path(),
        &["defer", "--title", "t", "--reason", "r", "--origin", "o"],
    );
    let dir = mini_install();
    let before = snapshot(dir.path());
    let cases: [&[&str]; 6] = [
        &["defer", "--reason", "r", "--origin", "o"],
        &[
            "defer", "--title", "t", "--item", "HR-901", "--reason", "r", "--origin", "o",
        ],
        &["defer", "--title", "t", "--origin", "o"],
        &["defer", "--title", "t", "--reason", "r"],
        &["defer", "--title", "t", "--reason", "", "--origin", "o"],
        &[
            "defer",
            "--item",
            "HR-901",
            "--reason",
            "r",
            "--origin",
            "o",
            "--section",
            "misc",
        ],
    ];
    for args in cases {
        let output = run(dir.path(), args);
        assert_eq!(output.status.code(), Some(2), "houserules {args:?}");
        assert_eq!(stdout(&output), "", "houserules {args:?}");
        assert!(!stderr(&output).is_empty(), "houserules {args:?}");
    }
    assert_same_tree(&before, &snapshot(dir.path()), "refused defer calls");
}

/// Spec 3.2: `list --deferred` lists the items that carry a deferral, each
/// row with the `deferral` object after the five fields of a row; the
/// other filters combine with it.
#[test]
fn list_deferred_rows_carry_the_deferral_object() {
    let dir = mini_install();
    run_ok(
        dir.path(),
        &[
            "defer",
            "--item",
            "HR-901",
            "--reason",
            "Waits for T2.",
            "--origin",
            "batch 25",
            "--file",
            "src/a.rs",
        ],
    );
    run_ok(
        dir.path(),
        &[
            "defer",
            "--item",
            "HR-902",
            "--reason",
            "Done already.",
            "--origin",
            "batch 25",
        ],
    );
    let output = run_ok(dir.path(), &["list", "--deferred"]);
    let rows: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(
        rows,
        json!([
            {
                "id": "HR-901", "status": "open", "milestone": null, "batch": null,
                "title": "Write the mini-tools smoke test",
                "deferral": {"reason": "Waits for T2.", "origin": "batch 25", "files": ["src/a.rs"]},
            },
            {
                "id": "HR-902", "status": "done", "milestone": null, "batch": 1,
                "title": "Ship the mini-tools smoke test",
                "deferral": {"reason": "Done already.", "origin": "batch 25", "files": []},
            },
        ])
    );
    let keys: Vec<&String> = rows[0].as_object().expect("an object").keys().collect();
    assert_eq!(
        keys,
        ["id", "status", "milestone", "batch", "title", "deferral"]
    );
    let open = run_ok(dir.path(), &["list", "--deferred", "--open"]);
    let open_rows: Value = serde_json::from_str(&stdout(&open)).expect("JSON");
    assert_eq!(open_rows.as_array().map(Vec::len), Some(1));
    assert_eq!(open_rows[0]["id"], json!("HR-901"));
}

/// Pin of unchanged behavior: `list` without `--deferred` prints the rows
/// of 1.4.0, with no `deferral` object, whether or not an item carries one.
#[test]
fn list_without_deferred_prints_the_rows_of_1_4_0() {
    let dir = mini_install();
    store_deferral(dir.path(), "HR-901");
    let output = run_ok(dir.path(), &["list"]);
    let rows: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(rows.as_array().map(Vec::len), Some(2));
    for row in rows.as_array().expect("an array") {
        assert!(row.get("deferral").is_none(), "got {row}");
    }
}

/// Spec 3.1: `get` prints the stored item, which carries its `deferral`. Pin
/// of unchanged behavior: `get` prints the record as stored.
#[test]
fn get_prints_the_deferral_of_a_deferred_item() {
    let dir = mini_install();
    store_deferral(dir.path(), "HR-901");
    let output = run_ok(dir.path(), &["get", "HR-901"]);
    let items: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(
        items[0]["deferral"],
        json!({"reason": "r", "origin": "o", "files": []})
    );
}

/// Pin of unchanged behavior: the JSON adapter prints what the 1.4.0
/// commands print, here `check-backlog`, `batch`, and `set` on the mini
/// backlog.
#[test]
fn the_json_adapter_prints_what_1_4_0_printed() {
    let dir = mini_install();
    let check = run_ok(dir.path(), &["check-backlog"]);
    assert_output(&check, 0, "backlog: ok\n", "");
    let batch = run_ok(dir.path(), &["batch", "1"]);
    let record: Value = serde_json::from_str(&stdout(&batch)).expect("JSON");
    assert_eq!(record["items"][0]["id"], json!("HR-902"));
    let set = run_ok(dir.path(), &["set", "HR-901", "status=done", "batch=2"]);
    assert_output(&set, 0, "HR-901: status=done batch=2\n", "");
}

/// A `tracker` key of the wrong shape is one named line from every command
/// that reads it, exit 2: `list`, which reads the marker validated, and `pack
/// add process`, which reads the marker as it is and then asks for the
/// tracker. `pack add process` never says that no tracker is installed when
/// the marker names a tracker that cannot be read, and changes nothing.
#[test]
fn a_tracker_key_of_the_wrong_shape_is_one_named_line() {
    let marker_name = |dir: &std::path::Path| dir.join(".houserules.json").display().to_string();
    let cases = [
        (json!("json"), "tracker must be an object"),
        (
            json!({"adapter": "svn"}),
            "tracker.adapter must be json, github, or beads",
        ),
        (
            json!({"adapter": "github"}),
            "tracker.repo must be <owner>/<name> for the github adapter",
        ),
        (
            json!({"adapter": "json", "repo": "acme/widgets"}),
            "tracker.repo belongs to the github adapter only",
        ),
    ];
    for (tracker, message) in cases {
        let dir = mini_install();
        edit_marker(dir.path(), |marker| {
            marker.insert("tracker".to_string(), tracker.clone());
        });
        let output = run(dir.path(), &["list"]);
        assert_output(
            &output,
            2,
            "",
            &format!("{}: {message}\n", marker_name(dir.path())),
        );

        let packs = install_without_backlog();
        edit_marker(packs.path(), |marker| {
            marker.insert("tracker".to_string(), tracker.clone());
        });
        let before = snapshot(packs.path());
        let output = run(packs.path(), &["pack", "add", "process"]);
        assert_output(
            &output,
            2,
            "",
            &format!("{}: {message}\n", marker_name(packs.path())),
        );
        assert_same_tree(&before, &snapshot(packs.path()), "pack add process");
    }
}
