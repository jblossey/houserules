//! The `natural`-label check of `check-report-claims` (HR-142). A `tdd`
//! entry is labelled `natural` only when its RED ran before the commit
//! that makes it green; this check proves the common failure of that
//! label from file times: a RED capture file modified after the newest
//! commit the report lists. The parent module doc's Limits list names
//! what the check does not see.

use std::path::{Component, Path};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// A commit's committer time in the two forms the finding needs.
struct CommitTime {
    /// `%ct`: whole seconds after the Unix epoch.
    seconds: u64,
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

/// Flags every `tdd` entry labelled `natural` whose RED capture file was
/// modified after the newest commit the report lists. The finding gives
/// the whole seconds by which the capture is newer and the commit time as
/// `git log --format=%cI` prints it. Each case below produces no finding
/// here, by design:
/// - a `mode` other than `natural`;
/// - a `red.command` without a plain `> <file> 2>&1` redirect at its end
///   (no redirect, an append, a redirect followed by more command);
/// - a capture file that does not exist (the redirect capture check
///   reports it);
/// - a report that lists no commit, a listed sha that does not resolve
///   (the `self_audit.summary.head` check reports it once `self_audit` is
///   filled in), or listed commits that form no single line;
/// - a capture file git tracks, whose modification time is the
///   checkout's, not the capture's (`tracked_files` is `git ls-files`
///   in `root`);
/// - a capture file dated before the Unix epoch, older than any commit.
///
/// Two failures are findings, not silence (`houserules.crash-paths-are-named`):
/// a capture whose modification time cannot be read for any reason other
/// than absence carries the operating system's own error text, and a
/// newest listed commit whose time `git log` cannot read carries git's
/// error text. The sha resolved a moment earlier, so that failure is
/// git's, not the report's, and no other check reports it.
pub(super) fn check_natural_red_labels(
    root: &Path,
    report: &Value,
    tracked_files: &[String],
    errors: &mut Vec<String>,
) {
    check_labels_with(root, report, tracked_files, &read_commit_time, errors);
}

/// `check_natural_red_labels` with the reader of the newest commit's time
/// passed in, so a test can make that read fail.
fn check_labels_with(
    root: &Path,
    report: &Value,
    tracked_files: &[String],
    read_commit_time: &dyn Fn(&Path, &str) -> Result<CommitTime, String>,
    errors: &mut Vec<String>,
) {
    let redirect = super::redirect_capture_pattern();
    let captures = natural_red_captures(report, &redirect);
    if captures.is_empty() {
        return;
    }
    let Some((listed_sha, full_sha)) = newest_listed_commit(root, report) else {
        return;
    };
    let committed = match read_commit_time(root, &full_sha) {
        Ok(committed) => committed,
        Err(detail) => {
            errors.push(format!(
                "a commit this report lists, \"{listed_sha}\", has a committer time that could not be read ({detail}) -- the capture times of its natural tdd entries cannot be checked against it"
            ));
            return;
        }
    };
    let newest = NewestCommit {
        listed_sha,
        committed,
    };
    for (index, path) in captures {
        let target = super::resolve_against(root, path);
        if is_tracked(root, &target, tracked_files) {
            continue;
        }
        let modified = match std::fs::metadata(&target).and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                errors.push(format!(
                    "tdd[{index}]: mode \"natural\", but the RED capture \"{path}\" has a modification time that could not be read ({error})"
                ));
                continue;
            }
        };
        let Some(modified_at) = unix_seconds(modified) else {
            continue;
        };
        if modified_at > newest.committed.seconds {
            errors.push(format!(
                "tdd[{index}]: mode \"natural\", but the RED capture \"{path}\" was modified {} s after the newest listed commit {} (committed {}); label the entry \"reconstructed\"",
                modified_at - newest.committed.seconds,
                newest.listed_sha,
                newest.committed.iso,
            ));
        }
    }
}

/// The index and RED capture path of every `tdd` entry labelled `natural`
/// whose `red.command` ends in a plain `> <file> 2>&1` redirect, the path
/// as the command spells it.
fn natural_red_captures<'a>(report: &'a Value, redirect: &regress::Regex) -> Vec<(usize, &'a str)> {
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
            let command = entry.get("red")?.get("command")?.as_str()?;
            let capture = super::redirect_capture(redirect, command)?;
            (!capture.appends).then_some((index, capture.path))
        })
        .collect()
}

