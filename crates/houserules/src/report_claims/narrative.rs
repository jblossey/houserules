//! The narrative checks of `check-report-claims`. They read the prose of a
//! report (`collect_narrative`) for a stale head, a stale pass count, and an
//! unresolvable sha. The ephemeral-path check reads every string of the report
//! (`collect_report_strings`) for a path that does not outlive the session. The
//! parent module doc states the stance of every Limits list.
//!
//! Limits (every doc comment of this file that says "see the module doc's
//! limits" means this list):
//! - Only four fields are scanned for narrative claims (`collect_narrative`):
//!   `implemented`, `self_review[].claim`, and
//!   `fix_rounds[].findings[].finding`/`.fix`. `docs_verified`, `concerns`, and
//!   an issue's `file`/`why` are not -- widening the set is future work, not a
//!   limit of the mechanism itself. A `self_review` item that is no object with
//!   a string `claim` is skipped (`validate` refuses it), so a report of the old
//!   form, whose items are strings, gets no narrative check for them.
//! - `check_self_audit_narrative` fires only on a `"<N>/<M> deterministic"`
//!   ratio, not on the bare word `self_audit`, which also names unrelated
//!   things. A sentence naming a stale head with no ratio nearby is not
//!   caught.
//! - The sha-token pattern skips a token with no `a`-`f` letter: an
//!   all-digit run is far more likely a byte or line count than a short
//!   sha (real, but rare).
//! - No check here can tell a sentence *asserting* a fact from one
//!   *quoting* a past mistake -- both contain the same stale sha and
//!   ratio. Spelling a quoted historical count as words ("nine of nine")
//!   avoids a false flag, since the digit-based checks do not parse it.
//! - `check_self_audit_head_is_current` compares `self_audit.summary.head`
//!   against the newest commit the report itself lists in `commits[]` and
//!   `fix_rounds[].commits[]` (see that function's own doc for why, and
//!   what it replaced) -- not against live HEAD. It trusts that list: a
//!   report that quietly omits one of its own real commits is not caught.
//! - Both `check_self_audit_head_is_current` and
//!   `check_narrative_shas_resolve` assume every sha a report names stays
//!   resolvable, which holds only while the branch is live:
//!   `houserules.pinned-shas-live-on-mains-ancestry` records that this
//!   repository's fast-forward merges discard every pre-aggregation branch
//!   sha, unreachable from any ref afterward. Run this tool while the
//!   branch that produced the report is still live -- after aggregation,
//!   every commit sha the report names (including one a sentence itself
//!   marks as replaced, like a pre-review amend) reads as unresolvable,
//!   and this tool cannot yet tell that shape of "correct but aged"
//!   report from a genuinely broken one.
//! - `check_narrative_shas_resolve` and `check_self_audit_narrative` both
//!   assume every sha-shaped token in narrative prose names a commit in
//!   THIS repository's own object database; neither has, or can safely
//!   gain, a notion of a citation whose repository is named and foreign
//!   (batch 25 T1: a sha from the read-only tag-pilot checkout,
//!   `houserules.tag-pilot-is-read-only`, correctly does not resolve
//!   here). A syntax this tool recognised and then skipped verifying
//!   (`tag-pilot@<sha>`, say) would not actually check the foreign sha
//!   against anything -- tag-pilot's checkout is outside this repository
//!   and this tool has no business reading it even if a path to it were
//!   known -- so such a form would let a typo'd or fabricated foreign sha
//!   pass exactly as clean as a real one, which is worse than the current
//!   plain false positive. The safe answer is procedural, not a checker
//!   change: keep a foreign-repository commit sha out of the four scanned
//!   narrative fields (a `source` field, or any field `collect_narrative`
//!   does not read, carries it without tripping either check) -- T1's own
//!   fix already did exactly this.
//! - `check_ephemeral_paths` flags an absolute `/tmp/`-rooted path or a
//!   `scratchpad/`-named directory segment in every string of the report
//!   (`collect_report_strings`) -- never inside a `command` (the literal
//!   command, where `/tmp` is the required text of what ran: a live-run
//!   scratch repository lives there by design, `houserules.live-run-recipe`), an
//!   `excerpt` (output data), the `anchor` of a citation (text copied from a
//!   cited file), a `sha256`, or the `self_audit` (tool output), which this
//!   check has no business judging. The exclusions go by key at any depth, so a
//!   future field named `command`, `excerpt`, or `sha256` is excluded too; the
//!   `anchor` is excluded only inside `citations[]`, and the `self_audit` only
//!   at the top of the report. `/tmp/` requires the word it roots to start
//!   with `/` (`word_qualifies`'s own doc has the boundary account): an
//!   unanchored substring match would otherwise flag a durable, tracked
//!   path like `tests/tmp/golden.json` or a URL's own path segment.
//!   `scratchpad/` carries no such requirement, since every real citation
//!   of that shape is the bare directory name, with or without a leading
//!   `/` or an elided `.../` prefix. Both shapes together are still blind
//!   to a macOS `$TMPDIR` (typically `/var/folders/.../T/`), a Windows
//!   `%TEMP%`, or any other platform's ephemeral home with no
//!   `scratchpad/` segment in it, and to a durable-LOOKING
//!   repository-relative path that is in fact untracked or never
//!   committed: a report cannot be checked against a git object that was
//!   never added. A bare mention of the WORD "scratchpad" with no
//!   `scratchpad/`-segmented path attached is deliberately not this
//!   check's business either. The `scratchpad/` arm carries no
//!   absolute-path requirement: it flags any word containing a
//!   `scratchpad/` segment, durable or not, so a real, tracked,
//!   repository-relative path naming one (e.g. `docs/scratchpad/notes.md`)
//!   over-flags the same way an unanchored `/tmp/` match would. A glued
//!   NON-punctuation prefix is also invisible to both shapes:
//!   `NARRATIVE_LEADING_PUNCTUATION` trims neither `=` nor `:`, so a
//!   word like `OUT=/tmp/x` or `dest:/tmp/x` -- an env-var assignment or a
//!   labeled value quoted in prose -- keeps that prefix after the trim, no
//!   longer starts with `/`, and `word_qualifies` rejects it the same way
//!   an unanchored bare path would have passed unflagged.
//! - A narrative sentence that names an ephemeral path's CLASS (a scratch
//!   file, a `/tmp` capture) without embedding the literal, and cites the
//!   literal only inside a retained workspace capture, is not a checker
//!   escape: the literal never reaches a scanned field, so
//!   `check_ephemeral_paths` has nothing to flag there, and nothing needs
//!   flagging -- the durable evidence lives in the retained capture, not
//!   the narrative.
//!
//! Further constraints:
//! - `check_self_audit_narrative`'s search window around a stale sha
//!   counts UTF-8 bytes. Only a report whose narrative packs many
//!   multi-byte characters within 100 units of a ratio would see the
//!   window's edge move; not observed in practice.

