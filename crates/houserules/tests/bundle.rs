//! `houserules bundle write` and `houserules bundle check` (batch 37, spec 7.1
//! to 7.3; acceptance case 3; review focus 1; HR-179). Each case runs the
//! compiled binary against a scratch repository whose install holds the JSON
//! tracker, a plan file with a why and one without, and captures that
//! `houserules capture` wrote. A line of the spec is the literal of a test.
//!
//! Every fixture is hermetic: the install is written by hand, and `bundle
//! check` runs no audit.

mod help_support;
mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use help_support::{help_of, help_of_args, states};
use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    assert_output, commit_all, copy_dir, git, houserules, repo_root, scratch_git_repo, stderr,
    stdout,
};

/// The heading of the section (spec 7.1).
const HEADING: &str = "## Evidence bundle";

/// The line for a description with no section (spec 7.3).
const NO_SECTION: &str = "bundle check: no \"## Evidence bundle\" section";

/// The line for a work item in an install with no tracker (spec 7.3), for two
/// work items.
const TWO_NOT_RESOLVED: &str =
    "bundle check: 2 work items not resolved: no tracker (run houserules init --tracker <name>)";

/// The line for work items of an install whose tracker cannot be reached (spec 7.3), for two work
/// items: the JSON tracker of the fixture with no `backlog/` folder.
const TWO_UNREACHABLE: &str = "bundle check: 2 work items not resolved: tracker json cannot be reached: backlog/ is missing; restore backlog/ from version control";

/// The line without budgets (spec 7.3).
const NO_BUDGETS: &str = "bundle check: no diff budgets configured (review.bundle.budgets)";

/// The adapter's line for a plan file that is not in the tree at the head (spec
/// 4.9).
const NO_REGULAR_FILE: &str = "no regular file at that path at the head";

/// The adapter's line for a work item that the tracker does not hold (spec 4.9).
const NO_SUCH_WORK_ITEM: &str = "the tracker holds no such work item";

// ---- fixtures -------------------------------------------------------------------------

/// The areas of every fixture.
fn areas() -> Value {
    json!({
        "global": {"paths": []},
        "docs": {"paths": ["docs/**"]},
        "code": {"paths": ["src/**"]},
    })
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

/// The first seven characters of `sha`.
fn short(sha: &str) -> &str {
    &sha[..7]
}

/// The `review` key of a typical project: two tiers with budgets, an exempt
/// author, and an exempt branch glob.
fn budgets() -> Value {
    json!({"bundle": {
        "budgets": {"light": 3, "standard": 100},
        "exemptAuthors": ["dependabot[bot]"],
        "exemptBranches": ["release-please--*"],
    }})
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
    /// A repository whose install in `folder` holds the marker with the `review`
    /// key `review`, the knowledge base, `docs/guide.md`, the plan files
    /// `docs/plans/with-why.md` and `docs/plans/no-why.md`, and `src/lib.rs`
    /// (10 lines). With `tracker`, the install has the JSON tracker with the
    /// backlog of the `mini` fixture, whose item `HR-901` has the title `Write
    /// the mini-tools smoke test`. The base commit holds all of it.
    fn new(folder: &'static str, review: Option<Value>, tracker: bool) -> Self {
        let install = Self {
            top: scratch_git_repo(),
            folder,
            scratch: tempfile::tempdir().expect("tempdir"),
        };
        let mut marker = json!({"schema": "houserules/install@2.3.0", "idPrefix": "WI"});
        if tracker {
            marker["idPrefix"] = json!("HR");
            marker["tracker"] = json!({"adapter": "json"});
        }
        if let Some(review) = review {
            marker["review"] = review;
        }
        install.write(
            ".houserules.json",
            &serde_json::to_string_pretty(&marker).expect("serialize the marker"),
        );
        install.write("knowledge/schema.json", "{}\n");
        install.write("knowledge/areas.json", &areas().to_string());
        install.write("knowledge/team.json", &json!({"entries": []}).to_string());
        install.write("docs/guide.md", &numbered("guide", 10));
        install.write(
            "docs/plans/with-why.md",
            "# A plan with a why\n\nBecause the change needs it.\n\n- [x] one\n- [ ] two\n",
        );
        install.write("docs/plans/no-why.md", "# A plan with no why\n");
        install.write("src/lib.rs", &numbered("lib", 10));
        if tracker {
            copy_dir(
                &repo_root().join("tests/fixtures/mini/backlog"),
                &install.root().join("backlog"),
            );
        }
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

    /// Runs `command` through `houserules capture` at the git top level and
    /// gives the capture file `name` of the scratch directory. The command may
    /// fail: the capture holds its exit.
    fn capture(&self, name: &str, command: &[&str]) -> PathBuf {
        let out = self.scratch_file(name);
        capture_in(self.top.path(), &out, command);
        out
    }

    /// Runs `bundle check` over the description `body` for the range `base` to
    /// `head`, with the arguments `extra`.
    fn check(&self, body: &str, base: &str, head: &str, extra: &[&str]) -> Output {
        let target = self.scratch_file("description.md");
        fs::write(&target, body).expect("write the description");
        houserules()
            .args(["bundle", "check", "--body"])
            .arg(&target)
            .args(["--base", base, "--head", head])
            .args(extra)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run bundle check")
    }
}

/// Runs `command` through `houserules capture` with the working directory
/// `cwd`, writing the capture file `out`.
fn capture_in(cwd: &Path, out: &Path, command: &[&str]) {
    houserules()
        .arg("capture")
        .arg("--out")
        .arg(out)
        .arg("--")
        .args(command)
        .current_dir(cwd)
        .output()
        .expect("run capture");
    assert!(out.exists(), "capture wrote no file");
}

/// The first line of the capture file `path`: its header.
fn header_line(path: &Path) -> String {
    fs::read_to_string(path)
        .expect("read the capture")
        .lines()
        .next()
        .expect("a header line")
        .to_string()
}

/// A change with one commit on top of the base: the install, the range, and a
/// capture of `sh -c 'echo ok'` that ran at the head.
struct Request {
    install: Install,
    base: String,
    head: String,
    /// The header line of the capture that ran at the head.
    run: String,
}

impl Request {
    /// The request of an install with the `review` key `review` and, with
    /// `tracker`, the JSON tracker. The commit adds five lines to `src/lib.rs`.
    fn new(review: Option<Value>, tracker: bool) -> Self {
        let install = Install::new("", review, tracker);
        let base = install.head();
        install.append("src/lib.rs", 5);
        let head = install.commit("feat: add five lines");
        let run = header_line(&install.capture("run.txt", &["sh", "-c", "echo ok"]));
        Self {
            install,
            base,
            head,
            run,
        }
    }

    /// `bundle check` of `body` over the range of the request.
    fn check(&self, body: &str, extra: &[&str]) -> Output {
        self.install.check(body, &self.base, &self.head, extra)
    }

    /// The `ok` line of a clean bundle of one run (spec 7.3).
    fn ok_line(&self) -> String {
        format!(
            "bundle check: ok (1 runs at {}, diff 5 lines)\n",
            short(&self.head)
        )
    }

    /// The section of a clean bundle: the parts of `standard()` and the run at
    /// the head.
    fn clean(&self) -> String {
        section(&standard(), &[run_block(&self.run, &["ok"])])
    }

    /// The section with the parts `pairs` and the run at the head.
    fn with_parts(&self, pairs: &[(&str, &str)]) -> String {
        section(pairs, &[run_block(&self.run, &["ok"])])
    }
}

/// The section of a bundle: the heading, a blank line, one line for each pair,
/// and one block for each of `runs`.
fn section(pairs: &[(&str, &str)], runs: &[String]) -> String {
    let mut text = format!("{HEADING}\n\n");
    for (label, value) in pairs {
        text.push_str(&format!("{label}: {value}\n"));
    }
    for run in runs {
        text.push('\n');
        text.push_str(run);
    }
    text
}

/// The block of a run: its header line and the `tail` lines.
fn run_block(header: &str, tail: &[&str]) -> String {
    let mut text = format!("```houserules-run\n{header}\n");
    for line in tail {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("```\n");
    text
}

/// The parts of a clean bundle.
fn standard() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Purpose", "Add five lines."),
        ("Work items", "HR-901"),
        ("Risk tier", "standard"),
        ("Criterion", "A small change."),
    ]
}

/// `parts` without the line `label`.
fn without<'a>(parts: &[(&'a str, &'a str)], label: &str) -> Vec<(&'a str, &'a str)> {
    parts
        .iter()
        .copied()
        .filter(|(name, _)| *name != label)
        .collect()
}

/// `parts` with the value of the line `label` replaced by `value`.
fn with_value<'a>(
    parts: &[(&'a str, &'a str)],
    label: &str,
    value: &'a str,
) -> Vec<(&'a str, &'a str)> {
    parts
        .iter()
        .map(|(name, old)| (*name, if *name == label { value } else { *old }))
        .collect()
}

/// The lines of the standard output of `output`.
fn lines(output: &Output) -> Vec<String> {
    stdout(output).lines().map(str::to_string).collect()
}

/// The text of the error of `fs::write` into a folder that does not exist: the
/// operating system's own words, taken from the same call as the code makes.
fn unwritable(target: &Path) -> String {
    fs::write(target, "")
        .expect_err("the folder does not exist")
        .to_string()
}

// ---- bundle check: the clean case -----------------------------------------------------

