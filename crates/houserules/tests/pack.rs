//! `houserules pack list|add|remove|upgrade`, run through the compiled
//! binary against scratch git repositories. A git source resolves through
//! a local bare repository with tags (a `file://` URL), so no test needs a
//! network. Every refusal pins its whole stderr line and leaves the tree
//! as it was (`quality.exclusive-pins-are-mutation-proven`).

mod tracker_support;
mod tree_snapshot;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{pack_pin, repo_root};

/// A `Command` for the compiled `houserules` binary under test.
/// `HOUSERULES_SKIP_SELF_UPDATE` keeps `update`'s self-update phase from
/// running; no `pack` command reaches it.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// Runs `git <args>` in `dir` with a fixed identity and no signing, fails
/// the test on a non-zero exit, and returns the trimmed stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// `value` as `emit` writes it: two-space indent and a trailing newline.
fn emit_form(value: &Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("serialize")
    )
}

/// One schema-valid entry in the `global` area.
fn entry(id: &str, summary: &str) -> Value {
    json!({
        "id": id,
        "kind": "rule",
        "area": "global",
        "standing": true,
        "summary": summary,
        "body": ["Body."],
        "tags": [],
        "source": {"date": "2026-10-19", "by": "controller"},
    })
}

/// A pack that serves `topics` and declares `areas`.
fn pack_value(
    name: &str,
    version: &str,
    topics: &[&str],
    areas: &Value,
    entries: &[Value],
) -> Value {
    let served: serde_json::Map<String, Value> = topics
        .iter()
        .map(|topic| {
            (
                topic.to_string(),
                json!({"title": format!("{topic} title")}),
            )
        })
        .collect();
    json!({
        "name": name,
        "version": version,
        "schema": "houserules/pack@1.0.0",
        "topics": served,
        "areas": areas,
        "entries": entries,
    })
}

/// A pack of one topic of its own name, declaring only `global`.
fn simple_pack(name: &str, version: &str, entries: &[Value]) -> Value {
    pack_value(
        name,
        version,
        &[name],
        &json!({"global": {"paths": []}}),
        entries,
    )
}

/// The `areas.json` of a gated project: the four areas of the kit's schema
/// on one line, so no emit-form write can pass for an untouched file.
const HAND_FORMATTED_AREAS: &str = r#"{"global":{"paths":[]},"process":{"paths":[]},"docs":{"paths":["docs/**"]},"tools":{"paths":["tools/**"]}}"#;

/// A scratch git repository holding the smallest install `pack` reads.
struct Project {
    dir: TempDir,
}

impl Project {
    /// `git init`, a 2.0 marker, an empty `schema.json`, an `areas.json`
    /// with `global` and `docs`, and two tracked files, so `git ls-files` has
    /// an answer.
    fn new() -> Self {
        let project = Self {
            dir: tempfile::tempdir().expect("tempdir"),
        };
        git(project.root(), &["init", "-q"]);
        project.write_json(
            ".houserules.json",
            &json!({
                "schema": "houserules/install@2.0.0",
                "version": "1.4.0",
                "idPrefix": "WI",
                "baselines": {},
            }),
        );
        project.write("knowledge/schema.json", "{}\n");
        project.write_json(
            "knowledge/areas.json",
            &json!({"global": {"paths": []}, "docs": {"paths": ["docs/**"]}}),
        );
        project.write("docs/a.md", "a\n");
        project.write("tools/run.sh", "run\n");
        git(project.root(), &["add", "-A"]);
        project
    }

    /// `new`, plus what both gates read: the kit's `knowledge/schema.json`,
    /// the four areas it names in a hand-formatted `areas.json` (one line,
    /// not emit form), an `AGENTS.md`, and the generated files rendered and
    /// tracked. `render --check` and `check-knowledge` pass on it.
    fn gated() -> Self {
        let project = Self::new();
        project.write(
            "knowledge/schema.json",
            &fs::read_to_string(
                repo_root()
                    .join("template")
                    .join("knowledge")
                    .join("schema.json"),
            )
            .expect("read the schema"),
        );
        project.write("knowledge/areas.json", HAND_FORMATTED_AREAS);
        project.write("AGENTS.md", "# Agents\n");
        git(project.root(), &["add", "-A"]);
        succeeded(&project.run(&["render"]));
        git(project.root(), &["add", "-A"]);
        project.assert_gates_green();
        project
    }

    /// Asserts that `render --check` and `check-knowledge` both pass.
    fn assert_gates_green(&self) {
        let rendered = self.run(&["render", "--check"]);
        assert_eq!(
            rendered.status.code(),
            Some(0),
            "render --check: {}",
            stderr_text(&rendered)
        );
        assert_eq!(stdout_lines(&rendered), ["render: up to date"]);
        let checked = self.run(&["check-knowledge"]);
        assert_eq!(
            checked.status.code(),
            Some(0),
            "check-knowledge: {}",
            stderr_text(&checked)
        );
        assert_eq!(stdout_lines(&checked), ["knowledge: ok"]);
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// The path of `relative` (`/`-separated) under the root, built one
    /// component at a time (`houserules.path-pins-mirror-the-code`).
    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root().to_path_buf(), |path, part| path.join(part))
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().expect("a parent")).expect("create the parent");
        fs::write(path, content).expect("write a file");
    }

    fn write_json(&self, relative: &str, value: &Value) {
        self.write(relative, &emit_form(value));
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path(relative)).expect("read a file")
    }

    fn read_json(&self, relative: &str) -> Value {
        serde_json::from_str(&self.read(relative)).expect("parse a JSON file")
    }

    fn exists(&self, relative: &str) -> bool {
        self.path(relative).exists()
    }

    /// Installs a pack copy and its pin by hand, as an earlier `pack add`
    /// would have left them.
    fn install_pack(&self, copy: &Value, source: &str, added_areas: &[&str]) {
        let name = copy["name"].as_str().expect("a name");
        self.write_json(&format!("knowledge/packs/{name}.json"), copy);
        let mut marker = self.read_json(".houserules.json");
        marker["packs"][name] = json!({
            "version": copy["version"],
            "source": source,
            "sha256": pack_pin(copy),
            "addedAreas": added_areas,
        });
        self.write_json(".houserules.json", &marker);
    }

    /// Runs `houserules <args> --dir <root>`.
    fn run(&self, args: &[&str]) -> Output {
        houserules()
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run houserules")
    }

    /// Every file under the root except `.git` (`tree_snapshot::snapshot`).
    fn snapshot(&self) -> tree_snapshot::Tree {
        tree_snapshot::snapshot(self.root())
    }
}

fn stdout_lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// `true` for a line that a render run prints: `render: up to date`, or
/// `<path>: written`, `<path>: block removed`, or `<path>: deleted` for each
/// file it changes.
fn is_render_line(line: &str) -> bool {
    line == "render: up to date"
        || [": written", ": block removed", ": deleted"]
            .iter()
            .any(|ending| line.ends_with(ending))
}

/// Asserts that `output` is a success and returns its stdout lines in two
/// parts: the receipt of the command, then the report of the render that
/// ends a command that changes files.
fn succeeded_with_render_report(output: &Output) -> (Vec<String>, Vec<String>) {
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_text(output)
    );
    let mut receipt = stdout_lines(output);
    let receipt_length = receipt
        .iter()
        .rposition(|line| !is_render_line(line))
        .map_or(0, |last| last + 1);
    let report = receipt.split_off(receipt_length);
    (receipt, report)
}

/// Asserts that `output` is a success and returns the receipt lines of the
/// command, without the report of the render that follows them.
fn succeeded(output: &Output) -> Vec<String> {
    succeeded_with_render_report(output).0
}

/// Asserts that `output` is a refusal: exit 2, nothing on stdout, and
/// exactly `line` on stderr.
fn refused_with(output: &Output, line: &str) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        stderr_text(output)
    );
    assert_eq!(stderr_text(output), format!("{line}\n"));
    assert_eq!(stdout_lines(output), Vec::<String>::new());
}

/// Asserts a refusal that changes no file of `project`.
fn refused_changing_nothing(project: &Project, args: &[&str], line: &str) {
    let before = project.snapshot();
    refused_with(&project.run(args), line);
    tree_snapshot::assert_same_tree(&before, &project.snapshot(), "a refusal");
}

/// A local bare repository with tags, served by a `file://` URL. The bare
/// repository lives under a directory named `git@host`, so the URL holds
/// an `@` that the source's tag split must leave alone.
struct Host {
    dir: TempDir,
}

impl Host {
    fn new() -> Self {
        let host = Self {
            dir: tempfile::tempdir().expect("tempdir"),
        };
        fs::create_dir_all(host.bare()).expect("create the bare repository directory");
        fs::create_dir_all(host.work()).expect("create the work directory");
        git(&host.bare(), &["init", "--bare", "-q"]);
        git(&host.work(), &["init", "-q"]);
        host
    }

    fn bare(&self) -> PathBuf {
        self.dir.path().join("git@host").join("repo.git")
    }

    fn work(&self) -> PathBuf {
        self.dir.path().join("work")
    }

