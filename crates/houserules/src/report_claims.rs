//! The `check-report-claims` flat-surface subcommand: cross-checks one
//! deliverable report's claims against the artifacts and git history it
//! cites. Lives at the crate root, like `emit`, `get`, `install`,
//! `node_path`, and `root`: this module needs none of `crate::rules`' or
//! `crate::backlog`'s modules (only `serde_json`, `std`, and a `git`
//! subprocess for the same plumbing every other command in this crate
//! already shells out to), so nesting it under either would buy nothing.
//!
//! A report's narrative prose can restate a fact that a later edit changed
//! elsewhere in the same report -- a stale head, a stale pass count, an
//! unresolvable sha -- and a hand re-check tends to miss exactly this
//! shape. This tool exists to catch that class mechanically.
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
//! - `check_truncation_markers` verifies the excerpt is a byte-prefix of
//!   the named file and the remaining-line count is exact; it never
//!   checks that the file is really that *other* command's output, only
//!   that the bytes match. A marker citing the right file with the wrong
//!   command label in the `run.command` field passes clean.
//! - Any captured run output this tool scans (`tdd[].red`/`.green`,
//!   `tests[]`, `live_run[]`, `fix_rounds[].tests[]`) that embeds a nested
//!   test's own failure text can itself contain a string shaped like a
//!   truncation marker, purely as quoted fixture data -- this tool cannot
//!   tell a marker asserting a real truncation from one quoted inside
//!   another test's panic output. A real, disclosed false-positive
//!   vehicle, not one this tool can rule out.
//! - A second, distinct vehicle for the same shape of false positive: this
//!   tool's own diagnostic line quotes the marker text verbatim, so once
//!   a report pastes that line into a scanned run field (typically
//!   `tests[]`, the final entry), the next run finds the same marker
//!   inside the checker's own prior output and flags it again. Unlike the
//!   panic-output vehicle above, the quoted text is this tool's own
//!   finding, not fixture data borrowed from another test. This converges
//!   to a fixed point rather than growing without bound, because the
//!   quoted marker text does not change between runs.
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
//!   report from a genuinely broken one. `check_citation_lines` (HR-107)
//!   carries the identical "correct but aged" exposure for a FILE
//!   citation, not only a sha: a report correctly citing a file that
//!   existed when the report was written reads as unresolvable once that
//!   file is later renamed, moved, or deleted. This task's own corpus run
//!   found the batch-20 T3 JS-to-Rust migration is by far the largest
//!   source of this shape (retired `.mjs`/`bin/houserules.mjs` citations
//!   in a pre-migration report), with a same-language in-repo rename a
//!   rarer instance of the identical exposure (a report once correctly
//!   citing `check-report-claims.rs`, since moved to this very file).
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
//! - `find_citations` (so every one of `check_citation_lines`,
//!   `check_narrative_number_claims`, and `check_narrative_sweep_coverage`)
//!   carries the file-citation sibling of the bullet above: a path naming
//!   a file in ANOTHER codebase entirely -- another tool's own source
//!   tree, or a third-party crate's own source read via the package
//!   registry cache while verifying a `docs_verified` entry -- never
//!   resolves against this repository's tracked tree or batch workspace,
//!   and never can. The same reasoning applies: a recognised-but-
//!   unverified "foreign path" syntax would let a typo'd or fabricated
//!   citation to a codebase this tool cannot read pass as clean, which is
//!   worse than the current plain false positive. The safe answer is the
//!   same procedural one -- name the foreign file by prose only, with no
//!   `path:NNN` shape, or keep it out of the four scanned narrative
//!   fields.
//! - `check_paste_run_lint` flags a command field carrying an
//!   angle-bracket placeholder, text appended after the command that a
//!   shell would run as a second, separate command, or quoting that does
//!   not balance. It cannot see a retyped command that still parses
//!   cleanly: a command field carrying two literal backslashes before a
//!   quote (`python3 -c \"import yaml; ...\"`) parses in every real
//!   shell -- differently from what was intended, but without error --
//!   which is exactly the shape no quote-balance check can catch, named
//!   here rather than silently missed.
//! - `check_paste_run_lint`'s appended-text check exempts a parenthesized
//!   subshell wrapping the WHOLE command (`( cmd )`, a real shell idiom)
//!   but not one chained after the command's first token (`cmd1 &&
//!   (cmd2)`, also legitimate shell) -- a real, disclosed false-positive
//!   vehicle rather than one special-cased away.
//! - `check_paste_run_lint`'s placeholder check also fires on command
//!   DATA that only looks like a placeholder, since it applies regardless
//!   of `quote_mask`'s state: a literal `<...>` sitting inside a quoted
//!   string (an HTML-comment probe, an email address) is real,
//!   paste-runnable text, not a substitution point, but the check cannot
//!   tell the two apart -- the lint's most frequent false-positive
//!   vehicle, most often a `git commit -m "... Co-Authored-By: ...
//!   <a@b.com>"` shape this repository's own standing no-coauthor rule
//!   guarantees will keep recurring as literal test data.
//! - `check_paste_run_lint`'s placeholder check reads any `<...>` span
//!   with no whitespace touching either bracket as a placeholder,
//!   excluding only `<(`/`>(` process substitution (`diff <(cmd1)
//!   <(cmd2)`, real shell syntax, never misread). A command legitimately
//!   using bare `< file` stdin redirection immediately followed later on
//!   the same line by a `>` output redirect would still misread as a
//!   placeholder -- theoretical, not yet observed.
//! - `check_ephemeral_paths` flags an absolute `/tmp/`-rooted path or a
//!   `scratchpad/`-named directory segment anywhere in the same four
//!   narrative fields `collect_narrative` already scans -- never inside a
//!   `command` field (`collect_runs`), where `/tmp` is the literal,
//!   required text of what ran (a live-run scratch repository lives there
//!   by design, `houserules.live-run-recipe`) and this check has no
//!   business judging it. `/tmp/` requires the word it roots to start
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
//! - `quote_mask` has no notion of `$(...)` command substitution
//!   resetting quote context: real bash parses a same-character quote
//!   opened again inside a `$(...)` (or `` `...` ``) as starting a fresh,
//!   independent quoted region, not as closing the one around the whole
//!   substitution. This walk does not track that: a command like
//!   `bash -c "$(python3 -c "import yaml; d=yaml.safe_load(open('...'));
//!   ...")"`, valid in real bash, reads the inner `"import yaml...` quote
//!   as closing the outer one, so by the time the walk reaches `open(...)`
//!   it believes itself outside any quote -- a false positive that a
//!   walker kept deliberately below full shell grammar (see Further
//!   constraints below) cannot rule out.
//! - A narrative sentence that names an ephemeral path's CLASS (a scratch
//!   file, a `/tmp` capture) without embedding the literal, and cites the
//!   literal only inside a retained workspace capture, is not a checker
//!   escape: the literal never reaches a scanned field, so
//!   `check_ephemeral_paths` has nothing to flag there, and nothing needs
//!   flagging -- the durable evidence lives in the retained capture, not
//!   the narrative.
//! - `find_citations` (the shared helper `check_citation_lines`,
//!   `check_narrative_number_claims`, and `check_narrative_sweep_coverage`
//!   all build on) only recognises a path ending in one of a small, closed
//!   set of extensions (its own doc names them); a real cited file with
//!   another extension is invisible to all three checks. It also reads
//!   only the FIRST `:NNN` or `:NNN-MMM` suffix on a token: a comma- or
//!   semicolon-separated multi-line citation (`file.rs:23,24,41`, a real
//!   shape in this repository's own review prose) is read for `:23` alone
//!   -- the remaining numbers in the list are not checked. Neither is a
//!   range's ordering (`path:50-10` is read as-is; NNN and MMM are each
//!   checked against the file's line count independently, never against
//!   each other).
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
//!   `root` with the same fs read every other check in this module uses:
//!   the report's own artifacts are expected to be checked from a clean
//!   checkout at HEAD, not a git blob pinned to any particular commit, so
//!   a dirty working tree can shift what "the file's line count" means
//!   between two runs of this tool. It also does not flag a citation of
//!   line `0`, since `0` never exceeds a real file's line count.
//! - `check_narrative_number_claims` (HR-110) implements only the number
//!   class HR-110's item body names ("N passed", "N files"), not the
//!   locative class ("the X live in Y") the same item also describes:
//!   the two batch-23 locative instances that motivated it both sat in a
//!   commit body and a plan file, neither a report narrative field this
//!   tool scans, so a mechanical, low-noise trigger for "this sentence
//!   asserts a location" could not be derived from this repository's own
//!   corpus (`quality.gates-derive-their-scope`); a future widening needs
//!   its own real instances to calibrate against, not this tool's report-
//!   narrative surface. "Citing a capture" is approximated as "naming
//!   EXACTLY ONE `find_citations` path within `NARRATIVE_CLAIM_WINDOW`
//!   bytes of the count phrase" -- a byte window, not a real sentence
//!   boundary, the same proxy `check_self_audit_narrative` already uses
//!   for a ratio and its co-located sha; requiring exactly one, not "at
//!   least one", rules out the enumeration-list shape the task's own
//!   corpus run found repeatedly ("N files (a.rs, b.rs, ...)" naming N as
//!   the list's own length), at the cost of also skipping a genuine
//!   count sitting near two unrelated citations. `contains_number_token`
//!   reads the claimed count as a literal digit string: a capture that
//!   spells the same count in words ("five hundred twenty-eight") or
//!   with a comma thousands separator ("1,234") reads as absent.
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
//!   cover it.
//!
//! Further constraints (this tool carries no frozen-corpus parity
//! contract, unlike the flat command surface, so these are simply its own
//! bytes and its own behaviour, not a divergence from anything else):
//! - `check_self_audit_narrative`'s search window around a stale sha
//!   counts UTF-8 bytes. Only a report whose narrative packs many
//!   multi-byte characters within 100 units of a ratio would see the
//!   window's edge move; not observed in practice.
//! - `resolve_against` joins a relative path onto `root` with
//!   `Path::join`; it does not collapse a `.`/`..` segment in the input
//!   path.
//! - A usage error (no report path given) is clap's own required-argument
//!   message, exit 2. A `git` failure (not a repository, `git` missing)
//!   is likewise a named error rather than an uncaught crash
//!   (`houserules.crash-paths-are-named`).

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
/// skipped.
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
/// cannot carry a claim, but because narrowing the scope keeps the
/// false-positive rate low; widening the set is future work, named in the
/// module doc's limits.
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

