//! The findings of `check-knowledge` about the keys of `.houserules.json`
//! that carry the runner and workspace settings (batch 35, spec 3.3 and 7):
//! the faults of the `runners` and `workspaces` keys, the default location of
//! the workspaces when git does not ignore it, and each `runner` check that
//! names a tool the marker does not define.
//!
//! A marker that cannot be read gives nothing here: the pin check, the
//! kit-citation lint, and the render check each name the defect of the marker.

use std::path::Path;

use crate::install::IGNORE_LINE;
use crate::install::marker::{DEFAULT_WORKSPACES, Marker};
use crate::root::INSTALL_MARKER;

use super::check_packs::source_file;
use super::check_shape::CheckType;
use super::ignored::{IgnoreFault, ignored_among};
use super::model::{Base, CheckField};

/// The findings, in this order: the faults of `runners`, the fault of
/// `workspaces` or, while the key is not set, the line for an unignored default
/// location, then one line for each active entry whose `runner` check names
/// an undefined tool, by entry id, at the file that holds the entry. A
/// `runner` check with no `tool` names no tool: `check::check_shape` reports
/// it, for an entry of every source.
pub(super) fn findings(base: &Base) -> Vec<String> {
    let Ok(marker) = Marker::read_or_default_for_gate(&base.root, "WI") else {
        return Vec::new();
    };
    let mut findings = marker.runner_lines();
    match marker.workspaces() {
        Err(line) => findings.push(line),
        Ok(None) => findings.extend(unignored_default_location(&base.root)),
        Ok(Some(_)) => {}
    }
    let mut entries: Vec<_> = base.entries.values().collect();
    entries.sort_by(|left, right| left.id.cmp(&right.id));
    for entry in entries {
        let CheckField::Valid(check) = &entry.check else {
            continue;
        };
        let (CheckType::Runner, Some(tool)) = (check.kind, check.tool.as_deref()) else {
            continue;
        };
        // A runners value or an entry with a fault has its own line above.
        if let Ok(None) = marker.runner(tool) {
            findings.push(format!(
                "{} {}: runner tool \"{tool}\" is not defined in .houserules.json runners",
                source_file(base, &entry.id, &entry.source),
                entry.id
            ));
        }
    }
    findings
}

