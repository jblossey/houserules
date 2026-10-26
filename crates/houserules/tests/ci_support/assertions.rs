//! One assertion for each trust rule of spec 10.1 and case 8, and `assert_trust_rules`,
//! which runs them all. Each assertion panics with the rule that failed.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::{Host, ROLES, Unit, WRITE_ROLES, bare_words, install_key, invokes, traces_the_shell};

/// The default keywords of the adopter's pipeline that a GitLab job of the unit
/// may inherit: they select or retry a runner and run no command, start no
/// service, restore no cache, and hand out no token.
pub const INHERITED_DEFAULTS: [&str; 3] = ["tags", "retry", "interruptible"];

/// What the `permissions` of a GitHub job hold, by role (spec 10.2).
pub fn expected_permissions(role: &str) -> BTreeMap<String, String> {
    let scopes: &[(&str, &str)] = match role {
        "gates" => &[("contents", "read"), ("issues", "read")],
        "floor" => &[("contents", "read")],
        "report" => &[
            ("pull-requests", "write"),
            ("statuses", "write"),
            ("issues", "write"),
        ],
        "record" => &[
            ("contents", "read"),
            ("pull-requests", "read"),
            ("issues", "read"),
        ],
        "record-push" => &[("contents", "write")],
        other => panic!("{other} is no role of the review layer"),
    };
    scopes
        .iter()
        .map(|(scope, access)| (scope.to_string(), access.to_string()))
        .collect()
}

/// Spec 10.1: the unit holds exactly the five jobs. A GitHub unit holds no
/// other job. A GitLab unit holds the five jobs of its install, named
/// `houserules-<key>-<role>`, and one hidden job that they extend.
pub fn assert_job_set(unit: &Unit) {
    let expected: BTreeSet<String> = ROLES.iter().map(|role| role.to_string()).collect();
    assert_eq!(
        unit.roles(),
        expected,
        "the unit holds exactly the jobs gates, floor, report, record, and record-push"
    );
    if let Host::GitLab { key } = &unit.host {
        let wrong: Vec<String> = unit
            .job_keys()
            .into_iter()
            .filter(|name| !name.starts_with(&format!("houserules-{key}-")))
            .collect();
        assert!(
            wrong.is_empty(),
            "jobs that carry no key of the install: {wrong:?}"
        );
        assert_eq!(
            unit.hidden_keys(),
            vec![unit.hidden_base_name()],
            "the unit holds one hidden job, which names the key of the install"
        );
    }
}

/// Spec 10.2: each GitHub job holds exactly its `permissions`. No other job
/// holds a write scope, and no workflow-level `permissions` key widens one.
pub fn assert_github_permissions(unit: &Unit) {
    assert_eq!(unit.host, Host::GitHub, "permissions are a GitHub rule");
    for role in ROLES {
        assert_eq!(
            unit.permissions(role),
            expected_permissions(role),
            "the permissions of the {role} job"
        );
    }
    assert!(
        unit.top_level("permissions").is_none(),
        "the workflow sets no permissions key above its jobs"
    );
}

/// Spec 10.1: no job that runs houserules, the setup command, or code of the
/// project reads a write credential. On GitHub only `report` and `record-push`
/// hold a write scope, and the `floor` job never reads the token. On GitLab
/// only `report` and `record-push` name the environment that holds
/// `HOUSERULES_TOKEN`, with `action: access`, and no other job names the
/// token anywhere.
pub fn assert_token_environment_on_write_jobs_only(unit: &Unit) {
    for role in ROLES {
        let writes = WRITE_ROLES.contains(&role);
        match &unit.host {
            Host::GitHub => {
                let holds_write = unit
                    .permissions(role)
                    .values()
                    .any(|access| access == "write");
                assert_eq!(
                    holds_write, writes,
                    "the {role} job holds a write scope only if it is report or record-push"
                );
                if role == "floor" {
                    let text = unit.job_text(role);
                    assert!(
                        !text.contains("github.token") && !text.contains("GH_TOKEN"),
                        "the floor job reads no token"
                    );
                }
            }
            Host::GitLab { key } => {
                let environment = unit.environment(role);
                let mentions_token = unit.job_text(role).contains("HOUSERULES_TOKEN");
                if writes {
                    assert_eq!(
                        environment,
                        Some((format!("houserules-{key}"), "access".to_string())),
                        "the {role} job names the token environment of its install"
                    );
                    assert!(mentions_token, "the {role} job reads HOUSERULES_TOKEN");
                } else {
                    assert_eq!(
                        environment, None,
                        "the {role} job names no environment, so it cannot read the token"
                    );
                    assert!(
                        !mentions_token,
                        "the {role} job never names HOUSERULES_TOKEN"
                    );
                }
            }
        }
    }
}

