//! The kit texts that batch 33 changes with the render port (task 8): the
//! implementer template's standing line, the seeded `AGENTS.md` with its
//! marked block, the notes that the agent templates install with
//! `--workflow reference`, and the retire text for a pack entry. The texts are
//! contract: an adopter reads them as printed.

use std::fs;
use std::path::{Path, PathBuf};

/// This checkout's repository root.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The text of the file at `relative` (`/`-separated) below `template/`.
fn template_text(relative: &str) -> String {
    let path = relative
        .split('/')
        .fold(repo_root().join("template"), |path, part| path.join(part));
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The standing line of the implementer template points at `AGENTS.md`
/// (spec section 7, design 5.141(1)): the `project-knowledge` skill no longer
/// holds the standing rules.
#[test]
fn the_implementer_template_points_its_standing_rules_at_agents_md() {
    let template = template_text("workflows/reference/.claude/agents/implementer.md");

    assert!(
        template.lines().any(|line| line
            == "3. The standing rules in the houserules block of `AGENTS.md` bind every change."),
        "the implementer template lacks the new standing line"
    );
    assert!(!template.contains("preloaded `project-knowledge` skill bind"));
}

/// The seeded `AGENTS.md` holds no marker: a file with no markers gets the
/// block appended at its end when `init` renders (design 5.115). A marker in
/// the seed would also make the render of this repository remove it: the
/// area `template` sets `"agentsMd": false`, so `render` writes no block into
/// `template/AGENTS.md` and removes one that it finds there.
#[test]
fn the_seeded_agents_md_holds_no_marker() {
    let seed = template_text("AGENTS.md");

    for marker in ["<!-- houserules:begin -->", "<!-- houserules:end -->"] {
        assert!(
            !seed.lines().any(|line| line.trim() == marker),
            "the seed holds {marker}"
        );
    }
}

/// The seeded `AGENTS.md` says where each kind of rule reaches an agent:
/// the standing rules through the block, the rules of an area through the
/// Claude area files or `houserules for`.
#[test]
fn the_seeded_agents_md_says_where_the_rules_reach_an_agent() {
    let seed = template_text("AGENTS.md");

    assert!(seed.contains(
        "- `.claude/rules/*.md` loads into Claude Code's context when you read a matching file: the rules of one area. `houserules init --target claude` writes these files. The standing rules reach every harness through the houserules block of this file; Claude Code reads it through `CLAUDE.md`. Other harnesses get the rules of an area from the `AGENTS.md` of its directory or from `houserules for <path>`.\n"
    ));
    assert!(!seed.contains("standing rules every session"));
}

/// The files of `template/` below `dir`, as `/`-separated paths relative to
/// `template/`.
fn template_files(dir: &Path, prefix: &str, found: &mut Vec<String>) {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("a directory entry"))
        .collect();
    names.sort_by_key(|entry| entry.file_name());
    for entry in names {
        let name = entry.file_name().into_string().expect("a UTF-8 name");
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type().expect("a file type").is_dir() {
            template_files(&dir.join(&name), &relative, found);
        } else {
            found.push(relative);
        }
    }
}

/// No seeded text outside the reference workflow says that `houserules init
/// --target claude` installs the agent templates: the workflow installs them
/// (spec 4.4, F10), so every seeded text that names them points at
/// `houserules init --workflow reference`. The set derives from the grep of
/// spec section 11: every line of `template/` that matches
/// `\.claude/agents|agent templates|subagent templates|Templates: `, outside
/// the reference workflow (`template/workflows/reference/`) and the process
/// pack, which govern a project that has the templates. The command sits on
/// the line or on the next one, the width of a wrapped Markdown line.
#[test]
fn every_seeded_text_that_names_the_agent_templates_points_at_the_workflow() {
    let mut files = Vec::new();
    template_files(&repo_root().join("template"), "", &mut files);
    let mut hits = 0;
    for file in &files {
        if file.starts_with("workflows/reference/") || file == "packs/process.json" {
            continue;
        }
        let text = template_text(file);
        let lines: Vec<&str> = text.lines().collect();
        for (number, line) in lines.iter().enumerate() {
            let names_the_templates = line.contains(".claude/agents")
                || line.contains("agent templates")
                || line.contains("subagent templates")
                || line.contains("Templates: ");
            if !names_the_templates {
                continue;
            }
            hits += 1;
            let window = format!("{line}\n{}", lines.get(number + 1).copied().unwrap_or(""));
            assert!(
                window.contains("init --workflow reference"),
                "template/{file}:{} names the agent templates without `init --workflow reference`: {line}",
                number + 1
            );
            assert!(
                !window.contains("--target claude"),
                "template/{file}:{} says `--target claude` installs the agent templates: {line}",
                number + 1
            );
        }
    }
    assert_eq!(
        hits, 3,
        "the grep of spec section 11 finds three lines outside the reference workflow and the process pack"
    );
}

/// The retire text of a pack entry: the entry stays in its pack copy, `retire`,
/// `demote`, and `narrow` are overrides, `retire` is an override with
/// `status: retired`, and `mechanize` files an item in the tracker. The
/// orchestrating skill and the process pack carry the same sentences, and the
/// pack names the item a work item, because a pack entry names no JSON backlog
/// (spec section 7).
#[test]
fn the_retire_text_names_the_override_for_a_pack_entry() {
    let retire = "`retire` sets `status: retired` and, for a local entry, runs `houserules archive`. A pack entry stays in its pack copy: a `retire`, `demote`, or `narrow` proposal for it is an override in `knowledge/overrides.json`, and `retire` is an override with `status: retired`.";
    let overrides = "A pack entry that the project retires, demotes, rewrites, or tags `ruled-keep` is an override in `knowledge/overrides.json`, and `houserules update` keeps the override.";

    for (file, mechanize) in [
        (
            "workflows/reference/.claude/skills/orchestrating/SKILL.md",
            "`mechanize` files a backlog item for the check.",
        ),
        (
            "packs/process.json",
            "`mechanize` files a work item for the check.",
        ),
    ] {
        let text = template_text(file);
        assert!(text.contains(retire), "{file} lacks the retire text");
        assert!(text.contains(mechanize), "{file} lacks the mechanize text");
        assert!(
            !text.contains("every proposal for it is an override"),
            "{file} makes a mechanize proposal an override"
        );
        assert!(text.contains(overrides), "{file} lacks the override text");
        assert!(
            !text.contains("and runs `houserules archive`."),
            "{file} keeps the text for a local entry only"
        );
        assert!(
            !text.contains("goes into `.houserules.json` `overrides`"),
            "{file} keeps the 1.x override text"
        );
    }
}
