//! `houserules report-facts` (batch 36, spec 5; acceptance case 3; review
//! focus 3 to 5): it fills the fact fields of a task report from git and from
//! captures, and it reports every finding before it writes. Each test runs the
//! compiled binary in a scratch repository. The captures come from the
//! `capture` command of the same binary, and the captured commands run through
//! `sh`, so the text of a test runs on Windows with Git for Windows too
//! (`houserules.platform-gated-tests`).
//!
//! A test whose doc comment calls it a pin asserts behavior that the code held
//! before the change this file pins.

mod tracker_support;

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{assert_output, houserules, run_ok, scratch_git_repo, sha256_hex, stdout};

/// The lines of `src/a.rs` at the base commit; the next commit appends one line.
const A_RS_AT_BASE: &str = "fn first() {\n    let alpha = 1;\n    alpha + 1\n}\nfn second() {}\n";

/// A scratch repository with three commits and the files that the cases cite.
struct Scenario {
    dir: TempDir,
    /// The first commit: the base of the task.
    base: String,
    /// The second commit.
    middle: String,
    /// The third commit, HEAD.
    head: String,
}

impl Scenario {
    fn root(&self) -> &Path {
        self.dir.path()
    }
}

/// Runs `git` in `dir` with a fixed identity and no signing; fails the test on
/// a git error. Returns stdout.
fn git(dir: &Path, args: &[&str]) -> String {
    git_at(dir, args, "2026-01-01T00:00:00+00:00")
}

/// Runs `git` in `dir` with `date` as the author date and the committer date.
fn git_at(dir: &Path, args: &[&str], date: &str) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