/// Spec 10.1: no unit sets a shell trace, and the write jobs unset the git
/// trace variables before their first git or curl call.
pub fn assert_no_trace_and_git_trace_unset_in_write_jobs(unit: &Unit) {
    for role in ROLES {
        for script in unit.scripts(role) {
            assert!(
                !traces_the_shell(&script.text),
                "the {role} job turns on a shell trace in {}",
                script.name
            );
        }
        let text = unit.job_text(role);
        for name in [
            "CI_DEBUG_TRACE",
            "ACTIONS_STEP_DEBUG",
            "ACTIONS_RUNNER_DEBUG",
        ] {
            assert!(!text.contains(name), "the {role} job sets {name}");
        }
    }
    for role in WRITE_ROLES {
        let unsets = unit.scripts(role).into_iter().any(|script| {
            let words = bare_words(&script.text);
            words.iter().enumerate().any(|(index, word)| {
                word == "unset"
                    && ["GIT_TRACE", "GIT_CURL_VERBOSE", "GIT_TRACE_CURL"]
                        .iter()
                        .all(|name| words[index + 1..].iter().take(3).any(|next| next == name))
            })
        });
        assert!(
            unsets,
            "the {role} job unsets GIT_TRACE, GIT_CURL_VERBOSE, and GIT_TRACE_CURL"
        );
    }
}

/// Spec 10.1: a GitHub write job registers the base64 form of the credential
/// with `::add-mask::` before the first use of it.
pub fn assert_github_credential_masked(unit: &Unit) {
    assert_eq!(unit.host, Host::GitHub, "the mask is a GitHub rule");
    let role = "record-push";
    let script = unit.job_script(role);
    let mask = script
        .find("::add-mask::")
        .unwrap_or_else(|| panic!("the {role} job registers no mask"));
    let first_use = script
        .find("extraheader")
        .unwrap_or_else(|| panic!("the {role} job sends no credential"));
    assert!(
        mask < first_use,
        "the {role} job masks the credential before the first use"
    );
}

/// Spec 10.1: `report` and `record-push` run no houserules step and hold no
/// checkout of the project: no `houserules` word outside a quote, no tool of the
/// project, no reference to the setup command, no checkout action, and on
/// GitLab `GIT_STRATEGY: none`.
pub fn assert_write_jobs_run_no_houserules_and_check_out_no_project(unit: &Unit) {
    for role in WRITE_ROLES {
        for script in unit.scripts(role) {
            assert!(
                !invokes(&script.text, "houserules"),
                "the {role} job runs houserules in {}",
                script.name
            );
            let words = bare_words(&script.text);
            for tool in [
                "cargo", "npm", "make", "mise", "pip", "python", "python3", "node", "bd",
            ] {
                assert!(
                    !words.iter().any(|word| word == tool),
                    "the {role} job runs the project tool {tool}"
                );
            }
            for (index, word) in words.iter().enumerate() {
                if word == "git" {
                    let next = words[index + 1..]
                        .iter()
                        .find(|candidate| !candidate.starts_with('-'));
                    assert!(
                        !matches!(
                            next.map(String::as_str),
                            Some("clone" | "checkout" | "worktree" | "switch" | "restore")
                        ),
                        "the {role} job checks out project files"
                    );
                }
            }
        }
        let text = unit.job_text(role);
        assert!(
            !text.contains("HOUSERULES_SETUP") && !text.contains("inputs.setup"),
            "the {role} job never reads the setup command"
        );
        match unit.host {
            Host::GitHub => assert!(
                !unit
                    .uses(role)
                    .iter()
                    .any(|uses| uses.starts_with("actions/checkout")),
                "the {role} job has no checkout step"
            ),
            Host::GitLab { .. } => assert_eq!(
                unit.variable(role, "GIT_STRATEGY").as_deref(),
                Some("none"),
                "the {role} job has GIT_STRATEGY none, so the runner checks out nothing"
            ),
        }
    }
}

