//! The git plumbing of `audit`, `check-commit`, and the `durable-sha` check:
//! the one place that runs `git` for a range and reads its output (`rev`,
//! `full_commit_sha`, `git_diff`, `changed_files`, `tree_files`, `show_file`, `commits_in`,
//! `changed_files_for`, `paths_changed_against_any_parent`, `removed_lines`,
//! `worktree_changes`). Every function returns a named one-line `Err`, never a panic
//! (`houserules.crash-paths-are-named`).
//!
//! The review modules build their calls on the same runner (`command`,
//! `run_command`, `run_command_with_input`, and `run_predicate`) and the same
//! resolver (`resolve_commit`), and add the settings of their own to their
//! calls. A range may start at git's empty tree
//! (`is_empty_tree`): it then holds every commit and every path of its head.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use super::git_paths::InstallPrefix;

/// One captured `git` failure: the process's stderr (or, when `git` could
/// not even be launched, the OS error's own text standing in for it) --
/// the raw material `rev`/`git_diff`/the tree/show/log helpers each turn
/// into their own one-line message.
pub(crate) struct RawGitError {
    stderr: String,
}

/// The `git` command that every run starts from: `LC_ALL=C` for
/// locale-independent output, `GIT_NO_REPLACE_OBJECTS` so that it reads the
/// objects as committed (a local replace ref does not reach a CI clone), and
/// `root` as the working directory. A caller
/// adds its arguments, and any setting of its own, and runs it with
/// [`run_command`]. `pub(crate)`: the review modules build their `git` calls
/// from this one command, so the kit starts `git` in one place.
pub(crate) fn command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .env("LC_ALL", "C")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .current_dir(root);
    command
}

/// Runs `command` to its end and gives what it wrote to the standard output.
/// The standard input is closed (`Command::output` gives the child none). An
/// exit code of `tolerated` is no failure, for a command such as `git grep` or
/// `git merge-base`, which exits with 1 when it finds nothing.
pub(crate) fn run_command(
    command: &mut Command,
    tolerated: Option<i32>,
) -> Result<Vec<u8>, RawGitError> {
    match command.output() {
        Ok(output) => {
            let code = output.status.code();
            if output.status.success() || (code.is_some() && code == tolerated) {
                Ok(output.stdout)
            } else {
                Err(failure_of(&output))
            }
        }
        Err(error) => Err(launch_failure(&error)),
    }
}

/// The failure of a `command` that ended without the exit code it was run for:
/// its stderr.
fn failure_of(output: &Output) -> RawGitError {
    RawGitError {
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// The failure of a `command` that could not start: the text of the operating
/// system's error stands in for the stderr.
fn launch_failure(error: &std::io::Error) -> RawGitError {
    RawGitError {
        stderr: error.to_string(),
    }
}

/// Runs `command` to its end with `input` on its standard input, and gives what
/// it wrote to the standard output. The input goes in from a thread of its own
/// while the output is read, so a command that answers as it reads, such as
/// `git cat-file --batch`, does not stall on a full pipe, and a command that
/// reads all its input first, such as `git hash-object --stdin`, runs the same
/// way. A write that fails means that the command has stopped reading, and its
/// exit code and stderr name the failure.
pub(crate) fn run_command_with_input(
    command: &mut Command,
    input: &[u8],
) -> Result<Vec<u8>, RawGitError> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| launch_failure(&error))?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let output = std::thread::scope(|scope| {
        scope.spawn(move || {
            let _written = stdin.write_all(input);
        });
        child.wait_with_output()
    })
    .map_err(|error| launch_failure(&error))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(failure_of(&output))
    }
}

/// Runs `command` to its end and tells how it answered: `true` for the exit
/// code 0, `false` for the exit code 1. Any other end is a failure. It is for a
/// command that answers by its exit code, such as `git merge-base
/// --is-ancestor`.
pub(crate) fn run_predicate(command: &mut Command) -> Result<bool, RawGitError> {
    let output = command.output().map_err(|error| launch_failure(&error))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(failure_of(&output)),
    }
}

/// Runs `git` with `args` inside `root`, forcing `LC_ALL=C` for
/// locale-independent output.
fn run_git(root: &Path, args: &[&str]) -> Result<String, RawGitError> {
    run_command(command(root).args(args), None)
        .map(|stdout| String::from_utf8_lossy(&stdout).into_owned())
}

