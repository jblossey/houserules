//! The record jobs of both units, run for real in a repository with a stub `houserules`, and the notes ref of an install.

use super::*;

/// What a run of a `record` job left: the output of the script, the calls of
/// the stub `houserules`, the sandbox, the folder of the record files, the key
/// commit, and the commit before it.
struct RecordRun {
    /// The output of the script.
    output: Output,
    /// The calls of the stub `houserules`, one line each.
    log: Vec<String>,
    /// The sandbox, for its call log.
    sandbox: Sandbox,
    /// The folder that holds the files of the record: `record.json` and, on
    /// GitLab, the request number.
    folder: PathBuf,
    /// The key commit: the second commit of the repository.
    key: String,
    /// The commit before the key commit.
    first: String,
}

/// A program that records its arguments and writes `{}` to the file of each
/// `--json` and `--out`, as `audit --json` and `record write --out` do.
const RECORDING_HOUSERULES: &str = "#!/bin/sh\necho \"$*\" >> \"$STUB_DIR/houserules.log\"\n\
     while [ $# -gt 0 ]; do case $1 in --json|--out) printf '{}\\n' > \"$2\"; shift 2;; *) shift;; esac; done\n";

/// Sets up a real repository in the work directory of a sandbox, with a bare
/// `origin`, two commits, the stub `houserules`, and, with `with_notes_ref`, a
/// notes ref on the remote.
fn record_sandbox(with_notes_ref: bool) -> (Sandbox, String, String) {
    let sandbox = Sandbox::new();
    let remote = sandbox.path().join("remote").join("widgets.git");
    let repository = sandbox.work();
    let (first, second) = repository_with_origin(&repository, &remote);
    if with_notes_ref {
        git(
            &repository,
            &["notes", "--ref=houserules", "add", "-m", "old", &first],
        );
        git(
            &repository,
            &[
                "push",
                "-q",
                "origin",
                "refs/notes/houserules:refs/notes/houserules",
            ],
        );
        git(&repository, &["update-ref", "-d", "refs/notes/houserules"]);
    }
    sandbox.install_program("houserules", RECORDING_HOUSERULES);
    (sandbox, first, second)
}

/// The path of request 12 on the stub host.
const GITHUB_REQUEST_PATH: &str = "/repos/acme/widgets/pulls/12";

/// The path of the reviews of request 12 on the stub host.
const GITHUB_REVIEWS_PATH: &str = "/repos/acme/widgets/pulls/12/reviews";

/// Makes the stub host answer the reads of the GitHub `record` job: the list of
/// requests of `key` with `pulls`, or a failing call for `None`, the reviews of
/// request 12 with `reviews`, and request 12 with a description. Each path of
/// `failing` fails in place of its answer.
fn stub_github_host(
    sandbox: &Sandbox,
    key: &str,
    pulls: Option<Value>,
    reviews: &Value,
    failing: &[&str],
) {
    let path = format!("/repos/acme/widgets/commits/{key}/pulls");
    match pulls {
        Some(reply) => sandbox.reply("GET", &path, &reply.to_string()),
        None => sandbox.fail("GET", &path),
    }
    sandbox.reply("GET", GITHUB_REVIEWS_PATH, &reviews.to_string());
    sandbox.reply(
        "GET",
        GITHUB_REQUEST_PATH,
        &json!({"body": "## Evidence bundle\n"}).to_string(),
    );
    for path in failing {
        sandbox.fail("GET", path);
    }
}

/// Runs the `Write the record` step of the GitHub unit in `sandbox`, whose key
/// commit is `key` and whose first commit is `first`, for a push whose commit
/// before is `before` (`FIRST` stands for the first commit).
fn run_github_record(sandbox: Sandbox, first: String, key: String, before: &str) -> RecordRun {
    let unit = github();
    let server = format!("file://{}", text(&sandbox.path().join("remote")));
    let output_file = text(&sandbox.temp().join("github_output"));
    let before = if before == "FIRST" {
        first.as_str()
    } else {
        before
    };
    let env = env_of(
        &unit,
        "record",
        &[
            ("GH_TOKEN", "ghs_read"),
            ("GITHUB_SERVER_URL", server.as_str()),
            ("GITHUB_REPOSITORY", "acme/widgets"),
            ("GITHUB_SHA", key.as_str()),
            ("HOUSERULES_DIR", "."),
            ("HOUSERULES_KEY", "root"),
            ("HOUSERULES_BEFORE", before),
            ("GITHUB_OUTPUT", output_file.as_str()),
        ],
    );
    let output = sandbox.run_bash(&unit.step_script("record", "Write the record"), env);
    let log = houserules_log(&sandbox);
    let folder = sandbox.temp().join("houserules-record-root");
    RecordRun {
        output,
        log,
        sandbox,
        folder,
        key,
        first,
    }
}

