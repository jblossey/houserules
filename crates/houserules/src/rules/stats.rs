//! The `stats` command: aggregates rule violations and unused injected
//! ids across the JSON deliverables of one or more workspaces, and
//! measures what the process cost. See `deliverable.rs`'s module doc for
//! why every read here is a tolerant `serde_json::Value`, never a typed
//! deliverable model.
//!
//! This module reads the files and keeps the four original keys
//! (`violations`, `unused_ids`, `audits`, `reviews`) and the `sanctioned`
//! list. `cost.rs` renders the `workspaces`, `models`, and `cost` keys;
//! `proposals.rs` renders `rules`, `packs`, and `proposals`;
//! `review_coverage.rs` renders the `review_coverage` key of `--records`.
//!
//! A ledger line is a reversal when it starts with `Reversal:` after optional
//! spaces and an optional list marker. `stats` warns on stderr about each other
//! line that holds `Reversal:` (spec 11.6): the count of a workspace stays the
//! number of reversals.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::emit::emit;

use super::cost::{self, AdherenceRow, Finding, Sanction, Workspace};
use super::deliverable::{
    BRANCH_REVIEW_FILE, array_field, read_deliverable_value, stats_reads_phrase, workspace_files,
};
use super::model::{Base, load_base};
use super::proposals;
use super::review_coverage::review_coverage;

/// Records that task `task` triggered (or violated) rule `id`,
/// deduplicating repeats.
fn stats_hit(map: &mut BTreeMap<String, BTreeSet<String>>, id: &str, task: &str) {
    map.entry(id.to_string())
        .or_default()
        .insert(task.to_string());
}

/// The task label between `task-` and the first following `-` in a
/// deliverable filename. `workspace_files` already restricts its
/// callers to names starting `task-<something>`, so the fallback (the
/// whole name) is unreached in practice; it exists so this never panics
/// on an unexpected shape (`quality.principles`).
fn stats_task(name: &str) -> &str {
    match name.strip_prefix("task-") {
        Some(rest) => match rest.find('-') {
            Some(0) | None => name,
            Some(end) => &rest[..end],
        },
        None => name,
    }
}

/// Renders `tasks` as the sorted `Vec<&str>` -> JSON array `stats`
/// reports under `tasks`. `BTreeSet` already iterates sorted, so this is
/// just the JSON shape.
fn tasks_json(tasks: &BTreeSet<String>) -> Value {
    Value::Array(tasks.iter().cloned().map(Value::String).collect())
}

/// What `stats` tells a person whose workspace holds a file it cannot
/// read as a deliverable. `stats` aborts on such a file
/// (`quality.gates-derive-their-scope`); this text names the way out. It
/// names the files from the one list in `deliverable.rs`.
fn unreadable_deliverable_remedy() -> String {
    format!(
        "stats reads every {} in the workspace as a deliverable; rename the file or repair it",
        stats_reads_phrase()
    )
}

/// Reads one workspace deliverable with `read_deliverable_value`, then
/// appends `unreadable_deliverable_remedy` to a failure. The shared reader
/// keeps its own text: `validate` and `audit` read through it too.
fn read_workspace_deliverable(path: &Path) -> Result<Value, String> {
    read_deliverable_value(path)
        .map_err(|error| format!("{error}. {}", unreadable_deliverable_remedy()))
}

/// The model families that `models` rows group by.
const MODEL_FAMILIES: [&str; 4] = ["fable", "haiku", "opus", "sonnet"];

/// The `models` row of the `model` value `model`: the name of its family when
/// it names exactly one as a whole word, whether it is the alias (`opus`) or an
/// id (`claude-opus-5-5`, `us.anthropic.claude-opus-5-5-v1:0`), whatever its
/// case. A value that names no family, or two, keeps its own row, as `human`,
/// `unknown`, and `pre-2.0` do. A dispatch that names the alias and one that
/// names an id then count one model in one row.
fn model_row(model: &str) -> String {
    let lowered = model.to_ascii_lowercase();
    let mut named = MODEL_FAMILIES.iter().filter(|family| {
        lowered
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|word| word == **family)
    });
    match (named.next(), named.next()) {
        (Some(family), None) => (*family).to_string(),
        _ => model.to_string(),
    }
}

/// The `models` row of the model that wrote the deliverable `data`
/// (`model_row` of its `model` string), or `cost::PRE_2_0` when the field is
/// absent or `null`. A `model` that is no non-empty string is a named error
/// with `name`, as a field of the wrong type is everywhere else in `stats`:
/// reading it as `pre-2.0` would hide the fault
/// (`houserules.crash-paths-are-named`).
fn model_of(data: &Value, name: &str) -> Result<String, String> {
    match data.get("model") {
        None | Some(Value::Null) => Ok(cost::PRE_2_0.to_string()),
        Some(Value::String(model)) if !model.is_empty() => Ok(model_row(model)),
        Some(_) => Err(format!("{name}: model is not a non-empty string")),
    }
}

/// One workspace directory named on the command line, with the label that
/// prefixes its task ids when several workspaces are read.
struct Source<'a> {
    /// The path as the caller gave it; every file is read through it.
    given: &'a Path,
    /// The last path component, never empty.
    label: String,
}

/// The label of a workspace: the last component of the path as given
/// (`Path::file_name` ignores a trailing separator), else the last
/// component of its canonical form (the given path ends in `.` or `..`),
/// else the path itself (a filesystem root).
fn workspace_label(given: &Path, canonical: &Path) -> String {
    given
        .file_name()
        .or_else(|| canonical.file_name())
        .map_or_else(
            || given.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
}

/// Resolves the command-line paths into the workspaces to read, in the
/// order given. A path whose canonical form an earlier path already named
/// is dropped, so one directory is read once. A path that cannot be
/// resolved is a named error with the operating system's own text. With a
/// `window`, only the most recent workspaces stay (`newest`), before any
/// workspace is read.
fn resolve_sources(dirs: &[PathBuf], window: Option<usize>) -> Result<Vec<Source<'_>>, String> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut sources = Vec::with_capacity(dirs.len());
    for given in dirs {
        let canonical =
            fs::canonicalize(given).map_err(|error| format!("{}: {error}", given.display()))?;
        if !seen.insert(canonical.clone()) {
            continue;
        }
        sources.push(Source {
            given,
            label: workspace_label(given, &canonical),
        });
    }
    Ok(match window {
        Some(window) => newest(sources, window),
        None => sources,
    })
}

/// The `window` most recent of `sources` by `cost::recency_cmp` of their
/// labels, in the order given. A `window` above the count keeps every source.
fn newest(sources: Vec<Source<'_>>, window: usize) -> Vec<Source<'_>> {
    if sources.len() <= window {
        return sources;
    }
    let mut by_recency: Vec<usize> = (0..sources.len()).collect();
    by_recency
        .sort_by(|&left, &right| cost::recency_cmp(&sources[left].label, &sources[right].label));
    let kept: HashSet<usize> = by_recency
        .split_off(sources.len() - window)
        .into_iter()
        .collect();
    sources
        .into_iter()
        .enumerate()
        .filter(|(index, _)| kept.contains(index))
        .map(|(_, source)| source)
        .collect()
}

/// What the four original keys aggregate across every workspace read.
#[derive(Default)]
struct Tally {
    violations: BTreeMap<String, BTreeSet<String>>,
    injected: BTreeMap<String, BTreeSet<String>>,
    cited: HashSet<String>,
    audit_files: usize,
    audit_tasks: HashSet<String>,
    review_files: usize,
    /// The lines for stderr, in the order of the workspaces and of the ledger
    /// lines (see [`count_reversals`]).
    warnings: Vec<String>,
}

/// Reads the deliverables of one workspace into a `Workspace`, feeding
/// the original keys into a `Tally` on the way.
struct Reader<'a> {
    dir: &'a Path,
    label: &'a str,
    /// Whether task labels carry the workspace label: true when several
    /// workspaces are read.
    prefix_tasks: bool,
}

impl Reader<'_> {
    /// The label of the task a deliverable `name` belongs to.
    fn task(&self, name: &str) -> String {
        if self.prefix_tasks {
            format!("{}/{}", self.label, stats_task(name))
        } else {
            stats_task(name).to_string()
        }
    }

    /// Reads the deliverable `name` of the workspace.
    fn read(&self, name: &str) -> Result<Value, String> {
        read_workspace_deliverable(&self.dir.join(name))
    }
}

/// One audit or `rule_adherence` row that names an entry.
struct Row<'a> {
    id: &'a str,
    /// The row's `result` is `fail` and no sanction accepts it.
    failed: bool,
    deterministic: bool,
    judged: bool,
    /// The row's `result` is `pass` or `fail`.
    verdict: bool,
    /// The reference of the sanction that accepts the row, when its `result`
    /// is `fail`.
    sanction: Option<String>,
    /// The row carries an `override`.
    overridden: bool,
}

