//! The subdirectory install (batch 33, HR-147): every command's default
//! root is the nearest directory at or above the current directory that
//! holds `.houserules.json`, searched no higher than the git top level,
//! else the git top level (`root::resolve_root`). Each case runs the
//! compiled binary from a nested directory of a scratch repository and
//! reads which install answered through the one standing rule each
//! install carries.
//!
//! Every path a gate matches is relative to the install root, and a path
//! outside the root is out of scope (spec section 8): the `audit` cases
//! below change files inside and outside the install and read the
//! command's own JSON.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

mod tracker_support;

use tracker_support::resolved;

/// A `Command` for the compiled `houserules` binary under test -- this
/// file's own copy of `common::houserules`, for the reason
/// `tests/check_commit.rs` gives for its own copy.
fn houserules() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_houserules"));
    command.env("HOUSERULES_SKIP_SELF_UPDATE", "1");
    command
}

/// The vendored knowledge schema's path, resolved at compile time from the
/// crate's own manifest directory so it does not depend on the test
/// binary's working directory.
fn vendored_schema_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json")
}

/// Runs `git` in `root`, panicking with its stderr on failure. Returns
/// stdout.
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

/// Stages everything under `root` and commits it with `message`; returns
/// the new commit's sha.
fn commit_all(root: &Path, message: &str) -> String {
    git(root, &["add", "-A"]);
    git(
        root,
        &[
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
        ],
    );
    git(root, &["rev-parse", "HEAD"]).trim().to_string()
}

fn write_file(root: &Path, relative: &str, content: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent directories");
    }
    fs::write(path, content).expect("write file");
}

/// A rule entry with the fields every test leaves alone. `fields` adds or
/// replaces keys: the id, area, summary, and check.
fn rule(fields: Value) -> Value {
    let mut entry = json!({
        "kind": "rule", "standing": false, "body": ["Body."], "tags": ["test"],
        "source": {"date": "2026-08-29", "by": "user"},
    });
    if let (Value::Object(entry_map), Value::Object(field_map)) = (&mut entry, fields) {
        entry_map.extend(field_map);
    }
    entry
}

/// Writes a knowledge base under `dir`: the areas `global` and `process`
/// with no paths and `rust` with the glob `crates/**`, and one topic file
/// per topic of `entries` (the id's first segment). With `marker`, `dir`
/// also holds `.houserules.json`.
fn write_install(dir: &Path, entries: &[Value], marker: bool) {
    let schema = fs::read_to_string(vendored_schema_path()).expect("read the vendored schema");
    write_file(dir, "knowledge/schema.json", &schema);
    write_file(
        dir,
        "knowledge/areas.json",
        r#"{"global":{"paths":[]},"process":{"paths":[]},"rust":{"paths":["crates/**"]}}"#,
    );
    let mut topics: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for entry in entries {
        let id = entry["id"].as_str().expect("entry id is a string");
        let topic = id.split('.').next().expect("entry id has a topic prefix");
        topics
            .entry(topic.to_string())
            .or_default()
            .push(entry.clone());
    }
    for (topic, topic_entries) in topics {
        let file = json!({
            "$schema": "./schema.json", "topic": topic, "title": format!("{topic} title"),
            "entries": topic_entries,
        });
        write_file(
            dir,
            &format!("knowledge/{topic}.json"),
            &serde_json::to_string(&file).expect("serialize the topic"),
        );
    }
    if marker {
        write_file(
            dir,
            ".houserules.json",
            "{\"schema\": \"houserules/install@2.0.0\"}\n",
        );
    }
}

/// Writes a knowledge base under `dir` with one standing rule whose summary
/// is `summary`. With `marker`, `dir` also holds `.houserules.json`.
fn write_knowledge(dir: &Path, summary: &str, marker: bool) {
    let standing = rule(json!({
        "id": "process.sequential", "area": "process", "standing": true, "summary": summary,
    }));
    write_install(dir, &[standing], marker);
}

/// A fresh scratch directory with `git init` run in `dir`.
fn git_init(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
}

