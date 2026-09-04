//! The glob matcher: `tools/kb.mjs`'s `globMatch` ported for `area_files`/
//! `areas_for`, which `audit` (kb.mjs:707) and `cmdFor` (kb.mjs:927) call
//! in the frozen JS source -- both phase-2 surfaces (docs/specs/
//! 2026-09-04-batch-15-tier2-spec.md §5). Neither `render` nor this
//! phase's `check-knowledge` task calls the matcher: `renderAll` never
//! matches a glob, and `checkBase` does not call `areaFiles`/`areasFor`
//! either. So `glob_match`/`area_files`/`areas_for` stay `#[allow(dead_code)]`
//! through the rest of phase 1; phase 2 wires them into `audit` and `for`.
//! `compile` and `GlobError` are not dead: `model::load_areas` calls
//! `compile` to validate every area's globs at load time (see its doc).
//!
//! Ruled 2026-09-04 (design.md §5.25, decisions.json, the
//! `houserules.glob-union-matcher` gotcha's fourth bullet, this spec's §3
//! glob bullet): the globset crate is the single matching engine, not a
//! port of `tools/kb.mjs`'s two-engine union (`matchesGlob(path, glob) ||
//! globToRegExp(glob).test(path)`). Every divergence from that frozen
//! union is pinned by a counterexample test asserting globset's answer,
//! named against the union's in the test's doc comment; malformed globs
//! are named errors, never panics; extglob (`+(...)`, `!(...)`, `@(...)`)
//! is not in the vocabulary. Verified live against globset 0.4.20 and the
//! frozen union (node 24.18.1 at a73a8c6) for every glob this repository's
//! `knowledge/areas.json` actually declares (`**`, `*`, literals): the
//! answers agree. The known divergences, each pinned below:
//! - Extglob narrows: the union treats `+(...)`/`!(...)`/`@(...)` as
//!   matching (verified live), globset treats the parens/pipe/bang/at as
//!   literal characters and does not.
//! - A bracket class, brace list, or `?` crossing a dot-segment under
//!   `**` widens: the union's `matchesGlob` half excludes a leading-dot
//!   segment there (the gotcha's original subject); globset does not.
//! - Nested brace lists now match, correctly: globset supports them
//!   natively, unlike this module's first cut (a hand-rolled translator
//!   whose brace parser did not nest -- fix round 1, finding 1).
//!
//! `GlobBuilder::literal_separator(true)` is set explicitly: without it,
//! globset's own default lets a bare `*` cross `/` (verified live), which
//! the union's `*` never does (`globToRegExp` translates it to `[^/]*`).

use std::collections::HashMap;
use std::fmt;

use globset::GlobBuilder;

use super::model::AreaDef;

/// Removes a single leading `./` from `path`, the same normalization
/// `tools/kb.mjs`'s `stripDot` applies before matching (a path is matched
/// relative-clean even when a caller passes it `git diff --name-only`
/// style with a leading `./`).
#[allow(dead_code)]
fn strip_dot(path: &str) -> &str {
    path.strip_prefix("./").unwrap_or(path)
}

/// A glob that failed to compile, naming the offending pattern and the
/// underlying globset error -- never a panic. `Display` reads as one line
/// suitable for a CLI error surface (`render`'s named-error, exit-2
/// contract, docs/specs/2026-09-04-batch-15-tier2-spec.md §6).
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
/// to compile. Verified live with globset 0.4.20: `a[z-a]b` (a descending
/// range) errors this way; `a[[]b` is valid POSIX bracket-class syntax --
/// a class containing the single literal character `[` -- and compiles
/// and matches fine, contrary to this module's first cut, which panicked
/// building a `regex` pattern for both (fix round 1, finding 2).
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
/// when `glob` fails to compile -- never a panic. The one matcher
/// `area_files` calls, and the one phase 2's `audit`/`cmdFor` will call.
#[allow(dead_code)]
pub(crate) fn glob_match(path: &str, glob: &str) -> Result<bool, GlobError> {
    compile(glob).map(|matcher| matcher.is_match(path))
}

