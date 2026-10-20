//! Integration test for `template/.githooks/commit-msg`. The hook's own
//! doc comment states its contract: `houserules check-commit` runs only
//! after a quiet `houserules check-commit --help` probe succeeds, so an
//! older or absent binary never blocks a commit. The hook then skips the
//! check and says so on stderr in one line that names the condition it
//! tested: no `houserules` on `PATH`, or a `houserules` whose
//! `check-commit --help` probe failed. The hook carries no
//! attribution-trailer gate of its own (T2 review ruling 5.81: attribution
//! is fully project-ruled) -- a project that wants one files its own
//! `commits`-type knowledge check and `check-commit` picks it up the same
//! way it picks up `process.conventional-commits`.
//! Every case here spawns the real POSIX-shell hook against a scratch
//! temp directory, on a hermetic `PATH` (any directory already carrying a
//! `houserules` executable removed) with, where the case needs one, a
//! fake `houserules` script placed on that `PATH`.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The shipped commit-msg hook under test -- built directly from
/// `CARGO_MANIFEST_DIR`, deliberately never through `Path::canonicalize`
/// (the `repo_root()` every sibling file in this crate keeps its own
/// copy of): this path becomes an `sh` command-line argument
/// (`run_hook`, below), and `canonicalize`'s own current docs state that
/// on Windows it "converts the path to use extended length path syntax
/// ... [which] may be incompatible with other applications ... passed to
/// the application on the command-line" -- Git-for-Windows' `sh` cannot
/// open a `\\?\`-prefixed path, failing every case here with exit 127 and
/// stderr opening `/usr/bin/bash:`. The leading `..` components stay
/// unresolved in the returned path; both Windows' and POSIX's own file
/// APIs resolve `..` while opening a file, so `sh` (and every other
/// non-Rust reader) still finds it.
fn hook_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/.githooks/commit-msg")
}

/// `true` when `dir` holds a file named `houserules` (`houserules.exe` on
/// Windows too). Deliberately coarser than a full executable-bit check: a
/// portable executable-bit check has no std-only form, and adding a crate for
/// this test-only concern is out of scope. Over-removing a `PATH` entry that
/// happens to hold a non-executable file named `houserules` costs nothing here
/// -- the goal is only a `PATH` no real `houserules` binary can leak through.
fn dir_has_houserules(dir: &Path) -> bool {
    dir.join("houserules").is_file() || (cfg!(windows) && dir.join("houserules.exe").is_file())
}

/// This process's own `PATH`, with every directory holding a `houserules`
/// executable removed -- the hermetic base every `run_hook` case builds
/// its own `PATH` from. `grep`/`head`/`sh` still resolve normally: only
/// directories that themselves carry a `houserules` entry are dropped.
fn path_without_houserules() -> OsString {
    let path = env::var_os("PATH").unwrap_or_default();
    let kept: Vec<PathBuf> = env::split_paths(&path)
        .filter(|dir| !dir_has_houserules(dir))
        .collect();
    env::join_paths(kept).expect("join PATH entries")
}

/// Copies the shipped hook to `<top>/<install>/.githooks/commit-msg`, the
/// place `init` puts it. With `marker`, the install root also holds
/// `.houserules.json`, which marks it as an install root. Returns the
/// install root.
fn install_hook(top: &Path, install: &str, marker: bool) -> PathBuf {
    let root = top.join(install);
    let hook_dir = root.join(".githooks");
    fs::create_dir_all(&hook_dir).expect("create the hook directory");
    fs::copy(hook_path(), hook_dir.join("commit-msg")).expect("copy the hook");
    if marker {
        fs::write(root.join(".houserules.json"), "{}\n").expect("write the marker");
    }
    root
}

