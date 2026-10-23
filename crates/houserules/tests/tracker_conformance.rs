//! The conformance suite of the tracker contract (batch 34, spec sections
//! 3.2, 3.4, 3.5, 3.6, and 3.8): one suite that runs against the JSON adapter,
//! the GitHub adapter, and the Beads adapter. The GitHub adapter runs against
//! `fake-gh`, a test executable that serves the `gh` JSON shapes from the
//! scratch store that `FAKE_GH_STORE` names; `HOUSERULES_GH` points the binary
//! at it. CI never writes to a GitHub repository. The Beads adapter runs
//! against the real `bd` of the release that `mise.toml` pins, in a scratch
//! store of each scenario; `HOUSERULES_BD` names another program. A Beads test
//! fails, never skips, when `bd` is missing.
//!
//! A conformance case is a function of the adapter under test. It builds two
//! scratch installs from one seed, one with the adapter under test and one
//! with the JSON adapter, runs the same command on both, and requires the same
//! output (`assert_equivalent`, spec 3.2). Each case is declared once, in
//! `conformance!`, and runs three times. Its `github` and `beads` tests compare
//! the adapter with the JSON adapter. Its `json` test builds both installs with
//! the JSON adapter, so its comparisons hold the JSON adapter against itself:
//! they fail on a non-zero exit or on output that is no JSON, and a case fails
//! its `json` test beyond that only through a literal that it asserts. The
//! outputs of the JSON adapter are pinned by `tests/backlog_parity.rs` and
//! `tests/tracker_json.rs`. A case that names the expected output of each
//! adapter (`row_archive`, the `check_` cases that assert a fault,
//! `get_names_an_unknown_id`, and `batch_names_an_unknown_batch`) compares no
//! adapter with another. The tests whose names start with `github_`, `beads_`,
//! `json_`, and `block_` hold what only one adapter does.
//!
//! The store of the fake `gh` is one JSON object: `repo` (the one repository
//! it serves), `auth` (`false` makes every call exit 4), `failure` (a
//! `{code, stderr}` object that makes every call fail), `issues` (the REST
//! shape: `number`, `title`, `body`, `state`, `state_reason`, `labels`,
//! `milestone`, and `pull_request` on a pull request), `labels` (names),
//! `milestones` (`number`, `title`), and `calls` (the argument list of every
//! call, in order). The same executable stands in for `bd` where a test needs
//! a program that records its calls or fails every call: it serves no `bd`
//! command, and only `calls` and `failure` of its store matter then.
//!
//! A Beads scenario seeds its store through `bd import`: one JSONL row for
//! each record, with the explicit id `hr-<n>` of the record's place in the seed,
//! so that the ids are known before the adapter reads them. The rows are the
//! Beads column of the mapping table of spec 3.5.

mod tracker_support;
mod tree_snapshot;

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Map, Value, json};
use tracker_support::{
    commit_all, git, hermetic_bd, houserules, read_json, repo_root, run_ok, scratch_git_repo,
    stderr, stdout, write_json,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The repository that the GitHub scenarios track and that the fake `gh`
/// serves.
const REPO: &str = "acme/widgets";

/// The labels of spec 3.5 that the adapter check requires for each record
/// kind.
const RECORD_LABELS: [&str; 6] = [
    "houserules:section",
    "houserules:batch",
    "houserules:amendment",
    "houserules:decision",
    "houserules:parked",
    "houserules:deferred",
];

/// The path of the fake `gh` that cargo builds for this test target.
fn fake_gh() -> &'static str {
    env!("CARGO_BIN_EXE_fake-gh")
}

/// The issue prefix of the Beads stores that the scenarios create.
const BEADS_PREFIX: &str = "hr";

/// The custom status of spec 3.5 in the form of `bd config set status.custom`:
/// `partial`, in the category that `bd ready` lists.
const BEADS_STATUS_CONFIG: &str = "partial:active";

/// The custom types of the records of spec 3.5 in the form of `bd config set
/// types.custom`.
const BEADS_TYPE_CONFIG: &str = "houserules-section,houserules-batch,houserules-amendment,houserules-decision,houserules-parked";

/// The adapter under test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Adapter {
    Json,
    Github,
    Beads,
}

/// The seed of one scenario, in the JSON backlog form, and the ids that the
/// hosted adapters give the records.
struct Fixture {
    /// The records, in seed order. Each carries its `kind`: `section`,
    /// `item`, `batch`, `amendment`, `parked`, `group` (the group of a batch
    /// of parked items, with its intro), or `decision`.
    seed: Vec<Value>,
    /// Each seed id and the GitHub id of its record: the GitHub adapter
    /// numbers the records one by one, from `#1`, in seed order.
    id_map: BTreeMap<String, String>,
}

impl Fixture {
    /// The fixture of `seed`.
    fn new(seed: Vec<Value>) -> Self {
        let id_map = seed
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                record
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| (id.to_string(), format!("#{}", index + 1)))
            })
            .collect();
        Self { seed, id_map }
    }

    /// The id of the record at `index` of the seed in `adapter`: the JSON
    /// adapter keeps the seed id, GitHub numbers the records from `#1`, and
    /// the Beads store holds them as `hr-1`, `hr-2`, and so on.
    fn id_at(&self, adapter: Adapter, index: usize) -> String {
        match adapter {
            Adapter::Json => self.seed[index]["id"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            Adapter::Github => format!("#{}", index + 1),
            Adapter::Beads => format!("{BEADS_PREFIX}-{}", index + 1),
        }
    }

    /// Each seed id and the id that `adapter` gives its record.
    fn ids(&self, adapter: Adapter) -> BTreeMap<String, String> {
        self.seed
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                record
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| (id.to_string(), self.id_at(adapter, index)))
            })
            .collect()
    }

    /// The id in `adapter` of the first record of `kind`, for a record that
    /// has no seed id (a parked group).
    fn id_of_first(&self, adapter: Adapter, kind: &str) -> String {
        let index = self
            .seed
            .iter()
            .position(|record| record["kind"] == json!(kind))
            .unwrap_or_else(|| panic!("the seed has no {kind} record"));
        self.id_at(adapter, index)
    }
}

/// The seed that most cases share: two sections, four items that cover every
/// status, two batches, an amendment, two parked items, and a decision.
fn seed() -> Vec<Value> {
    vec![
        json!({"kind": "section", "section": "E01", "heading": "Epic one", "title": "Epic one items", "spec": ""}),
        json!({
            "kind": "item", "id": "WI-001", "section": "E01", "type": "feat",
            "milestone": "v1", "label": "Core", "status": "partial",
            "title": "Build the core",
            "body": ["First paragraph.", "Second paragraph."],
            "extra_lines": ["An extra line."],
            "rulings": [{"date": "2026-10-01", "by": "user", "text": "Proceed."}],
            "batch": 1, "tags": ["alpha", "beta"], "see": ["WI-002", "A-01"],
        }),
        json!({
            "kind": "item", "id": "WI-002", "section": "E01", "type": "bug",
            "milestone": null, "status": "done", "title": "Fix the crash",
            "body": ["Fixed."], "batch": 1,
        }),
        json!({
            "kind": "item", "id": "WI-003", "section": "E01", "type": "chore",
            "milestone": "v1", "status": "dropped", "title": "Drop the cache",
            "body": ["Not needed."], "batch": null,
        }),
        json!({"kind": "section", "section": "E02", "heading": "Epic two", "title": "Epic two items", "spec": null}),
        json!({
            "kind": "item", "id": "WI-004", "section": "E02", "type": "decision",
            "milestone": null, "status": "open", "title": "Pick the store",
            "body": ["Waits for the design."], "batch": null,
            "deferral": {"reason": "Needs a design.", "origin": "batch 1", "files": ["src/a.rs", "src/b.rs"]},
        }),
        json!({
            "kind": "batch", "number": 1, "items": ["WI-001", "WI-002"],
            "summary": "The first batch", "kickoff": "Kick off the core.",
            "status": {"state": "in-progress", "text": "Under way."},
        }),
        json!({
            "kind": "batch", "number": 2, "items": [],
            "summary": "The second batch", "kickoff": "",
            "status": {"state": "planned", "text": ""},
        }),
        json!({
            "kind": "amendment", "id": "A-01", "type": "constraint", "label": "Scope",
            "date": "2026-10-02", "status": "open", "text": ["Stay in scope."],
        }),
        json!({"kind": "parked", "id": "PP-1-01", "batch": 1, "text": "Polish the help text."}),
        json!({
            "kind": "parked", "id": "PP-1-02", "batch": 1, "text": "Rename the flag.",
            "resolved": {"batch": 2, "note": "Renamed in batch 2."},
        }),
        json!({"kind": "decision", "date": "2026-10-03", "text": "Keep the labels."}),
    ]
}

/// `seed()` with `edit` applied to the record whose `id` or `number` is
/// `key`.
fn seed_with(key: &str, edit: impl FnOnce(&mut Map<String, Value>)) -> Vec<Value> {
    let mut seed = seed();
    let record = seed
        .iter_mut()
        .find(|record| record["id"] == json!(key) || record["number"].to_string().as_str() == key)
        .unwrap_or_else(|| panic!("the seed has no record {key}"));
    edit(record.as_object_mut().expect("a record is an object"));
    seed
}

/// The intro of the parked group of batch 1 in `grouped_seed()`.
const GROUP_INTRO: &str = "Polish for the first batch.";

/// `seed()` with the parked group of batch 1 (spec 3.5, C30): a group with an
/// intro and the two parked items that the seed holds for that batch.
fn grouped_seed() -> Vec<Value> {
    let mut seed = seed();
    let first_parked = seed
        .iter()
        .position(|record| record["id"] == json!("PP-1-01"))
        .expect("the seed has PP-1-01");
    seed.insert(
        first_parked,
        json!({"kind": "group", "batch": 1, "intro": GROUP_INTRO}),
    );
    seed
}

/// A plain value of a block line, as the spec writes it: a string as it is, any
/// other value as JSON.
fn block_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).expect("a value serializes"),
    }
}

/// An issue body: `prose`, a blank line, and the houserules block of `fields`
/// (spec 3.5). With no field, the body is the prose alone.
fn body_with_block(prose: &str, fields: &[(&str, Value)]) -> String {
    if fields.is_empty() {
        return prose.to_string();
    }
    let lines: Vec<String> = fields
        .iter()
        .map(|(key, value)| format!("{key}: {}", block_value(value)))
        .collect();
    let fence = format!("```houserules\n{}\n```", lines.join("\n"));
    if prose.is_empty() {
        fence
    } else {
        format!("{prose}\n\n{fence}")
    }
}

/// The REST issue of `record` as the mapping table of spec 3.5 writes it, with
/// the number `number`. `to_github` maps a seed id to its GitHub id.
fn github_issue(record: &Value, number: usize, to_github: &dyn Fn(&Value) -> Value) -> Value {
    let text = |key: &str| record[key].as_str().unwrap_or_default().to_string();
    let mut labels: Vec<String> = Vec::new();
    let mut fields: Vec<(&str, Value)> = Vec::new();
    let (title, prose, state, reason, milestone);
    match record["kind"].as_str().expect("a kind") {
        "item" => {
            title = text("title");
            prose = record["body"]
                .as_array()
                .map(|lines| {
                    lines
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n\n")
                })
                .unwrap_or_default();
            labels.push(format!("type:{}", text("type")));
            labels.push(format!("section:{}", text("section")));
            let status = text("status");
            if status == "partial" {
                labels.push("status:partial".to_string());
            }
            if let Some(batch) = record["batch"].as_i64() {
                labels.push(format!("batch:{batch}"));
            }
            for tag in record["tags"].as_array().into_iter().flatten() {
                labels.push(format!("tag:{}", tag.as_str().expect("a tag")));
            }
            if record.get("deferral").is_some() {
                labels.push("houserules:deferred".to_string());
                fields.push(("deferral", record["deferral"].clone()));
            }
            for key in ["label", "extra_lines", "rulings"] {
                if let Some(value) = record.get(key) {
                    fields.push((key, value.clone()));
                }
            }
            if let Some(see) = record.get("see") {
                fields.push(("see", to_github(see)));
            }
            (state, reason) = match status.as_str() {
                "done" => ("closed", json!("completed")),
                "dropped" => ("closed", json!("not_planned")),
                _ => ("open", Value::Null),
            };
            milestone = record["milestone"].as_str().map(str::to_string);
        }
        "section" => {
            title = text("title");
            prose = String::new();
            labels.push("houserules:section".to_string());
            labels.push(format!("section:{}", text("section")));
            fields.push(("heading", record["heading"].clone()));
            fields.push(("spec", record["spec"].clone()));
            (state, reason, milestone) = ("open", Value::Null, None);
        }
        "batch" => {
            title = format!("Batch {}: {}", record["number"], text("summary"));
            prose = String::new();
            labels.push("houserules:batch".to_string());
            fields.push(("kickoff", record["kickoff"].clone()));
            fields.push(("status", record["status"].clone()));
            (state, reason) = if record["status"]["state"] == json!("done") {
                ("closed", json!("completed"))
            } else {
                ("open", Value::Null)
            };
            milestone = None;
        }
        "amendment" => {
            let lines: Vec<&str> = record["text"]
                .as_array()
                .expect("amendment text")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            title = record["label"].as_str().unwrap_or(lines[0]).to_string();
            prose = lines.join("\n\n");
            labels.push("houserules:amendment".to_string());
            for key in ["type", "label", "date"] {
                if let Some(value) = record.get(key) {
                    fields.push((key, value.clone()));
                }
            }
            (state, reason) = if record["status"] == json!("done") {
                ("closed", json!("completed"))
            } else {
                ("open", Value::Null)
            };
            milestone = None;
        }
        "parked" => {
            title = text("text");
            prose = text("text");
            labels.push("houserules:parked".to_string());
            labels.push(format!("batch:{}", record["batch"]));
            if let Some(resolved) = record.get("resolved") {
                fields.push(("resolved", resolved.clone()));
            }
            (state, reason) = if record.get("resolved").is_some() {
                ("closed", json!("completed"))
            } else {
                ("open", Value::Null)
            };
            milestone = None;
        }
        "group" => {
            title = text("intro");
            prose = text("intro");
            labels.push("houserules:parked".to_string());
            labels.push(format!("batch:{}", record["batch"]));
            fields.push(("group", json!(true)));
            (state, reason, milestone) = ("open", Value::Null, None);
        }
        "decision" => {
            title = text("text");
            prose = text("text");
            labels.push("houserules:decision".to_string());
            fields.push(("date", record["date"].clone()));
            (state, reason, milestone) = ("open", Value::Null, None);
        }
        other => panic!("unknown seed kind {other}"),
    }
    json!({
        "number": number,
        "title": title,
        "body": body_with_block(&prose, &fields),
        "state": state,
        "state_reason": reason,
        "labels": labels.iter().map(|name| json!({"name": name})).collect::<Vec<_>>(),
        "milestone": milestone.map(|title| json!({"title": title, "number": 1})),
    })
}

