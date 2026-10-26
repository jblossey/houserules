//! `houserules record audit` (spec 8.3): the verdict of a person who audited a
//! merged change.
//!
//! The command sets `post_merge_audit` in the note of the record and changes no
//! other byte of it. A note is not written again from the typed record: a job
//! other than `record write` may have edited it (the GitLab job puts
//! `reviewers_more` after `truncated`), and the typed form would move the key
//! and write an escape another way. The command replaces the bytes of the one
//! value instead. It finds the value by walking the top-level object of the
//! note and reading each key as JSON, so a key of a nested object (an area
//! named like the key, for example) and a text inside a string are no key of the
//! record.

use std::ops::Range;
use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use serde::de::{DeserializeOwned, IgnoredAny};

use super::super::change;
use super::super::git;
use super::notes;
use super::{Done, Failure, no_records_line};
use crate::install::marker::Marker;
use crate::published::change_record::{AuditVerdict, PostMergeAudit};

/// The most bytes of `--by`.
const BY_BYTES: usize = 100;

/// The most bytes of `--note`.
const NOTE_BYTES: usize = 1_000;

/// The name of the key whose value the command sets.
const KEY: &str = "post_merge_audit";

/// The verdicts of `--verdict`.
#[derive(Clone, Copy, ValueEnum)]
enum Verdict {
    /// The merged change is sound.
    Pass,
    /// The merged change is not sound.
    Fail,
}

/// The arguments of `houserules record audit`.
#[derive(Args)]
pub(crate) struct AuditArgs {
    /// The commit to audit: a commit that a record keys, or one in the range of
    /// a record.
    #[arg(value_name = "SHA")]
    sha: String,
    /// The verdict of the audit: `pass` or `fail`.
    #[arg(long, value_enum)]
    verdict: Verdict,
    /// The name of the person who audited. The command cuts it to 100 bytes.
    #[arg(long, value_name = "NAME")]
    by: String,
    /// A note on the audit. The command cuts it to 1,000 bytes of UTF-8.
    #[arg(long, value_name = "TEXT")]
    note: Option<String>,
    /// Install root to read from; defaults to the nearest directory at or above
    /// the current one that holds `.houserules.json`, no higher than the git top
    /// level, else the git top level.
    #[arg(long)]
    pub(super) dir: Option<PathBuf>,
}

/// `text` cut to at most `limit` bytes of UTF-8, on a character boundary.
fn cut_bytes(text: &str, limit: usize) -> String {
    text[..text.floor_char_boundary(limit)].to_string()
}

/// The audit that `args` give, with `--by` and `--note` within their caps. An
/// empty `--note` is no note.
fn audit_of(args: &AuditArgs) -> PostMergeAudit {
    PostMergeAudit {
        verdict: match args.verdict {
            Verdict::Pass => AuditVerdict::Pass,
            Verdict::Fail => AuditVerdict::Fail,
        },
        by: cut_bytes(&args.by, BY_BYTES),
        note: args
            .note
            .as_deref()
            .filter(|note| !note.is_empty())
            .map(|note| cut_bytes(note, NOTE_BYTES)),
    }
}

/// Why the walk of the top-level object of a note stopped.
enum Fault {
    /// The bytes are no JSON object: a token is missing or out of place.
    NotAnObject,
    /// The value of the key `post_merge_audit` is no JSON value.
    AuditValue,
}

/// The index of the first byte at or after `at` that is no JSON white space.
fn skip_space(note: &[u8], at: usize) -> usize {
    at + note[at..]
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        .count()
}

/// The JSON value of type `T` that starts at or after `at` in `note`, and the
/// index of the byte after it.
fn read_value<T: DeserializeOwned>(note: &[u8], at: usize) -> Option<(T, usize)> {
    let mut values = serde_json::Deserializer::from_slice(&note[at..]).into_iter::<T>();
    let value = values.next()?.ok()?;
    Some((value, at + values.byte_offset()))
}

/// The byte spans of the values of the top-level keys of `note` that are named
/// `post_merge_audit`, in the order of the note. The walk reads each key as a JSON
/// string and each value as a JSON value, so no text inside a string or a nested
/// object is taken for a key.
fn audit_values(note: &[u8]) -> Result<Vec<Range<usize>>, Fault> {
    let mut spans = Vec::new();
    let mut at = skip_space(note, 0);
    if note.get(at) != Some(&b'{') {
        return Err(Fault::NotAnObject);
    }
    at = skip_space(note, at + 1);
    if note.get(at) == Some(&b'}') {
        return Ok(spans);
    }
    loop {
        let (name, after_name) = read_value::<String>(note, at).ok_or(Fault::NotAnObject)?;
        at = skip_space(note, after_name);
        if note.get(at) != Some(&b':') {
            return Err(Fault::NotAnObject);
        }
        let start = skip_space(note, at + 1);
        let is_audit = name == KEY;
        let end = read_value::<IgnoredAny>(note, start)
            .map(|(_, end)| end)
            .ok_or(if is_audit {
                Fault::AuditValue
            } else {
                Fault::NotAnObject
            })?;
        if is_audit {
            spans.push(start..end);
        }
        at = skip_space(note, end);
        match note.get(at) {
            Some(b',') => at = skip_space(note, at + 1),
            Some(b'}') => return Ok(spans),
            _ => return Err(Fault::NotAnObject),
        }
    }
}

