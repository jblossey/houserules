//! OpenSpec's validation of a `schema.yaml` (spec 3.3).
//!
//! OpenSpec 1.15.0 resolves a schema with `parseSchema`
//! (`artifact-graph/schema.ts`), which refuses a document that the zod schema
//! `SchemaYamlSchema` (`artifact-graph/types.ts`) rejects, and a document that
//! breaks one of four rules: no two artifacts share an `id`, each `requires`
//! names an artifact, each `apply.requires` names an artifact, and the
//! `requires` of the artifacts hold no cycle. [`is_valid`] mirrors both, so a
//! schema that OpenSpec refuses counts as unresolved and the change counts its
//! top-level `tasks.md`.

use std::collections::HashSet;

use saphyr::Yaml;

/// The most artifacts that a schema may declare (`MAX_ARTIFACTS` of `types.ts`).
const MAX_ARTIFACTS: usize = 1_000;

/// Whether OpenSpec 1.15.0 accepts `schema`, the document of a `schema.yaml`.
pub(super) fn is_valid(schema: &Yaml<'_>) -> bool {
    if !schema.is_mapping()
        || !is_non_empty_string(schema, "name")
        || !is_positive_integer(schema.as_mapping_get("version"))
        || !is_optional_string(schema, "description")
    {
        return false;
    }
    let Some(artifacts) = schema
        .as_mapping_get("artifacts")
        .and_then(Yaml::as_sequence)
    else {
        return false;
    };
    if !(1..=MAX_ARTIFACTS).contains(&artifacts.len()) || !artifacts.iter().all(is_valid_artifact) {
        return false;
    }
    let apply_ids = match schema.as_mapping_get("apply") {
        None => Vec::new(),
        Some(apply) => match checked_apply_requires(apply) {
            Some(ids) => ids,
            None => return false,
        },
    };
    let declared: Vec<(&str, Vec<&str>)> = artifacts
        .iter()
        .map(|artifact| {
            (
                artifact_id(artifact),
                string_list(artifact.as_mapping_get("requires")).unwrap_or_default(),
            )
        })
        .collect();
    has_consistent_references(&declared, &apply_ids)
}

/// The `id` of an artifact that passed [`is_valid_artifact`].
fn artifact_id<'a>(artifact: &'a Yaml<'_>) -> &'a str {
    artifact
        .as_mapping_get("id")
        .and_then(Yaml::as_str)
        .unwrap_or_default()
}

/// Whether `node` is a mapping that holds a non-empty string at `key`.
fn is_non_empty_string(node: &Yaml<'_>, key: &str) -> bool {
    node.as_mapping_get(key)
        .and_then(Yaml::as_str)
        .is_some_and(|text| !text.is_empty())
}

/// Whether `node` holds no `key`, or a string at it. A `key` with no value is
/// no string.
fn is_optional_string(node: &Yaml<'_>, key: &str) -> bool {
    node.as_mapping_get(key)
        .is_none_or(|value| value.as_str().is_some())
}

/// Whether `value` is a number that is an integer above zero, as `1` and `2.0`
/// are.
fn is_positive_integer(value: Option<&Yaml<'_>>) -> bool {
    let Some(value) = value else {
        return false;
    };
    match (value.as_integer(), value.as_floating_point()) {
        (Some(integer), _) => integer > 0,
        (None, Some(float)) => float.is_finite() && float.fract() == 0.0 && float > 0.0,
        (None, None) => false,
    }
}

/// Whether `text` is a non-empty relative path inside its folder: not absolute
/// for POSIX or Windows, with no drive, no NUL, and no `..` segment
/// (`relativePathSchema` of `types.ts`).
fn is_relative_path(text: &str) -> bool {
    let drive = text.as_bytes();
    let has_drive = drive.len() >= 2 && drive[0].is_ascii_alphabetic() && drive[1] == b':';
    !text.is_empty()
        && !text.starts_with(['/', '\\'])
        && !has_drive
        && !text.contains('\0')
        && !text.split(['/', '\\']).any(|segment| segment == "..")
}

/// Whether `value` is a string that [`is_relative_path`] accepts.
fn is_relative_path_value(value: &Yaml<'_>) -> bool {
    value.as_str().is_some_and(is_relative_path)
}

/// The strings of the list `value`; an absent value is the empty list, and
/// `None` is a value that is no list of strings.
fn string_list<'a>(value: Option<&'a Yaml<'_>>) -> Option<Vec<&'a str>> {
    match value {
        None => Some(Vec::new()),
        Some(list) => list
            .as_sequence()?
            .iter()
            .map(Yaml::as_str)
            .collect::<Option<Vec<_>>>(),
    }
}

/// Whether `artifact` has the fields of `ArtifactSchema`: a non-empty `id`, a
/// relative `generates`, a `description`, a relative `template`, an optional
/// `instruction`, and a `requires` that holds strings.
fn is_valid_artifact(artifact: &Yaml<'_>) -> bool {
    artifact.is_mapping()
        && is_non_empty_string(artifact, "id")
        && artifact
            .as_mapping_get("generates")
            .is_some_and(is_relative_path_value)
        && artifact
            .as_mapping_get("description")
            .and_then(Yaml::as_str)
            .is_some()
        && artifact
            .as_mapping_get("template")
            .is_some_and(is_relative_path_value)
        && is_optional_string(artifact, "instruction")
        && string_list(artifact.as_mapping_get("requires")).is_some()
}

