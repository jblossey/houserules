//! The check types `intent-required` and `intent-resolves` (batch 38, spec 5.5 to 5.7; acceptance
//! case 5; HR-190) and the grep of HR-244 (case 13). Each case runs the compiled binary against a
//! scratch install that `houserules init` made, with the two checks as entries of the project's own.
//!
//! The lines of the spec are the literals of these tests, and `the_spec_lines_of_these_tests_are_in_
//! the_spec` shows that the spec holds each. The evidence that the spec leaves to the command (a
//! check that does not apply, and the two passes) is pinned whole as the command writes it.
//!
//! A test whose doc comment calls it a pin asserts behavior that the code held before the change the
//! file pins; every other test asserts the change.

mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    commit_all, edit_marker, git, repo_root, run, run_ok, scratch_git_repo, stderr, stdout,
    with_default_git, write_json,
};

/// The id of the entry that holds an `intent-required` check.
const REQUIRED: &str = "team.require-intent";

/// The id of the entry that holds an `intent-resolves` check.
const RESOLVES: &str = "team.intents-resolve";

/// The name of git's empty tree, in a repository of the default object format.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// The spec of the batch, below the repository root.
const SPEC: &str = "docs/specs/2026-10-25-batch-38-openspec-and-release.md";

/// The spec of batch 37, below the repository root.
const SPEC_37: &str = "docs/specs/2026-10-24-batch-37-per-change-review-layer.md";

/// The evidence of `intent-resolves` for a range with no `Intent:` trailer (spec 5.5).
const NO_TRAILER: &str = "no Intent: trailer in the range";

/// The evidence of a row that does not apply.
const NOT_APPLICABLE: &str = "no changed path matches the check's if";

/// The line for a change that the tree does not hold (batch-38 spec 3.5).
const NO_SUCH_CHANGE: &str = "no OpenSpec change with that id at the head, active or archived";

/// The line for a value that is no `<source>:<ref>` (batch-37 spec 4.9).
const NOT_SOURCE_AND_REF: &str = "an intent is written <source>:<ref>";

/// The line for an intent source that is no adapter (batch-38 spec 3.6).
const UNKNOWN_SOURCE: &str = "is no intent source; use one of: work-item, file, openspec";

/// The lines of the spec that these tests carry as literals.
#[test]
fn the_spec_lines_of_these_tests_are_in_the_spec() {
    let spec = fs::read_to_string(repo_root().join(SPEC)).expect("read the spec");
    for line in [
        "no Intent: trailer in the range",
        "no intent of <sources> for <n> changed paths that match the check's if",
        "`1 changed path` for one",
        "<sha7> Intent: <source>:<ref>: <the adapter's line>",
        "check \"<type>\" cannot declare \"covers\": it reads no file",
        "<file> <id>: check sources \"<x>\" is no spec adapter; use file or openspec",
        "<file> <id>: check exempt_subjects is not a valid regex (<message>)",
        NO_SUCH_CHANGE,
        "\"<source>\" is no intent source; use one of: work-item, file, openspec",
    ] {
        assert!(
            spec.contains(line),
            "the spec does not hold this line as the test does: {line}"
        );
    }
    let spec_37 = fs::read_to_string(repo_root().join(SPEC_37)).expect("read the spec of batch 37");
    assert!(
        spec_37.contains(NOT_SOURCE_AND_REF),
        "the spec of batch 37 does not hold this line as the test does: {NOT_SOURCE_AND_REF}"
    );
}

// ---- fixtures -------------------------------------------------------------------------

/// A scratch install and its base commit.
struct Project {
    /// The git top level.
    dir: TempDir,
    /// The install root: the top level, or a folder below it.
    root: PathBuf,
    /// The full sha of the base commit.
    base: String,
}

/// One entry of the topic `team`: a standing rule whose check is `check`.
fn entry(id: &str, check: Value) -> Value {
    json!({
        "id": id,
        "kind": "rule",
        "area": "global",
        "standing": true,
        "summary": "A change names the intent it carries.",
        "body": ["Name the spec or the plan of the change in an Intent: trailer."],
        "tags": [],
        "source": {"date": "2026-10-26", "by": "controller"},
        "check": check,
    })
}

/// An `intent-required` check on `src/**` whose fields are `extra`'s, over `{type, level, if}`.
fn required(extra: Value) -> Value {
    let mut check = json!({"type": "intent-required", "level": "fail", "if": ["src/**"]});
    check
        .as_object_mut()
        .expect("an object")
        .extend(extra.as_object().expect("an object").clone());
    check
}

