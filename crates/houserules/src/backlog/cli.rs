//! CLI wrappers for the backlog command surface: resolves `--dir` or the
//! nearest install root the same way `render` and `check-knowledge`
//! do, finds the tracker of the install (`crate::tracker`), and calls its
//! contract -- the same `cmd_*`-returns-`ExitCode` split `rules::render`'s
//! `cmd_render` and `rules::check`'s `cmd_check_knowledge` already use. An
//! install with no tracker gets line A or line B of spec 3.4 from every
//! command here, with the exit code of the table (`cmd_check_backlog` is the
//! gate; the others exit 2).
//!
//! `get` carries no wrapper here: the flat surface's `get` routes an id
//! between the knowledge base and the tracker, so its one dispatcher lives at
//! the crate root (`crate::get`) instead.

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

use super::commands::{emit, set_line};
use crate::tracker::{self, DeferRequest, ListOpts, Lookup, NoTracker, Tracker};

/// Resolves `dir` (via `crate::root::resolve_root`) and finds the tracker of
/// the install there. A resolution failure, an install with no tracker, and
/// a tracker that cannot be built each print one named stderr line and signal
/// exit 2.
fn open(dir: Option<PathBuf>) -> Result<Box<dyn Tracker>, ExitCode> {
    let root = crate::root::resolve_root(dir)?;
    match tracker::lookup(&root) {
        Ok(Lookup::Installed(tracker)) => Ok(tracker),
        Ok(Lookup::Absent(state)) => {
            eprintln!("{}", state.line());
            Err(ExitCode::from(2))
        }
        Err(error) => Err(error.report(false)),
    }
}

/// Runs `list`: prints every matching row.
pub(crate) fn cmd_list(dir: Option<PathBuf>, opts: ListOpts) -> ExitCode {
    let tracker = match open(dir) {
        Ok(tracker) => tracker,
        Err(code) => return code,
    };
    match tracker.list(&opts) {
        Ok(rows) => {
            print!("{}", emit(&Value::Array(rows)));
            ExitCode::SUCCESS
        }
        Err(error) => error.report(false),
    }
}

/// Runs `batch`: prints the one named batch's record, or a "needs one
/// number" usage error when `numbers` does not hold exactly one value
/// (checked here, not by the tracker).
pub(crate) fn cmd_batch(dir: Option<PathBuf>, numbers: Vec<String>) -> ExitCode {
    let tracker = match open(dir) {
        Ok(tracker) => tracker,
        Err(code) => return code,
    };
    if numbers.len() != 1 {
        eprintln!("batch needs one number");
        return ExitCode::from(2);
    }
    match tracker.batch(&numbers[0]) {
        Ok(value) => {
            print!("{}", emit(&value));
            ExitCode::SUCCESS
        }
        Err(error) => error.report(false),
    }
}

/// Runs `set`: `args`' first element is the item id (if any), the rest
/// its `field=value` assignments. Prints the id and the changes made.
pub(crate) fn cmd_set(dir: Option<PathBuf>, args: Vec<String>) -> ExitCode {
    let mut tracker = match open(dir) {
        Ok(tracker) => tracker,
        Err(code) => return code,
    };
    let id = args.first().map(String::as_str).filter(|id| !id.is_empty());
    let assignments = if args.is_empty() { &[] } else { &args[1..] };
    let Some(id) = id.filter(|_| !assignments.is_empty()) else {
        eprintln!("set needs <id> and at least one field=value");
        return ExitCode::from(2);
    };
    match tracker.set(id, assignments) {
        Ok(result) => {
            print!("{}", set_line(&result));
            ExitCode::SUCCESS
        }
        Err(error) => error.report(false),
    }
}

/// Runs `defer`: files the deferral of `request` and prints the id of the
/// item it filed or marked.
pub(crate) fn cmd_defer(dir: Option<PathBuf>, request: DeferRequest) -> ExitCode {
    let mut tracker = match open(dir) {
        Ok(tracker) => tracker,
        Err(code) => return code,
    };
    match tracker.defer(&request) {
        Ok(id) => {
            println!("{id}");
            ExitCode::SUCCESS
        }
        Err(error) => error.report(false),
    }
}

/// Runs `check-backlog`: prints each warning as `warn: <text>`, then
/// either every error on stderr with exit 1, or `backlog: ok` with exit
/// 0. An install with no tracker prints line A and exits 0 when it has no
/// `backlog/` directory, and prints line B and exits 1 when it has one
/// (spec 3.4). A tracker that cannot be reached exits 1.
pub(crate) fn cmd_check_backlog(dir: Option<PathBuf>) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let tracker = match tracker::lookup(&root) {
        Ok(Lookup::Installed(tracker)) => tracker,
        Ok(Lookup::Absent(state @ NoTracker::BacklogDir)) => {
            eprintln!("{}", state.line());
            return ExitCode::from(1);
        }
        Ok(Lookup::Absent(state @ NoTracker::Bare)) => {
            println!("{}", state.line());
            return ExitCode::SUCCESS;
        }
        Err(error) => return error.report(true),
    };
    let (errors, warnings) = match tracker.check() {
        Ok(findings) => findings,
        Err(error) => return error.report(true),
    };
    for warning in &warnings {
        println!("warn: {warning}");
    }
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("{error}");
        }
        return ExitCode::from(1);
    }
    println!("backlog: ok");
    ExitCode::SUCCESS
}
