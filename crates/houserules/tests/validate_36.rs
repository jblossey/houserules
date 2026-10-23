//! The report form of batch 36 (spec 4 and 12; cases 5 and 10): the
//! deliverables schema's `run`, `self_review`, `citations`, and `reran`, and the
//! fix-round invariant of `validate`, which reads the captures of a fix round.
//! Each case runs the compiled binary against a scratch install that `init`
//! made, with the files of `tests/fixtures/report-36/` (a valid report of the
//! new form and its captures) copied into it.
//!
//! A test whose doc comment calls it a pin asserts behavior that the code held
//! before the change this file pins.

mod install_shape;
mod stats_support;
mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value, json};
use tempfile::TempDir;
use tracker_support::{
    AUDIT_LINE_ZERO, commit_all, copy_dir, edit_marker, git, read_json, repo_root, run, run_ok,
    scratch_git_repo, sha256_hex, stderr, stdout,
};

/// The deliverables schema of an install.
const DELIVERABLES: &str = ".claude/schemas/deliverables.json";

/// The commit on main that holds the kit of batch 35: the last kit before the
/// report form of batch 36.
const BATCH_35: &str = "ce0fc0f";

/// The folder of the fixture report: `report.json` and its `captures/`.
fn report_fixture() -> PathBuf {
    repo_root().join("tests").join("fixtures").join("report-36")
}

/// The fixture report, parsed.
fn report() -> Value {
    let path = report_fixture().join("report.json");
    serde_json::from_str(&fs::read_to_string(&path).expect("read the fixture report"))
        .expect("parse the fixture report")
}

/// The fixture report after `edit`.
fn report_with(edit: impl FnOnce(&mut Value)) -> Value {
    let mut value = report();
    edit(&mut value);
    value
}

/// A scratch install that `init` made, with the fixture report and its
/// captures copied below its root.
fn install_with_report() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    copy_dir(&report_fixture(), dir.path());
    dir
}

/// Writes `value` to `name` in `dir` and runs `validate` on it. Returns the exit
/// code and the errors of the one result row, each without the path of the file
/// that leads it, so that a case does not depend on how the platform prints
/// the path.
fn validate(dir: &Path, name: &str, value: &Value) -> (i32, Vec<String>) {
    fs::write(
        dir.join(name),
        serde_json::to_string_pretty(value).expect("serialize"),
    )
    .expect("write the report");
    validate_file(dir, name)
}

/// Runs `validate` on the file `name` of `dir` as it is.
fn validate_file(dir: &Path, name: &str) -> (i32, Vec<String>) {
    let file = dir.join(name);
    let output = run(dir, &["validate", file.to_str().expect("a UTF-8 path")]);
    let code = output.status.code().expect("an exit code");
    if code == 2 {
        return (code, vec![stderr(&output)]);
    }
    let rows: Value = serde_json::from_str(&stdout(&output)).expect("validate prints JSON");
    let errors = rows[0]["errors"]
        .as_array()
        .expect("errors is an array")
        .iter()
        .map(|error| {
            let error = error.as_str().expect("an error is a string");
            error
                .split_once(name)
                .unwrap_or_else(|| panic!("{error:?} does not name {name:?}"))
                .1
                .to_string()
        })
        .collect();
    (code, errors)
}

/// The result of `validate` for one finding: exit 1 and exactly `error`.
fn refused_with(error: &str) -> (i32, Vec<String>) {
    (1, vec![error.to_string()])
}

/// Runs `capture` in `cwd` of `sh -c <script>` and writes the capture to
/// `out`. Returns `out`.
fn capture_of(cwd: &Path, out: &str, script: &str) -> String {
    let status = tracker_support::houserules()
        .current_dir(cwd)
        .args(["capture", "--out", out, "--force", "--", "sh", "-c", script])
        .output()
        .expect("run capture")
        .status;
    assert!(
        status.success(),
        "capture of {script:?} exited with {status}"
    );
    out.to_string()
}

