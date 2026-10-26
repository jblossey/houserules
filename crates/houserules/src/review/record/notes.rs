//! The change records of an install in git notes (spec 8.1).
//!
//! A record is a note on its key, the last commit of a push to the default
//! branch, under the notes ref of the install: `refs/notes/houserules` for a
//! top-level install, and `refs/notes/houserules-sub/<install path>` for any
//! other. [`write`] stores the bytes of a note as they are; the CI units push
//! the ref. [`read`] finds the record that a commit keys, else the record whose
//! range holds the commit.
//!
//! A record lists its commits unless the budget cut the list (spec 8.4): a cut
//! record names its first and its last commit, and the minimal record names no
//! commit but its key, its base, and its head. Such a record holds the commits
//! that are ancestors of its head and no ancestors of its base (the range that
//! `record write` listed before the cut). git decides it with `merge-base
//! --is-ancestor`: one call tells that a commit does not reach the head, and a
//! second call, for the base, only follows when it does.
//!
//! A search costs one `git cat-file --batch` call for the blobs of all the
//! candidate notes, one `git rev-list` call that tells which heads and bases
//! the clone has, and at most two `merge-base` calls for each candidate. A
//! shallow clone drops the history older than its depth and never the newer, so
//! a record whose head the clone lacks holds no commit of the clone, and a
//! record whose base the clone lacks cannot tell whether a commit that reaches
//! its head is before its range (spec 8.3).
//!
//! A listing of the records reads no candidate: [`names`] lists every note with
//! the commit it annotates in one `git notes list` call, and [`read_named`] reads
//! the notes that stay in one `git cat-file --batch` call. [`read_bytes`] gives
//! the bytes of one note as they are, for a command that changes a note without
//! changing the bytes around the change.
//!
//! Every call goes through `review::git`, which starts `git` from the one
//! shared command. A note is written as a blob that git reads from its standard
//! input and attaches with `-C`, which keeps the bytes verbatim; `-F` and `-m`
//! clean up white space.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use super::super::git;
use crate::published::change_record::{ChangeRecord, Commits};

/// The text that every record holds: the id of its schema, the first key. A
/// note with no such text is no record, and the search for a range never reads
/// it.
const SCHEMA_TEXT: &str = r#""schema":"houserules/change-record@"#;

/// The pattern (an extended regular expression) of the key `commits` that holds
/// the full list of a record, which follows `merge_base` in the order of the
/// keys of spec 8.2. A record that a cut turned into a summary, and the minimal
/// record, lack it.
const LIST_OF_COMMITS: &str = r#""merge_base":(null|"[0-9a-f]+"),"commits":\["#;

/// The notes ref of the install `install`: the path of the install from the git
/// top level, `.` for a top-level install.
pub(crate) fn notes_ref(install: &str) -> String {
    if install == "." {
        "refs/notes/houserules".to_string()
    } else {
        format!("refs/notes/houserules-sub/{install}")
    }
}

/// Whether the notes ref of `install` exists in the repository at `root`.
pub(crate) fn ref_exists(root: &Path, install: &str) -> Result<bool, String> {
    let output = git::run_tolerating(
        root,
        &["rev-parse", "--verify", "--quiet", &notes_ref(install)],
        1,
    )?;
    Ok(!output.is_empty())
}

/// The record in the bytes `bytes`, which the note on `key` holds, or the one
/// line that names `key` and the fault of the bytes.
fn parse(key: &str, bytes: &[u8]) -> Result<ChangeRecord, String> {
    serde_json::from_slice(bytes)
        .map_err(|error| format!("record: {}: not a change record ({error})", git::short(key)))
}

/// The bytes of the note on `key`, or `None` when `key` has no note.
pub(crate) fn read_bytes(root: &Path, install: &str, key: &str) -> Result<Option<Vec<u8>>, String> {
    let listing = git::run_tolerating(
        root,
        &["notes", "--ref", &notes_ref(install), "list", key],
        1,
    )?;
    let blob = String::from_utf8_lossy(&listing).trim().to_string();
    if blob.is_empty() {
        return Ok(None);
    }
    git::run_bytes(root, &["cat-file", "blob", &blob]).map(Some)
}