/// The stdout of `houserules standing` run in `cwd`, which holds the
/// summary of the install that answered.
fn standing_in(cwd: &Path, extra_args: &[&std::ffi::OsStr]) -> String {
    let output = houserules()
        .arg("standing")
        .args(extra_args)
        .current_dir(cwd)
        .output()
        .expect("run standing");
    assert!(
        output.status.success(),
        "standing failed in {}: {}",
        cwd.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

/// The nearest `.houserules.json` above the current directory wins over one
/// further up, so a subdirectory install answers a command run inside it.
#[test]
fn a_nested_directory_resolves_to_the_nearest_install() {
    let repo = tempfile::tempdir().expect("tempdir");
    git_init(repo.path());
    write_knowledge(repo.path(), "top-level rule", true);
    write_knowledge(&repo.path().join("sub"), "subdirectory rule", true);
    let nested = repo.path().join("sub").join("x");
    fs::create_dir_all(&nested).expect("create the nested directory");

    let stdout = standing_in(&nested, &[]);

    assert!(stdout.contains("subdirectory rule"), "{stdout}");
    assert!(!stdout.contains("top-level rule"), "{stdout}");
}

/// A directory below an install with no marker of its own belongs to the
/// nearest install above it.
#[test]
fn a_nested_directory_without_a_nearer_marker_resolves_to_the_top_level_install() {
    let repo = tempfile::tempdir().expect("tempdir");
    git_init(repo.path());
    write_knowledge(repo.path(), "top-level rule", true);
    let nested = repo.path().join("sub").join("x");
    fs::create_dir_all(&nested).expect("create the nested directory");

    let stdout = standing_in(&nested, &[]);

    assert!(stdout.contains("top-level rule"), "{stdout}");
}

/// A subdirectory with a knowledge base and no `.houserules.json` is no
/// install: the default root is the git top level, as before the
/// subdirectory install.
#[test]
fn an_unmarked_subdirectory_is_no_install_and_the_default_root_is_the_git_top_level() {
    let repo = tempfile::tempdir().expect("tempdir");
    git_init(repo.path());
    write_knowledge(repo.path(), "top-level rule", true);
    write_knowledge(
        &repo.path().join("sub"),
        "unmarked subdirectory rule",
        false,
    );
    let nested = repo.path().join("sub").join("x");
    fs::create_dir_all(&nested).expect("create the nested directory");

    let stdout = standing_in(&nested, &[]);

    assert!(stdout.contains("top-level rule"), "{stdout}");
    assert!(!stdout.contains("unmarked subdirectory rule"), "{stdout}");
}

/// The search stops at the git top level: a `.houserules.json` in a
/// directory above the repository is no install of it, so D4 names the
/// marker that the top level lacks and not the one above.
#[test]
fn the_search_stops_at_the_git_top_level() {
    let outer = tempfile::tempdir().expect("tempdir");
    write_knowledge(outer.path(), "outer rule", true);
    let repo = outer.path().join("repo");
    fs::create_dir_all(&repo).expect("create the repository directory");
    git_init(&repo);
    write_knowledge(&repo, "repository rule", false);
    let nested = repo.join("sub");
    fs::create_dir_all(&nested).expect("create the nested directory");

    let output = houserules()
        .arg("standing")
        .current_dir(&nested)
        .output()
        .expect("run standing");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!(
            "{}: not found; run houserules update --id-prefix <prefix> to convert an install of houserules 1.x, or houserules init to start a new one\n",
            resolved(&repo).join(".houserules.json").display()
        )
    );
}

/// An explicit `--dir` is never searched: it names the root as given.
#[test]
fn an_explicit_dir_wins_over_a_nearer_marker() {
    let repo = tempfile::tempdir().expect("tempdir");
    git_init(repo.path());
    write_knowledge(repo.path(), "top-level rule", true);
    write_knowledge(&repo.path().join("sub"), "subdirectory rule", true);
    let nested = repo.path().join("sub").join("x");
    fs::create_dir_all(&nested).expect("create the nested directory");

    let stdout = standing_in(
        &nested,
        &[std::ffi::OsStr::new("--dir"), repo.path().as_os_str()],
    );

    assert!(stdout.contains("top-level rule"), "{stdout}");
    assert!(!stdout.contains("subdirectory rule"), "{stdout}");
}

/// A folder name with a space and a non-ASCII letter resolves: the search
/// reads git's own prefix and builds the candidates component by
/// component.
#[test]
fn a_folder_name_with_a_space_and_a_non_ascii_letter_resolves() {
    let repo = tempfile::tempdir().expect("tempdir");
    git_init(repo.path());
    write_knowledge(repo.path(), "top-level rule", true);
    let install = repo.path().join("sub dir").join("sä");
    write_knowledge(&install, "subdirectory rule", true);
    let nested = install.join("x");
    fs::create_dir_all(&nested).expect("create the nested directory");

    let stdout = standing_in(&nested, &[]);

    assert!(stdout.contains("subdirectory rule"), "{stdout}");
    assert!(!stdout.contains("top-level rule"), "{stdout}");
}

// ---- audit: install-root paths -------------------------------------------------

/// The install folders every audit case runs against: a plain one, and one
/// whose name holds a space and a non-ASCII letter (git quotes such a name
/// unless it prints NUL-terminated paths).
const INSTALL_FOLDERS: [&str; 2] = ["sub", "sub dir/sä"];

/// A `grep-absent` rule: no `FORBIDDEN` word in the `files` of `scope`.
fn no_forbidden_rule(id: &str, area: &str, standing: bool, files: &str, scope: &str) -> Value {
    rule(json!({
        "id": id, "area": area, "standing": standing, "summary": "No FORBIDDEN word.",
        "check": {
            "type": "grep-absent", "level": "fail", "files": files, "pattern": "FORBIDDEN",
            "scope": scope,
        },
    }))
}

/// A repository with an install in `install` (a path below the git top
/// level) that holds `entries`, and one commit of that install plus
/// `files` (paths from the top level). Returns the repository and the
/// commit's sha.
fn repo_with_install(
    install: &str,
    entries: &[Value],
    files: &[(String, &str)],
) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().expect("tempdir");
    git_init(repo.path());
    write_install(&repo.path().join(install), entries, true);
    for (path, content) in files {
        write_file(repo.path(), path, content);
    }
    let base = commit_all(repo.path(), "chore: base");
    (repo, base)
}

