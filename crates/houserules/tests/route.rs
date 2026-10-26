//! `houserules route` (batch 37, spec 6.2 to 6.4; acceptance case 2, the
//! command part; review focus 5; HR-186). Each case runs the compiled binary
//! against a scratch repository whose install holds the areas `docs`, `code`,
//! and `ops`, and reads the printed lines whole. A line of the spec is the
//! literal of a test, and the lines print in the order of the spec's sorting
//! rule (`route: <route>` first, then the lines of the reasons at that route).
//!
//! Every fixture is hermetic: the install is written by hand, so the audit that
//! `route` runs in process reads only the entries that a case lists.

mod help_support;
mod schema_support;
mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use help_support::{help_of, help_of_args, omits, states};
use schema_support::assert_valid;
use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    assert_output, commit_all, git, houserules, read_json, scratch_git_repo, stderr, stdout,
};

/// The schema file of the route decision, relative to the repository root.
const ROUTE_SCHEMA: &str = "schemas/route/1.0.0.json";

/// The first line of a file that `houserules ci write` created (spec 10.3).
const CI_WRITTEN_BY: &str = "# Written by houserules ci write; rewrite it with houserules ci write --force and the flags that wrote it.";

/// The line that `route` prints for `--score` when no threshold is configured
/// (spec 6.2, rule 7).
const SCORE_NEEDS_A_THRESHOLD: &str = "route: --score needs review.route.ownerScore or seniorScore";

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

/// A knowledge entry of the process `global` area whose check is `check`. The
/// entry is standing, so every audit has it in its package.
fn entry(id: &str, check: Value) -> Value {
    json!({
        "id": id, "kind": "rule", "area": "global", "standing": true,
        "summary": "A rule.", "body": ["Body."], "tags": [],
        "source": {"date": "2026-10-24", "by": "test"},
        "check": check,
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

/// The `review` key that makes the area `docs` `auto-merge-eligible` under the
/// two limits.
fn docs_auto_merge(max_diff_lines: u64, max_uncovered_share: f64) -> Value {
    json!({"route": {
        "areas": {"docs": {
            "route": "auto-merge-eligible",
            "maxDiffLines": max_diff_lines,
            "maxUncoveredShare": max_uncovered_share,
        }},
        "sampleRate": 0.1,
    }})
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

    /// Writes `content` to `path` below `dir`, with its folders.
    fn write_in(dir: &Path, path: &str, content: &str) {
        let target = file(dir, path);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write the file");
    }

    /// Writes `content` to `path` below the install root.
    fn write(&self, path: &str, content: &str) {
        Self::write_in(&self.root(), path, content);
    }

    /// Writes `content` to `path` below the git top level.
    fn write_top(&self, path: &str, content: &str) {
        Self::write_in(self.top.path(), path, content);
    }

    /// Adds `count` lines to the end of `path` below the install root.
    fn append(&self, path: &str, count: usize) {
        let target = file(&self.root(), path);
        let mut text = fs::read_to_string(&target).unwrap_or_default();
        text.push_str(&numbered("added", count));
        Self::write_in(&self.root(), path, &text);
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

    /// Runs `houserules route <args> --dir <root>`.
    fn route(&self, args: &[&str]) -> Output {
        houserules()
            .arg("route")
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run route")
    }

    /// The lines that `route <args>` prints. The run exits 0 and prints
    /// nothing on stderr.
    fn route_lines(&self, args: &[&str]) -> String {
        let output = self.route(args);
        assert_output(&output, 0, &stdout(&output), "");
        stdout(&output)
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

// ---- the routes and their lines --------------------------------------------------------

/// Unconfigured, every change is `owner-review` (spec 6.2; C3).
#[test]
fn an_unconfigured_change_is_owner_review() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.commit("docs: edit the guide");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: owner-review\narea `docs`: owner-review\n",
        "",
    );
}

/// A docs-only change in an area that is `auto-merge-eligible`, read by a
/// covering check, and within `maxDiffLines` is `auto-merge-eligible` (issue 66).
#[test]
fn a_docs_only_change_within_the_limits_is_auto_merge_eligible() {
    let install = Install::new("", &[docs_read()], Some(docs_auto_merge(5, 0.0)));
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.commit("docs: edit the guide");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: auto-merge-eligible\narea `docs`: auto-merge-eligible\n",
        "",
    );
}

/// Past `maxDiffLines` the area gives `owner-review`, and the line names the
/// size and the limit.
#[test]
fn a_diff_over_max_diff_lines_is_owner_review() {
    let install = Install::new("", &[docs_read()], Some(docs_auto_merge(5, 0.0)));
    let base = install.head();
    install.append("docs/guide.md", 6);
    install.commit("docs: edit the guide");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: owner-review\narea `docs`: diff 6 lines > maxDiffLines 5 -> owner-review\n",
        "",
    );
}

/// With one uncovered line in four and `maxUncoveredShare: 0`, the area gives
/// `owner-review`; the share prints with two decimals.
#[test]
fn an_uncovered_line_with_a_zero_share_limit_is_owner_review() {
    let install = Install::new("", &[docs_read()], Some(docs_auto_merge(5, 0.0)));
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("docs/data.csv", "a,b\n");
    install.commit("docs: edit the guide, add data");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: owner-review\n\
         area `docs`: uncovered share 0.25 > maxUncoveredShare 0.00 -> owner-review\n",
        "",
    );
}

/// Both limits fail at once: the two lines print, sorted.
#[test]
fn both_limit_lines_print_sorted() {
    let install = Install::new("", &[docs_read()], Some(docs_auto_merge(5, 0.0)));
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("docs/data.csv", &numbered("row", 3));
    install.commit("docs: edit the guide, add data");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: owner-review\n\
         area `docs`: diff 6 lines > maxDiffLines 5 -> owner-review\n\
         area `docs`: uncovered share 0.50 > maxUncoveredShare 0.00 -> owner-review\n",
        "",
    );
}

/// In a limit line the share rounds up and the limit rounds down at two
/// decimals, so the printed share is greater than the printed limit (spec 6.3):
/// one uncovered line in 300 against a limit of 0 is `0.01 > 0.00`, and a share
/// of 0.3 against a limit of 0.256 is `0.30 > 0.25`.
#[test]
fn a_limit_line_prints_the_share_rounded_up_and_the_limit_rounded_down() {
    let tiny = Install::new("", &[docs_read()], Some(docs_auto_merge(1000, 0.0)));
    let tiny_base = tiny.head();
    tiny.append("docs/guide.md", 299);
    tiny.write("docs/data.csv", "a,b\n");
    tiny.commit("docs: edit the guide, add data");
    let tenths = Install::new("", &[docs_read()], Some(docs_auto_merge(1000, 0.256)));
    let tenths_base = tenths.head();
    tenths.append("docs/guide.md", 7);
    tenths.write("docs/data.csv", &numbered("row", 3));
    tenths.commit("docs: edit the guide, add data");

    let one_in_300 = tiny.route(&["--base", &tiny_base]);
    let three_in_ten = tenths.route(&["--base", &tenths_base]);

    assert_output(
        &one_in_300,
        0,
        "route: owner-review\n\
         area `docs`: uncovered share 0.01 > maxUncoveredShare 0.00 -> owner-review\n",
        "",
    );
    assert_output(
        &three_in_ten,
        0,
        "route: owner-review\n\
         area `docs`: uncovered share 0.30 > maxUncoveredShare 0.25 -> owner-review\n",
        "",
    );
}

