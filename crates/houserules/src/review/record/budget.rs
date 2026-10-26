//! The size budget of a change record (spec 8.4).
//!
//! A record always fits `review.record.maxBytes`, so every push gets a record.
//! [`fit`] shortens a record that is over its budget through the steps of 8.4
//! in order, each step only while the record is still over, and names the field
//! of each step that changed something in `truncated`. When the steps do not
//! suffice, the record becomes the minimal record, which holds only fields with
//! a fixed cap, and `truncated` is `["all"]`.
//!
//! The size of a record is the length of its note: compact JSON and a line
//! feed ([`size`]). A text is cut on a character boundary. A cap in
//! characters counts characters; a cap in bytes counts the bytes of the text
//! once serialized as a JSON string, an escaped control character included,
//! without the quotes.

use indexmap::IndexMap;

use super::document;
use crate::published::change::{ChangeKind, LineRange};
use crate::published::change_record::{
    Artifacts, Audit, ChangeRecord, Commits, CommitsSummary, IntentName, Intents, PathList,
    PostMergeAudit, Ranges, RecordBundle, RecordRoute, Rules, UncoveredPaths,
};
use crate::published::route::Reason;

/// The bytes that `--reviewers-pending` leaves free for the names that a later
/// job may set (spec 8.3): the budget of the note is `maxBytes` less this.
pub(crate) const PENDING_RESERVE: u64 = 4_096;

/// The most characters of a purpose, a criterion, a budget reason, and a why
/// once the budget cuts them.
const TEXT_CHARS: usize = 1_000;

/// The most work items that a cut list keeps.
const WORK_ITEMS_KEPT: usize = 50;

/// The most sessions that a cut list keeps.
const SESSIONS_KEPT: usize = 20;

/// The most runs that a cut bundle keeps.
const RUNS_KEPT: usize = 20;

/// The most paths that a cut reason keeps.
const REASON_PATHS_KEPT: usize = 20;

/// The fewest commits of a range that a cut turns into a summary. A summary of
/// fewer commits is longer than the list.
const COMMITS_CUT_FROM: usize = 3;

/// The most intents that the minimal record keeps.
const MINIMAL_INTENTS: usize = 10;

/// The most serialized bytes of the `ref` of an intent in the minimal record.
/// With ten refs, the other caps of the minimal record, and a post-merge audit
/// at its caps, the record then takes at most 4,096 bytes: the smallest
/// `maxBytes`, 8,192, less [`PENDING_RESERVE`].
const REF_BYTES: usize = 150;

/// The most serialized bytes of the `by` of a post-merge audit in the minimal
/// record.
const BY_BYTES: usize = 100;

/// The most serialized bytes of the `note` of a post-merge audit in the minimal
/// record.
const NOTE_BYTES: usize = 1_000;

/// The kinds of change in the order of spec 4.2, each with its word.
const KINDS: [(ChangeKind, &str); 8] = [
    (ChangeKind::Text, "text"),
    (ChangeKind::Deleted, "deleted"),
    (ChangeKind::Renamed, "renamed"),
    (ChangeKind::Empty, "empty"),
    (ChangeKind::Binary, "binary"),
    (ChangeKind::Mode, "mode"),
    (ChangeKind::Symlink, "symlink"),
    (ChangeKind::Submodule, "submodule"),
];

/// The bytes of the note of `record`.
pub(crate) fn size(record: &ChangeRecord) -> u64 {
    document::bytes(record).len() as u64
}

/// One step of the cascade: it shortens the record and gives the names of the
/// fields that it changed, none when it changed nothing.
type Step = fn(&mut ChangeRecord) -> Vec<&'static str>;

/// The steps 1 to 7 of spec 8.4, in order.
const STEPS: [Step; 7] = [
    cut_ranges,
    cut_row_evidence,
    cut_uncovered_paths,
    cut_commits,
    cut_route,
    cut_audit_rows,
    cut_texts,
];

/// `record` within `max_bytes` less `reserve` (spec 8.4). The steps run in
/// order and each runs only while the note is still over the budget; each step
/// that changed the record adds the field it changed to `truncated`. A record
/// that the steps leave over the budget becomes the minimal record.
pub(crate) fn fit(mut record: ChangeRecord, max_bytes: u64, reserve: u64) -> ChangeRecord {
    let budget = max_bytes.saturating_sub(reserve);
    for step in STEPS {
        if size(&record) <= budget {
            return record;
        }
        let names = step(&mut record);
        record
            .truncated
            .extend(names.into_iter().map(str::to_string));
    }
    if size(&record) <= budget {
        record
    } else {
        minimal(record)
    }
}

// ---- the steps ------------------------------------------------------------------------

/// The lines that `ranges` touch: the lines of each range, and the lines that
/// a hunk deleted.
fn lines_of(ranges: &[LineRange]) -> u64 {
    ranges
        .iter()
        .map(|range| match range {
            LineRange::Lines { start, end } => end.saturating_sub(*start) + 1,
            LineRange::DeletedAfter { deleted, .. } => *deleted,
        })
        .sum()
}

