//! Enumerates every shipped reference to `tools/kb.sh` and `tools/
//! backlog.sh` across `template/` and the Rust source files that hold the
//! CLI's own generated-output literals -- ported from `tools/
//! find-shell-tool-refs.mjs` (batch 18 T5 fix round 1, HR-062, review
//! finding "no-Node ruling"): the owner's standing "no Node tools in this
//! codebase" ruling (docs/design.md decision 30) applies to this script
//! same as it did to `check-report-claims.mjs`, and no interim label like
//! `tools/make-corpus.mjs`'s is available for a script this batch adds
//! from scratch. Dev-only: not shipped in `template/` or the payload, the
//! same status `gen-goldens.rs` has (batch 20 T2, HR-066: `check-report-
//! claims.rs` left this list when it moved behind the flat surface as a
//! shipped subcommand -- `crate::report_claims`'s own module doc has the
//! full account).
//!
//! Bounds the reference rewrite's closure claim
//! (process.closure-claims-carry-enumeration): the pre-rewrite run is the
//! enumeration that bounds the rewrite, the post-rewrite run proves zero
//! remain unexplained.
//!
//! # Scope (the two places a shipped reference can live)
//!
//! Every file under `template/` (recursive), and five Rust source files
//! that hold literals the CLI prints or embeds in generated output:
//! `rules/render.rs`, `rules/check.rs`, `install.rs`, `rules/read.rs`, and
//! `main.rs` (added at fix round 1, review finding "enumeration": its
//! struct doc comment IS `houserules --help`'s "about" text verbatim, so a
//! stale reference there is adopter-visible, not internal history). Every
//! OTHER Rust source file under `crates/` narrates the frozen JS's own,
//! permanently fixed naming for porting-parity documentation -- not an
//! adopter-facing instruction a rewrite could make true or false -- so
//! this script does not search there.
//!
//! `PATTERNS` matches `kb.sh`/`backlog.sh` bare, without requiring a
//! `tools/` prefix (widened at fix round 1, review finding "enumeration"):
//! the pre-rewrite `template/tools/claude-session-start.sh` invoked
//! `"$dir/kb.sh" standing` by relative path, a real shipped reference the
//! prefixed pattern never saw.
//!
//! # Exceptions (found, but deliberately not rewritten)
//!
//! Named here, not hidden in a blanket file skip -- the "zero remaining"
//! claim means "zero UNEXPECTED", not "the search saw nothing":
//!   - `template/tools/kb.mjs` (whole file): the frozen-but-shipped JS
//!     engine, still live-invoked by this repository's own dev tests and
//!     by `tools/make-corpus.mjs`'s frozen-worktree regeneration; stays
//!     shipped-but-inert through phase 4, retires at phase 5.
//!   - `crates/houserules/src/rules/read.rs` (whole file): its one
//!     remaining hit narrates a batch 17 fix's measured JS behaviour, not
//!     an adopter-facing instruction. STANDING_COMMAND itself no longer
//!     needs this exception (fix round 1 rewrote it to the flat form).
//!   - `crates/houserules/src/install.rs`: narrowed at fix round 1 (review
//!     finding "enumeration") to exactly the `RETIRED` constant's own item
//!     doc and declaration, plus the doc and body of its test function --
//!     not the whole file. The prior, broader exception swallowed the two
//!     adopter-facing `next: tools/kb.sh check && tools/backlog.sh check`
//!     println literals this task had to rewrite; this file's own
//!     `install_rs_exempt_lines` states the exact, auditable rule.
//!
//! Usage: `cargo run --quiet --bin find-shell-tool-refs`. Prints every
//! match, then a summary line, then exits 0 if every match is either
//! rewritten-clean or explicitly excepted, exit 1 if any UNEXPECTED match
//! remains.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The two literal substrings a shipped reference to either shell wrapper
/// always contains, whether or not the sentence spells out the leading
/// `tools/` directory.
const PATTERNS: [&str; 2] = ["kb.sh", "backlog.sh"];

