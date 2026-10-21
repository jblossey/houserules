//! The `render` command: the generated-file writer, a port with one renderer
//! for each target. The reviewed goldens under `tests/goldens/render/` are
//! the parity gate -- see `crates/houserules/tests/`.
//!
//! A renderer plans its [`Output`]s from the merged base: [`agents_md`] is
//! always on and owns one marked block of each `AGENTS.md` it writes to (the
//! root block also names the session commands of the installed tracker and the
//! pointer of the installed workflow);
//! [`claude`] is on when `renderers` in `.houserules.json` lists `claude` (or
//! the key is absent) and owns the area files `.claude/rules/<area>.md` and
//! the `project-knowledge` skill whole. [`apply`] writes the plan, or with
//! `check` only reports what it would change. [`budget`](super::budget) reads
//! the plan's generated text, never the files on disk.
//!
//! `location` is the git resolution that the commands which load a base share
//! (`render`, `check-knowledge`, and the check of the installed tracker).
//! `block` holds the marker scan.
//!
//! `render_and_report` is `cmd_render`'s own non-`--check` body,
//! extracted so `install::cmd_init` can render a freshly-seeded target
//! exactly as a `houserules render` run there would: `init` seeds the
//! knowledge base and must produce the same generated markdown. A
//! second, independently-written "load, write stale files, report the
//! lines" sequence in `install/mod.rs` would risk exactly the drift this
//! crate's `emit`/`root`/`get` crate-root modules already exist to
//! prevent.

mod agents_md;
mod block;
mod claude;
mod location;

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::install::marker::{RenderInputs, read_render_inputs_for_gate};

use super::budget;
use super::model::{Base, Entry, load_base};

pub(super) use location::git_location_in;
pub(crate) use location::{GitLocation, git_location_from_cwd};

/// Header stamped on every file `render` writes, so an editor knows not to hand-edit it.
pub(crate) const GENERATED: &str = "Generated from knowledge/ by houserules render. Do not edit.";
/// Repo-relative path of the generated knowledge skill file.
pub(crate) const SKILL_PATH: &str = ".claude/skills/project-knowledge/SKILL.md";
/// Entry kinds eligible for the standing rules, in the order they render.
/// Also `check.rs`'s standing-shape check: a standing entry needs one of
/// these kinds (and area `global` or `process`).
pub(super) const RULE_KINDS: [&str; 2] = ["rule", "invariant"];
/// Entry kinds rendered into the rules of an area: a per-area
/// `.claude/rules/<area>.md` file and a directory `AGENTS.md` block.
/// `pub(super)`: `read::for_result` needs the same three kinds, plus
/// `procedure`, for `for`'s own kind filter.
pub(super) const AREA_FILE_KINDS: [&str; 3] = ["rule", "invariant", "gotcha"];
/// The sections of an area's rules, in render order, each paired with its
/// entry kind.
const SECTION_KINDS: [(&str, &str); 3] = [
    ("Rules", "rule"),
    ("Invariants", "invariant"),
    ("Gotchas", "gotcha"),
];

/// The budget class of an output: the limits that [`budget`](super::budget)
/// applies to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Class {
    /// The block of the root `AGENTS.md`; `standing_lines` is the length of
    /// its standing section.
    Root { standing_lines: usize },
    /// The block of a directory `AGENTS.md`.
    Directory,
    /// A `.claude/rules/<area>.md` file.
    AreaFile,
    /// The `project-knowledge` skill.
    Skill,
}

/// One output of a renderer, with paths relative to the install root.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Output {
    /// A file the renderer owns whole.
    File {
        path: String,
        content: String,
        class: Class,
    },
    /// The marked block of a file that the project also edits; `block` runs
    /// from the begin marker to the end marker, joined by `\n`.
    Block {
        path: String,
        block: String,
        class: Class,
    },
    /// A generated file the renderer no longer writes: it goes.
    Delete { path: String },
    /// A block the renderer no longer writes: it goes, and so does the file
    /// when nothing but whitespace is left.
    DropBlock { path: String },
}

/// A render fault, as the one line to print.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RenderError(String);

impl RenderError {
    /// The fault of an operation on `path`, with the operating system's own
    /// text.
    fn io(path: &str, error: &io::Error) -> Self {
        Self(format!("{path}: {error}"))
    }
}

