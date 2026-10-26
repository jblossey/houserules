//! The two published units of the review layer (spec 10.1, 10.2, 10.4 to 10.6,
//! and acceptance case 8): the GitHub reusable workflow
//! `.github/workflows/houserules-review.yml` and the GitLab file
//! `ci/gitlab-review.yml`.
//!
//! The tests read each unit with `saphyr` (`ci_support::parse_unit`) and run
//! the trust assertions of spec 10.1 on it. Each assertion has a mutation
//! test: a forbidden shape, made on the parsed unit, must fail it
//! (`quality.exclusive-pins-are-mutation-proven`). The scripts of the jobs run
//! for real, on Unix, with a stub `gh` and a stub `curl` on the `PATH`
//! (`ci_support::sandbox`): the `report` logic, the install step, the push of
//! the record, and the reviewers program of the GitLab push job. A run of a
//! script is the consumer's own run (`process.wiring-checks-run-the-resolution`),
//! and a match on its text is the first gate only.
//!
//! `actionlint` and `shellcheck` come from `mise.toml` (`mise install`); a test
//! fails, and does not skip, when one is missing. The shell tests are
//! Unix-only: the units run `jq` and `sh` on Linux images only.

mod ci_support;
#[cfg(unix)]
mod ci_units_shell;
mod tracker_support;

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use ci_support::{
    Host, ROLES, Unit, assert_floor_builds_outside_the_request,
    assert_floor_takes_no_setup_no_tool_and_never_none, assert_github_credential_masked,
    assert_github_permissions, assert_gitlab_jobs_inherit_only_runner_defaults, assert_inputs,
    assert_job_set, assert_key_input_accepts_every_install_key,
    assert_no_request_text_in_a_script_expression,
    assert_no_trace_and_git_trace_unset_in_write_jobs, assert_token_environment_on_write_jobs_only,
    assert_trust_rules, assert_write_jobs_run_no_houserules_and_check_out_no_project, install_key,
    parse_unit,
};
use serde_json::{Value, json};
use tracker_support::{assert_actionlint_passes, repo_root};

/// The GitHub unit, below the repository root.
const GITHUB_UNIT: &str = ".github/workflows/houserules-review.yml";

/// The GitLab unit, below the repository root.
const GITLAB_UNIT: &str = "ci/gitlab-review.yml";

/// The path of `relative` below the repository root, built component by
/// component.
fn path_of(relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(repo_root(), |path, part| path.join(part))
}

