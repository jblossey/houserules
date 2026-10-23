//! `houserules report-facts`: fills the fact fields of a task report from git
//! and from captures, so that the author writes the prose and the tool writes
//! the facts.
//!
//! The command reads the report, computes every fact, and writes the report
//! back only when it found nothing to report. The fill order is: `commits`,
//! `files_changed`, the `commits` of the last fix round, `self_audit`, the runs
//! (`tests`, `live_run`, `tdd[].red`, `tdd[].green`, `fix_rounds[].tests[]`),
//! and `citations`. A key that the report lacks is added at the end of the
//! report object; every other key keeps its place. The write goes through a
//! temporary file in the folder of the report and a rename, so a reader never
//! sees a partial report.
//!
//! A recorded hash never changes. A run whose `sha256` differs from its capture,
//! and a citation whose anchor is not on the cited lines at HEAD, are findings:
//! the command prints one line for each and writes nothing.
//! [`judge_citation`] is the one judgement of a citation.
//!
//! Limits:
//! - A run, citation, or fix round whose fields have the wrong type is left as
//!   it is. `validate` names it, and `tools/houserules-step.sh` runs `validate`
//!   after this command.
//! - A path that git prints and that is not UTF-8 reaches `files_changed` with
//!   U+FFFD in place of each invalid byte.
//! - `files_changed` lists a renamed file under both paths: the diff runs with
//!   `--no-renames`, so the list does not depend on the `diff.renames` setting.
//! - An anchor counts at every position where it starts, so `aa` occurs twice in
//!   `aaa`.
//! - A cited path is "not tracked" also when git cannot read its object; the
//!   judgement does not tell the two apart.

use std::fmt::Display;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use serde_json::{Map, Value, json};
use tempfile::NamedTempFile;

use crate::capture::{self, ReadError};
use crate::rules::InstallPrefix;
use crate::{baseline, emit, shell};

/// The cited lines of a file at HEAD, as a hash.
#[derive(Debug)]
pub(crate) struct CitedLines {
    /// The lowercase hex SHA-256 of the cited lines: from the first byte of the
    /// first cited line to the last byte of the last cited line, without the LF
    /// that ends the last line.
    pub(crate) sha256: String,
}

/// Why a citation fails. Each variant is one finding line of spec 5 and spec 6
/// (check 4); [`CitationFault::finding`] prints it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CitationFault {
    /// The path holds a `..` segment, or the object `HEAD:./<path>` at the
    /// install root is no blob (a directory, a submodule, or nothing).
    NotTracked,
    /// `end` is smaller than `line`.
    EndBeforeLine,
    /// The file has fewer lines than the citation needs.
    TooFewLines {
        /// The number of lines of the file.
        lines: u64,
        /// The larger of `line` and `end`.
        needed: u64,
    },
    /// The anchor is not in the cited lines.
    AnchorNotInRange,
    /// The anchor occurs more than once in the file.
    AnchorRepeated {
        /// The number of positions where the anchor starts.
        count: u64,
    },
}

impl CitationFault {
    /// The finding line for entry `index` of `citations`, which cites `path` at
    /// `line`, up to `end`.
    pub(crate) fn finding(&self, index: usize, path: &str, line: u64, end: Option<u64>) -> String {
        let leading = format!("citations[{index}]: ");
        match self {
            Self::NotTracked => format!("{leading}{path} is not tracked at HEAD"),
            Self::EndBeforeLine => {
                format!("{leading}end {} is before line {line}", end.unwrap_or(line))
            }
            Self::TooFewLines { lines, needed } => {
                format!("{leading}{path} has {lines} lines at HEAD, fewer than {needed}")
            }
            Self::AnchorNotInRange => format!(
                "{leading}the anchor is not in {path}:{} at HEAD; re-open the file at HEAD and \
                 correct line and end",
                range_label(line, end)
            ),
            Self::AnchorRepeated { count } => format!(
                "{leading}the anchor occurs {count} times in {path} at HEAD; choose text that \
                 occurs once"
            ),
        }
    }
}