/// Runs `houserules audit --base <base>` in `cwd`. Returns the exit code
/// and the parsed JSON on stdout.
fn audit_in(cwd: &Path, base: &str) -> (Option<i32>, Value) {
    let output = houserules()
        .args(["audit", "--base", base])
        .current_dir(cwd)
        .output()
        .expect("run audit");
    let result = serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "audit stdout is not JSON ({error}); stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.code(), result)
}

/// Every row of an audit result as `(id, result, evidence)`, sorted by id.
fn rows(result: &Value) -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = result["rules"]
        .as_array()
        .expect("rules is an array")
        .iter()
        .map(|row| {
            let field = |name: &str| row[name].as_str().unwrap_or_default().to_string();
            (field("id"), field("result"), field("evidence"))
        })
        .collect();
    rows.sort();
    rows
}

/// A change inside the install that breaks an area rule fails `audit`; the
/// changed file reads as an install-root path, and a change outside the
/// install is not in the result.
#[test]
fn a_change_inside_the_install_that_breaks_an_area_rule_fails_audit() {
    for install in INSTALL_FOLDERS {
        let rules = [
            no_forbidden_rule("process.no-forbidden", "process", true, "**", "changed"),
            no_forbidden_rule("rust.no-forbidden", "rust", false, "crates/**", "changed"),
        ];
        let inside = format!("{install}/crates/lib.rs");
        let (repo, base) = repo_with_install(
            install,
            &rules,
            &[
                (inside.clone(), "ok\n"),
                ("other/crates/x.rs".into(), "ok\n"),
            ],
        );
        write_file(repo.path(), &inside, "FORBIDDEN\n");
        write_file(repo.path(), "other/crates/x.rs", "FORBIDDEN\n");
        commit_all(repo.path(), "feat: change");

        let (code, result) = audit_in(&repo.path().join(install).join("crates"), &base);

        assert_eq!(code, Some(1), "{install}: {result}");
        assert_eq!(
            result["changed_files"],
            json!(["crates/lib.rs"]),
            "{install}"
        );
        let violation = "crates/lib.rs:1 matches FORBIDDEN".to_string();
        assert_eq!(
            rows(&result),
            vec![
                (
                    "process.no-forbidden".into(),
                    "fail".into(),
                    violation.clone()
                ),
                ("rust.no-forbidden".into(), "fail".into(), violation),
            ],
            "{install}"
        );
    }
}

/// A change outside the install is out of scope: nothing changed, the area
/// rule is not in the package, and the standing rule checks no file.
#[test]
fn a_change_outside_the_install_is_out_of_scope() {
    for install in INSTALL_FOLDERS {
        let rules = [
            no_forbidden_rule("process.no-forbidden", "process", true, "**", "changed"),
            no_forbidden_rule("rust.no-forbidden", "rust", false, "crates/**", "changed"),
        ];
        let (repo, base) =
            repo_with_install(install, &rules, &[("other/crates/x.rs".into(), "ok\n")]);
        write_file(repo.path(), "other/crates/x.rs", "FORBIDDEN\n");
        commit_all(repo.path(), "feat: change");

        let (code, result) = audit_in(&repo.path().join(install), &base);

        assert_eq!(code, Some(0), "{install}: {result}");
        assert_eq!(result["changed_files"], json!([]), "{install}");
        assert_eq!(
            rows(&result),
            vec![(
                "process.no-forbidden".into(),
                "pass".into(),
                "0 files checked".into()
            )],
            "{install}"
        );
    }
}

/// A tree-scoped check reads the install's own tree: it counts the files
/// under the install and finds a violation there, never one outside it.
#[test]
fn a_tree_scoped_check_reads_only_the_install_tree() {
    for install in INSTALL_FOLDERS {
        let rules = [no_forbidden_rule(
            "rust.tree",
            "rust",
            false,
            "crates/**",
            "tree",
        )];
        let files = [
            (format!("{install}/crates/a.rs"), "ok\n"),
            ("other/crates/b.rs".to_string(), "FORBIDDEN\n"),
        ];
        let (repo, base) = repo_with_install(install, &rules, &files);
        write_file(repo.path(), &format!("{install}/crates/lib.rs"), "ok\n");
        commit_all(repo.path(), "feat: change");

        let (code, result) = audit_in(&repo.path().join(install), &base);

        assert_eq!(code, Some(0), "{install}: {result}");
        assert_eq!(
            rows(&result),
            vec![("rust.tree".into(), "pass".into(), "2 files checked".into())],
            "{install}"
        );

        write_file(
            repo.path(),
            &format!("{install}/crates/old.rs"),
            "FORBIDDEN\n",
        );
        commit_all(repo.path(), "feat: change again");

        let (code, result) = audit_in(&repo.path().join(install), &base);

        assert_eq!(code, Some(1), "{install}: {result}");
        assert_eq!(
            rows(&result),
            vec![(
                "rust.tree".into(),
                "fail".into(),
                "crates/old.rs:1 matches FORBIDDEN".into()
            )],
            "{install}"
        );
    }
}

