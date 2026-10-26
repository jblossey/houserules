//! The push of the record: the GitHub job against a local bare remote, the GitLab job with its reviewers program.

use super::*;

/// The notes of the bare repository `remote` under `reference`, or `None` when the
/// reference does not exist.
fn remote_notes(remote: &Path, reference: &str) -> Option<Vec<(String, String)>> {
    let exists = Command::new("git")
        .args([
            "--git-dir",
            &text(remote),
            "show-ref",
            "--verify",
            "--quiet",
        ])
        .arg(reference)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .status()
        .expect("run git show-ref");
    if !exists.success() {
        return None;
    }
    let listing = Command::new("git")
        .args([
            "--git-dir",
            &text(remote),
            "notes",
            &format!("--ref={reference}"),
            "list",
        ])
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .expect("run git notes");
    if !listing.status.success() {
        return None;
    }
    let mut notes = Vec::new();
    for line in String::from_utf8_lossy(&listing.stdout).lines() {
        let (blob, commit) = line.split_once(' ').expect("a notes line");
        let content = git(
            remote,
            &["--git-dir", &text(remote), "cat-file", "-p", blob],
        );
        notes.push((commit.to_string(), content));
    }
    notes.sort();
    Some(notes)
}

/// Runs the push script of the GitHub unit against the local remote
/// `remote`/`widgets.git`, for the key commit `key`, the install `dir`, and the
/// extra environment `extra`.
fn github_push(sandbox: &Sandbox, key: &str, dir: &str, extra: &[(&str, &str)]) -> Output {
    let unit = github();
    let server = format!("file://{}", text(&sandbox.path().join("remote")));
    let mut pairs = vec![
        ("GH_TOKEN", "ghs_secret"),
        ("GITHUB_SERVER_URL", server.as_str()),
        ("GITHUB_REPOSITORY", "widgets.git"),
        ("GITHUB_SHA", key),
        ("HOUSERULES_DIR", dir),
        ("HOUSERULES_KEY", "root"),
    ];
    pairs.extend_from_slice(extra);
    sandbox.run_bash(
        &unit.step_script("record-push", "Push the record"),
        env_of(&unit, "record-push", &pairs),
    )
}

/// The base64 form of `text`, as the `base64` tool of the runner prints it.
fn base64_of(text: &str) -> String {
    use std::io::Write;
    use std::process::Stdio;
    let mut child = Command::new("base64")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run base64");
    child
        .stdin
        .take()
        .expect("a pipe")
        .write_all(text.as_bytes())
        .expect("write to base64");
    let output = child.wait_with_output().expect("wait for base64");
    String::from_utf8(output.stdout)
        .expect("UTF-8")
        .trim()
        .to_string()
}

/// Writes the record that the `record` job uploaded.
fn put_record(root: &Path, key: &str, record: &str) {
    let directory = root.join(format!("houserules-record-{key}"));
    fs::create_dir_all(&directory).expect("create the record folder");
    fs::write(directory.join("record.json"), record).expect("write the record");
}

/// The record of the tests.
const RECORD: &str = "{\"schema\":\"houserules/change-record@1.0.0\",\"key\":\"k\"}\n";

/// Spec 10.1: `record-push` pushes the note on the key under the install's
/// ref, masks the credential, and never prints it.
#[test]
fn the_github_push_adds_the_note_under_the_ref_of_the_install_and_masks_the_credential() {
    for (dir, reference) in [
        (".", "refs/notes/houserules"),
        ("./", "refs/notes/houserules"),
        ("docs", "refs/notes/houserules-sub/docs"),
        ("sub/dir/", "refs/notes/houserules-sub/sub/dir"),
    ] {
        let sandbox = Sandbox::new();
        let remote = sandbox.path().join("remote").join("widgets.git");
        let (_, key) = repository_with_origin(&sandbox.path().join("repository"), &remote);
        put_record(&sandbox.temp(), "root", RECORD);
        let output = github_push(&sandbox, &key, dir, &[]);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{dir}: {}{}",
            stdout(&output),
            stderr(&output)
        );
        assert_eq!(
            remote_notes(&remote, reference),
            Some(vec![(key.clone(), RECORD.to_string())]),
            "{dir}"
        );
        let printed = format!("{}{}", stdout(&output), stderr(&output));
        assert!(
            !printed.contains("ghs_secret"),
            "the token is never printed: {printed}"
        );
        assert!(
            !stderr(&output).contains("ghs_secret"),
            "{dir}: {}",
            stderr(&output)
        );
        assert_eq!(
            stdout(&output),
            format!("::add-mask::{}\n", base64_of("x-access-token:ghs_secret")),
            "{dir}: the only output is the mask of the base64 form"
        );
    }
}

