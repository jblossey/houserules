//! The `runner` check (spec 3.1, 3.3, 7, 10; cases 2, 14): the check
//! type, the `runners` and `workspaces` keys of `.houserules.json`, and the new
//! fields of the check, pack, overrides, and deliverables schemas. Each case
//! runs the compiled binary against a scratch install that `houserules init`
//! made.

mod tracker_support;

use std::path::Path;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{edit_marker, run, run_ok, scratch_git_repo, stderr, stdout, write_json};

/// The line of a malformed `workspaces` value (spec 7).
const WORKSPACES_LINE: &str =
    ".houserules.json: workspaces must be a relative path inside the install root";

/// A scratch install that `houserules init` made.
fn install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    dir
}

/// One schema-valid entry of the topic of its id, in the `global` area.
fn entry(id: &str, check: Value) -> Value {
    json!({
        "id": id,
        "kind": "rule",
        "area": "global",
        "summary": "Run the linter.",
        "body": ["Run it."],
        "tags": ["lint"],
        "source": {"date": "2026-10-22", "by": "user"},
        "check": check,
    })
}

/// Writes the local topic file `knowledge/team.json` with the entry
/// `team.lint`, which holds `check`.
fn add_team_lint(dir: &Path, check: Value) {
    write_json(
        dir,
        "knowledge/team.json",
        &json!({
            "$schema": "./schema.json",
            "topic": "team",
            "title": "Team rules",
            "entries": [entry("team.lint", check)],
        }),
    );
}

/// The check of `team.lint` that names the tool `semgrep`.
fn runner_check() -> Value {
    json!({
        "type": "runner", "level": "fail", "tool": "semgrep",
        "files": ["src/**"], "scope": "changed",
    })
}

/// A well-formed entry of `runners`.
fn tool() -> Value {
    json!({
        "command": ["semgrep", "scan", "--sarif", "--metrics", "off"],
        "parser": "sarif",
        "install": "pip install semgrep==1.2.3",
    })
}

/// `tool()` with `key` set to `value`.
fn tool_with(key: &str, value: Value) -> Value {
    with(tool(), key, value)
}

/// `entry` with `key` set to `value`.
fn with(mut entry: Value, key: &str, value: Value) -> Value {
    entry[key] = value;
    entry
}

/// `tool()` without `key`.
fn tool_without(key: &str) -> Value {
    let mut entry = tool();
    entry.as_object_mut().expect("an object").remove(key);
    entry
}

/// Sets the `runners` key of the marker to `value`.
fn set_runners(dir: &Path, value: Value) {
    edit_marker(dir, |marker| {
        marker.insert("runners".to_string(), value);
    });
}

/// Sets the `workspaces` key of the marker to `value`.
fn set_workspaces(dir: &Path, value: Value) {
    edit_marker(dir, |marker| {
        marker.insert("workspaces".to_string(), value);
    });
}

/// The exit code and the stderr lines of `check-knowledge`.
fn check_knowledge(dir: &Path) -> (i32, Vec<String>) {
    let output = run(dir, &["check-knowledge"]);
    (
        output.status.code().expect("an exit code"),
        stderr(&output).lines().map(str::to_string).collect(),
    )
}

/// Requires `check-knowledge` to fail with exactly the stderr lines
/// `expected`.
fn assert_check_knowledge_fails_with(dir: &Path, expected: &[String], context: &str) {
    let (code, lines) = check_knowledge(dir);
    assert_eq!((code, lines.as_slice()), (1, expected), "{context}");
}

/// Requires `check-knowledge` to pass.
fn assert_check_knowledge_passes(dir: &Path, context: &str) {
    let output = run(dir, &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (Some(0), "knowledge: ok\n".to_string(), String::new()),
        "{context}"
    );
}

/// A line of `check-knowledge` about the marker.
fn marker_line(text: &str) -> String {
    format!(".houserules.json: {text}")
}

// ---- the check shape ----

/// A `runner` check without `tool` gets the line of every other type that
/// misses a required field.
#[test]
fn runner_check_needs_tool() {
    let dir = install();
    add_team_lint(
        dir.path(),
        json!({"type": "runner", "level": "fail", "files": ["src/**"]}),
    );

    assert_check_knowledge_fails_with(
        dir.path(),
        &["knowledge/team.json team.lint: check \"runner\" needs \"tool\"".to_string()],
        "a runner check with no tool",
    );
}

