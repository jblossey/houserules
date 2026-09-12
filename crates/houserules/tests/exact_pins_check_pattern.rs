//! Compiles the shipped `security-hygiene.exact-pins` knowledge entry's
//! `check.pattern` -- the deterministic gate every `package.json`/
//! `Cargo.toml` change runs under `houserules audit` -- with `regress`, the
//! engine `rules::audit::compile_check_regex` uses in production, then
//! asserts it matches and rejects a fixed set of sample lines.
//! `compile_check_regex` is private to the `audit` module and unreachable
//! from an integration test, so this file calls `regress::Regex::with_flags`
//! directly instead of linking to it. The pattern is read live from the
//! shipped payload file, not copied inline, so an edit to the entry's
//! `check.pattern` changes what this test compiles the same way it changes
//! what `houserules audit` compiles.

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
/// security-hygiene.json` since this file has no knowledge-base loader of
/// its own to reuse. `flags` defaults to an empty string when absent,
/// matching `compile_check_regex`'s own `&str` parameter (never `Option`).
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

/// Checks 12 matching and 8 non-matching sample lines against the compiled
/// pattern. `regress::Regex::find` returns `Some` on any match anywhere in
/// `line`, since the pattern carries no `^`/`$` anchor.
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
