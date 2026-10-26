//! `houserules record list`, `houserules record audit`, `houserules stats
//! --records`, and the warn line of `houserules stats` for a `Reversal:` inside
//! a longer ledger line (batch 37, spec 8.3 and 11.6; acceptance case 5, the
//! list, audit, and stats parts; case 10, the warn line; HR-192 and HR-241).
//! Each case runs the compiled binary against a scratch repository whose
//! install holds the areas `docs`, `code`, and `ops`, and reads the printed text
//! or the note whole. A line of the spec is the literal of a test.
//!
//! Every fixture is hermetic: the install is written by hand, and a commit has
//! the committer date that its case gives it, so the order of a listing never
//! depends on the clock. A scratch repository holds its own `user.name` and
//! `user.email`, because the commands write to the notes ref; no test reads a
//! global git identity.

mod help_support;
mod install_shape;
mod note_support;
mod schema_support;
mod stats_support;
mod tracker_support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use help_support::{help_of_args, states};
use schema_support::assert_valid;
use serde_json::{Value, json};
use stats_support::{fixture, write_knowledge_base};
use tempfile::TempDir;
use tracker_support::{
    assert_output, git, houserules, scratch_git_repo, stderr, stdout, with_default_git,
};

/// The schema file of the change record, relative to the repository root.
const RECORD_SCHEMA: &str = "schemas/change-record/1.1.0.json";

/// The notes ref of a top-level install (spec 8.1).
const TOP_REF: &str = "refs/notes/houserules";

/// The line of a reader without the notes ref (spec 8.1) for a top-level
/// install.
const FETCH_LINE: &str = "no change records here; fetch them: git fetch origin refs/notes/houserules:refs/notes/houserules\n";

/// The committer time of the base commit of every fixture.
const BASE_TIME: u64 = 1_700_000_000;

/// The warn line of spec 11.6, after `<ledger>:<line>: `.
const EMBEDDED_REVERSAL: &str =
    "a Reversal: inside a longer entry; stats counts only a line that starts with it";

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

/// The entry `team.docs-read`: a covering check that passes on any change to a
/// Markdown file of `docs/`.
fn docs_read() -> Value {
    json!({
        "id": "team.docs-read", "kind": "rule", "area": "global", "standing": true,
        "summary": "A rule.", "body": ["Body."], "tags": [],
        "source": {"date": "2026-10-24", "by": "test"},
        "check": {
            "type": "grep-absent", "level": "fail", "files": ["docs/*.md"],
            "pattern": "ZZNEVERZZ", "scope": "changed", "covers": true,
        },
    })
}

/// The `review` key that makes the area `docs` `auto-merge-eligible` (every
/// draw selects the record: the sample rate is 1) and the area `ops`
/// `senior-review`.
fn three_routes() -> Value {
    json!({"route": {
        "areas": {
            "docs": {"route": "auto-merge-eligible", "maxDiffLines": 1000, "maxUncoveredShare": 1.0},
            "ops": {"route": "senior-review"},
        },
        "sampleRate": 1.0,
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

/// The first seven characters of `sha`.
fn short(sha: &str) -> &str {
    &sha[..7]
}

/// A scratch repository with one install and its base commit.
struct Install {
    /// The git top level.
    top: TempDir,
    /// The folder of the install below the top level: `""` or `"sub"`.
    folder: &'static str,
    /// A directory outside the repository, for the files that a case writes.
    scratch: TempDir,
}

impl Install {
    /// A repository whose install in `folder` holds `entries` and the marker
    /// with the `review` key `review`, and the files `docs/guide.md` (10
    /// lines), `src/lib.rs` (10 lines), and `ops/deploy.sh` (5 lines). The base
    /// commit holds them.
    fn new(folder: &'static str, entries: &[Value], review: Option<Value>) -> Self {
        let install = Self {
            top: scratch_git_repo(),
            folder,
            scratch: tempfile::tempdir().expect("tempdir"),
        };
        git(install.top.path(), &["config", "user.name", "t"]);
        git(install.top.path(), &["config", "user.email", "t@t.t"]);
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
        install.commit_at("chore: base", BASE_TIME);
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

    /// Adds one line to the end of `path` below the install root.
    fn append(&self, path: &str) {
        let target = file(&self.root(), path);
        let mut text = fs::read_to_string(&target).unwrap_or_default();
        text.push_str("added\n");
        self.write(path, &text);
    }

    /// Commits every change with the committer time `seconds` and an author
    /// time that is the same for every commit (a listing by author time would
    /// tie), and gives the sha of the new commit.
    fn commit_at(&self, message: &str, seconds: u64) -> String {
        git(self.top.path(), &["add", "-A"]);
        let output = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t.t",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-q",
                "--no-verify",
                "--allow-empty",
                "-m",
                message,
            ])
            .env("GIT_AUTHOR_DATE", "4000000000 +0000")
            .env("GIT_COMMITTER_DATE", format!("{seconds} +0000"))
            .current_dir(self.top.path())
            .output()
            .expect("run git commit");
        assert!(output.status.success(), "{}", stderr(&output));
        self.head()
    }

    /// Appends a line to `path` and commits it at the time `seconds`.
    fn edit(&self, path: &str, seconds: u64) -> String {
        self.append(path);
        self.commit_at(&format!("edit {path}"), seconds)
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

    /// The notes ref of the install (spec 8.1).
    fn notes_ref(&self) -> String {
        if self.folder.is_empty() {
            TOP_REF.to_string()
        } else {
            format!("refs/notes/houserules-sub/{}", self.folder)
        }
    }

    /// Runs `houserules <args> --dir <root>`.
    fn run(&self, args: &[&str]) -> Output {
        houserules()
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run houserules")
    }

    /// Writes the record of the range `base` to `head`, keyed by `head`, with
    /// the flags `extra`, and requires exit 0 and no output.
    fn write_record(&self, base: &str, head: &str, extra: &[&str]) {
        let mut args = vec!["record", "write", "--base", base, "--head", head];
        args.extend(extra);
        assert_output(&self.run(&args), 0, "", "");
    }

    /// The text of the note on `key` in the notes ref of the install.
    fn note_text(&self, key: &str) -> String {
        git(
            self.top.path(),
            &["notes", "--ref", &self.notes_ref(), "show", key],
        )
    }

    /// The note on `key`, parsed.
    fn note(&self, key: &str) -> Value {
        serde_json::from_str(&self.note_text(key)).expect("a note that holds JSON")
    }

    /// Replaces the note on `key` with `bytes`, which stay as they are.
    fn set_note_text(&self, key: &str, text: &str) {
        let target = self.scratch_file("note.txt");
        fs::write(&target, text).expect("write the note file");
        let blob = git(
            self.top.path(),
            &["hash-object", "-w", target.to_str().expect("UTF-8")],
        );
        git(
            self.top.path(),
            &[
                "notes",
                "--ref",
                &self.notes_ref(),
                "add",
                "-f",
                "-C",
                blob.trim(),
                key,
            ],
        );
    }

    /// Replaces the note on `key` with the compact JSON of `record` and a line
    /// feed.
    fn set_note(&self, key: &str, record: &Value) {
        self.set_note_text(key, &format!("{record}\n"));
    }

    /// Sets the post-merge audit of the record on `key` by hand.
    fn set_audit(&self, key: &str, verdict: &str) {
        let mut record = self.note(key);
        record["post_merge_audit"] = json!({"verdict": verdict, "by": "amy", "note": null});
        self.set_note(key, &record);
    }
}

/// The three merges of the listing cases, each with its own record:
///
/// - `docs` (committer time 1,700,000,100): an `auto-merge-eligible` change
///   with two reviewers, sampled, never audited.
/// - `ops` (1,700,000,200): a `senior-review` change with no reviewer, audited
///   `pass`.
/// - `code` (1,700,000,300): an `owner-review` change with one reviewer,
///   audited `fail`.
struct Merges {
    /// The repository.
    install: Install,
    /// The base commit of the first record.
    base: String,
    /// The key of the first record.
    docs: String,
    /// The key of the second record.
    ops: String,
    /// The key of the third record.
    code: String,
}

/// The repository of [`Merges`]; the audits are set by hand.
fn three_merges() -> Merges {
    let install = Install::new("", &[docs_read()], Some(three_routes()));
    let base = install.head();
    let docs = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &docs, &["--reviewer", "amy", "--reviewer", "bob"]);
    let ops = install.edit("ops/deploy.sh", BASE_TIME + 200);
    install.write_record(&docs, &ops, &[]);
    let code = install.edit("src/lib.rs", BASE_TIME + 300);
    install.write_record(&ops, &code, &["--reviewer", "amy"]);
    install.set_audit(&ops, "pass");
    install.set_audit(&code, "fail");
    Merges {
        install,
        base,
        docs,
        ops,
        code,
    }
}

/// The lines of `listing`, one for each of `rows` (key, route, reviewers, the
/// sample word, the audit word).
fn list_lines(rows: &[(&str, &str, u64, &str, &str)]) -> String {
    rows.iter()
        .map(|(key, route, reviewers, sampled, audit)| {
            format!(
                "{} {route} {reviewers} reviewers {sampled} {audit}\n",
                short(key)
            )
        })
        .collect()
}

/// `record` cut to the minimal form of spec 8.4, step 8: the keys that hold
/// the packet are gone, the route keeps its name and its floor flag, and the
/// area inputs go with the route inputs.
fn minimal_form(record: &mut Value) {
    let object = record.as_object_mut().expect("an object");
    for key in ["commits", "evidence", "bundle", "links"] {
        object.remove(key);
    }
    let route = record["route"]["route"].clone();
    record["route"] = json!({"route": route, "floor": false});
    record["truncated"] = json!(["all"]);
}

// ---- a record of schema 1.0.0 -------------------------------------------------------------

/// A note of schema 1.0.0, as the binary of batch 37 wrote it, lists and audits
/// like a record of 1.1.0: `record list` reads it, and `record audit` sets its
/// post-merge audit, changes no other byte, and adds no `sessions` key.
#[test]
fn a_record_of_schema_1_0_0_lists_and_audits() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    let head = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &head, &[]);
    let hash = install.note(&head)["rules"]["head_hash"]
        .as_str()
        .expect("a hash")
        .to_string();
    install.set_note(&head, &note_support::note_1_0_0(&base, &head, &hash));
    let before = install.note_text(&head);
    assert!(!before.contains("sessions"), "{before}");

    let listed = install.run(&["record", "list"]);
    let audited = install.run(&["record", "audit", &head, "--verdict", "pass", "--by", "amy"]);

    assert_output(
        &listed,
        0,
        &list_lines(&[(&head, "owner-review", 0, "-", "unaudited")]),
        "",
    );
    assert_output(&audited, 0, "", "");
    let audit = json!({"verdict": "pass", "by": "amy", "note": null});
    let expected = before.replacen(
        "\"post_merge_audit\":null",
        &format!("\"post_merge_audit\":{audit}"),
        1,
    );
    assert_ne!(expected, before);
    assert_eq!(install.note_text(&head), expected);
}

// ---- record list -------------------------------------------------------------------------

/// Spec 8.3: one line per record, by the committer date of its key, newest
/// first: `<key7> <route> <n> reviewers <sampled|-> <pass|fail|unaudited>`.
#[test]
fn record_list_prints_one_line_per_record_newest_first() {
    let merges = three_merges();

    let output = merges.install.run(&["record", "list"]);

    assert_output(
        &output,
        0,
        &list_lines(&[
            (&merges.code, "owner-review", 1, "-", "fail"),
            (&merges.ops, "senior-review", 0, "-", "pass"),
            (
                &merges.docs,
                "auto-merge-eligible",
                2,
                "sampled",
                "unaudited",
            ),
        ]),
        "",
    );
}

/// The ties of one committer time go by sha, ascending.
#[test]
fn records_of_one_committer_time_list_by_sha() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let first = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &first, &[]);
    let second = install.edit("ops/deploy.sh", BASE_TIME + 100);
    install.write_record(&first, &second, &[]);
    let third = install.edit("src/lib.rs", BASE_TIME + 100);
    install.write_record(&second, &third, &[]);
    let mut shas = [first, second, third];
    shas.sort();

    let output = install.run(&["record", "list"]);

    let expected: String = shas
        .iter()
        .map(|sha| format!("{} owner-review 0 reviewers - unaudited\n", short(sha)))
        .collect();
    assert_output(&output, 0, &expected, "");
}

