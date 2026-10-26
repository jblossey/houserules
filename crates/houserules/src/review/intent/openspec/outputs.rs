//! The patterns of an artifact's `generates` glob, as OpenSpec 1.15.0 resolves
//! them (spec 3.3).
//!
//! `resolveArtifactOutputs` (`artifact-graph/outputs.ts`) expands the brace
//! lists of a glob before it matches any file. [`alternatives`] ports that
//! step with the same patterns, the same bounds, and the same refusals:
//!
//! 1. The glob nests no more than 16 levels of `{` and `(`
//!    (`assertBraceNesting`: a class and a quoted run hide theirs).
//! 2. `expandArtifactBraces` takes the outermost group that has a comma or `..`
//!    of its own, reading no class, and expands it with the rules of
//!    `brace-expansion` 5.0.12 (the version of OpenSpec's lock file): a group
//!    nested in an item expands in the same pass, so a nest of `d` groups gives
//!    `d + 1` patterns. A group with no comma stays as it is, braces
//!    included. An empty result of a group is dropped, so a group that expands
//!    to nothing leaves the pattern as it is. The expansion refuses more than
//!    [`MAX_ALTERNATIVES`] patterns.
//! 3. The static directory prefix of each pattern (`globBase`) stays inside the
//!    change folder, judged by its path alone (`assertPathWithin`). A prefix
//!    that climbs out of the folder and back in by the folder's own name counts
//!    as outside here, because the reader knows no folder name.
//!
//! OpenSpec throws at each refusal and gives no count. [`alternatives`] gives
//! `None`, and the caller counts the `tasks.md` fallback: a departure that spec
//! 3.3 records. A brace range such as `{1..3}` does not expand either: it stays
//! literal, as spec 3.3 records. The checks that follow a symlink
//! (`assertGlobDirectoryTraversal`) have no counterpart, because a symlink is
//! no file of a change (spec 3.1).

use std::sync::LazyLock;

use regress::Regex;

/// OpenSpec's test for an extglob in a pattern (`EXTGLOB_RE` of `outputs.ts`).
const EXTGLOB: &str = r"[!*+?@]\([^(]*\)";

/// The compiled [`EXTGLOB`].
static EXTGLOB_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(EXTGLOB).expect("a constant pattern compiles"));

/// The deepest nesting of `{` and `(` that a glob may hold (`MAX_BRACE_NESTING`
/// of `outputs.ts`).
const MAX_NESTING: usize = 16;

/// The most patterns that the brace groups of one glob may expand to: OpenSpec's
/// own bound (`MAX_BRACE_EXPANSIONS` of `outputs.ts`), which it refuses beyond.
const MAX_ALTERNATIVES: usize = 1_024;

/// Stands for the opening brace of a group that does not expand, so that the
/// scan for the next group skips it (`LITERAL_BRACE_OPEN` of `outputs.ts`). A
/// NUL byte cannot occur in a pattern that [`alternatives`] accepts.
const LITERAL_OPEN: &str = "\0<";

/// Stands for the closing brace of a group that does not expand.
const LITERAL_CLOSE: &str = "\0>";

/// Whether `pattern` is a glob for OpenSpec (`isGlobPattern`): a wildcard, a
/// class, an extglob, or a brace group with a comma or `..`. Any other pattern
/// names one file.
pub(super) fn is_glob(pattern: &str) -> bool {
    pattern.contains(['*', '?', '['])
        || EXTGLOB_RE.find(pattern).is_some()
        || has_brace_expansion(pattern)
}