/// The one test function `install_rs_exempt_lines` opens a body window
/// for -- named exactly, not matched by substring (fix round 2, review
/// new_breakage: a bare `contains("retired")` also opened a window over
/// the production `delete_retired` and its three `delete_retired_*` unit
/// tests, none of which this file's own doc names).
const RETIRED_TEST_FN: &str = "retired_holds_the_shell_tools_moved_at_t5";

/// The Rust source files holding literals the CLI prints or embeds in
/// generated output -- this module's own doc has the full account of why
/// these five and no others.
const RUST_FILES: [&str; 5] = [
    "crates/houserules/src/rules/render.rs",
    "crates/houserules/src/rules/check.rs",
    "crates/houserules/src/install.rs",
    "crates/houserules/src/rules/read.rs",
    "crates/houserules/src/main.rs",
];

/// One matched line: its repo-relative path, 1-based line number, and
/// trimmed text.
struct Hit {
    file: String,
    line: usize,
    text: String,
}

/// This checkout's repository root, resolved at compile time -- every
/// other `src/bin/*.rs` file's own copy of this helper (`gen-goldens.rs`'s
/// own doc explains why each keeps its own rather than sharing one).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Every regular file under `dir`, recursed, sorted by name at each level
/// (matching the JS original's `readdirSync(...).toSorted()` order).
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read_dir {}: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Every line in `abs_path` containing a `PATTERNS` substring, as `Hit`s
/// with `file` relative to `root`. A file this process cannot read as
/// UTF-8 (a binary asset under `template/`, say) carries no text
/// reference and is silently skipped, matching the JS original.
fn find_matches(root: &Path, abs_path: &Path) -> Vec<Hit> {
    let rel = abs_path
        .strip_prefix(root)
        .expect("abs_path is under root")
        .to_string_lossy()
        .replace('\\', "/");
    let Ok(text) = fs::read_to_string(abs_path) else {
        return Vec::new();
    };
    let mut hits = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        if PATTERNS.iter().any(|pattern| line.contains(pattern)) {
            hits.push(Hit {
                file: rel.clone(),
                line: index + 1,
                text: line.trim().to_string(),
            });
        }
    }
    hits
}

/// The 1-based line numbers in `install.rs`'s own source `text` exempted
/// as "the `RETIRED` constant and its own doc and test lines" (module doc
/// has the full account of why this is narrower than a whole-file
/// exception): the item doc immediately preceding `const RETIRED` plus
/// that declaration itself, and the item doc, `#[test]` attribute, and
/// full body of `RETIRED_TEST_FN` -- named exactly, never matched by
/// substring (fix round 2, review new_breakage: a `contains("retired")`
/// match also opened this same window over the production
/// `fn delete_retired` and its own three `delete_retired_*` unit tests).
///
/// The item-doc walk stops at the first line that is not `///`-prefixed
/// (module-level `//!` doc lines do not count, so the file's general
/// "Deletion" narrative -- several paragraphs above the declaration in the
/// same module doc block -- is never swept in). The test-body walk tracks
/// brace depth from the function's own opening line so it stops exactly
/// at the matching closing brace, not at the next blank line.
fn install_rs_exempt_lines(text: &str) -> HashSet<usize> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut exempt = HashSet::new();

    if let Some(decl_index) = lines.iter().position(|line| line.contains("const RETIRED")) {
        exempt.insert(decl_index + 1);
        let mut above = decl_index;
        while above > 0 && lines[above - 1].trim_start().starts_with("///") {
            exempt.insert(above); // 1-based line number for 0-based index above-1
            above -= 1;
        }
    }

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("fn ") || !trimmed.contains(RETIRED_TEST_FN) {
            continue;
        }
        let mut above = index;
        while above > 0 {
            let previous = lines[above - 1].trim_start();
            if previous.starts_with("///") || previous.starts_with("#[test]") {
                exempt.insert(above);
                above -= 1;
            } else {
                break;
            }
        }
        let mut depth = 0i32;
        let mut opened = false;
        let mut cursor = index;
        loop {
            for ch in lines[cursor].chars() {
                match ch {
                    '{' => {
                        depth += 1;
                        opened = true;
                    }
                    '}' => depth -= 1,
                    _ => {}
                }
            }
            exempt.insert(cursor + 1);
            if opened && depth <= 0 {
                break;
            }
            cursor += 1;
            if cursor >= lines.len() {
                break;
            }
        }
    }

    exempt
}

