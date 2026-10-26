//! The report job of the GitLab unit, run for real with a stub `curl`.

use super::*;

/// The environment of the GitLab report script for the install `key` at `dir`.
fn gitlab_report_env(
    unit: &Unit,
    dir: &str,
    token: &str,
    floor: &str,
    source_project: &str,
) -> Vec<(String, String)> {
    env_of(
        unit,
        "report",
        &[
            ("HOUSERULES_DIR", dir),
            ("HOUSERULES_TOKEN", token),
            ("HOUSERULES_FLOOR_ROUTE", floor),
            ("CI_API_V4_URL", "https://gitlab.example/api/v4"),
            ("CI_MERGE_REQUEST_PROJECT_ID", "5"),
            ("CI_MERGE_REQUEST_SOURCE_PROJECT_ID", source_project),
            ("CI_MERGE_REQUEST_IID", "7"),
            ("CI_MERGE_REQUEST_SOURCE_BRANCH_SHA", HEAD),
            ("CI_COMMIT_SHA", "cccccccccccccccccccccccccccccccccccccccc"),
            ("CI_JOB_TOKEN", "job-token"),
            ("CI_ENVIRONMENT_NAME", "houserules-root"),
        ],
    )
}

/// Sets the replies of a GitLab request: the current head and the account of the
/// token.
fn gitlab_replies(sandbox: &Sandbox, current: &str) {
    sandbox.reply(
        "GET",
        "/api/v4/projects/5/merge_requests/7",
        &json!({"sha": current}).to_string(),
    );
    sandbox.reply("GET", "/api/v4/user", &json!({"id": 99}).to_string());
}

/// Runs the GitLab report script.
fn gitlab_report(sandbox: &Sandbox, unit: &Unit, env: Vec<(String, String)>) -> Output {
    sandbox.run_sh(&unit.job_script("report"), env)
}

