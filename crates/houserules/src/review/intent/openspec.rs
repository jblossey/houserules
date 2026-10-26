//! The `openspec` intent adapter (batch 38, spec 3; HR-209).
//!
//! The adapter reads an OpenSpec change from the tree of a commit and runs no
//! tool. A change is the folder `openspec/changes/<name>/` of the install, or an
//! archived folder `openspec/changes/archive/<YYYY-MM-DD>-<name>/`; its `ref` is
//! OpenSpec's id: the folder name. Each field reads each file as the OpenSpec
//! 1.15.0 function that spec 2 names reads it (see [`markdown`] and [`tasks`]).
//!
//! - [`found`] gives the intents of the changes that changed paths name: a path
//!   below `openspec/changes/<name>/`, or below `openspec/changes/archive/<name>/`.
//!   A name that starts with `.` is no change. An active change whose folder is
//!   gone at the commit follows its archived folder of the latest date, and gives
//!   no intent when there is none; a change that the range renamed is found by
//!   its new name.
//! - [`named`] gives the intent of `openspec:<id>`: the active folder, else the
//!   archived folder `<id>`, else the archived folder `<YYYY-MM-DD>-<id>` of the
//!   latest date. A name that no such folder holds, or a tree that cannot be read
//!   as a root, is an entry that carries the line of spec 3.5.
//! - [`root_kind`] tells what the `openspec/` folder of a list of tracked paths
//!   is, for `render` and for [`named`].
//!
//! A symlink and a submodule are no file of a change. An OpenSpec root above the
//! install, and a store that a pointer names, are not read. A `changes/` folder
//! that holds store metadata is a store root and holds no change (spec 3.1).

mod markdown;
mod outputs;
mod schema;
mod tasks;
mod tree;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use saphyr::{LoadableYamlNode, Yaml};

use self::markdown::Deltas;
use self::tree::Tree;
use super::{IntentRef, unresolved};
use crate::published::intent::{Intent, Requirements, Tasks};

/// The adapter's name in the config, the bundle, and the packet.
const SOURCE: &str = "openspec";

/// The folder of the changes, from the install root.
const CHANGES: &str = "openspec/changes";

/// The folder of the archived changes, from the install root.
const ARCHIVE: &str = "openspec/changes/archive";

/// The config of an OpenSpec root, from the install root.
const CONFIG: &str = "openspec/config.yaml";

/// The config under its other name, read when the first is absent.
const CONFIG_ALTERNATE: &str = "openspec/config.yml";

/// The line for a tree with no `openspec/` folder (spec 3.5).
const NO_FOLDER_LINE: &str = "no openspec/ folder at the head";

/// The line for a root that points at a store (spec 3.5).
const STORE_POINTER_LINE: &str =
    "the OpenSpec root points at a store, which houserules does not read";

/// The line for a change id that is not kebab-case (spec 3.5).
const NOT_KEBAB_LINE: &str =
    "an OpenSpec change id is kebab-case: lowercase letters, digits, and single hyphens";

/// The line for a change that the tree does not hold (spec 3.5).
const NO_SUCH_CHANGE_LINE: &str = "no OpenSpec change with that id at the head, active or archived";

/// What the `openspec/` folder of a tree is (spec 3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootKind {
    /// The tree holds neither a planning shape nor a store pointer.
    Absent,
    /// `openspec/specs/` or `openspec/changes/` is a folder that holds no store
    /// metadata.
    Planning,
    /// The root has no planning shape, and its config holds a `store:` key.
    StorePointer,
}

/// What the `openspec/` folder is for the tracked paths `paths` (from the
/// install root) and the text `config` of `openspec/config.yaml`, else of
/// `openspec/config.yml` (spec 3.1). A planning shape wins over a `store:` key.
/// A config that does not parse as YAML, or that is no mapping, holds no key.
pub(crate) fn root_kind(paths: &[String], config: Option<&str>) -> RootKind {
    let planning = ["specs", "changes"].iter().any(|folder| {
        let prefix = format!("openspec/{folder}/");
        paths.iter().any(|path| path.starts_with(&prefix))
            && !paths.contains(&store_metadata(folder))
    });
    if planning {
        return RootKind::Planning;
    }
    let points_at_a_store = config
        .and_then(document)
        .is_some_and(|config| config.as_mapping_get("store").is_some());
    if points_at_a_store {
        RootKind::StorePointer
    } else {
        RootKind::Absent
    }
}

