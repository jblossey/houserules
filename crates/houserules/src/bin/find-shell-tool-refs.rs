//! Enumerates every shipped reference to `tools/kb.sh` and `tools/
//! backlog.sh` across `template/` and the Rust source files that hold the
//! CLI's own generated-output literals. Dev-only: not shipped in
//! `template/` or the payload, the same status `gen-goldens.rs` has.
//!
//! Bounds the reference rewrite's closure claim
//! (process.closure-claims-carry-enumeration): the pre-rewrite run is the
//! enumeration that bounds the rewrite, the post-rewrite run proves zero
//! remain unexplained.
//!
//! # Scope (the two places a shipped reference can live)
//!
//! Every file under `template/` (recursive), and four Rust source files
//! that hold literals the CLI prints or embeds in generated output:
//! `rules/render.rs`, `rules/check.rs`, `install.rs`, and `main.rs` (its
//! struct doc comment IS `houserules --help`'s "about" text verbatim, so
//! a stale reference there is adopter-visible). Every OTHER Rust source
//! file under `crates/` is source the compiler and the test suite verify,
//! not instructional prose an adopter follows -- a stale reference there
//! cannot mislead anyone the way an adopter-facing string can -- so this
//! script does not search there.
//!
//! `PATTERNS` matches `kb.sh`/`backlog.sh` bare, without requiring a
//! `tools/` prefix: `template/tools/claude-session-start.sh` can invoke
//! either by relative path (`"$dir/kb.sh" standing`), a real shipped
//! reference a prefixed pattern would miss.
//!
//! # Exceptions (found, but deliberately not rewritten)
//!
//! Named here, not hidden in a blanket file skip -- the "zero remaining"
//! claim means "zero UNEXPECTED", not "the search saw nothing":
//!   - `crates/houserules/src/install.rs`: narrowed to exactly the
//!     `RETIRED` constant's own item doc and declaration, plus the doc and
//!     body of its test function -- not the whole file, since `RETIRED`
//!     and its own item doc and test literally are the two shell-wrapper
//!     paths the deletion mechanism removes (FILE PATHS, not command
//!     instructions a rewrite could flip), and a whole-file exception
//!     would also swallow any other, unrelated hit this file might later
//!     carry. This file's own `install_rs_exempt_lines` states the exact,
//!     auditable rule.
//!
//! Usage: `cargo run --quiet --bin find-shell-tool-refs`. Prints every
//! match, then a summary line, then exits 0 if every match is either
//! rewritten-clean or explicitly excepted, exit 1 if any UNEXPECTED match
//! remains.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// The two literal substrings a shipped reference to either shell wrapper
/// always contains, whether or not the sentence spells out the leading
/// `tools/` directory.
const PATTERNS: [&str; 2] = ["kb.sh", "backlog.sh"];

/// The one test function `install_rs_exempt_lines` opens a body window
/// for -- named exactly, not matched by substring: a bare
/// `contains("retired")` would also open a window over the production
/// `delete_retired` and its three `delete_retired_*` unit tests, none of
/// which this file's own doc names. This constant renames whenever the
/// test does, or its own window silently stops opening.
const RETIRED_TEST_FN: &str = "retired_holds_the_shell_tools_and_the_js_engines_they_fronted";

/// The Rust source files holding literals the CLI prints or embeds in
/// generated output -- this module's own doc has the full account of why
/// these four and no others.
const RUST_FILES: [&str; 4] = [
    "crates/houserules/src/rules/render.rs",
    "crates/houserules/src/rules/check.rs",
    "crates/houserules/src/install.rs",
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

/// Every regular file under `dir`, recursed, sorted by name at each
/// level: `WalkDir` sorts each directory's own entries before descending,
/// and follows a symlinked directory. An entry `WalkDir` cannot read is a
/// named, fatal error, never a silent skip
/// (`houserules.crash-paths-are-named`); a non-regular entry (a socket,
/// FIFO, or device file) is skipped rather than scanned, since only
/// `entry.file_type().is_file()` is pushed. Neither tree this binary
/// walks holds a symlink or a non-regular file today, so no pinned
/// capture exercises either path.
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in WalkDir::new(dir)
        .follow_links(true)
        .sort_by(|a, b| a.file_name().cmp(b.file_name()))
    {
        let entry = entry.unwrap_or_else(|error| panic!("walk {}: {error}", dir.display()));
        if entry.file_type().is_file() {
            out.push(entry.into_path());
        }
    }
}

