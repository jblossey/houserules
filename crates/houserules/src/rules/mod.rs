//! The knowledge-base surface: `knowledge/*.json`, rendering, the
//! knowledge checks, and the deliverable-facing commands (`audit`,
//! `validate`, `stats`).
//!
//! Owns reading and validating knowledge entries, rendering the blocks of
//! the `AGENTS.md` files and, where the install enables the Claude
//! renderer, the area files `.claude/rules/<area>.md` and the
//! `project-knowledge` skill, `audit`, `validate`, `stats`, and the
//! commands that read the generated files: the glob union matcher
//! (`glob`), the knowledge loading `render` needs (`model`), the render
//! port and its two renderers (`render`), the size budgets of the generated
//! files (`budget`), `check_base` and `check-knowledge` (`check`), the
//! pack-layer checks that
//! `check_base` calls (`check_packs`), the lines about the `runners` and
//! `workspaces` keys of the marker and the tools that `runner` checks name
//! (`check_marker_keys`), the compression-marker gate that
//! `check-knowledge`, `check-backlog`, and `validate` share (`markers`), the
//! kit-owned citation lint `check-knowledge` also runs (`kit_citations`), and
//! `validate`/`stats` (`validate_deliverable`, `stats`) on tolerant
//! `serde_json::Value` reads (see `deliverable.rs`'s and
//! `validate_deliverable.rs`'s own
//! module docs for the data-layer reasoning). `check_shape::CheckDef`
//! wires into `model::Entry.check` and the audit engine (`audit`).
//! `stats` composes two more modules: `cost` (the measurements of one
//! workspace and the `workspaces` and `cost` keys) and `proposals` (the
//! `rules` and `proposals` keys, rated against the loaded knowledge base).
//! `test_support` (test builds only) holds the knowledge-base fixtures
//! those two modules' tests share.
//!
//! `check::validate`, the generic JSON-Schema-subset engine `check_base`
//! already relies on, is re-exported as this module's own `validate`:
//! `crate::schema_pin`'s build tests use it as their oracle instead of
//! duplicating schema semantics, `backlog::commands::check_backlog` is a
//! second production caller, and `validate_deliverable` a third.
//! `render::git_location_from_cwd` is re-exported the same way and for the
//! same reason: the `backlog` module's CLI wrappers, and `audit`'s,
//! `validate`'s, and `stats`'s, all need the identical `--dir`-or-install-root
//! resolution `render`/`check-knowledge` already use (`root::resolve_root`
//! reads the git location). `glob::areas_for`
//! wires into `for` (`glob.rs`'s own doc has the full account).
//! `read::get_entries` and `model::{Base, load_base}` are re-exported
//! too, for the crate-root `get` command (`crate::get`) that spans this
//! module and `backlog` both -- see that file's own doc for why the flat
//! surface's `get` cannot live in either feature module. `git` is the git
//! plumbing of a range (`rev`, `git_diff`, `commits_in`, and the other
//! readers), and `runner` holds one module per check type behind a row
//! builder (`runner::run`, which `audit` calls for each entry with a
//! check). `check_commit` reuses `runner`'s `CommitsCheck` and `git`'s
//! `rev` and `commits_in` for its per-commit evaluation and range reading
//! rather than duplicating them, so it and `audit` can never independently
//! drift on what counts as a `commits`-type violation (`check_commit.rs`'s
//! own module doc has the command's full account, including the two real
//! call sites -- the commit-msg hook, CI's commitlint job -- its CLI shape
//! is derived from).
//!
//! `trailers` reads the trailers of a commit message with git's own rule and
//! resolves the work-item references that `audit` collects (the `Work-Item:`
//! and `Deferred:` trailers of its range and the `issues[].backlog` ids of
//! the deliverables it reads) through the installed tracker
//! (`trailers.rs`'s own module doc has the account; spec 3.7).
//!
//! `git_paths` is the one place git's path lists become install-root
//! paths: `git`'s `changed_files` and `tree_files`, `check_commit`'s
//! per-commit file read, and the `files_changed` of `report-facts` all go
//! through `InstallPrefix::map`, so a subdirectory install matches the paths
//! it owns and drops the rest (`git_paths.rs`'s own doc has the account). It
//! is also the one module that runs `git ls-files`: the tracked files, the
//! files of one name, and the submodules of the index.
//!
//! `durable_sha` is `CheckType::DurableSha`'s own git-plumbing (a diff
//! hunk's added lines, a hex-candidate scan, and the two `git`
//! subprocesses that decide whether a candidate resolves to a commit
//! reachable from the check's own `--base`), reusing `git`'s own
//! `git_diff`/`range` rather than a second diff invocation
//! (`durable_sha.rs`'s own module doc has the full account; HR-120).
//!
//! No command in this binary ever constructs or strictly parses a
//! schema-exact deliverable: `validate_deliverable` validates every
//! deliverable kind through the generic schema engine directly (never a
//! typed parse, per its own module doc); `stats` aggregates through
//! tolerant `Value` reads for the same reason `deliverable.rs`
//! documents; and `audit`'s own JSON output is not itself one of the
//! four schema-defined deliverable kinds at all (its judged rows carry
//! `result: "open"`, a value the schema's `auditRow.result` enum forbids
//! -- `audit.rs`'s own module doc has the full account). `check_shape.rs`'s
//! `CheckDef` is the one model layer with a real, direct consumer
//! (`model::Entry.check`, `runner::run`).

mod audit;
mod budget;
mod check;
mod check_commit;
mod check_marker_keys;
mod check_packs;
mod check_shape;
mod cost;
mod deliverable;
mod durable_sha;
mod git;
mod git_paths;
mod glob;
mod ignored;
mod kit_citations;
mod markers;
mod model;
mod proposals;
mod read;
mod render;
mod runner;
mod stats;
#[cfg(test)]
mod test_support;
mod trailers;
mod validate_deliverable;

pub(crate) use audit::cmd_audit;
pub(crate) use check::{cmd_check_knowledge, skipped_path_line, tracked_tree_files, validate};
pub(crate) use check_commit::cmd_check_commit;
pub(crate) use check_packs::{imports_agents_md, pin_mismatch_line};
pub(crate) use git_paths::{
    InstallPrefix, ListingFailure, listed_by_name, submodule_paths, tracked_files,
};
pub(crate) use glob::compile;
pub(crate) use ignored::ignored_among;
pub(crate) use kit_citations::KitScope;
#[cfg(test)]
pub(crate) use kit_citations::{Scan, kit_citation_findings, scan_kit_citations};
pub(crate) use markers::marker_findings;
pub(crate) use model::{Base, RESERVED, load_base};
pub(crate) use read::{IndexOpts, cmd_for, cmd_index, cmd_standing, cmd_topics, get_entries};
pub(crate) use render::{GitLocation, cmd_render, git_location_from_cwd, render_and_report};
pub(crate) use stats::cmd_stats;
pub(crate) use validate_deliverable::cmd_validate;
