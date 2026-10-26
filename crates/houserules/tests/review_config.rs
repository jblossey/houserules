//! The `review` key of `.houserules.json`, the `covers` field of a check, and
//! the install schema `houserules/install@2.3.0` (batch 37, spec 4.4, 6.1, and
//! 10.7; cases 7 and 9). Each case runs the compiled binary against a scratch
//! install that `houserules init` made.

mod help_support;
mod tracker_support;

use std::fs;
use std::path::Path;

use help_support::{help_of, states};
use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    commit_all, edit_marker, read_json, repo_root, run, run_ok, scratch_git_repo, sha256_hex,
    stderr, stdout, write_json,
};

/// The file of the spec of batch 38, below `docs/specs/`.
const SPEC_FILE: &str = "2026-10-25-batch-38-openspec-and-release.md";

/// What follows the file name in the line of a `review.sessions.trailers` that is no list of
/// trailer keys (spec 5.7).
const SESSION_TRAILERS_FAULT: &str =
    "review.sessions.trailers must be an array of trailer keys (letters, digits, and -)";

/// The three route values of the `review.route` keys, as the lines name them.
const ROUTES: &str = r#""auto-merge-eligible", "owner-review", or "senior-review""#;

/// The two route values of `review.route.default` and `review.route.warn`: a
/// change is auto-merge-eligible only through an area or a path entry
/// (design.md 257).
const FALLBACK_ROUTES: &str = r#""owner-review" or "senior-review""#;

/// What the line of a `default` or a `warn` of `auto-merge-eligible` adds to
/// the two routes it names.
const FALLBACK_REASON: &str =
    ": a change is auto-merge-eligible only through an area or a path entry that names both limits";

/// A scratch install that `houserules init` made.
fn install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    dir
}

/// The line of a fault of `.houserules.json`: `rest` follows the file name.
fn marker_line(rest: &str) -> String {
    format!(".houserules.json: {rest}")
}

/// Sets the `review` key of the marker to `value`.
fn set_review(dir: &Path, value: Value) {
    edit_marker(dir, |marker| {
        marker.insert("review".to_string(), value);
    });
}

/// The exit code and the stderr lines of `check-knowledge`.
fn check_knowledge(dir: &Path) -> (Option<i32>, Vec<String>) {
    let output = run(dir, &["check-knowledge"]);
    (
        output.status.code(),
        stderr(&output).lines().map(str::to_string).collect(),
    )
}

/// Requires `check-knowledge` to fail with exactly the stderr lines `expected`.
fn assert_fails_with(dir: &Path, expected: &[String], context: &str) {
    let (code, lines) = check_knowledge(dir);
    assert_eq!((code, lines.as_slice()), (Some(1), expected), "{context}");
}

