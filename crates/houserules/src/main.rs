//! Entry point for the `houserules` binary.
//!
//! Parses the command line and dispatches to the crate's modules. `render`
//! and `check-knowledge` (HR-054 tasks 3-4, spec §5 phase 1) were the
//! first ported subcommands; batch 17 T2 (spec §5 phase 2) added the
//! backlog command surface -- `list`, `get`, `batch`, `set`, and
//! `check-backlog`; batch 17 T3 added `audit`, `validate`, and `stats`;
//! batch 17 T4 adds the knowledge read commands (`index`, `for`,
//! `topics`, `standing`) and unifies `get` (`crate::get`) to resolve by id
//! shape between a backlog item and a knowledge entry (spec §3) -- the
//! last command the flat surface needed before the phase-3 reference
//! rewrite. `schema_pin` (test-only) is the mechanism `backlog`'s and
//! `rules`' own model layers use to pin their schema-typed structs against
//! the vendored schema files (spec §3, batch 17 T1); `json_shape`, its
//! `RequiredNullable`/`deserialize_optional_nullable` helper, was deleted
//! at T3 alongside `rules::deliverables`, its one consumer (`rules/mod.rs`'s
//! module doc has the full account). `emit` (fix round 1, issue 8) is the
//! one shared JSON output serializer `rules` and `backlog` both import,
//! kept at the crate root rather than duplicated in each so the two
//! feature modules' output format cannot drift apart (`emit.rs`'s own
//! module doc has the full account); `get` sits beside it for the same
//! reason (`get.rs`'s own module doc). `root` (batch 17 T4 fix round 1)
//! is the third crate-root file, holding `resolve_root`, the one
//! `--dir`-or-git-root fallback every load-first command and `get` itself
//! now share (`root.rs`'s own module doc has the full account).

mod backlog;
mod emit;
mod get;
mod install;
mod root;
mod rules;
#[cfg(test)]
mod schema_pin;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The `houserules` command line.
///
/// `--help` and `--version` come from clap; `--version` reports the crate
/// version (`CARGO_PKG_VERSION`), which does not track the kit's own
/// version until the release-please wiring lands (see the package comment
/// in `Cargo.toml`).
///
/// `arg_required_else_help = true` (HR-056): a bare `houserules`, no
/// subcommand and no flag, prints help on stderr and exits 2, instead of
/// clap's own default for an all-`Option` derive struct (silently
/// succeeding). Pinned rather than merely documented, because it is also
/// the choice that matches the frozen source's own contract: `tools/kb.sh`
/// (or `tools/backlog.sh`) with no command prints its usage line and fails
/// (batch 16 branch review, issue 4) -- clap's own message differs in
/// wording (its derived help text, not `kb.mjs`'s hand-written `usage:`
/// line), the same disclosed, ruled exception spec §7 already grants every
/// other unrecognized-command case in this flat surface.
///
/// `bin_name = "houserules"` (CI fix round 1, issue 2): without it, clap
/// derives the name shown in `Usage:` from `argv[0]` at runtime, which is
/// `houserules.exe` on Windows -- the same fix clap's own `typed-derive`
/// example carries, for the same reason (its own comment: "avoid `.exe`
/// in Usage on Windows"). `name` above only sets the program's own
/// identity (used for `--version`, for instance); it does not reach the
/// usage line clap builds from the binary's real invocation name.
#[derive(Parser)]
#[command(
    name = "houserules",
    version,
    arg_required_else_help = true,
    bin_name = "houserules"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

