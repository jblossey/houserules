//! OpenSpec beside houserules (batch 38, spec 6.1 and 6.2; acceptance cases 10 and 11). A fixture holds
//! the files that the released OpenSpec 1.15.0 wrote with `openspec init --tools claude,codex`
//! (`tests/fixtures/openspec-coexistence/`); no test runs OpenSpec. Each case runs the compiled binary
//! against a scratch git repository.
//!
//! A line of the spec is the literal of a test; `the_spec_lines_of_these_tests_are_in_the_spec` reads
//! the spec file and shows that each literal is in it.
//!
//! Order in each scratch install: the out-of-date lines run after the files are tracked and before
//! `houserules render`; the passing gates run after `render`.

mod tracker_support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;
use tempfile::TempDir;
use tracker_support::{
    OPENSPEC_SECTION as SECTION, assert_output, commit_all, copy_dir, git, repo_root, run, run_ok,
    scratch_git_repo, sha256_hex, stderr, stdout,
};

/// The version of the binary under test, which the warning of spec 6.2 names.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The file of the spec of batch 38, below `docs/specs/`.
const SPEC_FILE: &str = "2026-10-25-batch-38-openspec-and-release.md";

/// The warning of spec 6.2, with the version of the binary as `<version>`.
const WARNING_TEMPLATE: &str = "houserules: warning: this install runs the reference workflow and the project plans with OpenSpec (openspec/); plan changes with one of them. Beside OpenSpec, houserules needs no reference workflow: https://github.com/jblossey/houserules/blob/v<version>/docs/openspec.md";

/// The finding of `check-knowledge` for an `AGENTS.md` that `render` has not written yet (spec 6.1).
const OUT_OF_DATE: &str = "AGENTS.md: generated file is out of date (run houserules render)";

/// The line of `render --check` for the same file (spec 6.1).
const WOULD_CHANGE: &str = "AGENTS.md: would change";

/// The `init` arguments that install the reference workflow with everything it requires.
const INIT_WORKFLOW: [&str; 6] = [
    "init",
    "--workflow",
    "reference",
    "--tracker",
    "json",
    "--add-required-packs",
];

/// The warning of spec 6.2 as the binary under test prints it.
fn warning() -> String {
    WARNING_TEMPLATE.replace("<version>", VERSION)
}

/// The lines that lie in the spec as the tests write them.
#[test]
fn the_spec_lines_of_these_tests_are_in_the_spec() {
    let spec = fs::read_to_string(repo_root().join("docs").join("specs").join(SPEC_FILE))
        .expect("read the spec");
    assert!(
        spec.contains(&format!("```markdown\n{SECTION}\n```")),
        "the spec does not hold the section of 6.1 as the test does"
    );
    assert!(
        spec.contains(&format!("`{WARNING_TEMPLATE}`")),
        "the spec does not hold the warning of 6.2 as the test does"
    );
    for line in [OUT_OF_DATE, WOULD_CHANGE] {
        assert!(
            spec.contains(line),
            "the spec does not hold this line as the test does: {line}"
        );
    }
}

// ---- the fixture ----------------------------------------------------------------------

/// The folder of the fixture.
fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("openspec-coexistence")
}

/// The files of the fixture, by their path below `files/` (`/`-separated), each with its bytes.
fn fixture_files() -> BTreeMap<String, Vec<u8>> {
    fn collect(dir: &Path, prefix: &str, found: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(dir).expect("read a fixture folder") {
            let entry = entry.expect("a fixture entry");
            let name = entry.file_name().into_string().expect("a UTF-8 name");
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().expect("a file type").is_dir() {
                collect(&entry.path(), &path, found);
            } else {
                let bytes = fs::read(entry.path()).expect("read a fixture file");
                found.insert(path, bytes);
            }
        }
    }
    let mut found = BTreeMap::new();
    collect(&fixture_root().join("files"), "", &mut found);
    found
}

/// The digest of each file that `SHA256SUMS` lists, by path.
fn listed_digests() -> BTreeMap<String, String> {
    let text = fs::read_to_string(fixture_root().join("SHA256SUMS")).expect("read SHA256SUMS");
    text.lines()
        .map(|line| {
            let (digest, path) = line.split_once("  ").expect("a digest and a path");
            (path.to_string(), digest.to_string())
        })
        .collect()
}

