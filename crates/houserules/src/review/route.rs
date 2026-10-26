//! `houserules route`: the review route of a change (spec 6).
//!
//! [`decide`] reads the [`Analysis`] of a range and, unless it computes the
//! tool-free floor, the audit rows with their [`Coverage`]. The route is the
//! highest route that any rule of spec 6.2 gives. Each rule that was evaluated
//! leaves a [`Reason`] in the document (spec 6.4): a rule that found nothing
//! leaves one with the value 0 at the lowest route, and a limit or a threshold
//! that the change stays within leaves one too, so the document names every
//! input and every limit.
//!
//! A change is `auto-merge-eligible` only through an area or a path entry
//! (design.md 257): a checked `review` key never sets `default` or `warn` to
//! that route. It keeps the route only while the diff size and the uncovered
//! share stay within the limits of its area or its path entry. A path entry has
//! the limits of an area: `review.route.paths` entries with that route must
//! name both (spec 6.1), and the check is the one of rule 4. The floor
//! (`--no-audit`) reads no audit row, so it has no reason for rules 2 and 3 and
//! reads no uncovered share.
//!
//! [`reason_lines`] prints the reasons at the final route, one line for each
//! failed or warned row, area, and path, sorted. A rule that found nothing has
//! no line. A rule that names paths prints at most [`LISTED_PATHS`] of them and
//! then the count of the rest. A repository value (a path, an id, a glob, an
//! area name) prints through [`code_span`].

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args;
use serde_json::Number;

use super::change::{self, Analysis, AuditSource, Coverage};
use super::git;
use super::text::code_span;
use crate::emit::emit;
use crate::install::marker::Marker;
use crate::install::marker::review::RouteConfig;
use crate::published;
use crate::published::audit_result::{AuditResult, RowMode, RowResult};
use crate::published::route::{Inputs, PathInput, Reason, ReasonRule, Route, RouteDocument};
use crate::rules::{compile, load_base};

/// The paths that a reason lists before it counts the rest (spec 6.3).
const LISTED_PATHS: usize = 20;

/// The line for a `--score` when neither threshold is configured (spec 6.2,
/// rule 7).
const SCORE_NEEDS_A_THRESHOLD: &str = "route: --score needs review.route.ownerScore or seniorScore";

/// The arguments of `houserules route`.
#[derive(Args)]
pub(crate) struct RouteArgs {
    /// The base ref of the range. The range runs from the merge base of
    /// `--base` and `--head` to `--head`.
    #[arg(long, value_name = "REF")]
    pub(crate) base: String,
    /// The head ref of the range; defaults to `HEAD`.
    #[arg(long, value_name = "REF")]
    pub(crate) head: Option<String>,
    /// The file that `audit --json` wrote for the same range. Its `base` and
    /// `head` must name the same commits as `--base` and `--head`. Without this
    /// flag and without `--no-audit`, the command runs the audit in process.
    #[arg(long, value_name = "FILE", conflicts_with = "no_audit")]
    pub(crate) audit: Option<PathBuf>,
    /// Computes the floor: the route from the rules that read no audit row. A
    /// runner check whose tool is not installed does not stop it.
    #[arg(long)]
    pub(crate) no_audit: bool,
    /// An external score from 0 to 1. It needs `review.route.ownerScore` or
    /// `review.route.seniorScore`.
    #[arg(long, value_name = "SCORE", allow_negative_numbers = true)]
    pub(crate) score: Option<f64>,
    /// Also writes the decision document `houserules/route@1.0.0` to this file.
    #[arg(long, value_name = "FILE")]
    pub(crate) json: Option<PathBuf>,
    /// Install root to read from; defaults to the nearest directory at or
    /// above the current one that holds `.houserules.json`, no higher than the
    /// git top level, else the git top level.
    #[arg(long)]
    pub(crate) dir: Option<PathBuf>,
}

/// Gives the score as the decision reads it, or the `Err` line of spec 6.3 for
/// a score outside 0 to 1 (the range of `evidence --score`, spec 5.1). A score
/// of `-0` reads as 0, so no line and no document holds a negative zero.
fn score_in_range(score: f64) -> Result<f64, String> {
    if (0.0..=1.0).contains(&score) {
        Ok(score.abs())
    } else {
        Err(format!("route: --score {score} is outside 0 to 1"))
    }
}

