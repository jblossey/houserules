//! `houserules record write` and `houserules record show` (batch 37, spec 8.1
//! to 8.5; acceptance case 5, the write and show parts; the last sentence of
//! case 4; HR-192). Each case runs the compiled binary against a scratch
//! repository whose install holds the areas `docs`, `code`, and `ops`, and
//! reads the note or the printed text whole. A line of the spec is the literal
//! of a test.
//!
//! Every fixture is hermetic: the install is written by hand, so the audit that
//! `record write` runs in process reads only the entries that a case lists. A
//! scratch repository holds its own `user.name` and `user.email`, because the
//! command commits to the notes ref; no test reads a global git identity.

mod ci_support;
mod help_support;
mod note_support;
mod schema_support;
mod tracker_support;
mod validation_support;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use help_support::{help_of_args, states};
use schema_support::assert_valid;
use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    assert_output, commit_all, copy_dir, edit_marker, git, houserules, repo_root, scratch_git_repo,
    sha256_hex, stderr, stdout,
};
use validation_support::{
    arg, clean_capture, document_of, item, openspec_review, refused_captures, validation,
    validations,
};

/// The schema file of the change record, relative to the repository root.
const RECORD_SCHEMA: &str = "schemas/change-record/1.1.0.json";

/// The name of git's empty tree for a repository that hashes with SHA-1.
const EMPTY_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// The notes ref of a top-level install (spec 8.1).
const TOP_REF: &str = "refs/notes/houserules";

/// The line that follows the items of the uncovered surface (spec 5.4).
const COVERAGE_LINE: &str =
    "Coverage is per file: a file is covered when a check that declares covers: true read it.";

/// The keys of a record in its full form, in the order of spec 8.2.
const RECORD_KEYS: [&str; 21] = [
    "schema",
    "install",
    "key",
    "base",
    "head",
    "merge_base",
    "commits",
    "written_by",
    "host",
    "rules",
    "audit",
    "intent",
    "evidence",
    "route",
    "sample",
    "bundle",
    "reviewers",
    "reviewers_pending",
    "links",
    "post_merge_audit",
    "truncated",
];

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

/// The entry `team.docs-read`: a covering check that passes on any change to a
/// Markdown file of `docs/`.
fn docs_read() -> Value {
    entry(
        "team.docs-read",
        json!({
            "type": "grep-absent", "level": "fail", "files": ["docs/*.md"],
            "pattern": "ZZNEVERZZ", "scope": "changed", "covers": true,
        }),
    )
}

/// The `review` key that makes the area `docs` `auto-merge-eligible` under
/// generous limits, with the sample rate `rate`.
fn auto_merge(rate: f64) -> Value {
    json!({"route": {
        "areas": {"docs": {
            "route": "auto-merge-eligible",
            "maxDiffLines": 1000,
            "maxUncoveredShare": 1.0,
        }},
        "sampleRate": rate,
    }})
}

/// The `review` key with the record budget `max_bytes`.
fn budget(max_bytes: u64) -> Value {
    json!({"record": {"maxBytes": max_bytes}})
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

/// The text of `value` once serialized as a JSON string, without the quotes.
fn escaped(text: &str) -> String {
    let quoted = serde_json::to_string(text).expect("a string serializes");
    quoted[1..quoted.len() - 1].to_string()
}

/// The bytes of a note: the compact JSON of `record` and a line feed.
fn note_bytes(record: &Value) -> Vec<u8> {
    format!("{record}\n").into_bytes()
}

/// Whether the draw of spec 8.5 on `key` is below `rate`: the first eight bytes
/// of the SHA-256 of `houserules-sample:<key>`, read as a big-endian integer
/// and divided by 2^64.
fn sampled(key: &str, rate: f64) -> bool {
    let digest = sha256_hex(format!("houserules-sample:{key}").as_bytes());
    let value = u64::from_str_radix(&digest[..16], 16).expect("hex digits");
    (value as f64 / 18_446_744_073_709_551_616.0) < rate
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
    /// A repository with an identity and no commit.
    fn empty(folder: &'static str) -> Self {
        let install = Self {
            top: scratch_git_repo(),
            folder,
            scratch: tempfile::tempdir().expect("tempdir"),
        };
        git(install.top.path(), &["config", "user.name", "t"]);
        git(install.top.path(), &["config", "user.email", "t@t.t"]);
        install
    }

    /// Writes the install: the marker with the `review` key `review`, a
    /// knowledge base with `entries` in the topic `team`, and the files
    /// `docs/guide.md` (10 lines), `src/lib.rs` (10 lines), and `ops/deploy.sh`
    /// (5 lines). It commits nothing.
    fn lay_out(&self, entries: &[Value], review: Option<Value>) {
        let mut marker = json!({"schema": "houserules/install@2.3.0", "idPrefix": "WI"});
        if let Some(review) = review {
            marker["review"] = review;
        }
        self.write(
            ".houserules.json",
            &serde_json::to_string_pretty(&marker).expect("serialize the marker"),
        );
        self.write("knowledge/schema.json", "{}\n");
        self.write("knowledge/areas.json", &areas().to_string());
        self.write(
            "knowledge/team.json",
            &json!({"entries": entries}).to_string(),
        );
        self.write("docs/guide.md", &numbered("guide", 10));
        self.write("src/lib.rs", &numbered("lib", 10));
        self.write("ops/deploy.sh", &numbered("deploy", 5));
    }

    /// A repository whose install in `folder` holds `entries` and the marker with
    /// the `review` key `review`. The base commit holds them.
    fn new(folder: &'static str, entries: &[Value], review: Option<Value>) -> Self {
        let install = Self::empty(folder);
        install.lay_out(entries, review);
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

    /// The notes ref of the install (spec 8.1).
    fn notes_ref(&self) -> String {
        if self.folder.is_empty() {
            TOP_REF.to_string()
        } else {
            format!("refs/notes/houserules-sub/{}", self.folder)
        }
    }

    /// Runs `houserules record write <args> --dir <root>`.
    fn record_write(&self, args: &[&str]) -> Output {
        houserules()
            .args(["record", "write"])
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run record write")
    }

    /// Runs `houserules record show <args> --dir <root>`.
    fn record_show(&self, args: &[&str]) -> Output {
        houserules()
            .args(["record", "show"])
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run record show")
    }

    /// Writes the record of the range `base` to `head`, keyed by `head`, and
    /// requires exit 0 and no output.
    fn write_record(&self, base: &str, head: &str) {
        self.write_record_with(base, head, &[]);
    }

    /// Like [`Install::write_record`], with the flags `extra`.
    fn write_record_with(&self, base: &str, head: &str, extra: &[&str]) {
        let mut args = vec!["--base", base, "--head", head];
        args.extend(extra);
        let output = self.record_write(&args);
        assert_output(&output, 0, "", "");
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

    /// Replaces the note on `key` with `record`, as the push job of a unit
    /// does with a file.
    fn set_note(&self, key: &str, record: &Value) {
        let target = self.scratch_file("note.json");
        fs::write(&target, note_bytes(record)).expect("write the note file");
        git(
            self.top.path(),
            &[
                "notes",
                "--ref",
                &self.notes_ref(),
                "add",
                "-f",
                "-F",
                target.to_str().expect("UTF-8"),
                key,
            ],
        );
    }

    /// Whether the notes ref of the install exists.
    fn has_notes_ref(&self) -> bool {
        Command::new("git")
            .args(["rev-parse", "--verify", "--quiet", &self.notes_ref()])
            .current_dir(self.top.path())
            .output()
            .expect("run git")
            .status
            .success()
    }

    /// The hash of the rule sources of the install at `commit`, computed here
    /// as spec 4.5 states it: the SHA-256 of `git ls-tree -r --full-name` over
    /// the knowledge folder, the marker, and the seeded workflow.
    fn rules_hash_at(&self, commit: &str) -> String {
        let (folder, workflow) = if self.folder.is_empty() {
            (String::new(), "knowledge.yml".to_string())
        } else {
            (
                format!("{}/", self.folder),
                format!("knowledge-{}.yml", self.folder.replace('/', "-")),
            )
        };
        let listing = git(
            self.top.path(),
            &[
                "ls-tree",
                "-r",
                "--full-name",
                commit,
                "--",
                &format!(":(top,literal){folder}knowledge"),
                &format!(":(top,literal){folder}.houserules.json"),
                &format!(":(top,literal).github/workflows/{workflow}"),
            ],
        );
        sha256_hex(listing.as_bytes())
    }

    /// The first twelve characters of the hash of the rule sources at `commit`.
    fn hash12_at(&self, commit: &str) -> String {
        self.rules_hash_at(commit)[..12].to_string()
    }

    /// Runs `evidence --base <base> --head <head> --json <file>` and gives the
    /// document.
    fn evidence_json(&self, base: &str, head: &str, extra: &[&str]) -> Value {
        let target = self.scratch_file("evidence.json");
        let output = houserules()
            .args(["evidence", "--base", base, "--head", head, "--json"])
            .arg(&target)
            .args(extra)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run evidence");
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        schema_support::read_json(&target)
    }

    /// Writes the `audit --json` result of `base` to `HEAD` into the scratch
    /// directory and gives its path. The audit may exit 1 for a failed row;
    /// the file is written then too.
    fn audit_file(&self, base: &str) -> PathBuf {
        let target = self.scratch_file("audit.json");
        let output = houserules()
            .args(["audit", "--base", base, "--json"])
            .arg(&target)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run audit");
        assert!(target.exists(), "audit wrote no file: {}", stderr(&output));
        target
    }

    /// The lines that `audit --json` of `base` to `HEAD` holds as the failures
    /// of its reference summary, each ended by a line feed: what `record write`
    /// prints on stderr for the same range.
    fn audit_failure_lines(&self, base: &str) -> String {
        let audit = schema_support::read_json(&self.audit_file(base));
        failure_lines(&audit["references"]["failures"])
    }

    /// Runs `record write --base <base> --head <head> <extra>` and requires
    /// exit 0, nothing on stdout, and the failure lines of the audit on stderr:
    /// the work-item ids of these fixtures are in no backlog.
    fn write_record_printing_the_audit_failures(&self, base: &str, head: &str, extra: &[&str]) {
        let mut args = vec!["--base", base, "--head", head];
        args.extend(extra);
        let output = self.record_write(&args);
        assert_output(&output, 0, "", &self.audit_failure_lines(base));
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
}

/// A bare repository to push to and to clone from.
fn bare_remote() -> TempDir {
    let remote = tempfile::tempdir().expect("tempdir");
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    remote
}

/// The text of `path` as git takes it for a remote.
fn remote_path(remote: &TempDir) -> String {
    remote.path().to_str().expect("a UTF-8 path").to_string()
}

/// Clones `remote` into a new folder `clone` of a new directory, and gives the
/// directory and the path of the clone.
fn clone_of(remote: &TempDir) -> (TempDir, PathBuf) {
    let holder = tempfile::tempdir().expect("tempdir");
    git(
        holder.path(),
        &["clone", "-q", &remote_path(remote), "clone"],
    );
    let clone = holder.path().join("clone");
    (holder, clone)
}

/// A clone of `install` that holds the newest `depth` commits of `main` and the
/// notes ref of the install, and the directory that holds it. A shallow clone is
/// made with a `file://` URL, because git ignores `--depth` for a plain local
/// path.
fn shallow_clone_of(install: &Install, depth: u32) -> (TempDir, PathBuf) {
    let remote = bare_remote();
    let remote_url = remote_path(&remote);
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

/// Runs `houserules record show <sha> --dir <root>`.
fn show_in(root: &Path, sha: &str, extra: &[&str]) -> Output {
    houserules()
        .args(["record", "show", sha])
        .args(extra)
        .arg("--dir")
        .arg(root)
        .output()
        .expect("run record show")
}

/// The description of a request with the section of an evidence bundle: the
/// lines `lines`, then one run whose header line is `header`.
fn description(lines: &[&str], header: &str) -> String {
    format!(
        "Some text before.\n\n## Evidence bundle\n\n{}\n\n```houserules-run\n{header}\nok\n```\n",
        lines.join("\n")
    )
}

/// The header line of a capture at the commit `head`.
fn header_line(head: &str) -> String {
    json!({
        "schema": "houserules/capture@1.0.0",
        "argv": ["mise", "run", "test"],
        "cwd": "/work",
        "start": "2026-10-24T10:00:00.000000000Z",
        "end": "2026-10-24T10:00:01.000000000Z",
        "exit": 0,
        "signal": null,
        "head": head,
        "dirty": false,
        "lossy": 0,
    })
    .to_string()
}

/// The reviewers that spec 8.2 and 10.4 keep of `approvers`: sorted and unique,
/// the first 20, each cut to 100 characters, and no more names than keep the
/// serialized array within 3,072 bytes. The second value is the number of
/// approvers left out.
fn kept_reviewers(approvers: &[String]) -> (Vec<String>, usize) {
    let mut sorted: Vec<&String> = approvers.iter().collect();
    sorted.sort();
    sorted.dedup();
    let named: Vec<String> = sorted
        .iter()
        .take(20)
        .map(|name| name.chars().take(100).collect())
        .collect();
    let mut kept = named.len();
    while serde_json::to_string(&named[..kept])
        .expect("serialize")
        .len()
        > 3_072
    {
        kept -= 1;
    }
    (named[..kept].to_vec(), sorted.len() - kept)
}

/// Sets the reviewers of `record` as the program of the GitLab push job does
/// (spec 10.4): [`kept_reviewers`], the count of the names left out, and the
/// pending flag cleared.
fn set_reviewers(record: &mut Value, approvers: &[String]) {
    let (names, left_out) = kept_reviewers(approvers);
    record["reviewers"] = json!(names);
    if left_out > 0 {
        record["reviewers_more"] = json!(left_out);
    }
    record["reviewers_pending"] = json!(false);
}

/// `count` distinct names of 100 emoji each (400 bytes, 402 serialized).
fn emoji_names(count: u32) -> Vec<String> {
    (0..count)
        .map(|n| {
            let first = char::from_u32(0x1F600 + n).expect("an emoji");
            format!("{first}{}", "😀".repeat(99))
        })
        .collect()
}

/// The lines that a command printed on stderr, as the failure lines of
/// `failures` end them.
fn failure_lines(failures: &Value) -> String {
    failures
        .as_array()
        .expect("a list of failure lines")
        .iter()
        .map(|line| format!("{}\n", line.as_str().expect("a line")))
        .collect()
}

// ---- the record --------------------------------------------------------------------------

/// A record is a one-line note that holds the keys of spec 8.2 in their order,
/// and its document validates against the generated schema.
#[test]
fn a_record_is_a_one_line_note_with_the_keys_of_spec_8_2_in_order() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    let head = install.commit("docs: edit the guide");

    install.write_record(&base, &head);

    let text = install.note_text(&head);
    assert!(text.ends_with("}\n"), "{text}");
    assert_eq!(text.matches('\n').count(), 1, "one line: {text}");
    let record: Value = serde_json::from_str(&text).expect("JSON");
    let keys: Vec<&str> = record
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, RECORD_KEYS);
    assert_valid(&record, RECORD_SCHEMA);
}

/// The record names its range, the commits of the range, the writer, and the
/// hashes of the rule sources at both ends.
#[test]
fn a_record_names_its_range_its_commits_its_writer_and_its_rule_hashes() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let first = install.commit("docs: edit the guide");
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide again");

    install.write_record(&base, &head);

    let record = install.note(&head);
    assert_eq!(record["schema"], json!("houserules/change-record@1.1.0"));
    assert_eq!(record["install"], json!("."));
    assert_eq!(record["key"], json!(head));
    assert_eq!(record["base"], json!(base));
    assert_eq!(record["head"], json!(head));
    assert_eq!(record["merge_base"], json!(base));
    assert_eq!(record["commits"], json!([first, head]), "oldest first");
    assert_eq!(
        record["written_by"],
        json!(format!("houserules {}", env!("CARGO_PKG_VERSION")))
    );
    assert_eq!(record["host"], Value::Null);
    assert_eq!(
        record["rules"],
        json!({
            "merge_base_hash": install.rules_hash_at(&base),
            "merge_base_rules": "present",
            "head_hash": install.rules_hash_at(&head),
            "audited_with": "head",
            "packs": [],
        })
    );
    assert_eq!(record["sample"], Value::Null);
    assert_eq!(record["bundle"], Value::Null);
    assert_eq!(record["reviewers"], json!([]));
    assert_eq!(record["reviewers_pending"], json!(false));
    assert_eq!(record["links"], json!({"work_items": [], "sessions": []}));
    assert_eq!(record["post_merge_audit"], Value::Null);
    assert_eq!(record["truncated"], json!([]));
}

/// The evidence, the intent, the route, and the rule hashes of the record are
/// those that `evidence --json` writes for the range: the record keeps the
/// document of spec 5.9 whole, except the three keys that it holds on its own.
#[test]
fn the_record_keeps_the_evidence_document_of_the_range() {
    let install = Install::new("", &[docs_read(), judged_entry("team.judged")], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    install.write("misc.txt", "stray\n");
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let record = install.note(&head);

    let mut evidence = install.evidence_json(&base, &head, &[]);

    let object = evidence.as_object_mut().expect("an object");
    let rules = object.remove("rules").expect("rules");
    let intent = object.remove("intent").expect("intent");
    let route = object.remove("route").expect("route");
    let sessions = object.remove("sessions").expect("sessions");
    assert_eq!(record["evidence"], evidence);
    assert_eq!(record["links"]["sessions"], sessions);
    assert_eq!(record["route"], route);
    assert_eq!(record["intent"], intent);
    assert_eq!(record["rules"]["head_hash"], rules["head_hash"]);
    assert_eq!(record["rules"]["merge_base_hash"], rules["merge_base_hash"]);
    assert_eq!(record["rules"]["packs"], rules["packs"]);
    assert_eq!(
        record["links"]["work_items"], evidence["work_items"],
        "the work items of the range"
    );
}

/// The audit part of the record keeps the summary, the id, mode, and result of
/// each row, the reference summary as `audit --json` gives it, and no error. A
/// row that the file of `--audit` holds is the row that the in-process audit
/// gives.
#[test]
fn the_audit_part_keeps_the_summary_the_rows_and_the_reference_summary() {
    let install = Install::new("", &[docs_read(), judged_entry("team.judged")], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    let head = install.commit("docs: edit the guide");
    let audit = install.audit_file(&base);

    install.write_record(&base, &head);
    let in_process = install.note(&head);
    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--audit",
        audit.to_str().expect("UTF-8"),
    ]);
    assert_output(&output, 0, "", "");
    let from_file = install.note(&head);

    assert_eq!(
        in_process["audit"],
        json!({
            "summary": {
                "base": short(&base),
                "head": short(&head),
                "deterministic": 1,
                "pass": 1,
                "fail": 0,
                "warn": 0,
                "skipped": 0,
                "judged": 1,
                "reference_fail": 0,
            },
            "rules": [
                {"id": "team.docs-read", "mode": "deterministic", "result": "pass"},
                {"id": "team.judged", "mode": "judged", "result": "open"},
            ],
            "references": {"checked": 0, "passed": 0, "skipped": 0, "failures": []},
            "error": null,
        })
    );
    assert_eq!(from_file["audit"], in_process["audit"]);
    assert_eq!(
        in_process["audit"]["references"],
        schema_support::read_json(&audit)["references"],
        "the reference summary of `audit --json`"
    );
}

/// Every run of the same inputs writes the same bytes (spec 5.8), to `--out`
/// and to the note alike, and the note holds the bytes of the file.
#[test]
fn the_same_inputs_give_the_same_bytes_in_the_file_and_in_the_note() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    let head = install.commit("docs: edit the guide");
    let first = install.scratch_file("first.json");
    let second = install.scratch_file("second.json");
    for target in [&first, &second] {
        let output = install.record_write(&[
            "--base",
            &base,
            "--head",
            &head,
            "--out",
            target.to_str().expect("UTF-8"),
        ]);
        assert_output(&output, 0, "", "");
    }

    install.write_record(&base, &head);

    assert_eq!(
        fs::read(&first).expect("first"),
        fs::read(&second).expect("second")
    );
    assert_eq!(
        install.note_text(&head).into_bytes(),
        fs::read(&first).expect("first")
    );
}

/// `--out` writes the bytes of the note to a file and writes no note.
#[test]
fn out_writes_the_bytes_of_the_note_to_a_file_and_no_note() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let target = install.scratch_file("record.json");

    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--out",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(&output, 0, "", "");
    assert!(!install.has_notes_ref(), "no note, so no notes ref");
    let bytes = fs::read(&target).expect("the file");
    let record: Value = serde_json::from_slice(&bytes).expect("JSON");
    assert_eq!(bytes, note_bytes(&record));
    assert_eq!(record["key"], json!(head));
    assert_valid(&record, RECORD_SCHEMA);
}

