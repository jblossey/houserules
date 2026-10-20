//! The Claude renderer: the area files `.claude/rules/<area>.md` and the
//! `project-knowledge` skill, which it owns whole. It writes no file for the
//! standing rules: they reach Claude Code through the block of the root
//! `AGENTS.md` and the `@AGENTS.md` import of `CLAUDE.md`.
//!
//! A file in `.claude/rules/` that carries the generated header and that the
//! renderer no longer writes (its area is gone, or the area has no rule left)
//! is deleted. A file there without the header is no output of the renderer:
//! it stays, and `check-knowledge` reports it.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::Path;

use super::super::model::Base;
use super::{Class, GENERATED, Output, RenderError, SKILL_PATH, area_sections, capitalize};

/// The directory of the area files, relative to the install root.
const RULES_DIR: &str = ".claude/rules";

/// The retrieval protocol lines the knowledge skill lists under
/// `## Retrieval protocol`, after its lead sentence.
const PROTOCOL: [&str; 3] = [
    "1. Resolve every id under `Knowledge:` in your task: `houserules get <ids>` (JSON).",
    "2. Before editing, run `houserules for <every file you will change>` and `get` any rule you are unsure about.",
    "3. Write `REPORT_FILE` as a `task-report` (schema `.claude/schemas/deliverables.json`, `self_audit: null`), then run `houserules audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>`. The `--ids` value is the task's `Knowledge:` list, generated from it, never typed separately. Copy the audit `summary` and its `deterministic` rows into `self_audit` — never hand-written rows; the judged rows are the reviewer's. Fix every `fail`, re-run until clean, then run `houserules validate <REPORT_FILE>` and fix every error. Declare a spec-booked interim fail once with `--sanctioned <rule>=<ref>` instead of narrating it by hand. List the ids you relied on in `knowledge_used`.",
];

/// The sentence that opens the retrieval protocol: the agent saves the
/// output of a `houserules` command to a file and reads the file, so a
/// transport that shortens the output cannot hide a rule.
const SAVE_OUTPUT_LINE: &str = "Save the output of every `houserules` command to a file, then read the file with your file-read tool.";

/// Renders the `## Topics` markdown lines the knowledge skill file lists.
fn topic_lines(base: &Base) -> Vec<String> {
    base.topics
        .iter()
        .map(|t| format!("{}  {}  {}", t.name, t.entry_count, t.title))
        .collect()
}

