//! The files that hold the tasks of an OpenSpec change (spec 3.3, `tasks`).
//!
//! [`tracked_files`] picks the files of a change that OpenSpec 1.15.0 counts
//! (`getTaskProgressDetailForChange`):
//!
//! 1. The schema is the `schema` of the change's `.openspec.yaml` when the file
//!    exists; a file that fails to parse as YAML, holds no non-empty `schema`, or
//!    names a schema that neither the tree nor the built-ins hold leaves the
//!    change with no schema (OpenSpec throws, and its reader falls back). A
//!    change with no `.openspec.yaml` takes the `schema` of the config, else
//!    `spec-driven`.
//! 2. The schema file is `openspec/schemas/<name>/schema.yaml` of the tree; one
//!    that fails to parse counts as absent, and one that OpenSpec's validation
//!    refuses (see [`schema`]) is unresolved. Without a file, `spec-driven` is the
//!    built-in schema of 1.15.0, whose `tasks` artifact generates `tasks.md`; any
//!    other name is unresolved (a schema of the user's data folder is outside the
//!    tree).
//! 3. The tracked artifact is the one whose `generates` equals `apply.tracks`,
//!    else the one with the id `tasks`. Its `generates` is a glob relative to the
//!    change folder. [`outputs::alternatives`] gives its patterns as OpenSpec
//!    expands its brace lists; each pattern is matched segment by segment with
//!    `rules::compile` (`*` stays inside a path segment). As in OpenSpec's
//!    `dot: false`, a name that starts with `.` matches only a segment of an
//!    alternative that starts with `.`, and `**` crosses no such folder. A brace
//!    group that the expansion leaves, a brace range included, and a `[` with no
//!    closing bracket match as the characters themselves. An extglob is not
//!    expanded, so it selects no file of a tree. An alternative that does not
//!    compile selects no file, as picomatch's never-matching pattern does, and
//!    no line names it. A glob that OpenSpec refuses with an error of its own
//!    (an expansion past 1,024 patterns, braces nested more than 16 levels, an
//!    alternative that leaves the change folder) selects no file, so the
//!    `tasks.md` fallback counts, where OpenSpec gives no count: a departure that
//!    spec 3.3 records.
//! 4. With no schema, no tracked artifact, or no matched file, the top-level
//!    `tasks.md` counts.

use globset::GlobMatcher;
use saphyr::Yaml;

use super::{document, outputs, schema};
use crate::rules::compile;

/// The schema that a change with no named schema uses, built into OpenSpec.
const BUILT_IN_SCHEMA: &str = "spec-driven";

/// The file that the built-in schema tracks, and the fallback of every other
/// case.
const TASKS_FILE: &str = "tasks.md";

/// What [`tracked_files`] reads.
pub(super) struct Inputs<'a> {
    /// The text of the change's `.openspec.yaml`, when the change has the file.
    pub(super) metadata: Option<&'a str>,
    /// The non-empty `schema` of the project's `openspec/config.yaml`.
    pub(super) config_schema: Option<&'a str>,
    /// The text of `openspec/schemas/<name>/schema.yaml` of the tree, by name.
    /// The `Err` is the line of a failed read.
    pub(super) project_schema: &'a dyn Fn(&str) -> Result<Option<String>, String>,
    /// The regular files of the change folder, as paths from the folder, in the
    /// order that the result keeps.
    pub(super) files: &'a [String],
}

/// The files of the change that hold its tasks, in the order of `inputs.files`:
/// the files that the tracked artifact generates, else the top-level
/// `tasks.md` when the change has it, else none. The `Err` is the line of a
/// failed read of a schema file.
pub(super) fn tracked_files(inputs: &Inputs<'_>) -> Result<Vec<String>, String> {
    if let Some(name) = schema_name(inputs)
        && let Some(generates) = tracked_glob(inputs, &name)?
    {
        let selected = select(&generates, inputs.files);
        if !selected.is_empty() {
            return Ok(selected);
        }
    }
    Ok(inputs
        .files
        .iter()
        .filter(|file| file.as_str() == TASKS_FILE)
        .cloned()
        .collect())
}

