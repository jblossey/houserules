//! This repository's own wiring of the review layer (spec 9.6 and 10.8, and the
//! preparation of acceptance case 13): `.github/workflows/review.yml` calls the
//! published unit `.github/workflows/houserules-review.yml` of this repository,
//! `.github/workflows/schemas.yml` publishes `schemas/` to GitHub Pages,
//! `ci.yml` runs `gen-schemas --check`, and the request template holds the
//! bundle section.
//!
//! The tests read each workflow with `saphyr` (`ci_support::parse_unit`). A test
//! that pins "this and nothing else" compares the whole structure and proves each
//! forbidden shape with a mutation of the parsed workflow
//! (`quality.exclusive-pins-are-mutation-proven`). The `setup` command and the
//! staging step of the Pages workflow run for real, on Unix, in a sandbox with
//! stub programs on the `PATH` (`process.wiring-checks-run-the-resolution`): the
//! value a workflow produces goes to the program that consumes it, and a match
//! on the text of the workflow is the first gate only.
//!
//! `actionlint` and `shellcheck` come from `mise.toml` (`mise install`); a test
//! fails, and does not skip, when one is missing.

mod ci_support;
mod tracker_support;

use std::collections::BTreeMap;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use ci_support::{Host, ROLES, Unit, parse_unit};
use serde_json::{Value, json};
use tracker_support::{assert_actionlint_passes, repo_root};

/// This repository's caller of the review unit.
const REVIEW: &str = ".github/workflows/review.yml";

/// The workflow that publishes the schemas.
const SCHEMAS: &str = ".github/workflows/schemas.yml";

/// The workflow of the CI checks.
const CI: &str = ".github/workflows/ci.yml";

/// The published GitHub unit that `review.yml` calls.
const UNIT: &str = ".github/workflows/houserules-review.yml";

/// The pull request template of this repository.
const TEMPLATE: &str = ".github/PULL_REQUEST_TEMPLATE.md";

/// The spec that gives the text of the request template (section 7.4).
const SPEC: &str = "docs/specs/2026-10-24-batch-37-per-change-review-layer.md";

/// The path of `relative` below the repository root, built component by
/// component.
fn path_of(relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(repo_root(), |path, part| path.join(part))
}

