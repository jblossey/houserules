//! What `update` says about the CI wiring (spec 10.3 and 10.7): the pin line
//! for a file that `houserules ci write` created with another version, and the
//! opt-in line for a seeded CI workflow. `update` writes no CI file and adds no
//! job to one.
//!
//! A file of `ci write` starts with the line `CI_WRITTEN_BY_LINE` and holds the
//! version of the binary that wrote it in the line after it: `# houserules
//! <version>`. The scan reads the first bytes of each tracked file of the
//! repository, from its top level, so that an install below the top level finds
//! a file there. `ci write` writes the second line with [`version_line`] and
//! rewrites only a file that [`is_written_by_ci_write`] names.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::Path;

use crate::rules::tracked_from_top_level;

use super::marker::review::CI_WRITTEN_BY_LINE;

/// The start of the second line of a file of `ci write`.
const VERSION_LINE_PREFIX: &str = "# houserules ";

/// The number of bytes to read from each file. The two lines of a file of
/// `ci write` fit in it; a file whose first line is longer is no such file.
const HEAD_BYTES: u64 = 512;

/// The second line of a file that `ci write` creates for the binary of `version`: the line that
/// `update` reads back.
pub(crate) fn version_line(version: &str) -> String {
    format!("{VERSION_LINE_PREFIX}{version}")
}

/// `true` when `file` is a file of `ci write`: a regular file whose first line is
/// `CI_WRITTEN_BY_LINE`. A file that is gone, and one that is no regular file (a link, a folder),
/// is none. Any other read error is one line that names the file with the operating system's own
/// text.
pub(crate) fn is_written_by_ci_write(file: &Path) -> Result<bool, String> {
    let Some(head) = read_head(file)? else {
        return Ok(false);
    };
    Ok(String::from_utf8_lossy(&head).lines().next() == Some(CI_WRITTEN_BY_LINE))
}

/// The opt-in line of `update` for the seeded CI workflow at `path` (spec
/// 10.7): the workflow keeps running the gates, and the review layer comes with
/// `houserules ci print`.
pub(super) fn opt_in_line(path: &str) -> String {
    format!(
        "update: {path} runs the houserules gates; to add the review layer, run houserules ci print --host github"
    )
}

/// The pin line for each tracked file of the repository at `top_level` that
/// `ci write` created and whose version differs from `version`, in the order
/// of git's listing (spec 10.3). `root` is the install root, where git runs.
/// A listed file that is gone, and one that is no regular file (a link or a
/// submodule), has no version to read. Any other read error is one line that
/// names the file with the operating system's own text.
pub(super) fn pin_lines(
    root: &Path,
    top_level: &Path,
    version: &str,
) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for path in tracked_from_top_level(root)? {
        let file = path
            .split('/')
            .fold(top_level.to_path_buf(), |parent, part| parent.join(part));
        if let Some(pinned) = written_version(&file)?
            && pinned != version
        {
            lines.push(format!(
                "update: {path} pins houserules {pinned}; this binary is {version}: run houserules ci write --force with the flags that wrote it (a GitHub reference file takes the --sha of v{version})"
            ));
        }
    }
    Ok(lines)
}

/// The version in the second line of `file` when its first line is
/// `CI_WRITTEN_BY_LINE`. `None` for any other file, and for a file of `ci write`
/// whose second line is no version line.
fn written_version(file: &Path) -> Result<Option<String>, String> {
    let Some(head) = read_head(file)? else {
        return Ok(None);
    };
    let text = String::from_utf8_lossy(&head);
    let mut lines = text.lines();
    if lines.next() != Some(CI_WRITTEN_BY_LINE) {
        return Ok(None);
    }
    Ok(lines
        .next()
        .and_then(|line| line.strip_prefix(VERSION_LINE_PREFIX))
        .map(|version| version.trim().to_string()))
}

/// The first `HEAD_BYTES` bytes of `file`, or `None` for a file that is gone or
/// that is no regular file.
fn read_head(file: &Path) -> Result<Option<Vec<u8>>, String> {
    let named = |error: io::Error| format!("{}: {error}", file.display());
    match fs::symlink_metadata(file) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(named(error)),
    }
    let mut head = Vec::new();
    match File::open(file).and_then(|opened| opened.take(HEAD_BYTES).read_to_end(&mut head)) {
        Ok(_) => Ok(Some(head)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(named(error)),
    }
}
