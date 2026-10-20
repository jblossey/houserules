//! The marked block of a file that the project also edits: the text that
//! `render` owns, from a line that holds only the begin marker to a line
//! that holds only the end marker. `render` never touches a byte outside it.
//!
//! A marker is a line of its own: after trimming whitespace, the line equals
//! the marker. Text that names a marker inside a longer line is project text.
//! Limits of that token form:
//!
//! - A marker line inside a fenced code block counts as a marker. The scan
//!   reads lines and models no Markdown.
//! - A marker line that carries other text on it (`text <!-- houserules:end -->`)
//!   is project text, so a block whose end marker has such a prefix has no end
//!   marker.
//!
//! A file holds no marker, or exactly one begin marker followed by exactly one
//! end marker. Every other shape is a [`MarkerError`], never a guess.
//!
//! The line ending of a file is `\r\n` when the file holds one, else `\n`. A
//! block that `render` writes into a file takes that ending.

use std::ops::Range;

/// The line that opens the block.
pub(super) const BEGIN: &str = "<!-- houserules:begin -->";
/// The line that closes the block.
pub(super) const END: &str = "<!-- houserules:end -->";

/// Why a file has no usable block.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum MarkerError {
    /// A begin marker with no end marker after it.
    MissingEnd,
    /// An end marker with no begin marker before it.
    MissingBegin,
    /// More than one begin marker or more than one end marker.
    Several,
}

impl MarkerError {
    /// The one line that names `file` and the fault.
    pub(super) fn line(&self, file: &str) -> String {
        match self {
            Self::MissingEnd => format!(
                "{file}: the houserules block has no end marker {END}; add the end marker after the block"
            ),
            Self::MissingBegin => format!(
                "{file}: the houserules end marker has no begin marker before it; add the begin marker before the block"
            ),
            Self::Several => {
                format!("{file}: the file holds more than one houserules block; keep one block")
            }
        }
    }
}

/// Where the block sits in a file.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Located {
    /// The bytes from the start of the begin marker line to the end of the end
    /// marker line, without that line's terminator.
    pub block: Range<usize>,
    /// The end of the end marker line with its terminator, or the end of the
    /// file when the line has none.
    pub line_end: usize,
}

/// One line of a text: where it starts, where its content ends, and where
/// the next line starts.
struct Line {
    start: usize,
    content_end: usize,
    next: usize,
}

/// The lines of `text`, each with its offsets. A terminator is `\n` or
/// `\r\n`; the last line may have none.
fn lines(text: &str) -> impl Iterator<Item = Line> + '_ {
    let mut start = 0;
    std::iter::from_fn(move || {
        if start >= text.len() {
            return None;
        }
        let (content_end, next) = match text[start..].find('\n') {
            Some(offset) => {
                let newline = start + offset;
                let content_end = if newline > start && text.as_bytes()[newline - 1] == b'\r' {
                    newline - 1
                } else {
                    newline
                };
                (content_end, newline + 1)
            }
            None => (text.len(), text.len()),
        };
        let line = Line {
            start,
            content_end,
            next,
        };
        start = next;
        Some(line)
    })
}

/// Finds the block of `text`: `None` when `text` holds no marker.
pub(super) fn locate(text: &str) -> Result<Option<Located>, MarkerError> {
    let mut begins = Vec::new();
    let mut ends = Vec::new();
    for line in lines(text) {
        match text[line.start..line.content_end].trim() {
            BEGIN => begins.push(line),
            END => ends.push(line),
            _ => {}
        }
    }
    match (begins.as_slice(), ends.as_slice()) {
        ([], []) => Ok(None),
        ([_], []) => Err(MarkerError::MissingEnd),
        ([], [_]) => Err(MarkerError::MissingBegin),
        ([begin], [end]) if begin.start < end.start => Ok(Some(Located {
            block: begin.start..end.content_end,
            line_end: end.next,
        })),
        ([_], [_]) => Err(MarkerError::MissingBegin),
        _ => Err(MarkerError::Several),
    }
}

