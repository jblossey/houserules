//! `houserules evidence` (batch 37, spec 5; acceptance case 1; the packet half
//! of case 4; review focus 5; HR-185 and HR-246). Each case runs the compiled
//! binary against a scratch repository whose install holds the areas `docs`,
//! `code`, and `ops`, and reads the packet or the document whole. A line of the
//! spec is the literal of a test.
//!
//! Every fixture is hermetic: the install is written by hand, so the audit that
//! `evidence` runs in process reads only the entries that a case lists.

mod help_support;
mod schema_support;
mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use help_support::{help_of, help_of_args, states};
use schema_support::{assert_valid, read_json as read_file_json};
use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    add_submodule, assert_output, commit_all, copy_dir, edit_marker, git, houserules, repo_root,
    scratch_git_repo, stderr, stdout,
};

/// The schema file of the evidence document, relative to the repository root.
const EVIDENCE_SCHEMA: &str = "schemas/evidence/1.0.0.json";

/// The schema file of the route decision, relative to the repository root.
const ROUTE_SCHEMA: &str = "schemas/route/1.0.0.json";

/// The most characters of a packet (spec 5.8).
const PACKET_LIMIT: usize = 60_000;

/// The line that follows the items of the uncovered surface (spec 5.4).
const COVERAGE_LINE: &str =
    "Coverage is per file: a file is covered when a check that declares covers: true read it.";

/// The name of git's empty tree for a repository that hashes with SHA-1.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

// ---- fixtures -------------------------------------------------------------------------

/// The areas of every fixture.
fn areas() -> Value {
    json!({
        "global": {"paths": []},
        "docs": {"paths": ["docs/**"]},
        "code": {"paths": ["src/**"]},
        "ops": {"paths": ["ops/**"]},
    })
}

/// A knowledge entry of the `global` area whose check is `check`. The entry is
/// standing, so every audit has it in its package.
fn entry(id: &str, check: Value) -> Value {
    json!({
        "id": id, "kind": "rule", "area": "global", "standing": true,
        "summary": "A rule.", "body": ["Body."], "tags": [],
        "source": {"date": "2026-10-24", "by": "test"},
        "check": check,
    })
}

/// A knowledge entry with no check: its row is judged.
fn judged_entry(id: &str) -> Value {
    json!({
        "id": id, "kind": "rule", "area": "global", "standing": true,
        "summary": "A judged rule.", "body": ["Body."], "tags": [],
        "source": {"date": "2026-10-24", "by": "test"},
    })
}

/// A `grep-absent` check over the changed files that match `files`.
fn grep(files: &str, pattern: &str, level: &str, covers: bool) -> Value {
    json!({
        "type": "grep-absent", "level": level, "files": [files],
        "pattern": pattern, "scope": "changed", "covers": covers,
    })
}

/// The entry `team.docs-read`: a covering check that passes on any change to a
/// Markdown file of `docs/`.
fn docs_read() -> Value {
    entry(
        "team.docs-read",
        grep("docs/*.md", "ZZNEVERZZ", "fail", true),
    )
}

/// `count` numbered lines, each starting with `prefix`.
fn numbered(prefix: &str, count: usize) -> String {
    (1..=count)
        .map(|number| format!("{prefix} {number}\n"))
        .collect()
}

/// The file `relative` below `dir`, built component by component
/// (`houserules.path-pins-mirror-the-code`).
fn file(dir: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(dir.to_path_buf(), |path, part| path.join(part))
}

/// A scratch repository with one install and its base commit.
struct Install {
    /// The git top level.
    top: TempDir,
    /// The folder of the install below the top level: `""` or `"sub"`.
    folder: &'static str,
    /// A directory outside the repository, for the files that a case reads back.
    scratch: TempDir,
}

impl Install {
    /// A repository whose install in `folder` holds `entries` in the topic
    /// `team`, the marker with the `review` key `review`, and the files
    /// `docs/guide.md` (10 lines), `src/lib.rs` (10 lines), and `ops/deploy.sh`
    /// (5 lines). The base commit holds them.
    fn new(folder: &'static str, entries: &[Value], review: Option<Value>) -> Self {
        let install = Self {
            top: scratch_git_repo(),
            folder,
            scratch: tempfile::tempdir().expect("tempdir"),
        };
        let mut marker = json!({"schema": "houserules/install@2.3.0", "idPrefix": "WI"});
        if let Some(review) = review {
            marker["review"] = review;
        }
        install.write(
            ".houserules.json",
            &serde_json::to_string_pretty(&marker).expect("serialize the marker"),
        );
        install.write("knowledge/schema.json", "{}\n");
        install.write("knowledge/areas.json", &areas().to_string());
        install.write(
            "knowledge/team.json",
            &json!({"entries": entries}).to_string(),
        );
        install.write("docs/guide.md", &numbered("guide", 10));
        install.write("src/lib.rs", &numbered("lib", 10));
        install.write("ops/deploy.sh", &numbered("deploy", 5));
        install.commit("chore: base");
        install
    }

    /// The install root.
    fn root(&self) -> PathBuf {
        if self.folder.is_empty() {
            self.top.path().to_path_buf()
        } else {
            self.top.path().join(self.folder)
        }
    }

    /// Writes `content` to `path` below the install root, with its folders.
    fn write(&self, path: &str, content: &str) {
        let target = file(&self.root(), path);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write the file");
    }

    /// Writes `content` to `path` below the install root.
    fn write_bytes(&self, path: &str, content: &[u8]) {
        let target = file(&self.root(), path);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write the file");
    }

    /// Adds `count` lines to the end of `path` below the install root.
    fn append(&self, path: &str, count: usize) {
        let target = file(&self.root(), path);
        let mut text = fs::read_to_string(&target).unwrap_or_default();
        text.push_str(&numbered("added", count));
        self.write(path, &text);
    }

    /// Commits every change and gives the sha of the new commit.
    fn commit(&self, message: &str) -> String {
        commit_all(self.top.path(), message)
    }

    /// The full sha of `HEAD`.
    fn head(&self) -> String {
        git(self.top.path(), &["rev-parse", "HEAD"])
            .trim()
            .to_string()
    }

    /// A path in the scratch directory outside the repository.
    fn scratch_file(&self, name: &str) -> PathBuf {
        self.scratch.path().join(name)
    }

    /// Gives the install the JSON tracker with the backlog of the `mini`
    /// fixture, whose item `HR-901` has the title `Write the mini-tools smoke
    /// test`, and commits it.
    fn use_json_tracker(&self) {
        edit_marker(&self.root(), |marker| {
            marker.insert("idPrefix".to_string(), json!("HR"));
            marker.insert("tracker".to_string(), json!({"adapter": "json"}));
        });
        copy_dir(
            &repo_root().join("tests/fixtures/mini/backlog"),
            &self.root().join("backlog"),
        );
        self.commit("chore: add the backlog");
    }

    /// Runs `houserules evidence <args> --dir <root>`.
    fn evidence(&self, args: &[&str]) -> Output {
        houserules()
            .arg("evidence")
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run evidence")
    }

    /// The packet that `evidence <args>` prints. The run exits 0 and prints
    /// nothing on stderr.
    fn packet(&self, args: &[&str]) -> String {
        let output = self.evidence(args);
        assert_output(&output, 0, &stdout(&output), "");
        stdout(&output)
    }

    /// The packet and the document of the range from `base`.
    fn packet_and_document(&self, base: &str, extra: &[&str]) -> (String, Value) {
        let target = self.scratch_file("evidence.json");
        let mut args = vec!["--base", base, "--json", target.to_str().expect("UTF-8")];
        args.extend_from_slice(extra);
        let packet = self.packet(&args);
        (packet, read_file_json(&target))
    }

