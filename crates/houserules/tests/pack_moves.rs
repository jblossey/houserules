//! The pack moves of `update` on a 2.0 install (spec section 5, "Pack moves
//! on a 2.0 install"), end to end through the compiled binary. The binary
//! carries one version of each kit pack, so each case makes an older copy and
//! pin by hand: the state that an install holds after an earlier release of
//! the kit.

mod locked_file;
mod tree_snapshot;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// The path `relative` below `dir`, built one component at a time.
fn path_of(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
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

/// Runs `houserules <args> --dir <dir>`, requires exit 0, and returns the
/// stdout lines.
fn lines_of(dir: &Path, args: &[&str]) -> Vec<String> {
    let output = run(dir, args);
    assert_eq!(
        output.status.code(),
        Some(0),
        "houserules {args:?} failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8 stdout")
        .lines()
        .map(str::to_string)
        .collect()
}

/// A scratch repository that `houserules init --target claude --tracker json`
/// installed, with the four kit packs that a plain `init` leaves out added by
/// `pack add`: all five kit packs are pinned copies.
fn install() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    lines_of(
        dir.path(),
        &["init", "--target", "claude", "--tracker", "json"],
    );
    for pack in ["process", "quality", "security-hygiene", "writing-style"] {
        lines_of(dir.path(), &["pack", "add", pack]);
    }
    dir
}

fn read_json(dir: &Path, relative: &str) -> Value {
    serde_json::from_str(&fs::read_to_string(path_of(dir, relative)).expect("read a JSON file"))
        .expect("parse a JSON file")
}

fn write_json(dir: &Path, relative: &str, value: &Value) {
    let path = path_of(dir, relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
    fs::write(
        path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(value).expect("serialize")
        ),
    )
    .expect("write a JSON file");
}