/// The store of the fake `gh` for `fixture`: its issues, and every label and
/// milestone that they use, with the record labels the adapter check needs.
fn github_store(fixture: &Fixture) -> Value {
    let to_github = |see: &Value| -> Value {
        Value::Array(
            see.as_array()
                .expect("a list of ids")
                .iter()
                .map(|id| {
                    let id = id.as_str().expect("an id");
                    json!(fixture.id_map.get(id).map_or(id, String::as_str))
                })
                .collect(),
        )
    };
    let issues: Vec<Value> = fixture
        .seed
        .iter()
        .enumerate()
        .map(|(index, record)| github_issue(record, index + 1, &to_github))
        .collect();
    let mut labels: Vec<String> = RECORD_LABELS
        .iter()
        .map(|label| label.to_string())
        .collect();
    let mut milestones: Vec<String> = Vec::new();
    for issue in &issues {
        for label in issue["labels"].as_array().into_iter().flatten() {
            let name = label["name"].as_str().expect("a label name").to_string();
            if !labels.contains(&name) {
                labels.push(name);
            }
        }
        if let Some(title) = issue["milestone"]["title"].as_str()
            && !milestones.iter().any(|known| known == title)
        {
            milestones.push(title.to_string());
        }
    }
    json!({
        "repo": REPO,
        "auth": true,
        "failure": null,
        "issues": issues,
        "labels": labels,
        "milestones": milestones
            .iter()
            .enumerate()
            .map(|(index, title)| json!({"number": index + 1, "title": title}))
            .collect::<Vec<_>>(),
        "calls": [],
    })
}

/// The native Beads type of an item of the type `item_type` (spec 3.5): feat
/// is a feature, bug and fix are bugs, chore and decision keep their names,
/// and any other type is a task.
fn beads_native_type(item_type: &str) -> &'static str {
    match item_type {
        "feat" => "feature",
        "bug" | "fix" => "bug",
        "chore" => "chore",
        "decision" => "decision",
        _ => "task",
    }
}

/// The reference that stands for a `see` entry of the seed which names no
/// record: a Beads dependency cannot name an issue that the store lacks, but it
/// can name an external reference.
fn beads_external_reference(id: &str) -> String {
    format!("external:seed:{id}")
}

/// The row of `bd import` for `record`, which the store holds under `id`, as
/// the Beads column of the mapping table of spec 3.5 writes it. `ids` maps a
/// seed id to the Beads id of its record.
fn beads_issue(record: &Value, id: &str, ids: &BTreeMap<String, String>) -> Value {
    let text = |key: &str| record[key].as_str().unwrap_or_default().to_string();
    let mut labels: Vec<String> = Vec::new();
    let mut fields: Vec<(&str, Value)> = Vec::new();
    let mut dependencies: Vec<Value> = Vec::new();
    let (title, prose, issue_type, closed, reason);
    match record["kind"].as_str().expect("a kind") {
        "item" => {
            title = text("title");
            prose = record["body"]
                .as_array()
                .map(|lines| {
                    lines
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("\n\n")
                })
                .unwrap_or_default();
            issue_type = beads_native_type(&text("type")).to_string();
            labels.push(format!("type:{}", text("type")));
            labels.push(format!("section:{}", text("section")));
            if let Some(milestone) = record["milestone"].as_str() {
                labels.push(format!("milestone:{milestone}"));
            }
            if let Some(batch) = record["batch"].as_i64() {
                labels.push(format!("batch:{batch}"));
            }
            for tag in record["tags"].as_array().into_iter().flatten() {
                labels.push(format!("tag:{}", tag.as_str().expect("a tag")));
            }
            if record.get("deferral").is_some() {
                labels.push("houserules:deferred".to_string());
                fields.push(("deferral", record["deferral"].clone()));
            }
            for key in ["label", "extra_lines", "rulings"] {
                if let Some(value) = record.get(key) {
                    fields.push((key, value.clone()));
                }
            }
            for see in record["see"].as_array().into_iter().flatten() {
                let see = see.as_str().expect("an id");
                dependencies.push(json!({
                    "issue_id": id,
                    "depends_on_id": ids.get(see).cloned().unwrap_or_else(|| beads_external_reference(see)),
                    "type": "related",
                }));
            }
            (closed, reason) = match text("status").as_str() {
                "done" => (true, Some("completed")),
                "dropped" => (true, Some("dropped")),
                _ => (false, None),
            };
        }
        "section" => {
            title = text("title");
            prose = String::new();
            issue_type = "houserules-section".to_string();
            labels.push(format!("section:{}", text("section")));
            fields.push(("heading", record["heading"].clone()));
            fields.push(("spec", record["spec"].clone()));
            (closed, reason) = (false, None);
        }
        "batch" => {
            title = format!("Batch {}: {}", record["number"], text("summary"));
            prose = String::new();
            issue_type = "houserules-batch".to_string();
            fields.push(("kickoff", record["kickoff"].clone()));
            fields.push(("status", record["status"].clone()));
            (closed, reason) = (
                record["status"]["state"] == json!("done"),
                Some("completed"),
            );
        }
        "amendment" => {
            let lines: Vec<&str> = record["text"]
                .as_array()
                .expect("amendment text")
                .iter()
                .filter_map(Value::as_str)
                .collect();
            title = record["label"].as_str().unwrap_or(lines[0]).to_string();
            prose = lines.join("\n\n");
            issue_type = "houserules-amendment".to_string();
            for key in ["type", "label", "date"] {
                if let Some(value) = record.get(key) {
                    fields.push((key, value.clone()));
                }
            }
            (closed, reason) = (record["status"] == json!("done"), Some("completed"));
        }
        "parked" => {
            title = text("text");
            prose = text("text");
            issue_type = "houserules-parked".to_string();
            labels.push(format!("batch:{}", record["batch"]));
            if let Some(resolved) = record.get("resolved") {
                fields.push(("resolved", resolved.clone()));
            }
            (closed, reason) = (record.get("resolved").is_some(), Some("completed"));
        }
        "group" => {
            title = text("intro");
            prose = text("intro");
            issue_type = "houserules-parked".to_string();
            labels.push(format!("batch:{}", record["batch"]));
            fields.push(("group", json!(true)));
            (closed, reason) = (false, None);
        }
        "decision" => {
            title = text("text");
            prose = text("text");
            issue_type = "houserules-decision".to_string();
            fields.push(("date", record["date"].clone()));
            (closed, reason) = (false, None);
        }
        other => panic!("unknown seed kind {other}"),
    }
    let status = if closed {
        "closed"
    } else if record["status"] == json!("partial") {
        "partial"
    } else {
        "open"
    };
    let mut row = json!({
        "id": id,
        "title": title,
        "description": body_with_block(&prose, &fields),
        "issue_type": issue_type,
        "status": status,
        "labels": labels,
        "dependencies": dependencies,
    });
    if closed {
        row["close_reason"] = json!(reason);
        row["closed_at"] = json!("2026-10-20T10:00:00Z");
    }
    row
}

/// The rows of `bd import` for `fixture`, in seed order, each under the Beads
/// id of its place in the seed.
fn beads_rows(fixture: &Fixture) -> Vec<Value> {
    let ids = fixture.ids(Adapter::Beads);
    fixture
        .seed
        .iter()
        .enumerate()
        .map(|(index, record)| beads_issue(record, &fixture.id_at(Adapter::Beads, index), &ids))
        .collect()
}

/// The program that runs as `bd`: the one that `HOUSERULES_BD` names, else `bd`
/// from `PATH`, where `mise install` puts the release that `mise.toml` pins.
fn bd_program() -> OsString {
    std::env::var_os("HOUSERULES_BD")
        .filter(|program| !program.is_empty())
        .unwrap_or_else(|| OsString::from("bd"))
}

/// Runs `program` as `bd` with `args` in `dir`, with the environment of
/// `hermetic_bd`, and requires exit 0. It panics when the program is missing:
/// the Beads tests fail, never skip, without `bd` (spec 3.6).
fn run_bd_with(program: &OsStr, dir: &Path, home: &Path, args: &[&str]) -> Output {
    let mut command = Command::new(program);
    command.args(args).current_dir(dir);
    hermetic_bd(&mut command, home);
    let output = command.output().unwrap_or_else(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            panic!(
                "bd is not installed: run mise install, which pins it in mise.toml ({})",
                program.to_string_lossy()
            );
        }
        panic!("run bd {args:?}: {error}");
    });
    assert!(
        output.status.success(),
        "bd {args:?} failed: {}",
        stderr(&output)
    );
    output
}

/// Runs `bd` with `args` in `dir`; see `run_bd_with`.
fn run_bd(dir: &Path, home: &Path, args: &[&str]) -> Output {
    run_bd_with(&bd_program(), dir, home, args)
}

/// Creates the Beads store of a scenario in the git repository `dir`: `bd
/// init` with the prefix `hr` and no agent file, the custom status and the
/// custom types of spec 3.5, and `rows` imported. `home` is the Dolt and
/// configuration home of every `bd` run of the scenario.
fn create_beads_store(dir: &Path, home: &Path, rows: &[Value]) {
    run_bd(
        dir,
        home,
        &[
            "init",
            "--non-interactive",
            "--prefix",
            BEADS_PREFIX,
            "--quiet",
            "--skip-agents",
            "--skip-hooks",
        ],
    );
    run_bd(
        dir,
        home,
        &["config", "set", "status.custom", BEADS_STATUS_CONFIG],
    );
    run_bd(
        dir,
        home,
        &["config", "set", "types.custom", BEADS_TYPE_CONFIG],
    );
    import_rows(dir, home, rows);
}

/// Imports `rows` into the Beads store of `dir` with `bd import`. Rows that
/// name an id the store holds update that issue.
fn import_rows(dir: &Path, home: &Path, rows: &[Value]) {
    if rows.is_empty() {
        return;
    }
    let lines: Vec<String> = rows.iter().map(Value::to_string).collect();
    let file = home.join("rows.jsonl");
    fs::write(&file, format!("{}\n", lines.join("\n")))
        .unwrap_or_else(|error| panic!("write {}: {error}", file.display()));
    let file = file.to_str().expect("a UTF-8 path");
    run_bd(dir, home, &["import", file]);
}

/// Writes the JSON backlog of `fixture` into the install at `dir`, in place
/// of the example item that `init --tracker json` seeds.
fn write_json_backlog(dir: &Path, fixture: &Fixture) {
    fs::remove_file(dir.join("backlog/items/general.json")).expect("remove the example items");
    let of_kind = |kind: &str| -> Vec<&Value> {
        fixture
            .seed
            .iter()
            .filter(|record| record["kind"] == json!(kind))
            .collect()
    };
    let without = |record: &Value, keys: &[&str]| -> Value {
        let mut map = record.as_object().expect("a record").clone();
        for key in keys {
            map.remove(*key);
        }
        Value::Object(map)
    };
    let mut sections: Vec<(String, Value)> = Vec::new();
    for section in of_kind("section") {
        let name = section["section"]
            .as_str()
            .expect("a section name")
            .to_string();
        let items: Vec<Value> = of_kind("item")
            .into_iter()
            .filter(|item| item["section"] == section["section"])
            .map(|item| without(item, &["kind", "section"]))
            .collect();
        sections.push((
            name,
            json!({
                "$schema": "../schema.json",
                "section": section["section"],
                "heading": section["heading"],
                "title": section["title"],
                "spec": section["spec"],
                "items": items,
            }),
        ));
    }
    for (name, content) in &sections {
        write_json(dir, &format!("backlog/items/{name}.json"), content);
    }
    write_json(
        dir,
        "backlog/batches.json",
        &json!({
            "heading": "Batch planning", "intro": [], "table_header": [],
            "batches": of_kind("batch").into_iter().map(|batch| without(batch, &["kind"])).collect::<Vec<_>>(),
        }),
    );
    write_json(
        dir,
        "backlog/amendments.json",
        &json!({
            "heading": "Amendments",
            "amendments": of_kind("amendment").into_iter().map(|amendment| without(amendment, &["kind"])).collect::<Vec<_>>(),
        }),
    );
    let intro_of = |batch: &Value| -> Value {
        of_kind("group")
            .into_iter()
            .find(|group| group["batch"] == *batch)
            .map_or_else(
                || json!(format!("Parked in batch {batch}.")),
                |group| group["intro"].clone(),
            )
    };
    let mut groups: Vec<Value> = Vec::new();
    for parked in of_kind("parked") {
        let batch = parked["batch"].clone();
        let item = without(parked, &["kind", "batch"]);
        match groups.iter_mut().find(|group| group["batch"] == batch) {
            Some(group) => group["items"].as_array_mut().expect("items").push(item),
            None => groups.push(json!({
                "batch": batch,
                "intro": intro_of(&batch),
                "items": [item],
            })),
        }
    }
    write_json(dir, "backlog/parked.json", &json!({"groups": groups}));
    write_json(
        dir,
        "backlog/decisions.json",
        &json!({
            "preamble": "",
            "decisions": of_kind("decision").into_iter().map(|decision| without(decision, &["kind"])).collect::<Vec<_>>(),
            "notes": [],
        }),
    );
}

