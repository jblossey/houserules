//! The `rule-package` document: the output of `houserules for` and
//! `houserules for --full`.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::Serialize;
use serde_json::Value;

use super::Published;

/// The rules that a set of changed paths pulls in. `for` prints it with one
/// index row per entry; `for --full` prints each stored entry whole.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[schemars(title = "Rule package")]
pub(crate) struct RulePackage {
    /// The schema id, `houserules/rule-package@1.0.0`.
    pub(crate) schema: String,
    /// The paths asked for, each without a leading `./`.
    pub(crate) paths: Vec<String>,
    /// The areas that the paths resolve to, sorted.
    pub(crate) areas: Vec<String>,
    /// The entries that apply to the paths, sorted by id.
    pub(crate) entries: Vec<RuleEntry>,
    /// The command that prints the standing rules.
    pub(crate) standing: String,
}

impl Published for RulePackage {
    const NAME: &'static str = "rule-package";
    const VERSION: &'static str = "1.0.0";
}

/// One entry of a package: an index row, or with `--full` the stored entry
/// whole, with its keys in the order of its topic file. The entry stays a
/// `serde_json::Value`, because the topic files order keys differently and
/// the print keeps each order.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct RuleEntry(pub(crate) Value);

impl JsonSchema for RuleEntry {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("RuleEntry")
    }

    /// An object that requires `id`, `kind`, `area`, and `summary` and allows
    /// every other key. An index row always holds `standing`; a stored entry
    /// may lack it.
    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "An index row, or with --full the stored entry whole.",
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "kind": { "type": "string" },
                "area": { "type": "string" },
                "standing": { "type": "boolean" },
                "summary": { "type": "string" }
            },
            "required": ["id", "kind", "area", "summary"]
        })
    }
}
