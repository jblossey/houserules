//! The contract checks of `check-backlog` that every hosted adapter runs (spec
//! 3.5 and 5.139): the type labels, the deferrals, the `see` references, and the
//! batch records. The checks of the backend itself (the labels of a repository,
//! the configuration of a store) belong to the adapter.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::records::{batch_status, batch_title, deferral_value};
use super::{DEFERRED, HostedIssue, Kind};
use crate::backlog::ItemStatus;
use crate::tracker::block;

/// The errors and the warnings of the contract checks for `issues`. A pull
/// request is no issue and has no check.
pub(in crate::tracker) fn check_contract<T: HostedIssue>(
    issues: &[T],
) -> (Vec<String>, Vec<String>) {
    let issues: Vec<&T> = issues
        .iter()
        .filter(|issue| !issue.is_pull_request())
        .collect();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for issue in &issues {
        if issue.label_values("type:").count() > 1 {
            errors.push(format!("{} has two type labels", issue.id()));
        }
        if issue.has_label(DEFERRED) {
            let reason = deferral_value(&block::parse(issue.text()).1)["reason"].clone();
            if reason
                .as_str()
                .is_none_or(|reason| reason.trim().is_empty())
            {
                errors.push(format!("{}: the deferral has no reason", issue.id()));
            }
        }
    }
    let exists = |id: &str| issues.iter().any(|issue| issue.answers_to(id));
    for issue in issues.iter().filter(|issue| issue.is_item()) {
        let see = issue.see();
        for id in see
            .as_ref()
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|id| !exists(id))
        {
            errors.push(format!("{}: see \"{id}\" does not exist", issue.id()));
        }
        if issue.item_status() == ItemStatus::Done && issue.batch().is_none() {
            warnings.push(format!("{}: done without a batch", issue.id()));
        }
    }
    check_batches(&issues, &mut errors);
    (errors, warnings)
}

/// The batch checks: every batch issue has a title that names its number, no
/// number repeats, and at most one batch is in progress.
fn check_batches<T: HostedIssue>(issues: &[&T], errors: &mut Vec<String>) {
    let mut first: BTreeMap<u64, String> = BTreeMap::new();
    let mut in_progress = Vec::new();
    for issue in issues.iter().filter(|issue| issue.kind() == Kind::Batch) {
        let Some((number, _)) = batch_title(*issue) else {
            errors.push(format!(
                "{}: a batch issue is titled \"Batch <n>: <summary>\"",
                issue.id()
            ));
            continue;
        };
        match first.get(&number) {
            Some(earlier) => errors.push(format!(
                "{}: duplicate batch {number} (also {earlier})",
                issue.id()
            )),
            None => {
                first.insert(number, issue.id());
            }
        }
        let status = batch_status(*issue, &block::parse(issue.text()).1);
        if status["state"] == json!("in-progress") {
            in_progress.push(issue.id());
        }
    }
    if in_progress.len() > 1 {
        errors.push(format!(
            "{} batches in progress (at most one): {}",
            in_progress.len(),
            in_progress.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{TestIssue, closed, issue, marked};
    use super::*;

    /// Spec 3.5: each fault of the contract checks has its line: two type
    /// labels, a deferral with no reason, a `see` that names no issue, and a
    /// done item with no batch (a warning). A pull request has no check.
    #[test]
    fn the_checks_print_one_line_for_each_fault() {
        let mut done = closed(issue("hr-4", &["type:chore"], ""));
        done.see = Some(json!(["hr-1", "hr-9", "external:p:c", 7]));
        let mut pull_request = issue("hr-5", &["type:chore", "type:feat"], "");
        pull_request.pull_request = true;
        let issues = [
            issue("hr-1", &["type:chore"], ""),
            issue("hr-2", &["type:chore", "type:feat"], ""),
            issue(
                "hr-3",
                &[DEFERRED, "type:chore"],
                "```houserules\ndeferral: {\"reason\":\" \"}\n```",
            ),
            done,
            pull_request,
        ];
        let (errors, warnings) = check_contract(&issues);
        assert_eq!(
            errors,
            [
                "hr-2 has two type labels",
                "hr-3: the deferral has no reason",
                "hr-4: see \"hr-9\" does not exist",
                "hr-4: see \"external:p:c\" does not exist",
            ]
        );
        assert_eq!(warnings, ["hr-4: done without a batch"]);
    }

    /// A `see` that points at a pull request names no issue.
    #[test]
    fn a_see_that_points_at_a_pull_request_does_not_exist() {
        let mut item = issue("hr-1", &["type:chore"], "");
        item.see = Some(json!(["hr-2"]));
        let mut pull_request = issue("hr-2", &["type:chore"], "");
        pull_request.pull_request = true;
        let (errors, _) = check_contract(&[item, pull_request]);
        assert_eq!(errors, ["hr-1: see \"hr-2\" does not exist"]);
    }

    /// The batch check names a batch title that has no number, a duplicate
    /// number, and a second batch in progress.
    #[test]
    fn the_batch_checks_name_each_fault() {
        let batch = |id: &str, title: &str, state: &str| {
            let mut issue = marked(
                Kind::Batch,
                id,
                &[],
                &format!("```houserules\nstatus: {{\"state\":\"{state}\",\"text\":\"\"}}\n```"),
            );
            issue.title = title.to_string();
            issue
        };
        let issues: [TestIssue; 3] = [
            batch("hr-1", "Batch 1: One", "in-progress"),
            batch("hr-2", "Batch 1: Again", "in-progress"),
            batch("hr-3", "Nonsense", "planned"),
        ];
        let (errors, warnings) = check_contract(&issues);
        assert_eq!(
            errors,
            [
                "hr-2: duplicate batch 1 (also hr-1)",
                "hr-3: a batch issue is titled \"Batch <n>: <summary>\"",
                "2 batches in progress (at most one): hr-1, hr-2",
            ]
        );
        assert!(warnings.is_empty());
    }
}