/// Gives the `Err` line of spec 6.2, rule 7, for a score when `config` sets
/// neither threshold.
fn check_score(config: &RouteConfig, score: Option<f64>) -> Result<(), String> {
    if score.is_some() && config.owner_score.is_none() && config.senior_score.is_none() {
        Err(SCORE_NEEDS_A_THRESHOLD.to_string())
    } else {
        Ok(())
    }
}

/// The route decision for `analysis` (spec 6.2 and 6.4).
///
/// `audit` holds the audit result of the range with its coverage; `None` is
/// the floor (`--no-audit`), which reads no audit row. A `score` needs a
/// threshold in `config`, else the `Err` is the line of spec 6.2.
pub(crate) fn decide(
    analysis: &Analysis,
    audit: Option<(&AuditResult, &Coverage)>,
    config: &RouteConfig,
    score: Option<f64>,
) -> Result<RouteDocument, String> {
    check_score(config, score)?;
    let share = audit.map(|(_, coverage)| coverage.share);
    let (failed, warned) = audit.map_or_else(Default::default, |(result, _)| judged_rows(result));

    let mut reasons = vec![path_reason(
        ReasonRule::RuleSources,
        "rule_sources_changed",
        Route::SeniorReview,
        &analysis.rule_sources_changed,
    )];
    if audit.is_some() {
        row_reasons(
            ReasonRule::Failed,
            "failed",
            &failed,
            Route::OwnerReview,
            &mut reasons,
        );
        row_reasons(
            ReasonRule::Warned,
            "warned",
            &warned,
            config.warn,
            &mut reasons,
        );
    }
    area_reasons(analysis, config, share, &mut reasons);
    reasons.push(path_reason(
        ReasonRule::NoArea,
        "no_area",
        config.default,
        &analysis.no_area,
    ));
    let paths = path_entries(analysis, config, share, &mut reasons)?;
    score_reasons(config, score, &mut reasons);
    if analysis.paths.is_empty() && analysis.rule_sources_changed.is_empty() {
        reasons.push(Reason::new(ReasonRule::Default, "default", config.default));
    }

    let route = reasons
        .iter()
        .map(|reason| reason.route)
        .fold(Route::AutoMergeEligible, Route::max);
    for reason in &mut reasons {
        reason.decisive = reason.route == route;
    }
    Ok(RouteDocument {
        schema: published::id::<RouteDocument>(),
        install: analysis.install.clone(),
        head: analysis.head.clone(),
        route,
        floor: audit.is_none(),
        inputs: Inputs {
            failed,
            warned,
            areas: analysis.areas.clone(),
            no_area: analysis.no_area.clone(),
            paths,
            diff_lines: analysis.diff.lines,
            uncovered_share: share,
            score,
            rule_sources_changed: analysis.rule_sources_changed.clone(),
        },
        reasons,
    })
}

/// Adds the reasons of rule 4: the route of each area that holds a changed
/// path, and the limits of an area at `auto-merge-eligible`. With no such area
/// the rule found nothing.
fn area_reasons(
    analysis: &Analysis,
    config: &RouteConfig,
    share: Option<f64>,
    reasons: &mut Vec<Reason>,
) {
    if analysis.areas.is_empty() {
        reasons.push(nothing(ReasonRule::Area, "areas"));
    }
    for name in analysis.areas.keys() {
        let settings = config.areas.get(name);
        let route = settings
            .and_then(|settings| settings.route)
            .unwrap_or(config.default);
        reasons.push(Reason::new(ReasonRule::Area, name, route));
        if let Some(settings) =
            settings.filter(|settings| settings.route == Some(Route::AutoMergeEligible))
        {
            let limits = Limits {
                max_diff_lines: settings.max_diff_lines,
                max_uncovered_share: settings.max_uncovered_share,
            };
            limit_reasons(
                (ReasonRule::AreaDiff, ReasonRule::AreaShare),
                name,
                &limits,
                analysis.diff.lines,
                share,
                reasons,
            );
        }
    }
}

/// Adds the reasons of rule 7: the score against each threshold that the
/// configuration sets. A score below a threshold gives the lowest route.
fn score_reasons(config: &RouteConfig, score: Option<f64>, reasons: &mut Vec<Reason>) {
    let Some(score) = score else {
        return;
    };
    for (rule, threshold, route) in [
        (
            ReasonRule::ScoreSenior,
            config.senior_score,
            Route::SeniorReview,
        ),
        (
            ReasonRule::ScoreOwner,
            config.owner_score,
            Route::OwnerReview,
        ),
    ] {
        let Some(threshold) = threshold else {
            continue;
        };
        let route = if score >= threshold {
            route
        } else {
            Route::AutoMergeEligible
        };
        reasons.push(Reason {
            value: Number::from_f64(score),
            threshold: Number::from_f64(threshold),
            ..Reason::new(rule, "score", route)
        });
    }
}

