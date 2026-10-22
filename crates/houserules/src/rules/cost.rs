//! What `stats` measures in a workspace, and the cost profile built from
//! it: the `workspaces`, `models`, and `cost` keys.
//!
//! `stats.rs` reads the deliverables into one `Workspace` per directory.
//! This module classifies a finding (`Severity`, `Target`) and renders
//! the per-workspace rows, the per-model rows, and their totals.
//! `proposals.rs` reads the same `Workspace` values to rate each knowledge
//! entry.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// The designed rendering of a mean over zero tasks
/// (`quality.absence-is-designed`).
const NO_MEAN: &str = "none";

/// The model row of a deliverable with no `model` field: one that an earlier
/// kit wrote, before the field was required (`quality.absence-is-designed`).
pub(super) const PRE_2_0: &str = "pre-2.0";

/// How a reviewer rated a finding. A missing, non-string, or unknown
/// `severity` is `Other`, so an old review shape is counted, never a
/// crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Severity {
    Critical,
    Important,
    Minor,
    Other,
}

impl Severity {
    /// The severity of a finding row, from its `severity` string.
    fn of(issue: &Value) -> Severity {
        match issue.get("severity").and_then(Value::as_str) {
            Some("critical") => Severity::Critical,
            Some("important") => Severity::Important,
            Some("minor") => Severity::Minor,
            _ => Severity::Other,
        }
    }
}

/// What kind of file a finding is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Target {
    /// Source, tests, tooling: any path no other class claims.
    Code,
    /// A path inside the workspace, or a report, review, or audit file.
    Deliverable,
    /// Markdown, `docs/`, `knowledge/`, `backlog/`, or a commit message.
    Prose,
    /// No usable `file` field.
    Other,
}

/// Findings counted by `Severity`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct SeverityCounts {
    pub critical: usize,
    pub important: usize,
    pub minor: usize,
    pub other: usize,
}

impl SeverityCounts {
    /// Counts one more finding of `severity`.
    pub(super) fn add(&mut self, severity: Severity) {
        match severity {
            Severity::Critical => self.critical += 1,
            Severity::Important => self.important += 1,
            Severity::Minor => self.minor += 1,
            Severity::Other => self.other += 1,
        }
    }

    /// All findings counted, of any severity.
    pub(super) fn total(&self) -> usize {
        self.critical + self.important + self.minor + self.other
    }

    /// The counts as a JSON object, each count mapped through `leaf`.
    fn render(&self, leaf: &dyn Fn(u64) -> Value) -> Value {
        json!({
            "critical": leaf(self.critical as u64),
            "important": leaf(self.important as u64),
            "minor": leaf(self.minor as u64),
            "other": leaf(self.other as u64),
        })
    }

    /// The counts as a JSON object of integers.
    pub(super) fn to_json(self) -> Value {
        self.render(&|count| json!(count))
    }
}

/// Findings counted by `Target`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct TargetCounts {
    pub code: usize,
    pub deliverable: usize,
    pub prose: usize,
    pub other: usize,
}

impl TargetCounts {
    /// Counts one more finding aimed at `target`.
    pub(super) fn add(&mut self, target: Target) {
        match target {
            Target::Code => self.code += 1,
            Target::Deliverable => self.deliverable += 1,
            Target::Prose => self.prose += 1,
            Target::Other => self.other += 1,
        }
    }

    /// The counts as a JSON object, each count mapped through `leaf`.
    fn render(&self, leaf: &dyn Fn(u64) -> Value) -> Value {
        json!({
            "code": leaf(self.code as u64),
            "deliverable": leaf(self.deliverable as u64),
            "prose": leaf(self.prose as u64),
            "other": leaf(self.other as u64),
        })
    }

    /// The counts as a JSON object of integers.
    pub(super) fn to_json(self) -> Value {
        self.render(&|count| json!(count))
    }
}

/// One reviewer finding: an `issues` row of a task or branch review, a
/// `new_breakage` row of a re-review, or a `findings` row of a spec review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Finding {
    pub severity: Severity,
    pub target: Target,
    /// The knowledge entry the finding cites, when its `rule` is a string.
    pub rule: Option<String>,
    /// The row as the review holds it: a `mechanize` proposal quotes it as a
    /// sample.
    pub issue: Value,
}

impl Finding {
    /// Reads a finding from a raw `issue` row, tolerating every old shape:
    /// a row that is no object, or lacks `severity`, `file`, or `rule`,
    /// still yields a finding, classified `Other` where it lacks the field.
    /// `workspace_label` is the workspace's last path component.
    pub(super) fn from_issue(issue: &Value, workspace_label: &str) -> Finding {
        Finding {
            severity: Severity::of(issue),
            target: target(issue.get("file").and_then(Value::as_str), workspace_label),
            rule: issue.get("rule").and_then(Value::as_str).map(String::from),
            issue: issue.clone(),
        }
    }

