//! `check-report-claims --doc <path>` and `--item <id>` (spec 9.1, HR-165 (3)):
//! a spec or a backlog item that cites a workspace path must cite one that
//! exists. A workspace path is a cited path that git ignores: the batch
//! workspace is not tracked, so the check reads the disk. Each case runs the
//! compiled binary against a scratch install.

mod tracker_support;

use std::fs;
use std::path::Path;
use std::process::Output;

use serde_json::json;
use tracker_support::{
    LINE_A, LINE_B, assert_output, git, houserules, install_with_backlog_dir,
    install_without_backlog, mini_install, read_json, run, scratch_git_repo, stderr, write_json,
};

/// The doc that the cases check, relative to the install root.
const DOC: &str = "docs/specs/x.md";

/// The workspace that `.gitignore` names in every case.
const WORKSPACE: &str = ".superpowers/sdd/batch";

/// Appends `line` to the `.gitignore` of `dir`.
fn ignore(dir: &Path, line: &str) {
    let path = dir.join(".gitignore");
    let mut text = fs::read_to_string(&path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(line);
    text.push('\n');
    fs::write(path, text).expect("write .gitignore");
}

/// Writes `relative` below `dir` with `text`, creating its directories.
fn write(dir: &Path, relative: &str, text: &str) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().expect("a parent directory")).expect("create parents");
    fs::write(path, text).expect("write the file");
}

/// An install that ignores the workspace, holds `probe.txt` in it, and holds
/// `DOC` with `text`.
fn install_with_doc(text: &str) -> tempfile::TempDir {
    let dir = install_without_backlog();
    ignore(dir.path(), ".superpowers/");
    write(dir.path(), &format!("{WORKSPACE}/probe.txt"), "probe\n");
    write(dir.path(), DOC, text);
    dir
}

/// The install root as `check-report-claims` resolves it: the git top level of
/// `dir`, in git's own spelling. An absolute citation built from it is the
/// spelling that the check compares against.
fn root_of(dir: &Path) -> String {
    git(dir, &["rev-parse", "--show-toplevel"])
        .trim()
        .to_string()
}

/// Runs `check-report-claims --doc <DOC>` with `dir` as the working directory.
fn check_doc(dir: &Path) -> Output {
    houserules()
        .args(["check-report-claims", "--doc", DOC])
        .current_dir(dir)
        .output()
        .expect("run check-report-claims")
}

/// Spec 9.1: a cited workspace path that does not exist is one line, exit 1.
#[test]
fn doc_names_a_cited_workspace_path_that_does_not_exist() {
    let dir = install_with_doc(&format!(
        "The probe `{WORKSPACE}/probe.txt` shows it. The run {WORKSPACE}/missing.txt is cited too.\n"
    ));
    assert_output(
        &check_doc(dir.path()),
        1,
        "",
        &format!("{DOC}: cites {WORKSPACE}/missing.txt, which does not exist\n"),
    );
}

/// A doc whose cited workspace paths all exist is clean: one line on stdout.
#[test]
fn doc_with_only_existing_workspace_paths_is_clean() {
    let dir = install_with_doc(&format!("See {WORKSPACE}/probe.txt:1 for the run.\n"));
    assert_output(
        &check_doc(dir.path()),
        0,
        &format!("{DOC}: no claim mismatches found\n"),
        "",
    );
}

/// A path that git does not ignore is no workspace path: a spec cites files
/// the batch has yet to write, so the check leaves them alone.
#[test]
fn doc_does_not_check_a_cited_path_outside_the_workspace() {
    let dir = install_with_doc(
        "The batch adds crates/houserules/src/tracker/new.rs and docs/plans/next.md.\n",
    );
    assert_output(
        &check_doc(dir.path()),
        0,
        &format!("{DOC}: no claim mismatches found\n"),
        "",
    );
}

/// Spec 9.1: a cited workspace path must exist, whatever its spelling. An
/// absolute path inside the install root that git ignores is a workspace path.
#[test]
fn doc_names_an_absolute_citation_of_a_missing_workspace_file_inside_the_root() {
    let dir = install_with_doc("");
    let root = root_of(dir.path());
    write(
        dir.path(),
        DOC,
        &format!("The run {root}/{WORKSPACE}/missing.txt is cited.\n"),
    );
    assert_output(
        &check_doc(dir.path()),
        1,
        "",
        &format!("{DOC}: cites {root}/{WORKSPACE}/missing.txt, which does not exist\n"),
    );
}

