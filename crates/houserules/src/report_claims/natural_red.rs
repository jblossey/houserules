//! The `natural`-label check of `check-report-claims` (HR-142, HR-204 (6)). A
//! `tdd` entry is labelled `natural` only when its RED ran before the commit
//! that makes it green, and a RED fails. The check reads the header of the RED
//! capture: its `end` against the committer time of the newest commit the
//! report lists, and its `exit`. The parent module doc states the stance of
//! every Limits list.
//!
//! Limits (every doc comment of this file that says "see the module doc's
//! limits" means this list):
//! - `natural_red::check_natural_red_labels` (HR-142) compares the `end` of a
//!   `natural` RED capture with the newest commit the report itself lists
//!   (`commits[]` plus `fix_rounds[].commits[]`), so a RED captured between two
//!   commits of one task is not caught. It compares whole seconds, because a
//!   commit time has no finer unit: a capture that ended in the newest commit's
//!   own second is not later either. A report that lists no commit, a listed
//!   sha that does not resolve, and listed commits that form no single line
//!   leave the time unchecked; the exit is still judged.
//! - The `end` comes from the clock of the machine that ran `capture`, and the
//!   commit time from the clock of the machine that committed. Clocks that
//!   differ can make a genuine `natural` RED look late. The check reads the
//!   header as it stands: it does not prove that the header is the header that
//!   `capture` wrote (see the limits of `runs`).
//! - A `natural` entry whose RED cites no capture, or a capture that cannot be
//!   read or is no capture, is silent here: check 1 of `runs` names it.

use std::path::Path;
use std::process::{Command, Stdio};

use jiff::Timestamp;
use serde_json::Value;

use crate::capture::{self, Capture};

/// A commit's committer time in the two forms the finding needs.
struct CommitTime {
    /// `%ct`: whole seconds after the Unix epoch.
    seconds: i64,
    /// `%cI`: the committer date in strict ISO 8601, exactly as git prints it.
    iso: String,
}

/// The newest commit a report lists, with the time it was committed.
struct NewestCommit {
    /// The commit's sha exactly as the report lists it.
    listed_sha: String,
    /// The commit's committer time.
    committed: CommitTime,
}

/// A `tdd` entry labelled `natural` whose RED capture could be read.
struct NaturalRed<'a> {
    /// The index of the entry in `tdd`.
    index: usize,
    /// The path of the RED capture, as the report spells it.
    path: &'a str,
    /// The RED capture.
    capture: Capture,
}

/// Flags every `tdd` entry labelled `natural` whose RED capture ended after the
/// newest commit the report lists, or exited 0 (a RED fails). The first finding
/// gives the whole seconds by which the capture is later and the commit time as
/// `git log --format=%cI` prints it; the second comes after it for an entry that
/// has both. Each case below produces no finding here, by design:
/// - a `mode` other than `natural`;
/// - a RED that cites no capture, or whose capture does not exist, cannot be
///   read, or is no capture (check 1 of `runs` reports it);
/// - a report that lists no commit, a listed sha that does not resolve (the
///   `self_audit.summary.head` check reports it once `self_audit` is filled in),
///   or listed commits that form no single line: the time stays unchecked and
///   the exit is still judged;
/// - a RED that a signal ended, which has no exit code.
///
/// Two failures are findings, not silence (`houserules.crash-paths-are-named`):
/// a capture whose `end` is no RFC 3339 time carries the parser's error text,
/// and a newest listed commit whose time `git log` cannot read carries git's
/// error text. The sha resolved a moment earlier, so that failure is git's, not
/// the report's, and no other check reports it.
pub(super) fn check_natural_red_labels(root: &Path, report: &Value, errors: &mut Vec<String>) {
    check_labels_with(root, report, &read_commit_time, errors);
}

/// `check_natural_red_labels` with the reader of the newest commit's time
/// passed in, so a test can make that read fail.
fn check_labels_with(
    root: &Path,
    report: &Value,
    read_commit_time: &dyn Fn(&Path, &str) -> Result<CommitTime, String>,
    errors: &mut Vec<String>,
) {
    let reds = natural_reds(root, report);
    if reds.is_empty() {
        return;
    }
    let newest = newest_commit(root, report, read_commit_time, errors);
    for red in &reds {
        judge_end(red, newest.as_ref(), errors);
        if red.capture.header.exit == Some(0) {
            errors.push(format!(
                "tdd[{}]: mode \"natural\", but the RED capture \"{}\" exited 0; capture the failing run, or label the entry \"mutation\" or \"reconstructed\"",
                red.index, red.path
            ));
        }
    }
}

