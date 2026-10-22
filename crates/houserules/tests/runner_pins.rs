//! Output pins of the built-in check types that no other golden covers.
//!
//! `tests/goldens/audit` pins the rows of `grep-absent`, `commits`,
//! `co-change`, and `report-field`. This file pins the rows of
//! `diff-append-only` and `durable-sha`, the output of `check-commit` for
//! a fixed range, and the output of the `commit-msg` hook for the messages
//! of that range. Each pin compares bytes with a committed file under
//! `tests/fixtures/runner-pins/` and stands on its own: the hook runs
//! `check-commit`, so a comparison of the two with each other could not
//! fail.
//!
//! The scratch repository has fixed content, a fixed author and committer,
//! and fixed dates, so every commit sha is the same on every run and the
//! evidence that names a sha is stable. This file alone fixes every byte
//! that a commit holds: the schema, which the binary reads from the working
//! tree, stays out of every commit. The `.houserules.json` of the scratch
//! repository names the local branch `trunk` as the default branch, which
//! `durable-sha` reads first.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::json;

/// A `Command` for the compiled `houserules` binary under test, with the
/// self-update phase of `update` off (`tests/update.rs` has the account).
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// Runs `git` in `root` and returns its stdout. Panics with stderr on a
/// failure.
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

/// Writes `content` to `relative` below `root`, creating parent folders.
fn write_file(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent folders");
    }
    fs::write(path, content).expect("write file");
}

