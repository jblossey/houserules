//! The help texts and the documents that state the evidence behavior of batch
//! 36 (spec 7, 8.2, 8.3, and 11; case 8, and the part of case 10 that rests on
//! the texts): the `--help` of `update`, `validate`, and `check-knowledge`, the
//! README with its reference pages, and the upgrade guide. `capture --help`,
//! `report-facts --help`, and `check-report-claims --help` are pinned with
//! their commands, and the hand steps of the CI workflow in the unit tests of
//! `main.rs`.
//!
//! The file also holds the reds of an upgrade that the texts tell the way out
//! of (spec 12): an install whose `process` pack copy is one patch version
//! behind, and an install whose implementer template `update` keeps. The
//! `sh` blocks of the evidence section of `docs/evidence.md` run in a fresh
//! install, and the
//! texts that state what `update` prints for a kept schema follow the code.
//!
//! A test whose doc comment calls it a pin asserts behavior that the code held
//! before this change. Every statement is compared on folded text, as in
//! `help_35.rs`.

mod docs_support;
mod help_support;
mod tracker_support;

use std::fs;

#[cfg(unix)]
use help_support::{fenced_blocks_of, run_block};
use help_support::{folded, help_of, help_of_args, omits, read, states};
use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{edit_marker, git, read_json, repo_root, run_ok, scratch_git_repo, stdout};
use tracker_support::{pack_pin, write_json};

/// The commit on main that holds the kit of batch 35: the last kit before the
/// report form of batch 36.
const BATCH_35: &str = "ce0fc0f";

// ---- update --help --------------------------------------------------------------------

/// Spec 8.3: the no-install line quotes `<dir>` for a POSIX shell where it
/// needs quoting. `houserules --help` lists the same text for `update`.
#[test]
fn update_help_says_that_the_dir_of_the_no_install_line_is_quoted() {
    let sentence = "When `--dir` names a directory other than the working directory, the line names it: `run houserules init --dir <dir>`, with `<dir>` quoted for a POSIX shell where it needs quoting";
    states("update --help", &help_of("update"), &[sentence]);
    states("houserules --help", &help_of_args(&["--help"]), &[sentence]);
    omits(
        "houserules --help",
        &help_of_args(&["--help"]),
        &["with `<dir>` as written"],
    );
}

/// The hand steps for the read-only token belong to every kept seeded CI
/// workflow, not to a workflow of one kind: `hand_step_for_kept` tests the two
/// markers of each kept copy. Each scope clause of the sentence attaches to one
/// marker: `contents: read` is asked for every tracker, the Beads tracker
/// included, and only `persist-credentials: false` has the exception for the
/// Beads tracker. `houserules --help` lists the same text.
#[test]
fn update_help_says_that_every_kept_workflow_gets_the_token_hand_steps_it_lacks() {
    let sentence = "It also prints a hand step for each marker of the read-only token that a kept seeded CI workflow lacks: `contents: read` in the permissions of the gates job, for every install, and `persist-credentials: false` under `with:` of the `actions/checkout` step, for every install but one with the Beads tracker";
    states("update --help", &help_of("update"), &[sentence]);
    states("houserules --help", &help_of_args(&["--help"]), &[sentence]);
    let old_tails = [
        "that such a workflow lacks",
        "whose hand step for the lines of the tracker names the permissions",
        "for every install but one with the GitHub tracker",
        "of the gates job, and `persist-credentials: false`",
    ];
    omits("update --help", &help_of("update"), &old_tails);
    omits("houserules --help", &help_of_args(&["--help"]), &old_tails);
}

