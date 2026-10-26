//! `houserules ci`: the CI wiring of the review layer for GitHub and GitLab (spec 10.2, 10.3, and
//! the template of 7.4).
//!
//! The binary embeds the two published units: the GitHub reusable workflow and the GitLab file for
//! `include: remote`. `ci print` prints the lines that call the unit of this release (the reference
//! form) or the jobs of the unit with the inputs of one install bound (the inline form), and writes
//! nothing. `ci write` creates a file of its own with the same text behind two comment lines, adds its
//! path to `review.ci.files`, and rewrites only a file that carries those lines. The command edits no
//! other CI file of a project. Both subcommands print the request template of spec 7.4 on request,
//! and `ci write` writes it where the host reads none.
//!
//! The text of a form comes from [`forms`], the template from [`template`], and the key of an
//! install from [`key`]. Paths of the repository are paths from its git top level, with `/` between
//! the parts, in every line the command prints and in `review.ci.files`.
//!
//! A refusal that spec 10.3 and 7.4 name (an existing file, an existing template) exits 1. Every
//! other failure is one line and exit 2 (`houserules.crash-paths-are-named`): a host that neither
//! `--host` nor the origin tells, an install that shares a key, a default branch that nothing
//! names (GitHub forms only), a `--sha` or an `--out` that cannot be used, a marker with a fault,
//! and a failure of git or of the file system, with the operating system's own text.

mod forms;
mod key;
mod template;

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use base64::prelude::{BASE64_STANDARD, Engine};
use clap::{Args, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};

use super::git;
use crate::install::ci_files::{is_written_by_ci_write, version_line};
use crate::install::marker::Marker;
use crate::install::marker::review::CI_WRITTEN_BY_LINE;
use crate::install::{detected_host, host_of_url, origin_url};
use crate::root::{INSTALL_MARKER, resolve_root};
use crate::rules::{InstallPrefix, default_branch_name, tracked_from_top_level};
use crate::tracker::Adapter;
use forms::Wiring;
use key::install_key;

/// The version of the binary, which every form pins.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The bytes of the GitHub unit, as the checkout that built this binary holds them.
const GITHUB_UNIT: &[u8] = include_bytes!("../../../../.github/workflows/houserules-review.yml");

/// The bytes of the GitLab unit, as the checkout that built this binary holds them. The
/// `integrity` value that `ci print` prints is the hash of these bytes.
const GITLAB_UNIT: &[u8] = include_bytes!("../../../../ci/gitlab-review.yml");

/// The closing line of `ci print` (spec 10.3).
const CLOSING_LINE: &str = "ci print: add the file that holds these lines to review.ci.files in .houserules.json, so that a change to it routes to senior-review";

/// The second line of `ci write` for GitLab (spec 10.3).
const GITLAB_CI_FILES_LINE: &str = "ci write: add .gitlab-ci.yml, which holds the include, to review.ci.files in .houserules.json, so that a change to it routes to senior-review";

/// The refusal of a repository with no `origin` remote (spec 10.3).
const NO_ORIGIN: &str = "ci: no origin remote; pass --host github or --host gitlab";

/// The refusal of a GitHub form where no default branch resolves (spec 10.3).
const NO_DEFAULT_BRANCH: &str =
    "ci: cannot resolve the default branch; set \"defaultBranch\" in .houserules.json";

/// The refusal of an `--out` that does not name a path below the top level.
const OUT_OUTSIDE: &str =
    "ci write: --out must name a path below the top level of the repository, with no .. part";

/// The hash of `bytes` as a GitLab `include:remote` states it: `sha256-` and the base64 of the
/// SHA-256 digest.
pub(crate) fn integrity(bytes: &[u8]) -> String {
    format!("sha256-{}", BASE64_STANDARD.encode(Sha256::digest(bytes)))
}

/// The host that a wiring is written for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Host {
    /// GitHub or a self-hosted GitHub.
    Github,
    /// GitLab or a self-managed GitLab.
    Gitlab,
}

/// How a form refers to the unit of the review layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Mode {
    /// The lines that call the unit of this release.
    Reference,
    /// The jobs of the unit, with the inputs of the install bound.
    Inline,
}

