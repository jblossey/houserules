//! The `check-report-claims` flat-surface subcommand: cross-checks one
//! deliverable report's claims against the artifacts and git history it
//! cites (batch 20 T2, HR-066, docs/specs/2026-09-07-batch-20-phase5.md
//! §3). Ported first to a dev-only `src/bin/check-report-claims.rs` target
//! in this crate (batch 18 T1, HR-061, docs/specs/2026-09-05-batch-18-
//! phase3.md §7: `tools/check-report-claims.mjs`, ported byte-for-check,
//! not byte-for-byte -- see this module's own "Port notes" below for why
//! that distinction matters here); moved here and wired into
//! `crate::main`'s dispatch in the same commit the dev bin retires, no
//! thin wrapper left behind: the crate carries no `[lib]` target, so a
//! second binary sharing this module's code would need one purely to keep
//! an invocation shape (`cargo run --bin check-report-claims`) that only
//! this repository could ever run -- exactly the gap this move exists to
//! close (`quality.no-compat-softening`; every doc naming that invocation
//! now names `houserules check-report-claims` instead -- the tree-wide
//! residue sweep bounding that closure is retained at
//! `.superpowers/sdd/2026-09-07-batch-20/t2-evidence/residue-sweep-bin-
//! check-report-claims.txt` and cited in this task's own report's
//! `fix_rounds[0].tests[0]`; every hit it finds is historical prose, not
//! a live instruction). The dev bin's `cargo run --bin
//! check-report-claims` command could run only inside this repository;
//! `houserules check-report-claims` runs anywhere the shipped binary
//! does, so `template/.claude/agents/implementer.md`'s seeded closing act
//! (HR-060) now runs in every adopter repo `houserules init` seeds, not
//! only this one (`houserules.payload-runs-on-builtins`'s second
//! known-gap bullet, retired at this same commit). Lives at the crate
//! root, like `emit`, `get`, `install`, `node_path`, and `root`, for the
//! reason each of those gives: this module needs none of `crate::rules`'
//! or `crate::backlog`'s modules (only `serde_json`, `std`, and a `git`
//! subprocess for the same plumbing every other command in this crate
//! already shells out to), so nesting it under either would buy nothing.
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
//! - `check_paste_run_lint` (HR-071, batch 20 T2, spec §4) flags a command
//!   field carrying an angle-bracket placeholder, text appended after the
//!   command that a shell would run as a second, separate command, or
//!   quoting that does not balance -- the three shapes recurring across
//!   batch 19's four review rounds (T1 r1/r2, T2 r1/r3). It cannot see a
//!   retyped command that still parses cleanly: batch 19 T2 round 3's own
//!   review found a command field carrying two literal backslashes before
//!   a quote (`python3 -c \"import yaml; ...\"`) that every real shell
//!   parses -- differently from what was intended, but without error --
//!   which is exactly the shape no quote-balance check can catch, named
//!   here rather than silently missed.
//! - `check_paste_run_lint`'s appended-text check exempts a parenthesized
//!   subshell wrapping the WHOLE command (`( cmd )`, an idiom this
//!   project's own history uses live) but not one chained after the
//!   command's first token (`cmd1 && (cmd2)`, also legitimate shell):
//!   `.superpowers/sdd/2026-09-04-batch-17/task-2-report.json`'s own
//!   `live_run[8]` (`d=$(mktemp -d) && cp -r ... && git init -q "$d" &&
//!   (cd "$d" && ...)`) is exactly that shape, and the shipped checker
//!   flags it -- a real, measured false-positive vehicle (a full sweep of
//!   this project's own 65 workspace reports, retained at
//!   `t2-evidence/corpus-sweep-full.txt`, is where this instance surfaced),
//!   disclosed rather than special-cased away. Narrowing this arm is
//!   deferred to HR-072, filed against this measurement.
//! - `check_paste_run_lint`'s placeholder check also fires on command
//!   DATA that only looks like a placeholder, since it applies regardless
//!   of `quote_mask`'s state: a literal `<...>` sitting inside a quoted
//!   string (an HTML-comment probe, an email address) is real,
//!   paste-runnable text, not a substitution point, but the check cannot
//!   tell the two apart. This is the lint's most frequent false-positive
//!   vehicle in this project's own history: the same corpus sweep finds
//!   it 5 times across 4 reports --
//!   `.superpowers/sdd/2026-09-01-batch-4/task-4-report.json` `tests[10]`
//!   (an HTML-comment probe, `<!-- probe: ... -->`),
//!   `.superpowers/sdd/2026-09-02-batch-6/task-4-report.json` `tests[13]`
//!   and `fix_rounds[0].tests[18]`,
//!   `.superpowers/sdd/2026-09-05-batch-18/task-2-report.json`
//!   `live_run[4]`, and
//!   `.superpowers/sdd/2026-09-07-batch-20/task-1-report.json`
//!   `live_run[11]` (the last three of these four a `git commit -m
//!   "... Co-Authored-By: ... <a@b.com>"` shape this repository's own
//!   standing no-coauthor rule guarantees will keep recurring as literal
//!   test data). Narrowing this arm (a quote-aware placeholder check) is
//!   deferred to HR-072 alongside the subshell narrowing above.
//! - `check_paste_run_lint`'s placeholder check reads any `<...>` span
//!   with no whitespace touching either bracket as a placeholder,
//!   excluding only `<(`/`>(` process substitution -- verified against
//!   the same corpus sweep, which never misreads a real process
//!   substitution (`diff <(cmd1) <(cmd2)`, seen live in this project's
//!   own history) as one. A command legitimately using bare `< file`
//!   stdin redirection immediately followed later on the same line by a
//!   `>` output redirect would still misread as a placeholder; no report
//!   in the swept corpus does this, so it stays a theoretical,
//!   undemonstrated vehicle, not one this project's own history can point
//!   an instance at.
//! - `check_ephemeral_paths` (HR-075, batch 21 T2, docs/specs/2026-09-08-
//!   batch-21-gates.md §3) flags an absolute `/tmp/`-rooted path or a
//!   `scratchpad/`-named directory segment anywhere in the same four
//!   narrative fields `collect_narrative` already scans -- never inside a
//!   `command` field (`collect_runs`), where `/tmp` is the literal,
//!   required text of what ran (a live-run scratch repository lives there
//!   by design, `houserules.live-run-recipe`) and this check has no
//!   business judging it. The second shape widened the first at batch 21
//!   T2 fix round 1 (review important issue 1): six real corpus citations
//!   name this project's own session-scratchpad home with no `/tmp/`
//!   prefix at all, several eliding it to a bare `.../` (`.superpowers/sdd/
//!   2026-09-08-batch-21/t2-evidence/rollout-check-report-claims-
//!   ephemeral.sh`'s own re-derivation over the same 116-file corpus:
//!   batch 3 task 2, batch 4 task 3, batch 9 tasks 3 and 4, batch 10 tasks
//!   2 and 3). `/tmp/` still requires the word it roots to start with `/`
//!   (`word_qualifies`'s own doc has the boundary account, fixed the same
//!   round against review important issue 2: an unanchored substring match
//!   used to flag a durable, tracked path like `tests/tmp/golden.json` or
//!   a URL's own path segment); `scratchpad/` carries no such requirement,
//!   since every real citation of that shape is the bare directory name.
//!   Both shapes together are still blind to a macOS `$TMPDIR` (typically
//!   `/var/folders/.../T/`), a Windows `%TEMP%`, or any other platform's
//!   ephemeral home with no `scratchpad/` segment in it, and to a
//!   durable-LOOKING repository-relative path that is in fact untracked or
//!   never committed: a report cannot be checked against a git object that
//!   was never added. A bare mention of the WORD "scratchpad" with no
//!   `scratchpad/`-segmented path attached (this project's own history
//!   carries dozens, honestly discussing the concept) is deliberately not
//!   this check's business either. The `scratchpad/` arm carries no
//!   absolute-path requirement (batch-21 T2 fix round 2, review new
//!   breakage, minor): it flags any word containing a `scratchpad/`
//!   segment, durable or not, so a real, tracked, repository-relative path
//!   naming one -- verified live: `docs/scratchpad/notes.md` in a
//!   narrative field flags -- would over-flag the same way the unanchored
//!   `/tmp/` match once did. Left unanchored on measurement, not
//!   oversight: `git ls-files | grep -c 'scratchpad/'` is 0 in this
//!   repository's own tracked tree, and all six real corpus citations this
//!   arm was built from (module doc, above) are genuinely ephemeral, so
//!   nothing is mis-flagged today; named here as this project's own
//!   history grows, per this file's own standard of naming even
//!   undemonstrated false-positive vehicles. A glued NON-punctuation prefix
//!   is also invisible to both shapes (batch 21 branch review, minor
//!   issue): `EPHEMERAL_PATH_LEADING_PUNCTUATION` trims neither `=` nor
//!   `:`, so a word like `OUT=/tmp/x` or `dest:/tmp/x` -- an env-var
//!   assignment or a labeled value quoted in prose -- keeps that prefix
//!   after the trim, no longer starts with `/`, and `word_qualifies`
//!   rejects it the same way an unanchored bare path would have passed
//!   unflagged before this check existed. Undemonstrated in the swept
//!   corpus today (docs-only disclosure; the corpus-measured boundary
//!   stands, no behavior change licensed here), named per this file's own
//!   standard.
//! - `quote_mask` has no notion of `$(...)` command substitution
//!   resetting quote context: real bash parses a same-character quote
//!   opened again inside a `$(...)` (or `` `...` ``) as starting a fresh,
//!   independent quoted region, not as closing the one around the whole
//!   substitution. This walk does not track that: `bash -c "$(python3 -c
//!   "import yaml; d=yaml.safe_load(open('...')); ...")"` (batch 19 task
//!   2's own `fix_rounds[0].tests[4]`/`fix_rounds[2].tests[0]`, a real,
//!   valid command) reads the inner `"import yaml...` quote as closing
//!   the outer one, so by the time the walk reaches `open(...)` it
//!   believes itself outside any quote -- a false positive, surfaced live
//!   by this lint's first run against real report history (this task's
//!   own `live_run`), not one a walker this project deliberately keeps
//!   below full shell grammar (see the port notes above) can rule out.
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
//! - A usage error (no report path given) is clap's own required-argument
//!   message, exit 2, once this tool became a flat-surface subcommand
//!   (batch 20 T2) -- neither JS's thrown `UsageError` nor the dev bin's
//!   own hand-written `usage:` line, both retired with it. A `git`
//!   failure (not a repository, `git` missing) is likewise a named error
//!   rather than an uncaught crash (`houserules.crash-paths-are-named`).

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

