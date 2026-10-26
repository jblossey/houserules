//! The gates and floor scripts of both units, run for real with a stub `houserules`.

use super::*;
use crate::tracker_support::{commit_all, houserules, scratch_git_repo};

/// A `houserules` that records its arguments, writes the document of `$STUB_FLOOR`
/// to the file of each `--json` after `route`, `{}` to the file of every other
/// `--json` and `--out`, fails for the subcommand `$STUB_FAIL`, and fails an
/// `evidence` call that holds `--intent` when `$STUB_REJECT_INTENT` is set.
const SCRIPTED_HOUSERULES: &str = "#!/bin/sh\n\
    if [ \"$1\" = --version ]; then echo 'houserules 9.9.9'; exit 0; fi\n\
    echo \"$*\" >> \"$STUB_DIR/houserules.log\"\n\
    subcommand=$1\n\
    [ \"$1\" = bundle ] && subcommand=\"bundle $2\"\n\
    if [ \"$subcommand\" = \"${STUB_FAIL:-}\" ]; then exit 1; fi\n\
    case \"$*\" in *--intent=*) if [ -n \"${STUB_REJECT_INTENT:-}\" ]; then exit 2; fi ;; esac\n\
    floor=${STUB_FLOOR-}\n\
    [ -n \"$floor\" ] || floor='{}'\n\
    while [ $# -gt 0 ]; do case $1 in\n\
      --json) if [ \"$subcommand\" = route ]; then printf '%s\\n' \"$floor\" > \"$2\"; else printf '{}\\n' > \"$2\"; fi; shift 2;;\n\
      --out) printf '<!-- packet -->\\n' > \"$2\"; shift 2;;\n\
      *) shift;; esac; done\n";

/// Spec 10.1: the GitLab floor script installs the binary of its source, runs
/// `route --no-audit` at the source head, and writes only a valid route to the
/// dotenv report; the default branch is built from a work tree of its own.
#[test]
fn the_gitlab_floor_script_computes_the_floor_and_writes_the_dotenv_report() {
    let unit = gitlab("root");
    let run = |source: &str, floor: &str| {
        let sandbox = Sandbox::new();
        let remote = sandbox.path().join("remote").join("widgets.git");
        let repository = sandbox.work();
        let (first, _) = repository_with_origin(&repository, &remote);
        fs::create_dir_all(repository.join("crates").join("houserules")).expect("create the crate");
        fs::write(
            repository
                .join("crates")
                .join("houserules")
                .join("Cargo.toml"),
            "[package]\n",
        )
        .expect("write Cargo.toml");
        git(&repository, &["add", "."]);
        git(&repository, &["commit", "-q", "-m", "crate"]);
        let head = git(&repository, &["rev-parse", "HEAD"]).trim().to_string();
        git(
            &repository,
            &["push", "-q", "origin", "HEAD:refs/heads/main"],
        );
        let program = sandbox.path().join("runner-image").join("houserules");
        fs::create_dir_all(program.parent().unwrap()).expect("create the runner folder");
        fs::write(&program, SCRIPTED_HOUSERULES).expect("write the runner binary");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755))
            .expect("make it executable");
        sandbox.install_program(
            "cargo",
            &format!(
                "#!/bin/sh\necho \"$*\" >> \"$STUB_DIR/cargo.log\"\nroot=\nwhile [ $# -gt 0 ]; do case $1 in --root) root=$2; shift 2;; *) shift;; esac; done\n\
                 mkdir -p \"$root/bin\"\ncp {} \"$root/bin/houserules\"\n",
                text(&program)
            ),
        );
        let source = source.replace("RUNNER", &text(&program));
        let env = env_of(
            &unit,
            "floor",
            &[
                ("HOUSERULES_DIR", "."),
                ("HOUSERULES_SOURCE", source.as_str()),
                ("CI_DEFAULT_BRANCH", "main"),
                ("CI_MERGE_REQUEST_DIFF_BASE_SHA", first.as_str()),
                ("CI_MERGE_REQUEST_SOURCE_BRANCH_SHA", head.as_str()),
                ("CI_COMMIT_SHA", "cccccccccccccccccccccccccccccccccccccccc"),
                ("STUB_FLOOR", floor),
            ],
        );
        let output = sandbox.run_sh(&unit.job_script("floor"), env);
        (output, sandbox, first, head)
    };

    for source in ["runner:RUNNER", "default-branch"] {
        let (output, sandbox, first, head) = run(source, "{\"route\":\"owner-review\"}");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{source}: {}{}",
            stdout(&output),
            stderr(&output)
        );
        assert_eq!(
            fs::read_to_string(sandbox.work().join("floor.env")).expect("the dotenv report"),
            "HOUSERULES_FLOOR_ROUTE=owner-review\n",
            "{source}"
        );
        assert_eq!(
            houserules_log(&sandbox),
            vec![format!(
                "route --no-audit --base {first} --head {head} --json floor.json --dir ."
            )],
            "{source}"
        );
        assert_eq!(
            git(&sandbox.work(), &["rev-parse", "HEAD"]).trim(),
            head,
            "{source}: the job checks out the source head"
        );
        if source == "default-branch" {
            let cargo =
                fs::read_to_string(sandbox.stub_dir().join("cargo.log")).expect("cargo log");
            let path = cargo
                .split(" --path ")
                .nth(1)
                .and_then(|rest| rest.split(" --bin ").next())
                .expect("a --path argument");
            assert!(
                path.ends_with("/crates/houserules")
                    && Path::new(path).join("Cargo.toml").is_file(),
                "cargo builds the crate of a work tree of the default branch: {path}"
            );
        }
    }

    for floor in ["{\"route\":\"banana\"}", "{}", "not json"] {
        let (output, sandbox, _, _) = run("runner:RUNNER", floor);
        assert_eq!(output.status.code(), Some(1), "{floor}");
        assert_eq!(
            stdout(&output),
            "houserules: route --no-audit wrote no valid route\n",
            "{floor}"
        );
        assert!(
            !sandbox.work().join("floor.env").exists(),
            "{floor}: no dotenv report"
        );
    }
}

