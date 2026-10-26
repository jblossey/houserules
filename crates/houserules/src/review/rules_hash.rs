//! The hash of the rule sources of a commit, which the packet and the change
//! record store (spec 4.5).
//!
//! The rule sources of an install are its `knowledge/` folder, its
//! `.houserules.json`, and its CI files. The hash is the SHA-256 of the
//! listing that `git ls-tree -r --full-name` prints for them at one commit, so
//! it depends on the commit and the install path only, never on the work tree
//! or on the folder it runs from.
//!
//! The CI files are the files of the commit's own tree: the seeded workflow of
//! the install, the files that `review.ci.files` names in the marker of the
//! commit, and each file whose first line is `ci write`'s comment. All three
//! are top-level paths.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

use super::git;
use crate::baseline;
use crate::install::marker::review::CI_WRITTEN_BY_LINE;

/// The pathspec of `top_path` (a path from the git top level) with the magic
/// `top` and `literal`, so that no character of the path reads as a pattern.
fn literal_pathspec(top_path: &str) -> String {
    format!(":(top,literal){top_path}")
}

/// The folder of the install from the git top level with a closing `/`; empty
/// for a top-level install.
fn folder_of(install: &str) -> String {
    if install == "." {
        String::new()
    } else {
        format!("{install}/")
    }
}

/// The path of the marker of the install `install` from the git top level.
pub(crate) fn marker_path(install: &str) -> String {
    format!("{}.houserules.json", folder_of(install))
}

/// The text of the marker of the install `install` at `commit`, read from git:
/// `None` when the commit holds none.
pub(crate) fn marker_at(
    root: &Path,
    install: &str,
    commit: &str,
) -> Result<Option<String>, String> {
    blob_at(root, commit, &marker_path(install))
}

/// The path of the workflow file that `init` seeded for the install: its
/// folders joined by `-` in `knowledge-<folder>.yml`, or `knowledge.yml` for a
/// top-level install.
fn seeded_workflow_path(install: &str) -> String {
    let components: Vec<String> = if install == "." {
        Vec::new()
    } else {
        install.split('/').map(str::to_string).collect()
    };
    crate::install::workflow_path(&components)
}

/// The SHA-256 of the rule sources of the install `install` at `commit`: `None`
/// when the commit holds none. `install` is the path of the install from the
/// git top level, `.` for a top-level install.
pub(crate) fn rules_hash(
    root: &Path,
    install: &str,
    commit: &str,
) -> Result<Option<String>, String> {
    let ci_files = ci_files_at(root, install, commit)?;
    hash_with(root, install, commit, &ci_files)
}

/// Like [`rules_hash`], for CI files that the caller listed with
/// [`ci_files_at`] already.
pub(super) fn hash_with(
    root: &Path,
    install: &str,
    commit: &str,
    ci_files: &[String],
) -> Result<Option<String>, String> {
    let folder = folder_of(install);
    let mut pathspecs = vec![
        literal_pathspec(&format!("{folder}knowledge")),
        literal_pathspec(&format!("{folder}.houserules.json")),
    ];
    pathspecs.extend(ci_files.iter().map(|file| literal_pathspec(file)));
    let mut args = vec!["ls-tree", "-r", "--full-name", commit, "--"];
    args.extend(pathspecs.iter().map(String::as_str));
    let listing = git::run_bytes(root, &args)?;
    Ok((!listing.is_empty()).then(|| baseline::hash(&listing)))
}

