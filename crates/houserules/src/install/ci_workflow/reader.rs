//! The reader of a kept copy of the seeded CI workflow (spec 10.7, design.md
//! 226).
//!
//! `update` keeps a copy that the project changed. This module parses the copy
//! as YAML with `saphyr` and judges each gates job by its own keys, whatever
//! the name and the indentation of the job. A gates job is a job with a step
//! whose `run` holds `houserules check-knowledge`. A marker that another job
//! holds, or a comment holds, is no marker of the gates job. houserules writes
//! no YAML into the copy: the hand steps tell the project what to change.
//!
//! A copy that the parser rejects, and a copy with no gates job, each get one
//! hand step and no other: the markers belong to a gates job.

use saphyr::{LoadableYamlNode, Scalar, Yaml};

use crate::install::hand_steps;
use crate::tracker::Adapter;
use crate::tracker::beads::BD_VERSION;

/// The command whose step marks a job as a gates job.
const GATES_COMMAND: &str = "houserules check-knowledge";

/// The start of the `uses` value of a checkout step.
const CHECKOUT_ACTION: &str = "actions/checkout@";

/// The step of the Beads form that bootstraps the store.
const BEADS_BOOTSTRAP: &str = "bd bootstrap";

/// The value of the `ref` input of the checkout step that makes every step of
/// the job judge the head of a pull request, without spaces.
const HEAD_REF: &str = "${{github.event.pull_request.head.sha}}";

/// The value of the `GH_TOKEN` env of a step of the GitHub form, without
/// spaces.
const GITHUB_TOKEN: &str = "${{github.token}}";

/// The scope of the token that reads the code.
const CONTENTS: &str = "contents";

/// The scope of the token that reads the issues, which the GitHub tracker needs.
const ISSUES: &str = "issues";

/// What a gates job holds under its `permissions` key.
#[derive(Debug)]
enum JobPermissions {
    /// The job has no `permissions` key.
    Absent,
    /// The scalar `read-all`, which holds every read scope.
    ReadAll,
    /// The scalar `write-all`, which holds every scope.
    WriteAll,
    /// A mapping of scopes, in block or flow style. An empty value is an empty
    /// mapping.
    Mapping {
        /// The value of the `contents` scope.
        contents: Scope,
        /// The value of the `issues` scope.
        issues: Scope,
    },
}

/// The value of one scope of a `permissions` mapping.
#[derive(Debug)]
enum Scope {
    /// The mapping holds no such scope.
    Absent,
    /// The scope is `read`.
    Read,
    /// The scope holds another value, shown as the hand step shows it
    /// (`value_text`).
    Other(String),
}

/// The facts of one gates job that the hand steps depend on.
struct GatesJob {
    /// The key of the job under `jobs`.
    name: String,
    /// What the job holds under `permissions`.
    permissions: JobPermissions,
    /// `true` when a step runs `bd bootstrap`.
    bootstraps_beads: bool,
    /// `true` when the job or a step of it sets `GH_TOKEN` to the token of the
    /// workflow.
    sets_the_github_token: bool,
    /// `true` when a checkout step has the `ref` input of the pull request head.
    checks_out_the_head: bool,
    /// `true` when a checkout step has `persist-credentials: false`.
    drops_the_credential: bool,
}

/// `text` without its spaces.
fn squeezed(text: &str) -> String {
    text.split_whitespace().collect()
}

/// The steps of the job `job`.
fn steps<'a, 'input>(job: &'a Yaml<'input>) -> impl Iterator<Item = &'a Yaml<'input>> {
    job.as_mapping_get("steps")
        .and_then(Yaml::as_sequence)
        .into_iter()
        .flatten()
}

/// `true` when the `run` of `step` holds `needle`.
fn step_runs(step: &Yaml<'_>, needle: &str) -> bool {
    step.as_mapping_get("run")
        .and_then(Yaml::as_str)
        .is_some_and(|run| run.contains(needle))
}

/// `true` when `step` uses the checkout action.
fn is_checkout(step: &Yaml<'_>) -> bool {
    step.as_mapping_get("uses")
        .and_then(Yaml::as_str)
        .is_some_and(|uses| uses.starts_with(CHECKOUT_ACTION))
}

/// The input `name` under `with:` of `step`.
fn input<'a, 'input>(step: &'a Yaml<'input>, name: &str) -> Option<&'a Yaml<'input>> {
    step.as_mapping_get("with")?.as_mapping_get(name)
}

