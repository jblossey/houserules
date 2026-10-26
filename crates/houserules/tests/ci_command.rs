//! `houserules ci print` and `houserules ci write` (batch 37, spec 10.2, 10.3, 10.1 for the key, and
//! 7.4 for the template; acceptance case 8, the `ci` parts; HR-245 and HR-179). Each case runs the
//! compiled binary against a scratch repository whose install is written by hand.
//!
//! A line of the spec is the literal of a test; `the_spec_lines_of_these_tests_are_in_the_spec`
//! reads the spec file and shows that each literal is in it. The lines that the spec leaves to the
//! command (the reference lines, the include line, and the errors of an argument) are pinned whole.
//! The inline form for GitHub is compared with the parsed unit, edited by its own table of
//! conditions on the inputs (`expected_github_inline`). The inline form for GitLab is compared with
//! the unit text after a chain of plain replacements (`expected_gitlab_inline`). Both are run
//! through the trust assertions of `ci_support` that every published unit passes, and the GitHub
//! form also through `actionlint`.
//!
//! `actionlint` comes from `mise.toml` (`mise install`); a test fails, and does not skip, when it is
//! missing.

mod ci_support;
mod help_support;
mod tracker_support;

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::process::Output;

use base64::prelude::*;
use ci_support::{Host, Unit, assert_wiring_trust_rules, parse_inline_unit, parse_unit};
use help_support::{help_of_args, states};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tracker_support::{
    assert_actionlint_passes, assert_output, commit_all, git, houserules, install_without_backlog,
    repo_root, run, scratch_git_repo, stderr, stdout,
};

/// The version of the binary under test, which every reference line names.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The spec of the batch, below the repository root.
const SPEC: &str = "docs/specs/2026-10-24-batch-37-per-change-review-layer.md";

/// The GitHub unit, below the repository root.
const GITHUB_UNIT: &str = ".github/workflows/houserules-review.yml";

/// The GitLab unit, below the repository root.
const GITLAB_UNIT: &str = "ci/gitlab-review.yml";

// ---- the lines of the spec ----

/// The closing line of `ci print` (spec 10.3).
const CLOSING_LINE: &str = "ci print: add the file that holds these lines to review.ci.files in .houserules.json, so that a change to it routes to senior-review";

/// The sentence of spec 10.3 that puts the closing line of `ci print` on stderr, after the lines on
/// stdout.
const CLOSING_STREAM: &str = "it ends with one line on stderr, after the lines on stdout, so that `ci print > <file>` writes a valid file";

/// The refusal of a GitHub form where no default branch resolves (spec 10.3).
const NO_DEFAULT_BRANCH: &str =
    "ci: cannot resolve the default branch; set \"defaultBranch\" in .houserules.json";

/// The first comment line of a file of `ci write` (spec 10.3).
const WRITTEN_BY: &str = "# Written by houserules ci write; rewrite it with houserules ci write --force and the flags that wrote it.";

/// The line of `update` for a file of `ci write` of another version (spec 10.3).
const PIN_LINE: &str = "update: <path> pins houserules <old>; this binary is <new>: run houserules ci write --force with the flags that wrote it (a GitHub reference file takes the --sha of v<new>)";

/// The second comment line of a file of `ci write` (spec 10.3).
const VERSION_COMMENT: &str = "# houserules <version>";

/// The refusal of `ci write --host github --mode reference` without `--sha` (spec 10.3).
const SHA_REQUIRED: &str = "ci write: --sha is required to pin the reusable workflow; resolve it with git ls-remote https://github.com/jblossey/houserules refs/tags/v<version>";

/// The refusal of an existing path (spec 10.3).
const EXISTS: &str = "ci write: <path> exists; houserules writes only a file of its own (use --out for another path)";

/// The second line of `ci write --host gitlab` (spec 10.3).
const GITLAB_CI_FILES_LINE: &str = "ci write: add .gitlab-ci.yml, which holds the include, to review.ci.files in .houserules.json, so that a change to it routes to senior-review";

/// The line of `ci write --template` on what the host shows first (spec 10.3).
const PRIORITY: &str = "ci write: a GitLab project setting or a group template, or a GitHub organization template, takes priority over <path>";

/// The refusal of `ci write --template` where a template exists (spec 7.4).
const TEMPLATE_EXISTS: &str = "ci write: <path> exists; add the \"## Evidence bundle\" section that houserules bundle write prints";

/// The refusal of an origin that is no known host (spec 10.3).
const UNKNOWN_ORIGIN: &str =
    "ci: the origin <host> is no known host; pass --host github or --host gitlab";

/// The refusal of a repository with no origin (spec 10.3).
const NO_ORIGIN: &str = "ci: no origin remote; pass --host github or --host gitlab";

/// The refusal of two installs that share a key (spec 10.1).
const SHARED_KEY: &str = "ci: <path> and <path> share the key <key>; rename one install folder";

/// `template` with its placeholder `name` replaced by `value`.
fn with(template: &str, name: &str, value: &str) -> String {
    template.replace(name, value)
}

/// The refusal of `ci write` without `--sha`.
fn sha_required() -> String {
    with(SHA_REQUIRED, "<version>", VERSION)
}

/// The line of `update` for the file `path` of `ci write`, which pins the version `old`.
fn pin_line(path: &str, old: &str) -> String {
    PIN_LINE
        .replace("<path>", path)
        .replace("<old>", old)
        .replace("<new>", VERSION)
}

/// The refusal of the existing path `path`.
fn exists(path: &str) -> String {
    with(EXISTS, "<path>", path)
}

/// The refusal of the existing template `path`.
fn template_exists(path: &str) -> String {
    with(TEMPLATE_EXISTS, "<path>", path)
}

/// The line on what the host shows before the template at `path`.
fn priority(path: &str) -> String {
    with(PRIORITY, "<path>", path)
}

/// The refusal of the origin of the host `host`.
fn unknown_origin(host: &str) -> String {
    with(UNKNOWN_ORIGIN, "<host>", host)
}

/// The refusal of the installs at `first` and `second` that share `key`.
fn shared_key(first: &str, second: &str, key: &str) -> String {
    SHARED_KEY
        .replacen("<path>", first, 1)
        .replacen("<path>", second, 1)
        .replace("<key>", key)
}

/// The lines that the spec gives as a literal, each as the spec writes it.
#[test]
fn the_spec_lines_of_these_tests_are_in_the_spec() {
    let spec = text_of(SPEC);
    for line in [
        CLOSING_LINE,
        CLOSING_STREAM,
        NO_DEFAULT_BRANCH,
        WRITTEN_BY,
        PIN_LINE,
        VERSION_COMMENT,
        SHA_REQUIRED,
        EXISTS,
        GITLAB_CI_FILES_LINE,
        PRIORITY,
        TEMPLATE_EXISTS,
        UNKNOWN_ORIGIN,
        NO_ORIGIN,
        SHARED_KEY,
    ] {
        assert!(
            spec.contains(line),
            "the spec does not hold this line as the test does: {line}"
        );
    }
}

// ---- fixtures ----

/// A scratch repository with one install, whose marker is written by hand.
struct Repo {
    /// The git top level.
    top: TempDir,
    /// The folder of the install below the top level: `""` or a path.
    folder: &'static str,
}

impl Repo {
    /// A repository on `main` whose install in `folder` holds a marker of install schema 2.3.0 and
    /// the extra keys `extra`, committed. With `origin`, the repository has that `origin` remote.
    fn new(folder: &'static str, origin: Option<&str>, extra: Value) -> Self {
        Self::on("main", folder, origin, extra)
    }

    /// [`Repo::new`] on the branch `branch`. The branch is set before the first commit, so that no
    /// `init.defaultBranch` of the host decides what the repository names as its default branch.
    fn on(branch: &str, folder: &'static str, origin: Option<&str>, extra: Value) -> Self {
        let repo = Self {
            top: scratch_git_repo(),
            folder,
        };
        git(
            repo.top.path(),
            &["symbolic-ref", "HEAD", &format!("refs/heads/{branch}")],
        );
        let mut marker = json!({"schema": "houserules/install@2.3.0", "idPrefix": "WI"});
        for (key, value) in extra.as_object().expect("an object of extra keys") {
            marker[key] = value.clone();
        }
        repo.write_in_install(
            ".houserules.json",
            &serde_json::to_string_pretty(&marker).expect("serialize the marker"),
        );
        if let Some(origin) = origin {
            git(repo.top.path(), &["remote", "add", "origin", origin]);
        }
        commit_all(repo.top.path(), "chore: base");
        repo
    }

    /// A top-level install with no `origin` and no extra key.
    fn bare() -> Self {
        Self::new("", None, json!({}))
    }

    /// The install root.
    fn root(&self) -> PathBuf {
        file(self.top.path(), self.folder)
    }

    /// The file `relative` below the git top level, built component by component
    /// (`houserules.path-pins-mirror-the-code`).
    fn at(&self, relative: &str) -> PathBuf {
        file(self.top.path(), relative)
    }

