//! Trailers and the work-item references they carry (batch 34, spec 3.7).
//!
//! A commit names a work item in a `Work-Item: <id>` trailer and a deferral it
//! files in a `Deferred: <id> <reason>` trailer. [`parse`] reads the trailers
//! of a message with git's own rule (`git interpret-trailers --parse`: the
//! `Key: value` lines of the last paragraph), so CRLF line endings, trailing
//! whitespace, and folded values read as git reads them. The separator is `:`
//! on every install, whatever the repository's `trailer.separators`.
//! houserules reads neither trailer from a pull-request body.
//!
//! `audit` collects every reference of its range ([`collect`]): the trailers
//! of each commit, and the `issues[].backlog` ids of the reviews it reads.
//! [`judge`] decides what the audit says of them: an install with a tracker
//! resolves each reference through it; an install without one counts the
//! references it did not judge.

use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use serde_json::Value;

use crate::tracker::{Lookup, NoTracker, Resolution, Tracker, TrackerError};

use super::deliverable::array_field;
use super::git::{Commit, command};

/// The trailer key of a work item that a commit delivers.
const WORK_ITEM: &str = "Work-Item";

/// The trailer key of a deferral that a commit files.
const DEFERRED: &str = "Deferred";

/// The trailers of `message`, each as `(key, value)` in the order of the
/// message: git's `interpret-trailers --parse` run in `root`. The separator is
/// `:` whatever the repository's `trailer.separators` says, so a trailer reads
/// as `Key: value` on every install (spec 3.7). The comment character is `#`
/// whatever the repository's `core.commentChar` and `core.commentString` say,
/// so a trailer whose key starts with the character that a repository sets
/// (`Intent:` under `core.commentChar = I`) is still a trailer; a `-c
/// core.commentChar=#` on the command line also overrides `core.commentString`
/// (measured at git 2.56.0). A value has its surrounding whitespace trimmed, and
/// a folded value arrives as one line. A message with no trailer block has none.
/// One input stays that no option turns off: a key that `trailer.<alias>.key`
/// configures, in the repository or in a file of the user. A block of lines
/// that are not all trailers is a trailer block when it holds a configured key
/// and at least a quarter of its lines are trailers (measured at git 2.56.0:
/// with `trailer.wi.key=Work-Item`, a `Work-Item: HR-1` line before three prose
/// lines is a trailer block). A `git` that cannot run or fails, and a line of
/// its output that holds no `:`, is a named `Err`
/// (`houserules.crash-paths-are-named`).
pub(crate) fn parse(root: &Path, message: &str) -> Result<Vec<(String, String)>, String> {
    let mut child = command(root)
        .args([
            "-c",
            "trailer.separators=:",
            "-c",
            "core.commentChar=#",
            "interpret-trailers",
            "--parse",
            "--no-divider",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run git interpret-trailers: {error}"))?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    // `interpret-trailers` reads its whole input before it prints, so this
    // write never waits on a full output pipe.
    let written = stdin.write_all(message.as_bytes());
    drop(stdin);
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not run git interpret-trailers: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git interpret-trailers failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    written.map_err(|error| format!("could not write to git interpret-trailers: {error}"))?;
    read_trailer_lines(&String::from_utf8_lossy(&output.stdout))
}

/// The `(key, value)` pairs of the lines that `git interpret-trailers --parse`
/// printed under the separator `:`. A line with no `:` is a named `Err`:
/// the reader never skips a trailer
/// (`houserules.crash-paths-are-named`).
fn read_trailer_lines(output: &str) -> Result<Vec<(String, String)>, String> {
    output
        .lines()
        .map(|line| {
            line.split_once(':')
                .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
                .ok_or_else(|| format!("git interpret-trailers printed a line with no ':': {line}"))
        })
        .collect()
}

/// `true` when the trailer key `key` is `wanted`. Git's trailer rules take a
/// key without regard to its case.
pub(crate) fn is_key(key: &str, wanted: &str) -> bool {
    key.eq_ignore_ascii_case(wanted)
}

/// A reference to a work item that `audit` read.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Reference {
    /// `Work-Item: <id>`, a trailer of the commit `sha`.
    WorkItem {
        /// The short sha of the commit.
        sha: String,
        /// The id as written.
        id: String,
    },
    /// `Deferred: <id> <reason>`, a trailer of the commit `sha`.
    Deferred {
        /// The short sha of the commit.
        sha: String,
        /// The id as written.
        id: String,
        /// The reason as written; empty when the trailer names none.
        reason: String,
    },
    /// An `issues[].backlog` id of the deliverable `file`.
    Backlog {
        /// The deliverable, as the line names it.
        file: String,
        /// The id as written.
        id: String,
    },
}

/// The references of `commits`, each commit's trailers in message order, then
/// the `issues[].backlog` ids of each deliverable in `deliverables` (its name
/// and its parsed JSON). A deliverable whose `issues` is no array, or one of
/// whose `backlog` values is no string, is a named `Err`; a deliverable with
/// no `issues` has no reference.
pub(super) fn collect(
    commits: &[Commit],
    deliverables: &[(String, &Value)],
) -> Result<Vec<Reference>, String> {
    let mut references = Vec::new();
    for commit in commits {
        for (key, value) in &commit.trailers {
            if is_key(key, WORK_ITEM) {
                references.push(Reference::WorkItem {
                    sha: commit.short_sha.clone(),
                    id: value.clone(),
                });
            } else if is_key(key, DEFERRED) {
                let (id, reason) = value.split_once(char::is_whitespace).unwrap_or((value, ""));
                references.push(Reference::Deferred {
                    sha: commit.short_sha.clone(),
                    id: id.to_string(),
                    reason: reason.trim().to_string(),
                });
            }
        }
    }
    for (file, data) in deliverables {
        let issues = array_field(data, "issues").map_err(|error| format!("{file}: {error}"))?;
        for (index, issue) in issues.into_iter().enumerate() {
            match issue.get("backlog") {
                None | Some(Value::Null) => {}
                Some(Value::String(id)) => references.push(Reference::Backlog {
                    file: file.clone(),
                    id: id.clone(),
                }),
                Some(_) => return Err(format!("{file}: issues[{index}].backlog is not a string")),
            }
        }
    }
    Ok(references)
}

/// How a reference reads in a line when its id is empty: a designed token,
/// never a bare gap.
fn shown(id: &str) -> String {
    if id.is_empty() {
        "\"\"".to_string()
    } else {
        id.to_string()
    }
}

/// The line for a `Work-Item` trailer that the tracker does not hold.
fn work_item_unresolved(sha: &str, id: &str, adapter: &str) -> String {
    format!(
        "{sha}: Work-Item {} does not resolve in the {adapter} tracker",
        shown(id)
    )
}

/// The line for a `Work-Item` trailer that names a record the tracker does
/// not hold as a work item. `reason` is the adapter's text, such as `is a
/// pull request, not an issue`.
fn work_item_not_an_item(sha: &str, id: &str, reason: &str, adapter: &str) -> String {
    format!(
        "{sha}: Work-Item {} {reason}, in the {adapter} tracker",
        shown(id)
    )
}

/// The line for a `Deferred` trailer that names no reason.
fn deferred_without_reason(sha: &str, id: &str) -> String {
    format!("{sha}: Deferred {} names no reason", shown(id))
}

/// The line for a `Deferred` trailer whose item is no filed deferral.
fn deferred_unfiled(sha: &str, id: &str, adapter: &str) -> String {
    format!(
        "{sha}: Deferred {} is not a filed deferral with its reason in the {adapter} tracker",
        shown(id)
    )
}

/// The line for a deliverable's `issues[].backlog` id that is no filed
/// deferral.
fn backlog_unfiled(file: &str, id: &str, adapter: &str) -> String {
    format!(
        "{file}: {} is not a filed deferral with its reason in the {adapter} tracker",
        shown(id)
    )
}

/// `true` when `tracker` holds `id` as an item that carries the deferral
/// marker with a reason. An id that matches none of the adapter's id shapes
/// reaches no backend: it is no filed deferral.
fn is_filed_deferral(tracker: &dyn Tracker, id: &str) -> Result<bool, TrackerError> {
    if !tracker.matches_id(id) {
        return Ok(false);
    }
    Ok(match tracker.resolve(id)? {
        Resolution::Found(found) => found
            .deferral
            .is_some_and(|deferral| !deferral.reason.trim().is_empty()),
        Resolution::NotFound | Resolution::NotAnItem { .. } => false,
    })
}

/// Resolves each of `references` through `tracker` and returns one line for
/// each that fails, in the order of `references`. An id that matches none of
/// the adapter's declared id shapes fails with no call to the backend, so an
/// id such as `--help` never reaches a tool as an option. A tracker that
/// cannot be reached is an `Err`, and no line is returned.
fn verdict(tracker: &dyn Tracker, references: &[Reference]) -> Result<Vec<String>, TrackerError> {
    let adapter = tracker.name();
    let mut failures = Vec::new();
    for reference in references {
        match reference {
            Reference::WorkItem { sha, id } => {
                if !tracker.matches_id(id) {
                    failures.push(work_item_unresolved(sha, id, adapter));
                    continue;
                }
                match tracker.resolve(id)? {
                    Resolution::Found(_) => {}
                    Resolution::NotFound => failures.push(work_item_unresolved(sha, id, adapter)),
                    Resolution::NotAnItem { reason } => {
                        failures.push(work_item_not_an_item(sha, id, &reason, adapter));
                    }
                }
            }
            Reference::Deferred { sha, id, reason } => {
                if reason.is_empty() {
                    failures.push(deferred_without_reason(sha, id));
                } else if !is_filed_deferral(tracker, id)? {
                    failures.push(deferred_unfiled(sha, id, adapter));
                }
            }
            Reference::Backlog { file, id } => {
                if !is_filed_deferral(tracker, id)? {
                    failures.push(backlog_unfiled(file, id, adapter));
                }
            }
        }
    }
    Ok(failures)
}

/// What `audit` decided about the references of its range: the counts and
/// the failure lines that `audit`'s JSON result holds under `references`, the
/// line that only stderr carries, and the exit code of a tracker error.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Verdict {
    /// The references that the tracker judged, passed or failed.
    pub(super) checked: usize,
    /// The references among those `checked` that passed. A reference fails
    /// with at most one line, so this is `checked` less the number of lines
    /// in `failures` when the tracker judged every reference.
    pub(super) passed: usize,
    /// The references that the audit did not judge: all of them when the
    /// install has no tracker, when the tracker cannot be reached, or when the
    /// tracker answered with an error.
    pub(super) skipped: usize,
    /// One line for each failure, in the order that `audit` prints them. Line
    /// B and the line of a tracker error are failures.
    pub(super) failures: Vec<String>,
    /// The line of an install with no tracker and no `backlog/` directory,
    /// which counts the skipped references. It fails nothing, so it is no
    /// entry of `failures`.
    pub(super) skipped_line: Option<String>,
    /// The exit code that a tracker error sets, whatever the rule rows
    /// decided: 1 for a tracker that cannot be reached, 2 for any other
    /// error.
    pub(super) exit_code: Option<u8>,
}

