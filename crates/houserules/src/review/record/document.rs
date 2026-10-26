//! The change record as a document: how `record write` builds it from the
//! facts of a range, the bytes of its note, and the Markdown summary that
//! `record show` prints.
//!
//! The note holds the record as one line of compact JSON and a line feed.
//! `notes::write` stores those bytes as a blob and attaches it with `git notes
//! add -C`, which keeps them as they are, so the bytes that `--out` writes and
//! the bytes that `record write` stores are the bytes that a unit pushes. The
//! summary reuses the lines of the packet (`review::evidence`) for the parts
//! that the packet prints, and prints a part that a cut turned into a count as
//! the count.

use crate::baseline;
use crate::published;
use crate::published::audit_result::AuditResult;
use crate::published::change::UncoveredPath;
use crate::published::change_record::{
    Artifacts, Audit, AuditVerdict, AuditedRow, AuditedWith, ChangeRecord, Commits, Host,
    IntentName, Intents, Links, PathList, PostMergeAudit, Ranges, RecordBundle, RecordEvidence,
    RecordInputs, RecordIntent, RecordPathInput, RecordRoute, RecordRuleRow, RecordRun,
    RecordUncovered, RecordUncoveredPath, Rules, Sample, UncoveredPaths,
};
use crate::published::evidence::{Evidence, RuleRow};
use crate::published::intent::Intent;
use crate::published::route::RouteDocument;
use crate::review::bundle::Bundle;
use crate::review::change::Analysis;
use crate::review::evidence::{
    COVERAGE_LINE, WORK_ITEM_LIMIT, first_chars, intent_block, judged_line, packet_line, rule_line,
    sessions_line,
};
use crate::review::route::lines_of;
use crate::review::text::{code_span, fenced};

/// The reviewers of a record (spec 8.2).
pub(super) struct Reviewers {
    /// The approvers: sorted, without repeats, at most 20, each cut to 100
    /// characters, and no more than keep the serialized array within 3,072
    /// bytes.
    pub(super) names: Vec<String>,
    /// The number of approvers left out of `names`, or `None` when none is.
    pub(super) more: Option<u64>,
    /// `true` when the record holds no reviewers yet: a later job has not set
    /// them, or the approvers could not be read and stay unknown.
    pub(super) pending: bool,
}

/// The most approvers that a record lists.
const REVIEWERS_LISTED: usize = 20;

/// The most characters of an approver's name.
const REVIEWER_CHARS: usize = 100;

/// The most bytes of the serialized `reviewers` array (spec 8.2).
const REVIEWERS_ARRAY_BYTES: usize = 3_072;

/// The number of the first `names` that keep their serialized array, brackets
/// and commas included, within `limit` bytes.
fn names_within(names: &[String], limit: usize) -> usize {
    let mut size = "[]".len();
    let mut kept = 0;
    for name in names {
        let width = serde_json::to_string(name)
            .expect("a string serializes")
            .len();
        let comma = usize::from(kept > 0);
        if size + comma + width > limit {
            break;
        }
        size += comma + width;
        kept += 1;
    }
    kept
}

impl Reviewers {
    /// The reviewers for the logins `logins`, as the program of the GitLab push
    /// job sets them (spec 10.4): sorted and without repeats, the first 20, each
    /// cut to 100 characters, no more names than keep the serialized array
    /// within 3,072 bytes, and the count of the names left out. A pending record
    /// lists none.
    pub(super) fn of(logins: &[String], pending: bool) -> Self {
        if pending {
            return Self {
                names: Vec::new(),
                more: None,
                pending,
            };
        }
        let mut sorted: Vec<&String> = logins.iter().collect();
        sorted.sort();
        sorted.dedup();
        let mut names: Vec<String> = sorted
            .iter()
            .take(REVIEWERS_LISTED)
            .map(|name| first_chars(name, REVIEWER_CHARS).to_string())
            .collect();
        names.truncate(names_within(&names, REVIEWERS_ARRAY_BYTES));
        let more = sorted.len() - names.len();
        Self {
            names,
            more: (more > 0).then_some(more as u64),
            pending,
        }
    }
}

/// The facts of a range from which a record is built.
pub(super) struct Facts<'a> {
    /// The change analysis of the range.
    pub(super) analysis: &'a Analysis,
    /// The evidence document of the range, with the intents, the packs, and the
    /// route that the record keeps.
    pub(super) evidence: &'a Evidence,
    /// The audit of the range; an empty result when the audit could not run.
    pub(super) audit: &'a AuditResult,
    /// The line of an audit that could not run.
    pub(super) audit_error: Option<String>,
    /// The commits of the range, oldest first.
    pub(super) commits: Vec<String>,
    /// The commit that the note annotates.
    pub(super) key: &'a str,
    /// The host of the request.
    pub(super) host: Option<Host>,
    /// The sample draw.
    pub(super) sample: Option<Sample>,
    /// The bundle of the request, parsed.
    pub(super) bundle: Option<&'a Bundle>,
    /// The reviewers.
    pub(super) reviewers: Reviewers,
}

