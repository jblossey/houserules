//! The workflow-agnostic step (batch 34, spec 4.4a; live case 25): a core
//! skill and a core hook script that any workflow calls. Every install gets
//! them, a minimal install included. Each case runs the shipped script with
//! a POSIX shell against a scratch install, on a `PATH` that holds the
//! binary under test. With a report, the script also fills the facts of the
//! report (batch 36, spec 7 and case 11a).

mod tracker_support;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    commit_all, git, read_json, repo_root, run_ok, scratch_git_repo, stderr, stdout, write_json,
};

/// The script, relative to an install root.
const SCRIPT: &str = "tools/houserules-step.sh";

/// The skill, relative to an install root.
const SKILL: &str = ".claude/skills/houserules-step/SKILL.md";

/// The usage line of the script when its arguments fit no call.
const USAGE: &str = "usage: houserules-step.sh before <path>... | after <base> [<report>]";

/// A scratch install that a plain `init` made, with its first commit.
fn installed() -> (TempDir, String) {
    let dir = scratch_git_repo();
    run_ok(dir.path(), &["init"]);
    let base = commit_all(dir.path(), "chore: install the kit");
    (dir, base)
}

/// `PATH` with the directory of the binary under test in front.
fn path_with_the_binary() -> OsString {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_houserules"));
    let mut dirs = vec![binary.parent().expect("a parent directory").to_path_buf()];
    dirs.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    env::join_paths(dirs).expect("join PATH entries")
}

/// Runs the script of the install at `dir` with `args`, from the
/// install root, with `sh`.
fn step(dir: &Path, args: &[&str]) -> Output {
    Command::new("sh")
        .arg(SCRIPT)
        .args(args)
        .current_dir(dir)
        .env("PATH", path_with_the_binary())
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .output()
        .expect("run the step script")
}

/// The lines of stdout.
fn lines(output: &Output) -> Vec<String> {
    stdout(output).lines().map(str::to_string).collect()
}

/// Writes a validating `BLOCKED` task report to `name` below `dir`.
fn write_report(dir: &Path, name: &str, valid: bool) -> String {
    let report = if valid {
        json!({
            "kind": "task-report", "model": "claude-opus-5-5", "task": 1, "backlog": [],
            "status": "BLOCKED", "implemented": "x",
            "commits": [], "tests": [], "live_run": [], "tdd": [],
            "files_changed": [], "docs_verified": [], "dependency_vetting": null,
            "coverage": [], "self_audit": null, "self_review": [], "concerns": [],
            "knowledge_used": [],
        })
    } else {
        json!({"kind": "task-report"})
    };
    fs::write(
        dir.join(name),
        serde_json::to_string_pretty(&report).expect("serialize"),
    )
    .expect("write the report");
    name.to_string()
}