/// An `intent-resolves` check at the level `fail`.
fn resolves() -> Value {
    json!({"type": "intent-resolves", "level": "fail"})
}

/// An install made by `houserules init`, with `entries` in `knowledge/team.json`, `base_files` in
/// the base commit, and the base commit made.
fn project(entries: Vec<Value>, base_files: &[(&str, &str)]) -> Project {
    project_in("", entries, base_files)
}

/// Like [`project`], with the install in the folder `folder` below the git top level (`""` for the
/// top level itself). `base_files` are paths from the install root.
fn project_in(folder: &str, entries: Vec<Value>, base_files: &[(&str, &str)]) -> Project {
    let dir = scratch_git_repo();
    let root = if folder.is_empty() {
        dir.path().to_path_buf()
    } else {
        dir.path().join(folder)
    };
    fs::create_dir_all(&root).expect("create the install folder");
    run_ok(&root, &["init"]);
    write_json(
        &root,
        "knowledge/team.json",
        &json!({"topic": "team", "title": "Team rules", "entries": entries}),
    );
    write(&root, base_files);
    let base = commit_all(dir.path(), "chore: install the kit");
    Project { dir, root, base }
}

/// A project with one `intent-required` check on `src/**` and the extra fields `extra`.
fn requiring(extra: Value) -> Project {
    project(vec![entry(REQUIRED, required(extra))], &[])
}

/// A project with one `intent-resolves` check and an OpenSpec root that holds the change `other`.
fn resolving() -> Project {
    project(
        vec![entry(RESOLVES, resolves())],
        &[("openspec/changes/other/proposal.md", PROPOSAL)],
    )
}

/// The text of a proposal.
const PROPOSAL: &str = "# A change\n\n## Why\n\nBecause.\n";

/// Writes `files` below `dir`, with their folders.
fn write(dir: &Path, files: &[(&str, &str)]) {
    for (path, content) in files {
        let target = dir.join(path);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write a file");
    }
}

/// Writes `files`, commits everything with `subject` and the last paragraph `trailers`, and gives
/// the full sha of the commit.
fn commit(dir: &Path, files: &[(&str, &str)], subject: &str, trailers: &[&str]) -> String {
    write(dir, files);
    git(dir, &["add", "-A"]);
    let body = trailers.join("\n");
    let mut args = vec![
        "commit",
        "-q",
        "--no-verify",
        "--allow-empty",
        "-m",
        subject,
    ];
    if !trailers.is_empty() {
        args.extend(["-m", &body]);
    }
    git(dir, &args);
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

/// The first seven characters of `sha`.
fn sha7(sha: &str) -> &str {
    &sha[..7]
}

/// Runs `audit` over the range from the base of `project` to `HEAD`.
fn audit(project: &Project) -> Output {
    run(&project.root, &["audit", "--base", &project.base])
}

/// The audit row of the entry `id` over the range from `base` to `HEAD`.
fn row(project: &Project, id: &str) -> Value {
    row_of(&audit(project), id)
}

/// The row of the entry `id` in what `audit` printed.
fn row_of(output: &Output, id: &str) -> Value {
    let result: Value = serde_json::from_str(&stdout(output)).unwrap_or_else(|error| {
        panic!(
            "audit printed no JSON ({error}); stderr: {}",
            stderr(output)
        )
    });
    result["rules"]
        .as_array()
        .expect("the rows")
        .iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("the audit has no row for {id}"))
        .clone()
}

/// Requires the row `id` to hold `result` with the evidence `evidence`, whole.
fn assert_row(project: &Project, id: &str, result: &str, evidence: &str) {
    let row = row(project, id);
    assert_eq!(
        (row["result"].as_str(), row["evidence"].as_str()),
        (Some(result), Some(evidence)),
        "{row}"
    );
}

// ---- intent-required ------------------------------------------------------------------

