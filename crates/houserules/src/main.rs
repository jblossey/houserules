//! Entry point for the `houserules` binary.
//!
//! Parses the command line and dispatches to the crate's modules.
//! `backlog` and `rules` hold the backlog and knowledge command surfaces;
//! `get` (`crate::get::cmd_get`) unifies `get` to resolve by id shape
//! between a backlog item and a knowledge entry, rather than either
//! module handling it directly (see that module's doc for why).
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
//! checker (`report_claims.rs`'s own module doc has the full account).
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

mod archive;
mod backlog;
mod baseline;
mod emit;
mod get;
mod install;
mod node_path;
mod pack;
mod packs;
mod report_claims;
mod root;
mod rules;
#[cfg(test)]
mod schema_pin;
mod selfupdate;
mod update_check;

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
    /// Checks that every id a kit-owned file cites from a topic the kit seeds
    /// is an entry of the knowledge base. Prints each override and exclusion
    /// on stdout.
    CheckKnowledge {
        /// Install root to check; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Prints one or more items by id, each resolved by its own shape: a
    /// backlog item (`<idPrefix>-\d{3}`, the project's own stamped
    /// prefix), an amendment or parked item (`A-\d{2}`, `PP-\d+-\d{2}`,
    /// both kit-fixed regardless of idPrefix), or otherwise a knowledge
    /// entry. A pack entry with an override prints as its effective entry,
    /// with an `overridden` field that lists the replaced fields. An entry
    /// that `knowledge/overrides.json` excludes prints too, with an
    /// `excluded` field that holds the reason, or `true` when there is
    /// none. A pack entry with status `retired` or `superseded` prints,
    /// which the other commands leave out.
    Get {
        /// Backlog and/or knowledge ids to print, in any mix.
        ids: Vec<String>,
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
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
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Prints one development batch's summary and item rows.
    Batch {
        /// The batch number (exactly one).
        numbers: Vec<String>,
        /// Install root to read from; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Applies `field=value` assignments to a backlog item and rewrites its file.
    Set {
        /// The item id, followed by one or more `field=value` assignments.
        args: Vec<String>,
        /// Install root to write into; defaults to the nearest directory at
        /// or above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Validates the backlog: schema, cross-file invariants, and compression
    /// markers that a context proxy left in a backlog file.
    CheckBacklog {
        /// Install root to check; defaults to the nearest directory at or
        /// above the current one that holds `.houserules.json`, no higher
        /// than the git top level, else the git top level.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Builds the rule package for a git range and runs every member's
    /// deterministic check. Every path it matches is relative to the install
    /// root; a changed path outside the install root is out of scope.
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
        /// With `--report` (a task audit), an entry tagged `controller`
        /// joins the package only when `--ids` names it.
        #[arg(long)]
        report: Option<PathBuf>,
        /// A directory of `task-*-report.json` files a `report-field`
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
        /// is reported in `stale_sanctions`, not silently accepted. A rule
        /// outside the audit's package is an error: add it to `--ids`.
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
    /// that a context proxy left in a file.
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
    /// Aggregates rule violations, unused injected ids, process cost, and
    /// knowledge-base proposals across one or more workspace directories'
    /// JSON deliverables. Each `rules` row names its `source`: `local`, or
    /// the name of the pack that ships the entry. The `packs` key sums
    /// entries, standing entries, judged rows, and violations for `local`
    /// and for each installed pack.
    Stats {
        /// One or more workspace directories to aggregate.
        #[arg(required = true, value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
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
    /// touched the install.
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
    /// install, the five kit packs with their pins in `.houserules.json`.
    /// Without `--target claude` it leaves out the Claude Code files and the
    /// hook entry, and prints the line that enables them. Running it again
    /// seeds what is missing and keeps what exists. On an existing install it
    /// installs no pack and adds no area: only the `pack` commands and
    /// `update` change the pins. It never seeds a path that `ownedPaths` in
    /// `.houserules.json` lists; the files that `render` generates follow the
    /// knowledge base, not `ownedPaths`. It refuses an install whose schema is
    /// newer than the binary writes, with a line that names the release it
    /// needs, and an install whose schema is no install schema id, with a line
    /// that names the value; it changes nothing then.
    Init {
        /// The install root; defaults to the current directory. Unlike
        /// every read command's `--dir` above, this is not resolved against
        /// an enclosing install -- the directory itself is the root. It must
        /// exist and lie in a git repository: the top level, or a directory
        /// below it for a subdirectory install, which writes its CI workflow
        /// at the top level as `.github/workflows/knowledge-<folder>.yml`.
        /// It is refused when it nests with another install of the
        /// repository or would write another install's workflow file.
        #[arg(long)]
        dir: Option<PathBuf>,
        /// The backlog id prefix seeded schemas carry; defaults to `WI`.
        #[arg(long)]
        id_prefix: Option<String>,
        /// A render target to enable, in the install root. On an existing
        /// install it adds the target to `renderers`, seeds the files of the
        /// target that are missing, keeps every file that exists, and
        /// renders. For `claude`, it adds the hook entry to an existing
        /// `.claude/settings.json` and keeps the other keys. A
        /// `.claude/settings.json` that `ownedPaths` in `.houserules.json`
        /// lists gets no hook entry.
        #[arg(long, value_enum)]
        target: Option<install::InitTarget>,
    },
    /// Prints the file classes of the kit as JSON: the kit-owned files, the
    /// seed-once files, the copies of the kit packs, and the D3 files, which
    /// `init` seeds and `update` keeps current through a baseline. Each path
    /// is in one class. The CI workflow is a D3 file that only a GitHub-hosted
    /// target gets.
    Files,
    /// Self-updates the installed binary first, where the install channel
    /// supports it (`HOUSERULES_SKIP_SELF_UPDATE` opts out), then updates the
    /// install. After the self-update and before any write, it refuses an
    /// install whose schema is newer than the binary writes, with a line that
    /// names the release it needs, and an install whose schema is no install
    /// schema id, with a line that names the value. An install of houserules
    /// 1.x is converted to the 2.0 layout:
    /// the seeded entries become the five kit packs in `knowledge/packs/`,
    /// the `overrides` of `.houserules.json` become `ownedPaths` and
    /// `knowledge/overrides.json`, and the `schema` key is written last. A
    /// topic file `knowledge/overrides.json` of 1.x stops the conversion
    /// before any write, and so does a `knowledge/` directory that does not
    /// exist or is empty: restore the directory from version control and run
    /// `update` again, or remove `.houserules.json` and run `init --id-prefix
    /// <prefix>` for a fresh install, with the `idPrefix` of the marker. A
    /// `knowledge/areas.json` that `ownedPaths` lists and that does not exist
    /// stops it too. On a 2.0 install it moves the pins of the kit packs
    /// to the versions of the binary and turns a local entry with a pack id
    /// into an override. Then it syncs the kit-owned files of an
    /// already-`init`ed target from the embedded payload: it writes a missing
    /// file, or one that the project has not changed, and lists it only when
    /// its bytes change; it keeps and lists a file the project changed
    /// (`kept <file> (locally modified)`); it never syncs a path that
    /// `ownedPaths` in `.houserules.json` lists. While `renderers` lacks
    /// `claude`, it skips the Claude Code files. It deletes any retired kit
    /// file still present, prints each hand step that the project has left
    /// (`hand step: ...`), and reports the stamped-to-running version drift.
    /// Every other command that reads the install refuses an install of 1.x
    /// with one line that names this command.
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
    /// Cross-checks one deliverable report's claims against the artifacts
    /// and git history it cites: redirected captures, truncation markers,
    /// the natural label of tdd entries (RED capture time against commit
    /// time), the self-audit head against the newest listed commit, commit
    /// shas in the narrative, self-audit pass ratios in the narrative, the
    /// bounded no-execution paste-run lint over every captured command
    /// field, ephemeral paths in the narrative, line numbers of cited
    /// files, counts beside a cited capture, and a cited sweep against the
    /// files the report changed.
    CheckReportClaims {
        /// The report file to check.
        report_path: PathBuf,
        /// Install root the report's cited artifacts and commit shas resolve
        /// against; defaults to the nearest directory at or above the current
        /// one that holds `.houserules.json`, no higher than the git top
        /// level, else the git top level. Independent of `report_path`,
        /// which always resolves against the real current directory.
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Moves done/dropped backlog items, done batch entries, and
    /// superseded/retired knowledge entries into their `archive/` mirrors.
    /// It moves local knowledge entries only: a pack entry with status
    /// `retired` or `superseded` stays in its pack copy, whether the pack
    /// ships the status or an override sets it.
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
/// root, or `None` for the two commands that `main` leaves out: `files` reads
/// no install and runs on any tree, and `update` runs D4 itself after its
/// self-update phase (`install::cmd_update`, spec C22). `init` refuses a 1.x
/// install with its own line, so it runs on a tree with no install schema or
/// an older one (`Guard::ConvertsInstall`); every other command reads the
/// install (`Guard::ReadsInstall`). The match names every variant, so a new
/// command cannot skip D4 unseen.
fn d4_guard(command: &Command) -> Option<(install::Guard, &Option<PathBuf>)> {
    use install::Guard::{ConvertsInstall, ReadsInstall};
    match command {
        Command::Files | Command::Update { .. } => None,
        Command::Init { dir, .. } => Some((ConvertsInstall, dir)),
        Command::Render { dir, .. }
        | Command::CheckKnowledge { dir }
        | Command::Get { dir, .. }
        | Command::List { dir, .. }
        | Command::Batch { dir, .. }
        | Command::Set { dir, .. }
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
        Some(Command::Stats { workspaces, dir }) => rules::cmd_stats(dir, workspaces),
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
        }) => install::cmd_init(dir, id_prefix, target),
        Some(Command::Files) => install::cmd_files(),
        Some(Command::Update { dir, id_prefix }) => install::cmd_update(dir, id_prefix),
        Some(Command::CheckReportClaims { report_path, dir }) => {
            report_claims::cmd_check_report_claims(dir, report_path)
        }
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
    /// 21 `Command` variants (clap's own built-in `help` subcommand is
    /// rendered separately and is not itself one of `get_subcommands()`'s
    /// entries).
    #[test]
    fn no_subcommand_carries_a_long_about_beyond_its_short_one() {
        let command = Cli::command();
        let subcommands: Vec<_> = command.get_subcommands().collect();
        assert_eq!(
            subcommands.len(),
            21,
            "expected the full subcommand surface (21 Command variants), got: {:?}",
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
    const HELP_CHECKS: [(&str, &str); 11] = [
        ("check_redirected_captures", "redirected captures"),
        ("check_truncation_markers", "truncation markers"),
        (
            "natural_red::check_natural_red_labels",
            "natural label of tdd entries",
        ),
        ("check_self_audit_head_is_current", "self-audit head"),
        (
            "check_narrative_shas_resolve",
            "commit shas in the narrative",
        ),
        ("check_self_audit_narrative", "pass ratios in the narrative"),
        ("check_paste_run_lint", "paste-run lint"),
        ("check_ephemeral_paths", "ephemeral paths"),
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
    /// about is read as a comma-separated list after its first `: `, in
    /// call order: item `i` must carry the phrase of pair `i`.
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
            .map(|(_, list)| list.trim_end_matches('.'))
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

    /// The CLI surface: exactly these 21 subcommand names, in `--help`'s own
    /// declaration order. The count assertion above cannot see a same-length
    /// rename, because a renamed variant keeps `get_subcommands().len()`;
    /// comparing the names themselves does.
    #[test]
    fn the_cli_surface_names_exactly_these_21_subcommands() {
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
                "check-report-claims",
                "archive",
                "pack",
            ],
            "the CLI surface (the 1.0 surface of design.md 5.82, then `pack`) changed"
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
    /// install gets the five kit packs, a second run seeds the missing files
    /// and keeps those that exist, an existing install gets no pack and no
    /// area, a path of `ownedPaths` is never seeded, and an install schema that
    /// is newer or malformed is refused. The update about says the same of its
    /// sync, because both commands end with `render`. `--target` says that
    /// a settings file of `ownedPaths` gets no hook entry. The test pins every
    /// sentence of both texts whole, in order, so a deleted, added, or
    /// reworded sentence fails it.
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
                "Installs the kit into a target git repository from the embedded payload: the kit-owned files, the seed-once files, and, on a new install, the five kit packs with their pins in `.houserules.json`",
                "Without `--target claude` it leaves out the Claude Code files and the hook entry, and prints the line that enables them",
                "Running it again seeds what is missing and keeps what exists",
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
                "For `claude`, it adds the hook entry to an existing `.claude/settings.json` and keeps the other keys",
                "A `.claude/settings.json` that `ownedPaths` in `.houserules.json` lists gets no hook entry",
            ],
            "the sentences of the init --target help"
        );
    }

    /// The sentences of a help text, each without its period: clap drops the
    /// final period of a help, so the last sentence has none to strip.
    fn sentences(text: &str) -> Vec<&str> {
        text.split(". ").collect()
    }
}
