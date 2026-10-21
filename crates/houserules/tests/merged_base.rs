//! The merged knowledge base, run through the compiled binary: local topic
//! files, `knowledge/packs/*.json` copies, and `knowledge/overrides.json`
//! read as one base by `get`, `index`, `topics`, `standing`, `for`,
//! `render`, `audit`, and `archive`; the load errors that name a bad pack
//! copy, a repeated pack name, or a bad `overrides.json`; and the `--help`
//! text of the commands that state the merged base.

mod install_shape;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Map, Value, json};
use tempfile::TempDir;

/// A `Command` for the compiled `houserules` binary under test. This file
/// keeps its own copy of the helper: it needs nothing else from
/// `tests/common`, and importing that module makes its other helpers
/// warn as dead code in this binary. `HOUSERULES_SKIP_SELF_UPDATE` keeps
/// `update`'s self-update phase from running.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// A 2.0 install root with an empty `schema.json` and an `areas.json` that
/// declares `global` (no paths) and `rust` (`crates/**`): the smallest
/// knowledge base the read commands load.
fn tree() -> TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    let knowledge = root.path().join("knowledge");
    fs::create_dir_all(&knowledge).expect("create knowledge/");
    fs::write(knowledge.join("schema.json"), "{}").expect("write schema.json");
    write_json(
        &knowledge.join("areas.json"),
        &json!({"global": {"paths": []}, "rust": {"paths": ["crates/**"]}}),
    );
    install_shape::mark_2_0(root.path());
    root
}

/// Writes `value` as pretty JSON to `path`, creating its parent directory.
fn write_json(path: &Path, value: &Value) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create the parent directory");
    }
    fs::write(
        path,
        serde_json::to_string_pretty(value).expect("serialize"),
    )
    .expect("write a JSON file");
}

/// One schema-valid knowledge entry in the `global` area.
fn entry(id: &str, standing: bool, summary: &str) -> Value {
    json!({
        "id": id,
        "kind": "rule",
        "area": "global",
        "standing": standing,
        "summary": summary,
        "body": ["Body."],
        "tags": [],
        "source": {"date": "2026-10-19", "by": "controller"},
    })
}

/// Writes the local topic file `knowledge/<topic>.json`. A `None` title
/// leaves the `title` key out.
fn write_local_topic(root: &Path, topic: &str, title: Option<&str>, entries: &[Value]) {
    let mut file = Map::new();
    file.insert("topic".to_string(), json!(topic));
    if let Some(title) = title {
        file.insert("title".to_string(), json!(title));
    }
    file.insert("entries".to_string(), json!(entries));
    write_json(
        &root.join("knowledge").join(format!("{topic}.json")),
        &Value::Object(file),
    );
}

/// Writes the pack copy `knowledge/packs/<name>.json` that serves
/// `topics` (name and title) with `entries`.
fn write_pack(root: &Path, name: &str, topics: &[(&str, &str)], entries: &[Value]) {
    let served: Map<String, Value> = topics
        .iter()
        .map(|(topic, title)| (topic.to_string(), json!({"title": title})))
        .collect();
    write_json(
        &root
            .join("knowledge")
            .join("packs")
            .join(format!("{name}.json")),
        &json!({
            "name": name,
            "version": "1.0.0",
            "schema": "houserules/pack@1.0.0",
            "topics": served,
            "areas": {"global": {"paths": []}, "rust": {"paths": ["crates/**"]}},
            "entries": entries,
        }),
    );
}

/// Writes `knowledge/overrides.json` with the given overrides and
/// exclusions.
fn write_overrides(root: &Path, overrides: Value, exclusions: Value) {
    write_json(
        &root.join("knowledge").join("overrides.json"),
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": overrides,
            "exclusions": exclusions,
        }),
    );
}

/// Runs `houserules <args> --dir <root>`.
fn run(root: &Path, args: &[&str]) -> Output {
    houserules()
        .args(args)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run houserules")
}