/// A `co-change` check is not satisfied by a path outside the install, and
/// is satisfied by the install's own path.
#[test]
fn a_co_change_is_not_satisfied_by_a_path_outside_the_install() {
    for install in INSTALL_FOLDERS {
        let rules = [rule(json!({
            "id": "rust.cochange", "area": "rust", "summary": "lib.rs joins the harness.",
            "check": {
                "type": "co-change", "level": "fail", "if": "crates/lib.rs",
                "then": "crates/tests/harness.rs",
            },
        }))];
        let (repo, base) = repo_with_install(install, &rules, &[]);
        write_file(repo.path(), &format!("{install}/crates/lib.rs"), "x\n");
        write_file(repo.path(), "other/crates/tests/harness.rs", "x\n");
        commit_all(repo.path(), "feat: change");

        let (code, result) = audit_in(&repo.path().join(install), &base);

        assert_eq!(code, Some(1), "{install}: {result}");
        assert_eq!(
            rows(&result),
            vec![(
                "rust.cochange".into(),
                "fail".into(),
                "crates/lib.rs changed without crates/tests/harness.rs".into()
            )],
            "{install}"
        );

        write_file(
            repo.path(),
            &format!("{install}/crates/tests/harness.rs"),
            "x\n",
        );
        commit_all(repo.path(), "feat: change again");

        let (code, result) = audit_in(&repo.path().join(install), &base);

        assert_eq!(code, Some(0), "{install}: {result}");
        assert_eq!(
            rows(&result),
            vec![(
                "rust.cochange".into(),
                "pass".into(),
                "crates/lib.rs changed with crates/tests/harness.rs".into()
            )],
            "{install}"
        );
    }
}

/// A `diff-append-only` check reads the install's own file, named by its
/// install-root path.
#[test]
fn a_diff_append_only_check_reads_the_install_root_path() {
    for install in INSTALL_FOLDERS {
        let rules = [rule(json!({
            "id": "rust.append", "area": "rust", "summary": "The log only grows.",
            "check": {"type": "diff-append-only", "level": "fail", "files": "crates/log.md"},
        }))];
        let inside = format!("{install}/crates/log.md");
        let files = [
            (inside.clone(), "a\nb\n"),
            ("other/crates/log.md".to_string(), "a\nb\n"),
        ];
        let (repo, base) = repo_with_install(install, &rules, &files);
        write_file(repo.path(), &inside, "a\n");
        write_file(repo.path(), "other/crates/log.md", "a\n");
        commit_all(repo.path(), "feat: change");

        let (code, result) = audit_in(&repo.path().join(install), &base);

        assert_eq!(code, Some(1), "{install}: {result}");
        assert_eq!(
            rows(&result),
            vec![(
                "rust.append".into(),
                "fail".into(),
                "1 removed lines in crates/log.md".into()
            )],
            "{install}"
        );
    }
}

// ---- check-commit: commits outside the install ----------------------------------

/// What one `houserules` run printed.
struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn run_of(output: std::process::Output) -> Run {
    Run {
        code: output.status.code(),
        stdout: String::from_utf8(output.stdout).expect("utf8 stdout"),
        stderr: String::from_utf8(output.stderr).expect("utf8 stderr"),
    }
}

/// The conventional-commit rule: a subject starts with `feat: `, `fix: `,
/// or `chore: `.
fn conventional_commits_rule() -> Value {
    rule(json!({
        "id": "process.conventional-commits", "area": "process", "standing": true,
        "summary": "Conventional commits.",
        "check": {"type": "commits", "level": "fail", "subject": "^(feat|fix|chore): .+"},
    }))
}

/// The finding `check-commit` prints for a commit with `subject` against
/// `conventional_commits_rule`.
fn subject_finding(subject: &str) -> String {
    format!(
        "process.conventional-commits: commit \"{subject}\" does not match ^(feat|fix|chore): .+\n"
    )
}

/// The line that counts the commits `check-commit` skipped in the install
/// at `install`.
fn skipped_line(count: usize, install: &str) -> String {
    let noun = if count == 1 { "commit" } else { "commits" };
    format!("check-commit: skipped {count} {noun} with no change under {install}/\n")
}

