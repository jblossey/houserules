//! The `change-record` document: what a merge leaves as a git note (spec 8).
//! `houserules record write` computes it from git, the audit, and the inputs
//! of the host; `houserules record show` reads it back.
//!
//! The document has a full form and, for a record over its size budget, forms
//! that a cut shortens (spec 8.4). A cut turns a list into a count or a
//! summary, so a field that a cut shortens has one type for each form, and
//! the form decides which one the document holds. The smallest form, the
//! minimal record, leaves out the keys that hold the packet: `commits`,
//! `evidence`, `bundle`, and `links`.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::Published;
use super::audit_result::{References, RowMode, RowResult, Summary};
use super::change::{ChangeKind, EmptyChange, LineRange};
use super::evidence::{Diff, JudgedRow, PackPin};
use super::intent::{Requirements, Tasks, Validation};
use super::route::{Reason, Route};

/// The record of one merge: the range that landed, the rules it was judged
/// under, the audit, the intent, the evidence, the route, the sample draw, the
/// bundle of the request, the reviewers, and the post-merge audit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(title = "Change record")]
pub(crate) struct ChangeRecord {
    /// The schema id, `houserules/change-record@1.0.0`.
    pub(crate) schema: String,
    /// The path of the install from the git top level; `.` for a top-level
    /// install.
    pub(crate) install: String,
    /// The full sha of the commit that the note annotates: the last commit of
    /// the push.
    pub(crate) key: String,
    /// The full sha of the commit that `--base` names, or the name of git's
    /// empty tree for a range from the start of the history.
    pub(crate) base: String,
    /// The full sha of the head commit of the range.
    pub(crate) head: String,
    /// The full sha of the merge base of `base` and `head`; `null` for a range
    /// from the empty tree.
    pub(crate) merge_base: Option<String>,
    /// The commits of the range, oldest first, or after a cut their number,
    /// the first, and the last. The minimal record has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Commits")]
    pub(crate) commits: Option<Commits>,
    /// The writer of the record: `houserules <version>`.
    pub(crate) written_by: String,
    /// The host of the request, or `null` for a record written by hand.
    pub(crate) host: Option<Host>,
    /// The rule package that judged the range.
    pub(crate) rules: Rules,
    /// What the audit of the range found.
    pub(crate) audit: Audit,
    /// The intents of the change, as the packet lists them.
    pub(crate) intent: Intents,
    /// The evidence document of the change without its `rules`, `intent`, and
    /// `route`. The minimal record has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "RecordEvidence")]
    pub(crate) evidence: Option<RecordEvidence>,
    /// The route decision, computed at merge over the range that landed.
    pub(crate) route: RecordRoute,
    /// The post-merge sample draw; `null` for a route other than
    /// `auto-merge-eligible`.
    pub(crate) sample: Option<Sample>,
    /// The evidence bundle of the request, parsed; `null` for a push with no
    /// request or a description with no bundle. The minimal record has no such
    /// key.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_present",
        deserialize_with = "deserialize_present"
    )]
    #[schemars(with = "Option<RecordBundle>")]
    pub(crate) bundle: Option<Option<RecordBundle>>,
    /// The approvers, sorted, without repeats, at most 20, each cut to 100
    /// characters. Empty while `reviewers_pending` is `true`.
    pub(crate) reviewers: Vec<String>,
    /// The number of approvers past the 20 of `reviewers`. The key is absent
    /// when there is none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub(crate) reviewers_more: Option<u64>,
    /// `true` when the record holds no reviewers yet: a later job has not set
    /// them (spec 10.4), or the approvers could not be read and stay unknown
    /// (spec 10.1 and 10.4).
    pub(crate) reviewers_pending: bool,
    /// The work items of the range. The minimal record has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Links")]
    pub(crate) links: Option<Links>,
    /// The verdict of the audit after the merge; `null` until `record audit`
    /// sets it.
    pub(crate) post_merge_audit: Option<PostMergeAudit>,
    /// The fields that the size budget shortened, in the order of the cuts
    /// (spec 8.4); `["all"]` for the minimal record.
    pub(crate) truncated: Vec<String>,
}

impl Published for ChangeRecord {
    const NAME: &'static str = "change-record";
    const VERSION: &'static str = "1.0.0";
}

