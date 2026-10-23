//! The citation checks of `check-report-claims`. `find_citations` reads a
//! path-shaped citation (`path`, `path:NNN`, `path:NNN-MMM`, or a comma or
//! semicolon list of those numbers) from narrative prose.
//! `check_citation_lines`, `check_narrative_number_claims`, and
//! `check_narrative_sweep_coverage` hold each citation to the file it names.
//! `check_recorded_citations` holds each entry of the `citations` field of the
//! report to the file at HEAD. The parent module doc states the stance of every
//! Limits list.
//!
//! Limits (every doc comment of this file that says "see the module doc's
//! limits" means this list):
//! - `check_citation_lines` (HR-107) carries the "correct but aged"
//!   exposure of the sha checks (`narrative`'s Limits) for a FILE
//!   citation, not only a sha: a report correctly citing a file that
//!   existed when the report was written reads as unresolvable once that
//!   file is later renamed, moved, or deleted. This task's own corpus run
//!   found the batch-20 T3 JS-to-Rust migration is by far the largest
//!   source of this shape (retired `.mjs`/`bin/houserules.mjs` citations
//!   in a pre-migration report), with a same-language in-repo rename a
//!   rarer instance of the identical exposure (a report once correctly
//!   citing `check-report-claims.rs`, since moved to `report_claims.rs`).
//! - `find_citations` (so every one of `check_citation_lines`,
//!   `check_narrative_number_claims`, and `check_narrative_sweep_coverage`)
//!   carries the file-citation sibling of the foreign-sha bullet of
//!   `narrative`'s Limits: a path naming a file in ANOTHER codebase entirely --
//!   another tool's own source tree, or a third-party crate's own source read
//!   via the package registry cache while verifying a `docs_verified` entry --
//!   never resolves against this repository's tracked tree or batch workspace,
//!   and never can. The same reasoning applies: a recognised-but-unverified
//!   "foreign path" syntax would let a typo'd or fabricated citation to a
//!   codebase this tool cannot read pass as clean, which is worse than the
//!   current plain false positive. The safe answer is the same procedural
//!   one -- name the foreign file by prose only, with no `path:NNN` shape, or
//!   keep it out of the four scanned narrative fields.
//! - `find_citations` (the shared helper `check_citation_lines`,
//!   `check_narrative_number_claims`, and `check_narrative_sweep_coverage`
//!   all build on) only recognises a path ending in one of a small, closed
//!   set of extensions (its own doc names them); a real cited file with
//!   another extension is invisible to all three checks. It reads a comma- or
//!   semicolon-separated list of line numbers in full (`file.rs:23,24,41`, a
//!   real shape in this repository's own review prose), by a rule that looks at
//!   the text and not at the meaning (HR-148, spec 6.9): an element with no
//!   space after its separator always continues the list, whatever follows it;
//!   an element after the separator and one space continues the list only when
//!   the text ends there, or a separator, a closing `)` or `]`, or a `.` that
//!   ends the sentence follows it. So `file.rs:3,999 shows` reads two
//!   elements and `file.rs:12, 400 lines` reads one citation, whatever the
//!   second number counts; `file.rs:3, 4)` reads two elements, although the
//!   second may be a count. A list that restarts with a path (`a.rs:3, b.rs:4`)
//!   is two citations. A range's ordering is not checked (a reversed range such
//!   as `path:3-2` is read as-is and passes while both numbers fit the file;
//!   each number is checked against the file's line count, never against the
//!   other).
//! - `resolve_citation_path`'s bare-basename fallback refuses to guess
//!   when more than one tracked file shares a basename (this repository's
//!   own tree has several: `install.rs`, `check_commit.rs`, `model.rs`,
//!   `mod.rs`, `archive.rs` each name both a `src/` and a `tests/` file;
//!   `kit.json` names both `backlog/items/` and `backlog/archive/`;
//!   `SKILL.md` and `implementer.md` each name several skills or agent
//!   templates), so a bare citation of one of these reads as unresolvable
//!   even when the intended file plainly exists -- the task's own corpus
//!   run found this the single largest false-positive class by line
//!   count. It also does not extend a bare name to a PARTIAL relative
//!   path (`common/mod.rs` naming `crates/houserules/tests/common/mod.rs`
//!   reads as unresolvable too, since `common/mod.rs` contains a `/` and
//!   never reaches the basename fallback at all); nor does it recognise a
//!   glob or an ellipsis a sentence uses to reference many files or an
//!   elided middle segment at once (`task-*.json`, `tests/*.test.mjs`,
//!   `docs/specs/...-template-cluster.md`) -- each reads as one specific,
//!   nonexistent file rather than the descriptive shorthand it is.
//! - `find_citations`' extension anchor also matches an adjective built
//!   on a real extension ("an `install.rs`-shaped fixture", read here as
//!   citing a file literally named `install.rs-shaped`): the hyphen after
//!   the extension is a word-internal character to `word_end`'s walk, not
//!   a boundary, so the analogy's own suffix rides along into the path. A
//!   single instance in the task's own corpus run; not special-cased.
//! - `check_citation_lines` (HR-107) reads a citation's target file from
//!   `root` with the same fs read every other check of this tool uses:
//!   the report's own artifacts are expected to be checked from a clean
//!   checkout at HEAD, not a git blob pinned to any particular commit, so
//!   a dirty working tree can shift what "the file's line count" means
//!   between two runs of this tool. It also does not flag a citation of
//!   line `0`, since `0` never exceeds a real file's line count.
//! - `check_citation_lines` compares a cited line number with the file's
//!   line count and never compares the text of the line with anything. A
//!   citation in free prose whose number still fits the file after an edit
//!   moved the code is not caught: the only findings are a file that does not
//!   read, and a cited line number, or any number of a range or a list, past
//!   the end of the file. A report that wants the text of the line held
//!   records the citation in its `citations` field (`check_recorded_citations`).
//! - `check_recorded_citations` (HR-141) judges only an entry whose `path`,
//!   `line`, `end`, and `anchor` have the types of the schema and whose numbers
//!   are at least 1; `validate` names any other entry. An entry without a
//!   `sha256` passes: `report-facts` fills it. The hash and the anchor are tied
//!   to the file at HEAD. A report checked after a later commit changed the
//!   cited file reads as stale, which is the finding; a cited path that git
//!   cannot read at HEAD reads as not tracked (`report_facts`'s Limits).
//! - `check_narrative_number_claims` (HR-110) implements only the number
//!   class HR-110's item body names ("N passed", "N files"), not the
//!   locative class ("the X live in Y") the same item also describes:
//!   the two batch-23 locative instances that motivated it both sat in a
//!   commit body and a plan file, neither a report narrative field this
//!   tool scans, so a mechanical, low-noise trigger for "this sentence
//!   asserts a location" could not be derived from this repository's own
//!   corpus (`quality.gates-derive-their-scope`); a future widening needs
//!   its own real instances to calibrate against, not this tool's report-
//!   narrative surface. The body of each commit that the report lists is read
//!   too (`collect_commit_body_fields`), without its subject line, and a
//!   citation in a body counts only when it resolves to a capture, by an
//!   absolute path or a path relative to the install root; a source file, a
//!   bare file name that names no capture at the install root, and a capture
//!   that does not exist are skipped without a finding, while a bare file name
//!   that names a capture at the install root counts (spec 6.10). A cited capture
//!   is read without its header line, so a number in the header (a timestamp, a
//!   path, an argument) is not a match; a cited file that is no capture is
//!   read whole. "Citing a capture" is approximated as "naming
//!   EXACTLY ONE `find_citations` path within `NARRATIVE_CLAIM_WINDOW`
//!   bytes of the count phrase" -- a byte window, not a real sentence boundary,
//!   the same proxy `narrative::check_self_audit_narrative` already uses for a
//!   ratio and its co-located sha; requiring exactly one, not "at least one",
//!   rules out the enumeration-list shape the task's own corpus run found
//!   repeatedly ("N files (a.rs, b.rs, ...)" naming N as the list's own
//!   length), at the cost of also skipping a genuine count sitting near two
//!   unrelated citations. `contains_number_token` reads the claimed count as a
//!   literal digit string: a capture that spells the same count in words ("five
//!   hundred twenty-eight") or with a comma thousands separator ("1,234") reads
//!   as absent.
//! - `check_narrative_sweep_coverage` (HR-111) derives "this citation is
//!   sweep proof" from TWO signals together: the word "sweep" or
//!   "enumeration" sitting within `NARRATIVE_CLAIM_WINDOW` bytes of a
//!   citation (the sentence pattern), and the citation's own path ending
//!   in `.sh`/`.py`/`.txt`/`.log` (`looks_like_sweep_artifact`, the
//!   "cited artifact's shape" half) -- neither a hand list of known
//!   sweep script names, but still a lexical guess: a sweep citation
//!   phrased without either keyword, or one whose retained artifact
//!   happens to carry another extension (the task's own corpus run found
//!   exactly one such case, a `.md` disposition write-up), reads as a
//!   plain file mention and is not checked for coverage. It also runs
//!   only when the report's `files_changed` names exactly one file (see
//!   `check_narrative_sweep_coverage`'s own doc for the false-positive
//!   shape a wider comparison produced on this task's own corpus run) --
//!   a report touching more files never gets a coverage check from this
//!   function at all, whatever it cites. Coverage itself is a plain
//!   substring search for the one changed file inside the cited
//!   artifact's content, so a sweep that lists it under a different
//!   spelling (a relative path shortened, a renamed file cited by its
//!   old name) reads as vacuous even when the underlying sweep really did
//!   cover it. A cited capture is read without its header line, so a file
//!   that only the `argv` of the capture names does not count as covered.