impl Reason {
    /// A reason of `rule` about `input` that gave `route`, with no value, no
    /// threshold, and no path.
    fn new(rule: ReasonRule, input: &str, route: Route) -> Self {
        Self {
            rule,
            input: input.to_string(),
            value: None,
            threshold: None,
            route,
            paths: Vec::new(),
            more_paths: 0,
            decisive: false,
        }
    }
}

/// `reason` with its `value` set to the number of `names` and its paths set to
/// the first [`LISTED_PATHS`] of them.
fn listing(reason: Reason, names: &[String]) -> Reason {
    let listed = names.len().min(LISTED_PATHS);
    Reason {
        value: Some(Number::from(names.len() as u64)),
        paths: names[..listed].to_vec(),
        more_paths: (names.len() - listed) as u64,
        ..reason
    }
}

/// The reason of a rule that was evaluated and found nothing: it counts 0 and
/// gives the lowest route (spec 6.4). `input` is the key of `inputs` that the
/// rule read.
fn nothing(rule: ReasonRule, input: &str) -> Reason {
    listing(Reason::new(rule, input, Route::AutoMergeEligible), &[])
}

/// `true` for the reason of a rule that found nothing (see [`nothing`]): it
/// counts 0 and has no threshold.
fn found_nothing(reason: &Reason) -> bool {
    reason.threshold.is_none() && reason.value.as_ref().and_then(Number::as_u64) == Some(0)
}

/// The reason of a rule that names paths: `route` for the `names` it found, or
/// the reason of a rule that found nothing.
fn path_reason(rule: ReasonRule, input: &str, route: Route, names: &[String]) -> Reason {
    if names.is_empty() {
        nothing(rule, input)
    } else {
        listing(Reason::new(rule, input, route), names)
    }
}

/// Adds the reasons of a rule over audit rows: one reason at `route` for each
/// id of `ids`, or the reason of a rule that found nothing. `key` is the key of
/// `inputs` that holds the ids.
fn row_reasons(
    rule: ReasonRule,
    key: &str,
    ids: &[String],
    route: Route,
    reasons: &mut Vec<Reason>,
) {
    if ids.is_empty() {
        reasons.push(nothing(rule, key));
    } else {
        reasons.extend(ids.iter().map(|id| Reason::new(rule, id, route)));
    }
}

/// The ids of the deterministic rows of `result` that failed with no booking
/// and that warned, each list sorted.
fn judged_rows(result: &AuditResult) -> (Vec<String>, Vec<String>) {
    let mut failed = Vec::new();
    let mut warned = Vec::new();
    for row in result
        .rules
        .iter()
        .filter(|row| row.mode == RowMode::Deterministic)
    {
        match row.result {
            RowResult::Fail if row.sanctioned.is_none() => failed.push(row.id.clone()),
            RowResult::Warn => warned.push(row.id.clone()),
            _ => {}
        }
    }
    failed.sort();
    warned.sort();
    (failed, warned)
}

/// The limits of an area or a path entry, each `None` while it sets none.
struct Limits {
    /// The most lines that the diff may hold.
    max_diff_lines: Option<u64>,
    /// The largest share of uncovered lines, from 0 to 1.
    max_uncovered_share: Option<f64>,
}

/// Adds the reasons for the limits of the area or the path entry `subject`,
/// which holds `auto-merge-eligible`. `rules` are the rules of the diff limit
/// and the share limit. The share limit needs a share, which the floor lacks.
fn limit_reasons(
    rules: (ReasonRule, ReasonRule),
    subject: &str,
    limits: &Limits,
    diff_lines: u64,
    share: Option<f64>,
    reasons: &mut Vec<Reason>,
) {
    if let Some(max) = limits.max_diff_lines {
        reasons.push(Reason {
            value: Some(Number::from(diff_lines)),
            threshold: Some(Number::from(max)),
            ..Reason::new(rules.0, subject, route_for_limit(diff_lines > max))
        });
    }
    if let (Some(share), Some(max)) = (share, limits.max_uncovered_share) {
        reasons.push(Reason {
            value: Number::from_f64(share),
            threshold: Number::from_f64(max),
            ..Reason::new(rules.1, subject, route_for_limit(share > max))
        });
    }
}

