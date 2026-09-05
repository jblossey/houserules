//! The backlog model layer: `backlog/schema.json` typed
//! (docs/specs/2026-09-04-batch-15-tier2-spec.md §3). Every `$defs` entry
//! the schema declares gets a serde type here, named after the schema's own
//! field names -- but only where the spec §3 data-layer rule (controller-
//! accepted at the batch 17 T2 review, HR-059) actually sanctions one: a
//! typed model serves only a path where the data is never re-serialized
//! back to its source file and a parse failure is an acceptable outcome. A
//! type with no such consumer is not kept dormant; it is deleted with its
//! pin test, which is why this file holds one type, not the fifteen `$defs`
//! entries `backlog/schema.json` declares.
//!
//! `ItemStatus` (`$defs/status`) is `backlog::commands::set_item`'s one
//! caller: it validates a `set status=<value>` assignment against this
//! schema-pinned enum instead of a hand-typed string array that could drift
//! from `backlog/schema.json` unnoticed, and a rejected `set` never writes
//! anything back, so a parse failure here is the correct, final outcome.
//! Every other `$defs` entry (`item`, `batch`, `amendment`, `decision`,
//! `parkedItem`, and their file wrappers) named a type here through batch
//! 17 T2's first cut and back it out in this fix round (task-2-review.json,
//! issue 5): `backlog::commands::get_items`/`set_item` must reproduce each
//! item's own on-disk key order byte-for-byte, which a struct's fixed
//! declaration order cannot (verified against this repository's own
//! `HR-052`, whose real field order does not match any type's declared
//! one), and `backlog::commands::check_backlog` must tolerate a malformed
//! item -- an invalid `type`, an unknown field -- the way `rules::model`'s
//! loader tolerates one for the knowledge base, which a
//! `#[serde(deny_unknown_fields)]` round-trip cannot; see `load.rs`'s
//! module doc for the fuller account. Neither constraint applies to
//! `ItemStatus`: `set_item` reads it as a bare string, never round-trips a
//! whole `Item`, and correctly refuses malformed input.

use serde::{Deserialize, Serialize};

/// `$defs/status`: a backlog item's lifecycle state, lowercase in the
/// schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ItemStatus {
    Open,
    Partial,
    Done,
    Dropped,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::Value;

    use super::*;
    use crate::schema_pin::assert_enum_pinned;

    /// `template/backlog/schema.json` -- the vendored backlog schema (spec
    /// §3: "the JSON Schema files stay the vendored source of truth").
    /// Differs from this repository's own copy (`backlog/schema.json`)
    /// only in the item-id regex prefix (`WI-` vendored, `HR-` this
    /// repository's own), which the enum pin below does not read.
    fn schema() -> Value {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/backlog/schema.json");
        serde_json::from_str(&fs::read_to_string(path).expect("read the vendored backlog schema"))
            .expect("parse the vendored backlog schema")
    }

    #[test]
    fn item_status_is_pinned() {
        assert_enum_pinned(
            &schema(),
            "/$defs/status",
            &[
                ItemStatus::Open,
                ItemStatus::Partial,
                ItemStatus::Done,
                ItemStatus::Dropped,
            ],
        );
    }
}