/// The install whose area `ops` is `senior-review`, and its base commit.
fn senior_ops_install() -> (Install, String) {
    let review = json!({"route": {"areas": {"ops": {"route": "senior-review"}}}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    (install, base)
}

/// A change in an area configured `senior-review` is `senior-review`.
#[test]
fn a_change_in_a_senior_review_area_is_senior_review() {
    let (install, base) = senior_ops_install();
    install.append("ops/deploy.sh", 1);
    install.commit("ops: edit the script");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: senior-review\narea `ops`: senior-review\n",
        "",
    );
}

/// A deletion-only change in that area is `senior-review` too: a deleted path
/// counts by its old path (spec 4.2).
#[test]
fn a_deletion_only_change_in_a_senior_review_area_is_senior_review() {
    let (install, base) = senior_ops_install();
    fs::remove_file(file(&install.root(), "ops/deploy.sh")).expect("remove the script");
    install.commit("ops: drop the script");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: senior-review\narea `ops`: senior-review\n",
        "",
    );
}

/// A docs change plus a file in no area takes `review.route.default`, and the
/// default is `owner-review` while the key is not set.
#[test]
fn a_file_in_no_area_takes_the_default_route() {
    let mut review = docs_auto_merge(50, 1.0);
    let install = Install::new("", &[docs_read()], Some(review.clone()));
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.write("misc.txt", "stray\n");
    install.commit("chore: edit the guide, add a stray file");
    let unset = install.route(&["--base", &base]);

    review["route"]["default"] = json!("senior-review");
    let set = Install::new("", &[docs_read()], Some(review));
    let set_base = set.head();
    set.append("docs/guide.md", 1);
    set.write("misc.txt", "stray\n");
    set.commit("chore: edit the guide, add a stray file");
    let configured = set.route(&["--base", &set_base]);

    assert_output(
        &unset,
        0,
        "route: owner-review\nno area: `misc.txt` -> owner-review\n",
        "",
    );
    assert_output(
        &configured,
        0,
        "route: senior-review\nno area: `misc.txt` -> senior-review\n",
        "",
    );
}

/// A changed rule source is `senior-review` whatever the configuration (spec
/// 6.2, rule 1): the knowledge folder, the marker, a file of `review.ci.files`,
/// and a file that `ci write` created.
#[test]
fn a_changed_rule_source_is_senior_review_whatever_the_configuration() {
    let review = json!({"route": {
        "areas": {"docs": {
            "route": "auto-merge-eligible", "maxDiffLines": 1000, "maxUncoveredShare": 1.0,
        }},
        "paths": [{
            "glob": "**", "route": "auto-merge-eligible",
            "maxDiffLines": 1000, "maxUncoveredShare": 1.0,
        }],
        "sampleRate": 0.5,
    }, "ci": {"files": [".github/workflows/ci.yml"]}});
    let install = Install::new("", &[], Some(review));
    install.write_top(".github/workflows/ci.yml", "name: ci\n");
    install.commit("ci: add the workflow");

    let mut marker = read_json(&install.root(), ".houserules.json");
    marker["owner"] = json!("team");
    let cases: Vec<(&str, String)> = vec![
        (
            "knowledge/team.json",
            "{\"entries\": [], \"title\": \"Team rules\"}\n".to_string(),
        ),
        (
            ".houserules.json",
            serde_json::to_string_pretty(&marker).expect("serialize the marker"),
        ),
        (".github/workflows/ci.yml", "name: ci changed\n".to_string()),
        (
            ".github/workflows/review.yml",
            format!("{CI_WRITTEN_BY}\nname: review\n"),
        ),
    ];
    for (path, content) in cases {
        let base = install.head();
        install.write(path, &content);
        install.commit(&format!("chore: change {path}"));

        let output = install.route(&["--base", &base]);

        assert_output(
            &output,
            0,
            &format!("route: senior-review\nrule sources: `{path}` changed -> senior-review\n"),
            "",
        );
    }
}

/// The same in a subdirectory install: the knowledge folder prints from the
/// install root, a CI file from the git top level.
#[test]
fn a_changed_rule_source_of_a_subdirectory_install_is_senior_review() {
    let review = json!({"ci": {"files": [".github/workflows/ci.yml"]}});
    let install = Install::new("sub", &[], Some(review));
    install.write_top(".github/workflows/ci.yml", "name: ci\n");
    install.commit("ci: add the workflow");
    let base = install.head();
    install.write_top(".github/workflows/ci.yml", "name: ci changed\n");
    install.commit("ci: change the workflow");
    let ci = install.route(&["--base", &base]);
    let after_ci = install.head();
    install.write("knowledge/team.json", "{\"entries\": [], \"note\": 1}\n");
    install.commit("docs: change the knowledge");
    let knowledge = install.route(&["--base", &after_ci]);
    let after_knowledge = install.head();
    install.write_top(
        ".github/workflows/review.yml",
        &format!("{CI_WRITTEN_BY}\nname: review\n"),
    );
    install.commit("ci: add the file of ci write");
    let written = install.route(&["--base", &after_knowledge]);

    assert_output(
        &ci,
        0,
        "route: senior-review\n\
         rule sources: `.github/workflows/ci.yml` changed -> senior-review\n",
        "",
    );
    assert_output(
        &knowledge,
        0,
        "route: senior-review\nrule sources: `knowledge/team.json` changed -> senior-review\n",
        "",
    );
    assert_output(
        &written,
        0,
        "route: senior-review\n\
         rule sources: `.github/workflows/review.yml` changed -> senior-review\n",
        "",
    );
}

/// A project that sets no route still gets `senior-review` for a change to a
/// rule source, and `owner-review` for every other change: no configuration
/// lowers rule 1 (spec 6.2), and no configuration is needed to raise it.
#[test]
fn a_rule_source_change_of_an_unconfigured_project_is_senior_review() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.write(
        "knowledge/team.json",
        "{\"entries\": [], \"title\": \"Team\"}\n",
    );
    install.commit("docs: change the knowledge");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: senior-review\nrule sources: `knowledge/team.json` changed -> senior-review\n",
        "",
    );
}

