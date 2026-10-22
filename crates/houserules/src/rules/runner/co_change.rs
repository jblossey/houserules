//! The `co-change` check: a change to a file of `if` must come with a
//! change to a file of `then`. The audit answers the question for the whole
//! range (`run`); `check-commit` answers it for one commit at a time
//! (`CoChangeCheck`).

use serde_json::Value;

use crate::rules::check_shape::{CheckDef, CheckLevel};

use super::{AuditContext, RowBuilder, filter_matching, glob_list, match_any};

/// One `co-change` check, evaluated against one commit's own changed
/// files. `run` of this module answers a different question at a different
/// granularity -- whether the whole audited RANGE ever paired `if_changed`
/// with `then` somewhere across its commits, not whether any single commit
/// split them -- so `check_commit`'s per-commit range arm evaluates this
/// struct instead of `run`. `pub(in crate::rules)`: a sibling of
/// `CommitsCheck`, read by `check_commit` alone.
pub(in crate::rules) struct CoChangeCheck<'a> {
    check: &'a CheckDef,
}

impl<'a> CoChangeCheck<'a> {
    /// Wraps `check` for per-commit evaluation. Unlike `CommitsCheck::
    /// compile`, there is no regex to precompile: `if_changed`/`then` are
    /// globs, matched fresh per path by `glob_match` itself.
    pub(in crate::rules) fn compile(check: &'a CheckDef) -> Self {
        Self { check }
    }

    /// `Ok(Some(evidence))` when `files` (one commit's own changed paths)
    /// matches `if_changed` but none of them also matches `then`;
    /// `Ok(None)` when the check did not trigger on this commit, or the
    /// commit already pairs both sides itself. A malformed glob is a named
    /// `Err`, never a silently-false match -- the same care `run` of this
    /// module takes (see its inline comment).
    pub(in crate::rules) fn violation(&self, files: &[String]) -> Result<Option<String>, String> {
        let trigger = filter_matching(files, &self.check.if_changed)?;
        if trigger.is_empty() {
            return Ok(None);
        }
        for path in files {
            if match_any(path, &self.check.then)? {
                return Ok(None);
            }
        }
        Ok(Some(format!(
            "{} changed without {}",
            trigger[0],
            glob_list(&self.check.then).join(" or ")
        )))
    }

    /// This check's declared `level`, the same way `CommitsCheck::level`
    /// reads it.
    pub(in crate::rules) fn level(&self) -> CheckLevel {
        self.check.level
    }

    /// `true` when this check's own `then` target already exists somewhere
    /// in `parent_tree` -- a commit's own PARENT tree, from one commit
    /// before the diff (`tree_files`). A per-commit `co-change` check
    /// exists to enforce that a mechanism the repository ALREADY HAS was
    /// paired with its trigger in the same commit; a commit written before
    /// that mechanism existed could not have paired it, so `check_commit`'s
    /// own per-commit loop treats a `false` return as "not yet applicable
    /// to this commit," never a violation. A malformed `then` glob is a
    /// named `Err`, the same care `violation` takes.
    pub(in crate::rules) fn mechanism_exists(
        &self,
        parent_tree: &[String],
    ) -> Result<bool, String> {
        Ok(!filter_matching(parent_tree, &self.check.then)?.is_empty())
    }
}

/// Judges a `co-change` check against the whole range: the row passes when
/// the range changes no file of `if` or changes a file of `then` as well, and
/// fails when it changes a file of `if` and none of `then`.
pub(super) fn run(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<Value, String> {
    let trigger = filter_matching(ctx.changed, &check.if_changed)?;
    if trigger.is_empty() {
        return Ok(rows.pass("not triggered".to_string()));
    }
    // `check.then` has not been matched yet at this point (`trigger`
    // above matches `if_changed`, a different field) -- both loops
    // below propagate a malformed `then` with `?`, the same as
    // `filter_matching` already does for `files`/`if_changed`: there
    // is no `unwrap_or(false)` here to hide a `GlobError` behind a
    // false "did not match".
    let mut satisfying: Option<&String> = None;
    for path in ctx.changed {
        if match_any(path, &check.then)? {
            satisfying = Some(path);
            break;
        }
    }
    if let Some(satisfying) = satisfying {
        let mut real_trigger: Option<&String> = None;
        for path in &trigger {
            if !match_any(path, &check.then)? {
                real_trigger = Some(path);
                break;
            }
        }
        let evidence = match real_trigger {
            Some(t) => format!("{t} changed with {satisfying}"),
            None => format!(
                "only {} changed; the co-change is satisfied by definition",
                trigger.join(", ")
            ),
        };
        return Ok(rows.pass(evidence));
    }
    Ok(rows.violate(format!(
        "{} changed without {}",
        trigger[0],
        glob_list(&check.then).join(" or ")
    )))
}