/// `--sampled` keeps the sampled records (spec 8.3).
#[test]
fn sampled_keeps_the_sampled_records() {
    let merges = three_merges();

    let output = merges.install.run(&["record", "list", "--sampled"]);

    assert_output(
        &output,
        0,
        &list_lines(&[(
            &merges.docs,
            "auto-merge-eligible",
            2,
            "sampled",
            "unaudited",
        )]),
        "",
    );
}

/// A draw that did not select the record leaves it out of `--sampled`: the
/// sample of the record is `{rate, sampled: false}`.
#[test]
fn a_record_whose_draw_missed_is_not_sampled() {
    let merges = three_merges();
    let mut record = merges.install.note(&merges.docs);
    record["sample"] = json!({"rate": 0.5, "sampled": false});
    merges.install.set_note(&merges.docs, &record);

    let sampled = merges.install.run(&["record", "list", "--sampled"]);
    let all = merges.install.run(&["record", "list"]);

    assert_output(&sampled, 0, "", "");
    assert!(
        stdout(&all).ends_with(" 2 reviewers - unaudited\n"),
        "{}",
        stdout(&all)
    );
}

/// `--json` prints one object for each record, in the order of the lines.
#[test]
fn json_prints_one_object_for_each_record() {
    let merges = three_merges();

    let output = merges.install.run(&["record", "list", "--json"]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let text = stdout(&output);
    assert!(text.ends_with("\n"), "{text:?}");
    let listed: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(
        listed,
        json!([
            {"key": merges.code, "route": "owner-review", "reviewers": 1, "sampled": false, "audit": "fail"},
            {"key": merges.ops, "route": "senior-review", "reviewers": 0, "sampled": false, "audit": "pass"},
            {"key": merges.docs, "route": "auto-merge-eligible", "reviewers": 2, "sampled": true, "audit": "unaudited"},
        ])
    );
    let sampled = merges
        .install
        .run(&["record", "list", "--json", "--sampled"]);
    let kept: Value = serde_json::from_str(&stdout(&sampled)).expect("JSON");
    assert_eq!(kept.as_array().map(Vec::len), Some(1));
}

/// A reviewer left out of the list of 20 still counts in `<n>`: the count is
/// the names and the names left out.
#[test]
fn the_reviewer_count_includes_the_names_left_out() {
    let merges = three_merges();
    let mut record = merges.install.note(&merges.code);
    let names: Vec<String> = (0..20).map(|n| format!("user{n:02}")).collect();
    record["reviewers"] = json!(names);
    record["reviewers_more"] = json!(5);
    merges.install.set_note(&merges.code, &record);

    let output = merges.install.run(&["record", "list"]);

    assert!(
        stdout(&output).starts_with(&format!(
            "{} owner-review 25 reviewers - fail\n",
            short(&merges.code)
        )),
        "{}",
        stdout(&output)
    );
}

/// `<range>` keeps the records whose key is in the range, as `git rev-list`
/// reads it.
#[test]
fn a_range_keeps_the_records_whose_key_is_in_it() {
    let merges = three_merges();
    let after_docs = format!("{}..{}", merges.docs, merges.code);
    let up_to_ops = merges.ops.clone();

    let later = merges.install.run(&["record", "list", &after_docs]);
    let earlier = merges.install.run(&["record", "list", &up_to_ops]);

    assert_output(
        &later,
        0,
        &list_lines(&[
            (&merges.code, "owner-review", 1, "-", "fail"),
            (&merges.ops, "senior-review", 0, "-", "pass"),
        ]),
        "",
    );
    assert_output(
        &earlier,
        0,
        &list_lines(&[
            (&merges.ops, "senior-review", 0, "-", "pass"),
            (
                &merges.docs,
                "auto-merge-eligible",
                2,
                "sampled",
                "unaudited",
            ),
        ]),
        "",
    );
    let none = merges
        .install
        .run(&["record", "list", &format!("{0}..{0}", merges.code)]);
    assert_output(&none, 0, "", "");
}

/// A range that git does not read is a setup error: one named line, exit 2.
#[test]
fn a_range_that_git_does_not_read_exits_2() {
    let merges = three_merges();

    let output = merges.install.run(&["record", "list", "nope..HEAD"]);

    assert_output(&output, 2, "", "bad range \"nope..HEAD\"\n");
}

/// A record whose key the clone lacks has no committer date: it comes after the
/// dated records, by sha, and the clone does not stop. A range cannot hold it
/// (`a_shallow_clone_lists_a_range_it_holds_and_keeps_the_bad_range_line`).
#[test]
fn a_record_whose_key_the_clone_lacks_comes_last() {
    let merges = three_merges();
    let (_holder, clone) = shallow_clone_of(&merges.install, 1);

    let output = list_in(&clone, &[]);

    let mut undated = [merges.docs.clone(), merges.ops.clone()];
    undated.sort();
    let mut expected = format!("{} owner-review 1 reviewers - fail\n", short(&merges.code));
    for key in &undated {
        let (route, reviewers, sampled, audit) = if *key == merges.docs {
            ("auto-merge-eligible", 2, "sampled", "unaudited")
        } else {
            ("senior-review", 0, "-", "pass")
        };
        expected.push_str(&format!(
            "{} {route} {reviewers} reviewers {sampled} {audit}\n",
            short(key)
        ));
    }
    assert_output(&output, 0, &expected, "");
}

/// A clone without the notes ref gets the one line of spec 8.1, exit 1.
#[test]
fn a_clone_without_the_notes_ref_prints_the_fetch_line() {
    let install = Install::new("", &[], None);

    let output = install.run(&["record", "list"]);

    assert_output(&output, 1, "", FETCH_LINE);
}

/// Nothing to list prints nothing, and `--json` prints an empty list.
#[test]
fn an_empty_listing_prints_nothing_and_an_empty_json_list() {
    let merges = three_merges();
    let nothing = format!("{0}..{0}", merges.code);

    let lines = merges.install.run(&["record", "list", &nothing]);
    let json = merges.install.run(&["record", "list", &nothing, "--json"]);

    assert_output(&lines, 0, "", "");
    assert_output(&json, 0, "[]\n", "");
}

/// A note that is no change record is one named line, exit 2.
#[test]
fn a_note_that_is_no_change_record_is_one_named_line() {
    let merges = three_merges();
    merges.install.set_note_text(&merges.ops, "not json\n");

    let output = merges.install.run(&["record", "list"]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(stdout(&output), "");
    let line = stderr(&output);
    assert!(
        line.starts_with(&format!(
            "record: {}: not a change record (",
            short(&merges.ops)
        )),
        "{line}"
    );
    assert_eq!(line.lines().count(), 1, "{line}");
}

/// The listing is the same under the color settings of a user
/// (`houserules.pinned-git-text-runs-hermetic`): the listing of every record and
/// the listing of a range, which read git through two different calls, print the
/// same bytes with the settings as without them. The ranged listing is also the
/// records of the range, so a setting that emptied it would not pass for equal.
#[test]
fn the_listing_is_the_same_under_the_color_settings_of_a_user() {
    let merges = three_merges();
    let range = format!("{}..{}", merges.docs, merges.code);
    let in_range = list_lines(&[
        (&merges.code, "owner-review", 1, "-", "fail"),
        (&merges.ops, "senior-review", 0, "-", "pass"),
    ]);
    let plain = merges.install.run(&["record", "list", "--json"]);
    let plain_ranged = merges.install.run(&["record", "list", &range]);
    for (key, value) in [
        ("color.ui", "always"),
        ("color.grep", "always"),
        ("log.showSignature", "true"),
        ("log.date", "iso"),
    ] {
        git(merges.install.top.path(), &["config", key, value]);
    }

    let colored = merges.install.run(&["record", "list", "--json"]);
    let colored_ranged = merges.install.run(&["record", "list", &range]);

    assert_eq!(colored.status.code(), Some(0), "{}", stderr(&colored));
    assert_eq!(stdout(&colored), stdout(&plain));
    assert_output(&plain_ranged, 0, &in_range, "");
    assert_output(&colored_ranged, 0, &stdout(&plain_ranged), "");
}

/// A second run prints the same bytes: the order has no clock and no
/// environment in it.
#[test]
fn two_runs_print_the_same_bytes() {
    let merges = three_merges();

    let first = merges.install.run(&["record", "list"]);
    let second = merges.install.run(&["record", "list"]);

    assert_eq!(first.status.code(), Some(0), "{}", stderr(&first));
    assert_eq!(stdout(&first).lines().count(), 3);
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stderr, second.stderr);
}

/// A subdirectory install lists the records of its own notes ref only.
#[test]
fn a_subdirectory_install_lists_its_own_records() {
    let install = Install::new("sub", &[], None);
    let base = install.head();
    let head = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &head, &["--reviewer", "amy"]);
    git(
        install.top.path(),
        &[
            "notes",
            "--ref",
            TOP_REF,
            "add",
            "-f",
            "-m",
            "a note of another install",
            &head,
        ],
    );

    let output = install.run(&["record", "list"]);

    assert_output(
        &output,
        0,
        &format!("{} owner-review 1 reviewers - unaudited\n", short(&head)),
        "",
    );
}

