//! The adapter's checks for `check-backlog` (spec 3.5 and 5.139): the custom
//! status and the custom types of the store, then the contract checks that
//! every hosted adapter runs (`hosted::check_contract`).

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use super::issue::{Issue, PARTIAL};
use crate::tracker::hosted;

/// The key of `bd config` that lists the custom statuses.
const STATUS_KEY: &str = "status.custom";

/// The key of `bd config` that lists the custom types.
const TYPES_KEY: &str = "types.custom";

/// The custom status `partial` in the form of `bd config set status.custom`: in
/// the category that `bd ready` lists.
const PARTIAL_CONFIG: &str = "partial:active";

/// The custom types of the records, in the order that the check names them.
const RECORD_TYPES: [&str; 5] = [
    "houserules-section",
    "houserules-batch",
    "houserules-amendment",
    "houserules-decision",
    "houserules-parked",
];

/// The `config` object of `bd info --json`.
pub(super) type Config = Map<String, Value>;

/// The entries of a `bd config` list: the parts between commas, trimmed, with
/// no empty one.
fn entries(list: &str) -> Vec<&str> {
    list.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// The name of a list entry: the part before its `:<category>`.
fn name_of(entry: &str) -> &str {
    entry.split(':').next().unwrap_or_default().trim()
}

/// The names in a `bd config` list, each without its `:<category>`.
fn names(list: &str) -> BTreeSet<&str> {
    entries(list)
        .into_iter()
        .map(name_of)
        .filter(|name| !name.is_empty())
        .collect()
}

/// The entries of `listed` with the status `partial` in the active category:
/// the first entry of `partial` takes the category, another one of `partial`
/// goes, and every other entry stays.
fn with_partial_active(listed: &[&str]) -> String {
    let mut placed = false;
    let kept: Vec<&str> = listed
        .iter()
        .filter_map(|entry| {
            if name_of(entry) != PARTIAL {
                Some(*entry)
            } else if placed {
                None
            } else {
                placed = true;
                Some(PARTIAL_CONFIG)
            }
        })
        .collect();
    kept.join(",")
}

/// The line for a custom setting that the store lacks: `what` names it (`status
/// partial`, `type houserules-batch`, or `types a, b` with `are` for the
/// plural), and the step is `bd config set <key> <existing,missing>`, because
/// `bd config set` replaces the list.
fn config_line(what: &str, verb: &str, key: &str, existing: &str, missing: &[&str]) -> String {
    let existing = existing.trim();
    let list = if existing.is_empty() {
        missing.join(",")
    } else {
        format!("{existing},{}", missing.join(","))
    };
    format!("the custom {what} {verb} missing; run bd config set {key} {list}")
}

/// The faults of the store's configuration: the custom status `partial`, which
/// `config` must list in the active category so that `bd ready` shows it, and
/// the custom types of the records that `config` does not list.
fn check_config(config: &Config) -> Vec<String> {
    let list = |key: &str| config.get(key).and_then(Value::as_str).unwrap_or_default();
    let mut errors = Vec::new();
    let statuses = list(STATUS_KEY);
    let listed = entries(statuses);
    if !listed.contains(&PARTIAL_CONFIG) {
        if listed.iter().any(|entry| name_of(entry) == PARTIAL) {
            errors.push(format!(
                "the custom status partial is not in the active category; run bd config set {STATUS_KEY} {}",
                with_partial_active(&listed)
            ));
        } else {
            errors.push(config_line(
                "status partial",
                "is",
                STATUS_KEY,
                statuses,
                &[PARTIAL_CONFIG],
            ));
        }
    }
    let types = list(TYPES_KEY);
    let known = names(types);
    let missing: Vec<&str> = RECORD_TYPES
        .iter()
        .copied()
        .filter(|record| !known.contains(record))
        .collect();
    match missing[..] {
        [] => {}
        [one] => errors.push(config_line(
            &format!("type {one}"),
            "is",
            TYPES_KEY,
            types,
            &missing,
        )),
        _ => errors.push(config_line(
            &format!("types {}", missing.join(", ")),
            "are",
            TYPES_KEY,
            types,
            &missing,
        )),
    }
    errors
}

/// The errors and the warnings of `check-backlog` for `issues`, given the
/// `config` of the store (spec 3.5 and 5.139): the faults of the configuration
/// come first, then the contract checks that every hosted adapter runs
/// (`hosted::check_contract`).
pub(super) fn check_issues(issues: &[Issue], config: &Config) -> (Vec<String>, Vec<String>) {
    let mut errors = check_config(config);
    let (faults, warnings) = hosted::check_contract(issues);
    errors.extend(faults);
    (errors, warnings)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::test_support::issue;
    use super::*;

    /// A store that lists everything the check requires.
    fn complete() -> Config {
        let mut config = Config::new();
        config.insert(STATUS_KEY.to_string(), json!("partial:active"));
        config.insert(TYPES_KEY.to_string(), json!(RECORD_TYPES.join(",")));
        config
    }

    /// A fault of the configuration comes before the faults of the issues, and
    /// the `related` dependencies of an item are its `see` list.
    #[test]
    fn a_fault_of_the_configuration_comes_before_the_faults_of_the_issues() {
        let mut config = complete();
        config.remove(TYPES_KEY);
        let mut item = issue("hr-1", "chore", &["type:chore", "type:feat"], "");
        item.related = vec!["hr-9".to_string()];
        let (errors, warnings) = check_issues(&[item], &config);
        assert_eq!(
            errors,
            [
                format!(
                    "the custom types {} are missing; run bd config set types.custom {}",
                    RECORD_TYPES.join(", "),
                    RECORD_TYPES.join(",")
                ),
                "hr-1 has two type labels".to_string(),
                "hr-1: see \"hr-9\" does not exist".to_string(),
            ]
        );
        assert!(warnings.is_empty());
    }

    /// Spec 3.5: the check names the `bd config` step for a missing status or
    /// type, and the step keeps what the store lists, because `bd config set`
    /// replaces the list. The status `partial` must sit in the active category,
    /// so that `bd ready` shows it: any other category, or none, is a fault whose
    /// step replaces the category of `partial` and keeps every other entry.
    #[test]
    fn the_config_check_names_the_bd_config_step() {
        let config = |status: Option<&str>, types: Option<&str>| {
            let mut config = Config::new();
            for (key, value) in [(STATUS_KEY, status), (TYPES_KEY, types)] {
                if let Some(value) = value {
                    config.insert(key.to_string(), json!(value));
                }
            }
            config
        };
        let all_types = RECORD_TYPES.join(",");
        assert_eq!(check_config(&complete()), Vec::<String>::new());
        assert_eq!(
            check_config(&config(
                Some("review:wip, partial:active"),
                Some(&all_types)
            )),
            Vec::<String>::new(),
            "an entry is its name and its category, and spaces around it count for nothing"
        );
        for (listed, step) in [
            ("review:wip, partial:frozen", "review:wip,partial:active"),
            ("partial:frozen,review:wip", "partial:active,review:wip"),
            ("partial", "partial:active"),
            ("partial:wip,partial:frozen", "partial:active"),
            ("partial:active2,review:wip", "partial:active,review:wip"),
        ] {
            assert_eq!(
                check_config(&config(Some(listed), Some(&all_types))),
                [format!(
                    "the custom status partial is not in the active category; run bd config set status.custom {step}"
                )],
                "{listed}"
            );
        }
        assert_eq!(
            check_config(&config(None, Some(&all_types))),
            [
                "the custom status partial is missing; run bd config set status.custom partial:active"
            ]
        );
        assert_eq!(
            check_config(&config(Some("review:wip"), Some(&all_types))),
            [
                "the custom status partial is missing; run bd config set status.custom review:wip,partial:active"
            ]
        );
        assert_eq!(
            check_config(&config(Some("partial:active"), None)),
            [format!(
                "the custom types {} are missing; run bd config set types.custom {all_types}",
                RECORD_TYPES.join(", ")
            )]
        );
        assert_eq!(
            check_config(&config(
                Some("partial:active"),
                Some(
                    "houserules-section,spike-ish,houserules-batch,houserules-amendment,houserules-decision"
                )
            )),
            [
                "the custom type houserules-parked is missing; run bd config set types.custom houserules-section,spike-ish,houserules-batch,houserules-amendment,houserules-decision,houserules-parked"
            ]
        );
    }
}
