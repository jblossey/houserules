//! The one shared JSON output serializer, `emit` --
//! `template/tools/lib/json-store.mjs`'s own `emit` (`` `${JSON.stringify(
//! value, null, 2)}\n` ``), ported byte-for-byte. Lives at the crate root
//! and is imported by both `rules` and `backlog` (fix round 1, issue 8,
//! task-3-review.json): each module carrying its own copy risked exactly
//! the drift DRY names, in a crate whose whole contract is byte parity --
//! a later change to one copy would silently split `houserules audit`'s
//! output format from `houserules list`'s. A crate-root module compiles
//! into every configuration and gates nothing, so it does not compromise
//! the spec §3 modular-install boundary between `rules` and `backlog`
//! either: that boundary is about the two feature modules not depending on
//! EACH OTHER, not about a shared leaf utility neither owns.

use serde_json::Value;

/// Serializes `value` as indented JSON with a trailing newline --
/// `template/tools/lib/json-store.mjs`'s `emit` (`` `${JSON.stringify(value,
/// null, 2)}\n` ``), the CLI's only output format and, unchanged, the
/// format `backlog::commands::set_item` writes back to a rewritten items
/// file. `serde_json`'s default pretty-printer (two-space indent, one
/// element or property per line, no space before a comma, one space after
/// a colon) matches `JSON.stringify`'s own `, null, 2` output byte-for-byte
/// on every corpus sample this port's parity tests exercise, including
/// non-ASCII text and nested empty arrays/objects.
///
/// The one known boundary (task-2-review.json, issue 7): a JSON number's
/// own on-disk form re-renders differently through the two engines.
/// `serde_json::Number` keeps a non-integer or an integer past `2^53`
/// exactly (`2.0` stays `2.0`; `12345678901234567890`, which fits `u64`,
/// stays exact); `JSON.stringify` always re-renders a JS `Number` (an
/// `f64`), so the same values print as `2` and a rounded
/// `12345678901234567000`. `backlog/schema.json`'s only numeric item field,
/// `batch`, is typed `integer` with `minimum: 1` and every object sets
/// `additionalProperties: false`, so schema-valid data cannot reach this
/// boundary -- but a hand-edited file is not guaranteed schema-valid before
/// `set` touches it (`backlog::commands`'s own
/// `set_preserves_a_pre_existing_items_own_number_form` test pins Rust's
/// chosen output for both shapes).
pub(crate) fn emit(value: &Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("a JSON Value always serializes")
    )
}
