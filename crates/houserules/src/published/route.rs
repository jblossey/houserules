//! The `route` document and its parts: `Route`, how closely a change gets
//! reviewed, and the decision that `houserules route` writes (spec 6.4).

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Number;

use super::Published;

/// How closely a change gets reviewed. The variants are ordered from the
/// lowest route to the highest, so `Ord` picks the stricter of two routes.
/// Serialized, a route is its kebab-case name.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Route {
    /// No review is required before the change merges.
    AutoMergeEligible,
    /// The project owner reviews the change.
    OwnerReview,
    /// A senior reviewer reviews the change.
    SeniorReview,
}

impl Route {
    /// The kebab-case name of the route, as the document and the
    /// configuration write it.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Route::AutoMergeEligible => "auto-merge-eligible",
            Route::OwnerReview => "owner-review",
            Route::SeniorReview => "senior-review",
        }
    }
}

/// The decision of `houserules route` for a range of commits: the route, the
/// inputs that the rules read, and a reason for each rule that was evaluated.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[schemars(title = "Route decision")]
pub(crate) struct RouteDocument {
    /// The schema id, `houserules/route@1.0.0`.
    pub(crate) schema: String,
    /// The path of the install from the git top level; `.` for a top-level
    /// install.
    pub(crate) install: String,
    /// The full sha of the head commit of the range.
    pub(crate) head: String,
    /// The route of the change: the highest route of the reasons.
    pub(crate) route: Route,
    /// `true` for the tool-free floor that `--no-audit` computes. The floor
    /// reads no audit row, so it has no failed or warned row and no uncovered
    /// share.
    pub(crate) floor: bool,
    /// What the rules read. Every input appears whether or not it raised the
    /// route.
    pub(crate) inputs: Inputs,
    /// One reason for each rule of spec 6.2 that was evaluated, one that found
    /// nothing included, and one for each limit or threshold that the change
    /// stays within. A rule that found nothing has one reason with the value 0
    /// at the route `auto-merge-eligible`. A floor reads no audit row, so it
    /// has no reason for failed rows and warned rows. The score has reasons
    /// only with a `--score`, and the default route only for a change with no
    /// changed path and no changed rule source. They come in the order of the
    /// rules: rule sources, failed rows, warned rows, areas, paths in no area,
    /// path entries, score, default.
    pub(crate) reasons: Vec<Reason>,
}

impl Published for RouteDocument {
    const NAME: &'static str = "route";
    const VERSION: &'static str = "1.0.0";
}

/// The inputs of a route decision.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(crate) struct Inputs {
    /// The ids of the deterministic rows with the result `fail` that no
    /// `--sanctioned` booking accepted, sorted. Empty in a floor.
    pub(crate) failed: Vec<String>,
    /// The ids of the deterministic rows with the result `warn`, sorted. Empty
    /// in a floor.
    pub(crate) warned: Vec<String>,
    /// The areas that hold a changed path, sorted by name, each with its
    /// changed paths. A deleted path counts by its old path, and a renamed
    /// path by both.
    pub(crate) areas: IndexMap<String, Vec<String>>,
    /// The changed paths, old paths included, that match no area, sorted.
    pub(crate) no_area: Vec<String>,
    /// The entries of `review.route.paths` whose glob matches a changed path,
    /// in the order of the configuration.
    pub(crate) paths: Vec<PathInput>,
    /// The lines added and deleted, without the paths that
    /// `review.diffExclude` leaves out.
    pub(crate) diff_lines: u64,
    /// The share of the changed lines that no covering check read, from 0 to
    /// 1; `null` in a floor.
    pub(crate) uncovered_share: Option<f64>,
    /// The external score of `--score`, or `null`.
    pub(crate) score: Option<f64>,
    /// The changed rule sources, sorted: paths of the install's `knowledge/`
    /// folder and `.houserules.json` from the install root, and CI files from
    /// the git top level.
    pub(crate) rule_sources_changed: Vec<String>,
}

/// One entry of `review.route.paths` that matches a changed path.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub(crate) struct PathInput {
    /// The glob of the entry.
    pub(crate) glob: String,
    /// The route of the entry.
    pub(crate) route: Route,
    /// The changed paths that the glob matches, old paths included, sorted.
    pub(crate) paths: Vec<String>,
}

/// The rule that a reason belongs to. A rule with a limit has one reason for
/// its route and one for each limit. A rule that found nothing has one reason
/// that counts 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ReasonRule {
    /// A rule source changed: the knowledge folder, the marker of the install,
    /// or a CI file of the project.
    RuleSources,
    /// A deterministic row failed and no booking accepted it.
    Failed,
    /// A deterministic row warned.
    Warned,
    /// The route of an area that holds a changed path.
    Area,
    /// The `maxDiffLines` limit of an area at `auto-merge-eligible`.
    AreaDiff,
    /// The `maxUncoveredShare` limit of an area at `auto-merge-eligible`.
    AreaShare,
    /// Changed paths that match no area.
    NoArea,
    /// The route of an entry of `review.route.paths` whose glob matches a
    /// changed path.
    Path,
    /// The `maxDiffLines` limit of a path entry at `auto-merge-eligible`.
    PathDiff,
    /// The `maxUncoveredShare` limit of a path entry at `auto-merge-eligible`.
    PathShare,
    /// The score against `seniorScore`.
    ScoreSenior,
    /// The score against `ownerScore`.
    ScoreOwner,
    /// A change with no changed path takes the default route.
    Default,
}

