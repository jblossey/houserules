//! The AGENTS.md renderer: always on. In each `AGENTS.md` it writes to, it
//! owns the block between the markers of [`block`] and nothing else.
//!
//! The root `AGENTS.md` holds the standing rules and one section for each
//! area that has globs. A section names the globs and the command
//! `houserules for <path>`; it never carries the text of an area rule, so the
//! block has one shape whatever renderers are enabled. An area whose globs
//! share one directory prefix below the install root also gets that
//! directory's `AGENTS.md` block, with the rules of the area, unless it sets
//! `"agentsMd": false`. Areas that share a directory share one block.
//!
//! The renderer removes every block it no longer writes from the `AGENTS.md`
//! files that git lists under the install root (tracked, or untracked and not
//! ignored), or that a walk of the tree finds when git cannot list. A
//! directory that holds another `.houserules.json` or a `.git` belongs to
//! another install or repository: the renderer reads and writes none of its
//! files.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use crate::root::INSTALL_MARKER;

use super::super::model::{AreaDef, Base};
use super::block::{BEGIN, END, locate};
use super::{
    Class, GENERATED, Output, RenderError, area_sections, capitalize, read_optional, standing_lines,
};

/// The name of the instruction file that the root and every directory may hold.
const AGENTS_MD: &str = "AGENTS.md";

/// The sentence of an area section that names the command.
const FOR_LINE: &str = "Run `houserules for <path>` before you change a file that matches.";

/// Where the rules of an area go in `AGENTS.md` files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Placement {
    /// The area has no globs: no section, no block.
    NoSection,
    /// The root block names the area; no directory block holds its rules.
    RootOnly,
    /// The root block names the area, and the `AGENTS.md` of this directory
    /// (relative to the install root, `/`-separated) holds its rules.
    Directory(String),
}

/// The directory prefix of `glob`: the leading components that hold no
/// wildcard character, without the last component.
fn directory_prefix(glob: &str) -> String {
    let parts: Vec<&str> = glob.split('/').collect();
    let (_, directories) = parts.split_last().expect("split yields one part at least");
    directories
        .iter()
        .take_while(|part| !part.contains(['*', '?', '[', '{']))
        .copied()
        .collect::<Vec<_>>()
        .join("/")
}

/// `true` when `dir` names a directory below the install root: one or more
/// plain components, none empty, `.`, or `..`.
fn is_below_root(dir: &str) -> bool {
    !dir.is_empty()
        && dir
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}

/// Where the rules of `def` go: the directory its globs share, when they
/// share one below the install root and the area does not turn the block off.
pub(super) fn placement(def: &AreaDef) -> Placement {
    if def.paths.is_empty() {
        return Placement::NoSection;
    }
    if !def.agents_md {
        return Placement::RootOnly;
    }
    let mut prefixes = def.paths.iter().map(|glob| directory_prefix(glob));
    let first = prefixes.next().expect("an area with globs has a prefix");
    if is_below_root(&first) && prefixes.all(|prefix| prefix == first) {
        Placement::Directory(first)
    } else {
        Placement::RootOnly
    }
}

