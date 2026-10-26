//! The kit text that batch 37 changes in the reference workflow (spec 11.6; HR-241
//! (6); acceptance case 10): the branch reviewer names the warn line of `stats`
//! beside its own count of the `Reversal:` lines, and the orchestrating skill
//! names the model line of every dispatch, the source of the shas of a dispatch,
//! and the step that closes the cost window of a phase. The other texts of the
//! batch are in `workflow_texts.rs`. A phrase is compared on the text with every
//! run of white space collapsed to one space.

mod tracker_support;

use tracker_support::{flat, repo_root, text_below};

/// The agent template, relative to the package and to the installed tree.
const BRANCH_REVIEWER: &str = ".claude/agents/branch-reviewer.md";

/// The orchestrating skill, relative to the package and to the installed tree.
const ORCHESTRATING: &str = ".claude/skills/orchestrating/SKILL.md";

/// The sentences of the dispatch protocol that give every dispatch, an
/// implementer dispatch included, a model line.
const MODEL_SENTENCE: &str = "Every dispatch carries a `Model: <alias>` line, an implementer dispatch included. A report from a dispatch with no `Model:` line records `unknown` as its model.";

/// The sentence of the dispatch protocol about the shas of a dispatch.
const SHA_SENTENCE: &str = "Paste every sha of a dispatch from the output of git rev-parse in the same turn; never retype one.";

/// The step of the Build and Verify rows that closes the window of a phase.
const WINDOW_STEP: &str =
    "close the phase's cost window when the phase ends, where the project keeps one";

/// The sentence of Batch close about a window that the controller rebuilds.
const REBUILT_WINDOW_SENTENCE: &str =
    "A window rebuilt from commit dates says so in its phase name.";

/// The sentence of the template that starts the count of the reversals.
const COUNT_SENTENCE: &str = "Count the `Reversal:` lines of each ledger";

/// The sentences of the branch reviewer about the warn line (spec 11.6): the
/// counting rule of `stats`, the lines that get a warning, and the use of the
/// warning. A reversal starts with `Reversal:` after optional spaces and an
/// optional list marker, so a line that holds the word and is no reversal is the
/// line that gets a warning.
const WARN_SENTENCES: [&str; 3] = [
    "`stats` counts a ledger line as a reversal when it starts with `Reversal:` after optional spaces and an optional list marker (`- `, `* `, or `+ `).",
    "For each other ledger line that holds `Reversal:`, `stats` prints one warn line on stderr: `<ledger>:<line>: a Reversal: inside a longer entry; stats counts only a line that starts with it`.",
    "You count each such line and `stats` does not; name these lines beside the two counts.",
];

/// The paragraphs of `text` (the Markdown file holds one line for each), flat.
fn paragraphs(text: &str) -> Vec<String> {
    text.lines().map(flat).collect()
}

/// The text of the file `relative` in the template of the reference workflow
/// and in the copy that `update` installs, each with the name of its surface.
fn both_copies(relative: &str) -> [(&'static str, String); 2] {
    [
        (
            "template/workflows/reference",
            text_below(
                repo_root()
                    .join("template")
                    .join("workflows")
                    .join("reference"),
                relative,
            ),
        ),
        ("the installed copy", text_below(repo_root(), relative)),
    ]
}

/// The lines of `text` from the heading line `heading` up to the next heading
/// of the same level, flat.
fn section(text: &str, heading: &str) -> String {
    let lines: Vec<&str> = text.lines().skip_while(|line| *line != heading).collect();
    assert!(!lines.is_empty(), "no heading {heading}");
    let end = lines[1..]
        .iter()
        .position(|line| line.starts_with("## "))
        .map_or(lines.len(), |at| at + 1);
    flat(&lines[..end].join("\n"))
}

/// Spec 11.6: the paragraph of the branch reviewer that counts the `Reversal:`
/// lines of each ledger holds the warn line of `stats` and says how the reviewer
/// uses it, in the template and in the copy that `update` installs.
#[test]
fn the_branch_reviewer_names_the_warn_line_beside_its_count() {
    for (surface, text) in both_copies(BRANCH_REVIEWER) {
        let paragraph = paragraphs(&text)
            .into_iter()
            .find(|paragraph| paragraph.contains(COUNT_SENTENCE))
            .unwrap_or_else(|| panic!("{surface} has no paragraph that counts the reversals"));
        for sentence in WARN_SENTENCES {
            assert!(
                paragraph.contains(sentence),
                "{surface} lacks beside the count: {sentence}\n{paragraph}"
            );
        }
    }
}

/// The counting rule of design.md 226(2) stays in the same paragraph: the
/// reviewer still compares the `reversals` count with the ledger, including the
/// lines inside a longer entry.
#[test]
fn the_branch_reviewer_keeps_the_comparison_of_the_counts() {
    let text = text_below(
        repo_root()
            .join("template")
            .join("workflows")
            .join("reference"),
        BRANCH_REVIEWER,
    );
    assert!(
        flat(&text).contains(
            "Compare the `reversals` count of each workspace in `stats.json` with the `Reversal:` lines of that workspace's ledger, including the lines inside a longer ledger entry."
        ),
        "{text}"
    );
}

/// The dispatch protocol gives every dispatch a model line, an implementer
/// dispatch included, and names the source of the shas of a dispatch, in the
/// template and in the installed copy.
#[test]
fn the_dispatch_protocol_names_the_model_line_and_the_source_of_a_sha() {
    for (surface, text) in both_copies(ORCHESTRATING) {
        let protocol = section(&text, "## Dispatch protocol");
        for sentence in [MODEL_SENTENCE, SHA_SENTENCE] {
            assert!(
                protocol.contains(sentence),
                "{surface} lacks in the dispatch protocol: {sentence}"
            );
        }
    }
}

/// The Build and Verify rows of the stages table each name the step that closes
/// the window of the phase, in the template and in the installed copy.
#[test]
fn the_build_and_verify_rows_close_the_window_of_the_phase() {
    for (surface, text) in both_copies(ORCHESTRATING) {
        for row in ["| Build |", "| Verify |"] {
            let line = text
                .lines()
                .find(|line| line.starts_with(row))
                .unwrap_or_else(|| panic!("{surface} has no row {row}"));
            assert!(
                flat(line).contains(WINDOW_STEP),
                "{surface} lacks in the row {row}: {WINDOW_STEP}\n{line}"
            );
        }
    }
}

/// Batch close says that a window rebuilt from commit dates says so in its
/// phase name, in the template and in the installed copy.
#[test]
fn batch_close_names_a_window_that_the_controller_rebuilt() {
    for (surface, text) in both_copies(ORCHESTRATING) {
        assert!(
            section(&text, "## Batch close").contains(REBUILT_WINDOW_SENTENCE),
            "{surface} lacks in Batch close: {REBUILT_WINDOW_SENTENCE}"
        );
    }
}
