//! `init --dir <subdir>` and several installs in one repository (batch 33,
//! spec section 8, C12): an install below the git top level keeps `.claude/`
//! and `.githooks/` in its root and writes nothing outside it (spec 10.7). An
//! install that an earlier release seeded keeps its CI workflow at the top
//! level as `.github/workflows/knowledge-<folder>.yml` with `working-directory`,
//! a `paths` filter, and its own name, baselined under the top-level pathspec
//! form of the repository, and `update` keeps that file current
//! (`seed_workflow` makes the state). `init` refuses a second install whose
//! root nests with another install. Each case runs the compiled binary against
//! a scratch repository with a GitHub origin.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

mod tracker_support;
mod tree_snapshot;

use tracker_support::{quoted, resolved, seed_workflow, sha256_hex};
use tree_snapshot::{assert_same_tree, snapshot};

/// A `Command` for the compiled `houserules` binary under test.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// This checkout's repository root.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Runs `git` in `dir` and requires success.
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// A scratch repository whose `origin` is a GitHub URL, which `init` does not
/// use to write a CI file. The path is canonical, so the paths the binary
/// prints match the paths built here.
fn github_repo() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q"]);
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/example/example.git",
        ],
    );
    let root = resolved(dir.path());
    (dir, root)
}

/// Runs `init --dir <dir>` and returns the output.
fn init(dir: &Path) -> Output {
    houserules()
        .args(["init", "--dir"])
        .arg(dir)
        .output()
        .expect("run init")
}

/// Creates `dir` (`init` never creates its target), runs `init --dir <dir>`,
/// and requires exit 0.
fn init_ok(dir: &Path) -> Vec<String> {
    fs::create_dir_all(dir).expect("create the target");
    let output = init(dir);
    assert!(
        output.status.success(),
        "init {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("utf8 stdout")
        .lines()
        .map(str::to_string)
        .collect()
}

/// The marker of the install at `dir`.
fn marker(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join(".houserules.json")).expect("read marker"))
        .expect("parse the marker")
}

/// The text of the workflow template.
fn template_workflow() -> String {
    fs::read_to_string(repo_root().join("template/.github/workflows/knowledge.yml"))
        .expect("read the template workflow")
}

/// The lines of `generated` that `template` does not hold, when `template`'s
/// lines all appear in `generated` in order; `None` otherwise.
fn lines_added_to(template: &str, generated: &str) -> Option<Vec<String>> {
    let mut remaining = template.lines().peekable();
    let mut added = Vec::new();
    for line in generated.lines() {
        if remaining.peek() == Some(&line) {
            remaining.next();
        } else {
            added.push(line.to_string());
        }
    }
    remaining.peek().is_none().then_some(added)
}

/// The lines that the workflow of an install at `path` holds beyond the
/// template: the name line, `name`, and seven lines added (the two `paths`
/// filters and the job's `defaults`).
fn expected_added_lines(path: &str, name: &str) -> Vec<String> {
    let glob = format!("'{}/**'", path.replace('\'', "''"));
    let directory = format!("'{}'", path.replace('\'', "''"));
    vec![
        format!("name: '{}'", name.replace('\'', "''")),
        "    paths:".to_string(),
        format!("      - {glob}"),
        "    paths:".to_string(),
        format!("      - {glob}"),
        "    defaults:".to_string(),
        "      run:".to_string(),
        format!("        working-directory: {directory}"),
    ]
}