/// The JSON a successful run printed. Fails the test, with stderr, on a
/// non-zero exit.
fn stdout_json(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

/// The keys of the first object of a JSON array, in printed order.
fn first_keys(value: &Value) -> Vec<&str> {
    value[0]
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

/// The ids of an array of rows, in printed order.
fn ids(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("an array")
        .iter()
        .map(|row| row["id"].as_str().expect("an id"))
        .collect()
}

/// `get` of an excluded id prints the pack entry, its topic, and
/// `excluded` last with the reason, and exits 0.
#[test]
fn get_prints_an_excluded_entry_with_its_reason() {
    let root = tree();
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[entry("demo.one", false, "The pack text.")],
    );
    write_overrides(
        root.path(),
        json!([]),
        json!([{"id": "demo.one", "reason": "The project has its own rule."}]),
    );

    let printed = stdout_json(&run(root.path(), &["get", "demo.one"]));

    let mut expected = entry("demo.one", false, "The pack text.");
    expected["topic"] = json!("demo");
    expected["excluded"] = json!("The project has its own rule.");
    assert_eq!(printed, json!([expected]));
    assert_eq!(
        first_keys(&printed),
        [
            "id", "kind", "area", "standing", "summary", "body", "tags", "source", "topic",
            "excluded"
        ]
    );
}

/// An exclusion with no reason prints `"excluded": true`.
#[test]
fn get_prints_true_for_an_exclusion_without_a_reason() {
    let root = tree();
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[entry("demo.one", false, "The pack text.")],
    );
    write_overrides(root.path(), json!([]), json!([{"id": "demo.one"}]));

    let printed = stdout_json(&run(root.path(), &["get", "demo.one"]));

    assert_eq!(printed[0]["excluded"], json!(true));
}

/// `get` of an overridden id prints the effective entry: each replaced
/// field holds the override's value, a field set to `null` is gone, the
/// remaining fields keep their order, and `overridden` lists the replaced
/// field names, sorted, after `topic`.
#[test]
fn get_prints_the_effective_entry_and_the_replaced_field_names() {
    let root = tree();
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[entry("demo.one", true, "The pack text.")],
    );
    write_overrides(
        root.path(),
        json!([{"id": "demo.one", "fields": {"summary": "The project text.", "standing": null}}]),
        json!([]),
    );

    let printed = stdout_json(&run(root.path(), &["get", "demo.one"]));

    assert_eq!(
        printed,
        json!([{
            "id": "demo.one",
            "kind": "rule",
            "area": "global",
            "summary": "The project text.",
            "body": ["Body."],
            "tags": [],
            "source": {"date": "2026-10-19", "by": "controller"},
            "topic": "demo",
            "overridden": ["standing", "summary"],
        }])
    );
    assert_eq!(
        first_keys(&printed),
        [
            "id",
            "kind",
            "area",
            "summary",
            "body",
            "tags",
            "source",
            "topic",
            "overridden"
        ]
    );
}

/// An override replaces a whole field: an object field holds the
/// override's object and none of the pack's keys.
#[test]
fn an_override_replaces_a_whole_object_field() {
    let root = tree();
    let mut pack_entry = entry("demo.one", false, "The pack text.");
    pack_entry["check"] = json!({"type": "grep-absent", "level": "fail", "pattern": "x"});
    write_pack(root.path(), "demo", &[("demo", "Demo")], &[pack_entry]);
    write_overrides(
        root.path(),
        json!([{"id": "demo.one", "fields": {"check": {"type": "commits", "level": "warn", "subject": "^feat"}}}]),
        json!([]),
    );

    let printed = stdout_json(&run(root.path(), &["get", "demo.one"]));

    assert_eq!(
        printed[0]["check"],
        json!({"type": "commits", "level": "warn", "subject": "^feat"})
    );
}

/// `get` of a plain id, overridden or excluded in no way, prints the
/// entry and its `topic` and no marker: the whole value is compared.
#[test]
fn get_prints_no_marker_for_an_untouched_pack_entry_or_a_local_entry() {
    let root = tree();
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[entry("demo.one", false, "The pack text.")],
    );
    write_local_topic(
        root.path(),
        "mine",
        Some("Mine"),
        &[entry("mine.one", false, "The local text.")],
    );

    let printed = stdout_json(&run(root.path(), &["get", "demo.one", "mine.one"]));

    let mut pack_entry = entry("demo.one", false, "The pack text.");
    pack_entry["topic"] = json!("demo");
    let mut local_entry = entry("mine.one", false, "The local text.");
    local_entry["topic"] = json!("mine");
    assert_eq!(printed, json!([pack_entry, local_entry]));
    for row in printed.as_array().expect("an array") {
        assert_eq!(
            row.as_object()
                .expect("an object")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            [
                "id", "kind", "area", "standing", "summary", "body", "tags", "source", "topic"
            ]
        );
    }
}