/// The range of a citation as a finding names it: `<line>`, or `<line>-<end>`
/// when `end` is greater than `line`.
pub(crate) fn range_label(line: u64, end: Option<u64>) -> String {
    match end {
        Some(end) if end > line => format!("{line}-{end}"),
        _ => line.to_string(),
    }
}

/// Judges the citation of `line` (up to `end`) of `path` with `anchor`, against
/// the file as `git show HEAD:./<path>` prints it at the install root `root`:
/// the blob, so the working tree and its line endings do not matter.
///
/// `path` is tracked when it holds no `..` segment and `git cat-file -t` prints
/// `blob` for the object, so a directory, a submodule, and a file outside the
/// install root are not tracked. The cited lines run from the first byte of line
/// `line` to the last byte of line `end` (or of `line`), without the LF that ends
/// the last line. An LF ends a line, a CR before it stays in the line, and a last
/// line with no LF is a line. The anchor must occur in the cited lines, and
/// nowhere else in the file.
///
/// `line` and `end` are at least 1, as the deliverables schema requires; a
/// caller skips an entry whose numbers are lower and leaves it to `validate`.
pub(crate) fn judge_citation(
    root: &Path,
    path: &str,
    line: u64,
    end: Option<u64>,
    anchor: &str,
) -> Result<CitedLines, CitationFault> {
    let file = blob_at_head(root, path).ok_or(CitationFault::NotTracked)?;
    judge_file(&file, line, end, anchor)
}

/// The judgement of a citation against the bytes of the cited file; see
/// [`judge_citation`].
fn judge_file(
    file: &[u8],
    line: u64,
    end: Option<u64>,
    anchor: &str,
) -> Result<CitedLines, CitationFault> {
    let last = end.unwrap_or(line);
    if last < line {
        return Err(CitationFault::EndBeforeLine);
    }
    let lines = lines_of(file);
    let Some(stop) = usize::try_from(last)
        .ok()
        .filter(|stop| *stop <= lines.len())
    else {
        return Err(CitationFault::TooFewLines {
            lines: u64::try_from(lines.len()).unwrap_or(u64::MAX),
            needed: last,
        });
    };
    // `line` is at most `last`, so the conversion fits; `stop` makes the slice empty if not.
    let first = usize::try_from(line.saturating_sub(1)).map_or(stop, |first| first.min(stop));
    let bytes = lines[first..stop].join(&b'\n');
    if !occurs_in(&bytes, anchor.as_bytes()) {
        return Err(CitationFault::AnchorNotInRange);
    }
    let anchors = occurrences(file, anchor.as_bytes());
    if anchors != 1 {
        return Err(CitationFault::AnchorRepeated { count: anchors });
    }
    Ok(CitedLines {
        sha256: baseline::hash(&bytes),
    })
}

/// The lines of `file`: an LF ends a line, a CR stays in its line, and a last
/// line with no LF is a line. The empty file has no line.
fn lines_of(file: &[u8]) -> Vec<&[u8]> {
    let mut lines: Vec<&[u8]> = file.split(|byte| *byte == b'\n').collect();
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines
}

/// Whether `needle` occurs in `haystack`; the empty needle occurs nowhere.
fn occurs_in(haystack: &[u8], needle: &[u8]) -> bool {
    occurrences(haystack, needle) > 0
}

/// The number of positions in `haystack` where `needle` starts; the empty needle
/// starts nowhere.
fn occurrences(haystack: &[u8], needle: &[u8]) -> u64 {
    if needle.is_empty() {
        return 0;
    }
    let count = haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count();
    u64::try_from(count).unwrap_or(u64::MAX)
}

/// The bytes of the file `path` as the blob `HEAD:./<path>` at `root` holds
/// them, or `None` when `path` is not tracked (see [`judge_citation`]).
fn blob_at_head(root: &Path, path: &str) -> Option<Vec<u8>> {
    if path.is_empty() || path.starts_with('/') || path.split('/').any(|part| part == "..") {
        return None;
    }
    let object = format!("HEAD:./{path}");
    let kind = git(root, &["cat-file", "-t", &object]).ok()?;
    if kind.trim_ascii() != b"blob" {
        return None;
    }
    git(root, &["show", &object]).ok()
}