/// `owner-review` for a limit that the change is past, else the lowest route.
fn route_for_limit(past_the_limit: bool) -> Route {
    if past_the_limit {
        Route::OwnerReview
    } else {
        Route::AutoMergeEligible
    }
}

/// Adds the reasons of rule 6 and returns the entries that matched a changed
/// path (old paths and new paths alike), in the order of the configuration.
/// With no such entry the rule found nothing.
fn path_entries(
    analysis: &Analysis,
    config: &RouteConfig,
    share: Option<f64>,
    reasons: &mut Vec<Reason>,
) -> Result<Vec<PathInput>, String> {
    let names: Vec<&str> = analysis
        .paths
        .iter()
        .flat_map(|path| std::iter::once(path.path.as_str()).chain(path.old_path.as_deref()))
        .collect();
    let mut entries = Vec::new();
    for (index, entry) in config.paths.iter().enumerate() {
        let matcher = compile(&entry.glob)
            .map_err(|error| format!("review.route.paths[{index}].glob: {error}"))?;
        let matched: BTreeSet<&str> = names
            .iter()
            .copied()
            .filter(|name| matcher.is_match(name))
            .collect();
        if matched.is_empty() {
            continue;
        }
        let matched: Vec<String> = matched.into_iter().map(str::to_string).collect();
        reasons.push(listing(
            Reason::new(ReasonRule::Path, &entry.glob, entry.route),
            &matched,
        ));
        if entry.route == Route::AutoMergeEligible {
            let limits = Limits {
                max_diff_lines: entry.max_diff_lines,
                max_uncovered_share: entry.max_uncovered_share,
            };
            limit_reasons(
                (ReasonRule::PathDiff, ReasonRule::PathShare),
                &entry.glob,
                &limits,
                analysis.diff.lines,
                share,
                reasons,
            );
        }
        entries.push(PathInput {
            glob: entry.glob.clone(),
            route: entry.route,
            paths: matched,
        });
    }
    if entries.is_empty() {
        reasons.push(nothing(ReasonRule::Path, "paths"));
    }
    Ok(entries)
}

/// The lines of spec 6.3 for the reasons of `document` at its final route,
/// sorted. The lines of one reason stay together, with the count of the paths
/// beyond the listed ones last. A rule that found nothing, and a limit or a
/// threshold that the change stays within, have no line.
pub(crate) fn reason_lines(document: &RouteDocument) -> Vec<String> {
    lines_of(&document.reasons)
}

/// The lines of spec 6.3 for `reasons`, as [`reason_lines`] gives them for the
/// reasons of a document.
pub(super) fn lines_of(reasons: &[Reason]) -> Vec<String> {
    let mut blocks: Vec<Vec<String>> = reasons
        .iter()
        .filter(|reason| reason.decisive)
        .map(block)
        .filter(|lines| !lines.is_empty())
        .collect();
    blocks.sort_by(|a, b| a[0].cmp(&b[0]));
    blocks.concat()
}

/// The lines of one reason: none for a rule that found nothing, and none for a
/// limit or a threshold that the change stays within.
fn block(reason: &Reason) -> Vec<String> {
    if found_nothing(reason) {
        return Vec::new();
    }
    let route = reason.route.as_str();
    let subject = code_span(&reason.input);
    match reason.rule {
        ReasonRule::RuleSources => path_lines(reason, |path| {
            format!("rule sources: {path} changed -> {route}")
        }),
        ReasonRule::Failed => vec![format!("failed: {subject} -> {route}")],
        ReasonRule::Warned => vec![format!("warned: {subject} -> {route}")],
        ReasonRule::Area => vec![format!("area {subject}: {route}")],
        ReasonRule::AreaDiff => limit_line(reason, &format!("area {subject}"), Limit::Diff),
        ReasonRule::AreaShare => limit_line(reason, &format!("area {subject}"), Limit::Share),
        ReasonRule::NoArea => path_lines(reason, |path| format!("no area: {path} -> {route}")),
        ReasonRule::Path => path_lines(reason, |path| format!("path {subject}: {path} -> {route}")),
        ReasonRule::PathDiff => limit_line(reason, &format!("path {subject}"), Limit::Diff),
        ReasonRule::PathShare => limit_line(reason, &format!("path {subject}"), Limit::Share),
        ReasonRule::ScoreSenior => score_line(reason, "seniorScore"),
        ReasonRule::ScoreOwner => score_line(reason, "ownerScore"),
        ReasonRule::Default => vec![format!("default -> {route}")],
    }
}