use std::path::Path;

use regress::Regex;
use serde_json::Value;

use super::{
    NARRATIVE_LEADING_PUNCTUATION, NARRATIVE_TRAILING_PUNCTUATION, collect_listed_commit_shas,
    newest_listed_commit, resolve_commit, word_end, word_start,
};

/// Every free-text field a `process.claims-match-artifacts` review reads
/// as prose making checkable claims, not structured data: `implemented`,
/// the `claim` of each `self_review[]` item, and each
/// `fix_rounds[].findings[].finding`/`.fix`. Each is labelled with its JSON
/// path (`self_review[2].claim`). `concerns`, `docs_verified`, and
/// issue/finding `file`/`why` fields are left out -- not because they
/// cannot carry a claim, but because narrowing the scope keeps the
/// false-positive rate low; widening the set is future work, named in the
/// module doc's limits.
pub(super) fn collect_narrative(report: &Value) -> Vec<(String, String)> {
    let mut narrative = Vec::new();
    if let Some(text) = report.get("implemented").and_then(Value::as_str) {
        narrative.push(("implemented".to_string(), text.to_string()));
    }
    if let Some(items) = report.get("self_review").and_then(Value::as_array) {
        for (i, item) in items.iter().enumerate() {
            if let Some(text) = item.get("claim").and_then(Value::as_str) {
                narrative.push((format!("self_review[{i}].claim"), text.to_string()));
            }
        }
    }
    if let Some(rounds) = report.get("fix_rounds").and_then(Value::as_array) {
        for (i, round) in rounds.iter().enumerate() {
            let Some(findings) = round.get("findings").and_then(Value::as_array) else {
                continue;
            };
            for (j, finding) in findings.iter().enumerate() {
                if let Some(text) = finding.get("finding").and_then(Value::as_str) {
                    narrative.push((
                        format!("fix_rounds[{i}].findings[{j}].finding"),
                        text.to_string(),
                    ));
                }
                if let Some(text) = finding.get("fix").and_then(Value::as_str) {
                    narrative.push((
                        format!("fix_rounds[{i}].findings[{j}].fix"),
                        text.to_string(),
                    ));
                }
            }
        }
    }
    narrative
}

/// `true` when `sha` resolves to a real commit reachable in `root`'s
/// object database.
fn resolves_to_commit(root: &Path, sha: &str) -> bool {
    resolve_commit(root, sha).is_some()
}