/// Spec 10.4: the GitLab report posts the note, sets the status and the labels
/// with the token, and reads with the job token.
#[test]
fn the_gitlab_report_posts_the_note_the_status_and_the_labels() {
    let unit = gitlab("root");
    let sandbox = Sandbox::new();
    put_evidence(
        &sandbox.work(),
        "root",
        Some("auto-merge-eligible"),
        Some(PACKET),
    );
    gitlab_replies(&sandbox, HEAD);
    let output = gitlab_report(
        &sandbox,
        &unit,
        gitlab_report_env(&unit, ".", "stub-token", "owner-review", "5"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(stdout(&output), "");
    let calls = sandbox.calls();
    assert!(
        calls[0].contains("--header JOB-TOKEN: job-token")
            && calls[0].ends_with("https://gitlab.example/api/v4/projects/5/merge_requests/7"),
        "the head is read with the job token: {calls:?}"
    );
    let note = body_of(&sandbox, "--request POST");
    let body = note["body"].as_str().expect("a body");
    assert!(
        body.starts_with("<!-- houserules:evidence . -->\nApplied route: owner-review (computed auto-merge-eligible, floor owner-review)\n## houserules evidence\n"),
        "{body}"
    );
    let writes = sandbox.writes();
    assert!(
        writes[0].ends_with("https://gitlab.example/api/v4/projects/5/merge_requests/7/notes")
            && writes[0].contains("--header PRIVATE-TOKEN: stub-token"),
        "{writes:?}"
    );
    assert!(
        writes.iter().any(|call| call.contains("--data-urlencode state=success --data-urlencode name=houserules/route --data-urlencode description=owner-review")
            && call.ends_with(&format!("/api/v4/projects/5/statuses/{HEAD}"))),
        "{writes:?}"
    );
    let label_creations = writes
        .iter()
        .filter(|call| call.ends_with("/api/v4/projects/5/labels"))
        .count();
    assert_eq!(label_creations, 3, "{writes:?}");
    assert!(
        writes.last().expect("a last write").contains(
            "--request PUT --header PRIVATE-TOKEN: stub-token --data-urlencode add_labels=houserules: owner-review --data-urlencode remove_labels=houserules: auto-merge-eligible,houserules: senior-review"
        ),
        "{writes:?}"
    );
}

/// Spec 10.1: the GitLab report updates the newest note of its own account, over
/// every page of the notes, and no note of another author.
#[test]
fn the_gitlab_report_updates_the_newest_note_of_its_own_account_over_every_page() {
    let unit = gitlab("root");
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.work(), "root", Some("owner-review"), Some(PACKET));
    gitlab_replies(&sandbox, HEAD);
    let filler: Vec<Value> = (100..200)
        .map(|id| json!({"id": id, "author": {"id": 5}, "body": "a comment", "system": false}))
        .collect();
    sandbox.reply(
        "GET",
        "/api/v4/projects/5/merge_requests/7/notes",
        &Value::Array(filler).to_string(),
    );
    sandbox.reply_page(
        "GET",
        "/api/v4/projects/5/merge_requests/7/notes",
        2,
        &json!([
            {"id": 21, "author": {"id": 99}, "body": "<!-- houserules:evidence . -->\nold", "system": false},
            {"id": 22, "author": {"id": 99}, "body": "<!-- houserules:evidence . -->\r\nnewer", "system": false},
            {"id": 23, "author": {"id": 5}, "body": "<!-- houserules:evidence . -->\nfake", "system": false},
            {"id": 24, "author": {"id": 99}, "body": "<!-- houserules:evidence docs -->\nanother install", "system": false}
        ])
        .to_string(),
    );
    let output = gitlab_report(
        &sandbox,
        &unit,
        gitlab_report_env(&unit, ".", "stub-token", "owner-review", "5"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let writes = sandbox.writes();
    assert!(
        writes[0].contains("--request PUT")
            && writes[0].ends_with("/projects/5/merge_requests/7/notes/22"),
        "{writes:?}"
    );
    assert!(
        sandbox.calls().iter().any(|call| call.contains("page=2")),
        "page 2 was read"
    );
}

/// Spec 10.4: without `HOUSERULES_TOKEN`, the GitLab report prints one notice and
/// the packet, writes nothing, and exits 0. A fork's request posts nothing
/// either, and a moved head makes no write.
#[test]
fn the_gitlab_report_writes_nothing_without_a_token_for_a_fork_or_a_moved_head() {
    let unit = gitlab("root");

    // no token
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.work(), "root", Some("owner-review"), Some(PACKET));
    gitlab_replies(&sandbox, HEAD);
    let output = gitlab_report(
        &sandbox,
        &unit,
        gitlab_report_env(&unit, ".", "", "owner-review", "5"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(sandbox.writes().is_empty(), "{:?}", sandbox.writes());
    let printed = stdout(&output);
    assert!(
        printed.contains("HOUSERULES_TOKEN is not set for the environment houserules-root"),
        "{printed}"
    );
    assert!(
        printed.contains("Applied route: owner-review (computed owner-review, floor owner-review)"),
        "{printed}"
    );
    assert!(
        printed.contains("### Uncovered surface"),
        "the packet is printed: {printed}"
    );

    // a fork's request
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.work(), "root", Some("owner-review"), Some(PACKET));
    let output = gitlab_report(
        &sandbox,
        &unit,
        gitlab_report_env(&unit, ".", "stub-token", "owner-review", "9"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert!(
        sandbox.calls().is_empty(),
        "no call at all: {:?}",
        sandbox.calls()
    );
    assert!(
        stdout(&output).contains("comes from a fork"),
        "{}",
        stdout(&output)
    );

    // a moved head
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.work(), "root", Some("owner-review"), Some(PACKET));
    gitlab_replies(&sandbox, MOVED);
    let output = gitlab_report(
        &sandbox,
        &unit,
        gitlab_report_env(&unit, ".", "stub-token", "owner-review", "5"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(sandbox.calls().len(), 1, "{:?}", sandbox.calls());
    assert!(sandbox.writes().is_empty());
    assert!(
        stdout(&output).contains("this run writes nothing"),
        "{}",
        stdout(&output)
    );
}

/// Spec 10.1: a refused write prints one notice and exits 0 on GitLab too.
#[test]
fn the_gitlab_report_prints_one_notice_and_exits_0_on_a_refused_write() {
    let unit = gitlab("root");
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.work(), "root", Some("owner-review"), Some(PACKET));
    gitlab_replies(&sandbox, HEAD);
    sandbox.refuse_writes();
    let output = gitlab_report(
        &sandbox,
        &unit,
        gitlab_report_env(&unit, ".", "stub-token", "owner-review", "5"),
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        stdout(&output)
            .lines()
            .filter(|line| line.contains("refused"))
            .count(),
        1,
        "{}",
        stdout(&output)
    );
    assert_eq!(sandbox.writes().len(), 1, "{:?}", sandbox.writes());
}

/// Spec 10.1, C18: on GitLab the floor comes from the dotenv variable of the
/// floor job only; a floor document in the artifact of gates is never read, and a
/// fourth value is refused.
#[test]
fn the_gitlab_report_reads_no_floor_document_that_gates_uploaded() {
    let unit = gitlab("root");
    for (floor, expected, shown) in [
        ("", "owner-review", "missing"),
        ("banana", "owner-review", "missing"),
        ("senior-review", "senior-review", "senior-review"),
    ] {
        let sandbox = Sandbox::new();
        let evidence = put_evidence(
            &sandbox.work(),
            "root",
            Some("auto-merge-eligible"),
            Some(PACKET),
        );
        for planted in ["floor.json", "floor.env", "floor-document.json"] {
            fs::write(evidence.join(planted), route_doc("auto-merge-eligible"))
                .expect("plant a floor");
        }
        gitlab_replies(&sandbox, HEAD);
        let output = gitlab_report(
            &sandbox,
            &unit,
            gitlab_report_env(&unit, ".", "stub-token", floor, "5"),
        );
        assert_eq!(
            output.status.code(),
            Some(0),
            "{floor:?}: {}{}",
            stdout(&output),
            stderr(&output)
        );
        let note = body_of(&sandbox, "--request POST");
        let body = note["body"].as_str().expect("a body");
        assert_eq!(
            body.lines().nth(1).expect("the applied-route line"),
            format!("Applied route: {expected} (computed auto-merge-eligible, floor {shown})"),
            "{floor:?}"
        );
    }
}

/// Case 8: two installs of one repository get distinct markers, status contexts,
/// and labels on GitLab.
#[test]
fn two_gitlab_installs_get_distinct_markers_contexts_and_labels() {
    let mut seen: Vec<(String, String, String)> = Vec::new();
    for (dir, key) in [(".", "root"), ("docs", "docs")] {
        let unit = gitlab(key);
        let sandbox = Sandbox::new();
        put_evidence(&sandbox.work(), key, Some("owner-review"), Some(PACKET));
        gitlab_replies(&sandbox, HEAD);
        let mut env = gitlab_report_env(&unit, dir, "stub-token", "owner-review", "5");
        env.retain(|(name, _)| name != "CI_ENVIRONMENT_NAME");
        env.push((
            "CI_ENVIRONMENT_NAME".to_string(),
            format!("houserules-{key}"),
        ));
        let output = gitlab_report(&sandbox, &unit, env);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{dir}: {}{}",
            stdout(&output),
            stderr(&output)
        );
        let note = body_of(&sandbox, "--request POST");
        let marker = note["body"]
            .as_str()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        let writes = sandbox.writes();
        let context = writes
            .iter()
            .find_map(|call| call.split("--data-urlencode name=").nth(1))
            .and_then(|rest| rest.split(" --data-urlencode").next())
            .expect("a status context")
            .to_string();
        let label = writes
            .last()
            .and_then(|call| call.split("add_labels=").nth(1))
            .and_then(|rest| rest.split(" --data-urlencode").next())
            .expect("a label")
            .to_string();
        seen.push((marker, context, label));
    }
    assert_eq!(
        seen,
        vec![
            (
                "<!-- houserules:evidence . -->".to_string(),
                "houserules/route".to_string(),
                "houserules: owner-review".to_string()
            ),
            (
                "<!-- houserules:evidence docs -->".to_string(),
                "houserules/route (docs)".to_string(),
                "houserules docs: owner-review".to_string()
            ),
        ]
    );
}