/// The text before a reference in the suffix that ends the evidence of a
/// deterministic `fail` row that a `--sanctioned` booking accepted. An audit
/// file can hold the suffix on a row that has no `sanctioned` field.
const SANCTION_SUFFIX_OPENER: &str = "(sanctioned: ";

/// The reference at the end of `evidence`, when the evidence ends with
/// `(sanctioned: <reference>)`.
fn sanction_in_evidence(evidence: &str) -> Option<&str> {
    let (_, reference) = evidence
        .strip_suffix(')')?
        .rsplit_once(SANCTION_SUFFIX_OPENER)?;
    (!reference.is_empty()).then_some(reference)
}

/// The `sanctioned` field of `row`: `None` when it is absent or `null`, else
/// the reference. A field that is no non-empty string is an error text.
fn sanctioned_field(row: &Value) -> Result<Option<String>, &'static str> {
    match row.get("sanctioned") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(reference)) if !reference.is_empty() => Ok(Some(reference.clone())),
        Some(_) => Err("sanctioned is not a non-empty string"),
    }
}

/// Whether `row` carries an `override`: `false` when the field is absent or
/// `null`. A field that is no object is an error text.
fn has_override(row: &Value) -> Result<bool, &'static str> {
    match row.get("override") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Object(_)) => Ok(true),
        Some(_) => Err("override is not an object"),
    }
}

/// The rows of `data[field]` that carry a string `id`, or an error naming
/// `name` and the field when the field is no array, and naming the row when
/// its `sanctioned` or `override` field has the wrong type.
fn rows_of<'a>(data: &'a Value, field: &str, name: &str) -> Result<Vec<Row<'a>>, String> {
    let rows = array_field(data, field).map_err(|error| format!("{name}: {error}"))?;
    let mut parsed = Vec::with_capacity(rows.len());
    for row in rows {
        let Some(id) = row.get("id").and_then(Value::as_str) else {
            continue;
        };
        let named = |fault: &str| format!("{name}: {field} row \"{id}\": {fault}");
        let reference = sanctioned_field(row).map_err(named)?;
        let overridden = has_override(row).map_err(named)?;
        let result = row.get("result").and_then(Value::as_str);
        let mode = row.get("mode").and_then(Value::as_str);
        let deterministic = mode == Some("deterministic");
        let sanction = if result == Some("fail") {
            reference.or_else(|| {
                let evidence = row.get("evidence").and_then(Value::as_str)?;
                let reference = sanction_in_evidence(evidence)?;
                deterministic.then(|| reference.to_string())
            })
        } else {
            None
        };
        parsed.push(Row {
            id,
            failed: result == Some("fail") && sanction.is_none(),
            deterministic,
            judged: mode == Some("judged"),
            verdict: matches!(result, Some("pass" | "fail")),
            sanction,
            overridden,
        });
    }
    Ok(parsed)
}

/// Where a row came from decides which rows count and what a counted row
/// costs.
#[derive(Clone, Copy)]
enum RowSource {
    /// An audit file: its deterministic rows count.
    Audit,
    /// A review file: its judged rows count, and the row adds `share` bytes
    /// of the file to the cost of its rule.
    Review { share: u64 },
}

/// Adds `row` of the deliverable of `task` to `workspace`. A counted row that
/// a sanction accepts also joins the workspace's `sanctions`. A judged row of
/// a review with a `pass` or `fail` result is a verdict; an override counts
/// only on a verdict.
fn record_row(workspace: &mut Workspace, task: &str, row: &Row, source: RowSource) {
    let (counted, cost) = match source {
        RowSource::Audit => (row.deterministic, None),
        RowSource::Review { share } => (row.judged, row.judged.then_some(share)),
    };
    let verdict = cost.is_some() && row.verdict;
    if counted && let Some(reference) = &row.sanction {
        workspace.sanctions.push(Sanction {
            id: row.id.to_string(),
            task: task.to_string(),
            reference: reference.clone(),
        });
    }
    workspace.rows.push(AdherenceRow {
        id: row.id.to_string(),
        failed: row.failed,
        counted,
        verdict,
        overridden: verdict && row.overridden,
        sanctioned: row.sanction.clone(),
        cost,
    });
}

/// The findings of `data[field]`, in file order.
fn read_findings(
    data: &Value,
    field: &str,
    name: &str,
    label: &str,
) -> Result<Vec<Finding>, String> {
    let issues = array_field(data, field).map_err(|error| format!("{name}: {error}"))?;
    Ok(issues
        .into_iter()
        .map(|issue| Finding::from_issue(issue, label))
        .collect())
}

/// Reads the audits: injected ids and failed rules.
fn read_audits(
    reader: &Reader,
    names: &[String],
    tally: &mut Tally,
    workspace: &mut Workspace,
) -> Result<(), String> {
    for name in names {
        let data = reader.read(name)?;
        let task = reader.task(name);
        tally.audit_files += 1;
        tally.audit_tasks.insert(task.clone());
        let ids = array_field(&data, "ids").map_err(|error| format!("{name}: {error}"))?;
        for id in ids.into_iter().filter_map(Value::as_str) {
            stats_hit(&mut tally.injected, id, &task);
            workspace.injected.insert(id.to_string());
        }
        for row in rows_of(&data, "rules", name)? {
            if row.failed {
                stats_hit(&mut tally.violations, row.id, &task);
            }
            record_row(workspace, &task, &row, RowSource::Audit);
        }
    }
    Ok(())
}

/// The review files `read_review` reads; the kind decides the field that holds
/// the findings.
#[derive(Clone, Copy)]
enum ReviewFile {
    /// A task review, a re-review, or the branch review.
    Task,
    /// A spec review: its findings are its `findings` rows.
    Spec,
}

/// Reads the review fields every review kind shares -- `rule_adherence`,
/// the findings, and the model that wrote the file -- into `workspace`. The
/// findings of a spec review are its `findings` rows; those of a `re-review`
/// are its `new_breakage` rows; those of any other review are its `issues`.
/// The first judged row of a rule costs the rule the file's bytes divided by
/// all the judged rows of the review, rounded down; each further judged row of
/// the same rule in the review costs it nothing, so a review adds one share to
/// the cost of a rule whatever number of its rows judge the rule. The
/// `issues` of a task review and of the branch review also join the
/// workspace's `issues`, in file order. Returns the judged failed rows, which
/// only a task review counts as violations.
fn read_review<'a>(
    reader: &Reader,
    name: &str,
    data: &'a Value,
    file: ReviewFile,
    workspace: &mut Workspace,
) -> Result<Vec<Row<'a>>, String> {
    let model = model_of(data, name)?;
    let re_review = data.get("kind").and_then(Value::as_str) == Some("re-review");
    let rows = rows_of(data, "rule_adherence", name)?;
    let path = reader.dir.join(name);
    let bytes = fs::metadata(&path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .len();
    let judged_rows = rows.iter().filter(|row| row.judged).count() as u64;
    let share = bytes.checked_div(judged_rows).unwrap_or(0);
    let task = reader.task(name);
    let mut charged: BTreeSet<&str> = BTreeSet::new();
    for row in &rows {
        let share = if row.judged && !charged.insert(row.id) {
            0
        } else {
            share
        };
        record_row(workspace, &task, row, RowSource::Review { share });
    }
    let (field, is_issues) = match file {
        ReviewFile::Spec => ("findings", false),
        ReviewFile::Task if re_review => ("new_breakage", false),
        ReviewFile::Task => ("issues", true),
    };
    let findings = read_findings(data, field, name, reader.label)?;
    if is_issues {
        workspace.issues.extend(findings.iter().cloned());
    }
    let cost = workspace.model(&model);
    cost.reviews += 1;
    if re_review {
        cost.re_reviews += 1;
    }
    cost.findings.extend(findings);
    Ok(rows)
}

/// Reads the task reviews: judged failures and findings.
fn read_reviews(
    reader: &Reader,
    names: &[String],
    tally: &mut Tally,
    workspace: &mut Workspace,
) -> Result<(), String> {
    for name in names {
        let data = reader.read(name)?;
        let task = reader.task(name);
        tally.review_files += 1;
        for row in read_review(reader, name, &data, ReviewFile::Task, workspace)? {
            if row.judged && row.failed {
                stats_hit(&mut tally.violations, row.id, &task);
            }
        }
    }
    Ok(())
}

/// Reads `branch-review.json` when the workspace holds it. Its rows and
/// findings join the workspace; it feeds no original key.
fn read_branch_review(reader: &Reader, workspace: &mut Workspace) -> Result<(), String> {
    let path = reader.dir.join(BRANCH_REVIEW_FILE);
    let present = path
        .try_exists()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !present {
        return Ok(());
    }
    let data = reader.read(BRANCH_REVIEW_FILE)?;
    read_review(
        reader,
        BRANCH_REVIEW_FILE,
        &data,
        ReviewFile::Task,
        workspace,
    )?;
    Ok(())
}

/// Reads the spec reviews. Each counts as a review, with its findings; none
/// feeds an original key, since a spec review names no task.
fn read_spec_reviews(
    reader: &Reader,
    names: &[String],
    workspace: &mut Workspace,
) -> Result<(), String> {
    for name in names {
        let data = reader.read(name)?;
        read_review(reader, name, &data, ReviewFile::Spec, workspace)?;
    }
    Ok(())
}

