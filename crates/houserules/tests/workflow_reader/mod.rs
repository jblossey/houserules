//! The workflow reader of the two workflow test files,
//! `dependabot_config.rs` and `knowledge_workflow.rs`: helpers that read a
//! workflow file by indentation and run a step's own script.
//!
//! This crate has no YAML dependency, and a fixed workflow shape needs
//! none. Exactly these two test files declare `mod workflow_reader;`, and
//! each uses every item, so neither test binary leaves one unused. That is
//! why the helpers live here and not in `tests/common/mod.rs`: importing
//! `common` adds its worktree and binary helpers to a binary that uses
//! none of them, and adding a helper to `common` leaves it unused in the
//! five binaries that already import it (both break `cargo clippy
//! --all-targets -- -D warnings`). A third user of these helpers declares
//! `mod workflow_reader;` too; it must use all of them.

use std::path::{Path, PathBuf};

/// This checkout's repository root, resolved at compile time from the
/// crate's manifest directory, so it is correct whatever the test runner's
/// working directory. Other test files keep their own copy of this
/// function (`install.rs`'s own doc explains why); the two workflow test
/// files share this one.
pub fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The number of leading spaces of `line`.
pub fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The lines nested below the first line that equals `key` once trimmed:
/// every following line indented deeper than that line, up to the first
/// line that is not. Blank lines stay in the block. The block is empty
/// when no line equals `key`.
pub fn nested_under<'a>(lines: &[&'a str], key: &str) -> Vec<&'a str> {
    let Some(start) = lines.iter().position(|line| line.trim() == key) else {
        return Vec::new();
    };
    let key_indent = indent(lines[start]);
    lines[start + 1..]
        .iter()
        .copied()
        .take_while(|line| line.trim().is_empty() || indent(line) > key_indent)
        .collect()
}

/// The steps of the workflow's job, each as its raw lines.
pub fn workflow_steps(raw: &str) -> Vec<Vec<&str>> {
    let lines: Vec<&str> = raw.lines().collect();
    let steps = nested_under(&lines, "steps:");
    let item_indent = steps
        .iter()
        .find(|line| !line.trim().is_empty())
        .map(|line| indent(line))
        .expect("the workflow has a steps: block with at least one step");
    let mut items: Vec<Vec<&str>> = Vec::new();
    for line in steps {
        if indent(line) == item_indent && line.trim_start().starts_with("- ") {
            items.push(Vec::new());
        }
        items
            .last_mut()
            .expect("the first line of a steps: block opens a step")
            .push(line);
    }
    items
}

/// The text of a step-level key line at `position`: the first line loses
/// its `- ` marker; any other line counts only at the step's key indent.
fn key_content<'a>(step: &[&'a str], position: usize) -> Option<&'a str> {
    let line = step[position];
    if position == 0 {
        line.trim_start().strip_prefix("- ")
    } else if indent(line) == indent(step[0]) + 2 {
        Some(line.trim_start())
    } else {
        None
    }
}

/// One step-level key, as `(text after the colon, lines nested under it)`.
/// `None` when the step has no such key.
pub fn step_key<'a>(step: &[&'a str], key: &str) -> Option<(String, Vec<&'a str>)> {
    let prefix = format!("{key}:");
    let key_indent = indent(step[0]) + 2;
    let (start, inline) = (0..step.len()).find_map(|position| {
        key_content(step, position)?
            .strip_prefix(&prefix)
            .map(|rest| (position, rest.trim().to_string()))
    })?;
    let nested = step[start + 1..]
        .iter()
        .copied()
        .take_while(|line| line.trim().is_empty() || indent(line) > key_indent)
        .collect();
    Some((inline, nested))
}

/// A step key's value folded onto one line: a bare `>` or `|` indicator is
/// dropped and the nested lines join with single spaces.
pub fn step_scalar(step: &[&str], key: &str) -> Option<String> {
    let (inline, nested) = step_key(step, key)?;
    let inline = if matches!(inline.as_str(), ">" | "|") {
        ""
    } else {
        inline.as_str()
    };
    let parts: Vec<&str> = std::iter::once(inline)
        .chain(nested.iter().map(|line| line.trim()))
        .filter(|part| !part.is_empty())
        .collect();
    Some(parts.join(" "))
}

/// The `run:` value of a step as a shell script: the text after the colon
/// when it is a command, then the nested lines, dedented by their common
/// indent. A bare `|` or `>` indicator adds nothing.
#[cfg(unix)]
pub fn step_script(step: &[&str]) -> String {
    let (inline, nested) = step_key(step, "run").expect("the step has a run: key");
    let mut script = String::new();
    if !matches!(inline.as_str(), "" | "|" | ">") {
        script.push_str(&inline);
        script.push('\n');
    }
    let common_indent = nested
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| indent(line))
        .min()
        .unwrap_or(0);
    for line in nested {
        script.push_str(line.get(common_indent..).unwrap_or(""));
        script.push('\n');
    }
    assert!(!script.trim().is_empty(), "the run: key holds no script");
    script
}

/// The position of the one step whose `run:` holds `command`.
pub fn position_of_step_running(steps: &[Vec<&str>], command: &str) -> usize {
    let matching: Vec<usize> = steps
        .iter()
        .enumerate()
        .filter(|(_, step)| step_scalar(step, "run").is_some_and(|run| run.contains(command)))
        .map(|(position, _)| position)
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one step that runs {command:?}, found {}",
        matching.len()
    );
    matching[0]
}
