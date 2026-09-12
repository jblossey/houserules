//! The `audit` command: builds the rule package for a git range, runs
//! every member's deterministic check, and reports the result, plus the
//! git-plumbing helpers it runs on (`rev`, `git_diff`, `changed_files`,
//! `tree_files`, `show_file`, `commits_in`, `removed_lines`).
//!
//! ## Data-layer decisions
//!
//! - Each entry's `check` object is read through `check_shape::CheckDef`
//!   (`model::Entry.check`, typed `CheckField`), not raw `serde_json::Value`
//!   field access: it is a fixed, closed schema (`knowledge/schema.json`'s
//!   `$defs/check`), and `run_check` matches on its `type`/`level`/`scope`
//!   for every branch, exactly the "match on a type instead of a Value"
//!   case the rule favors. See `model::CheckField`'s own doc for how a
//!   malformed `check` is told apart from an absent one: this file's own
//!   row-building match (below) reports a `Malformed` check as a named,
//!   exit-2 error, never a judged row.
//! - `audit`'s own JSON output (`{base, head, ids, changed_files, areas,
//!   area_files, rules, summary}`) is built as `serde_json::Value`
//!   directly, not through any deliverables-schema type. This is not a
//!   parse-tolerance judgment (nothing here is *read* untyped) but a
//!   *shape* one: `audit`'s judged rows carry `result: "open"` — a value
//!   `.claude/schemas/deliverables.json`'s `auditRow.result` enum
//!   (`pass`/`fail`/`warn`/`skipped`) explicitly forbids. A report author
//!   copies only this command's *deterministic* rows into a report's
//!   `self_audit` (the retrieval protocol's own words: "never hand-written
//!   rows; the judged rows are the reviewer's") -- the raw `open` rows
//!   never reach a schema-validated file at all. Reusing the schema-pinned
//!   `AuditRow`/`AuditRowResult` types here would either make them accept a
//!   value the schema itself forbids (defeating their whole purpose as a
//!   schema-exact pin) or require a second, audit-only row representation
//!   alongside them -- more moving parts than one `Value` builder.
//! - This is also why this crate carries no typed deliverables model:
//!   `validate` (see `validate_deliverable.rs`'s module doc) and `stats`
//!   both aggregate through tolerant `Value` reads too, so no command
//!   constructs or strictly parses a schema-exact deliverable anywhere in
//!   this binary. `check_shape.rs`'s `CheckDef` is the one model layer
//!   that DOES get a real, direct consumer here (the bullet above) and
//!   stays.
//!
//! ## The crash-path extension this file adds
//!
//! A malformed check `pattern`/`subject`/`body_absent` regex, or a
//! malformed `files`/`if`/`then` glob, reaching `run_check` would panic
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

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use regress::Regex;
use serde_json::{Value, json};

use crate::emit::emit;

use super::check_shape::{CheckDef, CheckType, Glob, Scope};
use super::deliverable::{read_deliverable_value, workspace_files};
use super::glob::{area_files, glob_match};
use super::model::{Base, Entry, load_base};
use super::render::RULE_KINDS;

// ---- git plumbing ------------------------------------------------------------------

/// One captured `git` failure: the process's stderr (or, when `git` could
/// not even be launched, the OS error's own text standing in for it) --
/// the raw material `rev`/`git_diff`/the tree/show/log helpers each turn
/// into their own one-line message.
struct RawGitError {
    stderr: String,
}

/// Runs `git` with `args` inside `root`, forcing `LC_ALL=C` for
/// locale-independent output.
fn run_git(root: &Path, args: &[&str]) -> Result<String, RawGitError> {
    match Command::new("git")
        .args(args)
        .env("LC_ALL", "C")
        .current_dir(root)
        .output()
    {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => Err(RawGitError {
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }),
        Err(error) => Err(RawGitError {
            stderr: error.to_string(),
        }),
    }
}

/// The first non-blank line of `stderr`, trimmed, with a generic message
/// for the case of a nonzero exit with fully empty stderr.
fn stderr_headline(error: &RawGitError) -> String {
    match error.stderr.lines().find(|line| !line.trim().is_empty()) {
        Some(line) => line.trim().to_string(),
        None => "git command failed".to_string(),
    }
}

