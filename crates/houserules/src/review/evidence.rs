//! `houserules evidence`: the reviewer packet of a change (spec 5).
//!
//! [`build`] turns the facts of a range into the [`Evidence`] document: the
//! uncovered surface, the intents, the rows of the audit sorted into the lists
//! that a reviewer reads (failed, warned, proven, judged, skipped), the size,
//! the work items, the sessions, and the route. [`markdown`] prints the packet from the
//! document, for a comment on a pull or merge request. The document holds every
//! item whole; the packet is cut to [`PACKET_LIMIT`] characters in the order of
//! spec 5.8, and each cut list ends with the count of the items it leaves out.
//! The headings and their counts stay.
//!
//! A value that comes from the repository, a tool, or a request description
//! (a path, an evidence text, a work item, an intent) prints through
//! `review::text`, so that none of them changes the layout of the comment.
//!
//! The packet is a function of the document and the install path. It holds no
//! time, no environment value, and no absolute path, so a second run over the
//! same range and the same rules gives the same bytes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use serde_json::Value;

use super::change::{self, Analysis, AuditSource, Coverage};
use super::git;
use super::intent::{self, IntentRef};
use super::links;
use super::route;
use super::rules_hash;
use super::text::{code_span, fenced, token_or_code_span};
use super::validation;
use crate::emit::emit;
use crate::install::marker::review::{Adapter, no_intent_source};
use crate::install::marker::{self, Marker};
use crate::published;
use crate::published::audit_result::{AuditResult, AuditRow, RowMode, RowResult};
use crate::published::change::{ChangeKind, EmptyChange, LineRange, UncoveredPath};
use crate::published::evidence::{
    Diff, Evidence, JudgedRow, PackPin, RuleRow, Rules, Session, Uncovered, Verdict,
};
use crate::published::intent::Intent;
use crate::published::route::RouteDocument;
use crate::rules::load_base;

/// The most characters of the packet (spec 5.8).
const PACKET_LIMIT: usize = 60_000;

/// The most characters of an evidence text before it is cut (spec 5.5).
const EVIDENCE_LIMIT: usize = 300;

/// The most characters of a work-item id in the packet (spec 5.6).
pub(super) const WORK_ITEM_LIMIT: usize = 100;

/// The work items that the packet keeps when the cut reaches the list (spec
/// 5.8).
const WORK_ITEMS_KEPT: usize = 50;

/// The most sessions that the `Sessions:` line lists (batch 38, spec 5.3). The
/// line is bounded, so it stays out of the cut order of spec 5.8.
const SESSIONS_LISTED: usize = 20;

/// The most characters of the value of a session in the packet (batch 38, spec
/// 5.3).
const SESSION_VALUE_LIMIT: usize = 100;

/// The most characters of an intent's why once the cut reaches the intent
/// blocks (spec 5.8).
const CUT_WHY_LIMIT: usize = 300;

/// The line that follows the items of the uncovered surface (spec 5.4).
pub(super) const COVERAGE_LINE: &str =
    "Coverage is per file: a file is covered when a check that declares covers: true read it.";

/// The kinds of deliverable that hold the `rule_adherence` rows of a review.
const REVIEW_KINDS: [&str; 3] = ["task-review", "re-review", "branch-review"];

/// The arguments of `houserules evidence`.
#[derive(Args)]
pub(crate) struct EvidenceArgs {
    /// The base ref of the range. The range runs from the merge base of
    /// `--base` and `--head` to `--head`.
    #[arg(long, value_name = "REF")]
    pub(crate) base: String,
    /// The head ref of the range; defaults to `HEAD`.
    #[arg(long, value_name = "REF")]
    pub(crate) head: Option<String>,
    /// The file that `audit --json` wrote for the same range. Its `base` and
    /// `head` must name the same commits as `--base` and `--head`. Without this
    /// flag, the command runs the audit in process.
    #[arg(long, value_name = "FILE")]
    pub(crate) audit: Option<PathBuf>,
    /// A review deliverable of the kind `task-review`, `re-review`, or
    /// `branch-review`, as JSON. A judged rule shows the result of its
    /// `rule_adherence` row; a rule with no row shows that no verdict is
    /// recorded.
    #[arg(long, value_name = "FILE")]
    pub(crate) review: Option<PathBuf>,
    /// The intent to add, written `<source>:<ref>` as the `Intent:` line of an
    /// evidence bundle writes it. The source is `work-item`, `file`, or `openspec`.
    #[arg(long, value_name = "SOURCE:REF")]
    pub(crate) intent: Option<String>,
    /// The capture file of an `openspec validate --json` run, written by `houserules capture`
    /// at `--head` in a clean work tree. Each `openspec` intent takes the validation of its
    /// change from it. A capture that cannot be used prints one line on stderr and leaves
    /// every validation `null`.
    #[arg(long, value_name = "FILE")]
    pub(crate) validation: Option<PathBuf>,
    /// An external score from 0 to 1 for the route. It needs
    /// `review.route.ownerScore` or `review.route.seniorScore`.
    #[arg(long, value_name = "SCORE", allow_negative_numbers = true)]
    pub(crate) score: Option<f64>,
    /// Writes the packet to this file in place of stdout.
    #[arg(long, value_name = "FILE")]
    pub(crate) out: Option<PathBuf>,
    /// Also writes the evidence document `houserules/evidence@1.1.0` to this
    /// file.
    #[arg(long, value_name = "FILE")]
    pub(crate) json: Option<PathBuf>,
    /// Install root to read from; defaults to the nearest directory at or
    /// above the current one that holds `.houserules.json`, no higher than the
    /// git top level, else the git top level.
    #[arg(long)]
    pub(crate) dir: Option<PathBuf>,
}

/// Where a change came from: the intents that say what it was meant to do and the
/// sessions of the agent tools that wrote it. [`build`] copies both into the
/// document.
pub(crate) struct Provenance {
    /// The intents of the change, in the order of spec 5.8.
    pub(crate) intents: Vec<Intent>,
    /// The sessions that the commits of the range cite, in commit order.
    pub(crate) sessions: Vec<Session>,
}

/// The evidence document of the range of `analysis` (spec 5.9).
///
/// The rows of `audit` go into the lists by mode and result, each list sorted
/// by id: a deterministic row with the result `fail`, `warn`, `pass`, or
/// `skipped` goes into `failed`, `warned`, `proven`, or `skipped`; a judged row
/// goes into `judged` with the verdict of the first row of `review`
/// (`rule_adherence`) that holds its id. `provenance` holds the intents in the
/// order of spec 5.8 and the sessions, `packs` are the packs that the marker pins
/// at the head, and `route` is the decision for the same range.
pub(crate) fn build(
    analysis: &Analysis,
    coverage: &Coverage,
    audit: &AuditResult,
    review: Option<&Value>,
    provenance: Provenance,
    packs: Vec<PackPin>,
    route: RouteDocument,
) -> Evidence {
    let mut failed = Vec::new();
    let mut warned = Vec::new();
    let mut proven = Vec::new();
    let mut skipped = Vec::new();
    let mut judged = Vec::new();
    for row in &audit.rules {
        let list = match (row.mode, row.result) {
            (RowMode::Judged, _) => {
                judged.push(judged_row(row, review));
                continue;
            }
            (RowMode::Deterministic, RowResult::Fail) => &mut failed,
            (RowMode::Deterministic, RowResult::Warn) => &mut warned,
            (RowMode::Deterministic, RowResult::Pass) => &mut proven,
            (RowMode::Deterministic, RowResult::Skipped) => &mut skipped,
            (RowMode::Deterministic, RowResult::Open) => continue,
        };
        list.push(RuleRow {
            id: row.id.clone(),
            evidence: row.evidence.clone(),
        });
    }
    for list in [&mut failed, &mut warned, &mut proven, &mut skipped] {
        list.sort_by(|a, b| a.id.cmp(&b.id));
    }
    judged.sort_by(|a, b| a.id.cmp(&b.id));
    let mut work_items = analysis.work_items.clone();
    work_items.sort();
    Evidence {
        schema: published::id::<Evidence>(),
        install: analysis.install.clone(),
        base: analysis.base.clone(),
        head: analysis.head.clone(),
        merge_base: analysis.merge_base.clone(),
        intent: provenance.intents,
        rules: Rules {
            merge_base_hash: analysis.rules.merge_base_hash.clone(),
            head_hash: analysis.rules.head_hash.clone(),
            packs,
        },
        diff: Diff {
            files: analysis.diff.files,
            added: analysis.diff.added,
            deleted: analysis.diff.deleted,
            lines: analysis.diff.lines,
            excluded: analysis.diff.excluded,
        },
        uncovered: Uncovered {
            lines: coverage.uncovered_lines,
            total_lines: coverage.total_lines,
            share: coverage.share,
            paths: coverage.uncovered.iter().map(UncoveredPath::from).collect(),
        },
        failed,
        warned,
        proven,
        judged,
        skipped,
        work_items,
        sessions: provenance.sessions,
        route,
    }
}

