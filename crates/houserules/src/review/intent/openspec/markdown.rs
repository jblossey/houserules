//! The readers of the Markdown of an OpenSpec change (spec 3.3 and 3.4).
//!
//! Each reader reads a text as the OpenSpec 1.15.0 function that spec 2 names
//! reads it, with the patterns of that function (compiled with `regress`, which
//! follows the JavaScript rules for `\s`, `.`, `^`, and `$`):
//!
//! - [`title`] reads the raw text of `proposal.md` (`extractTitle`).
//! - [`why`] and [`delta_counts`] read a text that [`normalized`] made of the
//!   file, as `MarkdownParser` does: a leading BOM dropped, and CRLF and CR
//!   turned into LF.
//! - [`task_counts`] reads the raw text split on LF (`parseTaskLines`), so a
//!   file with CR line ends only is one line.
//! - A fenced block (`code-fence.ts`) masks the headings, sections, and
//!   requirements; it masks no title and no task line.

use std::sync::LazyLock;

use regress::Regex;

use crate::published::intent::Tasks;

/// The most characters of a why.
const WHY_LIMIT: usize = 1_000;

/// The title of a change: the first line of the raw proposal that starts with
/// `# ` or `# Change: ` (flags `im`).
const TITLE: &str = r"^#\s+(?:Change:\s+)?(.+)$";

/// A Markdown heading of one to six marks.
const HEADING: &str = r"^(#{1,6})\s+(.+)$";

/// The end of a section: the part of a heading that `getContentUntilNextHeader`
/// tests. It needs no title, so it reads a line of marks and one white-space
/// character (`## `) as a heading, which [`HEADING`] does not: that pattern
/// needs a title after the white space.
const SECTION_END: &str = r"^(#{1,6})\s+";

/// A level-two heading, which opens a section of a delta file.
const SECTION: &str = r"^(##)\s+(.+)$";

/// The header of a requirement (flag `i`).
const REQUIREMENT: &str = r"^###\s*Requirement:\s*(.+)\s*$";

/// A requirement that a REMOVED section lists as a bullet.
const REMOVED_BULLET: &str = r"^\s*[-*+]\s*`?###\s*Requirement:\s*(.+?)`?\s*$";

/// The `FROM:` line of a rename.
const RENAMED_FROM: &str = r"^\s*[-*+]?\s*FROM:\s*`?###\s*Requirement:\s*(.+?)`?\s*$";

/// The `TO:` line of a rename.
const RENAMED_TO: &str = r"^\s*[-*+]?\s*TO:\s*`?###\s*Requirement:\s*(.+?)`?\s*$";

/// A task line: a list item with a checkbox of at most one token. The first
/// group is the mark of the box.
const TASK: &str = r"^\s*(?:[-*+]|\d{1,9}[.)])\s*\[(?:\s*([^\]\s]?)\s*\](?![([])|\s+\])\s*(.*)";

/// A change id: lowercase letters and digits in groups with single hyphens.
const CHANGE_ID: &str = r"^[a-z0-9]+(?:-[a-z0-9]+)*$";

/// The line that opens a fenced block. The first group is the fence.
const FENCE_OPEN: &str = r"^\s*(`{3,}|~{3,})";

/// The line that can close a fenced block. The first group is the fence.
const FENCE_CLOSE: &str = r"^\s*(`{3,}|~{3,})\s*$";

/// Compiles one of the constant patterns above.
fn compile(pattern: &str, flags: &str) -> Regex {
    Regex::with_flags(pattern, flags).expect("a constant pattern compiles")
}