/// Case 3: `bundle write` then `bundle check` passes at the capture's head, and
/// prints the `ok` line of spec 7.3 with the run count, the head, and the diff.
#[test]
fn a_written_bundle_passes_at_the_head_of_its_captures() {
    let request = Request::new(Some(budgets()), true);
    let capture = request.install.scratch_file("run.txt");

    let written = houserules()
        .args([
            "bundle",
            "write",
            "--purpose",
            "Add five lines.",
            "--work-item",
            "HR-901",
            "--tier",
            "standard",
            "--criterion",
            "A small change.",
            "--capture",
        ])
        .arg(&capture)
        .output()
        .expect("run bundle write");
    assert_eq!(written.status.code(), Some(0), "{}", stderr(&written));
    assert_eq!(stderr(&written), "");

    let output = request.check(&stdout(&written), &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// A clean bundle of two runs counts both.
#[test]
fn the_ok_line_counts_every_run() {
    let request = Request::new(Some(budgets()), true);
    let second = header_line(
        &request
            .install
            .capture("second.txt", &["sh", "-c", "echo two"]),
    );
    let body = section(
        &standard(),
        &[
            run_block(&request.run, &["ok"]),
            run_block(&second, &["two"]),
        ],
    );

    let output = request.check(&body, &[]);

    assert_output(
        &output,
        0,
        &format!(
            "bundle check: ok (2 runs at {}, diff 5 lines)\n",
            short(&request.head)
        ),
        "",
    );
}

/// The output is the same for the same inputs, byte for byte.
#[test]
fn the_same_description_gives_the_same_bytes_twice() {
    let request = Request::new(Some(budgets()), true);
    let body = request.with_parts(&without(&standard(), "Risk tier"));

    let first = request.check(&body, &[]);
    let second = request.check(&body, &[]);

    assert_eq!(first.status.code(), Some(1));
    assert_eq!(
        (first.status.code(), first.stdout, first.stderr),
        (second.status.code(), second.stdout, second.stderr)
    );
}

/// A subdirectory install checks like a top-level one.
#[test]
fn a_subdirectory_install_checks_like_a_top_level_one() {
    let install = Install::new("sub", Some(budgets()), true);
    let base = install.head();
    install.append("src/lib.rs", 5);
    let head = install.commit("feat: add five lines");
    let run = header_line(&install.capture("run.txt", &["sh", "-c", "echo ok"]));

    let output = install.check(
        &section(&standard(), &[run_block(&run, &["ok"])]),
        &base,
        &head,
        &[],
    );

    assert_output(
        &output,
        0,
        &format!(
            "bundle check: ok (1 runs at {}, diff 5 lines)\n",
            short(&head)
        ),
        "",
    );
}

// ---- bundle check: the section --------------------------------------------------------

/// A description with no section fails with one line.
#[test]
fn a_description_with_no_section_fails() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check("Just a description.\n", &[]);

    assert_output(&output, 1, &format!("{NO_SECTION}\n"), "");
}

/// An empty description has no section either.
#[test]
fn an_empty_description_has_no_section() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check("", &[]);

    assert_output(&output, 1, &format!("{NO_SECTION}\n"), "");
}

/// Two sections fail with the count, and nothing else is checked.
#[test]
fn two_sections_fail_with_the_count() {
    let request = Request::new(Some(budgets()), true);
    let body = format!("{}\n{}", request.clean(), request.clean());

    let output = request.check(&body, &[]);

    assert_output(
        &output,
        1,
        "bundle check: 2 \"## Evidence bundle\" sections; keep one\n",
        "",
    );
}