/// Runs the `Write the record` step of the GitHub unit for a push whose commit
/// before is `before` (`FIRST` stands for the first commit of the repository).
/// `pulls` gives the reply for the list of requests of the key commit, or `None`
/// for a failing call; `reviews` is the reply for the reviews of request 12.
fn github_record(
    before: &str,
    pulls: impl Fn(&str) -> Option<Value>,
    reviews: Value,
    with_notes_ref: bool,
) -> RecordRun {
    let (sandbox, first, second) = record_sandbox(with_notes_ref);
    stub_github_host(&sandbox, &second, pulls(&second), &reviews, &[]);
    run_github_record(sandbox, first, second, before)
}

/// The list of requests of a commit: an open request 13, a merged request 14 of
/// another commit, and the merged request 12 of `key`.
fn pulls_of(key: &str) -> Option<Value> {
    Some(json!([
        {"number": 13, "merged_at": null, "merge_commit_sha": null},
        {"number": 14, "merged_at": "2026-01-01T00:00:00Z", "merge_commit_sha": "0000000000000000000000000000000000000001"},
        {"number": 12, "merged_at": "2026-01-02T00:00:00Z", "merge_commit_sha": key}
    ]))
}

/// The reviews of request 12: one approval kept, one dismissed by a later
/// request for changes, a comment that does not dismiss an approval, a dismissed
/// review, and a deleted account.
fn reviews_of_request_12() -> Value {
    json!([
        {"state": "APPROVED", "user": {"login": "amy"}, "submitted_at": "2026-01-01T10:00:00Z"},
        {"state": "COMMENTED", "user": {"login": "amy"}, "submitted_at": "2026-01-02T10:00:00Z"},
        {"state": "APPROVED", "user": {"login": "bob"}, "submitted_at": "2026-01-01T11:00:00Z"},
        {"state": "CHANGES_REQUESTED", "user": {"login": "bob"}, "submitted_at": "2026-01-02T11:00:00Z"},
        {"state": "APPROVED", "user": {"login": "cy"}, "submitted_at": "2026-01-01T12:00:00Z"},
        {"state": "DISMISSED", "user": {"login": "dee"}, "submitted_at": "2026-01-01T13:00:00Z"},
        {"state": "APPROVED", "user": null, "submitted_at": "2026-01-01T14:00:00Z"}
    ])
}

/// The empty tree that a root push uses as its base.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Spec 10.1: the `record` job audits the push range, finds the merged request of
/// the commit among the requests it is listed under, passes its description and
/// its approvers (the last review of each account counts, a comment does not
/// dismiss an approval), and writes the record with `--host github`. It fetches
/// the notes ref first.
#[test]
fn the_github_record_job_passes_the_audit_the_bundle_and_the_approvers() {
    let run = github_record("FIRST", pulls_of, reviews_of_request_12(), true);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    let key = &run.key;
    assert_eq!(run.log.len(), 2, "{:?}", run.log);
    assert!(
        run.log[0].starts_with(&format!("audit --base {} --head {key} --json ", run.first)),
        "{:?}",
        run.log
    );
    let record = &run.log[1];
    assert!(
        record.starts_with(&format!(
            "record write --base {} --head {key} --key {key} --host github --out ",
            run.first
        )),
        "{record}"
    );
    assert!(
        record.contains(" --audit ") && record.contains(" --bundle "),
        "{record}"
    );
    assert!(record.ends_with("--reviewer=amy --reviewer=cy"), "{record}");
    let calls = run.sandbox.calls();
    assert!(
        calls.iter().any(|call| call.contains("pulls/12 --jq")),
        "{calls:?}"
    );
    assert!(
        !calls
            .iter()
            .any(|call| call.contains("pulls/13") || call.contains("pulls/14")),
        "{calls:?}"
    );
    let reference = git(
        &run.sandbox.work(),
        &["rev-parse", "--verify", "refs/notes/houserules"],
    );
    assert_eq!(reference.trim().len(), 40, "the notes ref was fetched");
    assert_eq!(
        fs::read_to_string(run.sandbox.temp().join("github_output")).expect("the step output"),
        "written=true\n",
        "the step reports the record it wrote"
    );
}