/// A `runner` check whose tool the marker defines passes.
#[test]
fn a_runner_check_that_names_a_defined_tool_passes() {
    let dir = install();
    set_runners(dir.path(), json!({"semgrep": tool()}));
    add_team_lint(dir.path(), runner_check());

    assert_check_knowledge_passes(dir.path(), "a defined tool");
}

/// Spec 3.3, first line: a tool that the marker does not define, with no
/// `runners` key and with a `runners` object that lacks it.
#[test]
fn check_knowledge_names_an_undefined_runner_tool() {
    let dir = install();
    add_team_lint(dir.path(), runner_check());
    let expected = [
        "knowledge/team.json team.lint: runner tool \"semgrep\" is not defined in .houserules.json runners"
            .to_string(),
    ];

    assert_check_knowledge_fails_with(dir.path(), &expected, "no runners key");

    set_runners(dir.path(), json!({"other": tool()}));
    assert_check_knowledge_fails_with(dir.path(), &expected, "a runners object without the tool");
}

/// An entry of an installed pack that names an undefined tool is named at
/// the pack copy that holds it.
#[test]
fn a_pack_entry_that_names_an_undefined_runner_tool_is_named_at_its_pack_copy() {
    let dir = install();
    write_json(dir.path(), "team-pack.json", &demo_pack());
    run_ok(
        dir.path(),
        &["pack", "add", &path_of(dir.path(), "team-pack.json")],
    );

    assert_check_knowledge_fails_with(
        dir.path(),
        &[
            "knowledge/packs/demo.json demo.lint: runner tool \"semgrep\" is not defined in .houserules.json runners"
                .to_string(),
        ],
        "a pack entry with an undefined tool",
    );
}

// ---- the required fields of a check, whatever its source ----

/// A pack of the topic `demo` at pack schema 1.2.0 whose entries are `entries`.
fn demo_pack_of(entries: Vec<Value>) -> Value {
    let mut pack = demo_pack();
    pack["entries"] = Value::Array(entries);
    pack
}

/// Installs the pack `demo` of `entries` into `dir` with `pack add`.
fn add_demo_pack(dir: &Path, entries: Vec<Value>) {
    write_json(dir, "team-pack.json", &demo_pack_of(entries));
    run_ok(dir, &["pack", "add", &path_of(dir, "team-pack.json")]);
}

/// A check that holds every field its type requires.
fn complete_grep_absent_check() -> Value {
    json!({
        "type": "grep-absent", "level": "fail", "files": ["src/**"],
        "pattern": "x", "scope": "changed",
    })
}

/// An install whose pack copy `demo` holds the entry `demo.lint` with `check`.
fn install_with_pack_check(check: Value) -> TempDir {
    let dir = install();
    add_demo_pack(dir.path(), vec![entry("demo.lint", check)]);
    dir
}

/// An install whose pack copy holds `demo.lint` with a complete check and
/// whose `overrides.json` replaces that check with `check`.
fn install_with_override_check(check: Value) -> TempDir {
    let dir = install();
    add_demo_pack(
        dir.path(),
        vec![entry("demo.lint", complete_grep_absent_check())],
    );
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.1.0",
            "overrides": [{"id": "demo.lint", "fields": {"check": check}}],
        }),
    );
    dir
}

/// A line of `check-knowledge` about the entry `demo.lint` of the pack copy.
fn pack_copy_line(text: &str) -> String {
    format!("knowledge/packs/demo.json demo.lint: {text}")
}

