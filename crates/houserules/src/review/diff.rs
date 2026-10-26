//! The reader of git's diff text for the change analysis: the hunks of a
//! range and the line counts per path (spec 4.2).
//!
//! Two git calls describe a range. `git diff -U0` gives the hunks and the
//! header lines that tell the kind of each path; `git diff -z --numstat` gives
//! the line counts and the exact names. The second call has no `-U0`, because
//! that option makes git print the patch there too.
//!
//! The patch writes a name C-quoted when it holds a quote, a backslash, or a
//! control character, and writes a name with a space as it is. [`combine`]
//! reads the name of each section from the section itself, whichever form
//! it has, and pairs the section with the numstat row of the same path. A file
//! that becomes a symlink or a submodule, or the reverse, has two sections and
//! one row: the sections of one path make one changed path. A section with no
//! row, and a row with no section, is a named error
//! (`houserules.crash-paths-are-named`). Names are decoded lossily, as `audit`
//! decodes them.

use std::path::Path;

use indexmap::IndexMap;

use super::change::ChangedPath;
use super::git;
use crate::published::change::{ChangeKind, EmptyChange, LineRange};

/// The settings that git reads from its configuration, which both calls give
/// as options and which neither the user nor the repository can change: an
/// empty attributes file and git's own limit for a big file.
const PINNED_SETTINGS: [&str; 4] = [
    "-c",
    "core.attributesFile=",
    "-c",
    "core.bigFileThreshold=512m",
];

/// The options of both calls (spec 4.2). Each option gives the value that a
/// git without configuration uses, or turns a feature off, so that no
/// configuration of the host changes the kinds, the counts, the ranges, or the
/// paths of the parsed text:
/// - `-M` and `-l1000`: rename detection and git's own limit for it;
/// - `--no-relative`: top-level paths;
/// - `--diff-algorithm=myers`, `--inter-hunk-context=0`, and
///   `--indent-heuristic`: the hunks that git computes;
/// - `--ignore-submodules=none` and `--submodule=short`: a submodule shows as
///   its own change, in the short form;
/// - `--src-prefix=a/` and `--dst-prefix=b/`: fixed path prefixes;
/// - `--no-color`, `--no-ext-diff`, and `--no-textconv`: plain text from git's
///   own diff.
const DIFF_OPTIONS: [&str; 13] = [
    "-M",
    "-l1000",
    "--no-relative",
    "--diff-algorithm=myers",
    "--inter-hunk-context=0",
    "--indent-heuristic",
    "--ignore-submodules=none",
    "--submodule=short",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
];

/// The paths that the range `from` to `to` changes below the install whose
/// path from the git top level is `prefix` (`sub/`, empty for a top-level
/// install), sorted by path. A path outside the install is out of the range,
/// so a rename across the border is an added or a deleted file, never a
/// rename. `--attr-source` names `empty_tree`, so that no `.gitattributes` of
/// the request, and no `-diff` attribute in it, changes the text. The one
/// attribute source that stays is the repository's own
/// `$GIT_DIR/info/attributes`, an input of a local run that a CI clone does
/// not hold (design.md 255).
pub(crate) fn read_range(
    root: &Path,
    empty_tree: &str,
    from: &str,
    to: &str,
    prefix: &str,
) -> Result<Vec<ChangedPath>, String> {
    let attr_source = format!("--attr-source={empty_tree}");
    let pathspec =
        (!prefix.is_empty()).then(|| format!(":(top,literal){}", prefix.trim_end_matches('/')));
    let run = |extra: &[&str]| -> Result<Vec<u8>, String> {
        let mut args = Vec::from(PINNED_SETTINGS);
        args.extend([attr_source.as_str(), "diff"]);
        args.extend(DIFF_OPTIONS);
        args.extend(extra);
        args.extend([from, to, "--"]);
        args.extend(pathspec.as_deref());
        git::run_bytes(root, &args)
    };
    let patch = run(&["-U0"])?;
    let numstat = run(&["-z", "--numstat"])?;
    combine(&String::from_utf8_lossy(&patch), &numstat, prefix)
}

/// One hunk header: `@@ -<old_start>,<old_count> +<new_start>,<new_count> @@`.
#[derive(Debug, Clone, Copy)]
struct Hunk {
    old_start: u64,
    old_count: u64,
    new_start: u64,
    new_count: u64,
}

/// One `diff --git` section of the patch: its header facts and its hunks.
#[derive(Debug, Default)]
struct Section {
    /// The text after `diff --git `.
    header: String,
    renamed_to: Option<String>,
    /// The modes that the header lines name.
    modes: Vec<String>,
    new_file: bool,
    deleted_file: bool,
    mode_change: bool,
    binary: bool,
    hunks: Vec<Hunk>,
}

impl Section {
    /// Takes the facts of one header line, which comes before the first hunk.
    fn read_header_line(&mut self, line: &str) {
        if let Some(name) = line.strip_prefix("rename to ") {
            self.renamed_to = Some(read_name(name));
        } else if let Some(mode) = line.strip_prefix("new file mode ") {
            self.new_file = true;
            self.modes.push(mode.to_string());
        } else if let Some(mode) = line.strip_prefix("deleted file mode ") {
            self.deleted_file = true;
            self.modes.push(mode.to_string());
        } else if let Some(mode) = line
            .strip_prefix("old mode ")
            .or_else(|| line.strip_prefix("new mode "))
        {
            self.mode_change = true;
            self.modes.push(mode.to_string());
        } else if let Some(rest) = line.strip_prefix("index ") {
            if let Some(mode) = rest.split(' ').nth(1) {
                self.modes.push(mode.to_string());
            }
        } else if (line.starts_with("Binary files ") && line.ends_with(" differ"))
            || line == "GIT binary patch"
        {
            self.binary = true;
        }
    }

    /// The path that the section changes: the new name of a rename, else the
    /// one name that both sides of the `diff --git` line hold.
    fn new_path(&self) -> Result<String, String> {
        match &self.renamed_to {
            Some(name) => Ok(name.clone()),
            None => same_path(&self.header).ok_or_else(|| {
                format!(
                    "git diff printed a header that houserules cannot read: diff --git {}",
                    self.header
                )
            }),
        }
    }
}

