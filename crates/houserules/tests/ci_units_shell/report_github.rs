//! The report job of the GitHub unit, run for real with a stub `gh`.

use super::*;

/// Sets the replies of a GitHub request: the current head.
fn github_replies(sandbox: &Sandbox, current: &str) {
    sandbox.reply(
        "GET",
        "/repos/acme/widgets/pulls/7",
        &json!({"head": {"sha": current}}).to_string(),
    );
}

/// Runs the report step of the GitHub unit for the install `key` at `dir`, with
/// the floor route `floor`.
fn github_report(sandbox: &Sandbox, dir: &str, key: &str, floor: &str) -> Output {
    let unit = github();
    let env = env_of(
        &unit,
        "report",
        &[
            ("GH_TOKEN", "stub-token"),
            ("GITHUB_REPOSITORY", "acme/widgets"),
            ("GITHUB_SERVER_URL", "https://github.com"),
            ("HOUSERULES_DIR", dir),
            ("HOUSERULES_KEY", key),
            ("HOUSERULES_FLOOR_ROUTE", floor),
            ("HOUSERULES_REQUEST", "7"),
            ("HOUSERULES_HEAD", HEAD),
        ],
    );
    sandbox.run_bash(&unit.step_script("report", "Report the route"), env)
}

/// Spec 10.1: `report` inserts the applied-route line after the marker, posts the
/// one comment, sets the commit status, and sets the label of the applied
/// route, removing the other two.
#[test]
fn the_github_report_posts_the_comment_the_status_and_the_label() {
    let sandbox = Sandbox::new();
    put_evidence(
        &sandbox.temp(),
        "root",
        Some("auto-merge-eligible"),
        Some(PACKET),
    );
    github_replies(&sandbox, HEAD);
    let output = github_report(&sandbox, ".", "root", "owner-review");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(stdout(&output), "");
    let calls = sandbox.calls();
    assert_eq!(calls[0], "api repos/acme/widgets/pulls/7 --jq .head.sha");
    assert_eq!(
        calls[1],
        "api --paginate repos/acme/widgets/issues/7/comments?per_page=100"
    );
    let comment = body_of(
        &sandbox,
        "--method POST repos/acme/widgets/issues/7/comments",
    );
    let body = comment["body"].as_str().expect("a body");
    let expected_head = "<!-- houserules:evidence . -->\nApplied route: owner-review (computed auto-merge-eligible, floor owner-review)\n## houserules evidence\n";
    assert!(body.starts_with(expected_head), "{body}");
    assert!(body.ends_with("None.\n"), "{body}");
    assert_eq!(
        body.matches("houserules:evidence").count(),
        1,
        "one marker line"
    );
    let writes = sandbox.writes();
    let status = format!(
        "api --method POST repos/acme/widgets/statuses/{HEAD} -f state=success -f context=houserules/route -f description=owner-review"
    );
    assert!(writes.contains(&status), "{writes:?}");
    for route in ["auto-merge-eligible", "owner-review", "senior-review"] {
        assert!(
            writes.contains(&format!(
                "api --method POST repos/acme/widgets/labels -f name=houserules: {route} -f color=1d76db"
            )),
            "{writes:?}"
        );
    }
    for other in ["auto-merge-eligible", "senior-review"] {
        let encoded = format!("houserules%3A%20{other}");
        assert!(
            writes.contains(&format!(
                "api --method DELETE repos/acme/widgets/issues/7/labels/{encoded}"
            )),
            "{writes:?}"
        );
    }
    assert!(
        writes.contains(
            &"api --method POST repos/acme/widgets/issues/7/labels -f labels[]=houserules: owner-review"
                .to_string()
        ),
        "{writes:?}"
    );
    assert!(
        !writes
            .iter()
            .any(|call| call.contains("DELETE") && call.contains("houserules%3A%20owner-review")),
        "the label of the applied route stays: {writes:?}"
    );
}

/// Spec 10.1: the comment of the install is the newest one by the account of
/// the poster whose first line is the marker. A human's comment with the marker,
/// a comment of another install, and an older comment do not count.
#[test]
fn the_github_report_updates_the_newest_comment_of_its_own_account() {
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.temp(), "root", Some("owner-review"), Some(PACKET));
    github_replies(&sandbox, HEAD);
    sandbox.reply(
        "GET",
        "/repos/acme/widgets/issues/7/comments",
        &json!([
            {"id": 11, "user": {"login": "github-actions[bot]"}, "body": "<!-- houserules:evidence . -->\nold"},
            {"id": 12, "user": {"login": "github-actions[bot]"}, "body": "<!-- houserules:evidence . -->\r\nnewer, with a CRLF line end"},
            {"id": 13, "user": {"login": "mallory"}, "body": "<!-- houserules:evidence . -->\nfake"},
            {"id": 14, "user": {"login": "github-actions[bot]"}, "body": "<!-- houserules:evidence docs -->\nanother install"},
        ])
        .to_string(),
    );
    let output = github_report(&sandbox, ".", "root", "owner-review");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let writes = sandbox.writes();
    assert!(
        writes[0].starts_with("api --method PATCH repos/acme/widgets/issues/comments/12 --input "),
        "{writes:?}"
    );
    assert!(
        !writes
            .iter()
            .any(|call| call.contains("--method POST repos/acme/widgets/issues/7/comments")),
        "{writes:?}"
    );
}

