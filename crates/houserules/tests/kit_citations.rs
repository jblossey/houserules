//! `check-knowledge`'s kit-owned citation lint, end to end through the
//! compiled binary: a fresh install passes, a deleted or archived seeded
//! entry that kit-owned files cite fails with the whole finding line, an
//! override skips a file, each way the line prints clears the finding, a
//! topic the adopter adds never turns kit text into a finding, and the
//! seeded `docs/README.md` states the contract that two flows follow.
//! Follows `update.rs`'s pattern (a real subprocess against a real scratch
//! git repository, its own small copy of the helpers) for the reason that
//! file's module doc gives. The lint's own cases (what counts as an id,
//! skips, archive, unreadable input) are unit tests in
//! `src/rules/kit_citations.rs`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use regress::Regex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The entry every case below deletes. Two kit-owned files cite it:
/// `branch-reviewer.md` and the `orchestrating` skill.
const DELETED_ID: &str = "process.evals-rerun";

/// The topic file that holds `DELETED_ID` in a seeded install.
const ACTIVE_TOPIC_FILE: &str = "knowledge/process.json";

/// Where `houserules archive` moves `DELETED_ID` once it is retired.
const ARCHIVE_TOPIC_FILE: &str = "knowledge/archive/process.json";

/// The kit-owned files that cite `DELETED_ID`, in the order of the
/// `KIT_OWNED` list.
const CITING_FILES: [&str; 2] = [
    ".claude/agents/branch-reviewer.md",
    ".claude/skills/orchestrating/SKILL.md",
];

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off (`update.rs`'s own copy has the
/// account).
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// A scratch git repository already seeded by `houserules init`.
/// `tempfile`'s `TempDir` removes itself on drop.
fn seeded_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        output.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

/// Runs `houserules <subcommand> --dir <root>`.
fn run(root: &Path, subcommand: &str) -> Output {
    houserules()
        .arg(subcommand)
        .arg("--dir")
        .arg(root)
        .output()
        .unwrap_or_else(|error| panic!("run {subcommand}: {error}"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).expect("read JSON file")).expect("parse JSON")
}

fn write_json(path: &Path, value: &Value) {
    fs::write(
        path,
        serde_json::to_string_pretty(value).expect("serialize JSON"),
    )
    .expect("write JSON file");
}

/// Deletes `DELETED_ID` from `knowledge/process.json`, and from every `see`
/// list that names it, as an adopter pruning the entry does. Then renders,
/// so the generated files stay fresh and the lint is the only finding.
fn delete_the_entry(root: &Path) {
    let path = root.join(ACTIVE_TOPIC_FILE);
    let mut topic = read_json(&path);
    let entries = topic["entries"].as_array_mut().expect("entries array");
    entries.retain(|entry| entry["id"] != DELETED_ID);
    for entry in entries.iter_mut() {
        if let Some(see) = entry.get_mut("see").and_then(Value::as_array_mut) {
            see.retain(|id| id != DELETED_ID);
        }
    }
    write_json(&path, &topic);
    render(root);
}

/// Runs `houserules render` and fails the test when it fails.
fn render(root: &Path) {
    let output = run(root, "render");
    assert!(
        output.status.success(),
        "render failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Retires the entry `id` and sweeps it into `knowledge/archive/process.json`
/// with `houserules archive`, as an adopter retiring the entry does. Then
/// renders, so the generated files stay fresh and the lint is the only
/// finding.
fn archive_entry(root: &Path, id: &str) {
    let path = root.join(ACTIVE_TOPIC_FILE);
    let mut topic = read_json(&path);
    for entry in topic["entries"].as_array_mut().expect("entries array") {
        if entry["id"] == id {
            entry["status"] = json!("retired");
        }
    }
    write_json(&path, &topic);
    let output = run(root, "archive");
    assert!(
        output.status.success(),
        "archive failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    render(root);
}

/// Runs `houserules get <id> --dir <root>`.
fn run_get(root: &Path, id: &str) -> Output {
    houserules()
        .args(["get", id, "--dir"])
        .arg(root)
        .output()
        .expect("run get")
}

/// Whether `file` of `root` holds an entry with `DELETED_ID`.
fn holds_the_entry(root: &Path, file: &str) -> bool {
    read_json(&root.join(file))["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .any(|entry| entry["id"] == DELETED_ID)
}

/// The whole finding line for a file that cites `DELETED_ID`: the way that
/// restores the entry, then the way that keeps it deleted or archived.
fn finding_for(file: &str) -> String {
    format!(
        "{file}: cites \"process.evals-rerun\", which is not in the knowledge base; restore it: \
         in .houserules.json, delete \"process.evals-rerun\" from \"baselines\" and, if listed, \
         \"process.evals-rerun\" and \"knowledge/process.json\" from \"overrides\", then run \
         houserules update; or, to keep the entry deleted or archived, list \"{file}\" in \
         \"overrides\" in .houserules.json\n"
    )
}

/// Both lines `check-knowledge` prints after `delete_the_entry`, in the order
/// of the `KIT_OWNED` list.
fn both_findings() -> String {
    CITING_FILES.iter().map(|file| finding_for(file)).collect()
}

/// Edits the `.houserules.json` stamp of `root` in place.
fn edit_stamp(root: &Path, edit: impl FnOnce(&mut Value)) {
    let path = root.join(".houserules.json");
    let mut stamp = read_json(&path);
    edit(&mut stamp);
    write_json(&path, &stamp);
}

/// Follows the first way the printed line names: delete the id from
/// `baselines`, delete the id and the topic file from `overrides`, then run
/// `houserules update`. An install with no stamp has nothing to delete.
fn follow_the_remedy(root: &Path) {
    if root.join(".houserules.json").exists() {
        edit_stamp(root, |stamp| {
            stamp["baselines"]
                .as_object_mut()
                .expect("baselines object")
                .remove(DELETED_ID);
            if let Some(overrides) = stamp.get_mut("overrides").and_then(Value::as_array_mut) {
                overrides.retain(|item| item != DELETED_ID && item != "knowledge/process.json");
            }
        });
    }
    let output = run(root, "update");
    assert!(
        output.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The files the second way names: the quoted file of each `list "<file>" in
/// "overrides"` clause in the `cites` findings that `check-knowledge` prints,
/// in print order, once each.
fn files_the_findings_say_to_list(root: &Path) -> Vec<String> {
    let output = run(root, "check-knowledge");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let mut files: Vec<String> = Vec::new();
    for line in stderr.lines().filter(|line| line.contains(": cites \"")) {
        let (_, after) = line
            .rsplit_once("list \"")
            .unwrap_or_else(|| panic!("the finding names no file to list: {line}"));
        let (file, _) = after
            .split_once('"')
            .unwrap_or_else(|| panic!("the finding's listed file is not closed: {line}"));
        if !files.iter().any(|seen| seen == file) {
            files.push(file.to_string());
        }
    }
    files
}

/// Follows the second way the printed line names: list each file the
/// findings name in `overrides` of `.houserules.json`, and return them. An
/// install with no stamp gets a stamp that holds nothing but the list.
fn list_the_files_the_findings_name(root: &Path) -> Vec<String> {
    let files = files_the_findings_say_to_list(root);
    let path = root.join(".houserules.json");
    let mut stamp = if path.exists() {
        read_json(&path)
    } else {
        json!({})
    };
    let overrides = stamp
        .as_object_mut()
        .expect("the stamp is an object")
        .entry("overrides")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("overrides is an array");
    for file in &files {
        if !overrides.iter().any(|item| item == file.as_str()) {
            overrides.push(json!(file));
        }
    }
    write_json(&path, &stamp);
    files
}

/// The precondition of every remedy and override case: `check-knowledge`
/// fails with exit 1 and prints the finding for each of the two files that
/// cite `DELETED_ID`, among any other lines. The cases then prove a change
/// turns that failure into a pass, never a pass that held already.
fn assert_fails_with_both_findings(root: &Path) {
    let output = run(root, "check-knowledge");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    for file in CITING_FILES {
        assert!(
            stderr.contains(&finding_for(file)),
            "the finding for {file} is missing from: {stderr}"
        );
    }
}

fn assert_passes(root: &Path) {
    let output = run(root, "check-knowledge");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "",
        "check-knowledge printed findings"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "knowledge: ok\n");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn check_knowledge_passes_when_every_cited_id_resolves() {
    let dir = seeded_repo();
    assert_passes(dir.path());
    // The control: the same install fails once the entry is gone, so the
    // pass above ran the lint.
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
}

#[test]
fn check_knowledge_fails_a_kit_owned_citation_of_a_deleted_entry() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), both_findings());
    assert_eq!(output.status.code(), Some(1));
}

/// The adopter-visible behavior of an override: a kit-owned file the stamp
/// lists is the adopter's own, so a citation in it is no finding. The same
/// install fails before the override, so the pass is not vacuous.
#[test]
fn an_overridden_kit_owned_file_is_skipped() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!(CITING_FILES);
    });
    assert_passes(dir.path());
}

#[test]
fn the_printed_remedy_restores_a_deleted_entry() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
    follow_the_remedy(dir.path());
    assert_passes(dir.path());
}

