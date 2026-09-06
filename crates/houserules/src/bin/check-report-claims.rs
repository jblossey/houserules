//! Cross-checks one deliverable report's claims against the artifacts and
//! git history it cites -- `tools/check-report-claims.mjs`, ported to a
//! `src/bin/` target in this crate (batch 18 T1, HR-061,
//! docs/specs/2026-09-05-batch-18-phase3.md §7). Dev tooling: not part of
//! the flat command surface `crate::main` dispatches, not shipped in
//! `template/` or the payload. A single-file bin, not a `src/bin/<name>/`
//! directory or a separate workspace member: the checker needs none of
//! `crate::rules`' or `crate::backlog`'s modules (only `serde_json`, `std`,
//! and a `git` subprocess for the same plumbing every other command in
//! this crate already shells out to), so a second workspace member would
//! buy nothing but a second `Cargo.toml` to keep pinned.
//!
//! Born from batch 17 task 1 (HR-059): four fix rounds in a row each
//! closed on one `process.claims-match-artifacts` finding, and each
//! finding was a narrative sentence describing a field the same round had
//! just changed -- a hand re-check missed it every time, including once in
//! the sentence describing this tool's own coverage. See that task's
//! `task-1-report.json` (`fix_rounds[0..3]`) for the case history this
//! tool exists to stop repeating.
//!
//! Limits (every doc comment below that says "see the module doc's
//! limits" means this list): this tool narrows deliberately, in favour of
//! a low false-positive rate over completeness, and a description of it
//! should name what it does not catch, not only what it does.
//! - Only four fields are scanned for narrative claims (`collect_narrative`):
//!   `implemented`, `self_review[]`, and `fix_rounds[].findings[].finding`/
//!   `.fix`. `docs_verified`, `concerns`, and an issue's `file`/`why` are
//!   not -- widening the set is future work, not a limit of the mechanism
//!   itself.
//! - `check_self_audit_narrative` fires only on a `"<N>/<M> deterministic"`
//!   ratio, not on the word `self_audit` (an earlier version keyed on that
//!   word and flagged nine lines of this report's own legitimate history --
//!   a fixture's unrelated `self_audit` field, a past round's sha with no
//!   ratio nearby). A sentence naming a stale head with no ratio nearby is
//!   not caught.
//! - The sha-token pattern skips a token with no `a`-`f` letter: an
//!   all-digit run is far more likely a byte or line count than a short
//!   sha (real, but rare).
//! - No check here can tell a sentence *asserting* a fact from one
//!   *quoting* a past mistake -- both contain the same stale sha and
//!   ratio. `fix_rounds[3]`'s own finding text works around this by
//!   spelling quoted historical counts as words ("nine of nine"), which
//!   the digit-based checks do not parse.
//! - `check_truncation_markers` verifies the excerpt is a byte-prefix of
//!   the named file and the remaining-line count is exact; it never
//!   checks that the file is really that *other* command's output, only
//!   that the bytes match. A marker citing the right file with the wrong
//!   command label in the `run.command` field passes clean (batch 17 task
//!   1's own round-2 `tdd[1].green` mislabel, restored as a probe against
//!   this tool during its review, is exactly this shape).
//! - Any captured run output this tool scans (`tdd[].red`/`.green`,
//!   `tests[]`, `live_run[]`, `fix_rounds[].tests[]`) that embeds a nested
//!   test's own failure text can itself contain a string shaped like a
//!   truncation marker, purely as quoted fixture data -- this tool cannot
//!   tell a marker asserting a real truncation from one quoted inside
//!   another test's panic output. Two instances so far, same mechanism,
//!   different field: this task's own `tdd[].red` for the truncation-marker
//!   mutation (the captured panic prints the fixture string
//!   `[... 99 more lines; full run: full.txt ...]` verbatim inside its
//!   `right: [...]` array, and `full.txt` resolves to nothing at repo
//!   root) and batch 18 T1 fix round 1's own `fix_rounds[].tests[]` RED for
//!   the u64-overflow test (the captured panic prints the fixture string
//!   `[... 99999999999999999999999 more lines; full run: full.txt ...]`
//!   the same way) -- real, disclosed false positives, surfaced by this
//!   tool's own production use, not ones it can rule out.
//! - A second, distinct vehicle for the same shape of false positive: this
//!   tool's own diagnostic line quotes the marker text verbatim, so once
//!   the implementer template's closing act pastes that line into a
//!   scanned run field (typically `tests[]`, the final entry), the next
//!   run finds the same marker inside the checker's own prior output and
//!   flags it again. Unlike the panic-output vehicle above, the quoted
//!   text is this tool's own finding, not fixture data borrowed from
//!   another test. This converges to a fixed point rather than growing
//!   without bound -- verified by running the checker twice against a
//!   report already carrying the self-flagged line and diffing the two
//!   runs byte for byte -- because the quoted marker text does not change
//!   between runs. One instance so far: batch 18 T1's own `tests[10]`,
//!   which quotes `fix_rounds[0].tests[0]`'s finding line from the vehicle
//!   above.
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
//!
//! Port notes (batch 18 T1, disclosed divergences from the frozen JS --
//! this tool carries no frozen-corpus parity contract, unlike the flat
//! command surface):
//! - `check_self_audit_narrative`'s search window around a stale sha
//!   counts UTF-8 bytes, not the JS original's UTF-16 code units. The two
//!   agree everywhere the ported test suite looks; only a report whose
//!   narrative packs many multi-byte characters within 100 units of a
//!   ratio would see the window's edge move, and no ported test
//!   constructs that case.
//! - `resolve_against` joins a relative path onto `root` with
//!   `Path::join`, whose absolute-argument-replaces behaviour matches
//!   Node's `path.resolve(root, relPath)` for both the plain-relative and
//!   absolute-target cases the ported suite exercises (verified live by
//!   that suite), but it does not collapse a `.`/`..` segment the way
//!   `path.resolve` does; no ported test names a path containing one.
//! - A usage error (no report path given) is a named stderr line and exit
//!   code 2, this crate's own CLI-failure-path convention
//!   (`houserules.crash-paths-are-named`), not JS's thrown `UsageError`.
//!   A `git` failure (not a repository, `git` missing) is likewise a
//!   named error rather than an uncaught crash.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use regress::Regex;
use serde_json::Value;