/// Step 1: the ranges of each uncovered path become their line count.
fn cut_ranges(record: &mut ChangeRecord) -> Vec<&'static str> {
    let Some(evidence) = &mut record.evidence else {
        return Vec::new();
    };
    let UncoveredPaths::List(paths) = &mut evidence.uncovered.paths else {
        return Vec::new();
    };
    let mut changed = false;
    for path in paths {
        let Ranges::List(ranges) = &path.ranges else {
            continue;
        };
        if !ranges.is_empty() {
            path.ranges = Ranges::Count(lines_of(ranges));
            changed = true;
        }
    }
    if changed {
        vec!["evidence.uncovered.paths.ranges"]
    } else {
        Vec::new()
    }
}

/// Step 2: the evidence text of each passing and each judged row goes.
fn cut_row_evidence(record: &mut ChangeRecord) -> Vec<&'static str> {
    let Some(evidence) = &mut record.evidence else {
        return Vec::new();
    };
    let mut changed = false;
    for row in &mut evidence.proven {
        changed |= row.evidence.take().is_some();
    }
    for row in &mut evidence.judged {
        changed |= row.evidence.take().is_some();
    }
    if changed {
        vec!["evidence.rows.evidence"]
    } else {
        Vec::new()
    }
}

/// Step 3: the uncovered paths become a count for each kind.
fn cut_uncovered_paths(record: &mut ChangeRecord) -> Vec<&'static str> {
    let Some(evidence) = &mut record.evidence else {
        return Vec::new();
    };
    let UncoveredPaths::List(paths) = &evidence.uncovered.paths else {
        return Vec::new();
    };
    if paths.is_empty() {
        return Vec::new();
    }
    let mut counts = IndexMap::new();
    for (kind, word) in KINDS {
        let count = paths.iter().filter(|path| path.kind == kind).count() as u64;
        if count > 0 {
            counts.insert(word.to_string(), count);
        }
    }
    evidence.uncovered.paths = UncoveredPaths::Counts(counts);
    vec!["evidence.uncovered.paths"]
}

/// Step 4: a range of three commits or more becomes its count, its first
/// commit, and its last.
fn cut_commits(record: &mut ChangeRecord) -> Vec<&'static str> {
    let Some(Commits::List(commits)) = &record.commits else {
        return Vec::new();
    };
    if commits.len() < COMMITS_CUT_FROM {
        return Vec::new();
    }
    let summary = CommitsSummary {
        count: commits.len() as u64,
        first: commits[0].clone(),
        last: commits[commits.len() - 1].clone(),
    };
    record.commits = Some(Commits::Summary(summary));
    vec!["commits"]
}

/// Turns the list `list` into its count. Returns `true` when it changed.
fn count_paths(list: &mut PathList) -> bool {
    let PathList::List(paths) = list else {
        return false;
    };
    if paths.is_empty() {
        return false;
    }
    let count = paths.len() as u64;
    *list = PathList::Count(count);
    true
}

/// Cuts the paths of `reason` to the first [`REASON_PATHS_KEPT`]. Returns
/// `true` when it changed.
fn cut_reason(reason: &mut Reason) -> bool {
    let Some(removed) = reason.paths.len().checked_sub(REASON_PATHS_KEPT) else {
        return false;
    };
    if removed == 0 {
        return false;
    }
    reason.paths.truncate(REASON_PATHS_KEPT);
    reason.more_paths += removed as u64;
    true
}

/// Step 5: the path lists of the route inputs become counts, and each reason
/// keeps its first 20 paths.
fn cut_route(record: &mut ChangeRecord) -> Vec<&'static str> {
    let mut names = Vec::new();
    if let Some(inputs) = &mut record.route.inputs {
        let mut changed = false;
        for list in inputs.areas.values_mut() {
            changed |= count_paths(list);
        }
        changed |= count_paths(&mut inputs.no_area);
        for entry in &mut inputs.paths {
            changed |= count_paths(&mut entry.paths);
        }
        changed |= count_paths(&mut inputs.rule_sources_changed);
        if changed {
            names.push("route.inputs");
        }
    }
    if let Some(reasons) = &mut record.route.reasons {
        let mut changed = false;
        for reason in reasons {
            changed |= cut_reason(reason);
        }
        if changed {
            names.push("route.reasons");
        }
    }
    names
}

/// Step 6: the rows of the audit go; the summary stays.
fn cut_audit_rows(record: &mut ChangeRecord) -> Vec<&'static str> {
    match &record.audit.rules {
        Some(rows) if !rows.is_empty() => {
            record.audit.rules = None;
            vec!["audit.rules"]
        }
        _ => Vec::new(),
    }
}

/// Cuts `text` to `limit` characters. Returns `true` when it changed.
fn cut_chars(text: &mut String, limit: usize) -> bool {
    match text.char_indices().nth(limit) {
        Some((end, _)) => {
            text.truncate(end);
            true
        }
        None => false,
    }
}

/// Cuts `list` to its first `kept` items and adds the number of items that it
/// loses to `more`. Returns `true` when it changed.
fn cut_list<T>(list: &mut Vec<T>, more: &mut Option<u64>, kept: usize) -> bool {
    let Some(removed) = list.len().checked_sub(kept).filter(|removed| *removed > 0) else {
        return false;
    };
    list.truncate(kept);
    *more = Some(more.unwrap_or(0) + removed as u64);
    true
}