/// The bytes of the note go to git on its standard input: `record write` makes
/// no temporary file, so a temporary folder that does not exist stops nothing.
#[test]
fn record_write_makes_no_temporary_file() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let missing = install.scratch_file("no-such-folder");
    assert!(!missing.exists());

    let output = houserules()
        .args(["record", "write", "--base", &base, "--head", &head, "--dir"])
        .arg(install.root())
        .env("TMPDIR", &missing)
        .env("TMP", &missing)
        .env("TEMP", &missing)
        .output()
        .expect("run record write");

    assert_output(&output, 0, "", "");
    assert_eq!(install.note(&head)["key"], json!(head));
    assert!(!missing.exists(), "nothing made the folder");
}

/// `--key` names the commit that the note annotates; the range stays the range
/// of `--head`, and the record names the key.
#[test]
fn the_key_names_the_commit_that_the_note_annotates() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write("misc.txt", "later\n");
    let key = install.commit("chore: a later commit");

    let output = install.record_write(&["--base", &base, "--head", &head, "--key", &key]);

    assert_output(&output, 0, "", "");
    let record = install.note(&key);
    assert_eq!(record["key"], json!(key));
    assert_eq!(record["head"], json!(head));
    assert_eq!(record["commits"], json!([head]));
}

/// `--host` records the host; a write without it records none.
#[test]
fn the_host_flag_names_the_host_of_the_record() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");

    for host in ["github", "gitlab"] {
        let output = install.record_write(&["--base", &base, "--head", &head, "--host", host]);
        assert_output(&output, 0, "", "");
        assert_eq!(install.note(&head)["host"], json!(host));
    }
    let output = install.record_write(&["--base", &base, "--head", &head, "--host", "bitbucket"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("invalid value 'bitbucket' for '--host <HOST>'"),
        "{}",
        stderr(&output)
    );
}

/// The schema rejects a document that is no record: a missing key, a host or a
/// rules presence outside its words, and a key of the wrong type.
#[test]
fn the_schema_rejects_a_document_that_is_no_record() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let record = install.note(&head);
    let schema = schema_support::read_json(&repo_root().join(RECORD_SCHEMA));
    let validator = jsonschema::validator_for(&schema).expect("a usable schema");
    assert!(validator.is_valid(&record), "the record itself is valid");

    let mut without_key = record.clone();
    without_key
        .as_object_mut()
        .expect("an object")
        .remove("key");
    let mut bad_host = record.clone();
    bad_host["host"] = json!("bitbucket");
    let mut bad_presence = record.clone();
    bad_presence["rules"]["merge_base_rules"] = json!("maybe");
    let mut bad_reviewers = record.clone();
    bad_reviewers["reviewers"] = json!("amy");
    let mut bad_sample = record.clone();
    bad_sample["sample"] = json!({"rate": "high", "sampled": true});
    let mut bad_audit = record.clone();
    bad_audit["post_merge_audit"] = json!({"verdict": "maybe", "by": "amy", "note": null});
    let mut counted = record.clone();
    counted["evidence"]["uncovered"]["paths"] = json!({"text": 1, "binary": 2});
    assert!(
        validator.is_valid(&counted),
        "the kinds of spec 4.2 key the counts"
    );
    let mut bad_kind = record.clone();
    bad_kind["evidence"]["uncovered"]["paths"] = json!({"text": 1, "## Injected heading": 2});

    for (name, document) in [
        ("a missing key", without_key),
        ("a host outside the two", bad_host),
        ("a presence outside the two", bad_presence),
        ("reviewers that are no list", bad_reviewers),
        ("a sample rate that is text", bad_sample),
        ("a verdict outside the two", bad_audit),
        ("a count of a kind that spec 4.2 does not name", bad_kind),
    ] {
        assert!(!validator.is_valid(&document), "{name}");
    }
}

// ---- the reviewers -------------------------------------------------------------------------

/// `--reviewer` names the approvers: sorted, without repeats, at most 20, each
/// cut to 100 characters, with the count of the names left out (spec 8.2).
#[test]
fn the_reviewers_are_sorted_unique_capped_at_20_and_cut_to_100_characters() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let mut args = vec!["--base", &base, "--head", &head];
    let mut names: Vec<String> = (0..25).rev().map(|n| format!("user{n:02}")).collect();
    names.push("user03".to_string());
    names.push(format!("0{}", "é".repeat(149)));
    for name in &names {
        args.extend(["--reviewer", name]);
    }

    let output = install.record_write(&args);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), 26, "25 names, one repeat, one long name");
    let expected: Vec<String> = sorted[..20]
        .iter()
        .map(|name| name.chars().take(100).collect())
        .collect();
    assert_eq!(record["reviewers"], json!(expected));
    assert_eq!(record["reviewers_more"], json!(6));
    assert_eq!(record["reviewers_pending"], json!(false));
    assert_valid(&record, RECORD_SCHEMA);
}

/// Up to 20 reviewers have no `reviewers_more` key.
#[test]
fn twenty_reviewers_have_no_count_past_20() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let names: Vec<String> = (0..20).map(|n| format!("user{n:02}")).collect();
    let mut args = vec!["--base", &base, "--head", &head];
    for name in &names {
        args.extend(["--reviewer", name]);
    }

    let output = install.record_write(&args);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    assert_eq!(record["reviewers"].as_array().expect("a list").len(), 20);
    assert!(record.get("reviewers_more").is_none());
}

/// Twenty names of 100 emoji take 402 bytes each once serialized: the record
/// keeps the seven that fit 3,072 bytes of the array and counts the other 13
/// in `reviewers_more` (spec 8.2).
#[test]
fn reviewers_past_3072_bytes_of_the_array_are_left_out_and_counted() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let names = emoji_names(20);
    let mut args = vec!["--base", &base, "--head", &head];
    for name in &names {
        args.extend(["--reviewer", name]);
    }

    let output = install.record_write(&args);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    let (kept, left_out) = kept_reviewers(&names);
    assert_eq!(
        kept.len(),
        7,
        "403 bytes a name, less the brackets, in 3,072"
    );
    assert_eq!(record["reviewers"], json!(kept));
    assert_eq!(record["reviewers_more"], json!(left_out));
    assert_eq!(left_out, 13);
    assert_valid(&record, RECORD_SCHEMA);
}

/// `--reviewers-pending` writes `reviewers: []` and `reviewers_pending: true`
/// (spec 8.3), and cannot be combined with `--reviewer`.
#[test]
fn pending_reviewers_write_an_empty_list_and_the_pending_flag() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");

    let output = install.record_write(&["--base", &base, "--head", &head, "--reviewers-pending"]);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    assert_eq!(record["reviewers"], json!([]));
    assert_eq!(record["reviewers_pending"], json!(true));
    let both = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--reviewers-pending",
        "--reviewer",
        "amy",
    ]);
    assert_eq!(both.status.code(), Some(2));
    assert!(
        stderr(&both).contains("cannot be used with"),
        "{}",
        stderr(&both)
    );
}

// ---- the bundle -------------------------------------------------------------------------------