/// Runs `houserules check-commit --from <from>` in `cwd`.
fn check_commit_from(cwd: &Path, from: &str) -> Run {
    run_of(
        houserules()
            .args(["check-commit", "--from", from])
            .current_dir(cwd)
            .output()
            .expect("run check-commit"),
    )
}

/// In a subdirectory install `check-commit --from` checks only the
/// commits that change a path under the install, and prints one line that
/// counts the skipped commits.
#[test]
fn check_commit_from_skips_and_counts_the_commits_outside_the_install() {
    for install in INSTALL_FOLDERS {
        let (repo, base) = repo_with_install(install, &[conventional_commits_rule()], &[]);
        let root = repo.path();
        write_file(root, &format!("{install}/a.txt"), "a\n");
        commit_all(root, "feat: inside");
        write_file(root, "other/b.txt", "b\n");
        commit_all(root, "bad outside subject");
        write_file(root, "other/c.txt", "c\n");
        commit_all(root, "another bad outside subject");
        write_file(root, &format!("{install}/d.txt"), "d\n");
        write_file(root, "other/e.txt", "e\n");
        commit_all(root, "fix: inside and outside");

        let run = check_commit_from(&root.join(install), &base);

        assert_eq!(run.code, Some(0), "{install}: {}", run.stderr);
        assert_eq!(
            run.stdout,
            format!("{}check-commit: ok\n", skipped_line(2, install)),
            "{install}"
        );
        assert_eq!(run.stderr, "", "{install}");

        write_file(root, &format!("{install}/f.txt"), "f\n");
        commit_all(root, "bad inside subject");

        let run = check_commit_from(&root.join(install), &base);

        assert_eq!(run.code, Some(1), "{install}");
        assert_eq!(run.stdout, skipped_line(2, install), "{install}");
        assert_eq!(
            run.stderr,
            subject_finding("bad inside subject"),
            "{install}"
        );
    }
}

/// One skipped commit reads in the singular.
#[test]
fn check_commit_from_counts_one_skipped_commit_in_the_singular() {
    let (repo, base) = repo_with_install("sub", &[conventional_commits_rule()], &[]);
    write_file(repo.path(), "other/b.txt", "b\n");
    commit_all(repo.path(), "bad outside subject");

    let run = check_commit_from(&repo.path().join("sub"), &base);

    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert_eq!(
        run.stdout,
        "check-commit: skipped 1 commit with no change under sub/\ncheck-commit: ok\n"
    );
}

/// `--from` in a subdirectory install prints the count line on every run,
/// `skipped 0 commits` included: the line also tells the user that the
/// install-root scope is active.
#[test]
fn check_commit_from_prints_a_zero_count_when_it_skipped_nothing() {
    for install in INSTALL_FOLDERS {
        let (repo, base) = repo_with_install(install, &[conventional_commits_rule()], &[]);
        write_file(repo.path(), &format!("{install}/a.txt"), "a\n");
        commit_all(repo.path(), "feat: inside");

        let run = check_commit_from(&repo.path().join(install), &base);

        assert_eq!(run.code, Some(0), "{install}: {}", run.stderr);
        assert_eq!(
            run.stdout,
            "check-commit: skipped 0 commits with no change under ".to_string()
                + install
                + "/\ncheck-commit: ok\n",
            "{install}"
        );
    }
}

/// A commit that changes no path at all is checked by both arms, with one
/// outcome: the range arm reads its changed paths, the hook arm reads an
/// empty index. A commit is skipped only when it changes at least one path
/// and none of them is under the install root.
#[test]
fn an_empty_commit_is_checked_by_both_arms() {
    for install in INSTALL_FOLDERS {
        let (repo, base) = repo_with_install(install, &[conventional_commits_rule()], &[]);
        let root = repo.path();
        let bad = commit_all(root, "bad empty subject");

        let run = check_commit_from(&root.join(install), &base);

        assert_eq!(run.code, Some(1), "{install}");
        assert_eq!(run.stdout, skipped_line(0, install), "{install}");
        assert_eq!(
            run.stderr,
            subject_finding("bad empty subject"),
            "{install}"
        );

        let run = check_commit_message(root, install, "bad empty subject\n");

        assert_eq!(run.code, Some(1), "{install}");
        assert_eq!(run.stdout, "", "{install}");
        assert_eq!(
            run.stderr,
            subject_finding("bad empty subject"),
            "{install}"
        );

        commit_all(root, "chore: empty and fine");

        let run = check_commit_from(&root.join(install), &bad);

        assert_eq!(run.code, Some(0), "{install}: {}", run.stderr);
        assert_eq!(
            run.stdout,
            format!("{}check-commit: ok\n", skipped_line(0, install)),
            "{install}"
        );

        let run = check_commit_message(root, install, "chore: empty and fine\n");

        assert_eq!(run.code, Some(0), "{install}: {}", run.stderr);
        assert_eq!(run.stdout, "check-commit: ok\n", "{install}");
    }
}

