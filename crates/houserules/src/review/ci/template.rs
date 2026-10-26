//! The request template of spec 7.4: its section, the place where each host reads a template, and
//! the place where `ci write --template` puts one.
//!
//! GitHub reads `pull_request_template.md` in the root of the repository, in `docs/`, and in
//! `.github/`, in any case, and the folders `PULL_REQUEST_TEMPLATE/` in those three places; GitLab
//! reads every `.md` file of `.gitlab/merge_request_templates/`. A folder counts as a template
//! only when it holds a file.

use std::fs;
use std::io;
use std::path::Path;

use super::Host;

/// The section of the template, byte for byte as spec 7.4 gives it.
pub(super) const SECTION: &str = concat!(
    "## Evidence bundle\n",
    "\n",
    "Purpose: <one or two sentences>\n",
    "Intent: <source>:<ref>\n",
    "Work items: <id>, <id>\n",
    "Risk tier: <tier>\n",
    "Criterion: <the criterion that selected the tier>\n",
    "Budget reason: <only when the diff is above the tier's budget>\n",
    "\n",
    "```houserules-run\n",
    "<the header line of a capture file>\n",
    "<the last lines of the run's output>\n",
    "```\n",
);

/// The pointer to `houserules bundle write` that the written template holds below the heading.
const POINTER: &str = "<!-- Write this section with houserules bundle write. It copies the header of each run from the capture file. -->";

/// The file that `ci write --template` creates, as a path below the git top level.
pub(super) fn target(host: Host) -> &'static str {
    match host {
        Host::Github => ".github/pull_request_template.md",
        Host::Gitlab => ".gitlab/merge_request_templates/Default.md",
    }
}

/// The text of the written template: [`SECTION`] with the pointer to `houserules bundle write` and a
/// blank line below the heading.
pub(super) fn written() -> String {
    let (heading, rest) = SECTION
        .split_once("\n\n")
        .expect("the section has a heading and a blank line");
    format!("{heading}\n\n{POINTER}\n\n{rest}")
}

/// The first request template that `host` reads below `top_level`, as its path from the top level
/// with `/` between the parts, or `None` when there is none. A folder of templates is named by the
/// path of the folder. A folder that cannot be read is one line with the error of the operating
/// system; a folder that is not there holds no template.
pub(super) fn existing(host: Host, top_level: &Path) -> Result<Option<String>, String> {
    match host {
        Host::Github => {
            for folder in ["", "docs", ".github"] {
                for (name, is_folder) in entries(top_level, folder)? {
                    let found = if is_folder {
                        name.eq_ignore_ascii_case("pull_request_template")
                            && holds_a_file(top_level, &join(folder, &name))?
                    } else {
                        name.eq_ignore_ascii_case("pull_request_template.md")
                    };
                    if found {
                        return Ok(Some(join(folder, &name)));
                    }
                }
            }
            Ok(None)
        }
        Host::Gitlab => {
            let folder = ".gitlab/merge_request_templates";
            Ok(entries(top_level, folder)?
                .into_iter()
                .find(|(name, is_folder)| {
                    !is_folder
                        && name.len() > 3
                        && name.as_bytes()[name.len() - 3..].eq_ignore_ascii_case(b".md")
                })
                .map(|(name, _)| join(folder, &name)))
        }
    }
}

/// `folder` and `name` as one path with `/` between them; `folder` is empty for the top level.
fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// The entries of the folder `folder` below `top_level`, sorted by name, each with whether it is a
/// folder. A folder that does not exist has none, and a name that is no UTF-8 is left out: no
/// template has one.
fn entries(top_level: &Path, folder: &str) -> Result<Vec<(String, bool)>, String> {
    let path = folder
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(top_level.to_path_buf(), |path, part| path.join(part));
    let listing = match fs::read_dir(&path) {
        Ok(listing) => listing,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) if error.kind() == io::ErrorKind::NotADirectory => return Ok(Vec::new()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let mut found = Vec::new();
    for entry in listing {
        let entry = entry.map_err(|error| format!("{}: {error}", path.display()))?;
        let is_folder = entry
            .file_type()
            .map_err(|error| format!("{}: {error}", entry.path().display()))?
            .is_dir();
        if let Some(name) = entry.file_name().to_str() {
            found.push((name.to_string(), is_folder));
        }
    }
    found.sort();
    Ok(found)
}

/// `true` when the folder `folder` below `top_level` holds a file that is not a folder.
fn holds_a_file(top_level: &Path, folder: &str) -> Result<bool, String> {
    Ok(entries(top_level, folder)?
        .iter()
        .any(|(_, is_folder)| !is_folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The written template is the section with the pointer below the heading, and the section
    /// stays as it is in the written text.
    #[test]
    fn the_written_template_holds_the_section_and_the_pointer() {
        let written = written();
        assert!(written.starts_with(&format!("## Evidence bundle\n\n{POINTER}\n\nPurpose:")));
        assert_eq!(written.replace(&format!("{POINTER}\n\n"), ""), SECTION);
    }

    /// A folder is joined to a name with one slash, and the top level adds none.
    #[test]
    fn a_path_below_the_top_level_has_no_leading_slash() {
        assert_eq!(join("", "a.md"), "a.md");
        assert_eq!(join("docs", "a.md"), "docs/a.md");
    }

    /// A template of GitHub is found in the three places, in any case, and the folder of
    /// templates counts only with a file in it.
    #[test]
    fn github_reads_its_three_places_in_any_case() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert_eq!(existing(Host::Github, dir.path()), Ok(None));
        fs::create_dir_all(dir.path().join(".github").join("Pull_Request_Template"))
            .expect("create the folder");
        assert_eq!(
            existing(Host::Github, dir.path()),
            Ok(None),
            "an empty folder"
        );
        fs::write(
            dir.path()
                .join(".github")
                .join("Pull_Request_Template")
                .join("a.md"),
            "x",
        )
        .expect("write a template");
        assert_eq!(
            existing(Host::Github, dir.path()),
            Ok(Some(".github/Pull_Request_Template".to_string()))
        );
        fs::write(dir.path().join("PULL_request_TEMPLATE.MD"), "x").expect("write");
        assert_eq!(
            existing(Host::Github, dir.path()),
            Ok(Some("PULL_request_TEMPLATE.MD".to_string())),
            "the root comes first"
        );
    }

    /// A template of GitLab is a `.md` file of its folder; another file and a folder are none.
    #[test]
    fn gitlab_reads_the_markdown_files_of_its_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let folder = dir.path().join(".gitlab").join("merge_request_templates");
        fs::create_dir_all(folder.join("nested.md")).expect("create folders");
        fs::write(folder.join("notes.txt"), "x").expect("write");
        assert_eq!(existing(Host::Gitlab, dir.path()), Ok(None));
        fs::write(folder.join("Bug.md"), "x").expect("write");
        assert_eq!(
            existing(Host::Gitlab, dir.path()),
            Ok(Some(".gitlab/merge_request_templates/Bug.md".to_string()))
        );
    }
}