/// Spec 4.4a and live case 25: `before <path>...` writes the rule package of
/// the paths to `.houserules/step/rules.json` and prints that file's path.
#[test]
fn before_writes_the_rule_package_and_prints_its_path() {
    let (dir, _base) = installed();
    let output = step(dir.path(), &["before", "AGENTS.md", "docs/README.md"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let printed = lines(&output);
    assert_eq!(printed.len(), 1, "{printed:?}");
    assert!(
        printed[0].ends_with(".houserules/step/rules.json"),
        "{printed:?}"
    );
    let rules = read_json(dir.path(), ".houserules/step/rules.json");
    assert_eq!(rules["paths"], json!(["AGENTS.md", "docs/README.md"]));
    assert!(rules["entries"].is_array(), "got {rules}");
}

/// Spec 4.4a: `after <base>` runs the audit of the range, writes its JSON to
/// `.houserules/step/audit.json`, and prints that path and the exit status of
/// the audit.
#[test]
fn after_audits_the_range_and_prints_the_file_and_the_exit_status() {
    let (dir, base) = installed();
    fs::write(dir.path().join("docs/note.md"), "A note.\n").expect("write a document");
    commit_all(dir.path(), "docs: add a note");
    let output = step(dir.path(), &["after", &base]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let printed = lines(&output);
    assert_eq!(printed.len(), 2, "{printed:?}");
    assert!(
        printed[0].ends_with(".houserules/step/audit.json"),
        "{printed:?}"
    );
    assert_eq!(printed[1], "audit exit status: 0");
    let audit = read_json(dir.path(), ".houserules/step/audit.json");
    assert!(audit.get("summary").is_some(), "got {audit}");
}

/// Spec 4.4a and live case 25, spec 7 of batch 36: with a report path as the
/// second argument, `after` also fills the facts of the report and validates it.
/// It prints the file and the exit status of `report-facts`, then those of
/// `validate`.
#[test]
fn after_with_a_report_fills_its_facts_and_validates_it() {
    let (dir, base) = installed();
    fs::write(dir.path().join("docs/note.md"), "A note.\n").expect("write a document");
    let head = commit_all(dir.path(), "docs: add a note");
    let capture = Command::new(env!("CARGO_BIN_EXE_houserules"))
        .args(["capture", "--out", "cap.txt", "--", "sh", "-c", "echo ok"])
        .current_dir(dir.path())
        .output()
        .expect("run capture");
    assert_eq!(
        capture.status.code(),
        Some(0),
        "stderr: {}",
        stderr(&capture)
    );
    let report = write_report(dir.path(), "report.json", true);
    let mut draft = read_json(dir.path(), &report);
    draft["tests"] = json!([{"capture": "cap.txt"}]);
    draft["commits"] = json!([]);
    write_json(dir.path(), &report, &draft);

    let output = step(dir.path(), &["after", &base, &report]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let printed = lines(&output);
    assert_eq!(printed.len(), 6, "{printed:?}");
    assert!(
        printed[0].ends_with(".houserules/step/audit.json"),
        "{printed:?}"
    );
    assert_eq!(printed[1], "audit exit status: 0");
    assert!(
        printed[2].ends_with(".houserules/step/report-facts.txt"),
        "{printed:?}"
    );
    assert_eq!(printed[3], "report-facts exit status: 0");
    assert!(
        printed[4].ends_with(".houserules/step/validate.json"),
        "{printed:?}"
    );
    assert_eq!(printed[5], "validate exit status: 0");
    assert_eq!(
        fs::read_to_string(dir.path().join(".houserules/step/report-facts.txt"))
            .expect("read the output of report-facts"),
        "report.json: facts filled\n"
    );
    let filled = read_json(dir.path(), &report);
    assert_eq!(
        filled["commits"],
        json!([{"sha": head, "subject": "docs: add a note"}])
    );
    assert_eq!(filled["files_changed"], json!(["docs/note.md"]));
    assert_eq!(filled["tests"][0]["command"], json!("sh -c 'echo ok'"));
    assert_eq!(filled["tests"][0]["exit"], json!(0));
    assert!(filled["tests"][0]["sha256"].is_string(), "got {filled}");
    let audit = read_json(dir.path(), ".houserules/step/audit.json");
    assert_eq!(filled["self_audit"]["summary"], audit["summary"]);
    let deterministic: Vec<&Value> = audit["rules"]
        .as_array()
        .expect("the rules")
        .iter()
        .filter(|row| row["mode"] == "deterministic")
        .collect();
    assert_eq!(
        filled["self_audit"]["rows"],
        json!(deterministic),
        "self_audit holds the deterministic rows of the audit"
    );
    let result: Value = read_json(dir.path(), ".houserules/step/validate.json");
    assert_eq!(result[0]["errors"], json!([]));

    let first = fs::read(dir.path().join(&report)).expect("read the report");
    let again = step(dir.path(), &["after", &base, &report]);
    assert_eq!(again.status.code(), Some(0), "stderr: {}", stderr(&again));
    assert_eq!(
        fs::read(dir.path().join(&report)).expect("read the report"),
        first,
        "a second run changes no byte"
    );
}

/// Spec 7 of batch 36: with a report, `after` runs the audit with `--report`, so
/// a report that is missing stops the audit with exit status 2. The script
/// removes the audit result of the last run first, so that `report-facts` never
/// reads the result of another run.
#[test]
fn the_audit_of_a_call_with_a_report_reads_it_and_leaves_no_result_of_an_earlier_run() {
    let (dir, base) = installed();
    fs::write(dir.path().join("docs/note.md"), "A note.\n").expect("write a document");
    commit_all(dir.path(), "docs: add a note");
    let report = write_report(dir.path(), "report.json", true);
    step(dir.path(), &["after", &base, &report]);
    assert!(dir.path().join(".houserules/step/audit.json").is_file());

    let output = step(dir.path(), &["after", &base, "missing.json"]);

    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));
    assert_eq!(lines(&output)[1], "audit exit status: 2");
    assert!(
        !dir.path().join(".houserules/step/audit.json").exists(),
        "the audit result of the earlier run is still there"
    );
}

/// A report that fails `validate` shows in the printed status and in the exit
/// status of the script.
#[test]
fn after_with_an_invalid_report_exits_with_the_status_of_validate() {
    let (dir, base) = installed();
    fs::write(dir.path().join("docs/note.md"), "A note.\n").expect("write a document");
    commit_all(dir.path(), "docs: add a note");
    let report = write_report(dir.path(), "report.json", false);
    let output = step(dir.path(), &["after", &base, &report]);
    assert_eq!(output.status.code(), Some(1));
    let printed = lines(&output);
    assert_eq!(
        printed.last().map(String::as_str),
        Some("validate exit status: 1")
    );
}

/// A call that fits neither form prints the usage line and exits 2.
#[test]
fn a_call_that_fits_no_form_prints_the_usage_and_exits_2() {
    let (dir, _base) = installed();
    for args in [
        &[][..],
        &["before"][..],
        &["after"][..],
        &["sideways", "x"][..],
    ] {
        let output = step(dir.path(), args);
        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (Some(2), String::new(), format!("{USAGE}\n")),
            "args {args:?}"
        );
    }
}

/// Spec 4.4a: `init` ignores the scratch directory, so a run of the step
/// leaves the working tree clean.
#[test]
fn the_scratch_output_of_the_step_stays_out_of_the_working_tree() {
    let (dir, base) = installed();
    step(dir.path(), &["before", "AGENTS.md"]);
    step(dir.path(), &["after", &base]);
    assert!(dir.path().join(".houserules/step/rules.json").is_file());
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
}

/// Spec 4.4a (C34): an install that `update` gave the step has no
/// `.houserules/step/` line in its `.gitignore`. The script ignores its own
/// output there: the directory holds a `.gitignore` of `*`, and `before` and
/// `after` leave `git status` empty.
#[test]
fn the_step_ignores_its_own_output_on_an_install_without_the_gitignore_line() {
    let (dir, _base) = installed();
    fs::remove_file(dir.path().join(".gitignore")).expect("delete .gitignore");
    let base = commit_all(dir.path(), "chore: an install that update made");

    step(dir.path(), &["before", "AGENTS.md"]);
    assert!(dir.path().join(".houserules/step/rules.json").is_file());
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    step(dir.path(), &["after", &base]);
    assert!(dir.path().join(".houserules/step/audit.json").is_file());
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_eq!(
        fs::read_to_string(dir.path().join(".houserules/step/.gitignore"))
            .expect("read the ignore file of the directory"),
        "*\n"
    );
}

/// Spec 4.4a and live case 25: the skill names the four steps and the two
/// calls of the script, and the two files name no workflow and no plugin.
#[test]
fn the_skill_names_the_four_steps_and_the_two_calls_and_no_workflow_or_plugin() {
    let skill =
        fs::read_to_string(repo_root().join("template").join(SKILL)).expect("read the skill");
    let script =
        fs::read_to_string(repo_root().join("template").join(SCRIPT)).expect("read the script");
    assert!(skill.contains("tools/houserules-step.sh before"), "{skill}");
    assert!(skill.contains("tools/houserules-step.sh after"), "{skill}");
    assert!(skill.contains("houserules for"), "{skill}");
    assert!(skill.contains("houserules audit"), "{skill}");
    assert!(skill.contains("houserules validate"), "{skill}");
    for text in [&skill, &script] {
        let lower = text.to_lowercase();
        for word in ["superpowers", "orchestrating", "reference workflow", "bmad"] {
            assert!(!lower.contains(word), "a step file names {word:?}");
        }
    }
}

/// Step 4 of the skill, as spec 7 of batch 36 states it: the author runs each
/// test and live run through `capture`, and the script fills the facts of the
/// report.
const SKILL_STEP_4: &str = r#"**Write and validate the report.** Write the report in the report format of `.claude/schemas/deliverables.json`. Run each test and live run through `houserules capture --out <file> -- <command>`, with `<file>` in the workspace (default `.houserules/work/`), never in `.houserules/step/`, which the script overwrites; write each run as `{ "capture": "<path>" }`. Run `tools/houserules-step.sh after <base> <report>`. The script runs the audit with `--report`, then `houserules report-facts`, which fills the report's facts, then `houserules validate`, and writes their output to `.houserules/step/`. Fix every error."#;

/// Spec 7 of batch 36: the skill holds step 4 as the spec words it.
#[test]
fn the_skill_states_step_4_as_the_spec_words_it() {
    let skill =
        fs::read_to_string(repo_root().join("template").join(SKILL)).expect("read the skill");
    assert!(
        skill.contains(&format!("\n4. {SKILL_STEP_4}\n")),
        "step 4 of the skill differs from the spec:\n{skill}"
    );
}

/// Run through a symlink from another directory, the script still finds the
/// install root: it resolves its own links (`houserules.hooks-resolve-their-
/// own-path`), so its output lands below the root of the install that holds
/// the real script.
#[cfg(unix)]
#[test]
fn a_symlink_to_the_script_finds_the_install_root() {
    let (dir, _base) = installed();
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let link = elsewhere.path().join("step");
    std::os::unix::fs::symlink(dir.path().join(SCRIPT), &link).expect("link the script");
    let output = Command::new("sh")
        .arg(&link)
        .args(["before", "AGENTS.md"])
        .current_dir(elsewhere.path())
        .env("PATH", path_with_the_binary())
        .output()
        .expect("run the linked script");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(dir.path().join(".houserules/step/rules.json").is_file());
    assert!(!elsewhere.path().join(".houserules").exists());
}

/// A script in a directory whose parent holds no `.houserules.json` is in no
/// install: it names that and exits 2, and it writes nothing.
#[test]
fn a_script_outside_an_install_says_so_and_writes_nothing() {
    let (dir, _base) = installed();
    let stray = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(stray.path().join("tools")).expect("create tools/");
    fs::copy(
        dir.path().join(SCRIPT),
        stray.path().join("tools/houserules-step.sh"),
    )
    .expect("copy the script");
    let output = Command::new("sh")
        .arg("tools/houserules-step.sh")
        .args(["before", "AGENTS.md"])
        .current_dir(stray.path())
        .env("PATH", path_with_the_binary())
        .output()
        .expect("run the stray script");
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (
            Some(2),
            String::new(),
            "houserules-step.sh: no .houserules.json in the parent of the directory of the script\n"
                .to_string()
        )
    );
    assert!(!stray.path().join(".houserules").exists());
}