/// A section ends at the next `## ` line, and a later heading of another name
/// is no second section.
#[test]
fn another_heading_ends_the_section() {
    let request = Request::new(Some(budgets()), true);
    let body = format!(
        "Intro.\n\n{}\n## Notes\n\nPurpose: not this one.\n",
        request.clean()
    );

    let output = request.check(&body, &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// Case 3: a `## Evidence bundle` line inside a fenced block of another section
/// counts as no section.
#[test]
fn a_heading_inside_a_fence_of_another_section_is_no_section() {
    let request = Request::new(Some(budgets()), true);
    let body = "## Notes\n\n```\n## Evidence bundle\nPurpose: no\n```\n";

    let output = request.check(body, &[]);

    assert_output(&output, 1, &format!("{NO_SECTION}\n"), "");
}

/// A `## Evidence bundle` line inside a fenced block of the section itself is no
/// second section, and a `## ` line there ends nothing.
#[test]
fn a_heading_inside_a_fence_of_the_section_changes_nothing() {
    let request = Request::new(Some(budgets()), true);
    let body = section(
        &standard(),
        &[run_block(
            &request.run,
            &[
                "## Evidence bundle",
                "## Not a heading",
                "Purpose: not this",
            ],
        )],
    );

    let output = request.check(&body, &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// A fence closes at a run of backticks at least as long as its opening, so a
/// shorter run inside it holds the heading.
#[test]
fn a_longer_fence_holds_a_shorter_one() {
    let request = Request::new(Some(budgets()), true);
    let body = format!(
        "{}\n````\n```\n## Evidence bundle\n```\n````\n",
        request.clean()
    );

    let output = request.check(&body, &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// A fence that nothing closes holds the rest of the description, so the run
/// block of the section is a run.
#[test]
fn an_unclosed_fence_holds_the_rest_of_the_description() {
    let request = Request::new(Some(budgets()), true);
    let body = format!(
        "{HEADING}\n\nPurpose: Add five lines.\nWork items: HR-901\nRisk tier: standard\nCriterion: A small change.\n\n```houserules-run\n{}\n",
        request.run
    );

    let output = request.check(&body, &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// Review focus 1: a description with CRLF line ends parses like the same text
/// with LF line ends. `## Evidence bundle\r` is the heading, each line parses
/// without its `\r`, and the header line of a run compares without it.
#[test]
fn a_crlf_description_parses_like_lf() {
    let request = Request::new(Some(budgets()), true);
    let body = request.clean();
    let crlf = body.replace('\n', "\r\n");

    let lf = request.check(&body, &[]);
    let output = request.check(&crlf, &[]);

    assert_output(&lf, 0, &request.ok_line(), "");
    assert_eq!(
        (output.status.code(), stdout(&output), stderr(&output)),
        (Some(0), request.ok_line(), String::new())
    );
}

/// A CRLF description fails with the same lines as the LF one.
#[test]
fn a_crlf_description_fails_with_the_same_lines() {
    let request = Request::new(Some(budgets()), true);
    let body = request.with_parts(&without(&standard(), "Criterion"));

    let lf = request.check(&body, &[]);
    let crlf = request.check(&body.replace('\n', "\r\n"), &[]);

    assert_eq!(lf.status.code(), Some(1));
    assert_eq!(stdout(&lf), "bundle check: no Criterion line\n");
    assert_eq!(
        (crlf.status.code(), stdout(&crlf)),
        (lf.status.code(), stdout(&lf))
    );
}

// ---- bundle check: the lines ----------------------------------------------------------

/// Case 3: each missing line fails with its own line, and an empty one counts as
/// missing.
#[test]
fn each_missing_line_fails_with_its_line() {
    let request = Request::new(Some(budgets()), true);
    for label in ["Purpose", "Work items", "Risk tier", "Criterion"] {
        let missing = request.check(&request.with_parts(&without(&standard(), label)), &[]);
        let empty = request.check(
            &request.with_parts(&with_value(&standard(), label, "")),
            &[],
        );

        let expected = format!("bundle check: no {label} line\n");
        assert_output(&missing, 1, &expected, "");
        assert_output(&empty, 1, &expected, "");
    }
}

/// A work-items line of only commas and spaces names no work item.
#[test]
fn a_work_items_line_of_separators_names_none() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check(
        &request.with_parts(&with_value(&standard(), "Work items", " , ,")),
        &[],
    );

    assert_output(&output, 1, "bundle check: no Work items line\n", "");
}

/// Case 3: several missing lines print in the order of the spec, one line each.
#[test]
fn several_missing_lines_print_in_order() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check(&request.with_parts(&[("Risk tier", "standard")]), &[]);

    assert_output(
        &output,
        1,
        "bundle check: no Purpose line\nbundle check: no Work items line\nbundle check: no Criterion line\n",
        "",
    );
}

/// A label is read at the start of a line only, and the first line wins.
#[test]
fn a_label_is_read_at_the_start_of_a_line_and_the_first_one_wins() {
    let request = Request::new(Some(budgets()), true);
    let body = request.with_parts(&[
        ("Purpose", "Add five lines."),
        ("Work items", "HR-901"),
        ("Risk tier", "standard"),
        ("Criterion", "A small change."),
        ("Risk tier", "light"),
    ]);
    let indented = body.replace("Purpose:", " Purpose:");

    let output = request.check(&body, &[]);
    let indented = request.check(&indented, &[]);

    assert_output(&output, 0, &request.ok_line(), "");
    assert_output(&indented, 1, "bundle check: no Purpose line\n", "");
}

// ---- bundle check: the intent ---------------------------------------------------------

/// Case 3: an `Intent: file:<path>` with a why passes without `Purpose:`.
#[test]
fn an_intent_with_a_why_stands_in_for_the_purpose() {
    let request = Request::new(Some(budgets()), true);
    let mut parts = without(&standard(), "Purpose");
    parts.insert(0, ("Intent", "file:docs/plans/with-why.md"));

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// Case 3: an intent with no why fails `Purpose:`.
#[test]
fn an_intent_with_no_why_leaves_the_purpose_required() {
    let request = Request::new(Some(budgets()), true);
    let mut parts = without(&standard(), "Purpose");
    parts.insert(0, ("Intent", "file:docs/plans/no-why.md"));

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(&output, 1, "bundle check: no Purpose line\n", "");
}

/// A work-item intent has a title and no why, so it leaves the purpose required.
#[test]
fn a_work_item_intent_leaves_the_purpose_required() {
    let request = Request::new(Some(budgets()), true);
    let mut parts = without(&standard(), "Purpose");
    parts.insert(0, ("Intent", "work-item:HR-901"));

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(&output, 1, "bundle check: no Purpose line\n", "");
}

/// An intent beside a purpose passes.
#[test]
fn an_intent_beside_a_purpose_passes() {
    let request = Request::new(Some(budgets()), true);
    let mut parts = standard();
    parts.insert(1, ("Intent", "file:docs/plans/with-why.md"));

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// Case 3: an intent that no adapter resolves fails with the adapter's line:
/// no such adapter, no such file at the head, a path outside the install, a
/// work item that the tracker does not hold, and a text that is no
/// `<source>:<ref>`. The intent prints as a code span.
#[test]
fn an_intent_that_does_not_resolve_fails_with_the_line_of_its_adapter() {
    let request = Request::new(Some(budgets()), true);
    let cases = [
        (
            "openspec:add-rate-limit",
            "bundle check: intent `openspec:add-rate-limit`: \"openspec\" is no intent source; use one of: work-item, file\n".to_string(),
        ),
        (
            "file:docs/plans/missing.md",
            format!("bundle check: intent `file:docs/plans/missing.md`: {NO_REGULAR_FILE}\n"),
        ),
        (
            "file:../outside.md",
            "bundle check: intent `file:../outside.md`: the path must be relative to the install, with no empty, . or .. folder\n".to_string(),
        ),
        (
            "work-item:HR-9999",
            format!("bundle check: intent `work-item:HR-9999`: {NO_SUCH_WORK_ITEM}\n"),
        ),
        (
            "plan",
            "bundle check: intent `plan`: an intent is written <source>:<ref>\n".to_string(),
        ),
    ];
    for (intent, line) in cases {
        let mut parts = standard();
        parts.insert(1, ("Intent", intent));

        let output = request.check(&request.with_parts(&parts), &[]);

        assert_output(&output, 1, &line, "");
    }
}

/// The request template's placeholder `Intent: <source>:<ref>` fails the check
/// until its author fills it in (spec 7.4).
#[test]
fn the_placeholder_intent_of_the_template_fails() {
    let request = Request::new(Some(budgets()), true);
    let mut parts = standard();
    parts.insert(1, ("Intent", "<source>:<ref>"));

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(
        &output,
        1,
        "bundle check: intent `<source>:<ref>`: \"<source>\" is no intent source; use one of: work-item, file\n",
        "",
    );
}

/// An intent whose source the config leaves out of `review.intent.sources` still
/// resolves: the bundle's line names one intent directly (spec 4.9).
#[test]
fn a_named_intent_resolves_whatever_the_config_lists() {
    let review = json!({"bundle": budgets()["bundle"], "intent": {"sources": ["work-item"]}});
    let request = Request::new(Some(review), true);
    let mut parts = without(&standard(), "Purpose");
    parts.insert(0, ("Intent", "file:docs/plans/with-why.md"));

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

// ---- bundle check: the work items -----------------------------------------------------

/// Case 3: a work item that the tracker does not resolve fails with the line of
/// the adapter; one that it holds passes.
#[test]
fn a_work_item_that_the_tracker_does_not_hold_fails() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check(
        &request.with_parts(&with_value(&standard(), "Work items", "HR-901, HR-9999")),
        &[],
    );

    assert_output(
        &output,
        1,
        &format!("bundle check: work item `HR-9999`: {NO_SUCH_WORK_ITEM}\n"),
        "",
    );
}

/// Case 3: with no tracker, the line counts the work items and the check goes on
/// with exit 0 for this part, as `audit` does.
#[test]
fn with_no_tracker_the_work_items_are_counted_and_the_check_goes_on() {
    let request = Request::new(Some(budgets()), false);

    let output = request.check(
        &request.with_parts(&with_value(&standard(), "Work items", "HR-1, HR-2")),
        &[],
    );

    assert_output(
        &output,
        0,
        &format!("{TWO_NOT_RESOLVED}\n{}", request.ok_line()),
        "",
    );
}

/// With no tracker, a failure elsewhere still exits 1 beside the line.
#[test]
fn with_no_tracker_another_finding_still_fails() {
    let request = Request::new(Some(budgets()), false);
    let parts = with_value(
        &without(&standard(), "Criterion"),
        "Work items",
        "HR-1, HR-2",
    );

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(
        &output,
        1,
        &format!("bundle check: no Criterion line\n{TWO_NOT_RESOLVED}\n"),
        "",
    );
}

/// A work item that the bundle names twice counts once.
#[test]
fn a_work_item_named_twice_counts_once() {
    let request = Request::new(Some(budgets()), false);

    let output = request.check(
        &request.with_parts(&with_value(&standard(), "Work items", "HR-1, HR-1")),
        &[],
    );

    assert_output(
        &output,
        0,
        &format!(
            "bundle check: 1 work items not resolved: no tracker (run houserules init --tracker <name>)\n{}",
            request.ok_line()
        ),
        "",
    );
}

/// A request whose install has the JSON tracker and no `backlog/` folder: the
/// tracker cannot be reached (spec 3.4).
fn request_with_an_unreachable_tracker() -> Request {
    let request = Request::new(Some(budgets()), true);
    fs::remove_dir_all(request.install.root().join("backlog")).expect("remove the backlog");
    request
}

/// Spec 7.3 and 4.9: with a tracker that cannot be reached, the line counts the
/// work items and gives the line of the tracker, and the check goes on with exit
/// 0 for this part; no ok line hides the work items that nothing verified.
#[test]
fn with_an_unreachable_tracker_the_work_items_are_counted_and_the_check_goes_on() {
    let request = request_with_an_unreachable_tracker();

    let output = request.check(
        &request.with_parts(&with_value(&standard(), "Work items", "HR-1, HR-2")),
        &[],
    );

    assert_output(
        &output,
        0,
        &format!("{TWO_UNREACHABLE}\n{}", request.ok_line()),
        "",
    );
}

/// With a tracker that cannot be reached, a failure elsewhere still exits 1
/// beside the line.
#[test]
fn with_an_unreachable_tracker_another_finding_still_fails() {
    let request = request_with_an_unreachable_tracker();
    let parts = with_value(
        &without(&standard(), "Criterion"),
        "Work items",
        "HR-1, HR-2",
    );

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(
        &output,
        1,
        &format!("bundle check: no Criterion line\n{TWO_UNREACHABLE}\n"),
        "",
    );
}

/// A tracker that fails for a reason other than being out of reach is a setup
/// error: one named line with the prefix, exit 2.
#[test]
fn a_tracker_that_fails_exits_2_with_the_prefixed_line() {
    let request = Request::new(Some(budgets()), true);
    request.install.write("backlog/batches.json", "not json");

    let output = request.check(&request.clean(), &[]);

    assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let printed = stderr(&output);
    assert_eq!(printed.lines().count(), 1, "{printed}");
    assert!(printed.starts_with("bundle check: "), "{printed}");
}

// ---- bundle check: the runs -----------------------------------------------------------

/// Case 3: a bundle with no run block fails.
#[test]
fn a_bundle_with_no_run_fails() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check(&section(&standard(), &[]), &[]);

    assert_output(&output, 1, "bundle check: no houserules-run block\n", "");
}

/// A fenced block of another kind is no run.
#[test]
fn a_block_of_another_kind_is_no_run() {
    let request = Request::new(Some(budgets()), true);
    let body = format!(
        "{}\n```text\n{}\n```\n",
        section(&standard(), &[]),
        request.run
    );

    let output = request.check(&body, &[]);

    assert_output(&output, 1, "bundle check: no houserules-run block\n", "");
}

/// Case 3: a run whose first line is no capture header fails, and the runs
/// count from 1.
#[test]
fn a_run_with_no_header_fails() {
    let request = Request::new(Some(budgets()), true);
    let body = section(
        &standard(),
        &[
            run_block(&request.run, &["ok"]),
            run_block("this is no header", &["ok"]),
            "```houserules-run\n```\n".to_string(),
        ],
    );

    let output = request.check(&body, &[]);

    assert_output(
        &output,
        1,
        "bundle check: run 2: the first line is no capture header\nbundle check: run 3: the first line is no capture header\n",
        "",
    );
}

/// A header line that lacks a key is no capture header.
#[test]
fn a_header_that_lacks_a_key_is_no_header() {
    let request = Request::new(Some(budgets()), true);
    let mut header: Value = serde_json::from_str(&request.run).expect("a JSON header");
    header.as_object_mut().expect("an object").remove("exit");

    let output = request.check(
        &section(&standard(), &[run_block(&header.to_string(), &[])]),
        &[],
    );

    assert_output(
        &output,
        1,
        "bundle check: run 1: the first line is no capture header\n",
        "",
    );
}

/// Case 3: a run at another commit fails with both commits, and prints its
/// command shell-quoted in a code span.
#[test]
fn a_run_at_another_head_fails() {
    let install = Install::new("", Some(budgets()), true);
    let base = install.head();
    let older = header_line(&install.capture("older.txt", &["sh", "-c", "echo ok"]));
    install.append("src/lib.rs", 5);
    let head = install.commit("feat: add five lines");

    let output = install.check(
        &section(&standard(), &[run_block(&older, &["ok"])]),
        &base,
        &head,
        &[],
    );

    assert_output(
        &output,
        1,
        &format!(
            "bundle check: run 1 (`sh -c 'echo ok'`) ran at `{}`, not at {}\n",
            short(&base),
            short(&head)
        ),
        "",
    );
}

/// A run outside a git work tree has no head: the line names it as `none`.
#[test]
fn a_run_outside_a_work_tree_has_no_head() {
    let request = Request::new(Some(budgets()), true);
    let outside = tempfile::tempdir().expect("tempdir");
    let out = request.install.scratch_file("outside.txt");
    capture_in(outside.path(), &out, &["sh", "-c", "echo ok"]);

    let output = request.check(
        &section(&standard(), &[run_block(&header_line(&out), &["ok"])]),
        &[],
    );

    assert_output(
        &output,
        1,
        &format!(
            "bundle check: run 1 (`sh -c 'echo ok'`) ran at none, not at {}\n",
            short(&request.head)
        ),
        "",
    );
}

/// Case 3: a run in a dirty work tree fails.
#[test]
fn a_dirty_run_fails() {
    let request = Request::new(Some(budgets()), true);
    request.install.write("scratch.txt", "untracked\n");
    let dirty = header_line(
        &request
            .install
            .capture("dirty.txt", &["sh", "-c", "echo ok"]),
    );

    let output = request.check(&section(&standard(), &[run_block(&dirty, &["ok"])]), &[]);

    assert_output(
        &output,
        1,
        "bundle check: run 1 (`sh -c 'echo ok'`) ran in a dirty work tree\n",
        "",
    );
}

/// Case 3: a run that failed fails with its exit code.
#[test]
fn a_failed_run_fails_with_its_exit_code() {
    let request = Request::new(Some(budgets()), true);
    let failed = header_line(
        &request
            .install
            .capture("failed.txt", &["sh", "-c", "exit 3"]),
    );

    let output = request.check(&section(&standard(), &[run_block(&failed, &[])]), &[]);

    assert_output(
        &output,
        1,
        "bundle check: run 1 (`sh -c 'exit 3'`) exited 3\n",
        "",
    );
}

/// A run that a signal ended has no exit code: the line names the signal. Unix
/// only: a signal needs a POSIX shell.
#[cfg(unix)]
#[test]
fn a_run_that_a_signal_ended_fails_with_the_signal() {
    let request = Request::new(Some(budgets()), true);
    let killed = header_line(
        &request
            .install
            .capture("killed.txt", &["sh", "-c", "kill -9 $$"]),
    );

    let output = request.check(&section(&standard(), &[run_block(&killed, &[])]), &[]);

    assert_output(
        &output,
        1,
        "bundle check: run 1 (`sh -c 'kill -9 $$'`) ended by signal 9\n",
        "",
    );
}

/// A run can fail in several ways at once; the lines print in the order of the
/// spec: the head, the work tree, the exit.
#[test]
fn a_run_with_several_faults_prints_each_in_order() {
    let request = Request::new(Some(budgets()), true);
    request.install.write("scratch.txt", "untracked\n");
    let run = header_line(&request.install.capture("both.txt", &["sh", "-c", "exit 4"]));
    let mut header: Value = serde_json::from_str(&run).expect("a JSON header");
    header["head"] = json!("a".repeat(40));
    let run = header.to_string();

    let output = request.check(&section(&standard(), &[run_block(&run, &[])]), &[]);

    assert_output(
        &output,
        1,
        &format!(
            "bundle check: run 1 (`sh -c 'exit 4'`) ran at `aaaaaaa`, not at {}\nbundle check: run 1 (`sh -c 'exit 4'`) ran in a dirty work tree\nbundle check: run 1 (`sh -c 'exit 4'`) exited 4\n",
            short(&request.head)
        ),
        "",
    );
}

// ---- bundle check: the budget ---------------------------------------------------------

/// Case 3: a risk tier that has no budget fails with the tiers that have one.
#[test]
fn an_unknown_tier_fails_with_the_budget_keys() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check(
        &request.with_parts(&with_value(&standard(), "Risk tier", "critical")),
        &[],
    );

    assert_output(
        &output,
        1,
        "bundle check: risk tier \"`critical`\" has no budget; review.bundle.budgets names `light`, `standard`\n",
        "",
    );
}

/// Case 3: a diff above the budget of the tier fails without a reason, and
/// passes with one.
#[test]
fn a_diff_over_the_budget_needs_a_reason() {
    let request = Request::new(Some(budgets()), true);
    let light = with_value(&standard(), "Risk tier", "light");

    let output = request.check(&request.with_parts(&light), &[]);
    assert_output(
        &output,
        1,
        "bundle check: diff 5 lines > `light` budget 3; add a Budget reason line\n",
        "",
    );

    let mut with_reason = light.clone();
    with_reason.push(("Budget reason", "A mechanical change."));
    let output = request.check(&request.with_parts(&with_reason), &[]);
    assert_output(&output, 0, &request.ok_line(), "");
}

/// A diff of exactly the budget passes: the line says "above".
#[test]
fn a_diff_at_the_budget_passes() {
    let review = json!({"bundle": {"budgets": {"standard": 5}}});
    let request = Request::new(Some(review), true);

    let output = request.check(&request.clean(), &[]);

    assert_output(&output, 0, &request.ok_line(), "");
}

/// An empty `Budget reason:` line is no reason.
#[test]
fn an_empty_budget_reason_is_no_reason() {
    let request = Request::new(Some(budgets()), true);
    let mut parts = with_value(&standard(), "Risk tier", "light");
    parts.push(("Budget reason", ""));

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(
        &output,
        1,
        "bundle check: diff 5 lines > `light` budget 3; add a Budget reason line\n",
        "",
    );
}

/// Case 3: with no budgets, the check prints its line and reads no size.
#[test]
fn with_no_budgets_no_size_is_checked() {
    let request = Request::new(None, true);

    let output = request.check(
        &request.with_parts(&with_value(&standard(), "Risk tier", "anything")),
        &[],
    );

    assert_output(
        &output,
        0,
        &format!("{NO_BUDGETS}\n{}", request.ok_line()),
        "",
    );
}

/// With no budgets, a finding still fails beside the line.
#[test]
fn with_no_budgets_a_finding_still_fails() {
    let request = Request::new(None, true);

    let output = request.check(&request.with_parts(&without(&standard(), "Purpose")), &[]);

    assert_output(
        &output,
        1,
        &format!("bundle check: no Purpose line\n{NO_BUDGETS}\n"),
        "",
    );
}

/// Case 3: a `-diff` attribute in the request's own `.gitattributes` does not
/// lower the diff that the budget reads.
#[test]
fn a_diff_attribute_in_the_request_does_not_lower_the_budget_diff() {
    let install = Install::new("", Some(budgets()), true);
    install.write(".gitattributes", "*.rs -diff\n");
    install.commit("chore: attributes");
    let base = install.head();
    install.append("src/lib.rs", 30);
    let head = install.commit("feat: add thirty lines");
    let run = header_line(&install.capture("run.txt", &["sh", "-c", "echo ok"]));
    let parts = with_value(&standard(), "Risk tier", "light");

    let output = install.check(&section(&parts, &[run_block(&run, &[])]), &base, &head, &[]);

    assert_output(
        &output,
        1,
        "bundle check: diff 30 lines > `light` budget 3; add a Budget reason line\n",
        "",
    );
}

/// A path that `review.diffExclude` names leaves the diff that the budget reads.
#[test]
fn an_excluded_path_leaves_the_budget_diff() {
    let review = json!({
        "diffExclude": ["src/**"],
        "bundle": {"budgets": {"light": 3, "standard": 100}},
    });
    let request = Request::new(Some(review), true);
    let parts = with_value(&standard(), "Risk tier", "light");

    let output = request.check(&request.with_parts(&parts), &[]);

    assert_output(
        &output,
        0,
        &format!(
            "bundle check: ok (1 runs at {}, diff 0 lines)\n",
            short(&request.head)
        ),
        "",
    );
}

// ---- bundle check: exempt requests ----------------------------------------------------

/// Case 3: an exempt author passes with its line, whatever the description says.
#[test]
fn an_exempt_author_passes() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check("No section.\n", &["--author", "dependabot[bot]"]);

    assert_output(
        &output,
        0,
        "bundle check: `dependabot[bot]` is exempt\n",
        "",
    );
}

/// Case 3: an exempt branch passes with its line.
#[test]
fn an_exempt_branch_passes() {
    let request = Request::new(Some(budgets()), true);

    let output = request.check(
        "No section.\n",
        &["--head-ref", "release-please--branches--main"],
    );

    assert_output(
        &output,
        0,
        "bundle check: `release-please--branches--main` is exempt\n",
        "",
    );
}

/// An author that the config does not name, and a branch that no glob matches,
/// are not exempt; an empty value, as a unit passes when the host gives none,
/// is no value.
#[test]
fn another_author_or_branch_is_not_exempt() {
    let request = Request::new(Some(budgets()), true);

    for extra in [
        vec!["--author", "alice"],
        vec!["--head-ref", "feature/x"],
        vec!["--author", "", "--head-ref", ""],
        vec!["--author=", "--head-ref="],
        vec!["--author", "Dependabot[bot]"],
    ] {
        let output = request.check("No section.\n", &extra);

        assert_output(&output, 1, &format!("{NO_SECTION}\n"), "");
    }
}

/// Spec 13: a bot of GitLab (a project or group access token's user), a Renovate
/// request, and a release tool's request fail `bundle check` unless the config
/// names the user or a glob matches the branch; the config names exactly what it
/// exempts, and nothing else.
#[test]
fn the_author_classes_of_spec_13_pass_or_fail_by_the_config() {
    let review = json!({"bundle": {
        "exemptAuthors": ["dependabot[bot]", "project_12_bot_a1b2"],
        "exemptBranches": ["release-please--*", "renovate/**"],
    }});
    let request = Request::new(Some(review), true);

    for (extra, exempt) in [
        (vec!["--author", "dependabot[bot]"], true),
        (vec!["--author", "project_12_bot_a1b2"], true),
        (vec!["--author", "project_12_bot_c3d4"], false),
        (vec!["--author", "group_3_bot_e5f6"], false),
        (vec!["--author", "renovate[bot]"], false),
        (vec!["--head-ref", "release-please--branches--main"], true),
        (vec!["--head-ref", "renovate/major-serde"], true),
        (vec!["--head-ref", "renovate/npm/lodash-4.x"], true),
        (vec!["--head-ref", "release-please/x"], false),
        (
            vec!["--author", "renovate[bot]", "--head-ref", "renovate/serde"],
            true,
        ),
    ] {
        let output = request.check("No section.\n", &extra);

        let expected = if exempt {
            let who = extra[extra.len() - 1];
            (0, format!("bundle check: `{who}` is exempt\n"))
        } else {
            (1, format!("{NO_SECTION}\n"))
        };
        assert_eq!(
            (output.status.code(), stdout(&output)),
            (Some(expected.0), expected.1),
            "{extra:?}"
        );
    }
}

// ---- bundle check: untrusted text -----------------------------------------------------

/// Case 3: a value of the description that looks like a workflow command never
/// starts a line of the output, and every line starts with `bundle check: `.
#[test]
fn a_description_value_never_starts_a_printed_line() {
    let request = Request::new(Some(budgets()), true);
    let parts = [
        ("Purpose", "::error::x"),
        ("Intent", "::error::x"),
        ("Work items", "::error::x"),
        ("Risk tier", "::error::x"),
        ("Criterion", "::error::x"),
    ];
    let body = format!(
        "::error::x\n{}\n::warning::y\n",
        section(&parts, &[run_block("::error::x", &["::error::x"])])
    );

    let output = request.check(&body, &[]);

    assert_eq!(output.status.code(), Some(1));
    let printed = lines(&output);
    assert_eq!(printed.len(), 4, "{printed:?}");
    for line in &printed {
        assert!(line.starts_with("bundle check: "), "{line:?}");
    }
    assert!(
        printed.contains(&format!(
            "bundle check: work item `::error::x`: {NO_SUCH_WORK_ITEM}"
        )),
        "{printed:?}"
    );
}

/// A value with a control character prints as a JSON string inside its code
/// span, and a value with a backtick prints in a longer fence (spec 4.8).
#[test]
fn a_value_with_a_control_character_or_a_backtick_prints_whole() {
    let request = Request::new(Some(budgets()), true);
    let escape = request.check(
        &request.with_parts(&with_value(&standard(), "Risk tier", "red\u{1b}[31m")),
        &[],
    );
    let tick = request.check(
        &request.with_parts(&with_value(&standard(), "Risk tier", "a`b")),
        &[],
    );

    assert!(!stdout(&escape).contains('\u{1b}'), "{:?}", stdout(&escape));
    assert!(
        stdout(&escape).contains("risk tier \"`\"red\\u001b[31m\"`\" has no budget"),
        "{}",
        stdout(&escape)
    );
    assert!(
        stdout(&tick).contains("risk tier \"``a`b``\" has no budget"),
        "{}",
        stdout(&tick)
    );
}

/// An argument of a run with a control character prints as a JSON string.
#[test]
fn a_run_argument_with_a_control_character_prints_escaped() {
    let request = Request::new(Some(budgets()), true);
    let failed = header_line(
        &request
            .install
            .capture("tab.txt", &["sh", "-c", "exit 2", "x\ty"]),
    );

    let output = request.check(&section(&standard(), &[run_block(&failed, &[])]), &[]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout(&output).contains("run 1 (`\"sh -c 'exit 2' 'x\\ty'\"`) exited 2"),
        "{}",
        stdout(&output)
    );
}

// ---- bundle check: setup errors -------------------------------------------------------

/// A description file that cannot be read is one named line.
#[test]
fn an_unreadable_body_exits_2() {
    let request = Request::new(Some(budgets()), true);
    let missing = request.install.scratch_file("missing.md");
    let error = fs::read_to_string(&missing).expect_err("no such file");

    let output = houserules()
        .args(["bundle", "check", "--body"])
        .arg(&missing)
        .args(["--base", &request.base, "--head", &request.head, "--dir"])
        .arg(request.install.root())
        .output()
        .expect("run bundle check");

    assert_output(
        &output,
        2,
        "",
        &format!("bundle check: {}: {error}\n", missing.display()),
    );
}

/// A base that is no commit is one named line.
#[test]
fn a_base_that_is_no_commit_exits_2() {
    let request = Request::new(Some(budgets()), true);

    let output = request
        .install
        .check(&request.clean(), "no-such-ref", &request.head, &[]);

    assert_output(&output, 2, "", "bundle check: bad ref \"no-such-ref\"\n");
}

/// A head that is no commit is one named line.
#[test]
fn a_head_that_is_no_commit_exits_2() {
    let request = Request::new(Some(budgets()), true);

    let output = request
        .install
        .check(&request.clean(), &request.base, "no-such-ref", &[]);

    assert_output(&output, 2, "", "bundle check: bad ref \"no-such-ref\"\n");
}

/// A `review` key with a fault stops the command with the line that
/// `check-knowledge` prints for it.
#[test]
fn a_review_key_with_a_fault_exits_2_with_its_line() {
    let request = Request::new(
        Some(json!({"bundle": {"budgets": {"light": "many"}}})),
        true,
    );

    let output = request.check(&request.clean(), &[]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).starts_with("bundle check: .houserules.json: review.bundle.budgets.light"),
        "{}",
        stderr(&output)
    );
}

/// Spec 7.3: every line that `bundle check` prints starts with its prefix, the
/// lines of a setup error with several faults too.
#[test]
fn every_line_of_a_setup_error_starts_with_the_prefix() {
    let request = Request::new(
        Some(json!({"bundle": {"budgets": {"light": "many"}, "exemptAuthors": [1]}})),
        true,
    );

    let output = request.check(&request.clean(), &[]);

    assert_output(
        &output,
        2,
        "",
        "bundle check: .houserules.json: review.bundle.budgets.light must be a non-negative integer\nbundle check: .houserules.json: review.bundle.exemptAuthors must be an array of strings\n",
    );
}

/// `--body`, `--base`, and `--head` are required.
#[test]
fn the_body_the_base_and_the_head_are_required() {
    let request = Request::new(Some(budgets()), true);
    for missing in ["--body", "--base", "--head"] {
        let mut args = vec![
            "--body",
            "description.md",
            "--base",
            "main",
            "--head",
            "HEAD",
        ];
        let at = args
            .iter()
            .position(|arg| *arg == missing)
            .expect("an argument");
        args.drain(at..at + 2);

        let output = houserules()
            .args(["bundle", "check"])
            .args(&args)
            .arg("--dir")
            .arg(request.install.root())
            .output()
            .expect("run bundle check");

        assert_eq!(output.status.code(), Some(2), "{missing}");
        assert_eq!(stdout(&output), "");
        assert!(stderr(&output).contains(missing), "{}", stderr(&output));
    }
}

/// A git older than 2.41 stops the command with the line of spec 4.2; the shim
/// answers a call with `--version` and passes every other call to the real git.
/// Unix only: the shim is a POSIX shell script with an executable bit.
#[cfg(unix)]
#[test]
fn a_git_older_than_2_41_exits_2_with_its_line() {
    use std::os::unix::fs::PermissionsExt;

    let request = Request::new(Some(budgets()), true);
    let shim_dir = request.install.scratch_file("shim");
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
    let body = request.install.scratch_file("description.md");
    fs::write(&body, request.clean()).expect("write the description");

    let output = houserules()
        .args(["bundle", "check", "--body"])
        .arg(&body)
        .args(["--base", &request.base, "--head", &request.head, "--dir"])
        .arg(request.install.root())
        .env("PATH", std::env::join_paths(paths).expect("join the PATH"))
        .env("HOUSERULES_TEST_REAL_PATH", real_path)
        .output()
        .expect("run bundle check");

    assert_output(
        &output,
        2,
        "",
        "bundle check: git 2.39.0 is older than 2.41, which --attr-source needs\n",
    );
}

// ---- bundle write ---------------------------------------------------------------------

/// The capture file `name` of `dir`, written by `houserules capture` in `dir`
/// from `sh -c <script>`.
fn capture_of(dir: &Path, name: &str, script: &str) -> PathBuf {
    let out = dir.join(name);
    capture_in(dir, &out, &["sh", "-c", script]);
    out
}

/// A plain directory that is no git repository, with the capture `run.txt` of a
/// script that prints the lines `line 1` to `line 25`.
fn plain_with_capture() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(
        dir.path(),
        "run.txt",
        "i=1; while [ $i -le 25 ]; do echo \"line $i\"; i=$((i + 1)); done",
    );
    (dir, capture)
}

/// The arguments of `bundle write` that every case needs.
fn write_args() -> Vec<&'static str> {
    vec![
        "bundle",
        "write",
        "--purpose",
        "Add five lines.",
        "--work-item",
        "HR-901",
        "--tier",
        "standard",
        "--criterion",
        "A small change.",
    ]
}

/// Case 3 and spec 7.1: `bundle write` prints the section, in the order of the
/// spec: the heading, a blank line, the lines, and a block for each capture
/// with its header line byte for byte and the last 20 lines of its output.
#[test]
fn bundle_write_prints_the_section_of_spec_7_1() {
    let (dir, capture) = plain_with_capture();
    let header = header_line(&capture);
    let tail: String = (6..=25).map(|n| format!("line {n}\n")).collect();

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .current_dir(dir.path())
        .output()
        .expect("run bundle write");

    assert_output(
        &output,
        0,
        &format!(
            "## Evidence bundle\n\nPurpose: Add five lines.\nWork items: HR-901\nRisk tier: standard\nCriterion: A small change.\n\n```houserules-run\n{header}\n{tail}```\n"
        ),
        "",
    );
}

/// `--tail` sets the number of lines, and `--tail 0` copies the header alone.
#[test]
fn tail_sets_the_number_of_output_lines() {
    let (dir, capture) = plain_with_capture();
    let header = header_line(&capture);
    let run = |tail: &str| {
        let output = houserules()
            .args(write_args())
            .arg("--capture")
            .arg(&capture)
            .args(["--tail", tail])
            .current_dir(dir.path())
            .output()
            .expect("run bundle write");
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        stdout(&output)
    };

    assert!(
        run("2").ends_with(&format!(
            "```houserules-run\n{header}\nline 24\nline 25\n```\n"
        )),
        "{}",
        run("2")
    );
    assert!(
        run("0").ends_with(&format!("```houserules-run\n{header}\n```\n")),
        "{}",
        run("0")
    );
    assert!(
        run("100").contains(&format!("{header}\nline 1\nline 2\n")),
        "{}",
        run("100")
    );
}

/// Spec 7.2: the block holds the last `--tail` lines of the output, one line of
/// the block for each line of the output, whatever the line holds: an empty line
/// is a line, and a lone carriage return is no line end, so it is written as a
/// `\u` escape like the other control characters.
#[test]
fn the_block_holds_exactly_the_last_tail_lines_of_the_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (at, (script, tail, lines)) in [
        ("printf 'a\\n\\n'", "2", "a\n\n"),
        ("printf 'x\\na\\n\\n'", "2", "a\n\n"),
        ("printf '\\n'", "2", "\n"),
        ("printf '\\n\\n\\n'", "2", "\n\n"),
        ("printf 'one\\n50%%\\r100%%\\n'", "1", "50%\\u000d100%\n"),
        (
            "printf 'one\\n50%%\\r100%%\\n'",
            "2",
            "one\n50%\\u000d100%\n",
        ),
        ("printf 'a\\r\\nb\\r\\n'", "2", "a\nb\n"),
    ]
    .into_iter()
    .enumerate()
    {
        let capture = capture_of(dir.path(), &format!("tail-{at}.txt"), script);
        let header = header_line(&capture);

        let output = houserules()
            .args(write_args())
            .arg("--capture")
            .arg(&capture)
            .args(["--tail", tail])
            .output()
            .expect("run bundle write");

        assert_eq!(
            output.status.code(),
            Some(0),
            "{script}: {}",
            stderr(&output)
        );
        assert!(
            stdout(&output).ends_with(&format!("```houserules-run\n{header}\n{lines}```\n")),
            "{script} with --tail {tail}: {}",
            stdout(&output)
        );
    }
}

/// Every capture has its own block, in the order of the flags; the flag takes
/// several values or repeats.
#[test]
fn every_capture_has_its_block_in_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = capture_of(dir.path(), "first.txt", "echo first");
    let second = capture_of(dir.path(), "second.txt", "echo second");
    let (first_header, second_header) = (header_line(&first), header_line(&second));
    let expected = format!(
        "\n```houserules-run\n{first_header}\nfirst\n```\n\n```houserules-run\n{second_header}\nsecond\n```\n"
    );

    let repeated = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&first)
        .arg("--capture")
        .arg(&second)
        .output()
        .expect("run bundle write");
    let listed = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&first)
        .arg(&second)
        .output()
        .expect("run bundle write");

    assert!(
        stdout(&repeated).ends_with(&expected),
        "{}",
        stdout(&repeated)
    );
    assert_eq!(stdout(&listed), stdout(&repeated));
}