/// Requires `check-knowledge` to pass.
fn assert_passes(dir: &Path, context: &str) {
    let output = run(dir, &["check-knowledge"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (Some(0), "knowledge: ok\n".to_string(), String::new()),
        "{context}"
    );
}

/// The example of spec 6.1, with the area `docs` that `init` defines.
fn spec_example() -> Value {
    json!({
        "diffExclude": ["Cargo.lock"],
        "route": {
            "default": "owner-review",
            "warn": "owner-review",
            "areas": {
                "docs": {
                    "route": "auto-merge-eligible", "maxDiffLines": 200, "maxUncoveredShare": 0
                }
            },
            "paths": [{"glob": "crates/houserules/src/install/**", "route": "senior-review"}],
            "ownerScore": 0.5,
            "seniorScore": 0.8,
            "sampleRate": 0.1,
        },
        "bundle": {
            "budgets": {"light": 400, "standard": 1500},
            "exemptAuthors": ["dependabot[bot]"],
            "exemptBranches": ["release-please--*"],
        },
        "intent": {"sources": ["work-item", "file"], "files": ["docs/plans/*.md"]},
        "ci": {"files": [".github/workflows/ci.yml"]},
        "record": {"maxBytes": 65536},
    })
}

/// `spec_example()` with the value at `path` replaced by `value`; `path` runs
/// through object keys only.
fn example_with(path: &[&str], value: Value) -> Value {
    let mut review = spec_example();
    let (last, parents) = path.split_last().expect("a non-empty path");
    let mut node = &mut review;
    for key in parents {
        node = &mut node[*key];
    }
    node[*last] = value;
    review
}

/// Pin of unchanged behavior: an install with no `review` key passes. This
/// passes at the base commit and guards the lint against a default that
/// demands the key.
#[test]
fn a_marker_with_no_review_key_passes() {
    let dir = install();
    assert_passes(dir.path(), "a fresh install");
}

/// Pin of unchanged behavior: the example of spec 6.1 passes. This passes at
/// the base commit, where the key is unknown, and guards the lint against a
/// fault line for a sound value.
#[test]
fn the_example_of_the_spec_passes() {
    let dir = install();
    set_review(dir.path(), spec_example());
    assert_passes(dir.path(), "the example of spec 6.1");
}

/// Case 7: a `review` value that is no object is one line.
#[test]
fn a_review_value_that_is_no_object_is_one_line() {
    let dir = install();
    for (label, value) in [
        ("an array", json!([])),
        ("a string", json!("owner-review")),
        ("a number", json!(3)),
        ("null", Value::Null),
    ] {
        set_review(dir.path(), value);
        assert_fails_with(
            dir.path(),
            &[marker_line("review must be an object")],
            label,
        );
    }
}

/// Case 7: a route value outside the three is one line for each key that
/// takes a route.
#[test]
fn a_route_value_outside_the_three_is_one_line() {
    let dir = install();
    let cases = [
        (
            "the default",
            example_with(&["route", "default"], json!("never")),
            "review.route.default",
            FALLBACK_ROUTES,
        ),
        (
            "the warn route",
            example_with(&["route", "warn"], json!("sometimes")),
            "review.route.warn",
            FALLBACK_ROUTES,
        ),
        (
            "the route of an area",
            example_with(&["route", "areas", "docs", "route"], json!("owner")),
            "review.route.areas.docs.route",
            ROUTES,
        ),
        (
            "the route of a path",
            example_with(&["route", "paths"], json!([{"glob": "src/**", "route": 3}])),
            "review.route.paths[0].route",
            ROUTES,
        ),
        (
            "a path with no route",
            example_with(&["route", "paths"], json!([{"glob": "src/**"}])),
            "review.route.paths[0].route",
            ROUTES,
        ),
    ];
    for (label, review, key, routes) in cases {
        set_review(dir.path(), review);
        assert_fails_with(
            dir.path(),
            &[marker_line(&format!("{key} must be {routes}"))],
            label,
        );
    }
}

/// Design.md 257: a `review.route.default` or a `review.route.warn` of
/// `auto-merge-eligible` is one line that names the key. A change is
/// auto-merge-eligible only through an area or a path entry that names both
/// limits, so the fault adds no line for `sampleRate`.
#[test]
fn a_default_or_warn_route_of_auto_merge_eligible_is_one_line() {
    let dir = install();
    for key in ["default", "warn"] {
        let mut review = spec_example();
        review["route"][key] = json!("auto-merge-eligible");
        set_review(dir.path(), review);
        assert_fails_with(
            dir.path(),
            &[marker_line(&format!(
                "review.route.{key} must be {FALLBACK_ROUTES}{FALLBACK_REASON}"
            ))],
            key,
        );
    }

    set_review(
        dir.path(),
        json!({"route": {"default": "auto-merge-eligible", "warn": "auto-merge-eligible"}}),
    );
    assert_fails_with(
        dir.path(),
        &[
            marker_line(&format!(
                "review.route.default must be {FALLBACK_ROUTES}{FALLBACK_REASON}"
            )),
            marker_line(&format!(
                "review.route.warn must be {FALLBACK_ROUTES}{FALLBACK_REASON}"
            )),
        ],
        "both",
    );
}

/// Pin of unchanged behavior: a `default` and a `warn` of `owner-review` or
/// `senior-review` pass, and so does an area at `auto-merge-eligible` beside
/// them. This passes at the base commit and guards the new fault against a
/// route that it refuses for every key.
#[test]
fn a_default_and_a_warn_route_of_the_other_two_routes_pass() {
    let dir = install();
    for (default, warn) in [
        ("owner-review", "senior-review"),
        ("senior-review", "owner-review"),
    ] {
        let mut review = spec_example();
        review["route"]["default"] = json!(default);
        review["route"]["warn"] = json!(warn);
        set_review(dir.path(), review);
        assert_passes(dir.path(), &format!("default {default}, warn {warn}"));
    }
}

/// Case 7: an area that `knowledge/areas.json` does not define is one line.
#[test]
fn an_area_that_areas_json_does_not_define_is_one_line() {
    let dir = install();
    set_review(
        dir.path(),
        example_with(
            &["route", "areas", "nowhere"],
            json!({"route": "owner-review"}),
        ),
    );
    assert_fails_with(
        dir.path(),
        &[marker_line(
            "review.route.areas.nowhere is not an area of knowledge/areas.json",
        )],
        "an undefined area",
    );
}

/// The line of an area of `review.route.areas` that `knowledge/areas.json`
/// does not define.
fn undefined_area_line(name: &str) -> String {
    marker_line(&format!(
        "review.route.areas.{name} is not an area of knowledge/areas.json"
    ))
}

/// The line of a `maxBytes` below 8,192.
fn max_bytes_line() -> String {
    marker_line("review.record.maxBytes must be an integer of at least 8192")
}

/// Case 7: an undefined area has its line beside every other fault of the
/// key, and each fault of the key has its line in one run. A `maxBytes` of
/// 100 beside an undefined area gives both lines, the area first; the
/// fix of `maxBytes` leaves the line of the area.
#[test]
fn an_undefined_area_has_its_line_beside_the_other_faults_of_the_key() {
    let dir = install();
    let review = json!({
        "route": {"areas": {"nowhere": {"route": "owner-review"}}},
        "record": {"maxBytes": 100},
    });
    set_review(dir.path(), review.clone());
    assert_fails_with(
        dir.path(),
        &[undefined_area_line("nowhere"), max_bytes_line()],
        "an undefined area and a maxBytes of 100",
    );

    let mut fixed = review;
    fixed["record"]["maxBytes"] = json!(65536);
    set_review(dir.path(), fixed);
    assert_fails_with(
        dir.path(),
        &[undefined_area_line("nowhere")],
        "an undefined area",
    );
}

/// Case 7, the order of the lines: the sections come in the order of spec
/// 6.1. Inside `review.route` the lines of `default` and `warn` come first,
/// then each area in the order of the file (the line of an undefined area
/// before the other lines of the area), then the paths, the scores,
/// `sampleRate`, and the line that `sampleRate` is needed.
#[test]
fn the_line_of_an_undefined_area_comes_in_the_order_of_the_route_section() {
    let dir = install();
    set_review(
        dir.path(),
        json!({
            "record": {"maxBytes": 100},
            "route": {
                "ownerScore": -1,
                "paths": [{"glob": "src/**"}],
                "areas": {
                    "docs": {"route": "auto-merge-eligible"},
                    "nowhere": {"route": "owner"},
                    "also": {},
                },
                "default": "never",
            },
            "diffExclude": 3,
        }),
    );
    let needed = |key: &str| {
        marker_line(&format!(
            "{key} is needed when the route is \"auto-merge-eligible\""
        ))
    };
    assert_fails_with(
        dir.path(),
        &[
            marker_line("review.diffExclude must be an array of strings"),
            marker_line(&format!("review.route.default must be {FALLBACK_ROUTES}")),
            needed("review.route.areas.docs.maxDiffLines"),
            needed("review.route.areas.docs.maxUncoveredShare"),
            undefined_area_line("nowhere"),
            marker_line(&format!(
                "review.route.areas.nowhere.route must be {ROUTES}"
            )),
            undefined_area_line("also"),
            marker_line(&format!("review.route.paths[0].route must be {ROUTES}")),
            marker_line("review.route.ownerScore must be a number from 0 to 1"),
            marker_line(
                "review.route.sampleRate is needed when a route is \"auto-merge-eligible\"",
            ),
            max_bytes_line(),
        ],
        "a fault in each section and in each part of the route section",
    );
}

/// Case 7: a glob that does not compile is one line that names the key and
/// the glob. The rest of the line is the text of the glob library.
#[test]
fn an_invalid_glob_is_one_line_naming_its_key() {
    let dir = install();
    let cases = [
        (
            "diffExclude",
            example_with(&["diffExclude"], json!(["Cargo.lock", "a[z-a]b"])),
            "review.diffExclude[1]",
        ),
        (
            "the glob of a path",
            example_with(
                &["route", "paths"],
                json!([{"glob": "a[z-a]b", "route": "senior-review"}]),
            ),
            "review.route.paths[0].glob",
        ),
        (
            "exemptBranches",
            example_with(&["bundle", "exemptBranches"], json!(["a[z-a]b"])),
            "review.bundle.exemptBranches[0]",
        ),
        (
            "intent files",
            example_with(&["intent", "files"], json!(["a[z-a]b"])),
            "review.intent.files[0]",
        ),
    ];
    for (label, review, key) in cases {
        set_review(dir.path(), review);
        let (code, lines) = check_knowledge(dir.path());
        let start = marker_line(&format!("{key}: invalid glob \"a[z-a]b\": "));
        assert_eq!(code, Some(1), "{label}: {lines:?}");
        assert_eq!(lines.len(), 1, "{label}: {lines:?}");
        assert!(lines[0].starts_with(&start), "{label}: {lines:?}");
    }
}

/// Case 7: a route of `auto-merge-eligible` needs both thresholds in its area
/// or path entry, and the config needs `sampleRate`.
#[test]
fn a_missing_threshold_or_sample_rate_is_one_line() {
    let dir = install();
    let needed = |key: &str| {
        marker_line(&format!(
            "{key} is needed when the route is \"auto-merge-eligible\""
        ))
    };
    let cases = [
        (
            "an area without maxDiffLines",
            example_with(
                &["route", "areas", "docs"],
                json!({"route": "auto-merge-eligible", "maxUncoveredShare": 0}),
            ),
            vec![needed("review.route.areas.docs.maxDiffLines")],
        ),
        (
            "an area without maxUncoveredShare",
            example_with(
                &["route", "areas", "docs"],
                json!({"route": "auto-merge-eligible", "maxDiffLines": 200}),
            ),
            vec![needed("review.route.areas.docs.maxUncoveredShare")],
        ),
        (
            "a path without either threshold",
            example_with(
                &["route", "paths"],
                json!([{"glob": "docs/**", "route": "auto-merge-eligible"}]),
            ),
            vec![
                needed("review.route.paths[0].maxDiffLines"),
                needed("review.route.paths[0].maxUncoveredShare"),
            ],
        ),
        (
            "no sampleRate",
            {
                let mut review = spec_example();
                review["route"]
                    .as_object_mut()
                    .expect("an object")
                    .shift_remove("sampleRate");
                review
            },
            vec![marker_line(
                "review.route.sampleRate is needed when a route is \"auto-merge-eligible\"",
            )],
        ),
    ];
    for (label, review, lines) in cases {
        set_review(dir.path(), review);
        assert_fails_with(dir.path(), &lines, label);
    }
}

/// Case 7: a route that is not `auto-merge-eligible` needs no threshold and no
/// `sampleRate`. This passes at the base commit and guards the rule against a
/// threshold that every entry must name.
#[test]
fn no_threshold_and_no_sample_rate_without_an_auto_merge_route() {
    let dir = install();
    set_review(
        dir.path(),
        json!({
            "route": {
                "default": "owner-review",
                "areas": {"docs": {"route": "senior-review"}, "tools": {}},
                "paths": [{"glob": "tools/**", "route": "owner-review"}],
            }
        }),
    );
    assert_passes(dir.path(), "routes without thresholds");
}

/// Case 7: a share or a rate outside 0 to 1 is one line.
#[test]
fn a_share_or_a_rate_outside_0_to_1_is_one_line() {
    let dir = install();
    let cases = [
        (
            example_with(&["route", "areas", "docs", "maxUncoveredShare"], json!(1.5)),
            "review.route.areas.docs.maxUncoveredShare",
        ),
        (
            example_with(
                &["route", "areas", "docs", "maxUncoveredShare"],
                json!(-0.1),
            ),
            "review.route.areas.docs.maxUncoveredShare",
        ),
        (
            example_with(&["route", "sampleRate"], json!(2)),
            "review.route.sampleRate",
        ),
        (
            example_with(&["route", "sampleRate"], json!("often")),
            "review.route.sampleRate",
        ),
    ];
    for (review, key) in cases {
        set_review(dir.path(), review);
        assert_fails_with(
            dir.path(),
            &[marker_line(&format!("{key} must be a number from 0 to 1"))],
            key,
        );
    }
}

/// Case 7: the ends of the range 0 to 1 pass for a share and a rate. This
/// passes at the base commit and guards the lint against an exclusive range.
#[test]
fn a_share_and_a_rate_of_0_or_1_pass() {
    let dir = install();
    let mut review = spec_example();
    review["route"]["areas"]["docs"]["maxUncoveredShare"] = json!(1);
    review["route"]["sampleRate"] = json!(1.0);
    set_review(dir.path(), review.clone());
    assert_passes(dir.path(), "1");
    review["route"]["areas"]["docs"]["maxUncoveredShare"] = json!(0);
    review["route"]["sampleRate"] = json!(0);
    set_review(dir.path(), review);
    assert_passes(dir.path(), "0");
}

/// Spec 6.1 bounds a score threshold to 0 to 1, as it bounds a share and a rate
/// (the range of `--score`, spec 5.1): a threshold above 1 is one line that
/// names the key, and so is a threshold below 0 or that is no number.
#[test]
fn a_score_threshold_outside_0_to_1_is_one_line() {
    let dir = install();
    let cases = [
        (
            example_with(&["route", "ownerScore"], json!(50)),
            "review.route.ownerScore",
        ),
        (
            example_with(&["route", "seniorScore"], json!(1.5)),
            "review.route.seniorScore",
        ),
        (
            example_with(&["route", "seniorScore"], json!(2)),
            "review.route.seniorScore",
        ),
        (
            example_with(&["route", "ownerScore"], json!(-0.1)),
            "review.route.ownerScore",
        ),
        (
            example_with(&["route", "seniorScore"], json!(-50)),
            "review.route.seniorScore",
        ),
        (
            example_with(&["route", "seniorScore"], json!("high")),
            "review.route.seniorScore",
        ),
        (
            example_with(&["route", "ownerScore"], json!([0.5])),
            "review.route.ownerScore",
        ),
    ];
    for (review, key) in cases {
        set_review(dir.path(), review);
        assert_fails_with(
            dir.path(),
            &[marker_line(&format!("{key} must be a number from 0 to 1"))],
            key,
        );
    }
}

/// The ends of the range 0 to 1 pass for a score threshold, as they do for a
/// share and a rate.
#[test]
fn a_score_threshold_of_0_or_1_passes() {
    let dir = install();
    for (owner, senior) in [(json!(0), json!(1)), (json!(0.0), json!(1.0))] {
        let mut review = spec_example();
        review["route"]["ownerScore"] = owner.clone();
        review["route"]["seniorScore"] = senior.clone();
        set_review(dir.path(), review);
        assert_passes(
            dir.path(),
            &format!("ownerScore {owner}, seniorScore {senior}"),
        );
    }
}

/// Case 7: a negative number, or a number that is no integer where a count
/// stands, is one line.
#[test]
fn a_negative_number_is_one_line() {
    let dir = install();
    let cases = [
        (
            example_with(&["route", "areas", "docs", "maxDiffLines"], json!(-1)),
            "review.route.areas.docs.maxDiffLines",
        ),
        (
            example_with(&["route", "areas", "docs", "maxDiffLines"], json!(1.5)),
            "review.route.areas.docs.maxDiffLines",
        ),
        (
            example_with(&["bundle", "budgets", "light"], json!(-400)),
            "review.bundle.budgets.light",
        ),
        (
            example_with(&["bundle", "budgets", "light"], json!("400")),
            "review.bundle.budgets.light",
        ),
    ];
    for (review, key) in cases {
        set_review(dir.path(), review);
        assert_fails_with(
            dir.path(),
            &[marker_line(&format!(
                "{key} must be a non-negative integer"
            ))],
            key,
        );
    }
}

/// Case 7: an intent source that is no adapter of this binary is one line
/// that lists the adapters.
#[test]
fn an_intent_source_that_is_no_adapter_is_one_line() {
    let dir = install();
    set_review(
        dir.path(),
        example_with(&["intent", "sources"], json!(["work-item", "spec-kit"])),
    );
    assert_fails_with(
        dir.path(),
        &[marker_line(
            "review.intent.sources[1] \"spec-kit\" is no intent source; use one of: work-item, file, openspec",
        )],
        "an unknown source",
    );
}

/// Batch 38, spec 5.7: `review.sessions.trailers` is an array of trailer keys
/// (letters, digits, and `-`). A value with a fault is the one line of the spec, a
/// `sessions` value that is no object is one line of its own, and a sound list
/// passes.
#[test]
fn a_session_trailer_list_with_a_fault_is_one_line() {
    let dir = install();
    let list_line = marker_line(SESSION_TRAILERS_FAULT);
    for trailers in [
        json!("Session-Id"),
        json!(["Session-Id", 1]),
        json!([""]),
        json!(["a b", "c:d"]),
    ] {
        set_review(dir.path(), json!({"sessions": {"trailers": trailers}}));
        assert_fails_with(
            dir.path(),
            std::slice::from_ref(&list_line),
            &trailers.to_string(),
        );
    }
    set_review(dir.path(), json!({"sessions": []}));
    assert_fails_with(
        dir.path(),
        &[marker_line("review.sessions must be an object")],
        "a sessions value that is no object",
    );
    for trailers in [json!([]), json!(["Entire-Checkpoint", "Session-Id"])] {
        set_review(dir.path(), json!({"sessions": {"trailers": trailers}}));
        assert_passes(dir.path(), &trailers.to_string());
    }
}

/// The line of this file that the spec of batch 38 gives as a literal is in the spec
/// (`quality.user-facing-text-is-contract`).
#[test]
fn the_spec_lines_of_these_tests_are_in_the_spec() {
    let spec = fs::read_to_string(repo_root().join("docs").join("specs").join(SPEC_FILE))
        .expect("read the spec");
    let line = marker_line(SESSION_TRAILERS_FAULT);

    assert!(spec.contains(&format!("`{line}`")), "{line}");
}

/// Case 7: a `maxBytes` below 8,192 is one line, and 8,192 passes.
#[test]
fn a_max_bytes_below_8192_is_one_line() {
    let dir = install();
    let line = marker_line("review.record.maxBytes must be an integer of at least 8192");
    for value in [
        json!(100),
        json!(8191),
        json!(-1),
        json!("65536"),
        json!(9000.5),
    ] {
        set_review(
            dir.path(),
            example_with(&["record", "maxBytes"], value.clone()),
        );
        assert_fails_with(dir.path(), std::slice::from_ref(&line), &value.to_string());
    }
    set_review(
        dir.path(),
        example_with(&["record", "maxBytes"], json!(8192)),
    );
    assert_passes(dir.path(), "maxBytes of 8192");
}

/// Case 7: a value of the wrong type is one line that names the key.
#[test]
fn a_value_of_the_wrong_type_is_one_line_naming_its_key() {
    let dir = install();
    let cases = [
        (
            example_with(&["diffExclude"], json!("Cargo.lock")),
            "review.diffExclude must be an array of strings",
        ),
        (
            example_with(&["diffExclude"], json!(["Cargo.lock", 3])),
            "review.diffExclude must be an array of strings",
        ),
        (
            example_with(&["route"], json!([])),
            "review.route must be an object",
        ),
        (
            example_with(&["route", "areas"], json!([])),
            "review.route.areas must be an object",
        ),
        (
            example_with(&["route", "areas", "docs"], json!("owner-review")),
            "review.route.areas.docs must be an object",
        ),
        (
            example_with(&["route", "paths"], json!({})),
            "review.route.paths must be an array of objects",
        ),
        (
            example_with(&["route", "paths"], json!(["src/**"])),
            "review.route.paths[0] must be an object",
        ),
        (
            example_with(
                &["route", "paths"],
                json!([{"glob": 3, "route": "owner-review"}]),
            ),
            "review.route.paths[0].glob must be a string",
        ),
        (
            example_with(&["bundle"], json!("strict")),
            "review.bundle must be an object",
        ),
        (
            example_with(&["bundle", "budgets"], json!([400])),
            "review.bundle.budgets must be an object",
        ),
        (
            example_with(&["bundle", "exemptAuthors"], json!("dependabot[bot]")),
            "review.bundle.exemptAuthors must be an array of strings",
        ),
        (
            example_with(&["intent"], json!(["file"])),
            "review.intent must be an object",
        ),
        (
            example_with(&["intent", "sources"], json!("file")),
            "review.intent.sources must be an array of strings",
        ),
        (
            example_with(&["ci"], json!(".github/workflows/ci.yml")),
            "review.ci must be an object",
        ),
        (
            example_with(&["ci", "files"], json!(".github/workflows/ci.yml")),
            "review.ci.files must be an array of strings",
        ),
        (
            example_with(&["record"], json!(65536)),
            "review.record must be an object",
        ),
    ];
    for (review, line) in cases {
        set_review(dir.path(), review);
        assert_fails_with(dir.path(), &[marker_line(line)], line);
    }
}

/// Case 7: a key with a fault in each section gives one line for each fault,
/// in the order of the sections.
#[test]
fn several_faults_give_one_line_each_in_the_order_of_the_sections() {
    let dir = install();
    set_review(
        dir.path(),
        json!({
            "record": {"maxBytes": 100},
            "intent": {"sources": ["nope"]},
            "bundle": {"budgets": {"light": -1}},
            "route": {"default": "never"},
            "diffExclude": "Cargo.lock",
        }),
    );
    assert_fails_with(
        dir.path(),
        &[
            marker_line("review.diffExclude must be an array of strings"),
            marker_line(&format!("review.route.default must be {FALLBACK_ROUTES}")),
            marker_line("review.bundle.budgets.light must be a non-negative integer"),
            marker_line(
                "review.intent.sources[0] \"nope\" is no intent source; use one of: work-item, file, openspec",
            ),
            marker_line("review.record.maxBytes must be an integer of at least 8192"),
        ],
        "one fault in each section",
    );
}

/// Pin of unchanged behavior: `init`, `update`, and `pack add` keep the
/// `review` key as found, in its place, a malformed value included (the gate
/// `check-knowledge` names a defect; no writer refuses it). This passes at the
/// base commit and guards the key against a writer that rebuilds the marker.
#[test]
fn init_update_and_pack_add_keep_the_review_key() {
    for review in [spec_example(), json!(["not", "an", "object"])] {
        let dir = install();
        set_review(dir.path(), review.clone());
        for command in [vec!["init"], vec!["update"], vec!["pack", "add", "quality"]] {
            run_ok(dir.path(), &command);
            assert_eq!(
                read_json(dir.path(), ".houserules.json")["review"],
                review,
                "{command:?}"
            );
        }
    }
}

/// One entry of the topic `team` in the `global` area, with `check`.
fn entry(id: &str, check: Value) -> Value {
    json!({
        "id": id,
        "kind": "rule",
        "area": "global",
        "summary": "Declare what the check reads.",
        "body": ["Declare it."],
        "tags": ["coverage"],
        "source": {"date": "2026-10-24", "by": "user"},
        "check": check,
    })
}

/// Writes the local topic file `knowledge/team.json` with `entries`.
fn add_team_topic(dir: &Path, entries: Vec<Value>) {
    write_json(
        dir,
        "knowledge/team.json",
        &json!({
            "$schema": "./schema.json",
            "topic": "team",
            "title": "Team rules",
            "entries": entries,
        }),
    );
}

/// Case 7 (spec 4.4): `covers` on a `commits` check and on a `report-field`
/// check is one line each, because neither reads a file.
#[test]
fn covers_on_a_check_that_reads_no_file_is_one_line() {
    let dir = install();
    add_team_topic(
        dir.path(),
        vec![
            entry(
                "team.trailer",
                json!({
                    "type": "commits", "level": "warn",
                    "require_trailer": "Work-Item", "covers": true,
                }),
            ),
            entry(
                "team.report",
                json!({
                    "type": "report-field", "level": "warn", "if": "**",
                    "field": "concerns", "covers": true,
                }),
            ),
        ],
    );
    assert_fails_with(
        dir.path(),
        &[
            "knowledge/team.json team.trailer: check \"commits\" cannot declare \"covers\": it reads no file".to_string(),
            "knowledge/team.json team.report: check \"report-field\" cannot declare \"covers\": it reads no file".to_string(),
        ],
        "covers on a commits check and a report-field check",
    );
}

/// Spec 4.4: `covers` is a boolean.
#[test]
fn covers_that_is_no_boolean_is_the_line_of_the_schema() {
    let dir = install();
    add_team_topic(
        dir.path(),
        vec![entry(
            "team.lint",
            json!({
                "type": "grep-absent", "level": "fail", "files": ["src/**"],
                "pattern": "x", "scope": "changed", "covers": "yes",
            }),
        )],
    );
    assert_fails_with(
        dir.path(),
        &["knowledge/team.json.entries[0].check.covers: must be boolean".to_string()],
        "covers that is a string",
    );
}

/// Spec 4.4: the five checks that read files take `covers`, true or false.
#[test]
fn covers_on_a_check_that_reads_files_passes() {
    let dir = install();
    edit_marker(dir.path(), |marker| {
        marker.insert(
            "runners".to_string(),
            json!({"lint": {
                "command": ["sh", "-c", "true"], "parser": "exit-code", "install": "none",
            }}),
        );
    });
    let checks = [
        json!({
            "type": "grep-absent", "level": "fail", "files": ["src/**"],
            "pattern": "x", "scope": "changed", "covers": true,
        }),
        json!({
            "type": "co-change", "level": "warn", "if": "src/**", "then": "docs/**",
            "covers": true,
        }),
        json!({"type": "diff-append-only", "level": "fail", "files": ["docs/**"], "covers": true}),
        json!({"type": "durable-sha", "level": "warn", "files": ["docs/**"], "covers": false}),
        json!({"type": "runner", "level": "fail", "tool": "lint", "covers": true}),
    ];
    let entries = checks
        .into_iter()
        .enumerate()
        .map(|(index, check)| entry(&format!("team.check{index}"), check))
        .collect();
    add_team_topic(dir.path(), entries);
    assert_passes(dir.path(), "covers on the five checks that read files");
}

/// Case 9: `update` gives a knowledge schema that equals the content it
/// shipped before the property `covers`, and so holds its baseline, the
/// schema of the kit.
#[test]
fn update_gives_the_knowledge_schema_the_covers_property() {
    let dir = install();
    let path = dir.path().join("knowledge/schema.json");
    let mut older: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("read the schema")).expect("parse");
    older["$defs"]["check"]["properties"]
        .as_object_mut()
        .expect("the properties of a check")
        .shift_remove("covers");
    let older_text = format!(
        "{}\n",
        serde_json::to_string_pretty(&older).expect("serialize")
    );
    fs::write(&path, &older_text).expect("write the older schema");
    edit_marker(dir.path(), |marker| {
        marker["baselines"]["knowledge/schema.json"] = json!(sha256_hex(older_text.as_bytes()));
    });
    commit_all(dir.path(), "chore: install");

    run_ok(dir.path(), &["update"]);

    let kit = fs::read_to_string(repo_root().join("template/knowledge/schema.json"))
        .expect("read the schema of the kit");
    let updated = fs::read_to_string(&path).expect("read the updated schema");
    let parsed: Value = serde_json::from_str(&updated).expect("parse the updated schema");
    assert_eq!(
        parsed["$defs"]["check"]["properties"]["covers"],
        json!({"type": "boolean"})
    );
    assert_eq!(updated, kit);
}

