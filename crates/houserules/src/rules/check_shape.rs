//! The knowledge-check-shape model: `knowledge/schema.json`'s `$defs/check`
//! typed, for the audit engine's `runner::run` to match on instead of
//! reading raw `serde_json::Value` fields by name. `check.rs`'s existing
//! `check_shape`/`check_fields` functions are a different concern and
//! stay as they are: they validate a knowledge entry's `check` object
//! for `check-knowledge` (dynamic, per-`type` field requirements the
//! JSON schema itself does not encode -- its own `required` is only
//! `[type, level]`), while `CheckDef` here mirrors that JSON schema
//! definition exactly, matching what the schema-pin build test below can
//! actually pin.
//!
//! `CheckDef` wires into `rules::model::Entry.check` and the `audit`
//! engine's `runner::run`, so every type here has a real caller.

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

impl Glob {
    /// Every pattern of the glob, in the order of the entry.
    fn patterns(&self) -> Vec<String> {
        match self {
            Glob::One(pattern) => vec![pattern.clone()],
            Glob::Many(patterns) => patterns.clone(),
        }
    }
}

/// The `files` of a `runner` check that names none: every file.
pub(crate) const ALL_FILES: &str = "**";

/// A knowledge entry's `check.type` -- which deterministic check
/// `runner::run` runs, and which of `CheckDef`'s other fields it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CheckType {
    GrepAbsent,
    Commits,
    CoChange,
    DiffAppendOnly,
    ReportField,
    DurableSha,
    /// An external tool that the project defines in `.houserules.json`
    /// `runners`; `CheckDef::tool` names its key.
    Runner,
    /// A change to the paths of `if` needs an intent: an `Intent:` trailer or a
    /// changed OpenSpec change or plan file (batch 38, spec 5.5).
    IntentRequired,
    /// Each `Intent:` trailer of the range resolves at the head or at the merge
    /// base (batch 38, spec 5.5).
    IntentResolves,
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
    /// A `commits` check's required trailer key (`Work-Item`): a commit
    /// whose message has no trailer of that key breaks the check. Git's
    /// trailer rules read the key without regard to its case.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_trailer: Option<String>,
    /// The author names (`%an`, whole names) whose commits `require_trailer`
    /// leaves alone. The check-commit hook reads a message before git knows
    /// its author, so this list never applies there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exempt_authors: Option<Vec<String>>,
    /// The patterns, under the check's `flags`, of the subjects whose commits
    /// `require_trailer` leaves alone. A pattern may match anywhere in the
    /// subject; anchor it with `^` to match the start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exempt_subjects: Option<Vec<String>>,
    #[serde(rename = "if", default, skip_serializing_if = "Option::is_none")]
    pub if_changed: Option<Glob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then: Option<Glob>,
    /// A `co-change` check's opt-in into per-commit evaluation
    /// (`check_commit`'s own module doc has the reason it is opt-in, not
    /// the default): absent or `false` leaves the check range-level only
    /// (the `co_change` runner's own `run`, unaffected either
    /// way); `true` additionally runs it per commit on `check-commit`'s
    /// range arm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_commit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// A `runner` check's tool: a key of `.houserules.json` `runners`, which
    /// holds the command, the parser, and the install line of the tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The `non_empty` option of a `report-field` check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_empty: Option<bool>,
    /// The `covers_ids` option of a `report-field` check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers_ids: Option<bool>,
    /// Whether the check counts as coverage: a changed file that a covering
    /// check read is covered (batch 37, spec 4.4). Absent reads as `false`.
    /// A `commits` and a `report-field` check read no file, so
    /// `check-knowledge` refuses the key on them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covers: Option<bool>,
    /// An `intent-required` check's adapters: `file`, `openspec`, or both.
    /// Absent, and an empty list, read as both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sources: Option<Vec<String>>,
}