/// The line for a failure whose stderr holds no text.
const NO_STDERR_LINE: &str = "git command failed";

/// The first non-blank line of `stderr`, trimmed, with a generic message
/// for the case of a nonzero exit with fully empty stderr.
pub(crate) fn stderr_headline(error: &RawGitError) -> String {
    match error.stderr.lines().find(|line| !line.trim().is_empty()) {
        Some(line) => line.trim().to_string(),
        None => NO_STDERR_LINE.to_string(),
    }
}

/// The line of `error` that names the failure, for a command that prints a
/// notice before its error: the last line that starts with `fatal:` or
/// `error:`, else the last non-blank line, trimmed; the generic message for an
/// empty stderr. `git notes add -f` prints `Overwriting existing notes for
/// object <sha>` before it fails, so [`stderr_headline`] gives that notice for
/// the failure.
pub(crate) fn failure_headline(error: &RawGitError) -> String {
    let lines = || {
        error
            .stderr
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
    };
    lines()
        .rfind(|line| line.starts_with("fatal:") || line.starts_with("error:"))
        .or_else(|| lines().next_back())
        .map_or_else(|| NO_STDERR_LINE.to_string(), str::to_string)
}

/// The commit that `reference` names, as `git rev-parse --verify` prints it:
/// abbreviated with `abbreviate`, else the full sha. Any failure (a bad ref,
/// a ref that names no commit, or git itself failing to run) is
/// `bad ref "<reference>"`, discarding git's own message. The suffix
/// `^{commit}` makes a name such as `--help` no option: git reads it as a
/// revision and rejects it.
fn rev_parse_commit(root: &Path, reference: &str, abbreviate: bool) -> Result<String, String> {
    let revision = format!("{reference}^{{commit}}");
    let mut args = vec!["rev-parse"];
    if abbreviate {
        args.push("--short");
    }
    args.extend(["--verify", &revision]);
    run_git(root, &args)
        .map(|output| output.trim().to_string())
        .map_err(|_| format!("bad ref \"{reference}\""))
}

/// Resolves `reference` to its short commit sha. Any failure (a bad ref,
/// or git itself failing to run) is `bad ref "<reference>"`, discarding
/// git's own message. `pub(super)`: `check_commit`'s own range arm
/// resolves its `--from`/`--to` refs through this same function, so a bad
/// ref reads identically from either command.
pub(super) fn rev(root: &Path, reference: &str) -> Result<String, String> {
    rev_parse_commit(root, reference, true)
}

/// Resolves `reference` to the full sha of its commit; a failure reads as
/// [`rev`]'s does. `pub(crate)`: the review modules resolve a revision
/// through this function.
pub(crate) fn resolve_commit(root: &Path, reference: &str) -> Result<String, String> {
    rev_parse_commit(root, reference, false)
}

/// The full sha of the commit that the recorded sha `sha` names, or `None`
/// when `sha` names no commit or names more than one. A report records a
/// short sha, and a repack can lengthen the short sha of the same commit, so a
/// comparison reads both sides as full shas. `sha` must be 4 to 40 hex digits
/// (git's shortest abbreviation is 4): any other text is `None` with no `git`
/// call, so that text such as `--help` never reaches `git` as an option.
/// `pub(super)`: the `self_audit` comparison resolves the shas of a report
/// through this function.
pub(super) fn full_commit_sha(root: &Path, sha: &str) -> Option<String> {
    if !(4..=40).contains(&sha.len()) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    resolve_commit(root, sha).ok()
}

/// The object names of git's empty tree, one for each object format.
const EMPTY_TREES: [&str; 2] = [
    "4b825dc642cb6eb9a060e54bf8d69288fbee4904",
    "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321",
];

/// `true` when `name` is the full object name of git's empty tree. An audit
/// range that starts at the empty tree holds every commit of its head and
/// every path of the head's tree. `pub(super)`: the audit engine opens such a
/// range for the review commands.
pub(super) fn is_empty_tree(name: &str) -> bool {
    EMPTY_TREES.contains(&name)
}

/// The `git diff` range from `base` to `head`: three-dot, from the merge base
/// of the two commits to `head`. When `base` is git's empty tree, which has no
/// merge base, it is two-dot, and the diff holds every path of `head`.
pub(super) fn range(base: &str, head: &str) -> String {
    if is_empty_tree(base) {
        format!("{base}..{head}")
    } else {
        format!("{base}...{head}")
    }
}