/// Reads the reports: cited ids, fix rounds, file sizes, and the model that
/// wrote each.
fn read_reports(
    reader: &Reader,
    names: &[String],
    tally: &mut Tally,
    workspace: &mut Workspace,
) -> Result<(), String> {
    for name in names {
        let data = reader.read(name)?;
        let path = reader.dir.join(name);
        let size = fs::metadata(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?
            .len();
        let model = model_of(&data, name)?;
        let knowledge_used =
            array_field(&data, "knowledge_used").map_err(|error| format!("{name}: {error}"))?;
        let cited_here: BTreeSet<&str> = knowledge_used
            .into_iter()
            .filter_map(Value::as_str)
            .collect();
        for id in cited_here {
            tally.cited.insert(id.to_string());
            workspace.cited.push(id.to_string());
        }
        let fix_rounds =
            array_field(&data, "fix_rounds").map_err(|error| format!("{name}: {error}"))?;
        let cost = workspace.model(&model);
        cost.tasks += 1;
        cost.report_bytes += size;
        cost.fix_rounds += fix_rounds.len();
    }
    Ok(())
}

/// The ledger of a workspace, whose `Reversal:` lines `stats` counts.
const PROGRESS_FILE: &str = "progress.md";

/// Whether `line` records a reversal: it starts with `Reversal:` after
/// optional leading spaces and an optional list marker (`- `, `* `, or
/// `+ `), the form that `process.reversals-are-recorded` teaches.
fn is_reversal(line: &str) -> bool {
    let line = line.trim_start_matches(' ');
    let line = ["- ", "* ", "+ "]
        .iter()
        .find_map(|marker| line.strip_prefix(marker))
        .unwrap_or(line);
    line.starts_with("Reversal:")
}

/// The text after `<ledger>:<line>: ` of the warning for a ledger line that
/// holds `Reversal:` and is no reversal (spec 11.6).
const EMBEDDED_REVERSAL: &str =
    "a Reversal: inside a longer entry; stats counts only a line that starts with it";

/// Whether `line` holds `Reversal:` and is no reversal: the word sits inside a
/// longer entry, and the count leaves the line out.
fn embeds_a_reversal(line: &str) -> bool {
    !is_reversal(line) && line.contains("Reversal:")
}

/// Counts the reversals that the `progress.md` of the workspace `dir` records,
/// and adds to `warnings` one line for each ledger line that embeds a reversal:
/// `<ledger>:<line>: ` and [`EMBEDDED_REVERSAL`], with the line number counted
/// from 1. A workspace with no such file has none; a file that cannot be read as
/// text is a named error with the operating system's own text.
fn count_reversals(dir: &Path, warnings: &mut Vec<String>) -> Result<usize, String> {
    let path = dir.join(PROGRESS_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => {
            warnings.extend(
                text.lines()
                    .enumerate()
                    .filter(|(_, line)| embeds_a_reversal(line))
                    .map(|(index, _)| {
                        format!("{}:{}: {EMBEDDED_REVERSAL}", path.display(), index + 1)
                    }),
            );
            Ok(text.lines().filter(|line| is_reversal(line)).count())
        }
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(0),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Reads one workspace: audits, task reviews, the branch review, spec
/// reviews, then reports, each in this order so a malformed file is named
/// the way it always was, and last the reversals of its ledger. The counters
/// of the whole workspace are the sums over its models
/// (`Workspace::sum_models`).
fn read_workspace(
    source: &Source,
    prefix_tasks: bool,
    tally: &mut Tally,
) -> Result<Workspace, String> {
    let files = workspace_files(source.given)?;
    let reader = Reader {
        dir: source.given,
        label: &source.label,
        prefix_tasks,
    };
    let mut workspace = Workspace {
        path: source.given.display().to_string(),
        label: source.label.clone(),
        ..Workspace::default()
    };
    read_audits(&reader, &files.audits, tally, &mut workspace)?;
    read_reviews(&reader, &files.reviews, tally, &mut workspace)?;
    read_branch_review(&reader, &mut workspace)?;
    read_spec_reviews(&reader, &files.spec_reviews, &mut workspace)?;
    read_reports(&reader, &files.reports, tally, &mut workspace)?;
    workspace.reversals = count_reversals(source.given, &mut tally.warnings)?;
    workspace.sum_models();
    Ok(workspace)
}

/// The `sanctioned` key: one object per counted fail that a sanction accepts
/// -- the entry `id`, the `workspace` path as given, the `task` label, and
/// the `ref` that books the sanction -- sorted by those four in that order.
fn sanctioned_json(workspaces: &[Workspace]) -> Vec<Value> {
    let mut entries: Vec<(&str, &str, &str, &str)> = workspaces
        .iter()
        .flat_map(|workspace| {
            workspace.sanctions.iter().map(move |sanction| {
                (
                    sanction.id.as_str(),
                    workspace.path.as_str(),
                    sanction.task.as_str(),
                    sanction.reference.as_str(),
                )
            })
        })
        .collect();
    entries.sort_unstable();
    entries
        .into_iter()
        .map(|(id, workspace, task, reference)| {
            json!({"id": id, "workspace": workspace, "task": task, "ref": reference})
        })
        .collect()
}

/// Aggregates the JSON deliverables of `dirs`: `task-*-audit*.json` for
/// injected ids and deterministic failures, `task-*-review*.json` and
/// `branch-review.json` for judged failures and findings,
/// `spec-review.json` and `spec-review-r<n>.json` for findings,
/// `task-*-report.json` for the ids a report cites as used and for the fix
/// rounds. The `models` key groups the cost of every deliverable but an audit
/// by the `model` that wrote it. The knowledge base `base` decides which
/// entries get a `rules` row.
///
/// With one workspace a task label is the task id; with several it is
/// `<workspace directory name>/<task id>`. A directory named twice, by
/// any spelling, is read once. With a `window`, only the `window` most
/// recent workspaces are read (`newest`), and the labels follow the count
/// that remains.
///
/// A fail that a sanction accepts is in the `sanctioned` list and the rule's
/// `sanctioned` count, and in neither `violations` nor the rule's `fails`. It
/// fires the rule for every proposal all the same, as a fail did before the
/// sanction existed.
///
/// The warnings are the lines that the command prints on stderr: one for each
/// ledger line that embeds a `Reversal:` (see [`count_reversals`]).
pub(super) fn stats(
    dirs: &[PathBuf],
    base: &Base,
    window: Option<usize>,
) -> Result<Outcome, String> {
    let sources = resolve_sources(dirs, window)?;
    let prefix_tasks = sources.len() > 1;
    let mut tally = Tally::default();
    let mut workspaces = Vec::with_capacity(sources.len());
    for source in &sources {
        workspaces.push(read_workspace(source, prefix_tasks, &mut tally)?);
    }

    let violations_json: Vec<Value> = tally
        .violations
        .iter()
        .map(|(id, tasks)| json!({"id": id, "count": tasks.len(), "tasks": tasks_json(tasks)}))
        .collect();
    let unused_ids_json: Vec<Value> = tally
        .injected
        .iter()
        .filter(|(id, _)| !tally.cited.contains(id.as_str()))
        .map(|(id, tasks)| json!({"id": id, "tasks": tasks_json(tasks)}))
        .collect();
    let rows = proposals::rule_rows(&workspaces, base);

    let value = json!({
        "violations": violations_json,
        "unused_ids": unused_ids_json,
        "audits": {"files": tally.audit_files, "tasks": tally.audit_tasks.len()},
        "reviews": {"files": tally.review_files},
        "sanctioned": sanctioned_json(&workspaces),
        "workspaces": cost::workspace_rows(&workspaces),
        "models": cost::model_rows(&workspaces),
        "cost": cost::cost(&workspaces),
        "rules": proposals::rules_json(&rows),
        "packs": proposals::packs_json(base, &rows, &tally.violations),
        "proposals": proposals::proposals(&rows),
    });
    Ok(Outcome {
        value,
        warnings: tally.warnings,
    })
}

/// What `stats` aggregated: the JSON result for stdout and the warnings for
/// stderr.
#[derive(Debug)]
pub(super) struct Outcome {
    /// The aggregated keys.
    pub(super) value: Value,
    /// The warning lines, in the order of the workspaces.
    pub(super) warnings: Vec<String>,
}

/// Runs the `stats` subcommand: refuses a `window` of 0, resolves `root`
/// (`--dir`, or the nearest install root), loads the knowledge base there
/// before it reads any workspace -- so a repository whose knowledge base
/// fails to load fails `stats` too, not only `audit` -- then runs `stats`
/// over the `window` most recent of `workspaces`, or all of them with no
/// window, prints its warnings on stderr, and prints its JSON result.
///
/// With `records`, the result also holds the key `review_coverage` for the
/// change records whose key is in that range (spec 8.3). With no workspace, it
/// holds that key alone, and the command reads no knowledge base: it reads git.
pub(crate) fn cmd_stats(
    dir: Option<PathBuf>,
    workspaces: Vec<PathBuf>,
    window: Option<usize>,
    records: Option<String>,
) -> ExitCode {
    if window == Some(0) {
        eprintln!("--window must be 1 or more");
        return ExitCode::from(2);
    }
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let (mut value, warnings) = if workspaces.is_empty() {
        (json!({}), Vec::new())
    } else {
        let base = match load_base(&root) {
            Ok(base) => base,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        };
        match stats(&workspaces, &base, window) {
            Ok(outcome) => (outcome.value, outcome.warnings),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        }
    };
    if let Some(range) = records {
        match review_coverage(&root, &range) {
            Ok(coverage) => value["review_coverage"] = coverage,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        }
    }
    for warning in warnings {
        eprintln!("{warning}");
    }
    print!("{}", emit(&value));
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;
    use crate::rules::test_support::{base_with, base_with_packs, entry, valid_check};

    fn write(dir: &Path, name: &str, value: &Value) {
        fs::write(dir.join(name), serde_json::to_string(value).unwrap()).unwrap();
    }

    fn empty_base() -> (TempDir, Base) {
        base_with(&[])
    }

    /// Runs `stats` over `dirs` against a base holding `entries`.
    fn run_with(entries: &[Value], dirs: &[&Path]) -> Value {
        let (_guard, base) = base_with(entries);
        let dirs: Vec<PathBuf> = dirs.iter().map(|dir| dir.to_path_buf()).collect();
        stats(&dirs, &base, None).unwrap().value
    }

    fn run(dirs: &[&Path]) -> Value {
        run_with(&[], dirs)
    }

    /// The four keys `stats` printed before it read several workspaces.
    fn old_keys(value: &Value) -> Value {
        json!({
            "violations": value["violations"],
            "unused_ids": value["unused_ids"],
            "audits": value["audits"],
            "reviews": value["reviews"],
        })
    }

    /// A directory `name` under `parent`.
    fn workspace_dir(parent: &Path, name: &str) -> PathBuf {
        let dir = parent.join(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stats_rules(result: &str) -> Value {
        json!([{
            "id": "a.rule", "kind": "rule", "mode": "deterministic",
            "level": "fail", "result": result, "evidence": "",
        }])
    }

    /// Aggregates violations, unused ids, and file counts from a
    /// workspace of JSON deliverables.
    #[test]
    fn aggregates_violations_unused_ids_and_file_counts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::write(
            root.join("task-1-audit.json"),
            serde_json::to_string(&json!({"ids": ["a.rule", "c.d"], "rules": stats_rules("fail")}))
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("task-2-audit-r1.json"),
            serde_json::to_string(&json!({"ids": ["a.rule"], "rules": stats_rules("pass")}))
                .unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("task-1-report.json"),
            serde_json::to_string(
                &json!({"kind": "task-report", "knowledge_used": ["a.rule", "b.c"]}),
            )
            .unwrap(),
        )
        .unwrap();
        // A non-JSON report file; `stats` matches only `task-*-report.json`,
        // so this must not affect the count.
        fs::write(
            root.join("task-1-report.md"),
            "# r\n\nKnowledge used: a.rule, b.c\n",
        )
        .unwrap();
        fs::write(
            root.join("task-2-review.json"),
            serde_json::to_string(&json!({
                "kind": "task-review",
                "rule_adherence": [
                    {"id": "x.y", "mode": "judged", "result": "fail", "evidence": "ev"},
                    {"id": "a.rule", "mode": "deterministic", "result": "pass", "evidence": "ok"},
                ],
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(root.join("unrelated.txt"), "").unwrap();

        assert_eq!(
            old_keys(&run(&[root])),
            json!({
                "violations": [
                    {"id": "a.rule", "count": 1, "tasks": ["1"]},
                    {"id": "x.y", "count": 1, "tasks": ["2"]},
                ],
                "unused_ids": [{"id": "c.d", "tasks": ["1"]}],
                "audits": {"files": 2, "tasks": 2},
                "reviews": {"files": 1},
            })
        );

        let empty_dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            old_keys(&run(&[empty_dir.path()])),
            json!({
                "violations": [],
                "unused_ids": [],
                "audits": {"files": 0, "tasks": 0},
                "reviews": {"files": 0},
            })
        );
    }

    /// The four old keys come first, then `sanctioned`, `workspaces`,
    /// `models`, `cost`, `rules`, `packs`, and `proposals`, in this order.
    #[test]
    fn the_new_keys_follow_the_four_old_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let value = run(&[dir.path()]);
        let keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "violations",
                "unused_ids",
                "audits",
                "reviews",
                "sanctioned",
                "workspaces",
                "models",
                "cost",
                "rules",
                "packs",
                "proposals"
            ]
        );
    }

    /// Tolerates an audit file with no `ids` or `rules` field, whether a
    /// generated stats file or a hand-written one.
    #[test]
    fn tolerates_an_audit_file_with_no_ids_or_rules() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("task-9-audit.json"), "{}").unwrap();
        assert_eq!(
            old_keys(&run(&[dir.path()])),
            json!({
                "violations": [],
                "unused_ids": [],
                "audits": {"files": 1, "tasks": 1},
                "reviews": {"files": 0},
            })
        );
    }

    /// Tolerates a review with no `rule_adherence` and a report with no
    /// `knowledge_used`.
    #[test]
    fn tolerates_a_review_with_no_rule_adherence_and_a_report_with_no_knowledge_used() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-review.json"),
            serde_json::to_string(&json!({"kind": "task-review"})).unwrap(),
        )
        .unwrap();
        fs::write(
            dir.path().join("task-1-report.json"),
            serde_json::to_string(&json!({"kind": "task-report"})).unwrap(),
        )
        .unwrap();
        assert_eq!(
            old_keys(&run(&[dir.path()])),
            json!({
                "violations": [],
                "unused_ids": [],
                "audits": {"files": 0, "tasks": 0},
                "reviews": {"files": 1},
            })
        );
    }

    /// Names a malformed deliverable file in the error, instead of
    /// crashing.
    #[test]
    fn reports_a_malformed_deliverable_file_naming_it_instead_of_crashing() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("task-3-audit.json"), "{\"ids\": [").unwrap();
        let (_guard, base) = empty_base();
        let error = stats(&[dir.path().to_path_buf()], &base, None).unwrap_err();
        assert!(error.contains("task-3-audit.json"), "{error}");
    }

    /// Asserts that `stats` aborts on `name` holding non-JSON text with
    /// the shared reader's own first part, then the remedy. The remedy comes
    /// from the one list of the files `stats` reads; the sentence as printed
    /// is pinned byte for byte in `tests/stats_models.rs`.
    fn assert_names_the_remedy_for_a_non_json(name: &str) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(name);
        fs::write(&path, "not json").unwrap();
        let (_guard, base) = empty_base();
        let error = stats(&[dir.path().to_path_buf()], &base, None).unwrap_err();
        assert!(
            error.starts_with(&format!("{}: invalid JSON (", path.display())),
            "{error}"
        );
        assert!(
            error.ends_with(&format!(". {}", unreadable_deliverable_remedy())),
            "{error}"
        );
    }

    /// A non-JSON `spec-review.json` aborts `stats` the same way.
    #[test]
    fn stats_names_the_remedy_for_a_non_json_spec_review() {
        assert_names_the_remedy_for_a_non_json("spec-review.json");
    }

    /// A non-JSON `task-*-audit*.json` aborts `stats` and names the remedy.
    #[test]
    fn stats_names_the_remedy_for_a_non_json_audit() {
        assert_names_the_remedy_for_a_non_json("task-1-audit.json");
    }

    /// A non-JSON `task-*-review*.json` aborts `stats` and names the remedy.
    #[test]
    fn stats_names_the_remedy_for_a_non_json_review() {
        assert_names_the_remedy_for_a_non_json("task-1-review.json");
    }

    /// A non-JSON `task-*-report.json` aborts `stats` and names the remedy.
    #[test]
    fn stats_names_the_remedy_for_a_non_json_report() {
        assert_names_the_remedy_for_a_non_json("task-1-report.json");
    }

    /// A non-JSON `branch-review.json` aborts `stats` the same way.
    #[test]
    fn stats_names_the_remedy_for_a_non_json_branch_review() {
        assert_names_the_remedy_for_a_non_json("branch-review.json");
    }

    /// A present-but-wrongly-typed `rules` field is a named finding, not
    /// silence: this binary names the file and the field
    /// (`houserules.crash-paths-are-named`).
    #[test]
    fn reports_a_wrongly_typed_rules_field_naming_the_file_instead_of_silently_skipping_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-audit.json"),
            serde_json::to_string(&json!({"ids": [], "rules": {"a": 1}})).unwrap(),
        )
        .unwrap();
        let (_guard, base) = empty_base();
        let error = stats(&[dir.path().to_path_buf()], &base, None).unwrap_err();
        assert!(error.contains("task-1-audit.json"), "{error}");
        assert!(error.contains("rules"), "{error}");
    }

    /// The same treatment for `ids`, the sibling field on the same
    /// `task-*-audit*.json` shape.
    #[test]
    fn reports_a_wrongly_typed_ids_field_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-audit.json"),
            serde_json::to_string(&json!({"ids": {"a": 1}, "rules": []})).unwrap(),
        )
        .unwrap();
        let (_guard, base) = empty_base();
        let error = stats(&[dir.path().to_path_buf()], &base, None).unwrap_err();
        assert!(error.contains("task-1-audit.json"), "{error}");
        assert!(error.contains("ids"), "{error}");
    }

    /// `rule_adherence`, the review-side sibling.
    #[test]
    fn reports_a_wrongly_typed_rule_adherence_field_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-review.json"),
            serde_json::to_string(&json!({"kind": "task-review", "rule_adherence": {"a": 1}}))
                .unwrap(),
        )
        .unwrap();
        let (_guard, base) = empty_base();
        let error = stats(&[dir.path().to_path_buf()], &base, None).unwrap_err();
        assert!(error.contains("task-1-review.json"), "{error}");
        assert!(error.contains("rule_adherence"), "{error}");
    }

    /// `knowledge_used`, the report-side sibling.
    #[test]
    fn reports_a_wrongly_typed_knowledge_used_field_naming_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(
            dir.path().join("task-1-report.json"),
            serde_json::to_string(&json!({"kind": "task-report", "knowledge_used": {"a": 1}}))
                .unwrap(),
        )
        .unwrap();
        let (_guard, base) = empty_base();
        let error = stats(&[dir.path().to_path_buf()], &base, None).unwrap_err();
        assert!(error.contains("task-1-report.json"), "{error}");
        assert!(error.contains("knowledge_used"), "{error}");
    }

    /// The finding lists and `fix_rounds` the cost profile reads, each
    /// wrongly typed, are named findings too.
    #[test]
    fn reports_a_wrongly_typed_findings_or_fix_rounds_field_naming_the_file() {
        for (name, body, field) in [
            (
                "task-1-review.json",
                json!({"kind": "task-review", "issues": {"a": 1}}),
                "issues",
            ),
            (
                "task-1-review-r1.json",
                json!({"kind": "re-review", "new_breakage": "none"}),
                "new_breakage",
            ),
            (
                "branch-review.json",
                json!({"kind": "branch-review", "issues": 3}),
                "issues",
            ),
            (
                "task-1-report.json",
                json!({"kind": "task-report", "fix_rounds": {"a": 1}}),
                "fix_rounds",
            ),
        ] {
            let dir = tempfile::tempdir().expect("tempdir");
            write(dir.path(), name, &body);
            let (_guard, base) = empty_base();
            let error = stats(&[dir.path().to_path_buf()], &base, None).unwrap_err();
            assert!(error.contains(name), "{error}");
            assert!(error.contains(field), "{error}");
        }
    }

    /// A deliverable with no `model`, or a `null` one, counts as `pre-2.0`;
    /// a non-empty string is the model.
    #[test]
    fn model_of_reads_the_field_and_names_an_absent_one_pre_2_0() {
        assert_eq!(model_of(&json!({}), "f.json").unwrap(), "pre-2.0");
        assert_eq!(
            model_of(&json!({"model": null}), "f.json").unwrap(),
            "pre-2.0"
        );
        assert_eq!(
            model_of(&json!({"model": "human"}), "f.json").unwrap(),
            "human"
        );
    }

    /// A `model` that names one family as a whole token counts in the row of
    /// the family, whether it is the alias or an id, with a version, a date,
    /// a provider prefix, or a context suffix; the case of the family does not
    /// matter.
    #[test]
    fn model_of_groups_an_alias_and_every_id_of_a_family_in_one_row() {
        for (given, row) in [
            ("opus", "opus"),
            ("claude-opus-5-5", "opus"),
            ("claude-opus-5-5[1m]", "opus"),
            ("Opus", "opus"),
            ("claude-sonnet-5-5", "sonnet"),
            ("us.anthropic.claude-sonnet-4-5-v1:0", "sonnet"),
            ("claude-3-5-haiku-20241022", "haiku"),
            ("haiku", "haiku"),
            ("claude-fable-1", "fable"),
            ("fable", "fable"),
        ] {
            assert_eq!(
                model_of(&json!({"model": given}), "f.json").unwrap(),
                row,
                "{given}"
            );
        }
    }

    /// A `model` that names no family, or two, or a family only as a part of
    /// another word, keeps its own row, as `human`, `unknown`, and `pre-2.0`
    /// do.
    #[test]
    fn model_of_keeps_the_row_of_a_value_that_names_no_single_family() {
        for given in [
            "human",
            "unknown",
            "pre-2.0",
            "gpt-5",
            "opus-or-sonnet",
            "opusculum",
            "sonnet4",
        ] {
            assert_eq!(
                model_of(&json!({"model": given}), "f.json").unwrap(),
                given,
                "{given}"
            );
        }
    }

    /// A `model` that is no non-empty string is a named error, never a silent
    /// `pre-2.0`.
    #[test]
    fn model_of_names_a_field_that_is_no_non_empty_string() {
        for bad in [json!(7), json!(""), json!(["a"]), json!({"id": "a"})] {
            assert_eq!(
                model_of(&json!({"model": bad}), "f.json").unwrap_err(),
                "f.json: model is not a non-empty string",
                "{bad}"
            );
        }
    }

    /// A missing workspace is a named error that starts with the path as
    /// given (`houserules.path-pins-mirror-the-code`).
    #[test]
    fn a_missing_workspace_is_a_named_error() {
        let parent = tempfile::tempdir().expect("tempdir");
        let missing = parent.path().join("missing");
        let (_guard, base) = empty_base();
        let error = stats(std::slice::from_ref(&missing), &base, None).unwrap_err();
        assert!(
            error.starts_with(&format!("{}: ", missing.display())),
            "{error}"
        );
    }

    /// A file given as a workspace is a named error too.
    #[test]
    fn a_file_given_as_a_workspace_is_a_named_error() {
        let parent = tempfile::tempdir().expect("tempdir");
        let file = parent.path().join("notes.txt");
        fs::write(&file, "text").unwrap();
        let (_guard, base) = empty_base();
        let error = stats(std::slice::from_ref(&file), &base, None).unwrap_err();
        assert!(
            error.starts_with(&format!("{}: ", file.display())),
            "{error}"
        );
    }

    /// A directory named `branch-review.json` is no deliverable: the read
    /// error names the path and carries the remedy.
    #[test]
    fn a_directory_named_branch_review_is_a_named_error() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        let branch_review = workspace_dir(&dir, BRANCH_REVIEW_FILE);
        let (_guard, base) = empty_base();
        let error = stats(std::slice::from_ref(&dir), &base, None).unwrap_err();
        assert!(
            error.starts_with(&format!("{}: ", branch_review.display())),
            "{error}"
        );
        assert!(
            error.ends_with(&format!(". {}", unreadable_deliverable_remedy())),
            "{error}"
        );
    }

    /// A `branch-review.json` that cannot be inspected is a named error,
    /// never an absent file. A symlink that points at itself fails the
    /// stat with the same error for every user, a privileged one included.
    #[cfg(unix)]
    #[test]
    fn a_branch_review_that_cannot_be_inspected_is_a_named_error() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "looped");
        let branch_review = dir.join(BRANCH_REVIEW_FILE);
        std::os::unix::fs::symlink(BRANCH_REVIEW_FILE, &branch_review).unwrap();
        let inspected = fs::metadata(&branch_review).unwrap_err();
        let (_guard, base) = empty_base();
        let error = stats(std::slice::from_ref(&dir), &base, None).unwrap_err();
        assert_eq!(error, format!("{}: {inspected}", branch_review.display()));
    }

    /// The label is the last component as given; `.` and `..` fall back
    /// to the canonical name, and a filesystem root to the path itself.
    #[test]
    fn the_label_falls_back_to_the_canonical_name_and_then_to_the_path() {
        assert_eq!(
            workspace_label(Path::new("sdd/batch-9/"), Path::new("/x/y")),
            "batch-9"
        );
        assert_eq!(
            workspace_label(Path::new("."), Path::new("/work/batch-9")),
            "batch-9"
        );
        assert_eq!(
            workspace_label(Path::new("../"), Path::new("/work")),
            "work"
        );
        assert_eq!(workspace_label(Path::new("/"), Path::new("/")), "/");
    }

    fn workspace_with_failing_audit(parent: &Path, name: &str) -> PathBuf {
        let dir = workspace_dir(parent, name);
        write(
            &dir,
            "task-1-audit.json",
            &json!({"ids": ["a.rule"], "rules": stats_rules("fail")}),
        );
        dir
    }

    /// With several workspaces a task label is `<directory name>/<task>`,
    /// sorted; `audits.tasks` counts the labels.
    #[test]
    fn several_workspaces_prefix_task_labels_with_the_directory_name() {
        let parent = tempfile::tempdir().expect("tempdir");
        let first = workspace_with_failing_audit(parent.path(), "first");
        let second = workspace_with_failing_audit(parent.path(), "second");
        let value = run(&[&first, &second]);
        assert_eq!(
            value["violations"],
            json!([{"id": "a.rule", "count": 2, "tasks": ["first/1", "second/1"]}])
        );
        assert_eq!(value["audits"], json!({"files": 2, "tasks": 2}));
    }

    /// A path with a trailing separator, as a shell glob hands it over,
    /// still labels its tasks with the directory name, never an empty
    /// prefix.
    #[test]
    fn a_trailing_separator_does_not_empty_the_label() {
        let parent = tempfile::tempdir().expect("tempdir");
        let first = workspace_with_failing_audit(parent.path(), "first");
        let second = workspace_with_failing_audit(parent.path(), "second");
        let value = run(&[&first.join(""), &second.join("")]);
        assert_eq!(
            value["violations"],
            json!([{"id": "a.rule", "count": 2, "tasks": ["first/1", "second/1"]}])
        );
        let rows = value["workspaces"].as_array().expect("rows");
        assert_eq!(rows[0]["path"], json!(first.join("").display().to_string()));
    }

    /// The same workspace given twice, by the same spelling or by a
    /// different one, is read once: the result equals the single run.
    #[test]
    fn the_same_workspace_given_twice_is_read_once() {
        let parent = tempfile::tempdir().expect("tempdir");
        let only = workspace_with_failing_audit(parent.path(), "only");
        let once = run(&[&only]);
        assert_eq!(run(&[&only, &only]), once);
        let dotted = parent.path().join(".").join("only");
        assert_eq!(run(&[&only, &dotted]), once);
        assert_eq!(once["workspaces"].as_array().map(Vec::len), Some(1));
    }

    /// A directory with no deliverables is a row of zeros, not an error.
    #[test]
    fn a_directory_with_no_deliverables_is_a_zero_row() {
        let parent = tempfile::tempdir().expect("tempdir");
        let empty = workspace_dir(parent.path(), "empty");
        let value = run(&[&empty]);
        assert_eq!(
            value["workspaces"],
            json!([{
                "path": empty.display().to_string(),
                "tasks": 0, "reviews": 0, "re_reviews": 0, "fix_rounds": 0, "report_bytes": 0,
                "findings": {"critical": 0, "important": 0, "minor": 0, "other": 0},
                "targets": {"code": 0, "deliverable": 0, "prose": 0, "other": 0},
                "reversals": 0,
            }])
        );
    }

    /// `tasks` counts distinct labels over reports only; `reviews` counts
    /// task review files and the branch review; `re_reviews` the review
    /// files of kind `re-review`; `fix_rounds` sums the reports'
    /// `fix_rounds` lengths; `report_bytes` sums the report file sizes.
    #[test]
    fn workspace_rows_count_tasks_reviews_re_reviews_fix_rounds_and_report_bytes() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-report.json",
            &json!({"kind": "task-report", "fix_rounds": [{"round": 1}, {"round": 2}]}),
        );
        write(
            &dir,
            "task-2-report.json",
            &json!({"kind": "task-report", "fix_rounds": [{"round": 1}]}),
        );
        write(&dir, "task-3-report.json", &json!({"kind": "task-report"}));
        write(&dir, "task-4-audit.json", &json!({}));
        write(&dir, "task-1-review.json", &json!({"kind": "task-review"}));
        write(&dir, "task-1-review-r1.json", &json!({"kind": "re-review"}));
        write(&dir, "task-2-review.json", &json!({"kind": "task-review"}));
        write(
            &dir,
            "branch-review.json",
            &json!({"kind": "branch-review"}),
        );
        let bytes: u64 = [
            "task-1-report.json",
            "task-2-report.json",
            "task-3-report.json",
        ]
        .iter()
        .map(|name| fs::metadata(dir.join(name)).unwrap().len())
        .sum();

        let value = run(&[&dir]);
        let row = &value["workspaces"][0];
        assert_eq!(row["path"], json!(dir.display().to_string()));
        assert_eq!(row["tasks"], json!(3));
        assert_eq!(row["reviews"], json!(4));
        assert_eq!(row["re_reviews"], json!(1));
        assert_eq!(row["fix_rounds"], json!(3));
        assert_eq!(row["report_bytes"], json!(bytes));
        assert_eq!(value["reviews"], json!({"files": 3}));
        assert_eq!(value["audits"], json!({"files": 1, "tasks": 1}));
    }

    /// Findings come from a task review's `issues`, a re-review's
    /// `new_breakage` (never its `issues`), and the branch review's
    /// `issues`; they split by severity and by target.
    #[test]
    fn findings_come_from_issues_new_breakage_and_the_branch_review() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-review.json",
            &json!({
                "kind": "task-review",
                "issues": [
                    {"severity": "critical", "file": "src/lib.rs:3", "rule": "a.rule"},
                    {"severity": "minor", "file": "README.md"},
                ],
                "new_breakage": [{"severity": "critical", "file": "src/ignored.rs"}],
            }),
        );
        write(
            &dir,
            "task-1-review-r1.json",
            &json!({
                "kind": "re-review",
                "new_breakage": [
                    {"severity": "important", "file": ".superpowers/sdd/batch-9/task-1-report.json"},
                ],
                "issues": [{"severity": "critical", "file": "src/ignored.rs"}],
            }),
        );
        write(
            &dir,
            "branch-review.json",
            &json!({"kind": "branch-review", "issues": [{"severity": "minor"}]}),
        );
        let value = run(&[&dir]);
        let row = &value["workspaces"][0];
        assert_eq!(
            row["findings"],
            json!({"critical": 1, "important": 1, "minor": 2, "other": 0})
        );
        assert_eq!(
            row["targets"],
            json!({"code": 1, "deliverable": 1, "prose": 1, "other": 1})
        );
    }

    /// A finding on a code file whose name holds the word `commit` is a
    /// code finding, not a prose one; a finding that cites a commit message
    /// is prose. The `narrow` proposal reads this split.
    #[test]
    fn a_code_file_named_after_commits_is_code_and_a_commit_citation_is_prose() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-review.json",
            &json!({
                "kind": "task-review",
                "issues": [
                    {"severity": "important", "file": "crates/houserules/src/rules/check_commit.rs:34"},
                    {"severity": "important", "file": "template/.githooks/commit-msg"},
                    {"severity": "important", "file": "src/commit_service.ts"},
                    {"severity": "minor", "file": "13ba820 (commit message body)"},
                    {"severity": "minor", "file": "git log 3cc86e6..96b1201"},
                ],
            }),
        );
        let value = run(&[&dir]);
        assert_eq!(
            value["workspaces"][0]["targets"],
            json!({"code": 3, "deliverable": 0, "prose": 2, "other": 0})
        );
    }

    /// An old review shape -- an issue with no `rule`, no `severity`, or
    /// no `file`, or one that is no object -- counts under `other` and
    /// never crashes.
    #[test]
    fn old_review_shapes_count_under_other_and_never_crash() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-review.json",
            &json!({
                "kind": "task-review",
                "issues": [
                    {"what": "nothing else"},
                    {"severity": "minor", "what": "no file, no rule"},
                    {"severity": "major", "file": "a.rs"},
                    "a string",
                    5,
                ],
            }),
        );
        let value = run_with(&[entry("a.rule", json!({}))], &[&dir]);
        let row = &value["workspaces"][0];
        assert_eq!(
            row["findings"],
            json!({"critical": 0, "important": 0, "minor": 1, "other": 4})
        );
        assert_eq!(
            row["targets"],
            json!({"code": 1, "deliverable": 0, "prose": 0, "other": 4})
        );
        assert_eq!(value["rules"][0]["findings"]["other"], json!(0));
    }

    /// The branch review's `rule_adherence` feeds the `rules` rows but
    /// not `violations`, which keeps its old inputs.
    #[test]
    fn the_branch_review_rule_adherence_feeds_rules_but_not_violations() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "branch-review.json",
            &json!({
                "kind": "branch-review",
                "rule_adherence": [
                    {"id": "a.rule", "mode": "judged", "result": "fail", "evidence": "e"},
                ],
            }),
        );
        let value = run_with(&[entry("a.rule", json!({}))], &[&dir]);
        assert_eq!(value["violations"], json!([]));
        assert_eq!(value["rules"][0]["rows"], json!(1));
        assert_eq!(value["rules"][0]["fails"], json!(1));
    }

    /// `cost` sums the workspace rows and divides by the total tasks.
    #[test]
    fn cost_sums_the_workspace_rows_and_means_them_per_task() {
        let parent = tempfile::tempdir().expect("tempdir");
        let first = workspace_dir(parent.path(), "first");
        let second = workspace_dir(parent.path(), "second");
        write(&first, "task-1-report.json", &json!({"fix_rounds": [{}]}));
        write(
            &second,
            "task-1-report.json",
            &json!({"fix_rounds": [{}, {}, {}]}),
        );
        write(&second, "task-2-report.json", &json!({}));
        write(
            &second,
            "task-2-review.json",
            &json!({"kind": "task-review"}),
        );
        let value = run(&[&first, &second]);
        assert_eq!(value["cost"]["totals"]["tasks"], json!(3));
        assert_eq!(value["cost"]["totals"]["fix_rounds"], json!(4));
        assert_eq!(value["cost"]["totals"]["reviews"], json!(1));
        assert_eq!(value["cost"]["per_task"]["fix_rounds"], json!(1.33));
        assert_eq!(value["cost"]["per_task"]["reviews"], json!(0.33));
        assert!(value["cost"]["per_task"].get("tasks").is_none());
    }

    /// With no task at all, `per_task` is the token `none`.
    #[test]
    fn cost_per_task_is_none_when_no_workspace_holds_a_report() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "audits-only");
        write(&dir, "task-1-audit.json", &json!({}));
        let value = run(&[&dir]);
        assert_eq!(value["cost"]["totals"]["tasks"], json!(0));
        assert_eq!(value["cost"]["per_task"]["reviews"], json!("none"));
    }

    /// An id in a deliverable that the base does not hold gets no `rules`
    /// row and no proposal, and still counts in `violations` and
    /// `unused_ids`. A retired entry gets no row either.
    #[test]
    fn an_unknown_id_and_a_retired_entry_get_no_rules_row() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-audit.json",
            &json!({
                "ids": ["ghost.id", "gone.id", "a.rule"],
                "rules": [
                    {"id": "ghost.id", "result": "fail"},
                    {"id": "gone.id", "result": "fail"},
                ],
            }),
        );
        let value = run_with(
            &[
                entry("a.rule", json!({})),
                entry("gone.id", json!({"status": "retired"})),
            ],
            &[&dir],
        );
        let ids = |key: &str| -> Vec<String> {
            value[key]
                .as_array()
                .expect("array")
                .iter()
                .map(|row| row["id"].as_str().expect("id").to_string())
                .collect()
        };
        assert_eq!(ids("rules"), ["a.rule"]);
        assert_eq!(ids("violations"), ["ghost.id", "gone.id"]);
        assert_eq!(ids("unused_ids"), ["a.rule", "ghost.id", "gone.id"]);
        assert_eq!(value["proposals"], json!([]));
    }

    /// Each `rules` row carries the entry's flags and its counters.
    #[test]
    fn rules_rows_carry_the_entry_flags_and_counters() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-audit.json",
            &json!({
                "ids": ["b.two"],
                "rules": [{"id": "b.two", "mode": "deterministic", "result": "pass"}],
            }),
        );
        write(
            &dir,
            "task-1-report.json",
            &json!({"knowledge_used": ["b.two", "b.two"]}),
        );
        write(
            &dir,
            "task-1-review.json",
            &json!({
                "kind": "task-review",
                "issues": [{"severity": "important", "file": "src/a.rs", "rule": "b.two"}],
            }),
        );
        let value = run_with(
            &[
                entry("a.one", json!({})),
                entry("b.two", json!({"standing": true, "check": valid_check()})),
            ],
            &[&dir],
        );
        assert_eq!(
            value["rules"],
            json!([
                {
                    "id": "a.one", "source": "local", "standing": false, "mode": "judged",
                    "workspaces": 0, "injected": 0, "rows": 0, "fails": 0,
                    "findings": {"critical": 0, "important": 0, "minor": 0, "other": 0},
                    "targets": {"code": 0, "deliverable": 0, "prose": 0, "other": 0},
                    "cited": 0,
                    "verdicts": 0, "overrides": 0, "override_rate": null, "cost": null,
                    "sanctioned": 0,
                },
                {
                    "id": "b.two", "source": "local", "standing": true, "mode": "deterministic",
                    "workspaces": 1, "injected": 1, "rows": 1, "fails": 0,
                    "findings": {"critical": 0, "important": 1, "minor": 0, "other": 0},
                    "targets": {"code": 1, "deliverable": 0, "prose": 0, "other": 0},
                    "cited": 1,
                    "verdicts": 0, "overrides": 0, "override_rate": null, "cost": null,
                    "sanctioned": 0,
                },
            ])
        );
    }

    /// Each row counts once. `rows` and `fails` take the deterministic
    /// rows of the audit files and the judged rows of the review files
    /// (task reviews, re-reviews, `branch-review.json`). A reviewer's copy
    /// of a deterministic audit row, and an audit's `open` judged row, are
    /// not counted -- yet the workspace still holds the id, so `workspaces`
    /// counts it. `violations` keeps its own inputs.
    #[test]
    fn each_row_counts_once_deterministic_from_audits_and_judged_from_reviews() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-audit.json",
            &json!({
                "ids": [],
                "rules": [
                    {"id": "a.rule", "mode": "deterministic", "result": "fail"},
                    {"id": "b.rule", "mode": "judged", "result": "open"},
                ],
            }),
        );
        write(
            &dir,
            "task-1-review.json",
            &json!({
                "kind": "task-review",
                "rule_adherence": [
                    {"id": "a.rule", "mode": "deterministic", "result": "fail"},
                    {"id": "b.rule", "mode": "judged", "result": "fail"},
                    {"id": "c.rule", "mode": "deterministic", "result": "pass"},
                ],
            }),
        );
        write(
            &dir,
            "task-1-review-r1.json",
            &json!({
                "kind": "re-review",
                "rule_adherence": [{"id": "b.rule", "mode": "judged", "result": "pass"}],
            }),
        );
        write(
            &dir,
            "branch-review.json",
            &json!({
                "kind": "branch-review",
                "rule_adherence": [
                    {"id": "a.rule", "mode": "deterministic", "result": "fail"},
                    {"id": "b.rule", "mode": "judged", "result": "pass"},
                ],
            }),
        );
        let value = run_with(
            &[
                entry("a.rule", json!({})),
                entry("b.rule", json!({})),
                entry("c.rule", json!({})),
            ],
            &[&dir],
        );
        let counters = |index: usize| {
            let row = &value["rules"][index];
            (
                row["workspaces"].clone(),
                row["rows"].clone(),
                row["fails"].clone(),
            )
        };
        assert_eq!(counters(0), (json!(1), json!(1), json!(1)), "a.rule");
        assert_eq!(counters(1), (json!(1), json!(3), json!(1)), "b.rule");
        assert_eq!(counters(2), (json!(1), json!(0), json!(0)), "c.rule");
        assert_eq!(
            value["violations"],
            json!([
                {"id": "a.rule", "count": 1, "tasks": ["1"]},
                {"id": "b.rule", "count": 1, "tasks": ["1"]},
            ])
        );
    }

    /// `stats` over one local rule and one pack rule: each `rules` row
    /// names its source, and the `packs` key sums the cost of each source,
    /// `local` first. The violation counts come from the audit and review
    /// files the workspace holds, the way the top-level `violations` key
    /// counts them.
    #[test]
    fn stats_names_the_source_of_each_rule_and_sums_the_cost_per_source() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dir = workspace_dir(parent.path(), "batch-9");
        write(
            &dir,
            "task-1-audit.json",
            &json!({
                "ids": [],
                "rules": [
                    {"id": "test.local", "mode": "deterministic", "result": "fail"},
                    {"id": "alpha.pack", "mode": "judged", "result": "open"},
                ],
            }),
        );
        write(
            &dir,
            "task-1-review.json",
            &json!({
                "kind": "task-review",
                "rule_adherence": [
                    {"id": "alpha.pack", "mode": "judged", "result": "fail"},
                    {"id": "test.local", "mode": "deterministic", "result": "pass"},
                ],
            }),
        );
        let (_guard, base) = base_with_packs(
            &[entry(
                "test.local",
                json!({"standing": true, "check": valid_check()}),
            )],
            &[("alpha", &[entry("alpha.pack", json!({}))])],
            &[],
        );

        let value = stats(std::slice::from_ref(&dir), &base, None)
            .unwrap()
            .value;

        let sources: Vec<(&str, &str)> = value["rules"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| {
                (
                    row["id"].as_str().expect("id"),
                    row["source"].as_str().expect("source"),
                )
            })
            .collect();
        assert_eq!(sources, [("alpha.pack", "alpha"), ("test.local", "local")]);
        assert_eq!(
            value["packs"],
            json!([
                {"source": "local", "entries": 1, "standing": 1, "judged_rows": 0, "violations": 1},
                {"source": "alpha", "entries": 1, "standing": 0, "judged_rows": 1, "violations": 1},
            ])
        );
    }

    /// An entry tagged `ruled-keep` gets no `demote`, `retire`, `mechanize`,
    /// or `narrow` proposal, while the same entry without the tag does
    /// (design.md 5.93). Each pair below meets its threshold exactly.
    #[test]
    fn a_ruled_keep_entry_gets_no_demote_retire_mechanize_or_narrow() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dirs: Vec<PathBuf> = ["one", "two", "three"]
            .into_iter()
            .map(|name| {
                let dir = workspace_dir(parent.path(), name);
                write(
                    &dir,
                    "task-1-audit.json",
                    &json!({
                        "ids": ["r.keep", "r.plain"],
                        "rules": [
                            {"id": "s.keep", "mode": "deterministic", "result": "pass"},
                            {"id": "s.plain", "mode": "deterministic", "result": "pass"},
                        ],
                    }),
                );
                dir
            })
            .collect();
        for dir in &dirs[..2] {
            write(
                dir,
                "task-1-review.json",
                &json!({
                    "kind": "task-review",
                    "rule_adherence": [
                        {"id": "m.keep", "mode": "judged", "result": "fail"},
                        {"id": "m.plain", "mode": "judged", "result": "fail"},
                    ],
                }),
            );
        }
        write(
            &dirs[2],
            "task-1-review.json",
            &json!({
                "kind": "task-review",
                "issues": [
                    {"severity": "minor", "file": "a.rs", "rule": "n.keep"},
                    {"severity": "minor", "file": "a.rs", "rule": "n.keep"},
                    {"severity": "minor", "file": "a.rs", "rule": "n.keep"},
                    {"severity": "minor", "file": "a.rs", "rule": "n.plain"},
                    {"severity": "minor", "file": "a.rs", "rule": "n.plain"},
                    {"severity": "minor", "file": "a.rs", "rule": "n.plain"},
                ],
            }),
        );
        let refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
        let keep = json!({"tags": ["knowledge", "ruled-keep"]});
        let value = run_with(
            &[
                entry("s.keep", json!({"standing": true, "tags": ["ruled-keep"]})),
                entry("s.plain", json!({"standing": true})),
                entry("r.keep", keep.clone()),
                entry("r.plain", json!({})),
                entry("m.keep", keep.clone()),
                entry("m.plain", json!({})),
                entry("n.keep", keep),
                entry("n.plain", json!({})),
            ],
            &refs,
        );
        let pairs: Vec<(String, String)> = value["proposals"]
            .as_array()
            .expect("array")
            .iter()
            .map(|row| {
                (
                    row["action"].as_str().expect("action").to_string(),
                    row["id"].as_str().expect("id").to_string(),
                )
            })
            .collect();
        let expected: Vec<(String, String)> = [
            ("demote", "s.plain"),
            ("retire", "r.plain"),
            ("mechanize", "m.plain"),
            ("narrow", "n.plain"),
        ]
        .iter()
        .map(|(action, id)| (action.to_string(), id.to_string()))
        .collect();
        assert_eq!(pairs, expected);
    }

    /// A proposal reaches the output: a standing entry audited clean in
    /// three workspaces is a gated `demote`.
    #[test]
    fn a_clean_standing_entry_in_three_workspaces_proposes_a_gated_demote() {
        let parent = tempfile::tempdir().expect("tempdir");
        let dirs: Vec<PathBuf> = ["one", "two", "three"]
            .into_iter()
            .map(|name| {
                let dir = workspace_dir(parent.path(), name);
                write(
                    &dir,
                    "task-1-audit.json",
                    &json!({
                        "ids": ["s.one"],
                        "rules": [{"id": "s.one", "mode": "deterministic", "result": "pass"}],
                    }),
                );
                dir
            })
            .collect();
        let refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
        let value = run_with(&[entry("s.one", json!({"standing": true}))], &refs);
        let proposals = value["proposals"].as_array().expect("array");
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["action"], json!("demote"));
        assert_eq!(proposals[0]["id"], json!("s.one"));
        assert_eq!(proposals[0]["owner_gate"], json!(true));
        assert_eq!(
            proposals[0]["evidence"],
            json!({"workspaces": 3, "fails": 0, "findings": 0})
        );
    }

    /// A line records a reversal when it starts with `Reversal:` after
    /// optional spaces and an optional list marker.
    #[test]
    fn is_reversal_reads_the_word_after_spaces_and_a_list_marker() {
        for line in [
            "Reversal: a",
            "- Reversal: a",
            "* Reversal: a",
            "+ Reversal: a",
            "  * Reversal: a",
            "    Reversal: a",
            "Reversal:",
        ] {
            assert!(is_reversal(line), "{line}");
        }
        for line in [
            "",
            "reversal: a",
            "Reversal a",
            "the ledger has no Reversal: line",
            "- a Reversal: in the middle",
            "-Reversal: a",
            "-- Reversal: a",
        ] {
            assert!(!is_reversal(line), "{line:?}");
        }
    }

    /// A line embeds a reversal when it holds `Reversal:` and is no reversal:
    /// the word inside a longer entry, in any position but the counted start.
    #[test]
    fn embeds_a_reversal_reads_a_line_that_holds_the_word_and_does_not_count() {
        for line in [
            "the ledger has no Reversal: line",
            "Task 4 had a Reversal: after other text",
            "- a list item that mentions a Reversal: in its middle",
            "-Reversal: a",
            "-- Reversal: a",
            "x Reversal:",
        ] {
            assert!(embeds_a_reversal(line), "{line}");
        }
        for line in [
            "",
            "Reversal: a",
            "- Reversal: a",
            "  * Reversal: a, and Reversal: b",
            "reversal: a",
            "Reversal a",
            "the word reversal alone",
        ] {
            assert!(!embeds_a_reversal(line), "{line:?}");
        }
    }

    /// `stats` warns once for each embedded line of a ledger, with the line
    /// number counted from 1, and the count of the workspace stays.
    #[test]
    fn stats_warns_for_each_embedded_reversal_and_keeps_the_count() {
        let scratch = tempfile::tempdir().unwrap();
        let dir = workspace_dir(scratch.path(), "2026-10-01-batch-1");
        fs::write(
            dir.join(PROGRESS_FILE),
            "Reversal: a\nsee Reversal: b\n\n- Reversal: c\nand Reversal: d\n",
        )
        .unwrap();
        let (_guard, base) = empty_base();

        let outcome = stats(std::slice::from_ref(&dir), &base, None).unwrap();

        assert_eq!(outcome.value["workspaces"][0]["reversals"], json!(2));
        let ledger = dir.join(PROGRESS_FILE);
        assert_eq!(
            outcome.warnings,
            [
                format!("{}:2: {EMBEDDED_REVERSAL}", ledger.display()),
                format!("{}:5: {EMBEDDED_REVERSAL}", ledger.display()),
            ]
        );
    }

    /// The reference of a sanction is the text between the last
    /// `(sanctioned: ` and the closing parenthesis that ends the evidence.
    #[test]
    fn sanction_in_evidence_reads_the_suffix_that_audit_appends() {
        assert_eq!(
            sanction_in_evidence("e (sanctioned: plan T8)"),
            Some("plan T8")
        );
        assert_eq!(
            sanction_in_evidence("e (sanctioned: step 4 (T8))"),
            Some("step 4 (T8)")
        );
        for evidence in [
            "",
            "e",
            "e (sanctioned: )",
            "e (sanctioned: plan) and more",
            "e sanctioned: plan)",
        ] {
            assert_eq!(sanction_in_evidence(evidence), None, "{evidence}");
        }
    }

    /// A suffix on a judged row is no booking: only a deterministic row gets
    /// the suffix from `audit`.
    #[test]
    fn a_suffix_on_a_judged_row_does_not_sanction_it() {
        let data = json!({"rule_adherence": [
            {"id": "j.one", "mode": "judged", "result": "fail",
             "evidence": "e (sanctioned: plan)"},
            {"id": "d.one", "mode": "deterministic", "result": "fail",
             "evidence": "e (sanctioned: plan)"},
            {"id": "d.two", "mode": "deterministic", "result": "pass",
             "evidence": "e (sanctioned: plan)"},
        ]});
        let rows = rows_of(&data, "rule_adherence", "f.json").unwrap();
        let outcome: Vec<(&str, bool, Option<&str>)> = rows
            .iter()
            .map(|row| (row.id, row.failed, row.sanction.as_deref()))
            .collect();
        assert_eq!(
            outcome,
            [
                ("j.one", true, None),
                ("d.one", false, Some("plan")),
                ("d.two", false, None),
            ]
        );
    }

    /// The window keeps the most recent sources in the order given, and keeps
    /// all of them when it is above the count.
    #[test]
    fn newest_keeps_the_most_recent_sources_in_the_order_given() {
        let given: Vec<PathBuf> = ["x/2026-10-03-c", "x/2026-10-01-a", "x/2026-10-02-b"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let sources = |window: usize| -> Vec<String> {
            let all = given
                .iter()
                .map(|given| Source {
                    given,
                    label: workspace_label(given, given),
                })
                .collect();
            newest(all, window)
                .into_iter()
                .map(|source| source.label)
                .collect()
        };
        assert_eq!(sources(2), ["2026-10-03-c", "2026-10-02-b"]);
        assert_eq!(sources(1), ["2026-10-03-c"]);
        assert_eq!(sources(3).len(), 3);
        assert_eq!(sources(9).len(), 3);
    }
}