/// A kept seeded CI workflow of the GitHub tracker gets one hand step for its
/// permissions and one for its env, each only when it lacks a marker of it
/// (spec 8.5, design.md 5.223). `houserules --help` lists the same text.
#[test]
fn update_help_says_that_a_kept_github_workflow_gets_a_step_for_each_marker_it_lacks() {
    let sentence = "A kept seeded CI workflow of the GitHub tracker gets one hand step for its permissions and one for its env, each only when the workflow lacks a marker of it (`contents: read` and `issues: read` for the permissions, `GH_TOKEN: ${{ github.token }}` for the env), so that no workflow is asked for two `permissions` keys";
    states("update --help", &help_of("update"), &[sentence]);
    states("houserules --help", &help_of_args(&["--help"]), &[sentence]);
}

/// The `permissions` key of the gates job decides the form of the hand step for
/// the permissions of a kept seeded CI workflow (spec 8.5, design.md 5.226), so
/// the closing clause of the sentence before it, that no workflow is asked for
/// two `permissions` keys, holds. `houserules --help` lists the same text.
#[test]
fn update_help_says_that_the_permissions_key_of_the_gates_job_decides_the_hand_step() {
    let sentence = "The hand step for the permissions follows the `permissions` key of the gates job: a workflow with no key is asked for the block, a workflow with a block that lacks a scope is asked for that scope in the block, a workflow with `permissions: read-all` gets no step for its permissions, and a workflow with `permissions: write-all` is asked to replace the scalar with the block";
    states("update --help", &help_of("update"), &[sentence]);
    states("houserules --help", &help_of_args(&["--help"]), &[sentence]);
}

/// A 2.x install keeps a schema that the project changed and lists it with the
/// `kept` line. A schema with no baseline that equals no content of the kit
/// gets the hand step on every run. `houserules --help` lists the same text.
#[test]
fn update_help_says_what_it_prints_for_a_kept_schema() {
    let sentence = "a schema that the project changed stays, and `update` lists it (`kept <file> (locally modified)`); a schema with no baseline that differs from every content of it that the kit shipped, as in an install converted from 1.x, stays with a hand step on every run";
    states("update --help", &help_of("update"), &[sentence]);
    states("houserules --help", &help_of_args(&["--help"]), &[sentence]);
    omits(
        "update --help",
        &help_of("update"),
        &["shipped stays, with a hand step"],
    );
}

// ---- validate --help ------------------------------------------------------------------

/// Spec 4 and 11: `validate` reads the captures of a finished report's fix
/// rounds and fails an audit that skipped checks.
#[test]
fn validate_help_names_the_audit_checks_of_a_finished_task_report() {
    states(
        "validate --help",
        &help_of("validate"),
        &[
            "For a task report with the status `DONE` or `DONE_WITH_CONCERNS`, it also fails a `self_audit` of `null` and a `self_audit` whose `summary.skipped` is above 0",
            "For the same reports, it reads the capture of each run in `fix_rounds[].tests[]`, resolved against the install root, and fails a run whose output holds an audit result with a `summary.skipped` above 0: `<file>.fix_rounds[<i>].tests[<j>].capture: audit summary skipped is <n>; re-run audit with --report`",
            "It leaves a capture that it cannot read to `check-report-claims`",
        ],
    );
}

/// A pin: `validate` still names the deliverables schema and the compression
/// marker.
#[test]
fn validate_help_keeps_the_schema_and_the_marker_gate() {
    states(
        "validate --help",
        &help_of("validate"),
        &[
            "Validates one or more deliverable JSON files against `.claude/schemas/deliverables.json`, and rejects a compression marker that a context proxy left in a file",
        ],
    );
}

// ---- check-knowledge --help -----------------------------------------------------------

/// Spec 8.2: the regular expressions of a pack copy and of an override are
/// tested as those of a local entry are.
#[test]
fn check_knowledge_help_names_the_regex_test_of_pack_copies_and_overrides() {
    states(
        "check-knowledge --help",
        &help_of("check-knowledge"),
        &[
            "Checks that the `pattern`, `subject`, `body_absent`, and each string of `exempt_subjects` of a check compile as regular expressions, for the same entries",
            "An expression that does not compile gives the line `<file> <id>: check <field> is not a valid regex (<message>)`, where `<file>` is the topic file of a local entry, or the pack copy of a pack entry and of the entry that an override changes",
        ],
    );
}

