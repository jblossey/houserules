//! The intent of a change and the adapters that fill it from a source, such
//! as a work item, a file, or an OpenSpec change (spec 4.9).
//!
//! An intent is what a change was meant to do. [`resolve`] runs the adapters
//! that `review.intent.sources` lists, in that order, and adds the intents that
//! a caller names with [`IntentRef`], whatever the config says: the flag
//! `--intent`, the `Intent:` line of a bundle, and the `Intent:` trailers of the
//! commits of the range (`review::links`). It takes the
//! `review.intent` settings in the checked form of [`IntentConfig`]: the
//! adapters and the compiled globs, which only the marker reader builds, so
//! `resolve` checks nothing and drops no setting. [`resolve_named`] resolves
//! the named intent alone, from the install, a commit, and the name, with no
//! change analysis; it reads the tree at the commit it is given, so a caller
//! can resolve a name at the head and at the merge base. [`found_in`] gives the
//! intents that the changed paths of a range name, for the adapters that the
//! caller picks. No adapter runs a tool: each reads the tracker, as `audit`
//! reads it, or reads files of the tree at the head of the range.
//!
//! - `work-item`: one intent for each work item of the range. Its `title` is
//!   the tracker's title; every other field is `null`. An install with no
//!   tracker gives none, and a tracker that cannot be reached gives a `null`
//!   title. A named work item in such an install resolves to its intent with a
//!   `null` title, and gives no line.
//! - `file`: one intent for each changed path that matches a glob of
//!   `review.intent.files` and for the named path. Its `title` is the first
//!   `# ` heading, its `why` the first paragraph after it (cut to 1,000
//!   characters, the lines of the paragraph joined by line feeds), and its
//!   `tasks` the counts of the task-list lines.
//! - `openspec`: one intent for each OpenSpec change that a changed path names
//!   and for the named id (the module [`openspec`] holds the rules).
//!
//! The intents come in adapter order, then by `ref`. An adapter that the config
//! leaves out comes after the configured ones, in the order of `ADAPTERS`, and
//! an intent whose source is no adapter comes last, by source in byte order and
//! then by `ref`.
//!
//! A named intent that does not resolve is an entry of the list, in its place
//! by the order above: its `source` and `ref`, every other field `null`, and
//! `unresolved`, the adapter's line (spec 4.9). A caller prints the line after
//! the intent it names. A value that is no `<source>:<ref>` (a trailer is the
//! one place that can hold one) is named by [`IntentRef::split`], whose
//! `source` or `ref` is empty, and it is the entry with the line of a text that
//! is no `<source>:<ref>`. A failure that is not the named intent's own, whichever
//! adapter it comes from, is the `Err`: the line of the marker, of the
//! tracker's command, or of git. It names what failed, and a caller reports it
//! as its own failure.

pub(crate) mod openspec;

use std::path::Path;

use globset::GlobMatcher;

use super::change::Analysis;
use super::git;
use crate::install::marker::review::{Adapter, IntentConfig, no_intent_source};
use crate::published::change::ChangeKind;
use crate::published::intent::{Intent, Tasks};
use crate::tracker::{self, Lookup, Tracker, TrackerError};

/// The most characters of a `why`.
const WHY_LIMIT: usize = 1_000;

/// The line for an intent text that is no `<source>:<ref>`.
const NOT_SOURCE_AND_REF: &str = "an intent is written <source>:<ref>";

/// The line for a `file` path that is not relative to the install, or that
/// holds an empty, `.`, or `..` folder.
const PATH_NOT_RELATIVE: &str =
    "the path must be relative to the install, with no empty, . or .. folder";

/// The line for a `file` path with no regular file at the head.
const NO_REGULAR_FILE: &str = "no regular file at that path at the head";

/// The line for a named work item that the tracker does not hold.
pub(crate) const NO_SUCH_WORK_ITEM: &str = "the tracker holds no such work item";

/// An intent that a caller names, as the bundle's `Intent:` line or the
/// `--intent` flag writes it: `<source>:<ref>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntentRef {
    /// The name of the adapter that resolves the intent.
    pub(crate) source: String,
    /// The id of the intent for that adapter: a work-item id or a path.
    pub(crate) reference: String,
}

impl IntentRef {
    /// Reads `<source>:<ref>`: the text up to the first colon is the source,
    /// the rest is the ref, and white space around each is dropped. The text
    /// is the value after the label of the bundle's line, not the whole line.
    /// Whether the source is an adapter is [`resolve`]'s question, so a caller
    /// can print an unknown source with the intent it names.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let named = Self::split(text);
        if named.source.is_empty() || named.reference.is_empty() {
            return Err(NOT_SOURCE_AND_REF.to_string());
        }
        Ok(named)
    }

    /// Reads any text as an intent: the text up to the first colon is the
    /// source, the rest is the ref, and white space around each is dropped. A
    /// text with no colon has an empty source and the trimmed text as its ref. A
    /// source or a ref that is empty marks a text that [`IntentRef::parse`]
    /// refuses, and [`resolve`] gives such an intent the line of a text that is
    /// no `<source>:<ref>`.
    pub(crate) fn split(text: &str) -> Self {
        let (source, reference) = text.split_once(':').unwrap_or(("", text));
        Self {
            source: source.trim().to_string(),
            reference: reference.trim().to_string(),
        }
    }
}

/// The intents of the range `analysis` for the install at `root`: those of
/// each adapter that `config` lists, and each of `named`, whatever the config
/// says (spec 4.9). The `config` is the one that `Marker::review` read.
///
/// They come in adapter order, then by `ref`, without duplicates: a `source`
/// and `ref` that `named` repeats, or that an adapter also finds, is one
/// intent. A named intent that does not resolve is an entry of the list, in its
/// place by that order, with the adapter's line in `unresolved` (see
/// [`unresolved`]); it stands for what an adapter found under the same `source`
/// and `ref`. A tracker that cannot be reached is no error: the work items get a
/// `null` title. Every failure that is not a named intent's own is the `Err`
/// with its line.
pub(crate) fn resolve(
    root: &Path,
    analysis: &Analysis,
    config: &IntentConfig,
    named: &[IntentRef],
) -> Result<Vec<Intent>, String> {
    let lookup = tracker::lookup(root).map_err(|error| error.to_string());
    resolve_with(root, analysis, config, named, lookup)
}

/// [`resolve`] with the result of the tracker lookup given. The lookup makes
/// no backend call, and only the `work-item` adapter reads its `Err`.
fn resolve_with(
    root: &Path,
    analysis: &Analysis,
    config: &IntentConfig,
    named: &[IntentRef],
    lookup: Result<Lookup, String>,
) -> Result<Vec<Intent>, String> {
    let adapters = config.adapters();
    let mut intents = found(root, analysis, config, adapters, &lookup)?;
    for (index, name) in named.iter().enumerate() {
        if named[..index].contains(name) {
            continue;
        }
        let intent = named_intent(root, &analysis.head, name, &lookup)?;
        intents
            .retain(|found| found.source != intent.source || found.reference != intent.reference);
        intents.push(intent);
    }
    ordered(&mut intents, adapters);
    Ok(intents)
}

/// The intents that the adapters `sources` find from the changed paths and the
/// work items of `analysis`, with no named intent: those of the `work-item`
/// adapter for the work items of the range, of the `file` adapter for the paths
/// that `config` matches, and of the `openspec` adapter for the changes that a
/// path names. They come in the order of `sources`, then by `ref`, without
/// duplicates. A tracker that cannot be reached is no error: the work items get
/// a `null` title. Every failure that is not an intent's own is the `Err` with
/// its line.
pub(crate) fn found_in(
    root: &Path,
    analysis: &Analysis,
    config: &IntentConfig,
    sources: &[Adapter],
) -> Result<Vec<Intent>, String> {
    let lookup = tracker::lookup(root).map_err(|error| error.to_string());
    let mut intents = found(root, analysis, config, sources, &lookup)?;
    ordered(&mut intents, sources);
    Ok(intents)
}

/// The intents that each of `adapters` finds from `analysis`, in the order of
/// `adapters`. The `lookup` makes no backend call, and only the `work-item`
/// adapter reads its `Err`.
fn found(
    root: &Path,
    analysis: &Analysis,
    config: &IntentConfig,
    adapters: &[Adapter],
    lookup: &Result<Lookup, String>,
) -> Result<Vec<Intent>, String> {
    let files = config.file_matchers();
    let mut intents = Vec::new();
    for adapter in adapters {
        match adapter {
            Adapter::WorkItem => intents.extend(work_items(lookup, &analysis.work_items)?),
            Adapter::File => intents.extend(changed_files(root, analysis, &files)?),
            Adapter::OpenSpec => {
                intents.extend(openspec::found(
                    root,
                    &analysis.head,
                    &changed_paths(analysis),
                )?);
            }
        }
    }
    Ok(intents)
}