/// Writes the inner value of a key that is present. The `skip_serializing_if`
/// of the field leaves an absent key out, so a present key holds the inner
/// value, `null` included.
fn serialize_present<S, T>(value: &Option<Option<T>>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    match value {
        Some(inner) => inner.serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// Reads a key that is present as `Some`, whether it holds `null` or a value:
/// `serde` reads `null` as `None` for an `Option` field, so only a key that is
/// absent (the field's `default`) stays `None`.
fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// The host of the request that a record came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Host {
    /// GitHub or a self-hosted GitHub.
    Github,
    /// GitLab or a self-managed GitLab.
    Gitlab,
}

/// The commits of a range, in the form the budget left them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum Commits {
    /// Every commit of the range, oldest first.
    List(Vec<String>),
    /// The range after a cut.
    Summary(CommitsSummary),
}

/// A range of commits cut to its ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct CommitsSummary {
    /// The number of commits of the range.
    pub(crate) count: u64,
    /// The first commit, the oldest.
    pub(crate) first: String,
    /// The last commit, the newest.
    pub(crate) last: String,
}

/// The rule package that judged a range (spec 8.2): the hashes of the rule
/// sources at both ends, and the packs that the install pins at the head.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Rules {
    /// The SHA-256 of the rule sources at the merge base; `null` when the merge
    /// base holds none.
    pub(crate) merge_base_hash: Option<String>,
    /// `present` when the merge base holds rule sources, `absent` when it holds
    /// none: the first request of a fresh install, or a range from the empty
    /// tree.
    #[schemars(extend("enum" = ["present", "absent"]))]
    pub(crate) merge_base_rules: String,
    /// The SHA-256 of the rule sources at the head.
    pub(crate) head_hash: String,
    /// The rules that judged the range: the rules of the head.
    pub(crate) audited_with: AuditedWith,
    /// The packs that the marker of the install pins at the head, sorted by
    /// name. The minimal record lists none.
    pub(crate) packs: Vec<PackPin>,
}

/// The commit whose rules judged a range. Today it is the head only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AuditedWith {
    /// The rules of the head commit.
    Head,
}

/// What the audit of the range found.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Audit {
    /// The counts of the rows, as `audit --json` gives them.
    pub(crate) summary: Summary,
    /// The id, mode, and result of each row. The key is absent after a cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<AuditedRow>")]
    pub(crate) rules: Option<Vec<AuditedRow>>,
    /// The verdict on the work-item references of the range, as `audit --json`
    /// gives it: the counts, and one line for each failure. The minimal record
    /// has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "References")]
    pub(crate) references: Option<References>,
    /// The line of an audit that could not run, or `null`. The minimal record
    /// has no such key.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_present",
        deserialize_with = "deserialize_present"
    )]
    #[schemars(with = "Option<String>")]
    pub(crate) error: Option<Option<String>>,
}

/// One row of the audit as the record keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct AuditedRow {
    /// The id of the knowledge entry.
    pub(crate) id: String,
    /// Whether the audit ran a check for the rule.
    pub(crate) mode: RowMode,
    /// The result of the row; `open` for a judged row.
    pub(crate) result: RowResult,
}

/// The intents of a change in the form the budget left them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum Intents {
    /// Each intent with every field of the contract of spec 4.9.
    Full(Vec<RecordIntent>),
    /// The minimal record: the source and the ref of each intent.
    Refs(Vec<IntentName>),
}

/// An intent as the record keeps it: the contract of spec 4.9.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordIntent {
    /// The name of the adapter, such as `work-item` or `file`.
    pub(crate) source: String,
    /// The id that the adapter gives the intent.
    #[serde(rename = "ref")]
    pub(crate) reference: String,
    /// The title, or `null`.
    pub(crate) title: Option<String>,
    /// Why the change exists, or `null`.
    pub(crate) why: Option<String>,
    /// The counts of the task list, or `null`.
    pub(crate) tasks: Option<Tasks>,
    /// The counts of the requirements, or `null`.
    pub(crate) requirements: Option<Requirements>,
    /// The result of a spec tool's validation, or `null`.
    pub(crate) validation: Option<Validation>,
    /// The paths that the adapter read, or after a cut their number.
    pub(crate) artifacts: Artifacts,
    /// The line of the adapter for a named intent that does not resolve. The
    /// key is absent for an intent that resolves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub(crate) unresolved: Option<String>,
}

/// The name of an intent: the only fields that the minimal record keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct IntentName {
    /// The name of the adapter.
    pub(crate) source: String,
    /// The id that the adapter gives the intent, at most 150 bytes once
    /// serialized.
    #[serde(rename = "ref")]
    pub(crate) reference: String,
}