impl CheckDef {
    /// The globs of the files that the check reads, when it declares
    /// `covers: true`; empty for any other check. A `grep-absent`,
    /// `diff-append-only`, `durable-sha`, or `runner` check reads its `files`
    /// (a `runner` check with no `files` reads every file); a `co-change`
    /// check reads its `if` and `then` files; a `commits`, `report-field`,
    /// `intent-required`, or `intent-resolves` check reads no file (spec 4.4
    /// and batch-38 spec 5.5).
    pub(crate) fn covered_globs(&self) -> Vec<String> {
        if self.covers != Some(true) {
            return Vec::new();
        }
        let patterns = |glob: &Option<Glob>| glob.as_ref().map(Glob::patterns).unwrap_or_default();
        match self.kind {
            CheckType::GrepAbsent | CheckType::DiffAppendOnly | CheckType::DurableSha => {
                patterns(&self.files)
            }
            CheckType::Runner if self.files.is_none() => vec![ALL_FILES.to_string()],
            CheckType::Runner => patterns(&self.files),
            CheckType::CoChange => {
                let mut globs = patterns(&self.if_changed);
                globs.extend(patterns(&self.then));
                globs
            }
            CheckType::Commits
            | CheckType::ReportField
            | CheckType::IntentRequired
            | CheckType::IntentResolves => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use serde_json::{Value, json};

    use super::{CheckDef, CheckLevel, CheckType, Scope};
    use crate::schema_pin::{assert_enum_pinned, assert_object_pinned};

    /// `template/knowledge/schema.json` -- the vendored knowledge schema,
    /// the same file `check.rs`'s own tests read (`template_root`).
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
    /// `body_line_max`, `require_trailer`, `exempt_authors`,
    /// `exempt_subjects`, `if`, `then`, `per_commit`, `field`, `tool`,
    /// `non_empty`, `covers_ids`, `covers`, `sources`, plus `type`/`level`).
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
            "require_trailer": "Work-Item",
            "exempt_authors": ["dependabot[bot]"],
            "exempt_subjects": ["^chore\\(.+\\): release "],
            "if": ["**/*.rs"],
            "then": ["**/*.rs"],
            "per_commit": true,
            "field": "concerns",
            "tool": "semgrep",
            "non_empty": true,
            "covers_ids": true,
            "covers": true,
            "sources": ["file", "openspec"],
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
                CheckType::DurableSha,
                CheckType::Runner,
                CheckType::IntentRequired,
                CheckType::IntentResolves,
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

    /// The check that `value` describes.
    fn check(value: Value) -> CheckDef {
        serde_json::from_value(value).expect("a check")
    }

    #[test]
    fn a_check_that_does_not_declare_covers_reads_no_file_for_coverage() {
        for covers in [json!({}), json!({"covers": false})] {
            let mut value = json!({"type": "grep-absent", "level": "fail", "files": "src/**"});
            value
                .as_object_mut()
                .expect("object")
                .extend(covers.as_object().expect("object").clone());

            assert_eq!(check(value).covered_globs(), Vec::<String>::new());
        }
    }

    #[test]
    fn the_files_of_a_covering_check_are_the_files_it_reads() {
        for kind in ["grep-absent", "diff-append-only", "durable-sha"] {
            let one =
                check(json!({"type": kind, "level": "fail", "files": "src/**", "covers": true}));
            let many = check(json!({
                "type": kind, "level": "warn", "files": ["a/**", "b/*.rs"], "covers": true
            }));

            assert_eq!(one.covered_globs(), ["src/**"], "{kind}");
            assert_eq!(many.covered_globs(), ["a/**", "b/*.rs"], "{kind}");
        }
    }

    #[test]
    fn a_covering_co_change_check_reads_its_if_and_then_files() {
        let covering = check(json!({
            "type": "co-change", "level": "fail", "if": "src/lib.rs",
            "then": ["tests/a.rs", "tests/b.rs"], "covers": true
        }));

        assert_eq!(
            covering.covered_globs(),
            ["src/lib.rs", "tests/a.rs", "tests/b.rs"]
        );
    }

    #[test]
    fn a_covering_runner_check_with_no_files_reads_every_file() {
        let every = check(json!({"type": "runner", "level": "fail", "tool": "t", "covers": true}));
        let some = check(json!({
            "type": "runner", "level": "fail", "tool": "t", "files": "**/*.rs", "covers": true
        }));

        assert_eq!(every.covered_globs(), ["**"]);
        assert_eq!(some.covered_globs(), ["**/*.rs"]);
    }

    #[test]
    fn a_check_that_reads_no_file_reads_none_whatever_it_declares() {
        let commits = check(json!({"type": "commits", "level": "fail", "covers": true}));
        let report = check(json!({
            "type": "report-field", "level": "fail", "if": "**", "field": "f", "covers": true
        }));
        let required = check(json!({
            "type": "intent-required", "level": "fail", "if": "src/**", "covers": true
        }));
        let resolves = check(json!({"type": "intent-resolves", "level": "fail", "covers": true}));

        assert_eq!(commits.covered_globs(), Vec::<String>::new());
        assert_eq!(report.covered_globs(), Vec::<String>::new());
        assert_eq!(required.covered_globs(), Vec::<String>::new());
        assert_eq!(resolves.covered_globs(), Vec::<String>::new());
    }
}
