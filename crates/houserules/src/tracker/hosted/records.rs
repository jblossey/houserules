//! The JSON shapes that the hosted adapters build from issues (spec 3.5, C30):
//! the item, amendment, and parked records that `get` prints, the batch records
//! and the backlog view that the JSON adapter's own code reads, and what the
//! tracker says about an id (`resolve`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

use serde_json::{Map, Value, json};

use super::{DEFERRED, HostedIssue, Kind};
use crate::backlog::LoadedBacklog;
use crate::tracker::{Deferral, Resolved, TrackerError, block};

/// The paragraphs of `prose`: blocks of lines that blank lines separate.
fn paragraphs(prose: &str) -> Vec<Value> {
    let mut found: Vec<Vec<&str>> = vec![Vec::new()];
    for line in prose.lines() {
        if line.trim().is_empty() {
            if found.last().is_some_and(|paragraph| !paragraph.is_empty()) {
                found.push(Vec::new());
            }
        } else if let Some(paragraph) = found.last_mut() {
            paragraph.push(line);
        }
    }
    found
        .into_iter()
        .filter(|paragraph| !paragraph.is_empty())
        .map(|paragraph| json!(paragraph.join("\n")))
        .collect()
}

/// The `deferral` object of an issue that has the label: `reason`, `origin`,
/// and `files` from its block, each absent part as an empty one.
pub(super) fn deferral_value(fields: &BTreeMap<String, Value>) -> Value {
    let part = |key: &str| fields.get("deferral").map(|deferral| &deferral[key]);
    let text = |key: &str| match part(key) {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    };
    let files = match part("files") {
        Some(Value::Array(files)) => files.clone(),
        _ => Vec::new(),
    };
    json!({"reason": text("reason"), "origin": text("origin"), "files": files})
}

/// The record of an item, or, with `typed` false, of any other issue: the
/// 1.4.0 item shape with a `null` type, section, and milestone.
fn item_value<T: HostedIssue>(issue: &T, typed: bool) -> Value {
    let (prose, fields) = block::parse(issue.text());
    let mut map = Map::new();
    map.insert("id".to_string(), json!(issue.id()));
    let item_type = issue.item_type().filter(|_| typed);
    map.insert("type".to_string(), json!(item_type));
    let milestone = issue.milestone().filter(|_| typed);
    map.insert("milestone".to_string(), json!(milestone));
    if let Some(label) = fields.get("label") {
        map.insert("label".to_string(), label.clone());
    }
    map.insert("status".to_string(), json!(issue.item_status()));
    map.insert("title".to_string(), json!(issue.title()));
    map.insert("body".to_string(), Value::Array(paragraphs(&prose)));
    for key in ["extra_lines", "rulings"] {
        if let Some(value) = fields.get(key) {
            map.insert(key.to_string(), value.clone());
        }
    }
    map.insert("batch".to_string(), json!(issue.batch()));
    let tags: Vec<&str> = issue.label_values("tag:").collect();
    if !tags.is_empty() {
        map.insert("tags".to_string(), json!(tags));
    }
    if let Some(see) = issue.see() {
        map.insert("see".to_string(), see);
    }
    if issue.has_label(DEFERRED) {
        map.insert("deferral".to_string(), deferral_value(&fields));
    }
    let section = issue.label_values("section:").next().filter(|_| typed);
    map.insert("section".to_string(), json!(section));
    Value::Object(map)
}

/// The record of an amendment: its type, label, and date from the block, its
/// status from the state of the issue, and its text from the issue text.
fn amendment_value<T: HostedIssue>(issue: &T) -> Value {
    let (prose, fields) = block::parse(issue.text());
    let mut map = Map::new();
    map.insert("id".to_string(), json!(issue.id()));
    for key in ["type", "label", "date"] {
        if let Some(value) = fields.get(key) {
            map.insert(key.to_string(), value.clone());
        }
    }
    let status = if issue.closed() { "done" } else { "open" };
    map.insert("status".to_string(), json!(status));
    map.insert("text".to_string(), Value::Array(paragraphs(&prose)));
    Value::Object(map)
}

/// The parked item of `issue` as the JSON file holds it: its text and its
/// `resolved` note from the block, under the id of the issue.
fn parked_item<T: HostedIssue>(issue: &T) -> Map<String, Value> {
    let (prose, fields) = block::parse(issue.text());
    let mut map = Map::new();
    map.insert("id".to_string(), json!(issue.id()));
    map.insert("text".to_string(), json!(prose));
    if let Some(resolved) = fields.get("resolved") {
        map.insert("resolved".to_string(), resolved.clone());
    }
    map
}