/// Whether a brace group of `pattern` holds a comma or `..`.
fn has_brace_expansion(pattern: &str) -> bool {
    let mut openings = Vec::new();
    for (index, c) in pattern.char_indices() {
        match c {
            '{' => openings.push(index),
            '}' => {
                if let Some(opening) = openings.pop() {
                    let group = &pattern[opening..index];
                    if group.contains(',') || group.contains("..") {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// The patterns that OpenSpec 1.15.0 matches for the glob `pattern`, whose runs
/// of slashes are single: its alternatives after the brace expansion. `None`
/// when OpenSpec refuses the glob with an error of its own (see the module
/// doc).
pub(super) fn alternatives(pattern: &str) -> Option<Vec<String>> {
    if nests_too_deep(pattern) {
        return None;
    }
    let patterns = expand(pattern)?;
    patterns
        .iter()
        .all(|alternative| stays_inside_folder(alternative))
        .then_some(patterns)
}

/// Whether `pattern` nests `{` or `(` deeper than [`MAX_NESTING`]
/// (`assertBraceNesting`). A closer closes only the innermost opener of its own
/// kind; a class and a run in quotes are literal.
fn nests_too_deep(pattern: &str) -> bool {
    let chars: Vec<char> = pattern.chars().collect();
    let mut open: Vec<char> = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '[' => {
                let mut brackets = 1;
                while brackets > 0 && index + 1 < chars.len() {
                    index += 1;
                    match chars[index] {
                        '[' => brackets += 1,
                        ']' => brackets -= 1,
                        _ => {}
                    }
                }
            }
            quote @ ('"' | '\'' | '`') => {
                index = chars[index + 1..]
                    .iter()
                    .position(|c| *c == quote)
                    .map_or(chars.len(), |offset| index + 1 + offset);
            }
            opener @ ('{' | '(') => {
                open.push(opener);
                if open.len() > MAX_NESTING {
                    return true;
                }
            }
            '}' if open.last() == Some(&'{') => {
                open.pop();
            }
            ')' if open.last() == Some(&'(') => {
                open.pop();
            }
            _ => {}
        }
        index += 1;
    }
    false
}

/// A brace group of a pattern, with the text around it.
struct Group<'a> {
    /// The text before the group.
    pre: String,
    /// The text between the braces.
    body: &'a str,
    /// The text after the group.
    post: String,
}

/// Whether `body`, the text of a group, holds a comma or `..` outside its
/// nested groups (`hasTopLevelExpansion`).
fn has_separator(body: &str) -> bool {
    let bytes = body.as_bytes();
    let mut depth = 0usize;
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => return true,
            b'.' if depth == 0 && bytes.get(index + 1) == Some(&b'.') => return true,
            _ => {}
        }
    }
    false
}

/// The first outermost group of `pattern` that expands (`findTopLevelExpansion`):
/// one with a separator of its own. A group with none, such as `{prefix{a,b}}`,
/// keeps its braces, and the search goes on inside it. A class hides nothing.
fn find_group(pattern: &str) -> Option<Group<'_>> {
    let mut stack: Vec<usize> = Vec::new();
    for (index, byte) in pattern.bytes().enumerate() {
        match byte {
            b'{' => stack.push(index),
            b'}' => {
                let Some(open) = stack.pop() else { continue };
                if !stack.is_empty() {
                    continue;
                }
                let body = &pattern[open + 1..index];
                if has_separator(body) {
                    return Some(Group {
                        pre: pattern[..open].to_string(),
                        body,
                        post: pattern[index + 1..].to_string(),
                    });
                }
                if let Some(nested) = find_group(body) {
                    return Some(Group {
                        pre: format!("{}{}", &pattern[..=open], nested.pre),
                        body: nested.body,
                        post: format!("{}{}", nested.post, &pattern[index..]),
                    });
                }
            }
            _ => {}
        }
    }
    None
}

/// The patterns that the brace groups of `pattern` expand to, one group at a time
/// (`expandArtifactBraces`); `pattern` itself when none is left. `None` when the
/// pattern holds a NUL byte or expands to more than [`MAX_ALTERNATIVES`]
/// patterns.
fn expand(pattern: &str) -> Option<Vec<String>> {
    if pattern.contains('\0') {
        return None;
    }
    let mut pending = vec![pattern.to_string()];
    let mut finished: Vec<String> = Vec::new();
    while let Some(current) = pending.pop() {
        let Some(group) = find_group(&current) else {
            finished.push(
                current
                    .replace(LITERAL_OPEN, "{")
                    .replace(LITERAL_CLOSE, "}"),
            );
            continue;
        };
        let wrapped = format!("{{{}}}", group.body);
        let outstanding = pending.len() + finished.len();
        let budget = MAX_ALTERNATIVES.saturating_sub(outstanding);
        let expanded = expand_text(&wrapped, budget + 1, true);
        if expanded.len() == 1 && expanded[0] == wrapped {
            pending.push(format!(
                "{}{LITERAL_OPEN}{}{LITERAL_CLOSE}{}",
                group.pre, group.body, group.post
            ));
            continue;
        }
        if outstanding + expanded.len() > MAX_ALTERNATIVES {
            return None;
        }
        pending.extend(
            expanded
                .iter()
                .map(|alternative| format!("{}{alternative}{}", group.pre, group.post)),
        );
    }
    let normalized: Vec<String> = finished
        .iter()
        .map(|alternative| single_slashes(alternative))
        .filter(|alternative| !alternative.is_empty())
        .collect();
    Some(if normalized.is_empty() {
        vec![pattern.to_string()]
    } else {
        normalized
    })
}

