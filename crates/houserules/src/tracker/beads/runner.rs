//! The `bd` runner of the Beads adapter (spec 3.4, 3.5, and 3.7).
//!
//! [`Runner`] spawns `bd` from `PATH`, or the program that the variable
//! `HOUSERULES_BD` names: the tests point it at a stand-in that records its
//! calls. `bd` runs in the install's directory, where it finds the store. Each
//! argument is one argv entry and no shell runs. The child inherits the
//! environment, except that every `bd` that houserules starts gets
//! `BD_DISABLE_METRICS=1` and `DO_NOT_TRACK=1` (ruling 188, `docs/design.md`
//! 188): `bd` queues a usage event for its vendor on each call unless metrics
//! are off, and an adopter never chose to send one through houserules. The rest
//! of the configuration of the person who runs `bd` stays theirs.
//!
//! A failed call is a [`BdError`], which names one of the causes of spec 3.4.
//! The cause comes from the stderr of `bd`: a store that is missing is known by
//! its own sentence, and any other failure is the first line of stderr that is
//! no part of a warning, else the first line, else the exit status (C36,
//! spec 3.4). `bd` prints its warnings (a store directory open to the group, a
//! role that is not configured, a `metadata.json` that it cannot read) before
//! the error, and a warning may go on in indented lines. The GitHub adapter
//! keeps the literal rule, the first stderr line. A new `bd` that words an
//! error otherwise is an "other failure" with its first line, never a wrong
//! cause.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;

use crate::tracker::TrackerError;

/// The variable that names the program to run in place of `bd`.
const PROGRAM_VARIABLE: &str = "HOUSERULES_BD";

/// The variables that turn the usage metrics of `bd` off, each with its value.
const METRICS_OFF: [(&str, &str); 2] = [("BD_DISABLE_METRICS", "1"), ("DO_NOT_TRACK", "1")];

/// The sentence with which `bd` says that the directory holds no store: both
/// a directory with no `.beads` and a clone before `bd bootstrap` get it.
const NO_STORE_SENTENCE: &str = "no beads database found";

/// Spawns `bd` for the Beads adapter.
pub(super) struct Runner {
    program: OsString,
    dir: PathBuf,
}

impl Runner {
    /// The runner for the program that `HOUSERULES_BD` names, else `bd`, that
    /// runs in `dir`.
    pub(super) fn from_env(dir: &Path) -> Self {
        let program = std::env::var_os(PROGRAM_VARIABLE)
            .filter(|program| !program.is_empty())
            .unwrap_or_else(|| OsString::from("bd"));
        Self::with_program(program, dir)
    }

    /// The runner for `program`, that runs in `dir`.
    pub(super) fn with_program(program: impl Into<OsString>, dir: &Path) -> Self {
        Self {
            program: program.into(),
            dir: dir.to_path_buf(),
        }
    }

    /// The command that runs the program with `args` in the install's
    /// directory, with no input and with the usage metrics of `bd` off.
    fn command<I, S>(&self, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .envs(METRICS_OFF);
        command
    }

    /// Runs the program with `args`, with no input, and returns its stdout.
    pub(super) fn run<I, S>(&self, args: I) -> Result<String, BdError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = self
            .command(args)
            .output()
            .map_err(|error| match error.kind() {
                ErrorKind::NotFound => BdError::NotInstalled,
                _ => BdError::Failed(error.to_string()),
            })?;
        if output.status.success() {
            return String::from_utf8(output.stdout)
                .map_err(|_| BdError::Failed("bd printed output that is not UTF-8".to_string()));
        }
        Err(classify(
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        ))
    }

    /// Runs the program with `args` and parses its stdout as one JSON value.
    pub(super) fn json<I, S>(&self, args: I) -> Result<Value, BdError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let stdout = self.run(args)?;
        serde_json::from_str(&stdout)
            .map_err(|error| BdError::Output(format!("output that is no JSON ({error})")))
    }
}

/// Why a call to `bd` failed: one cause of spec 3.4, or a reply that the
/// adapter cannot read.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum BdError {
    /// The program is not on `PATH`.
    NotInstalled,
    /// The directory holds no Beads store.
    NoStore,
    /// Any other failure: the first stderr line that is no warning, or the exit
    /// status when stderr is empty.
    Failed(String),
    /// `bd` exited 0 and printed what the adapter cannot read; the text says
    /// what `bd` returned.
    Output(String),
}

impl BdError {
    /// The error of the tracker (spec 3.4).
    pub(super) fn into_tracker_error(self) -> TrackerError {
        let (cause, setup) = match self {
            BdError::NotInstalled => ("bd is not installed".to_string(), "install bd".to_string()),
            BdError::NoStore => (
                "no Beads store".to_string(),
                "run bd bootstrap in a clone, or bd init for a new store".to_string(),
            ),
            BdError::Failed(line) => (
                line,
                "run the command by hand to see the full error".to_string(),
            ),
            BdError::Output(what) => {
                return TrackerError::Command(format!("bd returned {what}"));
            }
        };
        TrackerError::Unreachable {
            adapter: "beads",
            cause,
            setup,
        }
    }
}

/// `true` when `line` opens a warning of `bd`: `Warning: ...` and `warning:
/// ...`. `bd` prints its warnings before the error that ends a call, and a
/// warning may go on in indented lines.
fn is_warning(line: &str) -> bool {
    line.get(..7)
        .is_some_and(|start| start.eq_ignore_ascii_case("warning"))
}