/// Commits every change of `dir` with a date one second after the previous
/// commit and returns the full sha.
fn commit(dir: &Path, message: &str) -> String {
    let counted = Command::new("git")
        .args(["rev-list", "--count", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("run git rev-list");
    let before: u32 = if counted.status.success() {
        String::from_utf8_lossy(&counted.stdout)
            .trim()
            .parse()
            .expect("a commit count")
    } else {
        0
    };
    let date = format!("2026-01-01T00:00:{:02}+00:00", before + 1);
    git_at(dir, &["add", "-A"], &date);
    git_at(dir, &["commit", "-q", "--no-verify", "-m", message], &date);
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

/// Writes `contents` to `relative` below `dir` and creates the parents.
fn write(dir: &Path, relative: &str, contents: impl AsRef<[u8]>) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("create the parents");
    fs::write(path, contents).expect("write the file");
}

/// Three commits: the seed (an install that `init` made, and the files that the
/// cases cite), a change of `src/a.rs` with a new document, and a new library
/// file. `.gitattributes` stops every line-ending conversion, so a test that
/// reads back a file sees the bytes it wrote.
fn scenario() -> Scenario {
    let dir = scratch_git_repo();
    let root = dir.path();
    write(root, ".gitattributes", "* -text\n");
    write(root, "src/a.rs", A_RS_AT_BASE);
    write(root, "src/dup.rs", "let x = 1;\nlet y = 2;\n");
    write(root, "src/crlf.txt", "one\r\ntwo\r\nthree");
    write(root, "src/empty.txt", "");
    write(root, "docs/old.md", "old\n");
    run_ok(root, &["init"]);
    let base = commit(root, "chore: seed");
    write(root, "src/a.rs", format!("{A_RS_AT_BASE}fn third() {{}}\n"));
    write(root, "docs/n.md", "new\n");
    let middle = commit(root, "feat: change a");
    write(root, "lib/b.rs", "pub fn b() {}\n");
    let head = commit(root, "fix: add b");
    Scenario {
        dir,
        base,
        middle,
        head,
    }
}

/// Runs `capture` in `dir` of `sh -c <script>`. The capture goes to `out`,
/// relative to `dir`, whose folder is created.
fn capture(dir: &Path, out: &str, script: &str) {
    fs::create_dir_all(dir.join(out).parent().expect("a parent directory"))
        .expect("create the folder of the capture");
    let output = houserules()
        .current_dir(dir)
        .args(["capture", "--out", out, "--force", "--", "sh", "-c", script])
        .output()
        .expect("run capture");
    assert!(
        dir.join(out).is_file(),
        "no capture at {out}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The four captures that `draft` cites, in `caps/` below `dir`.
fn make_captures(dir: &Path) {
    capture(dir, "caps/suite.txt", "echo ok");
    capture(dir, "caps/live.txt", "echo live");
    capture(dir, "caps/red.txt", "echo FAILED >&2; exit 1");
    capture(dir, "caps/green.txt", "echo ok");
}

/// A draft report: every field of a task report, the facts left empty, one
/// run of each place that holds a run, and one citation.
fn draft() -> Value {
    json!({
        "kind": "task-report",
        "model": "sonnet",
        "task": 5,
        "backlog": ["HR-158"],
        "status": "BLOCKED",
        "implemented": "A draft.",
        "commits": [],
        "tests": [{"capture": "caps/suite.txt"}],
        "live_run": [{"capture": "caps/live.txt", "excerpt": "live"}],
        "tdd": [{
            "test": "t",
            "mode": "natural",
            "red": {"capture": "caps/red.txt"},
            "green": {"capture": "caps/green.txt"},
        }],
        "files_changed": [],
        "docs_verified": [],
        "dependency_vetting": null,
        "coverage": null,
        "citations": [{"path": "src/a.rs", "line": 2, "end": 3, "anchor": "alpha + 1"}],
        "self_audit": null,
        "self_review": [],
        "concerns": [],
        "knowledge_used": [],
        "fix_rounds": [{
            "round": 1,
            "findings": [],
            "commits": [],
            "tests": [{"capture": "caps/suite.txt"}],
        }],
    })
}

/// The five places of `draft` that hold a run: the JSON pointer and the label
/// that a finding gives the run.
const RUN_PLACES: [(&str, &str); 5] = [
    ("/tests/0", "tests[0]"),
    ("/live_run/0", "live_run[0]"),
    ("/tdd/0/red", "tdd[0].red"),
    ("/tdd/0/green", "tdd[0].green"),
    ("/fix_rounds/0/tests/0", "fix_rounds[0].tests[0]"),
];

/// A result of `audit --json` with two deterministic rows and one judged row.
fn audit_result() -> Value {
    json!({
        "summary": {
            "base": "abc1233", "head": "abc1234",
            "deterministic": 2, "pass": 2, "fail": 0, "warn": 0, "skipped": 0, "judged": 1,
            "reference_fail": 0,
        },
        "rules": [
            {"id": "process.tdd", "kind": "rule", "mode": "deterministic", "level": "fail",
             "result": "pass", "evidence": "1 commits checked"},
            {"id": "quality.principles", "kind": "rule", "mode": "judged", "level": null,
             "result": "open", "evidence": "judged by the reviewer"},
            {"id": "process.conventional-commits", "kind": "rule", "mode": "deterministic",
             "level": "fail", "result": "pass", "evidence": "2 commits checked"},
        ],
        "references": {"checked": 0, "passed": 0, "skipped": 0, "failures": []},
    })
}

/// Writes `value` as the CLI prints JSON: indented, with a trailing LF.
fn write_json(dir: &Path, relative: &str, value: &Value) {
    write(
        dir,
        relative,
        format!(
            "{}\n",
            serde_json::to_string_pretty(value).expect("serialize")
        ),
    );
}

/// Reads and parses the JSON file `relative` below `dir`.
fn read_json(dir: &Path, relative: &str) -> Value {
    serde_json::from_slice(&fs::read(dir.join(relative)).expect("read the file"))
        .expect("parse the file")
}

/// The keys of the object `value`, in their order.
fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect()
}

/// `houserules report-facts report.json --base <base> <extra>...` run in `cwd`.
fn facts_in(cwd: &Path, base: &str, extra: &[&str]) -> Output {
    houserules()
        .args(["report-facts", "report.json", "--base", base])
        .args(extra)
        .current_dir(cwd)
        .output()
        .expect("run report-facts")
}

/// The names in `dir`, sorted.
fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read the directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Runs `report-facts` over `report` (written as `report.json` in the root of
/// `scenario`) and requires exit 1, the `lines` on stderr and nothing on
/// stdout, the report byte for byte as it was, and no new file.
fn expect_findings(scenario: &Scenario, report: &Value, extra: &[&str], lines: &[String]) {
    let root = scenario.root();
    write_json(root, "report.json", report);
    let before = fs::read(root.join("report.json")).expect("read the report");
    let listing = names_in(root);
    let output = facts_in(root, &scenario.base, extra);
    assert_output(&output, 1, "", &format!("{}\n", lines.join("\n")));
    assert_eq!(
        fs::read(root.join("report.json")).expect("read the report"),
        before,
        "a finding writes nothing"
    );
    assert_eq!(names_in(root), listing, "a finding leaves no file behind");
}

/// Like `expect_findings` for a setup error: exit 2 and one line.
fn expect_setup_error(scenario: &Scenario, report: Option<&Value>, args: &[&str], line: &str) {
    let root = scenario.root();
    if let Some(report) = report {
        write_json(root, "report.json", report);
    }
    let before = fs::read(root.join("report.json")).ok();
    let listing = names_in(root);
    let output = houserules()
        .arg("report-facts")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run report-facts");
    assert_output(&output, 2, "", &format!("{line}\n"));
    assert_eq!(
        fs::read(root.join("report.json")).ok(),
        before,
        "a setup error writes nothing"
    );
    assert_eq!(
        names_in(root),
        listing,
        "a setup error leaves no file behind"
    );
}

// --- Case 3: the fill ---------------------------------------------------------

/// Spec 5 and case 3: a draft with three captures (and two more runs), one
/// citation, `--base`, and `--audit` gets every fact, and the command prints
/// `<REPORT>: facts filled`.
#[test]
fn a_draft_with_captures_a_citation_a_base_and_an_audit_gets_every_fact() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    write_json(root, "audit.json", &audit_result());
    write_json(root, "report.json", &draft());

    let output = facts_in(root, &s.base, &["--audit", "audit.json"]);

    assert_output(&output, 0, "report.json: facts filled\n", "");
    let report = read_json(root, "report.json");
    assert_eq!(
        report["commits"],
        json!([
            {"sha": s.middle, "subject": "feat: change a"},
            {"sha": s.head, "subject": "fix: add b"},
        ]),
        "commits: oldest first, full shas"
    );
    assert_eq!(
        report["files_changed"],
        json!(["docs/n.md", "lib/b.rs", "src/a.rs"]),
        "files_changed: sorted"
    );
    let hash = |name: &str| sha256_hex(&fs::read(root.join("caps").join(name)).expect("read"));
    assert_eq!(
        report["tests"][0],
        json!({
            "capture": "caps/suite.txt",
            "sha256": hash("suite.txt"),
            "command": "sh -c 'echo ok'",
            "exit": 0,
        })
    );
    assert_eq!(
        report["live_run"][0],
        json!({
            "capture": "caps/live.txt",
            "excerpt": "live",
            "sha256": hash("live.txt"),
            "command": "sh -c 'echo live'",
            "exit": 0,
        })
    );
    assert_eq!(
        report["tdd"][0]["red"],
        json!({
            "capture": "caps/red.txt",
            "sha256": hash("red.txt"),
            "command": "sh -c 'echo FAILED >&2; exit 1'",
            "exit": 1,
        })
    );
    assert_eq!(
        report["tdd"][0]["green"]["sha256"],
        json!(hash("green.txt"))
    );
    assert_eq!(
        report["fix_rounds"][0]["tests"][0]["sha256"],
        json!(hash("suite.txt"))
    );
    assert_eq!(
        report["citations"][0],
        json!({
            "path": "src/a.rs", "line": 2, "end": 3, "anchor": "alpha + 1",
            "sha256": sha256_hex(b"    let alpha = 1;\n    alpha + 1"),
        })
    );
    let audit = audit_result();
    assert_eq!(
        report["self_audit"],
        json!({
            "summary": audit["summary"],
            "rows": [audit["rules"][0], audit["rules"][2]],
        }),
        "self_audit: the summary as written and the deterministic rows in order"
    );
    assert_eq!(
        report["fix_rounds"][0]["commits"],
        json!([]),
        "without --fix-base the fix rounds stay as they are"
    );
}

/// Spec 5: the fields and keys the command does not fill stay as they are, in
/// their order; a key it fills keeps its place; a run gets `sha256`, `command`,
/// and `exit` after the keys it holds.
#[test]
fn the_other_fields_and_every_key_order_stay_as_they_are() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    write_json(root, "audit.json", &audit_result());
    write_json(root, "report.json", &draft());

    assert_eq!(
        facts_in(root, &s.base, &["--audit", "audit.json"])
            .status
            .code(),
        Some(0)
    );

    let report = read_json(root, "report.json");
    assert_eq!(keys(&report), keys(&draft()));
    for field in [
        "kind",
        "model",
        "task",
        "backlog",
        "status",
        "implemented",
        "docs_verified",
        "dependency_vetting",
        "coverage",
        "self_review",
        "concerns",
        "knowledge_used",
    ] {
        assert_eq!(report[field], draft()[field], "{field}");
    }
    assert_eq!(
        keys(&report["tests"][0]),
        ["capture", "sha256", "command", "exit"]
    );
    assert_eq!(
        keys(&report["live_run"][0]),
        ["capture", "excerpt", "sha256", "command", "exit"]
    );
    assert_eq!(
        keys(&report["citations"][0]),
        ["path", "line", "end", "anchor", "sha256"]
    );
    let text = fs::read_to_string(root.join("report.json")).expect("read the report");
    let parsed: Value = serde_json::from_str(&text).expect("parse the report");
    assert_eq!(
        text,
        format!(
            "{}\n",
            serde_json::to_string_pretty(&parsed).expect("serialize")
        ),
        "the report is written in the form every command prints"
    );
}

/// Case 3: a second run changes no byte.
#[test]
fn a_second_run_changes_no_byte() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    write_json(root, "audit.json", &audit_result());
    write_json(root, "report.json", &draft());
    let args = ["--audit", "audit.json"];
    assert_eq!(facts_in(root, &s.base, &args).status.code(), Some(0));
    let first = fs::read(root.join("report.json")).expect("read the report");

    let output = facts_in(root, &s.base, &args);

    assert_output(&output, 0, "report.json: facts filled\n", "");
    assert_eq!(
        fs::read(root.join("report.json")).expect("read the report"),
        first
    );
}

/// Case 3: a draft that lacks `commits` gets it at the end of the object, and
/// so do the other keys the command fills, in the order of the fill.
#[test]
fn a_key_the_draft_lacks_is_added_at_the_end_of_the_object() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    write_json(root, "audit.json", &audit_result());
    let mut report = draft();
    for key in ["commits", "files_changed", "self_audit"] {
        report.as_object_mut().expect("an object").remove(key);
    }
    write_json(root, "report.json", &report);

    assert_eq!(
        facts_in(root, &s.base, &["--audit", "audit.json"])
            .status
            .code(),
        Some(0)
    );

    let filled = read_json(root, "report.json");
    let mut expected: Vec<&str> = keys(&report);
    expected.extend(["commits", "files_changed", "self_audit"]);
    assert_eq!(keys(&filled), expected);
}

