//! The printing of untrusted text: every value that comes from the
//! repository, a tool, or a request description prints through this module.
//!
//! A single-line value prints as a Markdown code span ([`code_span`]); a
//! multi-line value prints as a fenced block ([`fenced`]). The fence is longer
//! than every backtick run of the value, so a value never closes its own
//! fence, and a `<!--` or a heading inside it never changes the layout of the
//! comment around it (spec 4.8). No control character reaches the output
//! raw: a code span holds a value with one as a JSON string, and a fenced
//! block writes it as a `\u` escape. A code span holds a value that starts
//! with a double quote as a JSON string too, so a printed value starts with a
//! double quote exactly when it is a JSON string, and the text that spells the
//! JSON form of an escape byte never prints like the byte. A caller that must
//! keep the line count of a block exactly builds the block from
//! [`escaped_line`] and [`fence_of`]. A caller that prints a value inside a
//! line of its own text builds the value with [`plain_or_json_string`], the
//! text of a code span.

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

/// `value` as a JSON string ([`json_string`]) when it holds a control
/// character or starts with a double quote, and as it is otherwise: the text
/// that [`code_span`] puts inside its span. A line of a caller's own text that
/// holds a value prints it so.
///
/// A printed value starts with a double quote exactly when it is a JSON string,
/// so the two forms never meet: the escape byte of `x<ESC>y` prints as
/// `"x\u001by"`, the value `"x\u001by"` prints as `"\"x\\u001by\""`, and the
/// text `x\u001by` prints as it is. A double quote after the first character
/// stays as it is.
pub(crate) fn plain_or_json_string(value: &str) -> String {
    if value.starts_with('"') || value.chars().any(char::is_control) {
        json_string(value)
    } else {
        value.to_string()
    }
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
/// character, or one that starts with a double quote, prints as a JSON string
/// inside the span ([`plain_or_json_string`]), so the text inside the span
/// starts with a double quote exactly when it is a JSON string. The fence is one
/// backtick longer than the longest backtick run of the printed text. A text
/// that starts or ends with a backtick, or that starts and ends with a space,
/// gets one space of padding on each side, so that the span shows the text
/// whole. An empty value prints as a span of one space.
pub(crate) fn code_span(value: &str) -> String {
    let content = plain_or_json_string(value);
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

/// `value` as it is when it holds only ASCII letters, digits, and `-`, as a short
/// sha and a trailer key do, and as a code span for any other text. A token of
/// that alphabet holds no character that Markdown reads, so a sound value keeps
/// its plain form, and a value that came from a note and holds a line end or
/// markup stays inside a span.
pub(crate) fn token_or_code_span(value: &str) -> String {
    if !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        value.to_string()
    } else {
        code_span(value)
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
    fn a_token_prints_as_it_is_and_any_other_text_in_a_code_span() {
        assert_eq!(token_or_code_span("3f9dc04"), "3f9dc04");
        assert_eq!(token_or_code_span("Entire-Checkpoint"), "Entire-Checkpoint");
        assert_eq!(token_or_code_span("<b>x</b"), "`<b>x</b`");
        assert_eq!(token_or_code_span("a b"), "`a b`");
        assert_eq!(
            token_or_code_span("Evil\n## Injected"),
            "`\"Evil\\n## Injected\"`"
        );
        assert_eq!(token_or_code_span(""), "` `");
    }

    #[test]
    fn a_plain_value_prints_as_it_is_and_one_with_a_control_character_as_a_json_string() {
        assert_eq!(plain_or_json_string("file:a.md"), "file:a.md");
        assert_eq!(plain_or_json_string("x\\u001by"), "x\\u001by");
        assert_eq!(plain_or_json_string("x\u{1b}y"), "\"x\\u001by\"");
        assert_eq!(
            plain_or_json_string("a\u{7f}\u{9b}b\tc"),
            "\"a\\u007f\\u009bb\\tc\""
        );
        assert_eq!(plain_or_json_string(""), "");
    }

    #[test]
    fn a_value_that_starts_with_a_quote_prints_as_a_json_string() {
        assert_eq!(plain_or_json_string(r#""x\u001by""#), r#""\"x\\u001by\"""#);
        assert_eq!(plain_or_json_string("\""), r#""\"""#);
        assert_eq!(plain_or_json_string("\"a\" b"), r#""\"a\" b""#);
        assert_ne!(
            plain_or_json_string(r#""x\u001by""#),
            plain_or_json_string("x\u{1b}y")
        );
    }

    #[test]
    fn a_value_with_a_quote_after_its_start_prints_as_it_is() {
        assert_eq!(plain_or_json_string("say \"hi\""), "say \"hi\"");
        assert_eq!(plain_or_json_string("x\""), "x\"");
        assert_eq!(plain_or_json_string(" \"x\""), " \"x\"");
        assert_eq!(plain_or_json_string("\\\"x"), "\\\"x");
    }

    #[test]
    fn a_printed_value_starts_with_a_quote_exactly_when_it_is_a_json_string() {
        for value in [
            "",
            "plain",
            "x\u{1b}y",
            r#""x\u001by""#,
            "\"",
            "a\"b",
            "\"\u{1b}\"",
            " \"",
            "\u{7f}",
        ] {
            let printed = plain_or_json_string(value);

            let decoded = if printed.starts_with('"') {
                serde_json::from_str::<String>(&printed).expect("a JSON string")
            } else {
                printed
            };
            assert_eq!(decoded, value, "{value:?}");
        }
    }

    #[test]
    fn a_value_that_starts_with_a_quote_prints_in_a_span_as_a_json_string() {
        assert_eq!(
            code_span(r#""x\u001by""#),
            format!("`{}`", r#""\"x\\u001by\"""#)
        );
        assert_ne!(code_span(r#""x\u001by""#), code_span("x\u{1b}y"));
        assert_eq!(code_span("\"a`b"), "``\"\\\"a`b\"``");
    }

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
