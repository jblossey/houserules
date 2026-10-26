//! The change analysis of a range: the changed paths with their kinds and
//! line ranges, the areas they touch, and the lines that no check covers
//! (spec 4).
//!
//! [`analyze`] computes the facts that `evidence`, `route`, `bundle check`, and
//! `record write` share. [`audit_rows`] gives the audit rows of the same range,
//! from a file or from an audit run in process, and [`coverage`] reads which
//! changed paths a covering check read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use globset::GlobMatcher;
use indexmap::IndexMap;

use super::{diff, git, rules_hash};
use crate::install::marker::review::ReviewConfig;
use crate::published::audit_result::{AuditResult, RowMode, RowResult};
use crate::published::change::{ChangeKind, EmptyChange, LineRange, UncoveredPath};
use crate::rules::{Base, InstallPrefix, audit_result, compile};

/// One path that a range changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangedPath {
    /// The path from the install root. For a deleted path, the path it had.
    pub(crate) path: String,
    /// The path before a rename, from the install root.
    pub(crate) old_path: Option<String>,
    /// What happened to the path.
    pub(crate) kind: ChangeKind,
    /// The lines added; 0 for a kind that adds no line to the diff size.
    pub(crate) added: u64,
    /// The lines deleted; 0 for a kind that deletes no line from the diff
    /// size.
    pub(crate) deleted: u64,
    /// The lines that the hunks touch, in the order of the diff.
    pub(crate) ranges: Vec<LineRange>,
    /// Whether a glob of `review.diffExclude` matches the path.
    pub(crate) excluded: bool,
    /// For the kind `empty` only: whether the range adds or deletes the file.
    pub(crate) empty: Option<EmptyChange>,
}

impl ChangedPath {
    /// The lines that this path counts for the uncovered share (spec 4.4): the
    /// added and deleted lines of a text, a deleted, and a symlink path, and
    /// one line for every other kind.
    pub(crate) fn changed_lines(&self) -> u64 {
        match self.kind {
            ChangeKind::Text | ChangeKind::Deleted | ChangeKind::Symlink => {
                self.added + self.deleted
            }
            ChangeKind::Renamed
            | ChangeKind::Empty
            | ChangeKind::Binary
            | ChangeKind::Mode
            | ChangeKind::Submodule => 1,
        }
    }
}

impl From<&ChangedPath> for UncoveredPath {
    fn from(path: &ChangedPath) -> Self {
        Self {
            path: path.path.clone(),
            old_path: path.old_path.clone(),
            kind: path.kind,
            ranges: path.ranges.clone(),
            empty: path.empty,
        }
    }
}

/// The size of the diff of a range (spec 4.3), without the excluded paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DiffSize {
    /// The paths that count.
    pub(crate) files: u64,
    /// The lines added.
    pub(crate) added: u64,
    /// The lines deleted.
    pub(crate) deleted: u64,
    /// The lines added and deleted.
    pub(crate) lines: u64,
    /// The paths that a glob of `review.diffExclude` leaves out.
    pub(crate) excluded: u64,
}

/// Whether a commit holds rule sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RulesPresence {
    /// The commit holds rule sources.
    Present,
    /// The commit holds none.
    Absent,
}

impl RulesPresence {
    /// The word that the change record prints.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            RulesPresence::Present => "present",
            RulesPresence::Absent => "absent",
        }
    }
}

/// The hashes of the rule sources at the two ends of a range (spec 4.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RulesHashes {
    /// The hash at the merge base, or `None` when the merge base holds no rule
    /// source or the range has none.
    pub(crate) merge_base_hash: Option<String>,
    /// `Absent` exactly when `merge_base_hash` is `None`.
    pub(crate) merge_base_rules: RulesPresence,
    /// The hash at the head.
    pub(crate) head_hash: String,
}

/// The facts of a range that the review commands share (spec 4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Analysis {
    /// The path of the install from the git top level; `.` at the top.
    pub(crate) install: String,
    /// The full sha of the commit that the base names, or the name of the empty
    /// tree for a range from the start of the history.
    pub(crate) base: String,
    /// The full sha of the head.
    pub(crate) head: String,
    /// The full sha of the merge base; `None` for a range from the empty tree.
    pub(crate) merge_base: Option<String>,
    /// The changed paths below the install root, sorted by path.
    pub(crate) paths: Vec<ChangedPath>,
    /// The size of the diff.
    pub(crate) diff: DiffSize,
    /// The hashes of the rule sources.
    pub(crate) rules: RulesHashes,
    /// The `Work-Item` ids of the range, in commit order, without duplicates.
    pub(crate) work_items: Vec<String>,
    /// For each area of `knowledge/areas.json` that a changed path touches, the
    /// paths, sorted. A deleted path counts by its old path, and a renamed path
    /// by both.
    pub(crate) areas: IndexMap<String, Vec<String>>,
    /// The paths, old paths included, that match no area, sorted.
    pub(crate) no_area: Vec<String>,
    /// The changed rule sources: paths of `knowledge/` and `.houserules.json`
    /// from the install root, and CI files from the git top level, sorted.
    pub(crate) rule_sources_changed: Vec<String>,
}

/// The analysis of the range `base_ref` to `head_ref` of the install at
/// `root` (spec 4).
///
/// The range runs from the merge base of the two commits to the head, as
/// `audit` reads it. A `base_ref` that git resolves to its empty tree, in any
/// spelling, gives the whole history of the head, and its audit rows audit the
/// same range (see [`audit_rows`]). A caller checks the git version with
/// [`git::require_attr_source`] first, under its own command name.
///
/// A rule source of the install is a changed path of its `knowledge/` folder
/// or its `.houserules.json`. A CI file (spec 4.1) counts when it is one at
/// either end of the range, even outside the install root; the list holds its
/// top-level path. The CI files of the merge base and of the head both
/// decide, so a file that `ci write` created and the range deleted or
/// un-marked counts too.
pub(crate) fn analyze(
    root: &Path,
    base: &Base,
    config: &ReviewConfig,
    base_ref: &str,
    head_ref: &str,
) -> Result<Analysis, String> {
    let prefix = InstallPrefix::of(root)?;
    let install = install_name(&prefix);
    let empty_tree = git::empty_tree(root)?;
    let head = git::resolve_commit(root, head_ref)?;
    let base_commit = git::resolve_start(root, base_ref, &empty_tree)?;
    let merge_base = if base_commit == empty_tree {
        None
    } else {
        let merge_base = git::merge_base(root, &base_commit, &head)?
            .ok_or_else(|| format!("no merge base between \"{base_ref}\" and \"{head_ref}\""))?;
        Some(merge_base)
    };
    let from = merge_base.as_deref().unwrap_or(&empty_tree);

    let mut paths = diff::read_range(root, &empty_tree, from, &head, prefix.as_str())?;
    mark_excluded(&mut paths, &config.diff_exclude)?;
    let diff = diff_size(&paths);

    let head_ci_files = rules_hash::ci_files_at(root, &install, &head)?;
    let head_hash = rules_hash::hash_with(root, &install, &head, &head_ci_files)?
        .ok_or_else(|| format!("no rule sources at {}", git::short(&head)))?;
    let mut ci_files = head_ci_files;
    let merge_base_hash = match &merge_base {
        Some(commit) => {
            let merge_base_ci_files = rules_hash::ci_files_at(root, &install, commit)?;
            let hash = rules_hash::hash_with(root, &install, commit, &merge_base_ci_files)?;
            ci_files.extend(merge_base_ci_files);
            hash
        }
        None => None,
    };
    ci_files.sort();
    ci_files.dedup();
    let merge_base_rules = if merge_base_hash.is_some() {
        RulesPresence::Present
    } else {
        RulesPresence::Absent
    };

    let AreaMap { areas, no_area } = areas_of(base, &paths)?;
    let mut rule_sources: BTreeSet<String> = paths
        .iter()
        .flat_map(names_of)
        .filter(|name| *name == ".houserules.json" || name.starts_with("knowledge/"))
        .map(str::to_string)
        .collect();
    rule_sources.extend(changed_ci_files(root, from, &head, &ci_files)?);

    Ok(Analysis {
        work_items: work_items(root, &base_commit, &head, merge_base.is_none())?,
        install,
        base: base_commit,
        head,
        merge_base,
        paths,
        diff,
        rules: RulesHashes {
            merge_base_hash,
            merge_base_rules,
            head_hash,
        },
        areas,
        no_area,
        rule_sources_changed: rule_sources.into_iter().collect(),
    })
}

