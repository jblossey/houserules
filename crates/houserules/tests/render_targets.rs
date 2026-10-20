//! The render targets of batch 33 (spec section 7): the AGENTS.md renderer,
//! which is always on and owns one marked block of each AGENTS.md it writes
//! to, and the Claude renderer, which `renderers` in `.houserules.json`
//! enables. Each case runs the compiled binary against a scratch git
//! repository that `houserules init` seeded.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;

/// The line that opens the block that `render` owns.
const BEGIN: &str = "<!-- houserules:begin -->";
/// The line that closes the block that `render` owns.
const END: &str = "<!-- houserules:end -->";
/// The header of every generated file and block.
const GENERATED: &str = "Generated from knowledge/ by houserules render. Do not edit.";

/// A `Command` for the compiled `houserules` binary under test.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// Runs `git` in `dir` and requires success.
fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The text of a captured stream.
fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("utf8 output")
}

/// The lines of a captured stream.
fn lines(bytes: &[u8]) -> Vec<String> {
    text(bytes).lines().map(str::to_string).collect()
}

/// The block of `file`: the lines from the begin marker to the end marker,
/// both included, joined by `\n`.
fn block_of(file: &str) -> String {
    let all: Vec<&str> = file.lines().collect();
    let start = all
        .iter()
        .position(|line| line.trim() == BEGIN)
        .expect("a begin marker");
    let end = all
        .iter()
        .position(|line| line.trim() == END)
        .expect("an end marker");
    all[start..=end].join("\n")
}

/// A scratch install that `houserules init` seeded.
struct Install {
    dir: TempDir,
}

impl Install {
    /// `git init` and `houserules init`, with `--target claude` when
    /// `claude`. The seeded `AGENTS.md` and the generated files are rendered.
    fn new(claude: bool) -> Self {
        let install = Self {
            dir: tempfile::tempdir().expect("tempdir"),
        };
        git(install.root(), &["init", "-q"]);
        let mut command = houserules();
        command.args(["init", "--dir"]).arg(install.root());
        if claude {
            command.args(["--target", "claude"]);
        }
        let output = command.output().expect("run init");
        assert!(
            output.status.success(),
            "init failed: {}",
            text(&output.stderr)
        );
        install
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// The path of `relative` (`/`-separated) under the root, built one
    /// component at a time (`houserules.path-pins-mirror-the-code`).
    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root().to_path_buf(), |path, part| path.join(part))
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the parent");
        fs::write(path, content).expect("write a file");
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).expect("read a file")
    }

    fn exists(&self, relative: &str) -> bool {
        self.path(relative).exists()
    }

    fn read_json(&self, relative: &str) -> Value {
        serde_json::from_str(&self.read(relative)).expect("parse a JSON file")
    }

    fn write_json(&self, relative: &str, value: &Value) {
        self.write(
            relative,
            &format!(
                "{}\n",
                serde_json::to_string_pretty(value).expect("serialize")
            ),
        );
    }

    /// Runs `houserules <args>` in the install root.
    fn run(&self, args: &[&str]) -> Output {
        houserules()
            .args(args)
            .current_dir(self.root())
            .output()
            .expect("run houserules")
    }

    /// Runs `render` and requires exit 0; returns the printed lines.
    fn render(&self) -> Vec<String> {
        let output = self.run(&["render"]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "render failed: {}",
            text(&output.stderr)
        );
        assert_eq!(text(&output.stderr), "", "render prints nothing to stderr");
        lines(&output.stdout)
    }

    /// Runs `render --check` and returns the output.
    fn check(&self) -> Output {
        self.run(&["render", "--check"])
    }

    /// Sets `agentsMd` of an existing area.
    fn set_agents_md(&self, area: &str, value: Option<bool>) {
        let mut areas = self.read_json("knowledge/areas.json");
        match value {
            Some(flag) => areas[area]["agentsMd"] = json!(flag),
            None => {
                areas[area]
                    .as_object_mut()
                    .expect("an area")
                    .remove("agentsMd");
            }
        }
        self.write_json("knowledge/areas.json", &areas);
    }

    /// Adds or replaces the area `name`.
    fn set_area(&self, name: &str, globs: &[&str]) {
        let mut areas = self.read_json("knowledge/areas.json");
        areas[name] = json!({"paths": globs});
        self.write_json("knowledge/areas.json", &areas);
    }

    /// Removes the area `name`.
    fn remove_area(&self, name: &str) {
        let mut areas = self.read_json("knowledge/areas.json");
        areas.as_object_mut().expect("areas").remove(name);
        self.write_json("knowledge/areas.json", &areas);
    }

    /// Writes the local topic `local` with one entry of `kind` in `area` for
    /// each `(id, summary)`.
    fn set_local_entries(&self, area: &str, kind: &str, entries: &[(&str, &str)]) {
        let list: Vec<Value> = entries
            .iter()
            .map(|(id, summary)| {
                json!({
                    "id": id,
                    "kind": kind,
                    "area": area,
                    "standing": false,
                    "summary": summary,
                    "body": ["Body."],
                    "tags": [],
                    "source": {"date": "2026-10-20", "by": "controller"},
                })
            })
            .collect();
        self.write_json(
            "knowledge/local.json",
            &json!({"topic": "local", "title": "Local", "entries": list}),
        );
    }

    /// The `.md` files below `.claude/rules/`, sorted.
    fn rule_files(&self) -> Vec<String> {
        let mut names: Vec<String> = match fs::read_dir(self.path(".claude/rules")) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect(),
            Err(_) => Vec::new(),
        };
        names.sort();
        names
    }
}