impl fmt::Display for RenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What `render` did, or with `check` would do, to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    /// The file or its block is written.
    Written,
    /// The block goes; the file keeps the text around it.
    BlockRemoved,
    /// The file goes.
    Deleted,
}

/// One file `render` changes.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Change {
    /// The path relative to the install root.
    pub path: String,
    /// What happens to it.
    pub kind: ChangeKind,
}

impl Change {
    /// The line `render` prints after it made the change.
    fn line(&self) -> String {
        let what = match self.kind {
            ChangeKind::Written => "written",
            ChangeKind::BlockRemoved => "block removed",
            ChangeKind::Deleted => "deleted",
        };
        format!("{}: {what}", self.path)
    }

    /// The line `render --check` prints for a change it did not make.
    fn check_line(&self) -> String {
        let what = match self.kind {
            ChangeKind::Written | ChangeKind::BlockRemoved => "would change",
            ChangeKind::Deleted => "would be deleted",
        };
        format!("{}: {what}", self.path)
    }

    /// The finding of `check-knowledge` for a change that is left to make.
    fn finding(&self) -> String {
        let path = &self.path;
        match self.kind {
            ChangeKind::Written => {
                format!("{path}: generated file is out of date (run houserules render)")
            }
            ChangeKind::BlockRemoved => format!(
                "{path}: a houserules block that render no longer writes is left (run houserules render)"
            ),
            ChangeKind::Deleted => {
                format!("{path}: generated file is no longer written (run houserules render)")
            }
        }
    }
}

/// Renders one entry as `- [id] summary`. `standing_lines`, every area
/// section, and the root block's standing section all depend on this exact
/// format.
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

/// Uppercases the first character of `s`, the rest untouched -- builds
/// the `<Area> rules` heading of an area. Relies on every area name being a
/// single ASCII word (`knowledge/areas.json`'s keys, e.g. `docs`, `cli`):
/// `to_uppercase` on a non-ASCII first character can grow it to more than one
/// character (German `ß` uppercases to `SS`, for instance); an area name
/// outside that assumption is unverified.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The rules of `area` as sections at heading `level` (`2` gives `## Rules`),
/// each followed by a blank line, in the order of `SECTION_KINDS`; `None`
/// when the area has no rule, invariant, or gotcha. The sections of the
/// Claude area file and of a directory block share this text.
fn area_sections(base: &Base, area: &str, level: usize) -> Option<String> {
    let members: Vec<&Entry> = base
        .entries
        .values()
        .filter(|e| e.area == area && AREA_FILE_KINDS.contains(&e.kind.as_str()))
        .collect();
    if members.is_empty() {
        return None;
    }
    let hashes = "#".repeat(level);
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
            Some(format!("{hashes} {title}\n\n{lines}\n"))
        })
        .collect();
    Some(sections.join("\n"))
}

/// The outputs of every enabled renderer, in the order `render` and
/// `render --check` report them: the AGENTS.md renderer first, then the
/// Claude renderer when `inputs` enable it.
fn plan_for(base: &Base, inputs: &RenderInputs) -> Result<Vec<Output>, RenderError> {
    let mut outputs = agents_md::plan(base, inputs)?;
    if inputs.claude {
        outputs.extend(claude::plan(base)?);
    }
    Ok(outputs)
}

/// The outputs of every renderer that `.houserules.json` enables.
fn plan(base: &Base) -> Result<Vec<Output>, RenderError> {
    let inputs = read_render_inputs_for_gate(&base.root).map_err(RenderError)?;
    plan_for(base, &inputs)
}

/// Reads `path` below `root`; `None` when the file does not exist.
fn read_optional(root: &Path, path: &str) -> Result<Option<String>, RenderError> {
    match fs::read_to_string(root.join(path)) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(RenderError::io(path, &error)),
    }
}

/// One file operation of a plan.
enum Action {
    /// Write `content` to `path`, creating the directories above it.
    Write { path: String, content: String },
    /// Delete `path`.
    Remove { path: String },
}

