//! The houserules block of an issue body (spec 3.5).
//!
//! The GitHub adapter keeps the fields that no issue field holds in a fenced
//! block of the tag `houserules` at the end of the body:
//!
//! ````text
//! The paragraphs of the body.
//!
//! ```houserules
//! key: value
//! list: ["a", "b"]
//! ```
//! ````
//!
//! A line is `key: value`. A string is written as it is; any other value, and
//! a string that would read back as another value, is written as one line of
//! JSON. The adapter reads only the last block, and only at the end of the
//! body: a human-written fence of the same tag inside the body is prose, and so
//! is a fence that text follows. A body saved by the GitHub web form has CRLF
//! line endings; both functions read them as LF.

use std::collections::BTreeMap;

use serde_json::Value;

/// The line that opens the block.
const OPEN: &str = "```houserules";

/// The line that closes any fence.
const CLOSE: &str = "```";

/// Splits an issue `body` into its prose and the fields of its trailing
/// houserules block. A body with no block is all prose and has no field. A
/// block line that is no `key: value` pair is dropped; a value that is no
/// JSON is a string.
pub(super) fn parse(body: &str) -> (String, BTreeMap<String, Value>) {
    let normalized = body.replace("\r\n", "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    match trailing_block(&lines) {
        Some((open, close)) => {
            let prose = lines[..open].join("\n").trim().to_string();
            let fields = lines[open + 1..close]
                .iter()
                .filter_map(|line| field(line))
                .collect();
            (prose, fields)
        }
        None => (normalized.trim().to_string(), BTreeMap::new()),
    }
}

/// Writes `prose` and the block of `fields`: the prose, a blank line, and the
/// block with one line for each field, in key order. With no field the result
/// is the prose alone; with no prose it is the block alone.
pub(super) fn render(prose: &str, fields: &BTreeMap<String, Value>) -> String {
    let mut out = prose.trim().to_string();
    if fields.is_empty() {
        return out;
    }
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(OPEN);
    out.push('\n');
    for (key, value) in fields {
        out.push_str(&format!("{key}: {}\n", encode(value)));
    }
    out.push_str(CLOSE);
    out
}

/// The indexes of the opening and the closing line of the block that ends
/// `lines`: the last non-blank line closes it, and the nearest fence line
/// above must be the opening of the tag.
fn trailing_block(lines: &[&str]) -> Option<(usize, usize)> {
    let close = lines.iter().rposition(|line| !line.trim().is_empty())?;
    if lines[close].trim_end() != CLOSE {
        return None;
    }
    let open = lines[..close]
        .iter()
        .rposition(|line| line.starts_with(CLOSE))?;
    (lines[open].trim_end() == OPEN).then_some((open, close))
}

/// The field that a block `line` holds, if it holds one.
fn field(line: &str) -> Option<(String, Value)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    let value = value.trim();
    let value = serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()));
    Some((key.to_string(), value))
}

/// The text of `value` after the colon of its line.
fn encode(value: &Value) -> String {
    match value {
        Value::String(text) if reads_back_as_text(text) => text.clone(),
        other => serde_json::to_string(other).expect("a JSON value always serializes"),
    }
}

