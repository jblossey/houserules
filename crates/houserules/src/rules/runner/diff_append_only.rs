//! The `diff-append-only` check: the files of `files` may gain lines but
//! lose none.

use crate::published::audit_result::AuditRow;
use crate::rules::check_shape::CheckDef;

use super::{AuditContext, RowBuilder, filter_matching};

/// Judges a `diff-append-only` check: the row fails when the range removes a
/// line from a changed file of `files`.
pub(super) fn run(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<AuditRow, String> {
    let files = filter_matching(ctx.changed, &check.files)?;
    if files.is_empty() {
        return Ok(rows.pass("not triggered".to_string()));
    }
    let removed = ctx.removed(&files)?;
    if !removed.is_empty() {
        return Ok(rows.violate(format!(
            "{} removed lines in {}",
            removed.len(),
            files.join(", ")
        )));
    }
    Ok(rows.pass(format!("{}: no removed lines", files.join(", "))))
}