/// `--bundle` is the description of the request: the record keeps the parts of
/// its bundle section and, for each run, the argv, the exit, the head, and the
/// SHA-256 of the header line. A description with CRLF line ends parses alike.
#[test]
fn the_bundle_of_the_description_is_parsed_into_the_record() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let header = header_line(&head);
    let text = description(
        &[
            "Purpose: Edit the guide.",
            "Intent: file:docs/plans/a.md",
            "Work items: WI-1, WI-2, WI-1",
            "Risk tier: standard",
            "Criterion: a docs change",
            "Budget reason: none needed",
        ],
        &header,
    );
    let expected = json!({
        "purpose": "Edit the guide.",
        "intent": "file:docs/plans/a.md",
        "work_items": ["WI-1", "WI-2"],
        "tier": "standard",
        "criterion": "a docs change",
        "budget_reason": "none needed",
        "runs": [{
            "argv": ["mise", "run", "test"],
            "exit": 0,
            "head": head,
            "sha256": sha256_hex(header.as_bytes()),
        }],
    });

    for (name, body) in [("LF", text.clone()), ("CRLF", text.replace('\n', "\r\n"))] {
        let target = install.scratch_file("description.md");
        fs::write(&target, body).expect("write the description");
        let output = install.record_write(&[
            "--base",
            &base,
            "--head",
            &head,
            "--bundle",
            target.to_str().expect("UTF-8"),
        ]);
        assert_output(&output, 0, "", "");
        let record = install.note(&head);
        assert_eq!(record["bundle"], expected, "{name}");
        assert_valid(&record, RECORD_SCHEMA);
    }
}

/// A description with no bundle section gives `bundle: null`; so does a write
/// with no `--bundle` (a push with no merged request).
#[test]
fn a_description_with_no_bundle_section_gives_a_null_bundle() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let target = install.scratch_file("description.md");
    fs::write(
        &target,
        "Only a description.\n\n## Notes\n\nNothing else.\n",
    )
    .expect("write");

    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--bundle",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(&output, 0, "", "");
    assert_eq!(install.note(&head)["bundle"], Value::Null);
}

/// A run whose first line is no capture header is kept with the SHA-256 of that
/// line and empty facts: the record is written whatever the bundle holds.
#[test]
fn a_run_that_is_no_capture_header_keeps_the_hash_of_its_line() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let target = install.scratch_file("description.md");
    fs::write(
        &target,
        description(&["Purpose: p", "Work items: WI-1"], "not a header"),
    )
    .expect("write");

    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--bundle",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(&output, 0, "", "");
    assert_eq!(
        install.note(&head)["bundle"]["runs"],
        json!([{
            "argv": [],
            "exit": null,
            "head": null,
            "sha256": sha256_hex(b"not a header"),
        }])
    );
}

// ---- the intent ---------------------------------------------------------------------------------

/// The review config that lets the `file` adapter read the plans.
fn plan_review() -> Value {
    json!({"intent": {"sources": ["work-item", "file"], "files": ["docs/plans/*.md"]}})
}

/// The packet's Intent section and the record's `intent` show the same values
/// for one range (case 4's last sentence): the work items with the title of the
/// tracker, a plan file, a named intent, and a named intent that does not
/// resolve.
#[test]
fn the_intent_of_the_record_is_the_intent_of_the_packet() {
    let install = Install::new("", &[], Some(plan_review()));
    install.use_json_tracker();
    let base = install.head();
    install.write(
        "docs/plans/plan.md",
        "# The plan\n\nWhy this change exists.\n\n- [x] one\n- [ ] two\n",
    );
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: add a plan\n\nWork-Item: HR-901");

    for (name, record_args, packet_args) in [
        ("no named intent", vec![], vec![]),
        (
            "a named plan",
            vec!["--intent", "file:docs/plans/plan.md"],
            vec!["--intent", "file:docs/plans/plan.md"],
        ),
        (
            "a named work item",
            vec!["--intent", "work-item:HR-901"],
            vec!["--intent", "work-item:HR-901"],
        ),
        (
            "a named work item that the tracker does not hold",
            vec!["--intent", "work-item:HR-999"],
            vec!["--intent", "work-item:HR-999"],
        ),
    ] {
        let mut args = vec!["--base", base.as_str(), "--head", head.as_str()];
        args.extend(record_args);
        let output = install.record_write(&args);
        assert_output(&output, 0, "", "");
        let record = install.note(&head);
        let evidence = install.evidence_json(&base, &head, &packet_args);
        assert_eq!(record["intent"], evidence["intent"], "{name}");
        assert!(
            !record["intent"].as_array().expect("a list").is_empty(),
            "{name}"
        );
    }
}

/// Without `--intent`, the `Intent:` line of the bundle names the intent.
#[test]
fn the_intent_line_of_the_bundle_names_the_intent() {
    let install = Install::new("", &[], Some(plan_review()));
    let base = install.head();
    install.write("docs/plans/plan.md", "# The plan\n\nWhy.\n");
    let head = install.commit("docs: add a plan");
    let target = install.scratch_file("description.md");
    fs::write(
        &target,
        description(
            &["Intent: file:docs/plans/plan.md", "Work items: WI-1"],
            &header_line(&head),
        ),
    )
    .expect("write");

    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--bundle",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    let evidence = install.evidence_json(&base, &head, &["--intent", "file:docs/plans/plan.md"]);
    assert_eq!(record["intent"], evidence["intent"]);
    assert_eq!(record["intent"][0]["ref"], json!("docs/plans/plan.md"));
    assert_eq!(record["intent"][0]["title"], json!("The plan"));
}

// ---- the route and the sample ---------------------------------------------------------------------

/// A route that is not `auto-merge-eligible` has no sample.
#[test]
fn a_route_that_is_not_auto_merge_eligible_has_a_null_sample() {
    let install = Install::new("", &[], Some(auto_merge(1.0)));
    let base = install.head();
    install.append("src/lib.rs", 1);
    let head = install.commit("feat: edit the library");

    install.write_record(&base, &head);

    let record = install.note(&head);
    assert_eq!(record["route"]["route"], json!("owner-review"));
    assert_eq!(record["sample"], Value::Null);
}

/// An `auto-merge-eligible` route draws the sample on the key (spec 8.5), and a
/// re-run gives the same answer.
#[test]
fn an_auto_merge_route_draws_the_sample_on_the_key_and_a_rerun_agrees() {
    let install = Install::new("", &[], Some(auto_merge(0.5)));
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");

    install.write_record(&base, &head);
    let first = install.note_text(&head);
    install.write_record(&base, &head);

    let record = install.note(&head);
    assert_eq!(record["route"]["route"], json!("auto-merge-eligible"));
    assert_eq!(
        record["sample"],
        json!({"rate": 0.5, "sampled": sampled(&head, 0.5)})
    );
    assert_eq!(install.note_text(&head), first);
    assert_valid(&record, RECORD_SCHEMA);
}

/// The draw is on the key that the note annotates, not on the head of the range:
/// a squash commit has a key of its own (spec 8.5). The test makes commits until
/// the two draws differ.
#[test]
fn the_sample_is_drawn_on_the_key_and_not_on_the_head_of_the_range() {
    let install = Install::new("", &[], Some(auto_merge(0.5)));
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let mut key = String::new();
    for attempt in 0..64 {
        install.write("misc.txt", &format!("{attempt}\n"));
        key = install.commit("chore: another commit");
        if sampled(&key, 0.5) != sampled(&head, 0.5) {
            break;
        }
    }
    assert_ne!(sampled(&key, 0.5), sampled(&head, 0.5), "64 draws agree");

    let output = install.record_write(&["--base", &base, "--head", &head, "--key", &key]);

    assert_output(&output, 0, "", "");
    assert_eq!(
        install.note(&key)["sample"],
        json!({"rate": 0.5, "sampled": sampled(&key, 0.5)})
    );
}

/// A squash merge: the record of the squash commit draws on the squash commit,
/// and the request's own head draws otherwise.
#[test]
fn a_squash_commit_draws_its_sample_on_the_squash_commit() {
    let install = Install::new("", &[], Some(auto_merge(0.5)));
    let base = install.head();
    let trunk = git(install.top.path(), &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string();
    git(install.top.path(), &["checkout", "-q", "-b", "feature"]);
    install.append("docs/guide.md", 1);
    let request_head = install.commit("docs: edit the guide");
    let mut squash = String::new();
    for attempt in 0..64 {
        git(install.top.path(), &["checkout", "-q", &trunk]);
        git(install.top.path(), &["merge", "-q", "--squash", "feature"]);
        install.write("docs/variant.md", &format!("{attempt}\n"));
        squash = install.commit("docs: edit the guide (squashed)");
        if sampled(&squash, 0.5) != sampled(&request_head, 0.5) {
            break;
        }
        git(install.top.path(), &["reset", "-q", "--hard", &base]);
    }
    assert_ne!(
        sampled(&squash, 0.5),
        sampled(&request_head, 0.5),
        "64 draws agree"
    );

    let output = install.record_write(&["--base", &base, "--head", &squash, "--key", &squash]);

    assert_output(&output, 0, "", "");
    let record = install.note(&squash);
    assert_eq!(record["commits"], json!([squash]));
    assert_eq!(
        record["sample"],
        json!({"rate": 0.5, "sampled": sampled(&squash, 0.5)})
    );
}

/// The `--score` flag adds the score to the route of the record.
#[test]
fn a_score_enters_the_route_of_the_record() {
    let install = Install::new(
        "",
        &[],
        Some(json!({"route": {"ownerScore": 0.5, "seniorScore": 0.8}})),
    );
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");

    let output = install.record_write(&["--base", &base, "--head", &head, "--score", "0.9"]);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    assert_eq!(record["route"]["route"], json!("senior-review"));
    assert_eq!(record["route"]["inputs"]["score"], json!(0.9));
}

// ---- a range from the empty tree and a fresh install ------------------------------------------------

/// A root push (spec 8.5, 10.1): the base is git's empty tree, the range holds
/// every commit of the head, the audit runs in process over every path, and
/// the merge base has no rules.
#[test]
fn a_root_push_writes_a_record_against_the_empty_tree() {
    let install = Install::new("", &[docs_read()], None);
    let head = install.head();

    let output = install.record_write(&["--base", EMPTY_TREE, "--head", &head]);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    assert_eq!(record["base"], json!(EMPTY_TREE));
    assert_eq!(record["merge_base"], Value::Null);
    assert_eq!(record["commits"], json!([head]));
    assert_eq!(record["rules"]["merge_base_hash"], Value::Null);
    assert_eq!(record["rules"]["merge_base_rules"], json!("absent"));
    let tracked = git(install.top.path(), &["ls-files"]);
    assert_eq!(
        record["evidence"]["diff"]["files"],
        json!(tracked.lines().count()),
        "every path of the head"
    );
    assert_eq!(
        record["audit"]["rules"],
        json!([{"id": "team.docs-read", "mode": "deterministic", "result": "pass"}])
    );
    assert_valid(&record, RECORD_SCHEMA);
    let shown = install.record_show(&[&head]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
}

/// The first request of a fresh install: the merge base holds no rule sources,
/// so the record has `merge_base_rules: "absent"` and `record show` checks only
/// the head.
#[test]
fn a_fresh_install_has_absent_rules_at_its_merge_base() {
    let install = Install::empty("");
    install.write("README.md", "# Project\n");
    let base = install.commit("chore: readme");
    install.lay_out(&[], None);
    let head = install.commit("chore: add the install");

    install.write_record(&base, &head);

    let record = install.note(&head);
    assert_eq!(record["merge_base"], json!(base));
    assert_eq!(record["rules"]["merge_base_hash"], Value::Null);
    assert_eq!(record["rules"]["merge_base_rules"], json!("absent"));
    assert_eq!(
        record["rules"]["head_hash"],
        json!(install.rules_hash_at(&head))
    );
    let shown = install.record_show(&[&head]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    let mut edited = record.clone();
    edited["rules"]["head_hash"] = json!("0".repeat(64));
    install.set_note(&head, &edited);
    let mismatch = install.record_show(&[&head]);
    assert_output(
        &mismatch,
        1,
        "",
        &format!(
            "record: {} claims rules {} at the head; git gives {}\n",
            short(&head),
            "0".repeat(12),
            install.hash12_at(&head)
        ),
    );
}

// ---- the rules of a re-run -----------------------------------------------------------------------------

/// A key that has a record of the same range gets the new record, and the old
/// `post_merge_audit` stays.
#[test]
fn a_rerun_of_the_same_range_replaces_the_record_and_keeps_the_audit() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let mut audited = install.note(&head);
    audited["post_merge_audit"] = json!({"verdict": "pass", "by": "amy", "note": "looks right"});
    install.set_note(&head, &audited);
    let before = install.note_text(&head);

    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--reviewer",
        "bob",
        "--host",
        "github",
    ]);

    assert_output(&output, 0, "", "");
    let after = install.note(&head);
    assert_ne!(install.note_text(&head), before);
    assert_eq!(after["post_merge_audit"], audited["post_merge_audit"]);
    assert_eq!(after["reviewers"], json!(["bob"]));
    assert_eq!(after["host"], json!("github"));
}

/// The same inputs again leave the note as it was, audit included.
#[test]
fn a_rerun_with_the_same_inputs_leaves_the_bytes_as_they_were() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let mut audited = install.note(&head);
    audited["post_merge_audit"] = json!({"verdict": "fail", "by": "amy", "note": null});
    install.set_note(&head, &audited);
    let before = install.note_text(&head);

    install.write_record(&base, &head);

    assert_eq!(install.note_text(&head), before);
}

