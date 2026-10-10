//! The `rules` and `proposals` keys of `stats`: how each knowledge entry
//! fared across the workspaces, and which entries the numbers say to
//! change.
//!
//! A `RuleRow` exists for every entry of the knowledge base whose raw
//! `status` is absent or `active`. An id that a deliverable names but the
//! base does not hold gets no row and no proposal; it still counts in
//! `violations` and `unused_ids`. The thresholds below are fixed. A
//! rule that prevents violations by its presence also shows zero fails,
//! so every proposal that touches a standing entry carries
//! `owner_gate: true`.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::cost::{SeverityCounts, TargetCounts, Workspace};
use super::model::{Base, CheckField, raw_entry_has_tag};

/// A standing entry audited in this many workspaces or more, with no
/// fail and no finding, earns a `demote` proposal.
const DEMOTE_MIN_WORKSPACES: usize = 3;

/// A non-standing, never-cited entry injected into this many workspaces
/// or more, with no fail and no finding, earns a `retire` proposal.
const RETIRE_MIN_INJECTIONS: usize = 3;

/// A judged entry that failed or drew a finding in this many workspaces
/// or more earns a `mechanize` proposal.
const MECHANIZE_MIN_WORKSPACES: usize = 2;

/// An entry with this many findings or more can earn a `narrow`
/// proposal.
const NARROW_MIN_FINDINGS: usize = 3;

/// The numerator of the share of soft findings (minor, or aimed at a
/// deliverable) that earns a `narrow` proposal: two thirds.
const NARROW_SOFT_NUMERATOR: usize = 2;

/// The denominator of the share of soft findings that earns a `narrow`
/// proposal: two thirds.
const NARROW_SOFT_DENOMINATOR: usize = 3;

/// The advisory budget of standing entries. More than this many earns
/// one `budget` proposal. `check-knowledge` keeps its own hard limit on
/// the generated standing file.
const STANDING_BUDGET: usize = 25;

/// Whether an entry carries a valid `check` that `audit` runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Deterministic,
    Judged,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Mode::Deterministic => "deterministic",
            Mode::Judged => "judged",
        }
    }
}

/// How one entry was used across the workspaces.
#[derive(Clone, Debug, Default)]
pub(super) struct Usage {
    /// Workspaces where an audit's `rules` or a review's
    /// `rule_adherence` holds the id, in any mode.
    pub workspaces: usize,
    /// Workspaces where an audit's `ids` holds the id.
    pub injected: usize,
    /// Counted rows with the id: the deterministic rows of the audit
    /// files and the judged rows of the review files, each row once.
    pub rows: usize,
    /// Those rows whose `result` is `fail`.
    pub fails: usize,
    /// Findings whose `rule` is the id, by severity.
    pub findings: SeverityCounts,
    /// The same findings, by target.
    pub targets: TargetCounts,
    /// The findings that are minor or aimed at a deliverable.
    pub soft_findings: usize,
    /// Workspaces with a failed row or a finding for the id.
    pub flagged_workspaces: usize,
    /// Reports whose `knowledge_used` names the id.
    pub cited: usize,
}

impl Usage {
    /// Failed rows plus findings: what the entry caught.
    fn caught(&self) -> usize {
        self.fails + self.findings.total()
    }
}

/// The tag that records the owner's decision to keep an entry as it is:
/// the entry gets no `demote`, `retire`, `narrow`, or `mechanize`
/// proposal (design.md 5.93).
const RULED_KEEP_TAG: &str = "ruled-keep";

/// One row of the `rules` key.
#[derive(Clone, Debug)]
pub(super) struct RuleRow {
    pub id: String,
    pub standing: bool,
    /// `true` when the entry's `tags` hold `ruled-keep`. Such an entry gets
    /// only the `budget` row's count: it counts toward the standing
    /// budget and is left out of the row's `candidates`, because the
    /// owner has already ruled on it.
    pub ruled_keep: bool,
    pub mode: Mode,
    /// `true` for a standing entry and for an entry whose `source.by` is
    /// `user`: only the owner changes it.
    pub owner_gate: bool,
    pub usage: Usage,
}

/// Counts how each id was used across `workspaces`. The map holds ids
/// the knowledge base may not know; `rule_rows` looks up only the ones it
/// does.
fn tally(workspaces: &[Workspace]) -> BTreeMap<&str, Usage> {
    let mut usage: BTreeMap<&str, Usage> = BTreeMap::new();
    for workspace in workspaces {
        let mut audited: BTreeSet<&str> = BTreeSet::new();
        let mut flagged: BTreeSet<&str> = BTreeSet::new();
        for row in &workspace.rows {
            let counts = usage.entry(row.id.as_str()).or_default();
            audited.insert(row.id.as_str());
            if row.counted {
                counts.rows += 1;
                if row.failed {
                    counts.fails += 1;
                    flagged.insert(row.id.as_str());
                }
            }
        }
        for finding in &workspace.findings {
            let Some(id) = finding.rule.as_deref() else {
                continue;
            };
            let counts = usage.entry(id).or_default();
            counts.findings.add(finding.severity);
            counts.targets.add(finding.target);
            if finding.is_soft() {
                counts.soft_findings += 1;
            }
            flagged.insert(id);
        }
        for id in &workspace.injected {
            usage.entry(id.as_str()).or_default().injected += 1;
        }
        for id in &workspace.cited {
            usage.entry(id.as_str()).or_default().cited += 1;
        }
        for id in audited {
            usage.entry(id).or_default().workspaces += 1;
        }
        for id in flagged {
            usage.entry(id).or_default().flagged_workspaces += 1;
        }
    }
    usage
}

