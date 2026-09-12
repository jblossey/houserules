//! The glob matcher: `area_files`/`areas_for` group and resolve paths
//! against `knowledge/areas.json`'s declared globs; `audit` and
//! `read::for_result` are the production callers, `audit` also calling
//! `glob_match` directly for its checks' own glob matching. `compile` and
//! `GlobError` also serve `model::load_areas`, which validates every
//! area's globs at load time.
//!
//! globset is the single matching engine (`houserules.glob-union-matcher`).
//! Malformed globs are named errors, never panics. Extglob syntax
//! (`+(...)`, `!(...)`, `@(...)`) is not in the vocabulary: globset treats
//! the parens/pipe/bang/at as literal characters. Nested brace lists
//! match correctly (`a/{b,{c,d}}/c`). A bracket class, brace list, or `?`
//! crosses a leading-dot path segment under `**` (`src/[ab]/**` matches
//! `src/a/.x/y`).
//!
//! `GlobBuilder::literal_separator(true)` is set explicitly: without it,
//! globset's own default lets a bare `*` cross `/`.

use std::fmt;

use globset::GlobBuilder;
use indexmap::IndexMap;

use super::model::AreaDef;

/// Removes a single leading `./` from `path` before matching, so a path
/// is matched relative-clean even when a caller passes it
/// `git diff --name-only` style with a leading `./`. `pub(super)`:
/// `read::for_result` needs the identical normalization for its own
/// `paths`/`wanted`/`verify` comparison, and a second hand-written copy
/// could drift from this one silently.
pub(super) fn strip_dot(path: &str) -> &str {
    path.strip_prefix("./").unwrap_or(path)
}

/// A glob that failed to compile, naming the offending pattern and the
/// underlying globset error -- never a panic. `Display` reads as one line
/// suitable for a CLI error surface (`render`'s named-error, exit-2
/// contract).
#[derive(Debug)]
pub(crate) struct GlobError {
    glob: String,
    source: globset::Error,
}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid glob {:?}: {}", self.glob, self.source)
    }
}

