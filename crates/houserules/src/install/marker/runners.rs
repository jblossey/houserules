//! The `runners` key of `.houserules.json` (batch 35, spec 3.1): the external
//! tools that a `runner` check names. The key is an object whose members are
//! `{ "command": [...], "parser": "sarif" | "exit-code", "install": "..." }`,
//! one for each tool. Only the project writes it, and every command that
//! writes the marker keeps it as found.
//!
//! [`faults`] reads the whole key for `check-knowledge`, and [`find`] reads
//! the entry of one tool for `audit`. Both give the same line for the same
//! fault, and a line names the first fault of its entry: the `command`, then
//! the `parser`, then the `install` line, then a key that is none of the three.

use serde_json::{Map, Value};

/// The keys of one entry of `runners`.
const ENTRY_KEYS: [&str; 3] = ["command", "parser", "install"];

/// How `audit` reads the output of a tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Parser {
    /// The tool prints SARIF 2.1.0 on stdout; each result is a finding.
    Sarif,
    /// The tool exits 0 when the check holds and with any other status when it
    /// does not.
    ExitCode,
}

/// One tool of `runners`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunnerDef {
    /// The program and its arguments. The list is never empty.
    pub(crate) command: Vec<String>,
    /// How to read the output of the tool.
    pub(crate) parser: Parser,
    /// The line that tells a person how to install the tool.
    pub(crate) install: String,
}

/// One line for each fault of the `runners` value `runners`, in the order of
/// the file: a value that is no object, or the first fault of each entry.
/// `file` is how a line names the marker.
pub(super) fn faults(file: &str, runners: &Value) -> Vec<String> {
    match runners {
        Value::Object(tools) => tools
            .iter()
            .filter_map(|(tool, entry)| parse(file, tool, entry).err())
            .collect(),
        _ => vec![not_an_object(file)],
    }
}

/// The entry of `tool` in `runners`, the `runners` value of a marker, or
/// `None` when the marker has no `runners` key or the key has no such entry.
/// A `runners` value that is no object, or an entry with a fault, is the line
/// that [`faults`] gives for it.
pub(super) fn find(
    file: &str,
    runners: Option<&Value>,
    tool: &str,
) -> Result<Option<RunnerDef>, String> {
    match runners {
        None => Ok(None),
        Some(Value::Object(tools)) => tools
            .get(tool)
            .map(|entry| parse(file, tool, entry))
            .transpose(),
        Some(_) => Err(not_an_object(file)),
    }
}

/// The line of a `runners` value that is no object.
fn not_an_object(file: &str) -> String {
    format!("{file}: runners must be an object")
}

/// The entry `entry` of `tool`, or the line of its first fault.
fn parse(file: &str, tool: &str, entry: &Value) -> Result<RunnerDef, String> {
    let at = format!("{file}: runners.{tool}");
    let Some(members) = entry.as_object() else {
        return Err(format!("{at} must be an object"));
    };
    let command = command(members)
        .ok_or_else(|| format!("{at}.command must be a non-empty array of strings"))?;
    let parser = match members.get("parser").and_then(Value::as_str) {
        Some("sarif") => Parser::Sarif,
        Some("exit-code") => Parser::ExitCode,
        _ => return Err(format!("{at}.parser must be \"sarif\" or \"exit-code\"")),
    };
    let install = match members.get("install").and_then(Value::as_str) {
        Some(line) if !line.is_empty() => line.to_string(),
        _ => return Err(format!("{at}.install must be a non-empty string")),
    };
    if let Some(key) = members
        .keys()
        .find(|key| !ENTRY_KEYS.contains(&key.as_str()))
    {
        return Err(format!("{at} has an unknown key {key:?}"));
    }
    Ok(RunnerDef {
        command,
        parser,
        install,
    })
}

/// The `command` of an entry: a non-empty array of strings, else `None`.
fn command(members: &Map<String, Value>) -> Option<Vec<String>> {
    let items = members.get("command")?.as_array()?;
    if items.is_empty() {
        return None;
    }
    items
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}