/// The outputs of the Claude renderer, in the order `render` reports them:
/// each area with rules in `areas.json`'s declared order, the knowledge
/// skill, then each generated file in `.claude/rules/` that is no longer
/// written, by name.
pub(super) fn plan(base: &Base) -> Result<Vec<Output>, RenderError> {
    let mut outputs = Vec::new();
    for (area, def) in &base.areas {
        if def.paths.is_empty() {
            continue;
        }
        let Some(body) = area_sections(base, area, 2) else {
            continue;
        };
        let paths_block = def
            .paths
            .iter()
            .map(|glob| {
                format!(
                    "  - {}",
                    serde_json::to_string(glob).expect("a string always serializes")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        outputs.push(Output::File {
            path: format!("{RULES_DIR}/{area}.md"),
            content: format!(
                "---\npaths:\n{paths_block}\n---\n{GENERATED}\n\n# {title} rules\n\n{body}\nDetail: houserules get <id>\n",
                title = capitalize(area),
            ),
            class: Class::AreaFile,
        });
    }
    outputs.push(skill(base));
    let written: HashSet<&str> = outputs
        .iter()
        .filter_map(|output| match output {
            Output::File { path, .. } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    let (generated, _) = unwritten_rule_files(&base.root, &written)?;
    let deletions: Vec<Output> = generated
        .into_iter()
        .map(|path| Output::Delete { path })
        .collect();
    outputs.extend(deletions);
    Ok(outputs)
}

/// The `project-knowledge` skill: its front matter, the retrieval protocol,
/// and the topic list.
fn skill(base: &Base) -> Output {
    let mut lines: Vec<String> = vec![
        "---".to_string(),
        "name: project-knowledge".to_string(),
        "description: Use when working on this repository as a dispatched subagent, before reading or changing any file".to_string(),
        "user-invocable: false".to_string(),
        "---".to_string(),
        GENERATED.to_string(),
        String::new(),
        "# Project knowledge".to_string(),
        String::new(),
        "## Retrieval protocol".to_string(),
        String::new(),
        SAVE_OUTPUT_LINE.to_string(),
        String::new(),
    ];
    lines.extend(PROTOCOL.iter().map(|s| s.to_string()));
    lines.push(String::new());
    lines.push("## Topics".to_string());
    lines.push(String::new());
    lines.extend(topic_lines(base));
    lines.push(String::new());
    Output::File {
        path: SKILL_PATH.to_string(),
        content: lines.join("\n"),
        class: Class::Skill,
    }
}

/// `true` when `text` carries the generated header where `render` puts it:
/// as the first line of the file, or the first line after the front matter.
fn has_generated_header(text: &str) -> bool {
    let text = text.replace("\r\n", "\n");
    let body = match text.strip_prefix("---\n") {
        Some(rest) => match rest.split_once("\n---\n") {
            Some((_, after)) => after,
            None => return false,
        },
        None => text.as_str(),
    };
    body.lines().next() == Some(GENERATED)
}

/// The `.md` files of `.claude/rules/` that `written` does not name, by
/// name, split into those that carry the generated header and those that do
/// not. A directory that does not exist holds none.
fn unwritten_rule_files(
    root: &Path,
    written: &HashSet<&str>,
) -> Result<(Vec<String>, Vec<String>), RenderError> {
    let entries = match fs::read_dir(root.join(RULES_DIR)) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok((Vec::new(), Vec::new()));
        }
        Err(error) => return Err(RenderError::io(RULES_DIR, &error)),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| RenderError::io(RULES_DIR, &error))?;
        let is_file = entry.file_type().is_ok_and(|kind| kind.is_file());
        if let Ok(name) = entry.file_name().into_string()
            && is_file
            && name.ends_with(".md")
        {
            names.push(name);
        }
    }
    names.sort();
    let mut generated = Vec::new();
    let mut foreign = Vec::new();
    for name in names {
        let path = format!("{RULES_DIR}/{name}");
        if written.contains(path.as_str()) {
            continue;
        }
        match fs::read_to_string(root.join(&path)) {
            Ok(text) if has_generated_header(&text) => generated.push(path),
            _ => foreign.push(path),
        }
    }
    Ok((generated, foreign))
}

/// The findings for the files of `.claude/rules/` that no renderer owns:
/// those without the generated header. A fault of reading the directory is
/// one finding.
pub(super) fn foreign_findings(root: &Path, written: &HashSet<&str>) -> Vec<String> {
    match unwritten_rule_files(root, written) {
        Ok((_, foreign)) => foreign
            .into_iter()
            .map(|path| {
                format!(
                    "{path}: not generated by houserules (it has no generated header); remove it"
                )
            })
            .collect(),
        Err(error) => vec![error.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::fixture_base;
    use super::*;

    /// The area file of an area has the 1.4.0 text, and the skill has no
    /// standing section.
    #[test]
    fn renders_one_file_per_area_and_the_knowledge_skill() {
        let base = fixture_base();
        let outputs = plan(&base).expect("plan");
        let files: Vec<(&str, &str)> = outputs
            .iter()
            .map(|output| match output {
                Output::File { path, content, .. } => (path.as_str(), content.as_str()),
                other => panic!("expected a file: {other:?}"),
            })
            .collect();
        assert_eq!(
            files.iter().map(|(path, _)| *path).collect::<Vec<_>>(),
            vec![".claude/rules/rust.md", SKILL_PATH]
        );

        assert_eq!(
            files[0].1,
            format!(
                "---\npaths:\n  - \"crates/**\"\n  - \"Cargo.toml\"\n---\n{GENERATED}\n\n# Rust rules\n\n## Rules\n\n- [rust.floor] Never lower a floor.\n\n## Gotchas\n\n- [rust.clean] Clean before retry.\n\nDetail: houserules get <id>\n"
            ),
        );

        let skill = files[1].1;
        assert!(skill.starts_with(&format!(
            "---\nname: project-knowledge\ndescription: Use when working on this repository as a dispatched subagent, before reading or changing any file\nuser-invocable: false\n---\n{GENERATED}\n\n# Project knowledge\n\n## Retrieval protocol\n\n{SAVE_OUTPUT_LINE}\n\n1. Resolve every id under `Knowledge:`"
        )));
        assert!(!skill.contains("## Standing rules"));
        assert!(!skill.contains("process.sequential"));
        assert!(skill.contains(PROTOCOL[2]));
        assert!(skill.ends_with("## Topics\n\nprocess  2  process title\nrust  3  rust title\n"));
    }

    /// The header counts where `render` puts it, and nowhere else.
    #[test]
    fn the_generated_header_is_the_first_line_after_the_front_matter() {
        assert!(has_generated_header(&format!("{GENERATED}\n\n# Old\n")));
        assert!(has_generated_header(&format!(
            "---\npaths:\n  - \"a/**\"\n---\n{GENERATED}\n\n# A rules\n"
        )));
        assert!(has_generated_header(&format!(
            "---\r\npaths:\r\n---\r\n{GENERATED}\r\n"
        )));
        assert!(!has_generated_header("# Mine\n"));
        assert!(!has_generated_header(&format!("# Mine\n{GENERATED}\n")));
        assert!(!has_generated_header(&format!(
            "---\npaths:\n{GENERATED}\n"
        )));
        assert!(!has_generated_header(""));
    }
}
