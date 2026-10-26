//! The record of the files that a command writes.
//!
//! Every write of a file in `init` and `update` goes through [`write`]: the two shared writers of
//! the install (`install::write_into`, `archive::write_json`), the merges of
//! `.claude/settings.json`, the render, and the rewrite of the `backlogId` pattern. A command that
//! needs the list of the files it wrote opens a [`Recording`] before its first write and asks it
//! for the paths after its last. The shared writers leave a file that already holds the bytes
//! alone (`archive::write_json`, `install::write_if_changed`), so such a file is not in the
//! record. The merge of a `hooks` value that is a JSON array is the one writer that writes
//! unchanged bytes.
//!
//! The record belongs to the thread that opened it. A write on a thread with no open recording is
//! the plain `std::fs::write`.
//!
//! `tests/written_home.rs` fails when a production line outside this file writes a file directly,
//! unless the file is one of its exemptions: the commands that are no part of `init` or `update`.

use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

thread_local! {
    /// The paths written since the open [`Recording`] of this thread began, in the order of the
    /// writes; `None` while no recording is open.
    static RECORD: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}

/// Writes `content` to `path` as `std::fs::write` does and, when a [`Recording`] is open on this
/// thread, adds `path` to it after the write succeeds.
pub(crate) fn write(path: &Path, content: impl AsRef<[u8]>) -> io::Result<()> {
    fs::write(path, content)?;
    RECORD.with(|record| {
        if let Some(paths) = record.borrow_mut().as_mut() {
            paths.push(path.to_path_buf());
        }
    });
    Ok(())
}

/// The open record of the writes of one command on this thread. Dropping it closes the record.
pub(crate) struct Recording(());

impl Recording {
    /// Opens a record. A record that is open on this thread already is replaced.
    pub(crate) fn open() -> Self {
        RECORD.with(|record| *record.borrow_mut() = Some(Vec::new()));
        Self(())
    }

    /// Closes the record and returns the paths written since it was opened, in the order of the
    /// writes, a path once for each write.
    pub(crate) fn finish(self) -> Vec<PathBuf> {
        RECORD.with(|record| record.borrow_mut().take().unwrap_or_default())
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        RECORD.with(|record| *record.borrow_mut() = None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A write while a record is open puts the path in the record, in the order of the writes.
    #[test]
    fn a_write_while_a_record_is_open_is_in_the_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (first, second) = (dir.path().join("first.txt"), dir.path().join("second.txt"));
        let recording = Recording::open();

        write(&second, "text\n").expect("write the file");
        write(&first, b"text\n").expect("write the file");

        assert_eq!(recording.finish(), [second.clone(), first.clone()]);
        assert_eq!(fs::read_to_string(&first).expect("read the file"), "text\n");
    }

    /// A write with no open record writes the file and keeps no path, and a record that was closed
    /// holds none of the writes after it.
    #[test]
    fn a_write_outside_a_record_is_not_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let before = dir.path().join("before.txt");
        write(&before, "text\n").expect("write the file");
        let recording = Recording::open();
        let paths = recording.finish();
        write(&dir.path().join("after.txt"), "text\n").expect("write the file");

        assert!(before.is_file());
        assert!(paths.is_empty());
        assert_eq!(Recording::open().finish(), Vec::<PathBuf>::new());
    }

    /// A write that fails is no entry of the record.
    #[test]
    fn a_failed_write_is_not_in_the_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let recording = Recording::open();

        let error = write(&dir.path().join("missing").join("file.txt"), "text\n");

        assert!(error.is_err());
        assert!(recording.finish().is_empty());
    }

    /// Dropping a record closes it: the next record starts empty.
    #[test]
    fn a_dropped_record_leaves_nothing_for_the_next_one() {
        let dir = tempfile::tempdir().expect("tempdir");
        drop(Recording::open());
        write(&dir.path().join("lost.txt"), "text\n").expect("write the file");

        assert!(Recording::open().finish().is_empty());
    }

    /// The record belongs to its thread: a write on another thread is not in it.
    #[test]
    fn a_write_on_another_thread_is_not_in_the_record() {
        let dir = tempfile::tempdir().expect("tempdir");
        let elsewhere = dir.path().join("elsewhere.txt");
        let recording = Recording::open();

        std::thread::scope(|scope| {
            scope.spawn(|| write(&elsewhere, "text\n").expect("write the file"));
        });

        assert!(elsewhere.is_file());
        assert!(recording.finish().is_empty());
    }
}
