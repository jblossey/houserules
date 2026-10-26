//! The `intent-required` and `intent-resolves` check types (batch 38, spec 5.5
//! and 5.6; HR-190). Both are deterministic, and neither can declare `covers`.
//! `runner/intent.rs` turns the [`Outcome`] of each into an audit row.
//!
//! # `intent-required`
//!
//! The check's paths are the union of the paths that each commit of the range
//! contributes: a non-merge commit its diff against its first parent (a root
//! commit all its paths; a deleted path by its old path, a rename by both
//! names), a merge commit nothing, and a commit whose subject matches a pattern
//! of `exempt_subjects` nothing ([`changed_files_for`] with
//! [`RootCommit::EveryPath`]). When no such path matches `if`, the check does
//! not apply ([`Outcome::NotApplicable`]). Otherwise it holds when an intent of
//! `sources` exists for the range: first an `Intent:` trailer of any commit
//! whose source is in `sources` and which resolves at the head, oldest commit
//! first, then the intents that the adapters of `sources` find from the changed
//! paths of the whole range (`review::intent::found_in` with the `Analysis` of
//! `review::change::analyze`, the list that the packet prints). The trailers
//! come first so that the common pass reads no analysis; the analysis, and the
//! git of 2.41 or later that it needs, are read only when the check applies and
//! no trailer resolves.
//!
//! # `intent-resolves`
//!
//! The check fails for each `Intent:` trailer of the range whose intent
//! resolves neither at the head nor at the merge base: a change that the range
//! archived resolves at the head, and one that the range removed at the merge
//! base. The trailers come from `review::links::read`, which `evidence` and
//! `record write` read too.
//!
//! # Evidence
//!
//! The evidence of a row holds values that a commit message wrote: the value of
//! an `Intent:` trailer and the reference of an intent. Each prints through
//! `review::text::plain_or_json_string`, the text of a code span: a value with a
//! control character, or one that starts with a double quote, prints as a JSON
//! string, and every other value prints as it is. So the evidence holds no
//! control character, and a consumer that decodes the JSON of `audit` and
//! prints the evidence writes none to a terminal. A printed value starts with a
//! double quote exactly when it is a JSON string: the escape byte prints as
//! `"x\u001by"`, the text `"x\u001by"` prints as `"\"x\\u001by\""`, and the
//! text `x\u001by` prints as it is, so the three differ in the evidence and in
//! the packet's rows.
//!
//! # Settings of git
//!
//! The paths of a commit come from [`changed_files_for`], whose doc comment
//! names the setting that each option of `git diff-tree` pins. A merge prints no
//! path without an option that diffs it against a parent (`-m`, or
//! `--diff-merges=<format>`, for example `--diff-merges=first-parent`), which is
//! the contribution that spec 5.5 gives it.
//!
//! # Failures
//!
//! A failure that is no verdict is a named `Err` and exit 2
//! (`houserules.crash-paths-are-named`): a `sources` entry that is no spec
//! adapter or a pattern of `exempt_subjects` that does not compile (the entry's
//! own line, from [`Required::compile`]); a glob of `if` that does not compile;
//! the line of git for a commit that it cannot read; the marker's lines for a
//! `review` key that is malformed; `audit: git <v> is older than 2.41, which
//! --attr-source needs`; and the failures of the adapters and of the trailers.

use std::collections::BTreeSet;
use std::path::Path;

use regress::Regex;

use crate::install::marker::Marker;
use crate::install::marker::review::{Adapter, ReviewConfig};
use crate::published::intent::Intent;
use crate::review::change::analyze;
use crate::review::git::require_attr_source;
use crate::review::intent::{found_in, resolve_named};
use crate::review::links;
use crate::review::text::plain_or_json_string;

use super::check_shape::CheckDef;
use super::git::{
    Commit, RootCommit, changed_files_for, command, is_empty_tree, resolve_commit, run_command,
    stderr_headline,
};
use super::git_paths::InstallPrefix;
use super::model::Base;
use super::runner::{compile_check_regex, filter_matching};

