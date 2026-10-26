//! The printing of untrusted text: every value that comes from the
//! repository, a tool, or a request description prints through this module.
//!
//! A single-line value prints as a Markdown code span ([`code_span`]); a
//! multi-line value prints as a fenced block ([`fenced`]). The fence is longer
//! than every backtick run of the value, so a value never closes its own
//! fence, and a `<!--` or a heading inside it never changes the layout of the
//! comment around it (spec 4.8). No control character reaches the output
//! raw: a code span holds a value with one as a JSON string, and a fenced
//! block writes it as a `\u` escape. A caller that must keep the line count of
//! a block exactly builds the block from [`escaped_line`] and [`fence_of`].

/// The length of the longest run of backticks in `value`; 0 when it holds none.
fn longest_backtick_run(value: &str) -> usize {
    value.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// The `\u` escape of the control character `c`.
fn unicode_escape(c: char) -> String {
    format!("\\u{:04x}", u32::from(c))
}

/// `value` as a JSON string, with every control character written as an
/// escape. `serde_json` escapes the characters below U+0020; the delete
/// character and the C1 controls need the `\u` form here. The result is also a
/// YAML double-quoted scalar, which the CI forms of `houserules ci` use for a
/// value that comes from the project.
pub(crate) fn json_string(value: &str) -> String {
    serde_json::to_string(value)
        .expect("a string always serializes")
        .chars()
        .map(|c| {
            if c.is_control() {
                unicode_escape(c)
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// The fence of a block that holds `value`: one backtick longer than the
/// longest backtick run of the value, and at least three backticks long.
pub(crate) fn fence_of(value: &str) -> String {
    "`".repeat((longest_backtick_run(value) + 1).max(3))
}

/// `line` as one line of a block: a tab stays and every other control
/// character is written as a `\u` escape, a lone carriage return included. The
/// result holds no line end, so a block of such lines holds one line for each.
pub(crate) fn escaped_line(line: &str) -> String {
    line.chars()
        .map(|c| {
            if c != '\t' && c.is_control() {
                unicode_escape(c)
            } else {
                c.to_string()
            }
        })
        .collect()
}

/// `value` as a Markdown code span on one line. A value with a control
/// character prints as a JSON string inside the span. The fence is one
/// backtick longer than the longest backtick run of the printed text. A text
/// that starts or ends with a backtick, or that starts and ends with a space,
/// gets one space of padding on each side, so that the span shows the text
/// whole. An empty value prints as a span of one space.
pub(crate) fn code_span(value: &str) -> String {
    let content = if value.chars().any(char::is_control) {
        json_string(value)
    } else {
        value.to_string()
    };
    let fence = "`".repeat(longest_backtick_run(&content) + 1);
    if content.is_empty() {
        return format!("{fence} {fence}");
    }
    let all_spaces = content.chars().all(|c| c == ' ');
    let needs_padding = !all_spaces
        && (content.starts_with('`')
            || content.ends_with('`')
            || (content.starts_with(' ') && content.ends_with(' ')));
    if needs_padding {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

/// `value` as a fenced code block, for a value of several lines. The fence is
/// one backtick longer than the longest backtick run of the value, and at
/// least three backticks long. Every line end becomes a line feed, a tab stays,
/// and every other control character is written as a `\u` escape.
pub(crate) fn fenced(value: &str) -> String {
    let mut body = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                body.push('\n');
            }
            '\n' | '\t' => body.push(c),
            c if c.is_control() => body.push_str(&unicode_escape(c)),
            c => body.push(c),
        }
    }
    let body = body.strip_suffix('\n').unwrap_or(&body);
    let fence = fence_of(body);
    format!("{fence}\n{body}\n{fence}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_value_prints_in_one_backtick() {
        assert_eq!(code_span("crates/a/b.rs"), "`crates/a/b.rs`");
    }

    #[test]
    fn the_fence_of_a_span_is_one_backtick_longer_than_the_longest_run() {
        assert_eq!(code_span("a`b"), "``a`b``");
        assert_eq!(code_span("a``b`c"), "```a``b`c```");
        assert_eq!(code_span("a```b"), "````a```b````");
    }

    #[test]
    fn a_value_that_starts_or_ends_with_a_backtick_gets_one_space_of_padding() {
        assert_eq!(code_span("`"), "`` ` ``");
        assert_eq!(code_span("`a"), "`` `a ``");
        assert_eq!(code_span("a`"), "`` a` ``");
    }

    #[test]
    fn a_value_with_a_space_at_both_ends_keeps_both_spaces() {
        assert_eq!(code_span(" a "), "`  a  `");
        assert_eq!(code_span(" a"), "` a`");
        assert_eq!(code_span("   "), "`   `");
    }

    #[test]
    fn an_empty_value_prints_a_span_of_one_space() {
        assert_eq!(code_span(""), "` `");
    }

    #[test]
    fn a_comment_opener_and_a_heading_stay_inside_the_span() {
        assert_eq!(code_span("x<!--y"), "`x<!--y`");
        assert_eq!(code_span("# heading"), "`# heading`");
    }

    #[test]
    fn a_value_with_a_control_character_prints_as_a_json_string() {
        assert_eq!(code_span("a\nb"), "`\"a\\nb\"`");
        assert_eq!(code_span("tab\there"), "`\"tab\\there\"`");
        assert_eq!(code_span("esc\u{1b}[0m"), "`\"esc\\u001b[0m\"`");
    }

    #[test]
    fn a_delete_and_a_c1_control_character_are_escaped_too() {
        assert_eq!(code_span("a\u{7f}b"), "`\"a\\u007fb\"`");
        assert_eq!(code_span("a\u{85}b"), "`\"a\\u0085b\"`");
    }

    #[test]
    fn the_json_string_of_a_span_sizes_its_fence_by_its_own_backticks() {
        assert_eq!(code_span("a`\nb"), "``\"a`\\nb\"``");
    }

    #[test]
    fn a_quote_and_a_backslash_stay_raw_in_a_value_without_a_control_character() {
        assert_eq!(code_span("say \"hi\" \\ done"), "`say \"hi\" \\ done`");
    }

    #[test]
    fn a_block_has_a_fence_of_three_backticks_for_a_value_with_few() {
        assert_eq!(fenced("one\ntwo"), "```\none\ntwo\n```");
        assert_eq!(fenced("a``b"), "```\na``b\n```");
    }

    #[test]
    fn the_fence_of_a_block_is_one_backtick_longer_than_the_longest_run() {
        assert_eq!(fenced("a```b"), "````\na```b\n````");
        assert_eq!(fenced("a`````b"), "``````\na`````b\n``````");
    }

    #[test]
    fn a_comment_opener_stays_inside_the_block() {
        assert_eq!(
            fenced("<!--\n# heading\n-->"),
            "```\n<!--\n# heading\n-->\n```"
        );
    }

    #[test]
    fn a_block_holds_no_trailing_blank_line_and_normalizes_line_ends() {
        assert_eq!(fenced("a\n"), "```\na\n```");
        assert_eq!(fenced("a\r\nb\rc"), "```\na\nb\nc\n```");
        assert_eq!(fenced(""), "```\n\n```");
    }

    #[test]
    fn the_fence_of_a_value_is_one_backtick_longer_than_its_longest_run_and_at_least_three() {
        assert_eq!(fence_of(""), "```");
        assert_eq!(fence_of("a``b"), "```");
        assert_eq!(fence_of("a```b"), "````");
    }

    #[test]
    fn a_line_of_a_block_keeps_a_tab_and_escapes_every_other_control_character() {
        assert_eq!(escaped_line("a\tb"), "a\tb");
        assert_eq!(escaped_line("50%\r100%"), "50%\\u000d100%");
        assert_eq!(
            escaped_line("a\u{1b}b\u{7f}c\u{85}d"),
            "a\\u001bb\\u007fc\\u0085d"
        );
        assert_eq!(escaped_line("a\nb"), "a\\u000ab");
        assert_eq!(escaped_line("é ```"), "é ```");
    }

    #[test]
    fn a_block_escapes_a_control_character_and_keeps_a_tab() {
        assert_eq!(fenced("a\tb\u{1b}c\u{7f}"), "```\na\tb\\u001bc\\u007f\n```");
    }
}
