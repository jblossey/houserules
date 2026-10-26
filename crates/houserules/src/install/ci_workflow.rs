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
//! in the top-level pathspec form of `git` (`:/<path>`), which does not
//! depend on the install root.
//!
//! The steps of the file depend on the tracker of the install (spec 4.8). JSON
//! and no tracker keep the steps of the template. GitHub adds `issues: read` to
//! the job-level `permissions` block of the template and the token to the steps
//! that run `check-backlog` and `audit`, because `gh` reads the issues of the
//! repository. Beads adds a step that installs `bd` at the release that
//! `mise.toml` pins and a step that runs `bd bootstrap`, because a fresh clone
//! has no Beads store. Every form checks out the head of a pull request: the
//! `actions/checkout` step carries
//! `ref: ${{ github.event.pull_request.head.sha }}` (spec 3.2), so that every
//! step of the job, the audit included, judges the tree that lands.
//!
//! Every form runs with a read-only job token: the template sets
//! `permissions: {contents: read}` on the job. The template also sets
//! `persist-credentials: false` on the checkout, because no step of the JSON,
//! GitHub, or no-tracker form runs `git` against the remote after the checkout.
//! The Beads form drops that input: `bd bootstrap --yes` fetches the Dolt data
//! from the remote, so it needs the persisted credential, which is read-only.
//! That claim about `bd` rests on spec 8.5 (C11) of
//! `docs/specs/2026-10-22-batch-36-evidence-and-report-claims.md`; the run of
//! the Beads form on GitHub is the proof.
//!
//! `init` writes no such file (spec 10.7): an install holds the workflow only
//! when an earlier release seeded it ([`Workflow::is_held_by`]). [`template_for`]
//! builds the text from the template, and [`write_for`] keeps the file of an
//! install at the content that its tracker calls for: every command that sets
//! `tracker` calls it, and so does `update`. Both leave a workflow that the
//! project deleted absent, and both build the file name of the workflow only
//! for an install that holds one, so that no install path is refused for a
//! workflow that does not exist. The `reader` submodule reads a copy that the
//! project changed.

mod reader;

use std::path::Path;

use super::hand_steps;
use super::location::{self, Placement};
use super::marker::Marker;
use super::sync::sync_file;
use super::{is_overridden, payload_content};
use crate::tracker::Adapter;
use crate::tracker::beads::BD_VERSION;

pub(super) use reader::hand_step_for_kept;

/// The template's own path, below the git top level.
pub(super) const TEMPLATE_PATH: &str = ".github/workflows/knowledge.yml";

/// The step of the template that runs `check-backlog`.
const CHECK_BACKLOG_LINE: &str = "      - run: houserules check-backlog";

/// The last line of the step of the template that runs `audit`.
const AUDIT_LAST_LINE: &str = "          --head ${{ github.event.pull_request.head.sha }}";

/// The line of the permissions block of the template that the GitHub form
/// extends.
const CONTENTS_READ_LINE: &str = "      contents: read";

/// The permission that the GitHub form adds to the block, after
/// `CONTENTS_READ_LINE`: `gh` reads the issues.
const GITHUB_ISSUES_LINE: &str = "      issues: read";

/// The input of the checkout step that drops the persisted credential. The Beads
/// form removes it.
const PERSIST_CREDENTIALS_LINE: &str = "          persist-credentials: false";

/// The env of a step of the GitHub form that runs a gate: `gh` takes its token
/// from `GH_TOKEN`.
const GITHUB_TOKEN_ENV: &str = "        env:\n          GH_TOKEN: ${{ github.token }}";

/// The steps of the Beads form, which go before the step that runs
/// `check-backlog`: the release of `bd` that `mise.toml` pins is downloaded
/// with its checksums and verified, and the store is bootstrapped from the
/// Dolt data of the origin. The usage metrics of `bd` are off (ruling 188).
fn beads_steps() -> String {
    format!(
        r#"      - name: Install bd
        shell: bash
        run: |
          set -euo pipefail
          archive=beads_{BD_VERSION}_linux_amd64.tar.gz
          base=https://github.com/gastownhall/beads/releases/download/v{BD_VERSION}
          mkdir -p "$RUNNER_TEMP/bd"
          cd "$RUNNER_TEMP/bd"
          curl --proto '=https' --tlsv1.2 -fsSLO "$base/$archive"
          curl --proto '=https' --tlsv1.2 -fsSLO "$base/checksums.txt"
          sha256sum --check --ignore-missing checksums.txt
          tar -xzf "$archive"
          echo "$RUNNER_TEMP/bd" >> "$GITHUB_PATH"
      - name: Bootstrap the Beads store
        shell: bash
        env:
          BD_DISABLE_METRICS: '1'
          DO_NOT_TRACK: '1'
        run: bd bootstrap --yes"#
    )
}

/// Names a template that lacks the line `anchor`, or holds it twice, as a defect
/// of the kit, in one line.
fn require_one_line(template: &str, anchor: &str, adapter: Adapter) -> Result<(), String> {
    if template.lines().filter(|line| *line == anchor).count() != 1 {
        return Err(format!(
            "{TEMPLATE_PATH}: the line {anchor:?} must occur once for the workflow of the {} tracker",
            adapter.as_str()
        ));
    }
    Ok(())
}

