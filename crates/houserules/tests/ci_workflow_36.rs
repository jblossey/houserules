//! The read-only token of the seeded CI workflow (spec 8.5; case 9 and the
//! workflow half of case 10). `init` seeds no workflow now (spec 10.7), so each
//! case starts from the workflow that an earlier release seeded
//! (`seed_workflow`). Every form of the workflow, at the git top level
//! and in the subdirectory form, runs its gates job with `contents: read`. The
//! json, GitHub, and no-tracker forms also drop the persisted credential of the
//! checkout; the Beads form keeps it, because `bd bootstrap` fetches from the
//! remote after the checkout (spec 8.5, C11). `update` rewrites an unmodified
//! copy, and it gives a kept copy one hand step for each marker the copy lacks.
//! A kept copy that moves to another tracker keeps the lines of its first form;
//! the hand steps of the move, followed as printed, give a workflow that
//! `actionlint` passes.
//!
//! `actionlint` comes from `mise.toml` (`mise install`); a test fails, and does
//! not skip, when it is missing. A Beads install here is a marker that names
//! `beads`, because `update` writes the workflow whether or not `bd` can be
//! reached; `HOUSERULES_BD` names a program that does not exist, so that no real
//! `bd` runs.

mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::json;
use tempfile::TempDir;
use tracker_support::{
    assert_actionlint_passes, commit_all, count_lines, edit_marker, git, houserules, read_json,
    repo_root, run_ok, scratch_git_repo, seed_workflow, sha256_hex, stderr, stdout,
};

/// The workflow of an install at the git top level.
const WORKFLOW: &str = ".github/workflows/knowledge.yml";

/// The workflow of an install in the folder `sub`.
const SUB_WORKFLOW: &str = ".github/workflows/knowledge-sub.yml";

/// The unreachable line that `update` prints for a Beads install when `bd` is
/// not installed (spec 3.4, C38).
const BD_MISSING: &str = "tracker beads cannot be reached: bd is not installed; install bd";

/// The permissions block of the json, Beads, and no-tracker forms.
const PERMISSIONS: &str = "    permissions:\n      contents: read\n";

/// The permissions block of the GitHub form: the same block with `issues: read`.
const GITHUB_PERMISSIONS: &str = "    permissions:\n      contents: read\n      issues: read\n";

/// The scalar that grants every scope.
const WRITE_ALL_LINE: &str = "    permissions: write-all\n";

/// The env of a step of the GitHub form that runs a gate.
const GITHUB_ENV: &str = "        env:\n          GH_TOKEN: ${{ github.token }}\n";

/// The input of the checkout step that drops the persisted credential.
const PERSIST_LINE: &str = "          persist-credentials: false\n";

/// The input of the checkout step that makes every step judge the PR head.
const REF_LINE: &str = "          ref: ${{ github.event.pull_request.head.sha }}\n";

/// The tracker of an install.
#[derive(Clone, Copy, Debug)]
enum Tracker {
    None,
    Json,
    Github,
    Beads,
}

/// Every tracker, and no tracker.
const TRACKERS: [Tracker; 4] = [
    Tracker::None,
    Tracker::Json,
    Tracker::Github,
    Tracker::Beads,
];

impl Tracker {
    /// Only the Beads form keeps the persisted read-only credential, because
    /// `bd bootstrap --yes` fetches from the remote after the checkout (spec 8.5,
    /// C11, in docs/specs/2026-10-22-batch-36-evidence-and-report-claims.md).
    fn keeps_the_credential(self) -> bool {
        matches!(self, Tracker::Beads)
    }

    /// The `permissions` block of the gates job of the form.
    fn block(self) -> &'static str {
        match self {
            Tracker::Github => GITHUB_PERMISSIONS,
            _ => PERMISSIONS,
        }
    }

    /// The permissions of the gates job of the form, sorted.
    fn permissions(self) -> Vec<&'static str> {
        match self {
            Tracker::Github => vec!["contents: read", "issues: read"],
            _ => vec!["contents: read"],
        }
    }
}

/// Spec 8.5: the hand step for a kept copy that lacks `contents: read`.
fn contents_read_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks contents: read; add permissions: {{contents: read}} to the gates job"
    )
}

/// Spec 8.5: the hand step for a kept copy that lacks `persist-credentials: false`.
fn persist_credentials_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks persist-credentials: false; add it under with: of its actions/checkout step"
    )
}

/// The hand step of a kept copy that lacks the PR-head checkout (spec 3.2).
fn head_checkout_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks the PR-head checkout; add ref: ${{{{ github.event.pull_request.head.sha }}}} under with: of its actions/checkout step"
    )
}

/// Spec 8.5: the hand step of a kept GitHub-form copy that lacks both
/// `contents: read` and `issues: read`. It adds the whole block, which the copy
/// does not hold.
fn github_permissions_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks the permissions that the github tracker needs; add permissions: {{contents: read, issues: read}} to the gates job"
    )
}

/// Spec 8.5: the hand step of a kept GitHub-form copy that holds `contents: read`
/// and lacks `issues: read`. It adds the line to the block of the copy, so that
/// no copy is asked for a second `permissions` key.
fn issues_read_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks issues: read, which the github tracker needs; add issues: read to the permissions of the gates job"
    )
}