/// Stages everything under `root` and commits it with `message` as commit
/// number `step`. Author, committer, and both dates are fixed by `step`, so
/// the commit sha depends on the tree, the parent, the message, and `step`
/// alone. Returns the full sha.
fn commit_all(root: &Path, step: usize, message: &str) -> String {
    git(root, &["add", "-A"]);
    let date = format!("2026-01-01T00:{step:02}:00+0000");
    let output = Command::new("git")
        .args([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "-m",
            message,
        ])
        .env("GIT_AUTHOR_NAME", "Pin Author")
        .env("GIT_AUTHOR_EMAIL", "author@pins.test")
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_NAME", "Pin Committer")
        .env("GIT_COMMITTER_EMAIL", "committer@pins.test")
        .env("GIT_COMMITTER_DATE", &date)
        .current_dir(root)
        .output()
        .expect("run git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

/// The message of the base commit on `trunk`.
const BASE_MESSAGE: &str = "chore: base\n\nWork-Item: HR-1";

/// The messages of the four commits on `topic`, oldest first. The second has
/// a body line over 40 characters; the last has a bad subject and no
/// `Work-Item` trailer.
const TOPIC_MESSAGES: [&str; 4] = [
    "docs: append to the log\n\nWork-Item: HR-1",
    "feat: touch the source\n\nThis body line is longer than forty characters.\n\nWork-Item: HR-1",
    "docs: cite the touched source\n\nWork-Item: HR-1",
    "Rewrite the log",
];

/// The scratch repository: `trunk` holds the base commit, `topic` branches
/// from it with four commits.
struct PinRepo {
    dir: tempfile::TempDir,
    /// The sha of the base commit, the tip of `trunk`.
    base: String,
    /// The shas of the four commits of `topic`, oldest first.
    topic: [String; 4],
}

impl PinRepo {
    /// The repository root.
    fn root(&self) -> &Path {
        self.dir.path()
    }
}

/// The knowledge entries of the scratch repository, one standing entry for
/// each check the pins need.
fn pin_entries() -> serde_json::Value {
    let entry = |id: &str, check: serde_json::Value| {
        json!({
            "id": id, "kind": "rule", "area": "docs", "standing": true,
            "summary": "A pinned check.", "body": ["Body."], "tags": ["test"],
            "source": {"date": "2026-08-29", "by": "user"}, "check": check,
        })
    };
    json!({
        "$schema": "./schema.json", "topic": "pins", "title": "pins title",
        "entries": [
            entry("pins.commit-format", json!({
                "type": "commits", "level": "fail",
                "subject": "^(feat|fix|chore|docs|test): .+", "body_line_max": 40,
            })),
            entry("pins.trailer", json!({
                "type": "commits", "level": "fail", "require_trailer": "Work-Item",
            })),
            entry("pins.cochange", json!({
                "type": "co-change", "level": "fail", "if": "src/**",
                "then": "docs/changelog.md", "per_commit": true,
            })),
            entry("pins.append-only", json!({
                "type": "diff-append-only", "level": "fail", "files": "docs/log.md",
            })),
            entry("pins.durable-sha", json!({
                "type": "durable-sha", "level": "fail", "files": "docs/**",
            })),
        ],
    })
}

/// Builds the scratch repository. `topic`'s commits: appending to the log
/// (passes every check), changing `src/a.rs` without the changelog (breaks
/// `pins.cochange` and the body line limit), citing the third commit's sha
/// in `docs/notes.md` (breaks `pins.durable-sha`: the commit is on `topic`,
/// not on `trunk`), and rewriting a log line (breaks `pins.append-only`, the
/// subject pattern, and `pins.trailer`).
fn pin_repo() -> PinRepo {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "trunk"]);
    git(root, &["config", "core.abbrev", "7"]);
    // The binary loads the schema from the working tree, so the file must
    // exist, but no commit may hold it: a commit sha would then depend on the
    // bytes of the live `template/knowledge/schema.json`.
    write_file(root, ".git/info/exclude", "knowledge/schema.json\n");
    write_file(root, ".gitattributes", "* -text\n");
    write_file(
        root,
        "knowledge/schema.json",
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json"),
        )
        .expect("read the vendored schema"),
    );
    write_file(
        root,
        "knowledge/areas.json",
        r#"{"global":{"paths":[]},"docs":{"paths":["docs/**"]}}"#,
    );
    write_file(
        root,
        "knowledge/pins.json",
        &serde_json::to_string(&pin_entries()).expect("serialize the topic"),
    );
    write_file(
        root,
        ".houserules.json",
        "{\"schema\": \"houserules/install@2.0.0\", \"defaultBranch\": \"trunk\"}\n",
    );
    write_file(root, "docs/log.md", "entry one\nentry two\n");
    write_file(root, "docs/notes.md", "notes\n");
    write_file(root, "docs/changelog.md", "changelog\n");
    write_file(root, "src/a.rs", "fn a() {}\n");
    let base = commit_all(root, 0, BASE_MESSAGE);

    git(root, &["checkout", "-q", "-b", "topic"]);
    write_file(root, "docs/log.md", "entry one\nentry two\nentry three\n");
    let first = commit_all(root, 1, TOPIC_MESSAGES[0]);
    write_file(root, "src/a.rs", "fn a() { 1 }\n");
    let second = commit_all(root, 2, TOPIC_MESSAGES[1]);
    write_file(
        root,
        "docs/notes.md",
        &format!("notes\nsee {} for the change\n", &second[..7]),
    );
    let third = commit_all(root, 3, TOPIC_MESSAGES[2]);
    write_file(root, "docs/log.md", "entry one\nentry 2\nentry three\n");
    let fourth = commit_all(root, 4, TOPIC_MESSAGES[3]);
    PinRepo {
        dir,
        base,
        topic: [first, second, third, fourth],
    }
}

/// Asserts that `actual` equals the committed fixture `name` byte for byte.
fn assert_pinned(name: &str, actual: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("runner-pins")
        .join(name);
    let expected = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    assert!(
        actual == expected,
        "{name} differs from the pinned bytes\n--- actual\n{actual}--- pinned\n{expected}"
    );
}