/// Spec 10.1, design.md 242: `floor` reads no setup command, runs no tool step
/// (a step that uses an action other than the checkout), and takes no
/// `install` input, so it can never run a binary that the request built. Its
/// install script refuses `none` (`ci_units.rs` runs it).
pub fn assert_floor_takes_no_setup_no_tool_and_never_none(unit: &Unit) {
    let text = unit.job_text("floor");
    for forbidden in [
        "HOUSERULES_SETUP",
        "inputs.setup",
        "inputs.install",
        "HOUSERULES_TRACKER",
        "inputs.tracker",
    ] {
        assert!(!text.contains(forbidden), "the floor job names {forbidden}");
    }
    if unit.host == Host::GitHub {
        for uses in unit.uses("floor") {
            assert!(
                uses.starts_with("actions/checkout@"),
                "the floor job uses only the checkout action, not {uses}"
            );
        }
    }
    for script in unit.scripts("floor") {
        let words = bare_words(&script.text);
        for tool in [
            "npm", "make", "mise", "pip", "python", "python3", "node", "bd",
        ] {
            assert!(
                !words.iter().any(|word| word == tool),
                "the floor job runs the project tool {tool}"
            );
        }
    }
}

/// C6: no `run` script of a GitHub unit holds a `${{ }}` expression, so a value
/// of the request reaches a script through the environment, never as shell
/// text. On GitLab no script names a variable that holds text of the request.
pub fn assert_no_request_text_in_a_script_expression(unit: &Unit) {
    for role in ROLES {
        for script in unit.scripts(role) {
            match unit.host {
                Host::GitHub => assert!(
                    !script.text.contains("${{"),
                    "the {role} job holds an expression in the run script {}",
                    script.name
                ),
                Host::GitLab { .. } => {
                    for name in [
                        "CI_MERGE_REQUEST_DESCRIPTION",
                        "CI_MERGE_REQUEST_TITLE",
                        "CI_COMMIT_MESSAGE",
                        "CI_COMMIT_DESCRIPTION",
                        "CI_COMMIT_TITLE",
                    ] {
                        assert!(
                            !script.text.contains(name),
                            "the {role} job reads {name} of the request"
                        );
                    }
                }
            }
        }
    }
}

/// The path components of `path`, without empty and `.` parts.
fn components(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect()
}

/// `true` when `inner` is `outer` or lies below it.
fn is_below(inner: &str, outer: &str) -> bool {
    let (inner, outer) = (components(inner), components(outer));
    inner.starts_with(&outer)
}

/// The checkout step of the GitHub job `role` that reads the ref `ref_contains`, or `None` when the
/// job holds none. The step is found by its ref, because the tests read the shipped unit and the
/// shapes made from it alike.
fn checkout_step(unit: &Unit, role: &str, ref_contains: &str) -> Option<Value> {
    unit.job(role)
        .get("steps")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|step| {
            step.get("uses")
                .and_then(Value::as_str)
                .is_some_and(|uses| uses.starts_with("actions/checkout@"))
                && step["with"]["ref"]
                    .as_str()
                    .is_some_and(|value| value.contains(ref_contains))
        })
        .cloned()
}

/// The `path` of the checkout step of the GitHub job `role` that reads the
/// ref `ref_contains`, or `None` when the step sets no path.
///
/// # Panics
///
/// Panics when the job holds no such checkout step.
pub fn checkout_path(unit: &Unit, role: &str, ref_contains: &str) -> Option<String> {
    let step = checkout_step(unit, role, ref_contains)
        .unwrap_or_else(|| panic!("the job {role} checks out {ref_contains}"));
    step["with"]["path"].as_str().map(str::to_string)
}

/// Spec 10.1, design.md 242: no file of the request can change the program, the
/// toolchain, or the cargo configuration that builds the floor binary of
/// `floor-install: default-branch`. Cargo reads `.cargo/config.toml` in the
/// folders above the source it builds, and rustup reads `rust-toolchain.toml`
/// in the folders above its working directory, so the source of the default
/// branch and every folder above it lie outside the checkout of the request.
/// On GitHub the request goes into a folder of its own beside the default
/// branch, and the step that reads the request works in that folder. On GitLab
/// the default branch goes into a folder that `mktemp -d` makes. The build
/// itself works inside the source (`ci_units_shell/floor_build.rs` runs it).
pub fn assert_floor_builds_outside_the_request(unit: &Unit) {
    match &unit.host {
        Host::GitHub => {
            let request = checkout_path(unit, "floor", "pull_request.head.sha")
                .unwrap_or_else(|| panic!("the request is checked out into a folder of its own"));
            let default = checkout_path(unit, "floor", "default_branch")
                .unwrap_or_else(|| panic!("the default branch is checked out into a folder"));
            assert!(
                !components(&request).is_empty()
                    && !request.starts_with('/')
                    && !components(&request).contains(&".."),
                "the request goes into a folder below the workspace: {request:?}"
            );
            assert!(
                !components(&default).is_empty()
                    && !default.starts_with('/')
                    && !components(&default).contains(&".."),
                "the default branch goes into a folder below the workspace: {default:?}"
            );
            assert!(
                !is_below(&default, &request) && !is_below(&request, &default),
                "neither checkout lies inside the other: {request:?} and {default:?}"
            );
            let source = unit
                .step_variable("floor", "Install houserules", "HOUSERULES_DEFAULT_SRC")
                .unwrap_or_default();
            assert_eq!(
                source,
                format!("${{{{ github.workspace }}}}/{default}"),
                "the build reads the source of the default branch"
            );
            let step = unit.step("floor", "Compute the floor");
            assert_eq!(
                step["working-directory"].as_str(),
                Some(request.as_str()),
                "the step that reads the request works in the folder of the request"
            );
        }
        Host::GitLab { .. } => {
            let script = unit.job_script("floor");
            let assignment = script
                .lines()
                .map(str::trim)
                .find(|line| line.starts_with("HOUSERULES_DEFAULT_SRC="))
                .unwrap_or_else(|| panic!("the floor job names the source of the default branch"));
            assert!(
                assignment.starts_with("HOUSERULES_DEFAULT_SRC=$(mktemp -d"),
                "the source goes into a folder that mktemp makes: {assignment}"
            );
        }
    }
}