/// The `sha256` pin of a pack copy, as the binary computes it: the SHA-256 of
/// the compact JSON of the parsed copy with every object's keys sorted.
fn pin_of(copy: &Value) -> String {
    let mut canonical = copy.clone();
    canonical.sort_all_objects();
    Sha256::digest(serde_json::to_vec(&canonical).expect("serialize"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

/// The version of the kit pack `process` that this binary carries.
fn process_version() -> String {
    kit_pack("process")["version"]
        .as_str()
        .expect("a version")
        .to_string()
}

/// Turns the copy of the kit pack `name` into an older release: `edit` changes
/// the copy, the version becomes `0.9.0`, and the pin follows the copy.
/// `added_areas` replaces the areas that the pin records.
fn make_older(dir: &Path, name: &str, added_areas: Option<Value>, edit: impl FnOnce(&mut Value)) {
    let relative = format!("knowledge/packs/{name}.json");
    let mut copy = read_json(dir, &relative);
    edit(&mut copy);
    copy["version"] = json!("0.9.0");
    write_json(dir, &relative, &copy);
    let mut marker = read_json(dir, ".houserules.json");
    marker["packs"][name]["version"] = json!("0.9.0");
    marker["packs"][name]["sha256"] = json!(pin_of(&copy));
    if let Some(areas) = added_areas {
        marker["packs"][name]["addedAreas"] = areas;
    }
    write_json(dir, ".houserules.json", &marker);
}

/// Removes the entry `id` from the pack copy `copy`.
fn drop_entry(copy: &mut Value, id: &str) {
    copy["entries"]
        .as_array_mut()
        .expect("entries")
        .retain(|entry| entry["id"] != json!(id));
}

/// A pack of one topic `process`, named `name`, that ships `process.tdd` with
/// a text of its own: an upstream pack that shares an id with the kit's
/// `process` pack.
fn upstream_pack(name: &str) -> Value {
    let mut entry = kit_pack("process")["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == json!("process.tdd"))
        .expect("process.tdd")
        .clone();
    entry["summary"] = json!("The team's own test rule.");
    json!({
        "name": name,
        "version": "1.0.0",
        "schema": "houserules/pack@1.0.0",
        "topics": {"process": {"title": "Process"}},
        "areas": {"global": {"paths": []}},
        "entries": [entry],
    })
}

/// A line that `update` prints for a change: not the render line or the
/// three closing lines.
fn change_lines(lines: &[String]) -> Vec<&String> {
    lines
        .iter()
        .filter(|line| {
            !(line.starts_with("render: ")
                || line.starts_with("kit ")
                || line.starts_with("houserules: updated ")
                || line.starts_with("next: "))
        })
        .collect()
}

#[test]
fn update_moves_an_older_kit_pack_to_the_version_of_the_binary() {
    let dir = install();
    make_older(dir.path(), "process", None, |copy| {
        drop_entry(copy, "process.tdd");
    });

    let lines = lines_of(dir.path(), &["update"]);

    assert!(
        lines.contains(&format!(
            "moved pack process from 0.9.0 to {}",
            process_version()
        )),
        "{lines:?}"
    );
    let kit = kit_pack("process");
    assert_eq!(read_json(dir.path(), "knowledge/packs/process.json"), kit);
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["packs"]["process"]["version"], kit["version"]);
    assert_eq!(marker["packs"]["process"]["sha256"], json!(pin_of(&kit)));
    assert_eq!(marker["packs"]["process"]["source"], json!("kit"));
    assert_eq!(run(dir.path(), &["check-knowledge"]).status.code(), Some(0));
}

#[test]
fn a_second_update_after_a_move_prints_no_change() {
    let dir = install();
    make_older(dir.path(), "process", None, |copy| {
        drop_entry(copy, "process.tdd");
    });
    lines_of(dir.path(), &["update"]);

    let lines = lines_of(dir.path(), &["update"]);

    assert_eq!(change_lines(&lines), Vec::<&String>::new(), "{lines:?}");
}

#[test]
fn a_move_keeps_the_overrides_and_the_exclusions() {
    let dir = install();
    make_older(dir.path(), "process", None, |copy| {
        drop_entry(copy, "process.tdd");
    });
    let overrides = json!({
        "schema": "houserules/overrides@1.0.0",
        "overrides": [{"id": "process.model-policy", "fields": {"summary": "Our model policy."}}],
        "exclusions": [{"id": "process.risk-tiers", "reason": "not for us"}],
    });
    write_json(dir.path(), "knowledge/overrides.json", &overrides);
    lines_of(dir.path(), &["render"]);

    lines_of(dir.path(), &["update"]);

    assert_eq!(read_json(dir.path(), "knowledge/overrides.json"), overrides);
}

/// The older quality copy declared the area `legacy` and not `tools`; the
/// project's `areas.json` holds `legacy` (added by the pack) and no `tools`.
/// The move to the version of the binary adds `tools` and removes `legacy`.
fn an_older_quality_that_declared_legacy_and_not_tools(dir: &Path) {
    make_older(dir, "quality", Some(json!(["legacy"])), |copy| {
        copy["areas"] = json!({"legacy": {"paths": []}});
    });
    let mut areas = read_json(dir, "knowledge/areas.json");
    areas.as_object_mut().expect("areas").remove("tools");
    areas["legacy"] = json!({"paths": []});
    write_json(dir, "knowledge/areas.json", &areas);
    let mut copy = read_json(dir, "knowledge/packs/quality.json");
    for entry in copy["entries"].as_array_mut().expect("entries") {
        if entry["area"] == json!("tools") {
            entry["area"] = json!("global");
        }
    }
    write_json(dir, "knowledge/packs/quality.json", &copy);
    let mut marker = read_json(dir, ".houserules.json");
    marker["packs"]["quality"]["sha256"] = json!(pin_of(&copy));
    write_json(dir, ".houserules.json", &marker);
}

/// The older quality copy declared `legacy` and `tools`, and both are in
/// `areas.json` and in the record of the pin. The move to the version of the
/// binary adds no area and removes `legacy`.
fn an_older_quality_that_declared_legacy_and_tools(dir: &Path) {
    make_older(dir, "quality", Some(json!(["legacy", "tools"])), |copy| {
        copy["areas"] = json!({"legacy": {"paths": []}, "tools": {"paths": ["tools/**"]}});
    });
    let mut areas = read_json(dir, "knowledge/areas.json");
    areas["legacy"] = json!({"paths": []});
    write_json(dir, "knowledge/areas.json", &areas);
}

/// The older process copy lacks `process.tdd` and declares the same areas as
/// the version of the binary. The move changes the copy and the pin only.
fn an_older_process_without_tdd(dir: &Path) {
    make_older(dir, "process", None, |copy| {
        drop_entry(copy, "process.tdd");
    });
}

/// Interrupts `update` at the write to the file `locked` (below the install
/// root), and finishes the move with one more `update`. `prepare` changes the
/// installed kit of the case and of its control. A run that no fault stopped
/// leaves the tree that the finished run must reach (spec C20): the pin, its
/// `addedAreas`, the copy, and `areas.json`. The case ends at once for a user
/// with the right to write anywhere (`root`), whom a read-only file does not
/// stop.
fn assert_finishes_after_a_stop_at(locked: &str, prepare: impl Fn(&Path)) {
    let control = install();
    prepare(control.path());
    lines_of(control.path(), &["update"]);

    let dir = install();
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
    lines_of(dir.path(), &["update"]);

    tree_snapshot::assert_same_tree(
        &tree_snapshot::snapshot(control.path()),
        &tree_snapshot::snapshot(dir.path()),
        &format!("the move that stopped at the write to {locked}"),
    );
}

/// The move adds the area `tools`: the write to `areas.json` stops the run
/// before the copy and the version of the pin move.
#[test]
fn an_update_that_stops_at_the_areas_of_a_move_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        "knowledge/areas.json",
        an_older_quality_that_declared_legacy_and_not_tools,
    );
}

/// The write of the new copy stops the run after the move added `tools`: the
/// next run must still record `tools` as added by the pack.
#[test]
fn an_update_that_stops_at_the_copy_of_a_move_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        "knowledge/packs/quality.json",
        an_older_quality_that_declared_legacy_and_not_tools,
    );
}