// ---- README and its reference pages ---------------------------------------------------

/// The page of the evidence section, which the README links.
const EVIDENCE_PAGE: &str = "docs/evidence.md";

/// The README and each page that holds reference text that left it, folded
/// and joined: the text in which a stale statement must not come back.
fn readme_and_its_pages() -> String {
    docs_support::REFERENCE_PAGES
        .iter()
        .map(|path| folded(&read(path)))
        .fold(folded(&read("README.md")), |all, text| {
            format!("{all} {text}")
        })
}

/// The evidence section of its page: value, a runnable command with its
/// output, the report fragment, and what each command checks. The README links
/// the page.
#[test]
fn the_evidence_page_has_an_evidence_section_with_its_runnable_usage() {
    let raw = read(EVIDENCE_PAGE);
    assert!(
        raw.lines().any(|line| line == "## Evidence"),
        "{EVIDENCE_PAGE} has no section `## Evidence`"
    );
    states(
        "README.md",
        &folded(&read("README.md")),
        &["[The houserules step and evidence](docs/evidence.md)"],
    );
    states(
        EVIDENCE_PAGE,
        &folded(&raw),
        &[
            "A task report cites its evidence instead of holding it.",
            "The commands below start from a fresh install: a repository where `houserules init` ran and one commit holds its files.",
            "houserules capture --out .houserules/work/demo/files.txt -- ls src",
            ".houserules/work/demo/files.txt: exit 0",
            "`capture` runs the command with no shell, in the current directory, with stdin closed. It exits with the exit code of the command, so a failing test run still fails a gate chain. The directory of `--out` must exist.",
            "houserules report-facts report.json --base HEAD~1",
            "report.json: facts filled",
            "report.json: no claim mismatches found",
            "`report-facts` adds the `sha256`, the `command`, and the `exit` of each run from its capture,",
            "It never replaces a recorded hash: a capture or a cited line that differs from its hash is a finding.",
            "An `excerpt` is optional, holds at most 1000 UTF-16 code units, and must be a byte-exact part of the output of its capture.",
            "`check-report-claims` fails a report when a capture is missing or changed, when a `command` or an `exit` differs from its header, when an `excerpt` is not part of the output of its capture, and when a cited line differs from `HEAD`.",
            "re-read the capture, then remove sha256 and run report-facts",
        ],
    );
    omits(
        "README.md and its reference pages",
        &readme_and_its_pages(),
        &["when an `excerpt` is not part of its capture"],
    );
}

