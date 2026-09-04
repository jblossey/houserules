//! The knowledge-base surface: `knowledge/*.json`, rendering, and the
//! knowledge checks.
//!
//! Owns everything the spec assigns to the `rules` module boundary
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3): reading and
//! validating knowledge entries, rendering `.claude/rules/standing-rules.md`
//! and the `project-knowledge` skill, `audit`, `validate`, and the commands
//! that read the generated files. The render port (spec §5 phase 1, HR-054
//! task 3) landed the glob union matcher (`glob`), the knowledge loading
//! `render` needs (`model`), and `renderAll`/`render` (`render`). The check
//! port (HR-054 task 4) adds `checkBase` and `check-knowledge` (`check`).

mod check;
mod glob;
mod model;
mod render;

pub(crate) use check::cmd_check_knowledge;
pub(crate) use render::cmd_render;