/// Spec 10.1: a run whose head is no longer the request's head makes no write.
#[test]
fn the_github_report_writes_nothing_when_the_head_moved() {
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.temp(), "root", Some("owner-review"), Some(PACKET));
    github_replies(&sandbox, MOVED);
    let output = github_report(&sandbox, ".", "root", "owner-review");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        sandbox.calls().len(),
        1,
        "only the read of the head: {:?}",
        sandbox.calls()
    );
    assert!(sandbox.writes().is_empty());
    assert_eq!(
        stdout(&output).matches("::notice::").count(),
        1,
        "{}",
        stdout(&output)
    );
    assert!(stdout(&output).contains("this run writes nothing"));
}

/// Spec 10.1: a read-only token refuses a write; the job prints one notice and
/// exits 0, and it makes no further write.
#[test]
fn the_github_report_prints_one_notice_and_exits_0_on_a_refused_write() {
    let sandbox = Sandbox::new();
    put_evidence(&sandbox.temp(), "root", Some("owner-review"), Some(PACKET));
    github_replies(&sandbox, HEAD);
    sandbox.refuse_writes();
    let output = github_report(&sandbox, ".", "root", "owner-review");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    assert_eq!(
        stdout(&output).matches("::notice::").count(),
        1,
        "{}",
        stdout(&output)
    );
    assert!(
        stdout(&output).contains("refused a write"),
        "{}",
        stdout(&output)
    );
    assert_eq!(
        sandbox.writes().len(),
        1,
        "the first write was the last: {:?}",
        sandbox.writes()
    );
}

/// Spec 10.1, C18: the floor comes from the floor job only. A floor document in
/// the artifact of the gates job is never read, and a fourth value in the job
/// output is refused.
#[test]
fn the_github_report_reads_no_floor_document_that_gates_uploaded() {
    for (floor, expected) in [
        ("", "owner-review"),
        ("banana", "owner-review"),
        ("senior-review", "senior-review"),
    ] {
        let sandbox = Sandbox::new();
        let evidence = put_evidence(
            &sandbox.temp(),
            "root",
            Some("auto-merge-eligible"),
            Some(PACKET),
        );
        for planted in ["floor.json", "floor-root.json", "route-floor.json"] {
            fs::write(evidence.join(planted), route_doc("auto-merge-eligible"))
                .expect("plant a floor");
        }
        github_replies(&sandbox, HEAD);
        let output = github_report(&sandbox, ".", "root", floor);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{floor:?}: {}",
            stdout(&output)
        );
        let comment = body_of(
            &sandbox,
            "--method POST repos/acme/widgets/issues/7/comments",
        );
        let body = comment["body"].as_str().expect("a body");
        let second = body.lines().nth(1).expect("the applied-route line");
        let shown = if expected == "owner-review" {
            "missing"
        } else {
            "senior-review"
        };
        assert_eq!(
            second,
            format!("Applied route: {expected} (computed auto-merge-eligible, floor {shown})"),
            "{floor:?}"
        );
    }
}

/// Spec 10.1: a gates job that wrote no packet, or no route, still gives a
/// comment: the marker, the applied-route line, and a line that says so.
#[test]
fn the_github_report_posts_without_a_packet_when_gates_wrote_none() {
    let sandbox = Sandbox::new();
    github_replies(&sandbox, HEAD);
    let output = github_report(&sandbox, ".", "root", "senior-review");
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let comment = body_of(
        &sandbox,
        "--method POST repos/acme/widgets/issues/7/comments",
    );
    assert_eq!(
        comment["body"],
        "<!-- houserules:evidence . -->\nApplied route: senior-review (computed missing, floor senior-review)\n\nhouserules: the gates job wrote no evidence packet.\n"
    );
    assert_eq!(stdout(&output).matches("::notice::").count(), 1);
}

/// Case 8: two installs of one repository get distinct markers, status
/// contexts, and labels on GitHub.
#[test]
fn two_github_installs_get_distinct_markers_contexts_and_labels() {
    let mut seen: Vec<(String, String, String)> = Vec::new();
    for (dir, key) in [(".", "root"), ("docs", "docs"), ("docs/guide/", "guide")] {
        let sandbox = Sandbox::new();
        put_evidence(&sandbox.temp(), key, Some("owner-review"), None);
        github_replies(&sandbox, HEAD);
        let output = github_report(&sandbox, dir, key, "owner-review");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{dir}: {}{}",
            stdout(&output),
            stderr(&output)
        );
        let comment = body_of(
            &sandbox,
            "--method POST repos/acme/widgets/issues/7/comments",
        );
        let marker = comment["body"]
            .as_str()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string();
        let writes = sandbox.writes();
        let context = writes
            .iter()
            .find_map(|call| call.split(" -f context=").nth(1))
            .and_then(|rest| rest.split(" -f description=").next())
            .expect("a status context")
            .to_string();
        let label = writes
            .iter()
            .find_map(|call| call.split(" -f labels[]=").nth(1))
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
            (
                "<!-- houserules:evidence docs/guide -->".to_string(),
                "houserules/route (docs/guide)".to_string(),
                "houserules docs/guide: owner-review".to_string()
            ),
        ]
    );
}
