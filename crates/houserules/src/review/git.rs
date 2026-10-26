//! The git calls of the review commands: one hermetic runner, the resolution
//! of a revision, the empty tree, the merge base, the test of ancestry, and the
//! check of the git version that `--attr-source` needs (spec 4.2).
//!
//! Every call starts from `rules::git::command`, the one place that starts
//! `git`, and runs with `LC_ALL=C` and `-c core.quotePath=false`, so that
//! neither the locale nor the quoting of a path changes what the readers
//! parse. The environment variables that reshape the parsed text never reach
//! `git` (see [`CLEARED_VARIABLES`]), and no call that this module starts reads
//! an object that a replace ref replaces. A failure is a one-line `Err`
//! (`houserules.crash-paths-are-named`): the first line of git's own message,
//! or the text of the operating system's error when git does not start. The
//! text of a run is decoded lossily, as `audit` decodes its path lists.

use std::path::Path;
use std::process::Command;

pub(crate) use crate::rules::git::resolve_commit;
use crate::rules::git::{
    self as shared, failure_headline, run_command, run_command_with_input, run_predicate,
    stderr_headline,
};

/// The oldest git that supports `--attr-source`, as `(major, minor)`.
const MINIMUM_VERSION: (u64, u64) = (2, 41);

/// The environment variables whose values change the parsed text, and which no
/// review call lets through (spec 4.2): `GIT_DIFF_OPTS` replaces the context
/// of `-U0`, and the two global pathspec settings change which paths a
/// pathspec matches.
const CLEARED_VARIABLES: [&str; 3] = [
    "GIT_DIFF_OPTS",
    "GIT_ICASE_PATHSPECS",
    "GIT_LITERAL_PATHSPECS",
];

/// The `git` command of a review call with `args`, run in `root`, from the
/// shared command, which reads no replaced object. It also skips the
/// system-wide attributes file, an attribute source that `--attr-source` does
/// not name.
fn command(root: &Path, args: &[&str]) -> Command {
    let mut command = shared::command(root);
    command.args(["-c", "core.quotePath=false"]).args(args);
    for name in CLEARED_VARIABLES {
        command.env_remove(name);
    }
    command.env("GIT_ATTR_NOSYSTEM", "1");
    command
}

/// Runs `git` with `args` in `root`. Returns what it wrote to the standard
/// output when it exited with 0 or with `tolerated`.
fn exec(root: &Path, args: &[&str], tolerated: Option<i32>) -> Result<Vec<u8>, String> {
    run_command(&mut command(root, args), tolerated).map_err(|error| stderr_headline(&error))
}

/// Runs `git` with `args` in `root` and returns what it wrote to the standard
/// output, as bytes. A run that exits with a code other than 0 is an `Err`.
pub(crate) fn run_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    exec(root, args, None)
}

/// Like [`run_bytes`], and the text of the output.
pub(crate) fn run(root: &Path, args: &[&str]) -> Result<String, String> {
    run_bytes(root, args).map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Like [`run`], for a command that prints a notice before its error, such as
/// `git notes add -f`: a run that fails gives the `fatal:` or `error:` line of
/// git as its line (`failure_headline`), and the notice never stands for the
/// failure.
pub(crate) fn run_naming_the_failure(root: &Path, args: &[&str]) -> Result<String, String> {
    run_command(&mut command(root, args), None)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| failure_headline(&error))
}

/// Like [`run_bytes`], and a run that exits with `tolerated` is no failure
/// either: `git grep` and `git merge-base` exit with 1 when they find nothing.
pub(crate) fn run_tolerating(
    root: &Path,
    args: &[&str],
    tolerated: i32,
) -> Result<Vec<u8>, String> {
    exec(root, args, Some(tolerated))
}

/// Like [`run_bytes`], with `input` on the standard input of git. The input goes
/// in while the output is read, so the command may answer as it reads
/// (`cat-file --batch`) or read all its input first (`hash-object --stdin`).
pub(crate) fn run_bytes_with_input(
    root: &Path,
    args: &[&str],
    input: &[u8],
) -> Result<Vec<u8>, String> {
    run_command_with_input(&mut command(root, args), input).map_err(|error| stderr_headline(&error))
}

