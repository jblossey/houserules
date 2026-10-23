//! The document modes of `check-report-claims` (HR-165 (3)): `--doc <path>`
//! reads a spec, and `--item <id>` reads a backlog item through the installed
//! tracker. Both read the text for the workspace paths it cites and name each
//! one that does not exist, so a spec or an item cannot cite evidence that
//! the batch workspace no longer holds. The parent module doc states the stance
//! of every Limits list.
//!
//! Limits (every doc comment of this file that says "see the module doc's
//! limits" means this list):
//! - `doc::check_doc` and `doc::check_item` (`--doc` and `--item`, HR-165
//!   (3)) check one thing: a cited workspace path exists. A workspace path is
//!   a cited path that `git check-ignore` reports as ignored, so a project
//!   whose workspace git does not ignore gets no finding from either mode, and
//!   a path that git tracks is never checked, because a spec cites files that
//!   the batch has yet to write. A checkout that lacks the workspace (a linked
//!   git worktree keeps its ignored files apart from the main checkout) reads
//!   every citation of it as missing: run the check where the workspace lives.
//!   Reading every citation as a path to find would flag nearly all of them:
//!   in this repository's specs, plans, and backlog item bodies the citations
//!   that name no existing file outnumber the workspace citations by far.
//! - The two modes read citations through `find_citations`, so they carry the
//!   limits of `find_citations` (`citations`'s Limits), and add their own. A
//!   token with a glob, set, or placeholder character (`*`, `?`, `[`, `]`, `{`,
//!   `}`, `<`, `>`) or an ellipsis names a set of paths, not one, and is
//!   skipped; so is a markdown link's target, which reaches the walk glued to
//!   its link text (`[probe](.superpowers/x.txt)` holds `[`). A path with a
//!   `..` segment is skipped as well, and so is an absolute path outside the
//!   install root: git cannot classify either from the root. An absolute path
//!   inside the root is checked as a relative one is. Inside means that an
//!   ancestor of the path has the real path of the root (`doc::root_relative`),
//!   so a symlinked directory, the real path that git prints, and a Windows
//!   short name all name the same root. The names below that ancestor are the
//!   names of a relative citation, so the two spellings of one file get one
//!   verdict. A path through a symlink below the root is judged at the target
//!   of the link (`rules::ignored_among`): git is asked about the target when
//!   it lies inside the root, and about the link itself when it does not,
//!   because git refuses a path beyond a symlink. The disk lookup follows the
//!   link, so a missing target is a missing file. A path inside a submodule is
//!   no workspace path of the install, because git refuses it too; it is
//!   skipped. A path under `.houserules/step/` is the output of the step
//!   script, not a workspace path, and is skipped. A file cited in two
//!   spellings is named once, in the first spelling. A token that starts with a
//!   letter, a colon, and a slash or backslash reads as a Windows drive
//!   (`citations::drive_prefix_len`), so its colon stays in the path whether or
//!   not a drive of that letter exists. A bare basename names no workspace path
//!   and is never resolved against the workspace. Only the existence of the
//!   path is checked: its line numbers and the sentence that cites it are not
//!   (`check_citation_lines` reads the line numbers of a report).
//! - `--item` reads every string of the record that the tracker returns, in
//!   document order, as one text, so a path cited in any field counts. A
//!   record that holds no string is a clean run.

use std::path::{Component, Path};

use serde_json::Value;

use crate::rules::ignored_among;
use crate::tracker::{self, Lookup};

use super::citations::find_citations;
use super::{read_utf8_lossy, resolve_against};

/// The folder below the install root that holds the output of the step script
/// (`rules.json`, `audit.json`, `validate.json`). A tool wrote it, and the
/// checkout has not run the tool, so a citation of it names no evidence of the
/// workspace.
const STEP_OUTPUT_FOLDER: &str = ".houserules/step";

/// The characters that make a cited token a pattern or a template, not one
/// path: a glob, a set, or an angle-bracket placeholder.
const PATTERN_CHARACTERS: [char; 8] = ['*', '?', '[', ']', '{', '}', '<', '>'];

/// `true` when `path`, as a spec or an item cites it, names exactly one file:
/// it holds no `..` segment and is no glob, set, placeholder, or ellipsis.
fn names_one_file(path: &str) -> bool {
    !path.contains(PATTERN_CHARACTERS)
        && !path.contains("...")
        && !Path::new(path)
            .components()
            .any(|part| part == Component::ParentDir)
}

/// A cited path that git is asked about.
struct Candidate<'a> {
    /// The path as the text spells it: the name that a finding carries.
    cited: &'a str,
    /// The path below the install root, with `/` separators: the name that git
    /// and the disk lookup take.
    relative: String,
}