/// A key that holds a record of another range exits 1 and changes nothing, with
/// a note and with `--out` alike. Another range is another head or another
/// merge-base hash.
#[test]
fn a_key_that_holds_a_record_of_another_range_exits_1() {
    let install = Install::new("", &[], None);
    let first_base = install.head();
    install.append("docs/guide.md", 1);
    let middle = install.commit("docs: edit the guide");
    install.write("knowledge/extra.json", &json!({"entries": []}).to_string());
    let second_base = install.commit("chore: change the rule sources");
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide again");
    install.write_record(&first_base, &head);
    let before = install.note_text(&head);
    let target = install.scratch_file("record.json");
    let line = format!(
        "record: {} already holds a record of another range\n",
        short(&head)
    );

    let other_head =
        install.record_write(&["--base", &first_base, "--head", &middle, "--key", &head]);
    let other_hash = install.record_write(&["--base", &second_base, "--head", &head]);
    let with_out = install.record_write(&[
        "--base",
        &first_base,
        "--head",
        &middle,
        "--key",
        &head,
        "--out",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(&other_head, 1, "", &line);
    assert_output(&other_hash, 1, "", &line);
    assert_output(&with_out, 1, "", &line);
    assert_eq!(install.note_text(&head), before, "the note stays");
    assert!(!target.exists(), "--out writes nothing");
}

// ---- trailers: intents and sessions (batch 38, spec 5.1 to 5.4; case 4) ---------------------

/// The value of the session trailer in the cases below: the form of a checkpoint
/// of the Entire tool.
const CHECKPOINT: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

/// The keys of the object `value`, in order.
fn keys(value: &Value) -> Vec<String> {
    value
        .as_object()
        .expect("a JSON object")
        .keys()
        .cloned()
        .collect()
}

/// The sessions of the record are the sessions of the packet: `links` holds
/// `sessions` after `work_items`, and the `evidence` of the record leaves `sessions`
/// out, as it leaves out `intent` (spec 5.4). The record is of schema 1.1.0, and so is
/// the evidence document that it keeps.
#[test]
fn the_record_keeps_the_sessions_in_links_and_not_in_its_evidence() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let first = install.commit(&format!("docs: edit\n\nEntire-Checkpoint: {CHECKPOINT}"));
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit again\n\nEntire-Checkpoint: cp-2");

    install.write_record(&base, &head);

    let record = install.note(&head);
    let evidence = install.evidence_json(&base, &head, &[]);
    assert_eq!(record["schema"], json!("houserules/change-record@1.1.0"));
    assert_eq!(
        record["evidence"]["schema"],
        json!("houserules/evidence@1.1.0")
    );
    assert_eq!(
        record["links"],
        json!({
            "work_items": [],
            "sessions": [
                {"commit": first, "trailer": "Entire-Checkpoint", "value": CHECKPOINT},
                {"commit": head, "trailer": "Entire-Checkpoint", "value": "cp-2"},
            ],
        })
    );
    assert_eq!(record["links"]["sessions"], evidence["sessions"]);
    assert_eq!(keys(&record["links"]), ["work_items", "sessions"]);
    assert!(record["evidence"].get("sessions").is_none());
    assert_valid(&record, RECORD_SCHEMA);
}

/// Version 1.1.0 adds `links.sessions` as an optional key: the schema does not
/// require it, because a new required key is a major version. `record write`
/// prints the key always, and the same record without it stays valid.
#[test]
fn the_record_schema_1_1_0_does_not_require_the_sessions_key() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let mut record = install.note(&head);
    assert!(
        record["links"].get("sessions").is_some(),
        "the command writes it"
    );

    record["links"]
        .as_object_mut()
        .expect("an object")
        .remove("sessions");

    assert_valid(&record, RECORD_SCHEMA);
}

/// The trailer intents join the record's intents as they join the packet's, a
/// malformed value and an unknown source included, and the command exits 0 for
/// them: a commit's text never makes `record write` exit 2 (spec 5.2).
#[test]
fn the_intent_trailers_of_the_range_are_the_intents_of_the_record() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.write("docs/plans/plan.md", "# The plan\n\nWhy.\n\n- [ ] one\n");
    let head = install.commit(
        "docs: add a plan\n\nIntent: file:docs/plans/plan.md\nIntent: oops\nIntent: jira:ABC-1",
    );

    install.write_record(&base, &head);

    let record = install.note(&head);
    let evidence = install.evidence_json(&base, &head, &[]);
    assert_eq!(record["intent"], evidence["intent"]);
    let found: Vec<(&str, &str)> = record["intent"]
        .as_array()
        .expect("the intents")
        .iter()
        .map(|intent| {
            (
                intent["source"].as_str().expect("a source"),
                intent["ref"].as_str().expect("a ref"),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("file", "docs/plans/plan.md"),
            ("", "oops"),
            ("jira", "ABC-1")
        ]
    );
    assert_eq!(
        record["intent"][1]["unresolved"],
        json!("an intent is written <source>:<ref>")
    );
    assert_valid(&record, RECORD_SCHEMA);
}

/// `--intent` keeps its precedence over the `Intent:` line of the bundle, and the
/// trailers add their intents to either: each `<source>:<ref>` counts once.
#[test]
fn the_flag_the_bundle_line_and_the_trailers_name_each_intent_once() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for name in ["a", "b", "c"] {
        install.write(
            &format!("docs/plans/{name}.md"),
            &format!("# Plan {name}\n\nWhy {name}.\n"),
        );
    }
    let head = install.commit(
        "docs: add the plans\n\nIntent: file:docs/plans/c.md\nIntent: file:docs/plans/a.md",
    );
    let target = install.scratch_file("description.md");
    fs::write(
        &target,
        description(
            &["Intent: file:docs/plans/b.md", "Work items: WI-1"],
            &header_line(&head),
        ),
    )
    .expect("write");

    install.write_record_with(
        &base,
        &head,
        &[
            "--intent",
            "file:docs/plans/a.md",
            "--bundle",
            target.to_str().expect("UTF-8"),
        ],
    );

    let refs: Vec<String> = install.note(&head)["intent"]
        .as_array()
        .expect("the intents")
        .iter()
        .map(|intent| intent["ref"].as_str().expect("a ref").to_string())
        .collect();
    assert_eq!(
        refs,
        ["docs/plans/a.md", "docs/plans/c.md"],
        "the flag wins over the bundle line, the trailer adds c, and a counts once"
    );
}

/// A record over the budget cuts `links.sessions` to its first 20 in step 7 of
/// spec 8.4 and counts the rest in `sessions_more`; `truncated` names `links`.
#[test]
fn a_record_over_the_budget_cuts_the_sessions_to_20_and_counts_the_rest() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for number in 0..20 {
        install.commit(&format!("docs: {number}\n\nEntire-Checkpoint: cp-{number}"));
    }
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: 20\n\nEntire-Checkpoint: cp-20");
    let target = install.scratch_file("description.md");
    fs::write(
        &target,
        description(
            &[
                &format!("Purpose: {}", "x".repeat(100_000)),
                "Work items: WI-1",
                "Risk tier: standard",
                "Criterion: a docs change",
            ],
            &header_line(&head),
        ),
    )
    .expect("write");

    install.write_record_with(&base, &head, &["--bundle", target.to_str().expect("UTF-8")]);

    let record = install.note(&head);
    assert_eq!(
        record["links"]["sessions"].as_array().map(Vec::len),
        Some(20)
    );
    assert_eq!(record["links"]["sessions"][19]["value"], json!("cp-19"));
    assert_eq!(record["links"]["sessions_more"], json!(1));
    let truncated: Vec<&str> = record["truncated"]
        .as_array()
        .expect("truncated")
        .iter()
        .map(|name| name.as_str().expect("a name"))
        .collect();
    assert!(truncated.contains(&"links"), "{truncated:?}");
    assert_valid(&record, RECORD_SCHEMA);
}

/// The minimal record holds no `links`, so it holds no session (spec 5.4).
#[test]
fn the_minimal_record_holds_no_session() {
    let install = Install::new("", &[], Some(budget(8_192)));
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit(&format!(
        "docs: edit\n\nEntire-Checkpoint: {}",
        "c".repeat(9_000)
    ));

    install.write_record(&base, &head);

    let record = install.note(&head);
    assert_eq!(record["truncated"], json!(["all"]));
    assert!(record.get("links").is_none());
    assert!(!install.note_text(&head).contains("sessions"));
    assert_valid(&record, RECORD_SCHEMA);
}

/// `record show` prints the `Sessions:` line of the packet from `links`, in the
/// place and form of the packet, and `Sessions: none` for a record with none.
#[test]
fn record_show_prints_the_sessions_line() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit(&format!("docs: edit\n\nEntire-Checkpoint: {CHECKPOINT}"));
    install.write_record(&base, &head);

    let shown = stdout(&install.record_show(&[&head]));

    assert!(
        shown.contains(&format!(
            "Diff: +1 -0 lines in 1 files\nWork items: none\nSessions: {} Entire-Checkpoint: `{CHECKPOINT}`\n",
            short(&head)
        )),
        "{shown}"
    );
}

/// A note is data that anyone with push access to the notes ref can write. `record
/// show` prints the commit and the key of a session of a note through `review::text`,
/// as it prints the value: a field with a line end or a heading adds no line to the
/// summary and prints inside code spans.
#[test]
fn record_show_prints_the_fields_of_a_hostile_session_inside_code_spans() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit(&format!("docs: edit\n\nEntire-Checkpoint: {CHECKPOINT}"));
    install.write_record(&base, &head);
    let mut note = install.note(&head);
    note["links"]["sessions"] = json!([{
        "commit": "<b>x</b>\n## Injected heading",
        "trailer": "Evil\n\n## Injected",
        "value": "v",
    }]);
    install.set_note(&head, &note);

    let shown = install.record_show(&[&head]);

    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    let text = stdout(&shown);
    assert!(
        !text.lines().any(|line| line.starts_with("## Injected")),
        "{text}"
    );
    assert!(
        text.contains("\nSessions: `<b>x</b` `\"Evil\\n\\n## Injected\"`: `v`\n"),
        "{text}"
    );
}

/// A note of schema 1.0.0, as the binary of batch 37 wrote it, still reads: its
/// `links.sessions` reads as empty, `record show` prints `Sessions: none`, and
/// `--json` prints the empty list, with the schema id that the note holds.
#[test]
fn a_record_of_schema_1_0_0_reads_with_no_sessions() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    let head = install.commit("docs: edit the guide");
    let note = note_support::note_1_0_0(&base, &head, &install.rules_hash_at(&head));
    assert!(note["links"].get("sessions").is_none());
    install.set_note(&head, &note);

    let shown = install.record_show(&[&head]);
    let json = install.record_show(&[&head, "--json"]);

    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    assert!(
        stdout(&shown).contains("Work items: none\nSessions: none\n"),
        "{}",
        stdout(&shown)
    );
    assert_eq!(json.status.code(), Some(0), "{}", stderr(&json));
    let record: Value = serde_json::from_str(&stdout(&json)).expect("the record");
    assert_eq!(
        record["schema"],
        json!("houserules/change-record@1.0.0"),
        "the note keeps its own schema id"
    );
    assert_eq!(record["links"]["sessions"], json!([]));
}

// ---- record show -----------------------------------------------------------------------------------

/// The whole Markdown summary of a record, byte for byte: the packet's sections,
/// then the route, the sample, the reviewers, and the post-merge audit.
#[test]
fn record_show_prints_the_summary_of_the_record() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);

    let output = install.record_show(&[&head]);

    assert_output(
        &output,
        0,
        &format!(
            "## houserules change record\n\
             \n\
             Record {head7} · range {base7}..{head7} (merge base {base7}) · rules {hash12} · route owner-review\n\
             \n\
             Written by `houserules {version}`.\n\
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
             Sessions: none\n\
             \n\
             ### Route\n\
             \n\
             owner-review\n\
             \n\
             - area `docs`: owner-review\n\
             \n\
             ### Sample\n\
             \n\
             None.\n\
             \n\
             ### Reviewers\n\
             \n\
             None recorded.\n\
             \n\
             ### Post-merge audit\n\
             \n\
             None recorded.\n",
            head7 = short(&head),
            base7 = short(&base),
            hash12 = install.hash12_at(&head),
            version = env!("CARGO_PKG_VERSION"),
        ),
        "",
    );
}

/// `--json` prints the record document, as the note holds it.
#[test]
fn record_show_json_prints_the_document_of_the_note() {
    let install = Install::new("", &[docs_read()], None);
    let base = install.head();
    install.append("docs/guide.md", 3);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let record = install.note(&head);

    let output = install.record_show(&[&head, "--json"]);

    assert_output(
        &output,
        0,
        &format!(
            "{}\n",
            serde_json::to_string_pretty(&record).expect("pretty")
        ),
        "",
    );
    assert_valid(&record, RECORD_SCHEMA);
}

/// A short sha names the record, as a full sha does.
#[test]
fn record_show_takes_a_short_sha() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);

    let full = install.record_show(&[&head]);
    let abbreviated = install.record_show(&[short(&head)]);

    assert_eq!(
        abbreviated.status.code(),
        Some(0),
        "{}",
        stderr(&abbreviated)
    );
    assert_eq!(stdout(&abbreviated), stdout(&full));
}

/// The record of a commit that a record lists: `record show` finds it from any
/// commit of the range, not only from the key.
#[test]
fn record_show_finds_the_record_from_any_commit_it_lists() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let first = install.commit("docs: first");
    install.append("docs/guide.md", 1);
    let second = install.commit("docs: second");
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: third");
    install.write_record(&base, &head);

    for sha in [&first, &second, &head] {
        let output = install.record_show(&[sha, "--json"]);
        assert_eq!(output.status.code(), Some(0), "{sha}: {}", stderr(&output));
        let shown: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
        assert_eq!(shown["key"], json!(head), "{sha}");
    }
}

/// `record show <sha>` finds the record keyed by `key`.
fn assert_record_of(install: &Install, sha: &str, key: &str) {
    let output = install.record_show(&[sha, "--json"]);
    assert_eq!(output.status.code(), Some(0), "{sha}: {}", stderr(&output));
    let shown: Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(shown["key"], json!(key), "{sha}");
}

/// `record show <sha>` finds no record: `record: no record for <sha7>`, exit 1.
fn assert_no_record_of(install: &Install, sha: &str) {
    let output = install.record_show(&[sha]);
    assert_output(
        &output,
        1,
        "",
        &format!("record: no record for {}\n", short(sha)),
    );
}

/// Commits three edits of the guide, and gives the three commits, oldest first.
fn three_commits(install: &Install) -> [String; 3] {
    ["first", "second", "third"].map(|word| {
        install.append("docs/guide.md", 1);
        install.commit(&format!("docs: {word}"))
    })
}

/// The description of a request whose purpose has 100,000 characters: the
/// record of any range falls to the cuts of 8.4 that follow the commit list.
fn long_purpose_description(head: &str) -> String {
    description(
        &[
            &format!("Purpose: {}", "x".repeat(100_000)),
            "Work items: WI-1",
            "Risk tier: standard",
            "Criterion: a docs change",
        ],
        &header_line(head),
    )
}

/// Spec 8.1: the record lists every commit of its range, so `record show` finds
/// it from any of them. A budget that cut the list to `{count, first, last}`
/// does not lose the commits between: they are the commits of the range of the
/// record, and the merge base and later commits are not.
#[test]
fn record_show_finds_every_commit_of_a_range_whose_list_the_budget_cut() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let [first, middle, head] = three_commits(&install);
    let target = install.scratch_file("description.md");
    fs::write(&target, long_purpose_description(&head)).expect("write");
    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--bundle",
        target.to_str().expect("UTF-8"),
    ]);
    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    assert_eq!(
        record["commits"],
        json!({"count": 3, "first": first, "last": head}),
        "step 4 cut the list"
    );
    install.append("docs/guide.md", 1);
    let later = install.commit("docs: a later commit");

    for sha in [&first, &middle, &head] {
        assert_record_of(&install, sha, &head);
    }

    assert_no_record_of(&install, &base);
    assert_no_record_of(&install, &later);
}

