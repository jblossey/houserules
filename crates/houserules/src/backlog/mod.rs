//! The JSON backlog: `backlog/*.json` items, batches, and checks.
//!
//! Owns the files and the code of the 1.4.0 backlog. The JSON adapter of the
//! tracker contract (`crate::tracker`) wraps this module and prints what it
//! printed; the `list`, `batch`, `set`, `defer`, and `check-backlog` commands
//! call the contract, never these functions directly. The module splits
//! three ways to match `rules::render`'s own load/logic/CLI split: `load`
//! (`load_backlog`), `commands` (`check_backlog`/`get_items`/`list_items`/
//! `batch_record`/`set_item`) with `defer` (`defer`), and `cli` (the
//! `ExitCode`-returning wrappers `main.rs` dispatches to). `load`/`commands`
//! operate on raw `serde_json::Value`, not `model`'s typed structs -- see
//! `load`'s module doc for why. `test_support` (test-only) holds the fixture
//! builders `load`'s and `commands`' own test modules share.
//!
//! `get` dispatches from the crate root (`crate::get`, beside
//! `crate::emit`) rather than from `cli`: the flat surface's `get` routes an
//! id between the knowledge base and the tracker, so it cannot live inside
//! either feature module without that module depending on the other, which
//! the spec's modular-install boundary forbids (`emit.rs`'s own doc has the
//! fuller account of that boundary).

mod cli;
mod commands;
mod defer;
mod load;
mod model;
#[cfg(test)]
mod test_support;

pub(crate) use cli::{cmd_batch, cmd_check_backlog, cmd_defer, cmd_list, cmd_set};
pub(crate) use commands::{
    CommandError, batch_record, check_backlog, get_items, list_items, set_item,
};
pub(crate) use defer::defer;
pub(crate) use load::{LoadedBacklog, load_backlog};
pub(crate) use model::ItemStatus;