/// The JSON pointers of the five places that hold a run, with the label that
/// `validate` gives each one.
const RUN_PLACES: [(&str, &str); 5] = [
    ("/tests/0", ".tests[0]"),
    ("/live_run/0", ".live_run[0]"),
    ("/tdd/0/red", ".tdd[0].red"),
    ("/tdd/0/green", ".tdd[0].green"),
    ("/fix_rounds/0/tests/0", ".fix_rounds[0].tests[0]"),
];

/// The run of `value` at `pointer`, for an edit.
fn run_at<'a>(value: &'a mut Value, pointer: &str) -> &'a mut Map<String, Value> {
    value
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("no run at {pointer}"))
}

// --- The fixture -----------------------------------------------------------

/// Spec 4 and case 5: the fixture report, a valid report of the new form,
/// passes `validate` as it is committed.
#[test]
fn the_fixture_report_validates_as_committed() {
    let dir = install_with_report();
    assert_eq!(validate_file(dir.path(), "report.json"), (0, vec![]));
}

/// A pin of the fixture: each run records the SHA-256 of its capture file.
#[test]
fn the_fixture_runs_record_the_hash_of_their_captures() {
    let value = report();
    let mut runs = vec![];
    for (pointer, _) in RUN_PLACES {
        runs.push(value.pointer(pointer).expect("a run").clone());
    }
    for run in runs {
        let capture = run["capture"].as_str().expect("a capture path");
        let bytes = fs::read(report_fixture().join(capture)).expect("read the capture");
        assert_eq!(run["sha256"], json!(sha256_hex(&bytes)), "{capture}");
    }
}

// --- `run` ----------------------------------------------------------------