    /// The `file://` URL of the bare repository.
    fn url(&self) -> String {
        let path = self.bare().to_string_lossy().replace('\\', "/");
        if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        }
    }

    /// Commits `files` (a `/`-separated path and its JSON), moves `tag` to
    /// that commit, and pushes both. Returns the commit.
    fn publish(&self, files: &[(&str, &Value)], tag: &str, annotated: bool) -> String {
        for (relative, value) in files {
            let path = relative
                .split('/')
                .fold(self.work(), |path, part| path.join(part));
            fs::create_dir_all(path.parent().expect("a parent")).expect("create the parent");
            fs::write(path, emit_form(value)).expect("write a file");
        }
        git(&self.work(), &["add", "-A"]);
        git(&self.work(), &["commit", "-q", "-m", tag]);
        if annotated {
            git(&self.work(), &["tag", "-f", "-a", tag, "-m", tag]);
        } else {
            git(&self.work(), &["tag", "-f", tag]);
        }
        let bare = self.bare().to_string_lossy().into_owned();
        git(
            &self.work(),
            &["push", "-q", "--force", &bare, "HEAD:refs/heads/main"],
        );
        git(
            &self.work(),
            &["push", "-q", "--force", &bare, &format!("refs/tags/{tag}")],
        );
        git(&self.work(), &["rev-parse", "HEAD"])
    }

    /// Pushes the current work commit as the branch `name`.
    fn push_branch(&self, name: &str) {
        let bare = self.bare().to_string_lossy().into_owned();
        git(
            &self.work(),
            &[
                "push",
                "-q",
                "--force",
                &bare,
                &format!("HEAD:refs/heads/{name}"),
            ],
        );
    }

    /// The source argument of `pack add`: `git+<url>@<tag>#<path>`.
    fn source(&self, tag: &str, path: &str) -> String {
        format!("git+{}@{tag}#{path}", self.url())
    }
}

/// The pins of `.houserules.json`.
fn pins(project: &Project) -> Value {
    project.read_json(".houserules.json")["packs"].clone()
}

// ---- pack list ----

/// The version of each pack the binary carries, from the checkout's
/// `template/packs/`, by name.
fn kit_versions() -> BTreeMap<String, String> {
    let dir = repo_root().join("template").join("packs");
    let mut versions = BTreeMap::new();
    for item in fs::read_dir(dir).expect("read template/packs") {
        let path = item.expect("read an entry").path();
        let pack: Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read a pack")).expect("parse");
        versions.insert(
            pack["name"].as_str().expect("a name").to_string(),
            pack["version"].as_str().expect("a version").to_string(),
        );
    }
    versions
}

/// `list` prints each installed pack with its name, version, source,
/// entry count, and override and exclusion counts, then each pack the
/// binary carries that the install lacks. A copy with no pin shows the
/// designed source `unpinned`.
#[test]
fn list_prints_installed_packs_with_counts_and_the_packs_the_install_lacks() {
    let project = Project::new();
    project.install_pack(
        &simple_pack(
            "team",
            "1.2.0",
            &[
                entry("team.a", "A"),
                entry("team.b", "B"),
                entry("team.c", "C"),
            ],
        ),
        "file:vendor/team.json",
        &[],
    );
    project.write_json(
        "knowledge/packs/loose.json",
        &simple_pack("loose", "0.3.0", &[entry("loose.a", "A")]),
    );
    project.write_json(
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [
                {"id": "team.a", "fields": {"summary": "Changed"}},
                {"id": "loose.a", "fields": {"summary": "Changed"}},
            ],
            "exclusions": [{"id": "team.b"}, {"id": "team.c", "reason": "No."}],
        }),
    );

    let output = project.run(&["pack", "list"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_text(&output)
    );
    let printed: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    let available: Vec<Value> = kit_versions()
        .into_iter()
        .map(|(name, version)| json!({"name": name, "version": version}))
        .collect();
    assert_eq!(
        printed,
        json!({
            "installed": [
                {"name": "loose", "version": "0.3.0", "source": "unpinned",
                 "entries": 1, "overrides": 1, "exclusions": 0},
                {"name": "team", "version": "1.2.0", "source": "file:vendor/team.json",
                 "entries": 3, "overrides": 1, "exclusions": 2},
            ],
            "available": available,
        })
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), emit_form(&printed));
}

/// A pin with no copy counts as installed for `add` and `remove`, so `list`
/// shows it: its version and source come from the pin, and the designed
/// token `no copy` stands where the copy's counts would. The rows keep name
/// order across copies and pins, and the pack leaves the `available` list.
#[test]
fn list_shows_a_pin_that_has_no_copy_with_a_designed_token_for_its_counts() {
    let project = Project::new();
    project.install_pack(
        &simple_pack("team", "1.2.0", &[entry("team.a", "A")]),
        "file:vendor/team.json",
        &[],
    );
    let mut marker = project.read_json(".houserules.json");
    marker["packs"]["quality"] = json!({
        "version": "1.0.0", "source": "kit", "sha256": "0", "addedAreas": [],
    });
    project.write_json(".houserules.json", &marker);

    let output = project.run(&["pack", "list"]);

    let printed: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    let available: Vec<Value> = kit_versions()
        .into_iter()
        .filter(|(name, _)| name != "quality")
        .map(|(name, version)| json!({"name": name, "version": version}))
        .collect();
    assert_eq!(
        printed,
        json!({
            "installed": [
                {"name": "quality", "version": "1.0.0", "source": "kit",
                 "entries": "no copy", "overrides": "no copy", "exclusions": "no copy"},
                {"name": "team", "version": "1.2.0", "source": "file:vendor/team.json",
                 "entries": 1, "overrides": 0, "exclusions": 0},
            ],
            "available": available,
        })
    );
}

/// An installed kit pack leaves the `available` list.
#[test]
fn list_leaves_an_installed_kit_pack_out_of_the_available_packs() {
    let project = Project::new();
    succeeded(&project.run(&["pack", "add", "security-hygiene"]));

    let output = project.run(&["pack", "list"]);

    let printed: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    let installed: Vec<&str> = printed["installed"]
        .as_array()
        .expect("installed")
        .iter()
        .map(|row| row["name"].as_str().expect("a name"))
        .collect();
    let available: Vec<&str> = printed["available"]
        .as_array()
        .expect("available")
        .iter()
        .map(|row| row["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(installed, ["security-hygiene"]);
    assert_eq!(
        available,
        ["knowledge-base", "process", "quality", "writing-style"]
    );
}

// ---- pack add: a kit pack ----

/// `add <name>` copies the kit pack in emit form, pins it, adds its missing
/// areas, and keeps every other key of the marker in its place.
#[test]
fn add_copies_a_kit_pack_pins_it_and_keeps_the_other_marker_keys_in_order() {
    let project = Project::new();
    let kit_bytes = fs::read(
        repo_root()
            .join("template")
            .join("packs")
            .join("quality.json"),
    )
    .expect("read the kit pack");
    let kit: Value = serde_json::from_slice(&kit_bytes).expect("parse the kit pack");
    let version = kit["version"].as_str().expect("a version");

    let lines = succeeded(&project.run(&["pack", "add", "quality"]));

    assert_eq!(
        fs::read(project.path("knowledge/packs/quality.json")).expect("read the copy"),
        kit_bytes
    );
    assert_eq!(
        project.read(".houserules.json"),
        emit_form(&json!({
            "schema": "houserules/install@2.0.0",
            "version": "1.4.0",
            "idPrefix": "WI",
            "baselines": {},
            "packs": {
                "quality": {
                    "version": version,
                    "source": "kit",
                    "sha256": pack_pin(&kit),
                    "addedAreas": ["process", "tools"],
                },
            },
        }))
    );
    assert_eq!(
        project.read("knowledge/areas.json"),
        emit_form(&json!({
            "global": {"paths": []},
            "docs": {"paths": ["docs/**"]},
            "process": {"paths": []},
            "tools": {"paths": ["tools/**"]},
        }))
    );
    assert_eq!(
        lines,
        [
            format!("added pack quality {version} from kit"),
            "added area \"process\" to knowledge/areas.json with no globs".to_string(),
            "added area \"tools\" to knowledge/areas.json with the globs tools/**".to_string(),
        ]
    );
}

/// `add` of an installed name refuses, and so does an unknown name.
#[test]
fn add_refuses_an_installed_name_and_an_unknown_name_and_changes_nothing() {
    let project = Project::new();
    succeeded(&project.run(&["pack", "add", "security-hygiene"]));

    refused_changing_nothing(
        &project,
        &["pack", "add", "security-hygiene"],
        "pack security-hygiene is already installed; pack upgrade moves an upstream pack and update moves a kit pack",
    );
    refused_changing_nothing(
        &project,
        &["pack", "add", "procezz"],
        "procezz: not a pack that houserules carries (knowledge-base, process, quality, security-hygiene, writing-style) and not a file",
    );
}

/// An orphan pin or an orphan copy also counts as installed.
#[test]
fn add_refuses_a_name_that_has_only_a_copy_or_only_a_pin() {
    let copy_only = Project::new();
    copy_only.write_json(
        "knowledge/packs/quality.json",
        &simple_pack("quality", "1.0.0", &[entry("quality.x", "X")]),
    );
    refused_changing_nothing(
        &copy_only,
        &["pack", "add", "quality"],
        "pack quality is already installed; pack upgrade moves an upstream pack and update moves a kit pack",
    );

    let pin_only = Project::new();
    let mut marker = pin_only.read_json(".houserules.json");
    marker["packs"] = json!({"quality": {
        "version": "1.0.0", "source": "kit", "sha256": "0", "addedAreas": [],
    }});
    pin_only.write_json(".houserules.json", &marker);
    refused_changing_nothing(
        &pin_only,
        &["pack", "add", "quality"],
        "pack quality is already installed; pack upgrade moves an upstream pack and update moves a kit pack",
    );
}

/// Spec 3.10: `pack add process` reads the `tracker` key of the marker. A
/// `backlog/` directory is no tracker.
#[test]
fn add_process_refuses_without_a_tracker_and_adds_it_with_one() {
    let project = Project::new();
    let refusal = "pack process needs a tracker; run houserules init --tracker json|github|beads";
    refused_changing_nothing(&project, &["pack", "add", "process"], refusal);

    project.write("backlog/schema.json", "{}\n");
    refused_changing_nothing(&project, &["pack", "add", "process"], refusal);

    let mut marker = project.read_json(".houserules.json");
    marker["tracker"] = json!({"adapter": "json"});
    project.write_json(".houserules.json", &marker);
    let lines = succeeded(&project.run(&["pack", "add", "process"]));

    assert!(
        lines[0].starts_with("added pack process "),
        "got: {lines:?}"
    );
    assert!(project.exists("knowledge/packs/process.json"));
}

/// 5.110: another installed pack that ships one of the ids refuses the
/// add, naming each id and that pack.
#[test]
fn add_refuses_a_pack_whose_ids_another_installed_pack_ships() {
    let project = Project::new();
    project.install_pack(
        &simple_pack(
            "team",
            "1.0.0",
            &[entry("team.alpha", "A"), entry("team.beta", "B")],
        ),
        "file:vendor/team.json",
        &[],
    );
    project.write_json(
        "vendor/extra.json",
        &pack_value(
            "extra",
            "1.0.0",
            &["team", "extra"],
            &json!({"global": {"paths": []}}),
            &[
                entry("extra.own", "Own"),
                entry("team.alpha", "A"),
                entry("team.beta", "B"),
            ],
        ),
    );
    let file = project.path("vendor/extra.json");

    refused_changing_nothing(
        &project,
        &["pack", "add", file.to_str().expect("utf-8 path")],
        "cannot add pack extra: team.alpha is also in pack team, team.beta is also in pack team",
    );
}

// ---- pack add: a file source ----

/// `add <file>` copies a vendored file in emit form and records the path
/// relative to the install root. A CRLF file gives the same checksum as
/// the canonical form.
#[test]
fn add_copies_a_vendored_file_in_emit_form_and_records_a_root_relative_source() {
    let project = Project::new();
    let pack = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    let crlf = serde_json::to_string_pretty(&pack)
        .expect("serialize")
        .replace('\n', "\r\n");
    project.write("vendor/team.json", &crlf);
    let file = project.path("vendor/team.json");

    let lines = succeeded(&project.run(&["pack", "add", file.to_str().expect("utf-8 path")]));

    assert_eq!(project.read("knowledge/packs/team.json"), emit_form(&pack));
    assert_eq!(
        pins(&project),
        json!({"team": {
            "version": "1.0.0",
            "source": "file:vendor/team.json",
            "sha256": pack_pin(&pack),
            "addedAreas": [],
        }})
    );
    assert_eq!(lines, ["added pack team 1.0.0 from file:vendor/team.json"]);
}

/// A relative file argument resolves against the current directory, and the
/// recorded path is relative to the install root.
#[test]
fn add_resolves_a_relative_file_against_the_current_directory() {
    let project = Project::new();
    project.write_json(
        "vendor/team.json",
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]),
    );

    let output = houserules()
        .args(["pack", "add", "team.json", "--dir"])
        .arg(project.root())
        .current_dir(project.path("vendor"))
        .output()
        .expect("run houserules");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_text(&output)
    );
    assert_eq!(
        pins(&project)["team"]["source"],
        json!("file:vendor/team.json")
    );
}

