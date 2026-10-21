//! The texts that the kit seeds: the entries of its five packs, and the core
//! texts a plain `init` writes (spec sections 5, 6.4, 7, 8, and 9.1; design.md
//! 179, 182, and 183). The packs and the core texts name no file, command, or
//! role that a minimal install lacks; the process pack names the installed
//! tracker; no kit text names the full tier, a plugin and its skills, or a
//! vendor model; the entries the owner ruled on carry their words; and the
//! `security-hygiene.dependency-vetting` check reaches the files of a mise
//! tool.
//!
//! The rewrite rule is the script `f4-rewrite-set-v4.py` of the batch
//! workspace, ported to the `regress` engine that the checks use. Its classes
//! are the script's. Its readings of a hit are below, each with its reason:
//!
//! - A check's `files`, `if`, and `then` globs name the files a rule governs.
//!   A check over files that an install lacks passes, so a glob is no pointer
//!   to a file the install needs. The scan skips those three keys.
//! - The word `controller` has two readings that are knowledge vocabulary, not
//!   a role. One is the tag of `houserules audit`, which leaves an entry that
//!   carries it out of a task audit. The other is the value `controller` of
//!   `source.by` in the knowledge schema. A text spells either reading in
//!   backticks, or as a JSON string; the scan skips those two spellings and no
//!   other.
//! - The file-name prefix `eval-` is a literal value that an entry asks the
//!   reader to use, so it is no workflow file. The entries spell it in
//!   backticks; the scan of the pack entries skips that spelling and no other.
//! - An entry id is a name that cannot change (`knowledge-base.ids-are-permanent`),
//!   so a citation of an id is no pointer to a file or role. The scan replaces
//!   every kit id with a neutral token.
//! - `knowledge-base.state-only-the-source` names an amended entry, not an
//!   amendment record (the script's own exclusion).
//! - `houserules archive` has a knowledge half on every install (spec 3.4), so
//!   it is no backlog command.
//! - A core text may name the agent templates when it says how an install gets
//!   them: `houserules init --workflow reference` stands on the line or on the
//!   next one (the window of `kit_texts.rs`).
//! - The script's kept class `report-format` names no missing thing, so the scan
//!   leaves it out.
//! - The scan of the core texts reads every file of `core_template_files()`
//!   outside `packs/`. `CORE_READINGS` lists the files, lines, and sections
//!   that name a missing thing because that is their subject, each with its
//!   reason.
//!
//! Every exemption that the code applies fails its scan when it excuses no hit,
//! so that none outlives the text it excuses: a row of `CORE_READINGS` or
//! `EXCLUDED_ENTRY_HITS`, a name of `ENTRY_EXEMPTIONS` (the scan of the pack
//! entries), and a name of `CORE_EXEMPTIONS` (the scan of the core texts). The
//! scan of the lifecycle skills has no exemption.

mod guard_lists;
mod tracker_support;

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use globset::GlobBuilder;
use guard_lists::{FULL_TIER, PLUGIN_SKILLS, VENDOR_MODELS};
use regress::Regex;
use serde_json::{Value, json};
use tracker_support::{commit_all, repo_root, run, run_ok, scratch_git_repo, stdout};

/// A class of the rewrite rule: its name, its pattern, and its regex flags.
struct Class {
    name: &'static str,
    pattern: &'static str,
    flags: &'static str,
}

/// The classes of `f4-rewrite-set-v4.py`. The script gives its dispatch class
/// an inline `(?i:...)` group and two exact tokens; here the group and the
/// tokens are two entries of one class.
const CLASSES: [Class; 8] = [
    Class {
        name: "json-backlog",
        pattern: r"backlog/|batches\.json|amendments\.json|parked\.json|decisions\.json|items/[\w*-]*\.json|\bPP-|\bA-\d|backlog ids?\b",
        flags: "i",
    },
    Class {
        name: "workflow-files",
        pattern: r"\.claude/agents|\bimplementer\b|task-reviewer|branch-reviewer|\borchestrating\b|finishing-a-feature|\.claude/evals|\bevals?\b|\.githooks|commit-msg|session-start|\bsubagents?\b",
        flags: "i",
    },
    Class {
        name: "dispatch-protocol",
        pattern: r"\bdispatch\w*|REPORT_FILE|REVIEW_FILE|AUDIT_JSON|FIX_BASE|\bcontroller\b|\bthe orchestrator\b|\bbatch[- ]workspace|\bthe workspace\b|\bworkspace (doc|file)s?\b|\bledger\b|\bbatch (spec|report)s?\b",
        flags: "i",
    },
    Class {
        name: "dispatch-protocol",
        pattern: r"`Knowledge:`|`BASE:`",
        flags: "",
    },
    Class {
        name: "tracker-subject",
        pattern: r"\bbacklog\b(?!/)",
        flags: "i",
    },
    Class {
        name: "deferral",
        pattern: r"\bdefer(?:s|red|ring|ral|rals)?\b",
        flags: "i",
    },
    Class {
        name: "backlog-commands",
        pattern: r"houserules (list|batch|set|check-backlog)\b",
        flags: "i",
    },
    Class {
        name: "batch-record",
        pattern: r"\bbatch(es)?\b|\bamendments?\b|\bparked\b|\bmilestones?\b",
        flags: "i",
    },
];

/// The classes that no install without a workflow or a tracker may meet in
/// any text.
const REWRITE: [&str; 4] = [
    "json-backlog",
    "workflow-files",
    "dispatch-protocol",
    "tracker-subject",
];

/// The classes that only the process pack keeps: it needs a tracker.
const PROCESS_ONLY: [&str; 3] = ["deferral", "backlog-commands", "batch-record"];

/// The entry hits the script excludes by id: an amended entry is no
/// amendment record.
const EXCLUDED_ENTRY_HITS: [(&str, &str); 1] =
    [("knowledge-base.state-only-the-source", "batch-record")];