/// With the id also listed in `overrides`, `update` leaves the entry
/// deleted. The printed remedy names that state, so following it restores
/// the entry.
#[test]
fn the_printed_remedy_restores_an_entry_the_stamp_overrides() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!([DELETED_ID]);
    });
    assert_fails_with_both_findings(dir.path());
    follow_the_remedy(dir.path());
    assert_passes(dir.path());
}

/// With the whole topic file deleted and its path listed in `overrides`,
/// `update` leaves the file absent. The printed remedy names that state too.
#[test]
fn the_printed_remedy_restores_a_deleted_topic_file_the_stamp_overrides() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join("knowledge/process.json")).expect("delete the topic file");
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!(["knowledge/process.json"]);
    });
    assert_fails_with_both_findings(dir.path());
    follow_the_remedy(dir.path());
    assert_passes(dir.path());
}

/// A stamp that `update` rejects is a finding too: the lint cannot read the
/// overrides, so it names the stamp and says no citation was checked.
#[test]
fn a_malformed_stamp_is_a_named_finding() {
    let dir = seeded_repo();
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!("process.tdd");
    });
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: overrides must be an array of strings; kit-owned citations not checked\n",
            dir.path().join(".houserules.json").display()
        )
    );
    assert_eq!(output.status.code(), Some(1));
}