/// A change whose only path is a top-level CI file of `review.ci.files` has a
/// changed rule source, so a subdirectory install has a changed rule source
/// and no `default` reason: the `default` rule is for a change with no changed
/// path and no changed rule source (spec 6.2, 6.3).
#[test]
fn a_ci_only_change_of_a_subdirectory_install_takes_no_default_reason() {
    let review = json!({"route": {"default": "senior-review"}, "ci": {"files": [".github/workflows/ci.yml"]}});
    let install = Install::new("sub", &[], Some(review));
    install.write_top(".github/workflows/ci.yml", "name: ci\n");
    install.commit("ci: add the workflow");
    let base = install.head();
    install.write_top(".github/workflows/ci.yml", "name: ci changed\n");
    install.commit("ci: change the workflow");
    let target = install.scratch_file("route.json");

    let lines = install.route_lines(&["--base", &base, "--json", target.to_str().expect("UTF-8")]);

    assert_eq!(
        lines,
        "route: senior-review\n\
         rule sources: `.github/workflows/ci.yml` changed -> senior-review\n"
    );
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    assert!(reasons_of(&document, "default").is_empty());
    assert_eq!(document["inputs"]["areas"], json!({}));
    assert_eq!(document["inputs"]["no_area"], json!([]));
}

/// A warned row gives `review.route.warn`, and the route is `owner-review`
/// while the key is not set.
#[test]
fn a_warned_row_gives_the_warn_route() {
    let warning = entry("team.warn-me", grep("src/*.rs", "WARNME", "warn", false));
    let mut review = json!({"route": {"warn": "senior-review"}});
    let install = Install::new("", std::slice::from_ref(&warning), Some(review.clone()));
    let base = install.head();
    install.write("src/lib.rs", "WARNME\n");
    install.commit("feat: touch the library");
    let warned = install.route(&["--base", &base]);

    review["route"] = json!({});
    let plain = Install::new("", &[warning], Some(review));
    let plain_base = plain.head();
    plain.write("src/lib.rs", "WARNME\n");
    plain.commit("feat: touch the library");
    let default_warn = plain.route(&["--base", &plain_base]);

    assert_output(
        &warned,
        0,
        "route: senior-review\nwarned: `team.warn-me` -> senior-review\n",
        "",
    );
    assert_output(
        &default_warn,
        0,
        "route: owner-review\n\
         area `code`: owner-review\n\
         warned: `team.warn-me` -> owner-review\n",
        "",
    );
}

/// A failed row gives `owner-review`; a fail that `audit --sanctioned` booked
/// gives nothing (spec 6.2, rule 2).
#[test]
fn a_failed_row_gives_owner_review_unless_it_is_sanctioned() {
    let failing = entry(
        "team.no-bad-word",
        grep("docs/*.md", "BADWORD", "fail", false),
    );
    let install = Install::new("", &[failing], Some(docs_auto_merge(50, 1.0)));
    let base = install.head();
    install.write("docs/guide.md", "BADWORD\n");
    install.commit("docs: edit the guide");
    let failed = install.route(&["--base", &base]);
    let audit = install.audit_file(&base, &["team.no-bad-word=HR-1"]);
    let booked = install.route(&["--base", &base, "--audit", audit.to_str().expect("UTF-8")]);

    assert_output(
        &failed,
        0,
        "route: owner-review\nfailed: `team.no-bad-word` -> owner-review\n",
        "",
    );
    assert_output(
        &booked,
        0,
        "route: auto-merge-eligible\narea `docs`: auto-merge-eligible\n",
        "",
    );
}

/// A path entry gives its route for a changed path that its glob matches, old
/// and new paths alike.
#[test]
fn a_path_entry_gives_its_route_for_an_old_and_a_new_path() {
    let review = json!({"route": {"paths": [{"glob": "src/**", "route": "senior-review"}]}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    install.append("src/lib.rs", 1);
    install.commit("feat: edit the library");
    let edited = install.route(&["--base", &base]);
    let before_move = install.head();
    git(
        install.top.path(),
        &["mv", "src/lib.rs", "docs/library-notes.md"],
    );
    install.commit("docs: move the library");
    let moved = install.route(&["--base", &before_move]);

    assert_output(
        &edited,
        0,
        "route: senior-review\npath `src/**`: `src/lib.rs` -> senior-review\n",
        "",
    );
    assert_output(
        &moved,
        0,
        "route: senior-review\npath `src/**`: `src/lib.rs` -> senior-review\n",
        "",
    );
}

/// A path entry at `auto-merge-eligible` keeps its route only within its own
/// limits, like an area.
#[test]
fn a_path_entry_at_auto_merge_eligible_keeps_it_only_within_its_limits() {
    let review = json!({"route": {
        "paths": [{
            "glob": "docs/*.md", "route": "auto-merge-eligible",
            "maxDiffLines": 5, "maxUncoveredShare": 0.5,
        }],
        "areas": {"docs": {"route": "auto-merge-eligible", "maxDiffLines": 100, "maxUncoveredShare": 1.0}},
        "sampleRate": 0.1,
    }});
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 6);
    install.commit("docs: edit the guide");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: owner-review\n\
         path `docs/*.md`: diff 6 lines > maxDiffLines 5 -> owner-review\n",
        "",
    );
}

/// The share limit of a path entry works like the share limit of an area.
#[test]
fn a_path_entry_at_auto_merge_eligible_keeps_it_only_within_its_share_limit() {
    let review = json!({"route": {
        "paths": [{
            "glob": "docs/**", "route": "auto-merge-eligible",
            "maxDiffLines": 100, "maxUncoveredShare": 0.0,
        }],
        "areas": {"docs": {"route": "auto-merge-eligible", "maxDiffLines": 100, "maxUncoveredShare": 1.0}},
        "sampleRate": 0.1,
    }});
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("docs/data.csv", "a,b\n");
    install.commit("docs: edit the guide, add data");

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        0,
        "route: owner-review\n\
         path `docs/**`: uncovered share 0.25 > maxUncoveredShare 0.00 -> owner-review\n",
        "",
    );
}

/// `--score` gives `senior-review` from `seniorScore` and `owner-review` from
/// `ownerScore`; a score below both changes nothing.
#[test]
fn a_score_raises_the_route_from_its_thresholds() {
    let mut review = docs_auto_merge(50, 1.0);
    review["route"]["ownerScore"] = json!(0.5);
    review["route"]["seniorScore"] = json!(0.8);
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let senior = install.route(&["--base", &base, "--score", "0.9"]);
    let owner = install.route(&["--base", &base, "--score", "0.6"]);
    let low = install.route(&["--base", &base, "--score", "0.2"]);

    assert_output(
        &senior,
        0,
        "route: senior-review\nscore 0.9 >= seniorScore 0.8 -> senior-review\n",
        "",
    );
    assert_output(
        &owner,
        0,
        "route: owner-review\nscore 0.6 >= ownerScore 0.5 -> owner-review\n",
        "",
    );
    assert_output(
        &low,
        0,
        "route: auto-merge-eligible\narea `docs`: auto-merge-eligible\n",
        "",
    );
}