/// The path of the install at `root` from the git top level, `.` for a
/// top-level install: the `install` of every review document.
pub(crate) fn install_path(root: &Path) -> Result<String, String> {
    InstallPrefix::of(root).map(|prefix| install_name(&prefix))
}

/// The install path of `prefix`: its folders without the closing `/`, or `.`
/// for the top level.
fn install_name(prefix: &InstallPrefix) -> String {
    match prefix.as_str().trim_end_matches('/') {
        "" => ".".to_string(),
        folder => folder.to_string(),
    }
}

/// The names of a changed path: its path, and its old path when it has one.
fn names_of(path: &ChangedPath) -> impl Iterator<Item = &str> {
    std::iter::once(path.path.as_str()).chain(path.old_path.as_deref())
}

/// Sets `excluded` on the paths that a glob of `globs` matches.
fn mark_excluded(paths: &mut [ChangedPath], globs: &[String]) -> Result<(), String> {
    let matchers = globs
        .iter()
        .map(|glob| compile(glob).map_err(|error| format!("review.diffExclude: {error}")))
        .collect::<Result<Vec<_>, _>>()?;
    for path in paths {
        path.excluded = matchers.iter().any(|matcher| matcher.is_match(&path.path));
    }
    Ok(())
}

/// The size of the diff without the excluded paths (spec 4.3).
fn diff_size(paths: &[ChangedPath]) -> DiffSize {
    let counted = paths.iter().filter(|path| !path.excluded);
    let (files, added, deleted) = counted.fold((0, 0, 0), |(files, added, deleted), path| {
        (files + 1, added + path.added, deleted + path.deleted)
    });
    DiffSize {
        files,
        added,
        deleted,
        lines: added + deleted,
        excluded: paths.len() as u64 - files,
    }
}

/// The areas that the names of the changed paths match, and the names that
/// match none (spec 4.7). Both come sorted.
struct AreaMap {
    areas: IndexMap<String, Vec<String>>,
    no_area: Vec<String>,
}

/// The [`AreaMap`] of the names of `paths`.
fn areas_of(base: &Base, paths: &[ChangedPath]) -> Result<AreaMap, String> {
    let mut matchers = Vec::with_capacity(base.areas.len());
    for (area, definition) in &base.areas {
        let globs = definition
            .paths
            .iter()
            .map(|glob| compile(glob).map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        matchers.push((area, globs));
    }
    let mut by_area: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut no_area: BTreeSet<&str> = BTreeSet::new();
    for name in paths.iter().flat_map(names_of) {
        let mut matched = false;
        for (area, globs) in &matchers {
            if globs.iter().any(|glob| glob.is_match(name)) {
                by_area.entry(area).or_default().insert(name);
                matched = true;
            }
        }
        if !matched {
            no_area.insert(name);
        }
    }
    let areas = by_area
        .into_iter()
        .map(|(area, names)| {
            (
                area.to_string(),
                names.into_iter().map(str::to_string).collect(),
            )
        })
        .collect();
    Ok(AreaMap {
        areas,
        no_area: no_area.into_iter().map(str::to_string).collect(),
    })
}

/// The `Work-Item` ids of the commits of the range, oldest first, each once.
/// A range from the empty tree holds every commit of `head`; any other holds
/// the commits of `head` that `base` does not reach. git reads the trailers
/// with its own rule and the separator `:`, as `audit` does.
fn work_items(
    root: &Path,
    base: &str,
    head: &str,
    whole_history: bool,
) -> Result<Vec<String>, String> {
    let range = if whole_history {
        head.to_string()
    } else {
        format!("{base}..{head}")
    };
    let output = git::run(
        root,
        &[
            "-c",
            "trailer.separators=:",
            "log",
            "-z",
            "--reverse",
            "--format=%(trailers:key=Work-Item,valueonly,unfold,separator=%x0a)",
            &range,
            "--",
        ],
    )?;
    let mut seen = BTreeSet::new();
    let mut ids = Vec::new();
    for id in output
        .split(['\0', '\n'])
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if seen.insert(id) {
            ids.push(id.to_string());
        }
    }
    Ok(ids)
}

/// The CI files of `ci_files` that the range `from` to `to` changes, as
/// top-level paths. A rename counts by both names.
fn changed_ci_files(
    root: &Path,
    from: &str,
    to: &str,
    ci_files: &[String],
) -> Result<Vec<String>, String> {
    let pathspecs: Vec<String> = ci_files
        .iter()
        .map(|file| format!(":(top,literal){file}"))
        .collect();
    let mut args = vec![
        "diff",
        "--name-only",
        "--no-renames",
        "--no-relative",
        "-z",
        from,
        to,
        "--",
    ];
    args.extend(pathspecs.iter().map(String::as_str));
    let output = git::run(root, &args)?;
    Ok(output
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect())
}

/// Where the audit rows of a range come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuditSource {
    /// The `audit --json` file of the range.
    File(PathBuf),
    /// An audit that runs in process.
    InProcess,
}

/// The audit rows of the range of `analysis`. An in-process audit of a range
/// from git's empty tree audits every path and every commit of the head.
pub(crate) fn audit_rows(
    root: &Path,
    base: &Base,
    source: AuditSource,
    analysis: &Analysis,
) -> Result<AuditResult, String> {
    match source {
        AuditSource::InProcess => audit_result(base, &analysis.base, &analysis.head),
        AuditSource::File(file) => read_audit_file(root, &file, analysis),
    }
}

