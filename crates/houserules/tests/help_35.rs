//! The help texts and the documents that state the runner, workspaces, and
//! audit behavior (spec 12): the
//! `--help` of `audit`, `check-knowledge`, `check-report-claims`, `init`, and
//! `update`, and the documents that state the same behavior (README, the
//! upgrade guide, the docs README of the template, and the migrating-knowledge
//! skill). `stats --help` is pinned in `stats_35.rs`.
//!
//! Every statement is compared on the text with each run of white space
//! folded to one space, because clap and the Markdown files wrap their lines.
//! A test lists every statement it needs and names all that are missing, so
//! one run shows the whole gap. Clap drops the period that ends the last
//! sentence of a command's description, so no pin ends on it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `text` with every run of white space read as one space.
fn folded(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text that `houserules <command> --help` prints, folded.
fn help_of(command: &str) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_houserules"))
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .args([command, "--help"])
        .output()
        .expect("run --help");
    assert_eq!(output.status.code(), Some(0), "{command} --help exits 0");
    folded(&String::from_utf8(output.stdout).expect("utf8 stdout"))
}

/// The file `relative` below the repository root, built component by component
/// (`houserules.path-pins-mirror-the-code`).
fn repo_file(relative: &str) -> PathBuf {
    relative.split('/').fold(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".."),
        |path, part| path.join(part),
    )
}

