//! The validation capture of `evidence` and `record write`: the result of
//! `openspec validate --json` as a capture file, and the `validation` that it gives each
//! `openspec` intent.
//!
//! [`fill`] reads the capture that `--validation` names. The header of the capture binds it:
//! the run must have been at the head of the range, in a git work tree that was clean. The
//! output must be one JSON document of OpenSpec's `validate --json` form: the string `"1.0"` as
//! `version`, and `items`, each an object with a string `id` and `type`, a boolean `valid`, and
//! an array `issues`. Each `openspec` intent that resolved takes the item whose `type` is
//! `change` and whose `id` is its `ref`; an item of another type (a capability of the same id)
//! is another item. Every other intent, and an intent with no item, keeps `validation: null`.
//!
//! A capture that cannot be used is no error: [`fill`] gives the line of spec 4.1 for stderr and
//! leaves every `validation` as it is, so the command exits as it does without the flag. The text
//! that the capture holds (the reason of its reader and the head of its header) prints through
//! `review::text`, so that it stays on one line.

use std::fs;
use std::path::Path;

use serde_json::Value;

use super::git;
use super::text::escaped_line;
use crate::capture::{self, Capture};
use crate::install::marker::review::Adapter;
use crate::published::intent::{Intent, Validation};

/// The reason for a document that is no `openspec validate` document.
const NOT_A_DOCUMENT: &str = "is no openspec validate document (version \"1.0\" with items)";

/// The version of the document that `openspec validate --json` prints.
const DOCUMENT_VERSION: &str = "1.0";

/// The type of an item that stands for a change.
const CHANGE_TYPE: &str = "change";

/// One item of the document: a change or a capability that OpenSpec validated.
struct Item {
    /// The id of the change or of the capability.
    id: String,
    /// The `type` of the item: `change` or `spec`.
    kind: String,
    /// Whether OpenSpec judged the item valid.
    valid: bool,
    /// The number of issues of the item, of every level.
    issues: usize,
}

impl Item {
    /// The item that `value` stands for, or `None` for a value of another shape.
    fn read(value: &Value) -> Option<Self> {
        Some(Self {
            id: value.get("id")?.as_str()?.to_string(),
            kind: value.get("type")?.as_str()?.to_string(),
            valid: value.get("valid")?.as_bool()?,
            issues: value.get("issues")?.as_array()?.len(),
        })
    }

    /// The validation that the item gives its change: the verdict and the count of issues.
    fn validation(&self) -> Validation {
        let summary = match self.issues {
            0 => "no issues".to_string(),
            1 => "1 issue".to_string(),
            count => format!("{count} issues"),
        };
        Validation {
            tool: Adapter::OpenSpec.name().to_string(),
            ok: self.valid,
            summary,
        }
    }
}

/// The items of the document `value`, or `None` when `value` is no document of
/// `openspec validate --json`.
fn items_of(value: &Value) -> Option<Vec<Item>> {
    if value.get("version")?.as_str()? != DOCUMENT_VERSION {
        return None;
    }
    value
        .get("items")?
        .as_array()?
        .iter()
        .map(Item::read)
        .collect()
}

/// The items of the capture at `path`, or the reason of spec 4.1 that the capture is refused:
/// `head` is the full id of the commit of the range, and the run of the capture must have been
/// at that commit in a clean work tree.
fn read(path: &Path, head: &str) -> Result<Vec<Item>, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot be read ({})", capture::os_error_text(&error)))?;
    let header = capture::parse_header(&bytes)
        .map_err(|reason| format!("is no capture ({})", escaped_line(&reason)))?;
    let capture = Capture { header, bytes };
    let (Some(ran_at), Some(dirty)) = (&capture.header.head, capture.header.dirty) else {
        return Err("ran outside a git work tree".to_string());
    };
    if ran_at != head {
        return Err(format!(
            "ran at {}, not at {}",
            escaped_line(git::short(ran_at)),
            git::short(head)
        ));
    }
    if dirty {
        return Err("ran in a dirty work tree".to_string());
    }
    let document: Value = serde_json::from_slice(capture.output())
        .map_err(|error| format!("holds no JSON ({error})"))?;
    items_of(&document).ok_or_else(|| NOT_A_DOCUMENT.to_string())
}

/// Sets the `validation` of each intent of `intents` from the capture that `path` names, when
/// there is one: the intents are those of the range that ends at the commit `head`, and
/// `command` (`evidence` or `record`) opens the line of a refusal. An intent of another source,
/// an `openspec` intent that did not resolve, and one with no item in the capture keep what
/// they hold.
///
/// A capture that cannot be used leaves the intents as they are and gives the line of spec 4.1
/// for stderr: `<command>: --validation <path>: <reason>`.
pub(crate) fn fill(
    command: &str,
    path: Option<&Path>,
    head: &str,
    intents: &mut [Intent],
) -> Option<String> {
    let path = path?;
    let items = match read(path, head) {
        Ok(items) => items,
        Err(reason) => {
            return Some(format!(
                "{command}: --validation {}: {reason}",
                path.display()
            ));
        }
    };
    for intent in intents
        .iter_mut()
        .filter(|intent| intent.source == Adapter::OpenSpec.name() && intent.unresolved.is_none())
    {
        let item = items
            .iter()
            .find(|item| item.kind == CHANGE_TYPE && item.id == intent.reference);
        if let Some(item) = item {
            intent.validation = Some(item.validation());
        }
    }
    None
}