/// The vendored files are the files that the OpenSpec run wrote: the same paths, and each file
/// hashes to the digest that `SHA256SUMS` lists.
#[test]
fn the_vendored_files_match_their_checksums() {
    let files = fixture_files();
    let listed = listed_digests();

    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        listed.keys().collect::<Vec<_>>(),
        "the fixture holds the files that SHA256SUMS lists, and no other"
    );
    for (path, bytes) in &files {
        assert_eq!(&sha256_hex(bytes), &listed[path], "{path}");
    }
    let source = fs::read_to_string(fixture_root().join("SOURCE.txt")).expect("read SOURCE.txt");
    assert!(source.contains("tag v1.15.0"), "{source}");
    let notice =
        fs::read_to_string(fixture_root().join("LICENSE.openspec")).expect("read the MIT notice");
    assert!(notice.starts_with("MIT License"), "{notice}");
}

// ---- scratch installs -----------------------------------------------------------------

/// Writes the fixture files below `dir`.
fn add_openspec_files(dir: &Path) {
    copy_dir(&fixture_root().join("files"), dir);
}

/// A scratch repository with a plain `houserules init`, committed.
fn plain_install() -> TempDir {
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init"]);
    commit_all(repo.path(), "chore: houserules init");
    repo
}

/// A scratch repository that holds `files` (a path and its text each), staged, with the reference
/// workflow installed by `init`, committed. Returns the repository and the output of `init`.
fn workflow_install_over(files: &[(&str, &str)]) -> (TempDir, Output) {
    let repo = scratch_git_repo();
    for (path, text) in files {
        let file = path
            .split('/')
            .fold(repo.path().to_path_buf(), |dir, part| dir.join(part));
        fs::create_dir_all(file.parent().expect("a parent")).expect("create the folder");
        fs::write(file, text).expect("write a file");
    }
    git(repo.path(), &["add", "-A"]);
    let output = run(repo.path(), &INIT_WORKFLOW);
    commit_all(repo.path(), "chore: houserules init");
    (repo, output)
}

/// A scratch repository with the fixture files and the reference workflow, committed. Returns the
/// repository and the output of `init`.
fn workflow_install_with_openspec() -> (TempDir, Output) {
    let repo = scratch_git_repo();
    add_openspec_files(repo.path());
    git(repo.path(), &["add", "-A"]);
    let output = run(repo.path(), &INIT_WORKFLOW);
    commit_all(repo.path(), "chore: houserules init");
    (repo, output)
}

/// The text of `AGENTS.md` in `dir`.
fn agents_md(dir: &Path) -> String {
    fs::read_to_string(dir.join("AGENTS.md")).expect("read AGENTS.md")
}

/// The lines of the stderr of `output`.
fn stderr_lines(output: &Output) -> Vec<String> {
    stderr(output).lines().map(str::to_string).collect()
}

// ---- case 10: coexistence -------------------------------------------------------------

/// Spec case 10: an install that holds the OpenSpec files passes `check-knowledge`, `render
/// --check`, and `audit`, and `init --target claude` and `update` leave each of those files
/// byte-identical. The gates run after `render` has written the section of 6.1.
#[test]
fn an_install_with_the_openspec_files_passes_the_gates_and_keeps_them_byte_identical() {
    let repo = plain_install();
    let base = git(repo.path(), &["rev-parse", "HEAD"]).trim().to_string();
    add_openspec_files(repo.path());
    commit_all(repo.path(), "chore: add openspec");
    run_ok(repo.path(), &["render"]);
    commit_all(repo.path(), "chore: render");

    let gates = |when: &str| {
        assert_output(
            &run(repo.path(), &["check-knowledge"]),
            0,
            "knowledge: ok\n",
            "",
        );
        assert_output(
            &run(repo.path(), &["render", "--check"]),
            0,
            "render: up to date\n",
            "",
        );
        assert!(
            agents_md(repo.path()).contains(SECTION),
            "AGENTS.md holds the section of 6.1 {when}"
        );
    };
    let identical = |when: &str| {
        for (path, bytes) in fixture_files() {
            let file = path
                .split('/')
                .fold(repo.path().to_path_buf(), |dir, part| dir.join(part));
            assert_eq!(
                fs::read(&file).unwrap_or_else(|error| panic!("read {path} {when}: {error}")),
                bytes,
                "{path} {when}"
            );
        }
    };
    gates("after the files are added");
    identical("after the files are added");

    let audit = run(repo.path(), &["audit", "--base", &base, "--head", "HEAD"]);
    assert_eq!(
        audit.status.code(),
        Some(0),
        "audit failed: {}",
        stderr(&audit)
    );
    let result: Value = serde_json::from_str(&stdout(&audit)).expect("the audit result is JSON");
    let changed: Vec<&str> = result["changed_files"]
        .as_array()
        .expect("changed_files")
        .iter()
        .map(|path| path.as_str().expect("a path"))
        .collect();
    for path in fixture_files().keys() {
        assert!(changed.contains(&path.as_str()), "{path} is in the range");
    }
    let rows = result["rules"].as_array().expect("rules");
    assert!(
        rows.iter().all(|row| row["result"] != "fail"),
        "no row fails: {rows:?}"
    );

    run_ok(repo.path(), &["init", "--target", "claude"]);
    commit_all(repo.path(), "chore: houserules claude target");
    gates("after init --target claude");
    identical("after init --target claude");

    run_ok(repo.path(), &["update"]);
    gates("after update");
    identical("after update");
}