/// Several work items join with a comma and a space, in the order of the
/// flags; the flag takes several values or repeats.
#[test]
fn work_items_join_in_order() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "run.txt", "echo ok");
    let run = |extra: &[&str]| {
        let output = houserules()
            .args([
                "bundle",
                "write",
                "--purpose",
                "p",
                "--tier",
                "standard",
                "--criterion",
                "c",
            ])
            .args(extra)
            .arg("--capture")
            .arg(&capture)
            .output()
            .expect("run bundle write");
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        stdout(&output)
    };

    let repeated = run(&["--work-item", "HR-2", "--work-item", "HR-1"]);
    let listed = run(&["--work-item", "HR-2", "HR-1"]);

    assert!(
        repeated.contains("\nWork items: HR-2, HR-1\n"),
        "{repeated}"
    );
    assert_eq!(listed, repeated);
}

/// `--budget-reason` adds its line after the criterion.
#[test]
fn a_budget_reason_adds_its_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "run.txt", "echo ok");

    let output = houserules()
        .args(write_args())
        .args(["--budget-reason", "A mechanical change."])
        .arg("--capture")
        .arg(&capture)
        .output()
        .expect("run bundle write");

    assert!(
        stdout(&output).contains(
            "Criterion: A small change.\nBudget reason: A mechanical change.\n\n```houserules-run\n"
        ),
        "{}",
        stdout(&output)
    );
}