/// A run as text: the exit code, then stdout and stderr, each verbatim.
fn transcript(label: &str, output: &Output) -> String {
    let code = output
        .status
        .code()
        .map_or_else(|| "none".to_string(), |code| code.to_string());
    format!(
        "== {label}\nexit: {code}\n-- stdout\n{}-- stderr\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    )
}

/// Runs `audit` over `base..head` in `repo` and returns the run.
fn audit(repo: &PinRepo, head: &str) -> Output {
    houserules()
        .args(["audit", "--base", &repo.base, "--head", head])
        .current_dir(repo.root())
        .output()
        .expect("run audit")
}

/// Runs `check-commit` over the commits after `repo.base` up to `to`.
fn check_commit_range(repo: &PinRepo, to: &str) -> Output {
    houserules()
        .args(["check-commit", "--from", &repo.base, "--to", to])
        .current_dir(repo.root())
        .output()
        .expect("run check-commit")
}

/// The `PATH` of a hook run: the folder of the binary under test first, so
/// the hook finds this build as `houserules`.
fn hook_path_env() -> std::ffi::OsString {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_houserules"));
    let mut dirs = vec![binary.parent().expect("binary folder").to_path_buf()];
    dirs.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    env::join_paths(dirs).expect("join PATH entries")
}

/// Copies the shipped hook to `<root>/.githooks/commit-msg` (the place
/// `init` puts it), writes `message` to `<root>/MSG`, and runs the hook the
/// way git does: from the top level, by its relative path, through `sh`.
fn run_hook(repo: &PinRepo, message: &str) -> Output {
    let root = repo.root();
    let hook = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../template/workflows/reference/.githooks/commit-msg");
    fs::create_dir_all(root.join(".githooks")).expect("create the hook folder");
    fs::copy(&hook, root.join(".githooks").join("commit-msg")).expect("copy the hook");
    write_file(root, "MSG", &format!("{message}\n"));
    Command::new("sh")
        .arg(".githooks/commit-msg")
        .arg("MSG")
        .current_dir(root)
        .env("PATH", hook_path_env())
        .output()
        .expect("run the commit-msg hook")
}

/// The `audit` run over a range that breaks no check passes with the pinned
/// rows of `diff-append-only` and `durable-sha`.
#[test]
fn audit_pins_the_passing_rows_of_diff_append_only_and_durable_sha() {
    let repo = pin_repo();

    let output = audit(&repo, &repo.topic[0]);

    assert_eq!(output.status.code(), Some(0));
    assert_pinned("audit-pass.json", &String::from_utf8_lossy(&output.stdout));
}

/// The `audit` run over a range that breaks `diff-append-only` and
/// `durable-sha` fails with their pinned rows.
#[test]
fn audit_pins_the_failing_rows_of_diff_append_only_and_durable_sha() {
    let repo = pin_repo();

    let output = audit(&repo, &repo.topic[3]);

    assert_eq!(output.status.code(), Some(1));
    assert_pinned("audit-fail.json", &String::from_utf8_lossy(&output.stdout));
}

/// `check-commit` prints the pinned bytes for the range that breaks four
/// rules and for the range that breaks none.
#[test]
fn check_commit_pins_its_output_for_a_fixed_range() {
    let repo = pin_repo();

    let failing = transcript(
        "check-commit --from base --to topic-4",
        &check_commit_range(&repo, &repo.topic[3]),
    );
    let passing = transcript(
        "check-commit --from base --to topic-1",
        &check_commit_range(&repo, &repo.topic[0]),
    );

    assert_pinned("check-commit-range.txt", &format!("{failing}{passing}"));
}

/// The `commit-msg` hook prints the pinned bytes for the message of each
/// commit of the range.
#[test]
fn the_commit_msg_hook_pins_its_output_for_each_message_of_the_range() {
    let repo = pin_repo();

    let transcripts: String = TOPIC_MESSAGES
        .iter()
        .enumerate()
        .map(|(index, message)| {
            transcript(
                &format!("commit-msg topic-{}", index + 1),
                &run_hook(&repo, message),
            )
        })
        .collect();

    assert_pinned("commit-msg-hook-range.txt", &transcripts);
}