/// A pack of the topic `demo` at pack schema 1.3.0, with a `grep-absent` check
/// that sets `covers` when `covers` is `Some`.
fn demo_pack(covers: Option<bool>, schema: &str) -> Value {
    let mut check = json!({
        "type": "grep-absent", "level": "fail", "files": ["src/**"],
        "pattern": "x", "scope": "changed",
    });
    if let Some(covers) = covers {
        check["covers"] = json!(covers);
    }
    json!({
        "name": "demo",
        "version": "1.0.0",
        "schema": schema,
        "topics": {"demo": {"title": "Demo rules"}},
        "areas": {"global": {"paths": []}},
        "entries": [entry("demo.lint", check)],
    })
}

/// Spec 4.4 with the pack schema: a pack copy that declares
/// `houserules/pack@1.3.0` and sets `covers` on a check installs and passes
/// `check-knowledge`.
#[test]
fn a_pack_copy_at_1_3_0_with_covers_validates() {
    let dir = install();
    write_json(
        dir.path(),
        "team-pack.json",
        &demo_pack(Some(true), "houserules/pack@1.3.0"),
    );
    let pack = dir.path().join("team-pack.json");

    run_ok(
        dir.path(),
        &["pack", "add", pack.to_str().expect("a UTF-8 path")],
    );

    assert_passes(dir.path(), "the installed pack");
}