/// A file outside the install root refuses: its recorded path could not
/// be relative to the root.
#[test]
fn add_refuses_a_file_outside_the_install_root() {
    let project = Project::new();
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let file = elsewhere.path().join("team.json");
    fs::write(
        &file,
        emit_form(&simple_pack("team", "1.0.0", &[entry("team.alpha", "A")])),
    )
    .expect("write the pack");
    let argument = file.to_str().expect("utf-8 path");

    refused_changing_nothing(
        &project,
        &["pack", "add", argument],
        &format!(
            "{argument}: the file is outside the install root {}; a file source must lie inside it",
            project.root().display()
        ),
    );
}

/// L1: a file whose pack name is `../schema` or `local` refuses before any
/// write, and so does a file that is no JSON.
#[test]
fn add_refuses_an_illegal_pack_name_and_an_unreadable_pack_before_any_write() {
    let project = Project::new();
    for name in ["../schema", "local"] {
        let mut pack = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
        pack["name"] = json!(name);
        project.write_json("vendor/bad.json", &pack);
        let file = project.path("vendor/bad.json");
        let argument = file.to_str().expect("utf-8 path");

        refused_changing_nothing(
            &project,
            &["pack", "add", argument],
            &format!(
                "{argument}: pack name \"{name}\" is not allowed; it must match ^[a-z0-9-]+$ and must not be \"local\""
            ),
        );
    }

    project.write("vendor/bad.json", "{ not json\n");
    let file = project.path("vendor/bad.json");
    let argument = file.to_str().expect("utf-8 path");
    let output = project.run(&["pack", "add", argument]);
    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with(&format!("{argument}: invalid JSON (")),
        "got: {message}"
    );
    assert_eq!(message.lines().count(), 1, "one line only: {message}");
    assert!(!project.exists("knowledge/packs/team.json"));
}

// ---- pack add: a git source ----

/// A git source copies the file at the tag and pins the source, the commit
/// the tag points to, and the checksum. The URL holds an `@`, which the
/// split of the tag leaves alone.
#[test]
fn add_fetches_a_pack_from_a_tag_and_pins_its_commit() {
    let host = Host::new();
    let pack = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    let commit = host.publish(&[("packs/team.json", &pack)], "v1.0.0", false);
    let project = Project::new();
    let source = host.source("v1.0.0", "packs/team.json");

    let lines = succeeded(&project.run(&["pack", "add", &source]));

    assert_eq!(project.read("knowledge/packs/team.json"), emit_form(&pack));
    assert_eq!(
        pins(&project),
        json!({"team": {
            "version": "1.0.0",
            "source": source,
            "sha256": pack_pin(&pack),
            "addedAreas": [],
            "commit": commit,
        }})
    );
    assert_eq!(lines, [format!("added pack team 1.0.0 from {source}")]);
}

/// An annotated tag pins the commit it points to, never the tag object.
#[test]
fn add_pins_the_peeled_commit_of_an_annotated_tag() {
    let host = Host::new();
    let pack = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    let commit = host.publish(&[("packs/team.json", &pack)], "v1.0.0", true);
    let tag_object = git(&host.work(), &["rev-parse", "refs/tags/v1.0.0"]);
    assert_ne!(tag_object, commit, "the tag must be annotated");
    let project = Project::new();

    succeeded(&project.run(&["pack", "add", &host.source("v1.0.0", "packs/team.json")]));

    assert_eq!(pins(&project)["team"]["commit"], json!(commit));
}

/// A ref that names a branch, a commit, or nothing is no tag: the add
/// refuses with one line and writes nothing.
#[test]
fn add_refuses_a_ref_that_is_no_tag() {
    let host = Host::new();
    let pack = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    let commit = host.publish(&[("packs/team.json", &pack)], "v1.0.0", false);
    host.push_branch("release");
    let project = Project::new();
    for reference in ["release", commit.as_str(), "v9"] {
        let source = host.source(reference, "packs/team.json");

        refused_changing_nothing(
            &project,
            &["pack", "add", &source],
            &format!(
                "{source}: {} has no tag \"{reference}\"; a pin names a tag, never a branch or a commit",
                host.url()
            ),
        );
    }
}

/// A git failure is one line that names the source and the git command.
#[test]
fn add_names_a_git_failure_in_one_line() {
    let host = Host::new();
    let pack = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    host.publish(&[("packs/team.json", &pack)], "v1.0.0", false);
    let project = Project::new();

    let missing_repository = format!("git+{}-missing@v1.0.0#packs/team.json", host.url());
    let output = project.run(&["pack", "add", &missing_repository]);
    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with(&format!("{missing_repository}: git ls-remote failed: ")),
        "got: {message}"
    );
    assert_eq!(message.lines().count(), 1, "one line only: {message}");

    let missing_file = host.source("v1.0.0", "packs/nothing.json");
    let output = project.run(&["pack", "add", &missing_file]);
    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with(&format!("{missing_file}: git show failed: ")),
        "got: {message}"
    );
    assert_eq!(message.lines().count(), 1, "one line only: {message}");
    assert!(!project.exists("knowledge/packs/team.json"));
}

/// A file at the tag that is no JSON is one named line.
#[test]
fn add_names_a_fetched_file_that_is_no_json() {
    let host = Host::new();
    fs::write(host.work().join("bad.json"), "{ not json\n").expect("write the file");
    git(&host.work(), &["add", "-A"]);
    git(&host.work(), &["commit", "-q", "-m", "bad"]);
    git(&host.work(), &["tag", "v1"]);
    let bare = host.bare().to_string_lossy().into_owned();
    git(&host.work(), &["push", "-q", &bare, "refs/tags/v1"]);
    let project = Project::new();
    let source = host.source("v1", "bad.json");

    let output = project.run(&["pack", "add", &source]);

    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with(&format!("{source}: invalid JSON (")),
        "got: {message}"
    );
}