/// The name of the schema of the change, or `None` when its `.openspec.yaml`
/// names none.
fn schema_name(inputs: &Inputs<'_>) -> Option<String> {
    match inputs.metadata {
        Some(text) => document(text)
            .as_ref()
            .and_then(|metadata| metadata.as_mapping_get("schema"))
            .and_then(Yaml::as_str)
            .filter(|name| !name.is_empty())
            .map(str::to_string),
        None => Some(inputs.config_schema.unwrap_or(BUILT_IN_SCHEMA).to_string()),
    }
}

/// The `generates` of the tracked artifact of the schema `name`, or `None` when
/// the schema is unresolved (a schema that OpenSpec's validation refuses is
/// unresolved) or tracks no artifact.
fn tracked_glob(inputs: &Inputs<'_>, name: &str) -> Result<Option<String>, String> {
    let text = (inputs.project_schema)(name)?;
    Ok(match text.as_deref().and_then(document) {
        Some(parsed) => schema::is_valid(&parsed)
            .then(|| generates_of_tracked(&parsed))
            .flatten(),
        None if name == BUILT_IN_SCHEMA => Some(TASKS_FILE.to_string()),
        None => None,
    })
}

/// The `generates` of the artifact that `apply.tracks` of `schema` selects, or
/// of the artifact `tasks` when `apply.tracks` is not set.
fn generates_of_tracked(schema: &Yaml<'_>) -> Option<String> {
    let artifacts = schema
        .as_mapping_get("artifacts")
        .and_then(Yaml::as_sequence)?;
    let generates = |artifact: &Yaml<'_>| {
        artifact
            .as_mapping_get("generates")
            .and_then(Yaml::as_str)
            .map(str::to_string)
    };
    let tracks = schema
        .as_mapping_get("apply")
        .and_then(|apply| apply.as_mapping_get("tracks"))
        .filter(|tracks| !tracks.is_null());
    let tracked = match tracks {
        Some(tracks) => {
            let wanted = tracks.as_str()?;
            artifacts
                .iter()
                .find(|artifact| generates(artifact).as_deref() == Some(wanted))
        }
        None => artifacts
            .iter()
            .find(|artifact| artifact.as_mapping_get("id").and_then(Yaml::as_str) == Some("tasks")),
    }?;
    generates(tracked)
}

/// The files of `files` that `generates` selects: those that an alternative of
/// its brace expansion matches, segment by segment. An alternative that does not
/// compile selects none, and a glob that OpenSpec refuses selects none.
fn select(generates: &str, files: &[String]) -> Vec<String> {
    let pattern = tidy(generates);
    if !outputs::is_glob(&pattern) {
        return files
            .iter()
            .filter(|file| **file == pattern)
            .cloned()
            .collect();
    }
    let Some(patterns) = outputs::alternatives(&pattern) else {
        return Vec::new();
    };
    let alternatives: Vec<Vec<Segment>> = patterns
        .iter()
        .filter_map(|alternative| {
            split(alternative)
                .into_iter()
                .map(Segment::new)
                .collect::<Option<Vec<_>>>()
        })
        .collect();
    files
        .iter()
        .filter(|file| {
            let parts: Vec<&str> = file.split('/').collect();
            alternatives
                .iter()
                .any(|segments| segments_match(segments, &parts))
        })
        .cloned()
        .collect()
}

/// `generates` with every run of slashes made one and every leading `./`
/// dropped, as OpenSpec resolves the path of an artifact.
fn tidy(generates: &str) -> String {
    let mut single = String::with_capacity(generates.len());
    for c in generates.chars() {
        if c != '/' || !single.ends_with('/') {
            single.push(c);
        }
    }
    let mut pattern = single.as_str();
    while let Some(rest) = pattern.strip_prefix("./") {
        pattern = rest;
    }
    pattern.to_string()
}

/// The index of the `]` that closes the class that opens at `start`, if there
/// is one: a `]` first in the class is a member, as in `[]a]` and `[!]a]`.
fn class_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    if matches!(bytes.get(index), Some(b'!' | b'^')) {
        index += 1;
    }
    if bytes.get(index) == Some(&b']') {
        index += 1;
    }
    let offset = bytes.get(index..)?.iter().position(|byte| *byte == b']')?;
    Some(index + offset)
}

/// One segment of a pattern, between slashes.
enum Segment {
    /// `**`: any number of folders that start with no `.`.
    GlobStar,
    /// Any other segment: its text, and its matcher.
    Name {
        /// The text of the segment.
        text: String,
        /// The matcher of the segment.
        matcher: GlobMatcher,
    },
}

