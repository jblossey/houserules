//! Batch 36, task 8: the regex lines of pack copies and overrides (spec 8.2,
//! HR-226; case 7) and the quoted values of printed commands (spec 8.3,
//! HR-227; case 8). Each case runs the compiled binary against a scratch
//! install that `houserules init` made. The lists of `bd config set` are
//! pinned by `src/tracker/beads/check.rs` and by the Beads scenarios of
//! `tracker_conformance.rs`.

mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    houserules, quoted, read_json, repo_root, resolved, run, run_ok, scratch_git_repo, stderr,
    stdout, write_json,
};

// ---- case 7: regex lines for pack copies and overrides ----

/// A scratch install that `houserules init` made.
fn install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    dir
}

/// One schema-valid entry of the topic of its id, in the `global` area.
fn entry(id: &str, check: &Value) -> Value {
    json!({
        "id": id,
        "kind": "rule",
        "area": "global",
        "summary": "Run the linter.",
        "body": ["Run it."],
        "tags": ["lint"],
        "source": {"date": "2026-10-23", "by": "user"},
        "check": check,
    })
}

/// A check that holds every field its type requires and a valid pattern.
fn sound_check() -> Value {
    json!({
        "type": "grep-absent", "level": "fail", "files": ["src/**"],
        "pattern": "x", "scope": "changed",
    })
}

/// The path of `relative` below `dir`, built one component at a time.
fn path_of(dir: &Path, relative: &str) -> String {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
        .to_str()
        .expect("a UTF-8 path")
        .to_string()
}

/// Installs the pack `demo` with the one entry `demo.lint` that holds `check`.
fn add_demo_pack(dir: &Path, check: &Value) {
    write_json(
        dir,
        "team-pack.json",
        &json!({
            "name": "demo",
            "version": "1.0.0",
            "schema": "houserules/pack@1.2.0",
            "topics": {"demo": {"title": "Demo rules"}},
            "areas": {"global": {"paths": []}},
            "entries": [entry("demo.lint", check)],
        }),
    );
    run_ok(dir, &["pack", "add", &path_of(dir, "team-pack.json")]);
}

/// An install whose pack copy holds `demo.lint` with `check`.
fn install_with_pack_check(check: &Value) -> TempDir {
    let dir = install();
    add_demo_pack(dir.path(), check);
    dir
}