/// Spec case 5: an `Intent:` trailer whose intent resolves satisfies the check. The row names the
/// intent; the file that a `file:` trailer names is no changed path of the check.
#[test]
fn a_trailer_whose_intent_resolves_satisfies_intent_required() {
    let project = requiring(json!({}));
    commit(
        project.dir.path(),
        &[("docs/plan.md", "# Plan\n"), ("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Intent: file:docs/plan.md"],
    );

    assert_row(
        &project,
        REQUIRED,
        "pass",
        "intent file:docs/plan.md for 1 changed path that match the check's if",
    );
}

/// Spec case 5: a changed OpenSpec change folder satisfies the check with no trailer.
#[test]
fn a_changed_change_folder_satisfies_intent_required() {
    let project = requiring(json!({}));
    commit(
        project.dir.path(),
        &[
            ("openspec/changes/add-x/proposal.md", PROPOSAL),
            ("src/a.rs", "fn a() {}\n"),
            ("src/b.rs", "fn b() {}\n"),
        ],
        "feat: add a and b",
        &[],
    );

    assert_row(
        &project,
        REQUIRED,
        "pass",
        "intent openspec:add-x for 2 changed paths that match the check's if",
    );
}

/// Spec case 5: a changed file that `review.intent.files` matches is a `file` intent, which
/// satisfies the check when `sources` lists `file`.
#[test]
fn a_changed_plan_file_satisfies_intent_required_by_the_configured_globs() {
    let project = requiring(json!({"sources": ["file"]}));
    edit_marker(project.dir.path(), |marker| {
        marker.insert(
            "review".to_string(),
            json!({"intent": {"files": ["docs/plans/*.md"]}}),
        );
    });
    commit(
        project.dir.path(),
        &[("docs/plans/p.md", "# Plan\n"), ("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &[],
    );

    assert_row(
        &project,
        REQUIRED,
        "pass",
        "intent file:docs/plans/p.md for 1 changed path that match the check's if",
    );
}

/// Spec case 5 and 5.5: a change with neither fails, with the evidence line of the spec; one
/// changed path reads `1 changed path`, two read `2 changed paths`.
#[test]
fn a_change_with_neither_a_trailer_nor_a_change_folder_fails_intent_required() {
    let one = requiring(json!({}));
    commit(
        one.dir.path(),
        &[("src/a.rs", "fn a() {}\n"), ("docs/x.md", "x\n")],
        "feat: add a",
        &[],
    );
    assert_row(
        &one,
        REQUIRED,
        "fail",
        "no intent of file, openspec for 1 changed path that match the check's if",
    );

    let two = requiring(json!({}));
    commit(
        two.dir.path(),
        &[("src/a.rs", "fn a() {}\n"), ("src/b.rs", "fn b() {}\n")],
        "feat: add a and b",
        &[],
    );
    assert_row(
        &two,
        REQUIRED,
        "fail",
        "no intent of file, openspec for 2 changed paths that match the check's if",
    );
}

/// Spec case 5: a `Work-Item:` trailer is no intent.
#[test]
fn only_a_work_item_trailer_fails_intent_required() {
    let project = requiring(json!({}));
    commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Work-Item: WI-001"],
    );

    assert_row(
        &project,
        REQUIRED,
        "fail",
        "no intent of file, openspec for 1 changed path that match the check's if",
    );
}

/// 5.5: a trailer whose source `sources` leaves out, and one that does not resolve, count for
/// nothing; the evidence prints the sources in the order of the entry.
#[test]
fn a_trailer_of_another_source_or_one_that_does_not_resolve_is_no_intent() {
    let file_only = requiring(json!({"sources": ["file"]}));
    commit(
        file_only.dir.path(),
        &[
            ("openspec/changes/add-x/proposal.md", PROPOSAL),
            ("src/a.rs", "fn a() {}\n"),
        ],
        "feat: add a",
        &["Intent: openspec:add-x"],
    );
    assert_row(
        &file_only,
        REQUIRED,
        "fail",
        "no intent of file for 1 changed path that match the check's if",
    );

    let both_reversed = requiring(json!({"sources": ["openspec", "file"]}));
    commit(
        both_reversed.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Intent: openspec:ghost", "Intent: file:docs/missing.md"],
    );
    assert_row(
        &both_reversed,
        REQUIRED,
        "fail",
        "no intent of openspec, file for 1 changed path that match the check's if",
    );

    let openspec_only = requiring(json!({"sources": ["openspec"]}));
    commit(
        openspec_only.dir.path(),
        &[
            ("openspec/changes/add-x/proposal.md", PROPOSAL),
            ("src/a.rs", "fn a() {}\n"),
        ],
        "feat: add a",
        &["Intent: openspec:add-x"],
    );
    assert_row(
        &openspec_only,
        REQUIRED,
        "pass",
        "intent openspec:add-x for 1 changed path that match the check's if",
    );
}

/// Spec case 5: a range whose only change to a matching path is an exempt commit's does not apply
/// (a `skipped` row, which the packet counts as a check that did not apply); a range that changes
/// no matching path at all does not apply either.
#[test]
fn a_range_with_no_matching_path_of_a_non_exempt_commit_does_not_apply() {
    let exempt = requiring(json!({"exempt_subjects": ["^chore\\(deps\\)"]}));
    commit(
        exempt.dir.path(),
        &[("src/lock.rs", "pinned\n")],
        "chore(deps): bump the lock",
        &[],
    );
    assert_row(&exempt, REQUIRED, "skipped", NOT_APPLICABLE);

    let elsewhere = requiring(json!({}));
    commit(
        elsewhere.dir.path(),
        &[("docs/x.md", "x\n")],
        "docs: add x",
        &[],
    );
    assert_row(&elsewhere, REQUIRED, "skipped", NOT_APPLICABLE);
}

/// Spec case 5: a mixed range is judged by the paths of the non-exempt commits. A path that an
/// exempt and a non-exempt commit both change counts once.
#[test]
fn a_mixed_range_is_judged_by_the_paths_of_the_non_exempt_commits() {
    let project = requiring(json!({"exempt_subjects": ["^chore\\(deps\\)"]}));
    commit(
        project.dir.path(),
        &[("src/lock.rs", "pinned\n"), ("src/shared.rs", "one\n")],
        "chore(deps): bump the lock",
        &[],
    );
    commit(
        project.dir.path(),
        &[("src/shared.rs", "two\n"), ("src/own.rs", "own\n")],
        "feat: add own",
        &[],
    );

    assert_row(
        &project,
        REQUIRED,
        "fail",
        "no intent of file, openspec for 2 changed paths that match the check's if",
    );
}

/// The `flags` of the check apply to `exempt_subjects`, and a pattern matches anywhere in the
/// subject unless it anchors itself, as in a `commits` check.
#[test]
fn the_exempt_subjects_take_the_flags_of_the_check() {
    let project = requiring(json!({"exempt_subjects": ["BUMP"], "flags": "i"}));
    commit(
        project.dir.path(),
        &[("src/lock.rs", "pinned\n")],
        "chore(deps): bump the lock",
        &[],
    );

    assert_row(&project, REQUIRED, "skipped", NOT_APPLICABLE);
}

/// Spec case 5: a merge from the base branch contributes nothing, and the commits that it brings are
/// no commit of the range.
#[test]
fn a_merge_from_the_base_branch_is_ignored() {
    let project = requiring(json!({}));
    let dir = project.dir.path();
    let trunk = git(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string();
    git(dir, &["checkout", "-q", "-b", "feat"]);
    commit(dir, &[("docs/f.md", "f\n")], "docs: add f", &[]);
    git(dir, &["checkout", "-q", &trunk]);
    commit(
        dir,
        &[("src/m.rs", "fn m() {}\n")],
        "feat: on the trunk",
        &[],
    );
    git(dir, &["checkout", "-q", "feat"]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "merge",
            "-q",
            "--no-ff",
            &trunk,
            "-m",
            "merge the trunk",
        ],
    );
    let project = Project {
        base: trunk,
        ..project
    };

    assert_row(&project, REQUIRED, "skipped", NOT_APPLICABLE);
}

/// 5.5: the level of the check is the level of a violation.
#[test]
fn a_violation_of_a_warn_check_is_a_warn_row() {
    let project = requiring(json!({"level": "warn"}));
    commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &[],
    );

    assert_row(
        &project,
        REQUIRED,
        "warn",
        "no intent of file, openspec for 1 changed path that match the check's if",
    );
}

/// A source that is no spec adapter stops the audit with one named line, as a malformed check does.
#[test]
fn an_audit_names_a_source_that_is_no_spec_adapter() {
    let project = requiring(json!({"sources": ["work-item"]}));
    commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &[],
    );

    let output = run(&project.root, &["audit", "--base", &project.base]);

    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(2),
            String::new(),
            format!(
                "{REQUIRED}: check sources \"work-item\" is no spec adapter; use file or openspec\n"
            )
        )
    );
}

