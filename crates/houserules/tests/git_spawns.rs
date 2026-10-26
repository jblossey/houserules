//! No direct spawn of git in the source files of the part of HR-252 that this test covers.
//!
//! Every git call of a source file below `crates/houserules/src` goes through
//! `rules::git::command` or `review::git`. The runner sets `LC_ALL=C` and
//! `GIT_NO_REPLACE_OBJECTS=1`, so a replace ref in a clone cannot change what a call reads. A
//! direct `Command::new("git")` skips both settings.
//!
//! The part holds two kinds of file. Every finding names the kind that put the file in the part:
//!
//! - `Named`: a file in `NAMED`.
//! - `AddedSince`: a `.rs` file below `crates/houserules/src`, but not below `bin/`, that the index
//!   tracks and that `BASE` does not hold. Git lists them at run time, so a module is in the part
//!   when it is staged, and no list in this file needs a new entry.
//!
//! A named file that the index does not track is a fatal error. A file that cannot be read is a
//! fatal error too.
//!
//! The constraint has these exceptions, each with its reason:
//!
//! - `crates/houserules/src/bin/*` (the dev binaries) and `crates/houserules/tests/*` (the
//!   integration tests). The package has no library target, so neither can call
//!   `rules::git::command`. A `#[path]` include of `rules/git.rs` does not compile on its own,
//!   because that file depends on `rules::git_paths`.
//! - `crates/houserules/src/rules/git.rs`. It holds the spawn of the runner.
//! - Every other source file that `BASE` holds. HR-252 keeps those files and their spawns, and this
//!   test does not read them.
//!
//! The paths that git prints are pinned. The run uses `-z`, so `core.quotePath` quotes no path. It
//! sets `GIT_NO_REPLACE_OBJECTS=1`, so a replace ref cannot change the listing of `BASE`. It runs
//! through `with_default_git`, so the locale, the injected configuration, and the system and
//! global files change nothing, and it runs in the top of the work tree, so the paths are relative
//! to it.
//!
//! The run also takes every variable of the caller that starts with `GIT_` out of the environment
//! of git. The manual of git (`git help git`) names its repository, object, pathspec, and trace
//! variables with that prefix, and its other three variables (`HOME`, `PATH`, `EMAIL`) leave the
//! listing as it is. These variables change the listing:
//!
//! - `GIT_GLOB_PATHSPECS` and `GIT_ICASE_PATHSPECS` make `git ls-tree` fail.
//! - `GIT_DIR`, `GIT_COMMON_DIR`, `GIT_INDEX_FILE`, and `GIT_OBJECT_DIRECTORY` point git at
//!   another repository, index, or set of objects.
//! - `GIT_TRACE`, `GIT_TRACE2`, `GIT_TRACE2_EVENT`, `GIT_TRACE2_PERF`, `GIT_TRACE_PERFORMANCE`,
//!   `GIT_TRACE_REFS`, and `GIT_TRACE_SETUP` write into the listing when their value is
//!   `/dev/stdout`.
//!
//! A test below runs the derivation again in a child process for each of them, and another shows
//! that the run takes out every variable with the prefix. The repository itself is the input left:
//! its index, its objects, and its own configuration. A file is in the part after `git add`.
//!
//! The match is one pattern and no Rust parser. It sees `Command::new("git")` with any white
//! space between its tokens and any path before `Command`. It does not see a program name that a
//! variable, a constant, or a raw string holds. It counts a comment or a string that holds the
//! text. A test below pins each of these limits.

mod tracker_support;

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::LazyLock;

use regress::Regex;
use tracker_support::{
    commit_all, git, printed, repo_root, rerun_tests, scratch_git_repo, text_below,
    with_default_git,
};

/// The commit that the part is measured from: a source file that the index tracks and that this
/// commit does not hold is in the part.
const BASE: &str = "b9dd0d0ee4ca5af67d60eb5d4fafbeedc4506ac5";

/// The directory of the source files, relative to the top of the work tree.
const SOURCE: &str = "crates/houserules/src";

/// The directory of the dev binaries, an exception of the constraint.
const DEV_BINARIES: &str = "crates/houserules/src/bin/";

/// The files that HR-252 names for the part, relative to the top of the work tree.
const NAMED: [&str; 5] = [
    "crates/houserules/src/install/mod.rs",
    "crates/houserules/src/rules/render/location.rs",
    "crates/houserules/src/rules/check.rs",
    "crates/houserules/src/rules/audit.rs",
    "crates/houserules/src/rules/trailers.rs",
];