/// The 36 entries of the rewrite set `rewrite-ids-v4.txt` lists.
const REWRITE_SET: [&str; 36] = [
    "knowledge-base.ids-are-permanent",
    "knowledge-base.rules-need-a-loading-path",
    "knowledge-base.controller-rules-are-tagged",
    "knowledge-base.cite-durable-refs",
    "knowledge-base.context-proxy-output-is-lossy",
    "process.backlog-drives-work",
    "process.brief-carries-the-spec",
    "process.brainstorm-first",
    "process.rulings-to-file",
    "process.tdd",
    "process.model-policy",
    "process.no-tech-debt",
    "process.code-health-scan",
    "process.deliverables-json",
    "process.evals-rerun",
    "process.contract-refresh-sweep",
    "process.eval-fixture-procedure",
    "process.main-wins-backlog-collisions",
    "process.owner-content-mid-batch",
    "process.evidence-outlives-the-session",
    "process.claims-match-artifacts",
    "process.review-findings-are-claims-too",
    "process.suggestions-are-unverified",
    "process.fix-round-verification-record",
    "process.fix-proofs-extend-the-reviewers-run",
    "process.owner-rulings-need-owner-supersession",
    "process.report-evidence-verbatim",
    "process.risk-tiers",
    "process.severity-by-impact",
    "process.bounded-review-loop",
    "process.knowledge-earns-its-place",
    "process.parallel-on-disjoint-files",
    "quality.principles",
    "quality.no-compat-softening",
    "quality.well-maintained-libraries",
    "writing-style.instructions-cover-the-state-space",
];

/// The plugin skill name that a pack line may use as the plain word for the
/// activity: the summary of `process.brainstorm-first` and a body line of
/// `process.code-health-scan` say "brainstorming". The workflow guard of
/// `workflow_texts.rs` has no such line and no such exclusion.
const PACK_PLAIN_WORD: &str = "brainstorming";

/// The role phrase that the kit texts use for the agent template of each file
/// stem below `.claude/agents/`.
const TEMPLATE_ROLES: [(&str, &str); 4] = [
    ("implementer", "author of the change"),
    ("task-reviewer", "task review"),
    ("branch-reviewer", "branch review"),
    ("spec-reviewer", "spec review"),
];

/// The globs of `security-hygiene.dependency-vetting` that reach every
/// project config file that mise reads, at any depth (HR-207, HR-208).
///
/// The source is the config discovery of mise (https://github.com/jdx/mise):
/// `src/config/mod.rs` (the list of shared files, `LOCAL_OVERRIDE_CONFIG_FILENAMES`,
/// and `env_config_patterns`) and the documentation pages "Config file
/// locations" (`docs/configuration.md`) and "File names and precedence"
/// (`docs/configuration/environments.md`). Mise reads each location as a
/// shared, a local, an environment, and an environment-local file. The test
/// `the_dependency_vetting_check_reaches_the_files_of_a_mise_tool` derives the
/// file names with `mise_file_names` from `MISE_SHARED_PATTERNS`,
/// `MISE_LOCAL_PATTERNS`, and `MISE_ENV_DIRS`, and fails for a name that no
/// glob of the check matches.
const MISE_GLOBS: [&str; 14] = [
    "**/mise.toml",
    "**/.mise.toml",
    "**/.mise.*.toml",
    "**/mise.*.toml",
    "**/.tool-versions",
    "**/mise/config.toml",
    "**/mise/config.*.toml",
    "**/mise/conf.d/**/*.toml",
    "**/.mise/config.toml",
    "**/.mise/config.*.toml",
    "**/.mise/conf.d/**/*.toml",
    "**/.config/mise.toml",
    "**/.config/mise/*.toml",
    "**/.config/mise/conf.d/**/*.toml",
];

/// The shared file patterns that mise reads in one project directory: the
/// list of `src/config/mod.rs` that precedes `LOCAL_OVERRIDE_CONFIG_FILENAMES`.
const MISE_SHARED_PATTERNS: [&str; 13] = [
    ".config/mise/conf.d/*.toml",
    ".config/mise/conf.d/*/mise.toml",
    ".config/mise/config.toml",
    ".config/mise/mise.toml",
    ".config/mise.toml",
    ".mise/conf.d/*.toml",
    ".mise/conf.d/*/mise.toml",
    ".mise/config.toml",
    "mise/conf.d/*.toml",
    "mise/conf.d/*/mise.toml",
    "mise/config.toml",
    "mise.toml",
    ".mise.toml",
];

/// The local file patterns that mise reads in one project directory:
/// `LOCAL_OVERRIDE_CONFIG_FILENAMES` of `src/config/mod.rs`.
const MISE_LOCAL_PATTERNS: [&str; 10] = [
    ".config/mise/conf.d/*/mise.local.toml",
    ".config/mise/config.local.toml",
    ".config/mise/mise.local.toml",
    ".config/mise.local.toml",
    ".mise/conf.d/*/mise.local.toml",
    ".mise/config.local.toml",
    "mise/conf.d/*/mise.local.toml",
    "mise/config.local.toml",
    "mise.local.toml",
    ".mise.local.toml",
];

/// The directories that `env_config_patterns` of `src/config/mod.rs` crosses
/// with an environment name: each gives `<dir>/conf.d/*.<env>.toml`,
/// `<dir>/conf.d/*/mise.<env>.toml`, `<dir>/config.<env>.toml`, and
/// `<dir>.<env>.toml`. The environment local files use `<env>.local` as the
/// name.
const MISE_ENV_DIRS: [&str; 3] = [".config/mise", "mise", ".mise"];

/// A file name for every pattern of mise's config discovery: the shared and
/// the local patterns, the environment patterns for the environment `ci`, the
/// environment-local patterns for `ci.local`, and `.tool-versions`. A `*`
/// becomes `x`.
fn mise_file_names() -> Vec<String> {
    let mut patterns: Vec<String> = MISE_SHARED_PATTERNS
        .iter()
        .chain(MISE_LOCAL_PATTERNS.iter())
        .map(|pattern| pattern.to_string())
        .collect();
    for env in ["ci", "ci.local"] {
        for dir in MISE_ENV_DIRS {
            patterns.extend([
                format!("{dir}/conf.d/*.{env}.toml"),
                format!("{dir}/conf.d/*/mise.{env}.toml"),
                format!("{dir}/config.{env}.toml"),
                format!("{dir}.{env}.toml"),
            ]);
        }
    }
    patterns.push(".tool-versions".to_string());
    patterns
        .into_iter()
        .map(|pattern| pattern.replace('*', "x"))
        .collect()
}

/// The design-phase entries that only a coordinating act can break (design.md
/// 182): they carry the tag `controller`.
const CONTROLLER_TAGGED: [&str; 2] = [
    "process.specs-list-borrowed-concepts",
    "process.reversals-are-recorded",
];