/// Writes `store` as the store of the fake `gh` at `path`.
fn write_store(path: &Path, store: &Value) {
    let text = serde_json::to_string_pretty(store).expect("serialize the store");
    fs::write(path, text).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

/// One scratch install that has the adapter under test and holds the seed of a
/// fixture. The scratch directory is removed when the scenario drops.
struct Scenario {
    adapter: Adapter,
    dir: tempfile::TempDir,
    /// The store of the fake `gh` (GitHub scenarios), or of the program that
    /// stands in for `bd` (`recorded`).
    store: PathBuf,
    /// The Dolt and configuration home of every `bd` run (Beads scenarios).
    home: PathBuf,
    /// Each seed id and the id that the adapter gives it: the identity for
    /// the JSON adapter.
    map: BTreeMap<String, String>,
}

impl Scenario {
    /// An install with `adapter` that holds `fixture`.
    fn new(adapter: Adapter, fixture: &Fixture) -> Self {
        let dir = scratch_git_repo();
        let store = dir.path().join(".git/fake-gh-store.json");
        let home = dir.path().join(".git/bd-home");
        match adapter {
            Adapter::Json => {
                run_ok(dir.path(), &["init", "--tracker", "json"]);
                write_json_backlog(dir.path(), fixture);
                let map = fixture.ids(Adapter::Json);
                Self {
                    adapter,
                    dir,
                    store,
                    home,
                    map,
                }
            }
            Adapter::Github => {
                write_store(&store, &github_store(fixture));
                let scenario = Self {
                    adapter,
                    dir,
                    store,
                    home,
                    map: fixture.ids(Adapter::Github),
                };
                let init = scenario.output(&["init", "--tracker", "github", "--repo", REPO]);
                assert_eq!(
                    init.status.code(),
                    Some(0),
                    "init --tracker github failed: {}",
                    stderr(&init)
                );
                scenario
            }
            Adapter::Beads => {
                create_beads_store(dir.path(), &home, &beads_rows(fixture));
                let scenario = Self {
                    adapter,
                    dir,
                    store,
                    home,
                    map: fixture.ids(Adapter::Beads),
                };
                let init = scenario.output(&["init", "--tracker", "beads"]);
                assert_eq!(
                    init.status.code(),
                    Some(0),
                    "init --tracker beads failed: {}",
                    stderr(&init)
                );
                scenario
            }
        }
    }

    /// `args` with each seed id replaced by the adapter's id, and `--dir`.
    fn command(&self, args: &[&str]) -> Command {
        let mut command = houserules();
        for arg in args {
            command.arg(self.map.get(*arg).map_or(*arg, String::as_str));
        }
        command.arg("--dir").arg(self.dir.path());
        match self.adapter {
            Adapter::Json => {}
            Adapter::Github => {
                command
                    .env("HOUSERULES_GH", fake_gh())
                    .env("FAKE_GH_STORE", &self.store);
            }
            Adapter::Beads => hermetic_bd(&mut command, &self.home),
        }
        command
    }

    /// The command of `args` with the fake `gh` as the program that runs as
    /// `bd`: it records every call in `store` and, when `failure` is set,
    /// fails each with that `{code, stderr}`.
    fn recorded(&self, args: &[&str], failure: Option<Value>) -> Command {
        write_store(&self.store, &json!({"calls": [], "failure": failure}));
        let mut command = self.command(args);
        command
            .env("HOUSERULES_BD", fake_gh())
            .env("FAKE_GH_STORE", &self.store);
        command
    }

    /// Runs `bd` with `args` in the scenario's install and requires exit 0.
    fn bd(&self, args: &[&str]) -> Output {
        run_bd(self.dir.path(), &self.home, args)
    }

    /// Imports `rows` into the Beads store of the scenario.
    fn import(&self, rows: &[Value]) {
        import_rows(self.dir.path(), &self.home, rows);
    }

    /// Runs `bd` with `args` in the scenario's install and returns its stdout
    /// as JSON.
    fn bd_json(&self, args: &[&str]) -> Value {
        let text = stdout(&self.bd(args));
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("bd {args:?} printed {text}: {error}"))
    }

    /// Every issue of the Beads store, closed ones included, in the JSON that
    /// `bd list` prints.
    fn bd_issues(&self) -> Vec<Value> {
        match self.bd_json(&["list", "--json", "--all", "--limit", "0"]) {
            Value::Array(issues) => issues,
            other => panic!("bd list printed {other}"),
        }
    }

    /// The issue `id` of the Beads store, in the JSON that `bd list` prints.
    fn bd_issue(&self, id: &str) -> Value {
        let id = self.map.get(id).map_or(id, String::as_str);
        self.bd_issues()
            .into_iter()
            .find(|issue| issue["id"] == json!(id))
            .unwrap_or_else(|| panic!("the store has no issue {id}"))
    }

    /// Runs `args` and returns the output, whatever the exit code.
    fn output(&self, args: &[&str]) -> Output {
        self.command(args)
            .output()
            .unwrap_or_else(|error| panic!("run houserules {args:?}: {error}"))
    }

    /// Runs `args`, requires exit 0, and returns stdout.
    fn ok(&self, args: &[&str]) -> String {
        let output = self.output(args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "houserules {args:?} failed: {}",
            stderr(&output)
        );
        stdout(&output)
    }

    /// Runs `args`, requires exit 0, and returns stdout as JSON.
    fn json(&self, args: &[&str]) -> Value {
        let text = self.ok(args);
        serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("{args:?} printed {text}: {error}"))
    }

    /// The store of the fake `gh`.
    fn read_store(&self) -> Value {
        let text = fs::read_to_string(&self.store).expect("read the store");
        serde_json::from_str(&text).expect("parse the store")
    }

    /// Applies `edit` to the store of the fake `gh`.
    fn edit_store(&self, edit: impl FnOnce(&mut Value)) {
        let mut store = self.read_store();
        edit(&mut store);
        write_store(&self.store, &store);
    }

    /// The argument lists of the calls that the fake `gh` has served.
    fn calls(&self) -> Vec<Vec<String>> {
        self.read_store()["calls"]
            .as_array()
            .expect("calls")
            .iter()
            .map(|call| {
                call.as_array()
                    .expect("a call")
                    .iter()
                    .map(|arg| arg.as_str().expect("an argument").to_string())
                    .collect()
            })
            .collect()
    }
}

/// The scenario under test and the JSON scenario of the same seed.
fn pair(adapter: Adapter, seed: Vec<Value>) -> (Scenario, Scenario) {
    let fixture = Fixture::new(seed);
    (
        Scenario::new(adapter, &fixture),
        Scenario::new(Adapter::Json, &fixture),
    )
}

/// `text` with each id that `map` knows replaced by its seed id. An id is a
/// whole run of letters, digits, and `#`, `.`, `_`, `-`, with the dots at its end
/// left out, so `hr-3` is not part of `hr-31`.
fn to_seed_ids(text: &str, map: &BTreeMap<String, String>) -> String {
    let to_seed: BTreeMap<&str, &str> = map
        .iter()
        .map(|(seed, adapter)| (adapter.as_str(), seed.as_str()))
        .collect();
    let is_id_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '#' | '.' | '_' | '-');
    let flush = |token: &mut String, out: &mut String| {
        let id = token.trim_end_matches('.');
        out.push_str(to_seed.get(id).copied().unwrap_or(id));
        out.push_str(&token[id.len()..]);
        token.clear();
    };
    let mut out = String::new();
    let mut token = String::new();
    for c in text.chars() {
        if is_id_char(c) {
            token.push(c);
        } else {
            flush(&mut token, &mut out);
            out.push(c);
        }
    }
    flush(&mut token, &mut out);
    out
}

/// `value` as spec 3.2 compares it: each string that `to_seed` maps becomes
/// its seed id, the adapter-only `file` field is gone, and a list of records
/// with an `id` is a set keyed by that id.
fn normalized(value: &Value, map: &BTreeMap<String, String>) -> Value {
    let to_seed: BTreeMap<&str, &str> = map
        .iter()
        .map(|(seed, adapter)| (adapter.as_str(), seed.as_str()))
        .collect();
    fn walk(value: &Value, to_seed: &BTreeMap<&str, &str>) -> Value {
        match value {
            Value::String(text) => json!(to_seed.get(text.as_str()).copied().unwrap_or(text)),
            Value::Object(fields) => Value::Object(
                fields
                    .iter()
                    .filter(|(key, _)| key.as_str() != "file")
                    .map(|(key, value)| (key.clone(), walk(value, to_seed)))
                    .collect(),
            ),
            Value::Array(items) => {
                let mut items: Vec<Value> = items.iter().map(|item| walk(item, to_seed)).collect();
                if items.iter().all(|item| item.get("id").is_some()) {
                    items.sort_by_key(|item| item["id"].to_string());
                }
                Value::Array(items)
            }
            other => other.clone(),
        }
    }
    walk(value, &to_seed)
}

/// Spec 3.2: the adapter's output equals the JSON adapter's output as parsed
/// JSON, each id mapped through `map` (seed id to adapter id), `file`
/// removed, and rows compared as a set keyed by id.
fn assert_equivalent(adapter_output: &Value, json_output: &Value, map: &BTreeMap<String, String>) {
    assert_eq!(
        normalized(adapter_output, map),
        normalized(json_output, &BTreeMap::new()),
        "the adapter output differs from the JSON output"
    );
}

/// Runs `args` on both scenarios and requires equivalent JSON.
fn same(under_test: &Scenario, reference: &Scenario, args: &[&str]) {
    eprintln!("comparing houserules {args:?}");
    let got = under_test.json(args);
    let want = reference.json(args);
    assert_equivalent(&got, &want, &under_test.map);
}

/// Runs `args` on both scenarios and requires the same stdout once the adapter
/// ids are seed ids.
fn same_text(under_test: &Scenario, reference: &Scenario, args: &[&str]) {
    assert_eq!(
        to_seed_ids(&under_test.ok(args), &under_test.map),
        reference.ok(args),
        "houserules {args:?}"
    );
}

/// Runs `args` on both scenarios and requires the same exit code and the same
/// stderr once the adapter ids are seed ids.
fn same_failure(under_test: &Scenario, reference: &Scenario, args: &[&str], code: i32) {
    let got = under_test.output(args);
    let want = reference.output(args);
    assert_eq!(
        (
            got.status.code(),
            to_seed_ids(&stderr(&got), &under_test.map)
        ),
        (Some(code), stderr(&want)),
        "houserules {args:?}"
    );
    assert_eq!(want.status.code(), Some(code), "the reference exit code");
}

/// Declares each conformance case once and runs it three times: the `json`
/// test pins the JSON adapter against itself, and the `github` and `beads`
/// tests run the hosted adapters against it.
macro_rules! conformance {
    ($($case:ident),* $(,)?) => {
        $(
            mod $case {
                use super::*;

                #[test]
                fn json() {
                    super::$case(Adapter::Json);
                }

                #[test]
                fn github() {
                    super::$case(Adapter::Github);
                }

                #[test]
                fn beads() {
                    super::$case(Adapter::Beads);
                }
            }
        )*
    };
}

conformance!(
    row_item_id,
    row_title,
    row_body,
    row_type,
    row_status,
    row_milestone,
    row_batch,
    row_section,
    row_see,
    row_rulings,
    row_tags,
    row_label_and_extra_lines,
    row_deferral,
    row_section_metadata,
    row_batch_record,
    row_amendment_decision_and_parked,
    row_parked_group,
    row_archive,
    check_passes_on_clean_data,
    check_names_a_see_that_does_not_exist,
    check_warns_on_done_without_a_batch,
    check_fails_on_two_batches_in_progress,
    check_fails_on_a_duplicate_batch,
    check_fails_on_a_deferral_without_a_reason,
    set_writes_each_status,
    set_writes_the_batch,
    set_writes_the_milestone,
    set_refuses_what_the_contract_refuses,
    set_and_defer_refuse_a_record,
    defer_files_a_new_item,
    defer_marks_an_existing_item,
    defer_refuses_what_the_contract_refuses,
    get_names_an_unknown_id,
    batch_names_an_unknown_batch,
);

/// Row `item id`: `get` prints the stored record of each id, in the order
/// given.
fn row_item_id(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["get", "WI-001"]);
    same(
        &under_test,
        &reference,
        &["get", "WI-004", "WI-002", "WI-001"],
    );
    same(&under_test, &reference, &["list"]);
}

/// Row `title`: the rows of `list` carry each item's title.
fn row_title(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["list"]);
    let rows = under_test.json(&["list"]);
    let titles: Vec<&str> = rows
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["title"].as_str())
        .collect();
    assert_eq!(
        titles,
        [
            "Build the core",
            "Fix the crash",
            "Drop the cache",
            "Pick the store"
        ]
    );
}

/// Row `body`: a paragraph list is the text before the block, split on blank
/// lines.
fn row_body(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["get", "WI-001", "WI-002"]);
    let item = &under_test.json(&["get", "WI-001"])[0];
    assert_eq!(
        item["body"],
        json!(["First paragraph.", "Second paragraph."])
    );
}

/// Row `type`: `list --type` filters on the type label.
fn row_type(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    for kind in ["feat", "bug", "chore", "decision", "research"] {
        same(&under_test, &reference, &["list", "--type", kind]);
    }
}

/// Row `status`: open, partial, done, and dropped each read back.
fn row_status(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    for status in ["open", "partial", "done", "dropped"] {
        same(&under_test, &reference, &["list", "--status", status]);
    }
    same(&under_test, &reference, &["list", "--open"]);
}

/// Row `milestone`: `list --milestone` filters on it, and `-` means none.
fn row_milestone(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["list", "--milestone", "v1"]);
    same(&under_test, &reference, &["list", "--milestone", "-"]);
}

/// Row `batch`: `list --batch` filters on the batch label.
fn row_batch(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["list", "--batch", "1"]);
    same(&under_test, &reference, &["list", "--batch", "2"]);
}

/// Row `section`: `list --section` filters on the section label, and `get`
/// prints the section.
fn row_section(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["list", "--section", "E01"]);
    same(&under_test, &reference, &["list", "--section", "E02"]);
    same(&under_test, &reference, &["get", "WI-001", "WI-004"]);
}

/// Row `see`: the `see` list of an item reads back with the ids of the
/// adapter.
fn row_see(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["get", "WI-001"]);
    let item = &under_test.json(&["get", "WI-001"])[0];
    assert_eq!(item["see"].as_array().map(Vec::len), Some(2));
}

/// Row `rulings`: the rulings of an item read back whole.
fn row_rulings(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["get", "WI-001"]);
    let item = &under_test.json(&["get", "WI-001"])[0];
    assert_eq!(
        item["rulings"],
        json!([{"date": "2026-10-01", "by": "user", "text": "Proceed."}])
    );
}

/// Row `tags`: each tag is a label and reads back in order.
fn row_tags(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["get", "WI-001", "WI-002"]);
    let item = &under_test.json(&["get", "WI-001"])[0];
    assert_eq!(item["tags"], json!(["alpha", "beta"]));
}

/// Row `label, extra_lines`: both read back from the block.
fn row_label_and_extra_lines(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["get", "WI-001"]);
    let item = &under_test.json(&["get", "WI-001"])[0];
    assert_eq!(item["label"], json!("Core"));
    assert_eq!(item["extra_lines"], json!(["An extra line."]));
}

/// Row `deferral`: `list --deferred` carries the object, and `get` prints it.
fn row_deferral(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["list", "--deferred"]);
    same(&under_test, &reference, &["get", "WI-004"]);
    let rows = under_test.json(&["list", "--deferred"]);
    assert_eq!(
        rows[0]["deferral"],
        json!({"reason": "Needs a design.", "origin": "batch 1", "files": ["src/a.rs", "src/b.rs"]})
    );
}

/// Row `section metadata`: a section record is no item and shows in no
/// output, and the items keep their section.
fn row_section_metadata(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["list"]);
    assert_eq!(under_test.json(&["list"]).as_array().map(Vec::len), Some(4));
    same(&under_test, &reference, &["list", "--section", "E01"]);
}

/// Row `batch record`: `batch <n>` prints the number, summary, kickoff,
/// status, and the item rows of the batch.
fn row_batch_record(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["batch", "1"]);
    same(&under_test, &reference, &["batch", "2"]);
    let record = under_test.json(&["batch", "1"]);
    assert_eq!(record["summary"], json!("The first batch"));
    assert_eq!(
        record["status"],
        json!({"state": "in-progress", "text": "Under way."})
    );
    assert_eq!(record["items"].as_array().map(Vec::len), Some(2));
}

/// Row `amendment, decision, parked`: `get` prints an amendment and a parked
/// item; a decision record is no item and shows in no output.
fn row_amendment_decision_and_parked(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same(&under_test, &reference, &["get", "A-01"]);
    same(&under_test, &reference, &["get", "PP-1-01", "PP-1-02"]);
    same(&under_test, &reference, &["list"]);
}