/// Spec 10.1: the GitHub floor step passes a route of the three values to the job
/// output and refuses every other value.
#[test]
fn the_github_floor_step_writes_the_job_output_only_for_a_valid_route() {
    let unit = github();
    for (floor, expected) in [
        ("{\"route\":\"senior-review\"}", Some("senior-review")),
        ("{\"route\":\"banana\"}", None),
        ("{}", None),
    ] {
        let sandbox = Sandbox::new();
        sandbox.install_program("houserules", SCRIPTED_HOUSERULES);
        let output_file = text(&sandbox.temp().join("github_output"));
        let env = env_of(
            &unit,
            "floor",
            &[
                ("HOUSERULES_DIR", "."),
                ("HOUSERULES_BASE", "base-sha"),
                ("HOUSERULES_HEAD", "head-sha"),
                ("GITHUB_OUTPUT", output_file.as_str()),
                ("STUB_FLOOR", floor),
            ],
        );
        let output = sandbox.run_bash(&unit.step_script("floor", "Compute the floor"), env);
        assert_eq!(
            houserules_log(&sandbox),
            vec![format!(
                "route --no-audit --base base-sha --head head-sha --json {} --dir .",
                text(&sandbox.temp().join("houserules-floor.json"))
            )]
        );
        match expected {
            Some(route) => {
                assert_eq!(
                    output.status.code(),
                    Some(0),
                    "{floor}: {}",
                    stdout(&output)
                );
                assert_eq!(
                    fs::read_to_string(&output_file).expect("the job output"),
                    format!("route={route}\n")
                );
            }
            None => {
                assert_eq!(output.status.code(), Some(1), "{floor}");
                assert_eq!(
                    stdout(&output),
                    "::error::houserules: route --no-audit wrote no valid route\n"
                );
                assert!(!Path::new(&output_file).exists(), "{floor}: no job output");
            }
        }
    }
}