/// The text of the file at `relative`.
fn text_of(relative: &str) -> String {
    fs::read_to_string(path_of(relative)).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The workflow at `relative`, parsed as a GitHub workflow.
fn workflow(relative: &str) -> Unit {
    parse_unit(&text_of(relative), Host::GitHub)
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

/// The rank of a permission level.
fn rank(level: &str) -> u8 {
    match level {
        "none" => 0,
        "read" => 1,
        "write" => 2,
        other => panic!("unknown permission level {other}"),
    }
}

/// The union of the `permissions` of the five jobs of the published unit: for
/// each scope, the highest level that any job holds.
fn union_of_the_unit_permissions() -> BTreeMap<String, String> {
    let unit = workflow(UNIT);
    let mut union: BTreeMap<String, String> = BTreeMap::new();
    for role in ROLES {
        for (scope, level) in unit.permissions(role) {
            let held = union.entry(scope).or_insert_with(|| level.clone());
            if rank(&level) > rank(held) {
                *held = level;
            }
        }
    }
    union
}

/// The version that `mise.toml` pins for `rust`.
#[cfg(unix)]
fn rust_pin() -> String {
    let toml = text_of("mise.toml");
    let line = toml
        .lines()
        .find(|line| line.starts_with("rust = "))
        .expect("mise.toml pins rust");
    let (_, after) = line
        .split_once("version = \"")
        .expect("the rust pin names a version");
    let (version, _) = after.split_once('"').expect("the version is quoted");
    version.to_string()
}

/// The `setup` input that `review.yml` passes to the unit.
#[cfg(unix)]
fn setup_command() -> String {
    workflow(REVIEW).job("review")["with"]["setup"]
        .as_str()
        .expect("review.yml passes a setup command")
        .to_string()
}

/// The script of the step of the `gates` job that runs the setup command: the unit hands the
/// command to it in the variable `HOUSERULES_SETUP`.
#[cfg(unix)]
fn setup_step_script() -> String {
    workflow(UNIT).step_script("gates", "Run setup")
}

// ---- review.yml ----

/// Spec 9.6 and 10.8: actionlint passes on `review.yml` and on `schemas.yml`.
#[test]
fn the_review_and_schemas_workflows_pass_actionlint() {
    assert_actionlint_passes(&path_of(REVIEW));
    assert_actionlint_passes(&path_of(SCHEMAS));
}

/// Spec 10.8: `review.yml` holds one job, which calls the unit of this
/// repository by path with `install: none`, a `setup`, and `floor-install:
/// default-branch`, and passes no `version`, because no job installs `release`.
/// Every input it passes is one the unit declares.
fn assert_review_calls_the_unit(unit: &Unit) {
    assert_eq!(unit.job_keys(), vec!["review"], "one job calls the unit");
    let job = unit.job("review");
    assert_eq!(job["uses"], "./.github/workflows/houserules-review.yml");
    let with = job["with"].as_object().expect("the call passes inputs");
    let mut names: Vec<&str> = with.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["floor-install", "install", "setup"],
        "the call passes these inputs and no other (no version: no job installs release)"
    );
    assert_eq!(with["install"], "none");
    assert_eq!(with["floor-install"], "default-branch");
    assert!(
        with["setup"].as_str().is_some_and(|text| !text.is_empty()),
        "setup builds the tree binary"
    );
    let declared = workflow(UNIT).input_names();
    for name in with.keys() {
        assert!(declared.contains(name), "the unit declares no input {name}");
    }
}

/// Spec 10.8: the call of `review.yml`, with a mutation for each forbidden shape.
#[test]
fn review_calls_the_unit_with_the_tree_binary_and_the_default_branch_floor() {
    assert!(path_of(UNIT).is_file(), "the called unit exists");
    let unit = workflow(REVIEW);
    assert_review_calls_the_unit(&unit);
    assert_fails_on(
        &unit,
        "a version input",
        |body| body["jobs"]["review"]["with"]["version"] = json!("2.0.0"),
        assert_review_calls_the_unit,
    );
    assert_fails_on(
        &unit,
        "install: release",
        |body| body["jobs"]["review"]["with"]["install"] = json!("release"),
        assert_review_calls_the_unit,
    );
    assert_fails_on(
        &unit,
        "floor-install: release",
        |body| body["jobs"]["review"]["with"]["floor-install"] = json!("release"),
        assert_review_calls_the_unit,
    );
    assert_fails_on(
        &unit,
        "a call of the unit through a remote reference",
        |body| {
            body["jobs"]["review"]["uses"] =
                json!("jblossey/houserules/.github/workflows/houserules-review.yml@main");
        },
        assert_review_calls_the_unit,
    );
    assert_fails_on(
        &unit,
        "an input that the unit does not declare",
        |body| body["jobs"]["review"]["with"]["tracker"] = json!("json"),
        assert_review_calls_the_unit,
    );
}

/// Spec 10.8: the caller job grants exactly the union of the permissions of the
/// jobs of the unit (spec 10.2). A job of the unit cannot hold more than its
/// caller grants, and the caller grants nothing the unit does not use.
fn assert_review_grants_the_union(unit: &Unit) {
    assert_eq!(
        unit.permissions("review"),
        union_of_the_unit_permissions(),
        "the caller job grants the union of the permissions of the unit jobs"
    );
}

/// Spec 10.8 and 10.2: the permissions of the caller job, with a mutation for a
/// missing scope, a lower level, and an extra scope.
#[test]
fn review_grants_the_union_of_the_permissions_of_the_unit_jobs() {
    let union = union_of_the_unit_permissions();
    assert_eq!(
        union.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["contents", "issues", "pull-requests", "statuses"],
        "the unit jobs use these four scopes"
    );
    let unit = workflow(REVIEW);
    assert_review_grants_the_union(&unit);
    assert_fails_on(
        &unit,
        "a missing scope",
        |body| {
            body["jobs"]["review"]["permissions"]
                .as_object_mut()
                .expect("a permissions mapping")
                .remove("statuses");
        },
        assert_review_grants_the_union,
    );
    assert_fails_on(
        &unit,
        "a lower level",
        |body| body["jobs"]["review"]["permissions"]["issues"] = json!("read"),
        assert_review_grants_the_union,
    );
    assert_fails_on(
        &unit,
        "an extra scope",
        |body| body["jobs"]["review"]["permissions"]["actions"] = json!("read"),
        assert_review_grants_the_union,
    );
}

/// Spec 10.2 and 10.8: `review.yml` triggers on `pull_request` with `edited` and
/// on `push` to `main`, and on nothing else (the unit runs the code of the
/// request, so never `pull_request_target`); the token reads by default.
fn assert_review_triggers(unit: &Unit) {
    assert_eq!(
        unit.top_level("on"),
        Some(&json!({
            "pull_request": {"types": ["opened", "synchronize", "reopened", "edited"]},
            "push": {"branches": ["main"]},
        })),
    );
    assert_eq!(
        unit.top_level("permissions"),
        Some(&json!({"contents": "read"})),
        "a job without its own permissions reads only"
    );
}

/// The triggers of `review.yml`, with a mutation for each forbidden shape.
#[test]
fn review_triggers_on_edited_pull_requests_and_pushes_to_main() {
    let unit = workflow(REVIEW);
    assert_review_triggers(&unit);
    assert_fails_on(
        &unit,
        "pull_request_target",
        |body| body["on"]["pull_request_target"] = json!({"types": ["opened"]}),
        assert_review_triggers,
    );
    assert_fails_on(
        &unit,
        "no edited type",
        |body| body["on"]["pull_request"]["types"] = json!(["opened", "synchronize", "reopened"]),
        assert_review_triggers,
    );
    assert_fails_on(
        &unit,
        "a push to every branch",
        |body| body["on"]["push"] = json!({"branches": ["**"]}),
        assert_review_triggers,
    );
    assert_fails_on(
        &unit,
        "a write default for every job",
        |body| body["permissions"] = json!({"contents": "write"}),
        assert_review_triggers,
    );
}

// ---- the setup command ----

/// The `setup` command is shell text that the unit evaluates in a `bash` step,
/// so `actionlint` does not read it: it passes `shellcheck` as bash.
#[cfg(unix)]
#[test]
fn the_setup_command_passes_shellcheck() {
    let directory = tempfile::tempdir().expect("create a scratch directory");
    let file = directory.path().join("setup.sh");
    fs::write(&file, setup_command()).expect("write the setup command");
    let output = std::process::Command::new("shellcheck")
        .args(["-s", "bash"])
        .arg(&file)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "shellcheck is not installed ({error}): run mise install, which pins it in mise.toml"
            )
        });
    assert!(
        output.status.success(),
        "shellcheck rejects the setup command:\n{}",
        tracker_support::stdout(&output)
    );
}