/// Spec 10.1: a push whose `before` is all zeros or empty uses git's empty tree
/// as the base and runs no audit; `record write` audits the range in process.
#[test]
fn the_github_record_job_audits_a_root_push_in_process() {
    for before in ["0000000000000000000000000000000000000000", ""] {
        let run = github_record(before, pulls_of, reviews_of_request_12(), false);
        assert_eq!(
            run.output.status.code(),
            Some(0),
            "{before:?}: {}{}",
            stdout(&run.output),
            stderr(&run.output)
        );
        assert_eq!(run.log.len(), 1, "{before:?}: no audit: {:?}", run.log);
        assert!(
            run.log[0].starts_with(&format!(
                "record write --base {EMPTY_TREE} --head {} ",
                run.key
            )),
            "{before:?}: {:?}",
            run.log
        );
        assert!(
            !run.log[0].contains(" --audit "),
            "{before:?}: {:?}",
            run.log
        );
        assert!(
            run.log[0].contains(" --bundle "),
            "{before:?}: the request is still found"
        );
        assert!(
            !stdout(&run.output).contains("::notice::"),
            "{before:?}: a root push needs no notice: {}",
            stdout(&run.output)
        );
    }
}

/// The notice of spec 10.1 for a `before` that is not in the clone.
const FORCE_PUSH_NOTICE: &str =
    "houserules: ddddddd is not in this clone (a force push?); no record for this push";

/// The `before` of the force push tests: a commit that no clone holds.
const MISSING_BEFORE: &str = "dddddddddddddddddddddddddddddddddddddddd";

/// Spec 10.1, design.md 259: a push whose `before` commit is not in the clone (a force push)
/// writes no record: the job prints the notice and exits 0, runs no audit and no `record write`,
/// calls no host API, and reports no record to the push job.
#[test]
fn a_force_push_writes_no_record_on_github() {
    let run = github_record(MISSING_BEFORE, pulls_of, reviews_of_request_12(), false);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    let printed = stdout(&run.output);
    let lines: Vec<&str> = printed
        .lines()
        .filter(|line| !line.starts_with("::add-mask::"))
        .collect();
    assert_eq!(
        lines,
        vec![format!("::notice::{FORCE_PUSH_NOTICE}")],
        "the job prints the notice and nothing else but the mask"
    );
    assert!(run.log.is_empty(), "no audit and no record: {:?}", run.log);
    assert!(
        run.sandbox.calls().is_empty(),
        "no host call: {:?}",
        run.sandbox.calls()
    );
    assert!(
        !run.sandbox.temp().join("github_output").exists(),
        "the step reports no record"
    );
}

/// Spec 10.1, design.md 259: the same on GitLab.
#[test]
fn a_force_push_writes_no_record_on_gitlab() {
    let run = gitlab_record(MISSING_BEFORE, empty_listing);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    assert_eq!(stdout(&run.output), format!("{FORCE_PUSH_NOTICE}\n"));
    assert!(run.log.is_empty(), "no audit and no record: {:?}", run.log);
    assert!(
        run.sandbox.calls().is_empty(),
        "no host call: {:?}",
        run.sandbox.calls()
    );
    assert!(
        !run.sandbox
            .work()
            .join("houserules-record-root")
            .join("record.json")
            .exists(),
        "no record file"
    );
}

/// Spec 10.1: on GitHub the push job runs only when the record job reported a record, and the
/// upload of the record waits for the same report, so that a force push ends with no failed job.
#[test]
fn the_github_push_job_waits_for_a_record_that_the_record_job_reported() {
    let unit = github();
    let record = unit.job("record");
    assert_eq!(
        record["outputs"]["written"],
        json!("${{ steps.write.outputs.written }}"),
        "the record job reports the record"
    );
    assert_eq!(
        unit.step("record", "Write the record")["id"],
        json!("write")
    );
    assert_eq!(
        unit.step("record", "Upload the record")["if"],
        json!("${{ steps.write.outputs.written == 'true' }}")
    );
    assert_eq!(
        unit.job("record-push")["if"],
        json!("${{ needs.record.outputs.written == 'true' }}")
    );
}

/// The list of requests of a commit that holds no merged request: an open
/// request 13 and a request 14 that is not merged.
fn others_pulls(_key: &str) -> Option<Value> {
    Some(json!([
        {"number": 13, "merged_at": null, "merge_commit_sha": null},
        {"number": 14, "merged_at": null, "merge_commit_sha": "0000000000000000000000000000000000000001"}
    ]))
}

