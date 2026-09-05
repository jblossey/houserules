//! CLI wrappers for the backlog command surface (batch 17 T2): resolves
//! `--dir` or the enclosing git repository the same way `render` and
//! `check-knowledge` do, loads the backlog, dispatches to `commands`, and
//! prints `tools/backlog.mjs`'s `main` output and exit codes exactly --
//! the same `cmd_*`-returns-`ExitCode` split `rules::render`'s
//! `cmd_render` and `rules::check`'s `cmd_check_knowledge` already use.
//!
//! `get` carries no wrapper here (batch 17 T4 removed the one T2 wrote):
//! the flat surface's `get` resolves an id by shape between a backlog item
//! and a knowledge entry (spec §3), so its one dispatcher lives at the
//! crate root (`crate::get`) instead, calling `load_backlog` and
//! `commands::get_items` directly -- `mod.rs` re-exports
//! `LoadedBacklog`/`load_backlog`/`get_items` for exactly that caller.

use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::Value;

use super::commands::{self, CommandError, ListOpts, emit};
use super::load::{LoadedBacklog, load_backlog};

/// Resolves `dir` (via `crate::root::resolve_root`) and loads the backlog
/// there. A resolution or load failure prints one named stderr line and
/// signals exit 2 -- `main`'s own `loadBacklog(repoRoot(cwd))` call, whose
/// failure (missing file, invalid JSON) is an uncaught `Error` in JS, per
/// the CLI-failure-path deviation this whole crate follows (spec §6).
fn load(dir: Option<PathBuf>) -> Result<LoadedBacklog, ExitCode> {
    let root = crate::root::resolve_root(dir)?;
    load_backlog(&root).map_err(|error| {
        eprintln!("{error}");
        ExitCode::from(2)
    })
}

/// Prints `error`'s message and yields exit 2 -- `main`'s `UsageError`
/// catch arm (`io.err(\`${error.message}\n\`); return 2;`).
fn command_error(error: CommandError) -> ExitCode {
    eprintln!("{}", error.0);
    ExitCode::from(2)
}

/// Runs `list`: prints every matching row.
pub(crate) fn cmd_list(dir: Option<PathBuf>, opts: ListOpts) -> ExitCode {
    let b = match load(dir) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let rows = commands::list_items(&b, &opts);
    print!("{}", emit(&Value::Array(rows)));
    ExitCode::SUCCESS
}

/// Runs `batch`: prints the one named batch's record, or `main`'s own
/// "needs one number" usage error when `numbers` does not hold exactly
/// one value (checked here, not in `commands::batch_record`, matching the
/// frozen JS's `positional.length !== 1` guard in `main` itself).
pub(crate) fn cmd_batch(dir: Option<PathBuf>, numbers: Vec<String>) -> ExitCode {
    let b = match load(dir) {
        Ok(b) => b,
        Err(code) => return code,
    };
    if numbers.len() != 1 {
        eprintln!("batch needs one number");
        return ExitCode::from(2);
    }
    match commands::batch_record(&b, &numbers[0]) {
        Ok(value) => {
            print!("{}", emit(&value));
            ExitCode::SUCCESS
        }
        Err(error) => command_error(error),
    }
}

/// Runs `set`: `args`' first element is the item id (if any), the rest
/// its `field=value` assignments -- `main`'s own `positional[0]` /
/// `positional.slice(1)` split.
pub(crate) fn cmd_set(dir: Option<PathBuf>, args: Vec<String>) -> ExitCode {
    let b = match load(dir) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let id = args.first().map(String::as_str);
    let assignments = if args.is_empty() { &[] } else { &args[1..] };
    match commands::set_item(&b, id, assignments) {
        Ok(message) => {
            print!("{message}");
            ExitCode::SUCCESS
        }
        Err(error) => command_error(error),
    }
}

/// Runs `check-backlog`: prints each warning as `warn: <text>`, then
/// either every error on stderr with exit 1, or `backlog: ok` with exit 0
/// -- `main`'s `'check'` case, ported.
pub(crate) fn cmd_check_backlog(dir: Option<PathBuf>) -> ExitCode {
    let b = match load(dir) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let (errors, warnings) = commands::check_backlog(&b);
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