/// A git source without a `#` or an `@`, with an empty part, or with a URL
/// that starts with `-` refuses with the form it expects.
#[test]
fn add_refuses_a_malformed_git_source() {
    let project = Project::new();
    for source in [
        "git+file:///x/y.git@v1",
        "git+file:///x/y.git#p.json",
        "git+@v1#p.json",
        "git+file:///x/y.git@#p.json",
        "git+file:///x/y.git@v1#",
        "git+-oProxyCommand=x@v1#p.json",
    ] {
        refused_changing_nothing(
            &project,
            &["pack", "add", source],
            &format!(
                "{source}: a git source reads git+<url>@<tag>#<path>, with every part set and a url that does not start with -"
            ),
        );
    }
}

/// The line that a relative local path as a git URL is refused with, after
/// the source.
const RELATIVE_URL_LINE: &str = "a relative path as the url names another repository from each directory; use a file:// URL or a remote URL";

/// A git URL that is a relative local path refuses before any git call: git
/// would resolve it against the working directory of each call (the tag
/// listing runs in the install root, the fetch in a temporary directory),
/// so the tag would resolve and the fetch would fail. A directory with no
/// `git` command proves that no call runs. An `upgrade` of a pin that
/// records such a URL refuses the same way.
#[test]
fn a_git_url_that_is_a_relative_path_refuses_before_any_git_call() {
    let project = Project::new();
    let no_commands = tempfile::tempdir().expect("tempdir");
    for url in ["../host.git", "./host.git", "host.git"] {
        let source = format!("git+{url}@v1#team.json");
        let before = project.snapshot();

        let output = houserules()
            .args(["pack", "add", &source, "--dir"])
            .arg(project.root())
            .env("PATH", no_commands.path())
            .output()
            .expect("run houserules");

        refused_with(&output, &format!("{source}: {RELATIVE_URL_LINE}"));
        tree_snapshot::assert_same_tree(&before, &project.snapshot(), "a refusal");
    }

    let recorded = "git+../host.git@v1#team.json";
    project.install_pack(
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]),
        recorded,
        &[],
    );
    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "team", "v2"],
        &format!("{recorded}: {RELATIVE_URL_LINE}"),
    );
}

/// Without a `git` command the add names the operating system's own error.
#[test]
fn add_names_a_missing_git_command() {
    let host = Host::new();
    let pack = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    host.publish(&[("packs/team.json", &pack)], "v1.0.0", false);
    let project = Project::new();
    let no_commands = tempfile::tempdir().expect("tempdir");
    let source = host.source("v1.0.0", "packs/team.json");

    let output = houserules()
        .args(["pack", "add", &source, "--dir"])
        .arg(project.root())
        .env("PATH", no_commands.path())
        .output()
        .expect("run houserules");

    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with(&format!("{source}: cannot run git: ")),
        "got: {message}"
    );
    assert_eq!(message.lines().count(), 1, "one line only: {message}");
}

/// The pin keeps its commit and checksum when the tag moves: every
/// command reads the copy, so no network call and no change follows.
#[test]
fn a_moved_tag_changes_neither_the_copy_nor_the_pin() {
    let host = Host::new();
    let first = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    let commit = host.publish(&[("packs/team.json", &first)], "v1.0.0", false);
    let project = Project::new();
    succeeded(&project.run(&["pack", "add", &host.source("v1.0.0", "packs/team.json")]));
    let before = project.snapshot();

    let second = simple_pack("team", "1.0.1", &[entry("team.alpha", "Moved")]);
    let moved_commit = host.publish(&[("packs/team.json", &second)], "v1.0.0", false);
    assert_ne!(moved_commit, commit);

    let listed = project.run(&["pack", "list"]);
    let printed: Value = serde_json::from_slice(&listed.stdout).expect("stdout is JSON");
    assert_eq!(printed["installed"][0]["version"], json!("1.0.0"));
    tree_snapshot::assert_same_tree(&before, &project.snapshot(), "the command");
    assert_eq!(pins(&project)["team"]["commit"], json!(commit));
}

// ---- pack remove ----

/// A vendored pack of the given name and entries, written under `vendor/`.
/// Returns the file's path as `add` takes it.
fn vendor_pack(project: &Project, pack: &Value) -> String {
    let name = pack["name"].as_str().expect("a name");
    let relative = format!("vendor/{name}.json");
    project.write_json(&relative, pack);
    project
        .path(&relative)
        .to_str()
        .expect("utf-8 path")
        .to_string()
}

/// `remove` deletes the copy and the pin and prints one receipt line.
#[test]
fn remove_deletes_the_copy_and_the_pin() {
    let project = Project::new();
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]),
    );
    succeeded(&project.run(&["pack", "add", &file]));

    let lines = succeeded(&project.run(&["pack", "remove", "team"]));

    assert_eq!(lines, ["removed pack team 1.0.0"]);
    assert!(!project.exists("knowledge/packs/team.json"));
    assert_eq!(
        project.read(".houserules.json"),
        emit_form(&json!({
            "schema": "houserules/install@2.0.0",
            "version": "1.4.0",
            "idPrefix": "WI",
            "baselines": {},
            "packs": {},
        }))
    );
}

/// `remove` refuses a name that is not installed.
#[test]
fn remove_refuses_a_name_that_is_not_installed() {
    let project = Project::new();

    refused_changing_nothing(
        &project,
        &["pack", "remove", "team"],
        "pack team is not installed",
    );
}

/// `remove` fails, changing nothing, while `overrides.json` names one of
/// the pack's ids; the line names each id.
#[test]
fn remove_refuses_while_overrides_json_names_an_id_of_the_pack() {
    let project = Project::new();
    let file = vendor_pack(
        &project,
        &simple_pack(
            "team",
            "1.0.0",
            &[entry("team.alpha", "A"), entry("team.beta", "B")],
        ),
    );
    succeeded(&project.run(&["pack", "add", &file]));
    project.write_json(
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": "team.alpha", "fields": {"summary": "Changed"}}],
            "exclusions": [{"id": "team.beta"}],
        }),
    );

    refused_changing_nothing(
        &project,
        &["pack", "remove", "team"],
        "cannot remove pack team: knowledge/overrides.json names team.alpha, team.beta; remove those overrides and exclusions first",
    );
}

/// Spec 6, bullet 3: `remove` takes out an area only when a pack added it,
/// no remaining pack declares it, and no active entry of any source uses
/// it. It prints the removed area with its globs. A kept area that another
/// pack still declares moves its record to that pack. A project's own area
/// stays.
#[test]
fn remove_takes_out_only_the_areas_that_spec_six_allows() {
    let project = Project::new();
    project.write_json(
        "knowledge/areas.json",
        &json!({
            "global": {"paths": []},
            "docs": {"paths": ["docs/**"]},
            "d": {"paths": ["d/**"]},
        }),
    );
    project.write_json(
        "knowledge/mine.json",
        &json!({
            "topic": "mine",
            "title": "Mine",
            "entries": [{
                "id": "mine.thing", "kind": "rule", "area": "b", "summary": "Uses b",
                "body": ["Body."], "tags": [],
                "source": {"date": "2026-10-19", "by": "controller"},
            }],
        }),
    );
    let alpha = pack_value(
        "alpha",
        "1.0.0",
        &["alpha"],
        &json!({
            "global": {"paths": []},
            "a": {"paths": ["a/**"]},
            "b": {"paths": ["b/**"]},
            "c": {"paths": ["c/**"]},
            "d": {"paths": ["alpha-d/**"]},
        }),
        &[entry("alpha.one", "One")],
    );
    let beta = pack_value(
        "beta",
        "1.0.0",
        &["beta"],
        &json!({"global": {"paths": []}, "c": {"paths": ["c/**"]}}),
        &[entry("beta.one", "One")],
    );
    let alpha_file = vendor_pack(&project, &alpha);
    let beta_file = vendor_pack(&project, &beta);
    succeeded(&project.run(&["pack", "add", &alpha_file]));
    succeeded(&project.run(&["pack", "add", &beta_file]));
    assert_eq!(
        pins(&project)["alpha"]["addedAreas"],
        json!(["a", "b", "c"])
    );
    assert_eq!(pins(&project)["beta"]["addedAreas"], json!([]));

    let lines = succeeded(&project.run(&["pack", "remove", "alpha"]));

    assert_eq!(
        lines,
        [
            "removed pack alpha 1.0.0",
            "removed area \"a\" from knowledge/areas.json; its globs were a/**",
        ]
    );
    assert_eq!(
        project.read("knowledge/areas.json"),
        emit_form(&json!({
            "global": {"paths": []},
            "docs": {"paths": ["docs/**"]},
            "d": {"paths": ["d/**"]},
            "b": {"paths": ["b/**"]},
            "c": {"paths": ["c/**"]},
        }))
    );
    assert_eq!(
        pins(&project),
        json!({"beta": {
            "version": "1.0.0",
            "source": "file:vendor/beta.json",
            "sha256": pack_pin(&beta),
            "addedAreas": ["c"],
        }})
    );
}