/// `true` when `node`, a job or a step, sets `GH_TOKEN` to the token of the
/// workflow.
fn sets_the_token(node: &Yaml<'_>) -> bool {
    node.as_mapping_get("env")
        .and_then(|env| env.as_mapping_get("GH_TOKEN"))
        .and_then(Yaml::as_str)
        .is_some_and(|value| squeezed(value) == GITHUB_TOKEN)
}

/// `true` when `node` is the boolean `false`, or the text `false`.
fn is_false(node: &Yaml<'_>) -> bool {
    node.as_bool() == Some(false) || node.as_str() == Some("false")
}

/// `node` as a hand step shows the value of a scope: the text of a scalar (a
/// missing value is `null`), else `a sequence`, `a mapping`, or `another value`.
fn value_text(node: &Yaml<'_>) -> String {
    match node {
        Yaml::Value(Scalar::String(text)) => text.to_string(),
        Yaml::Value(Scalar::Null) => "null".to_string(),
        Yaml::Value(Scalar::Boolean(flag)) => flag.to_string(),
        Yaml::Value(Scalar::Integer(number)) => number.to_string(),
        Yaml::Value(Scalar::FloatingPoint(number)) => number.to_string(),
        Yaml::Sequence(_) => "a sequence".to_string(),
        Yaml::Mapping(_) => "a mapping".to_string(),
        _ => "another value".to_string(),
    }
}

/// The value of the scope `name` of the `permissions` mapping `mapping`.
fn scope(mapping: &Yaml<'_>, name: &str) -> Scope {
    let Some(node) = mapping.as_mapping_get(name) else {
        return Scope::Absent;
    };
    if node.as_str() == Some("read") {
        Scope::Read
    } else {
        Scope::Other(value_text(node))
    }
}

/// What `job` holds under `permissions`.
fn permissions_of(job: &Yaml<'_>) -> JobPermissions {
    let Some(node) = job.as_mapping_get("permissions") else {
        return JobPermissions::Absent;
    };
    match node.as_str() {
        Some("read-all") => JobPermissions::ReadAll,
        Some("write-all") => JobPermissions::WriteAll,
        _ => JobPermissions::Mapping {
            contents: scope(node, CONTENTS),
            issues: scope(node, ISSUES),
        },
    }
}

/// The facts of the job `name`, or `None` when no step of it runs
/// `houserules check-knowledge`.
fn read_gates_job(name: &str, job: &Yaml<'_>) -> Option<GatesJob> {
    if !steps(job).any(|step| step_runs(step, GATES_COMMAND)) {
        return None;
    }
    let checkouts: Vec<&Yaml<'_>> = steps(job).filter(|step| is_checkout(step)).collect();
    Some(GatesJob {
        name: name.to_string(),
        permissions: permissions_of(job),
        bootstraps_beads: steps(job).any(|step| step_runs(step, BEADS_BOOTSTRAP)),
        sets_the_github_token: sets_the_token(job) || steps(job).any(sets_the_token),
        checks_out_the_head: checkouts.iter().any(|checkout| {
            input(checkout, "ref")
                .and_then(Yaml::as_str)
                .is_some_and(|value| squeezed(value) == HEAD_REF)
        }),
        drops_the_credential: checkouts
            .iter()
            .any(|checkout| input(checkout, "persist-credentials").is_some_and(is_false)),
    })
}

/// Every gates job of the workflow `document`, in the order of the file.
fn gates_jobs(document: &Yaml<'_>) -> Vec<GatesJob> {
    document
        .as_mapping_get("jobs")
        .and_then(Yaml::as_mapping)
        .into_iter()
        .flatten()
        .filter_map(|(name, job)| read_gates_job(name.as_str()?, job))
        .collect()
}

/// The hand step for the scope `name` of the gates job `job` at `path` that
/// the mapping holds as `held`, or `None` when the scope is `read`. `add` is the
/// step for a scope that the mapping lacks. A scope with another value than
/// `read` gets that value replaced, and never a second key: `contents: write`
/// has its own step, and every other value is named by `ci_sets_a_scope`.
fn scope_step(
    path: &str,
    job: &GatesJob,
    name: &str,
    held: &Scope,
    add: fn(&str) -> String,
) -> Option<String> {
    match held {
        Scope::Read => None,
        Scope::Absent => Some(add(path)),
        Scope::Other(value) if name == CONTENTS && value == "write" => {
            Some(hand_steps::ci_grants_contents_write(path, &job.name))
        }
        Scope::Other(value) => Some(hand_steps::ci_sets_a_scope(path, &job.name, name, value)),
    }
}