/// The kit-owned skill the unseeded-id cases below put an older kit's text
/// into.
const FINISHING_SKILL: &str = ".claude/skills/finishing-a-feature/SKILL.md";

/// An id the payload never seeds, as an older kit's skill cited one.
const UNSEEDED_ID: &str = "process.cited-by-an-older-kit";

/// The skill as an older kit shipped it: the current text plus one line that
/// cites `UNSEEDED_ID`.
fn older_kit_skill() -> String {
    let current = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../template/.claude/skills/finishing-a-feature/SKILL.md"),
    )
    .expect("read the template skill");
    format!("{current}\nAn older kit cited `{UNSEEDED_ID}` here.\n")
}

/// Puts `older_kit_skill()` into `root`, stamped with its own hash as `update`
/// stamps a file it wrote, so the install looks like one that has not yet run
/// `update` with the new binary. With `local_edit` the file also carries one
/// line the adopter added after that stamp.
fn install_older_kit_skill(root: &Path, local_edit: bool) {
    let older = older_kit_skill();
    let text = if local_edit {
        format!("{older}A line the adopter added.\n")
    } else {
        older.clone()
    };
    fs::write(root.join(FINISHING_SKILL), text).expect("write the older skill");
    let hash: String = Sha256::digest(older.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    edit_stamp(root, |stamp| {
        stamp["baselines"][FINISHING_SKILL] = json!(hash);
    });
}

/// The whole finding line for `UNSEEDED_ID` cited by `FINISHING_SKILL`.
fn unseeded_finding() -> String {
    format!(
        "{FINISHING_SKILL}: cites \"{UNSEEDED_ID}\", which is not in the knowledge base; the kit \
         does not seed it, so no entry can be restored: run houserules update, and if update \
         prints \"kept {FINISHING_SKILL} (locally modified)\", remove the citation from \
         {FINISHING_SKILL} or list \"{FINISHING_SKILL}\" in \"overrides\" in .houserules.json\n"
    )
}

fn assert_fails_with_the_unseeded_finding(root: &Path) {
    let output = run(root, "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), unseeded_finding());
    assert_eq!(output.status.code(), Some(1));
}

/// Runs `houserules update` and returns what it printed.
fn update_output(root: &Path) -> String {
    let output = run(root, "update");
    assert!(
        output.status.success(),
        "update failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// State one of an unseeded id: the file is an older kit's text and the
/// adopter has not changed it, so `update` replaces it and the finding goes.
#[test]
fn the_printed_remedy_clears_an_unseeded_citation_in_an_unmodified_file() {
    let dir = seeded_repo();
    install_older_kit_skill(dir.path(), false);
    assert_fails_with_the_unseeded_finding(dir.path());
    let printed = update_output(dir.path());
    assert!(
        printed.contains(&format!("wrote {FINISHING_SKILL}")),
        "update did not rewrite the skill: {printed}"
    );
    assert_passes(dir.path());
}

/// State two of an unseeded id: the adopter changed the file, so `update`
/// keeps it and says so. The printed line then tells the adopter to remove
/// the citation.
#[test]
fn the_printed_remedy_clears_an_unseeded_citation_in_a_kept_file_by_editing_it_out() {
    let dir = seeded_repo();
    install_older_kit_skill(dir.path(), true);
    assert_fails_with_the_unseeded_finding(dir.path());
    let printed = update_output(dir.path());
    assert!(
        printed.contains(&format!("kept {FINISHING_SKILL} (locally modified)")),
        "update did not keep the skill: {printed}"
    );
    assert_fails_with_the_unseeded_finding(dir.path());
    let path = dir.path().join(FINISHING_SKILL);
    let kept: String = fs::read_to_string(&path)
        .expect("read the kept skill")
        .lines()
        .filter(|line| !line.contains(UNSEEDED_ID))
        .map(|line| format!("{line}\n"))
        .collect();
    fs::write(&path, kept).expect("write the edited skill");
    assert_passes(dir.path());
}

/// State two again, the other way out the printed line names: list the kept
/// file in `overrides`.
#[test]
fn the_printed_remedy_clears_an_unseeded_citation_in_a_kept_file_by_listing_it_in_overrides() {
    let dir = seeded_repo();
    install_older_kit_skill(dir.path(), true);
    assert_fails_with_the_unseeded_finding(dir.path());
    update_output(dir.path());
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!([FINISHING_SKILL]);
    });
    assert_passes(dir.path());
}

/// The seeded arm against a kit-owned file the adopter changed: `update`
/// keeps the file, and restoring the entry still clears both findings.
#[test]
fn the_printed_remedy_restores_a_deleted_entry_cited_by_a_locally_modified_file() {
    let dir = seeded_repo();
    let path = dir.path().join(CITING_FILES[0]);
    let mut text = fs::read_to_string(&path).expect("read the citing file");
    text.push_str("A line the adopter added.\n");
    fs::write(&path, text).expect("write the modified file");
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
    follow_the_remedy(dir.path());
    assert_passes(dir.path());
}

/// The checkout's `template/` directory: the kit text every install starts
/// from.
fn template_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template")
}

/// The topics the kit seeds: one for each topic file under
/// `template/knowledge/`, other than the schema and the areas.
fn seeded_topics() -> BTreeSet<String> {
    fs::read_dir(template_dir().join("knowledge"))
        .expect("read template/knowledge")
        .map(|entry| {
            entry
                .expect("read a directory entry")
                .file_name()
                .into_string()
                .expect("a UTF-8 file name")
        })
        .filter(|name| name != "schema.json" && name != "areas.json")
        .filter_map(|name| name.strip_suffix(".json").map(str::to_string))
        .collect()
}

/// Each kit-owned file under `template/` with its text, in the order
/// `houserules files` lists them under `kitOwned`.
fn kit_owned_template_texts() -> Vec<(String, String)> {
    let files = houserules().arg("files").output().expect("run files");
    assert!(files.status.success(), "files failed");
    let listed: Value = serde_json::from_slice(&files.stdout).expect("files prints JSON");
    listed["kitOwned"]
        .as_array()
        .expect("kitOwned is an array")
        .iter()
        .map(|file| {
            let file = file.as_str().expect("a path string");
            let text = fs::read_to_string(template_dir().join(file))
                .unwrap_or_else(|error| panic!("read template/{file}: {error}"));
            (file.to_string(), text)
        })
        .collect()
}

/// The prefix of every backticked, id-shaped token in the kit-owned files
/// under `template/` that is no seeded topic: the names an adopter could
/// give a topic to collide with kit text. The kit-owned list is the one
/// `houserules files` prints. The token shape is the id pattern of
/// `template/knowledge/schema.json`, read here again and not taken from the
/// lint, so the lint's own extractor is not its own oracle.
fn prefixes_the_kit_text_shows_outside(seeded: &BTreeSet<String>) -> BTreeSet<String> {
    let schema: Value = read_json(&template_dir().join("knowledge/schema.json"));
    let pattern = schema
        .pointer("/$defs/entry/properties/id/pattern")
        .and_then(Value::as_str)
        .expect("the schema carries the id pattern");
    let body = pattern.trim_start_matches('^').trim_end_matches('$');
    let backticked = Regex::new(&format!("`({body})(?=`)")).expect("compile the token shape");
    let mut prefixes = BTreeSet::new();
    for (_, text) in kit_owned_template_texts() {
        for found in backticked.find_iter(&text) {
            let token = &text[found.group(1).expect("the token group")];
            let prefix = token.split_once('.').map_or(token, |(prefix, _)| prefix);
            if !seeded.contains(prefix) {
                prefixes.insert(prefix.to_string());
            }
        }
    }
    prefixes
}

/// Writes the topic file an adopter would add for `topic`: one history entry
/// that the schema accepts.
fn add_adopter_topic(root: &Path, topic: &str) {
    write_json(
        &root.join(format!("knowledge/{topic}.json")),
        &json!({
            "topic": topic,
            "title": format!("The adopter's {topic} topic"),
            "entries": [{
                "id": format!("{topic}.adopter-note"),
                "kind": "history",
                "area": "docs",
                "summary": "A note the adopter wrote.",
                "body": ["The adopter chose this topic name."],
                "tags": ["adopter"],
                "source": {"date": "2026-10-02", "by": "user"},
            }],
        }),
    );
}

/// The invariant of the lint: a name the adopter chooses never turns
/// unchanged kit text into a finding. The kit text shows tokens such as
/// `source.date` and `implementer.md` in backticks, and they are field and
/// file names, not ids. Each such prefix becomes an adopter topic here, so
/// the lint sees a loaded topic of that name. The install stays green, and
/// `update`, which writes the same kit text again, leaves it green. The
/// control is the same install failing once a seeded entry that kit text
/// cites is deleted, so the pass is the lint run and not a lint that is off.
#[test]
fn an_adopter_topic_never_makes_kit_text_a_finding() {
    let dir = seeded_repo();
    let at_risk = prefixes_the_kit_text_shows_outside(&seeded_topics());
    assert!(
        !at_risk.is_empty(),
        "the kit text shows no token outside the seeded topics, so this test proves nothing"
    );
    for topic in &at_risk {
        add_adopter_topic(dir.path(), topic);
    }
    render(dir.path());
    assert_passes(dir.path());
    update_output(dir.path());
    assert_passes(dir.path());
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
}

/// A seeded entry that no kit-owned file under `template/` cites: the first
/// entry of `template/knowledge/process.json` whose id no kit-owned text
/// holds, in any form.
fn uncited_seeded_entry() -> String {
    let texts = kit_owned_template_texts();
    read_json(&template_dir().join("knowledge/process.json"))["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id string").to_string())
        .find(|id| texts.iter().all(|(_, text)| !text.contains(id.as_str())))
        .expect("the seed holds a process entry that no kit-owned file cites")
}