/// `get` of a pack entry that an override sets to `retired` or `superseded`
/// prints the effective entry: `status` after the entry's own fields, then
/// `topic`, then `overridden` with the replaced field name.
#[test]
fn get_prints_an_inactive_pack_entry_with_its_status_and_the_replaced_field_names() {
    for status in ["retired", "superseded"] {
        let root = tree();
        write_pack(
            root.path(),
            "demo",
            &[("demo", "Demo")],
            &[entry("demo.old", true, "Old.")],
        );
        write_overrides(
            root.path(),
            json!([{"id": "demo.old", "fields": {"status": status}}]),
            json!([]),
        );

        let printed = stdout_json(&run(root.path(), &["get", "demo.old"]));

        let mut expected = entry("demo.old", true, "Old.");
        expected["status"] = json!(status);
        expected["topic"] = json!("demo");
        expected["overridden"] = json!(["status"]);
        assert_eq!(printed, json!([expected]), "{status}");
        assert_eq!(
            first_keys(&printed),
            [
                "id",
                "kind",
                "area",
                "standing",
                "summary",
                "body",
                "tags",
                "source",
                "status",
                "topic",
                "overridden"
            ],
            "{status}"
        );
    }
}

/// A pack entry that an override sets to `retired` or `superseded` is in no
/// `index`, `standing`, `for`, or `topics` output, and the files `render`
/// writes do not name it. The entry that stays active is in all of them.
#[test]
fn an_inactive_pack_entry_leaves_index_standing_for_topics_and_render() {
    for status in ["retired", "superseded"] {
        let root = tree();
        write_pack(
            root.path(),
            "demo",
            &[("demo", "Demo")],
            &[
                entry("demo.old", true, "Old."),
                entry("demo.kept", true, "Kept."),
            ],
        );
        write_overrides(
            root.path(),
            json!([{"id": "demo.old", "fields": {"status": status}}]),
            json!([]),
        );

        assert_inactive_entry_is_out_of_every_read(root.path(), status);
    }
}

/// A pack copy that ships `status: retired` or `status: superseded` needs
/// no override: the entry leaves the same outputs, and `get` prints it with
/// no `overridden`.
#[test]
fn a_pack_copy_that_ships_an_inactive_status_leaves_the_reads_and_get_prints_it_unmarked() {
    for status in ["retired", "superseded"] {
        let root = tree();
        let mut old = entry("demo.old", true, "Old.");
        old["status"] = json!(status);
        write_pack(
            root.path(),
            "demo",
            &[("demo", "Demo")],
            &[old.clone(), entry("demo.kept", true, "Kept.")],
        );

        assert_inactive_entry_is_out_of_every_read(root.path(), status);

        old["topic"] = json!("demo");
        assert_eq!(
            stdout_json(&run(root.path(), &["get", "demo.old"])),
            json!([old]),
            "{status}"
        );
    }
}