// ---- pack upgrade ----

/// Spec 16: an upgrade to a second tag that adds one id and drops one
/// refuses, changing nothing, while an override names the dropped id. After
/// that override goes, it moves the pin, keeps the other override, and
/// prints the added and dropped ids.
#[test]
fn upgrade_moves_a_git_pack_to_a_new_tag_and_refuses_while_an_override_names_a_dropped_id() {
    let host = Host::new();
    let first = pack_value(
        "team",
        "1.0.0",
        &["team"],
        &json!({"global": {"paths": []}, "extra1": {"paths": ["extra1/**"]}}),
        &[
            entry("team.keep", "Keep v1"),
            entry("team.drop", "Drop"),
            entry("team.stay", "Stay"),
        ],
    );
    let second = pack_value(
        "team",
        "2.0.0",
        &["team"],
        &json!({"global": {"paths": []}, "extra2": {"paths": ["tools/**"]}}),
        &[
            entry("team.keep", "Keep v2"),
            entry("team.new", "New"),
            entry("team.stay", "Stay"),
        ],
    );
    host.publish(&[("packs/team.json", &first)], "v1", false);
    let second_commit = host.publish(&[("packs/team.json", &second)], "v2", true);
    let project = Project::new();
    succeeded(&project.run(&["pack", "add", &host.source("v1", "packs/team.json")]));
    assert_eq!(pins(&project)["team"]["addedAreas"], json!(["extra1"]));
    let kept_override = json!({"id": "team.stay", "fields": {"summary": "Mine"}});
    project.write_json(
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [
                {"id": "team.drop", "fields": {"summary": "Mine too"}},
                kept_override,
            ],
            "exclusions": [],
        }),
    );

    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "team", "v2"],
        "cannot upgrade pack team: knowledge/overrides.json names team.drop, which version 2.0.0 drops; remove those overrides and exclusions first",
    );

    project.write_json(
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [kept_override],
            "exclusions": [],
        }),
    );
    let overrides_before = project.read("knowledge/overrides.json");

    let lines = succeeded(&project.run(&["pack", "upgrade", "team", "v2"]));

    assert_eq!(
        project.read("knowledge/packs/team.json"),
        emit_form(&second)
    );
    assert_eq!(project.read("knowledge/overrides.json"), overrides_before);
    assert_eq!(
        pins(&project),
        json!({"team": {
            "version": "2.0.0",
            "source": host.source("v2", "packs/team.json"),
            "sha256": pack_pin(&second),
            "addedAreas": ["extra2"],
            "commit": second_commit,
        }})
    );
    assert_eq!(
        project.read("knowledge/areas.json"),
        emit_form(&json!({
            "global": {"paths": []},
            "docs": {"paths": ["docs/**"]},
            "extra2": {"paths": ["tools/**"]},
        }))
    );
    assert_eq!(
        lines,
        [
            "upgraded pack team from 1.0.0 to 2.0.0",
            "added id team.new",
            "dropped id team.drop",
            "removed area \"extra1\" from knowledge/areas.json; its globs were extra1/**",
            "added area \"extra2\" to knowledge/areas.json with the globs tools/**",
        ]
    );
}

/// A file source re-copies from its recorded path.
#[test]
fn upgrade_recopies_a_file_pack_from_its_recorded_path() {
    let project = Project::new();
    let first = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    let file = vendor_pack(&project, &first);
    succeeded(&project.run(&["pack", "add", &file]));
    let second = simple_pack(
        "team",
        "1.1.0",
        &[entry("team.alpha", "A"), entry("team.gamma", "G")],
    );
    project.write_json("vendor/team.json", &second);

    let lines = succeeded(&project.run(&["pack", "upgrade", "team"]));

    assert_eq!(
        project.read("knowledge/packs/team.json"),
        emit_form(&second)
    );
    assert_eq!(
        pins(&project),
        json!({"team": {
            "version": "1.1.0",
            "source": "file:vendor/team.json",
            "sha256": pack_pin(&second),
            "addedAreas": [],
        }})
    );
    assert_eq!(
        lines,
        [
            "upgraded pack team from 1.0.0 to 1.1.0",
            "added id team.gamma"
        ]
    );
}

/// Every refusal of `upgrade` is one line and changes nothing.
#[test]
fn upgrade_refuses_each_case_in_one_line_and_changes_nothing() {
    let host = Host::new();
    let published = simple_pack("remote", "1.0.0", &[entry("remote.alpha", "A")]);
    host.publish(&[("packs/remote.json", &published)], "v1", false);
    let project = Project::new();
    succeeded(&project.run(&["pack", "add", "security-hygiene"]));
    let team_file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]),
    );
    succeeded(&project.run(&["pack", "add", &team_file]));
    let remote_source = host.source("v1", "packs/remote.json");
    succeeded(&project.run(&["pack", "add", &remote_source]));
    project.write_json(
        "knowledge/packs/loose.json",
        &simple_pack("loose", "1.0.0", &[entry("loose.a", "A")]),
    );

    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "nothing"],
        "pack nothing is not installed",
    );
    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "security-hygiene"],
        "pack security-hygiene is a kit pack; houserules update moves it",
    );
    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "loose"],
        "pack loose has no pin in .houserules.json; remove it and add it again",
    );
    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "team", "v2"],
        "pack team comes from file:vendor/team.json, which takes no tag; run houserules pack upgrade team",
    );
    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "remote"],
        &format!(
            "pack remote comes from {remote_source}; run houserules pack upgrade remote <tag>"
        ),
    );
}

/// The file at the new source must name the installed pack; another pack
/// that ships one of the new ids refuses the upgrade (5.110).
#[test]
fn upgrade_refuses_a_renamed_pack_and_an_id_that_another_pack_ships() {
    let project = Project::new();
    let team_file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]),
    );
    succeeded(&project.run(&["pack", "add", &team_file]));
    let other_file = vendor_pack(
        &project,
        &pack_value(
            "other",
            "1.0.0",
            &["other", "team"],
            &json!({"global": {"paths": []}}),
            &[entry("team.newid", "N")],
        ),
    );
    succeeded(&project.run(&["pack", "add", &other_file]));

    project.write_json(
        "vendor/team.json",
        &simple_pack("renamed", "1.1.0", &[entry("renamed.alpha", "A")]),
    );
    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "team"],
        "pack team: the file at file:vendor/team.json names the pack \"renamed\"; to change the source, run pack remove, then pack add",
    );

    project.write_json(
        "vendor/team.json",
        &pack_value(
            "team",
            "1.1.0",
            &["team"],
            &json!({"global": {"paths": []}}),
            &[entry("team.alpha", "A"), entry("team.newid", "N")],
        ),
    );
    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "team"],
        "cannot upgrade pack team: team.newid is also in pack other",
    );
}

// ---- one id, one source (L8, L9) ----

/// A local topic file of the given topic.
fn local_topic(project: &Project, topic: &str, entries: &[Value]) {
    project.write_json(
        &format!("knowledge/{topic}.json"),
        &json!({"topic": topic, "title": format!("{topic} title"), "entries": entries}),
    );
}

/// An entry of `base` with `summary` replaced, and `standing` removed.
fn changed(base: &Value, summary: &str) -> Value {
    let mut copy = base.clone();
    copy["summary"] = json!(summary);
    copy.as_object_mut()
        .expect("an object")
        .shift_remove("standing");
    copy
}

/// L8 and L9 through `pack add`: a local entry that equals the pack entry
/// goes; one that differs becomes an override that carries each differing
/// field (a field the local copy lacks is set to `null`); an exclusion or
/// override of the id gives way, one printed line for each; a file that
/// holds only shipped entries goes; a file with an unshipped entry or with
/// `"entries": []` stays; an archived entry becomes a retired override and
/// leaves the archive.
#[test]
fn add_applies_one_id_one_source_to_local_and_archived_entries() {
    let project = Project::new();
    let same = entry("team.same", "Same");
    let base_changed = entry("team.changed", "Shipped");
    let one = entry("solo.one", "One");
    let old = entry("team.old", "Shipped old");
    let pack = pack_value(
        "team",
        "1.0.0",
        &["team", "solo"],
        &json!({"global": {"paths": []}}),
        &[same.clone(), base_changed.clone(), one.clone(), old.clone()],
    );
    let file = vendor_pack(&project, &pack);
    let mine = entry("team.mine", "Mine");
    local_topic(
        &project,
        "team",
        &[same, changed(&base_changed, "Local"), mine.clone()],
    );
    local_topic(&project, "solo", &[one]);
    local_topic(&project, "empty", &[]);
    let mut archived_old = changed(&old, "Local old");
    archived_old["status"] = json!("retired");
    project.write_json(
        "knowledge/archive/team.json",
        &json!({"topic": "team", "title": "team title", "entries": [archived_old]}),
    );
    project.write_json(
        "knowledge/overrides.json",
        &json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [{"id": "team.changed", "fields": {"summary": "Old override"}}],
            "exclusions": [
                {"id": "team.same", "reason": "No."},
                {"id": "team.old"},
            ],
        }),
    );
    let empty_before = project.read("knowledge/empty.json");

    let lines = succeeded(&project.run(&["pack", "add", &file]));

    assert_eq!(
        lines,
        [
            "added pack team 1.0.0 from file:vendor/team.json",
            "dropped solo.one from knowledge/solo.json: pack team ships the same entry",
            "removed knowledge/solo.json: installed packs ship every entry in it",
            "dropped team.same from knowledge/team.json: pack team ships the same entry",
            "replaced the exclusion of team.same in knowledge/overrides.json with the local entry",
            "moved team.changed from knowledge/team.json to knowledge/overrides.json: pack team ships it, and the override sets summary, standing",
            "replaced the override of team.changed in knowledge/overrides.json with the local entry",
            "retired team.old from knowledge/archive/team.json into knowledge/overrides.json: pack team ships it",
            "replaced the exclusion of team.old in knowledge/overrides.json with the local entry",
            "removed knowledge/archive/team.json: it holds no entry now",
        ]
    );
    assert!(!project.exists("knowledge/solo.json"));
    assert!(!project.exists("knowledge/archive/team.json"));
    assert_eq!(project.read("knowledge/empty.json"), empty_before);
    assert_eq!(
        project.read("knowledge/team.json"),
        emit_form(&json!({"topic": "team", "title": "team title", "entries": [mine]}))
    );
    assert_eq!(
        project.read("knowledge/overrides.json"),
        emit_form(&json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [
                {"id": "team.changed", "fields": {"summary": "Local", "standing": null}},
                {"id": "team.old", "fields": {"status": "retired", "summary": "Local old", "standing": null}},
            ],
            "exclusions": [],
        }))
    );
}