/// The same for the minimal record, which holds no list at all.
#[test]
fn record_show_finds_every_commit_of_a_minimal_record() {
    let install = Install::new("", &[], Some(budget(8_192)));
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    let first = install.commit("docs: first");
    install.append("docs/guide.md", 1);
    let middle = install.commit("docs: second");
    install.append("docs/guide.md", 1);
    let ids: Vec<String> = (0..10)
        .map(|n| format!("W{n:02}{}", "x".repeat(411)))
        .collect();
    let head = install.commit(&trailers(&ids));
    install.write_record_printing_the_audit_failures(&base, &head, &[]);
    let record = install.note(&head);
    assert_eq!(record["truncated"], json!(["all"]));
    assert!(
        record.get("commits").is_none(),
        "the minimal record lists none"
    );

    for sha in [&first, &middle, &head] {
        assert_record_of(&install, sha, &head);
    }

    assert_no_record_of(&install, &base);
}

/// A record of a range from git's empty tree has no merge base: every commit of
/// the history up to its head is in its range.
#[test]
fn a_cut_record_of_a_root_push_is_found_from_every_commit_of_the_history() {
    let install = Install::empty("");
    install.lay_out(&[], None);
    let root = install.commit("chore: base");
    let [first, middle, head] = three_commits(&install);
    let target = install.scratch_file("description.md");
    fs::write(&target, long_purpose_description(&head)).expect("write");
    let output = install.record_write(&[
        "--base",
        EMPTY_TREE,
        "--head",
        &head,
        "--bundle",
        target.to_str().expect("UTF-8"),
    ]);
    assert_output(&output, 0, "", "");
    assert_eq!(
        install.note(&head)["commits"],
        json!({"count": 4, "first": root, "last": head})
    );

    for sha in [&root, &first, &middle, &head] {
        assert_record_of(&install, sha, &head);
    }
}

/// A user whose git colors its output (`color.ui`, `color.grep`) still finds the
/// record of a commit that a record lists and of a commit between the ends of a
/// cut list (`houserules.pinned-git-text-runs-hermetic`).
#[test]
fn record_show_finds_a_record_under_the_color_settings_of_a_user() {
    for cut in [false, true] {
        let install = Install::new("", &[], None);
        for (key, value) in [("color.ui", "always"), ("color.grep", "always")] {
            git(install.top.path(), &["config", key, value]);
        }
        let base = install.head();
        let [first, middle, head] = three_commits(&install);
        let mut args = vec!["--base", &base, "--head", &head];
        let target = install.scratch_file("description.md");
        if cut {
            fs::write(&target, long_purpose_description(&head)).expect("write");
            args.extend(["--bundle", target.to_str().expect("UTF-8")]);
        }
        assert_output(&install.record_write(&args), 0, "", "");
        assert_eq!(install.note(&head)["commits"].is_array(), !cut, "cut {cut}");

        for sha in [&first, &middle, &head] {
            assert_record_of(&install, sha, &head);
        }
    }
}

/// `record show` prints the kinds of a count map as the note holds them, and a
/// note is repository data: each kind prints in a code span, so that none changes
/// the layout of the summary (spec 4.8).
#[test]
fn the_kinds_of_a_count_map_print_in_code_spans() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let mut record = install.note(&head);
    record["evidence"]["uncovered"]["paths"] = json!({"<!-- hidden": 1, "## Injected heading": 2});
    install.set_note(&head, &record);

    let output = install.record_show(&[&head]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("\n- `<!-- hidden`: 1 paths\n- `## Injected heading`: 2 paths\n"),
        "{}",
        stdout(&output)
    );
}

/// A commit that no record holds: `record: no record for <sha7>`, exit 1, when
/// the notes ref exists.
#[test]
fn a_commit_with_no_record_exits_1_with_its_line() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    install.append("docs/guide.md", 1);
    let later = install.commit("docs: a later commit");

    let output = install.record_show(&[&later]);

    assert_output(
        &output,
        1,
        "",
        &format!("record: no record for {}\n", short(&later)),
    );
    let base_output = install.record_show(&[&base]);
    assert_output(
        &base_output,
        1,
        "",
        &format!("record: no record for {}\n", short(&base)),
    );
}

/// A ref that is no commit is a setup error.
#[test]
fn a_sha_that_is_no_commit_exits_2() {
    let install = Install::new("", &[], None);

    let output = install.record_show(&["nope"]);

    assert_output(&output, 2, "", "bad ref \"nope\"\n");
}

// ---- the rule hashes ------------------------------------------------------------------------------------

/// A record whose `head_hash` another rule package would give fails `record
/// show` with its line, and prints nothing on stdout.
#[test]
fn a_record_with_an_edited_head_hash_fails_record_show() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let mut record = install.note(&head);
    record["rules"]["head_hash"] = json!("f".repeat(64));
    install.set_note(&head, &record);

    let output = install.record_show(&[&head]);

    assert_output(
        &output,
        1,
        "",
        &format!(
            "record: {} claims rules {} at the head; git gives {}\n",
            short(&head),
            "f".repeat(12),
            install.hash12_at(&head)
        ),
    );
    let json = install.record_show(&[&head, "--json"]);
    assert_eq!(json.status.code(), Some(1));
    assert_eq!(stdout(&json), "", "--json prints nothing for a bad record");
}

/// The same for `merge_base_hash`; both mismatches print one line each, the
/// merge base first.
#[test]
fn a_record_with_an_edited_merge_base_hash_fails_record_show() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let mut record = install.note(&head);
    record["rules"]["merge_base_hash"] = json!("e".repeat(64));
    install.set_note(&head, &record);
    let merge_base_line = format!(
        "record: {} claims rules {} at the merge base; git gives {}\n",
        short(&head),
        "e".repeat(12),
        install.hash12_at(&base)
    );

    let one = install.record_show(&[&head]);
    record["rules"]["head_hash"] = json!("d".repeat(64));
    install.set_note(&head, &record);
    let both = install.record_show(&[&head]);

    assert_output(&one, 1, "", &merge_base_line);
    assert_output(
        &both,
        1,
        "",
        &format!(
            "{merge_base_line}record: {} claims rules {} at the head; git gives {}\n",
            short(&head),
            "d".repeat(12),
            install.hash12_at(&head)
        ),
    );
}

/// The hash is read at the commit of the record, so a later change to
/// `review.ci.files` leaves the record valid.
#[test]
fn a_later_change_to_the_ci_files_still_passes_record_show() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    edit_marker(&install.root(), |marker| {
        marker.insert(
            "review".to_string(),
            json!({"ci": {"files": [".github/workflows/review.yml"]}}),
        );
    });
    install.write(".github/workflows/review.yml", "name: review\n");
    install.commit("ci: wire the review");

    let output = install.record_show(&[&head]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
}

/// A hash that reads a CI file of the same record: a record written after the
/// wiring names the CI files of its own commit, and changes with them.
#[test]
fn the_ci_files_of_the_commit_decide_the_head_hash() {
    let install = Install::new("", &[], None);
    edit_marker(&install.root(), |marker| {
        marker.insert(
            "review".to_string(),
            json!({"ci": {"files": [".github/workflows/review.yml"]}}),
        );
    });
    install.write(".github/workflows/review.yml", "name: review\n");
    let base = install.commit("ci: wire the review");
    install.write(".github/workflows/review.yml", "name: review again\n");
    let head = install.commit("ci: change the wiring");

    install.write_record(&base, &head);

    let record = install.note(&head);
    let listing = git(
        install.top.path(),
        &[
            "ls-tree",
            "-r",
            "--full-name",
            &head,
            "--",
            ":(top,literal)knowledge",
            ":(top,literal).houserules.json",
            ":(top,literal).github/workflows/review.yml",
            ":(top,literal).github/workflows/knowledge.yml",
        ],
    );
    assert_eq!(
        record["rules"]["head_hash"],
        json!(sha256_hex(listing.as_bytes()))
    );
    assert_ne!(
        record["rules"]["head_hash"], record["rules"]["merge_base_hash"],
        "the changed workflow changes the hash"
    );
    assert_eq!(
        record["route"]["route"],
        json!("senior-review"),
        "a changed CI file is a rule source"
    );
    let shown = install.record_show(&[&head]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
}

// ---- notes in a remote -----------------------------------------------------------------------------------

/// The notes ref pushes to a bare remote and fetches into a clone. A clone has
/// no ref after `git clone`: `record show` prints the fetch line; after the
/// fetch it shows the record.
#[test]
fn a_clone_without_the_ref_prints_the_fetch_line_and_shows_the_record_after_a_fetch() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let remote = bare_remote();
    let remote_url = remote_path(&remote);
    git(
        install.top.path(),
        &["push", "-q", &remote_url, "HEAD:refs/heads/main"],
    );
    git(
        install.top.path(),
        &["push", "-q", &remote_url, &format!("{TOP_REF}:{TOP_REF}")],
    );
    let (_holder, clone) = clone_of(&remote);

    let without = show_in(&clone, &head, &[]);
    git(
        &clone,
        &["fetch", "-q", "origin", &format!("{TOP_REF}:{TOP_REF}")],
    );
    let with = show_in(&clone, &head, &[]);

    assert_output(
        &without,
        1,
        "",
        &format!("no change records here; fetch them: git fetch origin {TOP_REF}:{TOP_REF}\n"),
    );
    assert_eq!(with.status.code(), Some(0), "{}", stderr(&with));
    assert!(
        stdout(&with).contains(&format!("Record {}", short(&head))),
        "{}",
        stdout(&with)
    );
    let from_install = install.record_show(&[&head]);
    assert_eq!(stdout(&with), stdout(&from_install));
}

/// A subdirectory install has its own ref, `refs/notes/houserules-sub/<path>`;
/// the fetch line names it; the hash is the same from the top level and from a
/// folder below the install; and an edit of a knowledge file changes the hash
/// of the head.
#[test]
fn a_subdirectory_install_keeps_its_record_under_its_own_ref() {
    let install = Install::new("sub", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let first = install.commit("docs: edit the guide");
    install.write("knowledge/extra.json", &json!({"entries": []}).to_string());
    let head = install.commit("chore: edit a knowledge file");

    install.write_record(&base, &first);
    install.write_record(&base, &head);

    assert!(install.has_notes_ref());
    assert_eq!(install.notes_ref(), "refs/notes/houserules-sub/sub");
    let top_ref = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", TOP_REF])
        .current_dir(install.top.path())
        .output()
        .expect("run git");
    assert!(
        !top_ref.status.success(),
        "no record under the top-level ref"
    );
    let record = install.note(&head);
    assert_eq!(record["install"], json!("sub"));
    assert_eq!(
        record["rules"]["head_hash"],
        json!(install.rules_hash_at(&head))
    );
    assert_ne!(
        install.note(&first)["rules"]["head_hash"],
        record["rules"]["head_hash"],
        "an edit of a knowledge file changes the head hash"
    );
    for directory in [
        install.top.path().to_path_buf(),
        install.top.path().join("sub").join("knowledge"),
    ] {
        let output = houserules()
            .args(["record", "show", &head, "--dir"])
            .arg(install.root())
            .current_dir(&directory)
            .output()
            .expect("run record show");
        assert_eq!(
            output.status.code(),
            Some(0),
            "from {}: {}",
            directory.display(),
            stderr(&output)
        );
    }
    let remote = bare_remote();
    git(
        install.top.path(),
        &["push", "-q", &remote_path(&remote), "HEAD:refs/heads/main"],
    );
    let (_holder, clone) = clone_of(&remote);
    let fetch_line = show_in(&clone.join("sub"), &head, &[]);
    assert_output(
        &fetch_line,
        1,
        "",
        "no change records here; fetch them: git fetch origin refs/notes/houserules-sub/sub:refs/notes/houserules-sub/sub\n",
    );
}

/// A shallow clone that lacks the merge base cannot check the hash: exit 2 with
/// the line of spec 8.3.
#[test]
fn a_shallow_clone_without_the_merge_base_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    install.write_record(&base, &head);
    let (_holder, clone) = shallow_clone_of(&install, 1);

    let output = show_in(&clone, &head, &[]);

    assert_output(
        &output,
        2,
        "",
        &format!(
            "record: {} is not in this clone; fetch the full history to check the rules hash\n",
            short(&base)
        ),
    );
}

/// A shallow clone drops the history older than its depth and never the newer
/// history. A cut record whose head the clone lacks cannot hold a commit that the
/// clone has, so `record show` answers `record: no record for <sha7>` (exit 1)
/// and never a line of git. A commit between the ends of a record whose base the
/// clone lacks gets the line of the key of that record (exit 2): the clone cannot
/// tell whether the commit is in the range or before it.
#[test]
fn a_shallow_clone_answers_with_the_lines_of_spec_8_3_for_a_cut_record() {
    let install = Install::new("", &[], None);
    let base = install.head();
    let [_first, middle, head] = three_commits(&install);
    let target = install.scratch_file("description.md");
    fs::write(&target, long_purpose_description(&head)).expect("write");
    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--bundle",
        target.to_str().expect("UTF-8"),
    ]);
    assert_output(&output, 0, "", "");
    assert!(
        install.note(&head)["commits"].is_object(),
        "step 4 cut the list"
    );
    install.append("docs/guide.md", 1);
    let later = install.commit("docs: a later commit");
    let no_record = |sha: &str| format!("record: no record for {}\n", short(sha));
    let base_is_absent = format!(
        "record: {} is not in this clone; fetch the full history to check the rules hash\n",
        short(&base)
    );

    let (_holder_one, depth_one) = shallow_clone_of(&install, 1);
    assert_output(&show_in(&depth_one, &later, &[]), 1, "", &no_record(&later));

    let (_holder_three, depth_three) = shallow_clone_of(&install, 3);
    assert_output(&show_in(&depth_three, &middle, &[]), 2, "", &base_is_absent);
    assert_output(&show_in(&depth_three, &head, &[]), 2, "", &base_is_absent);
    assert_output(
        &show_in(&depth_three, &later, &[]),
        1,
        "",
        &no_record(&later),
    );
}