// ---- case 11: the split in AGENTS.md --------------------------------------------------

/// Spec 6.1 and case 11: with the OpenSpec files tracked, `render` writes the section directly after
/// the standing rules, byte for byte.
#[test]
fn render_writes_the_openspec_section_after_the_standing_rules() {
    let repo = plain_install();
    add_openspec_files(repo.path());
    commit_all(repo.path(), "chore: add openspec");

    assert_output(
        &run(repo.path(), &["render"]),
        0,
        "AGENTS.md: written\n",
        "",
    );

    let block = agents_md(repo.path());
    assert!(
        block.contains(&format!(
            "## Standing rules\n\nNo standing rules.\n\n{SECTION}\n\n## Area rules\n"
        )),
        "{block}"
    );
    assert_output(
        &run(repo.path(), &["render"]),
        0,
        "render: up to date\n",
        "",
    );
}

/// Spec 6.1: without the OpenSpec files, no section.
#[test]
fn without_the_openspec_files_render_writes_no_section() {
    let repo = plain_install();

    assert_output(
        &run(repo.path(), &["render"]),
        0,
        "render: up to date\n",
        "",
    );

    assert!(!agents_md(repo.path()).contains("## OpenSpec"));
}

/// Spec 6.1: the section reads the tracked paths, so files that the work tree holds and git does not
/// track draw none.
#[test]
fn untracked_openspec_files_draw_no_section() {
    let repo = plain_install();
    add_openspec_files(repo.path());

    assert_output(
        &run(repo.path(), &["render"]),
        0,
        "render: up to date\n",
        "",
    );

    assert!(!agents_md(repo.path()).contains("## OpenSpec"));
}

/// Spec 6.1, accepted red: after the first tracked file of an OpenSpec root, `check-knowledge` and
/// `render --check` report the file until the project runs `render`.
#[test]
fn the_gates_report_agents_md_until_render_writes_the_section() {
    let repo = plain_install();
    add_openspec_files(repo.path());
    commit_all(repo.path(), "chore: add openspec");

    assert_output(
        &run(repo.path(), &["check-knowledge"]),
        1,
        "",
        &format!("{OUT_OF_DATE}\n"),
    );
    assert_output(
        &run(repo.path(), &["render", "--check"]),
        1,
        "",
        &format!("{WOULD_CHANGE}\n"),
    );

    assert_output(
        &run(repo.path(), &["render"]),
        0,
        "AGENTS.md: written\n",
        "",
    );

    assert_output(
        &run(repo.path(), &["check-knowledge"]),
        0,
        "knowledge: ok\n",
        "",
    );
    assert_output(
        &run(repo.path(), &["render", "--check"]),
        0,
        "render: up to date\n",
        "",
    );
}

// ---- case 11: the warning of two planning workflows -----------------------------------

/// Spec 6.2 and case 11: `init --workflow reference` in a tree that holds an OpenSpec root with a
/// planning shape prints the warning with the version of the binary on stderr and keeps exit 0.
#[test]
fn init_workflow_reference_warns_when_the_tree_holds_an_openspec_root() {
    let (_repo, output) = workflow_install_with_openspec();

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), format!("{}\n", warning()));
}