/// When no `overrides.json` exists, a differing local entry creates it;
/// when every local entry equals its pack entry, none is created.
#[test]
fn add_creates_overrides_json_only_when_an_entry_differs() {
    let equal = Project::new();
    let alpha = entry("team.alpha", "A");
    let file = vendor_pack(
        &equal,
        &simple_pack("team", "1.0.0", std::slice::from_ref(&alpha)),
    );
    local_topic(&equal, "team", std::slice::from_ref(&alpha));
    succeeded(&equal.run(&["pack", "add", &file]));
    assert!(!equal.exists("knowledge/overrides.json"));
    assert!(!equal.exists("knowledge/team.json"));

    let differing = Project::new();
    let file = vendor_pack(
        &differing,
        &simple_pack("team", "1.0.0", std::slice::from_ref(&alpha)),
    );
    local_topic(&differing, "team", &[changed(&alpha, "Local")]);
    succeeded(&differing.run(&["pack", "add", &file]));
    assert_eq!(
        differing.read("knowledge/overrides.json"),
        emit_form(&json!({
            "schema": "houserules/overrides@1.0.0",
            "overrides": [
                {"id": "team.alpha", "fields": {"summary": "Local", "standing": null}},
            ],
            "exclusions": [],
        }))
    );
}

/// An id that is active in a topic file and archived at once refuses the
/// step, naming both files; nothing changes.
#[test]
fn add_refuses_an_id_that_is_active_and_archived_at_once() {
    let project = Project::new();
    let alpha = entry("team.alpha", "A");
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", std::slice::from_ref(&alpha)),
    );
    local_topic(&project, "team", std::slice::from_ref(&alpha));
    project.write_json(
        "knowledge/archive/team.json",
        &json!({"topic": "team", "title": "team title", "entries": [alpha]}),
    );

    refused_changing_nothing(
        &project,
        &["pack", "add", &file],
        "team.alpha is active in knowledge/team.json and archived in knowledge/archive/team.json; remove one of the two, then run the command again",
    );
}

/// An id in two local topic files refuses the step, naming both files.
#[test]
fn add_refuses_an_id_that_two_local_topic_files_hold() {
    let project = Project::new();
    let alpha = entry("team.alpha", "A");
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", std::slice::from_ref(&alpha)),
    );
    local_topic(&project, "team", std::slice::from_ref(&alpha));
    local_topic(&project, "zeta", &[alpha]);

    refused_changing_nothing(
        &project,
        &["pack", "add", &file],
        "team.alpha is in knowledge/team.json and knowledge/zeta.json; keep one entry, then run the command again",
    );
}

/// A local entry that cannot become an override (here: a key the entry
/// schema does not name) refuses the add before anything is written.
#[test]
fn add_refuses_a_local_entry_that_cannot_become_an_override() {
    let project = Project::new();
    let alpha = entry("team.alpha", "A");
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", std::slice::from_ref(&alpha)),
    );
    let mut broken = alpha;
    broken["bogus"] = json!(1);
    local_topic(&project, "team", &[broken]);
    let before = project.snapshot();

    let output = project.run(&["pack", "add", &file]);

    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with("the one-id-one-source step cannot write knowledge/overrides.json: "),
        "got: {message}"
    );
    assert!(message.contains("bogus"), "got: {message}");
    assert_eq!(message.lines().count(), 1, "one line only: {message}");
    tree_snapshot::assert_same_tree(&before, &project.snapshot(), "a refusal");
}

// ---- areas on add ----

/// The line that `add` prints for a glob of an added area that matches no
/// tracked file: information, with no remedy to apply (spec C16).
fn dead_glob_line(glob: &str, area: &str) -> String {
    format!(
        "the glob \"{glob}\" of area \"{area}\" matches no tracked file; check-knowledge accepts it while the area keeps the globs of the pack"
    )
}

/// Spec 6, bullets 2 and 5: `add` adds each area the pack declares and
/// `areas.json` lacks, with the pack's globs, and records it; an existing
/// area keeps the project's globs; a glob that matches no tracked file is
/// named.
#[test]
fn add_adds_missing_areas_keeps_existing_ones_and_names_a_dead_glob() {
    let project = Project::new();
    project.write("scripts/x.sh", "untracked\n");
    let pack = pack_value(
        "team",
        "1.0.0",
        &["team"],
        &json!({
            "global": {"paths": []},
            "docs": {"paths": ["documentation/**"]},
            "tools": {"paths": ["tools/**", "absent/**"]},
            "scripts": {"paths": ["scripts/**"]},
        }),
        &[entry("team.alpha", "A")],
    );
    let file = vendor_pack(&project, &pack);

    let lines = succeeded(&project.run(&["pack", "add", &file]));

    assert_eq!(
        project.read("knowledge/areas.json"),
        emit_form(&json!({
            "global": {"paths": []},
            "docs": {"paths": ["docs/**"]},
            "tools": {"paths": ["tools/**", "absent/**"]},
            "scripts": {"paths": ["scripts/**"]},
        }))
    );
    assert_eq!(
        pins(&project)["team"]["addedAreas"],
        json!(["tools", "scripts"])
    );
    assert_eq!(
        lines,
        [
            "added pack team 1.0.0 from file:vendor/team.json",
            "added area \"tools\" to knowledge/areas.json with the globs tools/**, absent/**",
            "added area \"scripts\" to knowledge/areas.json with the globs scripts/**",
            &dead_glob_line("absent/**", "tools"),
            &dead_glob_line("scripts/**", "scripts"),
        ]
    );
    let checked = stderr_text(&project.run(&["check-knowledge"]));
    assert!(
        !checked.contains("matches no tracked file"),
        "check-knowledge accepts the globs of the areas the pack added: {checked}"
    );
}

/// With nothing tracked yet, a file on disk keeps its glob alive: the dead
/// glob check falls back to the file system as `check-knowledge` does.
#[test]
fn add_reads_the_file_system_when_the_index_is_empty() {
    let project = Project::new();
    git(project.root(), &["rm", "-r", "-q", "--cached", "."]);
    let pack = pack_value(
        "team",
        "1.0.0",
        &["team"],
        &json!({"global": {"paths": []}, "tools": {"paths": ["tools/**", "absent/**"]}}),
        &[entry("team.alpha", "A")],
    );
    let file = vendor_pack(&project, &pack);

    let lines = succeeded(&project.run(&["pack", "add", &file]));

    assert_eq!(
        lines[1..],
        [
            "added area \"tools\" to knowledge/areas.json with the globs tools/**, absent/**",
            &dead_glob_line("absent/**", "tools"),
        ]
    );
}

/// Registers `raw_path`, which is no UTF-8, as a tracked file through git's
/// byte-oriented index and never a file write: macOS rejects such a name at
/// creation (`houserules.platform-gated-tests`). The blob is the one of
/// `docs/a.md`.
#[cfg(unix)]
fn track_a_path_that_is_not_utf_8(project: &Project, raw_path: &[u8]) {
    use std::os::unix::ffi::OsStrExt;

    let blob = git(project.root(), &["rev-parse", ":docs/a.md"]);
    let status = Command::new("git")
        .args(["update-index", "--add", "--cacheinfo", "100644", &blob])
        .arg(std::ffi::OsStr::from_bytes(raw_path))
        .current_dir(project.root())
        .status()
        .expect("run git update-index");
    assert!(status.success(), "git update-index failed");
}

