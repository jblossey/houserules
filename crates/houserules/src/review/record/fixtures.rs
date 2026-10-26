//! Change records for the unit tests of the record modules. A record is built
//! as JSON and read through `ChangeRecord`, so a test depends on the shape of
//! the document of spec 8.2 and not on the fields of the Rust type.

use serde_json::{Value, json};

use crate::published::change_record::ChangeRecord;

/// The key of every fixture record: the last commit of its range.
pub(super) const KEY: &str = "4444444444444444444444444444444444444444";

/// The base of every fixture record.
pub(super) const BASE: &str = "0000000000000000000000000000000000000000";

/// The four commits of the range of a fixture record, oldest first.
pub(super) const COMMITS: [&str; 4] = [
    "1111111111111111111111111111111111111111",
    "2222222222222222222222222222222222222222",
    "3333333333333333333333333333333333333333",
    KEY,
];

/// The hash of the rule sources at the merge base.
pub(super) const MERGE_BASE_HASH: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// The hash of the rule sources at the head.
pub(super) const HEAD_HASH: &str =
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// `value` read as a change record.
pub(super) fn parsed(value: &Value) -> ChangeRecord {
    serde_json::from_value(value.clone())
        .unwrap_or_else(|error| panic!("not a change record: {error}\n{value:#}"))
}

/// `record` as JSON.
pub(super) fn json_of(record: &ChangeRecord) -> Value {
    serde_json::to_value(record).expect("a change record serializes")
}

/// The record in its full form (spec 8.2) with `paths` uncovered paths. Each
/// path has two ranges: the lines 1 to 3, and 2 lines deleted after line 9.
/// The area `code` holds the same paths, the route holds one reason that lists
/// them, and every list that a cut shortens has at least one item.
pub(super) fn full(paths: usize) -> Value {
    let names: Vec<String> = (0..paths).map(|n| format!("src/file{n:04}.rs")).collect();
    let uncovered: Vec<Value> = names
        .iter()
        .map(|name| {
            json!({
                "path": name,
                "old_path": null,
                "kind": "text",
                "ranges": [{"start": 1, "end": 3}, {"after": 9, "deleted": 2}],
            })
        })
        .collect();
    let listed: Vec<&String> = names.iter().take(20).collect();
    json!({
        "schema": "houserules/change-record@1.0.0",
        "install": ".",
        "key": KEY,
        "base": BASE,
        "head": KEY,
        "merge_base": BASE,
        "commits": COMMITS,
        "written_by": "houserules 2.0.0",
        "host": "github",
        "rules": {
            "merge_base_hash": MERGE_BASE_HASH,
            "merge_base_rules": "present",
            "head_hash": HEAD_HASH,
            "audited_with": "head",
            "packs": [{"name": "process", "version": "1.0.0", "sha256": HEAD_HASH}],
        },
        "audit": {
            "summary": {
                "base": "0000000",
                "head": "4444444",
                "deterministic": 3,
                "pass": 1,
                "fail": 1,
                "warn": 1,
                "skipped": 0,
                "judged": 1,
                "reference_fail": 0,
            },
            "rules": [
                {"id": "team.judged", "mode": "judged", "result": "open"},
                {"id": "team.lint", "mode": "deterministic", "result": "fail"},
                {"id": "team.note", "mode": "deterministic", "result": "warn"},
                {"id": "team.ok", "mode": "deterministic", "result": "pass"},
            ],
            "references": {"checked": 0, "passed": 0, "skipped": 2, "failures": []},
            "error": null,
        },
        "intent": [{
            "source": "work-item",
            "ref": "WI-1",
            "title": "A title",
            "why": "Because.",
            "tasks": {"done": 1, "total": 2},
            "requirements": null,
            "validation": null,
            "artifacts": ["docs/plans/a.md", "docs/plans/b.md"],
        }],
        "evidence": {
            "schema": "houserules/evidence@1.0.0",
            "install": ".",
            "base": BASE,
            "head": KEY,
            "merge_base": BASE,
            "diff": {"files": paths, "added": paths * 3, "deleted": 0, "lines": paths * 3, "excluded": 0},
            "uncovered": {
                "lines": paths * 5,
                "total_lines": paths * 5,
                "share": if paths == 0 { 0.0 } else { 1.0 },
                "paths": uncovered,
            },
            "failed": [{"id": "team.lint", "evidence": "a lint finding"}],
            "warned": [{"id": "team.note", "evidence": "a note"}],
            "proven": [{"id": "team.ok", "evidence": "3 files checked"}],
            "judged": [{"id": "team.judged", "verdict": "pass", "evidence": "read"}],
            "skipped": [{"id": "team.skipped", "evidence": "no file matched"}],
            "work_items": ["WI-1", "WI-2"],
        },
        "route": {
            "schema": "houserules/route@1.0.0",
            "install": ".",
            "head": KEY,
            "route": "owner-review",
            "floor": false,
            "inputs": {
                "failed": ["team.lint"],
                "warned": ["team.note"],
                "areas": {"code": names},
                "no_area": ["README.md"],
                "paths": [{"glob": "src/**", "route": "owner-review", "paths": names}],
                "diff_lines": paths * 3,
                "uncovered_share": 1.0,
                "score": null,
                "rule_sources_changed": [".houserules.json"],
            },
            "reasons": [{
                "rule": "area",
                "input": "code",
                "value": paths,
                "threshold": null,
                "route": "owner-review",
                "paths": listed,
                "more_paths": paths.saturating_sub(20),
                "decisive": true,
            }],
        },
        "sample": null,
        "bundle": {
            "purpose": "Edit the guide.",
            "intent": "file:docs/plans/a.md",
            "work_items": ["WI-1", "WI-2"],
            "tier": "standard",
            "criterion": "a docs change",
            "budget_reason": "",
            "runs": [{
                "argv": ["mise", "run", "test"],
                "exit": 0,
                "head": KEY,
                "sha256": HEAD_HASH,
            }],
        },
        "reviewers": ["amy", "bob"],
        "reviewers_pending": false,
        "links": {"work_items": ["WI-1", "WI-2"]},
        "post_merge_audit": null,
        "truncated": [],
    })
}

/// The smallest record that `record write` can write for `key` (spec 8.4,
/// step 8): the minimal form of a range of the one commit `key`.
pub(super) fn minimal(key: &str) -> Value {
    json!({
        "schema": "houserules/change-record@1.0.0",
        "install": ".",
        "key": key,
        "base": BASE,
        "head": key,
        "merge_base": BASE,
        "written_by": "houserules 2.0.0",
        "host": null,
        "rules": {
            "merge_base_hash": MERGE_BASE_HASH,
            "merge_base_rules": "present",
            "head_hash": HEAD_HASH,
            "audited_with": "head",
            "packs": [],
        },
        "audit": {"summary": {
            "base": "0000000",
            "head": "4444444",
            "deterministic": 0,
            "pass": 0,
            "fail": 0,
            "warn": 0,
            "skipped": 0,
            "judged": 0,
            "reference_fail": 0,
        }},
        "intent": [],
        "route": {"route": "owner-review", "floor": false},
        "sample": null,
        "reviewers": [],
        "reviewers_pending": false,
        "post_merge_audit": null,
        "truncated": ["all"],
    })
}

/// A record with the commits `commits`, keyed by the last of them, in the
/// minimal form: enough for a test of the notes, which read `key` and
/// `commits` only.
pub(super) fn listing(commits: &[&str]) -> Value {
    let mut value = minimal(commits.last().expect("a commit"));
    value["commits"] = json!(commits);
    value
}
