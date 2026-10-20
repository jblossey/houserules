//! A snapshot of a scratch tree, and the comparison of two snapshots. A
//! failed comparison names the paths that differ and prints no file content.
//! Every test file that compares trees includes this module, as
//! `tests/workflow_reader/mod.rs` is shared, and keeps its own other helpers.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Every file under a directory, by `/`-separated relative path.
pub type Tree = BTreeMap<String, Vec<u8>>;

/// Every file under `dir` except `.git/`, by `/`-separated relative path.
pub fn snapshot(dir: &Path) -> Tree {
    fn walk(dir: &Path, prefix: &str, out: &mut Tree) {
        for entry in fs::read_dir(dir).expect("read the directory") {
            let entry = entry.expect("read a directory entry");
            let name = entry.file_name().into_string().expect("utf8 file name");
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.file_type().expect("file type").is_dir() {
                if relative != ".git" {
                    walk(&entry.path(), &relative, out);
                }
            } else {
                out.insert(relative, fs::read(entry.path()).expect("read a file"));
            }
        }
    }
    let mut out = Tree::new();
    walk(dir, "", &mut out);
    out
}

/// Requires `after` to hold the files of `before`, byte for byte. A failure
/// panics with `context` and the three lists of paths: added, removed, and
/// changed.
pub fn assert_same_tree(before: &Tree, after: &Tree, context: &str) {
    let added: Vec<&String> = after
        .keys()
        .filter(|path| !before.contains_key(*path))
        .collect();
    let removed: Vec<&String> = before
        .keys()
        .filter(|path| !after.contains_key(*path))
        .collect();
    let changed: Vec<&String> = before
        .iter()
        .filter(|(path, bytes)| after.get(*path).is_some_and(|other| other != *bytes))
        .map(|(path, _)| path)
        .collect();
    assert!(
        added.is_empty() && removed.is_empty() && changed.is_empty(),
        "{context}: the tree changed\n  added: {added:?}\n  removed: {removed:?}\n  changed: {changed:?}"
    );
}
