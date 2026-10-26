//! The rules of the Markdown reader that the document tests share (`docs_support/mod.rs`): the
//! fenced blocks of a text, the titles of its headings, and the lines under a heading. Each test
//! states one rule of the module doc on a small text, so that a document test that reads the page,
//! the guide, or the README reads it as the module doc says.

mod docs_support;

use docs_support::{Block, block_with, blocks, headings, section};

/// The pair of info string and body of each block of `text`.
fn pairs(text: &str) -> Vec<(String, String)> {
    blocks(text)
        .into_iter()
        .map(|Block { info, body }| (info, body))
        .collect()
}

/// A block holds its blank lines, keeps its info string trimmed, and has no LF at its end.
#[test]
fn a_block_keeps_its_blank_lines_and_its_trimmed_info_string() {
    let text = "text\n\n```  sh \nfirst\n\nsecond\n```\n\n```\n```\n";

    assert_eq!(
        pairs(text),
        vec![
            ("sh".to_string(), "first\n\nsecond".to_string()),
            (String::new(), String::new()),
        ]
    );
}

/// A block of four backticks ends at a line of four or more backticks alone, and a line of three
/// backticks, a heading, or a closing line with text inside it is part of the block.
#[test]
fn a_block_ends_at_backticks_alone_that_are_as_many_as_its_fence() {
    let text = "````markdown\n```sh\n# a heading\n```\n```` x\n````\n\nafter\n";

    assert_eq!(
        pairs(text),
        vec![(
            "markdown".to_string(),
            "```sh\n# a heading\n```\n```` x".to_string()
        )]
    );
}

/// A line that is indented before its fence still opens and closes a block.
#[test]
fn an_indented_fence_opens_and_closes_a_block() {
    assert_eq!(
        pairs("  ```text\n  one\n  ```\n"),
        vec![("text".to_string(), "  one".to_string())]
    );
}

/// The reader reads no tilde fence: the lines of one are text.
#[test]
fn a_tilde_fence_is_text() {
    assert_eq!(pairs("~~~sh\nrun\n~~~\n"), Vec::<(String, String)>::new());
}

/// A block that the text does not close is a fault of the document.
#[test]
#[should_panic(expected = "a fenced block is not closed")]
fn a_block_that_is_not_closed_panics() {
    blocks("```sh\nrun\n");
}

/// The section holds the lines under its heading, its subsections, and its blocks, and ends at the
/// next heading of its level or a higher one.
#[test]
fn a_section_ends_at_a_heading_of_its_level_or_a_higher_one() {
    let text = "# One\nzero\n## A\none\n### A1\ntwo\n## B\nthree\n# Two\nfour\n";

    assert_eq!(section(text, "## A"), "one\n### A1\ntwo");
    assert_eq!(section(text, "### A1"), "two");
    assert_eq!(section(text, "## B"), "three");
    assert_eq!(
        section(text, "# One"),
        "zero\n## A\none\n### A1\ntwo\n## B\nthree"
    );
    assert_eq!(section(text, "# Two"), "four");
}

/// A line of a block that looks like a heading neither starts nor ends a section.
#[test]
fn a_heading_inside_a_block_neither_starts_nor_ends_a_section() {
    let text = "## A\n```sh\n## B\n# C\n```\nstill A\n## B\nreal B\n";

    assert_eq!(section(text, "## A"), "```sh\n## B\n# C\n```\nstill A");
    assert_eq!(section(text, "## B"), "real B");
}

/// The section of a heading that occurs twice is the first one.
#[test]
fn the_section_of_a_heading_that_occurs_twice_is_the_first() {
    assert_eq!(section("## A\none\n## A\ntwo\n", "## A"), "one");
}

/// A line with no space after its `#`, one `#` alone included, is text: it does not end a section.
#[test]
fn a_line_with_no_space_after_its_hashes_is_text() {
    let text = "## A\none\n#tag\n#\n####### seven\ntwo\n## B\n";

    assert_eq!(section(text, "## A"), "one\n#tag\n#\n####### seven\ntwo");
}

/// A line of seven `#` is no heading, so it names no section.
#[test]
#[should_panic(expected = "a heading")]
fn a_line_of_seven_hashes_names_no_section() {
    section("####### seven\ntext\n", "####### seven");
}

/// A line of six `#` and a space is a heading of level six.
#[test]
fn a_line_of_six_hashes_is_a_heading() {
    assert_eq!(section("###### six\ntext\n## B\n", "###### six"), "text");
}

/// A heading that the text lacks is a fault of the test.
#[test]
#[should_panic(expected = "the text holds no `## B` line")]
fn a_missing_heading_panics() {
    section("## A\none\n", "## B");
}

/// A line that is no heading line names no section.
#[test]
#[should_panic(expected = "a heading")]
fn a_line_that_is_no_heading_panics() {
    section("## A\none\n", "A");
}

/// The titles of the headings of one level come in order, without their `#`. A heading of another
/// level, a line inside a block, and a line with no space after its `#` are no heading of the level.
#[test]
fn the_headings_of_a_level_are_listed_in_order_outside_the_blocks() {
    let text = "# One\n## A\n### A1\n```sh\n## in a block\n```\n## B\n##C\n####### seven\n## C\n";

    assert_eq!(headings(text, 2), vec!["A", "B", "C"]);
    assert_eq!(headings(text, 3), vec!["A1"]);
    assert_eq!(headings(text, 1), vec!["One"]);
    assert_eq!(headings(text, 4), Vec::<String>::new());
}

/// The block that matches by info string and needle is the one block of the text, and the reader
/// panics for none and for more than one.
#[test]
fn the_search_for_a_block_finds_the_one_that_matches() {
    let text = "```sh\nrun one\n```\n```sh\nrun two\n```\n```json\n{\"one\": 1}\n```\n";

    assert_eq!(block_with(text, "sh", "two"), "run two");
    assert_eq!(block_with(text, "json", "one"), "{\"one\": 1}");
}

/// No block matches.
#[test]
#[should_panic(expected = "the text holds 0 `sh` blocks that contain `three`")]
fn the_search_for_a_block_panics_for_no_match() {
    block_with("```sh\nrun one\n```\n", "sh", "three");
}

/// Two blocks match.
#[test]
#[should_panic(expected = "the text holds 2 `sh` blocks that contain `run`")]
fn the_search_for_a_block_panics_for_two_matches() {
    block_with("```sh\nrun one\n```\n```sh\nrun two\n```\n", "sh", "run");
}