/// The changes `outputs` make under `root`, with the file operations that
/// make them. Reads files and writes none; a fault in any file stops the plan
/// before the first write.
fn changes_of(root: &Path, outputs: &[Output]) -> Result<(Vec<Change>, Vec<Action>), RenderError> {
    let mut changes = Vec::new();
    let mut actions = Vec::new();
    for output in outputs {
        match output {
            Output::File { path, content, .. } => {
                if read_optional(root, path)?.as_deref() != Some(content.as_str()) {
                    changes.push(Change {
                        path: path.clone(),
                        kind: ChangeKind::Written,
                    });
                    actions.push(Action::Write {
                        path: path.clone(),
                        content: content.clone(),
                    });
                }
            }
            Output::Block { path, block, .. } => {
                let current = read_optional(root, path)?;
                let updated = block::with_block(current.as_deref().unwrap_or(""), block)
                    .map_err(|fault| RenderError(fault.line(path)))?;
                if current.as_deref() != Some(updated.as_str()) {
                    changes.push(Change {
                        path: path.clone(),
                        kind: ChangeKind::Written,
                    });
                    actions.push(Action::Write {
                        path: path.clone(),
                        content: updated,
                    });
                }
            }
            Output::DropBlock { path } => {
                let Some(text) = read_optional(root, path)? else {
                    continue;
                };
                let Some(rest) =
                    block::without_block(&text).map_err(|fault| RenderError(fault.line(path)))?
                else {
                    continue;
                };
                let emptied = rest.trim().is_empty();
                changes.push(Change {
                    path: path.clone(),
                    kind: ChangeKind::BlockRemoved,
                });
                if emptied {
                    changes.push(Change {
                        path: path.clone(),
                        kind: ChangeKind::Deleted,
                    });
                    actions.push(Action::Remove { path: path.clone() });
                } else {
                    actions.push(Action::Write {
                        path: path.clone(),
                        content: rest,
                    });
                }
            }
            Output::Delete { path } => {
                if root.join(path).exists() {
                    changes.push(Change {
                        path: path.clone(),
                        kind: ChangeKind::Deleted,
                    });
                    actions.push(Action::Remove { path: path.clone() });
                }
            }
        }
    }
    Ok((changes, actions))
}

/// Writes `outputs` under `root`, or with `check` only reports what would
/// change. With `check`, a block that goes with its file is one change, the
/// deletion; written, it is two, the block and then the file. Returns the
/// changes in the order of `outputs`.
fn apply(root: &Path, outputs: &[Output], check: bool) -> Result<Vec<Change>, RenderError> {
    let (changes, actions) = changes_of(root, outputs)?;
    if check {
        return Ok(collapse_emptied_files(changes));
    }
    for action in actions {
        match action {
            Action::Write { path, content } => {
                let abs = root.join(&path);
                if let Some(parent) = abs.parent() {
                    fs::create_dir_all(parent).map_err(|error| RenderError::io(&path, &error))?;
                }
                fs::write(&abs, content.as_bytes())
                    .map_err(|error| RenderError::io(&path, &error))?;
            }
            Action::Remove { path } => match fs::remove_file(root.join(&path)) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(RenderError::io(&path, &error)),
            },
        }
    }
    Ok(changes)
}

/// The changes of a `--check` run: a file whose block goes and whose file
/// then goes is the one change, the deletion.
fn collapse_emptied_files(changes: Vec<Change>) -> Vec<Change> {
    let mut collapsed: Vec<Change> = Vec::new();
    for change in changes {
        let follows_its_block = change.kind == ChangeKind::Deleted
            && collapsed
                .last()
                .is_some_and(|last| last.path == change.path);
        if follows_its_block {
            collapsed.pop();
        }
        collapsed.push(change);
    }
    collapsed
}

/// Writes every stale generated file under `base.root`, or with `check`,
/// only reports which ones are stale without writing. Returns the changes in
/// the order of the renderers' outputs.
pub(crate) fn render(base: &Base, check: bool) -> Result<Vec<Change>, RenderError> {
    apply(&base.root, &plan(base)?, check)
}