impl Verdict {
    /// The lines that `audit` prints on stderr: each failure, then the line
    /// that counts the skipped references when there is one.
    pub(super) fn stderr_lines(&self) -> impl Iterator<Item = &str> {
        self.failures
            .iter()
            .chain(&self.skipped_line)
            .map(String::as_str)
    }
}

/// Judges `references` against the tracker state `lookup` of the install
/// (spec 3.4 and 3.7): an installed tracker resolves each reference
/// ([`verdict`]); a tracker error leaves every reference skipped and is the
/// one failure; an install with no tracker resolves none, and a `backlog/`
/// directory there is a failure (line B).
pub(super) fn judge(lookup: &Lookup, references: &[Reference]) -> Verdict {
    let total = references.len();
    match lookup {
        Lookup::Installed(tracker) => match verdict(tracker.as_ref(), references) {
            Ok(failures) => Verdict {
                checked: total,
                passed: total.saturating_sub(failures.len()),
                skipped: 0,
                failures,
                skipped_line: None,
                exit_code: None,
            },
            Err(error) => Verdict {
                checked: 0,
                passed: 0,
                skipped: total,
                failures: vec![error.to_string()],
                skipped_line: None,
                exit_code: Some(error.exit_code(true)),
            },
        },
        Lookup::Absent(state @ NoTracker::BacklogDir) => Verdict {
            checked: 0,
            passed: 0,
            skipped: total,
            failures: vec![state.line().to_string()],
            skipped_line: None,
            exit_code: None,
        },
        Lookup::Absent(NoTracker::Bare) => Verdict {
            checked: 0,
            passed: 0,
            skipped: total,
            failures: Vec::new(),
            skipped_line: Some(crate::tracker::lines::audit_skipped(total)),
            exit_code: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use serde_json::json;

    use super::*;
    use crate::rules::git::command as git_command;
    use crate::tracker::{DeferRequest, Deferral, ListOpts, Resolved};

    /// What a `Recorder` answers for an id of a shape that it claims.
    enum Holds {
        /// A work item with no deferral marker.
        Item,
        /// A work item that carries the deferral marker with `reason`.
        Deferral(&'static str),
        /// Nothing.
        Nothing,
        /// A record that is no work item, for the reason given.
        NoItem(&'static str),
        /// A tracker that cannot be reached.
        Unreachable,
        /// A failure that is no unreachable tracker, with the line it prints.
        Fails(&'static str),
    }

    /// A tracker named `github` that claims the ids that start with `#`,
    /// answers every claimed id as `holds`, and records each id it was asked
    /// to resolve. Every other operation panics.
    struct Recorder {
        holds: Holds,
        resolved: RefCell<Vec<String>>,
    }

    impl Recorder {
        fn new(holds: Holds) -> Self {
            Self {
                holds,
                resolved: RefCell::new(Vec::new()),
            }
        }

        /// The ids that the tracker was asked to resolve.
        fn calls(&self) -> Vec<String> {
            self.resolved.borrow().clone()
        }
    }

    impl Tracker for Recorder {
        fn name(&self) -> &'static str {
            "github"
        }

        fn matches_id(&self, id: &str) -> bool {
            id.starts_with('#')
        }

        fn resolve(&self, id: &str) -> Result<Resolution, TrackerError> {
            self.resolved.borrow_mut().push(id.to_string());
            match &self.holds {
                Holds::Item => Ok(Resolution::Found(Resolved { deferral: None })),
                Holds::Deferral(reason) => Ok(Resolution::Found(Resolved {
                    deferral: Some(Deferral {
                        reason: (*reason).to_string(),
                    }),
                })),
                Holds::Nothing => Ok(Resolution::NotFound),
                Holds::NoItem(reason) => Ok(Resolution::NotAnItem {
                    reason: (*reason).to_string(),
                }),
                Holds::Unreachable => Err(TrackerError::Unreachable {
                    adapter: "github",
                    cause: "gh is not installed".to_string(),
                    setup: "install gh".to_string(),
                }),
                Holds::Fails(line) => Err(TrackerError::Command((*line).to_string())),
            }
        }

        fn list(&self, _opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
            unreachable!("the verdict never lists")
        }

        fn batch(&self, _number: &str) -> Result<Value, TrackerError> {
            unreachable!("the verdict never reads a batch")
        }

        fn get(&self, _ids: &[String]) -> Result<Vec<Value>, TrackerError> {
            unreachable!("the verdict never gets")
        }

        fn set(&mut self, _id: &str, _assignments: &[String]) -> Result<Value, TrackerError> {
            unreachable!("the verdict never sets")
        }

        fn defer(&mut self, _request: &DeferRequest) -> Result<String, TrackerError> {
            unreachable!("the verdict never defers")
        }

        fn archive_backlog_half(&mut self, _lines: &mut Vec<String>) -> Result<(), TrackerError> {
            unreachable!("the verdict never archives")
        }

        fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
            unreachable!("the verdict never checks")
        }

        fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
            unreachable!("the verdict never reads the pattern")
        }
    }

    fn work_item(id: &str) -> Reference {
        Reference::WorkItem {
            sha: "abc1234".to_string(),
            id: id.to_string(),
        }
    }

    fn deferred(id: &str, reason: &str) -> Reference {
        Reference::Deferred {
            sha: "abc1234".to_string(),
            id: id.to_string(),
            reason: reason.to_string(),
        }
    }

    fn backlog(id: &str) -> Reference {
        Reference::Backlog {
            file: "task-1-review.json".to_string(),
            id: id.to_string(),
        }
    }

    /// A commit with the trailers `trailers`.
    fn commit(short_sha: &str, trailers: &[(&str, &str)]) -> Commit {
        Commit {
            sha: format!("{short_sha}0000000000000000000000000000000"),
            short_sha: short_sha.to_string(),
            subject: "feat: a change".to_string(),
            body: String::new(),
            author: "A Human".to_string(),
            trailers: trailers
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }

    /// Git reads a trailer block as git does, whatever line endings and
    /// trailing whitespace the message holds (review focus 2).
    #[test]
    fn trailers_parse_crlf_and_trailing_space() {
        let message = "feat: a change\r\n\r\nA body line.\r\n\r\nWork-Item: HR-001  \r\nWork-Item: HR-002\t\r\nDeferred: HR-003 Waits for the design. \r\n";
        let parsed = parse(Path::new("."), message).expect("git parses the message");
        assert_eq!(
            parsed,
            [
                ("Work-Item".to_string(), "HR-001".to_string()),
                ("Work-Item".to_string(), "HR-002".to_string()),
                (
                    "Deferred".to_string(),
                    "HR-003 Waits for the design.".to_string()
                ),
            ]
        );
        let lf = message.replace("\r\n", "\n");
        assert_eq!(parse(Path::new("."), &lf).expect("git parses"), parsed);
    }

    /// Only the last paragraph of a message holds trailers; a message of one
    /// paragraph holds none; a folded value is one line; a value that starts
    /// with a dash is a value.
    #[test]
    fn only_the_last_paragraph_holds_trailers() {
        let root = Path::new(".");
        let read = |message: &str| parse(root, message).expect("git parses the message");
        assert_eq!(
            read("subject\n\nWork-Item: HR-001\n\nA closing paragraph.\n"),
            Vec::<(String, String)>::new()
        );
        assert_eq!(
            read("subject\nWork-Item: HR-001\n"),
            Vec::<(String, String)>::new()
        );
        assert_eq!(
            read("subject\n\nDeferred: HR-001 A reason\n  that folds.\n"),
            [(
                "Deferred".to_string(),
                "HR-001 A reason that folds.".to_string()
            )]
        );
        assert_eq!(
            read("subject\n\nWork-Item: --help\n"),
            [("Work-Item".to_string(), "--help".to_string())]
        );
        assert_eq!(
            read("subject\n\nWork-Item:\n"),
            [("Work-Item".to_string(), String::new())]
        );
    }

    /// A `git` that cannot run in `root` is a named error, never a panic and
    /// never an empty trailer list (`houserules.crash-paths-are-named`).
    #[test]
    fn a_root_that_git_cannot_run_in_is_a_named_error() {
        let error = parse(Path::new("no/such/directory/for/git"), "subject\n\nA: b\n")
            .expect_err("git cannot run in a missing directory");
        assert!(
            error.starts_with("could not run git interpret-trailers: "),
            "{error}"
        );
    }

    /// A scratch repository with each `(key, value)` of `settings` in its
    /// configuration.
    fn repository_with_settings(settings: &[(&str, &str)]) -> tempfile::TempDir {
        let repository = tempfile::tempdir().expect("tempdir");
        let mut commands = vec![vec!["init".to_string(), "-q".to_string()]];
        commands.extend(settings.iter().map(|(key, value)| {
            vec![
                "config".to_string(),
                (*key).to_string(),
                (*value).to_string(),
            ]
        }));
        for args in commands {
            let status = git_command(repository.path())
                .args(&args)
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        }
        repository
    }

    /// Spec 3.7 (C32): trailers parse under git's default separator `:`,
    /// whatever the repository's `trailer.separators`. Under `#:` git prints
    /// a `Work-Item: WI-099` trailer as `Work-Item# WI-099`; the reader still
    /// returns it. A line that uses `#` is no trailer, as it is none under
    /// git's default.
    #[test]
    fn a_repository_trailer_separators_does_not_change_what_reads_as_a_trailer() {
        let repository = repository_with_settings(&[("trailer.separators", "#:")]);
        let read = |message: &str| parse(repository.path(), message).expect("git parses");
        assert_eq!(
            read("subject\n\nWork-Item: WI-099\nSigned-off-by: A <a@b.c>\n"),
            [
                ("Work-Item".to_string(), "WI-099".to_string()),
                ("Signed-off-by".to_string(), "A <a@b.c>".to_string()),
            ]
        );
        assert_eq!(
            read("subject\n\nWork-Item# WI-099\n"),
            Vec::<(String, String)>::new()
        );
    }

    /// A repository that sets `core.commentChar` or `core.commentString` to a
    /// letter does not turn a trailer whose key starts with it into a comment:
    /// git drops such a line from the trailer block unless the call pins the
    /// comment character. The settings are the ones that change the text that
    /// `interpret-trailers` prints (`houserules.parsed-git-text-pins-every-setting`).
    #[test]
    fn a_repository_comment_character_does_not_drop_a_trailer() {
        let message =
            "subject\n\nIntent: openspec:add-x\nEntire-Checkpoint: cp-1\nWork-Item: HR-1\n";
        let expected = [
            ("Intent".to_string(), "openspec:add-x".to_string()),
            ("Entire-Checkpoint".to_string(), "cp-1".to_string()),
            ("Work-Item".to_string(), "HR-1".to_string()),
        ];
        for settings in [
            vec![("core.commentChar", "I")],
            vec![("core.commentChar", "E")],
            vec![("core.commentChar", "W")],
            vec![("core.commentString", "Intent")],
            vec![("core.commentString", "Work")],
        ] {
            let repository = repository_with_settings(&settings);

            assert_eq!(
                parse(repository.path(), message).expect("git parses"),
                expected,
                "{settings:?}"
            );
        }
    }

    /// A line of the output of git that has no `:` is a named error, never a
    /// skipped trailer (`houserules.crash-paths-are-named`).
    #[test]
    fn a_git_output_line_with_no_separator_is_a_named_error() {
        assert_eq!(
            read_trailer_lines("Work-Item: WI-001\nWork-Item# WI-099\n"),
            Err("git interpret-trailers printed a line with no ':': Work-Item# WI-099".to_string())
        );
        assert_eq!(
            read_trailer_lines("Work-Item: WI-001\nDeferred: WI-002 A reason.\n"),
            Ok(vec![
                ("Work-Item".to_string(), "WI-001".to_string()),
                ("Deferred".to_string(), "WI-002 A reason.".to_string()),
            ])
        );
        assert_eq!(read_trailer_lines(""), Ok(Vec::new()));
    }

    /// A `---` line in a commit message is no divider: the trailers after it
    /// are read.
    #[test]
    fn a_divider_line_does_not_end_the_message() {
        let message = "subject\n\nA body.\n---\nMore body.\n\nWork-Item: HR-001\n";
        assert_eq!(
            parse(Path::new("."), message).expect("git parses"),
            [("Work-Item".to_string(), "HR-001".to_string())]
        );
    }

    /// Each commit contributes its `Work-Item` and `Deferred` trailers in
    /// message order, the key without regard to its case, and a `Deferred`
    /// value splits into the id and the rest as its reason. A commit with
    /// other trailers contributes nothing for them.
    #[test]
    fn collect_reads_the_work_item_and_deferred_trailers_of_each_commit() {
        let commits = [
            commit(
                "aaa1111",
                &[
                    ("Signed-off-by", "A <a@b.c>"),
                    ("Work-Item", "HR-001"),
                    ("work-item", "HR-002"),
                ],
            ),
            commit(
                "bbb2222",
                &[("DEFERRED", "HR-003 It waits."), ("Deferred", "HR-004")],
            ),
            commit("ccc3333", &[]),
        ];
        let references = collect(&commits, &[]).expect("collects");
        assert_eq!(
            references,
            [
                Reference::WorkItem {
                    sha: "aaa1111".to_string(),
                    id: "HR-001".to_string()
                },
                Reference::WorkItem {
                    sha: "aaa1111".to_string(),
                    id: "HR-002".to_string()
                },
                Reference::Deferred {
                    sha: "bbb2222".to_string(),
                    id: "HR-003".to_string(),
                    reason: "It waits.".to_string()
                },
                Reference::Deferred {
                    sha: "bbb2222".to_string(),
                    id: "HR-004".to_string(),
                    reason: String::new()
                },
            ]
        );
    }

    /// Each `issues[].backlog` id of a deliverable is a reference, in the
    /// order of the files and of the issues; an issue with no `backlog` and
    /// a deliverable with no `issues` add none.
    #[test]
    fn collect_reads_the_backlog_ids_of_the_issues_of_each_deliverable() {
        let review = json!({"issues": [
            {"severity": "minor", "backlog": "HR-010"},
            {"severity": "minor"},
            {"severity": "minor", "backlog": null},
            {"severity": "minor", "backlog": "HR-011"},
        ]});
        let report = json!({"kind": "task-report"});
        let deliverables = [
            ("task-1-review.json".to_string(), &review),
            ("task-1-report.json".to_string(), &report),
        ];
        let references = collect(&[], &deliverables).expect("collects");
        assert_eq!(
            references,
            [
                Reference::Backlog {
                    file: "task-1-review.json".to_string(),
                    id: "HR-010".to_string()
                },
                Reference::Backlog {
                    file: "task-1-review.json".to_string(),
                    id: "HR-011".to_string()
                },
            ]
        );
    }

    /// A present `issues` or `backlog` of another type is a named fault, not
    /// silence (`houserules.crash-paths-are-named`).
    #[test]
    fn collect_names_a_deliverable_whose_issues_have_the_wrong_type() {
        let not_an_array = json!({"issues": {"a": 1}});
        assert_eq!(
            collect(&[], &[("task-1-review.json".to_string(), &not_an_array)]),
            Err("task-1-review.json: issues is not an array".to_string())
        );
        let not_a_string = json!({"issues": [{"backlog": "HR-001"}, {"backlog": 7}]});
        assert_eq!(
            collect(&[], &[("task-1-review.json".to_string(), &not_a_string)]),
            Err("task-1-review.json: issues[1].backlog is not a string".to_string())
        );
    }

    /// An id of no declared shape fails with the not-resolve line and no call
    /// to the tracker, for each of the three kinds of reference.
    #[test]
    fn an_id_of_no_declared_shape_reaches_no_backend() {
        let tracker = Recorder::new(Holds::Deferral("Waits."));
        let failures = verdict(
            &tracker,
            &[
                work_item("--help"),
                deferred("-x", "Because."),
                backlog("12"),
                work_item(""),
            ],
        )
        .expect("a verdict");
        assert_eq!(
            failures,
            [
                "abc1234: Work-Item --help does not resolve in the github tracker",
                "abc1234: Deferred -x is not a filed deferral with its reason in the github tracker",
                "task-1-review.json: 12 is not a filed deferral with its reason in the github tracker",
                "abc1234: Work-Item \"\" does not resolve in the github tracker",
            ]
        );
        assert_eq!(tracker.calls(), Vec::<String>::new(), "no backend call");
    }

    /// An id of a declared shape reaches the tracker, once for each
    /// reference; a work item that the tracker holds passes whatever its
    /// deferral state.
    #[test]
    fn an_id_of_a_declared_shape_is_resolved_and_a_held_item_passes() {
        let tracker = Recorder::new(Holds::Item);
        let failures = verdict(&tracker, &[work_item("#12"), work_item("#13")]).expect("a verdict");
        assert_eq!(failures, Vec::<String>::new());
        assert_eq!(tracker.calls(), ["#12", "#13"]);
    }

    /// A work item that the tracker does not hold prints the not-resolve
    /// line with the adapter's name.
    #[test]
    fn a_work_item_the_tracker_does_not_hold_prints_the_not_resolve_line() {
        let tracker = Recorder::new(Holds::Nothing);
        let failures = verdict(&tracker, &[work_item("#99")]).expect("a verdict");
        assert_eq!(
            failures,
            ["abc1234: Work-Item #99 does not resolve in the github tracker"]
        );
    }

    /// Spec 3.5: a trailer that names a pull request prints the adapter's
    /// reason after the id, with the adapter's name at the end.
    #[test]
    fn a_work_item_that_is_a_pull_request_prints_the_adapters_reason() {
        let tracker = Recorder::new(Holds::NoItem("is a pull request, not an issue"));
        let failures = verdict(&tracker, &[work_item("#7")]).expect("a verdict");
        assert_eq!(
            failures,
            ["abc1234: Work-Item #7 is a pull request, not an issue, in the github tracker"]
        );
    }

    /// A `Deferred` trailer passes when it names a reason and the item
    /// carries the marker with a reason; the two texts are not compared. It
    /// fails for an item with no marker, an item with an empty reason, a
    /// missing item, and a record that is no work item; with no reason of its
    /// own it fails before the tracker is asked.
    #[test]
    fn a_deferred_trailer_needs_its_reason_and_a_filed_deferral() {
        let filed = Recorder::new(Holds::Deferral("The tracker's reason."));
        assert_eq!(
            verdict(&filed, &[deferred("#1", "The trailer's reason.")]).expect("a verdict"),
            Vec::<String>::new()
        );
        let unfiled = |holds| {
            let tracker = Recorder::new(holds);
            verdict(&tracker, &[deferred("#1", "A reason.")]).expect("a verdict")
        };
        let line =
            "abc1234: Deferred #1 is not a filed deferral with its reason in the github tracker";
        for holds in [
            Holds::Item,
            Holds::Deferral(" "),
            Holds::Nothing,
            Holds::NoItem("is a pull request, not an issue"),
        ] {
            assert_eq!(unfiled(holds), [line]);
        }
        let none = Recorder::new(Holds::Deferral("A reason."));
        assert_eq!(
            verdict(&none, &[deferred("#1", "")]).expect("a verdict"),
            ["abc1234: Deferred #1 names no reason"]
        );
        assert_eq!(none.calls(), Vec::<String>::new());
    }

    /// An `issues[].backlog` id passes only as a filed deferral with its
    /// reason, and the line names the deliverable.
    #[test]
    fn a_backlog_id_of_an_issue_needs_a_filed_deferral() {
        let filed = Recorder::new(Holds::Deferral("Waits."));
        assert_eq!(
            verdict(&filed, &[backlog("#5")]).expect("a verdict"),
            Vec::<String>::new()
        );
        let item = Recorder::new(Holds::Item);
        assert_eq!(
            verdict(&item, &[backlog("#5")]).expect("a verdict"),
            [
                "task-1-review.json: #5 is not a filed deferral with its reason in the github tracker"
            ]
        );
    }

    /// A tracker that cannot be reached is an error and no line: the gate
    /// reports the tracker, never a verdict.
    #[test]
    fn an_unreachable_tracker_is_an_error_not_a_verdict() {
        struct Down;
        impl Tracker for Down {
            fn name(&self) -> &'static str {
                "github"
            }
            fn matches_id(&self, _id: &str) -> bool {
                true
            }
            fn resolve(&self, _id: &str) -> Result<Resolution, TrackerError> {
                Err(TrackerError::Unreachable {
                    adapter: "github",
                    cause: "gh is not installed".to_string(),
                    setup: "install gh".to_string(),
                })
            }
            fn list(&self, _opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
                unreachable!()
            }
            fn batch(&self, _number: &str) -> Result<Value, TrackerError> {
                unreachable!()
            }
            fn get(&self, _ids: &[String]) -> Result<Vec<Value>, TrackerError> {
                unreachable!()
            }
            fn set(&mut self, _id: &str, _assignments: &[String]) -> Result<Value, TrackerError> {
                unreachable!()
            }
            fn defer(&mut self, _request: &DeferRequest) -> Result<String, TrackerError> {
                unreachable!()
            }
            fn archive_backlog_half(
                &mut self,
                _lines: &mut Vec<String>,
            ) -> Result<(), TrackerError> {
                unreachable!()
            }
            fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
                unreachable!()
            }
            fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
                unreachable!()
            }
        }
        let error = verdict(&Down, &[work_item("#1"), work_item("--help")])
            .expect_err("the tracker is down");
        assert_eq!(
            error,
            TrackerError::Unreachable {
                adapter: "github",
                cause: "gh is not installed".to_string(),
                setup: "install gh".to_string(),
            }
        );
    }

    const UNREACHABLE_LINE: &str =
        "tracker github cannot be reached: gh is not installed; install gh";

    /// An installed tracker judges every reference: each counts as checked,
    /// and the failure lines keep the order of the references. None passes
    /// here, because the tracker holds nothing. No tracker error sets the exit
    /// code.
    #[test]
    fn judge_counts_each_reference_of_an_installed_tracker_as_checked() {
        let lookup = Lookup::Installed(Box::new(Recorder::new(Holds::Nothing)));
        let verdict = judge(
            &lookup,
            &[work_item("#1"), backlog("#2"), deferred("#3", "")],
        );
        assert_eq!(
            verdict,
            Verdict {
                checked: 3,
                passed: 0,
                skipped: 0,
                failures: vec![
                    "abc1234: Work-Item #1 does not resolve in the github tracker".to_string(),
                    "task-1-review.json: #2 is not a filed deferral with its reason in the github tracker"
                        .to_string(),
                    "abc1234: Deferred #3 names no reason".to_string(),
                ],
                skipped_line: None,
                exit_code: None,
            }
        );
    }

    /// HR-221: a passing and a failing reference give `checked` 2 and `passed`
    /// 1. The `Work-Item` resolves, and the `Deferred` trailer names no reason.
    #[test]
    fn judge_counts_a_passing_and_a_failing_reference() {
        let lookup = Lookup::Installed(Box::new(Recorder::new(Holds::Item)));
        let verdict = judge(&lookup, &[work_item("#1"), deferred("#2", "")]);
        assert_eq!(
            verdict,
            Verdict {
                checked: 2,
                passed: 1,
                skipped: 0,
                failures: vec!["abc1234: Deferred #2 names no reason".to_string()],
                skipped_line: None,
                exit_code: None,
            }
        );
    }

    /// A tracker that cannot be reached is the one failure: every reference
    /// stays skipped, and the exit code is 1, the code of a gate.
    #[test]
    fn judge_skips_every_reference_of_a_tracker_that_cannot_be_reached() {
        let lookup = Lookup::Installed(Box::new(Recorder::new(Holds::Unreachable)));
        let verdict = judge(&lookup, &[work_item("#1"), work_item("#2")]);
        assert_eq!(
            verdict,
            Verdict {
                checked: 0,
                passed: 0,
                skipped: 2,
                failures: vec![UNREACHABLE_LINE.to_string()],
                skipped_line: None,
                exit_code: Some(1),
            }
        );
    }

    /// Any other tracker error is the one failure too, with the exit code 2
    /// that every other failure of a tracker command sets.
    #[test]
    fn judge_gives_a_tracker_error_that_is_no_unreachable_tracker_exit_2() {
        let lookup = Lookup::Installed(Box::new(Recorder::new(Holds::Fails("gh printed a mess"))));
        let verdict = judge(&lookup, &[work_item("#1")]);
        assert_eq!(
            verdict,
            Verdict {
                checked: 0,
                passed: 0,
                skipped: 1,
                failures: vec!["gh printed a mess".to_string()],
                skipped_line: None,
                exit_code: Some(2),
            }
        );
    }

    /// With no tracker and no `backlog/` directory, nothing is checked and
    /// nothing fails: the count of the skipped references is a line of
    /// stderr only.
    #[test]
    fn judge_skips_every_reference_of_an_install_with_no_tracker() {
        let verdict = judge(&Lookup::Absent(NoTracker::Bare), &[work_item("WI-1")]);
        assert_eq!(
            verdict,
            Verdict {
                checked: 0,
                passed: 0,
                skipped: 1,
                failures: Vec::new(),
                skipped_line: Some(
                    "skipped 1 work-item references: no tracker installed (houserules init --tracker)"
                        .to_string()
                ),
                exit_code: None,
            }
        );
    }

    /// With no tracker and a `backlog/` directory, line B is the one failure,
    /// and every reference stays skipped.
    #[test]
    fn judge_makes_line_b_the_one_failure_of_an_install_with_a_backlog_directory() {
        let verdict = judge(
            &Lookup::Absent(NoTracker::BacklogDir),
            &[work_item("WI-1"), work_item("WI-2")],
        );
        assert_eq!(
            verdict,
            Verdict {
                checked: 0,
                passed: 0,
                skipped: 2,
                failures: vec![
                    "backlog/ exists but no tracker is installed; run houserules init --tracker json"
                        .to_string()
                ],
                skipped_line: None,
                exit_code: None,
            }
        );
    }

    /// Stderr gets each failure, then the line that counts the skipped
    /// references.
    #[test]
    fn stderr_lines_print_the_failures_and_then_the_skipped_line() {
        let verdict = Verdict {
            checked: 0,
            passed: 0,
            skipped: 0,
            failures: vec!["one".to_string(), "two".to_string()],
            skipped_line: Some("three".to_string()),
            exit_code: None,
        };
        assert_eq!(
            verdict.stderr_lines().collect::<Vec<_>>(),
            ["one", "two", "three"]
        );
    }
}
