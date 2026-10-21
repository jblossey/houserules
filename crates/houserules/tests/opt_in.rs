//! The opt-in of a pack entry (batch 34, spec 3.7): a pack manifest lists
//! `optIn` ids, and every path that brings such an id onto an install writes
//! an exclusion with the reason `opt-in` once, and records the id in
//! `.houserules.json` `optInOffered`. A project that deletes the exclusion has
//! opted in for good; `pack remove` withdraws only an offer that the project
//! did not take. Each case runs the compiled binary against a scratch git
//! repository, with a pack of its own that lists opt-in ids: `pack add`,
//! `pack upgrade`, and `pack remove` are the paths that a binary test can
//! reach. Each command prints its whole stdout, which a test compares line
//! for line. No stop of `pack add` or `pack upgrade` leaves an opt-in entry
//! live: a read-only file stops each command at each of its writes in turn,
//! and a rerun finishes the work. The pin move of `update` and the functions
//! that every path calls are unit tests in `install/pack_moves.rs` and
//! `pack/opt_in.rs`.
//!
//! Tests that assert behavior this batch adds fail at the commit before it
//! (the RED of design.md 173(2)). A test that asserts behavior that did not
//! change says so in its doc comment: it is a pin of unchanged behavior.

mod locked_file;
mod tracker_support;
mod tree_snapshot;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    assert_output, read_json, run, run_ok, scratch_git_repo, stdout, write_json,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The line that says an offered entry is excluded until the project deletes
/// its exclusion.
fn offer_line(id: &str) -> String {
    format!(
        "{id} is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion"
    )
}

/// The line of spec 3.7 for an entry that the install opted in to before.
fn active_line(id: &str) -> String {
    format!("{id} is active: this install opted in before")
}

/// The first line of `pack add` for the vendored demo pack at version 1.0.0.
const ADDED_DEMO: &str = "added pack demo 1.0.0 from file:vendor/demo.json";

/// The line that the render after a pack command prints for the root
/// `AGENTS.md` when it changed.
const AGENTS_WRITTEN: &str = "AGENTS.md: written";

/// The lines that `output` printed on stdout.
fn lines(output: &std::process::Output) -> Vec<String> {
    stdout(output).lines().map(str::to_string).collect()
}

/// `lines` as the owned strings that [`lines`] returns, so a test compares a
/// whole stdout with one `assert_eq!`.
fn expected(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|line| (*line).to_string()).collect()
}

/// A scratch install with the JSON tracker and the kit's packs.
fn install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    dir
}