/// The four entries of the design phase (design.md 182) with the summary of
/// each.
const DESIGN_PHASE: [(&str, &str); 4] = [
    (
        "process.design-decisions-carry-probes",
        "Rejecting a library or departing from the source research or an approved spec needs a captured probe, the rejected option's best case, and a reversal condition.",
    ),
    (
        "process.design-claims-are-labelled",
        "Label each claim that supports a design decision run, read, or assumed; an assumed claim is never the sole reason to reject an option.",
    ),
    (
        "process.specs-list-borrowed-concepts",
        "A spec lists every concept it takes from its sources, with the source's status and a check of the concept against the goal.",
    ),
    (
        "process.reversals-are-recorded",
        "Record every decision that the owner or a reviewer overturns as a `Reversal:` line in the record of the work; count the lines at the retrospective.",
    ),
];

/// The compiled `class`.
fn compile(class: &Class) -> Regex {
    Regex::with_flags(class.pattern, class.flags)
        .unwrap_or_else(|error| panic!("compile {}: {error}", class.name))
}

/// The compiled `CLASSES`, in the order of the table.
static COMPILED: LazyLock<Vec<Regex>> = LazyLock::new(|| CLASSES.iter().map(compile).collect());

/// Every `(class, match)` pair that `text` holds for the classes in `names`.
fn hits(text: &str, names: &[&str]) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    for (class, regex) in CLASSES.iter().zip(COMPILED.iter()) {
        if !names.contains(&class.name) {
            continue;
        }
        for matched in regex.find_iter(text) {
            found.push((class.name, matched.as_str(text).to_string()));
        }
    }
    found
}

/// The text of `relative` below the repository root.
fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The kit packs, as `(name, content)` pairs in name order.
fn kit_packs() -> Vec<(String, Value)> {
    let dir = repo_root().join("template/packs");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("a directory entry").file_name())
        .map(|name| name.into_string().expect("a UTF-8 name"))
        .filter(|name| name.ends_with(".json"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let content = serde_json::from_str(&read(&format!("template/packs/{name}")))
                .unwrap_or_else(|error| panic!("parse {name}: {error}"));
            (name.trim_end_matches(".json").to_string(), content)
        })
        .collect()
}

/// The entries of `pack`.
fn entries(pack: &Value) -> &[Value] {
    pack["entries"].as_array().expect("a pack lists entries")
}

/// The entry `id` of the kit packs.
fn entry(id: &str) -> Value {
    kit_packs()
        .iter()
        .flat_map(|(_, pack)| entries(pack).iter())
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("no kit entry {id}"))
        .clone()
}

