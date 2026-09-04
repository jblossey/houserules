//! `renderAll` and the `render` command: `tools/kb.mjs`'s generated-file
//! writer, ported byte-for-byte (HR-054 task 3; the frozen fixture corpus
//! under `tests/corpus/` is the parity gate — see
//! `crates/houserules/tests/`).

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitCode};

use super::model::{Base, Entry, load_base};

/// Header stamped on every file `render` writes, so an editor knows not to hand-edit it.
pub(crate) const GENERATED: &str = "Generated from knowledge/ by tools/kb.sh render. Do not edit.";
/// Repo-relative path of the generated knowledge skill file.
pub(crate) const SKILL_PATH: &str = ".claude/skills/project-knowledge/SKILL.md";
/// Entry kinds eligible for the standing rules, in the order they render.
/// Also `check.rs`'s standing-shape check: a standing entry needs one of
/// these kinds (and area `global` or `process`), the same pair
/// `tools/kb.mjs`'s `RULE_KINDS` constant serves for both surfaces.
pub(super) const RULE_KINDS: [&str; 2] = ["rule", "invariant"];
/// Entry kinds rendered into a per-area `.claude/rules/<area>.md` file.
const AREA_FILE_KINDS: [&str; 3] = ["rule", "invariant", "gotcha"];
/// An area file's sections, in render order, each paired with its entry kind.
const SECTION_KINDS: [(&str, &str); 3] = [
    ("Rules", "rule"),
    ("Invariants", "invariant"),
    ("Gotchas", "gotcha"),
];
/// The retrieval protocol lines the knowledge skill lists under `## Retrieval protocol`.
const PROTOCOL: [&str; 3] = [
    "1. Resolve every id under `Knowledge:` in your task: `tools/kb.sh get <ids>` (JSON).",
    "2. Before editing, run `tools/kb.sh for <every file you will change>` and `get` any rule you are unsure about.",
    "3. Write `REPORT_FILE` as a `task-report` (schema `.claude/schemas/deliverables.json`, `self_audit: null`), then run `tools/kb.sh audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>`. The `--ids` value is the task's `Knowledge:` list, generated from it, never typed separately. Copy the audit `summary` and its `deterministic` rows into `self_audit` — never hand-written rows; the judged rows are the reviewer's. Fix every `fail`, re-run until clean, then run `tools/kb.sh validate <REPORT_FILE>` and fix every error. List the ids you relied on in `knowledge_used`.",
];

/// Uppercases the first character of `s`, the rest untouched -- the port
/// of `tools/kb.mjs`'s `cap` helper, which builds an area file's `#
/// <Area> rules` heading. Relies on every area name being a single ASCII
/// word (`knowledge/areas.json`'s keys, e.g. `docs`, `cli`): `to_uppercase`
/// on a non-ASCII first character can grow it to more than one character
/// (German `ß` uppercases to `SS`, for instance), which `cap`'s
/// `s[0].toUpperCase() + s.slice(1)` cannot do at all, since JS indexes a
/// string by UTF-16 code unit -- an area name outside that assumption is
/// unverified on both sides.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Renders one entry as `- [id] summary` -- the port of `tools/kb.mjs`'s
/// `ruleLine`. `standing_lines`, every area section, and the knowledge
/// skill's `## Standing rules` block all depend on this exact format for
/// byte parity with the frozen corpus.
fn rule_line(e: &Entry) -> String {
    format!("- [{}] {}", e.id, e.summary)
}

/// Renders the standing rules as `- [id] summary` markdown lines, every
/// standing rule (sorted by id) before every standing invariant (sorted by id).
fn standing_lines(base: &Base) -> Vec<String> {
    RULE_KINDS
        .iter()
        .flat_map(|kind| {
            let mut kind_entries: Vec<&Entry> = base
                .entries
                .values()
                .filter(|e| e.standing && e.kind == *kind)
                .collect();
            kind_entries.sort_by(|a, b| a.id.cmp(&b.id));
            kind_entries
        })
        .map(rule_line)
        .collect()
}

/// Renders the `## Topics` markdown lines the knowledge skill file lists.
fn topic_lines(base: &Base) -> Vec<String> {
    base.topics
        .iter()
        .map(|t| format!("{}  {}  {}", t.name, t.entry_count, t.title))
        .collect()
}

