//! The check runners of `audit`: one module per check type behind one row
//! builder.
//!
//! `run` matches the type of an entry's check and hands the check to the
//! module of that type. Each module exposes `run(check, ctx, rows)`: it
//! judges the check against the range that the `AuditContext` holds and
//! returns the audit row that the `RowBuilder` builds. The glob and regex
//! helpers that several runners share live here. The `runner` type starts an
//! external tool (`external`) and reads its SARIF output (`sarif`); the other
//! types need no tool.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use regress::Regex;
use serde_json::Value;

use super::check_shape::{CheckDef, CheckLevel, CheckType, Glob};
use super::git::{Commit, commits_in, removed_lines, show_file, tree_files};
use super::git_paths::InstallPrefix;
use super::glob::glob_match;
use super::model::{Base, Entry};
use crate::published::audit_result::{AuditRow, RowLevel, RowMode, RowResult};

mod co_change;
mod commits;
mod diff_append_only;
mod durable_sha;
mod external;
mod grep_absent;
mod intent;
mod report_field;
mod sarif;

pub(in crate::rules) use co_change::CoChangeCheck;
pub(in crate::rules) use commits::CommitsCheck;
pub(in crate::rules) use report_field::compare_self_audit;

/// Every string a knowledge-schema glob field holds: a `check` field
/// already narrowed to `Glob` (bare string or array of strings; a
/// JSON-Schema `["string", "array"]` property, `check_shape::Glob`'s own
/// doc).
fn glob_list(glob: &Option<Glob>) -> Vec<&str> {
    match glob {
        None => Vec::new(),
        Some(Glob::One(g)) => vec![g.as_str()],
        Some(Glob::Many(items)) => items.iter().map(String::as_str).collect(),
    }
}

/// `true` when `path` matches any glob in `glob`, on the single globset
/// engine `glob::glob_match` runs (`houserules.glob-union-matcher`).
fn match_any(path: &str, glob: &Option<Glob>) -> Result<bool, String> {
    for candidate in glob_list(glob) {
        if glob_match(path, candidate).map_err(|error| error.to_string())? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `paths` filtered to those matching any glob in `glob`, propagating a
/// malformed glob as an error instead of panicking.
pub(in crate::rules) fn filter_matching(
    paths: &[String],
    glob: &Option<Glob>,
) -> Result<Vec<String>, String> {
    let mut matched = Vec::new();
    for path in paths {
        if match_any(path, glob)? {
            matched.push(path.clone());
        }
    }
    Ok(matched)
}

/// Compiles `pattern` under `flags` with `g`/`y` stripped: a check's
/// regex is built once and reused across every commit or file in a loop,
/// so a stateful match-position flag has no place here. `regress::Regex`
/// carries no such state itself, but the flags are still stripped before
/// compiling for a consistent, documented contract.
pub(in crate::rules) fn compile_check_regex(pattern: &str, flags: &str) -> Result<Regex, String> {
    let stripped: String = flags.chars().filter(|c| *c != 'g' && *c != 'y').collect();
    Regex::with_flags(pattern, stripped.as_str()).map_err(|error| error.to_string())
}

/// The per-check evaluation context: the range's changed files, the
/// `--report`/`--workspace` inputs and the `--ids` a `report-field` check
/// reads, and the tree/blob/commit reads every check type may need, each
/// cached after its first read. A `RefCell` per cache lets every check share
/// one immutable `&AuditContext` while still mutating the cache on first use.
/// A `report-field` check also records here the id of each row that found a
/// filled `self_audit` (`note_self_audit_row`), for the comparison that
/// `compare_self_audit` makes.
pub(super) struct AuditContext<'a> {
    knowledge: &'a Base,
    root: &'a Path,
    prefix: &'a InstallPrefix,
    base_sha: String,
    head_sha: String,
    changed: &'a [String],
    ids: &'a [String],
    report: Option<&'a Value>,
    reports: Option<&'a [(String, Value)]>,
    show_cache: RefCell<HashMap<String, String>>,
    tree_cache: RefCell<Option<Vec<String>>>,
    commits_cache: RefCell<Option<Vec<Commit>>>,
    self_audit_rows: RefCell<Vec<String>>,
}

impl<'a> AuditContext<'a> {
    /// The context of the range `base_sha..head_sha` in the install of
    /// `knowledge`, whose changed files are `changed`. `ids` are the ids of
    /// `--ids`. `report` is the single report of a `--report` run and
    /// `reports` the reports of a `--workspace` run.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        knowledge: &'a Base,
        prefix: &'a InstallPrefix,
        base_sha: String,
        head_sha: String,
        changed: &'a [String],
        ids: &'a [String],
        report: Option<&'a Value>,
        reports: Option<&'a [(String, Value)]>,
    ) -> Self {
        Self {
            knowledge,
            root: &knowledge.root,
            prefix,
            base_sha,
            head_sha,
            changed,
            ids,
            report,
            reports,
            show_cache: RefCell::new(HashMap::new()),
            tree_cache: RefCell::new(None),
            commits_cache: RefCell::new(None),
            self_audit_rows: RefCell::new(Vec::new()),
        }
    }

    /// Records that the row of the entry `id` found a filled `self_audit` in
    /// the report.
    fn note_self_audit_row(&self, id: &str) {
        self.self_audit_rows.borrow_mut().push(id.to_string());
    }

    /// The ids of the entries whose rows found a filled `self_audit`, in the
    /// order of the checks.
    pub(super) fn self_audit_rows(&self) -> Vec<String> {
        self.self_audit_rows.borrow().clone()
    }

    /// `path`'s content at `head_sha`, read once.
    fn show(&self, path: &str) -> Result<String, String> {
        if let Some(cached) = self.show_cache.borrow().get(path) {
            return Ok(cached.clone());
        }
        let content = show_file(self.root, &self.head_sha, path)?;
        self.show_cache
            .borrow_mut()
            .insert(path.to_string(), content.clone());
        Ok(content)
    }

    /// Every file of the tree at `head_sha`, read once.
    fn tree(&self) -> Result<Vec<String>, String> {
        if let Some(cached) = self.tree_cache.borrow().as_ref() {
            return Ok(cached.clone());
        }
        let files = tree_files(self.root, self.prefix, &self.head_sha)?;
        *self.tree_cache.borrow_mut() = Some(files.clone());
        Ok(files)
    }

    /// The commits of the range, with their trailers, read once.
    pub(super) fn commits(&self) -> Result<Vec<Commit>, String> {
        if let Some(cached) = self.commits_cache.borrow().as_ref() {
            return Ok(cached.clone());
        }
        let commits = commits_in(self.root, &self.base_sha, &self.head_sha, true)?;
        *self.commits_cache.borrow_mut() = Some(commits.clone());
        Ok(commits)
    }

    /// Every line that the range removes from `files`.
    fn removed(&self, files: &[String]) -> Result<Vec<String>, String> {
        removed_lines(self.root, &self.base_sha, &self.head_sha, files)
    }
}

