//! Ports `tests/kb.test.mjs`'s `describe('the repository knowledge base')`,
//! "matches a range requirement with the exact-pins pattern, and no glob"
//! (batch 20 T1 fix round 1, HR-047; `.superpowers/sdd/2026-09-07-batch-20/
//! t1-evidence/mapping.md` row 9's corrected disposition): the shipped
//! `security-hygiene.exact-pins` knowledge entry's `check.pattern` is the
//! deterministic gate every `package.json`/`Cargo.toml` change runs under
//! `houserules audit` -- its subject, `template/knowledge/
//! security-hygiene.json`, is a `SEED_ONCE` payload file that does not
//! retire, so the JS unit test pinning it had no home to retire WITH; the
//! review (round 1, finding 1/2) found it dropped uncovered instead.
//!
//! The JS original compiled the pattern with V8's own `RegExp`, an engine
//! the shipped binary never runs; this port is strictly stronger,
//! compiling the same pattern with `regress` -- the engine
//! `rules::audit::compile_check_regex` actually uses in production (that
//! function is private to the `audit` module and unreachable from an
//! integration test, so this file calls `regress::Regex::with_flags`
//! directly, the same call `compile_check_regex`'s own doc names, rather
//! than linking to it). The pattern itself is read live from the real
//! payload file, not copied inline, so an edit to the shipped entry's
//! `check.pattern` changes what this test compiles the same way it would
//! change what `houserules audit` compiles.

use std::path::{Path, PathBuf};

/// This checkout's repository root, resolved at compile time -- this
/// file's own copy of the pattern every `tests/*.rs` file in this crate
/// keeps independently (`install.rs`'s own doc explains why: a file
/// needing none of `mod common;`'s other helpers still warns the rest of
/// that module dead if pulled in just for this one function).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The `security-hygiene.exact-pins` entry's own `check.pattern` and
/// `check.flags`, read live from the shipped `template/knowledge/
/// security-hygiene.json` -- `tests/kb.test.mjs`'s own `loadBase(TEMPLATE_
/// ROOT)` plus `base.entries.get(...)`, read directly here since this file
/// has no knowledge-base loader of its own to reuse. `flags` defaults to
/// an empty string when absent, matching the JS test's own `check.flags ??
/// ''` and `compile_check_regex`'s own `&str` parameter (never `Option`).
fn exact_pins_check_pattern() -> (String, String) {
    let path = repo_root().join("template/knowledge/security-hygiene.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let topic: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    let entries = topic["entries"]
        .as_array()
        .expect("security-hygiene.json has an entries array");
    let entry = entries
        .iter()
        .find(|entry| entry["id"] == "security-hygiene.exact-pins")
        .expect("security-hygiene.exact-pins entry is present");
    let pattern = entry["check"]["pattern"]
        .as_str()
        .expect("security-hygiene.exact-pins.check.pattern is a string")
        .to_string();
    let flags = entry["check"]["flags"].as_str().unwrap_or("").to_string();
    (pattern, flags)
}

/// Ports the JS case's 12 matching and 8 non-matching lines verbatim
/// (`tests/kb.test.mjs:183-208`) against the compiled pattern, the way the
/// JS test called `pattern.test(line)` -- `regress::Regex::find` returns
/// `Some` on any match anywhere in `line` (the pattern carries no `^`/`$`
/// anchor), the same unanchored semantics `RegExp.test` has.
#[test]
fn exact_pins_pattern_matches_the_shipped_lines_regress_compiles() {
    let (pattern, flags) = exact_pins_check_pattern();
    let regex = regress::Regex::with_flags(&pattern, flags.as_str())
        .unwrap_or_else(|error| panic!("compile check.pattern {pattern:?}: {error}"));

    let matching = [
        r#""x": "^1.0.0""#,
        r#""x": "~1.0""#,
        r#""x": ">=1.0.0""#,
        r#""x": "<2""#,
        r#""x": "*""#,
        r#""x": "latest""#,
        r#""x": "1.x""#,
        r#""x": "1.2.x""#,
        r#""^1.2.3""#,
        r#"{"devDependencies":{"x":">=22"}}"#,
        r#"{"dependencies": {"pnpm": ">=9"}}"#,
        r#"{"dependencies": {"x": "22.x"}}"#,
    ];
    for line in matching {
        assert!(regex.find(line).is_some(), "expected a match: {line}");
    }

    let non_matching = [
        r#""files": ["*.md"]"#,
        r#""x": "1.0.0""#,
        r#""main": "./dist/index.js""#,
        r#""*.md""#,
        r#"{"engines":{"node":">=22"},"packageManager":"pnpm@10.16.0"}"#,
        r#"{"engines": {"node": ">=22", "pnpm": ">=10"}}"#,
        "{\n  \"engines\": {\n    \"node\": \">=22\",\n    \"pnpm\": \">=10\"\n  }\n}",
        r#"{"engines": {"node": "22.x"}}"#,
    ];
    for line in non_matching {
        assert!(regex.find(line).is_none(), "expected no match: {line}");
    }
}
