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
use crate::tracker_support::{git, stderr, stdout};
use crate::{github, gitlab};

/// The head of the request in the report tests.
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A head that the request has moved to.
const MOVED: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// Spec 10.2: the line of a release install without a version.
const VERSION_REQUIRED: &str =
    "houserules: the version input is required when install or floor-install is release";

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