/// The sections of `patch`.
fn parse_patch(patch: &str) -> Result<Vec<Section>, String> {
    let mut sections: Vec<Section> = Vec::new();
    let mut in_hunks = false;
    for line in patch.split('\n') {
        if let Some(header) = line.strip_prefix("diff --git ") {
            sections.push(Section {
                header: header.to_string(),
                ..Section::default()
            });
            in_hunks = false;
        } else if let Some(section) = sections.last_mut() {
            if line.starts_with("@@ ") {
                in_hunks = true;
                section.hunks.push(parse_hunk(line)?);
            } else if !in_hunks {
                section.read_header_line(line);
            }
        }
    }
    Ok(sections)
}

/// The hunk that the header line `line` describes.
fn parse_hunk(line: &str) -> Result<Hunk, String> {
    let unreadable =
        || format!("git diff printed a hunk header that houserules cannot read: {line}");
    let rest = line.strip_prefix("@@ -").ok_or_else(unreadable)?;
    let (old, rest) = rest.split_once(" +").ok_or_else(unreadable)?;
    let (new, _) = rest.split_once(" @@").ok_or_else(unreadable)?;
    let (old_start, old_count) = parse_span(old).ok_or_else(unreadable)?;
    let (new_start, new_count) = parse_span(new).ok_or_else(unreadable)?;
    Ok(Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
    })
}

/// The start and the count of `<start>[,<count>]`; the count is 1 when absent.
fn parse_span(text: &str) -> Option<(u64, u64)> {
    match text.split_once(',') {
        Some((start, count)) => Some((start.parse().ok()?, count.parse().ok()?)),
        None => Some((text.parse().ok()?, 1)),
    }
}

/// The name in `text`, which git wrote C-quoted or as it is.
fn read_name(text: &str) -> String {
    unquote(text).map_or_else(|| text.to_string(), |(name, _)| name)
}

/// Reads the C-quoted name at the start of `text` and gives it with the text
/// after the closing quote. `None` when `text` does not start with a quote or
/// the quoted name does not end.
fn unquote(text: &str) -> Option<(String, &str)> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut name = Vec::new();
    let mut index = 1;
    loop {
        match *bytes.get(index)? {
            b'"' => break,
            b'\\' => {
                index += 1;
                let escaped = *bytes.get(index)?;
                match escaped {
                    b'a' => name.push(7),
                    b'b' => name.push(8),
                    b'f' => name.push(12),
                    b'n' => name.push(b'\n'),
                    b'r' => name.push(b'\r'),
                    b't' => name.push(b'\t'),
                    b'v' => name.push(11),
                    b'\\' | b'"' => name.push(escaped),
                    b'0'..=b'7' => {
                        let mut value = u32::from(escaped - b'0');
                        for _ in 0..2 {
                            match bytes.get(index + 1) {
                                Some(digit @ b'0'..=b'7') => {
                                    value = value * 8 + u32::from(digit - b'0');
                                    index += 1;
                                }
                                _ => break,
                            }
                        }
                        name.push(u8::try_from(value).ok()?);
                    }
                    _ => return None,
                }
            }
            byte => name.push(byte),
        }
        index += 1;
    }
    Some((
        String::from_utf8_lossy(&name).into_owned(),
        &text[index + 1..],
    ))
}

/// The one path that `a/<path> b/<path>` names, quoted or not. The two names
/// of a header are equal when the section holds no rename line, so an unquoted
/// header splits at the middle: its length is twice the path plus five.
fn same_path(header: &str) -> Option<String> {
    if header.starts_with('"') {
        let (old, rest) = unquote(header)?;
        let (new, _) = unquote(rest.strip_prefix(' ')?)?;
        let old = old.strip_prefix("a/")?;
        return (old == new.strip_prefix("b/")?).then(|| old.to_string());
    }
    let rest = header.len().checked_sub(5)?;
    if rest % 2 != 0 {
        return None;
    }
    let path_len = rest / 2;
    let old = header.get(..path_len + 2)?;
    let separator = header.get(path_len + 2..path_len + 5)?;
    let new = header.get(path_len + 5..)?;
    (old.strip_prefix("a/")? == new && separator == " b/").then(|| new.to_string())
}

/// One row of `git diff -z --numstat`.
#[derive(Debug)]
struct Row {
    /// The lines added; `None` for a binary file.
    added: Option<u64>,
    /// The lines deleted; `None` for a binary file.
    deleted: Option<u64>,
    path: String,
    old_path: Option<String>,
}

/// The rows of the output of `git diff -z --numstat`.
fn parse_numstat(numstat: &[u8]) -> Result<Vec<Row>, String> {
    let text = String::from_utf8_lossy(numstat);
    let mut tokens = text.split('\0');
    let mut rows = Vec::new();
    while let Some(token) = tokens.next() {
        if token.is_empty() {
            continue;
        }
        let unreadable =
            || format!("git diff --numstat printed a row that houserules cannot read: {token:?}");
        let mut parts = token.splitn(3, '\t');
        let (Some(added), Some(deleted), Some(name)) = (parts.next(), parts.next(), parts.next())
        else {
            return Err(unreadable());
        };
        let count = |text: &str| -> Result<Option<u64>, String> {
            if text == "-" {
                Ok(None)
            } else {
                text.parse().map(Some).map_err(|_| unreadable())
            }
        };
        let (path, old_path) = if name.is_empty() {
            let old = tokens.next().ok_or_else(unreadable)?;
            let new = tokens.next().ok_or_else(unreadable)?;
            (new.to_string(), Some(old.to_string()))
        } else {
            (name.to_string(), None)
        };
        rows.push(Row {
            added: count(added)?,
            deleted: count(deleted)?,
            path,
            old_path,
        });
    }
    Ok(rows)
}