/// An absolute citation of a workspace file that exists is clean.
#[test]
fn doc_with_an_existing_absolute_workspace_citation_is_clean() {
    let dir = install_with_doc("");
    let root = root_of(dir.path());
    write(
        dir.path(),
        DOC,
        &format!("See {root}/{WORKSPACE}/probe.txt:1 for the run.\n"),
    );
    assert_output(
        &check_doc(dir.path()),
        0,
        &format!("{DOC}: no claim mismatches found\n"),
        "",
    );
}

/// An absolute citation outside the install root names no workspace path of
/// this install. The check skips it, and git (which refuses a path outside its
/// repository) never sees it. A sibling directory whose name starts with the
/// name of the root is outside the root too.
#[test]
fn doc_skips_an_absolute_citation_outside_the_root() {
    let dir = install_with_doc("");
    let root = root_of(dir.path());
    write(
        dir.path(),
        DOC,
        &format!("Outside: /outside/x.txt and {root}-sibling/{WORKSPACE}/y.txt are cited.\n"),
    );
    assert_output(
        &check_doc(dir.path()),
        0,
        &format!("{DOC}: no claim mismatches found\n"),
        "",
    );
}

/// An absolute citation of a path that git does not ignore is no workspace
/// path, as a relative one is not.
#[test]
fn doc_does_not_check_an_absolute_citation_of_a_path_git_does_not_ignore() {
    let dir = install_with_doc("");
    let root = root_of(dir.path());
    write(
        dir.path(),
        DOC,
        &format!("The batch adds {root}/crates/houserules/src/tracker/new.rs.\n"),
    );
    assert_output(
        &check_doc(dir.path()),
        0,
        &format!("{DOC}: no claim mismatches found\n"),
        "",
    );
}

/// The workspace is whatever `.gitignore` names, not one directory name.
#[test]
fn doc_reads_the_workspace_from_gitignore() {
    let dir = install_with_doc("The capture build-out/run.log is cited.\n");
    ignore(dir.path(), "build-out/");
    assert_output(
        &check_doc(dir.path()),
        1,
        "",
        &format!("{DOC}: cites build-out/run.log, which does not exist\n"),
    );
}

/// A glob and a placeholder name a set of paths, not one path.
#[test]
fn doc_skips_a_cited_glob_or_placeholder() {
    let dir = install_with_doc(&format!(
        "Every {WORKSPACE}/task-*-report.json and {WORKSPACE}/<task>/x.txt and \
         {WORKSPACE}/{{a,b}}.txt is read.\n"
    ));
    assert_output(
        &check_doc(dir.path()),
        0,
        &format!("{DOC}: no claim mismatches found\n"),
        "",
    );
}

/// A path cited twice is named once, in the order of its first citation.
#[test]
fn doc_names_each_missing_path_once() {
    let dir = install_with_doc(&format!(
        "{WORKSPACE}/b.txt and {WORKSPACE}/a.txt, then {WORKSPACE}/b.txt:3 again.\n"
    ));
    assert_output(
        &check_doc(dir.path()),
        1,
        "",
        &format!(
            "{DOC}: cites {WORKSPACE}/b.txt, which does not exist\n\
             {DOC}: cites {WORKSPACE}/a.txt, which does not exist\n"
        ),
    );
}

/// A doc that cannot be read is a named error with the operating system's
/// text, exit 2.
#[test]
fn a_doc_that_does_not_exist_is_a_named_error() {
    let dir = install_without_backlog();
    let output = check_doc(dir.path());
    assert_eq!(output.status.code(), Some(2));
    let error = stderr(&output);
    assert!(error.starts_with(&format!("{DOC}: ")), "{error}");
}