/// One exception category's name, the reason it stays, and the predicate
/// deciding whether a given hit falls inside it.
struct Exception {
    label: &'static str,
    reason: &'static str,
    matches: fn(&Hit, &str) -> bool,
}

/// The three named, auditable exceptions -- this module's own doc has the
/// full reasoning for each.
fn exceptions() -> Vec<Exception> {
    vec![
        Exception {
            label: "template/tools/kb.mjs",
            reason: "the frozen-but-shipped JS engine, still live-invoked by this repo's own dev \
                tests (tests/kb.test.mjs, tests/backlog.test.mjs, tests/entry.test.mjs) and by \
                tools/make-corpus.mjs's frozen-worktree regeneration; stays shipped-but-inert \
                through phase 4 (spec §4), retires at phase 5.",
            matches: |hit, _file_text| hit.file == "template/tools/kb.mjs",
        },
        Exception {
            label: "crates/houserules/src/rules/read.rs",
            reason: "its one remaining hit narrates a batch 17 fix's measured JS behaviour \
                (porting-parity history, not an adopter-facing instruction) -- the same class of \
                comment every unsearched Rust source file under crates/ carries. STANDING_COMMAND \
                itself no longer needs this exception: fix round 1 rewrote it to the flat form.",
            matches: |hit, _file_text| hit.file == "crates/houserules/src/rules/read.rs",
        },
        Exception {
            label: "crates/houserules/src/install.rs (RETIRED's own doc/test only)",
            reason: "RETIRED and its own item doc and test literally are \
                \"tools/kb.sh\"/\"tools/backlog.sh\" -- FILE PATHS the deletion mechanism removes, \
                not command instructions a rewrite could flip. Narrowed at fix round 1 from a \
                whole-file exception, which wrongly swallowed the two next: println literals this \
                task had to rewrite.",
            matches: |hit, file_text| {
                hit.file == "crates/houserules/src/install.rs"
                    && install_rs_exempt_lines(file_text).contains(&hit.line)
            },
        },
    ]
}