/// Row `parked group` (spec 3.5, C30): a group of parked items keeps its intro
/// and its batch, and its items are the parked items of that batch. The JSON
/// adapter holds the group in `backlog/parked.json`; the GitHub adapter reads
/// the issue that carries the label `houserules:parked`, the `batch:<n>` label
/// of the group, and `group: true` in its block, the Beads adapter the issue of
/// the type `houserules-parked` with the `batch:<n>` label and the same block,
/// and `get` prints it as the JSON file holds it, under the id of the issue.
/// The group is no item, and the items of the group read the batch of their
/// group.
fn row_parked_group(adapter: Adapter) {
    let fixture = Fixture::new(grouped_seed());
    let under_test = Scenario::new(adapter, &fixture);
    let reference = Scenario::new(Adapter::Json, &fixture);
    same(&under_test, &reference, &["get", "PP-1-01", "PP-1-02"]);
    same(&under_test, &reference, &["list"]);
    same_text(&under_test, &reference, &["check-backlog"]);
    let group = match adapter {
        Adapter::Json => {
            read_json(under_test.dir.path(), "backlog/parked.json")["groups"][0].clone()
        }
        Adapter::Github | Adapter::Beads => {
            let id = fixture.id_of_first(adapter, "group");
            let mut record = under_test.json(&["get", &id])[0].clone();
            assert_eq!(record["id"], json!(id));
            record.as_object_mut().expect("a record").remove("id");
            normalized(&record, &under_test.map)
        }
    };
    assert_eq!(
        group,
        json!({
            "batch": 1,
            "intro": GROUP_INTRO,
            "items": [
                {"id": "PP-1-01", "text": "Polish the help text."},
                {
                    "id": "PP-1-02", "text": "Rename the flag.",
                    "resolved": {"batch": 2, "note": "Renamed in batch 2."},
                },
            ],
        })
    );
}

/// Row `archive`: the JSON adapter moves done records to `backlog/archive/`;
/// the hosted adapters move nothing, say so, and make no call to `gh` or `bd`.
fn row_archive(adapter: Adapter) {
    let scenario = Scenario::new(adapter, &Fixture::new(seed()));
    match adapter {
        Adapter::Json => {
            let text = scenario.ok(&["archive"]);
            assert_eq!(
                text,
                "archived WI-002 -> backlog/archive/E01.json\narchived WI-003 -> backlog/archive/E01.json\narchive: 2 moved\n"
            );
            assert!(scenario.dir.path().join("backlog/archive").is_dir());
        }
        Adapter::Github => {
            let text = scenario.ok(&["archive"]);
            assert_eq!(
                text,
                "archive: the github tracker keeps closed items in place\narchive: 0 moved\n"
            );
            assert!(scenario.calls().is_empty(), "archive called gh");
        }
        Adapter::Beads => {
            let output = scenario
                .recorded(&["archive"], None)
                .output()
                .expect("run houserules archive");
            assert_eq!(
                (output.status.code(), stdout(&output), stderr(&output)),
                (
                    Some(0),
                    "archive: the beads tracker keeps closed items in place\narchive: 0 moved\n"
                        .to_string(),
                    String::new()
                )
            );
            assert!(scenario.calls().is_empty(), "archive called bd");
        }
    }
}

/// Contract check: data with no fault passes with no warning.
fn check_passes_on_clean_data(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same_text(&under_test, &reference, &["check-backlog"]);
    assert_check(adapter, &Fixture::new(seed()), 0, "backlog: ok\n", "");
}

/// Runs `check-backlog` on a scenario of `fixture` and requires exit `code`, the
/// whole of stdout, and the whole of stderr: each case names the exact lines
/// of each adapter for its fault, so a change of either line fails it.
fn assert_check(
    adapter: Adapter,
    fixture: &Fixture,
    code: i32,
    stdout_text: &str,
    stderr_text: &str,
) {
    let scenario = Scenario::new(adapter, fixture);
    let output = scenario.output(&["check-backlog"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (Some(code), stdout_text.to_string(), stderr_text.to_string()),
        "houserules check-backlog on the {adapter:?} adapter"
    );
}

/// The text that `adapter` prints for a fault: `json` for the JSON adapter,
/// `github` for the GitHub adapter, `beads` for the Beads adapter.
fn per_adapter(adapter: Adapter, json: String, github: String, beads: String) -> String {
    match adapter {
        Adapter::Json => json,
        Adapter::Github => github,
        Adapter::Beads => beads,
    }
}

/// Contract check: a `see` that names no record is an error. A Beads
/// dependency cannot name an issue that the store lacks, so the Beads seed
/// holds the entry as an external reference, which the adapter reads as a
/// `see` that names no record.
fn check_names_a_see_that_does_not_exist(adapter: Adapter) {
    let fixture = Fixture::new(seed_with("WI-003", |item| {
        item.insert("see".to_string(), json!(["WI-099"]));
    }));
    let stderr_text = per_adapter(
        adapter,
        "backlog/items/E01.json WI-003: see \"WI-099\" does not exist\n".to_string(),
        format!(
            "{}: see \"WI-099\" does not exist\n",
            fixture.id_at(Adapter::Github, 3)
        ),
        format!(
            "{}: see \"{}\" does not exist\n",
            fixture.id_at(Adapter::Beads, 3),
            beads_external_reference("WI-099")
        ),
    );
    assert_check(adapter, &fixture, 1, "", &stderr_text);
}

/// Contract check: a done item with no batch is a warning, not an error.
fn check_warns_on_done_without_a_batch(adapter: Adapter) {
    let fixture = Fixture::new(seed_with("WI-003", |item| {
        item.insert("status".to_string(), json!("done"));
    }));
    let stdout_text = per_adapter(
        adapter,
        "warn: backlog/items/E01.json WI-003: done without a batch\nbacklog: ok\n".to_string(),
        format!(
            "warn: {}: done without a batch\nbacklog: ok\n",
            fixture.id_at(Adapter::Github, 3)
        ),
        format!(
            "warn: {}: done without a batch\nbacklog: ok\n",
            fixture.id_at(Adapter::Beads, 3)
        ),
    );
    assert_check(adapter, &fixture, 0, &stdout_text, "");
}

/// Contract check: at most one batch is in progress.
fn check_fails_on_two_batches_in_progress(adapter: Adapter) {
    let fixture = Fixture::new(seed_with("2", |batch| {
        batch.insert(
            "status".to_string(),
            json!({"state": "in-progress", "text": ""}),
        );
    }));
    // The two batch records of `seed()` are the issues #7 and #8, and hr-7 and
    // hr-8.
    let stderr_text = per_adapter(
        adapter,
        "backlog/batches.json: 2 batches in progress (at most one)\n".to_string(),
        "2 batches in progress (at most one): #7, #8\n".to_string(),
        "2 batches in progress (at most one): hr-7, hr-8\n".to_string(),
    );
    assert_check(adapter, &fixture, 1, "", &stderr_text);
}

/// Contract check: no two batches share a number.
fn check_fails_on_a_duplicate_batch(adapter: Adapter) {
    let mut seed = seed();
    seed.push(json!({
        "kind": "batch", "number": 1, "items": [], "summary": "Again",
        "kickoff": "", "status": {"state": "planned", "text": ""},
    }));
    // The new batch record is the issue #13 (hr-13), after the 12 records of
    // `seed()`; batch 1 is the issue #7 (hr-7). The check reads the issues in
    // ascending id order: GitHub numbers sort as numbers, so #7 comes first,
    // and Beads ids sort as text, so hr-13 comes before hr-7.
    let fixture = Fixture::new(seed);
    let stderr_text = per_adapter(
        adapter,
        "backlog/batches.json: duplicate batch 1\n".to_string(),
        "#13: duplicate batch 1 (also #7)\n".to_string(),
        "hr-7: duplicate batch 1 (also hr-13)\n".to_string(),
    );
    assert_check(adapter, &fixture, 1, "", &stderr_text);
}

/// Contract check: every deferral has its reason.
fn check_fails_on_a_deferral_without_a_reason(adapter: Adapter) {
    let fixture = Fixture::new(seed_with("WI-004", |item| {
        item.insert(
            "deferral".to_string(),
            json!({"reason": "", "origin": "batch 1", "files": []}),
        );
    }));
    let stderr_text = per_adapter(
        adapter,
        "backlog/items/E02.json.items[0].deferral.reason: shorter than 1\n".to_string(),
        format!(
            "{}: the deferral has no reason\n",
            fixture.id_at(Adapter::Github, 5)
        ),
        format!(
            "{}: the deferral has no reason\n",
            fixture.id_at(Adapter::Beads, 5)
        ),
    );
    assert_check(adapter, &fixture, 1, "", &stderr_text);
}

/// Writes: `set status=<s>` prints the change and the item then reads as `s`,
/// for each status.
fn set_writes_each_status(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    for status in ["done", "dropped", "partial", "open"] {
        let assignment = format!("status={status}");
        same_text(&under_test, &reference, &["set", "WI-001", &assignment]);
        same(&under_test, &reference, &["list"]);
        same(&under_test, &reference, &["get", "WI-001"]);
    }
}

/// Writes: `set batch=<n>` replaces the batch of an item. The batch record of
/// the JSON adapter lists the items of `backlog/batches.json`, which `set`
/// leaves alone; the rows of `list --batch` follow the item.
fn set_writes_the_batch(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same_text(&under_test, &reference, &["set", "WI-004", "batch=2"]);
    same_text(
        &under_test,
        &reference,
        &["set", "WI-001", "batch=2", "status=done"],
    );
    same(&under_test, &reference, &["list", "--batch", "2"]);
    same(&under_test, &reference, &["list", "--batch", "1"]);
    same(&under_test, &reference, &["get", "WI-001", "WI-004"]);
}

/// Writes: `set milestone=<m>` replaces the milestone of an item on every
/// adapter (spec 3.5, C30): the item then reads with it, and `list --milestone`
/// follows it.
fn set_writes_the_milestone(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same_text(&under_test, &reference, &["set", "WI-001", "milestone=v2"]);
    same(&under_test, &reference, &["get", "WI-001"]);
    same(&under_test, &reference, &["list", "--milestone", "v2"]);
    same(&under_test, &reference, &["list", "--milestone", "v1"]);
    assert_eq!(
        under_test.json(&["get", "WI-001"])[0]["milestone"],
        json!("v2")
    );
    same_text(
        &under_test,
        &reference,
        &["set", "WI-002", "milestone=v2", "status=open"],
    );
    same(&under_test, &reference, &["list", "--milestone", "v2"]);
}

/// Writes: the usage faults of `set` are the same line and exit 2, and an item
/// that does not exist is named before a field that does not exist.
fn set_refuses_what_the_contract_refuses(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same_failure(&under_test, &reference, &["set", "WI-001", "colour=red"], 2);
    same_failure(
        &under_test,
        &reference,
        &["set", "WI-001", "status=nearly"],
        2,
    );
    same_failure(&under_test, &reference, &["set", "WI-001", "batch=0"], 2);
    same_failure(&under_test, &reference, &["set", "WI-001", "batch=one"], 2);
    same_failure(
        &under_test,
        &reference,
        &["set", "WI-001", "milestone=a b"],
        2,
    );
    same_failure(&under_test, &reference, &["set", "WI-404", "colour=red"], 2);
}

/// Writes: `set` and `defer --item` write items only (spec 3.5, C30). An id that
/// names an amendment or a parked item gets the line `unknown item "<id>"`,
/// exit 2, and changes nothing.
fn set_and_defer_refuse_a_record(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    same_failure(&under_test, &reference, &["set", "A-01", "status=done"], 2);
    same_failure(
        &under_test,
        &reference,
        &["set", "PP-1-01", "status=done"],
        2,
    );
    same_failure(
        &under_test,
        &reference,
        &["defer", "--item", "A-01", "--reason", "r", "--origin", "o"],
        2,
    );
    let refused = under_test.output(&["set", "A-01", "status=done"]);
    assert_eq!(
        (refused.status.code(), stderr(&refused)),
        (
            Some(2),
            format!("unknown item \"{}\"\n", under_test.map["A-01"])
        )
    );
    same(&under_test, &reference, &["get", "A-01", "PP-1-01"]);
}

/// Files a deferral with `args` (a `defer --title` command) on both scenarios,
/// teaches the scenario under test the id that it printed, and returns the JSON
/// id of the filed item.
fn file_on_both(under_test: &mut Scenario, reference: &Scenario, args: &[&str]) -> String {
    let on_adapter = under_test.ok(args).trim().to_string();
    let on_json = reference.ok(args).trim().to_string();
    under_test.map.insert(on_json.clone(), on_adapter);
    on_json
}

/// Writes: `defer --title` files an item that reads back with its deferral,
/// its section, and the next id; with no section the item goes to `general`.
fn defer_files_a_new_item(adapter: Adapter) {
    let (mut under_test, reference) = pair(adapter, seed());
    let args = [
        "defer",
        "--title",
        "Cache the lookups",
        "--reason",
        "Needs a design.",
        "--origin",
        "batch 2",
        "--file",
        "src\\cache.rs",
        "--file",
        "src/lookup.rs",
        "--section",
        "E02",
    ];
    let filed = file_on_both(&mut under_test, &reference, &args);
    assert_eq!(filed, "WI-005");
    same(&under_test, &reference, &["get", &filed]);
    same(&under_test, &reference, &["list", "--deferred"]);
    same(&under_test, &reference, &["list", "--section", "E02"]);
    let item = &reference.json(&["get", &filed])[0];
    assert_eq!(
        item["deferral"]["files"],
        json!(["src/cache.rs", "src/lookup.rs"])
    );

    let general = file_on_both(
        &mut under_test,
        &reference,
        &[
            "defer", "--title", "Second", "--reason", "Later.", "--origin", "batch 2",
        ],
    );
    assert_eq!(general, "WI-006");
    same(&under_test, &reference, &["get", &general]);
    same(&under_test, &reference, &["list", "--section", "general"]);
}

/// Writes: `defer --item` marks an existing item as a deferral and prints its
/// id.
fn defer_marks_an_existing_item(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    let args = [
        "defer", "--item", "WI-002", "--reason", "Wait.", "--origin", "batch 3",
    ];
    same_text(&under_test, &reference, &args);
    same(&under_test, &reference, &["get", "WI-002"]);
    same(&under_test, &reference, &["list", "--deferred"]);
}

/// Writes: the refusals of `defer` are the same line and exit 2.
fn defer_refuses_what_the_contract_refuses(adapter: Adapter) {
    let (under_test, reference) = pair(adapter, seed());
    let long = "x".repeat(121);
    same_failure(
        &under_test,
        &reference,
        &["defer", "--title", &long, "--reason", "r", "--origin", "o"],
        2,
    );
    same_failure(
        &under_test,
        &reference,
        &["defer", "--title", "t", "--reason", " ", "--origin", "o"],
        2,
    );
    same_failure(
        &under_test,
        &reference,
        &["defer", "--title", "t", "--reason", "r", "--origin", ""],
        2,
    );
    same_failure(
        &under_test,
        &reference,
        &[
            "defer",
            "--title",
            "t",
            "--reason",
            "r",
            "--origin",
            "o",
            "--section",
            "a b",
        ],
        2,
    );
}

/// Reads: an id that names nothing is `unknown id "<id>"`, exit 2.
fn get_names_an_unknown_id(adapter: Adapter) {
    let scenario = Scenario::new(adapter, &Fixture::new(seed()));
    let unknown = match adapter {
        Adapter::Json => "WI-999",
        Adapter::Github => "#999",
        Adapter::Beads => "hr-999",
    };
    let output = scenario.output(&["get", unknown]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(2),
            String::new(),
            format!("unknown id \"{unknown}\"\n")
        )
    );
}