/// The adapters that `sources` may name: every adapter whose intents are
/// specs or plans. The work item is no spec.
pub(super) const SPEC_ADAPTERS: [Adapter; 2] = [Adapter::File, Adapter::OpenSpec];

/// The evidence of `intent-resolves` for a range with no `Intent:` trailer.
const NO_TRAILER: &str = "no Intent: trailer in the range";

/// The evidence of an `intent-required` check that does not apply.
const NOT_APPLICABLE: &str = "no changed path matches the check's if";

/// What a check decided for a range.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// The check does not apply to the range: its row is `skipped`, and the
    /// packet counts it among the checks that did not apply.
    NotApplicable(String),
    /// The check holds: the evidence of its `pass` row.
    Holds(String),
    /// The check is violated: the evidence of its `fail` or `warn` row.
    Violated(String),
}

/// What both checks read of an audit range.
pub(super) struct Range<'a> {
    /// The knowledge base of the install, which holds its root.
    pub(super) knowledge: &'a Base,
    /// The prefix of the install below the git top level.
    pub(super) prefix: &'a InstallPrefix,
    /// The abbreviated sha that the range starts after, or the name of git's
    /// empty tree for a range from the start of the history.
    pub(super) base: &'a str,
    /// The abbreviated sha of the head.
    pub(super) head: &'a str,
    /// The commits of the range, with their trailers.
    pub(super) commits: &'a [Commit],
}

/// `true` when `name` is the name of a spec adapter.
pub(super) fn is_spec_adapter(name: &str) -> bool {
    SPEC_ADAPTERS.iter().any(|adapter| adapter.name() == name)
}

/// The names of the spec adapters as a line writes them: `file or openspec`.
pub(super) fn spec_adapter_names() -> String {
    SPEC_ADAPTERS
        .iter()
        .map(|adapter| adapter.name())
        .collect::<Vec<_>>()
        .join(" or ")
}

/// The line for a `sources` entry that is no spec adapter, without the entry
/// that it belongs to.
pub(super) fn no_spec_adapter_line(source: &str) -> String {
    format!(
        "check sources \"{source}\" is no spec adapter; use {}",
        spec_adapter_names()
    )
}

/// The adapters that the check reads, in the order of its `sources`. An absent
/// `sources` and an empty list read as both spec adapters. A source that is no
/// spec adapter is the `Err` of [`no_spec_adapter_line`].
fn sources_of(check: &CheckDef) -> Result<Vec<Adapter>, String> {
    let listed = check.sources.as_deref().unwrap_or_default();
    if listed.is_empty() {
        return Ok(SPEC_ADAPTERS.to_vec());
    }
    listed
        .iter()
        .map(|source| {
            SPEC_ADAPTERS
                .iter()
                .copied()
                .find(|adapter| adapter.name() == source)
                .ok_or_else(|| no_spec_adapter_line(source))
        })
        .collect()
}