/// One check of each type that misses fields, and the lines `check-knowledge`
/// prints for it: the text after `<file> <id>: `.
fn checks_that_miss_a_required_field() -> Vec<(&'static str, Value, Vec<&'static str>)> {
    vec![
        (
            "a grep-absent check without files",
            json!({"type": "grep-absent", "level": "fail", "pattern": "x", "scope": "changed"}),
            vec!["check \"grep-absent\" needs \"files\""],
        ),
        (
            "a grep-absent check with none of its fields",
            json!({"type": "grep-absent", "level": "fail"}),
            vec![
                "check \"grep-absent\" needs \"files\"",
                "check \"grep-absent\" needs \"pattern\"",
                "check \"grep-absent\" needs \"scope\"",
            ],
        ),
        (
            "a commits check with none of its fields",
            json!({"type": "commits", "level": "fail"}),
            vec![
                "check \"commits\" needs \"subject\", \"body_absent\", \"body_line_max\", or \"require_trailer\"",
            ],
        ),
        (
            "a co-change check with neither if nor then",
            json!({"type": "co-change", "level": "fail"}),
            vec![
                "check \"co-change\" needs \"if\"",
                "check \"co-change\" needs \"then\"",
            ],
        ),
        (
            "a diff-append-only check without files",
            json!({"type": "diff-append-only", "level": "fail"}),
            vec!["check \"diff-append-only\" needs \"files\""],
        ),
        (
            "a report-field check with neither if nor field",
            json!({"type": "report-field", "level": "fail"}),
            vec![
                "check \"report-field\" needs \"if\"",
                "check \"report-field\" needs \"field\"",
            ],
        ),
        (
            "a durable-sha check without files",
            json!({"type": "durable-sha", "level": "fail"}),
            vec!["check \"durable-sha\" needs \"files\""],
        ),
        (
            "a runner check without tool",
            json!({"type": "runner", "level": "fail", "files": ["src/**"], "scope": "changed"}),
            vec!["check \"runner\" needs \"tool\""],
        ),
        (
            "an intent-required check without if",
            json!({"type": "intent-required", "level": "fail"}),
            vec!["check \"intent-required\" needs \"if\""],
        ),
    ]
}

/// Spec 3.3: a check of a pack copy that misses a field of its type gets the
/// line of a local check, at the pack copy.
#[test]
fn a_pack_copy_check_that_misses_a_required_field_gets_the_line_of_its_type() {
    for (label, check, texts) in checks_that_miss_a_required_field() {
        let dir = install_with_pack_check(check);
        let expected: Vec<String> = texts.iter().map(|text| pack_copy_line(text)).collect();

        assert_check_knowledge_fails_with(dir.path(), &expected, label);
    }
}

/// Spec 3.3: the effective check of an override gets the same line, at the
/// pack copy that holds the entry. The override listing comes first on stdout.
#[test]
fn an_override_check_that_misses_a_required_field_gets_the_line_of_its_type() {
    for (label, check, texts) in checks_that_miss_a_required_field() {
        let dir = install_with_override_check(check);
        let expected: Vec<String> = texts.iter().map(|text| pack_copy_line(text)).collect();

        assert_check_knowledge_fails_with(dir.path(), &expected, label);
        assert_eq!(
            stdout(&run(dir.path(), &["check-knowledge"])),
            "override demo.lint: check\n",
            "{label}"
        );
    }
}

/// The effective check decides: an override that gives a pack check the tool
/// it lacks leaves nothing to report. This pins today's behavior.
#[test]
fn an_override_that_completes_a_malformed_pack_check_leaves_no_line() {
    let dir = install();
    set_runners(dir.path(), json!({"semgrep": tool()}));
    add_demo_pack(
        dir.path(),
        vec![entry(
            "demo.lint",
            json!({"type": "runner", "level": "fail"}),
        )],
    );
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.1.0",
            "overrides": [{"id": "demo.lint", "fields": {"check": runner_check()}}],
        }),
    );

    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(0),
            "override demo.lint: check\nknowledge: ok\n".to_string(),
            String::new()
        ),
    );
}

/// The lines come in the order of the pack copy, one entry after the other.
#[test]
fn the_required_field_lines_of_a_pack_copy_follow_its_entries() {
    let dir = install();
    add_demo_pack(
        dir.path(),
        vec![
            entry("demo.lint", json!({"type": "runner", "level": "fail"})),
            entry("demo.sound", complete_grep_absent_check()),
            entry("demo.second", json!({"type": "co-change", "level": "warn"})),
        ],
    );

    assert_check_knowledge_fails_with(
        dir.path(),
        &[
            pack_copy_line("check \"runner\" needs \"tool\""),
            "knowledge/packs/demo.json demo.second: check \"co-change\" needs \"if\"".to_string(),
            "knowledge/packs/demo.json demo.second: check \"co-change\" needs \"then\"".to_string(),
        ],
        "two malformed entries and one sound entry",
    );
}