/// Reads: a batch that has no record is `unknown batch "<n>"`, exit 2, and a
/// number that is no number is `batch needs a number`.
fn batch_names_an_unknown_batch(adapter: Adapter) {
    let scenario = Scenario::new(adapter, &Fixture::new(seed()));
    let output = scenario.output(&["batch", "9"]);
    assert_eq!(
        (output.status.code(), stderr(&output)),
        (Some(2), "unknown batch \"9\"\n".to_string())
    );
    let output = scenario.output(&["batch", "nine"]);
    assert_eq!(
        (output.status.code(), stderr(&output)),
        (Some(2), "batch needs a number\n".to_string())
    );
}

/// Spec 3.2, row order: the JSON adapter keeps its file order, here the order
/// of `A.json` before `B.json`, against the order of the ids.
#[test]
fn json_list_keeps_file_order() {
    let scenario = Scenario::new(Adapter::Json, &Fixture::new(order_seed()));
    let rows = scenario.json(&["list"]);
    assert_eq!(ids_of(&rows), ["WI-002", "WI-001"]);
}

/// Spec 3.2, row order: the hosted adapters return rows in ascending id order,
/// whatever order the API returns them in.
#[test]
fn github_list_is_in_ascending_id_order() {
    let fixture = Fixture::new(order_seed());
    let scenario = Scenario::new(Adapter::Github, &fixture);
    let rows = scenario.json(&["list"]);
    assert_eq!(
        ids_of(&rows),
        [
            fixture.id_map["WI-001"].as_str(),
            fixture.id_map["WI-002"].as_str()
        ]
    );
}

/// Spec 3.5: the items of a batch record are the items that carry its
/// `batch:<n>` label, so `set batch=<n>` moves an item between the records.
#[test]
fn github_batch_record_lists_the_items_that_carry_its_label() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    scenario.ok(&["set", "WI-004", "batch=2"]);
    scenario.ok(&["set", "WI-001", "batch=2"]);
    let ids = |number: &str| -> Vec<String> {
        let record = scenario.json(&["batch", number]);
        ids_of(&record["items"])
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        ids("2"),
        [
            scenario.map["WI-001"].clone(),
            scenario.map["WI-004"].clone()
        ]
    );
    assert_eq!(ids("1"), [scenario.map["WI-002"].clone()]);
}

/// Spec 3.5 (C29): the JSON adapter lists the `items` of its `batches.json`
/// record, which `set batch=<n>` does not change; `list --batch` follows the
/// item.
#[test]
fn json_batch_record_lists_batches_json_which_set_leaves_alone() {
    let scenario = Scenario::new(Adapter::Json, &Fixture::new(seed()));
    scenario.ok(&["set", "WI-004", "batch=2"]);
    scenario.ok(&["set", "WI-001", "batch=2"]);
    let ids = |number: &str| -> Vec<String> {
        let record = scenario.json(&["batch", number]);
        ids_of(&record["items"])
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    assert_eq!(ids("2"), Vec::<String>::new());
    assert_eq!(ids("1"), ["WI-001", "WI-002"]);
    assert_eq!(
        ids_of(&scenario.json(&["list", "--batch", "2"])),
        ["WI-001", "WI-004"]
    );
}

/// Two items whose file order and id order differ: `WI-001` is in section `B`
/// and `WI-002` in section `A`.
fn order_seed() -> Vec<Value> {
    let item = |id: &str, section: &str| {
        json!({
            "kind": "item", "id": id, "section": section, "type": "chore",
            "milestone": null, "status": "open", "title": id, "body": ["Text."], "batch": null,
        })
    };
    vec![
        json!({"kind": "section", "section": "B", "heading": "B", "title": "B", "spec": ""}),
        item("WI-001", "B"),
        json!({"kind": "section", "section": "A", "heading": "A", "title": "A", "spec": ""}),
        item("WI-002", "A"),
    ]
}

/// The `id` of each row of `rows`.
fn ids_of(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["id"].as_str())
        .collect()
}

/// A GitHub scenario over the raw `issues` (REST shape), with the record
/// labels and every label that the issues use in the store.
fn github_with_issues(issues: Vec<Value>) -> Scenario {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(Vec::new()));
    scenario.edit_store(|store| {
        let mut labels: Vec<Value> = store["labels"].as_array().expect("labels").clone();
        for issue in &issues {
            for label in issue["labels"].as_array().into_iter().flatten() {
                let name = label["name"].clone();
                if !labels.contains(&name) {
                    labels.push(name);
                }
            }
        }
        store["labels"] = Value::Array(labels);
        store["issues"] = Value::Array(issues);
    });
    scenario
}

/// A REST issue with `number`, the labels `labels`, and the state `state`
/// with `reason`.
fn raw_issue(number: u64, labels: &[&str], state: &str, reason: Value, body: &str) -> Value {
    json!({
        "number": number,
        "title": format!("Issue {number}"),
        "body": body,
        "state": state,
        "state_reason": reason,
        "labels": labels.iter().map(|name| json!({"name": name})).collect::<Vec<_>>(),
        "milestone": null,
    })
}

/// Review focus 1 (spec 3.5): the adapter reads only the last fenced block of
/// the `houserules` tag, at the end of the body. A human-written fence inside
/// the body, and a fence with text after it, are prose.
#[test]
fn block_reads_only_the_trailing_houserules_fence() {
    let human = "Intro.\n\n```houserules\nlabel: human\n```\n\nMore prose.";
    let real = format!("{human}\n\n```houserules\nlabel: real\nextra_lines: [\"x\"]\n```");
    let fence_then_text = "Start.\n\n```houserules\nlabel: not-a-block\n```\n\nAfter the fence.";
    let scenario = github_with_issues(vec![
        raw_issue(1, &["type:chore"], "open", Value::Null, &real),
        raw_issue(2, &["type:chore"], "open", Value::Null, fence_then_text),
    ]);
    let records = scenario.json(&["get", "1", "2"]);
    assert_eq!(records[0]["label"], json!("real"));
    assert_eq!(records[0]["extra_lines"], json!(["x"]));
    assert_eq!(
        records[0]["body"],
        json!(["Intro.", "```houserules\nlabel: human\n```", "More prose."])
    );
    assert!(records[1].get("label").is_none(), "got {}", records[1]);
    assert_eq!(
        records[1]["body"],
        json!([
            "Start.",
            "```houserules\nlabel: not-a-block\n```",
            "After the fence."
        ])
    );
}

/// An issue body that the GitHub web form saved has CRLF line endings: the
/// block and the paragraphs read the same.
#[test]
fn github_reads_a_block_and_paragraphs_with_crlf_line_endings() {
    let body = "One.\r\n\r\nTwo.\r\n\r\n```houserules\r\nlabel: Core\r\nsee: [\"#2\"]\r\n```\r\n";
    let scenario = github_with_issues(vec![raw_issue(
        1,
        &["type:feat"],
        "open",
        Value::Null,
        body,
    )]);
    let record = &scenario.json(&["get", "1"])[0];
    assert_eq!(record["body"], json!(["One.", "Two."]));
    assert_eq!(record["label"], json!("Core"));
    assert_eq!(record["see"], json!(["#2"]));
}

/// Review focus 4: a repository with more than 100 issues reads every page.
#[test]
fn github_reads_every_page_of_250_issues() {
    let issues = (1..=250)
        .map(|number| raw_issue(number, &["type:chore"], "open", Value::Null, ""))
        .collect();
    let scenario = github_with_issues(issues);
    let rows = scenario.json(&["list"]);
    let ids = ids_of(&rows);
    assert_eq!(ids.len(), 250);
    assert_eq!((ids[0], ids[249]), ("#1", "#250"));
    let calls = scenario.calls();
    assert_eq!(calls.len(), 1, "got {calls:?}");
    assert!(
        calls[0].contains(&"--paginate".to_string()),
        "got {calls:?}"
    );
}

/// Spec 3.5: the adapter drops every row that has a `pull_request` key, even
/// one that carries a type label, and an id that names a pull request is no
/// issue.
#[test]
fn github_drops_pull_requests() {
    let mut pull_request = raw_issue(2, &["type:feat"], "open", Value::Null, "");
    pull_request["pull_request"] = json!({"url": "https://example.test/pull/2"});
    let scenario = github_with_issues(vec![
        raw_issue(1, &["type:chore"], "open", Value::Null, ""),
        pull_request,
        raw_issue(3, &["type:chore", "batch:1"], "open", Value::Null, ""),
    ]);
    assert_eq!(ids_of(&scenario.json(&["list"])), ["#1", "#3"]);
    let output = scenario.output(&["get", "2"]);
    assert_eq!(
        (output.status.code(), stderr(&output)),
        (Some(2), "#2 is a pull request, not an issue\n".to_string())
    );
}

/// Spec 3.5: `#<n>` and the bare `<n>` name the same issue.
#[test]
fn github_bare_number_and_hash_resolve_alike() {
    let (under_test, _) = pair(Adapter::Github, seed());
    let bare = under_test.output(&["get", "3"]);
    let hash = under_test.output(&["get", "#3"]);
    assert_eq!(bare.status.code(), Some(0), "{}", stderr(&bare));
    assert_eq!(stdout(&bare), stdout(&hash));
    assert_eq!(
        serde_json::from_str::<Value>(&stdout(&bare)).expect("json")[0]["id"],
        json!("#3")
    );
}

/// Spec 3.5: a closed issue with no close reason reads as `done`; not planned
/// and duplicate read as `dropped`; a reopened issue is open.
#[test]
fn github_closed_without_reason_reads_done() {
    let scenario = github_with_issues(vec![
        raw_issue(1, &["type:chore"], "closed", Value::Null, ""),
        raw_issue(2, &["type:chore"], "closed", json!("completed"), ""),
        raw_issue(3, &["type:chore"], "closed", json!("not_planned"), ""),
        raw_issue(4, &["type:chore"], "closed", json!("duplicate"), ""),
        raw_issue(5, &["type:chore"], "open", json!("reopened"), ""),
        raw_issue(
            6,
            &["type:chore", "status:partial"],
            "open",
            Value::Null,
            "",
        ),
        raw_issue(
            7,
            &["type:chore", "status:partial"],
            "closed",
            Value::Null,
            "",
        ),
    ]);
    let rows = scenario.json(&["list"]);
    let statuses: Vec<&str> = rows
        .as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["status"].as_str())
        .collect();
    assert_eq!(
        statuses,
        [
            "done", "done", "dropped", "dropped", "open", "partial", "done"
        ]
    );
}

/// Spec 3.5: an issue with two type labels fails the adapter check, naming
/// the issue.
#[test]
fn github_two_type_labels_fail_the_adapter_check() {
    let scenario = github_with_issues(vec![
        raw_issue(1, &["type:chore"], "open", Value::Null, ""),
        raw_issue(2, &["type:chore", "type:feat"], "open", Value::Null, ""),
    ]);
    let output = scenario.output(&["check-backlog"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(1),
            String::new(),
            "#2 has two type labels\n".to_string()
        )
    );
}

/// Spec 3.5: the adapter check fails, naming `gh label create <label>`, when a
/// record label is missing.
#[test]
fn github_a_missing_record_label_names_the_gh_command() {
    let scenario = github_with_issues(vec![raw_issue(1, &["type:chore"], "open", Value::Null, "")]);
    scenario.edit_store(|store| {
        let labels: Vec<Value> = store["labels"]
            .as_array()
            .expect("labels")
            .iter()
            .filter(|label| *label != "houserules:batch")
            .cloned()
            .collect();
        store["labels"] = Value::Array(labels);
    });
    let output = scenario.output(&["check-backlog"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(1),
            String::new(),
            format!(
                "the label houserules:batch is missing; run gh label create houserules:batch --repo {REPO}\n"
            )
        )
    );
}

/// Spec 3.5: an issue with no houserules label and no type label reads with
/// the item shape, type, section, and milestone null; `list` omits it.
#[test]
fn github_an_issue_with_no_houserules_label_reads_with_the_item_shape() {
    let mut plain = raw_issue(2, &["bug"], "open", Value::Null, "Plain text.");
    plain["milestone"] = json!({"title": "v9", "number": 3});
    let scenario = github_with_issues(vec![
        raw_issue(1, &["type:chore"], "open", Value::Null, ""),
        plain,
    ]);
    let record = &scenario.json(&["get", "2"])[0];
    assert_eq!(
        *record,
        json!({
            "id": "#2", "type": null, "milestone": null, "status": "open",
            "title": "Issue 2", "body": ["Plain text."], "batch": null, "section": null,
        })
    );
    assert_eq!(ids_of(&scenario.json(&["list"])), ["#1"]);
}

/// Spec 3.5 (C30): `set` and `defer --item` write an issue that is an item and
/// nothing else. A batch record, a section record, a decision record, a parked
/// group, and an issue with no `type:` label each get `unknown item "<n>"`,
/// exit 2, and no call that writes reaches `gh`.
#[test]
fn github_set_and_defer_write_items_only() {
    let scenario = github_with_issues(vec![
        raw_issue(1, &["type:chore"], "open", Value::Null, ""),
        raw_issue(2, &["houserules:batch"], "open", Value::Null, ""),
        raw_issue(3, &["bug"], "open", Value::Null, "Plain text."),
        raw_issue(4, &["houserules:decision"], "open", Value::Null, ""),
        raw_issue(5, &["houserules:section"], "open", Value::Null, ""),
        raw_issue(
            6,
            &["houserules:parked", "batch:1"],
            "open",
            Value::Null,
            "Intro.\n\n```houserules\ngroup: true\n```",
        ),
    ]);
    let before = scenario.read_store()["issues"].clone();
    for number in ["2", "3", "4", "5", "6"] {
        let set = scenario.output(&["set", number, "batch=4", "status=dropped"]);
        let defer = scenario.output(&["defer", "--item", number, "--reason", "r", "--origin", "o"]);
        for (output, command) in [(set, "set"), (defer, "defer --item")] {
            assert_eq!(
                (output.status.code(), stdout(&output), stderr(&output)),
                (
                    Some(2),
                    String::new(),
                    format!("unknown item \"{number}\"\n")
                ),
                "{command} {number}"
            );
        }
    }
    let calls = scenario.calls();
    assert!(
        calls
            .iter()
            .all(|call| call.first().is_some_and(|word| word == "api")
                && call.contains(&"--paginate".to_string())),
        "a call that writes reached gh: {calls:?}"
    );
    assert_eq!(scenario.read_store()["issues"], before);
    scenario.ok(&["set", "1", "status=done"]);
}