/// The hand steps for the permissions of the gates job `job` at `path`. The
/// `permissions` key of the job decides them, so that no copy is asked for a
/// second key:
///
/// - `read-all` holds every read scope and gets none.
/// - `write-all` is replaced by the block of the form.
/// - A mapping gets the scopes it holds no key for added to it: `contents:
///   read` for every tracker but GitHub, which reads `contents: read` and
///   `issues: read`. A scope it holds with another value than `read` gets that
///   value replaced (`scope_step`).
/// - A job with no key gets the block: `permissions: {contents: read}`, and
///   `{contents: read, issues: read}` for the GitHub tracker.
fn permission_steps(github: bool, path: &str, job: &GatesJob) -> Vec<String> {
    match &job.permissions {
        JobPermissions::ReadAll => Vec::new(),
        JobPermissions::WriteAll => vec![hand_steps::ci_grants_write_all(path, github)],
        JobPermissions::Absent if github => vec![hand_steps::ci_lacks_the_github_permissions(path)],
        JobPermissions::Absent => vec![hand_steps::ci_lacks_contents_read(path)],
        JobPermissions::Mapping { contents, issues } => {
            if github && matches!((contents, issues), (Scope::Absent, Scope::Absent)) {
                return vec![hand_steps::ci_lacks_the_github_scopes_in_the_permissions(
                    path,
                )];
            }
            let contents = scope_step(
                path,
                job,
                CONTENTS,
                contents,
                hand_steps::ci_lacks_contents_read_in_the_permissions,
            );
            let issues = github
                .then(|| scope_step(path, job, ISSUES, issues, hand_steps::ci_lacks_issues_read))
                .flatten();
            contents.into_iter().chain(issues).collect()
        }
    }
}

/// The hand steps for one gates job, in the order of the file:
///
/// 1. The steps of the Beads tracker: a step that runs `bd bootstrap`.
/// 2. The permissions of the job (`permission_steps`).
/// 3. The `GH_TOKEN` env of the GitHub tracker.
/// 4. The `ref` input of the checkout (the head of a pull request).
/// 5. The `persist-credentials` input of the checkout: the step adds it for
///    every tracker but Beads, and the Beads tracker gets the step that removes
///    it from a job that holds it.
fn job_steps(adapter: Option<Adapter>, path: &str, job: &GatesJob) -> Vec<String> {
    let github = adapter == Some(Adapter::Github);
    let beads = adapter == Some(Adapter::Beads);
    let mut steps = Vec::new();
    if beads && !job.bootstraps_beads {
        steps.push(hand_steps::ci_lacks_the_beads_steps(path, BD_VERSION));
    }
    steps.extend(permission_steps(github, path, job));
    if github && !job.sets_the_github_token {
        steps.push(hand_steps::ci_lacks_the_github_env(path));
    }
    if !job.checks_out_the_head {
        steps.push(hand_steps::ci_lacks_the_head_checkout(path));
    }
    if beads && job.drops_the_credential {
        steps.push(hand_steps::ci_holds_persist_credentials_false(path));
    } else if !beads && !job.drops_the_credential {
        steps.push(hand_steps::ci_lacks_persisted_credentials_off(path));
    }
    steps
}

/// The hand steps for a seeded CI workflow that the project changed and that
/// `update` kept. The file at `path` holds the bytes `current`. A copy that is
/// no YAML gets one step, and so does a copy with no gates job. Otherwise each
/// gates job gets its steps (`job_steps`), and a step that two jobs share
/// shows once.
pub(in crate::install) fn hand_step_for_kept(
    adapter: Option<Adapter>,
    path: &str,
    current: &[u8],
) -> Vec<String> {
    let text = String::from_utf8_lossy(current);
    let documents = match Yaml::load_from_str(&text) {
        Ok(documents) => documents,
        Err(error) => return vec![hand_steps::ci_is_not_valid_yaml(path, &error.to_string())],
    };
    let jobs = documents.first().map(gates_jobs).unwrap_or_default();
    if jobs.is_empty() {
        return vec![hand_steps::ci_has_no_gates_job(path)];
    }
    let mut steps: Vec<String> = Vec::new();
    for job in &jobs {
        for step in job_steps(adapter, path, job) {
            if !steps.contains(&step) {
                steps.push(step);
            }
        }
    }
    steps
}
