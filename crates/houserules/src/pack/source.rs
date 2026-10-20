//! The three source forms of a pack, and the read of a file source and a
//! git source.
//!
//! `.houserules.json` records a pack's `source` as one string. `kit` means
//! the binary carries the pack. `file:<path>` means a pack file inside the
//! install, the path relative to the install root. `git+<url>@<tag>#<path>`
//! means one file of a git repository at a tag: the part after the last `#`
//! is the file's path in that repository, and in the rest the last `@`
//! splits the URL from the tag, so `git+ssh://git@host:2222/o/r@v1#p.json`
//! keeps the `@` and `:` of its URL.
//!
//! A git source runs through the user's own `git` command, so any host and
//! the user's git credentials work and no library is added. The URL is never
//! a relative path: `git` would resolve it against the working directory of
//! each call, and a pin must name one repository from any directory.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::archive::read_json_as;

/// The text of the refusal of a git source that is not well formed.
const GIT_FORM: &str = "a git source reads git+<url>@<tag>#<path>, with every part set and a url that does not start with -";

/// The text of the refusal of a git source whose URL is a relative local
/// path.
const GIT_RELATIVE_URL: &str = "a relative path as the url names another repository from each directory; use a file:// URL or a remote URL";

/// Where a pack's copy came from, as `.houserules.json` records it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PackSource {
    /// The binary carries the pack.
    Kit,
    /// A pack file inside the install; the path is relative to the install
    /// root and uses `/` between components.
    File(String),
    /// One file of a git repository at a tag.
    Git {
        /// The repository URL, as `git` takes it.
        url: String,
        /// The tag, never a branch or a commit.
        tag: String,
        /// The file's path in the repository.
        path: String,
    },
}

impl PackSource {
    /// Parses a recorded source string: `kit`, `file:<path>`, or
    /// `git+<url>@<tag>#<path>`. A string of another shape, and a git
    /// source with an empty part, a URL that starts with `-`, or a URL that
    /// is a relative local path, is one refusal line that names it.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        if text == "kit" {
            return Ok(Self::Kit);
        }
        if let Some(path) = text.strip_prefix("file:") {
            return if path.is_empty() {
                Err(format!("{text}: a file source reads file:<path>"))
            } else {
                Ok(Self::File(path.to_string()))
            };
        }
        if let Some(rest) = text.strip_prefix("git+") {
            let refusal = || format!("{text}: {GIT_FORM}");
            let (before_path, path) = rest.rsplit_once('#').ok_or_else(refusal)?;
            let (url, tag) = before_path.rsplit_once('@').ok_or_else(refusal)?;
            if url.is_empty() || tag.is_empty() || path.is_empty() || url.starts_with('-') {
                return Err(refusal());
            }
            if is_relative_local_path(url) {
                return Err(format!("{text}: {GIT_RELATIVE_URL}"));
            }
            return Ok(Self::Git {
                url: url.to_string(),
                tag: tag.to_string(),
                path: path.to_string(),
            });
        }
        Err(format!(
            "{text}: a source is kit, file:<path>, or git+<url>@<tag>#<path>"
        ))
    }
}

/// `true` when `url` is a relative local path: git reads it against the
/// working directory of each call. An absolute path (`/` or `\` first) is
/// not one. Neither is a scheme URL, an scp-like address (`host:path`), or
/// a drive path (`C:\path`): each holds a colon before its first slash,
/// which is how git tells them from a path.
fn is_relative_local_path(url: &str) -> bool {
    if url.starts_with(['/', '\\']) {
        return false;
    }
    match (url.find(':'), url.find('/')) {
        (None, _) => true,
        (Some(colon), Some(slash)) => slash < colon,
        (Some(_), None) => false,
    }
}

impl fmt::Display for PackSource {
    /// The recorded string: the text that `parse` reads back.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kit => write!(f, "kit"),
            Self::File(path) => write!(f, "file:{path}"),
            Self::Git { url, tag, path } => write!(f, "git+{url}@{tag}#{path}"),
        }
    }
}

/// Reads the pack file at `path` as JSON. A failure is one line that names
/// the file as `name`.
pub(crate) fn read_pack_file(path: &Path, name: &str) -> Result<Value, String> {
    read_json_as(path, name)
}