/// The record of the range of `facts`, in its full form and before any cut.
pub(super) fn build(facts: &Facts) -> ChangeRecord {
    let analysis = facts.analysis;
    let evidence = facts.evidence;
    let work_items = evidence.work_items.clone();
    ChangeRecord {
        schema: published::id::<ChangeRecord>(),
        install: analysis.install.clone(),
        key: facts.key.to_string(),
        base: analysis.base.clone(),
        head: analysis.head.clone(),
        merge_base: analysis.merge_base.clone(),
        commits: Some(Commits::List(facts.commits.clone())),
        written_by: format!("houserules {}", env!("CARGO_PKG_VERSION")),
        host: facts.host,
        rules: Rules {
            merge_base_hash: evidence.rules.merge_base_hash.clone(),
            merge_base_rules: analysis.rules.merge_base_rules.as_str().to_string(),
            head_hash: evidence.rules.head_hash.clone(),
            audited_with: AuditedWith::Head,
            packs: evidence.rules.packs.clone(),
        },
        audit: audit_of(facts.audit, facts.audit_error.clone()),
        intent: Intents::Full(evidence.intent.iter().map(record_intent).collect()),
        evidence: Some(record_evidence(evidence)),
        route: record_route(&evidence.route),
        sample: facts.sample,
        bundle: Some(facts.bundle.map(record_bundle)),
        reviewers: facts.reviewers.names.clone(),
        reviewers_more: facts.reviewers.more,
        reviewers_pending: facts.reviewers.pending,
        links: Some(Links {
            work_items,
            work_items_more: None,
            sessions: evidence.sessions.clone(),
            sessions_more: None,
        }),
        post_merge_audit: None,
        truncated: Vec::new(),
    }
}

/// The audit part of a record from the audit result `audit`.
fn audit_of(audit: &AuditResult, error: Option<String>) -> Audit {
    Audit {
        summary: audit.summary.clone(),
        rules: Some(
            audit
                .rules
                .iter()
                .map(|row| AuditedRow {
                    id: row.id.clone(),
                    mode: row.mode,
                    result: row.result,
                })
                .collect(),
        ),
        references: Some(audit.references.clone()),
        error: Some(error),
    }
}

/// The intent `intent` as the record keeps it.
fn record_intent(intent: &Intent) -> RecordIntent {
    RecordIntent {
        source: intent.source.clone(),
        reference: intent.reference.clone(),
        title: intent.title.clone(),
        why: intent.why.clone(),
        tasks: intent.tasks,
        requirements: intent.requirements,
        validation: intent.validation.clone(),
        artifacts: Artifacts::List(intent.artifacts.clone()),
        unresolved: intent.unresolved.clone(),
    }
}

/// The rows of `rows` as the record keeps them.
fn record_rows(rows: &[RuleRow]) -> Vec<RecordRuleRow> {
    rows.iter()
        .map(|row| RecordRuleRow {
            id: row.id.clone(),
            evidence: Some(row.evidence.clone()),
        })
        .collect()
}

/// The evidence document `evidence` without its `rules`, `intent`, and `route`.
fn record_evidence(evidence: &Evidence) -> RecordEvidence {
    RecordEvidence {
        schema: published::id::<Evidence>(),
        install: evidence.install.clone(),
        base: evidence.base.clone(),
        head: evidence.head.clone(),
        merge_base: evidence.merge_base.clone(),
        diff: evidence.diff.clone(),
        uncovered: RecordUncovered {
            lines: evidence.uncovered.lines,
            total_lines: evidence.uncovered.total_lines,
            share: evidence.uncovered.share,
            paths: UncoveredPaths::List(
                evidence
                    .uncovered
                    .paths
                    .iter()
                    .map(|path| RecordUncoveredPath {
                        path: path.path.clone(),
                        old_path: path.old_path.clone(),
                        kind: path.kind,
                        ranges: Ranges::List(path.ranges.clone()),
                        empty: path.empty,
                    })
                    .collect(),
            ),
        },
        failed: record_rows(&evidence.failed),
        warned: record_rows(&evidence.warned),
        proven: record_rows(&evidence.proven),
        judged: evidence.judged.clone(),
        skipped: record_rows(&evidence.skipped),
        work_items: evidence.work_items.clone(),
        work_items_more: None,
    }
}

/// A list of paths as the record keeps it.
fn path_list(paths: &[String]) -> PathList {
    PathList::List(paths.to_vec())
}

/// The route document `route` as the record keeps it.
fn record_route(route: &RouteDocument) -> RecordRoute {
    let inputs = &route.inputs;
    RecordRoute {
        schema: Some(route.schema.clone()),
        install: Some(route.install.clone()),
        head: Some(route.head.clone()),
        route: route.route,
        floor: route.floor,
        inputs: Some(RecordInputs {
            failed: inputs.failed.clone(),
            warned: inputs.warned.clone(),
            areas: inputs
                .areas
                .iter()
                .map(|(area, paths)| (area.clone(), path_list(paths)))
                .collect(),
            no_area: path_list(&inputs.no_area),
            paths: inputs
                .paths
                .iter()
                .map(|entry| RecordPathInput {
                    glob: entry.glob.clone(),
                    route: entry.route,
                    paths: path_list(&entry.paths),
                })
                .collect(),
            diff_lines: inputs.diff_lines,
            uncovered_share: inputs.uncovered_share,
            score: inputs.score,
            rule_sources_changed: path_list(&inputs.rule_sources_changed),
        }),
        reasons: Some(route.reasons.clone()),
    }
}