#[cfg(unix)]
mod setup {
    //! The `setup` command, run by the step of the unit that evaluates it, as the `bash` step
    //! runs it, with a stub `rustup` on the `PATH` that records every call.

    use std::process::Output;

    use super::*;
    use crate::ci_support::sandbox::Sandbox;
    use crate::tracker_support::{stderr, stdout};

    /// A stub `rustup`: it logs its arguments. `rustup run <toolchain> rustc` fails while the
    /// file `$STUB_DIR/missing` exists and no `rustup toolchain` call has made the file
    /// `$STUB_DIR/installed`.
    const STUB_RUSTUP: &str = r#"#!/bin/sh
printf 'rustup %s\n' "$*" >> "$STUB_LOG"
case $1 in
  toolchain) : > "$STUB_DIR/installed" ;;
  run)
    if [ "$3" = rustc ] && [ -e "$STUB_DIR/missing" ] && [ ! -e "$STUB_DIR/installed" ]; then
      exit 1
    fi
    ;;
esac
"#;

    /// A sandbox whose work directory holds the `mise.toml` of this repository and whose
    /// `PATH` starts with the stub `rustup`.
    fn sandbox() -> Sandbox {
        let sandbox = Sandbox::new();
        sandbox.install_program("rustup", STUB_RUSTUP);
        fs::write(sandbox.work().join("mise.toml"), text_of("mise.toml")).expect("copy mise.toml");
        sandbox
    }

    /// Runs the step of the unit that evaluates the setup command, with the command in
    /// `HOUSERULES_SETUP`, in `sandbox`, with `GITHUB_PATH` set to the file `github-path`, and
    /// `extra` as more environment.
    fn run_setup(sandbox: &Sandbox, extra: &[(&str, &str)]) -> Output {
        let github_path = sandbox.path().join("github-path");
        let mut environment: Vec<(String, String)> = vec![
            (
                "GITHUB_PATH".to_string(),
                github_path.to_string_lossy().into_owned(),
            ),
            ("HOUSERULES_SETUP".to_string(), setup_command()),
        ];
        environment.extend(
            extra
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string())),
        );
        sandbox.run_bash(&setup_step_script(), environment)
    }

    /// The calls of the stub `rustup`, in order.
    fn rustup_calls(sandbox: &Sandbox) -> Vec<String> {
        sandbox
            .calls()
            .into_iter()
            .filter(|call| call.starts_with("rustup "))
            .collect()
    }

    /// Spec 10.8: the toolchain comes from `mise.toml` at run time and is never
    /// typed a second time. A toolchain that is installed is not installed again, so the
    /// build needs no download of it. The build is `cargo install --locked --path
    /// crates/houserules --bin houserules` under that toolchain, and its `bin` directory goes
    /// to the `PATH` of the later steps.
    #[test]
    fn the_setup_command_builds_under_the_toolchain_that_mise_toml_pins() {
        let pin = rust_pin();
        assert!(
            !setup_command().contains(&pin),
            "the setup command types the pin {pin} a second time"
        );
        let sandbox = sandbox();
        let output = run_setup(&sandbox, &[]);
        assert!(
            output.status.success(),
            "setup failed:\n{}{}",
            stdout(&output),
            stderr(&output)
        );
        let root = sandbox.temp().join("houserules-tree");
        assert_eq!(
            rustup_calls(&sandbox),
            vec![
                format!("rustup run {pin} rustc --version"),
                format!(
                    "rustup run {pin} cargo install --locked --path crates/houserules --bin houserules --root {}",
                    root.display()
                ),
            ]
        );
        assert_eq!(
            fs::read_to_string(sandbox.path().join("github-path")).expect("read GITHUB_PATH"),
            format!("{}\n", root.join("bin").display()),
            "the later steps find the binary on the PATH"
        );
    }

    /// A toolchain that the runner lacks is installed first, pinned by its version and with
    /// the minimal profile, before the build.
    #[test]
    fn the_setup_command_installs_a_toolchain_that_the_runner_lacks() {
        let pin = rust_pin();
        let sandbox = sandbox();
        fs::write(sandbox.stub_dir().join("missing"), "").expect("mark the toolchain missing");
        let output = run_setup(&sandbox, &[]);
        assert!(
            output.status.success(),
            "setup failed:\n{}{}",
            stdout(&output),
            stderr(&output)
        );
        let calls = rustup_calls(&sandbox);
        assert_eq!(calls.len(), 3, "{calls:?}");
        assert_eq!(calls[0], format!("rustup run {pin} rustc --version"));
        assert_eq!(
            calls[1],
            format!("rustup toolchain install {pin} --profile minimal")
        );
        assert!(
            calls[2].starts_with(&format!("rustup run {pin} cargo install --locked ")),
            "{calls:?}"
        );
    }

    /// `CARGO_INSTALL_ROOT` names the install root when it is set, as it does for cargo.
    #[test]
    fn the_setup_command_installs_into_the_cargo_install_root_when_it_is_set() {
        let sandbox = sandbox();
        let root = sandbox.path().join("scratch-root");
        let root_text = root.to_string_lossy().into_owned();
        let output = run_setup(&sandbox, &[("CARGO_INSTALL_ROOT", root_text.as_str())]);
        assert!(
            output.status.success(),
            "setup failed:\n{}{}",
            stdout(&output),
            stderr(&output)
        );
        assert!(
            rustup_calls(&sandbox)
                .last()
                .is_some_and(|call| call.ends_with(&format!(" --root {}", root.display()))),
            "{:?}",
            rustup_calls(&sandbox)
        );
        assert_eq!(
            fs::read_to_string(sandbox.path().join("github-path")).expect("read GITHUB_PATH"),
            format!("{}\n", root.join("bin").display())
        );
    }

    /// A `mise.toml` that pins no `rust` stops the setup with one line and builds nothing.
    #[test]
    fn the_setup_command_fails_when_mise_toml_pins_no_rust() {
        let sandbox = sandbox();
        fs::write(
            sandbox.work().join("mise.toml"),
            "[tools]\nshellcheck = \"0.11.0\"\n",
        )
        .expect("replace mise.toml");
        let output = run_setup(&sandbox, &[]);
        assert!(!output.status.success(), "setup must fail");
        assert!(
            stdout(&output).contains("::error::houserules: mise.toml pins no rust version"),
            "{}{}",
            stdout(&output),
            stderr(&output)
        );
        assert_eq!(rustup_calls(&sandbox), Vec::<String>::new());
    }

    /// Spec 10.8: with the real `rustup` and `cargo` of this host, in the user's
    /// `CARGO_HOME` with no network, the setup command puts a `houserules` whose `--version`
    /// is the crate's in `<root>/bin`, which the `PATH` of the child names. The test builds
    /// the crate a second time, so a plain `cargo test` leaves it out; run it with
    /// `--ignored`. The toolchain that `mise.toml` pins must be installed already.
    #[test]
    #[ignore = "builds the crate a second time; run with --ignored"]
    fn the_setup_command_installs_a_houserules_with_the_version_of_the_crate() {
        let scratch = tempfile::tempdir().expect("create a scratch directory");
        let root = scratch.path().join("root");
        let search_path = std::env::join_paths(std::iter::once(root.join("bin")).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .expect("join the PATH");
        let output = std::process::Command::new("bash")
            .args(["--noprofile", "--norc", "-eo", "pipefail", "-c"])
            .arg(setup_step_script())
            .current_dir(repo_root())
            .env("HOUSERULES_SETUP", setup_command())
            .env("PATH", &search_path)
            .env("CARGO_INSTALL_ROOT", &root)
            .env("CARGO_TARGET_DIR", scratch.path().join("target"))
            .env("CARGO_NET_OFFLINE", "true")
            .env("GITHUB_PATH", scratch.path().join("github-path"))
            .output()
            .expect("run the setup command");
        assert!(
            output.status.success(),
            "setup failed:\n{}{}",
            stdout(&output),
            stderr(&output)
        );
        let version = std::process::Command::new(root.join("bin").join("houserules"))
            .arg("--version")
            .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
            .output()
            .expect("run the installed houserules");
        assert!(version.status.success(), "{}", stderr(&version));
        assert_eq!(
            stdout(&version).trim(),
            format!("houserules {}", env!("CARGO_PKG_VERSION"))
        );
        assert!(
            root.join("bin").join("houserules").is_file(),
            "the binary sits in <root>/bin"
        );
    }
}

