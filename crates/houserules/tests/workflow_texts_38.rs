//! The faster loop in the reference workflow (batch 38, spec 19; HR-260;
//! design.md 280 to 283; acceptance case 14). The implementer template, the
//! task-reviewer template, the orchestrating skill, the planning skill, and four
//! pack entries carry the rules of 19.1 to 19.8 as whole sentences. A sentence is
//! compared on the text with every run of white space collapsed to one space.
//! The kit texts of the rest of the workflow are in `workflow_texts.rs`.

mod tracker_support;

use serde_json::Value;
use tracker_support::{flat, repo_root, text_below};

/// The implementer template, relative to the package and to the installed tree.
const IMPLEMENTER: &str = ".claude/agents/implementer.md";

/// The task-reviewer template, relative to the package and to the installed tree.
const TASK_REVIEWER: &str = ".claude/agents/task-reviewer.md";

/// The orchestrating skill, relative to the package and to the installed tree.
const ORCHESTRATING: &str = ".claude/skills/orchestrating/SKILL.md";

/// The planning skill, relative to the package and to the installed tree.
const PLANNING: &str = ".claude/skills/planning/SKILL.md";

/// The sentence that says whose run holds the whole-suite half of the evidence
/// of a critical task (ruling Q1 of the task): the implementer template, the
/// task-reviewer template, both skills, and the `process.tdd` and
/// `process.risk-tiers` entries carry it word for word.
const WHOLE_SUITE_HALF: &str =
    "The whole-suite half is the integration run of the person who runs the work.";

/// The sentences of the implementer about its own test runs (19.1): the focused
/// test while it iterates, the tests of its files before a commit, what those
/// tests are, and that they replace the full suite and every integration-only
/// suite.
const IMPLEMENTER_RUNS: [&str; 6] = [
    "While you iterate, run the focused test.",
    "Before you commit, run the tests of the files your task changes and every other gate of the brief.",
    "The tests of your files are each test file that you add or change, and each test file that runs a command or module whose source you change.",
    "Name the targets in the report.",
    "Run these tests in place of the full suite, and skip every suite that the plan marks as integration-only.",
    "The person who runs the work runs the full suite, and every suite the plan marks as integration-only, once per integration. A red in that run opens a fix round for your task.",
];

/// The sentence of the template before 2.0.0 that the faster loop replaces.
const OLD_TEST_SENTENCE: &str =
    "Run the focused test while iterating and the full suite once before committing.";

/// The sentences of the `coverage` bullet of the implementer: coverage is a
/// full-suite measure, so the implementer leaves the field `null` and the person
/// who runs the work fills it at integration.
const IMPLEMENTER_COVERAGE: [&str; 2] = [
    "`coverage` (`null`: a coverage gate measures the full suite, and the person who runs the work runs that suite at integration and writes the measure into this field.",
    "Do not explain the `null` in `concerns` or in any other field.)",
];

/// The sentences of the `coverage` bullet of the template before 2.0.0, which
/// keyed the measure to a full-suite run of the implementer.
const OLD_COVERAGE_SENTENCES: [&str; 2] = [
    "whenever the project has a coverage gate and the full suite ran",
    "`null` only when the suite did not run",
];

/// The phrase of the `tests` bullet of the implementer that lists the runs that
/// the field holds, and the phrase of the template before 2.0.0 that named a
/// suite run there.
const IMPLEMENTER_TESTS_RUNS: &str = "the test, lint, and audit runs stay here";
const OLD_IMPLEMENTER_TESTS_RUNS: &str = "the suite, lint, and audit runs stay here";

/// The sentences of the implementer about a fix round (19.1).
const IMPLEMENTER_FIX_ROUND: [&str; 2] = [
    "A fix round runs each finding's check, the tests of the files that the round changes, and the other gates of the brief.",
    "It runs no full suite and no suite that the plan marks as integration-only.",
];