/// The findings of `check-knowledge` about the generated files: each change
/// `render` has left to make, each file in `.claude/rules/` that no renderer
/// owns, and each output over its budget. An install whose marker cannot say
/// which renderers are enabled gives the one line that names the marker.
pub(super) fn check_findings(base: &Base) -> Vec<String> {
    let inputs = match read_render_inputs_for_gate(&base.root) {
        Ok(inputs) => inputs,
        Err(line) => return vec![line],
    };
    let outputs = match plan_for(base, &inputs) {
        Ok(outputs) => outputs,
        Err(error) => return vec![error.to_string()],
    };
    let mut findings = Vec::new();
    match apply(&base.root, &outputs, true) {
        Ok(changes) => findings.extend(changes.iter().map(Change::finding)),
        Err(error) => findings.push(error.to_string()),
    }
    if inputs.claude {
        let written: HashSet<&str> = outputs
            .iter()
            .filter_map(|output| match output {
                Output::File { path, .. } => Some(path.as_str()),
                _ => None,
            })
            .collect();
        findings.extend(claude::foreign_findings(&base.root, &written));
    }
    findings.extend(budget::findings(&base.root, &outputs));
    findings
}

/// Loads the knowledge base at `root`, writes every stale generated file,
/// and prints the same `<path>: written` / `render: up to date` lines a
/// non-`--check` `houserules render` run prints -- `cmd_render`'s own
/// non-`--check` body, shared with `install::cmd_init` (this module's own
/// doc has the reuse account). Named `String` errors, not `io::Error`:
/// `install::cmd_init` folds this into its own `Result<(), String>`
/// error chain (`houserules.crash-paths-are-named`'s one-named-line
/// contract), and an `io::Error`'s `Display` text is exactly what both
/// callers already print, so converting costs nothing.
pub(crate) fn render_and_report(root: &Path) -> Result<(), String> {
    let base = load_base(root).map_err(|error| error.to_string())?;
    let changes = render(&base, false).map_err(|error| error.to_string())?;
    if changes.is_empty() {
        println!("render: up to date");
    } else {
        for change in &changes {
            println!("{}", change.line());
        }
    }
    Ok(())
}

