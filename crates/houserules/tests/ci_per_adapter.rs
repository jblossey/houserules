//! The seeded CI workflow per adapter (spec 4.8): the content that every command
//! setting `tracker`, and `update`, write for JSON, GitHub, Beads, and no
//! tracker, in the form at the git top level and in the form of a
//! subdirectory install. Each file passes `actionlint`, and a file that the
//! project changed gets the hand step that names the lines to add.
//!
//! `actionlint` comes from `mise.toml` (`mise install`); a test fails, and does
//! not skip, when it is missing. A Beads install here is a marker that names
//! `beads`, because `update` writes the workflow whether or not `bd` can be
//! reached; `HOUSERULES_BD` names a program that does not exist, so that no real
//! `bd` runs.

mod tracker_support;

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::json;

#[cfg(unix)]
use tracker_support::hermetic_bd;
use tracker_support::{
    commit_all, edit_marker, git, houserules, read_json, repo_root, run_ok, scratch_git_repo,
    stderr, stdout,
};

/// The workflow of an install at the git top level.
const WORKFLOW: &str = ".github/workflows/knowledge.yml";

/// The unreachable line that `update` prints for a Beads install when `bd` is
/// not installed (spec 3.4, C38).
const BD_MISSING: &str = "tracker beads cannot be reached: bd is not installed; install bd";

/// The text of `mise.toml` of this repository.
fn mise_toml() -> String {
    fs::read_to_string(repo_root().join("mise.toml")).expect("read mise.toml")
}

/// The value of the line of `mise.toml` that starts with `key`, once the quotes
/// are off.
fn mise_pin(key: &str) -> String {
    let toml = mise_toml();
    let line = toml
        .lines()
        .find_map(|line| line.strip_prefix(key)?.strip_prefix(" = \""))
        .unwrap_or_else(|| panic!("mise.toml pins {key}"));
    line.trim_end_matches('"').to_string()
}

/// Runs `actionlint` on `file` and requires a clean exit. A missing tool fails
/// the test and names the step that installs it.
fn assert_actionlint_passes(file: &Path) {
    let output = Command::new("actionlint")
        .arg(file)
        .output()
        .unwrap_or_else(|error| {
            panic!(
                "actionlint is not installed ({error}): run mise install, which pins it in mise.toml"
            )
        });
    assert!(
        output.status.success(),
        "actionlint rejects {}:\n{}{}",
        file.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A scratch repository with a GitHub `origin`, so that `init` writes the CI
/// workflow.
fn repository() -> tempfile::TempDir {
    let dir = scratch_git_repo();
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/widgets.git",
        ],
    );
    dir
}

/// Reads `relative` below `dir` as text.
fn text(dir: &Path, relative: &str) -> String {
    fs::read_to_string(
        relative
            .split('/')
            .fold(dir.to_path_buf(), |p, c| p.join(c)),
    )
    .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The CI workflow of the template, as the kit ships it for JSON and for no
/// tracker.
fn template_workflow() -> String {
    fs::read_to_string(repo_root().join("template").join(WORKFLOW)).expect("read the template")
}

/// Runs `update --dir <dir>` with a `bd` that does not exist, and returns the
/// output.
fn update_without_bd(dir: &Path) -> std::process::Output {
    houserules()
        .args(["update", "--dir"])
        .arg(dir)
        .env("HOUSERULES_BD", dir.join("no-such-bd"))
        .output()
        .expect("run houserules update")
}

/// An install on the Beads tracker: the marker names `beads`, and `update`
/// writes the workflow of the adapter.
fn beads_install(dir: &Path, init_args: &[&str]) {
    run_ok(dir, init_args);
    edit_marker(dir, |marker| {
        marker.insert("tracker".to_string(), json!({"adapter": "beads"}));
    });
    let output = update_without_bd(dir);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), format!("{BD_MISSING}\n"));
}

