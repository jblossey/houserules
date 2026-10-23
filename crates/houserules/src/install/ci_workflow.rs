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

/// The text that the project must hold for the GitHub form: the read of the
/// issues, beside `CONTENTS_READ_MARKER` in the permissions of the gates job.
const ISSUES_READ_MARKER: &str = "issues: read";

/// The line of the template that opens the gates job, two spaces deep under
/// `jobs:`. The job ends at the next line that is indented by two spaces or fewer.
const GATES_JOB_LINE: &str = "  gates:";

/// The start of the line of the permissions of the gates job: the key, four
/// spaces deep, where the template has it.
const PERMISSIONS_KEY: &str = "    permissions:";

/// The text that the project must hold for the GitHub form: the token of the
/// steps that run a gate.
const GITHUB_TOKEN_MARKER: &str = "GH_TOKEN: ${{ github.token }}";

/// The text that the project must hold for the Beads form.
const BEADS_MARKER: &str = "bd bootstrap";

/// The text that the project must hold for every form: the `ref` input of the
/// checkout step, which makes every step of the job judge the head of a pull
/// request (spec 3.2).
const HEAD_CHECKOUT_INPUT: &str = "ref: ${{ github.event.pull_request.head.sha }}";

/// The text that the project must hold for every form: the read-only token of
/// the job. A file that lacks it gets the permissions step that
/// `permissions_hand_step` picks from the `permissions` key of the gates job: for
/// every tracker but GitHub, `ci_lacks_contents_read` where the job holds no key,
/// and a step that adds the scope to the mapping or replaces `write-all` where it
/// holds one. The GitHub tracker reads it with `ISSUES_READ_MARKER`, so that no
/// file is asked for two `permissions` keys.
const CONTENTS_READ_MARKER: &str = "contents: read";

/// The text that the project must hold for every form but Beads: the input of
/// the checkout step that drops the persisted credential. The Beads form must
/// not hold it: `bd bootstrap --yes` needs the credential (spec 8.5, C11).
const PERSIST_CREDENTIALS_MARKER: &str = "persist-credentials: false";

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

/// What the gates job of a kept copy holds under its `permissions` key.
enum JobPermissions {
    /// The job has no `permissions` key.
    Absent,
    /// A block of scopes, or a flow mapping such as `{contents: read}`.
    Mapping,
    /// The scalar `read-all`, which holds every read scope.
    ReadAll,
    /// The scalar `write-all`, which holds every scope.
    WriteAll,
}

impl JobPermissions {
    /// The state of a key whose text after the colon is `value`: a scalar, with
    /// or without quotes and a trailing comment, is `read-all` or `write-all`;
    /// anything else, the empty value of a block included, is a mapping.
    fn of_value(value: &str) -> Self {
        let scalar = value
            .split(" #")
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches(['\'', '"']);
        match scalar {
            "read-all" => Self::ReadAll,
            "write-all" => Self::WriteAll,
            _ => Self::Mapping,
        }
    }
}

/// The `permissions` key that the gates job of the kept copy `text` holds. The
/// reader tests the lines of the job and no more: the job begins at
/// `GATES_JOB_LINE` and ends at the next line indented by two spaces or fewer,
/// and a comment or a blank line ends nothing. The key is the line that starts
/// with `PERMISSIONS_KEY`; a key in a comment, in another job, at the workflow
/// level, or at another indentation is no key of the job. A copy that lacks
/// `GATES_JOB_LINE` has no key.
fn job_permissions(text: &str) -> JobPermissions {
    let job_indent = GATES_JOB_LINE.len() - GATES_JOB_LINE.trim_start().len();
    let lines = text
        .lines()
        .skip_while(|line| line.trim_end() != GATES_JOB_LINE)
        .skip(1);
    for line in lines {
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') {
            continue;
        }
        if line.len() - content.len() <= job_indent {
            break;
        }
        if let Some(value) = line.strip_prefix(PERMISSIONS_KEY) {
            return JobPermissions::of_value(value);
        }
    }
    JobPermissions::Absent
}