/// A threshold that is the only one set decides alone.
#[test]
fn one_threshold_is_enough_for_a_score() {
    let mut review = docs_auto_merge(50, 1.0);
    review["route"]["ownerScore"] = json!(0.5);
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let output = install.route(&["--base", &base, "--score", "0.9"]);

    assert_output(
        &output,
        0,
        "route: owner-review\nscore 0.9 >= ownerScore 0.5 -> owner-review\n",
        "",
    );
}

/// A `--score` with neither threshold configured is a setup error, with the
/// line of spec 6.2 and no stdout.
#[test]
fn a_score_with_no_threshold_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let output = install.route(&["--base", &base, "--score", "0.5"]);

    assert_output(&output, 2, "", &format!("{SCORE_NEEDS_A_THRESHOLD}\n"));
}

/// A score outside 0 to 1 exits 2 with the line of spec 6.3, whatever the
/// thresholds are: the range comes before the check for a threshold. The line
/// prints the score as a number.
#[test]
fn a_score_outside_zero_to_one_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for (score, printed) in [
        ("1.5", "1.5"),
        ("-0.1", "-0.1"),
        ("NaN", "NaN"),
        ("inf", "inf"),
        ("1e3", "1000"),
    ] {
        let output = install.route(&["--base", &base, "--score", score]);

        assert_output(
            &output,
            2,
            "",
            &format!("route: --score {printed} is outside 0 to 1\n"),
        );
    }
    let output = install.route(&["--base", &base, "--score=-inf"]);
    assert_output(&output, 2, "", "route: --score -inf is outside 0 to 1\n");
}

/// A score that is no number is a usage error of the argument parser.
#[test]
fn a_score_that_is_no_number_is_a_usage_error() {
    let install = Install::new("", &[], None);
    let base = install.head();

    let output = install.route(&["--base", &base, "--score", "high"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("invalid value 'high' for '--score <SCORE>'"),
        "{}",
        stderr(&output)
    );
}

/// The ends of the range 0 to 1 are scores, and `-0` reads as 0.
#[test]
fn a_score_of_0_or_1_is_in_range() {
    let mut review = docs_auto_merge(50, 1.0);
    review["route"]["ownerScore"] = json!(0.0);
    review["route"]["seniorScore"] = json!(1.0);
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let senior = install.route(&["--base", &base, "--score", "1"]);
    let owner = install.route(&["--base", &base, "--score", "0"]);
    let negative_zero = install.route(&["--base", &base, "--score", "-0"]);

    assert_output(
        &senior,
        0,
        "route: senior-review\nscore 1 >= seniorScore 1 -> senior-review\n",
        "",
    );
    let at_zero = "route: owner-review\nscore 0 >= ownerScore 0 -> owner-review\n";
    assert_output(&owner, 0, at_zero, "");
    assert_output(&negative_zero, 0, at_zero, "");
}

/// A change with no changed path takes `review.route.default`, else
/// `owner-review` (review focus 5): the head is an ancestor of the base, so the
/// merge base is the head.
#[test]
fn a_head_behind_its_base_routes_by_default() {
    let install = Install::new("", &[], None);
    let first = install.head();
    install.append("docs/guide.md", 1);
    let second = install.commit("docs: edit the guide");

    let output = install.route(&["--base", &second, "--head", &first]);

    assert_output(
        &output,
        0,
        "route: owner-review\ndefault -> owner-review\n",
        "",
    );
}

/// The same with `review.route.default` set.
#[test]
fn a_head_behind_its_base_takes_the_configured_default() {
    let install = Install::new(
        "",
        &[],
        Some(json!({"route": {"default": "senior-review"}})),
    );
    let first = install.head();
    install.append("docs/guide.md", 1);
    let second = install.commit("docs: edit the guide");

    let output = install.route(&["--base", &second, "--head", &first]);

    assert_output(
        &output,
        0,
        "route: senior-review\ndefault -> senior-review\n",
        "",
    );
}

// ---- the floor -------------------------------------------------------------------------

/// `--no-audit` prints `floor:` and reads no audit: a runner check whose tool
/// is not installed stops the audit run, and not the floor.
#[test]
fn the_floor_reads_no_audit_and_a_missing_tool_does_not_stop_it() {
    let runner = entry(
        "team.lint",
        json!({"type": "runner", "level": "fail", "tool": "lint", "files": "docs/*.md"}),
    );
    let install = Install::new("", &[runner], None);
    let mut marker = read_json(&install.root(), ".houserules.json");
    marker["runners"] = json!({"lint": {
        "command": ["houserules-test-tool-that-is-not-installed"],
        "parser": "exit-code",
        "install": "install the lint tool",
    }});
    install.write(
        ".houserules.json",
        &serde_json::to_string_pretty(&marker).expect("serialize the marker"),
    );
    install.commit("chore: declare the runner");
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");

    let floor = install.route(&["--base", &base, "--no-audit"]);
    let audited = install.route(&["--base", &base]);

    assert_output(
        &floor,
        0,
        "floor: owner-review\narea `docs`: owner-review\n",
        "",
    );
    assert_eq!(audited.status.code(), Some(2));
    assert_eq!(stdout(&audited), "");
    assert!(
        stderr(&audited).contains("install the lint tool"),
        "{}",
        stderr(&audited)
    );
}

/// The floor reads the configured route and `maxDiffLines` only: an uncovered
/// share that fails the audited route leaves the floor at `auto-merge-eligible`.
#[test]
fn the_floor_ignores_the_uncovered_share() {
    let install = Install::new("", &[docs_read()], Some(docs_auto_merge(5, 0.0)));
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("docs/data.csv", "a,b\n");
    install.commit("docs: edit the guide, add data");

    let floor = install.route(&["--base", &base, "--no-audit"]);
    let audited = install.route(&["--base", &base]);

    assert_output(
        &floor,
        0,
        "floor: auto-merge-eligible\narea `docs`: auto-merge-eligible\n",
        "",
    );
    assert_eq!(
        stdout(&audited),
        "route: owner-review\n\
         area `docs`: uncovered share 0.25 > maxUncoveredShare 0.00 -> owner-review\n"
    );
}

/// The floor keeps the rules that read no audit row: a diff over `maxDiffLines`
/// and a score still count.
#[test]
fn the_floor_keeps_the_diff_limit_and_the_score() {
    let mut review = docs_auto_merge(5, 1.0);
    review["route"]["ownerScore"] = json!(0.5);
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 6);
    install.commit("docs: edit the guide");

    let size = install.route(&["--base", &base, "--no-audit"]);
    let score = install.route(&["--base", &base, "--no-audit", "--score", "0.7"]);

    assert_output(
        &size,
        0,
        "floor: owner-review\narea `docs`: diff 6 lines > maxDiffLines 5 -> owner-review\n",
        "",
    );
    assert_eq!(
        stdout(&score),
        "floor: owner-review\n\
         area `docs`: diff 6 lines > maxDiffLines 5 -> owner-review\n\
         score 0.7 >= ownerScore 0.5 -> owner-review\n"
    );
}

