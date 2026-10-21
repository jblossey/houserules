//! What `set` and `defer` ask of a tracker, read and checked once for every
//! adapter (spec 3.5, C30). [`parse_set`] reads the `field=value` words of
//! `set` and [`check_defer_request`] checks a [`DeferRequest`]. An adapter
//! calls them after it has found the item and before its first write, so a
//! fault is the same line, in the same order, on every adapter. A fault is the
//! line to print; [`super::lines`] holds each one.

use serde_json::{Map, Value, json};

use super::lines;
use crate::backlog::ItemStatus;

/// The longest title of a work item, in characters.
const TITLE_LIMIT: usize = 120;

/// The section that a new deferral goes to when the request names none.
const DEFAULT_SECTION: &str = "general";

/// The statuses that `set status=<s>` takes, in the order of the backlog
/// schema.
const STATUSES: [ItemStatus; 4] = [
    ItemStatus::Open,
    ItemStatus::Partial,
    ItemStatus::Done,
    ItemStatus::Dropped,
];

/// One change of `set`, read from one `field=value` word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// `status=<s>`.
    Status(ItemStatus),
    /// `batch=<n>`: a positive integer.
    Batch(i64),
    /// `milestone=<m>`: 1 to 16 letters, digits, `.`, `/`, and `-`.
    Milestone(String),
}

/// What `defer` asks the tracker for (spec 3.1). Exactly one of `item` and
/// `title` is set: `item` marks an existing item, `title` files a new one.
pub(crate) struct DeferRequest {
    /// The id of an existing item to mark as a deferral.
    pub(crate) item: Option<String>,
    /// The title of a new item.
    pub(crate) title: Option<String>,
    /// Why the work waits.
    pub(crate) reason: String,
    /// The reference that names where the deferral came from.
    pub(crate) origin: String,
    /// The files the deferral concerns, `/`-separated.
    pub(crate) files: Vec<String>,
    /// The section a new item goes to, when the request names one.
    pub(crate) section: Option<String>,
}

impl DeferRequest {
    /// The `deferral` object that the request writes: its `reason`, its
    /// `origin`, and its `files` with `/` for a `\`.
    pub(crate) fn deferral(&self) -> Value {
        let files: Vec<String> = self
            .files
            .iter()
            .map(|file| file.replace('\\', "/"))
            .collect();
        json!({"reason": self.reason, "origin": self.origin, "files": files})
    }
}

/// What a checked [`DeferRequest`] asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DeferTarget<'a> {
    /// Mark the existing item with this id.
    Item(&'a str),
    /// File a new item with this title, in this section.
    New {
        /// The title, 1 to 120 characters.
        title: &'a str,
        /// The section: letters, digits, and hyphens.
        section: &'a str,
    },
}

/// The `field=value` words of `set` as changes, in order. The value of a word
/// is all the text after its first `=`. The first word that holds a fault
/// decides, and its fault is the line to print.
pub(crate) fn parse_set(assignments: &[String]) -> Result<Vec<Change>, String> {
    assignments
        .iter()
        .map(|assignment| parse_assignment(assignment))
        .collect()
}

/// One `field=value` word of `set` as a change.
fn parse_assignment(assignment: &str) -> Result<Change, String> {
    let (field, value) = match assignment.split_once('=') {
        Some((field, value)) => (field, Some(value)),
        None => (assignment, None),
    };
    match field {
        "status" => value
            .and_then(|value| serde_json::from_value::<ItemStatus>(json!(value)).ok())
            .map(Change::Status)
            .ok_or_else(|| {
                let names: Vec<String> =
                    STATUSES.iter().map(|status| status_name(*status)).collect();
                lines::status_must_be(&names.join(", "))
            }),
        "batch" => value
            .filter(|value| is_positive_integer(value))
            .and_then(|value| value.parse().ok())
            .map(Change::Batch)
            .ok_or_else(|| lines::BATCH_NOT_POSITIVE.to_string()),
        "milestone" => value
            .filter(|value| is_milestone(value))
            .map(|value| Change::Milestone(value.to_string()))
            .ok_or_else(|| lines::MILESTONE_FORMAT.to_string()),
        _ => Err(lines::unknown_field(field)),
    }
}

/// The lowercase schema name of `status`, through the `Serialize` derive of
/// [`ItemStatus`], so that no second table of names can drift from the schema.
pub(crate) fn status_name(status: ItemStatus) -> String {
    match serde_json::to_value(status).expect("ItemStatus always serializes") {
        Value::String(name) => name,
        other => unreachable!("ItemStatus serializes as a JSON string, got {other:?}"),
    }
}

