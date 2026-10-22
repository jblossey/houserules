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
//! and no tracker keep the steps of the template. GitHub adds a job-level
//! `permissions` block and the token to the steps that run `check-backlog` and
//! `audit`, because `gh` reads the issues of the repository. Beads adds a step
//! that installs `bd` at the release that `mise.toml` pins and a step that
//! runs `bd bootstrap`, because a fresh clone has no Beads store. Every form
//! checks out the head of a pull request: the `actions/checkout` step carries
//! `ref: ${{ github.event.pull_request.head.sha }}` (spec 3.2), so that every
//! step of the job, the audit included, judges the tree that lands.
//! [`template_for`] builds the text from the template, and [`write_for`] keeps
//! the file of an install at the content that its tracker calls for: every
//! command that sets `tracker` calls it, and so does `update`.

use std::path::Path;

use super::hand_steps;
use super::location::{self, Placement};
use super::marker::Marker;
use super::sync::sync_file;
use super::{is_overridden, payload_content};
use crate::tracker::Adapter;
use crate::tracker::beads::BD_VERSION;

/// The template's own path, below the git top level.
pub(super) const TEMPLATE_PATH: &str = ".github/workflows/knowledge.yml";

/// The step of the template that runs `check-backlog`.
const CHECK_BACKLOG_LINE: &str = "      - run: houserules check-backlog";

/// The last line of the step of the template that runs `audit`.
const AUDIT_LAST_LINE: &str = "          --head ${{ github.event.pull_request.head.sha }}";

/// The job-level permissions of the GitHub form: `gh` reads the issues.
const GITHUB_PERMISSIONS: &str = "    permissions:\n      contents: read\n      issues: read";

/// The env of a step of the GitHub form that runs a gate: `gh` takes its token
/// from `GH_TOKEN`.
const GITHUB_TOKEN_ENV: &str = "        env:\n          GH_TOKEN: ${{ github.token }}";

/// The text that the project must hold for the GitHub form: the read of the
/// issues and the token. A kept workflow that holds both needs no hand step.
const GITHUB_MARKERS: [&str; 2] = ["issues: read", "GH_TOKEN: ${{ github.token }}"];

/// The text that the project must hold for the Beads form.
const BEADS_MARKER: &str = "bd bootstrap";

/// The text that the project must hold for every form: the `ref` input of the
/// checkout step, which makes every step of the job judge the head of a pull
/// request (spec 3.2).
const HEAD_CHECKOUT_INPUT: &str = "ref: ${{ github.event.pull_request.head.sha }}";

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
    if template.lines().filter(|line| *line == anchor).count() != 1 {
        return Err(format!(
            "{TEMPLATE_PATH}: the line {anchor:?} must occur once for the workflow of the {} tracker",
            adapter.as_str()
        ));
    }
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
    let mut text = lines.join("\n");
    if template.ends_with('\n') {
        text.push('\n');
    }
    Ok(text)
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
            let text = insert_at_line(text, RUNS_ON_LINE, GITHUB_PERMISSIONS, true, github)?;
            let text = insert_at_line(&text, CHECK_BACKLOG_LINE, GITHUB_TOKEN_ENV, true, github)?;
            insert_at_line(&text, AUDIT_LAST_LINE, GITHUB_TOKEN_ENV, true, github)?
        }
        Some(Adapter::Beads) => insert_at_line(
            text,
            CHECK_BACKLOG_LINE,
            &beads_steps(),
            false,
            Adapter::Beads,
        )?,
    };
    Ok(form.into_bytes())
}

/// The hand steps for a seeded CI workflow that the project changed and that
/// `update` kept (spec 4.8 and 3.2). The file at `path` holds the bytes
/// `current`. The first hand step names the lines that the tracker `adapter`
/// needs when the text lacks them; JSON, no tracker, and a file that holds the
/// lines of its adapter get none. The last names the `ref` input of the
/// checkout (`HEAD_CHECKOUT_INPUT`) for every tracker whose file lacks it.
pub(super) fn hand_step_for_kept(
    adapter: Option<Adapter>,
    path: &str,
    current: &[u8],
) -> Vec<String> {
    let text = String::from_utf8_lossy(current);
    let mut steps = Vec::new();
    match adapter {
        Some(Adapter::Github) if GITHUB_MARKERS.iter().any(|needle| !text.contains(needle)) => {
            steps.push(hand_steps::ci_lacks_the_github_lines(path));
        }
        Some(Adapter::Beads) if !text.contains(BEADS_MARKER) => {
            steps.push(hand_steps::ci_lacks_the_beads_steps(path, BD_VERSION));
        }
        _ => {}
    }
    if !text.contains(HEAD_CHECKOUT_INPUT) {
        steps.push(hand_steps::ci_lacks_the_head_checkout(path));
    }
    steps
}