/// The descriptions of the intent tests, with the `--intent` value that the
/// evidence call must carry, or `None` for no such argument.
fn intent_cases() -> Vec<(String, Option<&'static str>)> {
    vec![
        (
            "## Evidence bundle\n\nPurpose: x\nIntent: file:docs/plans/a.md\nWork items: HR-1\n".to_string(),
            Some("file:docs/plans/a.md"),
        ),
        (
            "## Evidence bundle\r\n\r\nPurpose: x\r\nIntent: file:docs/plans/a.md\r\nWork items: HR-1\r\n".to_string(),
            Some("file:docs/plans/a.md"),
        ),
        (
            "Intent: file:before.md\n\n## Evidence bundle\nPurpose: x\n".to_string(),
            None,
        ),
        (
            "## Evidence bundle\nPurpose: x\n\n```houserules-run\nIntent: file:in-fence.md\n```\n".to_string(),
            None,
        ),
        (
            "## Evidence bundle\nPurpose: x\n## Another\nIntent: file:next-section.md\n".to_string(),
            None,
        ),
        ("no section at all\n".to_string(), None),
        // spec 7.1: a run block ends nothing, so the line after it counts
        (
            "## Evidence bundle\n\nPurpose: x\nWork items: HR-1\n\n```houserules-run\n{\"schema\":\"houserules/capture@1.0.0\"}\n```\n\nIntent: file:docs/plans/p.md\n".to_string(),
            Some("file:docs/plans/p.md"),
        ),
        (
            "## Evidence bundle\r\n\r\n```houserules-run\r\n{}\r\n```\r\nIntent: file:crlf.md\r\n".to_string(),
            Some("file:crlf.md"),
        ),
        // spec 7.1: a heading inside a fence of another section starts no section
        (
            "## Notes\n\n```\n## Evidence bundle\nIntent: file:evil.md\n```\n\n## Evidence bundle\n\nIntent: file:docs/plans/real.md\n".to_string(),
            Some("file:docs/plans/real.md"),
        ),
        (
            "## Notes\n\n```\n## Evidence bundle\nIntent: file:evil.md\n```\n".to_string(),
            None,
        ),
        // spec 7.1: a `## ` line inside a fence of the section ends nothing
        (
            "## Evidence bundle\n\n```\n## Not a heading\n```\nIntent: file:after-fenced-heading.md\n".to_string(),
            Some("file:after-fenced-heading.md"),
        ),
        // a fence closes at a run of backticks at least as long as its opening
        (
            "## Evidence bundle\n\n````\n```\nIntent: file:in-long-fence.md\n```\n````\n\nIntent: file:after-long-fence.md\n".to_string(),
            Some("file:after-long-fence.md"),
        ),
        (
            "## Evidence bundle\n\n```\nIntent: file:unclosed.md\n".to_string(),
            None,
        ),
        // a line of backticks and text is no fence
        (
            "## Evidence bundle\n\n```inline```\nIntent: file:inline.md\n".to_string(),
            Some("file:inline.md"),
        ),
        // a second bundle heading ends the section of the first
        (
            "## Evidence bundle\nPurpose: x\n## Evidence bundle\nIntent: file:second.md\n".to_string(),
            None,
        ),
        // spec 7.1: the white space of both rules is the ASCII set whatever the locale of the
        // runner is: an em space after the heading makes it no heading
        (
            "## Evidence bundle\u{2003}\n\nIntent: file:docs/plans/missing.md\n".to_string(),
            None,
        ),
        // and an em space after a run of backticks leaves the fence open
        (
            "## Evidence bundle\n\n```\nx\n```\u{2003}\nIntent: file:docs/plans/missing.md\n".to_string(),
            None,
        ),
    ]
}