/// The sentences of the implementer about an open reading (19.2).
const IMPLEMENTER_OPEN_READING: [&str; 3] = [
    "A reading that the spec or the brief leaves open, and that a test would pin, is a question before you write that test.",
    "Stop, name the reading and its choices, and return NEEDS_CONTEXT.",
    "The person who runs the work rules it and resumes you.",
];

/// The sentences of the orchestrating skill about the integration runs (19.1).
const ORCHESTRATING_INTEGRATION: [&str; 5] = [
    "Run the full suite, and every suite the plan marks as integration-only, once per integration.",
    "An implementer's run and a fix round run the tests of the files they change instead.",
    "Two independent integration suites may run at once.",
    "Independent suites share no output directory.",
    "A conflict or a red gate at integration is a fix round for the later task, except a conflict in a file that the plan names as shared.",
];

/// The sentences of the orchestrating skill about the integration of a task that
/// runs in the shared checkout: the task has no cherry-pick, so its close is its
/// integration.
const ORCHESTRATING_SHARED_CHECKOUT: [&str; 4] = [
    "A task that you dispatch into a shared checkout has no cherry-pick: it integrates when its implementer closes.",
    "A sequential task, the one task of a light batch, and each fix round of such a task integrate this way when they run in the shared checkout; a task in a worktree integrates by its cherry-pick.",
    "At that close, run the full suite and every suite the plan marks as integration-only, before you dispatch the next task.",
    "A red in that run opens a fix round for the task.",
];

/// The sentences of the orchestrating skill about the coverage measure of the
/// integration run.
const ORCHESTRATING_COVERAGE: [&str; 3] = [
    "When the project has a coverage gate, the full-suite run at integration measures it.",
    "Write the measure into the `coverage` field of the task report: one measure per target, with `target`, `metric`, `measured`, and `floor`.",
    "The implementer leaves that field `null`.",
];

/// The sentences of the orchestrating skill about a file that two tasks share
/// (19.6).
const ORCHESTRATING_SHARED_FILE: [&str; 3] = [
    "A file that the plan names as shared may conflict at the later cherry-pick.",
    "Resolve that conflict by keeping both sides, and run the gates.",
    "Any other resolution is a fix round for the later task.",
];

/// The sentences of the planning skill about a file that two tasks share (19.6).
const PLANNING_SHARED_FILE: [&str; 4] = [
    "The plan may name a file that two tasks share, such as a command-line entry point whose changes merge by keeping both sides.",
    "Two tasks whose only shared file is that file may run at once.",
    "You resolve the conflict of the later cherry-pick by keeping both sides and run the gates.",
    "Any other resolution is a fix round for the later task.",
];

/// The sentence of the planning skill before 2.0.0 that let two tasks run at once
/// without the bound of their only shared file.
const OLD_PLANNING_SHARED_FILE_SENTENCE: &str = "The two tasks may run at once.";

/// The sentences of the planning skill about the suites that only the person who
/// runs the work runs (ruling Q2 of the task).
const PLANNING_INTEGRATION_ONLY: [&str; 2] = [
    "Besides the full suite, the plan marks each suite that only you run at integration, for example a slow suite or a suite for another platform.",
    "A task's own run skips every such suite.",
];

/// The sentences of the orchestrating skill about the open readings of a brief
/// (19.2) and about asking the owner without blocking (19.4).
const ORCHESTRATING_READINGS: [&str; 6] = [
    "A brief rules every reading that the spec leaves open and that a test would pin.",
    "Find each one when you write the brief, and rule it there.",
    "When an implementer returns NEEDS_CONTEXT with a reading, rule it and resume the task.",
    "Ask the project's owner or decider in a plain message, never in a question tool that holds your turn.",
    "Keep dispatching every task and fix round that needs no ruling.",
    "Send the ruling to the waiting agent when it arrives.",
];

/// The sentences of the orchestrating skill about the times of the ledger (19.3).
const ORCHESTRATING_TIMES: [&str; 2] = [
    "The ledger records each dispatch's start and close, each with its UTC time.",
    "Read the time from the clock in the command that writes the line.",
];

