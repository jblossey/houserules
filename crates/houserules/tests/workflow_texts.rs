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
//! The report form of batch 36 (spec 7 and 8.6) reaches the implementer, the
//! task reviewer, the branch reviewer, the skills, the `dependency-add`
//! scenario, and the `process` pack: every run is a capture, a claim cites a
//! recorded line, and the closing act runs `report-facts`.
//!
//! The guard of the plugin (live case 20) walks `template/` only. It leaves
//! out `crates/houserules/released/`, which the binary embeds as well: the
//! files there keep the bytes of earlier releases, which the provenance rule
//! of spec 4.7 compares and never writes (C42). Five copies of the
//! orchestrating skill and `entries.json` there name the plugin.
//!
//! The template defects of HR-241 (spec 11.1 to 11.5) each have a case: the
//! description of `issue.backlog` in both copies of the deliverables schema,
//! the sentence of the two reviewer templates, and the sentences of the
//! orchestrating and planning skills.
//!
//! The per-change review layer of batch 37 reaches the kit texts at the end of
//! the file: the seed rule of the evidence bundle and the fix-diff review of
//! a critical task in the process pack, the bundle step of the finishing and
//! orchestrating skills, the parts of the review layer in the
//! migrating-knowledge skill and the upgrade guide, and what the README with
//! its review-layer page, the guide, and the knowledge base say about the CI
//! files that `init` writes.

mod docs_support;
mod guard_lists;
mod help_support;
mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use guard_lists::{FULL_TIER, PLUGIN_SKILLS, VENDOR_MODELS};
use help_support::{omits, states};
use regress::Regex;
use serde_json::{Value, json};
use tracker_support::{flat, repo_root};

const IMPLEMENTER: &str = ".claude/agents/implementer.md";
const TASK_REVIEWER: &str = ".claude/agents/task-reviewer.md";
const BRANCH_REVIEWER: &str = ".claude/agents/branch-reviewer.md";
const SPEC_REVIEWER: &str = ".claude/agents/spec-reviewer.md";
const ORCHESTRATING: &str = ".claude/skills/orchestrating/SKILL.md";
const FINISHING: &str = ".claude/skills/finishing-a-feature/SKILL.md";
const DESIGNING: &str = ".claude/skills/designing/SKILL.md";
const PLANNING: &str = ".claude/skills/planning/SKILL.md";
const SESSION_START: &str = "tools/reference-session-start.sh";
const DOCS_EDIT: &str = ".claude/evals/docs-edit.json";

/// The step skill of the core kit, below `template/`.
const STEP_SKILL: &str = ".claude/skills/houserules-step/SKILL.md";

/// The workspace wording of spec 7, as it stands in the middle of a sentence.
const WORKSPACES_LOCATION: &str = "the `workspaces` location (default `.houserules/work/`)";

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

/// The text of the file at `relative` (`/`-separated) below `template/`.
fn core_text(relative: &str) -> String {
    let path = relative
        .split('/')
        .fold(repo_root().join("template"), |path, part| path.join(part));
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
            "one task review on the task reviewer's configured model (default opus). It gets no re-review of its original findings.",
            "you record the check in the ledger (`process.fix-round-verification-record`)",
            "A Critical or Important finding goes to a fresh implementer. You fix a Minor finding yourself.",
            "The cap on re-reviews per task is the tier's: light 0, standard 1. A critical task gets no re-review of its original findings",
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
            "A critical task gets one task review and no re-review of its original findings.",
            "In the task review, check its evidence: the spec's cases committed first as tests, one recorded run that shows them failing at `BASE`, and the whole suite passing at `HEAD`",
            "Give each of its findings a reproduction, a command and its expected result, that the controller re-runs to verify the fix.",
        ],
    );
}

/// Ruling 260 reaches the task reviewer: after a fix round of the kind that
/// `process.risk-tiers` names, the dispatch asks for one review of the fix
/// diff, written in the re-review form; a critical task still gets no
/// re-review of its original findings.
#[test]
fn the_task_reviewer_writes_the_fix_diff_review_of_a_critical_task() {
    assert_holds(
        TASK_REVIEWER,
        &[
            "After a fix round that fixes a Critical or Important finding, or that changes files beyond the files its findings name, the dispatch asks for one review of the fix diff.",
            "Write it in the re-review form, as `task-<N>-review-r<R>.json` of kind `re-review`: `finding_verdicts` for the findings under verification, and every new problem of the fix diff under `new_breakage`.",
            "Only a new Critical or Important finding opens a further round, which closes by the controller's check, with no second fix-diff review.",
        ],
    );
    assert_lacks(
        TASK_REVIEWER,
        &[
            "this one review and no re-review",
            "A critical task gets this one review",
        ],
    );
}