/// The locale that a runner may have: a UTF-8 locale. An `awk` that runs under it widens the class
/// `[[:space:]]` to the white space of Unicode (gawk under `C.UTF-8` matches U+1680, U+2000 to
/// U+2006, U+2008 to U+200A, U+2028, U+2029, U+205F, and U+3000), so a test that reads a description
/// with `hr_intent` runs in it. The sandbox sets `LC_ALL=C`.
const UTF8_LOCALE: (&str, &str) = ("LC_ALL", "C.UTF-8");

/// Spec 5.1 and 7.1: the shared function `hr_intent` of both units reads the same `Intent:` line
/// of the bundle section for every description, as the section rules of 7.1 say: fences are
/// tracked through the whole description.
#[test]
fn the_shared_intent_function_follows_the_section_rules_of_spec_7_1() {
    for unit in [github(), gitlab("root")] {
        for (description, intent) in intent_cases() {
            let sandbox = Sandbox::new();
            fs::write(sandbox.work().join("description.md"), &description)
                .expect("write the description");
            let output = sandbox.run_sh(
                "eval \"$HOUSERULES_SH\"\nhr_intent description.md\n",
                env_of(&unit, "gates", &[UTF8_LOCALE]),
            );
            assert_eq!(
                output.status.code(),
                Some(0),
                "{description:?}: {}{}",
                stdout(&output),
                stderr(&output)
            );
            assert_eq!(
                stdout(&output).trim_end_matches(['\r', '\n']),
                intent.unwrap_or(""),
                "{description:?}"
            );
        }
    }
}

/// A scratch install with one commit, for `bundle check` over the empty range
/// from that commit to itself: the marker, an area file, and one topic file.
fn bundle_install() -> (tempfile::TempDir, String) {
    let top = scratch_git_repo();
    fs::write(
        top.path().join(".houserules.json"),
        "{\"schema\":\"houserules/install@2.3.0\",\"idPrefix\":\"WI\"}\n",
    )
    .expect("write the marker");
    let knowledge = top.path().join("knowledge");
    fs::create_dir(&knowledge).expect("create the knowledge folder");
    fs::write(knowledge.join("schema.json"), "{}\n").expect("write the schema");
    fs::write(
        knowledge.join("areas.json"),
        "{\"global\":{\"paths\":[]}}\n",
    )
    .expect("write the areas");
    fs::write(knowledge.join("team.json"), "{\"entries\":[]}\n").expect("write the topic");
    let head = commit_all(top.path(), "chore: base");
    (top, head)
}

/// The intent that `bundle check` names for `description`: the text between the
/// code span marks of the first finding line `bundle check: intent `<intent>`: ...`,
/// or `None` when it prints none. The intents of `intent_cases` name files that
/// the install lacks, so `bundle check` prints a line for each of them.
fn intent_named_by_bundle_check(top: &Path, head: &str, description: &str) -> Option<String> {
    let body = top.join("description.md");
    fs::write(&body, description).expect("write the description");
    let output = houserules()
        .args(["bundle", "check", "--body"])
        .arg(&body)
        .args(["--base", head, "--head", head, "--dir"])
        .arg(top)
        .output()
        .expect("run bundle check");
    fs::remove_file(&body).expect("remove the description");
    stdout(&output).lines().find_map(|line| {
        let named = line.strip_prefix("bundle check: intent `")?;
        named
            .split_once("`: ")
            .map(|(intent, _)| intent.to_string())
    })
}

/// Spec 7.1: `bundle check` and the shared function `hr_intent` of both units
/// read fences and sections by one rule, so both name the same `Intent:` line for
/// every description of `intent_cases`: the shell function runs for real, and
/// `bundle check` is the binary of this tree.
#[test]
fn bundle_check_reads_the_intent_line_as_the_shared_intent_function_does() {
    let (top, head) = bundle_install();
    for (description, expected) in intent_cases() {
        let named = intent_named_by_bundle_check(top.path(), &head, &description);
        assert_eq!(named.as_deref(), expected, "bundle check: {description:?}");
        for unit in [github(), gitlab("root")] {
            let sandbox = Sandbox::new();
            fs::write(sandbox.work().join("description.md"), &description)
                .expect("write the description");
            let output = sandbox.run_sh(
                "eval \"$HOUSERULES_SH\"\nhr_intent description.md\n",
                env_of(&unit, "gates", &[UTF8_LOCALE]),
            );
            assert_eq!(output.status.code(), Some(0), "{description:?}");
            let shell = stdout(&output).trim_end_matches(['\r', '\n']).to_string();
            assert_eq!(
                (!shell.is_empty()).then_some(shell),
                named,
                "hr_intent and bundle check differ: {description:?}"
            );
        }
    }
}