/// An install whose pack copy holds `demo.lint` with a sound check and whose
/// `overrides.json` replaces that check with `check`.
fn install_with_override_check(check: &Value) -> TempDir {
    let dir = install();
    add_demo_pack(dir.path(), &sound_check());
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

/// An install whose local topic file `knowledge/team.json` holds `team.lint`
/// with `check`.
fn install_with_local_check(check: &Value) -> TempDir {
    let dir = install();
    write_json(
        dir.path(),
        "knowledge/team.json",
        &json!({
            "$schema": "./schema.json",
            "topic": "team",
            "title": "Team rules",
            "entries": [entry("team.lint", check)],
        }),
    );
    dir
}

/// The exit code and the stderr lines of `check-knowledge`.
fn check_knowledge(dir: &Path) -> (Option<i32>, Vec<String>) {
    let output = run(dir, &["check-knowledge"]);
    (
        output.status.code(),
        stderr(&output).lines().map(str::to_string).collect(),
    )
}

/// The text of the regex line for `field` after `<file> <id>: `; `message` is
/// the reason of the regular expression engine.
fn regex_text(field: &str, message: &str) -> String {
    format!("check {field} is not a valid regex ({message})")
}

/// One check that holds a malformed regular expression, with the lines that
/// `check-knowledge` prints for it: the text after `<file> <id>: `.
struct Malformed {
    label: &'static str,
    check: Value,
    texts: Vec<String>,
}

/// One check for each field and flag shape that the regex lines of a local
/// entry cover.
fn malformed_checks() -> Vec<Malformed> {
    vec![
        Malformed {
            label: "an unterminated group in a grep-absent pattern",
            check: json!({
                "type": "grep-absent", "level": "fail", "files": ["src/**"],
                "pattern": "(", "scope": "changed",
            }),
            texts: vec![regex_text(
                "pattern",
                "Invalid regular expression: /(/: Unterminated group",
            )],
        },
        Malformed {
            label: "an unterminated class in a grep-absent pattern",
            check: json!({
                "type": "grep-absent", "level": "fail", "files": ["src/**"],
                "pattern": "[a-", "scope": "changed",
            }),
            texts: vec![regex_text(
                "pattern",
                "Invalid regular expression: /[a-/: Unterminated character class",
            )],
        },
        Malformed {
            label: "an unterminated group in a commits subject",
            check: json!({"type": "commits", "level": "fail", "subject": "("}),
            texts: vec![regex_text(
                "subject",
                "Invalid regular expression: /(/: Unterminated group",
            )],
        },
        Malformed {
            label: "an unmatched parenthesis in a commits body_absent",
            check: json!({"type": "commits", "level": "fail", "body_absent": ")"}),
            texts: vec![regex_text(
                "body_absent",
                "Invalid regular expression: /)/: Unmatched ')'",
            )],
        },
        Malformed {
            label: "a malformed item of exempt_subjects",
            check: json!({
                "type": "commits", "level": "fail", "require_trailer": "Work-Item",
                "exempt_subjects": ["^chore: ", "(unclosed"],
            }),
            texts: vec![regex_text(
                "exempt_subjects",
                "Invalid regular expression: /(unclosed/: Unterminated group",
            )],
        },
        Malformed {
            label: "flags that no regular expression takes",
            check: json!({
                "type": "grep-absent", "level": "fail", "files": ["src/**"],
                "pattern": "x", "scope": "changed", "flags": "zz",
            }),
            texts: vec![regex_text(
                "pattern",
                "Invalid flags supplied to RegExp constructor 'zz'",
            )],
        },
    ]
}

/// Spec 8.2, case 7: a check of a pack copy with a pattern that does not
/// compile gets the regex line of a local entry, with the pack copy and the
/// entry id.
#[test]
fn a_pack_copy_check_with_an_unbalanced_pattern_gets_the_regex_line() {
    let check = json!({
        "type": "grep-absent", "level": "fail", "files": ["src/**"],
        "pattern": "(", "scope": "changed",
    });
    let dir = install_with_pack_check(&check);

    assert_eq!(
        check_knowledge(dir.path()),
        (
            Some(1),
            vec![
                "knowledge/packs/demo.json demo.lint: check pattern is not a valid regex (Invalid regular expression: /(/: Unterminated group)"
                    .to_string()
            ]
        )
    );
}

/// Spec 8.2, case 7: the effective check of an override gets the same line,
/// at the pack copy that holds the entry, as the missing-field lines of a pack
/// check do. The override listing comes first on stdout.
#[test]
fn an_override_check_with_an_unbalanced_pattern_gets_the_regex_line() {
    let check = json!({
        "type": "grep-absent", "level": "fail", "files": ["src/**"],
        "pattern": "(", "scope": "changed",
    });
    let dir = install_with_override_check(&check);

    assert_eq!(
        check_knowledge(dir.path()),
        (
            Some(1),
            vec![
                "knowledge/packs/demo.json demo.lint: check pattern is not a valid regex (Invalid regular expression: /(/: Unterminated group)"
                    .to_string()
            ]
        )
    );
    assert_eq!(
        stdout(&run(dir.path(), &["check-knowledge"])),
        "override demo.lint: check\n"
    );
}

/// Spec 8.2: a local entry, a pack copy, and an override give the same text
/// for every field and flag shape, each at the file that holds the entry.
#[test]
fn every_source_of_a_check_gets_the_same_regex_text() {
    for Malformed {
        label,
        check,
        texts,
    } in malformed_checks()
    {
        let sources: [(&str, TempDir, &str); 3] = [
            (
                "a local entry",
                install_with_local_check(&check),
                "knowledge/team.json team.lint",
            ),
            (
                "a pack copy",
                install_with_pack_check(&check),
                "knowledge/packs/demo.json demo.lint",
            ),
            (
                "an override",
                install_with_override_check(&check),
                "knowledge/packs/demo.json demo.lint",
            ),
        ];
        for (source, dir, at) in sources {
            let expected: Vec<String> = texts.iter().map(|text| format!("{at}: {text}")).collect();

            assert_eq!(
                check_knowledge(dir.path()),
                (Some(1), expected),
                "{label}, in {source}"
            );
        }
    }
}

/// The effective check decides: an override that gives a pack check a pattern
/// that compiles leaves nothing to report. This pins today's behavior.
#[test]
fn an_override_that_repairs_a_malformed_pack_pattern_leaves_no_line() {
    let dir = install();
    add_demo_pack(
        dir.path(),
        &json!({
            "type": "grep-absent", "level": "fail", "files": ["src/**"],
            "pattern": "(", "scope": "changed",
        }),
    );
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.1.0",
            "overrides": [{"id": "demo.lint", "fields": {"check": sound_check()}}],
        }),
    );

    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(0),
            "override demo.lint: check\nknowledge: ok\n".to_string(),
            String::new()
        )
    );
}

/// An entry that an exclusion keeps out of the active set gets no regex line.
/// This pins today's behavior.
#[test]
fn an_excluded_pack_entry_with_an_unbalanced_pattern_gets_no_line() {
    let dir = install_with_pack_check(&json!({
        "type": "grep-absent", "level": "fail", "files": ["src/**"],
        "pattern": "(", "scope": "changed",
    }));
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
        )
    );
}

