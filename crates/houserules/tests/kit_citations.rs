//! `check-knowledge`'s kit-owned citation lint, end to end through the
//! compiled binary: a fresh install passes, a deleted entry, an excluded
//! entry, and an entry that an override retires fail with the whole finding
//! line, an owned file is skipped, each way the excluded line and the
//! override-status line print clears the finding, an id that no kit pack
//! ships prints the update-or-edit line, an upstream pack that carries the
//! name of a kit pack and a kit copy that does not ship a cited id as an
//! active entry print their own lines, a pin file that cannot give its pins
//! prints its defect once, a topic the adopter adds never turns kit text
//! into a finding, and the seeded `docs/README.md` states the contract that a
//! flow follows.
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

mod tracker_support;

use tracker_support::sha256_hex;

/// The entry every case below deletes. Two kit-owned files cite it:
/// `branch-reviewer.md` and the `orchestrating` skill.
const DELETED_ID: &str = "process.evals-rerun";

/// The topic file that holds `DELETED_ID` in a seeded install.
const ACTIVE_TOPIC_FILE: &str = "knowledge/process.json";

/// The kit-owned files that cite `DELETED_ID`, in the order of the
/// `KIT_OWNED` list.
const CITING_FILES: [&str; 2] = [
    ".claude/agents/branch-reviewer.md",
    ".claude/skills/orchestrating/SKILL.md",
];

/// The line of the requirement check (batch 34, spec 4.3) for `DELETED_ID`
/// when no installed pack provides it. The install holds the reference
/// workflow, which requires the id, so every case that takes the id from the
/// base also gets this line, before the lines of the lint.
const REQUIREMENT_NOT_PROVIDED: &str = "workflow reference requires process.evals-rerun: no \
     installed pack provides it; run houserules pack add process\n";