/// Every path of the check is an install-root path: in an install below the git top level a path
/// outside the install is no path of the check, and a change folder of the install is found.
#[test]
fn a_subdirectory_install_matches_the_paths_of_its_own_root() {
    let project = project_in("app", vec![entry(REQUIRED, required(json!({})))], &[]);
    commit(
        project.dir.path(),
        &[
            ("app/src/a.rs", "fn a() {}\n"),
            ("other/src/b.rs", "fn b() {}\n"),
        ],
        "feat: add a and b",
        &[],
    );
    assert_row(
        &project,
        REQUIRED,
        "fail",
        "no intent of file, openspec for 1 changed path that match the check's if",
    );

    commit(
        project.dir.path(),
        &[("app/openspec/changes/add-x/proposal.md", PROPOSAL)],
        "docs: add the change",
        &[],
    );
    assert_row(
        &project,
        REQUIRED,
        "pass",
        "intent openspec:add-x for 1 changed path that match the check's if",
    );

    let elsewhere = project_in("app", vec![entry(REQUIRED, required(json!({})))], &[]);
    commit(
        elsewhere.dir.path(),
        &[("other/src/b.rs", "fn b() {}\n")],
        "feat: add b",
        &[],
    );
    assert_row(&elsewhere, REQUIRED, "skipped", NOT_APPLICABLE);
}