/// The step blocks of a workflow text: each starts at a line that opens a list
/// item of the steps (six spaces and a dash).
fn steps(workflow: &str) -> Vec<String> {
    let mut blocks: Vec<String> = Vec::new();
    for line in workflow.lines() {
        if line.starts_with("      - ") {
            blocks.push(String::new());
        }
        if let Some(block) = blocks.last_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    blocks
}

/// The step blocks of `workflow` that run `command`.
fn steps_that_run(workflow: &str, command: &str) -> Vec<String> {
    steps(workflow)
        .into_iter()
        .filter(|step| step.contains(command))
        .collect()
}

/// The env that a step of the GitHub form gets when it runs a gate: `gh` takes
/// its token from `GH_TOKEN`.
const GITHUB_TOKEN: &str = "        env:\n          GH_TOKEN: ${{ github.token }}\n";

/// Spec 3.3 and 4.8 (`mise.toml`): `actionlint` is pinned to an exact release,
/// and the Beads release that the seeded workflow installs is the one that
/// `mise.toml` pins.
#[test]
fn mise_toml_pins_actionlint_to_an_exact_release() {
    let pin = mise_pin("actionlint");

    assert!(
        pin.split('.').count() == 3 && pin.split('.').all(|part| part.parse::<u32>().is_ok()),
        "actionlint is pinned to {pin:?}, not to an exact release"
    );
}

/// Spec 4.8: with no tracker and with the JSON tracker the workflow keeps the
/// steps of the template, byte for byte, and passes `actionlint`.
#[test]
fn no_tracker_and_json_keep_the_template_and_pass_actionlint() {
    for args in [vec!["init"], vec!["init", "--tracker", "json"]] {
        let dir = repository();
        run_ok(dir.path(), &args);

        assert_eq!(text(dir.path(), WORKFLOW), template_workflow(), "{args:?}");
        assert_actionlint_passes(&dir.path().join(WORKFLOW));
    }
}

/// Spec 4.8: GitHub adds the job-level permissions and, to the steps that run
/// `check-backlog` and `audit`, the token env, and nothing else.
#[test]
fn github_adds_job_permissions_and_the_token_to_the_two_gate_steps() {
    let dir = repository();
    run_ok(dir.path(), &["init", "--tracker", "github"]);

    let workflow = text(dir.path(), WORKFLOW);

    assert_eq!(
        workflow
            .matches("    permissions:\n      contents: read\n      issues: read\n")
            .count(),
        1,
        "{workflow}"
    );
    for command in ["houserules check-backlog", "houserules audit"] {
        let found = steps_that_run(&workflow, command);
        assert_eq!(found.len(), 1, "{command}");
        assert!(found[0].contains(GITHUB_TOKEN), "{}", found[0]);
    }
    assert_eq!(workflow.matches("GH_TOKEN").count(), 2, "{workflow}");
    assert_actionlint_passes(&dir.path().join(WORKFLOW));
}

/// The number of lines of `output` that equal `wrote .github/workflows/knowledge.yml`.
fn workflow_wrote_lines(output: &str) -> usize {
    output
        .lines()
        .filter(|line| *line == format!("wrote {WORKFLOW}"))
        .count()
}

/// Spec 4.8: one command prints one `wrote` line for each file that it writes.
/// `init --tracker github` builds the workflow in the form of the adapter
/// before its first write, so the file is written once, and its baseline holds
/// that form. A second run writes nothing.
#[test]
fn init_with_the_github_tracker_writes_the_ci_workflow_once_in_the_form_of_the_adapter() {
    let dir = repository();

    let first = run_ok(dir.path(), &["init", "--tracker", "github"]);

    assert_eq!(
        workflow_wrote_lines(&stdout(&first)),
        1,
        "{}",
        stdout(&first)
    );
    let workflow = text(dir.path(), WORKFLOW);
    assert_eq!(workflow.matches(GITHUB_TOKEN).count(), 2, "{workflow}");
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["baselines"][format!(":/{WORKFLOW}")],
        json!(sha256_hex(workflow.as_bytes()))
    );
    let second = run_ok(dir.path(), &["init", "--tracker", "github"]);
    assert_eq!(
        workflow_wrote_lines(&stdout(&second)),
        0,
        "{}",
        stdout(&second)
    );
}