/// One line for each listed path of `reason`, from `line`, and the count of the
/// rest.
fn path_lines(reason: &Reason, line: impl Fn(&str) -> String) -> Vec<String> {
    let mut lines: Vec<String> = reason
        .paths
        .iter()
        .map(|path| line(&code_span(path)))
        .collect();
    if reason.more_paths > 0 {
        lines.push(format!("and {} more", reason.more_paths));
    }
    lines
}

/// Which limit a line of an area or a path entry reports.
enum Limit {
    /// `maxDiffLines`.
    Diff,
    /// `maxUncoveredShare`.
    Share,
}

/// The line for a limit that `reason` is past, else none. `prefix` is
/// `area <name>` or `path <glob>`. A number that the document lacks prints as
/// `?` (`quality.absence-is-designed`).
fn limit_line(reason: &Reason, prefix: &str, limit: Limit) -> Vec<String> {
    let (Some(value), Some(threshold)) = (&reason.value, &reason.threshold) else {
        return Vec::new();
    };
    if !exceeds(value, threshold) {
        return Vec::new();
    }
    let route = reason.route.as_str();
    let line = match limit {
        Limit::Diff => format!(
            "{prefix}: diff {} lines > maxDiffLines {} -> {route}",
            count_text(value),
            count_text(threshold)
        ),
        Limit::Share => format!(
            "{prefix}: uncovered share {} > maxUncoveredShare {} -> {route}",
            share_text(value, two_decimals_rounded_up),
            share_text(threshold, two_decimals_rounded_down)
        ),
    };
    vec![line]
}

/// The line for a score that reaches the threshold of `reason`, else none.
/// `name` is `seniorScore` or `ownerScore`.
fn score_line(reason: &Reason, name: &str) -> Vec<String> {
    let (Some(value), Some(threshold)) = (&reason.value, &reason.threshold) else {
        return Vec::new();
    };
    let (Some(score), Some(limit)) = (value.as_f64(), threshold.as_f64()) else {
        return Vec::new();
    };
    if score < limit {
        return Vec::new();
    }
    vec![format!(
        "score {score} >= {name} {limit} -> {}",
        reason.route.as_str()
    )]
}

/// `true` when `value` is above `threshold`.
fn exceeds(value: &Number, threshold: &Number) -> bool {
    matches!((value.as_f64(), threshold.as_f64()), (Some(value), Some(threshold)) if value > threshold)
}

/// A count as text.
fn count_text(number: &Number) -> String {
    number
        .as_u64()
        .map_or_else(|| "?".to_string(), |count| count.to_string())
}

/// A share or a limit as text, from `round`, which gives it with two decimals.
fn share_text(number: &Number, round: fn(f64) -> String) -> String {
    number.as_f64().map_or_else(|| "?".to_string(), round)
}

/// `value` with two decimals, rounded up: never below `value`. A line of a
/// limit that the share is past prints the share this way and the limit down,
/// so the printed share is greater than the printed limit (spec 6.3).
///
/// The nearest two-decimal text is compared with `value` itself. Scaling by 100
/// and taking `ceil` would turn 0.07 into 0.08, because 0.07 * 100 is
/// 7.000000000000001.
fn two_decimals_rounded_up(value: f64) -> String {
    let nearest = format!("{value:.2}");
    match nearest.parse::<f64>() {
        Ok(printed) if printed < value => format!("{:.2}", printed + 0.01),
        _ => nearest,
    }
}

/// `value` with two decimals, rounded down: never above `value`.
fn two_decimals_rounded_down(value: f64) -> String {
    let nearest = format!("{value:.2}");
    match nearest.parse::<f64>() {
        Ok(printed) if printed > value => format!("{:.2}", printed - 0.01),
        _ => nearest,
    }
}