/// `true` when the raw entry `id` has no `status`, or `status: active`.
fn is_active(base: &Base, id: &str) -> bool {
    match base.raw_entries.get(id).and_then(|raw| raw.get("status")) {
        None | Some(Value::Null) => true,
        Some(status) => status.as_str() == Some("active"),
    }
}

/// `true` when the raw entry `id` names `user` in `source.by`.
fn is_user_sourced(base: &Base, id: &str) -> bool {
    base.raw_entries
        .get(id)
        .and_then(|raw| raw.get("source"))
        .and_then(|source| source.get("by"))
        .and_then(Value::as_str)
        == Some("user")
}

/// One row per active entry of `base`, sorted by id, with its usage
/// across `workspaces`.
pub(super) fn rule_rows(workspaces: &[Workspace], base: &Base) -> Vec<RuleRow> {
    let mut usage = tally(workspaces);
    let mut rows: Vec<RuleRow> = base
        .entries
        .values()
        .filter(|entry| is_active(base, &entry.id))
        .map(|entry| RuleRow {
            id: entry.id.clone(),
            standing: entry.standing,
            ruled_keep: raw_entry_has_tag(base, &entry.id, RULED_KEEP_TAG),
            mode: match entry.check {
                CheckField::Valid(_) => Mode::Deterministic,
                CheckField::Absent | CheckField::Malformed => Mode::Judged,
            },
            owner_gate: entry.standing || is_user_sourced(base, &entry.id),
            usage: usage.remove(entry.id.as_str()).unwrap_or_default(),
        })
        .collect();
    rows.sort_by(|left, right| left.id.cmp(&right.id));
    rows
}

/// The `rules` key: one object per row, in the order given.
pub(super) fn rules_json(rows: &[RuleRow]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| {
                json!({
                    "id": row.id,
                    "standing": row.standing,
                    "mode": row.mode.as_str(),
                    "workspaces": row.usage.workspaces,
                    "injected": row.usage.injected,
                    "rows": row.usage.rows,
                    "fails": row.usage.fails,
                    "findings": row.usage.findings.to_json(),
                    "targets": row.usage.targets.to_json(),
                    "cited": row.usage.cited,
                })
            })
            .collect(),
    )
}

/// What a proposal asks for. The derived order is the table order of the
/// output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Action {
    Demote,
    Retire,
    Mechanize,
    Narrow,
    Budget,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Demote => "demote",
            Action::Retire => "retire",
            Action::Mechanize => "mechanize",
            Action::Narrow => "narrow",
            Action::Budget => "budget",
        }
    }
}

/// One row of the `proposals` key.
struct Proposal {
    action: Action,
    id: String,
    owner_gate: bool,
    reason: String,
    evidence: Value,
}

impl Proposal {
    fn to_json(&self) -> Value {
        json!({
            "action": self.action.as_str(),
            "id": self.id,
            "owner_gate": self.owner_gate,
            "reason": self.reason,
            "evidence": self.evidence,
        })
    }
}

/// A proposal about `row`, gated as the row is.
fn about(row: &RuleRow, action: Action, reason: String, evidence: Value) -> Proposal {
    Proposal {
        action,
        id: row.id.clone(),
        owner_gate: row.owner_gate,
        reason,
        evidence,
    }
}

/// A standing entry audited clean in enough workspaces.
fn demote(row: &RuleRow) -> Option<Proposal> {
    let usage = &row.usage;
    (row.standing
        && usage.workspaces >= DEMOTE_MIN_WORKSPACES
        && usage.fails == 0
        && usage.findings.total() == 0)
        .then(|| {
            about(
                row,
                Action::Demote,
                format!(
                    "The rule is standing. {} workspaces audited it. \
                     No row failed and no finding cited it.",
                    usage.workspaces
                ),
                json!({"workspaces": usage.workspaces, "fails": 0, "findings": 0}),
            )
        })
}

/// A non-standing entry injected often, clean, and never cited.
fn retire(row: &RuleRow) -> Option<Proposal> {
    let usage = &row.usage;
    (!row.standing
        && usage.injected >= RETIRE_MIN_INJECTIONS
        && usage.fails == 0
        && usage.findings.total() == 0
        && usage.cited == 0)
        .then(|| {
            about(
                row,
                Action::Retire,
                format!(
                    "The rule is not standing. {} workspaces injected it. \
                     No report cited it. No row failed and no finding cited it.",
                    usage.injected
                ),
                json!({"injected": usage.injected, "fails": 0, "findings": 0, "cited": 0}),
            )
        })
}