/// The record that the note on `key` holds, or `None` when `key` has no note.
/// A commit that a record only lists has no note of its own: see [`read`].
pub(crate) fn read_keyed(
    root: &Path,
    install: &str,
    key: &str,
) -> Result<Option<ChangeRecord>, String> {
    read_bytes(root, install, key)?
        .map(|bytes| parse(key, &bytes))
        .transpose()
}

/// A note of the notes ref: the name of its blob and the full sha of the commit
/// that it annotates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteName {
    /// The full name of the object that holds the text of the note.
    pub(crate) blob: String,
    /// The full sha of the annotated commit: the key of the record.
    pub(crate) key: String,
}

/// Every note of the notes ref of `install`, in the order of `git notes list`:
/// one call, and no note is read. A ref that does not exist has none.
pub(crate) fn names(root: &Path, install: &str) -> Result<Vec<NoteName>, String> {
    let listing = git::run(root, &["notes", "--ref", &notes_ref(install), "list"])?;
    Ok(listing
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(blob, key)| NoteName {
            blob: blob.to_string(),
            key: key.to_string(),
        })
        .collect())
}

/// The records of the notes `names`, in their order: one `git cat-file --batch`
/// call for all of them. A note that is no record is the one line of [`parse`].
pub(crate) fn read_named(root: &Path, names: &[NoteName]) -> Result<Vec<ChangeRecord>, String> {
    if names.is_empty() {
        return Ok(Vec::new());
    }
    let input: String = names
        .iter()
        .map(|name| format!("{}\n", name.blob))
        .collect();
    let output = git::run_bytes_with_input(root, &["cat-file", "--batch"], input.as_bytes())?;
    let keys: Vec<String> = names.iter().map(|name| name.key.clone()).collect();
    let blobs = split_batch(&output, &keys)?;
    names
        .iter()
        .zip(&blobs)
        .map(|(name, bytes)| parse(&name.key, bytes))
        .collect()
}

/// The line for a commit that the clone lacks and a check needs: the merge base
/// of a record (spec 8.3). `record show` prints it for the key of a record and
/// for a commit that [`read`] cannot place in the range of a record.
pub(crate) fn not_in_clone_line(commit: &str) -> String {
    format!(
        "record: {} is not in this clone; fetch the full history to check the rules hash",
        git::short(commit)
    )
}

/// Whether `text` is the full name of an object in lowercase hexadecimal, the
/// only shape of a head or a base that goes to git: no value of a note reaches
/// git as an option or as a revision expression.
fn is_object_name(text: &str) -> bool {
    matches!(text.len(), 40 | 64)
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether the list of commits of `record` is whole: only then does the list
/// decide, and the range needs no ancestry.
fn lists_its_commits(record: &ChangeRecord) -> bool {
    matches!(record.commits, Some(Commits::List(_)))
}

/// The heads and the bases of the records among `records` that need ancestry,
/// as far as the clone has them: one `git rev-list` call over the object names
/// among them. A name that is no object name, and a name of an object that the
/// clone lacks, are not in the set.
fn present_commits(
    root: &Path,
    records: &[Result<ChangeRecord, String>],
) -> Result<HashSet<String>, String> {
    let mut names = BTreeSet::new();
    for record in records.iter().flatten().filter(|r| !lists_its_commits(r)) {
        names.insert(record.head.as_str());
        if record.merge_base.is_some() {
            names.insert(record.base.as_str());
        }
    }
    names.retain(|name| is_object_name(name));
    if names.is_empty() {
        return Ok(HashSet::new());
    }
    let input: String = names.iter().map(|name| format!("{name}\n")).collect();
    let listing = git::run_with_input(
        root,
        &[
            "rev-list",
            "--no-walk=unsorted",
            "--ignore-missing",
            "--stdin",
        ],
        input.as_bytes(),
    )?;
    Ok(listing.lines().map(str::to_string).collect())
}

/// Whether the range of `record` holds `sha`: its list of commits names it, or,
/// for a list that a cut turned into a summary or took away, `sha` is an
/// ancestor of the head of the record and no ancestor of its base. A record
/// from the empty tree has no commit to exclude. `present` holds the heads and
/// the bases that the clone has (see [`present_commits`]).
///
/// A record whose head the clone lacks holds nothing: the clone cannot have a
/// commit that is newer than the history it lacks. The base decides only for a
/// commit that reaches the head, and a clone that lacks it cannot tell whether
/// that commit is before the range: that is an `Err` with [`not_in_clone_line`].
fn holds(
    root: &Path,
    record: &ChangeRecord,
    sha: &str,
    present: &HashSet<String>,
) -> Result<bool, String> {
    if let Some(Commits::List(commits)) = &record.commits {
        return Ok(commits.iter().any(|commit| commit == sha));
    }
    if !present.contains(&record.head) || !git::is_ancestor(root, sha, &record.head)? {
        return Ok(false);
    }
    match &record.merge_base {
        None => Ok(true),
        Some(_) if !present.contains(&record.base) => Err(not_in_clone_line(&record.base)),
        Some(_) => git::is_ancestor(root, sha, &record.base).map(|reached| !reached),
    }
}

/// The paths of the notes of `reference` that `patterns` select, one `git grep`
/// call. The call lists names only and prints them with no color, so that no
/// setting of the user changes the text that is parsed. A path with another
/// character than a hexadecimal digit or a slash is no note (git does not list
/// it as one), and it never reaches the line-based input of [`note_blobs`].
fn note_paths(root: &Path, reference: &str, patterns: &[&str]) -> Result<Vec<String>, String> {
    let mut args = vec!["grep", "-l", "-z", "--no-color"];
    args.extend(patterns);
    args.extend([reference, "--"]);
    let found = git::run_tolerating(root, &args, 1)?;
    let prefix = format!("{reference}:");
    Ok(String::from_utf8_lossy(&found)
        .split('\0')
        .filter_map(|entry| entry.strip_prefix(&prefix))
        .filter(|path| {
            path.bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'/')
        })
        .map(str::to_string)
        .collect())
}