/// The audit result in `file`, when it is the audit of the range of `analysis`.
/// The base and the head of the file must name the same commits as the
/// analysis does.
fn read_audit_file(root: &Path, file: &Path, analysis: &Analysis) -> Result<AuditResult, String> {
    let text =
        std::fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let result: AuditResult = serde_json::from_str(&text)
        .map_err(|error| format!("{}: not an audit result: {error}", file.display()))?;
    let empty_tree = git::empty_tree(root)?;
    let names = |recorded: &str, actual: &str| {
        git::resolve_start(root, recorded, &empty_tree).is_ok_and(|name| name == actual)
    };
    if names(&result.base, &analysis.base) && names(&result.head, &analysis.head) {
        Ok(result)
    } else {
        Err(format!(
            "{} is the audit of {}..{}, not of {}..{}",
            file.display(),
            git::short(&result.base),
            git::short(&result.head),
            git::short(&analysis.base),
            git::short(&analysis.head)
        ))
    }
}

/// The part of a change that no covering check read (spec 4.4).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Coverage {
    /// The paths that no covering check read, in path order.
    pub(crate) uncovered: Vec<ChangedPath>,
    /// The changed lines of the uncovered paths.
    pub(crate) uncovered_lines: u64,
    /// The changed lines of all paths.
    pub(crate) total_lines: u64,
    /// `uncovered_lines` over `total_lines`; 0 when there is no changed line.
    pub(crate) share: f64,
}