/// Builds the generated markdown files (standing rules, per-area rules,
/// the knowledge skill), in the exact order `render`/`render --check`
/// reports them: standing rules first, each area with rendered entries in
/// `areas.json`'s declared order, then the knowledge skill last.
pub(crate) fn render_all(base: &Base) -> Vec<(String, String)> {
    let mut files = Vec::new();

    let standing = standing_lines(base).join("\n");
    files.push((
        ".claude/rules/standing-rules.md".to_string(),
        format!("{GENERATED}\n\n# Standing rules\n\n{standing}\n"),
    ));

    for (area, def) in &base.areas {
        if def.paths.is_empty() {
            continue;
        }
        let members: Vec<&Entry> = base
            .entries
            .values()
            .filter(|e| e.area == *area && AREA_FILE_KINDS.contains(&e.kind.as_str()))
            .collect();
        if members.is_empty() {
            continue;
        }
        let sections: Vec<String> = SECTION_KINDS
            .iter()
            .filter_map(|(title, kind)| {
                let mut items: Vec<&Entry> = members
                    .iter()
                    .copied()
                    .filter(|e| e.kind == *kind)
                    .collect();
                if items.is_empty() {
                    return None;
                }
                items.sort_by(|a, b| a.id.cmp(&b.id));
                let lines = items
                    .iter()
                    .map(|e| rule_line(e))
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(format!("## {title}\n\n{lines}\n"))
            })
            .collect();
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
        files.push((
            format!(".claude/rules/{area}.md"),
            format!(
                "---\npaths:\n{paths_block}\n---\n{GENERATED}\n\n# {title} rules\n\n{body}\nDetail: tools/kb.sh get <id>\n",
                title = capitalize(area),
                body = sections.join("\n"),
            ),
        ));
    }

    let mut skill_lines: Vec<String> = vec![
        "---".to_string(),
        "name: project-knowledge".to_string(),
        "description: Use when working on this repository as a dispatched subagent, before reading or changing any file".to_string(),
        "user-invocable: false".to_string(),
        "---".to_string(),
        GENERATED.to_string(),
        String::new(),
        "# Project knowledge".to_string(),
        String::new(),
        "## Standing rules".to_string(),
        String::new(),
    ];
    skill_lines.extend(standing_lines(base));
    skill_lines.push(String::new());
    skill_lines.push("## Retrieval protocol".to_string());
    skill_lines.push(String::new());
    skill_lines.extend(PROTOCOL.iter().map(|s| s.to_string()));
    skill_lines.push(String::new());
    skill_lines.push("## Topics".to_string());
    skill_lines.push(String::new());
    skill_lines.extend(topic_lines(base));
    skill_lines.push(String::new());
    files.push((SKILL_PATH.to_string(), skill_lines.join("\n")));

    files
}

/// Writes every stale file `render_all` produces under `base.root`, or
/// with `check`, only reports which ones are stale without writing.
/// Returns the stale paths in `render_all`'s order.
pub(crate) fn render(base: &Base, check: bool) -> io::Result<Vec<String>> {
    let mut stale = Vec::new();
    for (path, content) in render_all(base) {
        let abs = base.root.join(&path);
        let current = fs::read_to_string(&abs).ok();
        if current.as_deref() == Some(content.as_str()) {
            continue;
        }
        stale.push(path);
        if !check {
            if let Some(parent) = abs.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&abs, content.as_bytes())?;
        }
    }
    Ok(stale)
}

/// Resolves the enclosing git repository's top-level directory from the
/// current working directory, the same resolution `tools/kb.sh render` (and
/// `tools/kb.sh check`, via `cmd_check_knowledge` in `check.rs`) performs
/// before loading the knowledge base. On failure (no enclosing
/// repository, for instance) git itself can print more than one stderr
/// line -- verified live: `git rev-parse --show-toplevel` outside any
/// repository prints "fatal: not a git repository ..." AND a second
/// "Stopping at filesystem boundary ..." line -- so this keeps only the
/// first non-empty one, the same convention `tools/kb.mjs`'s `gitDiff`
/// uses for its own git-subprocess errors, to hold the recorded one-line
/// error contract (docs/specs/2026-09-04-batch-15-tier2-spec.md §6).
pub(super) fn repo_root_from_cwd() -> io::Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let first_line = stderr.lines().find(|line| !line.trim().is_empty());
        return Err(io::Error::other(
            first_line
                .unwrap_or("git rev-parse --show-toplevel failed")
                .trim()
                .to_string(),
        ));
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