/// The path of the metadata file that makes `openspec/<folder>/` a store root
/// and no planning folder (spec 3.1).
fn store_metadata(folder: &str) -> String {
    format!("openspec/{folder}/.openspec-store/store.yaml")
}

/// Whether `openspec/changes/` of `tree` is a planning folder, which can hold
/// changes: it holds no store metadata.
fn changes_are_planning(tree: &Tree) -> bool {
    !tree.has_path(&store_metadata("changes"))
}

/// The one document of the YAML text `text`; `None` for a text that fails to
/// parse, that holds no document, and that holds several (the parser that
/// OpenSpec uses refuses those).
fn document(text: &str) -> Option<Yaml<'static>> {
    let mut documents = Yaml::load_from_str(text).ok()?;
    (documents.len() == 1).then(|| documents.remove(0))
}

/// The text of the config of the OpenSpec root of `tree`.
fn config_text(tree: &Tree) -> Result<Option<String>, String> {
    match tree.text(CONFIG)? {
        Some(text) => Ok(Some(text)),
        None => tree.text(CONFIG_ALTERNATE),
    }
}

/// The non-empty `schema` of the config text `config`.
fn config_schema(config: Option<&str>) -> Option<String> {
    config
        .and_then(document)
        .as_ref()
        .and_then(|config| config.as_mapping_get("schema"))
        .and_then(Yaml::as_str)
        .filter(|schema| !schema.is_empty())
        .map(str::to_string)
}

/// A change in the tree: its id and its folder from the install root.
struct Located {
    /// The `ref`: the folder name.
    reference: String,
    /// The folder of the change.
    folder: String,
}

/// The folder `<date>-<id>` of the archive with the latest date, if any. A name
/// that only starts with `id` after a date, such as `<date>-<id>bar`, is
/// another change.
fn latest_archived(tree: &Tree, id: &str) -> Option<Located> {
    tree.child_names(ARCHIVE)
        .into_iter()
        .filter(|name| name.get(11..) == Some(id) && has_date_prefix(name))
        .max()
        .map(|name| Located {
            folder: format!("{ARCHIVE}/{name}"),
            reference: name,
        })
}

/// Whether `name` starts with a date `YYYY-MM-DD` and a hyphen.
fn has_date_prefix(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() > 11
        && bytes[..11]
            .iter()
            .enumerate()
            .all(|(index, byte)| match index {
                4 | 7 | 10 => *byte == b'-',
                _ => byte.is_ascii_digit(),
            })
}

/// The change that `openspec:<id>` names: the active folder (never `archive`,
/// which is no change id), else the archived folder `<id>`, else the archived
/// folder `<date>-<id>` of the latest date. A `changes/` folder that is no
/// planning folder holds none.
fn locate_named(tree: &Tree, id: &str) -> Option<Located> {
    if !changes_are_planning(tree) {
        return None;
    }
    let active = format!("{CHANGES}/{id}");
    if id != "archive" && tree.has_folder(&active) {
        return Some(Located {
            reference: id.to_string(),
            folder: active,
        });
    }
    let archived = format!("{ARCHIVE}/{id}");
    if tree.has_folder(&archived) {
        return Some(Located {
            reference: id.to_string(),
            folder: archived,
        });
    }
    latest_archived(tree, id)
}

/// A change that a changed path points at, before the tree is asked for it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Candidate {
    /// A folder below `openspec/changes/`: the name of the folder.
    Active(String),
    /// A folder below `openspec/changes/archive/`: the full name of the folder.
    Archived(String),
}

