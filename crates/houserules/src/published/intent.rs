//! The intent of a change: what it was meant to do, as a work item, a plan,
//! or a spec workflow records it (spec 4.9). The packet, the change record,
//! and the evidence bundle share these types. They are no document of their
//! own, so no file under `schemas/` describes them alone.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What one adapter found about the intent of a change. A field that the
/// adapter cannot fill is `null`; the keys come in this order. A named intent
/// that does not resolve is an entry with its `source` and `ref`, every other
/// field `null` (`artifacts` empty), and the key `unresolved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Intent {
    /// The name of the adapter, such as `work-item` or `file`.
    pub(crate) source: String,
    /// The id that the adapter gives the intent: a work-item id, a folder, or
    /// a path.
    #[serde(rename = "ref")]
    pub(crate) reference: String,
    /// The title, or `null`.
    pub(crate) title: Option<String>,
    /// Why the change exists, or `null`.
    pub(crate) why: Option<String>,
    /// The counts of the task list, or `null`.
    pub(crate) tasks: Option<Tasks>,
    /// The counts of the requirements, or `null`.
    pub(crate) requirements: Option<Requirements>,
    /// The result of a spec tool's validation, or `null`.
    pub(crate) validation: Option<Validation>,
    /// The paths that the adapter read.
    pub(crate) artifacts: Vec<String>,
    /// The line of the adapter for a named intent that does not resolve. The
    /// key is absent for an intent that resolves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "String")]
    pub(crate) unresolved: Option<String>,
}

/// The counts of a task list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Tasks {
    /// The tasks that are checked.
    pub(crate) done: u64,
    /// All tasks.
    pub(crate) total: u64,
}

/// The counts of the requirements that a change adds, edits, removes, and
/// renames. A count that the source does not give is `null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Requirements {
    /// The requirements that the change adds, or `null`.
    pub(crate) added: Option<u64>,
    /// The requirements that the change modifies, or `null`.
    pub(crate) modified: Option<u64>,
    /// The requirements that the change removes, or `null`.
    pub(crate) removed: Option<u64>,
    /// The requirements that the change renames, or `null`.
    pub(crate) renamed: Option<u64>,
    /// All requirements, or `null`.
    pub(crate) total: Option<u64>,
}

/// The result of a spec tool's validation, read from a file that a caller
/// passes; no adapter runs the tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Validation {
    /// The name of the tool.
    pub(crate) tool: String,
    /// Whether the tool judged the spec valid.
    pub(crate) ok: bool,
    /// The tool's one-line summary.
    pub(crate) summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An intent that fills every field.
    fn full() -> Intent {
        Intent {
            source: "file".to_string(),
            reference: "docs/plans/x.md".to_string(),
            title: Some("A plan".to_string()),
            why: Some("Because.".to_string()),
            tasks: Some(Tasks { done: 1, total: 3 }),
            requirements: Some(Requirements {
                added: Some(2),
                modified: None,
                removed: Some(0),
                renamed: Some(1),
                total: Some(5),
            }),
            validation: Some(Validation {
                tool: "openspec".to_string(),
                ok: true,
                summary: "all valid".to_string(),
            }),
            artifacts: vec!["docs/plans/x.md".to_string()],
            unresolved: None,
        }
    }

    #[test]
    fn an_intent_keeps_the_fields_of_the_contract_in_order() {
        let text = serde_json::to_string(&full()).expect("serialize");

        assert_eq!(
            text,
            concat!(
                r#"{"source":"file","ref":"docs/plans/x.md","title":"A plan","why":"Because.","#,
                r#""tasks":{"done":1,"total":3},"#,
                r#""requirements":{"added":2,"modified":null,"removed":0,"renamed":1,"total":5},"#,
                r#""validation":{"tool":"openspec","ok":true,"summary":"all valid"},"#,
                r#""artifacts":["docs/plans/x.md"]}"#
            )
        );
    }

    #[test]
    fn an_intent_reads_back_unchanged() {
        let intent = full();
        let text = serde_json::to_string(&intent).expect("serialize");

        let back: Intent = serde_json::from_str(&text).expect("deserialize");

        assert_eq!(back, intent);
    }

    #[test]
    fn a_work_item_intent_prints_null_for_every_field_it_leaves_out() {
        let intent = Intent {
            source: "work-item".to_string(),
            reference: "HR-185".to_string(),
            title: None,
            why: None,
            tasks: None,
            requirements: None,
            validation: None,
            artifacts: Vec::new(),
            unresolved: None,
        };

        let text = serde_json::to_string(&intent).expect("serialize");

        assert_eq!(
            text,
            concat!(
                r#"{"source":"work-item","ref":"HR-185","title":null,"why":null,"#,
                r#""tasks":null,"requirements":null,"validation":null,"artifacts":[]}"#
            )
        );
        let back: Intent = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, intent);
    }

    #[test]
    fn an_unresolved_intent_carries_the_line_of_its_adapter_in_a_last_key() {
        let intent = Intent {
            source: "file".to_string(),
            reference: "docs/plans/missing.md".to_string(),
            title: None,
            why: None,
            tasks: None,
            requirements: None,
            validation: None,
            artifacts: Vec::new(),
            unresolved: Some("no regular file at that path at the head".to_string()),
        };

        let text = serde_json::to_string(&intent).expect("serialize");

        assert_eq!(
            text,
            concat!(
                r#"{"source":"file","ref":"docs/plans/missing.md","title":null,"why":null,"#,
                r#""tasks":null,"requirements":null,"validation":null,"artifacts":[],"#,
                r#""unresolved":"no regular file at that path at the head"}"#
            )
        );
        let back: Intent = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back, intent);
    }

    #[test]
    fn an_intent_that_resolves_carries_no_unresolved_key() {
        let text = serde_json::to_string(&full()).expect("serialize");

        assert!(!text.contains("unresolved"), "{text}");
        let back: Intent = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(back.unresolved, None);
    }
}
