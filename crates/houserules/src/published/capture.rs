//! The `capture` document: the header on line 1 of a capture file.

use schemars::JsonSchema;
use serde::Serialize;

use super::Published;

/// The header of a capture file: line 1, one JSON object whose keys follow the
/// order of the fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[schemars(title = "Capture header")]
pub(crate) struct Header {
    /// The schema id, `houserules/capture@1.0.0` when `capture` writes it.
    pub(crate) schema: String,
    /// The command and its arguments.
    pub(crate) argv: Vec<String>,
    /// The absolute current directory of the run.
    pub(crate) cwd: String,
    /// The start of the run: RFC 3339 in UTC with nine fractional digits.
    pub(crate) start: String,
    /// The end of the run, in the form of `start`.
    pub(crate) end: String,
    /// The exit code, or `null` when a signal ended the command.
    pub(crate) exit: Option<i32>,
    /// The signal number that ended the command, or `null`.
    pub(crate) signal: Option<i32>,
    /// The full sha of HEAD, or `null` outside a git work tree (a `.git`
    /// directory and a bare repository count as outside), before the first
    /// commit, and when `git status` fails.
    pub(crate) head: Option<String>,
    /// Whether `git status --porcelain` printed a line; `null` exactly when
    /// `head` is `null`.
    pub(crate) dirty: Option<bool>,
    /// The number of byte sequences that were not UTF-8 and that `capture`
    /// replaced with U+FFFD.
    pub(crate) lossy: u64,
}

impl Published for Header {
    const NAME: &'static str = "capture";
    const VERSION: &'static str = "1.0.0";
}
