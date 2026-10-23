//! The `check-report-claims` flat-surface subcommand: cross-checks one
//! deliverable report's claims against the artifacts and git history it
//! cites. Lives at the crate root, like `emit`, `get`, `install`,
//! `node_path`, and `root`. From the rest of the crate it uses
//! `rules::tracked_files` (the tracked files), `root::resolve_root` (the
//! install root), and `node_path::resolve_like_node` (a report or doc path);
//! `doc.rs` also asks `rules::ignored_among` which cited paths git ignores and
//! `tracker::lookup` for the installed tracker. It needs none of
//! `crate::backlog`'s modules, so nesting it under `rules` or `backlog` would
//! buy nothing.
//!
//! A report's narrative prose can restate a fact that a later edit changed
//! elsewhere in the same report -- a stale head, a stale pass count, an
//! unresolvable sha -- and a hand re-check tends to miss exactly this
//! shape. This tool exists to catch that class mechanically.
//!
//! With `--doc <path>` or `--item <id>` in place of a report, the command
//! reads a spec or a backlog item and names each cited workspace path that
//! does not exist (`doc.rs`, `ClaimsSource`).
//!
//! The checks sit in submodules, one per family. Each submodule doc holds the
//! limits of its checks, and a doc comment that says "see the module doc's
//! limits" means the Limits list of the file that holds the comment. This tool
//! narrows deliberately, in favour of a low false-positive rate over
//! completeness, and a description of it should name what it does not catch,
//! not only what it does.
//!
//! Limits index:
//! - `narrative`: the stale head, the stale pass ratio, and the unresolvable sha
//!   in the narrative prose (`collect_narrative`), and the ephemeral path in
//!   every string of the report (`collect_report_strings`).
//! - `citations`: `find_citations` and the line, count, and sweep checks built
//!   on it, and the check of the recorded citations (`citations` of the report).
//! - `runs`: the capture check and the excerpt check (`collect_runs`).
//! - `natural_red`: the `natural`-label check.
//! - `evidence`: the evidence of the `self_review` claims and the re-run probes
//!   of the fix-round findings.
//! - `doc`: the `--doc` and `--item` modes.
//!
//! Further constraints (this tool carries no frozen-corpus parity
//! contract, unlike the flat command surface, so these are simply its own
//! bytes and its own behaviour, not a divergence from anything else):
//! - `resolve_against` joins a relative path onto `root` with
//!   `Path::join`; it does not collapse a `.`/`..` segment in the input
//!   path.
//! - A usage error (no report path given) is clap's own required-argument
//!   message, exit 2. A `git` failure (not a repository, `git` missing)
//!   is likewise a named error rather than an uncaught crash
//!   (`houserules.crash-paths-are-named`).

mod citations;
mod doc;
mod evidence;
mod narrative;
mod natural_red;
mod runs;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use serde_json::Value;

use crate::rules::tracked_files;
use citations::{
    check_citation_lines, check_narrative_number_claims, check_narrative_sweep_coverage,
    check_recorded_citations, collect_commit_body_fields, collect_files_changed,
    collect_narrative_fields,
};
use evidence::{check_rerun_probes, check_self_review_evidence};
use narrative::{
    check_ephemeral_paths, check_narrative_shas_resolve, check_self_audit_head_is_current,
    check_self_audit_narrative, collect_narrative, collect_report_strings,
};
use runs::{check_captures, check_excerpts, collect_runs};

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

/// The one sha among `shas` that every other sha is an ancestor of (a sha
/// equal to the candidate counts, so duplicates and a single sha qualify),
/// or `None` when no such sha exists: an empty list, shas from unrelated
/// or divergent history, or two or more distinct shas of which one git
/// cannot resolve -- `is_ancestor` returns `false` for every pairing that
/// includes it, so no candidate can satisfy its own `all()` check. That
/// last `None` is indistinguishable from the divergent-history one. The
/// result is not resolved here: a single sha is returned as it stands,
/// resolvable or not. `shas` are expected to form one straight line --
/// this task's own commit-by-commit history -- so this is the list's tip,
/// found by testing each candidate against every other rather than
/// assuming list order.
///
/// Two callers meet that contract differently.
/// `check_self_audit_head_is_current` resolves every listed sha first and
/// reports an unresolved one as its own error, so for it `None` means only
/// "no single line". `natural_red::newest_listed_commit`, which
/// `natural_red::check_labels_with` calls, passes the listed shas as they are,
/// resolves the result itself, and treats `None` and an unresolvable result
/// alike as silence (its `an_unresolvable_sha_is_silent` test).
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