/// Spec 3.5: a write creates the label and the milestone that it needs: `set`
/// adds the batch label and the partial label, creates the milestone, and
/// removes the old batch label; a milestone that exists is not created again.
#[test]
fn github_set_creates_label_and_milestone() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    let output = scenario.ok(&["set", "WI-002", "batch=7", "status=partial", "milestone=v9"]);
    assert_eq!(
        output,
        format!(
            "{}: batch=7 status=partial milestone=v9\n",
            scenario.map["WI-002"]
        )
    );
    // Spec 3.7: a backend call passes the id as its own argument, after `--`.
    let calls = scenario.calls();
    let edit = calls
        .iter()
        .find(|call| call.starts_with(&["issue".to_string(), "edit".to_string()]))
        .expect("an issue edit call");
    assert_eq!(edit[edit.len() - 2..], ["--", "3"]);
    let store = scenario.read_store();
    for label in ["batch:7", "status:partial"] {
        assert!(
            store["labels"]
                .as_array()
                .expect("labels")
                .contains(&json!(label)),
            "no label {label}"
        );
    }
    assert!(
        store["milestones"]
            .as_array()
            .expect("milestones")
            .iter()
            .any(|milestone| milestone["title"] == json!("v9")),
        "no milestone v9"
    );
    let issue = &store["issues"][2];
    let labels: Vec<&str> = issue["labels"]
        .as_array()
        .expect("labels")
        .iter()
        .filter_map(|label| label["name"].as_str())
        .collect();
    assert!(
        labels.contains(&"batch:7") && !labels.contains(&"batch:1"),
        "got {labels:?}"
    );
    assert_eq!(issue["state"], json!("open"));
    assert_eq!(issue["milestone"]["title"], json!("v9"));
    let before = store["milestones"].as_array().expect("milestones").len();
    scenario.ok(&["set", "WI-001", "milestone=v9"]);
    assert_eq!(
        scenario.read_store()["milestones"]
            .as_array()
            .expect("milestones")
            .len(),
        before
    );
}

/// Spec 3.5: `set status=done` closes as completed and `dropped` closes as not
/// planned; `open` reopens.
#[test]
fn github_set_closes_with_the_close_reason_of_the_status() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    let state = |scenario: &Scenario, seed_id: &str| -> (Value, Value) {
        let number: usize = scenario.map[seed_id][1..].parse().expect("a number");
        let issue = scenario.read_store()["issues"][number - 1].clone();
        (issue["state"].clone(), issue["state_reason"].clone())
    };
    scenario.ok(&["set", "WI-004", "status=done"]);
    assert_eq!(
        state(&scenario, "WI-004"),
        (json!("closed"), json!("completed"))
    );
    scenario.ok(&["set", "WI-004", "status=dropped"]);
    assert_eq!(
        state(&scenario, "WI-004"),
        (json!("closed"), json!("not_planned"))
    );
    scenario.ok(&["set", "WI-004", "status=open"]);
    assert_eq!(state(&scenario, "WI-004").0, json!("open"));
}

/// Spec 3.5: `defer --title` files an issue with the type, deferred, and
/// section labels (each created), and a body that holds the reason and the
/// deferral block, byte for byte.
#[test]
fn github_defer_files_the_issue_with_its_block() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    let id = scenario.ok(&[
        "defer",
        "--title",
        "Cache the lookups",
        "--reason",
        "Needs a design.",
        "--origin",
        "batch 2",
        "--file",
        "src\\cache.rs",
        "--section",
        "E09",
    ]);
    assert_eq!(id, "#13\n");
    let store = scenario.read_store();
    for label in ["type:chore", "houserules:deferred", "section:E09"] {
        assert!(
            store["labels"]
                .as_array()
                .expect("labels")
                .contains(&json!(label)),
            "no label {label}"
        );
    }
    let issue = store["issues"]
        .as_array()
        .expect("issues")
        .last()
        .expect("an issue")
        .clone();
    assert_eq!(issue["number"], json!(13));
    assert_eq!(issue["title"], json!("Cache the lookups"));
    assert_eq!(
        issue["body"],
        json!(
            "Needs a design.\n\n```houserules\ndeferral: {\"reason\":\"Needs a design.\",\"origin\":\"batch 2\",\"files\":[\"src/cache.rs\"]}\n```"
        )
    );
    let labels: Vec<&str> = issue["labels"]
        .as_array()
        .expect("labels")
        .iter()
        .filter_map(|label| label["name"].as_str())
        .collect();
    assert_eq!(labels, ["type:chore", "houserules:deferred", "section:E09"]);
}

/// The line of an unreachable GitHub tracker (spec 3.4).
fn unreachable_line(cause: &str, setup: &str) -> String {
    format!("tracker github cannot be reached: {cause}; {setup}\n")
}

/// Runs `check-backlog` and `list` on `scenario` and requires the line of an
/// unreachable tracker: exit 1 from the gate, exit 2 from `list`.
fn assert_unreachable(command: impl Fn(&[&str]) -> Command, line: &str) {
    for (args, code) in [(["check-backlog"], 1), (["list"], 2)] {
        let output = command(&args).output().expect("run houserules");
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(code), String::new(), line.to_string()),
            "houserules {args:?}"
        );
    }
}

/// Spec 3.4: `gh` is not installed.
#[test]
fn github_unreachable_gh_not_installed() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    let missing = scenario.dir.path().join("no-such-gh");
    assert_unreachable(
        |args| {
            let mut command = scenario.command(args);
            command.env("HOUSERULES_GH", &missing);
            command
        },
        &unreachable_line("gh is not installed", "install gh"),
    );
}

/// Spec 3.4: `gh` is not logged in.
#[test]
fn github_unreachable_not_logged_in() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    scenario.edit_store(|store| store["auth"] = json!(false));
    assert_unreachable(
        |args| scenario.command(args),
        &unreachable_line("gh is not logged in", "run gh auth login"),
    );
}

/// Spec 3.4: `gh` cannot see the repository.
#[test]
fn github_unreachable_cannot_see_repository() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    scenario.edit_store(|store| store["repo"] = json!("acme/other"));
    assert_unreachable(
        |args| scenario.command(args),
        &unreachable_line(
            &format!("gh cannot see {REPO}"),
            "check tracker.repo in .houserules.json and gh auth status",
        ),
    );
}

/// Spec 3.4: any other failure of `gh` prints the first line of its stderr.
#[test]
fn github_unreachable_other_failure_prints_first_stderr_line() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    scenario.edit_store(|store| {
        store["failure"] =
            json!({"code": 1, "stderr": "boom: the service fell over\nsecond line\n"});
    });
    assert_unreachable(
        |args| scenario.command(args),
        &unreachable_line(
            "boom: the service fell over",
            "run the command by hand to see the full error",
        ),
    );
}

/// Spec 3.4: a failure with no stderr is named by its exit status, never
/// printed as an empty cause.
#[test]
fn github_unreachable_failure_with_no_stderr_names_the_exit_status() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    scenario.edit_store(|store| store["failure"] = json!({"code": 3, "stderr": ""}));
    assert_unreachable(
        |args| scenario.command(args),
        &unreachable_line(
            "gh exited with status 3",
            "run the command by hand to see the full error",
        ),
    );
}

/// Spec 3.5: only `tracker.repo` is read. An id of another repository is no
/// id of this tracker, and the adapter makes no call to `gh` for it.
#[test]
fn github_does_not_resolve_other_repositories() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    for id in ["acme/other#12", "other#12", "#12/x"] {
        let output = scenario.output(&["get", id]);
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(2), String::new(), format!("unknown id \"{id}\"\n")),
            "get {id}"
        );
    }
    assert!(scenario.calls().is_empty(), "got {:?}", scenario.calls());
}

/// Spec 3.8: a knowledge read and `render --check` never call `gh`: with a
/// `gh` that is not logged in, both exit 0 and the fake serves no call.
#[test]
fn render_check_and_get_knowledge_never_call_gh() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    let added = scenario.output(&["pack", "add", "process"]);
    assert_eq!(added.status.code(), Some(0), "{}", stderr(&added));
    scenario.edit_store(|store| store["auth"] = json!(false));
    let render = scenario.output(&["render", "--check"]);
    assert_eq!(render.status.code(), Some(0), "{}", stderr(&render));
    let get = scenario.output(&["get", "process.tdd"]);
    assert_eq!(get.status.code(), Some(0), "{}", stderr(&get));
    assert!(
        stdout(&get).contains("\"id\": \"process.tdd\""),
        "got {}",
        stdout(&get)
    );
    assert!(scenario.calls().is_empty(), "got {:?}", scenario.calls());
}

/// Spec 3.3: the marker of a GitHub install holds the repository, and the
/// adapter reads only it: `init` wrote the key before any call to `gh`.
#[test]
fn github_init_stamps_the_tracker_key_with_the_repository() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(seed()));
    let marker = read_json(scenario.dir.path(), ".houserules.json");
    assert_eq!(
        marker["tracker"],
        json!({"adapter": "github", "repo": REPO})
    );
    assert!(
        scenario.calls().is_empty(),
        "init called gh: {:?}",
        scenario.calls()
    );
}

/// The refusal of `init --tracker beads` when `bd` is missing or the install
/// has no Beads store (spec 3.3).
const BEADS_REFUSAL: &str = "--tracker beads needs an initialized Beads store: run bd init (or bd bootstrap in a clone), then houserules init --tracker beads";

/// The line of an unreachable Beads tracker (spec 3.4).
fn beads_unreachable_line(cause: &str, setup: &str) -> String {
    format!("tracker beads cannot be reached: {cause}; {setup}\n")
}

/// The row of `bd import` for the issue `id`: a task titled `Issue <id>` with
/// the labels `labels`, the status `status`, and `description`.
fn bead(id: &str, labels: &[&str], status: &str, description: &str) -> Value {
    json!({
        "id": id,
        "title": format!("Issue {id}"),
        "description": description,
        "issue_type": "task",
        "status": status,
        "labels": labels,
    })
}

/// A closed `bead` with the close reason `reason`, when it has one.
fn closed_bead(id: &str, labels: &[&str], reason: Option<&str>) -> Value {
    let mut row = bead(id, labels, "closed", "");
    row["closed_at"] = json!("2026-10-20T10:00:00Z");
    if let Some(reason) = reason {
        row["close_reason"] = json!(reason);
    }
    row
}

/// A Beads scenario with an empty seed and `rows` imported.
fn beads_with_rows(rows: Vec<Value>) -> Scenario {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(Vec::new()));
    scenario.import(&rows);
    scenario
}

/// The labels of the issue `issue`, in the order that `bd list` prints them.
fn labels_of(issue: &Value) -> Vec<&str> {
    issue["labels"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// The `statuses` of the rows `rows`.
fn statuses_of(rows: &Value) -> Vec<&str> {
    rows.as_array()
        .expect("rows")
        .iter()
        .filter_map(|row| row["status"].as_str())
        .collect()
}

/// Spec 3.6: the Beads tests run the release that `mise.toml` pins, not
/// whichever `bd` comes first on `PATH`.
#[test]
fn beads_tests_run_the_pinned_bd() {
    let mise = fs::read_to_string(repo_root().join("mise.toml")).expect("read mise.toml");
    let pin = mise
        .lines()
        .find_map(|line| {
            line.strip_prefix("\"github:gastownhall/beads\" = \"")?
                .strip_suffix('"')
        })
        .expect("mise.toml pins github:gastownhall/beads");
    let dir = tempfile::tempdir().expect("tempdir");
    let output = run_bd(dir.path(), &dir.path().join("home"), &["--version"]);
    assert!(
        stdout(&output).contains(&format!("bd version {pin} ")),
        "mise.toml pins bd {pin}, and bd printed {}",
        stdout(&output)
    );
}

/// Spec 3.6: a Beads test fails, never skips, when `bd` is missing: the
/// harness panics, and the message names the remedy.
#[test]
fn beads_tests_fail_when_bd_is_missing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        run_bd_with(
            OsStr::new("houserules-no-such-bd"),
            dir.path(),
            &dir.path().join("home"),
            &["--version"],
        )
    }));
    let payload = outcome.expect_err("the harness must fail when bd is missing");
    let message = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
        .unwrap_or_default();
    assert!(
        message.contains("bd is not installed: run mise install"),
        "got {message:?}"
    );
}

/// Every `bd` that the tests start reads and writes no file outside the
/// scenario: the user configuration of the person who runs the tests stays
/// unread, and their home stays unwritten. The decoy stands for the home of the
/// test process; it holds a `bd` user configuration with a marker key.
#[test]
fn beads_runs_read_and_write_nothing_of_the_test_process_home() {
    let decoy = tempfile::tempdir().expect("tempdir");
    let config = decoy.path().join(".config/bd/config.yaml");
    fs::create_dir_all(config.parent().expect("a parent")).expect("create the decoy config dir");
    fs::write(&config, "list.limit: 7\n").expect("write the decoy");
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(Vec::new()));
    let before = snapshot(decoy.path());
    let mut command = Command::new(bd_program());
    command
        .args(["config", "show"])
        .current_dir(scenario.dir.path())
        .env("HOME", decoy.path())
        .env("USERPROFILE", decoy.path());
    hermetic_bd(&mut command, &scenario.home);
    let output = command.output().expect("run bd config show");
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        !stdout(&output).contains("(config.yaml)"),
        "bd read a user configuration: {}",
        stdout(&output)
    );
    assert_same_tree(&before, &snapshot(decoy.path()), "a bd run");
}

/// Runs `init --tracker beads` in `dir`, with `program` as `bd`.
fn init_beads(dir: &Path, program: &OsStr) -> Output {
    let mut command = houserules();
    command
        .args(["init", "--tracker", "beads"])
        .arg("--dir")
        .arg(dir)
        .env("HOUSERULES_BD", program);
    hermetic_bd(&mut command, &dir.join(".git/bd-home"));
    command.output().expect("run houserules init")
}

/// Requires `init --tracker beads` to refuse with the line of spec 3.3, exit 2,
/// and to write nothing, on every run: a refused `init --tracker` leaves the
/// tree as it found it, on every adapter (C37).
fn assert_init_refused(dir: &Path, program: &OsStr) {
    let refusal = (Some(2), format!("{BEADS_REFUSAL}\n"));
    let untouched = snapshot(dir);
    for run in ["first", "second"] {
        let output = init_beads(dir, program);
        assert_eq!((output.status.code(), stderr(&output)), refusal, "{run}");
        assert_same_tree(
            &untouched,
            &snapshot(dir),
            &format!("the {run} refused init --tracker beads"),
        );
    }
}

/// A program that runs as `bd`: the shell script `body`, written below `dir`.
#[cfg(unix)]
fn script_bd(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(".git").join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write the stand-in for bd");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("make it executable");
    path
}