/// Spec 10.1, design.md 242: on GitHub the floor job reads the request in a folder of its own below
/// the workspace, and the step that reads it works in that folder. This is the part of
/// [`assert_github_floor_follows_its_install_source`] for a floor that installs the release.
pub fn assert_floor_reads_the_request_in_a_folder_of_its_own(unit: &Unit) {
    assert_eq!(
        unit.host,
        Host::GitHub,
        "the folder of the request is a GitHub rule"
    );
    let request = checkout_path(unit, "floor", "pull_request.head.sha")
        .unwrap_or_else(|| panic!("the request is checked out into a folder of its own"));
    assert!(
        !components(&request).is_empty()
            && !request.starts_with('/')
            && !components(&request).contains(&".."),
        "the request goes into a folder below the workspace: {request:?}"
    );
    let step = unit.step("floor", "Compute the floor");
    assert_eq!(
        step["working-directory"].as_str(),
        Some(request.as_str()),
        "the step that reads the request works in the folder of the request"
    );
}

/// Spec 10.1, design.md 242: the rule for the floor of a GitHub wiring follows the source that the
/// floor installs houserules from, which its `Install houserules` step names in `HOUSERULES_SOURCE`.
/// A floor that installs the `release` builds nothing, so it checks out no default branch and reads
/// the request in a folder of its own ([`assert_floor_reads_the_request_in_a_folder_of_its_own`]).
/// A floor with any other source can build from source, and so passes the whole rule
/// ([`assert_floor_builds_outside_the_request`]). A published unit takes the source from an input,
/// which is another source here, so it passes the whole rule.
///
/// # Panics
///
/// Panics when the floor job names no install source.
pub fn assert_github_floor_follows_its_install_source(unit: &Unit) {
    assert_eq!(
        unit.host,
        Host::GitHub,
        "the install source of the floor is read from a GitHub step"
    );
    let source = unit
        .step_variable("floor", "Install houserules", "HOUSERULES_SOURCE")
        .unwrap_or_else(|| panic!("the floor installs houserules from a HOUSERULES_SOURCE"));
    if source == "release" {
        assert!(
            checkout_step(unit, "floor", "default_branch").is_none(),
            "the floor installs the release and checks out no default branch"
        );
        assert_floor_reads_the_request_in_a_folder_of_its_own(unit);
    } else {
        assert_floor_builds_outside_the_request(unit);
    }
}

/// Spec 10.1: no keyword of the adopter's `default:` section runs a command,
/// starts a service, restores a cache, or hands out a token in a job of the
/// unit. GitLab copies every `default:` keyword into a job that does not set it
/// (and runs a `before_script` of the adopter with `HOUSERULES_TOKEN` in the
/// environment of `report` and `record-push`), so every job, after `extends`,
/// sets `inherit: default` to `false` or to a list of [`INHERITED_DEFAULTS`].
pub fn assert_gitlab_jobs_inherit_only_runner_defaults(unit: &Unit) {
    assert!(
        matches!(unit.host, Host::GitLab { .. }),
        "the inheritance of defaults is a GitLab rule"
    );
    for role in ROLES {
        let inherit = unit.job(role).get("inherit").cloned();
        let default = inherit.as_ref().and_then(|inherit| inherit.get("default"));
        match default {
            Some(Value::Bool(false)) => {}
            Some(Value::Array(keywords)) => {
                for keyword in keywords {
                    let keyword = keyword.as_str().unwrap_or_default();
                    assert!(
                        INHERITED_DEFAULTS.contains(&keyword),
                        "the {role} job inherits the default keyword {keyword:?} of the adopter's pipeline"
                    );
                }
            }
            other => panic!(
                "the {role} job inherits every default keyword of the adopter's pipeline: inherit.default is {other:?}"
            ),
        }
    }
}