/// What stops the command before it writes.
enum Stop {
    /// A setup error: one line, exit status 2.
    Setup(String),
    /// Findings: one line each, exit status 1.
    Findings(Vec<String>),
}

/// The arguments of the command.
struct Request {
    report: PathBuf,
    base: String,
    fix_base: Option<String>,
    audit: Option<PathBuf>,
}

/// Runs `git` with `args` at `root`. `Err` is the first line of the error output
/// of git, or the operating system's error when git did not run.
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("git failed")
        .to_string())
}

/// The full sha of the commit that `reference` names at `root`, or `None` when
/// it names no commit.
fn resolve_commit(root: &Path, reference: &str) -> Option<String> {
    let peeled = format!("{reference}^{{commit}}");
    let output = git(root, &["rev-parse", "--verify", "--quiet", &peeled]).ok()?;
    let sha = String::from_utf8(output).ok()?.trim().to_string();
    (!sha.is_empty()).then_some(sha)
}

/// The commits of `since..HEAD`, oldest first, as `{ sha, subject }` objects.
fn commits_since(root: &Path, since: &str) -> Result<Value, Stop> {
    let range = format!("{since}..HEAD");
    let output = git(
        root,
        &[
            "log",
            "--reverse",
            "-z",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--format=%H%n%s",
            &range,
        ],
    )
    .map_err(|error| Stop::Setup(format!("report-facts: git log failed: {error}")))?;
    let text = String::from_utf8_lossy(&output);
    let commits: Vec<Value> = text
        .split('\0')
        .filter(|record| !record.is_empty())
        .map(|record| {
            let (sha, subject) = record.split_once('\n').unwrap_or((record, ""));
            json!({"sha": sha, "subject": subject})
        })
        .collect();
    Ok(Value::Array(commits))
}

/// The paths that `git diff --name-only --no-renames since..HEAD` prints inside
/// the install root, as install-root paths, sorted. A renamed file lists both
/// of its paths.
fn files_changed(root: &Path, prefix: &InstallPrefix, since: &str) -> Result<Value, Stop> {
    let range = format!("{since}..HEAD");
    let output = git(root, &["diff", "--name-only", "--no-renames", "-z", &range])
        .map_err(|error| Stop::Setup(format!("report-facts: git diff failed: {error}")))?;
    let mut paths = prefix.map(&String::from_utf8_lossy(&output));
    paths.sort();
    paths.dedup();
    Ok(json!(paths))
}

/// Reads the report at `request.report` and checks its kind. `Ok` holds the
/// resolved path of the file and the report object.
fn read_report(request: &Request) -> Result<(PathBuf, Map<String, Value>), Stop> {
    let given = &request.report;
    let named = |error: &dyn Display| Stop::Setup(format!("{}: {error}", given.display()));
    let resolved = crate::node_path::resolve_like_node(given).map_err(|e| named(&e))?;
    let text = fs::read_to_string(&resolved).map_err(|e| named(&e))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| named(&e))?;
    match value {
        Value::Object(report)
            if report.get("kind").and_then(Value::as_str) == Some("task-report") =>
        {
            Ok((resolved, report))
        }
        _ => Err(named(&"kind is not task-report")),
    }
}

/// Reads the file that `audit --json` wrote and returns the `self_audit` object
/// it gives: the summary as written, and the deterministic rows in their order.
fn read_audit(file: &Path) -> Result<Value, Stop> {
    let named = |error: &dyn Display| Stop::Setup(format!("--audit {}: {error}", file.display()));
    let text = fs::read_to_string(file).map_err(|e| named(&e))?;
    let audit: Value = serde_json::from_str(&text).map_err(|e| named(&e))?;
    let summary = audit.get("summary").filter(|summary| summary.is_object());
    let rules = audit.get("rules").and_then(Value::as_array);
    let (Some(summary), Some(rules)) = (summary, rules) else {
        return Err(named(
            &"it is no audit result (it needs a summary object and a rules array)",
        ));
    };
    let rows: Vec<&Value> = rules
        .iter()
        .filter(|rule| rule.get("mode").and_then(Value::as_str) == Some("deterministic"))
        .collect();
    Ok(json!({"summary": summary, "rows": rows}))
}