/// The git processes that `record show <sha>` started in the install, counted by
/// command name as the trace2 events of git report them. The commit has no
/// record.
fn git_processes_of_show(install: &Install, sha: &str) -> BTreeMap<String, usize> {
    let trace = install.scratch_file("trace2.json");
    let output = houserules()
        .env("GIT_TRACE2_EVENT", &trace)
        .args(["record", "show", sha])
        .arg("--dir")
        .arg(install.root())
        .output()
        .expect("run record show");
    assert_output(
        &output,
        1,
        "",
        &format!("record: no record for {}\n", short(sha)),
    );
    let mut counts = BTreeMap::new();
    for line in fs::read_to_string(&trace).expect("the trace").lines() {
        let event: Value = serde_json::from_str(line).expect("a trace2 event");
        if event["event"] == json!("cmd_name") {
            let name = event["name"].as_str().expect("a command name");
            *counts.entry(name.to_string()).or_default() += 1;
        }
    }
    counts
}

/// An install whose notes ref holds `count` cut records, each of the commit that
/// keys it, and the sha of a later commit that none of them holds.
fn install_with_cut_records(count: usize) -> (Install, String) {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let first = install.commit("docs: first");
    install.write_record(&base, &first);
    let mut record = install.note(&first);
    record["commits"] = json!({"count": 1, "first": first, "last": first});
    let mut previous = first;
    for n in 0..count {
        install.append("docs/guide.md", 1);
        let key = install.commit(&format!("docs: change {n}"));
        record["key"] = json!(key);
        record["head"] = json!(key);
        record["base"] = json!(previous);
        record["merge_base"] = json!(previous);
        install.set_note(&key, &record);
        previous = key;
    }
    install.append("docs/guide.md", 1);
    let later = install.commit("docs: a later commit");
    (install, later)
}

/// A search over the cut records starts one `git cat-file` for all the notes,
/// one `git merge-base` for each cut record, and a fixed number of other git
/// processes: the cost grows by one process for each record.
#[test]
fn record_show_starts_one_git_process_for_each_cut_record_and_a_fixed_number_besides() {
    let totals: Vec<(usize, usize)> = [4, 24]
        .map(|count| {
            let (install, later) = install_with_cut_records(count);
            let processes = git_processes_of_show(&install, &later);
            assert_eq!(
                processes.get("cat-file"),
                Some(&1),
                "{count}: {processes:?}"
            );
            assert_eq!(
                processes.get("merge-base"),
                Some(&count),
                "{count}: {processes:?}"
            );
            (count, processes.values().sum())
        })
        .to_vec();

    assert_eq!(
        totals[1].1 - totals[0].1,
        totals[1].0 - totals[0].0,
        "the 20 added records add 20 processes: {totals:?}"
    );
}

// ---- the budget (spec 8.4) ----------------------------------------------------------------------------------

/// The bytes of the note on `key`.
fn note_size(install: &Install, key: &str) -> usize {
    install.note_text(key).len()
}

/// A range of 5,000 changed paths in an area fits the default budget through
/// the cascade of 8.4: the ranges, the uncovered paths, and the route inputs
/// are cut, in that order, and no later step runs.
#[test]
fn a_range_of_5000_paths_in_an_area_fits_the_default_budget() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for n in 0..5_000 {
        install.write(&format!("docs/f{n:04}.md"), "x\n");
    }
    let head = install.commit("docs: add 5,000 files");

    install.write_record(&base, &head);

    let record = install.note(&head);
    assert!(note_size(&install, &head) <= 65_536);
    assert_eq!(
        record["truncated"],
        json!([
            "evidence.uncovered.paths.ranges",
            "evidence.uncovered.paths",
            "route.inputs"
        ])
    );
    assert_eq!(
        record["evidence"]["uncovered"]["paths"],
        json!({"text": 5000})
    );
    assert_eq!(record["route"]["inputs"]["areas"], json!({"docs": 5000}));
    assert_eq!(record["evidence"]["diff"]["files"], json!(5000));
    assert_valid(&record, RECORD_SCHEMA);
    let shown = install.record_show(&[&head]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    assert!(stdout(&shown).contains("### Cut\n"), "{}", stdout(&shown));
}

/// The same for 5,000 paths that are in no area: the list of `no_area` is cut
/// to a count.
#[test]
fn a_range_of_5000_paths_in_no_area_fits_the_default_budget() {
    let install = Install::new("", &[], None);
    let base = install.head();
    for n in 0..5_000 {
        install.write(&format!("misc/f{n:04}.txt"), "x\n");
    }
    let head = install.commit("chore: add 5,000 files");

    install.write_record(&base, &head);

    let record = install.note(&head);
    assert!(note_size(&install, &head) <= 65_536);
    assert_eq!(
        record["truncated"],
        json!([
            "evidence.uncovered.paths.ranges",
            "evidence.uncovered.paths",
            "route.inputs"
        ])
    );
    assert_eq!(record["route"]["inputs"]["no_area"], json!(5000));
    assert_valid(&record, RECORD_SCHEMA);
}

/// A purpose of 100,000 characters fits through the last cuts: the purpose is
/// cut to 1,000 characters, and the earlier steps have run on the way.
#[test]
fn a_purpose_of_100000_characters_is_cut_to_1000() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let target = install.scratch_file("description.md");
    let purpose = "x".repeat(100_000);
    fs::write(
        &target,
        description(
            &[
                &format!("Purpose: {purpose}"),
                "Work items: WI-1",
                "Risk tier: standard",
                "Criterion: a docs change",
            ],
            &header_line(&head),
        ),
    )
    .expect("write");

    let output = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--bundle",
        target.to_str().expect("UTF-8"),
    ]);

    assert_output(&output, 0, "", "");
    let record = install.note(&head);
    assert!(note_size(&install, &head) <= 65_536);
    assert_eq!(record["bundle"]["purpose"], json!("x".repeat(1_000)));
    assert_eq!(
        record["truncated"],
        json!([
            "evidence.uncovered.paths.ranges",
            "evidence.uncovered.paths",
            "route.inputs",
            "bundle"
        ])
    );
    assert_valid(&record, RECORD_SCHEMA);
}

/// The commit message of ten work items whose ids are `ids`.
fn trailers(ids: &[String]) -> String {
    let lines: Vec<String> = ids.iter().map(|id| format!("Work-Item: {id}")).collect();
    format!("docs: edit the guide\n\n{}", lines.join("\n"))
}

/// With `maxBytes: 8192`, ten intent refs of 414 characters, and 20 reviewer
/// names of 255 characters, the record falls to the minimal form: the refs are
/// cut to 150 bytes, the reviewers to 20 names of 100 characters, and the note
/// fits.
#[test]
fn ten_long_refs_and_twenty_long_reviewers_fall_to_the_minimal_record_within_8192_bytes() {
    let install = Install::new("", &[], Some(budget(8_192)));
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    let ids: Vec<String> = (0..10)
        .map(|n| format!("W{n:02}{}", "x".repeat(411)))
        .collect();
    let head = install.commit(&trailers(&ids));
    let names: Vec<String> = (0..20)
        .map(|n| format!("{n:02}{}", "r".repeat(253)))
        .collect();
    let mut args = vec!["--base", &base, "--head", &head];
    for name in &names {
        args.extend(["--reviewer", name]);
    }

    let output = install.record_write(&args);

    assert_output(&output, 0, "", &install.audit_failure_lines(&base));
    let record = install.note(&head);
    assert!(
        note_size(&install, &head) <= 8_192,
        "{}",
        note_size(&install, &head)
    );
    assert_eq!(record["truncated"], json!(["all"]));
    let intents = record["intent"].as_array().expect("a list");
    assert_eq!(intents.len(), 10);
    for (n, intent) in intents.iter().enumerate() {
        assert_eq!(
            intent,
            &json!({"source": "work-item", "ref": format!("W{n:02}{}", "x".repeat(147))})
        );
    }
    let kept: Vec<String> = names
        .iter()
        .map(|name| name.chars().take(100).collect())
        .collect();
    assert_eq!(record["reviewers"], json!(kept));
    assert_valid(&record, RECORD_SCHEMA);
}

/// The minimal record with ten refs of multi-byte and of control characters, the
/// host `gitlab`, an `auto-merge-eligible` route with its sample, and an audited
/// key with a 1,000-byte note: each ref is cut on a character boundary to 150
/// serialized bytes, the audit stays, the note takes at most `maxBytes` less the
/// 4,096 bytes of `--reviewers-pending` (spec 8.4), and the reviewers of 10.4 fit
/// `maxBytes: 8192`.
#[test]
fn the_minimal_record_of_hard_refs_and_an_audited_key_leaves_the_reserve_free() {
    let mut review = auto_merge(0.5);
    review["record"] = json!({"maxBytes": 8_192});
    let install = Install::new("", &[docs_read()], Some(review));
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    let kinds = [
        "é", "日", "😀", "\u{1}", "é", "日", "😀", "\u{1}", "\u{1f}", "\"\\",
    ];
    let ids: Vec<String> = kinds
        .iter()
        .enumerate()
        .map(|(n, kind)| format!("W{n}{}", kind.repeat(150)))
        .collect();
    let head = install.commit(&trailers(&ids));
    install.write_record_printing_the_audit_failures(&base, &head, &["--host", "gitlab"]);
    let mut audited = install.note(&head);
    audited["post_merge_audit"] =
        json!({"verdict": "pass", "by": "b".repeat(100), "note": "n".repeat(1_000)});
    install.set_note(&head, &audited);

    install.write_record_printing_the_audit_failures(
        &base,
        &head,
        &["--host", "gitlab", "--reviewers-pending"],
    );

    let mut record = install.note(&head);
    assert_eq!(record["truncated"], json!(["all"]));
    assert_eq!(record["host"], json!("gitlab"));
    assert_eq!(record["route"]["route"], json!("auto-merge-eligible"));
    assert!(record["sample"].is_object(), "the sample of the route");
    assert_eq!(record["post_merge_audit"], audited["post_merge_audit"]);
    assert!(
        note_size(&install, &head) <= 8_192 - 4_096,
        "the minimal record takes {} bytes",
        note_size(&install, &head)
    );
    let intents = record["intent"].as_array().expect("a list");
    assert_eq!(intents.len(), 10);
    for (n, intent) in intents.iter().enumerate() {
        let reference = intent["ref"].as_str().expect("a ref");
        assert!(
            escaped(reference).len() <= 150,
            "{n}: {} bytes",
            escaped(reference).len()
        );
        let original = &ids[n];
        assert!(original.starts_with(reference), "{n}: a prefix of the id");
        let next = original[reference.len()..]
            .chars()
            .next()
            .expect("a longer id");
        assert!(
            escaped(&format!("{reference}{next}")).len() > 150,
            "{n}: the next character would not fit"
        );
    }
    set_reviewers(&mut record, &emoji_names(25));
    assert_eq!(record["reviewers"].as_array().expect("names").len(), 7);
    assert!(
        note_bytes(&record).len() <= 8_192,
        "the reviewers fill the reserve: {} bytes",
        note_bytes(&record).len()
    );
}

/// Runs the program of the GitLab push job (spec 10.4) with `jq` on the note of
/// `head`, which `record write --reviewers-pending` wrote, with `approvers`.
#[cfg(unix)]
fn edit_reviewers(install: &Install, head: &str, approvers: &[String]) -> Output {
    let unit = ci_support::parse_unit(
        &fs::read_to_string(repo_root().join("ci").join("gitlab-review.yml")).expect("the unit"),
        ci_support::Host::gitlab("root"),
    );
    let program = unit
        .variable("record-push", "HOUSERULES_REVIEWERS_JQ")
        .expect("the reviewers program");
    let input = install.scratch_file("record.json");
    fs::write(&input, install.note_text(head)).expect("write the note");
    Command::new("jq")
        .args([
            "-c",
            "--argjson",
            "approvers",
            &json!(approvers).to_string(),
            &program,
        ])
        .arg(&input)
        .output()
        .expect("run jq")
}