/// The request list of the key commit on GitLab: an open request, a request
/// merged as another commit, and request 7 merged as `key`.
fn merged_listing(key: &str) -> Option<Value> {
    Some(json!([
        {"iid": 8, "state": "opened", "merge_commit_sha": null, "sha": "x"},
        {"iid": 9, "state": "merged", "merge_commit_sha": "0000000000000000000000000000000000000001", "sha": "y"},
        {"iid": 7, "state": "merged", "merge_commit_sha": key, "sha": "z"}
    ]))
}

/// A request list that holds an open and a closed request and no merged request.
fn unmerged_listing(_key: &str) -> Option<Value> {
    Some(json!([
        {"iid": 8, "state": "opened", "merge_commit_sha": null, "sha": "x"},
        {"iid": 10, "state": "closed", "merge_commit_sha": null, "sha": "w"}
    ]))
}

/// An empty request list.
fn empty_listing(_key: &str) -> Option<Value> {
    Some(json!([]))
}

/// A request list call that fails.
fn failing_listing(_key: &str) -> Option<Value> {
    None
}

/// Makes the stub host answer the list of requests of `key` with `listing`, or
/// fail the call for `None`, and give request 7 a description.
fn stub_gitlab_host(sandbox: &Sandbox, key: &str, listing: Option<Value>) {
    let path = format!("/api/v4/projects/5/repository/commits/{key}/merge_requests");
    match listing {
        Some(reply) => sandbox.reply("GET", &path, &reply.to_string()),
        None => sandbox.fail("GET", &path),
    }
    sandbox.reply(
        "GET",
        "/api/v4/projects/5/merge_requests/7",
        &json!({"description": "## Evidence bundle\n"}).to_string(),
    );
}

/// Runs the script of the GitLab `record` job in `sandbox`, whose key commit is
/// `key` and whose first commit is `first`, for a push whose commit before is
/// `before` (`FIRST` stands for the first commit).
fn run_gitlab_record(sandbox: Sandbox, first: String, key: String, before: &str) -> RecordRun {
    let unit = gitlab("root");
    let before = if before == "FIRST" {
        first.as_str()
    } else {
        before
    };
    let env = env_of(
        &unit,
        "record",
        &[
            ("HOUSERULES_DIR", "."),
            ("HOUSERULES_SOURCE", "none"),
            ("HOUSERULES_TRACKER", ""),
            ("HOUSERULES_SETUP", ""),
            ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
            ("CI_PROJECT_ID", "5"),
            ("CI_JOB_TOKEN", "job-token"),
            ("CI_COMMIT_SHA", key.as_str()),
            ("CI_COMMIT_BEFORE_SHA", before),
        ],
    );
    let output = sandbox.run_sh(&unit.job_script("record"), env);
    let log = houserules_log(&sandbox);
    let folder = sandbox.work().join("houserules-record-root");
    RecordRun {
        output,
        log,
        sandbox,
        folder,
        key,
        first,
    }
}

/// Runs the script of the GitLab `record` job with the stub `houserules`, for a
/// push whose commit before is `before` (`FIRST` stands for the first commit).
/// `listing` gives the reply to the list of requests of the key commit, or `None`
/// for a failing call.
fn gitlab_record(before: &str, listing: impl Fn(&str) -> Option<Value>) -> RecordRun {
    let (sandbox, first, second) = record_sandbox(true);
    stub_gitlab_host(&sandbox, &second, listing(&second));
    run_gitlab_record(sandbox, first, second, before)
}

/// A program that logs its arguments like the stub `houserules` and then runs
/// the `houserules` binary under test.
fn forwarding_houserules() -> String {
    format!(
        "#!/bin/sh\necho \"$*\" >> \"$STUB_DIR/houserules.log\"\nexec '{}' \"$@\"\n",
        env!("CARGO_BIN_EXE_houserules")
    )
}