/// The absolute form of `rel_path` against `root`: `Path::join`, which
/// does not collapse a `.`/`..` segment in `rel_path` (see the module
/// doc's Further constraints).
fn resolve_against(root: &Path, rel_path: &str) -> PathBuf {
    root.join(rel_path)
}

/// Reads `path` as UTF-8, replacing an invalid byte sequence with the
/// replacement character instead of failing. Only a missing or unreadable
/// file is an error here.
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

/// The line count `full_text` reports: splitting on `\n` and dropping one
/// trailing empty segment when the text ends with a newline, so a
/// trailing `\n` never counts as an extra blank line.
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

/// Resolves `sha` to the full 40-character commit id it names in `root`,
/// or `None` when it does not resolve to a commit at all (unknown,
/// ambiguous, or not a commit).
fn resolve_commit(root: &Path, sha: &str) -> Option<String> {
    let output = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{sha}^{{commit}}"),
        ])
        .current_dir(root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|full| full.trim().to_string())
}

/// `true` when `sha` resolves to a real commit reachable in `root`'s
/// object database.
fn resolves_to_commit(root: &Path, sha: &str) -> bool {
    resolve_commit(root, sha).is_some()
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

/// A reason `quote_mask` could not finish walking a command string to the
/// end -- the "failed shlex-style parse" shape `check_paste_run_lint`
/// flags. Both cases are exactly what a real shell's own
/// word-splitter also refuses: an unterminated quote leaves the shell
/// waiting for more input (`> ` on an interactive prompt), and a trailing
/// backslash with nothing to escape is a syntax error.
enum QuoteParseError {
    /// A `'` or `"` opened by the command is never closed.
    UnterminatedQuote(char),
    /// The command ends in a single, unescaped `\` with no following
    /// character for it to escape.
    TrailingBackslash,
}

impl std::fmt::Display for QuoteParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QuoteParseError::UnterminatedQuote(quote) => {
                write!(f, "an unterminated {quote} quote")
            }
            QuoteParseError::TrailingBackslash => write!(f, "a trailing, unescaped backslash"),
        }
    }
}