/// A fence is one backtick longer than the longest run of backticks in its
/// block, so an output with backticks never closes it; the heading and the
/// labels in an output are inside the fence.
#[test]
fn the_fence_is_longer_than_every_run_of_backticks_in_the_block() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(
        dir.path(),
        "ticks.txt",
        "printf '%s\\n' '```' '````' '## Evidence bundle' 'Purpose: no'",
    );
    let header = header_line(&capture);

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .output()
        .expect("run bundle write");

    assert!(
        stdout(&output).ends_with(&format!(
            "`````houserules-run\n{header}\n```\n````\n## Evidence bundle\nPurpose: no\n`````\n"
        )),
        "{}",
        stdout(&output)
    );
}

/// Spec 7.1: the first line of a block is the header line of the capture file,
/// byte for byte. A command argument with the delete character or a C1 control
/// character stays in the header as `capture` wrote it (JSON escapes the
/// characters below U+0020 only), and a control character in the output is
/// written as a `\u` escape.
#[test]
fn the_header_line_is_copied_byte_for_byte() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path().join("controls.txt");
    capture_in(
        dir.path(),
        &out,
        &["sh", "-c", "printf 'red\\033[31m\\n'", "x\u{7f}y\u{85}z"],
    );
    let header = header_line(&out);
    assert!(header.contains('\u{7f}'), "the capture keeps the character");

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&out)
        .output()
        .expect("run bundle write");

    assert!(
        stdout(&output).ends_with(&format!(
            "```houserules-run\n{header}\nred\\u001b[31m\n```\n"
        )),
        "{}",
        stdout(&output)
    );
}