/// The last entry of `fix_rounds`, when the report has one.
fn last_fix_round(report: &mut Map<String, Value>) -> Option<&mut Map<String, Value>> {
    report
        .get_mut("fix_rounds")?
        .as_array_mut()?
        .last_mut()?
        .as_object_mut()
}

/// Calls `visit` with the label and the object of each run of `list`.
fn visit_list(list: Option<&mut Value>, name: &str, visit: &mut impl FnMut(&str, &mut Run)) {
    if let Some(Value::Array(items)) = list {
        for (index, item) in items.iter_mut().enumerate() {
            if let Value::Object(run) = item {
                visit(&format!("{name}[{index}]"), run);
            }
        }
    }
}

/// One run of a report.
type Run = Map<String, Value>;

/// Calls `visit` with the label (the JSON path) and the object of each run of
/// the report, in the order of the report: `tests`, `live_run`, each `tdd` cycle
/// with its `red` and `green` run, and the `tests` of each fix round.
fn visit_runs(report: &mut Map<String, Value>, visit: &mut impl FnMut(&str, &mut Run)) {
    for field in ["tests", "live_run"] {
        visit_list(report.get_mut(field), field, visit);
    }
    if let Some(Value::Array(cycles)) = report.get_mut("tdd") {
        for (index, cycle) in cycles.iter_mut().enumerate() {
            for part in ["red", "green"] {
                if let Some(Value::Object(run)) = cycle.get_mut(part) {
                    visit(&format!("tdd[{index}].{part}"), run);
                }
            }
        }
    }
    if let Some(Value::Array(rounds)) = report.get_mut("fix_rounds") {
        for (index, round) in rounds.iter_mut().enumerate() {
            visit_list(
                round.get_mut("tests"),
                &format!("fix_rounds[{index}].tests"),
                visit,
            );
        }
    }
}

/// Fills `command`, `exit`, and `sha256` of the run `label` from its capture, or
/// adds a finding. A run with no `capture` string is left to `validate`.
fn fill_run(root: &Path, label: &str, run: &mut Run, findings: &mut Vec<String>) {
    let Some(path) = run
        .get("capture")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let capture = match capture::read(&root.join(&path)) {
        Ok(capture) => capture,
        Err(ReadError::Missing) => {
            findings.push(format!("{label}: capture {path} does not exist"));
            return;
        }
        Err(ReadError::Unreadable(error)) => {
            findings.push(format!(
                "{label}: capture {path} could not be read ({error})"
            ));
            return;
        }
        Err(ReadError::NoCapture(reason)) => {
            findings.push(format!("{label}: {path} is no capture ({reason})"));
            return;
        }
    };
    let actual = capture.sha256();
    match run.get("sha256") {
        None => {
            run.insert("sha256".to_string(), Value::String(actual));
        }
        Some(Value::String(recorded)) if *recorded == actual => {}
        Some(recorded) => {
            let recorded = recorded
                .as_str()
                .map_or_else(|| recorded.to_string(), str::to_string);
            findings.push(format!(
                "{label}: capture {path} has sha256 {actual}, but the report records \
                 {recorded}; re-read the capture, then remove sha256 and run report-facts"
            ));
            return;
        }
    }
    run.insert(
        "command".to_string(),
        Value::String(shell::join(&capture.header.argv)),
    );
    run.insert(
        "exit".to_string(),
        capture.header.exit.map_or(Value::Null, Value::from),
    );
}

