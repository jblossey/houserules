//! The records of an install in a range, newest first (spec 8.3).
//!
//! `record list` and `stats --records` read the records the same way: every note
//! of the install's notes ref, the committer time of its key, and the keys that
//! the range holds. A search costs a fixed number of git calls whatever the
//! number of records: one `git notes list`, one `git rev-list` for the times, and
//! one `git cat-file --batch` for the notes that stay. A search with a range adds
//! one `git rev-parse` that finds the shallow file, and a range that `git
//! rev-list` refuses in a shallow clone adds one `git cat-file --batch-check`
//! that resolves its ends.
//!
//! A record is in a range when its key is. Without a range every record is in.
//! The key of a record whose commit the clone lacks has no committer time: the
//! record comes after the dated records, and no range holds it.
//!
//! A shallow clone cannot tell which records its missing history holds, so it
//! counts none short in silence. A range reaches past the history of the clone
//! ([`past_the_history_line`]) when the commits of either side that the other
//! side does not reach (the symmetric difference of its two ends; for a range of
//! one revision, the commits it reaches) include a shallow commit, when an end is
//! a full object name that the clone lacks, or when an end is a relative revision
//! (`<rev>~<n>`, `<rev>^<n>`) that does not resolve while its `<rev>` does. Any
//! other end that git cannot resolve, an abbreviated object name included, is
//! `bad range`, because it cannot be told from a typo.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use super::super::git;
use super::notes::{self, NoteName};
use crate::published::change_record::ChangeRecord;

/// A record of the notes ref with the commit that it annotates.
#[derive(Debug)]
pub(super) struct Entry {
    /// The full sha of the commit that the note annotates.
    pub(super) key: String,
    /// The committer time of the key in seconds, or `None` when the clone lacks
    /// the commit.
    pub(super) committed: Option<i64>,
    /// The record that the note holds.
    pub(super) record: ChangeRecord,
}

/// The committer times in the output of `git rev-list --timestamp`: one `<time>
/// <sha>` line for each commit.
fn times_in(listing: &str) -> HashMap<String, i64> {
    listing
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter_map(|(time, sha)| Some((sha.to_string(), time.parse().ok()?)))
        .collect()
}

/// The line for a range that a shallow clone cannot walk to its end (spec 8.3).
fn past_the_history_line(range: &str) -> String {
    format!(
        "record: {range} reaches past the history of this clone; fetch the full history to read its records"
    )
}