/// Asserts that `demo.old` is out of `index`, `standing`, `for`, `topics`,
/// the block of `AGENTS.md`, and the knowledge skill, and that `demo.kept` is
/// in the four reads and in the block. A failure names `status`, the inactive
/// status of `demo.old`.
fn assert_inactive_entry_is_out_of_every_read(root: &Path, status: &str) {
    assert_eq!(
        ids(&stdout_json(&run(root, &["index"]))),
        ["demo.kept"],
        "{status}"
    );
    assert_eq!(
        ids(&stdout_json(&run(root, &["standing"]))),
        ["demo.kept"],
        "{status}"
    );
    let package = stdout_json(&run(root, &["for", "src/any.txt"]));
    assert_eq!(ids(&package["entries"]), ["demo.kept"], "{status}");
    assert_eq!(
        stdout_json(&run(root, &["topics"])),
        json!([{"topic": "demo", "entries": 1, "title": "Demo"}]),
        "{status}"
    );

    let rendered = run(root, &["render"]);
    assert_eq!(
        rendered.status.code(),
        Some(0),
        "{status}, stderr: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let agents_md = fs::read_to_string(root.join("AGENTS.md")).expect("read AGENTS.md");
    assert!(
        agents_md.contains("\n- [demo.kept] Kept.\n"),
        "{status}, AGENTS.md: {agents_md}"
    );
    assert!(
        !agents_md.contains("demo.old"),
        "{status}, AGENTS.md: {agents_md}"
    );
    let skill = fs::read_to_string(
        root.join(".claude")
            .join("skills")
            .join("project-knowledge")
            .join("SKILL.md"),
    )
    .expect("read SKILL.md");
    assert!(
        skill.contains("\ndemo  1  Demo\n"),
        "{status}, SKILL.md: {skill}"
    );
    assert!(!skill.contains("demo.old"), "{status}, SKILL.md: {skill}");
}

/// An excluded entry is in no `index`, `standing`, `for`, or `topics`
/// output.
#[test]
fn an_excluded_entry_leaves_index_standing_for_and_topics() {
    let root = tree();
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[
            entry("demo.kept", true, "Kept."),
            entry("demo.gone", true, "Gone."),
        ],
    );
    write_overrides(root.path(), json!([]), json!([{"id": "demo.gone"}]));

    assert_eq!(
        ids(&stdout_json(&run(root.path(), &["index"]))),
        ["demo.kept"]
    );
    assert_eq!(
        ids(&stdout_json(&run(root.path(), &["standing"]))),
        ["demo.kept"]
    );
    let package = stdout_json(&run(root.path(), &["for", "src/any.txt"]));
    assert_eq!(ids(&package["entries"]), ["demo.kept"]);
    assert_eq!(
        stdout_json(&run(root.path(), &["topics"])),
        json!([{"topic": "demo", "entries": 1, "title": "Demo"}])
    );
}

/// `index --topic` lists the entries of every source for the topic, and
/// `topics` sums the entries of every source.
#[test]
fn topics_and_index_cover_every_source() {
    let root = tree();
    write_local_topic(
        root.path(),
        "demo",
        Some("Local title"),
        &[entry("demo.local", false, "Local.")],
    );
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Pack title")],
        &[entry("demo.a", false, "A."), entry("demo.b", false, "B.")],
    );

    assert_eq!(
        ids(&stdout_json(&run(
            root.path(),
            &["index", "--topic", "demo"]
        ))),
        ["demo.a", "demo.b", "demo.local"]
    );
    assert_eq!(
        stdout_json(&run(root.path(), &["topics"])),
        json!([{"topic": "demo", "entries": 3, "title": "Local title"}])
    );
}

/// A topic's title is the local file's when the file has one, else the
/// first pack's in name order.
#[test]
fn a_topic_title_is_the_local_one_else_the_first_packs() {
    let root = tree();
    write_local_topic(
        root.path(),
        "untitled",
        None,
        &[entry("untitled.local", false, "Local.")],
    );
    write_pack(
        root.path(),
        "zeta",
        &[("shared", "From zeta"), ("untitled", "Zeta untitled")],
        &[entry("shared.z", false, "Z.")],
    );
    write_pack(
        root.path(),
        "alpha",
        &[("shared", "From alpha"), ("untitled", "Alpha untitled")],
        &[entry("shared.a", false, "A.")],
    );

    assert_eq!(
        stdout_json(&run(root.path(), &["topics"])),
        json!([
            {"topic": "shared", "entries": 2, "title": "From alpha"},
            {"topic": "untitled", "entries": 1, "title": "Alpha untitled"},
        ])
    );
}

