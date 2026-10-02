//! Pins the Dependabot auto-merge wiring: `.github/dependabot.yml` groups
//! the two `github/codeql-action` steps into one pull request for version
//! updates and for security updates, and
//! `.github/workflows/dependabot-auto-merge.yml` approves a minor or patch
//! update before it enables auto-merge.
//!
//! The `init` and `analyze` steps of `codeql.yml` must run one
//! `codeql-action` version: a pull request that moves one step alone fails
//! CodeQL on the version mismatch
//! (`houserules.codeql-action-steps-share-one-version`). An enabled
//! auto-merge waits for every ruleset requirement, the approving review
//! included (`houserules.auto-merge-waits-for-every-ruleset-rule`), so the
//! workflow approves first and enables auto-merge second. Both steps run
//! under the minor-or-patch condition, so a major update gets neither.
//!
//! This crate has no YAML dependency. The helpers of `workflow_reader` read
//! both files by indentation, which is enough for the fixed shape these
//! files keep.
//! `approve_script_approves_only_a_pull_request_that_is_not_approved` runs
//! the approve step's own script, so the decision logic is proven by its
//! consumer's run and not by a string match.

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