/// The way that keeps the entry deleted: list each citing file. The files
/// leave the lint, the entry stays out of its topic file, and `update`, which
/// leaves a listed file as found and respects the deletion, keeps both true.
#[test]
fn listing_the_citing_files_keeps_a_deleted_entry_deleted() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
    assert_eq!(list_the_files_the_findings_name(dir.path()), CITING_FILES);
    assert_passes(dir.path());
    assert!(!holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
    update_output(dir.path());
    assert_passes(dir.path());
    assert!(!holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
}

/// The archived state of the first way: the entry is active again, and it
/// stays in the archive file too. Neither `check-knowledge` nor a later
/// `archive` sweep reports the second copy.
#[test]
fn the_printed_remedy_restores_an_archived_entry_and_leaves_its_archive_copy() {
    let dir = seeded_repo();
    archive_entry(dir.path(), DELETED_ID);
    assert!(!holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
    assert!(holds_the_entry(dir.path(), ARCHIVE_TOPIC_FILE));
    assert_fails_with_both_findings(dir.path());
    follow_the_remedy(dir.path());
    assert_passes(dir.path());
    assert!(holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
    assert!(holds_the_entry(dir.path(), ARCHIVE_TOPIC_FILE));
    // A later sweep moves nothing: the restored entry is not retired.
    let sweep = run(dir.path(), "archive");
    assert_eq!(String::from_utf8_lossy(&sweep.stdout), "archive: 0 moved\n");
    assert_eq!(sweep.status.code(), Some(0));
    // `get` resolves the active entry first, so it carries no `archived`
    // label.
    let output = run_get(dir.path(), DELETED_ID);
    assert!(output.status.success(), "get failed");
    let records: Value = serde_json::from_slice(&output.stdout).expect("get prints JSON");
    assert_eq!(records.as_array().map(Vec::len), Some(1));
    assert!(records[0].get("archived").is_none(), "got {records}");
}

/// The archived state of the second way: the entry stays archived.
#[test]
fn listing_the_citing_files_keeps_an_archived_entry_archived() {
    let dir = seeded_repo();
    archive_entry(dir.path(), DELETED_ID);
    assert_fails_with_both_findings(dir.path());
    assert_eq!(list_the_files_the_findings_name(dir.path()), CITING_FILES);
    assert_passes(dir.path());
    assert!(!holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
    update_output(dir.path());
    assert_passes(dir.path());
    assert!(!holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
    assert!(holds_the_entry(dir.path(), ARCHIVE_TOPIC_FILE));
}

/// An install with no stamp has no baseline to delete. The first way then
/// reduces to `houserules update`, which writes the entry back and the stamp.
#[test]
fn the_printed_remedy_restores_a_deleted_entry_when_the_install_has_no_stamp() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    fs::remove_file(dir.path().join(".houserules.json")).expect("delete the stamp");
    assert_fails_with_both_findings(dir.path());
    follow_the_remedy(dir.path());
    assert_passes(dir.path());
    assert!(holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
    assert!(dir.path().join(".houserules.json").is_file());
}

/// An install with no stamp has no `overrides` to edit. The second way then
/// creates `.houserules.json` with the list, and the lint reads it.
#[test]
fn listing_the_citing_files_clears_a_deleted_entry_when_the_install_has_no_stamp() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    fs::remove_file(dir.path().join(".houserules.json")).expect("delete the stamp");
    assert_fails_with_both_findings(dir.path());
    assert_eq!(list_the_files_the_findings_name(dir.path()), CITING_FILES);
    assert_passes(dir.path());
    assert!(!holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
}

/// The second way, for a deleted entry whose id the stamp also lists in
/// `overrides`: listing the citing files clears the finding, and the entry
/// stays deleted.
#[test]
fn listing_the_citing_files_clears_a_deleted_entry_the_stamp_overrides() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!([DELETED_ID]);
    });
    assert_fails_with_both_findings(dir.path());
    assert_eq!(list_the_files_the_findings_name(dir.path()), CITING_FILES);
    assert_passes(dir.path());
    assert!(!holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
}

/// The second way, for a whole topic file that is deleted and listed in
/// `overrides`: the kit text cites more ids of that topic than the one entry
/// the other tests delete, so the findings name more files. Listing each of
/// them clears every citation finding. The deleted topic still leaves the
/// `see` links of other topics dangling, which `check-knowledge` reports
/// apart from the citation lint and listing files does not clear. The first
/// way, which restores the topic file, clears both
/// (`the_printed_remedy_restores_a_deleted_topic_file_the_stamp_overrides`).
#[test]
fn listing_the_files_the_findings_name_clears_the_citations_of_a_deleted_topic_file_only() {
    let dir = seeded_repo();
    fs::remove_file(dir.path().join(ACTIVE_TOPIC_FILE)).expect("delete the topic file");
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!([ACTIVE_TOPIC_FILE]);
    });
    assert_fails_with_both_findings(dir.path());
    let listed = list_the_files_the_findings_name(dir.path());
    assert!(
        listed.len() > CITING_FILES.len(),
        "the deleted topic is cited by more files than the two of one entry: {listed:?}"
    );
    for file in CITING_FILES {
        assert!(listed.iter().any(|name| name == file), "{file} not listed");
    }
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("cites"),
        "a citation finding is left: {stderr}"
    );
    for line in stderr.lines() {
        assert!(
            line.contains(": see \"") && line.ends_with("does not exist"),
            "a line other than a dangling `see` is left: {line}"
        );
    }
}