/// Every `tdd` entry labelled `natural` whose RED cites a capture that
/// `capture::read` reads, with the capture. The path resolves as check 1
/// resolves it.
fn natural_reds<'a>(root: &Path, report: &'a Value) -> Vec<NaturalRed<'a>> {
    let Some(entries) = report.get("tdd").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            if entry.get("mode").and_then(Value::as_str) != Some("natural") {
                return None;
            }
            let path = entry.get("red")?.get("capture")?.as_str()?;
            let capture = capture::read(&super::resolve_against(root, path)).ok()?;
            Some(NaturalRed {
                index,
                path,
                capture,
            })
        })
        .collect()
}

/// The newest commit the report lists with its committer time, or `None` when
/// the report lists no commit, a listed sha does not resolve, or the listed
/// commits form no single line. A committer time that `read_commit_time` cannot
/// read is a finding, and the result is `None`.
fn newest_commit(
    root: &Path,
    report: &Value,
    read_commit_time: &dyn Fn(&Path, &str) -> Result<CommitTime, String>,
    errors: &mut Vec<String>,
) -> Option<NewestCommit> {
    let (listed_sha, full_sha) = newest_listed_commit(root, report)?;
    match read_commit_time(root, &full_sha) {
        Ok(committed) => Some(NewestCommit {
            listed_sha,
            committed,
        }),
        Err(detail) => {
            errors.push(format!(
                "a commit this report lists, \"{listed_sha}\", has a committer time that could not be read ({detail}) -- the capture times of its natural tdd entries cannot be checked against it"
            ));
            None
        }
    }
}

/// Flags `red` when its capture ended, in whole seconds, after `newest`; flags
/// an `end` that is no RFC 3339 time whether or not a commit bounds it.
fn judge_end(red: &NaturalRed, newest: Option<&NewestCommit>, errors: &mut Vec<String>) {
    let NaturalRed {
        index,
        path,
        capture,
    } = red;
    let end = match capture.header.end.parse::<Timestamp>() {
        Ok(end) => end,
        Err(error) => {
            errors.push(format!(
                "tdd[{index}]: mode \"natural\", but the RED capture \"{path}\" has an end that is no RFC 3339 time ({error})"
            ));
            return;
        }
    };
    let Some(newest) = newest else {
        return;
    };
    let later_by = end.as_second() - newest.committed.seconds;
    if later_by > 0 {
        errors.push(format!(
            "tdd[{index}]: mode \"natural\", but the RED capture \"{path}\" ended {later_by} s after the newest listed commit {} (committed {}); label the entry \"reconstructed\"",
            newest.listed_sha, newest.committed.iso,
        ));
    }
}

/// The newest commit among `report`'s `commits[]` and
/// `fix_rounds[].commits[]`, as the sha the report lists and as the full
/// sha git resolves it to, or `None` when the report lists no commit, a
/// listed sha does not resolve, or the listed commits form no single line.
fn newest_listed_commit(root: &Path, report: &Value) -> Option<(String, String)> {
    let listed = super::collect_listed_commit_shas(report);
    let listed_sha = super::newest_listed_commit(root, &listed)?;
    let full_sha = super::resolve_commit(root, &listed_sha)?;
    Some((listed_sha, full_sha))
}

