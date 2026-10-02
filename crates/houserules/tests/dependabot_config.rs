//! Pins the Dependabot wiring: `.github/dependabot.yml` groups the two
//! `github/codeql-action` steps into one pull request for version updates
//! and for security updates,
//! `.github/workflows/dependabot-auto-merge.yml` approves a minor or patch
//! update before it enables auto-merge, and
//! `.github/workflows/dependabot-rebase.yml` asks Dependabot to rebase one
//! behind pull request per push to `main`, and none whose checks failed.
//!
//! The `init` and `analyze` steps of `codeql.yml` must run one
//! `codeql-action` version: a pull request that moves one step alone fails
//! CodeQL on the version mismatch
//! (`houserules.codeql-action-steps-share-one-version`). An enabled
//! auto-merge waits for every ruleset requirement, the approving review
//! included (`houserules.auto-merge-waits-for-every-ruleset-rule`), so the
//! workflow approves first and enables auto-merge second. Both steps run
//! under the minor-or-patch condition, so a major update gets neither. The
//! same entry explains the rebase workflow: the `main` ruleset requires an
//! up-to-date branch, so a push to `main` leaves an open pull request
//! behind.
//!
//! This crate has no YAML dependency. The helpers of `workflow_reader` read
//! the workflow files by indentation, which is enough for the fixed shape
//! these files keep.
//! `approve_script_approves_only_a_pull_request_that_is_not_approved` and
//! `rebase_script_asks_for_one_rebase_of_the_lowest_numbered_behind_pull_request`
//! run the steps' own scripts against a stub `gh`, so the decision logic is
//! proven by its consumer's run and not by a string match. The shape pins
//! of the rebase workflow run on every platform; the script tests run on
//! Unix.

mod workflow_reader;

use std::fs;

#[cfg(unix)]
use stubbed_step::StubbedStep;
#[cfg(unix)]
use workflow_reader::step_script;
use workflow_reader::{
    indent, nested_under, position_of_step_running, repo_root, step_key, step_scalar,
    workflow_steps,
};

/// The condition both Dependabot steps carry: only a minor or patch update
/// reaches them.
const MINOR_OR_PATCH: &str = "steps.metadata.outputs.update-type == 'version-update:semver-minor' \
     || steps.metadata.outputs.update-type == 'version-update:semver-patch'";

/// The command that identifies the approve step.
const APPROVE_COMMAND: &str = "gh pr review --approve";

/// The command that identifies the merge step.
const MERGE_COMMAND: &str = "gh pr merge --auto --rebase";

/// The workflow path, relative to the repository root.
const WORKFLOW: &str = ".github/workflows/dependabot-auto-merge.yml";

/// The rebase workflow path, relative to the repository root.
const REBASE_WORKFLOW: &str = ".github/workflows/dependabot-rebase.yml";

/// The JSON fields the rebase step reads from each open pull request: the
/// number, the head commit id, the auto-merge request, and the author.
const LIST_FIELDS: &str = "number,headRefOid,autoMergeRequest,author";

/// The JSON field the rebase step reads from the checks of a behind pull
/// request: the bucket, which `gh` sets to `pass`, `fail`, `pending`,
/// `skipping`, or `cancel`.
const CHECK_FIELDS: &str = "bucket";

/// The text of one tracked file, named relative to the repository root.
fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_root().join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The lines of the `updates:` entry for one package ecosystem in
/// `.github/dependabot.yml`.
fn update_entry<'a>(raw: &'a str, ecosystem: &str) -> Vec<&'a str> {
    let lines: Vec<&str> = raw.lines().collect();
    nested_under(&lines, &format!("- package-ecosystem: {ecosystem}"))
}

/// The approve step and the merge step, each named for failure messages.
fn approve_and_merge_steps(raw: &str) -> [(&'static str, Vec<&str>); 2] {
    let steps = workflow_steps(raw);
    let approve = position_of_step_running(&steps, APPROVE_COMMAND);
    let merge = position_of_step_running(&steps, MERGE_COMMAND);
    [
        ("approve", steps[approve].clone()),
        ("merge", steps[merge].clone()),
    ]
}

/// The workflow text without its comment lines. A comment line at any
/// indent can end an indentation block early and hide the lines after it, so
/// every pin of the rebase workflow and the extraction of its script read this
/// text. A comment that follows a value on the same line stays in the text and
/// changes the line that a pin compares.
fn without_comment_lines(raw: &str) -> String {
    raw.lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The trimmed lines nested below a top-level `key:` line of a workflow.
/// Fails when the workflow has no such line at indent zero.
fn top_level_block(raw: &str, key: &str) -> Result<Vec<String>, String> {
    let header = format!("{key}:");
    let text = without_comment_lines(raw);
    let lines: Vec<&str> = text.lines().collect();
    let start = lines
        .iter()
        .position(|line| *line == header)
        .ok_or_else(|| format!("the workflow has no top-level {header:?} line"))?;
    Ok(nested_under(&lines[start..], &header)
        .into_iter()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// The lines of `block` at the block's own indent, trimmed: the keys of a
/// mapping.
fn keys_of<'a>(block: &[&'a str]) -> Vec<&'a str> {
    let key_indent = block
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| indent(line))
        .min()
        .unwrap_or(0);
    block
        .iter()
        .copied()
        .filter(|line| !line.trim().is_empty() && indent(line) == key_indent)
        .map(str::trim)
        .collect()
}

/// The keys of the one job of a workflow, sorted. Fails when the workflow
/// has more or fewer than one job.
fn only_job_keys(raw: &str) -> Result<Vec<String>, String> {
    let text = without_comment_lines(raw);
    let lines: Vec<&str> = text.lines().collect();
    let jobs = nested_under(&lines, "jobs:");
    let job_ids = keys_of(&jobs);
    if job_ids.len() != 1 {
        return Err(format!("the workflow has one job, not {job_ids:?}"));
    }
    let mut keys: Vec<String> = keys_of(&nested_under(&jobs, job_ids[0]))
        .into_iter()
        .map(str::to_string)
        .collect();
    keys.sort_unstable();
    Ok(keys)
}

/// The key names of one step, sorted: the first line's key after its `- `
/// marker, then every line at the step's key indent.
fn step_key_names(step: &[&str]) -> Vec<String> {
    let key_indent = indent(step[0]) + 2;
    let mut names: Vec<String> = step
        .iter()
        .enumerate()
        .filter_map(|(position, line)| {
            let key_line = if position == 0 {
                line.trim_start().strip_prefix("- ")?
            } else if indent(line) == key_indent {
                line.trim_start()
            } else {
                return None;
            };
            key_line.split(':').next().map(str::to_string)
        })
        .collect();
    names.sort_unstable();
    names
}