/// An entry that an exclusion keeps out of the active set gets no line. This
/// pins today's behavior.
#[test]
fn an_excluded_pack_entry_with_a_malformed_check_gets_no_line() {
    let dir = install();
    add_demo_pack(
        dir.path(),
        vec![entry(
            "demo.lint",
            json!({"type": "runner", "level": "fail"}),
        )],
    );
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "exclusions": [{"id": "demo.lint"}],
        }),
    );

    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(0),
            "exclusion demo.lint\nknowledge: ok\n".to_string(),
            String::new()
        ),
    );
}

// ---- the `runners` key ----

/// Spec 3.3: each fault of the `runners` object and of one entry is one
/// line that names the first fault of its entry.
#[test]
fn each_malformed_runners_value_is_one_line_naming_its_first_fault() {
    let dir = install();
    let command = marker_line("runners.semgrep.command must be a non-empty array of strings");
    let parser = marker_line("runners.semgrep.parser must be \"sarif\" or \"exit-code\"");
    let install_line = marker_line("runners.semgrep.install must be a non-empty string");
    let cases: Vec<(&str, Value, String)> = vec![
        (
            "an array",
            json!([]),
            marker_line("runners must be an object"),
        ),
        (
            "a string",
            json!("semgrep"),
            marker_line("runners must be an object"),
        ),
        (
            "a number",
            json!(3),
            marker_line("runners must be an object"),
        ),
        (
            "null",
            Value::Null,
            marker_line("runners must be an object"),
        ),
        (
            "an entry that is a string",
            json!({"semgrep": "semgrep"}),
            marker_line("runners.semgrep must be an object"),
        ),
        (
            "no command",
            json!({"semgrep": tool_without("command")}),
            command.clone(),
        ),
        (
            "an empty command",
            json!({"semgrep": tool_with("command", json!([]))}),
            command.clone(),
        ),
        (
            "a command that is a string",
            json!({"semgrep": tool_with("command", json!("semgrep"))}),
            command.clone(),
        ),
        (
            "a command with a number",
            json!({"semgrep": tool_with("command", json!(["semgrep", 3]))}),
            command.clone(),
        ),
        (
            "no parser",
            json!({"semgrep": tool_without("parser")}),
            parser.clone(),
        ),
        (
            "a parser outside the two",
            json!({"semgrep": tool_with("parser", json!("json"))}),
            parser.clone(),
        ),
        (
            "a parser that is a number",
            json!({"semgrep": tool_with("parser", json!(3))}),
            parser.clone(),
        ),
        (
            "no install",
            json!({"semgrep": tool_without("install")}),
            install_line.clone(),
        ),
        (
            "an empty install",
            json!({"semgrep": tool_with("install", json!(""))}),
            install_line.clone(),
        ),
        (
            "an install that is a number",
            json!({"semgrep": tool_with("install", json!(3))}),
            install_line.clone(),
        ),
        (
            "an unknown key",
            json!({"semgrep": tool_with("timeout", json!(5))}),
            marker_line("runners.semgrep has an unknown key \"timeout\""),
        ),
        (
            "a bad command and a bad parser",
            json!({"semgrep": with(tool_with("parser", json!("json")), "command", json!([]))}),
            command.clone(),
        ),
        (
            "a bad parser and a bad install",
            json!({"semgrep": with(tool_with("parser", json!("json")), "install", json!(""))}),
            parser.clone(),
        ),
        (
            "a bad install and an unknown key",
            json!({"semgrep": with(tool_with("install", json!("")), "timeout", json!(5))}),
            install_line.clone(),
        ),
    ];

    for (label, runners, line) in cases {
        set_runners(dir.path(), runners);
        assert_check_knowledge_fails_with(dir.path(), &[line], label);
    }
}

/// Spec 3.3: the entries are checked in key order, one line each; a sound
/// entry gives none.
#[test]
fn several_malformed_runners_entries_give_one_line_each_in_key_order() {
    let dir = install();
    set_runners(
        dir.path(),
        json!({
            "zeta": tool_without("command"),
            "alpha": tool_with("parser", json!(3)),
            "middle": tool(),
            "omega": tool_with("install", json!("")),
        }),
    );

    assert_check_knowledge_fails_with(
        dir.path(),
        &[
            marker_line("runners.zeta.command must be a non-empty array of strings"),
            marker_line("runners.alpha.parser must be \"sarif\" or \"exit-code\""),
            marker_line("runners.omega.install must be a non-empty string"),
        ],
        "three malformed entries and one sound entry",
    );
}