/// Runs the `render` subcommand: loads the knowledge base at `root`
/// (resolving the enclosing git repository's top level when `root` is
/// `None`), writes every stale generated file — or, with `check`, only
/// reports which ones are stale — and prints the same messages and exit
/// code as `houserules render`.
pub(crate) fn cmd_render(root: Option<PathBuf>, check: bool) -> ExitCode {
    let root = match crate::root::resolve_root(root) {
        Ok(root) => root,
        Err(code) => return code,
    };
    if !check {
        return match render_and_report(&root) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(2)
            }
        };
    }
    let base = match load_base(&root) {
        Ok(base) => base,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let changes = match render(&base, true) {
        Ok(changes) => changes,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    if !changes.is_empty() {
        for change in &changes {
            eprintln!("{}", change.check_line());
        }
        return ExitCode::from(1);
    }
    println!("render: up to date");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::model::{AreaDef, CheckField, Source, TopicMeta};
    use super::*;

    fn entry(id: &str, kind: &str, area: &str, standing: bool, summary: &str) -> Entry {
        Entry {
            id: id.to_string(),
            kind: kind.to_string(),
            area: area.to_string(),
            standing,
            summary: summary.to_string(),
            check: CheckField::Absent,
            source: Source::Local,
        }
    }

    /// `process` carries two entries (a standing rule, a standing
    /// invariant), `rust` carries three (a rule, a gotcha, and a
    /// `history`-kind entry excluded from the rendered area file but
    /// still counted in the topic line).
    pub(super) fn fixture_base() -> Base {
        let areas = vec![
            (
                "global".to_string(),
                AreaDef {
                    paths: vec![],
                    agents_md: true,
                },
            ),
            (
                "process".to_string(),
                AreaDef {
                    paths: vec![],
                    agents_md: true,
                },
            ),
            (
                "rust".to_string(),
                AreaDef {
                    paths: vec!["crates/**".to_string(), "Cargo.toml".to_string()],
                    agents_md: true,
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
            raw_entries: HashMap::new(),
            topics,
            schema: serde_json::Value::Null,
            areas_raw: serde_json::Value::Null,
            topic_files: Vec::new(),
            ..Base::default()
        }
    }

    /// The paths of `outputs`, in order.
    fn paths(outputs: &[Output]) -> Vec<&str> {
        outputs
            .iter()
            .map(|output| match output {
                Output::File { path, .. }
                | Output::Block { path, .. }
                | Output::Delete { path }
                | Output::DropBlock { path } => path.as_str(),
            })
            .collect()
    }

    /// The Claude renderer is on by default and off for a marker whose
    /// `renderers` lacks `claude`; the AGENTS.md renderer is always on.
    #[test]
    fn the_claude_renderer_follows_the_renderers_key() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut base = fixture_base();
        base.root = dir.path().to_path_buf();

        assert_eq!(
            paths(&plan(&base).expect("plan")),
            ["AGENTS.md", ".claude/rules/rust.md", SKILL_PATH]
        );

        fs::write(dir.path().join(".houserules.json"), "{\"renderers\": []}\n").expect("write");
        assert_eq!(paths(&plan(&base).expect("plan")), ["AGENTS.md"]);

        fs::write(
            dir.path().join(".houserules.json"),
            "{\"renderers\": [\"claude\"]}\n",
        )
        .expect("write");
        assert_eq!(paths(&plan(&base).expect("plan")).len(), 3);
    }

    /// A `renderers` value that is no array of strings is one line that
    /// names the marker, for `render` and for `check-knowledge`.
    #[test]
    fn a_renderers_value_of_the_wrong_shape_is_one_line_that_names_the_marker() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut base = fixture_base();
        base.root = dir.path().to_path_buf();
        fs::write(dir.path().join(".houserules.json"), "{\"renderers\": 5}\n").expect("write");

        let line = ".houserules.json: renderers must be an array of strings";
        assert_eq!(render(&base, true).unwrap_err().to_string(), line);
        assert_eq!(check_findings(&base), [line]);
    }

    /// `render` writes stale files; `--check` only lists them.
    #[test]
    fn render_writes_stale_files_check_only_lists_them() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut base = fixture_base();
        base.root = dir.path().to_path_buf();
        let written = |path: &str| Change {
            path: path.to_string(),
            kind: ChangeKind::Written,
        };
        let expected = vec![
            written("AGENTS.md"),
            written(".claude/rules/rust.md"),
            written(SKILL_PATH),
        ];

        assert_eq!(render(&base, true).expect("render check"), expected);
        assert!(!dir.path().join(SKILL_PATH).exists());
        assert!(!dir.path().join("AGENTS.md").exists());

        assert_eq!(render(&base, false).expect("render write"), expected);
        let rust_md = fs::read_to_string(dir.path().join(".claude/rules/rust.md")).unwrap();
        assert!(rust_md.contains("# Rust rules"));
        assert!(
            fs::read_to_string(dir.path().join("AGENTS.md"))
                .unwrap()
                .contains("- [process.sequential] Run agents sequentially.")
        );
        assert!(render(&base, false).expect("render clean").is_empty());
    }

    /// A block that goes with its file is one change in a `--check` run, the
    /// deletion, and two changes in a run that writes.
    #[test]
    fn a_block_that_goes_with_its_file_is_one_change_when_checking() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("AGENTS.md"), format!("{}\n", block::BEGIN)).expect("write");
        fs::write(
            dir.path().join("only.md"),
            format!("{}\nx\n{}\n", block::BEGIN, block::END),
        )
        .expect("write");
        let outputs = vec![Output::DropBlock {
            path: "only.md".to_string(),
        }];

        let checked = apply(dir.path(), &outputs, true).expect("check");
        assert_eq!(
            checked,
            [Change {
                path: "only.md".to_string(),
                kind: ChangeKind::Deleted
            }]
        );
        assert!(dir.path().join("only.md").exists());

        let applied = apply(dir.path(), &outputs, false).expect("apply");
        assert_eq!(
            applied.iter().map(|change| change.kind).collect::<Vec<_>>(),
            [ChangeKind::BlockRemoved, ChangeKind::Deleted]
        );
        assert!(!dir.path().join("only.md").exists());
    }

    /// A marker fault in any block file stops the run before the first write.
    #[test]
    fn a_marker_fault_stops_the_run_before_any_write() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("broken.md"), format!("{}\n", block::BEGIN)).expect("write");
        let outputs = vec![
            Output::File {
                path: "first.md".to_string(),
                content: "x\n".to_string(),
                class: Class::AreaFile,
            },
            Output::Block {
                path: "broken.md".to_string(),
                block: format!("{}\n{}", block::BEGIN, block::END),
                class: Class::Directory,
            },
        ];

        let error = apply(dir.path(), &outputs, false).unwrap_err();

        assert_eq!(
            error.to_string(),
            "broken.md: the houserules block has no end marker <!-- houserules:end -->; add the end marker after the block"
        );
        assert!(!dir.path().join("first.md").exists());
    }
}