/// The block of the root `AGENTS.md`, and the length of its standing section.
/// The standing rules come first. One section for each area that has globs
/// names the globs and the command that prints the rules of the area; no
/// section carries the text of a rule.
fn root_block(base: &Base) -> (String, usize) {
    let mut standing = vec!["## Standing rules".to_string(), String::new()];
    let rules = standing_lines(base);
    if rules.is_empty() {
        standing.push("No standing rules.".to_string());
    } else {
        standing.extend(rules);
    }
    let standing_len = standing.len();
    let mut lines = vec![BEGIN.to_string(), GENERATED.to_string(), String::new()];
    lines.extend(standing);
    let mut first = true;
    for (name, def) in &base.areas {
        if placement(def) == Placement::NoSection {
            continue;
        }
        if first {
            lines.push(String::new());
            lines.push("## Area rules".to_string());
            first = false;
        }
        let globs = def
            .paths
            .iter()
            .map(|glob| format!("`{glob}`"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(String::new());
        lines.push(format!("### {name}"));
        lines.push(String::new());
        lines.push(format!("Globs: {globs}"));
        lines.push(FOR_LINE.to_string());
    }
    lines.push(END.to_string());
    (lines.join("\n"), standing_len)
}

/// The block of a directory `AGENTS.md`: the rules of each of `areas`, one
/// section for each, in the order given.
fn directory_block(base: &Base, areas: &[&str]) -> String {
    let mut text = format!("{BEGIN}\n{GENERATED}\n\n");
    for area in areas {
        let body = area_sections(base, area, 3).expect("a grouped area has rules");
        text.push_str(&format!("## {} rules\n\n{body}\n", capitalize(area)));
    }
    text.push_str(&format!("Detail: houserules get <id>\n{END}"));
    text
}

/// The directories that get a block, each with its areas, in the order of the
/// first area that names the directory. An area with no rule, invariant, or
/// gotcha gets no block.
fn directory_groups(base: &Base) -> Vec<(String, Vec<&str>)> {
    let mut groups: Vec<(String, Vec<&str>)> = Vec::new();
    for (name, def) in &base.areas {
        let Placement::Directory(dir) = placement(def) else {
            continue;
        };
        if area_sections(base, name, 3).is_none() {
            continue;
        }
        match groups.iter_mut().find(|(known, _)| *known == dir) {
            Some((_, areas)) => areas.push(name),
            None => groups.push((dir, vec![name])),
        }
    }
    groups
}

/// `true` when `dir` (relative to `root`, `/`-separated) or a directory above
/// it below `root` holds another `.houserules.json` or a `.git`: the files
/// there belong to another install or repository, and a submodule counts.
fn belongs_elsewhere(root: &Path, dir: &str) -> bool {
    let mut path = root.to_path_buf();
    for component in dir.split('/').filter(|component| !component.is_empty()) {
        path.push(component);
        if path.join(INSTALL_MARKER).exists() || path.join(".git").exists() {
            return true;
        }
    }
    false
}

/// The fault for a path that is not valid UTF-8.
fn undecodable(raw: &[u8]) -> RenderError {
    RenderError(format!(
        "{}: the path is not valid UTF-8, so render cannot search it for a houserules block",
        String::from_utf8_lossy(raw)
    ))
}

/// The `AGENTS.md` files that git lists under `root`, tracked or untracked
/// and not ignored. `None` when git cannot list: `root` is no work tree, or
/// git is not installed. git lists a submodule as one path and none of its
/// files.
fn git_listing(root: &Path) -> Result<Option<Vec<String>>, RenderError> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            ":(glob)**/AGENTS.md",
        ])
        .current_dir(root)
        .output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    let mut paths = Vec::new();
    for raw in output.stdout.split(|&byte| byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let path = std::str::from_utf8(raw).map_err(|_| undecodable(raw))?;
        paths.push(path.to_string());
    }
    Ok(Some(paths))
}

/// The `AGENTS.md` files below `dir` (the directory `relative` below the
/// install root), found by a walk of the tree that skips every `.git` and
/// follows no symbolic link.
fn walk_listing(root: &Path, relative: &str, found: &mut Vec<String>) -> Result<(), RenderError> {
    let dir = relative
        .split('/')
        .filter(|component| !component.is_empty())
        .fold(root.to_path_buf(), |path, component| path.join(component));
    let shown = if relative.is_empty() { "." } else { relative };
    let entries = fs::read_dir(&dir).map_err(|error| RenderError::io(shown, &error))?;
    for entry in entries {
        let entry = entry.map_err(|error| RenderError::io(shown, &error))?;
        let raw = entry.file_name();
        let name = raw
            .to_str()
            .ok_or_else(|| undecodable(raw.to_string_lossy().as_bytes()))?;
        let path = if relative.is_empty() {
            name.to_string()
        } else {
            format!("{relative}/{name}")
        };
        let kind = entry
            .file_type()
            .map_err(|error| RenderError::io(&path, &error))?;
        if kind.is_dir() {
            if name != ".git" {
                walk_listing(root, &path, found)?;
            }
        } else if name == AGENTS_MD {
            found.push(path);
        }
    }
    Ok(())
}