/// `self_audit.summary.head` must equal the newest commit the report
/// itself lists (`commits[]` plus every `fix_rounds[].commits[]`), not
/// `root`'s live `git rev-parse HEAD`. `process.deliverables-json` scopes
/// `self_audit` to `BASE..HEAD` at report time: a finished report's head
/// is that task's final commit forever, whatever the branch tip becomes
/// afterward. Skipped (not an error) when `self_audit` is still `null` (a
/// legitimate in-flight state) or the report lists no commit at all
/// (nothing to compare against).
///
/// Every listed sha must resolve before `newest_listed_commit` runs at
/// all: a sha `is_ancestor` cannot resolve makes EVERY candidate fail its
/// own `all()` check (it can satisfy neither "ancestor of" nor "descendant
/// of" any real sha), so `newest_listed_commit` returns `None` for the
/// whole list -- indistinguishable, without this guard, from the genuine
/// unrelated-history case that same `None` also signals. A resolution
/// failure here is reported as its own named error instead, and the
/// silent skip is reserved for the case `newest_listed_commit` alone
/// still can't resolve: every sha real, but forming no single line.
pub(super) fn check_self_audit_head_is_current(
    root: &Path,
    report: &Value,
    errors: &mut Vec<String>,
) {
    let Some(head) = report
        .get("self_audit")
        .and_then(|self_audit| self_audit.get("summary"))
        .and_then(|summary| summary.get("head"))
        .and_then(Value::as_str)
    else {
        return;
    };
    let listed = collect_listed_commit_shas(report);
    if listed.is_empty() {
        return;
    }
    let mut unique: Vec<String> = Vec::new();
    for sha in &listed {
        if !unique.contains(sha) {
            unique.push(sha.clone());
        }
    }
    let unresolved: Vec<&String> = unique
        .iter()
        .filter(|sha| !resolves_to_commit(root, sha))
        .collect();
    if !unresolved.is_empty() {
        for sha in unresolved {
            errors.push(format!(
                "a commit this report lists, \"{sha}\", does not resolve to a commit -- self_audit.summary.head cannot be checked against it"
            ));
        }
        return;
    }
    let Some(newest) = newest_listed_commit(root, &unique) else {
        return; // every listed sha is real; they just form no single line
    };
    // `head` and `newest` can each be short or full: `houserules audit`'s own
    // printed `summary.head` is always the short, git-log-style abbreviation, even
    // when `--head` was given in full, while `commits[].sha` names whatever length
    // the report happened to record. Both are already known to resolve (`head` is
    // checked here; every `unique` sha, `newest` included, was checked above), so
    // comparing resolved identity rather than the raw strings treats two different
    // lengths of the same commit as equal, as they are.
    let Some(head_resolved) = resolve_commit(root, head) else {
        errors.push(format!(
            "self_audit.summary.head \"{head}\" does not resolve to a commit"
        ));
        return;
    };
    let newest_resolved = resolve_commit(root, &newest)
        .expect("newest_listed_commit only returns a sha this function already verified resolves");
    if head_resolved != newest_resolved {
        errors.push(format!(
            "self_audit.summary.head is \"{head}\", but the report's own newest listed commit is \"{newest}\" -- self_audit is stale"
        ));
    }
}

/// `true` when `token` contains at least one `a`-`f` letter -- see the
/// module doc's limits for why an all-digit token is exempt.
fn looks_like_a_sha(token: &str) -> bool {
    token
        .chars()
        .any(|c| c.is_ascii_hexdigit() && !c.is_ascii_digit())
}

/// Every commit-shaped token in narrative prose must resolve to a real
/// commit in `root`'s object database. Catches a typo or a fabricated
/// sha; does not catch a real, resolvable sha that names the wrong thing
/// (see `check_self_audit_narrative` for the one shape of that this tool
/// does check, and the module doc for the rest).
pub(super) fn check_narrative_shas_resolve(
    root: &Path,
    narrative: &[(String, String)],
    errors: &mut Vec<String>,
) {
    let sha_token = Regex::new(r"\b[0-9a-f]{7,40}\b").expect("valid sha-token pattern");
    for (label, text) in narrative {
        let mut seen: Vec<&str> = Vec::new();
        for found in sha_token.find_iter(text) {
            let token = found.as_str(text);
            if !looks_like_a_sha(token) || seen.contains(&token) {
                continue;
            }
            seen.push(token);
            if !resolves_to_commit(root, token) {
                errors.push(format!(
                    "{label}: \"{token}\" looks like a commit sha but does not resolve to one"
                ));
            }
        }
    }
}

/// Characters either side of a deterministic-ratio match searched for a
/// co-located sha -- counted in UTF-8 bytes (see the module doc's Further
/// constraints).
const RATIO_SHA_WINDOW: usize = 100;

