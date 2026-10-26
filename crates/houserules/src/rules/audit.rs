//! The `audit` command: builds the rule package for a git range, runs
//! every member's deterministic check through `runner::run`, and reports
//! the result. The git helpers it runs on live in `git.rs`; the check types
//! live in `runner/`.
//!
//! ## Data-layer decisions
//!
//! - Each entry's `check` object is read through `check_shape::CheckDef`
//!   (`model::Entry.check`, typed `CheckField`), not raw `serde_json::Value`
//!   field access: it is a fixed, closed schema (`knowledge/schema.json`'s
//!   `$defs/check`), and `runner::run` matches on its `type`/`level`/`scope`
//!   for every branch, exactly the "match on a type instead of a Value"
//!   case the rule favors. See `model::CheckField`'s own doc for how a
//!   malformed `check` is told apart from an absent one: this file's own
//!   row-building match (below) reports a `Malformed` check as a named,
//!   exit-2 error, never a judged row.
//! - `audit`'s own JSON output is the typed
//!   `published::audit_result::AuditResult` (`{schema, base, head, ids,
//!   changed_files, areas, area_files, rules, summary, stale_sanctions,
//!   references}`; `stale_sanctions` only when a booking is stale), which
//!   `gen-schemas` turns into the published `audit-result` schema. It is its
//!   own type, apart from any deliverables-schema type, for a *shape*
//!   reason: `audit`'s judged rows carry `result: "open"` — a value
//!   `.claude/schemas/deliverables.json`'s `auditRow.result` enum
//!   (`pass`/`fail`/`warn`/`skipped`) explicitly forbids. A report author
//!   copies only this command's *deterministic* rows into a report's
//!   `self_audit` (the retrieval protocol's own words: "never hand-written
//!   rows; the judged rows are the reviewer's") -- the `open` rows never
//!   reach a schema-validated file at all. Reusing a schema-pinned
//!   deliverables row type here would either make it accept a value the
//!   schema itself forbids (defeating its whole purpose as a schema-exact
//!   pin) or require a second, audit-only row representation alongside it.
//! - This crate carries no typed deliverables model: `validate` (see
//!   `validate_deliverable.rs`'s module doc) and `stats` both aggregate
//!   through tolerant `Value` reads, so no command constructs or strictly
//!   parses a schema-exact deliverable anywhere in this binary. The
//!   `self_audit` copy of a report is such a tolerant `Value` read too:
//!   `runner::compare_self_audit` compares it with the JSON of the typed
//!   result. `check_shape.rs`'s `CheckDef` is the model layer of the checks
//!   (`runner::run` matches on it).
//!
//! ## The rule package
//!
//! The package is every standing entry, every rule-kind entry (or entry
//! with a check) whose area the range touches, and every entry `--ids`
//! names. A task audit (`--report`) leaves out an entry tagged
//! `controller`: only the controller's acts (specs, plans, dispatch,
//! rulings, merges, releases) can break it, so a task diff never does.
//! `--ids` brings such an entry back. The branch audit (`--workspace`)
//! and a plain audit keep it.
//!
//! ## The crash-path extension this file adds
//!
//! A malformed check `pattern`/`subject`/`body_absent` regex, or a
//! malformed `files`/`if`/`then` glob, reaching `runner::run` would panic
//! if left unguarded. `check-knowledge` (`check.rs`'s
//! `regex_validity_message`) already validates a check's
//! `pattern`/`subject`/`body_absent` regex fields eagerly at load time,
//! the same way `model::load_base` eagerly compiles `areas.json`'s globs.
//! It does NOT validate a check's `files`/`if`/`then` glob fields the
//! same way, so a malformed check glob can reach `audit` on a base that
//! already passes `check-knowledge` cleanly. `audit` is therefore the
//! first and only place a malformed check *glob* is diagnosed, and this
//! binary reports it as a `Result` propagated all the way out to
//! `cmd_audit`, which prints the standard one-line, exit-2 CLI failure
//! (`houserules.crash-paths-are-named`) -- reached, named, and exited,
//! never a panic and never silently swallowed either.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

use crate::emit::emit;
use crate::published::audit_result::{
    AuditResult, AuditRow, References, RowMode, RowResult, Summary,
};
use crate::tracker::Lookup;

use super::deliverable::{read_deliverable_value, workspace_files};
use super::git::{changed_files, is_empty_tree, rev};
use super::git_paths::InstallPrefix;
use super::glob::area_files;
use super::model::{Base, Entry, load_base, raw_entry_has_tag};
use super::render::RULE_KINDS;
use super::runner::{self, AuditContext};
use super::trailers::{self, Reference};

// ---- the audit engine -----------------------------------------------------------------

/// `audit`'s inputs (see this module's own "sanctioned-fail annotation"
/// section below for `sanctioned`).
#[derive(Default)]
pub(crate) struct AuditOptions {
    pub base_ref: Option<String>,
    pub head_ref: Option<String>,
    pub ids: Vec<String>,
    pub report: Option<PathBuf>,
    pub workspace: Option<PathBuf>,
    /// Each `--sanctioned <rule>=<ref>` as a `(rule id, reference)` pair,
    /// in the order given; a rule id may repeat (unusual, not rejected).
    pub sanctioned: Vec<(String, String)>,
}

/// `audit`'s result: the typed result without the verdict on the work-item
/// references (`references` holds no reference and `summary.reference_fail` is
/// zero until `cmd_audit` judges them), whether any row's `result` is `fail`,
/// and the work-item references of the range and of the deliverables it read,
/// which `cmd_audit` judges through the installed tracker and adds to the
/// result (spec 3.7). A `--report` run also hands `cmd_audit` the parsed
/// report and the ids of the rows that found a filled `self_audit`.
/// `cmd_audit` compares the copy with the final result (spec 6) and judges
/// again the bookings of the rows that the comparison moves.
#[derive(Debug)]
pub(crate) struct AuditOutcome {
    pub result: AuditResult,
    pub failed: bool,
    pub(super) references: Vec<Reference>,
    pub(super) report: Option<Value>,
    pub(super) self_audit_rows: Vec<String>,
}

/// `true` when `row`'s `mode` is `deterministic`.
fn is_deterministic(row: &AuditRow) -> bool {
    row.mode == RowMode::Deterministic
}

/// The row of a rule that has no check: a reviewer judges it, and its result
/// stays `open`.
fn judged_row(entry: &Entry) -> AuditRow {
    AuditRow {
        id: entry.id.clone(),
        kind: entry.kind.clone(),
        mode: RowMode::Judged,
        level: None,
        result: RowResult::Open,
        evidence: "\u{2014}".to_string(),
        findings: None,
        sanctioned: None,
    }
}

/// The tag of an entry only the controller's acts can break.
const CONTROLLER_TAG: &str = "controller";

/// `true` when the raw entry `id` carries the tag `controller`.
fn is_controller_entry(base: &Base, id: &str) -> bool {
    raw_entry_has_tag(base, id, CONTROLLER_TAG)
}

/// The named error for an id that `--ids` or `--sanctioned` names while the
/// base holds no active entry for it. A pack entry with an inactive status
/// (`retired` or `superseded`) and an excluded pack entry are in the pack
/// copy but out of the base, and each message names its cause, led by
/// `lead`. `unknown` is the message for an id that no source ships.
fn inactive_id_error(base: &Base, id: &str, lead: &str, unknown: String) -> String {
    if base.inactive.contains_key(id) {
        format!(
            "{lead} \"{id}\" is a pack entry with status `retired` or `superseded`, \
             which audit leaves out"
        )
    } else if base.excluded.contains_key(id) {
        format!("{lead} \"{id}\" is excluded in knowledge/overrides.json, which audit leaves out")
    } else {
        unknown
    }
}

/// The named error for a `--sanctioned` rule that is no row of the
/// package. An id with no active entry is `inactive_id_error`, which keeps
/// `unknown id` for an id that no source ships, as for `--ids`. A known id
/// is outside this audit's package, and `--ids` brings it in; the message
/// says so, and for an entry tagged `controller` in a task audit it names
/// the tag.
fn sanctioned_outside_package_error(base: &Base, rule: &str, is_task_audit: bool) -> String {
    if !base.entries.contains_key(rule) {
        inactive_id_error(
            base,
            rule,
            "--sanctioned",
            format!("unknown id \"{rule}\" for --sanctioned"),
        )
    } else if is_task_audit && is_controller_entry(base, rule) {
        format!(
            "--sanctioned \"{rule}\" is tagged controller, so a task audit leaves it out of \
             its package; add it to --ids"
        )
    } else {
        format!("--sanctioned \"{rule}\" is not in this audit's package; add it to --ids")
    }
}

/// Builds the rule package for a git range and runs every member's
/// deterministic check. A task audit (`opts.report` is set) leaves out an
/// entry tagged `controller` unless `opts.ids` names it. See this module's
/// doc for why the result type is not a deliverables type.
pub(crate) fn audit(base: &Base, opts: AuditOptions) -> Result<AuditOutcome, String> {
    let Some(base_ref) = &opts.base_ref else {
        return Err("audit needs --base <ref>".to_string());
    };
    if opts.report.is_some() && opts.workspace.is_some() {
        return Err("audit takes --report or --workspace, not both".to_string());
    }
    let base_sha = rev(&base.root, base_ref)?;
    audit_range(base, opts, base_sha)
}