/// C37 (spec 3.3, 3.4): `init --tracker beads` reads the issue prefix once,
/// before any write. When `bd` fails that read, the command prints the line of
/// an unreachable tracker, exits 2, and leaves the tree as it found it: no
/// install file, no `tracker` key, no schema. The stand-in fails every call.
#[test]
fn init_tracker_beads_with_a_failed_prefix_read_writes_nothing() {
    let dir = scratch_git_repo();
    let store = dir.path().join(".git/fake-gh-store.json");
    let failure = json!({"code": 1, "stderr": "Error: database is locked"});
    write_store(&store, &json!({"calls": [], "failure": failure}));
    let untouched = snapshot(dir.path());
    let mut command = houserules();
    command
        .args(["init", "--tracker", "beads", "--dir"])
        .arg(dir.path())
        .env("HOUSERULES_BD", fake_gh())
        .env("FAKE_GH_STORE", &store);
    hermetic_bd(&mut command, &dir.path().join(".git/bd-home"));
    let output = command.output().expect("run houserules init");
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(2),
            String::new(),
            beads_unreachable_line(
                "Error: database is locked",
                "run the command by hand to see the full error"
            )
        )
    );
    assert_same_tree(
        &untouched,
        &snapshot(dir.path()),
        "init --tracker beads after a failed prefix read",
    );
    let served: Value = serde_json::from_str(&fs::read_to_string(&store).expect("read the store"))
        .expect("parse the store");
    assert_eq!(
        served["calls"],
        json!([["config", "get", "issue_prefix", "--json"]]),
        "one read of the prefix, and no other call"
    );
}

/// C37: the prefix that `bd` names to `init --tracker beads` is the prefix of
/// the `backlogId` pattern, and `init` asks for it once. The stand-in answers
/// its first call and fails every later one, as the review of Task 3 found a
/// `bd` that locks its database does: a second read would fail, and the pattern
/// would fall back to any token.
#[cfg(unix)]
#[test]
fn init_tracker_beads_reads_the_prefix_once_and_writes_the_pattern_of_it() {
    let dir = scratch_git_repo();
    let log = dir.path().join(".git/flaky-bd.log");
    let count = dir.path().join(".git/flaky-bd.count");
    let flaky = script_bd(
        dir.path(),
        "flaky-bd",
        r#"n=$(cat "$FLAKY_COUNT" 2>/dev/null || echo 0); n=$((n+1)); echo $n > "$FLAKY_COUNT"
echo "call $n: $*" >> "$FLAKY_LOG"
if [ "$n" -eq 1 ]; then echo '{"key":"issue_prefix","value":"hr"}'; exit 0; fi
echo "Error: database is locked" >&2; exit 1"#,
    );
    let mut command = houserules();
    command
        .args(["init", "--tracker", "beads", "--dir"])
        .arg(dir.path())
        .env("HOUSERULES_BD", &flaky)
        .env("FLAKY_COUNT", &count)
        .env("FLAKY_LOG", &log);
    hermetic_bd(&mut command, &dir.path().join(".git/bd-home"));
    let output = command.output().expect("run houserules init");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "beads"}));
    let schema = read_json(dir.path(), ".claude/schemas/deliverables.json");
    assert_eq!(
        schema["$defs"]["backlogId"]["pattern"],
        json!("^hr-[0-9a-z]+(\\.[0-9]+)*$")
    );
    assert_eq!(
        fs::read_to_string(&log).expect("read the call log"),
        "call 1: config get issue_prefix --json\n"
    );
}

/// Ruling 188 (design.md 188): houserules turns the usage metrics of `bd` off
/// for every `bd` it starts, in a project and in its CI, whatever the
/// environment of the person who runs it. The stand-in records the two
/// variables it sees, and houserules runs with neither of them set.
#[cfg(unix)]
#[test]
fn every_bd_houserules_starts_has_the_usage_metrics_off() {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    tracker_support::edit_marker(dir.path(), |marker| {
        marker.insert("tracker".to_string(), json!({"adapter": "beads"}));
    });
    let record = dir.path().join(".git/metrics.log");
    let recorder = script_bd(
        dir.path(),
        "recording-bd",
        r#"echo "${BD_DISABLE_METRICS-unset} ${DO_NOT_TRACK-unset}" >> "$METRICS_LOG"
echo "Error: stand-in" >&2; exit 1"#,
    );
    let fresh = scratch_git_repo();
    for (root, args) in [
        (dir.path(), vec!["list"]),
        (dir.path(), vec!["check-backlog"]),
        (dir.path(), vec!["get", "hr-1"]),
        (fresh.path(), vec!["init", "--tracker", "beads"]),
    ] {
        let mut command = houserules();
        command
            .args(&args)
            .arg("--dir")
            .arg(root)
            .env("HOUSERULES_BD", &recorder)
            .env("METRICS_LOG", &record)
            .env_remove("BD_DISABLE_METRICS")
            .env_remove("DO_NOT_TRACK");
        command.output().expect("run houserules");
    }
    assert_eq!(
        fs::read_to_string(&record).expect("read the record"),
        "1 1\n".repeat(4),
        "the variables of each bd call: BD_DISABLE_METRICS and DO_NOT_TRACK"
    );
}

/// Spec 3.3: `init --tracker beads` with `bd` missing refuses.
#[test]
fn init_tracker_beads_without_bd_refuses() {
    let dir = scratch_git_repo();
    let missing = dir.path().join("no-such-bd");
    assert_init_refused(dir.path(), missing.as_os_str());
}

/// Spec 3.3: `init --tracker beads` with no store refuses; `bd init`, the step
/// the line names, then lets the same command finish.
#[test]
fn init_tracker_beads_without_store_refuses() {
    let dir = scratch_git_repo();
    assert_init_refused(dir.path(), &bd_program());
    let home = dir.path().join(".git/bd-home");
    run_bd(
        dir.path(),
        &home,
        &[
            "init",
            "--non-interactive",
            "--prefix",
            BEADS_PREFIX,
            "--quiet",
            "--skip-agents",
            "--skip-hooks",
        ],
    );
    let output = init_beads(dir.path(), &bd_program());
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "beads"}));
}

/// Spec 3.4: `bd` is not installed.
#[test]
fn beads_unreachable_bd_not_installed() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let missing = scenario.dir.path().join("no-such-bd");
    assert_unreachable(
        |args| {
            let mut command = scenario.command(args);
            command.env("HOUSERULES_BD", &missing);
            command
        },
        &beads_unreachable_line("bd is not installed", "install bd"),
    );
}

/// Spec 3.4: the install has no Beads store, as a clone has none before `bd
/// bootstrap`.
#[test]
fn beads_unreachable_no_store() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    fs::remove_dir_all(scenario.dir.path().join(".beads")).expect("remove .beads");
    assert_unreachable(
        |args| scenario.command(args),
        &beads_unreachable_line(
            "no Beads store",
            "run bd bootstrap in a clone, or bd init for a new store",
        ),
    );
}

/// Spec 3.4: any other failure of `bd` prints the first line of its stderr
/// that is no warning. The stand-in fails every call and prints a warning
/// first, as `bd` does for a store whose directory is open to the group.
#[test]
fn beads_unreachable_other_failure_prints_first_stderr_line() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let failure = json!({
        "code": 1,
        "stderr": "Warning: the store directory has permissions 0755\nboom: the service fell over\nsecond line\n",
    });
    assert_unreachable(
        |args| scenario.recorded(args, Some(failure.clone())),
        &beads_unreachable_line(
            "boom: the service fell over",
            "run the command by hand to see the full error",
        ),
    );
}

/// Spec 3.4 against the real `bd`: a store whose `metadata.json` is no JSON
/// fails with a warning that names the file, and then one error line; the
/// cause is the error line, not the warning that comes first.
#[test]
fn beads_unreachable_failure_of_the_real_bd_prints_its_error_line() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(Vec::new()));
    fs::write(scenario.dir.path().join(".beads/metadata.json"), "{").expect("write metadata.json");
    assert_unreachable(
        |args| scenario.command(args),
        &beads_unreachable_line(
            "Error: loading config: parsing config: unexpected end of JSON input",
            "run the command by hand to see the full error",
        ),
    );
}

/// Spec 3.4: a failure with no stderr is named by its exit status, never
/// printed as an empty cause.
#[test]
fn beads_unreachable_failure_with_no_stderr_names_the_exit_status() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    assert_unreachable(
        |args| scenario.recorded(args, Some(json!({"code": 3, "stderr": ""}))),
        &beads_unreachable_line(
            "bd exited with status 3",
            "run the command by hand to see the full error",
        ),
    );
}

/// Spec 3.8: a knowledge read and `render --check` never call `bd`: with a
/// stand-in that fails and records every call, both exit 0 and it serves no
/// call.
#[test]
fn render_check_and_get_knowledge_never_call_bd() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let added = scenario.output(&["pack", "add", "process"]);
    assert_eq!(added.status.code(), Some(0), "{}", stderr(&added));
    let failure = json!({"code": 1, "stderr": "boom"});
    let render = scenario
        .recorded(&["render", "--check"], Some(failure.clone()))
        .output()
        .expect("run houserules render");
    assert_eq!(render.status.code(), Some(0), "{}", stderr(&render));
    assert!(scenario.calls().is_empty(), "got {:?}", scenario.calls());
    let get = scenario
        .recorded(&["get", "process.tdd"], Some(failure))
        .output()
        .expect("run houserules get");
    assert_eq!(get.status.code(), Some(0), "{}", stderr(&get));
    assert!(
        stdout(&get).contains("\"id\": \"process.tdd\""),
        "got {}",
        stdout(&get)
    );
    assert!(scenario.calls().is_empty(), "got {:?}", scenario.calls());
}

/// Spec 3.7: an id that has no shape of a Beads id is no id of the tracker, and
/// the adapter makes no call to `bd` for it.
#[test]
fn beads_does_not_send_an_id_of_no_declared_shape_to_bd() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    for id in [
        "hr 1", "hr-", "x/hr-1", "hr-1;ls", "hr-1.", "hr-A1", "HR_ 1",
    ] {
        let output = scenario
            .recorded(&["get", id], None)
            .output()
            .expect("run houserules get");
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(2), String::new(), format!("unknown id \"{id}\"\n")),
            "get {id}"
        );
        assert!(scenario.calls().is_empty(), "got {:?}", scenario.calls());
    }
}

/// Spec 3.3 and 3.9: the marker of a Beads install holds the adapter alone, and
/// the `backlogId` pattern of the deliverables schema holds the prefix that
/// `bd` reports.
#[test]
fn beads_init_stamps_the_tracker_key_and_the_pattern_of_the_prefix() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let marker = read_json(scenario.dir.path(), ".houserules.json");
    assert_eq!(marker["tracker"], json!({"adapter": "beads"}));
    let schema = read_json(scenario.dir.path(), ".claude/schemas/deliverables.json");
    assert_eq!(
        schema["$defs"]["backlogId"]["pattern"],
        json!("^hr-[0-9a-z]+(\\.[0-9]+)*$")
    );
}

/// Spec 3.5, read direction: every state of `bd` that is no `partial` and no
/// `closed` reads as `open`.
#[test]
fn beads_in_progress_reads_open() {
    let rows = [
        "open",
        "in_progress",
        "blocked",
        "hooked",
        "pinned",
        "deferred",
    ]
    .iter()
    .enumerate()
    .map(|(index, status)| bead(&format!("hr-{}", index + 1), &["type:chore"], status, ""))
    .collect();
    let scenario = beads_with_rows(rows);
    assert_eq!(statuses_of(&scenario.json(&["list"])), ["open"; 6]);
    for filter in [&["list", "--open"][..], &["list", "--status", "open"][..]] {
        assert_eq!(scenario.json(filter).as_array().map(Vec::len), Some(6));
    }
    assert_eq!(
        scenario
            .json(&["list", "--status", "partial"])
            .as_array()
            .map(Vec::len),
        Some(0)
    );
}

/// Spec 3.5 and 3.2: a closed issue reads as `done` unless its close reason is
/// `dropped`; a `partial` issue reads as `partial`.
#[test]
fn beads_closed_without_reason_reads_done() {
    let scenario = beads_with_rows(vec![
        closed_bead("hr-1", &["type:chore"], None),
        closed_bead("hr-2", &["type:chore"], Some("completed")),
        closed_bead("hr-3", &["type:chore"], Some("dropped")),
        closed_bead("hr-4", &["type:chore"], Some("Closed")),
        closed_bead("hr-5", &["type:chore"], Some("Won't fix")),
        bead("hr-6", &["type:chore"], "open", ""),
        bead("hr-7", &["type:chore"], "partial", ""),
    ]);
    assert_eq!(
        statuses_of(&scenario.json(&["list"])),
        ["done", "done", "dropped", "done", "done", "open", "partial"]
    );
}

/// Spec 3.5: an item of the type `decision` and a decision record stay apart.
/// The item has the native type `decision` and the label `type:decision`; the
/// record has the custom type `houserules-decision`. `list` holds the item, the
/// record is read with the item shape and a `null` type, and `set` writes the
/// item only.
#[test]
fn beads_decision_item_and_decision_record_stay_apart() {
    let fixture = Fixture::new(seed());
    let scenario = Scenario::new(Adapter::Beads, &fixture);
    let item = fixture.id_at(Adapter::Beads, 5);
    let record = fixture.id_at(Adapter::Beads, 11);
    assert_eq!(scenario.bd_issue(&item)["issue_type"], json!("decision"));
    assert_eq!(
        scenario.bd_issue(&record)["issue_type"],
        json!("houserules-decision")
    );
    let decisions = scenario.json(&["list", "--type", "decision"]);
    assert_eq!(ids_of(&decisions), [item.as_str()]);
    assert!(
        !ids_of(&scenario.json(&["list"])).contains(&record.as_str()),
        "the decision record is no item"
    );
    let printed = &scenario.json(&["get", &record])[0];
    assert_eq!(
        (printed["id"].clone(), printed["type"].clone()),
        (json!(record), Value::Null)
    );
    let output = scenario.output(&["set", &record, "status=done"]);
    assert_eq!(
        (output.status.code(), stderr(&output)),
        (Some(2), format!("unknown item \"{record}\"\n"))
    );
    scenario.ok(&["set", &item, "status=done"]);
}

/// Spec 3.5: the adapter check fails, naming the `bd config` step, when the
/// custom status `partial` is missing or sits in another category than
/// `active` (so `bd ready` would not show it), or when a custom type of a
/// record is missing. The step keeps the entries that the store already has,
/// because `bd config set` replaces the whole list.
#[test]
fn beads_missing_partial_status_names_the_bd_config_step() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(Vec::new()));
    let check = |expected: &str| {
        let output = scenario.output(&["check-backlog"]);
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(1), String::new(), expected.to_string())
        );
    };
    scenario.bd(&["config", "unset", "status.custom"]);
    check("the custom status partial is missing; run bd config set status.custom partial:active\n");
    scenario.bd(&["config", "set", "status.custom", "review:wip"]);
    check(
        "the custom status partial is missing; run bd config set status.custom 'review:wip,partial:active'\n",
    );
    scenario.bd(&[
        "config",
        "set",
        "status.custom",
        "partial:frozen,review:wip",
    ]);
    check(
        "the custom status partial is not in the active category; run bd config set status.custom 'partial:active,review:wip'\n",
    );
    scenario.bd(&[
        "config",
        "set",
        "status.custom",
        "partial:active,review:wip",
    ]);
    scenario.bd(&["config", "unset", "types.custom"]);
    check(
        "the custom types houserules-section, houserules-batch, houserules-amendment, houserules-decision, houserules-parked are missing; run bd config set types.custom 'houserules-section,houserules-batch,houserules-amendment,houserules-decision,houserules-parked'\n",
    );
    scenario.bd(&[
        "config",
        "set",
        "types.custom",
        "houserules-section,spike-ish,houserules-batch,houserules-amendment,houserules-decision",
    ]);
    check(
        "the custom type houserules-parked is missing; run bd config set types.custom 'houserules-section,spike-ish,houserules-batch,houserules-amendment,houserules-decision,houserules-parked'\n",
    );
}