use std::path::Path;
use std::process::{Command, Stdio};

use regress::Regex;
use serde_json::Value;

use super::runs::spell_recorded;
use super::{
    NARRATIVE_LEADING_PUNCTUATION, NARRATIVE_TRAILING_PUNCTUATION, line_count, read_utf8_lossy,
    resolve_against, resolve_citation_path, resolve_commit, word_end, word_start,
};
use crate::capture;
use crate::report_facts::{CitationEntry, judge_citation, range_label};

/// File extensions a citation's path must end in -- the small, closed set
/// this repository's own tracked tree and batch workspaces actually use
/// (`rs`, `json`, `md`, `sh`, `toml`, `yml`/`yaml`, `txt`, `py`, `ts`,
/// `js`, `mjs`, `lock`, `log`, `html`). A knowledge or backlog id
/// (`houserules.crash-paths-are-named`, `process.tdd`) has exactly the
/// same `word.word` shape a looser "any short suffix" pattern would also
/// match, and narrative prose cites ids constantly -- the module docs of
/// this tool do too. Anchoring `find_citations` on a real extension
/// instead is what keeps a citation search from flagging every id a
/// report names; see the module doc's Limits for what a new extension
/// needs before this list covers it.
const CITATION_EXTENSION_PATTERN: &str =
    r"\.(?:rs|json|md|sh|toml|ya?ml|txt|py|ts|js|mjs|lock|log|html)\b";

/// A cited line, or a range of lines up to `end`: the `NNN` or `NNN-MMM` of a
/// citation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Lines {
    line: u64,
    end: Option<u64>,
}

impl Lines {
    /// The larger of the two numbers: a range is compared by it, whatever the
    /// order of the two.
    fn largest(self) -> u64 {
        self.end.map_or(self.line, |end| end.max(self.line))
    }
}

/// A path-shaped citation `find_citations` locates in narrative prose,
/// with the optional `:NNN` or `:NNN-MMM` line range HR-107 checks, or a
/// comma or semicolon list of them (`file.rs:3,5-7`). `lines` is empty when
/// the token carried no such suffix, the shape HR-110 and HR-111 also resolve
/// (a bare mention of a retained capture or a sweep script, with no line
/// pinned). `range` is the trimmed token's own byte span in the source text,
/// the whole list included, so a caller can test proximity to some other match
/// the way `check_self_audit_narrative` already does for a ratio and a
/// co-located sha.
pub(super) struct Citation {
    /// The cited path as written: without the punctuation around the token
    /// and without its `:NNN` or `:NNN-MMM` line suffix.
    pub(super) path: String,
    /// The cited lines in the order of the text: the first element, then each
    /// element of a list.
    lines: Vec<Lines>,
    /// The citation as written, from its path to its last element, when it is a
    /// list of more than one element.
    list_text: Option<String>,
    range: std::ops::Range<usize>,
}

impl Citation {
    /// The text a finding quotes: the list as written, or the path with its
    /// line or range. `None` when the citation names no line.
    fn token(&self) -> Option<String> {
        let first = self.lines.first()?;
        Some(self.list_text.clone().unwrap_or_else(|| match first.end {
            Some(end) => format!("{}:{}-{end}", self.path, first.line),
            None => format!("{}:{}", self.path, first.line),
        }))
    }

    /// The largest number any element of the citation names; 0 when it names
    /// no line.
    fn largest_number(&self) -> u64 {
        self.lines
            .iter()
            .copied()
            .map(Lines::largest)
            .max()
            .unwrap_or(0)
    }
}

/// The byte length of the Windows drive prefix that starts `token` (a letter,
/// a colon, then `/` or `\`: `C:/`, `C:\`), or 0 when the token has none.
fn drive_prefix_len(token: &str) -> usize {
    match token.as_bytes() {
        [letter, b':', b'/' | b'\\', ..] if letter.is_ascii_alphabetic() => 2,
        _ => 0,
    }
}

/// The number of ASCII digits that start `text`.
fn ascii_digits(text: &str) -> usize {
    text.bytes().take_while(u8::is_ascii_digit).count()
}

/// The `NNN` or `NNN-MMM` that starts `text`, and its length in bytes. `None`
/// when `text` starts with no digit, or with a number that does not fit 64 bits.
/// A range whose second half fails to parse (`170-abc`) still keeps the first
/// number: `170` alone is already a valid element.
fn read_lines(text: &str) -> Option<(Lines, usize)> {
    let line_len = ascii_digits(text);
    let line = text[..line_len].parse::<u64>().ok()?;
    if let Some(after_dash) = text[line_len..].strip_prefix('-') {
        let end_len = ascii_digits(after_dash);
        if let Ok(end) = after_dash[..end_len].parse::<u64>() {
            return Some((
                Lines {
                    line,
                    end: Some(end),
                },
                line_len + 1 + end_len,
            ));
        }
    }
    Some((Lines { line, end: None }, line_len))
}

/// Splits a trimmed citation token's optional `:NNN`/`:NNN-MMM` suffix off
/// its path, at the FIRST `:` rather than the last: a path itself carries no
/// colon but a Windows drive letter's (`drive_prefix_len`), so the first
/// other one always marks the line suffix's start. A `file:NNN:MM` column
/// citation keeps `NNN` as its line and drops the trailing `:MM` rather than
/// reading `NNN:MM` as YET more path.
/// Falls back to treating the whole token as a bare path (no line) when the
/// text right after that `:` has no leading digit at all -- a citation with a
/// genuinely non-numeric trailing colon (none observed in this repository's own
/// narrative prose) is not this tool's business to split further. The second
/// value is the first element and the byte offset of its end in `token`.
fn split_line_suffix(token: &str) -> (&str, Option<(Lines, usize)>) {
    let drive = drive_prefix_len(token);
    let Some(colon) = token[drive..].find(':').map(|offset| drive + offset) else {
        return (token, None);
    };
    match read_lines(&token[colon + 1..]) {
        Some((lines, len)) => (&token[..colon], Some((lines, colon + 1 + len))),
        None => (token, None),
    }
}

