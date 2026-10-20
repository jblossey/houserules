//! The size budgets of the generated files. Each output has one budget, read
//! from its generated text, never from the file on disk (design 5.115): the
//! block of the root `AGENTS.md` and its standing section, the block of each
//! directory `AGENTS.md`, each `.claude/rules/<area>.md` file, and the
//! `project-knowledge` skill. The standing rules count once, in the root
//! block (design 5.129).
//!
//! `CLAUDE.md` is no generated file. It keeps its 1.4.0 budget, the limits of
//! the root `AGENTS.md`, on the text of the file, and its absence is never a
//! finding: a harness that never reads it may delete it
//! (`houserules.agents-md-is-canonical`).

use std::fs;
use std::path::Path;

use super::render::{Class, Output};

/// The line and byte limits of one output class.
struct Limit {
    lines: usize,
    bytes: Option<usize>,
}

/// The block of the root `AGENTS.md`, and `CLAUDE.md`.
const ROOT: Limit = Limit {
    lines: 200,
    bytes: Some(12288),
};
/// The standing section of the root block.
const STANDING_LINES: usize = 60;
/// A directory block and a `.claude/rules/<area>.md` file.
const AREA: Limit = Limit {
    lines: 160,
    bytes: None,
};
/// The `project-knowledge` skill.
const SKILL: Limit = Limit {
    lines: 120,
    bytes: None,
};

/// The number of lines of `text`; a final terminator ends a line and starts
/// none.
fn line_count(text: &str) -> usize {
    text.lines().count()
}

/// The findings for `text` over `limit`, each as `<prefix> <n> lines, budget
/// <max>` or `<prefix> <n> bytes, budget <max>`.
fn limit_findings(prefix: &str, text: &str, limit: &Limit) -> Vec<String> {
    let mut findings = Vec::new();
    let lines = line_count(text);
    if lines > limit.lines {
        findings.push(format!("{prefix} {lines} lines, budget {}", limit.lines));
    }
    if let Some(max_bytes) = limit.bytes
        && text.len() > max_bytes
    {
        findings.push(format!("{prefix} {} bytes, budget {max_bytes}", text.len()));
    }
    findings
}

/// The findings for the block `text` of the file `path` over `limit`.
fn block_findings(path: &str, text: &str, limit: &Limit) -> Vec<String> {
    limit_findings(&format!("{path}: the houserules block has"), text, limit)
}

/// The findings for the generated `outputs` over their budgets, then for
/// `CLAUDE.md` under `root` over its budget.
pub(super) fn findings(root: &Path, outputs: &[Output]) -> Vec<String> {
    let mut findings = Vec::new();
    for output in outputs {
        match output {
            Output::Block {
                path,
                block,
                class: Class::Root { standing_lines },
            } => {
                findings.extend(block_findings(path, block, &ROOT));
                if *standing_lines > STANDING_LINES {
                    findings.push(format!(
                        "{path}: the standing rules have {standing_lines} lines, budget {STANDING_LINES}"
                    ));
                }
            }
            Output::Block { path, block, .. } => {
                findings.extend(block_findings(path, block, &AREA));
            }
            Output::File {
                path,
                content,
                class,
            } => {
                let limit = if *class == Class::Skill {
                    &SKILL
                } else {
                    &AREA
                };
                findings.extend(limit_findings(&format!("{path}:"), content, limit));
            }
            Output::Delete { .. } | Output::DropBlock { .. } => {}
        }
    }
    if let Ok(text) = fs::read_to_string(root.join("CLAUDE.md")) {
        findings.extend(limit_findings("CLAUDE.md:", &text, &ROOT));
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A block of `lines` lines, each of `width` bytes without the newline.
    fn block(lines: usize, width: usize) -> String {
        vec!["x".repeat(width); lines].join("\n")
    }

    fn root_block(text: String, standing_lines: usize) -> Output {
        Output::Block {
            path: "AGENTS.md".to_string(),
            block: text,
            class: Class::Root { standing_lines },
        }
    }

    /// The root block has 200 lines and 12288 bytes; the limit itself passes.
    #[test]
    fn the_root_block_has_the_agents_md_limits() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(findings(dir.path(), &[root_block(block(200, 10), 5)]).is_empty());
        assert_eq!(
            findings(dir.path(), &[root_block(block(201, 10), 5)]),
            ["AGENTS.md: the houserules block has 201 lines, budget 200"]
        );
        assert!(findings(dir.path(), &[root_block("x".repeat(12288), 5)]).is_empty());
        assert_eq!(
            findings(dir.path(), &[root_block("x".repeat(12289), 5)]),
            ["AGENTS.md: the houserules block has 12289 bytes, budget 12288"]
        );
    }

    /// The standing section has 60 lines, whatever the rest of the block.
    #[test]
    fn the_standing_section_has_a_limit_of_60_lines() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(findings(dir.path(), &[root_block(block(10, 10), 60)]).is_empty());
        assert_eq!(
            findings(dir.path(), &[root_block(block(10, 10), 61)]),
            ["AGENTS.md: the standing rules have 61 lines, budget 60"]
        );
    }

    /// A directory block has 160 lines and no byte limit.
    #[test]
    fn a_directory_block_has_the_area_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let block_of = |lines| Output::Block {
            path: "tests/AGENTS.md".to_string(),
            block: block(lines, 200),
            class: Class::Directory,
        };
        assert!(findings(dir.path(), &[block_of(160)]).is_empty());
        assert_eq!(
            findings(dir.path(), &[block_of(161)]),
            ["tests/AGENTS.md: the houserules block has 161 lines, budget 160"]
        );
    }

    /// An area file has 160 lines and the skill 120; a final terminator
    /// starts no line.
    #[test]
    fn the_claude_files_have_their_limits() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = |path: &str, lines, class| Output::File {
            path: path.to_string(),
            content: format!("{}\n", block(lines, 10)),
            class,
        };
        assert!(
            findings(
                dir.path(),
                &[
                    file(".claude/rules/a.md", 160, Class::AreaFile),
                    file("SKILL.md", 120, Class::Skill)
                ]
            )
            .is_empty()
        );
        assert_eq!(
            findings(
                dir.path(),
                &[
                    file(".claude/rules/a.md", 161, Class::AreaFile),
                    file("SKILL.md", 121, Class::Skill)
                ]
            ),
            [
                ".claude/rules/a.md: 161 lines, budget 160",
                "SKILL.md: 121 lines, budget 120"
            ]
        );
    }

    /// `CLAUDE.md` keeps the limits of the root `AGENTS.md`; its absence is
    /// no finding, and a file that cannot be read as text is not measured.
    #[test]
    fn claude_md_keeps_its_budget_and_its_absence_is_no_finding() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(findings(dir.path(), &[]).is_empty());

        fs::write(
            dir.path().join("CLAUDE.md"),
            format!("{}\n", block(201, 10)),
        )
        .expect("write");
        assert_eq!(
            findings(dir.path(), &[]),
            ["CLAUDE.md: 201 lines, budget 200"]
        );

        fs::write(dir.path().join("CLAUDE.md"), block(10, 1300)).expect("write");
        assert_eq!(
            findings(dir.path(), &[]),
            ["CLAUDE.md: 13009 bytes, budget 12288"]
        );
    }

    /// The text of the file on disk never counts for a generated output.
    #[test]
    fn the_file_on_disk_never_counts_for_a_generated_output() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("AGENTS.md"), block(500, 100)).expect("write");
        assert!(findings(dir.path(), &[root_block(block(10, 10), 5)]).is_empty());
    }
}