/// An output with CRLF line ends is copied with LF line ends.
#[test]
fn a_crlf_output_is_copied_with_lf_line_ends() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "crlf.txt", "printf 'one\\r\\ntwo\\r\\n'");
    let header = header_line(&capture);

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .output()
        .expect("run bundle write");

    assert!(
        stdout(&output).ends_with(&format!("```houserules-run\n{header}\none\ntwo\n```\n")),
        "{}",
        stdout(&output)
    );
}

/// A capture that wrote nothing has the header alone.
#[test]
fn a_capture_with_no_output_has_the_header_alone() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "quiet.txt", "true");
    let header = header_line(&capture);

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .output()
        .expect("run bundle write");

    assert!(
        stdout(&output).ends_with(&format!("```houserules-run\n{header}\n```\n")),
        "{}",
        stdout(&output)
    );
}

/// `--out` writes the section to the file and prints nothing.
#[test]
fn out_writes_the_section_to_a_file() {
    let (dir, capture) = plain_with_capture();
    let target = dir.path().join("bundle.md");
    let printed = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .output()
        .expect("run bundle write");

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .arg("--out")
        .arg(&target)
        .output()
        .expect("run bundle write");

    assert_output(&output, 0, "", "");
    assert_eq!(
        fs::read_to_string(&target).expect("read the section"),
        stdout(&printed)
    );
}

/// The same inputs give the same bytes twice.
#[test]
fn bundle_write_gives_the_same_bytes_twice() {
    let (_dir, capture) = plain_with_capture();
    let run = || {
        houserules()
            .args(write_args())
            .arg("--capture")
            .arg(&capture)
            .output()
            .expect("run bundle write")
    };

    let (first, second) = (run(), run());

    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.status.code(), Some(0));
}