/// Every path git tracks under `root`, through `rules::tracked_files` --
/// this repository's own tracked-file authority
/// (`quality.gates-derive-their-scope`), used to resolve a bare-filename
/// citation below. A tracked path that is no UTF-8 is left out: no
/// citation in a UTF-8 report can name it exactly. `None` when `root` is
/// not a git working tree, `git` itself is missing, or the command
/// otherwise fails -- never a filesystem walk, as `tracked_tree_files`
/// would give; a citation resolver given `None` degrades to the literal,
/// root-relative join for every citation rather than treating the
/// failure as its own finding -- the same silent-degrade precedent
/// `resolve_commit`/`is_ancestor` above already set for this module's
/// other `git` shell-outs (see the module doc's Further constraints).
fn list_tracked_files(root: &Path) -> Option<Vec<String>> {
    tracked_files(root).map(|(files, _undecodable)| files)
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
/// module doc's Limits of `citations` for the zero- and multiple-match cases
/// this still falls back on the literal join for.
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

/// Reads and parses `path` as a JSON report, naming the file in any read
/// or parse error (`houserules.crash-paths-are-named`):
/// `cmd_check_report_claims`, the one caller, maps a read failure and a
/// parse failure to the same exit code, 2.
fn load_report(path: &Path) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}: {error}", path.display()))
}

/// Runs every check of the submodules against the report at `report_path`
/// and returns its errors, each a one-line, self-contained description
/// naming the field and the mismatch. An empty vec is a clean run: every
/// capture, excerpt, narrative claim, and citation this tool knows how to
/// check matched its artifact. `Err` only when `report_path` could not be
/// read as JSON.
///
/// Each call below has a phrase in the `check-report-claims` about in
/// `main.rs`. The test `the_check_report_claims_about_names_every_check`
/// there derives the calls from this body and pins the pairing, so a call
/// added here fails that test until the about names it.
fn check_report_claims(report_path: &Path, root: &Path) -> Result<Vec<String>, String> {
    let report = load_report(report_path)?;
    let runs = collect_runs(&report);
    let narrative = collect_narrative(&report);
    let narrative_fields = collect_narrative_fields(&narrative);
    let commit_bodies = collect_commit_body_fields(root, &report);
    let strings = collect_report_strings(&report);
    let tracked_files = list_tracked_files(root).unwrap_or_default();
    let mut errors = Vec::new();
    check_captures(root, &runs, &mut errors);
    check_excerpts(root, &runs, &mut errors);
    natural_red::check_natural_red_labels(root, &report, &mut errors);
    check_recorded_citations(root, &report, &mut errors);
    check_self_review_evidence(root, &report, &mut errors);
    check_rerun_probes(root, report_path, &report, &mut errors);
    check_ephemeral_paths(&strings, &mut errors);
    check_self_audit_head_is_current(root, &report, &mut errors);
    check_narrative_shas_resolve(root, &narrative, &mut errors);
    check_self_audit_narrative(&narrative, &report, &mut errors);
    check_citation_lines(root, &narrative_fields, &tracked_files, &mut errors);
    check_narrative_number_claims(
        root,
        &narrative_fields,
        &commit_bodies,
        &tracked_files,
        &mut errors,
    );
    check_narrative_sweep_coverage(
        root,
        &narrative_fields,
        &collect_files_changed(&report),
        &tracked_files,
        &mut errors,
    );
    Ok(errors)
}

/// What `check-report-claims` reads the claims of: a deliverable report, a
/// spec, or a backlog item. Exactly one is given on the command line.
pub(crate) enum ClaimsSource {
    /// The report file at this path.
    Report(PathBuf),
    /// The spec at this path (`--doc`).
    Doc(PathBuf),
    /// The backlog item with this id, read through the installed tracker
    /// (`--item`).
    Item(String),
}

/// Checks the report at `report_path` against `root`: resolves the path
/// against the real working directory, then runs `check_report_claims`. An
/// error is the line to print, with the path named.
fn check_report_file(report_path: &Path, root: &Path) -> Result<Vec<String>, String> {
    let resolved = crate::node_path::resolve_like_node(report_path)
        .map_err(|error| format!("{}: {error}", report_path.display()))?;
    check_report_claims(&resolved, root)
}