/// One reason of a route decision: what a rule read and the route that it
/// gave. A rule that found nothing, and a limit or a threshold that the change
/// stays within, give `auto-merge-eligible`, the lowest route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Reason {
    /// The rule that the reason belongs to.
    pub(crate) rule: ReasonRule,
    /// What the rule read: the id of a row, the name of an area, a glob, or
    /// the key of `inputs` for the other rules and for a rule that found
    /// nothing.
    pub(crate) input: String,
    /// The value that the rule compared with its limit or threshold, or the
    /// number of paths of a rule that names paths, 0 for a rule that found
    /// nothing; `null` for a rule with none of these.
    pub(crate) value: Option<Number>,
    /// The limit or the threshold, or `null` for a rule that has none.
    pub(crate) threshold: Option<Number>,
    /// The route that the rule gave.
    pub(crate) route: Route,
    /// The first 20 paths of a rule that names paths, sorted.
    pub(crate) paths: Vec<String>,
    /// The number of paths beyond the 20 of `paths`.
    pub(crate) more_paths: u64,
    /// `true` for a reason at the final route.
    pub(crate) decisive: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_serializes_as_its_kebab_case_name() {
        for route in [
            Route::AutoMergeEligible,
            Route::OwnerReview,
            Route::SeniorReview,
        ] {
            let text = serde_json::to_string(&route).expect("serialize");
            assert_eq!(text, format!("\"{}\"", route.as_str()));
            let back: Route = serde_json::from_str(&text).expect("deserialize");
            assert_eq!(back, route);
        }
        assert_eq!(Route::AutoMergeEligible.as_str(), "auto-merge-eligible");
        assert_eq!(Route::OwnerReview.as_str(), "owner-review");
        assert_eq!(Route::SeniorReview.as_str(), "senior-review");
    }

    #[test]
    fn the_routes_order_from_the_lowest_to_the_highest() {
        assert!(Route::AutoMergeEligible < Route::OwnerReview);
        assert!(Route::OwnerReview < Route::SeniorReview);
        assert_eq!(
            Route::AutoMergeEligible.max(Route::SeniorReview),
            Route::SeniorReview
        );
    }

    #[test]
    fn a_reason_rule_serializes_as_its_kebab_case_name() {
        for (rule, name) in [
            (ReasonRule::RuleSources, "rule-sources"),
            (ReasonRule::Failed, "failed"),
            (ReasonRule::Warned, "warned"),
            (ReasonRule::Area, "area"),
            (ReasonRule::AreaDiff, "area-diff"),
            (ReasonRule::AreaShare, "area-share"),
            (ReasonRule::NoArea, "no-area"),
            (ReasonRule::Path, "path"),
            (ReasonRule::PathDiff, "path-diff"),
            (ReasonRule::PathShare, "path-share"),
            (ReasonRule::ScoreSenior, "score-senior"),
            (ReasonRule::ScoreOwner, "score-owner"),
            (ReasonRule::Default, "default"),
        ] {
            assert_eq!(
                serde_json::to_string(&rule).expect("serialize"),
                format!("\"{name}\"")
            );
        }
    }

    #[test]
    fn a_reason_keeps_its_eight_keys_in_order_with_null_for_no_value() {
        let reason = Reason {
            rule: ReasonRule::Failed,
            input: "team.rule".to_string(),
            value: None,
            threshold: None,
            route: Route::OwnerReview,
            paths: Vec::new(),
            more_paths: 0,
            decisive: true,
        };

        assert_eq!(
            serde_json::to_string(&reason).expect("serialize"),
            r#"{"rule":"failed","input":"team.rule","value":null,"threshold":null,"route":"owner-review","paths":[],"more_paths":0,"decisive":true}"#
        );
    }

    #[test]
    fn a_count_serializes_as_an_integer_and_a_share_as_a_fraction() {
        let reason = Reason {
            rule: ReasonRule::AreaShare,
            input: "docs".to_string(),
            value: Number::from_f64(0.25),
            threshold: Some(Number::from(5_u64)),
            route: Route::OwnerReview,
            paths: Vec::new(),
            more_paths: 0,
            decisive: false,
        };

        let text = serde_json::to_string(&reason).expect("serialize");

        assert!(text.contains(r#""value":0.25,"threshold":5,"#), "{text}");
    }
}