/// Spec 6.2 and case 11: `check-knowledge` prints the warning on stderr and keeps its exit code, 0
/// when the gate passes and 1 when it reports a finding.
#[test]
fn check_knowledge_warns_and_keeps_its_exit_code() {
    let (repo, _) = workflow_install_with_openspec();

    let passing = run(repo.path(), &["check-knowledge"]);
    assert_eq!(passing.status.code(), Some(0), "{}", stderr(&passing));
    assert!(stdout(&passing).ends_with("knowledge: ok\n"));
    assert_eq!(stderr(&passing), format!("{}\n", warning()));

    let without_section = agents_md(repo.path()).replace(&format!("{SECTION}\n\n"), "");
    fs::write(repo.path().join("AGENTS.md"), without_section).expect("write AGENTS.md");
    let failing = run(repo.path(), &["check-knowledge"]);
    assert_eq!(failing.status.code(), Some(1), "{}", stderr(&failing));
    let lines = stderr_lines(&failing);
    assert!(lines.contains(&OUT_OF_DATE.to_string()), "{lines:?}");
    assert!(lines.contains(&warning()), "{lines:?}");
    assert_eq!(lines.len(), 2, "{lines:?}");
}

/// Spec 6.2 and 275: a store-pointer root draws the warning and gets no section.
#[test]
fn a_store_pointer_root_draws_the_warning_and_no_section() {
    let (repo, init) = workflow_install_over(&[("openspec/config.yaml", "store: team\n")]);

    assert_eq!(init.status.code(), Some(0), "{}", stderr(&init));
    assert_eq!(stderr(&init), format!("{}\n", warning()));
    assert!(!agents_md(repo.path()).contains("## OpenSpec"));
    let checked = run(repo.path(), &["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(0), "{}", stderr(&checked));
    assert_eq!(stderr(&checked), format!("{}\n", warning()));
}

/// Spec 6.2 and 275: a config with bytes that are no UTF-8 is read lossily, as the adapter reads it,
/// so its `store:` key makes a store pointer: the warning and no section, and the gates pass.
#[test]
fn a_config_that_is_no_utf8_still_points_at_a_store() {
    let repo = scratch_git_repo();
    fs::create_dir(repo.path().join("openspec")).expect("create openspec/");
    fs::write(
        repo.path().join("openspec").join("config.yaml"),
        b"store: \xff\xfe\n",
    )
    .expect("write the config");
    git(repo.path(), &["add", "-A"]);

    let init = run(repo.path(), &INIT_WORKFLOW);

    assert_eq!(init.status.code(), Some(0), "{}", stderr(&init));
    assert_eq!(stderr(&init), format!("{}\n", warning()));
    assert!(!agents_md(repo.path()).contains("## OpenSpec"));
    commit_all(repo.path(), "chore: houserules init");
    let checked = run(repo.path(), &["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(0), "{}", stderr(&checked));
    assert_eq!(stderr(&checked), format!("{}\n", warning()));
    assert_output(
        &run(repo.path(), &["render", "--check"]),
        0,
        "render: up to date\n",
        "",
    );
}

/// Spec 6.2: `check-knowledge` prints the warning in every state past the root's resolution, so a
/// knowledge base that does not load keeps its exit 2 and its one named line, and the warning
/// follows it.
#[test]
fn check_knowledge_warns_when_the_knowledge_base_does_not_load() {
    let (repo, _) = workflow_install_with_openspec();
    fs::write(
        repo.path().join("knowledge").join("process.json"),
        "{not json",
    )
    .expect("break a knowledge file");

    let failing = run(repo.path(), &["check-knowledge"]);

    assert_eq!(failing.status.code(), Some(2), "{}", stderr(&failing));
    assert_eq!(stdout(&failing), "");
    let lines = stderr_lines(&failing);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0].contains("process.json: invalid JSON"), "{lines:?}");
    assert_eq!(lines[1], warning());
}

/// Spec 6.2: no warning without the reference workflow, none without an OpenSpec root, and none for
/// OpenSpec files that git does not track.
#[test]
fn no_warning_without_both_the_workflow_and_a_tracked_openspec_root() {
    let without_workflow = plain_install();
    add_openspec_files(without_workflow.path());
    commit_all(without_workflow.path(), "chore: add openspec");
    run_ok(without_workflow.path(), &["render"]);
    let checked = run(without_workflow.path(), &["check-knowledge"]);
    assert_output(&checked, 0, "knowledge: ok\n", "");

    let (without_openspec, init) = workflow_install_over(&[]);
    assert_eq!(init.status.code(), Some(0), "{}", stderr(&init));
    assert_eq!(stderr(&init), "");
    let checked = run(without_openspec.path(), &["check-knowledge"]);
    assert_eq!(checked.status.code(), Some(0), "{}", stderr(&checked));
    assert_eq!(stderr(&checked), "");

    add_openspec_files(without_openspec.path());
    let untracked = run(without_openspec.path(), &["check-knowledge"]);
    assert_eq!(untracked.status.code(), Some(0), "{}", stderr(&untracked));
    assert_eq!(stderr(&untracked), "");
}