/// The child `PATH` of every hook run: `extra_path_dir` (when given)
/// prepended to `path_without_houserules`'s hermetic base.
fn hook_path_env(extra_path_dir: Option<&Path>) -> OsString {
    let base = path_without_houserules();
    match extra_path_dir {
        Some(extra) => {
            let mut dirs = vec![extra.to_path_buf()];
            dirs.extend(env::split_paths(&base));
            env::join_paths(dirs).expect("join PATH entries")
        }
        None => base,
    }
}

/// Runs `command`, a hook run, and returns its exit status code and
/// stderr.
fn hook_outcome(mut command: Command) -> (Option<i32>, String) {
    let output = command.output().expect("run commit-msg hook");
    (
        output.status.code(),
        String::from_utf8(output.stderr).expect("utf8 stderr"),
    )
}

/// Runs a copy of the shipped commit-msg hook against `message` the way
/// git runs the hook of an install at the git top level: from the top
/// level, by its path relative to it. The scratch directory
/// (`tempfile::tempdir`, removed on drop --
/// `houserules.tests-clean-scratch-dirs`'s sanctioned Rust form) is the
/// install root. `extra_path_dir` (when given) is prepended to the hermetic
/// `PATH`. Returns the exit status code and stderr regardless of whether
/// the hook succeeds.
fn run_hook(message: &str, extra_path_dir: Option<&Path>) -> (Option<i32>, String) {
    let top = tempfile::tempdir().expect("tempdir");
    install_hook(top.path(), "", true);
    fs::write(top.path().join("MSG"), message).expect("write MSG");

    let mut command = Command::new("sh");
    command
        .arg(".githooks/commit-msg")
        .arg("MSG")
        .current_dir(top.path())
        .env("PATH", hook_path_env(extra_path_dir));
    hook_outcome(command)
}

/// Sets `path`'s Unix execute bits; a no-op on Windows, where `sh`
/// (Git-for-Windows/MSYS -- the same shell every test here already
/// depends on to run the shebang hook itself) resolves a shebang script
/// on `PATH` without a Unix execute-bit concept of its own.
fn make_executable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// Writes an executable fake `houserules` script into a fresh scratch
/// directory: a `check-commit --help` probe call exits 0 (the fake
/// understands the subcommand), and a real `check-commit <file>` call
/// exits `code`, writing `output` to stderr first when given. Returns the
/// directory, suitable as `run_hook`'s `extra_path_dir`.
fn fake_houserules(code: i32, output: Option<&str>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("houserules");
    let echo = output
        .map(|text| format!("echo '{text}' >&2\n"))
        .unwrap_or_default();
    fs::write(
        &script,
        format!(
            "#!/usr/bin/env sh\n\
             if [ \"$1\" = \"check-commit\" ] && [ \"$2\" = \"--help\" ]; then\n  exit 0\nfi\n\
             {echo}exit {code}\n"
        ),
    )
    .expect("write fake houserules");
    make_executable(&script);
    dir
}

/// Writes an executable fake `houserules` script that simulates an older
/// binary with no `check-commit` subcommand: a `check-commit --help`
/// probe call exits 2 (clap's own "unrecognized subcommand" exit); a real
/// `check-commit <file>` call -- one the hook must never make once the
/// probe has already failed -- prints a distinctive line and exits 1, so
/// a hook that calls it anyway fails the test loudly instead of
/// coincidentally passing. Returns the directory, suitable as
/// `run_hook`'s `extra_path_dir`.
fn fake_houserules_rejecting_check_commit() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("houserules");
    fs::write(
        &script,
        "#!/usr/bin/env sh\n\
         if [ \"$1\" = \"check-commit\" ] && [ \"$2\" = \"--help\" ]; then\n  exit 2\nfi\n\
         echo \"should not have been invoked for real\" >&2\nexit 1\n",
    )
    .expect("write fake houserules");
    make_executable(&script);
    dir
}

/// The one stderr line the hook prints when no `houserules` is on `PATH`.
const SKIPPED_NO_BINARY: &str = "commit-msg: houserules not on PATH; check-commit skipped\n";

