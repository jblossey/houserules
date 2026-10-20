//! The seeded CI workflow of one install (spec section 8).
//!
//! An install at the git top level keeps the template as it is, at
//! `.github/workflows/knowledge.yml`. An install below the top level gets
//! the template in its subdirectory form at
//! `.github/workflows/knowledge-<folder>.yml`, where `<folder>` is the
//! install root's path from the top level with each `/` replaced by `-`.
//! The form changes four places of the template: the `name` becomes
//! `knowledge (<path>)`, each of the two triggers gets a `paths` filter on
//! `<path>/**`, and the job gets `working-directory: <path>` as the default
//! of its steps. Each value is a YAML single-quoted scalar.
//!
//! The file lies outside the install root, so its baseline key is its path
//! in git's top-level pathspec form (`:/<path>`), which does not depend on
//! the install root.

use super::location::Placement;

/// The template's own path, below the git top level.
pub(super) const TEMPLATE_PATH: &str = ".github/workflows/knowledge.yml";

/// The characters that a GitHub path filter reads as a pattern: a folder name
/// that holds one would give a filter that never matches the folder.
const FILTER_PATTERN_CHARACTERS: [char; 6] = ['*', '?', '+', '[', ']', '!'];

/// The workflow file of one install.
pub(super) struct Workflow {
    /// The file's path below the git top level, `/`-separated.
    pub(super) path: String,
    /// The baseline key of the file: `:/` and its path.
    pub(super) key: String,
    /// The bytes that `init` writes and `update` syncs.
    pub(super) content: Vec<u8>,
}

impl Workflow {
    /// The workflow of the install at `placement`, from `template`, the text
    /// of the template workflow. An install path that the form cannot hold
    /// is one named line (`check_install_path`).
    pub(super) fn for_install(placement: &Placement, template: &[u8]) -> Result<Self, String> {
        let path = workflow_path(&placement.components);
        let content = if placement.is_top_level() {
            template.to_vec()
        } else {
            check_install_path(placement)?;
            let text = std::str::from_utf8(template)
                .map_err(|_| format!("{TEMPLATE_PATH}: not valid UTF-8"))?;
            subdirectory_form(text, &placement.path())?.into_bytes()
        };
        Ok(Self {
            key: format!(":/{path}"),
            path,
            content,
        })
    }
}

/// The path of the workflow file of the install whose root has `components`,
/// below the git top level.
pub(super) fn workflow_path(components: &[String]) -> String {
    if components.is_empty() {
        TEMPLATE_PATH.to_string()
    } else {
        format!(".github/workflows/knowledge-{}.yml", components.join("-"))
    }
}

/// Refuses an install path that holds a control character or a character
/// that a GitHub path filter reads as a pattern, with one line that names
/// the character.
fn check_install_path(placement: &Placement) -> Result<(), String> {
    let path = placement.path();
    let root = placement.root();
    if let Some(found) = path.chars().find(|c| c.is_control()) {
        return Err(format!(
            "{}: the install path {path:?} holds the control character {found:?}, which a workflow file name cannot hold",
            root.display()
        ));
    }
    if let Some(found) = path
        .chars()
        .find(|c| *c == '\\' || FILTER_PATTERN_CHARACTERS.contains(c))
    {
        return Err(format!(
            "{}: the install path \"{path}\" holds the character '{found}', which a GitHub path filter reads as a pattern; choose a directory without it",
            root.display()
        ));
    }
    Ok(())
}

/// `value` as a YAML single-quoted scalar: each `'` doubles.
fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// The template's lines that the subdirectory form builds on, each of which
/// must occur once.
const NAME_LINE: &str = "name: knowledge";
const PULL_REQUEST_LINE: &str = "  pull_request:";
const PUSH_BRANCHES_LINE: &str = "    branches: [main]";
const RUNS_ON_LINE: &str = "    runs-on: ubuntu-latest";

