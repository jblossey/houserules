//! The backlog surface: `backlog/*.json` items, batches, and checks.
//!
//! Owns everything the spec assigns to the `backlog` module boundary
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3): backlog items,
//! batch grouping, and the `list`, `get`, `set`, `batch`, and
//! `check-backlog` commands. Gains its first code in Tier-2 phase 2 (spec
//! §5).
