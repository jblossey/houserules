//! The `grep-absent` check: a pattern that no file in scope may match.

use serde_json::Value;

use crate::rules::check_shape::{CheckDef, Scope};

use super::{AuditContext, RowBuilder, compile_check_regex, filter_matching};

/// Judges a `grep-absent` check: the row names the first match of `pattern`
/// in a file of `files`. The files searched are the ones the range changed
/// or, with `scope: "tree"`, every file of the head tree.
pub(super) fn run(
    check: &CheckDef,
    ctx: &AuditContext,
    rows: &RowBuilder,
) -> Result<Value, String> {
    let pool: Vec<String> = if check.scope == Some(Scope::Tree) {
        ctx.tree()?
    } else {
        ctx.changed.to_vec()
    };
    let files = filter_matching(&pool, &check.files)?;
    let pattern = check.pattern.as_deref().unwrap_or_default();
    let regex = compile_check_regex(pattern, check.flags.as_deref().unwrap_or_default())?;
    for path in &files {
        let text = ctx.show(path)?;
        if let Some(found) = regex.find(&text) {
            let line = text[..found.start()].matches('\n').count() + 1;
            return Ok(rows.violate(format!("{path}:{line} matches {pattern}")));
        }
    }
    Ok(rows.pass(format!("{} files checked", files.len())))
}