/// Whether the text `after` an element that follows a separator and one space
/// lets the element continue the list: the text ends, or a separator, a closing
/// `)` or `]`, or a `.` that ends the sentence (a `.` followed by white space or
/// the end of the text) comes next.
fn continues_a_list(after: &str) -> bool {
    match after.chars().next() {
        None | Some(',' | ';' | ')' | ']') => true,
        Some('.') => after[1..].chars().next().is_none_or(char::is_whitespace),
        Some(_) => false,
    }
}

/// The elements of the list that continues in `text` at byte `from`, after its
/// first element, and the byte where the list ends. An element right after a
/// separator (`,` or `;`) always continues the list. An element after a
/// separator and one space continues it only when `continues_a_list` says so
/// (HR-148, spec 6.9; the module doc's Limits).
fn read_list_tail(text: &str, from: usize) -> (Vec<Lines>, usize) {
    let mut elements = Vec::new();
    let mut end = from;
    while let Some(after_separator) = text[end..].strip_prefix([',', ';']) {
        if let Some((lines, len)) = read_lines(after_separator) {
            elements.push(lines);
            end += 1 + len;
            continue;
        }
        let Some((lines, len)) = after_separator
            .strip_prefix(' ')
            .and_then(read_lines)
            .filter(|(_, len)| continues_a_list(&after_separator[1 + len..]))
        else {
            break;
        };
        elements.push(lines);
        end += 2 + len;
    }
    (elements, end)
}

/// Strips `NARRATIVE_TRAILING_PUNCTUATION` from `text`, then a trailing
/// possessive `'s` (straight or curly apostrophe), repeating until
/// neither applies. This repository's own narrative prose routinely cites
/// a file possessively ("`install.rs`'s own module doc has ..."), and
/// that suffix is not sentence punctuation `trim_end_matches` alone
/// strips -- left untrimmed, `find_citations` would read the citation as
/// `install.rs's`, a file that can never exist.
fn trim_trailing_citation_glue(text: &str) -> &str {
    let mut trimmed = text;
    loop {
        let before = trimmed;
        trimmed = trimmed.trim_end_matches(NARRATIVE_TRAILING_PUNCTUATION);
        if let Some(stripped) = trimmed
            .strip_suffix("'s")
            .or_else(|| trimmed.strip_suffix("\u{2019}s"))
        {
            trimmed = stripped;
        }
        if trimmed == before {
            return trimmed;
        }
    }
}

/// Every path-shaped citation `text` names. Each `CITATION_EXTENSION_PATTERN`
/// match anchors a search for its whole whitespace-delimited word
/// (`word_start`/`word_end`, the same walk `flag_ephemeral_words` already
/// does); `NARRATIVE_LEADING_PUNCTUATION` trims the sentence punctuation
/// glued to its start, `trim_trailing_citation_glue` trims the
/// punctuation and any possessive `'s` glued to its end, and
/// `split_line_suffix` splits off any trailing `:NNN`/`:NNN-MMM` that
/// survives that trim. `read_list_tail` then reads the elements of a comma or
/// semicolon list from the text after that first element. `search_from` only
/// advances, past the end of a list too, so no two returned citations can name
/// the exact same token twice.
pub(super) fn find_citations(text: &str) -> Vec<Citation> {
    let extension =
        Regex::new(CITATION_EXTENSION_PATTERN).expect("valid citation-extension pattern");
    let mut citations = Vec::new();
    let mut search_from = 0;
    while let Some(found) = extension.find(&text[search_from..]) {
        let anchor = search_from + found.start();
        let word_bound_start = word_start(text, anchor);
        let word_bound_end = word_end(text, anchor);
        search_from = word_bound_end.max(anchor + 1);
        let raw = &text[word_bound_start..word_bound_end];
        let after_leading_trim = raw.trim_start_matches(NARRATIVE_LEADING_PUNCTUATION);
        let leading_trimmed_len = raw.len() - after_leading_trim.len();
        let trimmed = trim_trailing_citation_glue(after_leading_trim);
        if trimmed.is_empty() {
            continue;
        }
        let range_start = word_bound_start + leading_trimmed_len;
        let mut range = range_start..range_start + trimmed.len();
        let (path, first) = split_line_suffix(trimmed);
        let mut lines = Vec::new();
        let mut list_text = None;
        if let Some((first, first_len)) = first {
            lines.push(first);
            let (more, list_end) = read_list_tail(text, range_start + first_len);
            if !more.is_empty() {
                lines.extend(more);
                list_text = Some(text[range_start..list_end].to_string());
                range.end = range.end.max(list_end);
                search_from = search_from.max(list_end);
            }
        }
        citations.push(Citation {
            path: path.to_string(),
            lines,
            list_text,
            range,
        });
    }
    citations
}

/// One `collect_narrative` field, its citations already parsed: `label`
/// and `text` are exactly `collect_narrative`'s own pair, and `citations`
/// is `find_citations(&text)`, run here exactly once. `check_citation_lines`,
/// `check_narrative_number_claims`, and `check_narrative_sweep_coverage`
/// all read `citations` from this shared pass. Each `find_citations` call
/// compiles `CITATION_EXTENSION_PATTERN` and scans the whole text, so a field
/// is parsed once, not once per check.
pub(super) struct NarrativeField {
    label: String,
    text: String,
    citations: Vec<Citation>,
}

/// Runs `find_citations` once per `narrative` field, pairing each with its
/// own `label`/`text` for the three checks built on `NarrativeField` below.
pub(super) fn collect_narrative_fields(narrative: &[(String, String)]) -> Vec<NarrativeField> {
    narrative
        .iter()
        .map(|(label, text)| {
            let citations = find_citations(text);
            NarrativeField {
                label: label.clone(),
                text: text.clone(),
                citations,
            }
        })
        .collect()
}

/// Loads a citation's target file from `root`: a bare filename resolves
/// through `resolve_citation_path` first (the tracked-tree basename
/// fallback), and everything else falls back to the same fs-read
/// (`resolve_against` + `read_utf8_lossy`) every other check of this
/// tool already uses -- this tool has never distinguished a tracked-
/// tree citation from one naming a file in the gitignored batch
/// workspace a report cites (`resolve_against` just joins).
pub(super) fn load_citation(
    root: &Path,
    citation: &Citation,
    tracked_files: &[String],
) -> std::io::Result<String> {
    read_utf8_lossy(&resolve_citation_path(root, &citation.path, tracked_files))
}

/// Loads a citation's target as `load_citation` does, but a capture without its
/// header line: the output of the command is what a count or a sweep rests on,
/// and the header holds the `argv`, the timestamps, and the working directory
/// (spec 6.10, 6.11). A file that is no capture is read whole.
fn load_citation_output(
    root: &Path,
    citation: &Citation,
    tracked_files: &[String],
) -> std::io::Result<String> {
    let path = resolve_citation_path(root, &citation.path, tracked_files);
    match capture::read(&path) {
        Ok(capture) => Ok(String::from_utf8_lossy(capture.output()).into_owned()),
        Err(_) => read_utf8_lossy(&path),
    }
}

/// `true` when `a` and `b` sit within `window` bytes of each other (zero
/// distance when they overlap or touch) -- the deterministic "same sentence"
/// proxy that `narrative` already uses for a self-audit ratio and its
/// co-located sha (`narrative::RATIO_SHA_WINDOW`); HR-110 and HR-111 reuse the
/// same proxy for a citation and the claim it is offered to support. See the
/// module doc's Limits for why a byte window stands in for a real sentence
/// boundary.
fn ranges_close(a: &std::ops::Range<usize>, b: &std::ops::Range<usize>, window: usize) -> bool {
    let gap = b
        .start
        .saturating_sub(a.end)
        .max(a.start.saturating_sub(b.end));
    gap <= window
}