/// Like [`audit`], for a range whose start the caller resolved already:
/// `base_sha` is the abbreviated sha of a commit, or the full object name of
/// git's empty tree, which starts the whole history of the head.
fn audit_range(base: &Base, opts: AuditOptions, base_sha: String) -> Result<AuditOutcome, String> {
    let root = &base.root;
    let head_sha = rev(root, opts.head_ref.as_deref().unwrap_or("HEAD"))?;
    let prefix = InstallPrefix::of(root)?;
    let changed = changed_files(root, &prefix, &base_sha, &head_sha)?;
    let changed_refs: Vec<&str> = changed.iter().map(String::as_str).collect();
    let area_file_map =
        area_files(&changed_refs, &base.areas).map_err(|error| error.to_string())?;
    let mut areas: Vec<String> = area_file_map.keys().cloned().collect();
    areas.sort();

    let is_task_audit = opts.report.is_some();
    let mut package: HashMap<String, &Entry> = HashMap::new();
    for entry in base.entries.values() {
        if is_task_audit && is_controller_entry(base, &entry.id) {
            continue;
        }
        let in_touched_area = areas.contains(&entry.area);
        // `!matches!(entry.check, CheckField::Absent)` joins the package
        // whether or not the check is valid: a `Malformed` check still
        // needs to reach row-building below to be named, not be silently
        // excluded here instead.
        let has_check = !matches!(entry.check, super::model::CheckField::Absent);
        if entry.standing
            || (RULE_KINDS.contains(&entry.kind.as_str()) && in_touched_area)
            || (has_check && in_touched_area)
        {
            package.insert(entry.id.clone(), entry);
        }
    }
    for id in &opts.ids {
        let entry = base
            .entries
            .get(id)
            .ok_or_else(|| inactive_id_error(base, id, "id", format!("unknown id \"{id}\"")))?;
        package.insert(id.clone(), entry);
    }

    let report_value = match &opts.report {
        Some(path) => Some(read_deliverable_value(path)?),
        None => None,
    };
    let mut reports_value: Option<Vec<(String, Value)>> = None;
    let mut reviews_value: Vec<(String, Value)> = Vec::new();
    if let Some(dir) = &opts.workspace {
        let files = workspace_files(dir)?;
        let read = |names: Vec<String>| -> Result<Vec<(String, Value)>, String> {
            let mut deliverables = Vec::with_capacity(names.len());
            for name in names {
                let data = read_deliverable_value(&dir.join(&name))?;
                deliverables.push((name, data));
            }
            Ok(deliverables)
        };
        reports_value = Some(read(files.reports)?);
        reviews_value = read(files.reviews)?;
    }

    let ctx = AuditContext::new(
        root,
        &prefix,
        base_sha.clone(),
        head_sha.clone(),
        &changed,
        &opts.ids,
        report_value.as_ref(),
        reports_value.as_deref(),
    );

    let mut ids: Vec<&String> = package.keys().collect();
    ids.sort();
    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        let entry = package[id];
        let row = match &entry.check {
            super::model::CheckField::Valid(check) => runner::run(entry, check, &ctx)?,
            // A malformed check is a named error, never a silently
            // downgraded judged row (`houserules.crash-paths-are-named`).
            super::model::CheckField::Malformed => {
                return Err(format!("{id}: malformed check"));
            }
            super::model::CheckField::Absent => judged_row(entry),
        };
        rows.push(row);
    }

    // ---- the sanctioned annotation ----
    //
    // A `--sanctioned <rule>=<ref>` declares a spec-booked interim fail once
    // per dispatch, instead of narrating it by hand across every audit and
    // review round that the interim fail spans. `book_sanction` judges each
    // booking against the TRUE result of its row: a fail stays "fail", and
    // never silently becomes "pass". `parse_sanctioned` already ruled out an
    // empty reference and a repeated rule id, so every remaining case is a row
    // or a named error. A rule that is no row of the package is
    // `sanctioned_outside_package_error` (an id the knowledge base lacks is the
    // same "unknown id" shape that `--ids` uses; a known id outside the
    // package names that cause). A deterministic row that did not fail is a
    // stale booking, reported in `stale_sanctions` and folded into `failed`,
    // never a silent no-op: reading "did not fail" for a rule that never ran
    // would send an author to un-book a typo instead of fixing it. The
    // comparison of `self_audit` can still move the row of its own check after
    // this loop; `cmd_audit` re-judges those bookings (`rejudge_moved_bookings`).
    let mut stale_sanctions: Vec<String> = Vec::new();
    let mut sanctioned_fail = 0u64;
    for (rule, reference) in &opts.sanctioned {
        let Some(row) = rows.iter_mut().find(|row| row.id == *rule) else {
            return Err(sanctioned_outside_package_error(base, rule, is_task_audit));
        };
        match book_sanction(row, rule, reference) {
            Booking::Judged => {}
            Booking::Failed => sanctioned_fail += 1,
            Booking::Stale(line) => stale_sanctions.push(line),
        }
    }

    let deterministic = rows.iter().filter(|r| is_deterministic(r)).count() as u64;
    let count_where = |result: RowResult| {
        rows.iter()
            .filter(|r| is_deterministic(r) && r.result == result)
            .count() as u64
    };
    let pass = count_where(RowResult::Pass);
    let fail = count_where(RowResult::Fail);
    let warn = count_where(RowResult::Warn);
    let skipped = count_where(RowResult::Skipped);
    let judged = rows.len() as u64 - deterministic;

    let commits = ctx.commits()?;
    let mut deliverables: Vec<(String, &Value)> = Vec::new();
    if let (Some(path), Some(report)) = (&opts.report, &report_value) {
        deliverables.push((path.display().to_string(), report));
    }
    deliverables.extend(
        reports_value
            .iter()
            .flatten()
            .chain(&reviews_value)
            .map(|(name, data)| (name.clone(), data)),
    );
    let references = trailers::collect(&commits, &deliverables)?;
    let empty_range = commits.is_empty();
    if empty_range {
        for row in rows.iter_mut().filter(|r| is_deterministic(r)) {
            row.evidence = format!("empty range: {}", row.evidence);
        }
    }

    // `empty_range` and `sanctioned_fail` are optional keys: the summary omits
    // them when they are `false` or 0, so the summary of a non-empty range
    // without `--sanctioned` holds only the counts.
    let summary = Summary {
        base: base_sha.clone(),
        head: head_sha.clone(),
        deterministic,
        pass,
        fail,
        warn,
        skipped,
        judged,
        empty_range: empty_range.then_some(true),
        sanctioned_fail: (sanctioned_fail > 0).then_some(sanctioned_fail),
        reference_fail: 0,
    };

    // A sanctioned fail still counts in `fail` above (its `result` never
    // changed) and still fails the run; a stale booking fails the run too,
    // even though every real row passed, since a stale booking is a
    // finding, not a no-op.
    let failed =
        rows.iter().any(|row| row.result == RowResult::Fail) || !stale_sanctions.is_empty();

    let self_audit_rows = ctx.self_audit_rows();
    let result = AuditResult {
        schema: crate::published::id::<AuditResult>(),
        base: base_sha,
        head: head_sha,
        ids: opts.ids,
        changed_files: changed,
        areas,
        area_files: area_file_map,
        rules: rows,
        summary,
        stale_sanctions: (!stale_sanctions.is_empty()).then_some(stale_sanctions),
        references: References::default(),
    };
    Ok(AuditOutcome {
        result,
        failed,
        references,
        report: report_value,
        self_audit_rows,
    })
}

/// The result of the audit of the range `base_ref` to `head_ref` as `audit
/// --json` prints it for those two options and no other: the rows of
/// [`audit`] and the verdict on the work-item references of the range, which
/// `cmd_audit` adds. A tracker that cannot be built is the `Err`; one that
/// cannot be reached shows in the `references` of the result, as it does for
/// `audit`.
///
/// A `base_ref` that is the name of git's empty tree starts the whole history
/// of the head, which the in-process audit of the review commands needs for the
/// first push to a repository: the audit then reads every path of the head and
/// every commit that the head reaches. The command `audit` still takes only a
/// commit for `--base`.
pub(crate) fn audit_result(
    base: &Base,
    base_ref: &str,
    head_ref: &str,
) -> Result<AuditResult, String> {
    let tracker = crate::tracker::lookup(&base.root).map_err(|error| error.to_string())?;
    let opts = AuditOptions {
        base_ref: Some(base_ref.to_string()),
        head_ref: Some(head_ref.to_string()),
        ..AuditOptions::default()
    };
    let mut outcome = if is_empty_tree(base_ref) {
        audit_range(base, opts, base_ref.to_string())?
    } else {
        audit(base, opts)?
    };
    judge_references(&tracker, &mut outcome);
    Ok(outcome.result)
}

/// What one `--sanctioned` booking made of its row.
enum Booking {
    /// The row is judged: it carries `sanctioned`, and the booking is never
    /// stale.
    Judged,
    /// The row is a deterministic `fail`: it carries `sanctioned` and the
    /// evidence suffix.
    Failed,
    /// The row is deterministic and did not fail: the row stays as it is, and
    /// the text is the booking's line of `stale_sanctions`.
    Stale(String),
}

/// The start of the `stale_sanctions` line of the booking `rule=reference`.
fn stale_line_start(rule: &str, reference: &str) -> String {
    format!("--sanctioned {rule}={reference}: ")
}

/// Books `reference` on `row`, the row of `rule`, as the result of the row
/// allows: a judged row and a deterministic `fail` carry `sanctioned`, and the
/// `fail` also gains the evidence suffix `(sanctioned: <reference>)`.
fn book_sanction(row: &mut AuditRow, rule: &str, reference: &str) -> Booking {
    if !is_deterministic(row) {
        row.sanctioned = Some(reference.to_string());
        Booking::Judged
    } else if row.result == RowResult::Fail {
        row.evidence = format!("{} (sanctioned: {reference})", row.evidence);
        row.sanctioned = Some(reference.to_string());
        Booking::Failed
    } else {
        Booking::Stale(format!(
            "{}row result is \"{}\", not fail",
            stale_line_start(rule, reference),
            row.result.as_str()
        ))
    }
}

/// Re-judges the `--sanctioned` bookings on the rows of `comparing` after
/// `compare_self_audit` moved them. The audit judged those bookings while each
/// row still passed, so a booking on a row that is now a `fail` was called
/// stale. Each such booking is judged again against the row's printed result:
/// a `fail` carries the booking and counts in `sanctioned_fail`, and any other
/// result keeps a stale line that names it. A row that still passes keeps the
/// first judgement.
fn rejudge_moved_bookings(
    result: &mut AuditResult,
    comparing: &[String],
    sanctioned: &[(String, String)],
) {
    let mut stale = result.stale_sanctions.clone().unwrap_or_default();
    let mut sanctioned_fail = result.summary.sanctioned_fail.unwrap_or(0);
    let mut moved = false;
    for (rule, reference) in sanctioned
        .iter()
        .filter(|(rule, _)| comparing.contains(rule))
    {
        let Some(row) = result.rules.iter_mut().find(|row| row.id == *rule) else {
            continue;
        };
        if row.result == RowResult::Pass {
            continue;
        }
        moved = true;
        let start = stale_line_start(rule, reference);
        let slot = stale.iter().position(|line| line.starts_with(&start));
        match (book_sanction(row, rule, reference), slot) {
            (Booking::Judged, _) => {}
            (Booking::Failed, slot) => {
                sanctioned_fail += 1;
                if let Some(index) = slot {
                    stale.remove(index);
                }
            }
            (Booking::Stale(line), Some(index)) => stale[index] = line,
            (Booking::Stale(line), None) => stale.push(line),
        }
    }
    if !moved {
        return;
    }
    if sanctioned_fail > 0 {
        result.summary.sanctioned_fail = Some(sanctioned_fail);
    }
    result.stale_sanctions = (!stale.is_empty()).then_some(stale);
}

// ---- CLI --------------------------------------------------------------------------

/// Parses each `--sanctioned` value as `<rule>=<ref>`: clap's own grammar
/// (`main.rs`'s `Audit::sanctioned: Vec<String>`) admits any string, so a
/// value missing the separator, naming an empty rule id or an empty
/// reference, or repeating a rule id an earlier value already named, is
/// this command's own usage error -- checked after `cmd_audit` resolves
/// `root` and loads the knowledge base, but before any audit work starts.
/// An empty reference would book a sanction naming no reference at all,
/// and a repeated rule id would let `sanctioned_fail` count past the one
/// row it can ever annotate -- both are this function's business, not
/// the apply loop's, so a malformed `--sanctioned` is caught before any
/// row is even read. Splits on the FIRST `=` only, so a reference itself
/// containing `=` (a URL query string, say) survives whole.
fn parse_sanctioned(raw: Vec<String>) -> Result<Vec<(String, String)>, String> {
    let mut seen_rules: Vec<String> = Vec::new();
    let mut pairs = Vec::with_capacity(raw.len());
    for item in raw {
        let Some((rule, reference)) = item.split_once('=') else {
            return Err(format!(
                "--sanctioned \"{item}\" needs the shape <rule>=<ref>"
            ));
        };
        if rule.is_empty() || reference.is_empty() {
            return Err(format!(
                "--sanctioned \"{item}\" needs the shape <rule>=<ref>"
            ));
        }
        if seen_rules.iter().any(|seen| seen == rule) {
            return Err(format!(
                "--sanctioned \"{rule}\" is already sanctioned; declare it once"
            ));
        }
        seen_rules.push(rule.to_string());
        pairs.push((rule.to_string(), reference.to_string()));
    }
    Ok(pairs)
}