/// The judged row for `row`: the verdict and the evidence of the first row of
/// `review.rule_adherence` that holds the id of `row`. A result other than
/// `pass`, `fail`, and `warn` is the verdict `open`.
fn judged_row(row: &AuditRow, review: Option<&Value>) -> JudgedRow {
    let found = review
        .and_then(|review| review["rule_adherence"].as_array())
        .and_then(|rows| rows.iter().find(|candidate| candidate["id"] == row.id));
    JudgedRow {
        id: row.id.clone(),
        verdict: found.map(|found| match found["result"].as_str() {
            Some("pass") => Verdict::Pass,
            Some("fail") => Verdict::Fail,
            Some("warn") => Verdict::Warn,
            _ => Verdict::Open,
        }),
        evidence: found.and_then(|found| found["evidence"].as_str().map(str::to_string)),
    }
}

/// Reads a review deliverable (`task-review`, `re-review`, or `branch-review`)
/// from `file`. Its `rule_adherence` must be a list of rows that each hold an
/// id. The `Err` is the line of spec 5.1 for a file that is no review.
pub(crate) fn read_review(file: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let not_a_review =
        |reason: String| format!("{}: not a review deliverable: {reason}", file.display());
    let review: Value =
        serde_json::from_str(&text).map_err(|error| not_a_review(error.to_string()))?;
    if !REVIEW_KINDS.iter().any(|kind| review["kind"] == *kind) {
        return Err(not_a_review(
            "kind must be \"task-review\", \"re-review\", or \"branch-review\"".to_string(),
        ));
    }
    let Some(rows) = review["rule_adherence"].as_array() else {
        return Err(not_a_review("rule_adherence must be an array".to_string()));
    };
    if let Some(index) = rows.iter().position(|row| !row["id"].is_string()) {
        return Err(not_a_review(format!(
            "rule_adherence[{index}] must be an object with an id"
        )));
    }
    Ok(review)
}

// ---- the packet ---------------------------------------------------------------------

/// The first `limit` characters of `text`.
pub(super) fn first_chars(text: &str, limit: usize) -> &str {
    match text.char_indices().nth(limit) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

/// `text`, or its first [`EVIDENCE_LIMIT`] characters and `...` when it is
/// longer (spec 5.5).
fn cut_evidence(text: &str) -> String {
    let kept = first_chars(text, EVIDENCE_LIMIT);
    if kept.len() < text.len() {
        format!("{kept}...")
    } else {
        text.to_string()
    }
}

/// The ranges of a path as the packet prints them: `lines <a>, <b>-<c>`, then
/// one `deleted after line <n>: <k> lines` for each hunk that only deletes
/// lines, joined by `; `. A line range of one line prints `<n>` unless
/// `span_always` asks for `<n>-<n>`.
fn ranges_text(ranges: &[LineRange], span_always: bool) -> String {
    let lines: Vec<String> = ranges
        .iter()
        .filter_map(|range| match range {
            LineRange::Lines { start, end } if start == end && !span_always => {
                Some(start.to_string())
            }
            LineRange::Lines { start, end } => Some(format!("{start}-{end}")),
            LineRange::DeletedAfter { .. } => None,
        })
        .collect();
    let mut parts = Vec::new();
    if !lines.is_empty() {
        parts.push(format!("lines {}", lines.join(", ")));
    }
    parts.extend(ranges.iter().filter_map(|range| match range {
        LineRange::DeletedAfter { after, deleted } => {
            Some(format!("deleted after line {after}: {deleted} lines"))
        }
        LineRange::Lines { .. } => None,
    }));
    parts.join("; ")
}

/// The packet line of an uncovered path, by its kind (spec 4.2). A kind whose
/// fact the document lacks prints the word of the kind.
pub(super) fn packet_line(path: &UncoveredPath) -> String {
    let renamed_from = path
        .old_path
        .as_ref()
        .map(|old| format!("renamed from {}", code_span(old)));
    match path.kind {
        ChangeKind::Text => {
            let mut parts = vec![ranges_text(&path.ranges, false)];
            parts.extend(renamed_from);
            parts.retain(|part| !part.is_empty());
            if parts.is_empty() {
                "text".to_string()
            } else {
                parts.join("; ")
            }
        }
        ChangeKind::Deleted => match ranges_text(&path.ranges, true).as_str() {
            "" => "deleted".to_string(),
            ranges => format!("deleted, {ranges} of the old file"),
        },
        ChangeKind::Renamed => renamed_from.unwrap_or_else(|| "renamed".to_string()),
        ChangeKind::Empty => match path.empty {
            Some(EmptyChange::Added) => "empty file added".to_string(),
            Some(EmptyChange::Deleted) => "empty file deleted".to_string(),
            None => "empty file".to_string(),
        },
        ChangeKind::Binary => "binary".to_string(),
        ChangeKind::Mode => "mode only".to_string(),
        ChangeKind::Symlink => match ranges_text(&path.ranges, true).as_str() {
            "" => "symlink".to_string(),
            ranges => format!("symlink, {ranges}"),
        },
        ChangeKind::Submodule => "submodule".to_string(),
    }
}

/// The `Sessions:` line (batch 38, spec 5.3): `sessions` as `<sha7> <key>:
/// <value>`, each value cut to [`SESSION_VALUE_LIMIT`] characters and in a code
/// span, joined by `, `. A sha and a key print as they are, and in a code span
/// when a note gave them with a character outside letters, digits, and `-`
/// ([`token_or_code_span`]). The line lists at most [`SESSIONS_LISTED`] sessions and
/// ends with `... and <k> more`, where `k` counts the sessions that it leaves out
/// and the `more` that a cut of the record already left out. A line with no
/// session is `Sessions: none`.
pub(super) fn sessions_line(sessions: &[Session], more: u64) -> String {
    let left_out = more + sessions.len().saturating_sub(SESSIONS_LISTED) as u64;
    if sessions.is_empty() && left_out == 0 {
        return "Sessions: none".to_string();
    }
    let mut parts: Vec<String> = sessions
        .iter()
        .take(SESSIONS_LISTED)
        .map(|session| {
            format!(
                "{} {}: {}",
                token_or_code_span(git::short(&session.commit)),
                token_or_code_span(&session.trailer),
                code_span(first_chars(&session.value, SESSION_VALUE_LIMIT))
            )
        })
        .collect();
    if left_out > 0 {
        parts.push(format!("... and {left_out} more"));
    }
    format!("Sessions: {}", parts.join(", "))
}

/// The item of a rule row: its id and its evidence, cut and in code spans.
pub(super) fn rule_line(row: &RuleRow) -> String {
    format!(
        "- {}: {}",
        code_span(&row.id),
        code_span(&cut_evidence(&row.evidence))
    )
}

/// The item of a judged row: its id, the verdict of the review and that row's
/// evidence, or the words that no verdict is recorded.
pub(super) fn judged_line(row: &JudgedRow) -> String {
    let id = code_span(&row.id);
    match (row.verdict, &row.evidence) {
        (None, _) => format!("- {id}: no verdict recorded"),
        (Some(verdict), None) => format!("- {id}: {}", verdict.as_str()),
        (Some(verdict), Some(evidence)) => format!(
            "- {id}: {} · {}",
            verdict.as_str(),
            code_span(&cut_evidence(evidence))
        ),
    }
}

/// A count of a requirement for the packet: the number, or `?` for a count the
/// source does not give.
fn count_or_question(count: Option<u64>) -> String {
    count.map_or_else(|| "?".to_string(), |count| count.to_string())
}

/// The block of one intent (spec 5.3): its identity and title, then a line for
/// each field that is not `null`. The why prints in a fenced block, cut to
/// `why_limit` characters when a limit is given. A named intent that does not
/// resolve prints its identity and the line of its adapter, which is the
/// binary's own text and prints as it is, and no other line.
pub(super) fn intent_block(intent: &Intent, why_limit: Option<usize>) -> String {
    let identity = code_span(&format!("{} {}", intent.source, intent.reference));
    if let Some(line) = &intent.unresolved {
        return format!("{identity}: {line}");
    }
    let title = intent
        .title
        .as_deref()
        .map_or_else(|| "no title".to_string(), code_span);
    let mut lines = vec![format!("{identity}: {title}")];
    if let Some(why) = &intent.why {
        let why = why_limit.map_or(why.as_str(), |limit| first_chars(why, limit));
        lines.push("Why:".to_string());
        lines.push(fenced(why));
    }
    if let Some(tasks) = &intent.tasks {
        lines.push(format!("Tasks: {} of {} done", tasks.done, tasks.total));
    }
    if let Some(requirements) = &intent.requirements {
        lines.push(format!(
            "Requirements: +{} ~{} -{} renamed {} (total {})",
            count_or_question(requirements.added),
            count_or_question(requirements.modified),
            count_or_question(requirements.removed),
            count_or_question(requirements.renamed),
            count_or_question(requirements.total)
        ));
    }
    if let Some(validation) = &intent.validation {
        lines.push(format!(
            "Validation: {} {}: {}",
            code_span(&validation.tool),
            if validation.ok { "ok" } else { "failed" },
            code_span(&validation.summary)
        ));
    }
    lines.join("\n")
}

/// The lines of the list `items`, of which the first `keep` print, and the line
/// that counts the rest. A list with no item prints `None.`. Every line ends
/// with a line feed.
fn bullets(items: &[String], keep: usize) -> String {
    if items.is_empty() {
        return "None.\n".to_string();
    }
    let mut text = String::new();
    for item in &items[..keep] {
        text.push_str(item);
        text.push('\n');
    }
    if keep < items.len() {
        text.push_str(&format!("- ... and {} more\n", items.len() - keep));
    }
    text
}

/// The lists of the packet that the cap may shorten, in the order of the cut
/// (spec 5.8).
#[derive(Clone, Copy)]
enum Cuttable {
    /// The uncovered paths.
    Uncovered,
    /// The proven rows.
    Proven,
    /// The judged rows.
    Judged,
    /// The warned rows.
    Warned,
    /// The failed rows.
    Failed,
    /// The reason lines of the route.
    Route,
    /// The intent blocks.
    Intents,
}

/// How much of each cuttable list the packet keeps.
#[derive(Clone)]
struct Keep {
    /// The uncovered paths that print.
    uncovered: usize,
    /// The proven rows that print.
    proven: usize,
    /// The judged rows that print.
    judged: usize,
    /// The warned rows that print.
    warned: usize,
    /// The failed rows that print.
    failed: usize,
    /// The reason lines of the route that print.
    route: usize,
    /// The work items that print.
    work_items: usize,
    /// The intent blocks that print.
    intents: usize,
    /// The limit of the characters of a why, once the cut reaches the intents.
    why_limit: Option<usize>,
}

impl Keep {
    /// The count that `list` keeps.
    fn slot(&mut self, list: Cuttable) -> &mut usize {
        match list {
            Cuttable::Uncovered => &mut self.uncovered,
            Cuttable::Proven => &mut self.proven,
            Cuttable::Judged => &mut self.judged,
            Cuttable::Warned => &mut self.warned,
            Cuttable::Failed => &mut self.failed,
            Cuttable::Route => &mut self.route,
            Cuttable::Intents => &mut self.intents,
        }
    }
}

/// The items of the packet, printed once, and the cut that the cap needs.
struct Packet<'a> {
    /// The document that the packet prints.
    document: &'a Evidence,
    /// The install path of the marker line.
    install: &'a str,
    /// The items of the uncovered surface.
    uncovered: Vec<String>,
    /// The items of the proven rows.
    proven: Vec<String>,
    /// The items of the judged rows.
    judged: Vec<String>,
    /// The items of the warned rows.
    warned: Vec<String>,
    /// The items of the failed rows.
    failed: Vec<String>,
    /// The items of the reason lines of the route.
    route: Vec<String>,
    /// The ids of the work items, each in a code span.
    work_items: Vec<String>,
}

