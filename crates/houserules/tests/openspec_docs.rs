//! The documents of the OpenSpec hookup (batch 38, spec 4.1, 5.5, 6, 6.1, 6.2, and 18.1; acceptance
//! cases 8 and 11; HR-209, HR-190, and HR-260): the page `docs/openspec.md`, the batch-38 section of
//! `docs/upgrading-to-2.md`, and the migrating-knowledge skill of the kit.
//!
//! Each test reads a document of the tree beside what the compiled binary prints or does, and shows
//! that the document states it: a line that the binary prints is the literal of the page, a block that
//! the page prints runs as printed in a scratch install, and a step that the page lists changes the
//! scratch install as the page says. The lines of the spec are the literals of these tests, and
//! `the_spec_lines_of_these_tests_are_in_the_spec` shows that the spec holds each. The guidance block
//! of spec 6 lives on the page alone; the README links the page.

mod docs_support;
mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use docs_support::{Block, block_with, blocks, section};
use saphyr::{LoadableYamlNode, Yaml};
use serde_json::{Map, Value, json};
use tempfile::TempDir;
use tracker_support::{
    OPENSPEC_SECTION as SECTION, assert_output, commit_all, copy_dir, edit_marker, flat, git,
    houserules, read_json, repo_root, run, run_ok, scratch_git_repo, stderr, stdout, text_below,
    write_json,
};

/// The version of the binary under test, which the warning of spec 6.2 names.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The spec of the batch, below the repository root.
const SPEC: &str = "docs/specs/2026-10-25-batch-38-openspec-and-release.md";

/// The page of the OpenSpec hookup, below the repository root.
const PAGE: &str = "docs/openspec.md";

/// The upgrade guide, below the repository root.
const GUIDE: &str = "docs/upgrading-to-2.md";

/// The migrating-knowledge skill of the kit, below the repository root.
const SKILL: &str = "template/.claude/skills/migrating-knowledge/SKILL.md";

/// The heading of the batch-38 section of the guide.
const GUIDE_SECTION: &str = "## OpenSpec, links, and install schema 2.4.0";

/// The heading of the hand step for a knowledge schema that the project changed or owns.
const SCHEMA_HAND_STEP: &str = "### Hand step: a knowledge schema that you changed or own";

/// The heading of the page section that holds the warning and the removal steps.
const ONE_WORKFLOW: &str = "## Plan with one workflow";

/// The command of spec 4.1 that writes the validation capture.
const CAPTURE_COMMAND: &str =
    "houserules capture --out <file> -- sh -c 'openspec validate --all --json 2>/dev/null'";

/// The form of the line that refuses a validation capture (spec 4.1). `record write` prints
/// `record:` in place of `evidence:`.
const REFUSAL_FORM: &str = "evidence: --validation <path>: <reason>";

/// The reasons of spec 4.1 for which a validation capture is refused.
const REFUSAL_REASONS: [&str; 7] = [
    "cannot be read (<os error>)",
    "is no capture (<the line of capture's reader>)",
    "ran outside a git work tree",
    "ran at <head7>, not at <head7>",
    "ran in a dirty work tree",
    "holds no JSON (<serde line>)",
    "is no openspec validate document (version \"1.0\" with items)",
];

/// The notice of spec 4.3 for a `validation` input that names no file.
const VALIDATION_NOTICE: &str = "houserules: the validation input names <path>, which does not exist, so this run passes no validation capture";

/// The lines of spec 3.5 that the adapter gives when a named change does not resolve.
const ADAPTER_LINES: [&str; 4] = [
    "no openspec/ folder at the head",
    "the OpenSpec root points at a store, which houserules does not read",
    "an OpenSpec change id is kebab-case: lowercase letters, digits, and single hyphens",
    "no OpenSpec change with that id at the head, active or archived",
];

/// The evidence of an `intent-required` row that fails (spec 5.5).
const REQUIRED_EVIDENCE: &str =
    "no intent of <sources> for <n> changed paths that match the check's if";

/// The evidence of an `intent-resolves` row that fails (spec 5.5).
const RESOLVES_EVIDENCE: &str = "<sha7> Intent: <source>:<ref>: <the adapter's line>";

/// The `Sessions:` line of the packet (spec 5.3).
const SESSIONS_LINE: &str = "Sessions: <sha7> <key>: <value>, ...";

/// The `check-knowledge` line for a bad `review.sessions.trailers` (spec 5.7).
const SESSIONS_KEY_LINE: &str = ".houserules.json: review.sessions.trailers must be an array of trailer keys (letters, digits, and -)";

/// The warning of spec 6.2, with `<version>` for the version of the binary.
const WARNING_TEMPLATE: &str = "houserules: warning: this install runs the reference workflow and the project plans with OpenSpec (openspec/); plan changes with one of them. Beside OpenSpec, houserules needs no reference workflow: https://github.com/jblossey/houserules/blob/v<version>/docs/openspec.md";

/// The lines that the packet prints under an `openspec` intent that has a validation (spec 4.2).
const VALIDATION_LINES: [&str; 2] = [
    "Validation: `openspec` ok: `no issues`",
    "Validation: `openspec` failed: `2 issues`",
];

/// The sentence of the page about the guidance of a store-selected root (spec 6).
const GUIDANCE_STORE_NOTE: &str =
    "With a store-selected OpenSpec root, OpenSpec reads the guidance from the store's config.";

/// The sentence of the page about the section of a store-pointer root (spec 6.1).
const SPLIT_STORE_NOTE: &str = "An install with a store-pointer root gets no section.";

/// The sentence of the page about archived changes and the validation capture (spec 4.1).
const ARCHIVED_NOTE: &str = "The run validates active changes only. An archived change has no item, so its validation is `null`.";

/// The `init` arguments that install the reference workflow with everything it requires.
const INIT_WORKFLOW: [&str; 6] = [
    "init",
    "--workflow",
    "reference",
    "--tracker",
    "json",
    "--add-required-packs",
];

/// The two check types that the page and the skill show one entry of.
const INTENT_CHECKS: [&str; 2] = ["intent-required", "intent-resolves"];

// ---- the documents --------------------------------------------------------------------

/// The text of the file `relative` of the repository.
fn doc(relative: &str) -> String {
    text_below(repo_root(), relative)
}

/// The path of the vendored OpenSpec files that the coexistence tests use.
fn openspec_files() -> PathBuf {
    ["tests", "fixtures", "openspec-coexistence", "files"]
        .iter()
        .fold(PathBuf::from(env!("CARGO_MANIFEST_DIR")), |path, part| {
            path.join(part)
        })
}