/// Spec 10.1: the units pass the intent line of the bundle section to `evidence`,
/// read by the section rules of spec 7.1 (not before the section, not in a fence,
/// not in the next section, and after a run block), without a carriage return.
/// When `evidence` refuses the intent, the units run it again without, so that
/// the packet still appears.
#[test]
fn the_github_evidence_step_passes_the_intent_line_of_the_bundle_section() {
    let unit = github();
    let run = |description: &str, reject: bool, audit: bool| {
        let sandbox = Sandbox::new();
        sandbox.install_program("houserules", SCRIPTED_HOUSERULES);
        let evidence = sandbox.temp().join("houserules-evidence-root");
        fs::create_dir_all(&evidence).expect("create the evidence folder");
        fs::write(evidence.join("description.md"), description).expect("write the description");
        if audit {
            fs::write(evidence.join("audit.json"), "{}\n").expect("write the audit");
        }
        let summary = text(&sandbox.temp().join("summary.md"));
        let mut extra = vec![
            ("HOUSERULES_DIR", "."),
            ("HOUSERULES_KEY", "root"),
            ("HOUSERULES_BASE", "base-sha"),
            ("HOUSERULES_HEAD", "head-sha"),
            ("GITHUB_STEP_SUMMARY", summary.as_str()),
            UTF8_LOCALE,
        ];
        if reject {
            extra.push(("STUB_REJECT_INTENT", "1"));
        }
        let env = env_of(&unit, "gates", &extra);
        let output = sandbox.run_bash(&unit.step_script("gates", "Write the evidence packet"), env);
        (output, sandbox, evidence, summary)
    };
    for (description, intent) in intent_cases() {
        let (output, sandbox, evidence, summary) = run(&description, false, true);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{description:?}: {}{}",
            stdout(&output),
            stderr(&output)
        );
        let out = text(&evidence.join("evidence.md"));
        let json = text(&evidence.join("evidence.json"));
        let audit = text(&evidence.join("audit.json"));
        let expected = match intent {
            Some(intent) => format!(
                "evidence --base base-sha --head head-sha --dir . --audit {audit} --intent={intent} --out {out} --json {json}"
            ),
            None => format!(
                "evidence --base base-sha --head head-sha --dir . --audit {audit} --out {out} --json {json}"
            ),
        };
        assert_eq!(houserules_log(&sandbox), vec![expected], "{description:?}");
        assert_eq!(
            fs::read_to_string(summary).expect("the step summary"),
            "<!-- packet -->\n"
        );
    }
    // no audit result: the evidence call carries no --audit
    let (output, sandbox, evidence, _) = run("no section\n", false, false);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        houserules_log(&sandbox),
        vec![format!(
            "evidence --base base-sha --head head-sha --dir . --out {} --json {}",
            text(&evidence.join("evidence.md")),
            text(&evidence.join("evidence.json"))
        )]
    );
    // evidence refuses the intent: the second call carries none
    let (output, sandbox, _, _) = run(&intent_cases()[0].0, true, true);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let calls = houserules_log(&sandbox);
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls[0].contains("--intent=file:docs/plans/a.md"),
        "{calls:?}"
    );
    assert!(!calls[1].contains("--intent"), "{calls:?}");
}