/// Spec 8.5 (design.md 5.226): the hand step of a kept copy of any form whose
/// gates job holds a `permissions` mapping that lacks `contents: read`. It adds
/// the line to the mapping.
fn contents_read_in_block_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks contents: read; add contents: read to the permissions of the gates job"
    )
}

/// Spec 8.5 (design.md 5.226): the hand step of a kept GitHub-form copy whose
/// gates job holds a `permissions` mapping that holds neither `contents: read`
/// nor `issues: read`. It adds both lines to the mapping.
fn github_scopes_in_block_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks the permissions that the github tracker needs; add contents: read and issues: read to the permissions of the gates job"
    )
}

/// Spec 8.5 (design.md 5.226): the hand step of a kept copy whose gates job holds
/// `permissions: write-all`. It replaces the scalar with the block of the form:
/// `{contents: read}`, and `{contents: read, issues: read}` for the GitHub form.
fn write_all_step(path: &str, github: bool) -> String {
    let scopes = if github {
        "contents: read, issues: read"
    } else {
        "contents: read"
    };
    format!(
        "hand step: {path} grants write-all; replace permissions: write-all with permissions: {{{scopes}}} in the gates job"
    )
}

/// Spec 8.5: the hand step of a kept GitHub-form copy that lacks the `GH_TOKEN`
/// env of its two gate steps.
fn github_env_step(path: &str) -> String {
    format!(
        "hand step: {path} lacks the GH_TOKEN env that the github tracker needs; add env: {{GH_TOKEN: ${{{{ github.token }}}}}} to each step that runs houserules check-backlog or houserules audit"
    )
}

/// Spec 8.5: the hand step of a kept copy that holds `persist-credentials: false`
/// and uses the Beads tracker.
fn beads_persist_step(path: &str) -> String {
    format!(
        "hand step: {path} holds persist-credentials: false, which the beads tracker cannot use; remove it from with: of its actions/checkout step, so that bd bootstrap can fetch from the remote"
    )
}

/// The hand step of a kept copy that lacks the steps of the Beads tracker
/// (spec 4.8). `bd_version` is the release of `bd` that the kit pins.
fn beads_steps_step(path: &str, bd_version: &str) -> String {
    format!(
        "hand step: {path} lacks the steps that the beads tracker needs; add a step that installs bd {bd_version} from its GitHub release, and a step that runs bd bootstrap --yes, before the step that runs houserules check-backlog"
    )
}

/// A scratch repository with a GitHub `origin`, where the GitHub tracker reads
/// its repository.
fn repository() -> TempDir {
    let dir = scratch_git_repo();
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
    );
    dir
}

/// Runs `update` on the install at `root` with a `bd` that does not exist.
fn update_output(root: &Path) -> Output {
    houserules()
        .args(["update", "--dir"])
        .arg(root)
        .env("HOUSERULES_BD", root.join("no-such-bd"))
        .output()
        .expect("run houserules update")
}

/// The stdout lines of `update` on the install at `root`, which must exit 0.
fn update_lines(root: &Path) -> Vec<String> {
    let output = update_output(root);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    stdout(&output).lines().map(str::to_string).collect()
}

/// The lines of `lines` that are hand steps.
fn hand_steps(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .filter(|line| line.starts_with("hand step: "))
        .cloned()
        .collect()
}

/// A scratch repository with one install.
struct Install {
    repo: TempDir,
    /// The install root: the git top level, or the folder `sub` below it.
    root: PathBuf,
    /// The workflow file below the git top level.
    workflow: &'static str,
}

impl Install {
    /// An install on `tracker` at the git top level, or in `sub/`, whose
    /// workflow an earlier release seeded. A Beads install is a plain `init`
    /// whose marker then names `beads`.
    fn new(tracker: Tracker, in_subdirectory: bool) -> Self {
        let repo = repository();
        let root = if in_subdirectory {
            let root = repo.path().join("sub");
            fs::create_dir(&root).expect("create sub/");
            root
        } else {
            repo.path().to_path_buf()
        };
        run_ok(&root, &["init"]);
        seed_workflow(repo.path(), &root);
        match tracker {
            Tracker::None => {}
            Tracker::Json => {
                run_ok(&root, &["init", "--tracker", "json"]);
            }
            Tracker::Github => {
                run_ok(&root, &["init", "--tracker", "github"]);
            }
            Tracker::Beads => {
                edit_marker(&root, |marker| {
                    marker.insert("tracker".to_string(), json!({"adapter": "beads"}));
                });
                let output = update_output(&root);
                assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
                assert_eq!(stderr(&output), format!("{BD_MISSING}\n"));
            }
        }
        let workflow = if in_subdirectory {
            SUB_WORKFLOW
        } else {
            WORKFLOW
        };
        Self {
            repo,
            root,
            workflow,
        }
    }

    /// The workflow file, built component by component.
    fn file(&self) -> PathBuf {
        self.workflow
            .split('/')
            .fold(self.repo.path().to_path_buf(), |path, part| path.join(part))
    }

    /// The text of the workflow file.
    fn text(&self) -> String {
        fs::read_to_string(self.file()).expect("read the workflow")
    }

    /// The baseline key of the workflow file in the marker.
    fn baseline_key(&self) -> String {
        format!(":/{}", self.workflow)
    }
}

