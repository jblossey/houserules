//! The ruled lists of the kit-text guards (spec sections 5 and 6.4, ruling
//! 189). `pack_texts.rs` guards the packs and the core texts with them, and
//! `workflow_texts.rs` guards the reference workflow. A ruling that changes
//! a list changes it here, once. A guard that excludes a name from a list
//! names the exclusion and the hit that needs it in its own file.

/// The pattern that the full-tier script `fact-full-tier.py` of the batch
/// workspace matches (live case 21).
pub const FULL_TIER: &str = r"full[- ]tier|full criterion|\|full>|light, standard,? or full|neither light nor full|^\W*\x22?Full:|full 2\b|standard or full|is the full tier|tier[^.\n]{0,30}\bfull\b|\bfull\b[^.\n]{0,30}\btier";

/// The skill names of the superpowers plugin (spec 6.4's table, ruling 189)
/// that no kit text names, in lower case.
pub const PLUGIN_SKILLS: [&str; 15] = [
    "brainstorming",
    "writing-plans",
    "executing-plans",
    "subagent-driven-development",
    "dispatching-parallel-agents",
    "using-git-worktrees",
    "test-driven-development",
    "verification-before-completion",
    "requesting-code-review",
    "receiving-code-review",
    "finishing-a-development-branch",
    "using-superpowers",
    "systematic-debugging",
    "writing-skills",
    "diagnosing-superpowers",
];

/// The pattern for the vendor model names that a kit text names only beside
/// the default of a role (spec 5).
pub const VENDOR_MODELS: &str = r"\b(?:opus|sonnet|fable|haiku)\b";