/// The second way, for a citing file the adopter changed: the lint skips a
/// listed file whatever its text.
#[test]
fn listing_the_citing_files_clears_a_deleted_entry_cited_by_a_locally_modified_file() {
    let dir = seeded_repo();
    let path = dir.path().join(CITING_FILES[0]);
    let mut text = fs::read_to_string(&path).expect("read the citing file");
    text.push_str("A line the adopter added.\n");
    fs::write(&path, text).expect("write the modified file");
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
    assert_eq!(list_the_files_the_findings_name(dir.path()), CITING_FILES);
    assert_passes(dir.path());
}

/// The first way, for an archived entry whose id the stamp lists in
/// `overrides`, the state the archive instruction of the seeded
/// `docs/README.md` leaves: the printed line names the id, and following it
/// restores the entry.
#[test]
fn the_printed_remedy_restores_an_archived_entry_the_stamp_overrides() {
    let dir = seeded_repo();
    archive_entry(dir.path(), DELETED_ID);
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!([DELETED_ID]);
    });
    assert_fails_with_both_findings(dir.path());
    follow_the_remedy(dir.path());
    assert_passes(dir.path());
    assert!(holds_the_entry(dir.path(), ACTIVE_TOPIC_FILE));
}

/// The seeded `docs/README.md` states the contract and the archive
/// instruction that the two tests below follow. The text is whitespace
/// normalized, because the file wraps its lines.
#[test]
fn the_seeded_readme_states_the_citation_contract() {
    let readme = fs::read_to_string(template_dir().join("docs/README.md"))
        .expect("read template/docs/README.md");
    let flat = readme.split_whitespace().collect::<Vec<_>>().join(" ");
    for sentence in [
        "A kit-owned file listed in `overrides` is not checked for the knowledge ids it cites.",
        "A seeded entry that a kit-owned file cites cannot be deleted or archived while that file \
         is checked: restore the entry, or list each citing file in `overrides`.",
        "list its id here after the sweep",
        "list each reported file here too, or restore the entry",
    ] {
        assert!(
            flat.contains(sentence),
            "the seeded docs/README.md lacks: {sentence}"
        );
    }
}