/// The move removes the area `legacy`: the write to `areas.json` stops the
/// run after the new copy is written.
#[test]
fn an_update_that_stops_at_the_removal_of_an_area_of_a_move_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        "knowledge/areas.json",
        an_older_quality_that_declared_legacy_and_tools,
    );
}

/// The write to the marker stops the run after `legacy` left `areas.json`:
/// the record of the pin still names it.
#[test]
fn an_update_that_stops_at_the_record_of_a_removed_area_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(
        ".houserules.json",
        an_older_quality_that_declared_legacy_and_tools,
    );
}

/// The move changes the copy and the pin only: the write to the marker stops
/// the run after the new copy is written.
#[test]
fn an_update_that_stops_at_the_pin_of_a_move_finishes_at_the_next_update() {
    assert_finishes_after_a_stop_at(".houserules.json", an_older_process_without_tdd);
}

#[test]
fn a_move_adds_the_areas_the_new_version_declares_and_removes_the_ones_it_drops() {
    let dir = install();
    an_older_quality_that_declared_legacy_and_not_tools(dir.path());

    let lines = lines_of(dir.path(), &["update"]);

    assert!(
        lines.contains(
            &"added area \"tools\" to knowledge/areas.json with the globs tools/**".to_string()
        ),
        "{lines:?}"
    );
    assert!(
        lines.contains(
            &"removed area \"legacy\" from knowledge/areas.json; it had no globs".to_string()
        ),
        "{lines:?}"
    );
    let areas = read_json(dir.path(), "knowledge/areas.json");
    assert!(areas.get("tools").is_some() && areas.get("legacy").is_none());
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["packs"]["quality"]["addedAreas"],
        json!(["tools"])
    );
}