/// Walks `template/` and `RUST_FILES`, classifies every `PATTERNS` hit as
/// excepted or a rewrite target, prints both lists plus the summary line,
/// and exits 1 if any target remains (0 otherwise) -- this module's own
/// doc has the full scope and exception account.
fn main() {
    let root = repo_root();
    let mut all_files = Vec::new();
    walk_files(&root.join("template"), &mut all_files);
    for rust_file in RUST_FILES {
        all_files.push(root.join(rust_file));
    }

    let exceptions = exceptions();
    let mut target: Vec<Hit> = Vec::new();
    let mut excepted: Vec<(Hit, &'static str, &'static str)> = Vec::new();

    for abs_path in &all_files {
        let file_text = fs::read_to_string(abs_path).unwrap_or_default();
        for hit in find_matches(&root, abs_path) {
            match exceptions
                .iter()
                .find(|exception| (exception.matches)(&hit, &file_text))
            {
                Some(exception) => excepted.push((hit, exception.label, exception.reason)),
                None => target.push(hit),
            }
        }
    }

    if !target.is_empty() {
        println!(
            "-- {} reference(s) needing the flat-command rewrite --",
            target.len()
        );
        for hit in &target {
            println!("{}:{}: {}", hit.file, hit.line, hit.text);
        }
    }
    if !excepted.is_empty() {
        println!(
            "\n-- {} excepted reference(s) (deliberately not rewritten) --",
            excepted.len()
        );
        let mut by_label: Vec<(&'static str, &'static str, Vec<&Hit>)> = Vec::new();
        for (hit, label, reason) in &excepted {
            match by_label
                .iter_mut()
                .find(|(existing_label, _, _)| existing_label == label)
            {
                Some((_, _, hits)) => hits.push(hit),
                None => by_label.push((label, reason, vec![hit])),
            }
        }
        for (label, reason, hits) in &by_label {
            println!("{label}: {} hit(s) -- {reason}", hits.len());
            for hit in hits {
                println!("  {}:{}: {}", hit.file, hit.line, hit.text);
            }
        }
    }

    println!(
        "\nsummary: {} files scanned, {} total hits, {} excepted, {} unexpected",
        all_files.len(),
        target.len() + excepted.len(),
        excepted.len(),
        target.len(),
    );
    std::process::exit(if target.is_empty() { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal install.rs-shaped fixture: `RETIRED`'s own doc and
    /// declaration, then a production `fn delete_retired` (the same
    /// doc-then-signature-then-body shape as the reviewer's measured HEAD
    /// window, install.rs:448-464), then the one test this file's own doc
    /// names. Built as a standalone string, not read from the real
    /// install.rs, so this test does not drift with unrelated edits there.
    const FIXTURE: &str = "\
/// The two paths `update` deletes from an existing install, in call
/// order (`delete_retired`'s own doc explains why the order matters).
const RETIRED: &[&str] = &[\"tools/kb.sh\", \"tools/backlog.sh\"];

/// Deletes every path in `retired` that exists under `target`, in call
/// order, and returns the ones actually removed.
fn delete_retired(target: &Path, retired: &[&str]) -> Result<Vec<String>, String> {
    let mut deleted = Vec::new();
    for path in retired {
        let full = target.join(path);
        if full.exists() {
            fs::remove_file(&full).map_err(|error| error.to_string())?;
            deleted.push((*path).to_string());
        }
    }
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    /// Pins `RETIRED`'s own contents and call order.
    #[test]
    fn retired_holds_the_shell_tools_moved_at_t5() {
        assert_eq!(RETIRED, [\"tools/kb.sh\", \"tools/backlog.sh\"]);
    }
}
";

    /// The 1-based line number of the first fixture line containing
    /// `needle`, so these tests assert against the fixture's own text
    /// instead of hand-counted line numbers that would drift silently if
    /// the fixture above ever changes.
    fn line_of(text: &str, needle: &str) -> usize {
        text.split('\n')
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("fixture has no line containing {needle:?}"))
            + 1
    }

    /// Review new_breakage (fix round 2): a `fn ` line matched by
    /// `to_lowercase().contains("retired")` wrongly opened a body window
    /// over the production `fn delete_retired`, not only over
    /// `RETIRED_TEST_FN`. Reverting the fix -- matching that looser
    /// substring again instead of `RETIRED_TEST_FN` exactly -- flips this
    /// test red: the disclosed-mutation RED this task report cites.
    #[test]
    fn delete_retired_is_never_exempt() {
        let exempt = install_rs_exempt_lines(FIXTURE);

        let first_line = line_of(FIXTURE, "/// Deletes every path in `retired`");
        let last_line = line_of(FIXTURE, "Ok(deleted)") + 1; // the fn's closing brace
        for line in first_line..=last_line {
            assert!(
                !exempt.contains(&line),
                "line {line} of fn delete_retired (doc through closing brace) must not be \
                 exempt, got: {exempt:?}"
            );
        }
    }

    /// The fix narrows the match, it does not remove the mechanism:
    /// `RETIRED`'s own declaration and the one named test still open
    /// their windows.
    #[test]
    fn retired_declaration_and_its_named_test_stay_exempt() {
        let exempt = install_rs_exempt_lines(FIXTURE);

        let decl_line = line_of(FIXTURE, "const RETIRED");
        assert!(
            exempt.contains(&decl_line),
            "RETIRED's own declaration must stay exempt"
        );

        let first_line = line_of(FIXTURE, "fn retired_holds_the_shell_tools_moved_at_t5(");
        let last_line = line_of(FIXTURE, "assert_eq!(RETIRED,") + 1; // the test's closing brace
        for line in first_line..=last_line {
            assert!(
                exempt.contains(&line),
                "line {line} of the named test must stay exempt, got: {exempt:?}"
            );
        }
    }
}