/// The `AGENTS.md` files that the search set of `render` holds, sorted, as
/// paths relative to `root`.
fn listed_agents_md(root: &Path) -> Result<Vec<String>, RenderError> {
    let mut paths = match git_listing(root)? {
        Some(paths) => paths,
        None => {
            let mut found = Vec::new();
            walk_listing(root, "", &mut found)?;
            found
        }
    };
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// `true` when the file at `path` holds a block. A file that is not there
/// holds none; a file with broken markers is the one line that names it.
fn holds_block(root: &Path, path: &str) -> Result<bool, RenderError> {
    let Some(text) = read_optional(root, path)? else {
        return Ok(false);
    };
    locate(&text)
        .map(|located| located.is_some())
        .map_err(|fault| RenderError(fault.line(path)))
}

/// The outputs of the AGENTS.md renderer: the block of the root `AGENTS.md`,
/// the block of each directory that gets one, and the removal of every block
/// that the renderer no longer writes.
pub(super) fn plan(base: &Base) -> Result<Vec<Output>, RenderError> {
    let (text, standing_lines) = root_block(base);
    let mut outputs = vec![Output::Block {
        path: AGENTS_MD.to_string(),
        block: text,
        class: Class::Root { standing_lines },
    }];
    let mut written: HashSet<String> = HashSet::from([AGENTS_MD.to_string()]);
    for (dir, areas) in directory_groups(base) {
        if belongs_elsewhere(&base.root, &dir) {
            continue;
        }
        let path = format!("{dir}/{AGENTS_MD}");
        written.insert(path.clone());
        outputs.push(Output::Block {
            path,
            block: directory_block(base, &areas),
            class: Class::Directory,
        });
    }
    for path in listed_agents_md(&base.root)? {
        let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        if written.contains(&path) || belongs_elsewhere(&base.root, dir) {
            continue;
        }
        if holds_block(&base.root, &path)? {
            outputs.push(Output::DropBlock { path });
        }
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::tests::fixture_base;
    use super::*;

    /// The areas of the JSON text `areas` as the loader builds them.
    fn areas_of(areas: &Value) -> Vec<(String, AreaDef)> {
        areas
            .as_object()
            .expect("an object")
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    serde_json::from_value(value.clone()).expect("an area"),
                )
            })
            .collect()
    }

    /// The placements of the areas that the file at `relative` (below the
    /// repository root) declares, in file order.
    fn placements_in(relative: &str) -> Vec<(String, Placement)> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative);
        let text = std::fs::read_to_string(&path).expect("read the areas file");
        let areas: Value = serde_json::from_str(&text).expect("parse the areas file");
        areas_of(&areas)
            .into_iter()
            .map(|(name, def)| (name, placement(&def)))
            .collect()
    }

    /// The expected placements as `(area, placement)` pairs.
    fn table(rows: &[(&str, Placement)]) -> Vec<(String, Placement)> {
        rows.iter()
            .map(|(name, placement)| (name.to_string(), placement.clone()))
            .collect()
    }

    /// The corpus run of the plan (`plan-corpus-directory-blocks.py`) over
    /// this repository's areas. With `template` opted out through
    /// `"agentsMd": false`, `cli` and `tests` get directory blocks; `docs`,
    /// `tools`, and `rust` span several directories and get a root section
    /// only; `global` and `process` have no globs.
    #[test]
    fn this_repositorys_areas_place_blocks_for_cli_and_tests_only() {
        assert_eq!(
            placements_in("knowledge/areas.json"),
            table(&[
                ("global", Placement::NoSection),
                ("process", Placement::NoSection),
                ("docs", Placement::RootOnly),
                ("tools", Placement::RootOnly),
                (
                    "cli",
                    Placement::Directory("crates/houserules/src".to_string())
                ),
                ("template", Placement::RootOnly),
                ("tests", Placement::Directory("tests".to_string())),
                ("rust", Placement::RootOnly),
            ])
        );
    }

    /// The corpus run over the areas that a fresh install gets: `tools`
    /// gets the one directory block.
    #[test]
    fn the_kit_areas_place_a_block_for_tools_only() {
        assert_eq!(
            placements_in("template/knowledge/areas.json"),
            table(&[
                ("global", Placement::NoSection),
                ("process", Placement::NoSection),
                ("docs", Placement::RootOnly),
                ("tools", Placement::Directory("tools".to_string())),
            ])
        );
    }

    /// The prefix of a glob stops at the first wildcard component and drops
    /// the last component.
    #[test]
    fn the_prefix_of_a_glob_stops_at_the_first_wildcard_component() {
        let cases = [
            ("crates/houserules/src/**", "crates/houserules/src"),
            ("tests/**", "tests"),
            ("docs/*.md", "docs"),
            ("a/b/c.txt", "a/b"),
            ("a/b*/c/**", "a"),
            ("a/{b,c}/d", "a"),
            ("a/[bc]/d", "a"),
            ("a/b?/d", "a"),
            ("README.md", ""),
            ("*.toml", ""),
            ("**/AGENTS.md", ""),
            ("Cargo.lock", ""),
        ];
        for (glob, expected) in cases {
            assert_eq!(directory_prefix(glob), expected, "{glob}");
        }
    }

    /// The root block of a base: the standing rules, then one section for each
    /// area that has globs, none for an area without globs, none with a rule
    /// text.
    #[test]
    fn the_root_block_holds_the_standing_rules_and_a_section_for_each_area_with_globs() {
        let base = fixture_base();

        let (text, standing) = root_block(&base);

        assert_eq!(
            text,
            format!(
                "{BEGIN}\n{GENERATED}\n\n## Standing rules\n\n- [process.sequential] Run agents sequentially.\n- [process.ask] Ask when unsure.\n\n## Area rules\n\n### rust\n\nGlobs: `crates/**`, `Cargo.toml`\n{FOR_LINE}\n{END}"
            )
        );
        assert_eq!(standing, 4);
    }

    /// A base without a standing rule says so in a line of its own, and a
    /// base without an area that has globs has no area section.
    #[test]
    fn a_base_without_standing_rules_or_areas_with_globs_says_so() {
        let mut base = fixture_base();
        base.entries.retain(|_, entry| !entry.standing);
        base.areas.retain(|(_, def)| def.paths.is_empty());

        let (text, standing) = root_block(&base);

        assert_eq!(
            text,
            format!("{BEGIN}\n{GENERATED}\n\n## Standing rules\n\nNo standing rules.\n{END}")
        );
        assert_eq!(standing, 3);
    }

    /// Several prefixes, a root prefix, no globs, `agentsMd` false, and a
    /// prefix that leaves the install root each have their placement.
    #[test]
    fn the_placement_follows_the_globs_and_the_agents_md_key() {
        let cases = [
            (json!({"paths": []}), Placement::NoSection),
            (json!({"paths": ["a/**", "a/b/**"]}), Placement::RootOnly),
            (json!({"paths": ["a/**", "README.md"]}), Placement::RootOnly),
            (
                json!({"paths": ["README.md", "*.toml"]}),
                Placement::RootOnly,
            ),
            (
                json!({"paths": ["a/x/**", "a/x/y.rs"]}),
                Placement::Directory("a/x".to_string()),
            ),
            (
                json!({"paths": ["a/**"], "agentsMd": true}),
                Placement::Directory("a".to_string()),
            ),
            (
                json!({"paths": ["a/**"], "agentsMd": false}),
                Placement::RootOnly,
            ),
            (json!({"paths": ["../a/**"]}), Placement::RootOnly),
            (json!({"paths": ["./a/**"]}), Placement::RootOnly),
            (json!({"paths": ["/abs/**"]}), Placement::RootOnly),
            (json!({"paths": ["a//b/**"]}), Placement::RootOnly),
        ];
        for (area, expected) in cases {
            let def: AreaDef = serde_json::from_value(area.clone()).expect("an area");
            assert_eq!(placement(&def), expected, "{area}");
        }
    }
}