/// One id in two sources: the local entry wins over a pack entry, and the
/// first pack in name order wins over another pack.
#[test]
fn a_local_entry_wins_over_a_pack_entry_and_the_first_pack_over_another() {
    let root = tree();
    write_local_topic(
        root.path(),
        "demo",
        Some("Demo"),
        &[entry("demo.both", false, "Local text.")],
    );
    write_pack(
        root.path(),
        "alpha",
        &[("demo", "Alpha")],
        &[
            entry("demo.both", false, "Alpha text."),
            entry("demo.pair", false, "Alpha pair."),
        ],
    );
    write_pack(
        root.path(),
        "zeta",
        &[("demo", "Zeta")],
        &[
            entry("demo.both", false, "Zeta text."),
            entry("demo.pair", false, "Zeta pair."),
        ],
    );

    let printed = stdout_json(&run(root.path(), &["get", "demo.both", "demo.pair"]));

    assert_eq!(printed[0]["summary"], json!("Local text."));
    assert_eq!(printed[1]["summary"], json!("Alpha pair."));
}

/// Asserts that `output` is a refusal: exit 2, nothing on stdout, and
/// stderr that is exactly `line` and one newline.
fn assert_one_line_refusal(output: &Output, line: &str) {
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "");
    assert_eq!(String::from_utf8_lossy(&output.stderr), format!("{line}\n"));
}

/// A pack copy that breaks the pack schema is a load error: one line
/// that holds each refusal of `parse_pack` and names the file, exit 2.
#[test]
fn a_bad_pack_copy_is_a_load_error_that_names_the_file() {
    let root = tree();
    write_json(
        &root.path().join("knowledge").join("packs").join("bad.json"),
        &json!({"name": "bad", "schema": "houserules/pack@1.0.0"}),
    );
    let path = root.path().join("knowledge").join("packs").join("bad.json");
    let named = path.display();

    let output = run(root.path(), &["topics"]);

    assert_one_line_refusal(
        &output,
        &format!(
            "{named}: missing \"version\"; {named}: missing \"topics\"; \
             {named}: missing \"areas\"; {named}: missing \"entries\""
        ),
    );
}

/// A pack copy that is not JSON is a load error that names the file and
/// carries the parser's own text.
#[test]
fn a_pack_copy_that_is_not_json_is_a_load_error_that_names_the_file() {
    let root = tree();
    let path = root
        .path()
        .join("knowledge")
        .join("packs")
        .join("broken.json");
    fs::create_dir_all(path.parent().expect("a parent")).expect("create packs/");
    fs::write(&path, "{ not json").expect("write the copy");
    let parse_error = serde_json::from_str::<Value>("{ not json").expect_err("not JSON");

    let output = run(root.path(), &["standing"]);

    assert_one_line_refusal(
        &output,
        &format!("{}: invalid JSON ({parse_error})", path.display()),
    );
}

/// An `overrides.json` that breaks its schema is a load error that names
/// the file and the field.
#[test]
fn a_bad_overrides_file_is_a_load_error_that_names_the_file_and_the_field() {
    let root = tree();
    write_overrides(
        root.path(),
        json!([{"id": "demo.one", "fields": {"kind": "nonsense"}}]),
        json!([]),
    );
    let path = root.path().join("knowledge").join("overrides.json");

    let output = run(root.path(), &["index"]);

    assert_one_line_refusal(
        &output,
        &format!(
            "{}.overrides[0].fields.kind: must be one of \"rule\", \"invariant\", \"gotcha\", \
             \"procedure\", \"decision\", \"history\"",
            path.display()
        ),
    );
}

/// Two pack copies that declare one name are a load error in every
/// command: one line that names the pack and both files, exit 2.
#[test]
fn two_pack_copies_that_declare_one_name_are_a_load_error_that_names_both_files() {
    let root = tree();
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[entry("demo.one", false, "One.")],
    );
    let packs = root.path().join("knowledge").join("packs");
    fs::copy(packs.join("demo.json"), packs.join("other.json")).expect("copy the pack");

    let output = run(root.path(), &["topics"]);

    assert_one_line_refusal(
        &output,
        &format!(
            "{}: pack name \"demo\" is also declared by {}",
            packs.join("demo.json").display(),
            packs.join("other.json").display()
        ),
    );
}