/// Spec 8.2, case 7: every pattern of every kit pack compiles, so a fresh
/// `init` with all five kit packs passes `check-knowledge`.
#[test]
fn a_fresh_init_with_every_kit_pack_passes_check_knowledge() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    for pack in ["process", "quality", "security-hygiene", "writing-style"] {
        run_ok(dir.path(), &["pack", "add", pack]);
    }
    let marker = read_json(dir.path(), ".houserules.json");
    let mut installed: Vec<&str> = marker["packs"]
        .as_object()
        .expect("the marker pins packs")
        .keys()
        .map(String::as_str)
        .collect();
    installed.sort_unstable();
    assert_eq!(
        installed,
        [
            "knowledge-base",
            "process",
            "quality",
            "security-hygiene",
            "writing-style"
        ]
    );

    let output = run(dir.path(), &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stderr(&output)),
        (Some(0), String::new())
    );
}

// ---- case 8: quoted values in printed commands ----

/// A directory named `a b` inside a scratch repository, which holds no install.
fn directory_with_a_space(repo: &Path) -> PathBuf {
    let directory = repo.join("a b");
    fs::create_dir(&directory).expect("create `a b`");
    directory
}

/// Spec 8.3, case 8: the refusal of `update` for a directory that holds no
/// install quotes the `--dir` value where it needs quoting.
#[test]
fn the_no_install_line_quotes_a_directory_with_a_space() {
    let repo = scratch_git_repo();
    let target = directory_with_a_space(repo.path());

    let output = houserules()
        .args(["update", "--dir"])
        .arg(&target)
        .output()
        .expect("run update");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        stderr(&output),
        format!(
            "no houserules install here (no .houserules.json); run houserules init --dir {}\n",
            quoted(&target)
        )
    );
    assert_ne!(
        quoted(&target),
        target.to_str().expect("a UTF-8 path"),
        "a path with a space is quoted"
    );
}

/// Spec 8.3, case 8: a relative path with no shell character prints as the
/// user wrote it, on every platform.
#[test]
fn the_no_install_line_leaves_a_plain_relative_directory_unquoted() {
    let repo = scratch_git_repo();
    fs::create_dir(repo.path().join("sub")).expect("create sub/");

    let output = houserules()
        .args(["update", "--dir", "sub"])
        .current_dir(repo.path())
        .output()
        .expect("run update");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        stderr(&output),
        "no houserules install here (no .houserules.json); run houserules init --dir sub\n"
    );
}

/// Spec 8.3, case 8: the refusal of `update` for a directory below an install
/// quotes the path of its remedy where it needs quoting. The path in front of
/// the colon names a location and stays as it is.
#[test]
fn the_subdirectory_line_quotes_a_root_with_a_space() {
    let repo = install();
    let target = directory_with_a_space(repo.path());

    let output = houserules()
        .args(["update", "--dir"])
        .arg(&target)
        .output()
        .expect("run update");

    let root = resolved(&target);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        stderr(&output),
        format!(
            "{}: this directory is inside the git repository at {} and holds no .houserules.json; run houserules init --dir {} to install there\n",
            root.display(),
            resolved(repo.path()).display(),
            quoted(&root)
        )
    );
    assert_ne!(
        quoted(&root),
        root.to_str().expect("a UTF-8 path"),
        "a path with a space is quoted"
    );
}

// ---- case 8: the migrating-knowledge skill shows the quoted lines ----

/// The template source of the migrating-knowledge skill.
const SKILL: &str = "template/.claude/skills/migrating-knowledge/SKILL.md";

/// The custom types of the records, in the order that `check-backlog` names
/// them.
const RECORD_TYPES: [&str; 5] = [
    "houserules-section",
    "houserules-batch",
    "houserules-amendment",
    "houserules-decision",
    "houserules-parked",
];

/// Spec 8.3: the skill shows the `bd config set` lines in the form that
/// `check-backlog` prints them: the list of types holds commas, so it is
/// quoted; the status `partial:active` holds no shell character and is not.
#[test]
fn the_migrating_knowledge_skill_shows_the_quoted_bd_config_lines() {
    let text = fs::read_to_string(repo_root().join(SKILL)).expect("read the skill");
    let types = shlex::try_quote(&RECORD_TYPES.join(","))
        .expect("a list holds no NUL byte")
        .into_owned();
    let types_line = format!("`bd config set types.custom {types}`");

    assert!(text.contains(&types_line), "the skill lacks {types_line}");
    assert!(
        types.starts_with('\''),
        "a list with commas is quoted: {types}"
    );
    assert!(
        text.contains("`bd config set status.custom partial:active`"),
        "the skill lacks the unquoted status line"
    );
}