/// The form of the workflow that the previous release wrote for `tracker`, made
/// from the current `form`: the json, Beads, and no-tracker forms had no
/// permissions block, the json, GitHub, and no-tracker forms persisted the
/// credential, and the GitHub form had the block with `issues: read` already.
fn previous_form(tracker: Tracker, form: &str) -> String {
    match tracker {
        Tracker::None | Tracker::Json => form.replace(PERMISSIONS, "").replace(PERSIST_LINE, ""),
        Tracker::Github => form.replace(PERSIST_LINE, ""),
        Tracker::Beads => form.replace(PERMISSIONS, ""),
    }
}

/// The `key: value` lines under the job-level `permissions:` key of `workflow`,
/// sorted and without comment lines, or the scalar after the key (`write-all`).
/// No such key gives no line.
fn permissions_of(workflow: &str) -> Vec<String> {
    const KEY: &str = "    permissions:";
    let mut lines = workflow.lines().skip_while(|line| !line.starts_with(KEY));
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let scalar = header[KEY.len()..].trim();
    if !scalar.is_empty() {
        return vec![scalar.to_string()];
    }
    let mut keys: Vec<String> = lines
        .take_while(|line| line.starts_with("      "))
        .map(str::trim)
        .filter(|line| !line.starts_with('#'))
        .map(str::to_string)
        .collect();
    keys.sort();
    keys
}

/// The inputs under `with:` of the checkout step of `workflow`, sorted.
fn checkout_inputs(workflow: &str) -> Vec<String> {
    let mut lines = workflow
        .lines()
        .skip_while(|line| !line.contains("uses: actions/checkout@"));
    lines.next();
    assert_eq!(
        lines.next(),
        Some("        with:"),
        "the checkout step opens its inputs"
    );
    let mut inputs: Vec<String> = lines
        .take_while(|line| line.starts_with("          "))
        .map(|line| line.trim().to_string())
        .collect();
    inputs.sort();
    inputs
}

/// Spec 8.5 and case 9: every form of the workflow, in the top-level and the
/// subdirectory placement, holds `contents: read` once in the one job-level
/// `permissions` block, with `issues: read` beside it in the GitHub form alone.
/// The json, GitHub, and no-tracker forms drop the persisted credential under
/// `with:` of the checkout; the Beads form holds no `persist-credentials` text.
/// `actionlint` passes on each file.
#[test]
fn every_form_runs_with_a_read_only_token_and_passes_actionlint() {
    for tracker in TRACKERS {
        for in_subdirectory in [false, true] {
            let install = Install::new(tracker, in_subdirectory);
            let workflow = install.text();
            let label = format!("{tracker:?}, in a subdirectory: {in_subdirectory}\n{workflow}");

            assert_eq!(workflow.matches("permissions").count(), 1, "{label}");
            assert_eq!(workflow.matches("contents: read").count(), 1, "{label}");
            assert_eq!(permissions_of(&workflow), tracker.permissions(), "{label}");
            let persisted = usize::from(!tracker.keeps_the_credential());
            assert_eq!(
                workflow.matches("persist-credentials").count(),
                persisted,
                "{label}"
            );
            assert_eq!(
                checkout_inputs(&workflow).contains(&"persist-credentials: false".to_string()),
                persisted == 1,
                "{label}"
            );
            assert_actionlint_passes(&install.file());
        }
    }
}

/// Pin of unchanged behavior: the forms keep the inputs of the checkout, the
/// token env of the GitHub gates, and the bootstrap step of Beads. The token
/// change adds a permission and drops an input; it adds nothing else.
#[test]
fn the_forms_keep_the_inputs_and_steps_they_had() {
    for tracker in TRACKERS {
        let install = Install::new(tracker, false);
        let workflow = install.text();

        let inputs: Vec<String> = checkout_inputs(&workflow)
            .into_iter()
            .filter(|input| !input.starts_with("persist-credentials"))
            .collect();
        assert_eq!(
            inputs,
            [
                "fetch-depth: 0",
                "ref: ${{ github.event.pull_request.head.sha }}"
            ],
            "{tracker:?}"
        );
        let token_steps = if matches!(tracker, Tracker::Github) {
            2
        } else {
            0
        };
        assert_eq!(
            workflow.matches(GITHUB_ENV).count(),
            token_steps,
            "{tracker:?}"
        );
        let bootstrap = usize::from(matches!(tracker, Tracker::Beads));
        assert_eq!(
            workflow.matches("run: bd bootstrap --yes\n").count(),
            bootstrap,
            "{tracker:?}"
        );
    }
}