static TITLE_RE: LazyLock<Regex> = LazyLock::new(|| compile(TITLE, "im"));
static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| compile(HEADING, ""));
static SECTION_END_RE: LazyLock<Regex> = LazyLock::new(|| compile(SECTION_END, ""));
static SECTION_RE: LazyLock<Regex> = LazyLock::new(|| compile(SECTION, ""));
static REQUIREMENT_RE: LazyLock<Regex> = LazyLock::new(|| compile(REQUIREMENT, "i"));
static REMOVED_BULLET_RE: LazyLock<Regex> = LazyLock::new(|| compile(REMOVED_BULLET, ""));
static RENAMED_FROM_RE: LazyLock<Regex> = LazyLock::new(|| compile(RENAMED_FROM, ""));
static RENAMED_TO_RE: LazyLock<Regex> = LazyLock::new(|| compile(RENAMED_TO, ""));
static TASK_RE: LazyLock<Regex> = LazyLock::new(|| compile(TASK, ""));
static CHANGE_ID_RE: LazyLock<Regex> = LazyLock::new(|| compile(CHANGE_ID, ""));
static FENCE_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| compile(FENCE_OPEN, ""));
static FENCE_CLOSE_RE: LazyLock<Regex> = LazyLock::new(|| compile(FENCE_CLOSE, ""));

/// The text of capture group `group` of the first match of `pattern` in
/// `text`.
fn capture<'t>(pattern: &Regex, text: &'t str, group: usize) -> Option<&'t str> {
    let found = pattern.find(text)?;
    found.group(group).map(|range| &text[range])
}

/// Whether `pattern` matches `text`.
fn matches(pattern: &Regex, text: &str) -> bool {
    pattern.find(text).is_some()
}

/// Whether `c` is white space for JavaScript's `String.prototype.trim`: the
/// white space and the line terminators of ECMAScript, and the byte order mark.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `text` without the white space of JavaScript's `trim` around it.
fn js_trim(text: &str) -> &str {
    text.trim_matches(is_js_space)
}