    /// `true` when the finding is minor, or aimed at a deliverable. A
    /// finding on prose is soft only when it is minor: in a repository whose
    /// docs are its product, prose is a contract surface (design.md 5.93).
    pub(super) fn is_soft(&self) -> bool {
        self.severity == Severity::Minor || self.target == Target::Deliverable
    }
}

/// One row of an audit's `rules` or of a review's `rule_adherence`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct AdherenceRow {
    pub id: String,
    /// `true` when the row's `result` is `fail` and no sanction accepts it.
    pub failed: bool,
    /// `true` when the row is the one source of its mode: a deterministic
    /// row of an audit file, or a judged row of a review file. A reviewer
    /// copies deterministic audit rows into `rule_adherence`, and an audit
    /// lists judged rows as `open`; neither copy is counted again. An
    /// uncounted row still puts its workspace among the entry's
    /// `workspaces`.
    pub counted: bool,
    /// `true` when the row is a counted judged row of a review whose
    /// `result` is `pass` or `fail`: a verdict that a person may overrule.
    pub verdict: bool,
    /// `true` when the row is a verdict that carries an `override`.
    pub overridden: bool,
    /// The reference of the sanction that accepts the row, when its `result`
    /// is `fail` and a sanction accepts it.
    pub sanctioned: Option<String>,
    /// What the row adds to the cost of its rule: for the first judged row of
    /// the rule in a review file, the file's bytes divided by all the judged
    /// rows of the review, rounded down; `0` for each further judged row of
    /// the rule in the same file, because a review costs a rule one share.
    /// `None` for a row that is no judged row of a review.
    pub cost: Option<u64>,
}

/// A fail that a sanction accepts, as the top-level `sanctioned` list names
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Sanction {
    /// The knowledge entry that failed.
    pub id: String,
    /// The task label of the deliverable that holds the row. The branch
    /// review and the spec reviews name no task: their label is the file name.
    pub task: String,
    /// The reference that books the sanction.
    pub reference: String,
}

/// What the deliverables of one model cost in one workspace.
#[derive(Debug, Default)]
pub(super) struct ModelCost {
    /// The `task-*-report.json` files the model wrote.
    pub tasks: usize,
    /// The review files the model wrote: task reviews, re-reviews, spec
    /// reviews, and `branch-review.json`.
    pub reviews: usize,
    /// Of those, the files whose `kind` is `re-review`.
    pub re_reviews: usize,
    /// The sum of the model's reports' `fix_rounds` lengths.
    pub fix_rounds: usize,
    /// The sum of the model's report file sizes.
    pub report_bytes: u64,
    /// The findings of the model's reviews.
    pub findings: Vec<Finding>,
}

/// Everything `stats` measured in one workspace directory.
///
/// `models` holds what the deliverables cost, by the `models` row of the model
/// that wrote them (`stats::model_row`: the family of a model that names one).
/// The counters below it, `tasks` to `findings`, are the sums over `models`:
/// `sum_models` sets them once the workspace is read.
#[derive(Debug, Default)]
pub(super) struct Workspace {
    /// The path as the caller gave it.
    pub path: String,
    /// The last component of the path: the folder name that orders the
    /// workspace by recency (`recency_cmp`).
    pub label: String,
    /// The cost of each model's deliverables, by `models` row name. A
    /// deliverable with no `model` field counts under `PRE_2_0`.
    pub models: BTreeMap<String, ModelCost>,
    /// The `task-*-report.json` files.
    pub tasks: usize,
    /// Task review files plus `branch-review.json` and the spec reviews.
    pub reviews: usize,
    /// Review files whose `kind` is `re-review`.
    pub re_reviews: usize,
    /// The sum of the reports' `fix_rounds` lengths.
    pub fix_rounds: usize,
    /// The sum of the report file sizes.
    pub report_bytes: u64,
    pub findings: Vec<Finding>,
    pub rows: Vec<AdherenceRow>,
    /// The counted fails that a sanction accepts.
    pub sanctions: Vec<Sanction>,
    /// The `issues` rows of the task reviews and of the branch review, in the
    /// order `stats` reads the files: the task reviews by file name, then the
    /// branch review. A re-review's `new_breakage` and a spec review's
    /// `findings` are in `findings` only. The `samples` of a `mechanize`
    /// proposal come from these rows.
    pub issues: Vec<Finding>,
    /// The lines of the workspace's `progress.md` that record a reversal.
    pub reversals: usize,
    /// The ids the audits injected.
    pub injected: BTreeSet<String>,
    /// One id per report that names it in `knowledge_used`.
    pub cited: Vec<String>,
}