/// The commits at which the history of the clone at `root` stops: the lines of
/// the file `shallow` in its git directory. `None` for a clone with the full
/// history, which has no such file.
fn shallow_commits(root: &Path) -> Result<Option<HashSet<String>>, String> {
    let located = git::run(root, &["rev-parse", "--git-path", "shallow"])?;
    let path = root.join(located.trim());
    match fs::read_to_string(&path) {
        Ok(text) => Ok(Some(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect(),
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// The shape of a revision range as `git rev-list` reads it. Git splits a range at
/// its first `..`; an end that is left out is `HEAD`.
enum Shape<'a> {
    /// One revision: the commits that it reaches.
    Revision(&'a str),
    /// `left..right`: the commits that `right` reaches and `left` does not.
    Difference { left: &'a str, right: &'a str },
    /// `left...right`: the commits that one end reaches and the other does not.
    Symmetric { left: &'a str, right: &'a str },
}

impl<'a> Shape<'a> {
    /// The shape of `range`.
    fn of(range: &'a str) -> Self {
        match range.find("..") {
            Some(at) => match range[at + 2..].strip_prefix('.') {
                Some(right) => Shape::Symmetric {
                    left: &range[..at],
                    right,
                },
                None => Shape::Difference {
                    left: &range[..at],
                    right: &range[at + 2..],
                },
            },
            None => Shape::Revision(range),
        }
    }

    /// The revisions that the range names, an end that is left out not among them.
    fn ends(&self) -> Vec<&'a str> {
        match *self {
            Shape::Revision(revision) => vec![revision],
            Shape::Difference { left, right } | Shape::Symmetric { left, right } => [left, right]
                .into_iter()
                .filter(|end| !end.is_empty())
                .collect(),
        }
    }

    /// The spelling that lists the commits of both sides: a symmetric difference
    /// names the same two ends as a range of two dots.
    fn both_sides(&self, range: &str) -> String {
        match *self {
            Shape::Difference { left, right } => format!("{left}...{right}"),
            _ => range.to_string(),
        }
    }
}

/// The commits that `git rev-list --left-right --timestamp` lists, as the
/// committer time, whether the commit is on the right side, and the sha. A range
/// of two dots lists the commits of its right end on the right side.
fn sides_in(listing: &str) -> Vec<(i64, bool, &str)> {
    listing
        .lines()
        .filter_map(|line| {
            let (time, marked) = line.split_once(' ')?;
            let (right, sha) = match marked.strip_prefix('<') {
                Some(sha) => (false, sha),
                None => (true, marked.strip_prefix('>').unwrap_or(marked)),
            };
            Some((time.parse().ok()?, right, sha))
        })
        .collect()
}

/// Whether `name` is a full object name: `hex_length` hexadecimal digits.
fn is_full_object_name(name: &str, hex_length: usize) -> bool {
    name.len() == hex_length && name.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The revision that the relative revision `end` counts from: `end` without its
/// last step, `~<n>` or `^<n>` (`~` and `^` alone count one). `None` for an end
/// that has no such step.
fn counted_from(end: &str) -> Option<&str> {
    let step = end.rfind(['~', '^'])?;
    let counts = end[step + 1..].bytes().all(|byte| byte.is_ascii_digit());
    (step > 0 && counts).then(|| &end[..step])
}

/// Whether the ends of `shape`, which `git rev-list` refused, are ends that a
/// shallow clone lacks: each end that git cannot resolve is a full object name,
/// or a relative revision whose `<rev>` resolves, and there is such an end. An
/// end of another kind that git cannot resolve (an abbreviation, a typo) makes
/// the range unreadable whatever its other end. One `git cat-file
/// --batch-check` resolves the ends and the revisions they count from.
fn ends_the_clone_lacks(
    root: &Path,
    shape: &Shape,
    full_name_length: usize,
) -> Result<bool, String> {
    let ends = shape.ends();
    let names: Vec<&str> = ends
        .iter()
        .flat_map(|end| std::iter::once(*end).chain(counted_from(end)))
        .collect();
    if names.is_empty() || names.iter().any(|name| name.contains('\n')) {
        return Ok(false);
    }
    let input = format!("{}\n", names.join("\n"));
    let answers = git::run_with_input(root, &["cat-file", "--batch-check"], input.as_bytes())?;
    let mut resolves = answers
        .lines()
        .map(|line| !line.ends_with(" missing") && !line.ends_with(" ambiguous"));
    let mut lacking = false;
    for end in ends {
        let end_resolves = resolves.next().unwrap_or(false);
        let revision_resolves = counted_from(end).map(|_| resolves.next().unwrap_or(false));
        if end_resolves {
            continue;
        }
        if is_full_object_name(end, full_name_length) || revision_resolves == Some(true) {
            lacking = true;
        } else {
            return Ok(false);
        }
    }
    Ok(lacking)
}

/// The committer time of each commit of `range` in a clone whose history stops
/// at the `shallow` commits. It lists the symmetric difference of the two ends,
/// because a shallow commit on either side cuts the walk of that side: a
/// shallow commit among the listed commits, and an end that the clone lacks, are
/// [`past_the_history_line`].
fn shallow_ranged_times(
    root: &Path,
    range: &str,
    shallow: &HashSet<String>,
) -> Result<HashMap<String, i64>, String> {
    let shape = Shape::of(range);
    let listing = git::run(
        root,
        &[
            "rev-list",
            "--left-right",
            "--timestamp",
            "--end-of-options",
            &shape.both_sides(range),
            "--",
        ],
    );
    match listing {
        Ok(listing) => {
            let sides = sides_in(&listing);
            if sides.iter().any(|(_, _, sha)| shallow.contains(*sha)) {
                return Err(past_the_history_line(range));
            }
            let right_side_only = matches!(shape, Shape::Difference { .. });
            Ok(sides
                .into_iter()
                .filter(|(_, right, _)| *right || !right_side_only)
                .map(|(time, _, sha)| (sha.to_string(), time))
                .collect())
        }
        Err(_) => {
            let full_name_length = shallow.iter().next().map_or(40, String::len);
            if ends_the_clone_lacks(root, &shape, full_name_length)? {
                Err(past_the_history_line(range))
            } else {
                Err(bad_range_line(range))
            }
        }
    }
}

/// The line for a range that `git rev-list` does not read.
fn bad_range_line(range: &str) -> String {
    format!("bad range \"{range}\"")
}

/// The committer time of each commit of `range`. In a shallow clone, a range
/// that reaches past the history of the clone is [`past_the_history_line`].
/// `bad range "<range>"` when git does not read the range.
fn ranged_times(root: &Path, range: &str) -> Result<HashMap<String, i64>, String> {
    match shallow_commits(root)? {
        Some(shallow) => shallow_ranged_times(root, range, &shallow),
        None => git::run(
            root,
            &["rev-list", "--timestamp", "--end-of-options", range, "--"],
        )
        .map(|listing| times_in(&listing))
        .map_err(|_| bad_range_line(range)),
    }
}

/// The committer time of each commit of `range`, or of each key of `names` that
/// the clone has when there is no range. A range that git does not read, or that
/// a shallow clone cannot walk to its end, is the line of [`ranged_times`].
fn committer_times(
    root: &Path,
    range: Option<&str>,
    names: &[NoteName],
) -> Result<HashMap<String, i64>, String> {
    match range {
        Some(range) => ranged_times(root, range),
        None if names.is_empty() => Ok(HashMap::new()),
        None => {
            let input: String = names.iter().map(|name| format!("{}\n", name.key)).collect();
            git::run_with_input(
                root,
                &[
                    "rev-list",
                    "--no-walk=unsorted",
                    "--ignore-missing",
                    "--stdin",
                    "--timestamp",
                ],
                input.as_bytes(),
            )
            .map(|listing| times_in(&listing))
        }
    }
}

/// The records of the notes ref of `install` whose key is in `range` (every
/// record without a range), newest first by the committer time of the key, a tie
/// by sha, and the records of a key that the clone lacks last. The notes ref must
/// exist. A range that git does not read is `bad range "<range>"`, and a range
/// that a shallow clone cannot walk to its end is [`past_the_history_line`].
pub(super) fn records(
    root: &Path,
    install: &str,
    range: Option<&str>,
) -> Result<Vec<Entry>, String> {
    let names = notes::names(root, install)?;
    let times = committer_times(root, range, &names)?;
    let kept: Vec<NoteName> = names
        .into_iter()
        .filter(|name| range.is_none() || times.contains_key(&name.key))
        .collect();
    let records = notes::read_named(root, &kept)?;
    let mut entries: Vec<Entry> = kept
        .into_iter()
        .zip(records)
        .map(|(name, record)| Entry {
            committed: times.get(&name.key).copied(),
            key: name.key,
            record,
        })
        .collect();
    entries.sort_by(|a, b| (Reverse(a.committed), &a.key).cmp(&(Reverse(b.committed), &b.key)));
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::super::document;
    use super::super::fixtures::{minimal, parsed};
    use super::*;
    use crate::review::git::scratch::Repo;

    /// Commits the working tree of `repo` with the committer time `seconds` and
    /// an author time that no commit shares, and gives the full sha.
    fn commit_at(repo: &Repo, name: &str, seconds: u64) -> String {
        repo.write("file.txt", &format!("{name}\n"));
        repo.git(&["add", "-A"]);
        repo.git_with_env(
            &["commit", "-q", "--allow-empty", "-m", name],
            &[
                ("GIT_COMMITTER_DATE", &format!("{seconds} +0000")),
                ("GIT_AUTHOR_DATE", "4000000000 +0000"),
            ],
        );
        repo.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Writes the minimal record of `key` as its note.
    fn record(repo: &Repo, key: &str) {
        let bytes = document::bytes(&parsed(&minimal(key)));
        notes::write(repo.root(), ".", key, &bytes).expect("write the note");
    }

    /// A repository with a record on each of three commits, whose committer
    /// times are 1,700,000,300, 1,700,000,100, and 1,700,000,200 (the oldest commit
    /// first), and the shas.
    fn three_records() -> (Repo, Vec<String>) {
        let repo = Repo::new();
        let shas: Vec<String> = [
            ("a", 1_700_000_300),
            ("b", 1_700_000_100),
            ("c", 1_700_000_200),
        ]
        .iter()
        .map(|(name, seconds)| commit_at(&repo, name, *seconds))
        .collect();
        for sha in &shas {
            record(&repo, sha);
        }
        (repo, shas)
    }

    /// The keys of `entries`.
    fn keys(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.key.as_str()).collect()
    }

    #[test]
    fn the_records_come_newest_first_by_the_committer_time_of_the_key() {
        let (repo, shas) = three_records();

        let entries = records(repo.root(), ".", None).expect("records");

        assert_eq!(keys(&entries), [&shas[0], &shas[2], &shas[1]]);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.committed)
                .collect::<Vec<_>>(),
            [
                Some(1_700_000_300),
                Some(1_700_000_200),
                Some(1_700_000_100)
            ]
        );
    }

    #[test]
    fn a_tie_goes_by_sha_ascending() {
        let repo = Repo::new();
        let shas: Vec<String> = ["a", "b", "c"]
            .iter()
            .map(|name| commit_at(&repo, name, 1_700_000_500))
            .collect();
        for sha in &shas {
            record(&repo, sha);
        }
        let mut sorted = shas.clone();
        sorted.sort();

        let entries = records(repo.root(), ".", None).expect("records");

        assert_eq!(keys(&entries), sorted);
    }

    #[test]
    fn a_range_keeps_the_records_whose_key_it_holds() {
        let (repo, shas) = three_records();

        let after_first =
            records(repo.root(), ".", Some(&format!("{}..HEAD", shas[0]))).expect("records");
        let up_to_second = records(repo.root(), ".", Some(&shas[1])).expect("records");
        let none = records(repo.root(), ".", Some("HEAD..HEAD")).expect("records");

        assert_eq!(keys(&after_first), [&shas[2], &shas[1]]);
        assert_eq!(keys(&up_to_second), [&shas[0], &shas[1]]);
        assert!(none.is_empty());
    }

    #[test]
    fn a_range_that_git_does_not_read_is_the_named_line() {
        let (repo, _shas) = three_records();

        assert_eq!(
            records(repo.root(), ".", Some("nope..HEAD")).expect_err("a bad range"),
            "bad range \"nope..HEAD\""
        );
        assert_eq!(
            records(repo.root(), ".", Some("--all")).expect_err("an option is no range"),
            "bad range \"--all\""
        );
    }

    #[test]
    fn a_clone_with_the_full_history_has_no_shallow_commits() {
        let (repo, _shas) = three_records();

        assert_eq!(shallow_commits(repo.root()), Ok(None));
    }

    #[test]
    fn the_shallow_commits_are_the_lines_of_the_shallow_file() {
        let (repo, shas) = three_records();
        repo.write(".git/shallow", &format!("{}\n\n{}\n", shas[1], shas[2]));

        assert_eq!(
            shallow_commits(repo.root()),
            Ok(Some(HashSet::from([shas[1].clone(), shas[2].clone()])))
        );
    }

    #[test]
    fn a_range_that_lists_a_shallow_commit_or_names_an_absent_one_reaches_past_the_history() {
        let (repo, shas) = three_records();
        repo.write(".git/shallow", &format!("{}\n", shas[1]));
        let absent = format!("{}..HEAD", "0".repeat(40));

        let listing_the_commit = records(repo.root(), ".", Some("HEAD"));
        let naming_an_absent_one = records(repo.root(), ".", Some(&absent));
        let above_the_commit = records(repo.root(), ".", Some(&format!("{}..HEAD", shas[1])));
        let unreadable = records(repo.root(), ".", Some("nope..HEAD"));

        assert_eq!(
            listing_the_commit.expect_err("a walk that lists the shallow commit"),
            past_the_history_line("HEAD")
        );
        assert_eq!(
            naming_an_absent_one.expect_err("a range with an end that the clone lacks"),
            past_the_history_line(&absent)
        );
        assert_eq!(
            keys(&above_the_commit.expect("a range above the shallow commit")),
            [&shas[2]]
        );
        assert_eq!(
            unreadable.expect_err("a range that git cannot read"),
            "bad range \"nope..HEAD\""
        );
    }

    #[test]
    fn a_relative_end_the_clone_lacks_reaches_past_the_history_when_its_revision_resolves() {
        let (repo, shas) = three_records();
        repo.write(".git/shallow", &format!("{}\n", shas[1]));

        let lacking_by_tilde = records(repo.root(), ".", Some("HEAD~2..HEAD"));
        let lacking_by_caret = records(repo.root(), ".", Some("HEAD..HEAD^^"));
        let symmetric = records(repo.root(), ".", Some("HEAD~5...HEAD"));
        let held = records(repo.root(), ".", Some("HEAD~1..HEAD"));
        let unresolvable_revision = records(repo.root(), ".", Some("nope~2..HEAD"));

        assert_eq!(
            lacking_by_tilde.expect_err("an end past the shallow commit"),
            past_the_history_line("HEAD~2..HEAD")
        );
        assert_eq!(
            lacking_by_caret.expect_err("an end past the shallow commit"),
            past_the_history_line("HEAD..HEAD^^")
        );
        assert_eq!(
            symmetric.expect_err("an end past the shallow commit"),
            past_the_history_line("HEAD~5...HEAD")
        );
        assert_eq!(
            keys(&held.expect("a range above the shallow commit")),
            [&shas[2]]
        );
        assert_eq!(
            unresolvable_revision.expect_err("a revision that git cannot resolve"),
            "bad range \"nope~2..HEAD\""
        );
    }

    #[test]
    fn a_shallow_commit_on_the_excluded_side_reaches_past_the_history() {
        let (repo, shas) = three_records();
        repo.git(&["switch", "-q", "-c", "side", &shas[0]]);
        let side = commit_at(&repo, "side", 1_700_000_150);
        repo.git(&["switch", "-q", "-"]);
        repo.write(".git/shallow", &format!("{side}\n"));

        let excluded = records(repo.root(), ".", Some("side..HEAD"));
        let unrelated_to_it = records(repo.root(), ".", Some(&format!("{}..HEAD", shas[1])));

        assert_eq!(
            excluded.expect_err("a shallow commit on the excluded side"),
            past_the_history_line("side..HEAD")
        );
        assert_eq!(
            keys(&unrelated_to_it.expect("a range that the shallow commit does not touch")),
            [&shas[2]]
        );
    }

    #[test]
    fn a_range_is_split_at_its_first_two_dots_and_a_left_out_end_is_head() {
        let cases = [
            ("a..b", vec!["a", "b"], "a...b"),
            ("a...b", vec!["a", "b"], "a...b"),
            ("..b", vec!["b"], "...b"),
            ("a..", vec!["a"], "a..."),
            ("a", vec!["a"], "a"),
        ];

        for (range, ends, both_sides) in cases {
            let shape = Shape::of(range);

            assert_eq!(shape.ends(), ends, "{range}");
            assert_eq!(shape.both_sides(range), both_sides, "{range}");
        }
    }

    #[test]
    fn a_relative_revision_counts_from_the_revision_before_its_last_step() {
        for (end, revision) in [
            ("HEAD~2", Some("HEAD")),
            ("HEAD~", Some("HEAD")),
            ("HEAD^", Some("HEAD")),
            ("HEAD^^", Some("HEAD^")),
            ("HEAD~2^", Some("HEAD~2")),
            ("v1.0^2", Some("v1.0")),
            ("HEAD", None),
            ("HEAD^{commit}", None),
            ("HEAD^@", None),
            ("HEAD~x", None),
            ("~2", None),
            ("^2", None),
        ] {
            assert_eq!(counted_from(end), revision, "{end}");
        }
    }

    #[test]
    fn a_full_object_name_has_the_length_of_the_object_names_of_the_repository() {
        let sha1 = "0123456789abcdefABCDEF0123456789abcdef01";
        let sha256 = "e".repeat(64);

        assert!(is_full_object_name(sha1, 40));
        assert!(is_full_object_name(&sha256, 64));
        assert!(!is_full_object_name(&sha1[..39], 40));
        assert!(!is_full_object_name(sha1, 64));
        assert!(!is_full_object_name(&sha256, 40));
        assert!(!is_full_object_name(&"g".repeat(40), 40));
    }

    #[test]
    fn the_object_names_of_the_shallow_file_give_the_length_of_a_full_name() {
        let (repo, _shas) = three_records();
        let sha256_names = HashSet::from(["e".repeat(64)]);
        let full = format!("{}..HEAD", "0".repeat(64));
        let abbreviation = format!("{}..HEAD", "0".repeat(40));

        let by_a_full_name = shallow_ranged_times(repo.root(), &full, &sha256_names);
        let by_an_abbreviation = shallow_ranged_times(repo.root(), &abbreviation, &sha256_names);

        assert_eq!(
            by_a_full_name.expect_err("an end that the clone lacks"),
            past_the_history_line(&full)
        );
        assert_eq!(
            by_an_abbreviation.expect_err("a name of another length"),
            format!("bad range \"{abbreviation}\"")
        );
    }

    #[test]
    fn a_range_with_a_line_feed_is_a_bad_range_in_a_shallow_clone() {
        let (repo, shas) = three_records();
        repo.write(".git/shallow", &format!("{}\n", shas[1]));

        let error = records(repo.root(), ".", Some("HEAD~2\n..HEAD")).expect_err("a bad range");

        assert_eq!(error, "bad range \"HEAD~2\n..HEAD\"");
    }

    #[test]
    fn the_listing_of_a_walk_tells_its_sides_and_skips_a_line_that_is_none() {
        let sides = sides_in("300 <aaaa\n200 >bbbb\n100 cccc\nbroken\nx dddd\n");

        assert_eq!(
            sides,
            [
                (300, false, "aaaa"),
                (200, true, "bbbb"),
                (100, true, "cccc")
            ]
        );
    }

    #[test]
    fn a_notes_ref_with_no_note_has_no_entry() {
        let repo = Repo::new();
        commit_at(&repo, "a", 1_700_000_100);

        assert!(records(repo.root(), ".", None).expect("records").is_empty());
        assert!(
            records(repo.root(), ".", Some("HEAD"))
                .expect("records")
                .is_empty()
        );
    }

    #[test]
    fn the_times_are_read_from_the_lines_of_rev_list() {
        let times = times_in("300 aaaa\n100 bbbb\nbroken\nx cccc\n");

        assert_eq!(times.len(), 2);
        assert_eq!(times["aaaa"], 300);
        assert_eq!(times["bbbb"], 100);
    }
}
