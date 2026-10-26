//! The `review_coverage` key of `houserules stats --records <range>` (spec 8.3):
//! how much of the merged change was reviewed, from the change records of a
//! range.
//!
//! The key counts the records whose key is in the range; those with at least one
//! reviewer; both counts for each route (all three routes, in the order of the
//! documents) and for each area that a record's route inputs name (sorted by
//! name); the records that the cut of spec 8.4 left in the minimal form, which
//! holds no route inputs and so no area; the sampled records; and the audited
//! records with their verdicts. The output holds no time, no path, and no value
//! of the environment.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::published::change_record::{AuditVerdict, ChangeRecord};
use crate::published::route::Route;
use crate::review::record::records_of;

/// The routes in the order of the documents: the lowest first.
const ROUTES: [Route; 3] = [
    Route::AutoMergeEligible,
    Route::OwnerReview,
    Route::SeniorReview,
];

/// Two counts of a group of records: all of them, and those with a reviewer.
#[derive(Default)]
struct Count {
    /// The records of the group.
    records: u64,
    /// The records of the group with at least one reviewer.
    with_reviewers: u64,
}

impl Count {
    /// Adds a record; `reviewed` tells whether it has a reviewer.
    fn add(&mut self, reviewed: bool) {
        self.records += 1;
        self.with_reviewers += u64::from(reviewed);
    }

    /// The counts as the object of the output.
    fn to_value(&self) -> Value {
        json!({"records": self.records, "with_reviewers": self.with_reviewers})
    }
}

/// The `review_coverage` of `records`.
fn coverage(records: &[ChangeRecord]) -> Value {
    let mut all = Count::default();
    let mut routes: BTreeMap<Route, Count> = ROUTES
        .iter()
        .map(|route| (*route, Count::default()))
        .collect();
    let mut areas: BTreeMap<&str, Count> = BTreeMap::new();
    let (mut without_area_data, mut sampled) = (0u64, 0u64);
    let (mut pass, mut fail) = (0u64, 0u64);
    for record in records {
        let reviewed = !record.reviewers.is_empty();
        all.add(reviewed);
        routes
            .get_mut(&record.route.route)
            .expect("every route is counted")
            .add(reviewed);
        match &record.route.inputs {
            Some(inputs) => {
                for area in inputs.areas.keys() {
                    areas.entry(area).or_default().add(reviewed);
                }
            }
            None => without_area_data += 1,
        }
        sampled += u64::from(record.sample.is_some_and(|sample| sample.sampled));
        match record.post_merge_audit.as_ref().map(|audit| audit.verdict) {
            Some(AuditVerdict::Pass) => pass += 1,
            Some(AuditVerdict::Fail) => fail += 1,
            None => {}
        }
    }
    let routes: Map<String, Value> = ROUTES
        .iter()
        .map(|route| (route.as_str().to_string(), routes[route].to_value()))
        .collect();
    let areas: Map<String, Value> = areas
        .into_iter()
        .map(|(area, count)| (area.to_string(), count.to_value()))
        .collect();
    json!({
        "records": all.records,
        "with_reviewers": all.with_reviewers,
        "routes": routes,
        "areas": areas,
        "without_area_data": without_area_data,
        "sampled": sampled,
        "audited": {"count": pass + fail, "pass": pass, "fail": fail},
    })
}