/// The bundle `bundle` as the record keeps it: its parts, and for each run the
/// facts of its capture header and the SHA-256 of its header line.
fn record_bundle(bundle: &Bundle) -> RecordBundle {
    RecordBundle {
        purpose: bundle.purpose.clone(),
        intent: bundle.intent.clone(),
        work_items: bundle.work_items.clone(),
        work_items_more: None,
        tier: bundle.tier.clone(),
        criterion: bundle.criterion.clone(),
        budget_reason: bundle.budget_reason.clone(),
        runs: bundle
            .runs
            .iter()
            .map(|run| {
                let header = run.header();
                RecordRun {
                    argv: header
                        .as_ref()
                        .map(|header| header.argv.clone())
                        .unwrap_or_default(),
                    exit: header.as_ref().and_then(|header| header.exit),
                    head: header.and_then(|header| header.head),
                    sha256: baseline::hash(run.header_line.as_bytes()),
                }
            })
            .collect(),
        runs_more: None,
    }
}

/// The bytes of the note of `record`: its compact JSON and a line feed.
pub(super) fn bytes(record: &ChangeRecord) -> Vec<u8> {
    let mut bytes = serde_json::to_vec(record).expect("a change record serializes");
    bytes.push(b'\n');
    bytes
}

// ---- the summary ---------------------------------------------------------------------

/// `value` as plain text when it is made of hex digits, as a sha is, else in a
/// code span: a note is repository data, and a value that is no sha prints as
/// untrusted text (spec 4.8).
fn sha_text(value: &str, chars: usize) -> String {
    let shown = first_chars(value, chars);
    if !shown.is_empty() && shown.chars().all(|c| c.is_ascii_hexdigit()) {
        shown.to_string()
    } else {
        code_span(shown)
    }
}

/// The word of a verdict.
pub(super) fn verdict_word(verdict: AuditVerdict) -> &'static str {
    match verdict {
        AuditVerdict::Pass => "pass",
        AuditVerdict::Fail => "fail",
    }
}

/// The word of a host.
fn host_word(host: Host) -> &'static str {
    match host {
        Host::Github => "github",
        Host::Gitlab => "gitlab",
    }
}

/// The lines of a list of items, each line ended by a line feed; `None.` for a
/// list with no item.
fn bullet_list(items: &[String]) -> String {
    if items.is_empty() {
        return "None.\n".to_string();
    }
    items.iter().map(|item| format!("{item}\n")).collect()
}

/// The range line of the summary.
fn range_line(record: &ChangeRecord) -> String {
    format!(
        "Record {} · range {}..{} (merge base {}) · rules {} · route {}",
        sha_text(&record.key, 7),
        sha_text(&record.base, 7),
        sha_text(&record.head, 7),
        record
            .merge_base
            .as_deref()
            .map_or_else(|| "none".to_string(), |sha| sha_text(sha, 7)),
        sha_text(&record.rules.head_hash, 12),
        record.route.route.as_str()
    )
}

/// The item of an uncovered path.
fn uncovered_item(path: &RecordUncoveredPath) -> String {
    let line = match &path.ranges {
        Ranges::List(ranges) => packet_line(&UncoveredPath {
            path: path.path.clone(),
            old_path: path.old_path.clone(),
            kind: path.kind,
            ranges: ranges.clone(),
            empty: path.empty,
        }),
        Ranges::Count(lines) => format!("{lines} changed lines"),
    };
    format!("- {}: {line}", code_span(&path.path))
}

/// The section of the uncovered surface.
fn uncovered_section(uncovered: &RecordUncovered) -> String {
    let mut text = format!(
        "### Uncovered surface ({} of {} changed lines)\n\n",
        uncovered.lines, uncovered.total_lines
    );
    let (items, none) = match &uncovered.paths {
        UncoveredPaths::List(paths) => (
            paths.iter().map(uncovered_item).collect::<Vec<_>>(),
            paths.is_empty(),
        ),
        UncoveredPaths::Counts(counts) => (
            counts
                .iter()
                .map(|(kind, count)| format!("- {}: {count} paths", code_span(kind)))
                .collect(),
            counts.is_empty(),
        ),
    };
    text.push_str(&bullet_list(&items));
    if !none {
        text.push_str(&format!("\n{COVERAGE_LINE}\n"));
    }
    text
}

/// The intent `intent` as the packet prints it.
fn packet_intent(intent: &RecordIntent) -> Intent {
    Intent {
        source: intent.source.clone(),
        reference: intent.reference.clone(),
        title: intent.title.clone(),
        why: intent.why.clone(),
        tasks: intent.tasks,
        requirements: intent.requirements,
        validation: intent.validation.clone(),
        artifacts: Vec::new(),
        unresolved: intent.unresolved.clone(),
    }
}

/// The section of the intents.
fn intent_section(intents: &Intents) -> String {
    let count = match intents {
        Intents::Full(list) => list.len(),
        Intents::Refs(list) => list.len(),
    };
    let mut text = format!("### Intent ({count})\n\n");
    if count == 0 {
        text.push_str("None recorded.\n");
        return text;
    }
    match intents {
        Intents::Full(list) => {
            let blocks: Vec<String> = list
                .iter()
                .map(|intent| intent_block(&packet_intent(intent), None))
                .collect();
            text.push_str(&format!("{}\n", blocks.join("\n\n")));
        }
        Intents::Refs(list) => {
            let items: Vec<String> = list.iter().map(ref_item).collect();
            text.push_str(&bullet_list(&items));
        }
    }
    text
}

/// The item of an intent of the minimal record.
fn ref_item(intent: &IntentName) -> String {
    format!(
        "- {}",
        code_span(&format!("{} {}", intent.source, intent.reference))
    )
}

/// The item of a rule row: its id, and its evidence while the row keeps it.
fn row_item(row: &RecordRuleRow) -> String {
    match &row.evidence {
        Some(evidence) => rule_line(&RuleRow {
            id: row.id.clone(),
            evidence: evidence.clone(),
        }),
        None => format!("- {}", code_span(&row.id)),
    }
}