/// Asserts that the workflow of the install at `path` is the template in
/// the subdirectory form, with every added line at its place.
fn assert_subdirectory_workflow(top: &Path, file: &str, path: &str) {
    let generated = fs::read_to_string(top.join(".github/workflows").join(file))
        .unwrap_or_else(|error| panic!("read {file}: {error}"));
    let template = template_workflow();
    // The template's own name line is the one line the form changes, so it
    // is the one template line that `generated` lacks.
    let without_name: String = template
        .lines()
        .filter(|line| *line != "name: knowledge")
        .map(|line| format!("{line}\n"))
        .collect();
    let added = lines_added_to(&without_name, &generated)
        .unwrap_or_else(|| panic!("{file} lost a line of the template:\n{generated}"));
    assert_eq!(
        added,
        expected_added_lines(path, &format!("knowledge ({path})")),
        "{file}"
    );
    let lines: Vec<&str> = generated.lines().collect();
    let at = |needle: &str| {
        lines
            .iter()
            .position(|line| *line == needle)
            .unwrap_or_else(|| panic!("{file} has no line {needle:?}"))
    };
    assert_eq!(
        lines[0],
        format!(
            "name: '{}'",
            format!("knowledge ({path})").replace('\'', "''")
        )
    );
    let glob = format!("      - '{}/**'", path.replace('\'', "''"));
    assert_eq!(lines[at("  pull_request:") + 1], "    paths:");
    assert_eq!(lines[at("  pull_request:") + 2], glob);
    assert_eq!(lines[at("    branches: [main]") + 1], "    paths:");
    assert_eq!(lines[at("    branches: [main]") + 2], glob);
    assert_eq!(lines[at("    runs-on: ubuntu-latest") + 1], "    defaults:");
    assert_eq!(lines[at("    runs-on: ubuntu-latest") + 2], "      run:");
}

/// `init --dir sub` installs in `sub`: `.claude/`, `tools/`, the
/// knowledge base, and the marker live in the install root, and `init` writes
/// no file outside it (spec 10.7).
#[test]
fn init_dir_installs_in_the_subdirectory_and_writes_nothing_outside_it() {
    let (_dir, top) = github_repo();
    fs::create_dir_all(top.join("sub")).expect("create sub/");
    fs::write(top.join("other.txt"), "other content\n").expect("write other.txt");
    fs::write(top.join("sub/code.rs"), "fn main() {}\n").expect("write code.rs");

    let lines = init_ok(&top.join("sub"));

    for file in [
        ".houserules.json",
        ".claude/skills/houserules-step/SKILL.md",
        "tools/houserules-step.sh",
        "knowledge/schema.json",
        "knowledge/packs/knowledge-base.json",
        "AGENTS.md",
    ] {
        assert!(top.join("sub").join(file).is_file(), "sub/{file}");
    }
    let outside: Vec<String> = snapshot(&top)
        .into_keys()
        .filter(|path| !path.starts_with("sub/"))
        .collect();
    assert_eq!(outside, ["other.txt"]);
    assert_eq!(
        fs::read_to_string(top.join("sub/code.rs")).unwrap(),
        "fn main() {}\n"
    );
    assert!(
        !lines.iter().any(|line| line.contains("knowledge-sub.yml")),
        "{lines:?}"
    );
    let baselines = marker(&top.join("sub"))["baselines"].clone();
    assert!(
        baselines[":/.github/workflows/knowledge-sub.yml"].is_null(),
        "{baselines}"
    );
}

/// A subdirectory install that an earlier release seeded keeps its workflow at
/// the top level in the subdirectory form, baselined under the top-level
/// pathspec form of the repository; `update` keeps that file current.
#[test]
fn a_seeded_subdirectory_install_keeps_its_workflow_in_the_subdirectory_form() {
    let (_dir, top) = github_repo();
    init_ok(&top.join("sub"));

    let file = seed_workflow(&top, &top.join("sub"));

    assert_eq!(file, ".github/workflows/knowledge-sub.yml");
    assert_subdirectory_workflow(&top, "knowledge-sub.yml", "sub");
    let baselines = marker(&top.join("sub"))["baselines"].clone();
    assert!(
        baselines[":/.github/workflows/knowledge-sub.yml"].is_string(),
        "{baselines}"
    );
}

/// An install at the top level that an earlier release seeded keeps
/// `.github/workflows/knowledge.yml`, the template as it is, baselined under
/// the same top-level form, and `update` leaves it as it is. A plain `init`
/// writes none.
#[test]
fn an_install_at_the_top_level_keeps_the_template_workflow_and_baselines_it() {
    let (_dir, top) = github_repo();
    init_ok(&top);
    assert!(!top.join(".github").exists(), "init writes no workflow");

    seed_workflow(&top, &top);
    let output = houserules()
        .args(["update", "--dir"])
        .arg(&top)
        .output()
        .expect("run update");
    assert!(output.status.success());

    assert_eq!(
        fs::read_to_string(top.join(".github/workflows/knowledge.yml")).unwrap(),
        template_workflow()
    );
    assert!(marker(&top)["baselines"][":/.github/workflows/knowledge.yml"].is_string());
}