/// Pairs the text of `git diff -U0` with the output of `git diff -z --numstat`.
fn combine(patch: &str, numstat: &[u8], prefix: &str) -> Result<Vec<ChangedPath>, String> {
    let disagreement =
        |path: &str| format!("git diff and git diff --numstat disagree about {path:?}");
    let mut groups: IndexMap<String, Vec<Section>> = IndexMap::new();
    for section in parse_patch(patch)? {
        groups.entry(section.new_path()?).or_default().push(section);
    }
    let mut rows: IndexMap<String, Row> = IndexMap::new();
    for row in parse_numstat(numstat)? {
        let path = row.path.clone();
        if rows.insert(path.clone(), row).is_some() {
            return Err(disagreement(&path));
        }
    }
    let mut paths = Vec::with_capacity(groups.len());
    for (path, sections) in groups {
        let row = rows.swap_remove(&path).ok_or_else(|| disagreement(&path))?;
        paths.push(changed_path(&path, &sections, &row, prefix)?);
    }
    if let Some(path) = rows.keys().next() {
        return Err(disagreement(path));
    }
    paths.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(paths)
}

/// The path `path` below the install `prefix`.
fn below(prefix: &str, path: &str) -> Result<String, String> {
    path.strip_prefix(prefix)
        .map(str::to_string)
        .ok_or_else(|| {
            format!("git diff printed {path:?}, which is outside the install {prefix:?}")
        })
}

/// The changed path that the `sections` and the numstat `row` of `path` make.
/// Only a text, a deleted, and a symlink path add to the diff size; the other
/// kinds keep 0 for both counts.
fn changed_path(
    path: &str,
    sections: &[Section],
    row: &Row,
    prefix: &str,
) -> Result<ChangedPath, String> {
    let kind = kind_of(path, sections)?;
    let counted = matches!(
        kind,
        ChangeKind::Text | ChangeKind::Deleted | ChangeKind::Symlink
    );
    let (added, deleted) = if counted {
        (row.added.unwrap_or(0), row.deleted.unwrap_or(0))
    } else {
        (0, 0)
    };
    Ok(ChangedPath {
        path: below(prefix, path)?,
        old_path: row
            .old_path
            .as_deref()
            .map(|old| below(prefix, old))
            .transpose()?,
        kind,
        added,
        deleted,
        ranges: ranges_of(kind, sections),
        excluded: false,
        empty: empty_change_of(kind, sections),
    })
}

/// Whether the section of an `empty` path adds or deletes the file; `None`
/// for every other kind.
fn empty_change_of(kind: ChangeKind, sections: &[Section]) -> Option<EmptyChange> {
    if kind != ChangeKind::Empty {
        return None;
    }
    Some(if sections[0].deleted_file {
        EmptyChange::Deleted
    } else {
        EmptyChange::Added
    })
}

/// The kind of the path whose `sections` the patch holds (spec 4.2). A
/// submodule or a symlink on either side decides the kind, then a binary file;
/// the other kinds follow the header lines of the one section.
fn kind_of(path: &str, sections: &[Section]) -> Result<ChangeKind, String> {
    let has_mode = |wanted: &str| {
        sections
            .iter()
            .any(|section| section.modes.iter().any(|mode| mode == wanted))
    };
    if has_mode("160000") {
        return Ok(ChangeKind::Submodule);
    }
    if has_mode("120000") {
        return Ok(ChangeKind::Symlink);
    }
    if sections.iter().any(|section| section.binary) {
        return Ok(ChangeKind::Binary);
    }
    let section = &sections[0];
    if !section.hunks.is_empty() {
        Ok(if section.deleted_file {
            ChangeKind::Deleted
        } else {
            ChangeKind::Text
        })
    } else if section.renamed_to.is_some() {
        Ok(ChangeKind::Renamed)
    } else if section.new_file || section.deleted_file {
        Ok(ChangeKind::Empty)
    } else if section.mode_change {
        Ok(ChangeKind::Mode)
    } else {
        Err(format!(
            "git diff printed a change to {path:?} that houserules cannot classify"
        ))
    }
}