/// `true` when `text` matches the regular expression `pattern` of an input.
fn matches_pattern(pattern: &str, text: &str) -> bool {
    regress::Regex::new(pattern)
        .unwrap_or_else(|error| panic!("the pattern {pattern} compiles: {error}"))
        .find(text)
        .is_some()
}

/// Design.md 258: every install path that the `dir` input accepts gives a key
/// that the `key` input accepts. A path may hold a dot or start with one, so the
/// key may start with a hyphen or an underscore.
pub fn assert_key_input_accepts_every_install_key(unit: &Unit) {
    assert!(
        matches!(unit.host, Host::GitLab { .. }),
        "only the GitLab unit validates the key"
    );
    let pattern_of = |name: &str| {
        unit.input(name)["regex"]
            .as_str()
            .unwrap_or_else(|| panic!("the input {name} has a regex"))
            .to_string()
    };
    let (dir, key) = (pattern_of("dir"), pattern_of("key"));
    for path in [
        ".",
        "./",
        "docs",
        "docs/",
        "./docs",
        "packages/app.v2",
        "docs.site",
        ".config/x",
        "./.config/x",
        "_internal/tools",
        "-odd",
        "a/b-c_d",
    ] {
        assert!(
            matches_pattern(&dir, path),
            "the dir input accepts the install path {path:?}"
        );
        let computed = install_key(path);
        assert!(
            matches_pattern(&key, &computed),
            "the key input accepts {computed:?}, the key of the install path {path:?}"
        );
    }
}

/// Acceptance case 8: `floor-install: runner:<path>` is among the inputs, and
/// the unit's inputs are the ones of spec 10.2.
pub fn assert_inputs(unit: &Unit) {
    let names: Vec<&str> = match unit.host {
        Host::GitHub => vec![
            "version",
            "dir",
            "key",
            "setup",
            "validation",
            "install",
            "floor-install",
            "tracker",
        ],
        Host::GitLab { .. } => vec![
            "dir",
            "key",
            "setup",
            "validation",
            "install",
            "floor-install",
            "tracker",
            "stage",
            "image",
            "token-environment",
        ],
    };
    let expected: BTreeSet<String> = names.iter().map(|name| name.to_string()).collect();
    assert_eq!(unit.input_names(), expected, "the inputs of the unit");
    let floor = unit.input("floor-install");
    let description = floor
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        description.contains("runner:<path>"),
        "floor-install names runner:<path> in its description: {description}"
    );
}

/// Every trust rule of spec 10.1 and case 8 that any wiring of the review layer must pass, on
/// either host: a published unit, and the inline form that `houserules ci print --mode inline`
/// prints and `ci write --mode inline` writes. The rules that read the inputs of a unit
/// ([`assert_inputs`], [`assert_key_input_accepts_every_install_key`]) are not among them,
/// because an inline form takes no input.
pub fn assert_wiring_trust_rules(unit: &Unit) {
    assert_job_set(unit);
    assert_token_environment_on_write_jobs_only(unit);
    assert_no_trace_and_git_trace_unset_in_write_jobs(unit);
    assert_write_jobs_run_no_houserules_and_check_out_no_project(unit);
    assert_floor_takes_no_setup_no_tool_and_never_none(unit);
    assert_no_request_text_in_a_script_expression(unit);
    match unit.host {
        Host::GitHub => {
            assert_github_floor_follows_its_install_source(unit);
            assert_github_permissions(unit);
            assert_github_credential_masked(unit);
        }
        Host::GitLab { .. } => {
            assert_floor_builds_outside_the_request(unit);
            assert_gitlab_jobs_inherit_only_runner_defaults(unit);
        }
    }
}

/// Every trust rule of spec 10.1 and case 8 that a published unit must pass, on either host: the
/// rules of [`assert_wiring_trust_rules`], which include the rule that the floor builds the default
/// branch outside the request, and the rules that read the inputs.
pub fn assert_trust_rules(unit: &Unit) {
    assert_inputs(unit);
    assert_wiring_trust_rules(unit);
    if matches!(unit.host, Host::GitLab { .. }) {
        assert_key_input_accepts_every_install_key(unit);
    }
}