impl Workspace {
    /// The cost slice of `model`, created empty on first use.
    pub(super) fn model(&mut self, model: &str) -> &mut ModelCost {
        self.models.entry(model.to_string()).or_default()
    }

    /// Sets the counters of the whole workspace to the sums over `models`.
    pub(super) fn sum_models(&mut self) {
        self.tasks = self.models.values().map(|cost| cost.tasks).sum();
        self.reviews = self.models.values().map(|cost| cost.reviews).sum();
        self.re_reviews = self.models.values().map(|cost| cost.re_reviews).sum();
        self.fix_rounds = self.models.values().map(|cost| cost.fix_rounds).sum();
        self.report_bytes = self.models.values().map(|cost| cost.report_bytes).sum();
        self.findings = self
            .models
            .values()
            .flat_map(|cost| cost.findings.iter().cloned())
            .collect();
    }
}

/// `true` when `locator` is `<line>` or `<line>-<line>`, numerals only.
fn is_line_locator(locator: &str) -> bool {
    let is_line = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    match locator.split_once('-') {
        Some((first, last)) => is_line(first) && is_line(last),
        None => is_line(locator),
    }
}

/// Removes a `:<line>` or `:<line>-<line>` tail from the end of `file`.
fn without_line_suffix(file: &str) -> &str {
    match file.rsplit_once(':') {
        Some((path, locator)) if is_line_locator(locator) => path,
        _ => file,
    }
}

/// `true` when `name` has `marker` followed, later, by `.json`: the
/// glob `*<marker>*.json`.
fn has_marker_then_json(name: &str, marker: &str) -> bool {
    name.find(marker)
        .is_some_and(|at| name[at + marker.len()..].ends_with(".json"))
}

/// `true` when `name` matches `*-report.json`, `*-review*.json`, or
/// `*-audit*.json`.
fn is_deliverable_name(name: &str) -> bool {
    name.ends_with("-report.json")
        || has_marker_then_json(name, "-review")
        || has_marker_then_json(name, "-audit")
}

/// The directories whose files are prose, not code.
const PROSE_DIRECTORIES: [&str; 3] = ["docs/", "knowledge/", "backlog/"];

/// The phrases that name a commit message wherever they sit in a `file`,
/// compared in lower case.
const COMMIT_MESSAGE_PHRASES: [&str; 3] = ["commit message", "commit body", "commit header"];

/// The shortest commit id a `file` may start with, in hexadecimal
/// characters.
const COMMIT_ID_MIN_LENGTH: usize = 7;

/// The longest commit id a `file` may start with, in hexadecimal
/// characters.
const COMMIT_ID_MAX_LENGTH: usize = 40;

/// `true` when `text` starts with a commit id: 7 to 40 hexadecimal
/// characters, then the end, a space, or `..`.
fn starts_with_commit_id(text: &str) -> bool {
    let length = text.bytes().take_while(u8::is_ascii_hexdigit).count();
    let rest = &text[length..];
    (COMMIT_ID_MIN_LENGTH..=COMMIT_ID_MAX_LENGTH).contains(&length)
        && (rest.is_empty() || rest.starts_with(' ') || rest.starts_with(".."))
}

/// `true` when `path` names a commit message, compared in lower case: it
/// starts with a commit id, or with `commit ` and a commit id, or with
/// `git log` or `git history`, or it contains `commit message`,
/// `commit body`, or `commit header`. A path that holds the word `commit`
/// as part of a file or directory name is code.
fn names_a_commit_message(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    starts_with_commit_id(&lower)
        || lower
            .strip_prefix("commit ")
            .is_some_and(starts_with_commit_id)
        || lower.starts_with("git log")
        || lower.starts_with("git history")
        || COMMIT_MESSAGE_PHRASES
            .iter()
            .any(|phrase| lower.contains(phrase))
}

/// `true` when `path` is Markdown, sits under a prose directory, or names
/// a commit message.
fn is_prose(path: &str) -> bool {
    path.ends_with(".md")
        || PROSE_DIRECTORIES
            .iter()
            .any(|directory| path.starts_with(directory))
        || names_a_commit_message(path)
}

/// The target of a finding, from its `file` field. `workspace_label` is
/// the workspace's last path component.
///
/// An absent, non-string, or blank `file` is `Other`. Else, without a
/// `:<line>` or `:<line>-<line>` tail: a path that contains
/// `workspace_label`, or whose file name is a report, review, or audit
/// file, is `Deliverable`; Markdown, a prose directory, or a commit
/// message (`names_a_commit_message`) is `Prose`; anything else is
/// `Code`.
pub(super) fn target(file: Option<&str>, workspace_label: &str) -> Target {
    let Some(file) = file.map(str::trim).filter(|file| !file.is_empty()) else {
        return Target::Other;
    };
    let path = without_line_suffix(file);
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if path.contains(workspace_label) || is_deliverable_name(name) {
        Target::Deliverable
    } else if is_prose(path) {
        Target::Prose
    } else {
        Target::Code
    }
}