impl<'a> Packet<'a> {
    /// The items of `document`.
    fn new(document: &'a Evidence, install: &'a str) -> Self {
        let lines = |rows: &[RuleRow]| rows.iter().map(rule_line).collect();
        Self {
            document,
            install,
            uncovered: document
                .uncovered
                .paths
                .iter()
                .map(|path| format!("- {}: {}", code_span(&path.path), packet_line(path)))
                .collect(),
            proven: lines(&document.proven),
            judged: document.judged.iter().map(judged_line).collect(),
            warned: lines(&document.warned),
            failed: lines(&document.failed),
            route: route::reason_lines(&document.route)
                .into_iter()
                .map(|line| format!("- {line}"))
                .collect(),
            work_items: document
                .work_items
                .iter()
                .map(|id| code_span(first_chars(id, WORK_ITEM_LIMIT)))
                .collect(),
        }
    }

    /// The number of items of `list`.
    fn len(&self, list: Cuttable) -> usize {
        match list {
            Cuttable::Uncovered => self.uncovered.len(),
            Cuttable::Proven => self.proven.len(),
            Cuttable::Judged => self.judged.len(),
            Cuttable::Warned => self.warned.len(),
            Cuttable::Failed => self.failed.len(),
            Cuttable::Route => self.route.len(),
            Cuttable::Intents => self.document.intent.len(),
        }
    }

    /// The cut that keeps every item.
    fn whole(&self) -> Keep {
        Keep {
            uncovered: self.uncovered.len(),
            proven: self.proven.len(),
            judged: self.judged.len(),
            warned: self.warned.len(),
            failed: self.failed.len(),
            route: self.route.len(),
            work_items: self.work_items.len(),
            intents: self.document.intent.len(),
            why_limit: None,
        }
    }

    /// The range line of the packet (spec 5.2).
    fn range_line(&self) -> String {
        let document = self.document;
        format!(
            "Range {}..{} (merge base {}) · rules {} · route {}",
            git::short(&document.base),
            git::short(&document.head),
            document.merge_base.as_deref().map_or("none", git::short),
            first_chars(&document.rules.head_hash, 12),
            document.route.route.as_str()
        )
    }

    /// The `Work items:` line, with the first `keep` ids.
    fn work_items_line(&self, keep: usize) -> String {
        if self.work_items.is_empty() {
            return "Work items: none".to_string();
        }
        let mut parts: Vec<String> = self.work_items[..keep].to_vec();
        if keep < self.work_items.len() {
            parts.push(format!("... and {} more", self.work_items.len() - keep));
        }
        format!("Work items: {}", parts.join(", "))
    }

    /// The intent blocks, of which the first `keep` print, and the line that
    /// counts the rest.
    fn intent_section(&self, keep: &Keep) -> String {
        let intents = &self.document.intent;
        if intents.is_empty() {
            return "None recorded.\n".to_string();
        }
        let mut blocks: Vec<String> = intents[..keep.intents]
            .iter()
            .map(|intent| intent_block(intent, keep.why_limit))
            .collect();
        if keep.intents < intents.len() {
            blocks.push(format!("... and {} more", intents.len() - keep.intents));
        }
        format!("{}\n", blocks.join("\n\n"))
    }

    /// The packet with the cut `keep`.
    fn render(&self, keep: &Keep) -> String {
        let document = self.document;
        let mut text = format!(
            "<!-- houserules:evidence {} -->\n## houserules evidence\n\n{}\n\n",
            self.install,
            self.range_line()
        );
        text.push_str(&format!(
            "### Uncovered surface ({} of {} changed lines)\n\n",
            document.uncovered.lines, document.uncovered.total_lines
        ));
        text.push_str(&bullets(&self.uncovered, keep.uncovered));
        if !self.uncovered.is_empty() {
            text.push_str(&format!("\n{COVERAGE_LINE}\n"));
        }
        text.push_str(&format!("\n### Intent ({})\n\n", document.intent.len()));
        text.push_str(&self.intent_section(keep));
        for (heading, count, items, kept) in [
            ("Failed", document.failed.len(), &self.failed, keep.failed),
            ("Warned", document.warned.len(), &self.warned, keep.warned),
            ("Proven", document.proven.len(), &self.proven, keep.proven),
        ] {
            text.push_str(&format!("\n### {heading} ({count})\n\n"));
            text.push_str(&bullets(items, kept));
        }
        text.push_str(&format!(
            "\n### Judged only ({})\n\n",
            document.judged.len()
        ));
        text.push_str(&bullets(&self.judged, keep.judged));
        text.push_str(&format!(
            "\n{} deterministic checks did not apply to this change.\n",
            document.skipped.len()
        ));
        text.push_str(&format!(
            "\n### Size and links\n\nDiff: +{} -{} lines in {} files",
            document.diff.added, document.diff.deleted, document.diff.files
        ));
        if document.diff.excluded > 0 {
            text.push_str(&format!(
                "; {} files excluded by review.diffExclude",
                document.diff.excluded
            ));
        }
        text.push_str(&format!("\n{}\n", self.work_items_line(keep.work_items)));
        text.push_str(&format!("{}\n", sessions_line(&document.sessions, 0)));
        text.push_str(&format!(
            "\n### Route\n\n{}\n",
            document.route.route.as_str()
        ));
        if !self.route.is_empty() {
            text.push('\n');
            text.push_str(&bullets(&self.route, keep.route));
        }
        text
    }

    /// `true` when the packet with the cut `keep` is over the cap.
    fn over(&self, keep: &Keep) -> bool {
        self.render(keep).chars().count() > PACKET_LIMIT
    }

    /// Cuts `list` to the most items that bring the packet within the cap, or
    /// to none when even that does not suffice.
    fn shrink(&self, keep: &mut Keep, list: Cuttable) {
        let mut fits = 0;
        let mut over = self.len(list);
        while over - fits > 1 {
            let middle = fits + (over - fits) / 2;
            *keep.slot(list) = middle;
            if self.over(keep) {
                over = middle;
            } else {
                fits = middle;
            }
        }
        *keep.slot(list) = fits;
        while fits > 0 && self.over(keep) {
            fits -= 1;
            *keep.slot(list) = fits;
        }
    }

    /// The packet within the cap (spec 5.8). A packet over the cap loses items
    /// from the lists in this order, each list to the items that fit before the
    /// next list is touched: uncovered, proven, judged, warned, failed, the
    /// reason lines of the route, the work items (to the first
    /// [`WORK_ITEMS_KEPT`]), and the intent blocks (each why first cut to
    /// [`CUT_WHY_LIMIT`] characters).
    fn fit(&self) -> String {
        let mut keep = self.whole();
        for list in [
            Cuttable::Uncovered,
            Cuttable::Proven,
            Cuttable::Judged,
            Cuttable::Warned,
            Cuttable::Failed,
            Cuttable::Route,
        ] {
            if !self.over(&keep) {
                break;
            }
            self.shrink(&mut keep, list);
        }
        if self.over(&keep) {
            keep.work_items = keep.work_items.min(WORK_ITEMS_KEPT);
        }
        if self.over(&keep) {
            keep.why_limit = Some(CUT_WHY_LIMIT);
        }
        if self.over(&keep) {
            self.shrink(&mut keep, Cuttable::Intents);
        }
        self.render(&keep)
    }
}