/// Step 9 of the task reviewer says that the `mutation` row that step 7 asks for
/// does not apply to a critical task: the evidence of a critical task is its one
/// recorded run at `BASE` and the whole suite at `HEAD`, and the fix-diff review
/// files no missing mutation row against it.
#[test]
fn step_9_of_the_task_reviewer_lifts_the_mutation_row_of_step_7_for_a_critical_task() {
    let text = package_text(TASK_REVIEWER);
    let step = |number: u32| {
        let lead = format!("{number}. ");
        let lines: Vec<String> = text
            .lines()
            .filter(|line| line.starts_with(&lead))
            .map(flat)
            .collect();
        assert_eq!(lines.len(), 1, "one step {number}: {lines:?}");
        lines[0].clone()
    };
    assert!(
        step(7).contains("has a `mutation` row in the round's `tdd`"),
        "step 7 asks for the row"
    );
    states(
        TASK_REVIEWER,
        &step(9),
        &["The `mutation` row that step 7 asks for does not apply to a critical task."],
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
            "An issue that cites a judged rule flips that rule's `rule_adherence` row to `fail` in the same review; the row of a deterministic rule keeps its result.",
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

/// Spec 7 and design.md 212: the closing act has no redirect rule, because the
/// check of the redirected capture retired with the inline output it read. A
/// fix to the report sends the implementer back to the citation step, and the
/// pointer to the limits of the checker names its submodules, where the limits
/// of each check now live.
#[test]
fn the_closing_act_has_no_redirect_rule_and_points_at_the_limits_of_the_submodules() {
    assert_lacks(
        IMPLEMENTER,
        &[
            "Record that entry with no redirect, or with a redirect to a capture file that no earlier entry names",
            "a re-run of an entry that redirects into a capture another entry names truncates the capture before the checker reads it",
            "paste its clean run as the final `tests` entry",
            "(`crates/houserules/src/report_claims.rs`), also name it as a new Limits bullet there",
        ],
    );
    assert_holds(
        IMPLEMENTER,
        &[
            "After a fix to the report, repeat the citation step (step 4), then the closing act.",
            "Where this repository carries the checker's own source (`crates/houserules/src/report_claims.rs` and the submodules in `crates/houserules/src/report_claims/`), also name it as a new Limits bullet in the module doc of the submodule that holds the check",
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
    let end = text
        .find("5. **Write the evidence bundle, push, and open the pull or merge request.**")
        .expect("step 5");
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

/// HR-215: every agent template records the value of its `Model:` line as
/// written, and `unknown` when the task has no such line.
#[test]
fn every_role_records_its_model_line_as_written() {
    for template in AGENT_TEMPLATES {
        assert_holds(
            template,
            &[
                "`model` (the value of your `Model:` line as written, with no alias swapped for a model id or the reverse; `unknown` when your task has no `Model:` line)",
            ],
        );
    }
}

/// HR-215 and spec 6: the implementer fixes every `fail` row and every line of
/// `references.failures`, and runs the closing act again when a stale
/// `self_audit` copy fails the report.
#[test]
fn the_implementer_fixes_reference_failures_and_reruns_the_closing_act_on_a_stale_self_audit() {
    assert_holds(
        IMPLEMENTER,
        &[
            "Fix a `fail` row in the code, the commits, or the report, and a line of `references.failures` in the commit trailer or the tracker item that it names; re-run until the audit shows no `fail` row other than a sanctioned one and an empty `references.failures`. Name a line that you cannot fix in `concerns`.",
            "A `self_audit differs at <path>` fail of `process.deliverables-json` means that your copy is stale: run the closing act again from the second step.",
        ],
    );
}

/// HR-219 (5): the implementer hands a refused command back and never runs it
/// another way.
#[test]
fn the_implementer_hands_a_refused_command_back() {
    assert_holds(
        IMPLEMENTER,
        &[
            "When the harness refuses a command, do not run it another way, through a wrapper, another shell, or another tool. Stop, and name the refused step in `concerns` so the person who runs the work takes it over.",
        ],
    );
}

/// HR-219 (6), HR-215, and spec 6: the task reviewer sets `result` on judged
/// rows only, names a failing reference as a finding, and keeps `sanctioned`
/// on a row it judges.
#[test]
fn the_task_reviewer_sets_result_on_judged_rows_only_and_reads_the_reference_counts() {
    assert_holds(
        TASK_REVIEWER,
        &[
            "Set `result` only on a judged row of `rule_adherence`. A deterministic row keeps the result the audit gave it; a finding against its rule goes into `issues[]`.",
            "A judged row that carries `sanctioned` keeps that field.",
            "A line in `references.failures`, or a `self_audit.summary.reference_fail` that differs from the audit, is a finding.",
        ],
    );
    assert_lacks(
        TASK_REVIEWER,
        &["An issue that cites a rule flips that rule's `rule_adherence` row"],
    );
}

/// HR-176, HR-188, and spec 6: the branch reviewer compares the read count of
/// the evidence `set in <triggered> of <read> reports`, runs `stats` with
/// `--window 5`, and keeps `sanctioned` on a row it judges.
#[test]
fn the_branch_reviewer_compares_the_read_count_and_names_the_stats_window() {
    assert_holds(
        BRANCH_REVIEWER,
        &[
            "compare the second number (`<read>`) of each report-field evidence that reads `set in <triggered> of <read> reports` with that workspace's `task-*-report.json` listing; a mismatch invalidates every report-field row. The first number counts only the reports that the check's `if` globs triggered, so it can be lower than the listing.",
            "(the controller's `houserules stats --window 5` run over the workspaces)",
            "run `houserules stats --window 5 <every WORKSPACE>` into a file only when no workspace holds that file",
            "a judged row that carries `sanctioned` keeps that field",
        ],
    );
    assert_lacks(BRANCH_REVIEWER, &["over the five most recent workspaces"]);
}

/// Spec 4, 7, 11, and 12: the orchestrating skill names the workspace, the
/// ledger, the override of a judged row, the window of `stats`, and the binary
/// of a template evaluation.
#[test]
fn the_orchestrating_skill_names_the_workspace_the_override_the_window_and_the_binary() {
    assert_holds(
        ORCHESTRATING,
        &[
            "The batch workspace is the folder `<YYYY-MM-DD>-batch-<n>` of the `workspaces` location (default `.houserules/work/`). A project sets another location with the `workspaces` key of `.houserules.json`. The folder holds the deliverables of the batch and its ledger, `progress.md`.",
            "Git ignores the default location; `houserules check-knowledge` fails an entry whose `verify` path names a file in any ignored location, the default included: commit a copy under a tracked path and verify that.",
            "read the ledger `progress.md` in the batch workspace",
            "When you or the project's owner or decider rule against a reviewer's verdict on a judged row, record the ruling on that row of the review file as `override`: `{ \"result\": \"pass\" | \"fail\", \"by\": \"<person or role>\", \"reason\": \"<text>\" }`. The reviewer's `result` stays on the row. You write the `override`; a reviewer never writes one. `houserules stats` reads it into the `override_rate` of the rule.",
            "Run `houserules stats --window 5 <WORKSPACE>...` with every batch workspace, this batch's included, and save the output as `<workspace>/stats.json`",
            "The flag keeps the five most recent of the workspaces you name, and every workspace when fewer than five exist.",
            "When the branch changes the houserules binary, replace `houserules` in the brief with the path of the binary built from the branch.",
        ],
    );
    assert_lacks(
        ORCHESTRATING,
        &["over the five most recent batch workspaces"],
    );
}

/// Spec 7: each skill that names the batch workspace names it as a folder of
/// the `workspaces` location.
#[test]
fn every_skill_names_the_workspace_as_a_folder_of_the_workspaces_location() {
    for skill in [ORCHESTRATING, PLANNING, DESIGNING, FINISHING] {
        assert_holds(skill, &[WORKSPACES_LOCATION]);
    }
    for skill in [PLANNING, DESIGNING, FINISHING] {
        assert_holds(
            skill,
            &[
                "to a file in the batch workspace, a folder of the `workspaces` location (default `.houserules/work/`), and read the file with your file-read tool",
            ],
        );
    }
}

/// HR-219 (7): the brief of the `docs-edit` scenario tells the agent to run
/// every `houserules` command as `houserules`, so that a run against a branch
/// that changes the binary can swap in the binary of the branch.
#[test]
fn the_docs_edit_scenario_names_the_binary() {
    let scenario: Value =
        serde_json::from_str(&package_text(DOCS_EDIT)).expect("docs-edit.json is JSON");
    let query = scenario["query"].as_str().expect("a query");
    assert!(
        query.contains("Run every houserules command as `houserules`."),
        "{query}"
    );
}

/// Spec 7 and 12: the step skill, a core text, names the `.houserules/` ignore
/// line and the `workspaces` location, and says that a runner check needs the
/// change committed.
#[test]
fn the_step_skill_names_the_ignore_line_and_the_committed_change_of_a_runner_check() {
    let skill = flat(&core_text(STEP_SKILL));
    for phrase in [
        "`init` adds `.houserules/` to `.gitignore`: this script writes its scratch output to `.houserules/step/`, and `.houserules/work/`, the default `workspaces` location, holds the workspaces of the project, which stay out of git.",
        "Commit the change first: `audit` judges the commits from `<base>` to `HEAD`, and a runner check runs a project tool on the working tree. When a file that the tool reads differs from `HEAD`, `audit` exits 2 with no rows and a line that names the file.",
    ] {
        assert!(skill.contains(phrase), "{STEP_SKILL} lacks: {phrase}");
    }
    assert!(
        !skill.contains("`init` adds `.houserules/step/` to `.gitignore`"),
        "{STEP_SKILL} still names the older ignore line"
    );
}

// ---- Batch 36: evidence by capture (spec 7 and 8.6) -----------------------------------

/// The eval scenario that expects a report in the form of the implementer.
const DEPENDENCY_ADD: &str = ".claude/evals/dependency-add.json";

/// The process pack of the kit, parsed.
fn process_pack() -> Value {
    let path = repo_root().join("template/packs/process.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).expect("the process pack is JSON")
}

/// The entry `id` of `pack`.
fn pack_entry<'a>(pack: &'a Value, id: &str) -> &'a Value {
    pack["entries"]
        .as_array()
        .expect("a pack holds entries")
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("the pack holds no entry {id}"))
}

/// The paragraphs of the body of `entry`, joined by one space.
fn body_text(entry: &Value) -> String {
    let paragraphs: Vec<&str> = entry["body"]
        .as_array()
        .expect("an entry holds a body")
        .iter()
        .map(|paragraph| paragraph.as_str().expect("a paragraph is a string"))
        .collect();
    flat(&paragraphs.join(" "))
}

/// Spec 7: the implementer runs every test, gate, live run, RED, and GREEN
/// through `capture`, and the report holds the run as a capture, not as output.
/// The example of a variable prefix names `houserules` as every install has it,
/// not the build output of this repository (design.md 222). A capture that
/// `--force` replaces needs the `sha256` of its run removed, because
/// `report-facts` fails on the recorded hash. An excerpt comes from the output
/// of the capture, not from its header line.
#[test]
fn the_implementer_runs_every_evidence_command_through_capture() {
    assert_holds(
        IMPLEMENTER,
        &[
            "Run every test, gate, live run, RED, and GREEN through `houserules capture --out <workspace>/<name>.txt -- <command>`.",
            "A command with a variable prefix runs through `env` (`-- env HOUSERULES_SKIP_SELF_UPDATE=1 houserules check-knowledge`); a pipe runs through `sh -c` (`-- sh -c '<pipeline>'`).",
            "Name captures without the `task-` prefix.",
            "Write the run as `{ \"capture\": \"<path>\" }`; quote at most one short excerpt that proves the claim, copied from the output of the capture (the file after its header line).",
            "A report holds no command output.",
            "`houserules report-facts` fills the `sha256`, the `command`, and the `exit` of each run from its capture, so never type them.",
            "`capture` stops when its `--out` file exists: name a new capture for a new run, or pass `--force` to replace the old one.",
            "After `--force` replaces the capture of a run whose `sha256` `report-facts` recorded, `report-facts` fails on that `sha256`: re-read the new capture, remove the `sha256` of that run, and run `report-facts` again.",
        ],
    );
    assert_lacks(
        IMPLEMENTER,
        &[
            "./target/debug/houserules",
            "Keep the verbatim RED and GREEN output for the report.",
            "`output` carries the command's stdout and stderr verbatim",
            "Never append an echo of the exit status",
            "captured as `<command> > <file> 2>&1`",
            "with its verbatim `output`",
            "each with the verbatim `command` and `output`",
            "Capture each RED with `> <file> 2>&1`",
            "copied from the capture file",
        ],
    );
}

/// Spec 7 and 6 (check 3): the `tests`, `live_run`, and `tdd` fields name runs,
/// and the `natural` label rests on the header of the RED capture.
#[test]
fn the_implementer_describes_the_run_fields_and_the_natural_label_by_capture() {
    assert_holds(
        IMPLEMENTER,
        &[
            "Each run of `tests`, `live_run`, `tdd[].red`, `tdd[].green`, and `fix_rounds[].tests[]` is `{ \"capture\": \"<path>\" }`, with an optional `excerpt`.",
            "`commits` and `files_changed` (`report-facts` writes both from git",
            "Capture each RED with `houserules capture` into the batch workspace; `houserules check-report-claims` fails a `natural` entry whose RED capture ended after the newest commit the report lists, and one whose RED capture exited 0.",
            "one command for each capture, so that the recorded `exit` is that command's own, with no `;` or `|` tail",
        ],
    );
}

/// Spec 7: a claim rests on a recorded citation, a self-review item carries
/// its evidence, and a fix round names the probe it re-ran.
#[test]
fn the_implementer_records_citations_self_review_evidence_and_reran_probes() {
    assert_holds(
        IMPLEMENTER,
        &[
            "For each `file:line` that a claim rests on, add `{ \"path\", \"line\", \"anchor\" }` (and `end` for a range) to `citations`; the anchor is the text on the cited lines that the claim rests on, copied from the file at HEAD.",
            "Write each `self_review` item as `{ \"claim\", \"evidence\" }`; a claim that says every, all, or no remaining names the capture of its enumeration; a claim that a test pins a behavior names the test file.",
            "A fix round names the review's probe it re-ran in each finding's `reran`.",
        ],
    );
}

/// Spec 7: the draft rule governs every write of the report, the closing act's
/// included; `report-facts` and `validate` run on the temp copy.
#[test]
fn every_write_of_the_report_goes_through_a_temp_copy_with_report_facts_and_validate() {
    assert_holds(
        IMPLEMENTER,
        &[
            "Edit a report by writing the new content to a temp copy, running `houserules report-facts <copy> --base <BASE>` and `houserules validate <copy>` on the copy, and moving it into place only when validation passes; never edit REPORT_FILE in place.",
            "This rule governs every write of `REPORT_FILE`, the closing act's included.",
            "While `self_audit` is still `null`, the only error `validate` lets pass is `needs a non-null self_audit`.",
        ],
    );
}

/// Spec 7: the closing act has four moves, in order.
#[test]
fn the_closing_act_runs_its_four_moves_in_the_order_of_the_spec() {
    let text = flat(&package_text(IMPLEMENTER));
    let moves = [
        "First, run step 4 again.",
        "Second, run the self-audit of step 1 through `houserules capture` with `--json <workspace>/<name>.json`.",
        "Third, write the report to a temp copy, run `houserules report-facts <copy> --base <BASE> --audit <that file>` (with `--fix-base <FIX_BASE>` in a fix round) and `houserules validate <copy>` on it, and move it into place.",
        "Fourth, run `houserules check-report-claims <REPORT_FILE>` through `houserules capture`, add it as the last `tests` entry of a new temp copy, run `report-facts` and `validate` on that copy once more, and move it into place, so that the last run gets its facts.",
    ];
    let mut from = 0;
    for step in moves {
        let at = text[from..]
            .find(step)
            .unwrap_or_else(|| panic!("the closing act lacks, or misorders: {step}"));
        from += at + step.len();
    }
    assert_holds(
        IMPLEMENTER,
        &[
            "Re-open the citations: after the last edit, list every citation in the report, recorded in `citations` and free-prose `file:line` alike, and re-open each cited line at HEAD.",
            "Never retype a `command` of a run; `report-facts` copies it from the capture.",
        ],
    );
}

/// Spec 7: the task reviewer runs `check-report-claims` against a checkout at
/// the task head, so that the check of recorded citations reads the task's own
/// HEAD, and files each finding.
#[test]
fn the_task_reviewer_runs_check_report_claims_against_a_checkout_at_the_task_head() {
    assert_holds(
        TASK_REVIEWER,
        &[
            "Run `houserules check-report-claims --dir <a checkout at the task head> <REPORT_FILE>` into a file: the task's worktree when it still exists, else a detached worktree at `HEAD`, so that the check of recorded citations reads the task's own HEAD. File each finding.",
        ],
    );
}

/// Spec 7: the `dependency-add` scenario expects a capture of the RED and of
/// the GREEN run in place of the verbatim output.
#[test]
fn the_dependency_add_scenario_expects_a_capture_of_the_red_and_the_green_run() {
    let scenario: Value =
        serde_json::from_str(&package_text(DEPENDENCY_ADD)).expect("dependency-add.json is JSON");
    let expected: Vec<&str> = scenario["expected_behavior"]
        .as_array()
        .expect("expected_behavior is an array")
        .iter()
        .map(|line| line.as_str().expect("a line"))
        .collect();
    assert!(
        expected.contains(
            &"Writes the failing test first; tdd cites a capture of the RED and of the GREEN run"
        ),
        "{expected:?}"
    );
    assert!(
        !expected
            .iter()
            .any(|line| line.contains("verbatim RED and GREEN output")),
        "{expected:?}"
    );
}

/// Spec 8.6 (1): a gate command that the harness refuses to an agent is a
/// controller-run integration gate. The orchestrating skill and the planning
/// skill say so in the same sentence.
#[test]
fn the_orchestrating_and_planning_skills_hand_a_refused_gate_to_the_controller() {
    for skill in [ORCHESTRATING, PLANNING] {
        assert_holds(
            skill,
            &[
                "A gate command that the harness refuses to an agent is a controller-run integration gate: the plan lists it, and you run it after each integration and record it in the ledger.",
            ],
        );
    }
}

/// Spec 8.6 (2): the branch reviewer compares the `reversals` count of each
/// workspace with the `Reversal:` lines of its ledger.
#[test]
fn the_branch_reviewer_compares_the_reversals_count_with_the_ledger() {
    assert_holds(
        BRANCH_REVIEWER,
        &[
            "Compare the `reversals` count of each workspace in `stats.json` with the `Reversal:` lines of that workspace's ledger, including the lines inside a longer ledger entry.",
        ],
    );
}

/// Spec 8.6 (3): a printed command or line that changes after the live run
/// runs again before the branch review. The sentence stands in the Verify row.
#[test]
fn the_verify_phase_runs_a_changed_printed_line_again_before_the_branch_review() {
    let text = package_text(ORCHESTRATING);
    let verify = text
        .lines()
        .find(|line| line.starts_with("| Verify |"))
        .expect("the lifecycle table has a Verify row");
    assert!(
        verify.contains(
            "a printed command or line that changes after the live run runs again before the branch review"
        ),
        "{verify}"
    );
    assert!(verify.contains("`process.live-run-before-ci`"), "{verify}");
}

/// Spec 8.6 (5): library research names each sub-problem of the design and
/// researches a library for each.
#[test]
fn the_designing_skill_researches_a_library_for_each_sub_problem() {
    assert_holds(
        DESIGNING,
        &[
            "The library research: it names each sub-problem of the design (a URI resolver inside a SARIF reader, an atomic file replace inside a writer) and researches a library for each.",
        ],
    );
}

/// Spec 7 and 10: three bodies and one summary of the `process` pack describe
/// captures in place of inline output, redirects, and truncation markers. The
/// pointer to the limits of the checker names its submodules.
#[test]
fn the_process_pack_teaches_captures_in_place_of_inline_output() {
    let pack = process_pack();

    let claims = body_text(pack_entry(&pack, "process.claims-match-artifacts"));
    for phrase in [
        "The `command` and the `exit` of a run come from the header of its capture, and `houserules report-facts` copies them into the report: never retype either",
        "A run with no capture, or a `command` or `exit` that differs from its capture, fails `houserules check-report-claims`; a reviewer re-runs the command from the header.",
        "through `houserules capture` after the last edit, and add that clean run as the last `tests` entry",
        "its Limits lists (the module docs of `crates/houserules/src/report_claims/`, indexed in `crates/houserules/src/report_claims.rs`, reachable only where this kit's own source lives)",
    ] {
        assert!(
            claims.contains(phrase),
            "claims-match-artifacts lacks: {phrase}"
        );
    }
    for phrase in [
        "paste-run",
        "paste its clean run",
        "(crates/houserules/src/report_claims.rs, reachable only where this kit's own source lives)",
    ] {
        assert!(
            !claims.contains(phrase),
            "claims-match-artifacts holds: {phrase}"
        );
    }

    let outlives = body_text(pack_entry(&pack, "process.evidence-outlives-the-session"));
    for phrase in [
        "Write each capture into the deliverables folder, beside the document that cites it.",
        "A task report cites each capture by its path and its SHA-256 hash, and holds at most one short excerpt of its output.",
        "The excerpt is a byte-exact part of the output of the capture, so the retained capture is the evidence",
        "`houserules check-report-claims` fails a capture that is missing or changed and an excerpt that is not part of the output of its capture.",
    ] {
        assert!(
            outlives.contains(phrase),
            "evidence-outlives-the-session lacks: {phrase}"
        );
    }
    for phrase in [
        "Truncation markers count as citations",
        "byte-prefix match",
        "Copy small captures into the deliverables folder",
        "quoted inline",
        "byte-exact part of the capture",
        "not part of its capture",
    ] {
        assert!(
            !outlives.contains(phrase),
            "evidence-outlives-the-session holds: {phrase}"
        );
    }

    let verbatim = pack_entry(&pack, "process.report-evidence-verbatim");
    assert_eq!(
        verbatim["summary"],
        "A deliverable cites only real captures of command output; a re-review reproduces at least one RED or audit before accepting a fix round."
    );
    let verbatim_body = body_text(verbatim);
    assert!(
        verbatim_body.contains(
            "A changed or missing capture, an edited `command` or `exit`, and an excerpt that is not part of the output of its capture fail `houserules check-report-claims`."
        ),
        "{verbatim_body}"
    );
    assert!(
        !verbatim_body.contains("not part of its capture"),
        "{verbatim_body}"
    );
    assert!(
        !verbatim_body.contains("never by a gate"),
        "{verbatim_body}"
    );
}

/// A pin: `process.tdd` keeps its summary. A capture is verbatim, so "verbatim
/// RED and GREEN" stays true, and the pack text of the rule does not change.
#[test]
fn the_process_pack_keeps_the_summary_of_the_tdd_rule() {
    let pack = process_pack();
    assert_eq!(
        pack_entry(&pack, "process.tdd")["summary"],
        "Test-driven development for every executable change: the failing test first, or a disclosed-mutation proof for already-correct behavior; verbatim RED and GREEN."
    );
}

/// The deliverables schema, below `template/` and below the install root.
const DELIVERABLES_SCHEMA: &str = ".claude/schemas/deliverables.json";

/// The description of `issue.backlog` in both copies of the deliverables
/// schema (HR-241 (1), spec 11.1).
const ISSUE_BACKLOG_DESCRIPTION: &str = "The id of the filed deferral that holds this finding; set it only for a finding that the fix round defers (process.no-tech-debt).";

/// The sentence that `task-reviewer.md` and `branch-reviewer.md` carry for the
/// `backlog` field of an issue (HR-241 (1), spec 11.1).
const ISSUE_BACKLOG_SENTENCE: &str = "An issue's `backlog` holds the id of the filed deferral that holds the finding. Leave it out of every issue you file; the controller sets it when the fix round defers the finding.";

/// The first act of a dispatch into a worktree (HR-241 (2), spec 11.2).
const WORKTREE_FIRST_ACT: &str = "Open the brief of a dispatch into a worktree with a first act. The implementer runs `git merge-base --is-ancestor <BASE> HEAD` in its worktree. When the command exits 0, the worktree holds `<BASE>`, and the implementer goes on. When the command exits 1, the implementer runs `git reset --hard <BASE>` and records the reset in `concerns`. When the command exits with any other status, the implementer stops and reports BLOCKED with the output. In a fix round, `<BASE>` is `FIX_BASE`. Leave the first act out of a dispatch into a shared checkout, where a reset discards work.";

/// The capture rule of a controller-run integration gate (HR-241 (3), spec
/// 11.3).
const INTEGRATION_GATE_CAPTURE: &str = "Run each controller-run integration gate through `houserules capture`, and put any injected configuration in the argv, so that the capture shows it: `houserules capture --out <workspace>/<name>.txt -- env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.autocrlf GIT_CONFIG_VALUE_0=true <gate command>`.";

/// The test command of an eval scenario brief (HR-241 (4), spec 11.4).
const EVAL_BRIEF_TEST_COMMAND: &str = "Each scenario brief names the command that runs the suite with the project's pinned tools: the test task of the project's task runner. When the project has no such task, the brief names the test command of the plan's gate list, or, in a batch with no plan, the test command that the project documents.";

/// The revision note of a plan that a decision makes false (HR-241 (5), spec
/// 11.5).
const PLAN_REVISION_NOTE: &str = "A decision that makes a sentence of the plan false adds a revision note to the plan in the same turn. At batch close, re-read the declared file set of each task (its Files line) and the plan's Global constraints, when it has them, against the tree. Add a revision note for each sentence that no longer holds.";

/// The plan re-read in the Batch close section of the orchestrating skill
/// (HR-241 (5), spec 11.5).
const BATCH_CLOSE_PLAN_REREAD: &str = "Before the branch review dispatches, or before Finish in a batch without one, re-read the declared file set of each task (its Files line) and the plan's Global constraints, when it has them, against the tree. Add a revision note to the plan for each sentence that no longer holds (the `planning` skill, \"Keep the plan true\"). A batch with no plan has nothing to re-read.";

/// The same-turn revision note in the Rulings section of the orchestrating
/// skill (HR-241 (5), spec 11.5).
const RULINGS_PLAN_REVISION_NOTE: &str = "A decision that makes a sentence of the plan false adds a revision note to the plan in the same turn (the `planning` skill, \"Keep the plan true\").";

/// The text of the section of the package file `file` that opens with the
/// heading line `heading`, up to the next `## ` heading, with every run of
/// white space collapsed.
fn section_text(file: &str, heading: &str) -> String {
    let text = package_text(file);
    let mut lines = text.lines().skip_while(|line| *line != heading);
    assert!(lines.next().is_some(), "{file} has no heading {heading}");
    let body: Vec<&str> = lines.take_while(|line| !line.starts_with("## ")).collect();
    flat(&body.join("\n"))
}

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` switched off.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// Both copies of the deliverables schema, parsed, as `(copy, schema)` pairs:
/// the source below `template/` and the installed copy at the repository root.
fn schema_copies() -> [(&'static str, Value); 2] {
    let installed = repo_root().join(DELIVERABLES_SCHEMA);
    let installed_text = fs::read_to_string(&installed)
        .unwrap_or_else(|error| panic!("read {}: {error}", installed.display()));
    [
        ("template", core_text(DELIVERABLES_SCHEMA)),
        ("root", installed_text),
    ]
    .map(|(copy, text)| {
        let schema = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("the {copy} schema is JSON: {error}"));
        (copy, schema)
    })
}

/// Spec 11.1 (HR-241 (1)): `issue.backlog` holds its `$ref` and the
/// description of the item, and no other keyword, in both copies of the
/// deliverables schema. A description is an annotation, so the property keeps
/// every constraint it had.
#[test]
fn issue_backlog_carries_its_description_beside_its_ref_in_both_schema_copies() {
    for (copy, schema) in schema_copies() {
        assert_eq!(
            schema["$defs"]["issue"]["properties"]["backlog"],
            json!({
                "$ref": "#/$defs/backlogId",
                "description": ISSUE_BACKLOG_DESCRIPTION,
            }),
            "the {copy} schema"
        );
    }
}

/// A pin: the committed review of batch 14 whose first issue names
/// `backlog: HR-050` validates against this repository's schema once it names
/// a model, and an id outside the pattern of `backlogId` still fails at that
/// issue. The description of `issue.backlog` changes no verdict of `validate`.
#[test]
fn a_review_issue_with_a_backlog_id_validates_and_a_malformed_id_still_fails() {
    let root = repo_root();
    let fixture = root
        .join("tests")
        .join("fixtures")
        .join("batch14-workspace")
        .join("task-2-review.json");
    let mut review: Value = serde_json::from_str(
        &fs::read_to_string(&fixture)
            .unwrap_or_else(|error| panic!("read {}: {error}", fixture.display())),
    )
    .expect("the fixture review is JSON");
    review["model"] = json!("opus");
    assert_eq!(
        review["issues"][0]["backlog"],
        json!("HR-050"),
        "the fixture's first issue names its deferral"
    );
    let scratch = tempfile::tempdir().expect("tempdir");
    let validate = |name: &str, review: &Value| {
        let path = scratch.path().join(name);
        fs::write(&path, review.to_string()).expect("write the review");
        houserules()
            .arg("validate")
            .arg(&path)
            .arg("--dir")
            .arg(&root)
            .output()
            .expect("run validate")
    };

    let valid = validate("valid.json", &review);
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stdout)
    );

    review["issues"][0]["backlog"] = json!("not valid!");
    let malformed = validate("malformed.json", &review);
    assert_eq!(malformed.status.code(), Some(1));
    let printed = String::from_utf8_lossy(&malformed.stdout);
    assert!(
        printed.contains(".issues[0].backlog: must match "),
        "{printed}"
    );
}

/// Spec 11.1 (HR-241 (1)): the task reviewer and the branch reviewer each tell
/// the reviewer to leave `backlog` out of every issue it files, and name the
/// controller as the actor that sets it when the fix round defers the finding.
#[test]
fn the_reviewer_templates_say_when_an_issue_names_its_deferral() {
    for file in [TASK_REVIEWER, BRANCH_REVIEWER] {
        assert_holds(file, &[ISSUE_BACKLOG_SENTENCE]);
    }
}

/// Spec 11.2 (HR-241 (2)): the orchestrating skill gives a dispatch into a
/// worktree the first act, with one arm for each exit status of the check: 0
/// goes on, 1 resets the worktree to `BASE`, and any other status reports
/// BLOCKED.
#[test]
fn the_orchestrating_skill_opens_a_worktree_dispatch_with_the_base_check() {
    assert_holds(ORCHESTRATING, &[WORKTREE_FIRST_ACT]);
}

/// Spec 11.3 (HR-241 (3)): the orchestrating skill runs each controller-run
/// integration gate through `houserules capture`, with the injected
/// configuration in the argv.
#[test]
fn the_orchestrating_skill_captures_each_integration_gate_with_its_configuration() {
    assert_holds(ORCHESTRATING, &[INTEGRATION_GATE_CAPTURE]);
}

/// Spec 11.4 (HR-241 (4)): the eval-run step of the orchestrating skill makes
/// each scenario brief name the command that runs the suite with the project's
/// pinned tools, and names the command of the plan or of the project's
/// documents for a project with no such task.
#[test]
fn the_eval_run_briefs_name_the_command_that_runs_the_suite_with_the_pinned_tools() {
    assert_holds(ORCHESTRATING, &[EVAL_BRIEF_TEST_COMMAND]);
}

/// Spec 11.5 (HR-241 (5)): the planning skill adds a revision note for a
/// decision that makes a plan sentence false, and re-reads the plan at batch
/// close.
#[test]
fn the_planning_skill_keeps_the_plan_true_with_revision_notes() {
    assert_holds(PLANNING, &[PLAN_REVISION_NOTE]);
}

/// Spec 11.5 (HR-241 (5)): the orchestrating skill, which the controller
/// follows at batch close and when it rules, carries the plan re-read in its
/// Batch close section and the same-turn revision note in its Rulings section.
#[test]
fn the_orchestrating_skill_keeps_the_plan_true_at_batch_close_and_in_rulings() {
    for (heading, sentence) in [
        ("## Batch close", BATCH_CLOSE_PLAN_REREAD),
        ("## Rulings", RULINGS_PLAN_REVISION_NOTE),
    ] {
        assert!(
            section_text(ORCHESTRATING, heading).contains(sentence),
            "the section {heading} lacks: {sentence}"
        );
    }
}

// ---- Batch 37: the per-change review layer in the kit texts ---------------------------
//
// Spec 7.4 (the seed rule), 7.5 (the finishing skill), 7.6 (the
// migrating-knowledge skill and the upgrade guide), 10.7 (what `init` writes),
// case 9 (the process pack's minor version), and case 10 (the guide's parts).
// The wording gate of spec 3 lives in `host_wording.rs`.

/// The migrating-knowledge skill of the core kit, below `template/`.
const MIGRATING_KNOWLEDGE: &str = ".claude/skills/migrating-knowledge/SKILL.md";

/// The upgrade guide.
const UPGRADE_GUIDE: &str = "docs/upgrading-to-2.md";

/// The summary of the seed rule of the evidence bundle, as spec 7.4 gives it.
/// The knowledge schema caps a summary at 160 characters.
const SEED_RULE_SUMMARY: &str = "A pull or merge request carries an evidence bundle before review: purpose, work items, tier, criterion, and each run at its head; bundle check enforces it.";

/// The body of `process.risk-tiers` as the process pack holds it (ruling 261 and the task-11 ruling Q1).
const RISK_TIERS_BODY: [&str; 8] = [
    "The tier sets the pipeline's depth. Light: one task; docs, comments, config values, or a dependency bump only; no executable behavior change; no task is critical. The work item's body is the spec, and the project's owner or decider still approves it. The pipeline is an author of the change, one task review, and the live run. It has no plan document and no branch review.",
    "Standard: every change that is not light. The pipeline is a short spec with the decision gate, a plan, an author of the change, one task review, at most one re-review, and the live run. A branch review runs only for a batch of two or more tasks.",
    "A critical task is a task whose change alters the commands, flags, or output of a command-line interface, a schema, authentication or security code, or the data that an installed product keeps on disk. It runs inside the standard tier with its own evidence: the plan writes the spec's cases for the task as tests before any code; the author of the change commits them first with one recorded run that shows them failing at the base commit, then makes them pass; the evidence is that run and the whole suite passing at the head (`process.tdd`). The whole-suite half is the integration run of the person who runs the work. For a task that runs in the shared checkout, that run is the one at the close of the task.",
    "A critical task gets one task review. The person who runs the work checks each fix by re-running the reviewer's own reproduction and records the check (`process.fix-round-verification-record`). A Critical or Important finding goes to a fresh author of the change; a Minor finding is fixed directly. A fix round that fixes a Critical or Important finding, or that changes files beyond the files its findings name, then gets one review of its fix diff. That review files only new breakage. Only a new Critical or Important finding of it opens a further round, which closes by the check of the person who runs the work; no second fix-diff review runs.",
    "A lighter tier drops only what its pipeline names: the light tier drops the plan document and the branch review, and the standard tier drops the branch review for a one-task batch. Every tier keeps item selection, the live run, the finish, and rollout and acceptance.",
    "One tier covers the batch: the highest tier that any of its changes needs. A critical task follows its own evidence rule inside the tier of the batch.",
    "The plan records the tier and the criterion that selected it. For a light change, the task brief records them. The task review checks the tier, and a wrong tier is a finding.",
    "A tier moves up during a batch, never down. `process.bounded-review-loop` states the one automatic move.",
];

/// The `source` of `process.risk-tiers` as the process pack holds it (ruling 261).
const RISK_TIERS_SOURCE: (&str, &str, &str) = ("2026-10-25", "user", "design.md 260");

/// The body of `process.bounded-review-loop` as the process pack holds it (rulings 261 and 281).
const BOUNDED_REVIEW_LOOP_BODY: [&str; 6] = [
    "A fix round that holds only Minor fixes closes with a verification note (`process.fix-round-verification-record`), never with a re-review. The person who runs the work runs the check that each Minor finding names and cites its result in the note.",
    "A re-review reads the fix diff and the original findings only. It files new findings at their impact (`process.severity-by-impact`). Only a new Critical or Important finding opens a further round. A new Minor finding joins a Minor-only round.",
    "Re-reviews per task: light 0, standard 1. A critical task gets no re-review of its original findings; it gets one fix-diff review after a fix round that fixes a Critical or Important finding or changes files beyond its findings' files (`process.risk-tiers`). A branch-review fix wave has the same cap as the batch's tier.",
    "While a fix-diff review or a re-review runs, the person who runs the work may integrate the fix and run the integration gates. When the review files a new Critical or Important finding, the person who runs the work undoes the integration (the batch branch returns to the commit before it) and the task opens its next round.",
    "At the cap, the open non-critical findings become one work item with the reason (a deferral under `process.no-tech-debt`). An open Critical finding goes to the project's owner or decider.",
    "A light task with an Important or Critical finding moves up to the standard tier and gets its one re-review.",
];

/// The `source` of `process.bounded-review-loop` as the process pack holds it (ruling 261).
const BOUNDED_REVIEW_LOOP_SOURCE: (&str, &str, &str) = ("2026-10-25", "user", "design.md 260");

/// The longest summary that the knowledge schema allows.
fn summary_limit() -> u64 {
    let schema: Value = serde_json::from_str(&help_support::read("knowledge/schema.json"))
        .expect("the knowledge schema is JSON");
    schema
        .pointer("/$defs/entry/properties/summary/maxLength")
        .and_then(Value::as_u64)
        .expect("the knowledge schema limits the length of a summary")
}

/// The pack `name` of the kit, parsed.
fn kit_pack(name: &str) -> Value {
    let text = help_support::read(&format!("template/packs/{name}.json"));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("the {name} pack is JSON: {error}"))
}

/// Spec 7.4: the process pack carries `process.evidence-before-review`, a rule
/// that is not standing, in the area `process`, with no `check` (the gate is
/// the CI step). Its body names `bundle write`, `houserules ci print`, the
/// budget with a reason, and the exempt authors and branches.
#[test]
fn the_process_pack_carries_the_seed_rule_of_the_evidence_bundle() {
    let pack = process_pack();
    let entry = pack_entry(&pack, "process.evidence-before-review");
    assert_eq!(entry["kind"], "rule");
    assert_eq!(entry["area"], "process");
    assert!(
        entry
            .get("standing")
            .is_none_or(|standing| standing == &json!(false)),
        "the seed rule is not standing"
    );
    assert!(entry.get("check").is_none(), "the gate is the CI step");
    assert_eq!(entry["summary"], SEED_RULE_SUMMARY);
    let length = SEED_RULE_SUMMARY.chars().count() as u64;
    assert!(
        length <= summary_limit(),
        "the summary has {length} characters; the knowledge schema allows {}",
        summary_limit()
    );
    let body = body_text(entry);
    for phrase in [
        "`houserules bundle write`",
        "`houserules ci print`",
        "`houserules bundle check`",
        "`Budget reason:` line",
        "`review.bundle.budgets`",
        "`review.bundle.exemptAuthors`",
        "`review.bundle.exemptBranches`",
        "The entry has no `check`.",
    ] {
        assert!(body.contains(phrase), "the seed rule lacks: {phrase}");
    }
}

/// Ruling 261 (design.md): `process.risk-tiers` and `process.bounded-review-loop`
/// in the process pack hold the body and the source of the two overrides of
/// ruling 260, so that an adopter reads the fix-diff review of a critical task.
#[test]
fn the_process_pack_ships_the_fix_diff_review_of_ruling_260() {
    let pack = process_pack();
    for (id, body, source) in [
        (
            "process.risk-tiers",
            RISK_TIERS_BODY.as_slice(),
            RISK_TIERS_SOURCE,
        ),
        (
            "process.bounded-review-loop",
            BOUNDED_REVIEW_LOOP_BODY.as_slice(),
            BOUNDED_REVIEW_LOOP_SOURCE,
        ),
    ] {
        let entry = pack_entry(&pack, id);
        assert_eq!(entry["body"], json!(body), "{id}");
        assert_eq!(
            entry["source"],
            json!({"date": source.0, "by": source.1, "ref": source.2}),
            "{id}"
        );
    }
}

/// Case 9 and case 14: the process pack moved one minor version for the added
/// entry and the two changed bodies, and then one patch version for the faster
/// loop (spec 19.9: a wording change); the quality pack moves one patch version
/// for the sentence of 19.8; the writing-style pack moved one patch version for
/// the changed wording of the README funnel entry.
#[test]
fn the_kit_packs_moved_by_the_class_of_their_change() {
    assert_eq!(kit_pack("process")["version"], "2.1.1");
    assert_eq!(kit_pack("quality")["version"], "1.0.3");
    assert_eq!(kit_pack("writing-style")["version"], "1.0.2");
}

/// Spec 3: the README funnel entry asks whether "pull or merge requests" are
/// accepted.
#[test]
fn the_readme_funnel_entry_says_pull_or_merge_requests() {
    let pack = kit_pack("writing-style");
    let entry = pack_entry(&pack, "writing-style.readme-is-an-adopter-funnel");
    let body = body_text(entry);
    assert!(
        body.contains("whether pull or merge requests are accepted"),
        "{body}"
    );
    assert!(!body.contains("whether pull requests are accepted"));
}

/// Spec 7.5: the finishing skill writes the bundle with `bundle write` from the
/// captures of the final gate run before it opens the request, and opens the
/// request with the CLI of the host. The skill names the check that CI runs on
/// the description.
#[test]
fn the_finishing_skill_writes_the_bundle_and_opens_the_request_with_the_hosts_cli() {
    assert_holds(
        FINISHING,
        &[
            "Write the evidence bundle, push, and open the pull or merge request.",
            "`houserules bundle write`",
            "the intent (`--intent <source>:<ref>`) or the purpose (`--purpose`), from the spec",
            "the work items of the batch, one `--work-item <id>` for each",
            "the tier and the criterion, from the plan (`--tier`, `--criterion`)",
            "the captures of the final gate run at the head, one `--capture <file>` for each",
            "gh pr create --title",
            "--body-file",
            "glab mr create --title",
            "--description",
            "houserules bundle check --body",
        ],
    );
    assert_lacks(
        FINISHING,
        &["gh pr create --fill", "Push and open a PR.", "pull request"],
    );
}

/// Spec 7.5: the orchestrating skill names the bundle step in the Finish row of
/// its lifecycle table.
#[test]
fn the_orchestrating_skill_names_the_bundle_step_in_the_finish_row() {
    let finish: Vec<String> = package_text(ORCHESTRATING)
        .lines()
        .filter(|line| line.starts_with("| Finish |"))
        .map(flat)
        .collect();
    assert_eq!(finish.len(), 1, "{finish:?}");
    for phrase in [
        "the evidence bundle (`houserules bundle write`)",
        "pull or merge request",
    ] {
        assert!(finish[0].contains(phrase), "the Finish row lacks: {phrase}");
    }
}

/// Ruling 261: the orchestrating skill states the fix-diff review of a critical
/// task that the process pack now ships, in the critical-task list and in the
/// review-loop rule, and no longer says that a critical task never gets a
/// re-review of any kind.
#[test]
fn the_orchestrating_skill_states_the_fix_diff_review_of_a_critical_task() {
    assert_holds(
        ORCHESTRATING,
        &[
            "one task review on the task reviewer's configured model (default opus). It gets no re-review of its original findings.",
            "A fix round that fixes a Critical or Important finding, or that changes files beyond the files its findings name, then gets one review of its fix diff.",
            "That review files only new breakage.",
            "A critical task gets no re-review of its original findings, and one review of its fix diff",
        ],
    );
    assert_lacks(
        ORCHESTRATING,
        &[
            "(default opus), and never a re-review",
            "A critical task gets none.",
        ],
    );
}

/// The settings of the review layer, the limits, and the three ways of wiring
/// that spec 7.6 lists. Each row is a part with the phrases that carry it; the
/// migrating-knowledge skill and the upgrade guide each hold every part.
const REVIEW_LAYER_PARTS: [(&str, &[&str]); 11] = [
    (
        "the output of bundle check",
        &[
            "`houserules bundle check --body <file> --base <ref> --head <ref>`",
            "0 for a clean bundle, 1 when it prints a finding, 2 for a setup error",
        ],
    ),
    (
        "the output of evidence",
        &["`houserules evidence --base <ref> --out <file>`"],
    ),
    (
        "the output of route",
        &[
            "`houserules route --base <ref> --json <file>`",
            "`houserules route --base <ref> --no-audit --json <file>`",
        ],
    ),
    (
        "the record and the notes ref",
        &[
            "`houserules record write --base <ref> --head <ref> --out <file>`",
            "`refs/notes/houserules`",
            "`git fetch origin refs/notes/houserules:refs/notes/houserules`",
        ],
    ),
    (
        "the three ways of wiring",
        &[
            "**Reference.**",
            "**Inline.**",
            "**A file of its own.**",
            "`houserules ci print`",
            "`houserules ci write`",
            "`--mode inline`",
            "`houserules ci write --host github --sha <sha>`",
            "`houserules ci write --host gitlab`",
            "`--host github --mode inline` needs no `--sha`",
        ],
    ),
    (
        "the GitLab token and its reach",
        &[
            "`HOUSERULES_TOKEN`",
            "a personal access token of a dedicated member",
            "reaches every project that its user can reach",
        ],
    ),
    (
        "the expiry of the GitLab token",
        &["at most 365 days", "rotate it"],
    ),
    (
        "the limit of the sample",
        &["unsteerable only under a host merge that writes a new commit"],
    ),
    (
        "the condition of the runner floor",
        &[
            "`floor-install: runner:<path>`",
            "fresh runner or an unwritable path",
        ],
    ),
    (
        "the limit of a request that edits its own wiring",
        &[
            "sets its own status and label",
            "a required status from a protected-branch rule",
            "the rule-source floor",
            "required code-owner review",
        ],
    ),
    (
        "the limits of the two hosts",
        &[
            "`workflow_run`",
            "not to `labeled`",
            "a description edit starts no pipeline",
            "`setup` input",
        ],
    ),
];

/// The parts of `REVIEW_LAYER_PARTS` that `text` lacks, as `part: phrase` lines.
fn missing_parts(text: &str) -> Vec<String> {
    let mut missing = Vec::new();
    for (part, phrases) in REVIEW_LAYER_PARTS {
        for phrase in phrases {
            if !text.contains(phrase) {
                missing.push(format!("{part}: {phrase}"));
            }
        }
    }
    missing
}

/// Spec 7.6: the migrating-knowledge skill teaches each part of the review
/// layer, asks the owner for the settings, and asks which file of the project's
/// own pipeline wires houserules.
#[test]
fn the_migrating_knowledge_skill_teaches_each_part_of_the_review_layer() {
    let skill = flat(&core_text(MIGRATING_KNOWLEDGE));
    let missing = missing_parts(&skill);
    assert!(
        missing.is_empty(),
        "the skill lacks {} phrase(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
    for phrase in [
        "`review.bundle.budgets`",
        "`review.route.sampleRate`",
        "`review.bundle.exemptAuthors`",
        "`review.bundle.exemptBranches`",
        "`review.intent.sources`",
        "`review.intent.files`",
        "which checks declare `covers: true`",
        "Ask which file of the project's own pipeline wires houserules",
        "`review.ci.files`",
    ] {
        assert!(skill.contains(phrase), "the skill lacks: {phrase}");
    }
}

/// Spec 10.7: `init` writes no CI file, so the skill no longer says that it
/// seeds one for a GitHub-hosted origin, and it points at `houserules ci print`.
#[test]
fn the_migrating_knowledge_skill_says_that_init_writes_no_ci_file() {
    let skill = flat(&core_text(MIGRATING_KNOWLEDGE));
    for phrase in [
        "`init` writes no CI file, and `update` writes none again.",
        "`houserules ci print --host github`",
    ] {
        assert!(skill.contains(phrase), "the skill lacks: {phrase}");
    }
    for stale in [
        "only for a GitHub-hosted",
        "The seeded GitHub workflow does this with",
        "the seeded workflow runs",
    ] {
        assert!(!skill.contains(stale), "the skill still holds: {stale}");
    }
}

/// Spec 7.6: the batch-37 section of the upgrade guide holds the same parts, the
/// rows of the new keys and schema ids, the reds of spec 13 with their remedies,
/// and what `init` and `update` write now.
#[test]
fn the_upgrade_guide_holds_each_part_of_the_review_layer() {
    let guide = folded_text(UPGRADE_GUIDE);
    let missing = missing_parts(&guide);
    assert!(
        missing.is_empty(),
        "the guide lacks {} phrase(s):\n{}",
        missing.len(),
        missing.join("\n")
    );
    let raw = help_support::read(UPGRADE_GUIDE);
    for heading in [
        "## The review layer",
        "### Changes of install schema 2.3.0",
        "### The reds of the review layer by design",
    ] {
        assert!(raw.lines().any(|line| line == heading), "no {heading}");
    }
    for phrase in [
        "| `review` |",
        "| `reviewLayerOffered` |",
        "| `covers` |",
        "| `houserules/pack@1.3.0` |",
        "| `houserules/overrides@1.2.0` |",
        "| `houserules/install@2.3.0` |",
        "`process.evidence-before-review`",
        "its open requests fail `bundle check` until each carries a bundle",
        "`update` prints the opt-in line once",
        "**A fresh install:** no CI file and no red.",
        "`audit.error`",
        "**A check that you meant as coverage:** it is not coverage until its entry declares `covers: true`. No red.",
        "`init` writes no CI file",
    ] {
        assert!(guide.contains(phrase), "the guide lacks: {phrase}");
    }
    omits(
        UPGRADE_GUIDE,
        &guide,
        &[
            "**A fresh install:** A fresh install",
            "**A check that you meant as coverage:** A check that you meant as coverage",
        ],
    );
}

/// The restamp of the install schema: the binary stamps `2.3.0` on a marker of
/// `2.2.0`, and leaves any other schema as it is. The guide states the pair that
/// the binary restamps, and no pair that it does not.
#[test]
fn the_upgrade_guide_names_the_pair_that_update_restamps() {
    let guide = folded_text(UPGRADE_GUIDE);
    for phrase in [
        "`update` stamps `houserules/install@2.3.0` on a marker of `houserules/install@2.2.0` and prints `wrote .houserules.json (houserules/install@2.3.0)` once",
        "It leaves a marker of any other schema as it is, `2.1.0` included.",
    ] {
        assert!(guide.contains(phrase), "the guide lacks: {phrase}");
    }
    assert!(!guide.contains("stamps `houserules/install@2.2.0` on an install of `2.1.0`"));
}

/// The anchor that a Markdown renderer gives the heading line `heading`: lower
/// case, each space a hyphen, and no punctuation but hyphens and underscores.
fn anchor_of(heading: &str) -> String {
    heading
        .trim_start_matches('#')
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// The hand step "a CI workflow of your own" compares a kept workflow with the
/// template of the kit. That file is the seeded workflow of one population only:
/// the step names the form that each other population holds, and each link of
/// the step lands on a heading of the guide.
#[test]
fn the_hand_step_of_a_workflow_of_your_own_names_the_reference_form_of_each_population() {
    let raw = help_support::read(UPGRADE_GUIDE);
    let section = raw
        .split("### Hand step: a CI workflow of your own")
        .nth(1)
        .and_then(|rest| rest.split("\n### ").next())
        .expect("the guide holds the hand step of a workflow of your own");
    states(
        UPGRADE_GUIDE,
        &help_support::folded(section),
        &[
            "That file is the form for an install at the top level of the repository with no tracker or the JSON tracker.",
            "- **The GitHub tracker:** the workflow also holds `issues: read` and the `GH_TOKEN` env.",
            "- **The Beads tracker:** the workflow also holds the steps that install `bd` and bootstrap the store, and it lacks `persist-credentials: false`.",
            "- **An install in a subdirectory:** the file is `.github/workflows/knowledge-<folder>.yml`, where `<folder>` is the path of the install below the top level with each `/` replaced by `-`. The `name` is `knowledge (<install path>)`, each of the two triggers has a `paths` filter on `<install path>/**`, and the job has `working-directory: <install path>` as the default of its steps.",
        ],
    );
    let anchors: Vec<String> = raw
        .lines()
        .filter(|line| line.starts_with('#'))
        .map(anchor_of)
        .collect();
    let links: Vec<&str> = section
        .split("](#")
        .skip(1)
        .filter_map(|rest| rest.split(')').next())
        .collect();
    assert!(
        links.len() >= 2,
        "the step links to the hand steps of the two trackers"
    );
    for link in links {
        assert!(
            anchors.iter().any(|anchor| anchor == link),
            "no heading of the guide has the anchor #{link}"
        );
    }
}

/// Spec 10.3: the guide quotes the first line of a file of `ci write` and the
/// line of `update` as the spec gives them. Each names `ci write --force` with the
/// flags that wrote the file, because a bare `--force` exits 2 for a GitHub
/// reference file, which needs the `--sha` of the new release.
#[test]
fn the_upgrade_guide_quotes_the_rewrite_lines_of_ci_write_with_the_flags_of_the_file() {
    let guide = folded_text(UPGRADE_GUIDE);
    states(
        UPGRADE_GUIDE,
        &guide,
        &[
            "`# Written by houserules ci write; rewrite it with houserules ci write --force and the flags that wrote it.`",
            "`update: <path> pins houserules <old>; this binary is <new>: run houserules ci write --force with the flags that wrote it (a GitHub reference file takes the --sha of v<new>)`",
            "Run the command again with the flags that you gave `ci write` the first time.",
            "For a GitHub reference file, give `--sha` the commit of the tag of the new release.",
        ],
    );
    omits(
        UPGRADE_GUIDE,
        &guide,
        &[
            "rewrite it with houserules ci write --force.`",
            "this binary is <new>: run houserules ci write --force`",
        ],
    );
}

/// The hand step "a CI workflow of your own" runs its diff command from the top
/// level of the repository on the file that the printed line names, `<path>`.
/// The seeded workflow of an install in a subdirectory is not
/// `.github/workflows/knowledge.yml`, so a command that names that file exits 2
/// for it.
#[test]
fn the_diff_command_of_the_hand_step_reads_the_file_that_the_printed_line_names() {
    let raw = help_support::read(UPGRADE_GUIDE);
    let section = raw
        .split("### Hand step: a CI workflow of your own")
        .nth(1)
        .and_then(|rest| rest.split("\n### ").next())
        .expect("the guide holds the hand step of a workflow of your own");
    let folded = help_support::folded(section);
    states(
        UPGRADE_GUIDE,
        &folded,
        &[
            "`<path>` is the workflow file, relative to the top level of the repository",
            "Run the command from the top level of the repository",
            "template/.github/workflows/knowledge.yml | diff <path> -",
        ],
    );
    omits(
        UPGRADE_GUIDE,
        &folded,
        &["diff .github/workflows/knowledge.yml"],
    );
}

/// The record rule teaches the counting rule of `stats` as the code and spec 11.6
/// state it: a reversal starts with `Reversal:` after optional spaces and an
/// optional list marker. The text does not say that `stats` counts only a line
/// that starts with `Reversal:`.
#[test]
fn the_reversal_rule_states_the_counting_rule_of_stats() {
    let pack = process_pack();
    let body = body_text(pack_entry(&pack, "process.reversals-are-recorded"));
    assert!(
        body.contains(
            "`houserules stats` counts a reversal that starts with `Reversal:` after optional spaces and an optional list marker, and the branch reviewer of the reference workflow also counts a `Reversal:` inside a longer entry."
        ),
        "{body}"
    );
    assert!(
        !body.contains("counts only a line that starts with"),
        "{body}"
    );
}

/// The folded text of the repository file `relative`.
fn folded_text(relative: &str) -> String {
    help_support::folded(&help_support::read(relative))
}

/// Spec 10.7 and the carry-over of the CI statements: the upgrade guide no
/// longer says that `init` writes a CI workflow, that a GitHub origin makes it
/// write one, or that the reference install holds one; it names the file of the
/// kit repository that the hand step names, and it states the tail of
/// ruling 261 for a critical task.
#[test]
fn the_upgrade_guide_states_what_init_and_update_write_now() {
    let guide = folded_text(UPGRADE_GUIDE);
    omits(
        UPGRADE_GUIDE,
        &guide,
        &[
            "- `.github/workflows/knowledge.yml`, on a GitHub-hosted origin",
            "The GitHub origin makes `init` write the CI workflow too.",
            "and the seeded CI workflow for it. `update` does both too.",
            "diff .github/workflows/knowledge.yml ../kit-reference/.github/workflows/knowledge.yml",
            "and the CI workflow of that tracker. Delete",
            "Copy the two steps from the workflow that `houserules init` writes for the Beads tracker.",
            "houserules reads no trailer from the body of a pull request.",
            "A critical task gets one task review and never a re-review.",
            "`init --tracker github` brings the seeded CI workflow",
        ],
    );
    states(
        UPGRADE_GUIDE,
        &guide,
        &[
            "It writes no tracker, no workflow, no CI file, no `backlog/` directory, and no eval file.",
            "Every command that sets `tracker` also writes the `backlogId` pattern of the deliverables schema for the tracker, and brings the seeded CI workflow of an install that holds one to the form of the tracker. It writes no workflow for an install that holds none. `update` does both too.",
            "Compare it with the seeded workflow of this release, `template/.github/workflows/knowledge.yml` in the houserules repository",
            "houserules reads no trailer from the body of a pull or merge request.",
            "A critical task gets one task review and no re-review of its original findings. A fix round that fixes a Critical or Important finding, or that changes files beyond the files its findings name, then gets one review of its fix diff.",
        ],
    );
}

/// The page of the review layer, which the README links.
const REVIEW_LAYER_PAGE: &str = "docs/review-layer.md";

/// The review-layer page, its link in the README, the rows of the keys and
/// schema ids, the rows of the CLI reference, and the statements about `init`
/// that spec 10.7 makes true.
#[test]
fn the_review_layer_page_holds_the_section_and_the_rows_of_its_keys() {
    let raw = help_support::read(REVIEW_LAYER_PAGE);
    assert_eq!(raw.lines().next(), Some("# The review layer"));
    let page = help_support::folded(&raw);
    let readme = help_support::folded(&help_support::read("README.md"));
    let reference = help_support::folded(&help_support::read("docs/cli.md"));
    states(
        "README.md",
        &readme,
        &[
            "[The review layer](docs/review-layer.md)",
            "You can contribute a change: [CONTRIBUTING.md](CONTRIBUTING.md) explains the setup, the commit rules, and the gates that each change must pass.",
        ],
    );
    states(
        REVIEW_LAYER_PAGE,
        &page,
        &[
            "The commands below start from a fresh install: a repository where `houserules init` ran and one commit holds its files.",
            "houserules route --base HEAD~1",
            "| `review` |",
            "| `covers` |",
            "| `houserules/pack@1.3.0` |",
            "| `houserules/overrides@1.2.0` |",
            "`init` writes no CI file.",
            "`houserules ci print --host github`",
        ],
    );
    states(
        "docs/cli.md",
        &reference,
        &[
            "| `evidence` |",
            "| `route` |",
            "| `bundle <write\\|check>` |",
            "| `record <write\\|show\\|list\\|audit>` |",
            "| `ci <print\\|write>` |",
            "`stats --records <range>`",
        ],
    );
    let all = docs_support::REFERENCE_PAGES
        .iter()
        .map(|path| help_support::folded(&help_support::read(path)))
        .fold(readme.clone(), |all, text| format!("{all} {text}"));
    omits(
        "README.md and its reference pages",
        &all,
        &[
            "and a CI workflow on a GitHub-hosted origin",
            "The CI workflow is written only when your `origin` resolves to GitHub",
            "`update` backfills it if your origin becomes GitHub-hosted later.",
            "(GitHub-hosted origin only)",
            "**A CI gate** (`.github/workflows/knowledge.yml`), on a GitHub-hosted origin",
            "On other hosts `init` prints a skip note",
            "The seeded CI workflow checks out the head of a pull request for this reason.",
            "or reuses a workflow file name",
            "Pull requests are welcome",
            "merge requests are welcome",
        ],
    );
}

/// The `sh` blocks of the section `# The review layer` of its page run as
/// printed, one after the other, in a fresh install (`init`, then one commit),
/// and each prints the `text` block that follows it, with `<version>` in place
/// of the version of the binary.
#[cfg(unix)]
#[test]
fn the_review_layer_page_block_runs_as_printed_in_a_fresh_install() {
    let dir = tracker_support::scratch_git_repo();
    // The page shows `branches: ["main"]`; git's default branch name is the host's setting.
    tracker_support::git(dir.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    tracker_support::run_ok(dir.path(), &["init"]);
    tracker_support::commit_all(dir.path(), "chore: install houserules knowledge setup");
    let blocks = docs_support::blocks(&docs_support::section(
        &help_support::read(REVIEW_LAYER_PAGE),
        "# The review layer",
    ));
    let mut ran = 0;
    for (at, block) in blocks.iter().enumerate() {
        if block.info != "sh" {
            continue;
        }
        let script = &block.body;
        let (code, printed) = help_support::run_block(dir.path(), script);
        let printed = printed.replace(env!("CARGO_PKG_VERSION"), "<version>");
        let shown = match blocks.get(at + 1) {
            Some(next) if next.info == "text" => next.body.as_str(),
            _ => "",
        };
        assert_eq!(code, 0, "the block exits 0:\n{script}\n{printed}");
        assert_eq!(
            printed.trim_end(),
            shown,
            "the block does not print the text that follows it:\n{script}"
        );
        ran += 1;
    }
    assert_eq!(ran, 2, "the review-layer section holds two sh blocks");
}

/// The seeded-repository live proof (`houserules.seeded-repo-live-proof`): a
/// fresh `init` writes no workflow, so the procedure wires the CI with
/// `ci write` before the pull request, and it runs after the 2.0.0 release.
#[test]
fn the_seeded_repo_live_proof_wires_a_fresh_init_with_ci_write() {
    let entries: Value = serde_json::from_str(&help_support::read("knowledge/houserules.json"))
        .expect("the topic file is JSON");
    let entry = entries["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == "houserules.seeded-repo-live-proof")
        .expect("the entry exists");
    let text = flat(&format!(
        "{} {}",
        entry["summary"].as_str().expect("a summary"),
        body_text(entry)
    ));
    for phrase in [
        "houserules ci write --host github --mode reference --sha <release commit sha>",
        "after the 2.0.0 release",
        "HR-253",
        "hold for an install that a 1.x release seeded, not for the fresh install of the steps above",
        "It runs from the first release after 2.0.0; at the 2.0.0 release no older release is accepted, so skip it then.",
    ] {
        assert!(text.contains(phrase), "the entry lacks: {phrase}");
    }
    for stale in [
        "`init` writes `.github/workflows/knowledge.yml` only when the `origin` remote is GitHub-hosted",
        "the seeded knowledge workflow must read the version",
    ] {
        assert!(!text.contains(stale), "the entry still holds: {stale}");
    }
}