/// The line of the requirement check for `DELETED_ID` when
/// `knowledge/overrides.json` excludes it.
const REQUIREMENT_EXCLUDED: &str = "workflow reference requires process.evals-rerun, which \
     knowledge/overrides.json excludes; remove the exclusion, and override the entry to change \
     it\n";

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off (`update.rs`'s own copy has the
/// account).
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// The kit's five packs.
const KIT_PACKS: [&str; 5] = [
    "knowledge-base",
    "process",
    "quality",
    "security-hygiene",
    "writing-style",
];

/// A scratch repository that `houserules init --target claude --tracker json
/// --workflow reference --add-required-packs` installed, with the `quality`
/// pack added, and whose kit packs then became local topic files: each topic
/// file holds the entries of its pack, and the install holds no pack, so each
/// case below builds its own pack state. The files that cite `DELETED_ID` are
/// the files of the reference workflow, which the install pins.
fn seeded_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    let init = houserules()
        .args([
            "init",
            "--target",
            "claude",
            "--tracker",
            "json",
            "--workflow",
            "reference",
            "--add-required-packs",
            "--dir",
        ])
        .arg(dir.path())
        .output()
        .expect("run init");
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let quality = houserules()
        .args(["pack", "add", "quality", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run pack add");
    assert!(
        quality.status.success(),
        "pack add quality failed: {}",
        String::from_utf8_lossy(&quality.stderr)
    );
    for pack in KIT_PACKS {
        let copy = read_json(&dir.path().join(format!("knowledge/packs/{pack}.json")));
        write_json(
            &dir.path().join(format!("knowledge/{pack}.json")),
            &json!({
                "topic": pack,
                "title": copy["topics"][pack]["title"],
                "entries": copy["entries"],
            }),
        );
        let removed = houserules()
            .args(["pack", "remove", pack, "--dir"])
            .arg(dir.path())
            .output()
            .expect("run pack remove");
        assert!(
            removed.status.success(),
            "pack remove {pack} failed: {}",
            String::from_utf8_lossy(&removed.stderr)
        );
    }
    render(dir.path());
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

/// The whole finding line for a file that cites `DELETED_ID` when the
/// install holds no copy of the pack that ships it.
fn finding_for(file: &str) -> String {
    format!(
        "{file}: cites \"process.evals-rerun\" from the pack \"process\", which is not \
         installed; run houserules pack add process\n"
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
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{REQUIREMENT_NOT_PROVIDED}{}", both_findings())
    );
    assert_eq!(output.status.code(), Some(1));
}

/// The adopter-visible behavior of an override: a workflow file the stamp
/// lists is the adopter's own, so a citation in it is no finding of the lint.
/// The same install fails with both findings before the override, so the
/// skip is not vacuous. The requirement check has no waiver (design.md 5.112):
/// the id is still gone, so its line stays and the install still fails.
#[test]
fn an_overridden_kit_owned_file_is_skipped() {
    let dir = seeded_repo();
    delete_the_entry(dir.path());
    assert_fails_with_both_findings(dir.path());
    edit_stamp(dir.path(), |stamp| {
        stamp["ownedPaths"] = json!(CITING_FILES);
    });
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        REQUIREMENT_NOT_PROVIDED
    );
    assert_eq!(output.status.code(), Some(1));
}

/// A stamp that `update` rejects is a finding too: the lint cannot read the
/// owned paths, so it names the stamp by its relative name, as every finding
/// of `check-knowledge` names its file, and says no citation was checked.
#[test]
fn a_malformed_stamp_is_a_named_finding() {
    let dir = seeded_repo();
    edit_stamp(dir.path(), |stamp| {
        stamp["ownedPaths"] = json!("process.tdd");
    });
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        ".houserules.json: ownedPaths must be an array of strings; kit-owned citations not checked\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// A `renderers` value that is no array of strings is a finding of the render
/// check and of the lint. Both lines name the stamp by its relative name.
#[test]
fn an_invalid_renderers_value_names_the_stamp_by_its_relative_name() {
    let dir = seeded_repo();
    edit_stamp(dir.path(), |stamp| {
        stamp["renderers"] = json!(5);
    });
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        ".houserules.json: renderers must be an array of strings\n\
         .houserules.json: renderers must be an array of strings; kit-owned citations not checked\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// A pin file that cannot give its pins prints its defect once, from the
/// pin check. The lint says only that it checked no citation, so the same
/// text never appears twice.
#[test]
fn a_pin_file_that_cannot_give_its_pins_prints_its_defect_once() {
    let dir = seeded_repo();
    edit_stamp(dir.path(), |stamp| {
        stamp["packs"] = json!({"process": {"sha256": "abc"}});
    });
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        ".houserules.json: packs.process: missing \"version\"\n\
         .houserules.json: the pins cannot be read, so kit-owned citations are not checked\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// The workflow skill the unseeded-id cases below put an older kit's text
/// into.
const FINISHING_SKILL: &str = ".claude/skills/finishing-a-feature/SKILL.md";

/// An id the payload never seeds, as an older kit's skill cited one.
const UNSEEDED_ID: &str = "process.cited-by-an-older-kit";

/// The skill as an older kit shipped it: the current text plus one line that
/// cites `UNSEEDED_ID`.
fn older_kit_skill() -> String {
    let current = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../template/workflows/reference/.claude/skills/finishing-a-feature/SKILL.md"),
    )
    .expect("read the template skill");
    format!("{current}\nAn older kit cited `{UNSEEDED_ID}` here.\n")
}

/// Puts `older_kit_skill()` into `root`, pinned at its own hash as `update`
/// pins a workflow file it wrote, so the install looks like one that has not
/// yet run `update` with the new binary. With `local_edit` the file also carries one
/// line the adopter added after that stamp.
fn install_older_kit_skill(root: &Path, local_edit: bool) {
    let older = older_kit_skill();
    let text = if local_edit {
        format!("{older}A line the adopter added.\n")
    } else {
        older.clone()
    };
    fs::write(root.join(FINISHING_SKILL), text).expect("write the older skill");
    let hash = sha256_hex(older.as_bytes());
    edit_stamp(root, |stamp| {
        stamp["workflow"]["files"][FINISHING_SKILL] = json!(hash);
    });
}

/// The whole finding line for `UNSEEDED_ID` cited by `FINISHING_SKILL`.
fn unseeded_finding() -> String {
    format!(
        "{FINISHING_SKILL}: cites \"{UNSEEDED_ID}\", which is not in the knowledge base; the kit \
         does not seed it, so no entry can be restored: run houserules update, and if update \
         prints a \"kept {FINISHING_SKILL}\" line, remove the citation from \
         {FINISHING_SKILL} or list \"{FINISHING_SKILL}\" in \"ownedPaths\" in .houserules.json\n"
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
        printed.contains(&format!(
            "kept {FINISHING_SKILL} (locally modified; reference workflow {})",
            env!("CARGO_PKG_VERSION")
        )),
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

/// The third way the unseeded line names, for a file the adopter changed:
/// list the file in `ownedPaths`. The file is then the adopter's own and no
/// subject of the lint, so the install passes.
#[test]
fn the_printed_remedy_clears_an_unseeded_citation_in_a_kept_file_by_listing_it_in_owned_paths() {
    let dir = seeded_repo();
    install_older_kit_skill(dir.path(), true);
    assert_fails_with_the_unseeded_finding(dir.path());
    let printed = update_output(dir.path());
    assert!(
        printed.contains(&format!(
            "kept {FINISHING_SKILL} (locally modified; reference workflow {})",
            env!("CARGO_PKG_VERSION")
        )),
        "update did not keep the skill: {printed}"
    );
    assert_fails_with_the_unseeded_finding(dir.path());
    edit_stamp(dir.path(), |stamp| {
        stamp["ownedPaths"] = json!([FINISHING_SKILL]);
    });
    assert_passes(dir.path());
}

/// The checkout's `template/` directory: the kit text every install starts
/// from.
fn template_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template")
}

/// The topics the kit seeds: the topics that a pack file under
/// `template/packs/` serves.
fn seeded_topics() -> BTreeSet<String> {
    fs::read_dir(template_dir().join("packs"))
        .expect("read template/packs")
        .flat_map(|entry| {
            let pack = read_json(&entry.expect("read a directory entry").path());
            pack["topics"]
                .as_object()
                .expect("the topics of a pack")
                .keys()
                .cloned()
                .collect::<Vec<_>>()
        })
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
/// entry of `template/packs/process.json` whose id no kit-owned text holds,
/// in any form.
fn uncited_seeded_entry() -> String {
    let texts = kit_owned_template_texts();
    read_json(&template_dir().join("packs/process.json"))["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id string").to_string())
        .find(|id| texts.iter().all(|(_, text)| !text.contains(id.as_str())))
        .expect("the seed holds a process entry that no kit-owned file cites")
}

/// The seeded `docs/README.md` states the contract and the remedies that the
/// cases above and below follow. The text is whitespace normalized, because
/// the file wraps its lines.
#[test]
fn the_seeded_readme_states_the_citation_contract() {
    let readme = fs::read_to_string(template_dir().join("docs/README.md"))
        .expect("read template/docs/README.md");
    let flat = readme.split_whitespace().collect::<Vec<_>>().join(" ");
    for sentence in [
        "A kit-owned file listed in `ownedPaths` is not checked for the knowledge ids it cites.",
        "A pack entry that a kit-owned file cites cannot stay excluded or retired while that file \
         is checked: restore the entry, or list each citing file in `ownedPaths`.",
        "To retire a pack entry, override its `status` with `retired`.",
        "The pack is not installed: run `houserules pack add <pack>`.",
        "The installed pack does not ship the entry as an active entry: run `houserules update`.",
    ] {
        assert!(
            flat.contains(sentence),
            "the seeded docs/README.md lacks: {sentence}"
        );
    }
}

/// A kit entry that no kit-owned file cites can be retired: an override of
/// its pack sets `status: retired`, the install passes, and `update` has
/// nothing to say about it.
#[test]
fn retiring_an_uncited_kit_entry_with_an_override_ends_green() {
    let dir = seeded_repo();
    install_the_process_pack(dir.path(), &kit_process_pack(), "kit");
    let id = uncited_seeded_entry();
    write_json(
        &dir.path().join("knowledge/overrides.json"),
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": id, "fields": {"status": "retired"}}],
        }),
    );
    render(dir.path());
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("override {id}: status\nknowledge: ok\n")
    );
    assert_eq!(output.status.code(), Some(0));

    assert!(
        !update_output(dir.path()).contains(&id),
        "update reports the retired entry"
    );
    assert_eq!(run(dir.path(), "check-knowledge").status.code(), Some(0));
}

/// The `sha256` pin of a pack copy, as `pack add` writes it: the SHA-256 of
/// the compact JSON of the parsed copy with every object's keys sorted.
fn pack_pin(copy: &Value) -> String {
    let mut canonical = copy.clone();
    canonical.sort_all_objects();
    sha256_hex(&serde_json::to_vec(&canonical).expect("serialize"))
}

/// The kit's `process` pack, as `template/packs/process.json` ships it.
fn kit_process_pack() -> Value {
    read_json(&template_dir().join("packs/process.json"))
}

/// Turns the seeded `process` topic file into the pack copy `copy`, and pins
/// it in `.houserules.json` with the version of the copy and `source`: the
/// state that `pack add process` leaves for `source` `kit`. Renders after,
/// so the lint is the only finding of a case that changes nothing else.
fn install_the_process_pack(root: &Path, copy: &Value, source: &str) {
    fs::create_dir_all(root.join("knowledge/packs")).expect("create knowledge/packs");
    write_json(&root.join("knowledge/packs/process.json"), copy);
    fs::remove_file(root.join(ACTIVE_TOPIC_FILE)).expect("remove the local process topic");
    edit_stamp(root, |stamp| {
        stamp["packs"] = json!({
            "process": {
                "version": copy["version"], "source": source,
                "sha256": pack_pin(copy), "addedAreas": [],
            },
        });
    });
    render(root);
}

/// The kit's `process` pack without `DELETED_ID`, and without the `see`
/// links to it, as an adopter or an older release leaves it.
fn process_pack_without_the_entry() -> Value {
    let mut copy = kit_process_pack();
    let entries = copy["entries"].as_array_mut().expect("entries array");
    entries.retain(|entry| entry["id"] != DELETED_ID);
    for entry in entries.iter_mut() {
        if let Some(see) = entry.get_mut("see").and_then(Value::as_array_mut) {
            see.retain(|id| id != DELETED_ID);
        }
    }
    copy
}

/// Writes `knowledge/overrides.json` with the one override of `DELETED_ID`
/// that sets `fields`, and renders.
fn override_the_entry(root: &Path, fields: Value) {
    write_json(
        &root.join("knowledge/overrides.json"),
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": DELETED_ID, "fields": fields}],
        }),
    );
    render(root);
}