/// Spec 3.2, row order: the hosted adapters return rows in ascending id order,
/// whatever order `bd` returns them in. Ids sort as text, as `bd list --sort
/// id` sorts them.
#[test]
fn beads_list_is_in_ascending_id_order() {
    let scenario = beads_with_rows(
        ["hr-b", "hr-10", "hr-9", "hr-a"]
            .iter()
            .map(|id| bead(id, &["type:chore"], "open", ""))
            .collect(),
    );
    assert_eq!(
        ids_of(&scenario.json(&["list"])),
        ["hr-10", "hr-9", "hr-a", "hr-b"]
    );
}

/// Spec 3.5 (C29): the items of a batch record are the items that carry its
/// `batch:<n>` label, so `set batch=<n>` moves an item between the records.
#[test]
fn beads_batch_record_lists_the_items_that_carry_its_label() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    scenario.ok(&["set", "WI-004", "batch=2"]);
    scenario.ok(&["set", "WI-001", "batch=2"]);
    let ids = |number: &str| -> Vec<String> {
        let record = scenario.json(&["batch", number]);
        ids_of(&record["items"])
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        ids("2"),
        [
            scenario.map["WI-001"].clone(),
            scenario.map["WI-004"].clone()
        ]
    );
    assert_eq!(ids("1"), [scenario.map["WI-002"].clone()]);
}

/// `bd list` stops at 50 rows unless it is told otherwise: the adapter reads
/// every issue of a store with more.
#[test]
fn beads_list_is_not_capped_at_the_default_limit() {
    let rows = (1..=60)
        .map(|number| bead(&format!("hr-{number}"), &["type:chore"], "open", ""))
        .collect();
    let scenario = beads_with_rows(rows);
    assert_eq!(ids_of(&scenario.json(&["list"])).len(), 60);
}

/// Spec 3.5: `see` reads the `related` dependencies of an issue, in id order;
/// a dependency of another type is no `see`.
#[test]
fn beads_see_reads_the_related_dependencies_in_id_order() {
    let mut item = bead("hr-1", &["type:chore"], "open", "");
    let edge = |target: &str, kind: &str| json!({"issue_id": "hr-1", "depends_on_id": target, "type": kind});
    item["dependencies"] = json!([
        edge("hr-9", "related"),
        edge("hr-3", "related"),
        edge("hr-4", "blocks"),
        edge("hr-5", "discovered-from"),
    ]);
    let scenario = beads_with_rows(vec![
        item,
        bead("hr-3", &["type:chore"], "open", ""),
        bead("hr-4", &["type:chore"], "open", ""),
        bead("hr-5", &["type:chore"], "open", ""),
        bead("hr-9", &["type:chore"], "open", ""),
    ]);
    assert_eq!(
        scenario.json(&["get", "hr-1"])[0]["see"],
        json!(["hr-3", "hr-9"])
    );
}

/// Spec 3.5: the two type labels fail the adapter check, naming the issue.
#[test]
fn beads_two_type_labels_fail_the_adapter_check() {
    let scenario = beads_with_rows(vec![
        bead("hr-1", &["type:chore"], "open", ""),
        bead("hr-2", &["type:chore", "type:feat"], "open", ""),
    ]);
    let output = scenario.output(&["check-backlog"]);
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(1),
            String::new(),
            "hr-2 has two type labels\n".to_string()
        )
    );
}

/// Spec 3.5: an issue with no type label reads with the item shape, type,
/// section, and milestone null; `list` omits it.
#[test]
fn beads_an_issue_with_no_houserules_label_reads_with_the_item_shape() {
    let mut plain = bead("hr-2", &["bug", "milestone:v9"], "open", "Plain text.");
    plain["issue_type"] = json!("bug");
    let scenario = beads_with_rows(vec![bead("hr-1", &["type:chore"], "open", ""), plain]);
    let record = &scenario.json(&["get", "hr-2"])[0];
    assert_eq!(
        *record,
        json!({
            "id": "hr-2", "type": null, "milestone": null, "status": "open",
            "title": "Issue hr-2", "body": ["Plain text."], "batch": null, "section": null,
        })
    );
    assert_eq!(ids_of(&scenario.json(&["list"])), ["hr-1"]);
}

/// Review focus 1 (spec 3.5): the adapter reads only the last fenced block of
/// the `houserules` tag, at the end of the description. A human-written fence
/// inside the description, and a fence with text after it, are prose.
#[test]
fn beads_reads_only_the_trailing_houserules_fence() {
    let human = "Intro.\n\n```houserules\nlabel: human\n```\n\nMore prose.";
    let real = format!("{human}\n\n```houserules\nlabel: real\nextra_lines: [\"x\"]\n```");
    let fence_then_text = "Start.\n\n```houserules\nlabel: not-a-block\n```\n\nAfter the fence.";
    let scenario = beads_with_rows(vec![
        bead("hr-1", &["type:chore"], "open", &real),
        bead("hr-2", &["type:chore"], "open", fence_then_text),
    ]);
    let records = scenario.json(&["get", "hr-1", "hr-2"]);
    assert_eq!(records[0]["label"], json!("real"));
    assert_eq!(records[0]["extra_lines"], json!(["x"]));
    assert_eq!(
        records[0]["body"],
        json!(["Intro.", "```houserules\nlabel: human\n```", "More prose."])
    );
    assert!(records[1].get("label").is_none(), "got {}", records[1]);
    assert_eq!(
        records[1]["body"],
        json!([
            "Start.",
            "```houserules\nlabel: not-a-block\n```",
            "After the fence."
        ])
    );
}

/// Spec 3.5 (C30): `set` and `defer --item` write an issue that is an item and
/// nothing else. A batch record, a section record, a decision record, a parked
/// group, and an issue with no `type:` label each get `unknown item "<id>"`,
/// exit 2, and change nothing in the store.
#[test]
fn beads_set_and_defer_write_items_only() {
    let typed = |id: &str, issue_type: &str, labels: &[&str], description: &str| {
        let mut row = bead(id, labels, "open", description);
        row["issue_type"] = json!(issue_type);
        row
    };
    let scenario = beads_with_rows(vec![
        bead("hr-1", &["type:chore"], "open", ""),
        typed("hr-2", "houserules-batch", &[], ""),
        bead("hr-3", &["bug"], "open", "Plain text."),
        typed("hr-4", "houserules-decision", &[], ""),
        typed("hr-5", "houserules-section", &[], ""),
        typed(
            "hr-6",
            "houserules-parked",
            &["batch:1"],
            "Intro.\n\n```houserules\ngroup: true\n```",
        ),
    ]);
    let before = scenario.bd_issues();
    for id in ["hr-2", "hr-3", "hr-4", "hr-5", "hr-6"] {
        let set = scenario.output(&["set", id, "batch=4", "status=dropped"]);
        let defer = scenario.output(&["defer", "--item", id, "--reason", "r", "--origin", "o"]);
        for (output, command) in [(set, "set"), (defer, "defer --item")] {
            assert_eq!(
                (output.status.code(), stdout(&output), stderr(&output)),
                (Some(2), String::new(), format!("unknown item \"{id}\"\n")),
                "{command} {id}"
            );
        }
    }
    assert_eq!(scenario.bd_issues(), before);
    scenario.ok(&["set", "hr-1", "status=done"]);
}

/// Spec 3.5: a write creates the label that it needs: `set` adds the batch
/// label, the partial status, and the milestone label, and removes the old
/// batch label; a milestone that an item already has is not written again.
#[test]
fn beads_set_writes_the_labels_and_the_status() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let output = scenario.ok(&["set", "WI-002", "batch=7", "status=partial", "milestone=v9"]);
    assert_eq!(
        output,
        format!(
            "{}: batch=7 status=partial milestone=v9\n",
            scenario.map["WI-002"]
        )
    );
    let issue = scenario.bd_issue("WI-002");
    let labels = labels_of(&issue);
    assert!(
        labels.contains(&"batch:7") && labels.contains(&"milestone:v9"),
        "got {labels:?}"
    );
    assert!(!labels.contains(&"batch:1"), "got {labels:?}");
    assert_eq!(issue["status"], json!("partial"));
    assert!(issue.get("close_reason").is_none(), "got {issue}");
    scenario.ok(&["set", "WI-001", "milestone=v9"]);
    let labels = labels_of(&scenario.bd_issue("WI-001")).join(" ");
    assert!(
        labels.contains("milestone:v9") && !labels.contains("milestone:v1"),
        "got {labels}"
    );
}

/// Spec 3.5: `set status=done` closes with the reason `completed` and
/// `dropped` with the reason `dropped`, reopening a closed issue first, because
/// `bd close` keeps the reason of an issue that is closed; `open` reopens.
#[test]
fn beads_set_closes_with_the_close_reason_of_the_status() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let state = |scenario: &Scenario| -> (Value, Value) {
        let issue = scenario.bd_issue("WI-004");
        (
            issue["status"].clone(),
            issue.get("close_reason").cloned().unwrap_or(Value::Null),
        )
    };
    scenario.ok(&["set", "WI-004", "status=done"]);
    assert_eq!(state(&scenario), (json!("closed"), json!("completed")));
    scenario.ok(&["set", "WI-004", "status=dropped"]);
    assert_eq!(state(&scenario), (json!("closed"), json!("dropped")));
    scenario.ok(&["set", "WI-004", "status=done"]);
    assert_eq!(state(&scenario), (json!("closed"), json!("completed")));
    scenario.ok(&["set", "WI-004", "status=partial"]);
    assert_eq!(state(&scenario), (json!("partial"), Value::Null));
    scenario.ok(&["set", "WI-004", "status=open"]);
    assert_eq!(state(&scenario), (json!("open"), Value::Null));
}

/// Spec 3.5: an issue that is `in_progress` reads as `open`, so `set
/// status=open` asks for nothing and the issue stays as it is.
#[test]
fn beads_set_status_open_leaves_an_in_progress_issue_alone() {
    let scenario = beads_with_rows(vec![bead("hr-1", &["type:chore"], "in_progress", "")]);
    let before = scenario.bd_issues();
    scenario.ok(&["set", "hr-1", "status=open"]);
    assert_eq!(scenario.bd_issues(), before);
}

/// The issue that `bd` holds under the id that `printed` shows.
fn filed_issue(scenario: &Scenario, printed: &str) -> Value {
    scenario
        .bd_issues()
        .into_iter()
        .find(|issue| issue["id"] == json!(printed.trim()))
        .unwrap_or_else(|| panic!("the store has no issue {printed:?}"))
}

/// Spec 3.5: `defer --title` files an issue with the native type `chore`, the
/// labels `type:chore`, `houserules:deferred`, and `section:<s>`, and a
/// description that holds the reason and the deferral block, byte for byte.
#[test]
fn beads_defer_files_the_issue_with_its_block() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let printed = scenario.ok(&[
        "defer",
        "--title",
        "Cache the lookups",
        "--reason",
        "Needs a design.",
        "--origin",
        "batch 2",
        "--file",
        "src\\cache.rs",
        "--section",
        "E09",
    ]);
    assert!(
        printed.starts_with("hr-") && printed.ends_with('\n') && !printed.trim().contains(' '),
        "got {printed:?}"
    );
    let issue = filed_issue(&scenario, &printed);
    assert_eq!(issue["title"], json!("Cache the lookups"));
    assert_eq!(issue["issue_type"], json!("chore"));
    assert_eq!(issue["status"], json!("open"));
    assert_eq!(
        labels_of(&issue),
        ["houserules:deferred", "section:E09", "type:chore"]
    );
    assert_eq!(
        issue["description"],
        json!(
            "Needs a design.\n\n```houserules\ndeferral: {\"reason\":\"Needs a design.\",\"origin\":\"batch 2\",\"files\":[\"src/cache.rs\"]}\n```"
        )
    );
    assert!(issue.get("dependencies").is_none(), "got {issue}");
}

/// Spec 3.5: a deferral whose origin is a Beads id that the store holds gets a
/// `discovered-from` dependency on it, filed or marked; any other origin gets
/// none, and an origin of the right shape that names no issue is not an error.
#[test]
fn beads_defer_links_an_origin_that_is_a_beads_id() {
    let scenario = Scenario::new(Adapter::Beads, &Fixture::new(seed()));
    let origin = scenario.map["WI-002"].clone();
    let discovered_from = |issue: &Value| -> Vec<String> {
        issue["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|edge| edge["type"] == json!("discovered-from"))
            .filter_map(|edge| edge["depends_on_id"].as_str().map(str::to_string))
            .collect()
    };
    let printed = scenario.ok(&[
        "defer", "--title", "Linked", "--reason", "Later.", "--origin", &origin,
    ]);
    assert_eq!(
        discovered_from(&filed_issue(&scenario, &printed)),
        [origin.as_str()]
    );
    scenario.ok(&[
        "defer", "--item", "WI-004", "--reason", "Wait.", "--origin", &origin,
    ]);
    assert_eq!(discovered_from(&scenario.bd_issue("WI-004")), [origin]);
    for text in ["batch 2", "hr-999"] {
        let printed = scenario.ok(&[
            "defer", "--title", "Unlinked", "--reason", "Later.", "--origin", text,
        ]);
        assert!(
            discovered_from(&filed_issue(&scenario, &printed)).is_empty(),
            "origin {text}"
        );
    }
}

/// Spec 3.5 and 3.7, end to end with the fake `gh`: an `audit` of a range with
/// a `Work-Item:` trailer that names a pull request fails with the line of
/// spec 3.5 and the short sha of the commit, and a trailer that names an
/// issue passes.
#[test]
fn audit_names_a_pull_request_trailer() {
    let scenario = Scenario::new(Adapter::Github, &Fixture::new(Vec::new()));
    scenario.edit_store(|store| {
        let mut pull_request = raw_issue(2, &[], "open", Value::Null, "");
        pull_request["pull_request"] = json!({"url": "https://example.test/pull/2"});
        store["issues"] = json!([raw_issue(1, &[], "open", Value::Null, ""), pull_request]);
    });
    let dir = scenario.dir.path();
    let base = commit_all(dir, "chore: install the kit");
    git(
        dir,
        &[
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "--author",
            "A Human <author@example.com>",
            "-m",
            "feat: add the lookup",
            "-m",
            "Work-Item: #1\nWork-Item: 2\nWork-Item: #2",
        ],
    );
    let sha = git(dir, &["rev-parse", "--short", "HEAD"])
        .trim()
        .to_string();
    let output = scenario.output(&["audit", "--base", &base]);
    let line = |id: &str| {
        format!("{sha}: Work-Item {id} is a pull request, not an issue, in the github tracker\n")
    };
    assert_eq!(
        (output.status.code(), stderr(&output)),
        (Some(1), format!("{}{}", line("2"), line("#2")))
    );
    serde_json::from_str::<Value>(&stdout(&output)).expect("stdout is the audit JSON");
}