/// An `AGENTS.md` whose block holds `stale` between the markers, with the
/// `eol` line ending, project text before the block and after it.
fn agents_md_with_project_text(eol: &str, stale: &str) -> String {
    [
        "# Project notes",
        "Keep this line.",
        BEGIN,
        stale,
        END,
        "After the block.",
        "Keep this one too.",
        "",
    ]
    .join(eol)
}

// ---- the AGENTS.md renderer is always on (spec section 7) ----

/// A fresh `init` without `--target claude` renders the root block and the
/// `tools` directory block, and writes none of the Claude files.
#[test]
fn init_without_the_claude_target_renders_the_agents_md_blocks_only() {
    let install = Install::new(false);

    assert!(install.read("AGENTS.md").contains(BEGIN));
    assert!(install.read("tools/AGENTS.md").contains(BEGIN));
    assert!(!install.exists(".claude/rules"), "no Claude rules");
    assert!(
        !install.exists(".claude/skills/project-knowledge"),
        "no knowledge skill"
    );
    assert_eq!(install.render(), ["render: up to date"]);
}

/// With the Claude renderer, `init --target claude` writes the area files
/// and the knowledge skill, and never `standing-rules.md`.
#[test]
fn init_with_the_claude_target_writes_the_area_files_and_no_standing_rules_file() {
    let install = Install::new(true);

    assert_eq!(install.rule_files(), ["docs.md", "tools.md"]);
    assert!(install.exists(".claude/skills/project-knowledge/SKILL.md"));
    assert!(!install.exists(".claude/rules/standing-rules.md"));
    assert!(install.read("AGENTS.md").contains(BEGIN));
    assert_eq!(install.render(), ["render: up to date"]);
}

/// `render` prints one `written` line for each file it writes, the
/// AGENTS.md renderer first and the Claude renderer after it, the
/// knowledge skill last.
#[test]
fn render_reports_the_agents_md_files_before_the_claude_files() {
    let install = Install::new(true);
    fs::remove_file(install.path("AGENTS.md")).expect("remove AGENTS.md");
    fs::remove_file(install.path(".claude/skills/project-knowledge/SKILL.md")).expect("remove");
    fs::remove_file(install.path(".claude/rules/docs.md")).expect("remove");
    fs::remove_file(install.path("tools/AGENTS.md")).expect("remove");

    assert_eq!(
        install.render(),
        [
            "AGENTS.md: written",
            "tools/AGENTS.md: written",
            ".claude/rules/docs.md: written",
            ".claude/skills/project-knowledge/SKILL.md: written",
        ]
    );
}

/// `renderers` in `.houserules.json` decides the Claude renderer: a list
/// without `claude` writes none of its files, and a marker without the key
/// reads as `["claude"]`.
#[test]
fn the_renderers_key_decides_the_claude_renderer() {
    let install = Install::new(false);
    let mut marker = install.read_json(".houserules.json");
    assert_eq!(marker["renderers"], json!([]));

    marker["renderers"] = json!(["claude"]);
    install.write_json(".houserules.json", &marker);
    assert_eq!(
        install.render(),
        [
            ".claude/rules/docs.md: written",
            ".claude/rules/tools.md: written",
            ".claude/skills/project-knowledge/SKILL.md: written",
        ]
    );

    marker.as_object_mut().unwrap().remove("renderers");
    install.write_json(".houserules.json", &marker);
    assert_eq!(install.render(), ["render: up to date"]);
}

// ---- the owned block (spec section 7, design 5.115) ----

/// A file with no markers gets the block appended at its end; the project
/// text stays byte for byte, and a second `render` changes nothing.
#[test]
fn a_file_without_markers_gets_the_block_appended_at_its_end() {
    let install = Install::new(false);
    install.write("AGENTS.md", "# Mine\r\nLine two.\r\n");

    assert_eq!(install.render(), ["AGENTS.md: written"]);

    let written = install.read("AGENTS.md");
    assert!(
        written.starts_with("# Mine\r\nLine two.\r\n<!-- houserules:begin -->\r\n"),
        "got {written:?}"
    );
    assert!(
        written.ends_with("<!-- houserules:end -->\r\n"),
        "got {written:?}"
    );
    assert!(
        !written.replace("\r\n", "").contains('\n'),
        "every line ending follows the file: {written:?}"
    );
    assert_eq!(install.render(), ["render: up to date"]);
}

/// Once the markers exist, the block stays where the project put it, and
/// the text before and after it stays byte for byte.
#[test]
fn the_block_stays_where_the_project_put_it() {
    let install = Install::new(false);
    install.write(
        "AGENTS.md",
        &agents_md_with_project_text("\n", "OLD-BLOCK-TEXT"),
    );

    assert_eq!(install.render(), ["AGENTS.md: written"]);

    let written = install.read("AGENTS.md");
    assert!(written.starts_with("# Project notes\nKeep this line.\n<!-- houserules:begin -->\n"));
    assert!(written.ends_with("<!-- houserules:end -->\nAfter the block.\nKeep this one too.\n"));
    assert!(!written.contains("OLD-BLOCK-TEXT"));
    assert_eq!(written.matches(BEGIN).count(), 1);
}