/// The `applies-to` value and the patterns of one group of the
/// `github-actions` entry in `.github/dependabot.yml`, as
/// `(applies-to, patterns)`. A missing group has no `applies-to` and no
/// patterns.
fn github_actions_group(group_name: &str) -> (Option<String>, Vec<String>) {
    let raw = read_repo_file(".github/dependabot.yml");
    let entry = update_entry(&raw, "github-actions");
    let groups = nested_under(&entry, "groups:");
    let group = nested_under(&groups, &format!("{group_name}:"));
    let group_indent = group
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| indent(line))
        .min();
    let applies_to = group
        .iter()
        .filter(|line| Some(indent(line)) == group_indent)
        .find_map(|line| line.trim().strip_prefix("applies-to:"))
        .map(|value| value.trim().to_string());
    let patterns = nested_under(&group, "patterns:")
        .into_iter()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.strip_prefix("- ").unwrap_or(line).to_string())
        .collect();
    (applies_to, patterns)
}

/// The `github-actions` entry carries the group `codeql-action` for version
/// updates, with the one pattern `github/codeql-action*`. A group with no
/// `applies-to` covers version updates only
/// (`applies-to` in the Dependabot options reference,
/// <https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference>).
/// `WildcardMatcher.match?` in dependabot-core's
/// `common/lib/wildcard_matcher.rb` turns each `*` of a pattern into `.*`,
/// which matches `/` too, so the pattern holds `github/codeql-action/init`
/// and `github/codeql-action/analyze` in one pull request.
#[test]
fn github_actions_entry_groups_the_codeql_action_steps() {
    let (applies_to, patterns) = github_actions_group("codeql-action");
    assert_eq!(
        patterns,
        ["github/codeql-action*"],
        "the github-actions entry needs groups > codeql-action > patterns"
    );
    assert!(
        matches!(applies_to.as_deref(), None | Some("version-updates")),
        "group codeql-action covers version updates; applies-to is {applies_to:?}"
    );
}

/// The `github-actions` entry carries a second group, `codeql-action-security`,
/// with `applies-to: security-updates` and the same pattern. A security update
/// takes only the groups that apply to security updates:
/// `DependencyGroupEngine.from_job_config` in dependabot-core's
/// `updater/lib/dependabot/dependency_group_engine.rb` keeps a group only when
/// its `applies_to` matches the job type. So the `codeql-action` group does
/// not move the two steps together in a security update, and this group does.
#[test]
fn github_actions_entry_groups_the_codeql_action_security_updates() {
    let (applies_to, patterns) = github_actions_group("codeql-action-security");
    assert_eq!(
        applies_to.as_deref(),
        Some("security-updates"),
        "group codeql-action-security needs applies-to: security-updates"
    );
    assert_eq!(
        patterns,
        ["github/codeql-action*"],
        "the github-actions entry needs groups > codeql-action-security > patterns"
    );
}

/// The approve step comes before the merge step: the workflow approves,
/// then enables auto-merge.
#[test]
fn approve_step_runs_before_the_merge_step() {
    let raw = read_repo_file(WORKFLOW);
    let steps = workflow_steps(&raw);
    let approve = position_of_step_running(&steps, APPROVE_COMMAND);
    let merge = position_of_step_running(&steps, MERGE_COMMAND);
    assert!(
        approve < merge,
        "step {approve} approves and step {merge} merges; the approval has to come first"
    );
}

/// Both steps carry the minor-or-patch condition, word for word, so a
/// major update gets neither the approval nor the auto-merge.
#[test]
fn approve_and_merge_steps_share_the_minor_or_patch_condition() {
    let raw = read_repo_file(WORKFLOW);
    for (name, step) in approve_and_merge_steps(&raw) {
        assert_eq!(
            step_scalar(&step, "if").as_deref(),
            Some(MINOR_OR_PATCH),
            "the {name} step needs the minor-or-patch condition"
        );
    }
}

/// Both steps read the pull request URL and authenticate with the owner's
/// token, the Dependabot secret `DEPENDABOT_AUTOMERGE_TOKEN`.
#[test]
fn approve_and_merge_steps_authenticate_with_the_owner_token() {
    let raw = read_repo_file(WORKFLOW);
    for (name, step) in approve_and_merge_steps(&raw) {
        let (_, env) =
            step_key(&step, "env").unwrap_or_else(|| panic!("the {name} step has no env:"));
        let env: Vec<&str> = env.iter().map(|line| line.trim()).collect();
        for expected in [
            "PR_URL: ${{ github.event.pull_request.html_url }}",
            "GH_TOKEN: ${{ secrets.DEPENDABOT_AUTOMERGE_TOKEN }}",
        ] {
            assert!(
                env.contains(&expected),
                "the {name} step's env lacks {expected:?}: {env:?}"
            );
        }
    }
}

/// Both steps name `shell: bash`: an Actions `run` step with no `shell`
/// key runs without `pipefail`
/// (`houserules.actions-default-shell-lacks-pipefail`).
#[test]
fn approve_and_merge_steps_name_bash() {
    let raw = read_repo_file(WORKFLOW);
    for (name, step) in approve_and_merge_steps(&raw) {
        assert_eq!(
            step_scalar(&step, "shell").as_deref(),
            Some("bash"),
            "the {name} step needs shell: bash"
        );
    }
}

/// A step script and a stub `gh`, written into a scratch directory that
/// the value removes when it drops.
#[cfg(unix)]
mod stubbed_step {
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::process::{Command, Output};

    /// A step script beside a stub `gh` that comes first on `PATH`.
    pub struct StubbedStep {
        _scratch: tempfile::TempDir,
        script_path: PathBuf,
        log_path: PathBuf,
        path: OsString,
    }

    impl StubbedStep {
        /// Writes `script` and an executable `gh` with the body `stub_body`
        /// into a fresh scratch directory. The stub appends one line per
        /// call to the file that its `GH_LOG` variable names.
        pub fn new(script: &str, stub_body: &str) -> Self {
            let scratch = tempfile::TempDir::new().expect("create scratch dir");
            let bin_dir = scratch.path().join("bin");
            fs::create_dir(&bin_dir).expect("create stub bin dir");
            let stub = bin_dir.join("gh");
            fs::write(&stub, stub_body).expect("write stub gh");
            fs::set_permissions(&stub, fs::Permissions::from_mode(0o755))
                .expect("make stub gh executable");
            let script_path = scratch.path().join("step.sh");
            fs::write(&script_path, script).expect("write the step script");
            let log_path = scratch.path().join("gh.log");
            let inherited_path = std::env::var_os("PATH").unwrap_or_default();
            let path = std::env::join_paths(
                std::iter::once(bin_dir).chain(std::env::split_paths(&inherited_path)),
            )
            .expect("join PATH");
            Self {
                _scratch: scratch,
                script_path,
                log_path,
                path,
            }
        }