/// Spec 10.1: every step of the GitHub gates job after the checks runs when an
/// earlier step failed, so that the packet and the route always appear.
#[test]
fn the_github_gates_steps_after_the_checks_run_when_an_earlier_step_failed() {
    let unit = github();
    for name in [
        "Write the request description",
        "Audit the request",
        "Check the evidence bundle",
        "Write the evidence packet",
        "Write the route",
        "Upload the evidence",
    ] {
        let condition = unit.step("gates", name)["if"]
            .as_str()
            .unwrap_or_else(|| panic!("the step {name} has a condition"))
            .to_string();
        assert!(
            condition.contains("!cancelled()")
                && condition.contains("github.event_name == 'pull_request'"),
            "{name}: {condition}"
        );
    }
}

/// Spec 10.1, 10.4: the GitLab gates script of a merge request reads the request
/// from the API into a file, checks out the source head, runs the checks, and
/// runs `evidence` and `route` even when a check failed; the job then fails.
#[test]
fn the_gitlab_gates_script_runs_evidence_and_route_after_a_failed_check() {
    let unit = gitlab("root");
    let run = |fail: &str, request: Option<&str>| {
        let sandbox = Sandbox::new();
        let remote = sandbox.path().join("remote").join("widgets.git");
        let repository = sandbox.work();
        let (first, second) = repository_with_origin(&repository, &remote);
        git(&repository, &["checkout", "-q", &first]);
        sandbox.install_program("houserules", SCRIPTED_HOUSERULES);
        match request {
            Some(body) => sandbox.reply("GET", "/api/v4/projects/5/merge_requests/7", body),
            None => sandbox.fail("GET", "/api/v4/projects/5/merge_requests/7"),
        }
        let env = env_of(
            &unit,
            "gates",
            &[
                ("HOUSERULES_DIR", "."),
                ("HOUSERULES_SOURCE", "none"),
                ("HOUSERULES_TRACKER", ""),
                ("HOUSERULES_SETUP", ""),
                ("CI_PIPELINE_SOURCE", "merge_request_event"),
                ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
                ("CI_MERGE_REQUEST_PROJECT_ID", "5"),
                ("CI_MERGE_REQUEST_IID", "7"),
                ("CI_MERGE_REQUEST_DIFF_BASE_SHA", first.as_str()),
                ("CI_MERGE_REQUEST_SOURCE_BRANCH_SHA", second.as_str()),
                ("CI_MERGE_REQUEST_SOURCE_BRANCH_NAME", "feature/x"),
                ("CI_COMMIT_SHA", "cccccccccccccccccccccccccccccccccccccccc"),
                ("CI_JOB_TOKEN", "job-token"),
                ("STUB_FAIL", fail),
            ],
        );
        let output = sandbox.run_sh(&unit.job_script("gates"), env);
        (output, sandbox, first, second)
    };
    let request = json!({
        "description": "## Evidence bundle\r\nPurpose: x\r\n```houserules-run\r\n{}\r\n```\r\nIntent: file:docs/plans/a.md\r\n",
        "author": {"username": "amy"}
    })
    .to_string();

    let (output, sandbox, first, second) = run("", Some(&request));
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let out = "houserules-evidence-root";
    let range = format!("--base {first} --head {second}");
    assert_eq!(
        houserules_log(&sandbox),
        vec![
            "check-knowledge --dir .".to_string(),
            "check-backlog --dir .".to_string(),
            format!("audit {range} --json {out}/audit.json --dir ."),
            format!(
                "bundle check --body {out}/description.md {range} --author=amy --head-ref=feature/x --dir ."
            ),
            format!(
                "evidence {range} --dir . --audit {out}/audit.json --intent=file:docs/plans/a.md --out {out}/evidence.md --json {out}/evidence.json"
            ),
            format!("route {range} --dir . --audit {out}/audit.json --json {out}/route.json"),
        ]
    );
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        calls[0].contains("--header JOB-TOKEN: job-token"),
        "{calls:?}"
    );
    assert_eq!(
        git(&sandbox.work(), &["rev-parse", "HEAD"]).trim(),
        second,
        "the job checks out the source head, not the merge commit"
    );

    // a failed check does not stop the packet and the route; the job still fails
    for failing in ["check-backlog", "audit", "bundle check"] {
        let (output, sandbox, _, _) = run(failing, Some(&request));
        assert_eq!(output.status.code(), Some(1), "{failing}");
        let calls = houserules_log(&sandbox);
        assert!(
            calls.iter().any(|call| call.starts_with("evidence ")),
            "{failing}: {calls:?}"
        );
        assert!(
            calls.iter().any(|call| call.starts_with("route ")),
            "{failing}: {calls:?}"
        );
    }

    // a request that cannot be read fails the job and still gives the packet
    let (output, sandbox, _, _) = run("", None);
    assert_eq!(output.status.code(), Some(1));
    let calls = houserules_log(&sandbox);
    assert!(
        calls
            .iter()
            .any(|call| call.starts_with("bundle check ") && call.contains("--author= ")),
        "{calls:?}"
    );
    assert!(
        calls.iter().any(|call| call.starts_with("evidence ")),
        "{calls:?}"
    );
    assert!(
        calls.iter().any(|call| call.starts_with("route ")),
        "{calls:?}"
    );
}