/// The floor reads the configured route and `maxDiffLines` of a path entry
/// and not its `maxUncoveredShare` (spec 6.2, rule 6).
#[test]
fn the_floor_reads_the_diff_limit_of_a_path_entry_and_not_its_share() {
    let review = json!({"route": {
        "paths": [{
            "glob": "docs/*.md", "route": "auto-merge-eligible",
            "maxDiffLines": 5, "maxUncoveredShare": 0.0,
        }],
        "areas": {"docs": {"route": "auto-merge-eligible", "maxDiffLines": 100, "maxUncoveredShare": 1.0}},
        "sampleRate": 0.1,
    }});
    let uncovered = Install::new("", &[docs_read()], Some(review.clone()));
    let uncovered_base = uncovered.head();
    uncovered.append("docs/guide.md", 3);
    uncovered.write("docs/data.csv", "a,b\n");
    uncovered.commit("docs: edit the guide, add data");
    let large = Install::new("", &[docs_read()], Some(review));
    let large_base = large.head();
    large.append("docs/guide.md", 6);
    large.commit("docs: edit the guide");

    let share = uncovered.route(&["--base", &uncovered_base, "--no-audit"]);
    let audited = uncovered.route(&["--base", &uncovered_base]);
    let size = large.route(&["--base", &large_base, "--no-audit"]);

    assert_output(
        &share,
        0,
        "floor: auto-merge-eligible\n\
         area `docs`: auto-merge-eligible\n\
         path `docs/*.md`: `docs/guide.md` -> auto-merge-eligible\n",
        "",
    );
    assert_output(
        &audited,
        0,
        "route: owner-review\n\
         path `docs/*.md`: uncovered share 0.25 > maxUncoveredShare 0.00 -> owner-review\n",
        "",
    );
    assert_output(
        &size,
        0,
        "floor: owner-review\n\
         path `docs/*.md`: diff 6 lines > maxDiffLines 5 -> owner-review\n",
        "",
    );
}

/// The floor keeps the rule sources and the route of a path in no area.
#[test]
fn the_floor_keeps_the_rule_sources_and_the_paths_in_no_area() {
    let install = Install::new(
        "",
        &[],
        Some(json!({"route": {"default": "senior-review"}})),
    );
    let base = install.head();
    install.write(
        "knowledge/team.json",
        "{\"entries\": [], \"title\": \"Team\"}\n",
    );
    install.write("misc.txt", "stray\n");
    install.commit("chore: change the knowledge, add a stray file");

    let output = install.route(&["--base", &base, "--no-audit"]);

    assert_output(
        &output,
        0,
        "floor: senior-review\n\
         no area: `knowledge/team.json` -> senior-review\n\
         no area: `misc.txt` -> senior-review\n\
         rule sources: `knowledge/team.json` changed -> senior-review\n",
        "",
    );
}

/// `--audit` and `--no-audit` exclude each other.
#[test]
fn audit_and_no_audit_exclude_each_other() {
    let install = Install::new("", &[], None);
    let base = install.head();

    let output = install.route(&["--base", &base, "--audit", "a.json", "--no-audit"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).contains("cannot be used with"),
        "{}",
        stderr(&output)
    );
}

// ---- the lines of a rule that names paths -----------------------------------------------

/// A rule with 25 paths prints 20 and then `and 5 more`.
#[test]
fn a_rule_with_25_paths_prints_20_and_then_the_rest_as_a_count() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for number in 1..=25 {
        install.write(&format!("misc/f{number:02}.txt"), "x\n");
    }
    install.commit("chore: add 25 stray files");

    let lines = install.route_lines(&["--base", &base]);

    let mut expected = String::from("route: owner-review\n");
    for number in 1..=20 {
        expected.push_str(&format!(
            "no area: `misc/f{number:02}.txt` -> owner-review\n"
        ));
    }
    expected.push_str("and 5 more\n");
    assert_eq!(lines, expected);
}

/// The same for the rule sources: 25 files of `ci write` print 20 and `and 5
/// more`.
#[test]
fn twenty_five_changed_rule_sources_print_20_and_then_the_rest_as_a_count() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for number in 1..=25 {
        install.write_top(
            &format!(".github/workflows/gen{number:02}.yml"),
            &format!("{CI_WRITTEN_BY}\nname: gen{number}\n"),
        );
    }
    install.commit("ci: add 25 files");

    let lines = install.route_lines(&["--base", &base]);

    let mut expected = String::from("route: senior-review\n");
    for number in 1..=20 {
        expected.push_str(&format!(
            "rule sources: `.github/workflows/gen{number:02}.yml` changed -> senior-review\n"
        ));
    }
    expected.push_str("and 5 more\n");
    assert_eq!(lines, expected);
}

/// A path with a backtick prints in a longer code span (spec 4.8).
#[test]
fn a_path_prints_through_the_code_span_of_the_review_text() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.write("odd`name.txt", "x\n");
    install.commit("chore: add a file with a backtick");

    let lines = install.route_lines(&["--base", &base]);

    assert_eq!(
        lines,
        "route: owner-review\nno area: ``odd`name.txt`` -> owner-review\n"
    );
}

// ---- the request's own .gitattributes ---------------------------------------------------

/// A request whose own `.gitattributes` sets `*.rs -diff` on a 2,000-line Rust
/// change still counts those 2,000 lines (and the one line of the attributes
/// file) and keeps the `maxDiffLines` floor of its area (spec 4.2).
#[test]
fn a_gitattributes_with_no_diff_does_not_lower_the_diff_size() {
    let review = json!({"route": {
        "areas": {"code": {
            "route": "auto-merge-eligible", "maxDiffLines": 1500, "maxUncoveredShare": 1.0,
        }},
        "sampleRate": 0.1,
    }});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    install.write(".gitattributes", "*.rs -diff\n");
    install.write("src/big.rs", &numbered("// line", 2000));
    install.commit("feat: add a big file");

    let floor = install.route(&["--base", &base, "--no-audit"]);

    assert_output(
        &floor,
        0,
        "floor: owner-review\n\
         area `code`: diff 2001 lines > maxDiffLines 1500 -> owner-review\n\
         no area: `.gitattributes` -> owner-review\n",
        "",
    );
}

// ---- the setup errors -------------------------------------------------------------------

/// A git older than 2.41 stops the command with the line of spec 4.2; the
/// shim answers a call with `--version` and passes every other call to the
/// real git. Unix only: the shim is a POSIX shell script with an executable
/// bit.
#[cfg(unix)]
#[test]
fn a_git_older_than_2_41_exits_2_with_its_line() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let shim = git_shim(install.scratch.path(), "echo 'git version 2.39.0'; exit 0");

    let output = routed_through(&install, &shim, &["--base", &base]);

    assert_output(
        &output,
        2,
        "",
        "route: git 2.39.0 is older than 2.41, which --attr-source needs\n",
    );
}