/// The arguments of `houserules ci`.
#[derive(Args)]
pub(crate) struct CiArgs {
    /// What to do with the wiring.
    #[command(subcommand)]
    action: CiAction,
}

impl CiArgs {
    /// The `--dir` by which D4 finds the install that the subcommand reads.
    pub(crate) fn dir(&self) -> &Option<PathBuf> {
        match &self.action {
            CiAction::Print(print) => &print.dir,
            CiAction::Write(write) => &write.dir,
        }
    }
}

/// The subcommands of `houserules ci`.
#[derive(Subcommand)]
enum CiAction {
    /// Prints the lines that wire the review layer into a pipeline, and writes nothing.
    /// `--host` is `github` or `gitlab`; it defaults to the host of the `origin` remote when that is
    /// a known host, and another origin or none needs it. `--mode reference`, the default, prints the
    /// lines that call the unit of this release: for GitHub a workflow that calls the reusable
    /// workflow, with the `<release commit sha>` to fill in and the `git ls-remote` command that
    /// resolves it; for GitLab an `include:` entry with the `integrity` value of the file. `--mode
    /// inline` prints the jobs of the unit with the inputs of this install filled in, for a project
    /// that adds them to its own pipeline file. `--template` prints the section of the evidence
    /// bundle for a request template instead. A GitHub form triggers on a push to the default branch
    /// by name: the `defaultBranch` of `.houserules.json`, else the branch that `origin/HEAD`
    /// names, else `main` or `master`, whichever `origin` or a local branch holds, as the
    /// `durable-sha` check looks for it. The lines to copy go to stdout. The closing line goes
    /// to stderr: it says to add the file that holds the lines to `review.ci.files` in
    /// `.houserules.json`, so that a change to it routes to senior-review. It exits 0. It exits 2
    /// for a setup error, for example: a host that neither `--host` nor the origin tells, two
    /// installs of the repository that share a key, a default branch that none of these places
    /// names, or a marker with a fault.
    Print(PrintArgs),
    /// Creates a file of its own that wires the review layer, and writes only a file of its own. The
    /// file goes below the git top level, by default `.github/workflows/houserules-review.yml` for
    /// GitHub and `.gitlab/houserules-review.gitlab-ci.yml` for GitLab, behind two comment lines
    /// that mark it and name the version of this binary. The command adds the path to
    /// `review.ci.files` in `.houserules.json`. `--host` and `--mode` are those of `ci print`. With
    /// `--host github --mode reference`, `--sha` is required: the commit of the release tag, which
    /// `git ls-remote https://github.com/jblossey/houserules refs/tags/v<version>` prints. A GitHub
    /// file triggers on a push to the default branch by name: the `defaultBranch` of
    /// `.houserules.json`, else the branch that `origin/HEAD` names, else `main` or `master`,
    /// whichever `origin` or a local branch holds, as the `durable-sha` check looks for it. For
    /// GitLab the command prints the include line for `.gitlab-ci.yml` and the line about
    /// `review.ci.files`. A path that exists is never written: the command exits 1 with one line,
    /// unless the file starts with the comment of `ci write` and `--force` is given. `--out` names
    /// another path below the top level. `--template` writes only the request template of the host
    /// (`.github/pull_request_template.md` or `.gitlab/merge_request_templates/Default.md`), and
    /// nothing in `review.ci.files`, where the host reads no template yet; where one exists it
    /// writes nothing and exits 1. It exits 2 for a setup error, for example: a host that neither
    /// `--host` nor the origin tells, a missing or malformed `--sha`, an `--out` outside the
    /// repository, two installs of the repository that share a key, a default branch that none of
    /// these places names, or a marker with a fault.
    Write(WriteArgs),
}

