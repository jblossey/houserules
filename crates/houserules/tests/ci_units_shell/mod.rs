//! The scripts of the review units, run for real (spec 10.1, 10.4 to 10.6, and
//! acceptance case 8). Each test takes the script of a job from the parsed unit
//! and runs it in a sandbox, with a stub `gh` and a stub `curl` on the `PATH`
//! that record every call and answer from files, a real `git` against a local
//! bare remote, and the environment that the runner would give the job.
//!
//! One file holds the tests of one script: `shellcheck` (every shell text),
//! `install` (the install step and the install lines of the GitLab scripts),
//! `route` (the `applied_route` logic), `report_github` and `report_gitlab`
//! (the report jobs), `push` (the push of the record and the reviewers
//! program), `record` (the record jobs and the notes ref), `gates` (the
//! evidence and gates scripts and the floor scripts), and `floor_build` (the floor
//! build of the default branch with the real `cargo`). This file holds what several
//! of them share.
//!
//! The module is Unix-only: the units run `jq` and `sh` on Linux images only.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};

use crate::ci_support::sandbox::Sandbox;
use crate::ci_support::{ROLES, Unit};
use crate::tracker_support::{git, repo_root, stderr, stdout};
use crate::{github, gitlab};

/// The head of the request in the report tests.
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A head that the request has moved to.
const MOVED: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// Spec 10.2: the line of a release install without a version.
const VERSION_REQUIRED: &str =
    "houserules: the version input is required when install or floor-install is release";

/// Batch 38, spec 4.3: the notice that a unit prints for a `validation` input whose file does
/// not exist, with the path that the unit looked for.
fn validation_notice(path: &str) -> String {
    format!(
        "houserules: the validation input names {path}, which does not exist, so this run passes no validation capture"
    )
}

/// The file of the spec of batch 38, below `docs/specs/`.
const SPEC_FILE: &str = "2026-10-25-batch-38-openspec-and-release.md";

/// The notice of spec 4.3 that the tests pin is the notice that the spec gives, with `<path>`
/// for the path (`quality.user-facing-text-is-contract`).
#[test]
fn the_validation_notice_is_in_the_spec() {
    let spec = fs::read_to_string(repo_root().join("docs").join("specs").join(SPEC_FILE))
        .expect("read the spec");

    let notice = format!("`{}`", validation_notice("<path>"));
    assert!(
        spec.contains(&notice),
        "the spec does not hold this notice: {notice}"
    );
}