/// Every path that the range of `analysis` changes, from the install root: the
/// path of each change and the old path of a rename.
fn changed_paths(analysis: &Analysis) -> Vec<String> {
    analysis
        .paths
        .iter()
        .flat_map(|path| std::iter::once(&path.path).chain(path.old_path.as_ref()))
        .cloned()
        .collect()
}

/// Sorts `intents` by adapter, then by `ref`: first the adapters of `listed`,
/// in that order, then every other adapter, in the order of [`Adapter::ALL`].
/// An intent of another source comes last, by source in byte order and then by
/// `ref`. It drops the intents that repeat a source and a `ref`.
fn ordered(intents: &mut Vec<Intent>, listed: &[Adapter]) {
    let order: Vec<Adapter> = listed
        .iter()
        .copied()
        .chain(
            Adapter::ALL
                .into_iter()
                .filter(|adapter| !listed.contains(adapter)),
        )
        .collect();
    let key = |intent: &Intent| {
        let rank = order
            .iter()
            .position(|adapter| adapter.name() == intent.source)
            .unwrap_or(order.len());
        let source = if rank == order.len() {
            intent.source.clone()
        } else {
            String::new()
        };
        (rank, source, intent.reference.clone())
    };
    intents.sort_by_cached_key(key);
    intents.dedup_by(|later, earlier| {
        later.source == earlier.source && later.reference == earlier.reference
    });
}

/// The entry of the named intent `named` for the install at `root`, whatever
/// the config lists: the adapter of its source resolves it against the tree at
/// `commit`, a full commit id, and the tracker (spec 4.9). It reads no change
/// analysis, so a caller that names one intent and runs no configured adapter
/// needs only the commit. A failure that is not the named intent's own is the
/// `Err` with its line.
pub(crate) fn resolve_named(
    root: &Path,
    commit: &str,
    named: &IntentRef,
) -> Result<Intent, String> {
    let lookup = tracker::lookup(root).map_err(|error| error.to_string());
    named_intent(root, commit, named, &lookup)
}

/// The entry of the named intent `named` at `commit`: the arm of its adapter, the
/// [`unresolved`] entry for a source that is no adapter, or the one for a value
/// that is no `<source>:<ref>`. The `lookup` makes
/// no backend call, and only the `work-item` arm reads its `Err`.
fn named_intent(
    root: &Path,
    commit: &str,
    named: &IntentRef,
    lookup: &Result<Lookup, String>,
) -> Result<Intent, String> {
    if named.source.is_empty() || named.reference.is_empty() {
        return Ok(unresolved(named, NOT_SOURCE_AND_REF));
    }
    match Adapter::named(&named.source) {
        None => Ok(unresolved(named, &no_intent_source(&named.source))),
        Some(Adapter::WorkItem) => named_work_item(lookup, named),
        Some(Adapter::File) => named_file(root, commit, named),
        Some(Adapter::OpenSpec) => openspec::named(root, commit, named),
    }
}

/// The entry of the named intent `named`, which its adapter does not resolve:
/// `line` is the adapter's line (spec 4.9). Every other field of the contract
/// is `null`, and `artifacts` is empty.
fn unresolved(named: &IntentRef, line: &str) -> Intent {
    Intent {
        source: named.source.clone(),
        reference: named.reference.clone(),
        title: None,
        why: None,
        tasks: None,
        requirements: None,
        validation: None,
        artifacts: Vec::new(),
        unresolved: Some(line.to_string()),
    }
}

/// The tracker lookup, or the line that stopped it.
fn tracker_of(lookup: &Result<Lookup, String>) -> Result<&Lookup, String> {
    lookup.as_ref().map_err(String::clone)
}

/// What the tracker says about the title of a work item.
pub(crate) enum Title {
    /// The tracker holds the item; its record may hold no title.
    Found(Option<String>),
    /// The tracker holds no such item.
    Missing,
    /// The tracker cannot be reached: the line that it gives for that, which
    /// `check-backlog` prints.
    Unreachable(String),
}

/// Asks `tracker` for the record of `id`, with one call. [`resolve`] leaves the
/// line of an unreachable tracker out of the intent (spec 4.9); `bundle check`
/// prints it. A failure that is neither a missing item nor an unreachable
/// tracker is the `Err` with its line.
pub(crate) fn title_of(tracker: &dyn Tracker, id: &str) -> Result<Title, String> {
    match tracker.get(&[id.to_string()]) {
        Ok(records) => Ok(Title::Found(
            records
                .first()
                .and_then(|record| record["title"].as_str())
                .map(str::to_string),
        )),
        Err(TrackerError::NotFound(_)) => Ok(Title::Missing),
        Err(error @ TrackerError::Unreachable { .. }) => Ok(Title::Unreachable(error.to_string())),
        Err(error @ TrackerError::Command(_)) => Err(error.to_string()),
    }
}

/// The intent of work item `id` with `title`.
fn work_item(id: &str, title: Option<String>) -> Intent {
    Intent {
        source: Adapter::WorkItem.name().to_string(),
        reference: id.to_string(),
        title,
        why: None,
        tasks: None,
        requirements: None,
        validation: None,
        artifacts: Vec::new(),
        unresolved: None,
    }
}

/// The `work-item` intents of `ids`. The tracker is asked for each id until it
/// cannot be reached; the work items after that get no title.
fn work_items(lookup: &Result<Lookup, String>, ids: &[String]) -> Result<Vec<Intent>, String> {
    let Lookup::Installed(tracker) = tracker_of(lookup)? else {
        return Ok(Vec::new());
    };
    let mut reachable = true;
    ids.iter()
        .map(|id| {
            let title = if reachable {
                match title_of(tracker.as_ref(), id)? {
                    Title::Found(title) => title,
                    Title::Missing => None,
                    Title::Unreachable(_) => {
                        reachable = false;
                        None
                    }
                }
            } else {
                None
            };
            Ok(work_item(id, title))
        })
        .collect()
}

/// The `work-item` intent that a caller names. A work item that the tracker
/// does not hold is the [`unresolved`] entry with [`NO_SUCH_WORK_ITEM`]. An
/// install with no tracker, and a tracker that cannot be reached, give the
/// work item a `null` title.
fn named_work_item(lookup: &Result<Lookup, String>, named: &IntentRef) -> Result<Intent, String> {
    let id = &named.reference;
    Ok(match tracker_of(lookup)? {
        Lookup::Absent(_) => work_item(id, None),
        Lookup::Installed(tracker) => match title_of(tracker.as_ref(), id)? {
            Title::Found(title) => work_item(id, title),
            Title::Unreachable(_) => work_item(id, None),
            Title::Missing => unresolved(named, NO_SUCH_WORK_ITEM),
        },
    })
}

/// The `file` intents of the changed paths that match a matcher of `matchers`.
/// A deleted or binary path, and a path with no regular file at the head (a
/// symlink or a submodule), gives none.
fn changed_files(
    root: &Path,
    analysis: &Analysis,
    matchers: &[GlobMatcher],
) -> Result<Vec<Intent>, String> {
    let mut intents = Vec::new();
    for path in &analysis.paths {
        let can_be_text = !matches!(path.kind, ChangeKind::Deleted | ChangeKind::Binary);
        if can_be_text
            && matchers.iter().any(|matcher| matcher.is_match(&path.path))
            && let Some(text) = text_at(root, &analysis.head, &path.path)?
        {
            intents.push(file_intent(&path.path, &text));
        }
    }
    Ok(intents)
}

/// The `file` intent that a caller names: the file `named.reference` below the
/// install root, at `commit`. A path that is not relative to the install is the
/// [`unresolved`] entry with [`PATH_NOT_RELATIVE`], and a path with no regular
/// file at `commit` is the entry with [`NO_REGULAR_FILE`].
fn named_file(root: &Path, commit: &str, named: &IntentRef) -> Result<Intent, String> {
    let path = &named.reference;
    if !is_relative_to_the_install(path) {
        return Ok(unresolved(named, PATH_NOT_RELATIVE));
    }
    Ok(match text_at(root, commit, path)? {
        Some(text) => file_intent(path, &text),
        None => unresolved(named, NO_REGULAR_FILE),
    })
}