        /// Runs the script under the shell Actions uses for `shell: bash`
        /// (`bash --noprofile --norc -eo pipefail {0}`, per
        /// `houserules.actions-default-shell-lacks-pipefail`) with `envs`
        /// set. Returns the process output and the lines the stub logged
        /// during this run.
        pub fn run(&self, envs: &[(&str, &str)]) -> (Output, Vec<String>) {
            let _ = fs::remove_file(&self.log_path);
            let output = Command::new("bash")
                .args(["--noprofile", "--norc", "-eo", "pipefail"])
                .arg(&self.script_path)
                .env("PATH", &self.path)
                .env("GH_LOG", &self.log_path)
                .envs(envs.iter().copied())
                .output()
                .expect("run the step script under bash");
            let calls = fs::read_to_string(&self.log_path)
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect();
            (output, calls)
        }
    }
}

/// Runs the approve step's own script against a stub `gh`. The script
/// reads the review decision, approves a pull request whose decision is not
/// `APPROVED`, and fails without an approval when the read fails.
#[cfg(unix)]
#[test]
fn approve_script_approves_only_a_pull_request_that_is_not_approved() {
    const PR_URL: &str = "https://github.com/jblossey/houserules/pull/99";
    const STUB_GH: &str = r#"#!/bin/sh
echo "$*" >> "$GH_LOG"
if [ "$1 $2" = "pr view" ]; then
  [ "$STUB_VIEW_EXIT" = 0 ] || exit "$STUB_VIEW_EXIT"
  echo "$STUB_DECISION"
fi
"#;

    let raw = read_repo_file(WORKFLOW);
    let steps = workflow_steps(&raw);
    let approve = position_of_step_running(&steps, APPROVE_COMMAND);
    let step = StubbedStep::new(&step_script(&steps[approve]), STUB_GH);

    let view = format!("pr view {PR_URL} --json reviewDecision --jq .reviewDecision");
    let approval = format!("pr review --approve {PR_URL}");
    let cases: [(&str, &str, bool, Vec<&str>); 5] = [
        ("APPROVED", "0", true, vec![&view]),
        ("REVIEW_REQUIRED", "0", true, vec![&view, &approval]),
        ("CHANGES_REQUESTED", "0", true, vec![&view, &approval]),
        ("", "0", true, vec![&view, &approval]),
        ("APPROVED", "1", false, vec![&view]),
    ];
    for (decision, view_exit, succeeds, expected_calls) in cases {
        let (output, calls) = step.run(&[
            ("PR_URL", PR_URL),
            ("GH_TOKEN", "unused"),
            ("STUB_DECISION", decision),
            ("STUB_VIEW_EXIT", view_exit),
        ]);
        assert_eq!(
            output.status.success(),
            succeeds,
            "decision {decision:?}, view exit {view_exit}: {output:?}"
        );
        assert_eq!(
            calls, expected_calls,
            "decision {decision:?}, view exit {view_exit}"
        );
    }
}

/// One pin of the rebase workflow's shape: `Ok` for a workflow that keeps
/// it, otherwise the violation that names what differs.
type Pin = fn(&str) -> Result<(), String>;

/// The pinned lines of a top-level block, or the violation that names the
/// block.
fn check_block(raw: &str, key: &str, expected: &[&str]) -> Result<(), String> {
    let actual = top_level_block(raw, key)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!("the {key}: block is {actual:?}, not {expected:?}"))
    }
}

/// The workflow has these top-level keys and no other: a top-level `env:` or
/// `defaults:` block would reach the step without the step's own `env:` or
/// `shell:` naming it.
fn check_top_level_keys(raw: &str) -> Result<(), String> {
    let text = without_comment_lines(raw);
    let lines: Vec<&str> = text.lines().collect();
    let mut keys: Vec<&str> = keys_of(&lines)
        .into_iter()
        .filter_map(|line| line.split(':').next())
        .collect();
    keys.sort_unstable();
    let expected = ["concurrency", "jobs", "name", "on", "permissions"];
    if keys == expected {
        Ok(())
    } else {
        Err(format!("the top-level keys are {keys:?}, not {expected:?}"))
    }
}

/// The keys of a flat top-level mapping, whose order carries no meaning: the
/// lines of the block and `expected`, each sorted, or the violation that names
/// the block.
fn check_mapping(raw: &str, key: &str, expected: &[&str]) -> Result<(), String> {
    let mut actual = top_level_block(raw, key)?;
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    if actual == expected {
        Ok(())
    } else {
        Err(format!("the {key}: block is {actual:?}, not {expected:?}"))
    }
}

/// The workflow runs on a push to `main` and on nothing else: a second
/// trigger or a branch other than `main` changes the block.
fn check_trigger(raw: &str) -> Result<(), String> {
    check_block(raw, "on", &["push:", "branches:", "- main"])
}

/// The workflow token holds the four read scopes that the step's reads need
/// and nothing more: `contents` for the compare call, `pull-requests` for the
/// pull request list, and `checks` and `statuses` for the check state. The
/// comment runs with the owner's token, so the workflow token needs no write
/// scope.
fn check_permissions(raw: &str) -> Result<(), String> {
    check_mapping(
        raw,
        "permissions",
        &[
            "contents: read",
            "pull-requests: read",
            "checks: read",
            "statuses: read",
        ],
    )
}

/// The concurrency group does not cancel a run in progress: the block holds
/// `group: dependabot-rebase` and `cancel-in-progress: false`, in either
/// order, and no other key. With no `queue` key the default `single` applies:
/// a newer pending run replaces an older pending run of the group ("any
/// existing pending job or workflow in the same concurrency group will be
/// canceled and the new queued job or workflow will take its place", the
/// `concurrency` entry of
/// <https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax>).
/// Each run reads the live state of the pull requests, so a replaced run
/// loses no request.
fn check_concurrency(raw: &str) -> Result<(), String> {
    check_mapping(
        raw,
        "concurrency",
        &["group: dependabot-rebase", "cancel-in-progress: false"],
    )
}