/// The names of `adapters` as the evidence writes them: `file, openspec`.
fn names_of(adapters: &[Adapter]) -> String {
    adapters
        .iter()
        .map(|adapter| adapter.name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// `1 changed path` or `<n> changed paths`.
fn changed_paths(count: usize) -> String {
    if count == 1 {
        "1 changed path".to_string()
    } else {
        format!("{count} changed paths")
    }
}

/// The full ids of the two ends of a range in the install of `knowledge`: the
/// base stays the name of git's empty tree for a range from the start of the
/// history.
fn commit_ids(knowledge: &Base, base: &str, head: &str) -> Result<(String, String), String> {
    let root = &knowledge.root;
    let head = resolve_commit(root, head)?;
    let base = if is_empty_tree(base) {
        base.to_string()
    } else {
        resolve_commit(root, base)?
    };
    Ok((base, head))
}

/// The `review` key of the marker of the install at `root`: the lines of its
/// faults are the `Err`.
fn review_config(root: &Path) -> Result<ReviewConfig, String> {
    let marker = Marker::read_or_default_for_gate(root, "WI")?;
    marker.review().map_err(|lines| lines.join("\n"))
}

/// An `intent-required` check, compiled: the adapters of its `sources` and the
/// patterns of its `exempt_subjects`, so that a malformed field is one
/// named error before any git call.
pub(super) struct Required<'a> {
    check: &'a CheckDef,
    sources: Vec<Adapter>,
    exempt: Vec<Regex>,
}

impl<'a> Required<'a> {
    /// Compiles `check`: its `sources` ([`sources_of`]) and each pattern of
    /// `exempt_subjects` under the check's `flags` (`g` and `y` stripped, as
    /// for a `commits` check). The `Err` is the line of the first fault.
    pub(super) fn compile(check: &'a CheckDef) -> Result<Self, String> {
        let flags = check.flags.as_deref().unwrap_or_default();
        let exempt = check
            .exempt_subjects
            .iter()
            .flatten()
            .map(|pattern| compile_check_regex(pattern, flags))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            check,
            sources: sources_of(check)?,
            exempt,
        })
    }

    /// Judges the range (the module doc has the rule).
    pub(super) fn judge(&self, range: &Range) -> Result<Outcome, String> {
        let matching = filter_matching(&self.paths(range)?, &self.check.if_changed)?;
        if matching.is_empty() {
            return Ok(Outcome::NotApplicable(NOT_APPLICABLE.to_string()));
        }
        let counted = changed_paths(matching.len());
        let holds = |intent: &Intent| {
            Outcome::Holds(holds_evidence(&intent.source, &intent.reference, &counted))
        };
        let root = &range.knowledge.root;
        let (base, head) = commit_ids(range.knowledge, range.base, range.head)?;
        if let Some(intent) = self.trailer_intent(root, &base, &head)? {
            return Ok(holds(&intent));
        }
        let config = review_config(root)?;
        require_attr_source("audit")?;
        let analysis = analyze(root, range.knowledge, &config, &base, &head)?;
        let found = found_in(root, &analysis, &config.intent, &self.sources)?;
        Ok(match found.first() {
            Some(intent) => holds(intent),
            None => Outcome::Violated(format!(
                "no intent of {} for {counted} that match the check's if",
                names_of(&self.sources)
            )),
        })
    }

    /// The paths of the check: each non-exempt commit's contribution, once.
    fn paths(&self, range: &Range) -> Result<Vec<String>, String> {
        let mut paths = BTreeSet::new();
        for commit in range.commits {
            if self
                .exempt
                .iter()
                .any(|pattern| pattern.find(&commit.subject).is_some())
            {
                continue;
            }
            paths.extend(changed_files_for(
                &range.knowledge.root,
                range.prefix,
                &commit.sha,
                RootCommit::EveryPath,
            )?);
        }
        Ok(paths.into_iter().collect())
    }

    /// The first intent, oldest commit first, that an `Intent:` trailer of the
    /// range names with a source of `sources` and that resolves at `head`.
    fn trailer_intent(
        &self,
        root: &Path,
        base: &str,
        head: &str,
    ) -> Result<Option<Intent>, String> {
        for trailer in links::read(root, base, head, &[])?.intents {
            let named = trailer.intent_ref();
            let Some(adapter) = Adapter::named(&named.source) else {
                continue;
            };
            if !self.sources.contains(&adapter) {
                continue;
            }
            let intent = resolve_named(root, head, &named)?;
            if intent.unresolved.is_none() {
                return Ok(Some(intent));
            }
        }
        Ok(None)
    }
}