/// The text of the file `relative` below the repository root.
fn read(relative: &str) -> String {
    fs::read_to_string(repo_file(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"))
}

/// Fails with every statement of `statements` that `text` lacks.
fn states(surface: &str, text: &str, statements: &[&str]) {
    let missing: Vec<&str> = statements
        .iter()
        .copied()
        .filter(|statement| !text.contains(statement))
        .collect();
    assert!(
        missing.is_empty(),
        "{surface} lacks {} statement(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
}

/// Fails with every statement of `statements` that `text` holds.
fn omits(surface: &str, text: &str, statements: &[&str]) {
    let found: Vec<&str> = statements
        .iter()
        .copied()
        .filter(|statement| text.contains(statement))
        .collect();
    assert!(
        found.is_empty(),
        "{surface} holds {} stale statement(s):\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The sentence on the reference counts of the audit JSON (spec 8.5). The help
/// of `audit`, README, and the upgrade guide say the same.
const REFERENCE_COUNTS: &str = "`references.failures` lists each failure line, `references.checked` counts the references that the tracker judged, `references.passed` counts those among them that passed, `references.skipped` counts those that the audit did not judge (no tracker, a tracker that cannot be reached, or a tracker that answered with an error), and `summary.reference_fail` counts the failures";

/// The count that the audit JSON no longer holds. The text is built from two
/// parts, so that a search of the sources for it finds no stale statement.
const OLD_REFERENCE_COUNT: &str = concat!("references", ".resolved");

// ---- audit --help ---------------------------------------------------------------------

/// A `runner` check starts the tool that `runners` defines, when a changed file
/// triggers it, and a triggered check needs the tree at `--head` (spec 3.2).
#[test]
fn audit_help_names_the_runner_check_and_the_tree_it_needs() {
    states(
        "audit --help",
        &help_of("audit"),
        &[
            "A `runner` check starts the tool that the `runners` key of `.houserules.json` defines for the `tool` of the check, when a changed file matches the `files` of the check; an untriggered check starts no process",
            "a triggered `runner` check needs the working tree to be the tree at `--head`: `--head` must be the checked-out commit, and no file that the tool reads may differ from its content there",
        ],
    );
}

/// A setup error of a triggered `runner` check stops the audit with exit 2, one
/// line on stderr, and no rows (spec 3.3).
#[test]
fn audit_help_names_the_exit_2_of_a_setup_error() {
    states(
        "audit --help",
        &help_of("audit"),
        &[
            "stops the audit before it has any row, with one line on stderr and exit 2:",
            "a check with no `tool`",
            "a tool that `runners` does not define",
            "a malformed `runners` entry",
            "a program that is not installed (the line holds the `install` text of the entry) or cannot start",
            "a working tree that is not the tree at `--head`",
            "SARIF output that is missing, invalid, or reports a failed run",
        ],
    );
}

/// The reference counts say what they count (HR-221, spec 8.5).
#[test]
fn audit_help_counts_the_references_by_checked_passed_and_skipped() {
    let help = help_of("audit");
    states("audit --help", &help, &[REFERENCE_COUNTS]);
    omits("audit --help", &help, &[OLD_REFERENCE_COUNT]);
}

/// `--sanctioned` accepts a judged rule too. A failed deterministic row and a
/// judged row gain `sanctioned`; a deterministic rule that did not fail gains
/// none and is reported in `stale_sanctions` (spec 6).
#[test]
fn audit_help_names_what_a_sanction_does_to_each_kind_of_row() {
    let help = help_of("audit");
    states(
        "audit --help",
        &help,
        &[
            "A deterministic row that failed gains `sanctioned`, which holds `<ref>`. It keeps its true `fail` result and gains `(sanctioned: <ref>)` in its evidence; the summary's `sanctioned_fail` counts it",
            "A judged rule is accepted too: its row gains `sanctioned`, and the booking is never stale",
            "A deterministic rule that did not fail gains no `sanctioned`: it is reported in `stale_sanctions`, not silently accepted",
            "A rule outside the audit's package is an error: add it to `--ids`",
        ],
    );
    omits(
        "audit --help",
        &help,
        &["The row of the rule gains `sanctioned`"],
    );
}

/// A `--report` audit compares the `self_audit` of the report with its own run
/// (HR-138, spec 6).
#[test]
fn audit_help_names_the_self_audit_comparison() {
    states(
        "audit --help",
        &help_of("audit"),
        &[
            "When the `self_audit` of the report records the range of this run, the audit compares the recorded summary and deterministic rows with this run, and the check that reads `self_audit` reports `self_audit differs at <JSON path>` for a difference; for another range it reports `not compared`, and a `self_audit` of `null` passes",
        ],
    );
}

/// `--ids` is the second input of `covers_ids` (HR-175, spec 5).
#[test]
fn audit_help_names_the_ids_that_covers_ids_reads() {
    states(
        "audit --help",
        &help_of("audit"),
        &[
            "With `--report`, a `report-field` check with `covers_ids` also requires that its field lists each of these ids",
        ],
    );
}

// ---- check-knowledge --help -----------------------------------------------------------

/// The `runners` key, the `runner` checks, and the `workspaces` key (spec 3.3
/// and 7).
#[test]
fn check_knowledge_help_names_the_runner_and_marker_lines() {
    states(
        "check-knowledge --help",
        &help_of("check-knowledge"),
        &[
            "Checks the `runners` key of `.houserules.json`: it must be an object, and each tool in it needs a non-empty `command` array of strings, a `parser` of `sarif` or `exit-code`, a non-empty `install` string, and no other key; one line names the first fault of each tool",
            "Checks that each `runner` check names a tool that `runners` defines",
            "Checks that `workspaces`, when set, is a relative path inside the install root",
        ],
    );
}

/// The required fields of a check, for a pack copy and an override too (spec
/// 3.3).
#[test]
fn check_knowledge_help_names_the_required_fields_of_every_check() {
    states(
        "check-knowledge --help",
        &help_of("check-knowledge"),
        &[
            "Checks that the `check` of each entry holds the fields that its type requires: a local entry as written, and an active pack entry with its override applied",
        ],
    );
}

/// The ignore tests of git (HR-213, spec 7).
#[test]
fn check_knowledge_help_names_the_ignored_verify_path_and_the_default_location() {
    states(
        "check-knowledge --help",
        &help_of("check-knowledge"),
        &[
            "While `workspaces` is not set, it names the default location `.houserules/work` when that folder exists and git does not ignore it",
            "Names each `verify` path that git ignores, since a fresh clone lacks it: commit a copy under a tracked path and verify that",
            "Outside a git work tree it runs no ignore test",
        ],
    );
}

/// Pin of unchanged behavior: the pack and override checks stay in the text.
#[test]
fn check_knowledge_help_keeps_the_pack_checks() {
    states(
        "check-knowledge --help",
        &help_of("check-knowledge"),
        &[
            "Checks each pack copy against its pin in `.houserules.json`, and each override and exclusion in `knowledge/overrides.json` against the installed packs",
            "Prints each override and exclusion on stdout",
        ],
    );
}

// ---- check-report-claims --help -------------------------------------------------------

/// The step output, the workspaces location, and a symlink (HR-222, HR-224,
/// spec 8.6).
#[test]
fn check_report_claims_help_names_the_step_path_the_workspaces_and_the_symlink() {
    states(
        "check-report-claims --help",
        &help_of("check-report-claims"),
        &[
            "a workspace path is a cited path that git ignores, such as a path below the default `workspaces` location `.houserules/work/`",
            "It skips a cited path under `.houserules/step/`, which holds tool output",
            "It judges a cited path through a symlink inside the install root at the target of the link, so a missing target is a missing file",
        ],
    );
}

/// Pin of unchanged behavior: `--doc` and `--item` still name a cited workspace
/// path that does not exist.
#[test]
fn check_report_claims_help_keeps_the_doc_and_item_modes() {
    states(
        "check-report-claims --help",
        &help_of("check-report-claims"),
        &[
            "With `--doc` or `--item` in place of a report, it reads a spec or a backlog item and names every cited workspace path that does not exist",
        ],
    );
}

// ---- update --help --------------------------------------------------------------------

/// `update` in a directory with no install (HR-216, spec 8.2).
#[test]
fn update_help_names_the_refusal_of_a_directory_with_no_install() {
    states(
        "update --help",
        &help_of("update"),
        &[
            "A directory with no `.houserules.json` and no `knowledge/` directory, at or above it within the git top level, is no install: `update` prints `no houserules install here (no .houserules.json); run houserules init`, exits 2, and writes nothing",
            "When `--dir` names a directory other than the working directory, the line names it: `run houserules init --dir <dir>`, with `<dir>` as written",
        ],
    );
}

/// The ignore line of `.gitignore` (HR-213, spec 7).
#[test]
fn update_help_names_the_ignore_line() {
    states(
        "update --help",
        &help_of("update"),
        &[
            "It brings `.gitignore` to hold the line `.houserules/`: it replaces a `.houserules/step/` line in its place (`merged .gitignore (.houserules/step/ replaced by .houserules/)`), adds the line at the end where neither line is present (`merged .gitignore (.houserules/ added)`), writes the file when it is absent (`wrote .gitignore`), and prints no line for a `.gitignore` that holds `.houserules/` already",
            "A `.gitignore` that `ownedPaths` lists stays as found",
        ],
    );
}

/// The restamp of the install schema and the keys that the project owns (spec
/// 10).
#[test]
fn update_help_names_the_restamp_and_the_kept_keys() {
    states(
        "update --help",
        &help_of("update"),
        &[
            "It restamps the install schema `houserules/install@2.1.0` to `houserules/install@2.2.0` and prints `wrote .houserules.json (houserules/install@2.2.0)` once; a second run prints no such line",
            "The keys `runners` and `workspaces` of `.houserules.json` keep their values",
        ],
    );
}

/// The hand step for a seeded CI workflow that lacks the PR-head checkout
/// (spec 3.2).
#[test]
fn update_help_names_the_pr_head_hand_step() {
    states(
        "update --help",
        &help_of("update"),
        &[
            "lacks what the tracker needs or the PR-head checkout (`ref: ${{ github.event.pull_request.head.sha }}` under `with:` of its `actions/checkout` step)",
        ],
    );
}

/// Pin of unchanged behavior: `update` keeps a file the project changed.
#[test]
fn update_help_keeps_the_locally_modified_line() {
    states(
        "update --help",
        &help_of("update"),
        &["it keeps and lists a file the project changed (`kept <file> (locally modified)`)"],
    );
}

// ---- init --help ----------------------------------------------------------------------

/// The ignore line of `.gitignore` (HR-213, spec 7).
#[test]
fn init_help_names_the_ignore_line() {
    states(
        "init --help",
        &help_of("init"),
        &[
            "It writes the line `.houserules/` to `.gitignore`: the folder holds the output of the houserules step and, by default, the batch workspaces",
            "A `.houserules/step/` line becomes `.houserules/` in its place, a `.gitignore` that holds neither gets the line at its end, and one that holds `.houserules/` stays; a `.gitignore` that `ownedPaths` lists stays as found",
        ],
    );
}

/// The `backlogId` pattern that `init` seeds with no `--tracker` (HR-220, spec
/// 8.4).
#[test]
fn init_help_names_the_tracker_pattern_it_keeps() {
    states(
        "init --help",
        &help_of("init"),
        &[
            "Where `init` writes the deliverables schema and `--tracker` is absent, the `backlogId` pattern comes from the tracker that `.houserules.json` names, else it accepts any token",
        ],
    );
}

// ---- README ---------------------------------------------------------------------------

/// The CLI table, the runner section, the ignore line, and the reference
/// counts (spec 12).
#[test]
fn readme_states_the_runners_the_ignore_line_and_the_reference_counts() {
    let raw = read("README.md");
    let readme = folded(&raw);
    assert!(
        raw.lines().any(|line| line == "## Runners"),
        "README has no section `## Runners`"
    );
    states(
        "README.md",
        &readme,
        &[
            "- [Runners](#runners)",
            REFERENCE_COUNTS,
            "`.houserules/` holds the scratch output of the step and, by default, the batch workspaces, and `init` adds it to `.gitignore`",
            "A runner check runs an external tool over a change and fails the audit on what the tool reports. Your project defines the tool; the kit ships none",
            "An `exit-code` tool passes when every call exits 0 (`exited 0 in 1 call`). Otherwise the row fails with the evidence of the failing call: the first non-blank line of its stderr, trimmed; `exited <n>` when stderr holds no text; or `ended by a signal` when a signal ended the tool",
            "The tool reads the working tree, so a triggered runner check needs the tree at `--head` checked out",
            "`audit` stops with exit 2, one line on stderr, and no rows when a triggered check cannot run",
            "It installs no tool: add the install step of your tools to your own CI",
            "| `audit` | Builds a git range's rule package and runs every deterministic check, the tool of each `runner` check included,",
        ],
    );
    omits(
        "README.md",
        &readme,
        &[
            OLD_REFERENCE_COUNT,
            "`.houserules/step/` holds scratch output",
        ],
    );
}

/// "Measure process cost": the window, the new `rules` keys, the `sanctioned`
/// list, and `reversals` (spec 4, 6, and 9 (8)).
#[test]
fn readme_states_the_stats_window_and_the_new_keys() {
    states(
        "README.md",
        &folded(&read("README.md")),
        &[
            "houserules stats --window 5 .houserules/work/*/",
            "`--window <n>` reads only the `n` most recent workspaces",
            "`sanctioned` counts the fails that a sanction accepts, which `violations` and `fails` leave out",
            "`verdicts` counts the judged rows with a `pass` or `fail` result, `overrides` those that a person overruled, and `override_rate` is `overrides` over `verdicts` (`null` at zero verdicts)",
            "`cost` is the sum, over the reviews that judged the rule, of each review's share of its bytes (`null` for a rule with no judged rows)",
            "`sanctioned`: one row for each fail that a sanction accepts, with the rule `id`, the `workspace`, the `task`, and the `ref` of the sanction",
            "`reversals` counts the `Reversal:` lines of the `progress.md` of the workspace: a line counts when it starts with `Reversal:`, after optional spaces and an optional list marker with its space (`- `, `* `, or `+ `)",
            "| `stats <workspace>...` | Aggregates rule violations, sanctioned fails, process cost, and knowledge-base proposals across one or more workspaces; `--window <n>` reads the `n` most recent |",
        ],
    );
}

// ---- the upgrade guide ----------------------------------------------------------------

/// The ignore line and the reference counts (spec 8.5 and 12).
#[test]
fn the_upgrade_guide_states_the_ignore_line_and_the_reference_counts() {
    let guide = folded(&read("docs/upgrading-to-2.md"));
    states(
        "docs/upgrading-to-2.md",
        &guide,
        &[
            REFERENCE_COUNTS,
            "`init` adds `.houserules/` to `.gitignore`, so the output of the script and the batch workspaces (`.houserules/work/` by default) stay out of git",
            "- the line `.houserules/` in `.gitignore`",
        ],
    );
    omits(
        "docs/upgrading-to-2.md",
        &guide,
        &[
            OLD_REFERENCE_COUNT,
            "`init` adds `.houserules/step/` to `.gitignore`",
            "- the line `.houserules/step/` in `.gitignore`",
        ],
    );
}

/// The batch-35 section: the move of tracked workspaces, whose `.gitignore`
/// line comes before `git rm`, the schemas of your own, and the reds by design
/// (spec 7 and 10, case 10).
#[test]
fn the_upgrade_guide_extends_its_section_of_install_schema_2_2_0() {
    let raw = read("docs/upgrading-to-2.md");
    for heading in [
        "### Hand step: workspaces that git tracks",
        "### The schemas of your own",
        "### The reds of 2.2.0 by design",
    ] {
        assert!(
            raw.lines().any(|line| line == heading),
            "the guide has no heading {heading:?}"
        );
    }
    states(
        "docs/upgrading-to-2.md",
        &folded(&raw),
        &[
            "Set `workspaces` in `.houserules.json` to the folder, or move the folder under `.houserules/work/`. For a folder outside `.houserules/`, add its line to `.gitignore` first, so that `check-knowledge` finds the folder ignored. Run `git rm -r --cached <folder>` and commit. For each `verify` path that `check-knowledge` then names, commit a copy under a tracked path (for example `tests/fixtures/`) and point `verify` at it",
            "add `\"runner\"` to the `type` enum of `$defs.check`, and add the properties `tool` (a string of at least one character), `non_empty` (a boolean), and `covers_ids` (a boolean)",
            "`$defs.auditRow` gains the properties `sanctioned`, `findings`, and `override`, and the schema gains the definitions `auditFinding` and `rowOverride`",
            "`security-hygiene.exact-pins` fails a change to a `Cargo.toml` that holds a dependency version that is not pinned with `=`: a bare version, which Cargo reads as a caret range, or a requirement that starts with `^`, `~`, `<`, or `>`, or that is `*`",
            "pin every dependency of that file exactly",
            "`security-hygiene.tool-pins-are-exact` and `security-hygiene.tool-versions-pins-are-exact` fail a change to a mise file or a `.tool-versions` file that holds a pin that is not a full version",
            "the `self_audit` of the next task report whose copy is stale fails `process.deliverables-json`: run the audit again and copy `self_audit` again",
        ],
    );
}

/// The section of install schema 2.2.0 names what `update` does to each state
/// of `.gitignore` that the help of `update` names (spec 7).
#[test]
fn the_upgrade_guide_names_what_update_does_to_each_state_of_gitignore() {
    states(
        "docs/upgrading-to-2.md",
        &folded(&read("docs/upgrading-to-2.md")),
        &[
            "It also brings `.gitignore` to hold the line `.houserules/`, which keeps the batch workspaces out of git too",
            "A `.houserules/step/` line becomes `.houserules/` in its place, and `update` prints `merged .gitignore (.houserules/step/ replaced by .houserules/)`",
            "A file that holds neither line gets `.houserules/` at its end, and `update` prints `merged .gitignore (.houserules/ added)`",
            "A file that is absent is written with the line, and `update` prints `wrote .gitignore`",
            "A file that holds `.houserules/` already stays, and `update` prints no line",
            "A file that `ownedPaths` lists stays as found, and `update` prints no line: replace its `.houserules/step/` line with `.houserules/` by hand, or add the line where neither is present",
        ],
    );
}

// ---- the docs README and the migrating-knowledge skill --------------------------------

/// The docs README of the template names the window of `stats`.
#[test]
fn the_docs_readme_of_the_template_names_the_stats_window() {
    states(
        "template/docs/README.md",
        &folded(&read("template/docs/README.md")),
        &[
            "`houserules stats --window 5 <WORKSPACE>...` over the folders of deliverables reads the five most recent",
        ],
    );
}

/// The runner section of the migrating-knowledge skill and its CI-gate section
/// (spec 3.3 and 12).
#[test]
fn the_migrating_knowledge_skill_teaches_runners() {
    let raw = read("template/.claude/skills/migrating-knowledge/SKILL.md");
    assert!(
        raw.lines()
            .any(|line| line == "## Run an external tool with a runner check"),
        "the skill has no runner section"
    );
    states(
        "migrating-knowledge skill",
        &folded(&raw),
        &[
            "Use rule files in the repository (`--config <file>`), never a registry config: Semgrep sends usage metrics when it pulls rules from the registry",
            "Switch the metrics and every other call home off in the command (`--metrics off` and `--disable-version-check` for Semgrep",
            "Pin an exact version of the tool in the `install` line and in your CI",
            "Check out the head of the pull or merge request, not a merge commit, before the audit when the project uses `runner` checks",
        ],
    );
}