/// A `"<N>/<M> deterministic"` ratio anywhere in narrative prose is a
/// specific, low-noise signal that the sentence is asserting a fact about
/// the report's own self-audit block (unlike the bare word `self_audit`,
/// which also names unrelated things; see the module doc's limits). Two
/// things about that ratio must hold: it must equal `pass`/`deterministic`,
/// and any commit sha within `RATIO_SHA_WINDOW` of it must be `base` or
/// `head`. This is the check the tool primarily exists for: a sentence
/// naming a stale head next to a stale pass count for the self-audit
/// sitting elsewhere in the file. Narrower than it could be, on purpose: a
/// sentence naming only a stale head, with no ratio nearby, is not caught
/// (also a module-doc limit).
pub(super) fn check_self_audit_narrative(
    narrative: &[(String, String)],
    report: &Value,
    errors: &mut Vec<String>,
) {
    let Some(summary) = report
        .get("self_audit")
        .and_then(|self_audit| self_audit.get("summary"))
    else {
        return;
    };
    // `quality.absence-is-designed`: a report whose self_audit.summary omits a
    // required field is malformed, not a legitimate absence -- but this checker
    // reads tolerantly throughout, so a missing field renders as a named token
    // rather than an empty string a reader could mistake for a real, blank value.
    let base = summary
        .get("base")
        .and_then(Value::as_str)
        .unwrap_or("(missing)");
    let head = summary
        .get("head")
        .and_then(Value::as_str)
        .unwrap_or("(missing)");
    let pass = summary.get("pass").and_then(Value::as_i64);
    let deterministic = summary.get("deterministic").and_then(Value::as_i64);
    let ratio = Regex::with_flags(r"(\d+)\s*/\s*(\d+)\s+deterministic", "i")
        .expect("valid deterministic-ratio pattern");
    let sha_token = Regex::new(r"\b[0-9a-f]{7,40}\b").expect("valid sha-token pattern");
    for (label, text) in narrative {
        for found in ratio.find_iter(text) {
            let ratio_text = found.as_str(text);
            let matched_pass: i64 = text
                [found.group(1).expect("group 1 always captures on a match")]
            .parse()
            .unwrap_or(-1);
            let matched_deterministic: i64 = text
                [found.group(2).expect("group 2 always captures on a match")]
            .parse()
            .unwrap_or(-1);
            if Some(matched_pass) != pass || Some(matched_deterministic) != deterministic {
                errors.push(format!(
                    "{label}: \"{ratio_text}\" does not match self_audit.summary (pass {}, deterministic {})",
                    pass.map_or_else(|| "(missing)".to_string(), |v| v.to_string()),
                    deterministic.map_or_else(|| "(missing)".to_string(), |v| v.to_string()),
                ));
                continue;
            }
            let range = found.range();
            let start = text.floor_char_boundary(range.start.saturating_sub(RATIO_SHA_WINDOW));
            let end = text.ceil_char_boundary((range.end + RATIO_SHA_WINDOW).min(text.len()));
            let window = &text[start..end];
            let mut seen: Vec<&str> = Vec::new();
            for found in sha_token.find_iter(window) {
                let token = found.as_str(window);
                if !looks_like_a_sha(token) || seen.contains(&token) {
                    continue;
                }
                seen.push(token);
                if token != base && token != head {
                    errors.push(format!(
                        "{label}: \"{ratio_text}\" sits within {RATIO_SHA_WINDOW} characters of sha \"{token}\", which is neither self_audit.summary.base (\"{base}\") nor .head (\"{head}\")"
                    ));
                }
            }
        }
    }
}

/// Whether `path`, the JSON path of an object, is an entry of `citations`
/// (`citations[3]`).
fn is_citation_entry(path: &str) -> bool {
    path.strip_prefix("citations[")
        .and_then(|rest| rest.strip_suffix(']'))
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Whether the ephemeral-path check leaves out the value at `key` of the object
/// at `path` (see the module doc's Limits).
fn is_unread_key(path: &str, key: &str) -> bool {
    match key {
        "command" | "excerpt" | "sha256" => true,
        "anchor" => is_citation_entry(path),
        "self_audit" => path.is_empty(),
        _ => false,
    }
}

/// Collects the strings below `value`, which sits at `path`, into `strings`.
fn walk_strings(value: &Value, path: &str, strings: &mut Vec<(String, String)>) {
    match value {
        Value::String(text) => strings.push((path.to_string(), text.clone())),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                walk_strings(item, &format!("{path}[{index}]"), strings);
            }
        }
        Value::Object(fields) => {
            for (key, field) in fields {
                if is_unread_key(path, key) {
                    continue;
                }
                let field_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk_strings(field, &field_path, strings);
            }
        }
        _ => {}
    }
}

/// Every string of the report that `check_ephemeral_paths` reads, in the order
/// of the report, each paired with its JSON path (`docs_verified[0].source`)
/// as its label. It leaves out the values of the keys `command`, `excerpt`, and
/// `sha256`, the `anchor` of a `citations[]` entry, and the top-level
/// `self_audit` (see the module doc's Limits).
pub(super) fn collect_report_strings(report: &Value) -> Vec<(String, String)> {
    let mut strings = Vec::new();
    walk_strings(report, "", &mut strings);
    strings
}

/// The absolute-path prefix `check_ephemeral_paths` treats as ephemeral: it
/// does not survive past the session that wrote it
/// (`process.evidence-outlives-the-session`). Matched only when it roots an
/// absolute path (`word_qualifies`'s own doc has the boundary account) --
/// see the module doc's Limits for what this prefix still cannot see.
const EPHEMERAL_PATH_PREFIX: &str = "/tmp/";