// ---- record audit -------------------------------------------------------------------------

/// `record audit` sets `post_merge_audit` and changes no other byte of the
/// record (spec 8.3); the note still validates against its schema.
#[test]
fn audit_sets_the_post_merge_audit_and_no_other_byte() {
    let merges = three_merges();
    let before = merges.install.note_text(&merges.docs);

    let output = merges.install.run(&[
        "record",
        "audit",
        &merges.docs,
        "--verdict",
        "pass",
        "--by",
        "amy",
        "--note",
        "read it",
    ]);

    assert_output(&output, 0, "", "");
    let audit = json!({"verdict": "pass", "by": "amy", "note": "read it"});
    let expected = before.replacen(
        "\"post_merge_audit\":null",
        &format!("\"post_merge_audit\":{audit}"),
        1,
    );
    assert_ne!(expected, before);
    assert_eq!(merges.install.note_text(&merges.docs), expected);
    assert_valid(&merges.install.note(&merges.docs), RECORD_SCHEMA);
    let shown = merges.install.run(&["record", "show", &merges.docs]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    assert!(
        stdout(&shown).contains("pass by `amy`"),
        "{}",
        stdout(&shown)
    );
}

/// A verdict of `fail` and no note: the note is `null`.
#[test]
fn an_audit_with_no_note_holds_a_null_note() {
    let merges = three_merges();

    let output = merges.install.run(&[
        "record",
        "audit",
        &merges.docs,
        "--verdict",
        "fail",
        "--by",
        "bob",
    ]);

    assert_output(&output, 0, "", "");
    assert_eq!(
        merges.install.note(&merges.docs)["post_merge_audit"],
        json!({"verdict": "fail", "by": "bob", "note": null})
    );
}

/// The record that lists a commit is audited from that commit, and the audit
/// goes into the note of the key.
#[test]
fn a_commit_of_the_range_audits_the_record_that_lists_it() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let first = install.edit("docs/guide.md", BASE_TIME + 100);
    install.edit("docs/guide.md", BASE_TIME + 200);
    let head = install.edit("docs/guide.md", BASE_TIME + 300);
    install.write_record(&base, &head, &[]);

    let output = install.run(&[
        "record",
        "audit",
        &first,
        "--verdict",
        "pass",
        "--by",
        "amy",
    ]);

    assert_output(&output, 0, "", "");
    assert_eq!(
        install.note(&head)["post_merge_audit"],
        json!({"verdict": "pass", "by": "amy", "note": null})
    );
    let own_note = Command::new("git")
        .args(["notes", "--ref", TOP_REF, "show", &first])
        .current_dir(install.top.path())
        .output()
        .expect("run git");
    assert!(!own_note.status.success(), "the first commit has no note");
}

/// A second audit of the same record is exit 1, and the record stays.
#[test]
fn a_second_audit_exits_1_and_changes_nothing() {
    let merges = three_merges();
    let before = merges.install.note_text(&merges.ops);

    let output = merges.install.run(&[
        "record",
        "audit",
        &merges.ops,
        "--verdict",
        "fail",
        "--by",
        "bob",
    ]);

    assert_output(
        &output,
        1,
        "",
        &format!(
            "record: {} already holds a post-merge audit\n",
            short(&merges.ops)
        ),
    );
    assert_eq!(merges.install.note_text(&merges.ops), before);
}