/// The kind of file that put a path in the part.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Why {
    /// The path is in `NAMED`.
    Named,
    /// The index tracks the path and the base commit does not hold it.
    AddedSince,
}

/// A source file of the part and the kind of file that put it there.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Member {
    path: String,
    why: Why,
}

/// Whether `name` is a variable of git's environment. Git names them with the prefix `GIT_`
/// (`git help git`, ENVIRONMENT VARIABLES), and Windows names variables without regard to case.
fn is_git_variable(name: &OsStr) -> bool {
    name.to_str()
        .and_then(|name| name.get(..4))
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("GIT_"))
}

/// The command `git <args>` in `top`, with the settings that the module doc names. It takes every
/// variable of git's environment that the caller holds out of the environment of git.
fn git_command(top: &Path, args: &[&str]) -> Command {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if is_git_variable(&name) {
            command.env_remove(name);
        }
    }
    with_default_git(&mut command)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .args(args)
        .current_dir(top);
    command
}

/// Runs `git <args>` in `top`, with the settings that the module doc names.
fn run_git(top: &Path, args: &[&str]) -> Output {
    git_command(top, args).output().expect("run git")
}

/// The paths that `git <args>` prints in `top`, split at the NUL bytes of `-z`.
fn listed(top: &Path, args: &[&str]) -> Vec<String> {
    let output = run_git(top, args);
    assert!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        top.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git prints UTF-8 paths")
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether `path` is a Rust source file that the constraint covers: it is a `.rs` file, and it is
/// no dev binary.
fn is_covered_source(path: &str) -> bool {
    path.ends_with(".rs") && !path.starts_with(DEV_BINARIES)
}

/// The files of the part in the work tree at `top`, in path order: the files of `named` and every
/// covered source file below [`SOURCE`] that the index tracks and `base` does not hold.
///
/// Panics when `base` is no commit of this clone (a shallow clone lacks it) or when a named file is
/// no tracked path: a silent skip would leave the part smaller than it is.
fn files_of_the_part(top: &Path, base: &str, named: &[&str]) -> Vec<Member> {
    let known = run_git(top, &["cat-file", "-e", &format!("{base}^{{commit}}")]);
    assert!(
        known.status.success(),
        "the base commit {base} is not in this clone; fetch the full history of the repository"
    );
    let tracked: BTreeSet<String> = listed(top, &["ls-files", "-z", "--", SOURCE])
        .into_iter()
        .collect();
    let held: BTreeSet<String> = listed(
        top,
        &["ls-tree", "-r", "-z", "--name-only", base, "--", SOURCE],
    )
    .into_iter()
    .collect();
    let mut members: Vec<Member> = named
        .iter()
        .map(|path| {
            assert!(
                tracked.contains(*path),
                "the named file {path} is no tracked path; correct NAMED"
            );
            Member {
                path: path.to_string(),
                why: Why::Named,
            }
        })
        .collect();
    for path in tracked.difference(&held) {
        if is_covered_source(path) && !named.contains(&path.as_str()) {
            members.push(Member {
                path: path.clone(),
                why: Why::AddedSince,
            });
        }
    }
    members.sort_by(|left, right| left.path.cmp(&right.path));
    members
}

/// The pattern of a direct spawn of git: `Command::new("git")` with any white space between its
/// tokens, and an optional trailing comma that a formatter leaves after the argument.
static SPAWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"Command\s*::\s*new\s*\(\s*"git"\s*,?\s*\)"#).expect("the pattern compiles")
});

/// The number of direct spawns of git in `text`.
fn spawns_of_git(text: &str) -> usize {
    SPAWN.find_iter(text).count()
}

/// One line for each file of `members` that holds a direct spawn of git: the path, the number of
/// spawns, and the kind of file that put it in the part. A file that cannot be read stops the test
/// with the path and the error.
fn findings(top: &Path, members: &[Member]) -> Vec<String> {
    members
        .iter()
        .filter_map(|member| {
            let count = spawns_of_git(&text_below(top.to_path_buf(), &member.path));
            (count > 0).then(|| {
                format!(
                    "{}: {count} direct Command::new(\"git\") ({:?})",
                    member.path, member.why
                )
            })
        })
        .collect()
}

