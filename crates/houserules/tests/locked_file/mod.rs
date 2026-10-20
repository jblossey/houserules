//! A file made read-only for the length of a test, so that the one write of
//! `update` that targets it fails and the writes before it succeed. A test
//! that interrupts a run this way finishes the run afterwards and compares the
//! tree with the tree of a run that no fault stopped. Every test file that
//! locks a file includes this module, as `tests/tree_snapshot/mod.rs` is
//! shared.

use std::fs;
use std::path::{Path, PathBuf};

/// A file that is read-only until the guard drops. The drop gives the file
/// its permissions back, so the scratch directory can be removed on every
/// platform, also after a failed assertion.
pub struct Locked {
    path: PathBuf,
    original: fs::Permissions,
}

/// Makes the file at `path` read-only, so that a write to it fails with the
/// operating system's own error until the returned guard drops. The answer is
/// `None` when a write still succeeds: a user with the right to write
/// anywhere (`root`) is not stopped by the lock, and a test that needs a
/// stopped run skips its case then.
pub fn lock(path: &Path) -> Option<Locked> {
    let original = fs::metadata(path)
        .unwrap_or_else(|error| panic!("read the metadata of {}: {error}", path.display()))
        .permissions();
    let mut read_only = original.clone();
    read_only.set_readonly(true);
    fs::set_permissions(path, read_only)
        .unwrap_or_else(|error| panic!("lock {}: {error}", path.display()));
    let locked = Locked {
        path: path.to_path_buf(),
        original,
    };
    let still_writable = fs::OpenOptions::new().append(true).open(path).is_ok();
    (!still_writable).then_some(locked)
}

impl Drop for Locked {
    /// Gives the file its permissions back. A file that a test deleted
    /// meanwhile has none to give back.
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, self.original.clone());
    }
}