/// The sentences about a wave (19.5): the orchestrating and the planning skill
/// carry each of them word for word.
const WAVE: [&str; 4] = [
    "A wave orders the tasks; it is no barrier.",
    "A task starts as soon as every task it needs is integrated.",
    "A task that consumes only an interface that no open finding changes starts from an interim base that holds the commits of the task it needs.",
    "You integrate the commits of the consuming task alone, after the task it needs closes.",
];

/// The sentence about the integration of a wave before 2.0.0, which named the
/// controller in the third person.
const OLD_WAVE_SENTENCE: &str = "The controller integrates the commits of the consuming task alone";

/// The sentence of the planning skill that opens the waves (19.5).
const PLANNING_WAVES: &str = "Order the tasks in waves when some tasks need others.";

/// The sentences of the orchestrating skill about the integration of a fix
/// during a fix-diff review (19.7).
const ORCHESTRATING_FIX_DIFF: [&str; 2] = [
    "While a fix-diff review or a re-review runs, you may integrate the fix and run the integration gates.",
    "When the review files a new Critical or Important finding, undo the integration (the batch branch returns to the commit before it) and open the next round for the task.",
];

/// The paragraph that `process.ask-when-missing` gains for an open reading (19.2).
const PACK_OPEN_READING: &str = "A reading that the spec or the handoff leaves open, and that a test would pin, is a question before the author of the change writes that test. The author stops, names the reading and its choices, and asks the person who runs the work. The person who runs the work rules every such reading it finds when it writes the handoff.";

/// The paragraph that `process.ask-when-missing` gains for asking without
/// blocking (19.4).
const PACK_ASK_WITHOUT_BLOCKING: &str = "The person who runs the work asks the project's owner or decider in a plain message, never in a question tool that holds its turn. It keeps working: it starts every task and fix round that needs no ruling, and it sends the ruling to the waiting agent when it arrives.";

/// The paragraph that `process.parallel-on-disjoint-files` gains for a shared
/// file (19.6).
const PACK_SHARED_FILE: &str = "Two tasks whose only shared file is a file that the plan names as shared, such as a command-line entry point whose changes merge by keeping both sides, may run at once. The person who runs the work resolves the cherry-pick conflict of the later task at integration, keeping both sides, and runs the gates. A resolution that needs more than keeping both sides is a fix round for the later task.";

/// The sentences of the pack entries `process.tdd` and `process.risk-tiers` that
/// say whose run holds the whole-suite half of the evidence of a critical task,
/// and which run it is for a task that runs in the shared checkout.
const PACK_WHOLE_SUITE_HALF: &str = "The whole-suite half is the integration run of the person who runs the work. For a task that runs in the shared checkout, that run is the one at the close of the task.";

/// The paragraph that `process.parallel-on-disjoint-files` gains for a task that
/// runs in the shared checkout: it has no cherry-pick, so its close is its
/// integration.
const PACK_SHARED_CHECKOUT: &str = "A task that runs in the shared checkout has no cherry-pick: it integrates when its author closes. At that close, the person who runs the work runs the full suite and every suite the plan marks as integration-only, before the next handoff. A red in that run is a fix round for the task.";

/// The paragraph that `process.bounded-review-loop` gains (19.7).
const PACK_FIX_DIFF: &str = "While a fix-diff review or a re-review runs, the person who runs the work may integrate the fix and run the integration gates. When the review files a new Critical or Important finding, the person who runs the work undoes the integration (the batch branch returns to the commit before it) and the task opens its next round.";

/// The paragraph that `process.tdd` gains (19.8).
const PACK_TDD_LATE_TEST: &str = "A test added after the recorded RED carries its own RED or a mutation row in the same report entry; the first RED proves only the tests that existed when it ran.";

/// The paragraph that `quality.principles` gains (19.8).
const PACK_STRUCT_READER: &str = "A struct whose fields a reader fills names, in its doc comment, the production reader of each field, so a field that no reader fills shows.";