/// Groups `paths` by every area whose globs match, each area mapped to
/// the paths that matched it. `global` always appears, mapped to an empty
/// list: it has no globs of its own but applies to every path. Stops at
/// the first `GlobError` a glob raises, matching `.some()`'s short-circuit
/// on the JS side: an area whose earlier glob already matched never
/// reaches a later, possibly-malformed one.
#[allow(dead_code)]
pub(crate) fn area_files(
    paths: &[&str],
    areas: &[(String, AreaDef)],
) -> Result<HashMap<String, Vec<String>>, GlobError> {
    let mut found: HashMap<String, Vec<String>> = HashMap::new();
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

/// Resolves `paths` to their areas through the glob map; `global` always applies.
#[allow(dead_code)]
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

    /// tests/kb.test.mjs, describe('areasFor'): "maps paths to areas
    /// through the globs, always including global, sorted and
    /// deduplicated".
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

    /// tests/kb.test.mjs, describe('areasFor'): "includes template for a
    /// path under template/.claude, crossing the dot-segment" (HR-019),
    /// against this repository's own real `knowledge/areas.json`.
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

    /// tests/kb.test.mjs, describe('areasFor'): "still matches ?,
    /// bracket-class, and brace-list globs, not only ** and *". Re-verified
    /// under globset after the fix round 1 engine swap (finding 1): still
    /// true on all three.
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

    /// tests/kb.test.mjs, describe('areaFiles'): "groups changed files by
    /// every area their globs match, plus global always empty".
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
        let expected: HashMap<String, Vec<String>> = [
            ("docs".to_string(), vec!["docs/x.md".to_string()]),
            ("global".to_string(), vec![]),
            ("infra".to_string(), vec!["tools/a.mjs".to_string()]),
        ]
        .into_iter()
        .collect();
        assert_eq!(result, expected);

        let empty = area_files(&[], &areas).unwrap();
        let expected_empty: HashMap<String, Vec<String>> =
            [("global".to_string(), vec![])].into_iter().collect();
        assert_eq!(empty, expected_empty);
    }

    /// Fix round 1, finding 1: the owner's globset ruling names nested
    /// brace lists as a case where globset must now match the frozen JS
    /// union's answer (`matchesGlob('a/c/c', 'a/{b,{c,d}}/c')` is `true`
    /// at the frozen sha, verified live), unlike this module's first cut:
    /// `find_brace_end` stopped at the first `}`, so `a/{b,{c,d}}/c`
    /// compiled to `a/(?:b|\{c|d)\}/c`, which did not match. This is the
    /// one instance of the review's five where the old engine's own
    /// answer actually diverged from the chosen one, so it is the natural
    /// RED for this fix round: it failed against the old `bool`-returning
    /// glob_match before the globset swap.
    #[test]
    fn glob_match_supports_nested_brace_lists() {
        assert!(glob_match("a/c/c", "a/{b,{c,d}}/c").unwrap());
    }

    /// Fix round 1, finding 1, review issue 1, divergence 1 of 3: the
    /// frozen union treats extglob as matching
    /// (`matchesGlob('src/x.js', 'src/+(x|y).js')` is `true`, verified
    /// live on node 24.18.1 at a73a8c6). The owner's ruling takes extglob
    /// out of the vocabulary; globset treats `+`, `(`, `)`, `|` as literal
    /// characters, so this pins globset's answer: `false`.
    #[test]
    fn glob_match_leaves_plus_extglob_out_of_the_vocabulary() {
        assert!(!glob_match("src/x.js", "src/+(x|y).js").unwrap());
    }

    /// Fix round 1, finding 1, divergence 2 of 3: the union's
    /// `matchesGlob('src/x.js', 'src/!(y).js')` is `true` (verified live);
    /// globset's `!` extglob form is not in the vocabulary either. Pins
    /// globset's answer: `false`.
    #[test]
    fn glob_match_leaves_bang_extglob_out_of_the_vocabulary() {
        assert!(!glob_match("src/x.js", "src/!(y).js").unwrap());
    }

    /// Fix round 1, finding 1, divergence 3 of 3: the union's
    /// `matchesGlob('src/x.js', 'src/@(x|y).js')` is `true` (verified
    /// live); globset's `@` extglob form is not in the vocabulary either.
    /// Pins globset's answer: `false`.
    #[test]
    fn glob_match_leaves_at_extglob_out_of_the_vocabulary() {
        assert!(!glob_match("src/x.js", "src/@(x|y).js").unwrap());
    }

    /// Fix round 1, finding 1, review issue 1, "widens" divergence: the
    /// union's `matchesGlob` half excludes a dot-segment under `**`
    /// (`houserules.glob-union-matcher`'s original subject), so
    /// `matchesGlob('src/a/.x/y', 'src/[ab]/**')` is `false` (verified
    /// live). globset has no such exclusion: this pins globset's answer,
    /// `true`, the deliberately chosen divergence.
    #[test]
    fn glob_match_crosses_a_dot_segment_after_a_bracket_class() {
        assert!(glob_match("src/a/.x/y", "src/[ab]/**").unwrap());
    }

    /// Fix round 2, finding 1: the `?` sibling of the bracket-class
    /// dot-segment crossing above. The union's `matchesGlob` half excludes
    /// a leading-dot segment under `**`, so
    /// `matchesGlob('src/a/.x/y', 'src/?/**')` is `false` (verified live);
    /// globset has no such exclusion: this pins globset's answer, `true`.
    #[test]
    fn glob_match_crosses_a_dot_segment_after_a_question_mark() {
        assert!(glob_match("src/a/.x/y", "src/?/**").unwrap());
    }

    /// Fix round 2, finding 1: the brace-list sibling of the bracket-class
    /// dot-segment crossing above. The union's `matchesGlob` half excludes
    /// a leading-dot segment under `**`, so
    /// `matchesGlob('src/a/.x/y', 'src/{a,b}/**')` is `false` (verified
    /// live); globset has no such exclusion: this pins globset's answer,
    /// `true`. With this and the two tests above, the review's 1853-cell
    /// matrix closes 7/7: every divergence between globset and the frozen
    /// union over this repository's globs and vocabulary is now pinned.
    #[test]
    fn glob_match_crosses_a_dot_segment_after_a_brace_list() {
        assert!(glob_match("src/a/.x/y", "src/{a,b}/**").unwrap());
    }

    /// Fix round 1, finding 2: a malformed descending range must be a
    /// named error, never a panic. Verified live with globset 0.4.20:
    /// `Glob::new("a[z-a]b")` returns `Err`.
    #[test]
    fn glob_match_names_a_descending_range_as_an_error_not_a_panic() {
        let error = glob_match("a[b", "a[z-a]b").unwrap_err();
        assert!(error.to_string().contains("a[z-a]b"));
    }

    /// Fix round 1, finding 2: `a[[]b` looks malformed (an unclosed `[`
    /// inside a class) but is valid POSIX bracket-class syntax -- a class
    /// containing the single literal character `[` -- so it must not
    /// panic either, and here it correctly compiles and matches. Verified
    /// live with globset 0.4.20: `Glob::new("a[[]b")` is `Ok`, and
    /// `"a[b"` matches it (`[` from the class, then `b`).
    #[test]
    fn glob_match_treats_a_bracket_literal_class_as_valid_not_a_panic() {
        assert!(glob_match("a[b", "a[[]b").unwrap());
    }
}
