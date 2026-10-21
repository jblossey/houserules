//! The adapter's checks for `check-backlog` (spec 3.5 and 5.139): the labels
//! of the repository, then the contract checks that every hosted adapter runs
//! (`hosted::check_contract`).

use std::collections::BTreeSet;

use crate::tracker::hosted::{self, DEFERRED};

use super::issue::Issue;

/// The `houserules:<kind>` labels that the adapter check requires.
const RECORD_LABELS: [&str; 6] = [
    "houserules:section",
    "houserules:batch",
    "houserules:amendment",
    "houserules:decision",
    "houserules:parked",
    DEFERRED,
];

/// The errors and the warnings of `check-backlog` for `issues`, given the
/// names of the `labels` of the repository (spec 3.5 and 5.139): a missing
/// record label comes first, then the faults of the issues.
pub(super) fn check_issues(
    repo: &str,
    issues: &[Issue],
    labels: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let mut errors: Vec<String> = RECORD_LABELS
        .iter()
        .filter(|label| !labels.contains(**label))
        .map(|label| {
            format!("the label {label} is missing; run gh label create {label} --repo {repo}")
        })
        .collect();
    let (faults, warnings) = hosted::check_contract(issues);
    errors.extend(faults);
    (errors, warnings)
}

#[cfg(test)]
mod tests {
    use super::super::test_support::issue;
    use super::*;

    /// A missing record label has its line, ahead of the faults of the issues,
    /// and a pull request has no check.
    #[test]
    fn a_missing_record_label_comes_before_the_faults_of_the_issues() {
        let labels: BTreeSet<String> = RECORD_LABELS
            .iter()
            .filter(|label| **label != "houserules:parked")
            .map(|label| label.to_string())
            .collect();
        let mut pull_request = issue(3, &["type:chore", "type:feat"], "");
        pull_request.pull_request = true;
        let issues = [
            issue(1, &["type:chore"], ""),
            issue(2, &["type:chore", "type:feat"], ""),
            pull_request,
        ];
        let (errors, warnings) = check_issues("acme/widgets", &issues, &labels);
        assert_eq!(
            errors,
            [
                "the label houserules:parked is missing; run gh label create houserules:parked --repo acme/widgets",
                "#2 has two type labels",
            ]
        );
        assert!(warnings.is_empty());
    }
}
