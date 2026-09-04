//! The install surface: bringing the kit into and up to date in a project
//! repository.
//!
//! Owns everything the spec assigns to the `install` module boundary
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3): `init`, `update`, and
//! `files`, including the KIT_OWNED sync and the vendored-file deletion
//! `update` gains for the no-shims migration. Gains its first code in
//! Tier-2 phase 3 (spec §5).