/// The `exclusions` of `knowledge/overrides.json`, or none when the file is
/// absent.
fn exclusions(dir: &Path) -> Vec<Value> {
    if !dir.join("knowledge/overrides.json").exists() {
        return Vec::new();
    }
    read_json(dir, "knowledge/overrides.json")["exclusions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The ids that `.houserules.json` records in `optInOffered`.
fn offered(dir: &Path) -> Vec<String> {
    read_json(dir, ".houserules.json")["optInOffered"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .map(|id| id.as_str().expect("an id").to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The exclusion that an offer writes for `id`.
fn opt_in_exclusion(id: &str) -> Value {
    json!({"id": id, "reason": "opt-in"})
}

/// Deletes the exclusion of `id` from `knowledge/overrides.json`: the act by
/// which a project opts in.
fn opt_in(dir: &Path, id: &str) {
    let mut document = read_json(dir, "knowledge/overrides.json");
    let kept: Vec<Value> = exclusions(dir)
        .into_iter()
        .filter(|exclusion| exclusion["id"] != json!(id))
        .collect();
    document["exclusions"] = json!(kept);
    write_json(dir, "knowledge/overrides.json", &document);
}

/// A pack at `schema` 1.1.0 named `demo` that serves the topic `demo` and
/// lists `opt_in` ids under `optIn`, with one entry for each id of `ids`.
fn demo_pack(version: &str, ids: &[&str], opt_in: &[&str]) -> Value {
    let entries: Vec<Value> = ids
        .iter()
        .map(|id| {
            json!({
                "id": id,
                "kind": "rule",
                "area": "global",
                "standing": true,
                "summary": format!("The rule {id}."),
                "body": ["Body."],
                "tags": [],
                "source": {"date": "2026-10-20", "by": "controller"},
            })
        })
        .collect();
    json!({
        "name": "demo",
        "version": version,
        "schema": "houserules/pack@1.1.0",
        "topics": {"demo": {"title": "Demo rules"}},
        "areas": {"global": {"paths": []}},
        "entries": entries,
        "optIn": opt_in,
    })
}

/// Writes `pack` to `vendor/demo.json` below `dir` and returns its absolute
/// path, as `pack add` takes a file source.
fn vendor(dir: &Path, pack: &Value) -> String {
    write_json(dir, "vendor/demo.json", pack);
    dir.join("vendor/demo.json")
        .to_str()
        .expect("utf-8 path")
        .to_string()
}

/// The install `install` made, with the demo pack vendored at version 1.0.0:
/// it lists `demo.one` under `optIn` and ships `demo.two` beside it. Returns
/// the install and the path of the vendored file.
fn install_with_the_demo_pack() -> (TempDir, String) {
    let dir = install();
    let file = vendor(
        dir.path(),
        &demo_pack("1.0.0", &["demo.one", "demo.two"], &["demo.one"]),
    );
    (dir, file)
}

/// Whether `get` shows the entry `id` as excluded, and for what reason.
fn excluded_reason(dir: &Path, id: &str) -> Option<Value> {
    let output = run_ok(dir, &["get", id]);
    let records: Value = serde_json::from_str(&stdout(&output)).expect("get prints JSON");
    records[0].get("excluded").cloned()
}

/// Spec 3.7: `pack add` of a pack with an `optIn` id writes the exclusion
/// with the reason `opt-in`, records the id in `optInOffered`, and prints the
/// line that says so. An entry of the pack that is not opt-in stays active.
/// The entry stays out of the base, so `check-knowledge` and `render --check`
/// pass.
#[test]
fn pack_add_excludes_an_opt_in_entry_and_records_the_offer() {
    let (dir, file) = install_with_the_demo_pack();
    let output = run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(
        lines(&output),
        expected(&[ADDED_DEMO, &offer_line("demo.one"), AGENTS_WRITTEN]),
        "the one offer line is for demo.one, and none is for demo.two"
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    assert_eq!(offered(dir.path()), vec!["demo.one".to_string()]);
    run_ok(dir.path(), &["check-knowledge"]);
    run_ok(dir.path(), &["render", "--check"]);
    assert_eq!(
        excluded_reason(dir.path(), "demo.one"),
        Some(json!("opt-in"))
    );
    assert_eq!(excluded_reason(dir.path(), "demo.two"), None);
}

/// Pin of unchanged behavior: a pack with no `optIn` list writes no
/// exclusion, creates no `overrides.json`, and records nothing in the marker.
#[test]
fn pack_add_of_a_pack_with_no_opt_in_list_writes_no_exclusion_and_no_record() {
    let dir = install();
    let mut pack = demo_pack("1.0.0", &["demo.one"], &[]);
    pack.as_object_mut().expect("an object").remove("optIn");
    let file = vendor(dir.path(), &pack);
    run_ok(dir.path(), &["pack", "add", &file]);
    assert!(!dir.path().join("knowledge/overrides.json").exists());
    assert!(
        read_json(dir.path(), ".houserules.json")
            .get("optInOffered")
            .is_none()
    );
}

/// Spec 3.7: a later `pack add` after the project opted in prints the line
/// that says the entry is active, and writes no exclusion. The opt-in
/// survived `pack remove`, which keeps `optInOffered` for an id whose
/// exclusion the project had deleted.
#[test]
fn pack_add_after_the_project_opted_in_prints_the_active_line_and_writes_no_exclusion() {
    let (dir, file) = install_with_the_demo_pack();
    run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    opt_in(dir.path(), "demo.one");
    run_ok(dir.path(), &["pack", "remove", "demo"]);
    assert_eq!(offered(dir.path()), vec!["demo.one".to_string()]);

    let output = run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(
        lines(&output),
        expected(&[ADDED_DEMO, &active_line("demo.one"), AGENTS_WRITTEN]),
        "a project that opted in is told the entry is active and is offered nothing"
    );
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), vec!["demo.one".to_string()]);
    assert_eq!(excluded_reason(dir.path(), "demo.one"), None);
}

/// Spec 3.7: `pack remove` drops an offer that the project did not take: the
/// exclusion goes with the pack, `optInOffered` loses the id, and a later
/// `pack add` offers the entry again. The refusal of `pack remove` for a
/// pack whose id `overrides.json` names does not apply to this exclusion.
#[test]
fn pack_remove_withdraws_an_offer_the_project_did_not_take() {
    let (dir, file) = install_with_the_demo_pack();
    run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    run_ok(dir.path(), &["pack", "remove", "demo"]);
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), Vec::<String>::new());

    let output = run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(
        lines(&output),
        expected(&[ADDED_DEMO, &offer_line("demo.one"), AGENTS_WRITTEN])
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    assert_eq!(offered(dir.path()), vec!["demo.one".to_string()]);
}

/// Spec 3.7: `pack remove` withdraws the exclusion of the pack and leaves
/// every other exclusion of the project where it is.
#[test]
fn pack_remove_keeps_the_exclusions_that_are_not_its_offers() {
    let (dir, file) = install_with_the_demo_pack();
    run_ok(dir.path(), &["pack", "add", &file]);
    let mut document = read_json(dir.path(), "knowledge/overrides.json");
    let own = json!({"id": "process.risk-tiers", "reason": "We run no tiers."});
    document["exclusions"] = json!([own, opt_in_exclusion("demo.one")]);
    write_json(dir.path(), "knowledge/overrides.json", &document);
    run_ok(dir.path(), &["pack", "remove", "demo"]);
    assert_eq!(exclusions(dir.path()), vec![own]);
}

/// Spec 3.7: an exclusion that the project wrote for its own reason is the
/// project's data. `pack remove` refuses as it did before, names the id, and
/// changes nothing.
#[test]
fn pack_remove_refuses_an_exclusion_that_the_project_wrote_for_its_own_reason() {
    let (dir, file) = install_with_the_demo_pack();
    run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    let mut document = read_json(dir.path(), "knowledge/overrides.json");
    document["exclusions"] = json!([{"id": "demo.one", "reason": "We write the trailer by hand."}]);
    write_json(dir.path(), "knowledge/overrides.json", &document);
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["pack", "remove", "demo"]);
    assert_output(
        &output,
        2,
        "",
        "cannot remove pack demo: knowledge/overrides.json names demo.one; remove those overrides and exclusions first\n",
    );
    assert_same_tree(&before, &snapshot(dir.path()), "pack remove demo");
}

/// Spec 3.7: `pack upgrade` offers an `optIn` id that the new version adds,
/// once, and offers nothing for an id that the project took: the earlier
/// entry stays active, the new one is excluded, and both are recorded.
#[test]
fn pack_upgrade_offers_the_opt_in_entries_a_new_version_adds() {
    let (dir, file) = install_with_the_demo_pack();
    let added = run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(
        lines(&added),
        expected(&[ADDED_DEMO, &offer_line("demo.one"), AGENTS_WRITTEN])
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    opt_in(dir.path(), "demo.one");

    vendor(
        dir.path(),
        &demo_pack(
            "1.1.0",
            &["demo.one", "demo.two", "demo.three"],
            &["demo.one", "demo.three"],
        ),
    );
    let upgraded = run_ok(dir.path(), &["pack", "upgrade", "demo"]);
    assert_eq!(
        lines(&upgraded),
        expected(&[
            "upgraded pack demo from 1.0.0 to 1.1.0",
            "added id demo.three",
            &offer_line("demo.three"),
            AGENTS_WRITTEN,
        ]),
        "the new id is offered, and the id the project took is not"
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.three")]);
    assert_eq!(
        offered(dir.path()),
        vec!["demo.one".to_string(), "demo.three".to_string()]
    );
    run_ok(dir.path(), &["render", "--check"]);
}

/// The pack command that a stop test interrupts.
#[derive(Clone, Copy, PartialEq)]
enum Command {
    /// `pack add` of the vendored demo pack.
    Add,
    /// `pack upgrade demo`.
    Upgrade,
}

impl Command {
    /// The arguments of the command; `file` is the vendored pack file.
    fn args(self, file: &str) -> Vec<String> {
        match self {
            Command::Add => vec!["pack".to_string(), "add".to_string(), file.to_string()],
            Command::Upgrade => ["pack", "upgrade", "demo"]
                .iter()
                .map(|part| (*part).to_string())
                .collect(),
        }
    }
}

/// What a stop test makes read-only.
enum Lock {
    /// A file below the install root, which a write to it fails to change.
    File(&'static str),
    /// A directory below the install root, in which the command can create no
    /// file. Windows ignores the read-only flag of a directory, so the case
    /// ends at once there.
    Directory(&'static str),
}

impl Lock {
    /// Makes the target read-only. The answer is `None`, and the target
    /// writable, when a write still succeeds: a user with the right to write
    /// anywhere (`root`) is not stopped by the lock.
    fn lock(&self, dir: &Path) -> Option<locked_file::Locked> {
        match self {
            Lock::File(relative) => locked_file::lock(&dir.join(relative)),
            Lock::Directory(relative) => {
                let path = dir.join(relative);
                let guard = locked_file::lock(&path)?;
                let probe = path.join(".probe");
                if fs::File::create(&probe).is_ok() {
                    let _ = fs::remove_file(&probe);
                    return None;
                }
                Some(guard)
            }
        }
    }

    /// The path that the lock covers, for a failure message.
    fn name(&self) -> &'static str {
        match self {
            Lock::File(relative) | Lock::Directory(relative) => relative,
        }
    }
}

/// `true` when `get` shows the entry `id` active: the install holds it and no
/// exclusion hides it. An install that holds no such entry answers `false`.
fn is_live(dir: &Path, id: &str) -> bool {
    let output = run(dir, &["get", id]);
    if !output.status.success() {
        return false;
    }
    let records: Value = serde_json::from_str(&stdout(&output)).expect("get prints JSON");
    records[0].get("excluded").is_none_or(Value::is_null)
}

/// An empty `knowledge/overrides.json`, which a stop test creates so that it
/// can make the file read-only before the command writes it.
fn write_empty_overrides(dir: &Path) {
    write_json(
        dir,
        "knowledge/overrides.json",
        &json!({"schema": "houserules/overrides@1.0.0", "overrides": [], "exclusions": []}),
    );
}

/// The state that `pack add` starts from in a stop test: the install with the
/// vendored demo pack and an `overrides.json` to lock.
fn before_pack_add() -> (TempDir, String) {
    let (dir, file) = install_with_the_demo_pack();
    write_empty_overrides(dir.path());
    (dir, file)
}

/// The state that `pack upgrade` starts from in a stop test: the demo pack at
/// 1.0.0 added, and the vendored file at 1.1.0, which ships the new entry
/// `demo.three` and lists it under `optIn`.
fn before_pack_upgrade_that_adds_an_opt_in_entry() -> (TempDir, String) {
    let (dir, file) = install_with_the_demo_pack();
    run_ok(dir.path(), &["pack", "add", &file]);
    let file = vendor(
        dir.path(),
        &demo_pack(
            "1.1.0",
            &["demo.one", "demo.two", "demo.three"],
            &["demo.one", "demo.three"],
        ),
    );
    (dir, file)
}

/// The state that `pack upgrade` starts from when the new version, 1.1.0,
/// drops `demo.one`, an offer that the project did not take.
fn before_pack_upgrade_that_drops_an_offer() -> (TempDir, String) {
    let (dir, file) = install_with_the_demo_pack();
    run_ok(dir.path(), &["pack", "add", &file]);
    let file = vendor(dir.path(), &demo_pack("1.1.0", &["demo.two"], &[]));
    (dir, file)
}

/// Interrupts `command` at the write to `lock`, and requires the three facts
/// of `houserules.migrations-finish-after-a-stop` for an opt-in entry: the
/// stopped run exits 2; no entry of `watched` is live without the project's
/// opt-in; and a rerun (after `pack remove` when `pack add` left the pack
/// installed with no pin) leaves the tree that an uninterrupted run leaves.
/// `prepare` builds the install of the case and of its control.
fn assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
    prepare: impl Fn() -> (TempDir, String),
    command: Command,
    lock: &Lock,
    watched: &[&str],
) {
    let run_command = |dir: &Path, file: &str| {
        let args = command.args(file);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run(dir, &refs)
    };
    let (control, control_file) = prepare();
    assert_eq!(
        run_command(control.path(), &control_file).status.code(),
        Some(0)
    );

    let (dir, file) = prepare();
    let Some(guard) = lock.lock(dir.path()) else {
        return;
    };
    let stopped = run_command(dir.path(), &file);
    drop(guard);
    let name = lock.name();
    assert_eq!(
        stopped.status.code(),
        Some(2),
        "the write to {name} stops the run: {stopped:?}"
    );
    for id in watched {
        assert!(
            !is_live(dir.path(), id),
            "after a stop at the write to {name}, {id} is live without the project's opt-in"
        );
    }
    if command == Command::Add && dir.path().join("knowledge/packs/demo.json").exists() {
        run_ok(dir.path(), &["pack", "remove", "demo"]);
    }
    let finished = run_command(dir.path(), &file);
    assert_eq!(finished.status.code(), Some(0), "{finished:?}");
    assert_same_tree(
        &snapshot(control.path()),
        &snapshot(dir.path()),
        &format!("the run that stopped at the write to {name}"),
    );
}

/// Spec 3.7 and `houserules.migrations-finish-after-a-stop`: `pack add` writes
/// the exclusion of an opt-in entry before it writes the copy that makes the
/// entry live. A stop at the write of the exclusion leaves no pack.
#[test]
fn a_pack_add_that_stops_at_the_exclusion_leaves_no_entry_live() {
    assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
        before_pack_add,
        Command::Add,
        &Lock::File("knowledge/overrides.json"),
        &["demo.one"],
    );
}

/// A stop at the write of the copy leaves the exclusion and no pack.
#[test]
fn a_pack_add_that_stops_at_the_copy_leaves_no_entry_live() {
    assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
        before_pack_add,
        Command::Add,
        &Lock::Directory("knowledge/packs"),
        &["demo.one"],
    );
}

/// A stop at the write of the marker leaves the copy and the exclusion: the
/// entry is excluded, and `pack remove` then `pack add` finishes the run.
#[test]
fn a_pack_add_that_stops_at_the_marker_leaves_no_entry_live() {
    assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
        before_pack_add,
        Command::Add,
        &Lock::File(".houserules.json"),
        &["demo.one"],
    );
}

/// Spec 3.7: `pack upgrade` writes the exclusion of a new opt-in entry before
/// it writes the copy that ships the entry. A stop at that write leaves the old
/// copy.
#[test]
fn a_pack_upgrade_that_stops_at_the_exclusion_leaves_no_entry_live() {
    assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
        before_pack_upgrade_that_adds_an_opt_in_entry,
        Command::Upgrade,
        &Lock::File("knowledge/overrides.json"),
        &["demo.three"],
    );
}

