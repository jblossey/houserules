//! `houserules record list` (spec 8.3): one line for each record of a range.
//!
//! The line is `<key7> <route> <n> reviewers <sampled|-> <pass|fail|unaudited>`:
//! the first seven characters of the key, the route of the record, the number of
//! its reviewers (the names left out of the list included), `sampled` for a
//! record that its draw selected, else `-`, and the verdict of the post-merge
//! audit, else `unaudited`. `--json` prints the same rows as objects.

use std::path::{Path, PathBuf};

use clap::Args;
use serde_json::{Value, json};

use super::super::change;
use super::super::git;
use super::document::verdict_word;
use super::notes;
use super::range::{self, Entry};
use super::{Done, Failure, no_records_line};
use crate::emit::emit;

/// The arguments of `houserules record list`.
#[derive(Args)]
pub(crate) struct ListArgs {
    /// A git revision range, such as `v1.0..main`. The command keeps the records
    /// whose key is in the range. Without it, the command lists every record.
    #[arg(value_name = "RANGE")]
    range: Option<String>,
    /// Keeps the records whose post-merge sample draw selected them.
    #[arg(long)]
    sampled: bool,
    /// Prints a list of objects in place of the lines.
    #[arg(long)]
    json: bool,
    /// Install root to read from; defaults to the nearest directory at or above
    /// the current one that holds `.houserules.json`, no higher than the git top
    /// level, else the git top level.
    #[arg(long)]
    pub(super) dir: Option<PathBuf>,
}

/// The verdict word of the post-merge audit of `entry`: `pass`, `fail`, or
/// `unaudited`.
fn audit_word(entry: &Entry) -> &'static str {
    entry
        .record
        .post_merge_audit
        .as_ref()
        .map_or("unaudited", |audit| verdict_word(audit.verdict))
}

/// The reviewers of `entry`: the names of its list and the names left out.
fn reviewer_count(entry: &Entry) -> u64 {
    entry.record.reviewers.len() as u64 + entry.record.reviewers_more.unwrap_or(0)
}

/// Whether the draw of the record of `entry` selected it.
fn is_sampled(entry: &Entry) -> bool {
    entry.record.sample.is_some_and(|sample| sample.sampled)
}

/// The line of `entry`.
fn line_of(entry: &Entry) -> String {
    format!(
        "{} {} {} reviewers {} {}\n",
        git::short(&entry.key),
        entry.record.route.route.as_str(),
        reviewer_count(entry),
        if is_sampled(entry) { "sampled" } else { "-" },
        audit_word(entry)
    )
}

/// The object of `entry` for `--json`.
fn object_of(entry: &Entry) -> Value {
    json!({
        "key": entry.key,
        "route": entry.record.route.route.as_str(),
        "reviewers": reviewer_count(entry),
        "sampled": is_sampled(entry),
        "audit": audit_word(entry),
    })
}

/// Runs `record list`: reads the records of the range, keeps the sampled ones
/// for `--sampled`, and prints them. A clone without the notes ref gets the line
/// of spec 8.1.
pub(super) fn run(root: &Path, args: &ListArgs) -> Result<Done, Failure> {
    let install = change::install_path(root)?;
    if !notes::ref_exists(root, &install)? {
        return Err(Failure::refused(vec![no_records_line(&install)]));
    }
    let mut entries = range::records(root, &install, args.range.as_deref())?;
    if args.sampled {
        entries.retain(is_sampled);
    }
    let stdout = if args.json {
        emit(&Value::Array(entries.iter().map(object_of).collect()))
    } else {
        entries.iter().map(line_of).collect()
    };
    Ok(Done {
        stdout,
        stderr: Vec::new(),
    })
}
