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
//! now share (`root.rs`'s own module doc has the full account). Batch 18 T2
//! (HR-062, spec §6) adds `check-commit`, a genuinely new command with no
//! frozen-JS predecessor: it runs the knowledge base's `commits`-type
//! checks against a not-yet-committed message or a git range, reusing
//! `audit`'s own per-commit evaluation rather than reimplementing it
//! (`rules::check_commit`'s own module doc has the full account). Batch 18
//! T3 (HR-047, spec §§1-2) adds `init` and `files` (`install::cmd_init`,
//! `install::cmd_files`): the kit payload now embeds inside the binary
//! (`rust-embed`), so `init` seeds a target repository and `files` prints
//! the ownership split with no `template/` checkout needed at runtime
//! (`install.rs`'s own module doc has the full account). `node_path`
//! (batch 18 T3) is the fourth crate-root file, holding
//! `resolve_like_node`, the Node-`path.resolve` parity `validate` and
//! `install` both need (`node_path.rs`'s own module doc has the full
//! account). Batch 18 T4 (same spec, §1) adds `update`
//! (`install::cmd_update`): the `KIT_OWNED` sync, the version-drift line,
//! and the new deletion capability for kit files the payload has retired
//! (`install.rs`'s own "update"/"Deletion" doc sections have the full
//! account). Batch 20 T2 (HR-066, HR-071, docs/specs/2026-09-07-batch-20-
//! phase5.md §§3-4) adds `check-report-claims`
//! (`report_claims::cmd_check_report_claims`): the deliverable-claims
//! checker, moved here from a dev-only `src/bin/` target so the seeded
//! implementer template's closing act runs in every adopter repo, not
//! only this one (`report_claims.rs`'s own module doc has the full
//! account, including the paste-run lint HR-071 adds in the same move).
//! `archive` (`archive::cmd_archive`) moves retired backlog items, batch
//! entries, and knowledge entries into their `archive/` mirrors;
//! `archive.rs`'s own module doc has the full account, including the
//! fallback lookups `get` (above) falls back to once an id it resolves
//! moves out of the active set.