/// The path `relative` (`/`-separated) below `dir`.
fn below(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The lines lie in the spec as the tests write them.
#[test]
fn the_spec_lines_of_these_tests_are_in_the_spec() {
    let spec = doc(SPEC);
    let warning = format!("`{WARNING_TEMPLATE}`");
    let section = format!("```markdown\n{SECTION}\n```");
    for line in [
        CAPTURE_COMMAND,
        REFUSAL_FORM,
        VALIDATION_NOTICE,
        REQUIRED_EVIDENCE,
        RESOLVES_EVIDENCE,
        SESSIONS_LINE,
        SESSIONS_KEY_LINE,
        warning.as_str(),
        section.as_str(),
    ] {
        assert!(
            spec.contains(line),
            "the spec does not hold this line as the test does: {line}"
        );
    }
    for reason in REFUSAL_REASONS {
        assert!(
            spec.contains(&format!("`{reason}`")),
            "the spec does not hold this reason as the test does: {reason}"
        );
    }
    for line in ADAPTER_LINES {
        assert!(
            spec.contains(&format!("`{line}`")),
            "the spec does not hold this adapter line as the test does: {line}"
        );
    }
    for text in [
        GUIDANCE_STORE_NOTE.trim_end_matches('.'),
        "a store project gets the warning of 6.2 and no section",
    ] {
        assert!(
            spec.contains(text),
            "the spec does not hold this text as the test does: {text}"
        );
    }
}

// ---- the page: version, capture, notes ------------------------------------------------

/// Spec 6: the page names the OpenSpec version that it was checked against.
#[test]
fn the_page_names_the_version_of_openspec_that_it_was_checked_against() {
    assert!(
        flat(&doc(PAGE)).contains("This page was checked against OpenSpec 1.15.0."),
        "the page names no checked version"
    );
}

/// Spec 4.1 and 4.3: the page prints the command of the capture, each reason of a refusal, the two
/// flags, the `validation` input, the notice, and the archived note.
#[test]
fn the_page_documents_the_validation_capture() {
    let page = doc(PAGE);
    let capture = section(&page, "## The validation capture");

    assert_eq!(
        block_with(&capture, "sh", "houserules capture"),
        CAPTURE_COMMAND
    );
    let flat_capture = flat(&capture);
    for text in [
        REFUSAL_FORM,
        "`evidence --validation <file>`",
        "`record write --validation <file>`",
        "`validation` input",
        VALIDATION_NOTICE,
        ARCHIVED_NOTE,
        VALIDATION_LINES[0],
        VALIDATION_LINES[1],
    ] {
        assert!(
            flat_capture.contains(text),
            "the validation section does not hold: {text}"
        );
    }
    for reason in REFUSAL_REASONS {
        assert!(
            flat_capture.contains(&format!("`{reason}`")),
            "the validation section does not hold this reason: {reason}"
        );
    }
}

/// Spec 3.5, 5.1, 5.3, and 5.5: the page prints the line of each adapter refusal, the `Sessions:` form,
/// and the evidence of each failing check.
#[test]
fn the_page_prints_the_lines_that_the_adapter_and_the_checks_give() {
    let page = flat(&doc(PAGE));
    for line in ADAPTER_LINES {
        assert!(page.contains(line), "the page does not hold: {line}");
    }
    for line in [SESSIONS_LINE, REQUIRED_EVIDENCE, RESOLVES_EVIDENCE] {
        assert!(page.contains(line), "the page does not hold: {line}");
    }
    assert!(
        page.contains("`review.sessions.trailers`") && page.contains("`Entire-Checkpoint`"),
        "the page names the key of the session trailers and its default"
    );
}

// ---- the page: the hookup block -------------------------------------------------------

/// The names of the subcommands of the binary under test.
fn subcommands() -> Vec<String> {
    let output = houserules()
        .arg("--help")
        .output()
        .expect("run houserules --help");
    let help = stdout(&output);
    let mut lines = help.lines().skip_while(|line| *line != "Commands:");
    assert!(lines.next().is_some(), "the help holds no Commands: list");
    lines
        .take_while(|line| !line.is_empty())
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .map(str::to_string)
        .collect()
}

/// The strings of `operations.<operation>.guidance` in the YAML `block`.
fn guidance(block: &str, operation: &str) -> Vec<String> {
    let documents = Yaml::load_from_str(block).expect("the block is YAML");
    let [document] = documents.as_slice() else {
        panic!("the block holds {} YAML documents", documents.len());
    };
    document
        .as_mapping_get("operations")
        .and_then(|operations| operations.as_mapping_get(operation))
        .and_then(|operation| operation.as_mapping_get("guidance"))
        .and_then(Yaml::as_sequence)
        .unwrap_or_else(|| panic!("the block holds no operations.{operation}.guidance list"))
        .iter()
        .map(|item| item.as_str().expect("a guidance string").to_string())
        .collect()
}

/// Spec 6: the page prints the block of the spec, it parses with `saphyr`, and each command that it
/// names is a subcommand of the binary, with the flags that its help names.
#[test]
fn the_hookup_block_is_the_spec_block_and_names_only_commands_of_the_binary() {
    let spec_block = block_with(&doc(SPEC), "yaml", "operations:");
    let page_block = block_with(&doc(PAGE), "yaml", "operations:");
    assert_eq!(
        page_block, spec_block,
        "the page prints the block of the spec"
    );

    let known = subcommands();
    assert!(known.iter().any(|name| name == "audit"), "{known:?}");
    let mut named = Vec::new();
    for operation in ["apply", "archive"] {
        let strings = guidance(&page_block, operation);
        assert!(
            !strings.is_empty(),
            "operations.{operation}.guidance is empty"
        );
        for text in strings {
            let words: Vec<&str> = text.split_whitespace().collect();
            let commands: Vec<&str> = words
                .windows(2)
                .filter(|pair| pair[0] == "houserules")
                .map(|pair| pair[1].trim_end_matches([',', ';', '.']))
                .collect();
            assert!(!commands.is_empty(), "no command in: {text}");
            for command in commands {
                assert!(
                    known.iter().any(|name| name == command),
                    "`{command}` is no subcommand of the binary (in: {text})"
                );
                let help = houserules()
                    .args([command, "--help"])
                    .output()
                    .expect("run --help");
                assert!(help.status.success(), "houserules {command} --help fails");
                let help = stdout(&help);
                for flag in words.iter().filter(|word| word.starts_with("--")) {
                    assert!(
                        help.contains(flag),
                        "`houserules {command}` has no flag {flag} (in: {text})"
                    );
                }
                named.push(command.to_string());
            }
        }
    }
    assert!(
        named.iter().any(|name| name == "for") && named.iter().any(|name| name == "audit"),
        "the block names houserules for and houserules audit: {named:?}"
    );
    assert!(
        flat(&section(&doc(PAGE), "## Guidance for the agent")).contains(GUIDANCE_STORE_NOTE),
        "the page does not say where a store-selected root reads the guidance"
    );
}

// ---- the page: the two example entries ------------------------------------------------

/// The JSON entries of `text` whose check is one of the two intent checks, in order.
fn example_entries(text: &str) -> Vec<Value> {
    blocks(text)
        .into_iter()
        .filter(|block| block.info == "json")
        .filter_map(|block| serde_json::from_str::<Value>(&block.body).ok())
        .filter(|value| {
            value["check"]["type"]
                .as_str()
                .is_some_and(|kind| INTENT_CHECKS.contains(&kind))
        })
        .collect()
}

/// The entry of `entries` whose check has the type `kind`.
fn entry_of_type<'a>(entries: &'a [Value], kind: &str) -> &'a Value {
    let mut found = entries
        .iter()
        .filter(|entry| entry["check"]["type"] == kind);
    let entry = found.next().unwrap_or_else(|| panic!("no entry of {kind}"));
    assert!(found.next().is_none(), "more than one entry of {kind}");
    entry
}

/// A scratch install with the entries `entries` in the topic file of their id prefix, the vendored
/// OpenSpec files tracked, `src/lib.rs`, and the section of spec 6.1 rendered, committed.
fn install_with_entries(entries: &[Value]) -> TempDir {
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init"]);
    let id = entries[0]["id"].as_str().expect("an id");
    let topic = id.split('.').next().expect("a topic");
    write_json(
        repo.path(),
        &format!("knowledge/{topic}.json"),
        &json!({"topic": topic, "title": "Project rules", "entries": entries}),
    );
    copy_dir(&openspec_files(), repo.path());
    write_file(
        repo.path(),
        "openspec/changes/add-login/proposal.md",
        PROPOSAL,
    );
    write_file(
        repo.path(),
        "src/lib.rs",
        "pub fn one() -> u8 {\n    1\n}\n",
    );
    git(repo.path(), &["add", "-A"]);
    run_ok(repo.path(), &["render"]);
    commit_all(repo.path(), "chore: base");
    repo
}

/// The proposal of the change that the tests name.
const PROPOSAL: &str = "# Add login\n\n## Why\n\nUsers need to sign in before they reach their data.\n\n## What Changes\n\n- Add a sign-in form.\n";

