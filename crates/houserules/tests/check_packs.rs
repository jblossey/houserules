//! The pack-layer checks of `check-knowledge`, end to end through the
//! compiled binary, on a scratch install that `houserules init` made, with
//! the `quality` pack added; the `quality` pack is the one under test: a
//! clean tree passes, and each failing state prints its whole line and exits
//! with the code the spec names. The checks' own cases are unit tests in
//! `src/rules/check_packs.rs`.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The kit pack that every case installs.
const PACK: &str = "quality";

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off (`update.rs`'s own copy has
/// the account).
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
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
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create the directory");
    }
    fs::write(
        path,
        serde_json::to_string_pretty(value).expect("serialize JSON"),
    )
    .expect("write JSON file");
}

/// The `sha256` pin of a pack copy, as `pack add` writes it: the SHA-256 of
/// the compact JSON of the parsed copy with every object's keys sorted.
fn pack_pin(copy: &Value) -> String {
    let mut canonical = copy.clone();
    canonical.sort_all_objects();
    Sha256::digest(serde_json::to_vec(&canonical).expect("serialize"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Moves the `version` and the `sha256` of the pin of `<PACK>` in
/// `.houserules.json` to the copy `knowledge/packs/<PACK>.json`, and leaves
/// the rest of that pin and the pins of the other packs as they are.
fn pin_the_copy(root: &Path) {
    let copy = read_json(&root.join(format!("knowledge/packs/{PACK}.json")));
    let path = root.join(".houserules.json");
    let mut stamp = read_json(&path);
    stamp["packs"][PACK]["version"] = copy["version"].clone();
    stamp["packs"][PACK]["sha256"] = json!(pack_pin(&copy));
    write_json(&path, &stamp);
}

/// A scratch repository that `houserules init` installed, with the pack under
/// test added: the `knowledge-base` pack and that pack are pinned copies, and
/// the generated files are fresh. `tempfile`'s `TempDir` removes itself on
/// drop.
fn pack_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init failed");
    let init = run(dir.path(), "init");
    assert!(
        init.status.success(),
        "init failed: {}",
        String::from_utf8_lossy(&init.stderr)
    );
    let add = houserules()
        .args(["pack", "add", PACK, "--dir"])
        .arg(dir.path())
        .output()
        .expect("run pack add");
    assert!(
        add.status.success(),
        "pack add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    dir
}

/// Asserts that `check-knowledge` fails with exit `code` and exactly the
/// `expected` lines on stderr.
fn assert_fails_with(root: &Path, code: i32, expected: &[&str]) {
    let output = run(root, "check-knowledge");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(
        lines,
        expected,
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert_eq!(output.status.code(), Some(code));
}

fn assert_passes(root: &Path) {
    let output = run(root, "check-knowledge");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "knowledge: ok\n");
    assert_eq!(output.status.code(), Some(0));
}

/// The id of the entry of the pack at `index`.
fn pack_id(root: &Path, index: usize) -> String {
    read_json(&root.join(format!("knowledge/packs/{PACK}.json")))["entries"][index]["id"]
        .as_str()
        .expect("an id")
        .to_string()
}

/// A pack copy beside its pin passes, and the same copy passes with a
/// newer minor of the pack schema and keys that the schema does not name
/// (spec C13), once its pin follows.
#[test]
fn a_pinned_copy_passes_and_so_does_a_newer_minor_with_unknown_keys() {
    let dir = pack_repo();
    assert_passes(dir.path());
    let path = dir.path().join(format!("knowledge/packs/{PACK}.json"));
    let mut copy = read_json(&path);
    copy["schema"] = json!("houserules/pack@1.9.0");
    copy["futureKey"] = json!({"anything": [1, 2]});
    write_json(&path, &copy);
    pin_the_copy(dir.path());
    assert_passes(dir.path());
}

/// A copy with a newer pack schema major fails every command that reads it
/// with exit 2 and one line that names the file; the control is the same
/// tree passing before the edit.
#[test]
fn a_copy_with_a_newer_schema_major_exits_2() {
    let dir = pack_repo();
    assert_passes(dir.path());
    let path = dir.path().join(format!("knowledge/packs/{PACK}.json"));
    let mut copy = read_json(&path);
    copy["schema"] = json!("houserules/pack@2.0.0");
    write_json(&path, &copy);
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(
        stderr.contains(&format!("{PACK}.json")) && stderr.contains("newer than houserules"),
        "{stderr}"
    );
}

/// A hand-edited copy fails its pin with one line that names the pack and
/// points to `overrides.json`.
#[test]
fn a_hand_edited_copy_fails_its_pin() {
    let dir = pack_repo();
    let path = dir.path().join(format!("knowledge/packs/{PACK}.json"));
    let mut copy = read_json(&path);
    copy["version"] = json!("9.9.9");
    write_json(&path, &copy);
    assert_fails_with(
        dir.path(),
        1,
        &[&format!(
            "knowledge/packs/{PACK}.json: the pack \"{PACK}\" differs from its pin in \
             .houserules.json; put your change in knowledge/overrides.json and restore the copy"
        )],
    );
}

/// A hand-written local entry with a pack id fails with both sources
/// named, and every other command keeps reading the local entry.
#[test]
fn a_local_entry_with_a_pack_id_fails_with_both_sources_named() {
    let dir = pack_repo();
    let id = pack_id(dir.path(), 0);
    let pack_entry =
        read_json(&dir.path().join(format!("knowledge/packs/{PACK}.json")))["entries"][0].clone();
    write_json(
        &dir.path().join("knowledge/team.json"),
        &json!({"topic": "team", "title": "Team", "entries": [pack_entry]}),
    );
    let output = run(dir.path(), "check-knowledge");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let duplicate =
        format!("knowledge/packs/{PACK}.json {id}: duplicate id (also in knowledge/team.json)");
    assert!(stderr.lines().any(|line| line == duplicate), "{stderr}");
    assert_eq!(output.status.code(), Some(1));
}

/// An override of an id that no installed pack ships fails with the id and
/// the case; an override of a pack id and an exclusion pass and are listed
/// on stdout.
#[test]
fn the_validity_of_overrides_json_is_checked_and_its_content_listed() {
    let dir = pack_repo();
    let id = pack_id(dir.path(), 0);
    let excluded = pack_id(dir.path(), 1);
    let overrides = dir.path().join("knowledge/overrides.json");
    write_json(
        &overrides,
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": "ghost.rule", "fields": {"summary": "Changed."}}],
        }),
    );
    assert_fails_with(
        dir.path(),
        1,
        &["knowledge/overrides.json ghost.rule: override of an id that no installed pack ships"],
    );

    write_json(
        &overrides,
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": id, "fields": {"summary": "Changed."}}],
            "exclusions": [{"id": excluded, "reason": "not for us"}],
        }),
    );
    let render = run(dir.path(), "render");
    assert!(render.status.success());
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("override {id}: summary\nexclusion {excluded}: not for us\nknowledge: ok\n")
    );
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    assert_eq!(output.status.code(), Some(0));
}