/// Spec 10.1: a re-run of the same push never removes the note that the remote
/// holds (a `post_merge_audit` may have been added): it prints one notice and
/// keeps the remote note.
#[test]
fn a_rerun_of_the_push_keeps_the_note_of_the_remote() {
    let sandbox = Sandbox::new();
    let remote = sandbox.path().join("remote").join("widgets.git");
    let (_, key) = repository_with_origin(&sandbox.path().join("repository"), &remote);
    put_record(&sandbox.temp(), "root", RECORD);
    let first = github_push(&sandbox, &key, ".", &[]);
    assert_eq!(
        first.status.code(),
        Some(0),
        "{}{}",
        stdout(&first),
        stderr(&first)
    );
    // another process audits the record on the remote
    let audited = "{\"schema\":\"houserules/change-record@1.0.0\",\"post_merge_audit\":{\"verdict\":\"pass\"}}\n";
    let racer = sandbox.path().join("auditor");
    git(
        sandbox.path(),
        &["clone", "-q", &text(&remote), &text(&racer)],
    );
    git(
        &racer,
        &[
            "fetch",
            "-q",
            "origin",
            "refs/notes/houserules:refs/notes/houserules",
        ],
    );
    git(
        &racer,
        &[
            "notes",
            "--ref=houserules",
            "add",
            "-f",
            "-m",
            audited.trim_end(),
            &key,
        ],
    );
    git(
        &racer,
        &[
            "push",
            "-q",
            "origin",
            "refs/notes/houserules:refs/notes/houserules",
        ],
    );
    let second = github_push(&sandbox, &key, ".", &[]);
    assert_eq!(
        second.status.code(),
        Some(0),
        "{}{}",
        stdout(&second),
        stderr(&second)
    );
    assert_eq!(
        stdout(&second).matches("::notice::").count(),
        1,
        "{}",
        stdout(&second)
    );
    assert!(
        stdout(&second).contains("already holds a record"),
        "{}",
        stdout(&second)
    );
    assert_eq!(
        remote_notes(&remote, "refs/notes/houserules"),
        Some(vec![(key.clone(), audited.to_string())])
    );
}