/// The dead-glob lines of `add` and of `check-knowledge` come from one list
/// of tracked files, so they never disagree on one tree. A tracked path that
/// is no UTF-8 is named and left out of the scan by both: the only file
/// under `x/` is such a path, so the glob `x/**` is dead for both. While the
/// area keeps the pack's globs, `check-knowledge` does not report the glob
/// (spec C16); once the project changes the globs, it does.
#[cfg(unix)]
#[test]
fn add_and_check_knowledge_agree_on_a_tracked_path_that_is_not_utf_8() {
    let project = Project::gated();
    project.write(
        "knowledge/areas.json",
        r#"{"global":{"paths":[]},"process":{"paths":[]},"docs":{"paths":["docs/**"]}}"#,
    );
    track_a_path_that_is_not_utf_8(&project, b"x/\xe9.md");
    let pack = pack_value(
        "team",
        "1.0.0",
        &["team"],
        &json!({"global": {"paths": []}, "tools": {"paths": ["x/**"]}}),
        &[entry("team.alpha", "A")],
    );
    let file = vendor_pack(&project, &pack);

    let lines = succeeded(&project.run(&["pack", "add", &file]));

    assert_eq!(
        lines,
        [
            "added pack team 1.0.0 from file:vendor/team.json",
            "added area \"tools\" to knowledge/areas.json with the globs x/**",
            "a tracked path is not valid UTF-8, skipped from the dead-glob scan: x/\u{fffd}.md",
            &dead_glob_line("x/**", "tools"),
        ]
    );
    let skipped_line = "knowledge/areas.json: a tracked path is not valid UTF-8, skipped from the dead-glob scan: x/\u{fffd}.md\n";
    let checked = project.run(&["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(1));
    assert_eq!(stderr_text(&checked), skipped_line);

    project.write(
        "knowledge/areas.json",
        r#"{"global":{"paths":[]},"process":{"paths":[]},"docs":{"paths":["docs/**"]},"tools":{"paths":["x/**","y/**"]}}"#,
    );
    succeeded(&project.run(&["render"]));
    let changed = project.run(&["check-knowledge"]);
    assert_eq!(changed.status.code(), Some(1));
    assert_eq!(
        stderr_text(&changed),
        format!(
            "{skipped_line}\
             knowledge/areas.json.tools.paths: \"x/**\" matches no tracked file (git ls-files)\n\
             knowledge/areas.json.tools.paths: \"y/**\" matches no tracked file (git ls-files)\n"
        )
    );
}

/// A pack glob that does not compile refuses the add before any write:
/// written to `areas.json`, it would break every later command.
#[test]
fn add_refuses_an_area_glob_that_does_not_compile() {
    let project = Project::new();
    let pack = pack_value(
        "team",
        "1.0.0",
        &["team"],
        &json!({"global": {"paths": []}, "bad": {"paths": ["a[z-a]b"]}}),
        &[entry("team.alpha", "A")],
    );
    let file = vendor_pack(&project, &pack);
    let before = project.snapshot();

    let output = project.run(&["pack", "add", &file]);

    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with(
            "pack team declares the glob \"a[z-a]b\" for area \"bad\", which does not compile: "
        ),
        "got: {message}"
    );
    assert_eq!(message.lines().count(), 1, "one line only: {message}");
    tree_snapshot::assert_same_tree(&before, &project.snapshot(), "a refusal");
}

/// An `areas.json` that is no JSON object refuses the add, naming the file.
#[test]
fn add_names_an_areas_file_that_is_no_object() {
    let project = Project::new();
    project.write("knowledge/areas.json", "[]\n");
    let pack = pack_value(
        "team",
        "1.0.0",
        &["team"],
        &json!({"global": {"paths": []}, "tools": {"paths": ["tools/**"]}}),
        &[entry("team.alpha", "A")],
    );
    let file = vendor_pack(&project, &pack);

    refused_changing_nothing(
        &project,
        &["pack", "add", &file],
        &format!(
            "{}: not a JSON object",
            project.path("knowledge/areas.json").display()
        ),
    );
}

// ---- the command surface ----

/// `pack --help` lists the four subcommands; `pack add --help` names the
/// three source forms.
#[test]
fn help_names_the_subcommands_and_the_three_source_forms() {
    let output = houserules().args(["pack", "--help"]).output().expect("run");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    for subcommand in ["list", "add", "remove", "upgrade"] {
        assert!(
            text.lines()
                .any(|line| line.trim_start().starts_with(subcommand)),
            "pack --help lacks {subcommand}: {text}"
        );
    }

    let output = houserules()
        .args(["pack", "add", "--help"])
        .output()
        .expect("run");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(text.contains("git+<url>@<tag>#<path>"), "got: {text}");
    assert!(text.contains("kit pack"), "got: {text}");
    assert!(text.contains("pack file"), "got: {text}");

    let output = houserules()
        .args(["pack", "list", "--help"])
        .output()
        .expect("run");
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(text.contains("`unpinned`"), "got: {text}");
    assert!(text.contains("`no copy`"), "got: {text}");
}

/// `pack` with no subcommand, and `pack upgrade` with no name, are usage
/// errors (exit 2).
#[test]
fn a_missing_subcommand_or_name_is_a_usage_error() {
    for args in [
        vec!["pack"],
        vec!["pack", "upgrade"],
        vec!["pack", "add"],
        vec!["pack", "remove"],
    ] {
        let output = houserules().args(&args).output().expect("run");
        assert_eq!(output.status.code(), Some(2), "args: {args:?}");
        let message = stderr_text(&output);
        assert!(
            message.contains("Usage: houserules pack"),
            "args: {args:?}, got: {message}"
        );
    }
}

/// `pack` outside an install names the marker the same way every command
/// that reads the install does: D4 refuses a tree with no `.houserules.json`
/// before the command loads the knowledge base.
#[test]
fn a_directory_without_an_install_is_one_named_line() {
    let dir = tempfile::tempdir().expect("tempdir");

    let output = houserules()
        .args(["pack", "list", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        stderr_text(&output),
        format!(
            "{}: not found; run houserules update --id-prefix <prefix> to convert an install of houserules 1.x, or houserules init to start a new one\n",
            dir.path().join(".houserules.json").display()
        )
    );
}

// ---- more cases ----

/// A tag that moves between the resolution of the tag and the fetch is
/// refused: the pin would otherwise name a commit that the copy did not
/// come from. A `git` wrapper on the path moves the tag right before the
/// fetch.
#[cfg(unix)]
#[test]
fn add_refuses_a_tag_that_moved_while_it_was_fetched() {
    use std::os::unix::fs::PermissionsExt;

    let host = Host::new();
    let first = simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]);
    host.publish(&[("packs/team.json", &first)], "v1", false);
    let second = simple_pack("team", "1.0.1", &[entry("team.alpha", "Moved")]);
    let moved = host.publish(&[("packs/team.json", &second)], "spare", false);
    let real_git = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .expect("find git");
    let real_git = String::from_utf8_lossy(&real_git.stdout).trim().to_string();
    let wrapper = tempfile::tempdir().expect("tempdir");
    let script = wrapper.path().join("git");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nif [ \"$1\" = fetch ]; then\n  '{real_git}' --git-dir '{}' update-ref refs/tags/v1 {moved}\nfi\nexec '{real_git}' \"$@\"\n",
            host.bare().display()
        ),
    )
    .expect("write the wrapper");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("make it executable");
    let project = Project::new();
    let source = host.source("v1", "packs/team.json");
    let before = project.snapshot();
    let path = format!(
        "{}:{}",
        wrapper.path().display(),
        std::env::var("PATH").expect("a PATH")
    );

    let output = houserules()
        .args(["pack", "add", &source, "--dir"])
        .arg(project.root())
        .env("PATH", path)
        .output()
        .expect("run houserules");

    refused_with(
        &output,
        &format!(
            "{source}: the tag \"v1\" moved while houserules fetched it; run the command again"
        ),
    );
    tree_snapshot::assert_same_tree(&before, &project.snapshot(), "a refusal");
}

/// A pin with no copy can be removed: the pin goes, and nothing else.
#[test]
fn remove_deletes_a_pin_that_has_no_copy() {
    let project = Project::new();
    let mut marker = project.read_json(".houserules.json");
    marker["packs"] = json!({"ghost": {
        "version": "1.0.0", "source": "kit", "sha256": "0", "addedAreas": [],
    }});
    project.write_json(".houserules.json", &marker);

    let lines = succeeded(&project.run(&["pack", "remove", "ghost"]));

    assert_eq!(lines, ["removed pack ghost 1.0.0"]);
    assert_eq!(pins(&project), json!({}));
}

/// An upgrade of a pack with a pin and no copy refuses.
#[test]
fn upgrade_refuses_a_pin_that_has_no_copy() {
    let project = Project::new();
    let mut marker = project.read_json(".houserules.json");
    marker["packs"] = json!({"ghost": {
        "version": "1.0.0", "source": "file:vendor/ghost.json", "sha256": "0", "addedAreas": [],
    }});
    project.write_json(".houserules.json", &marker);

    refused_changing_nothing(
        &project,
        &["pack", "upgrade", "ghost"],
        "pack ghost has a pin and no copy in knowledge/packs; remove it and add it again",
    );
}

/// Run with no `--dir` from a directory below the install root, every
/// subcommand finds the install: the nearest `.houserules.json` decides.
#[test]
fn a_command_without_dir_finds_the_install_from_a_subdirectory() {
    let project = Project::new();

    let output = houserules()
        .args(["pack", "list"])
        .current_dir(project.path("docs"))
        .output()
        .expect("run houserules");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        stderr_text(&output)
    );
    let printed: Value = serde_json::from_slice(&output.stdout).expect("stdout is JSON");
    assert_eq!(printed["installed"], json!([]));
}