impl std::error::Error for GlobError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Compiles `glob` into a matcher with a literal path separator (`*` stops
/// at `/`; only `**` crosses it), or a named `GlobError` when `glob` fails
/// to compile: `a[z-a]b` (a descending range) errors this way. `a[[]b` is
/// valid POSIX bracket-class syntax -- a class containing the single
/// literal character `[` -- and compiles and matches fine, not an error.
pub(crate) fn compile(glob: &str) -> Result<globset::GlobMatcher, GlobError> {
    GlobBuilder::new(glob)
        .literal_separator(true)
        .build()
        .map(|g| g.compile_matcher())
        .map_err(|source| GlobError {
            glob: glob.to_string(),
            source,
        })
}

/// Matches `path` against `glob`, or the `GlobError` `compile` returns
/// when `glob` fails to compile -- never a panic. `area_files` calls it
/// internally; `audit` also calls it directly for the
/// `report-field`/`grep-absent`/`co-change`/`diff-append-only` checks'
/// own glob matching.
pub(crate) fn glob_match(path: &str, glob: &str) -> Result<bool, GlobError> {
    compile(glob).map(|matcher| matcher.is_match(path))
}

/// Groups `paths` by every area whose globs match, each area mapped to
/// the paths that matched it, in insertion order: `global` first (it has no
/// globs of its own but applies to every path), then every other area in
/// the order its FIRST matching path touches it. This key order is
/// observable in `audit`'s own `area_files` JSON field, so `IndexMap`
/// (`.entry().or_default()` preserves first-touch order) matters here:
/// `serde_json::Value` equality under this crate's `preserve_order`
/// feature ignores object key order, but a live `houserules
/// audit --json` run does not. Stops at the first `GlobError` a glob
/// raises: an area whose earlier glob already matched never reaches a
/// later, possibly-malformed one. `audit`'s rule-package assembly (its
/// touched-areas computation) and `areas_for` (below, the `for` command's
/// own caller) are its production callers.
pub(crate) fn area_files(
    paths: &[&str],
    areas: &[(String, AreaDef)],
) -> Result<IndexMap<String, Vec<String>>, GlobError> {
    let mut found: IndexMap<String, Vec<String>> = IndexMap::new();
    found.insert("global".to_string(), Vec::new());
    for &path in paths {
        let rel = strip_dot(path);
        for (area, def) in areas {
            let mut matched = false;
            for glob in &def.paths {
                if glob_match(rel, glob)? {
                    matched = true;
                    break;
                }
            }
            if matched {
                found
                    .entry(area.clone())
                    .or_default()
                    .push(path.to_string());
            }
        }
    }
    Ok(found)
}

/// Resolves `paths` to their areas through the glob map; `global` always
/// applies. `read::for_result` is its production caller.
pub(crate) fn areas_for(
    paths: &[&str],
    areas: &[(String, AreaDef)],
) -> Result<Vec<String>, GlobError> {
    let mut names: Vec<String> = area_files(paths, areas)?.into_keys().collect();
    names.sort();
    Ok(names)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::model::load_areas;
    use super::*;

    fn areas(pairs: &[(&str, &[&str])]) -> Vec<(String, AreaDef)> {
        pairs
            .iter()
            .map(|(name, paths)| {
                (
                    name.to_string(),
                    AreaDef {
                        paths: paths.iter().map(|p| p.to_string()).collect(),
                    },
                )
            })
            .collect()
    }

    /// Maps paths to areas through the globs, always including `global`,
    /// sorted and deduplicated.
    #[test]
    fn areas_for_maps_paths_through_globs_always_including_global_sorted_deduplicated() {
        let areas = areas(&[
            ("global", &[]),
            ("process", &[]),
            ("rust", &["crates/**", "Cargo.toml"]),
            ("webview", &["apps/desktop/src/**"]),
            ("api", &["apps/api/**"]),
            ("schemas", &["packages/schemas/**"]),
            ("infra", &["tools/**", ".github/**"]),
            ("docs", &["docs/**", "CLAUDE.md"]),
        ]);
        assert_eq!(
            areas_for(
                &[
                    "./crates/x/src/a.rs",
                    "Cargo.toml",
                    "docs/a.md",
                    "README.md"
                ],
                &areas
            )
            .unwrap(),
            vec!["docs", "global", "rust"],
        );
        assert_eq!(areas_for(&["README.md"], &areas).unwrap(), vec!["global"]);
        assert_eq!(
            areas_for(&["docs/x.md"], &areas).unwrap(),
            vec!["docs", "global"]
        );
        assert_eq!(areas_for(&[], &areas).unwrap(), vec!["global"]);
    }

    /// Includes `template` for a path under `template/.claude`, crossing
    /// the dot-segment, against this repository's own real
    /// `knowledge/areas.json`.
    #[test]
    fn areas_for_includes_template_crossing_the_dot_segment_hr_019() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let areas = load_areas(&root.join("knowledge/areas.json")).expect("areas.json loads");
        assert!(
            areas_for(&["template/.claude/agents/implementer.md"], &areas)
                .unwrap()
                .contains(&"template".to_string())
        );
    }

    /// Still matches `?`, bracket-class, and brace-list globs, not only
    /// `**` and `*`.
    #[test]
    fn areas_for_still_matches_question_bracket_class_and_brace_list_globs() {
        let vocab = areas(&[
            ("global", &[]),
            ("question", &["crates/?.rs"]),
            ("bracket", &["src/*.[jt]s"]),
            ("brace", &["src/*.{js,ts}"]),
        ]);
        assert!(
            areas_for(&["crates/x.rs"], &vocab)
                .unwrap()
                .contains(&"question".to_string())
        );
        assert!(
            areas_for(&["src/a.ts"], &vocab)
                .unwrap()
                .contains(&"bracket".to_string())
        );
        assert!(
            areas_for(&["src/a.ts"], &vocab)
                .unwrap()
                .contains(&"brace".to_string())
        );
    }

    /// Groups changed files by every area their globs match, plus
    /// `global` always empty.
    #[test]
    fn area_files_groups_changed_files_by_every_area_their_globs_match() {
        let areas = areas(&[
            ("global", &[]),
            ("process", &[]),
            ("rust", &["crates/**", "Cargo.toml"]),
            ("webview", &["apps/desktop/src/**"]),
            ("api", &["apps/api/**"]),
            ("schemas", &["packages/schemas/**"]),
            ("infra", &["tools/**", ".github/**"]),
            ("docs", &["docs/**", "CLAUDE.md"]),
        ]);
        let result = area_files(&["docs/x.md", "tools/a.mjs"], &areas).unwrap();
        let expected: IndexMap<String, Vec<String>> = [
            ("global".to_string(), vec![]),
            ("docs".to_string(), vec!["docs/x.md".to_string()]),
            ("infra".to_string(), vec!["tools/a.mjs".to_string()]),
        ]
        .into_iter()
        .collect();
        assert_eq!(result, expected);
        assert_eq!(
            result.keys().collect::<Vec<_>>(),
            vec!["global", "docs", "infra"],
            "insertion order must be global first, then each area in first-touch order"
        );

        let empty = area_files(&[], &areas).unwrap();
        let expected_empty: IndexMap<String, Vec<String>> =
            [("global".to_string(), vec![])].into_iter().collect();
        assert_eq!(empty, expected_empty);
    }

    /// `area_files` orders its keys by first-touch, not sorted: `global,
    /// infra, docs` for this fixture (the first path, `tools/a.mjs`,
    /// touches `infra` before the second path, `docs/x.md`, touches
    /// `docs`), which is neither insertion order by area declaration nor
    /// alphabetical.
    #[test]
    fn area_files_key_order_follows_first_touch_by_path_order_not_area_declaration_order() {
        let areas = areas(&[
            ("global", &[]),
            ("docs", &["docs/**"]),
            ("infra", &["tools/**"]),
        ]);
        let result = area_files(&["tools/a.mjs", "docs/x.md"], &areas).unwrap();
        assert_eq!(
            result.keys().collect::<Vec<_>>(),
            vec!["global", "infra", "docs"]
        );
    }

    /// Nested brace lists match correctly: `a/{b,{c,d}}/c` matches
    /// `a/c/c`.
    #[test]
    fn glob_match_supports_nested_brace_lists() {
        assert!(glob_match("a/c/c", "a/{b,{c,d}}/c").unwrap());
    }

    /// Extglob is out of the vocabulary: globset treats `+`, `(`, `)`,
    /// `|` as literal characters, so `src/+(x|y).js` does not match
    /// `src/x.js`.
    #[test]
    fn glob_match_leaves_plus_extglob_out_of_the_vocabulary() {
        assert!(!glob_match("src/x.js", "src/+(x|y).js").unwrap());
    }

    /// globset's `!` extglob form is not in the vocabulary either:
    /// `src/!(y).js` does not match `src/x.js`.
    #[test]
    fn glob_match_leaves_bang_extglob_out_of_the_vocabulary() {
        assert!(!glob_match("src/x.js", "src/!(y).js").unwrap());
    }

    /// globset's `@` extglob form is not in the vocabulary either:
    /// `src/@(x|y).js` does not match `src/x.js`.
    #[test]
    fn glob_match_leaves_at_extglob_out_of_the_vocabulary() {
        assert!(!glob_match("src/x.js", "src/@(x|y).js").unwrap());
    }

    /// A bracket class crosses a leading-dot segment under `**`:
    /// `src/[ab]/**` matches `src/a/.x/y`.
    #[test]
    fn glob_match_crosses_a_dot_segment_after_a_bracket_class() {
        assert!(glob_match("src/a/.x/y", "src/[ab]/**").unwrap());
    }

    /// The `?` sibling of the bracket-class dot-segment crossing above:
    /// `src/?/**` matches `src/a/.x/y`.
    #[test]
    fn glob_match_crosses_a_dot_segment_after_a_question_mark() {
        assert!(glob_match("src/a/.x/y", "src/?/**").unwrap());
    }

    /// The brace-list sibling of the bracket-class dot-segment crossing
    /// above: `src/{a,b}/**` matches `src/a/.x/y`.
    #[test]
    fn glob_match_crosses_a_dot_segment_after_a_brace_list() {
        assert!(glob_match("src/a/.x/y", "src/{a,b}/**").unwrap());
    }

    /// A malformed descending range is a named error, never a panic:
    /// `Glob::new("a[z-a]b")` returns `Err`.
    #[test]
    fn glob_match_names_a_descending_range_as_an_error_not_a_panic() {
        let error = glob_match("a[b", "a[z-a]b").unwrap_err();
        assert!(error.to_string().contains("a[z-a]b"));
    }

    /// `a[[]b` looks malformed (an unclosed `[` inside a class) but is
    /// valid POSIX bracket-class syntax -- a class containing the single
    /// literal character `[` -- so it does not panic, and correctly
    /// compiles and matches: `Glob::new("a[[]b")` is `Ok`, and `"a[b"`
    /// matches it (`[` from the class, then `b`).
    #[test]
    fn glob_match_treats_a_bracket_literal_class_as_valid_not_a_panic() {
        assert!(glob_match("a[b", "a[[]b").unwrap());
    }
}