/// Brings the seeded CI workflow of the install at `root` to the content that
/// the tracker `adapter` (or none) calls for, as `update` keeps a D3 file: a
/// workflow that `marker` baselines, and that the project neither changed nor
/// owns, is written when its bytes would change, and the baseline moves with
/// it. A workflow with no baseline (an install that has none, or one that
/// `init` skipped for a host other than GitHub) is left as found. Returns the
/// lines that name each file written or kept, and the hand steps of a kept file.
pub(super) fn write_for(
    root: &Path,
    marker: &mut Marker,
    adapter: Option<Adapter>,
) -> Result<Vec<String>, String> {
    let placement = location::locate(root)?;
    let template = template_for(adapter, &marker.id_prefix_or("WI"))?;
    let workflow = Workflow::for_install(&placement, &template)?;
    let mut lines = Vec::new();
    if is_overridden(&marker.owned_paths(), &workflow.path) {
        return Ok(lines);
    }
    let mut baselines = marker.baselines();
    if baselines.contains_key(&workflow.key) {
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

    /// The GitHub form adds the permissions once and the token to the two gate
    /// steps; the Beads form adds the two steps before the `check-backlog`
    /// step, which stays, and no token.
    #[test]
    fn the_forms_of_github_and_beads_add_only_their_lines() {
        let github = String::from_utf8(template_for(Some(Adapter::Github), "WI").expect("form"))
            .expect("UTF-8");
        assert_eq!(github.matches(GITHUB_PERMISSIONS).count(), 1);
        assert_eq!(github.matches(GITHUB_TOKEN_ENV).count(), 2);
        let without: String = github
            .replace(&format!("\n{GITHUB_PERMISSIONS}"), "")
            .replace(&format!("\n{GITHUB_TOKEN_ENV}"), "");
        assert_eq!(without, template());

        let beads = String::from_utf8(template_for(Some(Adapter::Beads), "WI").expect("form"))
            .expect("UTF-8");
        let added = format!("{}\n", beads_steps());
        assert_eq!(beads.replace(&added, ""), template());
        assert!(beads.contains(&format!("beads_{BD_VERSION}_linux_amd64.tar.gz")));
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

    /// A kept workflow gets the hand step of its adapter only while it lacks
    /// what the adapter needs, and the hand step of the PR-head checkout, after
    /// the one of its adapter, only while it lacks the `ref` input.
    #[test]
    fn a_kept_workflow_gets_the_hand_steps_it_lacks_the_tracker_first() {
        let path = "w.yml";
        let head = hand_steps::ci_lacks_the_head_checkout(path);
        let with_head = format!("{HEAD_CHECKOUT_INPUT}\n");
        assert_eq!(
            hand_step_for_kept(Some(Adapter::Github), path, b"name: mine\n"),
            [hand_steps::ci_lacks_the_github_lines(path), head.clone()]
        );
        let github_lines = "permissions: issues: read\nGH_TOKEN: ${{ github.token }}\n";
        assert_eq!(
            hand_step_for_kept(Some(Adapter::Github), path, github_lines.as_bytes()),
            std::slice::from_ref(&head)
        );
        assert_eq!(
            hand_step_for_kept(
                Some(Adapter::Github),
                path,
                format!("{github_lines}{with_head}").as_bytes()
            ),
            Vec::<String>::new()
        );
        assert_eq!(
            hand_step_for_kept(Some(Adapter::Beads), path, b"name: mine\n"),
            [
                hand_steps::ci_lacks_the_beads_steps(path, BD_VERSION),
                head.clone()
            ]
        );
        assert_eq!(
            hand_step_for_kept(
                Some(Adapter::Beads),
                path,
                format!("run: bd bootstrap --yes\n{with_head}").as_bytes()
            ),
            Vec::<String>::new()
        );
        for adapter in [None, Some(Adapter::Json)] {
            assert_eq!(
                hand_step_for_kept(adapter, path, b"name: mine\n"),
                std::slice::from_ref(&head)
            );
            assert_eq!(
                hand_step_for_kept(adapter, path, with_head.as_bytes()),
                Vec::<String>::new()
            );
        }
    }
}