/// `text` as `MarkdownParser` reads it: a leading UTF-8 BOM dropped, and CRLF
/// and CR turned into LF.
pub(super) fn normalized(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// For each of `lines`, whether a fenced block holds it, the fence lines
/// included (`buildCodeFenceMask`). A fence closes at a line of the same mark,
/// at least as long, with nothing after it.
fn fence_mask(lines: &[&str]) -> Vec<bool> {
    let mut mask = vec![false; lines.len()];
    let mut open: Option<(char, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        match open {
            None => {
                if let Some(fence) = capture(&FENCE_OPEN_RE, line, 1) {
                    open = fence.chars().next().map(|mark| (mark, fence.len()));
                    mask[index] = true;
                }
            }
            Some((mark, length)) => {
                mask[index] = true;
                let closes = capture(&FENCE_CLOSE_RE, line, 1)
                    .is_some_and(|fence| fence.starts_with(mark) && fence.len() >= length);
                if closes {
                    open = None;
                }
            }
        }
    }
    mask
}

/// The title of the change `reference` from the raw text of its proposal: the
/// first line that starts with `# ` or `# Change: `, trimmed. `reference`
/// stands for a text with no such line and for a title that reads `proposal` in
/// any case, which names the document and not the change.
pub(super) fn title(raw: &str, reference: &str) -> String {
    match capture(&TITLE_RE, raw, 1).map(js_trim) {
        Some(title) if !title.is_empty() && title.to_lowercase() != "proposal" => title.to_string(),
        _ => reference.to_string(),
    }
}

/// The why of a proposal: the lines below the first heading, of any level,
/// titled `Why` in any case and outside a fence, up to the next heading of the
/// same or a higher level, trimmed and cut to [`WHY_LIMIT`] characters. A
/// subsection belongs to its section. `None` when there is no such heading,
/// and when its section is empty; a later `Why` heading is not read then.
pub(super) fn why(text: &str) -> Option<String> {
    let text = normalized(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let mask = fence_mask(&lines);
    for (index, line) in lines.iter().enumerate() {
        if mask[index] {
            continue;
        }
        let Some(found) = HEADING_RE.find(line) else {
            continue;
        };
        let marks = found.group(1).map_or(0, |range| range.len());
        let heading = found.group(2).map_or("", |range| js_trim(&line[range]));
        if heading.to_lowercase() != "why" {
            continue;
        }
        let section: Vec<&str> = lines[index + 1..]
            .iter()
            .zip(&mask[index + 1..])
            .take_while(|(later, fenced)| {
                **fenced
                    || !capture(&SECTION_END_RE, later, 1).is_some_and(|ends| ends.len() <= marks)
            })
            .map(|(later, _)| *later)
            .collect();
        let content = js_trim(&section.join("\n")).to_string();
        return (!content.is_empty()).then(|| content.chars().take(WHY_LIMIT).collect());
    }
    None
}

/// The counts of the task lines of the raw `text`, split on LF: every line that
/// matches [`TASK`], fenced or not; the lines whose box holds `x` or `X` are
/// done.
pub(super) fn task_counts(text: &str) -> Tasks {
    let mut counts = Tasks { done: 0, total: 0 };
    for line in text.split('\n') {
        if let Some(found) = TASK_RE.find(line) {
            let mark = found.group(1).map_or("", |range| &line[range]);
            counts.total += 1;
            counts.done += u64::from(mark.to_lowercase() == "x");
        }
    }
    counts
}

/// The entries of the four delta sections of one file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Deltas {
    /// The requirements of the ADDED sections.
    pub(super) added: u64,
    /// The requirements of the MODIFIED sections.
    pub(super) modified: u64,
    /// The requirements of the REMOVED sections.
    pub(super) removed: u64,
    /// The pairs of the RENAMED sections.
    pub(super) renamed: u64,
}

impl std::ops::AddAssign for Deltas {
    fn add_assign(&mut self, other: Self) {
        self.added += other.added;
        self.modified += other.modified;
        self.removed += other.removed;
        self.renamed += other.renamed;
    }
}

/// The entries of the delta sections of the spec file `text`: the level-two
/// sections outside fences that are titled `ADDED Requirements`, `MODIFIED
/// Requirements`, `REMOVED Requirements`, or `RENAMED Requirements` in any
/// case. A section counts each requirement header outside a fence (ADDED and
/// MODIFIED), each header or bullet (REMOVED), or each `FROM:` line that a
/// `TO:` line follows with no `FROM:` line between (RENAMED); a repeated
/// section counts again, and a pair never spans two sections. `None` when the
/// file holds none of the four sections.
pub(super) fn delta_counts(text: &str) -> Option<Deltas> {
    let text = normalized(text);
    let lines: Vec<&str> = text.split('\n').collect();
    let mask = fence_mask(&lines);
    let sections: Vec<(usize, &str)> = lines
        .iter()
        .enumerate()
        .filter(|(index, _)| !mask[*index])
        .filter_map(|(index, line)| {
            let title = capture(&SECTION_RE, line, 2)?;
            Some((index, js_trim(title)))
        })
        .collect();
    let mut found = false;
    let mut counts = Deltas::default();
    for (position, (start, title)) in sections.iter().enumerate() {
        let end = sections
            .get(position + 1)
            .map_or(lines.len(), |(next, _)| *next);
        let body = (start + 1..end)
            .filter(|index| !mask[*index])
            .map(|index| lines[index]);
        let title = title.to_lowercase();
        let in_section = match title.as_str() {
            "added requirements" => {
                counts.added += body.filter(|line| matches(&REQUIREMENT_RE, line)).count() as u64;
                true
            }
            "modified requirements" => {
                counts.modified +=
                    body.filter(|line| matches(&REQUIREMENT_RE, line)).count() as u64;
                true
            }
            "removed requirements" => {
                counts.removed += body
                    .filter(|line| {
                        matches(&REQUIREMENT_RE, line) || matches(&REMOVED_BULLET_RE, line)
                    })
                    .count() as u64;
                true
            }
            "renamed requirements" => {
                counts.renamed += rename_pairs(body);
                true
            }
            _ => false,
        };
        found |= in_section;
    }
    found.then_some(counts)
}

/// The pairs of `lines`, the body of one RENAMED section: a `FROM:` line and
/// the `TO:` line after it, with no `FROM:` line between. A second `FROM:` line
/// replaces the first, and a `TO:` line with no `FROM:` line before it counts
/// for nothing.
fn rename_pairs<'a>(lines: impl Iterator<Item = &'a str>) -> u64 {
    let mut pending = false;
    let mut pairs = 0;
    for line in lines {
        if matches(&RENAMED_FROM_RE, line) {
            pending = true;
        } else if matches(&RENAMED_TO_RE, line) && pending {
            pairs += 1;
            pending = false;
        }
    }
    pairs
}

