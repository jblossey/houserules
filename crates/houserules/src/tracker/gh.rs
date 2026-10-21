//! The `gh` runner of the GitHub adapter (spec 3.4, 3.5, and 3.7).
//!
//! [`Runner`] spawns `gh` from `PATH`, or the program that the variable
//! `HOUSERULES_GH` names: the tests point it at `fake-gh`. Each argument is
//! one argv entry and no shell runs. houserules never reads, takes, or stores
//! a token: `gh` keeps its own authentication, and the child inherits the
//! environment unchanged.
//!
//! A failed call is a [`GhError`], which names one of the causes of spec 3.4.
//! The cause comes from the exit status and the first stderr line of `gh`
//! only, so a new `gh` that words an error otherwise is an "other failure"
//! with its first line, never a wrong cause.

use std::ffi::OsString;
use std::io::ErrorKind;
use std::process::{Command, Stdio};

use serde_json::Value;

use super::TrackerError;

/// The variable that names the program to run in place of `gh`.
const PROGRAM_VARIABLE: &str = "HOUSERULES_GH";

/// The exit status that `gh` gives a command that needs authentication
/// (`gh help exit-codes`).
const EXIT_AUTH_REQUIRED: i32 = 4;

/// Spawns `gh` for the GitHub adapter.
pub(super) struct Runner {
    program: OsString,
}

impl Runner {
    /// The runner for the program that `HOUSERULES_GH` names, else `gh`.
    pub(super) fn from_env() -> Self {
        let program = std::env::var_os(PROGRAM_VARIABLE)
            .filter(|program| !program.is_empty())
            .unwrap_or_else(|| OsString::from("gh"));
        Self { program }
    }

    /// Runs the program with `args`, with no input, and returns its stdout.
    pub(super) fn run<I, S>(&self, args: I) -> Result<String, GhError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = Command::new(&self.program)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| match error.kind() {
                ErrorKind::NotFound => GhError::NotInstalled,
                _ => GhError::Failed(error.to_string()),
            })?;
        if output.status.success() {
            return String::from_utf8(output.stdout)
                .map_err(|_| GhError::Failed("gh printed output that is not UTF-8".to_string()));
        }
        Err(classify(
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        ))
    }
}

/// Why a call to `gh` failed: one cause of spec 3.4.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum GhError {
    /// The program is not on `PATH`.
    NotInstalled,
    /// `gh` has no login for the host.
    NotLoggedIn,
    /// `gh` answers that the repository is not there for its login.
    CannotSeeRepository,
    /// Any other failure: the first line of stderr, or the exit status when
    /// stderr is empty.
    Failed(String),
}

impl GhError {
    /// The error of a tracker whose repository is `repo` (spec 3.4).
    pub(super) fn into_tracker_error(self, repo: &str) -> TrackerError {
        let (cause, setup) = match self {
            GhError::NotInstalled => ("gh is not installed".to_string(), "install gh".to_string()),
            GhError::NotLoggedIn => (
                "gh is not logged in".to_string(),
                "run gh auth login".to_string(),
            ),
            GhError::CannotSeeRepository => (
                format!("gh cannot see {repo}"),
                "check tracker.repo in .houserules.json and gh auth status".to_string(),
            ),
            GhError::Failed(line) => (
                line,
                "run the command by hand to see the full error".to_string(),
            ),
        };
        TrackerError::Unreachable {
            adapter: "github",
            cause,
            setup,
        }
    }
}

/// The cause of a call that ended with `code` (`None` when a signal stopped
/// it) and printed `stderr`.
fn classify(code: Option<i32>, stderr: &str) -> GhError {
    if code == Some(EXIT_AUTH_REQUIRED)
        || stderr.contains("gh auth login")
        || stderr.contains("HTTP 401")
    {
        return GhError::NotLoggedIn;
    }
    if stderr.contains("HTTP 404") || stderr.contains("Could not resolve to a Repository") {
        return GhError::CannotSeeRepository;
    }
    let line = stderr.lines().map(str::trim).find(|line| !line.is_empty());
    GhError::Failed(match (line, code) {
        (Some(line), _) => line.to_string(),
        (None, Some(code)) => format!("gh exited with status {code}"),
        (None, None) => "gh was stopped by a signal".to_string(),
    })
}

