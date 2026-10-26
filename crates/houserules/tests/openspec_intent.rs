//! The `openspec` intent adapter (batch 38, spec 3; acceptance cases 1 and 2;
//! HR-209). Each case runs the compiled binary against a scratch repository
//! and reads the `intent` list of `evidence --json` whole: every field of every
//! intent, and every line of spec 3.5 as the packet prints it.
//!
//! The expected values come from OpenSpec 1.15.0 where its binary printed them
//! (`fixtures/openspec/outputs/`, and the figures of its edge probes, named at
//! each fixture), from the rules of spec 3 where it prints none (the why), and
//! from the lines of spec 3.5 for a fixture that only houserules defines. No
//! test runs OpenSpec.

mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};
use tempfile::TempDir;
use tracker_support::{
    assert_output, commit_all, copy_dir, git, houserules, repo_root, scratch_git_repo, stderr,
    stdout,
};

/// The line for a tree with no `openspec/` folder (spec 3.5).
const NO_FOLDER: &str = "no openspec/ folder at the head";

/// The line for a root that points at a store (spec 3.5).
const STORE_POINTER: &str = "the OpenSpec root points at a store, which houserules does not read";

/// The line for a change id that is not kebab-case (spec 3.5).
const NOT_KEBAB: &str =
    "an OpenSpec change id is kebab-case: lowercase letters, digits, and single hyphens";

/// The line for a change that the tree does not hold (spec 3.5).
const NO_SUCH_CHANGE: &str = "no OpenSpec change with that id at the head, active or archived";

/// The line for an intent source that is no adapter (spec 3.6).
const UNKNOWN_SOURCE: &str =
    "\"<source>\" is no intent source; use one of: work-item, file, openspec";

/// The file of the spec of batch 38, below `docs/specs/`.
const SPEC_FILE: &str = "2026-10-25-batch-38-openspec-and-release.md";

/// The archived change that `edges/plain/` holds.
const ARCHIVED_EDGE: &str = "2026-01-02-old-change";

// ---- fixtures -------------------------------------------------------------------------

/// The folder `relative` below `tests/fixtures/openspec/`, built component by
/// component (`houserules.path-pins-mirror-the-code`).
fn fixture(relative: &str) -> PathBuf {
    relative.split('/').fold(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("openspec"),
        |path, part| path.join(part),
    )
}

/// The files below `dir` as `/`-separated paths from `dir`, sorted. A name that
/// starts with `.` counts.
fn files_below(dir: &Path) -> Vec<String> {
    fn walk(dir: &Path, parts: &mut Vec<String>, found: &mut Vec<String>) {
        for entry in fs::read_dir(dir).expect("read the fixture folder") {
            let entry = entry.expect("an entry");
            parts.push(entry.file_name().to_string_lossy().into_owned());
            if entry.file_type().expect("a file type").is_dir() {
                walk(&entry.path(), parts, found);
            } else {
                found.push(parts.join("/"));
            }
            parts.pop();
        }
    }
    let mut found = Vec::new();
    walk(dir, &mut Vec::new(), &mut found);
    found.sort();
    found
}

/// The folder of the change `reference` in the fixtures, from the install root.
fn folder_of(reference: &str) -> String {
    if reference == ARCHIVED_EDGE {
        format!("openspec/changes/archive/{reference}")
    } else {
        format!("openspec/changes/{reference}")
    }
}

/// The paths of the files of the change folder `folder` of the fixture `name`,
/// from the install root, sorted: `openspec/changes/<id>/proposal.md` and the
/// like. The fixture holds the same files that the project holds.
fn artifacts_of(name: &str, folder: &str) -> Vec<String> {
    let below = folder
        .split('/')
        .fold(fixture(name), |path, part| path.join(part));
    files_below(&below)
        .into_iter()
        .map(|file| format!("{folder}/{file}"))
        .collect()
}