/// Resolves `reference` to its short commit sha. Any failure (a bad ref,
/// or git itself failing to run) is `bad ref "<reference>"`, discarding
/// git's own message. `pub(super)`: `check_commit`'s own range arm
/// resolves its `--from`/`--to` refs through this same function, so a bad
/// ref reads identically from either command.
pub(super) fn rev(root: &Path, reference: &str) -> Result<String, String> {
    run_git(
        root,
        &[
            "rev-parse",
            "--short",
            "--verify",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .map(|output| output.trim().to_string())
    .map_err(|_| format!("bad ref \"{reference}\""))
}

/// A three-dot `git diff` range from the merge base of `base` and `head`
/// to `head`.
fn range(base: &str, head: &str) -> String {
    format!("{base}...{head}")
}

/// Splits `text` into non-empty lines.
fn lines(text: &str) -> Vec<String> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

/// Runs `git diff` with `args` (everything after `diff`). A shared merge
/// base miss between `base` and `head` reports
/// the fixed `no merge base between "<base>" and "<head>"` message; any
/// other failure carries git's own first non-empty stderr line.
fn git_diff(root: &Path, base: &str, head: &str, args: &[&str]) -> Result<String, String> {
    let mut full_args = vec!["diff"];
    full_args.extend_from_slice(args);
    run_git(root, &full_args).map_err(|error| {
        if error.stderr.contains("no merge base") {
            format!("no merge base between \"{base}\" and \"{head}\"")
        } else {
            stderr_headline(&error)
        }
    })
}

/// Every file added, copied, modified, or renamed between `base` and
/// `head`'s merge base and `head`.
fn changed_files(root: &Path, base: &str, head: &str) -> Result<Vec<String>, String> {
    let output = git_diff(
        root,
        base,
        head,
        &["--name-only", "--diff-filter=ACMR", &range(base, head)],
    )?;
    Ok(lines(&output))
}

/// Every file in `head`'s tree.
fn tree_files(root: &Path, head: &str) -> Result<Vec<String>, String> {
    let output =
        run_git(root, &["ls-tree", "-r", "--name-only", head]).map_err(|e| stderr_headline(&e))?;
    Ok(lines(&output))
}

/// `path`'s content as it exists in `head`.
fn show_file(root: &Path, head: &str, path: &str) -> Result<String, String> {
    run_git(root, &["show", &format!("{head}:{path}")]).map_err(|e| stderr_headline(&e))
}

/// One commit as the `commits` checks read it: its subject, its body, and
/// its author name (`%an`), the last so a check can tell a generated
/// message from a human one (see [`CommitsCheck::violation`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Commit {
    pub(super) subject: String,
    pub(super) body: String,
    pub(super) author: String,
}

impl Commit {
    /// A message with no author on record: the `check-commit` hook arm,
    /// which reads a message file before git has attached an author.
    pub(super) fn unattributed(subject: String, body: String) -> Self {
        Self {
            subject,
            body,
            author: String::new(),
        }
    }

    /// GitHub App and Dependabot commits carry an author name ending in
    /// `[bot]`; their bodies are generated (release links, metadata) and
    /// are not wrapped by a human.
    fn is_bot_authored(&self) -> bool {
        self.author.ends_with("[bot]")
    }
}

/// Every commit strictly between `base` and `head` (two-dot range -- not
/// `range()`'s three-dot merge-base form), plus the author name.
/// `pub(super)`: `check_commit`'s own range arm
/// reads a git range through this same function, not a second `git log`
/// invocation of its own.
pub(super) fn commits_in(root: &Path, base: &str, head: &str) -> Result<Vec<Commit>, String> {
    let output = run_git(
        root,
        &[
            "log",
            "--format=%an%x00%s%x00%b%x1e",
            &format!("{base}..{head}"),
        ],
    )
    .map_err(|e| stderr_headline(&e))?;
    Ok(output
        .split('\u{1e}')
        .map(|record| record.strip_prefix('\n').unwrap_or(record))
        .filter(|record| !record.is_empty())
        .map(|record| {
            let mut parts = record.splitn(3, '\u{0}');
            let author = parts.next().unwrap_or_default().to_string();
            let subject = parts.next().unwrap_or_default().to_string();
            let body = parts.next().unwrap_or_default().to_string();
            Commit {
                subject,
                body,
                author,
            }
        })
        .collect())
}

/// Every removed (`-`-prefixed, excluding the `---` file header) line in
/// `files`'s diff between `base` and `head`.
fn removed_lines(
    root: &Path,
    base: &str,
    head: &str,
    files: &[String],
) -> Result<Vec<String>, String> {
    let mut args = vec![range(base, head), "--".to_string()];
    args.extend(files.iter().cloned());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git_diff(root, base, head, &arg_refs)?;
    Ok(output
        .lines()
        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
        .map(str::to_string)
        .collect())
}

// ---- check runners --------------------------------------------------------------------

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
fn filter_matching(paths: &[String], glob: &Option<Glob>) -> Result<Vec<String>, String> {
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
fn compile_check_regex(pattern: &str, flags: &str) -> Result<Regex, String> {
    let stripped: String = flags.chars().filter(|c| *c != 'g' && *c != 'y').collect();
    Regex::with_flags(pattern, stripped.as_str()).map_err(|error| error.to_string())
}

/// One `commits`-type check, its `subject`/`body_absent` patterns compiled
/// once -- the per-commit evaluation `run_check`'s `CheckType::Commits` arm
/// and `check_commit::check_commit` both run, factored here so the two
/// commands can never drift apart on what
/// counts as a violation or how it reads (`houserules check-commit` reuses
/// the audit's `commits` rules; it does not reimplement them). `pub(super)`
/// rather than `pub(crate)`: `check_commit` is a sibling module under
/// `rules`, and nothing outside this module needs it.
pub(super) struct CommitsCheck<'a> {
    check: &'a CheckDef,
    subject_re: Option<Regex>,
    body_re: Option<Regex>,
}

impl<'a> CommitsCheck<'a> {
    /// Compiles `check`'s `subject`/`body_absent` patterns under its own
    /// `flags` (`g`/`y` stripped, `compile_check_regex`) -- a malformed
    /// pattern is a named `Err`, never a panic
    /// (`houserules.crash-paths-are-named`).
    pub(super) fn compile(check: &'a CheckDef) -> Result<Self, String> {
        let flags = check.flags.as_deref().unwrap_or_default();
        let subject_re = match &check.subject {
            Some(s) if !s.is_empty() => Some(compile_check_regex(s, flags)?),
            _ => None,
        };
        let body_re = match &check.body_absent {
            Some(s) if !s.is_empty() => Some(compile_check_regex(s, flags)?),
            _ => None,
        };
        Ok(Self {
            check,
            subject_re,
            body_re,
        })
    }

    /// Evaluates one commit against this check, in the same
    /// subject/body_absent/body_line_max order `run_check`'s own commit
    /// loop tests them: `Some(evidence)` for the first rule this commit
    /// breaks, `None` when the commit satisfies every rule this check
    /// declares. `violation` is the single place that builds this
    /// evidence text; both `run_check`'s `CheckType::Commits` arm and
    /// `check_commit` call it, so the two commands' output cannot drift
    /// apart. A bot-authored commit (`[bot]` author suffix) is exempt
    /// from `body_line_max` alone: a generated body (Dependabot's compare
    /// links, say) carries lines no human wraps; its subject and
    /// `body_absent` rules still apply.
    pub(super) fn violation(&self, commit: &Commit) -> Option<String> {
        let (subject, body) = (commit.subject.as_str(), commit.body.as_str());
        if let Some(re) = &self.subject_re
            && re.find(subject).is_none()
        {
            return Some(format!(
                "commit \"{subject}\" does not match {}",
                self.check.subject.as_deref().unwrap_or_default()
            ));
        }
        if let Some(re) = &self.body_re
            && body.split('\n').any(|line| re.find(line).is_some())
        {
            return Some(format!(
                "commit \"{subject}\" body matches {}",
                self.check.body_absent.as_deref().unwrap_or_default()
            ));
        }
        if let Some(limit) = self.check.body_line_max.filter(|&limit| limit > 0)
            && !commit.is_bot_authored()
            && body
                .split('\n')
                .any(|line| line.encode_utf16().count() as u64 > limit)
        {
            return Some(format!(
                "commit \"{subject}\" has a body line over {limit} characters"
            ));
        }
        None
    }

    /// This check's declared `level`: `check_commit`'s own outcome carries
    /// this forward onto each finding the same way `run_check`'s
    /// `violated_result` already reads
    /// `check.level` to decide a row's `result` (`"warn"` vs `"fail"`), so
    /// a warn-level `commits` check cannot silently escalate into a hard
    /// block on either surface.
    pub(super) fn level(&self) -> super::check_shape::CheckLevel {
        self.check.level
    }
}

/// The value at a dot-separated `field` path in `data`, indexing into
/// either an object's keys or an array's numeric-string indices.
fn field_value<'a>(data: &'a Value, field: &str) -> Option<&'a Value> {
    field.split('.').try_fold(data, |node, key| match node {
        Value::Object(map) => map.get(key),
        Value::Array(items) => key.parse::<usize>().ok().and_then(|index| items.get(index)),
        _ => None,
    })
}