/// The `check-report-claims` subcommand (`crate::main`'s dispatch):
/// `dir`/`--dir` resolves the artifact-and-git root the same way every
/// other flat-surface command does (`crate::root::resolve_root`), and
/// a report or doc path resolves against the real process working directory
/// the way `validate`'s own file arguments do (`crate::node_path::
/// resolve_like_node`) -- the two are independent by design, since a
/// report can live under a subdirectory `--dir` has no reason to name.
/// Exit 0 (clean; prints `<source>: no claim mismatches found`, the report
/// or doc path as given, or the item id), 1 (one line per mismatch on
/// stderr), or 2 (the root could not be resolved, the report or doc could not
/// be resolved or read, a report could not be parsed as JSON, or an item
/// could not be read: no tracker, an id the tracker does not hold, a tracker
/// that cannot be reached). A missing source is clap's own required-argument
/// check; this function never sees that case.
pub(crate) fn cmd_check_report_claims(dir: Option<PathBuf>, source: ClaimsSource) -> ExitCode {
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let (label, outcome) = match source {
        ClaimsSource::Report(path) => (path.display().to_string(), check_report_file(&path, &root)),
        ClaimsSource::Doc(path) => (path.display().to_string(), doc::check_doc(&path, &root)),
        ClaimsSource::Item(id) => {
            let outcome = doc::check_item(&id, &root);
            (id, outcome)
        }
    };
    match outcome {
        Ok(errors) if errors.is_empty() => {
            println!("{label}: no claim mismatches found");
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
    pub(super) fn init_scratch_repo(prefix: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir()
            .expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        let head = commit(dir.path(), "seed");
        (dir, head)
    }

    /// Runs `git` with `args` in `root` and returns its stdout. Panics with
    /// the stderr of git when git exits non-zero.
    pub(super) fn git(root: &Path, args: &[&str]) -> String {
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
    pub(super) fn commit(root: &Path, message: &str) -> String {
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

    /// Writes `value`, pretty-printed, to the file at `path`. Panics when the
    /// write fails.
    pub(super) fn write_json(path: &Path, value: &Value) {
        std::fs::write(path, serde_json::to_string_pretty(value).unwrap()).unwrap();
    }

    /// A minimal, otherwise-clean task-report shape naming `head` as the
    /// one and only commit in both `self_audit` and `commits`, so `head`
    /// is trivially the report's own newest listed commit.
    pub(super) fn base_report(head: &str) -> Value {
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

    // ---- capture fixtures ----

    /// The `end` of a fixture capture: early enough to precede every commit a
    /// test makes.
    pub(super) const EARLY_END: &str = "2001-09-09T01:46:40.000000000Z";

    /// The header of a capture of `argv` that exited with `exit` (a signal
    /// ended the command when `exit` is `None`) and ended at `end`.
    pub(super) fn capture_header(
        argv: &[&str],
        exit: Option<i32>,
        end: &str,
    ) -> crate::capture::Header {
        crate::capture::Header {
            schema: crate::capture::SCHEMA.to_string(),
            argv: argv.iter().map(ToString::to_string).collect(),
            cwd: "/work".to_string(),
            start: EARLY_END.to_string(),
            end: end.to_string(),
            exit,
            signal: exit.is_none().then_some(9),
            head: None,
            dirty: None,
            lossy: 0,
        }
    }

    /// Writes the capture of `header` and `output` at `rel` below `root`, in
    /// the layout `capture` writes (the header line, then the output after a
    /// LF; the header alone when the output is empty), and returns the `run`
    /// that cites it: `capture` as `rel` spells it, the hash of the file, and
    /// the `command` and `exit` of the header.
    pub(super) fn write_capture(
        root: &Path,
        rel: &str,
        header: &crate::capture::Header,
        output: &str,
    ) -> Value {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent directory"))
            .expect("create the folder of the capture");
        let mut bytes = serde_json::to_vec(header).expect("serialize the header");
        if !output.is_empty() {
            bytes.push(b'\n');
            bytes.extend_from_slice(output.as_bytes());
        }
        std::fs::write(&path, &bytes).expect("write the capture");
        json!({
            "capture": rel,
            "sha256": crate::baseline::hash(&bytes),
            "command": crate::shell::join(&header.argv),
            "exit": header.exit,
        })
    }

    /// A run that cites a fresh capture at `rel` of `argv`, which exited 0 and
    /// printed `output`.
    pub(super) fn clean_run(root: &Path, rel: &str, argv: &[&str], output: &str) -> Value {
        write_capture(root, rel, &capture_header(argv, Some(0), EARLY_END), output)
    }

    // ---- the tracked-file list ----

    /// A tracked path that is no UTF-8 stays out of the list, and the other
    /// tracked paths stay in it: a citation in a UTF-8 report cannot name
    /// such a path. Linux only: APFS on macOS refuses to create a file with
    /// this name (`houserules.platform-gated-tests`).
    #[cfg(target_os = "linux")]
    #[test]
    fn a_tracked_path_that_is_not_utf8_is_left_out_of_the_list() {
        use std::os::unix::ffi::OsStrExt;

        let (dir, _head) = init_scratch_repo("check-report-claims-tracked-");
        std::fs::write(dir.path().join("plain.md"), "plain\n").expect("write plain.md");
        let raw = std::ffi::OsStr::from_bytes(b"caf\xe9.md");
        std::fs::write(dir.path().join(raw), "latin-1\n").expect("write the non-UTF-8 file");
        git(dir.path(), &["add", "."]);

        assert_eq!(
            list_tracked_files(dir.path()),
            Some(vec!["plain.md".to_string()])
        );
    }

    /// A directory that is no git work tree gives no list, where the
    /// filesystem walk of `tracked_tree_files` would give one.
    #[test]
    fn a_directory_that_is_no_work_tree_gives_no_list() {
        let dir = tempfile::Builder::new()
            .prefix("check-report-claims-no-tree-")
            .tempdir()
            .expect("tempdir");
        std::fs::write(dir.path().join("plain.md"), "plain\n").expect("write plain.md");

        assert_eq!(list_tracked_files(dir.path()), None);
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
}