/// Spec 9.1: with no tracker, `--item` prints line A and exits 2.
#[test]
fn item_with_no_tracker_prints_line_a_and_exits_2() {
    let dir = install_without_backlog();
    let output = run(dir.path(), &["check-report-claims", "--item", "HR-1"]);
    assert_output(&output, 2, "", &format!("{LINE_A}\n"));
}

/// Spec 3.4: with no tracker and a `backlog/` directory, `--item` prints
/// line B and exits 2, as every command that reads the tracker does.
#[test]
fn item_with_no_tracker_and_a_backlog_directory_prints_line_b_and_exits_2() {
    let dir = install_with_backlog_dir();
    let output = run(dir.path(), &["check-report-claims", "--item", "HR-1"]);
    assert_output(&output, 2, "", &format!("{LINE_B}\n"));
}

/// Sets the body of the item `HR-901` of the mini install `dir` to
/// `paragraph`.
fn cite_in_item(dir: &Path, paragraph: &str) {
    let mut misc = read_json(dir, "backlog/items/misc.json");
    misc["items"][0]["body"] = json!([paragraph]);
    write_json(dir, "backlog/items/misc.json", &misc);
}

/// Writes the item `HR-901` of the mini install with `paragraph` in its body,
/// and makes the workspace an ignored path.
fn mini_with_item_citing(paragraph: &str) -> tempfile::TempDir {
    let dir = mini_install();
    ignore(dir.path(), ".superpowers/");
    write(dir.path(), &format!("{WORKSPACE}/probe.txt"), "probe\n");
    cite_in_item(dir.path(), paragraph);
    dir
}

/// Spec 9.1: `--item` reads the item through the tracker and names a cited
/// workspace path that does not exist, with the item's id in the line.
#[test]
fn item_names_a_cited_workspace_path_that_does_not_exist() {
    let dir = mini_with_item_citing(&format!(
        "Held at {WORKSPACE}/probe.txt and {WORKSPACE}/missing.json."
    ));
    let output = run(dir.path(), &["check-report-claims", "--item", "HR-901"]);
    assert_output(
        &output,
        1,
        "",
        &format!("HR-901: cites {WORKSPACE}/missing.json, which does not exist\n"),
    );
}

/// `--item` checks an absolute citation inside the install root as `--doc`
/// does.
#[test]
fn item_names_an_absolute_citation_of_a_missing_workspace_file_inside_the_root() {
    let dir = mini_with_item_citing("");
    let root = root_of(dir.path());
    cite_in_item(
        dir.path(),
        &format!("Held at {root}/{WORKSPACE}/missing.json."),
    );
    let output = run(dir.path(), &["check-report-claims", "--item", "HR-901"]);
    assert_output(
        &output,
        1,
        "",
        &format!("HR-901: cites {root}/{WORKSPACE}/missing.json, which does not exist\n"),
    );
}

/// An item whose cited workspace paths all exist is clean.
#[test]
fn item_with_only_existing_workspace_paths_is_clean() {
    let dir = mini_with_item_citing(&format!("Held at {WORKSPACE}/probe.txt."));
    let output = run(dir.path(), &["check-report-claims", "--item", "HR-901"]);
    assert_output(&output, 0, "HR-901: no claim mismatches found\n", "");
}

/// The tracker's own not-found line is the error for an id it does not hold,
/// exit 2.
#[test]
fn item_that_the_tracker_does_not_hold_is_the_trackers_not_found_line() {
    let dir = mini_install();
    let output = run(dir.path(), &["check-report-claims", "--item", "HR-999"]);
    assert_output(&output, 2, "", "unknown id \"HR-999\"\n");
}

/// A report path, `--doc`, and `--item` exclude each other, and one of them
/// is required: clap refuses the call with exit 2 and names the fault.
#[test]
fn the_source_of_the_claims_is_exactly_one_of_report_doc_and_item() {
    let dir = scratch_git_repo();
    let call = |args: &[&str]| {
        let output = houserules()
            .arg("check-report-claims")
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("run check-report-claims");
        (output.status.code(), stderr(&output))
    };
    let cases: [(&[&str], &str); 4] = [
        (&[], "the following required arguments were not provided"),
        (&["--doc", "a.md", "--item", "HR-1"], "cannot be used with"),
        (&["report.json", "--doc", "a.md"], "cannot be used with"),
        (&["report.json", "--item", "HR-1"], "cannot be used with"),
    ];
    for (args, fault) in cases {
        let (code, error) = call(args);
        assert_eq!(code, Some(2), "{args:?}: {error}");
        assert!(error.contains(fault), "{args:?}: {error}");
    }
}

