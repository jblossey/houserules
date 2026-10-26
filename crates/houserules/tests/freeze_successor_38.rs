//! The surface freeze of 1.x retires for its successor (HR-238; batch-38 spec
//! section 7 and acceptance case 7).
//!
//! `houserules.1-0-surface-is-frozen` is retired and sits in the knowledge
//! archive, where `get` still reads it. `houserules.breaking-changes-need-an-owner-ruling`
//! is the standing successor, and the standing list names it in place of the
//! retired entry. No tracked file outside the history classes points to the
//! retired entry, and the two places that stated the freeze to a reader (the
//! Stability section of the README and the surface-impact field of the change
//! request form) state none. A line elsewhere in those documents that uses
//! the words in another meaning, such as `cargo build --frozen`, passes.
//!
//! The scan of the tracked tree derives its scope: every path that
//! `git ls-files -z` names, minus the declared history classes
//! (`quality.gates-derive-their-scope`). A history class that excludes no
//! tracked file fails the test.

mod tracker_support;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use saphyr::{LoadableYamlNode, Yaml};
use serde_json::Value;
use tracker_support::{git, houserules, repo_root};

/// The retired entry.
const FREEZE: &str = "houserules.1-0-surface-is-frozen";

/// The standing successor entry.
const SUCCESSOR: &str = "houserules.breaking-changes-need-an-owner-ruling";

/// The summary of the successor (spec 7.2).
const SUCCESSOR_SUMMARY: &str = "A change that would prompt a major release (a `!` commit or a `BREAKING CHANGE` footer) needs an owner ruling before it merges.";

/// The one body paragraph of the successor (spec 7.2).
const SUCCESSOR_BODY: &str = "It replaces houserules.1-0-surface-is-frozen, which froze the 1.x surface until 2.0.0 (design.md 5.220). After 2.0.0 a command, flag, or schema may change in a minor release when the change is not breaking; a breaking change still needs the owner's ruling (design.md 5.221).";

/// The tracked paths and path prefixes that hold history or a fixture, each
/// with its reason. A path that ends in `/` names a directory.
const HISTORY: &[(&str, &str)] = &[
    (
        "backlog/",
        "the tracker's items and archives record work, and the controller owns them",
    ),
    (
        "CHANGELOG.md",
        "release-please writes it from the commit headers",
    ),
    (
        "crates/houserules/tests/fixtures/conversion/",
        "the 1.x conversion fixture holds a 1.x knowledge base verbatim",
    ),
    (
        "crates/houserules/tests/freeze_successor_38.rs",
        "this test names the retired id",
    ),
    ("docs/design.md", "the decision log"),
    ("docs/history/", "earlier procedures kept as they ran"),
    ("docs/plans/", "plans of earlier batches"),
    ("docs/specs/", "specs of earlier batches"),
    (
        "knowledge/archive/",
        "the archive mirrors keep retired entries and the entries that cite them",
    ),
];

/// The README heading of the section that stated the freeze.
const README_FREEZE_SECTION: &str = "## Stability";

/// The change request form, which stated the freeze in one field.
const CHANGE_FORM: &str = ".github/ISSUE_TEMPLATE/2-change.yml";

/// The id of the field of the change request form that stated the freeze.
const CHANGE_FORM_FIELD: &str = "surface-impact";

/// The stems of the words that state a freeze, in lower case.
const FREEZE_STEMS: [&str; 2] = ["freez", "frozen"];

/// The path of `relative` below the repository root, built component by
/// component.
fn path_of(relative: &str) -> PathBuf {
    relative
        .split('/')
        .fold(repo_root(), |path, part| path.join(part))
}