/// The arguments of `houserules ci print`.
#[derive(Args)]
struct PrintArgs {
    /// The host to wire; defaults to the host of the `origin` remote when that is a known host.
    #[arg(long, value_enum)]
    host: Option<Host>,
    /// How the lines refer to the unit: `reference` calls the unit of this release, `inline`
    /// holds its jobs with the inputs of this install bound.
    #[arg(long, value_enum, default_value_t = Mode::Reference, conflicts_with = "template")]
    mode: Mode,
    /// Print the section of the evidence bundle for a request template.
    #[arg(long)]
    template: bool,
    /// Install root to read; defaults to the nearest directory at or above the current one that
    /// holds `.houserules.json`, no higher than the git top level, else the git top level.
    #[arg(long)]
    dir: Option<PathBuf>,
}

/// The arguments of `houserules ci write`.
#[derive(Args)]
struct WriteArgs {
    /// The host to wire; defaults to the host of the `origin` remote when that is a known host.
    #[arg(long, value_enum)]
    host: Option<Host>,
    /// How the file refers to the unit: `reference` calls the unit of this release, `inline`
    /// holds its jobs with the inputs of this install bound.
    #[arg(long, value_enum, default_value_t = Mode::Reference, conflicts_with = "template")]
    mode: Mode,
    /// The commit of the release tag that a GitHub caller pins; needed for `--host github
    /// --mode reference` only.
    #[arg(long, value_name = "SHA", conflicts_with = "template")]
    sha: Option<String>,
    /// Write the request template of the host, and nothing else.
    #[arg(long)]
    template: bool,
    /// The path to write, below the git top level; defaults to the file of the host.
    #[arg(long, value_name = "PATH", conflicts_with = "template")]
    out: Option<PathBuf>,
    /// Rewrite the file at the path when it starts with the comment of `ci write`.
    #[arg(long, conflicts_with = "template")]
    force: bool,
    /// Install root to read; defaults to the nearest directory at or above the current one that
    /// holds `.houserules.json`, no higher than the git top level, else the git top level.
    #[arg(long)]
    dir: Option<PathBuf>,
}

/// Why a subcommand did not finish: the line it prints on stderr, and its exit code.
#[derive(Debug)]
enum Failure {
    /// A refusal that the spec names, exit 1.
    Refused(String),
    /// A setup error, exit 2.
    Setup(String),
}

impl Failure {
    /// The exit code of the failure.
    fn code(&self) -> u8 {
        match self {
            Failure::Refused(_) => 1,
            Failure::Setup(_) => 2,
        }
    }

    /// The text to print on stderr.
    fn text(&self) -> &str {
        match self {
            Failure::Refused(text) | Failure::Setup(text) => text,
        }
    }
}

/// A setup error from `text`, a line that does not name the command yet.
fn setup(text: impl std::fmt::Display) -> Failure {
    Failure::Setup(format!("ci: {text}"))
}

/// What a finished subcommand prints.
struct Printed {
    /// The text for stdout.
    stdout: String,
    /// The text for stderr.
    stderr: String,
}

/// One install, as the forms and the files need it.
struct Install {
    /// The install root.
    root: PathBuf,
    /// The git top level.
    top_level: PathBuf,
    /// The install path below the top level, `.` for the top level.
    path: String,
    /// The key of the install.
    key: String,
    /// The tracker input of the units: `github` or `beads`.
    tracker: Option<&'static str>,
    /// The marker of the install.
    marker: Marker,
}

impl Install {
    /// Reads the install at `root`: its marker, its place in the repository, and its key.
    fn read(root: &Path) -> Result<Self, Failure> {
        let marker = Marker::read_or_default_for_gate(root, "WI").map_err(setup)?;
        let prefix = InstallPrefix::of(root).map_err(setup)?;
        let top = git::run(root, &["rev-parse", "--show-toplevel"]).map_err(setup)?;
        let folder = prefix.as_str().trim_end_matches('/');
        let path = if folder.is_empty() { "." } else { folder }.to_string();
        let tracker = match marker
            .tracker()
            .map_err(setup)?
            .map(|config| config.adapter)
        {
            Some(Adapter::Github) => Some("github"),
            Some(Adapter::Beads) => Some("beads"),
            Some(Adapter::Json) | None => None,
        };
        Ok(Self {
            root: root.to_path_buf(),
            top_level: PathBuf::from(top.trim_end_matches(['\n', '\r'])),
            key: install_key(&path),
            path,
            tracker,
            marker,
        })
    }