/// `output` with its first SHA-256 digest named `<hash of the file>` and its
/// second `<hash in the report>`, the words that the evidence page gives the
/// two digests of a changed capture: the digests change with each capture,
/// because the header holds the times.
#[cfg(unix)]
fn naming_the_digests(output: &str) -> String {
    let mut names = ["<hash of the file>", "<hash in the report>"].into_iter();
    output
        .split(' ')
        .map(|word| {
            let digest = word.trim_end_matches([',', ';']);
            if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                let name = names.next().expect("the output holds two digests at most");
                format!("{name}{}", &word[digest.len()..])
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every `sh` block of the evidence section of its page runs as printed, top
/// to bottom, in a fresh install (`init`, then one commit), and prints the
/// `text` block that follows it. The first two blocks exit 0. The last block
/// shows a finding of `check-report-claims`, which exits 1.
#[cfg(unix)]
#[test]
fn the_evidence_page_blocks_run_as_printed_in_a_fresh_install() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    tracker_support::commit_all(dir.path(), "chore: install houserules knowledge setup");
    let blocks = fenced_blocks_of(&read(EVIDENCE_PAGE), "Evidence");

    let mut exits = Vec::new();
    for (at, (info, script)) in blocks.iter().enumerate() {
        if info != "sh" {
            continue;
        }
        let (code, printed) = run_block(dir.path(), script);
        exits.push(code);
        let shown = match blocks.get(at + 1) {
            Some((next, text)) if next == "text" => text.as_str(),
            _ => "",
        };
        assert_eq!(
            naming_the_digests(printed.trim_end()),
            shown,
            "the block does not print the text that follows it:\n{script}"
        );
    }

    assert_eq!(exits, [0, 0, 1], "the exit codes of the three sh blocks");
}

/// The step section of the evidence page names the facts and the output files
/// of the report step (`tools/houserules-step.sh after <base> <report>`).
#[test]
fn the_evidence_page_states_what_the_step_script_does_with_a_report() {
    let page = folded(&read(EVIDENCE_PAGE));
    states(
        EVIDENCE_PAGE,
        &page,
        &[
            "`after <base>` runs `houserules audit --base <base>` and writes the result to `.houserules/step/audit.json`. With a report as the second argument, the audit runs with `--report`, and the script then runs `houserules report-facts` and `houserules validate` and writes `.houserules/step/report-facts.txt` and `.houserules/step/validate.json`.",
            "# the audit since main, the report's facts, and the report check",
        ],
    );
    omits(
        "README.md and its reference pages",
        &readme_and_its_pages(),
        &[
            "it also runs `houserules validate` and writes `.houserules/step/validate.json`",
            "# the audit since main, and the report check",
        ],
    );
}

/// The CLI reference lists `capture` and `report-facts`, and states what
/// `validate` and `check-report-claims` check now.
#[test]
fn the_cli_reference_lists_the_evidence_commands_and_the_changed_rows() {
    let reference = folded(&read("docs/cli.md"));
    states(
        "docs/cli.md",
        &reference,
        &[
            "Working with evidence:",
            "| `capture --out <file> -- <command>...` | Runs a command and writes its output to the file under a header (the command, the directory, the times, the exit status, `HEAD`, and a dirty flag); it exits with the exit code of the command |",
            "| `report-facts <report> --base <ref>` | Fills the fact fields of a task report from git and from captures: the commits, the changed files, the `sha256`, `command`, and `exit` of each run, the `sha256` of each recorded citation, and, with `--audit <file>`, the self-audit |",
            "| `validate` | Validates deliverable JSON files against the schema, which requires a `model` field; a finished task report fails when its self-audit, or the audit capture of a fix round, skipped checks |",
            "| `check-report-claims` | Cross-checks a report's claims against the captures, files, and commits it cites; with `--doc <path>` or `--item <id>` it checks a spec or a backlog item for cited workspace paths that do not exist, and skips a cited path under `.houserules/step/` |",
        ],
    );
    omits(
        "README.md and its reference pages",
        &readme_and_its_pages(),
        &[
            "| `validate` | Validates deliverable JSON files against the schema, which requires a `model` field |",
            "Cross-checks a report's claims against the artifacts it cites",
        ],
    );
}

// ---- the upgrade guide ----------------------------------------------------------------

/// The report form: what changed, the closed workspaces, the two hand steps,
/// and the reds by design with their remedies (spec 11 and 12).
#[test]
fn the_upgrade_guide_teaches_the_report_form_its_hand_steps_and_its_reds() {
    let raw = read("docs/upgrading-to-2.md");
    for heading in [
        "## The report form",
        "### What changed in the report",
        "### Closed workspaces need no conversion",
        "### Hand step: a deliverables schema of your own",
        "### Hand step: an agent template that you changed",
        "### The reds of the report form by design",
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
            "A task report cites its evidence by capture.",
            "`output` left the schema: a report holds no command output.",
            "`stats` and `audit --workspace` read the deliverables of a workspace as JSON without the schema, and print the same result before and after the upgrade. Leave the closed workspaces as they are.",
            "It keeps a copy that you changed. For a copy that has a baseline, `update` prints `kept .claude/schemas/deliverables.json (locally modified)` and no hand step; for a copy that has none, it prints the [hand step of a schema of your own](#hand-step-a-schema-of-your-own). A kept copy still requires `output` in a run, so every report in the new form fails `validate`. Merge these changes into the kept copy:",
            "Add the definition `sha256Hex`: a string that matches `^[0-9a-f]{64}$`.",
            "In `$defs.fixRound`, add the property `reran` (`text`) to the items of `findings`.",
            "In `$defs.taskReport`, add the property `citations` (an array of `citation`), and make `self_review` an array of `selfReviewItem`.",
            "`update` keeps a template or a skill of the reference workflow that you changed, and prints `kept <file> (locally modified; reference workflow <version>)` on every run until you merge the change",
            "**Every report in the new form fails `validate` against a deliverables schema that `update` kept.**",
            "**A report from an implementer template that `update` kept is in the old form,** which the new schema rejects.",
            "**A task report in flight across the upgrade fails `validate`,** and `check-report-claims` fails each of its runs with `<label>: no capture; run the command through houserules capture`, also on an install that kept the old schema. Rewrite the report in the new form.",
            "**`audit` of the update commit fails `process.evals-rerun`** on an install of the reference workflow",
            "**`check-knowledge` fails a pack copy or an override whose pattern does not compile.**",
        ],
    );
}

/// The guide says what `update` prints for a kept schema, as the code does: a
/// copy with a baseline gets the `kept` line, a copy with none gets the hand
/// step (`hand_steps::schema_kept`, printed by the schema step of the
/// conversion for a schema with no baseline).
#[test]
fn the_upgrade_guide_says_what_update_prints_for_a_kept_schema() {
    let guide = folded(&read("docs/upgrading-to-2.md"));
    states(
        "docs/upgrading-to-2.md",
        &guide,
        &[
            "and keeps a copy that you changed. A copy that has a baseline gets the line `kept <path> (locally modified)`; a copy that has none, and that differs from every content that the kit shipped, gets the [hand step of a schema of your own](#hand-step-a-schema-of-your-own).",
            "It prints this step for a schema that has no baseline, as in an install converted from 1.x. A schema that has a baseline and differs from it gets `kept <path> (locally modified)` and no hand step.",
            "It keeps a copy that you changed, and prints `kept <path> (locally modified)` for it; a deliverables schema with no baseline gets the [hand step of a schema of your own](#hand-step-a-schema-of-your-own) instead.",
        ],
    );
    omits(
        "docs/upgrading-to-2.md",
        &guide,
        &[
            "and keeps a copy that you changed, with a [hand step](#hand-step-a-schema-of-your-own).",
            "It keeps a copy that you changed, and prints the [hand step of a schema of your own](#hand-step-a-schema-of-your-own). A kept copy lacks",
            "prints only `kept .claude/schemas/deliverables.json (locally modified)`",
        ],
    );
}

// ---- the reds of an upgrade -----------------------------------------------------------

/// The process pack of batch 35, parsed.
fn process_pack_of_batch_35() -> Value {
    let text = git(
        &repo_root(),
        &["show", &format!("{BATCH_35}:template/packs/process.json")],
    );
    serde_json::from_str(&text).expect("parse the process pack of batch 35")
}

/// An install whose `process` pack is the copy of batch 35, at its pin, as
/// `update` of a batch-35 install finds it.
fn install_with_the_process_pack_of_batch_35() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init", "--tracker", "json"]);
    run_ok(dir.path(), &["pack", "add", "process"]);
    let old = process_pack_of_batch_35();
    write_json(dir.path(), "knowledge/packs/process.json", &old);
    edit_marker(dir.path(), |marker| {
        let pin = marker
            .get_mut("packs")
            .and_then(|packs| packs.get_mut("process"))
            .expect("the process pin");
        pin["version"] = old["version"].clone();
        pin["sha256"] = json!(pack_pin(&old));
    });
    dir
}

