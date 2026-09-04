//! Entry point for the `houserules` binary.
//!
//! Parses the command line and dispatches to the crate's modules. `render`
//! (spec §5 phase 1, HR-054 task 3) and `check-knowledge` (HR-054 task 4)
//! are the first ported subcommands; the rest of the flat surface
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3) lands in later tasks.

mod backlog;
mod install;
mod rules;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The `houserules` command line.
///
/// `--help` and `--version` come from clap; `--version` reports the crate
/// version (`CARGO_PKG_VERSION`), which does not track the kit's own
/// version until the release-please wiring lands (see the package comment
/// in `Cargo.toml`).
#[derive(Parser)]
#[command(name = "houserules", version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// The `houserules` subcommands.
///
/// `render` and `check-knowledge` port first (spec §5 phase 1); the rest of
/// the flat surface (§3) lands in later tasks.
#[derive(Subcommand)]
enum Command {
    /// Writes every stale generated knowledge file, or lists them with `--check`.
    Render {
        /// Report stale files on stderr and exit 1, instead of writing them.
        #[arg(long)]
        check: bool,
        /// Repository root to render from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validates the knowledge base: schema, cross-entry invariants, and
    /// every generated file's freshness and budget.
    CheckKnowledge {
        /// Repository root to check; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        None => ExitCode::SUCCESS,
        Some(Command::Render { check, dir }) => rules::cmd_render(dir, check),
        Some(Command::CheckKnowledge { dir }) => rules::cmd_check_knowledge(dir),
    }
}