/// Spec 16: after the archived case of L8, the gates pass. The tree holds
/// the project's real knowledge schema, so `check-knowledge` judges every
/// file the commands wrote. The override that the case wrote is listed on
/// stdout before `knowledge: ok`, and the list does not fail the gate
/// (spec 4.1).
#[test]
fn add_leaves_check_knowledge_green_after_the_archived_case() {
    let project = Project::new();
    project.write(
        "knowledge/schema.json",
        &fs::read_to_string(
            repo_root()
                .join("template")
                .join("knowledge")
                .join("schema.json"),
        )
        .expect("read the schema"),
    );
    project.write_json(
        "knowledge/areas.json",
        &json!({
            "global": {"paths": []},
            "process": {"paths": []},
            "docs": {"paths": ["docs/**"]},
            "tools": {"paths": ["tools/**"]},
        }),
    );
    project.write("AGENTS.md", "# Agents\n");
    let keep = entry("team.keep", "Keep");
    let old = entry("team.old", "Old");
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[keep.clone(), old.clone()]),
    );
    local_topic(&project, "team", &[keep]);
    let mut retired = old;
    retired["status"] = json!("retired");
    project.write_json(
        "knowledge/archive/team.json",
        &json!({"topic": "team", "title": "team title", "entries": [retired]}),
    );
    git(project.root(), &["add", "-A"]);
    succeeded(&project.run(&["render"]));
    let baseline = project.run(&["check-knowledge"]);
    assert_eq!(
        baseline.status.code(),
        Some(0),
        "stderr: {}",
        stderr_text(&baseline)
    );

    succeeded(&project.run(&["pack", "add", &file]));
    git(project.root(), &["add", "-A"]);
    succeeded(&project.run(&["render"]));

    let checked = project.run(&["check-knowledge"]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "stderr: {}",
        stderr_text(&checked)
    );
    assert_eq!(
        stdout_lines(&checked),
        ["override team.old: status", "knowledge: ok"]
    );
    assert!(!project.exists("knowledge/archive/team.json"));
    assert!(!project.exists("knowledge/team.json"));
    assert_eq!(
        project.read_json("knowledge/overrides.json")["overrides"],
        json!([{"id": "team.old", "fields": {"status": "retired"}}])
    );
}

// ---- render after a change (spec 4.2) ----

/// The two generated files that a change of the topic list rewrites: the
/// block of the root `AGENTS.md` holds the standing rules, and the knowledge
/// skill holds the topic list.
const RENDERED_BY_A_TOPIC_CHANGE: [&str; 2] = [
    "AGENTS.md: written",
    ".claude/skills/project-knowledge/SKILL.md: written",
];

/// The one generated file that a change of a standing entry alone rewrites.
const RENDERED_BY_A_STANDING_ENTRY: [&str; 1] = ["AGENTS.md: written"];

/// Every pack command that changes files ends with the render that `init`
/// and `update` run, so the generated files never lag the knowledge base.
/// The receipt prints first, then the report of the render; right after
/// each command both gates pass.
#[test]
fn add_upgrade_and_remove_end_with_a_render_that_keeps_both_gates_green() {
    let project = Project::gated();
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "Alpha")]),
    );

    let (receipt, report) = succeeded_with_render_report(&project.run(&["pack", "add", &file]));
    assert_eq!(
        receipt,
        ["added pack team 1.0.0 from file:vendor/team.json"]
    );
    assert_eq!(report, RENDERED_BY_A_TOPIC_CHANGE);
    project.assert_gates_green();

    project.write_json(
        "vendor/team.json",
        &simple_pack("team", "1.1.0", &[entry("team.alpha", "Changed")]),
    );
    let (receipt, report) =
        succeeded_with_render_report(&project.run(&["pack", "upgrade", "team"]));
    assert_eq!(receipt, ["upgraded pack team from 1.0.0 to 1.1.0"]);
    assert_eq!(report, RENDERED_BY_A_STANDING_ENTRY);
    project.assert_gates_green();

    let (receipt, report) = succeeded_with_render_report(&project.run(&["pack", "remove", "team"]));
    assert_eq!(receipt, ["removed pack team 1.1.0"]);
    assert_eq!(report, RENDERED_BY_A_TOPIC_CHANGE);
    project.assert_gates_green();
}

/// `list` changes nothing and a refusal changes nothing: neither renders,
/// so a stale generated file stays as it was.
#[test]
fn list_and_a_refusal_leave_a_stale_generated_file_stale() {
    let project = Project::gated();
    let stale = "<!-- houserules:begin -->\nstale\n<!-- houserules:end -->\n";
    project.write("AGENTS.md", stale);

    succeeded(&project.run(&["pack", "list"]));
    assert_eq!(project.read("AGENTS.md"), stale);

    refused_changing_nothing(
        &project,
        &["pack", "remove", "nothing"],
        "pack nothing is not installed",
    );
    assert_eq!(project.read("AGENTS.md"), stale);
}

/// A render that fails after the write is one line that says the pack
/// command already wrote its files and names the next step.
#[test]
fn a_render_failure_after_the_write_is_one_line_that_says_so() {
    let project = Project::gated();
    project.write(".claude/rules", "not a directory\n");
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "Alpha")]),
    );

    let output = project.run(&["pack", "add", &file]);

    assert_eq!(output.status.code(), Some(2));
    let message = stderr_text(&output);
    assert!(
        message.starts_with("render failed after the pack command wrote its files: "),
        "got: {message}"
    );
    assert!(
        message.ends_with("; fix the cause, then run houserules render\n"),
        "got: {message}"
    );
    assert_eq!(message.lines().count(), 1, "one line only: {message}");
    assert_eq!(
        stdout_lines(&output),
        ["added pack team 1.0.0 from file:vendor/team.json"]
    );
    assert!(project.exists("knowledge/packs/team.json"));
}

// ---- a pack command writes a file only when its content changes (spec 4.2) ----

/// The modification time of `relative`.
fn modified(project: &Project, relative: &str) -> SystemTime {
    fs::metadata(project.path(relative))
        .and_then(|metadata| metadata.modified())
        .expect("read the modification time")
}

/// Sets the modification time of `relative`.
fn set_modified(project: &Project, relative: &str, time: SystemTime) {
    fs::File::options()
        .write(true)
        .open(project.path(relative))
        .expect("open the file")
        .set_modified(time)
        .expect("set the modification time");
}

/// `upgrade` of a file pack whose file did not change writes no file. Each
/// file starts at a time long past, so any write, even of the same bytes,
/// moves it: the copy, the pin, `areas.json`, and the two generated files.
#[test]
fn upgrade_of_an_unchanged_pack_writes_no_file() {
    let project = Project::gated();
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]),
    );
    succeeded(&project.run(&["pack", "add", &file]));
    let files = [
        "knowledge/packs/team.json",
        ".houserules.json",
        "knowledge/areas.json",
        "AGENTS.md",
        ".claude/skills/project-knowledge/SKILL.md",
    ];
    let long_past = UNIX_EPOCH + Duration::from_secs(86_400);
    for relative in files {
        set_modified(&project, relative, long_past);
    }
    let before = project.snapshot();

    let (receipt, report) =
        succeeded_with_render_report(&project.run(&["pack", "upgrade", "team"]));

    assert_eq!(receipt, ["upgraded pack team from 1.0.0 to 1.0.0"]);
    assert_eq!(report, ["render: up to date"]);
    for relative in files {
        assert_eq!(
            modified(&project, relative),
            long_past,
            "{relative} was written"
        );
    }
    tree_snapshot::assert_same_tree(&before, &project.snapshot(), "the command");
}

/// A pack that adds no area leaves a hand-formatted `areas.json` byte for
/// byte: `add`, `upgrade`, and `remove` each skip the write. The file is one
/// line, so a write in emit form would change it.
#[test]
fn a_pack_that_adds_no_area_leaves_a_hand_formatted_areas_json_byte_for_byte() {
    let project = Project::gated();
    assert_eq!(
        project.read("knowledge/areas.json"),
        HAND_FORMATTED_AREAS,
        "the premise"
    );
    let file = vendor_pack(
        &project,
        &simple_pack("team", "1.0.0", &[entry("team.alpha", "A")]),
    );

    let receipt = succeeded(&project.run(&["pack", "add", &file]));
    assert_eq!(
        receipt,
        ["added pack team 1.0.0 from file:vendor/team.json"]
    );
    assert_eq!(project.read("knowledge/areas.json"), HAND_FORMATTED_AREAS);

    project.write_json(
        "vendor/team.json",
        &simple_pack("team", "1.1.0", &[entry("team.alpha", "A")]),
    );
    succeeded(&project.run(&["pack", "upgrade", "team"]));
    assert_eq!(project.read("knowledge/areas.json"), HAND_FORMATTED_AREAS);

    succeeded(&project.run(&["pack", "remove", "team"]));
    assert_eq!(project.read("knowledge/areas.json"), HAND_FORMATTED_AREAS);
}

/// `pack add --help`, `pack remove --help`, and `pack upgrade --help` say
/// that the command renders.
#[test]
fn the_help_of_each_changing_pack_command_says_it_renders() {
    for subcommand in ["add", "remove", "upgrade"] {
        let output = houserules()
            .args(["pack", subcommand, "--help"])
            .output()
            .expect("run");
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            text.contains("renders the generated files"),
            "pack {subcommand} --help does not say it renders: {text}"
        );
    }
}