/// The fields of a `citations` entry that the judgement reads.
pub(crate) struct CitationEntry {
    /// The cited path, as the report spells it.
    pub(crate) path: String,
    /// The first cited line, at least 1.
    pub(crate) line: u64,
    /// The last cited line, at least 1, when the entry names one.
    pub(crate) end: Option<u64>,
    /// The text of the cited lines that the claim rests on.
    pub(crate) anchor: String,
}

impl CitationEntry {
    /// The fields of `entry`, or `None` when one is missing or has the wrong
    /// type, or when `line` or `end` is below 1.
    pub(crate) fn of(entry: &Map<String, Value>) -> Option<Self> {
        let at_least_one = |value: &Value| value.as_u64().filter(|number| *number >= 1);
        Some(Self {
            path: entry.get("path")?.as_str()?.to_string(),
            line: at_least_one(entry.get("line")?)?,
            end: match entry.get("end") {
                None => None,
                Some(end) => Some(at_least_one(end)?),
            },
            anchor: entry.get("anchor")?.as_str()?.to_string(),
        })
    }
}

/// Judges each entry of `citations` and gives the one without a `sha256` the hash
/// of its cited lines. A recorded hash stays.
fn fill_citations(root: &Path, report: &mut Map<String, Value>, findings: &mut Vec<String>) {
    let Some(Value::Array(entries)) = report.get_mut("citations") else {
        return;
    };
    for (index, entry) in entries.iter_mut().enumerate() {
        let Value::Object(entry) = entry else {
            continue;
        };
        let Some(cited) = CitationEntry::of(entry) else {
            continue;
        };
        match judge_citation(root, &cited.path, cited.line, cited.end, &cited.anchor) {
            Ok(lines) if !entry.contains_key("sha256") => {
                entry.insert("sha256".to_string(), Value::String(lines.sha256));
            }
            Ok(_) => {}
            Err(fault) => {
                findings.push(fault.finding(index, &cited.path, cited.line, cited.end));
            }
        }
    }
}

/// The facts and the findings of one run of the command; writes the report when
/// there is no finding.
fn fill(root: &Path, request: &Request) -> Result<(), Stop> {
    let prefix = InstallPrefix::of(root).map_err(Stop::Setup)?;
    let (resolved, mut report) = read_report(request)?;
    let commit_or_stop = |option: &str, reference: &str| {
        resolve_commit(root, reference)
            .ok_or_else(|| Stop::Setup(format!("{option} {reference}: not a commit")))
    };
    let base = commit_or_stop("--base", &request.base)?;
    let fix_base = request
        .fix_base
        .as_deref()
        .map(|reference| commit_or_stop("--fix-base", reference))
        .transpose()?;
    if fix_base.is_some() && last_fix_round(&mut report).is_none() {
        return Err(Stop::Setup(
            "--fix-base: the report has no fix_rounds entry".to_string(),
        ));
    }
    let self_audit = request.audit.as_deref().map(read_audit).transpose()?;

    report.insert("commits".to_string(), commits_since(root, &base)?);
    report.insert(
        "files_changed".to_string(),
        files_changed(root, &prefix, &base)?,
    );
    if let Some(fix_base) = &fix_base {
        let commits = commits_since(root, fix_base)?;
        if let Some(round) = last_fix_round(&mut report) {
            round.insert("commits".to_string(), commits);
        }
    }
    if let Some(self_audit) = self_audit {
        report.insert("self_audit".to_string(), self_audit);
    }
    let mut findings = Vec::new();
    visit_runs(&mut report, &mut |label, run| {
        fill_run(root, label, run, &mut findings);
    });
    fill_citations(root, &mut report, &mut findings);
    if !findings.is_empty() {
        return Err(Stop::Findings(findings));
    }
    write_report(&resolved, &request.report, &Value::Object(report))
}

