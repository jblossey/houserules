//! The `evidence` document: the reviewer packet of a change as data (spec
//! 5.9). The Markdown packet that `houserules evidence` prints holds the same
//! content, cut to fit a comment; the document holds it whole.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Published;
use super::change::UncoveredPath;
use super::intent::Intent;
use super::route::RouteDocument;

/// The evidence of a change over the range of commits from `base` to `head`:
/// what no covering check read, the intent, the rules that failed, warned, and
/// held, the size, the work items, the sessions, and the route.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[schemars(title = "Evidence", transform = super::leave_sessions_optional)]
pub(crate) struct Evidence {
    /// The schema id, `houserules/evidence@1.1.0`.
    pub(crate) schema: String,
    /// The path of the install from the git top level; `.` for a top-level
    /// install.
    pub(crate) install: String,
    /// The full sha of the commit that `--base` names, or the name of git's
    /// empty tree for a range from the start of the history.
    pub(crate) base: String,
    /// The full sha of the head commit of the range.
    pub(crate) head: String,
    /// The full sha of the merge base of `base` and `head`; `null` for a range
    /// from the empty tree.
    pub(crate) merge_base: Option<String>,
    /// The intents of the change, in the order of the adapters, then by `ref`.
    /// A named intent that does not resolve is an entry with its `source` and
    /// `ref`, `null` for every other field of the contract (`artifacts` is
    /// empty), and the key `unresolved`: the line of its adapter. An intent
    /// that resolves carries no `unresolved` key.
    pub(crate) intent: Vec<Intent>,
    /// The rule package that judged the change.
    pub(crate) rules: Rules,
    /// The size of the diff.
    pub(crate) diff: Diff,
    /// The part of the change that no covering check read.
    pub(crate) uncovered: Uncovered,
    /// The deterministic rules with the result `fail`, sorted by id. A booked
    /// fail keeps the text `(sanctioned: <ref>)` in its evidence.
    pub(crate) failed: Vec<RuleRow>,
    /// The deterministic rules with the result `warn`, sorted by id.
    pub(crate) warned: Vec<RuleRow>,
    /// The deterministic rules with the result `pass`, sorted by id.
    pub(crate) proven: Vec<RuleRow>,
    /// The rules that have no check, sorted by id, each with the verdict of the
    /// review file.
    pub(crate) judged: Vec<JudgedRow>,
    /// The deterministic rules with the result `skipped`: the checks that did
    /// not apply to the change, sorted by id.
    pub(crate) skipped: Vec<RuleRow>,
    /// The `Work-Item` ids of the range, sorted.
    pub(crate) work_items: Vec<String>,
    /// The sessions that the commits of the range cite in a trailer whose key
    /// `review.sessions.trailers` lists, oldest commit first, and in the order of
    /// the message inside a commit.
    pub(crate) sessions: Vec<Session>,
    /// The route decision for the same range.
    pub(crate) route: RouteDocument,
}

impl Published for Evidence {
    const NAME: &'static str = "evidence";
    const VERSION: &'static str = "1.1.0";
}

/// A session of an agent tool that a commit cites in a trailer, such as
/// `Entire-Checkpoint: <id>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Session {
    /// The full sha of the commit that holds the trailer.
    pub(crate) commit: String,
    /// The key of the trailer, as the commit wrote it.
    pub(crate) trailer: String,
    /// The value of the trailer: the reference to the session.
    pub(crate) value: String,
}

/// The rule package that judged a change: the hashes of its sources at the two
/// ends of the range and the packs that the install pins at the head.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(crate) struct Rules {
    /// The SHA-256 of the rule sources at the merge base; `null` when the merge
    /// base holds none.
    pub(crate) merge_base_hash: Option<String>,
    /// The SHA-256 of the rule sources at the head.
    pub(crate) head_hash: String,
    /// The packs that the marker of the install pins at the head, read from
    /// git, sorted by name.
    pub(crate) packs: Vec<PackPin>,
}

/// A knowledge pack that the install pins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PackPin {
    /// The name of the pack.
    pub(crate) name: String,
    /// The version of the pack.
    pub(crate) version: String,
    /// The SHA-256 of the copy of the pack in the install.
    pub(crate) sha256: String,
}

/// The size of the diff of the range, without the paths that
/// `review.diffExclude` leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Diff {
    /// The paths that count.
    pub(crate) files: u64,
    /// The lines added.
    pub(crate) added: u64,
    /// The lines deleted.
    pub(crate) deleted: u64,
    /// The lines added and deleted.
    pub(crate) lines: u64,
    /// The paths that `review.diffExclude` leaves out.
    pub(crate) excluded: u64,
}

/// The part of a change that no covering check read: a path is covered when a
/// check that declares `covers: true` read it and its row passed or warned.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(crate) struct Uncovered {
    /// The changed lines of the uncovered paths.
    pub(crate) lines: u64,
    /// The changed lines of all paths.
    pub(crate) total_lines: u64,
    /// `lines` over `total_lines`, from 0 to 1; 0 when there is no changed
    /// line.
    pub(crate) share: f64,
    /// The uncovered paths, sorted by path.
    pub(crate) paths: Vec<UncoveredPath>,
}

/// The row of a deterministic rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct RuleRow {
    /// The id of the knowledge entry.
    pub(crate) id: String,
    /// What the check found, as the audit result gives it.
    pub(crate) evidence: String,
}

/// The row of a rule that has no check, with the verdict of the review file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct JudgedRow {
    /// The id of the knowledge entry.
    pub(crate) id: String,
    /// The result of the `rule_adherence` row of the review file for the id,
    /// or `open` for any result other than `pass`, `fail`, and `warn`. `null`
    /// when there is no review file or it has no row for the id.
    pub(crate) verdict: Option<Verdict>,
    /// The evidence of that row; `null` when there is no row or it has none.
    pub(crate) evidence: Option<String>,
}

/// The verdict of a review on a judged rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Verdict {
    /// The review found the rule kept.
    Pass,
    /// The review found the rule broken.
    Fail,
    /// The review found a fault that fails nothing.
    Warn,
    /// The review gave no verdict: its row was skipped or holds another result.
    Open,
}

impl Verdict {
    /// The word that the packet prints for the verdict.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Verdict::Pass => "pass",
            Verdict::Fail => "fail",
            Verdict::Warn => "warn",
            Verdict::Open => "open",
        }
    }
}
