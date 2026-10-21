//! The texts of the reference workflow (batch 34, spec sections 4.1, 5, 6, 9.1,
//! and 9.3; live cases 20 to 22): the agent templates, the lifecycle skills,
//! the eval scenarios, and the session-start script of
//! `template/workflows/reference/`. The package is a source tree, so every
//! case reads it there and installs nothing.
//!
//! The texts name no plugin and none of its skills, no full tier, and a
//! vendor model only beside its default (spec 5 and 6.4). The lists of those
//! guards are in `guard_lists`, which `pack_texts.rs` shares. They teach the
//! `Work-Item:` trailer per `Backlog:` id, the two tiers and the critical
//! task, the design review and its loop, the technical-debt candidates, the
//! reading of `houserules` output through files, and the riders of spec 9.1.
//! A phrase is compared on the text with every run of white space collapsed
//! to one space, because the finishing skill wraps its lines.
//!
//! The guard of the plugin (live case 20) walks `template/` only. It leaves
//! out `crates/houserules/released/`, which the binary embeds as well: the
//! files there keep the bytes of earlier releases, which the provenance rule
//! of spec 4.7 compares and never writes (C42). Five copies of the
//! orchestrating skill and `entries.json` there name the plugin.

mod guard_lists;

use std::fs;
use std::path::{Path, PathBuf};

use guard_lists::{FULL_TIER, PLUGIN_SKILLS, VENDOR_MODELS};
use regress::Regex;
use serde_json::Value;

const IMPLEMENTER: &str = ".claude/agents/implementer.md";
const TASK_REVIEWER: &str = ".claude/agents/task-reviewer.md";
const BRANCH_REVIEWER: &str = ".claude/agents/branch-reviewer.md";
const SPEC_REVIEWER: &str = ".claude/agents/spec-reviewer.md";
const ORCHESTRATING: &str = ".claude/skills/orchestrating/SKILL.md";
const FINISHING: &str = ".claude/skills/finishing-a-feature/SKILL.md";
const DESIGNING: &str = ".claude/skills/designing/SKILL.md";
const PLANNING: &str = ".claude/skills/planning/SKILL.md";
const SESSION_START: &str = "tools/reference-session-start.sh";

/// The four agent templates of the workflow.
const AGENT_TEMPLATES: [&str; 4] = [IMPLEMENTER, TASK_REVIEWER, BRANCH_REVIEWER, SPEC_REVIEWER];

/// The agent templates and the skills: every text that tells an agent what to
/// do with `houserules` output.
const INSTRUCTION_TEXTS: [&str; 8] = [
    IMPLEMENTER,
    TASK_REVIEWER,
    BRANCH_REVIEWER,
    SPEC_REVIEWER,
    ORCHESTRATING,
    FINISHING,
    DESIGNING,
    PLANNING,
];

/// The four entries of the design phase (design.md 182) that the spec
/// reviewer cites.
const DESIGN_PHASE: [&str; 4] = [
    "process.design-decisions-carry-probes",
    "process.design-claims-are-labelled",
    "process.specs-list-borrowed-concepts",
    "process.reversals-are-recorded",
];

/// The advice of spec 6.3, as it stands in the middle of a sentence.
const LOST_READ: &str =
    "`<<` marker or a \"more items\" note as a lost read and read the source file";

/// This checkout's repository root.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The directory of the reference workflow in the source tree.
fn package_dir() -> PathBuf {
    repo_root().join("template/workflows/reference")
}