/// The `requires` of an `apply` block that has the fields of `ApplyPhaseSchema`
/// (at least one artifact id, a `tracks` that is null, absent, or a relative
/// path, an optional `instruction`), or `None` for a block that has not.
fn checked_apply_requires<'a>(apply: &'a Yaml<'_>) -> Option<Vec<&'a str>> {
    if !apply.is_mapping() || !is_optional_string(apply, "instruction") {
        return None;
    }
    let tracks_ok = apply
        .as_mapping_get("tracks")
        .is_none_or(|tracks| tracks.is_null() || is_relative_path_value(tracks));
    let requires = string_list(apply.as_mapping_get("requires"))?;
    (tracks_ok && !requires.is_empty()).then_some(requires)
}

/// Whether the artifacts `declared` (an id and its `requires`) pass the four
/// rules of `parseSchema` that the fields do not cover: no repeated id, no
/// unknown id in `requires` or in `apply_requires`, and no cycle.
fn has_consistent_references(declared: &[(&str, Vec<&str>)], apply_requires: &[&str]) -> bool {
    let ids: HashSet<&str> = declared.iter().map(|(id, _)| *id).collect();
    if ids.len() != declared.len() {
        return false;
    }
    let names_a_declared_id = |id: &&str| ids.contains(*id);
    if !declared
        .iter()
        .all(|(_, requires)| requires.iter().all(names_a_declared_id))
        || !apply_requires.iter().all(names_a_declared_id)
    {
        return false;
    }
    let mut resolved: HashSet<&str> = HashSet::new();
    loop {
        let before = resolved.len();
        for (id, requires) in declared {
            if requires.iter().all(|required| resolved.contains(required)) {
                resolved.insert(*id);
            }
        }
        if resolved.len() == before {
            return resolved.len() == declared.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::document;
    use super::*;

    /// A schema that OpenSpec accepts, with every optional field.
    const VALID: &str = "\
name: custom
version: 1
description: a schema
artifacts:
  - id: proposal
    generates: proposal.md
    description: p
    template: proposal.md
    instruction: write it
    requires: []
  - id: tasks
    generates: \"work/*.md\"
    description: t
    template: tasks.md
    requires: [proposal]
apply:
  requires: [tasks]
  tracks: \"work/*.md\"
  instruction: do it
";

    /// Whether [`is_valid`] accepts the YAML text `text`.
    fn accepts(text: &str) -> bool {
        document(text).is_some_and(|schema| is_valid(&schema))
    }

    /// [`VALID`] with its text `old` replaced by `new`.
    fn edited(old: &str, new: &str) -> String {
        assert!(VALID.contains(old), "{old:?}");
        VALID.replacen(old, new, 1)
    }

    #[test]
    fn a_schema_that_openspec_accepts_is_valid() {
        assert!(accepts(VALID));
        let minimal = "name: m\nversion: 2.0\nartifacts:\n  - id: a\n    generates: a.md\n    description: ''\n    template: t.md\n";
        assert!(accepts(minimal));
        let extra_keys = edited("name: custom\n", "name: custom\nx-other: [1, 2]\n");
        assert!(accepts(&extra_keys));
        let diamond = "name: d\nversion: 1\nartifacts:\n  - {id: a, generates: a.md, description: d, template: t.md}\n  - {id: b, generates: b.md, description: d, template: t.md, requires: [a]}\n  - {id: c, generates: c.md, description: d, template: t.md, requires: [a]}\n  - {id: e, generates: e.md, description: d, template: t.md, requires: [b, c]}\n";
        assert!(accepts(diamond));
        let apply_without_tracks = edited("  tracks: \"work/*.md\"\n", "  tracks:\n");
        assert!(accepts(&apply_without_tracks));
    }

    /// A schema that is no YAML mapping, and a mapping that lacks or misreads a
    /// field of `SchemaYamlSchema`, is refused.
    #[test]
    fn a_schema_with_a_wrong_top_level_field_is_refused() {
        for (old, new) in [
            ("name: custom\n", ""),
            ("name: custom\n", "name: ''\n"),
            ("name: custom\n", "name: 5\n"),
            ("version: 1\n", ""),
            ("version: 1\n", "version: 0\n"),
            ("version: 1\n", "version: -1\n"),
            ("version: 1\n", "version: '1'\n"),
            ("version: 1\n", "version: 1.5\n"),
            ("version: 1\n", "version:\n"),
            ("description: a schema\n", "description: 5\n"),
            ("description: a schema\n", "description:\n"),
        ] {
            assert!(!accepts(&edited(old, new)), "{old:?} -> {new:?}");
        }
        for text in ["", "- a list\n", "text\n", "name: m\nversion: 1\n"] {
            assert!(!accepts(text), "{text:?}");
        }
    }

    #[test]
    fn a_schema_needs_between_one_and_a_thousand_artifacts() {
        let with = |count: usize| {
            let mut text = "name: m\nversion: 1\nartifacts:\n".to_string();
            for index in 0..count {
                text.push_str(&format!(
                    "  - {{id: a{index}, generates: a{index}.md, description: d, template: t.md}}\n"
                ));
            }
            text
        };

        assert!(!accepts("name: m\nversion: 1\nartifacts: []\n"));
        assert!(!accepts("name: m\nversion: 1\nartifacts: text\n"));
        assert!(accepts(&with(1)));
        assert!(accepts(&with(1000)));
        assert!(!accepts(&with(1001)));
    }

    #[test]
    fn an_artifact_with_a_wrong_field_is_refused() {
        for (old, new) in [
            ("  - id: proposal\n", "  - description: p\n"),
            ("  - id: proposal\n", "  - id: ''\n"),
            ("    generates: proposal.md\n", ""),
            ("    generates: proposal.md\n", "    generates: 5\n"),
            ("    generates: proposal.md\n", "    generates: ''\n"),
            ("    description: p\n", ""),
            ("    description: p\n", "    description:\n"),
            ("    template: proposal.md\n", ""),
            ("    template: proposal.md\n", "    template: ../t.md\n"),
            ("    instruction: write it\n", "    instruction: 5\n"),
            ("    requires: []\n", "    requires: text\n"),
            ("    requires: []\n", "    requires: [1]\n"),
            ("    requires: []\n", "    requires:\n"),
            ("  - id: proposal\n", "  - text\n  - id: proposal\n"),
        ] {
            assert!(!accepts(&edited(old, new)), "{old:?} -> {new:?}");
        }
    }

    /// `generates` and `template` take a relative path inside the change folder:
    /// no absolute path for POSIX or Windows, no drive, and no `..` segment.
    #[test]
    fn a_path_field_refuses_an_absolute_path_a_drive_and_a_parent_segment() {
        for path in [
            "/abs.md",
            "\\abs.md",
            "C:x.md",
            "a:b.md",
            "c:/x.md",
            "C:\\x.md",
            "../x.md",
            "a/../b.md",
            "a\\..\\b.md",
            "a/..",
        ] {
            let text = edited(
                "    generates: proposal.md\n",
                &format!("    generates: '{path}'\n"),
            );
            assert!(!accepts(&text), "{path}");
        }
        for path in ["a/b.md", "./x.md", "..a.md", "a..b/c.md", "ab:c.md", "x"] {
            let text = edited(
                "    generates: proposal.md\n",
                &format!("    generates: '{path}'\n"),
            );
            assert!(accepts(&text), "{path}");
        }
    }

    /// A one-artifact schema, followed by the text `apply`.
    fn with_apply(apply: &str) -> String {
        format!(
            "name: m\nversion: 1\nartifacts:\n  - {{id: a, generates: a.md, description: d, template: t.md}}\n{apply}"
        )
    }

    /// An `apply` that is no mapping is refused, as is a null one.
    #[test]
    fn an_apply_that_is_no_block_is_refused() {
        for apply in ["apply: text\n", "apply:\n", "apply: [a]\n", "apply: {}\n"] {
            assert!(!accepts(&with_apply(apply)), "{apply:?}");
        }
        assert!(accepts(&with_apply("apply:\n  requires: [a]\n")));
        assert!(accepts(&with_apply(
            "apply:\n  requires: [a]\n  tracks: null\n"
        )));
    }

    #[test]
    fn an_apply_block_with_a_wrong_field_is_refused() {
        for (old, new) in [
            ("  requires: [tasks]\n", ""),
            ("  requires: [tasks]\n", "  requires: []\n"),
            ("  requires: [tasks]\n", "  requires: tasks\n"),
            ("  requires: [tasks]\n", "  requires: [3]\n"),
            ("  tracks: \"work/*.md\"\n", "  tracks: 5\n"),
            ("  tracks: \"work/*.md\"\n", "  tracks: ''\n"),
            ("  tracks: \"work/*.md\"\n", "  tracks: ../x.md\n"),
            ("  instruction: do it\n", "  instruction: 5\n"),
        ] {
            assert!(!accepts(&edited(old, new)), "{old:?} -> {new:?}");
        }
    }

    /// The four rules of `parseSchema` beyond the fields.
    #[test]
    fn a_schema_with_a_repeated_id_an_unknown_reference_or_a_cycle_is_refused() {
        for (name, old, new) in [
            ("a repeated id", "  - id: tasks\n", "  - id: proposal\n"),
            (
                "an unknown requires",
                "    requires: [proposal]\n",
                "    requires: [nosuch]\n",
            ),
            (
                "an unknown apply.requires",
                "  requires: [tasks]\n",
                "  requires: [nosuch]\n",
            ),
            (
                "a self cycle",
                "    requires: [proposal]\n",
                "    requires: [tasks]\n",
            ),
            (
                "a two-artifact cycle",
                "    requires: []\n",
                "    requires: [tasks]\n",
            ),
        ] {
            assert!(!accepts(&edited(old, new)), "{name}");
        }
    }
}