/// The paths of the notes that can hold `sha`: those that name it as text, and
/// the records that a cut took the full list from. A note that names no commit
/// of interest is never in the result.
fn candidate_paths(root: &Path, reference: &str, sha: &str) -> Result<Vec<String>, String> {
    let mut paths = note_paths(root, reference, &["-F", "-e", sha])?;
    paths.extend(note_paths(
        root,
        reference,
        &[
            "-E",
            "-e",
            SCHEMA_TEXT,
            "--and",
            "--not",
            "-e",
            LIST_OF_COMMITS,
        ],
    )?);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// The key that the note at `path` annotates: the path with the slashes of the
/// fan-out taken away.
fn key_of(path: &str) -> String {
    path.replace('/', "")
}

/// The first blob in `output`, the answer of `git cat-file --batch` to one name
/// (`<oid> blob <size> LF <the size bytes> LF`), and the output after it. `None`
/// for any other answer, the `<name> missing` line included.
fn take_blob(output: &[u8]) -> Option<(&[u8], &[u8])> {
    let end = output.iter().position(|&byte| byte == b'\n')?;
    let mut fields = std::str::from_utf8(&output[..end]).ok()?.split(' ');
    let (_oid, kind, size) = (fields.next()?, fields.next()?, fields.next()?);
    if kind != "blob" || fields.next().is_some() {
        return None;
    }
    let size: usize = size.parse().ok()?;
    let rest = &output[end + 1..];
    let blob = rest.get(..size)?;
    let after = rest.get(size..)?.strip_prefix(b"\n")?;
    Some((blob, after))
}

/// The blobs in `output`, the answer of `git cat-file --batch` to the notes at
/// `paths`, in the order of `paths`. An answer that is no blob is one line that
/// names the key of the note.
fn split_batch(mut output: &[u8], paths: &[String]) -> Result<Vec<Vec<u8>>, String> {
    let mut blobs = Vec::with_capacity(paths.len());
    for path in paths {
        let (blob, rest) = take_blob(output).ok_or_else(|| {
            format!(
                "record: {}: git cat-file gave no blob for the note",
                git::short(&key_of(path))
            )
        })?;
        blobs.push(blob.to_vec());
        output = rest;
    }
    Ok(blobs)
}

/// The blobs of the notes at `paths` under `reference`, in the order of `paths`:
/// one `git cat-file --batch` call for all of them.
fn note_blobs(root: &Path, reference: &str, paths: &[String]) -> Result<Vec<Vec<u8>>, String> {
    let input: String = paths
        .iter()
        .map(|path| format!("{reference}:{path}\n"))
        .collect();
    let output = git::run_bytes_with_input(root, &["cat-file", "--batch"], input.as_bytes())?;
    split_batch(&output, paths)
}

/// The record of the commit `sha` (a full sha): the record that `sha` keys, else
/// the record whose range holds `sha`. `None` when there is none, or when the
/// notes ref does not exist.
///
/// The search reads only the notes that can hold `sha`: those that name it as
/// text, and the records that a cut took the full list from, so a note that
/// names no commit of interest is never parsed. A record that the clone cannot
/// place the commit in is an `Err` with [`not_in_clone_line`], unless another
/// record holds the commit; a record whose head the clone lacks never is.
pub(crate) fn read(root: &Path, install: &str, sha: &str) -> Result<Option<ChangeRecord>, String> {
    if !ref_exists(root, install)? {
        return Ok(None);
    }
    if let Some(record) = read_keyed(root, install, sha)? {
        return Ok(Some(record));
    }
    let reference = notes_ref(install);
    let paths = candidate_paths(root, &reference, sha)?;
    if paths.is_empty() {
        return Ok(None);
    }
    let blobs = note_blobs(root, &reference, &paths)?;
    let records: Vec<Result<ChangeRecord, String>> = paths
        .iter()
        .zip(&blobs)
        .map(|(path, bytes)| parse(&key_of(path), bytes))
        .collect();
    let present = present_commits(root, &records)?;
    let mut unjudged = None;
    for record in records {
        let record = record?;
        match holds(root, &record, sha, &present) {
            Ok(true) => return Ok(Some(record)),
            Ok(false) => {}
            Err(line) => {
                unjudged.get_or_insert(line);
            }
        }
    }
    unjudged.map_or(Ok(None), Err)
}

/// Stores `bytes` as the note on `key` under the notes ref of `install`, in
/// place of the note that `key` has. The bytes stay as they are. A failure is
/// the one line that git gives for it: `git notes add -f` prints `Overwriting
/// existing notes for object <sha>` before it fails, and that notice is no line
/// for the failure.
pub(crate) fn write(root: &Path, install: &str, key: &str, bytes: &[u8]) -> Result<(), String> {
    let blob = git::run_with_input(root, &["hash-object", "-w", "--stdin"], bytes)?;
    git::run_naming_the_failure(
        root,
        &[
            "notes",
            "--ref",
            &notes_ref(install),
            "add",
            "-f",
            "-C",
            blob.trim(),
            key,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::document;
    use super::*;
    use crate::review::git::scratch::Repo;
    use crate::review::record::fixtures::{json_of, listing, minimal, parsed};

    /// A repository with `count` commits, and their full shas, oldest first.
    fn history(count: usize) -> (Repo, Vec<String>) {
        let repo = Repo::new();
        let shas = (0..count)
            .map(|n| {
                repo.write("file.txt", &format!("{n}\n"));
                repo.commit(&format!("commit {n}"))
            })
            .collect();
        (repo, shas)
    }

    /// The bytes of the note of `value`, as `record write` writes them: the
    /// compact JSON of the record in the order of its keys, and a line feed.
    fn bytes_of(value: &Value) -> Vec<u8> {
        document::bytes(&parsed(value))
    }

    /// The record of the commits after `shas[base]` up to `shas[head]`, keyed by
    /// `shas[head]`, in the minimal form: it lists no commit.
    fn ranged(shas: &[String], base: usize, head: usize) -> Value {
        let mut value = minimal(&shas[head]);
        value["base"] = json!(shas[base]);
        value["merge_base"] = json!(shas[base]);
        value
    }

    /// The text of the note on `key` in the notes ref `reference`, as git
    /// prints it.
    fn note_text(repo: &Repo, reference: &str, key: &str) -> String {
        repo.git(&["notes", "--ref", reference, "show", key])
    }

    #[test]
    fn the_notes_ref_of_a_top_level_install_is_refs_notes_houserules() {
        assert_eq!(notes_ref("."), "refs/notes/houserules");
    }

    #[test]
    fn the_notes_ref_of_a_subdirectory_install_names_its_path() {
        assert_eq!(notes_ref("sub"), "refs/notes/houserules-sub/sub");
        assert_eq!(notes_ref("apps/web"), "refs/notes/houserules-sub/apps/web");
    }

    #[test]
    fn the_ref_exists_after_the_first_write_and_not_before() {
        let (repo, shas) = history(1);
        assert_eq!(ref_exists(repo.root(), "."), Ok(false));

        write(repo.root(), ".", &shas[0], &bytes_of(&minimal(&shas[0]))).expect("write");

        assert_eq!(ref_exists(repo.root(), "."), Ok(true));
        assert_eq!(
            ref_exists(repo.root(), "sub"),
            Ok(false),
            "the ref of another install is another ref"
        );
    }

    #[test]
    fn a_written_record_reads_back_by_its_key() {
        let (repo, shas) = history(2);
        let value = minimal(&shas[1]);

        write(repo.root(), ".", &shas[1], &bytes_of(&value)).expect("write");

        let read = read(repo.root(), ".", &shas[1]).expect("read");
        assert_eq!(read.map(|record| json_of(&record)), Some(value));
    }

    #[test]
    fn a_record_is_found_from_any_commit_that_it_lists() {
        let (repo, shas) = history(3);
        let refs: Vec<&str> = shas.iter().map(String::as_str).collect();
        let value = listing(&refs);
        write(repo.root(), ".", &shas[2], &bytes_of(&value)).expect("write");

        for sha in &shas {
            let found = read(repo.root(), ".", sha).expect("read");
            assert_eq!(
                found.map(|record| json_of(&record)),
                Some(value.clone()),
                "{sha}"
            );
        }
    }

    #[test]
    fn the_keyed_read_finds_the_key_and_no_commit_that_the_record_lists() {
        let (repo, shas) = history(3);
        let refs: Vec<&str> = shas.iter().map(String::as_str).collect();
        write(repo.root(), ".", &shas[2], &bytes_of(&listing(&refs))).expect("write");

        assert!(
            read_keyed(repo.root(), ".", &shas[2])
                .expect("read")
                .is_some()
        );
        assert!(
            read_keyed(repo.root(), ".", &shas[0])
                .expect("read")
                .is_none()
        );
    }

    #[test]
    fn a_commit_that_no_record_names_has_no_record() {
        let (repo, shas) = history(3);
        let value = listing(&[shas[1].as_str()]);
        write(repo.root(), ".", &shas[1], &bytes_of(&value)).expect("write");

        assert!(read(repo.root(), ".", &shas[2]).expect("read").is_none());
    }

    #[test]
    fn a_commit_that_a_record_names_as_its_base_only_has_no_record() {
        let (repo, shas) = history(3);
        let mut value = listing(&[shas[2].as_str()]);
        value["base"] = json!(shas[1]);
        value["merge_base"] = json!(shas[1]);
        write(repo.root(), ".", &shas[2], &bytes_of(&value)).expect("write");

        assert!(read(repo.root(), ".", &shas[1]).expect("read").is_none());
    }

    #[test]
    fn a_range_that_a_record_summarizes_is_found_from_every_commit_of_it() {
        let (repo, shas) = history(5);
        let mut value = ranged(&shas, 0, 3);
        value["commits"] = json!({"count": 3, "first": shas[1], "last": shas[3]});
        write(repo.root(), ".", &shas[3], &bytes_of(&value)).expect("write");

        for sha in &shas[1..=3] {
            let found = read(repo.root(), ".", sha).expect("read");
            assert_eq!(
                found.map(|record| json_of(&record)),
                Some(value.clone()),
                "{sha}"
            );
        }
        assert!(
            read(repo.root(), ".", &shas[0]).expect("read").is_none(),
            "the merge base is a commit of an earlier range"
        );
        assert!(
            read(repo.root(), ".", &shas[4]).expect("read").is_none(),
            "a later commit is in no range of the record"
        );
    }

    #[test]
    fn the_minimal_record_is_found_from_every_commit_of_its_range() {
        let (repo, shas) = history(5);
        let value = ranged(&shas, 0, 3);
        write(repo.root(), ".", &shas[3], &bytes_of(&value)).expect("write");

        for sha in &shas[1..=3] {
            let found = read(repo.root(), ".", sha).expect("read");
            assert_eq!(
                found.map(|record| json_of(&record)),
                Some(value.clone()),
                "{sha}"
            );
        }
        assert!(read(repo.root(), ".", &shas[0]).expect("read").is_none());
        assert!(read(repo.root(), ".", &shas[4]).expect("read").is_none());
    }

    #[test]
    fn a_commit_of_another_branch_is_in_no_range_of_a_cut_record() {
        let (repo, shas) = history(4);
        let value = ranged(&shas, 0, 3);
        write(repo.root(), ".", &shas[3], &bytes_of(&value)).expect("write");
        repo.git(&["checkout", "-q", "-b", "side", &shas[0]]);
        repo.write("side.txt", "side\n");
        let side = repo.commit("side commit");

        assert!(read(repo.root(), ".", &side).expect("read").is_none());
    }

    #[test]
    fn a_range_with_no_merge_base_holds_every_ancestor_of_its_head() {
        let (repo, shas) = history(4);
        let mut value = minimal(&shas[2]);
        value["base"] = json!("4b825dc642cb6eb9a060e54bf8d69288fbee4904");
        value["merge_base"] = Value::Null;
        write(repo.root(), ".", &shas[2], &bytes_of(&value)).expect("write");

        for sha in &shas[..=2] {
            assert!(
                read(repo.root(), ".", sha).expect("read").is_some(),
                "{sha}"
            );
        }
        assert!(read(repo.root(), ".", &shas[3]).expect("read").is_none());
    }

    #[test]
    fn a_record_whose_head_is_not_in_the_clone_holds_no_commit_of_the_clone() {
        let (repo, shas) = history(3);
        let mut value = ranged(&shas, 0, 2);
        value["head"] = json!("f".repeat(40));
        write(repo.root(), ".", &shas[2], &bytes_of(&value)).expect("write");

        for sha in &shas[..2] {
            assert!(
                read(repo.root(), ".", sha).expect("read").is_none(),
                "{sha}: the clone cannot hold a commit that is newer than the history it lacks"
            );
        }
    }

    #[test]
    fn a_head_that_is_no_object_name_holds_no_commit_and_reaches_git_as_no_option() {
        let (repo, shas) = history(3);
        let mut value = ranged(&shas, 0, 2);
        value["head"] = json!("--all");
        write(repo.root(), ".", &shas[2], &bytes_of(&value)).expect("write");

        let found = read(repo.root(), ".", &shas[1]).expect("read");

        assert!(found.is_none(), "an option of git is no commit");
    }

    #[test]
    fn a_commit_of_a_record_whose_base_is_not_in_the_clone_is_the_named_merge_base_line() {
        let (repo, shas) = history(5);
        let absent = "f".repeat(40);
        let mut value = ranged(&shas, 0, 3);
        value["base"] = json!(absent);
        value["merge_base"] = json!(absent);
        write(repo.root(), ".", &shas[3], &bytes_of(&value)).expect("write");

        for sha in &shas[1..3] {
            let error = read(repo.root(), ".", sha).expect_err("the base is not in the clone");

            assert_eq!(
                error,
                "record: fffffff is not in this clone; fetch the full history to check the rules hash",
                "{sha}"
            );
        }
        assert!(
            read(repo.root(), ".", &shas[4]).expect("read").is_none(),
            "a commit that does not reach the head is judged with no base"
        );
    }

    #[test]
    fn a_record_that_holds_the_commit_wins_over_a_record_that_cannot_be_judged() {
        let (repo, shas) = history(5);
        let absent = "f".repeat(40);
        let mut unjudged = ranged(&shas, 0, 2);
        unjudged["base"] = json!(absent);
        unjudged["merge_base"] = json!(absent);
        write(repo.root(), ".", &shas[2], &bytes_of(&unjudged)).expect("write");
        let judged = ranged(&shas, 0, 4);
        write(repo.root(), ".", &shas[4], &bytes_of(&judged)).expect("write");

        let found = read(repo.root(), ".", &shas[1]).expect("a record holds the commit");

        assert_eq!(found.map(|record| json_of(&record)), Some(judged));
    }

    #[test]
    fn records_are_found_under_the_color_and_grep_settings_of_a_user() {
        let (repo, shas) = history(5);
        for (key, value) in [
            ("color.ui", "always"),
            ("color.grep", "always"),
            ("grep.lineNumber", "true"),
            ("grep.column", "true"),
        ] {
            repo.git(&["config", key, value]);
        }
        let listed = listing(&[shas[0].as_str(), shas[1].as_str()]);
        write(repo.root(), ".", &shas[1], &bytes_of(&listed)).expect("write");
        let mut cut = ranged(&shas, 1, 4);
        cut["commits"] = json!({"count": 3, "first": shas[2], "last": shas[4]});
        write(repo.root(), ".", &shas[4], &bytes_of(&cut)).expect("write");

        let from_list = read(repo.root(), ".", &shas[0]).expect("read");
        let from_range = read(repo.root(), ".", &shas[3]).expect("read");

        assert_eq!(from_list.map(|record| json_of(&record)), Some(listed));
        assert_eq!(from_range.map(|record| json_of(&record)), Some(cut));
    }

    #[test]
    fn the_answer_of_cat_file_splits_into_the_blobs_in_the_order_of_the_paths() {
        let paths = vec!["ab/cd".to_string(), "ef".to_string(), "01".to_string()];
        let output = b"1111 blob 3\nabc\n2222 blob 0\n\n3333 blob 4\n\xff\n\n\x80\n";

        assert_eq!(
            split_batch(output, &paths),
            Ok(vec![b"abc".to_vec(), Vec::new(), b"\xff\n\n\x80".to_vec()]),
            "a blob keeps its line feeds and every byte"
        );
    }

    #[test]
    fn an_answer_of_cat_file_that_is_no_blob_is_one_line_that_names_the_key() {
        let paths = vec!["abcdef0123/4567".to_string()];
        let expected = "record: abcdef0: git cat-file gave no blob for the note".to_string();

        for output in [
            &b"abcdef0123/4567 missing\n"[..],
            b"",
            b"1111 tree 3\nabc\n",
            b"1111 blob 9\nabc\n",
            b"1111 blob x\nabc\n",
            b"1111 blob 3\nabcX",
            b"1111 blob 3 4\nabc\n",
            b"1111 blob 99999999999999999999999\nabc\n",
        ] {
            assert_eq!(
                split_batch(output, &paths),
                Err(expected.clone()),
                "{:?}",
                String::from_utf8_lossy(output)
            );
        }
    }

    #[test]
    fn only_a_full_lowercase_hexadecimal_name_is_an_object_name() {
        assert!(is_object_name(&"a1".repeat(20)));
        assert!(is_object_name(&"0f".repeat(32)));
        for text in [
            "",
            "--all",
            "main",
            "HEAD~1",
            &"A1".repeat(20),
            &"a1".repeat(19),
            &format!("{}\n", "a1".repeat(20)),
        ] {
            assert!(!is_object_name(text), "{text:?}");
        }
    }

    #[test]
    fn an_entry_of_the_notes_tree_that_is_no_note_is_never_a_candidate() {
        let (repo, shas) = history(1);
        let record = bytes_of(&minimal(&shas[0]));
        let blob = git::run_with_input(repo.root(), &["hash-object", "-w", "--stdin"], &record)
            .expect("hash-object");
        let entries = format!(
            "100644 blob {blob}\t{}\0100644 blob {blob}\tdead\nbeef\0",
            shas[0],
            blob = blob.trim()
        );
        let tree = git::run_with_input(repo.root(), &["mktree", "-z"], entries.as_bytes())
            .expect("mktree");
        let commit = repo.git(&["commit-tree", tree.trim(), "-m", "notes"]);
        repo.git(&["update-ref", "refs/notes/houserules", commit.trim()]);

        let paths = candidate_paths(repo.root(), "refs/notes/houserules", "")
            .expect("the candidates of the notes ref");

        assert_eq!(
            paths,
            vec![shas[0].clone()],
            "the entry with a line feed is no note"
        );
    }

    #[test]
    fn a_second_write_replaces_the_note() {
        let (repo, shas) = history(1);
        let mut value = minimal(&shas[0]);
        write(repo.root(), ".", &shas[0], &bytes_of(&value)).expect("write");
        value["written_by"] = json!("houserules 9.9.9");

        write(repo.root(), ".", &shas[0], &bytes_of(&value)).expect("write again");

        let read = read(repo.root(), ".", &shas[0])
            .expect("read")
            .expect("a record");
        assert_eq!(json_of(&read)["written_by"], json!("houserules 9.9.9"));
    }

    #[test]
    fn the_bytes_of_the_note_are_the_bytes_that_write_is_given() {
        let (repo, shas) = history(1);
        let bytes = b"{}\n\n\n  trailing   \n# not a comment\n";

        write(repo.root(), ".", &shas[0], bytes).expect("write");

        assert_eq!(
            note_text(&repo, "refs/notes/houserules", &shas[0]).as_bytes(),
            bytes,
            "git keeps blank lines, trailing spaces, and a line that starts with #"
        );
    }

    #[test]
    fn a_subdirectory_install_writes_its_own_ref() {
        let (repo, shas) = history(1);
        let bytes = bytes_of(&minimal(&shas[0]));

        write(repo.root(), "sub", &shas[0], &bytes).expect("write");

        assert_eq!(
            note_text(&repo, "refs/notes/houserules-sub/sub", &shas[0]).as_bytes(),
            bytes.as_slice()
        );
        assert_eq!(ref_exists(repo.root(), "."), Ok(false));
    }

    #[test]
    fn a_note_that_is_no_record_is_one_named_error() {
        let (repo, shas) = history(1);
        write(repo.root(), ".", &shas[0], b"not json\n").expect("write");

        let error = read(repo.root(), ".", &shas[0]).expect_err("not a record");

        assert!(
            error.starts_with(&format!("record: {}: not a change record (", &shas[0][..7])),
            "{error}"
        );
        assert!(!error.contains('\n'), "one line: {error}");
    }

    #[test]
    fn the_names_list_every_note_with_the_commit_it_annotates() {
        let (repo, shas) = history(3);
        write(repo.root(), ".", &shas[0], &bytes_of(&minimal(&shas[0]))).expect("write");
        write(repo.root(), ".", &shas[2], &bytes_of(&minimal(&shas[2]))).expect("write");

        let names = names(repo.root(), ".").expect("names");

        let mut keys: Vec<&str> = names.iter().map(|name| name.key.as_str()).collect();
        keys.sort_unstable();
        let mut expected = vec![shas[0].as_str(), shas[2].as_str()];
        expected.sort_unstable();
        assert_eq!(keys, expected);
        for name in &names {
            let blob = repo.git(&["notes", "--ref", "refs/notes/houserules", "list", &name.key]);
            assert_eq!(name.blob, blob.trim());
        }
    }

    #[test]
    fn a_notes_ref_that_does_not_exist_has_no_name() {
        let (repo, _shas) = history(1);

        assert_eq!(names(repo.root(), "."), Ok(Vec::new()));
        assert_eq!(read_named(repo.root(), &[]), Ok(Vec::new()));
    }

    #[test]
    fn the_named_notes_read_as_records_in_the_order_of_the_names() {
        let (repo, shas) = history(3);
        let values: Vec<Value> = shas.iter().map(|sha| minimal(sha)).collect();
        for (sha, value) in shas.iter().zip(&values) {
            write(repo.root(), ".", sha, &bytes_of(value)).expect("write");
        }
        let mut listed = names(repo.root(), ".").expect("names");
        listed.reverse();

        let records = read_named(repo.root(), &listed).expect("records");

        let expected: Vec<&String> = listed.iter().map(|name| &name.key).collect();
        let keys: Vec<String> = records
            .iter()
            .map(|record| json_of(record)["key"].as_str().expect("a key").to_string())
            .collect();
        assert_eq!(keys.iter().collect::<Vec<_>>(), expected);
    }

    #[test]
    fn a_named_note_that_is_no_record_is_the_one_line_of_its_key() {
        let (repo, shas) = history(2);
        write(repo.root(), ".", &shas[0], &bytes_of(&minimal(&shas[0]))).expect("write");
        write(repo.root(), ".", &shas[1], b"not json\n").expect("write");
        let listed = names(repo.root(), ".").expect("names");

        let error = read_named(repo.root(), &listed).expect_err("one note is no record");

        assert!(
            error.starts_with(&format!("record: {}: not a change record (", &shas[1][..7])),
            "{error}"
        );
    }

    #[test]
    fn the_bytes_of_a_note_are_the_bytes_written_and_a_commit_with_no_note_has_none() {
        let (repo, shas) = history(2);
        let bytes = b"{}\n\n  trailing   \n";
        write(repo.root(), ".", &shas[0], bytes).expect("write");

        assert_eq!(
            read_bytes(repo.root(), ".", &shas[0]),
            Ok(Some(bytes.to_vec()))
        );
        assert_eq!(read_bytes(repo.root(), ".", &shas[1]), Ok(None));
    }

    #[test]
    fn a_note_that_does_not_mention_the_sha_is_never_parsed() {
        let (repo, shas) = history(2);
        write(repo.root(), ".", &shas[0], b"not json\n").expect("write");

        let found = read(repo.root(), ".", &shas[1]).expect("a note that holds no sha is skipped");

        assert!(found.is_none());
    }
}