/// The one stderr line the hook prints when a `houserules` is on `PATH` and its
/// `check-commit --help` probe fails.
const SKIPPED_PROBE_FAILED: &str =
    "commit-msg: houserules check-commit --help failed; check-commit skipped\n";

#[test]
fn accepts_a_trailer_free_message_and_says_it_skipped_when_houserules_is_absent_from_path() {
    let (code, stderr) = run_hook("feat: a clean subject\n", None);
    assert_eq!(code, Some(0));
    assert_eq!(stderr, SKIPPED_NO_BINARY);
}

#[test]
fn accepts_a_co_authored_by_trailer_and_says_it_skipped_when_houserules_is_absent_from_path() {
    let (code, stderr) = run_hook("feat: x\n\nCo-Authored-By: Someone <a@b.com>\n", None);
    assert_eq!(code, Some(0));
    assert_eq!(stderr, SKIPPED_NO_BINARY);
}

#[test]
fn accepts_a_claude_session_trailer_and_says_it_skipped_when_houserules_is_absent_from_path() {
    let (code, stderr) = run_hook("feat: x\n\nClaude-Session: https://example.test/s\n", None);
    assert_eq!(code, Some(0));
    assert_eq!(stderr, SKIPPED_NO_BINARY);
}

#[test]
fn execs_houserules_check_commit_when_the_binary_is_on_path_inheriting_its_exit_and_stderr() {
    let path_dir = fake_houserules(1, Some("process.conventional-commits: bad subject"));
    let (code, stderr) = run_hook("bad subject\n", Some(path_dir.path()));
    assert_eq!(code, Some(1));
    assert_eq!(stderr.trim(), "process.conventional-commits: bad subject");
}

/// Writes an executable fake `houserules` script that answers the
/// `check-commit --help` probe and, for a real call, prints what the hook
/// passed: the argument count, the first two arguments, whether the third
/// is an absolute directory that holds `.githooks/commit-msg`, that
/// directory's own name, and the fourth argument's file name. Every check
/// runs in the shell, so a Windows `sh` path form compares the same way.
fn argv_printing_houserules() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("houserules");
    fs::write(
        &script,
        "#!/usr/bin/env sh\n\
         if [ \"$1\" = \"check-commit\" ] && [ \"$2\" = \"--help\" ]; then\n  exit 0\nfi\n\
         echo \"argc=$# $1 $2\" >&2\n\
         case \"$3\" in /*) echo \"dir is absolute\" >&2 ;; esac\n\
         if [ -f \"$3/.githooks/commit-msg\" ]; then echo \"dir holds the hook\" >&2; fi\n\
         echo \"dir name=$(basename \"$3\")\" >&2\n\
         echo \"message=$(basename \"$4\")\" >&2\n\
         exit 0\n",
    )
    .expect("write fake houserules");
    make_executable(&script);
    dir
}

/// Runs a copy of the shipped hook the way git runs a hook of the install
/// at `install` (a path below the git top level): from the top level, by
/// the path relative to it. Returns the exit status code and stderr.
/// `cdpath`, when given, is the child's `CDPATH`.
fn run_installed_hook(
    install: &str,
    extra_path_dir: &Path,
    cdpath: Option<&Path>,
) -> (Option<i32>, String) {
    let top = tempfile::tempdir().expect("tempdir");
    install_hook(top.path(), install, true);
    fs::write(top.path().join("MSG"), "feat: x\n").expect("write MSG");

    let mut command = Command::new("sh");
    command
        .arg(format!("{install}/.githooks/commit-msg"))
        .arg("MSG")
        .current_dir(top.path())
        .env("PATH", hook_path_env(Some(extra_path_dir)));
    if let Some(cdpath) = cdpath {
        command.env("CDPATH", cdpath);
    }
    hook_outcome(command)
}

