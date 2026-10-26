//! The key of an install (spec 10.1, design.md 258): the name that the jobs, the artifacts, the
//! status context, the labels, and the notes ref of one install carry, so that two installs of one
//! repository never share one.
//!
//! The file imports nothing of the crate, so that a test file includes it with `#[path]` and
//! calls the function that the command calls (the crate has no library target).

/// The key of the install at `path`, a path below the git top level: `root` for the top level,
/// else the path with each character outside `A-Za-z0-9_-` replaced by `-`. A leading `./`, a
/// trailing `/`, and any `.` or empty part are not part of the path.
pub(crate) fn install_key(path: &str) -> String {
    let parts: Vec<&str> = path
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.is_empty() {
        return "root".to_string();
    }
    parts
        .join("/")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect()
}
