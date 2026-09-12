//! The knowledge-base surface: `knowledge/*.json`, rendering, the
//! knowledge checks, and the deliverable-facing commands (`audit`,
//! `validate`, `stats`).
//!
//! Owns reading and validating knowledge entries, rendering
//! `.claude/rules/standing-rules.md` and the `project-knowledge` skill,
//! `audit`, `validate`, `stats`, and the commands that read the
//! generated files: the glob union matcher (`glob`), the knowledge
//! loading `render` needs (`model`), `render_all`/`render` (`render`),
//! `check_base` and `check-knowledge` (`check`), and `validate`/`stats`
//! (`validate_deliverable`, `stats`) on tolerant `serde_json::Value`
//! reads (see `deliverable.rs`'s and `validate_deliverable.rs`'s own
//! module docs for the data-layer reasoning). `check_shape::CheckDef`
//! wires into `model::Entry.check` and the audit engine (`audit`).
//!
//! `check::validate`, the generic JSON-Schema-subset engine `check_base`
//! already relies on, is re-exported as this module's own `validate`:
//! `crate::schema_pin`'s build tests use it as their oracle instead of
//! duplicating schema semantics, `backlog::commands::check_backlog` is a
//! second production caller, and `validate_deliverable` a third.
//! `render::repo_root_from_cwd` is re-exported the same way and for the
//! same reason: the `backlog` module's CLI wrappers, and `audit`'s,
//! `validate`'s, and `stats`'s, all need the identical `--dir`-or-git-root
//! resolution `render`/`check-knowledge` already use. `glob::areas_for`
//! wires into `for` (`glob.rs`'s own doc has the full account).
//! `read::get_entries` and `model::{Base, load_base}` are re-exported
//! too, for the crate-root `get` command (`crate::get`) that spans this
//! module and `backlog` both -- see that file's own doc for why the flat
//! surface's `get` cannot live in either feature module. `check_commit`
//! reuses `audit`'s own `CommitsCheck`, `rev`, and `commits_in` for its
//! per-commit evaluation and range reading rather than duplicating them,
//! so it and `audit` can never independently drift on what counts as a
//! `commits`-type violation (`check_commit.rs`'s own module doc has the
//! command's full account, including the two real call sites -- the
//! commit-msg hook, CI's commitlint job -- its CLI shape is derived
//! from).
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
//! (`model::Entry.check`, `audit::run_check`).

mod audit;
mod check;
mod check_commit;
mod check_shape;
mod deliverable;
mod glob;
mod model;
mod read;
mod render;
mod stats;
mod validate_deliverable;

pub(crate) use audit::cmd_audit;
pub(crate) use check::{cmd_check_knowledge, validate};
pub(crate) use check_commit::cmd_check_commit;
pub(crate) use model::{Base, load_base};
pub(crate) use read::{IndexOpts, cmd_for, cmd_index, cmd_standing, cmd_topics, get_entries};
pub(crate) use render::{cmd_render, render_and_report, repo_root_from_cwd};
pub(crate) use stats::cmd_stats;
pub(crate) use validate_deliverable::cmd_validate;