/// Runs `git diff` with `args` (everything after `diff`). A shared merge
/// base miss between `base` and `head` reports
/// the fixed `no merge base between "<base>" and "<head>"` message; any
/// other failure carries git's own first non-empty stderr line.
/// `pub(super)`: `durable_sha`'s own `added_lines` reads a files-scoped
/// diff through this same function, not a second `git diff` invocation
/// of its own.
pub(super) fn git_diff(
    root: &Path,
    base: &str,
    head: &str,
    args: &[&str],
) -> Result<String, String> {
    let mut full_args = vec!["diff"];
    full_args.extend_from_slice(args);
    run_git(root, &full_args).map_err(|error| {
        if error.stderr.contains("no merge base") {
            format!("no merge base between \"{base}\" and \"{head}\"")
        } else {
            stderr_headline(&error)
        }
    })
}

/// Every file added, copied, modified, or renamed between `base` and
/// `head`'s merge base and `head`, as install-root paths
/// (`InstallPrefix::map`). `pub(super)`: the audit engine reads the range
/// through this function.
pub(super) fn changed_files(
    root: &Path,
    prefix: &InstallPrefix,
    base: &str,
    head: &str,
) -> Result<Vec<String>, String> {
    let output = git_diff(
        root,
        base,
        head,
        &[
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
            &range(base, head),
        ],
    )?;
    Ok(prefix.map(&output))
}

/// Every file in `head`'s tree, as install-root paths. `--full-tree` lists
/// the whole tree, not only the directory `root`: `InstallPrefix::map`
/// drops the paths outside the install. `pub(super)`: `check_commit`'s own
/// per-commit `co-change` retroactivity guard reads a commit's PARENT
/// tree through this same function, not a second `git ls-tree`
/// invocation of its own.
pub(super) fn tree_files(
    root: &Path,
    prefix: &InstallPrefix,
    head: &str,
) -> Result<Vec<String>, String> {
    let output = run_git(
        root,
        &["ls-tree", "-r", "--name-only", "-z", "--full-tree", head],
    )
    .map_err(|e| stderr_headline(&e))?;
    Ok(prefix.map(&output))
}

/// `path`'s content as it exists in `head`. `path` is an install-root
/// path: the `./` makes git read it from the directory `root` rather than
/// from the git top level. `pub(super)`: the audit context reads and caches
/// file content through this function.
pub(super) fn show_file(root: &Path, head: &str, path: &str) -> Result<String, String> {
    run_git(root, &["show", &format!("{head}:./{path}")]).map_err(|e| stderr_headline(&e))
}

/// One commit as the `commits` checks read it: its subject, its body, its
/// author name (`%an`) -- so a check can tell a generated message from a
/// human one (see `CommitsCheck::violation`) -- and its trailers. `sha` is
/// empty for a commit that has no real one yet (`unattributed`); a
/// non-empty `sha` is what `check_commit`'s own per-commit `co-change`
/// evaluation uses to fetch that one commit's changed files
/// (`changed_files_for`) -- `Commit` itself carries no file list, since a
/// range-level `CheckType::Commits` check never needs one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Commit {
    pub(super) sha: String,
    /// The abbreviated sha that a line names the commit by; empty with `sha`.
    pub(super) short_sha: String,
    pub(super) subject: String,
    pub(super) body: String,
    pub(super) author: String,
    /// The trailers of the message as `(key, value)`, in message order
    /// (`trailers::parse`). Empty when the reader did not parse them
    /// (`commits_in`'s `read_trailers`).
    pub(super) trailers: Vec<(String, String)>,
}

impl Commit {
    /// A message with no author or sha on record: the `check-commit` hook
    /// arm, which reads a message file before git has committed it --
    /// there is no commit object yet for a `co-change` check to diff.
    pub(super) fn unattributed(
        subject: String,
        body: String,
        trailers: Vec<(String, String)>,
    ) -> Self {
        Self {
            sha: String::new(),
            short_sha: String::new(),
            subject,
            body,
            author: String::new(),
            trailers,
        }
    }

