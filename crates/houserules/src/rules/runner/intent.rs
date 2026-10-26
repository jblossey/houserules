//! The `intent-required` and `intent-resolves` checks: their rows.
//! `crate::rules::intent_checks` holds what each check decides; this module
//! hands it the range of the audit and builds the row. A check that does not
//! apply is a `skipped` row, which the evidence packet counts among the checks
//! that did not apply.

use crate::published::audit_result::{AuditRow, RowResult};
use crate::rules::check_shape::CheckDef;
use crate::rules::intent_checks::{Outcome, Range, Required, judge_resolves};

use super::{AuditContext, RowBuilder};

/// The row for `outcome`.
fn row_of(rows: &RowBuilder, outcome: Outcome) -> AuditRow {
    match outcome {
        Outcome::NotApplicable(evidence) => rows.row(RowResult::Skipped, evidence),
        Outcome::Holds(evidence) => rows.pass(evidence),
        Outcome::Violated(evidence) => rows.violate(evidence),
    }
}

/// Judges an `intent-required` check over the range of the audit. A field of
/// the check that cannot be read is one named error that starts with the id of
/// the entry.
pub(super) fn run_required(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<AuditRow, String> {
    let required = Required::compile(check).map_err(|line| format!("{}: {line}", rows.entry.id))?;
    let commits = ctx.commits()?;
    let outcome = required.judge(&Range {
        knowledge: ctx.knowledge,
        prefix: ctx.prefix,
        base: &ctx.base_sha,
        head: &ctx.head_sha,
        commits: &commits,
    })?;
    Ok(row_of(rows, outcome))
}

/// Judges an `intent-resolves` check over the range of the audit. The check
/// reads the trailers of the range itself, so it takes the two ends only.
pub(super) fn run_resolves(ctx: &AuditContext, rows: &RowBuilder) -> Result<AuditRow, String> {
    let outcome = judge_resolves(ctx.knowledge, &ctx.base_sha, &ctx.head_sha)?;
    Ok(row_of(rows, outcome))
}
