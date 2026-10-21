//! The deliverables schema (spec 3.9, 4.5, and 9.1): `model` is required on
//! every kind, the `spec-review` kind exists, a fix round may record
//! `gates_run`, and the template's `backlogId` default accepts any token
//! until a tracker names the shapes of its ids. Each case runs
//! `houserules validate` through the compiled binary against a scratch
//! install.

mod tracker_support;

use std::fs;
use std::path::Path;

use serde_json::{Value, json};
use tracker_support::{read_json, repo_root, run, run_ok, scratch_git_repo, stderr, stdout};

/// The default `backlogId` pattern of the template schema (spec 3.9).
const ANY_TOKEN_PATTERN: &str = "^[A-Za-z0-9#._-]+$";

/// The model id that the samples record.
const MODEL: &str = "claude-opus-5-5";

/// A report of status `BLOCKED`, which keeps `self_audit` null, so the
/// sample needs no audit rows.
fn task_report() -> Value {
    json!({
        "kind": "task-report", "model": MODEL, "task": 5, "backlog": ["HR-129"],
        "status": "BLOCKED", "implemented": "x",
        "commits": [], "tests": [], "live_run": [], "tdd": [],
        "files_changed": [], "docs_verified": [], "dependency_vetting": null,
        "coverage": [], "self_audit": null, "self_review": [], "concerns": [],
        "knowledge_used": [],
    })
}

fn task_review() -> Value {
    json!({
        "kind": "task-review", "model": MODEL, "task": 5,
        "base": "abc1234", "head": "abc1235",
        "spec_compliance": {"verdict": "compliant", "items": []},
        "rule_adherence": [], "strengths": ["Tests assert behavior."], "issues": [],
        "assessment": {"verdict": "approved", "text": "Ready."},
    })
}

fn re_review() -> Value {
    json!({
        "kind": "re-review", "model": MODEL, "task": 5, "round": 1,
        "fix_base": "abc1234", "head": "abc1235",
        "finding_verdicts": [], "rule_adherence": [], "new_breakage": [],
        "out_of_scope": [], "verdict": {"state": "all-addressed", "open": []},
    })
}

fn branch_review() -> Value {
    json!({
        "kind": "branch-review", "model": MODEL, "base": "abc1234", "head": "abc1235",
        "strengths": ["One review."], "issues": [], "rule_adherence": [],
        "recommendations": [],
        "retrospective": {
            "violated_rules": [], "uncovered_findings": [], "stale_entries": [],
            "unused_ids": [], "template_defects": [],
        },
        "assessment": {"ready": "yes", "text": "Ready."},
    })
}

/// A `spec-review` with the required fields of spec 4.5 only.
fn spec_review() -> Value {
    json!({
        "kind": "spec-review", "model": MODEL,
        "target": "docs/specs/2026-10-20-batch-34-workflow-and-work-items.md",
        "base": "abc1234", "head": "abc1235",
        "strengths": ["Every fact re-runs."],
        "findings": [{
            "id": "S1", "severity": "important", "section": "3.4",
            "what": "A line has no exit code.", "evidence": "probe.txt",
            "fix": "State the exit code.",
        }],
        "verdict": "needs-fixes",
    })
}

/// A `spec-review` that also holds every optional field of spec 4.5.
fn full_spec_review() -> Value {
    let mut review = spec_review();
    let fields = json!({
        "scope": "Round 1 of the design review.",
        "facts": [{"id": "F1", "verdict": "confirmed", "probe": "probe.txt"}],
        "controller_calls": [{"id": "C1", "verdict": "needs-owner", "why": "A product call."}],
        "coverage": [{"subject": "HR-129", "where": "9.1", "gap": "none"}],
        "questions_for_owner": [{"id": "Q1", "question": "Which one?", "recommendation": "The first."}],
        "prior": [{"id": "R1", "verdict": "partial", "where": "4.7"}],
    });
    review
        .as_object_mut()
        .expect("an object")
        .extend(fields.as_object().expect("an object").clone());
    review
}

/// An install made by a plain `init`: the deliverables schema carries the
/// template's default `backlogId` pattern.
fn plain_install() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    dir
}