/// Whether `id` is a change id: kebab-case.
pub(super) fn is_change_id(id: &str) -> bool {
    matches(&CHANGE_ID_RE, id)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    /// The spec of batch 38, which section 3.4 copies the patterns from.
    fn spec() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("docs")
            .join("specs")
            .join("2026-10-25-batch-38-openspec-and-release.md");
        fs::read_to_string(path).expect("read the spec")
    }

    /// The counts of a delta file.
    fn deltas(added: u64, modified: u64, removed: u64, renamed: u64) -> Option<Deltas> {
        Some(Deltas {
            added,
            modified,
            removed,
            renamed,
        })
    }

    // ---- the patterns (spec 3.4) ---------------------------------------------------------

    /// Each pattern of spec 3.4 is the text of the spec; the pattern for `TO:`
    /// is the pattern for `FROM:` with the word replaced.
    #[test]
    fn the_patterns_are_the_patterns_of_the_spec() {
        let spec = spec();

        for pattern in [
            TITLE,
            HEADING,
            SECTION,
            REQUIREMENT,
            REMOVED_BULLET,
            RENAMED_FROM,
            TASK,
            CHANGE_ID,
        ] {
            assert!(spec.contains(pattern), "{pattern}");
        }
        assert_eq!(RENAMED_FROM.replace("FROM", "TO"), RENAMED_TO);
    }

    /// The title pattern reads the first line that starts with `# ` or with
    /// `# Change: `, and its `\s+` crosses a line end, as OpenSpec's does.
    #[test]
    fn the_title_pattern_matches_a_heading_of_level_one() {
        assert_eq!(title("# Add X\n\nbody\n", "ref"), "Add X");
        assert_eq!(title("# Change: Add X\n", "ref"), "Add X");
        assert_eq!(title("# change:  Add X  \n", "ref"), "Add X");
        assert_eq!(title("text\n\n# Later\n# Earlier\n", "ref"), "Later");
        assert_eq!(title("## Level two\n#NoSpace\n", "ref"), "ref");
        assert_eq!(title("  # Indented\n", "ref"), "ref");
        assert_eq!(title("#\n\nNot A Heading\n", "ref"), "Not A Heading");
    }

    /// A title of `proposal`, in any case, names the document and not the
    /// change; so does no title at all.
    #[test]
    fn a_title_that_reads_proposal_is_the_ref() {
        assert_eq!(title("# Proposal\n", "add-x"), "add-x");
        assert_eq!(title("# PROPOSAL \n", "add-x"), "add-x");
        assert_eq!(title("# Change: proposal\n", "add-x"), "add-x");
        assert_eq!(title("# Proposals\n", "add-x"), "Proposals");
        assert_eq!(title("", "add-x"), "add-x");
    }

    /// The title reads the raw text: a BOM keeps the first line from matching,
    /// a fence masks nothing, and a CRLF line end stays out of the title.
    #[test]
    fn the_title_reads_the_raw_text() {
        assert_eq!(title("\u{feff}# Bom Title\n", "add-x"), "add-x");
        assert_eq!(
            title("```md\n# Fenced Title\n```\n# Real Title\n", "x"),
            "Fenced Title"
        );
        assert_eq!(title("# Crlf Title\r\n\r\nbody\r\n", "x"), "Crlf Title");
    }

    /// The heading pattern: one to six `#`, white space, and a title.
    #[test]
    fn the_heading_pattern_matches_one_to_six_marks_and_a_title() {
        let heading = regress::Regex::new(HEADING).expect("compile");

        for line in ["# a", "###### a", "## a b", "#\ta"] {
            assert!(heading.find(line).is_some(), "{line:?}");
        }
        for line in ["####### a", "#a", "#", "# ", " # a", "text # a"] {
            assert!(heading.find(line).is_none(), "{line:?}");
        }
    }

    /// The pattern that ends a section also ends it at a line that holds one
    /// white-space character after the marks and no title (`##` and a space, or
    /// `##` and a tab), which the heading pattern does not read as a heading
    /// (OpenSpec's `getContentUntilNextHeader`).
    #[test]
    fn a_line_of_marks_and_one_white_space_character_ends_a_section_and_opens_none() {
        let heading = regress::Regex::new(HEADING).expect("compile");

        for ending in ["## ", "##\t"] {
            assert!(heading.find(ending).is_none(), "{ending:?}");
            let text = format!("## Why\nOne.\n{ending}\nTwo.\n");

            assert_eq!(why(&text), Some("One.".to_string()), "{ending:?}");
        }
    }

    // ---- the why -------------------------------------------------------------------------

    /// The why is the lines below the first `Why` heading, whatever its level
    /// and case, up to the next heading of the same or a higher level.
    #[test]
    fn the_why_is_the_section_of_the_first_why_heading_of_any_level() {
        assert_eq!(
            why("# T\n\n## Why\nBecause.\n\n## What Changes\n- x\n"),
            Some("Because.".to_string())
        );
        assert_eq!(why("## WHY\nUpper.\n"), Some("Upper.".to_string()));
        assert_eq!(why("###### why\nSix.\n"), Some("Six.".to_string()));
        assert_eq!(
            why("# Why\nLevel one.\n\n## What Changes\n- x\n"),
            Some("Level one.\n\n## What Changes\n- x".to_string())
        );
        assert_eq!(why("# T\n\n## Motivation\nNo why here.\n"), None);
        assert_eq!(why("# Why not\nA different title.\n"), None);
    }

    /// A subsection belongs to its Why, with its lines kept; a heading of the
    /// same level ends the section; the text is trimmed.
    #[test]
    fn a_subsection_belongs_to_its_why() {
        assert_eq!(
            why("## Why\n\n  First.\n\n### Detail\nMore.\n\n## Next\nOther.\n"),
            Some("First.\n\n### Detail\nMore.".to_string())
        );
        assert_eq!(
            why("### Why\nA.\n#### Sub\nB.\n### Same level\nC.\n"),
            Some("A.\n#### Sub\nB.".to_string())
        );
    }

    /// The first `Why` heading in the document decides, even when it is empty;
    /// a `Why` heading in a fence is no heading.
    #[test]
    fn the_first_why_heading_decides_and_a_fenced_one_is_no_heading() {
        assert_eq!(why("## Why\n\n## Why\nSecond.\n"), None);
        assert_eq!(
            why("```md\n## Why\nFenced.\n```\n## Why\nReal.\n"),
            Some("Real.".to_string())
        );
        assert_eq!(why("~~~\n## Why\nFenced.\n~~~\n"), None);
    }

    /// A fence closes at a fence of the same mark and at least the same length,
    /// alone on its line; a shorter or another fence stays inside.
    #[test]
    fn a_fence_closes_at_the_same_mark_with_at_least_the_same_length() {
        let text = "````md\n```\n## Why\nFenced.\n~~~~\n````\n## Why\nReal.\n";

        assert_eq!(why(text), Some("Real.".to_string()));
        assert_eq!(
            why("```\n## Why\nFenced.\n``` text\n## Why\nStill fenced.\n"),
            None
        );
    }

    /// The text is normalized first: a BOM is dropped, and CRLF and CR turn
    /// into LF.
    #[test]
    fn the_why_reads_a_normalized_text() {
        assert_eq!(
            why("\u{feff}# T\r\n\r\n## Why\r\nOne.\r\nTwo.\r\n\r\n## Next\r\n"),
            Some("One.\nTwo.".to_string())
        );
        assert_eq!(why("## Why\rOne.\rTwo.\r"), Some("One.\nTwo.".to_string()));
    }

    /// The why is cut to 1,000 characters, not bytes.
    #[test]
    fn the_why_is_cut_to_1000_characters() {
        let long = format!("## Why\n{}\n", "ä".repeat(1500));
        let exact = format!("## Why\n{}\n", "b".repeat(1000));

        assert_eq!(why(&long), Some("ä".repeat(1000)));
        assert_eq!(why(&exact), Some("b".repeat(1000)));
    }

    // ---- the tasks -----------------------------------------------------------------------

    /// The counts of `text`: the lines that match, and the lines of them that
    /// are done.
    fn counts(text: &str) -> (u64, u64) {
        let tasks = task_counts(text);
        (tasks.done, tasks.total)
    }

    /// Every list marker of the pattern and every checkbox that holds at most
    /// one token counts; only `x` and `X` are done.
    #[test]
    fn the_task_pattern_counts_each_marker_and_each_box() {
        let text = concat!(
            "- [x] dash\n",
            "* [X] star\n",
            "+ [ ] plus\n",
            "1. [x] number and dot\n",
            "12) [ ] number and paren\n",
            "  - [ ] nested\n",
            "- [] empty\n",
            "- [~] other mark\n",
            "- [ x ] padded\n",
            "-[x] no space after the marker\n",
            "- [x]no space after the box\n",
            "- [ ](./link)\n",
            "- [ ][ref]\n",
        );

        assert_eq!(counts(text), (6, 13));
    }

    /// A link bullet, a multi-character label, a bare box, a number with ten
    /// digits, and a line that does not start with a marker are no task.
    #[test]
    fn a_line_that_is_no_task_counts_for_nothing() {
        let text = concat!(
            "- [A](https://example.com)\n",
            "- [1](./one)\n",
            "- [WIP] label\n",
            "- [xx] two marks\n",
            "[x] bare\n",
            "1234567890. [x] ten digits\n",
            "text - [x] after text\n",
            "- plain item\n",
            "- [x\n",
        );

        assert_eq!(counts(text), (0, 0));
    }

    /// A fenced line counts: OpenSpec's reader drops no fence.
    #[test]
    fn a_task_line_in_a_fence_counts() {
        assert_eq!(counts("- [x] real\n```md\n- [ ] example\n```\n"), (1, 2));
    }

    /// The reader splits the raw text on LF, as OpenSpec's `parseTaskLines`
    /// does: a CRLF line counts, a BOM stays in the first line (`\s` reads it),
    /// and a file with CR line ends only is one line (spec 3.1).
    #[test]
    fn the_tasks_read_the_raw_text_split_on_line_feeds() {
        assert_eq!(counts("\u{feff}- [x] a\r\n- [ ] b\r\n"), (1, 2));
        assert_eq!(counts("- [x] a\r- [ ] b\r"), (1, 1));
        assert_eq!(counts("- [ ] a\r- [x] b\r"), (0, 1));
        assert_eq!(counts("- [x] a\r- [ ] b\r\n- [ ] c\n"), (1, 2));
        assert_eq!(counts(""), (0, 0));
    }

    // ---- the requirements ----------------------------------------------------------------

    /// Each of the four sections counts its own entries, and a section of the
    /// wrong case still counts.
    #[test]
    fn the_four_sections_count_their_entries() {
        let text = concat!(
            "## ADDED Requirements\n",
            "### Requirement: One\n",
            "text\n",
            "#### Scenario: s\n",
            "### Requirement: Two\n",
            "## modified requirements\n",
            "### Requirement: Three\n",
            "## REMOVED Requirements\n",
            "### Requirement: Four\n",
            "- `### Requirement: Five`\n",
            "* ### Requirement: Six\n",
            "## Renamed Requirements\n",
            "- FROM: `### Requirement: Old`\n",
            "- TO: `### Requirement: New`\n",
        );

        assert_eq!(delta_counts(text), deltas(2, 1, 3, 1));
    }

    /// A file with none of the four sections holds no delta.
    #[test]
    fn a_file_with_no_delta_section_gives_none() {
        assert_eq!(delta_counts(""), None);
        assert_eq!(
            delta_counts("## Purpose\nx\n## Requirements\n### Requirement: Old\n"),
            None
        );
        assert_eq!(
            delta_counts("## ADDED  Requirements\n### Requirement: A\n"),
            None
        );
    }

    /// A section with no entry is a delta section: the counts are zero.
    #[test]
    fn an_empty_delta_section_gives_zero_counts() {
        assert_eq!(
            delta_counts("## ADDED Requirements\n\nnone\n"),
            deltas(0, 0, 0, 0)
        );
    }

    /// A header counts when it starts the line (`###` then optional white
    /// space, then `Requirement:`, any case); an indented one does not, and a
    /// header in a fence is no header.
    #[test]
    fn only_a_header_at_the_start_of_a_line_outside_a_fence_counts() {
        let text = concat!(
            "## ADDED Requirements\n",
            "###Requirement: No space\n",
            "### requirement: Lower\n",
            "   ### Requirement: Indented\n",
            "#### Requirement: Level four\n",
            "```md\n",
            "### Requirement: Fenced\n",
            "```\n",
            "### Requirement: Real\n",
        );

        assert_eq!(delta_counts(text), deltas(3, 0, 0, 0));
    }

    /// A section heading in a fence opens no section; a section ends at the
    /// next level-two heading, and a repeated section counts again.
    #[test]
    fn sections_end_at_the_next_level_two_heading_and_a_repeat_counts_again() {
        let text = concat!(
            "## ADDED Requirements\n",
            "### Requirement: A\n",
            "```md\n",
            "## REMOVED Requirements\n",
            "### Requirement: Fenced\n",
            "```\n",
            "## Notes\n",
            "### Requirement: Outside\n",
            "## ADDED Requirements\n",
            "### Requirement: B\n",
        );

        assert_eq!(delta_counts(text), deltas(2, 0, 0, 0));
    }

    /// A rename is a `FROM:` line followed by a `TO:` line with no `FROM:`
    /// between; a lone line of either kind counts for nothing, and a pair never
    /// spans two sections.
    #[test]
    fn a_rename_is_a_pair_and_a_lone_line_counts_for_nothing() {
        let text = concat!(
            "## RENAMED Requirements\n",
            "- FROM: `### Requirement: A`\n",
            "- TO: `### Requirement: B`\n",
            "- FROM: `### Requirement: Lonely`\n",
            "- FROM: `### Requirement: C`\n",
            "- TO: `### Requirement: D`\n",
            "- TO: `### Requirement: Orphan`\n",
            "## RENAMED Requirements\n",
            "- TO: `### Requirement: Other section`\n",
        );

        assert_eq!(delta_counts(text), deltas(0, 0, 0, 2));
    }

    /// The bullet of a rename is optional, and any list marker serves.
    #[test]
    fn a_rename_needs_no_bullet_and_takes_any_list_marker() {
        let text = concat!(
            "## RENAMED Requirements\n",
            "FROM: `### Requirement: A`\n",
            "TO: `### Requirement: B`\n",
            "* FROM: ### Requirement: C\n",
            "+ TO: ### Requirement: D\n",
        );

        assert_eq!(delta_counts(text), deltas(0, 0, 0, 2));
    }

    /// The reader normalizes first: a BOM before the first section, and CRLF.
    #[test]
    fn the_requirements_read_a_normalized_text() {
        let text =
            "\u{feff}## ADDED Requirements\r\n### Requirement: A\r\ntext\r\n### Requirement: B\r\n";

        assert_eq!(delta_counts(text), deltas(2, 0, 0, 0));
    }

    // ---- the id --------------------------------------------------------------------------

    /// A change id is kebab-case: lowercase letters and digits in groups with
    /// single hyphens between them.
    #[test]
    fn a_change_id_is_kebab_case() {
        for id in ["a", "add-x", "a1-b2", "2026-01-02-old-change", "0"] {
            assert!(is_change_id(id), "{id}");
        }
        for id in [
            "", "-a", "a-", "a--b", "A", "a_b", "a b", "a.b", "a/b", "é", "ab\n", ".hidden",
        ] {
            assert!(!is_change_id(id), "{id:?}");
        }
    }
}