    /// Writes the `audit --json` result of `base` to `HEAD` into the scratch
    /// directory, with the `--sanctioned` bookings `sanctioned`, and gives its
    /// path. The audit may exit 1 for a failed row; the file is written then too.
    fn audit_file(&self, base: &str, sanctioned: &[&str]) -> PathBuf {
        let target = self.scratch_file("audit.json");
        let mut command = houserules();
        command.args(["audit", "--base", base, "--json"]);
        command.arg(&target);
        for booking in sanctioned {
            command.args(["--sanctioned", booking]);
        }
        command.arg("--dir").arg(self.root());
        let output = command.output().expect("run audit");
        assert!(target.exists(), "audit wrote no file: {}", stderr(&output));
        target
    }
}

/// The first seven characters of `sha`.
fn short(sha: &str) -> &str {
    &sha[..7]
}

/// The `Range` line of a packet whose document is `document`.
fn range_line(document: &Value, route: &str) -> String {
    let text = |value: &Value| value.as_str().expect("a string").to_string();
    let hash = text(&document["rules"]["head_hash"]);
    format!(
        "Range {}..{} (merge base {}) · rules {} · route {route}",
        short(&text(&document["base"])),
        short(&text(&document["head"])),
        short(&text(&document["merge_base"])),
        &hash[..12]
    )
}

/// The lines of the section `heading` of `packet`: those after the heading line
/// up to the next `### ` heading, without the blank line before the next one.
fn section<'a>(packet: &'a str, heading: &str) -> Vec<&'a str> {
    let mut lines: Vec<&str> = packet
        .lines()
        .skip_while(|line| !line.starts_with(heading))
        .skip(1)
        .take_while(|line| !line.starts_with("### "))
        .collect();
    while lines.last() == Some(&"") {
        lines.pop();
    }
    lines
}

// ---- the packet ------------------------------------------------------------------------

/// The whole packet of an unconfigured docs change, byte for byte: the marker
/// line, the heading, the range line, and the sections in the order of spec 5
/// (the uncovered surface first, then the intent).
#[test]
fn an_unconfigured_change_prints_the_whole_packet() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.commit("docs: edit the guide");

    let (packet, document) = install.packet_and_document(&base, &[]);

    let range = range_line(&document, "owner-review");
    assert_eq!(
        packet,
        format!(
            "<!-- houserules:evidence . -->\n\
             ## houserules evidence\n\
             \n\
             {range}\n\
             \n\
             ### Uncovered surface (3 of 3 changed lines)\n\
             \n\
             - `docs/guide.md`: lines 11-13\n\
             \n\
             {COVERAGE_LINE}\n\
             \n\
             ### Intent (0)\n\
             \n\
             None recorded.\n\
             \n\
             ### Failed (0)\n\
             \n\
             None.\n\
             \n\
             ### Warned (0)\n\
             \n\
             None.\n\
             \n\
             ### Proven (0)\n\
             \n\
             None.\n\
             \n\
             ### Judged only (0)\n\
             \n\
             None.\n\
             \n\
             0 deterministic checks did not apply to this change.\n\
             \n\
             ### Size and links\n\
             \n\
             Diff: +3 -0 lines in 1 files\n\
             Work items: none\n\
             \n\
             ### Route\n\
             \n\
             owner-review\n\
             \n\
             - area `docs`: owner-review\n"
        )
    );
}

/// The same range gives the same bytes: the packet and the document (spec 5.8).
#[test]
fn the_same_range_gives_the_same_bytes_twice() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("misc.txt", "stray\n");
    install.commit("chore: change several areas");
    let first = install.scratch_file("first.json");
    let second = install.scratch_file("second.json");

    let first_packet = install.packet(&["--base", &base, "--json", first.to_str().expect("UTF-8")]);
    let second_packet =
        install.packet(&["--base", &base, "--json", second.to_str().expect("UTF-8")]);

    assert_eq!(first_packet, second_packet);
    assert_eq!(
        fs::read(&first).expect("read the first"),
        fs::read(&second).expect("read the second")
    );
}

/// With a check that declares `covers: true`, passing on a file, the file is
/// covered: the uncovered surface is empty and the rule is under Proven.
#[test]
fn a_covering_check_that_passed_covers_the_file() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.commit("docs: edit the guide");

    let (packet, document) = install.packet_and_document(&base, &[]);

    assert_eq!(
        section(&packet, "### Uncovered surface (0 of 3 changed lines)"),
        ["", "None."]
    );
    let proven = document["proven"][0]["evidence"]
        .as_str()
        .expect("evidence");
    assert_eq!(
        section(&packet, "### Proven (1)"),
        ["", &format!("- `team.docs-read`: `{proven}`")]
    );
    assert_eq!(document["uncovered"]["paths"], json!([]));
}