/// The reader of the permissions block tells the read-only token from every
/// shape that it forbids (a write scope, a further key, a scalar, no block), and
/// takes the harmless edits (a comment, a reordered key) for what they are.
/// Without this table, the pin on what the reader returns could pass a forbidden
/// shape.
#[test]
fn the_permissions_reader_sees_every_forbidden_shape() {
    for (label, block, is_the_read_only_token) in [
        (
            "the token",
            "    permissions:\n      contents: read\n",
            true,
        ),
        (
            "a write scope",
            "    permissions:\n      contents: write\n",
            false,
        ),
        (
            "a further key",
            "    permissions:\n      contents: read\n      pull-requests: write\n",
            false,
        ),
        (
            "a further read key",
            "    permissions:\n      contents: read\n      issues: read\n",
            false,
        ),
        ("a scalar", "    permissions: write-all\n", false),
        ("no block", "    runs-on: ubuntu-latest\n", false),
        (
            "a comment",
            "    permissions:\n      # the token reads\n      contents: read\n",
            true,
        ),
    ] {
        let workflow =
            format!("jobs:\n  gates:\n    runs-on: ubuntu-latest\n{block}    steps: []\n");

        assert_eq!(
            permissions_of(&workflow) == ["contents: read"],
            is_the_read_only_token,
            "{label}: {:?}",
            permissions_of(&workflow)
        );
    }
    let reordered = "jobs:\n  gates:\n    permissions:\n      issues: read\n      contents: read\n";
    assert_eq!(
        permissions_of(reordered),
        ["contents: read", "issues: read"],
        "a reordered key"
    );
}

/// Spec 8.5: `docs/upgrading-to-2.md` shows the hand steps of the read-only
/// token as `update` prints them: the two steps for a missing marker, the four
/// steps of a kept GitHub-form copy, the three steps for a gates job that holds a
/// `permissions` key already (design.md 5.226), and the step for a kept Beads copy.
#[test]
fn the_upgrade_guide_shows_the_hand_steps_of_the_read_only_token() {
    let guide = fs::read_to_string(repo_root().join("docs").join("upgrading-to-2.md"))
        .expect("read the upgrade guide");

    for step in [
        contents_read_step("<path>"),
        persist_credentials_step("<path>"),
        github_permissions_step("<path>"),
        issues_read_step("<path>"),
        contents_read_in_block_step("<path>"),
        github_scopes_in_block_step("<path>"),
        write_all_step("<path>", false),
        write_all_step("<path>", true),
        github_env_step("<path>"),
        beads_persist_step("<path>"),
    ] {
        assert!(guide.contains(&step), "the upgrade guide lacks:\n{step}");
    }
}

/// One kept copy: `install` gets the workflow that `alter` makes from its
/// current form, plus a comment line of the project. Two runs of `update` keep
/// the file, print the kept line, and print the same hand steps. Returns the
/// hand steps.
fn kept_steps(install: &Install, alter: &dyn Fn(&str) -> String) -> Vec<String> {
    let mut changed = alter(&install.text());
    changed.push_str("# our own step\n");
    fs::write(install.file(), &changed).expect("change the workflow");
    commit_all(&install.root, "chore: our step");
    let kept = format!("kept {} (locally modified)", install.workflow);

    let mut first = Vec::new();
    for run_number in 1..=2 {
        let lines = update_lines(&install.root);
        assert!(lines.contains(&kept), "run {run_number}: {lines:?}");
        let steps = hand_steps(&lines);
        if run_number == 1 {
            first = steps;
        } else {
            assert_eq!(steps, first, "the second run prints the same steps");
        }
    }
    assert_eq!(install.text(), changed, "update keeps the copy");
    first
}

/// Spec 8.5 and case 9: a kept copy gets one hand step for each marker it lacks,
/// and none for a marker it holds. The order is the order of the file: the
/// tracker, `contents: read`, the PR-head checkout, `persist-credentials`. The
/// `contents: read` step of these rows is never printed for the GitHub tracker
/// (`a_kept_github_form_copy_gets_one_hand_step_for_each_marker_it_lacks` has its
/// steps) and the `persist-credentials` step never for Beads.
#[test]
fn a_kept_copy_gets_one_hand_step_for_each_marker_it_lacks() {
    type Alter = Box<dyn Fn(&str) -> String>;
    let w = WORKFLOW;
    let previous =
        |tracker: Tracker| -> Alter { Box::new(move |form| previous_form(tracker, form)) };
    let rows: Vec<(&str, Tracker, Alter, Vec<String>)> = vec![
        (
            "the previous no-tracker form",
            Tracker::None,
            previous(Tracker::None),
            vec![contents_read_step(w), persist_credentials_step(w)],
        ),
        (
            "the previous json form",
            Tracker::Json,
            previous(Tracker::Json),
            vec![contents_read_step(w), persist_credentials_step(w)],
        ),
        (
            "the previous GitHub form",
            Tracker::Github,
            previous(Tracker::Github),
            vec![persist_credentials_step(w)],
        ),
        (
            "the previous Beads form",
            Tracker::Beads,
            previous(Tracker::Beads),
            vec![contents_read_step(w)],
        ),
        (
            "a json copy that lacks the permissions only",
            Tracker::Json,
            Box::new(|form| form.replace(PERMISSIONS, "")),
            vec![contents_read_step(w)],
        ),
        (
            "a json copy that lacks the persist input only",
            Tracker::Json,
            Box::new(|form| form.replace(PERSIST_LINE, "")),
            vec![persist_credentials_step(w)],
        ),
        (
            "a json copy that holds both",
            Tracker::Json,
            Box::new(|form| form.to_string()),
            vec![],
        ),
        (
            "a json copy whose comment holds the permissions: a comment is no marker",
            Tracker::Json,
            Box::new(|form| format!("{}# contents: read\n", previous_form(Tracker::Json, form))),
            vec![contents_read_step(w), persist_credentials_step(w)],
        ),
        (
            "a json copy whose comment holds the persist input: a comment is no marker",
            Tracker::Json,
            Box::new(|form| {
                format!(
                    "{}# persist-credentials: false\n",
                    previous_form(Tracker::Json, form)
                )
            }),
            vec![contents_read_step(w), persist_credentials_step(w)],
        ),
        (
            "a json copy that lacks the permissions, the ref, and the persist input",
            Tracker::Json,
            Box::new(|form| previous_form(Tracker::Json, form).replace(REF_LINE, "")),
            vec![
                contents_read_step(w),
                head_checkout_step(w),
                persist_credentials_step(w),
            ],
        ),
    ];

    for (label, tracker, alter, expected) in rows {
        let install = Install::new(tracker, false);

        assert_eq!(kept_steps(&install, &*alter), expected, "{label}");
    }
}