/// Walks `command` the way a POSIX shell splits it into words far enough
/// to answer two questions: does the quoting balance, and which byte
/// offsets sit inside an open quote? Single quotes (`'...'`) admit no
/// escapes, matching the POSIX shell grammar's own "a single-quote cannot
/// occur within single-quotes" (IEEE Std 1003.1-2024 §2.2.2). Outside
/// any quote, POSIX has a bare backslash escape exactly
/// the next character; this walk matches that. Inside double quotes,
/// POSIX escapes a backslash only before five characters (`$`, `` ` ``,
/// `"`, `\`, a literal newline); this walk instead escapes ANY following
/// byte the same way, a deliberate over-approximation verified not to
/// change either check `check_paste_run_lint` builds on this mask:
/// treating `\p` as one escaped pair rather than two independent literal
/// bytes never flips whether a quote later balances or whether a given
/// byte reads as "inside a quote" -- it only matters for a backslash
/// immediately before the closing quote itself, which POSIX escapes too
/// (`\"`), so the two readings never diverge in practice. That is the
/// whole grammar this walk implements -- no globs, no
/// `$(...)`/`` `...` `` substitution (see the module doc's limits for the
/// one false positive this omission produces), no parameter expansion --
/// because `check_paste_run_lint`'s two other checks need nothing more
/// than the quote/escape state to run safely.
///
/// A hand-rolled walk, not a crate (`shlex`, `shell-words`, `shellwords`):
/// each implements the full POSIX word-splitting grammar this lint has no
/// use for, and this repository carries no maintained, std-only,
/// no-execution tokenizer (`security-hygiene.dependency-vetting`,
/// `quality.well-maintained-libraries`).
fn quote_mask(command: &str) -> Result<Vec<bool>, QuoteParseError> {
    let bytes = command.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut open_quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        match open_quote {
            Some(quote) if byte == quote => {
                mask[i] = true;
                open_quote = None;
            }
            Some(b'"') if byte == b'\\' && i + 1 < bytes.len() => {
                mask[i] = true;
                mask[i + 1] = true;
                i += 1;
            }
            Some(_) => mask[i] = true,
            None if byte == b'\'' || byte == b'"' => open_quote = Some(byte),
            None if byte == b'\\' => {
                if i + 1 >= bytes.len() {
                    return Err(QuoteParseError::TrailingBackslash);
                }
                mask[i + 1] = true;
                i += 1;
            }
            None => {}
        }
        i += 1;
    }
    match open_quote {
        Some(quote) => Err(QuoteParseError::UnterminatedQuote(quote as char)),
        None => Ok(mask),
    }
}

/// The longest angle-bracket placeholder body this check will read as one
/// -- long enough for every placeholder this project's own reports use
/// (`<REPORT_FILE>`, `<pinned url>`, `<scratch-clone-of-repo>`), short
/// enough that a stray `<` far from any real `>` cannot force a long,
/// pointless scan.
const PLACEHOLDER_SCAN_LIMIT: usize = 80;