/// A begin marker with no end marker is one line that names the file, for
/// `render` and for `render --check`; neither writes anything.
#[test]
fn a_missing_end_marker_is_one_line_that_names_the_file() {
    let install = Install::new(false);
    let broken = format!("# Mine\n{BEGIN}\nhalf a block\n");
    install.write("AGENTS.md", &broken);
    let expected = "AGENTS.md: the houserules block has no end marker <!-- houserules:end -->; add the end marker after the block\n";

    for args in [&["render"][..], &["render", "--check"][..]] {
        let output = install.run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&output.stdout), "", "{args:?}");
        assert_eq!(text(&output.stderr), expected, "{args:?}");
    }
    assert_eq!(install.read("AGENTS.md"), broken);
}

/// Two blocks in one file are one line that names the file.
#[test]
fn two_blocks_in_one_file_are_one_line_that_names_the_file() {
    let install = Install::new(false);
    let doubled = format!("{BEGIN}\na\n{END}\ntext\n{BEGIN}\nb\n{END}\n");
    install.write("AGENTS.md", &doubled);
    let expected = "AGENTS.md: the file holds more than one houserules block; keep one block\n";

    for args in [&["render"][..], &["render", "--check"][..]] {
        let output = install.run(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert_eq!(text(&output.stdout), "", "{args:?}");
        assert_eq!(text(&output.stderr), expected, "{args:?}");
    }
    assert_eq!(install.read("AGENTS.md"), doubled);
}

/// An end marker with no begin marker before it is one line that names
/// the file.
#[test]
fn an_end_marker_without_a_begin_marker_is_one_line_that_names_the_file() {
    let install = Install::new(false);
    install.write("AGENTS.md", &format!("text\n{END}\n"));

    let output = install.run(&["render"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        text(&output.stderr),
        "AGENTS.md: the houserules end marker has no begin marker before it; add the begin marker before the block\n"
    );
}

/// A marker is a line of its own. Text that mentions a marker inside a line
/// is project text: `render` appends a block and leaves the mention alone.
#[test]
fn a_marker_inside_a_line_is_project_text() {
    let install = Install::new(false);
    let prose = format!("Write `{BEGIN}` and `{END}` around the block.\n");
    install.write("AGENTS.md", &prose);

    assert_eq!(install.render(), ["AGENTS.md: written"]);

    let written = install.read("AGENTS.md");
    assert!(written.starts_with(&prose), "got {written:?}");
    assert_eq!(block_of(&written).lines().next(), Some(BEGIN));
    assert_eq!(install.render(), ["render: up to date"]);
}

/// A marker line counts wherever it sits, a fenced code block included: the
/// scan reads lines and models no Markdown (the module doc of
/// `rules::render::block` states the limit). A directory `AGENTS.md` that
/// shows an example block in a fence loses the example lines, and the fence
/// stays.
#[test]
fn a_marker_pair_inside_a_fenced_code_block_is_removed_with_its_lines() {
    let install = Install::new(false);
    install.write(
        "docs/AGENTS.md",
        &format!(
            "# Docs notes\n\nAn example block:\n\n```md\n{BEGIN}\nexample\n{END}\n```\n\nEnd of notes.\n"
        ),
    );

    let check = install.check();
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(text(&check.stderr), "docs/AGENTS.md: would change\n");

    assert_eq!(install.render(), ["docs/AGENTS.md: block removed"]);
    assert_eq!(
        install.read("docs/AGENTS.md"),
        "# Docs notes\n\nAn example block:\n\n```md\n```\n\nEnd of notes.\n"
    );
    assert_eq!(install.render(), ["render: up to date"]);
}

// ---- the root block (spec section 7, design 5.163) ----

/// The root block holds the standing rules, then one section for each area
/// that has globs. A section names the globs and the command. An area with
/// no globs has no section.
#[test]
fn the_root_block_holds_the_standing_rules_and_one_section_for_each_area_with_globs() {
    let install = Install::new(false);
    let root_block = block_of(&install.read("AGENTS.md"));
    let standing: Value =
        serde_json::from_slice(&install.run(&["standing"]).stdout).expect("standing is JSON");
    let rows = standing.as_array().expect("an array of rows");

    assert!(root_block.starts_with(&format!("{BEGIN}\n{GENERATED}\n\n## Standing rules\n\n- [")));
    assert!(root_block.ends_with(&format!("\n{END}")));
    assert!(!rows.is_empty());
    for row in rows {
        let id = row["id"].as_str().expect("an id");
        let summary = row["summary"].as_str().expect("a summary");
        assert!(
            root_block.contains(&format!("\n- [{id}] {summary}\n")),
            "the root block lacks the standing rule {id}"
        );
    }
    assert!(root_block.contains(
        "\n### docs\n\nGlobs: `docs/**`, `AGENTS.md`, `knowledge/**`, `backlog/**`, `.claude/**`\nRun `houserules for <path>` before you change a file that matches.\n"
    ));
    assert!(root_block.contains(
        "\n### tools\n\nGlobs: `tools/**`\nRun `houserules for <path>` before you change a file that matches.\n"
    ));
    assert!(!root_block.contains("### global"));
    assert!(!root_block.contains("### process"));
}

/// The root block never carries the text of an area rule, and it has one
/// shape whatever renderers are enabled.
#[test]
fn the_root_block_carries_no_area_rule_text_and_has_one_shape_for_every_renderer() {
    let without_claude = Install::new(false);
    let with_claude = Install::new(true);
    without_claude.set_local_entries(
        "tools",
        "rule",
        &[("local.area-rule", "An area rule text.")],
    );
    with_claude.set_local_entries(
        "tools",
        "rule",
        &[("local.area-rule", "An area rule text.")],
    );
    without_claude.render();
    with_claude.render();

    let plain = block_of(&without_claude.read("AGENTS.md"));
    let claude = block_of(&with_claude.read("AGENTS.md"));

    assert_eq!(plain, claude);
    assert!(!plain.contains("An area rule text."));
    assert!(!plain.contains("local.area-rule"));
}

/// An area whose globs span more than one directory reaches a harness
/// other than Claude Code through `houserules for` only: no directory
/// block names its rules.
#[test]
fn a_multi_directory_area_reaches_other_harnesses_through_for_only() {
    let install = Install::new(false);
    install.set_local_entries("docs", "rule", &[("local.docs-rule", "A docs rule text.")]);

    install.render();

    for file in ["AGENTS.md", "tools/AGENTS.md"] {
        assert!(!install.read(file).contains("A docs rule text."), "{file}");
    }
    let output = install.run(&["for", "docs/guide.md"]);
    assert!(text(&output.stdout).contains("local.docs-rule"));
}

// ---- directory blocks (spec section 7, design 5.167) ----

/// An area whose globs share one directory prefix gets that directory's
/// block, in a new `AGENTS.md` and in one that holds project text; the block
/// carries the rules of the area. The `tools` area of the kit shows it.
#[test]
fn an_area_with_one_prefix_gets_a_directory_block_with_its_rules() {
    let install = Install::new(false);
    install.set_area("cli", &["crates/x/src/**", "crates/x/src/*.rs"]);
    install.set_local_entries(
        "cli",
        "rule",
        &[
            ("local.cli-rule", "A cli rule text."),
            ("local.cli-gotcha", "Never."),
        ],
    );
    install.write("crates/x/src/AGENTS.md", "Notes for this folder.\n");

    let printed = install.render();

    assert_eq!(
        printed,
        ["AGENTS.md: written", "crates/x/src/AGENTS.md: written"]
    );
    let file = install.read("crates/x/src/AGENTS.md");
    assert!(file.starts_with("Notes for this folder.\n<!-- houserules:begin -->\n"));
    let block = block_of(&file);
    assert_eq!(
        block,
        format!(
            "{BEGIN}\n{GENERATED}\n\n## Cli rules\n\n### Rules\n\n- [local.cli-gotcha] Never.\n- [local.cli-rule] A cli rule text.\n\nDetail: houserules get <id>\n{END}"
        )
    );
    let tools = install.read("tools/AGENTS.md");
    assert!(tools.starts_with(&format!("{BEGIN}\n{GENERATED}\n\n## Tools rules\n")));
}

/// Areas whose globs share a directory share its one block, a section for
/// each area, in the order of `areas.json`.
#[test]
fn areas_that_share_a_directory_share_one_block() {
    let install = Install::new(false);
    install.set_area("cli", &["crates/x/src/**"]);
    install.set_area("cli-extra", &["crates/x/src/*.rs"]);
    install.set_local_entries("cli", "rule", &[("local.cli-rule", "A cli rule text.")]);
    let mut entries = install.read_json("knowledge/local.json");
    entries["entries"]
        .as_array_mut()
        .expect("entries")
        .push(json!({
            "id": "local.extra-gotcha",
            "kind": "gotcha",
            "area": "cli-extra",
            "standing": false,
            "summary": "An extra gotcha.",
            "body": ["Body."],
            "tags": [],
            "source": {"date": "2026-10-20", "by": "controller"},
        }));
    install.write_json("knowledge/local.json", &entries);

    install.render();

    let block = block_of(&install.read("crates/x/src/AGENTS.md"));
    assert_eq!(
        block,
        format!(
            "{BEGIN}\n{GENERATED}\n\n## Cli rules\n\n### Rules\n\n- [local.cli-rule] A cli rule text.\n\n## Cli-extra rules\n\n### Gotchas\n\n- [local.extra-gotcha] An extra gotcha.\n\nDetail: houserules get <id>\n{END}"
        )
    );
    assert_eq!(
        install
            .read("crates/x/src/AGENTS.md")
            .matches(BEGIN)
            .count(),
        1
    );
}

/// An area whose globs share only the install root gets no directory block;
/// its section in the root block names the globs.
#[test]
fn an_area_whose_globs_share_the_install_root_gets_no_directory_block() {
    let install = Install::new(false);
    install.set_area("config", &["README.md", "*.toml"]);
    install.set_local_entries("config", "rule", &[("local.config-rule", "A config rule.")]);

    install.render();

    assert!(
        install
            .read("AGENTS.md")
            .contains("### config\n\nGlobs: `README.md`, `*.toml`\n")
    );
    assert!(!install.exists("README.md"), "no file for the root prefix");
    assert!(!install.read("AGENTS.md").contains("A config rule."));
}

/// An area with `agentsMd` set to false has no directory block, but its
/// section stays in the root block.
#[test]
fn an_area_with_agents_md_false_has_no_directory_block() {
    let install = Install::new(false);
    install.set_agents_md("tools", Some(false));

    let printed = install.render();

    assert_eq!(
        printed,
        ["tools/AGENTS.md: block removed", "tools/AGENTS.md: deleted"]
    );
    assert!(!install.exists("tools/AGENTS.md"));
    assert!(install.read("AGENTS.md").contains("### tools\n"));
}

/// A glob that leaves the install root has no directory block.
#[test]
fn a_prefix_outside_the_install_root_gets_no_block() {
    let install = Install::new(false);
    let name = install
        .root()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("a directory name")
        .to_string();
    install.set_area("outside", &[&format!("../{name}-elsewhere/**")]);
    install.set_local_entries("outside", "rule", &[("local.outside-rule", "Outside.")]);

    install.render();

    let outside = install
        .root()
        .parent()
        .expect("a parent")
        .join(format!("{name}-elsewhere"));
    assert!(!outside.exists(), "render wrote outside the install root");
}

// ---- stale blocks (design 5.167) ----

/// `agentsMd: false`, a changed glob prefix, and a removed area each leave
/// no stale block. A file that held only the block is gone; a file with
/// project text keeps the text byte for byte. `render --check` fails while
/// a block is left.
#[test]
fn a_block_that_render_no_longer_writes_goes_and_the_project_text_stays() {
    let install = Install::new(false);
    install.set_area("cli", &["crates/x/src/**"]);
    install.set_local_entries("cli", "rule", &[("local.cli-rule", "A cli rule text.")]);
    install.write("crates/x/src/AGENTS.md", "Folder notes.\n");
    install.write("crates/y/AGENTS.md", "Other notes.\r\nSecond line.\r\n");
    install.render();
    assert!(install.read("crates/x/src/AGENTS.md").contains(BEGIN));

    // 1. agentsMd false.
    install.set_agents_md("cli", Some(false));
    let check = install.check();
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(
        text(&check.stderr),
        "crates/x/src/AGENTS.md: would change\n"
    );
    assert_eq!(install.render(), ["crates/x/src/AGENTS.md: block removed"]);
    assert_eq!(install.read("crates/x/src/AGENTS.md"), "Folder notes.\n");
    assert_eq!(install.check().status.code(), Some(0));

    // 2. A glob prefix that moves to another directory.
    install.set_agents_md("cli", None);
    install.render();
    install.set_area("cli", &["crates/y/**"]);
    assert_eq!(
        install.render(),
        [
            "AGENTS.md: written",
            "crates/y/AGENTS.md: written",
            "crates/x/src/AGENTS.md: block removed"
        ]
    );
    assert_eq!(install.read("crates/x/src/AGENTS.md"), "Folder notes.\n");
    assert!(
        install
            .read("crates/y/AGENTS.md")
            .starts_with("Other notes.\r\nSecond line.\r\n")
    );

    // 3. A removed area; the file with project text keeps it.
    install.remove_area("cli");
    assert_eq!(
        install.render(),
        ["AGENTS.md: written", "crates/y/AGENTS.md: block removed"]
    );
    assert_eq!(
        install.read("crates/y/AGENTS.md"),
        "Other notes.\r\nSecond line.\r\n"
    );
}

/// A file that held only the block goes with the block, and `render
/// --check` names it while it is left.
#[test]
fn a_file_that_held_only_the_block_is_deleted() {
    let install = Install::new(false);
    install.set_area("cli", &["crates/x/src/**"]);
    install.set_local_entries("cli", "rule", &[("local.cli-rule", "A cli rule text.")]);
    install.render();
    assert!(install.exists("crates/x/src/AGENTS.md"));

    install.remove_area("cli");

    let check = install.check();
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(
        text(&check.stderr),
        "AGENTS.md: would change\ncrates/x/src/AGENTS.md: would be deleted\n"
    );
    assert_eq!(
        install.render(),
        [
            "AGENTS.md: written",
            "crates/x/src/AGENTS.md: block removed",
            "crates/x/src/AGENTS.md: deleted"
        ]
    );
    assert!(!install.exists("crates/x/src/AGENTS.md"));
    assert_eq!(install.render(), ["render: up to date"]);
}

/// A directory `AGENTS.md` with CRLF line endings, project text before and
/// after the block: `render` rewrites the block in CRLF, and removing the
/// block keeps the project text byte for byte.
#[test]
fn removing_a_block_from_a_crlf_file_keeps_the_project_text_byte_for_byte() {
    let install = Install::new(false);
    install.set_area("cli", &["crates/x/src/**"]);
    install.set_local_entries("cli", "rule", &[("local.cli-rule", "A cli rule text.")]);
    install.write(
        "crates/x/src/AGENTS.md",
        &agents_md_with_project_text("\r\n", "stale"),
    );

    install.render();
    let written = install.read("crates/x/src/AGENTS.md");
    assert!(
        written.starts_with("# Project notes\r\nKeep this line.\r\n<!-- houserules:begin -->\r\n")
    );
    assert!(
        written.ends_with("<!-- houserules:end -->\r\nAfter the block.\r\nKeep this one too.\r\n")
    );
    assert!(
        !written.replace("\r\n", "").contains('\n'),
        "the block follows the CRLF of the file: {written:?}"
    );
    assert_eq!(install.render(), ["render: up to date"]);

    install.remove_area("cli");
    install.render();

    assert_eq!(
        install.read("crates/x/src/AGENTS.md"),
        "# Project notes\r\nKeep this line.\r\nAfter the block.\r\nKeep this one too.\r\n"
    );
}

// ---- the search set (spec section 7) ----

/// A submodule and a vendored directory with its own `.houserules.json`,
/// each holding an `AGENTS.md` with a block, are left untouched by `render`
/// and `render --check`, even when an area names them.
#[test]
fn a_submodule_and_a_vendored_install_are_left_untouched() {
    let install = Install::new(false);
    let inner = tempfile::tempdir().expect("tempdir");
    git(inner.path(), &["init", "-q"]);
    let stale = format!("{BEGIN}\nsubmodule block\n{END}\n");
    fs::write(inner.path().join("AGENTS.md"), &stale).expect("write the inner file");
    // A Windows runner sets core.autocrlf, which would check the submodule out with CRLF.
    fs::write(inner.path().join(".gitattributes"), "* -text\n").expect("write .gitattributes");
    git(inner.path(), &["add", "-A"]);
    git(
        inner.path(),
        &[
            "-c",
            "user.name=test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-q",
            "-m",
            "inner",
        ],
    );
    git(
        install.root(),
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            inner.path().to_str().expect("utf8 path"),
            "vendor-sub",
        ],
    );
    install.write(
        "vendor-copy/.houserules.json",
        "{\"schema\":\"houserules/install@2.0.0\"}\n",
    );
    install.write(
        "vendor-copy/AGENTS.md",
        &format!("{BEGIN}\nvendored block\n{END}\n"),
    );
    install.write(
        "vendor-copy/deep/AGENTS.md",
        &format!("{BEGIN}\ndeep block\n{END}\n"),
    );
    git(install.root(), &["add", "-A"]);
    install.set_area("sub", &["vendor-sub/**"]);
    install.set_area("copy", &["vendor-copy/**"]);
    install.set_local_entries("sub", "rule", &[("local.sub-rule", "Sub rule.")]);

    let printed = install.render();

    assert_eq!(printed, ["AGENTS.md: written"]);
    assert_eq!(
        fs::read_to_string(install.path("vendor-sub/AGENTS.md")).expect("read"),
        stale
    );
    assert_eq!(
        install.read("vendor-copy/AGENTS.md"),
        format!("{BEGIN}\nvendored block\n{END}\n")
    );
    assert_eq!(
        install.read("vendor-copy/deep/AGENTS.md"),
        format!("{BEGIN}\ndeep block\n{END}\n")
    );
    assert_eq!(install.check().status.code(), Some(0));
}

/// A tracked `AGENTS.md` that the working tree lacks holds no block and is
/// skipped. A file that is neither tracked nor ignored is searched; an
/// ignored one is not.
#[test]
fn the_search_reads_what_git_lists() {
    let install = Install::new(false);
    install.write("gone/AGENTS.md", &format!("{BEGIN}\nold\n{END}\n"));
    git(install.root(), &["add", "gone/AGENTS.md"]);
    fs::remove_file(install.path("gone/AGENTS.md")).expect("remove the tracked file");
    install.write(
        "fresh/AGENTS.md",
        &format!("{BEGIN}\nuntracked block\n{END}\n"),
    );
    install.write(".gitignore", "ignored/\n");
    install.write(
        "ignored/AGENTS.md",
        &format!("{BEGIN}\nignored block\n{END}\n"),
    );

    let printed = install.render();

    assert_eq!(
        printed,
        ["fresh/AGENTS.md: block removed", "fresh/AGENTS.md: deleted"]
    );
    assert!(!install.exists("gone/AGENTS.md"));
    assert!(install.read("ignored/AGENTS.md").contains("ignored block"));
}

/// Outside a git repository the search walks the directory tree.
#[test]
fn without_git_the_search_walks_the_tree() {
    let install = Install::new(false);
    fs::remove_dir_all(install.path(".git")).expect("remove .git");
    install.write("deep/er/AGENTS.md", &format!("{BEGIN}\nleft\n{END}\n"));

    let output = houserules()
        .args(["render", "--dir"])
        .arg(install.root())
        .output()
        .expect("run render");

    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        lines(&output.stdout),
        [
            "deep/er/AGENTS.md: block removed",
            "deep/er/AGENTS.md: deleted"
        ]
    );
}