/// Merges `branch` into the current branch with a merge commit.
fn merge_no_ff(root: &Path, branch: &str, message: &str) {
    git(
        root,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "-c",
            "commit.gpgsign=false",
            "merge",
            "-q",
            "--no-ff",
            "-m",
            message,
            branch,
        ],
    );
}

/// A merge commit counts the paths of every parent diff. `git diff-tree`
/// without `-m` prints no path for a merge, so only the `-m` diff against
/// each parent shows the path a merge brings under the install root: a
/// merge that brings one is checked, and one that brings only paths outside
/// the install, from both sides, is skipped.
#[test]
fn check_commit_from_reads_every_parent_diff_of_a_merge() {
    for install in INSTALL_FOLDERS {
        let (repo, _base) = repo_with_install(install, &[conventional_commits_rule()], &[]);
        let root = repo.path();
        git(root, &["checkout", "-q", "-b", "side"]);
        write_file(root, &format!("{install}/side.txt"), "s\n");
        commit_all(root, "feat: side change inside");
        git(root, &["checkout", "-q", "main"]);
        write_file(root, "other/m.txt", "m\n");
        let before_merge = commit_all(root, "chore: main change outside");
        merge_no_ff(root, "side", "bad merge subject");

        let run = check_commit_from(&root.join(install), &before_merge);

        assert_eq!(run.code, Some(1), "{install}");
        assert_eq!(run.stdout, skipped_line(0, install), "{install}");
        assert_eq!(
            run.stderr,
            subject_finding("bad merge subject"),
            "{install}"
        );

        git(root, &["checkout", "-q", "-b", "side2"]);
        write_file(root, "other/s2.txt", "s\n");
        commit_all(root, "feat: side change outside");
        git(root, &["checkout", "-q", "main"]);
        write_file(root, "other/m2.txt", "m\n");
        let before_outside_merge = commit_all(root, "chore: main change outside again");
        merge_no_ff(root, "side2", "bad outside merge subject");

        let run = check_commit_from(&root.join(install), &before_outside_merge);

        assert_eq!(run.code, Some(0), "{install}: {}", run.stderr);
        assert_eq!(
            run.stdout,
            format!("{}check-commit: ok\n", skipped_line(2, install)),
            "{install}"
        );
    }
}

/// Commits a gitlink at `gitlink` (a path under the install) that records the
/// commit id of `id` repeated, together with a file outside the install, with
/// `message`. Gives the sha of the commit.
fn commit_gitlink_and_outside_file(root: &Path, gitlink: &str, id: char, message: &str) -> String {
    let entry = format!("160000,{},{gitlink}", id.to_string().repeat(40));
    git(root, &["update-index", "--add", "--cacheinfo", &entry]);
    fs::create_dir_all(root.join(gitlink)).expect("create the folder of the submodule");
    write_file(root, "other/x.txt", &format!("{message}\n"));
    commit_all(root, message)
}

/// A gitlink is a changed path whatever the repository says about ignoring
/// submodules: a commit that changes a gitlink under the install and a file
/// outside it is checked, not skipped, though `submodule.<name>.ignore` in
/// the configuration and `ignore = all` in `.gitmodules` both hide the
/// gitlink from `git diff-tree`.
#[test]
fn check_commit_from_does_not_skip_a_commit_that_changes_a_gitlink_under_the_install() {
    for install in INSTALL_FOLDERS {
        let (repo, base) = repo_with_install(install, &[conventional_commits_rule()], &[]);
        let root = repo.path();
        let gitlink = format!("{install}/mod");
        write_file(
            root,
            ".gitmodules",
            &format!("[submodule \"mod\"]\n\tpath = {gitlink}\n\turl = ./nowhere\n"),
        );
        commit_all(root, "chore: the submodule entry");
        git(root, &["config", "submodule.mod.ignore", "all"]);
        commit_gitlink_and_outside_file(root, &gitlink, '1', "bad config subject");

        let run = check_commit_from(&root.join(install), &base);

        assert_eq!(run.code, Some(1), "{install}, config: {}", run.stderr);
        assert_eq!(run.stdout, skipped_line(1, install), "{install}, config");
        assert_eq!(
            run.stderr,
            subject_finding("bad config subject"),
            "{install}, config"
        );

        git(root, &["config", "--unset", "submodule.mod.ignore"]);
        write_file(
            root,
            ".gitmodules",
            &format!(
                "[submodule \"mod\"]\n\tpath = {gitlink}\n\turl = ./nowhere\n\tignore = all\n"
            ),
        );
        let ignoring = commit_all(root, "chore: ignore the submodule");
        commit_gitlink_and_outside_file(root, &gitlink, '2', "bad gitmodules subject");

        let run = check_commit_from(&root.join(install), &ignoring);

        assert_eq!(run.code, Some(1), "{install}, .gitmodules: {}", run.stderr);
        assert_eq!(
            run.stdout,
            skipped_line(0, install),
            "{install}, .gitmodules"
        );
        assert_eq!(
            run.stderr,
            subject_finding("bad gitmodules subject"),
            "{install}, .gitmodules"
        );
    }
}

