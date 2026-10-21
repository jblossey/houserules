//! The every-commit entry of the process pack (batch 34, spec 3.7): the kit
//! ships `process.every-commit-names-a-work-item` as an opt-in entry, so every
//! path that brings the pack onto an install excludes it with the reason
//! `opt-in`, and a project that deletes the exclusion has a `commits` check
//! that fails a commit with no `Work-Item:` trailer. Each case runs the
//! compiled binary against a scratch git repository.
//!
//! The mechanism itself (`pack add`, `pack upgrade`, `pack remove`) is tested
//! with a pack of its own in `opt_in.rs`; this file tests the kit's entry and
//! the one path that only a kit pack takes, the pin move of `update`.

mod tracker_support;
mod tree_snapshot;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tracker_support::{
    assert_output, commit_all, edit_marker, git, read_json, run, run_ok, scratch_git_repo, stderr,
    stdout, write_json,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The id of the every-commit entry of the process pack.
const ENTRY: &str = "process.every-commit-names-a-work-item";

/// The line that says an offered entry is excluded until the project deletes
/// its exclusion.
fn offer_line(id: &str) -> String {
    format!(
        "{id} is an opt-in entry; knowledge/overrides.json excludes it until you delete its exclusion"
    )
}

/// The lines that `output` printed on stdout.
fn lines(output: &std::process::Output) -> Vec<String> {
    stdout(output).lines().map(str::to_string).collect()
}

/// A scratch install with the JSON tracker and no process pack: a plain
/// `init` installs no process pack (spec 4.6), so `pack add process` is the
/// first act that brings the pack onto the install.
fn install_without_the_process_pack() -> TempDir {
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

/// Spec 3.7: `pack add process` writes the exclusion of the every-commit
/// entry with the reason `opt-in`, records the id in `optInOffered`, and
/// prints the line that says so. The entry stays out of the base, so
/// `check-knowledge` and `render --check` pass, and `get` shows it excluded.
#[test]
fn pack_add_excludes_the_every_commit_entry_and_records_the_offer() {
    let dir = install_without_the_process_pack();
    let output = run_ok(dir.path(), &["pack", "add", "process"]);
    let printed = lines(&output);
    assert!(
        printed.contains(&offer_line(ENTRY)),
        "the offer line is missing from {printed:?}"
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion(ENTRY)]);
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
    run_ok(dir.path(), &["check-knowledge"]);
    run_ok(dir.path(), &["render", "--check"]);
    let entry = run_ok(dir.path(), &["get", ENTRY]);
    let record: Value = serde_json::from_str(&stdout(&entry)).expect("get prints JSON");
    assert_eq!(record[0]["excluded"], json!("opt-in"));
}

/// Spec 3.7: the process pack that the kit ships names the entry under
/// `optIn`, at the pack schema that names the key, and the entry is a
/// `commits` check with the three fields of the spec. The whole check is
/// compared: no other field may stand beside them.
#[test]
fn the_process_pack_ships_the_every_commit_entry_as_an_opt_in_commits_check() {
    let dir = install_without_the_process_pack();
    run_ok(dir.path(), &["pack", "add", "process"]);
    let copy = read_json(dir.path(), "knowledge/packs/process.json");
    assert_eq!(copy["schema"], json!("houserules/pack@1.1.0"));
    assert_eq!(copy["optIn"], json!([ENTRY]));
    let entry = copy["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == json!(ENTRY))
        .expect("the pack ships the entry");
    assert_eq!(
        entry["check"],
        json!({
            "type": "commits",
            "level": "fail",
            "require_trailer": "Work-Item",
            "exempt_authors": ["dependabot[bot]", "release-please[bot]", "github-actions[bot]"],
            "exempt_subjects": ["^chore\\(.+\\): release "],
        })
    );
    assert_eq!(entry["kind"], json!("rule"));
    assert_eq!(entry["standing"], json!(true));
}

/// The process pack as a batch-33 install holds it: the version 1.0.2 copy
/// and pin, with no every-commit entry, no `optIn` key, and no record of an
/// offer. The kit moves such a pin to the version of the binary on `update`.
fn as_a_batch_33_install_holds_it(dir: &Path) {
    let mut copy = read_json(dir, "knowledge/packs/process.json");
    copy["entries"]
        .as_array_mut()
        .expect("entries")
        .retain(|entry| entry["id"] != json!(ENTRY));
    copy.as_object_mut().expect("an object").remove("optIn");
    copy["version"] = json!("1.0.2");
    copy["schema"] = json!("houserules/pack@1.0.0");
    write_json(dir, "knowledge/packs/process.json", &copy);
    let mut canonical = copy.clone();
    canonical.sort_all_objects();
    let sha: String = Sha256::digest(serde_json::to_vec(&canonical).expect("serialize"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    edit_marker(dir, |marker| {
        let pin = marker
            .get_mut("packs")
            .and_then(|packs| packs.get_mut("process"))
            .expect("the install pins the process pack");
        pin["version"] = json!("1.0.2");
        pin["sha256"] = json!(sha);
        marker.remove("optInOffered");
    });
    let overrides = dir.join("knowledge/overrides.json");
    if overrides.exists() {
        fs::remove_file(overrides).expect("remove overrides.json");
    }
}

/// A scratch install with the JSON tracker whose process pack is as batch 33
/// left it.
fn batch_33_install() -> TempDir {
    let dir = install_without_the_process_pack();
    run_ok(dir.path(), &["pack", "add", "process"]);
    as_a_batch_33_install_holds_it(dir.path());
    dir
}

/// Spec 3.7 and live case 6: `update` moves the pin of the process pack and
/// brings the every-commit entry onto the install with the exclusion and the
/// record of the offer, in the same move.
#[test]
fn update_moves_the_pin_and_offers_the_every_commit_entry() {
    let dir = batch_33_install();
    let output = run_ok(dir.path(), &["update"]);
    let printed = lines(&output);
    assert!(
        printed
            .iter()
            .any(|line| line.starts_with("moved pack process from 1.0.2 to ")),
        "the move line is missing from {printed:?}"
    );
    assert!(
        printed.contains(&offer_line(ENTRY)),
        "the offer line is missing from {printed:?}"
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion(ENTRY)]);
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
    let marker = read_json(dir.path(), ".houserules.json");
    assert_ne!(marker["packs"]["process"]["version"], json!("1.0.2"));
    run_ok(dir.path(), &["check-knowledge"]);
}

/// Spec 3.7: a project that deleted the exclusion has opted in for good. A
/// later move of the pin writes no exclusion again, prints no offer line, and
/// leaves `optInOffered` as it was.
#[test]
fn update_never_offers_an_entry_the_project_already_took() {
    let dir = batch_33_install();
    run_ok(dir.path(), &["update"]);
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion(ENTRY)]);
    opt_in(dir.path(), ENTRY);
    as_a_batch_33_install_holds_it(dir.path());
    edit_marker(dir.path(), |marker| {
        marker.insert("optInOffered".to_string(), json!([ENTRY]));
    });

    let output = run_ok(dir.path(), &["update"]);
    let printed = lines(&output);
    assert!(
        printed
            .iter()
            .any(|line| line.starts_with("moved pack process from 1.0.2 to ")),
        "the pin moved: {printed:?}"
    );
    assert!(
        !printed.contains(&offer_line(ENTRY)),
        "nothing is offered twice: {printed:?}"
    );
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
}

/// Spec 3.7 and live case 6: a second `update` after the project deleted the
/// exclusion and rendered writes nothing: no exclusion, no change of
/// `optInOffered`, and no other file changes.
#[test]
fn a_second_update_after_the_project_deleted_the_exclusion_writes_nothing() {
    let dir = batch_33_install();
    run_ok(dir.path(), &["update"]);
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion(ENTRY)]);
    opt_in(dir.path(), ENTRY);
    run_ok(dir.path(), &["render"]);
    let before = snapshot(dir.path());
    let output = run_ok(dir.path(), &["update"]);
    assert_same_tree(&before, &snapshot(dir.path()), "the second update");
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
    assert_eq!(stderr(&output), "");
}