/// `true` when `text` written as it is reads back as the same string: it is
/// not empty, it has no edge space and no line break, and it is not JSON.
fn reads_back_as_text(text: &str) -> bool {
    !text.is_empty()
        && text == text.trim()
        && !text.contains(['\n', '\r'])
        && serde_json::from_str::<Value>(text).is_err()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The fields of `pairs`.
    fn fields(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect()
    }

    /// The prose and the fields of a trailing block read back.
    #[test]
    fn a_trailing_block_is_read_and_the_prose_is_what_comes_before_it() {
        let (prose, found) = parse(
            "One.\n\nTwo.\n\n```houserules\nlabel: Core\nsee: [\"#2\", \"#3\"]\nbatch: 4\n```\n",
        );
        assert_eq!(prose, "One.\n\nTwo.");
        assert_eq!(
            found,
            fields(&[
                ("label", json!("Core")),
                ("see", json!(["#2", "#3"])),
                ("batch", json!(4)),
            ])
        );
    }

    /// A body with no block is all prose, trimmed.
    #[test]
    fn a_body_with_no_block_is_all_prose() {
        assert_eq!(
            parse("\n One.\n\nTwo.\n\n"),
            ("One.\n\nTwo.".to_string(), BTreeMap::new())
        );
        assert_eq!(parse(""), (String::new(), BTreeMap::new()));
    }

    /// Review focus 1: a human-written fence earlier in the body is prose; the
    /// last block is the block.
    #[test]
    fn only_the_last_fence_at_the_end_of_the_body_is_the_block() {
        let body = "```houserules\nlabel: human\n```\n\nText.\n\n```houserules\nlabel: real\n```";
        let (prose, found) = parse(body);
        assert_eq!(found, fields(&[("label", json!("real"))]));
        assert_eq!(prose, "```houserules\nlabel: human\n```\n\nText.");
    }

    /// A fence that text follows, a fence of another tag, and a fence that is
    /// never closed are no block.
    #[test]
    fn a_fence_that_is_not_the_end_of_the_body_is_prose() {
        for body in [
            "```houserules\nlabel: x\n```\n\nAfter.",
            "Text.\n\n```rust\nlet a = 1;\n```",
            "Text.\n\n```houserules\nlabel: x",
            "Text.\n\n```houserules\nlabel: x\n```\n```",
        ] {
            let (prose, found) = parse(body);
            assert!(found.is_empty(), "{body:?}");
            assert_eq!(prose, body.trim(), "{body:?}");
        }
    }

    /// CRLF line endings and trailing spaces read the same as LF.
    #[test]
    fn crlf_line_endings_and_trailing_spaces_read_like_lf() {
        let (prose, found) =
            parse("One.\r\n\r\nTwo.\r\n\r\n```houserules  \r\nlabel: Core\r\n```  \r\n\r\n");
        assert_eq!(prose, "One.\n\nTwo.");
        assert_eq!(found, fields(&[("label", json!("Core"))]));
    }

    /// A line that is no `key: value` pair is dropped, and a value that is no
    /// JSON is a string, colons and all.
    #[test]
    fn a_line_that_is_no_pair_is_dropped_and_a_plain_value_is_a_string() {
        let (_, found) = parse(
            "```houserules\nno pair here\n: nokey\ntwo words: x\nlink: see https://example.test/a\nempty:\n```",
        );
        assert_eq!(
            found,
            fields(&[
                ("link", json!("see https://example.test/a")),
                ("empty", json!("")),
            ])
        );
    }

    /// The block is the prose, a blank line, and one line for each field in
    /// key order.
    #[test]
    fn render_writes_the_prose_and_the_block_in_key_order() {
        let out = render(
            "Needs a design.\n",
            &fields(&[
                (
                    "deferral",
                    json!({"reason": "r", "origin": "o", "files": ["a"]}),
                ),
                ("label", json!("Core")),
            ]),
        );
        assert_eq!(
            out,
            "Needs a design.\n\n```houserules\ndeferral: {\"reason\":\"r\",\"origin\":\"o\",\"files\":[\"a\"]}\nlabel: Core\n```"
        );
    }

    /// With no field the result is the prose; with no prose it is the block.
    #[test]
    fn render_with_no_field_is_the_prose_and_with_no_prose_is_the_block() {
        assert_eq!(render(" Text. ", &BTreeMap::new()), "Text.");
        assert_eq!(
            render("", &fields(&[("label", json!("x"))])),
            "```houserules\nlabel: x\n```"
        );
        assert_eq!(render("", &BTreeMap::new()), "");
    }

    /// Every value reads back as it was written, including the strings that
    /// would read as another value or that a line cannot hold.
    #[test]
    fn every_value_reads_back_as_written() {
        let values = fields(&[
            ("a_plain", json!("Core work")),
            ("b_true", json!("true")),
            ("c_number", json!("123")),
            ("d_quoted", json!("\"x\"")),
            ("e_padded", json!(" padded ")),
            ("f_empty", json!("")),
            ("g_lines", json!("one\ntwo")),
            ("h_colon", json!("a: b")),
            ("i_list", json!(["a", 1, null, {"k": "v"}])),
            ("j_number", json!(7)),
            ("k_bool", json!(false)),
            ("l_null", Value::Null),
            ("m_unicode", json!("Zeit für Änderungen")),
        ]);
        let (prose, back) = parse(&render("Prose.", &values));
        assert_eq!(prose, "Prose.");
        assert_eq!(back, values);
    }
}