// ---- schemas.yml ----

/// Spec 9.6: `schemas.yml` runs on a push to `main` that changes `schemas/**` and on
/// `workflow_dispatch`, and its one job holds `pages: write` and `id-token: write`, with
/// `contents: read` for the checkout, and nothing else.
fn assert_schemas_triggers_and_permissions(unit: &Unit) {
    assert_eq!(
        unit.top_level("on"),
        Some(&json!({
            "push": {"branches": ["main"], "paths": ["schemas/**"]},
            "workflow_dispatch": null,
        })),
    );
    assert_eq!(unit.job_keys(), vec!["deploy"], "one job deploys the site");
    assert_eq!(
        unit.permissions("deploy"),
        BTreeMap::from([
            ("contents".to_string(), "read".to_string()),
            ("id-token".to_string(), "write".to_string()),
            ("pages".to_string(), "write".to_string()),
        ])
    );
}

/// The triggers and permissions of `schemas.yml`, with a mutation for each forbidden shape.
#[test]
fn schemas_deploys_on_a_push_to_main_that_changes_schemas_and_on_dispatch() {
    let unit = workflow(SCHEMAS);
    assert_schemas_triggers_and_permissions(&unit);
    assert_fails_on(
        &unit,
        "a push that changes any path",
        |body| body["on"]["push"] = json!({"branches": ["main"]}),
        assert_schemas_triggers_and_permissions,
    );
    assert_fails_on(
        &unit,
        "a deploy from a pull request",
        |body| body["on"]["pull_request"] = json!({"paths": ["schemas/**"]}),
        assert_schemas_triggers_and_permissions,
    );
    assert_fails_on(
        &unit,
        "no manual run",
        |body| {
            body["on"]
                .as_object_mut()
                .expect("a trigger mapping")
                .remove("workflow_dispatch");
        },
        assert_schemas_triggers_and_permissions,
    );
    assert_fails_on(
        &unit,
        "no id-token scope",
        |body| {
            body["jobs"]["deploy"]["permissions"]
                .as_object_mut()
                .expect("a permissions mapping")
                .remove("id-token");
        },
        assert_schemas_triggers_and_permissions,
    );
    assert_fails_on(
        &unit,
        "an extra write scope",
        |body| body["jobs"]["deploy"]["permissions"]["contents"] = json!("write"),
        assert_schemas_triggers_and_permissions,
    );
}

