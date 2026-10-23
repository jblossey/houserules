//! `update` and `init` after the workspaces left git (spec 3.2, 7, 8.2, 8.4;
//! cases 2, 9, 10). `update` refuses a directory with no
//! install (HR-216), replaces the older ignore line `.houserules/step/`
//! (review focus 4), and delivers the PR-head checkout of the seeded CI
//! workflow with its hand step; `init` with no tracker flag seeds a lost
//! deliverables schema with the pattern of the installed tracker (HR-220).
//! Each case runs the compiled binary against a scratch install that
//! `houserules init` made.

mod locked_file;
mod tracker_support;
mod tree_snapshot;

use std::fs;
use std::path::Path;
use std::process::Output;

use serde_json::json;
use tempfile::TempDir;
use tracker_support::{
    assert_actionlint_passes, assert_output, commit_all, count_lines, edit_marker, git, houserules,
    quoted, read_json, repo_root, run, run_ok, scratch_git_repo, sha256_hex, stderr, stdout,
};
use tree_snapshot::{assert_same_tree, snapshot};

/// The seeded CI workflow of an install at the git top level.
const WORKFLOW: &str = ".github/workflows/knowledge.yml";

/// The `ref` input of the checkout that spec 3.2 adds (CI, 203).
const REF_INPUT: &str = "ref: ${{ github.event.pull_request.head.sha }}";

/// The refusal of `update` in a directory with no install (spec 8.2).
const NO_INSTALL: &str = "no houserules install here (no .houserules.json); run houserules init";

/// The line that `update` prints when it replaces the older ignore line.
const REPLACED_LINE: &str = "merged .gitignore (.houserules/step/ replaced by .houserules/)";

/// The unreachable line that `init` and `update` print for a Beads install
/// when `bd` is not installed (spec 3.4, C38).
const BD_MISSING: &str = "tracker beads cannot be reached: bd is not installed; install bd";

/// A scratch install that `houserules init` made.
fn install() -> TempDir {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    dir
}