/// `true` when `field_value` is present and not `null`.
fn has_field(data: &Value, field: &str) -> bool {
    !matches!(field_value(data, field), None | Some(Value::Null))
}

/// The per-check evaluation context: the range's changed files, the
/// `--report`/`--workspace` inputs a `report-field` check reads, and the
/// tree/blob/commit git reads every check type may need, each cached
/// after its first read. A `RefCell` per cache lets every check share one
/// immutable `&AuditContext` while still mutating the cache on first use.
struct AuditContext<'a> {
    root: &'a Path,
    base_sha: String,
    head_sha: String,
    changed: &'a [String],
    report: Option<&'a Value>,
    reports: Option<&'a [(String, Value)]>,
    show_cache: RefCell<HashMap<String, String>>,
    tree_cache: RefCell<Option<Vec<String>>>,
    commits_cache: RefCell<Option<Vec<Commit>>>,
}

impl AuditContext<'_> {
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

    fn tree(&self) -> Result<Vec<String>, String> {
        if let Some(cached) = self.tree_cache.borrow().as_ref() {
            return Ok(cached.clone());
        }
        let files = tree_files(self.root, &self.head_sha)?;
        *self.tree_cache.borrow_mut() = Some(files.clone());
        Ok(files)
    }

    fn commits(&self) -> Result<Vec<Commit>, String> {
        if let Some(cached) = self.commits_cache.borrow().as_ref() {
            return Ok(cached.clone());
        }
        let commits = commits_in(self.root, &self.base_sha, &self.head_sha)?;
        *self.commits_cache.borrow_mut() = Some(commits.clone());
        Ok(commits)
    }

    fn removed(&self, files: &[String]) -> Result<Vec<String>, String> {
        removed_lines(self.root, &self.base_sha, &self.head_sha, files)
    }
}