    /// The facts that the forms are written for.
    fn wiring(&self) -> Wiring<'_> {
        Wiring {
            version: VERSION,
            path: &self.path,
            key: &self.key,
            tracker: self.tracker,
        }
    }

    /// The name of the default branch, on which a GitHub form triggers on a push (spec 10.3): the
    /// `defaultBranch` of the marker, else the branch that the repository names, as `durable-sha`
    /// looks for it. Where nothing names one, the command stops: a form never guesses.
    fn default_branch(&self) -> Result<String, Failure> {
        default_branch_name(&self.root, self.marker.default_branch())
            .map_err(setup)?
            .ok_or_else(|| Failure::Setup(NO_DEFAULT_BRANCH.to_string()))
    }

    /// Refuses the install when another install of the repository has its key (spec 10.1,
    /// design.md 258). The installs are the `.houserules.json` files that git tracks; the first in
    /// the order of git's listing is the one named.
    fn refuse_a_shared_key(&self) -> Result<(), Failure> {
        for file in tracked_from_top_level(&self.root).map_err(setup)? {
            let folder = if file == INSTALL_MARKER {
                ""
            } else if let Some(folder) = file.strip_suffix(&format!("/{INSTALL_MARKER}")) {
                folder
            } else {
                continue;
            };
            let other = if folder.is_empty() { "." } else { folder };
            if other != self.path && install_key(other) == self.key {
                return Err(Failure::Setup(format!(
                    "ci: {} and {other} share the key {}; rename one install folder",
                    self.path, self.key
                )));
            }
        }
        Ok(())
    }
}

/// The host to wire: the one `chosen` by `--host`, else the host of the `origin` remote when it is
/// a known host (spec 10.3).
fn resolve_host(chosen: Option<Host>, root: &Path) -> Result<Host, Failure> {
    if let Some(host) = chosen {
        return Ok(host);
    }
    match detected_host(root) {
        Some("github") => Ok(Host::Github),
        Some(_) => Ok(Host::Gitlab),
        None => Err(Failure::Setup(match origin_url(root) {
            None => NO_ORIGIN.to_string(),
            Some(url) => format!(
                "ci: the origin {} is no known host; pass --host github or --host gitlab",
                host_of_url(&url)
            ),
        })),
    }
}

/// The text of a form of `host` for `install`; `sha` pins a GitHub caller. Only a GitHub form
/// needs the default branch, so only it can fail.
fn form(host: Host, mode: Mode, install: &Install, sha: Option<&str>) -> Result<String, Failure> {
    let wiring = install.wiring();
    Ok(match (host, mode) {
        (Host::Github, Mode::Reference) => {
            forms::github_reference(&wiring, sha, &install.default_branch()?)
        }
        (Host::Gitlab, Mode::Reference) => {
            forms::gitlab_reference(&wiring, &integrity(GITLAB_UNIT))
        }
        (Host::Github, Mode::Inline) => {
            forms::github_inline(&wiring, unit_text(GITHUB_UNIT), &install.default_branch()?)
        }
        (Host::Gitlab, Mode::Inline) => forms::gitlab_inline(&wiring, unit_text(GITLAB_UNIT)),
    })
}

/// The text of an embedded unit.
fn unit_text(bytes: &'static [u8]) -> &'static str {
    std::str::from_utf8(bytes).expect("the embedded unit is UTF-8")
}

/// Runs `ci print`.
fn print(root: &Path, args: &PrintArgs) -> Result<Printed, Failure> {
    let install = Install::read(root)?;
    let host = resolve_host(args.host, root)?;
    install.refuse_a_shared_key()?;
    if args.template {
        return Ok(Printed {
            stdout: template::SECTION.to_string(),
            stderr: String::new(),
        });
    }
    Ok(Printed {
        stdout: form(host, args.mode, &install, None)?,
        stderr: format!("{CLOSING_LINE}\n"),
    })
}

/// The lowercase 40-digit commit sha that `--sha` gives for a GitHub caller.
fn checked_sha(sha: Option<&str>) -> Result<String, Failure> {
    let resolve = forms::ls_remote(VERSION);
    match sha {
        None => Err(Failure::Setup(format!(
            "ci write: --sha is required to pin the reusable workflow; resolve it with {resolve}"
        ))),
        Some(sha) if sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) => {
            Ok(sha.to_ascii_lowercase())
        }
        Some(_) => Err(Failure::Setup(format!(
            "ci write: --sha is no commit sha of 40 hexadecimal digits; resolve it with {resolve}"
        ))),
    }
}