/// A pack copy whose `name` differs from its file stem is a load error:
/// one line that names the file, the declared name, and the stem.
#[test]
fn a_pack_copy_whose_name_differs_from_its_file_stem_is_a_load_error() {
    let root = tree();
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[entry("demo.one", false, "One.")],
    );
    let packs = root.path().join("knowledge").join("packs");
    fs::rename(packs.join("demo.json"), packs.join("renamed.json")).expect("rename the copy");

    let output = run(root.path(), &["standing"]);

    assert_one_line_refusal(
        &output,
        &format!(
            "{}: pack name \"demo\" does not match the file stem \"renamed\"",
            packs.join("renamed.json").display()
        ),
    );
}

/// `overrides.json` is no topic: it never appears in `topics`, whatever
/// it holds.
#[test]
fn overrides_json_is_not_a_topic() {
    let root = tree();
    write_overrides(root.path(), json!([]), json!([]));

    assert_eq!(stdout_json(&run(root.path(), &["topics"])), json!([]));
}

/// `archive` moves local entries only: a pack entry that an override sets
/// to `retired` or `superseded`, or that the pack copy ships with one of
/// them, stays in its copy, and the copy and `overrides.json` stay byte for
/// byte as they were.
#[test]
fn archive_leaves_an_inactive_pack_entry_in_its_copy() {
    for status in ["retired", "superseded"] {
        for shipped in [false, true] {
            let case = format!("{status}, shipped by the pack: {shipped}");
            let root = tree();
            let mut one = entry("demo.one", false, "The pack text.");
            if shipped {
                one["status"] = json!(status);
            } else {
                write_overrides(
                    root.path(),
                    json!([{"id": "demo.one", "fields": {"status": status}}]),
                    json!([]),
                );
            }
            write_pack(root.path(), "demo", &[("demo", "Demo")], &[one]);
            install_shape::mark_json_tracker(root.path());
            fs::create_dir_all(root.path().join("backlog").join("items"))
                .expect("create backlog/items/");
            write_json(
                &root.path().join("backlog").join("batches.json"),
                &json!({"batches": []}),
            );
            let pack_path: PathBuf = root
                .path()
                .join("knowledge")
                .join("packs")
                .join("demo.json");
            let overrides_path: PathBuf = root.path().join("knowledge").join("overrides.json");
            let pack_before = fs::read_to_string(&pack_path).expect("read the copy");
            let overrides_before = fs::read_to_string(&overrides_path).ok();

            let output = run(root.path(), &["archive"]);

            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                "archive: 0 moved\n",
                "{case}, stderr: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                fs::read_to_string(&pack_path).expect("read the copy"),
                pack_before,
                "{case}"
            );
            assert_eq!(
                fs::read_to_string(&overrides_path).ok(),
                overrides_before,
                "{case}"
            );
            assert!(
                !root.path().join("knowledge").join("archive").exists(),
                "{case}: archive wrote an archive directory"
            );
        }
    }
}