/// Runs one entry's deterministic `check` and returns its audit row (see
/// this module's doc for why the row is a raw `Value`, not a typed
/// model). `check` is passed separately from `entry` (rather than read
/// back off `entry.check`) so the caller's own `CheckField` match is the
/// one place that decides "does this entry get its check run at all".
fn run_check(entry: &Entry, check: &CheckDef, ctx: &AuditContext) -> Result<Value, String> {
    let level_str = match check.level {
        super::check_shape::CheckLevel::Fail => "fail",
        super::check_shape::CheckLevel::Warn => "warn",
    };
    let violated_result = if check.level == super::check_shape::CheckLevel::Warn {
        "warn"
    } else {
        "fail"
    };
    let row = |result: &str, evidence: String| -> Value {
        json!({
            "id": entry.id, "kind": entry.kind, "mode": "deterministic",
            "level": level_str, "result": result, "evidence": evidence,
        })
    };
    let pass = |evidence: String| row("pass", evidence);
    let violate = |evidence: String| row(violated_result, evidence);

    match check.kind {
        CheckType::GrepAbsent => {
            let pool: Vec<String> = if check.scope == Some(Scope::Tree) {
                ctx.tree()?
            } else {
                ctx.changed.to_vec()
            };
            let files = filter_matching(&pool, &check.files)?;
            let pattern = check.pattern.as_deref().unwrap_or_default();
            let regex = compile_check_regex(pattern, check.flags.as_deref().unwrap_or_default())?;
            for path in &files {
                let text = ctx.show(path)?;
                if let Some(found) = regex.find(&text) {
                    let line = text[..found.start()].matches('\n').count() + 1;
                    return Ok(violate(format!("{path}:{line} matches {pattern}")));
                }
            }
            Ok(pass(format!("{} files checked", files.len())))
        }
        CheckType::Commits => {
            let compiled = CommitsCheck::compile(check)?;
            let commits = ctx.commits()?;
            for commit in &commits {
                if let Some(evidence) = compiled.violation(commit) {
                    return Ok(violate(evidence));
                }
            }
            Ok(pass(format!("{} commits checked", commits.len())))
        }
        CheckType::CoChange => {
            let trigger = filter_matching(ctx.changed, &check.if_changed)?;
            if trigger.is_empty() {
                return Ok(pass("not triggered".to_string()));
            }
            // `check.then` has not been matched yet at this point (`trigger`
            // above matches `if_changed`, a different field) -- both loops
            // below propagate a malformed `then` with `?`, the same as
            // `filter_matching` already does for `files`/`if_changed`: there
            // is no `unwrap_or(false)` here to hide a `GlobError` behind a
            // false "did not match".
            let mut satisfying: Option<&String> = None;
            for path in ctx.changed {
                if match_any(path, &check.then)? {
                    satisfying = Some(path);
                    break;
                }
            }
            if let Some(satisfying) = satisfying {
                let mut real_trigger: Option<&String> = None;
                for path in &trigger {
                    if !match_any(path, &check.then)? {
                        real_trigger = Some(path);
                        break;
                    }
                }
                let evidence = match real_trigger {
                    Some(t) => format!("{t} changed with {satisfying}"),
                    None => format!(
                        "only {} changed; the co-change is satisfied by definition",
                        trigger.join(", ")
                    ),
                };
                return Ok(pass(evidence));
            }
            Ok(violate(format!(
                "{} changed without {}",
                trigger[0],
                glob_list(&check.then).join(" or ")
            )))
        }
        CheckType::DiffAppendOnly => {
            let files = filter_matching(ctx.changed, &check.files)?;
            if files.is_empty() {
                return Ok(pass("not triggered".to_string()));
            }
            let removed = ctx.removed(&files)?;
            if !removed.is_empty() {
                return Ok(violate(format!(
                    "{} removed lines in {}",
                    removed.len(),
                    files.join(", ")
                )));
            }
            Ok(pass(format!("{}: no removed lines", files.join(", "))))
        }
        CheckType::ReportField => {
            let trigger = filter_matching(ctx.changed, &check.if_changed)?;
            if trigger.is_empty() {
                return Ok(pass("not triggered".to_string()));
            }
            let field = check.field.as_deref().unwrap_or_default();
            if let Some(report) = ctx.report {
                if has_field(report, field) {
                    return Ok(pass(format!("report field {field} is set")));
                }
                return Ok(violate(format!(
                    "report lacks a value for {field} (triggered by {})",
                    trigger[0]
                )));
            }
            if let Some(reports) = ctx.reports {
                // A `files_changed` array holding a non-string element is
                // just as unusable for glob-matching as one that is not an
                // array at all, so both shapes are named the same "lacks
                // files_changed" finding here, not silently filtered
                // element-by-element.
                let has_valid_files_changed = |data: &Value| matches!(data.get("files_changed"), Some(Value::Array(items)) if items.iter().all(Value::is_string));
                let malformed = reports
                    .iter()
                    .find(|(_, data)| !has_valid_files_changed(data));
                if let Some((name, _)) = malformed {
                    return Ok(violate(format!("{name} lacks files_changed")));
                }
                fn report_files_changed(data: &Value) -> Vec<&str> {
                    data.get("files_changed")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .collect()
                }
                // The first of a report's `files_changed` matching `check.if_changed`,
                // or `None` when none do -- propagates a malformed glob with `?`
                // instead of `unwrap_or(false)`, which would hide it behind a
                // false "did not match".
                fn matching_file<'a>(
                    data: &'a Value,
                    globs: &Option<Glob>,
                ) -> Result<Option<&'a str>, String> {
                    for f in report_files_changed(data) {
                        if match_any(f, globs)? {
                            return Ok(Some(f));
                        }
                    }
                    Ok(None)
                }
                let mut hits: Vec<&(String, Value)> = Vec::new();
                for entry in reports {
                    if matching_file(&entry.1, &check.if_changed)?.is_some() {
                        hits.push(entry);
                    }
                }
                if hits.is_empty() {
                    return Ok(pass("not triggered by any report".to_string()));
                }
                for (name, data) in &hits {
                    if !has_field(data, field) {
                        let file = matching_file(data, &check.if_changed)?.unwrap_or_default();
                        return Ok(violate(format!(
                            "{name} lacks a value for {field} (triggered by {file})"
                        )));
                    }
                }
                return Ok(pass(format!(
                    "report field {field} is set in {} reports",
                    hits.len()
                )));
            }
            Ok(row("skipped", "no --report given".to_string()))
        }
    }
}

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
    pub json: Option<PathBuf>,
    /// Each `--sanctioned <rule>=<ref>` as a `(rule id, reference)` pair,
    /// in the order given; a rule id may repeat (unusual, not rejected).
    pub sanctioned: Vec<(String, String)>,
}