/// The steps of the deploy job, in order: the staging script, the upload of the site, and the
/// deployment, which targets the `github-pages` environment and reads the site's URL from
/// its own step.
#[test]
fn schemas_stages_uploads_and_deploys_in_order_into_the_pages_environment() {
    let unit = workflow(SCHEMAS);
    let job = unit.job("deploy");
    assert_eq!(
        job["environment"],
        json!({
            "name": "github-pages",
            "url": "${{ steps.deployment.outputs.page_url }}",
        })
    );
    let steps = job["steps"].as_array().expect("steps");
    let actions: Vec<&str> = steps
        .iter()
        .filter_map(|step| step["uses"].as_str())
        .map(|uses| uses.split('@').next().expect("an action name"))
        .collect();
    assert_eq!(
        actions,
        vec![
            "actions/checkout",
            "actions/upload-pages-artifact",
            "actions/deploy-pages"
        ]
    );
    let position = |needle: &str| {
        steps
            .iter()
            .position(|step| {
                step["uses"]
                    .as_str()
                    .is_some_and(|uses| uses.starts_with(needle))
            })
            .unwrap_or_else(|| panic!("the job holds no step that uses {needle}"))
    };
    let stage = steps
        .iter()
        .position(|step| step["run"].is_string())
        .expect("a step stages the site");
    assert!(position("actions/checkout") < stage);
    assert!(stage < position("actions/upload-pages-artifact"));
    assert!(position("actions/upload-pages-artifact") < position("actions/deploy-pages"));
    assert_eq!(steps[position("actions/deploy-pages")]["id"], "deployment");
}