/// Judges an `intent-resolves` check over the range from `base` to `head` in
/// the install of `knowledge` (the module doc has the rule). It reads the
/// trailers of the range through `review::links::read` and resolves each intent
/// at the two ends, so it needs no commit list. Each failing trailer is one
/// line, `<sha7> Intent: <value>: <line>`, oldest commit first and in message
/// order, joined by line feeds; the line is the adapter's line at the head.
pub(super) fn judge_resolves(knowledge: &Base, base: &str, head: &str) -> Result<Outcome, String> {
    let root = &knowledge.root;
    let (base, head) = commit_ids(knowledge, base, head)?;
    let trailers = links::read(root, &base, &head, &[])?.intents;
    if trailers.is_empty() {
        return Ok(Outcome::Holds(NO_TRAILER.to_string()));
    }
    let merge_base = merge_base(root, &base, &head)?;
    let mut failures = Vec::new();
    for trailer in &trailers {
        let named = trailer.intent_ref();
        let Some(line) = resolve_named(root, &head, &named)?.unresolved else {
            continue;
        };
        if let Some(merge_base) = &merge_base
            && resolve_named(root, merge_base, &named)?
                .unresolved
                .is_none()
        {
            continue;
        }
        failures.push(failure_line(&trailer.commit, &trailer.value, &line));
    }
    Ok(if failures.is_empty() {
        let count = trailers.len();
        Outcome::Holds(if count == 1 {
            "1 Intent: trailer resolves".to_string()
        } else {
            format!("{count} Intent: trailers resolve")
        })
    } else {
        Outcome::Violated(failures.join("\n"))
    })
}

/// The evidence of an `intent-required` check that holds by the intent
/// `<source>:<reference>`: `intent <source>:<reference> for <counted> that match
/// the check's if`. The reference comes from a commit message or a file name, so
/// an intent with a control character, or one that starts with a double quote,
/// prints as a JSON string (`review::text::plain_or_json_string`).
fn holds_evidence(source: &str, reference: &str, counted: &str) -> String {
    format!(
        "intent {} for {counted} that match the check's if",
        plain_or_json_string(&format!("{source}:{reference}"))
    )
}

/// The line of a failing `intent-resolves` trailer: `<sha7> Intent: <value>:
/// <line>`, with `commit` the full sha of the commit that holds the trailer and
/// `line` the adapter's line. The value comes from a commit message, so a value
/// with a control character, or one that starts with a double quote, prints as
/// a JSON string (`review::text::plain_or_json_string`).
fn failure_line(commit: &str, value: &str, line: &str) -> String {
    format!(
        "{} Intent: {}: {line}",
        commit.get(..7).unwrap_or(commit),
        plain_or_json_string(value)
    )
}