/// A stop at the write of the new copy leaves the old copy and the exclusion.
#[test]
fn a_pack_upgrade_that_stops_at_the_copy_leaves_no_entry_live() {
    assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
        before_pack_upgrade_that_adds_an_opt_in_entry,
        Command::Upgrade,
        &Lock::File("knowledge/packs/demo.json"),
        &["demo.three"],
    );
}

/// A stop at the write of the marker leaves the new copy and the exclusion:
/// the new entry is excluded, and the same `pack upgrade` finishes the run.
#[test]
fn a_pack_upgrade_that_stops_at_the_marker_leaves_no_entry_live() {
    assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
        before_pack_upgrade_that_adds_an_opt_in_entry,
        Command::Upgrade,
        &Lock::File(".houserules.json"),
        &["demo.three"],
    );
}

/// The one-id-one-source step of `pack add` writes `overrides.json` when a
/// local entry carries the id of a pack entry: it must write the document that
/// already holds the exclusion of the opt-in entry, never the file as it was
/// before the offer, so the exclusion survives the step.
#[test]
fn pack_add_keeps_the_exclusion_when_the_one_source_step_writes_overrides() {
    let (dir, file) = install_with_the_demo_pack();
    write_json(
        dir.path(),
        "knowledge/local.json",
        &json!({
            "topic": "local",
            "title": "Local rules",
            "entries": [{
                "id": "demo.two",
                "kind": "rule",
                "area": "global",
                "standing": true,
                "summary": "Our own words.",
                "body": ["Body."],
                "tags": [],
                "source": {"date": "2026-10-20", "by": "controller"},
            }],
        }),
    );
    let output = run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(
        lines(&output),
        expected(&[
            ADDED_DEMO,
            "moved demo.two from knowledge/local.json to knowledge/overrides.json: pack demo ships it, and the override sets summary",
            "removed knowledge/local.json: installed packs ship every entry in it",
            &offer_line("demo.one"),
            AGENTS_WRITTEN,
        ])
    );
    let overrides = read_json(dir.path(), "knowledge/overrides.json");
    assert_eq!(
        overrides["overrides"],
        json!([{"id": "demo.two", "fields": {"summary": "Our own words."}}])
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    assert_eq!(
        excluded_reason(dir.path(), "demo.one"),
        Some(json!("opt-in"))
    );
}

/// Writes the local topic file `knowledge/local.json` with one entry `id` whose
/// summary differs from the summary of the pack entry of that id.
fn write_a_local_entry_with_the_id(dir: &Path, id: &str) {
    write_json(
        dir,
        "knowledge/local.json",
        &json!({
            "topic": "local",
            "title": "Local rules",
            "entries": [{
                "id": id,
                "kind": "rule",
                "area": "global",
                "standing": true,
                "summary": "Our own words.",
                "body": ["Body."],
                "tags": [],
                "source": {"date": "2026-10-20", "by": "controller"},
            }],
        }),
    );
}

/// Spec 3.7: an opt-in entry is inactive until the project opts in, also when
/// the project holds a local entry with its id. The one-id-one-source step
/// moves the local entry into an override of the pack entry. It replaces the
/// items that the project wrote, never the exclusion that the offer writes, so
/// the entry stays excluded and the lines of the step do not name an exclusion
/// that the project never wrote.
#[test]
fn pack_add_keeps_an_opt_in_entry_excluded_when_a_local_entry_carries_its_id() {
    let (dir, file) = install_with_the_demo_pack();
    write_a_local_entry_with_the_id(dir.path(), "demo.one");
    let output = run_ok(dir.path(), &["pack", "add", &file]);
    assert_eq!(
        lines(&output),
        expected(&[
            ADDED_DEMO,
            "moved demo.one from knowledge/local.json to knowledge/overrides.json: pack demo ships it, and the override sets summary",
            "removed knowledge/local.json: installed packs ship every entry in it",
            &offer_line("demo.one"),
            AGENTS_WRITTEN,
        ])
    );
    let overrides = read_json(dir.path(), "knowledge/overrides.json");
    assert_eq!(
        overrides["overrides"],
        json!([{"id": "demo.one", "fields": {"summary": "Our own words."}}])
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    assert_eq!(
        excluded_reason(dir.path(), "demo.one"),
        Some(json!("opt-in"))
    );
}

/// The same for `pack upgrade`: a new version ships `demo.three` as an opt-in
/// entry, and the project holds a local entry with that id.
#[test]
fn pack_upgrade_keeps_a_new_opt_in_entry_excluded_when_a_local_entry_carries_its_id() {
    let (dir, _file) = before_pack_upgrade_that_adds_an_opt_in_entry();
    write_a_local_entry_with_the_id(dir.path(), "demo.three");
    let output = run_ok(dir.path(), &["pack", "upgrade", "demo"]);
    assert_eq!(
        lines(&output),
        expected(&[
            "upgraded pack demo from 1.0.0 to 1.1.0",
            "added id demo.three",
            "moved demo.three from knowledge/local.json to knowledge/overrides.json: pack demo ships it, and the override sets summary",
            "removed knowledge/local.json: installed packs ship every entry in it",
            &offer_line("demo.three"),
            "render: up to date",
        ])
    );
    assert_eq!(
        exclusions(dir.path()),
        vec![opt_in_exclusion("demo.one"), opt_in_exclusion("demo.three")]
    );
    assert_eq!(
        excluded_reason(dir.path(), "demo.three"),
        Some(json!("opt-in"))
    );
}

/// An untaken offer is the data of houserules, not of the project: `pack
/// upgrade` to a version that drops the entry withdraws the offer, as `pack
/// remove` does, instead of refusing for an exclusion that the project never
/// wrote. The exclusion and the record go, and the base stays clean.
#[test]
fn pack_upgrade_withdraws_an_untaken_offer_that_the_new_version_drops() {
    let (dir, _file) = before_pack_upgrade_that_drops_an_offer();
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion("demo.one")]);
    let output = run_ok(dir.path(), &["pack", "upgrade", "demo"]);
    assert_eq!(
        lines(&output),
        expected(&[
            "upgraded pack demo from 1.0.0 to 1.1.0",
            "dropped id demo.one",
            "render: up to date",
        ])
    );
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), Vec::<String>::new());
    run_ok(dir.path(), &["check-knowledge"]);
    run_ok(dir.path(), &["render", "--check"]);
}