/// The sums of the cost counters over some workspaces.
#[derive(Default)]
struct Profile {
    tasks: u64,
    reviews: u64,
    re_reviews: u64,
    fix_rounds: u64,
    report_bytes: u64,
    findings: SeverityCounts,
    targets: TargetCounts,
}

impl Profile {
    /// Sums the counters of `workspaces`.
    fn of(workspaces: &[Workspace]) -> Profile {
        let mut profile = Profile::default();
        for workspace in workspaces {
            profile.tasks += workspace.tasks as u64;
            profile.reviews += workspace.reviews as u64;
            profile.re_reviews += workspace.re_reviews as u64;
            profile.fix_rounds += workspace.fix_rounds as u64;
            profile.report_bytes += workspace.report_bytes;
            for finding in &workspace.findings {
                profile.findings.add(finding.severity);
                profile.targets.add(finding.target);
            }
        }
        profile
    }

    /// Adds what one model's deliverables cost in one workspace.
    fn add_model(&mut self, cost: &ModelCost) {
        self.tasks += cost.tasks as u64;
        self.reviews += cost.reviews as u64;
        self.re_reviews += cost.re_reviews as u64;
        self.fix_rounds += cost.fix_rounds as u64;
        self.report_bytes += cost.report_bytes;
        for finding in &cost.findings {
            self.findings.add(finding.severity);
            self.targets.add(finding.target);
        }
    }

    /// The counters as JSON fields, each count mapped through `leaf`;
    /// `tasks` is left out when `with_tasks` is false.
    fn fields(&self, leaf: &dyn Fn(u64) -> Value, with_tasks: bool) -> Map<String, Value> {
        let mut fields = Map::new();
        if with_tasks {
            fields.insert("tasks".to_string(), leaf(self.tasks));
        }
        fields.insert("reviews".to_string(), leaf(self.reviews));
        fields.insert("re_reviews".to_string(), leaf(self.re_reviews));
        fields.insert("fix_rounds".to_string(), leaf(self.fix_rounds));
        fields.insert("report_bytes".to_string(), leaf(self.report_bytes));
        fields.insert("findings".to_string(), self.findings.render(leaf));
        fields.insert("targets".to_string(), self.targets.render(leaf));
        fields
    }
}

/// The `workspaces` key: one row per workspace, in the order given. Each row
/// ends with the workspace's `reversals`; `0` when it holds no `progress.md`.
pub(super) fn workspace_rows(workspaces: &[Workspace]) -> Value {
    Value::Array(
        workspaces
            .iter()
            .map(|workspace| {
                let mut row = Map::new();
                row.insert("path".to_string(), json!(workspace.path));
                row.extend(
                    Profile::of(std::slice::from_ref(workspace))
                        .fields(&|count| json!(count), true),
                );
                row.insert("reversals".to_string(), json!(workspace.reversals));
                Value::Object(row)
            })
            .collect(),
    )
}

/// The `YYYY-MM-DD` prefix of a workspace folder name, when it has one.
fn date_prefix(label: &str) -> Option<&str> {
    let prefix = label.get(..10)?;
    prefix
        .bytes()
        .enumerate()
        .all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        })
        .then_some(prefix)
}

/// Orders two workspace folder names from the oldest to the most recent: by
/// the `YYYY-MM-DD` prefix, then by name. A name with no date prefix sorts
/// before every dated one, and two such names sort by name.
pub(super) fn recency_cmp(left: &str, right: &str) -> Ordering {
    date_prefix(left)
        .cmp(&date_prefix(right))
        .then_with(|| left.cmp(right))
}

/// The `models` key: one row per model family or other model value, sorted by
/// row name, each with the cost fields of a `workspaces` row summed over every
/// workspace. Workspaces that hold no deliverable give an empty list, never a
/// row of zeros.
pub(super) fn model_rows(workspaces: &[Workspace]) -> Value {
    let mut profiles: BTreeMap<&str, Profile> = BTreeMap::new();
    for workspace in workspaces {
        for (model, cost) in &workspace.models {
            profiles.entry(model).or_default().add_model(cost);
        }
    }
    Value::Array(
        profiles
            .into_iter()
            .map(|(model, profile)| {
                let mut row = Map::new();
                row.insert("model".to_string(), json!(model));
                row.extend(profile.fields(&|count| json!(count), true));
                Value::Object(row)
            })
            .collect(),
    )
}

/// `total` divided by `tasks`, rounded to two decimals, or the token
/// `none` when there is no task to divide by.
fn per_task(total: u64, tasks: u64) -> Value {
    if tasks == 0 {
        return json!(NO_MEAN);
    }
    json!(((total as f64 / tasks as f64) * 100.0).round() / 100.0)
}