// ---- the Claude renderer (spec section 7, C19) ----

/// An area file that carries the generated header and that the Claude
/// renderer no longer writes is deleted, with one line. `render --check`
/// fails while it is left, and `check-knowledge` reports it.
#[test]
fn the_claude_renderer_deletes_a_generated_file_it_no_longer_writes() {
    let install = Install::new(true);
    let doomed = install.read(".claude/rules/tools.md");
    install.write(".claude/rules/old-area.md", &doomed);
    install.write(
        ".claude/rules/standing-rules.md",
        &format!("{GENERATED}\n\n# Standing rules\n"),
    );

    let check = install.check();
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(
        text(&check.stderr),
        ".claude/rules/old-area.md: would be deleted\n.claude/rules/standing-rules.md: would be deleted\n"
    );
    let checked = install.run(&["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(
        lines(&checked.stderr),
        [
            ".claude/rules/old-area.md: generated file is no longer written (run houserules render)",
            ".claude/rules/standing-rules.md: generated file is no longer written (run houserules render)",
        ]
    );

    assert_eq!(
        install.render(),
        [
            ".claude/rules/old-area.md: deleted",
            ".claude/rules/standing-rules.md: deleted",
        ]
    );
    assert_eq!(install.rule_files(), ["docs.md", "tools.md"]);
    assert_eq!(install.check().status.code(), Some(0));
    assert_eq!(install.run(&["check-knowledge"]).status.code(), Some(0));
}

/// The probe of Task 6's re-review: `pack remove` of the pack that holds an
/// area's only entries leaves `.claude/rules/<area>.md` behind. The render
/// that `pack remove` runs deletes it, and both gates pass.
#[test]
fn pack_remove_deletes_the_area_file_of_an_area_without_entries() {
    let install = Install::new(true);
    assert!(install.exists(".claude/rules/tools.md"));

    let removed = install.run(&["pack", "remove", "quality"]);

    assert_eq!(removed.status.code(), Some(0), "{}", text(&removed.stderr));
    assert!(!install.exists(".claude/rules/tools.md"));
    assert!(
        lines(&removed.stdout).contains(&".claude/rules/tools.md: deleted".to_string()),
        "got {:?}",
        lines(&removed.stdout)
    );
    assert_eq!(install.check().status.code(), Some(0));
}

/// A file in `.claude/rules/` without the generated header stays a finding
/// of `check-knowledge`, and its line says why: it has no header. `render`
/// never deletes it.
#[test]
fn a_rules_file_without_the_generated_header_is_a_finding_and_stays() {
    let install = Install::new(true);
    install.write(".claude/rules/mine.md", "# My own rule\n");

    assert_eq!(install.render(), ["render: up to date"]);
    let checked = install.run(&["check-knowledge"]);

    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(
        lines(&checked.stderr),
        [
            ".claude/rules/mine.md: not generated by houserules (it has no generated header); remove it"
        ]
    );
    assert!(install.exists(".claude/rules/mine.md"));
}

/// An install without the Claude renderer owns no `.claude/rules/`: a file
/// there is no finding.
#[test]
fn an_install_without_the_claude_renderer_owns_no_rules_directory() {
    let install = Install::new(false);
    install.write(".claude/rules/mine.md", "# My own rule\n");

    assert_eq!(install.render(), ["render: up to date"]);
    assert_eq!(install.run(&["check-knowledge"]).status.code(), Some(0));
}

/// The knowledge skill has no standing section, and its retrieval protocol
/// tells the agent to save houserules output to a file and read the file
/// with its file-read tool.
#[test]
fn the_knowledge_skill_has_no_standing_section_and_reads_output_from_a_file() {
    let install = Install::new(true);

    let skill = install.read(".claude/skills/project-knowledge/SKILL.md");

    assert!(!skill.contains("## Standing rules"));
    assert!(skill.contains(
        "## Retrieval protocol\n\nSave the output of every `houserules` command to a file, then read the file with your file-read tool.\n\n1. Resolve every id"
    ));
    assert!(skill.contains("## Topics\n\n"));
}

// ---- Claude Code and another harness read one source (spec section 16) ----

/// Each area's rules reach a harness without Claude Code from the directory
/// block or `houserules for`, and reach Claude Code from
/// `.claude/rules/<area>.md`: one source for each harness. The root block
/// carries neither.
#[test]
fn each_area_rule_reaches_each_harness_from_one_source() {
    let install = Install::new(false);
    install.set_area("cli", &["crates/x/src/**"]);
    install.set_local_entries(
        "cli",
        "rule",
        &[("local.cli-rule", "The cli rule text, unique.")],
    );
    install.remove_file_if_present("CLAUDE.md");

    // A Codex-style read: no CLAUDE.md, no Claude renderer.
    install.render();
    let codex_files = ["AGENTS.md", "crates/x/src/AGENTS.md"];
    let holding: Vec<&str> = codex_files
        .iter()
        .copied()
        .filter(|file| install.read(file).contains("The cli rule text, unique."))
        .collect();
    assert_eq!(holding, ["crates/x/src/AGENTS.md"]);
    assert!(!install.exists(".claude/rules"));
    let via_for = install.run(&["for", "crates/x/src/lib.rs"]);
    assert!(text(&via_for.stdout).contains("local.cli-rule"));

    // A Claude Code read: the Claude renderer on, CLAUDE.md importing AGENTS.md.
    let mut marker = install.read_json(".houserules.json");
    marker["renderers"] = json!(["claude"]);
    install.write_json(".houserules.json", &marker);
    install.write("CLAUDE.md", "@AGENTS.md\n");
    install.render();
    assert!(
        install
            .read(".claude/rules/cli.md")
            .contains("- [local.cli-rule] The cli rule text, unique.")
    );
    assert!(
        install
            .read(".claude/rules/cli.md")
            .starts_with("---\npaths:\n  - \"crates/x/src/**\"\n---\n")
    );
    assert!(
        !install
            .read("AGENTS.md")
            .contains("The cli rule text, unique.")
    );
    assert_eq!(install.run(&["check-knowledge"]).status.code(), Some(0));
}

impl Install {
    /// Removes `relative` when it exists.
    fn remove_file_if_present(&self, relative: &str) {
        let _ = fs::remove_file(self.path(relative));
    }
}

// ---- the budget (spec sections 7 and 11) ----

/// Writes `count` standing rules of `area` global into the local topic.
fn many_standing_rules(install: &Install, count: usize, summary_len: usize) {
    let list: Vec<Value> = (0..count)
        .map(|n| {
            json!({
                "id": format!("local.rule-{n:03}"),
                "kind": "rule",
                "area": "global",
                "standing": true,
                "summary": "x".repeat(summary_len),
                "body": ["Body."],
                "tags": [],
                "source": {"date": "2026-10-20", "by": "controller"},
            })
        })
        .collect();
    install.write_json(
        "knowledge/local.json",
        &json!({"topic": "local", "title": "Local", "entries": list}),
    );
}

/// The standing section of the root block has the 1.4.0 standing limit of 60
/// lines, read from the generated text: the hand-written lines of the file
/// do not count. The section is its heading, a blank line, and the rules.
#[test]
fn the_standing_section_of_the_root_block_has_a_limit_of_60_lines() {
    let install = Install::new(false);
    many_standing_rules(&install, 80, 20);
    install.render();
    let block = block_of(&install.read("AGENTS.md"));
    let rules = block
        .lines()
        .take_while(|line| *line != "## Area rules")
        .filter(|line| line.starts_with("- ["))
        .count();
    assert!(
        rules > 80,
        "the kit's standing rules and the 80 local ones: {rules}"
    );

    let checked = install.run(&["check-knowledge"]);

    assert_eq!(checked.status.code(), Some(1));
    let findings = lines(&checked.stderr);
    let expected = format!(
        "AGENTS.md: the standing rules have {} lines, budget 60",
        rules + 2
    );
    assert!(
        findings.contains(&expected),
        "expected {expected:?} in {findings:?}"
    );
}

/// The root block has the 1.4.0 AGENTS.md limits of 200 lines and 12288
/// bytes, read from the block alone: 400 hand-written lines around it do
/// not count.
#[test]
fn the_root_block_has_the_agents_md_limits_of_200_lines_and_12288_bytes() {
    let install = Install::new(false);
    many_standing_rules(&install, 80, 150);
    install.write(
        "AGENTS.md",
        &format!("{}{BEGIN}\n{END}\n", "hand-written\n".repeat(400)),
    );
    install.render();

    let findings = lines(&install.run(&["check-knowledge"]).stderr);

    assert!(
        findings.iter().any(
            |line| line.starts_with("AGENTS.md: the houserules block has ")
                && line.ends_with(" bytes, budget 12288")
        ),
        "got {findings:?}"
    );
    assert!(
        !findings
            .iter()
            .any(|line| line.ends_with(" lines, budget 200")),
        "the 400 hand-written lines are not the block: {findings:?}"
    );
}

/// A directory block has the area limit of 160 lines.
#[test]
fn a_directory_block_has_a_limit_of_160_lines() {
    let install = Install::new(false);
    install.set_area("cli", &["crates/x/src/**"]);
    let entries: Vec<(String, String)> = (0..170)
        .map(|n| (format!("local.cli-{n:03}"), "Rule.".to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = entries
        .iter()
        .map(|(id, summary)| (id.as_str(), summary.as_str()))
        .collect();
    install.set_local_entries("cli", "rule", &refs);
    install.render();

    let findings = lines(&install.run(&["check-knowledge"]).stderr);

    assert!(
        findings.contains(
            &"crates/x/src/AGENTS.md: the houserules block has 180 lines, budget 160".to_string()
        ),
        "got {findings:?}"
    );
}

/// A Claude area file has the area limit, the knowledge skill the skill
/// limit, and each is a finding that names the file.
#[test]
fn the_claude_files_keep_their_limits() {
    let install = Install::new(true);
    let entries: Vec<(String, String)> = (0..170)
        .map(|n| (format!("local.cli-{n:03}"), "Rule.".to_string()))
        .collect();
    let refs: Vec<(&str, &str)> = entries
        .iter()
        .map(|(id, summary)| (id.as_str(), summary.as_str()))
        .collect();
    install.set_area("cli", &["crates/x/src/**", "crates/y/**"]);
    install.set_local_entries("cli", "rule", &refs);
    install.render();

    let findings = lines(&install.run(&["check-knowledge"]).stderr);

    assert!(
        findings
            .iter()
            .any(|line| line.starts_with(".claude/rules/cli.md: ")
                && line.ends_with(" lines, budget 160")),
        "got {findings:?}"
    );
}

/// `CLAUDE.md` keeps its 1.4.0 budget, and its absence is never a finding.
#[test]
fn claude_md_keeps_its_budget_and_its_absence_is_no_finding() {
    let install = Install::new(false);
    install.remove_file_if_present("CLAUDE.md");
    assert_eq!(install.run(&["check-knowledge"]).status.code(), Some(0));

    install.write(
        "CLAUDE.md",
        &format!("@AGENTS.md\n{}", "line\n".repeat(250)),
    );
    let findings = lines(&install.run(&["check-knowledge"]).stderr);

    assert_eq!(findings, ["CLAUDE.md: 251 lines, budget 200"]);
}

/// A `CLAUDE.md` without the import is a finding with or without the Claude
/// renderer (C15).
#[test]
fn a_claude_md_without_the_import_is_a_finding_for_every_renderer() {
    for claude in [false, true] {
        let install = Install::new(claude);
        install.write("CLAUDE.md", "My own text.\n");

        let checked = install.run(&["check-knowledge"]);

        assert_eq!(checked.status.code(), Some(1), "claude {claude}");
        assert_eq!(
            lines(&checked.stderr),
            ["CLAUDE.md: the standing rules do not reach Claude Code; add @AGENTS.md to CLAUDE.md"],
            "claude {claude}"
        );
    }
}

/// `check-knowledge` names a generated block that is out of date, and a
/// block with broken markers, as findings.
#[test]
fn check_knowledge_reports_a_stale_block_and_a_broken_block() {
    let install = Install::new(false);
    let fresh = install.read("AGENTS.md");
    install.write(
        "AGENTS.md",
        &fresh.replace("## Standing rules", "## Old standing rules"),
    );

    let stale = install.run(&["check-knowledge"]);
    assert_eq!(
        lines(&stale.stderr),
        ["AGENTS.md: generated file is out of date (run houserules render)"]
    );

    install.write("AGENTS.md", &format!("{BEGIN}\nhalf\n"));
    let broken = install.run(&["check-knowledge"]);
    assert_eq!(
        lines(&broken.stderr),
        [
            "AGENTS.md: the houserules block has no end marker <!-- houserules:end -->; add the end marker after the block"
        ]
    );
}