/// The directory-segment shape this project's own session-scratchpad home
/// always ends in, matched wherever it appears in a word with no
/// absolute-path requirement -- see the module doc's Limits.
const EPHEMERAL_SCRATCHPAD_SEGMENT: &str = "scratchpad/";

/// `true` when `word` (already trimmed of leading/trailing punctuation) is
/// the kind of token `trigger` may legitimately root. `/tmp/` counts only
/// when `word` is itself an absolute path (starts with `/`) -- otherwise
/// the trigger sits inside a relative path (`tests/tmp/golden.json`) or a
/// URL's own path segment (`https://example.test/tmp/report.html`),
/// neither an ephemeral filesystem location; `/var/tmp/capture.txt` still
/// qualifies, rooted at its own leading `/`, not at the `/tmp/` substring
/// partway through it. `scratchpad/` carries no such requirement: every
/// real citation of that shape (module doc's Limits) is the bare
/// directory name, with or without a leading `/` or an elided `.../`
/// prefix, so requiring an absolute-path start would exclude the shape
/// this arm exists to catch.
fn word_qualifies(trigger: &str, word: &str) -> bool {
    trigger != EPHEMERAL_PATH_PREFIX || word.starts_with('/')
}

/// Every word in `text` naming `trigger`, once per unique trimmed word,
/// pushed onto `errors` under `label` -- the shared walk
/// `check_ephemeral_paths` runs once per trigger shape. `seen` is threaded
/// across every call for one `(label, text)` pair so a word matching both
/// triggers (an absolute path with a `scratchpad/` segment) is not flagged
/// twice.
fn flag_ephemeral_words<'a>(
    text: &'a str,
    trigger: &str,
    seen: &mut Vec<&'a str>,
    label: &str,
    errors: &mut Vec<String>,
) {
    let mut search_from = 0;
    while let Some(relative) = text[search_from..].find(trigger) {
        let match_start = search_from + relative;
        let start = word_start(text, match_start);
        let end = word_end(text, match_start);
        let raw = &text[start..end];
        search_from = end;
        let path = raw
            .trim_start_matches(NARRATIVE_LEADING_PUNCTUATION)
            .trim_end_matches(NARRATIVE_TRAILING_PUNCTUATION);
        if path.len() <= trigger.len() || !word_qualifies(trigger, path) || seen.contains(&path) {
            continue;
        }
        seen.push(path);
        errors.push(format!(
            "{label}: cites \"{path}\", an ephemeral path that will not exist once the session ends"
        ));
    }
}