/// The text `houserules <command> --help` prints, with each run of
/// whitespace read as one space: clap wraps the text, and a sentence is
/// compared whole. Clap drops the period that ends the last sentence of a
/// command's description.
fn help_text(command: &str) -> String {
    let output = houserules()
        .args([command, "--help"])
        .output()
        .expect("run --help");
    assert_eq!(output.status.code(), Some(0));
    String::from_utf8(output.stdout)
        .expect("utf8 stdout")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `get --help` states the `overridden` and `excluded` fields and that a
/// pack entry with an inactive status prints.
#[test]
fn get_help_states_the_overridden_and_excluded_fields() {
    let help = help_text("get");

    for sentence in [
        "A pack entry with an override prints as its effective entry, with an `overridden` field \
         that lists the replaced fields.",
        "An entry that `knowledge/overrides.json` excludes prints too, with an `excluded` field \
         that holds the reason, or `true` when there is none.",
        "A pack entry with status `retired` or `superseded` prints, which the other commands \
         leave out",
    ] {
        assert!(help.contains(sentence), "missing {sentence:?} in {help}");
    }
}

/// `stats --help` states the `source` of each `rules` row and the `packs`
/// key.
#[test]
fn stats_help_states_the_source_of_each_rule_and_the_packs_key() {
    let help = help_text("stats");

    for sentence in [
        "Each `rules` row names its `source`: `local`, or the name of the pack that ships the \
         entry.",
        "The `packs` key sums entries, standing entries, judged rows, and violations for `local` \
         and for each installed pack",
    ] {
        assert!(help.contains(sentence), "missing {sentence:?} in {help}");
    }
}

/// `topics --help` states that a topic sums every source and leaves out an
/// excluded entry and a pack entry with an inactive status.
#[test]
fn topics_help_states_that_a_topic_sums_every_source() {
    let help = help_text("topics");

    assert!(
        help.contains(
            "A topic sums the entries of every source: the local topic file and each installed \
             pack copy. An excluded entry and a pack entry with status `retired` or \
             `superseded` do not count"
        ),
        "{help}"
    );
}

/// `archive --help` states that it moves local knowledge entries only, and
/// that a pack entry with an inactive status stays in its copy whether the
/// pack ships the status or an override sets it.
#[test]
fn archive_help_states_that_it_moves_local_knowledge_entries_only() {
    let help = help_text("archive");

    assert!(
        help.contains(
            "It moves local knowledge entries only: a pack entry with status `retired` or \
             `superseded` stays in its pack copy, whether the pack ships the status or an \
             override sets it"
        ),
        "{help}"
    );
}

/// Makes `root` a git repository with one commit of everything in it, so
/// that `audit --base HEAD` has a range.
fn commit_everything(root: &Path) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["add", "-A"],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "-m",
            "chore: scratch",
        ],
    ] {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// `audit` leaves a pack entry with an inactive status and an excluded pack
/// entry out of its package: a status that an override sets to `retired` or
/// `superseded`, and one that the pack copy ships. `--ids` and
/// `--sanctioned` name the cause of each, and keep `unknown id` for an id
/// that no source ships.
#[test]
fn audit_leaves_an_inactive_and_an_excluded_pack_entry_out_and_names_the_cause() {
    let root = tree();
    let mut shipped = entry("demo.shipped", true, "Shipped.");
    shipped["status"] = json!("superseded");
    write_pack(
        root.path(),
        "demo",
        &[("demo", "Demo")],
        &[
            entry("demo.kept", true, "Kept."),
            entry("demo.old", true, "Old."),
            entry("demo.stale", true, "Stale."),
            shipped,
            entry("demo.gone", true, "Gone."),
        ],
    );
    write_overrides(
        root.path(),
        json!([
            {"id": "demo.old", "fields": {"status": "retired"}},
            {"id": "demo.stale", "fields": {"status": "superseded"}},
        ]),
        json!([{"id": "demo.gone"}]),
    );
    commit_everything(root.path());

    let package = stdout_json(&run(root.path(), &["audit", "--base", "HEAD"]));
    assert_eq!(ids(&package["rules"]), ["demo.kept"]);

    let inactive = "is a pack entry with status `retired` or `superseded`, which audit leaves out";
    for (id, cause) in [
        ("demo.old", inactive),
        ("demo.stale", inactive),
        ("demo.shipped", inactive),
        (
            "demo.gone",
            "is excluded in knowledge/overrides.json, which audit leaves out",
        ),
    ] {
        let by_ids = run(root.path(), &["audit", "--base", "HEAD", "--ids", id]);
        assert_one_line_refusal(&by_ids, &format!("id \"{id}\" {cause}"));
        let sanctioned = format!("{id}=spec 4.1");
        let by_sanctioned = run(
            root.path(),
            &["audit", "--base", "HEAD", "--sanctioned", &sanctioned],
        );
        assert_one_line_refusal(&by_sanctioned, &format!("--sanctioned \"{id}\" {cause}"));
    }
    let unknown_id = run(
        root.path(),
        &["audit", "--base", "HEAD", "--ids", "demo.none"],
    );
    assert_one_line_refusal(&unknown_id, "unknown id \"demo.none\"");
    let unknown_rule = run(
        root.path(),
        &[
            "audit",
            "--base",
            "HEAD",
            "--sanctioned",
            "demo.none=spec 4.1",
        ],
    );
    assert_one_line_refusal(&unknown_rule, "unknown id \"demo.none\" for --sanctioned");
}
