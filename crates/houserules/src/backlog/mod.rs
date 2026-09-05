//! The backlog surface: `backlog/*.json` items, batches, and checks.
//!
//! Owns everything the spec assigns to the `backlog` module boundary
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3): backlog items,
//! batch grouping, and the `list`, `get`, `set`, `batch`, and
//! `check-backlog` commands. Batch 17 T1 added the serde model layer for
//! `backlog/schema.json` (`model`, pinned by a build test in
//! `crate::schema_pin`); batch 17 T2 wires the command surface itself
//! (spec §5 phase 2) across three files, matching `rules::render`'s own
//! load/logic/CLI split: `load` (`loadBacklog`, ported), `commands`
//! (`checkBacklog`/`cmdGet`/`cmdList`/`cmdBatch`/`cmdSet`, ported), and
//! `cli` (the `ExitCode`-returning wrappers `main.rs` dispatches to).
//! `load`/`commands` operate on raw `serde_json::Value`, not `model`'s
//! typed structs -- see `load`'s module doc for why. `test_support`
//! (test-only) holds the fixture builders `load`'s and `commands`' own
//! test modules share.
//!
//! Batch 17 T4 moves `get`'s own dispatch out of `cli` and to the crate
//! root (`crate::get`, beside `crate::emit`): the flat surface's `get`
//! resolves an id by shape between a backlog item and a knowledge entry
//! (spec §3), so it cannot live inside either feature module without that
//! module depending on the other, which the spec's modular-install
//! boundary forbids (`emit.rs`'s own doc has the fuller account of that
//! boundary). `LoadedBacklog`/`load_backlog`/`get_items` are re-exported
//! here for that one caller, which reads `get_items`'s `CommandError` only
//! through its public `.0` field, never by name.

mod cli;
mod commands;
mod load;
mod model;
#[cfg(test)]
mod test_support;

pub(crate) use cli::{cmd_batch, cmd_check_backlog, cmd_list, cmd_set};
pub(crate) use commands::{ListOpts, get_items};
pub(crate) use load::{LoadedBacklog, load_backlog};
