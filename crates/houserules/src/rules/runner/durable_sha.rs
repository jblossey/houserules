//! The `durable-sha` check: an added line of `files` must not cite a commit
//! that the default branch does not reach. `crate::rules::durable_sha` holds
//! the scan; this module narrows `files` to the changed ones and builds the
//! row.

use crate::published::audit_result::AuditRow;
use crate::rules::check_shape::CheckDef;

use super::{AuditContext, RowBuilder, filter_matching};

/// Judges a `durable-sha` check over the changed files of `files`. The
/// default branch is resolved on each evaluation, independent of the range.
pub(super) fn run(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<AuditRow, String> {
    let files = filter_matching(ctx.changed, &check.files)?;
    if files.is_empty() {
        return Ok(rows.pass("not triggered".to_string()));
    }
    // Resolved fresh per evaluation, independent of `--base`/
    // `--head`: `durable_sha.rs`'s own module doc, "Why the
    // default branch is resolved explicitly", has the full
    // account of why `ctx.base_sha` cannot stand in for it.
    let default_branch = crate::rules::durable_sha::resolve_default_branch(ctx.root)?;
    match crate::rules::durable_sha::find_violation(
        ctx.root,
        &ctx.base_sha,
        &ctx.head_sha,
        &default_branch,
        &files,
    )? {
        Some(evidence) => Ok(rows.violate(evidence)),
        None => Ok(rows.pass(format!(
            "{} file(s) checked for in-branch sha citations (against the default \
             branch, {default_branch})",
            files.len()
        ))),
    }
}