/// Every ephemeral path token `strings` (`collect_report_strings`) cite -- a
/// `/tmp/`-rooted absolute path or a `scratchpad/`-named directory segment --
/// once per unique path per string: a report that names an artifact there
/// describes a location that will not exist by the time anyone re-opens
/// the report. See the module doc's Limits for the strings it never reads, and
/// what these two shapes still cannot see.
pub(super) fn check_ephemeral_paths(strings: &[(String, String)], errors: &mut Vec<String>) {
    for (label, text) in strings {
        let mut seen: Vec<&str> = Vec::new();
        flag_ephemeral_words(text, EPHEMERAL_PATH_PREFIX, &mut seen, label, errors);
        flag_ephemeral_words(text, EPHEMERAL_SCRATCHPAD_SEGMENT, &mut seen, label, errors);
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

    /// Flags self_audit.summary.head when it is not the report's own newest listed commit.
    #[test]
    fn flags_self_audit_head_when_it_is_not_the_reports_own_newest_listed_commit() {
        let (dir, first_commit) = init_scratch_repo("check-report-claims-stale-head-");
        let second_commit = commit(dir.path(), "second");
        let mut report = base_report(&first_commit); // self_audit.head names the OLDER of the two
        report["commits"] = json!([{"sha": first_commit, "subject": "first"}, {"sha": second_commit, "subject": "second"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![format!(
                "self_audit.summary.head is \"{first_commit}\", but the report's own newest listed commit is \"{second_commit}\" -- self_audit is stale"
            )]
        );
    }

    /// Passes when self_audit.summary.head is behind live HEAD but is still the report's own newest listed commit.
    #[test]
    fn passes_when_self_audit_head_is_behind_live_head_but_is_still_the_reports_own_newest_listed_commit()
     {
        let (dir, head) = init_scratch_repo("check-report-claims-behind-head-ok-");
        let report = base_report(&head); // base_report already lists `head` as its one commit
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        // A later, unrelated commit -- simulating the next task landing on the same branch.
        // self_audit must still describe this report's own final commit, not the branch tip.
        commit(dir.path(), "later");
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Passes when self_audit.summary.head is an abbreviated form of the report's own
    /// newest listed commit, not a byte-identical string. `houserules audit`'s own
    /// printed `summary.head`/`summary.base` are always the short, git-log-style
    /// abbreviation regardless of whether `--head` was given short or full, so a
    /// verbatim-pasted self_audit routinely names the same commit in a shorter form
    /// than `commits[].sha`'s full sha.
    #[test]
    fn passes_when_self_audit_head_is_an_abbreviated_form_of_the_reports_own_newest_listed_commit()
    {
        let (dir, short_head) = init_scratch_repo("check-report-claims-abbreviated-head-");
        let full_head = git(dir.path(), &["rev-parse", "HEAD"]).trim().to_string();
        let mut report = base_report(&short_head);
        report["commits"] = json!([{"sha": full_head, "subject": "seed"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Flags a listed commit sha that does not resolve, instead of silently skipping the head check.
    #[test]
    fn flags_a_listed_commit_sha_that_does_not_resolve_instead_of_silently_skipping_the_head_check()
    {
        let (dir, head) = init_scratch_repo("check-report-claims-unresolvable-listed-sha-");
        let mut report = base_report(&head);
        // A bogus entry alongside the real, resolvable one: newest_listed_commit's own
        // all() can satisfy neither "ancestor of" nor "descendant of" for it, so every
        // candidate (including the real head) fails, and the unguarded function would
        // return None for the whole list -- the same None an honest unrelated-history
        // report also produces. A stale head next to this same bogus sha must now be
        // reported, not swallowed.
        report["self_audit"]["summary"]["head"] = json!("deadbee1"); // deliberately stale too
        report["commits"] =
            json!([{"sha": head, "subject": "seed"}, {"sha": "deadbee2", "subject": "bogus"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "a commit this report lists, \"deadbee2\", does not resolve to a commit -- self_audit.summary.head cannot be checked against it".to_string()
            ]
        );
    }

    /// Does not check self_audit.summary.head when self_audit is still null.
    #[test]
    fn does_not_check_self_audit_head_when_self_audit_is_still_null() {
        let (dir, head) = init_scratch_repo("check-report-claims-null-audit-");
        let mut report = base_report(&head);
        report["self_audit"] = Value::Null;
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Flags a narrative sha that does not resolve to a real commit.
    #[test]
    fn flags_a_narrative_sha_that_does_not_resolve_to_a_real_commit() {
        let (dir, head) = init_scratch_repo("check-report-claims-bad-sha-");
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim": "see commit deadbeef1 for context"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0].claim: \"deadbeef1\" looks like a commit sha but does not resolve to one"
                    .to_string()
            ]
        );
    }

    /// Does not flag an all-digit token even though it matches the sha shape.
    #[test]
    fn does_not_flag_an_all_digit_token_even_though_it_matches_the_sha_shape() {
        let (dir, head) = init_scratch_repo("check-report-claims-digit-token-");
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim": "the capture is 1234567 bytes"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Flags a deterministic-pass ratio sitting next to a real sha that is neither base nor head.
    #[test]
    fn flags_a_deterministic_pass_ratio_sitting_next_to_a_real_sha_that_is_neither_base_nor_head() {
        let (dir, _seed_head) = init_scratch_repo("check-report-claims-stale-narrative-sha-");
        let root = dir.path();
        let short_head = || {
            git(root, &["rev-parse", "--short", "HEAD"])
                .trim()
                .to_string()
        };
        // prior_head must contain an a-f letter: the narrative check applies the same
        // digit-only guard check_narrative_shas_resolve does (a bare number is far more
        // likely a count than a sha), and a random short sha is all-digit about 1 run
        // in 28 (16 values, 10 digits: (10/16)^7). Retry commits until one qualifies,
        // rather than accept that flakiness.
        let mut prior_head = short_head();
        for i in 0..20 {
            if looks_like_a_sha(&prior_head) {
                break;
            }
            commit(root, &format!("retry {i}"));
            prior_head = short_head();
        }
        assert!(looks_like_a_sha(&prior_head));
        // One more, real commit: prior_head is now a genuinely resolvable sha that is
        // neither self_audit.summary.base nor .head, isolating the narrative check
        // from check_narrative_shas_resolve (which prior_head would pass regardless).
        commit(root, "final");
        let current_head = short_head();
        let mut report = base_report(&current_head);
        // The ratio is the check's trigger and matches self_audit exactly (3/3, per
        // base_report); only the co-located sha is wrong, isolating this instance.
        report["self_review"] = json!([{"claim": format!(
            "refreshed from an earlier run at head {prior_head}, 3/3 deterministic pass"
        )}]);
        let report_path = root.join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, root).expect("report loads");
        assert_eq!(
            errors,
            vec![format!(
                "self_review[0].claim: \"3/3 deterministic\" sits within 100 characters of sha \"{prior_head}\", which is neither self_audit.summary.base (\"{current_head}\") nor .head (\"{current_head}\")"
            )]
        );
    }

    /// Flags a self_audit-describing sentence with the wrong deterministic pass ratio.
    #[test]
    fn flags_a_self_audit_describing_sentence_with_the_wrong_deterministic_pass_ratio() {
        let (dir, head) = init_scratch_repo("check-report-claims-stale-ratio-");
        let mut report = base_report(&head);
        report["self_review"] =
            json!([{"claim": "self_audit shows 2/2 deterministic pass, 0 fail"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec!["self_review[0].claim: \"2/2 deterministic\" does not match self_audit.summary (pass 3, deterministic 3)".to_string()]
        );
    }

    /// A report whose `self_audit.summary` omits `pass`/`deterministic`
    /// renders the gap as a named token (`quality.absence-is-designed`),
    /// never a raw absence.
    #[test]
    fn renders_a_missing_self_audit_field_as_a_named_token_not_a_raw_gap() {
        let (dir, head) = init_scratch_repo("check-report-claims-missing-summary-field-");
        let mut report = base_report(&head);
        let summary = report["self_audit"]["summary"].as_object_mut().unwrap();
        summary.remove("pass");
        summary.remove("deterministic");
        report["self_review"] =
            json!([{"claim": "self_audit shows 3/3 deterministic pass, 0 fail"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0].claim: \"3/3 deterministic\" does not match self_audit.summary (pass (missing), deterministic (missing))".to_string()
            ]
        );
    }

    /// Passes a self_audit-describing sentence naming the real head and the real ratio.
    #[test]
    fn passes_a_self_audit_describing_sentence_naming_the_real_head_and_the_real_ratio() {
        let (dir, head) = init_scratch_repo("check-report-claims-narrative-ok-");
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim": format!(
            "self_audit at head {head} shows 3/3 deterministic pass, 0 fail"
        )}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    // ---- the ephemeral-path check ----

    /// A fix-round finding citing a `/tmp`-rooted artifact path.
    #[test]
    fn flags_a_fix_round_finding_citing_a_slash_tmp_artifact_path() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-finding-");
        let mut report = base_report(&head);
        report["fix_rounds"] = json!([{
            "round": 0,
            "findings": [{
                "finding": "CRITICAL 3: the goldens closure's enumeration lived at /tmp/verify-goldens.py, a session scratchpad -- process.evidence-outlives-the-session names only the batch workspace or the tracked tree as citable, and the script could not rerun after the session ended.",
                "file": "t3-evidence/verify-goldens.py",
                "fix": "moved the script into the batch workspace",
            }],
            "commits": [],
            "tests": [],
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "fix_rounds[0].findings[0].finding: cites \"/tmp/verify-goldens.py\", an ephemeral path that will not exist once the session ends".to_string()
            ]
        );
    }

    /// The brief's own central boundary: a `/tmp` path inside a `command`
    /// field is the literal, required text of what ran
    /// (`houserules.live-run-recipe` puts every scratch git repository
    /// under the session scratchpad on purpose) and must never flag, even
    /// though `EPHEMERAL_PATH_PREFIX` would match it if `collect_runs`'
    /// fields were ever in this check's scope.
    #[test]
    fn does_not_flag_a_slash_tmp_path_inside_a_command_field() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-command-");
        let mut report = base_report(&head);
        report["live_run"] = json!([clean_run(
            dir.path(),
            "capture.txt",
            &[
                "sh",
                "-c",
                "cd /tmp/claude-1001/scratch/hr075-live && houserules check-knowledge"
            ],
            "knowledge: ok\n",
        )]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// The false-positive class this check deliberately does not catch
    /// (module doc's Limits): an honest narrative sentence discussing the
    /// CONCEPT of a session scratchpad with no path attached at all.
    #[test]
    fn does_not_flag_narrative_prose_that_only_mentions_scratchpad_with_no_path() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-prose-");
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim":
            "Ran the scratch git repository under the session scratchpad, never touching this repository's working tree."
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// The same path cited twice flags once (dedup), and a trailing comma
    /// or period ending the sentence is trimmed from the quoted path.
    #[test]
    fn flags_each_unique_slash_tmp_path_once_and_trims_trailing_sentence_punctuation() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-dedup-");
        let mut report = base_report(&head);
        report["implemented"] = json!(
            "Left /tmp/hr075-scratch, /tmp/hr075-scratch, in place; also see /tmp/other-artifact."
        );
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "implemented: cites \"/tmp/hr075-scratch\", an ephemeral path that will not exist once the session ends".to_string(),
                "implemented: cites \"/tmp/other-artifact\", an ephemeral path that will not exist once the session ends".to_string(),
            ]
        );
    }

    /// A scratchpad citation with an elided `.../` prefix and no `/tmp/`
    /// text at all still flags on its `scratchpad/` segment.
    #[test]
    fn flags_a_scratchpad_directory_segment_with_an_elided_prefix() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-scratchpad-");
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim":
            "Left the scratch install at .../scratchpad/hr033-live in place; no prior task in this batch reused it."
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0].claim: cites \".../scratchpad/hr033-live\", an ephemeral path that will not exist once the session ends".to_string()
            ]
        );
    }

    /// A parenthesized, no-leading-dots scratchpad citation needs the
    /// leading-punctuation trim in `flag_ephemeral_words`, or the flagged
    /// text would keep the enclosing `(`.
    #[test]
    fn flags_a_parenthesized_scratchpad_citation_trimmed_of_its_parens() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-scratchpad-parens-");
        let mut report = base_report(&head);
        report["self_review"] = json!([{"claim":
            "Fixed the live-run script (scratchpad/task2-scratch-audit.mjs) to mkdtempSync under the session scratchpad."
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0].claim: cites \"scratchpad/task2-scratch-audit.mjs\", an ephemeral path that will not exist once the session ends".to_string()
            ]
        );
    }

    /// A durable, tracked, repository-relative path must not flag merely
    /// for containing a `/tmp/` segment.
    #[test]
    fn does_not_flag_a_durable_relative_path_containing_a_slash_tmp_slash_segment() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-relative-");
        let mut report = base_report(&head);
        report["implemented"] = json!("The fixture at tests/tmp/golden.json is tracked.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A URL whose own path carries a `/tmp/` segment is not a local
    /// ephemeral path.
    #[test]
    fn does_not_flag_a_url_path_segment_containing_slash_tmp_slash() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-url-");
        let mut report = base_report(&head);
        report["implemented"] =
            json!("See https://example.test/tmp/report.html for the upstream note.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// `/var/tmp/` is still a real, ephemeral absolute path -- it must
    /// still flag, quoting the WHOLE path from its own leading `/`, not
    /// the bare `/tmp/capture.txt` suffix an unanchored match would
    /// fabricate.
    #[test]
    fn flags_slash_var_slash_tmp_quoting_the_whole_absolute_path() {
        let (dir, head) = init_scratch_repo("check-report-claims-ephemeral-var-tmp-");
        let mut report = base_report(&head);
        report["implemented"] = json!("The capture sits at /var/tmp/capture.txt.");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "implemented: cites \"/var/tmp/capture.txt\", an ephemeral path that will not exist once the session ends".to_string()
            ]
        );
    }

    // ---- the strings of the report and the claims of self_review ----

    /// The walk gives every string with its JSON path, in the order of the
    /// report, and leaves out the values of `command`, `excerpt`, and `sha256`
    /// at any depth, the `anchor` inside `citations[]` only, and the `self_audit`
    /// at the top only.
    #[test]
    fn the_string_walk_labels_by_json_path_and_leaves_out_the_unread_keys() {
        let report = json!({
            "kind": "task-report",
            "tests": [{
                "capture": "a.txt", "sha256": "ff", "command": "ls /tmp/x",
                "excerpt": "e", "exit": 0
            }],
            "citations": [{"path": "p.rs", "anchor": "/tmp/a", "sha256": "00"}],
            "self_audit": {"summary": {"head": "abc"}},
            "docs_verified": [{"api": "x", "source": "s"}],
            "concerns": ["c", 3],
            "fix_rounds": [{"findings": [{"finding": "f", "anchor": "kept"}]}],
            "other": {"self_audit": "kept", "anchor": "kept too", "nested": {"command": "x"}},
        });
        let pairs: Vec<(String, String)> = [
            ("kind", "task-report"),
            ("tests[0].capture", "a.txt"),
            ("citations[0].path", "p.rs"),
            ("docs_verified[0].api", "x"),
            ("docs_verified[0].source", "s"),
            ("concerns[0]", "c"),
            ("fix_rounds[0].findings[0].finding", "f"),
            ("fix_rounds[0].findings[0].anchor", "kept"),
            ("other.self_audit", "kept"),
            ("other.anchor", "kept too"),
        ]
        .into_iter()
        .map(|(label, text)| (label.to_string(), text.to_string()))
        .collect();
        assert_eq!(collect_report_strings(&report), pairs);
    }

    /// Only `citations[<digits>]` is a citation entry.
    #[test]
    fn only_an_indexed_entry_of_citations_is_a_citation_entry() {
        assert!(is_citation_entry("citations[0]"));
        assert!(is_citation_entry("citations[12]"));
        for path in [
            "",
            "citations",
            "citations[]",
            "citations[x]",
            "x.citations[0]",
        ] {
            assert!(!is_citation_entry(path), "{path}");
        }
    }

    /// An item of the old form, a plain string, adds no claim, and an item
    /// without a string `claim` adds none either.
    #[test]
    fn a_self_review_item_without_a_string_claim_adds_no_narrative() {
        let report = json!({"self_review": [
            "an old-form item",
            {"evidence": ["a.txt"]},
            {"claim": 3},
            {"claim": "kept"},
        ]});
        assert_eq!(
            collect_narrative(&report),
            vec![("self_review[3].claim".to_string(), "kept".to_string())]
        );
    }
}