/// Runs `houserules route`: prints the route and the lines of its reasons, and
/// writes `--json`. A setup error is one line on stderr for each fault and
/// exit 2 (`houserules.crash-paths-are-named`).
pub(crate) fn cmd_route(args: RouteArgs) -> ExitCode {
    let root = match crate::root::resolve_root(args.dir.clone()) {
        Ok(root) => root,
        Err(code) => return code,
    };
    match run(&root, &args) {
        Ok(text) => {
            print!("{text}");
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

/// The text that `route` prints on stdout, after it wrote `--json`.
fn run(root: &Path, args: &RouteArgs) -> Result<String, Vec<String>> {
    let score = args
        .score
        .map(score_in_range)
        .transpose()
        .map_err(|line| vec![line])?;
    git::require_attr_source("route").map_err(|line| vec![line])?;
    let base = load_base(root).map_err(|error| vec![error.to_string()])?;
    let marker = Marker::read_or_default_for_gate(root, "WI").map_err(|line| vec![line])?;
    let config = marker.review()?;
    check_score(&config.route, score).map_err(|line| vec![line])?;
    let head_ref = args.head.as_deref().unwrap_or("HEAD");
    let analysis =
        change::analyze(root, &base, &config, &args.base, head_ref).map_err(|line| vec![line])?;
    let audited = if args.no_audit {
        None
    } else {
        let source = match &args.audit {
            Some(file) => AuditSource::File(file.clone()),
            None => AuditSource::InProcess,
        };
        let result =
            change::audit_rows(root, &base, source, &analysis).map_err(|line| vec![line])?;
        let coverage = change::coverage(&analysis, &result, &base);
        Some((result, coverage))
    };
    let document = decide(
        &analysis,
        audited
            .as_ref()
            .map(|(result, coverage)| (result, coverage)),
        &config.route,
        score,
    )
    .map_err(|line| vec![line])?;
    if let Some(file) = &args.json {
        let text = emit(&serde_json::to_value(&document).expect("a route document serializes"));
        fs::write(file, text).map_err(|error| vec![format!("{}: {error}", file.display())])?;
    }
    Ok(rendered(&document))
}

/// The text of stdout: the first line, then the lines of the reasons.
fn rendered(document: &RouteDocument) -> String {
    let label = if document.floor { "floor" } else { "route" };
    let mut text = format!("{label}: {}\n", document.route.as_str());
    for line in reason_lines(document) {
        text.push_str(&line);
        text.push('\n');
    }
    text
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use serde_json::json;

    use super::*;
    use crate::install::marker::review::{AreaRoute, PathRoute};
    use crate::published::audit_result::{AuditRow, References, RowLevel, Summary};
    use crate::published::change::ChangeKind;
    use crate::review::change::{ChangedPath, DiffSize, RulesHashes, RulesPresence};

    /// An analysis of a top-level install with no changed path.
    fn analysis() -> Analysis {
        Analysis {
            install: ".".to_string(),
            base: "b".repeat(40),
            head: "h".repeat(40),
            merge_base: Some("m".repeat(40)),
            paths: Vec::new(),
            diff: DiffSize {
                files: 0,
                added: 0,
                deleted: 0,
                lines: 0,
                excluded: 0,
            },
            rules: RulesHashes {
                merge_base_hash: Some("1".repeat(64)),
                merge_base_rules: RulesPresence::Present,
                head_hash: "2".repeat(64),
            },
            work_items: Vec::new(),
            areas: IndexMap::new(),
            no_area: Vec::new(),
            rule_sources_changed: Vec::new(),
        }
    }

    /// A text path changed by one added line.
    fn changed(path: &str) -> ChangedPath {
        ChangedPath {
            path: path.to_string(),
            old_path: None,
            kind: ChangeKind::Text,
            added: 1,
            deleted: 0,
            ranges: Vec::new(),
            excluded: false,
            empty: None,
        }
    }

    /// A deterministic row with the result `result` and the booking
    /// `sanctioned`.
    fn row(id: &str, result: RowResult, sanctioned: Option<&str>) -> AuditRow {
        AuditRow {
            id: id.to_string(),
            kind: "rule".to_string(),
            mode: RowMode::Deterministic,
            level: Some(RowLevel::Fail),
            result,
            evidence: "evidence".to_string(),
            findings: None,
            sanctioned: sanctioned.map(str::to_string),
        }
    }

    /// An audit result that holds `rows`.
    fn result(rows: Vec<AuditRow>) -> AuditResult {
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

    /// A coverage with the share `share`.
    fn coverage(share: f64) -> Coverage {
        Coverage {
            uncovered: Vec::new(),
            uncovered_lines: 0,
            total_lines: 0,
            share,
        }
    }

    #[test]
    fn a_sanctioned_fail_a_pass_and_a_judged_row_raise_nothing() {
        let mut judged = row("team.judged", RowResult::Open, None);
        judged.mode = RowMode::Judged;
        let audit = result(vec![
            row("team.booked", RowResult::Fail, Some("HR-1")),
            row("team.passes", RowResult::Pass, None),
            judged,
            row("team.fails", RowResult::Fail, None),
            row("team.warns", RowResult::Warn, None),
        ]);

        let document = decide(
            &analysis(),
            Some((&audit, &coverage(0.0))),
            &RouteConfig::default(),
            None,
        )
        .expect("a decision");

        assert_eq!(document.inputs.failed, ["team.fails"]);
        assert_eq!(document.inputs.warned, ["team.warns"]);
    }

    #[test]
    fn a_glob_that_does_not_compile_is_a_named_error() {
        let mut changes = analysis();
        changes.paths.push(changed("a.rs"));
        changes.no_area.push("a.rs".to_string());
        let mut config = RouteConfig::default();
        config.paths.push(PathRoute {
            glob: "a[z-a]b".to_string(),
            route: Route::SeniorReview,
            max_diff_lines: None,
            max_uncovered_share: None,
        });

        let error = decide(&changes, None, &config, None).expect_err("a named error");

        assert!(
            error.starts_with("review.route.paths[0].glob: invalid glob \"a[z-a]b\""),
            "{error}"
        );
    }

    #[test]
    fn a_limit_that_the_change_stays_within_has_a_reason_and_no_line() {
        let mut changes = analysis();
        changes.paths.push(changed("docs/a.md"));
        changes.diff.lines = 3;
        changes
            .areas
            .insert("docs".to_string(), vec!["docs/a.md".to_string()]);
        let mut config = RouteConfig::default();
        config.areas.insert(
            "docs".to_string(),
            AreaRoute {
                route: Some(Route::AutoMergeEligible),
                max_diff_lines: Some(5),
                max_uncovered_share: Some(0.5),
            },
        );
        let audit = result(Vec::new());

        let document =
            decide(&changes, Some((&audit, &coverage(0.25))), &config, None).expect("a decision");

        let diff = document
            .reasons
            .iter()
            .find(|reason| reason.rule == ReasonRule::AreaDiff)
            .expect("the diff reason");
        assert_eq!(
            (&diff.value, &diff.threshold, diff.route, diff.decisive),
            (
                &Some(Number::from(3_u64)),
                &Some(Number::from(5_u64)),
                Route::AutoMergeEligible,
                true
            )
        );
        assert_eq!(
            reason_lines(&document),
            ["area `docs`: auto-merge-eligible"]
        );
    }

    #[test]
    fn a_number_that_is_no_count_prints_as_a_question_mark() {
        let reason = Reason {
            value: Number::from_f64(2.5),
            threshold: Some(Number::from(5_u64)),
            decisive: true,
            ..Reason::new(ReasonRule::AreaDiff, "docs", Route::OwnerReview)
        };
        let past = Reason {
            value: Number::from_f64(7.5),
            ..reason.clone()
        };

        assert_eq!(
            block(&past),
            ["area `docs`: diff ? lines > maxDiffLines 5 -> owner-review"]
        );
        assert!(block(&reason).is_empty(), "2.5 is not past 5");
    }

    #[test]
    fn a_score_from_0_to_1_is_in_range_and_any_other_has_the_line_of_the_spec() {
        for (score, read) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (-0.0, 0.0)] {
            let read_score = score_in_range(score).expect("in range");
            assert_eq!(read_score, read);
            assert!(read_score.is_sign_positive(), "{score}");
        }
        for (score, line) in [
            (1.01, "route: --score 1.01 is outside 0 to 1"),
            (-0.1, "route: --score -0.1 is outside 0 to 1"),
            (f64::NAN, "route: --score NaN is outside 0 to 1"),
            (f64::INFINITY, "route: --score inf is outside 0 to 1"),
            (f64::NEG_INFINITY, "route: --score -inf is outside 0 to 1"),
        ] {
            assert_eq!(score_in_range(score), Err(line.to_string()));
        }
    }

    /// The rule and the input of each reason of `document`, in order.
    fn rules_and_inputs(document: &RouteDocument) -> Vec<(ReasonRule, &str)> {
        document
            .reasons
            .iter()
            .map(|reason| (reason.rule, reason.input.as_str()))
            .collect()
    }

    #[test]
    fn a_rule_that_found_nothing_has_a_reason_with_the_value_0() {
        let mut changes = analysis();
        changes.paths.push(changed("docs/a.md"));
        changes
            .areas
            .insert("docs".to_string(), vec!["docs/a.md".to_string()]);
        let audit = result(Vec::new());

        let document = decide(
            &changes,
            Some((&audit, &coverage(0.0))),
            &RouteConfig::default(),
            None,
        )
        .expect("a decision");

        assert_eq!(
            rules_and_inputs(&document),
            [
                (ReasonRule::RuleSources, "rule_sources_changed"),
                (ReasonRule::Failed, "failed"),
                (ReasonRule::Warned, "warned"),
                (ReasonRule::Area, "docs"),
                (ReasonRule::NoArea, "no_area"),
                (ReasonRule::Path, "paths"),
            ]
        );
        for reason in &document.reasons {
            let nothing = reason.rule != ReasonRule::Area;
            assert_eq!(
                (reason.value.clone(), reason.route, reason.decisive),
                if nothing {
                    (Some(Number::from(0_u64)), Route::AutoMergeEligible, false)
                } else {
                    (None, Route::OwnerReview, true)
                },
                "{:?}",
                reason.rule
            );
        }
        assert_eq!(reason_lines(&document), ["area `docs`: owner-review"]);
    }

    #[test]
    fn a_floor_has_no_reason_for_a_failed_or_a_warned_row() {
        let document =
            decide(&analysis(), None, &RouteConfig::default(), None).expect("a decision");

        assert_eq!(
            rules_and_inputs(&document),
            [
                (ReasonRule::RuleSources, "rule_sources_changed"),
                (ReasonRule::Area, "areas"),
                (ReasonRule::NoArea, "no_area"),
                (ReasonRule::Path, "paths"),
                (ReasonRule::Default, "default"),
            ]
        );
    }

    #[test]
    fn a_changed_rule_source_with_no_changed_path_takes_no_default_reason() {
        let mut changes = analysis();
        changes
            .rule_sources_changed
            .push(".github/workflows/ci.yml".to_string());

        let document = decide(&changes, None, &RouteConfig::default(), None).expect("a decision");

        assert_eq!(document.route, Route::SeniorReview);
        assert!(
            document
                .reasons
                .iter()
                .all(|reason| reason.rule != ReasonRule::Default),
            "{:?}",
            rules_and_inputs(&document)
        );
        assert_eq!(
            reason_lines(&document),
            ["rule sources: `.github/workflows/ci.yml` changed -> senior-review"]
        );
    }

    /// The line of a share limit that the change is past.
    fn share_line(share: f64, limit: f64) -> String {
        let reason = Reason {
            value: Number::from_f64(share),
            threshold: Number::from_f64(limit),
            decisive: true,
            ..Reason::new(ReasonRule::AreaShare, "docs", Route::OwnerReview)
        };
        block(&reason).concat()
    }

    #[test]
    fn a_share_line_rounds_the_share_up_and_the_limit_down() {
        for (share, limit, line) in [
            (
                1.0 / 300.0,
                0.0,
                "area `docs`: uncovered share 0.01 > maxUncoveredShare 0.00 -> owner-review",
            ),
            (
                0.3,
                0.256,
                "area `docs`: uncovered share 0.30 > maxUncoveredShare 0.25 -> owner-review",
            ),
            (
                0.07,
                0.06,
                "area `docs`: uncovered share 0.07 > maxUncoveredShare 0.06 -> owner-review",
            ),
            (
                0.501,
                0.499,
                "area `docs`: uncovered share 0.51 > maxUncoveredShare 0.49 -> owner-review",
            ),
            (
                1.0,
                0.999,
                "area `docs`: uncovered share 1.00 > maxUncoveredShare 0.99 -> owner-review",
            ),
        ] {
            assert_eq!(share_line(share, limit), line, "{share} > {limit}");
        }
    }

    #[test]
    fn a_printed_share_is_greater_than_the_printed_limit_for_any_pair_past_it() {
        for share_thousandths in (1..=1000).step_by(7) {
            for limit_thousandths in (0..share_thousandths).step_by(11) {
                let share = f64::from(share_thousandths) / 1000.0;
                let limit = f64::from(limit_thousandths) / 1000.0;
                let line = share_line(share, limit);
                let numbers: Vec<f64> = line
                    .split_whitespace()
                    .filter_map(|word| word.parse::<f64>().ok())
                    .collect();
                assert_eq!(numbers.len(), 2, "{line}");
                assert!(numbers[0] > numbers[1], "{share} > {limit}: {line}");
            }
        }
    }

    #[test]
    fn a_change_with_no_changed_path_takes_the_default_route() {
        let document =
            decide(&analysis(), None, &RouteConfig::default(), None).expect("a decision");

        let value = serde_json::to_value(&document).expect("serialize");

        assert_eq!(
            value["inputs"],
            json!({
                "failed": [], "warned": [], "areas": {}, "no_area": [], "paths": [],
                "diff_lines": 0, "uncovered_share": null, "score": null,
                "rule_sources_changed": []
            })
        );
        assert_eq!(reason_lines(&document), ["default -> owner-review"]);
    }
}
