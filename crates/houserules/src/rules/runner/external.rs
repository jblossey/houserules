//! The `runner` check: an external tool that the project defines in
//! `.houserules.json` `runners` and that a check names by its `tool` key.
//!
//! A changed file that matches the `files` of the check (every file when the
//! check names none) triggers it. An untriggered check passes and starts no
//! process. A triggered check runs in this order: it reads the tool from the
//! marker, confirms that the working tree is the tree at `--head`, then
//! starts the tool with the install root as its working directory and the
//! environment of `audit`. With `scope: "tree"` it makes one call and appends
//! nothing. Otherwise it appends the triggering files, one argument each, in
//! calls of at most [`FILES_PER_CALL`]. No shell starts and no timeout applies.
//!
//! Every fault that stops the audit before it prints a row is a
//! [`SetupError`]: `run` hands its line to `audit`, which prints it on stderr
//! and exits 2.

use std::collections::HashSet;
use std::io;
use std::path::{self, Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

use crate::install::marker::Marker;
use crate::install::marker::runners::{Parser, RunnerDef};
use crate::rules::check_shape::{CheckDef, Glob, Scope};
use crate::rules::git::{rev, worktree_changes};

use super::sarif::{self, Fault, Finding};
use super::{AuditContext, RowBuilder, filter_matching};

/// The most triggering files that one call of a `changed` tool takes.
const FILES_PER_CALL: usize = 200;

/// The `files` of a check that names none.
const ALL_FILES: &str = "**";

/// A fault that stops the audit before it prints a row: the one stderr line
/// that names it.
#[derive(Debug)]
pub(super) struct SetupError(String);

impl From<String> for SetupError {
    fn from(line: String) -> Self {
        Self(line)
    }
}

/// Judges a `runner` check: the row names the findings of the tool or its
/// exit status. The `Err` is a [`SetupError`] line.
pub(super) fn run(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<Value, String> {
    judge(check, ctx, rows).map_err(|SetupError(line)| line)
}

/// [`run`] with its fault typed.
fn judge(check: &CheckDef, ctx: &AuditContext, rows: &RowBuilder) -> Result<Value, SetupError> {
    let files_glob = check
        .files
        .clone()
        .or_else(|| Some(Glob::One(ALL_FILES.to_string())));
    let files = filter_matching(ctx.changed, &files_glob)?;
    if files.is_empty() {
        return Ok(rows.pass("not triggered".to_string()));
    }
    let Some(name) = check.tool.as_deref() else {
        return Err(SetupError(format!(
            "{}: check \"runner\" needs \"tool\"",
            rows.entry.id
        )));
    };
    let tool = Tool::find(name, ctx.root)?;
    let whole_tree = check.scope == Some(Scope::Tree);
    ensure_head_is_checked_out(&tool, ctx, whole_tree, &files)?;
    let no_files: &[String] = &[];
    let batches: Vec<&[String]> = if whole_tree {
        vec![no_files]
    } else {
        files.chunks(FILES_PER_CALL).collect()
    };
    match tool.parser {
        Parser::Sarif => judge_sarif(&tool, &batches, rows),
        Parser::ExitCode => judge_exit_code(&tool, &batches, rows),
    }
}

/// A tool of `runners`, ready to start.
struct Tool<'a> {
    /// The key of the tool in `runners`.
    name: &'a str,
    /// The first entry of the command.
    program: String,
    /// The other entries of the command.
    arguments: Vec<String>,
    /// The line that tells a person how to install the tool.
    install: String,
    /// How to read the output of the tool.
    parser: Parser,
    /// The install root: the working directory of the tool.
    root: &'a Path,
}

impl<'a> Tool<'a> {
    /// The tool `name` of the marker at `root`. A marker that cannot be read,
    /// a `runners` value or entry with a fault, and a tool that the marker
    /// does not define are setup errors.
    fn find(name: &'a str, root: &'a Path) -> Result<Self, SetupError> {
        let marker = Marker::read_or_default_for_gate(root, "WI")?;
        let RunnerDef {
            command,
            parser,
            install,
        } = marker.runner(name)?.ok_or_else(|| {
            SetupError(format!(
                "runner tool \"{name}\" is not defined in .houserules.json runners"
            ))
        })?;
        let mut command = command.into_iter();
        let Some(program) = command.next() else {
            return Err(SetupError(format!(
                "{}: runners.{name}.command must be a non-empty array of strings",
                marker.file_name()
            )));
        };
        Ok(Self {
            name,
            program,
            arguments: command.collect(),
            install,
            parser,
            root,
        })
    }

    /// Starts the tool with `files` after its own arguments, and waits for it.
    fn call(&self, files: &[String]) -> Result<Output, SetupError> {
        let program = program_path(self.root, &self.program).map_err(|error| self.fault(&error))?;
        Command::new(program)
            .args(&self.arguments)
            .args(files)
            .current_dir(self.root)
            .output()
            .map_err(|error| self.fault(&error))
    }

    /// The setup error for a tool that did not start: the install line when
    /// the system does not find the program, else the error of the system.
    fn fault(&self, error: &io::Error) -> SetupError {
        let (name, program) = (self.name, &self.program);
        if error.kind() == io::ErrorKind::NotFound {
            SetupError(format!(
                "runner tool \"{name}\" ({program}) is not installed: {}",
                self.install
            ))
        } else {
            SetupError(format!(
                "runner tool \"{name}\" ({program}) could not start: {error}"
            ))
        }
    }
}

/// The program to start. A name with a path separator is a path below the
/// install root: `Command` leaves a relative program path to the platform, so
/// the path is made absolute here. Any other name is left to the search of the
/// system.
fn program_path(root: &Path, program: &str) -> io::Result<PathBuf> {
    if program.chars().any(path::is_separator) {
        path::absolute(root.join(program))
    } else {
        Ok(PathBuf::from(program))
    }
}

/// Confirms that the working tree is the tree at `--head`: the checked-out
/// commit is `--head`, and no file that the tool reads differs from its
/// content there. A `tree` tool reads every tracked file, a `changed` tool the
/// triggering `files`. Untracked files are not tested.
fn ensure_head_is_checked_out(
    tool: &Tool,
    ctx: &AuditContext,
    whole_tree: bool,
    files: &[String],
) -> Result<(), SetupError> {
    let checked_out = rev(ctx.root, "HEAD")?;
    let reason = if checked_out == ctx.head_sha {
        let differing = worktree_changes(ctx.root, &ctx.head_sha)?;
        let first = if whole_tree {
            differing.first()
        } else {
            let known: HashSet<&str> = differing.iter().map(String::as_str).collect();
            files.iter().find(|file| known.contains(file.as_str()))
        };
        first.map(|path| format!("{path} differs from --head"))
    } else {
        Some(format!("HEAD is {checked_out}, --head is {}", ctx.head_sha))
    };
    match reason {
        None => Ok(()),
        Some(reason) => Err(SetupError(format!(
            "runner tool \"{}\" needs the tree at --head checked out: {reason}",
            tool.name
        ))),
    }
}

/// Judges a `sarif` tool: every call prints a SARIF document, and the results
/// that count are the findings of the row, in call order.
fn judge_sarif(tool: &Tool, batches: &[&[String]], rows: &RowBuilder) -> Result<Value, SetupError> {
    let mut findings: Vec<Finding> = Vec::new();
    for files in batches {
        let output = tool.call(files)?;
        let log = sarif::parse(&output.stdout)
            .map_err(|fault| SetupError(fault_line(tool.name, &fault)))?;
        findings.extend(sarif::findings(&log, tool.root));
    }
    let row = match findings.split_first() {
        None => rows.pass(format!("no findings in {}", call_count(batches.len()))),
        Some((first, rest)) => rows.violate(evidence(first, rest.len())),
    };
    Ok(rows.with(row, "findings", json!(findings)))
}

/// Judges an `exit-code` tool: the check holds when every call exits 0.
fn judge_exit_code(
    tool: &Tool,
    batches: &[&[String]],
    rows: &RowBuilder,
) -> Result<Value, SetupError> {
    for files in batches {
        let output = tool.call(files)?;
        if !output.status.success() {
            return Ok(rows.violate(failure_evidence(&output)));
        }
    }
    Ok(rows.pass(format!("exited 0 in {}", call_count(batches.len()))))
}

/// `1 call` for one call, else `<count> calls`.
fn call_count(count: usize) -> String {
    if count == 1 {
        "1 call".to_string()
    } else {
        format!("{count} calls")
    }
}

/// The line of the setup error for a SARIF document that `fault` refuses.
fn fault_line(tool: &str, fault: &Fault) -> String {
    match fault {
        Fault::NoOutput => format!("runner tool \"{tool}\" printed no SARIF on stdout"),
        Fault::Invalid(first_line) => {
            format!("runner tool \"{tool}\" printed invalid SARIF: {first_line}")
        }
        Fault::ExecutionFailed => {
            format!("runner tool \"{tool}\" reported executionSuccessful: false")
        }
    }
}

/// The evidence of a violated `sarif` row: the first finding, and the count
/// of the others.
fn evidence(first: &Finding, others: usize) -> String {
    let described = describe(first);
    if others == 0 {
        described
    } else {
        format!("{described} and {others} more")
    }
}

/// One finding as a line: `<path>:<line> <rule>: <message>`. A finding with
/// no line drops `:<line>`, one with no location drops the location.
fn describe(finding: &Finding) -> String {
    let Finding {
        path,
        line,
        rule,
        message,
    } = finding;
    match (path, line) {
        (Some(path), Some(line)) => format!("{path}:{line} {rule}: {message}"),
        (Some(path), None) => format!("{path} {rule}: {message}"),
        (None, _) => format!("{rule}: {message}"),
    }
}

/// The evidence of a failed `exit-code` call: the first line of its stderr
/// that holds text, else `exited <n>`, else a designed token for a tool that
/// a signal ended and so has no status.
fn failure_evidence(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(line) = stderr.lines().map(str::trim).find(|line| !line.is_empty()) {
        return line.to_string();
    }
    match output.status.code() {
        Some(code) => format!("exited {code}"),
        None => "ended by a signal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A finding at `path` and `line` for the rule `r`.
    fn finding(path: Option<&str>, line: Option<u64>) -> Finding {
        Finding {
            path: path.map(str::to_string),
            line,
            rule: "r".to_string(),
            message: "m".to_string(),
        }
    }

    /// A finding names its location as far as the result has one.
    #[test]
    fn describe_drops_the_parts_a_result_lacks() {
        assert_eq!(describe(&finding(Some("a.rs"), Some(3))), "a.rs:3 r: m");
        assert_eq!(describe(&finding(Some("a.rs"), None)), "a.rs r: m");
        assert_eq!(describe(&finding(None, None)), "r: m");
    }

    /// The count of the other findings follows the first, and a single finding
    /// has no count.
    #[test]
    fn evidence_counts_the_other_findings() {
        let first = finding(Some("a.rs"), Some(3));
        assert_eq!(evidence(&first, 0), "a.rs:3 r: m");
        assert_eq!(evidence(&first, 2), "a.rs:3 r: m and 2 more");
    }

    /// The pass evidence counts the calls, and one call is `1 call`.
    #[test]
    fn call_count_is_singular_for_one_call() {
        assert_eq!(call_count(1), "1 call");
        assert_eq!(call_count(2), "2 calls");
        assert_eq!(call_count(0), "0 calls");
    }

    /// A program with a separator is a path below the root; a bare name is
    /// left to the search of the system.
    #[test]
    fn program_path_resolves_only_a_name_with_a_separator() {
        let root = std::env::temp_dir();
        assert_eq!(
            program_path(&root, "tools/my lint/run.sh").expect("a path"),
            path::absolute(root.join("tools/my lint/run.sh")).expect("absolute")
        );
        assert_eq!(
            program_path(&root, "semgrep").expect("a name"),
            PathBuf::from("semgrep")
        );
    }
}