/// The text of the file at `relative` (`/`-separated) below the package.
fn package_text(relative: &str) -> String {
    let path = relative
        .split('/')
        .fold(package_dir(), |path, part| path.join(part));
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The manifest of the package.
fn manifest() -> Value {
    serde_json::from_str(&package_text("workflow.json")).expect("workflow.json is JSON")
}

/// The install paths that the manifest lists.
fn manifest_files() -> Vec<String> {
    manifest()["files"]
        .as_array()
        .expect("files is an array")
        .iter()
        .map(|file| file.as_str().expect("a path").to_string())
        .collect()
}

/// The text of every file that the manifest lists, as `(path, text)` pairs.
fn package_texts() -> Vec<(String, String)> {
    manifest_files()
        .into_iter()
        .map(|file| {
            let text = package_text(&file);
            (file, text)
        })
        .collect()
}

/// `text` with every run of white space collapsed to one space.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Asserts that the package file `file` holds every phrase of `phrases`.
fn assert_holds(file: &str, phrases: &[&str]) {
    let text = flat(&package_text(file));
    for phrase in phrases {
        assert!(text.contains(phrase), "{file} lacks: {phrase}");
    }
}

/// Asserts that the package file `file` holds none of the phrases.
fn assert_lacks(file: &str, phrases: &[&str]) {
    let text = flat(&package_text(file));
    for phrase in phrases {
        assert!(!text.contains(phrase), "{file} still holds: {phrase}");
    }
}

/// Every file below `dir`, as paths.
fn files_below(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files_below(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// `file:line: text` for every line of the package texts that `holds`
/// accepts.
fn lines_where(holds: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    for (file, text) in package_texts() {
        for (number, line) in text.lines().enumerate() {
            if holds(line) {
                found.push(format!("{file}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    found
}

/// Live case 20 (spec 6.4): a case-insensitive search for `superpowers` over
/// every text below `template/`, the workflow included, finds nothing.
#[test]
fn no_template_text_names_the_superpowers_plugin() {
    let mut files = Vec::new();
    files_below(&repo_root().join("template"), &mut files);
    let mut findings = Vec::new();
    for path in &files {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for (number, line) in text.lines().enumerate() {
            if line.to_lowercase().contains("superpowers") {
                findings.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(!files.is_empty(), "the template tree holds no file");
    assert!(
        findings.is_empty(),
        "the plugin is named:\n{}",
        findings.join("\n")
    );
}

/// Live case 20 with ruling 189: no workflow text names a skill of the plugin.
#[test]
fn no_workflow_text_names_a_plugin_skill() {
    let findings = lines_where(&|line| {
        let lower = line.to_lowercase();
        PLUGIN_SKILLS.iter().any(|skill| lower.contains(skill))
    });
    assert!(
        findings.is_empty(),
        "a plugin skill is named:\n{}",
        findings.join("\n")
    );
}

/// Live case 21 (spec 5): no workflow text names the full tier.
#[test]
fn no_workflow_text_names_the_full_tier() {
    let full_tier = Regex::with_flags(FULL_TIER, "i").expect("the full-tier pattern compiles");
    let findings = lines_where(&|line| full_tier.find(line).is_some());
    assert!(
        findings.is_empty(),
        "the full tier is named:\n{}",
        findings.join("\n")
    );
}

/// `true` when `line` has a sentence or clause that names a vendor model and
/// lacks the word `default`.
fn names_a_vendor_model_without_its_default(models: &Regex, line: &str) -> bool {
    clauses(line)
        .iter()
        .any(|clause| models.find(clause).is_some() && !clause.to_lowercase().contains("default"))
}

/// The sentences and clauses of `line`. A sentence ends at `.`, `!`, or `?`,
/// then any closing bracket, quote, backtick, or asterisk, then white space; a
/// clause ends at `;` and white space.
fn clauses(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let mut end = i + 1;
        if matches!(bytes[i], b'.' | b'!' | b'?') {
            while end < bytes.len()
                && matches!(bytes[end], b')' | b']' | b'"' | b'\'' | b'`' | b'*')
            {
                end += 1;
            }
        } else if bytes[i] != b';' {
            i += 1;
            continue;
        }
        if end < bytes.len() && bytes[end].is_ascii_whitespace() {
            parts.push(&line[start..end]);
            start = end + 1;
            i = start;
        } else {
            i += 1;
        }
    }
    parts.push(&line[start..]);
    parts
}

/// The forbidden shapes of the vendor-model guard, each with the harmless
/// shapes beside it (`quality.exclusive-pins-are-mutation-proven`). The first
/// row is line 61 of the orchestrating skill at the base of batch 34: three
/// models alone, and the word `default` in a later clause.
#[test]
fn the_vendor_model_guard_flags_every_model_that_its_clause_does_not_tie_to_a_default() {
    let models = Regex::with_flags(VENDOR_MODELS, "i").expect("the model pattern compiles");
    let shapes = [
        (
            "- Templates: `implementer` (sonnet), `task-reviewer` (opus), `branch-reviewer` (fable); `houserules init --target claude` installs them. Name the model on every dispatch; the template value is the default, not a substitute for naming it.",
            true,
        ),
        (
            "The defaults are sonnet and opus. Run the branch reviewer on haiku.",
            true,
        ),
        (
            "Name the model on every dispatch (sonnet); the template value is the default.",
            true,
        ),
        ("Run the task review on opus.", true),
        ("Run the task review on Opus; the default is set.", true),
        (
            "(Run the reviewer on opus.) The default is set in workflow.models.",
            true,
        ),
        (
            "The task gets one task review on the task reviewer's configured model (default opus), and never a re-review.",
            false,
        ),
        (
            "The defaults are sonnet for the implementer and opus for the other three roles.",
            false,
        ),
        (
            "Each role's model is a configuration value; the default for the implementer is sonnet.",
            false,
        ),
        (
            "Name the model on every dispatch; the template value is the default.",
            false,
        ),
        ("Run the task review on opus (the default).", false),
    ];
    let wrong: Vec<String> = shapes
        .iter()
        .filter(|(line, flagged)| {
            names_a_vendor_model_without_its_default(&models, line) != *flagged
        })
        .map(|(line, flagged)| format!("the guard should flag it: {flagged}: {line}"))
        .collect();
    assert!(wrong.is_empty(), "wrong verdicts:\n{}", wrong.join("\n"));
}

/// Spec 5 and 4.1: a workflow text names a vendor model only in a sentence or
/// clause that also says `default`, because the model of a role is a
/// configuration value, and no agent template carries a `model:` line.
#[test]
fn a_workflow_text_names_a_vendor_model_only_beside_its_default() {
    let models = Regex::with_flags(VENDOR_MODELS, "i").expect("the model pattern compiles");
    let findings = lines_where(&|line| names_a_vendor_model_without_its_default(&models, line));
    assert!(
        findings.is_empty(),
        "a vendor model is named alone:\n{}",
        findings.join("\n")
    );
    for template in AGENT_TEMPLATES {
        let text = package_text(template);
        assert!(
            !text.lines().any(|line| line.starts_with("model:")),
            "{template} carries a model line"
        );
    }
}

/// Spec 4.1 and 6.4: the manifest lists the spec reviewer, the designing
/// skill, and the planning skill, and requires the four design-phase entries
/// that the spec reviewer cites.
#[test]
fn the_manifest_lists_the_design_files_and_requires_the_design_phase_entries() {
    let files = manifest_files();
    for expected in [SPEC_REVIEWER, DESIGNING, PLANNING] {
        assert!(files.iter().any(|file| file == expected), "{expected}");
    }
    let required: Vec<String> = manifest()["requires"]
        .as_array()
        .expect("requires is an array")
        .iter()
        .map(|id| id.as_str().expect("an id").to_string())
        .collect();
    for id in DESIGN_PHASE {
        assert!(required.iter().any(|known| known == id), "requires: {id}");
    }
    assert_eq!(manifest()["models"]["spec-reviewer"], "opus");
}

/// Spec 6.1 and 4.5: the spec reviewer is a read-only agent that cites the
/// four design-phase entries by id and files a `spec-review` with the
/// fields of the schema.
#[test]
fn the_spec_reviewer_names_the_design_phase_entries_and_the_spec_review_fields() {
    let text = package_text(SPEC_REVIEWER);
    assert!(text.starts_with("---\nname: spec-reviewer\n"), "{text}");
    assert!(text.contains("disallowedTools: Agent, Edit, Write, NotebookEdit\n"));
    let reviewer = flat(&text);
    for id in DESIGN_PHASE {
        assert!(reviewer.contains(&format!("`{id}`")), "{id}");
    }
    assert_holds(
        SPEC_REVIEWER,
        &[
            "`TARGET`",
            "`PRIOR`",
            "`Model:`",
            "`spec-review`",
            "`verdict` (`approve` when you file no finding, else `needs-fixes`)",
            "`severity` (`critical`, `important`, or `minor`)",
            "`facts`",
            "`controller_calls`",
            "`questions_for_owner`",
            "`prior`",
            "`<REVIEW_FILE stem>-probe-<slug>.txt`",
            "`spec-review.json` and `spec-review-r<n>.json`",
            "Rerun the source of every fact",
        ],
    );
}

/// Spec 6.1 and 6.2: the designing skill drafts the spec from what the project
/// holds, asks only what that leaves open, runs the design review with its
/// loop, and folds in the technical-debt candidates.
#[test]
fn the_designing_skill_drafts_the_spec_and_runs_the_design_review_loop() {
    let text = package_text(DESIGNING);
    assert!(
        text.starts_with("---\nname: designing\ndescription: Use when "),
        "{text}"
    );
    assert_holds(
        DESIGNING,
        &[
            "ask the human only what it leaves open",
            "A review that holds a Critical finding sends the spec to a fresh `spec-reviewer`",
            "Repeat until a review holds no Critical finding.",
            "Fold its Important and Minor findings into the spec without a further review.",
            "`spec-review-r<R>.json`",
            "as a `Reversal:` line in the ledger (`process.reversals-are-recorded`)",
            "`houserules list --deferred`",
            "folding it in does not turn a non-critical task into a critical task",
            "each overlapping deferral that you leave out with the reason",
            "lists the reds that the change causes by design on an install of the previous version",
            "`houserules check-report-claims --doc <the spec path>`",
            "A light change needs no spec document",
        ],
    );
}

/// Spec 6.2 and 5: the planning skill folds in the technical-debt candidates,
/// marks the critical tasks, scans the code health, and names the work items
/// each task delivers.
#[test]
fn the_planning_skill_folds_in_deferrals_and_marks_critical_tasks() {
    let text = package_text(PLANNING);
    assert!(
        text.starts_with("---\nname: planning\ndescription: Use when "),
        "{text}"
    );
    assert_holds(
        PLANNING,
        &[
            "`houserules list --deferred`",
            "folding it in does not turn a non-critical task into a critical task",
            "each overlapping deferral that you leave out with the reason",
            "Mark each critical task",
            "one recorded run that shows them failing at `BASE`",
            "Write a Code health section",
            "The work item ids it delivers (`Backlog:`)",
            "a paper walk",
            "Mark tasks parallel only when their declared file sets are disjoint",
        ],
    );
}

/// Spec 4.1: the implementer and the finishing skill write the `Work-Item:`
/// trailer for each id, and the orchestrating skill names those ids in every
/// dispatch.
#[test]
fn the_workflow_teaches_the_work_item_trailer_per_backlog_id() {
    assert_holds(
        IMPLEMENTER,
        &[
            "Write one `Work-Item: <id>` trailer for each id of your `Backlog:` line on every commit, in the last paragraph of the message, one trailer per line.",
            "A commit that files a deferral also carries `Deferred: <id> <reason>`.",
        ],
    );
    assert_holds(
        FINISHING,
        &[
            "Write one `Work-Item: <id>` trailer for each item that an aggregated commit delivers, in the last paragraph of its message, one trailer per line.",
            "The `Work-Item:` and `Deferred:` trailers are no co-author lines.",
        ],
    );
    assert_holds(
        ORCHESTRATING,
        &[
            "`Backlog: <ids the task delivers>` — the implementer writes one `Work-Item: <id>` trailer per id on every commit.",
        ],
    );
}

/// Spec 5: the orchestrating skill has two tiers, light and standard, the
/// dispatch line names them, and a critical task has its own evidence.
#[test]
fn the_orchestrating_skill_has_two_tiers_and_the_critical_task_rule() {
    assert_holds(
        ORCHESTRATING,
        &[
            "`Tier: <light|standard> (<criterion>)`",
            "You pick between two tiers, light and standard.",
            "no task is critical",
            "A critical task is a task whose change alters the CLI's commands, flags, or output, a schema, authentication or security code, or the data on disk of an install.",
            "The implementer commits those tests first, with one recorded run that shows them failing at `BASE`, then makes them pass.",
            "The evidence is that run and the whole suite passing at `HEAD`.",
            "one task review on the task reviewer's configured model (default opus), and never a re-review",
            "you record the check in the ledger (`process.fix-round-verification-record`)",
            "A Critical or Important finding goes to a fresh implementer. You fix a Minor finding yourself.",
            "The cap on re-reviews per task is the tier's: light 0, standard 1. A critical task gets none.",
        ],
    );
    let text = package_text(ORCHESTRATING);
    assert!(text.lines().any(|line| line.starts_with("| light |")));
    assert!(text.lines().any(|line| line.starts_with("| standard |")));
    assert!(
        !text
            .lines()
            .any(|line| line.to_lowercase().starts_with("| full"))
    );
}

/// Spec 5: the implementer and the task reviewer carry the critical-task
/// rules, and the task reviewer checks the tier against the two tiers.
#[test]
fn the_critical_task_rules_reach_the_implementer_and_the_task_reviewer() {
    assert_holds(
        IMPLEMENTER,
        &[
            "A critical task follows its own order.",
            "write the spec's cases as tests first and commit them alone, with one recorded run that shows them failing at `BASE`",
            "Your evidence is that run and the whole suite passing at `HEAD`",
            "the further steps need no RED/GREEN entry and no mutation proof",
        ],
    );
    assert_holds(
        TASK_REVIEWER,
        &[
            "The tiers are light and standard.",
            "is a critical task, and its `Tier:` line says so",
            "A critical task gets this one review and no re-review.",
            "the spec's cases committed first as tests, one recorded run that shows them failing at `BASE`, and the whole suite passing at `HEAD`",
            "Give each of its findings a reproduction, a command and its expected result, that the controller re-runs to verify the fix.",
        ],
    );
}

/// Spec 6.3: every agent template and skill tells an agent to write
/// `houserules` output to a file and to read a lost read from the source.
#[test]
fn every_role_reads_houserules_output_through_files() {
    for file in INSTRUCTION_TEXTS {
        assert_holds(file, &[LOST_READ]);
    }
    assert_holds(
        IMPLEMENTER,
        &["`houserules get <ids> > <workspace>/knowledge-<task>.json`"],
    );
    assert_holds(
        ORCHESTRATING,
        &["`houserules list --open > <workspace>/open.json`"],
    );
}

/// C28 (spec 4.5): each line that names `branch-review.json` as a file that
/// `stats` reads also names the two spec-review file names.
#[test]
fn the_stats_file_lists_name_the_spec_review_files() {
    let lines = lines_where(&|line| line.contains("branch-review.json") && line.contains("stats"));
    assert!(lines.len() >= 2, "{lines:?}");
    for line in lines {
        assert!(
            line.contains("`spec-review.json`") && line.contains("`spec-review-r<n>.json`"),
            "{line}"
        );
    }
}

/// Spec 4.1 and 4.5: the orchestrating skill names the model on every dispatch
/// from `workflow.models`, and each agent records it in `model`.
#[test]
fn a_dispatch_names_the_model_and_every_role_records_it() {
    assert_holds(
        ORCHESTRATING,
        &[
            "`workflow.models` in `.houserules.json` holds it, and the defaults are sonnet for the implementer and opus for the other three roles.",
            "A template carries no `model:` line, so a dispatch that names no model runs on your own session's model.",
            "check that the dispatch names one before you send it",
            "`Model: <the role's model>` — the model the agent records in the `model` field of its deliverable.",
            "`implementer`, `task-reviewer`, `branch-reviewer`, and `spec-reviewer`; `houserules init --workflow reference` installs them.",
        ],
    );
    for template in AGENT_TEMPLATES {
        assert_holds(template, &["`Model:` (the model you record in `model`)"]);
    }
    assert_holds(IMPLEMENTER, &["`model` (the value of your `Model:` line"]);
    assert_holds(TASK_REVIEWER, &["`model` (the value of your `Model:` line"]);
    assert_holds(
        BRANCH_REVIEWER,
        &["`model` (the value of your `Model:` line"],
    );
    assert_holds(SPEC_REVIEWER, &["`model` (the value of your `Model:` line"]);
}

/// The lifecycle table of the orchestrating skill names the designing and
/// planning skills and the design review (spec 6.4).
#[test]
fn the_lifecycle_table_names_the_designing_and_planning_skills() {
    let text = package_text(ORCHESTRATING);
    let rows: Vec<&str> = text.lines().filter(|line| line.starts_with("| ")).collect();
    let row = |phase: &str| {
        rows.iter()
            .find(|line| line.starts_with(&format!("| {phase} |")))
            .copied()
            .unwrap_or_else(|| panic!("no {phase} row"))
    };
    assert!(
        row("Design").contains("| `designing` |"),
        "{}",
        row("Design")
    );
    assert!(row("Design").contains("design review by the `spec-reviewer`"));
    assert!(row("Plan").contains("| `planning` |"), "{}", row("Plan"));
    assert!(row("Finish").contains("| `finishing-a-feature` |"));
}

/// HR-129 (1) and (4): an issue that cites a rule flips that rule's row, and
/// a fix round records the whole gate set.
#[test]
fn the_task_reviewer_flips_a_cited_row_and_checks_the_gate_set_of_a_fix_round() {
    assert_holds(
        TASK_REVIEWER,
        &[
            "An issue that cites a rule flips that rule's `rule_adherence` row to `fail` in the same review.",
            "Compare the round's `gates_run` with the whole gate set of the task, the gates of the round-0 `tests` included: a shrunken list is a finding under `new_breakage`.",
        ],
    );
    assert_holds(
        IMPLEMENTER,
        &["`gates_run` lists the whole gate set of the task, not only the gates the round touched"],
    );
    assert_holds(
        ORCHESTRATING,
        &[
            "Confirm that the round's `gates_run` lists the whole gate set of the task, not only the gates the round touched.",
        ],
    );
}

/// HR-129 (3): a brief that forbids tree changes names who files deferrals.
#[test]
fn a_brief_that_forbids_tree_changes_names_who_files_deferrals() {
    assert_holds(
        ORCHESTRATING,
        &[
            "A brief that forbids tree changes names who files its deferrals: you, by default. The implementer then lists each deferral in `concerns` and runs no `houserules defer`.",
        ],
    );
    assert_holds(
        IMPLEMENTER,
        &[
            "When your brief forbids tree changes, it names who files deferrals: list each deferral in `concerns` and file none yourself.",
        ],
    );
}

/// HR-150: the closing `check-report-claims` entry has no redirect, or a
/// capture that no earlier entry names, and a fix to the report sends the
/// implementer back to the citation step.
#[test]
fn the_closing_entry_has_no_redirect_and_a_report_fix_repeats_the_citation_step() {
    assert_holds(
        IMPLEMENTER,
        &[
            "Record that entry with no redirect, or with a redirect to a capture file that no earlier entry names",
            "After a fix to the report, repeat the citation step (step 4), then the closing run.",
        ],
    );
}

/// HR-165 (4), (5), (6), and (7): the pin of existing behavior, the closure
/// probe and the instruction keyed to a field, the check of the commits at
/// the finish, and the mutation row of a fix round.
#[test]
fn the_riders_of_hr_165_reach_the_templates_and_the_finishing_skill() {
    assert_holds(
        IMPLEMENTER,
        &[
            "A pin of existing behavior (a test that already passes) runs at `BASE` first. Record its passing exit beside its disclosed mutation, which proves that the pin can fail; a critical task names its pins in its one recorded run instead and needs no mutation.",
            "A test that your fix round adds and that passes before the fix gets a `mutation` row in `tdd` for that round, with the mutation diff captured.",
        ],
    );
    assert_holds(
        TASK_REVIEWER,
        &[
            "An instruction in the diff that is keyed to a tier, a count, or a version is read once per value of that field.",
            "Check that every test the round adds and that passes before the fix has a `mutation` row in the round's `tdd`, with the mutation diff captured",
        ],
    );
    assert_holds(
        FINISHING,
        &[
            "Run `houserules check-commit --from <merge base>` before the merge, even when the commit-msg hook skipped it",
        ],
    );
}

/// HR-166 (3): the orchestrating skill names only the sequential sentence of
/// the 1.0.0 `CLAUDE.md` line, and keeps the clause of the `agentsBlock`.
#[test]
fn the_orchestrating_skill_names_only_the_sequential_sentence_of_the_old_line() {
    assert_holds(
        ORCHESTRATING,
        &[
            "delete only its sentence `Dispatch agent work sequentially by default, until the project rules its own parallelism discipline.`",
            "or move your own text into `AGENTS.md` and leave `CLAUDE.md` as the one line `@AGENTS.md`",
            "`run handoffs in parallel only when the plan marks their file sets disjoint, each in its own worktree`",
        ],
    );
    assert_lacks(
        ORCHESTRATING,
        &[
            "delete the old line from `CLAUDE.md`",
            "`update` adds an `AGENTS.md` that already carries the new line",
        ],
    );
}

/// HR-153: the paper walk and the three measurements of a brief, the read-only
/// comparison and the unlisted commits of the branch review, and the fetch of
/// the finishing skill.
#[test]
fn the_riders_of_hr_153_reach_the_skills_and_the_branch_reviewer() {
    assert_holds(
        ORCHESTRATING,
        &[
            "carries a paper walk: a fresh install, an install on each released kit under the new binary, an install with kept or overridden kit-owned files, and each author class of commits. The report states each accepted red.",
            "A brief that dictates a message reads it once for each state that prints it.",
            "A brief that dictates an enumeration derives it from the subject, never from a file name.",
            "A brief that dictates a mechanism that a reviewer proposed has run it against the reviewer's own counter-example first.",
            "keep the old branch under a new name, write the old-to-new sha map to its own workspace file",
        ],
    );
    assert_holds(
        IMPLEMENTER,
        &["also each red that the paper walk of your brief accepts"],
    );
    assert_holds(
        BRANCH_REVIEWER,
        &[
            "Compare `BASE` with the tip of the remote default branch through a read-only call (`git ls-remote origin HEAD`).",
            "List the commits of `BASE..HEAD` that no task report lists",
            "keeps the old branch under a new name and the old-to-new sha map in its own workspace file",
            "Your loaded instructions can be older than `.claude/agents/branch-reviewer.md` at HEAD",
        ],
    );
    assert_holds(
        FINISHING,
        &[
            "Fetch first: `git fetch origin main`. If `git merge-base --is-ancestor origin/main HEAD` fails, main moved: rebuild the branch on `origin/main`",
            "git reset --soft $(git merge-base origin/main HEAD)",
        ],
    );
    assert_lacks(FINISHING, &["git reset --soft $(git merge-base main HEAD)"]);
}

/// The steps of the finishing skill from step 4 to step 5, with every run of
/// white space collapsed: the steps that run after the fetch.
fn finishing_steps_after_the_fetch() -> String {
    let text = flat(&package_text(FINISHING));
    let start = text.find("4. **Aggregate commits.**").expect("step 4");
    let end = text.find("5. **Push and open a PR.**").expect("step 5");
    text[start..end].to_string()
}

/// HR-153: step 4 moves the base to `origin/main`, so every step after the
/// fetch computes its range from that base. Local `main` stays stale until
/// step 7 pulls it.
#[test]
fn the_finishing_steps_after_the_fetch_use_the_fetched_base() {
    let steps = finishing_steps_after_the_fetch();
    let stale = steps
        .replace("git fetch origin main", "")
        .replace("origin/main", "")
        .replace("main moved", "");
    let local_main = Regex::new(r"\bmain\b").expect("the pattern compiles");
    assert!(
        local_main.find(&stale).is_none(),
        "a step after the fetch names the local main: {steps}"
    );
    assert_holds(
        FINISHING,
        &[
            "(`git diff origin/main...HEAD | grep -oE '\\b[0-9a-f]{7,40}\\b'`, judged in context)",
            "The merge base is the output of `git merge-base origin/main HEAD`.",
        ],
    );
    assert_lacks(
        FINISHING,
        &["git diff main...HEAD", "git merge-base main HEAD"],
    );
}

/// The rebuild of step 4 and of the first mistake prints its commands: the
/// old branch under a new name, the rebase, and the old-to-new sha map.
#[test]
fn the_finishing_skill_prints_the_commands_of_the_rebuild() {
    assert_holds(
        FINISHING,
        &[
            "git branch <branch>-pre-rebuild",
            "git rebase origin/main",
            "git log --reverse --format=%H origin/main..<branch>-pre-rebuild > <workspace>/old-shas.txt",
            "git log --reverse --format=%H origin/main..<branch> > <workspace>/new-shas.txt",
            "paste <workspace>/old-shas.txt <workspace>/new-shas.txt > <workspace>/sha-map.txt",
            "If a rebase drops a commit that `origin/main` already holds, the lists differ in length: pair the rest by subject.",
            "Run `git fetch origin main`, rebuild the branch with `git rebase origin/main` (step 4), re-verify green, then aggregate commits as in step 4.",
        ],
    );
}

/// HR-165 (6): step 4c names every condition under which the commit-msg hook
/// skips `houserules check-commit` (`.githooks/commit-msg`).
#[test]
fn step_4c_names_every_skip_condition_of_the_commit_msg_hook() {
    assert_holds(
        FINISHING,
        &[
            "The hook skips the check when `houserules check-commit --help` fails (no `houserules` on `PATH`, or one without the `check-commit` subcommand), when it cannot resolve the install root of the hook, and when the parent of the hook directory holds no `.houserules.json`.",
        ],
    );
    assert_lacks(
        FINISHING,
        &["the hook skips the check when no `houserules` is on `PATH`"],
    );
}

/// The batch record of each tracker: both skills that record a batch name the
/// JSON entry, the GitHub issue, and the Beads issue, as spec 3.5 (C30) lays
/// them out.
#[test]
fn the_skills_name_the_batch_record_of_each_tracker() {
    for skill in [ORCHESTRATING, FINISHING] {
        assert_holds(
            skill,
            &[
                "`backlog/batches.json`",
                "`houserules:batch`",
                "`houserules-batch`",
            ],
        );
    }
    assert_holds(
        ORCHESTRATING,
        &[
            "The batch record keeps the batch's `number`, `summary`, `kickoff`, and `status`.",
            "- JSON: an entry of `backlog/batches.json`.",
            "- GitHub: an issue labelled `houserules:batch` and titled `Batch <n>: <summary>`. A fenced `houserules` block at the end of its body holds `kickoff` and `status`, for example `status: {\"state\":\"in-progress\",\"text\":\"\"}`. The issue is open while the batch is planned or in progress, and closed when it is done.",
            "- Beads: an issue of the custom type `houserules-batch`, with the same title and block.",
            "No `houserules` command writes a GitHub or Beads batch record: create and edit the issue with `gh` or `bd`.",
        ],
    );
    assert_holds(
        FINISHING,
        &[
            "Set the `status` state of the batch record to `done`, and run `houserules check-backlog`.",
            "on the JSON tracker, edit its entry in `backlog/batches.json`; on the GitHub tracker, edit the `status` in the `houserules` block of the issue labelled `houserules:batch` and close it; on the Beads tracker, do the same on the issue of the custom type `houserules-batch`.",
            "Commit the files that this update changed.",
        ],
    );
    assert_lacks(
        ORCHESTRATING,
        &["(the JSON tracker keeps it in `backlog/batches.json`)"],
    );
    assert_lacks(
        FINISHING,
        &["(on the JSON tracker, a direct edit of `backlog/batches.json`)"],
    );
}

/// HR-203 item 3: the orchestrating skill removes a task's worktree and
/// branch after the integration.
#[test]
fn the_orchestrating_skill_removes_the_worktree_and_branch_after_integration() {
    assert_holds(
        ORCHESTRATING,
        &[
            "After you integrate a task, remove its worktree and its branch: `git worktree remove <path>`, then `git branch -D <branch>`",
        ],
    );
}

/// The eval scenarios expect the trailer and the two tiers (spec 4.1, 16),
/// and the scenario text names neither a full tier nor a plugin.
#[test]
fn the_scenarios_expect_the_trailer_and_the_two_tiers() {
    let behavior = |file: &str| -> Vec<String> {
        let scenario: Value = serde_json::from_str(&package_text(file)).expect("a scenario");
        scenario["expected_behavior"]
            .as_array()
            .expect("expected_behavior")
            .iter()
            .map(|line| line.as_str().expect("a line").to_string())
            .collect()
    };
    for file in [
        ".claude/evals/dependency-add.json",
        ".claude/evals/docs-edit.json",
    ] {
        assert!(
            behavior(file).iter().any(|line| line.contains(
                "carries one `Work-Item: <id>` trailer for each id of the brief's `Backlog:` line"
            )),
            "{file} expects no trailer"
        );
    }
    assert!(
        behavior(".claude/evals/seeded-violations.json")
            .iter()
            .any(|line| line.contains("checked against the two tiers, light and standard")),
        "seeded-violations expects no tier check"
    );
}

/// Spec 4.1 and 6.4: the session-start ritual reads the session commands from
/// the `AGENTS.md` block and names the orchestrating skill by its file.
#[test]
fn the_session_start_ritual_names_the_session_section_and_the_orchestrating_skill() {
    let text = package_text(SESSION_START);
    assert!(
        text.contains("Session ritual: run `git status --short && git log --oneline -15`, run the start command of the Session section in the houserules block of `AGENTS.md`, read the plan ledger when a plan is in flight, then read `.claude/skills/orchestrating/SKILL.md` before you act."),
        "{text}"
    );
    assert!(!text.contains("houserules list --batch"), "{text}");
}