/// One deliverable field shaped `{ command, output, exit? }` this tool can
/// inspect -- `collect_runs`'s own doc names every place the schema allows
/// one.
struct Run {
    command: String,
    output: String,
}

/// Pushes `value` onto `runs` under `label` when it is an object carrying
/// string `command` and `output` fields -- anything else (missing,
/// malformed, or a `run` the schema allows to omit `output`) is silently
/// skipped, matching `tools/check-report-claims.mjs`'s own tolerant reads.
fn push_run(runs: &mut Vec<(String, Run)>, label: String, value: Option<&Value>) {
    let Some(value) = value else { return };
    let Some(command) = value.get("command").and_then(Value::as_str) else {
        return;
    };
    let Some(output) = value.get("output").and_then(Value::as_str) else {
        return;
    };
    runs.push((
        label,
        Run {
            command: command.to_string(),
            output: output.to_string(),
        },
    ));
}

/// Every deliverable field shaped `{ command, output, exit? }` (the `run`
/// definition in `.claude/schemas/deliverables.json`) this tool can
/// inspect, each paired with a JSON-path-ish label for its findings.
/// Covers `tests`, `live_run`, each `tdd[].red`/`.green`, and each
/// `fix_rounds[].tests[]` -- every place the schema allows a captured
/// command's output.
fn collect_runs(report: &Value) -> Vec<(String, Run)> {
    let mut runs = Vec::new();
    if let Some(tests) = report.get("tests").and_then(Value::as_array) {
        for (i, run) in tests.iter().enumerate() {
            push_run(&mut runs, format!("tests[{i}]"), Some(run));
        }
    }
    if let Some(live_run) = report.get("live_run").and_then(Value::as_array) {
        for (i, run) in live_run.iter().enumerate() {
            push_run(&mut runs, format!("live_run[{i}]"), Some(run));
        }
    }
    if let Some(tdd) = report.get("tdd").and_then(Value::as_array) {
        for (i, cycle) in tdd.iter().enumerate() {
            push_run(&mut runs, format!("tdd[{i}].red"), cycle.get("red"));
            push_run(&mut runs, format!("tdd[{i}].green"), cycle.get("green"));
        }
    }
    if let Some(rounds) = report.get("fix_rounds").and_then(Value::as_array) {
        for (i, round) in rounds.iter().enumerate() {
            if let Some(tests) = round.get("tests").and_then(Value::as_array) {
                for (j, run) in tests.iter().enumerate() {
                    push_run(&mut runs, format!("fix_rounds[{i}].tests[{j}]"), Some(run));
                }
            }
        }
    }
    runs
}