/// The record of a parked item: its text, its `resolved` note from the block,
/// and the batch of its group.
fn parked_value<T: HostedIssue>(issue: &T) -> Value {
    let mut map = parked_item(issue);
    map.insert("batch".to_string(), json!(issue.batch()));
    Value::Object(map)
}

/// The record of a parked group (spec 3.5, C30), in the form of
/// `backlog/parked.json` under the id of the issue: its batch, its intro (the
/// issue text), and its items, the parked items of that batch in the order of
/// `issues`. A group with no batch has no item.
fn group_value<T: HostedIssue>(issue: &T, issues: &[T]) -> Value {
    let (intro, _) = block::parse(issue.text());
    let items: Vec<Value> = issues
        .iter()
        .filter(|other| {
            !other.is_pull_request()
                && other.kind() == Kind::Parked
                && issue
                    .batch()
                    .is_some_and(|batch| other.batch() == Some(batch))
        })
        .map(|other| Value::Object(parked_item(other)))
        .collect();
    json!({
        "id": issue.id(),
        "batch": issue.batch(),
        "intro": intro,
        "items": items,
    })
}

/// The record that `get` prints for `issue`, one of `issues`.
fn record_value<T: HostedIssue>(issue: &T, issues: &[T]) -> Value {
    match issue.kind() {
        Kind::Amendment => amendment_value(issue),
        Kind::Parked => parked_value(issue),
        Kind::Group => group_value(issue, issues),
        Kind::Item => item_value(issue, true),
        Kind::Batch | Kind::Other => item_value(issue, false),
    }
}

/// The record that `get` prints for each of `ids`, in order. `named` finds the
/// issue that an id names, and fails for an id that names none; `issues` reads
/// every issue, which the record of a parked group lists.
pub(in crate::tracker) fn records_of<T: HostedIssue>(
    ids: &[String],
    named: impl Fn(&str) -> Result<T, TrackerError>,
    issues: impl Fn() -> Result<Rc<Vec<T>>, TrackerError>,
) -> Result<Vec<Value>, TrackerError> {
    ids.iter()
        .map(|id| Ok(record_value(&named(id)?, &issues()?)))
        .collect()
}

/// The number and the summary of a batch record, from its title
/// `Batch <n>: <summary>`.
pub(super) fn batch_title<T: HostedIssue>(issue: &T) -> Option<(u64, String)> {
    let (number, summary) = issue.title().strip_prefix("Batch ")?.split_once(':')?;
    let number = number.parse().ok().filter(|number| *number > 0)?;
    Some((number, summary.trim().to_string()))
}

/// The `status` of a batch record: the `state` and `text` of its block, else
/// `done` when its issue is closed and `planned` when it is open.
pub(super) fn batch_status<T: HostedIssue>(issue: &T, fields: &BTreeMap<String, Value>) -> Value {
    let state = fields
        .get("status")
        .and_then(|status| status["state"].as_str())
        .filter(|state| ["planned", "in-progress", "done"].contains(state));
    match (state, fields.get("status")) {
        (Some(state), Some(status)) => {
            json!({"state": state, "text": status["text"].as_str().unwrap_or_default()})
        }
        _ => json!({"state": if issue.closed() { "done" } else { "planned" }, "text": ""}),
    }
}

/// The batch records of `issues`, in the form of `backlog/batches.json`, with
/// the items of each batch from the `batch:<n>` labels of `items`.
fn batches_value<T: HostedIssue>(issues: &[T], items: &[(String, Value)]) -> Value {
    let batches: Vec<Value> = issues
        .iter()
        .filter(|issue| !issue.is_pull_request() && issue.kind() == Kind::Batch)
        .filter_map(|issue| {
            let (number, summary) = batch_title(issue)?;
            let (_, fields) = block::parse(issue.text());
            let members: Vec<Value> = items
                .iter()
                .filter(|(_, item)| item["batch"] == json!(number))
                .map(|(id, _)| json!(id))
                .collect();
            Some(json!({
                "number": number,
                "items": members,
                "summary": summary,
                "kickoff": fields.get("kickoff").and_then(Value::as_str).unwrap_or_default(),
                "status": batch_status(issue, &fields),
            }))
        })
        .collect();
    json!({ "batches": batches })
}

/// The backlog of `issues` in the shape that the JSON adapter's code reads:
/// the items and the batches. The files of a JSON backlog have no part in it.
pub(in crate::tracker) fn backlog_view<T: HostedIssue>(issues: &[T]) -> LoadedBacklog {
    let items: Vec<(String, Value)> = issues
        .iter()
        .filter(|issue| issue.is_item())
        .map(|issue| (issue.id(), item_value(issue, true)))
        .collect();
    LoadedBacklog {
        root: PathBuf::new(),
        schema: Value::Null,
        amendments: Value::Null,
        batches: batches_value(issues, &items),
        decisions: Value::Null,
        parked: Value::Null,
        sections: Vec::new(),
        items,
    }
}