/// Runs the `audit` subcommand: resolves `root` (`--dir`, or the nearest
/// install root), loads the knowledge base there, parses
/// `--ids` as a comma-separated, trimmed, non-empty list and
/// `--sanctioned` per `parse_sanctioned`, and prints the JSON result on
/// stdout. `--json` writes the same result to a file.
///
/// A triggered `runner` check can stop the audit before it has a result: a
/// tool that `runners` does not define, a malformed `runners` entry, a
/// program the system does not find, a working tree that is not the tree at
/// `--head`, and a SARIF document that is unusable each give one stderr line
/// and exit 2, with no rows (`runner::run` returns the line as its `Err`).
///
/// The install's tracker comes into it as spec 3.4 and 3.7 say. The audit
/// reads the work-item references of its range: the `Work-Item:` and
/// `Deferred:` trailers of each commit, and the `issues[].backlog` ids of the
/// deliverables it reads (`trailers::collect`). It judges them
/// (`judge_references`) before it prints the result, and the result holds the
/// verdict: `references` (`checked`, `passed`, `skipped`, and one line in
/// `failures` for each failure) and the `reference_fail` count of the summary.
/// With a tracker, it prints each failure line on stderr too, and exits 1 when
/// there is one, whatever the checks decided; a tracker that cannot be reached
/// is its own failure line and exit 1, and any other tracker error is a
/// failure line and exit 2, each with every reference skipped. With no tracker
/// and no `backlog/` directory, it resolves none, counts them as skipped, and
/// prints the line that counts the references it did not resolve on stderr,
/// `skipped 0` included. With no tracker and a `backlog/` directory, line B is
/// the one failure and the exit is 1. Every such line is on stderr, so stdout
/// stays the JSON result. A tracker that cannot be built, or a tracker key that
/// cannot be read, is its own line and exit 2, with no result.
///
/// A `--report` run then compares the `self_audit` of the report with the
/// result (`compare_self_audit`, spec 6). A difference makes the row of the
/// check that reads `self_audit` a `fail` (a `warn` for a check of level
/// `warn`), moves the summary counts with it, and makes the exit 1 for a
/// `fail`. The audit then judges again the `--sanctioned` booking on that row
/// (`rejudge_moved_bookings`), so that `sanctioned`, `sanctioned_fail`, and
/// `stale_sanctions` describe the printed result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_audit(
    dir: Option<PathBuf>,
    base_ref: Option<String>,
    head_ref: Option<String>,
    ids: Option<String>,
    report: Option<PathBuf>,
    workspace: Option<PathBuf>,
    json: Option<PathBuf>,
    sanctioned: Vec<String>,
) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let base = match load_base(&root) {
        Ok(base) => base,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let tracker = match crate::tracker::lookup(&root) {
        Ok(tracker) => tracker,
        Err(error) => return error.report(true),
    };
    let ids = match ids {
        Some(raw) => raw
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .collect(),
        None => Vec::new(),
    };
    let sanctioned = match parse_sanctioned(sanctioned) {
        Ok(pairs) => pairs,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let outcome = audit(
        &base,
        AuditOptions {
            base_ref,
            head_ref,
            ids,
            report,
            workspace,
            sanctioned: sanctioned.clone(),
        },
    );
    let mut outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let verdict = judge_references(&tracker, &mut outcome);
    if let Some(report) = &outcome.report {
        outcome.failed |= runner::compare_self_audit(
            &base.root,
            report,
            &outcome.self_audit_rows,
            &mut outcome.result,
        );
        rejudge_moved_bookings(&mut outcome.result, &outcome.self_audit_rows, &sanctioned);
    }
    let printed =
        emit(&serde_json::to_value(&outcome.result).expect("an audit result always serializes"));
    if let Some(path) = &json
        && let Err(error) = std::fs::write(path, &printed)
    {
        eprintln!("{}: {error}", path.display());
        return ExitCode::from(2);
    }
    print!("{printed}");
    for line in verdict.stderr_lines() {
        eprintln!("{line}");
    }
    match verdict.exit_code {
        Some(code) => ExitCode::from(code),
        None if outcome.failed || !verdict.failures.is_empty() => ExitCode::from(1),
        None => ExitCode::SUCCESS,
    }
}

/// Judges the work-item references of `outcome` against `tracker` (spec 3.4
/// and 3.7) and adds the verdict to its result: the `references` object
/// (`checked`, `passed`, `skipped`, and the `failures` lines) and the
/// `reference_fail` count of the summary. The `fail` count of the summary stays
/// the count of the rule rows. Gives the verdict, which also holds the stderr
/// lines and the exit code of a tracker error. `cmd_audit` and [`audit_result`]
/// share this one step, so the in-process audit and `audit --json` print the
/// same references.
fn judge_references(tracker: &Lookup, outcome: &mut AuditOutcome) -> trailers::Verdict {
    let verdict = trailers::judge(tracker, &outcome.references);
    outcome.result.summary.reference_fail = verdict.failures.len() as u64;
    outcome.result.references = References {
        checked: verdict.checked as u64,
        passed: verdict.passed as u64,
        skipped: verdict.skipped as u64,
        failures: verdict.failures.clone(),
    };
    verdict
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::process::Command;

    use serde_json::json;

    use super::*;
    use crate::rules::model::load_base;

    /// The result of `outcome` as the JSON document that `audit` prints, which
    /// the tests below read by key.
    fn printed(outcome: &AuditOutcome) -> Value {
        serde_json::to_value(&outcome.result).expect("an audit result serializes")
    }

    // ---- fixture builders ----

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn write_file(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        fs_create_parent(&path);
        std::fs::write(path, content).unwrap();
    }

    fn fs_create_parent(path: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
    }

    fn commit(root: &Path, message: &str, body: Option<&str>) -> String {
        commit_as(root, "t <t@t.t>", message, body)
    }

    fn commit_as(root: &Path, author: &str, message: &str, body: Option<&str>) -> String {
        git(root, &["add", "-A"]);
        let author_flag = format!("--author={author}");
        let mut args = vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            &author_flag,
            "-m",
            message,
        ];
        if let Some(body) = body {
            args.push("-m");
            args.push(body);
        }
        git(root, &args);
        git(root, &["rev-parse", "HEAD"]).trim().to_string()
    }

    fn entry(overrides: Value) -> Value {
        let mut base = json!({
            "id": "process.sequential", "kind": "rule", "area": "process", "standing": true,
            "summary": "Run agents sequentially.", "body": ["One at a time."], "tags": ["dispatch"],
            "source": {"date": "2026-08-29", "by": "user"},
        });
        if let (Value::Object(base_map), Value::Object(over_map)) = (&mut base, overrides) {
            for (key, value) in over_map {
                base_map.insert(key, value);
            }
        }
        base
    }

    /// The knowledge schema as the kit ships it.
    fn seed_schema() -> Value {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn areas_json() -> Value {
        json!({
            "global": {"paths": []}, "process": {"paths": []},
            "rust": {"paths": ["crates/**", "Cargo.toml"]},
            "webview": {"paths": ["apps/desktop/src/**"]},
            "api": {"paths": ["apps/api/**"]},
            "schemas": {"paths": ["packages/schemas/**"]},
            "infra": {"paths": ["tools/**", ".github/**"]},
            "docs": {"paths": ["docs/**", "CLAUDE.md"]},
        })
    }

    fn write_topics(root: &Path, entries: &[Value]) {
        let mut by_topic: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for e in entries {
            let id = e["id"].as_str().expect("entry id is a string");
            let topic = id.split('.').next().expect("entry id has a topic prefix");
            by_topic
                .entry(topic.to_string())
                .or_default()
                .push(e.clone());
        }
        for (topic, topic_entries) in by_topic {
            let content = json!({
                "$schema": "./schema.json", "topic": topic,
                "title": format!("{topic} title"), "entries": topic_entries,
            });
            write_file(
                root,
                &format!("knowledge/{topic}.json"),
                &serde_json::to_string(&content).unwrap(),
            );
        }
    }

    /// A seeded repository with `files` written before the initial
    /// commit.
    fn make_repo_with_files(entries: &[Value], files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        write_file(
            root,
            "knowledge/schema.json",
            &serde_json::to_string(&seed_schema()).unwrap(),
        );
        write_file(
            root,
            "knowledge/areas.json",
            &serde_json::to_string(&areas_json()).unwrap(),
        );
        write_topics(root, entries);
        write_file(root, "CLAUDE.md", "# Test\n");
        for (path, content) in files {
            write_file(root, path, content);
        }
        commit(root, "chore: init", None);
        dir
    }

    fn make_repo(entries: &[Value]) -> tempfile::TempDir {
        make_repo_with_files(entries, &[])
    }

    fn audit_entries() -> Vec<Value> {
        vec![
            entry(json!({
                "id": "process.commits", "summary": "Conventional commits, no co-author.",
                "check": {
                    "type": "commits", "level": "fail",
                    "subject": "^(feat|fix|chore|docs|test): .+",
                    "body_absent": "co-authored-by", "flags": "i",
                },
            })),
            entry(json!({
                "id": "infra.pins", "area": "infra", "standing": false, "summary": "Exact pins.",
                "check": {
                    "type": "grep-absent", "level": "fail", "files": "**/package.json",
                    "pattern": "\"[\\^~]\\d", "scope": "changed",
                },
            })),
            entry(json!({
                "id": "infra.tree", "area": "infra", "standing": false,
                "summary": "No FORBIDDEN word in docs.",
                "check": {
                    "type": "grep-absent", "level": "warn", "files": "docs/**/*.md",
                    "pattern": "FORBIDDEN", "scope": "tree",
                },
            })),
            entry(json!({
                "id": "infra.a19", "area": "infra", "standing": false, "summary": "A-19 in the report.",
                "check": {
                    "type": "report-field", "level": "fail",
                    "if": ["**/package.json", "**/Cargo.toml"], "field": "a19",
                },
            })),
            entry(json!({
                "id": "rust.append", "area": "rust", "standing": false,
                "summary": "Migrations append-only.",
                "check": {"type": "diff-append-only", "level": "warn", "files": "crates/db/migrations.rs"},
            })),
            entry(json!({
                "id": "rust.cochange", "area": "rust", "standing": false,
                "summary": "lib.rs changes join the harness.",
                "check": {
                    "type": "co-change", "level": "fail", "if": "crates/lib.rs",
                    "then": "crates/tests/harness.rs",
                },
            })),
            entry(json!({
                "id": "rust.judged", "area": "rust", "standing": false, "summary": "A judged rule.",
            })),
            entry(json!({
                "id": "webview.unrelated", "area": "webview", "standing": false,
                "summary": "Not in the package.",
            })),
            entry(json!({
                "id": "process.proc", "kind": "procedure", "standing": false, "summary": "A procedure.",
            })),
        ]
    }

    /// A report-field check on any package.json -> `dependency_vetting`,
    /// shared by the workspace tests.
    fn report_field_entry() -> Value {
        entry(json!({
            "id": "process.reportws",
            "summary": "Every triggered report carries dependency_vetting.",
            "check": {
                "type": "report-field", "level": "fail", "if": "**/package.json",
                "field": "dependency_vetting",
            },
        }))
    }

    /// A repository with a rule that checks `field` on any change, and a
    /// range that changes `a.txt`; returns the repository and the range's
    /// base sha.
    fn make_repo_checking_field_on_a_change(field: &str) -> (tempfile::TempDir, String) {
        let dir = make_repo(&[entry(json!({
            "id": "process.fieldcheck",
            "summary": "Every report carries the field.",
            "check": {"type": "report-field", "level": "fail", "if": "**", "field": field},
        }))]);
        let root = dir.path();
        let base_sha = git(root, &["rev-parse", "HEAD"]).trim().to_string();
        write_file(root, "a.txt", "x\n");
        commit(root, "feat: change", None);
        (dir, base_sha)
    }

    /// The row a single-report (`--report`) audit gives a report-field check
    /// on `field` for `report`.
    fn single_report_field_row(field: &str, report: Value) -> Value {
        let (dir, base_sha) = make_repo_checking_field_on_a_change(field);
        let root = dir.path();
        let base = load_base(root).unwrap();
        let report_path = root.join("report.json");
        std::fs::write(&report_path, serde_json::to_string(&report).unwrap()).unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                report: Some(report_path),
                ..Default::default()
            },
        )
        .unwrap();
        printed(&outcome)["rules"][0].clone()
    }

    fn write_workspace(reports: &[(&str, Value)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        for (name, body) in reports {
            std::fs::write(dir.path().join(name), serde_json::to_string(body).unwrap()).unwrap();
        }
        dir
    }

    fn audit_opts(base_ref: &str) -> AuditOptions {
        AuditOptions {
            base_ref: Some(base_ref.to_string()),
            ..Default::default()
        }
    }

    fn row_field<'a>(rows: &'a [Value], id: &str, field: &str) -> &'a Value {
        &rows
            .iter()
            .find(|row| row["id"] == id)
            .unwrap_or_else(|| panic!("no row {id:?} in {rows:#?}"))[field]
    }

    // ---- audit ----

    #[test]
    fn derives_the_package_from_standing_rules_touched_areas_and_ids_and_runs_every_check() {
        let dir = make_repo_with_files(
            &audit_entries(),
            &[
                ("docs/x.md", "FORBIDDEN\n"),
                ("crates/db/migrations.rs", "a\nb\n"),
                ("crates/lib.rs", "x\n"),
            ],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(
            root,
            "tools/package.json",
            "{\"dependencies\":{\"x\":\"^1.0.0\"}}\n",
        );
        write_file(root, "crates/db/migrations.rs", "a\n");
        write_file(root, "crates/lib.rs", "y\n");
        commit(root, "feat: change", Some("Co-Authored-By: someone"));

        let base = load_base(root).expect("load base");
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                head_ref: Some("HEAD".to_string()),
                ids: vec!["process.proc".to_string()],
                ..Default::default()
            },
        )
        .expect("audit");
        assert!(outcome.failed);

        let rows = printed(&outcome)["rules"].as_array().unwrap().clone();
        let summary_view: Vec<(String, String, String)> = rows
            .iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_string(),
                    r["mode"].as_str().unwrap().to_string(),
                    r["result"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            summary_view,
            vec![
                ("infra.a19".into(), "deterministic".into(), "skipped".into()),
                ("infra.pins".into(), "deterministic".into(), "fail".into()),
                ("infra.tree".into(), "deterministic".into(), "warn".into()),
                (
                    "process.commits".into(),
                    "deterministic".into(),
                    "fail".into()
                ),
                ("process.proc".into(), "judged".into(), "open".into()),
                ("rust.append".into(), "deterministic".into(), "warn".into()),
                (
                    "rust.cochange".into(),
                    "deterministic".into(),
                    "fail".into()
                ),
                ("rust.judged".into(), "judged".into(), "open".into()),
            ]
        );
        assert_eq!(
            row_field(&rows, "infra.pins", "evidence"),
            &json!("tools/package.json:1 matches \"[\\^~]\\d")
        );
        assert_eq!(
            row_field(&rows, "infra.tree", "evidence"),
            &json!("docs/x.md:1 matches FORBIDDEN")
        );
        assert_eq!(
            row_field(&rows, "process.commits", "evidence"),
            &json!("commit \"feat: change\" body matches co-authored-by")
        );
        assert_eq!(
            row_field(&rows, "rust.append", "evidence"),
            &json!("1 removed lines in crates/db/migrations.rs")
        );
        assert_eq!(
            row_field(&rows, "rust.cochange", "evidence"),
            &json!("crates/lib.rs changed without crates/tests/harness.rs")
        );
        assert_eq!(
            row_field(&rows, "infra.a19", "evidence"),
            &json!("no --report given")
        );

        let summary = &printed(&outcome)["summary"];
        assert_eq!(summary["base"], printed(&outcome)["base"]);
        assert_eq!(summary["head"], printed(&outcome)["head"]);
        assert_eq!(summary["deterministic"], json!(6));
        assert_eq!(summary["pass"], json!(0));
        assert_eq!(summary["fail"], json!(3));
        assert_eq!(summary["warn"], json!(2));
        assert_eq!(summary["skipped"], json!(1));
        assert_eq!(summary["judged"], json!(2));
        assert!(summary.get("empty_range").is_none());

        let data = &printed(&outcome);
        assert_eq!(data["ids"], json!(["process.proc"]));
        assert_eq!(
            data["changed_files"],
            json!([
                "crates/db/migrations.rs",
                "crates/lib.rs",
                "tools/package.json"
            ])
        );
        assert_eq!(data["areas"], json!(["global", "infra", "rust"]));
        assert_eq!(
            data["area_files"],
            json!({
                "global": [], "infra": ["tools/package.json"],
                "rust": ["crates/db/migrations.rs", "crates/lib.rs"],
            })
        );
        assert_eq!(data["rules"].as_array().unwrap().len(), 8);
    }

    /// A check needs an audit loading path of its own; an area match
    /// must admit a checked entry of any kind, not only rule and
    /// invariant.
    #[test]
    fn joins_a_checked_procedure_entry_when_its_area_is_touched() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "infra.checked-proc", "kind": "procedure", "area": "infra",
                "standing": false, "summary": "A checked procedure.",
                "check": {"type": "report-field", "level": "warn", "if": "**", "field": "live_run"},
            }))],
            &[("tools/x.txt", "a\n")],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/x.txt", "b\n");
        commit(root, "feat: touch infra", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let printed_outcome = printed(&outcome);
        let ids: Vec<&str> = printed_outcome["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"infra.checked-proc"));
    }

    #[test]
    fn still_excludes_an_unchecked_procedure_entry_even_when_its_area_is_touched() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "infra.unchecked-proc", "kind": "procedure", "area": "infra",
                "standing": false, "summary": "An unchecked procedure.",
            }))],
            &[("tools/x.txt", "a\n")],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/x.txt", "b\n");
        commit(root, "feat: touch infra", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let printed_outcome = printed(&outcome);
        let ids: Vec<&str> = printed_outcome["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["id"].as_str().unwrap())
            .collect();
        assert!(!ids.contains(&"infra.unchecked-proc"));
    }

    /// An audit whose range holds no commits reads as a vacuous one, not
    /// clean evidence ('0 commits checked').
    #[test]
    fn stamps_a_base_equals_head_audit_as_vacuous() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        let base = load_base(root).unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha.clone()),
                head_ref: Some(base_sha),
                ids: vec!["infra.a19".to_string(), "rust.judged".to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(printed(&outcome)["summary"]["empty_range"], json!(true));
        let printed_outcome = printed(&outcome);
        let rows = printed_outcome["rules"].as_array().unwrap();
        let deterministic: Vec<&Value> = rows
            .iter()
            .filter(|r| r["mode"] == "deterministic")
            .collect();
        assert!(deterministic.len() > 1);
        assert!(
            deterministic
                .iter()
                .all(|r| r["evidence"].as_str().unwrap().starts_with("empty range: "))
        );
        assert_eq!(
            row_field(rows, "process.commits", "evidence"),
            &json!("empty range: 0 commits checked")
        );
        assert_eq!(
            row_field(rows, "infra.a19", "evidence"),
            &json!("empty range: not triggered")
        );
        let judged = rows.iter().find(|r| r["id"] == "rust.judged").unwrap();
        assert_eq!(judged["mode"], json!("judged"));
        assert_eq!(judged["evidence"], json!("\u{2014}"));
    }

    #[test]
    fn passes_a_clean_range_and_reports_report_field_against_a_json_report() {
        let dir = make_repo_with_files(&audit_entries(), &[("crates/tests/harness.rs", "h\n")]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(
            root,
            "tools/package.json",
            "{\"dependencies\":{\"x\":\"1.0.0\"}}\n",
        );
        write_file(root, "crates/lib.rs", "y\n");
        write_file(root, "crates/tests/harness.rs", "h2\n");
        write_file(root, "crates/db/migrations.rs", "new\n");
        commit(root, "feat: clean change", None);
        let base = load_base(root).unwrap();

        let report_a = root.join("report-a.json");
        std::fs::write(
            &report_a,
            serde_json::to_string(&json!({"a19": Value::Null})).unwrap(),
        )
        .unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha.clone()),
                report: Some(report_a),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(outcome.failed);
        assert_eq!(
            row_field(
                printed(&outcome)["rules"].as_array().unwrap(),
                "infra.a19",
                "evidence"
            ),
            &json!("report lacks a value for a19 (triggered by tools/package.json)")
        );

        let report_b = root.join("report-b.json");
        std::fs::write(
            &report_b,
            serde_json::to_string(
                &json!({"a19": {"manifests": ["tools/package.json"], "dependencies": []}}),
            )
            .unwrap(),
        )
        .unwrap();
        let outcome2 = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                report: Some(report_b),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!outcome2.failed);
        let printed_outcome2 = printed(&outcome2);
        let rows2 = printed_outcome2["rules"].as_array().unwrap();
        let view: Vec<(String, String, String)> = rows2
            .iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_string(),
                    r["result"].as_str().unwrap().to_string(),
                    r["evidence"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            view,
            vec![
                (
                    "infra.a19".into(),
                    "pass".into(),
                    "report field a19 is set".into()
                ),
                ("infra.pins".into(), "pass".into(), "1 files checked".into()),
                ("infra.tree".into(), "pass".into(), "0 files checked".into()),
                (
                    "process.commits".into(),
                    "pass".into(),
                    "1 commits checked".into()
                ),
                (
                    "rust.append".into(),
                    "pass".into(),
                    "crates/db/migrations.rs: no removed lines".into(),
                ),
                (
                    "rust.cochange".into(),
                    "pass".into(),
                    "crates/lib.rs changed with crates/tests/harness.rs".into(),
                ),
                ("rust.judged".into(), "open".into(), "\u{2014}".into()),
            ]
        );
        let summary2 = &printed(&outcome2)["summary"];
        assert_eq!(summary2["deterministic"], json!(6));
        assert_eq!(summary2["pass"], json!(6));
        assert_eq!(summary2["fail"], json!(0));
        assert_eq!(summary2["warn"], json!(0));
        assert_eq!(summary2["skipped"], json!(0));
        assert_eq!(summary2["judged"], json!(1));
    }

    #[test]
    fn errors_with_invalid_json_when_report_does_not_hold_valid_json() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        let bad = root.join("report.json");
        std::fs::write(&bad, "not json").unwrap();
        let base = load_base(root).unwrap();
        let error = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                report: Some(bad),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("invalid JSON"), "{error}");
    }

    #[test]
    fn reads_a_dotted_field_path_and_reports_warn_or_pass_by_its_value() {
        let dir = make_repo(&[entry(json!({
            "id": "process.dotted", "summary": "Self-audit summary is filled.",
            "check": {
                "type": "report-field", "level": "warn", "if": "**",
                "field": "self_audit.summary",
            },
        }))]);
        let root = dir.path();
        let base_sha = git(root, &["rev-parse", "HEAD"]).trim().to_string();
        write_file(root, "a.txt", "x\n");
        commit(root, "feat: change", None);
        let base = load_base(root).unwrap();

        let warn_report = root.join("report-warn.json");
        std::fs::write(
            &warn_report,
            serde_json::to_string(&json!({"self_audit": {"summary": Value::Null}})).unwrap(),
        )
        .unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha.clone()),
                report: Some(warn_report),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(printed(&outcome)["rules"][0]["result"], json!("warn"));

        let pass_report = root.join("report-pass.json");
        std::fs::write(
            &pass_report,
            serde_json::to_string(&json!({"self_audit": {"summary": {"pass": 1}}})).unwrap(),
        )
        .unwrap();
        let outcome2 = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                report: Some(pass_report),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(printed(&outcome2)["rules"][0]["result"], json!("pass"));
    }

    /// An implementer report missing a live-run recipe warns instead of a
    /// full-text search; disclosed-mutation proof, not a natural RED
    /// (`process.tdd`): with `if` set to a glob that never matches, the
    /// missing-report assertion below fails ('pass', not 'warn');
    /// restoring `if: "**"` makes it pass again.
    #[test]
    fn warns_a_report_field_row_when_live_run_is_missing_and_passes_when_present_even_empty() {
        let dir = make_repo(&[entry(json!({
            "id": "houserules.livesample", "summary": "Every report carries live_run.",
            "check": {"type": "report-field", "level": "warn", "if": "**", "field": "live_run"},
        }))]);
        let root = dir.path();
        let base_sha = git(root, &["rev-parse", "HEAD"]).trim().to_string();
        write_file(root, "a.txt", "x\n");
        commit(root, "feat: change", None);
        let base = load_base(root).unwrap();

        let missing_report = root.join("report-missing.json");
        std::fs::write(&missing_report, serde_json::to_string(&json!({})).unwrap()).unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha.clone()),
                report: Some(missing_report),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(printed(&outcome)["rules"][0]["result"], json!("warn"));

        let empty_report = root.join("report-empty.json");
        std::fs::write(
            &empty_report,
            serde_json::to_string(&json!({"live_run": []})).unwrap(),
        )
        .unwrap();
        let outcome2 = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                report: Some(empty_report),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(printed(&outcome2)["rules"][0]["result"], json!("pass"));
    }

    /// The draft state of an implementer report: `self_audit` is an explicit
    /// `null` until the implementer copies the audit's printed `summary` and
    /// deterministic rows into it. The row must equal the row of a filled
    /// `self_audit`, so the rows the implementer copies out of this run stay
    /// true after the field holds them. The evidence says the field is
    /// `present`, not `set`: a `null` field is present and not set.
    /// `validate` guards the final state
    /// (`rejects_a_done_report_with_a_null_self_audit` and
    /// `rejects_a_done_with_concerns_report_with_a_null_self_audit` in
    /// `validate_deliverable.rs`).
    #[test]
    fn report_field_passes_a_null_self_audit_in_a_single_report_run() {
        let draft = single_report_field_row("self_audit", json!({"self_audit": Value::Null}));
        let filled = single_report_field_row(
            "self_audit",
            json!({"self_audit": {"summary": {"pass": 1}, "rows": []}}),
        );

        assert_eq!(draft, filled);
        assert_eq!(draft["result"], json!("pass"));
        assert_eq!(
            draft["evidence"],
            json!("report field self_audit is present")
        );
    }

    /// Only an explicit `null` is the draft state: a report that lacks the
    /// key still fails with the text a missing field has always had.
    #[test]
    fn report_field_fails_an_absent_self_audit_key() {
        let row = single_report_field_row("self_audit", json!({"live_run": []}));

        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!("report lacks a value for self_audit (triggered by a.txt)")
        );
    }

    /// `--workspace` judges finished reports: a null `self_audit` there is
    /// still a violation, naming the report.
    #[test]
    fn report_field_fails_a_null_self_audit_in_a_workspace_run() {
        let (dir, base_sha) = make_repo_checking_field_on_a_change("self_audit");
        let base = load_base(dir.path()).unwrap();
        let workspace = write_workspace(&[(
            "task-1-report.json",
            json!({
                "kind": "task-report", "files_changed": ["a.txt"], "self_audit": Value::Null,
            }),
        )]);

        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();

        assert!(outcome.failed);
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!("task-1-report.json lacks a value for self_audit (triggered by a.txt)")
        );
    }

    /// `self_audit` is the one field with a draft state: a null value of
    /// any other field, a dotted path under a null `self_audit` included,
    /// still fails.
    #[test]
    fn report_field_still_fails_a_null_value_of_another_field() {
        let other = single_report_field_row(
            "dependency_vetting",
            json!({"dependency_vetting": Value::Null}),
        );
        assert_eq!(other["result"], json!("fail"));
        assert_eq!(
            other["evidence"],
            json!("report lacks a value for dependency_vetting (triggered by a.txt)")
        );

        let nested =
            single_report_field_row("self_audit.summary", json!({"self_audit": Value::Null}));
        assert_eq!(nested["result"], json!("fail"));
        assert_eq!(
            nested["evidence"],
            json!("report lacks a value for self_audit.summary (triggered by a.txt)")
        );
    }

    /// A malformed `then` glob must be named, never swallowed into a
    /// false violation: `match_any(path,
    /// &check.then).unwrap_or(false)` would turn a `GlobError` (a
    /// descending bracket range, say) into a silent "did not match", so
    /// the check would report a `fail` row instead of the audit itself
    /// failing to run.
    #[test]
    fn a_malformed_then_glob_is_named_not_swallowed_into_a_false_violation() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "process.badthen", "area": "global", "standing": false,
                "summary": "lib.rs changes join the harness.",
                "check": {
                    "type": "co-change", "level": "fail", "if": "crates/lib.rs",
                    "then": "src/[z-a].js",
                },
            }))],
            &[("crates/lib.rs", "a\n")],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "crates/lib.rs", "b\n");
        commit(root, "feat: change lib", None);
        let base = load_base(root).unwrap();
        let error = audit(&base, audit_opts(&base_sha)).unwrap_err();
        assert!(error.contains("invalid glob"), "{error}");
        assert!(error.contains("src/[z-a].js"), "{error}");
    }

    /// A standing rule carrying a malformed `check` object must be
    /// named, not silently downgraded to a judged row: the standard
    /// one-line, exit-2 finding names both the entry and the reason,
    /// never silence.
    #[test]
    fn a_malformed_check_on_a_standing_rule_is_named_not_silently_downgraded_to_judged() {
        let dir = make_repo(&[entry(json!({
            "id": "process.badcheck",
            "check": {"type": "unknown-type", "level": "fail"},
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "a.txt", "x\n");
        commit(root, "feat: change", None);
        let base = load_base(root).unwrap();
        let error = audit(&base, audit_opts(&base_sha)).unwrap_err();
        assert!(error.contains("process.badcheck"), "{error}");
        assert!(error.contains("malformed check"), "{error}");
    }

    /// A `co-change` `then` glob (`match_any`) must cross a dot-segment,
    /// the same as `area_files` does at the area-resolution call site.
    #[test]
    fn matches_a_co_change_then_glob_across_a_dot_segment() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "process.dotcochange", "area": "global", "standing": false,
                "summary": "trigger.txt co-changes with anything under src/, dot-segments included.",
                "check": {
                    "type": "co-change", "level": "fail", "if": "trigger.txt", "then": "src/**",
                },
            }))],
            &[("trigger.txt", "a\n")],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "trigger.txt", "b\n");
        write_file(root, "src/.config/x.json", "{}\n");
        commit(root, "feat: change", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        assert_eq!(
            row_field(
                printed(&outcome)["rules"].as_array().unwrap(),
                "process.dotcochange",
                "evidence"
            ),
            &json!("trigger.txt changed with src/.config/x.json")
        );
    }

    /// When the only `if` match is the `then` path itself, naming it as
    /// both the trigger and the record reads as circular; the evidence
    /// names the case plainly instead.
    #[test]
    fn names_a_record_only_co_change_satisfied_by_definition() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "process.recordonly", "area": "global", "standing": false,
                "summary": "trigger.txt or record.json co-changes with record.json.",
                "check": {
                    "type": "co-change", "level": "fail",
                    "if": ["trigger.txt", "record.json"], "then": "record.json",
                },
            }))],
            &[("trigger.txt", "a\n"), ("record.json", "{}\n")],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "record.json", "{\"n\":1}\n");
        commit(root, "feat: append a run", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        assert_eq!(
            row_field(
                printed(&outcome)["rules"].as_array().unwrap(),
                "process.recordonly",
                "evidence"
            ),
            &json!("only record.json changed; the co-change is satisfied by definition")
        );
    }

    #[test]
    fn names_the_real_trigger_not_the_record_in_a_mixed_co_change() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "process.recordmixed", "area": "global", "standing": false,
                "summary": "trigger.txt or record.json co-changes with record.json.",
                "check": {
                    "type": "co-change", "level": "fail",
                    "if": ["trigger.txt", "record.json"], "then": "record.json",
                },
            }))],
            &[("trigger.txt", "a\n"), ("record.json", "{}\n")],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "trigger.txt", "b\n");
        write_file(root, "record.json", "{\"n\":1}\n");
        commit(root, "feat: change trigger and record", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        assert_eq!(
            row_field(
                printed(&outcome)["rules"].as_array().unwrap(),
                "process.recordmixed",
                "evidence"
            ),
            &json!("trigger.txt changed with record.json")
        );
    }

    /// A `then` glob that matches several changed files, with nothing
    /// else matching `if`, must not name any of them as the trigger
    /// either.
    #[test]
    fn names_no_then_matching_file_as_the_trigger_when_several_then_files_changed() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "process.recordmulti", "area": "global", "standing": false,
                "summary": "Any recs/*.json co-changes with any recs/*.json.",
                "check": {
                    "type": "co-change", "level": "fail", "if": "recs/*.json", "then": "recs/*.json",
                },
            }))],
            &[("recs/a.json", "{}\n"), ("recs/record.json", "{}\n")],
        );
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "recs/a.json", "{\"n\":1}\n");
        write_file(root, "recs/record.json", "{\"n\":1}\n");
        commit(root, "feat: append two runs", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        assert_eq!(
            row_field(
                printed(&outcome)["rules"].as_array().unwrap(),
                "process.recordmulti",
                "evidence"
            ),
            &json!(
                "only recs/a.json, recs/record.json changed; the co-change is satisfied by definition"
            )
        );
    }

    #[test]
    fn reports_untriggered_checks_and_a_bad_subject() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "docs/new.md", "hello\n");
        commit(root, "bad subject", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let printed_outcome = printed(&outcome);
        let rows = printed_outcome["rules"].as_array().unwrap();
        let view: Vec<(String, String, String)> = rows
            .iter()
            .map(|r| {
                (
                    r["id"].as_str().unwrap().to_string(),
                    r["result"].as_str().unwrap().to_string(),
                    r["evidence"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            view,
            vec![(
                "process.commits".into(),
                "fail".into(),
                "commit \"bad subject\" does not match ^(feat|fix|chore|docs|test): .+".into(),
            )]
        );

        write_file(root, "Cargo.toml", "[package]\n");
        write_file(root, "tools/Cargo.toml", "[package]\n");
        commit(root, "chore: cargo", None);
        let base_again = load_base(root).unwrap();
        let again = audit(&base_again, audit_opts(&base_sha)).unwrap();
        let printed_again = printed(&again);
        let again_rows = printed_again["rules"].as_array().unwrap();
        assert_eq!(
            row_field(again_rows, "rust.append", "evidence"),
            &json!("not triggered")
        );
        assert_eq!(
            row_field(again_rows, "rust.cochange", "evidence"),
            &json!("not triggered")
        );
        assert_eq!(
            row_field(again_rows, "infra.a19", "result"),
            &json!("skipped")
        );
    }

    #[test]
    fn rejects_a_missing_base_a_bad_ref_and_an_unknown_id() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base = load_base(root).unwrap();
        assert_eq!(
            audit(&base, AuditOptions::default()).unwrap_err(),
            "audit needs --base <ref>"
        );
        assert_eq!(
            audit(
                &base,
                AuditOptions {
                    base_ref: Some("nope".to_string()),
                    ..Default::default()
                },
            )
            .unwrap_err(),
            "bad ref \"nope\""
        );
        let error = audit(
            &base,
            AuditOptions {
                base_ref: Some("HEAD".to_string()),
                ids: vec!["x.y".to_string()],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("unknown id \"x.y\""), "{error}");
    }

    /// Not in the brief: `auditEntries()`'s checks always carry both
    /// `subject` and `body_absent` on a commits check, and always trigger;
    /// these three tests cover the branches that combination never
    /// reaches.
    #[test]
    fn runs_a_commits_check_with_only_body_absent_and_one_with_only_subject() {
        let dir = make_repo(&[
            entry(json!({
                "id": "process.bodyonly", "summary": "No co-author.",
                "check": {
                    "type": "commits", "level": "fail",
                    "body_absent": "co-authored-by", "flags": "i",
                },
            })),
            entry(json!({
                "id": "process.subjectonly", "summary": "Conventional subject.",
                "check": {"type": "commits", "level": "warn", "subject": "^ok: "},
            })),
        ]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "ok: real commit", Some("No trailer here."));
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let printed_outcome = printed(&outcome);
        let rows = printed_outcome["rules"].as_array().unwrap();
        assert_eq!(
            row_field(rows, "process.bodyonly", "result"),
            &json!("pass")
        );
        assert_eq!(
            row_field(rows, "process.subjectonly", "result"),
            &json!("pass")
        );
    }

    #[test]
    fn passes_a_body_line_max_check_when_the_longest_body_line_is_exactly_the_limit() {
        let dir = make_repo(&[entry(json!({
            "id": "process.bodylimit", "summary": "Wrapped commit bodies.",
            "check": {"type": "commits", "level": "fail", "body_line_max": 100},
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "feat: at limit", Some(&"x".repeat(100)));
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.bodylimit"));
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(row["evidence"], json!("1 commits checked"));
    }

    #[test]
    fn fails_a_body_line_max_check_when_a_body_line_is_one_character_over_the_limit() {
        let dir = make_repo(&[entry(json!({
            "id": "process.bodylimit", "summary": "Wrapped commit bodies.",
            "check": {"type": "commits", "level": "fail", "body_line_max": 100},
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "feat: over limit", Some(&"x".repeat(101)));
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.bodylimit"));
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!("commit \"feat: over limit\" has a body line over 100 characters")
        );
    }

    #[test]
    fn exempts_a_bot_authored_commit_from_body_line_max() {
        let dir = make_repo(&[entry(json!({
            "id": "process.bodylimit", "summary": "Wrapped commit bodies.",
            "check": {"type": "commits", "level": "fail", "body_line_max": 100},
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        let compare_link = "- [Commits](https://github.com/actions/checkout/compare/v6.1.0...3d3c42e5aac5ba805825da76410c181273ba90b1)";
        assert!(compare_link.len() > 100);
        commit_as(
            root,
            "dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
            "chore(deps): bump actions/checkout from 6.1.0 to 7.0.1",
            Some(compare_link),
        );
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.bodylimit"));
        assert_eq!(row["result"], json!("pass"), "{row}");
    }

    #[test]
    fn a_bot_authored_commit_still_fails_the_subject_rule() {
        let dir = make_repo(&[entry(json!({
            "id": "process.subject", "summary": "Conventional subjects.",
            "check": {"type": "commits", "level": "fail", "subject": "^(feat|fix|chore)(\\([^)]+\\))?: .+"},
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit_as(
            root,
            "dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
            "Bump actions/checkout from 6.1.0 to 7.0.1",
            None,
        );
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.subject"));
        assert_eq!(row["result"], json!("fail"), "{row}");
    }
    #[test]
    fn reports_a_report_field_check_as_not_triggered_when_its_trigger_files_do_not_change() {
        let dir = make_repo(&[entry(json!({
            "id": "process.reportcheck", "summary": "Needs a report field.",
            "check": {
                "type": "report-field", "level": "warn", "if": "**/package.json",
                "field": "coverage",
            },
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "chore: unrelated", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.reportcheck"));
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(row["evidence"], json!("not triggered"));
    }

    #[test]
    fn caches_a_file_read_across_two_grep_absent_checks_on_the_same_path() {
        let dir = make_repo(&[
            entry(json!({
                "id": "process.grepa", "summary": "First reader.",
                "check": {
                    "type": "grep-absent", "level": "warn", "files": "a.txt",
                    "pattern": "nope", "scope": "changed",
                },
            })),
            entry(json!({
                "id": "process.grepb", "summary": "Second reader, same file.",
                "check": {
                    "type": "grep-absent", "level": "warn", "files": "a.txt",
                    "pattern": "nope", "scope": "changed",
                },
            })),
        ]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "a.txt", "hello\n");
        commit(root, "feat: add a", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        assert!(
            printed(&outcome)["rules"]
                .as_array()
                .unwrap()
                .iter()
                .all(|r| r["result"] == "pass")
        );
    }

    /// `body_absent` must match any line of the body, not only an
    /// anchored match against the whole body string.
    #[test]
    fn matches_body_absent_against_any_body_line_not_only_the_body_start() {
        let dir = make_repo(&[entry(json!({
            "id": "process.nocoauthor", "summary": "No co-author trailer.",
            "check": {
                "type": "commits", "level": "fail", "body_absent": "^co-authored-by",
                "flags": "i",
            },
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(
            root,
            "feat: change",
            Some("An innocent first line.\nCo-Authored-By: someone"),
        );
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.nocoauthor"));
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!("commit \"feat: change\" body matches ^co-authored-by")
        );
    }

    /// A check's regex is built once and reused across every commit in
    /// the loop. `regress::Regex` carries no state across `find` calls,
    /// so stripping `g`/`y` cannot be distinguished behaviorally here --
    /// this stays a direct behavioral pin (two "ok" commits both
    /// matched).
    #[test]
    fn strips_a_g_y_flag_so_a_check_regex_cannot_leak_lastindex_across_commits() {
        let dir = make_repo(&[entry(json!({
            "id": "process.stateful", "summary": "Subject mentions ok.",
            "check": {"type": "commits", "level": "warn", "subject": "ok", "flags": "g"},
        }))]);
        let root = dir.path();
        let base_sha = git(root, &["rev-parse", "HEAD"]).trim().to_string();
        commit(root, "ok: first", None);
        commit(root, "ok: second", None);
        let base = load_base(root).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.stateful"));
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(row["evidence"], json!("2 commits checked"));
    }

    /// A three-dot diff between `base` and `head` compares from their
    /// merge base to `head`, so main's later change to a file the branch
    /// never touches stays out of the branch's own diff.
    #[test]
    fn diffs_from_the_merge_base_not_a_base_tip_that_moved_past_the_branch() {
        let dir = make_repo_with_files(
            &[entry(json!({
                "id": "process.mainonly", "summary": "main-only.txt stays off the branch diff.",
                "check": {
                    "type": "grep-absent", "level": "fail", "files": "main-only.txt",
                    "pattern": "from-main",
                },
            }))],
            &[("main-only.txt", "seed\n")],
        );
        let root = dir.path();
        git(root, &["checkout", "-q", "-b", "topic"]);
        write_file(root, "topic.txt", "topic\n");
        commit(root, "feat: topic file", None);
        git(root, &["checkout", "-q", "main"]);
        write_file(root, "main-only.txt", "seed\nfrom-main\n");
        commit(root, "chore: main-only change", None);
        let base = load_base(root).unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some("main".to_string()),
                head_ref: Some("topic".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(printed(&outcome)["changed_files"], json!(["topic.txt"]));
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["id"], json!("process.mainonly"));
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(row["evidence"], json!("0 files checked"));
    }

    /// `--workspace` judges a `report-field` check against every
    /// `task-*-report.json` in a workspace directory, instead of the
    /// single `--report` file.
    #[test]
    fn fails_a_workspace_report_field_check_naming_the_first_report_lacking_the_field() {
        let dir = make_repo(&[report_field_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/package.json", "{}\n");
        commit(root, "feat: add a dependency", None);
        let base = load_base(root).unwrap();
        let workspace = write_workspace(&[
            (
                "task-1-report.json",
                json!({
                    "kind": "task-report", "files_changed": ["tools/package.json"],
                    "dependency_vetting": {"manifests": ["tools/package.json"], "dependencies": []},
                }),
            ),
            (
                "task-2-report.json",
                json!({
                    "kind": "task-report", "files_changed": ["tools/package.json"],
                    "dependency_vetting": Value::Null,
                }),
            ),
        ]);
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(outcome.failed);
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!(
                "task-2-report.json lacks a value for dependency_vetting (triggered by tools/package.json)"
            )
        );
    }

    /// A lettered task id (`task-3a-report.json`, from a split task) is a
    /// report like any other: `--workspace` must judge it, not skip it.
    #[test]
    fn fails_a_workspace_report_field_check_naming_a_lettered_task_report_lacking_the_field() {
        let dir = make_repo(&[report_field_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/package.json", "{}\n");
        commit(root, "feat: add a dependency", None);
        let base = load_base(root).unwrap();
        let workspace = write_workspace(&[
            (
                "task-1-report.json",
                json!({
                    "kind": "task-report", "files_changed": ["tools/package.json"],
                    "dependency_vetting": {"manifests": ["tools/package.json"], "dependencies": []},
                }),
            ),
            (
                "task-3a-report.json",
                json!({
                    "kind": "task-report", "files_changed": ["tools/package.json"],
                    "dependency_vetting": Value::Null,
                }),
            ),
        ]);
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(outcome.failed);
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!(
                "task-3a-report.json lacks a value for dependency_vetting (triggered by tools/package.json)"
            )
        );
    }

    #[test]
    fn passes_a_workspace_report_field_check_with_the_hit_count_when_every_report_has_the_field() {
        let dir = make_repo(&[report_field_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/package.json", "{}\n");
        commit(root, "feat: add a dependency", None);
        let base = load_base(root).unwrap();
        let vetted = json!({
            "kind": "task-report", "files_changed": ["tools/package.json"],
            "dependency_vetting": {"manifests": ["tools/package.json"], "dependencies": []},
        });
        let workspace = write_workspace(&[
            ("task-1-report.json", vetted.clone()),
            ("task-2-report.json", vetted),
        ]);
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!outcome.failed);
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(
            row["evidence"],
            json!("report field dependency_vetting is set in 2 of 2 reports")
        );
    }

    #[test]
    fn passes_a_workspace_report_field_check_as_not_triggered_by_any_report() {
        let dir = make_repo(&[report_field_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/package.json", "{}\n");
        commit(root, "feat: add a dependency", None);
        let base = load_base(root).unwrap();
        let workspace = write_workspace(&[(
            "task-1-report.json",
            json!({"kind": "task-report", "files_changed": ["docs/x.md"]}),
        )]);
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!outcome.failed);
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(row["evidence"], json!("not triggered by any report"));
    }

    /// A workspace report is required to carry `files_changed`; one that
    /// lacks it is malformed, not silently "no hit".
    #[test]
    fn fails_a_workspace_report_field_check_naming_a_report_that_lacks_files_changed() {
        let dir = make_repo(&[report_field_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/package.json", "{}\n");
        commit(root, "feat: add a dependency", None);
        let base = load_base(root).unwrap();
        let workspace = write_workspace(&[("task-1-report.json", json!({"kind": "task-report"}))]);
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(outcome.failed);
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!("task-1-report.json lacks files_changed")
        );
    }

    /// A `files_changed` array holding a non-string element (`[5,
    /// "src/a.js"]`, say) is named the same "lacks files_changed" finding
    /// as a wholly-non-array `files_changed`, not silently filtered down
    /// to its string elements.
    #[test]
    fn fails_a_workspace_report_field_check_naming_a_report_whose_files_changed_holds_a_non_string_entry()
     {
        let dir = make_repo(&[report_field_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/package.json", "{}\n");
        commit(root, "feat: add a dependency", None);
        let base = load_base(root).unwrap();
        let workspace = write_workspace(&[(
            "task-1-report.json",
            json!({
                "kind": "task-report", "files_changed": [5, "tools/package.json"],
                "dependency_vetting": {"manifests": ["tools/package.json"], "dependencies": []},
            }),
        )]);
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(outcome.failed);
        let row = &printed(&outcome)["rules"][0];
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!("task-1-report.json lacks files_changed")
        );
    }

    /// A report's `files_changed` that misses the first glob in a
    /// multi-glob `if` forces `match_any` to fall through to a later,
    /// malformed one -- `unwrap_or(false)` would turn that `GlobError`
    /// into a silent "did not match" (this hit's own report excluded
    /// from `hits`, or "not triggered by any report" if it was the only
    /// one). The real git diff changes `tools/package.json`, matching
    /// the first glob, so `filter_matching` never reaches the second when
    /// resolving the trigger; the report's own `files_changed` names a
    /// path that matches neither, so matching it must compile the
    /// malformed second glob.
    #[test]
    fn a_malformed_second_if_glob_is_named_when_a_reports_files_changed_misses_the_first() {
        let dir = make_repo(&[entry(json!({
            "id": "process.reportws2",
            "summary": "Every triggered report carries dependency_vetting.",
            "check": {
                "type": "report-field", "level": "fail",
                "if": ["**/package.json", "src/[z-a].js"],
                "field": "dependency_vetting",
            },
        }))]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "tools/package.json", "{}\n");
        commit(root, "feat: add a dependency", None);
        let base = load_base(root).unwrap();
        let workspace = write_workspace(&[(
            "task-1-report.json",
            json!({"kind": "task-report", "files_changed": ["docs/unrelated.md"]}),
        )]);
        let error = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.contains("invalid glob"), "{error}");
        assert!(error.contains("src/[z-a].js"), "{error}");
    }

    /// A missing `--workspace` directory is a named error, not a raw
    /// stack trace. The CLI-level "exactly one stderr line" assertion
    /// lives in `tests/validate_stats_audit_parity.rs` (`cmd_audit`'s own
    /// boundary), since this module's tests exercise `audit()` directly,
    /// not the printed `main` dispatch.
    #[test]
    fn rejects_a_missing_workspace_directory_as_a_usage_error_not_a_stack_trace() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        let base = load_base(root).unwrap();
        let missing = tempfile::tempdir().unwrap().path().join("missing");
        let error = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                workspace: Some(missing),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(!error.is_empty());
        assert!(!error.contains('\n'));
    }

    #[test]
    fn rejects_report_together_with_workspace() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        let report_path = root.join("report.json");
        std::fs::write(&report_path, serde_json::to_string(&json!({})).unwrap()).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let base = load_base(root).unwrap();
        let error = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                report: Some(report_path),
                workspace: Some(workspace.path().to_path_buf()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(error, "audit takes --report or --workspace, not both");
    }

    // ---- the `controller` tag ----

    /// Which flag a `controller` tag test gives the audit: `--report` (a
    /// task audit), `--workspace` (the branch audit), or neither.
    #[derive(Clone, Copy)]
    enum AuditKind {
        Task,
        Branch,
        Plain,
    }

    /// Four entries tagged `controller` (standing, non-standing in
    /// `global`, non-standing in the touched `rust` area, and the same
    /// area with a check), plus three entries a task audit keeps: a
    /// standing one, a touched-area one, and a standing one whose tags
    /// only resemble `controller`. A fifth `controller` entry sits in the
    /// untouched `webview` area: it joins no package and serves the
    /// `--sanctioned` tests.
    fn controller_entries() -> Vec<Value> {
        vec![
            entry(json!({
                "id": "process.controller-standing", "tags": ["dispatch", "controller"],
                "summary": "A standing rule only the controller breaks.",
            })),
            entry(json!({
                "id": "process.controller-global", "area": "global", "standing": false,
                "tags": ["controller"], "summary": "A global rule only the controller breaks.",
            })),
            entry(json!({
                "id": "rust.controller-touched", "area": "rust", "standing": false,
                "tags": ["controller"], "summary": "An area rule only the controller breaks.",
            })),
            entry(json!({
                "id": "rust.controller-checked", "area": "rust", "standing": false,
                "tags": ["controller"], "summary": "A checked rule only the controller breaks.",
                "check": {
                    "type": "grep-absent", "level": "warn", "files": "**/*.md",
                    "pattern": "FORBIDDEN", "scope": "changed",
                },
            })),
            entry(json!({
                "id": "process.plain-standing", "summary": "A standing rule a task diff breaks.",
            })),
            entry(json!({
                "id": "rust.plain-touched", "area": "rust", "standing": false,
                "summary": "An area rule a task diff breaks.",
            })),
            entry(json!({
                "id": "process.lookalike-tags", "tags": ["controllers", "Controller", "control"],
                "summary": "A standing rule whose tags only resemble the controller tag.",
            })),
            entry(json!({
                "id": "webview.controller-untouched", "area": "webview", "standing": false,
                "tags": ["controller"],
                "summary": "An untouched-area rule only the controller breaks.",
            })),
        ]
    }

    /// The result of an audit of `controller_entries()` over a range that
    /// changes `crates/lib.rs`, run as `kind` with `ids` on `--ids` and
    /// `sanctioned` rule ids on `--sanctioned`.
    fn controller_audit(
        kind: AuditKind,
        ids: &[&str],
        sanctioned: &[&str],
    ) -> Result<AuditOutcome, String> {
        let dir = make_repo_with_files(&controller_entries(), &[("crates/lib.rs", "x\n")]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        write_file(root, "crates/lib.rs", "y\n");
        commit(root, "feat: change", None);
        let scratch = tempfile::tempdir().expect("tempdir");
        let mut opts = audit_opts(&base_sha);
        opts.ids = ids.iter().map(|id| id.to_string()).collect();
        opts.sanctioned = sanctioned
            .iter()
            .map(|rule| (rule.to_string(), "spec 3.1".to_string()))
            .collect();
        match kind {
            AuditKind::Task => {
                let report = scratch.path().join("report.json");
                std::fs::write(&report, "{}").unwrap();
                opts.report = Some(report);
            }
            AuditKind::Branch => opts.workspace = Some(scratch.path().to_path_buf()),
            AuditKind::Plain => {}
        }
        audit(&load_base(root).unwrap(), opts)
    }

    /// The sorted ids of the audit package that `controller_entries()`
    /// gives, run as `kind` with `ids` on `--ids`.
    fn controller_package(kind: AuditKind, ids: &[&str]) -> Vec<String> {
        let outcome = controller_audit(kind, ids, &[]).expect("audit");
        let mut package: Vec<String> = printed(&outcome)["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["id"].as_str().unwrap().to_string())
            .collect();
        package.sort();
        package
    }

    /// Every id of `controller_entries()` a task audit keeps.
    const TASK_PACKAGE: [&str; 3] = [
        "process.lookalike-tags",
        "process.plain-standing",
        "rust.plain-touched",
    ];

    /// The four `controller` ids of `controller_entries()`.
    const CONTROLLER_IDS: [&str; 4] = [
        "process.controller-global",
        "process.controller-standing",
        "rust.controller-checked",
        "rust.controller-touched",
    ];

    fn sorted_ids(groups: &[&[&str]]) -> Vec<String> {
        let mut ids: Vec<String> = groups
            .iter()
            .flat_map(|group| group.iter().map(|id| id.to_string()))
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn a_task_audit_leaves_a_standing_controller_entry_out_of_its_package() {
        let package = controller_package(AuditKind::Task, &[]);
        assert!(
            !package.contains(&"process.controller-standing".to_string()),
            "{package:?}"
        );
        assert_eq!(package, sorted_ids(&[&TASK_PACKAGE]));
    }

    #[test]
    fn a_task_audit_leaves_a_global_controller_entry_out() {
        let package = controller_package(AuditKind::Task, &[]);
        assert!(
            !package.contains(&"process.controller-global".to_string()),
            "{package:?}"
        );
    }

    #[test]
    fn a_task_audit_leaves_touched_area_and_checked_controller_entries_out() {
        let package = controller_package(AuditKind::Task, &[]);
        for id in ["rust.controller-touched", "rust.controller-checked"] {
            assert!(!package.contains(&id.to_string()), "{id} in {package:?}");
        }
    }

    #[test]
    fn a_task_audit_keeps_a_controller_entry_that_ids_names() {
        let package = controller_package(
            AuditKind::Task,
            &["process.controller-standing", "rust.controller-checked"],
        );
        assert_eq!(
            package,
            sorted_ids(&[
                &TASK_PACKAGE,
                &["process.controller-standing", "rust.controller-checked"]
            ])
        );
    }

    #[test]
    fn a_task_audit_keeps_lookalike_tags_that_are_not_controller_entries() {
        let package = controller_package(AuditKind::Task, &[]);
        assert!(
            package.contains(&"process.lookalike-tags".to_string()),
            "{package:?}"
        );
    }

    #[test]
    fn a_branch_audit_keeps_controller_entries() {
        assert_eq!(
            controller_package(AuditKind::Branch, &[]),
            sorted_ids(&[&TASK_PACKAGE, &CONTROLLER_IDS])
        );
    }

    #[test]
    fn a_plain_audit_keeps_controller_entries() {
        assert_eq!(
            controller_package(AuditKind::Plain, &[]),
            sorted_ids(&[&TASK_PACKAGE, &CONTROLLER_IDS])
        );
    }

    #[test]
    fn a_task_audit_names_a_sanctioned_controller_id_as_left_out_and_points_to_ids() {
        let error =
            controller_audit(AuditKind::Task, &[], &["process.controller-standing"]).unwrap_err();
        assert_eq!(
            error,
            "--sanctioned \"process.controller-standing\" is tagged controller, so a task audit \
             leaves it out of its package; add it to --ids"
        );
    }

    #[test]
    fn a_task_audit_accepts_a_sanctioned_controller_id_that_ids_names() {
        let outcome = controller_audit(
            AuditKind::Task,
            &["rust.controller-checked"],
            &["rust.controller-checked"],
        )
        .expect("audit");
        assert_eq!(
            printed(&outcome)["stale_sanctions"],
            json!([
                "--sanctioned rust.controller-checked=spec 3.1: row result is \"pass\", not fail"
            ])
        );
    }

    #[test]
    fn a_branch_audit_names_a_sanctioned_controller_id_outside_the_package_without_the_tag() {
        let error = controller_audit(AuditKind::Branch, &[], &["webview.controller-untouched"])
            .unwrap_err();
        assert_eq!(
            error,
            "--sanctioned \"webview.controller-untouched\" is not in this audit's package; \
             add it to --ids"
        );
    }

    // ---- the sanctioned-fail annotation ----

    /// A commit whose subject violates `process.commits`' pattern, with no
    /// other file change -- every OTHER deterministic check in
    /// `audit_entries()` reads "not triggered"/"0 files checked" (pass or
    /// skip) on an empty file diff, isolating `process.commits` as the
    /// one row `--sanctioned` can act on.
    fn make_repo_with_one_bad_commit_subject() -> (tempfile::TempDir, String) {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "bad subject line", None);
        (dir, base_sha)
    }

    #[test]
    fn a_sanctioned_rules_row_keeps_its_true_fail_result_and_gains_the_declaration_in_evidence() {
        let (dir, base_sha) = make_repo_with_one_bad_commit_subject();
        let base = load_base(dir.path()).unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                sanctioned: vec![("process.commits".to_string(), "spec section 6".to_string())],
                ..Default::default()
            },
        )
        .unwrap();
        let printed_outcome = printed(&outcome);
        let rows = printed_outcome["rules"].as_array().unwrap();
        let row = rows.iter().find(|r| r["id"] == "process.commits").unwrap();
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!(
                "commit \"bad subject line\" does not match ^(feat|fix|chore|docs|test): .+ (sanctioned: spec section 6)"
            )
        );
        assert_eq!(printed(&outcome)["summary"]["sanctioned_fail"], json!(1));
        assert!(printed(&outcome).get("stale_sanctions").is_none());
        assert!(outcome.failed, "a sanctioned fail still fails the run");
    }

    /// Spec 6: every sanctioned row carries the structured `sanctioned` field
    /// with the reference, beside the evidence suffix of a deterministic fail.
    #[test]
    fn a_sanctioned_deterministic_fail_carries_the_sanctioned_field() {
        let (dir, base_sha) = make_repo_with_one_bad_commit_subject();
        let base = load_base(dir.path()).unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                sanctioned: vec![("process.commits".to_string(), "spec section 6".to_string())],
                ..Default::default()
            },
        )
        .unwrap();
        let printed_outcome = printed(&outcome);
        let rows = printed_outcome["rules"].as_array().unwrap();
        let row = rows.iter().find(|r| r["id"] == "process.commits").unwrap();
        assert_eq!(row["sanctioned"], json!("spec section 6"));
    }

    /// Spec 6: a booking on a judged rule is never stale. The judged row stays
    /// `open`, carries the reference in `sanctioned`, and fails nothing, and
    /// `sanctioned_fail` counts fails only.
    #[test]
    fn sanctioned_judged_rule_is_not_stale() {
        let outcome =
            controller_audit(AuditKind::Plain, &[], &["process.plain-standing"]).expect("audit");
        let printed_outcome = printed(&outcome);
        let rows = printed_outcome["rules"].as_array().unwrap();
        let row = rows
            .iter()
            .find(|r| r["id"] == "process.plain-standing")
            .unwrap();
        assert_eq!(
            row,
            &json!({
                "id": "process.plain-standing", "kind": "rule", "mode": "judged",
                "level": null, "result": "open", "evidence": "\u{2014}",
                "sanctioned": "spec 3.1",
            })
        );
        assert!(printed(&outcome).get("stale_sanctions").is_none());
        assert!(
            printed(&outcome)["summary"]
                .get("sanctioned_fail")
                .is_none()
        );
        assert!(!outcome.failed, "a booked judged rule fails nothing");
    }

    /// A rule that RAN and passed is a stale booking, distinguished by
    /// wording from a rule the package never carried at all
    /// (`reports_an_unknown_sanctioned_id_as_a_named_error_not_a_stale_
    /// booking`).
    #[test]
    fn reports_a_stale_sanction_naming_a_rule_that_ran_and_passed() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "feat: change", None);
        let base = load_base(root).unwrap();
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                sanctioned: vec![("process.commits".to_string(), "spec section 6".to_string())],
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            printed(&outcome)["stale_sanctions"],
            json!([
                "--sanctioned process.commits=spec section 6: row result is \"pass\", not fail"
            ])
        );
        assert!(
            printed(&outcome)["summary"]
                .get("sanctioned_fail")
                .is_none()
        );
        assert!(outcome.failed, "a stale booking still fails the run");
    }

    /// A rule id the knowledge base lacks (typically a typo) is this
    /// function's own named error -- the same "unknown id" shape `--ids`
    /// already uses one flag over -- never folded into `stale_sanctions`
    /// alongside a rule that genuinely ran and passed.
    #[test]
    fn reports_an_unknown_sanctioned_id_as_a_named_error_not_a_stale_booking() {
        let (dir, base_sha) = make_repo_with_one_bad_commit_subject();
        let base = load_base(dir.path()).unwrap();
        let error = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                sanctioned: vec![(
                    "process.totally-made-up".to_string(),
                    "spec section 6".to_string(),
                )],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            "unknown id \"process.totally-made-up\" for --sanctioned"
        );
    }

    /// A rule id the knowledge base holds, but this audit's package does
    /// not carry (`webview.unrelated`: its area is untouched), is no
    /// "unknown id": the error names the cause and the way in.
    #[test]
    fn reports_a_known_sanctioned_id_outside_the_package_without_calling_it_unknown() {
        let (dir, base_sha) = make_repo_with_one_bad_commit_subject();
        let base = load_base(dir.path()).unwrap();
        let error = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                sanctioned: vec![(
                    "webview.unrelated".to_string(),
                    "spec section 6".to_string(),
                )],
                ..Default::default()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            "--sanctioned \"webview.unrelated\" is not in this audit's package; add it to --ids"
        );
    }

    #[test]
    fn omits_sanctioned_fail_and_stale_sanctions_when_no_sanction_is_given() {
        let (dir, base_sha) = make_repo_with_one_bad_commit_subject();
        let base = load_base(dir.path()).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        assert!(
            printed(&outcome)["summary"]
                .get("sanctioned_fail")
                .is_none()
        );
        assert!(printed(&outcome).get("stale_sanctions").is_none());
    }

    #[test]
    fn parse_sanctioned_rejects_a_value_missing_the_equals_separator() {
        assert_eq!(
            parse_sanctioned(vec!["process.commits".to_string()]),
            Err("--sanctioned \"process.commits\" needs the shape <rule>=<ref>".to_string())
        );
    }

    /// An empty reference books a sanction naming nothing to check
    /// against later.
    #[test]
    fn parse_sanctioned_rejects_an_empty_reference() {
        assert_eq!(
            parse_sanctioned(vec!["process.commits=".to_string()]),
            Err("--sanctioned \"process.commits=\" needs the shape <rule>=<ref>".to_string())
        );
    }

    /// A repeated rule id would let `sanctioned_fail` count past the one
    /// row it can ever annotate.
    #[test]
    fn parse_sanctioned_rejects_a_repeated_rule_id() {
        assert_eq!(
            parse_sanctioned(vec![
                "process.commits=spec 6".to_string(),
                "process.commits=spec 7".to_string(),
            ]),
            Err(
                "--sanctioned \"process.commits\" is already sanctioned; declare it once"
                    .to_string()
            )
        );
    }

    #[test]
    fn parse_sanctioned_splits_on_the_first_equals_only_so_a_reference_keeps_its_own() {
        assert_eq!(
            parse_sanctioned(vec!["process.commits=https://x.test/?a=b".to_string()]),
            Ok(vec![(
                "process.commits".to_string(),
                "https://x.test/?a=b".to_string()
            )])
        );
    }

    /// The in-process audit of a range gives the rows of `audit` and the
    /// verdict on the work-item references that `cmd_audit` adds: the
    /// `Work-Item` trailer of the range is counted as skipped in an install
    /// with no tracker, where `audit` alone leaves `references` empty.
    #[test]
    fn audit_result_gives_the_rows_of_audit_and_the_verdict_on_the_references() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        let base_sha = git(root, &["rev-parse", "HEAD"]).trim().to_string();
        write_file(root, "docs/x.md", "text\n");
        commit(root, "docs: change", Some("Work-Item: HR-1"));
        let base = load_base(root).expect("load base");

        let outcome = audit(&base, audit_opts(&base_sha)).expect("audit");
        let result = audit_result(&base, &base_sha, "HEAD").expect("audit result");

        assert_eq!(result.rules, outcome.result.rules);
        assert_eq!(result.changed_files, ["docs/x.md"]);
        assert_eq!(outcome.result.references, References::default());
        assert_eq!(result.references.skipped, 1);
        assert_eq!(result.references.checked, 0);
        assert!(result.references.failures.is_empty());
        assert_eq!(result.summary.reference_fail, 0);
    }

    /// The name of git's empty tree in the repository `root`.
    fn empty_tree_of(root: &Path) -> String {
        git(root, &["hash-object", "-t", "tree", "--stdin"])
            .trim()
            .to_string()
    }

    /// The range from git's empty tree is the whole history: its audit
    /// reads every path of the head and every commit that the head reaches.
    #[test]
    fn audit_result_of_the_empty_tree_audits_every_path_and_every_commit_of_the_head() {
        let dir = make_repo(&audit_entries());
        let root = dir.path();
        write_file(root, "docs/x.md", "text\n");
        commit(root, "docs: change", Some("Work-Item: HR-1"));
        write_file(root, "docs/y.md", "text\n");
        commit(root, "docs: more", Some("Work-Item: HR-2"));
        let base = load_base(root).expect("load base");
        let empty = empty_tree_of(root);

        let result = audit_result(&base, &empty, "HEAD").expect("audit result");

        let mut tracked: Vec<String> = git(root, &["ls-tree", "-r", "--name-only", "HEAD"])
            .lines()
            .map(str::to_string)
            .collect();
        tracked.sort();
        let mut audited = result.changed_files.clone();
        audited.sort();
        assert_eq!(audited, tracked);
        assert_eq!(result.base, empty);
        assert_eq!(result.summary.base, empty);
        assert_eq!(result.references.skipped, 2);
        assert!(result.summary.empty_range.is_none());
    }

    /// `audit` itself stays as it was: the empty tree is no ref for `--base`.
    #[test]
    fn audit_refuses_the_empty_tree_as_a_base_ref() {
        let dir = make_repo(&audit_entries());
        let base = load_base(dir.path()).expect("load base");
        let empty = empty_tree_of(dir.path());

        assert_eq!(
            audit(&base, audit_opts(&empty)).err(),
            Some(format!("bad ref \"{empty}\""))
        );
    }

    #[test]
    fn audit_result_names_a_ref_that_does_not_resolve() {
        let dir = make_repo(&audit_entries());
        let base = load_base(dir.path()).expect("load base");

        assert_eq!(
            audit_result(&base, "no-such-ref", "HEAD").unwrap_err(),
            "bad ref \"no-such-ref\""
        );
    }
}