/// The summary and the body of `entry` as one text.
fn prose(entry: &Value) -> String {
    let body: Vec<&str> = entry["body"]
        .as_array()
        .map(|paragraphs| paragraphs.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    format!(
        "{}\n{}",
        entry["summary"].as_str().unwrap_or(""),
        body.join("\n")
    )
}

/// The ids of every entry of the kit packs, longest first.
fn kit_ids() -> Vec<String> {
    let mut ids: Vec<String> = kit_packs()
        .iter()
        .flat_map(|(_, pack)| entries(pack).iter())
        .map(|entry| entry["id"].as_str().expect("an entry id").to_string())
        .collect();
    ids.sort_by_key(|id| std::cmp::Reverse(id.len()));
    ids
}

/// `text` with every kit id replaced by a neutral token. Notes `kit ids` when
/// that drops a hit of the classes in `names`.
fn without_ids(text: &str, ids: &[String], names: &[&str], excused: &Excused) -> String {
    let replaced = ids
        .iter()
        .fold(text.to_string(), |text, id| text.replace(id.as_str(), "ID"));
    if replaced != text && hits(&replaced, names).len() < hits(text, names).len() {
        excused.note("kit ids");
    }
    replaced
}

/// The exemptions of the rewrite rule that excused a hit, by name. A scan
/// notes an exemption when it drops a hit, so that the scan can fail for an
/// exemption that excuses nothing.
#[derive(Default)]
struct Excused(RefCell<Vec<&'static str>>);

impl Excused {
    /// Notes that the exemption `name` excused a hit.
    fn note(&self, name: &'static str) {
        self.0.borrow_mut().push(name);
    }

    /// The names of `exemptions` that excused no hit.
    fn idle(&self, exemptions: &[&'static str]) -> Vec<&'static str> {
        let noted = self.0.borrow();
        exemptions
            .iter()
            .copied()
            .filter(|name| !noted.contains(name))
            .collect()
    }
}

/// `text` without `spelling`. Notes `name` when that drops a hit of the
/// classes in `names`.
fn without_spelling(
    text: &str,
    spelling: &str,
    name: &'static str,
    names: &[&str],
    excused: &Excused,
) -> String {
    let stripped = text.replace(spelling, "");
    if stripped != text && hits(&stripped, names).len() < hits(text, names).len() {
        excused.note(name);
    }
    stripped
}

/// The exemptions of the scan of the pack entries that `scanned` applies.
const ENTRY_EXEMPTIONS: [&str; 6] = [
    "check.files",
    "check.if",
    "check.then",
    "kit ids",
    "`controller`",
    "`eval-`",
];

/// The text of `entry` that the rewrite rule scans: its summary, body, `verify`
/// paths, and check, without the globs of the check, without the tag
/// `controller` in backticks, and without the file-name prefix `eval-` in
/// backticks. `names` are the classes that the scan of the entry meets; each
/// exemption that drops a hit of one of them is noted in `excused`.
fn scanned(entry: &Value, ids: &[String], names: &[&str], excused: &Excused) -> String {
    let mut check = entry.get("check").cloned().unwrap_or(Value::Null);
    if let Some(map) = check.as_object_mut() {
        for (key, name) in [
            ("files", "check.files"),
            ("if", "check.if"),
            ("then", "check.then"),
        ] {
            if let Some(globs) = map.remove(key)
                && !hits(&without_ids(&globs.to_string(), ids, names, excused), names).is_empty()
            {
                excused.note(name);
            }
        }
    }
    let text = without_ids(
        &json!({
            "summary": entry.get("summary"),
            "body": entry.get("body"),
            "verify": entry.get("verify"),
            "check": check,
        })
        .to_string(),
        ids,
        names,
        excused,
    );
    let text = without_spelling(&text, "`controller`", "`controller`", names, excused);
    without_spelling(&text, "`eval-`", "`eval-`", names, excused)
}

/// The files of `template/` below `dir`, as paths relative to `template/`.
fn template_files(dir: &Path, prefix: &str, found: &mut Vec<String>) {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
        .map(|entry| entry.expect("a directory entry"))
        .collect();
    names.sort_by_key(|entry| entry.file_name());
    for entry in names {
        let name = entry.file_name().into_string().expect("a UTF-8 name");
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if entry.file_type().expect("a file type").is_dir() {
            template_files(&dir.join(&name), &relative, found);
        } else {
            found.push(relative);
        }
    }
}

/// The text files of `template/` that a plain `init` can write: everything
/// outside the reference workflow, which names its own files and roles.
fn core_template_files() -> Vec<String> {
    let mut files = Vec::new();
    template_files(&repo_root().join("template"), "", &mut files);
    files.retain(|file| !file.starts_with("workflows/"));
    files
}

/// The core texts as paths below `template/`: the core files without the
/// kit packs, which the entry scan reads.
fn core_texts() -> Vec<String> {
    let mut files = core_template_files();
    files.retain(|file| !file.starts_with("packs/"));
    files
}

/// The kit packs and the core texts as paths below `template/`: every file
/// that a plain `init` can write and whose lines a text rule covers.
fn pack_and_core_files() -> Vec<String> {
    let mut files: Vec<String> = kit_packs()
        .iter()
        .map(|(name, _)| format!("packs/{name}.json"))
        .collect();
    files.extend(core_texts());
    files
}

/// `template/<file>:<line>: <text>` for every line of `files` (paths below
/// `template/`) that `holds` accepts. An unreadable file panics: a text rule
/// that skipped it would pass without reading it.
fn lines_where(files: &[String], holds: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut found = Vec::new();
    for file in files {
        let text = read(&format!("template/{file}"));
        for (number, line) in text.lines().enumerate() {
            if holds(line) {
                found.push(format!("template/{file}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    found
}

/// The pack entries name no file or role that a minimal install lacks. The
/// scan fails for an exemption that excuses no hit: a name of
/// `ENTRY_EXEMPTIONS` that `scanned` never noted, and a row of
/// `EXCLUDED_ENTRY_HITS` that excused no entry.
#[test]
fn no_pack_entry_names_a_file_or_role_that_a_minimal_install_lacks() {
    let ids = kit_ids();
    let excused = Excused::default();
    let used_rows: Vec<Cell<bool>> = EXCLUDED_ENTRY_HITS
        .iter()
        .map(|_| Cell::new(false))
        .collect();
    let mut findings = Vec::new();
    let mut scanned_entries = 0;
    for (pack, content) in kit_packs() {
        let names: Vec<&str> = if pack == "process" {
            REWRITE.to_vec()
        } else {
            REWRITE.iter().chain(PROCESS_ONLY.iter()).copied().collect()
        };
        for entry in entries(&content) {
            scanned_entries += 1;
            let id = entry["id"].as_str().expect("an entry id");
            for (class, matched) in hits(&scanned(entry, &ids, &names, &excused), &names) {
                if let Some(row) = EXCLUDED_ENTRY_HITS
                    .iter()
                    .position(|row| *row == (id, class))
                {
                    used_rows[row].set(true);
                    continue;
                }
                findings.push(format!("{id}: {class}: {matched}"));
            }
        }
    }
    for name in excused.idle(&ENTRY_EXEMPTIONS) {
        findings.push(format!("the exemption {name} excuses no hit: delete it"));
    }
    for (row, used) in EXCLUDED_ENTRY_HITS.iter().zip(&used_rows) {
        if !used.get() {
            findings.push(format!(
                "the exclusion {row:?} excuses no hit: delete the row"
            ));
        }
    }
    assert!(
        findings.is_empty(),
        "{} findings in the pack entries:\n{}",
        findings.len(),
        findings.join("\n")
    );
    assert_eq!(scanned_entries, 65, "the kit carries 65 entries");
}

#[test]
fn the_rewrite_set_is_still_in_the_packs() {
    for id in REWRITE_SET {
        entry(id);
    }
}

#[test]
fn no_pack_line_and_no_core_text_names_the_full_tier() {
    let full_tier = Regex::with_flags(FULL_TIER, "i").expect("the full-tier pattern compiles");
    let findings = lines_where(&pack_and_core_files(), &|line| {
        full_tier.find(line).is_some()
    });
    assert!(
        findings.is_empty(),
        "the full tier is named:\n{}",
        findings.join("\n")
    );
}

#[test]
fn no_pack_line_and_no_core_text_names_a_plugin_skill() {
    let files = pack_and_core_files();
    let findings = lines_where(&files, &|line| {
        let lower = line.to_lowercase();
        lower.contains("superpowers")
            || PLUGIN_SKILLS
                .iter()
                .filter(|skill| **skill != PACK_PLAIN_WORD)
                .any(|skill| lower.contains(skill))
    });
    assert!(
        findings.is_empty(),
        "a plugin or one of its skills is named:\n{}",
        findings.join("\n")
    );
    let excused = lines_where(&files, &|line| {
        line.to_lowercase().contains(PACK_PLAIN_WORD)
    });
    assert!(
        !excused.is_empty(),
        "the exclusion of `{PACK_PLAIN_WORD}` excuses no line"
    );
}

#[test]
fn no_pack_line_and_no_core_text_names_a_vendor_model() {
    let models = Regex::with_flags(VENDOR_MODELS, "i").expect("the model pattern compiles");
    let findings = lines_where(&pack_and_core_files(), &|line| models.find(line).is_some());
    assert!(
        findings.is_empty(),
        "a vendor model is named alone:\n{}",
        findings.join("\n")
    );
}

#[test]
fn the_four_design_phase_entries_exist_as_non_standing_process_rules() {
    let packs = kit_packs();
    let process = &packs
        .iter()
        .find(|(name, _)| name == "process")
        .expect("the process pack")
        .1;
    for (id, summary) in DESIGN_PHASE {
        let found = entries(process)
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap_or_else(|| panic!("the process pack lacks {id}"));
        assert_eq!(found["kind"], "rule", "{id}");
        assert_eq!(found["area"], "process", "{id}");
        assert!(
            found
                .get("standing")
                .is_none_or(|standing| standing == &json!(false)),
            "{id} is standing"
        );
        assert_eq!(found["summary"], summary, "{id}");
        let tagged = found["tags"]
            .as_array()
            .expect("tags")
            .contains(&json!("controller"));
        assert_eq!(
            tagged,
            CONTROLLER_TAGGED.contains(&id),
            "{id}: the tag controller belongs to the entries that only a coordinating act breaks"
        );
        assert!(
            !found["body"].as_array().expect("a body").is_empty(),
            "{id} has no body"
        );
        assert!(
            !process["optIn"]
                .as_array()
                .expect("optIn")
                .contains(&json!(id)),
            "{id} is opt-in"
        );
    }
}

/// `process.evals-rerun` states the trigger that its check enforces: the
/// summary and the first body paragraph name the role of each agent template
/// that the check's `if` globs name, and no other role.
#[test]
fn evals_rerun_names_the_templates_that_its_check_names_and_no_other() {
    let found = entry("process.evals-rerun");
    let checked: Vec<&str> = found["check"]["if"]
        .as_array()
        .expect("the check lists globs")
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|glob| glob.strip_prefix(".claude/agents/"))
        .map(|file| file.strip_suffix(".md").unwrap_or(file))
        .collect();
    assert!(!checked.is_empty(), "the check names no agent template");
    for stem in &checked {
        assert!(
            TEMPLATE_ROLES.iter().any(|(known, _)| known == stem),
            "no role phrase for the template {stem:?}: add it to TEMPLATE_ROLES"
        );
    }
    for (part, text) in [
        ("summary", found["summary"].as_str().expect("a summary")),
        (
            "body[0]",
            found["body"][0].as_str().expect("a first paragraph"),
        ),
    ] {
        for (stem, role) in TEMPLATE_ROLES {
            assert_eq!(
                text.contains(role),
                checked.contains(&stem),
                "{part} of process.evals-rerun and the {stem} template: the check {} it, the text {} the role {role:?}",
                if checked.contains(&stem) {
                    "names"
                } else {
                    "omits"
                },
                if text.contains(role) {
                    "names"
                } else {
                    "omits"
                }
            );
        }
    }
}

#[test]
fn the_runs_of_a_scenario_carry_a_named_file_name_prefix() {
    for id in ["process.evals-rerun", "process.eval-fixture-procedure"] {
        let text = prose(&entry(id));
        assert!(
            text.contains("carry the `eval-` file-name prefix"),
            "{id} does not name the prefix"
        );
        assert!(
            !text.contains("the prefix that marks"),
            "{id} keeps the clause without a prefix"
        );
    }
}

#[test]
fn skills_for_procedures_keeps_its_rule_and_drops_the_plugin_hint() {
    assert_eq!(
        entry("process.skills-for-procedures")["summary"],
        "Create a project skill (`.claude/skills/`) for a repeatable procedure."
    );
}

#[test]
fn cite_durable_refs_keeps_every_component_of_its_locator() {
    let found = entry("knowledge-base.cite-durable-refs");
    assert!(
        found["summary"]
            .as_str()
            .expect("a summary")
            .contains("by work/task/round"),
        "the summary lost a component of the locator"
    );
    assert!(
        found["body"][1]
            .as_str()
            .expect("a second paragraph")
            .contains("the event's body of work, task, and round plus its subject"),
        "the body lost a component of the locator"
    );
}

#[test]
fn rulings_to_file_amends_the_spec_only_for_a_decision_that_changes_what_it_states() {
    let found = entry("process.rulings-to-file");
    assert_eq!(
        found["summary"],
        "Every ruling goes to its home file in the same turn; one that changes what the spec states amends the spec; one inside its intent goes to the brief or report."
    );
    let text = prose(&found);
    assert!(
        text.contains("A decision inside the spec's intent"),
        "{text}"
    );
    assert!(
        !text.contains("brief amendment that adds or drops a test"),
        "the refused clause of HR-153 is in the entry"
    );
}

#[test]
fn the_approved_clauses_of_ruling_183_join_their_entries_and_the_refused_one_does_not() {
    let clauses = [
        (
            "process.claims-match-artifacts",
            "re-runs or re-pins an earlier round's evidence",
        ),
        (
            "process.claims-match-artifacts",
            "line citation into a regenerated evidence file",
        ),
        (
            "process.claims-match-artifacts",
            "quotes every passage of the page that bears on it",
        ),
        (
            "process.claims-match-artifacts",
            "recomputed from its two timestamps",
        ),
        (
            "process.closure-claims-carry-enumeration",
            "selected by content, never by file name",
        ),
        (
            "process.closure-claims-carry-enumeration",
            "exception list is a closure with its own script",
        ),
        ("writing-style.code-comments", "names an openable source"),
        (
            "writing-style.code-comments",
            "swept for the round's own vocabulary",
        ),
        (
            "writing-style.doc-comments",
            "re-opens the callee's doc comment",
        ),
        (
            "process.tdd",
            "A RED against a stub of an implementation already drafted is a `mutation` entry",
        ),
        (
            "knowledge-base.state-only-the-source",
            "quote the page's own sentence",
        ),
        ("knowledge-base.state-only-the-source", "key nouns"),
    ];
    for (id, needle) in clauses {
        let text = prose(&entry(id));
        assert!(text.contains(needle), "{id} lacks {needle:?}");
    }
    let comments = prose(&entry("writing-style.code-comments"));
    assert!(
        !comments.contains("files the knowledge entry"),
        "the refused clause of ruling 183 is in writing-style.code-comments"
    );
}

#[test]
fn the_tiers_are_light_and_standard_and_a_critical_task_has_its_evidence() {
    let tiers = entry("process.risk-tiers");
    assert_eq!(
        tiers["summary"],
        "Pick the tier (light or standard) from the change's risk at plan time; the plan records the tier and the criterion that selected it."
    );
    let text = prose(&tiers);
    for needle in [
        "no task is critical",
        "A critical task is a task whose change alters the commands, flags, or output of a command-line interface, a schema, authentication or security code, or the data that an installed product keeps on disk.",
        "one recorded run that shows them failing at the base commit",
        "never a re-review",
    ] {
        assert!(text.contains(needle), "process.risk-tiers lacks {needle:?}");
    }
    let tdd = prose(&entry("process.tdd"));
    for needle in [
        "A critical task (`process.risk-tiers`)",
        "the whole suite passing at the head",
        "still works test-first for every further change",
    ] {
        assert!(tdd.contains(needle), "process.tdd lacks {needle:?}");
    }
    let loop_text = prose(&entry("process.bounded-review-loop"));
    assert!(
        loop_text.contains("Re-reviews per task: light 0, standard 1."),
        "{loop_text}"
    );
    assert!(
        loop_text.contains("A critical task gets no re-review"),
        "{loop_text}"
    );
}

#[test]
fn brainstorm_first_keeps_its_summary_and_names_the_designing_skill() {
    let found = entry("process.brainstorm-first");
    assert_eq!(
        found["summary"],
        "Start each batch with a brainstorming session or a written spec; get approval from the project's owner or decider before implementing."
    );
    assert_eq!(
        found["body"][0],
        "Where the reference workflow is installed, its designing skill drafts the spec from what the project already holds and asks only what that leaves open; otherwise write the spec by hand. Specs live in `docs/specs/`."
    );
}

#[test]
fn the_summary_check_names_a_release_clause_that_holds_a_version_number() {
    let found = entry("knowledge-base.summary-is-the-rule");
    let pattern = found["check"]["pattern"].as_str().expect("a pattern");
    let regex = Regex::with_flags(pattern, "i").expect("the pattern compiles");
    for phrase in [
        "until 1.3.0 ships",
        "until HR-048 ships a distribution channel",
        "Until the release lands",
    ] {
        assert!(regex.find(phrase).is_some(), "no match for {phrase:?}");
    }
    for phrase in ["until the end of the line. It ships", "until 1. It ships"] {
        assert!(regex.find(phrase).is_none(), "a match for {phrase:?}");
    }
    let text = prose(&found);
    assert!(
        text.contains("Each branch of the pattern carries a bracketed letter"),
        "{text}"
    );
    assert!(
        !text.contains("spells each word with one bracketed letter"),
        "{text}"
    );
}

#[test]
fn the_dependency_vetting_check_reaches_the_files_of_a_mise_tool() {
    let found = entry("security-hygiene.dependency-vetting");
    let globs: Vec<&str> = found["check"]["if"]
        .as_array()
        .expect("the check lists globs")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for glob in MISE_GLOBS {
        assert!(globs.contains(&glob), "the check lacks {glob}");
    }
    let matchers: Vec<globset::GlobMatcher> = globs
        .iter()
        .map(|glob| {
            GlobBuilder::new(glob)
                .literal_separator(true)
                .build()
                .unwrap_or_else(|error| panic!("compile {glob}: {error}"))
                .compile_matcher()
        })
        .collect();
    let names = mise_file_names();
    assert_eq!(
        names.len(),
        48,
        "the file names of mise's discovery: {names:?}"
    );
    for name in names {
        for path in [name.clone(), format!("frontend/{name}")] {
            assert!(
                matchers.iter().any(|matcher| matcher.is_match(&path)),
                "no glob of the check reaches {path}, which mise reads"
            );
        }
    }
}

/// Writes the report `dependency_vetting` field `vetting` to a file outside
/// `repo` and returns its path as text.
fn report_with(vetting: Value, holder: &Path) -> String {
    let path = holder.join("report.json");
    let report = json!({
        "kind": "task-report",
        "files_changed": ["mise.toml"],
        "dependency_vetting": vetting,
    });
    fs::write(&path, report.to_string()).expect("write the report");
    path.to_str().expect("a UTF-8 path").to_string()
}

/// The text of a config file that adds the tool `bd` to a mise project.
const MISE_TOOL: &str = "[tools]\nbd = \"1.3.1\"\n";

/// HR-207, HR-208: an audit of a commit that adds a mise tool without
/// `dependency_vetting` fails the row of the check, at the root and below it,
/// for a project config file that mise reads: the shared, local, environment,
/// and environment-local kinds of each glob the check names. (The test
/// `the_dependency_vetting_check_reaches_the_files_of_a_mise_tool` derives
/// every name from mise's own patterns.) The control rows change a file that
/// mise does not read and that the check does not name. Each row also passes
/// with a report that carries the field.
#[test]
fn an_audit_of_a_commit_that_adds_a_mise_tool_without_vetting_fails_the_row() {
    let rule = "security-hygiene.dependency-vetting";
    for (file, content, read_by_mise) in [
        ("mise.toml", MISE_TOOL, true),
        ("frontend/mise.toml", MISE_TOOL, true),
        (".mise.toml", MISE_TOOL, true),
        ("mise.ci.toml", MISE_TOOL, true),
        (".mise.local.toml", MISE_TOOL, true),
        (".mise.ci.toml", MISE_TOOL, true),
        (".mise.ci.local.toml", MISE_TOOL, true),
        ("frontend/.mise.ci.toml", MISE_TOOL, true),
        (".tool-versions", "bd 1.3.1\n", true),
        ("services/api/.tool-versions", "bd 1.3.1\n", true),
        ("mise/config.toml", MISE_TOOL, true),
        ("mise/config.local.toml", MISE_TOOL, true),
        ("mise/config.ci.toml", MISE_TOOL, true),
        ("mise/config.ci.local.toml", MISE_TOOL, true),
        ("mise/conf.d/tools.toml", MISE_TOOL, true),
        (".mise/config.toml", MISE_TOOL, true),
        (".mise/config.local.toml", MISE_TOOL, true),
        (".mise/config.ci.toml", MISE_TOOL, true),
        (".mise/config.ci.local.toml", MISE_TOOL, true),
        (".mise/conf.d/tools.toml", MISE_TOOL, true),
        (".config/mise.toml", MISE_TOOL, true),
        (".config/mise/config.toml", MISE_TOOL, true),
        (".config/mise/conf.d/tools.toml", MISE_TOOL, true),
        (".config/starship.toml", "add_newline = false\n", false),
        ("README.md", "# A readme\n", false),
    ] {
        let dir = scratch_git_repo();
        run_ok(dir.path(), &["init"]);
        run_ok(dir.path(), &["pack", "add", "security-hygiene"]);
        let base = commit_all(dir.path(), "chore: install the kit");
        let path = dir.path().join(file);
        fs::create_dir_all(path.parent().expect("a parent directory"))
            .expect("create the directory");
        fs::write(path, content).expect("write the file");
        commit_all(dir.path(), "chore: add a file");
        let holder = tempfile::tempdir().expect("tempdir");

        for (vetting, expected) in [
            (Value::Null, if read_by_mise { "fail" } else { "pass" }),
            (json!({"manifests": [file], "dependencies": []}), "pass"),
        ] {
            let report = report_with(vetting.clone(), holder.path());
            let output = run(
                dir.path(),
                &["audit", "--base", &base, "--ids", rule, "--report", &report],
            );
            let exit = if expected == "fail" { 1 } else { 0 };
            assert_eq!(
                output.status.code(),
                Some(exit),
                "{file} with {vetting}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let audit: Value = serde_json::from_str(&stdout(&output)).expect("audit prints JSON");
            let row = audit["rules"]
                .as_array()
                .expect("rules")
                .iter()
                .find(|row| row["id"] == rule)
                .unwrap_or_else(|| panic!("{file}: no row for {rule}: {audit}"));
            assert_eq!(row["result"], expected, "{file} with {vetting}: {row}");
            if expected == "fail" {
                let evidence = row["evidence"].as_str().expect("evidence");
                assert!(evidence.contains(file), "{file}: {evidence}");
            }
        }
    }
}

/// The exemptions of the scan of the core texts that `line_hits` applies.
const CORE_EXEMPTIONS: [&str; 4] = [
    "kit ids",
    "`controller`",
    "\"controller\"",
    "init --workflow reference",
];

/// The lines of `text` that hold a hit of the rewrite rule, as
/// `(line number, class, match)`. `exempt` says whether a hit of a class on a
/// line is a reading of the rule (see the module doc). Each exemption that
/// `line_hits` applies itself and that drops a hit is noted in `excused`.
fn line_hits(
    text: &str,
    names: &[&str],
    exempt: &dyn Fn(usize, &str, &str) -> bool,
    excused: &Excused,
) -> Vec<(usize, &'static str, String)> {
    let ids = kit_ids();
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let window = format!("{line}\n{}", lines.get(index + 1).copied().unwrap_or(""));
        let line_text = without_spelling(
            &without_ids(line, &ids, names, excused),
            "`controller`",
            "`controller`",
            names,
            excused,
        );
        let line_text = without_spelling(
            &line_text,
            "\"controller\"",
            "\"controller\"",
            names,
            excused,
        );
        for (class, matched) in hits(&line_text, names) {
            if class == "workflow-files" && window.contains("init --workflow reference") {
                excused.note("init --workflow reference");
                continue;
            }
            if exempt(index, class, line) {
                continue;
            }
            found.push((index + 1, class, matched));
        }
    }
    found
}

/// The index of the line that starts the section headed `heading` in `text`
/// and the index of the line that ends it (the next heading of the same or a
/// higher level, else the end of the text).
fn section(text: &str, heading: &str) -> (usize, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let level = heading.chars().take_while(|c| *c == '#').count();
    let start = lines
        .iter()
        .position(|line| *line == heading)
        .unwrap_or_else(|| panic!("no section {heading:?}"));
    let end = lines[start + 1..]
        .iter()
        .position(|line| {
            let hashes = line.chars().take_while(|c| *c == '#').count();
            hashes > 0 && hashes <= level && line.chars().nth(hashes) == Some(' ')
        })
        .map_or(lines.len(), |offset| start + 1 + offset);
    (start, end)
}

/// The heading of the section of the migrating-knowledge skill that moves
/// `backlog/` data to a tracker.
const MOVE_SECTION: &str = "## Set up a tracker and move `backlog/` data";

/// The heading of the section of the migrating-knowledge skill that lists the
/// steps of the seeded CI workflow, which runs `check-backlog` on every
/// install.
const CI_SECTION: &str = "## Generate your own CI gate";

/// A reading of the rewrite rule for the core texts: a hit of `classes` on a
/// line that holds `line`, in a file that `paths` covers (and below the
/// heading `section`, when set), names a missing thing because that is its
/// subject, so the scan excuses it.
struct CoreReading {
    /// The files below `template/` that the reading covers; a path that ends
    /// in a slash covers the directory.
    paths: &'static [&'static str],
    /// The heading that opens the section the reading covers; empty for the
    /// whole file.
    section: &'static str,
    /// Text that the line holds; empty for every line.
    line: &'static str,
    /// The classes that the reading excuses; none for every class.
    classes: &'static [&'static str],
    /// Why the hit is no pointer to a thing that a minimal install lacks.
    reason: &'static str,
}

impl CoreReading {
    /// Whether the reading covers `file`, a path below `template/`.
    fn covers(&self, file: &str) -> bool {
        self.paths.iter().any(|path| {
            if path.ends_with('/') {
                file.starts_with(path)
            } else {
                file == *path
            }
        })
    }

    /// Whether the reading excuses a hit of `class` on `line`.
    fn excuses(&self, class: &str, line: &str) -> bool {
        (self.classes.is_empty() || self.classes.contains(&class)) && line.contains(self.line)
    }
}

/// The readings of the core-text scan. The scan fails when a reading excuses
/// no hit, so a row cannot outlive the text it excuses.
const CORE_READINGS: [CoreReading; 8] = [
    CoreReading {
        paths: &["backlog/"],
        section: "",
        line: "",
        classes: &[],
        reason: "`init --tracker json` writes these files and a plain `init` writes none; their subject is the backlog",
    },
    CoreReading {
        paths: &[".github/workflows/knowledge.yml"],
        section: "",
        line: "houserules check-backlog",
        classes: &["tracker-subject", "backlog-commands"],
        reason: "the seeded CI workflow runs `check-backlog` on every install, and the command exits 0 where no tracker is installed",
    },
    CoreReading {
        paths: &[".claude/settings.json", "tools/claude-session-start.sh"],
        section: "",
        line: "claude-session-start.sh",
        classes: &["workflow-files"],
        reason: "the hook runs the core script of the Claude target; the `session-start` in its name is no pointer to the session-start script of the reference workflow",
    },
    CoreReading {
        paths: &[".claude/schemas/deliverables.json"],
        section: "",
        line: "\"backlog\"",
        classes: &["tracker-subject"],
        reason: "`backlog` is a field of the report format, which is core (design.md 5.124)",
    },
    CoreReading {
        paths: &[".claude/schemas/deliverables.json"],
        section: "",
        line: "\"fix_base\"",
        classes: &["dispatch-protocol"],
        reason: "`fix_base` is a field of the report format, which is core (design.md 5.124)",
    },
    CoreReading {
        paths: &["knowledge/areas.json"],
        section: "",
        line: "\"docs\"",
        classes: &["json-backlog"],
        reason: "a glob of an area names the files the area governs where they exist, as the globs of a check do",
    },
    CoreReading {
        paths: &[".claude/skills/migrating-knowledge/SKILL.md"],
        section: MOVE_SECTION,
        line: "",
        classes: &[
            "json-backlog",
            "dispatch-protocol",
            "tracker-subject",
            "deferral",
            "backlog-commands",
            "batch-record",
        ],
        reason: "the guide for moving `backlog/` data names the backlog because that is its subject",
    },
    CoreReading {
        paths: &[".claude/skills/migrating-knowledge/SKILL.md"],
        section: CI_SECTION,
        line: "",
        classes: &["tracker-subject", "backlog-commands"],
        reason: "the section lists the steps of the seeded CI workflow, whose `check-backlog` step runs on every install",
    },
];

/// The name of every class of the rewrite rule that a core text may not meet.
fn every_class() -> Vec<&'static str> {
    REWRITE.iter().chain(PROCESS_ONLY.iter()).copied().collect()
}

/// The core texts name no file, command, or role that a minimal install
/// lacks. The scan reads every file of `core_texts()` and excuses a hit only
/// through a row of `CORE_READINGS` or an exemption of `CORE_EXEMPTIONS`. It
/// fails for a row or an exemption that excuses no hit.
#[test]
fn the_core_texts_name_no_file_or_role_that_a_minimal_install_lacks() {
    let files = core_texts();
    for known in [
        "AGENTS.md",
        "CLAUDE.md",
        "docs/README.md",
        ".claude/skills/migrating-knowledge/SKILL.md",
    ] {
        assert!(
            files.iter().any(|file| file == known),
            "no core text {known}"
        );
    }
    let names = every_class();
    let used: Vec<Cell<bool>> = CORE_READINGS.iter().map(|_| Cell::new(false)).collect();
    let excused = Excused::default();
    let mut findings = Vec::new();
    for file in &files {
        let text = read(&format!("template/{file}"));
        let sections: Vec<Option<(usize, usize)>> = CORE_READINGS
            .iter()
            .map(|reading| {
                (reading.covers(file) && !reading.section.is_empty())
                    .then(|| section(&text, reading.section))
            })
            .collect();
        let exempt = |index: usize, class: &str, line: &str| {
            for (position, reading) in CORE_READINGS.iter().enumerate() {
                let inside =
                    sections[position].is_none_or(|(start, end)| index >= start && index < end);
                if reading.covers(file) && reading.excuses(class, line) && inside {
                    used[position].set(true);
                    return true;
                }
            }
            false
        };
        for (number, class, matched) in line_hits(&text, &names, &exempt, &excused) {
            findings.push(format!("template/{file}:{number}: {class}: {matched}"));
        }
    }
    for (reading, used) in CORE_READINGS.iter().zip(&used) {
        if !used.get() {
            findings.push(format!(
                "the reading of {:?} excuses no hit ({}): delete the row",
                reading.paths, reading.reason
            ));
        }
    }
    for name in excused.idle(&CORE_EXEMPTIONS) {
        findings.push(format!("the exemption {name} excuses no hit: delete it"));
    }
    assert!(
        findings.is_empty(),
        "{} findings in the core texts:\n{}",
        findings.len(),
        findings.join("\n")
    );
}

/// The core texts point at no lifecycle skill and count no standing rules.
/// The scan has no exclusion: it reads every core text, the session-start
/// script of the Claude target included.
#[test]
fn the_core_texts_point_at_no_lifecycle_skill_and_count_no_standing_rules() {
    let found =
        Regex::new(r"33 standing|orchestrating|finishing-a-feature").expect("the pattern compiles");
    let findings = lines_where(&core_texts(), &|line| found.find(line).is_some());
    assert!(findings.is_empty(), "{}", findings.join("\n"));
}

#[test]
fn the_seeded_agents_md_leaves_the_workflow_pointers_to_the_workflow_block() {
    let seed = read("template/AGENTS.md");
    for gone in [
        "- Run `houserules standing` and read `.claude/skills/orchestrating/SKILL.md`",
        "- If your harness can hand a task to another agent",
        "- A batch runs:",
        "- The backlog (`backlog/`) drives all work.",
        "A harness with no subagent concept",
    ] {
        assert!(!seed.contains(gone), "the seed keeps {gone:?}");
    }
    assert!(
        seed.contains("- Run `houserules standing` at the start of a session.\n"),
        "{seed}"
    );
    assert!(
        seed.contains("`houserules init --workflow reference` installs them."),
        "the seed does not say how an install gets the agent templates"
    );
    assert!(
        seed.contains(
            "Its install adds a `## Reference workflow` section to the houserules block below."
        ),
        "the seed does not name the heading of the block's workflow section"
    );
}

/// The handoff sentence of the reference workflow lives in its `agentsBlock`
/// only (spec 4.1). An install with the workflow holds one section for each
/// heading of `AGENTS.md`: the block's `## Reference workflow` beside the
/// seeded `## Workflow`, and the sentence once.
#[test]
fn an_install_with_the_workflow_holds_each_heading_and_the_handoff_sentence_once() {
    let dir = scratch_git_repo();
    run_ok(
        dir.path(),
        &["init", "--workflow", "reference", "--tracker", "json"],
    );
    let agents = fs::read_to_string(dir.path().join("AGENTS.md")).expect("read AGENTS.md");
    let headings: Vec<&str> = agents
        .lines()
        .filter(|line| line.starts_with("## "))
        .collect();
    for heading in &headings {
        assert_eq!(
            headings.iter().filter(|other| *other == heading).count(),
            1,
            "{heading:?} heads two sections:\n{agents}"
        );
    }
    for heading in ["## Workflow", "## Reference workflow"] {
        assert!(headings.contains(&heading), "no {heading:?} in\n{agents}");
    }
    for sentence in [
        "run handoffs in parallel only when the plan marks their file sets disjoint",
        "in its own session",
    ] {
        assert_eq!(
            agents.matches(sentence).count(),
            1,
            "{sentence:?} is not in the block alone:\n{agents}"
        );
    }
}

#[test]
fn the_migrating_knowledge_skill_guides_each_tracker_and_says_what_the_workflow_leaves_out() {
    let skill = read("template/.claude/skills/migrating-knowledge/SKILL.md");
    for heading in [
        MOVE_SECTION,
        "### The JSON tracker",
        "### GitHub Issues",
        "### Beads",
        "## Read houserules output through files",
        "## What the reference workflow leaves to you",
    ] {
        assert!(
            skill.lines().any(|line| line == heading),
            "no heading {heading:?}"
        );
    }
    for needle in [
        "houserules init --tracker json",
        "houserules init --tracker github",
        "gh auth login",
        "gh label create",
        "houserules init --tracker beads",
        "bd init",
        "in a project where `houserules init` has run",
        "bd bootstrap",
        "bd config set",
        "Write houserules output to a file and read the file",
        "treat a `<<` marker or a \"more items\" note as a lost read",
        "debugging, writing new skills, or analysing a past session",
    ] {
        assert!(skill.contains(needle), "the skill lacks {needle:?}");
    }
}