/// Spec 5: without `--audit` the `self_audit` field stays as it is.
#[test]
fn without_an_audit_the_self_audit_stays_as_it_is() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let mut report = draft();
    report["self_audit"] = json!({"summary": {"base": "a"}, "rows": []});
    write_json(root, "report.json", &report);

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    assert_eq!(
        read_json(root, "report.json")["self_audit"],
        json!({"summary": {"base": "a"}, "rows": []})
    );
}

/// Spec 5: `command` and `exit` always come from the header; a hash that the
/// report records and that matches stays.
#[test]
fn command_and_exit_come_from_the_header_and_a_matching_hash_stays() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let hash = sha256_hex(&fs::read(root.join("caps/red.txt")).expect("read"));
    let mut report = draft();
    report["tdd"][0]["red"] = json!({
        "capture": "caps/red.txt", "sha256": hash, "command": "typed by hand", "exit": 0,
    });
    write_json(root, "report.json", &report);

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    assert_eq!(
        read_json(root, "report.json")["tdd"][0]["red"],
        json!({
            "capture": "caps/red.txt",
            "sha256": hash,
            "command": "sh -c 'echo FAILED >&2; exit 1'",
            "exit": 1,
        })
    );
}

/// Spec 4: the `exit` of a run that a signal ended is `null`. Unix only: no
/// other platform ends a command with a signal.
#[cfg(unix)]
#[test]
fn the_exit_of_a_run_that_a_signal_ended_is_null() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    capture(root, "caps/killed.txt", "kill -9 $$");
    let mut report = draft();
    report["tests"][0] = json!({"capture": "caps/killed.txt"});
    write_json(root, "report.json", &report);

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    let run = &read_json(root, "report.json")["tests"][0];
    assert_eq!(run["exit"], Value::Null);
    assert_eq!(run["command"], json!("sh -c 'kill -9 $$'"));
}

/// Spec 5: `--fix-base` fills the commits of the last `fix_rounds` entry and no
/// other entry.
#[test]
fn fix_base_fills_the_commits_of_the_last_fix_round() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let mut report = draft();
    report["fix_rounds"] = json!([
        {"round": 1, "findings": [], "commits": [{"sha": "aaaaaaa", "subject": "kept"}], "tests": []},
        {"round": 2, "findings": [], "commits": [], "tests": []},
    ]);
    write_json(root, "report.json", &report);

    let output = facts_in(root, &s.base, &["--fix-base", &s.middle]);

    assert_output(&output, 0, "report.json: facts filled\n", "");
    let filled = read_json(root, "report.json");
    assert_eq!(
        filled["fix_rounds"][0]["commits"],
        json!([{"sha": "aaaaaaa", "subject": "kept"}])
    );
    assert_eq!(
        filled["fix_rounds"][1]["commits"],
        json!([{"sha": s.head, "subject": "fix: add b"}])
    );
    assert_eq!(filled["commits"].as_array().map(Vec::len), Some(2));
}

/// Spec 5: `files_changed` lists a deleted file, and a renamed file under both
/// of its paths, whatever the `diff.renames` setting of the person says.
#[test]
fn a_deleted_file_and_both_paths_of_a_rename_are_changed_files() {
    let s = scenario();
    let root = s.root();
    fs::rename(root.join("docs/old.md"), root.join("docs/renamed.md")).expect("rename a file");
    fs::remove_file(root.join("src/dup.rs")).expect("delete a file");
    commit(root, "refactor: rename and delete");
    make_captures(root);
    write_json(root, "report.json", &draft());

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    assert_eq!(
        read_json(root, "report.json")["files_changed"],
        json!([
            "docs/n.md",
            "docs/old.md",
            "docs/renamed.md",
            "lib/b.rs",
            "src/a.rs",
            "src/dup.rs"
        ])
    );
}

/// Review focus 5: a `--base` that is HEAD itself gives empty `commits` and
/// `files_changed`, and the command still exits 0.
#[test]
fn a_base_at_head_gives_empty_facts() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let mut report = draft();
    report["commits"] = json!([{"sha": "aaaaaaa", "subject": "stale"}]);
    report["files_changed"] = json!(["stale.rs"]);
    report["fix_rounds"][0]["commits"] = json!([{"sha": "bbbbbbb", "subject": "stale"}]);
    write_json(root, "report.json", &report);

    let output = facts_in(root, &s.head, &["--fix-base", &s.head]);

    assert_output(&output, 0, "report.json: facts filled\n", "");
    let filled = read_json(root, "report.json");
    assert_eq!(filled["commits"], json!([]));
    assert_eq!(filled["files_changed"], json!([]));
    assert_eq!(filled["fix_rounds"][0]["commits"], json!([]));
}

/// Spec 5: `<REPORT>` resolves against the working directory; the captures, the
/// citations, and git resolve against the install root, which `--dir` names.
#[test]
fn the_report_resolves_against_the_working_directory_and_the_rest_against_the_root() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    fs::create_dir(root.join("work")).expect("create work/");
    write_json(&root.join("work"), "report.json", &draft());
    let root_text = root.to_str().expect("a UTF-8 path");

    let output = facts_in(&root.join("work"), &s.base, &["--dir", root_text]);

    assert_output(&output, 0, "report.json: facts filled\n", "");
    let filled = read_json(&root.join("work"), "report.json");
    assert_eq!(filled["commits"].as_array().map(Vec::len), Some(2));
    assert_eq!(filled["tests"][0]["exit"], json!(0));
    assert!(filled["citations"][0]["sha256"].is_string());
}