/// A local topic named `overrides` is a `knowledge/overrides.json` that
/// breaks the embedded overrides schema: the command exits 2 and names the
/// file and the field (spec L7).
#[test]
fn a_topic_named_overrides_is_rejected_as_an_overrides_file() {
    let dir = pack_repo();
    write_json(
        &dir.path().join("knowledge/overrides.json"),
        &json!({"topic": "overrides", "title": "A topic.", "entries": []}),
    );
    let output = run(dir.path(), "check-knowledge");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    let file = Path::new("knowledge").join("overrides.json");
    assert!(stderr.contains(&file.display().to_string()), "{stderr}");
    assert!(stderr.contains("unknown field \"topic\""), "{stderr}");
}

/// A `CLAUDE.md` that holds its own text and no `@AGENTS.md` fails with
/// the line that says where the standing rules stop; once the import is
/// added, `check-knowledge` passes.
#[test]
fn a_claude_md_without_the_import_fails_until_the_import_is_added() {
    let dir = pack_repo();
    let path = dir.path().join("CLAUDE.md");
    let original = fs::read_to_string(&path).expect("read CLAUDE.md");
    assert!(
        original.contains("@AGENTS.md"),
        "the seed imports AGENTS.md"
    );
    fs::write(&path, "# My own notes\n\nNothing else.\n").expect("write CLAUDE.md");
    assert_fails_with(
        dir.path(),
        1,
        &["CLAUDE.md: the standing rules do not reach Claude Code; add @AGENTS.md to CLAUDE.md"],
    );
    fs::write(&path, "# My own notes\n\n@AGENTS.md\n").expect("write CLAUDE.md");
    assert_passes(dir.path());
}

/// A tree that has no `CLAUDE.md` passes: the check has no subject.
#[test]
fn an_absent_claude_md_passes() {
    let dir = pack_repo();
    fs::remove_file(dir.path().join("CLAUDE.md")).expect("remove CLAUDE.md");
    assert_passes(dir.path());
}