/// The `review_coverage` key of the install at `root` for the records whose key
/// is in `range`. The error is one line: a range that git cannot read, a range
/// that reaches past the history of a shallow clone (spec 8.3), a clone without
/// the notes ref (spec 8.1), or a note that is no change record.
pub(crate) fn review_coverage(root: &Path, range: &str) -> Result<Value, String> {
    records_of(root, range).map(|records| coverage(&records))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A record with the route `route`, the areas `areas` in its route inputs
    /// (or none, for the minimal form), `reviewers` names, the sample draw
    /// `sample`, and the audit `audit`.
    fn record(
        route: &str,
        areas: Option<&[&str]>,
        reviewers: usize,
        sample: Value,
        audit: Value,
    ) -> ChangeRecord {
        let inputs = areas.map(|areas| {
            let named: Map<String, Value> = areas
                .iter()
                .map(|area| (area.to_string(), json!(["a.txt"])))
                .collect();
            json!({
                "failed": [], "warned": [], "areas": named, "no_area": [], "paths": [],
                "diff_lines": 1, "uncovered_share": 0.0, "score": null,
                "rule_sources_changed": [],
            })
        });
        let mut route_value = json!({"route": route, "floor": false});
        if let Some(inputs) = inputs {
            route_value["inputs"] = inputs;
        }
        serde_json::from_value(json!({
            "schema": "houserules/change-record@1.0.0",
            "install": ".",
            "key": "4".repeat(40),
            "base": "0".repeat(40),
            "head": "4".repeat(40),
            "merge_base": "0".repeat(40),
            "written_by": "houserules 2.0.0",
            "host": null,
            "rules": {
                "merge_base_hash": null, "merge_base_rules": "absent",
                "head_hash": "b".repeat(64), "audited_with": "head", "packs": [],
            },
            "audit": {"summary": {
                "base": "0000000", "head": "4444444", "deterministic": 0, "pass": 0,
                "fail": 0, "warn": 0, "skipped": 0, "judged": 0, "reference_fail": 0,
            }},
            "intent": [],
            "route": route_value,
            "sample": sample,
            "reviewers": (0..reviewers).map(|n| format!("user{n}")).collect::<Vec<_>>(),
            "reviewers_pending": false,
            "post_merge_audit": audit,
            "truncated": [],
        }))
        .expect("a change record")
    }

    /// An audit with the verdict `verdict`.
    fn audited(verdict: &str) -> Value {
        json!({"verdict": verdict, "by": "amy", "note": null})
    }

    #[test]
    fn no_record_gives_zero_counts_for_every_route_and_no_area() {
        assert_eq!(
            coverage(&[]),
            json!({
                "records": 0,
                "with_reviewers": 0,
                "routes": {
                    "auto-merge-eligible": {"records": 0, "with_reviewers": 0},
                    "owner-review": {"records": 0, "with_reviewers": 0},
                    "senior-review": {"records": 0, "with_reviewers": 0},
                },
                "areas": {},
                "without_area_data": 0,
                "sampled": 0,
                "audited": {"count": 0, "pass": 0, "fail": 0},
            })
        );
    }

    #[test]
    fn a_record_counts_in_its_route_and_in_each_area_of_its_inputs() {
        let records = [
            record(
                "owner-review",
                Some(&["docs", "code"]),
                1,
                json!(null),
                json!(null),
            ),
            record(
                "owner-review",
                Some(&["code"]),
                0,
                json!(null),
                audited("fail"),
            ),
            record(
                "auto-merge-eligible",
                Some(&["docs"]),
                0,
                json!({"rate": 0.5, "sampled": true}),
                audited("pass"),
            ),
            record(
                "senior-review",
                None,
                2,
                json!({"rate": 0.5, "sampled": false}),
                json!(null),
            ),
        ];

        let value = coverage(&records);

        assert_eq!(value["records"], json!(4));
        assert_eq!(value["with_reviewers"], json!(2));
        assert_eq!(
            value["routes"],
            json!({
                "auto-merge-eligible": {"records": 1, "with_reviewers": 0},
                "owner-review": {"records": 2, "with_reviewers": 1},
                "senior-review": {"records": 1, "with_reviewers": 1},
            })
        );
        assert_eq!(
            value["areas"],
            json!({
                "code": {"records": 2, "with_reviewers": 1},
                "docs": {"records": 2, "with_reviewers": 1},
            })
        );
        assert_eq!(value["without_area_data"], json!(1));
        assert_eq!(value["sampled"], json!(1));
        assert_eq!(value["audited"], json!({"count": 2, "pass": 1, "fail": 1}));
    }

    #[test]
    fn the_areas_print_sorted_by_name_and_the_routes_in_the_order_of_the_documents() {
        let records = [record(
            "owner-review",
            Some(&["zeta", "alpha", "mid"]),
            0,
            json!(null),
            json!(null),
        )];

        let value = coverage(&records);

        let areas: Vec<&str> = value["areas"]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        let routes: Vec<&str> = value["routes"]
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(areas, ["alpha", "mid", "zeta"]);
        assert_eq!(
            routes,
            ["auto-merge-eligible", "owner-review", "senior-review"]
        );
    }
}
