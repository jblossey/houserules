//! Entry point for the `houserules` binary.
//!
//! Parses the command line and dispatches to the crate's modules.
//! `backlog` and `rules` hold the backlog and knowledge command surfaces;
//! `tracker` (`tracker::Tracker`) is the contract that every backlog command
//! calls, with the JSON adapter behind it (`tracker`'s own module doc has the
//! full account); `get` (`crate::get::cmd_get`) routes each id between a
//! knowledge entry and the tracker, rather than either module handling it
//! directly (see that module's doc for why).
//! `schema_pin` (test-only) is the mechanism `backlog`'s and `rules`' own
//! model layers use to pin their schema-typed structs against the
//! vendored schema files. `emit` is the one shared JSON output serializer
//! `rules` and `backlog` both import, kept at the crate root rather than
//! duplicated in each so the two feature modules' output format cannot
//! drift apart (`emit.rs`'s own module doc has the full account); `get`
//! sits beside it for the same reason. `root` holds `resolve_root`, the
//! one `--dir`-or-install-root fallback every load-first command and `get`
//! itself share (`root.rs`'s own module doc has the full account).
//! `check-commit` (`rules::check_commit`) runs the knowledge base's
//! `commits`-type checks against a not-yet-committed message or a git
//! range, reusing `audit`'s own per-commit evaluation rather than
//! reimplementing it. `install` (`install::cmd_init`, `cmd_files`,
//! `cmd_update`) holds the kit payload embedded inside the binary
//! (`rust-embed`): `init` seeds a target repository, `files` prints the
//! ownership split, and `update` syncs an existing install against its
//! recorded baseline, all with no `template/` checkout needed at runtime
//! (`install/mod.rs`'s own module doc has the full account). `node_path`
//! holds `resolve_like_node`, the Node-`path.resolve` parity `validate`
//! and `install` both need (`node_path.rs`'s own module doc has the full
//! account). `check-report-claims`
//! (`report_claims::cmd_check_report_claims`) is the deliverable-claims
//! checker (`report_claims.rs`'s own module doc indexes the account of the
//! checks, which sits in the docs of its submodules).
//! `capture` (`capture::cmd_capture`) runs a command and writes its output
//! under a header, and `capture::read` reads such a file back (`capture.rs`'s
//! own module doc has the full account). `shell` quotes a value, or a whole
//! command line, for a POSIX shell. `report-facts`
//! (`report_facts::cmd_report_facts`) fills the fact fields of a task report
//! from git and from captures, and `report_facts::judge_citation` is the one
//! judgement of a recorded citation (`report_facts.rs`'s own module doc has the
//! full account).
//! `archive` (`archive::cmd_archive`) moves retired backlog items, batch
//! entries, and knowledge entries into their `archive/` mirrors;
//! `archive.rs`'s own module doc has the full account, including the
//! fallback lookups `get` (above) falls back to once an id it resolves
//! moves out of the active set. `selfupdate`
//! (`selfupdate::run_before_repo_phase`) is the one call `cmd_update`
//! makes before its own repo sync: keeps the installed binary itself
//! current on the shell/PowerShell installer channel, or degrades to a
//! guidance line or a warning everywhere else (`selfupdate.rs`'s own
//! module doc has the full account). `update_check`
//! (`update_check::maybe_print_header`) is the one call `main` makes
//! before dispatching to any command but `update` itself: prints the
//! update-available header on stderr when a cached, receipt-free check
//! says a newer release exists, or stays silent on every gate and every
//! failure (`update_check.rs`'s own module doc has the full account).
//! `workflow` holds the workflow packages that the binary embeds and the
//! pins that an install keeps for the files of its workflow
//! (`workflow/mod.rs`'s own module doc has the full account).
//! `pack` (`pack::cmd_pack_list`, `cmd_pack_add`, `cmd_pack_remove`,
//! `cmd_pack_upgrade`) lists, adds, removes, and upgrades the knowledge
//! packs of an install (`pack/mod.rs`'s own module doc has the full
//! account). D4 (`install::refuse_install_schema`) refuses a tree before a
//! command runs and exits 2 with one named line. `main` runs it before every
//! command but `files` and `update` (`d4_guard` lists the commands); `update`
//! runs it after its self-update phase and before its repo phase (spec C22),
//! so that it can install the release that the line names. A command that
//! reads the install refuses a tree whose install schema is missing or older
//! than major 2 with a line that names `houserules update`. `update` and
//! `init` run on such a tree. Every command but `files`, `update` and `init`
//! included, refuses a tree whose install schema is newer than major 2 with a
//! line that names the release it needs, and a tree whose install schema is no
//! install schema id with a line that names the value (spec C23).

#![warn(missing_docs)]

