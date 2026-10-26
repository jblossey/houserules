//! The per-change review layer: the change analysis that the review commands
//! share, the intent adapters, and one submodule per command. Each command
//! keeps its arguments in its own module. The `intent-required` and
//! `intent-resolves` checks of `rules` read the change analysis (`change`), the
//! trailers of a range (`links`), and the version gate of the analysis (`git`).

pub(crate) mod bundle;
pub(crate) mod change;
pub(crate) mod ci;
mod diff;
pub(crate) mod evidence;
pub(crate) mod git;
pub(crate) mod intent;
pub(crate) mod links;
pub(crate) mod record;
pub(crate) mod route;
mod rules_hash;
pub(crate) mod text;
mod validation;