/// The path that git takes for `cited` from `root`, or `None` when `cited` is
/// outside `root`. A relative path is its own answer. An absolute path is
/// inside `root` when one of its ancestors has the real path of `root`,
/// whatever the spelling of either (a symlinked directory, the real path that
/// git prints, a Windows short name). The answer is then the names below that
/// ancestor, as the text spells them, with `/` separators: an absolute
/// citation gets the verdict of the relative citation of the same names, also
/// where a symlink below the root lies on the path (`ignored_among` judges it).
/// An absolute path with no such ancestor, and the root itself, have no
/// answer: git refuses a path outside its repository. A `root` that cannot be
/// resolved is the operating system's error.
fn root_relative(cited: &str, root: &Path) -> std::io::Result<Option<String>> {
    let location = Path::new(cited);
    if !location.is_absolute() {
        return Ok((!location.has_root()).then(|| cited.to_string()));
    }
    let real_root = root.canonicalize()?;
    let mut below = Vec::new();
    for ancestor in location.ancestors() {
        if ancestor
            .canonicalize()
            .is_ok_and(|real_ancestor| real_ancestor == real_root)
        {
            below.reverse();
            return Ok((!below.is_empty()).then(|| below.join("/")));
        }
        let Some(name) = ancestor.file_name() else {
            return Ok(None);
        };
        below.push(name.to_string_lossy());
    }
    Ok(None)
}

/// The members of `candidates` that git ignores in the working tree at
/// `root`, in the order given. A failure to run git, or a git that fails, is a
/// named error with git's own text. A candidate inside a submodule is no
/// member (`ignored_among` has the rule).
fn ignored_candidates<'a>(
    root: &Path,
    candidates: Vec<Candidate<'a>>,
) -> Result<Vec<Candidate<'a>>, String> {
    let relatives: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.relative.as_str())
        .collect();
    let ignored = ignored_among(root, &relatives).map_err(|fault| fault.to_string())?;
    Ok(candidates
        .into_iter()
        .filter(|candidate| ignored.contains(&candidate.relative))
        .collect())
}

/// `true` when `relative`, a path below the install root, lies in the folder of
/// the step script's output.
fn is_step_output(relative: &str) -> bool {
    Path::new(relative).starts_with(STEP_OUTPUT_FOLDER)
}

/// Names each workspace path that `text` cites and that does not exist under
/// `root`, one line each: `<label>: cites <path>, which does not exist`. A
/// workspace path is a cited path that git ignores (the batch workspace is
/// not tracked, so only the disk can hold what a citation names). A file
/// cited twice, in one spelling or two, is named once, in the spelling and the
/// order of its first citation. A relative citation and an absolute citation
/// inside `root` are candidates (`root_relative` defines inside); an absolute
/// citation outside `root` is skipped, and so is a citation of the output of
/// the step script (`STEP_OUTPUT_FOLDER`). A citation through a symlink is
/// judged at the target of the link (`ignored_among` has the rule), and the
/// disk lookup follows the link, so a missing target is a missing file. A
/// cited path that exists but cannot be
/// inspected is a line with the operating system's error text. An absolute
/// citation with a `root` that cannot be resolved is the line `<label>:
/// cannot resolve the install root <root>: <the operating system's error>`.
fn check_workspace_citations(root: &Path, label: &str, text: &str) -> Result<Vec<String>, String> {
    let citations = find_citations(text);
    let mut candidates: Vec<Candidate> = Vec::new();
    for citation in &citations {
        if !names_one_file(&citation.path) {
            continue;
        }
        let relative = root_relative(&citation.path, root).map_err(|error| {
            format!(
                "{label}: cannot resolve the install root {}: {error}",
                root.display()
            )
        })?;
        if let Some(relative) = relative
            && !is_step_output(&relative)
            && !candidates
                .iter()
                .any(|candidate| candidate.relative == relative)
        {
            candidates.push(Candidate {
                cited: &citation.path,
                relative,
            });
        }
    }
    let mut errors = Vec::new();
    for candidate in
        ignored_candidates(root, candidates).map_err(|error| format!("{label}: {error}"))?
    {
        let cited = candidate.cited;
        match resolve_against(root, &candidate.relative).try_exists() {
            Ok(true) => {}
            Ok(false) => errors.push(format!("{label}: cites {cited}, which does not exist")),
            Err(error) => errors.push(format!(
                "{label}: cites {cited}, which could not be read ({error})"
            )),
        }
    }
    Ok(errors)
}