/// What the setup command left for the `validation` input below the work directory.
enum Left {
    /// A file at the path.
    File(&'static str),
    /// A folder at the path.
    Folder(&'static str),
}

/// One case of the `validation` input of a job that runs `evidence` or `record write` (spec
/// 4.3).
struct ValidationCase {
    /// The install folder, the `dir` input.
    dir: &'static str,
    /// The `validation` input.
    input: &'static str,
    /// What the setup command left below the work directory.
    left: Option<Left>,
    /// The path that the unit passes as `--validation`, or `None` for no flag.
    passed: Option<&'static str>,
    /// The path that the notice names, or `None` for no notice.
    notice: Option<&'static str>,
}

/// The cases of the `validation` input of a gates job. The path is the input below the install
/// folder, with the `./` of the folder `.` kept, and a file that lies outside the install, a
/// folder, and an empty input pass no flag.
fn validation_cases() -> Vec<ValidationCase> {
    let case = |dir, input, left, passed, notice| ValidationCase {
        dir,
        input,
        left,
        passed,
        notice,
    };
    vec![
        case(".", "", None, None, None),
        case(".", "", Some(Left::File("validation.json")), None, None),
        case(
            ".",
            "validation.json",
            Some(Left::File("validation.json")),
            Some("./validation.json"),
            None,
        ),
        case(
            ".",
            "validation.json",
            None,
            None,
            Some("./validation.json"),
        ),
        case(
            "sub",
            "out/validation.json",
            Some(Left::File("sub/out/validation.json")),
            Some("sub/out/validation.json"),
            None,
        ),
        case(
            "sub/",
            "validation.json",
            Some(Left::File("sub/validation.json")),
            Some("sub/validation.json"),
            None,
        ),
        case(
            "sub",
            "validation.json",
            Some(Left::Folder("sub/validation.json")),
            None,
            Some("sub/validation.json"),
        ),
        case(
            "sub",
            "validation.json",
            Some(Left::File("validation.json")),
            None,
            Some("sub/validation.json"),
        ),
    ]
}

/// Leaves `left` below the work directory `work`, with the folders above it.
fn leave(work: &Path, left: &Left) {
    let (path, is_folder) = match left {
        Left::File(path) => (path, false),
        Left::Folder(path) => (path, true),
    };
    let target = path
        .split('/')
        .fold(work.to_path_buf(), |folder, part| folder.join(part));
    if is_folder {
        fs::create_dir_all(&target).expect("create the folder");
    } else {
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(&target, "a capture\n").expect("write the file");
    }
}

/// The environment of `unit` for the job `role`, with the pairs of `extra`
/// replacing the values of the unit.
fn env_of(unit: &Unit, role: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut env = unit.static_env(role);
    for (name, value) in extra {
        env.retain(|(existing, _)| existing != name);
        env.push((name.to_string(), value.to_string()));
    }
    env
}

/// The path `path` as text.
fn text(path: &Path) -> String {
    path.to_str().expect("a UTF-8 path").to_string()
}

/// The route document with the route `route`.
fn route_doc(route: &str) -> String {
    json!({"route": route}).to_string()
}

mod floor_build;
mod gates;
mod install;
mod push;
mod record;
mod report_github;
mod report_gitlab;
mod route;
mod shellcheck;

/// The packet that the gates job wrote.
const PACKET: &str = "<!-- houserules:evidence . -->\n## houserules evidence\n\nRange aaaaaaa..bbbbbbb (merge base ccccccc) · rules 0123456789ab · route owner-review\n\n### Uncovered surface (0 of 0 changed lines)\n\nNone.\n";

/// Writes the evidence of the gates job for the install `key` into the sandbox:
/// the route document, the packet, and a floor document under the name of the
/// floor that the gates job has no right to write.
fn put_evidence(
    root: &Path,
    key: &str,
    gates_route: Option<&str>,
    packet: Option<&str>,
) -> PathBuf {
    let directory = root.join(format!("houserules-evidence-{key}"));
    fs::create_dir_all(&directory).expect("create the evidence folder");
    if let Some(route) = gates_route {
        fs::write(directory.join("route.json"), route_doc(route)).expect("write route.json");
    }
    if let Some(packet) = packet {
        fs::write(directory.join("evidence.md"), packet).expect("write evidence.md");
    }
    directory
}

/// A real git repository in `directory`: an `origin` remote (a bare repository
/// at `remote`) and two commits; returns the first and the second commit.
fn repository_with_origin(directory: &Path, remote: &Path) -> (String, String) {
    fs::create_dir_all(remote).expect("create the remote folder");
    git(remote, &["init", "-q", "--bare"]);
    fs::create_dir_all(directory).expect("create the work folder");
    git(directory, &["init", "-q"]);
    git(directory, &["remote", "add", "origin", &text(remote)]);
    fs::write(directory.join("one.txt"), "one\n").expect("write a file");
    git(directory, &["add", "."]);
    git(directory, &["commit", "-q", "-m", "one"]);
    let first = git(directory, &["rev-parse", "HEAD"]).trim().to_string();
    fs::write(directory.join("two.txt"), "two\n").expect("write a file");
    git(directory, &["add", "."]);
    git(directory, &["commit", "-q", "-m", "two"]);
    let second = git(directory, &["rev-parse", "HEAD"]).trim().to_string();
    git(directory, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    (first, second)
}

/// The JSON body that the stub kept for the call that holds `needle`.
fn body_of(sandbox: &Sandbox, needle: &str) -> Value {
    serde_json::from_str(&sandbox.input(sandbox.call_number(needle))).expect("a JSON body")
}

/// The lines of the log of the stub `houserules`.
fn houserules_log(sandbox: &Sandbox) -> Vec<String> {
    fs::read_to_string(sandbox.stub_dir().join("houserules.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}