/// A `git --version` that fails stops the command with the first line of git's
/// own message. Unix only: the shim is a POSIX shell script with an executable
/// bit.
#[cfg(unix)]
#[test]
fn a_git_that_cannot_report_its_version_exits_2_with_its_line() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let shim = git_shim(install.scratch.path(), "echo 'boom' >&2; exit 1");

    let output = routed_through(&install, &shim, &["--base", &base]);

    assert_output(&output, 2, "", "route: could not run git: boom\n");
}

/// A directory with a `git` that runs `version_script` for a call with the
/// argument `--version` (the command puts options before it, so the shim reads
/// every argument) and runs the real git for every other call. The script is a
/// POSIX shell script with an executable bit, so the shim exists on Unix only.
#[cfg(unix)]
fn git_shim(dir: &Path, version_script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let shim_dir = dir.join("shim");
    fs::create_dir_all(&shim_dir).expect("create the shim directory");
    let script = shim_dir.join("git");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\nfor argument in \"$@\"; do\n  if [ \"$argument\" = \"--version\" ]; then {version_script}; fi\ndone\nPATH=\"$HOUSERULES_TEST_REAL_PATH\" exec git \"$@\"\n"
        ),
    )
    .expect("write the shim");
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).expect("make it executable");
    shim_dir
}

/// Runs `route <args>` with `shim_dir` first on the `PATH` of the child. Unix
/// only: the shim is a POSIX shell script with an executable bit.
#[cfg(unix)]
fn routed_through(install: &Install, shim_dir: &Path, args: &[&str]) -> Output {
    let real_path = std::env::var_os("PATH").expect("a PATH");
    let mut paths = vec![shim_dir.to_path_buf()];
    paths.extend(std::env::split_paths(&real_path));
    houserules()
        .arg("route")
        .args(args)
        .arg("--dir")
        .arg(install.root())
        .env("PATH", std::env::join_paths(paths).expect("join the PATH"))
        .env("HOUSERULES_TEST_REAL_PATH", real_path)
        .output()
        .expect("run route")
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

    let output = install.route(&["--base", &middle, "--audit", audit.to_str().expect("UTF-8")]);

    let head = install.head();
    assert_output(
        &output,
        2,
        "",
        &format!(
            "{} is the audit of {}..{}, not of {}..{}\n",
            audit.display(),
            &base[..7],
            &head[..7],
            &middle[..7],
            &head[..7]
        ),
    );
}

/// An `--audit` file that is no audit result is one named line.
#[test]
fn an_audit_file_that_is_no_audit_result_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let audit = install.scratch_file("not-an-audit.json");
    fs::write(&audit, "{\"base\": 1}\n").expect("write the file");

    let output = install.route(&["--base", &base, "--audit", audit.to_str().expect("UTF-8")]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).starts_with(&format!("{}: not an audit result: ", audit.display())),
        "{}",
        stderr(&output)
    );
}

/// A `review` key with a fault stops the command with the line that
/// `check-knowledge` prints for it.
#[test]
fn a_review_key_with_a_fault_exits_2_with_its_line() {
    let install = Install::new("", &[], Some(json!({"route": {"default": "never-review"}})));
    let base = install.head();

    let output = install.route(&["--base", &base]);

    assert_output(
        &output,
        2,
        "",
        ".houserules.json: review.route.default must be \"owner-review\" or \"senior-review\"\n",
    );
}