/// The subdirectory form of `template` for the install at `path`. A template
/// line that the form builds on must occur exactly once: a template that
/// lost one is a defect of the kit, named in one line.
fn subdirectory_form(template: &str, path: &str) -> Result<String, String> {
    let filter = format!("    paths:\n      - {}", quoted(&format!("{path}/**")));
    let mut seen = [0usize; 4];
    let mut lines = Vec::new();
    for line in template.lines() {
        match line {
            NAME_LINE => {
                seen[0] += 1;
                lines.push(format!("name: {}", quoted(&format!("knowledge ({path})"))));
            }
            PULL_REQUEST_LINE => {
                seen[1] += 1;
                lines.push(format!("{line}\n{filter}"));
            }
            PUSH_BRANCHES_LINE => {
                seen[2] += 1;
                lines.push(format!("{line}\n{filter}"));
            }
            RUNS_ON_LINE => {
                seen[3] += 1;
                lines.push(format!(
                    "{line}\n    defaults:\n      run:\n        working-directory: {}",
                    quoted(path)
                ));
            }
            other => lines.push(other.to_string()),
        }
    }
    let anchors = [
        NAME_LINE,
        PULL_REQUEST_LINE,
        PUSH_BRANCHES_LINE,
        RUNS_ON_LINE,
    ];
    if let Some((anchor, _)) = anchors.iter().zip(seen).find(|(_, count)| *count != 1) {
        return Err(format!(
            "{TEMPLATE_PATH}: the line {anchor:?} must occur once for the subdirectory form"
        ));
    }
    let mut form = lines.join("\n");
    if template.ends_with('\n') {
        form.push('\n');
    }
    Ok(form)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    /// A placement below `/repo` with the path `components`.
    fn placement(components: &[&str]) -> Placement {
        Placement {
            top_level: PathBuf::from("/repo"),
            components: components.iter().map(|part| part.to_string()).collect(),
        }
    }

    /// The text of the template workflow, read from this checkout's
    /// `template/` directory.
    fn template() -> String {
        fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../template")
                .join(TEMPLATE_PATH),
        )
        .expect("read the template workflow")
    }

    /// The workflow of the install at `components`, as text.
    fn form_of(components: &[&str]) -> String {
        let workflow = Workflow::for_install(&placement(components), template().as_bytes())
            .expect("a workflow");
        String::from_utf8(workflow.content).expect("UTF-8")
    }

    /// An install at the top level keeps the template as it is, at the
    /// template's own path, under the top-level key.
    #[test]
    fn an_install_at_the_top_level_keeps_the_template() {
        let workflow =
            Workflow::for_install(&placement(&[]), template().as_bytes()).expect("a workflow");

        assert_eq!(workflow.path, ".github/workflows/knowledge.yml");
        assert_eq!(workflow.key, ":/.github/workflows/knowledge.yml");
        assert_eq!(workflow.content, template().as_bytes());
    }

    /// The file name joins the folder's components with `-`, and the key
    /// is the path in git's top-level pathspec form.
    #[test]
    fn a_subdirectory_install_gets_its_own_file_name_and_key() {
        for (components, file) in [
            (vec!["sub"], "knowledge-sub.yml"),
            (vec!["b", "c"], "knowledge-b-c.yml"),
            (vec!["my dir", "über"], "knowledge-my dir-über.yml"),
        ] {
            let workflow = Workflow::for_install(&placement(&components), template().as_bytes())
                .expect("a workflow");

            assert_eq!(workflow.path, format!(".github/workflows/{file}"));
            assert_eq!(workflow.key, format!(":/.github/workflows/{file}"));
        }
    }

    /// The subdirectory form changes the template in four places and no
    /// other: the name, two `paths` filters, and the job's default
    /// `working-directory`.
    #[test]
    fn the_subdirectory_form_changes_the_template_in_four_places() {
        let template = template();
        let mut expected = Vec::new();
        for line in template.lines() {
            match line {
                "name: knowledge" => expected.push("name: 'knowledge (b/c)'".to_string()),
                "  pull_request:" | "    branches: [main]" => {
                    expected.push(line.to_string());
                    expected.push("    paths:".to_string());
                    expected.push("      - 'b/c/**'".to_string());
                }
                "    runs-on: ubuntu-latest" => {
                    expected.push(line.to_string());
                    expected.push("    defaults:".to_string());
                    expected.push("      run:".to_string());
                    expected.push("        working-directory: 'b/c'".to_string());
                }
                other => expected.push(other.to_string()),
            }
        }

        assert_eq!(form_of(&["b", "c"]), format!("{}\n", expected.join("\n")));
    }

    /// A single quote doubles in each YAML scalar.
    #[test]
    fn a_single_quote_doubles_in_each_scalar() {
        let form = form_of(&["o'clock"]);

        assert!(form.contains("name: 'knowledge (o''clock)'\n"), "{form}");
        assert!(form.contains("      - 'o''clock/**'\n"), "{form}");
        assert!(
            form.contains("        working-directory: 'o''clock'\n"),
            "{form}"
        );
    }

    /// A template without a final newline gives a form without one.
    #[test]
    fn the_final_newline_of_the_template_decides_the_final_newline_of_the_form() {
        let without = template().trim_end_matches('\n').to_string();

        let form = subdirectory_form(&without, "sub").expect("a form");

        assert!(!form.ends_with('\n'));
        assert!(
            subdirectory_form(&template(), "sub")
                .expect("a form")
                .ends_with('\n')
        );
    }

    /// A template that lost one of the four lines, or holds one twice, is a
    /// defect of the kit, named in one line.
    #[test]
    fn a_template_line_that_does_not_occur_once_is_named() {
        for anchor in [
            NAME_LINE,
            PULL_REQUEST_LINE,
            PUSH_BRANCHES_LINE,
            RUNS_ON_LINE,
        ] {
            let expected = format!(
                "{TEMPLATE_PATH}: the line {anchor:?} must occur once for the subdirectory form"
            );
            let lost: String = template()
                .lines()
                .filter(|line| *line != anchor)
                .map(|line| format!("{line}\n"))
                .collect();
            let doubled: String = template()
                .lines()
                .flat_map(|line| {
                    if line == anchor {
                        vec![line, line]
                    } else {
                        vec![line]
                    }
                })
                .map(|line| format!("{line}\n"))
                .collect();

            assert_eq!(subdirectory_form(&lost, "sub"), Err(expected.clone()));
            assert_eq!(subdirectory_form(&doubled, "sub"), Err(expected));
        }
    }

    /// A folder name with a character that a GitHub path filter reads as a
    /// pattern is refused with one line that names the character.
    #[test]
    fn a_folder_name_that_a_path_filter_reads_as_a_pattern_is_refused() {
        for character in ['*', '?', '+', '[', ']', '!', '\\'] {
            let name = format!("a{character}b");
            let placed = placement(&[name.as_str()]);

            let error = Workflow::for_install(&placed, template().as_bytes())
                .err()
                .unwrap_or_else(|| panic!("{character:?} is refused"));

            assert_eq!(
                error,
                format!(
                    "{}: the install path \"{name}\" holds the character '{character}', which a GitHub path filter reads as a pattern; choose a directory without it",
                    placed.root().display()
                )
            );
        }
    }

    /// A control character in the path is refused: no workflow file name or
    /// YAML scalar can hold it.
    #[test]
    fn a_control_character_in_the_path_is_refused() {
        let placed = placement(&["a\nb"]);

        let error = Workflow::for_install(&placed, template().as_bytes())
            .err()
            .expect("a refusal");

        assert_eq!(
            error,
            format!(
                "{}: the install path \"a\\nb\" holds the control character '\\n', which a workflow file name cannot hold",
                placed.root().display()
            )
        );
    }

    /// A template that is no UTF-8 is one named line, for a subdirectory
    /// install; the top level copies the bytes as they are.
    #[test]
    fn a_template_that_is_no_utf8_is_one_named_line() {
        let bytes = [0xff, 0xfe];

        assert_eq!(
            Workflow::for_install(&placement(&["sub"]), &bytes).err(),
            Some(format!("{TEMPLATE_PATH}: not valid UTF-8"))
        );
        assert_eq!(
            Workflow::for_install(&placement(&[]), &bytes)
                .expect("a workflow")
                .content,
            bytes
        );
    }
}