/// `audit`'s result: the full JSON result value, and whether any row's
/// `result` is `fail`.
#[derive(Debug)]
pub(crate) struct AuditOutcome {
    pub result: Value,
    pub failed: bool,
}

/// `area_file_map` as a JSON object, preserving its own insertion order --
/// `glob::area_files` already produces the order this command's own JSON
/// output ships (`global` first, then each area the first time one of
/// `changed`'s paths matches it; that function's own doc has the fuller
/// account), so this is a direct `Value` conversion, no reordering.
/// Alphabetically sorting the keys here instead would be invisible to a
/// test comparing parsed `Value`s (key order is not part of a JSON
/// object's value under equality), but every LIVE `houserules
/// audit --json` run is a real difference a byte-comparing user or script
/// can see.
fn area_files_json(area_file_map: &indexmap::IndexMap<String, Vec<String>>) -> Value {
    let map: serde_json::Map<String, Value> = area_file_map
        .iter()
        .map(|(key, paths)| (key.clone(), json!(paths)))
        .collect();
    Value::Object(map)
}

/// `true` when `row`'s `mode` is `"deterministic"`.
fn is_deterministic(row: &Value) -> bool {
    row.get("mode").and_then(Value::as_str) == Some("deterministic")
}

/// Builds the rule package for a git range and runs every member's
/// deterministic check. See this module's doc for why the result is a
/// raw `Value`.
pub(crate) fn audit(base: &Base, opts: AuditOptions) -> Result<AuditOutcome, String> {
    let Some(base_ref) = opts.base_ref else {
        return Err("audit needs --base <ref>".to_string());
    };
    if opts.report.is_some() && opts.workspace.is_some() {
        return Err("audit takes --report or --workspace, not both".to_string());
    }
    let root = &base.root;
    let base_sha = rev(root, &base_ref)?;
    let head_sha = rev(root, opts.head_ref.as_deref().unwrap_or("HEAD"))?;
    let changed = changed_files(root, &base_sha, &head_sha)?;
    let changed_refs: Vec<&str> = changed.iter().map(String::as_str).collect();
    let area_file_map =
        area_files(&changed_refs, &base.areas).map_err(|error| error.to_string())?;
    let mut areas: Vec<String> = area_file_map.keys().cloned().collect();
    areas.sort();

    let mut package: HashMap<String, &Entry> = HashMap::new();
    for entry in base.entries.values() {
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
            .ok_or_else(|| format!("unknown id \"{id}\""))?;
        package.insert(id.clone(), entry);
    }

    let report_value = match &opts.report {
        Some(path) => Some(read_deliverable_value(path)?),
        None => None,
    };
    let reports_value: Option<Vec<(String, Value)>> = match &opts.workspace {
        Some(dir) => {
            let files = workspace_files(dir)?;
            let mut reports = Vec::with_capacity(files.reports.len());
            for name in files.reports {
                let data = read_deliverable_value(&dir.join(&name))?;
                reports.push((name, data));
            }
            Some(reports)
        }
        None => None,
    };

    let ctx = AuditContext {
        root,
        base_sha: base_sha.clone(),
        head_sha: head_sha.clone(),
        changed: &changed,
        report: report_value.as_ref(),
        reports: reports_value.as_deref(),
        show_cache: RefCell::new(HashMap::new()),
        tree_cache: RefCell::new(None),
        commits_cache: RefCell::new(None),
    };

    let mut ids: Vec<&String> = package.keys().collect();
    ids.sort();
    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        let entry = package[id];
        let row = match &entry.check {
            super::model::CheckField::Valid(check) => run_check(entry, check, &ctx)?,
            // A malformed check is a named error, never a silently
            // downgraded judged row (`houserules.crash-paths-are-named`).
            super::model::CheckField::Malformed => {
                return Err(format!("{id}: malformed check"));
            }
            super::model::CheckField::Absent => json!({
                "id": entry.id, "kind": entry.kind, "mode": "judged",
                "level": Value::Null, "result": "open", "evidence": "\u{2014}",
            }),
        };
        rows.push(row);
    }

    // ---- the sanctioned-fail annotation ----
    //
    // A `--sanctioned <rule>=<ref>` declares a spec-booked interim fail
    // once per dispatch, instead of narrating it by hand across every
    // audit and review round the interim fail spans. Semantics, defended
    // against the row shape above: the sanctioned rule's row keeps its
    // TRUE `result` -- a fail stays "fail", never silently becomes "pass"
    // -- and gains the declaration in its `evidence` text alone, so
    // `auditRow`'s schema shape (id/kind/mode/level/result/evidence,
    // unchanged) needs no change at all. `parse_sanctioned` already ruled
    // out an empty reference and a repeated rule id, so every remaining
    // case names a real row: absent from the package entirely (a typo
    // `--ids` itself would reject) is this function's own named error,
    // the same "unknown id" shape `--ids` already uses one flag over;
    // present but not `result: "fail"` is a stale booking, reported in
    // `stale_sanctions` and folded into `failed`, never a silent no-op --
    // distinguishing the two matters because reading "did not fail" for a
    // rule that never ran would send an author to un-book a typo instead
    // of fixing it.
    let mut stale_sanctions: Vec<String> = Vec::new();
    let mut sanctioned_fail = 0usize;
    for (rule, reference) in &opts.sanctioned {
        let Some(row) = rows
            .iter_mut()
            .find(|row| row.get("id").and_then(Value::as_str) == Some(rule.as_str()))
        else {
            return Err(format!("unknown id \"{rule}\" for --sanctioned"));
        };
        let result = row
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if result == "fail" {
            let evidence = row
                .get("evidence")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            row["evidence"] = Value::String(format!("{evidence} (sanctioned: {reference})"));
            sanctioned_fail += 1;
        } else {
            stale_sanctions.push(format!(
                "--sanctioned {rule}={reference}: row result is \"{result}\", not fail"
            ));
        }
    }

    let deterministic_count = rows.iter().filter(|r| is_deterministic(r)).count();
    let count_where = |result: &str| {
        rows.iter()
            .filter(|r| {
                is_deterministic(r) && r.get("result").and_then(Value::as_str) == Some(result)
            })
            .count()
    };
    let pass = count_where("pass");
    let fail = count_where("fail");
    let warn = count_where("warn");
    let skipped = count_where("skipped");
    let judged = rows.len() - deterministic_count;

    let commits = ctx.commits()?;
    let empty_range = commits.is_empty();
    if empty_range {
        for row in rows.iter_mut().filter(|r| is_deterministic(r)) {
            let evidence = row
                .get("evidence")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            row["evidence"] = Value::String(format!("empty range: {evidence}"));
        }
    }

    let mut summary = json!({
        "base": base_sha, "head": head_sha, "deterministic": deterministic_count,
        "pass": pass, "fail": fail, "warn": warn, "skipped": skipped, "judged": judged,
    });
    if empty_range {
        summary["empty_range"] = Value::Bool(true);
    }
    // Additive, like `empty_range` above: omitted entirely rather than
    // printed as 0, so an audit run with no `--sanctioned` at all keeps the
    // same summary shape an audit with no empty range does.
    if sanctioned_fail > 0 {
        summary["sanctioned_fail"] = json!(sanctioned_fail);
    }

    // A sanctioned fail still counts in `fail` above (its `result` never
    // changed) and still fails the run; a stale booking fails the run too,
    // even though every real row passed, since a stale booking is a
    // finding, not a no-op.
    let failed = rows
        .iter()
        .any(|row| row.get("result").and_then(Value::as_str) == Some("fail"))
        || !stale_sanctions.is_empty();
    let mut result = json!({
        "base": base_sha, "head": head_sha, "ids": opts.ids, "changed_files": changed,
        "areas": areas, "area_files": area_files_json(&area_file_map),
        "rules": rows, "summary": summary,
    });
    if !stale_sanctions.is_empty() {
        result["stale_sanctions"] = json!(stale_sanctions);
    }

    if let Some(json_path) = &opts.json {
        std::fs::write(json_path, emit(&result))
            .map_err(|error| format!("{}: {error}", json_path.display()))?;
    }

    Ok(AuditOutcome { result, failed })
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