/// The change that the path `path` is a file of, if it is one: a file below
/// `openspec/changes/<name>/` or below `openspec/changes/archive/<name>/`, with a
/// `<name>` that does not start with `.`.
fn candidate_of(path: &str) -> Option<Candidate> {
    let mut parts = path.split('/');
    if parts.next() != Some("openspec") || parts.next() != Some("changes") {
        return None;
    }
    let first = parts.next()?;
    let rest: Vec<&str> = parts.collect();
    let (name, below) = if first == "archive" {
        (*rest.first()?, rest.len() > 1)
    } else {
        (first, !rest.is_empty())
    };
    if !below || name.is_empty() || name.starts_with('.') {
        return None;
    }
    Some(if first == "archive" {
        Candidate::Archived(name.to_string())
    } else {
        Candidate::Active(name.to_string())
    })
}

/// The change in the tree that `candidate` stands for, if the tree holds it.
fn locate_found(tree: &Tree, candidate: &Candidate) -> Option<Located> {
    match candidate {
        Candidate::Active(name) => {
            let folder = format!("{CHANGES}/{name}");
            if tree.has_folder(&folder) {
                Some(Located {
                    reference: name.clone(),
                    folder,
                })
            } else {
                latest_archived(tree, name)
            }
        }
        Candidate::Archived(name) => {
            let folder = format!("{ARCHIVE}/{name}");
            tree.has_folder(&folder).then(|| Located {
                reference: name.clone(),
                folder,
            })
        }
    }
}

/// Whether `relative`, a path from a change folder, is a delta spec: a file named
/// `spec.md` at least one folder below `specs/`, with no name that starts with
/// `.`.
fn is_delta_spec(relative: &str) -> bool {
    let parts: Vec<&str> = relative.split('/').collect();
    parts.len() >= 3
        && parts[0] == "specs"
        && parts.last() == Some(&"spec.md")
        && parts.iter().all(|part| !part.starts_with('.'))
}

/// The intent of the change `located` at the commit of `tree`. `config_schema` is
/// the `schema` of the config.
fn read_change(
    tree: &Tree,
    config_schema: Option<&str>,
    located: &Located,
) -> Result<Intent, String> {
    let folder = &located.folder;
    let artifacts = tree.files_below(folder);
    let relative: Vec<String> = artifacts
        .iter()
        .map(|path| path[folder.len() + 1..].to_string())
        .collect();
    let proposal = tree.text(&format!("{folder}/proposal.md"))?;

    let metadata = tree.text(&format!("{folder}/.openspec.yaml"))?;
    let project_schema = |name: &str| tree.text(&format!("openspec/schemas/{name}/schema.yaml"));
    let tracked = tasks::tracked_files(&tasks::Inputs {
        metadata: metadata.as_deref(),
        config_schema,
        project_schema: &project_schema,
        files: &relative,
    })?;
    let mut task_counts: Option<Tasks> = None;
    for file in &tracked {
        let text = tree.text(&format!("{folder}/{file}"))?.unwrap_or_default();
        let counts = markdown::task_counts(&text);
        let sum = task_counts.get_or_insert(Tasks { done: 0, total: 0 });
        sum.done += counts.done;
        sum.total += counts.total;
    }

    let mut deltas: Option<Deltas> = None;
    for (path, file) in artifacts.iter().zip(&relative) {
        if is_delta_spec(file)
            && let Some(counts) = markdown::delta_counts(&tree.text(path)?.unwrap_or_default())
        {
            *deltas.get_or_insert_default() += counts;
        }
    }

    Ok(Intent {
        source: SOURCE.to_string(),
        reference: located.reference.clone(),
        title: Some(markdown::title(
            proposal.as_deref().unwrap_or_default(),
            &located.reference,
        )),
        why: proposal.as_deref().and_then(markdown::why),
        tasks: task_counts,
        requirements: deltas.map(|deltas| Requirements {
            added: Some(deltas.added),
            modified: Some(deltas.modified),
            removed: Some(deltas.removed),
            renamed: Some(deltas.renamed),
            total: Some(deltas.added + deltas.modified + deltas.removed + deltas.renamed),
        }),
        validation: None,
        artifacts,
        unresolved: None,
    })
}