/// What `argv_printing_houserules` prints for a hook of the install named
/// `dir_name`: the hook passes `--dir` with the install root, which holds
/// the hook, then the message file.
fn expected_argv_report(dir_name: &str) -> String {
    format!(
        "argc=4 check-commit --dir\ndir is absolute\ndir holds the hook\n\
         dir name={dir_name}\nmessage=MSG\n"
    )
}

#[test]
fn passes_the_install_root_read_from_its_own_path_and_the_message_file_as_argv() {
    let fake = argv_printing_houserules();

    for install in ["sub", "sub dir/sä", "a/b/c"] {
        let (code, stderr) = run_installed_hook(install, fake.path(), None);
        let dir_name = Path::new(install)
            .file_name()
            .and_then(|name| name.to_str())
            .expect("install folder name");
        assert_eq!(code, Some(0), "{install}: {stderr}");
        assert_eq!(stderr, expected_argv_report(dir_name), "{install}");
    }
}

/// The install root is the parent of the directory that holds the hook,
/// whatever `CDPATH` holds: `cd` prints the directory it finds through
/// `CDPATH`, which would end up in the `--dir` argument.
#[test]
fn a_cdpath_does_not_change_the_install_root() {
    let fake = argv_printing_houserules();
    let decoy = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(decoy.path().join("sub").join(".githooks")).expect("create the decoy");

    let (code, stderr) = run_installed_hook("sub", fake.path(), Some(decoy.path()));

    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stderr, expected_argv_report("sub"));
}

/// The one stderr line the hook prints when the parent of its directory
/// holds no `.houserules.json`.
const SKIPPED_NO_MARKER: &str = "commit-msg: no .houserules.json in the parent of the hook \
     directory; check-commit skipped\n";

/// A hook copied into `.git/hooks` has no install root: the parent of its
/// directory is `.git`. It passes no `--dir`, skips the check, and says so
/// in one line, instead of making `check-commit` fail on a knowledge file
/// below `.git`.
#[test]
fn a_copied_hook_without_an_install_root_skips_and_says_so() {
    let fake = argv_printing_houserules();
    let top = tempfile::tempdir().expect("tempdir");
    let hooks = top.path().join(".git").join("hooks");
    fs::create_dir_all(&hooks).expect("create .git/hooks");
    fs::copy(hook_path(), hooks.join("commit-msg")).expect("copy the hook");
    fs::write(top.path().join("MSG"), "feat: x\n").expect("write MSG");

    let mut command = Command::new("sh");
    command
        .arg(".git/hooks/commit-msg")
        .arg("MSG")
        .current_dir(top.path())
        .env("PATH", hook_path_env(Some(fake.path())));
    let (code, stderr) = hook_outcome(command);

    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stderr, SKIPPED_NO_MARKER);
}

/// How a test links `.git/hooks/commit-msg` to the install's hook.
#[cfg(unix)]
#[derive(Clone, Copy, Debug)]
enum LinkStyle {
    /// One link whose target is a relative path.
    Relative,
    /// One link whose target is an absolute path.
    Absolute,
    /// A link to a link, the first target relative and the second absolute.
    Chain,
}

/// The `readlink` a symlinked-hook run sees.
#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Readlink {
    /// The host's own `readlink`.
    Host,
    /// None on `PATH`: the hook falls back to the `ls -ld` listing. `PATH`
    /// holds only the tools the hook and the fake `houserules` need.
    Absent,
    /// One that always fails.
    Failing,
}

/// What a symlinked-hook run changes about its environment.
#[cfg(unix)]
#[derive(Clone, Copy, Debug)]
struct HookEnv {
    /// The child's `QUOTING_STYLE`, which GNU `ls` honors even when its
    /// output is not a terminal.
    quoting_style: Option<&'static str>,
    readlink: Readlink,
    /// A fake `houserules` is on `PATH`.
    with_binary: bool,
}

#[cfg(unix)]
impl HookEnv {
    fn new() -> Self {
        Self {
            quoting_style: None,
            readlink: Readlink::Host,
            with_binary: true,
        }
    }
}