/// `true` when `target` (`root` joined with a capture path) names a file
/// in `tracked_files`, git's own `/`-separated paths relative to `root`.
/// The comparison is lexical: `Path::components` already drops a `.`
/// segment after the first (https://doc.rust-lang.org/std/path/struct.Path.html#method.components),
/// and a path with a `..` segment, or outside `root`, is never recognized
/// as tracked.
fn is_tracked(root: &Path, target: &Path, tracked_files: &[String]) -> bool {
    let Ok(relative) = target.strip_prefix(root) else {
        return false;
    };
    let mut segments = Vec::new();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return false;
        };
        segments.push(segment.to_string_lossy());
    }
    let relative = segments.join("/");
    tracked_files.contains(&relative)
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

/// `time` in whole seconds after the Unix epoch, or `None` for a time
/// before it.
fn unix_seconds(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .map(|since_epoch| since_epoch.as_secs())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde_json::{Value, json};

    use super::*;
    use crate::report_claims::{
        check_report_claims, list_tracked_files, redirect_capture, redirect_capture_pattern,
    };

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

    /// Writes `name` under `root` and sets its modification time to
    /// `modified_at`, through the standard library only.
    fn write_capture_at(root: &Path, name: &str, modified_at: SystemTime) {
        let path = root.join(name);
        std::fs::write(&path, "RED output\n").expect("write capture");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("open capture")
            .set_modified(modified_at)
            .expect("set capture time");
    }

    /// `write_capture_at` with a time `modified_at` after the epoch.
    fn write_capture(root: &Path, name: &str, modified_at: Duration) {
        write_capture_at(root, name, UNIX_EPOCH + modified_at);
    }

    /// A `tdd` entry labelled `mode` whose RED ran `command`.
    fn tdd_entry(mode: &str, command: &str) -> Value {
        json!({
            "test": "t",
            "mode": mode,
            "red": {"command": command, "output": ""},
            "green": {"command": "x", "output": ""},
        })
    }

    /// A report listing `shas` as its commits, one per `commits[]` entry,
    /// with `tdd` as its `tdd` entries.
    fn report_with(shas: &[String], tdd: Vec<Value>) -> Value {
        let commits: Vec<Value> = shas
            .iter()
            .map(|sha| json!({"sha": sha, "subject": "s"}))
            .collect();
        json!({"commits": commits, "fix_rounds": [], "tdd": tdd})
    }

    /// Runs the check the way `check_report_claims` does: the tracked-file
    /// list comes from `git ls-files` in `root`.
    fn findings(root: &Path, report: &Value) -> Vec<String> {
        let tracked_files = list_tracked_files(root).unwrap_or_default();
        let mut errors = Vec::new();
        check_natural_red_labels(root, report, &tracked_files, &mut errors);
        errors
    }

    /// The finding for a `natural` entry at `index` whose capture `path`
    /// was modified `seconds_after` s after the newest listed commit
    /// `sha`, which the test repositories commit at `COMMITTED_AT`.
    fn finding(index: usize, path: &str, seconds_after: u64, sha: &str) -> String {
        format!(
            "tdd[{index}]: mode \"natural\", but the RED capture \"{path}\" was modified {seconds_after} s after the newest listed commit {sha} (committed {COMMITTED_AT_TEXT}); label the entry \"reconstructed\""
        )
    }

    /// The plain redirect form the checker recognizes as a capture.
    const CAPTURE_COMMAND: &str = "cargo test > capture.txt 2>&1";

    /// Passes a `natural` entry whose capture is older than the commit.
    #[test]
    fn natural_red_older_than_the_commit_passes() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT - 1_000),
        );
        let report = report_with(&shas, vec![tdd_entry("natural", CAPTURE_COMMAND)]);
        assert_eq!(findings(dir.path(), &report), Vec::<String>::new());
    }

    /// Flags a `natural` entry whose capture is newer than the newest
    /// commit, with the whole finding line pinned.
    #[test]
    fn natural_red_newer_than_the_newest_commit_fails() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(&shas, vec![tdd_entry("natural", CAPTURE_COMMAND)]);
        assert_eq!(
            findings(dir.path(), &report),
            vec![format!(
                "tdd[0]: mode \"natural\", but the RED capture \"capture.txt\" was modified 3600 s after the newest listed commit {} (committed 2023-11-15T00:13:20+02:00); label the entry \"reconstructed\"",
                shas[0]
            )]
        );
    }

    /// Ignores `reconstructed` and `mutation` entries, however new their
    /// captures. The `natural` entry after them is the control: it still
    /// fires, so the test cannot pass because the check did nothing.
    #[test]
    fn reconstructed_and_mutation_entries_are_ignored() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(
            &shas,
            vec![
                tdd_entry("reconstructed", CAPTURE_COMMAND),
                tdd_entry("mutation", CAPTURE_COMMAND),
                tdd_entry("natural", CAPTURE_COMMAND),
            ],
        );
        assert_eq!(
            findings(dir.path(), &report),
            vec![finding(2, "capture.txt", 3_600, &shas[0])]
        );
    }

    /// Stays silent on a RED command with no redirect.
    #[test]
    fn a_red_without_a_redirect_is_silent() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(
            &shas,
            vec![
                tdd_entry("natural", "cargo test"),
                tdd_entry("natural", "cargo test > capture.txt"),
                tdd_entry("natural", "cargo test > capture.txt 2>&1 && true"),
                tdd_entry("natural", CAPTURE_COMMAND),
            ],
        );
        assert_eq!(
            findings(dir.path(), &report),
            vec![finding(3, "capture.txt", 3_600, &shas[0])]
        );
    }

    /// Stays silent on an append redirect (`>>`, `2>>`): the file then
    /// holds the output of every earlier run too.
    #[test]
    fn an_append_redirect_is_silent() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(
            &shas,
            vec![
                tdd_entry("natural", "cargo test >> capture.txt 2>&1"),
                tdd_entry("natural", "cargo test 2>> capture.txt 2>&1"),
                tdd_entry("natural", CAPTURE_COMMAND),
            ],
        );
        assert_eq!(
            findings(dir.path(), &report),
            vec![finding(2, "capture.txt", 3_600, &shas[0])]
        );
    }

    /// Stays silent on a capture file that does not exist: the redirect
    /// capture check reports it.
    #[test]
    fn a_missing_capture_file_is_silent() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(
            &shas,
            vec![
                tdd_entry("natural", "cargo test > missing.txt 2>&1"),
                tdd_entry("natural", CAPTURE_COMMAND),
            ],
        );
        assert_eq!(
            findings(dir.path(), &report),
            vec![finding(1, "capture.txt", 3_600, &shas[0])]
        );
    }

    /// Stays silent on a report that lists no commit: there is nothing to
    /// compare the capture with.
    #[test]
    fn a_report_without_commits_is_silent() {
        let (dir, _shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(&[], vec![tdd_entry("natural", CAPTURE_COMMAND)]);
        assert_eq!(findings(dir.path(), &report), Vec::<String>::new());
    }

    /// Stays silent when a listed sha does not resolve (alone, or beside a
    /// real one): the self_audit head check reports it.
    #[test]
    fn an_unresolvable_sha_is_silent() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let bogus = "deadbee1".to_string();
        let tdd = vec![tdd_entry("natural", CAPTURE_COMMAND)];
        let alone = report_with(std::slice::from_ref(&bogus), tdd.clone());
        let beside_a_real_one = report_with(&[shas[0].clone(), bogus], tdd);
        assert_eq!(findings(dir.path(), &alone), Vec::<String>::new());
        assert_eq!(
            findings(dir.path(), &beside_a_real_one),
            Vec::<String>::new()
        );
    }

    /// Stays silent on a capture file git tracks: its modification time
    /// is the checkout's, not the capture's. The untracked capture after
    /// it is the control.
    #[test]
    fn a_tracked_capture_file_is_silent() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "tracked.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        git(dir.path(), &["add", "tracked.txt"]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(
            &shas,
            vec![
                tdd_entry("natural", "cargo test > tracked.txt 2>&1"),
                tdd_entry("natural", "cargo test > ./tracked.txt 2>&1"),
                tdd_entry("natural", CAPTURE_COMMAND),
            ],
        );
        assert_eq!(
            findings(dir.path(), &report),
            vec![finding(2, "capture.txt", 3_600, &shas[0])]
        );
    }

    /// The bound is the newest listed commit: a capture between two
    /// listed commits passes. This pins the limit the parent module doc
    /// names.
    #[test]
    fn the_newest_of_several_listed_commits_is_the_bound() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT, COMMITTED_AT + 7_200]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(&shas, vec![tdd_entry("natural", CAPTURE_COMMAND)]);
        assert_eq!(findings(dir.path(), &report), Vec::<String>::new());
    }

    /// Compares whole seconds, because a commit time has no finer unit: a
    /// capture in the commit's own second is not newer, and one in the
    /// next second is.
    #[test]
    fn a_capture_in_the_commit_second_is_not_newer() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_millis(COMMITTED_AT * 1_000 + 900),
        );
        write_capture(
            dir.path(),
            "next-second.txt",
            Duration::from_millis(COMMITTED_AT * 1_000 + 1_000),
        );
        let report = report_with(
            &shas,
            vec![
                tdd_entry("natural", CAPTURE_COMMAND),
                tdd_entry("natural", "cargo test > next-second.txt 2>&1"),
            ],
        );
        assert_eq!(
            findings(dir.path(), &report),
            vec![format!(
                "tdd[1]: mode \"natural\", but the RED capture \"next-second.txt\" was modified 1 s after the newest listed commit {} (committed 2023-11-15T00:13:20+02:00); label the entry \"reconstructed\"",
                shas[0]
            )]
        );
    }

    /// Names a capture whose modification time cannot be read for a
    /// reason other than absence: the operating system's own error text,
    /// never a silent skip (`houserules.crash-paths-are-named`). `stat` on
    /// a path below a regular file fails with `ENOTDIR` (Linux `stat(2)`,
    /// "A component of the path prefix ... is not a directory"); the test
    /// builds the expected text from that code, not from one platform's
    /// wording.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_capture_time_is_named_not_skipped() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(dir.path(), "plain.txt", Duration::from_secs(COMMITTED_AT));
        let report = report_with(
            &shas,
            vec![tdd_entry(
                "natural",
                "cargo test > plain.txt/inner.txt 2>&1",
            )],
        );
        let os_error = std::io::Error::from_raw_os_error(libc::ENOTDIR);
        assert_eq!(
            findings(dir.path(), &report),
            vec![format!(
                "tdd[0]: mode \"natural\", but the RED capture \"plain.txt/inner.txt\" has a modification time that could not be read ({os_error})"
            )]
        );
    }

    /// Runs from `check_report_claims`: a report file checked end to end
    /// carries the finding, so deleting the wiring call fails a test.
    #[test]
    fn the_check_runs_from_check_report_claims() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let mut entry = tdd_entry("natural", CAPTURE_COMMAND);
        entry["red"]["output"] = json!("RED output\n");
        let report = report_with(&shas, vec![entry]);
        let report_path = dir.path().join("report.json");
        std::fs::write(&report_path, report.to_string()).expect("write report");
        assert_eq!(
            check_report_claims(&report_path, dir.path()).expect("report loads"),
            vec![finding(0, "capture.txt", 3_600, &shas[0])]
        );
    }

    /// The shared redirect parser reads an append (`>>`, `2>>`) from the
    /// pattern's second `>` and marks it; a plain `>` is not marked.
    #[test]
    fn the_shared_redirect_parser_marks_an_append() {
        let pattern = redirect_capture_pattern();
        for (command, appends) in [
            ("cargo test > capture.txt 2>&1", false),
            ("cargo test >> capture.txt 2>&1", true),
            ("cargo test 2>> capture.txt 2>&1", true),
        ] {
            let capture = redirect_capture(&pattern, command).expect("a capture redirect");
            assert_eq!(
                (capture.path, capture.appends),
                ("capture.txt", appends),
                "{command}"
            );
        }
    }

    /// Stays silent when the listed commits form no single line (two
    /// commits on separate branches): no commit is the newest. The same
    /// capture beside only one of them is the control: it fires.
    #[test]
    fn a_report_whose_commits_form_no_single_line_is_silent() {
        let (dir, first, second) = repo_with_divergent_commits();
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let entries = vec![tdd_entry("natural", CAPTURE_COMMAND)];
        let both = report_with(&[first.clone(), second], entries.clone());
        let only_the_first = report_with(std::slice::from_ref(&first), entries);
        assert_eq!(findings(dir.path(), &both), Vec::<String>::new());
        assert_eq!(
            findings(dir.path(), &only_the_first),
            vec![finding(0, "capture.txt", 3_600, &first)]
        );
    }

    /// Stays silent on a capture dated before the Unix epoch, which is
    /// older than any commit; the capture after it is the control. The
    /// assert names a platform whose file system cannot store such a time
    /// (verified on Linux; the other CI platforms are judged by CI).
    #[test]
    fn a_capture_dated_before_the_epoch_is_silent() {
        let (dir, shas) = repo_with_commits(&[COMMITTED_AT]);
        let before_the_epoch = UNIX_EPOCH - Duration::from_secs(86_400);
        write_capture_at(dir.path(), "before-epoch.txt", before_the_epoch);
        let stored = std::fs::metadata(dir.path().join("before-epoch.txt"))
            .and_then(|metadata| metadata.modified())
            .expect("read the capture time back");
        assert!(
            stored < UNIX_EPOCH,
            "this platform's file system did not store a time before the epoch: {stored:?}"
        );
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(
            &shas,
            vec![
                tdd_entry("natural", "cargo test > before-epoch.txt 2>&1"),
                tdd_entry("natural", CAPTURE_COMMAND),
            ],
        );
        assert_eq!(
            findings(dir.path(), &report),
            vec![finding(1, "capture.txt", 3_600, &shas[0])]
        );
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
        write_capture(
            dir.path(),
            "capture.txt",
            Duration::from_secs(COMMITTED_AT + 3_600),
        );
        let report = report_with(&shas, vec![tdd_entry("natural", CAPTURE_COMMAND)]);
        let mut errors = Vec::new();
        check_labels_with(
            dir.path(),
            &report,
            &[],
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
}