/// One state of a kept GitHub-form copy: the label, the edit of the current form
/// that makes the state, and the hand steps of spec 8.5 that the state gets.
type GithubState = (&'static str, Box<dyn Fn(&str) -> String>, Vec<String>);

/// The states of a kept GitHub-form copy at `path`, one for each combination of
/// the markers of the tracker that the copy holds: `contents: read` and
/// `issues: read` in the permissions, and the `GH_TOKEN` env. A copy gets one
/// hand step for its permissions and one for its env, each only when it lacks a
/// marker of it (spec 8.5, design.md 5.223, HR-235).
fn github_states(path: &str) -> Vec<GithubState> {
    let drop_block = |form: &str| form.replace(GITHUB_PERMISSIONS, "");
    let drop_issues = |form: &str| form.replace("      issues: read\n", "");
    let drop_contents = |form: &str| form.replace("      contents: read\n", "");
    let drop_env = |form: &str| form.replace(GITHUB_ENV, "");
    vec![
        (
            "every marker held",
            Box::new(|form| form.to_string()),
            vec![],
        ),
        (
            "no permissions block",
            Box::new(drop_block),
            vec![github_permissions_step(path)],
        ),
        (
            "contents: read only",
            Box::new(drop_issues),
            vec![issues_read_step(path)],
        ),
        (
            "issues: read only",
            Box::new(drop_contents),
            vec![contents_read_in_block_step(path)],
        ),
        (
            "the env missing, both permissions held (HR-235)",
            Box::new(drop_env),
            vec![github_env_step(path)],
        ),
        (
            "no permissions block and the env missing",
            Box::new(move |form| drop_env(&drop_block(form))),
            vec![github_permissions_step(path), github_env_step(path)],
        ),
        (
            "contents: read only and the env missing",
            Box::new(move |form| drop_env(&drop_issues(form))),
            vec![issues_read_step(path), github_env_step(path)],
        ),
        (
            "issues: read only and the env missing",
            Box::new(move |form| drop_env(&drop_contents(form))),
            vec![contents_read_in_block_step(path), github_env_step(path)],
        ),
    ]
}

/// Spec 8.5 and case 9: a kept GitHub-form copy gets one hand step for its
/// permissions and one for its env, each only when it lacks a marker of it, and
/// none for a marker it holds. The four texts are the four lines of spec 8.5. No
/// state gets the `contents: read` step of the other trackers, and no state gets
/// the step of a block it holds.
#[test]
fn a_kept_github_form_copy_gets_one_hand_step_for_each_marker_it_lacks() {
    for in_subdirectory in [false, true] {
        let path = Install::new(Tracker::Github, in_subdirectory).workflow;
        for (label, alter, expected) in github_states(path) {
            let install = Install::new(Tracker::Github, in_subdirectory);

            assert_eq!(
                kept_steps(&install, &*alter),
                expected,
                "{label}, in a subdirectory: {in_subdirectory}"
            );
        }
    }
}

/// The branch review's reproduction, extended to each state of a kept GitHub-form
/// copy: each hand step that the copy gets, followed as printed, gives a workflow
/// that `actionlint` passes, with one `permissions` key that holds `contents: read`
/// and `issues: read`, and the `GH_TOKEN` env on the two gate steps. A further
/// `update` prints no hand step.
#[test]
fn the_hand_steps_of_a_kept_github_form_copy_give_a_valid_workflow() {
    let beads = (String::new(), String::new());
    for in_subdirectory in [false, true] {
        let path = Install::new(Tracker::Github, in_subdirectory).workflow;
        for (label, alter, expected) in github_states(path) {
            let install = Install::new(Tracker::Github, in_subdirectory);
            let label = format!("{label}, in a subdirectory: {in_subdirectory}");
            let mut changed = alter(&install.text());
            changed.push_str("# our own step\n");
            fs::write(install.file(), &changed).expect("change the workflow");
            commit_all(&install.root, "chore: our step");

            let printed = hand_steps(&update_lines(&install.root));

            assert_eq!(printed, expected, "{label}");
            let followed = printed
                .iter()
                .fold(changed, |text, step| follow(&text, step, path, &beads));
            fs::write(install.file(), &followed).expect("follow the hand steps");
            assert_actionlint_passes(&install.file());
            assert_eq!(
                followed.matches("permissions:").count(),
                1,
                "{label}\n{followed}"
            );
            assert_eq!(
                permissions_of(&followed),
                Tracker::Github.permissions(),
                "{label}\n{followed}"
            );
            assert_eq!(followed.matches(GITHUB_ENV).count(), 2, "{label}");
            commit_all(&install.root, "chore: follow the hand steps");
            let again = update_lines(&install.root);
            assert_eq!(
                hand_steps(&again),
                Vec::<String>::new(),
                "{label}: {again:?}"
            );
        }
    }
}

/// One state of the `permissions` key of the gates job of a kept copy that holds
/// a key already (spec 8.5, design.md 5.226).
#[derive(Clone, Copy, Debug)]
enum Key {
    /// `permissions: read-all`: the scalar holds every read scope.
    ReadAll,
    /// `permissions: write-all`.
    WriteAll,
    /// A block that holds one scope, `pull-requests: read`, and neither marker.
    OtherScope,
}

/// Every state of a `permissions` key.
const KEYS: [Key; 3] = [Key::ReadAll, Key::WriteAll, Key::OtherScope];

impl Key {
    /// The lines that replace the `permissions` block of the form.
    fn block(self) -> &'static str {
        match self {
            Key::ReadAll => "    permissions: read-all\n",
            Key::WriteAll => WRITE_ALL_LINE,
            Key::OtherScope => "    permissions:\n      pull-requests: read\n",
        }
    }

    /// The hand steps that spec 8.5 rules for the permissions of a kept copy of
    /// `tracker` at `path` that holds this key: `read-all` holds every read scope
    /// and gets none; `write-all` is replaced by the block of the form; a block
    /// that lacks the scopes gets them added to it.
    fn steps(self, tracker: Tracker, path: &str) -> Vec<String> {
        let github = matches!(tracker, Tracker::Github);
        match (self, github) {
            (Key::ReadAll, _) => vec![],
            (Key::WriteAll, github) => vec![write_all_step(path, github)],
            (Key::OtherScope, false) => vec![contents_read_in_block_step(path)],
            (Key::OtherScope, true) => vec![github_scopes_in_block_step(path)],
        }
    }

    /// The scopes of the key after the hand steps are followed, sorted.
    fn scopes_after(self, tracker: Tracker) -> Vec<&'static str> {
        match self {
            Key::ReadAll => vec!["read-all"],
            Key::WriteAll => tracker.permissions(),
            Key::OtherScope => {
                let mut scopes = tracker.permissions();
                scopes.push("pull-requests: read");
                scopes.sort_unstable();
                scopes
            }
        }
    }
}