/// A scratch repository with one install and its base commit: the marker with
/// the intent sources `sources`, an empty knowledge base, and `.gitattributes`
/// that turns every line-ending conversion off, so that a fixture with CRLF
/// line ends commits as it is on every host.
struct Project {
    /// The git top level.
    top: TempDir,
    /// The folder of the install below the top level: `""` or `"sub"`.
    folder: &'static str,
    /// A directory outside the repository, for the files that a case reads back.
    scratch: TempDir,
}

impl Project {
    /// A project whose install is in `folder`. The base commit holds the
    /// skeleton and nothing of OpenSpec.
    fn new(folder: &'static str, sources: &[&str]) -> Self {
        let project = Self {
            top: scratch_git_repo(),
            folder,
            scratch: tempfile::tempdir().expect("tempdir"),
        };
        fs::write(project.top.path().join(".gitattributes"), "* -text\n")
            .expect("write the attributes");
        let marker = json!({
            "schema": "houserules/install@2.3.0",
            "idPrefix": "WI",
            "review": {"intent": {"sources": sources}},
        });
        project.write(
            ".houserules.json",
            &serde_json::to_string_pretty(&marker).expect("serialize the marker"),
        );
        project.write("knowledge/schema.json", "{}\n");
        project.write(
            "knowledge/areas.json",
            &json!({"global": {"paths": []}}).to_string(),
        );
        project.write("knowledge/team.json", &json!({"entries": []}).to_string());
        project.commit("chore: base");
        project
    }

    /// The install root.
    fn root(&self) -> PathBuf {
        if self.folder.is_empty() {
            self.top.path().to_path_buf()
        } else {
            self.top.path().join(self.folder)
        }
    }

    /// The file `relative` below `base`, built component by component.
    fn path_in(base: &Path, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(base.to_path_buf(), |path, part| path.join(part))
    }

    /// Writes `content` to `path` below the install root, with its folders.
    fn write(&self, path: &str, content: &str) {
        let target = Self::path_in(&self.root(), path);
        fs::create_dir_all(target.parent().expect("a parent")).expect("create the folders");
        fs::write(target, content).expect("write the file");
    }

    /// Copies the fixture folder `name` into `target`.
    fn add_fixture_to(name: &str, target: &Path) {
        copy_dir(&fixture(name), target);
    }