/// The program of the GitLab push job (spec 10.4), run with `jq` on a record
/// written with `--reviewers-pending`: 25 approvers give 20 sorted names and
/// `reviewers_more: 5`, the record validates and fits `maxBytes`, and `record
/// show` accepts it. Unix only: the units run `jq` on Linux images only.
#[cfg(unix)]
#[test]
fn the_gitlab_reviewers_edit_keeps_the_record_valid_and_within_the_budget() {
    let install = Install::new("", &[], Some(budget(8_192)));
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let output = install.record_write(&["--base", &base, "--head", &head, "--reviewers-pending"]);
    assert_output(&output, 0, "", "");
    let approvers: Vec<String> = (0..25)
        .rev()
        .map(|n| format!("{n:02}{}", "r".repeat(253)))
        .collect();

    let edited = edit_reviewers(&install, &head, &approvers);

    assert!(edited.status.success(), "{}", stderr(&edited));
    let record: Value = serde_json::from_slice(&edited.stdout).expect("JSON");
    let mut sorted = approvers.clone();
    sorted.sort();
    let expected: Vec<String> = sorted[..20]
        .iter()
        .map(|name| name.chars().take(100).collect())
        .collect();
    assert_eq!(record["reviewers"], json!(expected));
    assert_eq!(record["reviewers_more"], json!(5));
    assert_eq!(record["reviewers_pending"], json!(false));
    assert_valid(&record, RECORD_SCHEMA);
    assert!(
        edited.stdout.len() <= 8_192,
        "{} bytes",
        edited.stdout.len()
    );
    install.set_note(&head, &record);
    let shown = install.record_show(&[&head]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    assert!(
        stdout(&shown).contains("### Reviewers\n\n`0"),
        "{}",
        stdout(&shown)
    );
}

/// Usernames of GitLab may hold emoji, which take four bytes each: 20 approvers
/// of 100 emoji would fill 8,000 bytes. The program keeps the names that fit
/// 3,072 bytes of the serialized array by `utf8bytelength`, drops the others
/// from the end, and counts them in `reviewers_more` (spec 10.4). The record
/// validates, fits `maxBytes: 8192`, and `record show` accepts it. Unix only:
/// the units run `jq` on Linux images only.
#[cfg(unix)]
#[test]
fn the_gitlab_reviewers_edit_keeps_no_more_names_than_fit_3072_bytes() {
    let install = Install::new("", &[], Some(budget(8_192)));
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let output = install.record_write(&["--base", &base, "--head", &head, "--reviewers-pending"]);
    assert_output(&output, 0, "", "");
    let names = emoji_names(20);
    let approvers: Vec<String> = names.iter().rev().cloned().collect();

    let edited = edit_reviewers(&install, &head, &approvers);

    assert!(edited.status.success(), "{}", stderr(&edited));
    let record: Value = serde_json::from_slice(&edited.stdout).expect("JSON");
    assert_eq!(
        record["reviewers"],
        json!(names[..7]),
        "7 names of 402 bytes"
    );
    assert_eq!(record["reviewers_more"], json!(13));
    assert_eq!(record["reviewers_pending"], json!(false));
    let serialized = serde_json::to_string(&record["reviewers"]).expect("serialize");
    assert!(serialized.len() <= 3_072, "{} bytes", serialized.len());
    assert!(
        serde_json::to_string(&names[..8]).expect("serialize").len() > 3_072,
        "the 8th name would not fit"
    );
    let (kept, left_out) = kept_reviewers(&approvers);
    assert_eq!(
        record["reviewers"],
        json!(kept),
        "the Rust rule of `--reviewer`"
    );
    assert_eq!(record["reviewers_more"], json!(left_out));
    assert_valid(&record, RECORD_SCHEMA);
    assert!(
        edited.stdout.len() <= 8_192,
        "{} bytes",
        edited.stdout.len()
    );
    install.set_note(&head, &record);
    let shown = install.record_show(&[&head]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
}

/// A cut record shows what it kept: the summary of a minimal record says that
/// the packet is not kept and names the cut.
#[test]
fn record_show_of_a_minimal_record_says_that_the_packet_is_not_kept() {
    let install = Install::new("", &[], Some(budget(8_192)));
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    let ids: Vec<String> = (0..10)
        .map(|n| format!("W{n:02}{}", "x".repeat(411)))
        .collect();
    let head = install.commit(&trailers(&ids));
    install.write_record_printing_the_audit_failures(&base, &head, &[]);

    let output = install.record_show(&[&head]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.contains("The packet is not kept: the record was cut to its minimal form.\n"),
        "{text}"
    );
    assert!(
        text.ends_with(
            "### Cut\n\nThe record was cut to fit review.record.maxBytes. These fields were shortened:\n\n- `all`\n"
        ),
        "{text}"
    );
}

// ---- an audit that cannot run ---------------------------------------------------------------------------------

/// An audit that cannot run (a runner tool that is not installed) does not stop
/// the record: `audit.error` holds its line, the command prints the line on
/// stderr and exits 0, and the route is the floor, which reads no audit row.
#[test]
fn an_audit_that_cannot_run_is_recorded_as_an_error_and_exits_0() {
    let runner = entry(
        "team.lint",
        json!({"type": "runner", "level": "fail", "tool": "lint", "files": "docs/*.md"}),
    );
    let install = Install::new("", &[runner], None);
    let mut marker = schema_support::read_json(&install.root().join(".houserules.json"));
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
    let head = install.commit("docs: edit the guide");

    let output = install.record_write(&["--base", &base, "--head", &head]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(stdout(&output), "");
    let record = install.note(&head);
    let line = record["audit"]["error"].as_str().expect("the line");
    assert!(line.contains("install the lint tool"), "{line}");
    assert_eq!(stderr(&output), format!("{line}\n"));
    assert_eq!(record["audit"]["rules"], json!([]));
    assert_eq!(record["route"]["floor"], json!(true));
    assert_eq!(record["route"]["route"], json!("owner-review"));
    assert_valid(&record, RECORD_SCHEMA);
}

/// The line of the JSON tracker whose `backlog/` folder is missing.
const JSON_UNREACHABLE: &str =
    "tracker json cannot be reached: backlog/ is missing; restore backlog/ from version control";

/// A tracker that cannot be reached does not stop the record (spec 8.3): the
/// reference summary of the audit holds its failure line, as `audit --json`
/// gives it, and `record write` prints the line on stderr and exits 0.
#[test]
fn a_tracker_that_cannot_be_reached_is_kept_in_the_reference_summary_and_printed() {
    let install = Install::new("", &[], None);
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide\n\nWork-Item: HR-901");
    fs::remove_dir_all(install.root().join("backlog")).expect("remove backlog/");

    let output = install.record_write(&["--base", &base, "--head", &head]);

    assert_output(&output, 0, "", &format!("{JSON_UNREACHABLE}\n"));
    let record = install.note(&head);
    assert_eq!(
        record["audit"]["references"],
        json!({"checked": 0, "passed": 0, "skipped": 1, "failures": [JSON_UNREACHABLE]})
    );
    assert_eq!(record["audit"]["error"], Value::Null);
    let audit = install.audit_file(&base);
    assert_eq!(
        record["audit"]["references"],
        schema_support::read_json(&audit)["references"],
        "the reference summary of `audit --json`"
    );
    assert_valid(&record, RECORD_SCHEMA);
    let from_file = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--audit",
        audit.to_str().expect("UTF-8"),
    ]);
    assert_output(&from_file, 0, "", &format!("{JSON_UNREACHABLE}\n"));
    assert_eq!(install.note(&head)["audit"], record["audit"]);
}

/// A reference that fails (a work item that the tracker does not hold) is a
/// failure line of the same summary, and the command prints it too.
#[test]
fn a_work_item_that_the_tracker_does_not_hold_is_printed_and_kept_as_a_failure_line() {
    let install = Install::new("", &[], None);
    install.use_json_tracker();
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide\n\nWork-Item: HR-901\nWork-Item: HR-999");

    let output = install.record_write(&["--base", &base, "--head", &head]);

    let line = format!(
        "{}: Work-Item HR-999 does not resolve in the json tracker",
        short(&head)
    );
    assert_output(&output, 0, "", &format!("{line}\n"));
    assert_eq!(
        install.note(&head)["audit"]["references"],
        json!({"checked": 2, "passed": 1, "skipped": 0, "failures": [line]})
    );
}

// ---- the setup errors -------------------------------------------------------------------------------------------

/// `--score` outside 0 to 1 exits 2 with its line.
#[test]
fn a_score_outside_0_to_1_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");

    let output = install.record_write(&["--base", &base, "--head", &head, "--score", "1.5"]);

    assert_output(&output, 2, "", "record: --score 1.5 is outside 0 to 1\n");
    assert!(!install.has_notes_ref());
}

/// `--score` with neither threshold configured exits 2 with the line of the
/// route (spec 6.2, rule 7).
#[test]
fn a_score_with_no_threshold_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");

    let output = install.record_write(&["--base", &base, "--head", &head, "--score", "0.5"]);

    assert_output(
        &output,
        2,
        "",
        "route: --score needs review.route.ownerScore or seniorScore\n",
    );
}

/// An `--intent` that is no `<source>:<ref>`, and one whose source is no adapter.
#[test]
fn an_intent_that_cannot_be_named_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");

    let malformed = install.record_write(&["--base", &base, "--head", &head, "--intent", "oops"]);
    let unknown =
        install.record_write(&["--base", &base, "--head", &head, "--intent", "jira:ABC-1"]);

    assert_output(
        &malformed,
        2,
        "",
        "record: --intent oops: an intent is written <source>:<ref>\n",
    );
    assert_output(
        &unknown,
        2,
        "",
        "record: --intent jira:ABC-1: \"jira\" is no intent source; use one of: work-item, file, openspec\n",
    );
}

/// An `--audit` file of another range exits 2 with the line of spec 4; so does
/// a file that is no audit result.
#[test]
fn an_audit_file_of_another_range_or_no_audit_exits_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let middle = install.commit("docs: edit the guide");
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide again");
    let audit = install.audit_file(&base);
    let not_audit = install.scratch_file("not-audit.json");
    fs::write(&not_audit, "{}\n").expect("write");

    let other = install.record_write(&[
        "--base",
        &middle,
        "--head",
        &head,
        "--audit",
        audit.to_str().expect("UTF-8"),
    ]);
    let no_audit = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--audit",
        not_audit.to_str().expect("UTF-8"),
    ]);

    assert_output(
        &other,
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
    assert_eq!(no_audit.status.code(), Some(2));
    assert!(
        stderr(&no_audit).starts_with(&format!("{}: not an audit result: ", not_audit.display())),
        "{}",
        stderr(&no_audit)
    );
}

/// A `--bundle` file that cannot be read, and an `--out` file that cannot be
/// written, exit 2 with the operating system's own text.
#[test]
fn a_bundle_that_cannot_be_read_and_an_out_that_cannot_be_written_exit_2() {
    let install = Install::new("", &[], None);
    let base = install.head();
    install.append("docs/guide.md", 1);
    let head = install.commit("docs: edit the guide");
    let missing = install.scratch_file("missing.md");
    let directory = install.scratch.path().to_path_buf();

    let bundle = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--bundle",
        missing.to_str().expect("UTF-8"),
    ]);
    let out = install.record_write(&[
        "--base",
        &base,
        "--head",
        &head,
        "--out",
        directory.to_str().expect("UTF-8"),
    ]);

    let read_error = fs::read_to_string(&missing).expect_err("no such file");
    let write_error = fs::write(&directory, b"").expect_err("a directory");
    assert_output(
        &bundle,
        2,
        "",
        &format!("{}: {read_error}\n", missing.display()),
    );
    assert_output(
        &out,
        2,
        "",
        &format!("{}: {write_error}\n", directory.display()),
    );
}

/// A base that is no commit exits 2 with git's reading of the ref.
#[test]
fn a_base_that_is_no_commit_exits_2() {
    let install = Install::new("", &[], None);
    let head = install.head();

    let output = install.record_write(&["--base", "nope", "--head", &head]);

    assert_output(&output, 2, "", "bad ref \"nope\"\n");
}

/// A git older than 2.41 stops the command with the line of spec 4.2; the shim
/// answers a call with `--version` and passes every other call to the real
/// git. Unix only: the shim is a POSIX shell script with an executable bit.
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
        .args([
            "record", "write", "--base", &base, "--head", "HEAD", "--dir",
        ])
        .arg(install.root())
        .env("PATH", std::env::join_paths(paths).expect("join the PATH"))
        .env("HOUSERULES_TEST_REAL_PATH", real_path)
        .output()
        .expect("run record write");

    assert_output(
        &output,
        2,
        "",
        "record write: git 2.39.0 is older than 2.41, which --attr-source needs\n",
    );
}

// ---- the validation capture (batch 38, spec 4; case 3) ---------------------------------

/// An install whose base commit holds nothing of OpenSpec and whose next commit adds the
/// active changes `ids`, each with a proposal (the title `Change <id>`) and a task list. It
/// gives the install, the base commit, and the head. It stays in this file: it builds the
/// `Install` type of this file.
fn openspec_changes(ids: &[&str]) -> (Install, String, String) {
    let install = Install::new("", &[], Some(openspec_review()));
    let base = install.head();
    for id in ids {
        install.write(
            &format!("openspec/changes/{id}/proposal.md"),
            &format!("# Change {id}\n\n## Why\n\nBecause {id}.\n"),
        );
        install.write(
            &format!("openspec/changes/{id}/tasks.md"),
            "- [x] a\n- [ ] b\n",
        );
    }
    let head = install.commit("feat: add the changes");
    (install, base, head)
}

/// Writes `text` as the file `name` of the scratch directory and gives its path. It stays in
/// this file: it reads the scratch directory of the `Install` type of this file.
fn capture_file(install: &Install, name: &str, text: &str) -> PathBuf {
    let path = install.scratch_file(name);
    fs::write(&path, text).expect("write the capture");
    path
}

/// The line that `record write` prints for a capture that it refuses (spec 4.1).
fn refusal_line(path: &Path, reason: &str) -> String {
    format!("record: --validation {}: {reason}\n", path.display())
}

/// `record write --out <name>` over the range with the flags `extra`, and the file that it
/// wrote.
fn write_to_file(install: &Install, base: &str, head: &str, extra: &[&str]) -> (Output, PathBuf) {
    let target = install.scratch_file("record-out.json");
    let mut args = vec!["--base", base, "--head", head, "--out", arg(&target)];
    args.extend_from_slice(extra);
    (install.record_write(&args), target)
}

/// A valid run fills the validation of the `openspec` intent of the record, as it fills the
/// intent of the packet: the note holds `{tool, ok, summary}`, the note validates against its
/// schema, and `record show` prints the line of the packet (spec 4.2). The command prints
/// nothing.
#[test]
fn a_valid_run_fills_the_validation_of_the_intent_of_the_record() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    let capture = capture_file(
        &install,
        "validation.json",
        &clean_capture(
            &head,
            &document_of(&[item("add-x", "change", false, &["ERROR"])]),
        ),
    );

    install.write_record_with(&base, &head, &["--validation", arg(&capture)]);

    let record = install.note(&head);
    assert_eq!(validations(&record), [validation(false, "1 issue")]);
    assert_eq!(
        keys(&record["intent"][0]["validation"]),
        ["tool", "ok", "summary"]
    );
    assert_valid(&record, RECORD_SCHEMA);
    let evidence = install.evidence_json(&base, &head, &["--validation", arg(&capture)]);
    assert_eq!(record["intent"], evidence["intent"]);
    let shown = install.record_show(&[head.as_str()]);
    assert_eq!(shown.status.code(), Some(0), "{}", stderr(&shown));
    assert!(
        stdout(&shown)
            .lines()
            .any(|line| line == "Validation: `openspec` failed: `1 issue`"),
        "{}",
        stdout(&shown)
    );
}

/// The summary counts every issue of every level (spec 4.2), and an item with no issue reads
/// `no issues`.
#[test]
fn the_summary_of_the_record_counts_every_issue_of_every_level() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    for (valid, levels, summary) in [
        (true, vec![], "no issues"),
        (true, vec!["WARNING"], "1 issue"),
        (false, vec!["ERROR", "WARNING"], "2 issues"),
        (false, vec!["ERROR", "INFO", "WARNING"], "3 issues"),
    ] {
        let capture = capture_file(
            &install,
            "validation.json",
            &clean_capture(
                &head,
                &document_of(&[item("add-x", "change", valid, &levels)]),
            ),
        );

        let (output, target) =
            write_to_file(&install, &base, &head, &["--validation", arg(&capture)]);

        assert_output(&output, 0, "", "");
        let record = schema_support::read_json(&target);
        assert_eq!(validations(&record), [validation(valid, summary)]);
    }
}