/// The CI files of the install at `commit`, as sorted top-level paths without
/// duplicates (spec 4.1): the seeded workflow, the files that `review.ci.files`
/// names, and the files whose first line is the comment of `ci write`.
pub(crate) fn ci_files_at(root: &Path, install: &str, commit: &str) -> Result<Vec<String>, String> {
    let mut files = BTreeSet::new();
    files.insert(seeded_workflow_path(install));
    let marker = marker_path(install);
    if let Some(text) = marker_at(root, install, commit)? {
        let value: Value = serde_json::from_str(&text)
            .map_err(|error| format!("{}:{marker}: not valid JSON: {error}", git::short(commit)))?;
        let listed = value
            .pointer("/review/ci/files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter(|path| names_a_file_below_the_top(path));
        files.extend(listed.map(str::to_string));
    }
    files.extend(files_written_by_ci_write(root, commit)?);
    Ok(files.into_iter().collect())
}

/// `true` when `path` can name a file below the git top level: it is relative
/// and holds no `.` or `..` folder. Any other entry of `review.ci.files`
/// matches nothing.
fn names_a_file_below_the_top(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// The content of the file `top_path` at `commit`, or `None` when the commit
/// holds no file of that path.
fn blob_at(root: &Path, commit: &str, top_path: &str) -> Result<Option<String>, String> {
    let listing = git::run_bytes(
        root,
        &[
            "ls-tree",
            "-z",
            "--full-name",
            commit,
            "--",
            &literal_pathspec(top_path),
        ],
    )?;
    let listing = String::from_utf8_lossy(&listing);
    let Some((meta, _name)) = listing
        .split('\0')
        .find(|entry| !entry.is_empty())
        .and_then(|entry| entry.split_once('\t'))
    else {
        return Ok(None);
    };
    let mut fields = meta.split(' ');
    let (Some(_mode), Some("blob"), Some(sha)) = (fields.next(), fields.next(), fields.next())
    else {
        return Ok(None);
    };
    let content = git::run_bytes(root, &["cat-file", "blob", sha])?;
    Ok(Some(String::from_utf8_lossy(&content).into_owned()))
}

/// The files of the tree of `commit` whose first line is the comment of
/// `ci write`. `git grep` finds the files that hold the line anywhere; the
/// first line decides.
fn files_written_by_ci_write(root: &Path, commit: &str) -> Result<Vec<String>, String> {
    let listing = git::run_tolerating(
        root,
        &[
            "grep",
            "-z",
            "-l",
            "--full-name",
            "-F",
            "-e",
            CI_WRITTEN_BY_LINE,
            commit,
            "--",
            ":/",
        ],
        1,
    )?;
    let listing = String::from_utf8_lossy(&listing);
    let prefix = format!("{commit}:");
    let mut written = Vec::new();
    for entry in listing.split('\0').filter(|entry| !entry.is_empty()) {
        let path = entry.strip_prefix(&prefix).unwrap_or(entry);
        let content = git::run_bytes(root, &["cat-file", "blob", &format!("{commit}:{path}")])?;
        let content = String::from_utf8_lossy(&content);
        if content.split('\n').next() == Some(CI_WRITTEN_BY_LINE) {
            written.push(path.to_string());
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::super::git::{self, scratch::Repo};
    use super::*;
    use crate::baseline;
    use crate::install::marker::review::CI_WRITTEN_BY_LINE;

    /// The hash that spec 4.5 defines, computed from the listing of git for the
    /// pathspecs `pathspecs`.
    fn expected_hash(repo: &Repo, commit: &str, pathspecs: &[&str]) -> Option<String> {
        let mut args = vec![
            "-c",
            "core.quotePath=false",
            "ls-tree",
            "-r",
            "--full-name",
            commit,
            "--",
        ];
        args.extend(pathspecs);
        let listing = repo.git(&args);
        (!listing.is_empty()).then(|| baseline::hash(listing.as_bytes()))
    }

    /// Writes a marker below `folder` (`""` or `sub/`) whose `review.ci.files`
    /// names `ci_files`.
    fn write_marker(repo: &Repo, folder: &str, ci_files: &[&str]) {
        let marker = serde_json::json!({"review": {"ci": {"files": ci_files}}});
        repo.write(&format!("{folder}.houserules.json"), &marker.to_string());
    }

    #[test]
    fn the_hash_is_the_sha256_of_the_listing_of_the_rule_sources() {
        let repo = Repo::new();
        repo.write("knowledge/a.json", "{}\n");
        repo.write("knowledge/packs/p.json", "{}\n");
        repo.write(".houserules.json", "{}\n");
        repo.write("README.md", "readme\n");
        let commit = repo.commit("first");

        let hash = rules_hash(repo.root(), ".", &commit).expect("hash");

        assert_eq!(
            hash,
            expected_hash(
                &repo,
                &commit,
                &[
                    ":/knowledge",
                    ":/.houserules.json",
                    ":/.github/workflows/knowledge.yml"
                ]
            )
        );
        assert_eq!(hash.expect("a hash").len(), 64);
    }

    #[test]
    fn a_change_outside_the_rule_sources_leaves_the_hash_and_a_change_inside_moves_it() {
        let repo = Repo::new();
        repo.write("knowledge/a.json", "{}\n");
        repo.write(".houserules.json", "{}\n");
        repo.write("README.md", "one\n");
        let first = repo.commit("first");
        repo.write("README.md", "two\n");
        let second = repo.commit("readme");
        repo.write("knowledge/a.json", "{\"a\": 1}\n");
        let third = repo.commit("knowledge");

        let hash = |commit: &str| rules_hash(repo.root(), ".", commit).expect("hash");

        assert_eq!(hash(&first), hash(&second));
        assert_ne!(hash(&second), hash(&third));
    }

    #[test]
    fn the_hash_follows_the_commit_and_never_the_work_tree() {
        let repo = Repo::new();
        repo.write("knowledge/a.json", "{}\n");
        let commit = repo.commit("first");
        let before = rules_hash(repo.root(), ".", &commit).expect("hash");
        repo.write("knowledge/a.json", "{\"edited\": true}\n");
        repo.write("knowledge/new.json", "{}\n");

        assert_eq!(rules_hash(repo.root(), ".", &commit).expect("hash"), before);
    }

    #[test]
    fn a_commit_with_no_rule_source_has_no_hash() {
        let repo = Repo::new();
        repo.write("README.md", "readme\n");
        let commit = repo.commit("first");

        assert_eq!(rules_hash(repo.root(), ".", &commit), Ok(None));
    }

    #[test]
    fn a_subdirectory_install_hashes_alike_from_the_top_and_from_below() {
        let repo = Repo::new();
        repo.write("sub/knowledge/a.json", "{}\n");
        repo.write("sub/.houserules.json", "{}\n");
        repo.write("knowledge/other.json", "{}\n");
        let commit = repo.commit("first");

        let from_top = rules_hash(repo.root(), "sub", &commit).expect("hash");
        let from_install = rules_hash(&repo.root().join("sub"), "sub", &commit).expect("hash");
        let from_knowledge =
            rules_hash(&repo.root().join("sub/knowledge"), "sub", &commit).expect("hash");

        assert_eq!(
            from_top,
            expected_hash(
                &repo,
                &commit,
                &[
                    ":/sub/knowledge",
                    ":/sub/.houserules.json",
                    ":/.github/workflows/knowledge-sub.yml"
                ]
            )
        );
        assert_eq!(from_install, from_top);
        assert_eq!(from_knowledge, from_top);
        assert!(from_top.is_some());
    }

    #[test]
    fn the_seeded_workflow_enters_the_hash_by_the_name_of_its_install() {
        let repo = Repo::new();
        repo.write("a/b/knowledge/x.json", "{}\n");
        repo.write(".github/workflows/knowledge-a-b.yml", "name: one\n");
        let first = repo.commit("first");
        repo.write(".github/workflows/knowledge-a-b.yml", "name: two\n");
        let second = repo.commit("workflow");
        repo.write(".github/workflows/knowledge-a-c.yml", "name: other\n");
        let third = repo.commit("workflow of another install");

        let hash = |commit: &str| rules_hash(repo.root(), "a/b", commit).expect("hash");

        assert_ne!(hash(&first), hash(&second));
        assert_eq!(hash(&second), hash(&third));
    }

    #[test]
    fn the_ci_files_of_the_marker_are_read_at_the_commit_of_the_hash() {
        let repo = Repo::new();
        write_marker(&repo, "", &[".gitlab-ci.yml"]);
        repo.write(".gitlab-ci.yml", "one\n");
        let first = repo.commit("first");
        let at_first = rules_hash(repo.root(), ".", &first).expect("hash");
        repo.write(".gitlab-ci.yml", "two\n");
        let second = repo.commit("edit the ci file");
        write_marker(&repo, "", &[".gitlab-ci.yml", "extra.yml"]);
        repo.write("extra.yml", "extra\n");
        let third = repo.commit("extend review.ci.files");

        assert_ne!(
            rules_hash(repo.root(), ".", &second).expect("hash"),
            at_first
        );
        assert_eq!(
            rules_hash(repo.root(), ".", &first).expect("hash"),
            at_first
        );
        assert_eq!(
            ci_files_at(repo.root(), ".", &first),
            Ok(vec![
                ".github/workflows/knowledge.yml".to_string(),
                ".gitlab-ci.yml".to_string()
            ])
        );
        assert_eq!(
            ci_files_at(repo.root(), ".", &third),
            Ok(vec![
                ".github/workflows/knowledge.yml".to_string(),
                ".gitlab-ci.yml".to_string(),
                "extra.yml".to_string()
            ])
        );
    }

    #[test]
    fn a_file_whose_first_line_is_the_ci_write_comment_is_a_ci_file_at_its_own_commit() {
        let repo = Repo::new();
        repo.write("knowledge/a.json", "{}\n");
        repo.write(".github/workflows/my review.yml", "name: review\n");
        let plain = repo.commit("a file without the comment");
        let plain_before_later_commits = rules_hash(repo.root(), ".", &plain).expect("hash");
        repo.write(".github/workflows/my review.yml", "name: review again\n");
        let plain_edited = repo.commit("edit it");
        repo.write(
            ".github/workflows/my review.yml",
            &format!("{CI_WRITTEN_BY_LINE}\nname: review\n"),
        );
        let written = repo.commit("ci write");
        repo.write(
            ".github/workflows/my review.yml",
            &format!("{CI_WRITTEN_BY_LINE}\nname: review edited\n"),
        );
        let written_edited = repo.commit("edit the written file");

        let hash = |commit: &str| rules_hash(repo.root(), ".", commit).expect("hash");

        assert_eq!(
            hash(&plain),
            plain_before_later_commits,
            "a file that gains the comment later leaves the earlier commit's hash alone"
        );
        assert_eq!(hash(&plain), hash(&plain_edited));
        assert_ne!(hash(&plain_edited), hash(&written));
        assert_ne!(hash(&written), hash(&written_edited));
        assert_eq!(
            ci_files_at(repo.root(), ".", &written),
            Ok(vec![
                ".github/workflows/knowledge.yml".to_string(),
                ".github/workflows/my review.yml".to_string()
            ])
        );
        assert_eq!(
            ci_files_at(repo.root(), ".", &plain),
            Ok(vec![".github/workflows/knowledge.yml".to_string()])
        );
    }

    #[test]
    fn the_comment_on_a_later_line_makes_no_ci_file() {
        let repo = Repo::new();
        repo.write("ci.yml", &format!("name: x\n{CI_WRITTEN_BY_LINE}\n"));
        let commit = repo.commit("first");

        assert_eq!(
            ci_files_at(repo.root(), ".", &commit),
            Ok(vec![".github/workflows/knowledge.yml".to_string()])
        );
    }

    #[test]
    fn the_ci_files_of_a_subdirectory_install_are_top_level_paths() {
        let repo = Repo::new();
        write_marker(&repo, "sub/", &[".gitlab-ci.yml"]);
        repo.write("sub/knowledge/a.json", "{}\n");
        let commit = repo.commit("first");

        assert_eq!(
            ci_files_at(&repo.root().join("sub"), "sub", &commit),
            Ok(vec![
                ".github/workflows/knowledge-sub.yml".to_string(),
                ".gitlab-ci.yml".to_string()
            ])
        );
    }

    #[test]
    fn an_entry_that_cannot_name_a_file_below_the_top_matches_nothing() {
        let repo = Repo::new();
        write_marker(
            &repo,
            "",
            &["", "/abs.yml", "../up.yml", "a/../b.yml", "ok.yml"],
        );
        let commit = repo.commit("first");

        assert_eq!(
            ci_files_at(repo.root(), ".", &commit),
            Ok(vec![
                ".github/workflows/knowledge.yml".to_string(),
                "ok.yml".to_string()
            ])
        );
        assert!(rules_hash(repo.root(), ".", &commit).is_ok());
    }

    #[test]
    fn a_marker_without_the_review_key_names_no_ci_file() {
        let repo = Repo::new();
        repo.write(".houserules.json", "{\"schema\": \"x\"}\n");
        let commit = repo.commit("first");

        assert_eq!(
            ci_files_at(repo.root(), ".", &commit),
            Ok(vec![".github/workflows/knowledge.yml".to_string()])
        );
    }

    #[test]
    fn a_marker_that_is_not_json_is_a_named_error() {
        let repo = Repo::new();
        repo.write(".houserules.json", "{\n");
        let commit = repo.commit("first");

        let error = ci_files_at(repo.root(), ".", &commit).expect_err("not JSON");

        assert!(
            error.starts_with(&format!(
                "{}:.houserules.json: not valid JSON: ",
                git::short(&commit)
            )),
            "{error}"
        );
    }

    #[test]
    fn a_revision_that_is_no_commit_is_a_named_error() {
        let repo = Repo::new();
        repo.write("README.md", "readme\n");
        repo.commit("first");

        assert!(rules_hash(repo.root(), ".", "no-such-commit").is_err());
        assert!(ci_files_at(repo.root(), ".", "no-such-commit").is_err());
    }

    #[test]
    fn the_marker_is_read_from_the_commit_and_never_from_the_work_tree() {
        let repo = Repo::new();
        repo.write(".houserules.json", "{\"top\": true}\n");
        repo.write("sub/.houserules.json", "{\"a\": 1}\n");
        let first = repo.commit("first");
        repo.write("sub/.houserules.json", "{\"a\": 2}\n");

        assert_eq!(
            marker_at(repo.root(), ".", &first),
            Ok(Some("{\"top\": true}\n".to_string()))
        );
        assert_eq!(
            marker_at(repo.root(), "sub", &first),
            Ok(Some("{\"a\": 1}\n".to_string()))
        );
        assert_eq!(marker_at(repo.root(), "other", &first), Ok(None));
    }
}