/// The Markdown packet of `document` for the install at `install` (spec 5.2 to
/// 5.8). `install` is the path of the install from the git top level, `.` for a
/// top-level install; the units match the marker line that names it. The packet
/// is at most [`PACKET_LIMIT`] characters.
pub(crate) fn markdown(document: &Evidence, install: &str) -> String {
    Packet::new(document, install).fit()
}

// ---- the command --------------------------------------------------------------------

/// Gives the score as the route reads it, or the `Err` line of spec 5.1 for a
/// score outside 0 to 1. A score of `-0` reads as 0.
fn score_in_range(score: f64) -> Result<f64, String> {
    if (0.0..=1.0).contains(&score) {
        Ok(score.abs())
    } else {
        Err(format!("evidence: --score {score} is outside 0 to 1"))
    }
}

/// Reads `--intent`: `<source>:<ref>` with a source that is an adapter. The
/// `Err` is the line of spec 5.1 for an intent that cannot be named.
fn parse_intent(text: &str) -> Result<IntentRef, String> {
    let named =
        IntentRef::parse(text).map_err(|line| format!("evidence: --intent {text}: {line}"))?;
    if Adapter::named(&named.source).is_none() {
        return Err(format!(
            "evidence: --intent {}:{}: {}",
            named.source,
            named.reference,
            no_intent_source(&named.source)
        ));
    }
    Ok(named)
}

/// The packs that the marker of the install pins at `commit`, read from git as
/// the rules hash of that commit is (spec 5.9), sorted by name. A commit with
/// no marker pins none. A fault of the marker is one line that names the marker
/// by the commit and its path from the git top level.
pub(super) fn pack_pins_at(
    root: &Path,
    install: &str,
    commit: &str,
) -> Result<Vec<PackPin>, String> {
    let Some(text) = rules_hash::marker_at(root, install, commit)? else {
        return Ok(Vec::new());
    };
    let file = format!(
        "{}:{}",
        git::short(commit),
        rules_hash::marker_path(install)
    );
    Ok(marker::pins_of_text(&file, &text)?
        .into_iter()
        .map(|(name, pin)| PackPin {
            name,
            version: pin.version,
            sha256: pin.sha256,
        })
        .collect())
}

