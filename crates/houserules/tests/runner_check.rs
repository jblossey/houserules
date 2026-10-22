//! The `runner` check of `audit` (batch 35, spec 3 and acceptance cases 1 and
//! 2): a check names a tool of `.houserules.json` `runners`, and `audit` runs
//! the tool and reads its SARIF or its exit status.
//!
//! Every test builds a scratch git repository whose base commit holds one
//! standing entry, `team.lint`, with a `runner` check, the `runners` object of
//! its marker, and the committed fake tool `tests/fixtures/runner/fake-sarif.sh`,
//! which a command runs through `sh` on every system. A test writes the SARIF
//! that the fake tool prints to the untracked `out/result.sarif`; the tool
//! appends the number of file arguments of each call to `out/calls.txt`, so a
//! test counts the calls and sees whether a process started at all.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use serde_json::{Value, json};

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` off (`tests/update.rs` has the account).
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// The id of the entry that holds the runner check.
const ENTRY_ID: &str = "team.lint";

/// The fake tool, as the repository stores it.
fn fake_tool() -> String {
    fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runner/fake-sarif.sh"),
    )
    .expect("read the fake tool")
}

/// The command of the fake tool: it prints `out/result.sarif`, exits with
/// `status`, and records its calls in `out/calls.txt`.
fn fake_command(status: i32) -> Value {
    json!([
        "sh",
        "tools/fake-sarif.sh",
        "out/result.sarif",
        status.to_string(),
        "out/calls.txt"
    ])
}

/// The `runners` object with one `sarif` tool, `fake`, that runs the fake
/// tool and exits with `status`.
fn fake_runners(status: i32) -> Value {
    json!({"fake": {
        "command": fake_command(status),
        "parser": "sarif",
        "install": "install the fake tool",
    }})
}

/// The `runners` object with one `exit-code` tool, `fake`, that runs `script`
/// through `sh -c`.
fn script_runners(script: &str) -> Value {
    json!({"fake": {
        "command": ["sh", "-c", script],
        "parser": "exit-code",
        "install": "install the fake tool",
    }})
}

/// A `runner` check of level `fail` that names `fake`, reads the files
/// matching `files`, and reads them with `scope`.
fn check(files: &str, scope: &str) -> Value {
    json!({"type": "runner", "level": "fail", "tool": "fake", "files": files, "scope": scope})
}

/// A scratch repository with the entry, the marker, and the fake tool
/// committed on `main` as the base commit.
struct Repo {
    dir: tempfile::TempDir,
    /// The sha of the base commit.
    base: String,
}

impl Repo {
    /// The repository whose entry holds `check` and whose marker holds
    /// `runners`.
    fn new(check: Value, runners: Value) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut repo = Self {
            dir,
            base: String::new(),
        };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.git(&["config", "core.abbrev", "7"]);
        repo.write(".git/info/exclude", "out/\n");
        repo.write(".gitattributes", "* -text\n");
        repo.write(
            "knowledge/schema.json",
            &fs::read_to_string(
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json"),
            )
            .expect("read the vendored schema"),
        );
        repo.write("knowledge/areas.json", r#"{"global":{"paths":[]}}"#);
        let topic = json!({
            "$schema": "./schema.json", "topic": "team", "title": "team title",
            "entries": [{
                "id": ENTRY_ID, "kind": "rule", "area": "global", "standing": true,
                "summary": "Lint the source.", "body": ["Body."], "tags": ["test"],
                "source": {"date": "2026-08-29", "by": "user"}, "check": check,
            }],
        });
        repo.write(
            "knowledge/team.json",
            &serde_json::to_string(&topic).expect("serialize the topic"),
        );
        let marker = json!({
            "schema": "houserules/install@2.2.0", "defaultBranch": "main", "runners": runners,
        });
        repo.write(
            ".houserules.json",
            &serde_json::to_string_pretty(&marker).expect("serialize the marker"),
        );
        repo.write("tools/fake-sarif.sh", &fake_tool());
        repo.write("src/a.rs", "fn a() {}\n");
        repo.write("docs/readme.md", "docs\n");
        repo.base = repo.commit("chore: base");
        repo
    }

    /// The repository with the `fake` tool of [`fake_runners`] that exits with
    /// 0, and a check that reads `src/**` with `scope`.
    fn with_sarif_tool(scope: &str) -> Self {
        Self::new(check("src/**", scope), fake_runners(0))
    }

    /// The repository root.
    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Runs `git` in the repository with a fixed identity and returns its
    /// stdout. Panics with stderr on a failure.
    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .env("GIT_AUTHOR_NAME", "Runner Author")
            .env("GIT_AUTHOR_EMAIL", "author@runner.test")
            .env("GIT_COMMITTER_NAME", "Runner Committer")
            .env("GIT_COMMITTER_EMAIL", "committer@runner.test")
            .current_dir(self.root())
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("utf8 stdout")
    }

    /// Writes `content` to `relative`, creating parent folders.
    fn write(&self, relative: &str, content: &str) {
        let path = self.root().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent folders");
        }
        fs::write(path, content).expect("write file");
    }

    /// Stages everything and commits it with `message`. Returns the sha.
    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            message,
        ]);
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Writes `files`, then commits them. Returns the sha.
    fn commit_files(&self, files: &[(&str, &str)]) -> String {
        for (relative, content) in files {
            self.write(relative, content);
        }
        self.commit("feat: change files")
    }

    /// Commits `count` new files `src/f000.rs`, `src/f001.rs`, and so on.
    /// Returns the sha.
    fn commit_many(&self, count: usize) -> String {
        for index in 0..count {
            self.write(
                &format!("src/f{index:03}.rs"),
                &format!("fn f{index}() {{}}\n"),
            );
        }
        self.commit("feat: add files")
    }

    /// The abbreviated sha of `revision`, as the binary prints it.
    fn short(&self, revision: &str) -> String {
        self.git(&["rev-parse", "--short", revision])
            .trim()
            .to_string()
    }

    /// The SARIF document that the fake tool prints.
    fn print_sarif(&self, document: &Value) {
        self.write(
            "out/result.sarif",
            &serde_json::to_string(document).expect("serialize the SARIF"),
        );
    }

    /// The text that the fake tool prints, as it is.
    fn print_text(&self, text: &str) {
        self.write("out/result.sarif", text);
    }

    /// The number of file arguments of each call of the fake tool, or `None`
    /// when the tool never ran.
    fn calls(&self) -> Option<Vec<usize>> {
        let text = fs::read_to_string(self.root().join("out/calls.txt")).ok()?;
        Some(
            text.lines()
                .map(|line| line.parse().expect("a count"))
                .collect(),
        )
    }

    /// Runs `audit --base <base> --head <head>` in the repository.
    fn audit(&self, base: &str, head: &str) -> Output {
        houserules()
            .args(["audit", "--base", base, "--head", head])
            .current_dir(self.root())
            .output()
            .expect("run audit")
    }

    /// Runs `audit` over the base commit up to `HEAD`.
    fn audit_head(&self) -> Output {
        self.audit(&self.base, "HEAD")
    }
}

/// A SARIF document with one run that holds `results`.
fn sarif(results: Value) -> Value {
    json!({"version": "2.1.0", "runs": [{"tool": {"driver": {"name": "fake"}}, "results": results}]})
}

/// A SARIF result of `level` for `rule` at `uri`:`line`.
fn result(rule: &str, level: &str, message: &str, uri: &str, line: u64) -> Value {
    json!({
        "ruleId": rule, "level": level, "message": {"text": message},
        "locations": [{"physicalLocation": {
            "artifactLocation": {"uri": uri}, "region": {"startLine": line},
        }}],
    })
}

/// The stdout of `output` as JSON.
fn stdout_json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout is no JSON ({error}): {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// The row of the entry `team.lint` in an audit that printed rows.
fn row(output: &Output) -> Value {
    stdout_json(output)["rules"]
        .as_array()
        .expect("rules array")
        .iter()
        .find(|row| row["id"] == ENTRY_ID)
        .unwrap_or_else(|| panic!("no row for {ENTRY_ID}"))
        .clone()
}

/// Asserts that `output` is a setup error: exit 2, no rows on stdout, and
/// `line` as the whole of stderr.
fn assert_setup_error(output: &Output, line: &str) {
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("{line}\n"),
        "stderr"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "", "stdout");
    assert_eq!(output.status.code(), Some(2), "exit");
}

/// Asserts that `output` printed a row with `result` and `evidence`, and
/// exited with `code`.
fn assert_row(output: &Output, result: &str, evidence: &str, code: i32) {
    let row = row(output);
    assert_eq!(row["result"], result, "{row}");
    assert_eq!(row["evidence"], evidence, "{row}");
    assert_eq!(
        output.status.code(),
        Some(code),
        "exit; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ---- spec acceptance case 1: the rows of the two parsers -------------------------------

/// A `sarif` run with three findings fails with the first finding as
/// `<path>:<line> <ruleId>: <message>`, `and <n> more` for the rest, and a
/// `findings` array in call order. The row is pinned key by key.
#[test]
fn runner_fails_on_findings_with_path_line_rule_message() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([
        result("no-todo", "error", "remove the todo", "src/a.rs", 3),
        result("no-todo", "warning", "remove this too", "src/b.rs", 7),
        result("style", "warning", "use snake case", "src/b.rs", 9),
    ])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n"), ("src/b.rs", "fn b() {}\n")]);

    let output = repo.audit_head();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        serde_json::to_string(&row(&output)).expect("serialize the row"),
        concat!(
            r#"{"id":"team.lint","kind":"rule","mode":"deterministic","level":"fail","#,
            r#""result":"fail","evidence":"src/a.rs:3 no-todo: remove the todo and 2 more","#,
            r#""findings":["#,
            r#"{"path":"src/a.rs","line":3,"rule":"no-todo","message":"remove the todo"},"#,
            r#"{"path":"src/b.rs","line":7,"rule":"no-todo","message":"remove this too"},"#,
            r#"{"path":"src/b.rs","line":9,"rule":"style","message":"use snake case"}]}"#,
        )
    );
}

/// A level `warn` check turns the same findings into a `warn` row, which fails
/// no audit.
#[test]
fn warn_level_runner_check_gives_a_warn_row_with_findings() {
    let repo = Repo::new(
        json!({"type": "runner", "level": "warn", "tool": "fake", "files": "src/**"}),
        fake_runners(0),
    );
    repo.print_sarif(&sarif(json!([result("r", "error", "m", "src/a.rs", 1)])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "warn", "src/a.rs:1 r: m", 0);
    assert_eq!(row(&output)["level"], "warn");
    assert_eq!(row(&output)["findings"].as_array().map(Vec::len), Some(1));
}

/// A clean run passes with the call count as evidence and an empty `findings`
/// array.
#[test]
fn runner_passes_on_a_clean_run() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
    assert_eq!(row(&output)["findings"], json!([]));
    assert_eq!(repo.calls(), Some(vec![1]));
}

/// An untriggered check passes with `not triggered` and starts no process.
#[test]
fn untriggered_runner_starts_no_process() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([result("r", "error", "m", "src/a.rs", 1)])));
    repo.commit_files(&[("docs/readme.md", "docs changed\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "not triggered", 0);
    assert!(row(&output).get("findings").is_none());
    assert_eq!(repo.calls(), None, "the tool started");
}

/// An untriggered check reads no `runners` entry: a tool that the marker does
/// not define is no error until a changed file triggers the check.
#[test]
fn untriggered_runner_with_an_undefined_tool_passes() {
    let repo = Repo::new(check("src/**", "changed"), json!({}));
    repo.commit_files(&[("docs/readme.md", "docs changed\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "not triggered", 0);
}

/// A check with no `files` reads every changed file.
#[test]
fn files_default_to_every_changed_file() {
    let repo = Repo::new(
        json!({"type": "runner", "level": "fail", "tool": "fake"}),
        fake_runners(0),
    );
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("docs/readme.md", "docs changed\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
    assert_eq!(repo.calls(), Some(vec![1]));
}

/// `scope: "changed"` appends the triggering files in batches of at most 200:
/// 250 files make two calls of 200 and 50, and the evidence counts them.
#[test]
fn two_hundred_fifty_files_make_two_calls() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_many(250);

    let output = repo.audit_head();

    assert_eq!(repo.calls(), Some(vec![200, 50]));
    assert_row(&output, "pass", "no findings in 2 calls", 0);
}

/// `scope: "tree"` appends no file and makes one call, however many files
/// the range changed.
#[test]
fn tree_scope_appends_no_files_and_makes_one_call() {
    let repo = Repo::with_sarif_tool("tree");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_many(250);

    let output = repo.audit_head();

    assert_eq!(repo.calls(), Some(vec![0]));
    assert_row(&output, "pass", "no findings in 1 call", 0);
}

/// A result with no `level` and no rule counts as a warning, and its rule is
/// the designed token `(no rule id)`.
#[test]
fn result_with_no_level_and_no_rule_counts_as_warning() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([{
        "message": {"text": "no level"},
        "locations": [{"physicalLocation": {
            "artifactLocation": {"uri": "src/a.rs"}, "region": {"startLine": 1},
        }}],
    }])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:1 (no rule id): no level", 1);
    assert_eq!(
        row(&output)["findings"],
        json!([{"path": "src/a.rs", "line": 1, "rule": "(no rule id)", "message": "no level"}])
    );
}

/// A SARIF document in Semgrep's shape: the results carry no `level`, and the
/// level of each rule is `defaultConfiguration.level` in `tool.driver.rules`.
fn semgrep_shape(default_level: &str) -> Value {
    json!({"version": "2.1.0", "runs": [{
        "tool": {"driver": {"name": "semgrep", "rules": [
            {"id": "other.rule", "defaultConfiguration": {"level": "warning"}},
            {"id": "team.rule", "defaultConfiguration": {"level": default_level}},
        ]}},
        "results": [{
            "ruleId": "team.rule", "ruleIndex": 1, "message": {"text": "found"},
            "locations": [{"physicalLocation": {
                "artifactLocation": {"uri": "src/a.rs"}, "region": {"startLine": 2},
            }}],
        }],
    }]})
}

/// A result under a rule whose default level is `note` does not count.
#[test]
fn semgrep_shape_note_default_does_not_count() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&semgrep_shape("note"));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
    assert_eq!(row(&output)["findings"], json!([]));
}

/// A result under a rule whose default level is `error` counts.
#[test]
fn semgrep_shape_error_default_counts() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&semgrep_shape("error"));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:2 team.rule: found", 1);
}

/// A document whose run overrides `team.rule` to `descriptor` at `level`, with
/// a result that names invocation 0 (or none, when `invocation` is false).
fn override_shape(descriptor: Value, level: &str, invocation: bool) -> Value {
    let mut found = json!({
        "ruleId": "team.rule", "ruleIndex": 0, "message": {"text": "found"},
        "locations": [{"physicalLocation": {
            "artifactLocation": {"uri": "src/a.rs"}, "region": {"startLine": 2},
        }}],
    });
    if invocation {
        found["provenance"] = json!({"invocationIndex": 0});
    }
    json!({"version": "2.1.0", "runs": [{
        "tool": {"driver": {"name": "semgrep", "rules": [
            {"id": "team.rule", "defaultConfiguration": {"level": "error"}},
        ]}},
        "invocations": [{
            "executionSuccessful": true,
            "ruleConfigurationOverrides": [{
                "descriptor": descriptor, "configuration": {"level": level},
            }],
        }],
        "results": [found],
    }]})
}

/// An override that sets a rule to `none`, named by `id`, drops its results.
#[test]
fn override_to_none_drops_results() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&override_shape(json!({"id": "team.rule"}), "none", true));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
}

/// An override that names the rule by `index` drops its results too.
#[test]
fn override_to_none_by_descriptor_index_drops_results() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&override_shape(json!({"index": 0}), "none", true));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
}

/// A result that names no invocation reads no override: the rule default
/// (`error`) counts.
#[test]
fn override_applies_only_to_a_result_that_names_its_invocation() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&override_shape(json!({"id": "team.rule"}), "none", false));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:2 team.rule: found", 1);
}

/// A result whose `kind` is not `fail` does not count, whatever its `level`.
#[test]
fn kind_pass_does_not_count() {
    let repo = Repo::with_sarif_tool("changed");
    let mut passing = result("r", "error", "m", "src/a.rs", 1);
    passing["kind"] = json!("pass");
    repo.print_sarif(&sarif(json!([passing])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
}

/// A result of `level` `note` is no finding of the row.
#[test]
fn note_result_is_not_in_findings() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([
        result("loud", "error", "counts", "src/a.rs", 1),
        result("quiet", "note", "does not count", "src/a.rs", 2),
    ])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:1 loud: counts", 1);
    assert_eq!(
        row(&output)["findings"],
        json!([{"path": "src/a.rs", "line": 1, "rule": "loud", "message": "counts"}])
    );
}

/// A result with no location is a finding with a `null` path and line; the
/// evidence has no location.
#[test]
fn result_with_no_location_has_null_path() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([
        {"ruleId": "r", "level": "error", "message": {"text": "m"}}
    ])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "r: m", 1);
    assert_eq!(
        row(&output)["findings"],
        json!([{"path": null, "line": null, "rule": "r", "message": "m"}])
    );
}

/// A result without `ruleId` takes the id of the rule that its `ruleIndex`
/// names, and the designed token `(no rule id)` when it has neither.
#[test]
fn result_without_rule_id_takes_the_indexed_rule_or_the_token() {
    let repo = Repo::with_sarif_tool("changed");
    let mut indexed = result("", "error", "indexed", "src/a.rs", 1);
    indexed.as_object_mut().expect("object").remove("ruleId");
    indexed["ruleIndex"] = json!(1);
    let mut bare = result("", "error", "bare", "src/a.rs", 2);
    bare.as_object_mut().expect("object").remove("ruleId");
    repo.print_sarif(&json!({"version": "2.1.0", "runs": [{
        "tool": {"driver": {"name": "fake", "rules": [{"id": "r.zero"}, {"id": "r.one"}]}},
        "results": [indexed, bare],
    }]}));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_eq!(
        row(&output)["findings"],
        json!([
            {"path": "src/a.rs", "line": 1, "rule": "r.one", "message": "indexed"},
            {"path": "src/a.rs", "line": 2, "rule": "(no rule id)", "message": "bare"},
        ])
    );
}

/// A path outside the install root is kept as the tool wrote it.
#[test]
fn sarif_path_outside_the_root_is_kept_as_given() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([result(
        "r",
        "error",
        "m",
        "file:///elsewhere/outside.rs",
        2
    )])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "file:///elsewhere/outside.rs:2 r: m", 1);
    assert_eq!(
        row(&output)["findings"][0]["path"],
        "file:///elsewhere/outside.rs"
    );
}

/// The exit status of a tool is ignored when stdout holds valid SARIF.
#[test]
fn sarif_exit_status_is_ignored_when_stdout_holds_valid_sarif() {
    let repo = Repo::new(check("src/**", "changed"), fake_runners(1));
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
}

/// Two calls (200 files and 1) that print a clean document pass, and the
/// evidence counts both calls.
#[test]
fn sarif_pass_evidence_counts_calls() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_many(201);

    let output = repo.audit_head();

    assert_eq!(repo.calls(), Some(vec![200, 1]));
    assert_row(&output, "pass", "no findings in 2 calls", 0);
}

/// The findings of batched calls merge in call order: the call of 200 files
/// prints one finding and the call of the last file prints another, and the
/// row lists the first call's finding first.
#[test]
fn sarif_findings_of_batched_calls_merge_in_call_order() {
    let repo = Repo::new(
        check("src/**", "changed"),
        json!({"fake": {
            "command": [
                "sh", "-c",
                r#"if [ "$#" -gt 1 ]; then cat out/first.sarif; else cat out/second.sarif; fi"#,
                "sh",
            ],
            "parser": "sarif",
            "install": "install the fake tool",
        }}),
    );
    repo.write(
        "out/first.sarif",
        &sarif(json!([result(
            "first",
            "error",
            "from call 1",
            "src/f000.rs",
            1
        )]))
        .to_string(),
    );
    repo.write(
        "out/second.sarif",
        &sarif(json!([result(
            "second",
            "error",
            "from call 2",
            "src/f200.rs",
            2
        )]))
        .to_string(),
    );
    repo.commit_many(201);

    let output = repo.audit_head();

    assert_row(
        &output,
        "fail",
        "src/f000.rs:1 first: from call 1 and 1 more",
        1,
    );
    assert_eq!(
        row(&output)["findings"],
        json!([
            {"path": "src/f000.rs", "line": 1, "rule": "first", "message": "from call 1"},
            {"path": "src/f200.rs", "line": 2, "rule": "second", "message": "from call 2"},
        ])
    );
}

/// A file name with a space reaches the tool as one argument: no shell splits
/// it.
#[test]
fn a_file_name_with_a_space_is_one_argument() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/two words.rs", "fn w() {}\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
    assert_eq!(repo.calls(), Some(vec![1]));
}

/// A rule that only `tool.extensions` holds is not read: its result takes the
/// level `warning`, whatever default that rule names.
#[test]
fn rule_in_tool_extensions_is_not_read() {
    let repo = Repo::with_sarif_tool("changed");
    let mut found = result("ext.rule", "warning", "found", "src/a.rs", 3);
    found.as_object_mut().expect("object").remove("level");
    repo.print_sarif(&json!({"version": "2.1.0", "runs": [{
        "tool": {
            "driver": {"name": "fake"},
            "extensions": [{"name": "plugin", "rules": [
                {"id": "ext.rule", "defaultConfiguration": {"level": "note"}},
            ]}],
        },
        "results": [found],
    }]}));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:3 ext.rule: found", 1);
}

/// An `exit-code` tool that exits 3 fails with the first line of its stderr
/// as evidence and carries no `findings`.
#[test]
fn exit_code_tool_exiting_3_fails_with_its_stderr_line() {
    let repo = Repo::new(
        check("src/**", "changed"),
        script_runners("echo 'lint failed' >&2; echo 'second line' >&2; exit 3"),
    );
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "lint failed", 1);
    assert!(row(&output).get("findings").is_none());
}

/// An `exit-code` tool with an empty stderr fails with `exited <n>`.
#[test]
fn exit_code_tool_with_empty_stderr_fails_with_exited_n() {
    let repo = Repo::new(check("src/**", "changed"), script_runners("exit 3"));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "exited 3", 1);
    assert!(row(&output).get("findings").is_none());
}

/// The evidence of an `exit-code` failure comes from the first call, in call
/// order, that exits with a non-zero status: the second call's line when the
/// first call passes, the first call's line when both fail.
#[test]
fn exit_code_evidence_comes_from_the_first_failing_call() {
    for (first_call, expected) in [
        ("exit 0", "second call failed"),
        ("echo 'first call failed' >&2; exit 4", "first call failed"),
    ] {
        let script = format!(
            r#"if [ "$#" -gt 1 ]; then {first_call}; else echo 'second call failed' >&2; exit 5; fi"#
        );
        let repo = Repo::new(check("src/**", "changed"), script_runners(&script));
        repo.commit_many(201);

        let output = repo.audit_head();

        assert_row(&output, "fail", expected, 1);
    }
}

/// An `exit-code` tool that exits 0 in every call passes with the call count.
#[test]
fn exit_code_pass_evidence_counts_calls() {
    let repo = Repo::new(check("src/**", "changed"), script_runners("exit 0"));
    repo.commit_many(201);

    let output = repo.audit_head();

    assert_row(&output, "pass", "exited 0 in 2 calls", 0);
    assert!(row(&output).get("findings").is_none());
}

/// A tool that a signal ended has no exit status: the evidence says so.
// A signal ends a process only on Unix; Windows has no such status.
#[cfg(unix)]
#[test]
fn exit_code_tool_ended_by_a_signal_fails_with_a_designed_token() {
    let repo = Repo::new(check("src/**", "changed"), script_runners("kill -9 $$"));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "ended by a signal", 1);
}

/// The tool runs in the install root with the environment of `audit`, however
/// the caller reached the root.
#[test]
fn tool_runs_in_the_install_root_with_the_environment_of_audit() {
    let repo = Repo::new(
        check("src/**", "changed"),
        script_runners("test -f .houserules.json && test \"$RUNNER_PROBE\" = present"),
    );
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    let elsewhere = tempfile::tempdir().expect("tempdir");

    let output = houserules()
        .args(["audit", "--base", &repo.base, "--head", "HEAD", "--dir"])
        .arg(repo.root())
        .env("RUNNER_PROBE", "present")
        .current_dir(elsewhere.path())
        .output()
        .expect("run audit");

    assert_row(&output, "pass", "exited 0 in 1 call", 0);
}

// ---- spec acceptance case 2: setup errors ----------------------------------------------

/// A triggered check that names a tool the marker does not define exits 2
/// with the tool and the marker key.
#[test]
fn undefined_tool_exits_2_with_its_line() {
    let repo = Repo::new(
        json!({"type": "runner", "level": "fail", "tool": "ghost", "files": "src/**"}),
        fake_runners(0),
    );
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "ghost" is not defined in .houserules.json runners"#,
    );
}

/// A triggered check whose `runners` entry is malformed exits 2 with the line
/// that `check-knowledge` prints for it, which names the marker by its
/// relative name.
#[test]
fn malformed_runner_entry_exits_2_with_its_line() {
    let repo = Repo::new(
        check("src/**", "changed"),
        json!({"fake": {"command": [], "parser": "sarif", "install": "install the fake tool"}}),
    );
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        ".houserules.json: runners.fake.command must be a non-empty array of strings",
    );
}

/// A defined tool whose program the system does not find exits 2 with the
/// install line of the project.
#[test]
fn missing_program_exits_2_with_the_install_line() {
    let repo = Repo::new(
        check("src/**", "changed"),
        json!({"fake": {
            "command": ["houserules-no-such-program", "--flag"],
            "parser": "sarif",
            "install": "run: brew install houserules-no-such-program",
        }}),
    );
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" (houserules-no-such-program) is not installed: run: brew install houserules-no-such-program"#,
    );
}

/// A program path inside the install root that the system cannot start names
/// the operating system's own error.
// A missing execute bit is a Unix mode; Windows runs a file by its extension.
#[cfg(unix)]
#[test]
fn program_that_cannot_start_names_the_os_error() {
    use std::os::unix::fs::PermissionsExt;

    let repo = Repo::new(
        check("src/**", "changed"),
        json!({"fake": {
            "command": ["tools/not-executable.sh"], "parser": "exit-code", "install": "x",
        }}),
    );
    repo.write("tools/not-executable.sh", "#!/bin/sh\nexit 0\n");
    fs::set_permissions(
        repo.root().join("tools/not-executable.sh"),
        fs::Permissions::from_mode(0o644),
    )
    .expect("clear the execute bit");
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" (tools/not-executable.sh) could not start: Permission denied (os error 13)"#,
    );
}

/// A check with no `tool` is no tool at all: a triggered audit names the
/// entry and the missing field.
#[test]
fn runner_check_without_a_tool_exits_2_with_its_line() {
    let repo = Repo::new(
        json!({"type": "runner", "level": "fail", "files": "src/**"}),
        fake_runners(0),
    );
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(&output, r#"team.lint: check "runner" needs "tool""#);
}

/// `--head` that is not the checked-out commit exits 2 with both short shas
/// and starts no process.
#[test]
fn head_not_checked_out_exits_2_with_the_tree_line() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    let first = repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    let second = repo.commit_files(&[("src/a.rs", "fn a() { 2 }\n")]);
    repo.git(&["checkout", "-q", "--detach", &first]);

    let output = repo.audit(&repo.base, &second);

    assert_setup_error(
        &output,
        &format!(
            r#"runner tool "fake" needs the tree at --head checked out: HEAD is {}, --head is {}"#,
            repo.short(&first),
            repo.short(&second)
        ),
    );
    assert_eq!(repo.calls(), None, "the tool started");
}

/// A triggering file that differs from its content at `--head` exits 2 with
/// its path and starts no process.
#[test]
fn changed_triggering_file_exits_2_with_the_tree_line() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    repo.write("src/a.rs", "fn a() { 2 }\n");

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" needs the tree at --head checked out: src/a.rs differs from --head"#,
    );
    assert_eq!(repo.calls(), None, "the tool started");
}

/// A deleted triggering file differs from its content at `--head` too.
#[test]
fn deleted_triggering_file_exits_2_with_the_tree_line() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    fs::remove_file(repo.root().join("src/a.rs")).expect("remove the file");

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" needs the tree at --head checked out: src/a.rs differs from --head"#,
    );
}

/// A `changed` check reads the triggering files only: a modified tracked file
/// that it does not read, and an untracked file, are no reason to refuse.
#[test]
fn changed_scope_ignores_files_the_check_does_not_read() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    repo.write("docs/readme.md", "docs, edited in the working tree\n");
    repo.write("src/untracked.rs", "fn untracked() {}\n");

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
}

/// A `tree` check reads every tracked file: a modified file that no changed
/// file names refuses the run.
#[test]
fn tree_scope_refuses_any_modified_tracked_file() {
    let repo = Repo::with_sarif_tool("tree");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    repo.write("docs/readme.md", "docs, edited in the working tree\n");

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" needs the tree at --head checked out: docs/readme.md differs from --head"#,
    );
    assert_eq!(repo.calls(), None, "the tool started");
}

/// An untracked file is an accepted gap of the tree test: a `tree` check
/// still runs.
#[test]
fn tree_scope_does_not_test_untracked_files() {
    let repo = Repo::with_sarif_tool("tree");
    repo.print_sarif(&sarif(json!([])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    repo.write("src/untracked.rs", "fn untracked() {}\n");

    let output = repo.audit_head();

    assert_row(&output, "pass", "no findings in 1 call", 0);
}

/// A SARIF tool that prints nothing is a setup error.
#[test]
fn sarif_tool_printing_nothing_is_a_setup_error() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_text("");
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(&output, r#"runner tool "fake" printed no SARIF on stdout"#);
}

/// A SARIF tool that prints text that is no JSON is a setup error, with the
/// first line of the parse error.
#[test]
fn sarif_tool_printing_garbage_is_a_setup_error() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_text("not json\n");
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" printed invalid SARIF: expected ident at line 1 column 2"#,
    );
}

/// A JSON document that holds no `runs` is invalid SARIF too.
#[test]
fn sarif_document_without_runs_is_a_setup_error() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_text("{}");
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" printed invalid SARIF: missing field `runs` at line 1 column 2"#,
    );
}

/// An invocation that says `executionSuccessful: false` is a setup error.
#[test]
fn sarif_execution_unsuccessful_is_a_setup_error() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&json!({"version": "2.1.0", "runs": [{
        "tool": {"driver": {"name": "fake"}},
        "invocations": [{"executionSuccessful": false}],
        "results": [],
    }]}));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_setup_error(
        &output,
        r#"runner tool "fake" reported executionSuccessful: false"#,
    );
}

/// A later call that is a setup error ends the audit, whatever the earlier
/// calls found.
#[test]
fn a_setup_error_in_a_later_call_ends_the_audit() {
    let repo = Repo::new(
        check("src/**", "changed"),
        json!({"fake": {
            "command": [
                "sh", "-c",
                "if [ \"$#\" -gt 1 ]; then echo '{\"version\":\"2.1.0\",\"runs\":[]}'; fi",
                "sh",
            ],
            "parser": "sarif",
            "install": "install the fake tool",
        }}),
    );
    repo.commit_many(201);

    let output = repo.audit_head();

    assert_setup_error(&output, r#"runner tool "fake" printed no SARIF on stdout"#);
}

// ---- spec acceptance case 2: the CI shape ----------------------------------------------

/// The pull request of the seeded workflow: `main` moved on after the PR
/// branch left it. A checkout of the PR head (`ref:
/// ${{ github.event.pull_request.head.sha }}`) is the detached PR head, and
/// `audit --base <base> --head <PR head>` runs a `tree` runner check and gives
/// its rows.
#[test]
fn ci_shape_pr_head_checkout_runs_a_tree_runner() {
    let repo = Repo::with_sarif_tool("tree");
    repo.print_sarif(&sarif(json!([])));
    repo.git(&["checkout", "-q", "-b", "pr"]);
    let pr_head = repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    repo.git(&["checkout", "-q", "main"]);
    let base_tip = repo.commit_files(&[("docs/readme.md", "docs moved on\n")]);
    repo.git(&["checkout", "-q", "--detach", &pr_head]);

    let output = repo.audit(&base_tip, &pr_head);

    assert_row(&output, "pass", "no findings in 1 call", 0);
    assert_eq!(stdout_json(&output)["changed_files"], json!(["src/a.rs"]));
    assert_eq!(repo.calls(), Some(vec![0]));
}

/// The default checkout of a pull request is the merge commit of the base and
/// the PR head; the same audit refuses it, which is why the seeded workflow
/// checks out the PR head.
#[test]
fn ci_shape_merge_commit_checkout_is_refused() {
    let repo = Repo::with_sarif_tool("tree");
    repo.print_sarif(&sarif(json!([])));
    repo.git(&["checkout", "-q", "-b", "pr"]);
    let pr_head = repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    repo.git(&["checkout", "-q", "main"]);
    let base_tip = repo.commit_files(&[("docs/readme.md", "docs moved on\n")]);
    repo.git(&["checkout", "-q", "--detach", &base_tip]);
    repo.git(&["merge", "-q", "--no-ff", "-m", "merge the PR", &pr_head]);

    let output = repo.audit(&base_tip, &pr_head);

    assert_setup_error(
        &output,
        &format!(
            r#"runner tool "fake" needs the tree at --head checked out: HEAD is {}, --head is {}"#,
            repo.short("HEAD"),
            repo.short(&pr_head)
        ),
    );
}

// ---- review focus 1 to 3 ---------------------------------------------------------------

/// A SARIF document with no `results` key, and one with an empty `runs`
/// array, are valid and clean.
#[test]
fn sarif_without_results_passes() {
    for document in [
        json!({"version": "2.1.0", "runs": [{"tool": {"driver": {"name": "fake"}}}]}),
        json!({"version": "2.1.0", "runs": []}),
    ] {
        let repo = Repo::with_sarif_tool("changed");
        repo.print_sarif(&document);
        repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

        let output = repo.audit_head();

        assert_row(&output, "pass", "no findings in 1 call", 0);
    }
}

/// A percent-encoded `uri` is decoded, and a `file://` `uri` under the install
/// root is made relative to it, with `/` separators.
#[test]
fn sarif_uri_is_decoded_and_made_root_relative() {
    let repo = Repo::with_sarif_tool("changed");
    let top = repo.git(&["rev-parse", "--show-toplevel"]);
    let under_root = format!(
        "file:///{}/src/nested/b.rs",
        top.trim().trim_start_matches('/').replace(' ', "%20")
    );
    repo.print_sarif(&sarif(json!([
        result("r", "error", "encoded", "src/a%20b.rs", 1),
        result("r", "error", "absolute", &under_root, 2),
    ])));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_eq!(
        row(&output)["findings"],
        json!([
            {"path": "src/a b.rs", "line": 1, "rule": "r", "message": "encoded"},
            {"path": "src/nested/b.rs", "line": 2, "rule": "r", "message": "absolute"},
        ])
    );
}

/// A `uriBaseId` is resolved through the `originalUriBaseIds` of the run.
#[test]
fn sarif_uri_base_id_is_resolved_through_original_uri_base_ids() {
    let repo = Repo::with_sarif_tool("changed");
    let top = repo.git(&["rev-parse", "--show-toplevel"]);
    let base = format!(
        "file:///{}/",
        top.trim().trim_start_matches('/').replace(' ', "%20")
    );
    repo.print_sarif(&json!({"version": "2.1.0", "runs": [{
        "tool": {"driver": {"name": "fake"}},
        "originalUriBaseIds": {"SRCROOT": {"uri": base}},
        "results": [{
            "ruleId": "r", "level": "error", "message": {"text": "m"},
            "locations": [{"physicalLocation": {
                "artifactLocation": {"uri": "src/a.rs", "uriBaseId": "SRCROOT"},
                "region": {"startLine": 4},
            }}],
        }],
    }]}));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:4 r: m", 1);
}

/// The `file:` URI of the repository root, with no trailing slash.
fn top_url(repo: &Repo) -> String {
    let top = repo.git(&["rev-parse", "--show-toplevel"]);
    format!(
        "file:///{}",
        top.trim().trim_start_matches('/').replace(' ', "%20")
    )
}

/// A SARIF document with one run whose `originalUriBaseIds` is `bases` and
/// whose results are `results`.
fn sarif_with_bases(bases: Value, results: Value) -> Value {
    json!({"version": "2.1.0", "runs": [{
        "tool": {"driver": {"name": "fake"}},
        "originalUriBaseIds": bases,
        "results": results,
    }]})
}

/// An `error` result for the rule `r` with the message `m` at line `line` of
/// `uri`, which `base_id` of `originalUriBaseIds` (when given) is the base of.
fn based_result(uri: &str, base_id: Option<&str>, line: u64) -> Value {
    let mut artifact = json!({"uri": uri});
    if let Some(id) = base_id {
        artifact["uriBaseId"] = json!(id);
    }
    json!({
        "ruleId": "r", "level": "error", "message": {"text": "m"},
        "locations": [{"physicalLocation": {
            "artifactLocation": artifact, "region": {"startLine": line},
        }}],
    })
}

/// A location whose base resolves outside the install root keeps its fully
/// resolved URI: the bare `uri` would name the file `src/x.rs` of the
/// install root.
#[test]
fn sarif_based_location_outside_the_root_keeps_its_resolved_form() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif_with_bases(
        json!({"SRCROOT": {"uri": "file:///elsewhere/"}}),
        json!([based_result("src/x.rs", Some("SRCROOT"), 4)]),
    ));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "file:///elsewhere/src/x.rs:4 r: m", 1);
    assert_eq!(
        row(&output)["findings"],
        json!([{"path": "file:///elsewhere/src/x.rs", "line": 4, "rule": "r", "message": "m"}])
    );
}

/// Dot segments go before the path is made relative to the root: `src/../src/a.rs`
/// and `../src/a.rs` under a base `<root>/src/` both name `src/a.rs`.
#[test]
fn sarif_dot_segments_are_removed_from_a_path_under_the_root() {
    let repo = Repo::with_sarif_tool("changed");
    let base = format!("{}/src/", top_url(&repo));
    repo.print_sarif(&sarif_with_bases(
        json!({"SRCROOT": {"uri": base}}),
        json!([
            based_result("src/../src/a.rs", None, 1),
            based_result("../src/a.rs", Some("SRCROOT"), 2),
        ]),
    ));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:1 r: m and 1 more", 1);
    assert_eq!(
        row(&output)["findings"],
        json!([
            {"path": "src/a.rs", "line": 1, "rule": "r", "message": "m"},
            {"path": "src/a.rs", "line": 2, "rule": "r", "message": "m"},
        ])
    );
}

/// A `uriBaseId` that the run does not define leaves the `uri` relative to the
/// install root.
#[test]
fn sarif_undefined_uri_base_id_leaves_the_uri_root_relative() {
    let repo = Repo::with_sarif_tool("changed");
    repo.print_sarif(&sarif_with_bases(
        json!({}),
        json!([based_result("src/a.rs", Some("UNDEFINED"), 3)]),
    ));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:3 r: m", 1);
    assert_eq!(row(&output)["findings"][0]["path"], "src/a.rs");
}

/// An empty or blank `ruleId` is absent and an empty or blank `message.text`
/// is `(no message)`, so the evidence holds no empty slot.
#[test]
fn blank_rule_id_and_message_read_as_absent() {
    for blank in ["", "  "] {
        let repo = Repo::with_sarif_tool("changed");
        repo.print_sarif(&sarif(json!([result(
            blank, "error", blank, "src/a.rs", 1
        )])));
        repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

        let output = repo.audit_head();

        assert_row(&output, "fail", "src/a.rs:1 (no rule id): (no message)", 1);
        assert_eq!(
            row(&output)["findings"],
            json!([{"path": "src/a.rs", "line": 1, "rule": "(no rule id)", "message": "(no message)"}]),
            "blank = {blank:?}"
        );
    }
}

/// A blank `ruleId` is absent, so the result takes the id of the rule that its
/// `ruleIndex` names.
#[test]
fn blank_rule_id_takes_the_indexed_rule() {
    let repo = Repo::with_sarif_tool("changed");
    let mut indexed = result("", "error", "indexed", "src/a.rs", 1);
    indexed["ruleIndex"] = json!(1);
    repo.print_sarif(&json!({"version": "2.1.0", "runs": [{
        "tool": {"driver": {"name": "fake", "rules": [{"id": "r.zero"}, {"id": "r.one"}]}},
        "results": [indexed],
    }]}));
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "fail", "src/a.rs:1 r.one: indexed", 1);
}

/// A program path with a space in a folder name resolves against the install
/// root, however the caller reached the root, and runs with no shell.
// The program is a script that the kernel starts through its `#!` line, and
// the execute bit is a Unix mode; the test below runs the same input through
// `sh` for every system.
#[cfg(unix)]
#[test]
fn runner_program_with_a_space_resolves_against_the_install_root() {
    use std::os::unix::fs::PermissionsExt;

    let repo = Repo::new(
        check("src/**", "changed"),
        json!({"fake": {
            "command": ["tools/my lint/run.sh"], "parser": "exit-code", "install": "x",
        }}),
    );
    repo.write("tools/my lint/run.sh", "#!/bin/sh\nexit 0\n");
    fs::set_permissions(
        repo.root().join("tools/my lint/run.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("set the execute bit");
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);
    let elsewhere = tempfile::tempdir().expect("tempdir");

    let output = houserules()
        .args(["audit", "--base", &repo.base, "--head", "HEAD", "--dir"])
        .arg(repo.root())
        .current_dir(elsewhere.path())
        .output()
        .expect("run audit");

    assert_row(&output, "pass", "exited 0 in 1 call", 0);
}

/// A script with a space in a folder name runs through `sh` on every system:
/// the path is one argv entry, resolved by the child from the install root.
#[test]
fn runner_script_with_a_space_runs_through_sh() {
    let repo = Repo::new(
        check("src/**", "changed"),
        json!({"fake": {
            "command": ["sh", "tools/my lint/run.sh"], "parser": "exit-code", "install": "x",
        }}),
    );
    repo.write("tools/my lint/run.sh", "exit 0\n");
    repo.commit_files(&[("src/a.rs", "fn a() { 1 }\n")]);

    let output = repo.audit_head();

    assert_row(&output, "pass", "exited 0 in 1 call", 0);
}