/// `true` for one or more ASCII digits, the first not `0`.
fn is_positive_integer(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_digit() && first != '0' => chars.all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// `true` when `value` is a milestone of the backlog schema: 1 to 16 letters,
/// digits, dots, slashes, and hyphens.
fn is_milestone(value: &str) -> bool {
    (1..=16).contains(&value.chars().count())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '-'))
}

/// The `changes` object that `set` returns: each field with the value it was
/// given, a later change replacing an earlier one of the same field in place.
pub(crate) fn changes_map(changes: &[Change]) -> Map<String, Value> {
    let mut map = Map::new();
    for change in changes {
        let (key, value) = match change {
            Change::Status(status) => ("status", Value::String(status_name(*status))),
            Change::Batch(batch) => ("batch", Value::Number((*batch).into())),
            Change::Milestone(milestone) => ("milestone", Value::String(milestone.clone())),
        };
        map.insert(key.to_string(), value);
    }
    map
}

/// Checks `request` and names its target. The faults come in this order: an
/// empty reason, an empty origin, a request that names both or neither of
/// `item` and `title`, an empty title, a title of more than 120 characters,
/// and a section with a character outside letters, digits, and hyphens.
pub(crate) fn check_defer_request(request: &DeferRequest) -> Result<DeferTarget<'_>, String> {
    require_text("reason", &request.reason)?;
    require_text("origin", &request.origin)?;
    match (&request.item, &request.title) {
        (Some(id), None) => Ok(DeferTarget::Item(id)),
        (None, Some(title)) => {
            require_text("title", title)?;
            let length = title.chars().count();
            if length > TITLE_LIMIT {
                return Err(lines::title_too_long(length));
            }
            let section = request.section.as_deref().unwrap_or(DEFAULT_SECTION);
            if section.is_empty()
                || !section
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err(lines::section_malformed(section));
            }
            Ok(DeferTarget::New { title, section })
        }
        _ => Err(lines::DEFER_NEEDS_ONE_TARGET.to_string()),
    }
}