/// Installs the kit's `process` pack and excludes `DELETED_ID` in
/// `knowledge/overrides.json`: the state that `pack add process` and an
/// exclusion leave. Then renders, so the lint is the only finding.
fn exclude_the_entry_through_the_process_pack(root: &Path) {
    install_the_process_pack(root, &kit_process_pack(), "kit");
    write_json(
        &root.join("knowledge/overrides.json"),
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "exclusions": [{"id": DELETED_ID, "reason": "not for this project"}],
        }),
    );
    render(root);
}

/// The whole finding line for a file that cites the excluded `DELETED_ID`.
fn excluded_finding_for(file: &str) -> String {
    format!(
        "{file}: cites \"process.evals-rerun\", which knowledge/overrides.json excludes; remove \
         the exclusion, or replace it with an override that carries your own text\n"
    )
}

/// The precondition of the two remedy cases: `check-knowledge` fails with
/// exit 1 and the excluded line for each of the two files that cite
/// `DELETED_ID`. The cases then prove a remedy turns that failure into a
/// pass, never a pass that held already.
fn assert_fails_with_both_excluded_findings(root: &Path) {
    let output = run(root, "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| excluded_finding_for(file))
        .collect();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{REQUIREMENT_EXCLUDED}{both}")
    );
    assert_eq!(output.status.code(), Some(1));
}