/// A capability that shares the id of the change is another item, and a run with no item for
/// the change leaves `validation: null` (spec 4.2).
#[test]
fn the_record_takes_the_item_of_the_change_and_no_other() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    let capability = item("add-x", "spec", false, &["ERROR", "ERROR"]);
    let change = item("add-x", "change", true, &[]);
    for (items, expected) in [
        (
            vec![capability.clone(), change],
            validation(true, "no issues"),
        ),
        (vec![capability], Value::Null),
        (vec![item("other", "change", true, &[])], Value::Null),
    ] {
        let capture = capture_file(
            &install,
            "validation.json",
            &clean_capture(&head, &document_of(&items)),
        );

        let (output, target) =
            write_to_file(&install, &base, &head, &["--validation", arg(&capture)]);

        assert_output(&output, 0, "", "");
        assert_eq!(validations(&schema_support::read_json(&target)), [expected]);
    }
}

/// A capture that `record write` cannot use is no error (spec 4.1): it prints
/// `record: --validation <path>: <reason>` on stderr, keeps every `validation` `null`, writes
/// the record that the same range gives without the flag, and exits 0. Each reason is pinned
/// whole.
#[test]
fn a_capture_that_cannot_be_used_prints_its_line_and_the_record_is_written() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    let (output, target) = write_to_file(&install, &base, &head, &[]);
    assert_output(&output, 0, "", "");
    let baseline = fs::read(&target).expect("read the record");
    let other = "9".repeat(40);
    for refusal in validation_support::refusals(&head, &other) {
        let path = capture_file(&install, "validation.json", &refusal.file);

        let (output, target) = write_to_file(&install, &base, &head, &["--validation", arg(&path)]);

        assert_eq!(
            output.status.code(),
            Some(0),
            "{}: {}",
            refusal.name,
            stderr(&output)
        );
        assert_eq!(stdout(&output), "", "{}", refusal.name);
        assert_eq!(
            stderr(&output),
            refusal_line(&path, &refusal.reason),
            "{}",
            refusal.name
        );
        assert_eq!(
            fs::read(&target).expect("read the record"),
            baseline,
            "{}",
            refusal.name
        );
    }
}

/// A capture file that the operating system cannot read gives the line with the error of the
/// operating system, verbatim, and the record is written (spec 4.1).
#[test]
fn a_capture_that_cannot_be_read_prints_the_error_of_the_operating_system() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    let (_, target) = write_to_file(&install, &base, &head, &[]);
    let baseline = fs::read(&target).expect("read the record");
    for path in [
        install.scratch_file("missing.json"),
        install.scratch.path().to_path_buf(),
    ] {
        let error = fs::read(&path).expect_err("the path cannot be read");

        let (output, target) = write_to_file(&install, &base, &head, &["--validation", arg(&path)]);

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(
            stderr(&output),
            refusal_line(&path, &format!("cannot be read ({error})"))
        );
        assert_eq!(fs::read(&target).expect("read the record"), baseline);
    }
}

/// A capture that `record write` refuses prints its line on every path that follows, before the
/// failure lines, and the exit stays as without the flag (spec 4.1): an `--out` into a folder
/// that does not exist stops the run with the line of the write and the exit 2. Each reason is
/// pinned whole.
#[test]
fn a_refused_capture_prints_its_line_before_the_failure_of_a_write() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    let captures = refused_captures(install.scratch.path(), &head, &"9".repeat(40));
    let target = install.scratch_file("no").join("such").join("target");
    let error = fs::write(&target, "").expect_err("the folder does not exist");
    let write_error = format!("{}: {error}\n", target.display());
    let without = install.record_write(&["--base", &base, "--head", &head, "--out", arg(&target)]);
    assert_output(&without, 2, "", &write_error);

    for capture in &captures {
        let output = install.record_write(&[
            "--base",
            &base,
            "--head",
            &head,
            "--validation",
            arg(&capture.path),
            "--out",
            arg(&target),
        ]);

        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (
                Some(2),
                String::new(),
                format!(
                    "{}{write_error}",
                    refusal_line(&capture.path, &capture.reason)
                )
            ),
            "{}",
            capture.name
        );
    }
}

/// A capture that `record write` refuses prints its line before the refusal of a key that holds
/// a record of another range, and the exit stays 1 and the note stays (spec 4.1).
#[test]
fn a_refused_capture_prints_its_line_before_the_refusal_of_another_range() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    install.write_record(&base, &head);
    let before = install.note_text(&head);
    install.append("docs/guide.md", 1);
    let later = install.commit("docs: edit the guide");
    let other = "9".repeat(40);
    let line = format!(
        "record: {} already holds a record of another range\n",
        short(&head)
    );
    let without = install.record_write(&["--base", &base, "--head", &later, "--key", &head]);
    assert_output(&without, 1, "", &line);

    for capture in refused_captures(install.scratch.path(), &later, &other) {
        let output = install.record_write(&[
            "--base",
            &base,
            "--head",
            &later,
            "--key",
            &head,
            "--validation",
            arg(&capture.path),
        ]);

        assert_eq!(
            (output.status.code(), stdout(&output), stderr(&output)),
            (
                Some(1),
                String::new(),
                format!("{}{line}", refusal_line(&capture.path, &capture.reason))
            ),
            "{}",
            capture.name
        );
        assert_eq!(install.note_text(&head), before, "{}", capture.name);
    }
}

/// The capture binds to the head of the record, not to the commit that is checked out: a
/// capture of `--head` is used while a later commit is checked out, and a capture of the
/// checked-out commit is refused (spec 4.1;
/// `houserules.commit-bound-documents-read-the-commit`).
#[test]
fn the_capture_binds_to_the_head_of_the_record_and_not_to_the_checked_out_commit() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    let later = install.commit("docs: later");
    let items = document_of(&[item("add-x", "change", true, &["WARNING"])]);
    let at_head = capture_file(&install, "at-head.json", &clean_capture(&head, &items));
    let at_later = capture_file(&install, "at-later.json", &clean_capture(&later, &items));

    let (used, target) = write_to_file(&install, &base, &head, &["--validation", arg(&at_head)]);
    let validations_used = validations(&schema_support::read_json(&target));
    let (refused, _) = write_to_file(&install, &base, &head, &["--validation", arg(&at_later)]);

    assert_output(&used, 0, "", "");
    assert_eq!(validations_used, [validation(true, "1 issue")]);
    assert_eq!(refused.status.code(), Some(0), "{}", stderr(&refused));
    assert_eq!(
        stderr(&refused),
        refusal_line(
            &at_later,
            &format!("ran at {}, not at {}", short(&later), short(&head))
        )
    );
}

/// A second run over the same range and the same capture writes the same bytes.
#[test]
fn the_same_capture_writes_the_same_record_twice() {
    let (install, base, head) = openspec_changes(&["add-x"]);
    let capture = capture_file(
        &install,
        "validation.json",
        &clean_capture(
            &head,
            &document_of(&[item("add-x", "change", false, &["ERROR"])]),
        ),
    );

    let (_, target) = write_to_file(&install, &base, &head, &["--validation", arg(&capture)]);
    let first = fs::read(&target).expect("read the record");
    let (_, target) = write_to_file(&install, &base, &head, &["--validation", arg(&capture)]);

    assert_eq!(fs::read(&target).expect("read the record"), first);
}

/// The `record:` prefix of the line is in the spec (`quality.user-facing-text-is-contract`).
#[test]
fn the_validation_prefix_of_these_tests_is_in_the_spec() {
    let spec = fs::read_to_string(
        repo_root()
            .join("docs")
            .join("specs")
            .join("2026-10-25-batch-38-openspec-and-release.md"),
    )
    .expect("read the spec");

    assert!(spec.contains("(`record:` for `record write`)"));
}

// ---- the help ---------------------------------------------------------------------------------------------------------

/// The sentences of the `about` of `record`.
const ABOUT: [&str; 3] = [
    "Keeps a change record for each merge as a git note.",
    "`record write` computes the record of a pushed range and writes it as a note, or to a file.",
    "`record show` prints the record of a commit and checks its rule hashes against git",
];

/// The sentences of the `about` of `record write`.
const WRITE_ABOUT: [&str; 18] = [
    "Computes the change record of the range `--base` to `--head` and writes it as a git note on `--key` (default: `--head`) under the notes ref of the install: `refs/notes/houserules` for a top-level install, else `refs/notes/houserules-sub/<install path>`.",
    "`--out` writes the bytes of the note to a file in place of the note; only the CI units push the ref.",
    "The record holds the commits of the range, the rule hashes at the merge base and at the head, the audit rows, the intents, the evidence, the route, the sample draw, the evidence bundle in `--bundle` (the description of the request), and the reviewers.",
    "The audit rows come from `--audit`, else from an audit that the command runs in process.",
    "`--intent <source>:<ref>` names an intent, else the `Intent:` line of the bundle does.",
    "An `Intent: <source>:<ref>` trailer of a commit of the range adds one too, and a trailer with a key of `review.sessions.trailers` (default `Entire-Checkpoint`) is a session of `links.sessions`; a trailer never makes the command fail.",
    "`--validation <file>` names a capture of `openspec validate --json` that `houserules capture` wrote at `--head` in a clean work tree, and each `openspec` intent takes its validation from it.",
    "A capture that cannot be used prints one line on stderr and leaves every validation `null`; the record is written, and the exit stays as without the flag.",
    "`--reviewer` names one approver; repeat the flag.",
    "`--reviewers-pending` writes no reviewers and leaves room for them.",
    "A later job may set them, or the approvers could not be read and stay unknown.",
    "`--score` adds an external score to the route.",
    "`--host` records the host.",
    "A record over `review.record.maxBytes` is cut in a fixed order, and `truncated` names each cut.",
    "A key that holds a record of the same range gets the new record, and its post-merge audit stays.",
    "A key that holds a record of another range exits 1.",
    "An audit that cannot run, or a tracker that cannot be reached, does not stop the record: `audit.error` holds the line of the audit, `audit.references` holds the failure lines of the references, the command prints each line on stderr, and the exit is 0.",
    "It exits 2 for a setup error, for example: a ref that is no commit, an `--audit` file of another range or that is no audit result, a `--bundle` file that cannot be read, an `--intent` whose source is no adapter, a `--score` outside 0 to 1, a `review` key with a fault, a git older than 2.41, or an `--out` file that cannot be written",
];

/// The sentences of the `about` of `record show`.
const SHOW_ABOUT: [&str; 6] = [
    "Prints the change record of a commit: the record that the commit keys, else the record whose range holds the commit.",
    "It prints a Markdown summary: the packet of the change, the route, the sample, the reviewers, and the post-merge audit.",
    "`--json` prints the record document `houserules/change-record@1.1.0` in place of it.",
    "It reads the notes ref of the install, and recomputes the rule hashes at the merge base and at the head from git.",
    "It exits 1 for a clone with no notes ref, for a commit with no record, and for a record whose rule hash differs from git's.",
    "It exits 2 when the merge base of the record is not in the clone, and for a setup error, for example a ref that is no commit or a note that is no change record",
];

/// The help text of each argument of `record write`.
const WRITE_ARGUMENTS: [&str; 13] = [
    "The base ref of the range. The range runs from the merge base of `--base` and `--head` to `--head`. git's empty tree starts the range at the first commit",
    "The head ref of the range",
    "The commit that the note annotates; defaults to `--head`. For a squash or a rebase merge it is the last commit of the push",
    "The file that `audit --json` wrote for the same range. Its `base` and `head` must name the same commits as `--base` and `--head`. Without this flag, the command runs the audit in process",
    "The file that holds the description of the pull or merge request. The record keeps its evidence bundle. Without this flag, or with a description that has no bundle section, the record has no bundle",
    "The intent to add, written `<source>:<ref>` as the `Intent:` line of an evidence bundle writes it. The source is `work-item`, `file`, or `openspec`. Without this flag, the `Intent:` line of the bundle names it",
    "The capture file of an `openspec validate --json` run, written by `houserules capture` at `--head` in a clean work tree. Each `openspec` intent takes the validation of its change from it. A capture that cannot be used prints one line on stderr and leaves every validation `null`",
    "The login of an approver; repeat the flag. The record keeps the names sorted and without repeats, the first 20 of them, each cut to 100 characters, no more names than fit 3,072 bytes of the serialized array, and the count of the names left out",
    "Writes no reviewers and marks them pending: a later job may set them, or the approvers could not be read and stay unknown. It budgets the note 4,096 bytes below `review.record.maxBytes`, the room that a later job may fill with at most 3,072 bytes of names. The minimal record takes at most the rest at every cap. It cannot be combined with `--reviewer`",
    "An external score from 0 to 1 for the route. It needs `review.route.ownerScore` or `review.route.seniorScore`",
    "The host that the record names: `github` or `gitlab`. Without this flag, the record names none",
    "Writes the bytes of the note to this file in place of the note",
    "Install root to read from; defaults to the nearest directory at or above the current one that holds `.houserules.json`, no higher than the git top level, else the git top level",
];

/// The help text of each argument of `record show`.
const SHOW_ARGUMENTS: [&str; 3] = [
    "The commit to show: a commit that a record keys, or one in the range of a record",
    "Prints the record document in place of the Markdown summary",
    "Install root to read from; defaults to the nearest directory at or above the current one that holds `.houserules.json`, no higher than the git top level, else the git top level",
];

/// The three `--help` texts state the record, the exits, and each condition of
/// an argument (`quality.user-facing-text-is-contract`), and `houserules
/// --help` lists the same about text.
#[test]
fn the_help_of_record_states_the_note_the_exits_and_each_argument() {
    states(
        "record --help",
        &help_of_args(&["record", "--help"]),
        &ABOUT,
    );
    states("houserules --help", &help_of_args(&["--help"]), &ABOUT);
    let write = help_of_args(&["record", "write", "--help"]);
    states("record write --help", &write, &WRITE_ABOUT);
    states("record write --help", &write, &WRITE_ARGUMENTS);
    let show = help_of_args(&["record", "show", "--help"]);
    states("record show --help", &show, &SHOW_ABOUT);
    states("record show --help", &show, &SHOW_ARGUMENTS);
}