/// A kept copy of `tracker` whose gates job holds `key` instead of the block of
/// the form, with the `GH_TOKEN` env dropped when `drop_env` is set: `update`
/// keeps the copy and prints the hand steps of spec 8.5, and the copy with those
/// steps followed as printed passes `actionlint`, holds one `permissions` key
/// with the scopes of `Key::scopes_after`, and gets no hand step from a further
/// `update`.
fn assert_the_hand_steps_follow_the_key(
    tracker: Tracker,
    key: Key,
    drop_env: bool,
    in_subdirectory: bool,
) {
    let install = Install::new(tracker, in_subdirectory);
    let path = install.workflow;
    let label = format!(
        "{tracker:?} with {key:?}, env dropped: {drop_env}, in a subdirectory: {in_subdirectory}"
    );
    let form = install.text();
    assert_eq!(form.matches(tracker.block()).count(), 1, "{label}\n{form}");
    let mut changed = form.replace(tracker.block(), key.block());
    if drop_env {
        changed = changed.replace(GITHUB_ENV, "");
    }
    changed.push_str("# our own step\n");
    fs::write(install.file(), &changed).expect("change the workflow");
    commit_all(&install.root, "chore: our step");
    let mut expected = key.steps(tracker, path);
    if drop_env {
        expected.push(github_env_step(path));
    }

    let printed = hand_steps(&update_lines(&install.root));

    assert_eq!(printed, expected, "{label}");
    assert_eq!(install.text(), changed, "{label}: update keeps the copy");
    let beads = (String::new(), String::new());
    let followed = printed
        .iter()
        .fold(changed, |text, step| follow(&text, step, path, &beads));
    fs::write(install.file(), &followed).expect("follow the hand steps");
    assert_actionlint_passes(&install.file());
    assert_eq!(
        followed.matches("permissions:").count(),
        1,
        "{label}\n{followed}"
    );
    assert_eq!(
        permissions_of(&followed),
        key.scopes_after(tracker),
        "{label}\n{followed}"
    );
    let envs = if matches!(tracker, Tracker::Github) {
        2
    } else {
        0
    };
    assert_eq!(followed.matches(GITHUB_ENV).count(), envs, "{label}");
    commit_all(&install.root, "chore: follow the hand steps");
    let again = update_lines(&install.root);
    assert_eq!(
        hand_steps(&again),
        Vec::<String>::new(),
        "{label}: {again:?}"
    );
}

/// The branch re-review's reproduction (a kept copy with `permissions: read-all`
/// or a block of another scope), extended to each form: the `permissions` key of
/// the gates job decides the form of the hand step for its permissions (spec 8.5,
/// design.md 5.226). `read-all` gets none, `write-all` gets the replacement of the
/// scalar, and a block of another scope gets the missing scopes added to it. The
/// Beads form gets the same steps as the JSON and no-tracker forms. Each copy,
/// with its steps followed as printed, passes `actionlint` with one `permissions`
/// key, and a further `update` prints none.
#[test]
fn the_hand_steps_of_a_kept_copy_follow_its_permissions_key() {
    for tracker in TRACKERS {
        for key in KEYS {
            for in_subdirectory in [false, true] {
                assert_the_hand_steps_follow_the_key(tracker, key, false, in_subdirectory);
            }
        }
    }
}

