//! The `commits` check: the rules one commit message must satisfy
//! (`subject`, `body_absent`, `body_line_max`, `require_trailer`).
//! `CommitsCheck` is the one evaluation that both `audit` (the `run` of this
//! module) and `check-commit` call.

use regress::Regex;
use serde_json::Value;

use crate::rules::check_shape::{CheckDef, CheckLevel};
use crate::rules::git::Commit;
use crate::rules::trailers;

use super::{AuditContext, RowBuilder, compile_check_regex};

/// One rule that a commit breaks, as the `commits` checks report it.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::rules) struct Violation {
    evidence: String,
    /// `true` when `evidence` is a whole finding that names the check itself,
    /// so `line` prints it as it is.
    names_the_check: bool,
}

impl Violation {
    /// The evidence of an audit row.
    fn evidence(&self) -> &str {
        &self.evidence
    }

    /// The finding that `check-commit` prints for the check `id`: the
    /// evidence behind the id, except for a finding that names the check
    /// itself.
    pub(in crate::rules) fn line(&self, id: &str) -> String {
        if self.names_the_check {
            self.evidence.clone()
        } else {
            format!("{id}: {}", self.evidence)
        }
    }
}

/// One `commits`-type check, its `subject`, `body_absent`, and
/// `exempt_subjects` patterns compiled once -- the per-commit evaluation
/// that `run` of this module and `check_commit::check_commit` both run,
/// factored here so the two commands can never drift apart on what
/// counts as a violation or how it reads (`houserules check-commit` reuses
/// the audit's `commits` rules; it does not reimplement them).
/// `pub(in crate::rules)` rather than `pub(crate)`: `check_commit` is a
/// sibling module under `rules`, and nothing outside `rules` needs it.
pub(in crate::rules) struct CommitsCheck<'a> {
    id: &'a str,
    check: &'a CheckDef,
    subject_re: Option<Regex>,
    body_re: Option<Regex>,
    exempt_subject_res: Vec<Regex>,
}

impl<'a> CommitsCheck<'a> {
    /// Compiles the `subject`, `body_absent`, and `exempt_subjects` patterns
    /// of `check`, the check of the entry `id`, under its own `flags` (`g`/`y`
    /// stripped, `compile_check_regex`) -- a malformed pattern is a named
    /// `Err`, never a panic (`houserules.crash-paths-are-named`).
    pub(in crate::rules) fn compile(id: &'a str, check: &'a CheckDef) -> Result<Self, String> {
        let flags = check.flags.as_deref().unwrap_or_default();
        let subject_re = match &check.subject {
            Some(s) if !s.is_empty() => Some(compile_check_regex(s, flags)?),
            _ => None,
        };
        let body_re = match &check.body_absent {
            Some(s) if !s.is_empty() => Some(compile_check_regex(s, flags)?),
            _ => None,
        };
        let exempt_subject_res = check
            .exempt_subjects
            .iter()
            .flatten()
            .map(|pattern| compile_check_regex(pattern, flags))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            id,
            check,
            subject_re,
            body_re,
            exempt_subject_res,
        })
    }

    /// `true` when `commit` is exempt from the trailer rule: its author is
    /// one of `exempt_authors` by whole name, or its subject matches one of
    /// `exempt_subjects`. A commit with no author on record (the message
    /// file of the commit-msg hook) is exempt by subject alone.
    fn is_exempt_from_the_trailer(&self, commit: &Commit) -> bool {
        (!commit.author.is_empty()
            && self
                .check
                .exempt_authors
                .iter()
                .flatten()
                .any(|author| author == &commit.author))
            || self
                .exempt_subject_res
                .iter()
                .any(|re| re.find(&commit.subject).is_some())
    }

    /// Evaluates one commit against this check, in the same
    /// subject/body_absent/body_line_max/require_trailer order: `Some(violation)`
    /// for the first rule this commit breaks, `None` when the commit satisfies
    /// every rule this check declares. `violation` is the single place that
    /// builds this evidence text; both `run` of this module and
    /// `check_commit` call it, so the two commands' output cannot drift
    /// apart. A bot-authored commit (`[bot]` author suffix) is exempt
    /// from `body_line_max` alone: a generated body (Dependabot's compare
    /// links, say) carries lines no human wraps; its subject and
    /// `body_absent` rules still apply. `exempt_authors` and
    /// `exempt_subjects` exempt a commit from `require_trailer` alone. A
    /// trailer with no value names nothing, so it does not satisfy
    /// `require_trailer`.
    pub(in crate::rules) fn violation(&self, commit: &Commit) -> Option<Violation> {
        let (subject, body) = (commit.subject.as_str(), commit.body.as_str());
        let rule = |evidence: String| {
            Some(Violation {
                evidence,
                names_the_check: false,
            })
        };
        if let Some(re) = &self.subject_re
            && re.find(subject).is_none()
        {
            return rule(format!(
                "commit \"{subject}\" does not match {}",
                self.check.subject.as_deref().unwrap_or_default()
            ));
        }
        if let Some(re) = &self.body_re
            && body.split('\n').any(|line| re.find(line).is_some())
        {
            return rule(format!(
                "commit \"{subject}\" body matches {}",
                self.check.body_absent.as_deref().unwrap_or_default()
            ));
        }
        if let Some(limit) = self.check.body_line_max.filter(|&limit| limit > 0)
            && !commit.is_bot_authored()
            && body
                .split('\n')
                .any(|line| line.encode_utf16().count() as u64 > limit)
        {
            return rule(format!(
                "commit \"{subject}\" has a body line over {limit} characters"
            ));
        }
        if let Some(key) = &self.check.require_trailer
            && !self.is_exempt_from_the_trailer(commit)
            && !commit
                .trailers
                .iter()
                .any(|(name, value)| trailers::is_key(name, key) && !value.is_empty())
        {
            let sha = if commit.short_sha.is_empty() {
                String::new()
            } else {
                format!("{}: ", commit.short_sha)
            };
            return Some(Violation {
                evidence: format!("{sha}no {key} trailer ({})", self.id),
                names_the_check: true,
            });
        }
        None
    }

    /// This check's declared `level`: `check_commit`'s own outcome carries
    /// this forward onto each finding the same way `RowBuilder::violate`
    /// reads the level to decide a row's `result` (`"warn"` vs `"fail"`), so
    /// a warn-level `commits` check cannot silently escalate into a hard
    /// block on either surface.
    pub(in crate::rules) fn level(&self) -> CheckLevel {
        self.check.level
    }
}