/// Like [`run_bytes_with_input`], and the text of the output.
pub(crate) fn run_with_input(root: &Path, args: &[&str], input: &[u8]) -> Result<String, String> {
    run_bytes_with_input(root, args, input)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// Whether `ancestor` is an ancestor of `descendant`, or the same commit
/// (`git merge-base --is-ancestor`). An object that git does not have is an
/// `Err` with the line of git.
pub(crate) fn is_ancestor(root: &Path, ancestor: &str, descendant: &str) -> Result<bool, String> {
    run_predicate(&mut command(
        root,
        &[
            "merge-base",
            "--is-ancestor",
            "--end-of-options",
            ancestor,
            descendant,
        ],
    ))
    .map_err(|error| stderr_headline(&error))
}

/// The name of git's empty tree in the object format of the repository.
pub(crate) fn empty_tree(root: &Path) -> Result<String, String> {
    run(root, &["hash-object", "-t", "tree", "--stdin"]).map(|name| name.trim().to_string())
}

/// The full object name of what a range starts from: `empty_tree` when git
/// resolves `rev` to it, in any spelling that git knows, else the full sha of
/// the commit that `rev` names (`bad ref "<rev>"` when it names none).
pub(crate) fn resolve_start(root: &Path, rev: &str, empty_tree: &str) -> Result<String, String> {
    let names_the_empty_tree = run(root, &["rev-parse", "--verify", "--end-of-options", rev])
        .is_ok_and(|name| name.trim() == empty_tree);
    if names_the_empty_tree {
        Ok(empty_tree.to_string())
    } else {
        resolve_commit(root, rev)
    }
}

/// The merge base of the commits `base` and `head`, or `None` when they share
/// no history.
pub(crate) fn merge_base(root: &Path, base: &str, head: &str) -> Result<Option<String>, String> {
    let output = run_tolerating(root, &["merge-base", base, head], 1)?;
    let text = String::from_utf8_lossy(&output);
    Ok(Some(text.trim().to_string()).filter(|sha| !sha.is_empty()))
}

/// The first seven characters of `sha`, or all of it when it is shorter.
pub(crate) fn short(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

/// The number at the start of `text`, if it starts with a digit.
fn leading_number(text: &str) -> Option<u64> {
    let digits = text.len() - text.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    text[..digits].parse().ok()
}

/// The line that a command prints when `version_text`, the output of `git
/// --version`, names a git older than 2.41; `None` for a newer git and for a
/// text that names no version.
pub(crate) fn attr_source_line(command: &str, version_text: &str) -> Option<String> {
    let version = version_text
        .trim()
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    let mut parts = version.split('.');
    let major = leading_number(parts.next()?)?;
    let minor = leading_number(parts.next()?)?;
    ((major, minor) < MINIMUM_VERSION).then(|| {
        format!(
            "{command}: git {version} is older than {}.{}, which --attr-source needs",
            MINIMUM_VERSION.0, MINIMUM_VERSION.1
        )
    })
}

/// Runs `git --version` and gives the [`attr_source_line`] of `command` as the
/// `Err` when the git is older than 2.41, which `--attr-source` needs. A git
/// that cannot run is `<command>: could not run git: <reason>`.
pub(crate) fn require_attr_source(command: &str) -> Result<(), String> {
    let text = run(Path::new("."), &["--version"])
        .map_err(|error| format!("{command}: could not run git: {error}"))?;
    match attr_source_line(command, &text) {
        Some(line) => Err(line),
        None => Ok(()),
    }
}

/// A scratch repository for the tests of the review modules.
#[cfg(test)]
pub(crate) mod scratch {
    use std::path::Path;
    use std::process::{Command, Output};

    use tempfile::TempDir;

    /// A git repository in a temporary directory, on the branch `main`, with
    /// its own identity and its own `core.excludesFile` at the null device: a
    /// repository-local value overrides the global one, so no ignore file of
    /// the host hides a file from a test. Dropping it removes the directory.
    pub(crate) struct Repo {
        dir: TempDir,
    }

    impl Repo {
        /// An empty repository.
        pub(crate) fn new() -> Self {
            let repo = Self {
                dir: tempfile::tempdir().expect("tempdir"),
            };
            repo.git(&["init", "-q", "-b", "main"]);
            for (key, value) in [
                ("user.name", "t"),
                ("user.email", "t@t.t"),
                ("commit.gpgsign", "false"),
                ("core.autocrlf", "false"),
                ("core.excludesFile", "/dev/null"),
            ] {
                repo.git(&["config", key, value]);
            }
            repo
        }

        /// The top of the work tree.
        pub(crate) fn root(&self) -> &Path {
            self.dir.path()
        }

        /// Writes `content` to `path` below the top, and the folders above it.
        pub(crate) fn write(&self, path: &str, content: &str) {
            self.write_bytes(path, content.as_bytes());
        }

        /// Writes `content` to `path` below the top, and the folders above it.
        pub(crate) fn write_bytes(&self, path: &str, content: &[u8]) {
            let file = self.dir.path().join(path);
            std::fs::create_dir_all(file.parent().expect("a parent folder"))
                .expect("create the folders");
            std::fs::write(file, content).expect("write the file");
        }

        /// Runs `git` with `args` in the top and returns its output. It fails
        /// the test with git's error when git exits with a code other than 0.
        pub(crate) fn git(&self, args: &[&str]) -> String {
            self.git_with_env(args, &[])
        }

        /// Like [`Repo::git`], with the environment variables `envs` set for
        /// the run, such as the dates of a commit.
        pub(crate) fn git_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> String {
            let output = fixture_git(self.dir.path(), args, envs);
            assert!(
                output.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8_lossy(&output.stdout).into_owned()
        }

        /// Stages every change and commits it with `message`, empty commits
        /// included. Returns the full sha of the commit.
        pub(crate) fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]);
            self.commit_staged(message)
        }

        /// Commits what the index holds, with no staging first, empty commits
        /// included. Returns the full sha of the commit.
        pub(crate) fn commit_staged(&self, message: &str) -> String {
            self.git(&["commit", "-q", "--allow-empty", "-m", message]);
            self.git(&["rev-parse", "HEAD"]).trim().to_string()
        }
    }

    /// Runs the unit tests `names` of this test binary again, as one child process, and returns
    /// the finished child. `configure` shapes the child's environment. The child runs
    /// sequentially and with `--no-capture`, so what a test prints reaches the output.
    fn rerun(names: &[&str], configure: impl FnOnce(&mut Command)) -> Output {
        let exe = std::env::current_exe().expect("the path of the test binary");
        let mut command = Command::new(exe);
        command
            .arg("--exact")
            .args(names)
            .args(["--no-capture", "--test-threads=1"]);
        configure(&mut command);
        command.output().expect("run the test binary again")
    }

    /// Runs the unit test `name` of this test binary again, as a child process
    /// whose environment also holds `envs`, and gives what the child printed.
    /// A test cannot change its own environment (it is process-wide state), so
    /// a test of an environment variable reads the child's result.
    pub(crate) fn rerun_test(name: &str, envs: &[(&str, &str)]) -> String {
        let output = rerun(&[name], |command| {
            command.envs(envs.iter().copied());
        });
        assert!(
            output.status.success(),
            "{name} failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// The warning that git prints when it stages a file of LF lines under `core.autocrlf=true`.
    const LINE_ENDING_WARNING: &str = "LF will be replaced by CRLF";

    /// Requires that the unit tests `names` pass in a child process under the configuration of
    /// the autocrlf suite (`core.autocrlf=true`, injected through `GIT_CONFIG_COUNT`) and that no
    /// git child of a fixture prints the line-ending warning. The child gets the locale and the
    /// configuration files of [`fixture_git`], so the host cannot change the text of the warning.
    /// The child must run every test: a name that matches none fails.
    pub(crate) fn assert_no_line_ending_warning_under_autocrlf(names: &[&str]) {
        let output = rerun(names, |command| {
            with_default_git(command)
                .env("GIT_CONFIG_COUNT", "1")
                .env("GIT_CONFIG_KEY_0", "core.autocrlf")
                .env("GIT_CONFIG_VALUE_0", "true");
        });
        let printed = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.status.success(),
            "{names:?} failed under core.autocrlf=true: {printed}"
        );
        assert!(
            printed.contains(&format!("test result: ok. {} passed", names.len())),
            "the child ran every test: {printed}"
        );
        assert!(!printed.contains(LINE_ENDING_WARNING), "{printed}");
    }

    /// Leaves the locale and the git configuration of the host out of `command` and the git that
    /// it starts: `LC_ALL=C`, the system and global files at the null device, and none of the
    /// variables that inject configuration (`GIT_CONFIG_COUNT` with its pairs, and
    /// `GIT_CONFIG_PARAMETERS`).
    fn with_default_git(command: &mut Command) -> &mut Command {
        for (name, _) in std::env::vars_os() {
            let injects_configuration = name.to_str().is_some_and(|name| {
                matches!(name, "GIT_CONFIG_COUNT" | "GIT_CONFIG_PARAMETERS")
                    || name.starts_with("GIT_CONFIG_KEY_")
                    || name.starts_with("GIT_CONFIG_VALUE_")
            });
            if injects_configuration {
                command.env_remove(name);
            }
        }
        command
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
    }

    /// Runs `git` in `dir` without the configuration of the host (see [`with_default_git`]), so
    /// that a fixture is the same on every host. The commands under test keep the environment they
    /// inherit. `envs` are the variables the run sets, such as the dates of a commit.
    pub(crate) fn fixture_git(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let mut command = Command::new("git");
        with_default_git(&mut command)
            .args(args)
            .current_dir(dir)
            .envs(envs.iter().copied())
            .output()
            .expect("run git")
    }
}

#[cfg(test)]
mod tests {
    use super::scratch::Repo;
    use super::*;

    #[test]
    fn a_git_older_than_2_41_gives_the_line_of_the_command() {
        assert_eq!(
            attr_source_line("route", "git version 2.39.0\n").as_deref(),
            Some("route: git 2.39.0 is older than 2.41, which --attr-source needs")
        );
        assert_eq!(
            attr_source_line("route", "git version 2.40.4\n").as_deref(),
            Some("route: git 2.40.4 is older than 2.41, which --attr-source needs")
        );
        assert_eq!(
            attr_source_line("evidence", "git version 2.39.3 (Apple Git-146)\n").as_deref(),
            Some("evidence: git 2.39.3 is older than 2.41, which --attr-source needs")
        );
        assert_eq!(
            attr_source_line("bundle check", "git version 1.9.5").as_deref(),
            Some("bundle check: git 1.9.5 is older than 2.41, which --attr-source needs")
        );
        assert_eq!(
            attr_source_line("route", "git version 2.9.0").as_deref(),
            Some("route: git 2.9.0 is older than 2.41, which --attr-source needs")
        );
    }

    #[test]
    fn a_git_of_2_41_or_later_gives_no_line() {
        for text in [
            "git version 2.41.0\n",
            "git version 2.56.0.windows.1\n",
            "git version 2.41.0-rc0",
            "git version 3.0.0",
            "git version 2.100.1 (Apple Git-200)",
        ] {
            assert_eq!(attr_source_line("route", text), None, "{text}");
        }
    }

    #[test]
    fn a_text_that_names_no_version_gives_no_line() {
        for text in ["", "git version", "not git", "git version two.forty"] {
            assert_eq!(attr_source_line("route", text), None, "{text:?}");
        }
    }

    #[test]
    fn the_installed_git_passes_the_version_check() {
        assert_eq!(require_attr_source("evidence"), Ok(()));
    }

    /// The name of the test that the tests of the version check run again as a
    /// child process (`scratch::rerun_test`).
    #[cfg(unix)]
    const RERUN_VERSION_CHECK: &str =
        "review::git::tests::rerun_prints_the_version_check_of_its_environment";

    /// Prints the result of the version check for the command that
    /// `HR_RERUN_COMMAND` names, and does nothing when it is absent. The child
    /// process finds `git` on the `PATH` it gets.
    #[test]
    fn rerun_prints_the_version_check_of_its_environment() {
        let Ok(command) = std::env::var("HR_RERUN_COMMAND") else {
            return;
        };
        println!("CHECK {:?}", require_attr_source(&command));
    }

    /// The result of the version check for `evidence` in a child process whose
    /// `PATH` is `path`, as debug text.
    #[cfg(unix)]
    fn version_check_with_path(path: &Path) -> String {
        let path = path.to_str().expect("a UTF-8 path");
        let printed = scratch::rerun_test(
            RERUN_VERSION_CHECK,
            &[("HR_RERUN_COMMAND", "evidence"), ("PATH", path)],
        );
        printed
            .lines()
            .find_map(|line| line.split_once("CHECK ").map(|(_, check)| check))
            .expect("the child printed the check")
            .to_string()
    }

    #[cfg(unix)]
    #[test]
    fn a_git_that_fails_gives_its_first_line_of_stderr_as_the_reason() {
        use std::os::unix::fs::PermissionsExt;

        let folder = tempfile::tempdir().expect("tempdir");
        let stub = folder.path().join("git");
        std::fs::write(
            &stub,
            "#!/bin/sh\necho 'fatal: stub git is broken' >&2\necho 'a second line' >&2\nexit 1\n",
        )
        .expect("write the stub");
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
            .expect("make the stub executable");

        assert_eq!(
            version_check_with_path(folder.path()),
            r#"Err("evidence: could not run git: fatal: stub git is broken")"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_git_that_does_not_start_gives_the_text_of_the_operating_system_as_the_reason() {
        let folder = tempfile::tempdir().expect("tempdir");
        let reason = Command::new("git")
            .env("PATH", folder.path())
            .output()
            .expect_err("no git on that path")
            .to_string();

        assert_eq!(
            version_check_with_path(folder.path()),
            format!(r#"Err("evidence: could not run git: {reason}")"#)
        );
    }

    /// The setting of the environment variable `name` that a review call
    /// gives its child: `Some(Some(value))` for a value, `Some(None)` for a
    /// removed variable, and `None` for one that the call leaves alone.
    fn environment_of_call(name: &str) -> Option<Option<String>> {
        let call = command(Path::new("."), &["status"]);
        call.get_envs()
            .find(|(variable, _)| *variable == name)
            .map(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()))
    }

    #[test]
    fn a_review_call_skips_the_system_attributes_file() {
        // Only a file under /etc holds the system attributes, so the call is
        // tested through the variable that tells git to skip it.
        assert_eq!(
            environment_of_call("GIT_ATTR_NOSYSTEM"),
            Some(Some("1".to_string()))
        );
    }

    #[test]
    fn a_review_call_reads_no_replaced_object() {
        assert_eq!(
            environment_of_call("GIT_NO_REPLACE_OBJECTS"),
            Some(Some("1".to_string()))
        );
    }

    #[test]
    fn a_review_call_clears_the_variables_that_reshape_the_parsed_text() {
        for name in [
            "GIT_DIFF_OPTS",
            "GIT_ICASE_PATHSPECS",
            "GIT_LITERAL_PATHSPECS",
        ] {
            assert_eq!(environment_of_call(name), Some(None), "{name}");
        }
    }

    #[test]
    fn a_review_call_runs_in_the_c_locale_with_unquoted_paths() {
        let call = command(Path::new("."), &["status"]);

        assert_eq!(environment_of_call("LC_ALL"), Some(Some("C".to_string())));
        assert_eq!(
            call.get_args().take(2).collect::<Vec<_>>(),
            ["-c", "core.quotePath=false"]
        );
    }

    #[test]
    fn a_sha_shortens_to_seven_characters() {
        assert_eq!(short("0123456789abcdef"), "0123456");
        assert_eq!(short("0123456"), "0123456");
        assert_eq!(short("0123"), "0123");
        assert_eq!(short(""), "");
    }

    #[test]
    fn a_revision_resolves_to_the_full_sha_of_its_commit() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let first = repo.commit("first");

        assert_eq!(resolve_commit(repo.root(), "HEAD"), Ok(first.clone()));
        assert_eq!(resolve_commit(repo.root(), &first[..9]), Ok(first.clone()));
        assert_eq!(resolve_commit(repo.root(), "main"), Ok(first));
    }

    #[test]
    fn a_revision_that_names_no_commit_is_a_bad_ref_and_never_an_option() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        repo.commit("first");

        for rev in ["no-such-branch", "--help", "-1234567", "HEAD:a.txt"] {
            assert_eq!(
                resolve_commit(repo.root(), rev),
                Err(format!("bad ref \"{rev}\"")),
                "{rev}"
            );
        }
    }

    #[test]
    fn the_empty_tree_is_the_name_git_gives_it() {
        let repo = Repo::new();

        assert_eq!(
            empty_tree(repo.root()).as_deref(),
            Ok("4b825dc642cb6eb9a060e54bf8d69288fbee4904")
        );
    }

    #[test]
    fn a_start_is_a_commit_or_the_empty_tree_and_never_an_option() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let first = repo.commit("first");
        let empty = empty_tree(repo.root()).expect("the empty tree");

        assert_eq!(resolve_start(repo.root(), "HEAD", &empty), Ok(first));
        assert_eq!(
            resolve_start(repo.root(), &empty, &empty),
            Ok(empty.clone())
        );
        for rev in ["--help", "HEAD^{tree}", "no-such-branch"] {
            assert_eq!(
                resolve_start(repo.root(), rev, &empty),
                Err(format!("bad ref \"{rev}\"")),
                "{rev}"
            );
        }
    }

    #[test]
    fn the_merge_base_of_two_branches_is_their_fork_point() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let fork = repo.commit("fork");
        repo.git(&["checkout", "-q", "-b", "side"]);
        repo.write("b.txt", "two\n");
        let side = repo.commit("side");
        repo.git(&["checkout", "-q", "main"]);
        repo.write("c.txt", "three\n");
        let main = repo.commit("main");

        assert_eq!(
            merge_base(repo.root(), &main, &side),
            Ok(Some(fork.clone()))
        );
        assert_eq!(merge_base(repo.root(), &fork, &side), Ok(Some(fork)));
    }

    #[test]
    fn two_histories_with_no_common_commit_have_no_merge_base() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let main = repo.commit("main");
        repo.git(&["checkout", "-q", "--orphan", "other"]);
        repo.write("b.txt", "two\n");
        let other = repo.commit("other");

        assert_eq!(merge_base(repo.root(), &main, &other), Ok(None));
    }

    #[test]
    fn a_failing_run_gives_the_first_line_of_the_message_of_git() {
        let repo = Repo::new();

        let error = run(repo.root(), &["rev-parse", "--verify", "no-such-ref"])
            .expect_err("the ref does not exist");

        assert!(error.starts_with("fatal: "), "{error}");
        assert!(!error.contains('\n'), "{error}");
    }

    #[test]
    fn a_run_that_cannot_start_gives_the_text_of_the_operating_system() {
        let repo = Repo::new();
        let missing = repo.root().join("no-such-folder");

        let error = run(&missing, &["status"]).expect_err("the folder does not exist");

        assert!(!error.is_empty());
        assert!(!error.contains('\n'), "{error}");
    }

    #[test]
    fn a_tolerated_exit_code_is_no_failure() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        repo.commit("first");

        let no_match = run_tolerating(repo.root(), &["grep", "-l", "-F", "-e", "zzz", "HEAD"], 1)
            .expect("exit 1 is tolerated");
        let bad_ref = run_tolerating(repo.root(), &["grep", "-l", "-F", "-e", "zzz", "nope"], 1);

        assert!(no_match.is_empty());
        assert!(bad_ref.is_err());
    }

    #[test]
    fn the_runner_quotes_no_path() {
        let repo = Repo::new();
        repo.write("caf\u{e9}.txt", "one\n");
        repo.commit("first");

        let text = run(repo.root(), &["ls-tree", "--name-only", "HEAD"]).expect("ls-tree");

        assert_eq!(text, "caf\u{e9}.txt\n");
    }

    #[test]
    fn a_commit_is_an_ancestor_of_its_descendants_and_of_itself() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let first = repo.commit("first");
        repo.write("a.txt", "two\n");
        let second = repo.commit("second");

        assert_eq!(is_ancestor(repo.root(), &first, &second), Ok(true));
        assert_eq!(is_ancestor(repo.root(), &second, &second), Ok(true));
        assert_eq!(is_ancestor(repo.root(), &second, &first), Ok(false));
    }

    #[test]
    fn a_commit_of_another_branch_is_no_ancestor() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let fork = repo.commit("fork");
        repo.git(&["checkout", "-q", "-b", "side"]);
        repo.write("b.txt", "two\n");
        let side = repo.commit("side");
        repo.git(&["checkout", "-q", "main"]);
        repo.write("c.txt", "three\n");
        let main = repo.commit("main");

        assert_eq!(is_ancestor(repo.root(), &side, &main), Ok(false));
        assert_eq!(is_ancestor(repo.root(), &fork, &main), Ok(true));
    }

    #[test]
    fn an_object_that_git_does_not_have_is_the_line_of_git_and_never_an_option() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let first = repo.commit("first");

        for missing in ["f".repeat(40), "--all".to_string()] {
            let expected = run(
                repo.root(),
                &[
                    "merge-base",
                    "--is-ancestor",
                    "--end-of-options",
                    &first,
                    &missing,
                ],
            )
            .expect_err("git has no such object");

            let error = is_ancestor(repo.root(), &first, &missing).expect_err("no such object");

            assert_eq!(error, expected, "{missing}");
            assert!(error.starts_with("fatal: Not a valid "), "{error}");
            assert!(error.ends_with(&missing), "{error}");
        }
    }

    #[test]
    fn the_answer_of_a_predicate_that_cannot_start_is_the_text_of_the_operating_system() {
        let repo = Repo::new();
        let missing = repo.root().join("no-such-folder");
        let reason = Command::new("git")
            .current_dir(&missing)
            .output()
            .expect_err("the folder does not exist")
            .to_string();

        assert_eq!(is_ancestor(&missing, "a", "b"), Err(reason));
    }

    #[test]
    fn the_standard_input_of_a_run_is_the_input() {
        let repo = Repo::new();
        let bytes = b"{}\n\n\n  trailing   \n# not a comment\r\n";

        let name = run_with_input(repo.root(), &["hash-object", "-w", "--stdin"], bytes)
            .expect("hash-object");

        let stored = run_bytes(repo.root(), &["cat-file", "blob", name.trim()]).expect("cat-file");
        assert_eq!(stored, bytes, "git stores the bytes as they are");
    }

    #[test]
    fn a_run_with_input_that_git_refuses_gives_the_first_line_of_the_message_of_git() {
        let repo = Repo::new();

        let error = run_with_input(
            repo.root(),
            &["hash-object", "--no-such-option", "--stdin"],
            b"input",
        )
        .expect_err("git refuses the option");

        assert_eq!(error, "error: unknown option `no-such-option'");
    }

    #[test]
    fn a_run_with_input_that_cannot_start_gives_the_text_of_the_operating_system() {
        let repo = Repo::new();
        let missing = repo.root().join("no-such-folder");
        let reason = Command::new("git")
            .current_dir(&missing)
            .output()
            .expect_err("the folder does not exist")
            .to_string();

        assert_eq!(
            run_with_input(&missing, &["hash-object", "--stdin"], b"input"),
            Err(reason)
        );
    }

    #[test]
    fn a_run_with_input_that_answers_while_it_reads_finishes() {
        let repo = Repo::new();
        let name = run_with_input(repo.root(), &["hash-object", "-w", "--stdin"], b"blob\n")
            .expect("hash-object")
            .trim()
            .to_string();
        let lines = 20_000;
        let input = format!("{name}\n").repeat(lines);
        let expected = format!("{name} blob 5\nblob\n\n").repeat(lines);
        let root = repo.root().to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _sent = sender.send(run_with_input(
                &root,
                &["cat-file", "--batch"],
                input.as_bytes(),
            ));
        });

        let answer = receiver
            .recv_timeout(std::time::Duration::from_secs(20))
            .expect("git answered every line while the input was still written");

        assert_eq!(answer, Ok(expected));
    }

    #[test]
    fn the_output_of_a_run_with_input_is_the_bytes_that_git_wrote() {
        let repo = Repo::new();
        let bytes = [0xff, 0xfe, b'\n', 0x80];
        let name = run_with_input(repo.root(), &["hash-object", "-w", "--stdin"], &bytes)
            .expect("hash-object");
        let name = name.trim();

        let answer = run_bytes_with_input(
            repo.root(),
            &["cat-file", "--batch"],
            format!("{name}\n").as_bytes(),
        )
        .expect("cat-file");

        let mut expected = format!("{name} blob 4\n").into_bytes();
        expected.extend_from_slice(&bytes);
        expected.push(b'\n');
        assert_eq!(answer, expected, "no byte is decoded or replaced");
    }

    #[test]
    fn a_run_with_input_that_git_never_reads_is_no_failure_of_the_write() {
        let repo = Repo::new();
        let input = vec![b'x'; 1 << 20];

        let version = run_with_input(repo.root(), &["--version"], &input).expect("git --version");

        assert!(version.starts_with("git version "), "{version}");
    }
}
