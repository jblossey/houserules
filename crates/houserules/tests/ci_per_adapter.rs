//! The seeded CI workflow per adapter (spec 4.8). `update`, and every command
//! that sets `tracker`, keep an existing seeded workflow at the content of its
//! adapter: JSON, GitHub, Beads, or no tracker. The content has one form at the
//! git top level and one form for a subdirectory install. `init` seeds no
//! workflow (spec 10.7), so each case starts from the workflow that an earlier
//! release seeded (`seed_workflow`). Each file passes `actionlint`. A file that
//! the project changed gets the hand step that names the lines to add.
//!
//! `actionlint` comes from `mise.toml` (`mise install`); a test fails, and does
//! not skip, when it is missing. A Beads install here is a marker that names
//! `beads`, because `update` writes the workflow whether or not `bd` can be
//! reached; `HOUSERULES_BD` names a program that does not exist, so that no real
//! `bd` runs.

mod tracker_support;

use std::fs;
use std::path::Path;

use serde_json::json;

#[cfg(unix)]
use tracker_support::hermetic_bd;
use tracker_support::{
    assert_actionlint_passes, commit_all, edit_marker, git, houserules, read_json, repo_root,
    run_ok, scratch_git_repo, seed_workflow, sha256_hex, stderr, stdout,
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

/// A scratch repository with a GitHub `origin`, where the GitHub tracker reads
/// its repository.
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

/// A scratch repository with an install that an earlier release seeded with
/// the CI workflow, and then the command `after`, when there is one.
fn seeded_install(after: &[&str]) -> tempfile::TempDir {
    let dir = repository();
    run_ok(dir.path(), &["init"]);
    seed_workflow(dir.path(), dir.path());
    if !after.is_empty() {
        run_ok(dir.path(), after);
    }
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

/// An install on the Beads tracker, in `root` below the repository at `top`:
/// the workflow was seeded earlier, the marker names `beads`, and `update`
/// brings the workflow to the form of the adapter.
fn beads_install(top: &Path, root: &Path, init_args: &[&str]) {
    run_ok(root, init_args);
    seed_workflow(top, root);
    edit_marker(root, |marker| {
        marker.insert("tracker".to_string(), json!({"adapter": "beads"}));
    });
    let output = update_without_bd(root);
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
    for args in [vec!["update"], vec!["init", "--tracker", "json"]] {
        let dir = seeded_install(&args);

        assert_eq!(text(dir.path(), WORKFLOW), template_workflow(), "{args:?}");
        assert_actionlint_passes(&dir.path().join(WORKFLOW));
    }
}

/// Spec 4.8: GitHub adds the job-level permissions and, to the steps that run
/// `check-backlog` and `audit`, the token env, and nothing else.
#[test]
fn github_adds_job_permissions_and_the_token_to_the_two_gate_steps() {
    let dir = seeded_install(&["init", "--tracker", "github"]);

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
/// `init --tracker github` brings the seeded workflow to the form of the
/// adapter in one write, and its baseline holds that form. A second run writes
/// nothing.
#[test]
fn init_with_the_github_tracker_brings_the_seeded_workflow_to_the_form_of_the_adapter_once() {
    let dir = seeded_install(&[]);

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
fn init_with_the_beads_tracker_brings_the_seeded_workflow_to_the_form_of_the_adapter_once() {
    use std::os::unix::fs::PermissionsExt;
    let dir = seeded_install(&[]);
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
    beads_install(dir.path(), dir.path(), &["init"]);

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
    let github_root = github.path().join("sub");
    fs::create_dir(&github_root).expect("create sub/");
    run_ok(&github_root, &["init"]);
    seed_workflow(github.path(), &github_root);
    run_ok(&github_root, &["init", "--tracker", "github"]);
    let beads = repository();
    let beads_root = beads.path().join("sub");
    fs::create_dir(&beads_root).expect("create sub/");
    beads_install(beads.path(), &beads_root, &["init"]);

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
    let dir = seeded_install(&[]);
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

/// The workflow `text`, the seeded form of no tracker, with the lines that the
/// tracker `adapter` needs added as YAML (`update` reads the copy as YAML, so a
/// comment adds nothing): `issues: read` and the token env of the two gate steps
/// for GitHub, and the steps that install `bd` and bootstrap the store, without
/// the input that drops the persisted credential, for Beads.
fn with_the_lines_of(adapter: &str, text: &str) -> String {
    match adapter {
        "github" => text
            .replace(
                "      contents: read\n",
                "      contents: read\n      issues: read\n",
            )
            .replace(
                "      - run: houserules check-backlog\n",
                &format!("      - run: houserules check-backlog\n{GITHUB_TOKEN}"),
            )
            .replace(
                "          --head ${{ github.event.pull_request.head.sha }}\n",
                &format!(
                    "          --head ${{{{ github.event.pull_request.head.sha }}}}\n{GITHUB_TOKEN}"
                ),
            ),
        "beads" => text
            .replace("          persist-credentials: false\n", "")
            .replace(
                "      - run: houserules check-backlog\n",
                "      - run: bd bootstrap --yes\n      - run: houserules check-backlog\n",
            ),
        _ => text.to_string(),
    }
}

/// Spec 4.8: a workflow that the project changed stays, and `update` names the
/// lines to add when the file lacks the ones that the adapter needs. A file
/// that holds them already gets the kept line and no hand step. With JSON, the
/// kept line stands alone. The copy is the seeded form of no tracker, which
/// holds `contents: read` and `persist-credentials: false`: the GitHub tracker
/// gets one step that adds `issues: read` to its block and one that adds the
/// `GH_TOKEN` env, and the Beads tracker asks for the removal of the input.
#[test]
fn a_changed_workflow_gets_the_hand_step_that_names_the_lines_to_add() {
    let github_steps = vec![
        format!(
            "hand step: {WORKFLOW} lacks issues: read, which the github tracker needs; add issues: read to the permissions of the gates job"
        ),
        format!(
            "hand step: {WORKFLOW} lacks the GH_TOKEN env that the github tracker needs; add env: {{GH_TOKEN: ${{{{ github.token }}}}}} to each step that runs houserules check-backlog or houserules audit"
        ),
    ];
    let bd = mise_pin("\"github:gastownhall/beads\"");
    let beads_step = format!(
        "hand step: {WORKFLOW} lacks the steps that the beads tracker needs; add a step that installs bd {bd} from its GitHub release, and a step that runs bd bootstrap --yes, before the step that runs houserules check-backlog"
    );
    let persist_step = format!(
        "hand step: {WORKFLOW} holds persist-credentials: false, which the beads tracker cannot use; remove it from with: of its actions/checkout step, so that bd bootstrap can fetch from the remote"
    );
    let kept = format!("kept {WORKFLOW} (locally modified)");

    for (adapter, steps) in [
        ("json", vec![]),
        ("github", github_steps),
        ("beads", vec![beads_step, persist_step]),
    ] {
        let dir = seeded_install(&[]);
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
            let printed: Vec<String> = lines
                .iter()
                .filter(|line| line.starts_with("hand step: "))
                .cloned()
                .collect();
            assert_eq!(printed, steps, "{adapter} run {run_number}");
        }
        assert_eq!(text(dir.path(), WORKFLOW), changed, "{adapter}");

        let complete = with_the_lines_of(adapter, &changed);
        fs::write(dir.path().join(WORKFLOW), &complete).expect("add the lines");
        let output = update_without_bd(dir.path());
        assert!(
            !stdout(&output).contains("hand step: "),
            "{adapter}: {}",
            stdout(&output)
        );
    }
}