/// The flat text of the file `relative` in the template of the reference
/// workflow and in the copy that `update` installs, each with the name of its
/// surface.
fn both_copies(relative: &str) -> [(&'static str, String); 2] {
    [
        (
            "template/workflows/reference",
            flat(&text_below(
                repo_root()
                    .join("template")
                    .join("workflows")
                    .join("reference"),
                relative,
            )),
        ),
        (
            "the installed copy",
            flat(&text_below(repo_root(), relative)),
        ),
    ]
}

/// Asserts that the file `relative` holds every sentence, in both copies.
fn assert_both_hold(relative: &str, sentences: &[&str]) {
    for (surface, text) in both_copies(relative) {
        for sentence in sentences {
            assert!(
                text.contains(sentence),
                "{surface} of {relative} lacks: {sentence}"
            );
        }
    }
}

/// Asserts that the file `relative` holds none of the phrases, in both copies.
fn assert_neither_holds(relative: &str, phrases: &[&str]) {
    for (surface, text) in both_copies(relative) {
        for phrase in phrases {
            assert!(
                !text.contains(phrase),
                "{surface} of {relative} still holds: {phrase}"
            );
        }
    }
}

/// The paragraphs of the body of the entry `id` of the kit pack `pack`, each
/// flat.
fn pack_paragraphs(pack: &str, id: &str) -> Vec<String> {
    let text = text_below(
        repo_root().join("template").join("packs"),
        &format!("{pack}.json"),
    );
    let pack: Value = serde_json::from_str(&text).expect("the pack is JSON");
    let entry = pack["entries"]
        .as_array()
        .expect("a pack holds entries")
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("the pack holds no entry {id}"));
    entry["body"]
        .as_array()
        .expect("an entry holds a body")
        .iter()
        .map(|paragraph| flat(paragraph.as_str().expect("a paragraph is a string")))
        .collect()
}

/// Asserts that the body of the entry `id` of `pack` holds the paragraph `text`
/// whole.
fn assert_pack_paragraph(pack: &str, id: &str, text: &str) {
    let paragraphs = pack_paragraphs(pack, id);
    assert!(
        paragraphs.iter().any(|paragraph| paragraph == text),
        "{id} lacks the paragraph: {text}\n{paragraphs:#?}"
    );
}

/// 19.1: the implementer runs the focused test while it iterates and the tests of
/// its files before a commit, names the targets, and leaves the full suite and
/// every integration-only suite to the person who runs the work.
#[test]
fn the_implementer_runs_the_tests_of_its_files_and_skips_the_integration_suites() {
    assert_both_hold(IMPLEMENTER, &IMPLEMENTER_RUNS);
    assert_neither_holds(IMPLEMENTER, &[OLD_TEST_SENTENCE]);
}

/// The `tests` field of the report holds the test runs of the implementer, and no
/// text keys it to a run of the suite.
#[test]
fn the_tests_field_of_the_report_holds_test_runs_and_names_no_suite_run() {
    assert_both_hold(IMPLEMENTER, &[IMPLEMENTER_TESTS_RUNS]);
    assert_neither_holds(IMPLEMENTER, &[OLD_IMPLEMENTER_TESTS_RUNS]);
}

/// Coverage is a full-suite measure: the implementer leaves `coverage` `null`
/// and says so in no other field, and the orchestrating skill writes the measure
/// of the integration run into the task report.
#[test]
fn the_coverage_measure_belongs_to_the_integration_run() {
    assert_both_hold(IMPLEMENTER, &IMPLEMENTER_COVERAGE);
    assert_both_hold(ORCHESTRATING, &ORCHESTRATING_COVERAGE);
    assert_neither_holds(IMPLEMENTER, &OLD_COVERAGE_SENTENCES);
}

/// A task that runs in the shared checkout has no cherry-pick: the orchestrating
/// skill runs the full suite and every integration-only suite when its
/// implementer closes, before the next dispatch.
#[test]
fn a_task_in_the_shared_checkout_integrates_when_its_implementer_closes() {
    assert_both_hold(ORCHESTRATING, &ORCHESTRATING_SHARED_CHECKOUT);
}