/// `text` with every run of slashes made one.
fn single_slashes(text: &str) -> String {
    let mut single = String::with_capacity(text.len());
    for c in text.chars() {
        if c != '/' || !single.ends_with('/') {
            single.push(c);
        }
    }
    single
}

/// The first balanced pair of braces of a text.
struct Balanced<'a> {
    /// The text before the opening brace.
    pre: &'a str,
    /// The text between the braces.
    body: &'a str,
    /// The text after the closing brace.
    post: &'a str,
}

/// The first pair of braces of `text` with the text around it (`balanced` of
/// `balanced-match`), or `None` when the first opening brace has no closing
/// brace. The texts that [`expand_text`] reads are balanced.
fn balanced(text: &str) -> Option<Balanced<'_>> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    for (index, byte) in text.bytes().enumerate().skip(start) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(Balanced {
                        pre: &text[..start],
                        body: &text[start + 1..index],
                        post: &text[index + 1..],
                    });
                }
            }
            _ => {}
        }
    }
    None
}

/// The items of the group body `body`: its text split at the commas that stand
/// outside its nested groups (`parseCommaParts`).
fn comma_parts(body: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut carry = String::new();
    let mut rest = body;
    loop {
        let Some(group) = balanced(rest) else {
            let mut tail = rest.split(',').map(str::to_string);
            if let Some(first) = tail.next() {
                parts.push(carry + &first);
            }
            parts.extend(tail);
            return parts;
        };
        let mut pieces: Vec<String> = group.pre.split(',').map(str::to_string).collect();
        if let Some(first) = pieces.first_mut() {
            first.insert_str(0, &carry);
        }
        if let Some(last) = pieces.last_mut() {
            last.push_str(&format!("{{{}}}", group.body));
        }
        if group.post.is_empty() {
            parts.extend(pieces);
            return parts;
        }
        carry = pieces.pop().unwrap_or_default();
        parts.extend(pieces);
        rest = group.post;
    }
}

/// Whether `text` holds a comma that no comma follows, and a closing brace after
/// it (`/,(?!,).*\}/` of `brace-expansion`).
fn comma_then_brace(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        *byte == b',' && bytes.get(index + 1) != Some(&b',') && bytes[index + 1..].contains(&b'}')
    })
}

/// Each of `acc` followed by `pre` and one of `values`, at most `limit` of them
/// (`combine` of `brace-expansion`); the empty strings stay out when
/// `drop_empties` is set.
fn combine(
    acc: &[String],
    pre: &str,
    values: &[String],
    limit: usize,
    drop_empties: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    for prefix in acc {
        for value in values {
            if out.len() >= limit {
                return out;
            }
            let expansion = format!("{prefix}{pre}{value}");
            if drop_empties && expansion.is_empty() {
                continue;
            }
            out.push(expansion);
        }
    }
    out
}