/// The full sha of the merge base of `base` and `head`, or `None` for a range
/// from git's empty tree and for commits that share no history.
fn merge_base(root: &Path, base: &str, head: &str) -> Result<Option<String>, String> {
    if is_empty_tree(base) {
        return Ok(None);
    }
    let output = run_command(command(root).args(["merge-base", base, head]), Some(1))
        .map_err(|error| stderr_headline(&error))?;
    let text = String::from_utf8_lossy(&output);
    let found = text.trim();
    Ok((!found.is_empty()).then(|| found.to_string()))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::install::marker::review::Adapter;
    use crate::rules::check_shape::CheckDef;

    /// A `check` that `value` describes.
    fn check(value: serde_json::Value) -> CheckDef {
        serde_json::from_value(value).expect("a check")
    }

    /// The spec adapters are the adapters that the check types name for
    /// `sources`: every adapter but the work item, which is no spec. The list
    /// derives from `Adapter::ALL`, so an adapter added later takes a side here
    /// before a check can name it.
    #[test]
    fn the_spec_adapters_are_every_adapter_but_the_work_item() {
        let derived: Vec<Adapter> = Adapter::ALL
            .into_iter()
            .filter(|adapter| *adapter != Adapter::WorkItem)
            .collect();

        assert_eq!(SPEC_ADAPTERS.to_vec(), derived);
        assert_eq!(spec_adapter_names(), "file or openspec");
        assert!(is_spec_adapter("file"));
        assert!(is_spec_adapter("openspec"));
        assert!(!is_spec_adapter("work-item"));
        assert!(!is_spec_adapter("File"));
    }

    /// No `sources`, and an empty list, read as both spec adapters; a listed
    /// source keeps the order of the list.
    #[test]
    fn an_absent_or_empty_sources_list_reads_as_the_default() {
        let default = vec![Adapter::File, Adapter::OpenSpec];
        let required = |extra: serde_json::Value| {
            let mut value = json!({"type": "intent-required", "level": "fail", "if": "**"});
            value
                .as_object_mut()
                .expect("an object")
                .extend(extra.as_object().expect("an object").clone());
            check(value)
        };

        assert_eq!(sources_of(&required(json!({}))), Ok(default.clone()));
        assert_eq!(sources_of(&required(json!({"sources": []}))), Ok(default));
        assert_eq!(
            sources_of(&required(json!({"sources": ["openspec", "file"]}))),
            Ok(vec![Adapter::OpenSpec, Adapter::File])
        );
    }

    /// The evidence of a failing trailer writes a value with a control character
    /// as a JSON string (`review::text::plain_or_json_string`, the form of a code
    /// span): the escape, the delete character, the C1 control `U+009B`, which a
    /// terminal reads as a control sequence, and a tab reach no consumer that
    /// prints the evidence raw.
    #[test]
    fn the_evidence_of_a_failing_trailer_holds_no_control_character() {
        let line = failure_line(
            "0123456789abcdef",
            "oops\u{1b}[31m\u{7f}\u{9b}",
            "an intent is written <source>:<ref>",
        );

        assert_eq!(
            line,
            "0123456 Intent: \"oops\\u001b[31m\\u007f\\u009b\": an intent is written <source>:<ref>"
        );
        assert!(!line.chars().any(char::is_control), "{line:?}");
        assert_eq!(
            failure_line("0123456789abcdef", "file:docs/a\tb.md", "line"),
            "0123456 Intent: \"file:docs/a\\tb.md\": line"
        );
    }

    /// The evidence of a held intent writes the identity of a reference with a
    /// control character as a JSON string, as the evidence of a failing trailer
    /// does.
    #[test]
    fn the_evidence_of_a_held_intent_holds_no_control_character() {
        let evidence = holds_evidence("file", "docs/a\u{1b}b\u{9b}.md", "1 changed path");

        assert_eq!(
            evidence,
            "intent \"file:docs/a\\u001bb\\u009b.md\" for 1 changed path that match the check's if"
        );
        assert!(!evidence.chars().any(char::is_control), "{evidence:?}");
    }

    /// A value with an escape byte and a value that spells the six characters of
    /// its escape print as two different lines, in the evidence of a failing
    /// trailer and in the evidence of a held intent.
    #[test]
    fn the_evidence_tells_a_control_character_from_text_that_spells_its_escape() {
        let failing = |value: &str| failure_line("0123456789abcdef", value, "line");
        let holding = |reference: &str| holds_evidence("file", reference, "1 changed path");

        assert_eq!(failing("x\u{1b}y"), "0123456 Intent: \"x\\u001by\": line");
        assert_eq!(failing("x\\u001by"), "0123456 Intent: x\\u001by: line");
        assert_ne!(failing("x\u{1b}y"), failing("x\\u001by"));
        assert_ne!(holding("a\u{1b}b"), holding("a\\u001bb"));
        assert_eq!(
            holding("a\\u001bb"),
            "intent file:a\\u001bb for 1 changed path that match the check's if"
        );
    }

    /// A source that is no spec adapter is a named error, with the line of
    /// `check-knowledge` (the work item included).
    #[test]
    fn a_source_that_is_no_spec_adapter_is_a_named_error() {
        let definition = check(json!({
            "type": "intent-required", "level": "fail", "if": "**",
            "sources": ["file", "work-item"],
        }));

        assert_eq!(
            sources_of(&definition),
            Err("check sources \"work-item\" is no spec adapter; use file or openspec".to_string())
        );
    }
}