/// One pack file fetched from a git tag.
pub(crate) struct GitFetch {
    /// The file, parsed.
    pub(crate) raw: Value,
    /// The commit the tag points to: the peeled commit for an annotated
    /// tag, never the tag object's own id.
    pub(crate) commit: String,
}

/// Fetches `path` at `tag` of the repository at `url` through `git`.
///
/// `git ls-remote --tags` resolves the tag first; a ref that is no tag (a
/// branch or a commit) is refused, because a pin is exact. The file then
/// comes from a shallow fetch of that one tag into a temporary bare
/// repository, which this function removes. Every failure is one line that
/// names the source and the git command.
pub(crate) fn fetch_git(url: &str, tag: &str, path: &str) -> Result<GitFetch, String> {
    let source = PackSource::Git {
        url: url.to_string(),
        tag: tag.to_string(),
        path: path.to_string(),
    }
    .to_string();
    let commit = resolve_tag(&source, url, tag)?;
    let scratch = ScratchRepo::create(&source)?;
    run_git(&source, "init", &["--bare", "--quiet"], Some(&scratch.0))?;
    let reference = format!("refs/tags/{tag}");
    run_git(
        &source,
        "fetch",
        &["--quiet", "--depth", "1", "--", url, &reference],
        Some(&scratch.0),
    )?;
    let fetched = run_git(
        &source,
        "rev-parse",
        &["--verify", "FETCH_HEAD^{commit}"],
        Some(&scratch.0),
    )?;
    if String::from_utf8_lossy(&fetched).trim() != commit {
        return Err(format!(
            "{source}: the tag \"{tag}\" moved while houserules fetched it; run the command again"
        ));
    }
    let bytes = run_git(
        &source,
        "show",
        &[&format!("FETCH_HEAD:{path}")],
        Some(&scratch.0),
    )?;
    let raw = serde_json::from_slice(&bytes)
        .map_err(|error| format!("{source}: invalid JSON ({error})"))?;
    Ok(GitFetch { raw, commit })
}

/// The commit that `tag` of the repository at `url` points to, from `git
/// ls-remote --tags`: the peeled line (`<tag>^{}`) wins for an annotated
/// tag. A repository with no such tag line is refused.
fn resolve_tag(source: &str, url: &str, tag: &str) -> Result<String, String> {
    let direct = format!("refs/tags/{tag}");
    let peeled = format!("{direct}^{{}}");
    let listing = run_git(
        source,
        "ls-remote",
        &["--tags", "--", url, &direct, &peeled],
        None,
    )?;
    let (mut direct_commit, mut peeled_commit) = (None, None);
    for line in String::from_utf8_lossy(&listing).lines() {
        match line.split_once('\t') {
            Some((commit, name)) if name == peeled => peeled_commit = Some(commit.to_string()),
            Some((commit, name)) if name == direct => direct_commit = Some(commit.to_string()),
            _ => {}
        }
    }
    peeled_commit.or(direct_commit).ok_or_else(|| {
        format!(
            "{source}: {url} has no tag \"{tag}\"; a pin names a tag, never a branch or a commit"
        )
    })
}

/// Runs `git <subcommand> <args>` in `dir` (the current directory when
/// `None`) and returns its stdout. A git that cannot run is the operating
/// system's own error. A git that fails is its stderr lines, joined into
/// one line. The text of git's own messages is English (`LC_ALL=C`).
///
/// A `GIT_DIR` or `GIT_WORK_TREE` in the environment, which a git hook
/// sets, would send every command to that repository, so both are removed.
fn run_git(
    source: &str,
    subcommand: &str,
    args: &[&str],
    dir: Option<&Path>,
) -> Result<Vec<u8>, String> {
    let mut command = Command::new("git");
    command
        .arg(subcommand)
        .args(args)
        .env("LC_ALL", "C")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE");
    if let Some(dir) = dir {
        command.current_dir(dir);
    }
    let output = command
        .output()
        .map_err(|error| format!("{source}: cannot run git: {error}"))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let detail: Vec<String> = String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let detail = if detail.is_empty() {
        output.status.to_string()
    } else {
        detail.join("; ")
    };
    Err(format!("{source}: git {subcommand} failed: {detail}"))
}

/// A temporary directory that holds the bare repository of one fetch. It
/// is removed when the value drops.
struct ScratchRepo(PathBuf);