    /// GitHub App and Dependabot commits carry an author name ending in
    /// `[bot]`; their bodies are generated (release links, metadata) and
    /// are not wrapped by a human.
    pub(super) fn is_bot_authored(&self) -> bool {
        self.author.ends_with("[bot]")
    }
}

/// Every commit strictly between `base` and `head` (two-dot range -- not
/// `range()`'s three-dot merge-base form), plus the author name and the
/// abbreviated sha, newest first. When `base` is git's empty tree, the commits
/// are every commit of `head`. With `read_trailers`, each commit also
/// carries its trailers (`trailers::parse` of its message, one `git` process
/// for each commit); without it they stay empty, so a reader that never
/// looks at a trailer pays for none. `pub(super)`: `check_commit`'s own range
/// arm reads a git range through this same function, not a second `git log`
/// invocation of its own.
pub(super) fn commits_in(
    root: &Path,
    base: &str,
    head: &str,
    read_trailers: bool,
) -> Result<Vec<Commit>, String> {
    let commits = if is_empty_tree(base) {
        head.to_string()
    } else {
        format!("{base}..{head}")
    };
    let output = run_git(
        root,
        &["log", "--format=%H%x00%h%x00%an%x00%s%x00%b%x1e", &commits],
    )
    .map_err(|e| stderr_headline(&e))?;
    output
        .split('\u{1e}')
        .map(|record| record.strip_prefix('\n').unwrap_or(record))
        .filter(|record| !record.is_empty())
        .map(|record| {
            let mut parts = record.splitn(5, '\u{0}');
            let sha = parts.next().unwrap_or_default().to_string();
            let short_sha = parts.next().unwrap_or_default().to_string();
            let author = parts.next().unwrap_or_default().to_string();
            let subject = parts.next().unwrap_or_default().to_string();
            let body = parts.next().unwrap_or_default().to_string();
            let trailers = if read_trailers {
                super::trailers::parse(root, &format!("{subject}\n\n{body}"))?
            } else {
                Vec::new()
            };
            Ok(Commit {
                sha,
                short_sha,
                subject,
                body,
                author,
                trailers,
            })
        })
        .collect()
}

/// Every path one commit (`sha`) itself changed, against its single first
/// parent -- `git diff-tree`'s own plumbing for "what did this commit
/// touch", one process per commit (`check_commit`'s range arm runs over
/// CI's own PR-sized commit range, never a full-history walk). A MERGE
/// commit (more than one parent) is read as touching NOTHING: plain `git
/// diff-tree` needs `-m`, `-c`, or `--first-parent` to diff a merge
/// against any parent at all, and this call passes none of them (verified
/// live against a scratch `--no-ff` merge: empty output, exit 0, no
/// error). That is the accepted contract here, not an oversight: this
/// repository merges fast-forward only (`process.ff-only-merges`), so no
/// merge commit ever reaches `check-commit`'s own range in practice, and a
/// silently-empty file list is exactly what a per-commit `co-change` check
/// needs on one -- `filter_matching` over an empty slice never matches
/// `if_changed`, so the check simply does not trigger, never a false
/// violation. `pub(super)`: `check_commit`'s own per-commit `co-change`
/// evaluation is the only caller. The paths are install-root paths
/// (`InstallPrefix::map`). An unresolvable `sha` or a failing `git` is a
/// named `Err`, never a panic (`houserules.crash-paths-are-named`).
pub(super) fn changed_files_for(
    root: &Path,
    prefix: &InstallPrefix,
    sha: &str,
) -> Result<Vec<String>, String> {
    let output = run_git(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-z",
            sha,
        ],
    )
    .map_err(|e| stderr_headline(&e))?;
    Ok(prefix.map(&output))
}

/// Every top-level path one commit (`sha`) changed against any of its
/// parents, NUL-separated and not mapped: `git diff-tree -m` prints the
/// diff against each parent of a merge, and the same paths as plain
/// `diff-tree` for a commit with one parent. `check_commit`'s skip decision
/// reads it through `InstallPrefix::changes_only_outside`, so a merge that
/// brings a path below the install root is not skipped. A failing `git` is
/// a named `Err` (`houserules.crash-paths-are-named`).
pub(super) fn paths_changed_against_any_parent(root: &Path, sha: &str) -> Result<String, String> {
    run_git(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--name-only",
            "-r",
            "-m",
            "-z",
            sha,
        ],
    )
    .map_err(|e| stderr_headline(&e))
}