/// The line for the default location of the workspaces (`DEFAULT_WORKSPACES`)
/// when the folder exists and git does not ignore it, and `None` otherwise. An
/// absent folder holds nothing that git could track, so a fresh clone gets no
/// line. A location that the marker sets is the project's choice and gets no
/// line (the caller). git is asked about the path with a trailing `/`, since the
/// usual ignore line `.houserules/work/` matches a directory only. A directory
/// outside a work tree gets no line, because the ignore test does not run there.
/// A git that fails inside a work tree gets the line that says the path was not
/// tested, with the text of git.
fn unignored_default_location(root: &Path) -> Option<String> {
    if !root.join(DEFAULT_WORKSPACES).is_dir() {
        return None;
    }
    let asked = format!("{DEFAULT_WORKSPACES}/");
    match ignored_among(root, &[asked.as_str()]) {
        Ok(ignored) => (!ignored.contains(&asked)).then(|| {
            format!(
                "{INSTALL_MARKER}: workspaces path \"{DEFAULT_WORKSPACES}\" is not ignored by git; add {IGNORE_LINE} to .gitignore"
            )
        }),
        Err(IgnoreFault::NotAWorkTree(_)) => None,
        Err(fault) => Some(format!(
            "{INSTALL_MARKER}: workspaces path \"{DEFAULT_WORKSPACES}\" not tested against .gitignore: {fault}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::TempDir;

    use super::*;
    use crate::rules::test_support::{base_with, base_with_packs, entry};

    /// The check of a `runner` entry that names `tool`.
    fn runner_check(tool: &str) -> Value {
        json!({"type": "runner", "level": "fail", "tool": tool})
    }

    /// Writes `.houserules.json` with `marker` below `dir`.
    fn write_marker(dir: &TempDir, marker: &Value) {
        fs::write(
            dir.path().join(".houserules.json"),
            serde_json::to_string(marker).expect("serialize the marker"),
        )
        .expect("write the marker");
    }

    /// A base of one local entry, `team.lint`, that holds `check`, and a
    /// marker that holds `marker`.
    fn base_with_marker(marker: &Value, check: Value) -> (TempDir, Base) {
        let (dir, base) = base_with(&[entry("team.lint", json!({"check": check}))]);
        write_marker(&dir, marker);
        (dir, base)
    }

    /// The marker lines come first, then the lines of undefined tools.
    #[test]
    fn the_marker_lines_come_before_the_lines_of_undefined_tools() {
        let (_dir, base) = base_with_marker(
            &json!({"runners": {"semgrep": {"command": []}}, "workspaces": "/abs"}),
            runner_check("other"),
        );

        assert_eq!(
            findings(&base),
            [
                ".houserules.json: runners.semgrep.command must be a non-empty array of strings",
                ".houserules.json: workspaces must be a relative path inside the install root",
                "knowledge/test.json team.lint: runner tool \"other\" is not defined in .houserules.json runners",
            ]
        );
    }

    /// The line names the file that holds the entry, as every other line of
    /// `check-knowledge` does, not the file its id suggests: `team.lint` is
    /// held by `knowledge/test.json` here.
    #[test]
    fn a_local_entry_is_named_at_the_file_that_holds_it() {
        let (_dir, base) = base_with_marker(&json!({}), runner_check("semgrep"));

        assert_eq!(
            findings(&base),
            [
                "knowledge/test.json team.lint: runner tool \"semgrep\" is not defined in .houserules.json runners"
            ]
        );
    }

    /// A tool whose entry has a fault is defined: the fault has its own line.
    /// A `runners` value that is no object gets no line for each tool.
    #[test]
    fn a_faulty_entry_or_a_faulty_runners_value_is_not_also_an_undefined_tool() {
        let (_dir, base) =
            base_with_marker(&json!({"runners": {"semgrep": 3}}), runner_check("semgrep"));
        assert_eq!(
            findings(&base),
            [".houserules.json: runners.semgrep must be an object"]
        );

        let (_dir, base) = base_with_marker(&json!({"runners": []}), runner_check("semgrep"));
        assert_eq!(
            findings(&base),
            [".houserules.json: runners must be an object"]
        );
    }

    /// A check of another type, and a `runner` check with no `tool`, add no
    /// line.
    #[test]
    fn only_a_runner_check_with_a_tool_can_name_an_undefined_tool() {
        for check in [
            json!({"type": "grep-absent", "level": "fail", "files": "a", "pattern": "x", "scope": "tree"}),
            json!({"type": "runner", "level": "fail"}),
        ] {
            let (_dir, base) = base_with_marker(&json!({}), check);
            assert_eq!(findings(&base), Vec::<String>::new());
        }
    }

    /// An entry of a pack is named at the pack copy that holds it.
    #[test]
    fn a_pack_entry_is_named_at_its_pack_copy() {
        let pack_entry = entry("demo.lint", json!({"check": runner_check("semgrep")}));
        let (dir, base) = base_with_packs(&[], &[("demo", &[pack_entry])], &[]);
        write_marker(&dir, &json!({}));

        assert_eq!(
            findings(&base),
            [
                "knowledge/packs/demo.json demo.lint: runner tool \"semgrep\" is not defined in .houserules.json runners"
            ]
        );
    }

    /// A marker that cannot be read gives no line here.
    #[test]
    fn a_marker_that_cannot_be_read_gives_no_line() {
        let (dir, base) = base_with_marker(&json!({}), runner_check("semgrep"));
        fs::write(dir.path().join(".houserules.json"), "not json").expect("write the marker");

        assert_eq!(findings(&base), Vec::<String>::new());
    }
}