/// The first file named `tool` in the directories of
/// `path_without_houserules`.
#[cfg(unix)]
fn host_tool(tool: &str) -> PathBuf {
    env::split_paths(&path_without_houserules())
        .map(|dir| dir.join(tool))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| panic!("no {tool} on PATH"))
}

/// A directory that holds links to only the host tools a symlinked-hook
/// run needs: `sh`, `ls`, `dirname`, and `basename`. With it as the whole
/// `PATH` (after the fake `houserules`'s own directory), there is no
/// `readlink`.
#[cfg(unix)]
fn tools_without_readlink() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for tool in ["sh", "ls", "dirname", "basename"] {
        std::os::unix::fs::symlink(host_tool(tool), dir.path().join(tool)).expect("link the tool");
    }
    dir
}

/// A directory that holds a `readlink` that always fails.
#[cfg(unix)]
fn failing_readlink() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let script = dir.path().join("readlink");
    fs::write(&script, "#!/usr/bin/env sh\nexit 1\n").expect("write the fake readlink");
    make_executable(&script);
    dir
}

/// Runs the install's hook through a symlink in `.git/hooks`, the way git
/// runs a hook that a project activates by linking it: from the top
/// level, with `$0` the link's own path. Returns the exit status code and
/// stderr, and the install root's folder name.
#[cfg(unix)]
fn run_symlinked_hook(
    install: &str,
    style: LinkStyle,
    hook_env: HookEnv,
) -> (Option<i32>, String, String) {
    use std::os::unix::fs::symlink;

    let fake = argv_printing_houserules();
    let top = tempfile::tempdir().expect("tempdir");
    let root = install_hook(top.path(), install, true);
    let hook = root.join(".githooks").join("commit-msg");
    let hooks = top.path().join(".git").join("hooks");
    fs::create_dir_all(&hooks).expect("create .git/hooks");
    let link = hooks.join("commit-msg");
    match style {
        LinkStyle::Relative => {
            let target = Path::new("../..")
                .join(install)
                .join(".githooks/commit-msg");
            symlink(target, &link).expect("link the hook");
        }
        LinkStyle::Absolute => symlink(&hook, &link).expect("link the hook"),
        LinkStyle::Chain => {
            symlink(&hook, top.path().join(".git").join("hop")).expect("link the hop");
            symlink("../hop", &link).expect("link the hook");
        }
    }
    fs::write(top.path().join("MSG"), "feat: x\n").expect("write MSG");

    let tools = tools_without_readlink();
    let failing = failing_readlink();
    let mut dirs: Vec<PathBuf> = Vec::new();
    if hook_env.with_binary {
        dirs.push(fake.path().to_path_buf());
    }
    match hook_env.readlink {
        Readlink::Host => dirs.extend(env::split_paths(&path_without_houserules())),
        Readlink::Absent => dirs.push(tools.path().to_path_buf()),
        Readlink::Failing => {
            dirs.push(failing.path().to_path_buf());
            dirs.extend(env::split_paths(&path_without_houserules()));
        }
    }
    let mut command = Command::new("sh");
    command
        .arg(".git/hooks/commit-msg")
        .arg("MSG")
        .current_dir(top.path())
        .env("PATH", env::join_paths(dirs).expect("join PATH entries"));
    if let Some(quoting_style) = hook_env.quoting_style {
        command.env("QUOTING_STYLE", quoting_style);
    }
    let (code, stderr) = hook_outcome(command);
    let folder = root
        .file_name()
        .and_then(|name| name.to_str())
        .expect("install folder name")
        .to_string();
    (code, stderr, folder)
}