/// The entry of the named intent `named` at `commit`: the intent of the change
/// that `named.reference` names, or the entry that carries the line of spec 3.5
/// for the first of these that holds: the tree has no `openspec/` folder; its
/// root points at a store; the id is no kebab-case id; the tree holds no such
/// change. The `Err` is the line of a failure that is not the named intent's
/// own.
pub(super) fn named(root: &Path, commit: &str, named: &IntentRef) -> Result<Intent, String> {
    let tree = Tree::read(root, commit)?;
    let paths = tree.paths();
    if paths.is_empty() {
        return Ok(unresolved(named, NO_FOLDER_LINE));
    }
    let config = config_text(&tree)?;
    if root_kind(&paths, config.as_deref()) == RootKind::StorePointer {
        return Ok(unresolved(named, STORE_POINTER_LINE));
    }
    if !markdown::is_change_id(&named.reference) {
        return Ok(unresolved(named, NOT_KEBAB_LINE));
    }
    match locate_named(&tree, &named.reference) {
        Some(located) => read_change(&tree, config_schema(config.as_deref()).as_deref(), &located),
        None => Ok(unresolved(named, NO_SUCH_CHANGE_LINE)),
    }
}

/// The intents of the changes that the changed paths `changed` (from the install
/// root; a deleted path by its old path, a renamed one by both) name, at
/// `commit`, in the order of their refs and without duplicates. A range with no
/// path below `openspec/changes/` reads nothing, and a `changes/` folder that is
/// no planning folder holds no change. The `Err` is the line of a failure.
pub(super) fn found(root: &Path, commit: &str, changed: &[String]) -> Result<Vec<Intent>, String> {
    let candidates: BTreeSet<Candidate> = changed
        .iter()
        .filter_map(|path| candidate_of(path))
        .collect();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let tree = Tree::read(root, commit)?;
    if !changes_are_planning(&tree) {
        return Ok(Vec::new());
    }
    let config = config_text(&tree)?;
    let schema = config_schema(config.as_deref());
    let mut located: BTreeMap<String, Located> = BTreeMap::new();
    for candidate in &candidates {
        if let Some(change) = locate_found(&tree, candidate) {
            located.entry(change.reference.clone()).or_insert(change);
        }
    }
    located
        .values()
        .map(|change| read_change(&tree, schema.as_deref(), change))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::IntentRef;
    use super::*;
    use crate::published::intent::{Intent, Requirements, Tasks};
    use crate::review::git::scratch::Repo;

    /// The line for a tree with no `openspec/` folder.
    const NO_FOLDER: &str = "no openspec/ folder at the head";

    /// The line for a root that points at a store.
    const STORE_POINTER: &str =
        "the OpenSpec root points at a store, which houserules does not read";

    /// The line for a change id that is not kebab-case.
    const NOT_KEBAB: &str =
        "an OpenSpec change id is kebab-case: lowercase letters, digits, and single hyphens";

    /// The line for a change that the tree does not hold.
    const NO_SUCH_CHANGE: &str = "no OpenSpec change with that id at the head, active or archived";

    /// The paths as the owned strings of a tracked-path list.
    fn paths(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_string()).collect()
    }

    // ---- the root (spec 3.1) -------------------------------------------------------------

    #[test]
    fn a_tree_with_no_openspec_file_has_no_root() {
        assert_eq!(root_kind(&[], None), RootKind::Absent);
        assert_eq!(
            root_kind(
                &paths(&["openspec-other/changes/a/x.md", "docs/x.md"]),
                None
            ),
            RootKind::Absent
        );
    }

    #[test]
    fn a_folder_of_specs_or_changes_gives_a_planning_root() {
        for path in [
            "openspec/changes/a/proposal.md",
            "openspec/changes/archive/2026-01-02-a/proposal.md",
            "openspec/specs/auth/spec.md",
            "openspec/changes/.gitkeep",
        ] {
            assert_eq!(
                root_kind(&paths(&[path]), None),
                RootKind::Planning,
                "{path}"
            );
        }
    }

    /// A file named `changes` or `specs`, and a folder whose name only starts
    /// with it, is no planning folder.
    #[test]
    fn only_a_folder_of_the_right_name_gives_a_planning_root() {
        for path in [
            "openspec/changes",
            "openspec/specs",
            "openspec/changesx/a/x.md",
            "openspec/spec/a/x.md",
            "openspec/config.yaml",
            "openspec/schemas/x/schema.yaml",
        ] {
            assert_eq!(root_kind(&paths(&[path]), None), RootKind::Absent, "{path}");
        }
    }

    /// A folder that holds store metadata is a store, not a planning folder.
    #[test]
    fn a_folder_with_store_metadata_is_no_planning_folder() {
        let store = paths(&[
            "openspec/changes/.openspec-store/store.yaml",
            "openspec/changes/x/y.md",
        ]);

        assert_eq!(root_kind(&store, None), RootKind::Absent);
        assert_eq!(
            root_kind(&store, Some("store: team\n")),
            RootKind::StorePointer
        );
        let one_of_two = paths(&[
            "openspec/specs/.openspec-store/store.yaml",
            "openspec/changes/a/proposal.md",
        ]);
        assert_eq!(root_kind(&one_of_two, None), RootKind::Planning);
    }

    /// A `store:` key makes a root without a planning shape a store pointer, and
    /// a planning shape wins over the key.
    #[test]
    fn a_store_key_points_a_root_with_no_planning_shape_at_a_store() {
        let config_only = paths(&["openspec/config.yaml"]);
        let planning = paths(&["openspec/config.yaml", "openspec/changes/a/proposal.md"]);

        assert_eq!(
            root_kind(&config_only, Some("store: team\n")),
            RootKind::StorePointer
        );
        assert_eq!(
            root_kind(&config_only, Some("store:\n")),
            RootKind::StorePointer
        );
        assert_eq!(
            root_kind(&config_only, Some("store: [a, b]\n")),
            RootKind::StorePointer
        );
        assert_eq!(
            root_kind(&config_only, Some("schema: spec-driven\n")),
            RootKind::Absent
        );
        assert_eq!(
            root_kind(&planning, Some("store: team\n")),
            RootKind::Planning
        );
    }

    /// A config that does not parse, is empty, or is no mapping holds no key.
    #[test]
    fn a_config_that_holds_no_key_points_nowhere() {
        let config_only = paths(&["openspec/config.yaml"]);

        for config in [
            "store: [unclosed\n",
            "",
            "- store\n",
            "text\n",
            "# store: team\n",
        ] {
            assert_eq!(
                root_kind(&config_only, Some(config)),
                RootKind::Absent,
                "{config:?}"
            );
        }
        assert_eq!(root_kind(&config_only, None), RootKind::Absent);
    }

    // ---- a change in a repository --------------------------------------------------------

    /// A repository whose tree holds the change `a` (with a delta, tasks, and a
    /// why) and the archived change `2026-01-02-b`, and its first commit.
    fn project() -> (Repo, String) {
        let repo = Repo::new();
        repo.write(
            "openspec/changes/a/proposal.md",
            "# Add A\n\n## Why\nBecause.\n\n## What Changes\n- a\n",
        );
        repo.write("openspec/changes/a/tasks.md", "- [x] one\n- [ ] two\n");
        repo.write(
            "openspec/changes/a/specs/cap/spec.md",
            "## ADDED Requirements\n### Requirement: A\ntext\n### Requirement: B\ntext\n",
        );
        repo.write("openspec/changes/archive/2026-01-02-b/proposal.md", "# B\n");
        let head = repo.commit("base");
        (repo, head)
    }

    /// The intent of the change `a` of [`project`].
    fn change_a() -> Intent {
        Intent {
            source: "openspec".to_string(),
            reference: "a".to_string(),
            title: Some("Add A".to_string()),
            why: Some("Because.".to_string()),
            tasks: Some(Tasks { done: 1, total: 2 }),
            requirements: Some(Requirements {
                added: Some(2),
                modified: Some(0),
                removed: Some(0),
                renamed: Some(0),
                total: Some(2),
            }),
            validation: None,
            artifacts: vec![
                "openspec/changes/a/proposal.md".to_string(),
                "openspec/changes/a/specs/cap/spec.md".to_string(),
                "openspec/changes/a/tasks.md".to_string(),
            ],
            unresolved: None,
        }
    }

    /// The intent of an entry that does not resolve.
    fn unresolved(reference: &str, line: &str) -> Intent {
        Intent {
            source: "openspec".to_string(),
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

    /// The named intent `openspec:<id>` at `commit`.
    fn named_at(repo: &Repo, commit: &str, id: &str) -> Result<Intent, String> {
        named(
            repo.root(),
            commit,
            &IntentRef::parse(&format!("openspec:{id}")).expect("an intent text"),
        )
    }

    #[test]
    fn a_named_active_change_fills_every_field() {
        let (repo, head) = project();

        assert_eq!(named_at(&repo, &head, "a"), Ok(change_a()));
    }

    #[test]
    fn a_named_archived_change_is_named_by_its_dated_folder_or_by_its_id() {
        let (repo, head) = project();
        let archived = Intent {
            reference: "2026-01-02-b".to_string(),
            title: Some("B".to_string()),
            why: None,
            tasks: None,
            requirements: None,
            artifacts: vec!["openspec/changes/archive/2026-01-02-b/proposal.md".to_string()],
            ..change_a()
        };

        assert_eq!(named_at(&repo, &head, "2026-01-02-b"), Ok(archived.clone()));
        assert_eq!(named_at(&repo, &head, "b"), Ok(archived));
    }

    /// The lines of spec 3.5, each for its own state of the tree.
    #[test]
    fn a_named_change_that_does_not_resolve_has_the_line_of_its_state() {
        let (repo, head) = project();
        let empty = Repo::new();
        empty.write("docs/x.md", "text\n");
        let empty_head = empty.commit("base");
        let store = Repo::new();
        store.write("openspec/config.yaml", "store: team\n");
        let store_head = store.commit("base");
        let config_only = Repo::new();
        config_only.write("openspec/config.yaml", "schema: spec-driven\n");
        let config_only_head = config_only.commit("base");

        for (name, result, reference, line) in [
            (
                "no folder",
                named_at(&empty, &empty_head, "a"),
                "a",
                NO_FOLDER,
            ),
            (
                "a store",
                named_at(&store, &store_head, "a"),
                "a",
                STORE_POINTER,
            ),
            ("not kebab", named_at(&repo, &head, "A_b"), "A_b", NOT_KEBAB),
            (
                "no such change",
                named_at(&repo, &head, "zz"),
                "zz",
                NO_SUCH_CHANGE,
            ),
            (
                "archive",
                named_at(&repo, &head, "archive"),
                "archive",
                NO_SUCH_CHANGE,
            ),
            (
                "a root with a config and no change",
                named_at(&config_only, &config_only_head, "a"),
                "a",
                NO_SUCH_CHANGE,
            ),
        ] {
            assert_eq!(result, Ok(unresolved(reference, line)), "{name}");
        }
    }

    /// The checks come in the order of the lines: the folder, the store, the id,
    /// and the change.
    #[test]
    fn the_checks_of_the_lines_come_in_the_order_of_the_lines() {
        let empty = Repo::new();
        empty.write("docs/x.md", "text\n");
        let empty_head = empty.commit("base");
        let store = Repo::new();
        store.write("openspec/config.yaml", "store: team\n");
        let store_head = store.commit("base");

        assert_eq!(
            named_at(&empty, &empty_head, "Not Kebab"),
            Ok(unresolved("Not Kebab", NO_FOLDER))
        );
        assert_eq!(
            named_at(&store, &store_head, "Not Kebab"),
            Ok(unresolved("Not Kebab", STORE_POINTER))
        );
    }

    /// An id with no active folder resolves to the archived folder `<date>-<id>`
    /// of the latest date, and never to a change whose name only starts with it.
    #[test]
    fn an_id_resolves_to_the_active_folder_else_the_latest_archived_one() {
        let repo = Repo::new();
        repo.write(
            "openspec/changes/archive/2026-01-02-foo/proposal.md",
            "# Old\n",
        );
        repo.write(
            "openspec/changes/archive/2026-03-04-foo/proposal.md",
            "# New\n",
        );
        repo.write(
            "openspec/changes/archive/2026-05-06-foobar/proposal.md",
            "# Foobar\n",
        );
        repo.write(
            "openspec/changes/archive/2026-07-08-oo/proposal.md",
            "# Oo\n",
        );
        let head = repo.commit("base");

        let archived = named_at(&repo, &head, "foo").expect("resolves");
        assert_eq!(archived.reference, "2026-03-04-foo");
        assert_eq!(archived.title.as_deref(), Some("New"));

        repo.write("openspec/changes/foo/proposal.md", "# Active\n");
        let head = repo.commit("reopen");
        let active = named_at(&repo, &head, "foo").expect("resolves");
        assert_eq!(active.reference, "foo");
        assert_eq!(active.title.as_deref(), Some("Active"));
    }

    /// A glob of a schema that does not compile selects no file (spec 3.3): the
    /// change counts its `tasks.md`, and the call gives no line.
    #[test]
    fn a_schema_glob_that_does_not_compile_counts_the_tasks_file() {
        let repo = Repo::new();
        repo.write(
            "openspec/schemas/bad/schema.yaml",
            "name: bad\nversion: 1\nartifacts:\n  - id: tasks\n    generates: \"a[z-a]b\"\n    description: t\n    template: t.md\n",
        );
        repo.write("openspec/changes/x/.openspec.yaml", "schema: bad\n");
        repo.write("openspec/changes/x/proposal.md", "# X\n");
        repo.write("openspec/changes/x/tasks.md", "- [x] one\n- [ ] two\n");
        let head = repo.commit("base");

        let intent = named_at(&repo, &head, "x").expect("a change that resolves");

        assert_eq!(intent.tasks, Some(Tasks { done: 1, total: 2 }));
    }

    /// A `changes/` folder that holds store metadata is a store root and no
    /// planning folder (spec 3.1): it holds no change, active or archived, for a
    /// named id and for a changed path.
    #[test]
    fn a_changes_folder_with_store_metadata_holds_no_change() {
        let repo = Repo::new();
        repo.write(
            "openspec/changes/.openspec-store/store.yaml",
            "id: changes\n",
        );
        repo.write("openspec/changes/x/proposal.md", "# X\n");
        repo.write("openspec/changes/archive/2026-01-02-b/proposal.md", "# B\n");
        let head = repo.commit("base");

        for id in ["x", "b", "2026-01-02-b"] {
            assert_eq!(
                named_at(&repo, &head, id),
                Ok(unresolved(id, NO_SUCH_CHANGE)),
                "{id}"
            );
        }
        assert_eq!(
            found_at(
                &repo,
                &head,
                &[
                    "openspec/changes/x/proposal.md",
                    "openspec/changes/archive/2026-01-02-b/proposal.md"
                ]
            ),
            Ok(Vec::new())
        );
    }

    // ---- the changes that changed paths name ---------------------------------------------

    /// The intents that the changed paths `changed` find at `commit`.
    fn found_at(repo: &Repo, commit: &str, changed: &[&str]) -> Result<Vec<Intent>, String> {
        found(repo.root(), commit, &paths(changed))
    }

    #[test]
    fn a_range_with_no_openspec_path_finds_nothing_and_reads_nothing() {
        let (repo, _) = project();

        assert_eq!(
            found_at(
                &repo,
                &"0".repeat(40),
                &["docs/x.md", "openspec/config.yaml"]
            ),
            Ok(Vec::new())
        );
        assert_eq!(found_at(&repo, &"0".repeat(40), &[]), Ok(Vec::new()));
    }

    #[test]
    fn a_changed_path_below_a_change_folder_finds_the_change_once() {
        let (repo, head) = project();

        assert_eq!(
            found_at(
                &repo,
                &head,
                &[
                    "openspec/changes/a/proposal.md",
                    "openspec/changes/a/tasks.md",
                    "openspec/changes/a/specs/cap/spec.md",
                ]
            ),
            Ok(vec![change_a()])
        );
    }

    /// An archived folder finds the archived change under its full folder name.
    #[test]
    fn a_changed_path_below_an_archived_folder_finds_the_archived_change() {
        let (repo, head) = project();

        let intents = found_at(
            &repo,
            &head,
            &["openspec/changes/archive/2026-01-02-b/proposal.md"],
        )
        .expect("found");

        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].reference, "2026-01-02-b");
    }

    /// A path that is no file of a change names no change: a file of `changes/`,
    /// of `archive/`, a hidden folder, a path of another folder, and a path
    /// outside the root.
    #[test]
    fn a_path_that_is_no_file_of_a_change_finds_nothing() {
        let (repo, _) = project();
        repo.write("openspec/changes/.hidden/proposal.md", "# Hidden\n");
        repo.write("openspec/changes/archive/.hidden/proposal.md", "# Hidden\n");
        let head = repo.commit("hidden");

        for path in [
            "openspec/changes/README.md",
            "openspec/changes/a",
            "openspec/changes/archive/note.md",
            "openspec/changes/archive/2026-01-02-b",
            "openspec/changes/.hidden/proposal.md",
            "openspec/changes/archive/.hidden/proposal.md",
            "openspec/specs/auth/spec.md",
            "docs/openspec/changes/a/proposal.md",
            "openspec-other/changes/a/proposal.md",
        ] {
            assert_eq!(found_at(&repo, &head, &[path]), Ok(Vec::new()), "{path}");
        }
    }

    /// An active change with no folder at the head resolves to its archived
    /// folder of the latest date; with no archived folder either, it gives no
    /// intent; an archived path whose folder is gone gives none.
    #[test]
    fn a_change_that_the_head_no_longer_holds_follows_its_archive_or_is_gone() {
        let (repo, head) = project();

        let followed = found_at(&repo, &head, &["openspec/changes/b/proposal.md"]).expect("found");
        assert_eq!(followed.len(), 1);
        assert_eq!(followed[0].reference, "2026-01-02-b");
        assert_eq!(
            found_at(&repo, &head, &["openspec/changes/gone/proposal.md"]),
            Ok(Vec::new())
        );
        assert_eq!(
            found_at(
                &repo,
                &head,
                &["openspec/changes/archive/2026-02-03-gone/proposal.md"]
            ),
            Ok(Vec::new())
        );
    }

    /// The old folder and the new folder of an archived change are one intent.
    #[test]
    fn the_old_and_the_new_folder_of_an_archived_change_are_one_intent() {
        let (repo, head) = project();

        let intents = found_at(
            &repo,
            &head,
            &[
                "openspec/changes/b/proposal.md",
                "openspec/changes/archive/2026-01-02-b/proposal.md",
            ],
        )
        .expect("found");

        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].reference, "2026-01-02-b");
    }

    /// The intents come in the order of their refs.
    #[test]
    fn the_found_intents_come_in_the_order_of_their_refs() {
        let (repo, _) = project();
        repo.write("openspec/changes/c/proposal.md", "# C\n");
        let head = repo.commit("add c");

        let intents = found_at(
            &repo,
            &head,
            &[
                "openspec/changes/c/proposal.md",
                "openspec/changes/archive/2026-01-02-b/proposal.md",
                "openspec/changes/a/proposal.md",
            ],
        )
        .expect("found");

        let refs: Vec<&str> = intents
            .iter()
            .map(|found| found.reference.as_str())
            .collect();
        assert_eq!(refs, ["2026-01-02-b", "a", "c"]);
    }
}