/// A reason `quote_mask` could not finish walking a command string to the
/// end -- the "failed shlex-style parse" shape `check_paste_run_lint`
/// (HR-071) flags. Both cases are exactly what a real shell's own
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
/// occur within single-quotes" (IEEE Std 1003.1-2024 §2.2.2, verified
/// live). Outside any quote, POSIX has a bare backslash escape exactly
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
/// none is already a dependency of this crate, spec §8
/// (docs/specs/2026-09-07-batch-20-phase5.md) bounds this task to std plus
/// the crate's existing dependencies, and every one of those crates
/// implements the full POSIX word-splitting grammar this lint has no use
/// for (`security-hygiene.dependency-vetting`,
/// `quality.well-maintained-libraries`: reasoning stated per the task,
/// since this repository carries no maintained, std-only, no-execution
/// tokenizer today).
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
/// comment only there, never mid-word -- verified live, see the
/// implementation's own doc), or an unquoted `(` that opens neither a
/// `$(...)` command substitution nor an escaped `\(` literal (`find`'s own
/// `\( -name a -o -name b \)` idiom, seen live in this project's own
/// history) nor process substitution's own `<(`/`>(`. A `(` that is
/// itself the command's first token -- a subshell wrapping the WHOLE
/// command (`( cmd; echo "EXIT=$?" )`, also seen live) -- is exempt too:
/// only text appended AFTER a command that has already started counts
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
                // POSIX (IEEE Std 1003.1-2024 §2.3, verified live): '#' begins a
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