/// The rows that `gh api --paginate` printed for a list endpoint: one JSON
/// array, or one array for each page. A text that holds anything else is a
/// named fault.
pub(super) fn parse_rows(stdout: &str) -> Result<Vec<Value>, TrackerError> {
    let fault = |what: &str| TrackerError::Command(format!("gh returned {what}"));
    let mut rows = Vec::new();
    for page in serde_json::Deserializer::from_str(stdout).into_iter::<Value>() {
        match page.map_err(|error| fault(&format!("a list that is no JSON ({error})")))? {
            Value::Array(page) => rows.extend(page),
            _ => return Err(fault("a page that is no list")),
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// Spec 3.4: exit status 4, and the login hint of `gh`, are "not logged
    /// in".
    #[test]
    fn a_call_that_needs_authentication_is_not_logged_in() {
        for (code, stderr) in [
            (Some(4), ""),
            (
                Some(1),
                "To get started with GitHub CLI, please run:  gh auth login",
            ),
            (Some(1), "gh: Bad credentials (HTTP 401)"),
        ] {
            assert_eq!(classify(code, stderr), GhError::NotLoggedIn, "{stderr}");
        }
    }

    /// Spec 3.4: a 404 and an unresolved repository are "cannot see".
    #[test]
    fn a_missing_repository_is_cannot_see() {
        for stderr in [
            "gh: Not Found (HTTP 404)",
            "GraphQL: Could not resolve to a Repository with the name 'a/b'. (repository)",
        ] {
            assert_eq!(
                classify(Some(1), stderr),
                GhError::CannotSeeRepository,
                "{stderr}"
            );
        }
    }

    /// Spec 3.4: any other failure is its first stderr line, or its exit
    /// status when there is none.
    #[test]
    fn any_other_failure_is_the_first_stderr_line_or_the_exit_status() {
        assert_eq!(
            classify(Some(1), "\n  boom: it broke \nsecond\n"),
            GhError::Failed("boom: it broke".to_string())
        );
        assert_eq!(
            classify(Some(3), " \n"),
            GhError::Failed("gh exited with status 3".to_string())
        );
        assert_eq!(
            classify(None, ""),
            GhError::Failed("gh was stopped by a signal".to_string())
        );
    }

    /// Spec 3.4: each cause prints its cause and its setup step.
    #[test]
    fn each_cause_names_its_setup_step() {
        let error = |cause: GhError| cause.into_tracker_error("acme/widgets").to_string();
        assert_eq!(
            error(GhError::NotInstalled),
            "tracker github cannot be reached: gh is not installed; install gh"
        );
        assert_eq!(
            error(GhError::NotLoggedIn),
            "tracker github cannot be reached: gh is not logged in; run gh auth login"
        );
        assert_eq!(
            error(GhError::CannotSeeRepository),
            "tracker github cannot be reached: gh cannot see acme/widgets; check tracker.repo in .houserules.json and gh auth status"
        );
        assert_eq!(
            error(GhError::Failed("boom".to_string())),
            "tracker github cannot be reached: boom; run the command by hand to see the full error"
        );
    }

    /// A program that does not exist is "not installed".
    #[test]
    fn a_program_that_does_not_exist_is_not_installed() {
        let runner = Runner {
            program: OsString::from("houserules-no-such-gh-program"),
        };
        assert_eq!(runner.run(["api"]), Err(GhError::NotInstalled));
    }

    /// One array, one array for each page, and an empty list all read.
    #[test]
    fn a_list_reads_as_one_array_or_as_one_array_for_each_page() {
        assert_eq!(
            parse_rows("[{\"a\":1},{\"a\":2}]").expect("one array"),
            vec![json!({"a": 1}), json!({"a": 2})]
        );
        assert_eq!(
            parse_rows("[{\"a\":1}]\n[{\"a\":2}]\n").expect("two pages"),
            vec![json!({"a": 1}), json!({"a": 2})]
        );
        assert_eq!(parse_rows("[]").expect("empty"), Vec::<Value>::new());
        assert_eq!(parse_rows("").expect("no output"), Vec::<Value>::new());
    }

    /// Output that is no list of rows is a named fault.
    #[test]
    fn output_that_is_no_list_is_a_named_fault() {
        assert_eq!(
            parse_rows("{\"message\":\"x\"}"),
            Err(TrackerError::Command(
                "gh returned a page that is no list".to_string()
            ))
        );
        let Err(TrackerError::Command(line)) = parse_rows("<html>") else {
            panic!("a fault is expected");
        };
        assert!(
            line.starts_with("gh returned a list that is no JSON ("),
            "{line}"
        );
    }
}
