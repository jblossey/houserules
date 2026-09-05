//! The knowledge-check-shape model: `knowledge/schema.json`'s `$defs/check`
//! typed, for the audit engine T3 ports (`runCheck`, `tools/kb.mjs:530-670`
//! at the frozen sha) to match on instead of reading raw `serde_json::Value`
//! fields by name. `check.rs`'s existing `check_shape`/`check_fields`
//! functions are a different concern and stay as they are: they validate a
//! knowledge entry's `check` object for `check-knowledge` (dynamic,
//! per-`type` field requirements the JSON schema itself does not encode --
//! its own `required` is only `[type, level]`), while `CheckDef` here
//! mirrors that JSON schema definition exactly, matching what the
//! schema-pin build test below can actually pin.
//!
//! Batch 17 T3 wires `CheckDef` into `rules::model::Entry.check` and the
//! `audit` engine's `run_check` (docs/specs/2026-09-04-batch-15-tier2-spec.md
//! §5 phase 2), so every type here now has a real caller and the file-level
//! `#[allow(dead_code)]` this module carried through T1/T2 is dropped.

use serde::{Deserialize, Serialize};

/// A JSON Schema `["string", "array"]`-of-strings field -- `knowledge/
/// schema.json`'s `$defs/glob`, which `check`'s `files`, `if`, and `then`
/// properties all reference. A knowledge entry may glob-match one path or
/// several, and the JSON is written either way (a bare string is the
/// common case; an array only when more than one glob is needed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum Glob {
    One(String),
    Many(Vec<String>),
}

/// A knowledge entry's `check.type` -- which deterministic check `runCheck`
/// runs, and which of `CheckDef`'s other fields it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CheckType {
    GrepAbsent,
    Commits,
    CoChange,
    DiffAppendOnly,
    ReportField,
}

/// A knowledge entry's `check.level`: whether a `fail` result blocks an
/// audit or only a `warn` one is noted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CheckLevel {
    Fail,
    Warn,
}

/// A `grep-absent` check's search pool: every file in the head tree
/// (`tree`), or only files the audited range changed (`changed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scope {
    Changed,
    Tree,
}

/// One knowledge entry's `check` object, typed -- `knowledge/schema.json`'s
/// `$defs/check`. Only `kind` (JSON key `type`) and `level` are required by
/// the schema; every other field is `Option` because the schema's own
/// `required` is `[type, level]` -- which fields a given `kind` actually
/// needs is `check.rs`'s `check_fields`/`check_shape` business rule, not a
/// constraint this schema-shaped struct encodes (see the module doc).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckDef {
    #[serde(rename = "type")]
    pub kind: CheckType,
    pub level: CheckLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Glob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_absent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_line_max: Option<u64>,
    #[serde(rename = "if", default, skip_serializing_if = "Option::is_none")]
    pub if_changed: Option<Glob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<Glob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::{CheckDef, CheckLevel, CheckType, Scope};
    use crate::schema_pin::{assert_enum_pinned, assert_object_pinned};

    /// `template/knowledge/schema.json` -- the vendored knowledge schema
    /// (spec §3: "the JSON Schema files stay the vendored source of
    /// truth"), the same file `check.rs`'s own tests read (`template_root`).
    fn schema() -> Value {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json");
        serde_json::from_str(&fs::read_to_string(path).expect("read the vendored knowledge schema"))
            .expect("parse the vendored knowledge schema")
    }

    /// Every field `CheckDef` declares, populated -- a `report-field` check
    /// exercises `field` alongside `if`, keeping the sample to one shape
    /// while still touching every property `$defs/check` declares (`files`,
    /// `pattern`, `flags`, `scope`, `subject`, `body_absent`,
    /// `body_line_max`, `if`, `then`, `field`, plus `type`/`level`).
    fn full_sample() -> Value {
        json!({
            "type": "report-field",
            "level": "fail",
            "files": ["knowledge/*.json"],
            "pattern": "unresolved-marker",
            "flags": "i",
            "scope": "changed",
            "subject": "^fix",
            "body_absent": "wip",
            "body_line_max": 100,
            "if": ["**/*.rs"],
            "then": ["**/*.rs"],
            "field": "concerns",
        })
    }

    #[test]
    fn check_def_is_pinned_to_the_vendored_schema() {
        assert_object_pinned::<CheckDef>(&schema(), "/$defs/check", &full_sample());
    }

    #[test]
    fn check_type_is_pinned_to_the_vendored_schema() {
        assert_enum_pinned(
            &schema(),
            "/$defs/check/properties/type",
            &[
                CheckType::GrepAbsent,
                CheckType::Commits,
                CheckType::CoChange,
                CheckType::DiffAppendOnly,
                CheckType::ReportField,
            ],
        );
    }

    #[test]
    fn check_level_is_pinned_to_the_vendored_schema() {
        assert_enum_pinned(
            &schema(),
            "/$defs/check/properties/level",
            &[CheckLevel::Fail, CheckLevel::Warn],
        );
    }

    #[test]
    fn scope_is_pinned_to_the_vendored_schema() {
        assert_enum_pinned(
            &schema(),
            "/$defs/check/properties/scope",
            &[Scope::Changed, Scope::Tree],
        );
    }

    /// A single-glob `files`/`if`/`then` value is the common case in this
    /// repository's own `knowledge/*.json` -- proving `Glob` accepts it,
    /// not only the array form the schema-pin sample above exercises.
    #[test]
    fn glob_accepts_a_bare_string_as_well_as_an_array() {
        let single = json!({
            "type": "grep-absent",
            "level": "fail",
            "files": "knowledge/**",
            "pattern": "unresolved-marker",
            "scope": "tree",
        });
        let parsed: CheckDef = serde_json::from_value(single).expect("single-string files");
        assert_eq!(
            parsed.files,
            Some(super::Glob::One("knowledge/**".to_string()))
        );
    }
}