/// The bounded, no-execution paste-run lint (HR-071, batch 20 T2, spec
/// §4): flags a command field carrying an angle-bracket placeholder, text
/// appended after the command that a shell would run as a second command,
/// or quoting that does not balance -- the three shapes recurring across
/// batch 19's four review rounds (see the module doc's limits for what
/// none of the three can see). A command whose quoting fails to parse
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
/// see the module doc's HR-075 bullet for what this prefix still cannot
/// see.
const EPHEMERAL_PATH_PREFIX: &str = "/tmp/";

/// The directory-segment shape this project's own session-scratchpad home
/// always ends in, matched wherever it appears in a word (no absolute-path
/// requirement: batch-21 T2 fix round 1, review important issue 1 -- six
/// real corpus citations name this shape with no `/tmp/` prefix at all,
/// several eliding the prefix to a bare `.../` -- see the module doc's
/// HR-075 bullet).
const EPHEMERAL_SCRATCHPAD_SEGMENT: &str = "scratchpad/";

/// Leading bytes this project's own narrative prose glues onto an ephemeral
/// citation that belong to the surrounding SENTENCE, not the path itself:
/// an opening bracket/brace/paren, or a quote or backtick opening a
/// markdown code span (`(scratchpad/task2-scratch-audit.mjs)`, a real
/// corpus citation -- see the module doc's HR-075 bullet).
const EPHEMERAL_PATH_LEADING_PUNCTUATION: &[char] = &['(', '[', '{', '\'', '"', '`'];