    /// Copies the fixture folder `name` into the install root.
    fn add_fixture(&self, name: &str) {
        Self::add_fixture_to(name, &self.root());
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

    /// Runs `evidence <args>` over the install.
    fn evidence(&self, args: &[&str]) -> Output {
        houserules()
            .arg("evidence")
            .args(args)
            .arg("--dir")
            .arg(self.root())
            .output()
            .expect("run evidence")
    }

    /// The packet and the `intent` list of `evidence --base <base> <extra>`.
    /// The run exits 0 and prints nothing on stderr.
    fn packet_and_intents(&self, base: &str, extra: &[&str]) -> (String, Vec<Value>) {
        let target = self.scratch_file("evidence.json");
        let mut args = vec!["--base", base, "--json", target.to_str().expect("UTF-8")];
        args.extend_from_slice(extra);
        let output = self.evidence(&args);
        assert_output(&output, 0, &stdout(&output), "");
        let document: Value =
            serde_json::from_str(&fs::read_to_string(&target).expect("read the document"))
                .expect("parse the document");
        let intents = document["intent"]
            .as_array()
            .expect("the document holds the intent list")
            .clone();
        (stdout(&output), intents)
    }

    /// The `intent` list of `evidence --base <base> <extra>`.
    fn intents(&self, base: &str, extra: &[&str]) -> Vec<Value> {
        self.packet_and_intents(base, extra).1
    }
}

/// An intent of the list, as `evidence --json` prints a change that resolves.
/// `requirements` are the counts `added`, `modified`, `removed`, and `renamed`.
fn intent(
    reference: &str,
    title: &str,
    why: Option<&str>,
    tasks: Option<(u64, u64)>,
    requirements: Option<[u64; 4]>,
    artifacts: &[String],
) -> Value {
    json!({
        "source": "openspec",
        "ref": reference,
        "title": title,
        "why": why,
        "tasks": tasks.map(|(done, total)| json!({"done": done, "total": total})),
        "requirements": requirements.map(|[added, modified, removed, renamed]| json!({
            "added": added,
            "modified": modified,
            "removed": removed,
            "renamed": renamed,
            "total": added + modified + removed + renamed,
        })),
        "validation": null,
        "artifacts": artifacts,
    })
}

/// A named intent that does not resolve, as `evidence --json` prints it: every
/// field `null`, no artifact, and the line of the adapter.
fn unresolved(reference: &str, line: &str) -> Value {
    json!({
        "source": "openspec",
        "ref": reference,
        "title": null,
        "why": null,
        "tasks": null,
        "requirements": null,
        "validation": null,
        "artifacts": [],
        "unresolved": line,
    })
}

// ---- the spec -------------------------------------------------------------------------

/// Every line of the spec that these tests pin is in the spec file
/// (`quality.user-facing-text-is-contract`).
#[test]
fn the_spec_lines_of_these_tests_are_in_the_spec() {
    let spec = fs::read_to_string(repo_root().join("docs").join("specs").join(SPEC_FILE))
        .expect("read the spec");

    for line in [
        NO_FOLDER,
        STORE_POINTER,
        NOT_KEBAB,
        NO_SUCH_CHANGE,
        UNKNOWN_SOURCE,
    ] {
        assert!(spec.contains(&format!("`{line}`")), "{line}");
    }
}

/// The fixtures that hold bytes a checkout may convert keep them: the BOM of a
/// proposal and the CRLF of a delta (`.gitattributes` marks every fixture
/// `-text`).
#[test]
fn the_byte_fixtures_keep_their_bytes() {
    let bom = fs::read(fixture(
        "edges/plain/openspec/changes/bom-title/proposal.md",
    ))
    .expect("read the BOM proposal");
    let crlf = fs::read(fixture(
        "edges/plain/openspec/changes/crlf/specs/cap/spec.md",
    ))
    .expect("read the CRLF delta");

    assert!(bom.starts_with(&[0xef, 0xbb, 0xbf]));
    let line_feeds = crlf.iter().filter(|byte| **byte == b'\n').count();
    assert!(line_feeds > 0);
    assert_eq!(
        crlf.windows(2).filter(|pair| *pair == b"\r\n").count(),
        line_feeds
    );
}

// ---- case 1: the edge fixtures --------------------------------------------------------

/// One change of `edges/plain/openspec/changes/` and what the adapter gives.
struct Want {
    /// The `ref`, which is the folder name.
    reference: &'static str,
    /// The title.
    title: &'static str,
    /// The why.
    why: Option<&'static str>,
    /// The task counts.
    tasks: Option<(u64, u64)>,
    /// The requirement counts.
    requirements: Option<[u64; 4]>,
}

/// The changes of `edges/plain/`, in the order of their refs. Where OpenSpec
/// 1.15.0's binary printed a value for the same layout, the comment names it.
fn plain() -> Vec<Want> {
    let want = |reference, title, why, tasks, requirements| Want {
        reference,
        title,
        why,
        tasks,
        requirements,
    };
    vec![
        // `validate --archived --json` prints this id.
        want(ARCHIVED_EDGE, "Old", Some("Because."), Some((1, 1)), None),
        // A change and a capability with the id `auth`: `list` shows 0 of 1 tasks.
        want(
            "auth",
            "Auth change",
            Some("Because."),
            Some((0, 1)),
            Some([1, 0, 0, 0]),
        ),
        // `show` prints the title `bom-title`: the BOM keeps the heading from matching.
        want(
            "bom-title",
            "bom-title",
            Some("Because."),
            Some((1, 1)),
            Some([1, 0, 0, 0]),
        ),
        // A brace range matches as literal text: no file, so `tasks.md` counts
        // (OpenSpec expands the range and counts `task1.md`).
        want("brace-range", "Brace", Some("Because."), Some((0, 1)), None),
        // `show` prints ADDED 1 for the CRLF delta.
        want(
            "crlf",
            "E",
            Some("Because."),
            Some((0, 1)),
            Some([1, 0, 0, 0]),
        ),
        // `list` shows 2 of 3 over `work/a.md` and `work/b.md`.
        want("custom-glob", "F", Some("Because."), Some((2, 3)), None),
        // An extglob matches as literal text: no file, so `tasks.md` counts.
        want("extglob", "Ext", Some("Because."), Some((1, 2)), None),
        // `show` prints the first heading, fence or not, and ADDED 1; `list`
        // shows 1 of 2: the fenced line counts.
        want(
            "fence-tasks",
            "Fenced Title",
            Some("Because."),
            Some((1, 2)),
            Some([1, 0, 0, 0]),
        ),
        // `list` shows 1 of 1: `work/sub/b.md` and `work/.hidden.md` are no match of `work/*.md`.
        want("glob-depth", "Glob", Some("Because."), Some((1, 1)), None),
        // `list` shows 2 of 3 over `todo/one.md`: no `apply` block, so the artifact `tasks`.
        want("no-apply", "F2", Some("Because."), Some((2, 3)), None),
        // `list` shows 1 of 2; `show` exits 1 for the missing What Changes.
        want(
            "no-what",
            "Has Title",
            Some("Because."),
            Some((1, 2)),
            Some([1, 0, 0, 0]),
        ),
        // `show` prints ADDED 1: only `specs/cap/spec.md` is a delta file.
        want(
            "nonspec-delta",
            "B",
            Some("Because."),
            Some((0, 1)),
            Some([1, 0, 0, 0]),
        ),
        // `show` counts the bullets of What Changes; the adapter gives `null`.
        want(
            "prose-deltas",
            "Prose",
            Some("Because."),
            Some((0, 1)),
            None,
        ),
        // `list` shows 1 of 2 over `tasks.md`: the schema is unresolved.
        want("unknown-schema", "G", Some("Because."), Some((1, 2)), None),
        // `show` prints RENAMED 1: one pair, and a `FROM:` line with no `TO:`.
        want(
            "unpaired-rename",
            "D",
            Some("Because."),
            Some((0, 1)),
            Some([0, 0, 0, 1]),
        ),
        // `validate` finds the Why at level 1; its section holds the lines below it.
        want(
            "why-h1",
            "Why",
            Some("Because at level one.\n\n## What Changes\n- x"),
            Some((0, 1)),
            Some([1, 0, 0, 0]),
        ),
        // A subsection belongs to the section of its Why.
        want(
            "why-sub",
            "Sub",
            Some("Because.\n\n### Detail\nMore."),
            Some((0, 1)),
            None,
        ),
    ]
}

/// The intent that the adapter gives for `want`.
fn plain_intent(want: &Want) -> Value {
    intent(
        want.reference,
        want.title,
        want.why,
        want.tasks,
        want.requirements,
        &artifacts_of("edges/plain", &folder_of(want.reference)),
    )
}

/// Case 1: each fixture of `edges/plain/` gives the figures of OpenSpec 1.15.0
/// (the comments of [`plain`]) and the why of spec 3, field by field.
#[test]
fn the_edge_fixtures_fill_every_field_of_the_intent() {
    let project = Project::new("", &["openspec"]);
    let base = project.head();
    project.add_fixture("edges/plain");
    project.commit("docs: add the changes");

    let intents = project.intents(&base, &[]);

    let expected: Vec<Value> = plain().iter().map(plain_intent).collect();
    assert_eq!(intents, expected);
}

/// Case 1: a named `openspec:<id>` gives the intent that the changed paths of
/// the same change give, whatever `review.intent.sources` lists.
#[test]
fn a_named_change_gives_the_intent_that_its_changed_paths_give() {
    let project = Project::new("", &["work-item"]);
    let base = project.head();
    project.add_fixture("edges/plain");
    project.commit("docs: add the changes");

    for want in plain() {
        let named = format!("openspec:{}", want.reference);

        let intents = project.intents(&base, &["--intent", &named]);

        assert_eq!(intents, [plain_intent(&want)], "{}", want.reference);
    }
}

/// Case 1: the config's `schema` decides the tasks of a change with no
/// `.openspec.yaml`; a `.openspec.yaml` with no `schema`, or that does not
/// parse, sends the count to `tasks.md`. OpenSpec 1.15.0's `list` shows 3 of 4,
/// 1 of 2, and 1 of 2 for the three changes. An install below the git top
/// level reads its own folder and names its paths from the install root.
#[test]
fn the_config_schema_decides_the_tasks_of_a_change_with_no_usable_metadata() {
    for folder in ["", "sub"] {
        let project = Project::new(folder, &["openspec"]);
        let base = project.head();
        project.add_fixture("edges/config-schema");
        project.commit("docs: add the changes");

        let intents = project.intents(&base, &[]);

        let change = |name: &str, tasks: (u64, u64)| {
            intent(
                name,
                name,
                Some("Because."),
                Some(tasks),
                None,
                &artifacts_of("edges/config-schema", &format!("openspec/changes/{name}")),
            )
        };
        assert_eq!(
            intents,
            [
                change("meta-absent", (3, 4)),
                change("meta-badyaml", (1, 2)),
                change("meta-noschema", (1, 2)),
            ],
            "install folder {folder:?}"
        );
    }
}

/// Case 1: the four lines of spec 3.5 print whole, as the packet prints them
/// and as the document holds them: no folder (also a root above a
/// subdirectory install, which is not read), a store pointer, an id that is no
/// kebab-case id, and a change that the tree does not hold (also `archive`,
/// which is no change id).
#[test]
fn a_named_intent_that_does_not_resolve_prints_the_line_of_its_adapter() {
    let with_fixture = |name: &str| {
        let project = Project::new("", &["openspec"]);
        project.add_fixture(name);
        project.commit("docs: add the fixture");
        project
    };
    let above_the_install = || {
        let project = Project::new("sub", &["openspec"]);
        Project::add_fixture_to("edges/plain", project.top.path());
        project.commit("docs: add the changes above the install");
        project
    };
    let cases: Vec<(&str, Project, &str, &str)> = vec![
        (
            "no folder",
            Project::new("", &["openspec"]),
            "add-x",
            NO_FOLDER,
        ),
        (
            "above the install",
            above_the_install(),
            "fence-tasks",
            NO_FOLDER,
        ),
        (
            "a store",
            with_fixture("edges/store"),
            "add-x",
            STORE_POINTER,
        ),
        (
            "an underscore",
            with_fixture("edges/plain"),
            "Not_Kebab",
            NOT_KEBAB,
        ),
        (
            "a double hyphen",
            with_fixture("edges/plain"),
            "a--b",
            NOT_KEBAB,
        ),
        (
            "an uppercase letter",
            with_fixture("edges/plain"),
            "Fence-Tasks",
            NOT_KEBAB,
        ),
        (
            "no such change",
            with_fixture("edges/plain"),
            "missing-change",
            NO_SUCH_CHANGE,
        ),
        (
            "the archive folder",
            with_fixture("edges/plain"),
            "archive",
            NO_SUCH_CHANGE,
        ),
    ];

    for (name, project, reference, line) in cases {
        let base = project.head();
        let named = format!("openspec:{reference}");

        let (packet, intents) = project.packet_and_intents(&base, &["--intent", &named]);

        assert_eq!(intents, [unresolved(reference, line)], "{name}");
        let printed = format!("`openspec {reference}`: {line}");
        assert!(
            packet.lines().any(|found| found == printed),
            "{name}: {packet}"
        );
    }
}

/// Case 1: a change that the range archived is one intent, with the archived
/// ref, whether the range names its old folder, its new folder, or both; a
/// named id, with or without its date, resolves to the same intent.
#[test]
fn a_change_archived_in_the_range_is_one_intent_with_the_archived_ref() {
    let project = Project::new("", &["openspec"]);
    project.write(
        "openspec/changes/foo/proposal.md",
        "# Foo\n\n## Why\nBecause.\n",
    );
    project.write("openspec/changes/foo/tasks.md", "- [x] a\n- [ ] b\n");
    let before = project.commit("docs: add foo");
    fs::create_dir_all(Project::path_in(
        &project.root(),
        "openspec/changes/archive",
    ))
    .expect("create the archive folder");
    git(
        project.top.path(),
        &[
            "mv",
            "openspec/changes/foo",
            "openspec/changes/archive/2026-10-26-foo",
        ],
    );
    project.commit("docs: archive foo");
    let archived = vec![
        "openspec/changes/archive/2026-10-26-foo/proposal.md".to_string(),
        "openspec/changes/archive/2026-10-26-foo/tasks.md".to_string(),
    ];
    let expected = intent(
        "2026-10-26-foo",
        "Foo",
        Some("Because."),
        Some((1, 2)),
        None,
        &archived,
    );

    let expected = std::slice::from_ref(&expected);
    assert_eq!(project.intents(&before, &[]), expected);
    assert_eq!(
        project.intents(&before, &["--intent", "openspec:foo"]),
        expected
    );
    assert_eq!(
        project.intents(&before, &["--intent", "openspec:2026-10-26-foo"]),
        expected
    );
}

/// Case 1: with two archived folders of one change, the id without a date
/// resolves to the latest date (and never to another change whose name starts
/// with it); an active folder wins over an archived one.
#[test]
fn a_named_id_resolves_to_the_active_folder_else_the_latest_archived_one() {
    let project = Project::new("", &["work-item"]);
    let base = project.head();
    project.write(
        "openspec/changes/archive/2026-01-02-foo/proposal.md",
        "# Old foo\n",
    );
    project.write(
        "openspec/changes/archive/2026-03-04-foo/proposal.md",
        "# New foo\n",
    );
    project.write(
        "openspec/changes/archive/2026-05-06-foobar/proposal.md",
        "# Foobar\n",
    );
    project.commit("docs: archive foo twice");

    let archived = project.intents(&base, &["--intent", "openspec:foo"]);

    assert_eq!(archived.len(), 1);
    assert_eq!(
        (&archived[0]["ref"], &archived[0]["title"]),
        (&json!("2026-03-04-foo"), &json!("New foo"))
    );

    project.write("openspec/changes/foo/proposal.md", "# Active foo\n");
    project.commit("docs: reopen foo");

    let active = project.intents(&base, &["--intent", "openspec:foo"]);

    assert_eq!(active.len(), 1);
    assert_eq!(
        (&active[0]["ref"], &active[0]["title"]),
        (&json!("foo"), &json!("Active foo"))
    );
}

/// Case 1: a change that the range deleted gives no intent; a change that the
/// range renamed gives one, under its new name.
#[test]
fn a_deleted_change_gives_no_intent_and_a_renamed_change_its_new_name() {
    let project = Project::new("", &["openspec"]);
    project.write("openspec/changes/gone/proposal.md", "# Gone\n");
    project.write(
        "openspec/changes/old-name/proposal.md",
        "# Moved\n\n## Why\nBecause.\n",
    );
    project.write("openspec/changes/old-name/tasks.md", "- [ ] a\n");
    let before = project.commit("docs: add two changes");
    git(
        project.top.path(),
        &["rm", "-r", "-q", "openspec/changes/gone"],
    );
    git(
        project.top.path(),
        &[
            "mv",
            "openspec/changes/old-name",
            "openspec/changes/new-name",
        ],
    );
    project.commit("docs: delete one change and rename the other");

    let intents = project.intents(&before, &[]);

    let artifacts = vec![
        "openspec/changes/new-name/proposal.md".to_string(),
        "openspec/changes/new-name/tasks.md".to_string(),
    ];
    assert_eq!(
        intents,
        [intent(
            "new-name",
            "Moved",
            Some("Because."),
            Some((0, 1)),
            None,
            &artifacts
        )]
    );
}

/// Case 1 and `houserules.commit-bound-documents-read-the-commit`: the adapter
/// reads the tree of `--head`, not the work tree and not the commit that is
/// checked out.
#[test]
fn the_adapter_reads_the_head_and_not_the_work_tree() {
    let project = Project::new("", &["openspec"]);
    let base = project.head();
    project.write(
        "openspec/changes/x/proposal.md",
        "# Original\n\n## Why\nFirst.\n",
    );
    project.write("openspec/changes/x/tasks.md", "- [x] a\n- [ ] b\n");
    let first = project.commit("docs: add x");
    project.write(
        "openspec/changes/x/proposal.md",
        "# Changed\n\n## Why\nSecond.\n",
    );
    project.write("openspec/changes/x/tasks.md", "- [x] a\n- [x] b\n- [x] c\n");
    project.commit("docs: change x");
    project.write(
        "openspec/changes/x/proposal.md",
        "# Dirty\n\n## Why\nThird.\n",
    );

    let at_first = project.intents(&base, &["--head", &first]);
    let at_head = project.intents(&base, &[]);

    let facts = |found: &Value| {
        (
            found["title"].clone(),
            found["why"].clone(),
            found["tasks"].clone(),
        )
    };
    assert_eq!(
        facts(&at_first[0]),
        (
            json!("Original"),
            json!("First."),
            json!({"done": 1, "total": 2})
        )
    );
    assert_eq!(
        facts(&at_head[0]),
        (
            json!("Changed"),
            json!("Second."),
            json!({"done": 3, "total": 3})
        )
    );
}

/// The same inputs give the same bytes: the packet and the document of a run
/// over every fixture are the same on a second run.
#[test]
fn a_second_run_gives_the_same_bytes() {
    let project = Project::new("", &["openspec"]);
    let base = project.head();
    project.add_fixture("edges/plain");
    project.commit("docs: add the changes");
    let document = project.scratch_file("evidence.json");
    let run = || {
        let output =
            project.evidence(&["--base", &base, "--json", document.to_str().expect("UTF-8")]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        (
            output.stdout,
            fs::read(&document).expect("read the document"),
        )
    };

    assert_eq!(run(), run());
}

// ---- case 2: the corpus ---------------------------------------------------------------

/// A row of `expected.json`: what the rules of spec 3 give for one change of
/// the vendored corpus.
struct Row {
    /// The `ref`.
    reference: String,
    /// Whether the change is below `archive/`.
    archived: bool,
    /// The title.
    title: String,
    /// The why.
    why: Option<String>,
    /// The task counts.
    tasks: Option<(u64, u64)>,
    /// The requirement counts.
    requirements: Option<[u64; 4]>,
}

/// The rows of `expected.json`, in the order of the file.
fn rows() -> Vec<Row> {
    let text = fs::read_to_string(fixture("expected.json")).expect("read expected.json");
    let rows: Value = serde_json::from_str(&text).expect("parse expected.json");
    rows.as_array()
        .expect("an array of rows")
        .iter()
        .map(|row| Row {
            reference: row["ref"].as_str().expect("a ref").to_string(),
            archived: row["archived"].as_bool().expect("archived"),
            title: row["title"].as_str().expect("a title").to_string(),
            why: row["why"].as_str().map(str::to_string),
            tasks: row["tasks"].as_object().map(|tasks| {
                (
                    tasks["done"].as_u64().expect("done"),
                    tasks["total"].as_u64().expect("total"),
                )
            }),
            requirements: row["requirements"].as_object().map(|counts| {
                ["added", "modified", "removed", "renamed"]
                    .map(|key| counts[key].as_u64().expect("a count"))
            }),
        })
        .collect()
}

/// The values of OpenSpec 1.15.0's binary for the active change `name` of the
/// corpus: the title of `show`, the task counts of `list` (`None` for a change
/// with no tasks file, where `list` prints 0 of 0), and the delta counts of
/// `show` by operation (`None` for a change whose `show` lists no delta).
fn binary_values(name: &str) -> (String, Option<(u64, u64)>, Option<[u64; 4]>) {
    let outputs = fixture("outputs");
    let list: Value = serde_json::from_str(
        &fs::read_to_string(outputs.join("list.json")).expect("read list.json"),
    )
    .expect("parse list.json");
    let entry = list["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .find(|change| change["name"] == name)
        .unwrap_or_else(|| panic!("list.json holds {name}"));
    let show: Value = serde_json::from_str(
        &fs::read_to_string(outputs.join(format!("show-{name}.json"))).expect("read show"),
    )
    .expect("parse show");
    let has_tasks_file = artifacts_of("corpus", &folder_of(name))
        .iter()
        .any(|path| path.ends_with("/tasks.md"));
    let tasks = has_tasks_file.then(|| {
        (
            entry["completedTasks"].as_u64().expect("completedTasks"),
            entry["totalTasks"].as_u64().expect("totalTasks"),
        )
    });
    let count = |operation: &str| {
        show["deltas"]
            .as_array()
            .expect("deltas")
            .iter()
            .filter(|delta| delta["operation"] == operation)
            .count() as u64
    };
    let requirements = (show["deltaCount"].as_u64().expect("deltaCount") > 0)
        .then(|| ["ADDED", "MODIFIED", "REMOVED", "RENAMED"].map(count));
    (
        show["title"].as_str().expect("a title").to_string(),
        tasks,
        requirements,
    )
}

/// Case 2: ten changes of OpenSpec's own repository, five archived. The active
/// ones give the title, the task counts, and the delta counts that OpenSpec
/// 1.15.0's binary printed for them, and the why of spec 3; the archived ones
/// give the values that the rules of spec 3 give (`expected.json`).
#[test]
fn the_corpus_subset_gives_the_values_of_openspec() {
    let project = Project::new("", &["openspec"]);
    let base = project.head();
    project.add_fixture("corpus");
    project.commit("docs: add the corpus");

    let intents = project.intents(&base, &[]);

    let mut expected: Vec<Value> = Vec::new();
    for row in rows() {
        let (title, tasks, requirements) = if row.archived {
            (row.title.clone(), row.tasks, row.requirements)
        } else {
            let values = binary_values(&row.reference);
            // The rules of spec 3 and the binary agree on what the binary prints.
            assert_eq!(
                (row.title.clone(), row.tasks, row.requirements),
                values,
                "{}",
                row.reference
            );
            values
        };
        let folder = if row.archived {
            format!("openspec/changes/archive/{}", row.reference)
        } else {
            folder_of(&row.reference)
        };
        expected.push(intent(
            &row.reference,
            &title,
            row.why.as_deref(),
            tasks,
            requirements,
            &artifacts_of("corpus", &folder),
        ));
    }
    expected.sort_by(|a, b| a["ref"].as_str().cmp(&b["ref"].as_str()));
    assert_eq!(intents.len(), 10);
    assert_eq!(intents, expected);
}