/// The workflow has one job. It runs on `ubuntu-latest` and carries the
/// repository guard of the auto-merge workflow, so a fork runs nothing. The
/// job has these keys and no other: a `permissions`, `continue-on-error`,
/// or `timeout-minutes` key would change what the pinned lines promise.
fn check_job(raw: &str) -> Result<(), String> {
    let keys = only_job_keys(raw)?;
    let expected = [
        "if: github.repository == 'jblossey/houserules'",
        "runs-on: ubuntu-latest",
        "steps:",
    ];
    if keys == expected {
        Ok(())
    } else {
        Err(format!("the job keys are {keys:?}, not {expected:?}"))
    }
}

/// Whether `line` opens with a plain `uses` key: after the indent, an optional
/// dash and spaces (`-   uses: x`), then an optional opening brace of a flow
/// mapping (`- {uses: x}`). A quoted key (`"uses":`) and a `uses` key that
/// follows another key in a flow mapping are not read here. A key whose name
/// ends in `uses`, such as the permission `statuses:`, does not match.
fn opens_with_uses_key(line: &str) -> bool {
    let rest = line.trim_start();
    let rest = match rest.strip_prefix('-') {
        Some(after_dash) if after_dash.starts_with(' ') => after_dash.trim_start(),
        _ => rest,
    };
    let rest = rest.strip_prefix('{').unwrap_or(rest).trim_start();
    rest.starts_with("uses:")
}

/// No line of the workflow opens with a `uses` key in a spelling that
/// `opens_with_uses_key` reads. The input is the text without comment lines,
/// as for the other pins. This function proves that and no more. The invariant
/// that the workflow runs no action, no checkout, and no pin rests on three
/// pins together: `check_step` holds exactly one step; `check_step` holds the
/// key names of that step, read by `step_key_names`, to exactly `env`, `run`,
/// and `shell`, so the step has no `uses` key in any spelling; and this
/// function reads a `uses` key that opens a line. Another spelling of a `uses`
/// key, such as a quoted key or a key after the first in a flow mapping, is a
/// second step or an extra key of the one step, and the step count and the key
/// names in `check_step` turn it red.
fn check_no_action(raw: &str) -> Result<(), String> {
    let text = without_comment_lines(raw);
    let uses: Vec<&str> = text
        .lines()
        .filter(|line| opens_with_uses_key(line))
        .collect();
    if uses.is_empty() {
        Ok(())
    } else {
        Err(format!("uses: lines: {uses:?}"))
    }
}

/// The one step names `shell: bash`
/// (`houserules.actions-default-shell-lacks-pipefail`), gives `gh` the workflow
/// token as `GH_TOKEN` for its reads, holds the owner's token from the Actions
/// store as `REBASE_TOKEN` for the comment, names the repository for `gh`,
/// reads four JSON fields of each open pull request, and reads the bucket of
/// each check of a behind pull request.
fn check_step(raw: &str) -> Result<(), String> {
    let text = without_comment_lines(raw);
    let steps = workflow_steps(&text);
    if steps.len() != 1 {
        return Err(format!("{} steps, not one", steps.len()));
    }
    let step = &steps[0];
    let keys = step_key_names(step);
    if keys != ["env", "run", "shell"] {
        return Err(format!("the step keys are {keys:?}, not env, run, shell"));
    }
    if step_scalar(step, "shell").as_deref() != Some("bash") {
        return Err("the step needs shell: bash".to_string());
    }
    let (_, env) = step_key(step, "env").ok_or("the step has no env:")?;
    let mut env: Vec<&str> = env.iter().map(|line| line.trim()).collect();
    env.sort_unstable();
    let expected_env = [
        "GH_REPO: ${{ github.repository }}",
        "GH_TOKEN: ${{ github.token }}",
        "REBASE_TOKEN: ${{ secrets.DEPENDABOT_AUTOMERGE_TOKEN }}",
    ];
    if env != expected_env {
        return Err(format!("the env: block is {env:?}, not {expected_env:?}"));
    }
    let run = step_scalar(step, "run").ok_or("the step has no run:")?;
    for (call, fields) in [("list", LIST_FIELDS), ("checks", CHECK_FIELDS)] {
        if !run.contains(&format!("--json {fields} --jq ")) {
            return Err(format!(
                "the {call} call does not read exactly {fields}: {run}"
            ));
        }
    }
    Ok(())
}

/// Runs one pin against the real workflow and panics with its violation.
fn assert_pinned(pin: Pin) {
    pin(&read_repo_file(REBASE_WORKFLOW))
        .unwrap_or_else(|violation| panic!("{REBASE_WORKFLOW}: {violation}"));
}

#[test]
fn rebase_workflow_runs_on_a_push_to_main_and_nothing_else() {
    assert_pinned(check_trigger);
}

#[test]
fn rebase_workflow_holds_these_read_permissions_and_no_other() {
    assert_pinned(check_permissions);
}

#[test]
fn rebase_workflow_has_these_top_level_keys_and_no_other() {
    assert_pinned(check_top_level_keys);
}

#[test]
fn rebase_workflow_never_cancels_a_run_in_progress() {
    assert_pinned(check_concurrency);
}

#[test]
fn rebase_workflow_has_one_job_that_only_this_repository_runs() {
    assert_pinned(check_job);
}

#[test]
fn rebase_workflow_uses_no_action() {
    assert_pinned(check_no_action);
}

#[test]
fn rebase_step_names_bash_the_token_the_repository_and_the_json_fields() {
    assert_pinned(check_step);
}