/// A range from git's empty tree, which `evidence` and `record write` read for a first push, holds
/// the root commit: it contributes every path that it adds, and `intent-resolves` reads the
/// head alone, since the range has no merge base.
#[test]
fn a_range_from_the_empty_tree_reads_the_root_commit() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    write_json(
        dir.path(),
        "knowledge/team.json",
        &json!({"topic": "team", "title": "Team rules", "entries": [
            entry(REQUIRED, required(json!({}))),
            entry(RESOLVES, resolves()),
        ]}),
    );
    commit(
        dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: root",
        &[],
    );
    let scratch = tempfile::tempdir().expect("tempdir");
    let json_file = scratch.path().join("evidence.json");

    let output = run(
        dir.path(),
        &[
            "evidence",
            "--base",
            EMPTY_TREE,
            "--head",
            "HEAD",
            "--json",
            json_file.to_str().expect("a UTF-8 path"),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&json_file).expect("read the evidence"))
            .expect("the evidence is JSON");
    let row = |list: &str, id: &str| {
        document[list]
            .as_array()
            .expect("a list")
            .iter()
            .find(|row| row["id"] == id)
            .map(|row| row["evidence"].as_str().expect("evidence").to_string())
    };
    assert_eq!(
        row("failed", REQUIRED),
        Some(
            "no intent of file, openspec for 1 changed path that match the check's if".to_string()
        )
    );
    assert_eq!(row("proven", RESOLVES), Some(NO_TRAILER.to_string()));
}

// ---- intent-resolves ------------------------------------------------------------------

/// Spec case 5: a range with no `Intent:` trailer passes.
#[test]
fn intent_resolves_passes_with_no_trailer() {
    let project = resolving();
    commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Work-Item: WI-001"],
    );

    assert_row(&project, RESOLVES, "pass", NO_TRAILER);
}

/// Spec case 5: a trailer that names a change that the tree does not hold fails, with the line of
/// the spec: the short sha, the trailer, and the adapter's line.
#[test]
fn intent_resolves_fails_for_a_trailer_naming_a_missing_change() {
    let project = resolving();
    let sha = commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Intent: openspec:ghost"],
    );

    assert_row(
        &project,
        RESOLVES,
        "fail",
        &format!("{} Intent: openspec:ghost: {NO_SUCH_CHANGE}", sha7(&sha)),
    );
}

/// Spec case 5: a trailer that names a change that the range archived passes: it resolves at the
/// head by the archived folder.
#[test]
fn intent_resolves_passes_for_a_change_archived_in_the_range() {
    let project = project(
        vec![entry(RESOLVES, resolves())],
        &[("openspec/changes/old-change/proposal.md", PROPOSAL)],
    );
    let dir = project.dir.path();
    fs::create_dir_all(dir.join("openspec/changes/archive")).expect("create the archive");
    git(
        dir,
        &[
            "mv",
            "openspec/changes/old-change",
            "openspec/changes/archive/2026-01-02-old-change",
        ],
    );
    commit(
        dir,
        &[],
        "docs: archive the change",
        &["Intent: openspec:old-change"],
    );

    assert_row(&project, RESOLVES, "pass", "1 Intent: trailer resolves");
}

/// 5.6: a trailer that names a change that the range removed resolves at the merge base.
#[test]
fn intent_resolves_passes_for_a_change_that_the_range_removed() {
    let project = project(
        vec![entry(RESOLVES, resolves())],
        &[
            ("openspec/changes/gone/proposal.md", PROPOSAL),
            ("openspec/changes/other/proposal.md", PROPOSAL),
        ],
    );
    let dir = project.dir.path();
    git(dir, &["rm", "-q", "-r", "openspec/changes/gone"]);
    commit(
        dir,
        &[],
        "docs: drop the change",
        &["Intent: openspec:gone", "Intent: openspec:other"],
    );

    assert_row(&project, RESOLVES, "pass", "2 Intent: trailers resolve");
}

