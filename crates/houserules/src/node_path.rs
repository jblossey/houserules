//! `resolve_like_node`, Node's `path.resolve(cwd, path)` ported exactly --
//! extracted from `rules::validate_deliverable` (batch 18 T3) once
//! `install::cmd_init` needed the identical resolution for `--dir`: both
//! callers turn a CLI-supplied path (or its absence) into the same
//! absolute, textually-normalized form Node's own `path.resolve` would
//! produce from the process's real working directory, and a second,
//! independently-written copy is exactly the drift DRY exists to prevent
//! in a crate whose whole contract is byte parity with the frozen JS.
//! Lives at the crate root, like `emit`, `get`, and `root`, for the same
//! reason those files give: `rules` and `install` neither should reach
//! into the other for a leaf utility neither owns.

use std::path::{Component, Path, PathBuf};

/// The absolute form of `path`, matching Node's `path.resolve(cwd, path)`
/// exactly -- `std::path::absolute` comes close but is not quite it: its
/// own docs say it deliberately keeps `..` components unresolved on
/// POSIX ("this function does not access the filesystem", so a `..` after
/// a possible symlink cannot be collapsed with confidence), where
/// `path.resolve` always collapses them textually, since Node's own
/// version is a pure string operation with no such caution to begin with
/// (verified live against `tools/kb.mjs` at the frozen sha:
/// `path.resolve('/foo/../../baz')` is `/baz`, never `/../baz`). Both
/// join onto `std::env::current_dir`/`process.cwd()` for a relative
/// `path` -- CI issue 1's own root cause was a test asserting a symlinked
/// temp directory's own name would survive that join, when neither
/// engine's real current-directory query ever preserves one
/// (`getcwd(3)`, which both ultimately call, is specified to resolve
/// every symlink; verified live, `tools/kb.sh validate` run through a
/// symlinked cwd reports the real directory, not the symlink's name --
/// see `validate_stats_audit_parity.rs`'s own symlink test). CI round 2's
/// own fix: `strip_verbatim_disk_prefix`'s doc has the Windows-only half
/// of this parity (a `\\?\` extended-length prefix `path.resolve`/
/// `GetFullPathNameW` never produce, but a Windows `canonicalize` --
/// this crate's own `tests/common/mod.rs::repo_root`, an already-absolute
/// argument built from it -- always does).
pub(crate) fn resolve_like_node(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(normalize_lexically(&strip_verbatim_disk_prefix(path)));
    }
    let cwd = std::env::current_dir()?;
    Ok(normalize_lexically(&strip_verbatim_disk_prefix(
        &cwd.join(path),
    )))
}

/// Strips a `\\?\` extended-length-path prefix from `path`'s own plain-disk
/// form, if present -- CI round 2, issue 1 (Windows only; a no-op
/// everywhere else, since the prefix cannot occur there): the binary's
/// own parity slices showed it verbatim (`\\?\D:\a\houserules\...`) where
/// Node's real output never carries one, because `std::fs::canonicalize`
/// returns Windows' own UNC-verbatim form and neither `path.resolve` nor
/// the Win32 calls it wraps (`GetFullPathNameW`, `GetCurrentDirectoryW`)
/// ever produce it (verified against the installed toolchain's own
/// `std::fs::canonicalize` docs: its "Platform-specific behavior" section
/// says plainly that on Windows "this converts the path to use extended
/// length path syntax", the `\\?\` form). Considered the `dunce` crate first
/// (crates.io, security-hygiene.dependency-vetting) -- it does this and
/// more (also declines to strip a path Windows would then read
/// differently: a reserved device name, or one past `MAX_PATH`) -- but
/// its own safety check is `const fn ... -> bool { false }` outside
/// `cfg(windows)` (its published source), making the transformation
/// itself impossible to exercise without a Windows runner, which this
/// development environment does not have; this crate's own narrower,
/// always-active equivalent keeps it testable here (below), with the
/// Windows CI parity slices as the platform proof for the cases it
/// cannot reach: a resolved path shaped this narrowly (a deliverable
/// JSON file a user or agent placed and is now pointing this command
/// at) is not expected to carry a reserved device name or exceed
/// `MAX_PATH`, and matching Node's own unprotected behavior there is the
/// correct parity, not a gap this fix introduces.
fn strip_verbatim_disk_prefix(path: &Path) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Collapses `.`/`..` path components without touching the filesystem --
/// `path.resolve`'s own final normalization step (`resolve_like_node`'s
/// doc). `Path::components` already drops repeated separators and a bare
/// `.` component on its own; only `..` needs handling here: it pops the
/// last pushed normal component, or is dropped outright at the root
/// (there being nothing above it to keep -- verified live: Node's own
/// `path.resolve('/foo/../../baz')` is `/baz`, not `/../baz`).
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | None => {}
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CI round 2, issue 1: pure string logic, so this runs the same on
    /// every platform even though the prefix itself only ever appears in
    /// a real path on Windows -- the point of writing it this way rather
    /// than reaching only for a Windows-gated library call, since no
    /// Windows runner exists in this development environment to exercise
    /// one otherwise (this function's own doc has the full account).
    #[test]
    fn strip_verbatim_disk_prefix_removes_the_prefix_from_a_plain_disk_path() {
        assert_eq!(
            strip_verbatim_disk_prefix(Path::new(r"\\?\C:\Users\runneradmin")),
            PathBuf::from(r"C:\Users\runneradmin")
        );
    }

    /// A UNC-share verbatim path (`\\?\UNC\server\share\...`) is not a
    /// plain disk path -- `C` at the position right after the prefix is
    /// `U`, not a drive letter followed by `:` -- so it must be left
    /// alone: stripping it would silently change which server the path
    /// names, not just its cosmetic form.
    #[test]
    fn strip_verbatim_disk_prefix_leaves_a_unc_share_path_alone() {
        let unc = Path::new(r"\\?\UNC\server\share\file.json");
        assert_eq!(strip_verbatim_disk_prefix(unc), unc.to_path_buf());
    }

    /// A path that never carried the prefix at all -- the common case on
    /// every non-Windows platform, and most Windows paths too -- passes
    /// through unchanged.
    #[test]
    fn strip_verbatim_disk_prefix_leaves_an_unprefixed_path_alone() {
        let plain = Path::new("/foo/bar.json");
        assert_eq!(strip_verbatim_disk_prefix(plain), plain.to_path_buf());
    }
}