/// `lines` joined by `\n`, with a final `\n` when `template` ends with one.
fn join_like(template: &str, lines: &[&str]) -> String {
    let mut text = lines.join("\n");
    if template.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// `template` with `addition` after (`after`) or before the one line that equals
/// `anchor`. A template that lacks the line, or holds it twice, is a defect of
/// the kit, named in one line.
fn insert_at_line(
    template: &str,
    anchor: &str,
    addition: &str,
    after: bool,
    adapter: Adapter,
) -> Result<String, String> {
    require_one_line(template, anchor, adapter)?;
    let mut lines = Vec::new();
    for line in template.lines() {
        if line == anchor && !after {
            lines.push(addition);
        }
        lines.push(line);
        if line == anchor && after {
            lines.push(addition);
        }
    }
    Ok(join_like(template, &lines))
}

/// `template` without the one line that equals `anchor`. A template that lacks
/// the line, or holds it twice, is a defect of the kit, named in one line.
fn remove_line(template: &str, anchor: &str, adapter: Adapter) -> Result<String, String> {
    require_one_line(template, anchor, adapter)?;
    let lines: Vec<&str> = template.lines().filter(|line| *line != anchor).collect();
    Ok(join_like(template, &lines))
}

/// The seeded CI workflow of an install with the tracker `adapter` (or none) and
/// the id prefix `prefix`, before the subdirectory form: the template, with the
/// lines of the adapter (the module doc has them).
pub(super) fn template_for(adapter: Option<Adapter>, prefix: &str) -> Result<Vec<u8>, String> {
    let template = payload_content(TEMPLATE_PATH, prefix)?;
    let text =
        std::str::from_utf8(&template).map_err(|_| format!("{TEMPLATE_PATH}: not valid UTF-8"))?;
    let form = match adapter {
        None | Some(Adapter::Json) => return Ok(template),
        Some(Adapter::Github) => {
            let github = Adapter::Github;
            let text = insert_at_line(text, CONTENTS_READ_LINE, GITHUB_ISSUES_LINE, true, github)?;
            let text = insert_at_line(&text, CHECK_BACKLOG_LINE, GITHUB_TOKEN_ENV, true, github)?;
            insert_at_line(&text, AUDIT_LAST_LINE, GITHUB_TOKEN_ENV, true, github)?
        }
        Some(Adapter::Beads) => {
            let beads = Adapter::Beads;
            let text = remove_line(text, PERSIST_CREDENTIALS_LINE, beads)?;
            insert_at_line(&text, CHECK_BACKLOG_LINE, &beads_steps(), false, beads)?
        }
    };
    Ok(form.into_bytes())
}

/// Brings the seeded CI workflow of the install at `root` to the content that
/// the tracker `adapter` (or none) calls for, as `update` keeps a D3 file: a
/// workflow that `marker` baselines, and that the project neither changed nor
/// owns, is written when its bytes would change, and the baseline moves with
/// it. An install that holds no workflow (`Workflow::held_by_install`) gets
/// none: a workflow with no baseline is left as found, and so is one that the
/// project deleted. Returns the lines that name each file written or kept, and
/// the hand steps of a kept file.
pub(super) fn write_for(
    root: &Path,
    marker: &mut Marker,
    adapter: Option<Adapter>,
) -> Result<Vec<String>, String> {
    let placement = location::locate(root)?;
    let template = template_for(adapter, &marker.id_prefix_or("WI"))?;
    let mut lines = Vec::new();
    let Some(workflow) = Workflow::held_by_install(&placement, &template, marker)? else {
        return Ok(lines);
    };
    if is_overridden(&marker.owned_paths(), &workflow.path) {
        return Ok(lines);
    }
    let mut baselines = marker.baselines();
    let kept = sync_file(
        &placement.top_level,
        &workflow.path,
        &workflow.key,
        &workflow.content,
        &[],
        &mut baselines,
        &mut lines,
    )?;
    marker.set_baselines(baselines);
    if kept {
        let current = std::fs::read(placement.top_level.join(&workflow.path))
            .map_err(|error| format!("{}: {error}", workflow.path))?;
        lines.extend(
            hand_step_for_kept(adapter, &workflow.path, &current)
                .into_iter()
                .map(|step| format!("{}{step}", hand_steps::PREFIX)),
        );
    }
    Ok(lines)
}

/// The characters that a GitHub path filter reads as a pattern: a folder name
/// that holds one would give a filter that never matches the folder.
const FILTER_PATTERN_CHARACTERS: [char; 6] = ['*', '?', '+', '[', ']', '!'];

/// The workflow file of one install.
pub(super) struct Workflow {
    /// The file's path below the git top level, `/`-separated.
    pub(super) path: String,
    /// The baseline key of the file: `:/` and its path.
    pub(super) key: String,
    /// The bytes that `update` and a tracker command sync.
    pub(super) content: Vec<u8>,
}

impl Workflow {
    /// `true` when the install at `placement` holds its seeded workflow: the
    /// file exists below the top level, and `marker` baselines it. A 1.x marker
    /// baselines nothing, so the file that an earlier release seeded counts
    /// there (`update` converts it). A workflow that the project deleted is no
    /// longer held, and `update` does not write it again.
    pub(super) fn is_held_by(placement: &Placement, marker: &Marker) -> bool {
        let path = workflow_path(&placement.components);
        let recorded = marker.is_1_x() || marker.baselines().contains_key(&format!(":/{path}"));
        recorded && placement.top_level.join(&path).is_file()
    }

    /// The workflow of the install at `placement`, from `template`, when the
    /// install holds one (`is_held_by`). An install that holds none needs no
    /// workflow file name, so it is no reason to refuse an install path that no
    /// such name could hold: the answer is `None` there.
    pub(super) fn held_by_install(
        placement: &Placement,
        template: &[u8],
        marker: &Marker,
    ) -> Result<Option<Self>, String> {
        if !Self::is_held_by(placement, marker) {
            return Ok(None);
        }
        Self::for_install(placement, template).map(Some)
    }

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
pub(crate) fn workflow_path(components: &[String]) -> String {
    if components.is_empty() {
        TEMPLATE_PATH.to_string()
    } else {
        format!(".github/workflows/knowledge-{}.yml", components.join("-"))
    }
}

/// Refuses an install path that holds a control character or a character
/// that a GitHub path filter reads as a pattern, with one line that names
/// the character. Only the workflow of an install below the top level has
/// such a name and filter (`Workflow::for_install`).
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

    /// A scratch top level that holds the marker `marker` and, for each path
    /// of `files`, the template workflow at that path; returns it with the
    /// marker as `Marker` reads it.
    fn install_holding(marker: serde_json::Value, files: &[&str]) -> (tempfile::TempDir, Marker) {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join(".houserules.json"), marker.to_string())
            .expect("write the marker");
        for path in files {
            let file = dir.path().join(path);
            fs::create_dir_all(file.parent().expect("a parent")).expect("create the folder");
            fs::write(&file, template()).expect("write the workflow");
        }
        let marker = Marker::read_or_default(dir.path(), "WI").expect("a marker");
        (dir, marker)
    }

    /// The marker of a 2.0 install that baselines the workflow at `path`, or
    /// baselines nothing.
    fn marker_baselining(path: Option<&str>) -> serde_json::Value {
        let baselines: serde_json::Map<String, serde_json::Value> = path
            .map(|path| (format!(":/{path}"), serde_json::json!("0".repeat(64))))
            .into_iter()
            .collect();
        serde_json::json!({
            "schema": "houserules/install@2.3.0",
            "version": "2.0.0",
            "idPrefix": "WI",
            "baselines": baselines,
        })
    }

    /// The placement of the install at `components` below `top_level`.
    fn placed_in(top_level: &Path, components: &[&str]) -> Placement {
        Placement {
            top_level: top_level.to_path_buf(),
            components: components.iter().map(|part| part.to_string()).collect(),
        }
    }

    /// An install that holds no seeded workflow needs no workflow file name, so
    /// no install path is refused for one: a name with a pattern character or
    /// a control character gives no workflow, and no refusal.
    #[test]
    fn an_install_that_holds_no_workflow_is_refused_for_no_file_name() {
        for name in ["a[b", "a+b", "a\nb"] {
            let (dir, marker) = install_holding(marker_baselining(None), &[]);

            let held = Workflow::held_by_install(&placed_in(dir.path(), &[name]), b"", &marker)
                .unwrap_or_else(|error| panic!("{name:?}: {error}"));

            assert!(held.is_none(), "{name:?}");
        }
    }

    /// A workflow that the project deleted is no workflow that the install
    /// holds, also when the marker still baselines it.
    #[test]
    fn a_baselined_workflow_that_the_project_deleted_is_not_held() {
        let path = ".github/workflows/knowledge-a[b.yml";
        let (dir, marker) = install_holding(marker_baselining(Some(path)), &[]);
        let placed = placed_in(dir.path(), &["a[b"]);

        assert!(!Workflow::is_held_by(&placed, &marker));
        assert!(
            Workflow::held_by_install(&placed, template().as_bytes(), &marker)
                .expect("no refusal")
                .is_none()
        );
    }

    /// A file at the workflow path that the marker does not baseline is the
    /// project's own file, not a seeded workflow.
    #[test]
    fn a_file_with_no_baseline_is_not_a_seeded_workflow() {
        let path = ".github/workflows/knowledge-sub.yml";
        let (dir, marker) = install_holding(marker_baselining(None), &[path]);

        assert!(!Workflow::is_held_by(
            &placed_in(dir.path(), &["sub"]),
            &marker
        ));
    }

    /// An install that baselines its workflow and still has the file holds it:
    /// the workflow has its own path and key.
    #[test]
    fn an_install_that_baselines_its_workflow_and_has_the_file_holds_it() {
        let path = ".github/workflows/knowledge-sub.yml";
        let (dir, marker) = install_holding(marker_baselining(Some(path)), &[path]);

        let held = Workflow::held_by_install(
            &placed_in(dir.path(), &["sub"]),
            template().as_bytes(),
            &marker,
        )
        .expect("no refusal")
        .expect("a held workflow");

        assert_eq!(held.path, path);
        assert_eq!(held.key, format!(":/{path}"));
    }

    /// Where the install holds a workflow, the refusal of a name that the
    /// workflow cannot have stays: its path filter would never match.
    #[test]
    fn a_held_workflow_whose_install_path_no_form_can_hold_is_refused() {
        let path = ".github/workflows/knowledge-a[b.yml";
        let (dir, marker) = install_holding(marker_baselining(Some(path)), &[path]);
        let placed = placed_in(dir.path(), &["a[b"]);

        let error = Workflow::held_by_install(&placed, template().as_bytes(), &marker)
            .err()
            .expect("a refusal");

        assert_eq!(
            error,
            format!(
                "{}: the install path \"a[b\" holds the character '[', which a GitHub path filter reads as a pattern; choose a directory without it",
                placed.root().display()
            )
        );
    }

    /// A 1.x marker baselines nothing, so the workflow that the file of an
    /// earlier release left is the install's workflow (`update` converts it).
    #[test]
    fn a_1_x_install_holds_the_workflow_it_has_on_disk() {
        let marker = serde_json::json!({"version": "1.4.0", "idPrefix": "WI"});
        let (dir, marker_with_file) = install_holding(marker.clone(), &[TEMPLATE_PATH]);
        let (other, marker_without_file) = install_holding(marker, &[]);

        assert!(Workflow::is_held_by(
            &placed_in(dir.path(), &[]),
            &marker_with_file
        ));
        assert!(!Workflow::is_held_by(
            &placed_in(other.path(), &[]),
            &marker_without_file
        ));
    }

    /// Spec 3.2 (203): every form of the seeded workflow checks out the PR head,
    /// in the top-level form and the subdirectory form of each tracker: the
    /// `with:` of its checkout step opens with the `ref` input, once.
    #[test]
    fn every_seeded_workflow_form_carries_the_ref() {
        let input = "ref: ${{ github.event.pull_request.head.sha }}";
        let adapters = [
            None,
            Some(Adapter::Json),
            Some(Adapter::Github),
            Some(Adapter::Beads),
        ];
        for adapter in adapters {
            let template = template_for(adapter, "WI").expect("a template");
            for components in [&[][..], &["sub"][..]] {
                let workflow =
                    Workflow::for_install(&placement(components), &template).expect("a workflow");
                let text = String::from_utf8(workflow.content).expect("UTF-8");
                let after_checkout = text
                    .split_once("uses: actions/checkout@")
                    .and_then(|(_, rest)| rest.split_once('\n'))
                    .map(|(_, rest)| rest)
                    .expect("the workflow checks out the repository");

                assert!(
                    after_checkout.starts_with(&format!("        with:\n          {input}\n")),
                    "{adapter:?} {components:?}: {text}"
                );
                assert_eq!(text.matches(input).count(), 1, "{adapter:?} {components:?}");
            }
        }
    }

    /// JSON and no tracker keep the template as it is.
    #[test]
    fn json_and_no_tracker_keep_the_template() {
        for adapter in [None, Some(Adapter::Json)] {
            assert_eq!(
                template_for(adapter, "WI").expect("a template"),
                template().as_bytes()
            );
        }
    }

    /// The GitHub form adds `issues: read` to the one permissions block and the
    /// token to the two gate steps; the Beads form drops the input that persists
    /// the credential and adds the two steps before the `check-backlog` step,
    /// which stays, and no token.
    #[test]
    fn the_forms_of_github_and_beads_add_only_their_lines() {
        let github = String::from_utf8(template_for(Some(Adapter::Github), "WI").expect("form"))
            .expect("UTF-8");
        assert_eq!(github.matches("permissions:").count(), 1);
        assert_eq!(
            github
                .matches(&format!("{CONTENTS_READ_LINE}\n{GITHUB_ISSUES_LINE}\n"))
                .count(),
            1
        );
        assert_eq!(github.matches(GITHUB_TOKEN_ENV).count(), 2);
        let without: String = github
            .replace(&format!("\n{GITHUB_ISSUES_LINE}"), "")
            .replace(&format!("\n{GITHUB_TOKEN_ENV}"), "");
        assert_eq!(without, template());

        let beads = String::from_utf8(template_for(Some(Adapter::Beads), "WI").expect("form"))
            .expect("UTF-8");
        let added = format!("{}\n", beads_steps());
        assert_eq!(
            beads.replace(&added, ""),
            template().replace(&format!("{PERSIST_CREDENTIALS_LINE}\n"), "")
        );
        assert!(!beads.contains("persist-credentials"));
        assert!(beads.contains(&format!("beads_{BD_VERSION}_linux_amd64.tar.gz")));
    }

    /// The template sets the read-only token and drops the persisted credential,
    /// each once, on the lines that the forms build on.
    #[test]
    fn the_template_holds_the_read_only_token_and_drops_the_credential_once() {
        let template = template();

        for line in [CONTENTS_READ_LINE, PERSIST_CREDENTIALS_LINE] {
            assert_eq!(
                template.lines().filter(|found| *found == line).count(),
                1,
                "{line}"
            );
        }
        assert_eq!(template.matches("permissions:").count(), 1);
    }

    /// A template that lacks the line of an anchor, or holds it twice, is a
    /// defect of the kit, named in one line.
    #[test]
    fn an_anchor_that_does_not_occur_once_is_named() {
        let expected = format!(
            "{TEMPLATE_PATH}: the line {CHECK_BACKLOG_LINE:?} must occur once for the workflow of the beads tracker"
        );
        let lost: String = template()
            .lines()
            .filter(|line| *line != CHECK_BACKLOG_LINE)
            .map(|line| format!("{line}\n"))
            .collect();
        let doubled = format!("{}{CHECK_BACKLOG_LINE}\n", template());

        for text in [lost, doubled] {
            assert_eq!(
                insert_at_line(&text, CHECK_BACKLOG_LINE, "x", false, Adapter::Beads),
                Err(expected.clone())
            );
        }
    }

    /// `remove_line` names a template that lacks the line, or holds it twice, in
    /// the same line as `insert_at_line`, and removes the one line otherwise.
    #[test]
    fn a_line_to_remove_that_does_not_occur_once_is_named() {
        let expected = format!(
            "{TEMPLATE_PATH}: the line {PERSIST_CREDENTIALS_LINE:?} must occur once for the workflow of the beads tracker"
        );
        let lost = template().replace(&format!("{PERSIST_CREDENTIALS_LINE}\n"), "");
        let doubled = format!("{}{PERSIST_CREDENTIALS_LINE}\n", template());

        for text in [lost, doubled] {
            assert_eq!(
                remove_line(&text, PERSIST_CREDENTIALS_LINE, Adapter::Beads),
                Err(expected.clone())
            );
        }
        assert_eq!(
            remove_line("a\nb\nc\n", "b", Adapter::Beads),
            Ok("a\nc\n".to_string())
        );
        assert_eq!(
            remove_line("a\nb\nc", "b", Adapter::Beads),
            Ok("a\nc".to_string())
        );
    }

    /// The `permissions` block of the json, Beads, and no-tracker forms.
    const READ: &str = "    permissions:\n      contents: read\n";

    /// The `permissions` block of the GitHub form.
    const GITHUB_READ: &str = "    permissions:\n      contents: read\n      issues: read\n";

    /// The `ref` input of the checkout step.
    const HEAD: &str = "          ref: ${{ github.event.pull_request.head.sha }}\n";

    /// The input of the checkout step that drops the persisted credential.
    const PERSIST: &str = "          persist-credentials: false\n";

    /// The step of the GitHub form that runs a gate with the token.
    const TOKEN_STEP: &str = "      - run: houserules check-backlog\n        env:\n          GH_TOKEN: ${{ github.token }}\n";

    /// The step of the Beads form that bootstraps the store.
    const BOOTSTRAP_STEP: &str = "      - run: bd bootstrap --yes\n";

    /// A kept copy with the one gates job `job`: `permissions` is the text of
    /// its `permissions` key (four spaces deep, or empty), `with` the inputs of
    /// its checkout step (ten spaces deep), and `extra` further steps before the
    /// step that runs the gates.
    fn kept_copy(job: &str, permissions: &str, with: &str, extra: &str) -> String {
        format!(
            "name: mine\non:\n  pull_request:\njobs:\n  {job}:\n    runs-on: ubuntu-latest\n{permissions}    steps:\n      - uses: actions/checkout@v4\n        with:\n{with}{extra}      - run: houserules check-knowledge\n"
        )
    }

    /// The hand step for the gates job `job` of the copy at `path` that grants
    /// `contents: write` (spec 10.7).
    fn contents_write_step(path: &str, job: &str) -> String {
        format!("{path} grants contents: write in the {job} job; replace it with contents: read")
    }

    /// The hand step for the gates job `job` of the copy at `path` that holds the
    /// scope `scope` with the value `value`, which is not `read` (spec 10.7).
    fn sets_step(path: &str, job: &str, scope: &str, value: &str) -> String {
        format!("{path} sets {scope}: {value} in the {job} job; replace it with {scope}: read")
    }

    /// The hand step for the copy at `path` that has no gates job (spec 10.7).
    fn no_gates_job_step(path: &str) -> String {
        format!(
            "{path} has no job that runs houserules check-knowledge; houserules update checks no permissions there"
        )
    }

    /// The steps for the copy `text` at `w.yml`.
    fn steps_of(adapter: Option<Adapter>, text: &str) -> Vec<String> {
        hand_step_for_kept(adapter, "w.yml", text.as_bytes())
    }

    /// A kept copy gets a hand step for each marker its gates job lacks, in the
    /// order of the file: the steps of the Beads tracker, the permissions, the
    /// env of the GitHub tracker, the `ref` input of the PR-head checkout, and
    /// the dropped persisted credential. The Beads tracker gets the step that
    /// removes the credential input in its place, for a job that holds it. A
    /// marker in a comment is no marker.
    #[test]
    fn a_kept_workflow_gets_the_hand_steps_its_gates_job_lacks_in_the_order_of_the_file() {
        let path = "w.yml";
        let github_permissions = hand_steps::ci_lacks_the_github_permissions(path);
        let issues_read = hand_steps::ci_lacks_issues_read(path);
        let contents_in_block = hand_steps::ci_lacks_contents_read_in_the_permissions(path);
        let github_env = hand_steps::ci_lacks_the_github_env(path);
        let beads = hand_steps::ci_lacks_the_beads_steps(path, BD_VERSION);
        let contents = hand_steps::ci_lacks_contents_read(path);
        let head = hand_steps::ci_lacks_the_head_checkout(path);
        let persist = hand_steps::ci_lacks_persisted_credentials_off(path);
        let drop = hand_steps::ci_holds_persist_credentials_false(path);
        let issues_only = "    permissions:\n      issues: read\n";
        let contents_only = READ;
        let both = GITHUB_READ;
        let head_and_persist = format!("{HEAD}{PERSIST}");
        let comment_persist = format!("{HEAD}          # persist-credentials: false\n");
        let cases: Vec<(Option<Adapter>, String, Vec<String>)> = vec![
            (
                Some(Adapter::Github),
                kept_copy("gates", "", "", ""),
                vec![
                    github_permissions.clone(),
                    github_env.clone(),
                    head.clone(),
                    persist.clone(),
                ],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", issues_only, "", TOKEN_STEP),
                vec![contents_in_block.clone(), head.clone(), persist.clone()],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", issues_only, HEAD, TOKEN_STEP),
                vec![contents_in_block.clone(), persist.clone()],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", issues_only, &head_and_persist, TOKEN_STEP),
                vec![contents_in_block.clone()],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", both, &head_and_persist, TOKEN_STEP),
                vec![],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", contents_only, "", ""),
                vec![
                    issues_read.clone(),
                    github_env.clone(),
                    head.clone(),
                    persist.clone(),
                ],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", contents_only, &head_and_persist, ""),
                vec![issues_read.clone(), github_env.clone()],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", both, &head_and_persist, ""),
                vec![github_env.clone()],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", contents_only, &head_and_persist, TOKEN_STEP),
                vec![issues_read.clone()],
            ),
            (
                Some(Adapter::Github),
                kept_copy("gates", issues_only, &head_and_persist, ""),
                vec![contents_in_block.clone(), github_env.clone()],
            ),
            (
                Some(Adapter::Beads),
                kept_copy("gates", "", "", ""),
                vec![beads.clone(), contents.clone(), head.clone()],
            ),
            (
                Some(Adapter::Beads),
                kept_copy("gates", "", HEAD, BOOTSTRAP_STEP),
                vec![contents.clone()],
            ),
            (
                Some(Adapter::Beads),
                kept_copy("gates", READ, HEAD, BOOTSTRAP_STEP),
                vec![],
            ),
            (
                Some(Adapter::Beads),
                kept_copy("gates", "", PERSIST, ""),
                vec![beads.clone(), contents.clone(), head.clone(), drop.clone()],
            ),
            (
                Some(Adapter::Beads),
                kept_copy("gates", READ, &head_and_persist, BOOTSTRAP_STEP),
                vec![drop.clone()],
            ),
            (
                Some(Adapter::Beads),
                kept_copy("gates", READ, &comment_persist, BOOTSTRAP_STEP),
                vec![],
            ),
            (
                None,
                kept_copy("gates", "", "", ""),
                vec![contents.clone(), head.clone(), persist.clone()],
            ),
            (
                None,
                kept_copy("gates", READ, &head_and_persist, ""),
                vec![],
            ),
            (
                Some(Adapter::Json),
                kept_copy("gates", "", "", ""),
                vec![contents.clone(), head.clone(), persist.clone()],
            ),
            (
                Some(Adapter::Json),
                kept_copy("gates", "", &head_and_persist, ""),
                vec![contents.clone()],
            ),
            (
                Some(Adapter::Json),
                kept_copy("gates", READ, HEAD, ""),
                vec![persist.clone()],
            ),
            (
                Some(Adapter::Json),
                kept_copy("gates", READ, PERSIST, ""),
                vec![head.clone()],
            ),
            (
                Some(Adapter::Json),
                kept_copy("gates", READ, &comment_persist, ""),
                vec![persist.clone()],
            ),
            (
                Some(Adapter::Json),
                kept_copy("gates", READ, &head_and_persist, ""),
                vec![],
            ),
        ];

        for (adapter, text, expected) in cases {
            assert_eq!(steps_of(adapter, &text), expected, "{adapter:?}\n{text}");
        }
    }

    /// The `permissions` key of the gates job decides the hand step for the
    /// permissions of a kept copy (design.md 226), for every adapter: with no key
    /// the step adds the block; with a mapping it adds the scopes the mapping
    /// lacks to it and replaces a needed scope that holds another value than
    /// `read` (`contents: write`, `contents: none`, and `issues: write` among
    /// them; the value shows as a scalar's text, or as `a sequence` or `a
    /// mapping`), so that no step asks for a second key; `read-all` gets none;
    /// and `write-all` is replaced by the block. A quoted scalar, a scalar with a
    /// comment, a flow mapping, and a comment between the key and its scopes are
    /// the same keys. A key in a comment, in another job, or at the workflow
    /// level is no key of the gates job. Every row holds the other markers of its
    /// adapter, so that only the steps for the permissions remain.
    #[test]
    fn the_permissions_key_of_the_gates_job_decides_the_hand_step() {
        let path = "w.yml";
        let block = hand_steps::ci_lacks_contents_read(path);
        let github_block = hand_steps::ci_lacks_the_github_permissions(path);
        let issues = hand_steps::ci_lacks_issues_read(path);
        let contents_in = hand_steps::ci_lacks_contents_read_in_the_permissions(path);
        let github_in = hand_steps::ci_lacks_the_github_scopes_in_the_permissions(path);
        let write_all = hand_steps::ci_grants_write_all(path, false);
        let github_write_all = hand_steps::ci_grants_write_all(path, true);
        let contents_write = contents_write_step(path, "gates");
        let contents_none = sets_step(path, "gates", "contents", "none");
        let contents_null = sets_step(path, "gates", "contents", "null");
        let contents_flag = sets_step(path, "gates", "contents", "true");
        let contents_integer = sets_step(path, "gates", "contents", "1");
        let contents_float = sets_step(path, "gates", "contents", "1.5");
        let contents_tagged = sets_step(path, "gates", "contents", "another value");
        let contents_sequence = sets_step(path, "gates", "contents", "a sequence");
        let contents_mapping = sets_step(path, "gates", "contents", "a mapping");
        let issues_write = sets_step(path, "gates", "issues", "write");
        let issues_none = sets_step(path, "gates", "issues", "none");
        let rows: Vec<(&str, String, Vec<&String>, Vec<&String>)> = vec![
            ("no key", String::new(), vec![&block], vec![&github_block]),
            (
                "a key in a comment",
                "    # permissions: write-all\n".to_string(),
                vec![&block],
                vec![&github_block],
            ),
            (
                "contents: read in a comment",
                "    # permissions:\n    #   contents: read\n".to_string(),
                vec![&block],
                vec![&github_block],
            ),
            (
                "read-all",
                "    permissions: read-all\n".to_string(),
                vec![],
                vec![],
            ),
            (
                "a quoted read-all",
                "    permissions: 'read-all'\n".to_string(),
                vec![],
                vec![],
            ),
            (
                "read-all with a comment",
                "    permissions: read-all # every read scope\n".to_string(),
                vec![],
                vec![],
            ),
            (
                "write-all",
                "    permissions: write-all\n".to_string(),
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "a quoted write-all",
                "    permissions: \"write-all\"\n".to_string(),
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "write-all with a comment",
                "    permissions: write-all # for now\n".to_string(),
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "write-all beside the markers in comments",
                "    permissions: write-all\n    # contents: read\n    # issues: read\n"
                    .to_string(),
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "a mapping of another scope",
                "    permissions:\n      pull-requests: read\n".to_string(),
                vec![&contents_in],
                vec![&github_in],
            ),
            (
                "a flow mapping of another scope",
                "    permissions: {pull-requests: read}\n".to_string(),
                vec![&contents_in],
                vec![&github_in],
            ),
            (
                "an empty flow mapping",
                "    permissions: {}\n".to_string(),
                vec![&contents_in],
                vec![&github_in],
            ),
            (
                "a key with no value",
                "    permissions:\n".to_string(),
                vec![&contents_in],
                vec![&github_in],
            ),
            (
                "contents: read only",
                READ.to_string(),
                vec![],
                vec![&issues],
            ),
            (
                "a flow mapping of contents: read",
                "    permissions: {contents: read}\n".to_string(),
                vec![],
                vec![&issues],
            ),
            (
                "issues: read only",
                "    permissions:\n      issues: read\n".to_string(),
                vec![&contents_in],
                vec![&contents_in],
            ),
            ("both scopes", GITHUB_READ.to_string(), vec![], vec![]),
            (
                "a comment between the key and its scopes",
                "    permissions: # the token\n      # the reads\n      contents: read\n"
                    .to_string(),
                vec![],
                vec![&issues],
            ),
            (
                "contents: write",
                "    permissions:\n      contents: write\n".to_string(),
                vec![&contents_write],
                vec![&contents_write, &issues],
            ),
            (
                "a flow mapping of contents: write",
                "    permissions: {contents: write}\n".to_string(),
                vec![&contents_write],
                vec![&contents_write, &issues],
            ),
            (
                "contents: write beside issues: read",
                "    permissions:\n      contents: write\n      issues: read\n".to_string(),
                vec![&contents_write],
                vec![&contents_write],
            ),
            (
                "contents: read beside another write scope",
                "    permissions:\n      contents: read\n      pull-requests: write\n".to_string(),
                vec![],
                vec![&issues],
            ),
            (
                "contents: none",
                "    permissions:\n      contents: none\n".to_string(),
                vec![&contents_none],
                vec![&contents_none, &issues],
            ),
            (
                "a flow mapping of contents: none",
                "    permissions: {contents: none}\n".to_string(),
                vec![&contents_none],
                vec![&contents_none, &issues],
            ),
            (
                "contents: none beside issues: read",
                "    permissions:\n      contents: none\n      issues: read\n".to_string(),
                vec![&contents_none],
                vec![&contents_none],
            ),
            (
                "contents with no value",
                "    permissions:\n      contents:\n".to_string(),
                vec![&contents_null],
                vec![&contents_null, &issues],
            ),
            (
                "contents: true",
                "    permissions:\n      contents: true\n".to_string(),
                vec![&contents_flag],
                vec![&contents_flag, &issues],
            ),
            (
                "contents: 1",
                "    permissions:\n      contents: 1\n".to_string(),
                vec![&contents_integer],
                vec![&contents_integer, &issues],
            ),
            (
                "contents: 1.5",
                "    permissions:\n      contents: 1.5\n".to_string(),
                vec![&contents_float],
                vec![&contents_float, &issues],
            ),
            (
                "contents with a tag",
                "    permissions:\n      contents: !custom read\n".to_string(),
                vec![&contents_tagged],
                vec![&contents_tagged, &issues],
            ),
            (
                "contents as a sequence",
                "    permissions:\n      contents: [read]\n".to_string(),
                vec![&contents_sequence],
                vec![&contents_sequence, &issues],
            ),
            (
                "contents as a mapping",
                "    permissions:\n      contents: {read: true}\n".to_string(),
                vec![&contents_mapping],
                vec![&contents_mapping, &issues],
            ),
            (
                "contents: read beside issues: write",
                "    permissions:\n      contents: read\n      issues: write\n".to_string(),
                vec![],
                vec![&issues_write],
            ),
            (
                "contents: read beside issues: none",
                "    permissions: {contents: read, issues: none}\n".to_string(),
                vec![],
                vec![&issues_none],
            ),
            (
                "issues: write only",
                "    permissions:\n      issues: write\n".to_string(),
                vec![&contents_in],
                vec![&contents_in, &issues_write],
            ),
            (
                "contents: write beside issues: write",
                "    permissions:\n      contents: write\n      issues: write\n".to_string(),
                vec![&contents_write],
                vec![&contents_write, &issues_write],
            ),
        ];

        for (label, permissions, others, github) in rows {
            for (adapter, expected) in [
                (None, &others),
                (Some(Adapter::Json), &others),
                (Some(Adapter::Beads), &others),
                (Some(Adapter::Github), &github),
            ] {
                let (with, extra) = match adapter {
                    Some(Adapter::Github) => (format!("{HEAD}{PERSIST}"), TOKEN_STEP),
                    Some(Adapter::Beads) => (HEAD.to_string(), BOOTSTRAP_STEP),
                    _ => (format!("{HEAD}{PERSIST}"), ""),
                };
                let copy = kept_copy("gates", &permissions, &with, extra);
                let expected: Vec<String> = expected.iter().map(|step| (*step).clone()).collect();

                assert_eq!(
                    steps_of(adapter, &copy),
                    expected,
                    "{label}, {adapter:?}\n{copy}"
                );
            }
        }
    }

    /// A key of `permissions` that another job or the workflow holds decides
    /// nothing for the gates job, wherever it stands in the file.
    #[test]
    fn a_permissions_key_outside_the_gates_job_decides_nothing() {
        let path = "w.yml";
        let full = format!("{HEAD}{PERSIST}");
        let before = format!(
            "jobs:\n  docs:\n    permissions: write-all\n    steps: []\n  gates:\n    permissions:\n      contents: read\n    steps:\n      - uses: actions/checkout@v4\n        with:\n{full}      - run: houserules check-knowledge\n"
        );
        let after = format!(
            "{}  docs:\n    permissions: write-all\n    steps: []\n",
            kept_copy("gates", READ, &full, "")
        );
        let workflow_level = format!(
            "permissions: read-all\n{}",
            kept_copy("gates", "", &full, "")
        );
        let workflow_write = format!(
            "permissions:\n  contents: write\n{}",
            kept_copy("gates", READ, &full, "")
        );

        assert_eq!(steps_of(None, &before), Vec::<String>::new(), "{before}");
        assert_eq!(steps_of(None, &after), Vec::<String>::new(), "{after}");
        assert_eq!(
            steps_of(None, &workflow_level),
            [hand_steps::ci_lacks_contents_read(path)],
            "{workflow_level}"
        );
        assert_eq!(
            steps_of(None, &workflow_write),
            Vec::<String>::new(),
            "{workflow_write}"
        );
    }

    /// HR-243: the reader finds the gates job whatever its name and its
    /// indentation, and judges each of two gates jobs by its own keys. A job
    /// that holds `contents: write` is named in its step.
    #[test]
    fn the_gates_job_is_found_whatever_its_name_and_indentation() {
        let path = "w.yml";
        let full = format!("{HEAD}{PERSIST}");
        let write = "    permissions:\n      contents: write\n";

        let renamed = kept_copy("knowledge-gate", write, &full, "");
        assert_eq!(
            steps_of(None, &renamed),
            [contents_write_step(path, "knowledge-gate")],
            "{renamed}"
        );

        let reindented = "jobs:\n    checks:\n        permissions:\n            contents: write\n        steps:\n            - uses: actions/checkout@v4\n              with:\n                  ref: ${{ github.event.pull_request.head.sha }}\n                  persist-credentials: false\n            - run: |\n                  houserules check-knowledge\n                  houserules check-backlog\n";
        assert_eq!(
            steps_of(None, reindented),
            [contents_write_step(path, "checks")],
            "{reindented}"
        );

        let two = format!(
            "{}  second:\n    permissions:\n      contents: write\n    steps:\n      - uses: actions/checkout@v4\n        with:\n{full}      - run: houserules check-knowledge\n",
            kept_copy("first", write, &full, "")
        );
        assert_eq!(
            steps_of(None, &two),
            [
                contents_write_step(path, "first"),
                contents_write_step(path, "second"),
            ],
            "{two}"
        );

        let shared = format!(
            "{}  second:\n    steps:\n      - uses: actions/checkout@v4\n        with:\n{full}      - run: houserules check-knowledge\n",
            kept_copy("first", "", &full, "")
        );
        assert_eq!(
            steps_of(None, &shared),
            [hand_steps::ci_lacks_contents_read(path)],
            "two jobs that lack the same marker give one step\n{shared}"
        );
    }

    /// A marker that another job holds is no marker of the gates job: the
    /// markers of a copy are read per job.
    #[test]
    fn a_marker_of_another_job_is_no_marker_of_the_gates_job() {
        let path = "w.yml";
        let other_job = "  docs:\n    permissions:\n      contents: read\n      issues: read\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          ref: ${{ github.event.pull_request.head.sha }}\n          persist-credentials: false\n      - run: bd bootstrap --yes\n        env:\n          GH_TOKEN: ${{ github.token }}\n";
        for (adapter, expected) in [
            (
                None,
                vec![
                    hand_steps::ci_lacks_contents_read(path),
                    hand_steps::ci_lacks_the_head_checkout(path),
                    hand_steps::ci_lacks_persisted_credentials_off(path),
                ],
            ),
            (
                Some(Adapter::Github),
                vec![
                    hand_steps::ci_lacks_the_github_permissions(path),
                    hand_steps::ci_lacks_the_github_env(path),
                    hand_steps::ci_lacks_the_head_checkout(path),
                    hand_steps::ci_lacks_persisted_credentials_off(path),
                ],
            ),
            (
                Some(Adapter::Beads),
                vec![
                    hand_steps::ci_lacks_the_beads_steps(path, BD_VERSION),
                    hand_steps::ci_lacks_contents_read(path),
                    hand_steps::ci_lacks_the_head_checkout(path),
                ],
            ),
        ] {
            let copy = format!("{}{other_job}", kept_copy("gates", "", "", ""));

            assert_eq!(steps_of(adapter, &copy), expected, "{adapter:?}\n{copy}");
        }
    }

    /// The token of the GitHub form counts at the job and in a step alike, and
    /// the PR-head `ref` and the credential input count only on a checkout step.
    #[test]
    fn the_markers_stand_where_the_workflow_puts_them() {
        let path = "w.yml";
        let job_env = "    env:\n      GH_TOKEN: ${{ github.token }}\n";
        let copy = format!(
            "jobs:\n  gates:\n{job_env}{GITHUB_READ}    steps:\n      - uses: actions/checkout@v4\n        with:\n{HEAD}{PERSIST}      - run: houserules check-knowledge\n"
        );
        assert_eq!(
            steps_of(Some(Adapter::Github), &copy),
            Vec::<String>::new(),
            "{copy}"
        );

        let not_a_checkout = format!(
            "jobs:\n  gates:\n{READ}    steps:\n      - uses: actions/cache@v4\n        with:\n{HEAD}{PERSIST}      - run: houserules check-knowledge\n"
        );
        assert_eq!(
            steps_of(None, &not_a_checkout),
            [
                hand_steps::ci_lacks_the_head_checkout(path),
                hand_steps::ci_lacks_persisted_credentials_off(path)
            ],
            "{not_a_checkout}"
        );

        let string_false = format!(
            "jobs:\n  gates:\n{READ}    steps:\n      - uses: actions/checkout@abc # v4\n        with:\n{HEAD}          persist-credentials: 'false'\n      - run: houserules check-knowledge\n"
        );
        assert_eq!(
            steps_of(None, &string_false),
            Vec::<String>::new(),
            "{string_false}"
        );
    }

    /// A copy with no job that runs `houserules check-knowledge` gets one step
    /// and no other, and so does a file that is no YAML; the second names the
    /// parser's line.
    #[test]
    fn a_copy_with_no_gates_job_or_no_yaml_gets_one_step() {
        let path = "w.yml";
        let no_job = "jobs:\n  lint:\n    steps:\n      - run: houserules check-backlog\n";
        for text in [no_job, "name: mine\n", "", "just text\n", "jobs: [1, 2]\n"] {
            assert_eq!(
                steps_of(Some(Adapter::Github), text),
                [no_gates_job_step(path)],
                "{text:?}"
            );
        }

        let invalid =
            "jobs:\n  gates:\n    steps: [unclosed\n      - run: houserules check-knowledge\n";
        let steps = steps_of(None, invalid);
        assert_eq!(steps.len(), 1, "{steps:?}");
        assert!(
            steps[0].starts_with("w.yml is not valid YAML (")
                && steps[0].ends_with("); houserules update checks no permissions there"),
            "{steps:?}"
        );
        assert!(!steps[0].contains('\n'), "{steps:?}");
    }

    /// The first document of a file with more than one is the workflow.
    #[test]
    fn the_first_document_of_a_copy_is_the_workflow() {
        let full = format!("{HEAD}{PERSIST}");
        let copy = format!(
            "{}---\nother: document\n",
            kept_copy("gates", READ, &full, "")
        );

        assert_eq!(steps_of(None, &copy), Vec::<String>::new(), "{copy}");
    }

    /// The seeded forms of the kit, kept as they are, need no hand step: the
    /// reader and the forms agree.
    #[test]
    fn every_seeded_form_needs_no_hand_step() {
        for adapter in [
            None,
            Some(Adapter::Json),
            Some(Adapter::Github),
            Some(Adapter::Beads),
        ] {
            let template = template_for(adapter, "WI").expect("a template");
            for components in [&[][..], &["sub"][..]] {
                let workflow =
                    Workflow::for_install(&placement(components), &template).expect("a workflow");

                assert_eq!(
                    hand_step_for_kept(adapter, &workflow.path, &workflow.content),
                    Vec::<String>::new(),
                    "{adapter:?} {components:?}"
                );
            }
        }
    }
}