impl ScratchRepo {
    /// Creates a new, empty directory under the operating system's
    /// temporary directory. The name holds the process id and the clock's
    /// nanoseconds; `create_dir` fails when the name exists.
    fn create(source: &str) -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let path =
            std::env::temp_dir().join(format!("houserules-pack-{}-{nanos}", std::process::id()));
        fs::create_dir(&path).map_err(|error| format!("{source}: {}: {error}", path.display()))?;
        Ok(Self(path))
    }
}

impl Drop for ScratchRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(url: &str, tag: &str, path: &str) -> PackSource {
        PackSource::Git {
            url: url.to_string(),
            tag: tag.to_string(),
            path: path.to_string(),
        }
    }

    /// The three recorded forms parse, and `Display` writes each back.
    #[test]
    fn each_recorded_form_round_trips() {
        let cases = [
            ("kit", PackSource::Kit),
            (
                "file:vendor/team.json",
                PackSource::File("vendor/team.json".to_string()),
            ),
            (
                "git+https://example.com/o/r.git@v1.2.0#packs/team.json",
                git("https://example.com/o/r.git", "v1.2.0", "packs/team.json"),
            ),
        ];
        for (text, source) in cases {
            assert_eq!(PackSource::parse(text), Ok(source.clone()), "{text}");
            assert_eq!(source.to_string(), text);
        }
    }

    /// Review focus 2: a URL that holds `@` and `:` keeps both; the last
    /// `@` splits the tag and the last `#` splits the path.
    #[test]
    fn the_last_at_and_the_last_hash_split_the_source() {
        assert_eq!(
            PackSource::parse("git+ssh://git@host:2222/o/r@v1.2.0#packs/p.json"),
            Ok(git("ssh://git@host:2222/o/r", "v1.2.0", "packs/p.json"))
        );
        assert_eq!(
            PackSource::parse("git+https://h/r#frag@v1#p.json"),
            Ok(git("https://h/r#frag", "v1", "p.json"))
        );
    }

    /// A string of another shape is refused with the forms a source takes.
    #[test]
    fn a_source_of_another_shape_is_refused() {
        assert_eq!(
            PackSource::parse("https://example.com/p.json"),
            Err("https://example.com/p.json: a source is kit, file:<path>, or git+<url>@<tag>#<path>"
                .to_string())
        );
        assert_eq!(
            PackSource::parse("file:"),
            Err("file:: a file source reads file:<path>".to_string())
        );
    }

    /// A URL that is a relative local path is refused with the source and
    /// the two forms to use instead: git resolves it against the working
    /// directory of each call, so it names no one repository.
    #[test]
    fn a_relative_local_path_is_no_git_url() {
        for url in [
            "../host.git",
            "./host.git",
            "host.git",
            "dir/host.git",
            "~/host.git",
            "dir/with:colon.git",
        ] {
            let text = format!("git+{url}@v1#p.json");
            assert_eq!(
                PackSource::parse(&text),
                Err(format!("{text}: {GIT_RELATIVE_URL}")),
                "{url}"
            );
        }
    }

    /// Every other URL form of git parses: a scheme URL, an scp-like
    /// address, and an absolute path.
    #[test]
    fn a_url_that_is_no_relative_path_parses() {
        for url in [
            "file:///srv/host.git",
            "https://example.com/o/r.git",
            "ssh://git@host:2222/o/r.git",
            "git@host:o/r.git",
            "host:o/r.git",
            "host:r.git",
            "/srv/host.git",
            "C:/srv/host.git",
            "C:\\srv\\host.git",
            "\\\\server\\share\\host.git",
        ] {
            let text = format!("git+{url}@v1#p.json");
            assert_eq!(
                PackSource::parse(&text),
                Ok(git(url, "v1", "p.json")),
                "{url}"
            );
        }
    }

    /// A git source needs every part, and its URL must not start with `-`.
    #[test]
    fn a_git_source_needs_every_part() {
        for text in [
            "git+u@v1",
            "git+u#p",
            "git+@v1#p",
            "git+u@#p",
            "git+u@v1#",
            "git+-x@v1#p",
        ] {
            assert_eq!(
                PackSource::parse(text),
                Err(format!("{text}: {GIT_FORM}")),
                "{text}"
            );
        }
    }
}