/// The `cost` key: `totals` sums the workspace rows; `per_task` divides
/// each total but `tasks` by the total tasks.
pub(super) fn cost(workspaces: &[Workspace]) -> Value {
    let total = Profile::of(workspaces);
    json!({
        "totals": Value::Object(total.fields(&|count| json!(count), true)),
        "per_task": Value::Object(total.fields(&|count| per_task(count, total.tasks), false)),
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn finding(severity: Severity, target: Target) -> Finding {
        Finding {
            severity,
            target,
            rule: None,
            issue: Value::Null,
        }
    }

    /// The workspace label every target test classifies against.
    const LABEL: &str = "batch-9";

    fn target_of(file: &str) -> Target {
        target(Some(file), LABEL)
    }

    /// A finding with no `file` field, or one that is not a string, has
    /// the target `other`.
    #[test]
    fn target_is_other_for_an_absent_or_non_string_file() {
        assert_eq!(target(None, LABEL), Target::Other);
        assert_eq!(
            Finding::from_issue(&json!({"severity": "minor"}), LABEL).target,
            Target::Other
        );
        assert_eq!(
            Finding::from_issue(&json!({"file": 7}), LABEL).target,
            Target::Other
        );
        assert_eq!(
            Finding::from_issue(&json!({"file": null}), LABEL).target,
            Target::Other
        );
    }

    /// A blank `file` is an absent slot, not a path.
    #[test]
    fn target_is_other_for_a_blank_file() {
        assert_eq!(target_of(""), Target::Other);
        assert_eq!(target_of("  "), Target::Other);
    }

    /// A path that holds the workspace's last component, or whose file
    /// name is a report, review, or audit deliverable, is a
    /// `deliverable`.
    #[test]
    fn target_is_deliverable_inside_the_workspace_or_for_a_deliverable_name() {
        for file in [
            ".superpowers/sdd/batch-9/task-1-report.json",
            ".superpowers/sdd/batch-9/notes.md",
            ".superpowers/sdd/batch-9/task-1-report.json (tests[0], the live run)",
            "task-1-report.json",
            "task-2-review.json",
            "task-2-review-r1.json:30",
            "task-3-audit.json",
            "task-3-audit-r2.json",
            "elsewhere/x-review.json",
            "-report.json",
        ] {
            assert_eq!(target_of(file), Target::Deliverable, "{file}");
        }
    }

    /// A path that ends in `.md` or starts with `docs/`, `knowledge/`, or
    /// `backlog/` is `prose`. A trailing `:<line>` does not hide the
    /// extension.
    #[test]
    fn target_is_prose_for_markdown_docs_knowledge_and_backlog() {
        for file in [
            "README.md",
            "template/docs/guide.md:3",
            "docs/design.rs",
            "knowledge/process.json",
            "backlog/items/kit.json:9",
        ] {
            assert_eq!(target_of(file), Target::Prose, "{file}");
        }
    }

    /// A `file` names a commit message when, in lower case, it starts with
    /// a commit id (7 to 40 hexadecimal characters, then the end, a space,
    /// or `..`), starts with `commit ` and a commit id, starts with
    /// `git log` or `git history`, or contains `commit message`,
    /// `commit body`, or `commit header`.
    #[test]
    fn target_is_prose_for_a_file_that_names_a_commit_message() {
        for file in [
            "13ba820 (commit message body)",
            "84f3941 and e4d4148 (commit message bodies)",
            "c9851cb (commit body)",
            "d7ee7d9 and 992a8a5 (commit headers); the planned third aggregate",
            "a462451",
            "A462451",
            "a462451:12",
            "commit 713cda6 (body)",
            "COMMIT 713CDA6 (body)",
            "commit 713cda6",
            "git log 3cc86e6..96b1201",
            "Git Log 3cc86e6..96b1201",
            "git history 9d0bb10..ed25cb7",
            "3cc86e6..96b1201",
            "Commit message",
            "the COMMIT body",
            "the commit header of the squash",
        ] {
            assert_eq!(target_of(file), Target::Prose, "{file}");
        }
        assert_eq!(target_of(&"a".repeat(40)), Target::Prose, "40 characters");
    }

    /// A path that holds the word `commit` as part of a file or directory
    /// name is code, however it is spelled.
    #[test]
    fn target_is_code_for_a_path_that_holds_the_word_commit() {
        for file in [
            "crates/houserules/src/rules/check_commit.rs:34",
            "crates/houserules/src/rules/check_commit.rs:117-118",
            "crates/houserules/tests/check_commit.rs",
            "template/.githooks/commit-msg",
            "template/.githooks/commit-msg:14",
            "tests/commit-msg-hook.test.mjs:19",
            "crates/houserules/tests/commit_msg_hook.rs",
            "src/commit_service.ts",
            "commit_service.ts",
            "Commit/Service.ts",
            "tools/commit-lint/index.js",
            "tests/commit_message_test.rs",
        ] {
            assert_eq!(target_of(file), Target::Code, "{file}");
        }
    }

    /// The commit-id edge: 6 and 41 hexadecimal characters are no id, and
    /// an id must end at the end, a space, or `..`.
    #[test]
    fn target_reads_a_commit_id_only_at_its_boundaries() {
        for file in [
            "abcdef",
            "a462451x",
            "a462451, more text",
            "a462451/lib.rs",
            "commit abcdef (body)",
            "commit a462451, message",
            "commits a462451",
        ] {
            assert_eq!(target_of(file), Target::Code, "{file}");
        }
        assert_eq!(target_of(&"a".repeat(41)), Target::Code, "41 characters");
    }

    /// Any other path is `code`, with or without a `:<line>` suffix.
    #[test]
    fn target_is_code_for_every_other_path() {
        for file in [
            "crates/houserules/src/lib.rs",
            "crates/houserules/src/lib.rs:120",
            "tools/kb.mjs",
            "report.json",
            "my-review.txt",
            "C:\\work\\lib.rs",
        ] {
            assert_eq!(target_of(file), Target::Code, "{file}");
        }
    }

    /// Only a `:<line>` or `:<line>-<line>` tail is a line locator; the
    /// deliverable and prose checks run on the path without it.
    #[test]
    fn target_strips_only_a_line_or_line_range_suffix() {
        assert_eq!(target_of("src/lib.rs:12"), Target::Code);
        assert_eq!(target_of("notes.md:12"), Target::Prose);
        assert_eq!(target_of("notes.md:x"), Target::Code);
        assert_eq!(target_of("notes.md:"), Target::Code);
        assert_eq!(target_of("notes.md:12-"), Target::Code);
        assert_eq!(target_of("notes.md:-12"), Target::Code);
        assert_eq!(target_of("notes.md:1-2-3"), Target::Code);
        assert_eq!(target_of("notes.md:1,2"), Target::Code);
    }

    /// A line range tail is a line locator: it hides neither the
    /// extension nor the file name.
    #[test]
    fn target_sees_through_a_line_range_suffix() {
        assert_eq!(target_of("README.md:3-5"), Target::Prose);
        assert_eq!(target_of("crates/x.rs:12-30"), Target::Code);
        assert_eq!(
            target_of(".superpowers/sdd/batch-9/task-1-report.json:3-5"),
            Target::Deliverable
        );
        assert_eq!(
            target_of("task-2-review-r1.json:30-41"),
            Target::Deliverable
        );
        assert_eq!(target_of("knowledge/process.json:7-9"), Target::Prose);
    }

    /// The deliverable check wins over the prose check.
    #[test]
    fn target_deliverable_wins_over_prose() {
        assert_eq!(target_of("docs/task-1-report.json"), Target::Deliverable);
        assert_eq!(target_of("commit/task-2-review.json"), Target::Deliverable);
    }

    /// Severity reads the issue's `severity` string; a missing,
    /// non-string, or unknown value is `other`.
    #[test]
    fn severity_is_other_for_a_missing_or_unknown_value() {
        let severity = |issue: Value| Finding::from_issue(&issue, LABEL).severity;
        assert_eq!(
            severity(json!({"severity": "critical"})),
            Severity::Critical
        );
        assert_eq!(
            severity(json!({"severity": "important"})),
            Severity::Important
        );
        assert_eq!(severity(json!({"severity": "minor"})), Severity::Minor);
        assert_eq!(severity(json!({})), Severity::Other);
        assert_eq!(severity(json!({"severity": 3})), Severity::Other);
        assert_eq!(severity(json!({"severity": "blocker"})), Severity::Other);
        assert_eq!(severity(json!("not an object")), Severity::Other);
    }

    /// An issue's `rule` is kept only when it is a string.
    #[test]
    fn a_finding_keeps_its_rule_only_when_it_is_a_string() {
        assert_eq!(
            Finding::from_issue(&json!({"rule": "a.rule"}), LABEL).rule,
            Some("a.rule".to_string())
        );
        assert_eq!(Finding::from_issue(&json!({"rule": 4}), LABEL).rule, None);
        assert_eq!(Finding::from_issue(&json!({}), LABEL).rule, None);
    }

    /// A finding is soft when it is minor or hits a deliverable. Prose is a
    /// contract surface (design.md 5.93): a finding on it is soft only
    /// when it is minor.
    #[test]
    fn a_finding_is_soft_when_minor_or_aimed_at_a_deliverable() {
        assert!(finding(Severity::Minor, Target::Code).is_soft());
        assert!(finding(Severity::Minor, Target::Prose).is_soft());
        assert!(finding(Severity::Critical, Target::Deliverable).is_soft());
        assert!(finding(Severity::Important, Target::Deliverable).is_soft());
        assert!(!finding(Severity::Important, Target::Prose).is_soft());
        assert!(!finding(Severity::Critical, Target::Prose).is_soft());
        assert!(!finding(Severity::Important, Target::Code).is_soft());
        assert!(!finding(Severity::Other, Target::Other).is_soft());
        assert!(!finding(Severity::Other, Target::Prose).is_soft());
    }

    fn workspace(path: &str, tasks: usize, findings: Vec<Finding>) -> Workspace {
        Workspace {
            path: path.to_string(),
            tasks,
            reviews: tasks * 2,
            re_reviews: 1,
            fix_rounds: 3,
            report_bytes: 1000,
            findings,
            ..Workspace::default()
        }
    }

    /// Each `workspaces` row carries the path as given, the counters, the
    /// findings by severity and by target, and the reversals, in this key
    /// order.
    #[test]
    fn workspace_rows_carry_every_field_in_order() {
        let rows = workspace_rows(&[workspace(
            "ws/",
            2,
            vec![
                finding(Severity::Critical, Target::Code),
                finding(Severity::Minor, Target::Prose),
                finding(Severity::Minor, Target::Deliverable),
                finding(Severity::Other, Target::Other),
            ],
        )]);
        let expected = json!([{
            "path": "ws/",
            "tasks": 2, "reviews": 4, "re_reviews": 1, "fix_rounds": 3, "report_bytes": 1000,
            "findings": {"critical": 1, "important": 0, "minor": 2, "other": 1},
            "targets": {"code": 1, "deliverable": 1, "prose": 1, "other": 1},
            "reversals": 0,
        }]);
        assert_eq!(rows, expected);
        let keys: Vec<&str> = rows[0]
            .as_object()
            .expect("row object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "path",
                "tasks",
                "reviews",
                "re_reviews",
                "fix_rounds",
                "report_bytes",
                "findings",
                "targets",
                "reversals"
            ]
        );
    }

    /// `cost.totals` sums the rows; `cost.per_task` divides each total
    /// but `tasks` by the total tasks and rounds to two decimals.
    #[test]
    fn cost_sums_the_rows_and_divides_by_the_total_tasks() {
        let value = cost(&[
            workspace("a", 1, vec![finding(Severity::Critical, Target::Code)]),
            workspace(
                "b",
                2,
                vec![
                    finding(Severity::Minor, Target::Prose),
                    finding(Severity::Minor, Target::Prose),
                ],
            ),
        ]);
        assert_eq!(
            value["totals"],
            json!({
                "tasks": 3, "reviews": 6, "re_reviews": 2, "fix_rounds": 6, "report_bytes": 2000,
                "findings": {"critical": 1, "important": 0, "minor": 2, "other": 0},
                "targets": {"code": 1, "deliverable": 0, "prose": 2, "other": 0},
            })
        );
        assert_eq!(
            value["per_task"],
            json!({
                "reviews": 2.0, "re_reviews": 0.67, "fix_rounds": 2.0, "report_bytes": 666.67,
                "findings": {"critical": 0.33, "important": 0.0, "minor": 0.67, "other": 0.0},
                "targets": {"code": 0.33, "deliverable": 0.0, "prose": 0.67, "other": 0.0},
            })
        );
    }

    /// With no tasks every per-task mean is the token `none`; the totals
    /// stay numbers.
    #[test]
    fn cost_per_task_is_none_at_zero_tasks() {
        let value = cost(&[workspace("a", 0, vec![])]);
        assert_eq!(value["totals"]["tasks"], json!(0));
        assert_eq!(value["totals"]["fix_rounds"], json!(3));
        assert_eq!(value["per_task"]["fix_rounds"], json!("none"));
        assert_eq!(value["per_task"]["findings"]["minor"], json!("none"));
        assert_eq!(value["per_task"]["targets"]["other"], json!("none"));
        assert_eq!(cost(&[])["per_task"]["reviews"], json!("none"));
    }

    /// A workspace that holds `cost` for `model`, with the counters of the
    /// whole workspace set from it.
    fn workspace_of_model(model: &str, cost: ModelCost) -> Workspace {
        let mut workspace = Workspace::default();
        workspace.models.insert(model.to_string(), cost);
        workspace.sum_models();
        workspace
    }

    /// The counters of a workspace are the sums over its models: tasks,
    /// reviews, re-reviews, fix rounds, report bytes, and every finding.
    #[test]
    fn sum_models_sets_the_workspace_counters_from_its_models() {
        let mut workspace = Workspace::default();
        workspace.model("m1").tasks = 2;
        workspace.model("m1").fix_rounds = 3;
        workspace.model("m1").report_bytes = 100;
        workspace.model("m2").reviews = 4;
        workspace.model("m2").re_reviews = 1;
        workspace
            .model("m2")
            .findings
            .push(finding(Severity::Minor, Target::Code));
        workspace.sum_models();
        assert_eq!(
            (
                workspace.tasks,
                workspace.reviews,
                workspace.re_reviews,
                workspace.fix_rounds,
                workspace.report_bytes,
                workspace.findings.len(),
            ),
            (2, 4, 1, 3, 100, 1)
        );
    }

    /// `models` rows carry the model first, then the fields of a `workspaces`
    /// row but its path, and sort by model name.
    #[test]
    fn model_rows_carry_the_model_then_the_cost_fields_sorted_by_model() {
        let rows = model_rows(&[
            workspace_of_model(
                "zeta",
                ModelCost {
                    reviews: 1,
                    ..ModelCost::default()
                },
            ),
            workspace_of_model(
                PRE_2_0,
                ModelCost {
                    tasks: 1,
                    report_bytes: 10,
                    ..ModelCost::default()
                },
            ),
        ]);
        let keys: Vec<&str> = rows[0]
            .as_object()
            .expect("row object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "model",
                "tasks",
                "reviews",
                "re_reviews",
                "fix_rounds",
                "report_bytes",
                "findings",
                "targets"
            ]
        );
        let models: Vec<&Value> = rows
            .as_array()
            .expect("an array")
            .iter()
            .map(|row| &row["model"])
            .collect();
        assert_eq!(models, [&json!("pre-2.0"), &json!("zeta")]);
    }

    /// A model that wrote deliverables in two workspaces has one row, summed.
    #[test]
    fn model_rows_sum_a_model_over_the_workspaces() {
        let one = |tasks, findings| ModelCost {
            tasks,
            findings,
            ..ModelCost::default()
        };
        let rows = model_rows(&[
            workspace_of_model("m", one(1, vec![finding(Severity::Critical, Target::Code)])),
            workspace_of_model("m", one(2, vec![finding(Severity::Minor, Target::Prose)])),
        ]);
        assert_eq!(rows.as_array().expect("an array").len(), 1);
        assert_eq!(rows[0]["tasks"], json!(3));
        assert_eq!(
            rows[0]["findings"],
            json!({"critical": 1, "important": 0, "minor": 1, "other": 0})
        );
        assert_eq!(
            rows[0]["targets"],
            json!({"code": 1, "deliverable": 0, "prose": 1, "other": 0})
        );
    }

    /// No workspace, or a workspace with no deliverable, gives no rows.
    #[test]
    fn model_rows_are_empty_with_no_deliverable() {
        assert_eq!(model_rows(&[]), json!([]));
        assert_eq!(model_rows(&[Workspace::default()]), json!([]));
    }

    /// A workspace row ends with its `reversals`.
    #[test]
    fn a_workspace_row_ends_with_its_reversals() {
        let rows = workspace_rows(&[Workspace {
            reversals: 4,
            ..Workspace::default()
        }]);
        let keys: Vec<&str> = rows[0]
            .as_object()
            .expect("a row object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.last(), Some(&"reversals"));
        assert_eq!(rows[0]["reversals"], json!(4));
    }

    /// A name that starts with `dddd-dd-dd` has a date prefix. A shorter name,
    /// a name of another shape, and a name whose tenth byte is inside a
    /// multi-byte character have none, and none of them panics.
    #[test]
    fn date_prefix_is_ten_characters_of_the_shape_dddd_dd_dd() {
        assert_eq!(date_prefix("2026-10-21-batch-35"), Some("2026-10-21"));
        assert_eq!(date_prefix("2026-10-21"), Some("2026-10-21"));
        for name in [
            "",
            "batch-35",
            "2026-10-2",
            "2026/10/21-x",
            "2026-1a-21-x",
            "2026-10-1äx",
        ] {
            assert_eq!(date_prefix(name), None, "{name}");
        }
    }

    /// Dated names sort by their date, then by name; an undated name sorts
    /// before every dated one, and undated names sort by name.
    #[test]
    fn recency_orders_by_date_then_name_with_undated_names_first() {
        let mut names = [
            "2026-10-10-batch-33",
            "zulu",
            "2026-10-09-batch-34",
            "alpha",
            "2026-10-10-batch-32",
        ];
        names.sort_by(|left, right| recency_cmp(left, right));
        assert_eq!(
            names,
            [
                "alpha",
                "zulu",
                "2026-10-09-batch-34",
                "2026-10-10-batch-32",
                "2026-10-10-batch-33",
            ]
        );
    }
}