/// The hand step for the permissions of a kept copy at `path` that holds the
/// bytes `text`, or none (spec 8.5, design.md 5.226). The `permissions` key of
/// the gates job decides its form, so that no copy is asked for a second key:
///
/// - `read-all` holds every read scope and gets none.
/// - `write-all` is replaced by the block of the form.
/// - A mapping gets the scopes it lacks added to it: `contents: read` for every
///   tracker but GitHub, which reads `contents: read` and `issues: read`.
/// - A job with no key gets the block: `permissions: {contents: read}`, and for
///   the GitHub tracker the whole block when the copy holds neither marker, or
///   the one missing line when it holds the other.
fn permissions_hand_step(adapter: Option<Adapter>, path: &str, text: &str) -> Option<String> {
    let github = adapter == Some(Adapter::Github);
    let contents = text.contains(CONTENTS_READ_MARKER);
    let issues = text.contains(ISSUES_READ_MARKER);
    match (job_permissions(text), github, contents, issues) {
        (JobPermissions::ReadAll, ..) => None,
        (JobPermissions::WriteAll, github, ..) => {
            Some(hand_steps::ci_grants_write_all(path, github))
        }
        (_, false, true, _) | (_, true, true, true) => None,
        (_, true, true, false) => Some(hand_steps::ci_lacks_issues_read(path)),
        (_, true, false, true) | (JobPermissions::Mapping, false, false, _) => {
            Some(hand_steps::ci_lacks_contents_read_in_the_permissions(path))
        }
        (JobPermissions::Mapping, true, false, false) => Some(
            hand_steps::ci_lacks_the_github_scopes_in_the_permissions(path),
        ),
        (JobPermissions::Absent, true, false, false) => {
            Some(hand_steps::ci_lacks_the_github_permissions(path))
        }
        (JobPermissions::Absent, false, false, _) => Some(hand_steps::ci_lacks_contents_read(path)),
    }
}