/// What `brace-expansion` 5.0.12 gives for `text` (`expand_`), at most `limit`
/// strings: the comma groups expand left to right, each item with its own
/// groups first. A group with no comma stays as it is, and so does a range such
/// as `{1..3}`, which spec 3.3 keeps literal. A group whose opening brace follows
/// a `$` stays as it is in this text, and [`expand`] finds it in a later pass.
/// `is_top` drops an empty result, as Bash does for the first group of a string.
fn expand_text(text: &str, limit: usize, is_top: bool) -> Vec<String> {
    let mut acc = vec![String::new()];
    let mut rest = text;
    let mut is_top = is_top;
    let mut drop_empties = false;
    let mut first_group = true;
    let once = [String::new()];
    loop {
        let Some(found) = balanced(rest) else {
            return combine(&acc, rest, &once, limit, drop_empties);
        };
        let Balanced { pre, body, post } = found;
        let literal = format!("{pre}{{{body}}}");
        if pre.ends_with('$') {
            acc = combine(
                &acc,
                &literal,
                &once,
                limit,
                drop_empties && post.is_empty(),
            );
            first_group = false;
            if post.is_empty() {
                break;
            }
            rest = post;
            continue;
        }
        if !body.contains(',') {
            if comma_then_brace(post) {
                acc = combine(&acc, &literal, &once, limit, false);
                is_top = true;
                rest = post;
                continue;
            }
            return combine(
                &acc,
                &format!("{literal}{post}"),
                &once,
                limit,
                drop_empties,
            );
        }
        if first_group {
            drop_empties = is_top;
            first_group = false;
        }
        let mut parts = comma_parts(body);
        if parts.len() == 1 {
            let embraced: Vec<String> = expand_text(&parts[0], limit, false)
                .iter()
                .map(|item| format!("{{{item}}}"))
                .collect();
            if embraced.len() == 1 {
                acc = combine(
                    &acc,
                    &format!("{pre}{}", embraced[0]),
                    &once,
                    limit,
                    drop_empties && post.is_empty(),
                );
                if post.is_empty() {
                    break;
                }
                rest = post;
                continue;
            }
            parts = embraced;
        }
        let drops_empty_items =
            drop_empties && post.is_empty() && pre.is_empty() && acc.iter().all(String::is_empty);
        let mut values: Vec<String> = Vec::new();
        'parts: for part in &parts {
            for value in expand_text(part, limit, false) {
                if drops_empty_items && value.is_empty() {
                    continue;
                }
                if values.len() >= limit {
                    break 'parts;
                }
                values.push(value);
            }
        }
        acc = combine(&acc, pre, &values, limit, drop_empties && post.is_empty());
        if post.is_empty() {
            break;
        }
        rest = post;
    }
    acc
}

/// Whether a segment of a pattern holds a wildcard, a class, or an extglob
/// (`isDynamicSegment`).
fn is_dynamic_segment(segment: &str) -> bool {
    segment.contains(['*', '?', '[']) || EXTGLOB_RE.find(segment).is_some()
}