/// Runs the `audit` subcommand: resolves `root` (`--dir`, or the enclosing
/// git repository's top level), loads the knowledge base there, parses
/// `--ids` as a comma-separated, trimmed, non-empty list and
/// `--sanctioned` per `parse_sanctioned`, and prints the JSON result.
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
            json,
            sanctioned,
        },
    );
    match outcome {
        Ok(outcome) => {
            print!("{}", emit(&outcome.result));
            if outcome.failed {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::process::Command;

    use serde_json::json;

    use super::*;
    use crate::rules::model::load_base;

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

    fn seed_schema() -> Value {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json");
        let mut schema: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        schema["$defs"]["area"]["enum"] = json!([
            "global", "process", "rust", "webview", "api", "schemas", "infra", "docs"
        ]);
        schema
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

        let json_path = root.join("audit.json");
        let base = load_base(root).expect("load base");
        let outcome = audit(
            &base,
            AuditOptions {
                base_ref: Some(base_sha),
                head_ref: Some("HEAD".to_string()),
                ids: vec!["process.proc".to_string()],
                json: Some(json_path.clone()),
                ..Default::default()
            },
        )
        .expect("audit");
        assert!(outcome.failed);

        let rows = outcome.result["rules"].as_array().unwrap().clone();
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

        let summary = &outcome.result["summary"];
        assert_eq!(summary["base"], outcome.result["base"]);
        assert_eq!(summary["head"], outcome.result["head"]);
        assert_eq!(summary["deterministic"], json!(6));
        assert_eq!(summary["pass"], json!(0));
        assert_eq!(summary["fail"], json!(3));
        assert_eq!(summary["warn"], json!(2));
        assert_eq!(summary["skipped"], json!(1));
        assert_eq!(summary["judged"], json!(2));
        assert!(summary.get("empty_range").is_none());

        let data: Value =
            serde_json::from_str(&std::fs::read_to_string(&json_path).unwrap()).unwrap();
        assert_eq!(data, outcome.result);
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
        let ids: Vec<&str> = outcome.result["rules"]
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
        let ids: Vec<&str> = outcome.result["rules"]
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
        assert_eq!(outcome.result["summary"]["empty_range"], json!(true));
        let rows = outcome.result["rules"].as_array().unwrap();
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
                outcome.result["rules"].as_array().unwrap(),
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
        let rows2 = outcome2.result["rules"].as_array().unwrap();
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
        let summary2 = &outcome2.result["summary"];
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
        assert_eq!(outcome.result["rules"][0]["result"], json!("warn"));

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
        assert_eq!(outcome2.result["rules"][0]["result"], json!("pass"));
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
        assert_eq!(outcome.result["rules"][0]["result"], json!("warn"));

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
        assert_eq!(outcome2.result["rules"][0]["result"], json!("pass"));
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
                outcome.result["rules"].as_array().unwrap(),
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
                outcome.result["rules"].as_array().unwrap(),
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
                outcome.result["rules"].as_array().unwrap(),
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
                outcome.result["rules"].as_array().unwrap(),
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
        let rows = outcome.result["rules"].as_array().unwrap();
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
        let again_rows = again.result["rules"].as_array().unwrap();
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
        let rows = outcome.result["rules"].as_array().unwrap();
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
        let row = &outcome.result["rules"][0];
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
        let row = &outcome.result["rules"][0];
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
        let row = &outcome.result["rules"][0];
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
        let row = &outcome.result["rules"][0];
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
        let row = &outcome.result["rules"][0];
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
            outcome.result["rules"]
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
        let row = &outcome.result["rules"][0];
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
        let row = &outcome.result["rules"][0];
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
        assert_eq!(outcome.result["changed_files"], json!(["topic.txt"]));
        let row = &outcome.result["rules"][0];
        assert_eq!(row["id"], json!("process.mainonly"));
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(row["evidence"], json!("0 files checked"));
    }

    /// `--workspace` judges a `report-field` check against every
    /// `task-<n>-report.json` in a workspace directory, instead of the
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
        let row = &outcome.result["rules"][0];
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!(
                "task-2-report.json lacks a value for dependency_vetting (triggered by tools/package.json)"
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
        let row = &outcome.result["rules"][0];
        assert_eq!(row["result"], json!("pass"));
        assert_eq!(
            row["evidence"],
            json!("report field dependency_vetting is set in 2 reports")
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
        let row = &outcome.result["rules"][0];
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
        let row = &outcome.result["rules"][0];
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
        let row = &outcome.result["rules"][0];
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

    /// Carries git's own stderr line for a diff failure that is not a
    /// merge-base miss: calls `git_diff` directly, since `audit()` never
    /// feeds it a malformed pathspec itself, and this is the only way to
    /// exercise its generic fallback.
    #[test]
    fn git_diff_carries_gits_own_stderr_line_for_a_non_merge_base_failure() {
        let dir = make_repo(&audit_entries());
        let error = git_diff(dir.path(), "main", "main", &["--name-only", ":(bad"]).unwrap_err();
        assert_eq!(error, "fatal: Invalid pathspec magic 'bad' in ':(bad'");
    }

    /// The discriminator looks for the literal substring "no merge base",
    /// so a pathspec that echoes back the different words "bad merge
    /// base" must not be mistaken for an actual merge-base miss.
    #[test]
    fn git_diff_does_not_mislabel_a_pathspec_failure_whose_text_contains_merge_base() {
        let dir = make_repo(&audit_entries());
        let error = git_diff(
            dir.path(),
            "main",
            "main",
            &["--name-only", ":(bad merge base"],
        )
        .unwrap_err();
        assert_eq!(
            error,
            "fatal: Invalid pathspec magic 'bad merge base' in ':(bad merge base'"
        );
    }

    /// A `cwd` that does not exist makes `Command::output` itself fail
    /// (git never launches), so `run_git`'s `RawGitError::stderr` carries
    /// the OS error's own text instead of anything git printed. This
    /// pins only that the fallback still produces a real, non-empty
    /// one-line message, since the OS error text is platform-specific.
    #[test]
    fn git_diff_falls_back_to_the_os_errors_own_message_when_git_never_runs() {
        let missing_root =
            std::env::temp_dir().join("houserules-audit-git-diff-missing-root-probe");
        let error =
            git_diff(&missing_root, "main", "main", &["--name-only"]).expect_err("git diff");
        assert!(!error.is_empty());
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
        let rows = outcome.result["rules"].as_array().unwrap();
        let row = rows.iter().find(|r| r["id"] == "process.commits").unwrap();
        assert_eq!(row["result"], json!("fail"));
        assert_eq!(
            row["evidence"],
            json!(
                "commit \"bad subject line\" does not match ^(feat|fix|chore|docs|test): .+ (sanctioned: spec section 6)"
            )
        );
        assert_eq!(outcome.result["summary"]["sanctioned_fail"], json!(1));
        assert!(outcome.result.get("stale_sanctions").is_none());
        assert!(outcome.failed, "a sanctioned fail still fails the run");
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
            outcome.result["stale_sanctions"],
            json!([
                "--sanctioned process.commits=spec section 6: row result is \"pass\", not fail"
            ])
        );
        assert!(outcome.result["summary"].get("sanctioned_fail").is_none());
        assert!(outcome.failed, "a stale booking still fails the run");
    }

    /// A rule id absent from this audit's package (never a row at all,
    /// typically a typo) is this function's own named error -- the same
    /// "unknown id" shape `--ids` already uses one flag over -- never
    /// folded into `stale_sanctions` alongside a rule that genuinely ran
    /// and passed.
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

    #[test]
    fn omits_sanctioned_fail_and_stale_sanctions_when_no_sanction_is_given() {
        let (dir, base_sha) = make_repo_with_one_bad_commit_subject();
        let base = load_base(dir.path()).unwrap();
        let outcome = audit(&base, audit_opts(&base_sha)).unwrap();
        assert!(outcome.result["summary"].get("sanctioned_fail").is_none());
        assert!(outcome.result.get("stale_sanctions").is_none());
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
}