/// One forbidden shape: its name, the workflow text that holds it, and the pin
/// that forbids it.
type ForbiddenShape = (&'static str, String, Pin);

/// Every forbidden shape of the rebase workflow, each an edit of `raw`. An
/// anchor is text that a reorder of keys keeps: a whole one-line key, a block
/// header, or the end of the file, never the line that opens the step, whose
/// text depends on which key comes first. An anchor must occur exactly once.
fn forbidden_shapes(raw: &str) -> Vec<ForbiddenShape> {
    let mutated = |anchor: &str, replacement: &str| {
        assert_eq!(
            raw.matches(anchor).count(),
            1,
            "{REBASE_WORKFLOW} must hold the anchor {anchor:?} exactly once"
        );
        raw.replacen(anchor, replacement, 1)
    };
    let after = |anchor: &str, addition: &str| mutated(anchor, &format!("{anchor}{addition}"));
    vec![
        (
            "a second trigger",
            after("      - main\n", "  workflow_dispatch:\n"),
            check_trigger,
        ),
        (
            "a second trigger after a column-zero comment",
            after("      - main\n", "# note\n  workflow_dispatch:\n"),
            check_trigger,
        ),
        (
            "a second branch",
            after("      - main\n", "      - develop\n"),
            check_trigger,
        ),
        (
            "a second permission after a column-zero comment",
            after("  contents: read\n", "# note\n  issues: write\n"),
            check_permissions,
        ),
        (
            "a write permission",
            mutated("  contents: read\n", "  contents: write\n"),
            check_permissions,
        ),
        (
            "an extra read permission",
            after("  contents: read\n", "  actions: read\n"),
            check_permissions,
        ),
        (
            "a missing read permission",
            mutated("  checks: read\n", ""),
            check_permissions,
        ),
        (
            "a top-level env block",
            mutated(
                "concurrency:\n",
                "env:\n  GH_HOST: example.com\nconcurrency:\n",
            ),
            check_top_level_keys,
        ),
        (
            "a top-level defaults block",
            mutated(
                "concurrency:\n",
                "defaults:\n  run:\n    shell: sh\nconcurrency:\n",
            ),
            check_top_level_keys,
        ),
        (
            "a job-level permissions block",
            after(
                "    runs-on: ubuntu-latest\n",
                "    permissions: write-all\n",
            ),
            check_job,
        ),
        (
            "a job-level continue-on-error",
            after(
                "    runs-on: ubuntu-latest\n",
                "    continue-on-error: true\n",
            ),
            check_job,
        ),
        (
            "a job key after an indented comment",
            after(
                "    runs-on: ubuntu-latest\n",
                "  # note\n    continue-on-error: true\n",
            ),
            check_job,
        ),
        (
            "no repository guard",
            mutated("    if: github.repository == 'jblossey/houserules'\n", ""),
            check_job,
        ),
        (
            "a second step after a comment",
            format!("{raw}    # note\n      - run: echo second\n"),
            check_step,
        ),
        (
            "a step-level continue-on-error",
            after("shell: bash\n", "        continue-on-error: true\n"),
            check_step,
        ),
        (
            "a token from another secret",
            mutated("secrets.DEPENDABOT_AUTOMERGE_TOKEN", "secrets.GITHUB_TOKEN"),
            check_step,
        ),
        (
            "the owner's secret mapped to GH_TOKEN",
            mutated(
                "GH_TOKEN: ${{ github.token }}",
                "GH_TOKEN: ${{ secrets.DEPENDABOT_AUTOMERGE_TOKEN }}",
            ),
            check_step,
        ),
        (
            "an action step",
            after(
                "    steps:\n",
                "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
            ),
            check_no_action,
        ),
        (
            "an action step with extra spaces after the dash",
            after(
                "    steps:\n",
                "      -   uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
            ),
            check_no_action,
        ),
        (
            "an action step written as a flow mapping",
            after(
                "    steps:\n",
                "      - {uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1}\n",
            ),
            check_no_action,
        ),
        (
            "a concurrency group that cancels",
            mutated("cancel-in-progress: false", "cancel-in-progress: true"),
            check_concurrency,
        ),
        (
            "a concurrency key beyond the group and the cancel setting",
            after("cancel-in-progress: false\n", "  queue: max\n"),
            check_concurrency,
        ),
    ]
}

/// `raw`, a workflow text without comment lines, with the keys of its
/// top-level block `header` in reverse order.
fn with_block_keys_reversed(raw: &str, header: &str) -> String {
    let mut lines: Vec<&str> = raw.lines().collect();
    let start = lines
        .iter()
        .position(|line| *line == header)
        .unwrap_or_else(|| panic!("the workflow has a {header} block"));
    let length = nested_under(&lines[start..], header).len();
    lines[start + 1..=start + length].reverse();
    format!("{}\n", lines.join("\n"))
}

/// `raw`, a workflow text without comment lines, with the first two keys of
/// its one step swapped, each with the lines nested under it.
fn with_step_keys_swapped(raw: &str) -> String {
    let lines: Vec<&str> = raw.lines().collect();
    let steps = workflow_steps(raw);
    let step = &steps[0];
    let start = lines
        .iter()
        .position(|line| *line == step[0])
        .expect("the step starts at one of the workflow's lines");
    let key_indent = indent(step[0]) + 2;
    let mut keys: Vec<Vec<String>> = Vec::new();
    for (position, line) in step.iter().enumerate() {
        if position == 0 || indent(line) == key_indent {
            keys.push(Vec::new());
        }
        let line = if position == 0 {
            let key = line
                .trim_start()
                .strip_prefix("- ")
                .expect("a step opens with a marker");
            format!("{}{key}", " ".repeat(key_indent))
        } else {
            (*line).to_string()
        };
        keys.last_mut().expect("a key group is open").push(line);
    }
    keys.swap(0, 1);
    keys[0][0] = format!(
        "{}- {}",
        " ".repeat(key_indent - 2),
        keys[0][0].trim_start()
    );
    let swapped: Vec<String> = lines[..start]
        .iter()
        .map(|line| (*line).to_string())
        .chain(keys.into_iter().flatten())
        .chain(
            lines[start + step.len()..]
                .iter()
                .map(|line| (*line).to_string()),
        )
        .collect();
    format!("{}\n", swapped.join("\n"))
}

/// The rebase workflow as written and with each reorder that changes nothing:
/// the keys of the `concurrency:` block and of the `permissions:` block
/// reversed, and two keys of the step swapped. The
/// reorders read and write the text without comment lines, so that a comment
/// line is never taken for a key. Fails when a reorder changes no text.
fn harmless_variants(raw: &str) -> Vec<(&'static str, String)> {
    let plain = format!("{}\n", without_comment_lines(raw));
    let variants = vec![
        ("as written", raw.to_string()),
        (
            "the concurrency keys swapped",
            with_block_keys_reversed(&plain, "concurrency:"),
        ),
        (
            "the permission keys reversed",
            with_block_keys_reversed(&plain, "permissions:"),
        ),
        ("the step keys swapped", with_step_keys_swapped(&plain)),
    ];
    for (name, text) in &variants[1..] {
        assert_ne!(*text, plain, "{name} changed nothing");
    }
    variants
}

/// Each shape that `forbidden_shapes` names turns the pin that forbids it red,
/// in every harmless variant of the workflow and whatever comment lines
/// surround the shape. The table names each class of shape: an extra trigger
/// or branch, an extra, missing, or write permission, an unlisted top-level
/// key, job key, step key, or concurrency key, a missing repository guard, an
/// extra step, an action, a token from another secret or in the wrong
/// variable, and a concurrency group that cancels.
#[test]
fn rebase_pins_reject_each_forbidden_shape() {
    let raw = read_repo_file(REBASE_WORKFLOW);
    for (variant, text) in harmless_variants(&raw) {
        let accepted: Vec<&str> = forbidden_shapes(&text)
            .iter()
            .filter(|(_, shape, pin)| pin(shape).is_ok())
            .map(|(name, _, _)| *name)
            .collect();
        assert!(
            accepted.is_empty(),
            "{variant}: shapes that pass the pin that forbids them: {accepted:?}"
        );
    }
}

/// Every pin of the rebase workflow, each named for failure messages.
const PINS: [(&str, Pin); 7] = [
    ("trigger", check_trigger),
    ("permissions", check_permissions),
    ("top-level keys", check_top_level_keys),
    ("concurrency", check_concurrency),
    ("job", check_job),
    ("no action", check_no_action),
    ("step", check_step),
];

/// The rebase workflow as written and with one comment line added in each
/// place where a comment can end an indentation block early: before the step,
/// between its keys, and between the two `concurrency:` keys.
fn commented_copies(raw: &str) -> Vec<(&'static str, String)> {
    let comment_after = |anchor: &str, comment: &str| {
        assert_eq!(raw.matches(anchor).count(), 1, "the anchor {anchor:?}");
        raw.replacen(anchor, &format!("{anchor}{comment}"), 1)
    };
    vec![
        ("as written", raw.to_string()),
        (
            "a column-zero comment before the step",
            comment_after("    steps:\n", "# note\n"),
        ),
        (
            "an indented comment before the step",
            comment_after("    steps:\n", "    # note\n"),
        ),
        (
            "a comment between the step keys",
            comment_after("shell: bash\n", "        # note\n"),
        ),
        (
            "a comment between the concurrency keys",
            comment_after("  group: dependabot-rebase\n", "  # note\n"),
        ),
    ]
}

/// A reorder that changes nothing about the workflow turns no pin red: a pin
/// fails only when the thing it names changes. A comment line anywhere changes
/// nothing either.
#[test]
fn rebase_pins_accept_a_reorder_of_keys() {
    let raw = read_repo_file(REBASE_WORKFLOW);
    for (copy, commented) in commented_copies(&raw) {
        for (variant, text) in harmless_variants(&commented) {
            for (name, pin) in PINS {
                pin(&text).unwrap_or_else(|violation| {
                    panic!("{copy}, {variant}, the {name} pin: {violation}")
                });
            }
        }
    }
}

/// The script of the one step of the rebase workflow, read from the text
/// without comment lines. The script loses its own full-line comments, which
/// change no behavior.
#[cfg(unix)]
fn rebase_step_script(raw: &str) -> String {
    let text = without_comment_lines(raw);
    let steps = workflow_steps(&text);
    step_script(&steps[0])
}

/// A comment line between `steps:` and the step, at column zero or inside the
/// block, leaves the extracted script as it was.
#[cfg(unix)]
#[test]
fn rebase_script_extraction_ignores_comment_lines() {
    let raw = read_repo_file(REBASE_WORKFLOW);
    let script = rebase_step_script(&raw);
    for comment in ["# note\n", "    # note\n"] {
        let commented = raw.replacen("    steps:\n", &format!("    steps:\n{comment}"), 1);
        assert_eq!(
            rebase_step_script(&commented),
            script,
            "a comment line {comment:?} between steps: and the step"
        );
    }
}

/// Runs the rebase step's own script against a stub `gh`. The script asks
/// Dependabot to rebase the lowest-numbered open pull request that Dependabot
/// authored, that has auto-merge on, that is behind `main`, and whose head has
/// no failed check, and no other. It reads the compare API only for a
/// Dependabot pull request with auto-merge, and the checks only for a behind
/// pull request. Pending, skipped, cancelled, and absent checks do not make a
/// pull request red. A red pull request gets one printed line and no comment.
/// The step fails, with no comment, when a read fails, when the compare API
/// returns no number, when the count of checks that it reads after a failed
/// check read is no number, or when `gh` reports a check bucket the step does
/// not know. A failed check read for a pull request that has checks, commit
/// statuses included, fails the step; for a pull request with no checks it
/// does not. It fails with a named error, with no `gh` call, when the owner's
/// token is empty. Every read carries the workflow token (`GH_TOKEN`), and the
/// comment call, which only the owner's token (`REBASE_TOKEN`) may make,
/// carries that token. Each case asserts the exit status, the exact `gh`
/// calls with the token of each, and the printed text.
#[cfg(unix)]
#[test]
fn rebase_script_asks_for_one_rebase_of_the_lowest_numbered_behind_pull_request() {
    /// The stub prints `STUB_LIST` for `pr list`. For `api` it prints the value
    /// that `STUB_COMPARE` holds for the compared head commit (`FAIL` makes the
    /// call fail, an unknown commit fails too). For `pr checks` and `pr view`
    /// it reads the line of `STUB_CHECKS` that names the pull request. `pr
    /// checks` prints the buckets of that line one per line; the words
    /// `ABSENT` (no checks), `READ_FAILS` (a failed read of a pull request that
    /// has checks), `STATUS_ONLY` (a failed read of a pull request whose two
    /// checks are commit statuses), `BOTH_FAIL`, and `COUNT_TEXT` make it exit
    /// 1, as `gh` does for a pull request with no checks and for a failed read.
    /// `pr view` answers the `--jq` it gets as `gh` does. The count form
    /// prints the number of entries of the check list: none for `ABSENT`, two
    /// for `STATUS_ONLY`, one per bucket for a bucket line, and the text `many`
    /// for `COUNT_TEXT`; `BOTH_FAIL` makes `pr view` exit 1. The names form
    /// prints an empty line for each commit status, which has no name, and
    /// `a-check` for every other entry. The stub logs every call with the
    /// `GH_TOKEN` it carried and its arguments, joined by tabs, so a split
    /// argument shows.
    const STUB_GH: &str = r#"#!/bin/sh
IFS=$(printf '\t')
printf '%s\t%s\n' "$GH_TOKEN" "$*" >> "$GH_LOG"
unset IFS
lookup() {
  while read -r name value; do
    if [ "$name" = "$1" ]; then
      printf '%s\n' "$value"
      return 0
    fi
  done <<TABLE
$2
TABLE
  return 1
}
case "$1 $2" in
"pr list")
  [ "$STUB_LIST_EXIT" = 0 ] || exit "$STUB_LIST_EXIT"
  [ -z "$STUB_LIST" ] || printf '%s\n' "$STUB_LIST"
  ;;
"api repos/"*)
  value=$(lookup "${2#*/compare/main...}" "$STUB_COMPARE") || exit 1
  [ "$value" != FAIL ] || exit 1
  printf '%s\n' "$value"
  ;;