/// Finds the first angle-bracket placeholder in `command`: a `<...>` span
/// with no whitespace touching either bracket from the inside (a real
/// placeholder in this project's own history is always packed text,
/// `<scratch-target>`/`<pinned url>`; a bare `< file` or `> file` is a
/// shell redirection, not a placeholder) and no nested `<`/`>` inside.
/// `<(`/`>(` (bash process substitution, seen live in this project's own
/// history: `diff <(git show ...) <(git show ...)`) is excluded up front,
/// since it never has whitespace at that boundary either and process
/// substitution is unambiguous shell syntax, never a placeholder.
/// Returns the placeholder's inner text (without its brackets). Applies
/// regardless of `quote_mask`'s state: a placeholder still needs manual
/// substitution whether or not it sits inside quotes.
fn find_placeholder(command: &str) -> Option<&str> {
    let bytes = command.as_bytes();
    let mut search_from = 0;
    while let Some(relative_open) = command[search_from..].find('<') {
        let open = search_from + relative_open;
        let after = open + 1;
        let starts_boundary = match bytes.get(after) {
            None => true,
            Some(byte) => *byte == b'(' || byte.is_ascii_whitespace(),
        };
        if starts_boundary {
            search_from = after;
            continue;
        }
        let scan_end = bytes.len().min(after + PLACEHOLDER_SCAN_LIMIT);
        let mut close = None;
        for (offset, byte) in bytes[after..scan_end].iter().enumerate() {
            match byte {
                b'>' => {
                    close = Some(after + offset);
                    break;
                }
                b'<' | b'\n' => break,
                _ => {}
            }
        }
        if let Some(close) = close
            && close > after
            && !bytes[close - 1].is_ascii_whitespace()
        {
            return Some(&command[after..close]);
        }
        search_from = after;
    }
    None
}