/// Spec 4.8: the same for `init --tracker beads`. A stand-in for `bd` names the
/// issue prefix.
#[cfg(unix)]
#[test]
fn init_with_the_beads_tracker_writes_the_ci_workflow_once_in_the_form_of_the_adapter() {
    use std::os::unix::fs::PermissionsExt;
    let dir = repository();
    let bd = dir.path().join(".git").join("stand-in-bd");
    fs::write(
        &bd,
        "#!/bin/sh\necho '{\"key\":\"issue_prefix\",\"value\":\"neo\"}'\n",
    )
    .expect("write the stand-in for bd");
    fs::set_permissions(&bd, fs::Permissions::from_mode(0o755)).expect("make it executable");
    let mut command = houserules();
    command
        .args(["init", "--tracker", "beads", "--dir"])
        .arg(dir.path())
        .env("HOUSERULES_BD", &bd);
    hermetic_bd(&mut command, &dir.path().join(".git/bd-home"));

    let output = command.output().expect("run houserules init");

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        workflow_wrote_lines(&stdout(&output)),
        1,
        "{}",
        stdout(&output)
    );
    let workflow = text(dir.path(), WORKFLOW);
    assert_eq!(workflow.matches("run: bd bootstrap --yes").count(), 1);
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["baselines"][format!(":/{WORKFLOW}")],
        json!(sha256_hex(workflow.as_bytes()))
    );
}

/// Spec 4.8 and ruling 188: Beads adds a step that installs `bd` at the pinned
/// release and verifies its checksum, and a step that runs `bd bootstrap` with
/// the usage metrics off, both before the gates. The workflow passes
/// `actionlint`.
#[test]
fn beads_installs_bd_at_the_pinned_release_and_bootstraps_the_store() {
    let dir = repository();
    beads_install(dir.path(), &["init"]);

    let workflow = text(dir.path(), WORKFLOW);

    let version = mise_pin("\"github:gastownhall/beads\"");
    let install = steps_that_run(&workflow, "checksums.txt");
    assert_eq!(install.len(), 1, "{workflow}");
    assert!(
        install[0].contains(&format!("beads_{version}_linux_amd64.tar.gz")),
        "{}",
        install[0]
    );
    assert!(install[0].contains("sha256sum"), "{}", install[0]);
    let bootstrap = steps_that_run(&workflow, "bd bootstrap");
    assert_eq!(bootstrap.len(), 1, "{workflow}");
    assert!(
        bootstrap[0].contains("run: bd bootstrap --yes\n"),
        "{}",
        bootstrap[0]
    );
    assert!(
        bootstrap[0].contains("BD_DISABLE_METRICS: '1'")
            && bootstrap[0].contains("DO_NOT_TRACK: '1'"),
        "{}",
        bootstrap[0]
    );
    let at = |needle: &str| workflow.find(needle).unwrap_or_else(|| panic!("{needle}"));
    assert!(at("checksums.txt") < at("bd bootstrap --yes"), "{workflow}");
    assert!(
        at("bd bootstrap --yes") < at("run: houserules check-backlog"),
        "{workflow}"
    );
    assert!(!workflow.contains("GH_TOKEN"), "{workflow}");
    assert_actionlint_passes(&dir.path().join(WORKFLOW));
}

/// Spec 4.8: a subdirectory install gets the lines of its adapter in its own
/// file, `knowledge-<folder>.yml`, with the lines of the subdirectory form, and
/// the file passes `actionlint`.
#[test]
fn a_subdirectory_install_gets_the_lines_of_its_adapter() {
    let github = repository();
    fs::create_dir(github.path().join("sub")).expect("create sub/");
    run_ok(&github.path().join("sub"), &["init", "--tracker", "github"]);
    let beads = repository();
    fs::create_dir(beads.path().join("sub")).expect("create sub/");
    beads_install(&beads.path().join("sub"), &["init"]);

    let file = ".github/workflows/knowledge-sub.yml";
    let github_text = text(github.path(), file);
    let beads_text = text(beads.path(), file);

    assert!(
        github_text.contains("      issues: read\n"),
        "{github_text}"
    );
    assert_eq!(github_text.matches(GITHUB_TOKEN).count(), 2);
    assert!(
        beads_text.contains("run: bd bootstrap --yes\n"),
        "{beads_text}"
    );
    for workflow in [&github_text, &beads_text] {
        assert!(
            workflow.contains("        working-directory: 'sub'\n"),
            "{workflow}"
        );
    }
    assert_actionlint_passes(&github.path().join(file));
    assert_actionlint_passes(&beads.path().join(file));
}

