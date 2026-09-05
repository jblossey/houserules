//! The knowledge-base surface: `knowledge/*.json`, rendering, the
//! knowledge checks, and the deliverable-facing commands (`audit`,
//! `validate`, `stats`).
//!
//! Owns everything the spec assigns to the `rules` module boundary
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3): reading and
//! validating knowledge entries, rendering `.claude/rules/standing-rules.md`
//! and the `project-knowledge` skill, `audit`, `validate`, and the commands
//! that read the generated files. The render port (spec §5 phase 1, HR-054
//! task 3) landed the glob union matcher (`glob`), the knowledge loading
//! `render` needs (`model`), and `renderAll`/`render` (`render`). The check
//! port (HR-054 task 4) adds `checkBase` and `check-knowledge` (`check`).
//! Batch 17 T1 added the deliverables and knowledge-check-shape serde model
//! layers; batch 17 T3 (spec §5 phase 2) wires `check_shape::CheckDef` into
//! `model::Entry.check` and the audit engine (`audit`), and ports
//! `validate`/`stats` (`validate_deliverable`, `stats`) on tolerant
//! `serde_json::Value` reads (see `deliverable.rs`'s and
//! `validate_deliverable.rs`'s own module docs for the data-layer
//! reasoning). `check::validate`, the generic JSON-Schema-subset engine
//! `checkBase` already relies on, is re-exported as this module's own
//! `validate` -- originally test-only, for `crate::schema_pin`'s build
//! tests to reuse as their oracle instead of duplicating schema semantics;
//! batch 17 T2 widened it to every build, because
//! `backlog::commands::check_backlog` is now a second production caller,
//! and batch 17 T3 adds `validate_deliverable` as a third.
//! `render::repo_root_from_cwd` is re-exported the same way and for the
//! same reason: the `backlog` module's CLI wrappers, and now `audit`'s,
//! `validate`'s, and `stats`'s, all need the identical `--dir`-or-git-root
//! resolution `render`/`check-knowledge` already use. Batch 17 T4 adds the
//! knowledge read commands (`topics`/`index`/`for`/`standing`, `read`) and
//! wires `glob::areas_for` into `for`, dropping its `#[allow(dead_code)]`
//! (`glob.rs`'s own doc has the full account). `read::get_entries` and
//! `model::{Base, load_base}` are re-exported too, for the crate-root `get`
//! command (`crate::get`) that spans this module and `backlog` both -- see
//! that file's own doc for why the flat surface's `get` cannot live in
//! either feature module.
//!
//! `rules::deliverables` and `crate::json_shape` (batch 17 T1's typed
//! deliverables-schema model layer) are DELETED in the same commit that
//! wires T3's three surfaces, not kept dormant: the spec §3 data-layer rule
//! ("model types with no consumer under this rule are deleted with their
//! pin tests, not kept dormant") and the HR-059 backlog item explicitly
//! deferred this judgment to T3 ("rules/deliverables.rs and check_shape.rs
//! stay for T3's aggregating readers, judged per the same rule at T3").
//! `validate_deliverable` validates every deliverable kind through the
//! generic schema engine directly (never a typed parse, per its own module
//! doc); `stats` aggregates through tolerant `Value` reads for the same
//! reason `deliverable.rs` documents; and `audit`'s own JSON output is not
//! itself one of the four schema-defined deliverable kinds at all (its
//! judged rows carry `result: "open"`, a value the schema's `auditRow.result`
//! enum forbids -- `audit.rs`'s own module doc has the full account). Across
//! all three surfaces, no command in this binary ever constructs or
//! strictly parses a schema-exact deliverable, so no consumer exists for
//! `rules::deliverables`'s ~30 types, and `crate::json_shape`'s
//! `RequiredNullable`/`deserialize_optional_nullable` (that file's only
//! consumer) fall with it. `check_shape.rs`'s `CheckDef` is the one model
//! layer that DOES get a real, direct consumer (`model::Entry.check`,
//! `audit::run_check`) and stays, its own file-level allow dropped.

mod audit;
mod check;
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
pub(crate) use model::{Base, load_base};
pub(crate) use read::{IndexOpts, cmd_for, cmd_index, cmd_standing, cmd_topics, get_entries};
pub(crate) use render::{cmd_render, repo_root_from_cwd};
pub(crate) use stats::cmd_stats;
pub(crate) use validate_deliverable::cmd_validate;