/// Spec 9.6: the site's root holds the directory `schemas/`, so that a file keeps its
/// `schemas/` segment in its URL. The staging script runs in a scratch directory that holds a
/// copy of `schemas/`; the directory that the upload step names then holds `schemas/<name>/
/// <semver>.json` and `schemas/published.json`, byte for byte.
#[cfg(unix)]
#[test]
fn schemas_stages_a_site_whose_root_holds_the_schemas_directory() {
    use crate::ci_support::sandbox::Sandbox;

    let unit = workflow(SCHEMAS);
    let job = unit.job("deploy");
    let steps = job["steps"].as_array().expect("steps");
    let script = steps
        .iter()
        .find_map(|step| step["run"].as_str())
        .expect("a step stages the site");
    let site = steps
        .iter()
        .find(|step| {
            step["uses"]
                .as_str()
                .is_some_and(|uses| uses.starts_with("actions/upload-pages-artifact@"))
        })
        .and_then(|step| step["with"]["path"].as_str())
        .expect("the upload step names the site directory")
        .to_string();
    assert!(
        !site.is_empty() && !site.starts_with('/') && !site.starts_with('.'),
        "the site directory {site} is a new directory of the workspace"
    );
    assert!(
        !site.starts_with("schemas"),
        "the site directory {site} is not schemas itself"
    );

    let sandbox = Sandbox::new();
    tracker_support::copy_dir(&path_of("schemas"), &sandbox.work().join("schemas"));
    let output = sandbox.run_bash(script, Vec::<(String, String)>::new());
    assert!(
        output.status.success(),
        "the staging script failed:\n{}{}",
        tracker_support::stdout(&output),
        tracker_support::stderr(&output)
    );
    let staged = sandbox.work().join(&site).join("schemas");
    let mut compared = 0;
    for relative in files_below(&path_of("schemas"), std::path::Path::new("")) {
        let original = fs::read(path_of("schemas").join(&relative)).expect("read a schema file");
        let copy = fs::read(staged.join(&relative)).unwrap_or_else(|error| {
            panic!(
                "the site holds no schemas/{}: {error}",
                relative.to_string_lossy()
            )
        });
        assert_eq!(
            original,
            copy,
            "schemas/{} differs in the site",
            relative.to_string_lossy()
        );
        compared += 1;
    }
    assert!(
        compared >= 5,
        "the site holds the published schemas and their manifest: {compared} files"
    );
}

/// The files below `base`/`relative`, as paths relative to `base`, in a stable order.
#[cfg(unix)]
fn files_below(base: &std::path::Path, relative: &std::path::Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(base.join(relative))
        .expect("read a directory")
        .map(|entry| entry.expect("an entry"))
        .collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let child = relative.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            found.extend(files_below(base, &child));
        } else {
            found.push(child);
        }
    }
    found
}

// ---- ci.yml ----

/// Spec 10.8 and 9.3: the `checks` job of `ci.yml` runs `gen-schemas --check`.
#[test]
fn ci_runs_gen_schemas_check_in_the_checks_job() {
    let unit = workflow(CI);
    let steps: Vec<String> = unit
        .scripts("checks")
        .into_iter()
        .map(|script| script.text.trim().to_string())
        .collect();
    assert!(
        steps.contains(&"cargo run --quiet --bin gen-schemas -- --check".to_string()),
        "the checks job runs gen-schemas --check: {steps:?}"
    );
}

// ---- actions ----

/// Every `uses:` value of `text` as `(line, value)`.
fn uses_values(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line
                .trim_start()
                .trim_start_matches("- ")
                .strip_prefix("uses:")?;
            Some((line.to_string(), rest.trim().to_string()))
        })
        .collect()
}

/// Every action reference of the workflow in `text`, as GitHub reads it from the parsed
/// workflow: the `uses` of a job that calls a reusable workflow and the `uses` of each step,
/// whatever the YAML style.
fn action_references(text: &str) -> Vec<String> {
    let unit = parse_unit(text, Host::GitHub);
    unit.job_keys()
        .into_iter()
        .flat_map(|job| {
            let call = unit
                .job(&job)
                .get("uses")
                .and_then(Value::as_str)
                .map(str::to_string);
            call.into_iter().chain(unit.uses(&job))
        })
        .collect()
}

/// The names of the workflow files in `directory`: GitHub reads `.yml` and `.yaml` files.
fn workflow_files(directory: &Path) -> Vec<String> {
    let mut files: Vec<String> = fs::read_dir(directory)
        .expect("read the workflow directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".yml") || name.ends_with(".yaml"))
        .collect();
    files.sort_unstable();
    files
}

/// An action reference pins its action to a full commit sha of 40 lowercase hex digits
/// (`houserules.actions-pinned-by-sha`). A path in this repository (`./`) names the checkout
/// itself and has no sha. `origin` names the workflow file in the panic message.
fn assert_pinned_by_sha(origin: &str, reference: &str) {
    if reference.starts_with("./") {
        return;
    }
    let (_, sha) = reference
        .rsplit_once('@')
        .unwrap_or_else(|| panic!("no pin on: {origin}: {reference}"));
    assert!(
        sha.len() == 40 && sha.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')),
        "not a full commit sha on: {origin}: {reference}"
    );
}

/// The `uses:` line of `reference` in the workflow `text` names the release in a `# vX.Y.Z`
/// comment. A path in this repository (`./`) has no release.
fn assert_names_release(origin: &str, text: &str, reference: &str) {
    if reference.starts_with("./") {
        return;
    }
    let release = uses_values(text)
        .into_iter()
        .find_map(|(_, value)| {
            value
                .strip_prefix(reference)?
                .strip_prefix(" # v")
                .map(str::to_string)
        })
        .unwrap_or_else(|| panic!("the comment names no vX.Y.Z on: {origin}: {reference}"));
    assert!(
        release.split('.').count() >= 2
            && release
                .split('.')
                .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())),
        "the comment names no vX.Y.Z on: {origin}: {reference}"
    );
}