/// `--by` is cut to 100 bytes and `--note` to 1,000 bytes of UTF-8, each on a
/// character boundary (spec 8.3).
#[test]
fn by_and_note_are_cut_on_a_character_boundary() {
    for (by, note, kept_by, kept_note) in [
        (
            "日".repeat(40),
            "😀".repeat(300),
            "日".repeat(33),
            "😀".repeat(250),
        ),
        (
            "é".repeat(60),
            "é".repeat(600),
            "é".repeat(50),
            "é".repeat(500),
        ),
        (
            "b".repeat(150),
            "n".repeat(1_500),
            "b".repeat(100),
            "n".repeat(1_000),
        ),
        (
            "a".repeat(100),
            "n".repeat(1_000),
            "a".repeat(100),
            "n".repeat(1_000),
        ),
    ] {
        let merges = three_merges();

        let output = merges.install.run(&[
            "record",
            "audit",
            &merges.docs,
            "--verdict",
            "pass",
            "--by",
            &by,
            "--note",
            &note,
        ]);

        assert_output(&output, 0, "", "");
        assert_eq!(
            merges.install.note(&merges.docs)["post_merge_audit"],
            json!({"verdict": "pass", "by": kept_by, "note": kept_note})
        );
    }
}

/// An empty `--note` leaves no note, and an empty `--by` is a setup error: an
/// audit with no name for the person is no audit.
#[test]
fn an_empty_note_is_no_note_and_an_empty_by_exits_2() {
    let merges = three_merges();

    let empty_by = merges.install.run(&[
        "record",
        "audit",
        &merges.docs,
        "--verdict",
        "pass",
        "--by",
        " ",
    ]);
    let empty_note = merges.install.run(&[
        "record",
        "audit",
        &merges.docs,
        "--verdict",
        "pass",
        "--by",
        "amy",
        "--note",
        "",
    ]);

    assert_output(&empty_by, 2, "", "record audit: --by needs a name\n");
    assert_output(&empty_note, 0, "", "");
    assert_eq!(
        merges.install.note(&merges.docs)["post_merge_audit"],
        json!({"verdict": "pass", "by": "amy", "note": null})
    );
}

/// The note of a record that a job other than `record write` edited keeps
/// every other byte: the GitLab job appends `reviewers_more` after
/// `truncated`, and a string may hold an escape that `serde_json` writes
/// another way (a DEL as `\u007f`).
#[test]
fn a_record_edited_by_the_gitlab_job_keeps_every_other_byte() {
    let merges = three_merges();
    let written = merges.install.note_text(&merges.docs);
    let edited = written
        .trim_end()
        .strip_suffix('}')
        .expect("a JSON object")
        .to_string()
        + ",\"reviewers_more\":5}\n";
    let edited = edited.replacen("houserules ", "houserules \\u007f ", 1);
    assert!(edited.contains("\\u007f"), "{edited}");
    merges.install.set_note_text(&merges.docs, &edited);

    let output = merges.install.run(&[
        "record",
        "audit",
        &merges.docs,
        "--verdict",
        "pass",
        "--by",
        "amy",
    ]);

    assert_output(&output, 0, "", "");
    let audit = json!({"verdict": "pass", "by": "amy", "note": null});
    assert_eq!(
        merges.install.note_text(&merges.docs),
        edited.replacen(
            "\"post_merge_audit\":null",
            &format!("\"post_merge_audit\":{audit}"),
            1
        )
    );
}

/// The `maxBytes` of the install bounds the audited note (spec 8.3): an audit
/// that would bring the record past it is exit 1 with the line, and the note
/// that fits it to the byte is written.
#[test]
fn an_audit_past_max_bytes_is_refused_with_its_line_and_one_to_the_byte_is_not() {
    let review = json!({"record": {"maxBytes": 8_192}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    let head = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &head, &[]);
    let written = install.note_text(&head);
    let audit = json!({"verdict": "pass", "by": "amy", "note": null}).to_string();
    let with_audit = written.len() - "null".len() + audit.len();
    let pad_to_the_limit = 8_192 - with_audit;
    let padded = |extra: usize| {
        written.replacen(
            "\"written_by\":\"",
            &format!("\"written_by\":\"{}", "p".repeat(pad_to_the_limit + extra)),
            1,
        )
    };
    let args = |head: &str| {
        vec![
            "record".to_string(),
            "audit".to_string(),
            head.to_string(),
            "--verdict".to_string(),
            "pass".to_string(),
            "--by".to_string(),
            "amy".to_string(),
        ]
    };
    let run = |args: &[String]| {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        install.run(&refs)
    };

    install.set_note_text(&head, &padded(1));
    let over = padded(1);
    let refused = run(&args(&head));
    assert_output(
        &refused,
        1,
        "",
        &format!(
            "record: the audit would bring {} to 8193 bytes > maxBytes 8192\n",
            short(&head)
        ),
    );
    assert_eq!(install.note_text(&head), over, "the record stays");

    install.set_note_text(&head, &padded(0));
    let fits = run(&args(&head));
    assert_output(&fits, 0, "", "");
    assert_eq!(install.note_text(&head).len(), 8_192);
}

/// A note of 1,000 control characters is 1,000 bytes of UTF-8 and 6,000 bytes
/// of JSON: the budget counts the bytes of the note.
#[test]
fn the_budget_counts_the_serialized_bytes_of_the_note() {
    let review = json!({"record": {"maxBytes": 8_192}});
    let install = Install::new("", &[], Some(review));
    let base = install.head();
    let head = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &head, &[]);
    let before = install.note_text(&head);
    let note = "\u{1}".repeat(1_000);

    let output = install.run(&[
        "record",
        "audit",
        &head,
        "--verdict",
        "pass",
        "--by",
        "amy",
        "--note",
        &note,
    ]);

    let audit = json!({"verdict": "pass", "by": "amy", "note": note}).to_string();
    let would_be = before.len() - "null".len() + audit.len();
    assert!(would_be > 8_192, "{would_be}");
    assert_output(
        &output,
        1,
        "",
        &format!(
            "record: the audit would bring {} to {would_be} bytes > maxBytes 8192\n",
            short(&head)
        ),
    );
    assert_eq!(install.note_text(&head), before);
}

/// A commit with no record is exit 1, a clone without the ref gets the line of
/// spec 8.1, and a sha that is no commit is exit 2.
#[test]
fn audit_names_a_commit_with_no_record_a_missing_ref_and_a_bad_sha() {
    let merges = three_merges();
    let bare = Install::new("", &[], None);

    let no_record = merges.install.run(&[
        "record",
        "audit",
        &merges.base,
        "--verdict",
        "pass",
        "--by",
        "amy",
    ]);
    let no_ref = bare.run(&[
        "record",
        "audit",
        "HEAD",
        "--verdict",
        "pass",
        "--by",
        "amy",
    ]);
    let bad_sha = merges.install.run(&[
        "record",
        "audit",
        "nope",
        "--verdict",
        "pass",
        "--by",
        "amy",
    ]);

    assert_output(
        &no_record,
        1,
        "",
        &format!("record: no record for {}\n", short(&merges.base)),
    );
    assert_output(&no_ref, 1, "", FETCH_LINE);
    assert_output(&bad_sha, 2, "", "bad ref \"nope\"\n");
}

/// A note that is no change record is one named line, exit 2, and the note
/// stays.
#[test]
fn audit_of_a_note_that_is_no_change_record_exits_2() {
    let merges = three_merges();
    merges.install.set_note_text(&merges.ops, "not json\n");

    let output = merges.install.run(&[
        "record",
        "audit",
        &merges.ops,
        "--verdict",
        "pass",
        "--by",
        "amy",
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).starts_with(&format!(
            "record: {}: not a change record (",
            short(&merges.ops)
        )),
        "{}",
        stderr(&output)
    );
    assert_eq!(merges.install.note_text(&merges.ops), "not json\n");
}

/// The verdict is `pass` or `fail`, and `--verdict` and `--by` are required:
/// clap stops the command with exit 2.
#[test]
fn audit_requires_a_verdict_of_pass_or_fail_and_a_name() {
    let merges = three_merges();

    let bad_verdict = merges.install.run(&[
        "record",
        "audit",
        &merges.docs,
        "--verdict",
        "maybe",
        "--by",
        "amy",
    ]);
    let no_verdict = merges
        .install
        .run(&["record", "audit", &merges.docs, "--by", "amy"]);
    let no_name = merges
        .install
        .run(&["record", "audit", &merges.docs, "--verdict", "pass"]);

    assert_eq!(bad_verdict.status.code(), Some(2));
    assert!(
        stderr(&bad_verdict).contains("[possible values: pass, fail]"),
        "{}",
        stderr(&bad_verdict)
    );
    assert_eq!(no_verdict.status.code(), Some(2));
    assert!(
        stderr(&no_verdict).contains("--verdict"),
        "{}",
        stderr(&no_verdict)
    );
    assert_eq!(no_name.status.code(), Some(2));
    assert!(stderr(&no_name).contains("--by"), "{}", stderr(&no_name));
}