"pr checks")
  value=$(lookup "$3" "$STUB_CHECKS") || exit 1
  case $value in ABSENT | READ_FAILS | STATUS_ONLY | BOTH_FAIL | COUNT_TEXT) exit 1 ;; esac
  for bucket in $value; do printf '%s\n' "$bucket"; done
  ;;
"pr view")
  value=$(lookup "$3" "$STUB_CHECKS") || exit 1
  entries=0
  case $value in
    BOTH_FAIL) exit 1 ;;
    ABSENT) ;;
    STATUS_ONLY) entries=2 ;;
    *) for word in $value; do entries=$((entries + 1)); done ;;
  esac
  case "$7" in
    '.statusCheckRollup | length')
      if [ "$value" = COUNT_TEXT ]; then echo many; else echo "$entries"; fi
      ;;
    '.statusCheckRollup[].name')
      if [ "$value" = STATUS_ONLY ]; then
        printf '\n\n'
      else
        count=0
        while [ "$count" -lt "$entries" ]; do echo a-check; count=$((count + 1)); done
      fi
      ;;
  esac
  ;;
esac
"#;
    const LIST_JQ: &str =
        r#".[] | "\(.number) \(.headRefOid) \(.autoMergeRequest != null) \(.author.login)""#;
    const REPOSITORY: &str = "jblossey/houserules";
    const WORKFLOW_TOKEN: &str = "workflow-token";
    const OWNER_TOKEN: &str = "owner-token";
    const DEPENDABOT: &str = "app/dependabot";
    const FIRST_HEAD: &str = "1111111111111111111111111111111111111111";
    const SECOND_HEAD: &str = "2222222222222222222222222222222222222222";
    const THIRD_HEAD: &str = "3333333333333333333333333333333333333333";
    const NONE_ASKED: &str =
        "No Dependabot pull request with auto-merge is behind main and free of failed checks.\n";
    const EMPTY_TOKEN: &str = "::error::The Actions secret DEPENDABOT_AUTOMERGE_TOKEN is empty or not set. Set it with: gh secret set DEPENDABOT_AUTOMERGE_TOKEN\n";

    struct Case {
        name: &'static str,
        owner_token: &'static str,
        list: String,
        list_exit: &'static str,
        compare: String,
        checks: String,
        succeeds: bool,
        calls: Vec<String>,
        stdout: String,
    }

    fn read_call(args: &[&str]) -> String {
        format!("{WORKFLOW_TOKEN}\t{}", args.join("\t"))
    }
    fn owner_call(args: &[&str]) -> String {
        format!("{OWNER_TOKEN}\t{}", args.join("\t"))
    }
    let list_call = read_call(&[
        "pr",
        "list",
        "--state",
        "open",
        "--limit",
        "100",
        "--json",
        LIST_FIELDS,
        "--jq",
        LIST_JQ,
    ]);
    let compare_call = |head: &str| {
        read_call(&[
            "api",
            &format!("repos/{REPOSITORY}/compare/main...{head}"),
            "--jq",
            ".behind_by",
        ])
    };
    let checks_call = |number: &str| {
        read_call(&[
            "pr",
            "checks",
            number,
            "--json",
            CHECK_FIELDS,
            "--jq",
            ".[].bucket",
        ])
    };
    let checklist_call = |number: &str| {
        read_call(&[
            "pr",
            "view",
            number,
            "--json",
            "statusCheckRollup",
            "--jq",
            ".statusCheckRollup | length",
        ])
    };
    let comment_call =
        |number: &str| owner_call(&["pr", "comment", number, "--body", "@dependabot rebase"]);
    let asked = |number: &str, behind: &str| {
        format!("Asked Dependabot to rebase pull request #{number}: {behind} behind main.\n")
    };
    let skipped = |number: &str| {
        format!("Skipped pull request #{number}: a check of its current head failed.\n")
    };
    let not_a_number = |number: &str, value: &str| {
        format!(
            "::error::The compare API returned '{value}' as behind_by for pull request #{number}. Expected a number.\n"
        )
    };
    let unreadable = |number: &str| {
        format!("::error::gh could not read the checks of pull request #{number}.\n")
    };
    let not_a_count = |number: &str, value: &str| {
        format!(
            "::error::gh returned '{value}' as the number of checks of pull request #{number}. Expected a number.\n"
        )
    };
    let unknown_bucket = |number: &str, bucket: &str| {
        format!(
            "::error::gh reported the check bucket '{bucket}' for pull request #{number}. Expected pass, fail, pending, skipping, or cancel.\n"
        )
    };
    let entry = |number: &str, head: &str, auto: &str, author: &str| {
        format!("{number} {head} {auto} {author}")
    };
    let dependabot = |number: &str, head: &str, auto: &str| entry(number, head, auto, DEPENDABOT);
    let case = |name,
                list: &[String],
                compare: &str,
                checks: &str,
                succeeds,
                calls: Vec<String>,
                stdout: String| Case {
        name,
        owner_token: OWNER_TOKEN,
        list: list.join("\n"),
        list_exit: "0",
        compare: compare.to_string(),
        checks: checks.to_string(),
        succeeds,
        calls,
        stdout,
    };

    let cases = vec![
        case(
            "no open pull request",
            &[],
            "",
            "",
            true,
            vec![list_call.clone()],
            NONE_ASKED.to_string(),
        ),
        case(
            "one Dependabot pull request with auto-merge, behind, checks passed or skipped",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 pass skipping",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                comment_call("48"),
            ],
            asked("48", "1"),
        ),
        case(
            "two such pull requests, listed in descending number",
            &[
                dependabot("10", SECOND_HEAD, "true"),
                dependabot("9", FIRST_HEAD, "true"),
            ],
            &format!("{FIRST_HEAD} 2\n{SECOND_HEAD} 1"),
            "9 pass\n10 pass",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("9"),
                comment_call("9"),
            ],
            asked("9", "2"),
        ),
        case(
            "a behind pull request without auto-merge, then one with it",
            &[
                dependabot("43", FIRST_HEAD, "false"),
                dependabot("48", SECOND_HEAD, "true"),
            ],
            &format!("{FIRST_HEAD} 3\n{SECOND_HEAD} 1"),
            "48 pass",
            true,
            vec![
                list_call.clone(),
                compare_call(SECOND_HEAD),
                checks_call("48"),
                comment_call("48"),
            ],
            asked("48", "1"),
        ),
        case(
            "a pull request of another author with auto-merge, behind, below a Dependabot pull request",
            &[
                entry("4", FIRST_HEAD, "true", "jblossey"),
                entry("5", SECOND_HEAD, "true", "app/renovate"),
                dependabot("9", THIRD_HEAD, "true"),
            ],
            &format!("{THIRD_HEAD} 2"),
            "9 pass",
            true,
            vec![
                list_call.clone(),
                compare_call(THIRD_HEAD),
                checks_call("9"),
                comment_call("9"),
            ],
            asked("9", "2"),
        ),
        case(
            "auto-merge on and up to date, then one with auto-merge, behind",
            &[
                dependabot("43", FIRST_HEAD, "true"),
                dependabot("48", SECOND_HEAD, "true"),
            ],
            &format!("{FIRST_HEAD} 0\n{SECOND_HEAD} 2"),
            "48 pass",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                compare_call(SECOND_HEAD),
                checks_call("48"),
                comment_call("48"),
            ],
            asked("48", "2"),
        ),
        case(
            "auto-merge on, up to date, alone, with a failed check that nothing reads",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 0"),
            "48 fail",
            true,
            vec![list_call.clone(), compare_call(FIRST_HEAD)],
            NONE_ASKED.to_string(),
        ),
        case(
            "behind without auto-merge, alone",
            &[dependabot("48", FIRST_HEAD, "false")],
            &format!("{FIRST_HEAD} 4"),
            "48 pass",
            true,
            vec![list_call.clone()],
            NONE_ASKED.to_string(),
        ),
        case(
            "a behind pull request with a failed check, then one with none",
            &[
                dependabot("43", FIRST_HEAD, "true"),
                dependabot("48", SECOND_HEAD, "true"),
            ],
            &format!("{FIRST_HEAD} 3\n{SECOND_HEAD} 1"),
            "43 pass fail skipping\n48 pass",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("43"),
                compare_call(SECOND_HEAD),
                checks_call("48"),
                comment_call("48"),
            ],
            format!("{}{}", skipped("43"), asked("48", "1")),
        ),
        case(
            "only a behind pull request with a failed check",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 fail",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
            ],
            format!("{}{NONE_ASKED}", skipped("48")),
        ),
        case(
            "a behind pull request whose checks are pending",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 pass pending",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                comment_call("48"),
            ],
            asked("48", "1"),
        ),
        case(
            "a behind pull request whose checks are skipped or cancelled",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 skipping cancel",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                comment_call("48"),
            ],
            asked("48", "1"),
        ),
        case(
            "a behind pull request with no checks",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 ABSENT",
            true,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                checklist_call("48"),
                comment_call("48"),
            ],
            asked("48", "1"),
        ),
        case(
            "the check read fails for a pull request that has checks",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 READ_FAILS",
            false,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                checklist_call("48"),
            ],
            unreadable("48"),
        ),
        case(
            "the check read fails for a pull request whose checks are commit statuses only",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 STATUS_ONLY",
            false,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                checklist_call("48"),
            ],
            unreadable("48"),
        ),
        case(
            "both check reads fail",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 BOTH_FAIL",
            false,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                checklist_call("48"),
            ],
            String::new(),
        ),
        case(
            "the check count is no number",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 COUNT_TEXT",
            false,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
                checklist_call("48"),
            ],
            not_a_count("48", "many"),
        ),
        case(
            "a check bucket that the step does not know",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} 1"),
            "48 pass mystery",
            false,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                checks_call("48"),
            ],
            unknown_bucket("48", "mystery"),
        ),
        Case {
            list_exit: "1",
            ..case(
                "the list call fails",
                &[],
                "",
                "",
                false,
                vec![list_call.clone()],
                String::new(),
            )
        },
        case(
            "a compare call fails after an earlier pull request read as up to date",
            &[
                dependabot("43", FIRST_HEAD, "true"),
                dependabot("48", SECOND_HEAD, "true"),
            ],
            &format!("{FIRST_HEAD} 0\n{SECOND_HEAD} FAIL"),
            "",
            false,
            vec![
                list_call.clone(),
                compare_call(FIRST_HEAD),
                compare_call(SECOND_HEAD),
            ],
            String::new(),
        ),
        case(
            "the compare value is null",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} null"),
            "",
            false,
            vec![list_call.clone(), compare_call(FIRST_HEAD)],
            not_a_number("48", "null"),
        ),
        case(
            "the compare value is empty",
            &[dependabot("48", FIRST_HEAD, "true")],
            FIRST_HEAD,
            "",
            false,
            vec![list_call.clone(), compare_call(FIRST_HEAD)],
            not_a_number("48", ""),
        ),
        case(
            "the compare value is text",
            &[dependabot("48", FIRST_HEAD, "true")],
            &format!("{FIRST_HEAD} Not Found"),
            "",
            false,
            vec![list_call.clone(), compare_call(FIRST_HEAD)],
            not_a_number("48", "Not Found"),
        ),
        Case {
            owner_token: "",
            ..case(
                "the owner's token is empty",
                &[dependabot("48", FIRST_HEAD, "true")],
                &format!("{FIRST_HEAD} 1"),
                "48 pass",
                false,
                vec![],
                EMPTY_TOKEN.to_string(),
            )
        },
    ];

    let step = StubbedStep::new(
        &rebase_step_script(&read_repo_file(REBASE_WORKFLOW)),
        STUB_GH,
    );
    for case in cases {
        let (output, calls) = step.run(&[
            ("GH_TOKEN", WORKFLOW_TOKEN),
            ("REBASE_TOKEN", case.owner_token),
            ("GH_REPO", REPOSITORY),
            ("STUB_LIST", &case.list),
            ("STUB_LIST_EXIT", case.list_exit),
            ("STUB_COMPARE", &case.compare),
            ("STUB_CHECKS", &case.checks),
        ]);
        assert_eq!(
            output.status.success(),
            case.succeeds,
            "{}: {output:?}",
            case.name
        );
        assert_eq!(calls, case.calls, "{}: the gh calls", case.name);
        for logged in &calls {
            let (token, arguments) = logged.split_once('\t').expect("a call logs its token");
            assert_eq!(
                token == OWNER_TOKEN,
                arguments.starts_with("pr\tcomment\t"),
                "{}: only the comment carries the owner's token: {logged:?}",
                case.name
            );
        }
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            case.stdout,
            "{}: the printed text",
            case.name
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "",
            "{}: stderr",
            case.name
        );
    }
}