/// Commits a houserules install on top of the repository in `directory`, pushes
/// it to `origin`, and returns the new commit.
fn commit_install(directory: &Path) -> String {
    let files = [
        (
            ".houserules.json",
            "{\"schema\":\"houserules/install@2.3.0\",\"idPrefix\":\"WI\"}\n",
        ),
        ("knowledge/schema.json", "{}\n"),
        ("knowledge/areas.json", "{\"global\":{\"paths\":[]}}\n"),
        ("knowledge/team.json", "{\"entries\":[]}\n"),
    ];
    for (path, content) in files {
        let target = directory.join(path);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write an install file");
    }
    git(directory, &["add", "."]);
    git(directory, &["commit", "-q", "-m", "install houserules"]);
    git(directory, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
    git(directory, &["rev-parse", "HEAD"]).trim().to_string()
}

/// Runs the script of the GitLab `record` job with the `houserules` binary under
/// test, in a repository whose key commit adds an install, for a push whose
/// commit before is the first commit. `listing` is as in `gitlab_record`.
fn gitlab_record_with_the_binary(listing: impl Fn(&str) -> Option<Value>) -> RecordRun {
    let (sandbox, first, _) = record_sandbox(true);
    sandbox.install_program("houserules", &forwarding_houserules());
    let key = commit_install(&sandbox.work());
    stub_gitlab_host(&sandbox, &key, listing(&key));
    run_gitlab_record(sandbox, first, key, "FIRST")
}

/// The `record write` call of a run: the last line of its log.
fn record_write_call(run: &RecordRun) -> &str {
    let call = run.log.last().expect("a record write call");
    assert!(call.starts_with("record write "), "{:?}", run.log);
    call
}

/// Whether `call` passes `--reviewers-pending` as an argument.
fn passes_pending_flag(call: &str) -> bool {
    call.split(' ')
        .any(|argument| argument == "--reviewers-pending")
}

/// The note that the `record` job of the run wrote.
fn written_note(run: &RecordRun) -> Value {
    let path = run.folder.join("record.json");
    serde_json::from_str(&fs::read_to_string(path).expect("the note")).expect("a JSON note")
}

/// Whether the job kept the number of the merged request for the push job.
fn kept_request_number(run: &RecordRun) -> bool {
    run.folder.join("iid").exists()
}

/// Spec 10.4: the GitLab `record` job lists the merged requests of the commit
/// with the job token, passes the description, writes the record with
/// `--reviewers-pending`, and keeps the request's number for the push job; a root
/// push runs no audit; a push with no merged request leaves no number.
#[test]
fn the_gitlab_record_job_writes_the_record_with_pending_reviewers() {
    let run = gitlab_record("FIRST", merged_listing);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    assert_eq!(run.log.len(), 2, "{:?}", run.log);
    assert!(
        run.log[0].starts_with(&format!(
            "audit --base {} --head {} --json ",
            run.first, run.key
        )),
        "{:?}",
        run.log
    );
    let record = &run.log[1];
    assert!(
        record.starts_with(&format!(
            "record write --base {} --head {} --key {} --host gitlab --out ",
            run.first, run.key, run.key
        )),
        "{record}"
    );
    assert!(
        record.contains(" --audit ")
            && record.contains(" --reviewers-pending ")
            && record.ends_with(" --bundle houserules-record-root/description.md"),
        "{record}"
    );
    assert_eq!(
        fs::read_to_string(
            run.sandbox
                .work()
                .join("houserules-record-root")
                .join("iid")
        )
        .expect("the iid"),
        "7\n"
    );
    let calls = run.sandbox.calls();
    assert!(
        calls
            .iter()
            .all(|call| call.contains("JOB-TOKEN: job-token")),
        "reads use the job token: {calls:?}"
    );

    let root = gitlab_record("0000000000000000000000000000000000000000", merged_listing);
    assert_eq!(
        root.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&root.output),
        stderr(&root.output)
    );
    assert_eq!(root.log.len(), 1, "no audit on a root push: {:?}", root.log);
    assert!(
        root.log[0].starts_with(&format!("record write --base {EMPTY_TREE} ")),
        "{:?}",
        root.log
    );

    let none = gitlab_record("FIRST", empty_listing);
    assert_eq!(
        none.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&none.output),
        stderr(&none.output)
    );
    let record = none.log.last().expect("a record write call");
    assert!(!record.contains("--bundle"), "{record}");
    assert!(!kept_request_number(&none));
}

/// Spec 10.4: a push whose list of requests holds no merged request has no
/// approvers, so the `record` job writes its note without `--reviewers-pending`:
/// the note holds `reviewers: []` and `reviewers_pending: false`, no bundle, and
/// no request number, and `record-push` has no approvals to read for it. An empty
/// list and a list of open and closed requests alike.
#[test]
fn a_gitlab_push_with_no_merged_request_writes_a_final_note() {
    for listing in [empty_listing, unmerged_listing] {
        let run = gitlab_record_with_the_binary(listing);
        assert_eq!(
            run.output.status.code(),
            Some(0),
            "{}{}",
            stdout(&run.output),
            stderr(&run.output)
        );
        let call = record_write_call(&run);
        assert!(!passes_pending_flag(call), "{call}");
        assert!(!call.contains("--bundle"), "{call}");
        assert!(!kept_request_number(&run));
        let note = written_note(&run);
        assert_eq!(note["host"], json!("gitlab"));
        assert_eq!(note["reviewers"], json!([]));
        assert_eq!(note["reviewers_pending"], json!(false));
        assert_eq!(note["bundle"], Value::Null);
    }
}

