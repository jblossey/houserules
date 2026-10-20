//! The compression-marker gate: `check-knowledge`, `check-backlog`, and
//! `validate` reject a marker that a context-compression proxy leaves in a
//! text.
//!
//! A proxy such as headroom shortens a long tool result and puts a
//! retrieval marker in its place. A file written from that result holds
//! the marker and not the text, so the file is wrong in a way that no
//! reader sees. The gate reads every string value of a parsed file and
//! never a key. A finding names the file and the field path, in the form
//! `check::validate` uses (`file.entries[2].body[0]`), and the kind of
//! marker. It never prints the marker itself, so a report that pastes the
//! finding stays clean for this gate.
//!
//! One pattern in [`MARKER_PATTERNS`] matches one token with its
//! delimiters, and nothing models the document around it
//! (`quality.lint-the-token-not-the-document`). The first three patterns
//! are the markers of headroom-ai 0.41.0 (its `CCR_RETRIEVAL_MARKER_RE`).
//! A new proxy adds one entry to the list.

use std::sync::LazyLock;

use regress::Regex;
use serde_json::Value;

use super::model::Base;

/// The kind of each marker that the gate rejects, and the pattern that
/// matches the whole token with its delimiters.
const MARKER_PATTERNS: [(&str, &str); 3] = [
    ("headroom retrieve-more", r"Retrieve more: hash="),
    ("headroom retrieve-original", r"Retrieve original: hash="),
    ("headroom ccr", r"<<ccr:[^>]+>>"),
];

/// [`MARKER_PATTERNS`], compiled once.
static MARKERS: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    MARKER_PATTERNS
        .iter()
        .map(|(kind, pattern)| {
            (
                *kind,
                Regex::new(pattern).expect("a marker pattern is a valid regular expression"),
            )
        })
        .collect()
});

/// One finding for each marker kind that `value` holds in a string value.
/// `at` names the file: every finding starts with `at` and the path of the
/// field below it, `.key` for an object member and `[index]` for an array
/// item. A key is never read. The findings come in document order, and
/// within one string in the order of [`MARKER_PATTERNS`].
pub(crate) fn marker_findings(value: &Value, at: &str) -> Vec<String> {
    let mut findings = Vec::new();
    collect(value, at, &mut findings);
    findings
}