/// An explicit `--dir` outside any repository is one named line that does
/// not tell the user to pass `--dir`: the user already did.
#[test]
fn check_commit_with_a_dir_outside_a_repository_names_the_directory_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_install(dir.path(), &[conventional_commits_rule()], true);
    let scratch = tempfile::tempdir().expect("tempdir");
    let message_file = scratch.path().join("MSG");
    fs::write(&message_file, "feat: x\n").expect("write the message file");

    let run = run_of(
        houserules()
            .args(["check-commit", "--dir"])
            .arg(dir.path())
            .arg(&message_file)
            .current_dir(scratch.path())
            .output()
            .expect("run check-commit"),
    );

    assert_eq!(run.code, Some(2));
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        format!("{} is not inside a git repository\n", dir.path().display())
    );
}

/// `for` takes each path relative to the install root, as every gate does:
/// run in a subdirectory install, `crates/x.rs` reaches the install's
/// `crates/**` area, and the same file named from the git top level does
/// not (spec section 8: no mapping of top-level paths).
#[test]
fn for_takes_paths_relative_to_the_install_root() {
    for install in INSTALL_FOLDERS {
        let repo = tempfile::tempdir().expect("tempdir");
        git_init(repo.path());
        let area_rule = rule(json!({
            "id": "rust.rule", "area": "rust", "summary": "A rule of the crates area.",
        }));
        write_install(&repo.path().join(install), &[area_rule], true);
        let crates = repo.path().join(install).join("crates");
        fs::create_dir_all(&crates).expect("create crates");
        let for_path = |path: &str| -> (Vec<String>, Vec<String>) {
            let output = houserules()
                .args(["for", path])
                .current_dir(&crates)
                .output()
                .expect("run for");
            assert!(output.status.success(), "{install}: {path}");
            let result: Value = serde_json::from_slice(&output.stdout).expect("for JSON");
            let strings = |field: &str| -> Vec<String> {
                result[field]
                    .as_array()
                    .expect("an array")
                    .iter()
                    .map(|item| {
                        item.as_str()
                            .unwrap_or(item["id"].as_str().unwrap_or_default())
                    })
                    .map(str::to_string)
                    .collect()
            };
            (strings("areas"), strings("entries"))
        };

        let (areas, entries) = for_path("crates/x.rs");
        assert_eq!(areas, ["global", "rust"], "{install}");
        assert_eq!(entries, ["rust.rule"], "{install}");

        let (areas, entries) = for_path(&format!("{install}/crates/x.rs"));
        assert_eq!(areas, ["global"], "{install}");
        assert_eq!(entries, Vec::<String>::new(), "{install}");
    }
}

/// The help of `for` says each path is relative to the install root.
#[test]
fn for_help_says_each_path_is_relative_to_the_install_root() {
    let output = houserules()
        .args(["for", "--help"])
        .output()
        .expect("run for --help");
    let help = String::from_utf8(output.stdout).expect("utf8 stdout");
    let help = help.split_whitespace().collect::<Vec<_>>().join(" ");

    assert!(
        help.contains("each relative to the install root"),
        "not in: {help}"
    );
}

/// A staged read that git cannot do is one named line and exit 2, never a
/// skipped or a passed commit: a corrupt index file stands in for any git
/// failure of `git diff --cached`.
#[test]
fn the_hook_arm_names_a_staged_read_that_git_cannot_do() {
    let (repo, _base) = repo_with_install("sub", &[conventional_commits_rule()], &[]);
    let scratch = tempfile::tempdir().expect("tempdir");
    let message_file = scratch.path().join("MSG");
    fs::write(&message_file, "feat: x\n").expect("write the message file");
    let corrupt_index = scratch.path().join("index");
    fs::write(&corrupt_index, "this is not an index file\n").expect("write the index file");

    let run = run_of(
        houserules()
            .args(["check-commit", "--dir"])
            .arg(repo.path().join("sub"))
            .arg(&message_file)
            .current_dir(repo.path())
            .env("GIT_INDEX_FILE", &corrupt_index)
            .output()
            .expect("run check-commit"),
    );

    assert_eq!(run.code, Some(2));
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr.starts_with("git diff --cached failed: ") && run.stderr.lines().count() == 1,
        "{:?}",
        run.stderr
    );
}

/// The help of `check-commit` states the skip rule, the zero count, and
/// the one limit of the hook arm (an amendment), in the words of spec
/// section 8.
#[test]
fn check_commit_help_states_the_skip_rule_and_the_amend_limit() {
    let output = houserules()
        .args(["check-commit", "--help"])
        .output()
        .expect("run check-commit --help");
    let help = String::from_utf8(output.stdout).expect("utf8 stdout");
    let help = help.split_whitespace().collect::<Vec<_>>().join(" ");

    for phrase in [
        "skips a commit that changes at least one path and none of them under the install root",
        "a commit that changes no path is checked",
        "`skipped 0 commits` included",
        "an amendment that stages only paths outside the install root is skipped",
    ] {
        assert!(help.contains(phrase), "{phrase:?} is not in: {help}");
    }
}