/// A score threshold above 1 and a default route of `auto-merge-eligible` are
/// faults of the key: `route` stops with the line of each, and prints no route
/// (spec 6.1; design.md 257).
#[test]
fn a_threshold_above_1_and_an_auto_merge_default_exit_2_with_their_lines() {
    let review = json!({"route": {"default": "auto-merge-eligible", "seniorScore": 2}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();

    let output = install.route(&["--base", &base, "--score", "1"]);

    assert_output(
        &output,
        2,
        "",
        ".houserules.json: review.route.default must be \"owner-review\" or \"senior-review\": \
         a change is auto-merge-eligible only through an area or a path entry that names both limits\n\
         .houserules.json: review.route.seniorScore must be a number from 0 to 1\n",
    );
}

/// A base that is no commit is one named line.
#[test]
fn a_base_that_is_no_commit_exits_2() {
    let install = Install::new("", &[], None);

    let output = install.route(&["--base", "no-such-ref"]);

    assert_output(&output, 2, "", "bad ref \"no-such-ref\"\n");
}

/// `--base` is required.
#[test]
fn a_missing_base_is_a_usage_error() {
    let install = Install::new("", &[], None);

    let output = install.route(&[]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("--base"), "{}", stderr(&output));
}

/// An unwritable `--json` is one named line, and nothing prints.
#[test]
fn an_unwritable_json_file_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let target = file(install.scratch.path(), "missing-folder/route.json");

    let output = install.route(&["--base", &base, "--json", target.to_str().expect("UTF-8")]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    assert!(
        stderr(&output).starts_with(&format!("{}: ", target.display())),
        "{}",
        stderr(&output)
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

/// The reasons of `document` with the rule `rule`.
fn reasons_of<'a>(document: &'a Value, rule: &str) -> Vec<&'a Value> {
    document["reasons"]
        .as_array()
        .expect("a reasons array")
        .iter()
        .filter(|reason| reason["rule"] == rule)
        .collect()
}

/// The rule of each reason of `document`, in the order of the reasons.
fn rules_of(document: &Value) -> Vec<String> {
    document["reasons"]
        .as_array()
        .expect("a reasons array")
        .iter()
        .map(|reason| reason["rule"].as_str().expect("a rule").to_string())
        .collect()
}

/// The one reason of `rule` in `document` is the reason of a rule that was
/// evaluated and found nothing (spec 6.4): it reads the key `input` of the
/// inputs, counts 0, has no threshold, gives the lowest route, and names no
/// path.
fn assert_found_nothing(document: &Value, rule: &str, input: &str, decisive: bool) {
    let reasons = reasons_of(document, rule);
    assert_eq!(reasons.len(), 1, "{rule}: {reasons:?}");
    assert_eq!(
        *reasons[0],
        json!({
            "rule": rule, "input": input, "value": 0, "threshold": null,
            "route": "auto-merge-eligible", "paths": [], "more_paths": 0,
            "decisive": decisive,
        }),
        "{rule}"
    );
}

/// The document names `schema` first, every input, and each threshold, and it
/// validates against its generated schema.
#[test]
fn the_document_names_every_input_and_each_threshold_and_validates() {
    let mut review = docs_auto_merge(5, 0.5);
    review["route"]["ownerScore"] = json!(0.5);
    review["route"]["seniorScore"] = json!(0.8);
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 3);
    let head = install.commit("docs: edit the guide");
    let target = install.scratch_file("route.json");

    let output = install.route(&[
        "--base",
        &base,
        "--score",
        "0.2",
        "--json",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(
        &output,
        0,
        "route: auto-merge-eligible\narea `docs`: auto-merge-eligible\n",
        "",
    );
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    assert_valid(&document, ROUTE_SCHEMA);
    assert_eq!(
        keys(&document),
        [
            "schema", "install", "head", "route", "floor", "inputs", "reasons"
        ]
    );
    assert_eq!(document["schema"], "houserules/route@1.0.0");
    assert_eq!(document["install"], ".");
    assert_eq!(document["head"], head);
    assert_eq!(document["route"], "auto-merge-eligible");
    assert_eq!(document["floor"], false);
    assert_eq!(
        keys(&document["inputs"]),
        [
            "failed",
            "warned",
            "areas",
            "no_area",
            "paths",
            "diff_lines",
            "uncovered_share",
            "score",
            "rule_sources_changed"
        ]
    );
    assert_eq!(
        document["inputs"]["areas"],
        json!({"docs": ["docs/guide.md"]})
    );
    assert_eq!(document["inputs"]["diff_lines"], 3);
    assert_eq!(document["inputs"]["uncovered_share"], 0.0);
    assert_eq!(document["inputs"]["score"], 0.2);
    let diff = reasons_of(&document, "area-diff");
    let share = reasons_of(&document, "area-share");
    let senior = reasons_of(&document, "score-senior");
    let owner = reasons_of(&document, "score-owner");
    assert_eq!(
        (diff[0]["value"].clone(), diff[0]["threshold"].clone()),
        (json!(3), json!(5))
    );
    assert_eq!(
        (share[0]["value"].clone(), share[0]["threshold"].clone()),
        (json!(0.0), json!(0.5))
    );
    assert_eq!(
        (senior[0]["value"].clone(), senior[0]["threshold"].clone()),
        (json!(0.2), json!(0.8))
    );
    assert_eq!(
        (owner[0]["value"].clone(), owner[0]["threshold"].clone()),
        (json!(0.2), json!(0.5))
    );
    for reason in document["reasons"].as_array().expect("reasons") {
        assert_eq!(
            keys(reason),
            [
                "rule",
                "input",
                "value",
                "threshold",
                "route",
                "paths",
                "more_paths",
                "decisive"
            ]
        );
    }
}

/// A floor document says `floor: true`, has no uncovered share, and holds no
/// failed or warned row; it validates.
#[test]
fn a_floor_document_has_no_uncovered_share_and_validates() {
    let failing = entry(
        "team.no-bad-word",
        grep("docs/*.md", "BADWORD", "fail", false),
    );
    let install = Install::new("", &[failing], Some(docs_auto_merge(50, 1.0)));
    let base = install.head();
    install.write("docs/guide.md", "BADWORD\n");
    install.commit("docs: edit the guide");
    let target = install.scratch_file("floor.json");

    let output = install.route(&[
        "--base",
        &base,
        "--no-audit",
        "--json",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(
        &output,
        0,
        "floor: auto-merge-eligible\narea `docs`: auto-merge-eligible\n",
        "",
    );
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    assert_valid(&document, ROUTE_SCHEMA);
    assert_eq!(document["floor"], true);
    assert_eq!(document["inputs"]["uncovered_share"], Value::Null);
    assert_eq!(document["inputs"]["failed"], json!([]));
    assert_eq!(document["inputs"]["warned"], json!([]));
    assert!(reasons_of(&document, "area-share").is_empty());
    assert_eq!(
        rules_of(&document),
        ["rule-sources", "area", "area-diff", "no-area", "path"],
        "a floor reads no audit row: no reason for rules 2 and 3"
    );
}

/// Every rule of spec 6.2 that found something is a reason of the document,
/// whether or not it is at the final route; `decisive` marks those at it.
#[test]
fn the_document_keeps_a_reason_below_the_final_route_and_marks_the_decisive_ones() {
    let review = json!({"route": {
        "areas": {"docs": {
            "route": "auto-merge-eligible", "maxDiffLines": 100, "maxUncoveredShare": 1.0,
        }},
        "sampleRate": 0.1,
    }});
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.write("misc.txt", "stray\n");
    install.commit("chore: edit the guide, add a stray file");
    let target = install.scratch_file("route.json");

    let lines = install.route_lines(&["--base", &base, "--json", target.to_str().expect("UTF-8")]);

    assert_eq!(
        lines,
        "route: owner-review\nno area: `misc.txt` -> owner-review\n"
    );
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    let area = reasons_of(&document, "area");
    let no_area = reasons_of(&document, "no-area");
    assert_eq!(area[0]["route"], "auto-merge-eligible");
    assert_eq!(area[0]["decisive"], false);
    assert_eq!(no_area[0]["route"], "owner-review");
    assert_eq!(no_area[0]["decisive"], true);
    assert_eq!(no_area[0]["paths"], json!(["misc.txt"]));
    assert_eq!(no_area[0]["more_paths"], 0);
}

/// The document holds one reason for every rule of spec 6.2 that was
/// evaluated, one that found nothing included (spec 6.4): an audited run of a
/// project with no route and a change in one area has a reason for the rules
/// 1 to 6, five of them at the lowest route with the value 0.
#[test]
fn the_document_holds_a_reason_for_every_rule_that_was_evaluated() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.commit("docs: edit the guide");
    let target = install.scratch_file("route.json");

    let lines = install.route_lines(&["--base", &base, "--json", target.to_str().expect("UTF-8")]);

    assert_eq!(lines, "route: owner-review\narea `docs`: owner-review\n");
    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    assert_valid(&document, ROUTE_SCHEMA);
    assert_eq!(
        rules_of(&document),
        [
            "rule-sources",
            "failed",
            "warned",
            "area",
            "no-area",
            "path"
        ]
    );
    assert_found_nothing(&document, "rule-sources", "rule_sources_changed", false);
    assert_found_nothing(&document, "failed", "failed", false);
    assert_found_nothing(&document, "warned", "warned", false);
    assert_found_nothing(&document, "no-area", "no_area", false);
    assert_found_nothing(&document, "path", "paths", false);
    assert_eq!(
        *reasons_of(&document, "area")[0],
        json!({
            "rule": "area", "input": "docs", "value": null, "threshold": null,
            "route": "owner-review", "paths": [], "more_paths": 0, "decisive": true,
        })
    );
}

/// A change with no changed path finds nothing in rules 1 to 6 and takes the
/// default route: the reasons of the six rules, then `default`.
#[test]
fn a_change_with_no_changed_path_has_a_reason_for_each_rule_and_the_default() {
    let install = Install::new("", &[], None);
    let first = install.head();
    install.append("docs/guide.md", 1);
    let second = install.commit("docs: edit the guide");
    let target = install.scratch_file("route.json");

    install.route_lines(&[
        "--base",
        &second,
        "--head",
        &first,
        "--json",
        target.to_str().expect("UTF-8"),
    ]);

    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    assert_eq!(
        rules_of(&document),
        [
            "rule-sources",
            "failed",
            "warned",
            "area",
            "no-area",
            "path",
            "default"
        ]
    );
    assert_found_nothing(&document, "area", "areas", false);
    assert_eq!(
        *reasons_of(&document, "default")[0],
        json!({
            "rule": "default", "input": "default", "value": null, "threshold": null,
            "route": "owner-review", "paths": [], "more_paths": 0, "decisive": true,
        })
    );
}

/// A reason caps its paths at 20 and counts the rest.
#[test]
fn a_reason_lists_at_most_20_paths_and_counts_the_rest() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for number in 1..=25 {
        install.write(&format!("misc/f{number:02}.txt"), "x\n");
    }
    install.commit("chore: add 25 stray files");
    let target = install.scratch_file("route.json");

    install.route_lines(&["--base", &base, "--json", target.to_str().expect("UTF-8")]);

    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    let no_area = reasons_of(&document, "no-area");
    assert_eq!(no_area[0]["paths"].as_array().expect("paths").len(), 20);
    assert_eq!(no_area[0]["more_paths"], 5);
    assert_eq!(
        document["inputs"]["no_area"]
            .as_array()
            .expect("no_area")
            .len(),
        25,
        "the input lists every path"
    );
}

/// A subdirectory install names itself in the document.
#[test]
fn a_subdirectory_install_names_its_path_in_the_document() {
    let install = Install::new("sub", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    install.commit("docs: edit the guide");
    let target = install.scratch_file("route.json");

    install.route_lines(&["--base", &base, "--json", target.to_str().expect("UTF-8")]);

    let document: Value =
        serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
            .expect("the document is JSON");
    assert_valid(&document, ROUTE_SCHEMA);
    assert_eq!(document["install"], "sub");
}

/// The same inputs give the same bytes: the lines and the document (spec 5.8).
#[test]
fn the_same_range_gives_the_same_bytes_twice() {
    let review = docs_auto_merge(5, 0.0);
    let install = Install::new("", &[docs_read()], Some(review));
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("docs/data.csv", &numbered("row", 3));
    install.write("misc.txt", "stray\n");
    install.commit("chore: change several areas");
    let first = install.scratch_file("first.json");
    let second = install.scratch_file("second.json");

    let first_lines =
        install.route_lines(&["--base", &base, "--json", first.to_str().expect("UTF-8")]);
    let second_lines =
        install.route_lines(&["--base", &base, "--json", second.to_str().expect("UTF-8")]);

    assert_eq!(first_lines, second_lines);
    assert_eq!(
        fs::read(&first).expect("read the first"),
        fs::read(&second).expect("read the second")
    );
}

// ---- the help ---------------------------------------------------------------------------

/// The sentences of the `about` of `route`.
const ABOUT: [&str; 18] = [
    "Decides the review route of a change, `auto-merge-eligible`, `owner-review`, or `senior-review`, and prints it.",
    "The route is the highest route that any rule gives.",
    "A changed rule source gives `senior-review`, whatever the configuration.",
    "A failed row of the audit gives `owner-review` unless a `--sanctioned` booking accepted it.",
    "A warned row gives `review.route.warn`.",
    "Each area that holds a changed path gives its route, else `review.route.default`.",
    "A changed path in no area gives `review.route.default`.",
    "Each entry of `review.route.paths` whose glob matches a changed path gives its route.",
    "An area or a path entry with the route `auto-merge-eligible` keeps it only while the diff size is at most its `maxDiffLines` and the uncovered share is at most its `maxUncoveredShare`; past either, it gives `owner-review`.",
    "`--score` gives `senior-review` from `seniorScore` and `owner-review` from `ownerScore`.",
    "A change with no changed path and no changed rule source takes `review.route.default`.",
    "A project that sets no route gets `owner-review` for each change, except a change to a rule source, which gets `senior-review`.",
    "The audit rows come from `--audit`, else from an audit that the command runs in process.",
    "`--no-audit` computes the tool-free floor: it runs no check, reads no audit row and no uncovered share, and prints `floor: <route>` in place of `route: <route>`.",
    "The command prints `route: <route>`, then one line for each reason at that route, sorted; a rule that names paths prints at most 20 of them and then `and <k> more`.",
    "`--json` also writes the decision document `houserules/route@1.0.0`.",
    "It exits 0 for every route.",
    "It exits 2 for a setup error, for example: a ref that is no commit, an `--audit` file of another range or that is no audit result, an audit that cannot run, a `review` key with a fault, a `--score` outside 0 to 1, a `--score` when `review.route.ownerScore` and `review.route.seniorScore` are both missing, a git older than 2.41, or a `--json` file that cannot be written",
];

/// The sentences that the help of `route` stated once and that are false.
const STALE: [&str; 3] = [
    "A project that sets no route gets `owner-review` for every change.",
    "A change with no changed path takes `review.route.default`.",
    "It exits 2 for a setup error: a ref that is no commit",
];

/// The help text of each argument of `route`.
const ARGUMENTS: [&str; 7] = [
    "The base ref of the range. The range runs from the merge base of `--base` and `--head` to `--head`",
    "The head ref of the range; defaults to `HEAD`",
    "The file that `audit --json` wrote for the same range. Its `base` and `head` must name the same commits as `--base` and `--head`. Without this flag and without `--no-audit`, the command runs the audit in process",
    "Computes the floor: the route from the rules that read no audit row. A runner check whose tool is not installed does not stop it",
    "An external score from 0 to 1. It needs `review.route.ownerScore` or `review.route.seniorScore`",
    "Also writes the decision document `houserules/route@1.0.0` to this file",
    "Install root to read from; defaults to the nearest directory at or above the current one that holds `.houserules.json`, no higher than the git top level, else the git top level",
];

/// `route --help` states each rule, the floor, the lines, the exits, and each
/// condition of an argument (`quality.user-facing-text-is-contract`), and
/// `houserules --help` lists the same about text. Neither holds a sentence that
/// the command made false: a project with no route gets `senior-review` for a
/// rule source, a `default` needs no changed rule source, and the exit-2 list
/// is a list of examples.
#[test]
fn the_help_of_route_states_the_rules_the_floor_the_lines_and_the_exits() {
    states("route --help", &help_of("route"), &ABOUT);
    states("route --help", &help_of("route"), &ARGUMENTS);
    states("houserules --help", &help_of_args(&["--help"]), &ABOUT);
    omits("route --help", &help_of("route"), &STALE);
    omits("houserules --help", &help_of_args(&["--help"]), &STALE);
}