/// The path to write, from the top level, with `/` between the parts: `out` when given, else the
/// file of `host`.
fn target_path(out: Option<&Path>, host: Host) -> Result<String, Failure> {
    let Some(out) = out else {
        return Ok(match host {
            Host::Github => ".github/workflows/houserules-review.yml",
            Host::Gitlab => ".gitlab/houserules-review.gitlab-ci.yml",
        }
        .to_string());
    };
    let mut parts = Vec::new();
    for component in out.components() {
        match component {
            Component::Normal(part) => match part.to_str() {
                Some(part) => parts.push(part),
                None => {
                    return Err(Failure::Setup(format!(
                        "ci write: {}: the path holds a name that is no UTF-8",
                        out.display()
                    )));
                }
            },
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(Failure::Setup(OUT_OUTSIDE.to_string()));
            }
        }
    }
    if parts.is_empty() {
        return Err(Failure::Setup(OUT_OUTSIDE.to_string()));
    }
    Ok(parts.join("/"))
}

/// `base` with the `/`-separated `relative` path below it, built one part at a time
/// (`houserules.path-pins-mirror-the-code`).
fn below(base: &Path, relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(base.to_path_buf(), |path, part| path.join(part))
}

/// The error of the file system at `path` as one setup line.
fn io_failure(path: &Path, error: &io::Error) -> Failure {
    setup(format!("{}: {error}", path.display()))
}

/// Creates the file `file`, or rewrites it when `overwrite` is set. A file that appeared since the
/// check is the refusal `refusal`. The folders above it are created.
fn put(file: &Path, text: &str, overwrite: bool, refusal: &str) -> Result<(), Failure> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent).map_err(|error| io_failure(parent, &error))?;
    }
    let mut options = OpenOptions::new();
    options.write(true);
    if overwrite {
        options.create(true).truncate(true);
    } else {
        options.create_new(true);
    }
    let mut handle = match options.open(file) {
        Ok(handle) => handle,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Err(Failure::Refused(refusal.to_string()));
        }
        Err(error) => return Err(io_failure(file, &error)),
    };
    handle
        .write_all(text.as_bytes())
        .map_err(|error| io_failure(file, &error))
}

/// Runs `ci write --template`: writes the template of `host` where the host reads none.
fn write_template(install: &Install, host: Host) -> Result<Printed, Failure> {
    if let Some(found) = template::existing(host, &install.top_level).map_err(setup)? {
        return Err(Failure::Refused(refusal_template_exists(&found)));
    }
    let target = template::target(host);
    put(
        &below(&install.top_level, target),
        &template::written(),
        false,
        &refusal_template_exists(target),
    )?;
    Ok(Printed {
        stdout: format!(
            "ci write: a GitLab project setting or a group template, or a GitHub organization template, takes priority over {target}\n"
        ),
        stderr: String::new(),
    })
}

/// The refusal of `ci write --template` where the template `path` exists (spec 7.4).
fn refusal_template_exists(path: &str) -> String {
    format!(
        "ci write: {path} exists; add the \"## Evidence bundle\" section that houserules bundle write prints"
    )
}

/// The refusal of `ci write` where `path` exists (spec 10.3).
fn refusal_exists(path: &str) -> String {
    format!(
        "ci write: {path} exists; houserules writes only a file of its own (use --out for another path)"
    )
}