/// Case 3: an intent whose adapter gives a why stands in for the purpose, so
/// the section has an `Intent:` line and no `Purpose:` line; with a purpose it
/// has both, and the intent prints as `<source>:<ref>`.
#[test]
fn an_intent_with_a_why_replaces_the_purpose_line() {
    let request = Request::new(Some(budgets()), true);
    let capture = request.install.scratch_file("run.txt");
    let run = |purpose: &[&str]| {
        let mut command = houserules();
        command
            .args(["bundle", "write"])
            .args(purpose)
            .args([
                "--intent",
                "file: docs/plans/with-why.md",
                "--work-item",
                "HR-901",
                "--tier",
                "standard",
                "--criterion",
                "A small change.",
                "--capture",
            ])
            .arg(&capture)
            .current_dir(request.install.top.path());
        command.output().expect("run bundle write")
    };

    let alone = run(&[]);
    let beside = run(&["--purpose", "Add five lines."]);

    assert_eq!(alone.status.code(), Some(0), "{}", stderr(&alone));
    assert!(
        stdout(&alone).starts_with(
            "## Evidence bundle\n\nIntent: file:docs/plans/with-why.md\nWork items: HR-901\n"
        ),
        "{}",
        stdout(&alone)
    );
    assert!(!stdout(&alone).contains("Purpose:"), "{}", stdout(&alone));
    assert!(
        stdout(&beside).starts_with(
            "## Evidence bundle\n\nPurpose: Add five lines.\nIntent: file:docs/plans/with-why.md\nWork items: HR-901\n"
        ),
        "{}",
        stdout(&beside)
    );
    let checked = request.check(&stdout(&alone), &[]);
    assert_output(&checked, 0, &request.ok_line(), "");
}

// ---- bundle write: setup errors -------------------------------------------------------

/// Runs `bundle write` with `args` and the working directory `cwd`.
fn write_in(cwd: &Path, args: &[&str]) -> Output {
    houserules()
        .args(["bundle", "write"])
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("run bundle write")
}

/// Case 3: a capture that does not parse exits 2 with the reason of the reader
/// of `capture`; so does one that does not exist or cannot be read.
#[test]
fn a_capture_that_does_not_parse_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let text = dir.path().join("text.txt");
    fs::write(&text, "hello\n").expect("write the file");
    let empty = dir.path().join("empty.txt");
    fs::write(&empty, "").expect("write the file");
    let missing = dir.path().join("missing.txt");
    let folder = dir.path().join("folder");
    fs::create_dir(&folder).expect("create the folder");
    let unreadable = fs::read(&folder)
        .expect_err("a folder is no file")
        .to_string();
    let base = write_args()[2..].to_vec();

    for (capture, line) in [
        (
            &text,
            format!(
                "bundle write: {} is no capture (its first line is no JSON object)\n",
                text.display()
            ),
        ),
        (
            &empty,
            format!(
                "bundle write: {} is no capture (it is empty)\n",
                empty.display()
            ),
        ),
        (
            &missing,
            format!("bundle write: {} does not exist\n", missing.display()),
        ),
        (
            &folder,
            format!(
                "bundle write: {} could not be read ({unreadable})\n",
                folder.display()
            ),
        ),
    ] {
        let output = houserules()
            .args(["bundle", "write"])
            .args(&base)
            .arg("--capture")
            .arg(capture)
            .output()
            .expect("run bundle write");

        assert_output(&output, 2, "", &line);
    }
}

/// Every capture that does not parse has its line.
#[test]
fn every_capture_that_does_not_parse_has_its_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let good = capture_of(dir.path(), "good.txt", "echo ok");
    let first = dir.path().join("first.txt");
    fs::write(&first, "one\n").expect("write the file");
    let second = dir.path().join("second.txt");
    fs::write(&second, "two\n").expect("write the file");

    let output = houserules()
        .args(["bundle", "write"])
        .args(&write_args()[2..])
        .arg("--capture")
        .arg(&first)
        .arg(&good)
        .arg(&second)
        .output()
        .expect("run bundle write");

    assert_output(
        &output,
        2,
        "",
        &format!(
            "bundle write: {} is no capture (its first line is no JSON object)\nbundle write: {} is no capture (its first line is no JSON object)\n",
            first.display(),
            second.display()
        ),
    );
}

/// Case 3: neither `--purpose` nor an `--intent` whose adapter gives a why exits
/// 2: no intent, a blank purpose, and an intent with no why.
#[test]
fn neither_a_purpose_nor_an_intent_with_a_why_exits_2() {
    let request = Request::new(Some(budgets()), true);
    let capture = request.install.scratch_file("run.txt");
    let capture = capture.to_str().expect("a UTF-8 path");
    let top = request.install.top.path();
    let line = "bundle write: --purpose is needed unless --intent names an intent whose adapter gives a why\n";
    for extra in [
        vec![],
        vec!["--purpose", "  "],
        vec!["--intent", "file:docs/plans/no-why.md"],
        vec!["--intent", "work-item:HR-901"],
    ] {
        let mut args = vec![
            "--work-item",
            "HR-901",
            "--tier",
            "standard",
            "--criterion",
            "A small change.",
        ];
        args.extend_from_slice(&extra);
        args.extend_from_slice(&["--capture", capture]);

        let output = write_in(top, &args);

        assert_output(&output, 2, "", line);
    }
}

/// An `--intent` that does not resolve exits 2 with the line of its adapter,
/// even beside a purpose; so does one that is no `<source>:<ref>`.
#[test]
fn an_intent_that_does_not_resolve_exits_2() {
    let request = Request::new(Some(budgets()), true);
    let capture = request.install.scratch_file("run.txt");
    let capture = capture.to_str().expect("a UTF-8 path");
    for (intent, line) in [
        (
            "file:docs/plans/missing.md",
            format!("bundle write: --intent file:docs/plans/missing.md: {NO_REGULAR_FILE}\n"),
        ),
        (
            "work-item:HR-9999",
            format!("bundle write: --intent work-item:HR-9999: {NO_SUCH_WORK_ITEM}\n"),
        ),
        (
            "openspec:x",
            "bundle write: --intent openspec:x: \"openspec\" is no intent source; use one of: work-item, file\n".to_string(),
        ),
        (
            "plan",
            "bundle write: --intent plan: an intent is written <source>:<ref>\n".to_string(),
        ),
    ] {
        let output = write_in(
            request.install.top.path(),
            &[
                "--purpose",
                "Add five lines.",
                "--intent",
                intent,
                "--work-item",
                "HR-901",
                "--tier",
                "standard",
                "--criterion",
                "A small change.",
                "--capture",
                capture,
            ],
        );

        assert_output(&output, 2, "", &line);
    }
}

/// Case 3: an empty tier or criterion exits 2.
#[test]
fn an_empty_tier_or_criterion_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "run.txt", "echo ok");
    let capture = capture.to_str().expect("a UTF-8 path");
    for (tier, criterion, line) in [
        ("", "A small change.", "bundle write: --tier is empty\n"),
        (" ", "A small change.", "bundle write: --tier is empty\n"),
        ("standard", "", "bundle write: --criterion is empty\n"),
    ] {
        let output = write_in(
            dir.path(),
            &[
                "--purpose",
                "p",
                "--work-item",
                "HR-1",
                "--tier",
                tier,
                "--criterion",
                criterion,
                "--capture",
                capture,
            ],
        );

        assert_output(&output, 2, "", line);
    }
}

/// Case 3: no `--work-item` and no `--capture` exit 2.
#[test]
fn no_work_item_and_no_capture_exit_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "run.txt", "echo ok");
    let capture = capture.to_str().expect("a UTF-8 path");
    let cases: [(&[&str], &str); 2] = [
        (
            &[
                "--purpose",
                "p",
                "--tier",
                "t",
                "--criterion",
                "c",
                "--capture",
                capture,
            ],
            "--work-item",
        ),
        (
            &[
                "--purpose",
                "p",
                "--tier",
                "t",
                "--criterion",
                "c",
                "--work-item",
                "HR-1",
            ],
            "--capture",
        ),
    ];
    for (args, missing) in cases {
        let output = write_in(dir.path(), args);

        assert_eq!(output.status.code(), Some(2), "{missing}");
        assert_eq!(stdout(&output), "");
        assert!(stderr(&output).contains(missing), "{}", stderr(&output));
    }
}

/// A work-item id that is empty, holds a comma, or holds a line break would
/// split or break the `Work items:` line, so it exits 2.
#[test]
fn a_work_item_that_would_break_the_line_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "run.txt", "echo ok");
    let capture = capture.to_str().expect("a UTF-8 path");
    for id in ["", "HR-1, HR-2", "HR-1\nHR-2"] {
        let output = write_in(
            dir.path(),
            &[
                "--purpose",
                "p",
                "--tier",
                "t",
                "--criterion",
                "c",
                "--work-item",
                id,
                "--capture",
                capture,
            ],
        );

        assert_output(
            &output,
            2,
            "",
            "bundle write: --work-item holds an empty id, a comma, or a line break\n",
        );
    }
}

/// A value that holds a line break would break the one-line parts of the
/// section, so it exits 2 with the flag named and writes no section. The
/// `--intent` is one of them: its line break would write a second line of the
/// section, `Risk tier: light` among them. The command runs in an install,
/// because an `--intent` makes it read the install.
#[test]
fn a_value_with_a_line_break_exits_2() {
    let install = Install::new("", None, false);
    let capture = install.capture("run.txt", &["sh", "-c", "echo ok"]);
    let capture = capture.to_str().expect("a UTF-8 path");
    let section = install.scratch_file("section.md");
    let section = section.to_str().expect("a UTF-8 path");
    for flag in [
        "--purpose",
        "--intent",
        "--tier",
        "--criterion",
        "--budget-reason",
    ] {
        let mut args = vec![
            "--purpose",
            "p",
            "--intent",
            "work-item:HR-1",
            "--tier",
            "t",
            "--criterion",
            "c",
            "--budget-reason",
            "r",
            "--work-item",
            "HR-1",
            "--capture",
            capture,
            "--out",
            section,
        ];
        let at = args.iter().position(|arg| *arg == flag).expect("a flag");
        args[at + 1] = "work-item:HR-1\nRisk tier: light";

        let output = write_in(install.top.path(), &args);

        assert_output(
            &output,
            2,
            "",
            &format!("bundle write: {flag} must be one line\n"),
        );
        assert!(
            !Path::new(section).exists(),
            "{flag}: a section was written"
        );
    }
}

