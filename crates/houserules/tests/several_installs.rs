//! `init --dir <subdir>` and several installs in one repository (batch 33,
//! spec section 8, C12): an install below the git top level keeps `.claude/`
//! and `.githooks/` in its root, writes its CI workflow at the top level as
//! `.github/workflows/knowledge-<folder>.yml` with `working-directory`, a
//! `paths` filter, and its own name, and baselines that file under git's
//! top-level pathspec form. `init` refuses a second install whose workflow
//! file name or root clashes with another install. Each case runs the
//! compiled binary against a scratch git repository with a GitHub origin.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

mod tree_snapshot;

use tree_snapshot::{assert_same_tree, snapshot};

/// `path` canonical, in the form houserules prints it: on Windows without
/// the `\\?\` prefix that `canonicalize` adds.
fn resolved(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().expect("canonicalize");
    match canonical
        .to_str()
        .and_then(|text| text.strip_prefix(r"\\?\"))
    {
        Some(plain) => PathBuf::from(plain),
        None => canonical,
    }
}

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

/// A scratch git repository whose `origin` is a GitHub URL, so `init`
/// writes the CI workflow. The path is canonical, so the paths the binary
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

/// `init --dir sub` installs in `sub`: `.claude/`, `.githooks/`, the
/// knowledge base, and the marker live in the install root, and the one file
/// outside it is the CI workflow at the top level, in the subdirectory form
/// and baselined under git's top-level pathspec form.
#[test]
fn init_dir_installs_in_the_subdirectory_and_writes_its_workflow_at_the_top_level() {
    let (_dir, top) = github_repo();
    fs::create_dir_all(top.join("sub")).expect("create sub/");
    fs::write(top.join("other.txt"), "other content\n").expect("write other.txt");
    fs::write(top.join("sub/code.rs"), "fn main() {}\n").expect("write code.rs");

    let lines = init_ok(&top.join("sub"));

    for file in [
        ".houserules.json",
        ".githooks/commit-msg",
        ".claude/skills/orchestrating/SKILL.md",
        "knowledge/schema.json",
        "knowledge/packs/process.json",
        "AGENTS.md",
    ] {
        assert!(top.join("sub").join(file).is_file(), "sub/{file}");
    }
    let outside: Vec<String> = snapshot(&top)
        .into_keys()
        .filter(|path| !path.starts_with("sub/"))
        .collect();
    assert_eq!(
        outside,
        [".github/workflows/knowledge-sub.yml", "other.txt"]
    );
    assert_eq!(
        fs::read_to_string(top.join("sub/code.rs")).unwrap(),
        "fn main() {}\n"
    );
    assert!(
        lines.contains(&"wrote .github/workflows/knowledge-sub.yml".to_string()),
        "{lines:?}"
    );
    assert_subdirectory_workflow(&top, "knowledge-sub.yml", "sub");
    let baselines = marker(&top.join("sub"))["baselines"].clone();
    assert!(
        baselines[":/.github/workflows/knowledge-sub.yml"].is_string(),
        "{baselines}"
    );
}

/// An install at the top level keeps `.github/workflows/knowledge.yml`, the
/// template as it is, and baselines it under the same top-level form.
#[test]
fn an_install_at_the_top_level_keeps_the_template_workflow_and_baselines_it() {
    let (_dir, top) = github_repo();

    init_ok(&top);

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
            [
                "knowledge-base",
                "process",
                "quality",
                "security-hygiene",
                "writing-style"
            ],
            "{}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Several installs in one repository: each writes its own workflow file,
/// `<folder>` being the root's path with each `/` replaced by `-`, and
/// `update --dir` in each rewrites only its own file.
#[test]
fn several_installs_each_write_and_update_only_their_own_workflow() {
    let (_dir, top) = github_repo();
    init_ok(&top.join("a"));
    init_ok(&top.join("b/c"));

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
    fs::remove_file(workflows.join("knowledge-a.yml")).expect("remove a's workflow");
    fs::remove_file(workflows.join("knowledge-b-c.yml")).expect("remove b/c's workflow");
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
    assert!(workflows.join("knowledge-a.yml").is_file());
    assert!(!workflows.join("knowledge-b-c.yml").exists());

    let stdout = update("b/c");
    assert!(
        stdout.contains("wrote .github/workflows/knowledge-b-c.yml\n"),
        "{stdout}"
    );
    assert_subdirectory_workflow(&top, "knowledge-b-c.yml", "b/c");
}

/// `init` refuses, changing nothing, with one line that names the other
/// install: a root inside another install, a root that holds another
/// install, and a workflow file name that another install already uses
/// (`b-c` and `b/c`). Each line is pinned whole.
#[test]
fn init_refuses_a_nested_install_and_a_workflow_name_clash_and_changes_nothing() {
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
        (
            top.join("b-c"),
            format!(
                "{}: the CI workflow file .github/workflows/knowledge-b-c.yml belongs to the install at {}; give this install a directory whose path makes another file name\n",
                top.join("b-c").display(),
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
        lines.contains(&"kept .github/workflows/knowledge-a.yml".to_string()),
        "{lines:?}"
    );
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
/// `working-directory`, and the baseline key valid (quoted in YAML).
#[test]
fn a_folder_name_with_a_space_a_non_ascii_letter_or_a_quote_gives_a_valid_workflow() {
    for (folder, file) in [
        ("my dir", "knowledge-my dir.yml"),
        ("über/ça va", "knowledge-über-ça va.yml"),
        ("o'clock", "knowledge-o'clock.yml"),
    ] {
        let (_dir, top) = github_repo();

        init_ok(&top.join(folder));

        assert_subdirectory_workflow(&top, file, folder);
        let key = format!(":/.github/workflows/{file}");
        assert!(
            marker(&top.join(folder))["baselines"][key.as_str()].is_string(),
            "{folder}"
        );
    }
}

/// A folder name with a character that a GitHub path filter reads as a
/// pattern is refused with one line, and nothing is written: the filter
/// would never match the folder.
#[test]
fn init_refuses_a_folder_name_that_a_path_filter_reads_as_a_pattern() {
    let (_dir, top) = github_repo();
    let target = top.join("a+b");
    fs::create_dir_all(&target).expect("create the target");
    let before = snapshot(&top);

    let output = init(&target);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!(
            "{}: the install path \"a+b\" holds the character '+', which a GitHub path filter reads as a pattern; choose a directory without it\n",
            target.display()
        )
    );
    assert_same_tree(&before, &snapshot(&top), "init in a folder named a pattern");
}

/// With no GitHub origin, a subdirectory install writes no workflow and
/// says why, naming the file it would have written.
#[test]
fn a_subdirectory_install_without_a_github_origin_skips_its_workflow() {
    let dir = tempfile::tempdir().expect("tempdir");
    git(dir.path(), &["init", "-q"]);
    let top = resolved(dir.path());

    let lines = init_ok(&top.join("sub"));

    assert!(
        lines.contains(
            &"skipped .github/workflows/knowledge-sub.yml (origin is not GitHub-hosted; see the migrating-knowledge skill to add your own CI gate)"
                .to_string()
        ),
        "{lines:?}"
    );
    assert!(!top.join(".github").exists());
}

/// A directory below the top level that holds no install is no install
/// root: `update` there refuses with one line and changes nothing.
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
            "{}: this directory is inside the git repository at {} and holds no .houserules.json; run houserules init --dir {} to install there\n",
            top.join("sub").display(),
            top.display(),
            top.join("sub").display()
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
        "check-commit: skipped 1 commit with no change under sub/\ncheck-commit: ok\n",
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(0));
}