/// A subdirectory install audits the record in its own notes ref.
#[test]
fn a_subdirectory_install_audits_its_own_ref() {
    let install = Install::new("sub", &[], None);
    let base = install.head();
    let head = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &head, &[]);

    let output = install.run(&["record", "audit", &head, "--verdict", "pass", "--by", "amy"]);

    assert_output(&output, 0, "", "");
    assert_eq!(
        install.note(&head)["post_merge_audit"],
        json!({"verdict": "pass", "by": "amy", "note": null})
    );
}

/// `command` run with no git identity and in the C locale: the system and global
/// configuration are the null device, no variable injects configuration or names
/// an identity (`houserules.pinned-git-text-runs-hermetic`).
fn run_without_identity(mut command: Command) -> Output {
    with_default_git(&mut command);
    for name in [
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        "EMAIL",
    ] {
        command.env_remove(name);
    }
    command.output().expect("run houserules")
}

/// A failed write of the note is one line that names the failure: the `fatal:`
/// line of git. `git notes add -f` over a note that exists prints the notice
/// `Overwriting existing notes for object <sha>` before its error, and both
/// `record audit` and a re-run of `record write` always overwrite. The
/// repository holds no identity for the commit of the notes ref, and
/// `user.useConfigOnly` keeps git from guessing one from the host, so the line
/// is the same on every host. The note stays as it was.
#[test]
fn a_failed_note_write_prints_the_fatal_line_of_git_and_not_its_notice() {
    let merges = three_merges();
    let top = merges.install.top.path();
    git(top, &["config", "--unset", "user.name"]);
    git(top, &["config", "--unset", "user.email"]);
    git(top, &["config", "user.useConfigOnly", "true"]);
    let before = merges.install.note_text(&merges.docs);
    let line = "fatal: no email was given and auto-detection is disabled\n";

    let mut audit = houserules();
    audit
        .args(["record", "audit", &merges.docs, "--verdict", "pass", "--by"])
        .arg("amy")
        .arg("--dir")
        .arg(merges.install.root());
    let audited = run_without_identity(audit);
    let mut rewrite = houserules();
    rewrite
        .args(["record", "write", "--base", &merges.base, "--head"])
        .arg(&merges.docs)
        .arg("--dir")
        .arg(merges.install.root());
    let rewritten = run_without_identity(rewrite);

    assert_output(&audited, 2, "", line);
    assert_output(&rewritten, 2, "", line);
    assert_eq!(merges.install.note_text(&merges.docs), before);
}

// ---- stats --records ---------------------------------------------------------------------

/// The count of one route or one area: its records and those with a reviewer.
fn count(records: u64, with_reviewers: u64) -> Value {
    json!({"records": records, "with_reviewers": with_reviewers})
}

/// The `review_coverage` of the three merges (spec 8.3): three records, two with
/// a reviewer, one count per route, one per area, one sampled record, and two
/// audits.
fn coverage_of_three_merges() -> Value {
    json!({
        "records": 3,
        "with_reviewers": 2,
        "routes": {
            "auto-merge-eligible": count(1, 1),
            "owner-review": count(1, 1),
            "senior-review": count(1, 0),
        },
        "areas": {
            "code": count(1, 1),
            "docs": count(1, 1),
            "ops": count(1, 0),
        },
        "without_area_data": 0,
        "sampled": 1,
        "audited": {"count": 2, "pass": 1, "fail": 1},
    })
}

/// Without workspaces, `stats --records <range>` prints only the key
/// `review_coverage`, in the order of spec 8.3.
#[test]
fn stats_records_without_workspaces_prints_only_review_coverage() {
    let merges = three_merges();
    let range = format!("{}..{}", merges.base, merges.code);

    let output = merges.install.run(&["stats", "--records", &range]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stderr(&output), "");
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(
        printed,
        json!({"review_coverage": coverage_of_three_merges()})
    );
    let keys: Vec<&str> = printed["review_coverage"]
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "records",
            "with_reviewers",
            "routes",
            "areas",
            "without_area_data",
            "sampled",
            "audited"
        ]
    );
    let again = merges.install.run(&["stats", "--records", &range]);
    assert_eq!(again.stdout, output.stdout, "two runs print the same bytes");
}

/// With a workspace, the key follows the keys of `stats`.
#[test]
fn stats_records_with_a_workspace_adds_the_key_after_the_others() {
    let merges = three_merges();
    let workspace = merges.install.scratch_file("2026-10-01-batch-1");
    fs::create_dir_all(&workspace).expect("create the workspace");
    let range = format!("{}..{}", merges.base, merges.code);

    let output = merges.install.run(&[
        "stats",
        workspace.to_str().expect("UTF-8"),
        "--records",
        &range,
    ]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    let keys: Vec<&str> = printed
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys.last(), Some(&"review_coverage"));
    assert!(keys.contains(&"violations"), "{keys:?}");
    assert_eq!(printed["review_coverage"], coverage_of_three_merges());
}