/// A repository in a temporary directory whose first commit holds the files `a.rs`, `old.rs`,
/// `rules/git.rs`, and `bin/tool.rs` below [`SOURCE`]. Returns the repository and the sha of that
/// commit. The first path of the listing of that commit is a covered file (`a.rs` sorts before
/// `bin/`), so a line that git prints before the listing changes the part.
fn repository_with_a_base() -> (tempfile::TempDir, String) {
    let repo = scratch_git_repo();
    for path in ["a.rs", "old.rs", "rules/git.rs", "bin/tool.rs"] {
        write(repo.path(), &format!("{SOURCE}/{path}"), "fn old() {}\n");
    }
    let base = commit_all(repo.path(), "chore: base");
    (repo, base)
}

/// Writes `text` to `relative` (`/`-separated) below `top` and creates the folders above it.
fn write(top: &Path, relative: &str, text: &str) {
    let path = relative
        .split('/')
        .fold(top.to_path_buf(), |path, part| path.join(part));
    fs::create_dir_all(path.parent().expect("a parent")).expect("create the folder");
    fs::write(path, text).expect("write the file");
}

/// No file of the part spawns git directly. The part comes from this repository's index at run
/// time.
#[test]
fn no_file_of_the_part_spawns_git_directly() {
    let top = repo_root();
    let members = files_of_the_part(&top, BASE, &NAMED);

    let found = findings(&top, &members);

    assert!(
        found.is_empty(),
        "route each git call through rules::git::command or review::git:\n{}",
        found.join("\n")
    );
}

/// The files that this test derives are the ones that `git diff` lists as added since the base
/// commit, less the covered exceptions: an independent derivation of the same set.
#[test]
fn the_added_files_are_the_ones_that_git_diff_lists() {
    let top = repo_root();
    let diffed: BTreeSet<String> = listed(
        &top,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--diff-filter=A",
            "--no-renames",
            "-z",
            BASE,
            "--",
            SOURCE,
        ],
    )
    .into_iter()
    .filter(|path| is_covered_source(path))
    .collect();

    let added: BTreeSet<String> = files_of_the_part(&top, BASE, &NAMED)
        .into_iter()
        .filter(|member| member.why == Why::AddedSince)
        .map(|member| member.path)
        .collect();

    assert_eq!(added, diffed);
}

/// The named files are in the part, the runner and the dev binaries are not, and every member of
/// the part is a covered source file.
#[test]
fn the_part_holds_the_named_files_and_no_exception() {
    let members = files_of_the_part(&repo_root(), BASE, &NAMED);

    let paths: BTreeSet<&str> = members.iter().map(|member| member.path.as_str()).collect();
    for named in NAMED {
        assert!(paths.contains(named), "{named} is in the part");
    }
    assert!(!paths.contains("crates/houserules/src/rules/git.rs"));
    assert!(
        paths.iter().all(|path| is_covered_source(path)),
        "{paths:?}"
    );
}

/// The part is the named files and the covered files that the index tracks and the base commit does
/// not hold. A runner, a dev binary, a file that is no Rust source, and a file of the base commit
/// are no members.
#[test]
fn the_derivation_classifies_every_tracked_path_of_a_scratch_repository() {
    let (repo, base) = repository_with_a_base();
    write(
        repo.path(),
        &format!("{SOURCE}/review/new.rs"),
        "fn new() {}\n",
    );
    write(
        repo.path(),
        &format!("{SOURCE}/bin/new_tool.rs"),
        "fn tool() {}\n",
    );
    write(repo.path(), &format!("{SOURCE}/notes.txt"), "notes\n");
    write(
        repo.path(),
        &format!("{SOURCE}/rules/git.rs"),
        "fn changed() {}\n",
    );
    commit_all(repo.path(), "feat: add modules");
    let named = [format!("{SOURCE}/old.rs")];
    let named: Vec<&str> = named.iter().map(String::as_str).collect();

    let members = files_of_the_part(repo.path(), &base, &named);

    assert_eq!(
        members,
        [
            Member {
                path: format!("{SOURCE}/old.rs"),
                why: Why::Named
            },
            Member {
                path: format!("{SOURCE}/review/new.rs"),
                why: Why::AddedSince
            },
        ]
    );
}