/// A key of the gates job leaves the env step of a kept GitHub-form copy alone:
/// `read-all` gets the env step only, and the other keys get their permissions
/// step first and the env step after it.
#[test]
fn a_permissions_key_leaves_the_env_step_of_a_kept_github_copy_alone() {
    for key in KEYS {
        for in_subdirectory in [false, true] {
            assert_the_hand_steps_follow_the_key(Tracker::Github, key, true, in_subdirectory);
        }
    }
}

/// Spec 8.5 and case 9: the hand steps of a copy in the subdirectory form name
/// the file of that form.
#[test]
fn a_kept_subdirectory_copy_names_its_own_file() {
    let install = Install::new(Tracker::Json, true);

    assert_eq!(
        kept_steps(&install, &|form| previous_form(Tracker::Json, form)),
        vec![
            contents_read_step(SUB_WORKFLOW),
            persist_credentials_step(SUB_WORKFLOW)
        ]
    );
}

/// Case 10 (the workflow half): an unmodified copy of the previous form is
/// rewritten to the current form of its tracker and baselined, once; a second
/// `update` prints nothing about the file.
#[test]
fn update_rewrites_an_unmodified_copy_of_the_previous_form() {
    for tracker in TRACKERS {
        for in_subdirectory in [false, true] {
            let install = Install::new(tracker, in_subdirectory);
            let label = format!("{tracker:?}, in a subdirectory: {in_subdirectory}");
            let current = install.text();
            let previous = previous_form(tracker, &current);
            assert!(
                previous != current,
                "{label}: the previous form differs from the current form"
            );
            fs::write(install.file(), &previous).expect("write the previous form");
            edit_marker(&install.root, |marker| {
                marker["baselines"][install.baseline_key()] =
                    json!(sha256_hex(previous.as_bytes()));
            });
            commit_all(&install.root, "chore: previous release");

            let first = update_lines(&install.root);

            assert_eq!(
                count_lines(&first.join("\n"), &format!("wrote {}", install.workflow)),
                1,
                "{label}: {first:?}"
            );
            assert_eq!(install.text(), current, "{label}");
            assert_eq!(
                read_json(&install.root, ".houserules.json")["baselines"][install.baseline_key()],
                json!(sha256_hex(current.as_bytes())),
                "{label}"
            );
            commit_all(&install.root, "chore: update");
            let second = update_lines(&install.root);
            assert!(
                !second.iter().any(|line| line.contains(install.workflow)),
                "{label}: {second:?}"
            );
        }
    }
}

/// The edit that the hand step `step` asks for, made to `workflow`. `beads` is
/// the pair of steps that the Beads form holds, with their final newline, and the
/// release of `bd` they install. A step that this function cannot follow fails
/// the test: every printed step is followed or named.
fn follow(workflow: &str, step: &str, path: &str, beads: &(String, String)) -> String {
    let after = |text: &str, anchor: &str, addition: &str| -> String {
        assert_eq!(text.matches(anchor).count(), 1, "{anchor:?} in:\n{text}");
        text.replace(anchor, &format!("{anchor}{addition}"))
    };
    let before = |text: &str, anchor: &str, addition: &str| -> String {
        assert_eq!(text.matches(anchor).count(), 1, "{anchor:?} in:\n{text}");
        text.replace(anchor, &format!("{addition}{anchor}"))
    };
    let check_backlog = "      - run: houserules check-backlog\n";
    let audit_last = "          --head ${{ github.event.pull_request.head.sha }}\n";
    if step == github_permissions_step(path) {
        after(workflow, "    runs-on: ubuntu-latest\n", GITHUB_PERMISSIONS)
    } else if step == issues_read_step(path) {
        after(workflow, "      contents: read\n", "      issues: read\n")
    } else if step == contents_read_in_block_step(path) {
        after(workflow, "    permissions:\n", "      contents: read\n")
    } else if step == github_scopes_in_block_step(path) {
        after(
            workflow,
            "    permissions:\n",
            "      contents: read\n      issues: read\n",
        )
    } else if step == write_all_step(path, false) || step == write_all_step(path, true) {
        assert_eq!(workflow.matches(WRITE_ALL_LINE).count(), 1, "{workflow}");
        let block = if step == write_all_step(path, true) {
            GITHUB_PERMISSIONS
        } else {
            PERMISSIONS
        };
        workflow.replace(WRITE_ALL_LINE, block)
    } else if step == github_env_step(path) {
        let text = after(workflow, check_backlog, GITHUB_ENV);
        after(&text, audit_last, GITHUB_ENV)
    } else if step == contents_read_step(path) {
        after(workflow, "    runs-on: ubuntu-latest\n", PERMISSIONS)
    } else if step == persist_credentials_step(path) {
        after(workflow, "          fetch-depth: 0\n", PERSIST_LINE)
    } else if step == beads_persist_step(path) {
        assert_eq!(workflow.matches(PERSIST_LINE).count(), 1, "{workflow}");
        workflow.replace(PERSIST_LINE, "")
    } else if step == beads_steps_step(path, &beads.1) {
        before(workflow, check_backlog, &beads.0)
    } else {
        panic!("a hand step that the test cannot follow: {step}");
    }
}