/// The coverage of the changed paths of `analysis` by the rows of `audit`
/// (spec 4.4, per row). A path is covered when a covering check that read it
/// has a deterministic row with the result `pass` or `warn`; a failed,
/// skipped, or judged row covers nothing, and neither does a check that does
/// not declare `covers: true`. The excluded paths leave both sides of the
/// share. A glob of a check that does not compile reads no path: the audit
/// stops on such a glob, so a passing row never holds one.
pub(crate) fn coverage(analysis: &Analysis, audit: &AuditResult, base: &Base) -> Coverage {
    let readers: Vec<GlobMatcher> = audit
        .rules
        .iter()
        .filter(|row| {
            row.mode == RowMode::Deterministic
                && matches!(row.result, RowResult::Pass | RowResult::Warn)
        })
        .flat_map(|row| base.covering_globs(&row.id))
        .filter_map(|glob| compile(&glob).ok())
        .collect();
    let mut uncovered = Vec::new();
    let mut uncovered_lines = 0;
    let mut total_lines = 0;
    for path in analysis.paths.iter().filter(|path| !path.excluded) {
        let lines = path.changed_lines();
        total_lines += lines;
        if !readers.iter().any(|reader| reader.is_match(&path.path)) {
            uncovered_lines += lines;
            uncovered.push(path.clone());
        }
    }
    let share = if total_lines == 0 {
        0.0
    } else {
        uncovered_lines as f64 / total_lines as f64
    };
    Coverage {
        uncovered,
        uncovered_lines,
        total_lines,
        share,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::{Value, json};

    use super::super::git::{self, scratch::Repo};
    use super::*;
    use crate::published::audit_result::{
        AuditRow, References, RowLevel, RowMode, RowResult, Summary,
    };
    use crate::rules::load_base;

    /// An install in a scratch repository: `folder` (`""` or `sub/`) holds a
    /// loadable knowledge base with `areas` and `entries`, and a marker.
    struct Install {
        repo: Repo,
        folder: &'static str,
    }

    impl Install {
        /// A repository with the install and one commit that holds it.
        fn new(folder: &'static str, areas: Value, entries: &[Value]) -> Self {
            let repo = Repo::new();
            repo.write(&format!("{folder}knowledge/schema.json"), "{}");
            repo.write(&format!("{folder}knowledge/areas.json"), &areas.to_string());
            repo.write(
                &format!("{folder}knowledge/demo.json"),
                &json!({"entries": entries}).to_string(),
            );
            repo.write(&format!("{folder}.houserules.json"), "{}\n");
            let install = Self { repo, folder };
            install.repo.commit("base");
            install
        }

        /// The install root.
        fn root(&self) -> PathBuf {
            if self.folder.is_empty() {
                self.repo.root().to_path_buf()
            } else {
                self.repo.root().join(self.folder.trim_end_matches('/'))
            }
        }

        /// The knowledge base of the install, loaded from the work tree.
        fn base(&self) -> Base {
            load_base(&self.root()).expect("load the base")
        }

        /// Writes `content` to `path` below the install root.
        fn write(&self, path: &str, content: &str) {
            self.repo.write(&format!("{}{path}", self.folder), content);
        }

        /// The analysis of `base_ref` to `head_ref` with the default
        /// configuration.
        fn analyze(&self, base_ref: &str, head_ref: &str) -> Result<Analysis, String> {
            self.analyze_with(&ReviewConfig::default(), base_ref, head_ref)
        }

        /// Like [`Install::analyze`], with `config`.
        fn analyze_with(
            &self,
            config: &ReviewConfig,
            base_ref: &str,
            head_ref: &str,
        ) -> Result<Analysis, String> {
            analyze(&self.root(), &self.base(), config, base_ref, head_ref)
        }
    }

    /// An entry whose check is `check`.
    fn checked(id: &str, check: Value) -> Value {
        json!({
            "id": id, "kind": "rule", "area": "global", "standing": true,
            "summary": "A rule.", "body": ["Body."], "tags": [],
            "source": {"date": "2026-10-24", "by": "test"},
            "check": check,
        })
    }

    /// The areas `docs` and `rust`.
    fn two_areas() -> Value {
        json!({"global": {"paths": []}, "docs": {"paths": ["docs/**"]}, "rust": {"paths": ["crates/**"]}})
    }

    /// A text path with the given counts, for the tests of the sums.
    fn text_path(path: &str, added: u64, deleted: u64) -> ChangedPath {
        ChangedPath {
            path: path.to_string(),
            old_path: None,
            kind: ChangeKind::Text,
            added,
            deleted,
            ranges: Vec::new(),
            excluded: false,
            empty: None,
        }
    }

    #[test]
    fn a_changed_path_counts_its_lines_by_its_kind() {
        for (kind, added, deleted, changed) in [
            (ChangeKind::Text, 2, 3, 5),
            (ChangeKind::Deleted, 0, 4, 4),
            (ChangeKind::Symlink, 1, 1, 2),
            (ChangeKind::Renamed, 0, 0, 1),
            (ChangeKind::Empty, 0, 0, 1),
            (ChangeKind::Binary, 0, 0, 1),
            (ChangeKind::Mode, 0, 0, 1),
            (ChangeKind::Submodule, 0, 0, 1),
        ] {
            let path = ChangedPath {
                kind,
                ..text_path("x", added, deleted)
            };

            assert_eq!(path.changed_lines(), changed, "{kind:?}");
        }
    }

    #[test]
    fn an_uncovered_path_keeps_the_fields_of_its_changed_path() {
        let path = ChangedPath {
            old_path: Some("old.rs".to_string()),
            kind: ChangeKind::Renamed,
            ranges: vec![LineRange::Lines { start: 1, end: 2 }],
            excluded: true,
            empty: Some(EmptyChange::Added),
            ..text_path("new.rs", 1, 1)
        };

        let uncovered = UncoveredPath::from(&path);

        assert_eq!(
            uncovered,
            UncoveredPath {
                path: "new.rs".to_string(),
                old_path: Some("old.rs".to_string()),
                kind: ChangeKind::Renamed,
                ranges: vec![LineRange::Lines { start: 1, end: 2 }],
                empty: Some(EmptyChange::Added),
            }
        );
    }

    #[test]
    fn the_presence_of_rules_prints_as_the_record_words_it() {
        assert_eq!(RulesPresence::Present.as_str(), "present");
        assert_eq!(RulesPresence::Absent.as_str(), "absent");
    }

    #[test]
    fn an_analysis_names_the_range_the_install_and_the_changed_paths() {
        let install = Install::new("", two_areas(), &[]);
        let base = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "one\ntwo\n");
        let head = install.repo.commit("docs");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(analysis.install, ".");
        assert_eq!(analysis.base, base);
        assert_eq!(analysis.head, head);
        assert_eq!(analysis.merge_base, Some(base));
        assert_eq!(analysis.paths.len(), 1);
        assert_eq!(analysis.paths[0].path, "docs/a.md");
        assert_eq!(
            analysis.diff,
            DiffSize {
                files: 1,
                added: 2,
                deleted: 0,
                lines: 2,
                excluded: 0
            }
        );
    }

    #[test]
    fn the_range_runs_from_the_merge_base_not_from_the_tip_of_the_base() {
        let install = Install::new("", two_areas(), &[]);
        let fork = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.repo.git(&["checkout", "-q", "-b", "feature"]);
        install.write("docs/feature.md", "feature\n");
        let feature = install.repo.commit("feature");
        install.repo.git(&["checkout", "-q", "main"]);
        install.write("docs/main.md", "main\n");
        let main = install.repo.commit("main moves on");

        let analysis = install.analyze(&main, &feature).expect("analyze");

        assert_eq!(analysis.merge_base, Some(fork));
        assert_eq!(analysis.base, main);
        assert_eq!(
            analysis
                .paths
                .iter()
                .map(|path| path.path.as_str())
                .collect::<Vec<_>>(),
            ["docs/feature.md"]
        );
    }

    #[test]
    fn a_head_behind_its_base_has_no_changed_path() {
        let install = Install::new("", two_areas(), &[]);
        let behind = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "one\n");
        let ahead = install.repo.commit("ahead");

        let analysis = install.analyze(&ahead, &behind).expect("analyze");

        assert_eq!(analysis.merge_base, Some(behind));
        assert!(analysis.paths.is_empty());
        assert_eq!(analysis.diff.lines, 0);
        assert!(analysis.work_items.is_empty());
        assert!(analysis.rule_sources_changed.is_empty());
    }

    #[test]
    fn histories_with_no_common_commit_have_no_merge_base() {
        let install = Install::new("", two_areas(), &[]);
        let main = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.repo.git(&["checkout", "-q", "--orphan", "other"]);
        let other = install.repo.commit("other");

        let error = install.analyze(&main, &other).expect_err("no merge base");

        assert_eq!(
            error,
            format!("no merge base between \"{main}\" and \"{other}\"")
        );
    }

    #[test]
    fn a_bad_ref_is_a_named_error() {
        let install = Install::new("", two_areas(), &[]);

        assert_eq!(
            install.analyze("no-such-ref", "HEAD").unwrap_err(),
            "bad ref \"no-such-ref\""
        );
        assert_eq!(
            install.analyze("HEAD", "no-such-ref").unwrap_err(),
            "bad ref \"no-such-ref\""
        );
    }

    #[test]
    fn the_empty_tree_as_the_base_gives_the_whole_history() {
        let install = Install::new("", two_areas(), &[]);
        install.repo.git(&[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "first\n\nWork-Item: HR-1",
        ]);
        install.write("docs/a.md", "one\n");
        let head = install.repo.commit("docs\n\nWork-Item: HR-2");
        let empty = git::empty_tree(install.repo.root()).expect("the empty tree");

        let analysis = install.analyze(&empty, &head).expect("analyze");

        assert_eq!(analysis.base, empty);
        assert_eq!(analysis.merge_base, None);
        assert_eq!(analysis.work_items, ["HR-1", "HR-2"]);
        assert!(analysis.paths.iter().any(|path| path.path == "docs/a.md"));
        assert!(
            analysis
                .paths
                .iter()
                .any(|path| path.path == "knowledge/areas.json")
        );
        assert_eq!(analysis.rules.merge_base_hash, None);
        assert_eq!(analysis.rules.merge_base_rules, RulesPresence::Absent);
    }

    /// A repository whose first commit holds nothing, so that its tree is
    /// git's empty tree, and whose second commit holds the install.
    fn install_after_an_empty_root() -> (Install, String, String) {
        let repo = Repo::new();
        let root = repo.commit("root");
        let install = Install { repo, folder: "" };
        install.write("knowledge/schema.json", "{}");
        install.write("knowledge/areas.json", &two_areas().to_string());
        install.write("knowledge/demo.json", "{\"entries\": []}");
        install.write(".houserules.json", "{}\n");
        install.write("docs/a.md", "one\n");
        let head = install.repo.commit("install\n\nWork-Item: HR-1");
        (install, root, head)
    }

    #[test]
    fn the_empty_tree_in_any_spelling_that_git_resolves_starts_the_whole_history() {
        let (install, root, head) = install_after_an_empty_root();
        let empty = git::empty_tree(install.repo.root()).expect("the empty tree");
        install
            .repo
            .git(&["hash-object", "-w", "-t", "tree", "--stdin"]);
        install.repo.git(&["tag", "empty", &empty]);

        for spelling in [
            empty.clone(),
            empty[..12].to_string(),
            "empty".to_string(),
            format!("{root}^{{tree}}"),
        ] {
            let analysis = install.analyze(&spelling, &head).expect(&spelling);

            assert_eq!(analysis.base, empty, "{spelling}");
            assert_eq!(analysis.merge_base, None, "{spelling}");
            assert!(
                analysis.paths.iter().any(|path| path.path == "docs/a.md"),
                "{spelling}"
            );
            assert_eq!(analysis.work_items, ["HR-1"], "{spelling}");
        }
    }

    #[test]
    fn a_name_that_resolves_to_a_tree_of_content_is_a_bad_ref() {
        let (install, _, head) = install_after_an_empty_root();

        assert_eq!(
            install
                .analyze(&format!("{head}^{{tree}}"), &head)
                .unwrap_err(),
            format!("bad ref \"{head}^{{tree}}\"")
        );
    }

    #[test]
    fn an_empty_tree_range_audits_every_path_of_the_head() {
        let (install, _, head) = install_after_an_empty_root();
        let empty = git::empty_tree(install.repo.root()).expect("the empty tree");
        let (analysis, rows) = in_process(&install, &empty, &head);

        let mut tracked: Vec<String> = install
            .repo
            .git(&["ls-tree", "-r", "--name-only", "HEAD"])
            .lines()
            .map(str::to_string)
            .collect();
        tracked.sort();
        let mut audited = rows.changed_files.clone();
        audited.sort();
        assert_eq!(audited, tracked);
        assert_eq!(rows.base, analysis.base);
        assert!(analysis.head.starts_with(&rows.head));
        assert_eq!(rows.references.skipped, 1);
    }

    #[test]
    fn an_audit_file_of_an_empty_tree_range_is_read_back_unchanged() {
        let (install, _, head) = install_after_an_empty_root();
        let empty = git::empty_tree(install.repo.root()).expect("the empty tree");
        let (analysis, rows) = in_process(&install, &empty, &head);
        let file = install.root().join("audit.json");
        std::fs::write(&file, serde_json::to_string(&rows).expect("serialize")).expect("write");

        let read = audit_rows(
            &install.root(),
            &install.base(),
            AuditSource::File(file),
            &analysis,
        )
        .expect("read the audit file");

        assert_eq!(read, rows);
    }

    #[test]
    fn the_diff_size_leaves_out_the_paths_that_match_diff_exclude() {
        let install = Install::new("", two_areas(), &[]);
        let base = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "one\ntwo\n");
        install.write("Cargo.lock", "l1\nl2\nl3\nl4\n");
        install.write("docs/b.md", "x\n");
        let head = install.repo.commit("change");
        let config = ReviewConfig {
            diff_exclude: vec!["Cargo.lock".to_string(), "docs/b.*".to_string()],
            ..ReviewConfig::default()
        };

        let analysis = install
            .analyze_with(&config, &base, &head)
            .expect("analyze");

        assert_eq!(
            analysis.diff,
            DiffSize {
                files: 1,
                added: 2,
                deleted: 0,
                lines: 2,
                excluded: 2
            }
        );
        let flags: Vec<(&str, bool)> = analysis
            .paths
            .iter()
            .map(|path| (path.path.as_str(), path.excluded))
            .collect();
        assert_eq!(
            flags,
            [
                ("Cargo.lock", true),
                ("docs/a.md", false),
                ("docs/b.md", true)
            ]
        );
    }

    #[test]
    fn an_invalid_diff_exclude_glob_is_a_named_error() {
        let install = Install::new("", two_areas(), &[]);
        let config = ReviewConfig {
            diff_exclude: vec!["a[z-a]b".to_string()],
            ..ReviewConfig::default()
        };

        let error = install.analyze_with(&config, "HEAD", "HEAD").unwrap_err();

        assert!(
            error.starts_with("review.diffExclude: invalid glob \"a[z-a]b\""),
            "{error}"
        );
    }

    #[test]
    fn the_rules_hashes_read_the_merge_base_and_the_head() {
        let install = Install::new("", two_areas(), &[]);
        let base = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("knowledge/more.json", "{}\n");
        let head = install.repo.commit("more knowledge");

        let analysis = install.analyze(&base, &head).expect("analyze");

        let at = |commit: &str| {
            crate::review::rules_hash::rules_hash(install.repo.root(), ".", commit)
                .expect("hash")
                .expect("a hash")
        };
        assert_eq!(analysis.rules.merge_base_hash, Some(at(&base)));
        assert_eq!(analysis.rules.merge_base_rules, RulesPresence::Present);
        assert_eq!(analysis.rules.head_hash, at(&head));
        assert_ne!(analysis.rules.head_hash, at(&base));
    }

    #[test]
    fn a_merge_base_with_no_rule_source_gives_a_null_hash_and_absent_rules() {
        let repo = Repo::new();
        repo.write("README.md", "readme\n");
        let base = repo.commit("before the install");
        repo.write("knowledge/schema.json", "{}");
        repo.write("knowledge/areas.json", "{}");
        let head = repo.commit("install");
        let loaded = load_base(repo.root()).expect("load the base");

        let analysis =
            analyze(repo.root(), &loaded, &ReviewConfig::default(), &base, &head).expect("analyze");

        assert_eq!(analysis.rules.merge_base_hash, None);
        assert_eq!(analysis.rules.merge_base_rules, RulesPresence::Absent);
        assert_eq!(analysis.rules.head_hash.len(), 64);
    }

    #[test]
    fn a_head_with_no_rule_source_is_a_named_error() {
        let repo = Repo::new();
        repo.write("README.md", "readme\n");
        let base = repo.commit("one");
        repo.write("README.md", "two\n");
        let head = repo.commit("two");

        let error = analyze(
            repo.root(),
            &Base::default(),
            &ReviewConfig::default(),
            &base,
            &head,
        )
        .unwrap_err();

        assert_eq!(error, format!("no rule sources at {}", git::short(&head)));
    }

    #[test]
    fn the_rules_hash_of_a_subdirectory_install_matches_the_hash_from_the_top() {
        let install = Install::new("sub/", two_areas(), &[]);
        let head = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();

        let analysis = install.analyze(&head, &head).expect("analyze");

        assert_eq!(analysis.install, "sub");
        assert_eq!(
            Some(analysis.rules.head_hash),
            crate::review::rules_hash::rules_hash(install.repo.root(), "sub", &head).expect("hash")
        );
    }

    #[test]
    fn the_work_items_come_in_commit_order_without_duplicates() {
        let install = Install::new("", two_areas(), &[]);
        let base = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "1\n");
        install.repo.commit("one\n\nWork-Item: HR-2");
        install.write("docs/a.md", "2\n");
        install
            .repo
            .commit("two\n\nSubject text.\n\nWork-Item: HR-1\nwork-item: HR-2\nOther: x");
        install.write("docs/a.md", "3\n");
        install.repo.commit("three\n\nWork-Item: HR-3\nWork-Item:");
        install.write("docs/a.md", "4\n");
        let head = install.repo.commit("four, with no trailer");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(analysis.work_items, ["HR-2", "HR-1", "HR-3"]);
    }

    #[test]
    fn a_work_item_of_the_base_history_is_not_in_the_range() {
        let install = Install::new("", two_areas(), &[]);
        install.repo.git(&[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "old\n\nWork-Item: HR-OLD",
        ]);
        let base = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "1\n");
        let head = install.repo.commit("new\n\nWork-Item: HR-NEW");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(analysis.work_items, ["HR-NEW"]);
    }

    #[test]
    fn the_areas_map_a_deleted_path_by_its_old_path_and_a_renamed_path_by_both() {
        let install = Install::new("", two_areas(), &[]);
        install.write("docs/gone.md", "gone\n");
        install.write("docs/old.md", "one\ntwo\nthree\nfour\n");
        install.write("crates/keep.rs", "keep\n");
        install.write("README.md", "readme\n");
        let base = install.repo.commit("files");
        std::fs::remove_file(install.root().join("docs/gone.md")).expect("delete the file");
        install.repo.git(&["mv", "docs/old.md", "crates/new.rs"]);
        install.write("crates/keep.rs", "kept\n");
        install.write("README.md", "edited\n");
        install.write("elsewhere.txt", "new\n");
        let head = install.repo.commit("change");

        let analysis = install.analyze(&base, &head).expect("analyze");

        let areas: Vec<(&str, Vec<&str>)> = analysis
            .areas
            .iter()
            .map(|(name, paths)| (name.as_str(), paths.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            areas,
            [
                ("docs", vec!["docs/gone.md", "docs/old.md"]),
                ("rust", vec!["crates/keep.rs", "crates/new.rs"])
            ]
        );
        assert_eq!(analysis.no_area, ["README.md", "elsewhere.txt"]);
    }

    #[test]
    fn the_rule_sources_of_a_top_level_install_are_its_knowledge_marker_and_ci_files() {
        let install = Install::new("", two_areas(), &[]);
        install.write(
            ".houserules.json",
            &json!({"review": {"ci": {"files": [".gitlab-ci.yml"]}}}).to_string(),
        );
        install.write(".gitlab-ci.yml", "one\n");
        install.write(".github/workflows/knowledge.yml", "one\n");
        install.write(".github/workflows/other.yml", "one\n");
        install.write("docs/a.md", "one\n");
        let base = install.repo.commit("files");
        install.write("knowledge/demo.json", "{\"entries\": []}\n");
        install.write(
            ".houserules.json",
            &json!({"review": {"ci": {"files": [".gitlab-ci.yml"]}}, "x": 1}).to_string(),
        );
        install.write(".gitlab-ci.yml", "two\n");
        install.write(".github/workflows/knowledge.yml", "two\n");
        install.write(".github/workflows/other.yml", "two\n");
        install.write("docs/a.md", "two\n");
        let head = install.repo.commit("change");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(
            analysis.rule_sources_changed,
            [
                ".github/workflows/knowledge.yml",
                ".gitlab-ci.yml",
                ".houserules.json",
                "knowledge/demo.json"
            ]
        );
    }

    #[test]
    fn a_file_written_by_ci_write_is_a_rule_source() {
        let install = Install::new("", two_areas(), &[]);
        let written = format!(
            "{}\nname: x\n",
            crate::install::marker::review::CI_WRITTEN_BY_LINE
        );
        install.write(".github/workflows/houserules-review.yml", &written);
        install.write(".github/workflows/plain.yml", "name: x\n");
        let base = install.repo.commit("files");
        install.write(
            ".github/workflows/houserules-review.yml",
            &format!("{written}more\n"),
        );
        install.write(".github/workflows/plain.yml", "name: y\n");
        let head = install.repo.commit("change");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(
            analysis.rule_sources_changed,
            [".github/workflows/houserules-review.yml"]
        );
    }

    #[test]
    fn a_ci_write_file_that_the_range_deletes_is_a_changed_rule_source() {
        let install = Install::new("", two_areas(), &[]);
        let workflow = ".github/workflows/houserules-review.yml";
        install.write(
            workflow,
            &format!(
                "{}\nname: x\n",
                crate::install::marker::review::CI_WRITTEN_BY_LINE
            ),
        );
        let base = install.repo.commit("ci write");
        std::fs::remove_file(install.root().join(workflow)).expect("delete the workflow");
        let head = install.repo.commit("delete the review workflow");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(analysis.rule_sources_changed, [workflow]);
        assert_ne!(
            analysis.rules.merge_base_hash,
            Some(analysis.rules.head_hash.clone())
        );
    }

    #[test]
    fn a_ci_write_file_that_loses_its_first_line_is_a_changed_rule_source() {
        let install = Install::new("", two_areas(), &[]);
        let workflow = ".github/workflows/houserules-review.yml";
        install.write(
            workflow,
            &format!(
                "{}\nname: x\n",
                crate::install::marker::review::CI_WRITTEN_BY_LINE
            ),
        );
        let base = install.repo.commit("ci write");
        install.write(workflow, "name: gutted\n");
        let head = install.repo.commit("strip the line");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(analysis.rule_sources_changed, [workflow]);
        assert_ne!(
            analysis.rules.merge_base_hash,
            Some(analysis.rules.head_hash.clone())
        );
    }

    #[test]
    fn a_ci_file_that_the_range_unlists_is_a_changed_rule_source() {
        let install = Install::new("", two_areas(), &[]);
        install.write(
            ".houserules.json",
            &json!({"review": {"ci": {"files": ["extra.yml"]}}}).to_string(),
        );
        install.write("extra.yml", "one\n");
        let base = install.repo.commit("list it");
        install.write(".houserules.json", "{}\n");
        install.write("extra.yml", "two\n");
        let head = install.repo.commit("unlist it and edit it");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(
            analysis.rule_sources_changed,
            [".houserules.json", "extra.yml"]
        );
    }

    #[test]
    fn a_replace_ref_of_the_head_commit_changes_nothing_in_the_analysis() {
        let install = Install::new("", two_areas(), &[]);
        install.write(
            ".houserules.json",
            &json!({"review": {"ci": {"files": ["extra.yml"]}}}).to_string(),
        );
        install.write("extra.yml", "one\n");
        let base = install.repo.commit("base");
        install.write("extra.yml", "two\n");
        let head_message = "edit extra.yml\n\nWork-Item: HR-1";
        let head = install.repo.commit(head_message);
        let plain = install.analyze(&base, &head).expect("analyze");
        assert_eq!(plain.rule_sources_changed, ["extra.yml"]);
        assert_eq!(plain.work_items, ["HR-1"]);
        assert_eq!(plain.merge_base.as_deref(), Some(base.as_str()));
        let base_tree = format!("{base}^{{tree}}");
        let head_tree = format!("{head}^{{tree}}");
        // Each replacement swaps in one thing a replace ref can swap: the tree
        // (the CI-file diff, the rules hash, the CI-file scan), the parents
        // (the merge base), and the message (the work items).
        let replacements: [(&str, &str, &[&str], &str); 3] = [
            ("the tree", &base_tree, &["-p", &base], head_message),
            ("the parents", &head_tree, &[], head_message),
            (
                "the message",
                &head_tree,
                &["-p", &base],
                "replaced\n\nWork-Item: HR-9",
            ),
        ];

        for (what, tree, parents, message) in replacements {
            let mut args = vec!["commit-tree", tree];
            args.extend(parents);
            args.extend(["-m", message]);
            let replacement = install.repo.git(&args).trim().to_string();
            install
                .repo
                .git(&["replace", "--force", &head, &replacement]);

            assert_eq!(
                install.analyze(&base, &head),
                Ok(plain.clone()),
                "a replace ref of {what}"
            );
        }
    }

    #[test]
    fn diff_relative_hides_no_ci_file_of_a_subdirectory_install() {
        let install = Install::new("sub/", two_areas(), &[]);
        install.write(
            ".houserules.json",
            &json!({"review": {"ci": {"files": [".gitlab-ci.yml"]}}}).to_string(),
        );
        install.repo.write(".gitlab-ci.yml", "one\n");
        let base = install.repo.commit("files");
        install.repo.write(".gitlab-ci.yml", "two\n");
        let head = install.repo.commit("edit the ci file");
        let plain = install.analyze(&base, &head).expect("analyze");
        install.repo.git(&["config", "diff.relative", "true"]);

        let relative = install.analyze(&base, &head).expect("analyze");

        assert_eq!(plain.rule_sources_changed, [".gitlab-ci.yml"]);
        assert_eq!(relative.rule_sources_changed, plain.rule_sources_changed);
    }

    #[test]
    fn the_rule_sources_of_a_subdirectory_install_list_ci_files_by_their_top_level_path() {
        let install = Install::new("sub/", two_areas(), &[]);
        install.repo.write(".gitlab-ci.yml", "one\n");
        install.write(
            ".houserules.json",
            &json!({"review": {"ci": {"files": [".gitlab-ci.yml"]}}}).to_string(),
        );
        install
            .repo
            .write(".github/workflows/knowledge-sub.yml", "one\n");
        install.repo.write("knowledge/other-install.json", "{}\n");
        let base = install.repo.commit("files");
        install.write("knowledge/demo.json", "{\"entries\": []}\n");
        install.repo.write(".gitlab-ci.yml", "two\n");
        install
            .repo
            .write(".github/workflows/knowledge-sub.yml", "two\n");
        install
            .repo
            .write("knowledge/other-install.json", "{\"x\": 1}\n");
        install.repo.write("README.md", "outside\n");
        let head = install.repo.commit("change");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(
            analysis.rule_sources_changed,
            [
                ".github/workflows/knowledge-sub.yml",
                ".gitlab-ci.yml",
                "knowledge/demo.json"
            ]
        );
        assert_eq!(
            analysis
                .paths
                .iter()
                .map(|path| path.path.as_str())
                .collect::<Vec<_>>(),
            ["knowledge/demo.json"]
        );
    }

    #[test]
    fn a_ci_file_listed_at_its_own_commit_counts_when_its_entry_is_added_in_the_range() {
        let install = Install::new("", two_areas(), &[]);
        install.write("extra.yml", "one\n");
        let base = install.repo.commit("a file that is no CI file yet");
        install.write(
            ".houserules.json",
            &json!({"review": {"ci": {"files": ["extra.yml"]}}}).to_string(),
        );
        install.write("extra.yml", "two\n");
        let head = install.repo.commit("list it and edit it");

        let analysis = install.analyze(&base, &head).expect("analyze");

        assert_eq!(
            analysis.rule_sources_changed,
            [".houserules.json", "extra.yml"]
        );
    }

    /// The audit rows of the install `install` for `base` to `head`, run in
    /// process.
    fn in_process(install: &Install, base: &str, head: &str) -> (Analysis, AuditResult) {
        let analysis = install.analyze(base, head).expect("analyze");
        let rows = audit_rows(
            &install.root(),
            &install.base(),
            AuditSource::InProcess,
            &analysis,
        )
        .expect("audit rows");
        (analysis, rows)
    }

    #[test]
    fn an_audit_file_of_the_range_is_read_back_unchanged() {
        let install = Install::new(
            "",
            two_areas(),
            &[checked(
                "demo.noforbidden",
                json!({"type": "grep-absent", "level": "fail", "files": "docs/**", "pattern": "forbidden", "scope": "changed"}),
            )],
        );
        let base = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "text\n");
        let head = install.repo.commit("docs");
        let (analysis, rows) = in_process(&install, &base, &head);
        let file = install.root().join("audit.json");
        std::fs::write(
            &file,
            serde_json::to_string_pretty(&rows).expect("serialize"),
        )
        .expect("write");

        let read = audit_rows(
            &install.root(),
            &install.base(),
            AuditSource::File(file),
            &analysis,
        )
        .expect("read the audit file");

        assert_eq!(read, rows);
        assert_eq!(
            rows.rules
                .iter()
                .filter(|row| row.id == "demo.noforbidden")
                .count(),
            1
        );
    }

    #[test]
    fn an_audit_file_of_another_range_is_refused_with_both_ranges() {
        let install = Install::new("", two_areas(), &[]);
        let first = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "1\n");
        let second = install.repo.commit("two");
        install.write("docs/a.md", "2\n");
        let third = install.repo.commit("three");
        let (_, rows) = in_process(&install, &first, &second);
        let file = install.root().join("audit.json");
        std::fs::write(&file, serde_json::to_string(&rows).expect("serialize")).expect("write");
        let analysis = install.analyze(&first, &third).expect("analyze");

        let error = audit_rows(
            &install.root(),
            &install.base(),
            AuditSource::File(file.clone()),
            &analysis,
        )
        .unwrap_err();

        assert_eq!(
            error,
            format!(
                "{} is the audit of {}..{}, not of {}..{}",
                file.display(),
                git::short(&rows.base),
                git::short(&rows.head),
                git::short(&first),
                git::short(&third)
            )
        );
    }

    #[test]
    fn an_audit_file_that_cannot_be_read_or_parsed_is_a_named_error() {
        let install = Install::new("", two_areas(), &[]);
        let head = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        let analysis = install.analyze(&head, &head).expect("analyze");
        let missing = install.root().join("missing.json");
        let broken = install.root().join("broken.json");
        std::fs::write(&broken, "{\"schema\": 1}").expect("write");

        let read = |file: &PathBuf| {
            audit_rows(
                &install.root(),
                &install.base(),
                AuditSource::File(file.clone()),
                &analysis,
            )
            .unwrap_err()
        };

        assert!(read(&missing).starts_with(&format!("{}: ", missing.display())));
        assert!(read(&broken).starts_with(&format!("{}: not an audit result: ", broken.display())));
    }

    #[test]
    fn the_in_process_audit_is_the_audit_of_the_range() {
        let install = Install::new("", two_areas(), &[]);
        let base = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "text\n");
        let head = install.repo.commit("docs\n\nWork-Item: HR-1");

        let (analysis, rows) = in_process(&install, &base, &head);

        assert!(analysis.base.starts_with(&rows.base));
        assert!(analysis.head.starts_with(&rows.head));
        assert_eq!(rows.changed_files, ["docs/a.md"]);
        assert_eq!(rows.references.skipped, 1);
    }

    /// An analysis of the given paths.
    fn analysis_of(paths: Vec<ChangedPath>) -> Analysis {
        Analysis {
            install: ".".to_string(),
            base: "b".repeat(40),
            head: "h".repeat(40),
            merge_base: None,
            paths,
            diff: DiffSize {
                files: 0,
                added: 0,
                deleted: 0,
                lines: 0,
                excluded: 0,
            },
            rules: RulesHashes {
                merge_base_hash: None,
                merge_base_rules: RulesPresence::Absent,
                head_hash: "0".repeat(64),
            },
            work_items: Vec::new(),
            areas: IndexMap::new(),
            no_area: Vec::new(),
            rule_sources_changed: Vec::new(),
        }
    }

    /// A row of the audit result for the rule `id`.
    fn row(id: &str, mode: RowMode, result: RowResult) -> AuditRow {
        AuditRow {
            id: id.to_string(),
            kind: "rule".to_string(),
            mode,
            level: (mode == RowMode::Deterministic).then_some(RowLevel::Fail),
            result,
            evidence: "e".to_string(),
            findings: None,
            sanctioned: None,
        }
    }

    /// An audit result with `rows`.
    fn audit_of(rows: Vec<AuditRow>) -> AuditResult {
        AuditResult {
            schema: "houserules/audit-result@1.0.0".to_string(),
            base: "bbbbbbb".to_string(),
            head: "hhhhhhh".to_string(),
            ids: Vec::new(),
            changed_files: Vec::new(),
            areas: Vec::new(),
            area_files: IndexMap::new(),
            rules: rows,
            summary: Summary {
                base: "bbbbbbb".to_string(),
                head: "hhhhhhh".to_string(),
                deterministic: 0,
                pass: 0,
                fail: 0,
                warn: 0,
                skipped: 0,
                judged: 0,
                empty_range: None,
                sanctioned_fail: None,
                reference_fail: 0,
            },
            stale_sanctions: None,
            references: References::default(),
        }
    }

    /// A base whose entries have the checks in `checks`, as `(id, check)`.
    fn base_of(checks: &[(&str, Value)]) -> (Install, Base) {
        let entries: Vec<Value> = checks
            .iter()
            .map(|(id, check)| checked(id, check.clone()))
            .collect();
        let install = Install::new("", two_areas(), &entries);
        let base = install.base();
        (install, base)
    }

    fn covering_grep(files: Value) -> Value {
        json!({"type": "grep-absent", "level": "fail", "files": files, "pattern": "X", "scope": "changed", "covers": true})
    }

    #[test]
    fn a_passing_covering_row_covers_the_paths_it_reads_and_a_failing_one_does_not() {
        let (_install, base) = base_of(&[
            ("demo.docs", covering_grep(json!("docs/**"))),
            ("demo.rust", covering_grep(json!("crates/**"))),
        ]);
        let analysis = analysis_of(vec![
            text_path("crates/a.rs", 3, 0),
            text_path("docs/a.md", 2, 0),
            text_path("tools/x.sh", 5, 0),
        ]);
        let audit = audit_of(vec![
            row("demo.docs", RowMode::Deterministic, RowResult::Pass),
            row("demo.rust", RowMode::Deterministic, RowResult::Fail),
        ]);

        let coverage = coverage(&analysis, &audit, &base);

        assert_eq!(
            coverage
                .uncovered
                .iter()
                .map(|path| path.path.as_str())
                .collect::<Vec<_>>(),
            ["crates/a.rs", "tools/x.sh"]
        );
        assert_eq!(coverage.uncovered_lines, 8);
        assert_eq!(coverage.total_lines, 10);
        assert!((coverage.share - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn a_warned_row_covers_and_a_skipped_a_judged_or_a_sanctioned_failing_row_does_not() {
        let (_install, base) = base_of(&[
            ("demo.warn", covering_grep(json!("a/**"))),
            ("demo.skipped", covering_grep(json!("b/**"))),
            ("demo.judged", covering_grep(json!("c/**"))),
            ("demo.sanctioned", covering_grep(json!("d/**"))),
        ]);
        let analysis = analysis_of(vec![
            text_path("a/x", 1, 0),
            text_path("b/x", 1, 0),
            text_path("c/x", 1, 0),
            text_path("d/x", 1, 0),
        ]);
        let mut sanctioned = row("demo.sanctioned", RowMode::Deterministic, RowResult::Fail);
        sanctioned.sanctioned = Some("interim".to_string());
        let audit = audit_of(vec![
            row("demo.warn", RowMode::Deterministic, RowResult::Warn),
            row("demo.skipped", RowMode::Deterministic, RowResult::Skipped),
            row("demo.judged", RowMode::Judged, RowResult::Open),
            sanctioned,
        ]);

        let coverage = coverage(&analysis, &audit, &base);

        assert_eq!(
            coverage
                .uncovered
                .iter()
                .map(|path| path.path.as_str())
                .collect::<Vec<_>>(),
            ["b/x", "c/x", "d/x"]
        );
    }

    #[test]
    fn a_check_that_does_not_declare_covers_never_covers() {
        let (_install, base) = base_of(&[(
            "demo.plain",
            json!({"type": "grep-absent", "level": "fail", "files": "**", "pattern": "X", "scope": "changed"}),
        )]);
        let analysis = analysis_of(vec![text_path("a.txt", 4, 0)]);
        let audit = audit_of(vec![row(
            "demo.plain",
            RowMode::Deterministic,
            RowResult::Pass,
        )]);

        let coverage = coverage(&analysis, &audit, &base);

        assert_eq!(coverage.uncovered.len(), 1);
        assert_eq!(coverage.uncovered_lines, 4);
    }

    #[test]
    fn a_covering_co_change_check_reads_both_sides_and_a_runner_with_no_files_reads_all() {
        let (_install, base) = base_of(&[
            (
                "demo.together",
                json!({"type": "co-change", "level": "fail", "if": "src/**", "then": "tests/**", "covers": true}),
            ),
            (
                "demo.tool",
                json!({"type": "runner", "level": "fail", "tool": "t", "covers": true}),
            ),
        ]);
        let analysis = analysis_of(vec![
            text_path("src/a.rs", 1, 0),
            text_path("tests/a.rs", 1, 0),
        ]);
        let together_only = audit_of(vec![row(
            "demo.together",
            RowMode::Deterministic,
            RowResult::Pass,
        )]);
        let tool_only = audit_of(vec![row(
            "demo.tool",
            RowMode::Deterministic,
            RowResult::Pass,
        )]);

        assert!(
            coverage(&analysis, &together_only, &base)
                .uncovered
                .is_empty()
        );
        assert!(coverage(&analysis, &tool_only, &base).uncovered.is_empty());
    }

    #[test]
    fn an_excluded_path_leaves_both_sides_of_the_share() {
        let (_install, base) = base_of(&[("demo.docs", covering_grep(json!("docs/**")))]);
        let mut lock = text_path("Cargo.lock", 100, 0);
        lock.excluded = true;
        let analysis = analysis_of(vec![
            lock,
            text_path("docs/a.md", 3, 0),
            text_path("x.rs", 1, 0),
        ]);
        let audit = audit_of(vec![row(
            "demo.docs",
            RowMode::Deterministic,
            RowResult::Pass,
        )]);

        let coverage = coverage(&analysis, &audit, &base);

        assert_eq!(coverage.total_lines, 4);
        assert_eq!(coverage.uncovered_lines, 1);
        assert_eq!(coverage.uncovered.len(), 1);
        assert!((coverage.share - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn a_change_with_no_changed_line_has_a_share_of_zero() {
        let (_install, base) = base_of(&[]);

        let none = coverage(&analysis_of(Vec::new()), &audit_of(Vec::new()), &base);

        assert_eq!(none.share.to_bits(), 0.0_f64.to_bits());
        assert_eq!(none.total_lines, 0);
        assert!(none.uncovered.is_empty());
    }

    #[test]
    fn a_change_that_no_covering_check_reads_lists_every_path_as_uncovered() {
        let (_install, base) = base_of(&[]);
        let analysis = analysis_of(vec![text_path("a.rs", 2, 1), text_path("b.rs", 1, 0)]);

        let coverage = coverage(&analysis, &audit_of(Vec::new()), &base);

        assert_eq!(coverage.uncovered, analysis.paths);
        assert!((coverage.share - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_covering_check_that_ran_in_the_audit_covers_the_real_change() {
        let install = Install::new(
            "",
            two_areas(),
            &[checked("demo.noforbidden", covering_grep(json!("docs/**")))],
        );
        let base_sha = install.repo.git(&["rev-parse", "HEAD"]).trim().to_string();
        install.write("docs/a.md", "text\n");
        install.write("other/b.txt", "text\nmore\n");
        let head = install.repo.commit("change");
        let (analysis, rows) = in_process(&install, &base_sha, &head);

        let coverage = coverage(&analysis, &rows, &install.base());

        assert_eq!(
            coverage
                .uncovered
                .iter()
                .map(|path| path.path.as_str())
                .collect::<Vec<_>>(),
            ["other/b.txt"]
        );
        assert_eq!(coverage.total_lines, 3);
        assert_eq!(coverage.uncovered_lines, 2);
    }
}