/// The ranges of the hunks of a path of `kind`: the lines of the old file for
/// a deleted file, the lines of the new file for a text or a symlink path, and
/// none for the other kinds.
fn ranges_of(kind: ChangeKind, sections: &[Section]) -> Vec<LineRange> {
    let hunks = sections.iter().flat_map(|section| &section.hunks);
    match kind {
        ChangeKind::Deleted => hunks
            .map(|hunk| LineRange::Lines {
                start: hunk.old_start,
                end: (hunk.old_start + hunk.old_count).saturating_sub(1),
            })
            .collect(),
        ChangeKind::Text | ChangeKind::Symlink => hunks
            .map(|hunk| {
                if hunk.new_count == 0 {
                    LineRange::DeletedAfter {
                        after: hunk.new_start,
                        deleted: hunk.old_count,
                    }
                } else {
                    LineRange::Lines {
                        start: hunk.new_start,
                        end: hunk.new_start + hunk.new_count - 1,
                    }
                }
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::git::{self, scratch, scratch::Repo};
    use super::*;
    use crate::published::change::{ChangeKind, EmptyChange, LineRange};

    /// The expected path of a test: not excluded, no old path.
    fn expected(
        kind: ChangeKind,
        path: &str,
        added: u64,
        deleted: u64,
        ranges: Vec<LineRange>,
    ) -> ChangedPath {
        ChangedPath {
            path: path.to_string(),
            old_path: None,
            kind,
            added,
            deleted,
            ranges,
            excluded: false,
            empty: None,
        }
    }

    /// The paths of the range `from` to `to` of `repo`, for an install at the
    /// top.
    fn read(repo: &Repo, from: &str, to: &str) -> Vec<ChangedPath> {
        let empty = git::empty_tree(repo.root()).expect("the empty tree");
        read_range(repo.root(), &empty, from, to, "").expect("read the range")
    }

    /// The one path of a range that changes one path.
    fn only(mut paths: Vec<ChangedPath>) -> ChangedPath {
        assert_eq!(paths.len(), 1, "{paths:?}");
        paths.remove(0)
    }

    #[test]
    fn an_added_file_is_text_with_all_its_lines() {
        let repo = Repo::new();
        repo.write("keep.txt", "keep\n");
        let base = repo.commit("base");
        repo.write("added.txt", "one\ntwo\n");
        let head = repo.commit("added");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(
                ChangeKind::Text,
                "added.txt",
                2,
                0,
                vec![LineRange::Lines { start: 1, end: 2 }]
            )
        );
    }

    #[test]
    fn an_edited_file_lists_the_new_side_of_each_hunk() {
        let repo = Repo::new();
        repo.write("f.txt", "a\nb\nc\n");
        let base = repo.commit("base");
        repo.write("f.txt", "a\nB\nc\nd\ne\n");
        let head = repo.commit("edit");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(
                ChangeKind::Text,
                "f.txt",
                3,
                1,
                vec![
                    LineRange::Lines { start: 2, end: 2 },
                    LineRange::Lines { start: 4, end: 5 }
                ]
            )
        );
    }

    #[test]
    fn a_hunk_that_only_deletes_lines_names_the_line_it_follows() {
        let repo = Repo::new();
        repo.write("f.txt", "1\n2\n3\n4\n5\n");
        let base = repo.commit("base");
        repo.write("f.txt", "1\n2\n5\n");
        let middle = repo.commit("delete in the middle");
        repo.write("f.txt", "5\n");
        let head = repo.commit("delete at the top");

        assert_eq!(
            only(read(&repo, &base, &middle)),
            expected(
                ChangeKind::Text,
                "f.txt",
                0,
                2,
                vec![LineRange::DeletedAfter {
                    after: 2,
                    deleted: 2
                }]
            )
        );
        assert_eq!(
            only(read(&repo, &middle, &head)),
            expected(
                ChangeKind::Text,
                "f.txt",
                0,
                2,
                vec![LineRange::DeletedAfter {
                    after: 0,
                    deleted: 2
                }]
            )
        );
    }

    #[test]
    fn a_deleted_file_lists_the_lines_of_the_old_file() {
        let repo = Repo::new();
        repo.write("gone.txt", "x\ny\n");
        repo.write("keep.txt", "keep\n");
        let base = repo.commit("base");
        std::fs::remove_file(repo.root().join("gone.txt")).expect("delete the file");
        let head = repo.commit("delete");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(
                ChangeKind::Deleted,
                "gone.txt",
                0,
                2,
                vec![LineRange::Lines { start: 1, end: 2 }]
            )
        );
    }

    #[test]
    fn a_pure_rename_has_no_line() {
        let repo = Repo::new();
        repo.write("old.txt", "one\ntwo\n");
        let base = repo.commit("base");
        repo.git(&["mv", "old.txt", "new.txt"]);
        let head = repo.commit("rename");

        let mut renamed = expected(ChangeKind::Renamed, "new.txt", 0, 0, Vec::new());
        renamed.old_path = Some("old.txt".to_string());
        assert_eq!(only(read(&repo, &base, &head)), renamed);
    }

    #[test]
    fn a_rename_with_edits_is_text_under_its_new_path() {
        let repo = Repo::new();
        repo.write("old.txt", "one\ntwo\nthree\nfour\n");
        let base = repo.commit("base");
        repo.git(&["mv", "old.txt", "new.txt"]);
        repo.write("new.txt", "one\ntwo\nTHREE\nfour\n");
        let head = repo.commit("rename and edit");

        let mut renamed = expected(
            ChangeKind::Text,
            "new.txt",
            1,
            1,
            vec![LineRange::Lines { start: 3, end: 3 }],
        );
        renamed.old_path = Some("old.txt".to_string());
        assert_eq!(only(read(&repo, &base, &head)), renamed);
    }

    #[test]
    fn an_empty_file_added_or_deleted_is_empty() {
        let repo = Repo::new();
        repo.write("keep.txt", "keep\n");
        let base = repo.commit("base");
        repo.write("blank.txt", "");
        let added = repo.commit("add an empty file");
        std::fs::remove_file(repo.root().join("blank.txt")).expect("delete the file");
        let deleted = repo.commit("delete it");

        assert_eq!(
            only(read(&repo, &base, &added)),
            ChangedPath {
                empty: Some(EmptyChange::Added),
                ..expected(ChangeKind::Empty, "blank.txt", 0, 0, Vec::new())
            }
        );
        assert_eq!(
            only(read(&repo, &added, &deleted)),
            ChangedPath {
                empty: Some(EmptyChange::Deleted),
                ..expected(ChangeKind::Empty, "blank.txt", 0, 0, Vec::new())
            }
        );
    }

    #[test]
    fn a_binary_file_has_no_line() {
        let repo = Repo::new();
        repo.write_bytes("b.bin", b"bin\0x");
        let base = repo.commit("base");
        repo.write_bytes("b.bin", b"bin\0y");
        let head = repo.commit("edit");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(ChangeKind::Binary, "b.bin", 0, 0, Vec::new())
        );
    }

    #[test]
    fn a_change_of_the_mode_alone_has_no_line() {
        let repo = Repo::new();
        repo.git(&["config", "core.fileMode", "false"]);
        repo.write("run.sh", "echo\n");
        let base = repo.commit("base");
        repo.git(&["update-index", "--chmod=+x", "run.sh"]);
        let head = repo.commit("make it executable");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(ChangeKind::Mode, "run.sh", 0, 0, Vec::new())
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_counts_its_target_line_on_each_side() {
        // Symlinks need the `unix` platform: Windows creates them only with a privilege.
        let repo = Repo::new();
        std::os::unix::fs::symlink("f.txt", repo.root().join("link")).expect("create the link");
        let base = repo.commit("base");
        std::fs::remove_file(repo.root().join("link")).expect("delete the link");
        std::os::unix::fs::symlink("other", repo.root().join("link")).expect("create the link");
        let head = repo.commit("retarget");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(
                ChangeKind::Symlink,
                "link",
                1,
                1,
                vec![LineRange::Lines { start: 1, end: 1 }]
            )
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_becomes_a_symlink_is_one_symlink_path() {
        // Symlinks need the `unix` platform: Windows creates them only with a privilege.
        let repo = Repo::new();
        repo.write("f.txt", "a\nb\nc\nd\n");
        let base = repo.commit("base");
        std::fs::remove_file(repo.root().join("f.txt")).expect("delete the file");
        std::os::unix::fs::symlink("zzz", repo.root().join("f.txt")).expect("create the link");
        let head = repo.commit("type change");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(
                ChangeKind::Symlink,
                "f.txt",
                1,
                4,
                vec![
                    LineRange::DeletedAfter {
                        after: 0,
                        deleted: 4
                    },
                    LineRange::Lines { start: 1, end: 1 }
                ]
            )
        );
    }

    #[test]
    fn a_submodule_has_no_line() {
        let repo = Repo::new();
        repo.write("keep.txt", "keep\n");
        let first = "1111111111111111111111111111111111111111";
        let second = "2222222222222222222222222222222222222222";
        repo.git(&["add", "keep.txt"]);
        repo.git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{first},vendor"),
        ]);
        let base = repo.commit_staged("base");
        repo.git(&[
            "update-index",
            "--cacheinfo",
            &format!("160000,{second},vendor"),
        ]);
        let head = repo.commit_staged("bump");

        assert_eq!(
            only(read(&repo, &base, &head)),
            expected(ChangeKind::Submodule, "vendor", 0, 0, Vec::new())
        );
    }

    #[test]
    fn the_no_newline_marker_is_no_line() {
        let repo = Repo::new();
        repo.write("old.txt", "x\ny");
        repo.write("new.txt", "a");
        let base = repo.commit("base");
        repo.write("old.txt", "x\ny\nz\n");
        repo.write("new.txt", "b");
        let head = repo.commit("edit");

        let paths = read(&repo, &base, &head);

        assert_eq!(
            paths,
            vec![
                expected(
                    ChangeKind::Text,
                    "new.txt",
                    1,
                    1,
                    vec![LineRange::Lines { start: 1, end: 1 }]
                ),
                expected(
                    ChangeKind::Text,
                    "old.txt",
                    2,
                    1,
                    vec![LineRange::Lines { start: 2, end: 3 }]
                ),
            ]
        );
    }

    #[test]
    fn a_path_with_a_space_or_a_non_ascii_character_pairs_its_hunks() {
        let repo = Repo::new();
        repo.write("keep.txt", "keep\n");
        let base = repo.commit("base");
        repo.write("sp ace.txt", "one\n");
        repo.write("caf\u{e9}.txt", "two\ntwo\n");
        repo.write("end .txt", "three\n");
        let head = repo.commit("odd names");

        let paths = read(&repo, &base, &head);

        assert_eq!(
            paths
                .iter()
                .map(|path| (path.path.as_str(), path.added))
                .collect::<Vec<_>>(),
            [("caf\u{e9}.txt", 2), ("end .txt", 1), ("sp ace.txt", 1)]
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_path_with_a_newline_pairs_its_hunks() {
        // A file name with a newline, a tab, a quote, or a backslash needs a
        // file system that allows it: Windows does not.
        let repo = Repo::new();
        repo.write("keep.txt", "keep\n");
        let base = repo.commit("base");
        for (name, text) in [
            ("we\nird.txt", "1\n"),
            ("ta\tb.txt", "2\n2\n"),
            ("qu\"ote.txt", "3\n3\n3\n"),
            ("back\\slash.txt", "4\n"),
        ] {
            repo.write(name, text);
        }
        let head = repo.commit("hostile names");

        let paths = read(&repo, &base, &head);

        assert_eq!(
            paths
                .iter()
                .map(|path| (path.path.as_str(), path.added))
                .collect::<Vec<_>>(),
            [
                ("back\\slash.txt", 1),
                ("qu\"ote.txt", 3),
                ("ta\tb.txt", 2),
                ("we\nird.txt", 1)
            ]
        );
        assert!(paths.iter().all(|path| path.ranges
            == [LineRange::Lines {
                start: 1,
                end: path.added
            }]));
    }

    #[test]
    fn the_configuration_and_the_attributes_of_the_request_change_nothing() {
        let repo = Repo::new();
        repo.git(&["config", "diff.noprefix", "true"]);
        repo.git(&["config", "diff.relative", "true"]);
        repo.git(&["config", "diff.mnemonicPrefix", "true"]);
        repo.write("sub/keep.txt", "keep\n");
        let base = repo.commit("base");
        repo.write(".gitattributes", "*.rs -diff\n");
        repo.write("sub/a.rs", "fn a() {}\nfn b() {}\n");
        repo.write("top.rs", "fn c() {}\n");
        let head = repo.commit("rust");
        let empty = git::empty_tree(repo.root()).expect("the empty tree");

        let inside = read_range(&repo.root().join("sub"), &empty, &base, &head, "sub/")
            .expect("read the range");
        let top = read_range(repo.root(), &empty, &base, &head, "").expect("read the range");

        assert_eq!(
            inside,
            vec![expected(
                ChangeKind::Text,
                "a.rs",
                2,
                0,
                vec![LineRange::Lines { start: 1, end: 2 }]
            )]
        );
        assert_eq!(
            top.iter()
                .map(|path| path.path.as_str())
                .collect::<Vec<_>>(),
            [".gitattributes", "sub/a.rs", "top.rs"]
        );
        assert!(top.iter().all(|path| path.kind == ChangeKind::Text));
    }

    /// The range read plain, and again after `settings` entered the
    /// configuration of `repo`.
    fn read_plain_and_configured(
        repo: &Repo,
        base: &str,
        head: &str,
        settings: &[(&str, &str)],
    ) -> (Vec<ChangedPath>, Vec<ChangedPath>) {
        let plain = read(repo, base, head);
        for (key, value) in settings {
            repo.git(&["config", key, value]);
        }
        (plain, read(repo, base, head))
    }

    /// A repository with the install files `sub/x.txt` and `SUB/y.txt` (the
    /// second is seeded through the index, because a case-insensitive file
    /// system holds one of the two folders) and an edit of both.
    fn two_folders_that_differ_in_case() -> (Repo, String, String) {
        let repo = Repo::new();
        repo.write("sub/x.txt", "1\n2\n");
        repo.git(&["add", "-A"]);
        let blob = repo.git(&["rev-parse", ":sub/x.txt"]).trim().to_string();
        repo.git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},SUB/y.txt"),
        ]);
        let base = repo.commit_staged("base");
        repo.write("sub/x.txt", "1\n3\n");
        repo.git(&["add", "-A"]);
        repo.git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},SUB/z.txt"),
        ]);
        let head = repo.commit_staged("edit");
        (repo, base, head)
    }

    /// The test that the tests of an environment variable run again as a
    /// child process (`git::scratch::rerun_test`): it prints the debug text of
    /// the range that the variables `HR_RERUN_*` name, and does nothing when
    /// they are absent.
    #[test]
    fn rerun_prints_the_range_of_its_environment() {
        let Some(root) = std::env::var_os("HR_RERUN_ROOT") else {
            return;
        };
        let text = |name: &str| std::env::var(name).expect(name);
        let empty = git::empty_tree(Path::new(&root)).expect("the empty tree");
        let paths = read_range(
            Path::new(&root),
            &empty,
            &text("HR_RERUN_BASE"),
            &text("HR_RERUN_HEAD"),
            &text("HR_RERUN_PREFIX"),
        )
        .expect("read the range");
        println!("RANGE {paths:?}");
    }

    /// The range of `repo` that a child process reads with `envs` in its
    /// environment, as debug text.
    fn read_with_environment(
        repo: &Repo,
        base: &str,
        head: &str,
        install: &str,
        envs: &[(&str, &str)],
    ) -> String {
        let root = repo.root().join(install);
        let prefix = if install.is_empty() {
            String::new()
        } else {
            format!("{install}/")
        };
        let root = root.to_str().expect("a UTF-8 path").to_string();
        let mut all = vec![
            ("HR_RERUN_ROOT", root.as_str()),
            ("HR_RERUN_BASE", base),
            ("HR_RERUN_HEAD", head),
            ("HR_RERUN_PREFIX", prefix.as_str()),
        ];
        all.extend_from_slice(envs);
        let printed = scratch::rerun_test(
            "review::diff::tests::rerun_prints_the_range_of_its_environment",
            &all,
        );
        printed
            .lines()
            .find_map(|line| line.split_once("RANGE ").map(|(_, range)| range))
            .expect("the child printed the range")
            .to_string()
    }

    #[test]
    fn a_configured_diff_algorithm_changes_neither_the_counts_nor_the_ranges() {
        let repo = Repo::new();
        repo.write("a.txt", "{\ne\na\nb\n}\n}\ne\na\ne\ne\nd\na\n");
        let base = repo.commit("base");
        repo.write("a.txt", "b\na\ne\n{\nb\nc\nd\nb\ne\na\ne\nc\n");
        let head = repo.commit("edit");

        let (plain, patience) =
            read_plain_and_configured(&repo, &base, &head, &[("diff.algorithm", "patience")]);

        let text = only(plain.clone());
        assert_eq!((text.added, text.deleted), (7, 7));
        assert_eq!(patience, plain);
    }

    #[test]
    fn a_configured_inter_hunk_context_keeps_the_hunks_apart() {
        let repo = Repo::new();
        repo.write("f.txt", "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n");
        let base = repo.commit("base");
        repo.write("f.txt", "1\nX\n3\n4\n5\n6\n7\n8\nY\n10\n");
        let head = repo.commit("edit");

        let (plain, merged) =
            read_plain_and_configured(&repo, &base, &head, &[("diff.interHunkContext", "10")]);

        assert_eq!(
            only(plain.clone()).ranges,
            [
                LineRange::Lines { start: 2, end: 2 },
                LineRange::Lines { start: 9, end: 9 }
            ]
        );
        assert_eq!(merged, plain);
    }

    #[test]
    fn a_configured_indent_heuristic_keeps_the_hunk_where_the_default_puts_it() {
        let repo = Repo::new();
        repo.write("i.txt", "\n}\n    a\n}\n");
        let base = repo.commit("base");
        repo.write("i.txt", "\n}\nx\n    b\n    a\n}\n    a\n}\n");
        let head = repo.commit("edit");

        let (plain, unshifted) =
            read_plain_and_configured(&repo, &base, &head, &[("diff.indentHeuristic", "false")]);

        assert_eq!(
            only(plain.clone()).ranges,
            [LineRange::Lines { start: 2, end: 5 }]
        );
        assert_eq!(unshifted, plain);
    }

    #[test]
    fn a_configured_rename_limit_keeps_the_renames_of_the_default_limit() {
        let repo = Repo::new();
        repo.write(
            "old1.txt",
            "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\n",
        );
        repo.write(
            "old2.txt",
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n",
        );
        let base = repo.commit("base");
        repo.git(&["mv", "old1.txt", "new1.txt"]);
        repo.git(&["mv", "old2.txt", "new2.txt"]);
        repo.write(
            "new1.txt",
            "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\nTHETA\n",
        );
        repo.write(
            "new2.txt",
            "one\ntwo\nthree\nfour\nfive\nsix\nseven\nEIGHT\n",
        );
        let head = repo.commit("rename with edits");

        let (plain, limited) =
            read_plain_and_configured(&repo, &base, &head, &[("diff.renameLimit", "1")]);

        assert_eq!(
            plain
                .iter()
                .map(|path| path.old_path.as_deref())
                .collect::<Vec<_>>(),
            [Some("old1.txt"), Some("old2.txt")]
        );
        assert_eq!(limited, plain);
    }

    /// A repository whose range bumps the submodule `vendor`, which
    /// `.gitmodules` names.
    fn submodule_bump() -> (Repo, String, String) {
        let repo = Repo::new();
        repo.write("keep.txt", "keep\n");
        repo.write(
            ".gitmodules",
            "[submodule \"vendor\"]\n\tpath = vendor\n\turl = ./nowhere\n",
        );
        repo.git(&["add", "keep.txt", ".gitmodules"]);
        repo.git(&[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,1111111111111111111111111111111111111111,vendor",
        ]);
        let base = repo.commit_staged("base");
        repo.git(&[
            "update-index",
            "--cacheinfo",
            "160000,2222222222222222222222222222222222222222,vendor",
        ]);
        let head = repo.commit_staged("bump");
        (repo, base, head)
    }

    #[test]
    fn a_configured_ignore_of_submodules_keeps_the_submodule_in_the_range() {
        let (repo, base, head) = submodule_bump();

        let (plain, ignored) =
            read_plain_and_configured(&repo, &base, &head, &[("diff.ignoreSubmodules", "all")]);

        assert_eq!(only(plain.clone()).kind, ChangeKind::Submodule);
        assert_eq!(ignored, plain);
    }

    #[test]
    fn the_ignore_setting_of_one_submodule_keeps_it_in_the_range() {
        let (repo, base, head) = submodule_bump();

        let (plain, ignored) =
            read_plain_and_configured(&repo, &base, &head, &[("submodule.vendor.ignore", "all")]);

        assert_eq!(only(plain.clone()).kind, ChangeKind::Submodule);
        assert_eq!(ignored, plain);
    }

    #[test]
    fn a_configured_submodule_format_keeps_the_short_form() {
        let (repo, base, head) = submodule_bump();
        let plain = read(&repo, &base, &head);

        for format in ["log", "diff"] {
            repo.git(&["config", "diff.submodule", format]);

            assert_eq!(read(&repo, &base, &head), plain, "diff.submodule={format}");
        }
        assert_eq!(only(plain).kind, ChangeKind::Submodule);
    }

    #[test]
    fn a_configured_big_file_threshold_keeps_a_big_file_text() {
        let repo = Repo::new();
        repo.write("big.txt", &"b".repeat(100));
        let base = repo.commit("base");
        repo.write("big.txt", &"c".repeat(100));
        let head = repo.commit("edit");

        let (plain, small) =
            read_plain_and_configured(&repo, &base, &head, &[("core.bigFileThreshold", "10")]);

        assert_eq!(only(plain.clone()).kind, ChangeKind::Text);
        assert_eq!(small, plain);
    }

    #[test]
    fn a_global_attributes_file_makes_no_path_binary() {
        let repo = Repo::new();
        let folder = tempfile::tempdir().expect("tempdir");
        let attributes = folder.path().join("attributes");
        std::fs::write(&attributes, "*.rs -diff\n").expect("write the attributes");
        repo.write("a.rs", "fn a() {}\n");
        let base = repo.commit("base");
        repo.write("a.rs", "fn a() {}\nfn b() {}\n");
        let head = repo.commit("edit");

        let (plain, global) = read_plain_and_configured(
            &repo,
            &base,
            &head,
            &[(
                "core.attributesFile",
                attributes.to_str().expect("a UTF-8 path"),
            )],
        );

        assert_eq!(only(plain.clone()).kind, ChangeKind::Text);
        assert_eq!(global, plain);
    }

    #[test]
    fn the_info_attributes_file_of_the_repository_is_an_input_of_a_local_run() {
        // design.md 255: no git option turns off $GIT_DIR/info/attributes, so a
        // local run reads it and a CI clone, which holds none, does not.
        let repo = Repo::new();
        repo.write("a.rs", "fn a() {}\n");
        let base = repo.commit("base");
        repo.write("a.rs", "fn a() {}\nfn b() {}\n");
        let head = repo.commit("edit");
        assert_eq!(only(read(&repo, &base, &head)).kind, ChangeKind::Text);
        let info = repo.root().join(".git").join("info");
        std::fs::create_dir_all(&info).expect("create the info folder");
        std::fs::write(info.join("attributes"), "*.rs -diff\n").expect("write the attributes");

        assert_eq!(only(read(&repo, &base, &head)).kind, ChangeKind::Binary);
    }

    #[test]
    fn a_replace_ref_changes_nothing_in_the_range() {
        let repo = Repo::new();
        repo.write("a.rs", "fn a() {}\n");
        let base = repo.commit("base");
        repo.write("a.rs", "fn a() {}\nfn b() {}\n");
        let head = repo.commit("edit");
        let old = repo
            .git(&["rev-parse", &format!("{head}:a.rs")])
            .trim()
            .to_string();
        let plain = read(&repo, &base, &head);
        repo.write("replacement.txt", "replaced\n");
        let new = repo
            .git(&["hash-object", "-w", "replacement.txt"])
            .trim()
            .to_string();
        repo.git(&["replace", &old, &new]);

        assert_eq!(read(&repo, &base, &head), plain);
        assert_eq!(only(plain).added, 1);
    }

    #[test]
    fn git_diff_opts_in_the_environment_changes_nothing() {
        let repo = Repo::new();
        repo.write("f.txt", "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n");
        let base = repo.commit("base");
        repo.write("f.txt", "1\nX\n3\n4\n5\n6\n7\n8\nY\n10\n");
        let head = repo.commit("edit");
        let plain = format!("{:?}", read(&repo, &base, &head));

        for opts in ["-u5", "--unified=5"] {
            assert_eq!(
                read_with_environment(&repo, &base, &head, "", &[("GIT_DIFF_OPTS", opts)]),
                plain,
                "GIT_DIFF_OPTS={opts}"
            );
        }
    }

    #[test]
    fn global_pathspec_settings_in_the_environment_change_nothing() {
        let (repo, base, head) = two_folders_that_differ_in_case();
        let plain = read_with_environment(&repo, &base, &head, "sub", &[]);
        assert!(plain.contains("x.txt"), "{plain}");

        for name in ["GIT_ICASE_PATHSPECS", "GIT_LITERAL_PATHSPECS"] {
            assert_eq!(
                read_with_environment(&repo, &base, &head, "sub", &[(name, "1")]),
                plain,
                "{name}=1"
            );
        }
    }

    #[test]
    fn a_prefix_limits_the_range_to_the_install_and_never_pairs_a_rename_across_it() {
        let repo = Repo::new();
        repo.write("other/moved.txt", "one\ntwo\n");
        repo.write("sub/keep.txt", "keep\n");
        let base = repo.commit("base");
        repo.git(&["mv", "other/moved.txt", "sub/moved.txt"]);
        repo.write("other/outside.txt", "outside\n");
        let head = repo.commit("move into the install");
        let empty = git::empty_tree(repo.root()).expect("the empty tree");

        let paths = read_range(&repo.root().join("sub"), &empty, &base, &head, "sub/")
            .expect("read the range");

        assert_eq!(
            paths,
            vec![expected(
                ChangeKind::Text,
                "moved.txt",
                2,
                0,
                vec![LineRange::Lines { start: 1, end: 2 }]
            )]
        );
    }

    #[test]
    fn a_range_with_no_change_has_no_path() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        let head = repo.commit("first");

        assert_eq!(read(&repo, &head, &head), Vec::new());
    }

    #[test]
    fn the_empty_tree_as_the_start_lists_every_file_as_added() {
        let repo = Repo::new();
        repo.write("a.txt", "one\n");
        repo.write("dir/b.txt", "two\ntwo\n");
        let head = repo.commit("first");
        let empty = git::empty_tree(repo.root()).expect("the empty tree");

        let paths = read(&repo, &empty, &head);

        assert_eq!(
            paths,
            vec![
                expected(
                    ChangeKind::Text,
                    "a.txt",
                    1,
                    0,
                    vec![LineRange::Lines { start: 1, end: 1 }]
                ),
                expected(
                    ChangeKind::Text,
                    "dir/b.txt",
                    2,
                    0,
                    vec![LineRange::Lines { start: 1, end: 2 }]
                ),
            ]
        );
    }

    /// The patch and the numstat of a change to `a.txt` (two lines in, one
    /// out) and to `b.txt` (one line in).
    const PATCH: &str = "diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n-x\n+y\n+z\ndiff --git a/b.txt b/b.txt\nindex 1..2 100644\n--- a/b.txt\n+++ b/b.txt\n@@ -3,0 +4 @@\n+w\n";

    #[test]
    fn the_patch_and_the_numstat_pair_up_by_path_whatever_their_order() {
        let both = [&b"1\t0\tb.txt\0"[..], &b"2\t1\ta.txt\0"[..]].concat();

        let paths = combine(PATCH, &both, "").expect("pair them");

        assert_eq!(
            paths,
            vec![
                expected(
                    ChangeKind::Text,
                    "a.txt",
                    2,
                    1,
                    vec![LineRange::Lines { start: 1, end: 2 }]
                ),
                expected(
                    ChangeKind::Text,
                    "b.txt",
                    1,
                    0,
                    vec![LineRange::Lines { start: 4, end: 4 }]
                ),
            ]
        );
    }

    #[test]
    fn a_section_with_no_numstat_row_is_a_named_disagreement() {
        let error = combine(PATCH, b"2\t1\ta.txt\0", "").expect_err("b.txt has no row");

        assert_eq!(
            error,
            "git diff and git diff --numstat disagree about \"b.txt\""
        );
    }

    #[test]
    fn a_numstat_row_with_no_section_is_a_named_disagreement() {
        let rows = b"2\t1\ta.txt\0\
                     1\t0\tb.txt\0\
                     1\t1\tc.txt\0";
        let error = combine(PATCH, rows, "").expect_err("c.txt has no section");

        assert_eq!(
            error,
            "git diff and git diff --numstat disagree about \"c.txt\""
        );
    }

    #[test]
    fn a_quoted_name_is_read_with_its_escapes() {
        let patch = "diff --git \"a/we\\nird\\t\\\"x\\\\.txt\" \"b/we\\nird\\t\\\"x\\\\.txt\"\nnew file mode 100644\nindex 0000000..1\n--- /dev/null\n+++ \"b/we\\nird\\t\\\"x\\\\.txt\"\n@@ -0,0 +1 @@\n+q\n";

        let paths = combine(patch, b"1\t0\twe\nird\t\"x\\.txt\0", "").expect("pair them");

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].path, "we\nird\t\"x\\.txt");
    }

    #[test]
    fn a_name_in_octal_escapes_is_read_as_the_bytes_of_its_characters() {
        let patch = "diff --git \"a/caf\\303\\251.txt\" \"b/caf\\303\\251.txt\"\nnew file mode 100644\nindex 0000000..1\n--- /dev/null\n+++ \"b/caf\\303\\251.txt\"\n@@ -0,0 +1 @@\n+q\n";

        let paths = combine(patch, "1\t0\tcaf\u{e9}.txt\0".as_bytes(), "").expect("pair them");

        assert_eq!(paths[0].path, "caf\u{e9}.txt");
    }

    #[test]
    fn a_rename_takes_both_names_from_its_own_lines_even_when_they_hold_spaces() {
        let patch = "diff --git a/old name.txt b/new name.txt\nsimilarity index 100%\nrename from old name.txt\nrename to new name.txt\n";

        let paths = combine(patch, b"0\t0\t\0old name.txt\0new name.txt\0", "").expect("pair them");

        let mut renamed = expected(ChangeKind::Renamed, "new name.txt", 0, 0, Vec::new());
        renamed.old_path = Some("old name.txt".to_string());
        assert_eq!(paths, vec![renamed]);
    }

    #[test]
    fn a_path_outside_the_install_is_a_named_error() {
        let patch = "diff --git a/other/x.txt b/other/x.txt\nnew file mode 100644\nindex 0000000..1\n--- /dev/null\n+++ b/other/x.txt\n@@ -0,0 +1 @@\n+q\n";

        let error = combine(patch, b"1\t0\tother/x.txt\0", "sub/").expect_err("outside");

        assert_eq!(
            error,
            "git diff printed \"other/x.txt\", which is outside the install \"sub/\""
        );
    }

    #[test]
    fn a_hunk_header_that_cannot_be_read_is_a_named_error() {
        let patch = "diff --git a/x.txt b/x.txt\nindex 1..2 100644\n--- a/x.txt\n+++ b/x.txt\n@@ nonsense @@\n";

        let error = combine(patch, b"1\t0\tx.txt\0", "").expect_err("bad header");

        assert_eq!(
            error,
            "git diff printed a hunk header that houserules cannot read: @@ nonsense @@"
        );
    }

    #[test]
    fn a_change_with_no_hunk_and_no_header_form_is_a_named_error() {
        let patch = "diff --git a/x.txt b/x.txt\nindex 1..2 100644\n";

        let error = combine(patch, b"0\t0\tx.txt\0", "").expect_err("no form");

        assert_eq!(
            error,
            "git diff printed a change to \"x.txt\" that houserules cannot classify"
        );
    }
}