/// The two steps of the Beads form that go before the step that runs
/// `check-backlog`, with their final newline, and the release of `bd` they
/// install.
fn beads_steps_of(beads_form: &str) -> (String, String) {
    let start = beads_form
        .find("      - name: Install bd\n")
        .expect("the Beads form installs bd");
    let end = beads_form
        .find("      - run: houserules check-backlog\n")
        .expect("the Beads form runs check-backlog");
    let steps = beads_form[start..end].to_string();
    let version = steps
        .split("archive=beads_")
        .nth(1)
        .and_then(|rest| rest.split("_linux_amd64").next())
        .expect("the install step names the release")
        .to_string();
    (steps, version)
}

/// Spec 8.5 and case 9: a kept copy that moves to another tracker keeps the
/// lines that the template gave its first form. For each of `moves`, in the
/// top-level and the subdirectory placement, each hand step that the copy gets,
/// followed as printed, gives a workflow that `actionlint` passes and that the
/// tracker can run, and a further `update` prints no hand step. The job holds one
/// `permissions` key: the block of the first form, with `issues: read` added for
/// the GitHub tracker.
fn assert_the_hand_steps_of_each_move_give_a_valid_workflow(moves: &[(Tracker, Tracker)]) {
    let beads = beads_steps_of(&Install::new(Tracker::Beads, false).text());
    for in_subdirectory in [false, true] {
        for &(from, to) in moves {
            let install = Install::new(from, in_subdirectory);
            let path = install.workflow;
            let label = format!("{from:?} to {to:?}, in a subdirectory: {in_subdirectory}");
            let mut changed = install.text();
            changed.push_str("# our own step\n");
            fs::write(install.file(), &changed).expect("change the workflow");
            commit_all(&install.root, "chore: our step");
            let expected = match (from, to) {
                (Tracker::Beads, Tracker::Github) => vec![
                    issues_read_step(path),
                    github_env_step(path),
                    persist_credentials_step(path),
                ],
                (_, Tracker::Github) => vec![issues_read_step(path), github_env_step(path)],
                _ => vec![beads_steps_step(path, &beads.1), beads_persist_step(path)],
            };

            let tracker = match to {
                Tracker::Github => json!({"adapter": "github", "repo": "acme/widgets"}),
                _ => json!({"adapter": "beads"}),
            };
            edit_marker(&install.root, |marker| {
                marker.insert("tracker".to_string(), tracker);
            });
            let printed = hand_steps(&update_lines(&install.root));

            assert_eq!(printed, expected, "{label}");
            assert_eq!(install.text(), changed, "{label}: update keeps the copy");
            let followed = printed
                .iter()
                .fold(changed, |text, step| follow(&text, step, path, &beads));
            fs::write(install.file(), &followed).expect("follow the hand steps");
            assert_actionlint_passes(&install.file());
            assert_eq!(
                followed.matches("permissions:").count(),
                1,
                "{label}\n{followed}"
            );
            let kept_permissions = match to {
                Tracker::Github => to.permissions(),
                _ => from.permissions(),
            };
            assert_eq!(
                permissions_of(&followed),
                kept_permissions,
                "{label}\n{followed}"
            );
            if matches!(to, Tracker::Github) {
                assert_eq!(followed.matches(GITHUB_ENV).count(), 2, "{label}");
                assert!(followed.contains(PERSIST_LINE), "{label}");
            } else {
                assert!(!followed.contains("persist-credentials"), "{label}");
                assert_eq!(
                    followed.matches("run: bd bootstrap --yes\n").count(),
                    1,
                    "{label}"
                );
            }
            commit_all(&install.root, "chore: follow the hand steps");
            let again = update_lines(&install.root);
            assert_eq!(
                hand_steps(&again),
                Vec::<String>::new(),
                "{label}: {again:?}"
            );
        }
    }
}

/// A kept copy of the no-tracker or JSON form holds `contents: read`, and a kept
/// copy of the Beads form holds it too. Its move to the GitHub tracker adds
/// `issues: read` to that block and never asks for a second `permissions` key.
#[test]
fn the_hand_steps_of_a_kept_copy_that_moves_to_github_give_a_valid_workflow() {
    assert_the_hand_steps_of_each_move_give_a_valid_workflow(&[
        (Tracker::None, Tracker::Github),
        (Tracker::Json, Tracker::Github),
        (Tracker::Beads, Tracker::Github),
    ]);
}

/// A kept copy of the no-tracker, JSON, or GitHub form holds
/// `persist-credentials: false`, which `bd bootstrap` cannot use (spec 8.5, C11).
/// Its move to the Beads tracker asks for the removal of the input.
#[test]
fn the_hand_steps_of_a_kept_copy_that_moves_to_beads_give_a_valid_workflow() {
    assert_the_hand_steps_of_each_move_give_a_valid_workflow(&[
        (Tracker::None, Tracker::Beads),
        (Tracker::Json, Tracker::Beads),
        (Tracker::Github, Tracker::Beads),
    ]);
}