/// Trailing bytes this project's own narrative prose glues onto an
/// ephemeral citation that belong to the surrounding SENTENCE, not the path
/// itself: a comma or period ending the clause, a closing bracket/paren/
/// brace, a quote, or a backtick closing a markdown code span. Re-measured
/// at batch-21 T2 fix round 2 (review critical issue 1b: fix round 1's own
/// correction traded one wrong count for another) directly from
/// `t2-evidence/enumerate-narrative-tmp-hits.py`'s own run at HEAD
/// (`t2-evidence/r2-c1b-remeasure.txt`, retained): 10 tokens total, six
/// carrying trailing punctuation glued to the path with no space --
/// `/tmp/houserules-target-uQK5Qz/tools/kb.mjs,`, `/tmp/renamed-binary)`,
/// `/tmp/release-committed.yml,`, `/tmp/verify-goldens.py,`, and two from
/// this very report's own fix_rounds[0] narrative illustrating the
/// anchoring fix rather than citing a real ephemeral artifact,
/// `tests/tmp/golden.json)` and `.../tmp/report.html)`. The remaining four
/// -- `/tmp/hr009-*`, both `/tmp/hr009-aKTTWA` occurrences, and
/// `/tmp/hr009-nN2oLG` -- carry none: each is followed by a space in its
/// source sentence, verified against the raw corpus text, not restated.
/// The `` /tmp/houserules-fixbase-worktree`; `` token both earlier rounds
/// counted no longer appears in this run at all: ruling R1 rewrote
/// task-1-report.json's three narrative citations later in fix round 1,
/// removing it from the corpus.
const EPHEMERAL_PATH_TRAILING_PUNCTUATION: &[char] =
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
/// the kind of token `trigger` may legitimately root (batch-21 T2 fix
/// round 1, review important issue 2). `/tmp/` counts only when `word` is
/// itself an absolute path (starts with `/`) -- otherwise the trigger sits
/// inside a relative path (`tests/tmp/golden.json`) or a URL's own path
/// segment (`https://example.test/tmp/report.html`), neither an ephemeral
/// filesystem location; `/var/tmp/capture.txt` still qualifies, rooted at
/// its own leading `/`, not at the `/tmp/` substring partway through it.
/// `scratchpad/` carries no such requirement: every real corpus citation of
/// that shape (module doc's HR-075 bullet) is the bare directory name,
/// with or without a leading `/` or an elided `.../` prefix, so requiring
/// an absolute-path start would exclude the shape this arm exists to
/// catch.
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
            .trim_start_matches(EPHEMERAL_PATH_LEADING_PUNCTUATION)
            .trim_end_matches(EPHEMERAL_PATH_TRAILING_PUNCTUATION);
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
/// unique path per field (HR-075, docs/specs/2026-09-08-batch-21-gates.md
/// §3): a narrative claim naming an artifact there describes a location
/// that will not exist by the time anyone re-opens the report. Never scans
/// a `command` field -- see the module doc's HR-075 bullet for the full
/// boundary account of why, and what these two shapes still cannot see.
fn check_ephemeral_paths(narrative: &[(String, String)], errors: &mut Vec<String>) {
    for (label, text) in narrative {
        let mut seen: Vec<&str> = Vec::new();
        flag_ephemeral_words(text, EPHEMERAL_PATH_PREFIX, &mut seen, label, errors);
        flag_ephemeral_words(text, EPHEMERAL_SCRATCHPAD_SEGMENT, &mut seen, label, errors);
    }
}