/// The `houserules` subcommands.
///
/// `render` and `check-knowledge` ported first (spec §5 phase 1); `list`,
/// `get`, `batch`, `set`, and `check-backlog` (batch 17 T2), then `audit`,
/// `validate`, and `stats` (batch 17 T3); `index`, `for`, `topics`, and
/// `standing` (batch 17 T4, spec §5 phase 2) round out the flat surface
/// (§3). `get`'s own dispatch (below) is `crate::get::cmd_get`, not a
/// `backlog`/`rules` function directly -- see that module's doc for why.
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
    /// Prints one or more items by id, each resolved by its own shape: a
    /// backlog item, amendment, or parked item (`HR-\d{3}`, `A-\d{2}`,
    /// `PP-\d+-\d{2}`), or otherwise a knowledge entry.
    Get {
        /// Backlog and/or knowledge ids to print, in any mix.
        ids: Vec<String>,
        /// Repository root to read from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists backlog items, optionally filtered.
    List {
        /// Only items whose status is `open` or `partial`.
        #[arg(long)]
        open: bool,
        /// Only items with this exact status.
        #[arg(long)]
        status: Option<String>,
        /// Only items with this exact milestone (`-` matches a missing one).
        #[arg(long)]
        milestone: Option<String>,
        /// Only items filed under this section.
        #[arg(long)]
        section: Option<String>,
        /// Only items of this exact type.
        #[arg(long = "type")]
        item_type: Option<String>,
        /// Only items assigned to this exact batch number.
        #[arg(long)]
        batch: Option<String>,
        /// Repository root to read from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Prints one development batch's summary and item rows.
    Batch {
        /// The batch number (exactly one).
        numbers: Vec<String>,
        /// Repository root to read from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Applies `field=value` assignments to a backlog item and rewrites its file.
    Set {
        /// The item id, followed by one or more `field=value` assignments.
        args: Vec<String>,
        /// Repository root to write into; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validates the backlog: schema, cross-file invariants.
    CheckBacklog {
        /// Repository root to check; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Builds the rule package for a git range and runs every member's
    /// deterministic check.
    Audit {
        /// The range's base ref.
        #[arg(long)]
        base: Option<String>,
        /// The range's head ref; defaults to `HEAD`.
        #[arg(long)]
        head: Option<String>,
        /// Extra knowledge ids to include in the package, comma-separated.
        #[arg(long)]
        ids: Option<String>,
        /// A single JSON deliverable a `report-field` check reads directly.
        #[arg(long)]
        report: Option<PathBuf>,
        /// A directory of `task-<n>-report.json` files a `report-field`
        /// check judges by each report's `files_changed`.
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Also writes the JSON result to this file.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Repository root to audit; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validates one or more deliverable JSON files against
    /// `.claude/schemas/deliverables.json`.
    Validate {
        /// Deliverable files to validate.
        files: Vec<PathBuf>,
        /// Repository root the deliverables schema resolves from; defaults
        /// to the enclosing git repository's top level, resolved from the
        /// current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Aggregates rule violations and unused injected ids across a
    /// workspace directory's JSON deliverables.
    Stats {
        /// The workspace directory to aggregate.
        workspace: PathBuf,
        /// Repository root to check the knowledge base under; defaults to
        /// the enclosing git repository's top level, resolved from the
        /// current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists knowledge-entry index rows, optionally filtered.
    Index {
        /// Only entries in this exact area.
        #[arg(long)]
        area: Option<String>,
        /// Only entries in this exact topic.
        #[arg(long)]
        topic: Option<String>,
        /// Only entries carrying this exact tag.
        #[arg(long)]
        tag: Option<String>,
        /// Only entries of this exact kind.
        #[arg(long)]
        kind: Option<String>,
        /// Only standing entries.
        #[arg(long)]
        standing: bool,
        /// Repository root to read from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Prints the rule package one or more changed paths pull in: their
    /// areas' rule-shaped entries, plus every entry whose `verify` names
    /// one of the paths.
    For {
        /// Changed paths to resolve.
        paths: Vec<String>,
        /// Print each matching entry whole, instead of its index row.
        #[arg(long)]
        full: bool,
        /// Repository root to read from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists every loaded knowledge topic's name, entry count, and title.
    Topics {
        /// Repository root to read from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists the standing rules, rules before invariants.
    Standing {
        /// Repository root to read from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        // Unreachable through the CLI itself once `arg_required_else_help`
        // is set (HR-056): clap exits the process before `main` ever sees a
        // bare invocation. Kept for match exhaustiveness over `Option<Command>`.
        None => ExitCode::SUCCESS,
        Some(Command::Render { check, dir }) => rules::cmd_render(dir, check),
        Some(Command::CheckKnowledge { dir }) => rules::cmd_check_knowledge(dir),
        Some(Command::Get { ids, dir }) => get::cmd_get(dir, ids),
        Some(Command::List {
            open,
            status,
            milestone,
            section,
            item_type,
            batch,
            dir,
        }) => backlog::cmd_list(
            dir,
            backlog::ListOpts {
                open,
                status,
                milestone,
                section,
                item_type,
                batch,
            },
        ),
        Some(Command::Batch { numbers, dir }) => backlog::cmd_batch(dir, numbers),
        Some(Command::Set { args, dir }) => backlog::cmd_set(dir, args),
        Some(Command::CheckBacklog { dir }) => backlog::cmd_check_backlog(dir),
        Some(Command::Audit {
            base,
            head,
            ids,
            report,
            workspace,
            json,
            dir,
        }) => rules::cmd_audit(dir, base, head, ids, report, workspace, json),
        Some(Command::Validate { files, dir }) => rules::cmd_validate(dir, files),
        Some(Command::Stats { workspace, dir }) => rules::cmd_stats(dir, workspace),
        Some(Command::Index {
            area,
            topic,
            tag,
            kind,
            standing,
            dir,
        }) => rules::cmd_index(
            dir,
            rules::IndexOpts {
                area,
                topic,
                tag,
                kind,
                standing,
            },
        ),
        Some(Command::For { paths, full, dir }) => rules::cmd_for(dir, paths, full),
        Some(Command::Topics { dir }) => rules::cmd_topics(dir),
        Some(Command::Standing { dir }) => rules::cmd_standing(dir),
    }
}