/// The bytes of `note` with the value of its one top-level `post_merge_audit`
/// key replaced by `audit`. Every other byte stays. `key` names the record in the
/// line for a note that is no JSON object, whose audit value is no JSON value, or
/// that has not exactly one such key.
fn with_audit(note: &[u8], key: &str, audit: &PostMergeAudit) -> Result<Vec<u8>, String> {
    let record = git::short(key);
    let spans = audit_values(note).map_err(|fault| match fault {
        Fault::NotAnObject => format!("record: {record}: the note is no JSON object"),
        Fault::AuditValue => {
            format!("record: {record}: the value of post_merge_audit is no JSON value")
        }
    })?;
    let [span] = &spans[..] else {
        return Err(format!(
            "record: {record}: the note does not hold exactly one post_merge_audit key"
        ));
    };
    let mut bytes = note[..span.start].to_vec();
    bytes.extend(serde_json::to_vec(audit).expect("a post-merge audit serializes"));
    bytes.extend_from_slice(&note[span.end..]);
    Ok(bytes)
}

/// Runs `record audit`: sets the audit of the record of the commit.
pub(super) fn run(root: &Path, args: &AuditArgs) -> Result<Done, Failure> {
    if args.by.trim().is_empty() {
        return Err(Failure::from("record audit: --by needs a name".to_string()));
    }
    let sha = git::resolve_commit(root, &args.sha)?;
    let install = change::install_path(root)?;
    if !notes::ref_exists(root, &install)? {
        return Err(Failure::refused(vec![no_records_line(&install)]));
    }
    let Some(record) = notes::read(root, &install, &sha)? else {
        return Err(Failure::refused(vec![format!(
            "record: no record for {}",
            git::short(&sha)
        )]));
    };
    let key = git::short(&record.key);
    if record.post_merge_audit.is_some() {
        return Err(Failure::refused(vec![format!(
            "record: {key} already holds a post-merge audit"
        )]));
    }
    let max_bytes = Marker::read_or_default_for_gate(root, "WI")?
        .review()?
        .max_bytes;
    let note = notes::read_bytes(root, &install, &record.key)?
        .ok_or_else(|| Failure::from(format!("record: {key}: the note of the key is missing")))?;
    let bytes = with_audit(&note, &record.key, &audit_of(args))?;
    let size = bytes.len() as u64;
    if size > max_bytes {
        return Err(Failure::refused(vec![format!(
            "record: the audit would bring {key} to {size} bytes > maxBytes {max_bytes}"
        )]));
    }
    notes::write(root, &install, &record.key, &bytes)?;
    Ok(Done {
        stdout: String::new(),
        stderr: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The audit `pass` by `amy` with the note `note`.
    fn audit(note: Option<&str>) -> PostMergeAudit {
        PostMergeAudit {
            verdict: AuditVerdict::Pass,
            by: "amy".to_string(),
            note: note.map(str::to_string),
        }
    }

    /// The text of `with_audit` for the note `note`.
    fn spliced(note: &str, audit: &PostMergeAudit) -> Result<String, String> {
        with_audit(note.as_bytes(), &"a".repeat(40), audit)
            .map(|bytes| String::from_utf8(bytes).expect("UTF-8"))
    }

    #[test]
    fn the_value_of_the_key_is_replaced_and_every_other_byte_stays() {
        let note = "{\"a\":\"\\u007f\",\"post_merge_audit\":null,\"truncated\":[],\"b\":1.0}\n";

        assert_eq!(
            spliced(note, &audit(Some("ok"))),
            Ok("{\"a\":\"\\u007f\",\"post_merge_audit\":{\"verdict\":\"pass\",\"by\":\"amy\",\"note\":\"ok\"},\"truncated\":[],\"b\":1.0}\n".to_string())
        );
    }

    #[test]
    fn a_value_that_ends_with_a_brace_inside_a_string_is_replaced_whole() {
        let note = "{\"post_merge_audit\":{\"verdict\":\"fail\",\"by\":\"}\",\"note\":null},\"truncated\":[]}\n";

        assert_eq!(
            spliced(note, &audit(None)),
            Ok("{\"post_merge_audit\":{\"verdict\":\"pass\",\"by\":\"amy\",\"note\":null},\"truncated\":[]}\n".to_string())
        );
    }

    #[test]
    fn a_text_that_holds_the_key_inside_a_string_is_no_key() {
        let note = "{\"purpose\":\"\\\"post_merge_audit\\\":\",\"post_merge_audit\":null}\n";

        assert_eq!(
            spliced(note, &audit(None)),
            Ok("{\"purpose\":\"\\\"post_merge_audit\\\":\",\"post_merge_audit\":{\"verdict\":\"pass\",\"by\":\"amy\",\"note\":null}}\n".to_string())
        );
    }

    #[test]
    fn a_note_with_no_key_or_with_two_is_one_named_line() {
        let line = "record: aaaaaaa: the note does not hold exactly one post_merge_audit key";

        assert_eq!(spliced("{\"a\":1}\n", &audit(None)), Err(line.to_string()));
        assert_eq!(spliced("{}\n", &audit(None)), Err(line.to_string()));
        assert_eq!(
            spliced(
                "{\"post_merge_audit\":null,\"x\":1,\"post_merge_audit\":null}\n",
                &audit(None)
            ),
            Err(line.to_string())
        );
    }

    #[test]
    fn only_a_top_level_key_is_the_key() {
        let note = "{\"x\":{\"post_merge_audit\":1,\"y\":[{\"post_merge_audit\":2}]},\"post_merge_audit\":null}\n";

        assert_eq!(
            spliced(note, &audit(None)),
            Ok("{\"x\":{\"post_merge_audit\":1,\"y\":[{\"post_merge_audit\":2}]},\"post_merge_audit\":{\"verdict\":\"pass\",\"by\":\"amy\",\"note\":null}}\n".to_string())
        );
    }

    #[test]
    fn an_area_named_like_the_key_after_an_escaped_quote_is_no_key() {
        let note = "{\"route\":{\"inputs\":{\"areas\":{\"x\\\"post_merge_audit\":{\"paths\":[]}}}},\"post_merge_audit\":null,\"truncated\":[]}\n";

        assert_eq!(
            spliced(note, &audit(None)),
            Ok("{\"route\":{\"inputs\":{\"areas\":{\"x\\\"post_merge_audit\":{\"paths\":[]}}}},\"post_merge_audit\":{\"verdict\":\"pass\",\"by\":\"amy\",\"note\":null},\"truncated\":[]}\n".to_string())
        );
    }

    #[test]
    fn a_top_level_key_is_found_whatever_its_spelling_or_the_white_space_around_it() {
        let spaced = "{ \"a\" : 1 ,\n \"post_merge_audit\" : null ,\n \"b\" : 2 }\n";
        let escaped = "{\"post\\u005fmerge_audit\":null}\n";

        assert_eq!(
            spliced(spaced, &audit(None)),
            Ok("{ \"a\" : 1 ,\n \"post_merge_audit\" : {\"verdict\":\"pass\",\"by\":\"amy\",\"note\":null} ,\n \"b\" : 2 }\n".to_string())
        );
        assert_eq!(
            spliced(escaped, &audit(None)),
            Ok("{\"post\\u005fmerge_audit\":{\"verdict\":\"pass\",\"by\":\"amy\",\"note\":null}}\n".to_string())
        );
    }

    #[test]
    fn a_note_that_is_no_json_object_is_one_named_line() {
        let line = "record: aaaaaaa: the note is no JSON object";

        for note in [
            "",
            "[1]\n",
            "\"post_merge_audit\":null",
            "{\"a\" 1}",
            "{\"a\":1",
            "{1:2}",
        ] {
            assert_eq!(
                spliced(note, &audit(None)),
                Err(line.to_string()),
                "{note:?}"
            );
        }
    }

    #[test]
    fn a_key_with_no_value_is_one_named_line() {
        assert_eq!(
            spliced("{\"post_merge_audit\":", &audit(None)),
            Err("record: aaaaaaa: the value of post_merge_audit is no JSON value".to_string())
        );
    }

    #[test]
    fn a_text_is_cut_on_a_character_boundary() {
        assert_eq!(cut_bytes("日".repeat(40).as_str(), 100), "日".repeat(33));
        assert_eq!(
            cut_bytes("😀".repeat(300).as_str(), 1_000),
            "😀".repeat(250)
        );
        assert_eq!(cut_bytes("abc", 100), "abc");
        assert_eq!(cut_bytes("", 100), "");
    }
}