/// A path with a character outside ASCII is listed as the name it has: git quotes such a path
/// (`core.quotePath`) unless the listing is NUL-separated.
#[test]
fn a_path_outside_ascii_is_listed_unquoted() {
    let (repo, base) = repository_with_a_base();
    write(repo.path(), &format!("{SOURCE}/sä.rs"), "fn umlaut() {}\n");
    commit_all(repo.path(), "feat: add a module");

    let members = files_of_the_part(repo.path(), &base, &[]);

    assert_eq!(
        members,
        [Member {
            path: format!("{SOURCE}/sä.rs"),
            why: Why::AddedSince
        }]
    );
}

/// A replace ref that makes the base commit look like a later commit changes nothing: the listing
/// of the base reads the objects that the commit holds. This is the input that the runner's
/// `GIT_NO_REPLACE_OBJECTS=1` closes.
#[test]
fn a_replace_ref_does_not_change_the_listing_of_the_base_commit() {
    let (repo, base) = repository_with_a_base();
    write(
        repo.path(),
        &format!("{SOURCE}/review/new.rs"),
        "fn new() {}\n",
    );
    let later = commit_all(repo.path(), "feat: add a module");
    git(repo.path(), &["replace", &base, &later]);

    let members = files_of_the_part(repo.path(), &base, &[]);

    assert_eq!(
        members,
        [Member {
            path: format!("{SOURCE}/review/new.rs"),
            why: Why::AddedSince
        }]
    );
}

/// The variable that names the repository that the probe of the listing reads.
const PROBE_TOP: &str = "HR_SPAWNS_PROBE_TOP";

/// The variable that names the commit that the probe of the listing starts from.
const PROBE_BASE: &str = "HR_SPAWNS_PROBE_BASE";

/// The variable that switches the probe of the variables on.
const PROBE_VARIABLES: &str = "HR_SPAWNS_PROBE_VARIABLES";

/// The text that a probe prints before each result: a line break first, so that the result starts
/// its own line when the test harness does not capture the output.
const RESULT: &str = "\nRESULT ";

/// Prints the part that `files_of_the_part` finds below the repository that `PROBE_TOP` names, from
/// the commit that `PROBE_BASE` names, one result for each member. It does nothing when `PROBE_TOP`
/// is absent. A test cannot change its own environment, so the tests of the variables below run
/// this one in a child process.
#[test]
fn probe_prints_the_part_of_the_repository_of_its_environment() {
    let (Ok(top), Ok(base)) = (std::env::var(PROBE_TOP), std::env::var(PROBE_BASE)) else {
        return;
    };
    for member in files_of_the_part(Path::new(&top), &base, &[]) {
        println!("{RESULT}{} {:?}", member.path, member.why);
    }
}

/// Prints, for each variable of git's environment that this process holds, `removed` when the
/// command of the part takes it out of the environment of git, `set` when the command gives it
/// its own value, and `kept` otherwise. It does nothing when `PROBE_VARIABLES` is absent.
#[test]
fn probe_prints_what_the_command_does_with_the_git_variables_of_its_environment() {
    if std::env::var_os(PROBE_VARIABLES).is_none() {
        return;
    }
    let command = git_command(Path::new("."), &["status"]);
    let handled: Vec<(OsString, Option<OsString>)> = command
        .get_envs()
        .map(|(name, value)| (name.to_owned(), value.map(OsStr::to_owned)))
        .collect();
    for (name, _) in std::env::vars_os().filter(|(name, _)| is_git_variable(name)) {
        let state = match handled.iter().find(|(handled, _)| *handled == name) {
            Some((_, None)) => "removed",
            Some((_, Some(_))) => "set",
            None => "kept",
        };
        println!("{RESULT}{state} {}", name.to_string_lossy());
    }
}

/// The results that `text` holds, in order, without the lines of the test harness.
fn results(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.strip_prefix(RESULT.trim_start()))
        .collect()
}

/// The variables of git that write a trace to the path that their value names. The value
/// `/dev/stdout` puts the trace into the standard output of git, which is what the part reads. A
/// platform with no such device skips these cases.
const TRACE_VARIABLES: [&str; 7] = [
    "GIT_TRACE",
    "GIT_TRACE2",
    "GIT_TRACE2_EVENT",
    "GIT_TRACE2_PERF",
    "GIT_TRACE_PERFORMANCE",
    "GIT_TRACE_REFS",
    "GIT_TRACE_SETUP",
];