/// The section `heading` of the rule rows `rows`.
fn rows_section(heading: &str, rows: &[RecordRuleRow]) -> String {
    let items: Vec<String> = rows.iter().map(row_item).collect();
    format!("### {heading} ({})\n\n{}", rows.len(), bullet_list(&items))
}

/// The line of the work items of a record: the ids in code spans, and the count
/// of the ids that a cut left out.
fn work_items_line(ids: &[String], more: Option<u64>) -> String {
    if ids.is_empty() && more.is_none() {
        return "Work items: none".to_string();
    }
    let mut parts: Vec<String> = ids
        .iter()
        .map(|id| code_span(first_chars(id, WORK_ITEM_LIMIT)))
        .collect();
    if let Some(more) = more {
        parts.push(format!("... and {more} more"));
    }
    format!("Work items: {}", parts.join(", "))
}

/// The sections of the packet that the evidence of the record holds.
fn packet_sections(record: &ChangeRecord, evidence: &RecordEvidence) -> String {
    let mut text = uncovered_section(&evidence.uncovered);
    text.push_str(&format!("\n{}", intent_section(&record.intent)));
    for (heading, rows) in [
        ("Failed", &evidence.failed),
        ("Warned", &evidence.warned),
        ("Proven", &evidence.proven),
    ] {
        text.push_str(&format!("\n{}", rows_section(heading, rows)));
    }
    let judged: Vec<String> = evidence.judged.iter().map(judged_line).collect();
    text.push_str(&format!(
        "\n### Judged only ({})\n\n{}",
        evidence.judged.len(),
        bullet_list(&judged)
    ));
    text.push_str(&format!(
        "\n{} deterministic checks did not apply to this change.\n",
        evidence.skipped.len()
    ));
    text.push_str(&format!(
        "\n### Size and links\n\nDiff: +{} -{} lines in {} files",
        evidence.diff.added, evidence.diff.deleted, evidence.diff.files
    ));
    if evidence.diff.excluded > 0 {
        text.push_str(&format!(
            "; {} files excluded by review.diffExclude",
            evidence.diff.excluded
        ));
    }
    let (ids, more) = match &record.links {
        Some(links) => (&links.work_items, links.work_items_more),
        None => (&evidence.work_items, evidence.work_items_more),
    };
    text.push_str(&format!("\n{}\n", work_items_line(ids, more)));
    if let Some(links) = &record.links {
        text.push_str(&format!(
            "{}\n",
            sessions_line(&links.sessions, links.sessions_more.unwrap_or(0))
        ));
    }
    text
}

/// The section of the route.
fn route_section(route: &RecordRoute) -> String {
    let floor = if route.floor { " (floor)" } else { "" };
    let mut text = format!("### Route\n\n{}{floor}\n", route.route.as_str());
    let lines: Vec<String> = route
        .reasons
        .as_deref()
        .map(lines_of)
        .unwrap_or_default()
        .into_iter()
        .map(|line| format!("- {line}"))
        .collect();
    if !lines.is_empty() {
        text.push_str(&format!("\n{}", bullet_list(&lines)));
    }
    text
}

/// The section of the sample draw.
fn sample_section(sample: Option<Sample>) -> String {
    let body = match sample {
        None => "None.".to_string(),
        Some(sample) => format!(
            "Drawn at rate {}: {}.",
            sample.rate,
            if sample.sampled {
                "sampled"
            } else {
                "not sampled"
            }
        ),
    };
    format!("### Sample\n\n{body}\n")
}

/// The section of the reviewers.
fn reviewers_section(record: &ChangeRecord) -> String {
    let body = if record.reviewers_pending {
        "Pending: no later job has set the reviewers, or the approvers could not be read."
            .to_string()
    } else if record.reviewers.is_empty() {
        "None recorded.".to_string()
    } else {
        let mut parts: Vec<String> = record
            .reviewers
            .iter()
            .map(|name| code_span(name))
            .collect();
        if let Some(more) = record.reviewers_more {
            parts.push(format!("... and {more} more"));
        }
        parts.join(", ")
    };
    format!("### Reviewers\n\n{body}\n")
}

/// The section of the post-merge audit.
fn audit_section(audit: Option<&PostMergeAudit>) -> String {
    let mut text = "### Post-merge audit\n\n".to_string();
    match audit {
        None => text.push_str("None recorded.\n"),
        Some(audit) => {
            text.push_str(&format!(
                "{} by {}\n",
                verdict_word(audit.verdict),
                code_span(&audit.by)
            ));
            if let Some(note) = &audit.note {
                text.push_str(&format!("\nNote:\n\n{}\n", fenced(note)));
            }
        }
    }
    text
}

/// The section that names the fields a cut shortened.
fn cut_section(names: &[String]) -> String {
    let items: String = names
        .iter()
        .map(|name| format!("- {}\n", code_span(name)))
        .collect();
    format!(
        "### Cut\n\nThe record was cut to fit review.record.maxBytes. These fields were shortened:\n\n{items}"
    )
}