/// Case 10: `update` of a batch-35 install moves the copy of the `process` pack
/// to the version of the kit, which carries the reworded bodies, and a second
/// `update` moves nothing.
#[test]
fn update_moves_the_process_pack_of_batch_35_to_the_version_of_the_kit() {
    let dir = install_with_the_process_pack_of_batch_35();
    let old = process_pack_of_batch_35();
    assert_eq!(old["version"], json!("2.0.0"));

    let output = run_ok(dir.path(), &["update"]);

    let kit: Value = serde_json::from_str(&read("template/packs/process.json"))
        .expect("parse the kit process pack");
    let version = kit["version"].as_str().expect("the kit pack has a version");
    let lines: Vec<String> = stdout(&output).lines().map(str::to_string).collect();
    assert!(
        lines.contains(&format!("moved pack process from 2.0.0 to {version}")),
        "{lines:?}"
    );
    let moved = read_json(dir.path(), "knowledge/packs/process.json");
    assert_eq!(moved, kit);
    assert_eq!(moved["version"], json!(version));
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["packs"]["process"]["version"],
        json!(version)
    );
    assert!(
        !moved
            .to_string()
            .contains("Truncation markers count as citations"),
        "the moved copy still teaches the truncation marker"
    );

    tracker_support::commit_all(dir.path(), "chore: update");
    let second = stdout(&run_ok(dir.path(), &["update"]));
    assert!(!second.contains("moved pack"), "{second}");
}