// ---- HR-141: a recorded citation must hold at HEAD ----

/// Check 4. Every entry of `citations` is judged against the file at HEAD
/// (`judge_citation`, the one judgement that `report-facts` uses too): its path
/// is tracked, `end` is not before `line`, the lines exist, and the `anchor`
/// occurs inside them and nowhere else in the file. An entry that holds a
/// `sha256` must also have the hash of those lines; one that holds none passes,
/// for `report-facts` fills it. An entry that does not parse is left to
/// `validate`. See the module doc's Limits.
pub(super) fn check_recorded_citations(root: &Path, report: &Value, errors: &mut Vec<String>) {
    let Some(entries) = report.get("citations").and_then(Value::as_array) else {
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        let Some(cited) = entry.as_object().and_then(CitationEntry::of) else {
            continue;
        };
        match judge_citation(root, &cited.path, cited.line, cited.end, &cited.anchor) {
            Err(fault) => {
                errors.push(fault.finding(index, &cited.path, cited.line, cited.end));
            }
            Ok(lines) => {
                let Some(recorded) = entry.get("sha256") else {
                    continue;
                };
                if recorded.as_str() != Some(lines.sha256.as_str()) {
                    errors.push(format!(
                        "citations[{index}]: {}:{} has sha256 {} at HEAD, but the report records {}; re-open the lines at HEAD, correct line and end, then remove sha256 and run report-facts",
                        cited.path,
                        range_label(cited.line, cited.end),
                        lines.sha256,
                        spell_recorded(recorded)
                    ));
                }
            }
        }
    }
}

// ---- HR-107: a citation's line range must fit the file it names ----

/// Every `path:NNN`/`path:NNN-MMM` citation `collect_narrative`'s fields
/// carry must name a file that reads clean under `root`, and every
/// number the citation names -- NNN, and MMM for a range, and each element of a
/// comma or semicolon list -- must not exceed that file's line count. A finding
/// quotes the whole list. A range is compared by its larger
/// number, so a reversed range (`path:5-2`) is a finding when either
/// number is past the end and passes when both fit (`path:3-2`): this
/// check never judges a range's order. A bare citation with no line suffix
/// (`NarrativeField::citations` also carries those, for HR-110 and HR-111
/// below) is not this check's business. See the module doc's Limits for the
/// rule that decides where a list ends.
pub(super) fn check_citation_lines(
    root: &Path,
    narrative: &[NarrativeField],
    tracked_files: &[String],
    errors: &mut Vec<String>,
) {
    let mut seen: Vec<(String, String)> = Vec::new();
    for field in narrative {
        let label = &field.label;
        for citation in &field.citations {
            let Some(token) = citation.token() else {
                continue;
            };
            let key = (label.clone(), token.clone());
            if seen.contains(&key) {
                continue;
            }
            match load_citation(root, citation, tracked_files) {
                Ok(content) => {
                    let count = line_count(&content) as u64;
                    if citation.largest_number() > count {
                        seen.push(key);
                        errors.push(format!(
                            "{label}: cites \"{token}\", but {} has only {count} lines",
                            citation.path
                        ));
                    }
                }
                Err(error) => {
                    seen.push(key);
                    errors.push(format!(
                        "{label}: cites \"{token}\", which could not be read ({error})"
                    ));
                }
            }
        }
    }
}

// ---- HR-110: a narrative count must appear in the capture it cites ----

/// Characters either side of a citation searched for a co-located count
/// claim, or of a "sweep"/"enumeration" keyword searched for a co-located
/// citation -- the same deterministic "same sentence" proxy as
/// `narrative::RATIO_SHA_WINDOW`, at the same value, for the same reason (see
/// `ranges_close`'s own doc).
const NARRATIVE_CLAIM_WINDOW: usize = 100;

/// `true` when `number` (an ASCII decimal string, e.g. `"528"`) appears in
/// `content` as a standalone token: flanked by a non-digit or the text's
/// edge on each side, so `"3"` does not read as present inside `"23"`.
fn contains_number_token(content: &str, number: &str) -> bool {
    let pattern =
        Regex::new(&format!(r"\b{number}\b")).expect("a decimal-digit string is a valid pattern");
    pattern.find(content).is_some()
}

/// HR-110's number class: a narrative sentence stating a count ("N
/// passed", "N files") while citing a capture must find that count
/// inside the capture it cites. "Citing a capture" is approximated as
/// "naming exactly one `find_citations` path within `NARRATIVE_CLAIM_WINDOW`
/// bytes of the count phrase" -- the same citation `check_citation_lines`
/// resolves, bare mention or not. Requiring exactly one nearby citation
/// (not "at least one") rules out the enumeration-list shape this task's
/// own corpus run found repeatedly: "N files (a.rs, b.rs, ..., z.rs)"
/// names N as the LIST's own length, not a fact any one of a/b/.../z's
/// content should contain, and a citation-per-item count check flagged
/// every listed file as failing to "contain" the list's own size. This
/// scope is deliberately narrow (`quality.gates-derive-their-scope`); see
/// the module doc's Limits for what it leaves uncovered, including the
/// locative ("the X live in Y") class this function does not attempt.
///
/// `commit_bodies` (`collect_commit_body_fields`) are checked the same way,
/// each with the citations that resolve to a capture. A cited capture is read
/// without its header line.
pub(super) fn check_narrative_number_claims(
    root: &Path,
    narrative: &[NarrativeField],
    commit_bodies: &[NarrativeField],
    tracked_files: &[String],
    errors: &mut Vec<String>,
) {
    let count_phrase = Regex::with_flags(r"\b(\d+)\s+(?:passed|files?)\b", "i")
        .expect("valid count-phrase pattern");
    for field in narrative.iter().chain(commit_bodies) {
        let (label, text, citations) = (&field.label, &field.text, &field.citations);
        if citations.is_empty() {
            continue;
        }
        for found in count_phrase.find_iter(text) {
            let phrase_range = found.range();
            let phrase_text = found.as_str(text).to_string();
            let number =
                text[found.group(1).expect("group 1 always captures on a match")].to_string();
            let mut nearby = citations.iter().filter(|citation| {
                ranges_close(&phrase_range, &citation.range, NARRATIVE_CLAIM_WINDOW)
            });
            let Some(citation) = nearby.next() else {
                continue; // no citation nearby: nothing to cross-check
            };
            if nearby.next().is_some() {
                continue; // more than one nearby citation: an enumeration list, not one capture
            }
            match load_citation_output(root, citation, tracked_files) {
                Ok(content) => {
                    if !contains_number_token(&content, &number) {
                        errors.push(format!(
                            "{label}: \"{phrase_text}\" cites \"{}\", which does not contain \"{number}\"",
                            citation.path
                        ));
                    }
                }
                Err(error) => errors.push(format!(
                    "{label}: \"{phrase_text}\" cites \"{}\", which could not be read ({error})",
                    citation.path
                )),
            }
        }
    }
}

