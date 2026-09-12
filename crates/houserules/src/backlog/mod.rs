//! The backlog surface: `backlog/*.json` items, batches, and checks.
//!
//! Owns backlog items, batch grouping, and the `list`, `set`, `batch`,
//! and `check-backlog` commands, split three ways to match
//! `rules::render`'s own load/logic/CLI split: `load` (`load_backlog`),
//! `commands` (`check_backlog`/`get_items`/`list_items`/`batch_record`/
//! `set_item`), and `cli` (the `ExitCode`-returning wrappers `main.rs`
//! dispatches to). `load`/`commands` operate on raw `serde_json::Value`,
//! not `model`'s typed structs -- see `load`'s module doc for why.
//! `test_support` (test-only) holds the fixture builders `load`'s and
//! `commands`' own test modules share.
//!
//! `get` dispatches from the crate root (`crate::get`, beside
//! `crate::emit`) rather than from `cli`: the flat surface's `get`
//! resolves an id by shape between a backlog item and a knowledge entry,
//! so it cannot live inside either feature module without that module
//! depending on the other, which the spec's modular-install boundary
//! forbids (`emit.rs`'s own doc has the fuller account of that boundary).
//! `LoadedBacklog`/`load_backlog`/`get_items` are re-exported here for
//! that one caller, which reads `get_items`'s `CommandError` only through
//! its public `.0` field, never by name.

mod cli;
mod commands;
mod load;
mod model;
#[cfg(test)]
mod test_support;

pub(crate) use cli::{cmd_batch, cmd_check_backlog, cmd_list, cmd_set};
pub(crate) use commands::{ListOpts, get_items};
pub(crate) use load::{LoadedBacklog, load_backlog};