/// Spec 10.1: every step of the GitLab gates job, the setup command included, runs at the source
/// head of the request, never at the temporary merge commit that `CI_COMMIT_SHA` names in a merged
/// results pipeline. The setup command writes the head it sees; the same script with the checkout
/// removed shows a different head, so the check can fail.
#[test]
fn the_gitlab_setup_command_runs_at_the_source_head() {
    let unit = gitlab("root");
    let head_seen_by_setup = |script: &str| {
        let sandbox = Sandbox::new();
        let remote = sandbox.path().join("remote").join("widgets.git");
        let repository = sandbox.work();
        let (first, second) = repository_with_origin(&repository, &remote);
        git(&repository, &["checkout", "-q", &first]);
        sandbox.install_program("houserules", SCRIPTED_HOUSERULES);
        sandbox.reply(
            "GET",
            "/api/v4/projects/5/merge_requests/7",
            &json!({"description": "", "author": {"username": "amy"}}).to_string(),
        );
        let env = env_of(
            &unit,
            "gates",
            &[
                ("HOUSERULES_DIR", "."),
                ("HOUSERULES_SOURCE", "none"),
                ("HOUSERULES_TRACKER", ""),
                ("HOUSERULES_SETUP", "git rev-parse HEAD > setup-head.txt"),
                ("CI_PIPELINE_SOURCE", "merge_request_event"),
                ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
                ("CI_MERGE_REQUEST_PROJECT_ID", "5"),
                ("CI_MERGE_REQUEST_IID", "7"),
                ("CI_MERGE_REQUEST_DIFF_BASE_SHA", first.as_str()),
                ("CI_MERGE_REQUEST_SOURCE_BRANCH_SHA", second.as_str()),
                ("CI_MERGE_REQUEST_SOURCE_BRANCH_NAME", "feature/x"),
                ("CI_COMMIT_SHA", first.as_str()),
                ("CI_JOB_TOKEN", "job-token"),
            ],
        );
        let output = sandbox.run_sh(script, env);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}{}",
            stdout(&output),
            stderr(&output)
        );
        let seen = fs::read_to_string(sandbox.work().join("setup-head.txt")).expect("the head");
        (seen.trim().to_string(), first, second)
    };
    let script = unit.job_script("gates");
    let (seen, _, second) = head_seen_by_setup(&script);
    assert_eq!(seen, second, "the setup command sees the source head");

    let checkout = "git checkout -q --detach \"$head_sha\"";
    assert!(
        script.contains(checkout),
        "the script checks out the source head"
    );
    let without = script.replace(checkout, ": # no checkout");
    let (seen, first, second) = head_seen_by_setup(&without);
    assert_eq!(
        seen, first,
        "without the checkout the setup command sees the merge commit"
    );
    assert_ne!(seen, second);
}