/// Spec 5: the command prints the report as given.
#[test]
fn the_success_line_names_the_report_as_given() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    write_json(root, "out/report.json", &draft());

    let output = houserules()
        .args(["report-facts", "out/report.json", "--base", &s.base])
        .current_dir(root)
        .output()
        .expect("run report-facts");

    assert_output(&output, 0, "out/report.json: facts filled\n", "");
}

/// The write goes through a temporary file: no file is left behind, and the
/// report keeps its mode. Unix only: a mode is a Unix permission set.
#[cfg(unix)]
#[test]
fn the_write_leaves_no_file_behind_and_keeps_the_mode_of_the_report() {
    use std::os::unix::fs::PermissionsExt;

    let s = scenario();
    let root = s.root();
    make_captures(root);
    write_json(root, "report.json", &draft());
    fs::set_permissions(root.join("report.json"), fs::Permissions::from_mode(0o640))
        .expect("set the mode");
    let listing = names_in(root);

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    assert_eq!(names_in(root), listing);
    let mode = fs::metadata(root.join("report.json"))
        .expect("metadata")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
}

/// A directory that refuses a new file stops the run with one named line; the
/// report stays as it was. Unix only: a read-only directory is a Unix mode.
#[cfg(unix)]
#[test]
fn an_unwritable_directory_stops_the_run_with_a_named_line() {
    use std::os::unix::fs::PermissionsExt;

    let s = scenario();
    let root = s.root();
    make_captures(root);
    fs::create_dir(root.join("locked")).expect("create locked/");
    write_json(&root.join("locked"), "report.json", &draft());
    let locked = root.join("locked");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).expect("lock the directory");
    let Some(refusal) = fs::File::create(locked.join("probe")).err() else {
        // The process can write into a read-only directory (root): nothing to test.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
        return;
    };
    let before = fs::read(locked.join("report.json")).expect("read the report");

    let output = houserules()
        .args(["report-facts", "locked/report.json", "--base", &s.base])
        .current_dir(root)
        .output()
        .expect("run report-facts");

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).expect("unlock");
    assert_output(
        &output,
        2,
        "",
        &format!("locked/report.json: could not write: {refusal}\n"),
    );
    assert_eq!(
        fs::read(locked.join("report.json")).expect("read the report"),
        before
    );
}

// --- Case 3: the findings about a run ------------------------------------------

/// Spec 5: a capture that does not exist is a finding in each place that holds
/// a run, with the JSON path of the run as its label.
#[test]
fn a_missing_capture_is_a_finding_in_every_place_that_holds_a_run() {
    let s = scenario();
    make_captures(s.root());
    for (pointer, label) in RUN_PLACES {
        let mut report = draft();
        *report.pointer_mut(pointer).expect("a run") = json!({"capture": "caps/none.txt"});
        expect_findings(
            &s,
            &report,
            &[],
            &[format!("{label}: capture caps/none.txt does not exist")],
        );
    }
}

/// Spec 5: a file that is no capture is a finding with one of the four reasons.
#[test]
fn a_file_that_is_no_capture_is_a_finding_with_its_reason() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    write(root, "caps/empty.txt", "");
    write(root, "caps/plain.txt", "just text\n");
    write(
        root,
        "caps/short.txt",
        "{\"schema\":\"houserules/capture@1.0.0\"}\n",
    );
    write(
        root,
        "caps/other.txt",
        "{\"schema\":\"houserules/capture@2.0.0\"}\n",
    );
    for (file, reason) in [
        ("empty", "it is empty"),
        ("plain", "its first line is no JSON object"),
        ("short", "its header lacks argv"),
        (
            "other",
            "its schema is houserules/capture@2.0.0, not houserules/capture@1",
        ),
    ] {
        let mut report = draft();
        report["live_run"][0] = json!({"capture": format!("caps/{file}.txt")});
        expect_findings(
            &s,
            &report,
            &[],
            &[format!(
                "live_run[0]: caps/{file}.txt is no capture ({reason})"
            )],
        );
    }
}

/// Spec 5: a capture that the operating system refuses to read is a finding
/// that names the operating system's error. A directory in place of the file
/// is the refusal that every platform gives.
#[test]
fn a_capture_that_cannot_be_read_is_a_finding_with_the_os_error() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    fs::create_dir(root.join("caps/dir.txt")).expect("create a directory");
    let refusal = fs::read(root.join("caps/dir.txt")).expect_err("a directory is no file");
    let mut report = draft();
    report["tests"][0] = json!({"capture": "caps/dir.txt"});

    expect_findings(
        &s,
        &report,
        &[],
        &[format!(
            "tests[0]: capture caps/dir.txt could not be read ({refusal})"
        )],
    );
}

/// Spec 5: `report-facts` never replaces a recorded hash. A capture that
/// changed after its hash was recorded is a finding with both hashes.
#[test]
fn a_capture_changed_after_its_hash_was_recorded_is_a_finding() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let recorded = sha256_hex(&fs::read(root.join("caps/suite.txt")).expect("read"));
    capture(root, "caps/suite.txt", "echo changed");
    let actual = sha256_hex(&fs::read(root.join("caps/suite.txt")).expect("read"));
    let mut report = draft();
    report["tests"][0] = json!({"capture": "caps/suite.txt", "sha256": recorded});

    expect_findings(
        &s,
        &report,
        &[],
        &[format!(
            "tests[0]: capture caps/suite.txt has sha256 {actual}, but the report records \
             {recorded}; re-read the capture, then remove sha256 and run report-facts"
        )],
    );
}

/// Spec 5: the hash that is recorded and not a string cannot match a capture.
#[test]
fn a_recorded_hash_that_is_not_a_string_is_a_finding() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let actual = sha256_hex(&fs::read(root.join("caps/suite.txt")).expect("read"));
    let mut report = draft();
    report["tests"][0] = json!({"capture": "caps/suite.txt", "sha256": 7});

    expect_findings(
        &s,
        &report,
        &[],
        &[format!(
            "tests[0]: capture caps/suite.txt has sha256 {actual}, but the report records 7; \
             re-read the capture, then remove sha256 and run report-facts"
        )],
    );
}

/// Spec 5: a run without `capture` is left to `validate`; the command fills
/// nothing into it.
#[test]
fn a_run_without_a_capture_is_left_as_it_is() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let mut report = draft();
    report["tests"][0] = json!({"command": "typed", "exit": 0});
    write_json(root, "report.json", &report);

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    assert_eq!(
        read_json(root, "report.json")["tests"][0],
        json!({"command": "typed", "exit": 0})
    );
}

/// Spec 5 and case 3: every finding prints, in the order of the fill, and the
/// command writes nothing, also not the facts it could fill.
#[test]
fn every_finding_prints_in_the_order_of_the_fill_and_nothing_is_written() {
    let s = scenario();
    make_captures(s.root());
    let mut report = draft();
    report["tdd"][0]["green"] = json!({"capture": "caps/none.txt"});
    report["tests"][0] = json!({"capture": "caps/gone.txt"});
    report["citations"] = json!([
        {"path": "src/a.rs", "line": 1, "anchor": "fn first"},
        {"path": "nope.rs", "line": 1, "anchor": "nothing"},
        {"path": "src/a.rs", "line": 99, "anchor": "fn first"},
    ]);

    expect_findings(
        &s,
        &report,
        &[],
        &[
            "tests[0]: capture caps/gone.txt does not exist".to_string(),
            "tdd[0].green: capture caps/none.txt does not exist".to_string(),
            "citations[1]: nope.rs is not tracked at HEAD".to_string(),
            "citations[2]: src/a.rs has 6 lines at HEAD, fewer than 99".to_string(),
        ],
    );
}