/// Runs the `render` subcommand: loads the knowledge base at `root`
/// (resolving the enclosing git repository's top level when `root` is
/// `None`), writes every stale generated file — or, with `check`, only
/// reports which ones are stale — and prints the same messages and exit
/// code as `tools/kb.sh render`.
pub(crate) fn cmd_render(root: Option<PathBuf>, check: bool) -> ExitCode {
    let root = match root {
        Some(path) => path,
        None => match repo_root_from_cwd() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        },
    };
    let base = match load_base(&root) {
        Ok(base) => base,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let stale = match render(&base, check) {
        Ok(stale) => stale,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };

    if check && !stale.is_empty() {
        for path in &stale {
            eprintln!("{path}: would change");
        }
        return ExitCode::from(1);
    }
    if !stale.is_empty() && !check {
        for path in &stale {
            println!("{path}: written");
        }
    } else {
        println!("render: up to date");
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::model::{AreaDef, TopicMeta};
    use super::*;

    fn entry(id: &str, kind: &str, area: &str, standing: bool, summary: &str) -> Entry {
        Entry {
            id: id.to_string(),
            kind: kind.to_string(),
            area: area.to_string(),
            standing,
            summary: summary.to_string(),
        }
    }

    /// tests/kb.test.mjs, describe('render')'s fixture: `process` carries
    /// two entries (a standing rule, a standing invariant), `rust` carries
    /// three (a rule, a gotcha, and a `history`-kind entry excluded from
    /// the rendered area file but still counted in the topic line).
    fn fixture_base() -> Base {
        let areas = vec![
            ("global".to_string(), AreaDef { paths: vec![] }),
            ("process".to_string(), AreaDef { paths: vec![] }),
            (
                "rust".to_string(),
                AreaDef {
                    paths: vec!["crates/**".to_string(), "Cargo.toml".to_string()],
                },
            ),
        ];
        let mut entries = HashMap::new();
        for e in [
            entry(
                "process.sequential",
                "rule",
                "process",
                true,
                "Run agents sequentially.",
            ),
            entry(
                "process.ask",
                "invariant",
                "process",
                true,
                "Ask when unsure.",
            ),
            entry("rust.clean", "gotcha", "rust", false, "Clean before retry."),
            entry("rust.floor", "rule", "rust", false, "Never lower a floor."),
            entry("rust.old", "history", "rust", false, "Old."),
        ] {
            entries.insert(e.id.clone(), e);
        }
        let topics = vec![
            TopicMeta {
                name: "process".to_string(),
                title: "process title".to_string(),
                entry_count: 2,
            },
            TopicMeta {
                name: "rust".to_string(),
                title: "rust title".to_string(),
                entry_count: 3,
            },
        ];
        Base {
            root: PathBuf::new(),
            areas,
            entries,
            topics,
            schema: serde_json::Value::Null,
            areas_raw: serde_json::Value::Null,
            topic_files: Vec::new(),
        }
    }

    /// tests/kb.test.mjs, describe('render'): "renders standing rules, one
    /// file per area with entries, and the knowledge skill".
    #[test]
    fn renders_standing_rules_one_file_per_area_and_the_knowledge_skill() {
        let base = fixture_base();
        let files = render_all(&base);
        let keys: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                ".claude/rules/standing-rules.md",
                ".claude/rules/rust.md",
                SKILL_PATH
            ]
        );

        assert_eq!(
            files[0].1,
            format!(
                "{GENERATED}\n\n# Standing rules\n\n- [process.sequential] Run agents sequentially.\n- [process.ask] Ask when unsure.\n"
            ),
        );

        assert_eq!(
            files[1].1,
            format!(
                "---\npaths:\n  - \"crates/**\"\n  - \"Cargo.toml\"\n---\n{GENERATED}\n\n# Rust rules\n\n## Rules\n\n- [rust.floor] Never lower a floor.\n\n## Gotchas\n\n- [rust.clean] Clean before retry.\n\nDetail: tools/kb.sh get <id>\n"
            ),
        );

        let skill = &files[2].1;
        assert!(skill.starts_with(&format!(
            "---\nname: project-knowledge\ndescription: Use when working on this repository as a dispatched subagent, before reading or changing any file\nuser-invocable: false\n---\n{GENERATED}\n\n# Project knowledge\n"
        )));
        assert!(skill.contains(
            "## Standing rules\n\n- [process.sequential] Run agents sequentially.\n- [process.ask] Ask when unsure.\n\n## Retrieval protocol\n\n1. Resolve every id under `Knowledge:`"
        ));
        assert!(skill.contains(PROTOCOL[2]));
        assert!(skill.ends_with("## Topics\n\nprocess  2  process title\nrust  3  rust title\n"));
    }

    /// tests/kb.test.mjs, describe('render'): "render writes stale files;
    /// --check only lists them".
    #[test]
    fn render_writes_stale_files_check_only_lists_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut base = fixture_base();
        base.root = dir.path().to_path_buf();

        let mut stale = render(&base, true).expect("render check");
        stale.sort();
        let mut expected = vec![
            SKILL_PATH.to_string(),
            ".claude/rules/rust.md".to_string(),
            ".claude/rules/standing-rules.md".to_string(),
        ];
        expected.sort();
        assert_eq!(stale, expected);
        assert!(!dir.path().join(SKILL_PATH).exists());

        assert_eq!(render(&base, false).expect("render write").len(), 3);
        let rust_md = fs::read_to_string(dir.path().join(".claude/rules/rust.md")).unwrap();
        assert!(rust_md.contains("# Rust rules"));
        assert!(render(&base, false).expect("render clean").is_empty());
    }
}