/// The message of the commit `sha` in `root` without its subject line, or
/// `None` when `sha` names no commit or git cannot print it. The sha resolves to
/// a full id first (`resolve_commit`), and git prints the commit of that id.
fn commit_body(root: &Path, sha: &str) -> Option<String> {
    let full = resolve_commit(root, sha)?;
    let output = Command::new("git")
        .args([
            "log",
            "-1",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--format=%B",
            &full,
        ])
        .current_dir(root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let message = String::from_utf8_lossy(&output.stdout);
    Some(
        message
            .split_once('\n')
            .map_or("", |(_, body)| body)
            .to_string(),
    )
}

/// Whether the file that `citation` names, by an absolute path or a path
/// relative to `root`, is a capture.
fn resolves_to_capture(root: &Path, citation: &Citation) -> bool {
    capture::read(&resolve_against(root, &citation.path)).is_ok()
}

/// The body of each commit that the report lists (`commits[]` and
/// `fix_rounds[].commits[]`), as the fields check 10 reads beside the narrative
/// (HR-204 (10)). The label is the JSON path of the commit, the sha as listed,
/// and `body` (`commits[0] 04bf16a body`). A field holds only the citations that
/// resolve to a capture (`resolves_to_capture`): every other citation, a bare
/// file name that names no capture at the install root included, is skipped; a
/// bare file name that names a capture at the install root counts. A commit
/// that git cannot print adds no field.
pub(super) fn collect_commit_body_fields(root: &Path, report: &Value) -> Vec<NarrativeField> {
    let mut fields = Vec::new();
    let mut add_commits = |path: String, commits: Option<&Value>| {
        let listed = commits.and_then(Value::as_array);
        for (index, commit) in listed.into_iter().flatten().enumerate() {
            let Some(sha) = commit.get("sha").and_then(Value::as_str) else {
                continue;
            };
            let Some(body) = commit_body(root, sha) else {
                continue;
            };
            let citations = find_citations(&body)
                .into_iter()
                .filter(|citation| resolves_to_capture(root, citation))
                .collect();
            fields.push(NarrativeField {
                label: format!("{path}[{index}] {sha} body"),
                text: body,
                citations,
            });
        }
    };
    add_commits("commits".to_string(), report.get("commits"));
    let rounds = report.get("fix_rounds").and_then(Value::as_array);
    for (round, entry) in rounds.into_iter().flatten().enumerate() {
        add_commits(format!("fix_rounds[{round}].commits"), entry.get("commits"));
    }
    fields
}

// ---- HR-111: a cited sweep must cover the file it certifies ----

/// Every path `report["files_changed"]` names -- the plain string list
/// every task-report and branch-review already carries.
/// `check_narrative_sweep_coverage` treats each as a file a cited sweep
/// should cover: this schema field exists on every deliverable this tool
/// already reads, so no narrative-parsing heuristic is needed to know
/// which files a sweep's clean result has to certify.
pub(super) fn collect_files_changed(report: &Value) -> Vec<String> {
    report
        .get("files_changed")
        .and_then(Value::as_array)
        .map(|files| {
            files
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Extensions a citation's path must end in before `check_narrative_sweep_coverage`
/// treats it as a candidate sweep/enumeration ARTIFACT at all -- the
/// "cited artifact's shape" half of HR-111's mechanical trigger, alongside
/// the "sweep"/"enumeration" keyword. A real retained sweep in this
/// repository's own corpus is always a script or a captured run (`.sh`,
/// `.py`, `.txt`, `.log`); a `.md`/`.json`/`.toml`/`.rs`/`.yml` citation
/// near the same keyword is, in every corpus instance found, the sentence
/// naming the sweep's SCOPE ("the sweep (README.md, CLAUDE.md, ...)") or
/// an unrelated file the sentence happens to also mention -- not the
/// sweep's own output. See the module doc's Limits for the corpus count
/// this excludes on purpose, and the one true positive (a `.md`
/// disposition write-up) it also excludes as the cost of that precision.
const SWEEP_ARTIFACT_EXTENSIONS: &[&str] = &["sh", "py", "txt", "log"];

/// `true` when `path`'s extension is one of `SWEEP_ARTIFACT_EXTENSIONS`.
fn looks_like_sweep_artifact(path: &str) -> bool {
    SWEEP_ARTIFACT_EXTENSIONS
        .iter()
        .any(|extension| path.ends_with(&format!(".{extension}")))
}

/// HR-111's mechanical trigger for "this citation is offered as sweep or
/// enumeration proof": the word "sweep" or "enumeration" (case-
/// insensitive) within `NARRATIVE_CLAIM_WINDOW` bytes of a citation whose
/// own path `looks_like_sweep_artifact` -- a sentence pattern plus an
/// artifact-shape filter, not a hand list of known sweep script names
/// (see the module doc's Limits for what this still misses).
///
/// Runs only when `files_changed` names exactly one file. A report that
/// touches many files makes "the sweep's content names every one of
/// them" the wrong test -- a real, narrowly-scoped sweep (checking one
/// concern across the tracked tree) has no reason to mention a plan file
/// or a schema copy the same task also happened to edit, and the corpus
/// run behind this item found exactly that shape of false positive at
/// every files_changed count above one. A single-file task is the shape
/// HR-111's own origin names (batch 23 T1: `deliverable.rs`, the file the
/// task changed most, missing from a sweep's ten-file list) and the one
/// case this comparison is precise for; see the module doc's Limits for
/// what this leaves uncovered.
pub(super) fn check_narrative_sweep_coverage(
    root: &Path,
    narrative: &[NarrativeField],
    files_changed: &[String],
    tracked_files: &[String],
    errors: &mut Vec<String>,
) {
    if files_changed.len() != 1 {
        return;
    }
    let keyword =
        Regex::with_flags(r"\b(?:sweep|enumeration)\b", "i").expect("valid sweep-keyword pattern");
    for field in narrative {
        let (label, text, citations) = (&field.label, &field.text, &field.citations);
        let sweep_citations: Vec<&Citation> = citations
            .iter()
            .filter(|citation| looks_like_sweep_artifact(&citation.path))
            .collect();
        if sweep_citations.is_empty() {
            continue;
        }
        // Every sweep-shaped citation within NARRATIVE_CLAIM_WINDOW of ANY
        // keyword match, deduplicated by citation identity (its own byte
        // range): a field naming "sweep" more than once near the same
        // citation still reads that citation's target file exactly once
        // below: `load_citation` runs once per qualifying citation, not once
        // per keyword match.
        let mut qualifying: Vec<&Citation> = Vec::new();
        for keyword_match in keyword.find_iter(text) {
            let keyword_range = keyword_match.range();
            for citation in &sweep_citations {
                let already_queued = qualifying
                    .iter()
                    .any(|queued| queued.range.start == citation.range.start);
                if !already_queued
                    && ranges_close(&keyword_range, &citation.range, NARRATIVE_CLAIM_WINDOW)
                {
                    qualifying.push(citation);
                }
            }
        }
        for citation in qualifying {
            let content = match load_citation_output(root, citation, tracked_files) {
                Ok(content) => content,
                Err(error) => {
                    errors.push(format!(
                        "{label}: cites \"{}\" as a sweep, which could not be read ({error})",
                        citation.path
                    ));
                    continue;
                }
            };
            for changed in files_changed {
                if content.contains(changed.as_str()) {
                    continue;
                }
                errors.push(format!(
                    "{label}: cites \"{}\" as a sweep, but its retained content does not name \"{changed}\", a file this report's own files_changed lists",
                    citation.path
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::report_claims::check_report_claims;
    use crate::report_claims::tests::{
        base_report, clean_run, commit, git, init_scratch_repo, write_json,
    };

    // ---- the citation-line check (HR-107) ----

    /// Passes a `path:NNN` citation whose line sits within the named
    /// file's line count.
    #[test]
    fn passes_a_citation_whose_line_is_within_the_named_files_line_count() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-ok-");
        std::fs::write(dir.path().join("cited.rs"), "one\ntwo\nthree\n").unwrap();
        let mut report = base_report(&head);
        report["implemented"] = json!("Fixed the panic at cited.rs:2.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A Rust-panic-style `path:NNN:MM` column citation reads NNN as the
    /// line and drops the trailing `:MM` -- not `NNN:MM` glued together
    /// as an unresolvable path.
    #[test]
    fn passes_a_file_line_column_citation_reading_the_line_and_dropping_the_column() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-line-col-");
        std::fs::write(dir.path().join("cited.rs"), "one\ntwo\nthree\n").unwrap();
        let mut report = base_report(&head);
        report["implemented"] = json!("Fixed the panic at cited.rs:2:14.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Flags a `path:NNN` citation whose line exceeds the named file's
    /// line count -- the stream-index-as-file-line class HR-107 exists
    /// to catch.
    #[test]
    fn flags_a_citation_whose_line_exceeds_the_named_files_line_count() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-overflow-");
        std::fs::write(dir.path().join("cited.rs"), "one\ntwo\nthree\n").unwrap();
        let mut report = base_report(&head);
        report["implemented"] = json!("Fixed the panic at cited.rs:99.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec!["implemented: cites \"cited.rs:99\", but cited.rs has only 3 lines".to_string()]
        );
    }

    /// Flags a `path:NNN-MMM` citation whose range end exceeds the named
    /// file's line count, even though the range start is in bounds.
    #[test]
    fn flags_a_citation_range_whose_end_exceeds_the_named_files_line_count() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-range-overflow-");
        std::fs::write(dir.path().join("cited.rs"), "one\ntwo\nthree\n").unwrap();
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim": "Reviewed the fix at cited.rs:2-10."}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0].claim: cites \"cited.rs:2-10\", but cited.rs has only 3 lines"
                    .to_string()
            ]
        );
    }

    /// Flags a `path:NNN-MMM` citation whose range start exceeds the named
    /// file's line count, even though the range end is in bounds: every
    /// number a citation names must fit the file, whatever the order.
    #[test]
    fn flags_a_citation_range_whose_start_exceeds_the_named_files_line_count() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-range-start-overflow-");
        std::fs::write(dir.path().join("cited.rs"), "one\ntwo\nthree\n").unwrap();
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim": "Reviewed the fix at cited.rs:5-2."}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0].claim: cites \"cited.rs:5-2\", but cited.rs has only 3 lines"
                    .to_string()
            ]
        );
    }

    /// Passes a `path:NNN-MMM` range whose two numbers both fit the named
    /// file, reversed or not: this check compares each number with the
    /// file's line count and never judges a range's order (the module
    /// doc's Limits say so).
    #[test]
    fn passes_a_range_whose_numbers_both_fit_the_named_file_in_either_order() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-range-order-");
        std::fs::write(dir.path().join("cited.rs"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim": "Reviewed cited.rs:2-3 and then cited.rs:3-2."}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Flags a citation naming a file that does not exist under `root`,
    /// tracked tree or batch workspace alike.
    #[test]
    fn flags_a_citation_naming_a_file_that_does_not_exist() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-missing-");
        let mut report = base_report(&head);
        report["implemented"] = json!("See notes at .superpowers/sdd/batch-1/evidence.txt:5.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with(
                "implemented: cites \".superpowers/sdd/batch-1/evidence.txt:5\", which could not be read ("
            ),
            "unexpected error: {}",
            errors[0]
        );
    }

    /// A bare path mention with no `:NNN` suffix is not this check's
    /// business (HR-110/HR-111 resolve those instead): citing a source
    /// file by name alone, with no line pinned, never triggers HR-107,
    /// even when the file does not exist.
    #[test]
    fn does_not_flag_a_bare_path_citation_with_no_line_suffix() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-bare-");
        let mut report = base_report(&head);
        report["implemented"] = json!("See nonexistent.rs for the old shape.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A `path:NNN`-shaped token inside a `command` field is exempt, the
    /// same ephemeral-path precedent HR-107's own item body names: only
    /// `collect_narrative`'s four fields are ever scanned for a citation.
    #[test]
    fn does_not_flag_a_path_nnn_shaped_token_inside_a_command_field() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-command-exempt-");
        let mut report = base_report(&head);
        let run = clean_run(
            dir.path(),
            "capture.txt",
            &["sed", "-n", "1,999p", "cited.rs:999"],
            "irrelevant",
        );
        assert_eq!(run["command"], json!("sed -n '1,999p' cited.rs:999"));
        report["live_run"] = json!([run]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A knowledge or backlog id's `area.hyphenated-name` shape never
    /// reads as a citation: `CITATION_EXTENSION_PATTERN` requires a real
    /// extension immediately after the dot, and no shipped id ends that
    /// way.
    #[test]
    fn does_not_flag_a_knowledge_id_as_a_citation() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-knowledge-id-");
        let mut report = base_report(&head);
        report["implemented"] =
            json!("Relied on houserules.crash-paths-are-named and process.deliverables-json.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A bare-basename citation (`install.rs:170`, this repository's own
    /// dominant citation style -- no directory prefix) resolves through
    /// the one tracked file with that basename, even though it sits in a
    /// nested directory the citation never names.
    #[test]
    fn resolves_a_bare_basename_citation_to_the_one_tracked_file_it_names() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-basename-");
        std::fs::create_dir_all(dir.path().join("crates/houserules/src")).unwrap();
        std::fs::write(
            dir.path().join("crates/houserules/src/install.rs"),
            "one\ntwo\nthree\n",
        )
        .unwrap();
        git(dir.path(), &["add", "crates/houserules/src/install.rs"]);
        commit(dir.path(), "add install.rs");
        let mut report = base_report(&head);
        report["implemented"] = json!("Fixed the panic at install.rs:2.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A bare-basename citation matching more than one tracked file falls
    /// back to the literal, root-relative join (here, nonexistent) rather
    /// than guessing which tracked file the citation meant.
    #[test]
    fn falls_back_to_the_literal_path_when_a_bare_basename_is_ambiguous() {
        let (dir, head) = init_scratch_repo("check-report-claims-citation-basename-ambiguous-");
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        std::fs::write(dir.path().join("a/mod.rs"), "one\ntwo\n").unwrap();
        std::fs::write(dir.path().join("b/mod.rs"), "one\ntwo\nthree\n").unwrap();
        git(dir.path(), &["add", "a/mod.rs", "b/mod.rs"]);
        commit(dir.path(), "add two mod.rs files");
        let mut report = base_report(&head);
        report["implemented"] = json!("Fixed the panic at mod.rs:2.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("implemented: cites \"mod.rs:2\", which could not be read ("),
            "unexpected error: {}",
            errors[0]
        );
    }

    // ---- the narrative number-claim cross-check (HR-110) ----

    /// Flags a narrative count that does not appear in the capture it
    /// cites -- the 523-vs-528 shape HR-110's own item body names.
    #[test]
    fn flags_a_narrative_count_absent_from_its_cited_capture() {
        let (dir, head) = init_scratch_repo("check-report-claims-number-claim-stale-");
        std::fs::write(
            dir.path().join("t2-full-suite.txt"),
            "test result: ok. 528 passed; 0 failed\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["implemented"] =
            json!("cargo test --locked (523 passed, captured in t2-full-suite.txt).");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "implemented: \"523 passed\" cites \"t2-full-suite.txt\", which does not contain \"523\"".to_string()
            ]
        );
    }

    /// Passes a narrative count that does appear in the capture it cites.
    #[test]
    fn passes_a_narrative_count_present_in_its_cited_capture() {
        let (dir, head) = init_scratch_repo("check-report-claims-number-claim-ok-");
        std::fs::write(
            dir.path().join("t2-full-suite.txt"),
            "test result: ok. 528 passed; 0 failed\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["implemented"] =
            json!("cargo test --locked (528 passed, captured in t2-full-suite.txt).");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Passes a narrative "N files" count present in the capture it
    /// cites -- the item body's second named count shape.
    #[test]
    fn passes_a_narrative_files_count_present_in_its_cited_capture() {
        let (dir, head) = init_scratch_repo("check-report-claims-number-claim-files-ok-");
        std::fs::write(
            dir.path().join("corpus-run.txt"),
            "checked 3 files, 0 hits\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["implemented"] = json!("Swept 3 files, retained at corpus-run.txt.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A count with no citation anywhere nearby is not this check's
    /// business: nothing to cross-check it against.
    #[test]
    fn does_not_flag_a_count_with_no_nearby_citation() {
        let (dir, head) = init_scratch_repo("check-report-claims-number-claim-no-citation-");
        let mut report = base_report(&head);
        report["implemented"] = json!("Captured 523 passed, not written down anywhere.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A count naming the length of an enumerated list ("2 files
    /// (a.rs, b.rs)") is not a claim about either listed file's own
    /// content: more than one citation sitting near the count phrase
    /// means it counts the list, not one capture, so neither citation is
    /// checked -- the real corpus shape this scope choice exists for.
    #[test]
    fn does_not_flag_a_count_naming_an_enumerated_lists_own_length() {
        let (dir, head) = init_scratch_repo("check-report-claims-number-claim-enumeration-");
        std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "fn b() {}\n").unwrap();
        let mut report = base_report(&head);
        report["implemented"] = json!("2 files touched (a.rs, b.rs).");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Flags a narrative count citing a capture that does not exist,
    /// rather than silently passing an unresolvable citation.
    #[test]
    fn flags_a_narrative_count_citing_a_capture_that_does_not_exist() {
        let (dir, head) = init_scratch_repo("check-report-claims-number-claim-missing-");
        let mut report = base_report(&head);
        report["implemented"] =
            json!("cargo test --locked (523 passed, see missing-evidence.txt).");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with(
                "implemented: \"523 passed\" cites \"missing-evidence.txt\", which could not be read ("
            ),
            "unexpected error: {}",
            errors[0]
        );
    }

    /// A trailing possessive `'s` glued onto a citation ("`corpus-run.txt`'s
    /// own summary", a shape this repository's own narrative prose uses
    /// constantly) is not part of the path: `find_citations` resolves the
    /// file underneath it, not a `corpus-run.txt's` that can never exist.
    #[test]
    fn passes_a_narrative_count_citing_a_capture_through_a_trailing_possessive() {
        let (dir, head) = init_scratch_repo("check-report-claims-number-claim-possessive-");
        std::fs::write(
            dir.path().join("corpus-run.txt"),
            "checked 6 files, 0 hits\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["implemented"] = json!("6 files, per corpus-run.txt's own summary.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    // ---- the narrative sweep-coverage check (HR-111) ----

    /// Flags a sweep citation whose retained content omits a file this
    /// report's own `files_changed` lists -- the vacuous-zero shape
    /// HR-111's own item body names (batch 23 T1 round 0's sweep, whose
    /// ten-file list omitted `deliverable.rs`).
    #[test]
    fn flags_a_sweep_citation_whose_retained_content_omits_a_changed_file() {
        let (dir, head) = init_scratch_repo("check-report-claims-sweep-vacuous-");
        std::fs::write(
            dir.path().join("coverage-check.txt"),
            "9 hits across backlog and docs, 0 remaining\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["files_changed"] = json!(["crates/houserules/src/rules/deliverable.rs"]);
        report["implemented"] = json!("The retained sweep (coverage-check.txt) still exits 0.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "implemented: cites \"coverage-check.txt\" as a sweep, but its retained content does not name \"crates/houserules/src/rules/deliverable.rs\", a file this report's own files_changed lists".to_string()
            ]
        );
    }

    /// Passes a sweep citation whose retained content names every file
    /// this report's own `files_changed` lists.
    #[test]
    fn passes_a_sweep_citation_whose_retained_content_covers_every_changed_file() {
        let (dir, head) = init_scratch_repo("check-report-claims-sweep-covered-");
        std::fs::write(
            dir.path().join("coverage-check.txt"),
            "9 hits: crates/houserules/src/rules/deliverable.rs among them, 0 remaining\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["files_changed"] = json!(["crates/houserules/src/rules/deliverable.rs"]);
        report["implemented"] = json!("The retained sweep (coverage-check.txt) still exits 0.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A report naming no `files_changed` at all gives this check nothing
    /// to certify, so a citation that would otherwise look like a vacuous
    /// sweep is not flagged.
    #[test]
    fn does_not_flag_a_sweep_citation_when_files_changed_is_empty() {
        let (dir, head) = init_scratch_repo("check-report-claims-sweep-no-files-changed-");
        std::fs::write(
            dir.path().join("coverage-check.txt"),
            "9 hits across backlog and docs, 0 remaining\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["implemented"] = json!("The retained sweep (coverage-check.txt) still exits 0.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A report naming TWO changed files gives this check nothing
    /// precise to certify (HR-111's own doc: comparing a sweep against
    /// every changed file floods a multi-file report with false
    /// positives), so a citation that would otherwise look like a
    /// vacuous sweep is not flagged -- pins the `files_changed.len() !=
    /// 1` boundary.
    #[test]
    fn does_not_flag_a_vacuous_sweep_citation_when_files_changed_has_two_entries() {
        let (dir, head) = init_scratch_repo("check-report-claims-sweep-multi-file-");
        std::fs::write(
            dir.path().join("coverage-check.txt"),
            "9 hits across backlog and docs, 0 remaining\n",
        )
        .unwrap();
        let mut report = base_report(&head);
        report["files_changed"] = json!([
            "crates/houserules/src/rules/deliverable.rs",
            "crates/houserules/src/rules/check.rs"
        ]);
        report["implemented"] = json!("The retained sweep (coverage-check.txt) still exits 0.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A `.md` artifact cited beside "sweep" is not a candidate sweep
    /// artifact (`SWEEP_ARTIFACT_EXTENSIONS`), so it is not flagged even
    /// though its content omits the report's own single changed file --
    /// pins the artifact-shape boundary.
    #[test]
    fn does_not_flag_a_non_sweep_shaped_artifact_cited_beside_sweep_omitting_the_changed_file() {
        let (dir, head) = init_scratch_repo("check-report-claims-sweep-non-sweep-shape-");
        std::fs::write(dir.path().join("notes.md"), "unrelated prose\n").unwrap();
        let mut report = base_report(&head);
        report["files_changed"] = json!(["crates/houserules/src/rules/deliverable.rs"]);
        report["implemented"] = json!("The retained sweep (notes.md) still exits 0.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A citation with no "sweep"/"enumeration" keyword nearby is a plain
    /// file mention, not sweep proof, even when its content omits a
    /// changed file: HR-107 and HR-110 are this shape's checks, not
    /// HR-111.
    #[test]
    fn does_not_flag_a_plain_citation_with_no_sweep_keyword_nearby() {
        let (dir, head) = init_scratch_repo("check-report-claims-sweep-no-keyword-");
        std::fs::write(dir.path().join("other-evidence.txt"), "unrelated content\n").unwrap();
        let mut report = base_report(&head);
        report["files_changed"] = json!(["crates/houserules/src/rules/deliverable.rs"]);
        report["implemented"] = json!("See other-evidence.txt for details.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A Windows drive letter's colon belongs to the path: the line suffix
    /// starts at the next colon. A path with no drive letter splits at its
    /// first colon, as before, and keeps the line and drops the column.
    #[test]
    fn find_citations_keeps_a_drive_letter_in_the_path() {
        let cases = [
            (
                "See C:/Users/x/probe.txt:1 for the run.",
                "C:/Users/x/probe.txt",
                vec![lines(1, None)],
            ),
            (
                r"See C:\Users\x\probe.txt:12-14 for the run.",
                r"C:\Users\x\probe.txt",
                vec![lines(12, Some(14))],
            ),
            (
                "See C:/Users/x/probe.txt for the run.",
                "C:/Users/x/probe.txt",
                vec![],
            ),
            (
                "See src/x.rs:7:3 for the run.",
                "src/x.rs",
                vec![lines(7, None)],
            ),
        ];
        for (text, path, expected) in cases {
            let citations = find_citations(text);
            assert_eq!(citations.len(), 1, "{text}");
            assert_eq!(citations[0].path, path, "{text}");
            assert_eq!(citations[0].lines, expected, "{text}");
        }
    }

    // ---- lists of line numbers (HR-148, spec 6.9) ----

    /// A line or a range, for the expected side of a case.
    fn lines(line: u64, end: Option<u64>) -> Lines {
        Lines { line, end }
    }

    /// The elements and the quoted token of the one citation in `text`.
    fn read_one(text: &str) -> (Vec<Lines>, Option<String>) {
        let citations = find_citations(text);
        assert_eq!(citations.len(), 1, "{text}");
        (citations[0].lines.clone(), citations[0].token())
    }

    /// The five examples of spec 6.9, and the corners of the rule: an element
    /// that follows its separator with no space always continues the list; one
    /// after a separator and one space continues it only when the text ends, or
    /// a separator, a closing bracket, or a sentence-ending period follows it.
    #[test]
    fn a_list_of_line_numbers_follows_the_rule_of_the_spec() {
        let cases: [(&str, Vec<Lines>, &str); 20] = [
            (
                "file.rs:3,999 shows",
                vec![lines(3, None), lines(999, None)],
                "file.rs:3,999",
            ),
            (
                "README.md:3,999:",
                vec![lines(3, None), lines(999, None)],
                "README.md:3,999",
            ),
            (
                "see README.md:3, 999.",
                vec![lines(3, None), lines(999, None)],
                "README.md:3, 999",
            ),
            (
                "backlog_parity.rs:6, 42 total",
                vec![lines(6, None)],
                "backlog_parity.rs:6",
            ),
            ("file.rs:12, 400 lines", vec![lines(12, None)], "file.rs:12"),
            (
                "file.rs:3;999 shows",
                vec![lines(3, None), lines(999, None)],
                "file.rs:3;999",
            ),
            (
                "file.rs:2,5-7,9 shows",
                vec![lines(2, None), lines(5, Some(7)), lines(9, None)],
                "file.rs:2,5-7,9",
            ),
            (
                "(file.rs:3, 4)",
                vec![lines(3, None), lines(4, None)],
                "file.rs:3, 4",
            ),
            (
                "[file.rs:3, 4]",
                vec![lines(3, None), lines(4, None)],
                "file.rs:3, 4",
            ),
            (
                "file.rs:3, 4, 5.",
                vec![lines(3, None), lines(4, None), lines(5, None)],
                "file.rs:3, 4, 5",
            ),
            ("file.rs:3, 4.5 ratio", vec![lines(3, None)], "file.rs:3"),
            (
                "file.rs:3, 4. Next",
                vec![lines(3, None), lines(4, None)],
                "file.rs:3, 4",
            ),
            (
                "file.rs:3, 4",
                vec![lines(3, None), lines(4, None)],
                "file.rs:3, 4",
            ),
            (
                "file.rs:3, 4; and more",
                vec![lines(3, None), lines(4, None)],
                "file.rs:3, 4",
            ),
            ("file.rs:3,  4", vec![lines(3, None)], "file.rs:3"),
            ("file.rs:3,x", vec![lines(3, None)], "file.rs:3"),
            (
                "file.rs:3, 4, 5 total",
                vec![lines(3, None), lines(4, None)],
                "file.rs:3, 4",
            ),
            (
                "file.rs:3-5, 9.",
                vec![lines(3, Some(5)), lines(9, None)],
                "file.rs:3-5, 9",
            ),
            ("file.rs:3 , 4", vec![lines(3, None)], "file.rs:3"),
            (
                "file.rs:3,4abc",
                vec![lines(3, None), lines(4, None)],
                "file.rs:3,4",
            ),
        ];
        for (text, expected, token) in cases {
            assert_eq!(
                read_one(text),
                (expected, Some(token.to_string())),
                "{text}"
            );
        }
    }

    /// The nine lists of the corpus run of the rule (`corpus-comma-rule-r2.txt`
    /// of the batch-36 workspace), each with the text that follows it there,
    /// read with the element counts of that run: eight one-word lists in full,
    /// and the count list `backlog_parity.rs:6, 42 total` as one citation.
    #[test]
    fn the_lists_of_the_corpus_read_as_the_corpus_run_read_them() {
        let cases = [
            ("README.md:25,84: the agen", 2),
            ("crates/houserules/src/rules/audit.rs:558,567: two resi", 2),
            ("backlog_parity.rs:6, 42 total", 1),
            ("(SKILL.md:41,46)' underco", 2),
            ("SKILL.md:41,46,55, each quo", 3),
            ("mise.toml:15,26 all pass ", 2),
            ("selfupdate.rs:132-137,275-287 -- Axoupd", 2),
            ("selfupdate.rs:172-184,241-287 -- the \"c", 2),
            ("selfupdate.rs:22,255-258 -- two co", 2),
        ];
        for (text, count) in cases {
            assert_eq!(read_one(text).0.len(), count, "{text}");
        }
    }

    /// A list that restarts with a path is two citations, and the search for the
    /// next citation starts after the end of a spaced list.
    #[test]
    fn a_list_ends_before_the_next_citation_and_widens_the_range() {
        let text = "see a.rs:3, 4. Also b.rs:7";
        let citations = find_citations(text);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].token().as_deref(), Some("a.rs:3, 4"));
        assert_eq!(&text[citations[0].range.clone()], "a.rs:3, 4");
        assert_eq!(citations[1].token().as_deref(), Some("b.rs:7"));
    }

    /// A line number that does not fit 64 bits ends the list; the elements
    /// before it stay.
    #[test]
    fn an_element_that_does_not_fit_64_bits_ends_the_list() {
        let (found, token) = read_one("file.rs:3,99999999999999999999 shows");
        assert_eq!(found, vec![lines(3, None)]);
        assert_eq!(token.as_deref(), Some("file.rs:3"));
    }

    // ---- the body of a listed commit (check 10) ----

    /// A body field leaves out the subject line, is labelled by the JSON path of
    /// the commit with the sha as listed, and keeps only the citations that
    /// resolve to a capture. A fix-round commit gets the path of its round.
    #[test]
    fn a_commit_body_field_is_labelled_by_its_path_and_keeps_only_captures() {
        let (dir, _head) = init_scratch_repo("check-report-claims-commit-body-");
        let root = dir.path();
        let run = clean_run(
            root,
            "evidence/run.txt",
            &["sh", "-c", "true"],
            "11 passed\n",
        );
        assert!(run["capture"].is_string());
        std::fs::write(root.join("lib.rs"), "fn main() {}\n").unwrap();
        let first = commit(
            root,
            "feat: 99 passed in the subject (lib.rs)\n\n12 passed (evidence/run.txt), 7 passed (lib.rs).",
        );
        let second = commit(root, "fix: again\n\n13 passed (evidence/run.txt).");
        let report = json!({
            "commits": [{"sha": first, "subject": "feat"}],
            "fix_rounds": [{"commits": [{"sha": second, "subject": "fix"}]}],
        });

        let fields = collect_commit_body_fields(root, &report);

        let labels: Vec<&str> = fields.iter().map(|field| field.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                format!("commits[0] {first} body"),
                format!("fix_rounds[0].commits[0] {second} body"),
            ]
        );
        assert!(!fields[0].text.contains("99 passed"), "{}", fields[0].text);
        let cited: Vec<&str> = fields[0]
            .citations
            .iter()
            .map(|citation| citation.path.as_str())
            .collect();
        assert_eq!(cited, ["evidence/run.txt"]);
    }

    /// A listed sha that names no commit adds no field.
    #[test]
    fn a_listed_sha_that_names_no_commit_adds_no_body_field() {
        let (dir, _head) = init_scratch_repo("check-report-claims-commit-body-none-");
        let report = json!({"commits": [{"sha": "deadbeef1", "subject": "x"}, {"subject": "y"}]});
        assert!(collect_commit_body_fields(dir.path(), &report).is_empty());
    }
}