#[test]
fn a_new_version_that_ships_an_id_of_an_upstream_pack_stays_at_its_old_pin_with_a_hand_step() {
    let dir = install();
    make_older(dir.path(), "process", None, |copy| {
        drop_entry(copy, "process.tdd");
    });
    write_json(dir.path(), "vendor/team.json", &upstream_pack("team"));
    let file = path_of(dir.path(), "vendor/team.json");
    lines_of(
        dir.path(),
        &["pack", "add", file.to_str().expect("a UTF-8 path")],
    );
    let before = tree_snapshot::snapshot(dir.path());

    let lines = lines_of(dir.path(), &["update"]);

    assert!(
        lines.contains(&format!("hand step: pack process {} ships process.tdd, which the installed pack team ships too, so houserules update kept process at 0.9.0; edit vendor/team.json to drop the id and run houserules pack upgrade team, or run houserules pack remove team, then run houserules update again", process_version())),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.starts_with("moved pack ")),
        "{lines:?}"
    );
    let after = tree_snapshot::snapshot(dir.path());
    for kept in [
        "knowledge/packs/process.json",
        "knowledge/packs/team.json",
        "knowledge/areas.json",
    ] {
        assert_eq!(before.get(kept), after.get(kept), "{kept} changed");
    }
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["packs"]["process"]["version"],
        json!("0.9.0")
    );

    // The next update tests the state again: with the upstream pack gone, the
    // kit pack moves.
    lines_of(dir.path(), &["pack", "remove", "team"]);
    let lines = lines_of(dir.path(), &["update"]);
    assert!(
        lines.contains(&format!(
            "moved pack process from 0.9.0 to {}",
            process_version()
        )),
        "{lines:?}"
    );
}

/// The hand step of `update` for a kit pack whose new version shares an id
/// with the installed pack `team`, when the pin of `team` records `source`.
/// `update` fetches nothing, so the source needs no repository behind it.
fn shared_id_hand_step_for_a_team_pin_with_the_source(source: &str) -> String {
    let dir = install();
    make_older(dir.path(), "process", None, |copy| {
        drop_entry(copy, "process.tdd");
    });
    write_json(dir.path(), "vendor/team.json", &upstream_pack("team"));
    let file = path_of(dir.path(), "vendor/team.json");
    lines_of(
        dir.path(),
        &["pack", "add", file.to_str().expect("a UTF-8 path")],
    );
    let mut marker = read_json(dir.path(), ".houserules.json");
    marker["packs"]["team"]["source"] = json!(source);
    write_json(dir.path(), ".houserules.json", &marker);

    let lines = lines_of(dir.path(), &["update"]);

    let steps: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with("hand step: "))
        .collect();
    assert_eq!(steps.len(), 1, "{lines:?}");
    steps[0].clone()
}

/// A pack with a git source takes a tag: the hand step names the command that
/// moves it to a version without the id.
#[test]
fn the_hand_step_for_a_shared_id_names_the_upgrade_to_a_tag_for_a_git_source() {
    assert_eq!(
        shared_id_hand_step_for_a_team_pin_with_the_source(
            "git+https://example.invalid/org/repo@v1.0.0#team.json"
        ),
        format!(
            "hand step: pack process {} ships process.tdd, which the installed pack team ships too, so houserules update kept process at 0.9.0; move team to a version that drops the id with houserules pack upgrade team <tag>, or run houserules pack remove team, then run houserules update again",
            process_version()
        )
    );
}

/// A pack with a file source takes no tag: `pack upgrade` copies the file
/// again, so the hand step has the project edit the file first.
#[test]
fn the_hand_step_for_a_shared_id_names_the_edit_of_the_file_for_a_file_source() {
    assert_eq!(
        shared_id_hand_step_for_a_team_pin_with_the_source("file:vendor/team.json"),
        format!(
            "hand step: pack process {} ships process.tdd, which the installed pack team ships too, so houserules update kept process at 0.9.0; edit vendor/team.json to drop the id and run houserules pack upgrade team, or run houserules pack remove team, then run houserules update again",
            process_version()
        )
    );
}