/// Writes `value` to `name` in `dir` and runs `validate` on it. Returns the
/// exit code and the errors of the one result row.
fn validate(dir: &Path, name: &str, value: &Value) -> (i32, Vec<String>) {
    let file = dir.join(name);
    fs::write(
        &file,
        serde_json::to_string_pretty(value).expect("serialize"),
    )
    .expect("write");
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
        .map(|error| error.as_str().expect("an error is a string").to_string())
        .collect();
    (code, errors)
}

/// The kinds and the file name each one is saved under.
fn every_kind() -> Vec<(&'static str, Value)> {
    vec![
        ("task-1-report.json", task_report()),
        ("task-1-review.json", task_review()),
        ("task-1-review-r1.json", re_review()),
        ("branch-review.json", branch_review()),
        ("spec-review.json", spec_review()),
    ]
}

/// Spec 4.5: every kind validates with a `model`, the `spec-review` kind
/// included.
#[test]
fn every_kind_validates_with_a_model() {
    let dir = plain_install();
    for (name, value) in every_kind() {
        assert_eq!(validate(dir.path(), name, &value), (0, vec![]), "{name}");
    }
}

/// Spec 4.5 and case 17: `validate` requires `model` on every kind, and
/// names it.
#[test]
fn validate_requires_model_on_every_kind() {
    let dir = plain_install();
    for (name, mut value) in every_kind() {
        value.as_object_mut().expect("an object").remove("model");
        let (code, errors) = validate(dir.path(), name, &value);
        assert_eq!(code, 1, "{name}: {errors:?}");
        assert!(
            errors
                .iter()
                .any(|error| error.ends_with(": missing \"model\"")),
            "{name}: {errors:?}"
        );
    }
}

/// Spec 4.5: `model` is a model alias, a model id, `human`, or `unknown`; any
/// non-empty string passes and an empty one does not.
#[test]
fn model_takes_an_alias_an_id_human_or_unknown_and_refuses_an_empty_string() {
    let dir = plain_install();
    for model in ["sonnet", "claude-sonnet-5-5", "human", "unknown"] {
        let mut value = task_report();
        value["model"] = json!(model);
        assert_eq!(
            validate(dir.path(), "r.json", &value),
            (0, vec![]),
            "{model}"
        );
    }
    let mut value = task_report();
    value["model"] = json!("");
    let (code, errors) = validate(dir.path(), "r.json", &value);
    assert_eq!(code, 1, "{errors:?}");
    assert!(
        errors.iter().any(|error| error.contains(".model")),
        "{errors:?}"
    );
}

/// The description of `model` in both copies of the deliverables schema, the
/// template's and this repository's, says what a writer records: the value that
/// its task names, which is a model alias or a model id. The schema is a core
/// text, so the description names no workflow (`tests/pack_texts.rs`).
const MODEL_DESCRIPTION: &str = "The model that wrote the deliverable: the value that its task names (a model alias or a model id), `human`, or `unknown`.";

/// Both copies of the schema describe `model` the same way, so the agent who
/// reads either one records the value that its task names, alias or id, as it
/// reads.
#[test]
fn the_model_description_names_the_value_of_the_dispatch_in_both_copies() {
    for path in [
        "template/.claude/schemas/deliverables.json",
        ".claude/schemas/deliverables.json",
    ] {
        let schema = read_json(&repo_root(), path);
        assert_eq!(
            schema["$defs"]["model"]["description"],
            json!(MODEL_DESCRIPTION),
            "{path}"
        );
    }
}

/// Spec 4.5: a `spec-review` accepts `scope`, `facts`, `controller_calls`,
/// `coverage`, `questions_for_owner`, and `prior`.
#[test]
fn spec_review_accepts_every_optional_field() {
    let dir = plain_install();
    assert_eq!(
        validate(dir.path(), "spec-review.json", &full_spec_review()),
        (0, vec![])
    );
}

/// Spec 4.5: a finding's `severity` is the lowercase vocabulary of the schema
/// and of `stats`; `Critical` fails.
#[test]
fn spec_review_refuses_a_capitalized_severity() {
    let dir = plain_install();
    let mut value = spec_review();
    value["findings"][0]["severity"] = json!("Critical");
    let (code, errors) = validate(dir.path(), "spec-review.json", &value);
    assert_eq!(code, 1, "{errors:?}");
    assert!(
        errors.iter().any(|error| error.contains(".severity")),
        "{errors:?}"
    );
}