/// Spec 4.4 with the overrides schema: an override may set `covers` on the
/// check of a pack entry.
#[test]
fn an_override_can_set_covers_on_the_check_of_a_pack_entry() {
    let dir = install();
    write_json(
        dir.path(),
        "team-pack.json",
        &demo_pack(None, "houserules/pack@1.0.0"),
    );
    let pack = dir.path().join("team-pack.json");
    run_ok(
        dir.path(),
        &["pack", "add", pack.to_str().expect("a UTF-8 path")],
    );
    write_json(
        dir.path(),
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.2.0",
            "overrides": [{"id": "demo.lint", "fields": {"check": {
                "type": "grep-absent", "level": "fail", "files": ["src/**"],
                "pattern": "x", "scope": "changed", "covers": true,
            }}}],
        }),
    );

    let output = run(dir.path(), &["check-knowledge"]);

    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(0),
            "override demo.lint: check\nknowledge: ok\n".to_string(),
            String::new()
        ),
    );
}

/// The help of `check-knowledge` states the lines of the `review` key and of
/// `covers`.
#[test]
fn check_knowledge_help_names_the_review_key_and_covers() {
    states(
        "check-knowledge --help",
        &help_of("check-knowledge"),
        &[
            "Checks the `review` key of `.houserules.json`: a route must be `auto-merge-eligible`, `owner-review`, or `senior-review`; an area of `review.route.areas` must be an area of `knowledge/areas.json`; a glob must compile; a route of `auto-merge-eligible` needs `maxDiffLines` and `maxUncoveredShare` in its area or path entry, and `review.route.sampleRate` in the key; `review.route.default` and `review.route.warn` must not be `auto-merge-eligible`; a share, rate, or score must lie between 0 and 1; a count must not be negative; an intent source must be one that this binary reads; `review.record.maxBytes` must be at least 8192; one line names the key of each fault",
            "Checks that a check sets `covers` only when it reads files: a `commits` check and a `report-field` check read none",
        ],
    );
}