mod archive;
mod backlog;
mod emit;
mod get;
mod install;
mod node_path;
mod report_claims;
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
/// version (`CARGO_PKG_VERSION`), which IS the kit's own version --
/// `Cargo.toml`'s own comment has the full account, including HR-073, the
/// known gap batch 20 T3 (HR-047) left behind when `package.json` (the
/// earlier lockstep partner release-please's `extra-files` config kept in
/// sync) retired from this repository.
///
/// `arg_required_else_help = true` (HR-056): a bare `houserules`, no
/// subcommand and no flag, prints help on stderr and exits 2, instead of
/// clap's own default for an all-`Option` derive struct (silently
/// succeeding). Pinned rather than merely documented, because it is also
/// the choice that matches the frozen JS's own contract: its two shell
/// wrappers, run with no command, print a usage line and fail (batch 16
/// branch review, issue 4) -- clap's own message differs in wording (its
/// derived help text, not the frozen JS's hand-written `usage:` line), the
/// same disclosed, ruled exception spec §7 already grants every other
/// unrecognized-command case in this flat surface. This struct's own doc
/// comment is clap's `--help` "about" text verbatim (batch 18 T5 fix round
/// 1, spec §1: `--help` printing a retired shell wrapper's name is itself
/// a remaining shipped reference) -- every paragraph here is
/// adopter-visible output, not only internal history, so this doc names
/// the two retired shell wrappers only by role, never by their literal,
/// no-longer-shipped filenames.
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
/// `standing` (batch 17 T4, spec §5 phase 2) round out the ported flat
/// surface (§3). `check-commit` (batch 18 T2, HR-062, spec §6) is the first
/// command with no frozen-JS predecessor at all; `init` and `files`
/// (batch 18 T3) and `update` (batch 18 T4) round out spec §1's install
/// surface. `check-report-claims` (batch 20 T2, HR-066/HR-071) is the
/// second: moved from a dev-only `src/bin/` target, not ported from any
/// frozen JS. `get`'s own dispatch (below) is `crate::get::cmd_get`, not a
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
        /// Declares a spec-booked interim fail, `<rule>=<ref>`; repeatable.
        /// The rule's row keeps its true `fail` result and gains
        /// `(sanctioned: <ref>)` in its evidence; the summary's
        /// `sanctioned_fail` counts it. A rule named here that did not fail
        /// is reported in `stale_sanctions`, not silently accepted.
        #[arg(long)]
        sanctioned: Vec<String>,
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
    /// Runs every `commits`-type knowledge check against a not-yet-committed
    /// message or a git range.
    CheckCommit {
        /// A not-yet-committed message file (the commit-msg hook's own call
        /// shape: git passes the proposed message's path as `$1`). Exactly
        /// one of this or `--from` is required.
        message_file: Option<PathBuf>,
        /// The range's exclusive base ref (CI's own call shape: commitlint's
        /// `--from`). Checks every commit strictly after this ref.
        #[arg(long)]
        from: Option<String>,
        /// The range's inclusive head ref; defaults to `HEAD`. Only valid
        /// alongside `--from`.
        #[arg(long)]
        to: Option<String>,
        /// Repository root to check from; defaults to the enclosing git
        /// repository's top level, resolved from the current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Seeds the kit into a target git repository from the embedded payload.
    Init {
        /// The target directory; defaults to the current directory. Unlike
        /// every read command's `--dir` above, this is not resolved against
        /// an enclosing git repository -- the target itself must already be
        /// one (`install::cmd_init`'s own doc has the full account).
        #[arg(long)]
        dir: Option<PathBuf>,
        /// The backlog id prefix seeded schemas carry; defaults to `WI`.
        #[arg(long)]
        id_prefix: Option<String>,
    },
    /// Prints the kit-owned and seed-once file lists the embedded payload defines.
    Files,
    /// Syncs an already-`init`ed target's `KIT_OWNED` files from the
    /// embedded payload, deletes any retired kit file still present, and
    /// reports the stamped-to-running version drift.
    Update {
        /// The target directory; defaults to the current directory. Resolved
        /// the same way `init`'s own `--dir` is (`install::cmd_update`'s own
        /// doc has the full account).
        #[arg(long)]
        dir: Option<PathBuf>,
        /// The backlog id prefix a still-unstamped install's marker
        /// defaults to; defaults to `WI`.
        #[arg(long)]
        id_prefix: Option<String>,
    },
    /// Cross-checks one deliverable report's claims against the artifacts
    /// and git history it cites: redirected captures, truncation markers,
    /// listed commit shas, self-audit narrative, and the bounded
    /// no-execution paste-run lint over every captured command field.
    CheckReportClaims {
        /// The report file to check.
        report_path: PathBuf,
        /// Repository root the report's cited artifacts and commit shas
        /// resolve against; defaults to the enclosing git repository's
        /// top level, resolved from the current directory. Independent of
        /// `report_path`, which always resolves against the real current
        /// directory (`report_claims.rs`'s own module doc has the full
        /// account).
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Moves done/dropped backlog items, done batch entries, and
    /// superseded/retired knowledge entries into their `archive/` mirrors.
    Archive {
        /// Repository root to sweep; defaults to the enclosing git
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
            sanctioned,
            dir,
        }) => rules::cmd_audit(dir, base, head, ids, report, workspace, json, sanctioned),
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
        Some(Command::CheckCommit {
            message_file,
            from,
            to,
            dir,
        }) => rules::cmd_check_commit(dir, message_file, from, to),
        Some(Command::Init { dir, id_prefix }) => install::cmd_init(dir, id_prefix),
        Some(Command::Files) => install::cmd_files(),
        Some(Command::Update { dir, id_prefix }) => install::cmd_update(dir, id_prefix),
        Some(Command::CheckReportClaims { report_path, dir }) => {
            report_claims::cmd_check_report_claims(dir, report_path)
        }
        Some(Command::Archive { dir }) => archive::cmd_archive(dir),
    }
}