/// The hand steps for a seeded CI workflow that the project changed and that
/// `update` kept. The file at `path` holds the bytes `current`. Each step is for
/// a marker that the text lacks, or for the credential input that the Beads
/// tracker cannot use, in the order of the file:
///
/// 1. The steps of the Beads tracker (`BEADS_MARKER`).
/// 2. The permissions of the gates job: the read-only token
///    (`CONTENTS_READ_MARKER`) for every tracker, and `issues: read`
///    (`ISSUES_READ_MARKER`) for the GitHub tracker. `permissions_hand_step` picks
///    the step from the `permissions` key of the job, and a file that holds the
///    markers gets none.
/// 3. The `GH_TOKEN` env of the steps that run a gate, for the GitHub tracker
///    (`GITHUB_TOKEN_MARKER`).
/// 4. The `ref` input of the checkout (`HEAD_CHECKOUT_INPUT`).
/// 5. The `persist-credentials` input of the checkout
///    (`PERSIST_CREDENTIALS_MARKER`): the step adds it for every tracker but
///    Beads, and the Beads tracker gets the step that removes it from a file
///    that holds it.
pub(super) fn hand_step_for_kept(
    adapter: Option<Adapter>,
    path: &str,
    current: &[u8],
) -> Vec<String> {
    let text = String::from_utf8_lossy(current);
    let mut steps = Vec::new();
    if adapter == Some(Adapter::Beads) && !text.contains(BEADS_MARKER) {
        steps.push(hand_steps::ci_lacks_the_beads_steps(path, BD_VERSION));
    }
    steps.extend(permissions_hand_step(adapter, path, &text));
    if adapter == Some(Adapter::Github) && !text.contains(GITHUB_TOKEN_MARKER) {
        steps.push(hand_steps::ci_lacks_the_github_env(path));
    }
    if !text.contains(HEAD_CHECKOUT_INPUT) {
        steps.push(hand_steps::ci_lacks_the_head_checkout(path));
    }
    let holds_persist_credentials = text.contains(PERSIST_CREDENTIALS_MARKER);
    if adapter == Some(Adapter::Beads) {
        if holds_persist_credentials {
            steps.push(hand_steps::ci_holds_persist_credentials_false(path));
        }
    } else if !holds_persist_credentials {
        steps.push(hand_steps::ci_lacks_persisted_credentials_off(path));
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

    /// A kept workflow whose gates job holds no `permissions` key gets a hand step
    /// for each marker it lacks, in the order of the file: the steps of the Beads
    /// tracker, the permissions (the read-only token for every tracker, and for
    /// GitHub the markers `contents: read` and `issues: read` that the copy
    /// holds), the env of the GitHub tracker, the `ref` input of the PR-head
    /// checkout, and the dropped persisted credential. The Beads tracker gets the
    /// step that removes the credential input in its place, for a copy that holds
    /// it. The texts are the lines of spec 8.5.
    #[test]
    fn a_kept_workflow_gets_the_hand_steps_it_lacks_in_the_order_of_the_file() {
        let path = "w.yml";
        let github = format!(
            "{path} lacks the permissions that the github tracker needs; add permissions: {{contents: read, issues: read}} to the gates job"
        );
        let issues_read = format!(
            "{path} lacks issues: read, which the github tracker needs; add issues: read to the permissions of the gates job"
        );
        let contents_beside_issues = format!(
            "{path} lacks contents: read; add contents: read to the permissions of the gates job"
        );
        let github_env = format!(
            "{path} lacks the GH_TOKEN env that the github tracker needs; add env: {{GH_TOKEN: ${{{{ github.token }}}}}} to each step that runs houserules check-backlog or houserules audit"
        );
        let drop = hand_steps::ci_holds_persist_credentials_false(path);
        let beads = hand_steps::ci_lacks_the_beads_steps(path, BD_VERSION);
        let contents = hand_steps::ci_lacks_contents_read(path);
        let head = hand_steps::ci_lacks_the_head_checkout(path);
        let persist = hand_steps::ci_lacks_persisted_credentials_off(path);
        let issues_marker = "issues: read";
        let token_marker = "GH_TOKEN: ${{ github.token }}";
        let github_lines = format!("{issues_marker}\n{token_marker}\n");
        let beads_lines = "run: bd bootstrap --yes\n";
        let common = format!(
            "{HEAD_CHECKOUT_INPUT}\n{CONTENTS_READ_MARKER}\n{PERSIST_CREDENTIALS_MARKER}\n"
        );
        let cases: Vec<(Option<Adapter>, String, Vec<String>)> = vec![
            (
                Some(Adapter::Github),
                "name: mine\n".to_string(),
                vec![
                    github.clone(),
                    github_env.clone(),
                    head.clone(),
                    persist.clone(),
                ],
            ),
            (
                Some(Adapter::Github),
                github_lines.to_string(),
                vec![
                    contents_beside_issues.clone(),
                    head.clone(),
                    persist.clone(),
                ],
            ),
            (
                Some(Adapter::Github),
                format!("{github_lines}{HEAD_CHECKOUT_INPUT}\n"),
                vec![contents_beside_issues.clone(), persist.clone()],
            ),
            (
                Some(Adapter::Github),
                format!("{github_lines}{HEAD_CHECKOUT_INPUT}\n{PERSIST_CREDENTIALS_MARKER}\n"),
                vec![contents_beside_issues.clone()],
            ),
            (
                Some(Adapter::Github),
                format!("{github_lines}{common}"),
                vec![],
            ),
            (
                Some(Adapter::Github),
                format!("{CONTENTS_READ_MARKER}\n"),
                vec![
                    issues_read.clone(),
                    github_env.clone(),
                    head.clone(),
                    persist.clone(),
                ],
            ),
            (
                Some(Adapter::Github),
                common.clone(),
                vec![issues_read.clone(), github_env.clone()],
            ),
            (
                Some(Adapter::Github),
                format!("{common}{issues_marker}\n"),
                vec![github_env.clone()],
            ),
            (
                Some(Adapter::Github),
                format!("{common}{token_marker}\n"),
                vec![issues_read.clone()],
            ),
            (
                Some(Adapter::Github),
                format!("{HEAD_CHECKOUT_INPUT}\n{PERSIST_CREDENTIALS_MARKER}\n{issues_marker}\n"),
                vec![contents_beside_issues.clone(), github_env.clone()],
            ),
            (
                Some(Adapter::Beads),
                "name: mine\n".to_string(),
                vec![beads.clone(), contents.clone(), head.clone()],
            ),
            (
                Some(Adapter::Beads),
                format!("{beads_lines}{HEAD_CHECKOUT_INPUT}\n"),
                vec![contents.clone()],
            ),
            (
                Some(Adapter::Beads),
                format!("{beads_lines}{HEAD_CHECKOUT_INPUT}\n{CONTENTS_READ_MARKER}\n"),
                vec![],
            ),
            (
                Some(Adapter::Beads),
                format!("{PERSIST_CREDENTIALS_MARKER}\n"),
                vec![beads.clone(), contents.clone(), head.clone(), drop.clone()],
            ),
            (
                Some(Adapter::Beads),
                format!("{beads_lines}{common}"),
                vec![drop.clone()],
            ),
            (
                Some(Adapter::Beads),
                format!(
                    "{beads_lines}{HEAD_CHECKOUT_INPUT}\n{CONTENTS_READ_MARKER}\n# {PERSIST_CREDENTIALS_MARKER}\n"
                ),
                vec![drop.clone()],
            ),
            (
                None,
                "name: mine\n".to_string(),
                vec![contents.clone(), head.clone(), persist.clone()],
            ),
            (None, common.clone(), vec![]),
            (
                Some(Adapter::Json),
                "name: mine\n".to_string(),
                vec![contents.clone(), head.clone(), persist.clone()],
            ),
            (
                Some(Adapter::Json),
                format!("{HEAD_CHECKOUT_INPUT}\n{PERSIST_CREDENTIALS_MARKER}\n"),
                vec![contents.clone()],
            ),
            (
                Some(Adapter::Json),
                format!("{HEAD_CHECKOUT_INPUT}\n{CONTENTS_READ_MARKER}\n"),
                vec![persist.clone()],
            ),
            (
                Some(Adapter::Json),
                format!("{CONTENTS_READ_MARKER}\n{PERSIST_CREDENTIALS_MARKER}\n"),
                vec![head.clone()],
            ),
            (Some(Adapter::Json), common, vec![]),
        ];

        for (adapter, text, expected) in cases {
            assert_eq!(
                hand_step_for_kept(adapter, path, text.as_bytes()),
                expected,
                "{adapter:?} {text:?}"
            );
        }
    }

    /// The `permissions` key of the gates job decides the hand step for the
    /// permissions of a kept copy (spec 8.5, design.md 5.226), for every adapter:
    /// with no key the step adds the block (the whole block for the GitHub
    /// tracker, by the markers the copy holds); with a mapping it adds the scopes
    /// the mapping lacks to it; `read-all` gets none; `write-all` is replaced by
    /// the block. The key is read from the lines of the gates job: a quoted
    /// scalar, a scalar with a comment, and a flow mapping are the same key, and a
    /// key in a comment, in another job, or at the workflow level is no key of the
    /// gates job. Every row holds the other markers of its adapter, so that only
    /// the steps for the permissions remain.
    #[test]
    fn a_kept_workflow_follows_the_permissions_key_of_the_gates_job() {
        let path = "w.yml";
        let block = format!(
            "{path} lacks contents: read; add permissions: {{contents: read}} to the gates job"
        );
        let github_block = format!(
            "{path} lacks the permissions that the github tracker needs; add permissions: {{contents: read, issues: read}} to the gates job"
        );
        let issues = format!(
            "{path} lacks issues: read, which the github tracker needs; add issues: read to the permissions of the gates job"
        );
        let contents_in = format!(
            "{path} lacks contents: read; add contents: read to the permissions of the gates job"
        );
        let github_in = format!(
            "{path} lacks the permissions that the github tracker needs; add contents: read and issues: read to the permissions of the gates job"
        );
        let write_all = format!(
            "{path} grants write-all; replace permissions: write-all with permissions: {{contents: read}} in the gates job"
        );
        let github_write_all = format!(
            "{path} grants write-all; replace permissions: write-all with permissions: {{contents: read, issues: read}} in the gates job"
        );
        let workflow = |permissions: &str| {
            format!("jobs:\n  gates:\n    runs-on: ubuntu-latest\n{permissions}    steps: []\n")
        };
        let other_scope = workflow("    permissions:\n      pull-requests: read\n");
        let flow_other_scope = workflow("    permissions: {pull-requests: read}\n");
        let empty_flow = workflow("    permissions: {}\n");
        let read_all = workflow("    permissions: read-all\n");
        let quoted_read_all = workflow("    permissions: 'read-all'\n");
        let read_all_with_comment = workflow("    permissions: read-all # every read scope\n");
        let write_all_text = workflow("    permissions: write-all\n");
        let quoted_write_all = workflow("    permissions: \"write-all\"\n");
        let write_all_with_comment = workflow("    permissions: write-all # for now\n");
        let contents = workflow("    permissions:\n      contents: read\n");
        let flow_contents = workflow("    permissions: {contents: read}\n");
        let issues_only = workflow("    permissions:\n      issues: read\n");
        let both = workflow("    permissions:\n      contents: read\n      issues: read\n");
        let write_all_with_markers =
            workflow("    permissions: write-all\n    # contents: read\n    # issues: read\n");
        let no_key = workflow("");
        let comment_key = workflow("    # permissions: write-all\n");
        let other_job = format!("{no_key}  docs:\n    permissions: write-all\n");
        let key_before = "jobs:\n  docs:\n    permissions: write-all\n  gates:\n    permissions:\n      contents: read\n      issues: read\n".to_string();
        let workflow_level = format!("permissions: read-all\n{no_key}");
        let after_comment =
            workflow("    permissions: # the token\n      # the reads\n      contents: read\n");
        let tracker_lines = |adapter: Option<Adapter>| -> String {
            let mut lines = format!("{HEAD_CHECKOUT_INPUT}\n");
            match adapter {
                Some(Adapter::Github) => lines.push_str("GH_TOKEN: ${{ github.token }}\n"),
                Some(Adapter::Beads) => lines.push_str("run: bd bootstrap --yes\n"),
                _ => {}
            }
            if adapter != Some(Adapter::Beads) {
                lines.push_str(&format!("{PERSIST_CREDENTIALS_MARKER}\n"));
            }
            lines
        };
        // (workflow text, steps of an adapter other than GitHub, steps of the GitHub tracker)
        let rows: Vec<(&str, &String, Vec<&String>, Vec<&String>)> = vec![
            ("no key", &no_key, vec![&block], vec![&github_block]),
            (
                "a key in a comment",
                &comment_key,
                vec![&block],
                vec![&github_block],
            ),
            (
                "a key of another job",
                &other_job,
                vec![&block],
                vec![&github_block],
            ),
            (
                "a key at the workflow level",
                &workflow_level,
                vec![&block],
                vec![&github_block],
            ),
            ("read-all", &read_all, vec![], vec![]),
            ("a quoted read-all", &quoted_read_all, vec![], vec![]),
            (
                "read-all with a comment",
                &read_all_with_comment,
                vec![],
                vec![],
            ),
            (
                "write-all",
                &write_all_text,
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "a quoted write-all",
                &quoted_write_all,
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "write-all with a comment",
                &write_all_with_comment,
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "write-all beside the markers in comments",
                &write_all_with_markers,
                vec![&write_all],
                vec![&github_write_all],
            ),
            (
                "a mapping of another scope",
                &other_scope,
                vec![&contents_in],
                vec![&github_in],
            ),
            (
                "a flow mapping of another scope",
                &flow_other_scope,
                vec![&contents_in],
                vec![&github_in],
            ),
            (
                "an empty flow mapping",
                &empty_flow,
                vec![&contents_in],
                vec![&github_in],
            ),
            ("contents: read only", &contents, vec![], vec![&issues]),
            (
                "a flow mapping of contents: read",
                &flow_contents,
                vec![],
                vec![&issues],
            ),
            (
                "issues: read only",
                &issues_only,
                vec![&contents_in],
                vec![&contents_in],
            ),
            ("both scopes", &both, vec![], vec![]),
            (
                "the key of another job before the gates job",
                &key_before,
                vec![],
                vec![],
            ),
            (
                "a comment between the key and its scopes",
                &after_comment,
                vec![],
                vec![&issues],
            ),
        ];

        for (label, text, others, github) in rows {
            for (adapter, expected) in [
                (None, &others),
                (Some(Adapter::Json), &others),
                (Some(Adapter::Beads), &others),
                (Some(Adapter::Github), &github),
            ] {
                let copy = format!("{text}{}", tracker_lines(adapter));
                let expected: Vec<String> = expected.iter().map(|step| (*step).clone()).collect();

                assert_eq!(
                    hand_step_for_kept(adapter, path, copy.as_bytes()),
                    expected,
                    "{label}, {adapter:?}\n{copy}"
                );
            }
        }
        let github_without_env =
            format!("{read_all}{HEAD_CHECKOUT_INPUT}\n{PERSIST_CREDENTIALS_MARKER}\n");
        let github_env = format!(
            "{path} lacks the GH_TOKEN env that the github tracker needs; add env: {{GH_TOKEN: ${{{{ github.token }}}}}} to each step that runs houserules check-backlog or houserules audit"
        );
        assert_eq!(
            hand_step_for_kept(Some(Adapter::Github), path, github_without_env.as_bytes()),
            [github_env],
            "read-all leaves the env step of the GitHub tracker alone"
        );
    }
}