/// A pin with no upstream source (a kit source, or a source that is no source
/// form) has no upgrade to name: the hand step names the removal only.
#[test]
fn the_hand_step_for_a_shared_id_names_only_the_removal_for_a_pin_with_no_upstream_source() {
    for source in ["kit", "nonsense"] {
        assert_eq!(
            shared_id_hand_step_for_a_team_pin_with_the_source(source),
            format!(
                "hand step: pack process {} ships process.tdd, which the installed pack team ships too, so houserules update kept process at 0.9.0; run houserules pack remove team, then run houserules update again",
                process_version()
            ),
            "{source}"
        );
    }
}

#[test]
fn a_kit_copy_that_fails_its_pin_at_an_unchanged_version_is_kept_and_named() {
    let dir = install();
    let relative = "knowledge/packs/process.json";
    let mut copy = read_json(dir.path(), relative);
    copy["entries"][0]["summary"] = json!("A hand edit.");
    write_json(dir.path(), relative, &copy);
    let before = tree_snapshot::snapshot(dir.path());

    let lines = lines_of(dir.path(), &["update"]);

    assert!(
        lines.contains(&"knowledge/packs/process.json: the pack \"process\" differs from its pin in .houserules.json; put your change in knowledge/overrides.json and restore the copy".to_string()),
        "{lines:?}"
    );
    assert_eq!(
        before.get(relative),
        tree_snapshot::snapshot(dir.path()).get(relative),
        "update rewrote a copy that fails its pin"
    );
}

#[test]
fn a_kit_pin_with_no_copy_gets_its_copy_at_the_version_of_the_binary() {
    let dir = install();
    let relative = "knowledge/packs/process.json";
    let kit = fs::read(path_of(dir.path(), relative)).expect("read the copy");
    fs::remove_file(path_of(dir.path(), relative)).expect("delete the copy");

    let lines = lines_of(dir.path(), &["update"]);

    assert!(
        lines.contains(&format!(
            "wrote knowledge/packs/process.json ({})",
            process_version()
        )),
        "{lines:?}"
    );
    assert_eq!(fs::read(path_of(dir.path(), relative)).expect("read"), kit);
    assert_eq!(run(dir.path(), &["check-knowledge"]).status.code(), Some(0));
}

#[test]
fn update_installs_no_pack_the_project_removed() {
    let dir = install();
    lines_of(dir.path(), &["pack", "remove", "writing-style"]);

    let lines = lines_of(dir.path(), &["update"]);

    assert!(
        !path_of(dir.path(), "knowledge/packs/writing-style.json").exists(),
        "{lines:?}"
    );
    assert!(
        read_json(dir.path(), ".houserules.json")["packs"]
            .get("writing-style")
            .is_none()
    );
}

#[test]
fn an_upstream_pack_with_the_name_of_a_kit_pack_is_not_moved() {
    let dir = install();
    lines_of(dir.path(), &["pack", "remove", "process"]);
    // A version other than the kit's: a move would have a version to move to.
    let mut upstream = upstream_pack("process");
    upstream["version"] = json!("0.5.0");
    write_json(dir.path(), "vendor/process.json", &upstream);
    let file = path_of(dir.path(), "vendor/process.json");
    lines_of(
        dir.path(),
        &["pack", "add", file.to_str().expect("a UTF-8 path")],
    );
    let before = tree_snapshot::snapshot(dir.path());

    let lines = lines_of(dir.path(), &["update"]);

    assert_eq!(change_lines(&lines), Vec::<&String>::new(), "{lines:?}");
    tree_snapshot::assert_same_tree(
        &before,
        &tree_snapshot::snapshot(dir.path()),
        "update over an upstream pack with a kit name",
    );
}