mod archive;
mod backlog;
mod baseline;
mod capture;
mod emit;
mod get;
mod install;
mod node_path;
mod pack;
mod packs;
mod report_claims;
mod report_facts;
mod root;
mod rules;
#[cfg(test)]
mod schema_pin;
mod selfupdate;
mod shell;
mod tracker;
mod update_check;
mod workflow;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// The `houserules` command line.
//
// This doc comment IS clap's `--help`/`-h` "about" text (`derive(Parser)`
// reads it verbatim), so it stays to exactly this one adopter-facing
// line; every constraint below is a plain comment for exactly that
// reason -- `clap_derive` reads only `///`/`#[doc]`, never a bare `//`,
// so none of these reach an adopter's terminal.
//
// `--help` and `--version` come from clap; `--version` reports the crate
// version (`CARGO_PKG_VERSION`), which IS the kit's own version
// (`Cargo.toml`'s own comment has the full account).
//
// `arg_required_else_help = true`: a bare `houserules`, no subcommand and
// no flag, prints help on stderr and exits 2, instead of clap's own
// default for an all-`Option` derive struct (silently succeeding).
//
// `bin_name = "houserules"`: without it, clap derives the name shown in
// `Usage:` from `argv[0]` at runtime, which is `houserules.exe` on
// Windows -- the same fix clap's own `typed-derive` example carries, for
// the same reason (its own comment: "avoid `.exe` in Usage on Windows").
// `name` above only sets the program's own identity (used for
// `--version`, for instance); it does not reach the usage line clap
// builds from the binary's real invocation name.
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
/// `get`'s own dispatch (below) is `crate::get::cmd_get`, not a
/// `backlog`/`rules` function directly -- see that module's doc for why.
#[derive(Subcommand)]
enum Command {
    /// Writes every stale generated knowledge file, or lists them with
    /// `--check`. The AGENTS.md renderer is always on: it owns the block
    /// between the `<!-- houserules:begin -->` and `<!-- houserules:end -->`
    /// lines of the root `AGENTS.md` and of the `AGENTS.md` of each directory
    /// that the globs of an area share, and it removes a block it no longer
    /// writes. The Claude renderer, which `renderers` in `.houserules.json`
    /// enables, writes `.claude/rules/<area>.md` and the `project-knowledge`
    /// skill, and deletes a generated file in `.claude/rules/` that it no
    /// longer writes. A marker with no pair, or two blocks in one file, is
    /// one line that names the file, and exit 2.
    Render {
        /// Report the files that would change on stderr and exit 1, instead
        /// of writing them.
        #[arg(long)]
        check: bool,
        /// Install root to render from; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validates the knowledge base: schema, cross-entry invariants, one
    /// source for each id, the area of every entry, every generated file's
    /// freshness and budget, the `@AGENTS.md` import in CLAUDE.md, and
    /// compression markers that a context proxy left in a file. Checks each
    /// pack copy against its pin in `.houserules.json`, and each override and
    /// exclusion in `knowledge/overrides.json` against the installed packs.
    /// Checks that the `check` of each entry holds the fields that its type
    /// requires: a local entry as written, and an active pack entry with its
    /// override applied. Checks that the `pattern`, `subject`, `body_absent`,
    /// and each string of `exempt_subjects` of a check compile as regular
    /// expressions, for the same entries. An expression that does not compile
    /// gives the line `<file> <id>: check <field> is not a valid regex
    /// (<message>)`, where `<file>` is the topic file of a local entry, or the
    /// pack copy of a pack entry and of the entry that an override changes.
    /// Checks that every id a kit-owned file cites from a topic the kit seeds
    /// is an entry of the knowledge base. Checks the `runners` key of
    /// `.houserules.json`: it must be an object, and each tool in it needs a
    /// non-empty `command` array of strings, a `parser` of `sarif` or
    /// `exit-code`, a non-empty `install` string, and no other key; one line
    /// names the first fault of each tool. Checks that each `runner` check
    /// names a tool that `runners` defines. Checks that `workspaces`, when
    /// set, is a relative path inside the install root. While
    /// `workspaces` is not set, it names the default location
    /// `.houserules/work` when that folder exists and git does not ignore it.
    /// Names each `verify` path that git ignores, since a fresh clone lacks
    /// it: commit a copy under a tracked path and verify that. Outside a git
    /// work tree it runs no ignore test. Prints each override and exclusion on
    /// stdout.
    CheckKnowledge {
        /// Install root to check; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Prints one or more items by id, each routed by its own shape: an id
    /// `<topic>.<rest>` of a topic of the knowledge base reads the knowledge
    /// base and never needs the tracker; every other id goes to the installed
    /// tracker, which resolves its own id shapes (for the JSON tracker an item
    /// `<idPrefix>-\d{3}`, the project's own stamped prefix, an amendment or
    /// parked item `A-\d{2}`, `PP-\d+-\d{2}`, both kit-fixed regardless of
    /// idPrefix; for the GitHub tracker an issue `#<n>` or the bare `<n>`; for
    /// the Beads tracker an issue `<prefix>-<hash>`, the store's own issue
    /// prefix and a lowercase base-36 hash, such as `hr-a3f`, with `.<n>` for
    /// each child level). An id that both resolve fails. With no tracker, an id for the
    /// tracker prints one line that says how to add one, and exit 2; an id of a
    /// kit pack that the install lacks prints the line that names `pack add`.
    /// A pack entry with an override prints as its effective entry, with an
    /// `overridden` field that lists the replaced fields. An entry that
    /// `knowledge/overrides.json` excludes prints too, with an `excluded`
    /// field that holds the reason, or `true` when there is none. A pack entry
    /// with status `retired` or `superseded` prints, which the other commands
    /// leave out.
    Get {
        /// Backlog and/or knowledge ids to print, in any mix.
        ids: Vec<String>,
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists the work items of the installed tracker, optionally filtered.
    /// With no tracker it prints one line that says how to add one, and
    /// exits 2.
    List {
        /// Only items whose status is `open` or `partial`.
        #[arg(long)]
        open: bool,
        /// Only items that carry a deferral; each row then ends with its
        /// `deferral` object (`reason`, `origin`, `files`).
        #[arg(long)]
        deferred: bool,
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
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Prints one development batch's summary and item rows, from the
    /// installed tracker.
    Batch {
        /// The batch number (exactly one).
        numbers: Vec<String>,
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Applies `field=value` assignments to a work item of the installed
    /// tracker and rewrites its record. The fields are `status`, `batch`, and
    /// `milestone`; a write creates the label or milestone it needs.
    Set {
        /// The item id, followed by one or more `field=value` assignments.
        args: Vec<String>,
        /// Install root to write into; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Files a deferral in the installed tracker and prints its id on one
    /// line: `--title` files a new item (a chore, open, with the reason as its
    /// body), and `--item` marks an existing item as a deferral. Either form
    /// names the reason and the origin of the deferral, and the files it
    /// concerns. A title of more than 120 characters is refused.
    #[command(group(
        clap::ArgGroup::new("deferral_target")
            .required(true)
            .args(["title", "item"])
    ))]
    Defer {
        /// The title of a new item; at most 120 characters.
        #[arg(long)]
        title: Option<String>,
        /// The id of an existing item to mark as a deferral.
        #[arg(long)]
        item: Option<String>,
        /// Why the work waits.
        #[arg(long)]
        reason: String,
        /// The reference that says where the deferral came from: a batch, a
        /// task, a review, or a spec.
        #[arg(long)]
        origin: String,
        /// A file the deferral concerns, `/`-separated; repeat it for more.
        #[arg(long = "file")]
        files: Vec<String>,
        /// The section a new item goes to, which is the name of its items
        /// file; defaults to `general`. Goes with `--title`.
        #[arg(long, conflicts_with = "item")]
        section: Option<String>,
        /// Install root to write into; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validates the work items of the installed tracker: on every tracker
    /// the contract's checks of ids, references, batches, and deferral
    /// reasons; for the JSON
    /// tracker also its schema, cross-file invariants, and compression markers
    /// that a context proxy left in a backlog file; for the GitHub tracker the
    /// `houserules:` labels; for the Beads tracker the custom types and the
    /// `partial` status. With no tracker it prints one line and exits 0, or
    /// exits 1 when a `backlog/` directory exists.
    CheckBacklog {
        /// Install root to check; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Builds the rule package for a git range and runs every member's
    /// deterministic check. Every path it matches is relative to the install
    /// root; a changed path outside the install root is out of scope. A
    /// `runner` check starts the tool that the `runners` key of
    /// `.houserules.json` defines for the `tool` of the check, when a changed
    /// file matches the `files` of the check; an untriggered check starts no
    /// process. The tool reads the working tree, so a triggered `runner` check
    /// needs the working tree to be the tree at `--head`: `--head` must be the
    /// checked-out commit, and no file that the tool reads may differ from its
    /// content there. A triggered `runner` check that cannot run stops the
    /// audit before it has any row, with one line on stderr and exit 2: a
    /// check with no `tool`, a tool that `runners` does not define, a
    /// malformed `runners` entry, a program that is not installed (the line
    /// holds the `install` text of the entry) or cannot start, a working tree
    /// that is not the tree at `--head`, or SARIF output that is missing,
    /// invalid, or reports a failed run. The audit also resolves each
    /// `Work-Item:` and `Deferred:` trailer of the range's commits, and each
    /// `issues[].backlog` id of the deliverables it reads (`--report` and
    /// `--workspace`), through the installed tracker. It prints one line on
    /// stderr for each reference that fails, and exits 1 when one does, when a
    /// member's check fails, or when the tracker cannot be reached. With no
    /// tracker it resolves nothing: it prints one line that counts the
    /// references it skipped, or, when a `backlog/` directory exists, the line
    /// that asks for a tracker, and exits 1. The JSON result holds the same
    /// verdict: `references.failures` lists each failure line,
    /// `references.checked` counts the references that the tracker judged,
    /// `references.passed` counts those among them that passed,
    /// `references.skipped` counts those that the audit did not judge (no
    /// tracker, a tracker that cannot be reached, or a tracker that answered
    /// with an error), and `summary.reference_fail` counts the failures. The
    /// line that asks for a tracker, and the line of a tracker that cannot be
    /// reached, are failures too.
    Audit {
        /// The range's base ref.
        #[arg(long)]
        base: Option<String>,
        /// The range's head ref; defaults to `HEAD`.
        #[arg(long)]
        head: Option<String>,
        /// Extra knowledge ids to include in the package, comma-separated.
        /// With `--report`, a `report-field` check with `covers_ids` also
        /// requires that its field lists each of these ids.
        #[arg(long)]
        ids: Option<String>,
        /// A single JSON deliverable a `report-field` check reads directly.
        /// The audit also resolves the `issues[].backlog` ids of the file.
        /// With `--report` (a task audit), an entry tagged `controller`
        /// joins the package only when `--ids` names it. When the
        /// `self_audit` of the report records the range of this run, the
        /// audit compares the recorded summary and deterministic rows with
        /// this run, and the check that reads `self_audit` reports
        /// `self_audit differs at <JSON path>` for a difference; for another
        /// range it reports `not compared`, and a `self_audit` of `null`
        /// passes.
        #[arg(long)]
        report: Option<PathBuf>,
        /// A directory of `task-*-report.json` and `task-*-review*.json`
        /// files. A `report-field` check judges each report by its
        /// `files_changed`, and the audit resolves the `issues[].backlog` ids
        /// of every file.
        #[arg(long)]
        workspace: Option<PathBuf>,
        /// Also writes the JSON result that stdout prints to this file.
        #[arg(long)]
        json: Option<PathBuf>,
        /// Declares a spec-booked interim fail, `<rule>=<ref>`; repeatable.
        /// A deterministic row that failed gains `sanctioned`, which holds
        /// `<ref>`. It keeps its true `fail` result and gains `(sanctioned:
        /// <ref>)` in its evidence; the summary's `sanctioned_fail` counts it.
        /// A judged rule is accepted too: its row gains `sanctioned`, and the
        /// booking is never stale. A deterministic rule that did not fail
        /// gains no `sanctioned`: it is reported in `stale_sanctions`, not
        /// silently accepted. A rule outside the audit's package is an error:
        /// add it to `--ids`.
        #[arg(long)]
        sanctioned: Vec<String>,
        /// Install root to audit; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validates one or more deliverable JSON files against
    /// `.claude/schemas/deliverables.json`, and rejects a compression marker
    /// that a context proxy left in a file. For a task report with the status
    /// `DONE` or `DONE_WITH_CONCERNS`, it also fails a `self_audit` of `null`
    /// and a `self_audit` whose `summary.skipped` is above 0. For the same
    /// reports, it reads the capture of each run in `fix_rounds[].tests[]`,
    /// resolved against the install root, and fails a run whose output holds
    /// an audit result with a `summary.skipped` above 0:
    /// `<file>.fix_rounds[<i>].tests[<j>].capture: audit summary skipped is
    /// <n>; re-run audit with --report`. It leaves a capture that it cannot
    /// read to `check-report-claims`.
    Validate {
        /// Deliverable files to validate.
        files: Vec<PathBuf>,
        /// Install root the deliverables schema resolves from; defaults to
        /// the nearest directory at or above the current one that holds
        /// `.houserules.json`, no higher than the git top level, else the git
        /// top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Aggregates rule violations, sanctioned fails, unused injected ids,
    /// process cost, and knowledge-base proposals across one or more
    /// workspace directories' JSON deliverables. Each `rules` row names its
    /// `source`: `local`, or the name of the pack that ships the entry. Each
    /// `rules` row also counts its `verdicts`, `overrides`, the
    /// `override_rate`, and the `cost`: the sum of each judging review's
    /// share of its bytes. A fail that a sanction accepts counts under
    /// `sanctioned`, not under `violations`. Each workspace row counts
    /// `reversals`: the `Reversal:` lines of the `progress.md` in that
    /// workspace. The `packs` key sums entries, standing entries, judged
    /// rows, and violations for `local` and for each installed pack.
    Stats {
        /// One or more workspace directories to aggregate.
        #[arg(required = true, value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
        /// Reads only the N most recent of the workspaces. A workspace is
        /// more recent by the YYYY-MM-DD prefix of its folder name, then by
        /// name; a folder with no date prefix is the oldest. N above the
        /// number of workspaces reads all of them. N must be 1 or more.
        #[arg(long, value_name = "N")]
        window: Option<usize>,
        /// Install root to check the knowledge base under; defaults to the
        /// nearest directory at or above the current one that holds
        /// `.houserules.json`, no higher than the git top level, else the git
        /// top level.
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
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Prints the rule package one or more changed paths pull in: their
    /// areas' rule-shaped entries, plus every entry whose `verify` names
    /// one of the paths.
    For {
        /// Changed paths to resolve, each relative to the install root, not
        /// to the git top level.
        paths: Vec<String>,
        /// Print each matching entry whole, instead of its index row.
        #[arg(long)]
        full: bool,
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists every loaded knowledge topic's name, entry count, and title.
    /// A topic sums the entries of every source: the local topic file and
    /// each installed pack copy. An excluded entry and a pack entry with
    /// status `retired` or `superseded` do not count.
    Topics {
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists the standing rules, rules before invariants.
    Standing {
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Runs every `commits`-type knowledge check against a not-yet-committed
    /// message or a git range. In an install below the git top level, it
    /// skips a commit that changes at least one path and none of them under
    /// the install root; a merge counts the paths of every parent diff, and
    /// a commit that changes no path is checked. With `--from` it always
    /// prints one line that counts the skipped commits, `skipped 0 commits`
    /// included. With a message file it reads the staged files and prints
    /// that line only when it skips: an amendment that stages only paths
    /// outside the install root is skipped, though the amended commit
    /// touched the install. A `commits` check with `require_trailer` also
    /// reads the trailers of each message and fails a message that has none
    /// of that key. For a message file only `exempt_subjects` applies, since
    /// no author exists yet. It exits 1 when a `fail`-level check finds a
    /// violation. When the install holds no `commits` check and no
    /// `per_commit` `co-change` check, it checks nothing: it says so in place
    /// of `check-commit: ok` and exits 0.
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
        /// Install root to check from; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Installs the kit into a target git repository from the embedded
    /// payload: the kit-owned files, the seed-once files, and, on a new
    /// install, the `knowledge-base` pack with its pin in `.houserules.json`.
    /// It installs no tracker and no workflow unless `--tracker` or
    /// `--workflow` asks for one.
    /// Without `--target claude` it leaves out the Claude Code files and the
    /// hook entries, and prints the line that enables them. Running it again
    /// seeds what is missing and keeps what exists. It writes the line
    /// `.houserules/` to `.gitignore`: the folder holds the output of the
    /// houserules step and, by default, the batch workspaces. A
    /// `.houserules/step/` line becomes `.houserules/` in its place, a
    /// `.gitignore` that holds neither gets the line at its end, and one that
    /// holds `.houserules/` stays; a `.gitignore` that `ownedPaths` lists stays
    /// as found. Where `init` writes the deliverables schema and `--tracker`
    /// is absent, the `backlogId` pattern comes from the tracker that
    /// `.houserules.json` names, else it accepts any token. On an existing
    /// install it installs no pack and adds no area: only the `pack`
    /// commands and `update` change the pins. It never seeds a path that
    /// `ownedPaths` in `.houserules.json` lists; the files that `render`
    /// generates follow the knowledge base, not `ownedPaths`. It refuses an
    /// install whose schema is newer than the binary writes, with a line that
    /// names the release it needs, and an install whose schema is no install
    /// schema id, with a line that names the value; it changes nothing then.
    Init {
        /// The install root; defaults to the current directory. Unlike
        /// every read command's `--dir` above, this is not resolved against
        /// an enclosing install -- the directory itself is the root. It must
        /// exist and lie in a git repository: the top level, or a directory
        /// below it for a subdirectory install, which writes its CI workflow
        /// at the top level as `.github/workflows/knowledge-<folder>.yml`.
        /// A new install root (one with no `.houserules.json` yet) is
        /// refused when it nests with another install of the repository or
        /// would write another install's workflow file; a directory that
        /// holds its own `.houserules.json` is no new root.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// The backlog id prefix seeded schemas carry; defaults to `WI`.
        #[arg(long)]
        id_prefix: Option<String>,
        /// A render target to enable, in the install root. On an existing
        /// install it adds the target to `renderers`, seeds the files of the
        /// target that are missing, keeps every file that exists, and
        /// renders. For `claude`, it adds the hook entries to an existing
        /// `.claude/settings.json` and keeps the other keys. Where the install
        /// runs a workflow, it also adds the hook entry of the session-start
        /// script of that workflow. A `.claude/settings.json` that
        /// `ownedPaths` in `.houserules.json` lists gets no hook entry.
        #[arg(long, value_enum)]
        target: Option<install::InitTarget>,
        /// The tracker to install: `json` writes the JSON backlog files in
        /// `backlog/`, `github` keeps the work items in the issues of a GitHub
        /// repository (`--repo`, else the `origin` remote), and `beads` keeps
        /// them in a Beads store that exists already: run `bd init` (or
        /// `bd bootstrap` in a clone) first, or `init` refuses. Every tracker
        /// also writes the `backlogId` pattern of the deliverables schema for
        /// the adapter, and brings the seeded CI workflow of the install, where
        /// it has one, to the form of the adapter: `json` keeps the plain form,
        /// `github` adds the token and the read of the issues to the steps that
        /// run the gates, and `beads` adds the steps that install `bd` and
        /// bootstrap the store. An install keeps its tracker: `init` with another
        /// tracker refuses, and with the same one changes nothing for the
        /// tracker. `github` and `beads` refuse an install that has a
        /// `backlog/` directory.
        #[arg(long, value_enum)]
        tracker: Option<tracker::Adapter>,
        /// The GitHub repository of `--tracker github`, as `<owner>/<name>`;
        /// defaults to the repository that the `origin` remote names.
        #[arg(long, requires = "tracker")]
        repo: Option<String>,
        /// The workflow to install: `reference` writes the agent templates,
        /// the orchestrating and finishing-a-feature skills, the evals, the
        /// commit-msg hook, and the session-start script of the reference
        /// workflow at their install paths, and pins each file in
        /// `.houserules.json`. A workflow needs a tracker: the install has
        /// one, or `--tracker` installs one in the same run. A file that
        /// exists at a workflow path stays as it is, and a file that is not
        /// the kit's stays the project's.
        #[arg(long, value_enum)]
        workflow: Option<install::InitWorkflow>,
        /// With `--workflow`, also adds the kit packs that hold the rules the
        /// workflow requires and the install lacks. Without the flag, `init`
        /// names those packs.
        #[arg(long, requires = "workflow")]
        add_required_packs: bool,
    },
    /// Prints the file classes of the kit as JSON: the kit-owned files, the
    /// seed-once files, the copies of the kit packs, the D3 files, which
    /// `init` seeds and `update` keeps current through a baseline, the
    /// tracker files, which `init --tracker json` writes, the files of the
    /// reference workflow, which `init --workflow reference` writes, and the
    /// data file of the workflow. Each path is in one class. The CI workflow
    /// is a D3 file that only a GitHub-hosted target gets.
    Files,
    /// Self-updates the installed binary first, where the install channel
    /// supports it (`HOUSERULES_SKIP_SELF_UPDATE` opts out), then updates the
    /// install. After the self-update and before any write, it refuses an
    /// install whose schema is newer than the binary writes, with a line that
    /// names the release it needs, and an install whose schema is no install
    /// schema id, with a line that names the value. A directory with no
    /// `.houserules.json` and no `knowledge/` directory, at or above it within
    /// the git top level, is no install: `update` prints `no houserules install
    /// here (no .houserules.json); run houserules init`, exits 2, and writes
    /// nothing. When `--dir` names a directory other than the working
    /// directory, the line names it: `run houserules init --dir <dir>`, with
    /// `<dir>` quoted for a POSIX shell where it needs quoting. An install of
    /// houserules 1.x is converted to the 2.0 layout: the seeded entries become
    /// the five kit packs in `knowledge/packs/`, the `overrides` of
    /// `.houserules.json` become `ownedPaths` and `knowledge/overrides.json`,
    /// and the `schema` key is written last. A 1.x install with a `backlog/`
    /// directory gets the JSON tracker in the same run, and one without gets
    /// none. An opt-in entry of a kit pack that the conversion or a pin move
    /// brings onto the install arrives excluded: it writes the exclusion to
    /// `knowledge/overrides.json` once, records the id in `optInOffered`, and
    /// prints that the entry is opt-in. A topic file `knowledge/overrides.json`
    /// of 1.x stops the conversion before any write, and so does a `knowledge/`
    /// directory that does not exist or is empty: restore the directory from
    /// version control and run `update` again, or remove `.houserules.json` and
    /// run `init --id-prefix <prefix>` for a fresh install, with the `idPrefix`
    /// of the marker. A `knowledge/areas.json` that `ownedPaths` lists and that
    /// does not exist stops it too. On a 2.0 install it moves the pins of the
    /// kit packs to the versions of the binary and turns a local entry with a
    /// pack id into an override. Then it syncs the kit-owned files of an
    /// already-`init`ed target from the embedded payload: it writes a missing
    /// file, or one that the project has not changed, and lists it only when
    /// its bytes change; it keeps and lists a file the project changed (`kept
    /// <file> (locally modified)`); it never syncs a path that `ownedPaths` in
    /// `.houserules.json` lists. Where the install runs a workflow, it syncs
    /// the files of the workflow the same way: it writes a file that is missing
    /// or that the project has not changed, and it keeps and lists a changed
    /// file on every run (`kept <file> (locally modified; reference workflow
    /// <version>)`) until the project merges the change by hand. An install
    /// with files of the reference workflow at their paths and no workflow yet
    /// adopts the workflow: it pins each file that is the kit's, writes each
    /// file that is absent, and leaves a file that is not the kit's to the
    /// project, with a hand step. Where the session-start script of the Claude
    /// Code files still holds the ritual line of 1.x and the install runs the
    /// workflow, it adds the hook entry of the session-start script of the
    /// workflow to `.claude/settings.json`; the script that the kit wrote
    /// becomes the core script, and a script that the project changed stays,
    /// with a hand step. It removes the startup hook entry that the kit seeded
    /// before, where `.claude/settings.json` holds it unchanged. It delivers
    /// the deliverables schema and, with the JSON tracker, the backlog schema,
    /// and writes the `backlogId` pattern of the tracker of the install into
    /// the deliverables schema (a pattern that accepts any token, for an
    /// install with no tracker); a schema that the project changed stays, and
    /// `update` lists it (`kept <file> (locally modified)`); a schema with no
    /// baseline that differs from every content of it that the kit shipped, as
    /// in an install converted from 1.x, stays with a hand step on every run.
    /// With the Beads tracker it reads the issue prefix again: where `bd`
    /// cannot be reached, it keeps the pattern, prints one line on stderr, and
    /// finishes. It prints a hand step for a seeded CI workflow that the
    /// project changed and that lacks what the tracker needs or the PR-head
    /// checkout (`ref: ${{ github.event.pull_request.head.sha }}` under `with:`
    /// of its `actions/checkout` step). It also prints a hand step for each
    /// marker of the read-only token that a kept seeded CI workflow lacks:
    /// `contents: read` in the permissions of the gates job, for every install,
    /// and `persist-credentials: false` under `with:` of the `actions/checkout`
    /// step, for every install but one with the Beads tracker. A kept seeded CI
    /// workflow of the GitHub tracker gets one hand step for its permissions and
    /// one for its env, each only when the workflow lacks a marker of it
    /// (`contents: read` and `issues: read` for the permissions,
    /// `GH_TOKEN: ${{ github.token }}` for the env), so that no workflow is
    /// asked for two `permissions` keys. The hand step for the permissions
    /// follows the `permissions` key of the gates job: a workflow with no key is
    /// asked for the block, a workflow with a block that lacks a scope is asked
    /// for that scope in the block, a workflow with `permissions: read-all` gets
    /// no step for its permissions, and a workflow with `permissions: write-all`
    /// is asked to replace the scalar with the block. A workflow of the Beads
    /// tracker that holds `persist-credentials: false` gets a hand step that
    /// removes it, because `bd bootstrap --yes` needs the persisted credential.
    /// It writes each seed-once file that is missing and, on an install with the
    /// JSON tracker, each tracker data file that is missing. While `renderers`
    /// lacks `claude`, it skips the Claude Code files. It brings `.gitignore` to hold
    /// the line `.houserules/`: it replaces a `.houserules/step/` line in its
    /// place (`merged .gitignore (.houserules/step/ replaced by
    /// .houserules/)`), adds the line at the end where neither line is present
    /// (`merged .gitignore (.houserules/ added)`), writes the file when it is
    /// absent (`wrote .gitignore`), and prints no line for a `.gitignore` that
    /// holds `.houserules/` already. A `.gitignore` that `ownedPaths` lists
    /// stays as found. It restamps the install schema
    /// `houserules/install@2.1.0` to `houserules/install@2.2.0` and prints
    /// `wrote .houserules.json (houserules/install@2.2.0)` once; a second run
    /// prints no such line. The keys `runners` and `workspaces` of
    /// `.houserules.json` keep their values. It deletes any retired kit file
    /// still present, prints each hand step that the project has left (`hand
    /// step: ...`), and reports the stamped-to-running version drift. Every
    /// other command that reads the install refuses an install of 1.x with one
    /// line that names this command.
    Update {
        /// The install root; defaults to the current directory. Resolved the
        /// same way `init`'s own `--dir` is. A directory below the git top
        /// level must hold a `.houserules.json`.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// The backlog id prefix of an install that has no `.houserules.json`
        /// yet; defaults to `WI`. A directory that holds `knowledge/` and no
        /// `.houserules.json` needs it: `update` refuses without it, so that
        /// it never stamps `WI` on a project with its own prefix.
        #[arg(long)]
        id_prefix: Option<String>,
    },
    /// Runs a command and writes its output to the file `--out` under a
    /// header. The command runs with no shell, in the current directory, with
    /// the environment of `capture`, and with stdin closed. One pipe takes
    /// stdout and stderr, so the file holds both streams in the order the
    /// command wrote them. Line 1 of the file is the header, one JSON object
    /// with these keys in this order: `schema` (`houserules/capture@1.0.0`),
    /// `argv` (the command and its arguments), `cwd` (the absolute current
    /// directory), `start` and `end` (RFC 3339 UTC timestamps with nine
    /// fractional digits), `exit` (the exit code, or `null` when a signal
    /// ended the command), `signal` (the signal number, or `null`), `head`
    /// (the full sha of HEAD, or `null` outside a git work tree, before the
    /// first commit, or when `git status` fails), `dirty` (`true` when
    /// `git status --porcelain` prints a line, `false` when it prints none,
    /// `null` when `head` is `null`), and `lossy` (the number of byte
    /// sequences that were not UTF-8 and that `capture` replaced with U+FFFD).
    /// The output follows the first LF as
    /// UTF-8; `capture` adds no newline, and a command with no output leaves
    /// the header alone. When the file is written, `capture` prints `<file>:
    /// exit <n>` or `<file>: ended by signal <n>`, and it exits with the exit
    /// code of the command, or 128 plus the signal number. A setup error
    /// prints one line on stderr, writes no file at `--out` (a file that
    /// exists stays as it was), and exits 2: the file exists and `--force` is
    /// absent, the directory of the file does not exist, an argument, the
    /// current directory, or the `--out` path is not UTF-8, the current
    /// directory cannot be read, the command cannot start, or a file cannot be
    /// written. One error follows the start: the operating system gives no
    /// exit status for the command; `capture` then prints one line, writes no
    /// file at `--out`, and exits 2. A command that exits 2 also gives exit 2,
    /// and its file then exists.
    Capture {
        /// The file to write. Its directory must exist, and the file must not
        /// exist unless `--force` is set.
        #[arg(long, value_name = "FILE")]
        out: PathBuf,
        /// Replaces the file `--out` when it exists.
        #[arg(long)]
        force: bool,
        /// The command and its arguments; they follow `--`, and each is UTF-8.
        #[arg(last = true, required = true, value_name = "COMMAND")]
        command: Vec<OsString>,
    },
    /// Cross-checks one deliverable report's claims against the artifacts
    /// and git history it cites: the captures that runs cite, the excerpts
    /// of runs, the natural label of tdd entries (a RED capture that ended
    /// after the newest listed commit or exited 0), recorded citations, the
    /// evidence of self-review claims, re-run probes, ephemeral paths in the
    /// strings of the report, the self-audit head against the newest listed
    /// commit, commit shas in the narrative, self-audit pass ratios in the
    /// narrative, line numbers of cited files, counts beside a cited capture,
    /// and a cited sweep against the files the report changed. A capture must
    /// exist, be a capture, and have the sha256, command, and exit that the
    /// run records. An excerpt must be a byte-exact substring of the output
    /// of its capture, the file without its header line. A recorded citation
    /// must name a file that git tracks at HEAD, lines that exist there, an
    /// anchor that occurs once in the file and inside the cited lines, and the
    /// sha256 of those lines. A self-review claim that says every, all, or no
    /// remaining must name a capture in its evidence, and one that says pin,
    /// pins, or pinned must name an existing path. A re-run probe must exist
    /// in the folder of the report or below it. The ephemeral-path check reads
    /// every string of the report except a command, an excerpt, an anchor, a
    /// hash, and the self_audit. Every element of a comma or semicolon list of
    /// line numbers is checked. A count in the body of a listed commit is
    /// checked against a cited capture only, and a cited capture is read
    /// without its header line. With
    /// `--doc` or `--item` in place of a report, it reads a spec or a
    /// backlog item and names every cited workspace path that does not
    /// exist; a workspace path is a cited path that git ignores, such as a
    /// path below the default `workspaces` location `.houserules/work/`. It
    /// skips a cited path under `.houserules/step/`, which holds tool output.
    /// It judges a cited path through a symlink inside the install root at the
    /// target of the link, so a missing target is a missing file.
    #[command(group(
        clap::ArgGroup::new("claims_source")
            .required(true)
            .args(["report_path", "doc", "item"])
    ))]
    CheckReportClaims {
        /// The report file to check.
        report_path: Option<PathBuf>,
        /// A spec to check in place of a report: the file at this path, which
        /// resolves against the real current directory.
        #[arg(long, value_name = "PATH")]
        doc: Option<PathBuf>,
        /// A backlog item to check in place of a report: the item with this
        /// id, which the installed tracker reads.
        #[arg(long, value_name = "ID")]
        item: Option<String>,
        /// Install root the cited artifacts and commit shas resolve against;
        /// defaults to the nearest directory at or above the current one that
        /// holds `.houserules.json`, no higher than the git top level, else
        /// the git top level. Independent of `report_path` and `--doc`, which
        /// always resolve against the real current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Fills the fact fields of a task report from git and from captures, and
    /// writes the report back. `<REPORT>` resolves against the current
    /// directory; the captures, the cited files, and git resolve against the
    /// install root. It fills `commits` (every commit of `<base>..HEAD`,
    /// oldest first), `files_changed` (the paths that `git diff --name-only
    /// --no-renames <base>..HEAD` prints inside the install root, sorted), the
    /// `commits` of the last `fix_rounds` entry with `--fix-base`, and
    /// `self_audit` with `--audit` (the summary of the file that `audit --json`
    /// wrote, and its deterministic rows). For each run that holds a `capture`
    /// it fills `command` and `exit` from the header of the capture, and
    /// `sha256` when the run holds none. For each entry of `citations` it tests
    /// the anchor against the cited lines in `git show HEAD:./<path>`, run at
    /// the install root: the anchor must occur in the cited lines and nowhere
    /// else in the file. It fills `sha256` when the entry holds none. It never
    /// replaces a recorded `sha256`: a capture or a citation that differs from
    /// its recorded hash is a finding. A key that the report lacks goes at the
    /// end of the report. When it writes the report, it prints `<REPORT>: facts
    /// filled` and exits 0. A finding prints one line on stderr, writes
    /// nothing, and exits 1: a capture that does not exist, is no capture, or
    /// differs from its hash, and a citation whose path is not tracked at HEAD,
    /// whose lines do not exist, whose `end` is before `line`, or whose anchor
    /// is not in the cited lines or occurs more than once. A setup error prints
    /// one line, writes nothing, and exits 2: the report cannot be read or
    /// parsed or is no task report, `--base` or `--fix-base` is no commit, the
    /// report has no `fix_rounds` entry for `--fix-base`, `--audit` cannot be
    /// read or is no audit result, git cannot list the commits or the changed
    /// files, or the report cannot be written.
    ReportFacts {
        /// The task report to fill.
        report: PathBuf,
        /// The commit that the task started from: `commits` and
        /// `files_changed` come from the range `<REF>..HEAD`.
        #[arg(long, value_name = "REF")]
        base: String,
        /// The commit that the last fix round started from: the `commits` of
        /// the last `fix_rounds` entry come from the range `<REF>..HEAD`.
        #[arg(long, value_name = "REF")]
        fix_base: Option<String>,
        /// A file that `audit --json` wrote: it fills `self_audit`. It
        /// resolves against the real current directory.
        #[arg(long, value_name = "FILE")]
        audit: Option<PathBuf>,
        /// Install root that the captures, the cited files, and git resolve
        /// against; defaults to the nearest directory at or above the current
        /// one that holds `.houserules.json`, no higher than the git top level,
        /// else the git top level. Independent of `<REPORT>`, which always
        /// resolves against the real current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Moves done/dropped backlog items and done batch entries (through the
    /// installed tracker) and superseded/retired knowledge entries into their
    /// `archive/` mirrors. It moves local knowledge entries only: a pack entry
    /// with status `retired` or `superseded` stays in its pack copy, whether
    /// the pack ships the status or an override sets it. With no tracker and no
    /// `backlog/` directory it skips the backlog half and says so; with a
    /// `backlog/` directory and no tracker it moves nothing and exits 1.
    Archive {
        /// Install root to sweep; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Lists, adds, upgrades, and removes the knowledge packs of an install.
    Pack {
        #[command(subcommand)]
        action: PackCommand,
    },
}

/// The `pack` subcommands.
#[derive(Subcommand)]
enum PackCommand {
    /// Lists the installed packs with their version, source, entry count,
    /// and the counts of their overrides and exclusions, and the kit packs
    /// that the install lacks. A copy with no pin shows the source
    /// `unpinned`; a pin with no copy shows `no copy` for its counts.
    List {
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Installs a pack: a kit pack by name, an upstream pack from
    /// `git+<url>@<tag>#<path>`, or a pack file inside the install. It copies
    /// the pack into `knowledge/packs/`, pins it in `.houserules.json`, adds
    /// the areas it declares, and turns each local entry that the pack ships
    /// into an override or drops it. It then renders the generated files.
    Add {
        /// A kit pack name, `git+<url>@<tag>#<path>` (the tag must be a tag,
        /// never a branch or a commit), or the path of a pack file.
        source: String,
        /// Install root to write into; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Deletes a pack's copy and pin and removes the areas it added. It
    /// refuses while `knowledge/overrides.json` names one of the pack's ids.
    /// It then renders the generated files.
    Remove {
        /// The pack name.
        name: String,
        /// Install root to write into; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Moves an upstream pack to a new tag of its git source, or re-copies
    /// it from its file source. It keeps the overrides and the exclusions,
    /// prints the ids it adds and drops, and renders the generated files. A
    /// kit pack moves with `update`.
    Upgrade {
        /// The pack name.
        name: String,
        /// The new tag, for a pack with a git source; a pack with a file
        /// source takes none.
        tag: Option<String>,
        /// Install root to write into; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
}

/// How `main` runs D4 before a command, with the `--dir` that names its install
/// root, or `None` for the three commands that `main` leaves out: `files` and
/// `capture` read no install and run on any tree, and `update` runs D4 itself
/// after its self-update phase (`install::cmd_update`, spec C22). `init` refuses a 1.x
/// install with its own line, so it runs on a tree with no install schema or
/// an older one (`Guard::ConvertsInstall`); every other command reads the
/// install (`Guard::ReadsInstall`). The match names every variant, so a new
/// command cannot skip D4 unseen.
fn d4_guard(command: &Command) -> Option<(install::Guard, &Option<PathBuf>)> {
    use install::Guard::{ConvertsInstall, ReadsInstall};
    match command {
        Command::Files | Command::Capture { .. } | Command::Update { .. } => None,
        Command::Init { dir, .. } => Some((ConvertsInstall, dir)),
        Command::Render { dir, .. }
        | Command::CheckKnowledge { dir }
        | Command::Get { dir, .. }
        | Command::List { dir, .. }
        | Command::Batch { dir, .. }
        | Command::Set { dir, .. }
        | Command::Defer { dir, .. }
        | Command::CheckBacklog { dir }
        | Command::Audit { dir, .. }
        | Command::Validate { dir, .. }
        | Command::Stats { dir, .. }
        | Command::Index { dir, .. }
        | Command::For { dir, .. }
        | Command::Topics { dir }
        | Command::Standing { dir }
        | Command::CheckCommit { dir, .. }
        | Command::CheckReportClaims { dir, .. }
        | Command::ReportFacts { dir, .. }
        | Command::Archive { dir } => Some((ReadsInstall, dir)),
        Command::Pack { action } => Some((
            ReadsInstall,
            match action {
                PackCommand::List { dir }
                | PackCommand::Add { dir, .. }
                | PackCommand::Remove { dir, .. }
                | PackCommand::Upgrade { dir, .. } => dir,
            },
        )),
    }
}

/// D4: resolves the install root of a command like the command does, and
/// refuses the tree that `guard` does not allow. A refusal is one stderr line
/// and exit 2. `init` resolves `--dir` as itself (the directory is the root);
/// a path that cannot resolve is its to name.
fn refuse_install_schema(guard: install::Guard, dir: &Option<PathBuf>) -> Result<(), ExitCode> {
    let root = match guard {
        install::Guard::ReadsInstall => root::resolve_root(dir.clone())?,
        install::Guard::ConvertsInstall => {
            match node_path::resolve_like_node(dir.as_deref().unwrap_or(Path::new("."))) {
                Ok(root) => root,
                Err(_) => return Ok(()),
            }
        }
    };
    install::refuse_install_schema(&root, guard).map_err(|line| {
        eprintln!("{line}");
        ExitCode::from(2)
    })
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    update_check::maybe_print_header(matches!(cli.command, Some(Command::Update { .. })));
    if let Some((guard, dir)) = cli.command.as_ref().and_then(d4_guard)
        && let Err(code) = refuse_install_schema(guard, dir)
    {
        return code;
    }
    match cli.command {
        // Unreachable through the CLI itself: `arg_required_else_help` makes
        // clap exit the process before `main` ever sees a bare invocation.
        // Kept for match exhaustiveness over `Option<Command>`.
        None => ExitCode::SUCCESS,
        Some(Command::Render { check, dir }) => rules::cmd_render(dir, check),
        Some(Command::CheckKnowledge { dir }) => {
            rules::cmd_check_knowledge(dir, install::kit_citation_scope)
        }
        Some(Command::Get { ids, dir }) => get::cmd_get(dir, ids),
        Some(Command::List {
            open,
            deferred,
            status,
            milestone,
            section,
            item_type,
            batch,
            dir,
        }) => backlog::cmd_list(
            dir,
            tracker::ListOpts {
                open,
                status,
                milestone,
                section,
                item_type,
                batch,
                deferred,
            },
        ),
        Some(Command::Batch { numbers, dir }) => backlog::cmd_batch(dir, numbers),
        Some(Command::Set { args, dir }) => backlog::cmd_set(dir, args),
        Some(Command::Defer {
            title,
            item,
            reason,
            origin,
            files,
            section,
            dir,
        }) => backlog::cmd_defer(
            dir,
            tracker::DeferRequest {
                item,
                title,
                reason,
                origin,
                files,
                section,
            },
        ),
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
        Some(Command::Stats {
            workspaces,
            window,
            dir,
        }) => rules::cmd_stats(dir, workspaces, window),
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
        Some(Command::Init {
            dir,
            id_prefix,
            target,
            tracker,
            repo,
            workflow,
            add_required_packs,
        }) => install::cmd_init(
            dir,
            id_prefix,
            target,
            tracker.map(|adapter| install::TrackerRequest { adapter, repo }),
            workflow.map(|workflow| install::WorkflowRequest {
                workflow,
                add_required_packs,
            }),
        ),
        Some(Command::Files) => install::cmd_files(),
        Some(Command::Update { dir, id_prefix }) => install::cmd_update(dir, id_prefix),
        Some(Command::Capture {
            out,
            force,
            command,
        }) => capture::cmd_capture(out, force, command),
        Some(Command::CheckReportClaims {
            report_path,
            doc,
            item,
            dir,
        }) => match (report_path, doc, item) {
            (Some(path), None, None) => report_claims::cmd_check_report_claims(
                dir,
                report_claims::ClaimsSource::Report(path),
            ),
            (None, Some(path), None) => {
                report_claims::cmd_check_report_claims(dir, report_claims::ClaimsSource::Doc(path))
            }
            (None, None, Some(id)) => {
                report_claims::cmd_check_report_claims(dir, report_claims::ClaimsSource::Item(id))
            }
            _ => {
                eprintln!(
                    "check-report-claims needs exactly one of a report path, --doc, or --item"
                );
                ExitCode::from(2)
            }
        },
        Some(Command::ReportFacts {
            report,
            base,
            fix_base,
            audit,
            dir,
        }) => report_facts::cmd_report_facts(dir, report, base, fix_base, audit),
        Some(Command::Archive { dir }) => archive::cmd_archive(dir),
        Some(Command::Pack { action }) => match action {
            PackCommand::List { dir } => pack::cmd_pack_list(dir),
            PackCommand::Add { source, dir } => pack::cmd_pack_add(dir, source),
            PackCommand::Remove { name, dir } => pack::cmd_pack_remove(dir, name),
            PackCommand::Upgrade { name, tag, dir } => pack::cmd_pack_upgrade(dir, name, tag),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    /// `Cli`'s own doc comment IS clap's `about`/`long_about` text: keeping
    /// it to one adopter-facing line means `--help` (long help) shows
    /// nothing more than `-h` (short help) does. `long_about` is `None`,
    /// not `Some` duplicating `about` -- `clap_derive` only sets
    /// `long_about` when a doc comment carries a second, blank-line-
    /// separated paragraph.
    #[test]
    fn the_top_level_about_is_the_one_adopter_facing_line_with_no_long_about() {
        let command = Cli::command();
        assert_eq!(
            command.get_about().map(ToString::to_string),
            Some("The `houserules` command line".to_string())
        );
        assert!(
            command.get_long_about().is_none(),
            "long_about must stay unset, not carry a maintainer-narration paragraph back: {:?}",
            command.get_long_about().map(ToString::to_string)
        );
    }

    /// The same maintainer-narration class swept across every subcommand:
    /// none carries a `long_about` distinct from its own short `about`
    /// either. Enumerated from `Cli::command().get_subcommands()` -- the
    /// same set clap itself renders, not a hand-typed list -- exactly the
    /// 24 `Command` variants (clap's own built-in `help` subcommand is
    /// rendered separately and is not itself one of `get_subcommands()`'s
    /// entries).
    #[test]
    fn no_subcommand_carries_a_long_about_beyond_its_short_one() {
        let command = Cli::command();
        let subcommands: Vec<_> = command.get_subcommands().collect();
        assert_eq!(
            subcommands.len(),
            24,
            "expected the full subcommand surface (24 Command variants), got: {:?}",
            subcommands.iter().map(|s| s.get_name()).collect::<Vec<_>>()
        );
        for subcommand in subcommands {
            let about = subcommand.get_about().map(ToString::to_string);
            let long_about = subcommand.get_long_about().map(ToString::to_string);
            assert!(
                long_about.is_none() || long_about == about,
                "{}: long_about {long_about:?} diverges from about {about:?} -- a maintainer-\
                 narration paragraph beyond the short about",
                subcommand.get_name(),
            );
        }
    }

    /// The checks `report_claims::check_report_claims` runs, in call order,
    /// each with the phrase the `check-report-claims` about must carry for
    /// it. A callee is written as the call spells it (`natural_red::` and
    /// all). `the_check_report_claims_about_names_every_check` derives the
    /// callees from the source and compares them with this list.
    const HELP_CHECKS: [(&str, &str); 13] = [
        ("check_captures", "captures that runs cite"),
        ("check_excerpts", "excerpts of runs"),
        (
            "natural_red::check_natural_red_labels",
            "natural label of tdd entries",
        ),
        ("check_recorded_citations", "recorded citations"),
        (
            "check_self_review_evidence",
            "evidence of self-review claims",
        ),
        ("check_rerun_probes", "re-run probes"),
        ("check_ephemeral_paths", "ephemeral paths"),
        ("check_self_audit_head_is_current", "self-audit head"),
        (
            "check_narrative_shas_resolve",
            "commit shas in the narrative",
        ),
        ("check_self_audit_narrative", "pass ratios in the narrative"),
        ("check_citation_lines", "line numbers of cited files"),
        (
            "check_narrative_number_claims",
            "counts beside a cited capture",
        ),
        ("check_narrative_sweep_coverage", "cited sweep"),
    ];

    /// The callee of every check call in the body of `fn
    /// check_report_claims` in `source`, in call order. Fails with a named
    /// message when the function or its body cannot be found, or when the
    /// body holds no check call: an empty derivation is never a pass.
    ///
    /// What counts as a check call: a statement of the body that ends in
    /// `;` and names `errors` (every check reports through that
    /// accumulator), other than the `let mut errors` declaration. The callee
    /// is the text before the statement's first `(`. A call split over
    /// several lines is one statement and counts once. Whole-line `//`
    /// comments are dropped first. A check that is not called through a
    /// statement naming `errors` is not counted; the callee comparison in
    /// the test then names the difference.
    fn check_calls_in(source: &str) -> Result<Vec<String>, String> {
        let source = format!("\n{source}");
        let start = source.find("\nfn check_report_claims(").ok_or_else(|| {
            "the source has no `fn check_report_claims(` at the start of a line".to_string()
        })?;
        let body_start = source[start..]
            .find('{')
            .map(|offset| start + offset + 1)
            .ok_or_else(|| "`fn check_report_claims` has no body".to_string())?;
        let body_end = source[body_start..]
            .find("\n}")
            .map(|offset| body_start + offset)
            .ok_or_else(|| {
                "the body of `fn check_report_claims` has no closing brace at the start of a line"
                    .to_string()
            })?;
        let body = source[body_start..body_end]
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut statements: Vec<&str> = body.split(';').collect();
        // The text after the last `;` is the tail expression, or nothing.
        statements.pop();
        let calls: Vec<String> = statements
            .iter()
            .map(|statement| statement.trim())
            .filter(|statement| {
                statement.contains("errors") && !statement.starts_with("let mut errors")
            })
            .map(|statement| {
                statement
                    .split('(')
                    .next()
                    .unwrap_or(statement)
                    .trim()
                    .to_string()
            })
            .collect();
        if calls.is_empty() {
            return Err("the body of `fn check_report_claims` holds no check call (a `;`-terminated statement that names `errors`)".to_string());
        }
        Ok(calls)
    }

    /// A call split over several lines counts once, a commented-out call
    /// does not count, and the callee keeps its module path.
    #[test]
    fn check_calls_in_counts_a_split_call_once_and_skips_comments() {
        let source = "fn other() {}\n\nfn check_report_claims(path: &Path) -> Result<Vec<String>, String> {\n    let report = load_report(path)?;\n    let mut errors = Vec::new();\n    // check_commented_out(&mut errors);\n    check_one(&report, &mut errors);\n    module::check_two(\n        &report,\n        &mut errors,\n    );\n    Ok(errors)\n}\n";
        assert_eq!(
            check_calls_in(source),
            Ok(vec![
                "check_one".to_string(),
                "module::check_two".to_string()
            ])
        );
    }

    /// A source without the function fails with a message that names it.
    #[test]
    fn check_calls_in_names_a_missing_function() {
        let error = check_calls_in("fn other() {}\n").expect_err("no function to derive from");
        assert!(error.contains("fn check_report_claims("), "got: {error}");
    }

    /// A body with no check call fails with a message that names the
    /// shape it looked for, instead of deriving an empty list.
    #[test]
    fn check_calls_in_names_a_body_without_a_check_call() {
        let source = "fn check_report_claims() -> Result<Vec<String>, String> {\n    let mut errors = Vec::new();\n    Ok(errors)\n}\n";
        let error = check_calls_in(source).expect_err("no check call to derive");
        assert!(error.contains("holds no check call"), "got: {error}");
    }

    /// `check-report-claims --help` names every check the command runs. The
    /// callees come from the source of `check_report_claims` (read with
    /// `include_str!`, see `check_calls_in` for what counts as a call), so
    /// a check added there without a pair in `HELP_CHECKS` fails here, and
    /// so does a phrase added to the about with no call behind it. The
    /// about is read as a comma-separated list after its first `: `, up to
    /// the end of that sentence (the sentence about `--doc` and `--item`
    /// follows it), in call order: item `i` must carry the phrase of pair
    /// `i`.
    #[test]
    fn the_check_report_claims_about_names_every_check() {
        let calls = check_calls_in(include_str!("report_claims.rs"))
            .expect("derive the check calls of check_report_claims");
        let pinned: Vec<&str> = HELP_CHECKS.iter().map(|(callee, _)| *callee).collect();
        assert_eq!(
            calls,
            pinned,
            "check_report_claims makes {} check calls but HELP_CHECKS pairs {} with a help phrase; \
             add or remove the pair here and its phrase in the about of `CheckReportClaims` in main.rs",
            calls.len(),
            pinned.len()
        );
        let command = Cli::command();
        let about = command
            .find_subcommand("check-report-claims")
            .and_then(|subcommand| subcommand.get_about())
            .map(ToString::to_string)
            .expect("check-report-claims has an about");
        let list = about
            .split_once(": ")
            .map(|(_, rest)| rest.split_once(". ").map_or(rest, |(checks, _)| checks))
            .map(|checks| checks.trim_end_matches('.'))
            .expect("the about lists its checks after a colon");
        let normalized = list.replace(", and ", ", ");
        let items: Vec<&str> = normalized.split(", ").collect();
        assert_eq!(
            items.len(),
            HELP_CHECKS.len(),
            "the about names {} checks ({items:?}) but check_report_claims makes {}",
            items.len(),
            HELP_CHECKS.len()
        );
        for (item, (callee, phrase)) in items.iter().zip(HELP_CHECKS) {
            assert!(
                item.contains(phrase),
                "the about item `{item}` does not carry `{phrase}`, the phrase of {callee}"
            );
        }
    }

    /// `check-report-claims --help` states what the run checks demand of a
    /// capture and of an excerpt, and no longer names the three checks that
    /// the capture replaced (batch-36 spec 6, design.md 212).
    #[test]
    fn the_check_report_claims_about_states_the_run_checks() {
        let about = about_of("check-report-claims");
        for sentence in [
            "A capture must exist, be a capture, and have the sha256, command, and exit that the run records.",
            "An excerpt must be a byte-exact substring of the output of its capture, the file without its header line.",
            "the natural label of tdd entries (a RED capture that ended after the newest listed commit or exited 0)",
        ] {
            assert!(
                about.contains(sentence),
                "the about lacks `{sentence}`: {about:?}"
            );
        }
        for retired in [
            "redirected captures",
            "truncation markers",
            "paste-run lint",
        ] {
            assert!(
                !about.contains(retired),
                "the about still names the retired check `{retired}`: {about:?}"
            );
        }
    }

    /// `check-report-claims --help` states what the claim checks demand of a
    /// recorded citation, of the evidence of a self-review claim, of a re-run
    /// probe, of the strings that the ephemeral-path check reads, of a list of
    /// line numbers, and of a count in a commit body (batch-36 spec 6, checks 4
    /// to 11).
    #[test]
    fn the_check_report_claims_about_states_the_claim_checks() {
        let about = about_of("check-report-claims");
        for sentence in [
            "A recorded citation must name a file that git tracks at HEAD, lines that exist there, an anchor that occurs once in the file and inside the cited lines, and the sha256 of those lines.",
            "A self-review claim that says every, all, or no remaining must name a capture in its evidence, and one that says pin, pins, or pinned must name an existing path.",
            "A re-run probe must exist in the folder of the report or below it.",
            "The ephemeral-path check reads every string of the report except a command, an excerpt, an anchor, a hash, and the self_audit.",
            "Every element of a comma or semicolon list of line numbers is checked.",
            "A count in the body of a listed commit is checked against a cited capture only, and a cited capture is read without its header line.",
        ] {
            assert!(
                about.contains(sentence),
                "the about lacks `{sentence}`: {about:?}"
            );
        }
    }

    /// `check-knowledge --help` names the kit-owned citation lint, the
    /// check that is none of schema, cross-entry invariants, or a generated
    /// file's freshness and budget, and the vocabulary the lint reads: the
    /// ids of the topics the kit seeds, not those of a topic the adopter adds.
    #[test]
    fn the_check_knowledge_about_names_the_kit_owned_citation_lint() {
        let command = Cli::command();
        let about = command
            .find_subcommand("check-knowledge")
            .and_then(|subcommand| subcommand.get_about())
            .map(ToString::to_string)
            .expect("check-knowledge has an about");
        assert!(
            about.contains("kit-owned file cites"),
            "the about does not name the kit-owned citation lint: {about:?}"
        );
        assert!(
            about.contains("from a topic the kit seeds"),
            "the about does not name the topics the lint reads: {about:?}"
        );
    }

    /// The about of `name`, as `--help` prints it.
    fn about_of(name: &str) -> String {
        Cli::command()
            .find_subcommand(name)
            .and_then(|subcommand| subcommand.get_about())
            .map(ToString::to_string)
            .unwrap_or_else(|| panic!("{name} has an about"))
    }

    /// `check-knowledge --help` names each class of check the command runs
    /// beyond the schema and the generated files, and the one kind of line
    /// it prints on stdout. Each phrase pairs with the check it names.
    #[test]
    fn the_check_knowledge_about_names_every_class_of_check_and_line() {
        let about = about_of("check-knowledge");
        for (phrase, names) in [
            ("one source for each id", "one id, one source"),
            ("against its pin in `.houserules.json`", "the pack pins"),
            (
                "`knowledge/overrides.json`",
                "the validity of overrides.json",
            ),
            ("the area of every entry", "the area of an entry"),
            ("`@AGENTS.md` import in CLAUDE.md", "the CLAUDE.md import"),
            ("compression markers", "the marker gate"),
            ("each override and exclusion on stdout", "the listing"),
        ] {
            assert!(
                about.contains(phrase),
                "the about does not name {names} ({phrase:?}): {about:?}"
            );
        }
    }

    /// The marker gate runs in `check-knowledge`, `check-backlog`, and
    /// `validate`, so each of the three abouts names it.
    #[test]
    fn the_three_gate_commands_name_the_compression_marker_gate() {
        let silent: Vec<&str> = ["check-knowledge", "check-backlog", "validate"]
            .into_iter()
            .filter(|name| !about_of(name).contains("compression marker"))
            .collect();
        assert!(
            silent.is_empty(),
            "the about of {silent:?} does not name the marker gate"
        );
    }

    /// The CLI surface: exactly these 24 subcommand names, in `--help`'s own
    /// declaration order. The count assertion above cannot see a same-length
    /// rename, because a renamed variant keeps `get_subcommands().len()`;
    /// comparing the names themselves does.
    #[test]
    fn the_cli_surface_names_exactly_these_24_subcommands() {
        let command = Cli::command();
        let names: Vec<&str> = command.get_subcommands().map(|s| s.get_name()).collect();
        assert_eq!(
            names,
            [
                "render",
                "check-knowledge",
                "get",
                "list",
                "batch",
                "set",
                "defer",
                "check-backlog",
                "audit",
                "validate",
                "stats",
                "index",
                "for",
                "topics",
                "standing",
                "check-commit",
                "init",
                "files",
                "update",
                "capture",
                "check-report-claims",
                "report-facts",
                "archive",
                "pack",
            ],
            "the CLI surface (the 1.0 surface of design.md 5.82, then `pack` and `defer`) changed"
        );
    }

    /// `pack` has exactly these four subcommands, each with a short `about`
    /// and no `long_about` beyond it, as every top-level subcommand has.
    #[test]
    fn the_pack_subcommands_are_exactly_these_four() {
        let command = Cli::command();
        let pack = command.find_subcommand("pack").expect("pack is declared");
        let names: Vec<&str> = pack.get_subcommands().map(|s| s.get_name()).collect();
        assert_eq!(names, ["list", "add", "remove", "upgrade"]);
        for subcommand in pack.get_subcommands() {
            let about = subcommand.get_about().map(ToString::to_string);
            let long_about = subcommand.get_long_about().map(ToString::to_string);
            assert!(about.is_some(), "{}: no about", subcommand.get_name());
            assert!(
                long_about.is_none() || long_about == about,
                "{}: long_about {long_about:?} diverges from about {about:?}",
                subcommand.get_name(),
            );
        }
    }

    /// `get`'s own about text is adopter-visible (`houserules --help`'s
    /// Commands list); HR-100 made the item shape follow the project's
    /// own stamped `idPrefix`, so this text must not assert the
    /// hardcoded `HR-` shape the fix removed from the code itself.
    #[test]
    fn get_subcommand_about_states_the_item_shape_follows_idprefix_not_a_hardcoded_hr() {
        let command = Cli::command();
        let get = command
            .find_subcommand("get")
            .expect("get is a declared subcommand");
        let about = get
            .get_about()
            .map(ToString::to_string)
            .expect("get has an about");
        assert!(
            !about.contains("HR-"),
            "get's about still names the hardcoded HR- shape: {about:?}"
        );
        assert!(
            about.contains("idPrefix"),
            "get's about does not state that the item shape follows idPrefix: {about:?}"
        );
    }

    /// Every help text of `command` and of its subcommands, as pairs of the
    /// surface it belongs to and its text: the about, the long about, and the
    /// help and long help of each argument.
    fn help_texts(command: &clap::Command, surface: &str, out: &mut Vec<(String, String)>) {
        let surface = if surface.is_empty() {
            command.get_name().to_string()
        } else {
            format!("{surface} {}", command.get_name())
        };
        for text in [command.get_about(), command.get_long_about()]
            .into_iter()
            .flatten()
        {
            out.push((surface.clone(), text.to_string()));
        }
        for argument in command.get_arguments() {
            for text in [argument.get_help(), argument.get_long_help()]
                .into_iter()
                .flatten()
            {
                out.push((
                    format!("{surface} --{}", argument.get_id()),
                    text.to_string(),
                ));
            }
        }
        for subcommand in command.get_subcommands() {
            help_texts(subcommand, &surface, out);
        }
    }

    /// Help is for the adopter who runs the command. No help text names a
    /// Rust path, a source file, or a doc comment: an adopter cannot read
    /// them. The texts come from the clap definition itself, so a command or
    /// an argument added later is swept without an edit here.
    #[test]
    fn no_help_text_names_a_rust_symbol_a_source_file_or_a_doc_comment() {
        let mut texts = Vec::new();
        help_texts(&Cli::command(), "", &mut texts);
        assert!(texts.len() > 50, "the sweep found {} texts", texts.len());
        let offenders: Vec<String> = texts
            .into_iter()
            .filter(|(_, text)| {
                ["::", ".rs", "own doc", "module doc"]
                    .iter()
                    .any(|needle| text.contains(needle))
            })
            .map(|(surface, _)| surface)
            .collect();
        assert!(
            offenders.is_empty(),
            "these help texts name a Rust symbol, a source file, or a doc comment: {offenders:?}"
        );
    }

    /// `init --help` says what `init` installs and what it leaves alone: a new
    /// install gets the `knowledge-base` pack and no tracker or workflow
    /// unless a flag asks for one, a second run seeds the missing files and
    /// keeps those that exist, an existing install gets no pack and no area, a
    /// path of `ownedPaths` is never seeded, and an install schema that is
    /// newer or malformed is refused. The update about says the same of its
    /// sync, because both commands end with `render`, and names the acts of the
    /// conversion steps: the JSON tracker, the adoption of the workflow, the
    /// hook entry, the delivery of the schemas, the `bd` that cannot be
    /// reached, and the hand step of the CI workflow. `--target` says that a
    /// settings file of `ownedPaths` gets no hook entry. `--tracker` says that
    /// the CI workflow takes the form of the adapter. The test pins every
    /// sentence of the init about, of `--target`, and of `--tracker`, whole and
    /// in order, and each new sentence of the update about, so a deleted,
    /// added, or reworded sentence fails it.
    #[test]
    fn the_init_help_states_what_an_existing_install_and_an_owned_path_get() {
        let command = Cli::command();
        let init = command.find_subcommand("init").expect("init is declared");
        let about = init
            .get_about()
            .map(ToString::to_string)
            .expect("init has an about");
        assert_eq!(
            sentences(&about),
            [
                "Installs the kit into a target git repository from the embedded payload: the kit-owned files, the seed-once files, and, on a new install, the `knowledge-base` pack with its pin in `.houserules.json`",
                "It installs no tracker and no workflow unless `--tracker` or `--workflow` asks for one",
                "Without `--target claude` it leaves out the Claude Code files and the hook entries, and prints the line that enables them",
                "Running it again seeds what is missing and keeps what exists",
                "It writes the line `.houserules/` to `.gitignore`: the folder holds the output of the houserules step and, by default, the batch workspaces",
                "A `.houserules/step/` line becomes `.houserules/` in its place, a `.gitignore` that holds neither gets the line at its end, and one that holds `.houserules/` stays; a `.gitignore` that `ownedPaths` lists stays as found",
                "Where `init` writes the deliverables schema and `--tracker` is absent, the `backlogId` pattern comes from the tracker that `.houserules.json` names, else it accepts any token",
                "On an existing install it installs no pack and adds no area: only the `pack` commands and `update` change the pins",
                "It never seeds a path that `ownedPaths` in `.houserules.json` lists; the files that `render` generates follow the knowledge base, not `ownedPaths`",
                "It refuses an install whose schema is newer than the binary writes, with a line that names the release it needs, and an install whose schema is no install schema id, with a line that names the value; it changes nothing then",
            ],
            "the sentences of the init about"
        );
        let update = command
            .find_subcommand("update")
            .and_then(|update| update.get_about())
            .map(ToString::to_string)
            .expect("update has an about");
        assert!(
            update.contains("it never syncs a path that `ownedPaths` in `.houserules.json` lists")
                && !update.contains("never writes"),
            "the update about limits ownedPaths to the synced files: {update}"
        );
        assert!(
            !update.contains("such a workflow lacks")
                && !update.contains("names the permissions")
                && !update.contains("shipped stays, with a hand step"),
            "the update about scopes the token hand steps to a kind of workflow, or promises a hand step for every kept schema: {update}"
        );
        let update_sentences = sentences(&update);
        for sentence in [
            "A 1.x install with a `backlog/` directory gets the JSON tracker in the same run, and one without gets none",
            "An opt-in entry of a kit pack that the conversion or a pin move brings onto the install arrives excluded: it writes the exclusion to `knowledge/overrides.json` once, records the id in `optInOffered`, and prints that the entry is opt-in",
            "Where the install runs a workflow, it syncs the files of the workflow the same way: it writes a file that is missing or that the project has not changed, and it keeps and lists a changed file on every run (`kept <file> (locally modified; reference workflow <version>)`) until the project merges the change by hand",
            "An install with files of the reference workflow at their paths and no workflow yet adopts the workflow: it pins each file that is the kit's, writes each file that is absent, and leaves a file that is not the kit's to the project, with a hand step",
            "Where the session-start script of the Claude Code files still holds the ritual line of 1.x and the install runs the workflow, it adds the hook entry of the session-start script of the workflow to `.claude/settings.json`; the script that the kit wrote becomes the core script, and a script that the project changed stays, with a hand step",
            "It removes the startup hook entry that the kit seeded before, where `.claude/settings.json` holds it unchanged",
            "It delivers the deliverables schema and, with the JSON tracker, the backlog schema, and writes the `backlogId` pattern of the tracker of the install into the deliverables schema (a pattern that accepts any token, for an install with no tracker); a schema that the project changed stays, and `update` lists it (`kept <file> (locally modified)`); a schema with no baseline that differs from every content of it that the kit shipped, as in an install converted from 1.x, stays with a hand step on every run",
            "With the Beads tracker it reads the issue prefix again: where `bd` cannot be reached, it keeps the pattern, prints one line on stderr, and finishes",
            "It prints a hand step for a seeded CI workflow that the project changed and that lacks what the tracker needs or the PR-head checkout (`ref: ${{ github.event.pull_request.head.sha }}` under `with:` of its `actions/checkout` step)",
            "It also prints a hand step for each marker of the read-only token that a kept seeded CI workflow lacks: `contents: read` in the permissions of the gates job, for every install, and `persist-credentials: false` under `with:` of the `actions/checkout` step, for every install but one with the Beads tracker",
            "A kept seeded CI workflow of the GitHub tracker gets one hand step for its permissions and one for its env, each only when the workflow lacks a marker of it (`contents: read` and `issues: read` for the permissions, `GH_TOKEN: ${{ github.token }}` for the env), so that no workflow is asked for two `permissions` keys",
            "The hand step for the permissions follows the `permissions` key of the gates job: a workflow with no key is asked for the block, a workflow with a block that lacks a scope is asked for that scope in the block, a workflow with `permissions: read-all` gets no step for its permissions, and a workflow with `permissions: write-all` is asked to replace the scalar with the block",
            "A workflow of the Beads tracker that holds `persist-credentials: false` gets a hand step that removes it, because `bd bootstrap --yes` needs the persisted credential",
            "It writes each seed-once file that is missing and, on an install with the JSON tracker, each tracker data file that is missing",
        ] {
            assert!(
                update_sentences.contains(&sentence),
                "the update about lacks the sentence {sentence:?}: {update}"
            );
        }
        let target = init
            .get_arguments()
            .find(|argument| argument.get_id() == "target")
            .and_then(|argument| argument.get_help())
            .map(ToString::to_string)
            .expect("init --target has a help");
        assert_eq!(
            sentences(&target),
            [
                "A render target to enable, in the install root",
                "On an existing install it adds the target to `renderers`, seeds the files of the target that are missing, keeps every file that exists, and renders",
                "For `claude`, it adds the hook entries to an existing `.claude/settings.json` and keeps the other keys",
                "Where the install runs a workflow, it also adds the hook entry of the session-start script of that workflow",
                "A `.claude/settings.json` that `ownedPaths` in `.houserules.json` lists gets no hook entry",
            ],
            "the sentences of the init --target help"
        );
        let tracker = init
            .get_arguments()
            .find(|argument| argument.get_id() == "tracker")
            .and_then(|argument| argument.get_help())
            .map(ToString::to_string)
            .expect("init --tracker has a help");
        assert_eq!(
            sentences(&tracker),
            [
                "The tracker to install: `json` writes the JSON backlog files in `backlog/`, `github` keeps the work items in the issues of a GitHub repository (`--repo`, else the `origin` remote), and `beads` keeps them in a Beads store that exists already: run `bd init` (or `bd bootstrap` in a clone) first, or `init` refuses",
                "Every tracker also writes the `backlogId` pattern of the deliverables schema for the adapter, and brings the seeded CI workflow of the install, where it has one, to the form of the adapter: `json` keeps the plain form, `github` adds the token and the read of the issues to the steps that run the gates, and `beads` adds the steps that install `bd` and bootstrap the store",
                "An install keeps its tracker: `init` with another tracker refuses, and with the same one changes nothing for the tracker",
                "`github` and `beads` refuse an install that has a `backlog/` directory",
            ],
            "the sentences of the init --tracker help"
        );
        let claude = init
            .get_arguments()
            .find(|argument| argument.get_id() == "target")
            .and_then(|argument| {
                argument
                    .get_possible_values()
                    .into_iter()
                    .find(|value| value.get_name() == "claude")
            })
            .and_then(|value| value.get_help().map(ToString::to_string))
            .expect("the claude target has a help");
        assert_eq!(
            claude,
            "The Claude Code files: the session-start script and its hook entries in `.claude/settings.json`",
            "`--target claude` installs no agent template (spec 4.4)"
        );
    }

    /// The sentences of a help text, each without its period: clap drops the
    /// final period of a help, so the last sentence has none to strip.
    fn sentences(text: &str) -> Vec<&str> {
        text.split(". ").collect()
    }
}