/// Spec 5.5: each failing trailer is one line, oldest commit first and in message
/// order, joined by line feeds; a value that is no `<source>:<ref>` prints as written, and a
/// resolving trailer prints no line. The line of an unknown source holds the source.
#[test]
fn intent_resolves_prints_one_line_for_each_failing_trailer() {
    let project = resolving();
    let first = commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Intent: openspec:ghost"],
    );
    let second = commit(
        project.dir.path(),
        &[("src/b.rs", "fn b() {}\n")],
        "feat: add b",
        &["Intent: oops", "Intent: openspec:other", "Intent: bogus:x"],
    );

    let expected = [
        format!("{} Intent: openspec:ghost: {NO_SUCH_CHANGE}", sha7(&first)),
        format!("{} Intent: oops: {NOT_SOURCE_AND_REF}", sha7(&second)),
        format!(
            "{} Intent: bogus:x: \"bogus\" {UNKNOWN_SOURCE}",
            sha7(&second)
        ),
    ]
    .join("\n");
    assert_row(&project, RESOLVES, "fail", &expected);
}

/// A control character of a trailer value reaches no consumer of the evidence raw: the row writes
/// the escape, the delete character, and the C1 control `U+009B` (a control sequence to a
/// terminal) as `\u` escapes, so the text of the evidence holds none. The bytes that `audit` prints
/// hold no delete byte and no C1 byte: `serde_json` writes the controls below `U+0020` as escapes
/// by itself, so only the row keeps these two out of the output.
#[test]
fn intent_resolves_prints_a_control_character_of_a_trailer_as_an_escape() {
    let project = resolving();
    let sha = commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Intent: oops\u{1b}[31m\u{7f}\u{9b}"],
    );

    let output = audit(&project);

    assert!(!output.stdout.contains(&0x7f), "a raw delete byte");
    assert!(
        !output.stdout.windows(2).any(|pair| pair == [0xc2, 0x9b]),
        "a raw U+009B"
    );
    let evidence = row_of(&output, RESOLVES)["evidence"]
        .as_str()
        .expect("the evidence")
        .to_string();
    assert_eq!(
        evidence,
        format!(
            "{} Intent: \"oops\\u001b[31m\\u007f\\u009b\": {NOT_SOURCE_AND_REF}",
            sha7(&sha)
        )
    );
}

/// A value with an escape byte and a value that spells the six characters of its escape print as
/// two different lines: the byte as a JSON string, the text as it is.
#[test]
fn intent_resolves_tells_a_control_character_of_a_trailer_from_text_that_spells_its_escape() {
    let project = resolving();
    let byte = commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Intent: x\u{1b}y"],
    );
    let text = commit(
        project.dir.path(),
        &[("src/b.rs", "fn b() {}\n")],
        "feat: add b",
        &["Intent: x\\u001by"],
    );

    let output = audit(&project);

    let evidence = row_of(&output, RESOLVES)["evidence"]
        .as_str()
        .expect("the evidence")
        .to_string();
    assert_eq!(
        evidence,
        [
            format!(
                "{} Intent: \"x\\u001by\": {NOT_SOURCE_AND_REF}",
                sha7(&byte)
            ),
            format!("{} Intent: x\\u001by: {NOT_SOURCE_AND_REF}", sha7(&text)),
        ]
        .join("\n")
    );
}