/// The file whose template the reds of the report form name.
const IMPLEMENTER: &str = ".claude/agents/implementer.md";

/// A reference-workflow install whose implementer template is the one of batch
/// 35 with a project addition, so that `update` keeps it as modified.
fn install_with_a_kept_implementer() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(
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
    let old = git(
        &repo_root(),
        &[
            "show",
            &format!("{BATCH_35}:template/workflows/reference/{IMPLEMENTER}"),
        ],
    );
    fs::write(
        dir.path().join(IMPLEMENTER),
        format!("{old}\nProject addition.\n"),
    )
    .expect("write the implementer template");
    dir
}

/// A pin, and a red of spec 12: `update` keeps an implementer template that the
/// project changed and names it on every run. The kept file still teaches the
/// old form of a report, which the schema of this kit rejects.
#[test]
fn update_keeps_an_implementer_template_of_the_old_form_and_prints_its_kept_line() {
    let dir = install_with_a_kept_implementer();
    let before = fs::read(dir.path().join(IMPLEMENTER)).expect("read the template");
    let kept = format!(
        "kept {IMPLEMENTER} (locally modified; reference workflow {})",
        env!("CARGO_PKG_VERSION")
    );

    for _ in 0..2 {
        let output = run_ok(dir.path(), &["update"]);
        let lines: Vec<String> = stdout(&output).lines().map(str::to_string).collect();
        assert!(lines.contains(&kept), "{lines:?}");
    }

    assert_eq!(
        fs::read(dir.path().join(IMPLEMENTER)).expect("read the template"),
        before
    );
    let text = folded(&String::from_utf8(before).expect("UTF-8"));
    assert!(
        text.contains("`output` carries the command's stdout and stderr verbatim"),
        "the kept template does not teach the old form"
    );
}

/// A red of spec 12: the guide gives the remedy of a kept implementer template,
/// and the way to compare it with the template of this kit.
#[test]
fn the_upgrade_guide_gives_the_remedy_of_a_kept_implementer_template() {
    states(
        "docs/upgrading-to-2.md",
        &folded(&read("docs/upgrading-to-2.md")),
        &[
            "**A report from an implementer template that `update` kept is in the old form,** which the new schema rejects. Merge the changes of [the hand step for an agent template that you changed](#hand-step-an-agent-template-that-you-changed).",
            "houserules init --workflow reference --tracker json --add-required-packs --dir ../kit-reference",
            "diff .claude/agents/implementer.md ../kit-reference/.claude/agents/implementer.md",
        ],
    );
}