// --- Case 3: the findings about a citation -------------------------------------

/// Runs `report-facts` over the draft with `citations` as its only citations and
/// requires the one finding `line`. The captures of the draft exist.
fn expect_citation_finding(s: &Scenario, citations: Value, line: &str) {
    let mut report = draft();
    report["citations"] = citations;
    expect_findings(s, &report, &[], &[line.to_string()]);
}

/// Spec 5: a path that is not tracked at HEAD is a finding: a file that exists
/// only in the working tree, a file that does not exist, a directory, a path
/// with a `..` segment, and an absolute path.
#[test]
fn a_path_that_is_not_a_blob_at_head_is_not_tracked() {
    let s = scenario();
    make_captures(s.root());
    write(s.root(), "scratch.txt", "never added\n");
    for path in [
        "scratch.txt",
        "src/none.rs",
        "src",
        "src/",
        "../a.txt",
        "src/../src/a.rs",
        "/src/a.rs",
    ] {
        expect_citation_finding(
            &s,
            json!([{"path": path, "line": 1, "anchor": "fn first"}]),
            &format!("citations[0]: {path} is not tracked at HEAD"),
        );
    }
}

/// Spec 5: a citation is judged against the file at HEAD, not the working tree.
#[test]
fn a_citation_is_judged_against_head_and_not_the_working_tree() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    write(root, "src/a.rs", "edited in the working tree\n");
    write_json(root, "report.json", &draft());

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    assert_eq!(
        read_json(root, "report.json")["citations"][0]["sha256"],
        json!(sha256_hex(b"    let alpha = 1;\n    alpha + 1"))
    );
}

/// Spec 5: lines past the end of the file are a finding that names the count
/// and the larger of `line` and `end`. A last line that ends in LF adds no
/// empty line, and an empty file has no line.
#[test]
fn lines_past_the_end_of_the_file_are_a_finding() {
    let s = scenario();
    make_captures(s.root());
    for (path, line, end, count, needed) in [
        ("src/a.rs", 9, None, 6, 9),
        ("src/a.rs", 7, None, 6, 7),
        ("src/a.rs", 2, Some(8), 6, 8),
        ("src/crlf.txt", 4, None, 3, 4),
        ("src/empty.txt", 1, None, 0, 1),
    ] {
        let mut entry = json!({"path": path, "line": line, "anchor": "fn first"});
        if let Some(end) = end {
            entry["end"] = json!(end);
        }
        expect_citation_finding(
            &s,
            json!([entry]),
            &format!("citations[0]: {path} has {count} lines at HEAD, fewer than {needed}"),
        );
    }
}

/// Spec 5: `end` before `line` is a finding.
#[test]
fn end_before_line_is_a_finding() {
    let s = scenario();
    make_captures(s.root());
    expect_citation_finding(
        &s,
        json!([{"path": "src/a.rs", "line": 4, "end": 2, "anchor": "fn first"}]),
        "citations[0]: end 2 is before line 4",
    );
}

/// Spec 5: an anchor that is not on the cited lines is a finding that names the
/// range: `<line>`, or `<line>-<end>` when `end` is greater than `line`.
#[test]
fn an_anchor_that_is_not_on_the_cited_lines_is_a_finding_with_the_range() {
    let s = scenario();
    make_captures(s.root());
    for (line, end, range) in [
        (1, None, "1"),
        (1, Some(1), "1"),
        (1, Some(2), "1-2"),
        (4, Some(6), "4-6"),
    ] {
        let mut entry = json!({"path": "src/a.rs", "line": line, "anchor": "alpha + 1"});
        if let Some(end) = end {
            entry["end"] = json!(end);
        }
        expect_citation_finding(
            &s,
            json!([entry]),
            &format!(
                "citations[0]: the anchor is not in src/a.rs:{range} at HEAD; re-open the file \
                 at HEAD and correct line and end"
            ),
        );
    }
}

/// Case 3: a draft line number that a later commit moved is a finding, whatever
/// the anchor: the anchor is no longer on the cited lines.
#[test]
fn a_line_number_that_a_later_commit_moved_is_a_finding() {
    let s = scenario();
    let root = s.root();
    let text = fs::read_to_string(root.join("src/a.rs")).expect("read a.rs");
    write(root, "src/a.rs", format!("// a new first line\n{text}"));
    commit(root, "refactor: add a line on top");
    make_captures(root);

    expect_citation_finding(
        &s,
        json!([{"path": "src/a.rs", "line": 2, "end": 3, "anchor": "alpha + 1"}]),
        "citations[0]: the anchor is not in src/a.rs:2-3 at HEAD; re-open the file at HEAD and \
         correct line and end",
    );
}

/// Spec 5: an anchor that occurs more than once in the file is a finding, also
/// when it is on the cited lines.
#[test]
fn an_anchor_that_occurs_twice_in_the_file_is_a_finding() {
    let s = scenario();
    make_captures(s.root());
    expect_citation_finding(
        &s,
        json!([{"path": "src/dup.rs", "line": 1, "anchor": "let"}]),
        "citations[0]: the anchor occurs 2 times in src/dup.rs at HEAD; choose text that \
         occurs once",
    );
}

/// An entry that holds no usable path, line, or anchor is left to `validate`;
/// the index of a finding is the index in the array.
#[test]
fn a_malformed_citation_is_skipped_and_the_index_counts_it() {
    let s = scenario();
    make_captures(s.root());
    expect_citation_finding(
        &s,
        json!([
            {"path": "src/a.rs", "line": 0, "anchor": "fn first"},
            {"path": 7, "line": 1, "anchor": "fn first"},
            {"path": "src/a.rs", "line": 1, "end": "x", "anchor": "fn first"},
            "text",
            {"path": "src/none.rs", "line": 1, "anchor": "fn first"},
        ]),
        "citations[4]: src/none.rs is not tracked at HEAD",
    );
}

