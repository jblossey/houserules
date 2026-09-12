//! `houserules archive` CLI-level tests: the sweep moves each
//! qualifying record class out of the active backlog/knowledge base, is
//! idempotent, and the reader split around it holds -- `get` still
//! resolves an archived id, and `check-knowledge`/`check-backlog` schema-
//! validate the archive directories without judging their liveness. Runs
//! the compiled binary as a real subprocess against real scratch git
//! repositories, following `install.rs`'s structural pattern (that file's
//! own doc explains why each `tests/*.rs` file keeps its own small copy of
//! these helpers rather than sharing `mod common;`).

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

/// A `Command` for the compiled `houserules` binary under test.
fn houserules() -> Command {
    Command::new(env!("CARGO_BIN_EXE_houserules"))
}

/// A fresh scratch directory with `git init` already run -- the minimal
/// fixture `init`'s own `.git`-existence check accepts.
/// `tempfile::TempDir` removes itself on drop
/// (`houserules.tests-clean-scratch-dirs`'s sanctioned Rust form).
fn scratch_git_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    dir
}

/// Seeds `dir` with the real kit payload via `houserules init`, exactly
/// `houserules.live-run-recipe`'s own scratch-repo procedure -- every test
/// here starts from a real `init`ed repository, then edits specific files
/// to build its own scenario, rather than a hand-built fixture that could
/// drift from what `init` actually ships.
fn seed(dir: &Path) {
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir)
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Seeds `dir` the same way `seed` does, but with `--id-prefix HR` --
/// for a test that also runs `check-backlog`, whose schema requires
/// every item id to match the stamped prefix (`init`'s own default, `WI`,
/// otherwise).
fn seed_with_hr_prefix(dir: &Path) {
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir)
        .args(["--id-prefix", "HR"])
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Runs `houserules render` against `dir`, asserting success -- needed
/// after a test hand-edits a knowledge topic file and then checks
/// `check-knowledge`, which fails on stale generated files exactly the
/// way a real sweep's own render step guards against.
fn render(dir: &Path) {
    let output = houserules()
        .args(["render", "--dir"])
        .arg(dir)
        .output()
        .expect("run render");
    assert!(
        output.status.success(),
        "render failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Reads and parses a JSON file at `dir.join(relative)`.
fn read_json(dir: &Path, relative: &str) -> Value {
    let text = fs::read_to_string(dir.join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {relative}: {error}"))
}

/// Writes `value` as pretty JSON with a trailing newline to
/// `dir.join(relative)`, creating parent directories as needed.
fn write_json(dir: &Path, relative: &str, value: &Value) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(
        path,
        format!("{}\n", serde_json::to_string_pretty(value).unwrap()),
    )
    .unwrap();
}

/// A minimal, schema-valid knowledge entry: `id`, `kind`, `area`,
/// `summary`, `body`, `tags`, `source`, plus whatever `overrides` adds or
/// replaces (`status`, typically).
fn knowledge_entry(id: &str, area: &str, overrides: Value) -> Value {
    let mut entry = json!({
        "id": id,
        "kind": "gotcha",
        "area": area,
        "summary": "A fixture entry.",
        "body": ["Fixture body."],
        "tags": [],
        "source": {"date": "2026-09-11", "by": "controller"},
    });
    if let (Value::Object(base), Value::Object(over)) = (&mut entry, overrides) {
        for (key, value) in over {
            base.insert(key, value);
        }
    }
    entry
}

/// A minimal, schema-valid backlog item: `id`, `type`, `milestone: null`,
/// `title`, `body`, plus whatever `overrides` adds or replaces (`status`,
/// typically).
fn backlog_item(id: &str, overrides: Value) -> Value {
    let mut item = json!({
        "id": id,
        "type": "chore",
        "milestone": null,
        "status": "open",
        "title": "Fixture item.",
        "body": ["Fixture body."],
    });
    if let (Value::Object(base), Value::Object(over)) = (&mut item, overrides) {
        for (key, value) in over {
            base.insert(key, value);
        }
    }
    item
}

/// Seeds a done item, a dropped item, and an open item into
/// `backlog/items/general.json`; a done batch and a planned batch into
/// `backlog/batches.json`; and an active entry plus a retired entry into
/// `knowledge/process.json`.
fn seed_records_to_archive(dir: &Path) {
    let mut general = read_json(dir, "backlog/items/general.json");
    general["items"] = json!([
        backlog_item("HR-100", json!({"status": "done"})),
        backlog_item("HR-101", json!({"status": "dropped"})),
        backlog_item("HR-102", json!({"status": "open"})),
    ]);
    write_json(dir, "backlog/items/general.json", &general);

    let mut batches = read_json(dir, "backlog/batches.json");
    batches["batches"] = json!([
        {
            "number": 1,
            "items": [],
            "summary": "Closed batch.",
            "kickoff": "docs/specs/x.md",
            "status": {"state": "done", "text": "Done."},
        },
        {
            "number": 2,
            "items": [],
            "summary": "Open batch.",
            "kickoff": "docs/specs/y.md",
            "status": {"state": "planned", "text": ""},
        },
    ]);
    write_json(dir, "backlog/batches.json", &batches);

    let mut process = read_json(dir, "knowledge/process.json");
    let mut entries = process["entries"].as_array().cloned().unwrap_or_default();
    entries.push(knowledge_entry(
        "process.fixture-active",
        "process",
        json!({}),
    ));
    entries.push(knowledge_entry(
        "process.fixture-retired",
        "process",
        json!({"status": "retired"}),
    ));
    process["entries"] = Value::Array(entries);
    write_json(dir, "knowledge/process.json", &process);
}

/// Moves every seeded qualifying record on the first run, reports each
/// with a named `archived <id> -> <path>` line and a matching summary, and
/// never touches `backlog/decisions.json`.
#[test]
fn archives_done_dropped_items_done_batches_and_retired_knowledge_entries() {
    let dir = scratch_git_repo();
    seed(dir.path());
    seed_records_to_archive(dir.path());
    let decisions_before = fs::read(dir.path().join("backlog/decisions.json")).unwrap();

    let output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(output.status.success(), "stderr: {stderr}");
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        vec![
            "archived HR-100 -> backlog/archive/general.json",
            "archived HR-101 -> backlog/archive/general.json",
            "archived batch 1 -> backlog/archive/batches.json",
            "archived process.fixture-retired -> knowledge/archive/process.json",
            "archive: 4 moved",
        ]
    );

    let general = read_json(dir.path(), "backlog/items/general.json");
    let active_ids: Vec<&str> = general["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(active_ids, vec!["HR-102"]);
    let archived_general = read_json(dir.path(), "backlog/archive/general.json");
    let archived_ids: Vec<&str> = archived_general["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(archived_ids, vec!["HR-100", "HR-101"]);
    assert_eq!(archived_general["section"], general["section"]);

    let batches = read_json(dir.path(), "backlog/batches.json");
    let active_numbers: Vec<i64> = batches["batches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["number"].as_i64().unwrap())
        .collect();
    assert_eq!(active_numbers, vec![2]);
    let archived_batches = read_json(dir.path(), "backlog/archive/batches.json");
    let archived_numbers: Vec<i64> = archived_batches["batches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["number"].as_i64().unwrap())
        .collect();
    assert_eq!(archived_numbers, vec![1]);

    let process = read_json(dir.path(), "knowledge/process.json");
    let active_ids: Vec<&str> = process["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert!(
        active_ids.contains(&"process.fixture-active"),
        "got {active_ids:?}"
    );
    assert!(
        !active_ids.contains(&"process.fixture-retired"),
        "got {active_ids:?}"
    );
    let archived_process = read_json(dir.path(), "knowledge/archive/process.json");
    let archived_ids: Vec<&str> = archived_process["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert_eq!(archived_ids, vec!["process.fixture-retired"]);
    assert_eq!(archived_process["topic"], process["topic"]);

    let decisions_after = fs::read(dir.path().join("backlog/decisions.json")).unwrap();
    assert_eq!(decisions_before, decisions_after);
}

/// A second sweep, with nothing new to move, moves nothing and reports it.
#[test]
fn archive_is_idempotent_a_second_run_moves_nothing() {
    let dir = scratch_git_repo();
    seed(dir.path());
    seed_records_to_archive(dir.path());
    houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");

    let general_before = fs::read(dir.path().join("backlog/items/general.json")).unwrap();
    let archived_general_before =
        fs::read(dir.path().join("backlog/archive/general.json")).unwrap();

    let output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive again");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(output.status.success());
    assert_eq!(stdout, "archive: 0 moved\n");

    assert_eq!(
        fs::read(dir.path().join("backlog/items/general.json")).unwrap(),
        general_before
    );
    assert_eq!(
        fs::read(dir.path().join("backlog/archive/general.json")).unwrap(),
        archived_general_before
    );
}

/// Once a backlog item and a knowledge entry are archived, `get` still
/// resolves each by id and labels the result `"archived": true`; an active
/// id's own `get` output never carries that key at all.
#[test]
fn get_resolves_an_archived_item_and_entry_and_labels_both_archived() {
    let dir = scratch_git_repo();
    seed(dir.path());
    seed_records_to_archive(dir.path());
    houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");

    let output = houserules()
        .args(["get", "HR-100", "process.fixture-retired", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run get");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let results: Vec<Value> = serde_json::from_slice(&output.stdout).expect("get prints JSON");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["id"], "HR-100");
    assert_eq!(results[0]["archived"], Value::Bool(true));
    assert_eq!(results[1]["id"], "process.fixture-retired");
    assert_eq!(results[1]["archived"], Value::Bool(true));

    let active_output = houserules()
        .args(["get", "process.fixture-active", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run get on an active id");
    let active_results: Vec<Value> =
        serde_json::from_slice(&active_output.stdout).expect("get prints JSON");
    assert!(active_results[0].get("archived").is_none());
}

/// An archived record's own liveness problems -- a `verify` path that no
/// longer exists -- are not judged by `check-knowledge` once the sweep has
/// moved it out of the active set, even though the identical entry WOULD
/// have failed that same check while still active.
#[test]
fn check_knowledge_ignores_an_archived_entrys_liveness() {
    let dir = scratch_git_repo();
    seed(dir.path());
    let mut process = read_json(dir.path(), "knowledge/process.json");
    let mut entries = process["entries"].as_array().cloned().unwrap_or_default();
    entries.push(knowledge_entry(
        "process.fixture-dangling-verify",
        "process",
        json!({"verify": ["does/not/exist.rs"]}),
    ));
    process["entries"] = Value::Array(entries);
    write_json(dir.path(), "knowledge/process.json", &process);

    let before = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge");
    assert_eq!(
        before.status.code(),
        Some(1),
        "expected the active entry to fail"
    );
    let before_stderr = String::from_utf8_lossy(&before.stderr).into_owned();
    assert!(
        before_stderr.contains("process.fixture-dangling-verify"),
        "got {before_stderr:?}"
    );

    let mut process = read_json(dir.path(), "knowledge/process.json");
    for entry in process["entries"].as_array_mut().unwrap() {
        if entry["id"] == "process.fixture-dangling-verify" {
            entry["status"] = json!("retired");
        }
    }
    write_json(dir.path(), "knowledge/process.json", &process);
    houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");

    let after = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge again");
    let after_stderr = String::from_utf8_lossy(&after.stderr).into_owned();
    assert!(
        after.status.success(),
        "expected the archived entry to be ignored, got: {after_stderr}"
    );
}

/// Both check gates schema-validate every file directly under their
/// `archive/` directory, so a corrupt archive file is a loud finding
/// rather than a silent pass.
#[test]
fn checks_schema_validate_the_archive_directories() {
    let dir = scratch_git_repo();
    seed(dir.path());
    fs::create_dir_all(dir.path().join("knowledge/archive")).unwrap();
    write_json(
        dir.path(),
        "knowledge/archive/process.json",
        &json!({"topic": "process", "title": "t", "entries": [{"not": "an entry"}]}),
    );
    fs::create_dir_all(dir.path().join("backlog/archive")).unwrap();
    write_json(
        dir.path(),
        "backlog/archive/general.json",
        &json!({"section": "general"}),
    );

    let knowledge_output = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge");
    assert_eq!(knowledge_output.status.code(), Some(1));
    let knowledge_stderr = String::from_utf8_lossy(&knowledge_output.stderr).into_owned();
    assert!(
        knowledge_stderr.contains("knowledge/archive/process.json"),
        "got {knowledge_stderr:?}"
    );

    let backlog_output = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert_eq!(backlog_output.status.code(), Some(1));
    let backlog_stderr = String::from_utf8_lossy(&backlog_output.stderr).into_owned();
    assert!(
        backlog_stderr.contains("backlog/archive/general.json"),
        "got {backlog_stderr:?}"
    );
}

/// A dead-glob area is unaffected by archiving that area's only entry:
/// `houserules archive` never edits `knowledge/areas.json`, and
/// `check-knowledge`'s dead-glob gate keeps firing on the same area both
/// before and after the sweep -- the sweep reports the move, never the
/// gate's own finding for it.
#[test]
fn archiving_an_areas_only_entry_never_edits_areas_json_and_the_dead_glob_gate_still_fires() {
    let dir = scratch_git_repo();
    seed(dir.path());

    let mut schema = read_json(dir.path(), "knowledge/schema.json");
    let mut area_enum = schema["$defs"]["area"]["enum"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    area_enum.push(json!("example"));
    schema["$defs"]["area"]["enum"] = Value::Array(area_enum);
    write_json(dir.path(), "knowledge/schema.json", &schema);

    let mut areas = read_json(dir.path(), "knowledge/areas.json");
    areas["example"] = json!({"paths": ["does-not-exist/**"]});
    write_json(dir.path(), "knowledge/areas.json", &areas);
    let areas_before = fs::read(dir.path().join("knowledge/areas.json")).unwrap();

    let mut misc = json!({"topic": "misc", "title": "Misc", "entries": []});
    misc["entries"] = json!([knowledge_entry(
        "misc.temp",
        "example",
        json!({"status": "retired"})
    )]);
    write_json(dir.path(), "knowledge/misc.json", &misc);

    let before = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge");
    let before_stderr = String::from_utf8_lossy(&before.stderr).into_owned();
    assert!(
        before_stderr.contains("knowledge/areas.json.example.paths"),
        "got {before_stderr:?}"
    );

    let archive_output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    assert!(archive_output.status.success());

    let areas_after = fs::read(dir.path().join("knowledge/areas.json")).unwrap();
    assert_eq!(
        areas_before, areas_after,
        "archive must never edit areas.json"
    );

    let after = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge again");
    let after_stderr = String::from_utf8_lossy(&after.stderr).into_owned();
    assert!(
        after_stderr.contains("knowledge/areas.json.example.paths"),
        "the dead-glob gate must still fire on the emptied area, got: {after_stderr:?}"
    );
}

/// A read failure during the sweep (here, `backlog/items` removed
/// entirely) is one named stderr line and exit 2, never a panic
/// (`houserules.crash-paths-are-named`).
#[test]
fn archive_names_one_error_and_exits_2_on_an_unreadable_items_directory() {
    let dir = scratch_git_repo();
    seed(dir.path());
    fs::remove_dir_all(dir.path().join("backlog/items")).unwrap();

    let output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert_eq!(stderr.lines().count(), 1, "got {stderr:?}");
    assert!(stderr.contains("backlog/items"), "got {stderr:?}");
    assert!(output.stdout.is_empty());
}

/// A `see` citation from an active knowledge entry to one the sweep is
/// about to retire keeps resolving after the sweep: `check-knowledge`
/// stays green both before and after `houserules archive` moves the cited
/// entry into `knowledge/archive/`.
#[test]
fn see_citations_resolve_against_archived_knowledge_entries_across_a_sweep() {
    let dir = scratch_git_repo();
    seed(dir.path());
    let mut process = read_json(dir.path(), "knowledge/process.json");
    let mut entries = process["entries"].as_array().cloned().unwrap_or_default();
    entries.push(knowledge_entry(
        "process.fixture-keeper",
        "process",
        json!({"see": ["process.fixture-gone"]}),
    ));
    entries.push(knowledge_entry(
        "process.fixture-gone",
        "process",
        json!({"status": "retired"}),
    ));
    process["entries"] = Value::Array(entries);
    write_json(dir.path(), "knowledge/process.json", &process);
    render(dir.path());

    let before = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge before the sweep");
    assert!(
        before.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&before.stderr)
    );

    let archive_output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    assert!(archive_output.status.success());
    render(dir.path());

    let after = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge after the sweep");
    let after_stderr = String::from_utf8_lossy(&after.stderr).into_owned();
    assert!(
        after.status.success(),
        "a see citation to an archived entry must still resolve, got: {after_stderr}"
    );
}

/// The backlog counterpart: an active item's `see` citation to an item the
/// sweep is about to archive (`status: done`) keeps resolving after the
/// sweep, and `check-backlog` stays green both before and after.
#[test]
fn see_citations_resolve_against_archived_backlog_items_across_a_sweep() {
    let dir = scratch_git_repo();
    seed_with_hr_prefix(dir.path());
    let mut general = read_json(dir.path(), "backlog/items/general.json");
    general["items"] = json!([
        backlog_item("HR-100", json!({"see": ["HR-101"]})),
        backlog_item("HR-101", json!({"status": "done"})),
    ]);
    write_json(dir.path(), "backlog/items/general.json", &general);

    let before = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog before the sweep");
    assert!(
        before.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&before.stderr)
    );

    let archive_output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    assert!(archive_output.status.success());

    let after = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog after the sweep");
    let after_stderr = String::from_utf8_lossy(&after.stderr).into_owned();
    assert!(
        after.status.success(),
        "a see citation to an archived item must still resolve, got: {after_stderr}"
    );
}

/// `get` resolves an archived item id in a default-prefix (`WI`) project,
/// where the active id-shape matcher would never route the id to the
/// backlog branch at all -- the archive fallback tries both domains
/// regardless of that routing.
#[test]
fn get_resolves_an_archived_item_with_the_default_id_prefix() {
    let dir = scratch_git_repo();
    seed(dir.path());
    houserules()
        .args(["set", "WI-001", "status=done"])
        .arg("--dir")
        .arg(dir.path())
        .output()
        .expect("run set");

    let archive_output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    let archive_stdout = String::from_utf8_lossy(&archive_output.stdout).into_owned();
    assert!(
        archive_stdout.contains("archived WI-001"),
        "got {archive_stdout:?}"
    );

    let get_output = houserules()
        .args(["get", "WI-001", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run get");
    assert!(
        get_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&get_output.stderr)
    );
    let results: Vec<Value> = serde_json::from_slice(&get_output.stdout).expect("get prints JSON");
    assert_eq!(results[0]["id"], "WI-001");
    assert_eq!(results[0]["archived"], Value::Bool(true));
}

/// A malformed archive file makes `get` name the file instead of printing
/// the misleading `unknown id`: the archive fallback surfaces a parse
/// failure rather than silently treating it as "no match".
#[test]
fn get_names_a_malformed_archive_file_instead_of_printing_unknown_id() {
    let dir = scratch_git_repo();
    seed(dir.path());
    let mut process = read_json(dir.path(), "knowledge/process.json");
    let mut entries = process["entries"].as_array().cloned().unwrap_or_default();
    entries.push(knowledge_entry(
        "process.fixture-retired",
        "process",
        json!({"status": "retired"}),
    ));
    process["entries"] = Value::Array(entries);
    write_json(dir.path(), "knowledge/process.json", &process);
    houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    fs::write(
        dir.path().join("knowledge/archive/process.json"),
        "{ not json",
    )
    .unwrap();

    let output = houserules()
        .args(["get", "process.fixture-retired", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run get");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    // get keeps the shared reader's own (absolute, Path-joined) message for
    // its own error text -- unlike a check-gate finding, it has no shorter
    // relative name to report instead. Mirror the join component by
    // component (houserules.path-pins-mirror-the-code): a hardcoded
    // forward-slash literal would diverge from the real separator on
    // Windows.
    let archive_path = dir.path().join("knowledge/archive").join("process.json");
    assert!(
        stderr.contains(&format!("{}: invalid JSON", archive_path.display())),
        "got {stderr:?}"
    );
    assert!(
        !stderr.contains("unknown id"),
        "a parse failure must be named, not reported as unknown id: got {stderr:?}"
    );
}

/// A `knowledge/archive` (or `backlog/archive`) path that exists but is
/// not a directory is a named finding, not a silent pass -- `check-
/// knowledge` and `check-backlog` distinguish "does not exist" (tolerated)
/// from every other reason the directory cannot be opened.
#[test]
fn check_gates_name_an_archive_path_that_is_not_a_directory() {
    let dir = scratch_git_repo();
    seed(dir.path());
    fs::write(dir.path().join("knowledge/archive"), "oops").unwrap();
    fs::write(dir.path().join("backlog/archive"), "oops").unwrap();

    let knowledge_output = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge");
    assert_eq!(knowledge_output.status.code(), Some(1));
    let knowledge_stderr = String::from_utf8_lossy(&knowledge_output.stderr).into_owned();
    assert!(
        knowledge_stderr.contains("knowledge/archive"),
        "got {knowledge_stderr:?}"
    );

    let backlog_output = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert_eq!(backlog_output.status.code(), Some(1));
    let backlog_stderr = String::from_utf8_lossy(&backlog_output.stderr).into_owned();
    assert!(
        backlog_stderr.contains("backlog/archive"),
        "got {backlog_stderr:?}"
    );
}

/// Every move already made before a later step fails is still printed:
/// the sweep buffers `archived ...` lines but must not withhold them just
/// because a subsequent step errors.
#[test]
fn archive_prints_moves_already_made_before_a_later_step_fails() {
    let dir = scratch_git_repo();
    seed(dir.path());
    let mut general = read_json(dir.path(), "backlog/items/general.json");
    general["items"] = json!([backlog_item("HR-100", json!({"status": "done"}))]);
    write_json(dir.path(), "backlog/items/general.json", &general);
    fs::write(dir.path().join("backlog/batches.json"), "{ not json").unwrap();

    let output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    assert_eq!(output.status.code(), Some(2));
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("archived HR-100 -> backlog/archive/general.json"),
        "got {stdout:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    assert!(stderr.contains("backlog/batches.json"), "got {stderr:?}");
}

/// A batch entry with no `number` field renders through the same `"?"`
/// designed-absence token `record_id` uses for a missing item id, instead
/// of a fabricated `0`.
#[test]
fn archive_renders_a_numberless_batch_with_the_designed_token() {
    let dir = scratch_git_repo();
    seed(dir.path());
    let mut batches = read_json(dir.path(), "backlog/batches.json");
    batches["batches"] = json!([{
        "items": [],
        "summary": "No number.",
        "kickoff": "docs/specs/x.md",
        "status": {"state": "done", "text": "Done."},
    }]);
    write_json(dir.path(), "backlog/batches.json", &batches);

    let output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    assert!(
        stdout.contains("archived batch ? -> backlog/archive/batches.json"),
        "got {stdout:?}"
    );
}

/// A batch not yet archived can still list an item the sweep just
/// archived: batch membership resolves against active plus archived
/// items, the same invariant the `see`-citation fix already gives the
/// backlog gate's other id reference. `check-backlog` stays green both
/// before and after the sweep, and a batch entry naming a genuinely
/// unknown id still reports it.
#[test]
fn batch_membership_resolves_against_archived_items_across_a_sweep() {
    let dir = scratch_git_repo();
    seed_with_hr_prefix(dir.path());
    let mut general = read_json(dir.path(), "backlog/items/general.json");
    general["items"] = json!([backlog_item("HR-048", json!({}))]);
    write_json(dir.path(), "backlog/items/general.json", &general);
    let mut batches = read_json(dir.path(), "backlog/batches.json");
    batches["batches"] = json!([
        {
            "number": 19,
            "items": ["HR-048"],
            "summary": "Planned batch listing a not-yet-archived item.",
            "kickoff": "docs/specs/x.md",
            "status": {"state": "planned", "text": ""},
        },
        {
            "number": 20,
            "items": ["HR-999"],
            "summary": "Planned batch listing an id that never existed.",
            "kickoff": "docs/specs/y.md",
            "status": {"state": "planned", "text": ""},
        },
    ]);
    write_json(dir.path(), "backlog/batches.json", &batches);

    let before = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog before the sweep");
    let before_stderr = String::from_utf8_lossy(&before.stderr).into_owned();
    assert!(
        before_stderr.contains("HR-999"),
        "a genuinely unknown id must already report, got: {before_stderr:?}"
    );

    houserules()
        .args(["set", "HR-048", "status=done"])
        .arg("--dir")
        .arg(dir.path())
        .output()
        .expect("run set");
    let archive_output = houserules()
        .args(["archive", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run archive");
    let archive_stdout = String::from_utf8_lossy(&archive_output.stdout).into_owned();
    assert!(
        archive_stdout.contains("archived HR-048"),
        "got {archive_stdout:?}"
    );

    let after = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog after the sweep");
    let after_stderr = String::from_utf8_lossy(&after.stderr).into_owned();
    assert!(
        !after_stderr.contains("HR-048"),
        "batch 19's now-archived member must still resolve, got: {after_stderr:?}"
    );
    assert!(
        after_stderr.contains("HR-999"),
        "a genuinely unknown id must still report after the sweep, got: {after_stderr:?}"
    );
}

/// A check-gate finding for a corrupt (unparsable) archive FILE names its
/// file by the repository-relative path both check_archive passes already
/// compute, built as a string -- never the absolute path the shared
/// reader returns for `get`'s own messages. The relative form is a plain
/// `format!` string with a literal forward slash, so it never diverges
/// across platforms the way a `Path::join`ed path would
/// (`houserules.path-pins-mirror-the-code`); this assertion also checks
/// the scratch directory's own absolute prefix is ABSENT, which a
/// regression back to `Path::display()` would fail.
#[test]
fn check_gates_name_a_corrupt_archive_file_by_its_relative_path() {
    let dir = scratch_git_repo();
    seed(dir.path());
    fs::create_dir_all(dir.path().join("knowledge/archive")).unwrap();
    fs::write(
        dir.path().join("knowledge/archive/process.json"),
        "{ not json",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("backlog/archive")).unwrap();
    fs::write(
        dir.path().join("backlog/archive/general.json"),
        "{ not json",
    )
    .unwrap();

    let knowledge_output = houserules()
        .args(["check-knowledge", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-knowledge");
    assert_eq!(knowledge_output.status.code(), Some(1));
    let knowledge_stderr = String::from_utf8_lossy(&knowledge_output.stderr).into_owned();
    assert!(
        knowledge_stderr.contains("knowledge/archive/process.json: invalid JSON"),
        "got {knowledge_stderr:?}"
    );
    assert!(
        !knowledge_stderr.contains(&dir.path().display().to_string()),
        "a check-gate finding must name the relative path, not the scratch directory's own \
         absolute one: got {knowledge_stderr:?}"
    );

    let backlog_output = houserules()
        .args(["check-backlog", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run check-backlog");
    assert_eq!(backlog_output.status.code(), Some(1));
    let backlog_stderr = String::from_utf8_lossy(&backlog_output.stderr).into_owned();
    assert!(
        backlog_stderr.contains("backlog/archive/general.json: invalid JSON"),
        "got {backlog_stderr:?}"
    );
    assert!(
        !backlog_stderr.contains(&dir.path().display().to_string()),
        "a check-gate finding must name the relative path, not the scratch directory's own \
         absolute one: got {backlog_stderr:?}"
    );
}