/// A judged entry that keeps failing or drawing findings.
fn mechanize(row: &RuleRow) -> Option<Proposal> {
    let usage = &row.usage;
    (row.mode == Mode::Judged && usage.flagged_workspaces >= MECHANIZE_MIN_WORKSPACES).then(|| {
        about(
            row,
            Action::Mechanize,
            format!(
                "This judged rule failed or drew a finding in {} workspaces. \
                 Add a deterministic check.",
                usage.flagged_workspaces
            ),
            json!({
                "flagged_workspaces": usage.flagged_workspaces,
                "fails": usage.fails,
                "findings": usage.findings.total(),
            }),
        )
    })
}

/// An entry whose findings are mostly minor or aimed at deliverables.
/// Findings on prose count as soft only when they are minor.
fn narrow(row: &RuleRow) -> Option<Proposal> {
    let usage = &row.usage;
    let findings = usage.findings.total();
    (findings >= NARROW_MIN_FINDINGS
        && usage.soft_findings * NARROW_SOFT_DENOMINATOR >= findings * NARROW_SOFT_NUMERATOR)
        .then(|| {
            about(
                row,
                Action::Narrow,
                format!(
                    "{} of {} findings are minor or hit a deliverable. \
                     Narrow the rule to contract surfaces.",
                    usage.soft_findings, findings
                ),
                json!({"findings": findings, "soft_findings": usage.soft_findings}),
            )
        })
}

/// Orders two rows by ascending catch rate -- `caught / max(rows, 1)`,
/// compared by cross-multiplication -- then by id.
fn by_catch_rate(left: &RuleRow, right: &RuleRow) -> Ordering {
    let left_rows = left.usage.rows.max(1);
    let right_rows = right.usage.rows.max(1);
    (left.usage.caught() * right_rows)
        .cmp(&(right.usage.caught() * left_rows))
        .then_with(|| left.id.cmp(&right.id))
}

/// One proposal over the standing set when it exceeds the budget. It is
/// always owner-gated.
///
/// Every standing entry counts toward the budget, a `ruled-keep` entry
/// included: `standing` is the full count and `ruled_keep` is how many of
/// them carry the tag. `candidates` lists the others by ascending catch
/// rate and leaves the `ruled-keep` entries out, because the owner has
/// already ruled to keep them.
fn budget(rows: &[RuleRow]) -> Option<Proposal> {
    let standing: Vec<&RuleRow> = rows.iter().filter(|row| row.standing).collect();
    if standing.len() <= STANDING_BUDGET {
        return None;
    }
    let ruled_keep = standing.iter().filter(|row| row.ruled_keep).count();
    let mut open: Vec<&RuleRow> = standing
        .iter()
        .copied()
        .filter(|row| !row.ruled_keep)
        .collect();
    open.sort_by(|left, right| by_catch_rate(left, right));
    let candidates: Vec<&str> = open.iter().map(|row| row.id.as_str()).collect();
    Some(Proposal {
        action: Action::Budget,
        id: "standing".to_string(),
        owner_gate: true,
        reason: format!(
            "The base holds {} standing entries. The budget is {STANDING_BUDGET}. \
             Entries tagged {RULED_KEEP_TAG}: {ruled_keep}. \
             Demote the others that catch the least.",
            standing.len()
        ),
        evidence: json!({
            "standing": standing.len(),
            "ruled_keep": ruled_keep,
            "budget": STANDING_BUDGET,
            "candidates": candidates,
        }),
    })
}