/// A stop at the write that removes the withdrawn exclusion leaves an
/// exclusion that no pack ships; the same `pack upgrade` removes it.
#[test]
fn a_pack_upgrade_that_stops_at_the_withdrawal_of_an_offer_finishes_at_the_next_upgrade() {
    assert_a_stop_at_leaves_no_entry_live_and_a_rerun_finishes(
        before_pack_upgrade_that_drops_an_offer,
        Command::Upgrade,
        &Lock::File("knowledge/overrides.json"),
        &["demo.one"],
    );
}

/// Pin of unchanged behavior: an exclusion that the project wrote for its own
/// reason is the project's data. `pack upgrade` to a version that drops the
/// entry refuses as it did, names the id, and changes nothing.
#[test]
fn pack_upgrade_refuses_a_dropped_entry_that_the_project_excluded_for_its_own_reason() {
    let (dir, _file) = before_pack_upgrade_that_drops_an_offer();
    let mut document = read_json(dir.path(), "knowledge/overrides.json");
    document["exclusions"] = json!([{"id": "demo.one", "reason": "We write the trailer by hand."}]);
    write_json(dir.path(), "knowledge/overrides.json", &document);
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["pack", "upgrade", "demo"]);
    assert_output(
        &output,
        2,
        "",
        "cannot upgrade pack demo: knowledge/overrides.json names demo.one, which version 1.1.0 drops; remove those overrides and exclusions first\n",
    );
    assert_same_tree(&before, &snapshot(dir.path()), "pack upgrade demo");
}

/// Spec 3.7: an `optIn` id must be an entry that the pack ships, so a typo
/// cannot leave a dead exclusion. `pack add` refuses with one line and
/// changes nothing.
#[test]
fn a_pack_that_lists_an_opt_in_id_it_does_not_ship_is_refused() {
    let dir = install();
    let file = vendor(
        dir.path(),
        &demo_pack("1.0.0", &["demo.one"], &["demo.missing"]),
    );
    let before = snapshot(dir.path());
    let output = run(dir.path(), &["pack", "add", &file]);
    assert_output(
        &output,
        2,
        "",
        &format!("{file}: optIn id \"demo.missing\" is not an entry of this pack\n"),
    );
    assert_same_tree(&before, &snapshot(dir.path()), "pack add");
}