/// Every command run from inside the install finds it (spec 16): the
/// nearest `.houserules.json` is the root. `check-backlog` reads the
/// install's own `backlog/`, which the top level does not have, and
/// `topics` reads the install's own packs.
#[test]
fn every_command_from_inside_the_subdirectory_finds_the_install() {
    let (_dir, top) = github_repo();
    init_ok(&top.join("sub"));
    let tracker = houserules()
        .args(["init", "--tracker", "json", "--dir"])
        .arg(top.join("sub"))
        .output()
        .expect("run init --tracker json");
    assert!(
        tracker.status.success(),
        "init --tracker json failed: {}",
        String::from_utf8_lossy(&tracker.stderr)
    );
    fs::create_dir_all(top.join("sub/deeper")).expect("create deeper/");

    for cwd in [top.join("sub"), top.join("sub/deeper")] {
        let output = houserules()
            .arg("check-backlog")
            .current_dir(&cwd)
            .output()
            .expect("run check-backlog");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "backlog: ok\n",
            "{}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(0));

        let output = houserules()
            .arg("topics")
            .current_dir(&cwd)
            .output()
            .expect("run topics");
        let topics: Value = serde_json::from_slice(&output.stdout).expect("topics prints JSON");
        let names: Vec<&str> = topics
            .as_array()
            .expect("an array of topics")
            .iter()
            .map(|topic| topic["topic"].as_str().expect("a topic name"))
            .collect();
        assert_eq!(
            names,
            ["knowledge-base"],
            "{}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Several installs in one repository that an earlier release seeded: each
/// holds its own workflow file, `<folder>` being the root's path with each `/`
/// replaced by `-`, and `update --dir` in each rewrites only its own file.
#[test]
fn several_installs_each_update_only_their_own_workflow() {
    let (_dir, top) = github_repo();
    init_ok(&top.join("a"));
    init_ok(&top.join("b/c"));
    seed_workflow(&top, &top.join("a"));
    seed_workflow(&top, &top.join("b/c"));

    assert_subdirectory_workflow(&top, "knowledge-a.yml", "a");
    assert_subdirectory_workflow(&top, "knowledge-b-c.yml", "b/c");
    for (root, key) in [
        ("a", ":/.github/workflows/knowledge-a.yml"),
        ("b/c", ":/.github/workflows/knowledge-b-c.yml"),
    ] {
        assert!(
            marker(&top.join(root))["baselines"][key].is_string(),
            "{root}"
        );
    }

    let workflows = top.join(".github/workflows");
    let stale = template_workflow();
    for (root, file) in [("a", "knowledge-a.yml"), ("b/c", "knowledge-b-c.yml")] {
        fs::write(workflows.join(file), &stale).expect("write the stale workflow");
        let mut stamp = marker(&top.join(root));
        stamp["baselines"][format!(":/.github/workflows/{file}")] =
            Value::String(sha256_hex(stale.as_bytes()));
        fs::write(
            top.join(root).join(".houserules.json"),
            format!("{}\n", serde_json::to_string_pretty(&stamp).unwrap()),
        )
        .expect("write the marker");
    }
    let update = |root: &str| {
        let output = houserules()
            .args(["update", "--dir"])
            .arg(top.join(root))
            .output()
            .expect("run update");
        assert!(
            output.status.success(),
            "update {root}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf8 stdout")
    };

    let stdout = update("a");
    assert!(
        stdout.contains("wrote .github/workflows/knowledge-a.yml\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("knowledge-b-c.yml"), "{stdout}");
    assert_subdirectory_workflow(&top, "knowledge-a.yml", "a");
    assert_eq!(
        fs::read_to_string(workflows.join("knowledge-b-c.yml")).unwrap(),
        stale,
        "update in a leaves the workflow of b/c"
    );

    let stdout = update("b/c");
    assert!(
        stdout.contains("wrote .github/workflows/knowledge-b-c.yml\n"),
        "{stdout}"
    );
    assert_subdirectory_workflow(&top, "knowledge-b-c.yml", "b/c");
}

/// `init` refuses, changing nothing, with one line that names the other
/// install: a root inside another install, and a root that holds another
/// install. Each line is pinned whole.
#[test]
fn init_refuses_a_nested_install_and_changes_nothing() {
    let (_dir, top) = github_repo();
    init_ok(&top.join("a"));
    init_ok(&top.join("b/c"));
    let before = snapshot(&top);
    let a = top.join("a");
    let bc = top.join("b").join("c");

    let cases = [
        (
            top.join("a").join("d"),
            format!(
                "{}: the install root lies inside the install at {}; houserules installs do not nest\n",
                top.join("a").join("d").display(),
                a.display()
            ),
        ),
        (
            top.clone(),
            format!(
                "{}: the install root holds the install at {}; houserules installs do not nest\n",
                top.display(),
                a.display()
            ),
        ),
        (
            top.join("b"),
            format!(
                "{}: the install root holds the install at {}; houserules installs do not nest\n",
                top.join("b").display(),
                bc.display()
            ),
        ),
    ];
    for (target, expected) in cases {
        fs::create_dir_all(&target).expect("create the target");
        let output = init(&target);
        assert_eq!(
            output.status.code(),
            Some(2),
            "{}: {}",
            target.display(),
            String::from_utf8_lossy(&output.stdout)
        );
        assert_eq!(output.stdout, b"", "{}", target.display());
        assert_eq!(String::from_utf8(output.stderr).unwrap(), expected);
    }
    // The empty directories above are the only change.
    assert_same_tree(&before, &snapshot(&top), "the refusals");
}

/// Two roots whose paths make one dashed name (`b-c` and `b/c`) are two
/// installs: `init` writes no workflow file whose name could clash.
#[test]
fn init_accepts_a_root_whose_dashed_name_equals_that_of_another_install() {
    let (_dir, top) = github_repo();
    init_ok(&top.join("b/c"));

    let lines = init_ok(&top.join("b-c"));

    assert!(top.join("b-c/.houserules.json").is_file(), "{lines:?}");
    assert!(!top.join(".github").exists());
}

/// `init` in an existing install's own root is no second install: it seeds
/// what is missing and keeps what exists (C12).
#[test]
fn init_in_the_own_root_of_an_install_is_no_second_install() {
    let (_dir, top) = github_repo();
    init_ok(&top.join("a"));
    let before = snapshot(&top);

    let lines = init_ok(&top.join("a"));

    assert_same_tree(&before, &snapshot(&top), "init in the own root");
    assert!(
        !lines.iter().any(|line| line.starts_with("wrote ")),
        "{lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains(".github/workflows")),
        "{lines:?}"
    );
}

/// `init` on a target that holds its own marker refuses no placement that
/// `update` accepts, and `update` runs no overlap check: the install is no
/// new root. Each of the two overlaps that `init` refuses for a new root
/// leaves `init` on the existing install at exit 0 and the tree as it was: a
/// marker below the root (a vendored project or a fixture), and a marker above
/// it.
#[test]
fn init_on_an_existing_install_runs_no_overlap_check() {
    for (install, other) in [("a", "a/vendor"), ("b/c", "b")] {
        let (_dir, top) = github_repo();
        init_ok(&top.join(install));
        fs::create_dir_all(top.join(other)).expect("create the other root");
        fs::write(top.join(other).join(".houserules.json"), "{}\n").expect("write the marker");
        let before = snapshot(&top);

        let lines = init_ok(&top.join(install));

        assert_same_tree(&before, &snapshot(&top), install);
        assert!(
            !lines.iter().any(|line| line.starts_with("wrote ")),
            "{install}: {lines:?}"
        );
    }
}

/// An install below a top-level install is refused, and the line names the
/// top-level install.
#[test]
fn init_below_a_top_level_install_is_refused() {
    let (_dir, top) = github_repo();
    init_ok(&top);
    let before = snapshot(&top);
    fs::create_dir_all(top.join("sub")).expect("create sub/");

    let output = init(&top.join("sub"));

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: the install root lies inside the install at {}; houserules installs do not nest\n",
            top.join("sub").display(),
            top.display()
        )
    );
    assert_same_tree(&before, &snapshot(&top), "init below a top-level install");
}

/// Review focus 5: a folder name with a space, a non-ASCII letter, or a
/// single quote keeps the workflow file name, the `paths` filter, the
/// `working-directory`, and the baseline key valid (quoted in YAML) in the
/// workflow that `update` keeps for a seeded install.
#[test]
fn a_folder_name_with_a_space_a_non_ascii_letter_or_a_quote_gives_a_valid_workflow() {
    for (folder, file) in [
        ("my dir", "knowledge-my dir.yml"),
        ("über/ça va", "knowledge-über-ça va.yml"),
        ("o'clock", "knowledge-o'clock.yml"),
    ] {
        let (_dir, top) = github_repo();

        init_ok(&top.join(folder));
        seed_workflow(&top, &top.join(folder));

        assert_subdirectory_workflow(&top, file, folder);
        let key = format!(":/.github/workflows/{file}");
        assert!(
            marker(&top.join(folder))["baselines"][key.as_str()].is_string(),
            "{folder}"
        );
    }
}

/// With no origin, a subdirectory install writes no workflow, and `init` says
/// that it wrote no CI file and names no detected host.
#[test]
fn a_subdirectory_install_without_an_origin_writes_no_workflow() {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q"]);
    let top = resolved(dir.path());

    let lines = init_ok(&top.join("sub"));

    assert!(
        lines.contains(
            &"init: houserules wrote no CI file; to run the gates and the review layer in your pipeline, run houserules ci print --host github or gitlab"
                .to_string()
        ),
        "{lines:?}"
    );
    assert!(!top.join(".github").exists());
}

/// A directory below the top level that holds no install, with none above it
/// within the top level, is no install root: `update` there refuses with the
/// line for a tree with no install (HR-216), whose remedy names the directory
/// that `--dir` gave, and changes nothing.
#[test]
fn update_in_a_subdirectory_without_an_install_refuses() {
    let (_dir, top) = github_repo();
    fs::create_dir_all(top.join("sub")).expect("create sub/");
    let before = snapshot(&top);

    let output = houserules()
        .args(["update", "--dir"])
        .arg(top.join("sub"))
        .output()
        .expect("run update");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "no houserules install here (no .houserules.json); run houserules init --dir {}\n",
            quoted(&top.join("sub"))
        )
    );
    assert_same_tree(
        &before,
        &snapshot(&top),
        "update in a subdirectory without an install",
    );
}

/// `init --dir sub`, then commits inside and outside `sub`: `check-commit
/// --from` run from inside `sub` checks the commits that touch it and counts
/// the others (spec 16).
#[test]
fn check_commit_from_counts_the_commits_outside_a_freshly_initialized_subdirectory() {
    let (_dir, top) = github_repo();
    fs::write(top.join("other.txt"), "other\n").expect("write other.txt");
    let commit = |message: &str| {
        git(&top, &["add", "-A"]);
        git(
            &top,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t.t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--no-verify",
                "-m",
                message,
            ],
        );
    };
    commit("chore: base");
    let base = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&top)
            .output()
            .expect("rev-parse")
            .stdout,
    )
    .expect("utf8")
    .trim()
    .to_string();
    init_ok(&top.join("sub"));
    commit("chore: install the kit in sub");
    fs::write(top.join("other.txt"), "more\n").expect("write other.txt");
    commit("chore: change a file outside sub");

    let output = houserules()
        .args(["check-commit", "--from", &base])
        .current_dir(top.join("sub"))
        .output()
        .expect("run check-commit");

    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "check-commit: skipped 1 commit with no change under sub/\ncheck-commit: no commits check is installed; checked nothing\n",
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
}