/// 19.1: a fix round of the implementer runs each finding's check, the tests of
/// the files that the round changes, and the other gates, and no full suite.
#[test]
fn a_fix_round_of_the_implementer_runs_the_checks_and_the_targeted_tests() {
    assert_both_hold(IMPLEMENTER, &IMPLEMENTER_FIX_ROUND);
}

/// 19.2: the implementer stops before the test that an open reading would pin,
/// names the reading and its choices, and returns `NEEDS_CONTEXT`.
#[test]
fn the_implementer_stops_for_an_open_reading_before_its_test() {
    assert_both_hold(IMPLEMENTER, &IMPLEMENTER_OPEN_READING);
}

/// 19.1: the orchestrating skill runs the full suite and every integration-only
/// suite once per integration, may run two independent ones at once, and
/// resolves a conflict in a file that the plan names as shared by keeping both
/// sides (19.6).
#[test]
fn the_orchestrating_skill_runs_the_integration_suites_once_per_integration() {
    assert_both_hold(ORCHESTRATING, &ORCHESTRATING_INTEGRATION);
    assert_both_hold(ORCHESTRATING, &ORCHESTRATING_SHARED_FILE);
}

/// 19.2 and 19.4: the orchestrating skill rules each open reading when it writes
/// the brief, rules a `NEEDS_CONTEXT` of an implementer, asks the owner in a
/// plain message, and keeps dispatching what needs no ruling.
#[test]
fn the_orchestrating_skill_rules_open_readings_and_asks_without_blocking() {
    assert_both_hold(ORCHESTRATING, &ORCHESTRATING_READINGS);
}

/// 19.3: the ledger of the orchestrating skill stamps each dispatch and each
/// close with its UTC time, read from the clock in the command that writes the
/// line.
#[test]
fn the_ledger_stamps_each_dispatch_and_close_with_its_utc_time() {
    assert_both_hold(ORCHESTRATING, &ORCHESTRATING_TIMES);
}

/// 19.5: a wave orders the tasks and is no barrier, in the orchestrating skill
/// and in the planning skill, with one wording in both; the planning skill opens
/// the waves.
#[test]
fn a_wave_is_no_barrier_in_the_orchestrating_and_the_planning_skill() {
    assert_both_hold(ORCHESTRATING, &WAVE);
    assert_both_hold(PLANNING, &WAVE);
    assert_both_hold(PLANNING, &[PLANNING_WAVES]);
    assert_neither_holds(ORCHESTRATING, &[OLD_WAVE_SENTENCE]);
    assert_neither_holds(PLANNING, &[OLD_WAVE_SENTENCE]);
}

/// 19.6: the planning skill lets the plan name one file that two tasks share and
/// says how the later cherry-pick resolves it. Two tasks run at once only when
/// that file is their only shared file, as `process.parallel-on-disjoint-files`
/// states it.
#[test]
fn the_planning_skill_names_the_file_that_two_tasks_share() {
    assert_both_hold(PLANNING, &PLANNING_SHARED_FILE);
    assert_neither_holds(PLANNING, &[OLD_PLANNING_SHARED_FILE_SENTENCE]);
    assert!(
        pack_paragraphs("process", "process.parallel-on-disjoint-files")
            .iter()
            .any(|paragraph| paragraph.contains("whose only shared file is")),
        "process.parallel-on-disjoint-files lacks the bound of the shared file"
    );
}

/// Ruling Q2 of the task: the planning skill marks each suite that only the
/// person who runs the work runs at integration, and a task's own run skips it.
#[test]
fn the_planning_skill_marks_the_integration_only_suites() {
    assert_both_hold(PLANNING, &PLANNING_INTEGRATION_ONLY);
}

/// 19.7: the orchestrating skill integrates a fix while its fix-diff review runs
/// and undoes the integration for a new Critical or Important finding.
#[test]
fn the_orchestrating_skill_integrates_a_fix_during_its_fix_diff_review() {
    assert_both_hold(ORCHESTRATING, &ORCHESTRATING_FIX_DIFF);
}