/// Every free-text field a `process.claims-match-artifacts` review reads
/// as prose making checkable claims, not structured data: `implemented`,
/// each `self_review[]` entry, and each
/// `fix_rounds[].findings[].finding`/`.fix`. `concerns`, `docs_verified`,
/// and issue/finding `file`/`why` fields are left out -- not because they
/// cannot carry a claim, but because the three failures this tool was
/// built from were all in these four shapes; widening the set is future
/// work, named in the module doc's limits.
fn collect_narrative(report: &Value) -> Vec<(String, String)> {
    let mut narrative = Vec::new();
    if let Some(text) = report.get("implemented").and_then(Value::as_str) {
        narrative.push(("implemented".to_string(), text.to_string()));
    }
    if let Some(items) = report.get("self_review").and_then(Value::as_array) {
        for (i, item) in items.iter().enumerate() {
            if let Some(text) = item.as_str() {
                narrative.push((format!("self_review[{i}]"), text.to_string()));
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

/// The absolute form of `rel_path` against `root` -- `Path::join`, whose
/// absolute-argument-replaces behaviour matches Node's
/// `path.resolve(root, relPath)` for both cases the ported suite
/// exercises (see the module doc's port notes for the one case it does
/// not: `.`/`..` collapse).
fn resolve_against(root: &Path, rel_path: &str) -> PathBuf {
    root.join(rel_path)
}

/// Reads `path` as UTF-8, replacing an invalid byte sequence with the
/// replacement character instead of failing -- Node's
/// `readFileSync(path, 'utf8')` never throws on invalid UTF-8 either; only
/// a missing or unreadable file is an error here.
fn read_utf8_lossy(path: &Path) -> std::io::Result<String> {
    std::fs::read(path).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Checks every `collect_runs` entry whose `command` ends in a `> <path>
/// 2>&1` redirect: `output` must be byte-identical to the named file's
/// current content, resolved against `root`. A command that redirects to
/// a file is asserting "this is what that file holds" -- the strongest,
/// most literal claim a `run` entry can make, so this check accepts no
/// near-match, unlike the truncation-marker check below.
fn check_redirected_captures(root: &Path, runs: &[(String, Run)], errors: &mut Vec<String>) {
    let redirect = Regex::new(r">\s*(\S+)\s+2>&1\s*$").expect("valid redirect-capture pattern");
    for (label, run) in runs {
        let Some(found) = redirect.find(&run.command) else {
            continue;
        };
        let Some(group) = found.group(1) else {
            continue;
        };
        let rel_path = &run.command[group];
        let target = resolve_against(root, rel_path);
        match read_utf8_lossy(&target) {
            Ok(content) => {
                if run.output != content {
                    errors.push(format!(
                        "{label}: output does not byte-match the file its own command redirects to ({rel_path})"
                    ));
                }
            }
            Err(error) => errors.push(format!(
                "{label}: command redirects to \"{rel_path}\", which could not be read ({error})"
            )),
        }
    }
}

/// The line count `full_text` would report under `tools/kb.mjs`'s own
/// convention: splitting on `\n` and dropping one trailing empty segment
/// when the text ends with a newline, so a trailing `\n` never counts as
/// an extra blank line.
fn line_count(full_text: &str) -> usize {
    let lines: Vec<&str> = full_text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.len() - 1
    } else {
        lines.len()
    }
}

/// Checks every `collect_runs` entry whose `output` carries a truncation
/// marker (`[... N more lines; full run: <path> ...]`, the shape
/// `process.evidence-outlives-the-session` asks for): the text before the
/// marker must be an exact byte-prefix of the named file, and `N` must
/// exactly equal that file's remaining line count.
fn check_truncation_markers(root: &Path, runs: &[(String, Run)], errors: &mut Vec<String>) {
    let marker = Regex::new(r"\[\.\.\. (\d+) more lines?; full run: (\S+?) \.\.\.\]")
        .expect("valid truncation-marker pattern");
    for (label, run) in runs {
        let Some(found) = marker.find(&run.output) else {
            continue;
        };
        let marker_text = found.as_str(&run.output).to_string();
        let claimed_group = found.group(1).expect("group 1 always captures on a match");
        let path_group = found.group(2).expect("group 2 always captures on a match");
        let claimed_str = &run.output[claimed_group];
        let rel_path = run.output[path_group].to_string();
        let excerpt = &run.output[..found.start()];
        let claimed_remaining: u64 = match claimed_str.parse() {
            Ok(value) => value,
            Err(_) => {
                errors.push(format!(
                    "{label}: marker \"{marker_text}\" claims {claimed_str} more lines, which does not fit a u64"
                ));
                continue;
            }
        };
        let target = resolve_against(root, &rel_path);
        match read_utf8_lossy(&target) {
            Ok(full_text) => {
                if !full_text.starts_with(excerpt) {
                    errors.push(format!(
                        "{label}: the excerpt before its marker is not a byte-prefix of {rel_path}"
                    ));
                    continue;
                }
                let full_line_count = line_count(&full_text) as u64;
                let excerpt_line_count = excerpt.matches('\n').count() as u64;
                let real_remaining = full_line_count.saturating_sub(excerpt_line_count);
                if real_remaining != claimed_remaining {
                    errors.push(format!(
                        "{label}: marker \"{marker_text}\" claims {claimed_str} more lines; {rel_path} actually has {real_remaining} more"
                    ));
                }
            }
            Err(error) => errors.push(format!(
                "{label}: marker names \"{rel_path}\", which could not be read ({error})"
            )),
        }
    }
}

/// Every commit sha the report itself names: `commits[].sha` plus every
/// `fix_rounds[].commits[].sha`.
fn collect_listed_commit_shas(report: &Value) -> Vec<String> {
    let mut shas = Vec::new();
    let mut collect = |commits: &Value| {
        if let Some(commits) = commits.as_array() {
            for commit in commits {
                if let Some(sha) = commit.get("sha").and_then(Value::as_str) {
                    shas.push(sha.to_string());
                }
            }
        }
    };
    if let Some(commits) = report.get("commits") {
        collect(commits);
    }
    if let Some(rounds) = report.get("fix_rounds").and_then(Value::as_array) {
        for round in rounds {
            if let Some(commits) = round.get("commits") {
                collect(commits);
            }
        }
    }
    shas
}

/// `true` when `sha` resolves to a real commit reachable in `root`'s
/// object database.
fn resolves_to_commit(root: &Path, sha: &str) -> bool {
    Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{sha}^{{commit}}"),
        ])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// `true` when `ancestor` is an ancestor of (or equal to) `descendant` in
/// `root`.
fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// The one sha among `shas` that every other sha is an ancestor of, or
/// `None` when no such total order exists (an empty list, or shas from
/// unrelated history). `shas` are expected to form one straight line --
/// this task's own commit-by-commit history -- so this is the list's tip,
/// found by testing each candidate against every other rather than
/// assuming list order. Every element of `shas` must already resolve to a
/// real commit: one that does not makes `is_ancestor` return `false` for
/// every pairing it appears in, so no candidate can ever satisfy its own
/// `all()` check and this returns `None` for the whole list -- the same
/// `None` an honest unrelated-history list produces.
/// `check_self_audit_head_is_current`, this function's only caller,
/// resolves that ambiguity itself before calling in (see its own doc).
fn newest_listed_commit(root: &Path, shas: &[String]) -> Option<String> {
    for candidate in shas {
        if shas
            .iter()
            .all(|other| other == candidate || is_ancestor(root, other, candidate))
        {
            return Some(candidate.clone());
        }
    }
    None
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
fn check_self_audit_head_is_current(root: &Path, report: &Value, errors: &mut Vec<String>) {
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
    if head != newest {
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
/// sha; does not catch a real, resolvable sha used to describe the wrong
/// thing (see `check_self_audit_narrative` for the one shape of that this
/// tool does check, and the module doc for the rest).
fn check_narrative_shas_resolve(
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
/// co-located sha -- see the module doc's port notes for why this counts
/// UTF-8 bytes, not the JS original's UTF-16 code units.
const RATIO_SHA_WINDOW: usize = 100;

/// A `"<N>/<M> deterministic"` ratio anywhere in narrative prose is a
/// specific, low-noise signal that the sentence is asserting a fact about
/// the report's own self-audit block (unlike the bare word `self_audit`,
/// which also names unrelated things; see the module doc's limits). Two
/// things about that ratio must hold: it must equal `pass`/`deterministic`,
/// and any commit sha within `RATIO_SHA_WINDOW` of it must be `base` or
/// `head`. This is the check the tool exists for: all three of batch 17
/// task 1's fix-round findings were exactly this shape -- a sentence
/// naming a stale head next to a stale pass count for the self-audit
/// sitting elsewhere in the file. Narrower than it could be, on purpose: a
/// sentence naming only a stale head, with no ratio nearby, is not caught
/// (also a module-doc limit).
fn check_self_audit_narrative(
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

/// Reads and parses `path` as a JSON report, naming the file in any read
/// or parse error -- `tools/lib/json-store.mjs`'s `readJson`, collapsed to
/// one named-error shape (`houserules.crash-paths-are-named`): JS raises a
/// plain `Error` for a missing file and a `UsageError` for invalid JSON,
/// kept apart only so its `main` can choose an exit code, but this
/// binary's one caller (`run`) treats both the same way, exit 2.
fn load_report(path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Runs every check above against the report at `report_path` and returns
/// its errors, each a one-line, self-contained description naming the
/// field and the mismatch. An empty vec is a clean run: every capture,
/// marker, and narrative claim this tool knows how to check matched its
/// artifact. `Err` only when `report_path` could not be read as JSON.
fn check_report_claims(report_path: &Path, root: &Path) -> Result<Vec<String>, String> {
    let report = load_report(report_path)?;
    let runs = collect_runs(&report);
    let narrative = collect_narrative(&report);
    let mut errors = Vec::new();
    check_redirected_captures(root, &runs, &mut errors);
    check_truncation_markers(root, &runs, &mut errors);
    check_self_audit_head_is_current(root, &report, &mut errors);
    check_narrative_shas_resolve(root, &narrative, &mut errors);
    check_self_audit_narrative(&narrative, &report, &mut errors);
    Ok(errors)
}

/// Resolves the enclosing git repository's top level from `cwd` -- `git
/// rev-parse --show-toplevel`, `tools/lib/json-store.mjs`'s `repoRoot`. A
/// failure (not inside a repository, or `git` itself missing) is a named
/// error, never an uncaught crash (`houserules.crash-paths-are-named`).
fn repo_root(cwd: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("could not run git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "not inside a git repository: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

/// Parses `args`, runs the checks, and writes the result to `out`/`err` --
/// `tools/check-report-claims.mjs`'s `main(argv, io, cwd)`. Exit code 0
/// (clean), 1 (mismatches found; one line per mismatch on `err`), or 2 (no
/// report path given, or `report_path` could not be resolved or read --
/// see the module doc's port notes for why this is a named line rather
/// than JS's thrown `UsageError`).
fn run(args: &[String], cwd: &Path, out: &mut impl Write, err: &mut impl Write) -> u8 {
    let Some(report_arg) = args.first() else {
        let _ = writeln!(err, "usage: check-report-claims <report.json>");
        return 2;
    };
    let root = match repo_root(cwd) {
        Ok(root) => root,
        Err(message) => {
            let _ = writeln!(err, "{message}");
            return 2;
        }
    };
    let report_path = cwd.join(report_arg);
    match check_report_claims(&report_path, &root) {
        Ok(errors) if errors.is_empty() => {
            let _ = writeln!(out, "{report_arg}: no claim mismatches found");
            0
        }
        Ok(errors) => {
            for error in &errors {
                let _ = writeln!(err, "{error}");
            }
            1
        }
        Err(message) => {
            let _ = writeln!(err, "{message}");
            2
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("could not read the current directory: {error}");
            return ExitCode::from(2);
        }
    };
    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let code = run(&args, &cwd, &mut stdout.lock(), &mut stderr.lock());
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use serde_json::{Value, json};

    use super::*;

    // ---- fixture builders, ported from tests/check-report-claims.test.mjs's own module-level helpers ----

    /// `tests/check-report-claims.test.mjs`'s `initScratchRepo`: a fresh
    /// git repo under a scratch dir, with one commit so `HEAD` resolves.
    fn init_scratch_repo(prefix: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        let head = commit(dir.path(), "seed");
        (dir, head)
    }

    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Commits everything staged (there is nothing to stage; every test
    /// repo here carries no tracked files) as an empty commit and returns
    /// its short sha.
    fn commit(root: &Path, message: &str) -> String {
        git(
            root,
            &[
                "-c",
                "user.email=test@test.invalid",
                "-c",
                "user.name=Test",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                message,
            ],
        );
        git(root, &["rev-parse", "--short", "HEAD"])
            .trim()
            .to_string()
    }

    fn write_json(path: &Path, value: &Value) {
        std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    /// `tests/check-report-claims.test.mjs`'s `baseReport`: a minimal,
    /// otherwise-clean task-report shape naming `head` as the one and only
    /// commit in both `self_audit` and `commits`, so `head` is trivially
    /// the report's own newest listed commit.
    fn base_report(head: &str) -> Value {
        json!({
            "kind": "task-report",
            "self_audit": {
                "summary": {"base": head, "head": head, "deterministic": 3, "pass": 3, "fail": 0, "warn": 0, "skipped": 0, "judged": 0},
                "rows": [],
            },
            "implemented": "nothing checkable here",
            "commits": [{"sha": head, "subject": "seed"}],
            "self_review": [],
            "tests": [],
            "live_run": [],
            "tdd": [],
            "fix_rounds": [],
        })
    }

    /// Mapping 1/18: "passes a report with no captures, markers, or narrative claims".
    #[test]
    fn passes_a_report_with_no_captures_markers_or_narrative_claims() {
        let (dir, head) = init_scratch_repo("check-report-claims-clean-");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &base_report(&head));
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Mapping 2/18: "flags a redirected capture whose output does not byte-match its own target file".
    #[test]
    fn flags_a_redirected_capture_whose_output_does_not_byte_match_its_own_target_file() {
        let (dir, head) = init_scratch_repo("check-report-claims-redirect-");
        std::fs::write(dir.path().join("capture.txt"), "real content\n").unwrap();
        let mut report = base_report(&head);
        report["live_run"] =
            json!([{"command": "echo hi > capture.txt 2>&1", "output": "stale content\n"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec!["live_run[0]: output does not byte-match the file its own command redirects to (capture.txt)".to_string()]
        );
    }

    /// Mapping 3/18: "passes a redirected capture whose output byte-matches its target file".
    #[test]
    fn passes_a_redirected_capture_whose_output_byte_matches_its_target_file() {
        let (dir, head) = init_scratch_repo("check-report-claims-redirect-ok-");
        std::fs::write(dir.path().join("capture.txt"), "real content\n").unwrap();
        let mut report = base_report(&head);
        report["live_run"] =
            json!([{"command": "echo hi > capture.txt 2>&1", "output": "real content\n"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Mapping 4/18: "resolves an absolute-path redirect target as-is, not joined onto root".
    #[test]
    fn resolves_an_absolute_path_redirect_target_as_is_not_joined_onto_root() {
        let (dir, head) = init_scratch_repo("check-report-claims-absolute-redirect-");
        let outside = tempfile::Builder::new()
            .prefix("check-report-claims-absolute-target-")
            .tempdir()
            .expect("tempdir");
        let absolute_path = outside.path().join("capture.txt");
        std::fs::write(&absolute_path, "real content\n").unwrap();
        let mut report = base_report(&head);
        report["live_run"] = json!([{"command": format!("echo hi > {} 2>&1", absolute_path.display()), "output": "real content\n"}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        // root.join(absolute_path) would append the absolute path onto root instead of
        // using it as-is, so this only passes once the check resolves it correctly.
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Mapping 5/18: "flags a truncation marker whose claimed remaining-line count is wrong".
    #[test]
    fn flags_a_truncation_marker_whose_claimed_remaining_line_count_is_wrong() {
        let (dir, head) = init_scratch_repo("check-report-claims-marker-");
        std::fs::write(dir.path().join("full.txt"), "one\ntwo\nthree\nfour\n").unwrap();
        let mut report = base_report(&head);
        report["tdd"] = json!([{
            "test": "x", "mode": "natural",
            "red": {"command": "x", "output": "one\n[... 99 more lines; full run: full.txt ...]\n"},
            "green": {"command": "x", "output": "one\ntwo\nthree\nfour\n"},
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec!["tdd[0].red: marker \"[... 99 more lines; full run: full.txt ...]\" claims 99 more lines; full.txt actually has 3 more".to_string()]
        );
    }

    /// Mapping 6/18: "passes a truncation marker with the correct remaining-line count".
    #[test]
    fn passes_a_truncation_marker_with_the_correct_remaining_line_count() {
        let (dir, head) = init_scratch_repo("check-report-claims-marker-ok-");
        std::fs::write(dir.path().join("full.txt"), "one\ntwo\nthree\nfour\n").unwrap();
        let mut report = base_report(&head);
        report["tdd"] = json!([{
            "test": "x", "mode": "natural",
            "red": {"command": "x", "output": "one\n[... 3 more lines; full run: full.txt ...]\n"},
            "green": {"command": "x", "output": "one\ntwo\nthree\nfour\n"},
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Not a port: batch 18 T1 fix round 1 finding 3
    /// (`houserules.crash-paths-are-named`). A marker's claimed count can
    /// exceed `u64::MAX` even though it matches `\d+`; this is its own
    /// named error, never a silent `u64::MAX` default that then reports a
    /// fabricated mismatch against the real remaining-line count.
    #[test]
    fn flags_a_truncation_marker_whose_claimed_count_does_not_fit_a_u64() {
        let (dir, head) = init_scratch_repo("check-report-claims-marker-overflow-");
        std::fs::write(dir.path().join("full.txt"), "one\ntwo\nthree\nfour\n").unwrap();
        let mut report = base_report(&head);
        report["tdd"] = json!([{
            "test": "x", "mode": "natural",
            "red": {"command": "x", "output": "one\n[... 99999999999999999999999 more lines; full run: full.txt ...]\n"},
            "green": {"command": "x", "output": "one\ntwo\nthree\nfour\n"},
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec!["tdd[0].red: marker \"[... 99999999999999999999999 more lines; full run: full.txt ...]\" claims 99999999999999999999999 more lines, which does not fit a u64".to_string()]
        );
    }

    /// Not a port: batch 18 T1 fix round 1 finding 3
    /// (`process.claims-match-artifacts`). The mismatch message quotes the
    /// marker's own captured digits, not a re-parsed-and-reformatted
    /// count: a leading-zero claim like "007" must read "007" in the
    /// error text, not silently become "7".
    #[test]
    fn uses_the_markers_own_digits_in_a_mismatch_message_not_a_reparsed_count() {
        let (dir, head) = init_scratch_repo("check-report-claims-marker-leading-zero-");
        std::fs::write(dir.path().join("full.txt"), "one\ntwo\nthree\nfour\n").unwrap();
        let mut report = base_report(&head);
        report["tdd"] = json!([{
            "test": "x", "mode": "natural",
            "red": {"command": "x", "output": "one\n[... 007 more lines; full run: full.txt ...]\n"},
            "green": {"command": "x", "output": "one\ntwo\nthree\nfour\n"},
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec!["tdd[0].red: marker \"[... 007 more lines; full run: full.txt ...]\" claims 007 more lines; full.txt actually has 3 more".to_string()]
        );
    }

    /// Mapping 7/18: "flags self_audit.summary.head when it is not the report's own newest listed commit".
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

    /// Mapping 8/18: "passes when self_audit.summary.head is behind live HEAD but is still the report's own newest listed commit".
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

    /// Mapping 9/18: "flags a listed commit sha that does not resolve, instead of silently skipping the head check".
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

    /// Mapping 10/18: "does not check self_audit.summary.head when self_audit is still null".
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

    /// Mapping 11/18: "flags a narrative sha that does not resolve to a real commit".
    #[test]
    fn flags_a_narrative_sha_that_does_not_resolve_to_a_real_commit() {
        let (dir, head) = init_scratch_repo("check-report-claims-bad-sha-");
        let mut report = base_report(&head);
        report["self_review"] = json!(["see commit deadbeef1 for context"]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0]: \"deadbeef1\" looks like a commit sha but does not resolve to one"
                    .to_string()
            ]
        );
    }

    /// Mapping 12/18: "does not flag an all-digit token even though it matches the sha shape".
    #[test]
    fn does_not_flag_an_all_digit_token_even_though_it_matches_the_sha_shape() {
        let (dir, head) = init_scratch_repo("check-report-claims-digit-token-");
        let mut report = base_report(&head);
        report["self_review"] = json!(["the capture is 1234567 bytes"]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Mapping 13/18: "flags a deterministic-pass ratio sitting next to a real sha that is neither base nor head".
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
        report["self_review"] = json!([format!(
            "refreshed from an earlier run at head {prior_head}, 3/3 deterministic pass"
        )]);
        let report_path = root.join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, root).expect("report loads");
        assert_eq!(
            errors,
            vec![format!(
                "self_review[0]: \"3/3 deterministic\" sits within 100 characters of sha \"{prior_head}\", which is neither self_audit.summary.base (\"{current_head}\") nor .head (\"{current_head}\")"
            )]
        );
    }

    /// Mapping 14/18: "flags a self_audit-describing sentence with the wrong deterministic pass ratio".
    #[test]
    fn flags_a_self_audit_describing_sentence_with_the_wrong_deterministic_pass_ratio() {
        let (dir, head) = init_scratch_repo("check-report-claims-stale-ratio-");
        let mut report = base_report(&head);
        report["self_review"] = json!(["self_audit shows 2/2 deterministic pass, 0 fail"]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec!["self_review[0]: \"2/2 deterministic\" does not match self_audit.summary (pass 3, deterministic 3)".to_string()]
        );
    }

    /// Not a port: `quality.absence-is-designed`, surfaced by this task's
    /// own knowledge lookup, has no JS counterpart (the frozen original
    /// interpolates a bare `undefined` here). A report whose
    /// `self_audit.summary` omits `pass`/`deterministic` renders the gap
    /// as a named token, never a raw absence.
    #[test]
    fn renders_a_missing_self_audit_field_as_a_named_token_not_a_raw_gap() {
        let (dir, head) = init_scratch_repo("check-report-claims-missing-summary-field-");
        let mut report = base_report(&head);
        let summary = report["self_audit"]["summary"].as_object_mut().unwrap();
        summary.remove("pass");
        summary.remove("deterministic");
        report["self_review"] = json!(["self_audit shows 3/3 deterministic pass, 0 fail"]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0]: \"3/3 deterministic\" does not match self_audit.summary (pass (missing), deterministic (missing))".to_string()
            ]
        );
    }

    /// Mapping 15/18: "passes a self_audit-describing sentence naming the real head and the real ratio".
    #[test]
    fn passes_a_self_audit_describing_sentence_naming_the_real_head_and_the_real_ratio() {
        let (dir, head) = init_scratch_repo("check-report-claims-narrative-ok-");
        let mut report = base_report(&head);
        report["self_review"] = json!([format!(
            "self_audit at head {head} shows 3/3 deterministic pass, 0 fail"
        )]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Mapping 16/18: "main: exits 0 and reports no mismatches for a clean report".
    #[test]
    fn main_exits_0_and_reports_no_mismatches_for_a_clean_report() {
        let (dir, head) = init_scratch_repo("check-report-claims-main-clean-");
        write_json(&dir.path().join("report.json"), &base_report(&head));
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(&["report.json".to_string()], dir.path(), &mut out, &mut err);
        assert_eq!(code, 0);
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "report.json: no claim mismatches found\n"
        );
    }

    /// Mapping 17/18: "main: exits 1 and prints one line per mismatch for a broken report".
    #[test]
    fn main_exits_1_and_prints_one_line_per_mismatch_for_a_broken_report() {
        let (dir, head) = init_scratch_repo("check-report-claims-main-broken-");
        let mut report = base_report(&head);
        report["self_review"] = json!(["see commit deadbeef1 for context"]);
        write_json(&dir.path().join("report.json"), &report);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(&["report.json".to_string()], dir.path(), &mut out, &mut err);
        assert_eq!(code, 1);
        assert!(String::from_utf8(err).unwrap().contains("deadbeef1"));
    }

    /// Mapping 18/18 (reconstructed shape): "main: throws a UsageError when no report path is given"
    /// becomes "reports usage on stderr and exits 2" -- this binary's own
    /// CLI-failure-path convention (`houserules.crash-paths-are-named`),
    /// not JS's thrown `UsageError` (see the module doc's port notes).
    #[test]
    fn main_reports_usage_and_exits_2_when_no_report_path_is_given() {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = run(&[], Path::new("."), &mut out, &mut err);
        assert_eq!(code, 2);
        assert_eq!(
            String::from_utf8(err).unwrap(),
            "usage: check-report-claims <report.json>\n"
        );
    }
}