/// A kit-owned citation of an excluded entry fails with the whole line,
/// and `check-knowledge` lists the exclusion on stdout, whether it fails or
/// passes.
#[test]
fn check_knowledge_fails_a_kit_owned_citation_of_an_excluded_entry() {
    let dir = seeded_repo();
    exclude_the_entry_through_the_process_pack(dir.path());
    let output = run(dir.path(), "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| excluded_finding_for(file))
        .collect();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{REQUIREMENT_EXCLUDED}{both}")
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "exclusion process.evals-rerun: not for this project\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// The first way the excluded line names: remove the exclusion. The entry
/// is the pack's again, and the install passes.
#[test]
fn removing_the_exclusion_clears_the_excluded_finding() {
    let dir = seeded_repo();
    exclude_the_entry_through_the_process_pack(dir.path());
    assert_fails_with_both_excluded_findings(dir.path());
    fs::remove_file(dir.path().join("knowledge/overrides.json")).expect("remove the exclusion");
    render(dir.path());
    assert_passes(dir.path());
}

/// The second way: replace the exclusion with an override that carries the
/// project's own text. The entry is live again, with that text, and the
/// install passes; `check-knowledge` lists the override.
#[test]
fn replacing_the_exclusion_with_an_override_clears_the_excluded_finding() {
    let dir = seeded_repo();
    exclude_the_entry_through_the_process_pack(dir.path());
    assert_fails_with_both_excluded_findings(dir.path());
    write_json(
        &dir.path().join("knowledge/overrides.json"),
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{
                "id": DELETED_ID,
                "fields": {"summary": "The project's own summary of the rule."},
            }],
        }),
    );
    render(dir.path());
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "override process.evals-rerun: summary\nknowledge: ok\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