/// Follows the archive instruction of the seeded `docs/README.md` for an
/// entry that kit-owned files cite: archive it, list its id in `overrides`,
/// run `check-knowledge`, and list each file it reports. The run ends green.
#[test]
fn following_the_readme_archive_instruction_for_a_cited_entry_ends_green() {
    let dir = seeded_repo();
    archive_entry(dir.path(), DELETED_ID);
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!([DELETED_ID]);
    });
    assert!(
        !update_output(dir.path()).contains(DELETED_ID),
        "update still reports the archived entry after its id is listed"
    );
    assert_fails_with_both_findings(dir.path());
    assert_eq!(list_the_files_the_findings_name(dir.path()), CITING_FILES);
    assert_passes(dir.path());
}

/// The same instruction for an entry no kit-owned file cites: archive it,
/// list its id, and the run is green without the second step. Before the id
/// is listed, `update` reports the entry; after, it does not.
#[test]
fn following_the_readme_archive_instruction_for_an_uncited_entry_ends_green() {
    let dir = seeded_repo();
    let id = uncited_seeded_entry();
    archive_entry(dir.path(), &id);
    assert!(
        update_output(dir.path()).contains(&format!("skipped {id} (deleted)")),
        "update does not report the archived entry before its id is listed"
    );
    edit_stamp(dir.path(), |stamp| {
        stamp["overrides"] = json!([id]);
    });
    assert!(
        !update_output(dir.path()).contains(&id),
        "update still reports the archived entry after its id is listed"
    );
    assert_passes(dir.path());
}