/// `text` with `block` (the lines from the begin marker to the end marker,
/// joined by `\n`, with no final terminator) in place of its block, or
/// appended at its end when `text` holds no marker.
pub(super) fn with_block(text: &str, block: &str) -> Result<String, MarkerError> {
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let block = block.replace('\n', eol);
    Ok(match locate(text)? {
        Some(located) => format!(
            "{}{block}{}",
            &text[..located.block.start],
            &text[located.block.end..]
        ),
        None => {
            let mut out = text.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push_str(eol);
            }
            out.push_str(&block);
            out.push_str(eol);
            out
        }
    })
}

/// `text` without its block, from the begin marker line through the end
/// marker line with its terminator; `None` when `text` holds no marker. The
/// text before and after stays byte for byte.
pub(super) fn without_block(text: &str) -> Result<Option<String>, MarkerError> {
    Ok(locate(text)?.map(|located| {
        format!(
            "{}{}",
            &text[..located.block.start],
            &text[located.line_end..]
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block of three lines, as `render` builds one.
    fn block() -> String {
        format!("{BEGIN}\nnew\n{END}")
    }

    /// A text with no marker has no block.
    #[test]
    fn a_text_without_markers_has_no_block() {
        assert_eq!(locate("one\ntwo\n"), Ok(None));
        assert_eq!(locate(""), Ok(None));
    }

    /// The block runs from the begin line to the end of the end line, and its
    /// terminator is outside it.
    #[test]
    fn a_block_runs_from_the_begin_line_to_the_end_line() {
        let text = format!("a\n{BEGIN}\nold\n{END}\nb\n");
        let located = locate(&text).unwrap().expect("a block");
        assert_eq!(&text[located.block.clone()], format!("{BEGIN}\nold\n{END}"));
        assert_eq!(&text[located.line_end..], "b\n");
    }

    /// Whitespace around a marker does not make it project text.
    #[test]
    fn whitespace_around_a_marker_is_tolerated() {
        let text = format!("  {BEGIN}  \nold\n\t{END} \r\nafter");
        let located = locate(&text).unwrap().expect("a block");
        assert_eq!(
            &text[located.block.clone()],
            format!("  {BEGIN}  \nold\n\t{END} ")
        );
        assert_eq!(&text[located.line_end..], "after");
    }

    /// Text that names a marker inside a longer line is project text. A block
    /// whose end marker carries other text on its line has no end marker.
    #[test]
    fn a_marker_inside_a_longer_line_is_project_text() {
        let text = format!("see `{BEGIN}` and {END} for the block\n");
        assert_eq!(locate(&text), Ok(None));
        let text = format!("x {BEGIN}\ny\n{END} z\n");
        assert_eq!(locate(&text), Ok(None));
        let text = format!("{BEGIN}\ny\ntext {END}\n");
        assert_eq!(locate(&text), Err(MarkerError::MissingEnd));
    }

    /// A marker line inside a fenced code block counts as a marker: the scan
    /// reads lines and models no Markdown. Removing the block leaves the
    /// empty fence.
    #[test]
    fn a_marker_line_inside_a_fenced_code_block_counts_as_a_marker() {
        for (open, close) in [("```md", "```"), ("~~~", "~~~")] {
            let text = format!("a\n{open}\n{BEGIN}\nexample\n{END}\n{close}\nb\n");
            let located = locate(&text).unwrap().expect("a block");
            assert_eq!(
                &text[located.block.clone()],
                format!("{BEGIN}\nexample\n{END}"),
                "{open}"
            );
            assert_eq!(
                without_block(&text).unwrap().as_deref(),
                Some(format!("a\n{open}\n{close}\nb\n").as_str()),
                "{open}"
            );
        }
    }

    /// An end marker on the last line, with no terminator, still closes the
    /// block.
    #[test]
    fn an_end_marker_on_the_last_line_without_a_terminator_closes_the_block() {
        let text = format!("{BEGIN}\nold\n{END}");
        let located = locate(&text).unwrap().expect("a block");
        assert_eq!(located.block, 0..text.len());
        assert_eq!(located.line_end, text.len());
    }

    /// Every shape that is neither no marker nor one block in order is an
    /// error that says what is wrong.
    #[test]
    fn a_broken_shape_is_an_error_that_names_the_fault() {
        let cases: [(&str, MarkerError); 6] = [
            (&format!("{BEGIN}\nx\n"), MarkerError::MissingEnd),
            (&format!("x\n{END}\n"), MarkerError::MissingBegin),
            (&format!("{END}\nx\n{BEGIN}\n"), MarkerError::MissingBegin),
            (
                &format!("{BEGIN}\n{END}\n{BEGIN}\n{END}\n"),
                MarkerError::Several,
            ),
            (&format!("{BEGIN}\n{BEGIN}\n{END}\n"), MarkerError::Several),
            (&format!("{BEGIN}\n{END}\n{END}\n"), MarkerError::Several),
        ];
        for (text, expected) in cases {
            assert_eq!(locate(text), Err(expected), "{text:?}");
        }
    }

    /// The line of each fault names the file and says what to add or keep.
    #[test]
    fn the_error_line_names_the_file() {
        assert_eq!(
            MarkerError::MissingEnd.line("docs/AGENTS.md"),
            "docs/AGENTS.md: the houserules block has no end marker <!-- houserules:end -->; add the end marker after the block"
        );
        assert_eq!(
            MarkerError::MissingBegin.line("AGENTS.md"),
            "AGENTS.md: the houserules end marker has no begin marker before it; add the begin marker before the block"
        );
        assert_eq!(
            MarkerError::Several.line("AGENTS.md"),
            "AGENTS.md: the file holds more than one houserules block; keep one block"
        );
    }

    /// A text with no marker gets the block at its end; a text that lacks a
    /// final terminator gets one first. Removing the block then gives the
    /// project text back, byte for byte, when it ended in a terminator.
    #[test]
    fn a_block_is_appended_at_the_end_and_removing_it_gives_the_text_back() {
        let text = "one\ntwo\n";
        let appended = with_block(text, &block()).unwrap();
        assert_eq!(appended, format!("one\ntwo\n{}\n", block()));
        assert_eq!(without_block(&appended).unwrap().as_deref(), Some(text));

        assert_eq!(
            with_block("no terminator", &block()).unwrap(),
            format!("no terminator\n{}\n", block())
        );
        assert_eq!(with_block("", &block()).unwrap(), format!("{}\n", block()));
    }

    /// A file with CRLF line endings gets a block with CRLF line endings, and
    /// the project text around it stays byte for byte.
    #[test]
    fn a_block_takes_the_line_ending_of_the_file() {
        let text = format!("one\r\n{BEGIN}\r\nold\r\n{END}\r\ntwo\r\n");
        let rewritten = with_block(&text, &block()).unwrap();
        assert_eq!(
            rewritten,
            format!("one\r\n{BEGIN}\r\nnew\r\n{END}\r\ntwo\r\n")
        );
        let appended = with_block("one\r\ntwo\r\n", &block()).unwrap();
        assert_eq!(
            appended,
            format!("one\r\ntwo\r\n{BEGIN}\r\nnew\r\n{END}\r\n")
        );
    }

    /// A block replaces the old one in place and leaves every byte outside it.
    #[test]
    fn a_block_replaces_the_old_one_in_place() {
        let text = format!("top\n\n{BEGIN}\nold\n{END}\n\nbottom");
        assert_eq!(
            with_block(&text, &block()).unwrap(),
            format!("top\n\n{}\n\nbottom", block())
        );
    }

    /// Removing a block takes the lines from the begin marker through the end
    /// marker line and its terminator, in LF and CRLF files alike.
    #[test]
    fn removing_a_block_keeps_the_text_around_it_byte_for_byte() {
        for eol in ["\n", "\r\n"] {
            let before = format!("intro{eol}{eol}text{eol}");
            let after = format!("after{eol}last{eol}");
            let text = format!("{before}{BEGIN}{eol}old{eol}{END}{eol}{after}");
            assert_eq!(
                without_block(&text).unwrap().as_deref(),
                Some(format!("{before}{after}").as_str())
            );
        }
        let at_end = format!("keep\n{BEGIN}\nold\n{END}");
        assert_eq!(without_block(&at_end).unwrap().as_deref(), Some("keep\n"));
    }

    /// A text without a marker has no block to remove; a broken one is an
    /// error.
    #[test]
    fn removing_from_a_text_without_a_block_gives_none() {
        assert_eq!(without_block("plain\n"), Ok(None));
        assert_eq!(
            without_block(&format!("{BEGIN}\n")),
            Err(MarkerError::MissingEnd)
        );
    }
}
