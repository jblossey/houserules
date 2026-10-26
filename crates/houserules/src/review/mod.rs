//! The per-change review layer: the change analysis that the review commands
//! share, the intent adapters, and one submodule per command. Each command
//! keeps its arguments in its own module.

pub(crate) mod bundle;
mod change;
pub(crate) mod ci;
mod diff;
pub(crate) mod evidence;
mod git;
mod intent;
pub(crate) mod record;
pub(crate) mod route;
mod rules_hash;
mod text;