/// The `proposals` key: every proposal the thresholds earn, sorted by
/// action in table order, then by id. One entry may earn several. A
/// `ruled-keep` entry earns none of the four per-entry actions.
pub(super) fn proposals(rows: &[RuleRow]) -> Vec<Value> {
    let mut proposals: Vec<Proposal> = rows
        .iter()
        .filter(|row| !row.ruled_keep)
        .flat_map(|row| [demote(row), retire(row), mechanize(row), narrow(row)])
        .flatten()
        .collect();
    proposals.extend(budget(rows));
    proposals.sort_by(|left, right| {
        left.action
            .cmp(&right.action)
            .then_with(|| left.id.cmp(&right.id))
    });
    proposals.iter().map(Proposal::to_json).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::rules::cost::{AdherenceRow, Finding, Severity, Target, Workspace};
    use crate::rules::test_support::{base_with, entry, valid_check};

    fn adherence(id: &str, failed: bool) -> AdherenceRow {
        AdherenceRow {
            id: id.to_string(),
            failed,
            counted: true,
        }
    }

    /// A row that is a copy: it names the id but is not counted.
    fn copied_row(id: &str, failed: bool) -> AdherenceRow {
        AdherenceRow {
            counted: false,
            ..adherence(id, failed)
        }
    }

    fn issue(rule: &str, severity: Severity, target: Target) -> Finding {
        Finding {
            severity,
            target,
            rule: Some(rule.to_string()),
        }
    }

    /// A rule row with no use at all: judged, not standing, not gated.
    fn blank(id: &str) -> RuleRow {
        RuleRow {
            id: id.to_string(),
            standing: false,
            ruled_keep: false,
            mode: Mode::Judged,
            owner_gate: false,
            usage: Usage::default(),
        }
    }

    fn standing(id: &str) -> RuleRow {
        RuleRow {
            standing: true,
            owner_gate: true,
            ..blank(id)
        }
    }

    /// The same row, tagged `ruled-keep`.
    fn kept(row: RuleRow) -> RuleRow {
        RuleRow {
            ruled_keep: true,
            ..row
        }
    }

    fn with_usage(mut row: RuleRow, edit: impl FnOnce(&mut Usage)) -> RuleRow {
        edit(&mut row.usage);
        row
    }

    fn actions(rows: &[RuleRow]) -> Vec<(String, String)> {
        proposals(rows)
            .iter()
            .map(|row| {
                (
                    row["action"].as_str().expect("action").to_string(),
                    row["id"].as_str().expect("id").to_string(),
                )
            })
            .collect()
    }

    fn pair(action: &str, id: &str) -> (String, String) {
        (action.to_string(), id.to_string())
    }

    /// One workspace that audited `a.one` (pass) and `b.two` (fail), with
    /// findings that cite `a.one` and an id the base does not hold.
    fn first_workspace() -> Workspace {
        Workspace {
            path: "ws1".to_string(),
            rows: vec![
                adherence("a.one", false),
                adherence("b.two", true),
                adherence("ghost.id", true),
            ],
            findings: vec![
                issue("a.one", Severity::Critical, Target::Code),
                issue("a.one", Severity::Minor, Target::Prose),
                issue("ghost.id", Severity::Minor, Target::Prose),
                Finding {
                    severity: Severity::Minor,
                    target: Target::Code,
                    rule: None,
                },
            ],
            injected: ["a.one", "b.two", "ghost.id"]
                .into_iter()
                .map(String::from)
                .collect(),
            cited: vec!["a.one".to_string()],
            ..Workspace::default()
        }
    }

    fn second_workspace() -> Workspace {
        Workspace {
            path: "ws2".to_string(),
            rows: vec![adherence("a.one", false), adherence("a.one", false)],
            findings: vec![issue("a.one", Severity::Important, Target::Deliverable)],
            injected: ["a.one"].into_iter().map(String::from).collect(),
            cited: vec!["a.one".to_string(), "b.two".to_string()],
            ..Workspace::default()
        }
    }

    fn sample_base_entries() -> Vec<Value> {
        vec![
            entry("b.two", json!({"standing": true, "check": valid_check()})),
            entry(
                "a.one",
                json!({"source": {"date": "2026-10-09", "by": "user"}}),
            ),
            entry("c.gone", json!({"status": "retired"})),
            entry("d.live", json!({"status": "active"})),
            entry("e.old", json!({"status": "superseded"})),
        ]
    }

    /// One row per entry whose status is absent or `active`, sorted by
    /// id; a retired or superseded entry and an id the base does not hold
    /// get none.
    #[test]
    fn rule_rows_list_the_active_entries_by_id() {
        let (_guard, base) = base_with(&sample_base_entries());
        let rows = rule_rows(&[first_workspace(), second_workspace()], &base);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, ["a.one", "b.two", "d.live"]);
    }

    /// The mode follows the check: a valid check is deterministic, none is
    /// judged. The standing flag follows the entry.
    #[test]
    fn rule_rows_carry_the_mode_and_the_standing_flag() {
        let (_guard, base) = base_with(&sample_base_entries());
        let rows = rule_rows(&[], &base);
        let by_id = |id: &str| rows.iter().find(|row| row.id == id).expect("row");
        assert_eq!(by_id("b.two").mode, Mode::Deterministic);
        assert!(by_id("b.two").standing);
        assert_eq!(by_id("a.one").mode, Mode::Judged);
        assert!(!by_id("a.one").standing);
    }

    /// A malformed `check` is not a valid check: the row is judged.
    #[test]
    fn rule_rows_treat_a_malformed_check_as_judged() {
        let (_guard, base) = base_with(&[entry("a.one", json!({"check": {"type": "no-such"}}))]);
        assert_eq!(rule_rows(&[], &base)[0].mode, Mode::Judged);
    }

    /// An uncounted row puts its workspace among the entry's `workspaces`
    /// but adds to neither `rows` nor `fails`, and flags no workspace.
    #[test]
    fn an_uncounted_row_counts_for_workspaces_only() {
        let (_guard, base) = base_with(&[entry("a.one", json!({}))]);
        let workspace = Workspace {
            path: "ws1".to_string(),
            rows: vec![copied_row("a.one", true), adherence("a.one", false)],
            ..Workspace::default()
        };
        let rows = rule_rows(&[workspace], &base);
        assert_eq!(rows[0].usage.workspaces, 1);
        assert_eq!(rows[0].usage.rows, 1);
        assert_eq!(rows[0].usage.fails, 0);
        assert_eq!(rows[0].usage.flagged_workspaces, 0);
    }

    /// Every counter of the `rules` row, measured over two workspaces.
    #[test]
    fn rule_rows_count_workspaces_injections_rows_fails_findings_and_citations() {
        let (_guard, base) = base_with(&sample_base_entries());
        let rows = rule_rows(&[first_workspace(), second_workspace()], &base);
        let a_one = &rows[0];
        assert_eq!(a_one.usage.workspaces, 2);
        assert_eq!(a_one.usage.injected, 2);
        assert_eq!(a_one.usage.rows, 3);
        assert_eq!(a_one.usage.fails, 0);
        assert_eq!(a_one.usage.cited, 2);
        assert_eq!(a_one.usage.findings.total(), 3);
        assert_eq!(a_one.usage.soft_findings, 2);
        assert_eq!(a_one.usage.flagged_workspaces, 2);
        let b_two = &rows[1];
        assert_eq!(b_two.usage.workspaces, 1);
        assert_eq!(b_two.usage.injected, 1);
        assert_eq!(b_two.usage.rows, 1);
        assert_eq!(b_two.usage.fails, 1);
        assert_eq!(b_two.usage.cited, 1);
        assert_eq!(b_two.usage.findings.total(), 0);
        assert_eq!(b_two.usage.flagged_workspaces, 1);
        let d_live = &rows[2];
        assert_eq!(d_live.usage.workspaces, 0);
        assert_eq!(d_live.usage.cited, 0);
    }

    /// The `rules` JSON row: these keys in this order, findings and
    /// targets split by class.
    #[test]
    fn rules_json_rows_carry_the_spec_keys_in_order() {
        let (_guard, base) = base_with(&sample_base_entries());
        let rows = rule_rows(&[first_workspace(), second_workspace()], &base);
        let json = rules_json(&rows);
        assert_eq!(
            json[0],
            json!({
                "id": "a.one", "standing": false, "mode": "judged",
                "workspaces": 2, "injected": 2, "rows": 3, "fails": 0,
                "findings": {"critical": 1, "important": 1, "minor": 1, "other": 0},
                "targets": {"code": 1, "deliverable": 1, "prose": 1, "other": 0},
                "cited": 2,
            })
        );
        let keys: Vec<&str> = json[0]
            .as_object()
            .expect("row object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "id",
                "standing",
                "mode",
                "workspaces",
                "injected",
                "rows",
                "fails",
                "findings",
                "targets",
                "cited"
            ]
        );
    }

    /// `owner_gate` is true for a standing entry and for an entry whose
    /// `source.by` is `user`; false otherwise.
    #[test]
    fn owner_gate_is_true_for_standing_and_user_sourced_entries_only() {
        let (_guard, base) = base_with(&sample_base_entries());
        let rows = rule_rows(&[], &base);
        let gate = |id: &str| {
            rows.iter()
                .find(|row| row.id == id)
                .expect("row")
                .owner_gate
        };
        assert!(gate("b.two"), "standing");
        assert!(gate("a.one"), "source.by user");
        assert!(!gate("d.live"), "neither");
    }

    /// An entry with no `source` at all is not gated by source.
    #[test]
    fn owner_gate_is_false_for_an_entry_with_no_source() {
        let (_guard, base) = base_with(&[json!({"id": "a.one", "standing": false})]);
        assert!(!rule_rows(&[], &base)[0].owner_gate);
    }

    /// `demote`: standing, in 3 or more workspaces, no fail, no finding.
    #[test]
    fn demote_fires_at_three_workspaces_and_not_one_below() {
        let at = with_usage(standing("s.one"), |usage| usage.workspaces = 3);
        let below = with_usage(standing("s.one"), |usage| usage.workspaces = 2);
        assert_eq!(actions(&[at]), [pair("demote", "s.one")]);
        assert_eq!(actions(&[below]), []);
    }

    /// A fail, a finding, or a missing standing flag stops `demote`.
    #[test]
    fn demote_needs_a_clean_standing_entry() {
        let failed = with_usage(standing("s.one"), |usage| {
            usage.workspaces = 3;
            usage.fails = 1;
        });
        let found = with_usage(standing("s.one"), |usage| {
            usage.workspaces = 3;
            usage.findings.minor = 1;
        });
        let not_standing = with_usage(blank("s.one"), |usage| usage.workspaces = 3);
        assert_eq!(actions(&[failed]), []);
        assert_eq!(actions(&[found]), []);
        assert_eq!(actions(&[not_standing]), []);
    }

    /// The `demote` row: the owner gate of a standing entry, a reason in
    /// plain sentences, and the numbers the condition read.
    #[test]
    fn demote_row_carries_the_gate_reason_and_evidence() {
        let row = with_usage(standing("s.one"), |usage| usage.workspaces = 3);
        let rows = proposals(&[row]);
        assert_eq!(
            rows[0],
            json!({
                "action": "demote", "id": "s.one", "owner_gate": true,
                "reason": "The rule is standing. 3 workspaces audited it. \
                           No row failed and no finding cited it.",
                "evidence": {"workspaces": 3, "fails": 0, "findings": 0},
            })
        );
    }

    /// `retire`: not standing, injected in 3 or more workspaces, no fail,
    /// no finding, never cited.
    #[test]
    fn retire_fires_at_three_injections_and_not_one_below() {
        let at = with_usage(blank("r.one"), |usage| usage.injected = 3);
        let below = with_usage(blank("r.one"), |usage| usage.injected = 2);
        assert_eq!(actions(&[at]), [pair("retire", "r.one")]);
        assert_eq!(actions(&[below]), []);
    }

    /// A citation, a fail, a finding, or a standing flag stops `retire`.
    #[test]
    fn retire_needs_an_uncited_clean_non_standing_entry() {
        let cited = with_usage(blank("r.one"), |usage| {
            usage.injected = 3;
            usage.cited = 1;
        });
        let failed = with_usage(blank("r.one"), |usage| {
            usage.injected = 3;
            usage.fails = 1;
        });
        let found = with_usage(blank("r.one"), |usage| {
            usage.injected = 3;
            usage.findings.other = 1;
        });
        let kept = with_usage(standing("r.one"), |usage| usage.injected = 3);
        for row in [cited, failed, found, kept] {
            assert_eq!(actions(&[row]), []);
        }
    }

    /// The `retire` row: a user-sourced entry is owner-gated.
    #[test]
    fn retire_row_carries_the_gate_and_evidence() {
        let mut row = with_usage(blank("r.one"), |usage| usage.injected = 4);
        row.owner_gate = true;
        let rows = proposals(&[row]);
        assert_eq!(rows[0]["owner_gate"], json!(true));
        assert_eq!(
            rows[0]["evidence"],
            json!({"injected": 4, "fails": 0, "findings": 0, "cited": 0})
        );
        assert!(
            rows[0]["reason"]
                .as_str()
                .expect("reason")
                .contains("4 workspaces")
        );
    }

    /// `mechanize`: a judged entry with a fail or a finding in 2 or more
    /// workspaces.
    #[test]
    fn mechanize_fires_at_two_flagged_workspaces_and_not_one_below() {
        let at = with_usage(blank("m.one"), |usage| {
            usage.flagged_workspaces = 2;
            usage.fails = 2;
        });
        let below = with_usage(blank("m.one"), |usage| {
            usage.flagged_workspaces = 1;
            usage.fails = 5;
        });
        assert_eq!(actions(&[at]), [pair("mechanize", "m.one")]);
        assert_eq!(actions(&[below]), []);
    }

    /// A deterministic entry already has its check.
    #[test]
    fn mechanize_skips_a_deterministic_entry() {
        let mut row = with_usage(blank("m.one"), |usage| usage.flagged_workspaces = 3);
        row.mode = Mode::Deterministic;
        assert_eq!(actions(&[row]), []);
    }

    /// The `mechanize` evidence.
    #[test]
    fn mechanize_row_carries_the_evidence() {
        let row = with_usage(blank("m.one"), |usage| {
            usage.flagged_workspaces = 2;
            usage.fails = 1;
            usage.findings.minor = 2;
        });
        assert_eq!(
            proposals(&[row])[0]["evidence"],
            json!({"flagged_workspaces": 2, "fails": 1, "findings": 2})
        );
    }

    /// `narrow`: 3 or more findings, at least two thirds of them soft.
    #[test]
    fn narrow_fires_at_two_thirds_soft_and_not_one_below() {
        let narrow_row = |findings: usize, soft: usize| {
            with_usage(blank("n.one"), |usage| {
                usage.findings.minor = findings;
                usage.soft_findings = soft;
            })
        };
        assert_eq!(actions(&[narrow_row(3, 2)]), [pair("narrow", "n.one")]);
        assert_eq!(actions(&[narrow_row(6, 4)]), [pair("narrow", "n.one")]);
        assert_eq!(actions(&[narrow_row(3, 1)]), []);
        assert_eq!(actions(&[narrow_row(6, 3)]), []);
    }

    /// Two findings are below the count threshold, however soft.
    #[test]
    fn narrow_needs_three_findings() {
        let row = with_usage(blank("n.one"), |usage| {
            usage.findings.minor = 2;
            usage.soft_findings = 2;
        });
        assert_eq!(actions(&[row]), []);
    }

    /// The `narrow` evidence.
    #[test]
    fn narrow_row_carries_the_evidence() {
        let row = with_usage(blank("n.one"), |usage| {
            usage.findings.minor = 3;
            usage.soft_findings = 3;
        });
        assert_eq!(
            proposals(&[row])[0]["evidence"],
            json!({"findings": 3, "soft_findings": 3})
        );
    }

    /// The `narrow` reason names the soft set as minor or deliverable.
    #[test]
    fn narrow_reason_names_minor_and_deliverable_only() {
        let row = with_usage(blank("n.one"), |usage| {
            usage.findings.minor = 3;
            usage.soft_findings = 2;
        });
        assert_eq!(
            proposals(&[row])[0]["reason"],
            json!(
                "2 of 3 findings are minor or hit a deliverable. \
                 Narrow the rule to contract surfaces."
            )
        );
    }

    /// A finding on prose is soft only when it is minor: three important
    /// findings on prose give no `narrow`, however many there are; a
    /// minor finding on prose and an important one on a deliverable are
    /// soft.
    #[test]
    fn narrow_does_not_count_a_prose_finding_as_soft() {
        let (_guard, base) = base_with(&[entry("a.one", json!({}))]);
        let prose_only = Workspace {
            findings: vec![
                issue("a.one", Severity::Important, Target::Prose),
                issue("a.one", Severity::Important, Target::Prose),
                issue("a.one", Severity::Critical, Target::Prose),
            ],
            ..Workspace::default()
        };
        let rows = rule_rows(&[prose_only], &base);
        assert_eq!(rows[0].usage.soft_findings, 0);
        assert_eq!(actions(&rows), []);

        let mixed = Workspace {
            findings: vec![
                issue("a.one", Severity::Minor, Target::Prose),
                issue("a.one", Severity::Important, Target::Deliverable),
                issue("a.one", Severity::Important, Target::Prose),
            ],
            ..Workspace::default()
        };
        let rows = rule_rows(&[mixed], &base);
        assert_eq!(rows[0].usage.soft_findings, 2);
        assert_eq!(actions(&rows), [pair("narrow", "a.one")]);
    }

    /// An entry tagged `ruled-keep` gets no `demote`, `retire`, `mechanize`,
    /// or `narrow` proposal. Each row below meets its threshold exactly, so
    /// the untagged control fires and the tagged twin does not.
    #[test]
    fn a_ruled_keep_entry_earns_none_of_the_four_per_entry_actions() {
        let demote_row = with_usage(standing("k.demote"), |usage| usage.workspaces = 3);
        let retire_row = with_usage(blank("k.retire"), |usage| usage.injected = 3);
        let mechanize_row = with_usage(blank("k.mechanize"), |usage| usage.flagged_workspaces = 2);
        let narrow_row = with_usage(blank("k.narrow"), |usage| {
            usage.findings.minor = 3;
            usage.soft_findings = 3;
        });
        for (action, row) in [
            ("demote", demote_row),
            ("retire", retire_row),
            ("mechanize", mechanize_row),
            ("narrow", narrow_row),
        ] {
            let id = row.id.clone();
            assert_eq!(
                actions(std::slice::from_ref(&row)),
                [pair(action, &id)],
                "{action} control"
            );
            assert_eq!(actions(&[kept(row)]), [], "{action} ruled keep");
        }
    }

    /// A ruled-keep entry leaves the other entries' proposals alone.
    #[test]
    fn a_ruled_keep_entry_does_not_silence_its_neighbours() {
        let rows = [
            kept(with_usage(standing("a.kept"), |usage| usage.workspaces = 3)),
            with_usage(standing("b.open"), |usage| usage.workspaces = 3),
        ];
        assert_eq!(actions(&rows), [pair("demote", "b.open")]);
    }

    /// Only the exact tag `ruled-keep`, in an array, marks an entry.
    #[test]
    fn rule_rows_read_the_ruled_keep_tag_exactly() {
        let (_guard, base) = base_with(&[
            entry("a.kept", json!({"tags": ["knowledge", "ruled-keep"]})),
            entry("b.alone", json!({"tags": ["ruled-keep"]})),
            entry(
                "c.near",
                json!({"tags": ["ruled-keep-not", "ruled_keep", "Ruled-Keep"]}),
            ),
            entry("d.string", json!({"tags": "ruled-keep"})),
            entry("e.empty", json!({"tags": []})),
            entry("f.none", json!({})),
        ]);
        let rows = rule_rows(&[], &base);
        let flags: Vec<(&str, bool)> = rows
            .iter()
            .map(|row| (row.id.as_str(), row.ruled_keep))
            .collect();
        assert_eq!(
            flags,
            [
                ("a.kept", true),
                ("b.alone", true),
                ("c.near", false),
                ("d.string", false),
                ("e.empty", false),
                ("f.none", false),
            ]
        );
    }

    /// A ruled-keep entry counts toward the standing budget but is left
    /// out of the `candidates`: `standing` is the full count, `ruled_keep`
    /// says how many of them carry the tag.
    #[test]
    fn budget_counts_ruled_keep_entries_and_leaves_them_out_of_the_candidates() {
        let mut rows = standing_rows(26);
        for row in &mut rows[..3] {
            row.ruled_keep = true;
        }
        let rows = proposals(&rows);
        assert_eq!(rows.len(), 1);
        let evidence = &rows[0]["evidence"];
        assert_eq!(evidence["standing"], json!(26));
        assert_eq!(evidence["ruled_keep"], json!(3));
        assert_eq!(evidence["budget"], json!(25));
        let candidates: Vec<&str> = evidence["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .map(|id| id.as_str().expect("id"))
            .collect();
        assert_eq!(candidates.len(), 23);
        assert_eq!(&candidates[..2], ["s.04", "s.05"]);
        assert!(!candidates.contains(&"s.01") && !candidates.contains(&"s.03"));
        assert_eq!(
            rows[0]["reason"],
            json!(
                "The base holds 26 standing entries. The budget is 25. \
                 Entries tagged ruled-keep: 3. Demote the others that catch the least."
            )
        );
    }

    /// The reason reads right for one tagged entry too: a figure, not a
    /// verb that must agree with it.
    #[test]
    fn budget_reason_gives_the_ruled_keep_count_as_a_figure() {
        let mut rows = standing_rows(26);
        rows[0].ruled_keep = true;
        assert_eq!(
            proposals(&rows)[0]["reason"],
            json!(
                "The base holds 26 standing entries. The budget is 25. \
                 Entries tagged ruled-keep: 1. Demote the others that catch the least."
            )
        );
    }

    /// The count decides whether `budget` fires: 25 standing entries are
    /// within the budget however many are ruled keep, and 26 are over it
    /// even when every one is ruled keep (then no candidate remains).
    #[test]
    fn budget_fires_on_the_full_standing_count_whatever_is_ruled_keep() {
        let mut within = standing_rows(25);
        for row in &mut within[..10] {
            row.ruled_keep = true;
        }
        assert_eq!(actions(&within), []);

        let mut over = standing_rows(26);
        for row in &mut over {
            row.ruled_keep = true;
        }
        let rows = proposals(&over);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["action"], json!("budget"));
        assert_eq!(rows[0]["evidence"]["standing"], json!(26));
        assert_eq!(rows[0]["evidence"]["ruled_keep"], json!(26));
        assert_eq!(rows[0]["evidence"]["candidates"], json!([]));
    }

    fn standing_rows(count: usize) -> Vec<RuleRow> {
        (1..=count)
            .map(|n| standing(&format!("s.{n:02}")))
            .collect()
    }

    /// `budget`: more than 25 standing entries. Exactly 25 is within it.
    #[test]
    fn budget_fires_above_twenty_five_standing_entries_only() {
        assert_eq!(actions(&standing_rows(25)), []);
        let over = actions(&standing_rows(26));
        assert_eq!(over, [pair("budget", "standing")]);
    }

    /// A non-standing entry does not count toward the budget.
    #[test]
    fn budget_counts_standing_entries_only() {
        let mut rows = standing_rows(25);
        rows.push(blank("x.extra"));
        assert_eq!(actions(&rows), []);
    }

    /// The `budget` row is always owner-gated and lists the standing ids
    /// by ascending catch rate, then by id.
    #[test]
    fn budget_row_lists_candidates_by_ascending_catch_rate_then_id() {
        let mut rows = standing_rows(26);
        rows[0].usage.rows = 1;
        rows[0].usage.fails = 1;
        rows[2].usage.rows = 10;
        rows[2].usage.fails = 1;
        rows[4].usage.rows = 4;
        rows[4].usage.findings.minor = 2;
        for row in &mut rows {
            row.owner_gate = false;
        }
        let rows = proposals(&rows);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["owner_gate"], json!(true));
        let evidence = &rows[0]["evidence"];
        assert_eq!(evidence["standing"], json!(26));
        assert_eq!(evidence["budget"], json!(25));
        let candidates: Vec<&str> = evidence["candidates"]
            .as_array()
            .expect("candidates")
            .iter()
            .map(|id| id.as_str().expect("id"))
            .collect();
        assert_eq!(candidates.len(), 26);
        assert_eq!(&candidates[..3], ["s.02", "s.04", "s.06"]);
        assert_eq!(&candidates[23..], ["s.03", "s.05", "s.01"]);
    }

    /// A row whose rows count is 0 divides by 1, so a finding still ranks
    /// it above a silent entry.
    #[test]
    fn budget_catch_rate_divides_by_at_least_one_row() {
        let mut rows = standing_rows(26);
        rows[0].usage.findings.minor = 1;
        let rows = proposals(&rows);
        let last = rows[0]["evidence"]["candidates"]
            .as_array()
            .and_then(|ids| ids.last())
            .and_then(Value::as_str)
            .map(String::from);
        assert_eq!(last, Some("s.01".to_string()));
    }

    /// Rows come out by action in table order, then by id; one entry may
    /// carry several actions.
    #[test]
    fn proposals_sort_by_action_then_id_and_an_entry_may_have_several() {
        let both = with_usage(blank("b.both"), |usage| {
            usage.flagged_workspaces = 2;
            usage.findings.minor = 3;
            usage.soft_findings = 3;
        });
        let demoted = with_usage(standing("z.demote"), |usage| usage.workspaces = 3);
        let retired = with_usage(blank("a.retire"), |usage| usage.injected = 3);
        let mut rows = standing_rows(26);
        rows.extend([both, retired, demoted]);
        assert_eq!(
            actions(&rows),
            [
                pair("demote", "z.demote"),
                pair("retire", "a.retire"),
                pair("mechanize", "b.both"),
                pair("narrow", "b.both"),
                pair("budget", "standing"),
            ]
        );
    }
}