/// A value that starts with a quote prints as a JSON string, as a value with a control character
/// does, so a printed value starts with a quote exactly when it is a JSON string. The value that
/// spells the JSON form of an escape byte (`"x\u001by"`) and the escape byte print as two
/// different lines.
#[test]
fn intent_resolves_tells_a_control_character_of_a_trailer_from_a_value_that_starts_with_a_quote() {
    let project = resolving();
    let byte = commit(
        project.dir.path(),
        &[("src/a.rs", "fn a() {}\n")],
        "feat: add a",
        &["Intent: x\u{1b}y"],
    );
    let quoted = commit(
        project.dir.path(),
        &[("src/b.rs", "fn b() {}\n")],
        "feat: add b",
        &[r#"Intent: "x\u001by""#],
    );

    let output = audit(&project);

    let evidence = row_of(&output, RESOLVES)["evidence"]
        .as_str()
        .expect("the evidence")
        .to_string();
    assert_eq!(
        evidence,
        [
            format!(
                r#"{} Intent: "x\u001by": {NOT_SOURCE_AND_REF}"#,
                sha7(&byte)
            ),
            format!(
                r#"{} Intent: "\"x\\u001by\"": {NOT_SOURCE_AND_REF}"#,
                sha7(&quoted)
            ),
        ]
        .join("\n")
    );
}

/// Determinism: `audit` prints the same bytes on two runs of the same range. Both checks report:
/// `intent-resolves` fails with two lines, and `intent-required` passes by the first of two
/// change folders that the range adds, so a later change that lets the order of the trailers, the
/// paths, or the found intents depend on iteration order shows as two different outputs.
#[test]
fn audit_prints_the_same_bytes_on_two_runs_of_a_range_where_both_checks_report() {
    let project = project(
        vec![
            entry(REQUIRED, required(json!({}))),
            entry(RESOLVES, resolves()),
        ],
        &[],
    );
    let sha = commit(
        project.dir.path(),
        &[
            ("openspec/changes/add-b/proposal.md", PROPOSAL),
            ("openspec/changes/add-a/proposal.md", PROPOSAL),
            ("src/b.rs", "fn b() {}\n"),
            ("src/a.rs", "fn a() {}\n"),
        ],
        "feat: add a and b",
        &["Intent: openspec:ghost", "Intent: bogus:x"],
    );

    let first = audit(&project);
    let second = audit(&project);

    assert_eq!(
        (first.status.code(), stdout(&first), stderr(&first)),
        (second.status.code(), stdout(&second), stderr(&second))
    );

    let required_row = row_of(&first, REQUIRED);
    assert_eq!(
        (
            required_row["result"].as_str(),
            required_row["evidence"].as_str()
        ),
        (
            Some("pass"),
            Some("intent openspec:add-a for 2 changed paths that match the check's if")
        ),
        "{required_row}"
    );
    let resolves_row = row_of(&first, RESOLVES);
    assert_eq!(
        (resolves_row["result"].as_str(), resolves_row["evidence"].as_str()),
        (
            Some("fail"),
            Some(
                format!(
                    "{short} Intent: openspec:ghost: {NO_SUCH_CHANGE}\n{short} Intent: bogus:x: \"bogus\" {UNKNOWN_SOURCE}",
                    short = sha7(&sha)
                )
                .as_str()
            )
        ),
        "{resolves_row}"
    );
}

// ---- check-knowledge ------------------------------------------------------------------

/// A scratch install with `entries` in `knowledge/team.json`.
fn install_with(entries: Vec<Value>) -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    write_json(
        dir.path(),
        "knowledge/team.json",
        &json!({"topic": "team", "title": "Team rules", "entries": entries}),
    );
    dir
}

/// What `check-knowledge` printed in `dir`: its exit code, standard output, and standard error,
/// whole.
fn check_knowledge(dir: &Path) -> (Option<i32>, String, String) {
    let output = run(dir, &["check-knowledge"]);
    (output.status.code(), stdout(&output), stderr(&output))
}

/// The standard error of a `check-knowledge` run that failed with exactly `lines`: each line ends
/// with a line feed, and nothing else is printed.
fn failure_with(lines: &[String]) -> (Option<i32>, String, String) {
    let text: String = lines.iter().map(|line| format!("{line}\n")).collect();
    (Some(1), String::new(), text)
}

/// Spec 5.5: the schema holds both types and the field `sources`, so an entry of each passes.
#[test]
fn check_knowledge_accepts_an_entry_of_each_intent_check() {
    let dir = install_with(vec![
        entry(
            REQUIRED,
            required(
                json!({"sources": ["file", "openspec"], "exempt_subjects": ["^chore"], "flags": "i"}),
            ),
        ),
        entry(RESOLVES, resolves()),
    ]);
    run_ok(dir.path(), &["render"]);

    let output = run(dir.path(), &["check-knowledge"]);

    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (Some(0), "knowledge: ok\n".to_string(), String::new())
    );
}

/// Spec 5.5: a `covers` on either type prints the line of a check that reads no file.
#[test]
fn a_covers_on_either_intent_check_prints_the_line_of_a_check_that_reads_no_file() {
    let dir = install_with(vec![
        entry(REQUIRED, required(json!({"covers": true}))),
        entry(
            RESOLVES,
            json!({"type": "intent-resolves", "level": "warn", "covers": false}),
        ),
    ]);

    assert_eq!(
        check_knowledge(dir.path()),
        failure_with(&[
            format!(
                "knowledge/team.json {REQUIRED}: check \"intent-required\" cannot declare \"covers\": it reads no file"
            ),
            format!(
                "knowledge/team.json {RESOLVES}: check \"intent-resolves\" cannot declare \"covers\": it reads no file"
            ),
        ])
    );
}

/// Spec 5.5: a missing `if` prints the line of a required field.
#[test]
fn an_intent_required_check_without_if_prints_the_line_of_a_required_field() {
    let dir = install_with(vec![entry(
        REQUIRED,
        json!({"type": "intent-required", "level": "fail"}),
    )]);

    assert_eq!(
        check_knowledge(dir.path()),
        failure_with(&[format!(
            "knowledge/team.json {REQUIRED}: check \"intent-required\" needs \"if\""
        )])
    );
}