/// The first line of `stderr` that is no part of a warning: no warning, and no
/// indented line that follows one. Blank lines count for nothing.
fn first_error_line(stderr: &str) -> Option<&str> {
    let mut in_warning = false;
    for raw in stderr.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if is_warning(line) {
            in_warning = true;
        } else if in_warning && raw.starts_with(char::is_whitespace) {
            continue;
        } else {
            return Some(line);
        }
    }
    None
}

/// The cause of a call that ended with `code` (`None` when a signal stopped
/// it) and printed `stderr`.
fn classify(code: Option<i32>, stderr: &str) -> BdError {
    if stderr.contains(NO_STORE_SENTENCE) {
        return BdError::NoStore;
    }
    let first = stderr.lines().map(str::trim).find(|line| !line.is_empty());
    let line = first_error_line(stderr).or(first);
    BdError::Failed(match (line, code) {
        (Some(line), _) => line.to_string(),
        (None, Some(code)) => format!("bd exited with status {code}"),
        (None, None) => "bd was stopped by a signal".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    /// Spec 3.4: a directory with no store, and a clone before `bd bootstrap`,
    /// are "no Beads store", whatever warnings come first.
    #[test]
    fn a_directory_with_no_store_is_no_beads_store() {
        for stderr in [
            "Error: no beads database found\nHint: run 'bd where' to inspect the resolved workspace, or 'bd init' to create a new database\n",
            "Warning: .beads has permissions 0755 (recommended: 0700). Run: chmod 700 .beads\nError: no beads database found\n",
        ] {
            assert_eq!(classify(Some(1), stderr), BdError::NoStore, "{stderr}");
        }
    }

    /// Spec 3.4: any other failure is the first stderr line that is no part of
    /// a warning, the indented lines that go on a warning included (the role
    /// warning of `bd` has two); a call that printed only warnings names the
    /// first of them; a call that printed nothing names its exit status.
    #[test]
    fn any_other_failure_is_the_first_line_that_is_no_warning_or_the_exit_status() {
        assert_eq!(
            classify(Some(1), "\n  boom: it broke \nsecond\n"),
            BdError::Failed("boom: it broke".to_string())
        );
        assert_eq!(
            classify(
                Some(1),
                "Warning: one\nwarning: beads.role not configured (GH#2950).\n  Fix: git config beads.role maintainer\n  Or:  git config beads.role contributor\n\nError: it broke\n"
            ),
            BdError::Failed("Error: it broke".to_string())
        );
        assert_eq!(
            classify(
                Some(1),
                "warning: only a warning\n  Fix: and its second line\n"
            ),
            BdError::Failed("warning: only a warning".to_string())
        );
        assert_eq!(
            classify(Some(3), " \n"),
            BdError::Failed("bd exited with status 3".to_string())
        );
        assert_eq!(
            classify(None, ""),
            BdError::Failed("bd was stopped by a signal".to_string())
        );
    }

    /// A line that is shorter than the word "warning", or that holds a
    /// multi-byte character where the word would end, is no warning and does
    /// not stop the reader.
    #[test]
    fn a_short_line_and_a_multi_byte_line_are_no_warning() {
        for line in ["", "w", "warn", "wärning", "ünicode", "日本語のエラー"] {
            assert!(!is_warning(line), "{line:?}");
        }
        assert!(is_warning("Warning: x") && is_warning("warning: x"));
    }

    /// Spec 3.4: each cause prints its cause and its setup step; a reply that
    /// the adapter cannot read is a named fault, not an unreachable store.
    #[test]
    fn each_cause_names_its_setup_step() {
        let error = |cause: BdError| cause.into_tracker_error().to_string();
        assert_eq!(
            error(BdError::NotInstalled),
            "tracker beads cannot be reached: bd is not installed; install bd"
        );
        assert_eq!(
            error(BdError::NoStore),
            "tracker beads cannot be reached: no Beads store; run bd bootstrap in a clone, or bd init for a new store"
        );
        assert_eq!(
            error(BdError::Failed("boom".to_string())),
            "tracker beads cannot be reached: boom; run the command by hand to see the full error"
        );
        assert_eq!(
            error(BdError::Output("a list that is no JSON (eof)".to_string())),
            "bd returned a list that is no JSON (eof)"
        );
    }

    /// Ruling 188: the command of every call turns the usage metrics of `bd`
    /// off, whatever the environment that houserules runs in.
    #[test]
    fn every_call_turns_the_usage_metrics_off() {
        let runner = Runner::with_program("bd", &std::env::temp_dir());
        let command = runner.command(["list"]);
        let set: Vec<(&OsStr, Option<&OsStr>)> = command.get_envs().collect();
        assert_eq!(
            set,
            [
                (OsStr::new("BD_DISABLE_METRICS"), Some(OsStr::new("1"))),
                (OsStr::new("DO_NOT_TRACK"), Some(OsStr::new("1"))),
            ]
        );
    }

    /// A program that does not exist is "not installed".
    #[test]
    fn a_program_that_does_not_exist_is_not_installed() {
        let runner = Runner::with_program("houserules-no-such-bd-program", &std::env::temp_dir());
        assert_eq!(runner.run(["list"]), Err(BdError::NotInstalled));
    }
}