/// Review focus 3: a report that cites the same lines twice with two anchors is
/// judged entry by entry. A hash that one entry records stays, and the other
/// entry gets its own.
#[test]
fn two_citations_of_one_range_keep_their_own_hashes() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let recorded = "0".repeat(64);
    let computed = sha256_hex(b"    let alpha = 1;\n    alpha + 1");
    for (first, second) in [
        (Some(recorded.as_str()), None),
        (None, Some(recorded.as_str())),
        (None, None),
    ] {
        let mut one = json!({"path": "src/a.rs", "line": 2, "end": 3, "anchor": "alpha + 1"});
        let mut two = json!({"path": "src/a.rs", "line": 2, "end": 3, "anchor": "let alpha"});
        if let Some(hash) = first {
            one["sha256"] = json!(hash);
        }
        if let Some(hash) = second {
            two["sha256"] = json!(hash);
        }
        let mut report = draft();
        report["citations"] = json!([one, two]);
        write_json(root, "report.json", &report);

        let output = facts_in(root, &s.base, &[]);

        assert_output(&output, 0, "report.json: facts filled\n", "");
        let filled = read_json(root, "report.json");
        assert_eq!(
            filled["citations"][0]["sha256"],
            json!(first.map_or(computed.as_str(), |hash| hash)),
            "the first entry, {first:?} {second:?}"
        );
        assert_eq!(
            filled["citations"][1]["sha256"],
            json!(second.map_or(computed.as_str(), |hash| hash)),
            "the second entry, {first:?} {second:?}"
        );
    }
}

/// Spec 4 and case 3: the hash covers the cited lines from the first byte of
/// the first line to the last byte of the last line, without the LF that ends
/// the last line. A CR before the LF stays, and a last line with no LF is a line.
#[test]
fn the_hash_covers_the_cited_lines_and_keeps_the_cr() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    for (line, end, anchor, expected) in [
        (1, None, "one", "one\r"),
        (1, Some(2), "one", "one\r\ntwo\r"),
        (2, Some(3), "two", "two\r\nthree"),
        (3, None, "three", "three"),
        (1, Some(3), "one", "one\r\ntwo\r\nthree"),
    ] {
        let mut entry = json!({"path": "src/crlf.txt", "line": line, "anchor": anchor});
        if let Some(end) = end {
            entry["end"] = json!(end);
        }
        let mut report = draft();
        report["citations"] = json!([entry]);
        write_json(root, "report.json", &report);

        assert_eq!(
            facts_in(root, &s.base, &[]).status.code(),
            Some(0),
            "{line} {end:?}"
        );

        assert_eq!(
            read_json(root, "report.json")["citations"][0]["sha256"],
            json!(sha256_hex(expected.as_bytes())),
            "{line} {end:?}"
        );
    }
}

/// Case 3: the floor of the anchor is `validate`'s. `report-facts` fills the
/// hash of an anchor `{` that occurs once, and `validate` of the filled report
/// refuses the anchor.
#[test]
fn a_short_anchor_passes_report_facts_and_fails_validate() {
    let s = scenario();
    let root = s.root();
    write(root, "src/one.rs", "fn main()\n{\n}\n");
    commit(root, "chore: add one");
    make_captures(root);
    let mut report = draft();
    report["citations"] = json!([{"path": "src/one.rs", "line": 2, "anchor": "{"}]);
    write_json(root, "report.json", &report);

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    let filled = read_json(root, "report.json");
    assert_eq!(filled["citations"][0]["sha256"], json!(sha256_hex(b"{")));
    let output = houserules()
        .args(["validate", "report.json"])
        .current_dir(root)
        .output()
        .expect("run validate");
    let errors = read_validate_errors(&stdout(&output));
    assert_eq!(output.status.code(), Some(1), "{errors:?}");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].ends_with(".citations[0].anchor: must match \\S{3}"),
        "{errors:?}"
    );
}

// --- Review focus 4 and the subdirectory install ------------------------------

/// Review focus 4: a capture path with a space and a non-ASCII character in a
/// folder name, cited relative to the install root, resolves; `command` quotes
/// the path, which the `argv` of the capture held.
#[test]
fn a_capture_path_with_a_space_and_a_non_ascii_character_resolves() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let out = "caps dir/\u{fc}n\u{ef}/run.txt";
    let input = "caps dir/\u{fc}n\u{ef}/in.txt";
    write(root, input, "input\n");
    fs::create_dir_all(root.join("caps dir/\u{fc}n\u{ef}")).expect("create the folder");
    let status = houserules()
        .current_dir(root)
        .args([
            "capture",
            "--out",
            out,
            "--",
            "sh",
            "-c",
            "echo \"$1\"",
            "sh",
            input,
        ])
        .output()
        .expect("run capture")
        .status;
    assert_eq!(status.code(), Some(0));
    let mut report = draft();
    report["tests"][0] = json!({"capture": out});
    report["live_run"][0] = json!({"capture": "caps dir/\u{fc}n\u{ef}/none.txt"});
    expect_findings(
        &s,
        &report,
        &[],
        &["live_run[0]: capture caps dir/\u{fc}n\u{ef}/none.txt does not exist".to_string()],
    );
    report["live_run"] = json!([]);
    write_json(root, "report.json", &report);

    assert_eq!(facts_in(root, &s.base, &[]).status.code(), Some(0));

    let run = &read_json(root, "report.json")["tests"][0];
    assert_eq!(
        run["command"],
        json!("sh -c 'echo \"$1\"' sh 'caps dir/\u{fc}n\u{ef}/in.txt'")
    );
    assert_eq!(
        run["sha256"],
        json!(sha256_hex(&fs::read(root.join(out)).expect("read")))
    );
}

/// An install below the git top level. The base commit holds the kit that
/// `init` wrote into `sub/`, a file `a.txt` at the top level, and a file `a.txt`
/// in `sub/`; the next commit changes both and adds a file outside the install.
fn subdirectory_install() -> Scenario {
    let dir = scratch_git_repo();
    let root = dir.path();
    write(root, ".gitattributes", "* -text\n");
    write(root, "a.txt", "top level file\n");
    write(root, "sub/a.txt", "inner one\ninner two\n");
    fs::create_dir_all(root.join("sub")).expect("create sub/");
    run_ok(&root.join("sub"), &["init"]);
    let base = commit(root, "chore: seed");
    write(root, "a.txt", "top level file, changed\n");
    write(root, "sub/a.txt", "inner one\ninner two\ninner three\n");
    write(root, "other.txt", "outside the install\n");
    let middle = commit(root, "feat: change both");
    write(root, "outside-only.txt", "x\n");
    let head = commit(root, "fix: touch the outside only");
    Scenario {
        dir,
        base,
        middle,
        head,
    }
}