/// The paths that an adapter read, in the form the budget left them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum Artifacts {
    /// The paths.
    List(Vec<String>),
    /// The number of paths, after a cut.
    Count(u64),
}

/// The evidence of the change: the document of spec 5.9 without its `rules`,
/// `intent`, and `route`, which the record holds on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordEvidence {
    /// The schema id of the evidence document, `houserules/evidence@1.0.0`.
    pub(crate) schema: String,
    /// The path of the install from the git top level.
    pub(crate) install: String,
    /// The base of the range.
    pub(crate) base: String,
    /// The head of the range.
    pub(crate) head: String,
    /// The merge base of the range, or `null`.
    pub(crate) merge_base: Option<String>,
    /// The size of the diff.
    pub(crate) diff: Diff,
    /// The part of the change that no covering check read.
    pub(crate) uncovered: RecordUncovered,
    /// The deterministic rules with the result `fail`, sorted by id.
    pub(crate) failed: Vec<RecordRuleRow>,
    /// The deterministic rules with the result `warn`, sorted by id.
    pub(crate) warned: Vec<RecordRuleRow>,
    /// The deterministic rules with the result `pass`, sorted by id. After a
    /// cut a row keeps its id alone.
    pub(crate) proven: Vec<RecordRuleRow>,
    /// The rules that have no check, with the verdict of a review. After a cut
    /// the evidence of a row is `null`.
    pub(crate) judged: Vec<JudgedRow>,
    /// The deterministic rules that did not apply to the change, sorted by id.
    pub(crate) skipped: Vec<RecordRuleRow>,
    /// The `Work-Item` ids of the range, sorted; the first 50 after a cut.
    pub(crate) work_items: Vec<String>,
    /// The number of ids past the listed ones. The key is absent when none was
    /// cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub(crate) work_items_more: Option<u64>,
}

/// The part of a change that no covering check read, in the form the budget
/// left it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordUncovered {
    /// The changed lines of the uncovered paths.
    pub(crate) lines: u64,
    /// The changed lines of all paths.
    pub(crate) total_lines: u64,
    /// `lines` over `total_lines`, from 0 to 1.
    pub(crate) share: f64,
    /// The uncovered paths, or after a cut their number for each kind.
    pub(crate) paths: UncoveredPaths,
}

/// The uncovered paths in the form the budget left them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum UncoveredPaths {
    /// The paths, sorted by path.
    List(Vec<RecordUncoveredPath>),
    /// The number of paths of each kind, in the order of the kinds of spec
    /// 4.2; a kind with no path is left out.
    #[schemars(extend("propertyNames" = {"enum": [
        "text", "deleted", "renamed", "empty", "binary", "mode", "symlink", "submodule"
    ]}))]
    Counts(IndexMap<String, u64>),
}

/// A changed path that no covering check read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordUncoveredPath {
    /// The path from the install root. A deleted path is its old path.
    pub(crate) path: String,
    /// The path before a rename, or `null`.
    pub(crate) old_path: Option<String>,
    /// What happened to the path.
    pub(crate) kind: ChangeKind,
    /// The lines that the hunks of the path touch, or after a cut their number.
    pub(crate) ranges: Ranges,
    /// For the kind `empty` only: whether the range adds or deletes the file.
    /// The key is absent for every other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "EmptyChange")]
    pub(crate) empty: Option<EmptyChange>,
}

/// The lines that the hunks of a path touch, in the form the budget left them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum Ranges {
    /// One range for each hunk, in the order of the diff.
    List(Vec<LineRange>),
    /// The lines of all hunks: the lines added and the lines deleted.
    Count(u64),
}

/// The row of a deterministic rule as the record keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordRuleRow {
    /// The id of the knowledge entry.
    pub(crate) id: String,
    /// What the check found. The key is absent after a cut of a passing row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub(crate) evidence: Option<String>,
}

/// The route decision of the range in the form the budget left it: the
/// document of spec 6.4, or after the last cut its route and floor flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordRoute {
    /// The schema id of the route document, `houserules/route@1.0.0`. The
    /// minimal record has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub(crate) schema: Option<String>,
    /// The install path. The minimal record has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub(crate) install: Option<String>,
    /// The head of the range. The minimal record has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub(crate) head: Option<String>,
    /// The route of the change: the highest route of the reasons.
    pub(crate) route: Route,
    /// `true` for the tool-free floor, which reads no audit row.
    pub(crate) floor: bool,
    /// What the rules read. The minimal record has no such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "RecordInputs")]
    pub(crate) inputs: Option<RecordInputs>,
    /// One reason for each rule that was evaluated. The minimal record has no
    /// such key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Vec<Reason>")]
    pub(crate) reasons: Option<Vec<Reason>>,
}