/// The text of the unit at `relative`.
fn unit_text(relative: &str) -> String {
    fs::read_to_string(path_of(relative)).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The GitHub unit.
fn github() -> Unit {
    parse_unit(&unit_text(GITHUB_UNIT), Host::GitHub)
}

/// The GitLab unit, read for the install with `key`.
fn gitlab(key: &str) -> Unit {
    parse_unit(&unit_text(GITLAB_UNIT), Host::gitlab(key))
}

/// `true` when `check` panics.
fn fails(check: impl FnOnce()) -> bool {
    catch_unwind(AssertUnwindSafe(check)).is_err()
}

/// Shows that `assertion` passes on `unit` and fails on the copy of it that
/// `edit` changes into the forbidden shape `shape`.
fn assert_fails_on(unit: &Unit, shape: &str, edit: impl FnOnce(&mut Value), assertion: fn(&Unit)) {
    assertion(unit);
    let mutated = unit.mutated(edit);
    assert!(
        fails(|| assertion(&mutated)),
        "the assertion must fail on: {shape}"
    );
}

/// Appends `addition` to the `run` script of the GitHub step `name` of the job
/// `role`.
fn append_to_step(body: &mut Value, role: &str, name: &str, addition: &str) {
    let steps = body["jobs"][role]["steps"]
        .as_array_mut()
        .unwrap_or_else(|| panic!("the job {role} holds steps"));
    let step = steps
        .iter_mut()
        .find(|step| step["name"] == name)
        .unwrap_or_else(|| panic!("the job {role} holds the step {name}"));
    let run = step["run"].as_str().expect("a run step").to_string();
    step["run"] = Value::String(format!("{run}\n{addition}\n"));
}

/// Appends `addition` to the script of the GitLab job `key`.
fn append_to_script(body: &mut Value, key: &str, addition: &str) {
    let script = body[key]["script"][0]
        .as_str()
        .expect("a script block")
        .to_string();
    body[key]["script"][0] = Value::String(format!("{script}\n{addition}\n"));
}

// ---- spec 10.1 and case 8: the structure of the units ----

/// Case 8: actionlint passes on the GitHub unit.
#[test]
fn the_github_unit_passes_actionlint() {
    assert_actionlint_passes(&path_of(GITHUB_UNIT));
}

/// Spec 10.1: both units hold the five jobs, and the GitLab jobs carry the key
/// of the install.
#[test]
fn both_units_parse_into_the_job_set_of_spec_10_1() {
    assert_job_set(&github());
    assert_job_set(&gitlab("root"));
    assert_job_set(&gitlab("docs"));
    let docs = gitlab("docs");
    assert_eq!(
        docs.job_keys(),
        vec![
            "houserules-docs-gates",
            "houserules-docs-floor",
            "houserules-docs-report",
            "houserules-docs-record",
            "houserules-docs-record-push",
        ],
        "the job names of the install docs, in the order of the file"
    );
}

/// Case 8: `floor-install: runner:<path>` is among the inputs of both units, and
/// so are the other inputs of spec 10.2.
#[test]
fn both_units_take_the_inputs_of_spec_10_2() {
    assert_inputs(&github());
    assert_inputs(&gitlab("root"));
}

/// Spec 10.2: each GitHub job holds exactly its `permissions`.
#[test]
fn each_github_job_holds_exactly_its_permissions() {
    assert_github_permissions(&github());
}

/// Spec 10.1: the token environment is on `report` and `record-push` only.
#[test]
fn only_report_and_record_push_can_read_a_write_credential() {
    assert_token_environment_on_write_jobs_only(&github());
    assert_token_environment_on_write_jobs_only(&gitlab("root"));
    assert_token_environment_on_write_jobs_only(&gitlab("docs"));
}

/// Spec 10.1: no unit sets a shell trace, and the write jobs unset the git trace
/// variables.
#[test]
fn no_unit_sets_a_shell_trace_and_the_write_jobs_unset_the_git_trace_variables() {
    assert_no_trace_and_git_trace_unset_in_write_jobs(&github());
    assert_no_trace_and_git_trace_unset_in_write_jobs(&gitlab("root"));
}

/// Spec 10.1: the GitHub write job masks the base64 form of the credential
/// before its first use.
#[test]
fn the_github_credential_is_masked_before_its_first_use() {
    assert_github_credential_masked(&github());
}

/// Spec 10.1: `report` and `record-push` run no houserules step and check out no
/// project code.
#[test]
fn the_write_jobs_run_no_houserules_and_check_out_no_project() {
    assert_write_jobs_run_no_houserules_and_check_out_no_project(&github());
    assert_write_jobs_run_no_houserules_and_check_out_no_project(&gitlab("root"));
}

/// Spec 10.1, design.md 242: `floor` takes no setup, runs no tool step, and
/// never takes `install: none`.
#[test]
fn the_floor_takes_no_setup_no_tool_and_never_none() {
    assert_floor_takes_no_setup_no_tool_and_never_none(&github());
    assert_floor_takes_no_setup_no_tool_and_never_none(&gitlab("root"));
}

/// C6: no script names the request description in an expression.
#[test]
fn no_script_names_the_request_description_in_an_expression() {
    assert_no_request_text_in_a_script_expression(&github());
    assert_no_request_text_in_a_script_expression(&gitlab("root"));
}

/// Both units pass every trust rule together.
#[test]
fn both_units_pass_every_trust_rule() {
    assert_trust_rules(&github());
    assert_trust_rules(&gitlab("root"));
}

/// Spec 10.1, design.md 242: the floor builds the default branch in a folder
/// that no file of the request reaches, on both hosts.
#[test]
fn the_floor_builds_the_default_branch_outside_the_request() {
    assert_floor_builds_outside_the_request(&github());
    assert_floor_builds_outside_the_request(&gitlab("root"));
    assert_floor_builds_outside_the_request(&gitlab("docs"));
}

/// Spec 10.1: no job of the GitLab unit inherits a `default:` keyword that runs
/// a command, starts a service, or restores a cache, for any install key.
#[test]
fn no_gitlab_job_inherits_a_default_keyword_that_runs_a_command() {
    assert_gitlab_jobs_inherit_only_runner_defaults(&gitlab("root"));
    assert_gitlab_jobs_inherit_only_runner_defaults(&gitlab("docs"));
}

/// Design.md 258: the key of an install is `root` for a top-level install, else
/// the path with each character outside `A-Za-z0-9_-` replaced by `-`.
#[test]
fn the_key_of_an_install_follows_design_258() {
    for (path, key) in [
        (".", "root"),
        ("./", "root"),
        ("docs", "docs"),
        ("./docs/", "docs"),
        ("sub/dir", "sub-dir"),
        ("packages/app.v2", "packages-app-v2"),
        ("docs.site", "docs-site"),
        (".config/x", "-config-x"),
        ("_internal/tools", "_internal-tools"),
    ] {
        assert_eq!(install_key(path), key, "the key of {path:?}");
    }
}

/// Design.md 258: the `key` input of the GitLab unit accepts every key that the
/// rule gives for a path that the `dir` input accepts, a key that starts with a
/// hyphen or an underscore included. The pattern that refused such a key fails
/// the assertion.
#[test]
fn the_key_input_accepts_every_key_that_the_rule_gives() {
    assert_key_input_accepts_every_install_key(&gitlab("root"));
    let text = unit_text(GITLAB_UNIT);
    let old = text.replace(
        "regex: ^[A-Za-z0-9_-]+$",
        "regex: ^[A-Za-z0-9][A-Za-z0-9_-]*$",
    );
    assert_ne!(
        old, text,
        "the key input holds the pattern that the test replaces"
    );
    assert!(
        fails(|| assert_key_input_accepts_every_install_key(&parse_unit(
            &old,
            Host::gitlab("root")
        ))),
        "a pattern that needs a letter or a digit first refuses the key -config-x"
    );
}

// ---- the mutation proofs: each forbidden shape fails its assertion ----

/// Each forbidden shape of the GitHub unit fails the assertion that forbids it.
#[test]
fn each_forbidden_shape_of_the_github_unit_fails_its_assertion() {
    let unit = github();
    assert_fails_on(
        &unit,
        "a sixth job",
        |body| body["jobs"]["extra"] = json!({"runs-on": "ubuntu-latest", "steps": []}),
        assert_job_set,
    );
    assert_fails_on(
        &unit,
        "issues: write on gates",
        |body| body["jobs"]["gates"]["permissions"]["issues"] = json!("write"),
        assert_github_permissions,
    );
    assert_fails_on(
        &unit,
        "an extra scope on floor",
        |body| body["jobs"]["floor"]["permissions"]["pull-requests"] = json!("read"),
        assert_github_permissions,
    );
    assert_fails_on(
        &unit,
        "a workflow-level permissions key",
        |body| body["permissions"] = json!({"contents": "write"}),
        assert_github_permissions,
    );
    assert_fails_on(
        &unit,
        "statuses: write on gates",
        |body| body["jobs"]["gates"]["permissions"]["statuses"] = json!("write"),
        assert_token_environment_on_write_jobs_only,
    );
    assert_fails_on(
        &unit,
        "the token in the environment of floor",
        |body| body["jobs"]["floor"]["env"]["GH_TOKEN"] = json!("${{ github.token }}"),
        assert_token_environment_on_write_jobs_only,
    );
    assert_fails_on(
        &unit,
        "a shell trace in gates",
        |body| append_to_step(body, "gates", "Check the backlog", "set -x"),
        assert_no_trace_and_git_trace_unset_in_write_jobs,
    );
    assert_fails_on(
        &unit,
        "a shell trace with -o xtrace in record",
        |body| append_to_step(body, "record", "Write the record", "set -o xtrace"),
        assert_no_trace_and_git_trace_unset_in_write_jobs,
    );
    assert_fails_on(
        &unit,
        "report without the unset of the git trace variables",
        |body| {
            let steps = body["jobs"]["report"]["steps"]
                .as_array_mut()
                .expect("steps");
            let step = steps
                .iter_mut()
                .find(|step| step["name"] == "Report the route")
                .expect("the report step");
            let run = step["run"]
                .as_str()
                .expect("run")
                .replace("unset", "export");
            step["run"] = Value::String(run);
        },
        assert_no_trace_and_git_trace_unset_in_write_jobs,
    );
    assert_fails_on(
        &unit,
        "record-push without the mask",
        |body| {
            let steps = body["jobs"]["record-push"]["steps"]
                .as_array_mut()
                .expect("steps");
            let step = steps
                .iter_mut()
                .find(|step| step["name"] == "Push the record")
                .expect("the push step");
            let run = step["run"]
                .as_str()
                .expect("run")
                .replace("::add-mask::", "::debug::");
            step["run"] = Value::String(run);
        },
        assert_github_credential_masked,
    );
    assert_fails_on(
        &unit,
        "a checkout in report",
        |body| {
            body["jobs"]["report"]["steps"]
                .as_array_mut()
                .expect("steps")
                .push(json!({"uses": "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"}));
        },
        assert_write_jobs_run_no_houserules_and_check_out_no_project,
    );
    assert_fails_on(
        &unit,
        "a houserules step in record-push",
        |body| {
            body["jobs"]["record-push"]["steps"]
                .as_array_mut()
                .expect("steps")
                .push(json!({"name": "Show", "run": "houserules record show HEAD"}));
        },
        assert_write_jobs_run_no_houserules_and_check_out_no_project,
    );
    assert_fails_on(
        &unit,
        "a project tool in report",
        |body| append_to_step(body, "report", "Report the route", "cargo test"),
        assert_write_jobs_run_no_houserules_and_check_out_no_project,
    );
    assert_fails_on(
        &unit,
        "the setup command in record-push",
        |body| {
            append_to_step(
                body,
                "record-push",
                "Push the record",
                "eval \"$HOUSERULES_SETUP\"",
            )
        },
        assert_write_jobs_run_no_houserules_and_check_out_no_project,
    );
    assert_fails_on(
        &unit,
        "the setup input in floor",
        |body| body["jobs"]["floor"]["env"]["HOUSERULES_SETUP"] = json!("${{ inputs.setup }}"),
        assert_floor_takes_no_setup_no_tool_and_never_none,
    );
    assert_fails_on(
        &unit,
        "the install input in floor",
        |body| body["jobs"]["floor"]["env"]["HOUSERULES_SOURCE"] = json!("${{ inputs.install }}"),
        assert_floor_takes_no_setup_no_tool_and_never_none,
    );
    assert_fails_on(
        &unit,
        "a tool action in floor",
        |body| {
            body["jobs"]["floor"]["steps"]
                .as_array_mut()
                .expect("steps")
                .push(json!({"uses": "jdx/mise-action@fc42bcc7ddbf51124c30f95a877a0c5168a3bfde"}));
        },
        assert_floor_takes_no_setup_no_tool_and_never_none,
    );
    assert_fails_on(
        &unit,
        "the description in an expression of a run script",
        |body| {
            append_to_step(
                body,
                "gates",
                "Check the backlog",
                "echo ${{ github.event.pull_request.body }}",
            );
        },
        assert_no_request_text_in_a_script_expression,
    );
    assert_fails_on(
        &unit,
        "no floor-install input",
        |body| {
            body["on"]["workflow_call"]["inputs"]
                .as_object_mut()
                .expect("inputs")
                .remove("floor-install");
        },
        assert_inputs,
    );
    assert_fails_on(
        &unit,
        "an input that spec 10.2 does not list",
        |body| {
            body["on"]["workflow_call"]["inputs"]["extra"] =
                json!({"type": "string", "default": ""});
        },
        assert_inputs,
    );
    assert_fails_on(
        &unit,
        "a floor-install input that does not name runner:<path>",
        |body| {
            body["on"]["workflow_call"]["inputs"]["floor-install"]["description"] =
                json!("the binary of the floor");
        },
        assert_inputs,
    );
    assert_fails_on(
        &unit,
        "the request checked out at the root of the workspace",
        |body| set_checkout_path(body, "pull_request.head.sha", None),
        assert_floor_builds_outside_the_request,
    );
    assert_fails_on(
        &unit,
        "the request checked out at the workspace itself",
        |body| set_checkout_path(body, "pull_request.head.sha", Some(".")),
        assert_floor_builds_outside_the_request,
    );
    assert_fails_on(
        &unit,
        "the default branch checked out below the request",
        |body| {
            let request = body["jobs"]["floor"]["steps"]
                .as_array()
                .expect("steps")
                .iter()
                .find_map(|step| {
                    (step["with"]["ref"]
                        .as_str()?
                        .contains("pull_request.head.sha"))
                    .then(|| step["with"]["path"].as_str().map(str::to_string))
                    .flatten()
                })
                .expect("the request path");
            set_checkout_path(body, "default_branch", Some(&format!("{request}/default")));
        },
        assert_floor_builds_outside_the_request,
    );
    assert_fails_on(
        &unit,
        "the request checked out below the default branch",
        |body| {
            set_checkout_path(
                body,
                "pull_request.head.sha",
                Some("houserules-default-branch/request"),
            )
        },
        assert_floor_builds_outside_the_request,
    );
    assert_fails_on(
        &unit,
        "the floor read in the workspace instead of the folder of the request",
        |body| {
            let steps = body["jobs"]["floor"]["steps"]
                .as_array_mut()
                .expect("steps");
            let step = steps
                .iter_mut()
                .find(|step| step["name"] == "Compute the floor")
                .expect("the floor step");
            step.as_object_mut()
                .expect("a step")
                .remove("working-directory");
        },
        assert_floor_builds_outside_the_request,
    );
    assert_fails_on(
        &unit,
        "a build of a source that no checkout made",
        |body| {
            let steps = body["jobs"]["floor"]["steps"]
                .as_array_mut()
                .expect("steps");
            let step = steps
                .iter_mut()
                .find(|step| step["name"] == "Install houserules")
                .expect("the install step");
            step["env"]["HOUSERULES_DEFAULT_SRC"] = json!("${{ github.workspace }}/elsewhere");
        },
        assert_floor_builds_outside_the_request,
    );
}

/// Sets the `path` of the checkout step of the job `floor` whose ref holds
/// `ref_contains`, or removes it with `None`.
fn set_checkout_path(body: &mut Value, ref_contains: &str, path: Option<&str>) {
    let steps = body["jobs"]["floor"]["steps"]
        .as_array_mut()
        .expect("steps");
    let step = steps
        .iter_mut()
        .find(|step| {
            step["with"]["ref"]
                .as_str()
                .is_some_and(|value| value.contains(ref_contains))
        })
        .expect("the checkout step");
    match path {
        Some(path) => step["with"]["path"] = json!(path),
        None => {
            step["with"].as_object_mut().expect("with").remove("path");
        }
    }
}

/// Each forbidden shape of the GitLab unit fails the assertion that forbids it.
#[test]
fn each_forbidden_shape_of_the_gitlab_unit_fails_its_assertion() {
    let unit = gitlab("root");
    assert_fails_on(
        &unit,
        "a sixth job",
        |body| body["houserules-root-extra"] = json!({"script": ["true"]}),
        assert_job_set,
    );
    assert_fails_on(
        &unit,
        "a job without the key of the install",
        |body| body["houserules-gates"] = json!({"script": ["true"]}),
        assert_job_set,
    );
    assert_fails_on(
        &unit,
        "the token environment on gates",
        |body| {
            body["houserules-root-gates"]["environment"] =
                json!({"name": "houserules-root", "action": "access"});
        },
        assert_token_environment_on_write_jobs_only,
    );
    assert_fails_on(
        &unit,
        "no token environment on report",
        |body| {
            body["houserules-root-report"]
                .as_object_mut()
                .expect("a job")
                .remove("environment");
        },
        assert_token_environment_on_write_jobs_only,
    );
    assert_fails_on(
        &unit,
        "the token environment on report with another action",
        |body| body["houserules-root-report"]["environment"]["action"] = json!("start"),
        assert_token_environment_on_write_jobs_only,
    );
    assert_fails_on(
        &unit,
        "the token named in a script of record",
        |body| append_to_script(body, "houserules-root-record", "echo \"$HOUSERULES_TOKEN\""),
        assert_token_environment_on_write_jobs_only,
    );
    assert_fails_on(
        &unit,
        "a shell trace in report",
        |body| append_to_script(body, "houserules-root-report", "set -ex"),
        assert_no_trace_and_git_trace_unset_in_write_jobs,
    );
    assert_fails_on(
        &unit,
        "CI_DEBUG_TRACE in a variable of gates",
        |body| body["houserules-root-gates"]["variables"]["CI_DEBUG_TRACE"] = json!("true"),
        assert_no_trace_and_git_trace_unset_in_write_jobs,
    );
    assert_fails_on(
        &unit,
        "a checkout in record-push",
        |body| body["houserules-root-record-push"]["variables"]["GIT_STRATEGY"] = json!("fetch"),
        assert_write_jobs_run_no_houserules_and_check_out_no_project,
    );
    assert_fails_on(
        &unit,
        "git clone in report",
        |body| {
            append_to_script(
                body,
                "houserules-root-report",
                "git clone \"$CI_REPOSITORY_URL\" project",
            )
        },
        assert_write_jobs_run_no_houserules_and_check_out_no_project,
    );
    assert_fails_on(
        &unit,
        "houserules in record-push",
        |body| {
            append_to_script(
                body,
                "houserules-root-record-push",
                "houserules record show HEAD",
            )
        },
        assert_write_jobs_run_no_houserules_and_check_out_no_project,
    );
    assert_fails_on(
        &unit,
        "the setup command in floor",
        |body| append_to_script(body, "houserules-root-floor", "eval \"$HOUSERULES_SETUP\""),
        assert_floor_takes_no_setup_no_tool_and_never_none,
    );
    assert_fails_on(
        &unit,
        "the install input in floor",
        |body| {
            body["houserules-root-floor"]["variables"]["HOUSERULES_SOURCE"] =
                json!("$[[ inputs.install ]]");
        },
        assert_floor_takes_no_setup_no_tool_and_never_none,
    );
    assert_fails_on(
        &unit,
        "the description variable in a script",
        |body| {
            append_to_script(
                body,
                "houserules-root-gates",
                "echo \"$CI_MERGE_REQUEST_DESCRIPTION\"",
            )
        },
        assert_no_request_text_in_a_script_expression,
    );
    assert_fails_on(
        &unit,
        "no inherit key on the hidden job",
        |body| {
            body[".houserules-root-base"]
                .as_object_mut()
                .expect("the hidden job")
                .remove("inherit");
        },
        assert_gitlab_jobs_inherit_only_runner_defaults,
    );
    assert_fails_on(
        &unit,
        "every default keyword inherited",
        |body| body[".houserules-root-base"]["inherit"] = json!({"default": true}),
        assert_gitlab_jobs_inherit_only_runner_defaults,
    );
    assert_fails_on(
        &unit,
        "an inherit key that leaves the defaults alone",
        |body| body[".houserules-root-base"]["inherit"] = json!({"variables": false}),
        assert_gitlab_jobs_inherit_only_runner_defaults,
    );
    for keyword in [
        "before_script",
        "after_script",
        "hooks",
        "services",
        "cache",
        "id_tokens",
    ] {
        assert_fails_on(
            &unit,
            "a default keyword that runs a command, starts a service, or restores a cache",
            |body| {
                body[".houserules-root-base"]["inherit"] = json!({"default": ["tags", keyword]});
            },
            assert_gitlab_jobs_inherit_only_runner_defaults,
        );
    }
    assert_fails_on(
        &unit,
        "report that inherits every default keyword again",
        |body| body["houserules-root-report"]["inherit"] = json!({"default": true}),
        assert_gitlab_jobs_inherit_only_runner_defaults,
    );
    assert_fails_on(
        &unit,
        "a source of the default branch below the project folder",
        |body| {
            let script = body["houserules-root-floor"]["script"][0]
                .as_str()
                .expect("a script block")
                .replace(
                    "HOUSERULES_DEFAULT_SRC=$(mktemp -d",
                    "HOUSERULES_DEFAULT_SRC=$CI_PROJECT_DIR/default #",
                );
            body["houserules-root-floor"]["script"][0] = Value::String(script);
        },
        assert_floor_builds_outside_the_request,
    );
}

// ---- the quoting helpers of ci_support ----

/// A word in a quote, in a comment, or in a message is no invocation; a word
/// outside a quote is.
#[test]
fn the_shell_lexer_separates_a_word_from_a_message() {
    use ci_support::{bare_words, invokes, traces_the_shell};
    assert!(invokes("houserules check-knowledge", "houserules"));
    assert!(invokes(
        "if ! houserules audit; then exit 1; fi",
        "houserules"
    ));
    assert!(invokes("x=$(houserules route)", "houserules"));
    assert!(invokes("./target/debug/houserules route", "houserules"));
    assert!(!invokes(
        "hr_notice \"houserules: it follows; houserules\"",
        "houserules"
    ));
    assert!(!invokes("echo 'run houserules ci print'", "houserules"));
    assert!(!invokes("# houserules check-knowledge", "houserules"));
    assert!(!invokes("echo x # houserules route", "houserules"));
    assert_eq!(bare_words("a \"b c\" d'e' f"), vec!["a", "f"]);
    assert!(traces_the_shell("set -x"));
    assert!(traces_the_shell("set -eux"));
    assert!(traces_the_shell("set -o xtrace"));
    assert!(traces_the_shell("bash -x script.sh"));
    assert!(!traces_the_shell("set -eu"));
    assert!(!traces_the_shell("set -- --base x"));
    assert!(!traces_the_shell("echo \"set -x\""));
}

// ---- pins, versions, and hosts ----

/// Every `uses:` line of `text` pins its action to a full commit sha of 40
/// lowercase hex digits and names the release in a comment
/// (`houserules.actions-pinned-by-sha`). At least one such line exists.
fn assert_actions_pinned_by_sha(text: &str) {
    let mut count = 0;
    for line in text.lines() {
        let trimmed = line.trim_start().trim_start_matches("- ");
        let Some(rest) = trimmed.strip_prefix("uses:") else {
            continue;
        };
        count += 1;
        let (action, comment) = rest
            .trim()
            .split_once(" # ")
            .unwrap_or_else(|| panic!("no release comment on: {line}"));
        let (_, sha) = action
            .rsplit_once('@')
            .unwrap_or_else(|| panic!("no pin on: {line}"));
        assert!(
            sha.len() == 40 && sha.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
            "not a full commit sha on: {line}"
        );
        let release = comment
            .strip_prefix('v')
            .unwrap_or_else(|| panic!("the comment names no vX.Y.Z on: {line}"));
        assert!(
            release.split('.').count() >= 2
                && release
                    .split('.')
                    .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())),
            "the comment names no vX.Y.Z on: {line}"
        );
    }
    assert!(count > 0, "the unit uses at least one action");
}