/// Spec 5, C5: in a subdirectory install a cited file is read as `git show
/// HEAD:./<path>` at the install root, so `a.txt` is the file of the install and
/// not the file of the same name at the git top level; `files_changed` holds the
/// install-root paths of the install only; a path above the install is not
/// tracked; every commit of the range counts.
#[test]
fn a_subdirectory_install_reads_its_own_files() {
    let s = subdirectory_install();
    let install = s.root().join("sub");
    capture(&install, "caps/suite.txt", "echo ok");
    let mut report = draft();
    report["tests"] = json!([{"capture": "caps/suite.txt"}]);
    report["live_run"] = json!([]);
    report["tdd"] = json!([]);
    report["fix_rounds"] = json!([]);
    report["citations"] = json!([{"path": "a.txt", "line": 1, "end": 2, "anchor": "inner one"}]);
    write_json(&install, "report.json", &report);

    let output = facts_in(&install, &s.base, &[]);

    assert_output(&output, 0, "report.json: facts filled\n", "");
    let filled = read_json(&install, "report.json");
    assert_eq!(
        filled["citations"][0]["sha256"],
        json!(sha256_hex(b"inner one\ninner two"))
    );
    assert_eq!(filled["files_changed"], json!(["a.txt"]));
    assert_eq!(
        filled["commits"],
        json!([
            {"sha": s.middle, "subject": "feat: change both"},
            {"sha": s.head, "subject": "fix: touch the outside only"},
        ])
    );

    report["citations"] = json!([
        {"path": "a.txt", "line": 1, "anchor": "top level"},
        {"path": "../a.txt", "line": 1, "anchor": "top level"},
        {"path": "sub", "line": 1, "anchor": "inner"},
    ]);
    write_json(&install, "report.json", &report);
    let output = facts_in(&install, &s.base, &[]);
    assert_output(
        &output,
        1,
        "",
        "citations[0]: the anchor is not in a.txt:1 at HEAD; re-open the file at HEAD and \
         correct line and end\n\
         citations[1]: ../a.txt is not tracked at HEAD\n\
         citations[2]: sub is not tracked at HEAD\n",
    );
}

// --- Setup errors -------------------------------------------------------------

/// Spec 5: each setup error prints one line, exits 2, and writes nothing.
#[test]
fn a_setup_error_prints_one_line_and_exits_2() {
    let s = scenario();
    let root = s.root();
    make_captures(root);
    let base = s.base.as_str();
    let good = draft();

    let missing = fs::read_to_string(root.join("nope.json")).expect_err("no such file");
    expect_setup_error(
        &s,
        None,
        &["nope.json", "--base", base],
        &format!("nope.json: {missing}"),
    );

    write(root, "broken.json", "{");
    let broken = serde_json::from_str::<Value>("{").expect_err("broken JSON");
    expect_setup_error(
        &s,
        None,
        &["broken.json", "--base", base],
        &format!("broken.json: {broken}"),
    );

    for not_a_report in [
        json!({"kind": "task-review"}),
        json!([]),
        json!({"task": 5}),
    ] {
        expect_setup_error(
            &s,
            Some(&not_a_report),
            &["report.json", "--base", base],
            "report.json: kind is not task-report",
        );
    }

    expect_setup_error(
        &s,
        Some(&good),
        &["report.json", "--base", "no-such-ref"],
        "--base no-such-ref: not a commit",
    );
    expect_setup_error(
        &s,
        Some(&good),
        &["report.json", "--base", base, "--fix-base", "no-such-ref"],
        "--fix-base no-such-ref: not a commit",
    );
    let mut without_rounds = draft();
    without_rounds
        .as_object_mut()
        .expect("an object")
        .remove("fix_rounds");
    for report in [without_rounds, {
        let mut empty = draft();
        empty["fix_rounds"] = json!([]);
        empty
    }] {
        expect_setup_error(
            &s,
            Some(&report),
            &["report.json", "--base", base, "--fix-base", &s.middle],
            "--fix-base: the report has no fix_rounds entry",
        );
    }

    let absent = fs::read_to_string(root.join("audit.json")).expect_err("no such file");
    expect_setup_error(
        &s,
        Some(&good),
        &["report.json", "--base", base, "--audit", "audit.json"],
        &format!("--audit audit.json: {absent}"),
    );
    write(root, "audit.json", "[");
    let unparsable = serde_json::from_str::<Value>("[").expect_err("broken JSON");
    expect_setup_error(
        &s,
        Some(&good),
        &["report.json", "--base", base, "--audit", "audit.json"],
        &format!("--audit audit.json: {unparsable}"),
    );
    for no_result in [
        json!({}),
        json!({"summary": {}}),
        json!({"summary": 1, "rules": []}),
        json!({"rules": []}),
        json!({"summary": 1, "rules": {}}),
        json!({"summary": {}, "rules": "none"}),
    ] {
        write_json(root, "audit.json", &no_result);
        expect_setup_error(
            &s,
            Some(&good),
            &["report.json", "--base", base, "--audit", "audit.json"],
            "--audit audit.json: it is no audit result (it needs a summary object and a rules array)",
        );
    }
}

/// The first line of the error output of `git <args>` in `dir`, reduced as
/// `report-facts` reduces it: `LC_ALL=C`, the first line that is not blank,
/// trimmed. The call must fail.
fn first_git_error_line(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .env("LC_ALL", "C")
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(!output.status.success(), "git {args:?} must fail");
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .expect("a line of error output")
        .to_string()
}

/// Deletes the loose object `sha` of the repository at `root`, so that git
/// cannot read it.
fn delete_loose_object(root: &Path, sha: &str) {
    let object = root
        .join(".git")
        .join("objects")
        .join(&sha[..2])
        .join(&sha[2..]);
    fs::remove_file(&object).expect("delete the loose object");
}

/// Spec 5: a `git log` of `<base>..HEAD` that fails is a setup error. The line
/// is `report-facts: git log failed: ` and the first line of the error output of
/// git, with exit status 2, nothing on stdout, and the report as it was. Here
/// the second commit of the range is gone.
#[test]
fn a_git_log_that_fails_is_a_setup_error_with_the_line_of_git() {
    let s = scenario();
    let root = s.root();
    delete_loose_object(root, &s.middle);
    let range = format!("{}..HEAD", s.base);
    let git_line = first_git_error_line(root, &["log", &range]);
    assert!(
        git_line.contains(&s.middle),
        "git names the lost commit: {git_line}"
    );

    expect_setup_error(
        &s,
        Some(&json!({"kind": "task-report"})),
        &["report.json", "--base", &s.base],
        &format!("report-facts: git log failed: {git_line}"),
    );
}

/// Spec 5: a `git diff` of `<base>..HEAD` that fails is a setup error. The line
/// is `report-facts: git diff failed: ` and the first line of the error output
/// of git, with exit status 2, nothing on stdout, and the report as it was. Here
/// the tree of HEAD is gone, which `git log` does not read.
#[test]
fn a_git_diff_that_fails_is_a_setup_error_with_the_line_of_git() {
    let s = scenario();
    let root = s.root();
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]).trim().to_string();
    delete_loose_object(root, &tree);
    let range = format!("{}..HEAD", s.base);
    let git_line = first_git_error_line(root, &["diff", "--name-only", &range]);
    assert!(
        git_line.contains(&tree),
        "git names the lost tree: {git_line}"
    );

    expect_setup_error(
        &s,
        Some(&json!({"kind": "task-report"})),
        &["report.json", "--base", &s.base],
        &format!("report-facts: git diff failed: {git_line}"),
    );
}