/// The committer time of the commit `full_sha` names in `root`, read with
/// one `git log` call (`%ct` is "committer date, UNIX timestamp" and `%cI`
/// is "committer date, strict ISO 8601 format" in git-log's PRETTY
/// FORMATS, https://git-scm.com/docs/git-log). `Err` names the failure:
/// git's own stderr text, or the output it printed.
fn read_commit_time(root: &Path, full_sha: &str) -> Result<CommitTime, String> {
    let output = Command::new("git")
        .args(["log", "-1", "--format=%ct %cI", full_sha, "--"])
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("git log could not run: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let printed = String::from_utf8_lossy(&output.stdout);
    let printed = printed.trim();
    let (seconds, iso) = printed
        .split_once(' ')
        .ok_or_else(|| format!("git log printed \"{printed}\", not \"<seconds> <date>\""))?;
    let seconds = seconds.parse().map_err(|_| {
        format!("git log printed \"{seconds}\" for the committer time, not whole seconds")
    })?;
    Ok(CommitTime {
        seconds,
        iso: iso.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use serde_json::{Value, json};

    use super::*;
    use crate::report_claims::check_report_claims;

    /// The committer time of the one commit most tests make.
    const COMMITTED_AT: u64 = 1_700_000_000;

    /// `COMMITTED_AT` with the `+0200` offset the test repositories commit
    /// with, as `git log --format=%cI` prints it.
    const COMMITTED_AT_TEXT: &str = "2023-11-15T00:13:20+02:00";

    /// Runs `git` in `root`, panicking with its stderr on failure.
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

    /// Makes one empty commit in `root`, committed `seconds` after the
    /// epoch with a `+0200` offset (`GIT_COMMITTER_DATE`, set on the one
    /// `git commit` process only; the offset makes the `%cI` text a fixed
    /// string). Returns the short sha.
    fn commit_at(root: &Path, seconds: u64) -> String {
        let output = Command::new("git")
            .args([
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
                "commit",
            ])
            .env("GIT_COMMITTER_DATE", format!("{seconds} +0200"))
            .env("GIT_AUTHOR_DATE", format!("{seconds} +0200"))
            .current_dir(root)
            .output()
            .expect("run git commit");
        assert!(
            output.status.success(),
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        git(root, &["rev-parse", "--short", "HEAD"])
            .trim()
            .to_string()
    }

    /// A scratch repository with one empty commit per entry of
    /// `committed_at`, each committed that many seconds after the epoch
    /// (`commit_at`). Returns the short shas, oldest first.
    fn repo_with_commits(committed_at: &[u64]) -> (tempfile::TempDir, Vec<String>) {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        let shas = committed_at
            .iter()
            .map(|seconds| commit_at(dir.path(), *seconds))
            .collect();
        (dir, shas)
    }

    /// A scratch repository with two commits on separate branches off one
    /// base commit, so neither is an ancestor of the other. The first
    /// returned commit is committed at `COMMITTED_AT`, the second 200
    /// seconds later.
    fn repo_with_divergent_commits() -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        commit_at(dir.path(), COMMITTED_AT - 1_000);
        git(dir.path(), &["checkout", "-q", "-b", "branch-a"]);
        let first = commit_at(dir.path(), COMMITTED_AT);
        git(dir.path(), &["checkout", "-q", "main"]);
        git(dir.path(), &["checkout", "-q", "-b", "branch-b"]);
        let second = commit_at(dir.path(), COMMITTED_AT + 200);
        (dir, first, second)
    }

    /// The parent's `newest_listed_commit` follows the contract its doc
    /// states and this check relies on: the newest of a straight line
    /// whatever the list order; `None` for an empty list, for divergent
    /// commits, and for two distinct shas of which one does not resolve;
    /// a single sha, resolvable or not, as it stands.
    #[test]
    fn the_parent_bound_follows_its_documented_contract() {
        let bound = crate::report_claims::newest_listed_commit;
        let (line, shas) = repo_with_commits(&[COMMITTED_AT, COMMITTED_AT + 200]);
        let (older, newer) = (shas[0].clone(), shas[1].clone());
        let bogus = "deadbee1".to_string();
        assert_eq!(bound(line.path(), &[]), None);
        assert_eq!(
            bound(line.path(), &[newer.clone(), older.clone()]),
            Some(newer.clone())
        );
        assert_eq!(
            bound(line.path(), &[older.clone(), older.clone()]),
            Some(older.clone())
        );
        assert_eq!(
            bound(line.path(), std::slice::from_ref(&bogus)),
            Some(bogus.clone())
        );
        assert_eq!(bound(line.path(), &[older, bogus.clone()]), None);
        assert_eq!(bound(line.path(), &[bogus, newer]), None);
        let (divergent, first, second) = repo_with_divergent_commits();
        assert_eq!(bound(divergent.path(), &[first, second]), None);
    }

    /// Names a newest commit whose time cannot be read, instead of
    /// skipping every natural entry in silence
    /// (`houserules.crash-paths-are-named`): git read the sha a moment
    /// before, so a failure here is git's, not the report's.
    #[test]
    fn an_unreadable_commit_time_is_named_not_skipped() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(1),
            &iso(COMMITTED_AT as i64 + 3_600, 0),
        );
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        let mut errors = Vec::new();
        check_labels_with(
            dir.path(),
            &report,
            &|_root, _sha| Err("git log failed: simulated".to_string()),
            &mut errors,
        );
        assert_eq!(
            errors,
            vec![format!(
                "a commit this report lists, \"{}\", has a committer time that could not be read (git log failed: simulated) -- the capture times of its natural tdd entries cannot be checked against it",
                shas[0]
            )]
        );
    }

    /// The real reader fails with git's own error text for a commit that
    /// does not exist, and names the command that failed.
    #[test]
    fn reading_the_time_of_an_unknown_commit_carries_git_s_error() {
        let (dir, _shas) = repo_with_commits(&[COMMITTED_AT]);
        let unknown = "0".repeat(40);
        let error = read_commit_time(dir.path(), &unknown)
            .err()
            .expect("an unknown commit has no time");
        assert!(error.starts_with("git log failed: "), "got: {error}");
        assert!(error.contains(&unknown), "got: {error}");
    }

    // ---- the label by the header of the RED capture ----

    /// The RFC 3339 text of the time `seconds` and `nanos` after the Unix
    /// epoch, in the form `capture` writes it.
    fn iso(seconds: i64, nanos: i32) -> String {
        format!(
            "{:.9}",
            jiff::Timestamp::new(seconds, nanos).expect("a valid time")
        )
    }

    /// A RED capture at `rel` that exited `exit` and ended at `end`, and the
    /// `run` that cites it.
    fn red_run(root: &Path, rel: &str, exit: Option<i32>, end: &str) -> Value {
        crate::report_claims::tests::write_capture(
            root,
            rel,
            &crate::report_claims::tests::capture_header(&["cargo", "test"], exit, end),
            "FAILED\n",
        )
    }

    /// A `tdd` entry labelled `mode` with `red` as its RED and a passing
    /// capture, `green.txt`, as its GREEN.
    fn entry_with(root: &Path, mode: &str, red: Value) -> Value {
        let green = crate::report_claims::tests::write_capture(
            root,
            "green.txt",
            &crate::report_claims::tests::capture_header(
                &["cargo", "test"],
                Some(0),
                crate::report_claims::tests::EARLY_END,
            ),
            "ok\n",
        );
        json!({"test": "t", "mode": mode, "red": red, "green": green})
    }

    /// A report that lists `shas` as its commits and holds `tdd` as its `tdd`
    /// entries.
    fn listing(shas: &[String], tdd: Vec<Value>) -> Value {
        let commits: Vec<Value> = shas
            .iter()
            .map(|sha| json!({"sha": sha, "subject": "s"}))
            .collect();
        json!({"commits": commits, "fix_rounds": [], "tdd": tdd})
    }

    /// The findings of every check of `check_report_claims` over `report`,
    /// written as `report.json` in `root`.
    fn all_findings(root: &Path, report: &Value) -> Vec<String> {
        let report_path = root.join("report.json");
        std::fs::write(&report_path, report.to_string()).expect("write report");
        check_report_claims(&report_path, root).expect("report loads")
    }

    /// The finding for the `natural` entry `index` whose RED capture `path`
    /// ended `seconds_after` s after the newest listed commit `sha`, which the
    /// test repositories commit at `COMMITTED_AT`.
    fn ended_after(index: usize, path: &str, seconds_after: u64, sha: &str) -> String {
        format!(
            "tdd[{index}]: mode \"natural\", but the RED capture \"{path}\" ended {seconds_after} s after the newest listed commit {sha} (committed {COMMITTED_AT_TEXT}); label the entry \"reconstructed\""
        )
    }

    /// The finding for the `natural` entry `index` whose RED capture `path`
    /// exited 0.
    fn exited_zero(index: usize, path: &str) -> String {
        format!(
            "tdd[{index}]: mode \"natural\", but the RED capture \"{path}\" exited 0; capture the failing run, or label the entry \"mutation\" or \"reconstructed\""
        )
    }

    /// Pin: a `natural` RED that ended before the commit passes. The old
    /// check does not see the new form, so this passes at the base too.
    #[test]
    fn a_natural_red_that_ended_before_the_commit_passes() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(101),
            &iso(COMMITTED_AT as i64 - 1_000, 0),
        );
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(all_findings(dir.path(), &report), Vec::<String>::new());
    }

    /// A `natural` RED whose header says it ended after the newest commit
    /// fails, with the whole line pinned.
    #[test]
    fn a_natural_red_that_ended_after_the_newest_commit_fails() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(101),
            &iso(COMMITTED_AT as i64 + 3_600, 0),
        );
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(
            all_findings(dir.path(), &report),
            [format!(
                "tdd[0]: mode \"natural\", but the RED capture \"red.txt\" ended 3600 s after the newest listed commit {} (committed 2023-11-15T00:13:20+02:00); label the entry \"reconstructed\"",
                shas[0]
            )]
        );
    }

    /// The comparison is in whole seconds, because a commit time has no finer
    /// unit: a RED that ended in the second of the commit is not later, and
    /// one that ended in the next second is.
    #[test]
    fn a_red_in_the_second_of_the_commit_is_not_later() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let same_second = red_run(
            dir.path(),
            "same-second.txt",
            Some(1),
            &iso(COMMITTED_AT as i64, 900_000_000),
        );
        let next_second = red_run(
            dir.path(),
            "next-second.txt",
            Some(1),
            &iso(COMMITTED_AT as i64 + 1, 0),
        );
        let report = listing(
            &shas,
            vec![
                entry_with(dir.path(), "natural", same_second),
                entry_with(dir.path(), "natural", next_second),
            ],
        );
        assert_eq!(
            all_findings(dir.path(), &report),
            [ended_after(1, "next-second.txt", 1, &shas[0])]
        );
    }

    /// The bound is the newest listed commit: a RED between two listed
    /// commits passes. This pins the limit that the module doc names.
    #[test]
    fn a_red_between_two_listed_commits_passes() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT, COMMITTED_AT + 7_200]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(1),
            &iso(COMMITTED_AT as i64 + 3_600, 0),
        );
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(all_findings(dir.path(), &report), Vec::<String>::new());
    }

    /// A `natural` RED whose header says it exited 0 fails: a RED fails.
    #[test]
    fn a_natural_red_that_exited_0_fails() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(0),
            &iso(COMMITTED_AT as i64 - 1_000, 0),
        );
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(
            all_findings(dir.path(), &report),
            [exited_zero(0, "red.txt")]
        );
    }

    /// Pin: a command that a signal ended has no exit code, so it is no exit
    /// 0.
    #[test]
    fn a_natural_red_that_a_signal_ended_is_no_exit_0() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            None,
            &iso(COMMITTED_AT as i64 - 1_000, 0),
        );
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(all_findings(dir.path(), &report), Vec::<String>::new());
    }

    /// A `mutation` or `reconstructed` entry passes whatever its RED ran and
    /// ended at. The `natural` entry after them is the control: it still
    /// fails, so the test cannot pass because the check did nothing.
    #[test]
    fn a_mutation_or_reconstructed_red_that_exited_0_passes() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let late = iso(COMMITTED_AT as i64 + 3_600, 0);
        let report = listing(
            &shas,
            vec![
                entry_with(
                    dir.path(),
                    "mutation",
                    red_run(dir.path(), "mutation.txt", Some(0), &late),
                ),
                entry_with(
                    dir.path(),
                    "reconstructed",
                    red_run(dir.path(), "reconstructed.txt", Some(0), &late),
                ),
                entry_with(
                    dir.path(),
                    "natural",
                    red_run(dir.path(), "natural.txt", Some(0), &late),
                ),
            ],
        );
        assert_eq!(
            all_findings(dir.path(), &report),
            [
                ended_after(2, "natural.txt", 3_600, &shas[0]),
                exited_zero(2, "natural.txt")
            ]
        );
    }

    /// One entry that is late and exited 0 gets both lines, the line of the
    /// time first, as the spec lists them.
    #[test]
    fn both_findings_of_one_entry_come_in_the_order_of_the_spec() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(0),
            &iso(COMMITTED_AT as i64 + 60, 0),
        );
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(
            all_findings(dir.path(), &report),
            [
                ended_after(0, "red.txt", 60, &shas[0]),
                exited_zero(0, "red.txt")
            ]
        );
    }

    /// The check judges a capture that git tracks like any other: the time
    /// is the header's, not the file's.
    #[test]
    fn a_tracked_capture_is_judged_like_any_other() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(1),
            &iso(COMMITTED_AT as i64 + 3_600, 0),
        );
        git(dir.path(), &["add", "red.txt"]);
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(
            all_findings(dir.path(), &report),
            [ended_after(0, "red.txt", 3_600, &shas[0])]
        );
    }

    /// An `end` that is no RFC 3339 time is named, with the error of the
    /// parser, instead of being read as a default
    /// (`houserules.crash-paths-are-named`).
    #[test]
    fn an_end_time_that_cannot_be_read_is_named() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(dir.path(), "red.txt", Some(1), "yesterday");
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", red)]);
        let error = "yesterday"
            .parse::<jiff::Timestamp>()
            .expect_err("not a time");
        assert_eq!(
            all_findings(dir.path(), &report),
            [format!(
                "tdd[0]: mode \"natural\", but the RED capture \"red.txt\" has an end that is no RFC 3339 time ({error})"
            )]
        );
    }

    /// A report that lists no commit has nothing to compare the time with, but
    /// the exit of a `natural` RED still counts.
    #[test]
    fn a_report_without_commits_still_judges_the_exit() {
        let (dir, _shas) = repo_with_commits(&[COMMITTED_AT]);
        let red = red_run(
            dir.path(),
            "red.txt",
            Some(0),
            &iso(COMMITTED_AT as i64 + 3_600, 0),
        );
        let report = listing(&[], vec![entry_with(dir.path(), "natural", red)]);
        assert_eq!(
            all_findings(dir.path(), &report),
            [exited_zero(0, "red.txt")]
        );
    }

    /// A listed sha that does not resolve, and listed commits that form no
    /// single line, leave the time unchecked (the self_audit head check names
    /// the first); the exit still counts.
    #[test]
    fn an_unresolvable_or_divergent_bound_leaves_the_time_unchecked() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let late = iso(COMMITTED_AT as i64 + 3_600, 0);
        let entries = || {
            vec![entry_with(
                dir.path(),
                "natural",
                red_run(dir.path(), "red.txt", Some(0), &late),
            )]
        };
        let bogus = "deadbee1".to_string();
        assert_eq!(
            all_findings(
                dir.path(),
                &listing(std::slice::from_ref(&bogus), entries())
            ),
            [exited_zero(0, "red.txt")]
        );
        assert_eq!(
            all_findings(dir.path(), &listing(&[shas[0].clone(), bogus], entries())),
            [exited_zero(0, "red.txt")]
        );
        let (divergent, first, second) = repo_with_divergent_commits();
        let divergent_entries = vec![entry_with(
            divergent.path(),
            "natural",
            red_run(divergent.path(), "red.txt", Some(0), &late),
        )];
        assert_eq!(
            all_findings(
                divergent.path(),
                &listing(&[first, second], divergent_entries)
            ),
            [exited_zero(0, "red.txt")]
        );
    }

    /// A `natural` entry whose RED cites no capture, or one that cannot be
    /// read, gets only the line of check 1.
    #[test]
    fn a_natural_red_without_a_readable_capture_adds_no_line_of_its_own() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let old_form = json!({"command": "cargo test", "output": "FAILED\n", "exit": 101});
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", old_form)]);
        assert_eq!(
            all_findings(dir.path(), &report),
            ["tdd[0].red: no capture; run the command through houserules capture"]
        );
        let missing = json!({
            "capture": "gone.txt",
            "sha256": "0".repeat(64),
            "command": "cargo test",
            "exit": 101,
        });
        let report = listing(&shas, vec![entry_with(dir.path(), "natural", missing)]);
        assert_eq!(
            all_findings(dir.path(), &report),
            ["tdd[0].red: capture gone.txt does not exist"]
        );
    }
}