/// An install root has more than one spelling: macOS reaches a temp directory
/// through the symlink `/var` while git prints `/private/var`, and Windows
/// reaches it by a short and a long name. A symlink to the install root is the
/// same cause on Linux. A citation inside the root is inside it in every
/// spelling (spec 9.1: a cited workspace path must exist, whatever its
/// spelling).
#[cfg(unix)]
mod symlinked_root {
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use super::tracker_support::stdout;
    use super::*;

    /// The source of the claims that a case checks.
    #[derive(Clone, Copy)]
    enum Mode {
        /// `check-report-claims --doc`.
        Doc,
        /// `check-report-claims --item`.
        Item,
    }

    impl Mode {
        /// The label that the mode puts in front of each line.
        fn label(self) -> &'static str {
            match self {
                Self::Doc => DOC,
                Self::Item => "HR-901",
            }
        }
    }

    /// Which spelling of the install root a citation uses.
    #[derive(Clone, Copy)]
    enum Spelling {
        /// The real path, as `git rev-parse --show-toplevel` prints it.
        Real,
        /// The path of a symlink to the install root, as `--dir` names it.
        Link,
    }

    /// A symlink to the install root, in a directory of its own. The returned
    /// directory holds the symlink: drop it and the symlink goes.
    fn link_to(install: &Path) -> (tempfile::TempDir, PathBuf) {
        let outer = tempfile::tempdir().expect("tempdir");
        let link = outer.path().join("link");
        symlink(install, &link).expect("symlink the install root");
        (outer, link)
    }

    /// `check-report-claims --doc <DOC> --dir <link>`, run in the symlink.
    fn check_doc_through(link: &Path) -> Output {
        houserules()
            .args(["check-report-claims", "--doc", DOC, "--dir"])
            .arg(link)
            .current_dir(link)
            .output()
            .expect("run check-report-claims")
    }

    /// Runs the check with `--dir` naming the symlink to an install whose doc,
    /// or item, cites `file` of the workspace by `spelling` of the root. Returns
    /// the citation as the text spells it and the output of the check.
    fn run_case(mode: Mode, spelling: Spelling, file: &str) -> (String, Output) {
        let install = match mode {
            Mode::Doc => install_with_doc(""),
            Mode::Item => mini_with_item_citing(""),
        };
        let (_outer, link) = link_to(install.path());
        let root = match spelling {
            Spelling::Real => root_of(install.path()),
            Spelling::Link => link.to_str().expect("a UTF-8 path").to_string(),
        };
        let cited = format!("{root}/{WORKSPACE}/{file}");
        let output = match mode {
            Mode::Doc => {
                write(install.path(), DOC, &format!("The run {cited} is cited.\n"));
                check_doc_through(&link)
            }
            Mode::Item => {
                cite_in_item(install.path(), &format!("Held at {cited}."));
                run(&link, &["check-report-claims", "--item", "HR-901"])
            }
        };
        (cited, output)
    }

    /// A citation of a missing workspace file is named, in the spelling the
    /// text gives.
    fn assert_missing_file_is_named(mode: Mode, spelling: Spelling) {
        let (cited, output) = run_case(mode, spelling, "missing.txt");
        assert_output(
            &output,
            1,
            "",
            &format!("{}: cites {cited}, which does not exist\n", mode.label()),
        );
    }

    /// A citation of an existing workspace file is clean.
    fn assert_existing_file_is_clean(mode: Mode, spelling: Spelling) {
        let (_, output) = run_case(mode, spelling, "probe.txt");
        assert_output(
            &output,
            0,
            &format!("{}: no claim mismatches found\n", mode.label()),
            "",
        );
    }

    #[test]
    fn doc_names_a_missing_file_cited_by_the_real_path_of_the_root() {
        assert_missing_file_is_named(Mode::Doc, Spelling::Real);
    }

    #[test]
    fn doc_names_a_missing_file_cited_by_the_symlink_of_the_root() {
        assert_missing_file_is_named(Mode::Doc, Spelling::Link);
    }

    #[test]
    fn doc_accepts_an_existing_file_cited_by_the_real_path_of_the_root() {
        assert_existing_file_is_clean(Mode::Doc, Spelling::Real);
    }

    #[test]
    fn doc_accepts_an_existing_file_cited_by_the_symlink_of_the_root() {
        assert_existing_file_is_clean(Mode::Doc, Spelling::Link);
    }

    #[test]
    fn item_names_a_missing_file_cited_by_the_real_path_of_the_root() {
        assert_missing_file_is_named(Mode::Item, Spelling::Real);
    }

    #[test]
    fn item_names_a_missing_file_cited_by_the_symlink_of_the_root() {
        assert_missing_file_is_named(Mode::Item, Spelling::Link);
    }

    #[test]
    fn item_accepts_an_existing_file_cited_by_the_real_path_of_the_root() {
        assert_existing_file_is_clean(Mode::Item, Spelling::Real);
    }

    #[test]
    fn item_accepts_an_existing_file_cited_by_the_symlink_of_the_root() {
        assert_existing_file_is_clean(Mode::Item, Spelling::Link);
    }

    /// With no `--dir`, the root is the git top level, in its real spelling. A
    /// citation through a symlink to the root is inside it.
    #[test]
    fn doc_names_a_missing_file_cited_by_the_symlink_when_git_gives_the_root() {
        let install = install_with_doc("");
        let (_outer, link) = link_to(install.path());
        let cited = format!("{}/{WORKSPACE}/missing.txt", link.display());
        write(install.path(), DOC, &format!("The run {cited} is cited.\n"));
        assert_output(
            &check_doc(&link),
            1,
            "",
            &format!("{DOC}: cites {cited}, which does not exist\n"),
        );
    }

    /// A file that the text cites in two spellings of the root is one finding,
    /// in the spelling of its first citation.
    #[test]
    fn doc_names_a_file_cited_in_two_spellings_once() {
        let install = install_with_doc("");
        let (_outer, link) = link_to(install.path());
        let real = format!("{}/{WORKSPACE}/missing.txt", root_of(install.path()));
        let linked = format!("{}/{WORKSPACE}/missing.txt", link.display());
        write(
            install.path(),
            DOC,
            &format!("The run {real} is cited, and {linked} again.\n"),
        );
        assert_output(
            &check_doc_through(&link),
            1,
            "",
            &format!("{DOC}: cites {real}, which does not exist\n"),
        );
    }

    /// A symlink below the root that leaves it (a workspace that the worktrees
    /// share, say) makes git refuse the path, and the run fails with git's
    /// line. An absolute citation gets the verdict of the relative citation of
    /// the same names: the check never passes a file silently that it must
    /// fail. The pathspec that git names is the path below the root.
    #[test]
    fn doc_gives_an_absolute_citation_through_a_symlink_the_verdict_of_the_relative_one() {
        let install = install_with_doc("");
        let outside = tempfile::tempdir().expect("tempdir");
        symlink(
            outside.path(),
            install.path().join(WORKSPACE).join("elsewhere"),
        )
        .expect("symlink out of the root");
        let below_the_root = format!("{WORKSPACE}/elsewhere/missing.txt");
        write(
            install.path(),
            DOC,
            &format!("The run {below_the_root} is cited.\n"),
        );
        let relative = check_doc(install.path());
        let root = root_of(install.path());
        write(
            install.path(),
            DOC,
            &format!("The run {root}/{below_the_root} is cited.\n"),
        );
        let absolute = check_doc(install.path());
        assert_eq!(relative.status.code(), Some(2));
        let relative_line = stderr(&relative);
        assert!(
            relative_line.starts_with(&format!("{DOC}: git check-ignore failed: ")),
            "{relative_line}"
        );
        assert!(relative_line.contains(&below_the_root), "{relative_line}");
        assert_eq!(
            (absolute.status.code(), stdout(&absolute), stderr(&absolute)),
            (relative.status.code(), stdout(&relative), relative_line),
        );
    }
}