/// A hook that git runs through a link in `.git/hooks` reads its install
/// root from the file the link names, not from the link's own directory:
/// the parent of `.git/hooks` is `.git`, which is no install root.
#[cfg(unix)]
#[test]
fn a_symlinked_hook_passes_the_install_root_of_its_target() {
    for install in ["", "sub dir/sä"] {
        for style in [LinkStyle::Relative, LinkStyle::Absolute, LinkStyle::Chain] {
            let (code, stderr, folder) = run_symlinked_hook(install, style, HookEnv::new());
            assert_eq!(code, Some(0), "{install:?} {style:?}: {stderr}");
            assert_eq!(
                stderr,
                expected_argv_report(&folder),
                "{install:?} {style:?}"
            );
        }
    }
}

/// `QUOTING_STYLE` is an environment variable of GNU `ls`: it quotes the
/// link target in the listing (`'../../.githooks/commit-msg'`) for every
/// path under `shell-always` and `c`, and for a path with a space under
/// `shell-escape`. The hook reads its link target whatever the variable
/// holds, with the host's `readlink` and, where the host has none, with the
/// `ls` listing at `QUOTING_STYLE=literal`.
#[cfg(unix)]
#[test]
fn a_symlinked_hook_reads_its_link_target_whatever_the_ls_quoting_style() {
    for readlink in [Readlink::Host, Readlink::Absent] {
        for install in ["", "sub dir/sä"] {
            for quoting_style in ["literal", "shell-escape", "shell-always", "c"] {
                for style in [LinkStyle::Relative, LinkStyle::Chain] {
                    let hook_env = HookEnv {
                        quoting_style: Some(quoting_style),
                        readlink,
                        with_binary: true,
                    };
                    let (code, stderr, folder) = run_symlinked_hook(install, style, hook_env);
                    let case = format!("{readlink:?} {install:?} {quoting_style} {style:?}");
                    assert_eq!(code, Some(0), "{case}: {stderr}");
                    assert_eq!(stderr, expected_argv_report(&folder), "{case}");
                }
            }
        }
    }
}

/// The one stderr line the hook prints when it cannot find its install
/// root.
#[cfg(unix)]
const SKIPPED_NO_ROOT: &str =
    "commit-msg: cannot resolve the install root of the hook; check-commit skipped\n";

/// A link target the hook cannot read is one named line and a skip, never
/// an unnamed `set -e` exit that makes git refuse the commit.
#[cfg(unix)]
#[test]
fn a_symlinked_hook_that_cannot_read_its_link_skips_and_says_so() {
    let hook_env = HookEnv {
        readlink: Readlink::Failing,
        ..HookEnv::new()
    };

    let (code, stderr, _folder) = run_symlinked_hook("", LinkStyle::Relative, hook_env);

    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stderr, SKIPPED_NO_ROOT);
}

/// The root is resolved only once the binary probe has passed: with no
/// `houserules` on `PATH` the hook says so, whatever its link or its
/// environment.
#[cfg(unix)]
#[test]
fn a_symlinked_hook_without_houserules_names_the_missing_binary_first() {
    let hook_env = HookEnv {
        readlink: Readlink::Failing,
        with_binary: false,
        ..HookEnv::new()
    };

    let (code, stderr, _folder) = run_symlinked_hook("", LinkStyle::Relative, hook_env);

    assert_eq!(code, Some(0), "{stderr}");
    assert_eq!(stderr, SKIPPED_NO_BINARY);
}

#[test]
fn skips_check_commit_and_says_so_when_houserules_is_absent_from_path() {
    let (code, stderr) = run_hook("bad subject\n", None);
    assert_eq!(code, Some(0));
    assert_eq!(stderr, SKIPPED_NO_BINARY);
}

#[test]
fn skips_check_commit_and_says_so_when_an_on_path_houserules_rejects_the_subcommand() {
    let path_dir = fake_houserules_rejecting_check_commit();
    let (code, stderr) = run_hook("bad subject\n", Some(path_dir.path()));
    assert_eq!(code, Some(0));
    assert_eq!(stderr, SKIPPED_PROBE_FAILED);
}