/// A scratch repository with a GitHub `origin`, so that `init` writes the CI
/// workflow.
fn repository() -> TempDir {
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
    fs::read_to_string(dir.join(relative))
        .unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

// ---- HR-216: update with no install ----

/// The refusal of `update` for a target that is not the working directory: the
/// remedy names the target (spec 8.2, D9), quoted for a POSIX shell where it
/// needs quoting (spec 8.3): a scratch directory of Windows holds a backslash.
fn no_install_for(dir: &str) -> String {
    format!("{NO_INSTALL} --dir {}", quoted(Path::new(dir)))
}

/// Runs `houserules <args>` with `cwd` as the working directory and no
/// `--dir` of its own.
fn run_in(cwd: &Path, args: &[&str]) -> Output {
    houserules()
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run houserules")
}

/// Spec 8.2 and case 9: `update` in a directory with no `.houserules.json` and
/// no `knowledge/`, at or above it within the git top level, prints the line,
/// exits 2, and writes nothing. When the target is the working directory the
/// remedy is `run houserules init`; when it is not, the remedy names the target
/// as the user gave it (`--dir`).
#[test]
fn update_with_no_install_prints_one_line_and_writes_nothing() {
    let dir = scratch_git_repo();
    let top = dir.path();
    fs::create_dir(top.join("sub")).expect("create sub/");
    fs::write(top.join("README.md"), "readme\n").expect("write README.md");
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let top_text = top.to_str().expect("a UTF-8 path");
    let before = snapshot(top);

    for (cwd, args, expected) in [
        (top, vec!["update"], NO_INSTALL.to_string()),
        (
            top,
            vec!["update", "--id-prefix", "HR"],
            NO_INSTALL.to_string(),
        ),
        (top, vec!["update", "--dir", "."], NO_INSTALL.to_string()),
        (&top.join("sub"), vec!["update"], NO_INSTALL.to_string()),
        (top, vec!["update", "--dir", "sub"], no_install_for("sub")),
        (
            &top.join("sub"),
            vec!["update", "--dir", ".."],
            no_install_for(".."),
        ),
        (
            elsewhere.path(),
            vec!["update", "--dir", top_text],
            no_install_for(top_text),
        ),
    ] {
        let output = run_in(cwd, &args);

        assert_output(&output, 2, "", &format!("{expected}\n"));
        assert_same_tree(&before, &snapshot(top), &format!("{args:?}"));
    }
}

/// Spec 8.2: the remedy, followed as printed from the same working directory,
/// installs in the target that `update` looked at.
#[test]
fn the_remedy_of_update_with_no_install_installs_where_update_looked() {
    for (cwd_below_top, args) in [
        ("", vec!["update", "--dir", "sub"]),
        ("sub", vec!["update"]),
    ] {
        let dir = scratch_git_repo();
        fs::create_dir(dir.path().join("sub")).expect("create sub/");
        let cwd = dir.path().join(cwd_below_top);

        let refused = run_in(&cwd, &args);

        let line = stderr(&refused);
        let remedy = line
            .trim_end()
            .strip_prefix("no houserules install here (no .houserules.json); run houserules ")
            .unwrap_or_else(|| panic!("the line of the refusal: {line}"));
        let init_args: Vec<&str> = remedy.split(' ').collect();
        assert_eq!(init_args[0], "init", "{line}");
        let installed = run_in(&cwd, &init_args);
        assert_eq!(installed.status.code(), Some(0), "{args:?}: {installed:?}");
        assert!(
            dir.path().join("sub/.houserules.json").is_file(),
            "{args:?}"
        );
        assert!(!dir.path().join(".houserules.json").exists(), "{args:?}");
    }
}

/// A tree with `knowledge/` and no marker gets the `--id-prefix` refusal, which
/// the line for a tree with no install does not replace.
#[test]
fn update_in_a_tree_with_knowledge_and_no_marker_keeps_the_id_prefix_refusal() {
    let dir = scratch_git_repo();
    fs::create_dir(dir.path().join("knowledge")).expect("create knowledge/");
    let before = snapshot(dir.path());

    let output = run(dir.path(), &["update"]);

    assert_output(
        &output,
        2,
        "",
        &format!(
            "{}: not found, but {} exists; run houserules update --id-prefix <prefix> with the id prefix of your backlog (WI for WI-001), so that update stamps the right one\n",
            dir.path().join(".houserules.json").display(),
            dir.path().join("knowledge").display()
        ),
    );
    assert_same_tree(&before, &snapshot(dir.path()), "the refused update");
    let with_prefix = run(dir.path(), &["update", "--id-prefix", "HR"]);
    assert!(
        !stderr(&with_prefix).contains(NO_INSTALL),
        "{}",
        stderr(&with_prefix)
    );
}

/// A directory below an install belongs to that install: `update` there keeps
/// its own refusal and is no case of the line for a tree with no install.
#[test]
fn update_below_an_install_keeps_the_refusal_that_names_init_dir() {
    let dir = install();
    fs::create_dir(dir.path().join("sub")).expect("create sub/");

    let output = run(&dir.path().join("sub"), &["update"]);

    assert_eq!(output.status.code(), Some(2));
    let error = stderr(&output);
    assert!(
        error.contains("holds no .houserules.json; run houserules init --dir"),
        "{error}"
    );
}

// ---- the ignore line ----

/// Writes `.gitignore` of `dir` whole.
fn write_gitignore(dir: &Path, bytes: &[u8]) {
    fs::write(dir.join(".gitignore"), bytes).expect("write .gitignore");
}

/// Spec 7 and case 10: `update` replaces a `.houserules/step/` line with
/// `.houserules/` in the place of the line, keeps every other line, and prints
/// the line of the replacement. A second `update` prints nothing about the file
/// and changes nothing.
#[test]
fn update_replaces_the_step_line_and_keeps_every_other_line() {
    let dir = install();
    write_gitignore(dir.path(), b"target/\n.houserules/step/\n*.log\n");

    let first = run_ok(dir.path(), &["update"]);

    assert_eq!(count_lines(&stdout(&first), REPLACED_LINE), 1, "{first:?}");
    assert_eq!(
        text(dir.path(), ".gitignore"),
        "target/\n.houserules/\n*.log\n"
    );
    let after_first = snapshot(dir.path());
    let second = run_ok(dir.path(), &["update"]);
    assert!(
        !stdout(&second).contains(".gitignore"),
        "{}",
        stdout(&second)
    );
    assert_same_tree(&after_first, &snapshot(dir.path()), "the second update");
}

/// Review focus 4: a `.gitignore` whose `.houserules/step/` line ends in CRLF
/// or in trailing spaces gets `.houserules/` in the place of the line, and the
/// file keeps its line ending.
#[test]
fn update_replaces_a_crlf_step_line() {
    for (before, after) in [
        (
            "target/\r\n.houserules/step/\r\n*.log\r\n",
            "target/\r\n.houserules/\r\n*.log\r\n",
        ),
        (
            "target/\n.houserules/step/   \n*.log\n",
            "target/\n.houserules/\n*.log\n",
        ),
        (
            "target/\r\n.houserules/step/ \r\n",
            "target/\r\n.houserules/\r\n",
        ),
        ("target/\n.houserules/step/", "target/\n.houserules/"),
    ] {
        let dir = install();
        write_gitignore(dir.path(), before.as_bytes());

        let output = run_ok(dir.path(), &["update"]);

        assert_eq!(
            count_lines(&stdout(&output), REPLACED_LINE),
            1,
            "{before:?}: {output:?}"
        );
        assert_eq!(text(dir.path(), ".gitignore"), after, "{before:?}");
    }
}

/// `houserules.migrations-finish-after-a-stop`: the write of `.gitignore` stops
/// the run before the write of the marker, so the run prints no line for the
/// file; the next `update` replaces the line and leaves the tree of a run that
/// no fault stopped.
#[test]
fn an_update_that_stops_at_the_gitignore_finishes_at_the_next_update() {
    let control = install();
    write_gitignore(control.path(), b"target/\n.houserules/step/\n");
    run_ok(control.path(), &["update"]);

    let dir = install();
    write_gitignore(dir.path(), b"target/\n.houserules/step/\n");
    let Some(lock) = locked_file::lock(&dir.path().join(".gitignore")) else {
        return;
    };
    let failed = run(dir.path(), &["update"]);
    drop(lock);
    assert_eq!(failed.status.code(), Some(2), "{failed:?}");
    assert_eq!(
        count_lines(&stdout(&failed), REPLACED_LINE),
        0,
        "{failed:?}"
    );
    assert_eq!(
        text(dir.path(), ".gitignore"),
        "target/\n.houserules/step/\n"
    );

    let rerun = run_ok(dir.path(), &["update"]);

    assert_eq!(count_lines(&stdout(&rerun), REPLACED_LINE), 1, "{rerun:?}");
    assert_same_tree(
        &snapshot(control.path()),
        &snapshot(dir.path()),
        "the update that stopped at .gitignore",
    );
}

/// Spec 7: a line reads as git reads it. Unescaped trailing spaces do not count
/// and leading spaces do; a tab is no space. A line that git does not read as
/// `.houserules/` or as `.houserules/step/` is another pattern: `update` keeps
/// it and adds `.houserules/`. The check is git's own verdict, asked for the
/// folder of the workspaces.
#[test]
fn update_reads_an_ignore_line_as_git_does() {
    for (before, after) in [
        (" .houserules/\n", " .houserules/\n.houserules/\n"),
        (" .houserules/step/\n", " .houserules/step/\n.houserules/\n"),
        (".houserules/\t\n", ".houserules/\t\n.houserules/\n"),
        (
            "\t.houserules/step/\r\n",
            "\t.houserules/step/\r\n.houserules/\n",
        ),
    ] {
        let dir = install();
        write_gitignore(dir.path(), before.as_bytes());
        fs::create_dir_all(dir.path().join(".houserules/work")).expect("create the workspaces");

        let output = run_ok(dir.path(), &["update"]);

        assert_eq!(
            count_lines(&stdout(&output), "merged .gitignore (.houserules/ added)"),
            1,
            "{before:?}: {output:?}"
        );
        assert_eq!(text(dir.path(), ".gitignore"), after, "{before:?}");
        git(dir.path(), &["check-ignore", "-q", ".houserules/work/"]);
        let checked = run(dir.path(), &["check-knowledge"]);
        assert_eq!(checked.status.code(), Some(0), "{before:?}: {checked:?}");
    }
}

/// Spec 7: where neither line is present, `update` adds `.houserules/`; for no
/// file it writes one.
#[test]
fn update_adds_the_line_where_neither_is_present() {
    let dir = install();
    write_gitignore(dir.path(), b"target/\n*.log");

    let output = run_ok(dir.path(), &["update"]);

    assert_eq!(
        count_lines(&stdout(&output), "merged .gitignore (.houserules/ added)"),
        1,
        "{output:?}"
    );
    assert_eq!(
        text(dir.path(), ".gitignore"),
        "target/\n*.log\n.houserules/\n"
    );

    fs::remove_file(dir.path().join(".gitignore")).expect("remove .gitignore");
    let output = run_ok(dir.path(), &["update"]);
    assert_eq!(
        count_lines(&stdout(&output), "wrote .gitignore"),
        1,
        "{output:?}"
    );
    assert_eq!(text(dir.path(), ".gitignore"), ".houserules/\n");
}

/// `update` prints nothing about a `.gitignore` that holds `.houserules/`, and
/// leaves it alone. A file that holds both lines keeps both.
#[test]
fn update_leaves_a_gitignore_that_holds_the_line_alone() {
    for bytes in [
        "target/\n.houserules/\n",
        "target/\r\n.houserules/\r\n",
        ".houserules/step/\n.houserules/\n",
        "target/\n.houserules/   \n",
        "target/\r\n.houserules/ \r\n",
    ] {
        let dir = install();
        write_gitignore(dir.path(), bytes.as_bytes());

        let output = run_ok(dir.path(), &["update"]);

        assert!(
            !stdout(&output).contains(".gitignore"),
            "{bytes:?}: {}",
            stdout(&output)
        );
        assert_eq!(text(dir.path(), ".gitignore"), bytes);
    }
}

/// A project that lists `.gitignore` in `ownedPaths` owns it: `update` leaves
/// the line `.houserules/step/` in it.
#[test]
fn update_never_writes_a_gitignore_that_the_project_owns() {
    let dir = install();
    write_gitignore(dir.path(), b".houserules/step/\n");
    edit_marker(dir.path(), |marker| {
        marker.insert("ownedPaths".to_string(), json!([".gitignore"]));
    });

    let output = run_ok(dir.path(), &["update"]);

    assert!(
        !stdout(&output).contains(".gitignore"),
        "{}",
        stdout(&output)
    );
    assert_eq!(text(dir.path(), ".gitignore"), ".houserules/step/\n");
}

// ---- the PR-head checkout of the seeded CI workflow ----

/// `workflow` without the `ref` input: the file of an install that `update`
/// has not yet brought to the PR-head checkout.
fn without_the_ref(workflow: &str) -> String {
    workflow.replace(&format!("          {REF_INPUT}\n"), "")
}

/// Gives `dir` the file `old` as its seeded workflow, with the baseline of the
/// file at `old`: an unmodified copy of an older release.
fn install_workflow_as_unmodified(dir: &Path, old: &str) {
    fs::write(dir.join(WORKFLOW), old).expect("write the workflow");
    edit_marker(dir, |marker| {
        marker["baselines"][format!(":/{WORKFLOW}")] = json!(sha256_hex(old.as_bytes()));
    });
    commit_all(dir, "chore: install");
}

/// Case 10: an unmodified seeded workflow is rewritten with the `ref` input,
/// and the baseline moves to the new bytes. A second `update` prints nothing
/// about the file.
#[test]
fn update_rewrites_an_unmodified_seeded_workflow_with_the_ref() {
    let dir = repository();
    run_ok(dir.path(), &["init"]);
    let template = fs::read_to_string(repo_root().join("template").join(WORKFLOW))
        .expect("read the template workflow");
    install_workflow_as_unmodified(dir.path(), &without_the_ref(&text(dir.path(), WORKFLOW)));

    let first = run_ok(dir.path(), &["update"]);

    assert_eq!(
        count_lines(&stdout(&first), &format!("wrote {WORKFLOW}")),
        1,
        "{first:?}"
    );
    let workflow = text(dir.path(), WORKFLOW);
    assert_eq!(workflow, template);
    assert!(workflow.contains(REF_INPUT), "{workflow}");
    assert_eq!(
        read_json(dir.path(), ".houserules.json")["baselines"][format!(":/{WORKFLOW}")],
        json!(sha256_hex(workflow.as_bytes()))
    );
    commit_all(dir.path(), "chore: update");
    let second = run_ok(dir.path(), &["update"]);
    assert!(!stdout(&second).contains(WORKFLOW), "{}", stdout(&second));
}

/// The hand step of a kept workflow that lacks the PR-head checkout (spec 3.2).
fn head_checkout_step() -> String {
    format!(
        "hand step: {WORKFLOW} lacks the PR-head checkout; add {REF_INPUT} under with: of its actions/checkout step"
    )
}

/// The hand step of a kept workflow that holds `contents: read` and lacks
/// `issues: read` (spec 8.5).
fn issues_read_step() -> String {
    format!(
        "hand step: {WORKFLOW} lacks issues: read, which the github tracker needs; add issues: read to the permissions of the gates job"
    )
}

/// The hand step of a kept workflow that lacks the `GH_TOKEN` env of the GitHub
/// tracker (spec 8.5).
fn github_env_step() -> String {
    format!(
        "hand step: {WORKFLOW} lacks the GH_TOKEN env that the github tracker needs; add env: {{GH_TOKEN: ${{{{ github.token }}}}}} to each step that runs houserules check-backlog or houserules audit"
    )
}

/// Runs `update` with a `bd` that does not exist.
fn update_without_bd(dir: &Path) -> std::process::Output {
    houserules()
        .args(["update", "--dir"])
        .arg(dir)
        .env("HOUSERULES_BD", dir.join("no-such-bd"))
        .output()
        .expect("run houserules update")
}

/// Spec 3.2: a modified seeded workflow is kept, and `update` prints the hand
/// step of the PR-head checkout for every tracker, after the hand step of the
/// tracker. The line comes on every run until the file holds the input.
#[test]
fn update_keeps_a_modified_seeded_workflow_and_prints_its_hand_step() {
    let kept = format!("kept {WORKFLOW} (locally modified)");
    for (tracker, steps) in [
        (None, vec![head_checkout_step()]),
        (Some(json!({"adapter": "json"})), vec![head_checkout_step()]),
        (
            Some(json!({"adapter": "github", "repo": "acme/widgets"})),
            vec![issues_read_step(), github_env_step(), head_checkout_step()],
        ),
    ] {
        let dir = repository();
        run_ok(dir.path(), &["init"]);
        let mut changed = without_the_ref(&text(dir.path(), WORKFLOW));
        changed.push_str("# our own step\n");
        fs::write(dir.path().join(WORKFLOW), &changed).expect("change the workflow");
        edit_marker(dir.path(), |marker| {
            if let Some(tracker) = &tracker {
                marker.insert("tracker".to_string(), tracker.clone());
            }
        });
        commit_all(dir.path(), "chore: our step");

        for run_number in 1..=2 {
            let output = update_without_bd(dir.path());
            assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
            let lines: Vec<String> = stdout(&output).lines().map(str::to_string).collect();
            assert!(
                lines.contains(&kept),
                "{tracker:?} run {run_number}: {lines:?}"
            );
            let printed: Vec<String> = lines
                .iter()
                .filter(|line| line.starts_with("hand step: "))
                .cloned()
                .collect();
            assert_eq!(printed, steps, "{tracker:?} run {run_number}");
        }
        assert_eq!(text(dir.path(), WORKFLOW), changed, "{tracker:?}");

        changed.push_str(&format!("# {REF_INPUT}\n"));
        fs::write(dir.path().join(WORKFLOW), &changed).expect("add the input");
        let output = update_without_bd(dir.path());
        assert!(
            !stdout(&output).contains(&head_checkout_step()),
            "{tracker:?}: {}",
            stdout(&output)
        );
    }
}

/// Spec 3.2 and case 2: `init` writes the `ref` input under the `with:` of the
/// checkout in the top-level form, the form of the GitHub tracker, and the
/// subdirectory form, and `actionlint` passes on each file. The Beads form is
/// the case of `ci_workflow`'s own test.
#[test]
fn every_seeded_workflow_form_carries_the_ref_and_passes_actionlint() {
    let plain = repository();
    run_ok(plain.path(), &["init"]);
    let github = repository();
    run_ok(github.path(), &["init", "--tracker", "github"]);
    let nested = repository();
    fs::create_dir(nested.path().join("sub")).expect("create sub/");
    run_ok(&nested.path().join("sub"), &["init", "--tracker", "github"]);

    for (dir, file) in [
        (plain.path(), WORKFLOW),
        (github.path(), WORKFLOW),
        (nested.path(), ".github/workflows/knowledge-sub.yml"),
    ] {
        let workflow = text(dir, file);
        assert_eq!(workflow.matches(REF_INPUT).count(), 1, "{file}: {workflow}");
        let after_checkout = workflow
            .split_once("uses: actions/checkout@")
            .and_then(|(_, rest)| rest.split_once('\n'))
            .map(|(_, rest)| rest)
            .expect("the workflow checks out the repository");
        assert!(
            after_checkout.starts_with(&format!("        with:\n          {REF_INPUT}\n")),
            "{file}: {workflow}"
        );
        assert_actionlint_passes(&dir.join(file));
    }
}

/// The text of the job `name` in `ci`: its header line and the lines below it,
/// up to the header of the next job.
fn job_of(ci: &str, name: &str) -> String {
    let mut body = String::new();
    let mut inside = false;
    for line in ci.lines() {
        if line.starts_with("  ") && !line.starts_with("   ") && line.trim_end().ends_with(':') {
            inside = line.trim() == format!("{name}:");
        }
        if inside {
            body.push_str(line);
            body.push('\n');
        }
    }
    body
}

/// Spec 3.2 (203): the `checks` job of this repository's `ci.yml`, which runs
/// the audit, checks out the PR head. The other jobs keep the default
/// checkout.
#[test]
fn the_checks_job_of_ci_yml_checks_out_the_pr_head() {
    let ci = fs::read_to_string(repo_root().join(".github/workflows/ci.yml")).expect("read ci.yml");

    let checks = job_of(&ci, "checks");
    assert_eq!(checks.matches(REF_INPUT).count(), 1, "{checks}");
    assert!(
        checks.contains(&format!("        with:\n          {REF_INPUT}\n")),
        "{checks}"
    );
    for other in ["rust", "check-commit"] {
        assert!(!job_of(&ci, other).contains(REF_INPUT), "{other}");
    }
}

// ---- HR-220: init with no tracker flag on an install that has a tracker ----

/// Removes the deliverables schema of the install at `dir`, as a lost file.
fn lose_the_deliverables_schema(dir: &Path) {
    fs::remove_file(dir.join(".claude/schemas/deliverables.json")).expect("remove the schema");
}

/// The `backlogId` pattern that the deliverables schema of `dir` holds.
fn backlog_id_pattern(dir: &Path) -> String {
    read_json(dir, ".claude/schemas/deliverables.json")["$defs"]["backlogId"]["pattern"]
        .as_str()
        .expect("the pattern is a string")
        .to_string()
}

/// Spec 8.4 and case 9: `init` with no `--tracker` on an install whose marker
/// names a tracker seeds a lost deliverables schema with the pattern of that
/// tracker, for JSON and GitHub with no backend call.
#[test]
fn init_without_a_tracker_flag_seeds_the_pattern_of_the_installed_tracker() {
    let json_install = repository();
    run_ok(
        json_install.path(),
        &["init", "--tracker", "json", "--id-prefix", "HR"],
    );
    let github_install = repository();
    run_ok(
        github_install.path(),
        &["init", "--tracker", "github", "--repo", "acme/widgets"],
    );

    for (dir, pattern) in [
        (
            json_install.path(),
            "^(HR-[0-9]+|A-[0-9]+|PP-[0-9]+-[0-9]+)$",
        ),
        (github_install.path(), "^#[0-9]+$"),
    ] {
        lose_the_deliverables_schema(dir);

        let output = run_ok(dir, &["init"]);

        assert_eq!(
            count_lines(&stdout(&output), "wrote .claude/schemas/deliverables.json"),
            1,
            "{output:?}"
        );
        assert_eq!(backlog_id_pattern(dir), pattern);
        assert_eq!(stderr(&output), "");
    }
}

/// Spec 8.4: for Beads, `init` keeps the handling of an unreachable `bd` that
/// `update` has (C38): it prints the unreachable line on stderr, exits 0, and
/// seeds the lost schema with the pattern of the kit, since no pattern is held.
#[test]
fn init_without_a_tracker_flag_on_an_unreachable_beads_install_prints_the_unreachable_line() {
    let dir = repository();
    run_ok(dir.path(), &["init"]);
    edit_marker(dir.path(), |marker| {
        marker.insert("tracker".to_string(), json!({"adapter": "beads"}));
    });
    lose_the_deliverables_schema(dir.path());
    let kit: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(repo_root().join("template/.claude/schemas/deliverables.json"))
            .expect("read the kit schema"),
    )
    .expect("parse the kit schema");

    let output = houserules()
        .args(["init", "--dir"])
        .arg(dir.path())
        .env("HOUSERULES_BD", dir.path().join("no-such-bd"))
        .output()
        .expect("run houserules init");

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), format!("{BD_MISSING}\n"));
    assert_eq!(
        backlog_id_pattern(dir.path()),
        kit["$defs"]["backlogId"]["pattern"]
            .as_str()
            .expect("a pattern")
    );
}