/// Writes `report` to `resolved` through a temporary file in the same folder. The
/// new file keeps the permissions of the file it replaces. `given` is the path as
/// the person typed it.
fn write_report(resolved: &Path, given: &Path, report: &Value) -> Result<(), Stop> {
    let cannot_write = |error: io::Error| {
        Stop::Setup(format!(
            "{}: could not write: {}",
            given.display(),
            capture::os_error_text(&error)
        ))
    };
    let folder = resolved
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = NamedTempFile::new_in(folder).map_err(cannot_write)?;
    file.write_all(emit::emit(report).as_bytes())
        .map_err(cannot_write)?;
    if let Ok(metadata) = fs::metadata(resolved) {
        fs::set_permissions(file.path(), metadata.permissions()).map_err(cannot_write)?;
    }
    file.persist(resolved)
        .map_err(|failure| cannot_write(failure.error))?;
    Ok(())
}

/// The `report-facts` subcommand (`crate::main`'s dispatch): resolves the install
/// root first (outside a repository it prints the `--dir` remedy and exits 2,
/// before `report` is read), fills the facts of the report at `report`, and
/// prints `<report>: facts filled`. Exit status 0 when the report was written,
/// 1 when it holds a finding (one line each on stderr, nothing written), and 2 for
/// a setup error (one line on stderr, nothing written).
pub(crate) fn cmd_report_facts(
    dir: Option<PathBuf>,
    report: PathBuf,
    base: String,
    fix_base: Option<String>,
    audit: Option<PathBuf>,
) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let request = Request {
        report,
        base,
        fix_base,
        audit,
    };
    match fill(&root, &request) {
        Ok(()) => {
            println!("{}: facts filled", request.report.display());
            ExitCode::SUCCESS
        }
        Err(Stop::Setup(line)) => {
            eprintln!("{line}");
            ExitCode::from(2)
        }
        Err(Stop::Findings(lines)) => {
            for line in &lines {
                eprintln!("{line}");
            }
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The finding of a fault for the first entry of `src/a.rs:2-3`.
    fn finding(fault: &CitationFault) -> String {
        fault.finding(0, "src/a.rs", 2, Some(3))
    }

    #[test]
    fn the_empty_file_has_no_line_and_a_final_lf_adds_none() {
        assert_eq!(lines_of(b"").len(), 0);
        assert_eq!(lines_of(b"\n"), [b"".as_slice()]);
        assert_eq!(lines_of(b"a\n\n"), [b"a".as_slice(), b""]);
        assert_eq!(lines_of(b"a\nb"), [b"a".as_slice(), b"b"]);
        assert_eq!(lines_of(b"a\r\nb\r\n"), [b"a\r".as_slice(), b"b\r"]);
    }

    #[test]
    fn the_cited_lines_run_to_the_last_byte_of_the_last_line() {
        let file = b"one\r\ntwo\r\nthree";
        let cited = |line, end, anchor| judge_file(file, line, end, anchor).map(|c| c.sha256);
        assert_eq!(cited(1, None, "one"), Ok(baseline::hash(b"one\r")));
        assert_eq!(
            cited(1, Some(2), "two"),
            Ok(baseline::hash(b"one\r\ntwo\r"))
        );
        assert_eq!(cited(3, None, "three"), Ok(baseline::hash(b"three")));
        assert_eq!(cited(1, Some(3), "one\r\ntwo"), Ok(baseline::hash(file)));
    }

    #[test]
    fn the_hash_is_the_hash_of_the_cited_bytes() {
        let cited = judge_file(b"alpha\nbeta\n", 2, None, "beta").expect("a judgement");
        assert_eq!(cited.sha256, baseline::hash(b"beta"));
    }

    #[test]
    fn a_fault_is_the_first_that_the_order_of_the_spec_finds() {
        let file = b"let x = 1;\nlet y = 2;\n";
        let fault = |line, end, anchor| judge_file(file, line, end, anchor).unwrap_err();
        assert_eq!(fault(3, Some(2), "let"), CitationFault::EndBeforeLine);
        assert_eq!(
            fault(3, None, "let"),
            CitationFault::TooFewLines {
                lines: 2,
                needed: 3
            }
        );
        assert_eq!(fault(1, None, "missing"), CitationFault::AnchorNotInRange);
        assert_eq!(
            fault(1, None, "let"),
            CitationFault::AnchorRepeated { count: 2 }
        );
    }

    #[test]
    fn an_anchor_counts_at_every_position_where_it_starts() {
        assert_eq!(
            judge_file(b"aaa\n", 1, None, "aa").unwrap_err(),
            CitationFault::AnchorRepeated { count: 2 }
        );
    }

    #[test]
    fn an_empty_anchor_is_on_no_line() {
        assert_eq!(
            judge_file(b"a\n", 1, None, "").unwrap_err(),
            CitationFault::AnchorNotInRange
        );
    }

    #[test]
    fn a_line_number_of_zero_cites_nothing_and_stops_nothing() {
        assert_eq!(
            judge_file(b"a\nb\n", 0, None, "a").unwrap_err(),
            CitationFault::AnchorNotInRange
        );
        assert_eq!(
            judge_file(b"a\nb\n", 0, Some(2), "a").map(|c| c.sha256),
            Ok(baseline::hash(b"a\nb"))
        );
    }

    #[test]
    fn a_line_number_beyond_any_file_is_too_few_lines() {
        assert_eq!(
            judge_file(b"a\n", u64::MAX, None, "a").unwrap_err(),
            CitationFault::TooFewLines {
                lines: 1,
                needed: u64::MAX
            }
        );
    }

    #[test]
    fn a_range_is_named_by_its_line_and_by_its_end_when_the_end_is_greater() {
        assert_eq!(range_label(4, None), "4");
        assert_eq!(range_label(4, Some(4)), "4");
        assert_eq!(range_label(4, Some(6)), "4-6");
    }

    #[test]
    fn each_fault_gives_the_line_of_the_spec() {
        assert_eq!(
            finding(&CitationFault::NotTracked),
            "citations[0]: src/a.rs is not tracked at HEAD"
        );
        assert_eq!(
            finding(&CitationFault::EndBeforeLine),
            "citations[0]: end 3 is before line 2"
        );
        assert_eq!(
            finding(&CitationFault::TooFewLines {
                lines: 1,
                needed: 3
            }),
            "citations[0]: src/a.rs has 1 lines at HEAD, fewer than 3"
        );
        assert_eq!(
            finding(&CitationFault::AnchorNotInRange),
            "citations[0]: the anchor is not in src/a.rs:2-3 at HEAD; re-open the file at HEAD \
             and correct line and end"
        );
        assert_eq!(
            finding(&CitationFault::AnchorRepeated { count: 4 }),
            "citations[0]: the anchor occurs 4 times in src/a.rs at HEAD; choose text that \
             occurs once"
        );
    }

    #[test]
    fn a_path_with_a_dotdot_segment_or_an_absolute_path_is_not_tracked() {
        let dir = tempfile::tempdir().expect("tempdir");
        for path in ["", "..", "../a", "a/../b", "/a"] {
            assert_eq!(blob_at_head(dir.path(), path), None, "{path:?}");
        }
    }

    #[test]
    fn an_entry_needs_a_path_a_line_of_at_least_one_and_an_anchor() {
        let entry = |value: Value| CitationEntry::of(value.as_object().expect("an object"));
        let ok = entry(json!({"path": "a", "line": 2, "end": 3, "anchor": "abc"})).expect("entry");
        assert_eq!((ok.path.as_str(), ok.line, ok.end), ("a", 2, Some(3)));
        for bad in [
            json!({"line": 2, "anchor": "abc"}),
            json!({"path": "a", "anchor": "abc"}),
            json!({"path": "a", "line": 0, "anchor": "abc"}),
            json!({"path": "a", "line": 2, "end": 0, "anchor": "abc"}),
            json!({"path": "a", "line": 2, "end": "3", "anchor": "abc"}),
            json!({"path": "a", "line": 2.5, "anchor": "abc"}),
            json!({"path": "a", "line": 2}),
        ] {
            assert!(entry(bad.clone()).is_none(), "{bad}");
        }
    }
}