/// Runs `ci write`.
fn write(root: &Path, args: &WriteArgs) -> Result<Printed, Failure> {
    let mut install = Install::read(root)?;
    let host = resolve_host(args.host, root)?;
    let sha = if !args.template && host == Host::Github && args.mode == Mode::Reference {
        Some(checked_sha(args.sha.as_deref())?)
    } else {
        None
    };
    install.refuse_a_shared_key()?;
    if args.template {
        return write_template(&install, host);
    }
    let target = target_path(args.out.as_deref(), host)?;
    let file = below(&install.top_level, &target);
    let overwrite = match fs::symlink_metadata(&file) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(io_failure(&file, &error)),
        Ok(_) if args.force && is_written_by_ci_write(&file).map_err(setup)? => true,
        Ok(_) => return Err(Failure::Refused(refusal_exists(&target))),
    };
    install.marker.review().map_err(|lines| {
        Failure::Setup(
            lines
                .iter()
                .map(|line| format!("ci: {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        )
    })?;
    let text = format!(
        "{CI_WRITTEN_BY_LINE}\n{}\n{}",
        version_line(VERSION),
        form(host, args.mode, &install, sha.as_deref())?
    );
    // The marker goes first: a stop between the two writes leaves a listed file that does not
    // exist, and the next run writes it without `--force`.
    if install.marker.add_ci_file(&target) {
        install.marker.write().map_err(setup)?;
    }
    put(&file, &text, overwrite, &refusal_exists(&target))?;
    let stdout = match host {
        Host::Github => String::new(),
        Host::Gitlab => format!("{}\n{GITLAB_CI_FILES_LINE}\n", forms::include_line(&target)),
    };
    Ok(Printed {
        stdout,
        stderr: String::new(),
    })
}

/// Prints what a finished subcommand printed, or its failure, and gives the exit code.
fn finish(result: Result<Printed, Failure>) -> ExitCode {
    match result {
        Ok(printed) => {
            print!("{}", printed.stdout);
            eprint!("{}", printed.stderr);
            ExitCode::SUCCESS
        }
        Err(failure) => {
            eprintln!("{}", failure.text());
            ExitCode::from(failure.code())
        }
    }
}

/// Runs `houserules ci`: the subcommand that `args` names.
pub(crate) fn cmd_ci(args: CiArgs) -> ExitCode {
    let dir = args.dir().clone();
    let root = match resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    finish(match &args.action {
        CiAction::Print(print_args) => print(&root, print_args),
        CiAction::Write(write_args) => write(&root, write_args),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash of the empty input is the well-known SHA-256 digest of nothing, in base64.
    #[test]
    fn the_integrity_of_nothing_is_the_digest_of_the_empty_input() {
        assert_eq!(
            integrity(b""),
            "sha256-47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU="
        );
    }

    /// The integrity of the embedded GitLab unit is the hash of its bytes, and the embedded bytes
    /// are the file of this repository.
    #[test]
    fn the_embedded_units_are_the_files_of_the_repository() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        assert_eq!(
            fs::read(root.join("ci/gitlab-review.yml")).expect("read the GitLab unit"),
            GITLAB_UNIT
        );
        assert_eq!(
            fs::read(root.join(".github/workflows/houserules-review.yml"))
                .expect("read the GitHub unit"),
            GITHUB_UNIT
        );
    }

    /// An `--out` keeps its parts, drops `.`, and refuses a part that leaves the top level.
    #[test]
    fn an_out_path_is_a_path_below_the_top_level() {
        assert_eq!(
            target_path(Some(Path::new("./ci/review.yml")), Host::Github)
                .as_deref()
                .ok(),
            Some("ci/review.yml")
        );
        for bad in ["", ".", "..", "a/../b", "/a"] {
            assert!(
                matches!(target_path(Some(Path::new(bad)), Host::Github), Err(Failure::Setup(line)) if line == OUT_OUTSIDE),
                "{bad:?}"
            );
        }
    }

    /// A sha is 40 hexadecimal digits and is written in lowercase.
    #[test]
    fn a_commit_sha_has_forty_hexadecimal_digits() {
        let sha = "0123456789ABCDEF0123456789abcdef01234567";
        assert_eq!(
            checked_sha(Some(sha)).as_deref().ok(),
            Some(sha.to_ascii_lowercase().as_str())
        );
        for bad in [None, Some(""), Some("main"), Some(&sha[1..])] {
            assert!(
                matches!(checked_sha(bad), Err(Failure::Setup(_))),
                "{bad:?}"
            );
        }
    }
}