/// Refuses a `text` that says nothing; `what` names it.
fn require_text(what: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(lines::defer_text_empty(what));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The words of `items` as `set` receives them.
    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    /// The fault that `set` finds in the one assignment `item`.
    fn refused(item: &str) -> String {
        parse_set(&words(&[item])).expect_err("refused")
    }

    /// A request that files a new item titled `title`.
    fn filing(title: &str) -> DeferRequest {
        DeferRequest {
            item: None,
            title: Some(title.to_string()),
            reason: "Needs a design.".to_string(),
            origin: "batch 1".to_string(),
            files: Vec::new(),
            section: None,
        }
    }

    /// `set` takes status, batch, and milestone, in the order given, and the
    /// changes object keeps one entry for each field: a later assignment
    /// replaces an earlier one in place.
    #[test]
    fn a_set_takes_status_batch_and_milestone_in_order() {
        let changes = parse_set(&words(&[
            "status=done",
            "batch=3",
            "milestone=v2",
            "status=open",
        ]))
        .expect("parsed");
        assert_eq!(
            changes,
            [
                Change::Status(ItemStatus::Done),
                Change::Batch(3),
                Change::Milestone("v2".to_string()),
                Change::Status(ItemStatus::Open),
            ]
        );
        let map = changes_map(&changes);
        assert_eq!(
            serde_json::Value::Object(map.clone()),
            json!({"status": "open", "batch": 3, "milestone": "v2"})
        );
        assert_eq!(
            map.keys().map(String::as_str).collect::<Vec<_>>(),
            ["status", "batch", "milestone"]
        );
    }

    /// The value of an assignment is all the text after its first `=`, so a
    /// second `=` is part of the value and the field's own rule refuses it.
    #[test]
    fn a_second_equals_sign_is_part_of_the_value() {
        assert_eq!(refused("status=done=x"), refused("status=x"));
        assert_eq!(refused("batch=3=4"), lines::BATCH_NOT_POSITIVE);
        assert_eq!(refused("milestone=v2=x"), lines::MILESTONE_FORMAT);
    }

    /// The refusals of `set` are the lines of the contract, and the first
    /// faulty assignment decides.
    #[test]
    fn the_assignments_of_set_are_refused_with_the_lines_of_the_contract() {
        assert_eq!(refused("colour=red"), "unknown field \"colour\"");
        assert_eq!(refused("colour"), "unknown field \"colour\"");
        assert_eq!(refused("=done"), "unknown field \"\"");
        for bad in ["status=nearly", "status", "status=", "status=Done"] {
            assert_eq!(
                refused(bad),
                "status must be one of open, partial, done, dropped",
                "{bad}"
            );
        }
        for bad in [
            "batch=0",
            "batch=one",
            "batch=",
            "batch",
            "batch=-1",
            "batch=+1",
            "batch=01",
            "batch=9223372036854775808",
        ] {
            assert_eq!(refused(bad), "batch must be a positive integer", "{bad}");
        }
        for bad in ["milestone=", "milestone", "milestone=a b", "milestone=é"] {
            assert_eq!(
                refused(bad),
                "milestone must be 1 to 16 characters from A-Z, a-z, 0-9, ., /, and -",
                "{bad}"
            );
        }
        assert_eq!(
            parse_set(&words(&["status=done", "colour=red", "batch=0"])),
            Err("unknown field \"colour\"".to_string())
        );
    }

    /// A batch is a positive `i64`, and a milestone is 1 to 16 characters of
    /// the schema's set.
    #[test]
    fn the_limits_of_batch_and_milestone_are_those_of_the_backlog_schema() {
        assert_eq!(
            parse_set(&words(&["batch=9223372036854775807"])),
            Ok(vec![Change::Batch(i64::MAX)])
        );
        for good in ["v1", "1.2/x-y", "A", "0123456789abcdef"] {
            assert_eq!(
                parse_set(&words(&[&format!("milestone={good}")])),
                Ok(vec![Change::Milestone(good.to_string())]),
                "{good}"
            );
        }
        assert!(parse_set(&words(&["milestone=0123456789abcdefg"])).is_err());
    }

    /// An item request marks that item; a title request files a new one, in the
    /// default section unless it names one; each fault is its line, checked in
    /// the order reason, origin, target, title, section.
    #[test]
    fn a_defer_request_names_one_target_and_each_fault_is_its_line() {
        let mut marking = filing("unused");
        marking.title = None;
        marking.item = Some("WI-002".to_string());
        assert_eq!(
            check_defer_request(&marking).expect("an item"),
            DeferTarget::Item("WI-002")
        );
        assert_eq!(
            check_defer_request(&filing("Cache it")).expect("a title"),
            DeferTarget::New {
                title: "Cache it",
                section: "general"
            }
        );
        let mut sectioned = filing("Cache it");
        sectioned.section = Some("E01".to_string());
        assert_eq!(
            check_defer_request(&sectioned).expect("a title"),
            DeferTarget::New {
                title: "Cache it",
                section: "E01"
            }
        );
        assert_eq!(
            check_defer_request(&filing(&"é".repeat(120))).map(|_| ()),
            Ok(())
        );

        let mut blank_reason = filing("t");
        blank_reason.reason = "  ".to_string();
        blank_reason.origin = String::new();
        assert_eq!(
            check_defer_request(&blank_reason).map(|_| ()),
            Err("defer: the reason is empty".to_string())
        );
        let mut blank_origin = filing("t");
        blank_origin.origin = String::new();
        assert_eq!(
            check_defer_request(&blank_origin).map(|_| ()),
            Err("defer: the origin is empty".to_string())
        );
        let mut both = filing("t");
        both.item = Some("WI-002".to_string());
        let mut neither = filing("t");
        neither.title = None;
        for request in [both, neither] {
            assert_eq!(
                check_defer_request(&request).map(|_| ()),
                Err("defer needs exactly one of --title and --item".to_string())
            );
        }
        assert_eq!(
            check_defer_request(&filing(" ")).map(|_| ()),
            Err("defer: the title is empty".to_string())
        );
        assert_eq!(
            check_defer_request(&filing(&"é".repeat(121))).map(|_| ()),
            Err("defer: the title is 121 characters; the limit is 120".to_string())
        );
        for section in ["../x", "a/b", "", "a b", "é"] {
            let mut request = filing("t");
            request.section = Some(section.to_string());
            assert_eq!(
                check_defer_request(&request).map(|_| ()),
                Err(format!(
                    "defer: the section \"{section}\" must hold only letters, digits, and hyphens"
                )),
                "{section:?}"
            );
        }
    }

    /// The deferral object holds the reason, the origin, and the files with
    /// forward slashes.
    #[test]
    fn the_deferral_object_holds_files_with_forward_slashes() {
        let mut request = filing("t");
        request.files = vec!["a\\b.rs".to_string(), "c/d.rs".to_string()];
        assert_eq!(
            request.deferral(),
            json!({
                "reason": "Needs a design.",
                "origin": "batch 1",
                "files": ["a/b.rs", "c/d.rs"],
            })
        );
    }
}