/// Reads and parses `path` as a JSON report, naming the file in any read
/// or parse error -- `tools/lib/json-store.mjs`'s `readJson`, collapsed to
/// one named-error shape (`houserules.crash-paths-are-named`): JS raises a
/// plain `Error` for a missing file and a `UsageError` for invalid JSON,
/// kept apart only so its `main` can choose an exit code, but
/// `cmd_check_report_claims`, the one caller that maps this function's
/// errors to an exit code, treats both the same way, exit 2.
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
    let mut errors = Vec::new();
    check_redirected_captures(root, &runs, &mut errors);
    check_truncation_markers(root, &runs, &mut errors);
    check_self_audit_head_is_current(root, &report, &mut errors);
    check_narrative_shas_resolve(root, &narrative, &mut errors);
    check_self_audit_narrative(&narrative, &report, &mut errors);
    check_paste_run_lint(&runs, &mut errors);
    check_ephemeral_paths(&narrative, &mut errors);
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
/// `report_path` could not be resolved, read, or parsed as JSON -- see the
/// module doc's port notes for the missing-argument case, which clap's
/// own required-positional check now owns instead of this function).
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

    // ---- HR-071: the bounded, no-execution paste-run lint ----

    /// Natural RED (pre-fix: `check_report_claims` called only the
    /// original five checks; this shape was invisible). One of the three
    /// batch-19 shapes: an angle-bracket placeholder.
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

    /// One of the three batch-19 shapes: prose (here, a parenthetical
    /// label, batch 19 T2 round 3's own recurrence) appended after the
    /// command's closing quote.
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

    /// One of the three batch-19 shapes: a failed shlex-style parse (an
    /// unterminated double quote).
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

    /// A clean report using real, legitimate shell shapes from this
    /// project's own history must not flag: process substitution
    /// (`diff <(...) <(...)`, batch 8 task 2), a subshell wrapping the
    /// WHOLE command (`( cmd; echo "EXIT=$?" )`, batch 9 task 2), and an
    /// escaped `find`-style parenthesis group (batch 10 task 1) all stay
    /// clean.
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

    /// POSIX begins a comment only at the start of a word (verified live
    /// against IEEE Std 1003.1-2024 §2.3); a mid-word `#`, like a GitHub
    /// issue reference glued onto a path, is one literal token, not a
    /// comment.
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

    // ---- HR-075: the ephemeral-path check ----

    /// Natural RED (pre-fix: `check_report_claims` called only the original
    /// six checks; this shape was invisible). HR-075's own incident,
    /// verbatim: `.superpowers/sdd/2026-09-07-batch-20/task-3-report.json`'s
    /// `fix_rounds[0].findings[2].finding` names the exact narrative
    /// citation that cost batch 20 T3 a fix round.
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

    /// The measured false-positive class this check deliberately does not
    /// catch (module doc's HR-075 bullet): this project's own history
    /// carries dozens of honest narrative sentences discussing the CONCEPT
    /// of a session scratchpad with no path attached at all.
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

    /// Two more real corpus shapes in one document: the same path cited
    /// twice flags once (dedup), and a trailing comma or period ending the
    /// sentence is trimmed from the quoted path.
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

    /// Natural RED (batch-21 T2 fix round 1, review important issue 1): the
    /// widened scratchpad-shape arm did not exist before this round. Seeded
    /// verbatim from one of the six real corpus shapes the review's own
    /// probe re-derives: `.superpowers/sdd/2026-09-02-batch-9/task-3-
    /// report.json`'s `self_review[7]` names a scratch install at an
    /// elided `.../scratchpad/hr033-live` path.
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

    /// Natural RED (batch-21 T2 fix round 1, review important issue 1): a
    /// parenthesized, no-leading-dots scratchpad citation -- the other real
    /// corpus shape (batch-3 task-2-report.json fix_rounds[0].findings[2].
    /// fix) -- also needs the leading-punctuation trim `flag_ephemeral_
    /// words` added this round, or the flagged text would keep the
    /// enclosing `(`.
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

    /// Natural RED (batch-21 T2 fix round 1, review important issue 2, the
    /// reviewer's own first probe): an unanchored substring match used to
    /// flag a durable, tracked, repository-relative path merely for
    /// containing a `/tmp/` segment.
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

    /// Natural RED (batch-21 T2 fix round 1, review important issue 2, the
    /// reviewer's own second probe): a URL whose own path carries a
    /// `/tmp/` segment is not a local ephemeral path.
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

    /// Natural RED (batch-21 T2 fix round 1, review important issue 2, the
    /// reviewer's own third probe): `/var/tmp/` is still a real, ephemeral
    /// absolute path -- it must still flag, quoting the WHOLE path from its
    /// own leading `/`, not the bare `/tmp/capture.txt` suffix an
    /// unanchored match used to fabricate.
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
}