/// Cuts the texts and lists of `bundle`. Returns `true` when it changed.
fn cut_bundle(bundle: &mut RecordBundle) -> bool {
    let mut changed = cut_chars(&mut bundle.purpose, TEXT_CHARS);
    changed |= cut_chars(&mut bundle.criterion, TEXT_CHARS);
    changed |= cut_chars(&mut bundle.budget_reason, TEXT_CHARS);
    changed |= cut_list(
        &mut bundle.work_items,
        &mut bundle.work_items_more,
        WORK_ITEMS_KEPT,
    );
    changed |= cut_list(&mut bundle.runs, &mut bundle.runs_more, RUNS_KEPT);
    changed
}

/// Step 7: the why of each intent, and the purpose, the criterion, and the
/// budget reason of the bundle, go to 1,000 characters; the artifacts of an
/// intent become a count; the work items, the runs, and the sessions go to their
/// first 50, 20, and 20 and a count.
fn cut_texts(record: &mut ChangeRecord) -> Vec<&'static str> {
    let mut names = Vec::new();
    if let Intents::Full(intents) = &mut record.intent {
        let mut changed = false;
        for intent in intents {
            if let Some(why) = &mut intent.why {
                changed |= cut_chars(why, TEXT_CHARS);
            }
            if let Artifacts::List(paths) = &intent.artifacts
                && !paths.is_empty()
            {
                intent.artifacts = Artifacts::Count(paths.len() as u64);
                changed = true;
            }
        }
        if changed {
            names.push("intent");
        }
    }
    if let Some(evidence) = &mut record.evidence
        && cut_list(
            &mut evidence.work_items,
            &mut evidence.work_items_more,
            WORK_ITEMS_KEPT,
        )
    {
        names.push("evidence.work_items");
    }
    if let Some(Some(bundle)) = &mut record.bundle
        && cut_bundle(bundle)
    {
        names.push("bundle");
    }
    if let Some(links) = &mut record.links {
        let mut changed = cut_list(
            &mut links.work_items,
            &mut links.work_items_more,
            WORK_ITEMS_KEPT,
        );
        changed |= cut_list(&mut links.sessions, &mut links.sessions_more, SESSIONS_KEPT);
        if changed {
            names.push("links");
        }
    }
    names
}

// ---- the minimal record ---------------------------------------------------------------

/// The bytes that the serialized form of the character `c` takes inside a JSON
/// string, without the quotes.
fn escaped_width(c: char) -> usize {
    serde_json::to_string(&c)
        .expect("a character serializes")
        .len()
        - 2
}

/// The longest prefix of `text` that takes at most `limit` bytes once
/// serialized as a JSON string, without the quotes. It ends on a character
/// boundary.
fn cut_serialized(text: &str, limit: usize) -> String {
    let mut used = 0;
    for (index, c) in text.char_indices() {
        used += escaped_width(c);
        if used > limit {
            return text[..index].to_string();
        }
    }
    text.to_string()
}

/// The post-merge audit `audit` with its `by` and `note` within their caps.
fn cut_audit(audit: PostMergeAudit) -> PostMergeAudit {
    PostMergeAudit {
        verdict: audit.verdict,
        by: cut_serialized(&audit.by, BY_BYTES),
        note: audit.note.map(|note| cut_serialized(&note, NOTE_BYTES)),
    }
}

/// The names of the intents of `intents`: the first ten, each ref within its
/// cap.
fn intent_names(intents: Intents) -> Vec<IntentName> {
    let named: Vec<IntentName> = match intents {
        Intents::Full(list) => list
            .into_iter()
            .map(|intent| IntentName {
                source: intent.source,
                reference: intent.reference,
            })
            .collect(),
        Intents::Refs(list) => list,
    };
    named
        .into_iter()
        .take(MINIMAL_INTENTS)
        .map(|name| IntentName {
            source: name.source,
            reference: cut_serialized(&name.reference, REF_BYTES),
        })
        .collect()
}