    /// Writes `content` to `relative` below the install root, with its folders.
    fn write_in_install(&self, relative: &str, content: &str) {
        let target = file(&self.root(), relative);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write the file");
    }

    /// Writes `content` to `relative` below the git top level, with its folders.
    fn write(&self, relative: &str, content: &str) {
        let target = self.at(relative);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write the file");
    }

    /// The text of `relative` below the git top level.
    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.at(relative))
            .unwrap_or_else(|error| panic!("read {relative}: {error}"))
    }

    /// The marker as JSON.
    fn marker(&self) -> Value {
        serde_json::from_str(
            &fs::read_to_string(self.root().join(".houserules.json")).expect("read"),
        )
        .expect("parse the marker")
    }

    /// The bytes of the marker.
    fn marker_bytes(&self) -> Vec<u8> {
        fs::read(self.root().join(".houserules.json")).expect("read the marker")
    }

    /// `houserules ci <args> --dir <install root>`.
    fn ci(&self, args: &[&str]) -> Output {
        houserules()
            .arg("ci")
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run houserules ci")
    }

    /// The files below the top level that `git status` shows as new or changed.
    fn status(&self) -> String {
        git(self.top.path(), &["status", "--porcelain"])
    }

    /// A local branch `name` at the commit of `HEAD`.
    fn add_local_branch(&self, name: &str) {
        git(self.top.path(), &["branch", name]);
    }

    /// A remote branch `origin/<name>` at the commit of `HEAD`, as a fetch leaves it.
    fn add_remote_branch(&self, name: &str) {
        git(
            self.top.path(),
            &["update-ref", &format!("refs/remotes/origin/{name}"), "HEAD"],
        );
    }

    /// `origin/HEAD` as a plain ref at the commit of `HEAD`: it resolves, and it names no branch.
    fn add_origin_head_naming_no_branch(&self) {
        git(
            self.top.path(),
            &["update-ref", "refs/remotes/origin/HEAD", "HEAD"],
        );
    }

    /// `origin/HEAD` pointing at `origin/<name>`, as `git clone` and `git remote set-head origin
    /// <name>` leave it. The remote branch is made too.
    fn point_origin_head_at(&self, name: &str) {
        self.add_remote_branch(name);
        git(
            self.top.path(),
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                &format!("refs/remotes/origin/{name}"),
            ],
        );
    }
}