/// An install with the process pack whose project opted in to the every-commit
/// entry, and a base commit.
fn opted_in_install() -> (TempDir, String) {
    let dir = install_without_the_process_pack();
    run_ok(dir.path(), &["pack", "add", "process"]);
    opt_in(dir.path(), ENTRY);
    let base = commit_all(dir.path(), "chore: install the kit");
    (dir, base)
}

/// An empty commit by `author` with `subject`, and `trailers` as its last
/// paragraph. Returns the short sha.
fn commit_as(dir: &Path, author: &str, subject: &str, trailers: &[&str]) -> String {
    let mut args = vec![
        "commit".to_string(),
        "-q".to_string(),
        "--no-verify".to_string(),
        "--allow-empty".to_string(),
        "--author".to_string(),
        format!("{author} <author@example.com>"),
        "-m".to_string(),
        subject.to_string(),
    ];
    if !trailers.is_empty() {
        args.push("-m".to_string());
        args.push(trailers.join("\n"));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    git(dir, &refs);
    git(dir, &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_string()
}

/// Spec 3.7 and live case 6: with the exclusion deleted, `check-commit` fails
/// a human `feat` commit that has no `Work-Item:` trailer, with the line of
/// the spec, passes a commit that names its item, passes a commit by a bot of
/// the exempt list, and passes a release commit that a human authored.
#[test]
fn check_commit_applies_the_entry_once_the_project_opts_in() {
    let (dir, base) = opted_in_install();
    let bad = commit_as(dir.path(), "A Human", "feat: add the lookup", &[]);
    commit_as(
        dir.path(),
        "A Human",
        "feat: add the cache",
        &["Work-Item: WI-001"],
    );
    commit_as(
        dir.path(),
        "dependabot[bot]",
        "chore(deps): bump serde",
        &[],
    );
    commit_as(dir.path(), "A Human", "chore(main): release 2.0.0", &[]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(
        &output,
        1,
        "",
        &format!("{bad}: no Work-Item trailer ({ENTRY})\n"),
    );
}

/// Spec 3.7: an install that has not opted in is unaffected. The entry is
/// excluded after `pack add process`, so a commit with no trailer passes.
#[test]
fn check_commit_leaves_a_commit_with_no_trailer_alone_until_the_project_opts_in() {
    let dir = install_without_the_process_pack();
    run_ok(dir.path(), &["pack", "add", "process"]);
    let base = commit_all(dir.path(), "chore: install the kit");
    commit_as(dir.path(), "A Human", "feat: add the lookup", &[]);
    let output = run(dir.path(), &["check-commit", "--from", &base]);
    assert_output(&output, 0, "check-commit: ok\n", "");
}

/// Spec 3.7: in the message-file mode of the commit-msg hook there is no
/// author yet, so only the subject exemption applies.
#[test]
fn check_commit_message_file_mode_applies_the_subject_exemption_only() {
    let (dir, _base) = opted_in_install();
    let messages = tempfile::tempdir().expect("tempdir");
    let check = |text: &str, author: &str| {
        let file = messages.path().join("COMMIT_EDITMSG");
        fs::write(&file, text).expect("write the message");
        let mut command = tracker_support::houserules();
        command
            .args(["check-commit", file.to_str().expect("utf-8 path")])
            .arg("--dir")
            .arg(dir.path())
            .env("GIT_AUTHOR_NAME", author);
        command.output().expect("run check-commit")
    };
    let failed = check("feat: add the lookup\n", "dependabot[bot]");
    assert_output(&failed, 1, "", &format!("no Work-Item trailer ({ENTRY})\n"));
    let release = check("chore(main): release 2.0.0\n", "A Human");
    assert_output(&release, 0, "check-commit: ok\n", "");
    let named = check("feat: add the lookup\n\nWork-Item: WI-001\n", "A Human");
    assert_output(&named, 0, "check-commit: ok\n", "");
}

/// Spec 3.7 and live case 6, first half: a new install that brings the
/// process pack in with `init --workflow reference --add-required-packs` has
/// the every-commit entry excluded with the reason `opt-in`, `optInOffered`
/// holds it, and the offer line is printed once.
#[test]
fn a_new_install_with_the_process_pack_has_the_entry_excluded_and_offered() {
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
    let offers = lines(&output)
        .into_iter()
        .filter(|line| *line == offer_line(ENTRY))
        .count();
    assert_eq!(offers, 1, "{}", stdout(&output));
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion(ENTRY)]);
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
    run_ok(dir.path(), &["check-knowledge"]);
    run_ok(dir.path(), &["render", "--check"]);
}

/// A plain `init` installs no process pack, so a new install has no live
/// entry: nothing is excluded and nothing is offered (spec 4.6).
#[test]
fn a_plain_init_has_no_every_commit_entry_and_offers_nothing() {
    let dir = scratch_git_repo();
    let output = run_ok(dir.path(), &["init"]);
    assert!(!stdout(&output).contains(ENTRY), "{}", stdout(&output));
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), Vec::<String>::new());
}