/// The text of the tracked file `relative`.
fn text_of(relative: &str) -> String {
    fs::read_to_string(path_of(relative)).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The JSON that `houserules <args> --dir <repository root>` prints.
fn printed(args: &[&str]) -> Value {
    let output = houserules()
        .args(args)
        .arg("--dir")
        .arg(repo_root())
        .output()
        .expect("run houserules");
    assert!(
        output.status.success(),
        "houserules {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("parse {args:?}: {error}"))
}

/// The one entry that `houserules get <id>` prints.
fn entry(id: &str) -> Value {
    let Value::Array(mut entries) = printed(&["get", id]) else {
        panic!("get {id} prints a JSON array");
    };
    assert_eq!(entries.len(), 1, "get {id} prints one entry");
    entries.remove(0)
}

/// The ids that `houserules standing` prints.
fn standing_ids() -> Vec<String> {
    let Value::Array(entries) = printed(&["standing"]) else {
        panic!("standing prints a JSON array");
    };
    entries
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id").to_string())
        .collect()
}

/// Every tracked path, repo-relative with `/` separators, sorted.
fn tracked_paths() -> Vec<String> {
    let listing = git(&repo_root(), &["ls-files", "-z"]);
    let mut paths: Vec<String> = listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort();
    paths
}

/// The lines of `text` after the line `heading`, up to the next `## ` heading.
fn section_of(text: &str, heading: &str) -> String {
    let mut lines = text.lines().skip_while(|line| *line != heading);
    assert!(lines.next().is_some(), "the text holds no `{heading}` line");
    let section = lines
        .take_while(|line| !line.starts_with("## "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !section.trim().is_empty(),
        "the `{heading}` section is empty"
    );
    section
}

/// The description of the field `id` of the change request form.
fn change_form_description(id: &str) -> String {
    let documents = Yaml::load_from_str(&text_of(CHANGE_FORM))
        .unwrap_or_else(|error| panic!("parse {CHANGE_FORM}: {error}"));
    let [form] = documents.as_slice() else {
        panic!("{CHANGE_FORM} holds {} YAML documents", documents.len());
    };
    let field = form
        .as_mapping_get("body")
        .and_then(Yaml::as_sequence)
        .and_then(|fields| {
            fields
                .iter()
                .find(|field| field.as_mapping_get("id").and_then(Yaml::as_str) == Some(id))
        })
        .unwrap_or_else(|| panic!("{CHANGE_FORM} holds no field {id}"));
    field
        .as_mapping_get("attributes")
        .and_then(|attributes| attributes.as_mapping_get("description"))
        .and_then(Yaml::as_str)
        .unwrap_or_else(|| panic!("the field {id} of {CHANGE_FORM} has no description"))
        .to_string()
}

/// `true` when `path` is the history entry `class`, or lies below it.
fn in_class(path: &str, class: &str) -> bool {
    if class.ends_with('/') {
        path.starts_with(class)
    } else {
        path == class
    }
}

#[test]
fn the_freeze_entry_is_retired_and_archived() {
    let retired = entry(FREEZE);
    assert_eq!(retired["status"], "retired", "the entry's status");
    assert_eq!(retired["archived"], true, "the entry sits in the archive");
    assert_eq!(retired["id"], FREEZE);

    let live: Value = serde_json::from_str(&text_of("knowledge/houserules.json"))
        .expect("parse knowledge/houserules.json");
    let live_ids: Vec<&str> = live["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id"))
        .collect();
    assert!(
        !live_ids.contains(&FREEZE),
        "the live topic file still holds {FREEZE}"
    );
}

#[test]
fn the_successor_is_a_standing_rule_that_names_the_entry_it_replaces() {
    let successor = entry(SUCCESSOR);
    assert_eq!(successor["kind"], "rule");
    assert_eq!(successor["area"], "global");
    assert_eq!(successor["standing"], true);
    assert_eq!(successor["summary"], SUCCESSOR_SUMMARY);
    assert_eq!(successor["body"], serde_json::json!([SUCCESSOR_BODY]));
    assert_eq!(
        successor["tags"],
        serde_json::json!(["cli", "schema", "breaking-change"])
    );
    assert_eq!(successor["see"], serde_json::json!([FREEZE]));
    assert_eq!(successor["source"]["by"], "user");
    assert_eq!(successor["source"]["ref"], "design.md 5.221");
}

#[test]
fn the_standing_list_swaps_the_freeze_for_its_successor() {
    let ids = standing_ids();
    assert!(ids.iter().any(|id| id == SUCCESSOR), "no {SUCCESSOR}");
    assert!(!ids.iter().any(|id| id == FREEZE), "still lists {FREEZE}");
}

#[test]
fn no_tracked_file_outside_the_history_classes_points_to_the_retired_entry() {
    let paths = tracked_paths();
    assert!(
        paths.iter().any(|path| path == "README.md"),
        "the listing misses README.md"
    );
    for (class, reason) in HISTORY {
        assert!(
            paths.iter().any(|path| in_class(path, class)),
            "the history class {class} ({reason}) excludes no tracked file"
        );
    }

    let mut pointers = BTreeSet::new();
    let mut scanned = 0;
    for path in &paths {
        if HISTORY.iter().any(|(class, _)| in_class(path, class)) {
            continue;
        }
        let bytes = fs::read(path_of(path)).unwrap_or_else(|error| panic!("read {path}: {error}"));
        scanned += 1;
        if String::from_utf8_lossy(&bytes).contains(FREEZE) {
            pointers.insert(path.clone());
        }
    }
    assert!(scanned > 100, "the scan visited {scanned} files");

    // The successor names the entry it replaces, in `see` and in its body.
    assert!(
        pointers.remove("knowledge/houserules.json"),
        "the successor names {FREEZE} in knowledge/houserules.json"
    );
    assert_eq!(
        pointers,
        BTreeSet::new(),
        "live files that point to {FREEZE}"
    );

    let live: Value = serde_json::from_str(&text_of("knowledge/houserules.json"))
        .expect("parse knowledge/houserules.json");
    let naming: Vec<&str> = live["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry.to_string().contains(FREEZE))
        .map(|entry| entry["id"].as_str().expect("an id"))
        .collect();
    assert_eq!(naming, [SUCCESSOR], "the live entries that name {FREEZE}");
}

#[test]
fn the_places_that_stated_the_freeze_state_none() {
    let places = [
        (
            format!("the `{README_FREEZE_SECTION}` section of README.md"),
            section_of(&text_of("README.md"), README_FREEZE_SECTION),
        ),
        (
            format!("the `{CHANGE_FORM_FIELD}` field of {CHANGE_FORM}"),
            change_form_description(CHANGE_FORM_FIELD),
        ),
    ];
    for (place, text) in places {
        let text = text.to_lowercase();
        for stem in FREEZE_STEMS {
            assert!(
                !text.contains(stem),
                "{place} still holds the stem `{stem}`"
            );
        }
    }
}