/// The inputs of a route decision in the form the budget left them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordInputs {
    /// The ids of the failed rows that no booking accepted, sorted.
    pub(crate) failed: Vec<String>,
    /// The ids of the warned rows, sorted.
    pub(crate) warned: Vec<String>,
    /// The areas that hold a changed path, each with its paths or their number.
    pub(crate) areas: IndexMap<String, PathList>,
    /// The changed paths in no area, or their number.
    pub(crate) no_area: PathList,
    /// The entries of `review.route.paths` that match a changed path.
    pub(crate) paths: Vec<RecordPathInput>,
    /// The lines added and deleted, without the excluded paths.
    pub(crate) diff_lines: u64,
    /// The share of the changed lines that no covering check read; `null` in a
    /// floor.
    pub(crate) uncovered_share: Option<f64>,
    /// The external score, or `null`.
    pub(crate) score: Option<f64>,
    /// The changed rule sources, or their number.
    pub(crate) rule_sources_changed: PathList,
}

/// One entry of `review.route.paths` that matches a changed path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordPathInput {
    /// The glob of the entry.
    pub(crate) glob: String,
    /// The route of the entry.
    pub(crate) route: Route,
    /// The changed paths that the glob matches, or their number.
    pub(crate) paths: PathList,
}

/// A list of paths in the form the budget left it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub(crate) enum PathList {
    /// The paths, sorted.
    List(Vec<String>),
    /// The number of paths, after a cut.
    Count(u64),
}

/// The post-merge sample draw of a record (spec 8.5).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Sample {
    /// The `review.route.sampleRate` at the draw.
    pub(crate) rate: f64,
    /// Whether the draw on the key is below the rate: the record is sampled.
    pub(crate) sampled: bool,
}

/// The evidence bundle of the request, parsed (spec 7.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordBundle {
    /// The text of the `Purpose:` line.
    pub(crate) purpose: String,
    /// The text of the `Intent:` line, or `null`.
    pub(crate) intent: Option<String>,
    /// The work items of the `Work items:` line; the first 50 after a cut.
    pub(crate) work_items: Vec<String>,
    /// The number of work items past the listed ones. The key is absent when
    /// none was cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub(crate) work_items_more: Option<u64>,
    /// The text of the `Risk tier:` line.
    pub(crate) tier: String,
    /// The text of the `Criterion:` line.
    pub(crate) criterion: String,
    /// The text of the `Budget reason:` line.
    pub(crate) budget_reason: String,
    /// The verification runs of the bundle; the first 20 after a cut.
    pub(crate) runs: Vec<RecordRun>,
    /// The number of runs past the listed ones. The key is absent when none was
    /// cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub(crate) runs_more: Option<u64>,
}

/// One verification run of the bundle: the facts of its capture header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct RecordRun {
    /// The command and its arguments; empty for a header that does not parse.
    pub(crate) argv: Vec<String>,
    /// The exit code of the run, or `null`.
    pub(crate) exit: Option<i32>,
    /// The full sha of `HEAD` when the run started, or `null`.
    pub(crate) head: Option<String>,
    /// The SHA-256 of the header line.
    pub(crate) sha256: String,
}

/// The work items of the range in the form the budget left them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Links {
    /// The `Work-Item` ids of the range, sorted; the first 50 after a cut.
    pub(crate) work_items: Vec<String>,
    /// The number of ids past the listed ones. The key is absent when none was
    /// cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "u64")]
    pub(crate) work_items_more: Option<u64>,
}

/// The audit of a merged change that a person ran after the merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PostMergeAudit {
    /// The verdict of the audit.
    pub(crate) verdict: AuditVerdict,
    /// Who audited; at most 100 bytes.
    pub(crate) by: String,
    /// A note on the audit, or `null`; at most 1,000 bytes.
    pub(crate) note: Option<String>,
}

/// The verdict of a post-merge audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AuditVerdict {
    /// The merged change is sound.
    Pass,
    /// The merged change is not sound.
    Fail,
}
