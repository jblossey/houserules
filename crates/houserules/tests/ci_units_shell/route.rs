//! The `report` logic that both units share: `applied_route` on documents of the gates job and the floor job.

use super::*;

/// What `applied_route` printed.
#[derive(Debug, Default)]
struct Applied {
    /// The applied route, or `None` when the function wrote no route.
    route: Option<String>,
    /// The applied-route line.
    line: Option<String>,
    /// The notices.
    notices: Vec<String>,
    /// Every line of the output, for a failure message.
    output: String,
}

/// Runs `applied_route` of the shared `report` logic on the documents. `gates`
/// and `floor` are the text of a document, or `None` for a missing file.
fn applied(gates: Option<&str>, floor: Option<&str>, own: &str, current: &str) -> Applied {
    let sandbox = Sandbox::new();
    let unit = github();
    let gates_path = sandbox.work().join("gates.json");
    let floor_path = sandbox.work().join("floor.json");
    for (path, document) in [(&gates_path, gates), (&floor_path, floor)] {
        if let Some(document) = document {
            fs::write(path, document).expect("write a document");
        }
    }
    let route_sh = unit
        .variable("report", "HOUSERULES_ROUTE_SH")
        .expect("the report logic");
    let output = sandbox.run(
        &["sh"],
        "eval \"$HOUSERULES_ROUTE_SH\"\napplied_route \"$1\" \"$2\" \"$3\" \"$4\"\n",
        &[&text(&gates_path), &text(&floor_path), own, current],
        [("HOUSERULES_ROUTE_SH", route_sh)],
    );
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );
    let printed = stdout(&output);
    let mut result = Applied {
        output: printed.clone(),
        ..Applied::default()
    };
    for line in printed.lines() {
        if let Some(route) = line.strip_prefix("route=") {
            result.route = Some(route.to_string());
        } else if let Some(applied_line) = line.strip_prefix("line=") {
            result.line = Some(applied_line.to_string());
        } else if let Some(notice) = line.strip_prefix("notice=") {
            result.notices.push(notice.to_string());
        } else {
            panic!("an unknown line of applied_route: {line}");
        }
    }
    result
}

/// Case 2: the applied route is the higher of the route of gates and the floor.
#[test]
fn the_applied_route_is_the_higher_of_gates_and_floor() {
    let routes = ["auto-merge-eligible", "owner-review", "senior-review"];
    for (gates_rank, gates) in routes.iter().enumerate() {
        for (floor_rank, floor) in routes.iter().enumerate() {
            let result = applied(Some(&route_doc(gates)), Some(&route_doc(floor)), HEAD, HEAD);
            let expected = routes[gates_rank.max(floor_rank)];
            assert_eq!(
                result.route.as_deref(),
                Some(expected),
                "{gates} and {floor}"
            );
            let line = result.line.expect("an applied-route line");
            assert_eq!(
                line,
                format!("Applied route: {expected} (computed {gates}, floor {floor})")
            );
            assert!(
                line.chars().count() <= 120,
                "{} characters: {line}",
                line.chars().count()
            );
            assert!(result.notices.is_empty(), "{:?}", result.notices);
        }
    }
}

/// Case 2: a route that is none of the three values is refused: the document
/// counts as missing, its text is never printed, and the line stays short.
#[test]
fn a_fourth_route_value_is_refused_in_either_document() {
    let fourth = "a-very-long-fourth-route-value-that-a-tool-of-the-project-wrote-to-lengthen-the-line-of-the-packet";
    for invalid in [
        route_doc(fourth),
        json!({"route": 5}).to_string(),
        json!({"route": null}).to_string(),
        json!({"route": ["owner-review"]}).to_string(),
        "{}".to_string(),
        "[]".to_string(),
        "not json".to_string(),
        String::new(),
    ] {
        // an invalid gates document leaves the floor alone
        let result = applied(
            Some(&invalid),
            Some(&route_doc("auto-merge-eligible")),
            HEAD,
            HEAD,
        );
        assert_eq!(
            result.route.as_deref(),
            Some("auto-merge-eligible"),
            "gates {invalid:?}"
        );
        assert_eq!(
            result.line.as_deref(),
            Some(
                "Applied route: auto-merge-eligible (computed missing, floor auto-merge-eligible)"
            )
        );
        assert_eq!(result.notices.len(), 1, "{:?}", result.notices);
        // an invalid floor document means a floor of owner-review
        let result = applied(
            Some(&route_doc("auto-merge-eligible")),
            Some(&invalid),
            HEAD,
            HEAD,
        );
        assert_eq!(
            result.route.as_deref(),
            Some("owner-review"),
            "floor {invalid:?}"
        );
        assert_eq!(
            result.line.as_deref(),
            Some("Applied route: owner-review (computed auto-merge-eligible, floor missing)")
        );
        assert_eq!(result.notices.len(), 1, "{:?}", result.notices);
        let result = applied(
            Some(&route_doc("senior-review")),
            Some(&invalid),
            HEAD,
            HEAD,
        );
        assert_eq!(
            result.route.as_deref(),
            Some("senior-review"),
            "floor {invalid:?}"
        );
        assert!(
            !result.output.contains("a-very-long"),
            "the invalid text is never printed"
        );
        assert!(result.line.expect("a line").chars().count() <= 120);
    }
}

/// Spec 10.1: a missing floor document means a floor of `owner-review`, a
/// missing gates document leaves the floor alone, and each case prints one
/// notice.
#[test]
fn a_missing_document_means_an_owner_review_floor_or_leaves_the_floor_alone() {
    let result = applied(Some(&route_doc("auto-merge-eligible")), None, HEAD, HEAD);
    assert_eq!(result.route.as_deref(), Some("owner-review"));
    assert_eq!(result.notices.len(), 1);
    assert!(result.notices[0].contains("floor document is missing or invalid"));
    let result = applied(None, Some(&route_doc("auto-merge-eligible")), HEAD, HEAD);
    assert_eq!(result.route.as_deref(), Some("auto-merge-eligible"));
    assert_eq!(result.notices.len(), 1);
    assert!(result.notices[0].contains("gates document is missing or invalid"));
    let result = applied(None, None, HEAD, HEAD);
    assert_eq!(result.route.as_deref(), Some("owner-review"));
    assert_eq!(
        result.line.as_deref(),
        Some("Applied route: owner-review (computed missing, floor missing)")
    );
    assert_eq!(result.notices.len(), 2, "one notice for each case");
}

/// Spec 10.1: a head that differs from the request's current head, or a head
/// that cannot be read, gives one notice and no route.
#[test]
fn a_moved_or_unreadable_head_gives_a_notice_and_no_route() {
    for current in [MOVED, ""] {
        let result = applied(
            Some(&route_doc("senior-review")),
            Some(&route_doc("senior-review")),
            HEAD,
            current,
        );
        assert_eq!(result.route, None, "current {current:?}");
        assert_eq!(result.line, None);
        assert_eq!(result.notices.len(), 1);
        assert!(
            result.notices[0].contains("this run writes nothing"),
            "{:?}",
            result.notices
        );
        assert!(
            result.notices[0].contains("aaaaaaa"),
            "{:?}",
            result.notices
        );
    }
}