/// Whether the static directory prefix of `pattern` (`globBase`: the folders
/// before the first segment with a wildcard) stays inside the change folder,
/// judged by its path alone (`assertPathWithin`): an absolute prefix and a
/// prefix with more `..` than folders leave it.
fn stays_inside_folder(pattern: &str) -> bool {
    let segments: Vec<&str> = pattern.split('/').collect();
    if segments.len() <= 1 {
        return true;
    }
    if pattern.starts_with('/') {
        return false;
    }
    let mut depth = 0usize;
    for segment in &segments[..segments.len() - 1] {
        if segment.is_empty() {
            continue;
        }
        if is_dynamic_segment(segment) {
            break;
        }
        match *segment {
            "." => {}
            ".." => match depth.checked_sub(1) {
                Some(shallower) => depth = shallower,
                None => return false,
            },
            _ => depth += 1,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The patterns of `pattern`, sorted, or the word `refused`.
    fn expanded(pattern: &str) -> Vec<String> {
        let mut patterns = alternatives(pattern).unwrap_or_else(|| vec!["refused".to_string()]);
        patterns.sort();
        patterns
    }

    /// A group expands to one pattern for each of its items, and the groups of
    /// one pattern combine.
    #[test]
    fn the_groups_of_a_pattern_combine() {
        assert_eq!(expanded("w/{a,b}.md"), ["w/a.md", "w/b.md"]);
        assert_eq!(
            expanded("{a,b}/{c,d}.md"),
            ["a/c.md", "a/d.md", "b/c.md", "b/d.md"]
        );
        assert_eq!(expanded("{a,{b,c}}.md"), ["a.md", "b.md", "c.md"]);
        assert_eq!(expanded("{a,b{c,d}}.md"), ["a.md", "bc.md", "bd.md"]);
    }

    /// The outermost group goes first and its nested groups expand in the same
    /// pass: a nest of `depth` groups is `depth + 1` patterns, within the bound.
    #[test]
    fn a_nest_gives_one_pattern_for_each_item() {
        let nest = |depth: usize| format!("{}b{}.md", "{a,".repeat(depth), "}".repeat(depth));

        for depth in [11, 12, 16] {
            let patterns = alternatives(&nest(depth)).expect("a nest within the limits");
            assert_eq!(patterns.len(), depth + 1, "depth {depth}");
        }
    }

    /// A group with no comma stays as it is, braces included, and so does a
    /// range, which spec 3.3 keeps literal; a group in it expands.
    #[test]
    fn a_group_with_no_comma_keeps_its_braces() {
        assert_eq!(expanded("{draft}*.md"), ["{draft}*.md"]);
        assert_eq!(expanded("{x}{a,b}.md"), ["{x}a.md", "{x}b.md"]);
        assert_eq!(expanded("task{1..3}.md"), ["task{1..3}.md"]);
        assert_eq!(expanded("{p,{q{a,b}}}.md"), ["p.md", "{qa}.md", "{qb}.md"]);
        assert_eq!(expanded("{a,b${c,d}}.md"), ["a.md", "b$c.md", "b$d.md"]);
        assert_eq!(expanded("{p,{x}{a,b}}.md"), ["p.md", "{x}a.md", "{x}b.md"]);
    }

    /// A class hides nothing: the group of `[{,}]` has no item, and a pattern
    /// with no expansion is the pattern itself.
    #[test]
    fn a_class_hides_no_group() {
        assert_eq!(expanded("{[a,b],c}.md"), ["[a.md", "b].md", "c.md"]);
        assert_eq!(expanded("[{,}].md"), ["[{,}].md"]);
        assert_eq!(expanded("[ab]{c,d}.md"), ["[ab]c.md", "[ab]d.md"]);
    }

    /// An empty item of a group stays out of the result: OpenSpec expands the
    /// group alone, and `brace-expansion` drops an empty result of a string at
    /// its top level, so the text around the group does not keep it.
    #[test]
    fn an_empty_item_of_a_group_stays_out() {
        assert_eq!(expanded("{a,}.md"), ["a.md"]);
        assert_eq!(expanded("x{a,}.md"), ["xa.md"]);
    }

    /// OpenSpec refuses an expansion past 1,024 patterns, braces nested past 16
    /// levels, and an alternative whose folders leave the change folder; each
    /// gives `None` here.
    #[test]
    fn the_refusals_of_openspec_give_no_patterns() {
        let nest = |depth: usize| format!("{}b{}.md", "{a,".repeat(depth), "}".repeat(depth));

        assert_eq!(
            alternatives(&format!("{}.md", "{a,b}".repeat(10))).map(|p| p.len()),
            Some(1_024)
        );
        assert_eq!(alternatives(&format!("{}.md", "{a,b}".repeat(11))), None);
        assert!(alternatives(&nest(16)).is_some());
        assert_eq!(alternatives(&nest(17)), None);
        assert_eq!(alternatives("{..,w}/t.md"), None);
        assert_eq!(alternatives("a/../../{b,c}.md"), None);
        assert_eq!(alternatives("/{b,c}.md"), None);
        assert_eq!(alternatives("{b,c}\0.md"), None);
        assert_eq!(expanded("{v,w}/t.md"), ["v/t.md", "w/t.md"]);
        assert_eq!(expanded("a/../{b,c}.md"), ["a/../b.md", "a/../c.md"]);
    }

    /// A class and a run in quotes hide their braces from the nesting count; a
    /// parenthesis counts as a level.
    #[test]
    fn the_nesting_count_reads_classes_quotes_and_parentheses() {
        let braces = "{".repeat(17);

        assert!(nests_too_deep(&braces));
        assert!(!nests_too_deep(&format!("[{braces}]")));
        assert!(!nests_too_deep(&format!("\"{braces}\"")));
        assert!(nests_too_deep(&"(".repeat(17)));
        assert!(!nests_too_deep(&format!(
            "{}{}",
            "{(".repeat(8),
            ")}".repeat(8)
        )));
    }
}