/// A scratch install whose `intent-required` check lists the work item, a spec adapter, and a name
/// that is no adapter as its `sources`.
fn faulty_sources_install() -> TempDir {
    install_with(vec![entry(
        REQUIRED,
        required(json!({"sources": ["work-item", "file", "nope"]})),
    )])
}

/// The failure of `check-knowledge` for the install of [`faulty_sources_install`].
fn faulty_sources_failure() -> (Option<i32>, String, String) {
    failure_with(&[
        format!(
            "knowledge/team.json {REQUIRED}: check sources \"work-item\" is no spec adapter; use file or openspec"
        ),
        format!(
            "knowledge/team.json {REQUIRED}: check sources \"nope\" is no spec adapter; use file or openspec"
        ),
    ])
}

/// Spec 5.5: a source that is no spec adapter is one line, whether it is the work item or no adapter.
#[test]
fn each_source_that_is_no_spec_adapter_prints_one_line() {
    let dir = faulty_sources_install();

    assert_eq!(check_knowledge(dir.path()), faulty_sources_failure());
}

/// Determinism: `check-knowledge` prints the same bytes on two runs of the faulty-sources install,
/// whose two lines come from one list in the order of the entry.
#[test]
fn check_knowledge_prints_the_same_bytes_on_two_runs_of_the_faulty_sources_install() {
    let dir = faulty_sources_install();

    let first = check_knowledge(dir.path());
    let second = check_knowledge(dir.path());

    assert_eq!(first, second);
    assert_eq!(first, faulty_sources_failure());
}

/// Spec 5.5: a malformed `exempt_subjects` pattern prints the existing regex line.
#[test]
fn a_malformed_exempt_subjects_pattern_prints_the_regex_line() {
    let dir = install_with(vec![entry(
        REQUIRED,
        required(json!({"exempt_subjects": ["^chore", "(unclosed"]})),
    )]);

    assert_eq!(
        check_knowledge(dir.path()),
        failure_with(&[format!(
            "knowledge/team.json {REQUIRED}: check exempt_subjects is not a valid regex (Invalid regular expression: /(unclosed/: Unterminated group)"
        )])
    );
}

/// Pin of unchanged behavior: a check type that the schema does not list is still one line of the
/// schema, and the `commits` and `report-field` checks still refuse `covers`.
#[test]
fn an_unknown_type_and_the_covers_of_the_older_checks_keep_their_lines() {
    let dir = install_with(vec![
        entry(
            "team.unknown",
            json!({"type": "intent-everything", "level": "fail"}),
        ),
        entry(
            "team.trailer",
            json!({"type": "commits", "level": "warn", "require_trailer": "Work-Item", "covers": true}),
        ),
    ]);

    let (code, standard_output, standard_error) = check_knowledge(dir.path());

    assert_eq!(code, Some(1));
    assert_eq!(standard_output, "");
    let lines: Vec<&str> = standard_error.lines().collect();
    assert_eq!(lines.len(), 2, "{standard_error}");
    assert!(
        lines[0].starts_with(
            "knowledge/team.json.entries[0].check.type: must be one of \"grep-absent\", "
        ),
        "{}",
        lines[0]
    );
    assert_eq!(
        lines[1],
        "knowledge/team.json team.trailer: check \"commits\" cannot declare \"covers\": it reads no file"
    );
}

// ---- HR-244 ---------------------------------------------------------------------------

/// Case 13 (HR-244): the three doc comments that narrated a history now state the current
/// constraint only, so a `git grep -n -F` for the phrase that marked them prints nothing over
/// their two files. The two other matches of the crate state a current contract and stay.
#[test]
fn the_three_doc_comments_of_hr_244_hold_no_history() {
    let files = [
        "crates/houserules/src/rules/check.rs",
        "crates/houserules/src/bin/residue-gate.rs",
    ];
    let mut tracked = Command::new("git");
    tracked
        .args(["ls-files", "--error-unmatch", "--"])
        .args(files)
        .current_dir(repo_root());
    let listed: Output = with_default_git(&mut tracked)
        .output()
        .expect("run git ls-files");
    assert!(listed.status.success(), "the two files are tracked");

    let mut grep = Command::new("git");
    grep.args(["grep", "-n", "-F", "used to", "--"])
        .args(files)
        .current_dir(repo_root());
    let found = with_default_git(&mut grep).output().expect("run git grep");

    assert_eq!(
        (found.status.code(), stdout(&found)),
        (Some(1), String::new())
    );
}