/// A covering check that failed covers nothing: the file stays uncovered, and
/// the rule is listed under Failed (spec 4.4, per row).
#[test]
fn a_covering_check_that_failed_leaves_the_file_uncovered_and_is_listed_under_failed() {
    let failing = entry("team.docs-read", grep("docs/*.md", "added", "fail", true));
    let install = Install::new("", &[failing], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.commit("docs: edit the guide");

    let (packet, document) = install.packet_and_document(&base, &[]);

    assert_eq!(
        section(&packet, "### Uncovered surface (3 of 3 changed lines)"),
        ["", "- `docs/guide.md`: lines 11-13", "", COVERAGE_LINE]
    );
    let failed = document["failed"][0]["evidence"]
        .as_str()
        .expect("evidence");
    assert_eq!(
        section(&packet, "### Failed (1)"),
        ["", &format!("- `team.docs-read`: `{failed}`")]
    );
}

/// A check that does not declare `covers` covers nothing, whatever its result.
#[test]
fn a_check_that_does_not_declare_covers_leaves_every_hunk_uncovered() {
    let reading = entry(
        "team.docs-read",
        grep("docs/*.md", "ZZNEVERZZ", "fail", false),
    );
    let install = Install::new("", &[reading], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.commit("docs: edit the guide");

    let packet = install.packet(&["--base", &base]);

    assert_eq!(
        section(&packet, "### Uncovered surface (3 of 3 changed lines)")[1],
        "- `docs/guide.md`: lines 11-13"
    );
}

/// A fail that `audit --sanctioned` booked keeps its `(sanctioned: <ref>)`
/// text; it reaches the packet through `--audit` only (spec 5.5).
#[test]
fn a_sanctioned_fail_keeps_its_text_when_the_audit_comes_from_a_file() {
    let failing = entry("team.fails", grep("docs/*.md", "added", "fail", false));
    let install = Install::new("", &[failing], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let audit = install.audit_file(&base, &["team.fails=HR-1"]);

    let (packet, document) =
        install.packet_and_document(&base, &["--audit", audit.to_str().expect("UTF-8")]);

    let evidence = document["failed"][0]["evidence"]
        .as_str()
        .expect("evidence");
    assert!(evidence.ends_with("(sanctioned: HR-1)"), "{evidence}");
    assert_eq!(
        section(&packet, "### Failed (1)"),
        ["", &format!("- `team.fails`: `{evidence}`")]
    );
}

/// An evidence text over 300 characters is cut to 300 characters and `...`
/// (spec 5.5); the document keeps the whole text.
#[test]
fn an_evidence_text_over_300_characters_is_cut_with_dots() {
    let pattern = format!("added.*{}", "x".repeat(400));
    let failing = entry("team.fails", grep("docs/*.md", &pattern, "fail", false));
    let install = Install::new("", &[failing], None);
    let base = install.head();
    install.write("docs/long.md", &format!("added {}\n", "x".repeat(400)));
    install.commit("docs: add a long line");

    let (packet, document) = install.packet_and_document(&base, &[]);

    let whole = document["failed"][0]["evidence"]
        .as_str()
        .expect("evidence");
    assert!(whole.chars().count() > 300, "{whole}");
    let cut: String = whole.chars().take(300).collect();
    assert_eq!(
        section(&packet, "### Failed (1)"),
        ["", &format!("- `team.fails`: `{cut}...`")]
    );
}

/// Rows with the result `skipped` are counted in the line that closes the
/// judged rules (spec 5.5), and the document lists them.
#[test]
fn a_skipped_row_counts_as_a_check_that_did_not_apply() {
    let report = entry(
        "team.report",
        json!({"type": "report-field", "level": "fail", "if": "**", "field": "self_audit"}),
    );
    let install = Install::new("", &[report], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let (packet, document) = install.packet_and_document(&base, &[]);

    assert!(
        packet.contains("\n1 deterministic checks did not apply to this change.\n"),
        "{packet}"
    );
    assert_eq!(document["skipped"][0]["id"], "team.report");
}

/// Without `--review`, a judged row has no verdict. With one, it shows the
/// verdict of the `rule_adherence` row of its id and that row's evidence; a
/// `skipped` row shows `open` (spec 5.5).
#[test]
fn a_judged_row_shows_the_verdict_of_the_review_and_a_skipped_review_row_shows_open() {
    let entries = [
        judged_entry("team.judged-a"),
        judged_entry("team.judged-b"),
        judged_entry("team.judged-c"),
    ];
    let install = Install::new("", &entries, None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let review = install.scratch_file("review.json");
    fs::write(
        &review,
        json!({
            "kind": "task-review",
            "rule_adherence": [
                {"id": "team.judged-a", "mode": "judged", "result": "pass",
                 "evidence": "Checked by reading the diff."},
                {"id": "team.judged-b", "mode": "judged", "result": "skipped",
                 "evidence": "No time."},
                {"id": "team.other", "mode": "judged", "result": "fail", "evidence": "x"},
            ],
        })
        .to_string(),
    )
    .expect("write the review");

    let without = install.packet(&["--base", &base]);
    let (with, document) =
        install.packet_and_document(&base, &["--review", review.to_str().expect("UTF-8")]);

    assert_eq!(
        section(&without, "### Judged only (3)"),
        [
            "",
            "- `team.judged-a`: no verdict recorded",
            "- `team.judged-b`: no verdict recorded",
            "- `team.judged-c`: no verdict recorded",
            "",
            "0 deterministic checks did not apply to this change."
        ]
    );
    assert_eq!(
        section(&with, "### Judged only (3)"),
        [
            "",
            "- `team.judged-a`: pass · `Checked by reading the diff.`",
            "- `team.judged-b`: open · `No time.`",
            "- `team.judged-c`: no verdict recorded",
            "",
            "0 deterministic checks did not apply to this change."
        ]
    );
    assert_eq!(
        document["judged"],
        json!([
            {"id": "team.judged-a", "verdict": "pass", "evidence": "Checked by reading the diff."},
            {"id": "team.judged-b", "verdict": "open", "evidence": "No time."},
            {"id": "team.judged-c", "verdict": null, "evidence": null},
        ])
    );
}

/// A review result of `fail` or `warn` is the verdict.
#[test]
fn a_failed_or_warned_review_row_is_the_verdict() {
    let entries = [judged_entry("team.judged-a"), judged_entry("team.judged-b")];
    let install = Install::new("", &entries, None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let review = install.scratch_file("review.json");
    fs::write(
        &review,
        json!({
            "kind": "branch-review",
            "rule_adherence": [
                {"id": "team.judged-a", "mode": "judged", "result": "fail", "evidence": "Broken."},
                {"id": "team.judged-b", "mode": "judged", "result": "warn", "evidence": "Close."},
            ],
        })
        .to_string(),
    )
    .expect("write the review");

    let packet = install.packet(&["--base", &base, "--review", review.to_str().expect("UTF-8")]);

    assert_eq!(
        section(&packet, "### Judged only (2)")[1..3],
        [
            "- `team.judged-a`: fail · `Broken.`",
            "- `team.judged-b`: warn · `Close.`"
        ]
    );
}

/// A path with a backtick prints in a code span whose fence is one backtick
/// longer than the longest run (spec 4.8).
#[test]
fn a_path_with_a_backtick_prints_in_a_longer_fence() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.write("docs/a`b.md", &numbered("tick", 3));
    install.commit("docs: add a file with a backtick");

    let packet = install.packet(&["--base", &base]);

    assert!(
        packet
            .lines()
            .any(|line| line == "- ``docs/a`b.md``: lines 1-3"),
        "{packet}"
    );
}

/// A finding message that holds a comment opener prints inside a code span, so
/// that it never opens a comment in the packet (spec 4.8).
#[test]
fn a_finding_message_with_a_comment_opener_prints_inside_a_code_span() {
    let check = entry("team.no-opener", grep("docs/*.md", "<!--", "fail", false));
    let install = Install::new("", &[check], None);
    let base = install.head();
    install.write("docs/opener.md", "see <!-- hidden\n");
    install.commit("docs: add a comment opener");

    let (packet, document) = install.packet_and_document(&base, &[]);

    let evidence = document["failed"][0]["evidence"]
        .as_str()
        .expect("evidence");
    assert!(evidence.contains("<!--"), "{evidence}");
    assert!(
        packet
            .lines()
            .any(|line| line == format!("- `team.no-opener`: `{evidence}`")),
        "{packet}"
    );
    assert_eq!(
        packet
            .lines()
            .filter(|line| line.starts_with("<!--"))
            .count(),
        1,
        "only the marker line opens a comment"
    );
}

/// The packet of a subdirectory install names the install path in its marker
/// line, and the document names it too.
#[test]
fn a_subdirectory_install_names_its_path_in_the_marker_line() {
    let install = Install::new("sub", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let (packet, document) = install.packet_and_document(&base, &[]);

    assert!(
        packet.starts_with("<!-- houserules:evidence sub -->\n## houserules evidence\n"),
        "{packet}"
    );
    assert_eq!(document["install"], "sub");
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// `--score` joins the route of the packet: the range line and the route
/// section name the route that the score gives (spec 5.2).
#[test]
fn a_score_above_the_senior_threshold_gives_the_route_of_the_range_line() {
    let review = json!({"route": {"ownerScore": 0.5, "seniorScore": 0.8}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let (packet, document) = install.packet_and_document(&base, &["--score", "0.9"]);

    assert!(
        packet.contains(&range_line(&document, "senior-review")),
        "{packet}"
    );
    assert_eq!(
        section(&packet, "### Route"),
        [
            "",
            "senior-review",
            "",
            "- score 0.9 >= seniorScore 0.8 -> senior-review"
        ]
    );
    assert_eq!(document["route"]["inputs"]["score"], json!(0.9));
}

/// The route of the packet is the route that `route` decides over the same
/// range: the document holds the same decision document.
#[test]
fn the_route_of_the_document_is_the_decision_of_route() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("misc.txt", "stray\n");
    install.commit("chore: change several areas");
    let decision = install.scratch_file("route.json");
    let output = houserules()
        .args(["route", "--base", &base, "--json"])
        .arg(&decision)
        .arg("--dir")
        .arg(install.root())
        .output()
        .expect("run route");
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));

    let (_, document) = install.packet_and_document(&base, &[]);

    assert_eq!(document["route"], read_file_json(&decision));
}

// ---- the kinds of change ----------------------------------------------------------------

/// How a case of the kinds table changes the repository.
type Change = fn(&Install);

/// One kind of change of spec 4.2: the file the base commit holds, the change,
/// the path that the packet lists, and its packet line.
struct Kind {
    /// What the case shows.
    name: &'static str,
    /// Run before the base commit.
    setup: Change,
    /// Run after the base commit; the case commits it.
    change: Change,
    /// The path that the packet lists.
    path: &'static str,
    /// The packet line after `<path>: `.
    line: &'static str,
    /// Whether the change is in the index and the case commits the index as it
    /// is: a change of the mode that `git add` would take back on a platform that
    /// tracks the file mode.
    index_only: bool,
}

/// The ten lines of a file whose line 2 and lines 5 to 7 differ.
fn edited_lines() -> String {
    let mut lines: Vec<String> = (1..=10).map(|number| format!("guide {number}")).collect();
    for number in [2, 5, 6, 7] {
        lines[number - 1] = format!("changed {number}");
    }
    format!("{}\n", lines.join("\n"))
}

/// The ten lines of a file with line 2 changed and lines 5 and 6 gone.
fn edited_and_shortened() -> String {
    let mut lines: Vec<String> = (1..=10).map(|number| format!("guide {number}")).collect();
    lines[1] = "changed 2".to_string();
    lines.drain(4..6);
    format!("{}\n", lines.join("\n"))
}

/// The ten lines of a file with lines 5 and 6 gone.
fn shortened() -> String {
    let mut lines: Vec<String> = (1..=10).map(|number| format!("guide {number}")).collect();
    lines.drain(4..6);
    format!("{}\n", lines.join("\n"))
}

/// Every kind of spec 4.2 that needs no symlink: added, edited, a
/// deletion-only hunk, edited with a deletion, deleted, a pure rename, a rename
/// with edits, an empty file added and deleted, binary, mode only, and a
/// submodule.
fn kinds() -> Vec<Kind> {
    vec![
        Kind {
            name: "an added file",
            setup: |_| {},
            change: |install| install.write("docs/new.md", &numbered("new", 3)),
            path: "docs/new.md",
            line: "lines 1-3",
            index_only: false,
        },
        Kind {
            name: "an edited file",
            setup: |_| {},
            change: |install| install.write("docs/guide.md", &edited_lines()),
            path: "docs/guide.md",
            line: "lines 2, 5-7",
            index_only: false,
        },
        Kind {
            name: "a deletion-only hunk",
            setup: |_| {},
            change: |install| install.write("docs/guide.md", &shortened()),
            path: "docs/guide.md",
            line: "deleted after line 4: 2 lines",
            index_only: false,
        },
        Kind {
            name: "an edit and a deletion",
            setup: |_| {},
            change: |install| install.write("docs/guide.md", &edited_and_shortened()),
            path: "docs/guide.md",
            line: "lines 2; deleted after line 4: 2 lines",
            index_only: false,
        },
        Kind {
            name: "a deleted file",
            setup: |install| install.write("docs/gone.md", &numbered("gone", 2)),
            change: |install| {
                fs::remove_file(file(&install.root(), "docs/gone.md")).expect("delete the file");
            },
            path: "docs/gone.md",
            line: "deleted, lines 1-2 of the old file",
            index_only: false,
        },
        Kind {
            name: "a pure rename",
            setup: |install| install.write("docs/old.md", &numbered("old", 10)),
            change: |install| {
                fs::rename(
                    file(&install.root(), "docs/old.md"),
                    file(&install.root(), "docs/new.md"),
                )
                .expect("rename the file");
            },
            path: "docs/new.md",
            line: "renamed from `docs/old.md`",
            index_only: false,
        },
        Kind {
            name: "a rename with edits",
            setup: |install| install.write("docs/old.md", &numbered("old", 10)),
            change: |install| {
                fs::remove_file(file(&install.root(), "docs/old.md")).expect("delete the file");
                let mut lines: Vec<String> =
                    (1..=10).map(|number| format!("old {number}")).collect();
                lines[2] = "changed 3".to_string();
                lines[3] = "changed 4".to_string();
                install.write("docs/new.md", &format!("{}\n", lines.join("\n")));
            },
            path: "docs/new.md",
            line: "lines 3-4; renamed from `docs/old.md`",
            index_only: false,
        },
        Kind {
            name: "an empty file added",
            setup: |_| {},
            change: |install| install.write("docs/blank.md", ""),
            path: "docs/blank.md",
            line: "empty file added",
            index_only: false,
        },
        Kind {
            name: "an empty file deleted",
            setup: |install| install.write("docs/blank.md", ""),
            change: |install| {
                fs::remove_file(file(&install.root(), "docs/blank.md")).expect("delete the file");
            },
            path: "docs/blank.md",
            line: "empty file deleted",
            index_only: false,
        },
        Kind {
            name: "a binary file",
            setup: |install| install.write_bytes("docs/data.bin", &[0, 1, 2, 3]),
            change: |install| install.write_bytes("docs/data.bin", &[0, 9, 9, 9]),
            path: "docs/data.bin",
            line: "binary",
            index_only: false,
        },
        Kind {
            name: "a change of the file mode only",
            setup: |install| install.write("docs/run.sh", "echo run\n"),
            change: |install| {
                git(
                    install.top.path(),
                    &["update-index", "--chmod=+x", "docs/run.sh"],
                );
            },
            path: "docs/run.sh",
            line: "mode only",
            index_only: true,
        },
        Kind {
            name: "a submodule",
            setup: |_| {},
            change: |install| add_submodule(install.top.path(), "vendor/lib", "f.txt"),
            path: "vendor/lib",
            line: "submodule",
            index_only: false,
        },
    ]
}

/// Each kind of change prints its packet line (spec 4.2; case 1).
#[test]
fn each_kind_of_change_prints_its_packet_line() {
    let mut wrong = Vec::new();
    for kind in kinds() {
        let install = Install::new("", &[], None);
        (kind.setup)(&install);
        install.commit("chore: set up the case");
        let base = install.head();
        (kind.change)(&install);
        if kind.index_only {
            git(
                install.top.path(),
                &[
                    "commit",
                    "-q",
                    "--no-verify",
                    "-m",
                    "chore: make the change",
                ],
            );
        } else {
            install.commit("chore: make the change");
        }

        let packet = install.packet(&["--base", &base]);

        let expected = format!("- `{}`: {}", kind.path, kind.line);
        if !packet.lines().any(|line| line == expected) {
            wrong.push(format!("{}: no line {expected:?} in\n{packet}", kind.name));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A symlink prints its lines: the target is the one line of the link. Unix
/// only: Windows creates a symlink only with a privilege.
#[cfg(unix)]
#[test]
fn a_symlink_prints_its_line() {
    let install = Install::new("", &[], None);
    std::os::unix::fs::symlink("guide", file(&install.root(), "docs/link")).expect("create");
    install.commit("chore: add a link");
    let base = install.head();
    fs::remove_file(file(&install.root(), "docs/link")).expect("delete the link");
    std::os::unix::fs::symlink("other", file(&install.root(), "docs/link")).expect("create");
    install.commit("chore: retarget the link");

    let packet = install.packet(&["--base", &base]);

    assert!(
        packet
            .lines()
            .any(|line| line == "- `docs/link`: symlink, lines 1-1"),
        "{packet}"
    );
}

/// The JSON tells an added empty file from a deleted one: the packet line needs
/// it, and the kind alone does not. The two changes are in two ranges, because
/// git reads an empty file that goes away and one that comes in the same range
/// as a rename.
#[test]
fn the_document_tells_an_added_empty_file_from_a_deleted_one() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.write("docs/blank.md", "");
    let added = install.commit("chore: add an empty file");
    fs::remove_file(file(&install.root(), "docs/blank.md")).expect("delete the file");
    let deleted = install.commit("chore: delete the empty file");

    let (_, added_document) = install.packet_and_document(&base, &["--head", &added]);
    let (_, deleted_document) = install.packet_and_document(&added, &["--head", &deleted]);

    assert_eq!(
        added_document["uncovered"]["paths"],
        json!([{"path": "docs/blank.md", "old_path": null, "kind": "empty", "ranges": [],
                "empty": "added"}])
    );
    assert_eq!(
        deleted_document["uncovered"]["paths"],
        json!([{"path": "docs/blank.md", "old_path": null, "kind": "empty", "ranges": [],
                "empty": "deleted"}])
    );
    assert_valid(&added_document, EVIDENCE_SCHEMA);
    assert_valid(&deleted_document, EVIDENCE_SCHEMA);
}

// ---- the intent -------------------------------------------------------------------------

/// The Intent section follows the uncovered surface and comes before the rules
/// (issue 65, spec 5.3).
#[test]
fn the_intent_section_follows_the_uncovered_surface() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let packet = install.packet(&["--base", &base]);

    let positions: Vec<usize> = [
        "### Uncovered surface (",
        "### Intent (",
        "### Failed (",
        "### Warned (",
        "### Proven (",
        "### Judged only (",
        "### Size and links",
        "### Route",
    ]
    .iter()
    .map(|heading| {
        packet
            .find(heading)
            .unwrap_or_else(|| panic!("no {heading}"))
    })
    .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{positions:?}"
    );
}

/// The `work-item` adapter gives one intent for each trailer with the title
/// that the tracker holds (spec 4.9).
#[test]
fn a_work_item_trailer_prints_an_intent_with_the_title_of_the_tracker() {
    let install = Install::new("", &[], None);
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide\n\nWork-Item: HR-901");

    let (packet, document) = install.packet_and_document(&base, &[]);

    assert_eq!(
        section(&packet, "### Intent (1)"),
        ["", "`work-item HR-901`: `Write the mini-tools smoke test`"]
    );
    assert_eq!(
        document["intent"],
        json!([{
            "source": "work-item", "ref": "HR-901",
            "title": "Write the mini-tools smoke test", "why": null,
            "tasks": null, "requirements": null, "validation": null, "artifacts": [],
        }])
    );
    assert_eq!(
        section(&packet, "### Size and links")[2],
        "Work items: `HR-901`"
    );
}

/// The `file` adapter reads a changed path that matches `review.intent.files`:
/// its heading is the title, the first paragraph is the why (a fenced block),
/// and the task-list lines give the counts (spec 4.9; case 4).
#[test]
fn a_changed_plan_file_prints_its_heading_its_why_and_its_task_counts() {
    let review = json!({"intent": {"sources": ["file"], "files": ["docs/plans/*.md"]}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    install.write(
        "docs/plans/p.md",
        "# The plan\n\nBecause <!-- hidden --> it\nmatters.\n\n- [x] one\n* [X] two\n+ [ ] three\n  - [ ] four\n",
    );
    install.commit("docs: add the plan");

    let (packet, document) = install.packet_and_document(&base, &[]);

    assert_eq!(
        section(&packet, "### Intent (1)"),
        [
            "",
            "`file docs/plans/p.md`: `The plan`",
            "Why:",
            "```",
            "Because <!-- hidden --> it",
            "matters.",
            "```",
            "Tasks: 2 of 4 done"
        ]
    );
    assert_eq!(
        document["intent"][0]["artifacts"],
        json!(["docs/plans/p.md"])
    );
}

/// `--intent` names an intent whatever the configuration says (spec 5.1).
#[test]
fn a_named_intent_joins_the_packet_whatever_the_configuration_says() {
    let install = Install::new("", &[], None);
    install.write("docs/plans/named.md", "# Named\n\nWhy it is named.\n");
    install.commit("docs: add the plan");
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let packet = install.packet(&["--base", &base, "--intent", "file:docs/plans/named.md"]);

    assert_eq!(
        section(&packet, "### Intent (1)"),
        [
            "",
            "`file docs/plans/named.md`: `Named`",
            "Why:",
            "```",
            "Why it is named.",
            "```",
            "Tasks: 0 of 0 done"
        ]
    );
}

/// A named intent of an adapter that cannot resolve it prints its block as
/// `<source> <ref>: <the adapter's line>` with no other line, and the command
/// exits 0 (spec 5.3). The line is the binary's own text, so it prints with no
/// code span. The document tells the intent from a resolved one by its
/// structure: every field of the contract is `null` (`artifacts` is empty) and
/// the key `unresolved` holds the line (spec 5.9).
#[test]
fn a_named_intent_that_does_not_resolve_prints_the_line_of_its_adapter() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let (packet, document) =
        install.packet_and_document(&base, &["--intent", "file:docs/plans/missing.md"]);

    assert_eq!(
        section(&packet, "### Intent (1)"),
        [
            "",
            "`file docs/plans/missing.md`: no regular file at that path at the head"
        ]
    );
    assert_eq!(
        document["intent"],
        json!([{
            "source": "file", "ref": "docs/plans/missing.md",
            "title": null, "why": null,
            "tasks": null, "requirements": null, "validation": null, "artifacts": [],
            "unresolved": "no regular file at that path at the head",
        }])
    );
    assert_eq!(
        keys(&document["intent"][0]),
        [
            "source",
            "ref",
            "title",
            "why",
            "tasks",
            "requirements",
            "validation",
            "artifacts",
            "unresolved"
        ]
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// A named work item that the tracker does not hold prints the line of the
/// `work-item` adapter.
#[test]
fn a_named_work_item_that_the_tracker_does_not_hold_prints_its_line() {
    let install = Install::new("", &[], None);
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let (packet, document) = install.packet_and_document(&base, &["--intent", "work-item:HR-999"]);

    assert_eq!(
        section(&packet, "### Intent (1)"),
        [
            "",
            "`work-item HR-999`: the tracker holds no such work item"
        ]
    );
    assert_eq!(
        document["intent"][0]["unresolved"],
        "the tracker holds no such work item"
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// A named intent that does not resolve takes its place among the others by
/// the order of spec 5.8, adapter order and then `ref`: here the plan that the
/// configuration finds comes after the named path that sorts before it.
#[test]
fn an_unresolved_named_intent_sits_among_the_others_in_adapter_order_then_ref() {
    let review =
        json!({"intent": {"sources": ["work-item", "file"], "files": ["docs/plans/*.md"]}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    install.write("docs/plans/b.md", "# Plan B\n\nWhy B.\n");
    install.commit("docs: add a plan");

    let (packet, document) =
        install.packet_and_document(&base, &["--intent", "file:docs/other/0.md"]);

    let refs: Vec<&str> = document["intent"]
        .as_array()
        .expect("the intents")
        .iter()
        .map(|intent| intent["ref"].as_str().expect("a ref"))
        .collect();
    assert_eq!(refs, ["docs/other/0.md", "docs/plans/b.md"]);
    assert_eq!(
        section(&packet, "### Intent (2)"),
        [
            "",
            "`file docs/other/0.md`: no regular file at that path at the head",
            "",
            "`file docs/plans/b.md`: `Plan B`",
            "Why:",
            "```",
            "Why B.",
            "```",
            "Tasks: 0 of 0 done"
        ]
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// A change with no changed path prints `None.` for the uncovered surface when
/// the head is an ancestor of the base: the merge base is the head (review
/// focus 5).
#[test]
fn a_head_behind_its_base_prints_none() {
    let install = Install::new("", &[], None);
    let first = install.head();
    install.append("docs/guide.md", 1);
    let second = install.commit("docs: edit the guide");

    let packet = install.packet(&["--base", &second, "--head", &first]);

    assert_eq!(
        section(&packet, "### Uncovered surface (0 of 0 changed lines)"),
        ["", "None."]
    );
    assert_eq!(
        section(&packet, "### Route"),
        ["", "owner-review", "", "- default -> owner-review"]
    );
}

// ---- the document -----------------------------------------------------------------------

/// The keys of the JSON object `value`, in the order of the text.
fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// The document holds the keys of spec 5.9 in order, validates against its
/// generated schema, and carries both forms of a line range: the lines of an
/// edit and the place of a deletion-only hunk.
#[test]
fn the_document_validates_with_both_range_forms() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.write("docs/guide.md", &edited_and_shortened());
    install.write("misc.txt", "stray\n");
    install.commit("docs: edit the guide");

    let (_, document) = install.packet_and_document(&base, &[]);

    assert_eq!(
        keys(&document),
        [
            "schema",
            "install",
            "base",
            "head",
            "merge_base",
            "intent",
            "rules",
            "diff",
            "uncovered",
            "failed",
            "warned",
            "proven",
            "judged",
            "skipped",
            "work_items",
            "route"
        ]
    );
    assert_eq!(document["schema"], "houserules/evidence@1.0.0");
    assert_eq!(
        keys(&document["rules"]),
        ["merge_base_hash", "head_hash", "packs"]
    );
    assert_eq!(
        keys(&document["diff"]),
        ["files", "added", "deleted", "lines", "excluded"]
    );
    assert_eq!(
        keys(&document["uncovered"]),
        ["lines", "total_lines", "share", "paths"]
    );
    assert_eq!(document["uncovered"]["paths"][0]["path"], "misc.txt");
    assert_eq!(
        keys(&document["uncovered"]["paths"][0]),
        ["path", "old_path", "kind", "ranges"]
    );
    assert_eq!(
        document["route"]["schema"], "houserules/route@1.0.0",
        "the route is the decision document of route"
    );
    assert!(
        document["uncovered"]["paths"]
            .as_array()
            .expect("paths")
            .iter()
            .all(|path| path["path"] != "docs/guide.md"),
        "the covering check read the guide"
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
    assert_valid(&document["route"], ROUTE_SCHEMA);
}

/// An uncovered edit and an uncovered deletion-only hunk give the two forms of
/// a line range, and the document validates.
#[test]
fn both_forms_of_a_line_range_validate() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.write("docs/guide.md", &edited_and_shortened());
    install.commit("docs: edit the guide");

    let (_, document) = install.packet_and_document(&base, &[]);

    assert_eq!(
        document["uncovered"]["paths"][0]["ranges"],
        json!([{"start": 2, "end": 2}, {"after": 4, "deleted": 2}])
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// The packs of the install are in the document with their name, version, and
/// digest (spec 5.9).
#[test]
fn the_document_lists_the_packs_of_the_install() {
    let install = Install::new("", &[], None);
    edit_marker(&install.root(), |marker| {
        marker.insert(
            "packs".to_string(),
            json!({
                "zeta": {"version": "1.2.0", "source": "kit", "sha256": "ab".repeat(32), "addedAreas": []},
                "alpha": {"version": "3.0.1", "source": "kit", "sha256": "cd".repeat(32), "addedAreas": []},
            }),
        );
    });
    let base = install.commit("chore: pin the packs");
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let (_, document) = install.packet_and_document(&base, &[]);

    assert_eq!(
        document["rules"]["packs"],
        json!([
            {"name": "alpha", "version": "3.0.1", "sha256": "cd".repeat(32)},
            {"name": "zeta", "version": "1.2.0", "sha256": "ab".repeat(32)},
        ])
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// The packs of the document are those of the marker at the head, read from git
/// like the head's rules hash, and not those of the checked-out work tree
/// (spec 5.9): the head pins `knowledge-base` at 9.9.9 while the checkout pins
/// 3.0.0.
#[test]
fn the_packs_are_those_of_the_marker_at_the_head_and_not_of_the_work_tree() {
    let install = Install::new("", &[], None);
    let pin = |version: &str| {
        edit_marker(&install.root(), |marker| {
            marker.insert(
                "packs".to_string(),
                json!({"knowledge-base": {
                    "version": version, "source": "kit",
                    "sha256": "ab".repeat(32), "addedAreas": [],
                }}),
            );
        });
    };
    pin("3.0.0");
    let checked_out = install.commit("chore: pin the pack");
    pin("9.9.9");
    let head = install.commit("chore: move the pack pin");
    git(install.top.path(), &["checkout", "-q", &checked_out]);

    let (_, document) = install.packet_and_document(&checked_out, &["--head", &head]);

    assert_eq!(document["head"], head);
    assert_eq!(
        document["rules"]["packs"],
        json!([{"name": "knowledge-base", "version": "9.9.9", "sha256": "ab".repeat(32)}])
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// A head with no marker pins no pack, whatever the checked-out marker pins.
#[test]
fn a_head_with_no_marker_pins_no_pack() {
    let install = Install::new("", &[], None);
    edit_marker(&install.root(), |marker| {
        marker.insert(
            "packs".to_string(),
            json!({"knowledge-base": {
                "version": "3.0.0", "source": "kit",
                "sha256": "ab".repeat(32), "addedAreas": [],
            }}),
        );
    });
    let checked_out = install.commit("chore: pin the pack");
    git(install.top.path(), &["rm", "-q", ".houserules.json"]);
    let head = install.commit("chore: remove the marker");
    git(install.top.path(), &["checkout", "-q", &checked_out]);

    let (_, document) = install.packet_and_document(&checked_out, &["--head", &head]);

    assert_eq!(document["rules"]["packs"], json!([]));
}

/// A `packs` key of the head's marker that is no object stops the command with
/// the line of the marker reader, which names the marker as git holds it: the
/// head and the path from the git top level.
#[test]
fn a_packs_key_that_is_no_object_at_the_head_exits_2_with_the_head_named() {
    let install = Install::new("", &[], None);
    let checked_out = install.head();
    edit_marker(&install.root(), |marker| {
        marker.insert("packs".to_string(), json!([]));
    });
    let head = install.commit("chore: break the packs key");
    git(install.top.path(), &["checkout", "-q", &checked_out]);

    let output = install.evidence(&["--base", &checked_out, "--head", &head]);

    assert_output(
        &output,
        2,
        "",
        &format!(
            "{}:.houserules.json: packs must be an object\n",
            short(&head)
        ),
    );
}

/// The two intents of the OpenSpec and Spec Kit fixtures, placed in an
/// evidence document, validate against the committed schema: the contract fits
/// both workflows (spec 4.9; case 4).
#[test]
fn the_intents_of_the_openspec_and_spec_kit_fixtures_validate_in_a_document() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let (_, mut document) = install.packet_and_document(&base, &[]);

    let fixtures = repo_root().join("crates/houserules/tests/fixtures/intent");
    document["intent"] = json!([
        read_file_json(&fixtures.join("openspec-change").join("intent.json")),
        read_file_json(&fixtures.join("spec-kit-feature").join("intent.json")),
    ]);

    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// A range from git's empty tree names no merge base: the document prints
/// `null` and the range line prints `none`.
#[test]
fn a_range_from_the_empty_tree_has_no_merge_base() {
    let install = Install::new("", &[], None);

    let (packet, document) = install.packet_and_document(EMPTY_TREE, &[]);

    assert_eq!(document["merge_base"], Value::Null);
    assert_eq!(document["base"], EMPTY_TREE);
    assert!(
        packet.lines().any(|line| line.starts_with(&format!(
            "Range {}..{} (merge base none) · rules ",
            &EMPTY_TREE[..7],
            short(document["head"].as_str().expect("a head"))
        ))),
        "{packet}"
    );
    assert_eq!(document["rules"]["merge_base_hash"], Value::Null);
    assert_valid(&document, EVIDENCE_SCHEMA);
}

// ---- the cap ----------------------------------------------------------------------------

/// A change of 3,000 paths in no area makes a packet over the cap. The uncovered
/// list is cut to the items that fit and ends with the count of the rest; the
/// count in the heading stays (spec 5.8).
#[test]
fn a_change_of_3000_paths_in_no_area_cuts_the_uncovered_list_to_fit() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for number in 0..3000 {
        install.write(&format!("misc/f{number:04}.txt"), "x\n");
    }
    install.commit("chore: add 3000 files");

    let (packet, document) = install.packet_and_document(&base, &[]);

    assert!(
        packet.chars().count() <= PACKET_LIMIT,
        "{} characters",
        packet.chars().count()
    );
    let uncovered = section(
        &packet,
        "### Uncovered surface (3000 of 3000 changed lines)",
    );
    let items: Vec<&&str> = uncovered
        .iter()
        .filter(|line| line.starts_with("- `misc/"))
        .collect();
    let rest = uncovered
        .iter()
        .find_map(|line| {
            line.strip_prefix("- ... and ")
                .and_then(|rest| rest.strip_suffix(" more"))
        })
        .expect("the count of the rest")
        .parse::<usize>()
        .expect("a count");
    assert_eq!(items.len() + rest, 3000);
    assert!(rest > 0 && !items.is_empty());
    assert_eq!(*items[0], "- `misc/f0000.txt`: lines 1");
    assert_eq!(
        document["uncovered"]["paths"].as_array().map(Vec::len),
        Some(3000),
        "the document holds the whole list"
    );
    assert_valid(&document, EVIDENCE_SCHEMA);
}

/// A range whose trailers name 3,000 work items cuts the Work items list to the
/// first 50 and counts the rest (spec 5.8; case 1). The ids are long enough
/// that the list alone is over the cap.
#[test]
fn trailers_that_name_3000_work_items_cut_the_list_to_50() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let ids: Vec<String> = (0..3000)
        .map(|number| format!("WORK-{number:030}"))
        .collect();
    let trailers: Vec<String> = ids.iter().map(|id| format!("Work-Item: {id}")).collect();
    let message = install.scratch_file("message.txt");
    fs::write(
        &message,
        format!("docs: edit the guide\n\n{}\n", trailers.join("\n")),
    )
    .expect("write the message");
    git(install.top.path(), &["add", "-A"]);
    git(
        install.top.path(),
        &[
            "commit",
            "-q",
            "--no-verify",
            "-F",
            message.to_str().expect("UTF-8"),
        ],
    );

    let (packet, document) = install.packet_and_document(&base, &[]);

    let listed: Vec<String> = ids[..50].iter().map(|id| format!("`{id}`")).collect();
    assert_eq!(
        section(&packet, "### Size and links")[2],
        format!("Work items: {}, ... and 2950 more", listed.join(", "))
    );
    assert!(packet.chars().count() <= PACKET_LIMIT);
    assert_eq!(
        document["work_items"].as_array().map(Vec::len),
        Some(3000),
        "the document holds every id"
    );
}

// ---- the exits --------------------------------------------------------------------------

/// `--out` writes the packet to the file and prints nothing on stdout.
#[test]
fn out_writes_the_packet_to_a_file() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let printed = install.packet(&["--base", &base]);
    let target = install.scratch_file("packet.md");

    let output = install.evidence(&["--base", &base, "--out", target.to_str().expect("UTF-8")]);

    assert_output(&output, 0, "", "");
    assert_eq!(
        fs::read_to_string(&target).expect("read the packet"),
        printed
    );
}

/// An `--audit` file of another range exits 2 with the line of spec 4.
#[test]
fn an_audit_file_of_another_range_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let middle = install.commit("docs: edit the guide");
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide again");
    let audit = install.audit_file(&base, &[]);

    let output = install.evidence(&["--base", &middle, "--audit", audit.to_str().expect("UTF-8")]);

    let head = install.head();
    assert_output(
        &output,
        2,
        "",
        &format!(
            "{} is the audit of {}..{}, not of {}..{}\n",
            audit.display(),
            short(&base),
            short(&head),
            short(&middle),
            short(&head)
        ),
    );
}

/// An `--audit` file that is no audit result is one named line: the file, the
/// words `not an audit result`, and the reason that the parser gives. A text
/// that is no JSON gets the same reason from a parse into any type, so the test
/// takes it from a parse into a JSON value.
#[test]
fn an_audit_file_that_is_no_audit_result_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let audit = install.scratch_file("not-an-audit.json");
    let text = "not json";
    fs::write(&audit, text).expect("write the file");
    let reason = serde_json::from_str::<Value>(text).expect_err("the text is no JSON");

    let output = install.evidence(&["--base", &base, "--audit", audit.to_str().expect("UTF-8")]);

    assert_output(
        &output,
        2,
        "",
        &format!("{}: not an audit result: {reason}\n", audit.display()),
    );
}

/// A `--review` file that is no review deliverable is one named line: a file
/// that does not exist, text that is no JSON, a deliverable of another kind,
/// and a `rule_adherence` that is no list of rows.
#[test]
fn a_review_file_that_is_no_review_deliverable_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let review = install.scratch_file("review.json");
    let path = review.display().to_string();

    let missing = install.evidence(&["--base", &base, "--review", review.to_str().expect("UTF-8")]);
    let unreadable = fs::read_to_string(&review).expect_err("the file does not exist");
    assert_output(&missing, 2, "", &format!("{path}: {unreadable}\n"));

    for (text, reason) in [
        ("not json", "expected ident at line 1 column 2"),
        (
            "{\"kind\": \"task-report\"}",
            "kind must be \"task-review\", \"re-review\", or \"branch-review\"",
        ),
        (
            "{\"kind\": \"task-review\", \"rule_adherence\": {}}",
            "rule_adherence must be an array",
        ),
        (
            "{\"kind\": \"task-review\", \"rule_adherence\": [{\"result\": \"pass\"}]}",
            "rule_adherence[0] must be an object with an id",
        ),
    ] {
        fs::write(&review, text).expect("write the review");

        let output =
            install.evidence(&["--base", &base, "--review", review.to_str().expect("UTF-8")]);

        assert_output(
            &output,
            2,
            "",
            &format!("{path}: not a review deliverable: {reason}\n"),
        );
    }
}

/// An `--intent` whose source is no adapter exits 2 with the line of spec 5.1.
#[test]
fn an_intent_whose_source_is_no_adapter_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();

    let output = install.evidence(&["--base", &base, "--intent", "openspec:add-x"]);

    assert_output(
        &output,
        2,
        "",
        "evidence: --intent openspec:add-x: \"openspec\" is no intent source; use one of: work-item, file\n",
    );
}

/// An `--intent` that is no `<source>:<ref>` exits 2 with the line of spec 4.9.
#[test]
fn an_intent_that_is_no_source_and_ref_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();

    let output = install.evidence(&["--base", &base, "--intent", "docs/plans/p.md"]);

    assert_output(
        &output,
        2,
        "",
        "evidence: --intent docs/plans/p.md: an intent is written <source>:<ref>\n",
    );
}

/// A `--score` outside 0 to 1 exits 2 with the line of spec 5.1, whatever the
/// thresholds are. The line prints the score as a number.
#[test]
fn a_score_outside_zero_to_one_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for (score, printed) in [
        ("1.5", "1.5"),
        ("-0.1", "-0.1"),
        ("NaN", "NaN"),
        ("1e3", "1000"),
    ] {
        let output = install.evidence(&["--base", &base, "--score", score]);

        assert_output(
            &output,
            2,
            "",
            &format!("evidence: --score {printed} is outside 0 to 1\n"),
        );
    }
}

/// A `--score` with neither threshold configured is a setup error: the route
/// that the packet names needs a threshold (spec 6.2, rule 7), and the line is
/// the one of `route`.
#[test]
fn a_score_with_no_threshold_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let output = install.evidence(&["--base", &base, "--score", "0.5"]);

    assert_output(
        &output,
        2,
        "",
        "route: --score needs review.route.ownerScore or seniorScore\n",
    );
}

/// A `packs` key of the marker that is no object stops the command with the
/// line of the marker reader, which names the marker as git holds it at the
/// head: the head and the path from the git top level.
#[test]
fn a_packs_key_that_is_no_object_exits_2() {
    let install = Install::new("", &[], None);
    edit_marker(&install.root(), |marker| {
        marker.insert("packs".to_string(), json!([]));
    });
    let base = install.commit("chore: break the packs key");

    let output = install.evidence(&["--base", &base]);

    assert_output(
        &output,
        2,
        "",
        &format!(
            "{}:.houserules.json: packs must be an object\n",
            short(&base)
        ),
    );
}

/// An unwritable `--out` is one named line, and nothing prints.
#[test]
fn an_unwritable_out_file_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let target = file(install.scratch.path(), "missing-folder/packet.md");

    let output = install.evidence(&["--base", &base, "--out", target.to_str().expect("UTF-8")]);

    let unwritable = fs::write(&target, "").expect_err("the folder does not exist");
    assert_output(
        &output,
        2,
        "",
        &format!("{}: {unwritable}\n", target.display()),
    );
}

/// An unwritable `--json` is one named line, and nothing prints.
#[test]
fn an_unwritable_json_file_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let target = file(install.scratch.path(), "missing-folder/evidence.json");

    let output = install.evidence(&["--base", &base, "--json", target.to_str().expect("UTF-8")]);

    let unwritable = fs::write(&target, "").expect_err("the folder does not exist");
    assert_output(
        &output,
        2,
        "",
        &format!("{}: {unwritable}\n", target.display()),
    );
}

/// A `review` key with a fault stops the command with the line that
/// `check-knowledge` prints for it.
#[test]
fn a_review_key_with_a_fault_exits_2_with_its_line() {
    let install = Install::new("", &[], Some(json!({"route": {"default": "never-review"}})));
    let base = install.head();

    let output = install.evidence(&["--base", &base]);

    assert_output(
        &output,
        2,
        "",
        ".houserules.json: review.route.default must be \"owner-review\" or \"senior-review\"\n",
    );
}

/// A base that is no commit is one named line.
#[test]
fn a_base_that_is_no_commit_exits_2() {
    let install = Install::new("", &[], None);

    let output = install.evidence(&["--base", "no-such-ref"]);

    assert_output(&output, 2, "", "bad ref \"no-such-ref\"\n");
}

/// `--base` is required.
#[test]
fn a_missing_base_is_a_usage_error() {
    let install = Install::new("", &[], None);

    let output = install.evidence(&[]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("--base"), "{}", stderr(&output));
}

/// A git older than 2.41 stops the command with the line of spec 4.2; the shim
/// answers a call with `--version` and passes every other call to the real git.
/// Unix only: the shim is a POSIX shell script with an executable bit.
#[cfg(unix)]
#[test]
fn a_git_older_than_2_41_exits_2_with_its_line() {
    use std::os::unix::fs::PermissionsExt;

    let install = Install::new("", &[], None);
    let base = install.head();
    let shim_dir = install.scratch.path().join("shim");
    fs::create_dir_all(&shim_dir).expect("create the shim directory");
    let script = shim_dir.join("git");
    fs::write(
        &script,
        "#!/bin/sh\nfor argument in \"$@\"; do\n  if [ \"$argument\" = \"--version\" ]; then echo 'git version 2.39.0'; exit 0; fi\ndone\nPATH=\"$HOUSERULES_TEST_REAL_PATH\" exec git \"$@\"\n",
    )
    .expect("write the shim");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("make it executable");
    let real_path = std::env::var_os("PATH").expect("a PATH");
    let mut paths = vec![shim_dir];
    paths.extend(std::env::split_paths(&real_path));

    let output = houserules()
        .args(["evidence", "--base", &base, "--dir"])
        .arg(install.root())
        .env("PATH", std::env::join_paths(paths).expect("join the PATH"))
        .env("HOUSERULES_TEST_REAL_PATH", real_path)
        .output()
        .expect("run evidence");

    assert_output(
        &output,
        2,
        "",
        "evidence: git 2.39.0 is older than 2.41, which --attr-source needs\n",
    );
}

// ---- the help ---------------------------------------------------------------------------

/// The sentences of the `about` of `evidence`.
const ABOUT: [&str; 12] = [
    "Writes the reviewer packet of a change.",
    "The packet starts with the uncovered surface: the changed files and line ranges that no check with `covers: true` read.",
    "Then it lists the intent of the change, the failed, warned, proven, and judged rules, the size of the diff with the work items, and the route.",
    "It prints the packet as Markdown on stdout, for a comment on a pull or merge request, or writes it to `--out`.",
    "`--json` also writes the document `houserules/evidence@1.0.0`.",
    "The audit rows come from `--audit`, else from an audit that the command runs in process.",
    "`--review` names a review deliverable: a judged rule then shows the verdict of that review.",
    "`--intent <source>:<ref>` adds an intent to the packet, as the `Intent:` line of an evidence bundle writes it.",
    "A packet over 60,000 characters is cut, and each cut list ends with the count of the items that it leaves out.",
    "The packet is the same for the same range and the same rules.",
    "It exits 0 whatever the audit finds.",
    "It exits 2 for a setup error, for example: a ref that is no commit, an `--audit` file of another range or that is no audit result, a `--review` file that is no review deliverable, an `--intent` whose source is no adapter, a `--score` outside 0 to 1, a git older than 2.41, or an `--out` or `--json` file that cannot be written",
];

/// The help text of each argument of `evidence`.
const ARGUMENTS: [&str; 9] = [
    "The base ref of the range. The range runs from the merge base of `--base` and `--head` to `--head`",
    "The head ref of the range; defaults to `HEAD`",
    "The file that `audit --json` wrote for the same range. Its `base` and `head` must name the same commits as `--base` and `--head`. Without this flag, the command runs the audit in process",
    "A review deliverable of the kind `task-review`, `re-review`, or `branch-review`, as JSON. A judged rule shows the result of its `rule_adherence` row; a rule with no row shows that no verdict is recorded",
    "The intent to add, written `<source>:<ref>` as the `Intent:` line of an evidence bundle writes it. The source is `work-item` or `file`",
    "An external score from 0 to 1 for the route. It needs `review.route.ownerScore` or `review.route.seniorScore`",
    "Writes the packet to this file in place of stdout",
    "Also writes the evidence document `houserules/evidence@1.0.0` to this file",
    "Install root to read from; defaults to the nearest directory at or above the current one that holds `.houserules.json`, no higher than the git top level, else the git top level",
];

/// `evidence --help` states what the packet holds, where it goes, the cap, the
/// exits, and each condition of an argument (`quality.user-facing-text-is-contract`),
/// and `houserules --help` lists the same about text.
#[test]
fn the_help_of_evidence_states_the_packet_the_cap_and_the_exits() {
    states("evidence --help", &help_of("evidence"), &ABOUT);
    states("evidence --help", &help_of("evidence"), &ARGUMENTS);
    states("houserules --help", &help_of_args(&["--help"]), &ABOUT);
}