/// The file `relative` below `dir`, built component by component.
fn file(dir: &std::path::Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// The text of the file `relative` below the repository root.
fn text_of(relative: &str) -> String {
    fs::read_to_string(file(&repo_root(), relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// `true` when `check` panics.
fn fails(check: impl FnOnce()) -> bool {
    catch_unwind(AssertUnwindSafe(check)).is_err()
}

/// The `integrity` value of the GitLab unit, computed from the file of the repository.
fn integrity_of_the_unit() -> String {
    let bytes = fs::read(file(&repo_root(), GITLAB_UNIT)).expect("read the GitLab unit");
    format!("sha256-{}", BASE64_STANDARD.encode(Sha256::digest(&bytes)))
}

// ---- the texts of the forms ----

/// The caller workflow of the reference form for GitHub, from the line `name:` on.
fn github_reference(
    sha: &str,
    dir: &str,
    key: &str,
    tracker: Option<&str>,
    branch: &str,
) -> String {
    let tracker = tracker
        .map(|name| format!("      tracker: {name}\n"))
        .unwrap_or_default();
    [
        "name: houserules review\n",
        "on:\n",
        "  pull_request:\n",
        "    types: [opened, synchronize, reopened, edited]\n",
        "  push:\n",
        &format!("    branches: [\"{branch}\"]\n"),
        "jobs:\n",
        "  review:\n",
        "    permissions:\n",
        "      contents: write\n",
        "      pull-requests: write\n",
        "      statuses: write\n",
        "      issues: write\n",
        &format!(
            "    uses: jblossey/houserules/.github/workflows/houserules-review.yml@{sha} # v{VERSION}\n"
        ),
        "    with:\n",
        &format!("      version: {VERSION}\n"),
        &format!("      dir: \"{dir}\"\n"),
        &format!("      key: \"{key}\"\n"),
        &tracker,
    ]
    .concat()
}

/// What `ci print --host github` prints on stdout: the hint that resolves the sha, then the
/// caller with the placeholder for the sha.
fn printed_github_reference(dir: &str, key: &str, tracker: Option<&str>, branch: &str) -> String {
    format!(
        "# Resolve <release commit sha> with: git ls-remote https://github.com/jblossey/houserules refs/tags/v{VERSION}\n{}",
        github_reference("<release commit sha>", dir, key, tracker, branch)
    )
}

/// The include of the reference form for GitLab.
fn gitlab_reference(dir: &str, key: &str, tracker: Option<&str>) -> String {
    let tracker = tracker
        .map(|name| format!("      tracker: {name}\n"))
        .unwrap_or_default();
    format!(
        "include:\n  - remote: https://raw.githubusercontent.com/jblossey/houserules/v{VERSION}/ci/gitlab-review.yml\n    integrity: {}\n    inputs:\n      dir: \"{dir}\"\n      key: \"{key}\"\n{tracker}",
        integrity_of_the_unit()
    )
}

/// The `on` block that an inline form for GitHub gets in place of `workflow_call`.
fn github_triggers(branch: &str) -> String {
    format!(
        "on:\n  pull_request:\n    types: [opened, synchronize, reopened, edited]\n  push:\n    branches: [\"{branch}\"]\n"
    )
}

/// The expression of the GitHub unit that gives a job the token for the GitHub tracker only.
const TOKEN_EXPRESSION: &str = "${{ inputs.tracker == 'github' && github.token || '' }}";

/// The step conditions of the GitHub unit that read an input, each with whether it holds for an
/// install with the tracker `tracker`, no setup command, and the `release` binary for the floor.
fn input_condition_holds(condition: &str, tracker: &str) -> Option<bool> {
    match condition {
        "${{ inputs.setup != '' }}" | "${{ inputs.floor-install == 'default-branch' }}" => {
            Some(false)
        }
        "${{ inputs.tracker == 'beads' }}" => Some(tracker == "beads"),
        _ => None,
    }
}

/// `value` with the inputs of an install bound, as a person would bind them by hand: a string that
/// is exactly one input becomes its value, the key inside a longer string becomes the key, the
/// condition of the Beads tracker in a `with` becomes a boolean, and the expression that gives the
/// token to the GitHub tracker becomes the token or an empty string.
fn bind_json(value: &mut Value, dir: &str, key: &str, tracker: &str) {
    match value {
        Value::String(text) => {
            let bound = match text.as_str() {
                "${{ inputs.dir }}" => Value::from(dir),
                "${{ inputs.key }}" => Value::from(key),
                "${{ inputs.version }}" => Value::from(VERSION),
                "${{ inputs.install }}" | "${{ inputs.floor-install }}" => Value::from("release"),
                "${{ inputs.setup }}" => Value::from(""),
                "${{ inputs.tracker == 'beads' }}" => Value::Bool(tracker == "beads"),
                TOKEN_EXPRESSION if tracker == "github" => Value::from("${{ github.token }}"),
                TOKEN_EXPRESSION => Value::from(""),
                other => Value::from(other.replace("${{ inputs.key }}", key)),
            };
            *value = bound;
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| bind_json(item, dir, key, tracker)),
        Value::Object(map) => map
            .values_mut()
            .for_each(|item| bind_json(item, dir, key, tracker)),
        _ => {}
    }
}

/// The oracle for the inline form for GitHub, as a document: the unit with the triggers of the
/// caller in place of `workflow_call`, each step dropped whose condition on an input fails, the
/// condition of each step kept whose condition holds removed, and the inputs bound. It works on the
/// parsed unit, not on its text, so it shares nothing with the scanner of the command.
fn expected_github_inline(dir: &str, key: &str, tracker: &str, branch: &str) -> Value {
    let mut body = parse_unit(&text_of(GITHUB_UNIT), Host::GitHub)
        .body()
        .clone();
    body["on"] = json!({
        "pull_request": {"types": ["opened", "synchronize", "reopened", "edited"]},
        "push": {"branches": [branch]},
    });
    for job in body["jobs"].as_object_mut().expect("jobs").values_mut() {
        let Some(steps) = job["steps"].as_array_mut() else {
            continue;
        };
        steps.retain_mut(|step| {
            let condition = step["if"].as_str().map(str::to_string);
            let verdict = condition
                .as_deref()
                .and_then(|condition| input_condition_holds(condition, tracker));
            if verdict == Some(true) {
                step.as_object_mut().expect("a step").remove("if");
            }
            verdict != Some(false)
        });
    }
    bind_json(&mut body, dir, key, tracker);
    body
}

/// The names of the steps of the job `role`, in the order of the file.
fn step_names(body: &Value, role: &str) -> Vec<String> {
    body["jobs"][role]["steps"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| step["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The oracle for the inline form for GitLab: the jobs of the unit after its `spec` document, with
/// every input replaced by its value.
fn expected_gitlab_inline(unit: &Unit, dir: &str, key: &str, tracker: &str) -> String {
    let text = text_of(GITLAB_UNIT);
    let (_, body) = text
        .split_once("\n---\n")
        .expect("the unit has a spec header");
    let default_of = |name: &str| {
        unit.input(name)["default"]
            .as_str()
            .unwrap_or_else(|| panic!("the input {name} has a default"))
            .to_string()
    };
    let json = |value: &str| serde_json::to_string(value).expect("a JSON string");
    let mut text = body.replace(" # x-release-please-version", "").replace(
        "HOUSERULES_KEY: $[[ inputs.key ]]",
        &format!("HOUSERULES_KEY: {}", json(key)),
    );
    text = text.replace("$[[ inputs.key ]]", key);
    for (input, value) in [
        ("dir", dir.to_string()),
        ("image", default_of("image")),
        ("stage", default_of("stage")),
        ("install", "release".to_string()),
        ("floor-install", "release".to_string()),
        ("tracker", tracker.to_string()),
        ("setup", String::new()),
        ("token-environment", format!("houserules-{key}")),
    ] {
        text = text.replace(&format!("$[[ inputs.{input} ]]"), &json(&value));
    }
    text
}

/// The template block of spec 7.4, read from the spec file: the lines of the first `markdown`
/// fence below the `### 7.4` heading, without the two-space indent of the list item that holds it,
/// each with its line break.
fn template_block() -> String {
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
            .expect("the template block closes");
    section[open + 1..close]
        .iter()
        .map(|line| format!("{}\n", line.strip_prefix("  ").unwrap_or(line)))
        .collect()
}

/// The pointer to `houserules bundle write` that the written template holds below the heading.
const POINTER: &str = "<!-- Write this section with houserules bundle write. It copies the header of each run from the capture file. -->";

/// The template that `ci write --template` writes: the block of spec 7.4 with the pointer and a
/// blank line after the heading and its blank line.
fn written_template() -> String {
    let block = template_block();
    let (heading, rest) = block
        .split_once("\n\n")
        .expect("a heading and a blank line");
    format!("{heading}\n\n{POINTER}\n\n{rest}")
}

/// The lines of `text` that hold a `github.com` outside the URLs of this repository.
fn stray_github_hosts(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (index, _) in text.match_indices("github.com") {
        let after = &text[index + "github.com".len()..];
        let own = text[..index].ends_with("://")
            && (after.starts_with("/jblossey/houserules")
                || after.starts_with("/gastownhall/beads/releases/download/"));
        if !own {
            found.push(after.lines().next().unwrap_or_default().to_string());
        }
    }
    found
}

// ---- print: the reference forms ----

/// Spec 10.2 and 10.3: `ci print --host github` prints the caller of the release of the binary: the
/// union of the permissions of the five jobs, `version:` beside the pinned line, the install, and the
/// hint that resolves the sha. The closing line of 10.3 ends the output, on stderr, so that the
/// lines on stdout stay a valid workflow.
#[test]
fn print_github_reference_prints_the_caller_of_this_release() {
    let repo = Repo::bare();
    let output = repo.ci(&["print", "--host", "github"]);
    assert_output(
        &output,
        0,
        &printed_github_reference(".", "root", None, "main"),
        &format!("{CLOSING_LINE}\n"),
    );
}

/// Spec 10.2: the caller fills `dir` and `key` for an install below the top level, passes the
/// tracker of the install when it needs access (`github` and `beads`, not `json`), and listens to
/// the `defaultBranch` of the marker.
#[test]
fn print_github_reference_names_the_install_the_tracker_and_the_default_branch() {
    for (tracker, line) in [
        (json!({"adapter": "github", "repo": "o/r"}), Some("github")),
        (json!({"adapter": "beads"}), Some("beads")),
        (json!({"adapter": "json"}), None),
    ] {
        let repo = Repo::new(
            "sub/dir",
            None,
            json!({"tracker": tracker, "defaultBranch": "trunk"}),
        );
        let output = repo.ci(&["print", "--host", "github"]);
        assert_output(
            &output,
            0,
            &printed_github_reference("sub/dir", "sub-dir", line, "trunk"),
            &format!("{CLOSING_LINE}\n"),
        );
    }
}

/// Spec 10.2 and case 8: `ci print --host gitlab` prints the include of the unit of this release,
/// with the `integrity` value `sha256-<base64>` of the embedded file and the inputs of the install.
#[test]
fn print_gitlab_reference_prints_the_include_with_the_integrity_of_the_unit() {
    let repo = Repo::bare();
    let output = repo.ci(&["print", "--host", "gitlab"]);
    assert_output(
        &output,
        0,
        &gitlab_reference(".", "root", None),
        &format!("{CLOSING_LINE}\n"),
    );
    let integrity = integrity_of_the_unit();
    assert!(
        integrity.starts_with("sha256-") && integrity.ends_with('='),
        "a padded base64 value: {integrity}"
    );
    let nested = Repo::new(
        "docs",
        None,
        json!({"tracker": {"adapter": "github", "repo": "o/r"}}),
    );
    assert_output(
        &nested.ci(&["print", "--host", "gitlab", "--mode", "reference"]),
        0,
        &gitlab_reference("docs", "docs", Some("github")),
        &format!("{CLOSING_LINE}\n"),
    );
}

// ---- print: the inline forms ----

/// Writes `text` to a file of a scratch directory and requires `actionlint` to pass it.
fn assert_workflow_passes_actionlint(text: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("workflow.yml");
    fs::write(&path, text).expect("write the workflow");
    assert_actionlint_passes(&path);
}

/// Spec 10.3 and case 8: `ci print --host github --mode inline` prints the unit as a workflow of the
/// project: the triggers of the caller in place of `workflow_call`, every input bound, a step dropped
/// where its condition on an input fails (a constant condition is a defect of a workflow), and the
/// rest of the unit as it is. The oracle works on the parsed unit.
#[test]
fn print_inline_github_binds_every_input_and_keeps_the_unit_otherwise() {
    let repo = Repo::bare();
    let output = repo.ci(&["print", "--host", "github", "--mode", "inline"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), format!("{CLOSING_LINE}\n"));
    let printed = stdout(&output);
    assert!(
        printed.starts_with(&format!(
            "name: houserules review\n\n{}\nenv:\n",
            github_triggers("main")
        )),
        "the form starts with the name and the triggers of the caller"
    );
    assert!(!printed.contains("inputs."), "an input is left unbound");
    assert!(
        !printed.contains("workflow_call"),
        "the form is no reusable workflow"
    );
    assert!(printed.contains("      HOUSERULES_DIR: \".\"\n"));
    assert!(printed.contains("      HOUSERULES_KEY: \"root\"\n"));
    assert!(printed.contains(&format!("      HOUSERULES_VERSION: \"{VERSION}\"\n")));
    assert!(printed.contains("      GH_TOKEN: \"\"\n"));
    assert!(printed.contains("persist-credentials: false\n"));
    let unit = parse_unit(&printed, Host::GitHub);
    assert_eq!(
        unit.body(),
        &expected_github_inline(".", "root", "", "main"),
        "the form is the unit with the inputs bound"
    );
    let published = parse_unit(&text_of(GITHUB_UNIT), Host::GitHub);
    for (role, dropped) in [
        (
            "gates",
            vec!["Run setup", "Install bd", "Bootstrap the Beads store"],
        ),
        ("floor", vec!["Check out the default branch"]),
        (
            "record",
            vec!["Run setup", "Install bd", "Bootstrap the Beads store"],
        ),
    ] {
        let kept = step_names(unit.body(), role);
        let all = step_names(published.body(), role);
        let left_out: Vec<&String> = all.iter().filter(|name| !kept.contains(name)).collect();
        assert_eq!(left_out, dropped, "the steps of {role} that the form drops");
    }
    assert_workflow_passes_actionlint(&printed);
    assert_wiring_trust_rules(&unit);
}

/// An inline form for GitHub of an install below the top level binds `dir`, `key`, and the tracker,
/// keeps the steps of the tracker without their condition, and passes the same checks, for each
/// tracker that needs access in a job.
#[test]
fn print_inline_github_for_a_subdirectory_install_with_a_tracker() {
    for (tracker, config) in [
        ("github", json!({"adapter": "github", "repo": "o/r"})),
        ("beads", json!({"adapter": "beads"})),
    ] {
        let repo = Repo::new(
            "sub/dir",
            None,
            json!({"tracker": config, "defaultBranch": "trunk"}),
        );
        let output = repo.ci(&["print", "--host", "github", "--mode", "inline"]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let printed = stdout(&output);
        let unit = parse_unit(&printed, Host::GitHub);
        assert_eq!(
            unit.body(),
            &expected_github_inline("sub/dir", "sub-dir", tracker, "trunk")
        );
        assert!(printed.contains("      HOUSERULES_DIR: \"sub/dir\"\n"));
        assert!(printed.contains("name: houserules-evidence-sub-dir\n"));
        assert!(printed.contains("branches: [\"trunk\"]\n"));
        assert!(!printed.contains("inputs."));
        let github = tracker == "github";
        assert_eq!(
            printed
                .lines()
                .any(|line| line == "      GH_TOKEN: ${{ github.token }}"),
            github,
            "the job env gives the token to the GitHub tracker only"
        );
        assert_eq!(
            step_names(unit.body(), "gates").contains(&"Install bd".to_string()),
            !github,
            "the Beads steps are kept for the Beads tracker only"
        );
        assert_workflow_passes_actionlint(&printed);
        assert_wiring_trust_rules(&unit);
    }
}

/// Spec 10.3 and case 8: `ci print --host gitlab --mode inline` prints the jobs of the unit without
/// its `spec` header, every input bound. The oracle is a chain of plain replacements.
#[test]
fn print_inline_gitlab_binds_every_input_and_keeps_the_unit_otherwise() {
    let published = parse_unit(&text_of(GITLAB_UNIT), Host::gitlab("root"));
    for (folder, dir, key, tracker, adapter) in [
        ("", ".", "root", "", json!({"adapter": "json"})),
        ("docs", "docs", "docs", "beads", json!({"adapter": "beads"})),
    ] {
        let repo = Repo::new(folder, None, json!({"tracker": adapter}));
        let output = repo.ci(&["print", "--host", "gitlab", "--mode", "inline"]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(stderr(&output), format!("{CLOSING_LINE}\n"));
        let printed = stdout(&output);
        assert_eq!(
            printed,
            expected_gitlab_inline(&published, dir, key, tracker)
        );
        assert!(!printed.contains("$[["), "an input is left unbound");
        assert!(!printed.contains("\nspec:"), "the form takes no input");
        let unit = parse_inline_unit(&printed, Host::gitlab(key));
        assert_wiring_trust_rules(&unit);
        assert_eq!(
            unit.job_keys().first().map(String::as_str),
            Some(format!("houserules-{key}-gates").as_str())
        );
    }
}

/// A control character of an install path is written as an escape in every form, so that the form
/// stays valid YAML: the delete character and a C1 control sit in the folder name. The parsed form
/// holds the folder name again, character for character.
#[test]
fn a_control_character_of_the_install_path_is_escaped_in_every_form() {
    const FOLDER: &str = "a\u{7f}b\u{85}c";
    const ESCAPED: &str = "\"a\\u007fb\\u0085c\"";
    let repo = Repo::new(FOLDER, None, json!({}));
    for (args, host) in [
        (vec!["print", "--host", "gitlab"], Host::gitlab("a-b-c")),
        (
            vec!["print", "--host", "gitlab", "--mode", "inline"],
            Host::gitlab("a-b-c"),
        ),
        (vec!["print", "--host", "github"], Host::GitHub),
        (
            vec!["print", "--host", "github", "--mode", "inline"],
            Host::GitHub,
        ),
    ] {
        let output = repo.ci(&args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
        let printed = stdout(&output);
        assert!(
            printed.contains(&format!("dir: {ESCAPED}\n"))
                || printed.contains(&format!("HOUSERULES_DIR: {ESCAPED}\n")),
            "{args:?}: the folder name is an escaped JSON string"
        );
        assert!(
            printed.chars().all(|c| c == '\n' || !c.is_control()),
            "{args:?}: no raw control character"
        );
        let unit = match host {
            Host::GitHub => parse_unit(&printed, Host::GitHub),
            gitlab => parse_inline_unit(&printed, gitlab),
        };
        assert!(
            unit.body().to_string().contains(FOLDER),
            "{args:?}: the parsed form holds the folder name"
        );
    }
}

/// The wiring rules reject a forbidden shape of an inline form, so a pass of the tests above shows
/// that the form holds none: a `floor` job that writes or that reads the request in the workspace
/// (GitHub), and a `gates` job that names the token environment (GitLab).
#[test]
fn the_wiring_rules_reject_a_forbidden_shape_in_an_inline_form() {
    let repo = Repo::bare();
    let github = stdout(&repo.ci(&["print", "--host", "github", "--mode", "inline"]));
    let unit = parse_unit(&github, Host::GitHub);
    assert_wiring_trust_rules(&unit);
    let widened = unit.mutated(|body| {
        body["jobs"]["floor"]["permissions"]["contents"] = json!("write");
    });
    assert!(fails(|| assert_wiring_trust_rules(&widened)));
    let in_the_workspace = unit.mutated(|body| {
        let steps = body["jobs"]["floor"]["steps"]
            .as_array_mut()
            .expect("steps");
        for step in steps
            .iter_mut()
            .filter(|step| step["name"] == "Compute the floor")
        {
            step["working-directory"] = json!(".");
        }
    });
    assert!(fails(|| assert_wiring_trust_rules(&in_the_workspace)));

    // The floor of the form installs the release. A floor that installs the default branch must
    // build it in a folder beside the request: with no checkout of its own, or with a checkout inside
    // the folder of the request, the rules reject the form, and with a checkout beside it they
    // accept it.
    let source = |body: &mut Value, default_branch_folder: Option<&str>| {
        let steps = body["jobs"]["floor"]["steps"]
            .as_array_mut()
            .expect("steps");
        let request = steps
            .iter()
            .find(|step| step["name"] == "Check out the request")
            .expect("the checkout of the request")
            .clone();
        let install = steps
            .iter_mut()
            .find(|step| step["name"] == "Install houserules")
            .expect("the install step");
        install["env"]["HOUSERULES_SOURCE"] = json!("default-branch");
        if let Some(folder) = default_branch_folder {
            install["env"]["HOUSERULES_DEFAULT_SRC"] =
                json!(format!("${{{{ github.workspace }}}}/{folder}"));
            let mut checkout = request;
            checkout["name"] = json!("Check out the default branch");
            checkout["with"]["ref"] = json!("${{ github.event.repository.default_branch }}");
            checkout["with"]["path"] = json!(folder);
            steps.insert(1, checkout);
        }
    };
    let no_checkout = unit.mutated(|body| source(body, None));
    assert!(fails(|| assert_wiring_trust_rules(&no_checkout)));
    let inside_the_request = unit.mutated(|body| source(body, Some("houserules-request/source")));
    assert!(fails(|| assert_wiring_trust_rules(&inside_the_request)));
    let beside_the_request = unit.mutated(|body| source(body, Some("houserules-source")));
    assert_wiring_trust_rules(&beside_the_request);

    let gitlab = stdout(&repo.ci(&["print", "--host", "gitlab", "--mode", "inline"]));
    let unit = parse_inline_unit(&gitlab, Host::gitlab("root"));
    assert_wiring_trust_rules(&unit);
    let reading = unit.mutated(|body| {
        body["houserules-root-gates"]["environment"] =
            json!({"name": "houserules-root", "action": "access"});
    });
    assert!(fails(|| assert_wiring_trust_rules(&reading)));
}

// ---- print: the template ----

/// Spec 7.4: `ci print --template` prints the section of the template, byte for byte, and no closing
/// line, for either host.
#[test]
fn print_template_prints_the_block_of_spec_7_4() {
    let repo = Repo::bare();
    let block = template_block();
    assert!(block.starts_with("## Evidence bundle\n\nPurpose: <one or two sentences>\n"));
    for host in ["github", "gitlab"] {
        assert_output(
            &repo.ci(&["print", "--host", host, "--template"]),
            0,
            &block,
            "",
        );
    }
}

/// The pointer in the written template is the one of this repository's own request template.
#[test]
fn the_pointer_of_the_written_template_is_the_one_of_the_repository_template() {
    assert!(
        text_of(".github/PULL_REQUEST_TEMPLATE.md")
            .lines()
            .any(|line| line == POINTER),
        "the repository template holds the pointer line"
    );
}

// ---- the default branch ----

/// The branches that the `push` trigger of the GitHub form `form` names.
fn pushed_branches(form: &str) -> Value {
    parse_unit(form, Host::GitHub).body()["on"]["push"]["branches"].clone()
}

/// The branches that the `push` trigger names in each of the four GitHub forms of `repo`: the
/// reference and the inline form, each printed and then written. Every form must exit 0.
fn branches_of_every_github_form(repo: &Repo) -> Vec<Value> {
    let mut found = Vec::new();
    for args in [
        vec!["print", "--host", "github"],
        vec!["print", "--host", "github", "--mode", "inline"],
    ] {
        let output = repo.ci(&args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
        found.push(pushed_branches(&stdout(&output)));
    }
    for (index, args) in [
        vec!["write", "--host", "github", "--sha", SHA],
        vec!["write", "--host", "github", "--mode", "inline"],
    ]
    .into_iter()
    .enumerate()
    {
        let out = format!("wired/{index}.yml");
        let mut args = args;
        args.extend(["--out", &out]);
        let output = repo.ci(&args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
        found.push(pushed_branches(&repo.read(&out)));
    }
    found
}

/// Spec 10.3: a GitHub form triggers on a push to the default branch by name: the `defaultBranch`
/// of the marker, else the branch that `origin/HEAD` names, else `main` or `master` when
/// `origin/<name>` or a local `<name>` exists, in the order of `durable-sha`. Each case runs the four
/// GitHub forms (reference and inline, printed and written), and the order of the places is the
/// order of `durable-sha`: `origin/HEAD`, `origin/main`, `origin/master`, `main`, `master`.
#[test]
fn a_github_form_triggers_on_the_default_branch_that_the_repository_names() {
    type Setup = fn(&Repo);
    let cases: [(&str, &str, Value, Setup, &str); 10] = [
        (
            "the key of the marker wins over origin/HEAD",
            "main",
            json!({"defaultBranch": "trunk"}),
            |repo| repo.point_origin_head_at("master"),
            "trunk",
        ),
        (
            "origin/HEAD names master, and a local main exists",
            "main",
            json!({}),
            |repo| repo.point_origin_head_at("master"),
            "master",
        ),
        (
            "origin/HEAD wins over origin/main",
            "main",
            json!({}),
            |repo| {
                repo.add_remote_branch("main");
                repo.point_origin_head_at("master");
            },
            "master",
        ),
        (
            "origin/HEAD names a branch with a slash",
            "main",
            json!({}),
            |repo| repo.point_origin_head_at("release/1.x"),
            "release/1.x",
        ),
        (
            "an origin/HEAD that names no branch is passed over",
            "master",
            json!({}),
            Repo::add_origin_head_naming_no_branch,
            "master",
        ),
        (
            "origin/main wins over a local master",
            "master",
            json!({}),
            |repo| repo.add_remote_branch("main"),
            "main",
        ),
        (
            "origin/master wins over a local main",
            "main",
            json!({}),
            |repo| repo.add_remote_branch("master"),
            "master",
        ),
        (
            "a local main wins over a local master",
            "main",
            json!({}),
            |repo| repo.add_local_branch("master"),
            "main",
        ),
        ("a local main only", "main", json!({}), |_| {}, "main"),
        ("a local master only", "master", json!({}), |_| {}, "master"),
    ];
    for (label, local, marker, setup, expected) in cases {
        let repo = Repo::on(local, "", None, marker);
        setup(&repo);
        assert_eq!(
            branches_of_every_github_form(&repo),
            vec![json!([expected]); 4],
            "{label}"
        );
    }
}

/// Spec 10.3: where the marker names no `defaultBranch` and none of the places holds a branch, each
/// of the four GitHub forms is exit 2 with its line and writes nothing. The GitLab forms and the
/// request template name no default branch, so they do not need one.
#[test]
fn a_github_form_needs_a_default_branch_and_no_other_form_does() {
    let repo = Repo::on("trunk", "", None, json!({}));
    repo.add_remote_branch("trunk");
    let marker = repo.marker_bytes();
    for args in [
        vec!["print", "--host", "github"],
        vec!["print", "--host", "github", "--mode", "inline"],
        vec!["write", "--host", "github", "--sha", SHA],
        vec!["write", "--host", "github", "--mode", "inline"],
    ] {
        assert_output(&repo.ci(&args), 2, "", &format!("{NO_DEFAULT_BRANCH}\n"));
    }
    assert_eq!(repo.status(), "", "nothing was written");
    assert_eq!(repo.marker_bytes(), marker);

    for args in [
        vec!["print", "--host", "gitlab"],
        vec!["print", "--host", "gitlab", "--mode", "inline"],
        vec!["print", "--host", "github", "--template"],
        vec!["write", "--host", "gitlab"],
        vec!["write", "--host", "github", "--template"],
    ] {
        let output = repo.ci(&args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
    }

    // A marker that names the branch settles it, whatever the repository holds.
    let named = Repo::on("trunk", "", None, json!({"defaultBranch": "trunk"}));
    assert_eq!(
        branches_of_every_github_form(&named),
        vec![json!(["trunk"]); 4]
    );
}

// ---- the host ----

/// Spec 10.3: `--host` defaults to the host of the origin when it is github.com or gitlab.com, in
/// every remote form.
#[test]
fn the_host_is_the_host_of_the_origin_when_it_is_github_com_or_gitlab_com() {
    for (origin, github) in [
        ("https://github.com/o/r.git", true),
        ("git@github.com:o/r.git", true),
        ("ssh://git@github.com/o/r", true),
        ("https://GitHub.com/o/r", true),
        ("https://gitlab.com/o/r.git", false),
        ("git@gitlab.com:o/r.git", false),
        ("https://user@gitlab.com:443/o/r", false),
    ] {
        let repo = Repo::new("", Some(origin), json!({}));
        let output = repo.ci(&["print"]);
        let expected = if github {
            printed_github_reference(".", "root", None, "main")
        } else {
            gitlab_reference(".", "root", None)
        };
        assert_output(&output, 0, &expected, &format!("{CLOSING_LINE}\n"));
    }
}

/// Spec 10.3: `--host` wins over the origin.
#[test]
fn the_host_flag_wins_over_the_origin() {
    let repo = Repo::new("", Some("https://gitlab.com/o/r"), json!({}));
    assert_output(
        &repo.ci(&["print", "--host", "github"]),
        0,
        &printed_github_reference(".", "root", None, "main"),
        &format!("{CLOSING_LINE}\n"),
    );
}

/// Spec 10.3 and case 8: an origin that is no known host, a self-hosted forge or a path, and a
/// repository with no origin need `--host`; each is exit 2 with its line, for every subcommand and
/// every form, and writes nothing.
#[test]
fn another_origin_or_none_needs_the_host_flag() {
    for (origin, line) in [
        (
            Some("https://git.example.org/o/r.git"),
            unknown_origin("git.example.org"),
        ),
        (
            Some("git@Git.Example.Org:o/r.git"),
            unknown_origin("git.example.org"),
        ),
        (Some("/srv/git/r.git"), unknown_origin("/srv/git/r.git")),
        (None, NO_ORIGIN.to_string()),
    ] {
        let repo = Repo::new("", origin, json!({}));
        let marker = repo.marker_bytes();
        for args in [
            vec!["print"],
            vec!["print", "--mode", "inline"],
            vec!["print", "--template"],
            vec!["write", "--sha", "0123456789012345678901234567890123456789"],
            vec!["write", "--mode", "inline"],
            vec!["write", "--template"],
        ] {
            let output = repo.ci(&args);
            assert_output(&output, 2, "", &format!("{line}\n"));
        }
        assert_eq!(repo.status(), "", "nothing was written");
        assert_eq!(repo.marker_bytes(), marker);
    }
}

// ---- write: the files ----

/// A release commit sha for the tests of `--sha`.
const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// Spec 10.3: `ci write --host github --mode reference --sha <sha>` creates the workflow at its
/// default path with the two comment lines, the sha in the pinned line, and prints nothing. It adds
/// the path to `review.ci.files` and changes no other key of the marker.
#[test]
fn write_github_reference_creates_its_file_and_lists_it_in_the_marker() {
    let repo = Repo::bare();
    let before = repo.marker();
    let output = repo.ci(&["write", "--host", "github", "--sha", SHA]);
    assert_output(&output, 0, "", "");
    let path = ".github/workflows/houserules-review.yml";
    assert_eq!(
        repo.read(path),
        format!(
            "{WRITTEN_BY}\n# houserules {VERSION}\n{}",
            github_reference(SHA, ".", "root", None, "main")
        )
    );
    let mut expected = before;
    expected["review"] = json!({"ci": {"files": [path]}});
    assert_eq!(repo.marker(), expected);
    assert_eq!(
        repo.status().lines().count(),
        2,
        "only the workflow and the marker changed: {}",
        repo.status()
    );
}

/// Spec 10.3: the file lives at the git top level, whatever the install, and the entry of
/// `review.ci.files` is its path from the top level (4.1).
#[test]
fn write_for_a_subdirectory_install_writes_at_the_top_level_and_lists_the_top_level_path() {
    let repo = Repo::new("sub/dir", None, json!({}));
    let output = repo.ci(&["write", "--host", "gitlab"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let path = ".gitlab/houserules-review.gitlab-ci.yml";
    assert_eq!(
        repo.read(path),
        format!(
            "{WRITTEN_BY}\n# houserules {VERSION}\n{}",
            gitlab_reference("sub/dir", "sub-dir", None)
        )
    );
    assert!(
        !repo.root().join(".gitlab").exists(),
        "nothing below the install"
    );
    assert_eq!(repo.marker()["review"]["ci"]["files"], json!([path]));
}

/// Spec 10.3: without `--sha`, `ci write --host github --mode reference` is exit 2 with its line and
/// writes nothing; a value that is no commit sha is refused too; the inline form and GitLab need no
/// sha.
#[test]
fn write_github_reference_needs_a_commit_sha() {
    let repo = Repo::bare();
    let marker = repo.marker_bytes();
    assert_output(
        &repo.ci(&["write", "--host", "github"]),
        2,
        "",
        &format!("{}\n", sha_required()),
    );
    assert_output(
        &repo.ci(&["write", "--host", "github", "--mode", "reference"]),
        2,
        "",
        &format!("{}\n", sha_required()),
    );
    for bad in [
        "main",
        "v1.4.0",
        "0123456",
        "0123456789abcdef0123456789abcdef0123456g",
        "",
    ] {
        let output = repo.ci(&["write", "--host", "github", "--sha", bad]);
        assert_output(
            &output,
            2,
            "",
            &format!(
                "ci write: --sha is no commit sha of 40 hexadecimal digits; resolve it with git ls-remote https://github.com/jblossey/houserules refs/tags/v{VERSION}\n"
            ),
        );
    }
    assert_eq!(repo.status(), "", "nothing was written");
    assert_eq!(repo.marker_bytes(), marker);
    for args in [
        vec!["write", "--host", "github", "--mode", "inline"],
        vec!["write", "--host", "gitlab"],
    ] {
        assert_eq!(repo.ci(&args).status.code(), Some(0), "{args:?}");
    }
}

/// Spec 10.3: `ci write` never writes into an existing file. A second write is exit 1 with its line
/// and changes nothing; `--force` rewrites a file whose first line is the comment of `ci write`, and
/// only such a file.
#[test]
fn write_refuses_an_existing_file_and_force_rewrites_only_its_own() {
    let repo = Repo::bare();
    let path = ".github/workflows/houserules-review.yml";
    assert_eq!(
        repo.ci(&["write", "--host", "github", "--sha", SHA])
            .status
            .code(),
        Some(0)
    );
    let written = repo.read(path);
    let marker = repo.marker_bytes();
    assert_output(
        &repo.ci(&["write", "--host", "github", "--sha", SHA]),
        1,
        "",
        &format!("{}\n", exists(path)),
    );
    assert_eq!(repo.read(path), written);

    let other = "1111111111111111111111111111111111111111";
    assert_output(
        &repo.ci(&["write", "--host", "github", "--sha", other, "--force"]),
        0,
        "",
        "",
    );
    assert_eq!(repo.read(path), written.replace(SHA, other));
    assert_eq!(repo.marker_bytes(), marker, "the entry is there already");

    // A file of the project at the path is never rewritten, with or without --force.
    let mine = "name: mine\n";
    repo.write(path, mine);
    for extra in [vec![], vec!["--force"]] {
        let mut args = vec!["write", "--host", "github", "--sha", SHA];
        args.extend(extra);
        assert_output(&repo.ci(&args), 1, "", &format!("{}\n", exists(path)));
        assert_eq!(repo.read(path), mine);
    }
    // A directory at the path is no file of its own either.
    fs::remove_file(repo.at(path)).expect("remove the file");
    fs::create_dir(repo.at(path)).expect("create a directory");
    assert_output(
        &repo.ci(&["write", "--host", "github", "--sha", SHA, "--force"]),
        1,
        "",
        &format!("{}\n", exists(path)),
    );
}

/// A refused write changes neither the file nor the marker, and a first write that finds the marker
/// with a fault names the fault and writes no file.
#[test]
fn a_refused_write_changes_nothing() {
    let repo = Repo::new("", None, json!({"review": {"ci": {"files": "not a list"}}}));
    let marker = repo.marker_bytes();
    let output = repo.ci(&["write", "--host", "github", "--sha", SHA]);
    assert_output(
        &output,
        2,
        "",
        "ci: .houserules.json: review.ci.files must be an array of strings\n",
    );
    assert_eq!(repo.status(), "", "nothing was written");
    assert_eq!(repo.marker_bytes(), marker);
}

/// Spec 10.3: `--out` names another path below the top level, and the marker lists that path.
#[test]
fn write_with_out_names_another_path() {
    let repo = Repo::bare();
    let output = repo.ci(&[
        "write",
        "--host",
        "github",
        "--mode",
        "inline",
        "--out",
        ".github/workflows/review-houserules.yml",
    ]);
    assert_output(&output, 0, "", "");
    let path = ".github/workflows/review-houserules.yml";
    assert!(repo.read(path).starts_with(&format!(
        "{WRITTEN_BY}\n# houserules {VERSION}\nname: houserules review\n"
    )));
    assert_eq!(repo.marker()["review"]["ci"]["files"], json!([path]));
    assert!(!repo.at(".github/workflows/houserules-review.yml").exists());
}

/// A path that leaves the top level, or that is no relative path, is refused before anything is
/// written.
#[test]
fn out_must_name_a_path_below_the_top_level() {
    let repo = Repo::bare();
    for bad in ["../x.yml", "a/../../x.yml", "/tmp/x.yml", "."] {
        let output = repo.ci(&["write", "--host", "gitlab", "--out", bad]);
        assert_output(
            &output,
            2,
            "",
            "ci write: --out must name a path below the top level of the repository, with no .. part\n",
        );
    }
    assert_eq!(repo.status(), "");
}

/// An `--out` with a part that is no UTF-8 is refused before anything is written, with one line
/// that has the prefix of the other `--out` line.
// A non-UTF-8 `--out` path exists on Unix only. The path is never created, so a file system that
// refuses such a name (APFS) is not involved.
#[cfg(unix)]
#[test]
fn an_out_path_that_is_not_utf8_is_refused_with_one_line() {
    use std::os::unix::ffi::OsStrExt;

    let repo = Repo::bare();
    let out = std::path::Path::new(std::ffi::OsStr::from_bytes(b"ci/a\xffb.yml"));
    let output = houserules()
        .args(["ci", "write", "--host", "gitlab", "--out"])
        .arg(out)
        .arg("--dir")
        .arg(repo.root())
        .output()
        .expect("run houserules ci write");
    assert_output(
        &output,
        2,
        "",
        &format!(
            "ci write: {}: the path holds a name that is no UTF-8\n",
            out.display()
        ),
    );
    assert_eq!(repo.status(), "", "nothing was written");
}

/// A git failure while the GitHub forms look for the default branch is one line with the command
/// and git's own text, exit 2, and nothing on stdout: here `git symbolic-ref` fails on the
/// `origin/HEAD` that names the branch.
// The failing git is a shell script on `PATH`, which needs the Unix executable bit.
#[cfg(unix)]
#[test]
fn a_git_failure_of_the_default_branch_lookup_is_one_line() {
    use std::os::unix::fs::PermissionsExt;

    let repo = Repo::bare();
    git(
        repo.top.path(),
        &["update-ref", "refs/remotes/origin/master", "HEAD"],
    );
    git(
        repo.top.path(),
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/master",
        ],
    );
    let path = std::env::var_os("PATH").expect("PATH is set");
    let real_git = std::env::split_paths(&path)
        .map(|folder| folder.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git on PATH");
    let shims = TempDir::new().expect("a folder for the git shim");
    let shim = shims.path().join("git");
    fs::write(
        &shim,
        format!(
            "#!/bin/sh\nif [ \"$1\" = symbolic-ref ]; then echo 'fatal: probe failure' >&2; exit 128; fi\nexec '{}' \"$@\"\n",
            real_git.display()
        ),
    )
    .expect("write the git shim");
    fs::set_permissions(&shim, fs::Permissions::from_mode(0o755)).expect("make the shim runnable");
    let mut search = vec![shims.path().to_path_buf()];
    search.extend(std::env::split_paths(&path));
    let output = houserules()
        .args(["ci", "print", "--host", "github", "--dir"])
        .arg(repo.root())
        .env("PATH", std::env::join_paths(search).expect("join PATH"))
        .output()
        .expect("run houserules ci print");
    assert_output(
        &output,
        2,
        "",
        "ci: git symbolic-ref --quiet refs/remotes/origin/HEAD: fatal: probe failure\n",
    );
}

/// Spec 10.3 and case 8: `ci write --host gitlab` writes its file and prints two lines: the include
/// line for `.gitlab-ci.yml`, and the line about `review.ci.files`. The include line names the file
/// that was written.
#[test]
fn write_gitlab_prints_the_include_line_and_the_ci_files_line() {
    let repo = Repo::bare();
    let output = repo.ci(&["write", "--host", "gitlab", "--mode", "inline"]);
    let path = ".gitlab/houserules-review.gitlab-ci.yml";
    assert_output(
        &output,
        0,
        &format!("include: [{{ local: \"{path}\" }}]\n{GITLAB_CI_FILES_LINE}\n"),
        "",
    );
    let written = repo.read(path);
    let published = parse_unit(&text_of(GITLAB_UNIT), Host::gitlab("root"));
    assert_eq!(
        written,
        format!(
            "{WRITTEN_BY}\n# houserules {VERSION}\n{}",
            expected_gitlab_inline(&published, ".", "root", "")
        )
    );
    assert_eq!(repo.marker()["review"]["ci"]["files"], json!([path]));

    let other = Repo::bare();
    let output = other.ci(&["write", "--host", "gitlab", "--out", "ci/review.yml"]);
    assert_output(
        &output,
        0,
        &format!("include: [{{ local: \"ci/review.yml\" }}]\n{GITLAB_CI_FILES_LINE}\n"),
        "",
    );
    assert_eq!(
        other.read("ci/review.yml"),
        format!(
            "{WRITTEN_BY}\n# houserules {VERSION}\n{}",
            gitlab_reference(".", "root", None)
        )
    );
}

/// A file of `ci write --mode inline` for GitHub is the inline form behind the two comment lines,
/// and passes `actionlint`.
#[test]
fn write_inline_github_writes_the_inline_form_behind_its_comment_lines() {
    let repo = Repo::bare();
    assert_output(
        &repo.ci(&["write", "--host", "github", "--mode", "inline"]),
        0,
        "",
        "",
    );
    let written = repo.read(".github/workflows/houserules-review.yml");
    let body = written
        .strip_prefix(&format!("{WRITTEN_BY}\n# houserules {VERSION}\n"))
        .expect("the two comment lines");
    assert_eq!(
        body,
        stdout(&repo.ci(&["print", "--host", "github", "--mode", "inline"])),
        "the file holds the text that print prints"
    );
    assert_eq!(
        parse_unit(body, Host::GitHub).body(),
        &expected_github_inline(".", "root", "", "main")
    );
    assert_workflow_passes_actionlint(&written);
    assert_wiring_trust_rules(&parse_unit(&written, Host::GitHub));
}

/// Spec 10.3: `update` finds a file of `ci write` by its first two lines, so the file starts with
/// exactly them and the version line names the binary.
#[test]
fn a_written_file_starts_with_the_two_lines_that_update_reads() {
    let repo = Repo::bare();
    repo.ci(&["write", "--host", "gitlab"]);
    let written = repo.read(".gitlab/houserules-review.gitlab-ci.yml");
    let mut lines = written.lines();
    assert_eq!(lines.next(), Some(WRITTEN_BY));
    assert_eq!(
        lines.next(),
        Some(with(VERSION_COMMENT, "<version>", VERSION).as_str())
    );
}

// ---- write: the template ----

/// Spec 7.4: `ci write --template` writes only the template of the host, with the section of the
/// spec and the pointer, and the line on what the host shows first. It adds nothing to
/// `review.ci.files`.
#[test]
fn write_template_writes_only_the_template_of_the_host() {
    for (host, path) in [
        ("github", ".github/pull_request_template.md"),
        ("gitlab", ".gitlab/merge_request_templates/Default.md"),
    ] {
        let repo = Repo::bare();
        let marker = repo.marker_bytes();
        let output = repo.ci(&["write", "--host", host, "--template"]);
        assert_output(&output, 0, &format!("{}\n", priority(path)), "");
        assert_eq!(repo.read(path), written_template());
        assert_eq!(
            repo.marker_bytes(),
            marker,
            "{host}: nothing in review.ci.files"
        );
        assert_eq!(repo.status().lines().count(), 1, "{}", repo.status());
    }
}

/// Spec 7.4: `ci write --template` writes nothing and exits 1 with its line where the host reads a
/// template already: for GitHub `pull_request_template.md` in the root, `docs/`, and `.github/` in
/// any case, and the folder `PULL_REQUEST_TEMPLATE/` in those three places; for GitLab any `.md`
/// file in `.gitlab/merge_request_templates/`.
#[test]
fn write_template_refuses_where_a_template_exists() {
    let github = [
        "pull_request_template.md",
        "PULL_REQUEST_TEMPLATE.md",
        "Pull_Request_Template.md",
        "docs/pull_request_template.md",
        "docs/PULL_REQUEST_TEMPLATE.md",
        ".github/pull_request_template.md",
        ".github/PULL_REQUEST_TEMPLATE.md",
        "PULL_REQUEST_TEMPLATE/feature.md",
        "pull_request_template/feature.md",
        "docs/PULL_REQUEST_TEMPLATE/feature.md",
        ".github/PULL_REQUEST_TEMPLATE/feature.md",
    ];
    for existing in github {
        let repo = Repo::bare();
        repo.write(existing, "Existing.\n");
        commit_all(repo.top.path(), "docs: template");
        let output = repo.ci(&["write", "--host", "github", "--template"]);
        let shown = existing
            .rsplit_once('/')
            .filter(|(folder, _)| folder.to_lowercase().ends_with("pull_request_template"))
            .map_or(existing, |(folder, _)| folder);
        assert_output(&output, 1, "", &format!("{}\n", template_exists(shown)));
        assert_eq!(repo.status(), "", "{existing}: nothing was written");
    }
    let repo = Repo::bare();
    repo.write(".gitlab/merge_request_templates/Bug.md", "Existing.\n");
    commit_all(repo.top.path(), "docs: template");
    assert_output(
        &repo.ci(&["write", "--host", "gitlab", "--template"]),
        1,
        "",
        &format!(
            "{}\n",
            template_exists(".gitlab/merge_request_templates/Bug.md")
        ),
    );
    assert_eq!(repo.status(), "");
}

/// A template of the other host, or an empty template folder, is no template where this host reads.
#[test]
fn write_template_ignores_what_the_host_does_not_read() {
    let repo = Repo::bare();
    repo.write(".gitlab/merge_request_templates/Bug.md", "Existing.\n");
    fs::create_dir_all(repo.at(".github/PULL_REQUEST_TEMPLATE")).expect("an empty folder");
    repo.write("docs/notes/pull_request_template.md", "Not read.\n");
    commit_all(repo.top.path(), "docs: other");
    let output = repo.ci(&["write", "--host", "github", "--template"]);
    assert_output(
        &output,
        0,
        &format!("{}\n", priority(".github/pull_request_template.md")),
        "",
    );
}

// ---- the key ----

/// Spec 10.1 and design.md 258: two installs of a repository with the same key are refused by
/// `print` and `write`, with the line that names both paths, and nothing is written.
#[test]
fn two_installs_that_share_a_key_are_refused() {
    let repo = Repo::new("a/b", None, json!({}));
    repo.write("a-b/.houserules.json", &repo.read("a/b/.houserules.json"));
    commit_all(repo.top.path(), "chore: second install");
    let marker = repo.marker_bytes();
    for args in [
        vec!["print", "--host", "gitlab"],
        vec!["print", "--host", "github", "--mode", "inline"],
        vec!["write", "--host", "gitlab"],
        vec!["write", "--host", "github", "--sha", SHA],
    ] {
        assert_output(
            &repo.ci(&args),
            2,
            "",
            &format!("{}\n", shared_key("a/b", "a-b", "a-b")),
        );
    }
    assert_eq!(repo.status(), "");
    assert_eq!(repo.marker_bytes(), marker);

    // The install at the top level has the key root, which a folder named root shares.
    let top = Repo::bare();
    top.write("root/.houserules.json", &top.read(".houserules.json"));
    commit_all(top.top.path(), "chore: second install");
    assert_output(
        &top.ci(&["print", "--host", "gitlab"]),
        2,
        "",
        &format!("{}\n", shared_key(".", "root", "root")),
    );
}

/// Two installs with distinct keys are no clash, and each gets its own job names.
#[test]
fn two_installs_with_distinct_keys_are_no_clash() {
    let repo = Repo::new("docs", None, json!({}));
    repo.write(".houserules.json", &repo.read("docs/.houserules.json"));
    repo.write(
        "docs.site/.houserules.json",
        &repo.read("docs/.houserules.json"),
    );
    commit_all(repo.top.path(), "chore: more installs");
    let output = repo.ci(&["print", "--host", "gitlab", "--mode", "inline"]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(stdout(&output).contains("houserules-docs-gates:"));
}

// ---- the whole command ----

/// Spec 10.3 and the rule of `houserules.writers-render-after-a-change`: after `ci write` the gates
/// of the install pass, so the write left no generated file stale and no marker in a bad shape.
#[test]
fn the_gates_pass_after_a_write() {
    let install = install_without_backlog();
    // A GitHub form names the default branch, which a repository holds from its first commit.
    git(install.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    commit_all(install.path(), "chore: init");
    for args in [
        vec!["ci", "write", "--host", "gitlab", "--out", "ci/review.yml"],
        vec!["ci", "write", "--host", "github", "--sha", SHA],
    ] {
        let output = run(install.path(), &args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    for args in [vec!["check-knowledge"], vec!["render", "--check"]] {
        let output = run(install.path(), &args);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{args:?}: {}",
            stderr(&output)
        );
    }
    let marker: Value = serde_json::from_str(
        &fs::read_to_string(install.path().join(".houserules.json")).expect("read the marker"),
    )
    .expect("parse");
    assert_eq!(
        marker["review"]["ci"]["files"],
        json!(["ci/review.yml", ".github/workflows/houserules-review.yml"])
    );
}

/// Spec 10.3: `update` reads the second line of a file of `ci write` and, when the version differs
/// from the binary's, prints the pin line and writes nothing; `ci write --force` is the remedy that
/// the line names, and after it `update` prints no pin line.
#[test]
fn update_names_a_written_file_of_another_version_and_force_rewrites_it() {
    let install = install_without_backlog();
    let path = "ci/review.yml";
    let args = ["ci", "write", "--host", "gitlab", "--out", path];
    assert_eq!(run(install.path(), &args).status.code(), Some(0));
    let written = file(install.path(), path);
    let text = fs::read_to_string(&written).expect("read the file");
    let old = text.replacen(&format!("# houserules {VERSION}"), "# houserules 0.9.0", 1);
    assert_ne!(old, text);
    fs::write(&written, &old).expect("write the old version");
    commit_all(install.path(), "chore: wire the review layer");
    let first = stdout(&run(install.path(), &["update"]));
    assert!(
        first.lines().any(|line| line == pin_line(path, "0.9.0")),
        "{first}"
    );
    assert_eq!(
        fs::read_to_string(&written).expect("read"),
        old,
        "update wrote nothing"
    );

    let mut forced = args.to_vec();
    forced.push("--force");
    assert_eq!(run(install.path(), &forced).status.code(), Some(0));
    assert_eq!(fs::read_to_string(&written).expect("read"), text);
    commit_all(install.path(), "chore: rewrite the review layer");
    let second = stdout(&run(install.path(), &["update"]));
    assert!(!second.contains(" pins houserules "), "{second}");
}

/// Spec 10.3: the pin line of a GitHub reference file says that the rewrite takes the `--sha` of the
/// new release. The command that the line names, with the flags that wrote the file and the new
/// `--sha`, rewrites the file and exits 0, and `update` prints no pin line afterwards. The same
/// command with the flags of the file but no `--sha` is exit 2, because the reference form pins the
/// reusable workflow by the commit of the release.
#[test]
fn the_pin_line_of_a_github_reference_file_is_followed_with_the_flags_and_the_new_sha() {
    let install = install_without_backlog();
    git(install.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    commit_all(install.path(), "chore: init");
    let path = ".github/workflows/houserules-review.yml";
    let flags = ["--host", "github"];
    let mut write = vec!["ci", "write"];
    write.extend(flags);
    write.extend(["--sha", SHA]);
    assert_eq!(run(install.path(), &write).status.code(), Some(0));
    let written = file(install.path(), path);
    let text = fs::read_to_string(&written).expect("read the file");
    let old = text.replacen(&format!("# houserules {VERSION}"), "# houserules 0.9.0", 1);
    assert_ne!(old, text);
    fs::write(&written, &old).expect("write the old version");
    commit_all(install.path(), "chore: wire the review layer");
    let first = stdout(&run(install.path(), &["update"]));
    assert!(
        first.lines().any(|line| line == pin_line(path, "0.9.0")),
        "{first}"
    );

    let mut without_sha = vec!["ci", "write"];
    without_sha.extend(flags);
    without_sha.push("--force");
    assert_output(
        &run(install.path(), &without_sha),
        2,
        "",
        &format!("{}\n", sha_required()),
    );
    assert_eq!(fs::read_to_string(&written).expect("read"), old);

    let new_sha = "1111111111111111111111111111111111111111";
    let mut forced = vec!["ci", "write"];
    forced.extend(flags);
    forced.extend(["--sha", new_sha, "--force"]);
    assert_eq!(run(install.path(), &forced).status.code(), Some(0));
    assert_eq!(
        fs::read_to_string(&written).expect("read"),
        text.replace(SHA, new_sha)
    );
    commit_all(install.path(), "chore: rewrite the review layer");
    let second = stdout(&run(install.path(), &["update"]));
    assert!(!second.contains(" pins houserules "), "{second}");
}

/// Case 8 and the determinism rule of the batch: a command writes the same bytes for the same
/// inputs, for every form, on stdout and in a file.
#[test]
fn every_form_is_the_same_bytes_on_a_second_run() {
    let repo = Repo::bare();
    for args in [
        vec!["print", "--host", "github"],
        vec!["print", "--host", "gitlab"],
        vec!["print", "--host", "github", "--mode", "inline"],
        vec!["print", "--host", "gitlab", "--mode", "inline"],
        vec!["print", "--host", "gitlab", "--template"],
    ] {
        let first = repo.ci(&args);
        let second = repo.ci(&args);
        assert_eq!(first.status.code(), Some(0), "{args:?}");
        assert_eq!(first.stdout, second.stdout, "{args:?}");
        assert_eq!(first.stderr, second.stderr, "{args:?}");
    }
    let (one, two) = (Repo::bare(), Repo::bare());
    let (a, b) = (
        one.ci(&["write", "--host", "gitlab", "--mode", "inline"]),
        two.ci(&["write", "--host", "gitlab", "--mode", "inline"]),
    );
    assert_eq!(a.stdout, b.stdout);
    assert_eq!(
        one.read(".gitlab/houserules-review.gitlab-ci.yml"),
        two.read(".gitlab/houserules-review.gitlab-ci.yml")
    );
}

/// Case 8: no form, and no source of the command, holds `github.com` outside this repository's own
/// URLs (the `ls-remote` line and the release download) and the Beads release download of the Beads
/// tracker form. The origin host detection of `ci` lives in `install`, not in these files.
#[test]
fn no_form_and_no_source_holds_github_com_outside_the_listed_uses() {
    let repo = Repo::new("", None, json!({"tracker": {"adapter": "beads"}}));
    for args in [
        vec!["print", "--host", "github"],
        vec!["print", "--host", "gitlab"],
        vec!["print", "--host", "github", "--mode", "inline"],
        vec!["print", "--host", "gitlab", "--mode", "inline"],
        vec!["print", "--host", "gitlab", "--template"],
    ] {
        let output = repo.ci(&args);
        let printed = format!("{}{}", stdout(&output), stderr(&output));
        assert!(
            stray_github_hosts(&printed).is_empty(),
            "{args:?}: {:?}",
            stray_github_hosts(&printed)
        );
    }
    let mut sources = vec![text_of("crates/houserules/src/review/ci.rs")];
    for entry in fs::read_dir(file(&repo_root(), "crates/houserules/src/review/ci")).expect("ci/") {
        let path = entry.expect("an entry").path();
        sources.push(fs::read_to_string(path).expect("read a source"));
    }
    for source in sources {
        assert_eq!(stray_github_hosts(&source), Vec::<String>::new());
    }
}

/// Help is the contract of the command: it names what each subcommand does, the files it writes, its
/// exit codes, and the flags.
#[test]
fn the_help_names_what_the_command_does() {
    states(
        "ci --help",
        &help_of_args(&["ci", "--help"]),
        &[
            "print",
            "write",
            "never edits a CI file that it did not create",
        ],
    );
    states(
        "ci print --help",
        &help_of_args(&["ci", "print", "--help"]),
        &[
            "writes nothing",
            "--host",
            "--mode",
            "--template",
            "review.ci.files",
            "origin",
            "exits 2",
            "push to the default branch",
            "`defaultBranch`",
            "`origin/HEAD`",
        ],
    );
    states(
        "ci write --help",
        &help_of_args(&["ci", "write", "--help"]),
        &[
            "writes only a file of its own",
            "--sha",
            "--force",
            "--out",
            "--template",
            "review.ci.files",
            "exits 1",
            "exits 2",
            "push to the default branch",
            "`defaultBranch`",
            "`origin/HEAD`",
        ],
    );
}

/// The `ci` subcommands read the install like every other command: outside a repository they name
/// the `--dir` remedy, as `check_parity.rs` pins for all of them, and in a repository with no
/// install they name the install (D4).
#[test]
fn ci_in_a_tree_with_no_install_names_the_install() {
    let dir = scratch_git_repo();
    let output = houserules()
        .args(["ci", "print", "--host", "github", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run ci print");
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains(".houserules.json"),
        "{}",
        stderr(&output)
    );
}

/// The values of `--host` and `--mode` that clap accepts are the ones of the spec, `--template`
/// takes no other form flag, and `print` takes no `--sha`: each misuse is a usage error, exit 2,
/// that names the argument.
#[test]
fn a_misused_flag_is_a_usage_error_that_names_it() {
    let repo = Repo::bare();
    for (args, named) in [
        (
            vec!["print", "--host", "bitbucket"],
            "invalid value 'bitbucket'",
        ),
        (
            vec!["print", "--host", "github", "--mode", "fancy"],
            "invalid value 'fancy'",
        ),
        (
            vec![
                "print",
                "--host",
                "github",
                "--template",
                "--mode",
                "inline",
            ],
            "cannot be used with",
        ),
        (
            vec!["print", "--host", "github", "--sha", SHA],
            "unexpected argument '--sha'",
        ),
        (
            vec!["write", "--host", "github", "--template", "--force"],
            "cannot be used with",
        ),
    ] {
        let output = repo.ci(&args);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{args:?}: {}",
            stderr(&output)
        );
        assert_eq!(stdout(&output), "", "{args:?}");
        assert!(
            stderr(&output).contains(named),
            "{args:?}: {}",
            stderr(&output)
        );
        for flag in args
            .iter()
            .filter(|word| ["--template", "--mode", "--force"].contains(word))
        {
            assert!(
                named != "cannot be used with" || stderr(&output).contains(flag),
                "{args:?}: the error names {flag}: {}",
                stderr(&output)
            );
        }
    }
}