/// Runs `houserules evidence`: prints the packet, or writes it to `--out`, and
/// writes `--json`. A setup error is one line on stderr for each fault and exit
/// 2 (`houserules.crash-paths-are-named`).
pub(crate) fn cmd_evidence(args: EvidenceArgs) -> ExitCode {
    let root = match crate::root::resolve_root(args.dir.clone()) {
        Ok(root) => root,
        Err(code) => return code,
    };
    match run(&root, &args) {
        Ok(printed) => {
            print!("{}", printed.stdout);
            if let Some(line) = printed.notice {
                eprintln!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(lines) => {
            for line in lines {
                eprintln!("{line}");
            }
            ExitCode::from(2)
        }
    }
}

/// What a run of `evidence` that succeeded prints.
struct Printed {
    /// The text for stdout: the packet, empty when `--out` names a file.
    stdout: String,
    /// The line of `--validation` for stderr, when the capture was refused.
    notice: Option<String>,
}

/// The text that `evidence` prints, after it wrote `--json` and `--out`. A failure after the
/// capture was read has the line of a refused capture before its own lines.
fn run(root: &Path, args: &EvidenceArgs) -> Result<Printed, Vec<String>> {
    let one = |line: String| vec![line];
    let score = args.score.map(score_in_range).transpose().map_err(one)?;
    let review = args
        .review
        .as_deref()
        .map(read_review)
        .transpose()
        .map_err(one)?;
    let named = args
        .intent
        .as_deref()
        .map(parse_intent)
        .transpose()
        .map_err(one)?;
    git::require_attr_source("evidence").map_err(one)?;
    let base = load_base(root).map_err(|error| vec![error.to_string()])?;
    let marker = Marker::read_or_default_for_gate(root, "WI").map_err(one)?;
    let config = marker.review()?;
    let head_ref = args.head.as_deref().unwrap_or("HEAD");
    let analysis = change::analyze(root, &base, &config, &args.base, head_ref).map_err(one)?;
    let packs = pack_pins_at(root, &analysis.install, &analysis.head).map_err(one)?;
    let source = match &args.audit {
        Some(file) => AuditSource::File(file.clone()),
        None => AuditSource::InProcess,
    };
    let result = change::audit_rows(root, &base, source, &analysis).map_err(one)?;
    let coverage = change::coverage(&analysis, &result, &base);
    let decision =
        route::decide(&analysis, Some((&result, &coverage)), &config.route, score).map_err(one)?;
    let linked = links::read(
        root,
        &analysis.base,
        &analysis.head,
        &config.sessions.trailers,
    )
    .map_err(one)?;
    let mut intents =
        intent::resolve(root, &analysis, &config.intent, &linked.names(named)).map_err(one)?;
    let notice = validation::fill(
        "evidence",
        args.validation.as_deref(),
        &analysis.head,
        &mut intents,
    );
    let document = build(
        &analysis,
        &coverage,
        &result,
        review.as_ref(),
        Provenance {
            intents,
            sessions: linked.sessions,
        },
        packs,
        decision,
    );
    match write_outputs(args, &document) {
        Ok(stdout) => Ok(Printed { stdout, notice }),
        Err(lines) => Err(notice.into_iter().chain(lines).collect()),
    }
}

/// Writes `--json` and `--out` for `document` and gives the text for stdout: the packet, or
/// nothing when `--out` names a file. The `Err` is the line of the write that failed.
fn write_outputs(args: &EvidenceArgs, document: &Evidence) -> Result<String, Vec<String>> {
    if let Some(file) = &args.json {
        let text = emit(&serde_json::to_value(document).expect("an evidence document serializes"));
        fs::write(file, text).map_err(|error| vec![format!("{}: {error}", file.display())])?;
    }
    let packet = markdown(document, &document.install);
    match &args.out {
        Some(file) => {
            fs::write(file, packet)
                .map_err(|error| vec![format!("{}: {error}", file.display())])?;
            Ok(String::new())
        }
        None => Ok(packet),
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use serde_json::{Value, json};

    use super::*;
    use crate::published::audit_result::{
        AuditResult, AuditRow, References, RowLevel, RowMode, RowResult, Summary,
    };
    use crate::published::change::{ChangeKind, EmptyChange, LineRange, UncoveredPath};
    use crate::published::evidence::{
        Diff, Evidence, JudgedRow, PackPin, RuleRow, Rules, Uncovered, Verdict,
    };
    use crate::published::intent::{Intent, Requirements, Tasks, Validation};
    use crate::published::route::{Inputs, Reason, ReasonRule, Route, RouteDocument};
    use crate::review::change::{
        Analysis, ChangedPath, Coverage, DiffSize, RulesHashes, RulesPresence,
    };

    /// The most characters of a packet.
    const LIMIT: usize = 60_000;

    // ---- fixtures ---------------------------------------------------------------------

    /// A route decision for the range of [`doc`]: `owner-review`, with the
    /// reasons `reasons`.
    fn route_document(reasons: Vec<Reason>) -> RouteDocument {
        RouteDocument {
            schema: "houserules/route@1.0.0".to_string(),
            install: ".".to_string(),
            head: "2".repeat(40),
            route: Route::OwnerReview,
            floor: false,
            inputs: Inputs {
                failed: Vec::new(),
                warned: Vec::new(),
                areas: IndexMap::new(),
                no_area: Vec::new(),
                paths: Vec::new(),
                diff_lines: 0,
                uncovered_share: Some(0.0),
                score: None,
                rule_sources_changed: Vec::new(),
            },
            reasons,
        }
    }

    /// A decisive reason of the rule `failed` about the row `id`.
    fn failed_reason(id: &str) -> Reason {
        Reason {
            rule: ReasonRule::Failed,
            input: id.to_string(),
            value: None,
            threshold: None,
            route: Route::OwnerReview,
            paths: Vec::new(),
            more_paths: 0,
            decisive: true,
        }
    }

    /// A document of a top-level install with no list entry.
    fn doc() -> Evidence {
        Evidence {
            schema: "houserules/evidence@1.1.0".to_string(),
            install: ".".to_string(),
            base: "1".repeat(40),
            head: "2".repeat(40),
            merge_base: Some("3".repeat(40)),
            intent: Vec::new(),
            rules: Rules {
                merge_base_hash: Some("4".repeat(64)),
                head_hash: "0123456789abcdef".repeat(4),
                packs: Vec::new(),
            },
            diff: Diff {
                files: 0,
                added: 0,
                deleted: 0,
                lines: 0,
                excluded: 0,
            },
            uncovered: Uncovered {
                lines: 0,
                total_lines: 0,
                share: 0.0,
                paths: Vec::new(),
            },
            failed: Vec::new(),
            warned: Vec::new(),
            proven: Vec::new(),
            judged: Vec::new(),
            skipped: Vec::new(),
            work_items: Vec::new(),
            sessions: Vec::new(),
            route: route_document(Vec::new()),
        }
    }

    /// An uncovered text path whose one hunk is the line `line`.
    fn uncovered_path(path: &str, line: u64) -> UncoveredPath {
        UncoveredPath {
            path: path.to_string(),
            old_path: None,
            kind: ChangeKind::Text,
            ranges: vec![LineRange::Lines {
                start: line,
                end: line,
            }],
            empty: None,
        }
    }

    /// A row of a list.
    fn rule_row(id: &str, evidence: &str) -> RuleRow {
        RuleRow {
            id: id.to_string(),
            evidence: evidence.to_string(),
        }
    }

    /// An intent that fills the title only.
    fn titled(source: &str, reference: &str, title: Option<&str>) -> Intent {
        Intent {
            source: source.to_string(),
            reference: reference.to_string(),
            title: title.map(str::to_string),
            why: None,
            tasks: None,
            requirements: None,
            validation: None,
            artifacts: Vec::new(),
            unresolved: None,
        }
    }

    /// The provenance of `intents` and `sessions`.
    fn provenance(intents: Vec<Intent>, sessions: Vec<Session>) -> Provenance {
        Provenance { intents, sessions }
    }

    /// A session of the commit that starts with `commit`.
    fn session(commit: &str, trailer: &str, value: &str) -> Session {
        Session {
            commit: commit.repeat(40),
            trailer: trailer.to_string(),
            value: value.to_string(),
        }
    }

    /// The lines of the section that starts with `heading`: those after the
    /// heading line up to the next heading, without the closing blank lines.
    fn section<'a>(packet: &'a str, heading: &str) -> Vec<&'a str> {
        let mut lines: Vec<&str> = packet
            .lines()
            .skip_while(|line| !line.starts_with(heading))
            .skip(1)
            .take_while(|line| !line.starts_with("### "))
            .collect();
        while lines.last() == Some(&"") {
            lines.pop();
        }
        lines
    }

    // ---- build ------------------------------------------------------------------------

    /// An analysis with a few changed paths, for the tests of [`build`].
    fn analysis() -> Analysis {
        Analysis {
            install: "sub".to_string(),
            base: "b".repeat(40),
            head: "h".repeat(40),
            merge_base: Some("m".repeat(40)),
            paths: vec![ChangedPath {
                path: "src/a.rs".to_string(),
                old_path: None,
                kind: ChangeKind::Text,
                added: 3,
                deleted: 1,
                ranges: vec![LineRange::Lines { start: 1, end: 3 }],
                excluded: false,
                empty: None,
            }],
            diff: DiffSize {
                files: 2,
                added: 5,
                deleted: 4,
                lines: 9,
                excluded: 1,
            },
            rules: RulesHashes {
                merge_base_hash: Some("a".repeat(64)),
                merge_base_rules: RulesPresence::Present,
                head_hash: "c".repeat(64),
            },
            work_items: vec!["HR-2".to_string(), "HR-10".to_string(), "HR-1".to_string()],
            areas: IndexMap::new(),
            no_area: vec!["src/a.rs".to_string()],
            rule_sources_changed: Vec::new(),
        }
    }

    /// A coverage that leaves the path of [`analysis`] uncovered.
    fn coverage(analysis: &Analysis) -> Coverage {
        Coverage {
            uncovered: analysis.paths.clone(),
            uncovered_lines: 4,
            total_lines: 8,
            share: 0.5,
        }
    }

    /// An audit row of `mode` and `result`.
    fn row(id: &str, mode: RowMode, result: RowResult, evidence: &str) -> AuditRow {
        AuditRow {
            id: id.to_string(),
            kind: "rule".to_string(),
            mode,
            level: (mode == RowMode::Deterministic).then_some(RowLevel::Fail),
            result,
            evidence: evidence.to_string(),
            findings: None,
            sanctioned: None,
        }
    }

    /// An audit result that holds `rows`.
    fn audit(rows: Vec<AuditRow>) -> AuditResult {
        AuditResult {
            schema: "houserules/audit-result@1.0.0".to_string(),
            base: "bbbbbbb".to_string(),
            head: "hhhhhhh".to_string(),
            ids: Vec::new(),
            changed_files: Vec::new(),
            areas: Vec::new(),
            area_files: IndexMap::new(),
            rules: rows,
            summary: Summary {
                base: "bbbbbbb".to_string(),
                head: "hhhhhhh".to_string(),
                deterministic: 0,
                pass: 0,
                fail: 0,
                warn: 0,
                skipped: 0,
                judged: 0,
                empty_range: None,
                sanctioned_fail: None,
                reference_fail: 0,
            },
            stale_sanctions: None,
            references: References::default(),
        }
    }

    #[test]
    fn build_sorts_the_rows_into_the_five_lists_by_id() {
        let result = audit(vec![
            row("z.judged", RowMode::Judged, RowResult::Open, "—"),
            row("b.pass", RowMode::Deterministic, RowResult::Pass, "ok"),
            row(
                "a.fail",
                RowMode::Deterministic,
                RowResult::Fail,
                "bad (sanctioned: HR-1)",
            ),
            row("c.warn", RowMode::Deterministic, RowResult::Warn, "careful"),
            row("a.pass", RowMode::Deterministic, RowResult::Pass, "fine"),
            row(
                "d.skip",
                RowMode::Deterministic,
                RowResult::Skipped,
                "no --report given",
            ),
            row("a.judged", RowMode::Judged, RowResult::Open, "—"),
        ]);
        let analysis = analysis();

        let built = build(
            &analysis,
            &coverage(&analysis),
            &result,
            None,
            provenance(Vec::new(), Vec::new()),
            Vec::new(),
            route_document(Vec::new()),
        );

        assert_eq!(built.failed, [rule_row("a.fail", "bad (sanctioned: HR-1)")]);
        assert_eq!(built.warned, [rule_row("c.warn", "careful")]);
        assert_eq!(
            built.proven,
            [rule_row("a.pass", "fine"), rule_row("b.pass", "ok")]
        );
        assert_eq!(built.skipped, [rule_row("d.skip", "no --report given")]);
        let judged: Vec<&str> = built.judged.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(judged, ["a.judged", "z.judged"]);
        assert!(
            built
                .judged
                .iter()
                .all(|row| row.verdict.is_none() && row.evidence.is_none())
        );
    }

    #[test]
    fn build_copies_the_facts_of_the_range() {
        let analysis = analysis();
        let packs = vec![PackPin {
            name: "process".to_string(),
            version: "2.0.1".to_string(),
            sha256: "d".repeat(64),
        }];
        let intent = vec![titled("work-item", "HR-1", Some("A title"))];
        let sessions = vec![session("c", "Entire-Checkpoint", "cp-1")];

        let built = build(
            &analysis,
            &coverage(&analysis),
            &audit(Vec::new()),
            None,
            provenance(intent.clone(), sessions.clone()),
            packs.clone(),
            route_document(Vec::new()),
        );

        assert_eq!(built.schema, "houserules/evidence@1.1.0");
        assert_eq!(built.install, "sub");
        assert_eq!(built.base, "b".repeat(40));
        assert_eq!(built.head, "h".repeat(40));
        assert_eq!(built.merge_base, Some("m".repeat(40)));
        assert_eq!(built.intent, intent);
        assert_eq!(built.sessions, sessions);
        assert_eq!(built.rules.merge_base_hash, Some("a".repeat(64)));
        assert_eq!(built.rules.head_hash, "c".repeat(64));
        assert_eq!(built.rules.packs, packs);
        assert_eq!(
            (
                built.diff.files,
                built.diff.added,
                built.diff.deleted,
                built.diff.lines,
                built.diff.excluded
            ),
            (2, 5, 4, 9, 1)
        );
        assert_eq!(
            (
                built.uncovered.lines,
                built.uncovered.total_lines,
                built.uncovered.share
            ),
            (4, 8, 0.5)
        );
        assert_eq!(
            built.uncovered.paths,
            [UncoveredPath {
                path: "src/a.rs".to_string(),
                old_path: None,
                kind: ChangeKind::Text,
                ranges: vec![LineRange::Lines { start: 1, end: 3 }],
                empty: None,
            }]
        );
        assert_eq!(built.work_items, ["HR-1", "HR-10", "HR-2"], "sorted by id");
    }

    #[test]
    fn build_reads_the_verdict_and_the_evidence_of_the_review_row() {
        let result = audit(vec![
            row("a.pass", RowMode::Judged, RowResult::Open, "—"),
            row("b.fail", RowMode::Judged, RowResult::Open, "—"),
            row("c.warn", RowMode::Judged, RowResult::Open, "—"),
            row("d.skipped", RowMode::Judged, RowResult::Open, "—"),
            row("e.other", RowMode::Judged, RowResult::Open, "—"),
            row("f.none", RowMode::Judged, RowResult::Open, "—"),
            row("g.bare", RowMode::Judged, RowResult::Open, "—"),
        ]);
        let review = json!({
            "kind": "task-review",
            "rule_adherence": [
                {"id": "a.pass", "result": "pass", "evidence": "good"},
                {"id": "b.fail", "result": "fail", "evidence": "bad"},
                {"id": "c.warn", "result": "warn", "evidence": "so so"},
                {"id": "d.skipped", "result": "skipped", "evidence": "no time"},
                {"id": "e.other", "result": 7, "evidence": "odd"},
                {"id": "g.bare", "result": "pass"},
                {"id": "a.pass", "result": "fail", "evidence": "the first row of an id counts"},
            ],
        });
        let analysis = analysis();

        let built = build(
            &analysis,
            &coverage(&analysis),
            &result,
            Some(&review),
            provenance(Vec::new(), Vec::new()),
            Vec::new(),
            route_document(Vec::new()),
        );

        let judged = |id: &str, verdict: Option<Verdict>, evidence: Option<&str>| JudgedRow {
            id: id.to_string(),
            verdict,
            evidence: evidence.map(str::to_string),
        };
        assert_eq!(
            built.judged,
            [
                judged("a.pass", Some(Verdict::Pass), Some("good")),
                judged("b.fail", Some(Verdict::Fail), Some("bad")),
                judged("c.warn", Some(Verdict::Warn), Some("so so")),
                judged("d.skipped", Some(Verdict::Open), Some("no time")),
                judged("e.other", Some(Verdict::Open), Some("odd")),
                judged("f.none", None, None),
                judged("g.bare", Some(Verdict::Pass), None),
            ]
        );
    }

    // ---- the review file --------------------------------------------------------------

    /// The text of the error of reading `text` as a review file.
    fn review_error(text: &str) -> String {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("review.json");
        std::fs::write(&file, text).expect("write the review");
        let error = read_review(&file).expect_err("not a review deliverable");
        error
            .strip_prefix(&format!("{}: not a review deliverable: ", file.display()))
            .unwrap_or_else(|| panic!("{error}"))
            .to_string()
    }

    #[test]
    fn a_review_file_reads_for_each_review_kind() {
        let dir = tempfile::tempdir().expect("tempdir");
        for kind in ["task-review", "re-review", "branch-review"] {
            let file = dir.path().join("review.json");
            let review = json!({"kind": kind, "rule_adherence": []});
            std::fs::write(&file, review.to_string()).expect("write the review");

            assert_eq!(read_review(&file), Ok(review), "{kind}");
        }
    }

    #[test]
    fn a_review_file_names_what_makes_it_no_review_deliverable() {
        assert_eq!(
            review_error("{\"kind\": \"task-report\"}"),
            "kind must be \"task-review\", \"re-review\", or \"branch-review\""
        );
        assert_eq!(
            review_error("[]"),
            "kind must be \"task-review\", \"re-review\", or \"branch-review\""
        );
        assert_eq!(
            review_error("{\"kind\": \"task-review\"}"),
            "rule_adherence must be an array"
        );
        assert_eq!(
            review_error("{\"kind\": \"branch-review\", \"rule_adherence\": {}}"),
            "rule_adherence must be an array"
        );
        assert_eq!(
            review_error("{\"kind\": \"re-review\", \"rule_adherence\": [{\"id\": \"a\"}, 3]}"),
            "rule_adherence[1] must be an object with an id"
        );
        assert_eq!(
            review_error("{\"kind\": \"re-review\", \"rule_adherence\": [{\"id\": 4}]}"),
            "rule_adherence[0] must be an object with an id"
        );
        assert!(review_error("not json").starts_with("expected"));
    }

    // ---- the packet lines of the kinds ------------------------------------------------

    /// A path of `kind` with `ranges`.
    fn path_of(kind: ChangeKind, ranges: Vec<LineRange>) -> UncoveredPath {
        UncoveredPath {
            path: "p".to_string(),
            old_path: None,
            kind,
            ranges,
            empty: None,
        }
    }

    #[test]
    fn a_text_path_prints_its_lines_and_then_its_deletions() {
        let path = path_of(
            ChangeKind::Text,
            vec![
                LineRange::Lines { start: 2, end: 2 },
                LineRange::DeletedAfter {
                    after: 4,
                    deleted: 2,
                },
                LineRange::Lines { start: 5, end: 7 },
            ],
        );

        assert_eq!(
            packet_line(&path),
            "lines 2, 5-7; deleted after line 4: 2 lines"
        );
    }

    #[test]
    fn a_text_path_prints_each_deletion_only_hunk() {
        let path = path_of(
            ChangeKind::Text,
            vec![
                LineRange::DeletedAfter {
                    after: 0,
                    deleted: 1,
                },
                LineRange::DeletedAfter {
                    after: 9,
                    deleted: 3,
                },
            ],
        );

        assert_eq!(
            packet_line(&path),
            "deleted after line 0: 1 lines; deleted after line 9: 3 lines"
        );
    }

    #[test]
    fn a_renamed_text_path_names_its_old_path_after_its_lines() {
        let path = UncoveredPath {
            old_path: Some("old.rs".to_string()),
            ..path_of(
                ChangeKind::Text,
                vec![LineRange::Lines { start: 3, end: 4 }],
            )
        };

        assert_eq!(packet_line(&path), "lines 3-4; renamed from `old.rs`");
    }

    #[test]
    fn a_deleted_path_and_a_symlink_print_each_range_with_both_ends() {
        let deleted = path_of(
            ChangeKind::Deleted,
            vec![LineRange::Lines { start: 1, end: 2 }],
        );
        let one_line = path_of(
            ChangeKind::Deleted,
            vec![LineRange::Lines { start: 1, end: 1 }],
        );
        let link = path_of(
            ChangeKind::Symlink,
            vec![LineRange::Lines { start: 1, end: 1 }],
        );
        let retyped = path_of(
            ChangeKind::Symlink,
            vec![
                LineRange::DeletedAfter {
                    after: 0,
                    deleted: 4,
                },
                LineRange::Lines { start: 1, end: 1 },
            ],
        );

        assert_eq!(packet_line(&deleted), "deleted, lines 1-2 of the old file");
        assert_eq!(packet_line(&one_line), "deleted, lines 1-1 of the old file");
        assert_eq!(packet_line(&link), "symlink, lines 1-1");
        assert_eq!(
            packet_line(&retyped),
            "symlink, lines 1-1; deleted after line 0: 4 lines"
        );
    }

    #[test]
    fn a_renamed_path_names_its_old_path() {
        let path = UncoveredPath {
            old_path: Some("a `b`.rs".to_string()),
            ..path_of(ChangeKind::Renamed, Vec::new())
        };

        assert_eq!(packet_line(&path), "renamed from ``a `b`.rs``");
    }

    #[test]
    fn the_kinds_with_no_hunk_print_their_word() {
        let added = UncoveredPath {
            empty: Some(EmptyChange::Added),
            ..path_of(ChangeKind::Empty, Vec::new())
        };
        let deleted = UncoveredPath {
            empty: Some(EmptyChange::Deleted),
            ..path_of(ChangeKind::Empty, Vec::new())
        };

        assert_eq!(packet_line(&added), "empty file added");
        assert_eq!(packet_line(&deleted), "empty file deleted");
        assert_eq!(
            packet_line(&path_of(ChangeKind::Binary, Vec::new())),
            "binary"
        );
        assert_eq!(
            packet_line(&path_of(ChangeKind::Mode, Vec::new())),
            "mode only"
        );
        assert_eq!(
            packet_line(&path_of(ChangeKind::Submodule, Vec::new())),
            "submodule"
        );
    }

    #[test]
    fn a_kind_with_a_fact_that_the_document_lacks_prints_the_word_of_the_kind() {
        assert_eq!(
            packet_line(&path_of(ChangeKind::Empty, Vec::new())),
            "empty file"
        );
        assert_eq!(packet_line(&path_of(ChangeKind::Text, Vec::new())), "text");
        assert_eq!(
            packet_line(&path_of(ChangeKind::Deleted, Vec::new())),
            "deleted"
        );
        assert_eq!(
            packet_line(&path_of(ChangeKind::Symlink, Vec::new())),
            "symlink"
        );
        assert_eq!(
            packet_line(&path_of(ChangeKind::Renamed, Vec::new())),
            "renamed"
        );
    }

    // ---- the packet -------------------------------------------------------------------

    #[test]
    fn the_range_line_names_the_short_shas_the_hash_and_the_route() {
        let packet = markdown(&doc(), ".");

        assert!(
            packet.starts_with(
                "<!-- houserules:evidence . -->\n## houserules evidence\n\n\
                 Range 1111111..2222222 (merge base 3333333) · rules 0123456789ab · route owner-review\n\n"
            ),
            "{packet}"
        );
    }

    #[test]
    fn a_range_with_no_merge_base_prints_none() {
        let mut document = doc();
        document.merge_base = None;

        let packet = markdown(&document, ".");

        assert!(packet.contains("(merge base none)"), "{packet}");
    }

    #[test]
    fn the_marker_line_names_the_install_it_is_given() {
        assert!(
            markdown(&doc(), "docs/guide").starts_with("<!-- houserules:evidence docs/guide -->\n")
        );
    }

    #[test]
    fn the_counts_of_the_headings_come_from_the_lists() {
        let mut document = doc();
        document.uncovered.lines = 7;
        document.uncovered.total_lines = 21;
        document.intent = vec![titled("work-item", "HR-1", None)];
        document.failed = vec![rule_row("a", "x"), rule_row("b", "y")];
        document.proven = vec![rule_row("c", "z")];
        document.judged = vec![JudgedRow {
            id: "d".to_string(),
            verdict: None,
            evidence: None,
        }];
        document.skipped = vec![rule_row("e", "q"), rule_row("f", "r"), rule_row("g", "s")];

        let packet = markdown(&document, ".");

        for heading in [
            "### Uncovered surface (7 of 21 changed lines)",
            "### Intent (1)",
            "### Failed (2)",
            "### Warned (0)",
            "### Proven (1)",
            "### Judged only (1)",
        ] {
            assert!(
                packet.lines().any(|line| line == heading),
                "{heading}\n{packet}"
            );
        }
        assert!(packet.contains("\n3 deterministic checks did not apply to this change.\n"));
    }

    #[test]
    fn the_size_section_names_the_diff_the_excluded_files_and_the_work_items() {
        let mut document = doc();
        document.diff = Diff {
            files: 3,
            added: 10,
            deleted: 4,
            lines: 14,
            excluded: 0,
        };
        document.work_items = vec!["HR-1".to_string(), "A `b`".to_string()];

        let plain = markdown(&document, ".");
        document.diff.excluded = 2;
        let excluded = markdown(&document, ".");

        assert_eq!(
            section(&plain, "### Size and links"),
            [
                "",
                "Diff: +10 -4 lines in 3 files",
                "Work items: `HR-1`, `` A `b` ``",
                "Sessions: none"
            ]
        );
        assert_eq!(
            section(&excluded, "### Size and links")[1],
            "Diff: +10 -4 lines in 3 files; 2 files excluded by review.diffExclude"
        );
        assert_eq!(
            section(&markdown(&doc(), "."), "### Size and links")[2],
            "Work items: none"
        );
    }

    #[test]
    fn an_id_of_the_work_items_line_is_cut_to_100_characters() {
        let mut document = doc();
        document.work_items = vec!["x".repeat(150)];

        let packet = markdown(&document, ".");

        assert_eq!(
            section(&packet, "### Size and links")[2],
            format!("Work items: `{}`", "x".repeat(100))
        );
    }

    #[test]
    fn the_route_section_names_the_route_and_the_reasons_at_that_route() {
        let mut document = doc();
        document.route = route_document(vec![failed_reason("team.a"), failed_reason("team.b")]);

        let packet = markdown(&document, ".");

        assert_eq!(
            section(&packet, "### Route"),
            [
                "",
                "owner-review",
                "",
                "- failed: `team.a` -> owner-review",
                "- failed: `team.b` -> owner-review"
            ]
        );
    }

    #[test]
    fn a_rule_row_prints_its_id_and_its_evidence_in_code_spans() {
        let mut document = doc();
        document.failed = vec![
            rule_row("team.a", "x `y` <!-- z"),
            rule_row("team.b", "line\nbreak"),
        ];

        let packet = markdown(&document, ".");

        assert_eq!(
            section(&packet, "### Failed (2)"),
            [
                "",
                "- `team.a`: ``x `y` <!-- z``",
                "- `team.b`: `\"line\\nbreak\"`"
            ]
        );
    }

    #[test]
    fn an_evidence_text_over_300_characters_keeps_300_and_gets_dots() {
        let mut document = doc();
        document.proven = vec![
            rule_row("exact", &"e".repeat(300)),
            rule_row("long", &"l".repeat(301)),
        ];

        let packet = markdown(&document, ".");

        assert_eq!(
            section(&packet, "### Proven (2)"),
            [
                "".to_string(),
                format!("- `exact`: `{}`", "e".repeat(300)),
                format!("- `long`: `{}...`", "l".repeat(300)),
            ]
        );
    }

    #[test]
    fn an_intent_block_prints_each_field_that_is_not_null() {
        let mut document = doc();
        document.intent = vec![
            Intent {
                source: "openspec".to_string(),
                reference: "add-x".to_string(),
                title: Some("Add x".to_string()),
                why: Some("Because x.\nAnd y `z`.".to_string()),
                tasks: Some(Tasks { done: 2, total: 4 }),
                requirements: Some(Requirements {
                    added: Some(2),
                    modified: None,
                    removed: Some(0),
                    renamed: Some(1),
                    total: None,
                }),
                validation: Some(Validation {
                    tool: "openspec".to_string(),
                    ok: false,
                    summary: "0 of 1 valid".to_string(),
                }),
                artifacts: vec!["openspec/changes/add-x/proposal.md".to_string()],
                unresolved: None,
            },
            titled("work-item", "HR-1", None),
        ];

        let packet = markdown(&document, ".");

        assert_eq!(
            section(&packet, "### Intent (2)"),
            [
                "",
                "`openspec add-x`: `Add x`",
                "Why:",
                "```",
                "Because x.",
                "And y `z`.",
                "```",
                "Tasks: 2 of 4 done",
                "Requirements: +2 ~? -0 renamed 1 (total ?)",
                "Validation: `openspec` failed: `0 of 1 valid`",
                "",
                "`work-item HR-1`: no title"
            ]
        );
    }

    #[test]
    fn an_unresolved_intent_prints_the_line_of_its_adapter_and_no_other_line() {
        let mut document = doc();
        document.intent = vec![
            Intent {
                unresolved: Some("the tracker holds no such work item".to_string()),
                ..titled("work-item", "HR-404", None)
            },
            Intent {
                unresolved: Some("a line with `ticks` and <!-- a comment -->".to_string()),
                ..titled("work-item", "HR-405", None)
            },
        ];

        let packet = markdown(&document, ".");

        assert_eq!(
            section(&packet, "### Intent (2)"),
            [
                "",
                "`work-item HR-404`: the tracker holds no such work item",
                "",
                "`work-item HR-405`: a line with `ticks` and <!-- a comment -->"
            ],
            "the line is the binary's own text: it prints as it is"
        );
    }

    #[test]
    fn a_validation_that_passed_prints_ok() {
        let mut document = doc();
        document.intent = vec![Intent {
            validation: Some(Validation {
                tool: "t".to_string(),
                ok: true,
                summary: "fine".to_string(),
            }),
            ..titled("file", "p.md", Some("P"))
        }];

        let packet = markdown(&document, ".");

        assert_eq!(
            section(&packet, "### Intent (1)")[2],
            "Validation: `t` ok: `fine`"
        );
    }

    // ---- the cap ----------------------------------------------------------------------

    /// The number of characters of `packet`.
    fn size(packet: &str) -> usize {
        packet.chars().count()
    }

    /// The items kept and the count of the rest in the list of the section that
    /// starts with `heading`: the lines that start with `- ` and are no count
    /// line, and the number in the count line (0 without one).
    fn kept(packet: &str, heading: &str) -> (usize, usize) {
        let lines = section(packet, heading);
        let rest = lines
            .iter()
            .find_map(|line| {
                line.strip_prefix("- ... and ")
                    .and_then(|rest| rest.strip_suffix(" more"))
            })
            .map_or(0, |count| count.parse().expect("a count"));
        let items = lines
            .iter()
            .filter(|line| line.starts_with("- ") && !line.starts_with("- ... and "))
            .count();
        (items, rest)
    }

    /// A document with `count` uncovered paths, `count` proven rows, and
    /// `count` failed rows, each item about `width` characters long.
    fn big(count: usize, width: usize) -> Evidence {
        let mut document = doc();
        document.uncovered.lines = count as u64;
        document.uncovered.total_lines = count as u64;
        document.uncovered.paths = (0..count)
            .map(|number| uncovered_path(&format!("{number:0>width$}"), 1))
            .collect();
        document.proven = (0..count)
            .map(|number| rule_row(&format!("proven.{number}"), &"p".repeat(width)))
            .collect();
        document.failed = (0..count)
            .map(|number| rule_row(&format!("failed.{number}"), &"f".repeat(width)))
            .collect();
        document
    }

    #[test]
    fn a_packet_under_the_cap_is_not_cut() {
        let packet = markdown(&big(50, 100), ".");

        assert!(size(&packet) < LIMIT);
        assert!(!packet.contains("... and "), "{packet}");
    }

    #[test]
    fn the_uncovered_list_is_cut_first_to_the_items_that_fit() {
        let mut document = doc();
        document.uncovered.lines = 3000;
        document.uncovered.total_lines = 3000;
        document.uncovered.paths = (0..3000)
            .map(|number| uncovered_path(&format!("misc/f{number:04}.txt"), 1))
            .collect();

        let packet = markdown(&document, ".");

        let (items, rest) = kept(
            &packet,
            "### Uncovered surface (3000 of 3000 changed lines)",
        );
        assert_eq!(items + rest, 3000);
        assert!(items > 0 && rest > 0);
        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        let next = format!("- `misc/f{items:04}.txt`: lines 1\n");
        assert!(
            LIMIT - size(&packet) < next.len(),
            "one more item would fit: {} left, {} needed",
            LIMIT - size(&packet),
            next.len()
        );
        assert!(packet.contains(&format!("- ... and {rest} more\n")));
    }

    /// `count` rows with the ids `<prefix>.<n>` and `width` characters of
    /// evidence.
    fn rows(prefix: &str, count: usize, width: usize) -> Vec<RuleRow> {
        (0..count)
            .map(|number| rule_row(&format!("{prefix}.{number}"), &"e".repeat(width)))
            .collect()
    }

    /// `count` judged rows with a verdict and `width` characters of evidence.
    fn judged_rows(count: usize, width: usize) -> Vec<JudgedRow> {
        (0..count)
            .map(|number| JudgedRow {
                id: format!("judged.{number}"),
                verdict: Some(Verdict::Pass),
                evidence: Some("j".repeat(width)),
            })
            .collect()
    }

    #[test]
    fn the_proven_list_is_cut_after_the_uncovered_list_and_the_later_lists_stay_whole() {
        // The uncovered list and the proven list are about 110,000 characters
        // each; the others are small. Emptying the uncovered list is not enough.
        let mut document = big(400, 250);
        document.warned = rows("warned", 40, 250);
        document.failed = rows("failed", 40, 250);
        document.judged = judged_rows(40, 250);

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        let uncovered = kept(&packet, "### Uncovered surface (400 of 400 changed lines)");
        let proven = kept(&packet, "### Proven (400)");
        assert_eq!(uncovered, (0, 400), "the uncovered list goes first");
        assert_eq!(proven.0 + proven.1, 400);
        assert!(
            proven.0 > 0 && proven.1 > 0,
            "the proven list is cut to the items that fit"
        );
        assert_eq!(kept(&packet, "### Judged only (40)"), (40, 0));
        assert_eq!(kept(&packet, "### Warned (40)"), (40, 0));
        assert_eq!(kept(&packet, "### Failed (40)"), (40, 0));
    }

    #[test]
    fn the_judged_list_follows_the_proven_list() {
        let mut document = doc();
        document.proven = rows("proven", 300, 250);
        document.judged = judged_rows(300, 250);
        document.warned = rows("warned", 40, 250);
        document.failed = rows("failed", 40, 250);

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        assert_eq!(kept(&packet, "### Proven (300)"), (0, 300));
        let judged = kept(&packet, "### Judged only (300)");
        assert_eq!(judged.0 + judged.1, 300);
        assert!(
            judged.0 > 0 && judged.1 > 0,
            "the judged list is cut to the items that fit"
        );
        assert_eq!(kept(&packet, "### Warned (40)"), (40, 0));
        assert_eq!(kept(&packet, "### Failed (40)"), (40, 0));
    }

    #[test]
    fn the_warned_list_follows_the_judged_list() {
        let mut document = doc();
        document.judged = judged_rows(300, 250);
        document.warned = rows("warned", 300, 250);
        document.failed = rows("failed", 40, 250);

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        assert_eq!(kept(&packet, "### Judged only (300)"), (0, 300));
        let warned = kept(&packet, "### Warned (300)");
        assert_eq!(warned.0 + warned.1, 300);
        assert!(warned.0 > 0 && warned.1 > 0);
        assert_eq!(kept(&packet, "### Failed (40)"), (40, 0));
    }

    #[test]
    fn the_failed_list_follows_the_warned_list() {
        let mut document = doc();
        document.warned = rows("warned", 300, 250);
        document.failed = rows("failed", 300, 250);

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        assert_eq!(kept(&packet, "### Warned (300)"), (0, 300));
        let failed = kept(&packet, "### Failed (300)");
        assert_eq!(failed.0 + failed.1, 300);
        assert!(failed.0 > 0 && failed.1 > 0);
    }

    #[test]
    fn the_route_reason_lines_are_cut_after_the_failed_list() {
        let mut document = doc();
        document.failed = (0..300)
            .map(|number| rule_row(&format!("failed.{number}"), &"f".repeat(250)))
            .collect();
        document.route = route_document(
            (0..2000)
                .map(|number| failed_reason(&format!("rule.{number:0>40}")))
                .collect(),
        );

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        let failed = kept(&packet, "### Failed (300)");
        let route = kept(&packet, "### Route");
        assert_eq!(failed, (0, 300), "the failed list is emptied first");
        assert_eq!(route.0 + route.1, 2000);
        assert!(
            route.0 > 0 && route.1 > 0,
            "the reason lines are cut to the lines that fit"
        );
    }

    #[test]
    fn the_work_items_are_cut_to_50_only_when_every_earlier_list_is_empty_and_the_packet_is_still_over()
     {
        let mut document = big(400, 250);
        document.work_items = (0..3000)
            .map(|number| format!("WORK-{number:030}"))
            .collect();

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        let listed: Vec<String> = (0..50)
            .map(|number| format!("`WORK-{number:030}`"))
            .collect();
        assert_eq!(
            section(&packet, "### Size and links")[2],
            format!("Work items: {}, ... and 2950 more", listed.join(", "))
        );
        assert_eq!(kept(&packet, "### Failed (400)"), (0, 400));
    }

    #[test]
    fn the_work_items_stay_whole_while_the_cut_of_an_earlier_list_makes_the_packet_fit() {
        let mut document = big(400, 250);
        document.work_items = (0..3000).map(|number| format!("W-{number:04}")).collect();

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        let work_items = section(&packet, "### Size and links")[2];
        assert!(!work_items.contains("... and"), "{work_items}");
        assert!(work_items.contains("`W-2999`"));
    }

    /// An intent with a why of `why_len` characters.
    fn intent_with_why(number: usize, why_len: usize) -> Intent {
        Intent {
            why: Some("y".repeat(why_len)),
            ..titled(
                "file",
                &format!("docs/plans/{number:04}.md"),
                Some("A plan"),
            )
        }
    }

    #[test]
    fn the_intent_blocks_are_cut_last_and_each_why_is_cut_to_300_characters_first() {
        let mut document = doc();
        // 70 blocks of 1,000 characters are about 71,000 characters: the cut of
        // the whys to 300 characters is enough.
        document.intent = (0..70)
            .map(|number| intent_with_why(number, 1000))
            .collect();

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        let block = section(&packet, "### Intent (70)");
        assert!(
            block.iter().all(|line| !line.contains("... and ")),
            "no block is cut"
        );
        let whys: Vec<&&str> = block.iter().filter(|line| line.starts_with('y')).collect();
        assert_eq!(whys.len(), 70);
        assert!(whys.iter().all(|line| line.chars().count() == 300));
    }

    #[test]
    fn the_intent_blocks_are_cut_to_the_blocks_that_fit_when_the_cut_whys_are_not_enough() {
        let mut document = doc();
        document.intent = (0..1000)
            .map(|number| intent_with_why(number, 1000))
            .collect();

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT, "{} characters", size(&packet));
        let block = section(&packet, "### Intent (1000)");
        let blocks = block
            .iter()
            .filter(|line| line.starts_with("`file docs/plans/"))
            .count();
        let rest: usize = block
            .iter()
            .find_map(|line| {
                line.strip_prefix("... and ")
                    .and_then(|rest| rest.strip_suffix(" more"))
            })
            .expect("the count of the rest")
            .parse()
            .expect("a count");
        assert_eq!(blocks + rest, 1000);
        assert!(blocks > 0 && rest > 0);
    }

    #[test]
    fn the_cut_leaves_the_fixed_lines_and_every_heading() {
        let packet = markdown(&big(2000, 400), ".");

        assert!(size(&packet) <= LIMIT);
        for heading in [
            "## houserules evidence",
            "### Uncovered surface (2000 of 2000 changed lines)",
            "### Intent (0)",
            "### Failed (2000)",
            "### Warned (0)",
            "### Proven (2000)",
            "### Judged only (0)",
            "### Size and links",
            "### Route",
        ] {
            assert!(packet.lines().any(|line| line == heading), "{heading}");
        }
        assert!(packet.ends_with('\n'));
    }

    #[test]
    fn a_cut_packet_counts_characters_and_not_bytes() {
        let mut document = doc();
        document.uncovered.lines = 4000;
        document.uncovered.total_lines = 4000;
        document.uncovered.paths = (0..4000)
            .map(|number| uncovered_path(&format!("é/ü{number:04}.txt"), 1))
            .collect();

        let packet = markdown(&document, ".");

        assert!(size(&packet) <= LIMIT);
        assert!(
            packet.len() > LIMIT,
            "the packet holds multi-byte characters"
        );
    }

    #[test]
    fn the_document_serializes_its_keys_in_the_order_of_the_spec() {
        let text = serde_json::to_string(&doc()).expect("serialize");
        let value: Value = serde_json::from_str(&text).expect("parse");

        let keys: Vec<&String> = value.as_object().expect("an object").keys().collect();

        assert_eq!(
            keys,
            [
                "schema",
                "install",
                "base",
                "head",
                "merge_base",
                "intent",
                "rules",
                "diff",
                "uncovered",
                "failed",
                "warned",
                "proven",
                "judged",
                "skipped",
                "work_items",
                "sessions",
                "route"
            ]
        );
    }

    #[test]
    fn the_sessions_line_lists_20_sessions_cuts_each_value_and_counts_the_rest() {
        let many: Vec<Session> = (0..23)
            .map(|number| session("a", "Entire-Checkpoint", &format!("cp-{number}")))
            .collect();

        assert_eq!(sessions_line(&[], 0), "Sessions: none");
        assert_eq!(
            sessions_line(&many[..2], 0),
            "Sessions: aaaaaaa Entire-Checkpoint: `cp-0`, aaaaaaa Entire-Checkpoint: `cp-1`"
        );
        let listed = sessions_line(&many, 0);
        assert!(listed.ends_with(", ... and 3 more"), "{listed}");
        assert_eq!(listed.matches("Entire-Checkpoint").count(), 20);
        assert!(
            sessions_line(&many[..20], 4).ends_with(", ... and 4 more"),
            "a record that a cut shortened adds its own count"
        );
        assert!(
            sessions_line(&many, 4).ends_with(", ... and 7 more"),
            "the sessions past 20 and the sessions that a cut left out"
        );
        let long = session("b", "Session-Id", &"v".repeat(150));
        assert_eq!(
            sessions_line(&[long], 0),
            format!("Sessions: bbbbbbb Session-Id: `{}`", "v".repeat(100)),
            "a value is cut to 100 characters"
        );
        let hostile = session("c", "Session-Id", "a``b\nc");
        assert_eq!(
            sessions_line(&[hostile], 0),
            "Sessions: ccccccc Session-Id: ```\"a``b\\nc\"```",
            "a value with a control character prints as a JSON string in a span"
        );
    }
}