/// Spec 4.8: `update` writes the workflow for the adapter that the marker names,
/// as it keeps a D3 file: a workflow at its baseline is rewritten and
/// baselined, and a second `update` prints nothing about it.
#[test]
fn update_moves_a_workflow_at_its_baseline_to_the_content_of_the_adapter() {
    let dir = repository();
    run_ok(dir.path(), &["init"]);
    edit_marker(dir.path(), |marker| {
        marker.insert(
            "tracker".to_string(),
            json!({"adapter": "github", "repo": "acme/widgets"}),
        );
    });
    commit_all(dir.path(), "github");

    let first = run_ok(dir.path(), &["update"]);

    assert!(
        stdout(&first)
            .lines()
            .any(|line| line == format!("wrote {WORKFLOW}")),
        "{}",
        stdout(&first)
    );
    let workflow = text(dir.path(), WORKFLOW);
    assert_eq!(workflow.matches(GITHUB_TOKEN).count(), 2, "{workflow}");
    let marker = read_json(dir.path(), ".houserules.json");
    assert_eq!(
        marker["baselines"][format!(":/{WORKFLOW}")],
        json!(sha256_hex(workflow.as_bytes()))
    );
    commit_all(dir.path(), "update");
    let second = run_ok(dir.path(), &["update"]);
    assert!(!stdout(&second).contains(WORKFLOW), "{}", stdout(&second));
}

/// The lowercase hex SHA-256 digest of `content`.
fn sha256_hex(content: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Spec 4.8: a workflow that the project changed stays, and `update` names the
/// lines to add when the file lacks the ones that the adapter needs. A file
/// that holds them already gets the kept line and no hand step. With JSON, the
/// kept line stands alone.
#[test]
fn a_changed_workflow_gets_the_hand_step_that_names_the_lines_to_add() {
    let github_step = format!(
        "hand step: {WORKFLOW} lacks the lines that the github tracker needs; add permissions: {{contents: read, issues: read}} to the gates job, and env: {{GH_TOKEN: ${{{{ github.token }}}}}} to each step that runs houserules check-backlog or houserules audit"
    );
    let bd = mise_pin("\"github:gastownhall/beads\"");
    let beads_step = format!(
        "hand step: {WORKFLOW} lacks the steps that the beads tracker needs; add a step that installs bd {bd} from its GitHub release, and a step that runs bd bootstrap --yes, before the step that runs houserules check-backlog"
    );
    let kept = format!("kept {WORKFLOW} (locally modified)");

    for (adapter, step) in [
        ("json", None),
        ("github", Some(github_step)),
        ("beads", Some(beads_step)),
    ] {
        let dir = repository();
        run_ok(dir.path(), &["init"]);
        let mut changed = text(dir.path(), WORKFLOW);
        changed.push_str("# our own step\n");
        fs::write(dir.path().join(WORKFLOW), &changed).expect("change the workflow");
        edit_marker(dir.path(), |marker| {
            let tracker = match adapter {
                "github" => json!({"adapter": "github", "repo": "acme/widgets"}),
                other => json!({"adapter": other}),
            };
            marker.insert("tracker".to_string(), tracker);
        });
        commit_all(dir.path(), "our step");

        for run_number in 1..=2 {
            let output = update_without_bd(dir.path());
            assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
            let lines: Vec<String> = stdout(&output).lines().map(str::to_string).collect();
            assert!(
                lines.contains(&kept),
                "{adapter} run {run_number}: {lines:?}"
            );
            let steps: Vec<&String> = lines
                .iter()
                .filter(|line| line.starts_with("hand step: "))
                .collect();
            assert_eq!(
                steps,
                step.iter().collect::<Vec<_>>(),
                "{adapter} run {run_number}"
            );
        }
        assert_eq!(text(dir.path(), WORKFLOW), changed, "{adapter}");

        let mut complete = changed.clone();
        complete.push_str(
            "# permissions: issues: read, env: GH_TOKEN: ${{ github.token }}, bd bootstrap\n",
        );
        fs::write(dir.path().join(WORKFLOW), &complete).expect("add the lines");
        let output = update_without_bd(dir.path());
        assert!(
            !stdout(&output).contains("hand step: "),
            "{adapter}: {}",
            stdout(&output)
        );
    }
}