/// Every action reference of every workflow file in `directory` is pinned by sha. A file in
/// `release_files` also names the release of each pinned action in a `# vX.Y.Z` comment.
fn assert_every_action_is_pinned(directory: &Path, release_files: &[&str]) {
    for name in workflow_files(directory) {
        let text = fs::read_to_string(directory.join(&name))
            .unwrap_or_else(|error| panic!("read {name}: {error}"));
        for reference in action_references(&text) {
            assert_pinned_by_sha(&name, &reference);
            if release_files.contains(&name.as_str()) {
                assert_names_release(&name, &text, &reference);
            }
        }
    }
}

/// A workflow file of one job whose one step is `step`.
fn workflow_with_step(step: &str) -> String {
    format!(
        "name: probe\non: push\njobs:\n  probe:\n    runs-on: ubuntu-latest\n    steps:\n      {step}\n"
    )
}

/// A scratch directory that holds the one workflow file `name`.
fn scratch_with(name: &str, text: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().expect("a scratch directory");
    fs::write(directory.path().join(name), text).expect("write the probe workflow");
    directory
}

/// The message of the panic that `check` raises; `None` when `check` passes.
fn panic_message(check: impl FnOnce()) -> Option<String> {
    let payload = catch_unwind(AssertUnwindSafe(check)).err()?;
    Some(
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
            .unwrap_or_default(),
    )
}

/// The Actions policy of this repository rejects an action that a tag or a branch names, so
/// every action reference of every workflow file is pinned by sha. The scope is the directory,
/// not a list: GitHub reads `.yml` and `.yaml` files and every YAML style, and a new workflow
/// file joins the check. The two files of this wiring also name the release in a comment.
#[test]
fn every_action_of_every_workflow_is_pinned_by_sha() {
    let directory = path_of(".github/workflows");
    let files = workflow_files(&directory);
    for own in ["review.yml", "schemas.yml"] {
        assert!(files.iter().any(|name| name == own), "{own} is checked");
    }
    assert_every_action_is_pinned(&directory, &["review.yml", "schemas.yml"]);

    let sha = "3d3c42e5aac5ba805825da76410c181273ba90b1";
    let unpinned: Vec<(&str, &str, String, &str)> = vec![
        (
            "a step that names a tag, in a .yaml file",
            "probe.yaml",
            workflow_with_step("- uses: actions/checkout@v4"),
            "not a full commit sha",
        ),
        (
            "a step in flow style that names a tag",
            "probe.yml",
            workflow_with_step("- { uses: actions/checkout@v4 }"),
            "not a full commit sha",
        ),
        (
            "a step in block style that names a tag",
            "probe.yml",
            workflow_with_step("- uses: actions/checkout@v4"),
            "not a full commit sha",
        ),
        (
            "a step with no pin",
            "probe.yml",
            workflow_with_step("- uses: actions/checkout"),
            "no pin on",
        ),
        (
            "a step with an uppercase sha",
            "probe.yml",
            workflow_with_step(&format!("- uses: actions/checkout@{}", sha.to_uppercase())),
            "not a full commit sha",
        ),
        (
            "a step with a sha of 39 digits",
            "probe.yml",
            workflow_with_step(&format!("- uses: actions/checkout@{}", &sha[1..])),
            "not a full commit sha",
        ),
        (
            "a job that calls a reusable workflow by tag",
            "probe.yml",
            "name: probe\non: push\njobs:\n  call:\n    uses: octo/repo/.github/workflows/w.yml@v1\n"
                .to_string(),
            "not a full commit sha",
        ),
    ];
    let escaped: Vec<&str> = unpinned
        .iter()
        .filter(|(_, name, text, needle)| {
            let directory = scratch_with(name, text);
            !panic_message(|| assert_every_action_is_pinned(directory.path(), &[]))
                .is_some_and(|message| message.contains(needle))
        })
        .map(|(shape, ..)| *shape)
        .collect();
    assert!(escaped.is_empty(), "the check must fail on: {escaped:?}");

    let no_release: Vec<(&str, String)> = vec![
        (
            "a release comment that names a branch",
            workflow_with_step(&format!("- uses: actions/checkout@{sha} # main")),
        ),
        (
            "no release comment",
            workflow_with_step(&format!("- uses: actions/checkout@{sha}")),
        ),
    ];
    for (shape, text) in &no_release {
        let directory = scratch_with("review.yml", text);
        let message =
            panic_message(|| assert_every_action_is_pinned(directory.path(), &["review.yml"]))
                .unwrap_or_else(|| panic!("the check must fail on: {shape}"));
        assert!(
            message.contains("the comment names no vX.Y.Z"),
            "{shape}: {message}"
        );
    }

    let pinned: Vec<(&str, &str, String)> = vec![
        (
            "a pinned step in a .yaml file",
            "probe.yaml",
            workflow_with_step(&format!("- uses: actions/checkout@{sha}")),
        ),
        (
            "a call of a workflow of this repository",
            "review.yml",
            "name: probe\non: push\njobs:\n  call:\n    uses: ./.github/workflows/houserules-review.yml\n"
                .to_string(),
        ),
        (
            "a Dependabot bump that moves the sha and the release comment together",
            "schemas.yml",
            workflow_with_step(
                "- uses: actions/upload-pages-artifact@0123456789abcdef0123456789abcdef01234567 # v5.1.0",
            ),
        ),
    ];
    for (shape, name, text) in &pinned {
        let directory = scratch_with(name, text);
        if let Some(message) = panic_message(|| {
            assert_every_action_is_pinned(directory.path(), &["review.yml", "schemas.yml"])
        }) {
            panic!("the check must pass on {shape}: {message}");
        }
    }
}