impl Segment {
    /// The segment `text`, or `None` for a glob that does not compile.
    fn new(text: &str) -> Option<Self> {
        if text == "**" {
            return Some(Segment::GlobStar);
        }
        compile(&literal_characters(text))
            .ok()
            .map(|matcher| Segment::Name {
                text: text.to_string(),
                matcher,
            })
    }
}

/// `text` with each brace, and each `[` that closes no class, written as a class
/// of one character. The expansion has run, so a brace that is left is a
/// character of a name, as it is to picomatch with brace expansion off; globset
/// would read it as a list.
fn literal_characters(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut literal = String::with_capacity(text.len());
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let replacement = match bytes[index] {
            b'[' => match class_end(bytes, index) {
                Some(end) => {
                    index = end;
                    None
                }
                None => Some("[[]"),
            },
            b'{' => Some("[{]"),
            b'}' => Some("[}]"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            literal.push_str(&text[start..index]);
            literal.push_str(replacement);
            start = index + 1;
        }
        index += 1;
    }
    literal.push_str(&text[start..]);
    literal
}

/// The segments of `pattern`: the text between the slashes that stand outside
/// a class.
fn split(pattern: &str) -> Vec<&str> {
    let bytes = pattern.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'[' => {
                if let Some(end) = class_end(bytes, index) {
                    index = end;
                }
            }
            b'/' => {
                parts.push(&pattern[start..index]);
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    parts.push(&pattern[start..]);
    parts
}

/// Whether `segments` match the path segments `path`, one by one: a name that
/// starts with `.` only a segment that starts with `.`, and `**` no such name.
fn segments_match(segments: &[Segment], path: &[&str]) -> bool {
    match segments.split_first() {
        None => path.is_empty(),
        Some((Segment::GlobStar, rest)) => {
            segments_match(rest, path)
                || (path.first().is_some_and(|name| !name.starts_with('.'))
                    && segments_match(segments, &path[1..]))
        }
        Some((Segment::Name { text, matcher }, rest)) => {
            path.first().is_some_and(|name| {
                (!name.starts_with('.') || text.starts_with('.')) && matcher.is_match(name)
            }) && segments_match(rest, &path[1..])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A schema that OpenSpec accepts, with the artifacts `proposal` and `tasks`,
    /// the second with the glob `generates`, and an `apply` block that tracks
    /// `tracks` when it is given.
    fn schema(generates: &str, tracks: Option<&str>) -> String {
        let mut text = format!(
            "name: custom\nversion: 1\nartifacts:\n  - id: proposal\n    generates: proposal.md\n    description: p\n    template: proposal.md\n  - id: tasks\n    generates: \"{generates}\"\n    description: t\n    template: tasks.md\n    requires: [proposal]\n"
        );
        if let Some(tracks) = tracks {
            text.push_str(&format!(
                "apply:\n  requires: [tasks]\n  tracks: \"{tracks}\"\n"
            ));
        }
        text
    }

    /// The files that count for a change with the metadata `metadata`, the
    /// config schema `config`, the schemas `schemas` of the tree (name and
    /// text), and the files `files`.
    fn tracked(
        metadata: Option<&str>,
        config: Option<&str>,
        schemas: &[(&str, String)],
        files: &[&str],
    ) -> Result<Vec<String>, String> {
        let files: Vec<String> = files.iter().map(|file| (*file).to_string()).collect();
        let project_schema = |name: &str| {
            Ok(schemas
                .iter()
                .find(|(known, _)| *known == name)
                .map(|(_, text)| text.clone()))
        };
        tracked_files(&Inputs {
            metadata,
            config_schema: config,
            project_schema: &project_schema,
            files: &files,
        })
    }

    /// The files that count, as text.
    fn files_of(result: Result<Vec<String>, String>) -> Vec<String> {
        result.expect("a schema that resolves")
    }

    // ---- the schema ----------------------------------------------------------------------

    /// With no metadata and no config, the schema is the built-in `spec-driven`,
    /// whose tasks artifact generates `tasks.md`.
    #[test]
    fn the_built_in_schema_tracks_the_top_level_tasks_file() {
        let files = ["proposal.md", "tasks.md", "work/tasks.md"];

        assert_eq!(files_of(tracked(None, None, &[], &files)), ["tasks.md"]);
        assert_eq!(
            files_of(tracked(
                Some("schema: spec-driven\ncreated: 2026-10-25\n"),
                None,
                &[],
                &files
            )),
            ["tasks.md"]
        );
        assert_eq!(
            files_of(tracked(None, None, &[], &["proposal.md"])),
            Vec::<String>::new()
        );
    }

    /// The schema of the metadata wins over the config's, and the config's is
    /// used for a change with no metadata.
    #[test]
    fn the_metadata_decides_before_the_config() {
        let schemas = [
            ("todo", schema("todo/*.md", None)),
            ("work", schema("work/*.md", None)),
        ];
        let files = ["tasks.md", "todo/one.md", "work/two.md"];

        assert_eq!(
            files_of(tracked(None, Some("todo"), &schemas, &files)),
            ["todo/one.md"]
        );
        assert_eq!(
            files_of(tracked(
                Some("schema: work\n"),
                Some("todo"),
                &schemas,
                &files
            )),
            ["work/two.md"]
        );
    }

    /// A metadata file that fails to parse, names no schema, names an empty or
    /// no text schema, or names a schema that the tree does not hold sends the
    /// count to `tasks.md`, whatever the config says (OpenSpec throws, and its
    /// reader falls back).
    #[test]
    fn metadata_that_does_not_name_a_schema_sends_the_count_to_the_tasks_file() {
        let schemas = [("todo", schema("todo/*.md", None))];
        let files = ["tasks.md", "todo/one.md"];

        for metadata in [
            "schema: [unclosed\n",
            "created: 2026-10-25\n",
            "",
            "schema: \"\"\n",
            "schema: 5\n",
            "- a list\n",
            "schema: todo\n---\nschema: todo\n",
            "schema: nosuchschema\n",
        ] {
            assert_eq!(
                files_of(tracked(Some(metadata), Some("todo"), &schemas, &files)),
                ["tasks.md"],
                "{metadata:?}"
            );
        }
    }

    /// A schema of the tree that fails to parse counts as absent: a custom name
    /// is unresolved, and `spec-driven` is the built-in.
    #[test]
    fn a_schema_file_that_fails_to_parse_counts_as_absent() {
        let broken = [("todo", "artifacts: [unclosed\n".to_string())];
        let shadow = [("spec-driven", "artifacts: [unclosed\n".to_string())];
        let files = ["tasks.md", "todo/one.md"];

        assert_eq!(
            files_of(tracked(Some("schema: todo\n"), None, &broken, &files)),
            ["tasks.md"]
        );
        assert_eq!(files_of(tracked(None, None, &shadow, &files)), ["tasks.md"]);
    }

    /// A `spec-driven` schema of the tree wins over the built-in one.
    #[test]
    fn a_project_schema_named_spec_driven_wins_over_the_built_in_one() {
        let local = [("spec-driven", schema("todo/*.md", None))];
        let files = ["tasks.md", "todo/one.md"];

        assert_eq!(
            files_of(tracked(None, None, &local, &files)),
            ["todo/one.md"]
        );
    }

    /// A schema that OpenSpec's validation refuses is unresolved (spec 3.3): the
    /// count falls back to `tasks.md`, whatever its `generates` says; the same
    /// schema with the missing field selects its files.
    #[test]
    fn a_schema_that_openspec_refuses_is_unresolved() {
        let accepted = schema("work/*.md", None);
        let no_version = accepted.replace("version: 1\n", "");
        let schemas = [
            ("accepted", accepted),
            ("no-version", no_version.clone()),
            ("spec-driven", no_version),
        ];
        let files = ["tasks.md", "work/a.md"];
        let by = |name: &str| {
            files_of(tracked(
                Some(&format!("schema: {name}\n")),
                None,
                &schemas,
                &files,
            ))
        };

        assert_eq!(by("accepted"), ["work/a.md"]);
        assert_eq!(by("no-version"), ["tasks.md"]);
        assert_eq!(by("spec-driven"), ["tasks.md"]);
    }

    // ---- the tracked artifact ------------------------------------------------------------

    /// The artifact whose `generates` equals `apply.tracks` is the tracked one,
    /// whatever its id; with no `apply.tracks`, it is the artifact with the id
    /// `tasks`; a `tracks` that no artifact generates leaves none.
    #[test]
    fn the_apply_block_selects_the_tracked_artifact() {
        let artifact = |id: &str, generates: &str| {
            format!(
                "  - id: {id}\n    generates: \"{generates}\"\n    description: d\n    template: t.md\n"
            )
        };
        let schema_of = |name: &str, artifacts: &str, apply: &str| {
            format!("name: {name}\nversion: 1\nartifacts:\n{artifacts}{apply}")
        };
        let both = schema_of(
            "both",
            &(artifact("tasks", "tasks.md") + &artifact("todo", "todo/*.md")),
            "apply:\n  requires: [tasks]\n  tracks: \"todo/*.md\"\n",
        );
        let none = schema_of(
            "none",
            &artifact("tasks", "todo/*.md"),
            "apply:\n  requires: [tasks]\n  tracks: other.md\n",
        );
        let number = schema_of(
            "number",
            &artifact("tasks", "todo/*.md"),
            "apply:\n  requires: [tasks]\n  tracks: 5\n",
        );
        let no_tasks = schema_of("no-tasks", &artifact("plan", "todo/*.md"), "");
        let schemas = [
            ("both", both),
            ("none", none),
            ("number", number),
            ("no-tasks", no_tasks),
        ];
        let files = ["tasks.md", "todo/one.md"];
        let by = |name: &str| {
            files_of(tracked(
                Some(&format!("schema: {name}\n")),
                None,
                &schemas,
                &files,
            ))
        };

        assert_eq!(by("both"), ["todo/one.md"]);
        assert_eq!(by("none"), ["tasks.md"]);
        assert_eq!(by("number"), ["tasks.md"]);
        assert_eq!(by("no-tasks"), ["tasks.md"]);
    }

    /// A schema with no `artifacts` list, or a list that holds no `generates`, is
    /// no schema that OpenSpec accepts: it tracks nothing.
    #[test]
    fn a_schema_with_no_usable_artifact_tracks_nothing() {
        let schemas = [
            ("empty", "name: empty\n".to_string()),
            (
                "bare",
                "name: bare\nartifacts:\n  - id: tasks\n".to_string(),
            ),
            ("scalar", "artifacts: text\n".to_string()),
        ];
        for name in ["empty", "bare", "scalar"] {
            assert_eq!(
                files_of(tracked(
                    Some(&format!("schema: {name}\n")),
                    None,
                    &schemas,
                    &["tasks.md"]
                )),
                ["tasks.md"],
                "{name}"
            );
        }
    }

    // ---- the glob ------------------------------------------------------------------------

    /// The files that `generates` selects, for a change that holds `files`.
    fn selected(generates: &str, files: &[&str]) -> Vec<String> {
        let schemas = [("custom", schema(generates, None))];
        let mut with_fallback = files.to_vec();
        with_fallback.retain(|file| *file != "tasks.md");
        files_of(tracked(
            Some("schema: custom\n"),
            None,
            &schemas,
            &with_fallback,
        ))
    }

    /// `*` stays inside a path segment; `**` crosses segments; `?`, a class,
    /// and a brace list match as in every glob of the kit.
    #[test]
    fn the_glob_matches_as_the_other_globs_of_the_kit() {
        let files = ["a.md", "work/a.md", "work/c.txt", "work/sub/b.md", "x1.md"];

        assert_eq!(selected("*.md", &files), ["a.md", "x1.md"]);
        assert_eq!(selected("work/*.md", &files), ["work/a.md"]);
        assert_eq!(
            selected("**/*.md", &files),
            ["a.md", "work/a.md", "work/sub/b.md", "x1.md"]
        );
        assert_eq!(
            selected("work/**", &files),
            ["work/a.md", "work/c.txt", "work/sub/b.md"]
        );
        assert_eq!(selected("x?.md", &files), ["x1.md"]);
        assert_eq!(selected("[a-b].md", &files), ["a.md"]);
        assert_eq!(
            selected("work/{a,c}.*", &files),
            ["work/a.md", "work/c.txt"]
        );
    }

    /// A name that starts with `.` matches only a pattern segment that starts
    /// with `.`, and `**` never crosses a folder that starts with `.`.
    #[test]
    fn a_dot_name_matches_only_a_pattern_segment_that_starts_with_a_dot() {
        let files = [
            "work/a.md",
            "work/.hidden.md",
            ".secret/x.md",
            "work/.dir/y.md",
        ];

        assert_eq!(selected("work/*.md", &files), ["work/a.md"]);
        assert_eq!(selected("work/.*.md", &files), ["work/.hidden.md"]);
        assert_eq!(selected("**/*.md", &files), ["work/a.md"]);
        assert_eq!(selected(".secret/*.md", &files), [".secret/x.md"]);
        assert_eq!(selected("work/.dir/*.md", &files), ["work/.dir/y.md"]);
        assert_eq!(selected("**/.dir/*.md", &files), ["work/.dir/y.md"]);
    }

    /// A brace list expands before the dot rule applies to each segment (spec
    /// 3.3): each alternative is a pattern of its own, so a segment that an
    /// alternative starts with `.` selects a name that starts with `.`, and the
    /// other alternatives do not.
    #[test]
    fn a_brace_list_expands_before_the_dot_rule_applies() {
        assert_eq!(
            selected("{work/.hidden,x}.md", &["work/.hidden.md"]),
            ["work/.hidden.md"]
        );
        assert_eq!(
            selected("{.a,b}.md", &[".a.md", "b.md", ".b.md", "a.md"]),
            [".a.md", "b.md"]
        );
        assert_eq!(
            selected(
                "work/{.dir/y,a}.md",
                &["work/.dir/y.md", "work/a.md", "work/.a.md"]
            ),
            ["work/.dir/y.md", "work/a.md"]
        );
        assert_eq!(
            selected(
                "w/{a,{b,.c}}.md",
                &["w/a.md", "w/b.md", "w/.c.md", "w/.a.md"]
            ),
            ["w/a.md", "w/b.md", "w/.c.md"]
        );
        assert_eq!(
            selected("{a,b}/{c,d}.md", &["a/c.md", "a/d.md", "b/c.md", "e/c.md"]),
            ["a/c.md", "a/d.md", "b/c.md"]
        );
    }

    /// A nested brace list expands as OpenSpec's `expandArtifactBraces` expands
    /// it: the outermost list with a comma of its own first, and the lists inside
    /// its items in the same pass. A nest of `depth` lists gives `depth + 1`
    /// patterns, not `2^depth`, so a nest of 11 or 12 stays within the bound.
    #[test]
    fn a_nested_brace_list_gives_one_pattern_for_each_item() {
        let nest = |depth: usize| format!("{}b{}.md", "{a,".repeat(depth), "}".repeat(depth));

        for depth in [9, 10, 11, 12, 16] {
            assert_eq!(
                selected(&nest(depth), &["a.md", "b.md", "c.md"]),
                ["a.md", "b.md"],
                "depth {depth}"
            );
        }
    }

    /// A brace list is found without reading classes, as OpenSpec's
    /// `findTopLevelExpansion` finds it: a comma inside a class separates the
    /// items, and a bracket with no closing bracket in its item matches as the
    /// character itself.
    #[test]
    fn a_class_does_not_hide_a_brace_list() {
        assert_eq!(selected("{[a,b],c}.md", &["a.md", "c.md"]), ["c.md"]);
        assert_eq!(
            selected("{[a,b],c}.md", &["[a.md", "c.md"]),
            ["[a.md", "c.md"]
        );
        assert_eq!(
            selected("[ab]{c,d}.md", &["ac.md", "bd.md", "ae.md"]),
            ["ac.md", "bd.md"]
        );
    }

    /// A list that expands to no pattern leaves the pattern as it is: `{,}` has
    /// only empty items, which OpenSpec drops, so `[{,}]` stays the class of
    /// `{`, `,`, and `}`.
    #[test]
    fn a_list_that_expands_to_nothing_leaves_the_pattern_as_it_is() {
        assert_eq!(
            selected("[{,}].md", &["{.md", ",.md", "}.md", "a.md"]),
            ["{.md", ",.md", "}.md"]
        );
    }

    /// A group with no comma that the expansion leaves matches as literal
    /// braces: OpenSpec matches with brace expansion off, so `{draft}` names the
    /// characters `{draft}` and not the word `draft`.
    #[test]
    fn a_group_with_no_comma_matches_as_literal_braces() {
        assert_eq!(
            selected("{draft}*.md", &["{draft}x.md", "draftx.md"]),
            ["{draft}x.md"]
        );
    }

    /// A glob that OpenSpec 1.15.0 refuses with an error of its own selects no
    /// file, so the `tasks.md` fallback counts (spec 3.3): braces nested past 16
    /// levels, and an alternative that leaves the change folder. The same globs
    /// within the limits select as usual.
    #[test]
    fn a_glob_that_openspec_refuses_counts_the_tasks_file() {
        let nest = |depth: usize| format!("{}b{}.md", "{a,".repeat(depth), "}".repeat(depth));
        let counted = |generates: &str| {
            let schemas = [("custom", schema(generates, None))];
            files_of(tracked(
                Some("schema: custom\n"),
                None,
                &schemas,
                &["tasks.md", "a.md", "w/t.md"],
            ))
        };
        let got = [
            nest(16),
            nest(17),
            "{v,w}/t.md".to_string(),
            "{..,w}/t.md".to_string(),
        ]
        .map(|generates| counted(&generates).join(" "));

        assert_eq!(got, ["a.md", "tasks.md", "w/t.md", "tasks.md"]);
    }

    /// A pattern that expands to at most 1,024 patterns selects as usual; one
    /// that expands to more selects no file, as OpenSpec refuses it (spec 3.3).
    #[test]
    fn a_brace_expansion_past_the_bound_selects_no_file() {
        let within = format!("{}.md", "{a,b}".repeat(10));
        let past = format!("{}.md", "{a,b}".repeat(11));

        assert_eq!(
            selected(&within, &["aaaaaaaaaa.md", "c.md"]),
            ["aaaaaaaaaa.md"]
        );
        assert_eq!(
            selected(&past, &["aaaaaaaaaaa.md", "c.md"]),
            Vec::<String>::new()
        );
    }

    /// A pattern with no wildcard names one file; a `./` before it and a doubled
    /// slash inside it change nothing.
    #[test]
    fn a_literal_pattern_names_one_file() {
        let files = ["plan/other.md", "plan/tasks.md", "tasks.md"];

        assert_eq!(selected("plan/tasks.md", &files), ["plan/tasks.md"]);
        assert_eq!(selected("./plan/tasks.md", &files), ["plan/tasks.md"]);
        assert_eq!(selected("plan//tasks.md", &files), ["plan/tasks.md"]);
        assert_eq!(
            selected("./plan/*.md", &files),
            ["plan/other.md", "plan/tasks.md"]
        );
        assert_eq!(
            selected("{draft}.md", &["{draft}.md", "draft.md"]),
            ["{draft}.md"]
        );
    }

    /// A brace range and an extglob are not expanded (spec 3.3): no file of the
    /// change matches them, so the count falls back to `tasks.md`.
    #[test]
    fn a_brace_range_and_an_extglob_expand_to_nothing() {
        let files = [
            "task1.md", "task2.md", "task3.md", "a.md", "b.md", "tasks.md",
        ];
        let schemas = [
            ("range", schema("task{1..3}.md", None)),
            ("ext", schema("+(a|b).md", None)),
        ];

        for name in ["range", "ext"] {
            assert_eq!(
                files_of(tracked(
                    Some(&format!("schema: {name}\n")),
                    None,
                    &schemas,
                    &files
                )),
                ["tasks.md"],
                "{name}"
            );
        }
    }

    /// A glob that does not compile selects no file, as picomatch's never-matching
    /// pattern does (spec 3.3): the count falls back to `tasks.md`, and no line
    /// names the glob.
    #[test]
    fn a_glob_that_does_not_compile_selects_no_file() {
        for generates in ["a[z-a]b", "a["] {
            let schemas = [("custom", schema(generates, None))];

            assert_eq!(
                files_of(tracked(
                    Some("schema: custom\n"),
                    None,
                    &schemas,
                    &["tasks.md", "a.md"]
                )),
                ["tasks.md"],
                "{generates}"
            );
        }
    }

    /// The files come in the order of the input, which the caller sorts.
    #[test]
    fn the_files_come_in_the_order_of_the_input() {
        let files = ["work/c.md", "work/a.md", "work/b.md"];

        assert_eq!(
            selected("work/*.md", &files),
            ["work/c.md", "work/a.md", "work/b.md"]
        );
    }
}
