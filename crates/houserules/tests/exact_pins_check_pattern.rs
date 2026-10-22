//! Compiles the shipped `security-hygiene.exact-pins` knowledge entry's
//! `check.pattern` -- the deterministic gate every `package.json`/
//! `Cargo.toml` change runs under `houserules audit` -- with `regress`, the
//! engine `rules::runner::compile_check_regex` uses in production, then
//! asserts it matches and rejects a fixed set of sample lines.
//! `compile_check_regex` is private to the `runner` module and unreachable
//! from an integration test, so this file calls `regress::Regex::with_flags`
//! directly instead of linking to it. The pattern is read live from the
//! shipped payload file, not copied inline, so an edit to the entry's
//! `check.pattern` changes what this test compiles the same way it changes
//! what `houserules audit` compiles. The cost test times the compiled pattern
//! on a generated `package.json` and a generated `Cargo.toml`.

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
/// `check.flags`, read live from the shipped `template/packs/
/// security-hygiene.json` since this file has no knowledge-base loader of
/// its own to reuse. `flags` defaults to an empty string when absent,
/// matching `compile_check_regex`'s own `&str` parameter (never `Option`).
fn exact_pins_check_pattern() -> (String, String) {
    let path = repo_root().join("template/packs/security-hygiene.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let pack: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    let entries = pack["entries"]
        .as_array()
        .expect("the security-hygiene pack has an entries array");
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

/// The line count of each generated manifest of the cost test.
const GENERATED_LINES: usize = 3000;

/// A generated `package.json` of `GENERATED_LINES` lines: exact dependency
/// pins and, on the last dependency line, a caret range. Returns the text and
/// the line of the range.
fn generated_package_json() -> (String, usize) {
    let mut text = String::from("{\n  \"dependencies\": {\n");
    let exact = GENERATED_LINES - 5;
    for index in 0..exact {
        text.push_str(&format!("    \"dep-{index}\": \"1.{index}.0\",\n"));
    }
    text.push_str("    \"dep-last\": \"^1.0.0\"\n  }\n}\n");
    (text, GENERATED_LINES - 2)
}

/// A generated `Cargo.toml` of `GENERATED_LINES` lines: a `[features]` table
/// whose values are arrays, so no value starts with a digit, then a `[dependencies]` table with an exact pin and,
/// on its last line, a bare range. Returns the text and the line of the range.
fn generated_cargo_toml() -> (String, usize) {
    let mut text = String::from("[package]\nname = \"big\"\nversion = \"0.1.0\"\n\n[features]\n");
    let features = GENERATED_LINES - 9;
    for index in 0..features {
        text.push_str(&format!("f{index} = [\"dep:a{index}\"]\n"));
    }
    text.push_str("\n[dependencies]\nserde = \"=1.0.200\"\nregex = \"1.11\"\n");
    (text, GENERATED_LINES)
}

/// The line (1-based) where the first match of `regex` in `text` starts.
fn first_match_line(regex: &regress::Regex, text: &str) -> Option<usize> {
    let found = regex.find(text)?;
    Some(text[..found.start()].matches('\n').count() + 1)
}

/// Spec 8.1, the cost test: the pattern scans a generated 3000-line
/// `package.json` and a generated 3000-line `Cargo.toml` of features within two
/// seconds in a debug build, and finds the range on the last line of each. The
/// range on the last line forces a scan of the whole text.
#[test]
fn exact_pins_scans_a_3000_line_manifest_within_two_seconds() {
    let (pattern, flags) = exact_pins_check_pattern();
    let regex = regress::Regex::with_flags(&pattern, flags.as_str())
        .unwrap_or_else(|error| panic!("compile check.pattern {pattern:?}: {error}"));

    for (name, (text, range_line)) in [
        ("package.json", generated_package_json()),
        ("Cargo.toml", generated_cargo_toml()),
    ] {
        assert_eq!(text.lines().count(), GENERATED_LINES, "{name}");
        let started = std::time::Instant::now();
        let line = first_match_line(&regex, &text);
        let elapsed = started.elapsed();
        assert_eq!(line, Some(range_line), "{name}: the line of the range");
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "{name}: the scan took {elapsed:?}"
        );
    }
}