/// An install at the git top level checks every commit and prints no
/// count line.
#[test]
fn an_install_at_the_top_level_checks_every_commit() {
    let repo = tempfile::tempdir().expect("tempdir");
    git_init(repo.path());
    write_install(repo.path(), &[conventional_commits_rule()], true);
    let base = commit_all(repo.path(), "chore: base");
    write_file(repo.path(), "other/b.txt", "b\n");
    commit_all(repo.path(), "bad subject");
    commit_all(repo.path(), "bad empty subject");

    let run = check_commit_from(repo.path(), &base);

    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, "");
    assert_eq!(
        run.stderr,
        format!(
            "{}{}",
            subject_finding("bad empty subject"),
            subject_finding("bad subject")
        )
    );
}

/// Runs the hook arm the way the `commit-msg` hook does: `check-commit
/// --dir <install> <message file>` from the git top level, with the
/// relative `GIT_INDEX_FILE` git exports to a hook. `message` is written
/// to a file outside the repository.
fn check_commit_message(repo: &Path, install: &str, message: &str) -> Run {
    let scratch = tempfile::tempdir().expect("tempdir");
    let message_file = scratch.path().join("MSG");
    fs::write(&message_file, message).expect("write the message file");
    run_of(
        houserules()
            .args(["check-commit", "--dir"])
            .arg(repo.join(install))
            .arg(&message_file)
            .current_dir(repo)
            .env("GIT_INDEX_FILE", ".git/index")
            .output()
            .expect("run check-commit"),
    )
}

/// The hook arm reads the staged files: a commit that stages nothing under
/// the install is skipped and counted; one that stages a path under it is
/// checked; one that stages nothing at all (an empty commit, or an
/// amendment) is checked too.
#[test]
fn the_hook_arm_reads_the_staged_files() {
    for install in INSTALL_FOLDERS {
        let (repo, _base) = repo_with_install(install, &[conventional_commits_rule()], &[]);
        let root = repo.path();

        write_file(root, "other/y.txt", "y\n");
        git(root, &["add", "--", "other/y.txt"]);
        let run = check_commit_message(root, install, "bad subject\n");
        assert_eq!(run.code, Some(0), "{install}: {}", run.stderr);
        assert_eq!(
            run.stdout,
            format!("{}check-commit: ok\n", skipped_line(1, install)),
            "{install}"
        );
        assert_eq!(run.stderr, "", "{install}");

        let inside = format!("{install}/z.txt");
        write_file(root, &inside, "z\n");
        git(root, &["add", "--", &inside]);
        let run = check_commit_message(root, install, "bad subject\n");
        assert_eq!(run.code, Some(1), "{install}");
        assert_eq!(run.stdout, "", "{install}");
        assert_eq!(run.stderr, subject_finding("bad subject"), "{install}");

        git(root, &["reset", "-q"]);
        let run = check_commit_message(root, install, "bad subject\n");
        assert_eq!(run.code, Some(1), "{install}");
        assert_eq!(run.stdout, "", "{install}");
        assert_eq!(run.stderr, subject_finding("bad subject"), "{install}");
    }
}

/// A per-commit `co-change` check reads install-root paths: a path outside
/// the install does not pair the trigger, and the mechanism it needs is
/// the one in the install's own tree.
#[test]
fn a_per_commit_co_change_reads_install_root_paths() {
    for install in INSTALL_FOLDERS {
        let rules = [rule(json!({
            "id": "rust.stamp", "area": "rust", "summary": "lib.rs changes carry the stamp.",
            "check": {
                "type": "co-change", "level": "fail", "if": "crates/lib.rs",
                "then": "crates/stamp", "per_commit": true,
            },
        }))];
        let files = [(format!("{install}/crates/stamp"), "1\n")];
        let (repo, base) = repo_with_install(install, &rules, &files);
        let root = repo.path();
        write_file(root, &format!("{install}/crates/lib.rs"), "x\n");
        write_file(root, "other/crates/stamp", "2\n");
        let split = commit_all(root, "feat: lib without the stamp");
        write_file(root, &format!("{install}/crates/lib.rs"), "y\n");
        write_file(root, &format!("{install}/crates/stamp"), "3\n");
        commit_all(root, "feat: lib with the stamp");

        let run = check_commit_from(&root.join(install), &base);

        assert_eq!(run.code, Some(1), "{install}");
        assert_eq!(
            run.stderr, "rust.stamp: crates/lib.rs changed without crates/stamp\n",
            "{install}"
        );

        let run = check_commit_from(&root.join(install), &split);

        assert_eq!(run.code, Some(0), "{install}: {}", run.stderr);
        assert_eq!(
            run.stdout,
            format!("{}check-commit: ok\n", skipped_line(0, install)),
            "{install}"
        );
    }
}