/// `true` when `path` is relative and holds no empty, `.`, or `..` folder.
fn is_relative_to_the_install(path: &str) -> bool {
    !path.starts_with('/') && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

/// The text of the regular file `path` (from the install root) in the tree of
/// `commit`, or `None` when the tree holds no regular file there. The path
/// matches literally, so a folder, a symlink, a submodule, and a name that is
/// only the start of another name are no file.
fn text_at(root: &Path, commit: &str, path: &str) -> Result<Option<String>, String> {
    let listing = git::run_bytes(
        root,
        &["ls-tree", "-z", commit, "--", &format!(":(literal){path}")],
    )?;
    let listing = String::from_utf8_lossy(&listing);
    let Some((meta, name)) = listing
        .split('\0')
        .find(|entry| !entry.is_empty())
        .and_then(|entry| entry.split_once('\t'))
    else {
        return Ok(None);
    };
    let mut fields = meta.split(' ');
    match (fields.next(), fields.next(), fields.next()) {
        (Some(mode), Some("blob"), Some(sha)) if mode.starts_with("100") && name == path => {
            let content = git::run_bytes(root, &["cat-file", "blob", sha])?;
            Ok(Some(String::from_utf8_lossy(&content).into_owned()))
        }
        _ => Ok(None),
    }
}

/// The `file` intent of the file `path` with `text`.
fn file_intent(path: &str, text: &str) -> Intent {
    let lines: Vec<&str> = text.lines().collect();
    let heading = lines.iter().position(|line| heading_text(line).is_some());
    Intent {
        source: Adapter::File.name().to_string(),
        reference: path.to_string(),
        title: heading
            .and_then(|at| heading_text(lines[at]))
            .map(str::to_string),
        why: heading.and_then(|at| first_paragraph(&lines[at + 1..])),
        tasks: Some(count_tasks(text)),
        requirements: None,
        validation: None,
        artifacts: vec![path.to_string()],
        unresolved: None,
    }
}

/// The text of a level-one heading: a line that starts with `# ` and holds
/// more than white space after it.
fn heading_text(line: &str) -> Option<&str> {
    let text = line.strip_prefix("# ")?.trim();
    (!text.is_empty()).then_some(text)
}

/// The first run of lines with text in `lines`, joined by line feeds and cut to
/// [`WHY_LIMIT`] characters; `None` when there is none.
fn first_paragraph(lines: &[&str]) -> Option<String> {
    let is_blank = |line: &&str| line.trim().is_empty();
    let paragraph: Vec<&str> = lines
        .iter()
        .skip_while(|line| is_blank(line))
        .take_while(|line| !is_blank(line))
        .copied()
        .collect();
    (!paragraph.is_empty()).then(|| paragraph.join("\n").chars().take(WHY_LIMIT).collect())
}

/// The counts of the task-list lines of `text`: a line that holds `[x]`, `[X]`,
/// or `[ ]` after any leading white space, one of the markers `-`, `*`, or
/// `+`, and white space.
fn count_tasks(text: &str) -> Tasks {
    let mut counts = Tasks { done: 0, total: 0 };
    for done in text.lines().filter_map(task_state) {
        counts.total += 1;
        counts.done += u64::from(done);
    }
    counts
}

/// `Some(true)` for a checked task-list line, `Some(false)` for an open one,
/// and `None` for any other line.
fn task_state(line: &str) -> Option<bool> {
    let after_marker = line.trim_start().strip_prefix(['-', '*', '+'])?;
    let checkbox = after_marker.trim_start();
    if checkbox.len() == after_marker.len() {
        return None;
    }
    let mut box_chars = checkbox.strip_prefix('[')?.chars();
    let mark = box_chars.next()?;
    if box_chars.next()? != ']' {
        return None;
    }
    match mark {
        'x' | 'X' => Some(true),
        ' ' => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use serde_json::{Value, json};
    use walkdir::WalkDir;

    use super::super::change::{Analysis, analyze};
    use super::super::git::scratch::Repo;
    use super::*;
    use crate::install::marker::Marker;
    use crate::install::marker::review::{ADAPTERS, IntentConfig, ReviewConfig};
    use crate::published::intent::{Intent, Requirements, Tasks, Validation};
    use crate::rules::load_base;
    use crate::tracker::{
        DeferRequest, ListOpts, Lookup, NoTracker, Resolution, Tracker, TrackerError, lines,
    };

    /// An install in a scratch repository: `folder` (`""` or `sub/`) holds a
    /// loadable knowledge base and a marker, and one commit holds them.
    struct Install {
        repo: Repo,
        folder: &'static str,
    }

    impl Install {
        fn new(folder: &'static str) -> Self {
            let repo = Repo::new();
            repo.write(&format!("{folder}knowledge/schema.json"), "{}");
            repo.write(&format!("{folder}knowledge/areas.json"), "{}");
            repo.write(&format!("{folder}knowledge/demo.json"), "{\"entries\": []}");
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

        /// Writes `content` to `path` below the install root.
        fn write(&self, path: &str, content: &str) {
            self.repo.write(&format!("{}{path}", self.folder), content);
        }

        /// Writes `content` to `path` below the install root.
        fn write_bytes(&self, path: &str, content: &[u8]) {
            self.repo
                .write_bytes(&format!("{}{path}", self.folder), content);
        }

        /// Commits every change and returns the full sha.
        fn commit(&self, message: &str) -> String {
            self.repo.commit(message)
        }

        /// The full sha of the current commit.
        fn head(&self) -> String {
            self.repo.git(&["rev-parse", "HEAD"]).trim().to_string()
        }

        /// The analysis of the range `base` to `head`.
        fn analysis(&self, base: &str, head: &str) -> Analysis {
            let root = self.root();
            analyze(
                &root,
                &load_base(&root).expect("load the base"),
                &ReviewConfig::default(),
                base,
                head,
            )
            .expect("analyze")
        }

        /// [`resolve_with`] on this install, with at most one named intent.
        fn resolve(
            &self,
            analysis: &Analysis,
            config: &IntentConfig,
            named: Option<&IntentRef>,
            lookup: Result<Lookup, String>,
        ) -> Result<Vec<Intent>, String> {
            resolve_with(
                &self.root(),
                analysis,
                config,
                named.map_or(&[], std::slice::from_ref),
                lookup,
            )
        }
    }

    /// The `IntentConfig` that `Marker::review` reads from a marker with
    /// these sources and globs, the one way to build a non-default config.
    fn config(sources: &[&str], files: &[&str]) -> IntentConfig {
        let dir = tempfile::tempdir().expect("a scratch folder");
        fs::write(
            dir.path().join(".houserules.json"),
            json!({"review": {"intent": {"sources": sources, "files": files}}}).to_string(),
        )
        .expect("write the marker");
        Marker::read(dir.path())
            .expect("a marker")
            .review()
            .expect("a checked review key")
            .intent
    }

    /// The intent that names `text`.
    fn named(text: &str) -> IntentRef {
        IntentRef::parse(text).expect("an intent text")
    }

    /// An install with no tracker.
    fn no_tracker() -> Result<Lookup, String> {
        Ok(Lookup::Absent(NoTracker::Bare))
    }

    /// What the stub tracker answers for an id.
    enum Answer {
        Record(Value),
        Missing,
        Down,
        Fails,
    }

    /// A tracker that answers `get` from a table and records each id it was
    /// asked for. An id that the table lacks is missing.
    struct Titles {
        answers: Vec<(&'static str, Answer)>,
        asked: Rc<RefCell<Vec<String>>>,
    }

    impl Titles {
        /// The lookup of an install with this tracker, and the list of the
        /// ids it is asked for.
        fn lookup(
            answers: Vec<(&'static str, Answer)>,
        ) -> (Result<Lookup, String>, Rc<RefCell<Vec<String>>>) {
            let asked = Rc::new(RefCell::new(Vec::new()));
            let tracker = Self {
                answers,
                asked: Rc::clone(&asked),
            };
            (Ok(Lookup::Installed(Box::new(tracker))), asked)
        }
    }

    impl Tracker for Titles {
        fn name(&self) -> &'static str {
            "stub"
        }

        fn matches_id(&self, _id: &str) -> bool {
            true
        }

        fn resolve(&self, _id: &str) -> Result<Resolution, TrackerError> {
            unreachable!("an adapter only gets")
        }

        fn list(&self, _opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
            unreachable!("an adapter only gets")
        }

        fn batch(&self, _number: &str) -> Result<Value, TrackerError> {
            unreachable!("an adapter only gets")
        }

        fn get(&self, ids: &[String]) -> Result<Vec<Value>, TrackerError> {
            self.asked.borrow_mut().push(ids[0].clone());
            let answer = self
                .answers
                .iter()
                .find(|(id, _)| *id == ids[0])
                .map(|(_, answer)| answer);
            match answer {
                Some(Answer::Record(record)) => Ok(vec![record.clone()]),
                Some(Answer::Down) => Err(TrackerError::Unreachable {
                    adapter: "stub",
                    cause: "the stub is down".to_string(),
                    setup: "bring it up".to_string(),
                }),
                Some(Answer::Fails) => {
                    Err(TrackerError::Command("the backlog is broken".to_string()))
                }
                Some(Answer::Missing) | None => {
                    Err(TrackerError::NotFound(lines::unknown_id(&ids[0])))
                }
            }
        }

        fn set(&mut self, _id: &str, _assignments: &[String]) -> Result<Value, TrackerError> {
            unreachable!("an adapter only gets")
        }

        fn defer(&mut self, _request: &DeferRequest) -> Result<String, TrackerError> {
            unreachable!("an adapter only gets")
        }

        fn archive_backlog_half(&mut self, _lines: &mut Vec<String>) -> Result<(), TrackerError> {
            unreachable!("an adapter only gets")
        }

        fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
            unreachable!("an adapter only gets")
        }

        fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
            unreachable!("an adapter only gets")
        }
    }

    /// A record of the tracker that holds `title`.
    fn titled(id: &str, title: &str) -> Answer {
        Answer::Record(json!({"id": id, "status": "open", "title": title}))
    }

    /// The intent of a work item with this title.
    fn work_item(id: &str, title: Option<&str>) -> Intent {
        Intent {
            source: "work-item".to_string(),
            reference: id.to_string(),
            title: title.map(str::to_string),
            why: None,
            tasks: None,
            requirements: None,
            validation: None,
            artifacts: Vec::new(),
            unresolved: None,
        }
    }

    /// The entry of a named intent that does not resolve: every field of the
    /// contract is `null` and `unresolved` holds the line of the adapter.
    fn unresolved(source: &str, reference: &str, line: &str) -> Intent {
        Intent {
            source: source.to_string(),
            reference: reference.to_string(),
            title: None,
            why: None,
            tasks: None,
            requirements: None,
            validation: None,
            artifacts: Vec::new(),
            unresolved: Some(line.to_string()),
        }
    }

    /// The intent that the `file` adapter gives for a plan with these parts.
    fn file(path: &str, title: Option<&str>, why: Option<&str>, tasks: (u64, u64)) -> Intent {
        Intent {
            source: "file".to_string(),
            reference: path.to_string(),
            title: title.map(str::to_string),
            why: why.map(str::to_string),
            tasks: Some(Tasks {
                done: tasks.0,
                total: tasks.1,
            }),
            requirements: None,
            validation: None,
            artifacts: vec![path.to_string()],
            unresolved: None,
        }
    }

    /// The analysis of an install whose range changes nothing, with these
    /// work items.
    fn analysis_with_work_items(install: &Install, ids: &[&str]) -> Analysis {
        let head = install.head();
        let mut analysis = install.analysis(&head, &head);
        analysis.work_items = ids.iter().map(|id| (*id).to_string()).collect();
        analysis
    }

    #[test]
    fn an_intent_ref_splits_at_the_first_colon() {
        assert_eq!(
            IntentRef::parse("file:docs/plans/x.md"),
            Ok(IntentRef {
                source: "file".to_string(),
                reference: "docs/plans/x.md".to_string()
            })
        );
        assert_eq!(
            IntentRef::parse("  work-item : HR-185 "),
            Ok(IntentRef {
                source: "work-item".to_string(),
                reference: "HR-185".to_string()
            })
        );
        assert_eq!(
            IntentRef::parse("file:docs/a:b.md"),
            Ok(IntentRef {
                source: "file".to_string(),
                reference: "docs/a:b.md".to_string()
            })
        );
    }

    #[test]
    fn an_intent_text_without_a_source_or_a_ref_is_refused() {
        for text in ["", "file", "file:", ":HR-1", " : ", "file:  "] {
            assert_eq!(
                IntentRef::parse(text),
                Err("an intent is written <source>:<ref>".to_string()),
                "{text:?}"
            );
        }
    }

    #[test]
    fn each_source_name_has_an_adapter() {
        for name in ADAPTERS {
            assert!(Adapter::named(name).is_some(), "{name} has no adapter");
        }
        for adapter in Adapter::ALL {
            assert!(
                ADAPTERS.contains(&adapter.name()),
                "{} is no source name",
                adapter.name()
            );
            assert_eq!(Adapter::named(adapter.name()), Some(adapter));
        }
    }

    #[test]
    fn a_plan_gives_its_heading_its_first_paragraph_and_its_task_counts() {
        let text = concat!(
            "Preface.\n\n",
            "# Plan: rate limit\n\n",
            "Spec: `docs/specs/x.md`.\nSecond line of the paragraph.\n\n",
            "A later paragraph.\n\n",
            "- [x] one\n* [X] two\n+ [x] three\n  - [ ] four\n- [ ] five\n",
        );

        assert_eq!(
            file_intent("docs/plans/x.md", text),
            file(
                "docs/plans/x.md",
                Some("Plan: rate limit"),
                Some("Spec: `docs/specs/x.md`.\nSecond line of the paragraph."),
                (3, 5)
            )
        );
    }

    #[test]
    fn only_the_checkboxes_of_a_task_list_count() {
        let text = concat!(
            "# T\n\nWhy.\n\n",
            "1. [x] numbered\n",
            "[x] bare\n",
            "- [y] other mark\n",
            "- [] empty box\n",
            "- [ x] spaced mark\n",
            "text - [x] after text\n",
            "- plain item\n",
            "- [x] counted\n",
        );

        assert_eq!(count_tasks(text), Tasks { done: 1, total: 1 });
    }

    #[test]
    fn a_blank_line_after_the_heading_does_not_end_the_search_for_the_paragraph() {
        assert_eq!(
            file_intent("p.md", "# T\n\n\n\nWhy.\n").why.as_deref(),
            Some("Why.")
        );
    }

    #[test]
    fn the_why_is_cut_to_1000_characters() {
        let long = "ä".repeat(1500);
        let intent = file_intent("p.md", &format!("# T\n\n{long}\n"));
        assert_eq!(intent.why, Some("ä".repeat(1000)));

        let exact = "b".repeat(1000);
        let intent = file_intent("p.md", &format!("# T\n\n{exact}\n"));
        assert_eq!(intent.why, Some(exact));
    }

    #[test]
    fn a_heading_with_no_paragraph_has_no_why() {
        assert_eq!(file_intent("p.md", "# T").why, None);
        assert_eq!(file_intent("p.md", "# T\n\n\n").why, None);
        assert_eq!(file_intent("p.md", "# T\n\n\n").title.as_deref(), Some("T"));
    }

    #[test]
    fn a_file_with_no_heading_has_no_title_and_no_why_but_counts_its_tasks() {
        assert_eq!(
            file_intent("p.md", "Text.\n\n- [x] a\n- [ ] b\n"),
            file("p.md", None, None, (1, 2))
        );
    }

    #[test]
    fn only_a_level_one_heading_that_starts_a_line_is_the_title() {
        let text = "## Not\n#Not\n  # Not\n#\n# \n#  \n# Real  \n# Second\n";

        assert_eq!(file_intent("p.md", text).title.as_deref(), Some("Real"));
    }

    #[test]
    fn crlf_line_ends_read_like_lf() {
        let text = "# Title\r\n\r\nWhy.\r\nMore.\r\n\r\n- [x] a\r\n- [ ] b\r\n";

        assert_eq!(
            file_intent("p.md", text),
            file("p.md", Some("Title"), Some("Why.\nMore."), (1, 2))
        );
    }

    #[test]
    fn a_work_item_intent_gives_the_title_of_the_tracker_and_nothing_else() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-2", "HR-1"]);
        let (lookup, _) = Titles::lookup(vec![
            ("HR-1", titled("HR-1", "First item.")),
            (
                "HR-2",
                Answer::Record(json!({"id": "HR-2", "status": "open"})),
            ),
        ]);

        let intents = install
            .resolve(&analysis, &config(&["work-item"], &[]), None, lookup)
            .expect("resolve");

        assert_eq!(
            intents,
            [
                work_item("HR-1", Some("First item.")),
                work_item("HR-2", None)
            ]
        );
    }

    #[test]
    fn a_work_item_the_tracker_does_not_hold_has_a_null_title() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1", "HR-9"]);
        let (lookup, _) = Titles::lookup(vec![
            ("HR-1", titled("HR-1", "First item.")),
            ("HR-9", Answer::Missing),
        ]);

        let intents = install
            .resolve(&analysis, &config(&["work-item"], &[]), None, lookup)
            .expect("resolve");

        assert_eq!(
            intents,
            [
                work_item("HR-1", Some("First item.")),
                work_item("HR-9", None)
            ]
        );
    }

    #[test]
    fn with_no_tracker_the_work_item_adapter_gives_no_intent() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1"]);

        let intents = install
            .resolve(&analysis, &config(&["work-item"], &[]), None, no_tracker())
            .expect("resolve");

        assert_eq!(intents, []);
    }

    #[test]
    fn an_unreachable_tracker_gives_a_null_title() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1", "HR-2", "HR-3"]);
        let (lookup, asked) = Titles::lookup(vec![
            ("HR-1", Answer::Down),
            ("HR-2", Answer::Down),
            ("HR-3", Answer::Down),
        ]);

        let intents = install
            .resolve(&analysis, &config(&["work-item"], &[]), None, lookup)
            .expect("resolve");

        assert_eq!(
            intents,
            [
                work_item("HR-1", None),
                work_item("HR-2", None),
                work_item("HR-3", None)
            ]
        );
        assert_eq!(
            *asked.borrow(),
            ["HR-1"],
            "the tracker is asked until it is down"
        );
    }

    #[test]
    fn a_tracker_failure_that_is_no_unreachable_tracker_is_a_failure() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1"]);
        let (lookup, _) = Titles::lookup(vec![("HR-1", Answer::Fails)]);

        assert_eq!(
            install.resolve(&analysis, &config(&["work-item"], &[]), None, lookup),
            Err("the backlog is broken".to_string())
        );
    }

    #[test]
    fn a_marker_that_cannot_be_read_stops_the_work_item_adapter_only() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1"]);
        let broken = || Err("marker: tracker must be an object".to_string());

        assert_eq!(
            install.resolve(&analysis, &config(&["work-item"], &[]), None, broken()),
            Err("marker: tracker must be an object".to_string())
        );
        assert_eq!(
            install.resolve(&analysis, &config(&["file"], &[]), None, broken()),
            Ok(Vec::new())
        );
    }

    #[test]
    fn a_named_work_item_gets_its_title_whatever_the_config() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &[]);
        let (lookup, _) = Titles::lookup(vec![("HR-9", titled("HR-9", "Ninth."))]);

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &[]),
                Some(&named("work-item:HR-9")),
                lookup,
            )
            .expect("resolve");

        assert_eq!(intents, [work_item("HR-9", Some("Ninth."))]);
    }

    #[test]
    fn a_named_work_item_that_the_range_also_names_is_one_intent() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1"]);
        let (lookup, _) = Titles::lookup(vec![("HR-1", titled("HR-1", "First."))]);

        let intents = install
            .resolve(
                &analysis,
                &config(&["work-item"], &[]),
                Some(&named("work-item:HR-1")),
                lookup,
            )
            .expect("resolve");

        assert_eq!(intents, [work_item("HR-1", Some("First."))]);
    }

    #[test]
    fn a_named_work_item_that_the_tracker_does_not_hold_is_listed_with_its_line() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &[]);
        let (lookup, _) = Titles::lookup(vec![]);

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["work-item"], &[]),
                Some(&named("work-item:HR-404")),
                lookup
            ),
            Ok(vec![unresolved(
                "work-item",
                "HR-404",
                "the tracker holds no such work item"
            )])
        );
    }

    #[test]
    fn a_named_work_item_on_an_unreachable_tracker_has_a_null_title() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &[]);
        let (lookup, _) = Titles::lookup(vec![("HR-9", Answer::Down)]);

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &[]),
                Some(&named("work-item:HR-9")),
                lookup,
            )
            .expect("resolve");

        assert_eq!(intents, [work_item("HR-9", None)]);
    }

    #[test]
    fn a_named_work_item_with_no_tracker_has_a_null_title() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &[]);

        for state in [NoTracker::Bare, NoTracker::BacklogDir] {
            assert_eq!(
                install.resolve(
                    &analysis,
                    &config(&["work-item"], &[]),
                    Some(&named("work-item:HR-1")),
                    Ok(Lookup::Absent(state))
                ),
                Ok(vec![work_item("HR-1", None)])
            );
        }
    }

    /// A range that adds `docs/plans/a.md` and `docs/other.md` on top of an
    /// install, and the analysis of it.
    fn range_with_a_plan(folder: &'static str) -> (Install, Analysis) {
        let install = Install::new(folder);
        let base = install.head();
        install.write(
            "docs/plans/a.md",
            "# Plan A\n\nWhy A.\n\n- [x] one\n- [ ] two\n",
        );
        install.write("docs/other.md", "# Other\n\nNot a plan.\n");
        let head = install.commit("plan");
        let analysis = install.analysis(&base, &head);
        (install, analysis)
    }

    #[test]
    fn the_file_adapter_reads_a_changed_path_that_matches_review_intent_files() {
        let (install, analysis) = range_with_a_plan("");

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                None,
                no_tracker(),
            )
            .expect("resolve");

        assert_eq!(
            intents,
            [file(
                "docs/plans/a.md",
                Some("Plan A"),
                Some("Why A."),
                (1, 2)
            )]
        );
    }

    #[test]
    fn the_file_adapter_reads_nothing_while_review_intent_files_is_empty() {
        let (install, analysis) = range_with_a_plan("");

        assert_eq!(
            install.resolve(&analysis, &config(&["file"], &[]), None, no_tracker()),
            Ok(Vec::new())
        );
    }

    #[test]
    fn a_path_below_a_subdirectory_install_is_relative_to_the_install_root() {
        let (install, analysis) = range_with_a_plan("sub/");

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                None,
                no_tracker(),
            )
            .expect("resolve");
        assert_eq!(
            intents,
            [file(
                "docs/plans/a.md",
                Some("Plan A"),
                Some("Why A."),
                (1, 2)
            )]
        );

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &[]),
                Some(&named("file:docs/other.md")),
                no_tracker(),
            )
            .expect("resolve");
        assert_eq!(
            intents,
            [file(
                "docs/other.md",
                Some("Other"),
                Some("Not a plan."),
                (0, 0)
            )]
        );
    }

    #[test]
    fn the_file_adapter_reads_the_head_and_not_the_work_tree() {
        let (install, analysis) = range_with_a_plan("");
        install.write("docs/plans/a.md", "# Changed after the commit\n");

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                None,
                no_tracker(),
            )
            .expect("resolve");

        assert_eq!(
            intents,
            [file(
                "docs/plans/a.md",
                Some("Plan A"),
                Some("Why A."),
                (1, 2)
            )]
        );
    }

    #[test]
    fn a_renamed_plan_is_read_at_its_new_path() {
        let install = Install::new("");
        install.write("docs/plans/old.md", "# Old\n\nWhy.\n");
        let base = install.commit("old");
        install
            .repo
            .git(&["mv", "docs/plans/old.md", "docs/plans/new.md"]);
        let head = install.commit("rename");
        let analysis = install.analysis(&base, &head);

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                None,
                no_tracker(),
            )
            .expect("resolve");

        assert_eq!(
            intents,
            [file("docs/plans/new.md", Some("Old"), Some("Why."), (0, 0))]
        );
    }

    #[test]
    fn a_deleted_and_a_binary_path_that_match_the_globs_give_no_intent() {
        let install = Install::new("");
        install.write("docs/plans/gone.md", "# Gone\n");
        let base = install.commit("gone");
        fs::remove_file(install.root().join("docs/plans/gone.md")).expect("remove");
        install.write_bytes("docs/plans/bin.md", b"\0\x01\x02 not text\0");
        install.write("docs/plans/kept.md", "# Kept\n\nWhy.\n");
        let head = install.commit("change");
        let analysis = install.analysis(&base, &head);

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                None,
                no_tracker(),
            )
            .expect("resolve");

        assert_eq!(
            intents,
            [file(
                "docs/plans/kept.md",
                Some("Kept"),
                Some("Why."),
                (0, 0)
            )]
        );
    }

    // Symlinks need unix: a symlink in the tree holds its target, which no
    // intent file may be.
    #[cfg(unix)]
    #[test]
    fn a_symlink_is_no_intent_file() {
        let install = Install::new("");
        let base = install.head();
        install.write("docs/plans/real.md", "# Real\n\nWhy.\n");
        std::os::unix::fs::symlink("real.md", install.root().join("docs/plans/link.md"))
            .expect("symlink");
        let head = install.commit("link");
        let analysis = install.analysis(&base, &head);

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                None,
                no_tracker(),
            )
            .expect("resolve");
        assert_eq!(
            intents,
            [file(
                "docs/plans/real.md",
                Some("Real"),
                Some("Why."),
                (0, 0)
            )]
        );

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["file"], &[]),
                Some(&named("file:docs/plans/link.md")),
                no_tracker()
            ),
            Ok(vec![unresolved(
                "file",
                "docs/plans/link.md",
                "no regular file at that path at the head"
            )])
        );
    }

    #[test]
    fn a_named_file_is_read_whatever_the_config_and_joins_the_other_intents() {
        let (install, analysis) = range_with_a_plan("");

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                Some(&named("file:docs/other.md")),
                no_tracker(),
            )
            .expect("resolve");

        assert_eq!(
            intents,
            [
                file("docs/other.md", Some("Other"), Some("Not a plan."), (0, 0)),
                file("docs/plans/a.md", Some("Plan A"), Some("Why A."), (1, 2)),
            ]
        );
    }

    #[test]
    fn a_named_file_that_a_glob_also_matches_is_one_intent() {
        let (install, analysis) = range_with_a_plan("");

        let intents = install
            .resolve(
                &analysis,
                &config(&["file"], &["docs/plans/*.md"]),
                Some(&named("file:docs/plans/a.md")),
                no_tracker(),
            )
            .expect("resolve");

        assert_eq!(intents.len(), 1);
    }

    #[test]
    fn a_named_file_that_is_no_regular_file_at_the_head_is_listed_with_its_line() {
        let (install, analysis) = range_with_a_plan("");
        let line = "no regular file at that path at the head";

        for reference in ["docs/plans/missing.md", "docs", "docs/plans/a", "docs/pl*"] {
            assert_eq!(
                install.resolve(
                    &analysis,
                    &config(&["file"], &[]),
                    Some(&named(&format!("file:{reference}"))),
                    no_tracker()
                ),
                Ok(vec![unresolved("file", reference, line)]),
                "{reference}"
            );
        }
    }

    #[test]
    fn a_named_path_that_leaves_the_install_or_names_a_folder_is_listed_with_its_line() {
        let (install, analysis) = range_with_a_plan("sub/");
        let line = "the path must be relative to the install, with no empty, . or .. folder";

        for reference in [
            "../x.md",
            "/etc/passwd",
            "docs/../x.md",
            "./docs/other.md",
            "docs//other.md",
            "docs/",
        ] {
            assert_eq!(
                install.resolve(
                    &analysis,
                    &config(&["file"], &[]),
                    Some(&named(&format!("file:{reference}"))),
                    no_tracker()
                ),
                Ok(vec![unresolved("file", reference, line)]),
                "{reference}"
            );
        }
    }

    #[test]
    fn a_named_source_that_is_no_adapter_is_listed_with_its_line() {
        let (install, analysis) = range_with_a_plan("");

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["file"], &[]),
                Some(&named("spec-kit:add-x")),
                no_tracker()
            ),
            Ok(vec![unresolved(
                "spec-kit",
                "add-x",
                "\"spec-kit\" is no intent source; use one of: work-item, file, openspec"
            )])
        );
    }

    /// An install whose range adds the change `add-x` of OpenSpec and a plan
    /// file, and the analysis of the range.
    fn range_with_a_change(folder: &'static str) -> (Install, Analysis) {
        let install = Install::new(folder);
        let base = install.head();
        install.write(
            "openspec/changes/add-x/proposal.md",
            "# Add X\n\n## Why\nBecause.\n",
        );
        install.write("openspec/changes/add-x/tasks.md", "- [x] a\n- [ ] b\n");
        install.write("docs/plans/a.md", "# Plan A\n\nWhy A.\n");
        let head = install.commit("change");
        let analysis = install.analysis(&base, &head);
        (install, analysis)
    }

    /// The intent that the `openspec` adapter gives for `add-x` of
    /// [`range_with_a_change`].
    fn add_x() -> Intent {
        Intent {
            source: "openspec".to_string(),
            reference: "add-x".to_string(),
            title: Some("Add X".to_string()),
            why: Some("Because.".to_string()),
            tasks: Some(Tasks { done: 1, total: 2 }),
            requirements: None,
            validation: None,
            artifacts: vec![
                "openspec/changes/add-x/proposal.md".to_string(),
                "openspec/changes/add-x/tasks.md".to_string(),
            ],
            unresolved: None,
        }
    }

    #[test]
    fn the_openspec_adapter_reads_the_changes_that_the_changed_paths_name() {
        let (install, analysis) = range_with_a_change("");

        assert_eq!(
            install.resolve(&analysis, &config(&["openspec"], &[]), None, no_tracker()),
            Ok(vec![add_x()])
        );
        assert_eq!(
            install.resolve(&analysis, &config(&["file"], &[]), None, no_tracker()),
            Ok(Vec::new()),
            "an adapter that the config leaves out reads nothing"
        );
    }

    #[test]
    fn a_named_openspec_change_resolves_whatever_the_config_lists() {
        let (install, analysis) = range_with_a_change("sub/");

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["file"], &[]),
                Some(&named("openspec:add-x")),
                no_tracker()
            ),
            Ok(vec![add_x()])
        );
    }

    #[test]
    fn a_named_openspec_change_that_the_range_also_changes_is_one_intent() {
        let (install, analysis) = range_with_a_change("");

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["openspec"], &[]),
                Some(&named("openspec:add-x")),
                no_tracker()
            ),
            Ok(vec![add_x()])
        );
    }

    #[test]
    fn a_named_openspec_change_that_does_not_resolve_is_listed_with_its_line() {
        let (install, analysis) = range_with_a_plan("");

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["openspec"], &[]),
                Some(&named("openspec:add-x")),
                no_tracker()
            ),
            Ok(vec![unresolved(
                "openspec",
                "add-x",
                "no openspec/ folder at the head"
            )])
        );
    }

    #[test]
    fn the_openspec_adapter_comes_after_the_listed_adapters_in_the_order_of_all_of_them() {
        let (install, analysis) = range_with_a_change("");
        let refs = |sources: &[&str], named: Option<&IntentRef>| -> Vec<String> {
            install
                .resolve(
                    &analysis,
                    &config(sources, &["docs/plans/*.md"]),
                    named,
                    no_tracker(),
                )
                .expect("resolve")
                .into_iter()
                .map(|intent| format!("{} {}", intent.source, intent.reference))
                .collect()
        };

        assert_eq!(
            refs(&["openspec", "file"], None),
            ["openspec add-x", "file docs/plans/a.md"]
        );
        assert_eq!(
            refs(&["file", "openspec"], None),
            ["file docs/plans/a.md", "openspec add-x"]
        );
        assert_eq!(
            refs(&["file"], Some(&named("openspec:add-x"))),
            ["file docs/plans/a.md", "openspec add-x"],
            "a named intent of an adapter that the config leaves out comes after the listed ones"
        );
    }

    #[test]
    fn found_in_gives_the_intents_of_the_changed_paths_of_the_adapters_it_is_given() {
        let (install, analysis) = range_with_a_change("");
        let sources = config(&["file", "openspec"], &["docs/plans/*.md"]);
        let found = |adapters: &[Adapter]| {
            found_in(&install.root(), &analysis, &sources, adapters).expect("found")
        };

        assert_eq!(found(&[Adapter::OpenSpec]), vec![add_x()]);
        assert_eq!(
            found(&[Adapter::File]),
            vec![file(
                "docs/plans/a.md",
                Some("Plan A"),
                Some("Why A."),
                (0, 0)
            )]
        );
        assert_eq!(
            found(&[Adapter::OpenSpec, Adapter::File])
                .iter()
                .map(|intent| intent.source.as_str())
                .collect::<Vec<_>>(),
            ["openspec", "file"],
            "the order of the adapters that the caller gives"
        );
        assert_eq!(found(&[]), Vec::new());
    }

    #[test]
    fn found_in_fails_with_the_line_of_a_failure_that_is_no_named_intents_own() {
        let (install, mut analysis) = range_with_a_change("");
        analysis.head = "0".repeat(40);

        let result = found_in(
            &install.root(),
            &analysis,
            &config(&["openspec"], &[]),
            &[Adapter::OpenSpec],
        );

        assert!(result.is_err(), "{result:?}");
    }

    #[test]
    fn a_failure_that_is_not_the_named_intents_own_is_no_entry_of_the_list() {
        let (install, mut analysis) = range_with_a_plan("");
        analysis.work_items = vec!["HR-1".to_string()];
        let (broken, _) = Titles::lookup(vec![("HR-1", Answer::Fails)]);
        let (healthy, _) = Titles::lookup(vec![("HR-1", titled("HR-1", "First."))]);

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["work-item"], &[]),
                Some(&named("file:docs/other.md")),
                broken
            ),
            Err("the backlog is broken".to_string()),
            "a configured adapter fails while the named file resolves"
        );
        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["work-item"], &[]),
                Some(&named("file:docs/missing.md")),
                healthy
            ),
            Ok(vec![
                work_item("HR-1", Some("First.")),
                unresolved(
                    "file",
                    "docs/missing.md",
                    "no regular file at that path at the head"
                )
            ]),
            "the named file does not resolve"
        );
    }

    #[test]
    fn a_failure_of_the_named_intents_lookup_is_a_failure_not_an_entry() {
        let (install, analysis) = range_with_a_plan("");
        let (broken, _) = Titles::lookup(vec![("HR-1", Answer::Fails)]);
        let marker_broken = || Err("marker: tracker must be an object".to_string());
        let named_item = Some(named("work-item:HR-1"));

        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["file"], &[]),
                named_item.as_ref(),
                broken
            ),
            Err("the backlog is broken".to_string()),
            "the command of the tracker fails"
        );
        assert_eq!(
            install.resolve(
                &analysis,
                &config(&["file"], &[]),
                named_item.as_ref(),
                marker_broken()
            ),
            Err("marker: tracker must be an object".to_string()),
            "the marker cannot be read"
        );
    }

    #[test]
    fn a_git_failure_is_a_failure_for_a_named_and_a_configured_file() {
        let (install, mut analysis) = range_with_a_plan("");
        analysis.head = "0".repeat(40);

        let named_file = install.resolve(
            &analysis,
            &config(&["file"], &[]),
            Some(&named("file:docs/other.md")),
            no_tracker(),
        );
        assert!(named_file.is_err(), "{named_file:?}");
        let configured_file = install.resolve(
            &analysis,
            &config(&["file"], &["docs/plans/*.md"]),
            None,
            no_tracker(),
        );
        assert!(configured_file.is_err(), "{configured_file:?}");
    }

    #[test]
    fn resolve_takes_the_config_that_the_marker_read_checked() {
        let (install, mut analysis) = range_with_a_plan("");
        analysis.work_items = vec!["HR-1".to_string()];
        install.write(
            ".houserules.json",
            &json!({"review": {"intent": {
                "sources": ["file", "work-item", "file"],
                "files": ["docs/plans/*.md"]
            }}})
            .to_string(),
        );
        let review = Marker::read(&install.root())
            .expect("a marker")
            .review()
            .expect("a checked review key");

        let intents = resolve(&install.root(), &analysis, &review.intent, &[]).expect("resolve");

        assert_eq!(
            intents,
            [file(
                "docs/plans/a.md",
                Some("Plan A"),
                Some("Why A."),
                (1, 2)
            )],
            "the file adapter runs on the globs of the marker, and an install with no tracker gives no work item intent"
        );
    }

    #[test]
    fn a_source_listed_twice_runs_once() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1"]);
        let (lookup, asked) = Titles::lookup(vec![("HR-1", titled("HR-1", "First."))]);

        let intents = install
            .resolve(
                &analysis,
                &config(&["work-item", "work-item"], &[]),
                None,
                lookup,
            )
            .expect("resolve");

        assert_eq!(intents, [work_item("HR-1", Some("First."))]);
        assert_eq!(*asked.borrow(), ["HR-1"]);
    }

    #[test]
    fn intents_come_in_adapter_order_then_by_ref() {
        let install = Install::new("");
        let base = install.head();
        install.write("docs/plans/b.md", "# Plan B\n");
        install.write("docs/plans/a.md", "# Plan A\n");
        install.write("docs/other.md", "# Other\n");
        let head = install.commit("plans");
        let mut analysis = install.analysis(&base, &head);
        analysis.work_items = vec!["HR-2".to_string(), "HR-1".to_string()];
        let answers = || {
            vec![
                ("HR-1", titled("HR-1", "One.")),
                ("HR-2", titled("HR-2", "Two.")),
                ("HR-3", titled("HR-3", "Three.")),
            ]
        };
        let refs = |sources: &[&str], named: Option<&IntentRef>| -> Vec<String> {
            let (lookup, _) = Titles::lookup(answers());
            install
                .resolve(
                    &analysis,
                    &config(sources, &["docs/plans/*.md"]),
                    named,
                    lookup,
                )
                .expect("resolve")
                .into_iter()
                .map(|intent| format!("{} {}", intent.source, intent.reference))
                .collect()
        };

        assert_eq!(
            refs(&["file", "work-item"], None),
            [
                "file docs/plans/a.md",
                "file docs/plans/b.md",
                "work-item HR-1",
                "work-item HR-2"
            ]
        );
        assert_eq!(
            refs(&["work-item", "file"], None),
            [
                "work-item HR-1",
                "work-item HR-2",
                "file docs/plans/a.md",
                "file docs/plans/b.md"
            ]
        );
        assert_eq!(
            refs(&["work-item"], Some(&named("file:docs/other.md"))),
            ["work-item HR-1", "work-item HR-2", "file docs/other.md"],
            "an adapter that the config leaves out comes after the configured ones, and runs only for the named intent"
        );
        assert_eq!(
            refs(&["file", "work-item"], Some(&named("work-item:HR-3"))),
            [
                "file docs/plans/a.md",
                "file docs/plans/b.md",
                "work-item HR-1",
                "work-item HR-2",
                "work-item HR-3"
            ]
        );
    }

    #[test]
    fn an_unresolved_named_intent_sits_among_the_others_in_adapter_order_then_ref() {
        let install = Install::new("");
        let base = install.head();
        install.write("docs/plans/b.md", "# Plan B\n");
        let head = install.commit("plan");
        let mut analysis = install.analysis(&base, &head);
        analysis.work_items = vec!["HR-1".to_string()];
        let line = "no regular file at that path at the head";
        let (lookup, _) = Titles::lookup(vec![("HR-1", titled("HR-1", "One."))]);

        let intents = install
            .resolve(
                &analysis,
                &config(&["work-item", "file"], &["docs/plans/*.md"]),
                Some(&named("file:docs/other/0.md")),
                lookup,
            )
            .expect("resolve");

        assert_eq!(
            intents,
            [
                work_item("HR-1", Some("One.")),
                unresolved("file", "docs/other/0.md", line),
                file("docs/plans/b.md", Some("Plan B"), None, (0, 0)),
            ]
        );
    }

    #[test]
    fn an_unresolved_named_work_item_that_the_range_names_is_one_entry_with_its_line() {
        let install = Install::new("");
        let analysis = analysis_with_work_items(&install, &["HR-1", "HR-404"]);
        let (lookup, _) = Titles::lookup(vec![("HR-1", titled("HR-1", "First."))]);

        let intents = install
            .resolve(
                &analysis,
                &config(&["work-item"], &[]),
                Some(&named("work-item:HR-404")),
                lookup,
            )
            .expect("resolve");

        assert_eq!(
            intents,
            [
                work_item("HR-1", Some("First.")),
                unresolved("work-item", "HR-404", "the tracker holds no such work item"),
            ]
        );
    }

    // The contract-shape tests below read the fixture folders that the spec
    // workflows leave on disk and fill each column of the table of spec 4.9 by
    // hand. They test the shape of the contract, not an adapter.

    /// The root of a fixture folder.
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/intent")
            .join(name)
    }

    /// The text of the file `path` of a fixture folder.
    fn fixture_text(name: &str, path: &str) -> String {
        fs::read_to_string(fixture(name).join(path)).expect("read the fixture")
    }

    /// The files below `folder` of a fixture, as `/`-separated paths from the
    /// fixture root, sorted.
    fn fixture_files(name: &str, folder: &str) -> Vec<String> {
        let root = fixture(name);
        let mut files: Vec<String> = WalkDir::new(root.join(folder))
            .into_iter()
            .map(|entry| entry.expect("walk the fixture"))
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                entry
                    .path()
                    .strip_prefix(&root)
                    .expect("below the root")
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/")
            })
            .collect();
        files.sort();
        files
    }

    /// Asserts that `intent` serializes, reads back unchanged, and equals the
    /// `intent.json` of the fixture.
    fn assert_the_contract_shape(name: &str, intent: &Intent) {
        let text = serde_json::to_string(intent).expect("serialize");
        let back: Intent = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(&back, intent);

        let on_disk: Value =
            serde_json::from_str(&fixture_text(name, "intent.json")).expect("parse intent.json");
        assert_eq!(serde_json::to_value(intent).expect("to value"), on_disk);
    }

    /// The number of lines below the heading `## <kind> Requirements` of an
    /// OpenSpec delta that `matches`, up to the next `## ` heading.
    fn delta_count(delta: &str, kind: &str, matches: impl Fn(&str) -> bool) -> u64 {
        let heading = format!("## {kind} Requirements");
        let section = delta
            .lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| !line.starts_with("## "));
        section.filter(|line| matches(line)).count() as u64
    }

    #[test]
    fn the_openspec_fixture_fills_every_field_of_the_contract() {
        let name = "openspec-change";
        let folder = "openspec/changes/add-rate-limit";
        let proposal = fixture_text(name, &format!("{folder}/proposal.md"));
        let tasks = fixture_text(name, &format!("{folder}/tasks.md"));
        let delta = fixture_text(name, &format!("{folder}/specs/api/spec.md"));
        let validate: Value = serde_json::from_str(&fixture_text(name, "validate.json"))
            .expect("parse validate.json");
        let intent = Intent {
            source: "openspec".to_string(),
            reference: "add-rate-limit".to_string(),
            title: Some("Add rate limiting to the public API".to_string()),
            why: Some("One client that retries in a tight loop takes the API down for every other client. A limit for each client keeps a noisy client from starving the rest.".to_string()),
            tasks: Some(Tasks { done: 2, total: 4 }),
            requirements: Some(Requirements {
                added: Some(2),
                modified: Some(1),
                removed: Some(1),
                renamed: Some(1),
                total: None,
            }),
            validation: Some(Validation {
                tool: "openspec".to_string(),
                ok: true,
                summary: "1 of 1 valid".to_string(),
            }),
            artifacts: fixture_files(name, folder),
            unresolved: None,
        };

        assert_the_contract_shape(name, &intent);
        let title = intent.title.as_deref().expect("a title");
        assert!(proposal.starts_with(&format!("# {title}\n")));
        let why = intent.why.as_deref().expect("a why");
        assert!(proposal.contains(&format!("## Why\n{why}\n")));
        assert_eq!(intent.tasks, Some(count_tasks(&tasks)));
        let requirements = intent.requirements.expect("requirements");
        let header = |line: &str| line.starts_with("### Requirement: ");
        assert_eq!(
            requirements.added,
            Some(delta_count(&delta, "ADDED", header))
        );
        assert_eq!(
            requirements.modified,
            Some(delta_count(&delta, "MODIFIED", header))
        );
        assert_eq!(
            requirements.removed,
            Some(delta_count(&delta, "REMOVED", header))
        );
        assert_eq!(
            requirements.renamed,
            Some(delta_count(&delta, "RENAMED", |line| line.starts_with("- FROM: ")))
        );
        let results = &validate["results"]["changes"];
        assert_eq!(results.as_array().map(Vec::len), Some(1));
        let summary = &validate["summary"];
        assert_eq!(
            intent.validation.as_ref().map(|v| v.ok),
            Some(summary["invalid"] == 0 && results[0]["valid"] == true)
        );
        assert_eq!(
            intent.validation.map(|v| v.summary),
            Some(format!(
                "{} of {} valid",
                summary["valid"], summary["total"]
            ))
        );
    }

    #[test]
    fn the_spec_kit_fixture_fills_every_field_of_the_contract() {
        let name = "spec-kit-feature";
        let folder = "specs/001-rate-limit";
        let spec = fixture_text(name, &format!("{folder}/spec.md"));
        let tasks = fixture_text(name, &format!("{folder}/tasks.md"));
        let feature: Value = serde_json::from_str(&fixture_text(name, ".specify/feature.json"))
            .expect("parse feature.json");
        let intent = Intent {
            source: "spec-kit".to_string(),
            reference: "001-rate-limit".to_string(),
            title: Some("Feature Specification: Rate limiting".to_string()),
            why: Some("Limit the requests of each API client so that one noisy client cannot starve the rest.".to_string()),
            tasks: Some(Tasks { done: 1, total: 3 }),
            requirements: Some(Requirements {
                added: None,
                modified: None,
                removed: None,
                renamed: None,
                total: Some(3),
            }),
            validation: None,
            artifacts: fixture_files(name, folder),
            unresolved: None,
        };

        assert_the_contract_shape(name, &intent);
        assert_eq!(feature["feature_directory"], folder);
        assert_eq!(
            intent.reference,
            Path::new(folder)
                .file_name()
                .and_then(|part| part.to_str())
                .expect("a folder name")
        );
        let title = intent.title.as_deref().expect("a title");
        assert!(spec.starts_with(&format!("# {title}\n")));
        let why = intent.why.as_deref().expect("a why");
        assert!(spec.contains(&format!("**Input**: User description: \"{why}\"")));
        assert_eq!(intent.tasks, Some(count_tasks(&tasks)));
        let ids = spec
            .lines()
            .filter(|line| line.starts_with("- **FR-"))
            .count() as u64;
        assert_eq!(intent.requirements.expect("requirements").total, Some(ids));
    }

    // ---- named intents of a list: the trailers (batch 38, spec 5.1 and 5.2) ---------------

    /// The intent that `source` and `reference` name, as a caller that does not
    /// check them names it.
    fn ref_of(source: &str, reference: &str) -> IntentRef {
        IntentRef {
            source: source.to_string(),
            reference: reference.to_string(),
        }
    }

    /// The `<source>:<ref>` of each intent, in order.
    fn identities(intents: &[Intent]) -> Vec<String> {
        intents
            .iter()
            .map(|intent| format!("{}:{}", intent.source, intent.reference))
            .collect()
    }

    /// A value that is no `<source>:<ref>` splits at its first colon, trimmed; a
    /// value with no colon has an empty source and the trimmed value as its ref.
    #[test]
    fn a_malformed_trailer_value_splits_at_its_first_colon() {
        for (text, source, reference) in [
            ("oops", "", "oops"),
            ("  oops  ", "", "oops"),
            ("file:", "file", ""),
            (":x", "", "x"),
            (" : ", "", ""),
            ("a:b:c", "a", "b:c"),
            ("file: docs/a.md ", "file", "docs/a.md"),
        ] {
            assert_eq!(
                IntentRef::split(text),
                ref_of(source, reference),
                "{text:?}"
            );
        }
    }

    /// A named intent with an empty source or an empty ref is a malformed value:
    /// it is the unresolved entry with the line of a text that is no
    /// `<source>:<ref>`, whatever the adapter of its source would say.
    #[test]
    fn a_named_intent_with_an_empty_part_is_unresolved_with_the_malformed_line() {
        let install = Install::new("");
        let base = install.head();
        install.write("docs/a.md", "# A\n");
        let head = install.commit("docs: a");
        let analysis = install.analysis(&base, &head);
        let malformed = [
            IntentRef::split("oops"),
            IntentRef::split(":x"),
            IntentRef::split("file:"),
        ];

        let intents = resolve_with(
            &install.root(),
            &analysis,
            &config(&["work-item"], &[]),
            &malformed,
            no_tracker(),
        )
        .expect("resolve");

        assert_eq!(identities(&intents), ["file:", ":oops", ":x"]);
        assert!(
            intents
                .iter()
                .all(|intent| intent.unresolved.as_deref() == Some(NOT_SOURCE_AND_REF)),
            "{intents:?}"
        );
    }

    /// A list of named intents counts each `<source>:<ref>` once, and an intent
    /// that an adapter also finds is one intent.
    #[test]
    fn a_named_intent_that_the_list_repeats_counts_once() {
        let install = Install::new("");
        let base = install.head();
        install.write("docs/plans/p.md", "# P\n");
        install.write("docs/plans/q.md", "# Q\n");
        let head = install.commit("docs: plans");
        let analysis = install.analysis(&base, &head);
        let names = [
            named("file:docs/plans/p.md"),
            named("file:docs/plans/q.md"),
            named("file:docs/plans/p.md"),
        ];

        let intents = resolve_with(
            &install.root(),
            &analysis,
            &config(&["file"], &["docs/plans/*.md"]),
            &names,
            no_tracker(),
        )
        .expect("resolve");

        assert_eq!(
            identities(&intents),
            ["file:docs/plans/p.md", "file:docs/plans/q.md"]
        );
    }

    /// The intents of a source that is no adapter sort by source in byte order and
    /// then by ref, after the intents of every adapter; an adapter that the
    /// configuration does not list keeps the order of `ADAPTERS` (spec 5.2).
    #[test]
    fn intents_of_sources_that_are_no_adapter_sort_by_source_then_ref() {
        let entry = |source: &str, reference: &str| unresolved(source, reference, "line");
        let mut intents = vec![
            entry("zzz", "1"),
            entry("file", "b"),
            entry("aaa", "9"),
            entry("work-item", "w"),
            entry("aaa", "2"),
            entry("", "x"),
        ];

        ordered(&mut intents, &[Adapter::OpenSpec]);

        assert_eq!(
            identities(&intents),
            ["work-item:w", "file:b", ":x", "aaa:2", "aaa:9", "zzz:1"]
        );
    }
}