/// A well-formed `runners` object, an empty one included, passes.
#[test]
fn a_well_formed_runners_object_passes() {
    let dir = install();
    set_runners(dir.path(), json!({}));
    assert_check_knowledge_passes(dir.path(), "an empty object");

    set_runners(
        dir.path(),
        json!({
            "semgrep": tool(),
            "ast-grep": tool_with("parser", json!("exit-code")),
        }),
    );
    assert_check_knowledge_passes(dir.path(), "two sound entries");
}

// ---- the `workspaces` key ----

/// Spec 7: a value that is no non-empty string, is absolute, or holds a `..`
/// segment gets the line.
#[test]
fn each_malformed_workspaces_value_gets_its_line() {
    let dir = install();
    for (label, value) in [
        ("a unix absolute path", json!("/var/work")),
        ("a windows drive path", json!("C:\\work")),
        ("a rooted windows path", json!("\\work")),
        ("a parent segment first", json!("../work")),
        ("a parent segment inside", json!("work/../other")),
        ("a windows parent segment", json!("work\\..\\other")),
        ("a lone parent segment", json!("..")),
        ("an empty string", json!("")),
        ("a number", json!(5)),
        ("an array", json!(["work"])),
        ("null", Value::Null),
    ] {
        set_workspaces(dir.path(), value);
        assert_check_knowledge_fails_with(dir.path(), &[WORKSPACES_LINE.to_string()], label);
    }
}

/// A relative value inside the root, one with a dotted name included, is no
/// defect. This pins today's behavior: the marker holds the key without a
/// check, and the new line must not refuse a sound value.
#[test]
fn a_relative_workspaces_path_inside_the_root_passes() {
    let dir = install();
    for value in [
        ".superpowers/sdd",
        "work/",
        "./work",
        ".houserules/work",
        "a..b/work",
    ] {
        set_workspaces(dir.path(), json!(value));
        assert_check_knowledge_passes(dir.path(), value);
    }
}

// ---- the schemas (case 14) ----

/// The path of `relative` below `dir` as text, built one component at a time.
fn path_of(dir: &Path, relative: &str) -> String {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
        .to_str()
        .expect("a UTF-8 path")
        .to_string()
}

/// A pack of the topic `demo` at pack schema 1.2.0, with a `runner` check and
/// a `report-field` check that uses `non_empty` and `covers_ids`.
fn demo_pack() -> Value {
    json!({
        "name": "demo",
        "version": "1.0.0",
        "schema": "houserules/pack@1.2.0",
        "topics": {"demo": {"title": "Demo rules"}},
        "areas": {"global": {"paths": []}},
        "entries": [
            entry("demo.lint", runner_check()),
            entry(
                "demo.report",
                json!({
                    "type": "report-field", "level": "warn", "if": "**",
                    "field": "knowledge_used", "non_empty": true, "covers_ids": true,
                }),
            ),
        ],
    })
}

/// Case 14, the pack half: a copy that declares `houserules/pack@1.2.0` with a
/// `runner` check and each new field installs and passes `check-knowledge`.
#[test]
fn a_pack_copy_at_1_2_0_with_each_new_check_field_validates() {
    let dir = install();
    set_runners(dir.path(), json!({"semgrep": tool()}));
    write_json(dir.path(), "team-pack.json", &demo_pack());

    run_ok(
        dir.path(),
        &["pack", "add", &path_of(dir.path(), "team-pack.json")],
    );

    assert_check_knowledge_passes(dir.path(), "the installed pack");
}