/// No variable of the caller's environment changes what the part lists or stops the listing: each
/// variable below, set to a value that changes what git lists in the scratch repository, leaves
/// the part as it is. The part is the one file added since the base commit. A first run with no
/// variable shows that the probe reads that part.
#[test]
fn no_variable_of_the_caller_changes_the_listing() {
    let (repo, base) = repository_with_a_base();
    write(
        repo.path(),
        &format!("{SOURCE}/review/new.rs"),
        "fn new() {}\n",
    );
    commit_all(repo.path(), "feat: add a module");
    let other_repository = scratch_git_repo();
    let no_objects = tempfile::tempdir().expect("tempdir");
    let indexes = tempfile::tempdir().expect("tempdir");
    let old_index = indexes.path().join("index");
    let read_tree = git_command(repo.path(), &["read-tree", &base])
        .env("GIT_INDEX_FILE", &old_index)
        .output()
        .expect("run git read-tree");
    assert!(read_tree.status.success(), "{}", printed(&read_tree));

    let other_git_dir = other_repository.path().join(".git").into_os_string();
    let trace: &[&str] = if cfg!(unix) { &TRACE_VARIABLES } else { &[] };
    let hostile: Vec<(&str, OsString)> = [
        ("GIT_GLOB_PATHSPECS", OsString::from("1")),
        ("GIT_ICASE_PATHSPECS", OsString::from("1")),
        ("GIT_INDEX_FILE", old_index.into_os_string()),
        ("GIT_DIR", other_git_dir.clone()),
        ("GIT_COMMON_DIR", other_git_dir),
        ("GIT_OBJECT_DIRECTORY", no_objects.path().into()),
    ]
    .into_iter()
    .chain(
        trace
            .iter()
            .map(|name| (*name, OsString::from("/dev/stdout"))),
    )
    .collect();
    let wanted = [format!("{SOURCE}/review/new.rs AddedSince")];

    let mut failures = Vec::new();
    for (name, value) in std::iter::once(("no variable", None))
        .chain(hostile.iter().map(|(name, value)| (*name, Some(value))))
    {
        let output = rerun_tests(
            &["probe_prints_the_part_of_the_repository_of_its_environment"],
            |command| {
                command.env(PROBE_TOP, repo.path()).env(PROBE_BASE, &base);
                if let Some(value) = value {
                    command.env(name, value);
                }
            },
        );
        let text = printed(&output);
        let found = results(&text);
        if !output.status.success() || found != wanted {
            failures.push(format!(
                "{name}: exit {:?}, part {found:?}\n{text}",
                output.status.code()
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A sample of the variables that the manual of git names, from each family, and a name that no
/// git defines yet: the rule is the prefix, so a variable of a later git needs no entry.
const SAMPLE: [&str; 15] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_LITERAL_PATHSPECS",
    "GIT_GLOB_PATHSPECS",
    "GIT_NOGLOB_PATHSPECS",
    "GIT_ICASE_PATHSPECS",
    "GIT_TRACE",
    "GIT_TRACE2_EVENT",
    "GIT_CONFIG_NOSYSTEM",
    "GIT_A_VARIABLE_OF_A_LATER_GIT",
];

/// The command of the part takes every variable of git's environment out of the environment of
/// git, not only the ones that the test above names.
#[test]
fn the_command_removes_every_git_variable_of_the_caller() {
    let output = rerun_tests(
        &["probe_prints_what_the_command_does_with_the_git_variables_of_its_environment"],
        |command| {
            command.env(PROBE_VARIABLES, "1");
            for name in SAMPLE {
                command.env(name, "x");
            }
        },
    );

    let text = printed(&output);
    assert!(output.status.success(), "{text}");
    let results = results(&text);
    for name in SAMPLE {
        assert!(
            results.contains(&format!("removed {name}").as_str()),
            "{name} reaches git: {results:?}"
        );
    }
}

/// The prefix is `GIT_`, in any case, and nothing longer or shorter.
#[test]
fn a_git_variable_is_a_name_with_the_prefix_git_underscore() {
    for (name, expected) in [
        ("GIT_DIR", true),
        ("GIT_", true),
        ("git_dir", true),
        ("Git_Dir", true),
        ("GIT", false),
        ("GITHUB_TOKEN", false),
        ("XGIT_DIR", false),
        ("HOME", false),
        ("", false),
    ] {
        assert_eq!(is_git_variable(OsStr::new(name)), expected, "{name:?}");
    }
}

/// A named file that the index does not track stops the test: a rename of a named file must reach
/// `NAMED`, and a skip would shrink the part without a sign.
#[test]
#[should_panic(expected = "the named file crates/houserules/src/gone.rs is no tracked path")]
fn a_named_file_that_is_no_tracked_path_is_fatal() {
    let (repo, base) = repository_with_a_base();

    files_of_the_part(repo.path(), &base, &["crates/houserules/src/gone.rs"]);
}

/// A base commit that the clone lacks (a shallow clone) stops the test with a line that names it.
#[test]
#[should_panic(expected = "is not in this clone")]
fn a_base_commit_that_the_clone_lacks_is_fatal() {
    let (repo, _base) = repository_with_a_base();

    files_of_the_part(repo.path(), &"0".repeat(40), &[]);
}

/// A file of the part that cannot be read stops the test with its path.
#[test]
#[should_panic(expected = "read ")]
fn a_member_that_cannot_be_read_is_fatal() {
    let repo = scratch_git_repo();
    let missing = Member {
        path: format!("{SOURCE}/missing.rs"),
        why: Why::AddedSince,
    };

    findings(repo.path(), &[missing]);
}

/// A finding names the file, the number of spawns, and the kind of file that put it in the part;
/// a file with no spawn gives none.
#[test]
fn a_finding_names_the_file_the_count_and_the_kind() {
    let repo = scratch_git_repo();
    write(
        repo.path(),
        "named.rs",
        "fn a() { Command::new(\"git\"); }\nfn b() { Command::new(\"git\"); }\n",
    );
    write(
        repo.path(),
        "added.rs",
        "fn c() { Command::new(\"git\"); }\n",
    );
    write(
        repo.path(),
        "clean.rs",
        "fn d() { crate::rules::git::command(root); }\n",
    );
    let members = [
        Member {
            path: "named.rs".to_string(),
            why: Why::Named,
        },
        Member {
            path: "added.rs".to_string(),
            why: Why::AddedSince,
        },
        Member {
            path: "clean.rs".to_string(),
            why: Why::AddedSince,
        },
    ];

    assert_eq!(
        findings(repo.path(), &members),
        [
            "named.rs: 2 direct Command::new(\"git\") (Named)",
            "added.rs: 1 direct Command::new(\"git\") (AddedSince)",
        ]
    );
}

/// The spellings of a direct spawn that the pattern counts once each.
const SPAWNS: [&str; 6] = [
    r#"Command::new("git")"#,
    r#"std::process::Command::new("git").args(["add"])"#,
    r#"tokio::process::Command::new("git")"#,
    r#"Command :: new ( "git" )"#,
    "Command::new(\n    \"git\",\n)",
    "let mut command = Command::new(\"git\");",
];

/// The text that is no direct spawn of git, and that the pattern leaves alone: another program, a
/// word that holds `git`, and a call through the runner.
const NO_SPAWNS: [&str; 7] = [
    r#"Command::new("gitk")"#,
    r#"Command::new("git-lfs")"#,
    r#"Command::new("cargo").arg("git")"#,
    r#"Command::new("sh").args(["-c", "git status"])"#,
    "rules::git::command(root)",
    r#"let program = "git";"#,
    r#"Command::new("not git")"#,
];

/// The text that holds a program name that the pattern cannot see: this is a limit of the match.
const NOT_SEEN: [&str; 4] = [
    "Command::new(program)",
    "Command::new(GIT)",
    r#"Command::new(r"git")"#,
    r#"Command::new(["g", "it"].concat())"#,
];

/// The text that the pattern counts although it spawns nothing: a comment and a string. This is a
/// limit of the match; the author words the comment another way.
const COUNTED_THOUGH_NO_CODE: [&str; 2] = [
    r#"// a direct Command::new("git") is not allowed"#,
    r###"let text = r#"Command::new("git")"#;"###,
];

/// The pattern counts each spelling of a spawn, leaves the other text alone, and shows its limits.
#[test]
fn the_pattern_counts_each_spelling_and_shows_its_limits() {
    for text in SPAWNS {
        assert_eq!(spawns_of_git(text), 1, "{text}");
    }
    for text in NO_SPAWNS {
        assert_eq!(spawns_of_git(text), 0, "{text}");
    }
    for text in NOT_SEEN {
        assert_eq!(spawns_of_git(text), 0, "a limit of the match: {text}");
    }
    for text in COUNTED_THOUGH_NO_CODE {
        assert_eq!(spawns_of_git(text), 1, "a limit of the match: {text}");
    }
}