/// Spec 10.1: a second writer that wins the race makes the push fail once; the
/// next try fetches into a side ref, merges with `-s theirs`, and pushes, so the
/// remote holds both notes.
#[test]
fn a_second_writer_that_wins_the_race_costs_one_more_try_and_loses_no_note() {
    let sandbox = Sandbox::new();
    let remote = sandbox.path().join("remote").join("widgets.git");
    let (first, key) = repository_with_origin(&sandbox.path().join("repository"), &remote);
    put_record(&sandbox.temp(), "root", RECORD);
    // the second writer: its own repository with a note on `first`
    let racer = sandbox.path().join("racer");
    fs::create_dir_all(&racer).expect("create the racer");
    git(&racer, &["init", "-q"]);
    git(&racer, &["remote", "add", "origin", &text(&remote)]);
    git(&racer, &["fetch", "-q", "origin", "main"]);
    git(
        &racer,
        &[
            "notes",
            "--ref=houserules",
            "add",
            "-m",
            "second writer",
            &first,
        ],
    );
    let script = sandbox.path().join("race.sh");
    fs::write(
        &script,
        format!(
            "\"$REAL_GIT\" -C {} push -q origin refs/notes/houserules:refs/notes/houserules\n",
            text(&racer)
        ),
    )
    .expect("write the race script");
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git on the PATH");
    sandbox.install_program(
        "git",
        &fs::read_to_string(crate::ci_support::sandbox::fixture_dir().join("race-git"))
            .expect("read race-git"),
    );
    let flag = text(&sandbox.path().join("race.flag"));
    let script_path = text(&script);
    let real = text(&real_git);
    let output = github_push(
        &sandbox,
        &key,
        ".",
        &[
            ("REAL_GIT", real.as_str()),
            ("RACE_FLAG", flag.as_str()),
            ("RACE_SCRIPT", script_path.as_str()),
        ],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(
        sandbox.path().join("race.flag").exists(),
        "the second writer pushed first"
    );
    assert_eq!(
        remote_notes(&remote, "refs/notes/houserules"),
        Some({
            let mut notes = vec![
                (first.clone(), "second writer\n".to_string()),
                (key.clone(), RECORD.to_string()),
            ];
            notes.sort();
            notes
        })
    );
}

/// Spec 10.4: the GitLab push job sets the reviewers from the approvals with the
/// reviewers program, pushes the note with the token, and prints no credential.
/// Without the token it prints one notice and pushes nothing, and an iid that is
/// no number is never used.
#[test]
fn the_gitlab_push_sets_the_reviewers_and_pushes_the_note() {
    let unit = gitlab("root");
    let run = |token: &str, iid: &str, remote_exists: bool| {
        let sandbox = Sandbox::new();
        let remote = sandbox.path().join("remote").join("widgets.git");
        let (_, key) = repository_with_origin(&sandbox.path().join("repository"), &remote);
        let record = "{\"schema\":\"houserules/change-record@1.0.0\",\"reviewers\":[],\"reviewers_pending\":true}\n";
        put_record(&sandbox.work(), "root", record);
        fs::write(
            sandbox.work().join("houserules-record-root").join("iid"),
            format!("{iid}\n"),
        )
        .expect("write iid");
        sandbox.reply(
            "GET",
            "/api/v4/projects/5/merge_requests/7/approvals",
            &json!({"approved_by": [{"user": {"username": "zed"}}, {"user": {"username": "amy"}}, {"user": {"username": "zed"}}]}).to_string(),
        );
        let server = format!("file://{}", text(&sandbox.path().join("remote")));
        let env = env_of(
            &unit,
            "record-push",
            &[
                ("HOUSERULES_DIR", "."),
                ("HOUSERULES_TOKEN", token),
                ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
                ("CI_PROJECT_ID", "5"),
                ("CI_SERVER_URL", server.as_str()),
                ("CI_PROJECT_PATH", "widgets"),
                ("CI_COMMIT_SHA", key.as_str()),
                ("CI_ENVIRONMENT_NAME", "houserules-root"),
            ],
        );
        let output = sandbox.run_sh(&unit.job_script("record-push"), env);
        let notes = remote_notes(&remote, "refs/notes/houserules");
        let _ = remote_exists;
        (output, notes, key, sandbox.calls())
    };

    let (output, notes, key, calls) = run("glpat-secret", "7", true);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let notes = notes.expect("the note was pushed");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].0, key);
    let record: Value = serde_json::from_str(&notes[0].1).expect("the note is JSON");
    assert_eq!(record["reviewers"], json!(["amy", "zed"]));
    assert_eq!(record["reviewers_pending"], json!(false));
    assert!(record.get("reviewers_more").is_none());
    assert!(
        calls[0].contains("--header PRIVATE-TOKEN: glpat-secret"),
        "{calls:?}"
    );
    let printed = format!("{}{}", stdout(&output), stderr(&output));
    assert!(
        !printed.contains("glpat-secret"),
        "the token is never printed: {printed}"
    );

    // no token: one notice, nothing pushed
    let (output, notes, _, calls) = run("", "7", true);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(notes, None);
    assert!(calls.is_empty());
    assert_eq!(stdout(&output).lines().count(), 1, "{}", stdout(&output));
    assert!(
        stdout(&output).contains("HOUSERULES_TOKEN is not set for the environment houserules-root")
    );

    // an iid that is no number is not used: the record keeps reviewers_pending
    let (output, notes, _, calls) = run("glpat-secret", "7;id", true);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(calls.is_empty(), "no approvals call: {calls:?}");
    let record: Value = serde_json::from_str(&notes.expect("a note")[0].1).expect("JSON");
    assert_eq!(record["reviewers_pending"], json!(true));
}