/// Case 14, the override half: an override at `houserules/overrides@1.1.0`
/// sets a `runner` check and the report-field options.
#[test]
fn an_override_with_each_new_check_field_validates() {
    let dir = install();
    set_runners(dir.path(), json!({"semgrep": tool()}));
    let mut pack = demo_pack();
    pack["schema"] = json!("houserules/pack@1.0.0");
    pack["entries"] = json!([
        entry(
            "demo.lint",
            json!({
                "type": "grep-absent", "level": "fail", "files": ["src/**"],
                "pattern": "x", "scope": "changed",
            })
        ),
        entry(
            "demo.report",
            json!({
                "type": "report-field", "level": "warn", "if": "**", "field": "knowledge_used",
            })
        ),
    ]);
    write_json(dir.path(), "team-pack.json", &pack);
    run_ok(
        dir.path(),
        &["pack", "add", &path_of(dir.path(), "team-pack.json")],
    );
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.1.0",
            "overrides": [
                {"id": "demo.lint", "fields": {"check": runner_check()}},
                {"id": "demo.report", "fields": {"check": {
                    "type": "report-field", "level": "warn", "if": "**",
                    "field": "knowledge_used", "non_empty": true, "covers_ids": true,
                }}},
            ],
        }),
    );

    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(0),
            "override demo.lint: check\noverride demo.report: check\nknowledge: ok\n".to_string(),
            String::new()
        ),
        "the overrides"
    );
}

/// The two intent checks of batch 38, one for each entry of a pack copy.
fn intent_checks() -> Vec<Value> {
    vec![
        entry(
            "demo.required",
            json!({
                "type": "intent-required", "level": "warn", "if": ["src/**"],
                "sources": ["file", "openspec"], "exempt_subjects": ["^chore"],
            }),
        ),
        entry(
            "demo.resolves",
            json!({"type": "intent-resolves", "level": "fail"}),
        ),
    ]
}

/// Batch 38, spec 5.7, the pack half: a copy that declares `houserules/pack@1.4.0` with an
/// `intent-required` check that sets `sources`, and an `intent-resolves` check, installs and passes
/// `check-knowledge`.
#[test]
fn a_pack_copy_at_1_4_0_with_each_intent_check_validates() {
    let dir = install();
    let mut pack = demo_pack_of(intent_checks());
    pack["schema"] = json!("houserules/pack@1.4.0");
    write_json(dir.path(), "team-pack.json", &pack);

    run_ok(
        dir.path(),
        &["pack", "add", &path_of(dir.path(), "team-pack.json")],
    );

    assert_check_knowledge_passes(dir.path(), "the installed pack");
}

/// Batch 38, spec 5.7, the override half: an override at `houserules/overrides@1.3.0` sets each
/// intent check, `sources` included.
#[test]
fn an_override_with_each_intent_check_validates() {
    let dir = install();
    add_demo_pack(
        dir.path(),
        vec![
            entry("demo.required", complete_grep_absent_check()),
            entry("demo.resolves", complete_grep_absent_check()),
        ],
    );
    let checks: Vec<Value> = intent_checks()
        .into_iter()
        .map(|entry| entry["check"].clone())
        .collect();
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.3.0",
            "overrides": [
                {"id": "demo.required", "fields": {"check": checks[0]}},
                {"id": "demo.resolves", "fields": {"check": checks[1]}},
            ],
        }),
    );

    let output = run(dir.path(), &["check-knowledge"]);

    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(0),
            "override demo.required: check\noverride demo.resolves: check\nknowledge: ok\n"
                .to_string(),
            String::new()
        )
    );
}

/// A task review that holds `rule_adherence`.
fn review_with(rows: Value) -> Value {
    json!({
        "kind": "task-review", "model": "opus", "task": 2,
        "base": "0cc2a72", "head": "abc1235",
        "spec_compliance": {"verdict": "compliant", "items": []},
        "rule_adherence": rows,
        "strengths": ["The change is small."],
        "issues": [],
        "assessment": {"verdict": "approved", "text": "No finding."},
    })
}

/// Writes `review` to `review.json` in `dir`, runs `validate` on it, and
/// returns the exit code and the errors of the one result row.
fn validate(dir: &Path, review: &Value) -> (i32, Vec<String>) {
    write_json(dir, "review.json", review);
    let output = run(dir, &["validate", &path_of(dir, "review.json")]);
    let code = output.status.code().expect("an exit code");
    let rows: Value = serde_json::from_str(&stdout(&output)).expect("validate prints JSON");
    let errors = rows[0]["errors"]
        .as_array()
        .expect("errors is an array")
        .iter()
        .map(|error| error.as_str().expect("an error is a string").to_string())
        .collect();
    (code, errors)
}