/// What `resolve` says about the issue `issue` that the tracker holds: it is a
/// work item whatever its labels, and the gates read its deferral reason.
pub(in crate::tracker) fn resolved<T: HostedIssue>(issue: &T) -> Resolved {
    Resolved {
        deferral: issue.has_label(DEFERRED).then(|| {
            let value = deferral_value(&block::parse(issue.text()).1);
            Deferral {
                reason: value["reason"].as_str().unwrap_or_default().to_string(),
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{TestIssue, closed, issue, marked};
    use super::*;

    /// A text is split into paragraphs on blank lines, and blank lines of any
    /// length separate.
    #[test]
    fn paragraphs_split_on_blank_lines() {
        assert_eq!(
            paragraphs("One\nstill one.\n\n\n  \nTwo."),
            vec![json!("One\nstill one."), json!("Two.")]
        );
        assert_eq!(paragraphs(""), Vec::<Value>::new());
    }

    /// Spec 3.5: an issue with no marker and no `type:` label reads with the
    /// item shape and a `null` type, section, and milestone, and keeps its
    /// batch.
    #[test]
    fn an_unlabelled_issue_reads_with_the_item_shape() {
        let mut plain = issue("hr-4", &["bug", "batch:2", "section:E01"], "Text.");
        plain.milestone = Some("v1".to_string());
        assert_eq!(
            record_value(&plain, &[]),
            json!({
                "id": "hr-4", "type": null, "milestone": null, "status": "open",
                "title": "Issue hr-4", "body": ["Text."], "batch": 2, "section": null,
            })
        );
    }

    /// Spec 3.5: an item reads its type, section, milestone, tags, and batch
    /// from its labels, its `label`, `rulings`, and deferral from its block,
    /// and its `see` list from the adapter.
    #[test]
    fn an_item_reads_its_labels_its_block_and_its_see_list() {
        let mut item = issue(
            "hr-2",
            &[
                "type:feat",
                "section:E01",
                "batch:1",
                "tag:alpha",
                "tag:beta",
                DEFERRED,
            ],
            "First.\n\nSecond.\n\n```houserules\nlabel: Core\ndeferral: {\"reason\":\"Why.\",\"origin\":\"batch 1\",\"files\":[\"a.rs\"]}\n```",
        );
        item.milestone = Some("v1".to_string());
        item.see = Some(json!(["hr-3", "hr-9"]));
        assert_eq!(
            record_value(&item, &[]),
            json!({
                "id": "hr-2", "type": "feat", "milestone": "v1", "label": "Core",
                "status": "open", "title": "Issue hr-2", "body": ["First.", "Second."],
                "batch": 1, "tags": ["alpha", "beta"], "see": ["hr-3", "hr-9"],
                "deferral": {"reason": "Why.", "origin": "batch 1", "files": ["a.rs"]},
                "section": "E01",
            })
        );
    }

    /// Spec 3.5, C30: a parked group reads with its batch, its intro, and its
    /// items, the parked items of its batch, in the shape that the JSON file
    /// holds; a parked item reads as before, and a group with no batch has no
    /// item.
    #[test]
    fn a_parked_group_reads_with_its_intro_and_the_parked_items_of_its_batch() {
        let group_block = "Intro.\n\n```houserules\ngroup: true\n```";
        let parked = |id: &str, labels: &[&str], text: &str| marked(Kind::Parked, id, labels, text);
        let resolved = closed(parked(
            "hr-3",
            &["batch:1"],
            "Second.\n\n```houserules\nresolved: {\"batch\":2,\"note\":\"Done.\"}\n```",
        ));
        let issues = [
            parked("hr-1", &["batch:1"], group_block),
            parked("hr-2", &["batch:1"], "First."),
            resolved,
            parked("hr-4", &["batch:2"], "Other batch."),
            issue("hr-5", &["type:chore", "batch:1"], "An item."),
            parked("hr-6", &[], group_block),
            parked("hr-7", &[], "No batch."),
        ];
        assert_eq!(
            record_value(&issues[0], &issues),
            json!({
                "id": "hr-1", "batch": 1, "intro": "Intro.",
                "items": [
                    {"id": "hr-2", "text": "First."},
                    {"id": "hr-3", "text": "Second.", "resolved": {"batch": 2, "note": "Done."}},
                ],
            })
        );
        assert_eq!(
            record_value(&issues[1], &issues),
            json!({"id": "hr-2", "text": "First.", "batch": 1})
        );
        assert_eq!(
            record_value(&issues[5], &issues),
            json!({"id": "hr-6", "batch": null, "intro": "Intro.", "items": []})
        );
    }

    /// A pull request is no parked item of a group.
    #[test]
    fn a_pull_request_is_no_item_of_a_parked_group() {
        let group = marked(
            Kind::Parked,
            "hr-1",
            &["batch:1"],
            "Intro.\n\n```houserules\ngroup: true\n```",
        );
        let mut pull_request = marked(Kind::Parked, "hr-2", &["batch:1"], "A pull request.");
        pull_request.pull_request = true;
        let issues = [group, pull_request];
        assert_eq!(record_value(&issues[0], &issues)["items"], json!([]));
    }

    /// An amendment reads its block and its text, and is `done` when closed.
    #[test]
    fn an_amendment_is_done_when_its_issue_is_closed() {
        let amendment = marked(
            Kind::Amendment,
            "hr-1",
            &[],
            "Stay in scope.\n\n```houserules\ntype: constraint\nlabel: Scope\ndate: 2026-10-02\n```",
        );
        assert_eq!(
            record_value(&amendment, &[]),
            json!({
                "id": "hr-1", "type": "constraint", "label": "Scope", "date": "2026-10-02",
                "status": "open", "text": ["Stay in scope."],
            })
        );
        assert_eq!(
            record_value(&closed(amendment), &[])["status"],
            json!("done")
        );
    }

    /// The backlog that the JSON code reads holds the items and no pull
    /// request, and each batch record lists the items that carry its label.
    #[test]
    fn the_backlog_view_holds_the_items_and_the_batches() {
        let mut batch = marked(Kind::Batch, "hr-3", &[], "");
        batch.title = "Batch 1: One".to_string();
        let mut pull_request = issue("hr-5", &["type:feat", "batch:1"], "");
        pull_request.pull_request = true;
        let issues: [TestIssue; 5] = [
            issue("hr-1", &["type:chore", "batch:1"], ""),
            marked(Kind::Amendment, "hr-2", &["type:feat"], ""),
            batch,
            issue("hr-4", &["bug"], ""),
            pull_request,
        ];
        let view = backlog_view(&issues);
        let ids: Vec<&str> = view.items.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["hr-1"]);
        assert_eq!(
            view.batches,
            json!({"batches": [{
                "number": 1, "items": ["hr-1"], "summary": "One", "kickoff": "",
                "status": {"state": "planned", "text": ""},
            }]})
        );
    }

    /// A batch title names its number and its summary; anything else names
    /// neither.
    #[test]
    fn a_batch_title_names_its_number_and_its_summary() {
        let titled = |title: &str| {
            let mut batch = marked(Kind::Batch, "hr-1", &[], "");
            batch.title = title.to_string();
            batch_title(&batch)
        };
        assert_eq!(
            titled("Batch 12:  The summary "),
            Some((12, "The summary".to_string()))
        );
        for bad in [
            "Batch 0: x",
            "Batch x: y",
            "Batch 1",
            "Nonsense",
            "batch 1: x",
        ] {
            assert_eq!(titled(bad), None, "{bad}");
        }
    }

    /// The status of a batch record: its block first, else its state.
    #[test]
    fn a_batch_status_is_its_block_or_its_state() {
        let batch = marked(Kind::Batch, "hr-1", &[], "");
        let planned = json!({"state": "planned", "text": ""});
        assert_eq!(batch_status(&batch, &BTreeMap::new()), planned);
        assert_eq!(
            batch_status(&closed(batch), &BTreeMap::new()),
            json!({"state": "done", "text": ""})
        );
        let batch = marked(Kind::Batch, "hr-2", &[], "");
        let fields = BTreeMap::from([(
            "status".to_string(),
            json!({"state": "in-progress", "text": "Half way."}),
        )]);
        assert_eq!(
            batch_status(&batch, &fields),
            json!({"state": "in-progress", "text": "Half way."})
        );
        let unknown = BTreeMap::from([("status".to_string(), json!({"state": "stalled"}))]);
        assert_eq!(batch_status(&batch, &unknown), planned);
    }

    /// Spec 3.5 and 3.7: every issue is a work item that a trailer may name,
    /// whatever its labels; a deferral carries its reason (C31).
    #[test]
    fn any_issue_resolves_as_a_work_item_and_a_deferral_carries_its_reason() {
        let block = "Why.\n\n```houserules\ndeferral: {\"reason\":\"Why.\",\"origin\":\"batch 2\",\"files\":[\"a.rs\"]}\n```";
        assert!(resolved(&issue("hr-1", &[], "")).deferral.is_none());
        let found = resolved(&closed(issue("hr-3", &[DEFERRED], block)));
        assert_eq!(found.deferral.expect("a deferral").reason, "Why.");
    }
}