/// Judges a `commits` check against every commit of the range: the row
/// names the first commit that breaks a rule.
pub(super) fn run(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<Value, String> {
    let compiled = CommitsCheck::compile(&rows.entry.id, check)?;
    let commits = ctx.commits()?;
    for commit in &commits {
        if let Some(violation) = compiled.violation(commit) {
            return Ok(rows.violate(violation.evidence().to_string()));
        }
    }
    Ok(rows.pass(format!("{} commits checked", commits.len())))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A `commits` check that requires the `Work-Item` trailer, with the
    /// fields of `extra` added.
    fn trailer_check(extra: serde_json::Value) -> CheckDef {
        let mut check = json!({"type": "commits", "level": "fail", "require_trailer": "Work-Item"});
        if let (Some(map), Some(more)) = (check.as_object_mut(), extra.as_object()) {
            map.extend(more.clone());
        }
        serde_json::from_value(check).expect("a valid commits check")
    }

    /// A commit by `author` with `subject` and `trailers`, abbreviated sha
    /// `abc1234`.
    fn commit_by(author: &str, subject: &str, trailers: &[(&str, &str)]) -> Commit {
        Commit {
            sha: "abc1234ffffffffffffffffffffffffffffffff".to_string(),
            short_sha: "abc1234".to_string(),
            subject: subject.to_string(),
            body: String::new(),
            author: author.to_string(),
            trailers: trailers
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect(),
        }
    }

    /// The finding that `check-commit` prints for `commit` under `check`, or
    /// none.
    fn finding(check: &CheckDef, commit: &Commit) -> Option<String> {
        let compiled = CommitsCheck::compile("team.trailers", check).expect("compiles");
        compiled
            .violation(commit)
            .map(|violation| violation.line("team.trailers"))
    }

    /// A commit with no trailer of the required key breaks the rule, and the
    /// finding names the commit and the check itself: it is whole, with no id
    /// in front of it. The audit row carries the same text.
    #[test]
    fn a_commit_without_the_required_trailer_breaks_the_rule() {
        let check = trailer_check(json!({}));
        let commit = commit_by(
            "A Human",
            "feat: a change",
            &[("Signed-off-by", "A <a@b.c>")],
        );
        assert_eq!(
            finding(&check, &commit),
            Some("abc1234: no Work-Item trailer (team.trailers)".to_string())
        );
        let compiled = CommitsCheck::compile("team.trailers", &check).expect("compiles");
        assert_eq!(
            compiled.violation(&commit).expect("a violation").evidence(),
            "abc1234: no Work-Item trailer (team.trailers)"
        );
    }

    /// A message with no sha on record, the hook's, names no sha.
    #[test]
    fn a_message_with_no_sha_names_no_sha() {
        let check = trailer_check(json!({}));
        let message = Commit::unattributed("feat: a change".to_string(), String::new(), vec![]);
        assert_eq!(
            finding(&check, &message),
            Some("no Work-Item trailer (team.trailers)".to_string())
        );
    }

    /// A trailer of the key with a value satisfies the rule, whatever the case
    /// of the key; a trailer with no value names nothing and does not.
    #[test]
    fn the_key_matches_without_regard_to_case_and_needs_a_value() {
        let check = trailer_check(json!({}));
        for key in ["Work-Item", "work-item", "WORK-ITEM"] {
            let commit = commit_by("A Human", "feat: a change", &[(key, "HR-001")]);
            assert_eq!(finding(&check, &commit), None, "{key}");
        }
        let empty = commit_by("A Human", "feat: a change", &[("Work-Item", "")]);
        assert!(finding(&check, &empty).is_some());
    }

    /// `exempt_authors` match the whole author name; a commit with no author
    /// on record is never exempt by author, even if the list names an empty
    /// string.
    #[test]
    fn exempt_authors_match_whole_names_and_never_an_empty_author() {
        let check = trailer_check(json!({"exempt_authors": ["dependabot[bot]", ""]}));
        let by = |author: &str| commit_by(author, "feat: a change", &[]);
        assert_eq!(finding(&check, &by("dependabot[bot]")), None);
        assert!(finding(&check, &by("not dependabot[bot]")).is_some());
        assert!(finding(&check, &by("dependabot")).is_some());
        let unattributed =
            Commit::unattributed("feat: a change".to_string(), String::new(), vec![]);
        assert!(finding(&check, &unattributed).is_some());
    }

    /// `exempt_subjects` are patterns under the check's flags that may match
    /// anywhere in the subject unless they anchor themselves.
    #[test]
    fn exempt_subjects_match_anywhere_unless_anchored_and_take_the_check_flags() {
        let anchored = trailer_check(json!({"exempt_subjects": ["^chore\\(.+\\): release "]}));
        let release = commit_by("A Human", "chore(main): release 2.0.0", &[]);
        let inside = commit_by("A Human", "feat: prepare chore(main): release 2.0.0", &[]);
        assert_eq!(finding(&anchored, &release), None);
        assert!(finding(&anchored, &inside).is_some());

        let loose = trailer_check(json!({"exempt_subjects": ["RELEASE"], "flags": "i"}));
        assert_eq!(finding(&loose, &inside), None);
        let strict = trailer_check(json!({"exempt_subjects": ["RELEASE"]}));
        assert!(finding(&strict, &inside).is_some());
    }

    /// The exemptions release a commit from the trailer rule alone: an exempt
    /// author still breaks the subject rule of the same check, and that
    /// finding reads behind the id as every other rule's does.
    #[test]
    fn the_exemptions_leave_the_other_rules_of_the_check_alone() {
        let check = trailer_check(json!({
            "subject": "^(feat|fix): ",
            "exempt_authors": ["dependabot[bot]"],
        }));
        let commit = commit_by("dependabot[bot]", "Bump serde", &[]);
        assert_eq!(
            finding(&check, &commit),
            Some("team.trailers: commit \"Bump serde\" does not match ^(feat|fix): ".to_string())
        );
    }

    /// A malformed `exempt_subjects` pattern is a named error, never a panic
    /// (`houserules.crash-paths-are-named`).
    #[test]
    fn a_malformed_exempt_subject_pattern_is_a_named_error() {
        let check = trailer_check(json!({"exempt_subjects": ["(unclosed"]}));
        assert!(CommitsCheck::compile("team.trailers", &check).is_err());
    }
}