/// Spec 10.4: a push whose list of requests holds a merged request keeps
/// `--reviewers-pending`: the approvers are unknown until `record-push` reads
/// them, and the job keeps the request's number and its description.
#[test]
fn a_gitlab_push_with_a_merged_request_keeps_the_pending_flag() {
    let run = gitlab_record_with_the_binary(merged_listing);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    let call = record_write_call(&run);
    assert!(passes_pending_flag(call), "{call}");
    assert!(call.contains(" --bundle "), "{call}");
    assert!(kept_request_number(&run));
    let note = written_note(&run);
    assert_eq!(note["reviewers"], json!([]));
    assert_eq!(note["reviewers_pending"], json!(true));
}

/// Spec 10.4: a list of requests that fails keeps `--reviewers-pending`, because
/// the approvals are unknown; the job finds no request and writes the note with no
/// bundle and no request number.
#[test]
fn a_gitlab_push_whose_listing_fails_keeps_the_pending_flag() {
    let run = gitlab_record_with_the_binary(failing_listing);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    let call = record_write_call(&run);
    assert!(passes_pending_flag(call), "{call}");
    assert!(!call.contains("--bundle"), "{call}");
    assert!(!kept_request_number(&run));
    let note = written_note(&run);
    assert_eq!(note["reviewers"], json!([]));
    assert_eq!(note["reviewers_pending"], json!(true));
}

/// Runs the `Write the record` step of the GitHub unit with the `houserules`
/// binary under test, in a repository whose key commit adds an install, for a push
/// whose commit before is the first commit. `pulls` and `reviews` are as in
/// `github_record`, and each path of `failing` fails.
fn github_record_with_the_binary(
    pulls: impl Fn(&str) -> Option<Value>,
    reviews: Value,
    failing: &[&str],
) -> RecordRun {
    let (sandbox, first, _) = record_sandbox(true);
    sandbox.install_program("houserules", &forwarding_houserules());
    let key = commit_install(&sandbox.work());
    stub_github_host(&sandbox, &key, pulls(&key), &reviews, failing);
    run_github_record(sandbox, first, key, "FIRST")
}

/// The `--reviewer` arguments of a `record write` call.
fn reviewer_arguments(call: &str) -> Vec<&str> {
    call.split(' ')
        .filter(|argument| argument.starts_with("--reviewer="))
        .collect()
}

/// The `::notice::` lines that the job of the run printed.
fn notices(run: &RecordRun) -> Vec<String> {
    stdout(&run.output)
        .lines()
        .filter(|line| line.starts_with("::notice::"))
        .map(str::to_string)
        .collect()
}

/// Spec 10.1: the notice of a GitHub read that fails, for the key commit `key`.
fn unreadable_notice(key: &str) -> String {
    format!(
        "::notice::houserules: the pull requests of {} cannot be read, so the record keeps reviewers_pending",
        &key[..7]
    )
}

/// Asserts what spec 10.1 says of a push whose approvers are unknown on GitHub:
/// the job exits 0, prints the notice once, calls `record write` with
/// `--reviewers-pending` and no `--reviewer`, and writes a note with no
/// reviewers and `reviewers_pending: true`. Returns the note.
fn assert_reviewers_stay_pending(run: &RecordRun) -> Value {
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    assert_eq!(notices(run), vec![unreadable_notice(&run.key)]);
    let call = record_write_call(run);
    assert!(passes_pending_flag(call), "{call}");
    assert_eq!(reviewer_arguments(call), Vec::<&str>::new(), "{call}");
    let note = written_note(run);
    assert_eq!(note["host"], json!("github"));
    assert_eq!(note["reviewers"], json!([]));
    assert_eq!(note["reviewers_pending"], json!(true));
    note
}

/// Spec 10.1: a list of requests that fails leaves the approvers unknown, so the
/// GitHub `record` job passes `--reviewers-pending` and prints the notice; it finds
/// no request, reads nothing more, and writes the note with no bundle.
#[test]
fn a_github_push_whose_listing_fails_keeps_the_pending_flag() {
    let run = github_record_with_the_binary(|_| None, reviews_of_request_12(), &[]);
    let note = assert_reviewers_stay_pending(&run);
    assert!(!record_write_call(&run).contains("--bundle"));
    assert_eq!(note["bundle"], Value::Null);
    assert_eq!(
        run.sandbox.calls().len(),
        1,
        "only the list is read: {:?}",
        run.sandbox.calls()
    );
}