/// Spec 5: the setup errors come in one order, and the first one is the only
/// line: the report is read before its kind, the kind before `--base`, `--base`
/// before `--fix-base`, and `--fix-base` before the `fix_rounds` test and
/// `--audit`.
#[test]
fn the_first_setup_error_is_the_only_line() {
    let s = scenario();
    let root = s.root();
    let missing = fs::read_to_string(root.join("nope.json")).expect_err("no such file");
    expect_setup_error(
        &s,
        None,
        &[
            "nope.json",
            "--base",
            "no-such-ref",
            "--fix-base",
            "no-such-ref",
            "--audit",
            "nope",
        ],
        &format!("nope.json: {missing}"),
    );
    expect_setup_error(
        &s,
        Some(&json!({"kind": "task-review"})),
        &["report.json", "--base", "no-such-ref"],
        "report.json: kind is not task-report",
    );
    expect_setup_error(
        &s,
        Some(&draft()),
        &[
            "report.json",
            "--base",
            "no-such-ref",
            "--fix-base",
            "no-such-ref",
        ],
        "--base no-such-ref: not a commit",
    );
    let mut without_rounds = draft();
    without_rounds
        .as_object_mut()
        .expect("an object")
        .remove("fix_rounds");
    expect_setup_error(
        &s,
        Some(&without_rounds),
        &[
            "report.json",
            "--base",
            &s.base,
            "--fix-base",
            "no-such-ref",
            "--audit",
            "nope",
        ],
        "--fix-base no-such-ref: not a commit",
    );
    expect_setup_error(
        &s,
        Some(&without_rounds),
        &[
            "report.json",
            "--base",
            &s.base,
            "--fix-base",
            &s.middle,
            "--audit",
            "nope",
        ],
        "--fix-base: the report has no fix_rounds entry",
    );
}

/// A `--base` is a commit: a ref that names a tree, or a text that git reads as an
/// option, is no commit.
#[test]
fn a_base_must_name_a_commit() {
    let s = scenario();
    let tree = format!("{}^{{tree}}", s.head);
    for base in [tree.as_str(), "--help", "-1"] {
        let root = s.root();
        write_json(root, "report.json", &draft());
        let output = houserules()
            .arg("report-facts")
            .arg("report.json")
            .arg(format!("--base={base}"))
            .current_dir(root)
            .output()
            .expect("run report-facts");
        assert_output(&output, 2, "", &format!("--base {base}: not a commit\n"));
    }
}

/// Spec 5: an explicit `--dir` that holds an install and is no repository is
/// named, and the report is not read.
#[test]
fn a_dir_that_is_no_repository_is_named() {
    let s = scenario();
    let dir = tempfile::tempdir().expect("tempdir");
    fs::copy(
        s.root().join(".houserules.json"),
        dir.path().join(".houserules.json"),
    )
    .expect("copy the marker");
    let output = houserules()
        .args(["report-facts", "nope.json", "--base", "HEAD", "--dir"])
        .arg(dir.path())
        .output()
        .expect("run report-facts");
    assert_output(
        &output,
        2,
        "",
        &format!("{} is not inside a git repository\n", dir.path().display()),
    );
}

/// Case 3: the base of the task is required.
#[test]
fn a_call_without_a_base_is_a_usage_error() {
    let s = scenario();
    let output = houserules()
        .args(["report-facts", "report.json"])
        .current_dir(s.root())
        .output()
        .expect("run report-facts");
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--base <REF>"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// --- The result validates -------------------------------------------------------

/// Case 3: the result validates. The fixture report of the new form
/// (`tests/fixtures/report-36/`) loses its facts, `report-facts` fills them
/// from a scratch install, and `validate` accepts the report.
#[test]
fn the_filled_report_validates() {
    let s = scenario();
    let root = s.root();
    write(root, "src/fixture.rs", "// a\n// b\nfn main() {\n}\n");
    let base = commit(root, "chore: install and add a fixture file");
    write(
        root,
        "src/fixture.rs",
        "// a\n// b\nfn main() {\n    run();\n}\n",
    );
    commit(root, "feat: change the fixture file");
    let fixture = tracker_support::repo_root().join("tests/fixtures/report-36");
    tracker_support::copy_dir(&fixture.join("captures"), &root.join("captures"));
    let mut report: Value =
        serde_json::from_slice(&fs::read(fixture.join("report.json")).expect("read"))
            .expect("parse");
    for pointer in [
        "/tests/0",
        "/live_run/0",
        "/tdd/0/red",
        "/tdd/0/green",
        "/fix_rounds/0/tests/0",
    ] {
        let run = report
            .pointer_mut(pointer)
            .and_then(Value::as_object_mut)
            .expect("a run");
        for key in ["sha256", "command", "exit"] {
            run.remove(key);
        }
    }
    report["commits"] = json!([]);
    report["files_changed"] = json!([]);
    report["self_audit"] = Value::Null;
    report["citations"] =
        json!([{"path": "src/fixture.rs", "line": 3, "end": 4, "anchor": "fn main"}]);
    write_json(root, "report.json", &report);
    let fixture_audit = json!({
        "summary": read_json(&fixture, "report.json")["self_audit"]["summary"],
        "rules": read_json(&fixture, "report.json")["self_audit"]["rows"],
    });
    write_json(root, "audit.json", &fixture_audit);

    let output = facts_in(root, &base, &["--audit", "audit.json"]);
    assert_output(&output, 0, "report.json: facts filled\n", "");

    let validated = houserules()
        .args(["validate", "report.json"])
        .current_dir(root)
        .output()
        .expect("run validate");
    assert_eq!(
        (
            validated.status.code(),
            read_validate_errors(&stdout(&validated))
        ),
        (Some(0), Vec::<String>::new())
    );
}

/// The errors of the one result row that `validate` printed.
fn read_validate_errors(text: &str) -> Vec<String> {
    let rows: Value = serde_json::from_str(text).expect("validate prints JSON");
    rows[0]["errors"]
        .as_array()
        .expect("errors is an array")
        .iter()
        .map(|error| error.as_str().expect("an error is a string").to_string())
        .collect()
}

// --- The help text ---------------------------------------------------------------

/// The help of the command names the fields it fills, its options, its
/// findings, and its exit statuses. Clap wraps the lines of the help, so the
/// statements are compared on the text with each run of white space folded to
/// one space. The test names every statement that is missing.
#[test]
fn the_help_names_the_fields_the_options_and_the_exit_statuses() {
    let output = houserules()
        .args(["report-facts", "--help"])
        .output()
        .expect("run --help");
    assert_eq!(output.status.code(), Some(0));
    let help = stdout(&output)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let missing: Vec<&str> = [
        "Fills the fact fields of a task report",
        "`commits`",
        "`files_changed`",
        "`git diff --name-only --no-renames <base>..HEAD`",
        "`self_audit`",
        "`sha256`",
        "`command` and `exit`",
        "`citations`",
        "git show HEAD:./<path>",
        "never replaces a recorded `sha256`",
        "<REPORT>: facts filled",
        "exits 1",
        "exits 2",
        "git cannot list the commits or the changed files",
        "--base <REF>",
        "--fix-base <REF>",
        "--audit <FILE>",
        "--dir <DIR>",
    ]
    .into_iter()
    .filter(|statement| !help.contains(statement))
    .collect();
    assert!(missing.is_empty(), "the help lacks {missing:?}:\n{help}");
}