/// Spec 4: `output` leaves the schema in every place that holds a run.
#[test]
fn a_run_with_output_is_refused_in_every_place() {
    let dir = install_with_report();
    for (pointer, label) in RUN_PLACES {
        let value = report_with(|r| {
            run_at(r, pointer).insert("output".to_string(), json!("ok\n"));
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(&format!("{label}: unknown field \"output\"")),
            "{label}"
        );
    }
}

/// Spec 4: a run holds `capture`, `sha256`, `command`, and `exit`.
#[test]
fn a_run_without_a_capture_a_hash_a_command_or_an_exit_is_refused() {
    let dir = install_with_report();
    for key in ["capture", "sha256", "command", "exit"] {
        let value = report_with(|r| {
            run_at(r, "/tests/0").remove(key);
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(&format!(".tests[0]: missing \"{key}\"")),
            "{key}"
        );
    }
}

/// Spec 4: a run without `excerpt` is valid; an unknown field is refused.
#[test]
fn a_run_may_leave_out_its_excerpt_and_may_hold_no_other_field() {
    let dir = install_with_report();
    let without = report_with(|r| {
        run_at(r, "/tests/0").remove("excerpt");
    });
    assert_eq!(validate(dir.path(), "r.json", &without), (0, vec![]));
    let extra = report_with(|r| {
        run_at(r, "/tests/0").insert("note".to_string(), json!("x"));
    });
    assert_eq!(
        validate(dir.path(), "r.json", &extra),
        refused_with(".tests[0]: unknown field \"note\"")
    );
}

/// Spec 4: `sha256` is 64 lowercase hex digits, and nothing else.
#[test]
fn a_run_hash_is_64_lowercase_hex_digits() {
    let dir = install_with_report();
    let digits = "0123456789abcdef".repeat(4);
    let line_feed = format!("{digits}\n");
    for bad in [
        digits.to_uppercase(),
        digits[..63].to_string(),
        format!("{digits}0"),
        "g".repeat(64),
        line_feed,
        String::new(),
    ] {
        let value = report_with(|r| {
            run_at(r, "/tests/0").insert("sha256".to_string(), json!(bad));
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(".tests[0].sha256: must match ^[0-9a-f]{64}$"),
            "{bad:?}"
        );
    }
    let value = report_with(|r| {
        run_at(r, "/tests/0").insert("sha256".to_string(), json!(digits));
    });
    assert_eq!(validate(dir.path(), "r.json", &value), (0, vec![]));
}

/// Spec 4: `exit` is the header's exit, an integer or `null`.
#[test]
fn a_run_exit_is_an_integer_or_null() {
    let dir = install_with_report();
    for good in [json!(0), json!(2), json!(300), json!(null)] {
        let value = report_with(|r| {
            run_at(r, "/tests/0").insert("exit".to_string(), good.clone());
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            (0, vec![]),
            "{good}"
        );
    }
    for bad in [json!("2"), json!(1.5), json!(true)] {
        let value = report_with(|r| {
            run_at(r, "/tests/0").insert("exit".to_string(), bad.clone());
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(".tests[0].exit: must be integer or null"),
            "{bad}"
        );
    }
}

/// Spec 4: `capture` and `command` are non-empty text.
#[test]
fn a_run_capture_and_command_are_not_empty() {
    let dir = install_with_report();
    for key in ["capture", "command"] {
        let value = report_with(|r| {
            run_at(r, "/tests/0").insert(key.to_string(), json!(""));
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(&format!(".tests[0].{key}: shorter than 1")),
            "{key}"
        );
    }
}

/// Spec 4: an `excerpt` holds 1 to 1000 UTF-16 code units, the unit of the
/// validator, so a character outside the Basic Multilingual Plane counts 2.
#[test]
fn an_excerpt_holds_1_to_1000_utf16_code_units() {
    let dir = install_with_report();
    let astral = "\u{1F600}";
    for good in ["x".to_string(), "x".repeat(1000), astral.repeat(500)] {
        let value = report_with(|r| {
            run_at(r, "/tests/0").insert("excerpt".to_string(), json!(good));
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            (0, vec![]),
            "{} units",
            good.encode_utf16().count()
        );
    }
    for too_long in ["x".repeat(1001), astral.repeat(501)] {
        let value = report_with(|r| {
            run_at(r, "/tests/0").insert("excerpt".to_string(), json!(too_long));
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(".tests[0].excerpt: longer than 1000 characters"),
            "{} units",
            too_long.encode_utf16().count()
        );
    }
    let empty = report_with(|r| {
        run_at(r, "/tests/0").insert("excerpt".to_string(), json!(""));
    });
    assert_eq!(
        validate(dir.path(), "r.json", &empty),
        refused_with(".tests[0].excerpt: shorter than 1")
    );
}

/// Spec 4 and case 5: a report of the old form (`command` and `output`, no
/// capture) fails `validate` against the new schema. This is a red of spec 12;
/// the remedy is to write the report in the new form.
#[test]
fn an_old_form_report_fails_validate_against_the_new_schema() {
    let dir = install_with_report();
    fs::copy(
        stats_support::fixture("old-form").join("task-1-report.json"),
        dir.path().join("old.json"),
    )
    .expect("copy the old-form report");
    let (code, errors) = validate_file(dir.path(), "old.json");
    assert_eq!(code, 1, "{errors:?}");
    for expected in [
        ".tests[0]: missing \"capture\"",
        ".tests[0]: missing \"sha256\"",
        ".tests[0]: unknown field \"output\"",
        ".tdd[0].red: unknown field \"output\"",
        ".fix_rounds[0].tests[0]: missing \"capture\"",
        ".self_review[0]: must be object",
    ] {
        assert!(
            errors.iter().any(|error| error == expected),
            "{expected}: {errors:?}"
        );
    }
}

// --- `self_review` ---------------------------------------------------------

/// Spec 4: a `self_review` item is `{ claim, evidence? }`, and a string item
/// of the old form is refused, as is the one-character-per-item shredding of a
/// string appended to the list.
#[test]
fn a_self_review_item_is_an_object_with_a_claim_and_optional_evidence() {
    let dir = install_with_report();
    let string_item = report_with(|r| r["self_review"] = json!(["The diff holds one change."]));
    assert_eq!(
        validate(dir.path(), "r.json", &string_item),
        refused_with(".self_review[0]: must be object")
    );
    let shredded: Vec<Value> = "Re-read the whole change."
        .chars()
        .map(|c| json!(c.to_string()))
        .collect();
    let value = report_with(|r| r["self_review"] = json!(shredded));
    let (code, errors) = validate(dir.path(), "r.json", &value);
    assert_eq!(code, 1, "{errors:?}");
    assert_eq!(errors.len(), 25, "{errors:?}");
    assert_eq!(errors[0], ".self_review[0]: must be object");

    let no_claim = report_with(|r| r["self_review"] = json!([{"evidence": ["captures/red.txt"]}]));
    assert_eq!(
        validate(dir.path(), "r.json", &no_claim),
        refused_with(".self_review[0]: missing \"claim\"")
    );
    let short = report_with(|r| r["self_review"] = json!([{"claim": "ab"}]));
    assert_eq!(
        validate(dir.path(), "r.json", &short),
        refused_with(".self_review[0].claim: shorter than 3")
    );
    let extra = report_with(|r| r["self_review"] = json!([{"claim": "The claim.", "note": "x"}]));
    assert_eq!(
        validate(dir.path(), "r.json", &extra),
        refused_with(".self_review[0]: unknown field \"note\"")
    );
    let one_string = report_with(|r| {
        r["self_review"] = json!([{"claim": "The claim.", "evidence": "captures/red.txt"}]);
    });
    assert_eq!(
        validate(dir.path(), "r.json", &one_string),
        refused_with(".self_review[0].evidence: must be array")
    );
    let empty_path = report_with(|r| {
        r["self_review"] = json!([{"claim": "The claim.", "evidence": [""]}]);
    });
    assert_eq!(
        validate(dir.path(), "r.json", &empty_path),
        refused_with(".self_review[0].evidence[0]: shorter than 1")
    );
}

/// Spec 4: an item with evidence, an item without, and an empty list pass.
#[test]
fn a_self_review_item_may_leave_out_its_evidence() {
    let dir = install_with_report();
    assert_eq!(
        report()["self_review"].as_array().map(|items| items
            .iter()
            .map(|item| item.get("evidence").is_some())
            .collect()),
        Some(vec![true, false]),
        "the fixture holds one item with evidence and one without"
    );
    assert_eq!(validate(dir.path(), "r.json", &report()), (0, vec![]));
    let empty = report_with(|r| r["self_review"] = json!([]));
    assert_eq!(validate(dir.path(), "r.json", &empty), (0, vec![]));
    let empty_evidence =
        report_with(|r| r["self_review"] = json!([{"claim": "The claim.", "evidence": []}]));
    assert_eq!(validate(dir.path(), "r.json", &empty_evidence), (0, vec![]));
}

// --- `citations` -----------------------------------------------------------

/// Spec 4: `citations` is optional.
#[test]
fn citations_are_optional() {
    let dir = install_with_report();
    let value = report_with(|r| {
        r.as_object_mut().expect("an object").remove("citations");
    });
    assert_eq!(validate(dir.path(), "r.json", &value), (0, vec![]));
}

/// Spec 4: a citation holds `path`, `line`, `anchor`, and optionally `end` and
/// `sha256`.
#[test]
fn a_citation_needs_a_path_a_line_and_an_anchor() {
    let dir = install_with_report();
    let cited = |edit: &dyn Fn(&mut Map<String, Value>)| {
        report_with(|r| edit(r["citations"][0].as_object_mut().expect("a citation")))
    };
    for key in ["path", "line", "anchor"] {
        let value = cited(&|c| {
            c.remove(key);
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(&format!(".citations[0]: missing \"{key}\"")),
            "{key}"
        );
    }
    for key in ["end", "sha256"] {
        let value = cited(&|c| {
            c.remove(key);
        });
        assert_eq!(validate(dir.path(), "r.json", &value), (0, vec![]), "{key}");
    }
    let extra = cited(&|c| {
        c.insert("note".to_string(), json!("x"));
    });
    assert_eq!(
        validate(dir.path(), "r.json", &extra),
        refused_with(".citations[0]: unknown field \"note\"")
    );
}

/// Spec 4: `line` and `end` are integers of at least 1.
#[test]
fn a_citation_line_and_end_are_integers_of_at_least_1() {
    let dir = install_with_report();
    for key in ["line", "end"] {
        for bad in [json!(0), json!(-3)] {
            let value = report_with(|r| {
                r["citations"][0][key] = bad.clone();
            });
            assert_eq!(
                validate(dir.path(), "r.json", &value),
                refused_with(&format!(".citations[0].{key}: below 1")),
                "{key} {bad}"
            );
        }
        let value = report_with(|r| {
            r["citations"][0][key] = json!("3");
        });
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(&format!(".citations[0].{key}: must be integer")),
            "{key}"
        );
    }
}

/// Spec 4: an `anchor` holds three non-whitespace characters in a row, so the
/// anchor `{` that occurs in many places is refused, and the hash of a
/// citation has the form of the hash of a run.
#[test]
fn a_citation_anchor_holds_three_non_whitespace_characters_in_a_row() {
    let dir = install_with_report();
    for bad in ["{", "ab", "a b c", "  \t  ", "", "x\ny z"] {
        let value = report_with(|r| r["citations"][0]["anchor"] = json!(bad));
        let (code, errors) = validate(dir.path(), "r.json", &value);
        assert_eq!(code, 1, "{bad:?}: {errors:?}");
        assert!(
            errors
                .iter()
                .any(|error| error == ".citations[0].anchor: must match \\S{3}"),
            "{bad:?}: {errors:?}"
        );
    }
    for good in ["fn main", "let", "});", "a b cde"] {
        let value = report_with(|r| r["citations"][0]["anchor"] = json!(good));
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            (0, vec![]),
            "{good:?}"
        );
    }
    let bad_hash = report_with(|r| r["citations"][0]["sha256"] = json!("ABC"));
    assert_eq!(
        validate(dir.path(), "r.json", &bad_hash),
        refused_with(".citations[0].sha256: must match ^[0-9a-f]{64}$")
    );
}

// --- `reran` ---------------------------------------------------------------

/// Spec 4: a finding may name the probe that its round re-ran; `reran` is
/// optional and holds non-empty text.
#[test]
fn a_finding_may_name_the_probe_it_reran() {
    let dir = install_with_report();
    assert_eq!(
        report()["fix_rounds"][0]["findings"][0]["reran"],
        json!("captures/red.txt"),
        "the fixture finding names its probe"
    );
    assert_eq!(validate(dir.path(), "r.json", &report()), (0, vec![]));
    let without = report_with(|r| {
        r["fix_rounds"][0]["findings"][0]
            .as_object_mut()
            .expect("a finding")
            .remove("reran");
    });
    assert_eq!(validate(dir.path(), "r.json", &without), (0, vec![]));
    for (bad, error) in [(json!(""), "shorter than 1"), (json!(3), "must be string")] {
        let value = report_with(|r| r["fix_rounds"][0]["findings"][0]["reran"] = bad.clone());
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            refused_with(&format!(".fix_rounds[0].findings[0].reran: {error}")),
            "{bad}"
        );
    }
}

// --- The fix-round invariant -------------------------------------------------

/// Spec 4 and case 5: a fix round whose audit capture holds `"skipped": 2`
/// fails with the line of the invariant, which names the `capture` field.
#[test]
fn a_fix_round_audit_capture_that_skipped_checks_is_refused() {
    let dir = install_with_report();
    let capture = capture_of(
        dir.path(),
        "captures/skipped.txt",
        r#"printf '%s\n' '{"summary": {"skipped": 2}}'"#,
    );
    let value = report_with(|r| {
        r["fix_rounds"][0]["tests"][0]["capture"] = json!(capture);
    });
    assert_eq!(
        validate(dir.path(), "r.json", &value),
        refused_with(
            ".fix_rounds[0].tests[0].capture: audit summary skipped is 2; re-run audit with --report"
        )
    );
}

/// Spec 4: the invariant names the round and the test of each finding.
#[test]
fn the_invariant_names_each_round_and_test() {
    let dir = install_with_report();
    let skipped = capture_of(
        dir.path(),
        "captures/skipped.txt",
        r#"printf '%s\n' '{"summary": {"skipped": 3}}'"#,
    );
    let value = report_with(|r| {
        let ok = r["fix_rounds"][0]["tests"][0].clone();
        let mut bad = ok.clone();
        bad["capture"] = json!(skipped);
        r["fix_rounds"][0]["tests"] = json!([ok, bad]);
        let mut second = r["fix_rounds"][0].clone();
        second["round"] = json!(2);
        second["tests"] = json!([bad]);
        r["fix_rounds"] = json!([r["fix_rounds"][0].clone(), second]);
    });
    assert_eq!(
        validate(dir.path(), "r.json", &value),
        (
            1,
            vec![
                ".fix_rounds[0].tests[1].capture: audit summary skipped is 3; re-run audit with --report"
                    .to_string(),
                ".fix_rounds[1].tests[0].capture: audit summary skipped is 3; re-run audit with --report"
                    .to_string(),
            ]
        )
    );
}

/// Spec 4: the invariant reads the output of the capture and no other line:
/// an audit result that the header of the capture carries in its `argv`
/// raises nothing.
#[test]
fn the_invariant_reads_the_output_and_not_the_header() {
    let dir = install_with_report();
    let capture = capture_of(
        dir.path(),
        "captures/in-argv.txt",
        r#"true # {"summary": {"skipped": 3}}"#,
    );
    let value = report_with(|r| {
        r["fix_rounds"][0]["tests"][0]["capture"] = json!(capture);
    });
    assert_eq!(validate(dir.path(), "r.json", &value), (0, vec![]));
}

/// Spec 4: a capture that `validate` cannot read is left to
/// `check-report-claims`: a missing file and a file that is no capture raise
/// nothing here, even when the file holds an audit result that skipped checks.
#[test]
fn a_capture_that_cannot_be_read_is_left_to_check_report_claims() {
    let dir = install_with_report();
    let missing = report_with(|r| {
        r["fix_rounds"][0]["tests"][0]["capture"] = json!("captures/missing.txt");
    });
    assert_eq!(validate(dir.path(), "r.json", &missing), (0, vec![]));
    fs::write(
        dir.path().join("captures").join("plain.txt"),
        "{\"summary\": {\"skipped\": 2}}\n",
    )
    .expect("write a file that is no capture");
    let plain = report_with(|r| {
        r["fix_rounds"][0]["tests"][0]["capture"] = json!("captures/plain.txt");
    });
    assert_eq!(validate(dir.path(), "r.json", &plain), (0, vec![]));
}

/// Spec 4: the path of a capture is relative to the install root or absolute;
/// an absolute path outside the root is read.
#[test]
fn an_absolute_capture_path_outside_the_install_root_is_read() {
    let dir = install_with_report();
    let elsewhere = tempfile::tempdir().expect("tempdir");
    capture_of(
        elsewhere.path(),
        "audit.txt",
        r#"printf '%s\n' '{"summary": {"skipped": 1}}'"#,
    );
    let absolute = elsewhere.path().join("audit.txt");
    let value = report_with(|r| {
        r["fix_rounds"][0]["tests"][0]["capture"] = json!(absolute.to_str().expect("UTF-8"));
    });
    assert_eq!(
        validate(dir.path(), "r.json", &value),
        refused_with(
            ".fix_rounds[0].tests[0].capture: audit summary skipped is 1; re-run audit with --report"
        )
    );
}

/// A pin: a report with a status that is not terminal raises no finding of the
/// invariant, as it raises none for its top-level audit.
#[test]
fn a_blocked_report_is_exempt_from_the_invariant() {
    let dir = install_with_report();
    let capture = capture_of(
        dir.path(),
        "captures/skipped.txt",
        r#"printf '%s\n' '{"summary": {"skipped": 2}}'"#,
    );
    let value = report_with(|r| {
        r["status"] = json!("BLOCKED");
        r["fix_rounds"][0]["tests"][0]["capture"] = json!(capture);
    });
    assert_eq!(validate(dir.path(), "r.json", &value), (0, vec![]));
}

// --- Upgrade ---------------------------------------------------------------

/// The bytes of `template/<DELIVERABLES>` at the kit of batch 35.
fn schema_of_batch_35() -> Vec<u8> {
    let output = Command::new("git")
        .arg("show")
        .arg(format!("{BATCH_35}:template/{DELIVERABLES}"))
        .current_dir(repo_root())
        .output()
        .expect("run git show");
    assert!(
        output.status.success(),
        "git show {BATCH_35}: {}",
        stderr(&output)
    );
    output.stdout
}

/// The bytes of `template/<DELIVERABLES>` in this checkout.
fn schema_of_the_template() -> Vec<u8> {
    fs::read(repo_root().join("template").join(DELIVERABLES)).expect("read the template schema")
}

/// An install with no tracker whose deliverables schema is the one of batch 35,
/// at its baseline, as `update` of a batch-35 install finds it. The fixture
/// report is copied in.
fn batch_35_install(schema: &[u8]) -> TempDir {
    let dir = install_with_report();
    fs::write(dir.path().join(DELIVERABLES), schema).expect("write the schema");
    edit_marker(dir.path(), |marker| {
        marker
            .get_mut("baselines")
            .and_then(Value::as_object_mut)
            .expect("baselines")
            .insert(
                DELIVERABLES.to_string(),
                json!(sha256_hex(&schema_of_batch_35())),
            );
    });
    commit_all(dir.path(), "chore: a batch-35 install");
    dir
}

/// The `$defs.run.required` list of the schema in `dir`.
fn required_of_run(dir: &Path) -> Value {
    read_json(dir, DELIVERABLES)["$defs"]["run"]["required"].clone()
}

/// Case 10: `update` of a batch-35 install replaces the deliverables schema
/// that the project has not changed, with the schema of this kit, and a second
/// `update` changes nothing.
#[test]
fn update_replaces_an_unmodified_batch_35_schema() {
    let dir = batch_35_install(&schema_of_batch_35());
    assert_eq!(required_of_run(dir.path()), json!(["command", "output"]));

    let output = run_ok(dir.path(), &["update"]);

    let lines: Vec<String> = stdout(&output).lines().map(str::to_string).collect();
    assert!(
        lines.contains(&format!("wrote {DELIVERABLES}")),
        "{lines:?}"
    );
    assert_eq!(
        fs::read(dir.path().join(DELIVERABLES)).expect("read the schema"),
        schema_of_the_template()
    );
    assert_eq!(
        required_of_run(dir.path()),
        json!(["capture", "sha256", "command", "exit"])
    );
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["baselines"][DELIVERABLES],
        json!(sha256_hex(&schema_of_the_template()))
    );
    assert_eq!(validate_file(dir.path(), "report.json"), (0, vec![]));

    commit_all(dir.path(), "chore: update");
    let second = stdout(&run_ok(dir.path(), &["update"]));
    assert!(
        !second.contains(&format!("wrote {DELIVERABLES}")),
        "{second}"
    );
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

/// Case 10 and a red of spec 12: `update` keeps a deliverables schema that the
/// project changed and says so, and a report of the new form fails `validate`
/// against the kept schema, because that schema still requires `output`.
///
/// A pin: the kept schema, the line, and the baseline are what `update` did
/// before this change.
#[test]
fn update_keeps_a_modified_schema_and_a_new_form_report_fails_against_it() {
    let modified = String::from_utf8(schema_of_batch_35())
        .expect("UTF-8")
        .replace(
            "\"title\": \"Agent deliverables\"",
            "\"title\": \"Our deliverables\"",
        )
        .into_bytes();
    assert_ne!(
        modified,
        schema_of_batch_35(),
        "the edit changes the schema"
    );
    let dir = batch_35_install(&modified);

    let output = run_ok(dir.path(), &["update"]);

    let lines: Vec<String> = stdout(&output).lines().map(str::to_string).collect();
    assert!(
        lines.contains(&format!("kept {DELIVERABLES} (locally modified)")),
        "{lines:?}"
    );
    assert_eq!(
        fs::read(dir.path().join(DELIVERABLES)).expect("read the schema"),
        modified
    );
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["baselines"][DELIVERABLES],
        json!(sha256_hex(&schema_of_batch_35()))
    );

    let (code, errors) = validate_file(dir.path(), "report.json");
    assert_eq!(code, 1, "{errors:?}");
    for expected in [
        ".tests[0]: missing \"output\"",
        ".tests[0]: unknown field \"capture\"",
        ": unknown field \"citations\"",
        ".self_review[0]: must be string",
    ] {
        assert!(
            errors.iter().any(|error| error == expected),
            "{expected}: {errors:?}"
        );
    }
}

// --- `stats` and `audit --workspace` -----------------------------------------

/// A git repository that holds a loadable knowledge base with the judged rules
/// `j.one`, `j.two`, `j.three`, and `j.unused` and the deterministic rules
/// `d.one`, `d.two`, and `d.three`, committed as the base commit; a second
/// commit changes `src/example.rs`. Returns the repository, the base, and the
/// head.
fn knowledge_repository() -> (TempDir, String, String) {
    let root = scratch_git_repo();
    stats_support::write_knowledge_base(root.path());
    let base = commit_all(root.path(), "chore: knowledge");
    fs::create_dir_all(root.path().join("src")).expect("create src/");
    fs::write(
        root.path().join("src").join("example.rs"),
        "fn example() {}\n",
    )
    .expect("write example.rs");
    let head = commit_all(root.path(), "feat: example");
    (root, base, head)
}

/// Replaces, in every string of `value`, each `(from, to)` of `replacements` in
/// order.
fn replace_in_strings(value: &mut Value, replacements: &[(String, String)]) {
    match value {
        Value::String(text) => {
            for (from, to) in replacements {
                *text = text.replace(from.as_str(), to);
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| replace_in_strings(item, replacements)),
        Value::Object(map) => map
            .values_mut()
            .for_each(|item| replace_in_strings(item, replacements)),
        _ => {}
    }
}

/// The JSON that a successful run printed, with the workspace and the two
/// commits (whole and as the seven characters that `audit` prints) replaced by
/// placeholders.
fn normalized(stdout_text: &str, workspace: &Path, base: &str, head: &str) -> Value {
    let mut value: Value = serde_json::from_str(stdout_text).expect("the command prints JSON");
    let replacements = [
        (workspace.display().to_string(), "<workspace>".to_string()),
        (base.to_string(), "<base>".to_string()),
        (head.to_string(), "<head>".to_string()),
        (base[..7].to_string(), "<base>".to_string()),
        (head[..7].to_string(), "<head>".to_string()),
    ];
    replace_in_strings(&mut value, &replacements);
    value
}

/// The parsed JSON of the committed result `name` of the old-form workspace.
fn expected_result(name: &str) -> Value {
    let path = stats_support::fixture("old-form-expected").join(name);
    serde_json::from_str(&fs::read_to_string(&path).expect("read the expected result"))
        .expect("parse the expected result")
}

/// Case 10 and a pin: `stats` and `audit --workspace` read the deliverables of
/// a workspace as raw JSON, so a workspace of the old form (batch 35 and
/// earlier) gives the result that the committed files hold. Those files are the
/// output of the binary of batch 35 on this workspace, with the placeholders of
/// `normalized`.
#[test]
fn stats_and_audit_workspace_read_an_old_form_workspace_as_before() {
    let (root, base, head) = knowledge_repository();
    let workspace = tempfile::tempdir().expect("tempdir");
    copy_dir(&stats_support::fixture("old-form"), workspace.path());
    let shown = workspace.path().to_str().expect("a UTF-8 path");

    let stats = run(root.path(), &["stats", shown]);
    assert_eq!(stats.status.code(), Some(0), "{}", stderr(&stats));
    assert_eq!(stderr(&stats), "");
    assert_eq!(
        normalized(&stdout(&stats), workspace.path(), &base, &head),
        expected_result("stats.json")
    );

    let audit = run(
        root.path(),
        &[
            "audit",
            "--base",
            &base,
            "--head",
            &head,
            "--workspace",
            shown,
        ],
    );
    assert_eq!(audit.status.code(), Some(0), "{}", stderr(&audit));
    assert_eq!(stderr(&audit), format!("{AUDIT_LINE_ZERO}\n"));
    assert_eq!(
        normalized(&stdout(&audit), workspace.path(), &base, &head),
        expected_result("audit.json")
    );
}