/// Case 3: an unwritable `--out` exits 2 with the error of the operating
/// system, and nothing prints.
#[test]
fn an_unwritable_out_exits_2() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "run.txt", "echo ok");
    let target = dir.path().join("no-such-folder").join("bundle.md");
    let error = unwritable(&target);

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .arg("--out")
        .arg(&target)
        .output()
        .expect("run bundle write");

    assert_output(
        &output,
        2,
        "",
        &format!("bundle write: {}: {error}\n", target.display()),
    );
}

/// A named intent needs the tree at `HEAD` and the tracker, and no change
/// analysis: in an install whose `HEAD` holds no rule source (`knowledge/` and
/// `.houserules.json` are not committed yet), `bundle write --intent` prints
/// the section and exits 0.
#[test]
fn a_named_intent_resolves_where_head_holds_no_rule_source() {
    let top = scratch_git_repo();
    fs::write(
        top.path().join(".houserules.json"),
        json!({"schema": "houserules/install@2.3.0", "idPrefix": "WI"}).to_string(),
    )
    .expect("write the marker");
    fs::create_dir(top.path().join("knowledge")).expect("create knowledge/");
    fs::write(top.path().join("knowledge").join("schema.json"), "{}\n").expect("write schema");
    fs::write(
        top.path().join("knowledge").join("areas.json"),
        areas().to_string(),
    )
    .expect("write the areas");
    fs::create_dir(top.path().join("docs")).expect("create docs/");
    fs::write(
        top.path().join("docs").join("plan.md"),
        "# A plan\n\nBecause the change needs it.\n",
    )
    .expect("write the plan");
    git(top.path(), &["add", "docs/plan.md"]);
    git(
        top.path(),
        &["commit", "-q", "--no-verify", "-m", "docs: plan"],
    );
    let capture = capture_of(top.path(), "run.txt", "echo ok");

    let output = write_in(
        top.path(),
        &[
            "--intent",
            "file:docs/plan.md",
            "--work-item",
            "X",
            "--tier",
            "light",
            "--criterion",
            "c",
            "--capture",
            capture.to_str().expect("a UTF-8 path"),
        ],
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output)
            .starts_with("## Evidence bundle\n\nIntent: file:docs/plan.md\nWork items: X\n"),
        "{}",
        stdout(&output)
    );
}

/// An `--intent` makes `bundle write` read the install of the current
/// directory, so D4 refuses a 1.x install for it, as for every command that
/// reads the install; without an `--intent` it reads none, and runs there too.
#[test]
fn bundle_write_with_an_intent_refuses_a_1_x_install() {
    let top = scratch_git_repo();
    fs::write(
        top.path().join(".houserules.json"),
        "{\"idPrefix\": \"WI\"}\n",
    )
    .expect("write the marker");
    let capture = capture_of(top.path(), "run.txt", "echo ok");
    let with_intent = [
        "--intent",
        "file:docs/plans/with-why.md",
        "--work-item",
        "HR-1",
        "--tier",
        "t",
        "--criterion",
        "c",
    ];

    let refused = houserules()
        .args(["bundle", "write"])
        .args(with_intent)
        .arg("--capture")
        .arg(&capture)
        .current_dir(top.path())
        .output()
        .expect("run bundle write");
    let alone = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .current_dir(top.path())
        .output()
        .expect("run bundle write");

    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(stdout(&refused), "");
    assert!(
        stderr(&refused).ends_with(
            ": this is an install of houserules 1.x; run houserules update to convert it\n"
        ),
        "{}",
        stderr(&refused)
    );
    assert_eq!(alone.status.code(), Some(0), "{}", stderr(&alone));
}

/// Spec 7.2: `bundle write` takes no `--dir`.
#[test]
fn bundle_write_takes_no_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let capture = capture_of(dir.path(), "run.txt", "echo ok");

    let output = houserules()
        .args(write_args())
        .arg("--capture")
        .arg(&capture)
        .arg("--dir")
        .arg(dir.path())
        .output()
        .expect("run bundle write");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("--dir"), "{}", stderr(&output));
}

/// `bundle` alone is a usage error that lists both subcommands.
#[test]
fn bundle_alone_is_a_usage_error() {
    let output = houserules().arg("bundle").output().expect("run bundle");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("write"), "{}", stderr(&output));
    assert!(stderr(&output).contains("check"), "{}", stderr(&output));
}

// ---- the help -------------------------------------------------------------------------

/// The sentences of the `about` of `bundle`.
const ABOUT: [&str; 4] = [
    "Writes and checks the evidence bundle of a pull or merge request.",
    "The bundle is a section of the description of the request: the purpose or an intent, the work items, the risk tier with its criterion, and each verification run as its capture header with the tail of its output.",
    "`bundle write` writes the section from captures.",
    "`bundle check` reads a description and fails a missing or wrong part",
];

/// The sentences of the `about` of `bundle write`.
const WRITE_ABOUT: [&str; 8] = [
    "Writes the evidence bundle section for the description of a pull or merge request, on stdout or to `--out`.",
    "Each `--capture` is a file that `houserules capture` wrote: the section copies its header line whole and the last `--tail` lines of its output, so no header passes through a model.",
    "`--purpose` is needed unless `--intent <source>:<ref>` names an intent whose adapter gives a why; the why is then the purpose.",
    "The command reads an `--intent` in the install of the current directory, at `HEAD`.",
    "`--budget-reason` adds the line that a diff above the budget of the tier needs.",
    "The command takes no `--dir`.",
    "The same inputs give the same bytes.",
    "It exits 2 for a capture that does not parse, an `--intent` that does not resolve, neither `--purpose` nor an `--intent` with a why, an empty `--tier` or `--criterion`, a `--purpose`, `--intent`, `--tier`, `--criterion`, or `--budget-reason` that holds a line break, a `--work-item` that is empty or holds a comma or a line break, or an `--out` file that cannot be written",
];

/// The sentences of the `about` of `bundle check`.
const CHECK_ABOUT: [&str; 8] = [
    "Checks the evidence bundle in the description of a pull or merge request, read from `--body`, against the range `--base` to `--head`.",
    "It runs no audit.",
    "It prints one line for each finding, each line starts with `bundle check: `, and it exits 1 when it prints a finding: no section or more than one, a missing Purpose, Work items, Risk tier, or Criterion line, an intent or a work item that does not resolve, no run block, a run that has no capture header, that ran at another commit, in a dirty work tree, or with a failed exit, and a diff above the budget of the tier with no Budget reason line.",
    "A work item that cannot be resolved, because the install has no tracker or because its tracker cannot be reached, is counted in one line and fails nothing.",
    "An `--author` in `review.bundle.exemptAuthors`, or a `--head-ref` that matches a glob of `review.bundle.exemptBranches`, exempts the request.",
    "A clean bundle prints `bundle check: ok (<n> runs at <head7>, diff <lines> lines)`.",
    "With no `review.bundle.budgets` it checks no size.",
    "It exits 2 for a setup error, for example: a `--body` file that cannot be read, a ref that is no commit, a `review` key with a fault, or a git older than 2.41",
];

/// The help text of each argument of `bundle write`.
const WRITE_ARGUMENTS: [&str; 9] = [
    "One or two sentences on why the change exists. Needed unless `--intent` names an intent whose adapter gives a why",
    "The intent to name, written `<source>:<ref>`. The source is `work-item` or `file`. The command reads it in the install of the current directory, at `HEAD`",
    "A work-item id; repeat the flag or give several values",
    "The risk tier of the change",
    "The criterion that selected the tier",
    "Why the diff is above the budget of the tier; add it only then",
    "A capture file that `houserules capture` wrote; repeat the flag or give several files",
    "The number of lines from the end of each output to copy; defaults to 20",
    "Writes the section to this file in place of stdout",
];

/// The help text of each argument of `bundle check`.
const CHECK_ARGUMENTS: [&str; 6] = [
    "The file that holds the description of the pull or merge request",
    "The base ref of the range. The range runs from the merge base of `--base` and `--head` to `--head`",
    "The head ref of the range. Each run of the bundle must have run at this commit",
    "The login of the author of the request; an author in `review.bundle.exemptAuthors` needs no bundle",
    "The source branch of the request; a branch that matches a glob of `review.bundle.exemptBranches` needs no bundle",
    "Install root to read from; defaults to the nearest directory at or above the current one that holds `.houserules.json`, no higher than the git top level, else the git top level",
];

/// The three `--help` texts state the bundle, the exits, and each condition of
/// an argument (`quality.user-facing-text-is-contract`), and `houserules
/// --help` lists the same about text.
#[test]
fn the_help_of_bundle_states_the_section_the_checks_and_the_exits() {
    states("bundle --help", &help_of("bundle"), &ABOUT);
    states("houserules --help", &help_of_args(&["--help"]), &ABOUT);
    states(
        "bundle write --help",
        &help_of_args(&["bundle", "write", "--help"]),
        &WRITE_ABOUT,
    );
    states(
        "bundle write --help",
        &help_of_args(&["bundle", "write", "--help"]),
        &WRITE_ARGUMENTS,
    );
    states(
        "bundle check --help",
        &help_of_args(&["bundle", "check", "--help"]),
        &CHECK_ABOUT,
    );
    states(
        "bundle check --help",
        &help_of_args(&["bundle", "check", "--help"]),
        &CHECK_ARGUMENTS,
    );
}