/// `houserules.actions-pinned-by-sha`: every action of the GitHub unit is pinned
/// by a full sha with its release in a comment.
#[test]
fn every_action_of_the_github_unit_is_pinned_by_sha() {
    assert_actions_pinned_by_sha(&unit_text(GITHUB_UNIT));
    for unpinned in [
        "      - uses: actions/checkout@v7 # v7.0.1\n",
        "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1\n",
        "      - uses: actions/checkout@3D3C42E5AAC5BA805825DA76410C181273BA90B1 # v7.0.1\n",
        "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b # v7.0.1\n",
        "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # main\n",
    ] {
        assert!(
            fails(|| assert_actions_pinned_by_sha(unpinned)),
            "the check must fail on {unpinned:?}"
        );
    }
}

/// `houserules.actions-default-shell-lacks-pipefail`: every run step of the
/// GitHub unit names `shell: bash`, so a failing left side of a pipe fails the
/// step.
#[test]
fn every_run_step_of_the_github_unit_names_shell_bash() {
    let unit = github();
    let mut count = 0;
    for role in ROLES {
        let steps = unit.job(role)["steps"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for step in steps {
            if step.get("run").is_some() {
                count += 1;
                assert_eq!(
                    step["shell"], "bash",
                    "the run step {} of the {role} job names shell: bash",
                    step["name"]
                );
            }
        }
    }
    assert!(count > 10, "the unit holds its run steps: {count}");
}

/// The default image of the GitLab unit is pinned by digest, and the text of
/// its description names a git of 2.41 or later, a curl, and a jq with their
/// versions. The test reads the text of the unit and runs no image: it does not
/// show that the image holds those versions. The versions of the binaries on
/// the `PATH` of the image are read by running them in it
/// (`podman run --rm --network=none <image> git --version`, and the same for
/// `curl` and `jq`), and the description is kept in step by hand at each bump of
/// the digest.
#[test]
fn the_gitlab_image_default_is_pinned_by_digest_and_its_description_names_the_tools() {
    let unit = gitlab("root");
    let input = unit.input("image");
    let default = input["default"].as_str().expect("a default image");
    let (name, digest) = default.split_once("@sha256:").expect("a digest pin");
    assert!(
        !name.is_empty() && !name.contains('@'),
        "the image name: {name}"
    );
    assert!(
        digest.len() == 64 && digest.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "a sha256 digest: {digest}"
    );
    let description = input["description"].as_str().expect("a description");
    let git_version = description
        .rsplit("git ")
        .next()
        .and_then(|rest| rest.split([',', ' ']).next())
        .unwrap_or_else(|| panic!("the description names the git version: {description}"));
    let mut parts = git_version
        .split('.')
        .map(|part| part.parse::<u32>().expect("a numeric git version"));
    let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    assert!(
        (major, minor) >= (2, 41),
        "git 2.41 or later has --attr-source: {git_version}"
    );
    for tool in ["curl ", "jq "] {
        assert!(
            description.contains(tool),
            "the description names the version of {tool}: {description}"
        );
    }
}

/// The first version of `line` as release-please's generic updater reads it
/// (`VERSION_REGEX` of `src/updaters/generic.ts`, v17.6.0): digits, a dot,
/// digits, a dot, digits, then an optional `-` and `[\w.]+`, then an optional
/// `+` and `[-\w.]+`. Returns the byte range.
fn first_version(line: &str) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let digits = |from: usize| {
        bytes[from..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
    };
    for start in 0..bytes.len() {
        let mut at = start;
        let mut ok = true;
        for part in 0..3 {
            let count = digits(at);
            if count == 0 {
                ok = false;
                break;
            }
            at += count;
            if part < 2 {
                if bytes.get(at) == Some(&b'.') {
                    at += 1;
                } else {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        let word = |byte: &u8| byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b'.';
        if bytes.get(at) == Some(&b'-') {
            let count = bytes[at + 1..].iter().take_while(|byte| word(byte)).count();
            if count > 0 {
                at += 1 + count;
            }
        }
        if bytes.get(at) == Some(&b'+') {
            let count = bytes[at + 1..]
                .iter()
                .take_while(|byte| word(byte) || **byte == b'-')
                .count();
            if count > 0 {
                at += 1 + count;
            }
        }
        return Some((start, at));
    }
    None
}

/// The lines of `text` that the updater treats as annotated: they hold
/// `x-release-please-` and one of its scopes (`INLINE_UPDATE_REGEX`,
/// `BLOCK_START_REGEX`, and `BLOCK_END_REGEX`).
fn annotated_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.contains("x-release-please-"))
        .collect()
}

/// What the updater does to `text` for the release `next`: the first version of
/// each line with the inline annotation is replaced.
fn apply_release(text: &str, next: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.contains("x-release-please-version") {
            let (start, end) = first_version(line).expect("a version on the annotated line");
            lines.push(format!("{}{next}{}", &line[..start], &line[end..]));
        } else {
            lines.push(line.to_string());
        }
    }
    let mut result = lines.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// The version of the one annotated line of the GitLab unit text `text`.
fn version_line_of(text: &str) -> String {
    let lines = annotated_lines(text);
    assert_eq!(
        lines.len(),
        1,
        "one line carries the release annotation: {lines:?}"
    );
    assert!(
        lines[0].contains("x-release-please-version"),
        "the annotation is the inline one: {}",
        lines[0]
    );
    let (start, end) = first_version(lines[0]).expect("a version on the annotated line");
    assert!(
        first_version(&lines[0][end..]).is_none(),
        "a second version on the annotated line would be rewritten too: {}",
        lines[0]
    );
    lines[0][start..end].to_string()
}

/// Case 8, C16: the GitLab unit holds one version line, and it equals the
/// crate's version, so the published file, the embedded file, and the hash that
/// `ci print` prints are the same bytes at each release.
#[test]
fn the_gitlab_version_line_equals_the_crate_version() {
    assert_eq!(
        version_line_of(&unit_text(GITLAB_UNIT)),
        env!("CARGO_PKG_VERSION"),
        "ci/gitlab-review.yml and crates/houserules/Cargo.toml hold one version"
    );
}

/// Case 8: the GitHub unit holds no release version line. Release-please's token
/// cannot change a workflow file, so the unit carries no annotation, and the
/// updater would leave it as it is.
#[test]
fn the_github_unit_holds_no_release_version_line() {
    let text = unit_text(GITHUB_UNIT);
    assert!(
        annotated_lines(&text).is_empty(),
        "no release annotation in the GitHub unit"
    );
    assert_eq!(
        apply_release(&text, "9.8.7"),
        text,
        "the updater changes nothing in it"
    );
    let minimum = github()
        .variable("gates", "HOUSERULES_MINIMUM_VERSION")
        .expect("the minimum version line");
    let (start, end) = first_version(&minimum).expect("a release version");
    assert_eq!(
        (start, end),
        (0, minimum.len()),
        "the minimum is one release version: {minimum}"
    );
    let numbers: Vec<u32> = minimum
        .split('.')
        .map(|part| part.parse().expect("a numeric release version"))
        .collect();
    assert!(
        numbers[0] >= 2,
        "2.0.0 is the first release with the commands of the unit: {minimum}"
    );
}

/// The updater's own rewrite (`first_version`, `apply_release`) changes the
/// version of the one annotated line and no other line.
#[test]
fn a_release_rewrites_the_version_line_of_the_gitlab_unit_and_nothing_else() {
    let text = unit_text(GITLAB_UNIT);
    let bumped = apply_release(&text, "9.8.7");
    let changed: Vec<(&str, &str)> = text
        .lines()
        .zip(bumped.lines())
        .filter(|(before, after)| before != after)
        .collect();
    assert_eq!(changed.len(), 1, "one line changes: {changed:?}");
    assert_eq!(version_line_of(&bumped), "9.8.7");
    assert_eq!(text.lines().count(), bumped.lines().count());
    for (version, expected) in [
        (
            "HOUSERULES_VERSION: '1.4.0' # x-release-please-version",
            Some("1.4.0"),
        ),
        ("x 10.20.30-rc.1+b.2 y", Some("10.20.30-rc.1+b.2")),
        ("v2.0.0", Some("2.0.0")),
        ("no version 1.2 here", None),
    ] {
        let found = first_version(version).map(|(start, end)| &version[start..end]);
        assert_eq!(found, expected, "the first version of {version:?}");
    }
}

/// `process.gates-cover-generated-commits`, release-please: a release commit
/// moves the version line of the GitLab unit and the crate's version in one
/// commit. The version test passes on both files of the commit, and it fails
/// when only one moved (the accepted red of a release PR whose unit file is
/// out of step). The unit stays valid.
#[test]
fn a_release_commit_passes_the_version_test_only_with_both_files() {
    let text = unit_text(GITLAB_UNIT);
    let bumped = apply_release(&text, "9.8.7");
    assert_ne!(version_line_of(&bumped), env!("CARGO_PKG_VERSION"));
    assert_eq!(version_line_of(&bumped), "9.8.7");
    assert_trust_rules(&parse_unit(&bumped, Host::gitlab("root")));
}

/// `process.gates-cover-generated-commits`, Dependabot: a bump of one action pin
/// in the GitHub unit changes the sha and the release comment, and no test
/// reads either value: the unit stays pinned, valid, and trusted.
#[test]
fn a_dependabot_bump_of_one_action_pin_passes_every_static_check() {
    let text = unit_text(GITHUB_UNIT);
    let line = text
        .lines()
        .find(|line| line.contains("actions/upload-artifact@"))
        .expect("an upload-artifact pin");
    let (_, pin) = line.split_once("actions/upload-artifact@").expect("a pin");
    let (sha, comment) = pin.split_once(" # v").expect("a release comment");
    // Another sha and the next patch release, whatever the pin holds now.
    let other_first = if sha.starts_with('0') { "1" } else { "0" };
    let (release, patch) = comment
        .rsplit_once('.')
        .expect("a release with a patch part");
    let next_patch: u64 = patch.parse::<u64>().expect("a numeric patch") + 1;
    let bumped = text.replace(
        &format!("actions/upload-artifact@{pin}"),
        &format!(
            "actions/upload-artifact@{other_first}{} # v{release}.{next_patch}",
            &sha[1..]
        ),
    );
    assert_ne!(bumped, text);
    assert_actions_pinned_by_sha(&bumped);
    assert_trust_rules(&parse_unit(&bumped, Host::GitHub));
    assert_eq!(apply_release(&bumped, "9.8.7"), bumped);
}

/// Case 8: no unit holds `github.com` outside the listed uses: this repository's
/// own release download, and the Beads release download of the Beads tracker
/// form. The origin host detection of `ci` is in the binary, not in a unit.
#[test]
fn no_unit_holds_github_com_outside_the_listed_uses() {
    for relative in [GITHUB_UNIT, GITLAB_UNIT] {
        let text = unit_text(relative);
        for (index, _) in text.match_indices("github.com") {
            assert!(
                text[..index].ends_with("://"),
                "github.com is part of a URL in {relative} at byte {index}"
            );
            let after = &text[index + "github.com".len()..];
            assert!(
                after.starts_with("/jblossey/houserules/releases/download/")
                    || after.starts_with("/gastownhall/beads/releases/download/"),
                "github.com outside the listed uses in {relative}: {}",
                after.lines().next().unwrap_or_default()
            );
        }
    }
}

/// `houserules.secret-store-follows-the-triggering-actor` and
/// `houserules.gh-filtered-lists-read-the-search-index`: the GitHub unit reads
/// no secret, so a run that Dependabot starts needs no copy of one in the
/// Dependabot store; and no unit lists requests with a filter flag of `gh pr
/// list`, which reads the search index. The units read the REST list of the
/// commit.
#[test]
fn the_units_read_no_secret_and_no_search_backed_list() {
    let github = unit_text(GITHUB_UNIT);
    assert!(
        !github.contains("secrets."),
        "the GitHub unit names no secret"
    );
    for relative in [GITHUB_UNIT, GITLAB_UNIT] {
        let text = unit_text(relative);
        assert!(
            !text.contains("gh pr list"),
            "{relative} lists no request with gh pr list"
        );
        assert!(!text.contains("gh search"), "{relative} searches nothing");
    }
}

/// The Beads release that the units install is the one `mise.toml` pins, which
/// the seeded Beads form installs too.
#[test]
fn the_units_install_the_beads_release_that_mise_pins() {
    let mise = fs::read_to_string(path_of("mise.toml")).expect("read mise.toml");
    let version = mise
        .lines()
        .find_map(|line| line.strip_prefix("\"github:gastownhall/beads\" = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("mise.toml pins beads");
    for relative in [GITHUB_UNIT, GITLAB_UNIT] {
        let text = unit_text(relative);
        assert!(
            text.contains(&format!("beads_{version}_linux_amd64.tar.gz"))
                && text.contains(&format!("releases/download/v{version}")),
            "{relative} installs beads {version}"
        );
    }
}

/// The shell text that both units hold is one text: the shared functions of the
/// install step and of the push of the record, and the `report` logic.
#[test]
fn the_shared_shell_text_is_the_same_in_both_units() {
    let github = github();
    let gitlab = gitlab("root");
    let shared = github
        .variable("gates", "HOUSERULES_SH")
        .expect("the shared functions");
    assert!(shared.contains("hr_install()") && shared.contains("hr_push_note()"));
    assert_eq!(gitlab.variable("gates", "HOUSERULES_SH"), Some(shared));
    let route = github
        .variable("report", "HOUSERULES_ROUTE_SH")
        .expect("the report logic");
    assert!(route.contains("applied_route()"));
    assert_eq!(
        gitlab.variable("report", "HOUSERULES_ROUTE_SH"),
        Some(route)
    );
}

/// The names that a GitHub unit gives its artifacts and its concurrency group.
fn github_names(unit: &Unit) -> Vec<String> {
    let mut names = Vec::new();
    for role in ROLES {
        let job = unit.job(role);
        if let Some(group) = job["concurrency"]["group"].as_str() {
            names.push(group.to_string());
        }
        for step in job["steps"].as_array().cloned().unwrap_or_default() {
            if step["uses"]
                .as_str()
                .is_some_and(|uses| uses.contains("-artifact@"))
            {
                names.push(
                    step["with"]["name"]
                        .as_str()
                        .expect("an artifact name")
                        .to_string(),
                );
            }
        }
    }
    names
}

/// Case 8: two installs of one repository get distinct job names, token
/// environments, artifacts, and (see the shell tests) markers, status contexts,
/// labels, and notes refs.
#[test]
fn two_installs_of_one_repository_get_distinct_names() {
    let root = gitlab("root");
    let docs = gitlab("docs");
    let root_keys: std::collections::BTreeSet<_> = root
        .job_keys()
        .into_iter()
        .chain(root.hidden_keys())
        .collect();
    let docs_keys: std::collections::BTreeSet<_> = docs
        .job_keys()
        .into_iter()
        .chain(docs.hidden_keys())
        .collect();
    assert_eq!(
        root_keys.intersection(&docs_keys).count(),
        0,
        "no job name is shared"
    );
    for role in ["report", "record-push"] {
        assert_eq!(
            root.environment(role),
            Some(("houserules-root".to_string(), "access".to_string()))
        );
        assert_eq!(
            docs.environment(role),
            Some(("houserules-docs".to_string(), "access".to_string()))
        );
    }
    let artifact_names = |unit: &Unit| -> Vec<String> {
        ROLES
            .iter()
            .filter_map(|role| {
                unit.job(role)["artifacts"]["name"]
                    .as_str()
                    .map(str::to_string)
            })
            .collect()
    };
    assert_eq!(
        artifact_names(&root),
        vec!["houserules-evidence-root", "houserules-record-root"]
    );
    assert_eq!(
        artifact_names(&docs),
        vec!["houserules-evidence-docs", "houserules-record-docs"]
    );
    for role in ["report", "record-push"] {
        for need in docs.job(role)["needs"].as_array().expect("needs") {
            let job = need["job"].as_str().expect("a needed job");
            assert!(
                docs_keys.contains(job),
                "{role} needs only jobs of its own install: {job}"
            );
        }
    }
    let text = unit_text(GITHUB_UNIT);
    let for_key = |key: &str| {
        github_names(&parse_unit(
            &text.replace("${{ inputs.key }}", key),
            Host::GitHub,
        ))
    };
    let (first, second) = (for_key("root"), for_key("docs"));
    assert_eq!(
        first.len(),
        5,
        "an evidence upload and download, a record upload and download, and a group: {first:?}"
    );
    for (one, other) in first.iter().zip(&second) {
        assert_ne!(
            one, other,
            "a name of the GitHub unit does not carry the key"
        );
    }
}