/// The range decides which records count.
#[test]
fn stats_records_counts_the_records_of_the_range() {
    let merges = three_merges();
    let range = format!("{}..{}", merges.docs, merges.code);

    let output = merges.install.run(&["stats", "--records", &range]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(
        printed["review_coverage"],
        json!({
            "records": 2,
            "with_reviewers": 1,
            "routes": {
                "auto-merge-eligible": count(0, 0),
                "owner-review": count(1, 1),
                "senior-review": count(1, 0),
            },
            "areas": {"code": count(1, 1), "ops": count(1, 0)},
            "without_area_data": 0,
            "sampled": 0,
            "audited": {"count": 2, "pass": 1, "fail": 1},
        })
    );
}

/// A record in the minimal form holds no area, so it counts in `records` and in
/// its route and in `without_area_data`, and in no area.
#[test]
fn a_record_in_the_minimal_form_counts_in_no_area() {
    let merges = three_merges();
    let mut record = merges.install.note(&merges.code);
    minimal_form(&mut record);
    merges.install.set_note(&merges.code, &record);
    let range = format!("{}..{}", merges.ops, merges.code);

    let output = merges.install.run(&["stats", "--records", &range]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(
        printed["review_coverage"],
        json!({
            "records": 1,
            "with_reviewers": 1,
            "routes": {
                "auto-merge-eligible": count(0, 0),
                "owner-review": count(1, 1),
                "senior-review": count(0, 0),
            },
            "areas": {},
            "without_area_data": 1,
            "sampled": 0,
            "audited": {"count": 1, "pass": 0, "fail": 1},
        })
    );
}

/// A clone without the notes ref gets the line of spec 8.1 and exit 2, the
/// only error exit of `stats`; a range that git does not read is exit 2 too.
#[test]
fn stats_records_names_a_missing_ref_and_a_bad_range() {
    let bare = Install::new("", &[], None);
    let merges = three_merges();

    let no_ref = bare.run(&["stats", "--records", "HEAD"]);
    let bad_range = merges.install.run(&["stats", "--records", "nope..HEAD"]);

    assert_output(&no_ref, 2, "", FETCH_LINE);
    assert_output(&bad_range, 2, "", "bad range \"nope..HEAD\"\n");
}

/// Pin of the unchanged rule: `stats` with neither a workspace nor `--records`
/// still needs a workspace.
#[test]
fn stats_without_a_workspace_and_without_records_still_needs_a_workspace() {
    let merges = three_merges();

    let output = merges.install.run(&["stats"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("<WORKSPACE>"),
        "{}",
        stderr(&output)
    );
}

// ---- the warn line for an embedded Reversal: (spec 11.6) -------------------------------------

/// A repository root that holds a loadable knowledge base, as `stats` needs.
fn knowledge_root() -> TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    write_knowledge_base(root.path());
    root
}

/// A workspace directory `name` below `parent` whose ledger holds `ledger`.
fn workspace_with_ledger(parent: &Path, name: &str, ledger: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).expect("create the workspace");
    fs::write(dir.join("progress.md"), ledger).expect("write the ledger");
    dir
}

/// Runs `houserules stats <workspaces> --dir <root>`.
fn stats_of(root: &Path, workspaces: &[PathBuf]) -> Output {
    houserules()
        .arg("stats")
        .args(workspaces)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run stats")
}

/// The warn line of the ledger line `line` of the ledger below `workspace`.
fn warn_line(workspace: &Path, line: usize) -> String {
    format!(
        "{}:{line}: {EMBEDDED_REVERSAL}\n",
        workspace.join("progress.md").display()
    )
}

/// Spec 11.6: one warn line on stderr for each ledger line that holds
/// `Reversal:` after its start, and the count of design.md 226(2) stays 5 (the
/// lines 3 to 7 of the committed ledger).
#[test]
fn stats_warns_for_each_reversal_inside_a_longer_ledger_line() {
    let root = knowledge_root();
    let workspace = fixture("reversals");

    let output = stats_of(root.path(), std::slice::from_ref(&workspace));

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(printed["workspaces"][0]["reversals"], json!(5));
    let expected: String = [8, 9, 10]
        .iter()
        .map(|line| warn_line(&workspace, *line))
        .collect();
    assert_eq!(stderr(&output), expected);
}

/// A line that starts with `Reversal:`, a lowercase word, and a word with no
/// colon give no warning; a ledger with CRLF line ends warns by the same rule.
#[test]
fn stats_warns_for_no_line_that_counts_or_that_holds_no_reversal() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let quiet = workspace_with_ledger(
        scratch.path(),
        "2026-10-01-batch-1",
        "Reversal: a, and Reversal: b\r\n- Reversal: c\r\nreversal: lower\r\nReversal without a colon\r\n",
    );
    let loud = workspace_with_ledger(
        scratch.path(),
        "2026-10-02-batch-2",
        "Reversal: a\r\nTask 4 had a Reversal: late\r\n",
    );

    let output = stats_of(root.path(), &[quiet.clone(), loud.clone()]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let printed: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(printed["workspaces"][0]["reversals"], json!(2));
    assert_eq!(printed["workspaces"][1]["reversals"], json!(1));
    assert_eq!(stderr(&output), warn_line(&loud, 2));
}

/// The warnings follow the order of the workspaces, and the lines of one
/// ledger follow its own order.
#[test]
fn the_warnings_follow_the_order_of_the_workspaces_and_of_the_lines() {
    let root = knowledge_root();
    let scratch = tempfile::tempdir().expect("tempdir");
    let second = workspace_with_ledger(
        scratch.path(),
        "2026-10-02-batch-2",
        "see Reversal: x\nplain\nand Reversal: y\n",
    );
    let first = workspace_with_ledger(
        scratch.path(),
        "2026-10-01-batch-1",
        "ok\nthe Reversal: z\n",
    );

    let output = stats_of(root.path(), &[second.clone(), first.clone()]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(
        stderr(&output),
        format!(
            "{}{}{}",
            warn_line(&second, 1),
            warn_line(&second, 3),
            warn_line(&first, 2)
        )
    );
}

// ---- the help ------------------------------------------------------------------------------

/// The sentences that `record --help` and `houserules --help` add to the
/// `about` of `record`.
const RECORD_ABOUT: [&str; 2] = [
    "`record list` lists the records of a range, one line for each.",
    "`record audit` sets the post-merge audit of a record",
];

/// The sentences of the `about` of `record list`.
const LIST_ABOUT: [&str; 12] = [
    "Lists the change records of the install, one line for each record: `<key7> <route> <n> reviewers <sampled|-> <pass|fail|unaudited>`.",
    "The lines run by the committer date of the key of the record, newest first, and by sha for a tie.",
    "A record whose key the clone lacks has no date and comes last.",
    "`<n>` counts the reviewers of the record, those left out of its list included.",
    "`sampled` marks a record whose draw selected it for a post-merge audit, and the last word gives the verdict of that audit, `unaudited` when there is none.",
    "`<range>` keeps the records whose key is in the range, as `git rev-list` reads it; without it the command lists every record.",
    "`--sampled` keeps the sampled records.",
    "`--json` prints a list of one object for each record in place of the lines: `key` (the full sha), `route`, `reviewers`, `sampled` (`true` or `false`), and `audit` (`pass`, `fail`, or `unaudited`).",
    "A range reaches past the history of a shallow clone when the commits of one side that the other side does not reach (for a range of one revision, the commits it reaches) include a shallow commit, when an end is a full object name that the clone lacks, or when an end is a relative revision (`<rev>~<n>` or `<rev>^<n>`) that does not resolve while its `<rev>` does.",
    "A range with any other end that git cannot resolve, an abbreviated object name included, is a range that git cannot read.",
    "It exits 1 for a clone with no notes ref.",
    "It exits 2 for a range that git cannot read, for a range that reaches past the history of a shallow clone, and for a note that is no change record",
];

/// The help text of each argument of `record list`.
const LIST_ARGUMENTS: [&str; 4] = [
    "A git revision range, such as `v1.0..main`. The command keeps the records whose key is in the range. Without it, the command lists every record",
    "Keeps the records whose post-merge sample draw selected them",
    "Prints a list of objects in place of the lines",
    "Install root to read from; defaults to the nearest directory at or above the current one that holds `.houserules.json`, no higher than the git top level, else the git top level",
];

/// The sentences of the `about` of `record audit`.
const AUDIT_ABOUT: [&str; 6] = [
    "Sets the post-merge audit of a record: the record that the commit `<sha>` keys, else the record whose range holds the commit.",
    "`--verdict` is `pass` or `fail`, `--by` names the person who audited, and `--note` adds a text.",
    "The command cuts `--by` to 100 bytes and `--note` to 1,000 bytes of UTF-8, each on a character boundary, and an empty `--note` leaves no note.",
    "It sets the audit in the notes ref of the clone and changes no other byte of the record; the project pushes the ref.",
    "It exits 1 for a clone with no notes ref, for a commit with no record, for a record that holds an audit already, and for an audit that would bring the record past `review.record.maxBytes`.",
    "It exits 2 for a setup error, for example: a ref that is no commit, an empty `--by`, or a note that is no change record",
];

/// The help text of each argument of `record audit`.
const AUDIT_ARGUMENTS: [&str; 5] = [
    "The commit to audit: a commit that a record keys, or one in the range of a record",
    "The verdict of the audit: `pass` or `fail`",
    "The name of the person who audited. The command cuts it to 100 bytes",
    "A note on the audit. The command cuts it to 1,000 bytes of UTF-8",
    "Install root to read from; defaults to the nearest directory at or above the current one that holds `.houserules.json`, no higher than the git top level, else the git top level",
];

/// The sentences that `stats --help` adds for `--records` and the warn line.
const STATS_ABOUT: [&str; 4] = [
    "`--records <range>` adds the key `review_coverage`: the change records whose key is in the range, those with a reviewer, both counts for each route and each area, the sampled records, and the audited records with their verdicts.",
    "Without a workspace, `stats` then prints only that key.",
    "`stats` counts a ledger line as a reversal when it starts with `Reversal:` after optional spaces and an optional list marker (`- `, `* `, or `+ `).",
    "For each other ledger line that holds `Reversal:`, the command prints one warning on stderr",
];

/// The help text of the arguments of `stats` that `--records` changes.
const STATS_ARGUMENTS: [&str; 2] = [
    "One or more workspace directories to aggregate. Required unless `--records` is given",
    "A git revision range, such as `v1.0..main`. Adds the key `review_coverage` for the change records whose key is in the range, and exits 2 for a clone with no notes ref, for a range that git cannot read, and for a range that reaches past the history of a shallow clone. A range reaches past the history of a shallow clone when the commits of one side that the other side does not reach (for a range of one revision, the commits it reaches) include a shallow commit, when an end is a full object name that the clone lacks, or when an end is a relative revision (`<rev>~<n>` or `<rev>^<n>`) that does not resolve while its `<rev>` does. A range with any other end that git cannot resolve, an abbreviated object name included, is a range that git cannot read",
];

/// The `--help` texts state the new subcommands, their exits, and each
/// condition of an argument (`quality.user-facing-text-is-contract`).
#[test]
fn the_help_states_record_list_record_audit_and_stats_records() {
    states(
        "record --help",
        &help_of_args(&["record", "--help"]),
        &RECORD_ABOUT,
    );
    states(
        "houserules --help",
        &help_of_args(&["--help"]),
        &RECORD_ABOUT,
    );
    let list = help_of_args(&["record", "list", "--help"]);
    states("record list --help", &list, &LIST_ABOUT);
    states("record list --help", &list, &LIST_ARGUMENTS);
    let audit = help_of_args(&["record", "audit", "--help"]);
    states("record audit --help", &audit, &AUDIT_ABOUT);
    states("record audit --help", &audit, &AUDIT_ARGUMENTS);
    let stats = help_of_args(&["stats", "--help"]);
    states("stats --help", &stats, &STATS_ABOUT);
    states("stats --help", &stats, &STATS_ARGUMENTS);
}

// ---- the cost of a search -------------------------------------------------------------------

/// The git processes that `houserules <args> --dir <root>` started in the
/// install, counted by command name as the trace2 events of git report them.
/// The command exits 0.
fn git_processes_of(install: &Install, args: &[&str]) -> BTreeMap<String, usize> {
    git_processes_in(
        &install.root(),
        &install.scratch_file("trace2.json"),
        args,
        0,
    )
}

/// The git processes that `houserules <args> --dir <root>` started, counted by
/// command name as the trace2 events of git report them, written to the file
/// `trace`. The command exits with `code`.
fn git_processes_in(
    root: &Path,
    trace: &Path,
    args: &[&str],
    code: i32,
) -> BTreeMap<String, usize> {
    let _ = fs::remove_file(trace);
    let output = houserules()
        .env("GIT_TRACE2_EVENT", trace)
        .args(args)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run houserules");
    assert_eq!(output.status.code(), Some(code), "{}", stderr(&output));
    let mut counts = BTreeMap::new();
    for line in fs::read_to_string(trace).expect("the trace").lines() {
        let event: Value = serde_json::from_str(line).expect("a trace2 event");
        if event["event"] == json!("cmd_name") {
            let name = event["name"].as_str().expect("a command name");
            *counts.entry(name.to_string()).or_default() += 1;
        }
    }
    counts
}

/// An install whose notes ref holds `count` records, one for each of `count`
/// commits, and the key of the last one.
fn install_with_records(count: usize) -> (Install, String) {
    let install = Install::new("", &[], None);
    let base = install.head();
    let first = install.edit("docs/guide.md", BASE_TIME + 100);
    install.write_record(&base, &first, &[]);
    let record = install.note(&first);
    let mut last = first;
    for n in 1..count {
        last = install.edit("docs/guide.md", BASE_TIME + 100 + n as u64);
        let mut copy = record.clone();
        copy["key"] = json!(last);
        copy["head"] = json!(last);
        install.set_note(&last, &copy);
    }
    (install, last)
}

/// A listing, the coverage of a range, and an audit start the same git
/// processes for 4 records as for 24: the cost of a search does not grow with
/// the number of records. The listing reads all the notes with one `git
/// cat-file`.
#[test]
fn a_listing_starts_a_fixed_number_of_git_processes_whatever_the_number_of_records() {
    let (small, small_key) = install_with_records(4);
    let (large, large_key) = install_with_records(24);
    let audit = |key: &str| {
        vec![
            "record".to_string(),
            "audit".to_string(),
            key.to_string(),
            "--verdict".to_string(),
            "pass".to_string(),
            "--by".to_string(),
            "amy".to_string(),
        ]
    };
    for args in [
        vec!["record".to_string(), "list".to_string()],
        vec![
            "record".to_string(),
            "list".to_string(),
            "--sampled".to_string(),
        ],
        vec!["record".to_string(), "list".to_string(), "HEAD".to_string()],
        vec![
            "stats".to_string(),
            "--records".to_string(),
            "HEAD".to_string(),
        ],
    ] {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();

        let few = git_processes_of(&small, &refs);
        let many = git_processes_of(&large, &refs);

        assert_eq!(few, many, "{args:?}");
        assert_eq!(few.get("cat-file"), Some(&1), "{args:?}: {few:?}");
    }
    let (few, many) = (audit(&small_key), audit(&large_key));
    let few: Vec<&str> = few.iter().map(String::as_str).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    assert_eq!(
        git_processes_of(&small, &few),
        git_processes_of(&large, &many),
        "record audit"
    );
}

/// A range that reaches past a shallow clone, by its walk, by a missing end, by a
/// relative revision that the clone lacks, or by a shallow commit on its excluded
/// side, starts the same git processes for 4 records as for 24, and none reads a
/// note: the search ends at the range. A missing end adds the one `git cat-file`
/// that tells which end the clone lacks, and a walk adds none.
#[test]
fn a_range_past_a_shallow_clone_starts_a_fixed_number_of_git_processes() {
    let (small, _) = install_with_records(4);
    let (large, _) = install_with_records(24);
    let parent_of_head = |install: &Install| {
        git(install.top.path(), &["rev-parse", "HEAD~1"])
            .trim()
            .to_string()
    };
    let (_small_holder, small_clone) = shallow_clone_of(&small, 2);
    let (_large_holder, large_clone) = shallow_clone_of(&large, 2);
    let (_small_side_holder, small_side) =
        shallow_clone_with_a_shallow_side(&small, &parent_of_head(&small));
    let (_large_side_holder, large_side) =
        shallow_clone_with_a_shallow_side(&large, &parent_of_head(&large));
    let missing_end = |install: &Install| {
        format!(
            "{}..HEAD",
            git(install.top.path(), &["rev-parse", "HEAD~3"]).trim()
        )
    };
    let trace = |clone: &Path, range: &str| {
        let holder = tempfile::tempdir().expect("tempdir");
        git_processes_in(
            clone,
            &holder.path().join("trace2.json"),
            &["record", "list", range],
            2,
        )
    };

    for (name, in_the_side_clone, range, cat_files) in [
        (
            "a walk that lists the shallow commit",
            false,
            Some("HEAD"),
            None,
        ),
        ("an end that the clone lacks", false, None, Some(1)),
        (
            "a relative end that the clone lacks",
            false,
            Some("HEAD~5..HEAD"),
            Some(1),
        ),
        (
            "a shallow commit on the excluded side",
            true,
            Some("origin/side..main"),
            None,
        ),
    ] {
        let range_in = |install: &Install| range.map_or_else(|| missing_end(install), String::from);
        let (few_clone, many_clone) = if in_the_side_clone {
            (&small_side, &large_side)
        } else {
            (&small_clone, &large_clone)
        };

        let few = trace(few_clone, &range_in(&small));
        let many = trace(many_clone, &range_in(&large));

        assert_eq!(few, many, "{name}");
        assert_eq!(few.get("cat-file"), cat_files.as_ref(), "{name}: {few:?}");
        assert_eq!(few.get("rev-list"), Some(&1), "{name}: {few:?}");
    }
}

// ---- a shallow clone -------------------------------------------------------------------------

/// A clone of `install` that holds the newest `depth` commits of `main` and the
/// notes ref of the install, and the directory that holds it. A shallow clone is
/// made with a `file://` URL, because git ignores `--depth` for a plain local
/// path.
fn shallow_clone_of(install: &Install, depth: u32) -> (TempDir, PathBuf) {
    let remote = tempfile::tempdir().expect("tempdir");
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    let remote_url = remote.path().to_str().expect("a UTF-8 path").to_string();
    git(
        install.top.path(),
        &["push", "-q", &remote_url, "HEAD:refs/heads/main"],
    );
    git(
        install.top.path(),
        &["push", "-q", &remote_url, &format!("{TOP_REF}:{TOP_REF}")],
    );
    let holder = tempfile::tempdir().expect("tempdir");
    let url = url::Url::from_file_path(remote.path()).expect("a file URL");
    git(
        holder.path(),
        &[
            "clone",
            "-q",
            "--depth",
            &depth.to_string(),
            url.as_str(),
            "clone",
        ],
    );
    let clone = holder.path().join("clone");
    git(
        &clone,
        &["fetch", "-q", "origin", &format!("{TOP_REF}:{TOP_REF}")],
    );
    (holder, clone)
}

/// Runs `houserules record list <args> --dir <root>`.
fn list_in(root: &Path, args: &[&str]) -> Output {
    houserules()
        .args(["record", "list"])
        .args(args)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run record list")
}

/// Runs `houserules stats --records <range> --dir <root>`.
fn stats_records_in(root: &Path, range: &str) -> Output {
    houserules()
        .args(["stats", "--records", range])
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run stats --records")
}

/// The line of a range that a shallow clone cannot walk to its end (spec 8.3),
/// after the `record: ` prefix of the record reader.
fn past_the_history_line(range: &str) -> String {
    format!(
        "record: {range} reaches past the history of this clone; fetch the full history to read its records\n"
    )
}

/// The clone of the four commits of [`three_merges`] with the newest two: it holds
/// `code` and `ops`, and `ops` is its shallow commit. The commits `docs` and
/// `base` are past the boundary.
fn shallow_merges() -> (Merges, TempDir, PathBuf) {
    let merges = three_merges();
    let (holder, clone) = shallow_clone_of(&merges.install, 2);
    (merges, holder, clone)
}

/// A clone of `install` whose `main` holds the whole history, and whose branch
/// `origin/side` is fetched with depth 1. The branch `side` forks from `fork` and
/// holds one more commit, which `main` does not reach: the clone has one shallow
/// commit, the tip of `side`, and the history of `main` is complete. Gives the
/// directory that holds the clone.
fn shallow_clone_with_a_shallow_side(install: &Install, fork: &str) -> (TempDir, PathBuf) {
    let top = install.top.path();
    git(top, &["branch", "side", fork]);
    git(top, &["switch", "-q", "side"]);
    install.write("side.txt", "side\n");
    install.commit_at("side", BASE_TIME + 150);
    git(top, &["switch", "-q", "-"]);
    let remote = tempfile::tempdir().expect("tempdir");
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    let remote_url = remote.path().to_str().expect("a UTF-8 path").to_string();
    git(
        top,
        &[
            "push",
            "-q",
            &remote_url,
            "HEAD:refs/heads/main",
            "side:refs/heads/side",
            &format!("{TOP_REF}:{TOP_REF}"),
        ],
    );
    let holder = tempfile::tempdir().expect("tempdir");
    let url = url::Url::from_file_path(remote.path()).expect("a file URL");
    git(
        holder.path(),
        &[
            "clone",
            "-q",
            "--single-branch",
            "--branch",
            "main",
            url.as_str(),
            "clone",
        ],
    );
    let clone = holder.path().join("clone");
    git(
        &clone,
        &["fetch", "-q", "origin", &format!("{TOP_REF}:{TOP_REF}")],
    );
    git(
        &clone,
        &[
            "fetch",
            "-q",
            "--depth",
            "1",
            "origin",
            "side:refs/remotes/origin/side",
        ],
    );
    (holder, clone)
}

/// In a shallow clone, a range whose end the clone lacks is named, for `record
/// list` and for `stats --records`: exit 2 with the line of spec 8.3. The range
/// is valid in the full repository, and git itself calls it an invalid range.
/// Every spelling that names the missing commit by its full sha is covered: a
/// range of two dots, a symmetric difference, the commit alone, and the missing
/// commit as the end.
#[test]
fn a_range_whose_end_a_shallow_clone_lacks_exits_2_with_the_line_of_the_spec() {
    let (merges, _holder, clone) = shallow_merges();
    let ranges = [
        format!("{}..HEAD", merges.docs),
        format!("{}...HEAD", merges.base),
        format!("HEAD..{}", merges.docs),
        merges.docs.clone(),
    ];

    for range in &ranges {
        let line = past_the_history_line(range);

        assert_output(&list_in(&clone, &[range]), 2, "", &line);
        assert_output(&stats_records_in(&clone, range), 2, "", &line);
    }
}

/// In a shallow clone, a range whose walk reaches the shallow commit is named
/// and not counted short: the clone cannot tell which records the missing
/// history holds, so `stats --records HEAD` does not count the two records of the
/// clone in silence.
#[test]
fn a_range_whose_walk_reaches_a_shallow_commit_exits_2_with_the_line_of_the_spec() {
    let (merges, _holder, clone) = shallow_merges();
    let ranges = [
        "HEAD".to_string(),
        merges.code.clone(),
        merges.ops.clone(),
        format!("{}~0", merges.ops),
    ];

    for range in &ranges {
        let line = past_the_history_line(range);

        assert_output(&list_in(&clone, &[range]), 2, "", &line);
        assert_output(&stats_records_in(&clone, range), 2, "", &line);
    }
}

/// In a shallow clone, a range whose end is a relative revision that the clone
/// lacks, while the revision it counts from resolves, is named like a range
/// whose end is a full sha that the clone lacks: `HEAD~2` is the parent of the
/// shallow commit `ops`. The full repository lists the same ranges.
#[test]
fn a_range_whose_end_is_a_relative_revision_the_clone_lacks_exits_2_with_the_line_of_the_spec() {
    let (merges, _holder, clone) = shallow_merges();
    let ranges = [
        "HEAD~2..HEAD",
        "HEAD~5..HEAD",
        "HEAD^^..HEAD",
        "HEAD..HEAD~2",
        "HEAD~2...HEAD",
        "HEAD~2",
    ];

    for range in ranges {
        let line = past_the_history_line(range);

        assert_output(&list_in(&clone, &[range]), 2, "", &line);
        assert_output(&stats_records_in(&clone, range), 2, "", &line);
    }
    let in_full = merges.install.run(&["record", "list", "HEAD~2..HEAD"]);
    assert_output(
        &in_full,
        0,
        &list_lines(&[
            (&merges.code, "owner-review", 1, "-", "fail"),
            (&merges.ops, "senior-review", 0, "-", "pass"),
        ]),
        "",
    );
}

/// In a shallow clone whose history of `main` is complete, a range whose
/// excluded side reaches a shallow commit is named and not counted wrong in
/// silence: `side` forks from `docs`, and the clone holds its tip alone, so the
/// walk of `origin/side..main` would list the four commits of `main` where the
/// full repository lists `ops` and `code`. A range that does not reach the
/// shallow commit is listed.
#[test]
fn a_range_whose_excluded_side_reaches_a_shallow_commit_exits_2_with_the_line_of_the_spec() {
    let merges = three_merges();
    let (_holder, clone) = shallow_clone_with_a_shallow_side(&merges.install, &merges.docs);
    let in_range = list_lines(&[
        (&merges.code, "owner-review", 1, "-", "fail"),
        (&merges.ops, "senior-review", 0, "-", "pass"),
    ]);

    for range in [
        "origin/side..main",
        "origin/side...main",
        "main..origin/side",
        "origin/side",
    ] {
        let line = past_the_history_line(range);

        assert_output(&list_in(&clone, &[range]), 2, "", &line);
        assert_output(&stats_records_in(&clone, range), 2, "", &line);
    }
    assert_output(
        &merges.install.run(&["record", "list", "side..HEAD"]),
        0,
        &in_range,
        "",
    );
    let held = list_in(&clone, &["main~2..main"]);
    let counted = stats_records_in(&clone, "main~2..main");
    assert_output(&held, 0, &in_range, "");
    assert_eq!(counted.status.code(), Some(0), "{}", stderr(&counted));
    let printed: Value = serde_json::from_str(&stdout(&counted)).expect("JSON");
    assert_eq!(printed["review_coverage"]["records"], json!(2));
}

/// Pins of the rest of the rule: a range whose walk stops above the shallow
/// commit is listed and counted; a range with an end that git cannot resolve,
/// and that is neither a full sha nor a relative revision of a revision that
/// resolves, stays `bad range` in a shallow clone, an abbreviation of a missing
/// commit and an end that is a typo beside a missing end included; and a commit
/// that a full repository lacks is a bad range there, because only a shallow
/// clone can lack a commit that exists.
#[test]
fn a_shallow_clone_lists_a_range_it_holds_and_keeps_the_bad_range_line() {
    let (merges, _holder, clone) = shallow_merges();
    let inside = format!("{}..{}", merges.ops, merges.code);
    let abbreviated = format!("{}..HEAD", &merges.docs[..10]);
    let absent = format!("{}..HEAD", "0".repeat(40));

    let listed = list_in(&clone, &[&inside]);
    let counted = stats_records_in(&clone, &inside);
    let nothing = list_in(&clone, &["HEAD..HEAD"]);

    assert_output(
        &listed,
        0,
        &format!("{} owner-review 1 reviewers - fail\n", short(&merges.code)),
        "",
    );
    assert_eq!(counted.status.code(), Some(0), "{}", stderr(&counted));
    let printed: Value = serde_json::from_str(&stdout(&counted)).expect("JSON");
    assert_eq!(printed["review_coverage"]["records"], json!(1));
    assert_eq!(stderr(&counted), "");
    assert_output(&nothing, 0, "", "");
    let typo_beside_a_missing_end = format!("nope..{}", merges.docs);
    for range in [
        "nope..HEAD",
        abbreviated.as_str(),
        typo_beside_a_missing_end.as_str(),
    ] {
        let bad = format!("bad range \"{range}\"\n");

        assert_output(&list_in(&clone, &[range]), 2, "", &bad);
        assert_output(&stats_records_in(&clone, range), 2, "", &bad);
    }
    let bad = format!("bad range \"{absent}\"\n");
    assert_output(
        &merges.install.run(&["record", "list", &absent]),
        2,
        "",
        &bad,
    );
    assert_output(
        &merges.install.run(&["stats", "--records", &absent]),
        2,
        "",
        &bad,
    );
}