/// Appends the findings of `value` at the field path `at` to `findings`.
fn collect(value: &Value, at: &str, findings: &mut Vec<String>) {
    match value {
        Value::String(text) => {
            for (kind, marker) in MARKERS.iter() {
                if marker.find(text).is_some() {
                    findings.push(format!(
                        "{at}: holds a compression marker ({kind}); a context proxy changed \
                         this text, so restore the original"
                    ));
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect(item, &format!("{at}[{index}]"), findings);
            }
        }
        Value::Object(members) => {
            for (key, member) in members {
                collect(member, &format!("{at}.{key}"), findings);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

/// The findings for every `knowledge/` file that `base` loaded, except the
/// archive files, which `check_archive` reads: `schema.json`, `areas.json`,
/// each local topic file, each pack copy, and `overrides.json` when it
/// exists.
pub(crate) fn base_marker_findings(base: &Base) -> Vec<String> {
    let mut findings = marker_findings(&base.schema, "knowledge/schema.json");
    findings.extend(marker_findings(&base.areas_raw, "knowledge/areas.json"));
    for (file, _, content) in &base.topic_files {
        findings.extend(marker_findings(content, file));
    }
    for pack in &base.packs {
        findings.extend(marker_findings(
            &pack.raw,
            &format!("knowledge/packs/{}.json", pack.name),
        ));
    }
    findings.extend(marker_findings(
        &base.overrides_raw,
        "knowledge/overrides.json",
    ));
    findings
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::rules::test_support::{base_with, base_with_packs, entry};

    /// The three tokens, built here so that no file of the tree holds one
    /// outside the pattern list itself.
    fn retrieve_more() -> String {
        format!("{} {}abc123", "Retrieve more:", "hash=")
    }

    fn retrieve_original() -> String {
        format!("{} {}abc123", "Retrieve original:", "hash=")
    }

    fn ccr() -> String {
        format!("{}ref-7{}", "<<ccr:", ">>")
    }

    fn finding(at: &str, kind: &str) -> String {
        format!(
            "{at}: holds a compression marker ({kind}); a context proxy changed this text, so \
             restore the original"
        )
    }

    /// Each token is found alone, with prose around it, and the finding
    /// names the file, the field path, and the kind, never the token.
    #[test]
    fn each_marker_token_is_one_finding_naming_the_field_and_the_kind() {
        let document = json!({
            "entries": [
                {"summary": "fine", "body": ["fine", format!("see {} here", retrieve_more())]},
                {"summary": retrieve_original(), "body": []},
                {"summary": "fine", "body": [format!("a {} b", ccr())]},
            ],
        });
        assert_eq!(
            marker_findings(&document, "knowledge/x.json"),
            [
                finding(
                    "knowledge/x.json.entries[0].body[1]",
                    "headroom retrieve-more"
                ),
                finding(
                    "knowledge/x.json.entries[1].summary",
                    "headroom retrieve-original"
                ),
                finding("knowledge/x.json.entries[2].body[0]", "headroom ccr"),
            ]
        );
    }

    /// A marker in an object key is never found: the gate reads values.
    #[test]
    fn a_marker_inside_a_key_is_not_found() {
        let document = json!({retrieve_more(): "fine", "list": [{ccr(): 1}]});
        assert_eq!(
            marker_findings(&document, "knowledge/x.json"),
            Vec::<String>::new()
        );
    }

    /// One pattern matches one token with its delimiters: a token cut short
    /// of its delimiter, or an empty ccr reference, is no marker.
    #[test]
    fn a_token_without_its_delimiters_is_no_marker() {
        let document = json!([
            "Retrieve more: hash",
            "Retrieve more:hash=abc",
            "retrieve more: hash=abc",
            "Retrieve original hash=abc",
            format!("{}{}", "<<ccr:", ">>"),
            format!("{}x", "<<ccr:"),
            "ccr: abc>>",
        ]);
        assert_eq!(marker_findings(&document, "f"), Vec::<String>::new());
    }

    /// A ccr reference ends at the first `>`: text after it is prose.
    #[test]
    fn a_ccr_reference_may_hold_any_character_but_a_closing_bracket() {
        let document = json!([format!("{}a b/c.d-e{}", "<<ccr:", ">>")]);
        assert_eq!(marker_findings(&document, "f").len(), 1);
    }

    /// A string with two kinds gets one finding for each, in the order of
    /// the pattern list; a repeated token is one finding.
    #[test]
    fn two_kinds_in_one_string_are_two_findings_and_a_repeat_is_one() {
        let text = format!("{} {} {}", retrieve_more(), ccr(), retrieve_more());
        assert_eq!(
            marker_findings(&json!({"k": text}), "f"),
            [
                finding("f.k", "headroom retrieve-more"),
                finding("f.k", "headroom ccr"),
            ]
        );
    }

    /// Numbers, booleans, and `null` are no text.
    #[test]
    fn a_scalar_that_is_no_string_is_never_a_finding() {
        let document = json!({"a": 1, "b": true, "c": null, "d": [2.5, false]});
        assert_eq!(marker_findings(&document, "f"), Vec::<String>::new());
    }

    /// A top-level string value is named by `at` alone.
    #[test]
    fn a_top_level_string_is_named_by_the_file_alone() {
        assert_eq!(
            marker_findings(&Value::String(ccr()), "report.json"),
            [finding("report.json", "headroom ccr")]
        );
    }

    /// The base walk covers every file that `load_base` reads under
    /// `knowledge/`: the schema, `areas.json`, a topic file, a pack copy,
    /// and `overrides.json`. Each finding names its own file.
    #[test]
    fn the_base_walk_covers_every_loaded_knowledge_file() {
        let (dir, _) = base_with_packs(
            &[entry("test.a", json!({"summary": ccr()}))],
            &[(
                "alpha",
                &[entry("alpha.b", json!({"summary": retrieve_more()}))],
            )],
            &["alpha.b"],
        );
        let knowledge = dir.path().join("knowledge");
        std::fs::write(
            knowledge.join("schema.json"),
            serde_json::to_string(&json!({"description": retrieve_original()})).unwrap(),
        )
        .unwrap();
        std::fs::write(
            knowledge.join("areas.json"),
            serde_json::to_string(&json!({"global": {"paths": [], "note": ccr()}})).unwrap(),
        )
        .unwrap();
        let base = crate::rules::model::load_base(dir.path()).expect("loads");
        assert_eq!(
            base_marker_findings(&base),
            [
                finding(
                    "knowledge/schema.json.description",
                    "headroom retrieve-original"
                ),
                finding("knowledge/areas.json.global.note", "headroom ccr"),
                finding("knowledge/test.json.entries[0].summary", "headroom ccr"),
                finding(
                    "knowledge/packs/alpha.json.entries[0].summary",
                    "headroom retrieve-more"
                ),
            ]
        );
    }

    /// The list is headroom-ai 0.41.0's `CCR_RETRIEVAL_MARKER_RE` split into
    /// one pattern per token (spec F8), so an edit that drops a token or
    /// changes its delimiters fails here.
    #[test]
    fn the_pattern_list_is_headrooms_alternation_split_per_token() {
        let joined: Vec<&str> = MARKER_PATTERNS
            .iter()
            .map(|(_, pattern)| *pattern)
            .collect();
        // The message names no token: a failure must not print a marker.
        assert!(
            joined.join("|") == "Retrieve more: hash=|Retrieve original: hash=|<<ccr:[^>]+>>",
            "the pattern list is not headroom's alternation split per token"
        );
    }

    /// A clean base, with a pack copy and an `overrides.json`, has no
    /// finding.
    #[test]
    fn a_clean_base_has_no_finding() {
        let (_dir, base) = base_with_packs(
            &[entry("test.a", json!({}))],
            &[("alpha", &[entry("alpha.b", json!({}))])],
            &["alpha.b"],
        );
        assert_eq!(base_marker_findings(&base), Vec::<String>::new());
        let (_dir, plain) = base_with(&[entry("test.a", json!({}))]);
        assert_eq!(base_marker_findings(&plain), Vec::<String>::new());
    }

    /// A marker in `overrides.json` is found, in a field value an override
    /// writes.
    #[test]
    fn a_marker_in_an_override_field_is_found() {
        let (dir, _) = base_with_packs(&[], &[("alpha", &[entry("alpha.b", json!({}))])], &[]);
        std::fs::write(
            dir.path().join("knowledge/overrides.json"),
            serde_json::to_string(&json!({
                "schema": "houserules/overrides@1.0.0",
                "overrides": [{"id": "alpha.b", "fields": {"summary": ccr()}}],
            }))
            .unwrap(),
        )
        .unwrap();
        let base = crate::rules::model::load_base(dir.path()).expect("loads");
        assert_eq!(
            base_marker_findings(&base),
            [finding(
                "knowledge/overrides.json.overrides[0].fields.summary",
                "headroom ccr"
            )]
        );
    }
}