/// A judged row that holds `extra`.
fn judged_row(extra: Value) -> Value {
    let mut row = json!({
        "id": "writing-style.code-comments", "kind": "rule", "mode": "judged",
        "level": null, "result": "fail", "evidence": "a comment narrates a review round",
    });
    for (key, value) in extra.as_object().expect("an object") {
        row[key] = value.clone();
    }
    row
}

/// Spec 4: a judged row of a review may carry `override`.
#[test]
fn review_row_with_override_validates() {
    let dir = install();
    let row = judged_row(json!({
        "override": {"result": "pass", "by": "owner", "reason": "The comment states a constraint."},
    }));

    assert_eq!(
        validate(dir.path(), &review_with(json!([row]))),
        (0, vec![])
    );
}

/// An `override` holds `result`, `by`, and `reason`, and no other key; its
/// result is `pass` or `fail`.
#[test]
fn an_override_needs_a_result_of_pass_or_fail_and_a_by_and_a_reason() {
    let dir = install();
    let full = json!({"result": "pass", "by": "owner", "reason": "A reason."});
    for (label, override_value, error) in [
        (
            "no reason",
            json!({"result": "pass", "by": "owner"}),
            ".rule_adherence[0].override: missing \"reason\"",
        ),
        (
            "no by",
            json!({"result": "pass", "reason": "A reason."}),
            ".rule_adherence[0].override: missing \"by\"",
        ),
        (
            "a result outside the two",
            json!({"result": "open", "by": "owner", "reason": "A reason."}),
            ".rule_adherence[0].override.result: must be one of \"pass\", \"fail\"",
        ),
        (
            "an unknown key",
            {
                let mut value = full.clone();
                value["note"] = json!("x");
                value
            },
            ".rule_adherence[0].override: unknown field \"note\"",
        ),
    ] {
        let (code, errors) = validate(
            dir.path(),
            &review_with(json!([judged_row(json!({"override": override_value}))])),
        );
        assert_eq!(code, 1, "{label}: {errors:?}");
        assert!(
            errors.iter().any(|line| line.ends_with(error)),
            "{label}: {errors:?}"
        );
    }
}

/// Spec 3.2 and 6: an audit row may carry `findings` and `sanctioned`.
#[test]
fn audit_row_with_findings_and_sanctioned_validates() {
    let dir = install();
    let row = json!({
        "id": "team.lint", "kind": "rule", "mode": "deterministic", "level": "fail",
        "result": "fail", "evidence": "src/a.rs:3 no-eval: eval is unsafe",
        "findings": [
            {"path": "src/a.rs", "line": 3, "rule": "no-eval", "message": "eval is unsafe"},
            {"path": null, "line": null, "rule": "(no rule id)", "message": "A finding with no location."},
        ],
        "sanctioned": "controller step 4",
    });

    assert_eq!(
        validate(dir.path(), &review_with(json!([row]))),
        (0, vec![])
    );
}

/// A finding holds `path`, `line`, `rule`, and `message`, and no other key.
#[test]
fn a_finding_needs_its_four_keys() {
    let dir = install();
    for (label, finding, error) in [
        (
            "no rule",
            json!({"path": "src/a.rs", "line": 3, "message": "m"}),
            ".rule_adherence[0].findings[0]: missing \"rule\"",
        ),
        (
            "no message",
            json!({"path": "src/a.rs", "line": 3, "rule": "r"}),
            ".rule_adherence[0].findings[0]: missing \"message\"",
        ),
        (
            "an unknown key",
            json!({"path": "src/a.rs", "line": 3, "rule": "r", "message": "m", "level": "error"}),
            ".rule_adherence[0].findings[0]: unknown field \"level\"",
        ),
        (
            "a path that is a number",
            json!({"path": 3, "line": 3, "rule": "r", "message": "m"}),
            ".rule_adherence[0].findings[0].path: must be string or null",
        ),
    ] {
        let row = json!({
            "id": "team.lint", "kind": "rule", "mode": "deterministic", "level": "fail",
            "result": "fail", "evidence": "e", "findings": [finding],
        });
        let (code, errors) = validate(dir.path(), &review_with(json!([row])));
        assert_eq!(code, 1, "{label}: {errors:?}");
        assert!(
            errors.iter().any(|line| line.ends_with(error)),
            "{label}: {errors:?}"
        );
    }
}