/// Builds the audit rows of one entry's check. Every row carries the id and
/// kind of the entry, the mode `deterministic`, and the level of the check.
struct RowBuilder<'a> {
    entry: &'a Entry,
    level: CheckLevel,
}

impl RowBuilder<'_> {
    /// The level of the check as a row prints it.
    fn row_level(&self) -> RowLevel {
        match self.level {
            CheckLevel::Fail => RowLevel::Fail,
            CheckLevel::Warn => RowLevel::Warn,
        }
    }

    /// The row of the entry with `result` and `evidence`.
    fn row(&self, result: RowResult, evidence: String) -> AuditRow {
        AuditRow {
            id: self.entry.id.clone(),
            kind: self.entry.kind.clone(),
            mode: RowMode::Deterministic,
            level: Some(self.row_level()),
            result,
            evidence,
            findings: None,
            sanctioned: None,
        }
    }

    /// The row of a check that holds.
    fn pass(&self, evidence: String) -> AuditRow {
        self.row(RowResult::Pass, evidence)
    }

    /// The row of a violated check: `fail` or `warn` by the level of the
    /// check.
    fn violate(&self, evidence: String) -> AuditRow {
        self.row(self.row_level().violation(), evidence)
    }
}

/// Runs one entry's deterministic `check` and returns its audit row (the
/// module doc of `audit` says why the row type is not a deliverables type).
/// `check` is passed separately from `entry` (rather than read back off
/// `entry.check`) so the caller's own `CheckField` match is the one place that
/// decides "does this entry get its check run at all".
pub(super) fn run(entry: &Entry, check: &CheckDef, ctx: &AuditContext) -> Result<AuditRow, String> {
    let rows = RowBuilder {
        entry,
        level: check.level,
    };
    match check.kind {
        CheckType::GrepAbsent => grep_absent::run(check, ctx, &rows),
        CheckType::Commits => commits::run(check, ctx, &rows),
        CheckType::CoChange => co_change::run(check, ctx, &rows),
        CheckType::DiffAppendOnly => diff_append_only::run(check, ctx, &rows),
        CheckType::ReportField => report_field::run(check, ctx, &rows),
        CheckType::DurableSha => durable_sha::run(check, ctx, &rows),
        CheckType::Runner => external::run(check, ctx, &rows),
        CheckType::IntentRequired => intent::run_required(check, ctx, &rows),
        CheckType::IntentResolves => intent::run_resolves(ctx, &rows),
    }
}