/// Finds the byte offset of text after `command`'s first token that a
/// shell would run as its own, separate command if the field were pasted:
/// an unquoted `#` comment marker at the start of a word (POSIX begins a
/// comment only there, never mid-word), or an unquoted `(` that opens
/// neither a `$(...)` command substitution nor an escaped `\(` literal
/// (`find`'s own `\( -name a -o -name b \)` idiom) nor process
/// substitution's own `<(`/`>(`. A `(` that is itself the command's first
/// token -- a subshell wrapping the WHOLE command (`( cmd; echo
/// "EXIT=$?" )`) -- is exempt too: only text appended AFTER a command
/// that has already started counts
/// (see the module doc's limits for the one shape of chained, legitimate
/// parenthetical this does not exempt). `mask` (from `quote_mask`)
/// excludes both markers from consideration while inside a quoted string,
/// where they are ordinary text, not shell syntax.
fn find_appended_annotation(command: &str, mask: &[bool]) -> Option<usize> {
    let bytes = command.as_bytes();
    let mut seen_token = false;
    for (i, &byte) in bytes.iter().enumerate() {
        if !mask[i] {
            let word_initial = i == 0 || bytes[i - 1].is_ascii_whitespace();
            match byte {
                // POSIX (IEEE Std 1003.1-2024 §2.3): '#' begins a
                // comment only as the first character of a word, never mid-word
                // (`foo#bar` is one literal token, not `foo` plus a comment).
                b'#' if seen_token && word_initial => return Some(i),
                b'(' if seen_token => {
                    let preceding = if i == 0 { None } else { Some(bytes[i - 1]) };
                    if !matches!(
                        preceding,
                        Some(b'$') | Some(b'\\') | Some(b'<') | Some(b'>')
                    ) {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        if !byte.is_ascii_whitespace() {
            seen_token = true;
        }
    }
    None
}

/// The bounded, no-execution paste-run lint: flags a command field
/// carrying an angle-bracket placeholder, text appended after the command
/// that a shell would run as a second command, or quoting that does not
/// balance (see the module doc's limits for what none of the three can
/// see). A command whose quoting fails to parse
/// skips the other two checks: without a reliable quote mask,
/// `find_appended_annotation` cannot safely tell quoted text from shell
/// syntax, and one finding per broken command is enough.
fn check_paste_run_lint(runs: &[(String, Run)], errors: &mut Vec<String>) {
    for (label, run) in runs {
        let mask = match quote_mask(&run.command) {
            Ok(mask) => mask,
            Err(error) => {
                errors.push(format!(
                    "{label}: command's quoting does not parse ({error}) and cannot paste-run as written"
                ));
                continue;
            }
        };
        if let Some(placeholder) = find_placeholder(&run.command) {
            errors.push(format!(
                "{label}: command carries the placeholder \"<{placeholder}>\" and cannot paste-run as written"
            ));
        }
        if let Some(position) = find_appended_annotation(&run.command, &mask) {
            errors.push(format!(
                "{label}: command has \"{}\" appended after it, which a shell would run as a second, separate command if pasted",
                run.command[position..].trim_end()
            ));
        }
    }
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

/// Leading bytes this project's own narrative prose glues onto a cited
/// token that belong to the surrounding SENTENCE, not the citation itself:
/// an opening bracket/brace/paren, or a quote or backtick opening a
/// markdown code span (`(scratchpad/task2-scratch-audit.mjs)`). Shared by
/// `flag_ephemeral_words` and `find_citations`: both trim a whitespace-
/// delimited word down to the path a reader would actually follow.
const NARRATIVE_LEADING_PUNCTUATION: &[char] = &['(', '[', '{', '\'', '"', '`'];

/// Trailing bytes this project's own narrative prose glues onto a cited
/// token that belong to the surrounding SENTENCE, not the citation itself:
/// a comma or period ending the clause, a closing bracket/paren/brace, a
/// quote, or a backtick closing a markdown code span. Shared by
/// `flag_ephemeral_words` and `find_citations` (see that constant's own
/// doc).
const NARRATIVE_TRAILING_PUNCTUATION: &[char] =
    &[',', ';', ':', ')', ']', '}', '\'', '"', '`', '.'];

/// The byte offset of the end of the maximal non-whitespace run starting at
/// `start` in `text` -- `text.len()` when none is found.
fn word_end(text: &str, start: usize) -> usize {
    text[start..]
        .find(|c: char| c.is_whitespace())
        .map_or(text.len(), |offset| start + offset)
}

/// The byte offset of the start of the maximal non-whitespace run
/// containing `pos` in `text` -- `0` when `pos` sits in the text's first
/// word.
fn word_start(text: &str, pos: usize) -> usize {
    text[..pos]
        .char_indices()
        .rev()
        .find(|&(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8())
}

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

/// Every ephemeral path token `narrative` cites -- a `/tmp/`-rooted
/// absolute path or a `scratchpad/`-named directory segment -- once per
/// unique path per field: a narrative claim naming an artifact there
/// describes a location that will not exist by the time anyone re-opens
/// the report. Never scans a `command` field -- see the module doc's
/// Limits for the full boundary account of why, and what these two
/// shapes still cannot see.
fn check_ephemeral_paths(narrative: &[(String, String)], errors: &mut Vec<String>) {
    for (label, text) in narrative {
        let mut seen: Vec<&str> = Vec::new();
        flag_ephemeral_words(text, EPHEMERAL_PATH_PREFIX, &mut seen, label, errors);
        flag_ephemeral_words(text, EPHEMERAL_SCRATCHPAD_SEGMENT, &mut seen, label, errors);
    }
}

// ---- the shared capture-resolution helper (HR-107/HR-110/HR-111 all build on this) ----

/// File extensions a citation's path must end in -- the small, closed set
/// this repository's own tracked tree and batch workspaces actually use
/// (`rs`, `json`, `md`, `sh`, `toml`, `yml`/`yaml`, `txt`, `py`, `ts`,
/// `js`, `mjs`, `lock`, `log`, `html`). A knowledge or backlog id
/// (`houserules.crash-paths-are-named`, `process.tdd`) has exactly the
/// same `word.word` shape a looser "any short suffix" pattern would also
/// match, and narrative prose cites ids constantly -- this very module
/// doc does, throughout. Anchoring `find_citations` on a real extension
/// instead is what keeps a citation search from flagging every id a
/// report names; see the module doc's Limits for what a new extension
/// needs before this list covers it.
const CITATION_EXTENSION_PATTERN: &str =
    r"\.(?:rs|json|md|sh|toml|ya?ml|txt|py|ts|js|mjs|lock|log|html)\b";

/// A path-shaped citation `find_citations` locates in narrative prose,
/// with the optional `:NNN` or `:NNN-MMM` line range HR-107 checks --
/// `line`/`line_end` are `None` when the token carried no such suffix, the
/// shape HR-110 and HR-111 also resolve (a bare mention of a retained
/// capture or a sweep script, with no line pinned). `range` is the
/// trimmed token's own byte span in the source text, so a caller can test
/// proximity to some other match the way `check_self_audit_narrative`
/// already does for a ratio and a co-located sha.
struct Citation {
    path: String,
    line: Option<u64>,
    line_end: Option<u64>,
    range: std::ops::Range<usize>,
}

/// Splits a trimmed citation token's optional `:NNN`/`:NNN-MMM` suffix off
/// its path, at the FIRST `:` rather than the last: a path itself never
/// carries a colon, so the first one always marks the line suffix's
/// start, and a Rust-panic-style `file:NNN:MM` column citation (a real
/// shape this task's own corpus run found) keeps `NNN` as its line and
/// drops the trailing `:MM` rather than reading `NNN:MM` as YET more path.
/// Falls back to treating the whole token as a bare path (no line fields)
/// when the text right after that `:` has no leading digit at all -- a
/// citation with a genuinely non-numeric trailing colon (none observed in
/// this repository's own narrative prose) is not this tool's business to
/// split further. A range whose second half fails to parse (`path:170-abc`)
/// still keeps the first number: `path:170` alone is already a valid
/// citation this function found by construction.
fn split_line_suffix(token: &str) -> (&str, Option<u64>, Option<u64>) {
    let Some(colon) = token.find(':') else {
        return (token, None, None);
    };
    let path = &token[..colon];
    let suffix = &token[colon + 1..];
    let digit_end = suffix
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(suffix.len());
    if digit_end == 0 {
        return (token, None, None);
    }
    let Ok(line) = suffix[..digit_end].parse::<u64>() else {
        return (token, None, None);
    };
    let line_end = suffix[digit_end..].strip_prefix('-').and_then(|range_end| {
        let range_digit_end = range_end
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(range_end.len());
        (range_digit_end > 0)
            .then(|| range_end[..range_digit_end].parse::<u64>().ok())
            .flatten()
    });
    (path, Some(line), line_end)
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
/// survives that trim. `search_from` only advances, so no two returned
/// citations can name the exact same token twice.
fn find_citations(text: &str) -> Vec<Citation> {
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
        let range = range_start..range_start + trimmed.len();
        let (path, line, line_end) = split_line_suffix(trimmed);
        citations.push(Citation {
            path: path.to_string(),
            line,
            line_end,
            range,
        });
    }
    citations
}

/// One `collect_narrative` field, its citations already parsed: `label`
/// and `text` are exactly `collect_narrative`'s own pair, and `citations`
/// is `find_citations(&text)`, run here exactly once. `check_citation_lines`,
/// `check_narrative_number_claims`, and `check_narrative_sweep_coverage`
/// all read `citations` from this shared pass instead of each calling
/// `find_citations` on the same text again -- fix round 1, minor issue 7:
/// round 0 parsed a report's four narrative fields up to three times each
/// (once per check) and recompiled `CITATION_EXTENSION_PATTERN` every
/// time.
struct NarrativeField {
    label: String,
    text: String,
    citations: Vec<Citation>,
}

/// Runs `find_citations` once per `narrative` field, pairing each with its
/// own `label`/`text` for the three checks built on `NarrativeField` below.
fn collect_narrative_fields(narrative: &[(String, String)]) -> Vec<NarrativeField> {
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

/// Every path `git ls-files` names under `root` -- this repository's own
/// tracked-file authority (`quality.gates-derive-their-scope`), used to
/// resolve a bare-filename citation below. `None` when `root` is not a
/// git working tree, `git` itself is missing, or the command otherwise
/// fails; a citation resolver given `None` degrades to the literal,
/// root-relative join for every citation rather than treating the
/// failure as its own finding -- the same silent-degrade precedent
/// `resolve_commit`/`is_ancestor` above already set for this module's
/// other `git` shell-outs (see the module doc's Further constraints).
fn list_tracked_files(root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        output
            .stdout
            .split(|&byte| byte == 0)
            .filter(|entry| !entry.is_empty())
            .map(|entry| String::from_utf8_lossy(entry).into_owned())
            .collect(),
    )
}

/// Resolves a citation's `path` to the file it names under `root`: the
/// literal, root-relative join when that exists, or -- for a bare
/// filename (no `/`) with no such literal match -- the one entry in
/// `tracked_files` whose own basename equals it, when exactly one such
/// file exists. This repository's own narrative prose overwhelmingly
/// cites a source file by its bare basename (`install.rs:170`, never
/// `crates/houserules/src/install.rs:170`, a real shape this task's own
/// corpus run measured at 21 citations for that one file alone); without
/// this fallback nearly every such citation in the retained corpus would
/// read as "does not exist" even though the file plainly does. See the
/// module doc's Limits for the zero- and multiple-match cases this still
/// falls back on the literal join for.
fn resolve_citation_path(root: &Path, path: &str, tracked_files: &[String]) -> PathBuf {
    let literal = resolve_against(root, path);
    if literal.is_file() || path.contains('/') {
        return literal;
    }
    let mut matches = tracked_files.iter().filter(|tracked| {
        Path::new(tracked)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(path)
    });
    let Some(only_match) = matches.next() else {
        return literal;
    };
    if matches.next().is_some() {
        return literal; // ambiguous basename: more than one tracked file matches
    }
    root.join(only_match)
}

/// Loads a citation's target file from `root`: a bare filename resolves
/// through `resolve_citation_path` first (the tracked-tree basename
/// fallback), and everything else falls back to the same fs-read
/// (`resolve_against` + `read_utf8_lossy`) every other check in this
/// module already uses -- this tool has never distinguished a tracked-
/// tree citation from one naming a file in the gitignored batch
/// workspace a report cites (`resolve_against` just joins).
fn load_citation(
    root: &Path,
    citation: &Citation,
    tracked_files: &[String],
) -> std::io::Result<String> {
    read_utf8_lossy(&resolve_citation_path(root, &citation.path, tracked_files))
}

/// `true` when `a` and `b` sit within `window` bytes of each other (zero
/// distance when they overlap or touch) -- the deterministic "same
/// sentence" proxy this module already uses for a self-audit ratio and
/// its co-located sha (`RATIO_SHA_WINDOW`); HR-110 and HR-111 reuse the
/// same proxy for a citation and the claim it is offered to support. See
/// the module doc's Limits for why a byte window stands in for a real
/// sentence boundary.
fn ranges_close(a: &std::ops::Range<usize>, b: &std::ops::Range<usize>, window: usize) -> bool {
    let gap = b
        .start
        .saturating_sub(a.end)
        .max(a.start.saturating_sub(b.end));
    gap <= window
}

// ---- HR-107: a citation's line range must fit the file it names ----

/// Every `path:NNN`/`path:NNN-MMM` citation `collect_narrative`'s four
/// fields carry must name a file that reads clean under `root`, and NNN
/// (and MMM, for a range) must not exceed that file's line count. A bare
/// citation with no line suffix (`NarrativeField::citations` also carries
/// those, for HR-110 and HR-111 below) is not this check's business. See
/// the module doc's Limits for the comma-list and range-ordering shapes
/// this does not chase.
fn check_citation_lines(
    root: &Path,
    narrative: &[NarrativeField],
    tracked_files: &[String],
    errors: &mut Vec<String>,
) {
    let mut seen: Vec<(String, String)> = Vec::new();
    for field in narrative {
        let label = &field.label;
        for citation in &field.citations {
            let Some(line) = citation.line else {
                continue;
            };
            let token = match citation.line_end {
                Some(end) => format!("{}:{line}-{end}", citation.path),
                None => format!("{}:{line}", citation.path),
            };
            let key = (label.clone(), token.clone());
            if seen.contains(&key) {
                continue;
            }
            match load_citation(root, citation, tracked_files) {
                Ok(content) => {
                    let count = line_count(&content) as u64;
                    let max_cited = citation.line_end.unwrap_or(line);
                    if max_cited > count {
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
/// `RATIO_SHA_WINDOW`, at the same value, for the same reason (see
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
fn check_narrative_number_claims(
    root: &Path,
    narrative: &[NarrativeField],
    tracked_files: &[String],
    errors: &mut Vec<String>,
) {
    let count_phrase = Regex::with_flags(r"\b(\d+)\s+(?:passed|files?)\b", "i")
        .expect("valid count-phrase pattern");
    for field in narrative {
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
            match load_citation(root, citation, tracked_files) {
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

// ---- HR-111: a cited sweep must cover the file it certifies ----

/// Every path `report["files_changed"]` names -- the plain string list
/// every task-report and branch-review already carries.
/// `check_narrative_sweep_coverage` treats each as a file a cited sweep
/// should cover: this schema field exists on every deliverable this tool
/// already reads, so no narrative-parsing heuristic is needed to know
/// which files a sweep's clean result has to certify.
fn collect_files_changed(report: &Value) -> Vec<String> {
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
fn check_narrative_sweep_coverage(
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
        // below (fix round 1, minor issue 7 -- round 0 called
        // `load_citation` once per keyword match instead of once per
        // qualifying citation).
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
            let content = match load_citation(root, citation, tracked_files) {
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

/// Reads and parses `path` as a JSON report, naming the file in any read
/// or parse error (`houserules.crash-paths-are-named`):
/// `cmd_check_report_claims`, the one caller, maps a read failure and a
/// parse failure to the same exit code, 2.
fn load_report(path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Runs every check above against the report at `report_path` and returns
/// its errors, each a one-line, self-contained description naming the
/// field and the mismatch. An empty vec is a clean run: every capture,
/// marker, narrative claim, and command field this tool knows how to
/// check matched its artifact and could paste-run. `Err` only when
/// `report_path` could not be read as JSON.
fn check_report_claims(report_path: &Path, root: &Path) -> Result<Vec<String>, String> {
    let report = load_report(report_path)?;
    let runs = collect_runs(&report);
    let narrative = collect_narrative(&report);
    let narrative_fields = collect_narrative_fields(&narrative);
    let tracked_files = list_tracked_files(root).unwrap_or_default();
    let mut errors = Vec::new();
    check_redirected_captures(root, &runs, &mut errors);
    check_truncation_markers(root, &runs, &mut errors);
    check_self_audit_head_is_current(root, &report, &mut errors);
    check_narrative_shas_resolve(root, &narrative, &mut errors);
    check_self_audit_narrative(&narrative, &report, &mut errors);
    check_paste_run_lint(&runs, &mut errors);
    check_ephemeral_paths(&narrative, &mut errors);
    check_citation_lines(root, &narrative_fields, &tracked_files, &mut errors);
    check_narrative_number_claims(root, &narrative_fields, &tracked_files, &mut errors);
    check_narrative_sweep_coverage(
        root,
        &narrative_fields,
        &collect_files_changed(&report),
        &tracked_files,
        &mut errors,
    );
    Ok(errors)
}

/// The `check-report-claims` subcommand (`crate::main`'s dispatch):
/// `dir`/`--dir` resolves the artifact-and-git root the same way every
/// other flat-surface command does (`crate::root::resolve_root`), and
/// `report_path` resolves against the real process working directory the
/// way `validate`'s own file arguments do (`crate::node_path::
/// resolve_like_node`) -- the two are independent by design, since a
/// report can live under a subdirectory `--dir` has no reason to name.
/// Exit 0 (clean; prints `no claim mismatches found`), 1 (one line per
/// mismatch on stderr), or 2 (the root could not be resolved, or
/// `report_path` could not be resolved, read, or parsed as JSON). A
/// missing `report_path` argument is clap's own required-positional
/// check; this function never sees that case.
pub(crate) fn cmd_check_report_claims(dir: Option<PathBuf>, report_path: PathBuf) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let resolved = match crate::node_path::resolve_like_node(&report_path) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{}: {error}", report_path.display());
            return ExitCode::from(2);
        }
    };
    match check_report_claims(&resolved, &root) {
        Ok(errors) if errors.is_empty() => {
            println!("{}: no claim mismatches found", report_path.display());
            ExitCode::SUCCESS
        }
        Ok(errors) => {
            for error in &errors {
                eprintln!("{error}");
            }
            ExitCode::from(1)
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use serde_json::{Value, json};

    use super::*;

    // ---- fixture builders ----

    /// A fresh git repo under a scratch dir, with one commit so `HEAD`
    /// resolves.
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

    /// A minimal, otherwise-clean task-report shape naming `head` as the
    /// one and only commit in both `self_audit` and `commits`, so `head`
    /// is trivially the report's own newest listed commit.
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

    /// Passes a report with no captures, markers, or narrative claims.
    #[test]
    fn passes_a_report_with_no_captures_markers_or_narrative_claims() {
        let (dir, head) = init_scratch_repo("check-report-claims-clean-");
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &base_report(&head));
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Flags a redirected capture whose output does not byte-match its own target file.
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

    /// Passes a redirected capture whose output byte-matches its target file.
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

    /// Resolves an absolute-path redirect target as-is, not joined onto root.
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

    /// Flags a truncation marker whose claimed remaining-line count is wrong.
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

    /// Passes a truncation marker with the correct remaining-line count.
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

    /// A marker's claimed count can exceed `u64::MAX` even though it
    /// matches `\d+`; this is its own named error
    /// (`houserules.crash-paths-are-named`), never a silent `u64::MAX`
    /// default that then reports a fabricated mismatch against the real
    /// remaining-line count.
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

    /// The mismatch message quotes the marker's own captured digits
    /// (`process.claims-match-artifacts`), not a re-parsed-and-reformatted
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

    /// Does not flag an all-digit token even though it matches the sha shape.
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

    /// Flags a self_audit-describing sentence with the wrong deterministic pass ratio.
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

    /// Passes a self_audit-describing sentence naming the real head and the real ratio.
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

    // ---- the bounded, no-execution paste-run lint ----

    /// An angle-bracket placeholder.
    #[test]
    fn flags_a_command_carrying_an_angle_bracket_placeholder() {
        let (dir, head) = init_scratch_repo("check-report-claims-lint-placeholder-");
        let mut report = base_report(&head);
        report["tests"] =
            json!([{"command": "houserules check-report-claims <REPORT_FILE>", "output": ""}]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "tests[0]: command carries the placeholder \"<REPORT_FILE>\" and cannot paste-run as written".to_string()
            ]
        );
    }

    /// Prose (here, a parenthetical label) appended after the command's
    /// closing quote.
    #[test]
    fn flags_a_command_with_a_parenthetical_label_appended_after_it() {
        let (dir, head) = init_scratch_repo("check-report-claims-lint-annotation-");
        let mut report = base_report(&head);
        report["tests"] = json!([{
            "command": "curl -sSL \"https://example.invalid/install.sh\" | sh (expect exit 22)",
            "output": "",
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "tests[0]: command has \"(expect exit 22)\" appended after it, which a shell would run as a second, separate command if pasted".to_string()
            ]
        );
    }

    /// A failed shlex-style parse (an unterminated double quote).
    #[test]
    fn flags_a_command_whose_quoting_does_not_parse() {
        let (dir, head) = init_scratch_repo("check-report-claims-lint-unbalanced-");
        let mut report = base_report(&head);
        report["tests"] = json!([{
            "command": "python3 -c \"import sys; print(sys.argv[1]",
            "output": "",
        }]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "tests[0]: command's quoting does not parse (an unterminated \" quote) and cannot paste-run as written".to_string()
            ]
        );
    }

    /// A clean report using real, legitimate shell shapes must not flag:
    /// process substitution (`diff <(...) <(...)`), a subshell wrapping
    /// the WHOLE command (`( cmd; echo "EXIT=$?" )`), and an escaped
    /// `find`-style parenthesis group all stay clean.
    #[test]
    fn does_not_flag_legitimate_process_substitution_subshells_or_escaped_parens() {
        let (dir, head) = init_scratch_repo("check-report-claims-lint-clean-");
        let mut report = base_report(&head);
        report["tests"] = json!([
            {"command": "diff <(git show b9e7b9a:template/tools/kb.mjs) <(git show b9e7b9a:tools/kb.mjs)", "output": ""},
            {"command": "( mise x -- pnpm exec vitest run tests/dogfood.test.mjs --coverage.enabled=false; echo \"EXIT=$?\" )", "output": ""},
            {"command": "find /tmp -maxdepth 1 -type d \\( -name 'a' -o -name 'b' \\)", "output": ""},
        ]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(errors, Vec::<String>::new());
    }

    /// POSIX begins a comment only at the start of a word (IEEE Std
    /// 1003.1-2024 §2.3); a mid-word `#`, like a GitHub issue reference
    /// glued onto a path, is one literal token, not a comment.
    #[test]
    fn does_not_flag_a_mid_word_hash_that_is_not_a_comment_marker() {
        let (dir, head) = init_scratch_repo("check-report-claims-lint-midword-hash-");
        let mut report = base_report(&head);
        report["tests"] = json!([{"command": "grep -n pattern notes#1.txt", "output": ""}]);
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
        report["live_run"] = json!([{
            "command": "cd /tmp/claude-1001/scratch/hr075-live && houserules check-knowledge",
            "output": "knowledge: ok\n",
        }]);
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
        report["self_review"] = json!([
            "Ran the scratch git repository under the session scratchpad, never touching this repository's working tree."
        ]);
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
        report["self_review"] = json!([
            "Left the scratch install at .../scratchpad/hr033-live in place; no prior task in this batch reused it."
        ]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0]: cites \".../scratchpad/hr033-live\", an ephemeral path that will not exist once the session ends".to_string()
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
        report["self_review"] = json!([
            "Fixed the live-run script (scratchpad/task2-scratch-audit.mjs) to mkdtempSync under the session scratchpad."
        ]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0]: cites \"scratchpad/task2-scratch-audit.mjs\", an ephemeral path that will not exist once the session ends".to_string()
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
        report["self_review"] = json!(["Reviewed the fix at cited.rs:2-10."]);
        let report_path = dir.path().join("report.json");
        write_json(&report_path, &report);
        let errors = check_report_claims(&report_path, dir.path()).expect("report loads");
        assert_eq!(
            errors,
            vec![
                "self_review[0]: cites \"cited.rs:2-10\", but cited.rs has only 3 lines"
                    .to_string()
            ]
        );
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
        report["live_run"] =
            json!([{"command": "sed -n '1,999p' cited.rs:999", "output": "irrelevant"}]);
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
    /// 1` boundary (fix round 1, important issue 4).
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
    /// pins the artifact-shape boundary (fix round 1, important issue 4).
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
}