/// Writes `text` to `relative` below `dir`, with its folders.
fn write_file(dir: &Path, relative: &str, text: &str) {
    let path = below(dir, relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("create the folders");
    fs::write(path, text).expect("write a file");
}

/// The full sha of HEAD in `dir`.
fn head(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

/// Commits the files `files` (path and text) in `dir` with the subject `subject` and, when `trailer`
/// is set, a body that holds it. The commit skips the hooks.
fn commit(dir: &Path, files: &[(&str, &str)], subject: &str, trailer: Option<&str>) {
    for (path, text) in files {
        write_file(dir, path, text);
    }
    git(dir, &["add", "-A"]);
    let mut args = vec!["commit", "-q", "--no-verify", "-m", subject];
    if let Some(trailer) = trailer {
        args.extend(["-m", trailer]);
    }
    git(dir, &args);
}

/// The result and the evidence of the audit row `id` over the range from `base` to HEAD.
fn audit_row(dir: &Path, base: &str, id: &str) -> (String, String) {
    let output = run(dir, &["audit", "--base", base]);
    let result: Value = serde_json::from_str(&stdout(&output))
        .unwrap_or_else(|error| panic!("the audit result is JSON ({error}): {}", stderr(&output)));
    let row = result["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("the audit holds no row {id}"));
    (
        row["result"].as_str().expect("a result").to_string(),
        row["evidence"].as_str().expect("evidence").to_string(),
    )
}

/// Asserts that `check-knowledge` passes in `dir`.
fn assert_check_knowledge_passes(dir: &Path) {
    assert_output(&run(dir, &["check-knowledge"]), 0, "knowledge: ok\n", "");
}

/// Spec 5.5: the page shows one entry of each check type, and each passes `check-knowledge`.
#[test]
fn each_example_entry_of_the_page_passes_check_knowledge() {
    let entries = example_entries(&doc(PAGE));
    assert_eq!(entries.len(), 2, "the page shows one entry of each type");
    for kind in INTENT_CHECKS {
        let entry = entry_of_type(&entries, kind);
        let repo = install_with_entries(std::slice::from_ref(entry));
        assert_check_knowledge_passes(repo.path());
    }
}

/// Spec 5.5: the entries of the page act as the page says. A change to `src/` with no intent fails
/// `intent-required`; a trailer or a changed change folder passes it; a commit whose subject an
/// `exempt_subjects` pattern matches contributes no path; a trailer that names no change fails
/// `intent-resolves`.
#[test]
fn the_example_entries_act_as_the_page_says() {
    let entries = example_entries(&doc(PAGE));
    let required = entry_of_type(&entries, "intent-required")["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let resolves = entry_of_type(&entries, "intent-resolves")["id"]
        .as_str()
        .expect("an id")
        .to_string();
    let repo = install_with_entries(&entries);
    let dir = repo.path();
    assert_check_knowledge_passes(dir);
    let lib = |value: u8| format!("pub fn one() -> u8 {{\n    {value}\n}}\n");

    let base = head(dir);
    commit(
        dir,
        &[("src/lib.rs", lib(2).as_str())],
        "feat: change the library",
        None,
    );
    let (result, evidence) = audit_row(dir, &base, &required);
    assert_eq!(result, "fail", "{evidence}");
    assert_eq!(
        evidence,
        "no intent of openspec for 1 changed path that match the check's if"
    );
    assert_eq!(audit_row(dir, &base, &resolves).0, "pass");

    let base = head(dir);
    commit(
        dir,
        &[("src/lib.rs", lib(3).as_str())],
        "feat: change the library again",
        Some("Intent: openspec:add-login"),
    );
    assert_eq!(audit_row(dir, &base, &required).0, "pass");
    assert_eq!(audit_row(dir, &base, &resolves).0, "pass");

    let base = head(dir);
    commit(
        dir,
        &[
            ("src/lib.rs", lib(4).as_str()),
            ("openspec/changes/add-login/tasks.md", "- [ ] 1.1 Add\n"),
        ],
        "feat: change the library with its change folder",
        None,
    );
    assert_eq!(audit_row(dir, &base, &required).0, "pass");

    let base = head(dir);
    commit(
        dir,
        &[("src/lib.rs", lib(5).as_str())],
        "chore(deps): bump the library",
        None,
    );
    assert_eq!(
        audit_row(dir, &base, &required),
        (
            "skipped".to_string(),
            "no changed path matches the check's if".to_string()
        )
    );

    let base = head(dir);
    commit(
        dir,
        &[("src/lib.rs", lib(6).as_str())],
        "feat: change the library for a typo",
        Some("Intent: openspec:ad-login"),
    );
    let (result, evidence) = audit_row(dir, &base, &resolves);
    assert_eq!(result, "fail", "{evidence}");
    assert!(
        evidence.ends_with(
            "Intent: openspec:ad-login: no OpenSpec change with that id at the head, active or archived"
        ),
        "{evidence}"
    );
}

// ---- the page: the packet and the capture ---------------------------------------------

/// The files of the change `add-login`: a proposal with a Why, two tasks of which one is done, and a
/// delta spec that adds one requirement.
const CHANGE_FILES: [(&str, &str); 3] = [
    ("openspec/changes/add-login/proposal.md", PROPOSAL),
    (
        "openspec/changes/add-login/tasks.md",
        "## 1. Form\n\n- [x] 1.1 Add the form\n- [ ] 1.2 Test the form\n",
    ),
    (
        "openspec/changes/add-login/specs/auth/spec.md",
        "## ADDED Requirements\n\n### Requirement: Sign in\nThe system SHALL let a user sign in.\n\n#### Scenario: Valid credentials\n- **WHEN** a user submits valid credentials\n- **THEN** the user is signed in\n",
    ),
];

/// A scratch install whose last commit adds the change `add-login` with the trailer
/// `Intent: openspec:add-login`. Returns the install and the commit before it.
fn install_with_change() -> (TempDir, String) {
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init"]);
    let base = commit_all(repo.path(), "chore: houserules init");
    commit(
        repo.path(),
        &CHANGE_FILES,
        "feat: add the sign-in form",
        Some("Intent: openspec:add-login"),
    );
    (repo, base)
}

/// The lines of the packet from the heading `### Intent (1)` up to the next heading, without the
/// blank lines at the end.
fn intent_section(packet: &str) -> String {
    let lines: Vec<&str> = packet
        .lines()
        .skip_while(|line| *line != "### Intent (1)")
        .take_while(|line| *line != "### Failed (0)")
        .collect();
    assert!(
        !lines.is_empty(),
        "the packet holds no Intent section:\n{packet}"
    );
    lines.join("\n").trim_end().to_string()
}

/// Spec 3.3: the page shows the intent section that `evidence` prints for a change, with its title,
/// its Why, its task count, and its requirement counts.
#[test]
fn the_page_shows_the_intent_section_that_evidence_prints() {
    let (repo, base) = install_with_change();
    let output = run_ok(
        repo.path(),
        &["evidence", "--base", &base, "--head", "HEAD"],
    );

    let shown = intent_section(&stdout(&output));

    assert_eq!(
        block_with(&doc(PAGE), "text", "### Intent (1)"),
        shown,
        "the page shows the section as the binary prints it"
    );
    assert!(
        shown.contains("`openspec add-login`: `Add login`"),
        "{shown}"
    );
}

/// Spec 3.2 and 5.1: the key that the page prints enables the adapter, so that the packet finds a change
/// by the paths that the range changes, and the packet prints the `Sessions:` line for a trailer of
/// the default key. Without the key, the range holds no intent.
#[test]
fn the_page_enables_the_adapter_as_printed_and_the_packet_shows_the_sessions() {
    let reads = section(&doc(PAGE), "## What houserules reads");
    let patch: Value = serde_json::from_str(&block_with(&reads, "json", "\"sources\""))
        .expect("the printed key is JSON");
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init"]);
    let base = commit_all(repo.path(), "chore: houserules init");
    commit(
        repo.path(),
        &[(
            "openspec/changes/fix-typo/proposal.md",
            "# Fix a typo\n\n## Why\n\nThe label is wrong.\n",
        )],
        "docs: plan the typo fix",
        Some("Entire-Checkpoint: 01HXYZ"),
    );
    let shown = block_with(
        &section(&doc(PAGE), "## Name a change in a commit"),
        "text",
        "Sessions:",
    );
    let sessions = shown.replace("<sha7>", &head(repo.path())[..7]);
    let packet = |dir: &Path| {
        stdout(&run_ok(
            dir,
            &["evidence", "--base", &base, "--head", "HEAD"],
        ))
    };

    let before = packet(repo.path());
    assert!(before.contains("### Intent (0)"), "{before}");
    assert!(before.contains(&sessions), "{before}");

    edit_marker(repo.path(), |marker| {
        marker.extend(patch.as_object().expect("an object").clone());
    });
    run_ok(repo.path(), &["render"]);
    assert_check_knowledge_passes(repo.path());
    let after = packet(repo.path());
    assert!(
        after.contains("### Intent (1)\n\n`openspec fix-typo`: `Fix a typo`\n"),
        "{after}"
    );
    assert!(after.contains(&sessions), "{after}");
}

/// A shell script named `openspec` in a new folder, which prints `document` on stdout and a line on
/// stderr, as `openspec validate --all --json` does. Returns the folder.
#[cfg(unix)]
fn stub_openspec(document: &str) -> TempDir {
    use std::os::unix::fs::PermissionsExt;
    let folder = tempfile::tempdir().expect("tempdir");
    let script = folder.path().join("openspec");
    fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s\\n' '{document}'\necho 'Totals: done' >&2\n"),
    )
    .expect("write the stub");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("make it executable");
    folder
}

/// Spec 4.1 and 4.2: the command of the page, run as printed with an `openspec` that prints a
/// document, writes a capture that `evidence --validation` reads: the packet prints the line that the
/// page shows. A run with a failing item prints `failed` and the count of its issues. This test
/// needs a shell script that stands in for the `openspec` binary, and Windows runs no script by that
/// name, so it runs on Unix only (`houserules.platform-gated-tests`).
#[cfg(unix)]
#[test]
fn the_capture_command_of_the_page_feeds_evidence() {
    let capture = section(&doc(PAGE), "## The validation capture");
    let shown = block_with(&capture, "text", "Validation:");
    assert_eq!(shown, VALIDATION_LINES.join("\n"));
    let printed = block_with(&capture, "sh", "houserules capture");
    let documents = [
        r#"{"items":[{"id":"add-login","type":"change","valid":true,"issues":[],"durationMs":3}],"summary":{"totals":{"items":1,"passed":1,"failed":0}},"version":"1.0","root":"."}"#,
        r#"{"items":[{"id":"add-login","type":"change","valid":false,"issues":[{"level":"ERROR","path":"proposal","message":"x"},{"level":"WARNING","path":"specs","message":"y"}],"durationMs":3}],"summary":{"totals":{"items":1,"passed":0,"failed":1}},"version":"1.0","root":"."}"#,
    ];
    for (document, expected) in documents.iter().zip(VALIDATION_LINES) {
        let (repo, base) = install_with_change();
        let stub = stub_openspec(document);
        let out = stub.path().join("validation.txt");
        let words = shlex::split(&printed.replace("<file>", out.to_str().expect("a UTF-8 path")))
            .expect("the printed command is shell words");
        assert_eq!(words[0], "houserules");
        let path = std::env::join_paths(std::iter::once(stub.path().to_path_buf()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .expect("join PATH");

        let captured = houserules()
            .args(&words[1..])
            .current_dir(repo.path())
            .env("PATH", path)
            .output()
            .expect("run houserules capture");
        assert_eq!(captured.status.code(), Some(0), "{}", stderr(&captured));

        let packet = run_ok(
            repo.path(),
            &[
                "evidence",
                "--base",
                &base,
                "--head",
                "HEAD",
                "--validation",
                out.to_str().expect("a UTF-8 path"),
            ],
        );
        let printed_lines: Vec<String> = stdout(&packet)
            .lines()
            .filter(|line| line.starts_with("Validation:"))
            .map(str::to_string)
            .collect();
        assert_eq!(printed_lines, [expected]);
    }
}

// ---- the page: the split and the warning ----------------------------------------------

/// A scratch install that tracks the vendored OpenSpec files and runs the reference workflow with the
/// Claude renderer, committed.
fn workflow_install_with_openspec() -> TempDir {
    let repo = scratch_git_repo();
    copy_dir(&openspec_files(), repo.path());
    git(repo.path(), &["add", "-A"]);
    run_ok(repo.path(), &INIT_WORKFLOW);
    run_ok(repo.path(), &["init", "--target", "claude"]);
    commit_all(repo.path(), "chore: houserules init");
    repo
}

/// Spec 6.1 and 6.2: the page prints the section that `render` writes into `AGENTS.md` and the
/// warning that `check-knowledge` prints, each as the binary prints it, and says that a store project
/// gets the warning and no section.
#[test]
fn the_page_shows_the_section_and_the_warning_that_the_binary_prints() {
    let page = doc(PAGE);

    let rendered = scratch_git_repo();
    run_ok(rendered.path(), &["init"]);
    copy_dir(&openspec_files(), rendered.path());
    commit_all(rendered.path(), "chore: add openspec");
    run_ok(rendered.path(), &["render"]);
    let agents = fs::read_to_string(rendered.path().join("AGENTS.md")).expect("read AGENTS.md");
    assert!(agents.contains(&format!("{SECTION}\n")), "{agents}");
    assert_eq!(
        block_with(
            &section(&page, "## The split in AGENTS.md"),
            "markdown",
            "## OpenSpec"
        ),
        SECTION
    );

    let repo = workflow_install_with_openspec();
    let checked = run_ok(repo.path(), &["check-knowledge"]);
    assert_eq!(
        stderr(&checked).trim_end().replace(VERSION, "<version>"),
        WARNING_TEMPLATE
    );
    assert_eq!(
        block_with(
            &section(&page, ONE_WORKFLOW),
            "text",
            "houserules: warning:"
        ),
        WARNING_TEMPLATE
    );

    let split = flat(&section(&page, "## The split in AGENTS.md"));
    assert!(
        split.contains(SPLIT_STORE_NOTE),
        "the split names no store note"
    );
    assert!(
        split.contains("it gets the warning of [Plan with one workflow](#plan-with-one-workflow)"),
        "the store note names the warning"
    );
}

// ---- the page: the removal steps ------------------------------------------------------

/// The files that `houserules files` lists under `key`.
fn listed_files(key: &str) -> Vec<String> {
    let output = houserules()
        .arg("files")
        .output()
        .expect("run houserules files");
    let listing: Value = serde_json::from_str(&stdout(&output)).expect("files prints JSON");
    listing[key]
        .as_array()
        .unwrap_or_else(|| panic!("files lists no {key}"))
        .iter()
        .map(|path| path.as_str().expect("a path").to_string())
        .collect()
}

/// The files that the page tells the project to delete: the one `text` block of the section
/// `ONE_WORKFLOW` whose lines hold no space.
fn files_to_delete(page: &str) -> Vec<String> {
    let found: Vec<Block> = blocks(&section(page, ONE_WORKFLOW))
        .into_iter()
        .filter(|block| block.info == "text" && !block.body.contains(' '))
        .collect();
    assert_eq!(found.len(), 1, "the section holds one list of files");
    found[0].body.lines().map(str::to_string).collect()
}

/// Removes the `SessionStart` entries of `.claude/settings.json` that run the session-start script of
/// the workflow.
fn drop_the_hook_entry(dir: &Path) {
    let mut settings = read_json(dir, ".claude/settings.json");
    let entries = settings["hooks"]["SessionStart"]
        .as_array_mut()
        .expect("SessionStart entries");
    entries.retain(|entry| {
        !entry["hooks"]
            .as_array()
            .expect("hooks")
            .iter()
            .any(|hook| {
                hook["command"]
                    .as_str()
                    .is_some_and(|command| command.contains("tools/reference-session-start.sh"))
            })
    });
    write_json(dir, ".claude/settings.json", &settings);
}

/// Which steps of the page a run of the steps takes.
struct Steps {
    delete_files: bool,
    drop_key: bool,
    drop_hook: bool,
    keep_file: Option<&'static str>,
}

/// Takes the steps of the page in `dir`.
fn take(dir: &Path, files: &[String], steps: &Steps) {
    if steps.delete_files {
        for file in files {
            if steps.keep_file != Some(file.as_str()) {
                fs::remove_file(below(dir, file))
                    .unwrap_or_else(|error| panic!("delete {file}: {error}"));
            }
        }
    }
    if steps.drop_key {
        edit_marker(dir, |marker: &mut Map<String, Value>| {
            assert!(
                marker.remove("workflow").is_some(),
                "the marker has a workflow key"
            );
        });
    }
    if steps.drop_hook {
        drop_the_hook_entry(dir);
    }
}

/// The hook entry of the workflow is in `.claude/settings.json`.
fn holds_the_hook_entry(dir: &Path) -> bool {
    fs::read_to_string(dir.join(".claude").join("settings.json"))
        .expect("read settings.json")
        .contains("tools/reference-session-start.sh")
}

/// Spec 6.2 and case 11: the page lists every file of the reference workflow and the `workflow` key.
#[test]
fn the_page_lists_every_file_of_the_reference_workflow_and_the_key() {
    let page = doc(PAGE);
    let mut listed = files_to_delete(&page);
    let mut workflow = listed_files("workflowFiles");
    listed.sort();
    workflow.sort();
    assert_eq!(
        listed, workflow,
        "the page lists the files that `files` lists"
    );
    let steps = flat(&section(&page, ONE_WORKFLOW));
    assert!(steps.contains("`workflow` key"), "the page names the key");
    for data in listed_files("workflowData") {
        assert!(steps.contains(&data), "the page names {data}");
    }
}

/// Spec 6.2 and case 11: with the steps as the page prints them, a second `update` writes no file of
/// the workflow back and the warning stops. Each step is needed: without the key step, `update`
/// writes the files back; with the key gone and one unmodified file left, it adopts the workflow
/// again; without the hook step, the entry stays in `.claude/settings.json`.
#[test]
fn following_the_removal_steps_stops_the_write_back_and_the_warning() {
    let page = doc(PAGE);
    let files = files_to_delete(&page);
    assert!(!files.is_empty(), "the page lists no file");
    let all = Steps {
        delete_files: true,
        drop_key: true,
        drop_hook: true,
        keep_file: None,
    };

    let repo = workflow_install_with_openspec();
    let dir = repo.path();
    assert!(holds_the_hook_entry(dir));
    assert_eq!(
        stderr(&run_ok(dir, &["check-knowledge"])),
        format!("{}\n", WARNING_TEMPLATE.replace("<version>", VERSION))
    );
    take(dir, &files, &all);
    let updated = run_ok(dir, &["update"]);
    for file in &files {
        assert!(!below(dir, file).exists(), "update wrote {file} back");
        assert!(
            !stdout(&updated).contains(file.as_str()),
            "{}",
            stdout(&updated)
        );
    }
    assert!(
        !fs::read_to_string(dir.join("AGENTS.md"))
            .expect("read AGENTS.md")
            .contains("This install runs the reference workflow."),
        "AGENTS.md still points at the workflow"
    );
    assert!(!holds_the_hook_entry(dir));
    let checked = run_ok(dir, &["check-knowledge"]);
    assert!(
        stdout(&checked).ends_with("knowledge: ok\n"),
        "{}",
        stdout(&checked)
    );
    assert_eq!(stderr(&checked), "", "the warning stops");
    let second = run_ok(dir, &["update"]);
    assert!(
        !stdout(&second)
            .lines()
            .any(|line| line.starts_with("wrote ") || line.starts_with("kept ")),
        "{}",
        stdout(&second)
    );
    for file in &files {
        assert!(
            !below(dir, file).exists(),
            "the second update wrote {file} back"
        );
    }

    let kept_key = workflow_install_with_openspec();
    take(
        kept_key.path(),
        &files,
        &Steps {
            drop_key: false,
            ..all
        },
    );
    let written = stdout(&run_ok(kept_key.path(), &["update"]));
    for file in &files {
        assert!(written.contains(&format!("wrote {file}")), "{written}");
    }

    let one_file_left = workflow_install_with_openspec();
    take(
        one_file_left.path(),
        &files,
        &Steps {
            keep_file: Some(".githooks/commit-msg"),
            ..all
        },
    );
    let written = stdout(&run_ok(one_file_left.path(), &["update"]));
    for file in files.iter().filter(|file| *file != ".githooks/commit-msg") {
        assert!(written.contains(&format!("wrote {file}")), "{written}");
    }

    let kept_hook = workflow_install_with_openspec();
    take(
        kept_hook.path(),
        &files,
        &Steps {
            drop_hook: false,
            ..all
        },
    );
    run_ok(kept_hook.path(), &["update"]);
    assert!(
        holds_the_hook_entry(kept_hook.path()),
        "update leaves the hook entry in .claude/settings.json"
    );
}

// ---- the guide ------------------------------------------------------------------------

/// The batch-38 section of the guide.
fn guide_section() -> String {
    section(&doc(GUIDE), GUIDE_SECTION)
}

/// The text of `section`, with every run of white space collapsed to one space.
fn flat_guide() -> String {
    flat(&guide_section())
}

/// A scratch install made by `init`, committed.
fn plain_install() -> TempDir {
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init"]);
    commit_all(repo.path(), "chore: houserules init");
    repo
}

/// The install schema stamp that `init` writes.
fn stamp(dir: &Path) -> String {
    read_json(dir, ".houserules.json")["schema"]
        .as_str()
        .expect("a schema")
        .to_string()
}

/// Spec 5.7 and 18.1: the section names the schema ids that the upgrade adds or moves: the install
/// stamp as `init` writes it, the pack and overrides ids that carry the intent checks, and the two
/// published documents; and it names the new key and the new source.
#[test]
fn the_guide_names_the_schema_rows_of_the_batch() {
    let section = flat_guide();
    let repo = plain_install();
    assert_eq!(stamp(repo.path()), "houserules/install@2.4.0");
    for id in [
        "houserules/install@2.4.0",
        "houserules/pack@1.4.0",
        "houserules/overrides@1.3.0",
        "houserules/evidence@1.1.0",
        "houserules/change-record@1.1.0",
    ] {
        assert!(
            section.contains(&format!("`{id}`")),
            "the section names no {id}"
        );
    }
    for text in [
        "(openspec.md)",
        "`review.sessions.trailers`",
        "`openspec`",
        "`intent-required`",
        "`intent-resolves`",
        "`sources`",
        "`evidence/1.0.0.json`",
        "`change-record/1.0.0.json`",
        SESSIONS_KEY_LINE,
        VALIDATION_NOTICE,
        "`validation` input",
    ] {
        assert!(section.contains(text), "the section does not hold: {text}");
    }
}

/// Spec 5.7: `update` restamps `2.2.0` and `2.3.0` to `2.4.0` and prints the line that the section
/// shows, once; any other stamp stays.
#[test]
fn the_guide_shows_the_restamp_line_that_update_prints() {
    let line = "wrote .houserules.json (houserules/install@2.4.0)";
    assert!(
        flat_guide().contains(line),
        "the section does not hold the line"
    );
    for previous in ["houserules/install@2.2.0", "houserules/install@2.3.0"] {
        let repo = plain_install();
        edit_marker(repo.path(), |marker| {
            marker.insert("schema".to_string(), json!(previous));
        });
        let updated = run_ok(repo.path(), &["update"]);
        assert!(
            stdout(&updated).lines().any(|printed| printed == line),
            "{previous}: {}",
            stdout(&updated)
        );
        assert_eq!(stamp(repo.path()), "houserules/install@2.4.0");
        assert!(
            !stdout(&run_ok(repo.path(), &["update"])).contains(line),
            "a second update prints the line again"
        );
        assert!(
            flat_guide().contains(&format!("`{previous}`")),
            "{previous}"
        );
    }
}

/// The marker lines that `check-knowledge` prints for a bad `review.sessions.trailers` and for an
/// empty `defaultBranch`, as the section shows them.
#[test]
fn the_guide_shows_the_marker_lines_that_the_binary_prints() {
    let section = flat_guide();
    let repo = plain_install();
    edit_marker(repo.path(), |marker| {
        marker.insert(
            "review".to_string(),
            json!({"sessions": {"trailers": ["bad key"]}}),
        );
    });
    let checked = run(repo.path(), &["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(1));
    assert!(
        stderr(&checked)
            .lines()
            .any(|line| line == SESSIONS_KEY_LINE),
        "{}",
        stderr(&checked)
    );

    edit_marker(repo.path(), |marker| {
        marker.remove("review");
        marker.insert("defaultBranch".to_string(), json!(""));
    });
    let updated = run(repo.path(), &["update"]);
    assert_eq!(updated.status.code(), Some(2));
    let marker = repo.path().join(".houserules.json");
    assert_eq!(
        stderr(&updated),
        format!(
            "{}: defaultBranch must be a non-empty string\n",
            marker.display()
        )
    );
    let checked = run(repo.path(), &["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(1));
    let gate_line = ".houserules.json: defaultBranch must be a non-empty string; kit-owned citations not checked";
    assert!(
        stderr(&checked).lines().any(|line| line == gate_line),
        "{}",
        stderr(&checked)
    );
    let gate_text = format!("`{gate_line}`");
    for line in [
        "`<path>/.houserules.json: defaultBranch must be a non-empty string`",
        gate_text.as_str(),
    ] {
        assert!(
            section.contains(line),
            "the section does not show the line: {line}"
        );
    }
}

/// The kit schema of the project knowledge base as a project would hold it from before the intent
/// checks, changed by the project: without the two types and `sources`, with a `description`.
fn older_project_schema(dir: &Path) {
    let mut schema = read_json(dir, "knowledge/schema.json");
    let check = &mut schema["$defs"]["check"]["properties"];
    let types = check["type"]["enum"].as_array_mut().expect("the types");
    types.retain(|kind| !INTENT_CHECKS.contains(&kind.as_str().expect("a type")));
    check
        .as_object_mut()
        .expect("properties")
        .remove("sources")
        .expect("the kit schema has sources");
    schema["description"] = json!("The schema of my project.");
    write_json(dir, "knowledge/schema.json", &schema);
}

/// A topic file that holds `entry`, an entry of the page with one of the two intent checks.
fn use_the_intent_check(dir: &Path, entry: &Value) {
    write_json(
        dir,
        "knowledge/project.json",
        &json!({"topic": "project", "title": "Project rules", "entries": [entry]}),
    );
}

/// The lines that `check-knowledge` prints for an entry that uses an intent check against a schema
/// that lacks the two types and `sources`, with the topic file and the index of the entry as the
/// section shows them.
fn schema_lines(output: &Output) -> Vec<String> {
    stderr(output)
        .lines()
        .chain(stdout(output).lines())
        .filter(|line| line.starts_with("knowledge/project.json.entries[0].check"))
        .map(|line| {
            line.replace("knowledge/project.json", "knowledge/<topic>.json")
                .replace("entries[0]", "entries[<n>]")
        })
        .collect()
}

/// The lines that `check-knowledge` prints for `entry` against `schema`: the line of the check type,
/// and one line for each field of the check that the schema lacks. The lines carry no topic file and
/// no index.
fn expected_schema_lines(entry: &Value, schema: &Value) -> Vec<String> {
    let check = &schema["$defs"]["check"]["properties"];
    let types: Vec<String> = check["type"]["enum"]
        .as_array()
        .expect("the types")
        .iter()
        .map(Value::to_string)
        .collect();
    let mut lines = vec![format!(".check.type: must be one of {}", types.join(", "))];
    for field in entry["check"].as_object().expect("a check").keys() {
        if check.get(field).is_none() {
            lines.push(format!(".check: unknown field \"{field}\""));
        }
    }
    lines.sort();
    lines
}

/// The lines that `check-knowledge` printed for `entry` are the lines that the schema `schema`
/// gives, and the step shows each of them.
fn assert_schema_lines(lines: &[String], entry: &Value, schema: &Value, step_text: &str) {
    let mut found: Vec<String> = lines
        .iter()
        .map(|line| line.replacen("knowledge/<topic>.json.entries[<n>]", "", 1))
        .collect();
    found.sort();
    assert_eq!(found, expected_schema_lines(entry, schema), "{lines:?}");
    for line in lines {
        assert!(
            step_text.contains(line.as_str()),
            "the step does not show: {line}"
        );
    }
}

/// The two edits that the hand step prints, made to `knowledge/schema.json` of `dir`: the two types
/// in the enum of the check types, and the property `sources` of the snippet.
fn make_the_two_edits(dir: &Path, step_text: &str, property: &Map<String, Value>) {
    let mut schema = read_json(dir, "knowledge/schema.json");
    let check = &mut schema["$defs"]["check"]["properties"];
    for kind in INTENT_CHECKS {
        assert!(step_text.contains(&format!("\"{kind}\"")), "{kind}");
        check["type"]["enum"]
            .as_array_mut()
            .expect("the types")
            .push(json!(kind));
    }
    check
        .as_object_mut()
        .expect("properties")
        .extend(property.clone());
    write_json(dir, "knowledge/schema.json", &schema);
}

/// The property that the snippet of the hand step adds to `$defs.check.properties`.
fn snippet_property(step: &str) -> Map<String, Value> {
    let snippet = block_with(step, "json", "\"sources\"");
    let property: Map<String, Value> =
        serde_json::from_str(&format!("{{{snippet}}}")).expect("the snippet is a JSON property");
    assert!(property.contains_key("sources"), "{snippet}");
    property
}

/// The sentences of the hand step that say which entry draws which line, and why the two edits are
/// not enough on an install of 1.x.
const SCHEMA_LINE_SENTENCES: [&str; 5] = [
    "An entry that uses one of the types draws the first line below.",
    "An entry that sets `sources` draws the second line too",
    "A schema of 1.x is older: it lacks the type `runner` and more fields of a check than `sources`.",
    "An entry that sets such a field draws one more line for it.",
    "The two edits are not enough: an entry that sets `exempt_subjects`, or another field that the schema of 1.x lacks, still draws a line for it.",
];

/// Spec 10.4 and 18.1: on an install of 2.x, a knowledge schema that the project changed is kept
/// and one that it owns is left as found; an entry that uses an intent check then draws the first
/// line, and the second line too when it sets `sources`; and the two edits that the hand step prints
/// make `check-knowledge` pass.
#[test]
fn the_hand_step_for_a_kept_knowledge_schema_clears_the_lines_of_each_entry() {
    let step = section(&guide_section(), SCHEMA_HAND_STEP);
    let step_text = flat(&step);
    let property = snippet_property(&step);
    for sentence in SCHEMA_LINE_SENTENCES {
        assert!(step_text.contains(sentence), "the step lacks: {sentence}");
    }
    let entries = example_entries(&doc(PAGE));
    assert_eq!(entries.len(), 2, "the page shows one entry of each type");

    for entry in &entries {
        for owned in [false, true] {
            let repo = plain_install();
            older_project_schema(repo.path());
            if owned {
                edit_marker(repo.path(), |marker| {
                    marker.insert("ownedPaths".to_string(), json!(["knowledge/schema.json"]));
                });
            }
            let updated = run_ok(repo.path(), &["update"]);
            let kept_line = "kept knowledge/schema.json (locally modified)";
            assert_eq!(
                stdout(&updated).contains(kept_line),
                !owned,
                "{}",
                stdout(&updated)
            );
            assert!(
                owned || step_text.contains(kept_line),
                "the step does not show the line of a kept schema"
            );
            if owned {
                assert!(
                    !stdout(&updated).contains("knowledge/schema.json"),
                    "update prints a line for an owned schema: {}",
                    stdout(&updated)
                );
                assert!(
                    step_text.contains("`ownedPaths`"),
                    "the step names ownedPaths"
                );
            }

            use_the_intent_check(repo.path(), entry);
            let failing = run(repo.path(), &["check-knowledge"]);
            assert_eq!(failing.status.code(), Some(1), "{}", stderr(&failing));
            let schema = read_json(repo.path(), "knowledge/schema.json");
            assert_schema_lines(&schema_lines(&failing), entry, &schema, &step_text);

            make_the_two_edits(repo.path(), &step_text, &property);
            run_ok(repo.path(), &["render"]);
            assert_check_knowledge_passes(repo.path());
        }
    }
}

/// The populations of an install of 1.x whose knowledge schema the conversion lists as a hand step.
#[derive(Clone, Copy)]
enum Schema1x {
    /// A schema that the project changed.
    Changed,
    /// A schema that the `overrides` key of the 1.x marker lists.
    Owned,
}

/// The one install of 1.x that the tests of the guide convert: the marker and the knowledge files
/// of `tests/fixtures/conversion/repo`, committed in a scratch repository, with the knowledge
/// schema `schema`. `update` has converted it. Returns the install and the `hand step:` line that
/// `update` printed for `knowledge/schema.json`.
fn converted_install_of_1_x(schema: Schema1x) -> (TempDir, String) {
    let fixture = below(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "tests/fixtures/conversion/repo",
    );
    let repo = scratch_git_repo();
    fs::copy(
        fixture.join(".houserules.json"),
        repo.path().join(".houserules.json"),
    )
    .expect("copy the marker");
    copy_dir(&fixture.join("knowledge"), &repo.path().join("knowledge"));
    match schema {
        Schema1x::Changed => {
            let mut changed = read_json(repo.path(), "knowledge/schema.json");
            changed["description"] = json!("The schema of my project.");
            write_json(repo.path(), "knowledge/schema.json", &changed);
        }
        Schema1x::Owned => edit_marker(repo.path(), |marker| {
            marker
                .get_mut("overrides")
                .and_then(Value::as_array_mut)
                .expect("the overrides of the 1.x marker")
                .push(json!("knowledge/schema.json"));
        }),
    }
    commit_all(repo.path(), "chore: houserules 1.4.0");
    let updated = run_ok(repo.path(), &["update"]);
    let mut lines: Vec<String> = stdout(&updated)
        .lines()
        .filter(|line| line.starts_with("hand step: ") && line.contains("knowledge/schema.json"))
        .map(str::to_string)
        .collect();
    assert_eq!(lines.len(), 1, "{}", stdout(&updated));
    (repo, lines.remove(0))
}

/// The schema of the kit as `init` writes it, which the reference install of the hand steps holds.
fn kit_schema() -> Value {
    read_json(plain_install().path(), "knowledge/schema.json")
}

/// The area edits that the hand step of an owned schema of 1.x prints: the object `$defs.area` goes,
/// and `area` of an entry takes the text that the line prints.
fn make_the_area_edits(dir: &Path, hand_step: &str) {
    let area = r#"{"type":"string","minLength":1}"#;
    assert!(hand_step.ends_with(area), "{hand_step}");
    let mut schema = read_json(dir, "knowledge/schema.json");
    schema["$defs"]
        .as_object_mut()
        .expect("the definitions")
        .remove("area")
        .expect("the owned schema keeps its area enum");
    schema["$defs"]["entry"]["properties"]["area"] =
        serde_json::from_str(area).expect("the area property");
    write_json(dir, "knowledge/schema.json", &schema);
}

/// Spec 10.4 and 18.1: on an install of 1.x, the conversion lists a changed knowledge schema and an
/// owned one as a hand step, each with its own line. The schema still lacks the two intent types
/// and more, so an entry that uses an intent check draws the lines of the step. The two edits of an
/// install of 2.x clear the lines of an entry of 2.x only: the `intent-required` entry of the page
/// draws the line of `exempt_subjects` after them. The merge of the schema of the kit clears every
/// line, after the area edits for an owned schema.
#[test]
fn the_hand_step_for_a_converted_knowledge_schema_asks_for_the_merge_of_the_kit_schema() {
    let step = section(&guide_section(), SCHEMA_HAND_STEP);
    let step_text = flat(&step);
    let property = snippet_property(&step);
    for sentence in SCHEMA_LINE_SENTENCES {
        assert!(step_text.contains(sentence), "the step lacks: {sentence}");
    }
    let entries = example_entries(&doc(PAGE));
    assert_eq!(entries.len(), 2, "the page shows one entry of each type");
    let kit = kit_schema();

    for schema in [Schema1x::Changed, Schema1x::Owned] {
        for entry in &entries {
            let (repo, hand_step) = converted_install_of_1_x(schema);
            assert!(
                step_text.contains(&hand_step),
                "the step does not show: {hand_step}"
            );
            let converted = read_json(repo.path(), "knowledge/schema.json");
            match schema {
                Schema1x::Changed => {
                    assert!(
                        converted["$defs"].get("area").is_none(),
                        "the conversion keeps the area enum of a changed schema"
                    );
                    assert!(
                        converted["$defs"]["entry"]["properties"]["area"]
                            .get("$ref")
                            .is_none(),
                        "the conversion keeps the $ref of the area property"
                    );
                    assert_eq!(converted["description"], "The schema of my project.");
                }
                Schema1x::Owned => {
                    assert!(
                        converted["$defs"].get("area").is_some(),
                        "the conversion rewrote an owned schema"
                    );
                    let owned = read_json(repo.path(), ".houserules.json")["ownedPaths"].clone();
                    assert!(
                        owned
                            .as_array()
                            .is_some_and(|paths| paths.contains(&json!("knowledge/schema.json"))),
                        "{owned}"
                    );
                }
            }

            use_the_intent_check(repo.path(), entry);
            let failing = run(repo.path(), &["check-knowledge"]);
            assert_schema_lines(&schema_lines(&failing), entry, &converted, &step_text);

            if let Schema1x::Owned = schema {
                make_the_area_edits(repo.path(), &hand_step);
            }
            make_the_two_edits(repo.path(), &step_text, &property);
            let edited = read_json(repo.path(), "knowledge/schema.json");
            let left = schema_lines(&run(repo.path(), &["check-knowledge"]));
            let expected: Vec<String> = if entry["check"].get("exempt_subjects").is_some() {
                vec![
                    "knowledge/<topic>.json.entries[<n>].check: unknown field \"exempt_subjects\""
                        .to_string(),
                ]
            } else {
                Vec::new()
            };
            assert_eq!(left, expected, "after the two edits");
            assert!(
                edited["$defs"]["check"]["properties"]
                    .get("exempt_subjects")
                    .is_none(),
                "the schema of 1.x holds exempt_subjects"
            );

            let mut merged = kit.clone();
            if let Schema1x::Changed = schema {
                merged["description"] = json!("The schema of my project.");
            }
            write_json(repo.path(), "knowledge/schema.json", &merged);
            let fixed = run(repo.path(), &["check-knowledge"]);
            assert_eq!(
                schema_lines(&fixed),
                Vec::<String>::new(),
                "after the merge"
            );
        }
    }
}

/// The bullets of the reds list of the guide section, each with its white space collapsed.
fn red_bullets() -> Vec<String> {
    let section = guide_section();
    let start = section
        .find("### The reds of this section by design")
        .expect("the section holds a reds list");
    section[start..]
        .lines()
        .filter(|line| line.starts_with("- **"))
        .map(flat)
        .collect()
}

/// The bullet of the reds list that starts with the bold text `title`.
fn red_bullet(title: &str) -> String {
    let start = format!("- **{title}**");
    let mut found = red_bullets()
        .into_iter()
        .filter(|bullet| bullet.starts_with(&start));
    let bullet = found
        .next()
        .unwrap_or_else(|| panic!("the reds list holds no bullet {title}"));
    assert!(found.next().is_none(), "two bullets start with {title}");
    bullet
}

/// The anchor of the hand step for a knowledge schema that the project changed or owns.
const SCHEMA_HAND_STEP_LINK: &str = "(#hand-step-a-knowledge-schema-that-you-changed-or-own)";

/// Spec 10.4 and 18.1: the section names each red of the upgrade proof with its remedy: the gates of
/// an install of 1.x exit 2 with the line of D4 until `update`; a knowledge schema that the project
/// changed or owns is kept (2.x) or listed as a hand step (1.x), a bullet for each of the four
/// populations; the install schema is restamped. It names the further reds of the section too: the
/// evals of the reference workflow, an empty `defaultBranch`, and the bodies of the packs that an
/// override replaced. The 1.x lines are the lines that the conversion of the fixture of an install
/// of 1.x prints.
#[test]
fn the_guide_names_each_red_of_the_upgrade_proof_with_its_remedy() {
    let section = flat_guide();
    let reds = section_of_reds(&section);

    let repo = plain_install();
    edit_marker(repo.path(), |marker| {
        marker.remove("schema");
    });
    let checked = run(repo.path(), &["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(2));
    let printed = stderr(&checked);
    let (_, message) = printed
        .trim_end()
        .split_once(".houserules.json: ")
        .expect("the line names the marker");
    assert_eq!(
        message,
        "this is an install of houserules 1.x; run houserules update to convert it"
    );
    let d4 = format!("<path>/.houserules.json: {message}");
    for text in [
        d4.as_str(),
        "`houserules update`",
        "exits 2",
        "wrote .houserules.json (houserules/install@2.4.0)",
        "converted .houserules.json to the install schema houserules/install@2.4.0",
        "defaultBranch must be a non-empty string",
        "`process.evals-rerun`",
    ] {
        assert!(reds.contains(text), "the reds do not hold: {text}");
    }

    let changed_2x =
        red_bullet("`update` keeps a knowledge schema that you changed, on an install of 2.x.");
    for text in [
        "kept knowledge/schema.json (locally modified)",
        SCHEMA_HAND_STEP_LINK,
        "The remedy is the two edits of that step.",
    ] {
        assert!(changed_2x.contains(text), "{changed_2x}");
    }
    let owned_2x = red_bullet(
        "`update` leaves a knowledge schema that `ownedPaths` lists, on an install of 2.x.",
    );
    for text in [
        "It prints no line for the file",
        SCHEMA_HAND_STEP_LINK,
        "The remedy is the two edits of that step.",
    ] {
        assert!(owned_2x.contains(text), "{owned_2x}");
    }
    for (title, schema, remedy) in [
        (
            "The conversion of an install of 1.x lists a knowledge schema that you changed as a hand step.",
            Schema1x::Changed,
            "The remedy is the merge of the schema of the kit that the line asks for.",
        ),
        (
            "The conversion of an install of 1.x lists a knowledge schema that you own as a hand step.",
            Schema1x::Owned,
            "The remedy is the area edits that the line asks for, then the merge of the schema of the kit.",
        ),
    ] {
        let bullet = red_bullet(title);
        for text in [
            "`hand step:`",
            SCHEMA_HAND_STEP_LINK,
            remedy,
            "The two edits of that step are not enough.",
        ] {
            assert!(bullet.contains(text), "{bullet}");
        }
        let (_, hand_step) = converted_install_of_1_x(schema);
        assert!(
            section.contains(&hand_step),
            "the section does not show: {hand_step}"
        );
    }
}

/// The meta-spec of release 2.0.0, below the repository root.
const META_SPEC: &str = "docs/specs/2026-10-15-release-2-0-0-meta-spec.md";

/// Design.md 284 (meta-spec 7): the binary knows the kit files that houserules shipped from 1.0.0
/// on, so `update` keeps the deliverables schema and the dependency-add scenario that 0.3.0 seeded
/// and lists them as hand steps; the reds list names each kept file with the hand step that is its
/// remedy, and the meta-spec states the population.
#[test]
fn the_guide_names_the_kit_files_that_update_keeps_on_an_install_of_0_3_0() {
    let bullet =
        red_bullet("`update` keeps the kit files of an install that houserules 0.3.0 seeded.");
    for text in [
        "`.claude/schemas/deliverables.json`",
        "(#hand-step-a-schema-of-your-own)",
        "every report in the new form fails `validate`",
        "`.claude/evals/dependency-add.json`",
        "(#hand-step-a-file-at-a-workflow-path-that-is-not-the-kits)",
        "`knowledge/schema.json`",
        "(#hand-step-a-schema-with-other-changes)",
        "run `houserules update` again",
    ] {
        assert!(bullet.contains(text), "the bullet lacks {text}: {bullet}");
    }
    assert!(
        flat(&doc(META_SPEC)).contains(
            "the kit files that 0.3.0 seeded are kept and listed as hand steps, design.md 284"
        ),
        "meta-spec 7 does not state the 0.3.0 population"
    );
}

/// Meta-spec 7 (accepted red, review F18): every gate of an install whose CI installs the newest
/// release exits 2 with the line of D4. The meta-spec names four populations: a workflow that 0.3.0,
/// 1.0.0, or 1.1.0 seeded, and a workflow of 1.2.0 or later on a `.houserules.json` with no readable
/// `version`. The bullet of the guide names each of them.
#[test]
fn the_f18_red_names_each_population_of_the_meta_spec() {
    let meta_spec = flat(&doc(META_SPEC));
    for text in [
        "seeded by 0.3.0, 1.0.0, or 1.1.0, or with no readable stamp",
        "from 1.2.0 it installs the stamped version, and the latest release only when `.houserules.json` has no readable `version`",
    ] {
        assert!(meta_spec.contains(text), "the meta-spec lacks: {text}");
    }

    let bullet = red_bullet(
        "Every gate exits 2, on an install of 1.x whose CI installs the newest release.",
    );
    for text in [
        "The workflow that houserules 0.3.0, 1.0.0, or 1.1.0 seeded installs the newest release",
        "and so does a workflow of 1.2.0 or later on a `.houserules.json` with no readable `version`",
    ] {
        assert!(bullet.contains(text), "the bullet lacks: {text}");
    }

    let gates = flat(&section(&doc(GUIDE), "### Gates stop on an install of 1.x"));
    assert!(
        gates.contains("When `.houserules.json` records no readable `version`, a workflow of 1.2.0 or later installs the newest release too"),
        "the section on the gates of an install of 1.x names no such workflow"
    );
}

/// The heading of the hand step for a CI workflow that the project holds.
const WORKFLOW_HAND_STEP: &str = "### Hand step: a CI workflow of your own";

/// The seeded workflow of an install at the git top level, below the install root.
const WORKFLOW_PATH: &str = ".github/workflows/knowledge.yml";

/// The workflow of the kit, below the repository root. A release fetches it from this path.
const KIT_WORKFLOW: &str = "template/.github/workflows/knowledge.yml";

/// A scratch install of 1.x that holds the workflow that 1.2.0 seeded, the same text as the
/// workflow of 1.4.0: the knowledge files of `tests/fixtures/conversion/repo`, committed. With
/// `with_marker`, the install holds the marker of that fixture without its `overrides` entry for the
/// workflow, so that `update` may replace the workflow. Without it, the install holds a
/// `knowledge/` directory and no `.houserules.json`.
fn install_of_1_x_with_workflow(with_marker: bool) -> TempDir {
    let fixture = below(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        "tests/fixtures/conversion/repo",
    );
    let repo = scratch_git_repo();
    copy_dir(&fixture.join("knowledge"), &repo.path().join("knowledge"));
    if with_marker {
        fs::copy(
            fixture.join(".houserules.json"),
            repo.path().join(".houserules.json"),
        )
        .expect("copy the marker");
        edit_marker(repo.path(), |marker| {
            marker
                .get_mut("overrides")
                .and_then(Value::as_array_mut)
                .expect("the overrides of the 1.x marker")
                .retain(|path| path != WORKFLOW_PATH);
        });
    }
    write_file(repo.path(), WORKFLOW_PATH, &released_workflow());
    commit_all(repo.path(), "chore: houserules 1.4.0");
    repo
}

/// The workflow that 1.2.0 seeded, which `released/` keeps. The workflow of 1.4.0 has the same text.
fn released_workflow() -> String {
    text_below(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        "released/knowledge-workflow-1.2.0.yml",
    )
}

/// The exit code, stdout, and stderr of `check-knowledge` and of `check-backlog` in `dir`, which no
/// line may name the workflow file in.
fn gate_outputs(dir: &Path) -> Vec<(Option<i32>, String, String)> {
    ["check-knowledge", "check-backlog"]
        .iter()
        .map(|gate| {
            let output = run(dir, &[gate]);
            assert_eq!(
                lines_naming_the_workflow(&output),
                Vec::<String>::new(),
                "{gate}"
            );
            (output.status.code(), stdout(&output), stderr(&output))
        })
        .collect()
}

/// The lines of `output` (stdout and stderr) that name the workflow file.
fn lines_naming_the_workflow(output: &Output) -> Vec<String> {
    format!("{}{}", stdout(output), stderr(output))
        .lines()
        .filter(|line| line.contains(WORKFLOW_PATH))
        .map(str::to_string)
        .collect()
}

/// Meta-spec 7 and the upgrade proof: an install with a `knowledge/` directory and no
/// `.houserules.json` has no baseline for its seeded workflow, so `update --id-prefix` leaves the
/// file as found, even when it equals a workflow of 1.x, and prints no line for it. An install with
/// a marker gets `wrote .github/workflows/knowledge.yml`. The bullet of the guide states this, the
/// three settings that the workflow of 1.x lacks, and the remedy: the workflow of the kit written over
/// the file. No gate reads the workflow, so the remedy changes no output of a gate, and every later
/// `update` leaves the file as found.
#[test]
fn the_guide_names_the_workflow_that_update_keeps_on_an_install_with_no_marker() {
    let bullet = red_bullet(
        "`update` keeps the seeded CI workflow of an install that has no `.houserules.json`.",
    );
    let kit = doc(KIT_WORKFLOW);
    let released = released_workflow();
    let trimmed =
        |text: &str| -> Vec<String> { text.lines().map(|line| line.trim().to_string()).collect() };
    let held = trimmed(&released);
    let lacking: Vec<String> = trimmed(&kit)
        .into_iter()
        .filter(|line| !held.contains(line))
        .collect();
    assert_eq!(
        lacking,
        [
            "permissions:",
            "contents: read",
            "ref: ${{ github.event.pull_request.head.sha }}",
            "persist-credentials: false"
        ],
        "the lines that the workflow of the kit adds to the workflow of 1.x"
    );
    let fetch = block_with(&section(&doc(GUIDE), WORKFLOW_HAND_STEP), "sh", "curl");
    assert!(
        fetch.ends_with(&format!("/{KIT_WORKFLOW} | diff <path> -")),
        "{fetch}"
    );
    for text in [
        "no `.houserules.json`",
        "`houserules update --id-prefix <prefix>`",
        "(#gates-stop-on-an-install-of-1x)",
        "leaves the file as it found it and prints no line and no hand step for it",
        "even when it equals a workflow that houserules 1.x released",
        "`wrote .github/workflows/knowledge.yml`",
        "`permissions` with `contents: read`",
        "`ref: ${{ github.event.pull_request.head.sha }}`",
        "`persist-credentials: false`",
        "No gate is red because of it.",
        "the `curl` command of [the hand step for a CI workflow of your own](#hand-step-a-ci-workflow-of-your-own)",
        "with `-o <path>` in place of `| diff <path> -`",
        "leaves the new file as found on every later run",
    ] {
        assert!(bullet.contains(text), "the bullet lacks: {text}");
    }

    let stamped = install_of_1_x_with_workflow(true);
    let updated = run_ok(stamped.path(), &["update"]);
    assert!(
        lines_naming_the_workflow(&updated).contains(&format!("wrote {WORKFLOW_PATH}")),
        "{}",
        stdout(&updated)
    );
    assert_eq!(
        fs::read_to_string(below(stamped.path(), WORKFLOW_PATH)).expect("read the workflow"),
        kit
    );

    let repo = install_of_1_x_with_workflow(false);
    let updated = run_ok(repo.path(), &["update", "--id-prefix", "HR"]);
    assert_eq!(lines_naming_the_workflow(&updated), Vec::<String>::new());
    let workflow = below(repo.path(), WORKFLOW_PATH);
    assert_eq!(
        fs::read_to_string(&workflow).expect("read the workflow"),
        released
    );
    let baselines = read_json(repo.path(), ".houserules.json")["baselines"].clone();
    assert!(
        baselines.get(format!(":/{WORKFLOW_PATH}")).is_none(),
        "{baselines}"
    );
    let gates_before = gate_outputs(repo.path());

    fs::write(&workflow, &kit).expect("write the workflow of the kit");
    let again = run_ok(repo.path(), &["update"]);
    assert_eq!(lines_naming_the_workflow(&again), Vec::<String>::new());
    assert_eq!(gate_outputs(repo.path()), gates_before);
    assert_eq!(fs::read_to_string(&workflow).expect("read"), kit);

    let own = format!("{kit}# a line of the project\n");
    fs::write(&workflow, &own).expect("write the workflow of the project");
    let later = run_ok(repo.path(), &["update"]);
    assert_eq!(lines_naming_the_workflow(&later), Vec::<String>::new());
    assert_eq!(fs::read_to_string(&workflow).expect("read"), own);
}

/// Spec 20.3 and design.md 289: the reds list names the red that `check-backlog` gains, an item whose
/// batch no record holds, with its line and its remedy. A run of the binary gives the line and the
/// exit code that the bullet states, and the remedy of the bullet, a record for the batch, clears it.
#[test]
fn the_guide_names_the_red_of_an_item_whose_batch_no_record_holds() {
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init", "--tracker", "json"]);
    run_ok(repo.path(), &["set", "WI-001", "status=done", "batch=7"]);

    let checked = run(repo.path(), &["check-backlog"]);

    assert_eq!(checked.status.code(), Some(1));
    let line = stderr(&checked)
        .trim_end()
        .replace("backlog/items/general.json", "<item file>")
        .replace("WI-001", "<id>")
        .replace("batch 7", "batch <n>");
    let bullet = red_bullet("`check-backlog` fails for an item whose batch no batch record holds.");
    for text in [
        format!("`{line}`"),
        "exits 1".to_string(),
        "`houserules set <id> batch=<n>`".to_string(),
        "The remedy is to add a record for the batch to `backlog/batches.json`".to_string(),
        "or to set the item's `batch` to a batch that a record holds.".to_string(),
    ] {
        assert!(bullet.contains(&text), "the bullet lacks: {text}\n{bullet}");
    }

    let mut batches = read_json(repo.path(), "backlog/batches.json");
    batches["batches"] = json!([{
        "number": 7,
        "items": ["WI-001"],
        "summary": "A batch",
        "kickoff": "kickoff",
        "status": {"state": "done", "text": "done"},
    }]);
    write_json(repo.path(), "backlog/batches.json", &batches);
    assert_output(
        &run(repo.path(), &["check-backlog"]),
        0,
        "backlog: ok\n",
        "",
    );
}

/// The text of the reds list of the guide section.
fn section_of_reds(section: &str) -> String {
    let start = section
        .find("The reds of this section by design")
        .expect("the section holds a reds list");
    section[start..].to_string()
}

/// The version that a kit pack declares at HEAD.
fn kit_pack_version(name: &str) -> String {
    read_json(&repo_root(), &format!("template/packs/{name}.json"))["version"]
        .as_str()
        .expect("a version")
        .to_string()
}

/// Spec 19.9: the section shows the move of each pack that the faster loop changes, as `update`
/// prints it, and the four files of the workflow that `update` writes.
#[test]
fn the_guide_names_the_moves_of_the_faster_loop() {
    let section = flat_guide();
    for (name, old) in [("process", "2.1.0"), ("quality", "1.0.2")] {
        let line = format!("moved pack {name} from {old} to {}", kit_pack_version(name));
        assert!(
            section.contains(&format!("`{line}`")),
            "the section lacks: {line}"
        );
    }
    for file in [
        ".claude/agents/implementer.md",
        ".claude/agents/task-reviewer.md",
        ".claude/skills/orchestrating/SKILL.md",
        ".claude/skills/planning/SKILL.md",
    ] {
        assert!(
            section.contains(&format!("`{file}`")),
            "the section lacks {file}"
        );
    }
    for id in [
        "process.ask-when-missing",
        "process.tdd",
        "process.risk-tiers",
        "process.bounded-review-loop",
        "process.parallel-on-disjoint-files",
        "quality.principles",
    ] {
        assert!(
            section.contains(&format!("`{id}`")),
            "the section lacks {id}"
        );
    }
}

// ---- the skill ------------------------------------------------------------------------

/// The check types of the kit schema, as the skill lists them.
fn schema_types() -> Vec<String> {
    read_json(&repo_root(), "template/knowledge/schema.json")["$defs"]["check"]["properties"]
        ["type"]["enum"]
        .as_array()
        .expect("the types")
        .iter()
        .map(|kind| format!("`{}`", kind.as_str().expect("a type")))
        .collect()
}

/// Spec 5.5: the skill names the evidence document of this release, lists the check types of the
/// kit schema, names the new key and the validation flag, and shows one entry of each intent check
/// that passes `check-knowledge` and equals the entry of the page.
#[test]
fn the_skill_states_the_evidence_schema_the_check_types_and_the_examples() {
    let skill = doc(SKILL);
    let flat_skill = flat(&skill);

    assert!(flat_skill.contains("`houserules/evidence@1.1.0`"));
    assert!(!skill.contains("houserules/evidence@1.0.0"));
    assert!(
        flat_skill.contains(&format!("({})", schema_types().join(", "))),
        "the skill does not list the check types of the kit schema"
    );
    for text in [
        "`review.sessions.trailers`",
        "`--validation <file>`",
        "`Intent: openspec:<id>`",
    ] {
        assert!(flat_skill.contains(text), "the skill does not hold: {text}");
    }

    let entries = example_entries(&skill);
    assert_eq!(entries.len(), 2, "the skill shows one entry of each type");
    assert_eq!(
        entries,
        example_entries(&doc(PAGE)),
        "the skill and the page show the same entries"
    );
    for entry in &entries {
        let repo = install_with_entries(std::slice::from_ref(entry));
        assert_check_knowledge_passes(repo.path());
    }
}