/// Spec 3.7 and live case 6, last half: `pack remove process` then `pack add
/// process` on an install whose project opted in to the entry prints the
/// active line, writes no exclusion, and keeps the record of the offer.
#[test]
fn pack_remove_then_pack_add_prints_the_active_line_once_the_project_opted_in() {
    let (dir, _base) = opted_in_install();
    run_ok(dir.path(), &["pack", "remove", "process"]);
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
    let output = run_ok(dir.path(), &["pack", "add", "process"]);
    let printed = lines(&output);
    assert!(
        printed.contains(&format!("{ENTRY} is active: this install opted in before")),
        "{printed:?}"
    );
    assert!(!printed.contains(&offer_line(ENTRY)), "{printed:?}");
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
}

/// A project that has not opted in gets the offer again after `pack remove
/// process` and `pack add process`: the removal took the exclusion and the
/// record back.
#[test]
fn pack_remove_then_pack_add_offers_the_entry_again_while_the_project_has_not_opted_in() {
    let dir = install_without_the_process_pack();
    run_ok(dir.path(), &["pack", "add", "process"]);
    run_ok(dir.path(), &["pack", "remove", "process"]);
    assert_eq!(exclusions(dir.path()), Vec::<Value>::new());
    assert_eq!(offered(dir.path()), Vec::<String>::new());
    let output = run_ok(dir.path(), &["pack", "add", "process"]);
    assert!(
        lines(&output).contains(&offer_line(ENTRY)),
        "{}",
        stdout(&output)
    );
    assert_eq!(exclusions(dir.path()), vec![opt_in_exclusion(ENTRY)]);
    assert_eq!(offered(dir.path()), vec![ENTRY.to_string()]);
}