/// Step 8: the minimal record. It keeps the fields that name the range, the
/// rules (without the packs), the audit summary, the intent names, the route
/// and the floor flag, the sample, the reviewers, and the post-merge audit; every
/// other field goes, and `truncated` is `["all"]`.
fn minimal(record: ChangeRecord) -> ChangeRecord {
    ChangeRecord {
        schema: record.schema,
        install: record.install,
        key: record.key,
        base: record.base,
        head: record.head,
        merge_base: record.merge_base,
        commits: None,
        written_by: record.written_by,
        host: record.host,
        rules: Rules {
            packs: Vec::new(),
            ..record.rules
        },
        audit: Audit {
            summary: record.audit.summary,
            rules: None,
            references: None,
            error: None,
        },
        intent: Intents::Refs(intent_names(record.intent)),
        evidence: None,
        route: RecordRoute {
            schema: None,
            install: None,
            head: None,
            route: record.route.route,
            floor: record.route.floor,
            inputs: None,
            reasons: None,
        },
        sample: record.sample,
        bundle: None,
        reviewers: record.reviewers,
        reviewers_more: record.reviewers_more,
        reviewers_pending: record.reviewers_pending,
        links: None,
        post_merge_audit: record.post_merge_audit.map(cut_audit),
        truncated: vec!["all".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::review::record::fixtures::{
        BASE, COMMITS, HEAD_HASH, KEY, full, json_of, minimal, parsed,
    };

    /// The bytes of the note of `value`: its compact JSON and a line feed.
    fn size_of(value: &Value) -> u64 {
        size(&parsed(value))
    }

    /// `value` fitted to `max_bytes` with no reserve, as JSON.
    fn fitted(value: &Value, max_bytes: u64) -> Value {
        json_of(&fit(parsed(value), max_bytes, 0))
    }

    /// `value` fitted to one byte under its own size: the first step that
    /// changes it applies, and no later step does.
    fn one_byte_under(value: &Value) -> Value {
        fitted(value, size_of(value) - 1)
    }

    /// The length of `text` in bytes once serialized as a JSON string, without
    /// the quotes.
    fn escaped_len(text: &str) -> usize {
        serde_json::to_string(text)
            .expect("a string serializes")
            .len()
            - 2
    }

    /// `value` with every list that steps 1 to 6 shorten empty or short, so
    /// that none of these steps changes it.
    fn quiet(mut value: Value) -> Value {
        value["evidence"]["uncovered"]["paths"] = json!([]);
        value["evidence"]["uncovered"]["lines"] = json!(0);
        value["evidence"]["uncovered"]["total_lines"] = json!(0);
        value["evidence"]["uncovered"]["share"] = json!(0.0);
        value["evidence"]["proven"] = json!([{"id": "team.ok"}]);
        value["evidence"]["judged"] =
            json!([{"id": "team.judged", "verdict": "pass", "evidence": null}]);
        value["commits"] = json!([KEY]);
        value["route"]["inputs"]["areas"] = json!({});
        value["route"]["inputs"]["no_area"] = json!([]);
        value["route"]["inputs"]["paths"] = json!([]);
        value["route"]["inputs"]["rule_sources_changed"] = json!([]);
        value["route"]["reasons"][0]["paths"] = json!([]);
        value["route"]["reasons"][0]["more_paths"] = json!(0);
        value["audit"]["rules"] = json!([]);
        value["intent"][0]["artifacts"] = json!([]);
        value
    }

    /// `count` ids `WI-<n>`.
    fn ids(count: usize) -> Vec<String> {
        (0..count).map(|n| format!("WI-{n}")).collect()
    }

    // ---- the budget -----------------------------------------------------------------------

    #[test]
    fn the_size_of_a_record_is_its_compact_json_and_a_line_feed() {
        let value = minimal(KEY);

        let text = serde_json::to_string(&value).expect("serialize");

        assert_eq!(size_of(&value), text.len() as u64 + 1);
    }

    #[test]
    fn a_record_within_the_budget_is_returned_whole() {
        let value = full(3);

        assert_eq!(fitted(&value, size_of(&value)), value);
        assert_eq!(fitted(&value, size_of(&value) + 1_000), value);
    }

    #[test]
    fn the_reserve_counts_against_the_budget() {
        let value = full(3);
        let size = size_of(&value);

        assert_eq!(json_of(&fit(parsed(&value), size + 4_096, 4_096)), value);
        assert_ne!(json_of(&fit(parsed(&value), size + 4_095, 4_096)), value);
    }

    // ---- the cuts, one at a time ----------------------------------------------------------

    #[test]
    fn step_1_cuts_the_ranges_of_each_uncovered_path_to_their_line_count() {
        let value = full(3);

        let result = one_byte_under(&value);

        let mut expected = value.clone();
        for n in 0..3 {
            expected["evidence"]["uncovered"]["paths"][n]["ranges"] = json!(5);
        }
        expected["truncated"] = json!(["evidence.uncovered.paths.ranges"]);
        assert_eq!(result, expected, "lines 1 to 3 and 2 deleted lines make 5");
    }

    #[test]
    fn step_2_cuts_the_evidence_text_of_passing_and_judged_rows() {
        let mut value = quiet(full(0));
        value["evidence"]["failed"] = json!([{"id": "team.lint", "evidence": "f".repeat(100)}]);
        value["evidence"]["proven"] = Value::Array(
            (0..10)
                .map(|n| json!({"id": format!("team.p{n}"), "evidence": "p".repeat(100)}))
                .collect(),
        );
        value["evidence"]["judged"] = Value::Array(
            (0..10)
                .map(|n| {
                    json!({"id": format!("team.j{n}"), "verdict": "fail", "evidence": "j".repeat(100)})
                })
                .collect(),
        );

        let result = one_byte_under(&value);

        let mut expected = value.clone();
        expected["evidence"]["proven"] = Value::Array(
            (0..10)
                .map(|n| json!({"id": format!("team.p{n}")}))
                .collect(),
        );
        expected["evidence"]["judged"] = Value::Array(
            (0..10)
                .map(|n| json!({"id": format!("team.j{n}"), "verdict": "fail", "evidence": null}))
                .collect(),
        );
        expected["truncated"] = json!(["evidence.rows.evidence"]);
        assert_eq!(result, expected, "failed rows keep their evidence");
    }

    #[test]
    fn step_3_cuts_the_uncovered_paths_to_a_count_for_each_kind() {
        let mut value = quiet(full(0));
        let kinds = ["mode", "text", "binary", "text", "mode", "mode"];
        value["evidence"]["uncovered"]["paths"] = Value::Array(
            kinds
                .iter()
                .enumerate()
                .map(|(n, kind)| {
                    json!({"path": format!("f{n}"), "old_path": null, "kind": kind, "ranges": []})
                })
                .collect(),
        );

        let result = one_byte_under(&value);

        let mut expected = value.clone();
        expected["evidence"]["uncovered"]["paths"] = json!({"text": 2, "binary": 1, "mode": 3});
        expected["truncated"] = json!(["evidence.uncovered.paths"]);
        assert_eq!(result, expected, "the kinds come in the order of spec 4.2");
        assert_eq!(
            result["evidence"]["uncovered"]["paths"]
                .as_object()
                .expect("an object")
                .keys()
                .collect::<Vec<_>>(),
            ["text", "binary", "mode"]
        );
    }

    #[test]
    fn step_4_cuts_the_commits_to_their_count_and_the_first_and_the_last() {
        let mut value = quiet(full(0));
        let commits: Vec<String> = (0..10).map(|n| format!("{n:040x}")).collect();
        value["commits"] = json!(commits);

        let result = one_byte_under(&value);

        let mut expected = value.clone();
        expected["commits"] = json!({"count": 10, "first": commits[0], "last": commits[9]});
        expected["truncated"] = json!(["commits"]);
        assert_eq!(result, expected);
    }

    #[test]
    fn step_4_leaves_a_list_of_two_commits_alone() {
        let mut value = quiet(full(0));
        value["commits"] = json!([COMMITS[0], COMMITS[3]]);
        value["bundle"]["purpose"] = json!("p".repeat(5_000));

        let result = one_byte_under(&value);

        assert_eq!(result["commits"], value["commits"]);
        assert_eq!(result["truncated"], json!(["bundle"]));
    }

    #[test]
    fn step_5_cuts_the_path_lists_of_the_route_inputs_to_counts() {
        let mut value = quiet(full(0));
        let names: Vec<String> = (0..25).map(|n| format!("src/f{n}.rs")).collect();
        value["route"]["inputs"]["areas"] = json!({"code": names, "docs": ["docs/a.md"]});
        value["route"]["inputs"]["no_area"] = json!(["a", "b", "c"]);
        value["route"]["inputs"]["paths"] =
            json!([{"glob": "src/**", "route": "owner-review", "paths": names}]);
        value["route"]["inputs"]["rule_sources_changed"] = json!([".houserules.json"]);

        let result = one_byte_under(&value);

        let mut expected = value.clone();
        expected["route"]["inputs"]["areas"] = json!({"code": 25, "docs": 1});
        expected["route"]["inputs"]["no_area"] = json!(3);
        expected["route"]["inputs"]["paths"] =
            json!([{"glob": "src/**", "route": "owner-review", "paths": 25}]);
        expected["route"]["inputs"]["rule_sources_changed"] = json!(1);
        expected["truncated"] = json!(["route.inputs"]);
        assert_eq!(
            result, expected,
            "the ids of failed and warned rows are no path lists and stay"
        );
    }

    #[test]
    fn step_5_cuts_the_paths_of_each_reason_to_the_first_20() {
        let mut value = quiet(full(0));
        let names: Vec<String> = (0..25).map(|n| format!("src/f{n:02}.rs")).collect();
        value["route"]["reasons"][0]["paths"] = json!(names);
        value["route"]["reasons"][0]["more_paths"] = json!(3);

        let result = one_byte_under(&value);

        assert_eq!(
            result["route"]["reasons"][0]["paths"],
            json!(names[..20].to_vec())
        );
        assert_eq!(result["route"]["reasons"][0]["more_paths"], json!(8));
        assert_eq!(result["truncated"], json!(["route.reasons"]));
    }

    #[test]
    fn step_6_cuts_the_audit_rows_to_the_summary() {
        let mut value = quiet(full(0));
        value["audit"]["rules"] = Value::Array(
            (0..20)
                .map(|n| json!({"id": format!("team.r{n}"), "mode": "deterministic", "result": "pass"}))
                .collect(),
        );

        let result = one_byte_under(&value);

        let mut expected = value.clone();
        expected["audit"] = json!({
            "summary": value["audit"]["summary"],
            "references": value["audit"]["references"],
            "error": null,
        });
        expected["truncated"] = json!(["audit.rules"]);
        assert_eq!(
            result, expected,
            "the summary, the references, and the error stay"
        );
    }

    #[test]
    fn step_7_cuts_the_texts_of_the_bundle_to_1000_characters() {
        let mut value = quiet(full(0));
        value["bundle"]["purpose"] = json!("é".repeat(100_000));
        value["bundle"]["criterion"] = json!("c".repeat(2_000));
        value["bundle"]["budget_reason"] = json!("b".repeat(1_001));
        value["bundle"]["tier"] = json!("t".repeat(1_500));

        let result = one_byte_under(&value);

        assert_eq!(result["bundle"]["purpose"], json!("é".repeat(1_000)));
        assert_eq!(result["bundle"]["criterion"], json!("c".repeat(1_000)));
        assert_eq!(result["bundle"]["budget_reason"], json!("b".repeat(1_000)));
        assert_eq!(
            result["bundle"]["tier"], value["bundle"]["tier"],
            "only the purpose, the criterion, and the budget reason are cut"
        );
        assert_eq!(result["truncated"], json!(["bundle"]));
    }

    #[test]
    fn step_7_cuts_the_why_of_each_intent_and_its_artifacts_to_a_count() {
        let mut value = quiet(full(0));
        value["intent"][0]["why"] = json!("w".repeat(5_000));
        value["intent"][0]["artifacts"] = json!(["docs/plans/a.md", "docs/plans/b.md"]);
        value["intent"].as_array_mut().expect("a list").push(json!({
            "source": "work-item", "ref": "WI-404", "title": null, "why": null,
            "tasks": null, "requirements": null, "validation": null,
            "artifacts": [], "unresolved": "the tracker holds no such work item",
        }));

        let result = one_byte_under(&value);

        let mut expected = value.clone();
        expected["intent"][0]["why"] = json!("w".repeat(1_000));
        expected["intent"][0]["artifacts"] = json!(2);
        expected["truncated"] = json!(["intent"]);
        assert_eq!(
            result, expected,
            "an intent with no artifacts keeps its empty list"
        );
    }

    #[test]
    fn step_7_cuts_the_lists_to_the_first_50_and_a_count() {
        let mut value = quiet(full(0));
        value["bundle"]["work_items"] = json!(ids(120));
        value["links"]["work_items"] = json!(ids(120));
        value["evidence"]["work_items"] = json!(ids(120));
        let run =
            json!({"argv": ["mise", "run", "test"], "exit": 0, "head": KEY, "sha256": HEAD_HASH});
        value["bundle"]["runs"] = Value::Array(vec![run; 30]);

        let result = one_byte_under(&value);

        assert_eq!(result["bundle"]["work_items"], json!(ids(50)));
        assert_eq!(result["bundle"]["work_items_more"], json!(70));
        assert_eq!(result["links"]["work_items"], json!(ids(50)));
        assert_eq!(result["links"]["work_items_more"], json!(70));
        assert_eq!(result["evidence"]["work_items"], json!(ids(50)));
        assert_eq!(result["evidence"]["work_items_more"], json!(70));
        assert_eq!(result["bundle"]["runs"].as_array().expect("runs").len(), 20);
        assert_eq!(result["bundle"]["runs_more"], json!(10));
        assert_eq!(
            result["truncated"],
            json!(["evidence.work_items", "bundle", "links"]),
            "the names follow the keys of the record"
        );
    }

    #[test]
    fn step_7_keeps_a_list_of_50_whole_and_counts_the_one_past_it() {
        let mut value = quiet(full(0));
        value["links"]["work_items"] = json!(ids(50));
        value["bundle"]["work_items"] = json!(ids(51));
        value["bundle"]["purpose"] = json!("p".repeat(5_000));

        let result = one_byte_under(&value);

        assert_eq!(result["links"]["work_items"], json!(ids(50)));
        assert!(result["links"].get("work_items_more").is_none());
        assert_eq!(result["bundle"]["work_items"], json!(ids(50)));
        assert_eq!(result["bundle"]["work_items_more"], json!(1));
    }

    // ---- the order and the stop -----------------------------------------------------------

    #[test]
    fn the_cuts_run_in_order_and_stop_when_the_record_fits() {
        let mut value = full(200);
        value["evidence"]["proven"] = json!([{"id": "team.ok"}]);
        value["evidence"]["judged"] =
            json!([{"id": "team.judged", "verdict": "pass", "evidence": null}]);
        let after_step_1 = fitted(&value, size_of(&value) - 1);
        let after_step_3 = fitted(&value, size_of(&after_step_1) - 1);

        assert_eq!(
            after_step_1["truncated"],
            json!(["evidence.uncovered.paths.ranges"])
        );
        assert_eq!(
            after_step_3["truncated"],
            json!([
                "evidence.uncovered.paths.ranges",
                "evidence.uncovered.paths"
            ]),
            "the next step is step 3: step 2 has no evidence text to cut in these rows"
        );
        assert_eq!(
            after_step_3["evidence"]["uncovered"]["paths"],
            json!({"text": 200})
        );
    }

    #[test]
    fn a_record_that_no_step_can_shorten_goes_to_the_minimal_form_and_names_only_all() {
        let value = quiet(full(0));

        let result = one_byte_under(&value);

        assert_eq!(
            result["truncated"],
            json!(["all"]),
            "steps 1 to 7 cut nothing in this record, so none of them is named"
        );
    }

    #[test]
    fn a_second_fit_to_the_same_budget_changes_nothing() {
        let value = full(300);
        let size = size_of(&value);

        for max in [size / 2, size / 8, size / 64, 100] {
            let once = fitted(&value, max);
            assert_eq!(fitted(&once, max), once, "max {max}");
        }
    }

    // ---- the minimal record ---------------------------------------------------------------

    /// A record whose every cut leaves it over `max`: the minimal form is what
    /// remains.
    fn minimal_of(value: &Value) -> Value {
        fitted(value, 1)
    }

    #[test]
    fn step_8_keeps_the_fields_of_the_minimal_record_and_names_all() {
        let mut value = full(40);
        value["host"] = json!("gitlab");
        value["sample"] = json!({"rate": 0.5, "sampled": true});
        value["reviewers"] = json!(["amy", "bob"]);
        value["reviewers_more"] = json!(5);
        value["reviewers_pending"] = json!(false);
        value["post_merge_audit"] = json!({"verdict": "fail", "by": "carol", "note": "late"});
        value["truncated"] = json!(["commits"]);

        let result = minimal_of(&value);

        let keys: Vec<&str> = result
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "schema",
                "install",
                "key",
                "base",
                "head",
                "merge_base",
                "written_by",
                "host",
                "rules",
                "audit",
                "intent",
                "route",
                "sample",
                "reviewers",
                "reviewers_more",
                "reviewers_pending",
                "post_merge_audit",
                "truncated"
            ]
        );
        assert_eq!(
            result["truncated"],
            json!(["all"]),
            "all replaces the names"
        );
        assert_eq!(result["host"], json!("gitlab"));
        assert_eq!(result["sample"], value["sample"]);
        assert_eq!(result["reviewers"], value["reviewers"]);
        assert_eq!(result["reviewers_more"], json!(5));
        assert_eq!(result["post_merge_audit"], value["post_merge_audit"]);
        assert_eq!(
            result["audit"],
            json!({"summary": value["audit"]["summary"]})
        );
        assert_eq!(
            result["route"],
            json!({"route": "owner-review", "floor": false})
        );
        assert_eq!(
            result["intent"],
            json!([{"source": "work-item", "ref": "WI-1"}])
        );
        assert_eq!(
            result["rules"],
            json!({
                "merge_base_hash": value["rules"]["merge_base_hash"],
                "merge_base_rules": "present",
                "head_hash": HEAD_HASH,
                "audited_with": "head",
                "packs": [],
            }),
            "the packs go: the hash names the rules"
        );
    }

    #[test]
    fn the_minimal_record_has_a_reviewers_more_only_when_the_record_had_one() {
        let result = minimal_of(&full(40));

        assert!(result.get("reviewers_more").is_none());
    }

    #[test]
    fn the_minimal_record_of_a_minimal_record_is_itself() {
        let value = minimal(KEY);

        assert_eq!(minimal_of(&value), value);
    }

    #[test]
    fn the_minimal_record_keeps_at_most_ten_intents_and_cuts_each_ref_to_150_bytes() {
        let mut value = full(40);
        let intents: Vec<Value> = (0..12)
            .map(|n| {
                json!({
                    "source": "work-item", "ref": format!("{n:02}{}", "x".repeat(300)),
                    "title": "t", "why": null, "tasks": null, "requirements": null,
                    "validation": null, "artifacts": [],
                })
            })
            .collect();
        value["intent"] = json!(intents);

        let result = minimal_of(&value);

        let kept = result["intent"].as_array().expect("a list");
        assert_eq!(kept.len(), 10, "the first ten");
        for (n, intent) in kept.iter().enumerate() {
            let reference = intent["ref"].as_str().expect("a ref");
            assert_eq!(reference, format!("{n:02}{}", "x".repeat(148)));
            assert_eq!(
                intent.as_object().expect("an object").len(),
                2,
                "source and ref only"
            );
        }
    }

    #[test]
    fn a_ref_is_cut_on_a_character_boundary_by_its_serialized_bytes() {
        for (unit, kept) in [
            ("é", 75),
            ("日", 50),
            ("😀", 37),
            ("\u{1}", 25),
            ("\n", 75),
            ("\"", 75),
            ("\\", 75),
            ("\u{7f}", 150),
        ] {
            let mut value = full(40);
            value["intent"][0]["ref"] = json!(unit.repeat(300));

            let result = minimal_of(&value);

            let reference = result["intent"][0]["ref"].as_str().expect("a ref");
            assert_eq!(reference, unit.repeat(kept), "{unit:?}");
            assert!(escaped_len(reference) <= 150, "{unit:?}");
            assert!(
                escaped_len(&format!("{reference}{unit}")) > 150,
                "{unit:?}: the next character would not fit"
            );
        }
    }

    #[test]
    fn a_short_ref_is_kept_whole() {
        let mut value = full(40);
        value["intent"][0]["ref"] = json!("é".repeat(75));

        let result = minimal_of(&value);

        assert_eq!(result["intent"][0]["ref"], json!("é".repeat(75)));
    }

    #[test]
    fn the_post_merge_audit_of_the_minimal_record_is_cut_to_100_and_1000_serialized_bytes() {
        let mut value = full(40);
        value["post_merge_audit"] = json!({
            "verdict": "pass",
            "by": "b".repeat(150),
            "note": "\u{1}".repeat(400),
        });

        let result = minimal_of(&value);

        assert_eq!(result["post_merge_audit"]["verdict"], json!("pass"));
        assert_eq!(result["post_merge_audit"]["by"], json!("b".repeat(100)));
        assert_eq!(
            result["post_merge_audit"]["note"],
            json!("\u{1}".repeat(166)),
            "166 escapes of 6 bytes are 996 bytes; the 167th would pass 1,000"
        );
    }

    #[test]
    fn a_post_merge_audit_within_its_caps_stays_whole() {
        let mut value = full(40);
        value["post_merge_audit"] =
            json!({"verdict": "pass", "by": "b".repeat(100), "note": "n".repeat(1_000)});

        let result = minimal_of(&value);

        assert_eq!(result["post_merge_audit"], value["post_merge_audit"]);
    }

    #[test]
    fn a_post_merge_audit_without_a_note_keeps_its_null() {
        let mut value = full(40);
        value["post_merge_audit"] = json!({"verdict": "pass", "by": "amy", "note": null});

        let result = minimal_of(&value);

        assert_eq!(result["post_merge_audit"]["note"], Value::Null);
    }

    /// The record `full(40)` with every cap of the minimal record filled: the
    /// host `gitlab`, the longest route word with its sample, the optional keys
    /// of the audit summary, ten intents whose refs pass 150 bytes, a post-merge
    /// audit with its `by` at 100 and its note at 1,000 bytes, and the pending
    /// reviewers.
    fn every_cap_filled() -> Value {
        let mut value = full(40);
        value["host"] = json!("gitlab");
        value["route"]["route"] = json!("auto-merge-eligible");
        value["sample"] = json!({"rate": 0.123456789, "sampled": true});
        value["audit"]["summary"]["empty_range"] = json!(true);
        value["audit"]["summary"]["sanctioned_fail"] = json!(99_999);
        value["reviewers"] = json!([]);
        value["reviewers_pending"] = json!(true);
        value["intent"] = Value::Array(
            (0..10)
                .map(|n| {
                    json!({
                        "source": "work-item", "ref": format!("é{n}{}", "é".repeat(412)),
                        "title": null, "why": null, "tasks": null, "requirements": null,
                        "validation": null, "artifacts": [],
                    })
                })
                .collect(),
        );
        value["post_merge_audit"] =
            json!({"verdict": "pass", "by": "b".repeat(100), "note": "n".repeat(1_000)});
        value
    }

    #[test]
    fn the_minimal_record_at_every_cap_leaves_the_reserve_under_the_smallest_budget() {
        let written = fit(parsed(&every_cap_filled()), 8_192, PENDING_RESERVE);

        let json = json_of(&written);
        assert_eq!(json["truncated"], json!(["all"]));
        assert_eq!(json["intent"].as_array().expect("refs").len(), 10);
        assert!(
            size_of(&json) <= 8_192 - PENDING_RESERVE,
            "the minimal record takes {} bytes, and the budget less the reserve is {}",
            size_of(&json),
            8_192 - PENDING_RESERVE
        );
    }

    #[test]
    fn the_minimal_record_leaves_room_for_the_reviewers_that_a_later_job_sets() {
        let written = fit(parsed(&every_cap_filled()), 8_192, PENDING_RESERVE);

        let mut json = json_of(&written);
        let names: Vec<String> = (0..7).map(|n| format!("{n}{}", "😀".repeat(99))).collect();
        assert!(
            serde_json::to_string(&names).expect("serialize").len() <= 3_072,
            "the reviewers of spec 10.4 take at most 3,072 bytes"
        );
        json["reviewers"] = json!(names);
        json["reviewers_more"] = json!(13);
        json["reviewers_pending"] = json!(false);
        assert!(
            size_of(&json) <= 8_192,
            "the reviewers fill the reserve and the record stays within maxBytes: {}",
            size_of(&json)
        );
    }

    #[test]
    fn the_base_of_the_minimal_record_is_the_base_of_the_record() {
        let result = minimal_of(&full(40));

        assert_eq!(result["base"], json!(BASE));
        assert_eq!(result["key"], json!(KEY));
    }

    /// The sessions of a record that cites `count` checkpoints.
    fn sessions(count: u32) -> Value {
        Value::Array(
            (0..count)
                .map(|number| {
                    json!({"commit": KEY, "trailer": "Entire-Checkpoint", "value": format!("cp-{number}")})
                })
                .collect(),
        )
    }

    #[test]
    fn step_7_cuts_the_sessions_to_the_first_20_and_a_count() {
        let mut value = quiet(full(0));
        value["links"]["sessions"] = sessions(25);

        let result = one_byte_under(&value);

        assert_eq!(result["links"]["sessions"], sessions(20));
        assert_eq!(result["links"]["sessions_more"], json!(5));
        assert_eq!(result["truncated"], json!(["links"]));
    }

    #[test]
    fn step_7_keeps_20_sessions_whole_and_counts_the_one_past_them() {
        let mut value = quiet(full(0));
        value["links"]["sessions"] = sessions(20);
        value["bundle"]["purpose"] = json!("p".repeat(5_000));

        let result = one_byte_under(&value);

        assert_eq!(result["links"]["sessions"], sessions(20));
        assert!(result["links"].get("sessions_more").is_none());

        value["links"]["sessions"] = sessions(21);
        let result = one_byte_under(&value);

        assert_eq!(result["links"]["sessions"], sessions(20));
        assert_eq!(result["links"]["sessions_more"], json!(1));
    }
}