/// Every line in `abs_path` containing a `PATTERNS` substring, as `Hit`s
/// with `file` relative to `root`. A file this process cannot read as
/// UTF-8 (a binary asset under `template/`, say) carries no text
/// reference and is silently skipped.
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
/// substring, since a `contains("retired")` match would also open this
/// same window over the production `fn delete_retired` and its own three
/// `delete_retired_*` unit tests.
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
        let mut above = decl_index;
        while above > 0 && lines[above - 1].trim_start().starts_with("///") {
            exempt.insert(above); // 1-based line number for 0-based index above-1
            above -= 1;
        }
        // Tracking `[`/`]` depth from the declaration's own line, the same
        // technique `RETIRED_TEST_FN`'s body window uses below, keeps
        // every element line -- and the array's own closing `];` --
        // exempt regardless of how many lines the literal spans.
        let mut depth = 0i32;
        let mut opened = false;
        let mut cursor = decl_index;
        loop {
            for ch in lines[cursor].chars() {
                match ch {
                    '[' => {
                        depth += 1;
                        opened = true;
                    }
                    ']' => depth -= 1,
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

/// The one named, auditable exception -- this module's own doc has the
/// full reasoning.
fn exceptions() -> Vec<Exception> {
    vec![Exception {
        label: "crates/houserules/src/install.rs (RETIRED's own doc/test only)",
        reason: "RETIRED and its own item doc and test literally are \
            \"tools/kb.sh\"/\"tools/backlog.sh\" -- FILE PATHS the deletion mechanism removes, \
            not command instructions a rewrite could flip. Narrowed to exactly that window \
            (RETIRED's own doc and test) so a whole-file exception cannot swallow an unrelated \
            hit.",
        matches: |hit, file_text| {
            hit.file == "crates/houserules/src/install.rs"
                && install_rs_exempt_lines(file_text).contains(&hit.line)
        },
    }]
}

/// Every declared exception's own real hit count against `excepted`, in
/// `exceptions`' declared order, `0` included for one that matched
/// nothing this run -- `excepted` alone cannot show that: an exception
/// with zero hits never appears in it at all. `bin/vacuous-exception-
/// gate.rs` reads this printed count to flag exactly that shape (a
/// declared exception the current tree no longer needs).
/// `residue-gate.rs` carries the identical helper -- each `src/bin/*.rs`
/// file keeps its own copy rather than sharing one, since this package
/// has no library target for a `src/bin/*.rs` file to share code through
/// (`gen-goldens.rs`'s own module doc explains why).
fn declared_exception_counts(
    exceptions: &[Exception],
    excepted: &[(Hit, &'static str, &'static str)],
) -> Vec<(&'static str, usize)> {
    let mut counts: Vec<(&'static str, usize)> = exceptions
        .iter()
        .map(|exception| (exception.label, 0))
        .collect();
    for (_, label, _) in excepted {
        if let Some(entry) = counts.iter_mut().find(|(existing, _)| existing == label) {
            entry.1 += 1;
        }
    }
    counts
}

/// Walks `template/` and `RUST_FILES`, classifies every `PATTERNS` hit as
/// excepted or a rewrite target, prints both lists plus the summary line,
/// and exits 1 if any target remains (0 otherwise) -- this module's own
/// doc has the full scope and exception account. This gate's own exit
/// code never depends on whether a declared exception matched zero hits:
/// `bin/vacuous-exception-gate.rs` reads the printed counts below and is
/// the one gate that fails on that condition (HR-115).
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

    let declared_counts = declared_exception_counts(&exceptions, &excepted);
    println!(
        "\n-- {} declared exception(s), by label --",
        declared_counts.len()
    );
    for (label, count) in &declared_counts {
        println!("{label}: {count} hit(s)");
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
    /// doc-then-signature-then-body shape install.rs itself uses), then
    /// the one test this file's own doc names. Built as a standalone
    /// string, not read from the real install.rs, so this test does not
    /// drift with unrelated edits there.
    const FIXTURE: &str = "\
/// The two paths `update` deletes from an existing install, in call
/// order (`delete_retired`'s own doc explains why the order matters).
/// Batch 20 T3 widened this to six paths, one per line, across the JS
/// retirement (HR-047) -- the multi-line shape this fixture now matches.
const RETIRED: &[&str] = &[
    \"tools/kb.sh\",
    \"tools/backlog.sh\",
    \"tools/kb.mjs\",
    \"tools/backlog.mjs\",
    \"tools/lib/cli.mjs\",
    \"tools/lib/json-store.mjs\",
];

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
    fn retired_holds_the_shell_tools_and_the_js_engines_they_fronted() {
        assert_eq!(RETIRED, [\"tools/kb.sh\", \"tools/backlog.sh\", \"tools/kb.mjs\", \"tools/backlog.mjs\", \"tools/lib/cli.mjs\", \"tools/lib/json-store.mjs\"]);
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

    /// A `fn ` line matched by a bare `contains("retired")` substring
    /// test would wrongly open a body window over the production `fn
    /// delete_retired`, not only over `RETIRED_TEST_FN`. Reverting to
    /// that looser match flips this test red.
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

    /// Matching `RETIRED_TEST_FN` by exact name, not by substring, still
    /// lets `RETIRED`'s own declaration and the one named test open
    /// their windows.
    #[test]
    fn retired_declaration_and_its_named_test_stay_exempt() {
        let exempt = install_rs_exempt_lines(FIXTURE);

        let decl_line = line_of(FIXTURE, "const RETIRED");
        assert!(
            exempt.contains(&decl_line),
            "RETIRED's own declaration must stay exempt"
        );

        let first_line = line_of(
            FIXTURE,
            "fn retired_holds_the_shell_tools_and_the_js_engines_they_fronted(",
        );
        let last_line = line_of(FIXTURE, "assert_eq!(RETIRED,") + 1; // the test's closing brace
        for line in first_line..=last_line {
            assert!(
                exempt.contains(&line),
                "line {line} of the named test must stay exempt, got: {exempt:?}"
            );
        }
    }

    /// Every element line of `RETIRED`'s own multi-line array literal,
    /// and the closing `];`, stay exempt: each is FILE-PATH content, the
    /// same kind the declaration's own exemption covers, not a command
    /// instruction a rewrite could flip.
    #[test]
    fn retired_array_continuation_lines_stay_exempt() {
        let exempt = install_rs_exempt_lines(FIXTURE);

        let open_line = line_of(FIXTURE, "const RETIRED");
        let close_line = line_of(FIXTURE, "tools/lib/json-store.mjs") + 1; // the array's own "];"
        for line in open_line..=close_line {
            assert!(
                exempt.contains(&line),
                "line {line} of RETIRED's own multi-line array must stay exempt, got: {exempt:?}"
            );
        }
    }

    /// `declared_exception_counts` names every declared label, in
    /// declared order, with its real count -- `0` for one `excepted`
    /// never carries, proven here against two synthetic labels rather
    /// than this file's own real (currently non-zero) exception.
    #[test]
    fn declared_exception_counts_includes_zero_for_a_label_excepted_names_nothing_for() {
        let declared = vec![
            Exception {
                label: "alpha",
                reason: "r",
                matches: |_, _| false,
            },
            Exception {
                label: "beta",
                reason: "r",
                matches: |_, _| false,
            },
        ];
        let hit = Hit {
            file: "f".to_string(),
            line: 1,
            text: "t".to_string(),
        };
        let excepted = vec![(hit, "alpha", "r")];
        assert_eq!(
            declared_exception_counts(&declared, &excepted),
            vec![("alpha", 1), ("beta", 0)]
        );
    }

    /// `true` when `text` holds a whole `T<digits>` token (a task
    /// reference like `T3`/`T20`) -- split on every non-alphanumeric
    /// byte so `T3` inside a longer word never false-positives, and a
    /// bare `T` (no digits) never counts.
    fn contains_task_reference(text: &str) -> bool {
        text.split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| {
                word.len() >= 2
                    && word.starts_with('T')
                    && word[1..].chars().all(|c| c.is_ascii_digit())
            })
    }

    /// `true` when `text` holds a maximal run of 7-40 ASCII hex
    /// characters -- a git commit sha's own shape, the identical bound
    /// `rules::durable_sha::hex_candidates` uses (kept as its own copy
    /// here: this package has no library target for a `src/bin/*.rs`
    /// file to share code through). A longer run (a sha256 digest, say)
    /// is excluded the same way: it merely looks hex.
    fn contains_hex_sha(text: &str) -> bool {
        let mut run = 0usize;
        for ch in text.chars().chain(std::iter::once(' ')) {
            if ch.is_ascii_hexdigit() {
                run += 1;
            } else {
                if (7..=40).contains(&run) {
                    return true;
                }
                run = 0;
            }
        }
        false
    }

    /// `true` when `text` cites a backlog id as historical NARRATION --
    /// `"per HR-"` (the reviewer's own probe shape, "... per HR-084.")
    /// or `"HR-<digits>,"` (a citation-list shape, "citing HR-115,
    /// HR-047") -- rather than every backlog-id occurrence. A bare
    /// parenthetical pointer, `"(HR-115)"`, is the tree's own established
    /// CURRENT-CONTRACT convention (this file's own `main`'s doc keeps
    /// exactly that shape, and `FIXTURE`'s `HR-047` is fixture data, not
    /// narration) and must not trip this check.
    fn contains_backlog_id_narration(text: &str) -> bool {
        if text.contains("per HR-") {
            return true;
        }
        for (index, _) in text.match_indices("HR-") {
            let after = &text[index + "HR-".len()..];
            let digits = after.chars().take_while(char::is_ascii_digit).count();
            if digits > 0 && after.as_bytes().get(digits) == Some(&b',') {
                return true;
            }
        }
        false
    }

    /// The current, disclosed set of history-narration marker classes a
    /// printed `reason` string must never carry: the literal phrases
    /// `"fix round"`, `"task had"`, and `"this task"`; a `"batch "`
    /// substring; a whole `T<n>` task-reference token; a backlog-id
    /// NARRATIVE citation (`"per HR-"` or `"HR-<n>,"`); and a 7-40
    /// character hex run (a commit sha's own shape -- this also catches
    /// a bare CI run id, since ASCII decimal digits are hex digits too).
    /// Returns the matched class's own name, or `None` when `reason` is
    /// clean.
    ///
    /// Out of scope, by design: HR-106's own sweep (docs/specs/2026-09-
    /// 11-batch-22-stewardship.md §3's keep/drop line) additionally
    /// drops dates, spec and archive paths, and a non-`HR` id prefix
    /// (its own example is `WI-`). This is a one-string guard over one
    /// project's own reason literal, and this repository's own stamped
    /// `idPrefix` is `HR`; a date, a doc path, or another project's own
    /// id prefix reaching this string is the reviewer's and HR-106's
    /// own sweep's job to catch, not this guard's.
    fn history_narration_marker(reason: &str) -> Option<&'static str> {
        let lower = reason.to_lowercase();
        if lower.contains("fix round") {
            return Some("\"fix round\"");
        }
        if lower.contains("task had") {
            return Some("\"task had\"");
        }
        if lower.contains("this task") {
            return Some("\"this task\"");
        }
        if lower.contains("batch ") {
            return Some("\"batch \"");
        }
        if contains_task_reference(reason) {
            return Some("a T<n> task reference");
        }
        if contains_backlog_id_narration(reason) {
            return Some("a backlog-id narrative citation (\"per HR-\" or \"HR-<n>,\")");
        }
        if contains_hex_sha(reason) {
            return Some("a 7-40 character hex run (a commit sha's own shape)");
        }
        None
    }

    /// The reviewer's own round-1 probe: appending this sentence to the
    /// `install.rs` exception's reason once passed the (too-narrow)
    /// history-marker check, so it stays pinned here as a permanent
    /// regression test independent of the live `reason` literal's own
    /// current wording.
    #[test]
    fn history_narration_marker_catches_the_reviewers_batch_task_sha_and_backlog_id_probe() {
        let probe = "Introduced at batch 20 T3, commit e36c4b2, per HR-084.";
        assert!(
            history_narration_marker(probe).is_some(),
            "the widened marker check no longer catches the reviewer's own probe sentence"
        );
    }

    /// `history_narration_marker` never flags this file's own legitimate,
    /// current-contract backlog-id pointers -- a bare parenthetical
    /// reference, not a narrative citation.
    #[test]
    fn history_narration_marker_does_not_flag_a_bare_parenthetical_backlog_id_pointer() {
        assert_eq!(
            history_narration_marker("the one gate that fails on that condition (HR-115)."),
            None
        );
    }

    /// The surviving `install.rs` exception's own printed `reason` states
    /// the present-tense rule (RETIRED's own doc and test ARE the file
    /// paths the deletion mechanism removes, narrowed to exactly that
    /// window) and carries none of `history_narration_marker`'s classes
    /// -- this is USER-FACING gate output (`find-shell-tool-refs`'s own
    /// printed exceptions section), not a comment a sweep would catch.
    #[test]
    fn the_install_rs_exceptions_reason_states_the_rule_not_its_own_history() {
        let reason = exceptions()
            .into_iter()
            .find(|exception| {
                exception
                    .label
                    .starts_with("crates/houserules/src/install.rs")
            })
            .expect("the install.rs exception is still declared")
            .reason;
        assert_eq!(
            history_narration_marker(reason),
            None,
            "reason carries a history marker: {reason}"
        );
        assert!(
            reason
                .to_lowercase()
                .contains("narrowed to exactly that window"),
            "reason does not state the current narrowing rule: {reason}"
        );
    }
}