/// Spec 10.1: a merged request that cannot be read leaves the approvers unknown,
/// so the job passes `--reviewers-pending` and prints the notice; it passes no
/// bundle, because the description is unknown, and reads no reviews.
#[test]
fn a_github_push_whose_request_read_fails_keeps_the_pending_flag() {
    let run =
        github_record_with_the_binary(pulls_of, reviews_of_request_12(), &[GITHUB_REQUEST_PATH]);
    let note = assert_reviewers_stay_pending(&run);
    assert!(!record_write_call(&run).contains("--bundle"));
    assert_eq!(note["bundle"], Value::Null);
    assert!(
        !run.sandbox
            .calls()
            .iter()
            .any(|call| call.contains("/reviews")),
        "{:?}",
        run.sandbox.calls()
    );
}

/// Spec 10.1: reviews that cannot be read leave the approvers unknown, so the job
/// passes `--reviewers-pending` and prints the notice; the request was read, so the
/// job still passes its description as the bundle.
#[test]
fn a_github_push_whose_reviews_read_fails_keeps_the_pending_flag() {
    let run =
        github_record_with_the_binary(pulls_of, reviews_of_request_12(), &[GITHUB_REVIEWS_PATH]);
    let note = assert_reviewers_stay_pending(&run);
    assert!(record_write_call(&run).contains(" --bundle "));
    assert_ne!(note["bundle"], Value::Null);
}

/// Spec 10.1: a push whose list of requests holds no merged request has no
/// approvers to read, so the GitHub `record` job passes no `--reviewers-pending`
/// and prints no notice: the note holds `reviewers: []` and `reviewers_pending:
/// false`, with no bundle. Two lists hold no merged request: an open request and
/// a request that is not merged (`others_pulls`), and an empty list.
#[test]
fn a_github_push_with_no_merged_request_writes_a_final_note() {
    for pulls in [
        Box::new(others_pulls) as Box<dyn Fn(&str) -> Option<Value>>,
        Box::new(|_: &str| Some(json!([]))),
    ] {
        let run = github_record_with_the_binary(pulls, json!([]), &[]);
        assert_eq!(
            run.output.status.code(),
            Some(0),
            "{}{}",
            stdout(&run.output),
            stderr(&run.output)
        );
        assert_eq!(notices(&run), Vec::<String>::new());
        let call = record_write_call(&run);
        assert!(!passes_pending_flag(call), "{call}");
        assert!(!call.contains("--bundle"), "{call}");
        let note = written_note(&run);
        assert_eq!(note["host"], json!("github"));
        assert_eq!(note["reviewers"], json!([]));
        assert_eq!(note["reviewers_pending"], json!(false));
        assert_eq!(note["bundle"], Value::Null);
    }
}

/// Spec 10.1: a push whose merged request and reviews read passes `--reviewer`
/// for each approver and no `--reviewers-pending`, and prints no notice: the note
/// holds the approvers and `reviewers_pending: false`.
#[test]
fn a_github_push_with_readable_approvers_passes_each_approver() {
    let run = github_record_with_the_binary(pulls_of, reviews_of_request_12(), &[]);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    assert_eq!(notices(&run), Vec::<String>::new());
    let call = record_write_call(&run);
    assert!(!passes_pending_flag(call), "{call}");
    assert_eq!(
        reviewer_arguments(call),
        vec!["--reviewer=amy", "--reviewer=cy"],
        "{call}"
    );
    assert!(call.contains(" --bundle "), "{call}");
    let note = written_note(&run);
    assert_eq!(note["reviewers"], json!(["amy", "cy"]));
    assert_eq!(note["reviewers_pending"], json!(false));
}