// ---- the request template ----

/// The block of spec 7.4 that this repository's request template shares with `ci write --template`
/// and `ci print --template`: the lines of the first `markdown` fence below the `### 7.4`
/// heading, read from the spec file, without the two-space indent of the list item that holds it.
fn template_block_of_the_spec() -> Vec<String> {
    let spec = text_of(SPEC);
    let section: Vec<&str> = spec
        .lines()
        .skip_while(|line| !line.starts_with("### 7.4 "))
        .skip(1)
        .take_while(|line| !line.starts_with("### "))
        .collect();
    let open = section
        .iter()
        .position(|line| *line == "  ````markdown")
        .expect("spec 7.4 holds the template block");
    let close = open
        + 1
        + section[open + 1..]
            .iter()
            .position(|line| *line == "  ````")
            .expect("the template block of spec 7.4 closes");
    section[open + 1..close]
        .iter()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                line.strip_prefix("  ")
                    .unwrap_or_else(|| panic!("a block line without the list indent: {line}"))
                    .to_string()
            }
        })
        .collect()
}

/// The `## Evidence bundle` section of `template` equals `block`, line by line, once the pointer
/// to `houserules bundle write` is set aside: one comment line below the heading and the blank
/// line after it. The section runs to the next `## ` heading, so that it ends where the checklist
/// starts.
fn assert_template_section_equals(template: &str, block: &[String]) {
    let lines: Vec<&str> = template.lines().collect();
    assert_eq!(
        lines
            .iter()
            .filter(|line| **line == "## Evidence bundle")
            .count(),
        1,
        "one section"
    );
    let heading = lines
        .iter()
        .position(|line| *line == "## Evidence bundle")
        .expect("the template holds the heading");
    let end = heading
        + 1
        + lines[heading + 1..]
            .iter()
            .position(|line| line.starts_with("## "))
            .expect("a heading follows the section");
    let mut section: Vec<&str> = lines[heading..end].to_vec();
    while section.last() == Some(&"") {
        section.pop();
    }
    let pointers: Vec<usize> = (0..section.len())
        .filter(|at| {
            section[*at].starts_with("<!--") && section[*at].contains("houserules bundle write")
        })
        .collect();
    assert_eq!(pointers.len(), 1, "one pointer to houserules bundle write");
    section.remove(pointers[0]);
    assert_eq!(
        section.get(pointers[0]),
        Some(&""),
        "a blank line follows the pointer"
    );
    section.remove(pointers[0]);
    assert_eq!(section.join("\n"), block.join("\n"));
}

/// Spec 7.4 and 10.8: `.github/PULL_REQUEST_TEMPLATE.md` holds the section of the block in spec
/// 7.4 byte for byte, with a pointer to `houserules bundle write`. Each changed line fails the
/// comparison.
#[test]
fn the_pull_request_template_holds_the_evidence_bundle_section() {
    let template = text_of(TEMPLATE);
    let block = template_block_of_the_spec();
    assert_eq!(
        block[0], "## Evidence bundle",
        "the block starts at the heading"
    );
    assert_template_section_equals(&template, &block);
    for (shape, from, to) in [
        (
            "an empty Work items line",
            "Work items: <id>, <id>",
            "Work items:",
        ),
        (
            "a changed Risk tier line",
            "Risk tier: <tier>",
            "Risk tier: <garbled>",
        ),
        (
            "a changed capture header line",
            "<the header line of a capture file>",
            "not a capture header",
        ),
        (
            "an example value in place of the Intent placeholder",
            "Intent: <source>:<ref>",
            "Intent: file:docs/plans/2026-10-24-rate-limit.md",
        ),
        (
            "a dropped pointer",
            "Write this section with houserules bundle write.",
            "Write this section.",
        ),
    ] {
        let mutated = template.replace(from, to);
        assert_ne!(mutated, template, "the template holds {from:?}");
        assert!(
            fails(|| assert_template_section_equals(&mutated, &block)),
            "the comparison must fail on: {shape}"
        );
    }
}