/// Ruling Q1 of the task: every place that states the evidence of a critical task
/// as the whole suite passing at `HEAD` says whose run holds that half. The task
/// reviewer also names the run that the report holds in its place.
#[test]
fn the_whole_suite_half_of_a_critical_task_is_the_integration_run_in_every_place() {
    for file in [IMPLEMENTER, TASK_REVIEWER, ORCHESTRATING, PLANNING] {
        assert_both_hold(file, &[WHOLE_SUITE_HALF]);
    }
    assert_both_hold(
        TASK_REVIEWER,
        &["Check the report for a run of the tests of the task's files at `HEAD` in its place."],
    );
    for (pack, id) in [
        ("process", "process.tdd"),
        ("process", "process.risk-tiers"),
    ] {
        let paragraphs = pack_paragraphs(pack, id);
        assert!(
            paragraphs
                .iter()
                .any(|paragraph| paragraph.contains(PACK_WHOLE_SUITE_HALF)),
            "{id} lacks: {PACK_WHOLE_SUITE_HALF}\n{paragraphs:#?}"
        );
    }
}

/// 19.2 and 19.4: `process.ask-when-missing` carries both paragraphs, after the
/// paragraph that says who answers.
#[test]
fn the_ask_when_missing_entry_carries_the_open_reading_and_the_plain_message() {
    let paragraphs = pack_paragraphs("process", "process.ask-when-missing");
    assert_eq!(paragraphs.len(), 3, "{paragraphs:#?}");
    assert!(paragraphs[0].starts_with("Who answers depends on the project"));
    assert_eq!(paragraphs[1], PACK_OPEN_READING);
    assert_eq!(paragraphs[2], PACK_ASK_WITHOUT_BLOCKING);
}

/// 19.6: `process.parallel-on-disjoint-files` carries the shared-file paragraph
/// after the paragraph of the integration, and names no file of this repository.
#[test]
fn the_parallel_entry_carries_the_file_that_the_plan_names_as_shared() {
    let paragraphs = pack_paragraphs("process", "process.parallel-on-disjoint-files");
    let integration = paragraphs
        .iter()
        .position(|paragraph| {
            paragraph.starts_with("The person who runs the work integrates a finished task")
        })
        .expect("the paragraph of the integration");
    assert_eq!(paragraphs[integration + 1], PACK_SHARED_FILE);
    assert!(
        paragraphs
            .iter()
            .all(|paragraph| !paragraph.contains("main.rs")),
        "{paragraphs:#?}"
    );
}

/// `process.parallel-on-disjoint-files` carries the paragraph of a task that runs
/// in the shared checkout after the paragraph of the shared file: the close of
/// such a task is its integration.
#[test]
fn the_parallel_entry_carries_the_integration_at_the_close_of_a_task_in_the_shared_checkout() {
    let paragraphs = pack_paragraphs("process", "process.parallel-on-disjoint-files");
    let shared_file = paragraphs
        .iter()
        .position(|paragraph| paragraph == PACK_SHARED_FILE)
        .expect("the paragraph of the shared file");
    assert_eq!(paragraphs[shared_file + 1], PACK_SHARED_CHECKOUT);
}

/// 19.7: `process.bounded-review-loop` carries the paragraph of the integration
/// during a fix-diff review after the paragraph of the caps.
#[test]
fn the_bounded_review_loop_carries_the_integration_during_a_fix_diff_review() {
    let paragraphs = pack_paragraphs("process", "process.bounded-review-loop");
    let caps = paragraphs
        .iter()
        .position(|paragraph| paragraph.starts_with("Re-reviews per task: light 0, standard 1."))
        .expect("the paragraph of the caps");
    assert_eq!(paragraphs[caps + 1], PACK_FIX_DIFF);
}

/// 19.8: `process.tdd` carries the sentence of the late test, and
/// `quality.principles` the sentence of the struct and its reader.
#[test]
fn the_tdd_and_principles_entries_carry_the_two_body_sentences() {
    assert_pack_paragraph("process", "process.tdd", PACK_TDD_LATE_TEST);
    assert_pack_paragraph("quality", "quality.principles", PACK_STRUCT_READER);
}
