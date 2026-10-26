//! A reader of the Markdown files that the document tests check: the fenced code blocks of a text,
//! the titles of its headings, and the lines under a heading. `openspec_docs.rs` reads the page and
//! the upgrade guide with it, `readme_funnel.rs` reads the README and its reference pages under
//! `docs/` with it, `workflow_texts.rs` reads the review-layer page, and `host_wording.rs` reads the
//! Documentation section of the README. `help_35.rs`, `help_36.rs`, and `workflow_texts.rs` read
//! the list of the reference pages. A test file that needs it declares this module, as
//! `tracker_support/mod.rs` is shared. The module allows dead code, because the test files that
//! include it use different parts of it.
//!
//! The reader follows these rules. Where a rule differs from CommonMark 0.31.2, the rule says so.
//!
//! - A fence is a line that starts with at least three backticks once its white space is trimmed.
//!   CommonMark allows up to three spaces of indentation; the reader allows any.
//! - A block starts at a fence and ends at the first later line that holds backticks alone, at
//!   least as many as the fence. A line of fewer backticks, or a line with text after its
//!   backticks, is part of the block. A block of four backticks can hold a block of three.
//! - The reader reads no tilde fence, which CommonMark reads. A line of `~~~` is a line of text.
//! - A line between the fences, a blank line included, belongs to the block. A line that looks
//!   like a heading there is no heading.
//! - A block that is not closed before the end of the text is a fault of the document, and the
//!   reader panics. CommonMark ends such a block at the end of the document.
//! - A heading is a line outside every block that starts with one to six `#` and a space. The
//!   number of `#` is its level. A line with no space after the `#`, a line of seven `#`, and a
//!   line of one `#` alone are text. CommonMark reads the last one as an empty heading.

#![allow(dead_code)]

/// The pages under `docs/` that hold the reference text that left the README, in the order of the
/// Documentation section of the README. `readme_funnel.rs` pins the title and the sections of each
/// page; a test that checks a statement the README held reads the README and these pages.
pub const REFERENCE_PAGES: [&str; 10] = [
    "docs/updating.md",
    "docs/subdirectory-install.md",
    "docs/trackers.md",
    "docs/evidence.md",
    "docs/runners.md",
    "docs/review-layer.md",
    "docs/ownership.md",
    "docs/reference-workflow.md",
    "docs/process-cost.md",
    "docs/cli.md",
];

/// Where a line of a document lies in respect of the fenced code blocks.
#[derive(Clone, Copy, PartialEq)]
enum Place {
    /// Outside every fenced block.
    Outside,
    /// The line that opens a block.
    Opens,
    /// A line between the fences of a block.
    Body,
    /// The line that closes a block.
    Closes,
}

/// The lines of `text` with their place. A fence is a run of at least three backticks; a block ends
/// at a line of backticks alone that is at least as long as the opening run. Panics when a block is
/// not closed.
fn classified(text: &str) -> Vec<(&str, Place)> {
    let mut open: Option<usize> = None;
    let mut lines = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let run = trimmed.chars().take_while(|c| *c == '`').count();
        match open {
            None if run >= 3 => {
                open = Some(run);
                lines.push((line, Place::Opens));
            }
            Some(length) if run >= length && trimmed.chars().all(|c| c == '`') => {
                open = None;
                lines.push((line, Place::Closes));
            }
            Some(_) => lines.push((line, Place::Body)),
            None => lines.push((line, Place::Outside)),
        }
    }
    assert!(open.is_none(), "a fenced block is not closed");
    lines
}

/// One fenced code block: its info string and the lines between the fences, joined by LF.
pub struct Block {
    /// The text after the opening backticks, with its white space trimmed. It is empty for a block
    /// that names no language.
    pub info: String,
    /// The lines between the fences, joined by LF, with no LF at the end. The body of an empty
    /// block is the empty text.
    pub body: String,
}

/// The fenced code blocks of `text`, in order. Panics when a block is not closed.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut found = Vec::new();
    let mut info = String::new();
    let mut body: Vec<&str> = Vec::new();
    for (line, place) in classified(text) {
        match place {
            Place::Outside => {}
            Place::Opens => {
                info = line.trim().trim_start_matches('`').trim().to_string();
                body.clear();
            }
            Place::Body => body.push(line),
            Place::Closes => found.push(Block {
                info: info.clone(),
                body: body.join("\n"),
            }),
        }
    }
    found
}

/// The level of a Markdown heading line, the number of its `#`, or `None` for any other line. A
/// heading is one to six `#` and a space. The caller decides whether the line lies in a block.
fn heading_level(line: &str) -> Option<usize> {
    let level = line.chars().take_while(|c| *c == '#').count();
    ((1..=6).contains(&level) && line[level..].starts_with(' ')).then_some(level)
}

/// The titles of the headings of exactly `level` in `text`, in order: each line outside every block
/// that starts with `level` `#` and a space, without the `#` and the space. Panics when a block is
/// not closed.
pub fn headings(text: &str, level: usize) -> Vec<String> {
    classified(text)
        .into_iter()
        .filter(|(_, place)| *place == Place::Outside)
        .filter(|(line, _)| heading_level(line) == Some(level))
        .map(|(line, _)| line[level + 1..].to_string())
        .collect()
}

/// The lines under the first heading line of `text` that equals `heading`, joined by LF, up to the
/// next heading of the same or a higher level (the same number of `#` or fewer). A heading of a
/// lower level belongs to the section, and so does each block. A heading line inside a block
/// neither starts nor ends a section. The heading line itself is not part of the result. Panics
/// when `heading` is no heading line, when no line of `text` outside the blocks equals it, and
/// when a block is not closed.
pub fn section(text: &str, heading: &str) -> String {
    let level = heading_level(heading).expect("a heading");
    let mut capturing = false;
    let mut lines = Vec::new();
    for (line, place) in classified(text) {
        if place == Place::Outside
            && let Some(found) = heading_level(line)
        {
            if capturing && found <= level {
                break;
            }
            if !capturing && line == heading {
                capturing = true;
                continue;
            }
        }
        if capturing {
            lines.push(line);
        }
    }
    assert!(capturing, "the text holds no `{heading}` line");
    lines.join("\n")
}

/// The body of the one block of `text` whose info string equals `info` and whose body contains
/// `needle`. Panics when no block or more than one block matches, and when a block is not closed.
pub fn block_with(text: &str, info: &str, needle: &str) -> String {
    let mut found: Vec<String> = blocks(text)
        .into_iter()
        .filter(|block| block.info == info && block.body.contains(needle))
        .map(|block| block.body)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "the text holds {} `{info}` blocks that contain `{needle}`",
        found.len()
    );
    found.remove(0)
}