/// The whole finding line for a file that cites `DELETED_ID` when an
/// override sets the status `retired` for it.
fn override_status_finding_for(file: &str) -> String {
    format!(
        "{file}: cites \"process.evals-rerun\", which knowledge/overrides.json sets to status \
         \"retired\"; remove that status from the override, or replace the override with one \
         that carries your own text\n"
    )
}

/// The precondition of the two remedy cases of the override-status line:
/// `check-knowledge` fails with exit 1 and the line for each of the two
/// files that cite `DELETED_ID`.
fn assert_fails_with_both_override_status_findings(root: &Path) {
    let output = run(root, "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| override_status_finding_for(file))
        .collect();
    assert_eq!(String::from_utf8_lossy(&output.stderr), both);
    assert_eq!(output.status.code(), Some(1));
}

/// A kit-owned citation of an entry that an override retires fails with the
/// whole line, and `check-knowledge` lists the override on stdout.
#[test]
fn check_knowledge_fails_a_kit_owned_citation_of_an_entry_an_override_retires() {
    let dir = seeded_repo();
    install_the_process_pack(dir.path(), &kit_process_pack(), "kit");
    override_the_entry(dir.path(), json!({"status": "retired"}));
    let output = run(dir.path(), "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| override_status_finding_for(file))
        .collect();
    assert_eq!(String::from_utf8_lossy(&output.stderr), both);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "override process.evals-rerun: status\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// The first way the override-status line names: remove that status from the
/// override. The override stays, with no field, and the install passes.
#[test]
fn removing_the_status_from_the_override_clears_the_override_status_finding() {
    let dir = seeded_repo();
    install_the_process_pack(dir.path(), &kit_process_pack(), "kit");
    override_the_entry(dir.path(), json!({"status": "retired"}));
    assert_fails_with_both_override_status_findings(dir.path());
    let path = dir.path().join("knowledge/overrides.json");
    let mut overrides = read_json(&path);
    overrides["overrides"][0]["fields"]
        .as_object_mut()
        .expect("the override fields")
        .remove("status");
    write_json(&path, &overrides);
    render(dir.path());
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "override process.evals-rerun\nknowledge: ok\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

/// The second way: replace the override with one that carries the project's
/// own text and no inactive status. The entry is live again, with that text,
/// and the install passes.
#[test]
fn replacing_the_override_with_one_that_carries_own_text_clears_the_override_status_finding() {
    let dir = seeded_repo();
    install_the_process_pack(dir.path(), &kit_process_pack(), "kit");
    override_the_entry(dir.path(), json!({"status": "retired"}));
    assert_fails_with_both_override_status_findings(dir.path());
    override_the_entry(
        dir.path(),
        json!({"summary": "The project's own summary of the rule."}),
    );
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "override process.evals-rerun: summary\nknowledge: ok\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

/// The source that an upstream pack with the name `process` records.
const UPSTREAM_SOURCE: &str = "git+https://example.invalid/org/repo@v1.0.0#process.json";

/// The whole finding line for a file that cites `DELETED_ID` when an
/// installed pack named `process` comes from `source`.
fn name_held_finding_for(file: &str, source: &str) -> String {
    format!(
        "{file}: cites \"process.evals-rerun\" from the kit pack \"process\", but the \
         installed pack \"process\" comes from {source}; the kit pack cannot be installed \
         while another pack holds its name\n"
    )
}

/// An upstream pack that carries the name `process` is not the kit pack, so
/// it does not make the kit text's citations live: an id that it lacks is a
/// finding for each citing file, whatever its name.
#[test]
fn an_upstream_pack_named_like_a_kit_pack_does_not_make_the_citations_live() {
    let dir = seeded_repo();
    install_the_process_pack(
        dir.path(),
        &process_pack_without_the_entry(),
        UPSTREAM_SOURCE,
    );
    let output = run(dir.path(), "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| name_held_finding_for(file, UPSTREAM_SOURCE))
        .collect();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{REQUIREMENT_NOT_PROVIDED}{both}")
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(output.status.code(), Some(1));
}

/// The control of the case above: the same upstream source passes when its
/// pack ships every id that the kit text cites, so the finding is the missing
/// id and not the name.
#[test]
fn an_upstream_pack_named_like_a_kit_pack_passes_when_it_ships_every_cited_id() {
    let dir = seeded_repo();
    install_the_process_pack(dir.path(), &kit_process_pack(), UPSTREAM_SOURCE);
    assert_passes(dir.path());
}

/// The whole finding line for a file that cites `DELETED_ID` when the
/// installed kit pack `process` at `version` does not ship it as an active
/// entry.
fn inactive_in_copy_finding_for(file: &str, version: &str) -> String {
    format!(
        "{file}: cites \"process.evals-rerun\", which the installed pack \"process\" {version} \
         does not ship as an active entry; run houserules update\n"
    )
}

/// The installed kit pack is the copy whose pin records the source `kit`. A
/// copy older than the kit text that lacks a cited id is one finding for each
/// citing file, with the version of the pin.
#[test]
fn an_older_kit_copy_that_lacks_a_cited_id_is_a_finding_for_each_citing_file() {
    let dir = seeded_repo();
    let mut older = process_pack_without_the_entry();
    older["version"] = json!("0.9.0");
    install_the_process_pack(dir.path(), &older, "kit");
    let output = run(dir.path(), "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| inactive_in_copy_finding_for(file, "0.9.0"))
        .collect();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{REQUIREMENT_NOT_PROVIDED}{both}")
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(output.status.code(), Some(1));
}

/// The line says to run `houserules update`, and the run clears it: `update`
/// moves the older kit copy to the pack of the binary, which ships the id, so
/// the install passes.
#[test]
fn the_printed_remedy_of_an_older_kit_copy_clears_the_finding_through_update() {
    let dir = seeded_repo();
    let mut older = process_pack_without_the_entry();
    older["version"] = json!("0.9.0");
    install_the_process_pack(dir.path(), &older, "kit");
    let output = run(dir.path(), "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| inactive_in_copy_finding_for(file, "0.9.0"))
        .collect();
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{REQUIREMENT_NOT_PROVIDED}{both}")
    );
    assert_eq!(output.status.code(), Some(1));

    let printed = update_output(dir.path());

    assert!(
        printed.contains(&format!(
            "moved pack process from 0.9.0 to {}",
            kit_process_pack()["version"].as_str().expect("a version")
        )),
        "update did not move the pack: {printed}"
    );
    // The move offers the every-commit entry of the process pack (spec 3.7):
    // the exclusion with the reason `opt-in` is listed on stdout.
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "exclusion process.every-commit-names-a-work-item: opt-in\nknowledge: ok\n"
    );
    assert_eq!(output.status.code(), Some(0));
}

/// The same line when the status of the kit copy itself retires the cited
/// id: the copy ships the id, and no read returns it.
#[test]
fn a_kit_copy_whose_own_status_retires_a_cited_id_is_a_finding_for_each_citing_file() {
    let dir = seeded_repo();
    let mut copy = kit_process_pack();
    for entry in copy["entries"].as_array_mut().expect("entries array") {
        if entry["id"] == DELETED_ID {
            entry["status"] = json!("retired");
        }
    }
    install_the_process_pack(dir.path(), &copy, "kit");
    let output = run(dir.path(), "check-knowledge");
    let both: String = CITING_FILES
        .iter()
        .map(|file| {
            inactive_in_copy_finding_for(
                file,
                kit_process_pack()["version"].as_str().expect("a version"),
            )
        })
        .collect();
    assert_eq!(String::from_utf8_lossy(&output.stderr), both);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(output.status.code(), Some(1));
}