/// The Markdown summary of `record`: the packet of the change while the record
/// keeps it, then the route, the sample, the reviewers, and the post-merge
/// audit.
pub(super) fn markdown(record: &ChangeRecord) -> String {
    let host = record
        .host
        .map_or_else(String::new, |host| format!(" on {}", host_word(host)));
    let mut text = format!(
        "## houserules change record\n\n{}\n\nWritten by {}{host}.\n\n",
        range_line(record),
        code_span(&record.written_by)
    );
    match &record.evidence {
        Some(evidence) => text.push_str(&packet_sections(record, evidence)),
        None => {
            text.push_str("The packet is not kept: the record was cut to its minimal form.\n");
            text.push_str(&format!("\n{}", intent_section(&record.intent)));
        }
    }
    text.push_str(&format!("\n{}", route_section(&record.route)));
    text.push_str(&format!("\n{}", sample_section(record.sample)));
    text.push_str(&format!("\n{}", reviewers_section(record)));
    text.push_str(&format!(
        "\n{}",
        audit_section(record.post_merge_audit.as_ref())
    ));
    if !record.truncated.is_empty() {
        text.push_str(&format!("\n{}", cut_section(&record.truncated)));
    }
    text
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::published::change_record::ChangeRecord;
    use crate::review::record::fixtures::{COMMITS, KEY, full, json_of, listing, minimal, parsed};

    /// The Markdown summary of the record `value`.
    fn summary(value: &Value) -> String {
        markdown(&parsed(value))
    }

    /// The lines of the section `heading` of `text`: those after the heading
    /// line up to the next `### ` heading, without the blank line before it.
    fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
        let mut lines: Vec<&str> = text
            .lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| !line.starts_with("### "))
            .collect();
        while lines.last() == Some(&"") {
            lines.pop();
        }
        lines
    }

    #[test]
    fn the_bytes_of_a_record_are_its_compact_json_and_a_line_feed() {
        let value = full(2);

        let bytes = bytes(&parsed(&value));

        assert_eq!(bytes.last(), Some(&b'\n'));
        let text = String::from_utf8(bytes).expect("UTF-8");
        assert_eq!(text.matches('\n').count(), 1, "one line");
        assert!(!text.contains(": "), "no space after a colon");
        assert_eq!(serde_json::from_str::<Value>(&text).expect("JSON"), value);
    }

    #[test]
    fn the_bytes_of_a_record_keep_the_order_of_its_keys() {
        let text = String::from_utf8(bytes(&parsed(&full(2)))).expect("UTF-8");

        assert!(
            text.starts_with(r#"{"schema":"houserules/change-record@1.1.0","install":".","key":"#),
            "{text}"
        );
    }

    #[test]
    fn the_summary_of_a_full_record_holds_the_packet_and_the_sections_of_the_record() {
        assert_eq!(
            summary(&full(2)),
            "## houserules change record\n\
             \n\
             Record 4444444 · range 0000000..4444444 (merge base 0000000) · rules bbbbbbbbbbbb · route owner-review\n\
             \n\
             Written by `houserules 2.0.0` on github.\n\
             \n\
             ### Uncovered surface (10 of 10 changed lines)\n\
             \n\
             - `src/file0000.rs`: lines 1-3; deleted after line 9: 2 lines\n\
             - `src/file0001.rs`: lines 1-3; deleted after line 9: 2 lines\n\
             \n\
             Coverage is per file: a file is covered when a check that declares covers: true read it.\n\
             \n\
             ### Intent (1)\n\
             \n\
             `work-item WI-1`: `A title`\n\
             Why:\n\
             ```\n\
             Because.\n\
             ```\n\
             Tasks: 1 of 2 done\n\
             \n\
             ### Failed (1)\n\
             \n\
             - `team.lint`: `a lint finding`\n\
             \n\
             ### Warned (1)\n\
             \n\
             - `team.note`: `a note`\n\
             \n\
             ### Proven (1)\n\
             \n\
             - `team.ok`: `3 files checked`\n\
             \n\
             ### Judged only (1)\n\
             \n\
             - `team.judged`: pass · `read`\n\
             \n\
             1 deterministic checks did not apply to this change.\n\
             \n\
             ### Size and links\n\
             \n\
             Diff: +6 -0 lines in 2 files\n\
             Work items: `WI-1`, `WI-2`\n\
             Sessions: none\n\
             \n\
             ### Route\n\
             \n\
             owner-review\n\
             \n\
             - area `code`: owner-review\n\
             \n\
             ### Sample\n\
             \n\
             None.\n\
             \n\
             ### Reviewers\n\
             \n\
             `amy`, `bob`\n\
             \n\
             ### Post-merge audit\n\
             \n\
             None recorded.\n"
        );
    }

    #[test]
    fn the_summary_of_a_minimal_record_says_that_the_packet_is_not_kept() {
        assert_eq!(
            summary(&minimal(KEY)),
            "## houserules change record\n\
             \n\
             Record 4444444 · range 0000000..4444444 (merge base 0000000) · rules bbbbbbbbbbbb · route owner-review\n\
             \n\
             Written by `houserules 2.0.0`.\n\
             \n\
             The packet is not kept: the record was cut to its minimal form.\n\
             \n\
             ### Intent (0)\n\
             \n\
             None recorded.\n\
             \n\
             ### Route\n\
             \n\
             owner-review\n\
             \n\
             ### Sample\n\
             \n\
             None.\n\
             \n\
             ### Reviewers\n\
             \n\
             None recorded.\n\
             \n\
             ### Post-merge audit\n\
             \n\
             None recorded.\n\
             \n\
             ### Cut\n\
             \n\
             The record was cut to fit review.record.maxBytes. These fields were shortened:\n\
             \n\
             - `all`\n"
        );
    }

    #[test]
    fn the_range_line_says_none_for_a_range_with_no_merge_base() {
        let mut value = full(0);
        value["merge_base"] = Value::Null;

        let text = summary(&value);

        assert!(text.contains("(merge base none) · rules"), "{text}");
    }

    #[test]
    fn a_floor_route_says_so() {
        let mut value = full(0);
        value["route"]["floor"] = json!(true);

        let text = summary(&value);

        assert_eq!(section(&text, "### Route")[1], "owner-review (floor)");
    }

    #[test]
    fn a_sample_prints_its_rate_and_its_draw() {
        let mut value = full(0);
        value["sample"] = json!({"rate": 0.1, "sampled": true});
        assert_eq!(
            section(&summary(&value), "### Sample"),
            ["", "Drawn at rate 0.1: sampled."]
        );

        value["sample"] = json!({"rate": 0.5, "sampled": false});
        assert_eq!(
            section(&summary(&value), "### Sample"),
            ["", "Drawn at rate 0.5: not sampled."]
        );
    }

    #[test]
    fn reviewers_print_their_state() {
        let mut value = full(0);
        value["reviewers"] = json!([]);
        value["reviewers_pending"] = json!(true);
        assert_eq!(
            section(&summary(&value), "### Reviewers"),
            [
                "",
                "Pending: no later job has set the reviewers, or the approvers could not be read."
            ]
        );

        value["reviewers_pending"] = json!(false);
        assert_eq!(
            section(&summary(&value), "### Reviewers"),
            ["", "None recorded."]
        );

        value["reviewers"] = json!(["amy", "bob"]);
        value["reviewers_more"] = json!(5);
        assert_eq!(
            section(&summary(&value), "### Reviewers"),
            ["", "`amy`, `bob`, ... and 5 more"]
        );
    }

    #[test]
    fn a_post_merge_audit_prints_its_verdict_its_author_and_its_note() {
        let mut value = full(0);
        value["post_merge_audit"] = json!({"verdict": "fail", "by": "amy", "note": "too late"});
        assert_eq!(
            section(&summary(&value), "### Post-merge audit"),
            [
                "",
                "fail by `amy`",
                "",
                "Note:",
                "",
                "```",
                "too late",
                "```"
            ]
        );

        value["post_merge_audit"] = json!({"verdict": "pass", "by": "bob", "note": null});
        assert_eq!(
            section(&summary(&value), "### Post-merge audit"),
            ["", "pass by `bob`"]
        );
    }

    #[test]
    fn a_cut_record_names_its_cuts() {
        let mut value = full(0);
        value["truncated"] = json!(["commits", "route.inputs"]);

        let text = summary(&value);

        assert_eq!(
            section(&text, "### Cut"),
            [
                "",
                "The record was cut to fit review.record.maxBytes. These fields were shortened:",
                "",
                "- `commits`",
                "- `route.inputs`"
            ]
        );
    }

    #[test]
    fn a_record_that_no_cut_touched_has_no_cut_section() {
        assert!(!summary(&full(2)).contains("### Cut"));
    }

    #[test]
    fn an_uncovered_surface_cut_to_counts_prints_the_counts() {
        let mut value = full(3);
        for n in 0..3 {
            value["evidence"]["uncovered"]["paths"][n]["ranges"] = json!(5);
        }
        let text = summary(&value);
        assert_eq!(
            section(&text, "### Uncovered surface (15 of 15 changed lines)")[..5],
            [
                "",
                "- `src/file0000.rs`: 5 changed lines",
                "- `src/file0001.rs`: 5 changed lines",
                "- `src/file0002.rs`: 5 changed lines",
                ""
            ]
        );

        value["evidence"]["uncovered"]["paths"] = json!({"text": 3, "binary": 2});
        let text = summary(&value);
        assert_eq!(
            section(&text, "### Uncovered surface (15 of 15 changed lines)")[..4],
            ["", "- `text`: 3 paths", "- `binary`: 2 paths", ""]
        );
    }

    #[test]
    fn the_kinds_of_a_count_map_come_from_the_note_and_print_in_code_spans() {
        let mut value = full(3);
        value["evidence"]["uncovered"]["paths"] =
            json!({"<!-- hidden": 1, "## Injected heading": 2, "a `tick": 3});

        let text = summary(&value);

        assert_eq!(
            section(&text, "### Uncovered surface (15 of 15 changed lines)")[..5],
            [
                "",
                "- `<!-- hidden`: 1 paths",
                "- `## Injected heading`: 2 paths",
                "- ``a `tick``: 3 paths",
                ""
            ]
        );
    }

    #[test]
    fn a_route_cut_to_counts_still_prints_its_reasons() {
        let mut value = full(2);
        value["route"]["inputs"]["areas"] = json!({"code": 2});
        value["route"]["inputs"]["no_area"] = json!(1);
        value["route"]["inputs"]["paths"] =
            json!([{"glob": "src/**", "route": "owner-review", "paths": 2}]);
        value["route"]["inputs"]["rule_sources_changed"] = json!(1);

        let text = summary(&value);

        assert_eq!(
            section(&text, "### Route"),
            ["", "owner-review", "", "- area `code`: owner-review"]
        );
    }

    #[test]
    fn an_unresolved_intent_prints_the_line_of_its_adapter() {
        let mut value = full(0);
        value["intent"] = json!([{
            "source": "work-item", "ref": "WI-404", "title": null, "why": null,
            "tasks": null, "requirements": null, "validation": null,
            "artifacts": [], "unresolved": "the tracker holds no such work item",
        }]);

        let text = summary(&value);

        assert_eq!(
            section(&text, "### Intent (1)"),
            [
                "",
                "`work-item WI-404`: the tracker holds no such work item"
            ]
        );
    }

    #[test]
    fn the_intents_of_a_minimal_record_print_their_source_and_ref() {
        let mut value = minimal(KEY);
        value["intent"] = json!([{"source": "work-item", "ref": "WI-1"}, {"source": "file", "ref": "docs/plans/a.md"}]);

        let text = summary(&value);

        assert_eq!(
            section(&text, "### Intent (2)"),
            ["", "- `work-item WI-1`", "- `file docs/plans/a.md`"]
        );
    }

    #[test]
    fn untrusted_text_prints_in_code_spans() {
        let mut value = full(0);
        value["evidence"]["failed"] = json!([{"id": "team.lint", "evidence": "<!-- x --> `y`"}]);
        value["written_by"] = json!("houserules `1`");

        let text = summary(&value);

        assert!(
            text.contains("- `team.lint`: `` <!-- x --> `y` ``"),
            "{text}"
        );
        assert!(text.contains("Written by `` houserules `1` ``"), "{text}");
    }

    // ---- the reviewers ----------------------------------------------------------------------

    /// The serialized array of `names`, in bytes.
    fn array_bytes(names: &[String]) -> usize {
        serde_json::to_string(names).expect("serialize").len()
    }

    #[test]
    fn reviewers_are_sorted_unique_capped_at_20_and_cut_to_100_characters() {
        let mut logins: Vec<String> = (0..25).rev().map(|n| format!("user{n:02}")).collect();
        logins.push("user03".to_string());
        logins.push("x".repeat(150));

        let reviewers = Reviewers::of(&logins, false);

        let expected: Vec<String> = (0..20).map(|n| format!("user{n:02}")).collect();
        assert_eq!(reviewers.names, expected);
        assert_eq!(
            reviewers.more,
            Some(6),
            "25 names and the long one, less 20"
        );
        assert!(!reviewers.pending);
    }

    #[test]
    fn reviewers_keep_no_more_names_than_fit_3072_bytes_of_the_serialized_array() {
        let logins: Vec<String> = (0..20)
            .map(|n| {
                let first = char::from_u32(0x1F600 + n).expect("an emoji");
                format!("{first}{}", "😀".repeat(99))
            })
            .collect();

        let reviewers = Reviewers::of(&logins, false);

        assert_eq!(reviewers.names, logins[..7], "7 names of 402 bytes fit");
        assert_eq!(reviewers.more, Some(13), "the 13 names left out");
        assert!(array_bytes(&reviewers.names) <= 3_072);
        assert!(
            array_bytes(&logins[..8]) > 3_072,
            "the 8th name would not fit"
        );
    }

    #[test]
    fn reviewers_that_fit_the_bytes_have_no_count_unless_names_pass_20() {
        let logins: Vec<String> = (0..20).map(|n| format!("user{n:02}")).collect();

        let reviewers = Reviewers::of(&logins, false);

        assert_eq!(reviewers.names, logins);
        assert_eq!(reviewers.more, None);
    }

    #[test]
    fn pending_reviewers_are_none() {
        let reviewers = Reviewers::of(&["amy".to_string()], true);

        assert!(reviewers.names.is_empty());
        assert_eq!(reviewers.more, None);
        assert!(reviewers.pending);
    }

    // ---- the audit part ---------------------------------------------------------------------

    /// An audit result with no row whose references are `references`.
    fn audit_with(references: Value) -> AuditResult {
        serde_json::from_value(json!({
            "schema": "houserules/audit-result@1.0.0",
            "base": "0000000",
            "head": "4444444",
            "ids": [],
            "changed_files": [],
            "areas": [],
            "area_files": {},
            "rules": [],
            "summary": {
                "base": "0000000",
                "head": "4444444",
                "deterministic": 0,
                "pass": 0,
                "fail": 0,
                "warn": 0,
                "skipped": 0,
                "judged": 0,
                "reference_fail": 1,
            },
            "references": references,
        }))
        .expect("an audit result")
    }

    #[test]
    fn the_audit_part_keeps_the_reference_summary_of_the_audit_with_its_failure_lines() {
        let line = "tracker json cannot be reached: backlog/ is missing; restore backlog/ from version control";
        let references = json!({"checked": 2, "passed": 1, "skipped": 1, "failures": [line]});

        let part = audit_of(&audit_with(references.clone()), None);

        assert_eq!(
            serde_json::to_value(&part).expect("serialize")["references"],
            references
        );
    }

    // ---- the document type ----------------------------------------------------------------

    /// The record `value` written back as compact JSON.
    fn written(value: &Value) -> String {
        serde_json::to_string(&parsed(value)).expect("a record serializes")
    }

    /// `full(3)` with every cut of spec 8.4 applied to it by hand: the forms
    /// that the cut lists take.
    fn cut_by_hand() -> Value {
        let mut value = full(3);
        for n in 0..3 {
            value["evidence"]["uncovered"]["paths"][n]["ranges"] = json!(5);
        }
        value["evidence"]["proven"] = json!([{"id": "team.ok"}]);
        value["evidence"]["judged"][0]["evidence"] = Value::Null;
        value["evidence"]["work_items"] = json!(["WI-1"]);
        value["evidence"]["work_items_more"] = json!(1);
        value["commits"] = json!({"count": 4, "first": COMMITS[0], "last": COMMITS[3]});
        value["route"]["inputs"]["areas"] = json!({"code": 3});
        value["route"]["inputs"]["no_area"] = json!(1);
        value["route"]["inputs"]["paths"] =
            json!([{"glob": "src/**", "route": "owner-review", "paths": 3}]);
        value["route"]["inputs"]["rule_sources_changed"] = json!(1);
        value["intent"][0]["artifacts"] = json!(2);
        value["bundle"]["runs_more"] = json!(4);
        value["bundle"]["work_items_more"] = json!(3);
        value["links"]["work_items_more"] = json!(1);
        value["audit"] = json!({
            "summary": value["audit"]["summary"],
            "references": value["audit"]["references"],
            "error": null,
        });
        value["truncated"] = json!(["commits"]);
        value
    }

    #[test]
    fn every_form_of_a_record_reads_back_unchanged() {
        let mut counted = cut_by_hand();
        counted["evidence"]["uncovered"]["paths"] = json!({"text": 3});
        let mut with_everything = full(3);
        with_everything["host"] = json!("gitlab");
        with_everything["merge_base"] = Value::Null;
        with_everything["rules"]["merge_base_hash"] = Value::Null;
        with_everything["rules"]["merge_base_rules"] = json!("absent");
        with_everything["audit"]["error"] = json!("runner tool \"lint\" is not installed");
        with_everything["sample"] = json!({"rate": 0.25, "sampled": false});
        with_everything["reviewers_more"] = json!(5);
        with_everything["reviewers_pending"] = json!(true);
        with_everything["post_merge_audit"] = json!({"verdict": "pass", "by": "amy", "note": null});
        with_everything["intent"][0]["unresolved"] = json!("the tracker holds no such work item");

        for (name, value) in [
            ("full", full(3)),
            ("full with every optional key", with_everything),
            ("cut by hand", cut_by_hand()),
            ("cut to counts", counted),
            ("minimal", minimal(KEY)),
            ("listing", listing(&COMMITS)),
        ] {
            assert_eq!(json_of(&parsed(&value)), value, "{name}");
        }
    }

    #[test]
    fn a_record_keeps_the_keys_of_spec_8_2_in_order() {
        for (name, value) in [("full", full(3)), ("minimal", minimal(KEY))] {
            assert_eq!(
                written(&value),
                serde_json::to_string(&value).expect("serialize"),
                "{name}"
            );
        }
    }

    #[test]
    fn a_record_with_no_bundle_prints_null_and_a_minimal_record_prints_no_key() {
        let mut value = full(3);
        value["bundle"] = Value::Null;

        let text = written(&value);
        let minimal = written(&minimal(KEY));

        assert!(text.contains(r#""bundle":null"#), "{text}");
        assert!(!minimal.contains("bundle"), "{minimal}");
        assert!(!minimal.contains("evidence"), "{minimal}");
        assert!(!minimal.contains("commits"), "{minimal}");
        assert!(!minimal.contains("links"), "{minimal}");
    }

    #[test]
    fn a_reviewers_more_key_is_absent_unless_the_record_has_one() {
        let text = written(&full(3));

        assert!(!text.contains("reviewers_more"), "{text}");
        assert!(text.contains(r#""reviewers_pending":false"#), "{text}");
    }

    #[test]
    fn the_schema_id_of_the_record_is_houserules_change_record_1_1_0() {
        assert_eq!(
            crate::published::id::<ChangeRecord>(),
            "houserules/change-record@1.1.0"
        );
        assert_eq!(
            json_of(&parsed(&full(0)))["schema"],
            json!("houserules/change-record@1.1.0")
        );
    }

    #[test]
    fn a_document_that_is_no_record_does_not_read() {
        for value in [
            json!({}),
            json!({"schema": "houserules/change-record@1.1.0"}),
            json!([]),
            json!("a record"),
        ] {
            assert!(
                serde_json::from_value::<ChangeRecord>(value.clone()).is_err(),
                "{value}"
            );
        }
    }

    /// A record that holds sessions prints them after the work items, 20 at most and
    /// the count of the rest (the cut of the budget included); a record whose
    /// `links` has no `sessions` key, as a record of schema 1.0.0, reads as none.
    #[test]
    fn the_summary_prints_the_sessions_of_the_record() {
        let session = |number: u32| json!({"commit": KEY, "trailer": "Entire-Checkpoint", "value": format!("cp-{number}")});
        let mut value = full(2);
        value["links"]["sessions"] = json!([session(1), session(2)]);
        value["links"]["sessions_more"] = json!(3);
        let line = summary(&value)
            .lines()
            .find(|line| line.starts_with("Sessions: "))
            .expect("a sessions line")
            .to_string();
        assert_eq!(
            line,
            "Sessions: 4444444 Entire-Checkpoint: `cp-1`, 4444444 Entire-Checkpoint: `cp-2`, ... and 3 more"
        );

        value["links"]["sessions"] = json!((0..25).map(session).collect::<Vec<_>>());
        value["links"]["sessions_more"] = json!(1);
        let long = summary(&value)
            .lines()
            .find(|line| line.starts_with("Sessions: "))
            .expect("a sessions line")
            .to_string();
        assert!(long.ends_with(", ... and 6 more"), "{long}");
        assert_eq!(long.matches("Entire-Checkpoint").count(), 20);

        let mut old = full(2);
        old["links"]
            .as_object_mut()
            .expect("links")
            .remove("sessions");
        assert!(
            summary(&old).contains("Work items: `WI-1`, `WI-2`\nSessions: none\n"),
            "a record with no sessions key reads as none"
        );
    }
}