/// Spec 4.5: `verdict` is `approve` or `needs-fixes`, and every enumerated
/// field of the optional lists keeps its vocabulary.
#[test]
fn spec_review_refuses_a_verdict_outside_its_vocabulary() {
    let dir = plain_install();
    let cases = [
        ("verdict", json!("approved")),
        (
            "facts",
            json!([{"id": "F1", "verdict": "true", "probe": "p"}]),
        ),
        (
            "controller_calls",
            json!([{"id": "C1", "verdict": "fine", "why": "w"}]),
        ),
        (
            "prior",
            json!([{"id": "R1", "verdict": "done", "where": "w"}]),
        ),
    ];
    for (field, bad) in cases {
        let mut value = full_spec_review();
        value[field] = bad;
        let (code, errors) = validate(dir.path(), "spec-review.json", &value);
        assert_eq!(code, 1, "{field}: {errors:?}");
        assert!(
            errors.iter().any(|error| error.contains(field)),
            "{field}: {errors:?}"
        );
    }
}

/// Spec 4.5: `target`, `base`, `head`, `strengths`, `findings`, and `verdict`
/// are required, and no other field is allowed.
#[test]
fn spec_review_requires_its_fields_and_refuses_an_unknown_one() {
    let dir = plain_install();
    for field in ["target", "base", "head", "strengths", "findings", "verdict"] {
        let mut value = spec_review();
        value.as_object_mut().expect("an object").remove(field);
        let (code, errors) = validate(dir.path(), "spec-review.json", &value);
        assert_eq!(code, 1, "{field}: {errors:?}");
        assert!(
            errors
                .iter()
                .any(|error| error.ends_with(&format!(": missing \"{field}\""))),
            "{field}: {errors:?}"
        );
    }
    let mut value = spec_review();
    value["issues"] = json!([]);
    let (code, errors) = validate(dir.path(), "spec-review.json", &value);
    assert_eq!(code, 1, "{errors:?}");
    assert!(
        errors
            .iter()
            .any(|error| error.ends_with(": unknown field \"issues\"")),
        "{errors:?}"
    );
}

/// Spec 9.1, HR-129 (2): `fix_rounds[].gates_run` is optional, and a list of
/// the gates the round ran.
#[test]
fn a_fix_round_may_record_gates_run_as_a_list() {
    let dir = plain_install();
    let round = |gates_run: Value| {
        let mut value = task_report();
        value["fix_rounds"] = json!([{
            "round": 1, "findings": [], "commits": [], "tests": [], "gates_run": gates_run,
        }]);
        value
    };
    assert_eq!(
        validate(
            dir.path(),
            "r.json",
            &round(json!(["cargo fmt --check", "cargo test --locked"]))
        ),
        (0, vec![])
    );
    let (code, errors) = validate(dir.path(), "r.json", &round(json!("every gate")));
    assert_eq!(code, 1, "{errors:?}");
    assert!(
        errors.iter().any(|error| error.contains("gates_run")),
        "{errors:?}"
    );
    let mut without = task_report();
    without["fix_rounds"] = json!([{"round": 1, "findings": [], "commits": [], "tests": []}]);
    assert_eq!(validate(dir.path(), "r.json", &without), (0, vec![]));
}

/// Spec 3.9: with no tracker, `backlogId` accepts any token of
/// `^[A-Za-z0-9#._-]+$`, so a plain install holds that pattern and `validate`
/// accepts a GitHub id, a Beads id, and a JSON id alike.
#[test]
fn a_plain_install_accepts_any_token_as_a_backlog_id() {
    let dir = plain_install();
    let schema = read_json(dir.path(), ".claude/schemas/deliverables.json");
    assert_eq!(
        schema["$defs"]["backlogId"]["pattern"],
        json!(ANY_TOKEN_PATTERN)
    );
    let mut value = task_report();
    value["backlog"] = json!(["#12", "proj-4.2", "WI-001", "A_1"]);
    assert_eq!(validate(dir.path(), "r.json", &value), (0, vec![]));
    for bad in ["has space", "", "a/b", "#12!"] {
        let mut value = task_report();
        value["backlog"] = json!([bad]);
        let (code, errors) = validate(dir.path(), "r.json", &value);
        assert_eq!(code, 1, "{bad:?}: {errors:?}");
    }
}