/// Spec 10.1: a merged request that nobody approved has no approvers to name,
/// and its reviews read, so the approvers are known: the GitHub `record` job passes
/// no `--reviewers-pending` and no `--reviewer`, prints no notice, and passes the
/// description as the bundle. The note holds `reviewers: []` and `reviewers_pending: false`.
#[test]
fn a_github_push_whose_merged_request_has_no_approver_writes_a_final_note() {
    let run = github_record_with_the_binary(pulls_of, json!([]), &[]);
    assert_eq!(
        run.output.status.code(),
        Some(0),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    assert_eq!(notices(&run), Vec::<String>::new());
    let call = record_write_call(&run);
    assert!(!passes_pending_flag(call), "{call}");
    assert_eq!(reviewer_arguments(call), Vec::<&str>::new(), "{call}");
    assert!(call.contains(" --bundle "), "{call}");
    let note = written_note(&run);
    assert_eq!(note["reviewers"], json!([]));
    assert_eq!(note["reviewers_pending"], json!(false));
}

/// The line of a `record` job whose remote cannot give the notes ref.
const UNREADABLE_NOTES_REF_LINE: &str =
    "houserules: cannot read the notes ref refs/notes/houserules from the remote";

/// Points `origin` of the work folder of `sandbox` at a folder that holds no
/// repository, so that `git ls-remote origin` fails.
fn make_the_remote_unreadable(sandbox: &Sandbox) {
    let missing = text(&sandbox.path().join("remote").join("missing.git"));
    git(&sandbox.work(), &["remote", "set-url", "origin", &missing]);
}

/// The `record` job tolerates an absent notes ref and nothing else: when
/// `git ls-remote` fails, the job ends with exit status 1 and the line, runs no
/// audit and no `record write`, calls no host API, and reports no record. The
/// push gets no record until the job runs again.
#[test]
fn a_github_push_whose_notes_ref_cannot_be_read_gets_no_record() {
    let (sandbox, first, second) = record_sandbox(false);
    stub_github_host(
        &sandbox,
        &second,
        pulls_of(&second),
        &reviews_of_request_12(),
        &[],
    );
    make_the_remote_unreadable(&sandbox);
    let run = run_github_record(sandbox, first, second, "FIRST");

    assert_eq!(
        run.output.status.code(),
        Some(1),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    let printed = stdout(&run.output);
    let lines: Vec<&str> = printed
        .lines()
        .filter(|line| !line.starts_with("::add-mask::"))
        .collect();
    assert_eq!(
        lines,
        vec![format!("::error::{UNREADABLE_NOTES_REF_LINE}")],
        "the job prints the line and nothing else but the mask"
    );
    assert!(run.log.is_empty(), "no audit and no record: {:?}", run.log);
    assert!(
        run.sandbox.calls().is_empty(),
        "no host call: {:?}",
        run.sandbox.calls()
    );
    assert!(
        !run.sandbox.temp().join("github_output").exists(),
        "the step reports no record"
    );
}

/// The same on GitLab: exit status 1, the line, and no record.
#[test]
fn a_gitlab_push_whose_notes_ref_cannot_be_read_gets_no_record() {
    let (sandbox, first, second) = record_sandbox(false);
    stub_gitlab_host(&sandbox, &second, merged_listing(&second));
    make_the_remote_unreadable(&sandbox);
    let run = run_gitlab_record(sandbox, first, second, "FIRST");

    assert_eq!(
        run.output.status.code(),
        Some(1),
        "{}{}",
        stdout(&run.output),
        stderr(&run.output)
    );
    assert_eq!(
        stdout(&run.output),
        format!("{UNREADABLE_NOTES_REF_LINE}\n")
    );
    assert!(run.log.is_empty(), "no audit and no record: {:?}", run.log);
    assert!(
        run.sandbox.calls().is_empty(),
        "no host call: {:?}",
        run.sandbox.calls()
    );
    assert!(!run.folder.join("record.json").exists(), "no record file");
}

/// Case 8: two installs of one repository get distinct notes refs; a top-level
/// install has `refs/notes/houserules`, a subdirectory install
/// `refs/notes/houserules-sub/<install path>`, and a path is read the same with
/// a leading `./` or a trailing `/`.
#[test]
fn two_installs_of_one_repository_get_distinct_notes_refs() {
    let sandbox = Sandbox::new();
    let unit = github();
    let shared = unit
        .variable("gates", "HOUSERULES_SH")
        .expect("the shared functions");
    let mut seen = Vec::new();
    for (dir, expected) in [
        (".", "refs/notes/houserules"),
        ("./", "refs/notes/houserules"),
        ("", "refs/notes/houserules"),
        ("docs", "refs/notes/houserules-sub/docs"),
        ("./docs/", "refs/notes/houserules-sub/docs"),
        ("sub/dir", "refs/notes/houserules-sub/sub/dir"),
    ] {
        let output = sandbox.run(
            &["sh"],
            "eval \"$HOUSERULES_SH\"\nhr_notes_ref \"$1\"\n",
            &[dir],
            [("HOUSERULES_SH", shared.clone())],
        );
        assert_eq!(output.status.code(), Some(0), "{dir:?}");
        assert_eq!(stdout(&output), expected, "{dir:?}");
        seen.push(stdout(&output));
    }
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 3, "three installs, three refs: {seen:?}");
}