/// Every removed (`-`-prefixed, excluding the `---` file header) line in
/// `files`'s diff between `base` and `head`. `pub(super)`: the audit context
/// reads the `diff-append-only` check's removed lines through this function.
pub(super) fn removed_lines(
    root: &Path,
    base: &str,
    head: &str,
    files: &[String],
) -> Result<Vec<String>, String> {
    let mut args = vec![range(base, head), "--".to_string()];
    args.extend(files.iter().cloned());
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git_diff(root, base, head, &arg_refs)?;
    Ok(output
        .lines()
        .filter(|line| line.starts_with('-') && !line.starts_with("---"))
        .map(str::to_string)
        .collect())
}

/// Every tracked path under `root` whose content in the working tree differs
/// from its content in `head`, as install-root paths in git's order. A path
/// that is staged, modified, or deleted is listed; an untracked file is not.
/// `pub(super)`: the `runner` check tests that the working tree is the tree
/// at `--head` through this function before it starts a tool.
pub(super) fn worktree_changes(root: &Path, head: &str) -> Result<Vec<String>, String> {
    let output = run_git(
        root,
        &["diff", "--name-only", "-z", "--relative", head, "--"],
    )
    .map_err(|e| stderr_headline(&e))?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// An empty repository on `main`.
    fn empty_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(dir.path())
            .output()
            .expect("run git");
        assert!(output.status.success());
        dir
    }

    /// Runs `git` in `dir` with a fixed identity; panics with stderr on a
    /// failure.
    fn git_in(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t.t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// The failure of a command whose stderr is `stderr`.
    fn failed_with(stderr: &str) -> RawGitError {
        RawGitError {
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn a_failure_is_named_by_its_fatal_line_and_not_by_the_notice_before_it() {
        let stderr = "Overwriting existing notes for object 65e49b2d54baf22aa55b0d91f4305e73d3d7c2dd\n\
            Author identity unknown\n\n*** Please tell me who you are.\n\nRun\n\n  \
            git config --global user.email \"you@example.com\"\n\
            to set your account's default identity.\n\n\
            fatal: unable to auto-detect email address (got 'amy@host.(none)')\n";

        assert_eq!(
            failure_headline(&failed_with(stderr)),
            "fatal: unable to auto-detect email address (got 'amy@host.(none)')"
        );
        assert_eq!(
            stderr_headline(&failed_with(stderr)),
            "Overwriting existing notes for object 65e49b2d54baf22aa55b0d91f4305e73d3d7c2dd"
        );
    }

    #[test]
    fn the_last_line_of_error_or_fatal_names_the_failure() {
        assert_eq!(
            failure_headline(&failed_with("error: first\nhint: text\nfatal: second\n")),
            "fatal: second"
        );
        assert_eq!(
            failure_headline(&failed_with("fatal: first\nerror: second\n")),
            "error: second"
        );
        assert_eq!(
            failure_headline(&failed_with("warning: a notice\n  error: only one  \n\n")),
            "error: only one"
        );
    }

    #[test]
    fn a_failure_without_an_error_line_is_named_by_its_last_line() {
        assert_eq!(
            failure_headline(&failed_with("a notice\nthe failure\n\n  \n")),
            "the failure"
        );
        assert_eq!(failure_headline(&failed_with("")), "git command failed");
        assert_eq!(
            failure_headline(&failed_with(" \n\n")),
            "git command failed"
        );
    }

    /// The full sha that `rev-parse` prints for `revision` in `dir`.
    fn rev_parse(dir: &Path, revision: &str) -> String {
        let output = Command::new("git")
            .args(["rev-parse", revision])
            .current_dir(dir)
            .output()
            .expect("run git");
        assert!(output.status.success());
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// A local replace ref, which a CI clone does not fetch, changes no
    /// commit that a ref resolves to: here a tag object replaced by a tag of
    /// another commit.
    #[test]
    fn a_replace_ref_changes_no_resolved_commit() {
        let dir = empty_repo();
        git_in(
            dir.path(),
            &["commit", "-q", "--allow-empty", "-m", "first"],
        );
        let first = rev_parse(dir.path(), "HEAD");
        git_in(
            dir.path(),
            &["commit", "-q", "--allow-empty", "-m", "second"],
        );
        git_in(
            dir.path(),
            &["tag", "-a", "release", "-m", "release", &first],
        );
        git_in(dir.path(), &["tag", "-a", "other", "-m", "release", "HEAD"]);
        let release = rev_parse(dir.path(), "refs/tags/release");
        let other = rev_parse(dir.path(), "refs/tags/other");
        git_in(dir.path(), &["replace", &release, &other]);
        assert_eq!(resolve_commit(dir.path(), "release"), Ok(first));
    }

    /// git's empty tree is known by the object name of either format; any
    /// other name is a commit or a tree of content.
    #[test]
    fn the_empty_tree_is_known_by_the_name_of_either_object_format() {
        let dir = empty_repo();
        let output = Command::new("git")
            .args(["hash-object", "-t", "tree", "--stdin"])
            .current_dir(dir.path())
            .output()
            .expect("run git");
        let host = String::from_utf8(output.stdout).expect("utf-8");

        assert!(is_empty_tree(host.trim()));
        assert!(is_empty_tree(
            "6ef19b41225c5369f1c104d45d8d85efa9b057b53b14b4b9b939dd74decc5321"
        ));
        for other in [
            "",
            "4b825dc",
            "HEAD",
            "0000000000000000000000000000000000000000",
        ] {
            assert!(!is_empty_tree(other), "{other:?}");
        }
    }

    /// A commit base has a merge base (three dots); the empty tree has none
    /// (two dots).
    #[test]
    fn range_is_three_dot_for_a_commit_and_two_dot_for_the_empty_tree() {
        let empty = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

        assert_eq!(range("abc1234", "def5678"), "abc1234...def5678");
        assert_eq!(range(empty, "def5678"), format!("{empty}..def5678"));
    }

    /// From the empty tree, the changed files are every file of the head and
    /// the commits are every commit that the head reaches.
    #[test]
    fn the_empty_tree_as_a_base_reads_every_file_and_every_commit_of_the_head() {
        let dir = empty_repo();
        let top = dir.path();
        std::fs::create_dir_all(top.join("app")).expect("create folders");
        std::fs::write(top.join("app/a.txt"), "one\n").expect("write file");
        std::fs::write(top.join("b.txt"), "one\n").expect("write file");
        git_in(top, &["add", "-A"]);
        git_in(top, &["commit", "-q", "-m", "first"]);
        std::fs::write(top.join("app/c.txt"), "two\n").expect("write file");
        git_in(top, &["add", "-A"]);
        git_in(top, &["commit", "-q", "-m", "second"]);
        let empty = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";
        let head = rev(top, "HEAD").expect("rev");
        let prefix = InstallPrefix::of(&top.join("app")).expect("the prefix of app");

        let files = changed_files(top, &prefix, empty, &head).expect("changed files");
        let commits = commits_in(top, empty, &head, false).expect("commits");

        assert_eq!(files, ["a.txt", "c.txt"]);
        assert_eq!(
            commits
                .iter()
                .map(|commit| commit.subject.as_str())
                .collect::<Vec<_>>(),
            ["second", "first"]
        );
    }

    /// A short prefix of a commit sha resolves to the full sha. A blob id, a
    /// sha that names no object, text that is no hex, and a length outside 4 to
    /// 40 give `None`, and text that starts with `-` never reaches `git`.
    #[test]
    fn full_commit_sha_resolves_a_commit_prefix_and_refuses_everything_else() {
        let dir = empty_repo();
        std::fs::write(dir.path().join("a.txt"), "one\n").expect("write file");
        git_in(dir.path(), &["add", "-A"]);
        git_in(dir.path(), &["commit", "-q", "-m", "base"]);
        let short = rev(dir.path(), "HEAD").expect("rev");
        let output = Command::new("git")
            .args(["rev-parse", "HEAD", "HEAD:a.txt"])
            .current_dir(dir.path())
            .output()
            .expect("run git");
        let text = String::from_utf8(output.stdout).expect("utf-8");
        let mut ids = text.lines();
        let commit = ids.next().expect("the commit id");
        let blob = ids.next().expect("the blob id");

        assert_eq!(full_commit_sha(dir.path(), &short).as_deref(), Some(commit));
        assert_eq!(
            full_commit_sha(dir.path(), &commit[..9]).as_deref(),
            Some(commit)
        );
        assert_eq!(full_commit_sha(dir.path(), commit).as_deref(), Some(commit));
        for refused in [
            blob,
            "deadbeefdeadbeef",
            "not-a-sha",
            "--help",
            "-1234567",
            "abc",
            "",
            &format!("{commit}0"),
        ] {
            assert_eq!(full_commit_sha(dir.path(), refused), None, "{refused:?}");
        }
    }

    /// The paths below the install root `app/` that differ from `HEAD`: a
    /// modified, a staged, and a deleted file are listed relative to the
    /// root, an untracked file and a change outside the root are not.
    #[test]
    fn worktree_changes_lists_tracked_differences_relative_to_the_root() {
        let dir = empty_repo();
        let top = dir.path();
        for path in ["app/a.txt", "app/b.txt", "app/c.txt", "other.txt"] {
            let file = top.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("create folders");
            std::fs::write(file, "one\n").expect("write file");
        }
        git_in(top, &["add", "-A"]);
        git_in(top, &["commit", "-q", "-m", "base"]);
        std::fs::write(top.join("app/a.txt"), "two\n").expect("modify");
        std::fs::write(top.join("app/b.txt"), "two\n").expect("modify");
        git_in(top, &["add", "app/b.txt"]);
        std::fs::remove_file(top.join("app/c.txt")).expect("delete");
        std::fs::write(top.join("app/untracked.txt"), "new\n").expect("write untracked");
        std::fs::write(top.join("other.txt"), "two\n").expect("modify outside the root");

        let changes = worktree_changes(&top.join("app"), "HEAD").expect("changes");

        assert_eq!(changes, ["a.txt", "b.txt", "c.txt"]);
    }

    /// A tree that matches `HEAD` lists nothing, and a revision that does not
    /// resolve is git's own line.
    #[test]
    fn worktree_changes_lists_nothing_for_a_clean_tree_and_names_a_bad_revision() {
        let dir = empty_repo();
        std::fs::write(dir.path().join("a.txt"), "one\n").expect("write file");
        git_in(dir.path(), &["add", "-A"]);
        git_in(dir.path(), &["commit", "-q", "-m", "base"]);

        assert_eq!(
            worktree_changes(dir.path(), "HEAD").expect("changes"),
            Vec::<String>::new()
        );
        let error = worktree_changes(dir.path(), "no-such-revision").unwrap_err();
        assert!(error.starts_with("fatal: "), "{error}");
    }

    /// Carries git's own stderr line for a diff failure that is not a
    /// merge-base miss: calls `git_diff` directly, since `audit()` never
    /// feeds it a malformed pathspec itself, and this is the only way to
    /// exercise its generic fallback.
    #[test]
    fn git_diff_carries_gits_own_stderr_line_for_a_non_merge_base_failure() {
        let dir = empty_repo();
        let error = git_diff(dir.path(), "main", "main", &["--name-only", ":(bad"]).unwrap_err();
        assert_eq!(error, "fatal: Invalid pathspec magic 'bad' in ':(bad'");
    }

    /// The discriminator looks for the literal substring "no merge base",
    /// so a pathspec that echoes back the different words "bad merge
    /// base" must not be mistaken for an actual merge-base miss.
    #[test]
    fn git_diff_does_not_mislabel_a_pathspec_failure_whose_text_contains_merge_base() {
        let dir = empty_repo();
        let error = git_diff(
            dir.path(),
            "main",
            "main",
            &["--name-only", ":(bad merge base"],
        )
        .unwrap_err();
        assert_eq!(
            error,
            "fatal: Invalid pathspec magic 'bad merge base' in ':(bad merge base'"
        );
    }

    /// A `cwd` that does not exist makes `Command::output` itself fail
    /// (git never launches), so `run_git`'s `RawGitError::stderr` carries
    /// the OS error's own text instead of anything git printed. This
    /// pins only that the fallback still produces a real, non-empty
    /// one-line message, since the OS error text is platform-specific.
    #[test]
    fn git_diff_falls_back_to_the_os_errors_own_message_when_git_never_runs() {
        let missing_root =
            std::env::temp_dir().join("houserules-audit-git-diff-missing-root-probe");
        let error =
            git_diff(&missing_root, "main", "main", &["--name-only"]).expect_err("git diff");
        assert!(!error.is_empty());
    }
}