/// Checks the spec at `doc`: reads it and names each cited workspace path
/// that does not exist under `root`. `doc` is resolved against the working
/// directory and named in each line as the caller gave it. A doc that cannot
/// be read is a named error.
pub(super) fn check_doc(doc: &Path, root: &Path) -> Result<Vec<String>, String> {
    let label = doc.display().to_string();
    let resolved =
        crate::node_path::resolve_like_node(doc).map_err(|error| format!("{label}: {error}"))?;
    let text = read_utf8_lossy(&resolved).map_err(|error| format!("{label}: {error}"))?;
    check_workspace_citations(root, &label, &text)
}

/// Every string in `value`, in document order, one per line.
fn text_of(value: &Value, lines: &mut Vec<String>) {
    match value {
        Value::String(text) => lines.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| text_of(item, lines)),
        Value::Object(fields) => fields.values().for_each(|field| text_of(field, lines)),
        _ => {}
    }
}

/// Checks the backlog item `id`: reads its record through the installed
/// tracker and names each cited workspace path that does not exist under
/// `root`. With no tracker the error is line A or line B of spec 3.4; an item
/// that the tracker does not hold, or a tracker that cannot be reached, is the
/// tracker's own line.
pub(super) fn check_item(id: &str, root: &Path) -> Result<Vec<String>, String> {
    let tracker = match tracker::lookup(root).map_err(|error| error.to_string())? {
        Lookup::Installed(tracker) => tracker,
        Lookup::Absent(state) => return Err(state.line().to_string()),
    };
    let records = tracker
        .get(&[id.to_string()])
        .map_err(|error| error.to_string())?;
    let mut lines = Vec::new();
    records
        .iter()
        .for_each(|record| text_of(record, &mut lines));
    check_workspace_citations(root, id, &lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    /// A relative path with plain segments names one file.
    #[test]
    fn a_relative_plain_path_names_one_file() {
        for path in [
            ".superpowers/sdd/batch/probe.txt",
            "probe.txt",
            "a/b.c/d.json",
        ] {
            assert!(names_one_file(path), "{path}");
        }
    }

    /// The output folder of the step script is matched by whole path
    /// components, at the root only.
    #[test]
    fn the_step_output_folder_is_matched_by_components_at_the_root() {
        for path in [".houserules/step/rules.json", ".houserules/step/a/b.json"] {
            assert!(is_step_output(path), "{path}");
        }
        for path in [
            ".houserules/steps/rules.json",
            ".houserules/step.json",
            ".houserules/work/step/rules.json",
            "docs/.houserules/step/rules.json",
        ] {
            assert!(!is_step_output(path), "{path}");
        }
    }

    /// A glob, a set, a placeholder, an ellipsis, and a path that climbs out of
    /// the root name no one file.
    #[test]
    fn a_pattern_or_a_parent_segment_names_no_one_file() {
        for path in [
            ".superpowers/sdd/*/x.txt",
            "x?.txt",
            "dir/[ab].txt",
            "dir/{a,b}.txt",
            "dir/<task>/x.txt",
            "dir/x>.txt",
            "dir/.../x.txt",
            "../x.txt",
            "a/../x.txt",
            "/abs/../x.txt",
        ] {
            assert!(!names_one_file(path), "{path}");
        }
    }

    /// A relative path is its own answer. An absolute path under the root
    /// answers with its path below the root, built component by component, even
    /// when the directories between do not exist. An absolute path elsewhere
    /// has no answer, and neither has a sibling directory whose name starts with
    /// the name of the root, nor the root itself.
    #[test]
    fn a_path_is_inside_the_root_when_it_is_relative_or_under_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("repo");
        std::fs::create_dir(&root).expect("create the root");
        let spelled = |path: PathBuf| path.to_str().expect("a UTF-8 path").to_string();
        let answers = [
            (
                ".superpowers/sdd/b/x.txt".to_string(),
                Some(".superpowers/sdd/b/x.txt"),
            ),
            ("x.txt".to_string(), Some("x.txt")),
            (
                spelled(root.join(".superpowers").join("x.txt")),
                Some(".superpowers/x.txt"),
            ),
            (spelled(root.join("x.txt")), Some("x.txt")),
            (
                spelled(root.join("a").join("b").join("c.txt")),
                Some("a/b/c.txt"),
            ),
            (spelled(dir.path().join("repo-sibling").join("x.txt")), None),
            (spelled(dir.path().join("x.txt")), None),
            (spelled(root.clone()), None),
        ];
        for (path, answer) in answers {
            let found = root_relative(&path, &root).expect("the root exists");
            assert_eq!(found.as_deref(), answer, "{path}");
        }
    }

    /// An absolute path is inside the root when an ancestor of it has the real
    /// path of the root: a symlink to the root names the root. The names below
    /// that ancestor stay as the text spells them, so a symlink below the root,
    /// to a directory inside it or outside it, changes no answer.
    #[cfg(unix)]
    #[test]
    fn a_path_is_inside_the_root_when_an_ancestor_has_the_real_path_of_the_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("repo");
        let elsewhere = dir.path().join("elsewhere");
        for path in [&root, &elsewhere, &root.join("sub")] {
            std::fs::create_dir(path).expect("create a directory");
        }
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&root, &link).expect("symlink to the root");
        std::os::unix::fs::symlink(&elsewhere, root.join("out")).expect("symlink out");
        std::os::unix::fs::symlink(root.join("sub"), root.join("alias")).expect("symlink in");
        let spelled = |path: PathBuf| path.to_str().expect("a UTF-8 path").to_string();
        let answers = [
            (spelled(link.join("x.txt")), Some("x.txt")),
            (spelled(link.join("a").join("x.txt")), Some("a/x.txt")),
            (spelled(root.join("out").join("x.txt")), Some("out/x.txt")),
            (
                spelled(root.join("alias").join("x.txt")),
                Some("alias/x.txt"),
            ),
            (spelled(link.join("out").join("x.txt")), Some("out/x.txt")),
        ];
        for (path, answer) in answers {
            let found = root_relative(&path, &root).expect("the root exists");
            assert_eq!(found.as_deref(), answer, "{path}");
        }
        let through_the_link = root_relative(&spelled(root.join("x.txt")), &link);
        assert_eq!(through_the_link.unwrap().as_deref(), Some("x.txt"));
    }

    /// A root that cannot be resolved, with an absolute citation to classify, is
    /// a named line that carries the operating system's text.
    #[test]
    fn a_root_that_cannot_be_resolved_is_a_named_error_for_an_absolute_citation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("gone");
        let text = format!("See {}/x.txt.", root.display());
        let error = check_workspace_citations(&root, "doc.md", &text)
            .expect_err("the root cannot be resolved");
        let prefix = format!(
            "doc.md: cannot resolve the install root {}: ",
            root.display()
        );
        assert!(error.starts_with(&prefix), "{error}");
        assert!(error.len() > prefix.len(), "{error}");
    }

    /// `text_of` collects every string of a record, nested or not, and skips
    /// numbers, booleans, and nulls.
    #[test]
    fn text_of_collects_every_string_of_a_record() {
        let record = json!({
            "id": "HR-1",
            "title": "T",
            "body": ["a", "b"],
            "batch": 3,
            "see": [],
            "deferral": {"reason": "r", "files": ["f.rs"], "origin": null},
        });
        let mut lines = Vec::new();
        text_of(&record, &mut lines);
        assert_eq!(lines, ["HR-1", "T", "a", "b", "r", "f.rs"]);
    }

    /// With no candidate path, git does not run: a directory that is no
    /// repository is not an error then. A glob is no candidate.
    #[test]
    fn no_candidate_path_needs_no_git() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(
            check_workspace_citations(dir.path(), "doc.md", "Only the glob .superpowers/*/x.txt.")
                .unwrap(),
            Vec::<String>::new()
        );
    }

    /// An absolute citation inside the root is a candidate: git is asked about
    /// it, and a directory that is no repository makes git fail.
    #[test]
    fn an_absolute_citation_inside_the_root_is_a_candidate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = format!("See {}/.superpowers/x.txt.", dir.path().display());
        let error = check_workspace_citations(dir.path(), "doc.md", &text)
            .expect_err("git is asked about a candidate");
        assert!(
            error.starts_with("doc.md: git check-ignore failed: "),
            "{error}"
        );
    }

    /// An absolute citation outside the root is no candidate, so git never
    /// sees it. A sibling directory whose name starts with the name of the root
    /// is outside the root.
    #[test]
    fn an_absolute_citation_outside_the_root_is_no_candidate() {
        let dir = tempfile::tempdir().expect("tempdir");
        let text = format!(
            "See /outside/x.txt and {}-sibling/.superpowers/x.txt.",
            dir.path().display()
        );
        assert_eq!(
            check_workspace_citations(dir.path(), "doc.md", &text).unwrap(),
            Vec::<String>::new()
        );
    }

    /// A directory that git cannot read is a named error with git's text, never
    /// a clean run.
    #[test]
    fn a_directory_that_is_no_repository_is_a_named_git_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = check_workspace_citations(dir.path(), "doc.md", "See .superpowers/x.txt.")
            .expect_err("git cannot classify a path outside a repository");
        assert!(
            error.starts_with("doc.md: git check-ignore failed: "),
            "{error}"
        );
    }
}