/// Spec 10.1, design.md 259: after a force push the record job wrote no record, so the GitLab push
/// job prints one notice, pushes nothing, and exits 0. A token that is not set still gets its own
/// notice first.
#[test]
fn the_gitlab_push_without_a_record_prints_one_notice_and_pushes_nothing() {
    let unit = gitlab("root");
    let sandbox = Sandbox::new();
    let remote = sandbox.path().join("remote").join("widgets.git");
    let (_, key) = repository_with_origin(&sandbox.path().join("repository"), &remote);
    let server = format!("file://{}", text(&sandbox.path().join("remote")));
    let env = env_of(
        &unit,
        "record-push",
        &[
            ("HOUSERULES_DIR", "."),
            ("HOUSERULES_TOKEN", "glpat-secret"),
            ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
            ("CI_PROJECT_ID", "5"),
            ("CI_SERVER_URL", server.as_str()),
            ("CI_PROJECT_PATH", "widgets"),
            ("CI_COMMIT_SHA", key.as_str()),
            ("CI_ENVIRONMENT_NAME", "houserules-root"),
        ],
    );
    let output = sandbox.run_sh(&unit.job_script("record-push"), env);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        stdout(&output),
        "houserules: the record job wrote no record for this push, so this run pushes nothing\n"
    );
    assert_eq!(remote_notes(&remote, "refs/notes/houserules"), None);
    assert!(sandbox.calls().is_empty(), "{:?}", sandbox.calls());
}

/// Spec 10.4: the reviewers program sets the approvers sorted and unique, the
/// first 20, each cut to 100 characters, with no more names than keep the
/// serialized array within 3,072 bytes by the `utf8bytelength` of jq. It sets
/// `reviewers_more` to the count of the names left out, and removes it when no
/// name is left out. It sets `reviewers_pending` to `false`. This test pins the
/// cap of 20 names; `record.rs` pins the byte rule.
#[test]
fn the_reviewers_program_caps_sorts_and_marks_the_reviewers_set() {
    let unit = gitlab("root");
    let program = unit
        .variable("record-push", "HOUSERULES_REVIEWERS_JQ")
        .expect("the reviewers program");
    let run = |approvers: &Value, record: &Value| -> Value {
        let directory = tempfile::tempdir().expect("scratch");
        let input = directory.path().join("record.json");
        fs::write(&input, record.to_string()).expect("write the record");
        let output = Command::new("jq")
            .args([
                "-c",
                "--argjson",
                "approvers",
                &approvers.to_string(),
                &program,
            ])
            .arg(&input)
            .output()
            .expect("run jq");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON output")
    };
    let many: Vec<String> = (0..25).rev().map(|n| format!("user{n:02}")).collect();
    let mut with_duplicates = many.clone();
    with_duplicates.push("user03".to_string());
    let long = "x".repeat(150);
    with_duplicates.push(long.clone());
    let pending =
        json!({"schema": "s", "reviewers": [], "reviewers_pending": true, "truncated": []});
    let set = run(&json!(with_duplicates), &pending);
    let reviewers = set["reviewers"].as_array().expect("reviewers");
    assert_eq!(reviewers.len(), 20);
    let mut sorted: Vec<String> = many.clone();
    sorted.push(long.clone());
    sorted.sort();
    sorted.dedup();
    let expected: Vec<String> = sorted[..20]
        .iter()
        .map(|name| name.chars().take(100).collect())
        .collect();
    assert_eq!(
        reviewers
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(set["reviewers_more"], json!(sorted.len() - 20));
    assert_eq!(set["reviewers_pending"], json!(false));
    assert_eq!(set["truncated"], json!([]), "every other key stays");
    assert!(
        reviewers
            .iter()
            .all(|name| name.as_str().unwrap().chars().count() <= 100)
    );

    // 20 or fewer: no reviewers_more, even when the record held one
    let stale = json!({"reviewers": [], "reviewers_more": 3, "reviewers_pending": true});
    let set = run(&json!(["bob", "amy"]), &stale);
    assert_eq!(set["reviewers"], json!(["amy", "bob"]));
    assert!(set.get("reviewers_more").is_none());
    assert_eq!(set["reviewers_pending"], json!(false));
    let set = run(&json!([]), &stale);
    assert_eq!(set["reviewers"], json!([]));
    let twenty: Vec<String> = (0..20).map(|n| format!("u{n:02}")).collect();
    let set = run(&json!(twenty), &stale);
    assert_eq!(set["reviewers"].as_array().unwrap().len(), 20);
    assert!(set.get("reviewers_more").is_none());
}
