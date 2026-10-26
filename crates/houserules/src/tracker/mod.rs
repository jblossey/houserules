//! The work-item tracker contract (batch 34, spec section 3).
//!
//! One trait, [`Tracker`], is every backlog function: the reads `list`,
//! `batch`, `get`, and `resolve`, the writes `set`, `defer`, and the backlog
//! half of `archive`, the checks of `check-backlog`, and the description of
//! the adapter (its name, its id shapes, its `backlogId` pattern). No command
//! reads `backlog/` itself; each calls the adapter that [`installed`]
//! builds. The JSON adapter (`json`) wraps the files and code of the 1.4.0
//! backlog and prints what that code printed. The GitHub adapter (`github`)
//! keeps the work items in the issues of one repository: it runs the `gh` CLI
//! (`gh`) and keeps the fields that no issue field holds in a fenced block of
//! the issue body (`block`). The Beads adapter (`beads`) keeps them in the
//! issues of a Beads store: it runs the `bd` CLI and keeps the fields that no
//! issue field holds in a block of the description. Both read an issue through
//! one trait (`hosted`), which holds the record shapes of the 1.4.0 backlog,
//! the contract checks, and the planning of the writes that the two share.
//!
//! An install has the tracker that `.houserules.json` names under `tracker`,
//! or none ([`Marker::tracker`]). [`installed`] builds the adapter and makes
//! no backend call: a backend is reached only by an operation, so a command
//! that never needs the tracker (a knowledge read, `render`) never fails for
//! a tracker that cannot be reached. [`lines`] holds the exact text of every
//! line that the contract prints, and `request` the one reader and the one
//! checker of what `set` and `defer` ask of any adapter.

pub(crate) mod beads;
mod block;
mod gh;
mod github;
mod hosted;
mod json;
pub(crate) mod lines;
mod request;
#[cfg(test)]
pub(crate) mod test_support;

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::process::ExitCode;

use serde_json::Value;

pub(crate) use request::{
    Change, DeferRequest, DeferTarget, changes_map, check_defer_request, parse_set,
};

use crate::install::marker::Marker;

/// The path of the deliverables schema below the install root, in the
/// components that [`write_backlog_id_pattern`] joins.
const DELIVERABLES_SCHEMA: [&str; 3] = [".claude", "schemas", "deliverables.json"];

/// A tracker adapter: where the work items of an install live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Adapter {
    /// The `backlog/*.json` files of the install.
    Json,
    /// The issues of a GitHub repository, through the `gh` CLI.
    Github,
    /// The issues of a Beads store, through the `bd` CLI.
    Beads,
}

impl Adapter {
    /// The name that `.houserules.json` and every line use for the adapter.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Adapter::Json => "json",
            Adapter::Github => "github",
            Adapter::Beads => "beads",
        }
    }

    /// The adapter that `name` names, when it names one.
    fn parse(name: &str) -> Option<Self> {
        [Adapter::Json, Adapter::Github, Adapter::Beads]
            .into_iter()
            .find(|adapter| adapter.as_str() == name)
    }
}

/// The `tracker` key of `.houserules.json`: the adapter, and for GitHub the
/// repository (`<owner>/<name>`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrackerConfig {
    /// The adapter.
    pub(crate) adapter: Adapter,
    /// The repository, for the GitHub adapter only.
    pub(crate) repo: Option<String>,
}

impl TrackerConfig {
    /// The value that `.houserules.json` holds under `tracker`: `adapter`,
    /// then `repo` when there is one.
    pub(crate) fn to_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("adapter".to_string(), Value::from(self.adapter.as_str()));
        if let Some(repo) = &self.repo {
            map.insert("repo".to_string(), Value::from(repo.as_str()));
        }
        Value::Object(map)
    }

    /// Reads the value of the `tracker` key. A fault is the sentence that
    /// names it; the caller puts the file in front.
    pub(crate) fn from_value(value: &Value) -> Result<Self, String> {
        let Some(map) = value.as_object() else {
            return Err("tracker must be an object".to_string());
        };
        if let Some(key) = map
            .keys()
            .find(|key| !["adapter", "repo"].contains(&key.as_str()))
        {
            return Err(format!("tracker: unknown key {key:?}"));
        }
        let adapter = map
            .get("adapter")
            .and_then(Value::as_str)
            .and_then(Adapter::parse)
            .ok_or_else(|| "tracker.adapter must be json, github, or beads".to_string())?;
        let repo = match (adapter, map.get("repo")) {
            (Adapter::Github, Some(Value::String(repo))) if is_repo_name(repo) => {
                Some(repo.clone())
            }
            (Adapter::Github, _) => {
                return Err(lines::TRACKER_REPO_FORMAT.to_string());
            }
            (_, Some(_)) => {
                return Err("tracker.repo belongs to the github adapter only".to_string());
            }
            (_, None) => None,
        };
        Ok(Self { adapter, repo })
    }
}

/// `true` when `repo` is `<owner>/<name>`: one slash, two non-empty parts,
/// each of ASCII letters, digits, `_`, `.`, and `-`.
pub(crate) fn is_repo_name(repo: &str) -> bool {
    let part = |text: &str| {
        !text.is_empty()
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
    };
    repo.split_once('/')
        .is_some_and(|(owner, name)| part(owner) && part(name))
}

/// Every filter that `list` accepts, typed. Each field uses the loose
/// comparison that the adapter's own reader documents.
pub(crate) struct ListOpts {
    /// Only items whose status is `open` or `partial`.
    pub(crate) open: bool,
    /// Only items with this exact status.
    pub(crate) status: Option<String>,
    /// Only items with this exact milestone (`-` matches a missing one).
    pub(crate) milestone: Option<String>,
    /// Only items filed under this section.
    pub(crate) section: Option<String>,
    /// Only items of this exact type.
    pub(crate) item_type: Option<String>,
    /// Only items assigned to this exact batch number.
    pub(crate) batch: Option<String>,
    /// Only items that carry a deferral; each row then holds the deferral.
    pub(crate) deferred: bool,
}

/// What the tracker says about an id (`Tracker::resolve`).
pub(crate) enum Resolution {
    /// The id names a work item or record.
    Found(Resolved),
    /// The tracker holds nothing under the id.
    NotFound,
    /// The id names something the tracker does not hold as a work item; the
    /// reason is the text of the line that tells the author why.
    NotAnItem {
        /// Why the id is no work item.
        reason: String,
    },
}

/// A work item or record that `Tracker::resolve` found. The gates read
/// whether it carries the deferral marker with a reason (spec 3.7) and read
/// nothing else of it.
pub(crate) struct Resolved {
    /// The deferral marker, when the item carries one.
    pub(crate) deferral: Option<Deferral>,
}

/// The deferral marker of a work item, as far as a gate reads it.
pub(crate) struct Deferral {
    /// Why the work waits.
    pub(crate) reason: String,
}

/// Why a tracker operation failed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TrackerError {
    /// The tracker cannot be reached (spec 3.4): its `adapter`, the `cause`,
    /// and the `setup` step that fixes it.
    Unreachable {
        /// The adapter's name.
        adapter: &'static str,
        /// What is wrong.
        cause: String,
        /// What to do about it.
        setup: String,
    },
    /// The tracker holds nothing under the id; the text is the line to print.
    NotFound(String),
    /// Any other failure, or a usage fault; the text is the line to print.
    Command(String),
}

impl TrackerError {
    /// The exit code of the command that failed: a gate (`check-backlog`,
    /// `audit`) exits 1 for a tracker that cannot be reached, every other
    /// command exits 2, and every other failure exits 2 (spec 3.4).
    pub(crate) fn exit_code(&self, gate: bool) -> u8 {
        match self {
            TrackerError::Unreachable { .. } if gate => 1,
            _ => 2,
        }
    }

    /// Prints the error on stderr and returns the exit code of the command.
    pub(crate) fn report(&self, gate: bool) -> ExitCode {
        eprintln!("{self}");
        ExitCode::from(self.exit_code(gate))
    }
}

impl fmt::Display for TrackerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackerError::Unreachable {
                adapter,
                cause,
                setup,
            } => f.write_str(&lines::unreachable(adapter, cause, setup)),
            TrackerError::NotFound(line) | TrackerError::Command(line) => f.write_str(line),
        }
    }
}

/// The contract of a tracker adapter (spec 3.1). Every operation may reach
/// the backend, so every one returns a [`TrackerError`] for a backend that
/// cannot be reached.
pub(crate) trait Tracker {
    /// The adapter's name: `json`, `github`, or `beads`.
    fn name(&self) -> &'static str;

    /// `true` when `id` has one of the adapter's declared id shapes. A pure
    /// function of the id: it reaches no backend.
    fn matches_id(&self, id: &str) -> bool;

    /// Says whether the tracker holds `id`, and if so whether it carries the
    /// deferral marker.
    fn resolve(&self, id: &str) -> Result<Resolution, TrackerError>;

    /// The rows of `list` for the items that match `opts`.
    fn list(&self, opts: &ListOpts) -> Result<Vec<Value>, TrackerError>;

    /// The record of `batch <number>`: its summary and its item rows.
    fn batch(&self, number: &str) -> Result<Value, TrackerError>;

    /// The stored record of each id, in the order given. A missing id is
    /// [`TrackerError::NotFound`].
    fn get(&self, ids: &[String]) -> Result<Vec<Value>, TrackerError>;

    /// Applies `field=value` assignments to the item `id`. Returns the id and
    /// the applied changes as `{"id": <id>, "changes": {<field>: <value>}}`.
    fn set(&mut self, id: &str, assignments: &[String]) -> Result<Value, TrackerError>;

    /// Files a deferral (`request.title`) or marks an existing item
    /// (`request.item`). Returns the id of the item.
    fn defer(&mut self, request: &DeferRequest) -> Result<String, TrackerError>;

    /// Runs the backlog half of `archive`, and pushes one line for each move
    /// to `lines`. The lines stay in `lines` when the sweep fails, so the
    /// caller prints what happened before the failure.
    fn archive_backlog_half(&mut self, lines: &mut Vec<String>) -> Result<(), TrackerError>;

    /// The contract checks and the adapter's own checks of `check-backlog`:
    /// `(errors, warnings)`.
    fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError>;

    /// The pattern that the `backlogId` definition of the deliverables schema
    /// holds for this adapter (spec 3.9). An adapter whose pattern comes from
    /// its backend (Beads: the issue prefix of the store) fails with the error
    /// of that backend when it cannot name the pattern, and never answers with
    /// a default one: a pattern is written only from what the backend named.
    fn backlog_id_pattern(&self) -> Result<String, TrackerError>;
}

/// The commands that the AGENTS.md block names for a session on `adapter`:
/// the one to run at the start, and the one to run at the end (spec 3.10). A
/// function of the adapter, so `render` never builds a tracker.
pub(crate) fn session_commands(adapter: Adapter) -> (&'static str, &'static str) {
    match adapter {
        Adapter::Json => ("houserules list --open", "houserules check-backlog"),
        Adapter::Github => (
            "gh issue list --label houserules:batch --state open",
            "houserules check-backlog",
        ),
        Adapter::Beads => ("bd ready", "bd dolt push"),
    }
}

/// What a command that needs the tracker finds in an install.
pub(crate) enum Lookup {
    /// The install has this tracker.
    Installed(Box<dyn Tracker>),
    /// The install has no tracker.
    Absent(NoTracker),
}

/// The two states of an install with no tracker (spec 3.4): the state decides
/// which line a command prints and, for the gates, the exit code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoTracker {
    /// No `backlog/` directory exists: line A.
    Bare,
    /// A `backlog/` directory exists: line B.
    BacklogDir,
}

impl NoTracker {
    /// The state of the install at `root`, which has no tracker.
    pub(crate) fn of(root: &Path) -> Self {
        if root.join("backlog").is_dir() {
            NoTracker::BacklogDir
        } else {
            NoTracker::Bare
        }
    }

    /// Line A or line B.
    pub(crate) fn line(self) -> &'static str {
        match self {
            NoTracker::Bare => lines::NO_TRACKER,
            NoTracker::BacklogDir => lines::BACKLOG_WITHOUT_TRACKER,
        }
    }
}

/// Looks for the tracker of the install at `root`: reads the marker and
/// builds the adapter it names (`installed`). A marker that cannot be read is
/// a [`TrackerError::Command`] with the marker's own line.
pub(crate) fn lookup(root: &Path) -> Result<Lookup, TrackerError> {
    let marker = Marker::read_or_default(root, "WI").map_err(TrackerError::Command)?;
    Ok(match installed(root, &marker)? {
        Some(tracker) => Lookup::Installed(tracker),
        None => Lookup::Absent(NoTracker::of(root)),
    })
}

/// Builds the adapter that `marker` names for the install at `root`, or
/// `None` when the marker names none. A `tracker` key that cannot be read is
/// a [`TrackerError::Command`] with the marker's own line. Makes no backend
/// call.
pub(crate) fn installed(
    root: &Path,
    marker: &Marker,
) -> Result<Option<Box<dyn Tracker>>, TrackerError> {
    marker
        .tracker()
        .map_err(TrackerError::Command)?
        .map(|config| register(root, &marker.id_prefix_or("WI"), &config, None))
        .transpose()
}

/// The one place that maps an [`Adapter`] to the code that implements it.
/// `id_prefix` is the prefix of the item ids of the install, for the JSON
/// adapter. `issue_prefix` is the issue prefix that `bd` named to `init`, for
/// the Beads adapter; an adapter built without it asks `bd` when it needs it.
fn register(
    root: &Path,
    id_prefix: &str,
    config: &TrackerConfig,
    issue_prefix: Option<&str>,
) -> Result<Box<dyn Tracker>, TrackerError> {
    match config.adapter {
        Adapter::Json => Ok(Box::new(json::JsonTracker::new(
            root,
            id_prefix.to_string(),
        ))),
        Adapter::Github => {
            let repo = config
                .repo
                .clone()
                .ok_or_else(|| TrackerError::Command(lines::TRACKER_REPO_FORMAT.to_string()))?;
            Ok(Box::new(github::GithubTracker::new(
                repo,
                gh::Runner::from_env(),
            )))
        }
        Adapter::Beads => Ok(Box::new(beads::BeadsTracker::new(root, issue_prefix))),
    }
}

/// A tracker that `init --tracker` installs: the `tracker` key to write, and
/// what the backend of the adapter named before any write (spec 3.3, C37).
pub(crate) struct TrackerInstall {
    /// The `tracker` key.
    pub(crate) config: TrackerConfig,
    /// The issue prefix that `bd` named to this command, for the Beads
    /// adapter; `None` for every other adapter.
    pub(crate) issue_prefix: Option<String>,
}

/// The work of `init --tracker` (spec 3.3), run on the install at `root` whose
/// `marker` `init` has stamped, before the `tracker` key is set. The arm of
/// the adapter comes first: the JSON adapter seeds the tracker files that are
/// missing (spec 4.2, `trackerFiles`) and records their baselines in
/// `marker`; the GitHub adapter has no file and makes no call, because `init`
/// resolved its repository into `install` before any write
/// (`install::tracker_to_install`); the Beads adapter has no file and makes no
/// call either, because `init` read the issue prefix from `bd` there too, and
/// refused with the line of spec 3.3 when `bd` is missing or the install has no
/// store. Then every adapter shares one tail ([`finish_init`]) on the tracker
/// that [`register`] builds, so a failure of the tail leaves the `tracker` key
/// unwritten. Returns the lines `init` prints.
pub(crate) fn prepare_init(
    root: &Path,
    marker: &mut Marker,
    install: &TrackerInstall,
) -> Result<Vec<String>, TrackerError> {
    let config = &install.config;
    let mut lines = match config.adapter {
        Adapter::Json => {
            crate::install::seed_tracker_files(root, marker).map_err(TrackerError::Command)?
        }
        Adapter::Github | Adapter::Beads => Vec::new(),
    };
    let tracker = register(
        root,
        &marker.id_prefix_or("WI"),
        config,
        install.issue_prefix.as_deref(),
    )?;
    lines.extend(finish_init(root, marker, config.adapter, tracker.as_ref())?);
    Ok(lines)
}

/// The `backlogId` pattern of the tracker that `init --tracker` installs
/// (`install`) on the install at `root` whose item ids carry `id_prefix`
/// (spec 3.9). `init` asks before it writes a file and seeds the deliverables
/// schema with the pattern, so that the tail of the install ([`finish_init`])
/// finds the schema current. Makes no backend call: `install` holds the issue
/// prefix that `bd` named. A tracker that cannot name its pattern is the error
/// of the call (C37).
pub(crate) fn install_backlog_id_pattern(
    root: &Path,
    id_prefix: &str,
    install: &TrackerInstall,
) -> Result<String, TrackerError> {
    register(
        root,
        id_prefix,
        &install.config,
        install.issue_prefix.as_deref(),
    )?
    .backlog_id_pattern()
}

/// The tail of `init --tracker` that every adapter shares (spec 3.3): writes
/// the `backlogId` pattern of `tracker` (a deliverables schema in `ownedPaths`
/// is left alone, spec 3.9) and, where the install holds the seeded CI
/// workflow, brings it to the content for `adapter` (spec 4.8). It holds no code of one adapter, so an adapter
/// gets both by being built in [`register`]. A tracker that cannot name its
/// pattern fails the tail before the schema and the workflow are written (C37).
/// Returns the lines `init` prints.
fn finish_init(
    root: &Path,
    marker: &mut Marker,
    adapter: Adapter,
    tracker: &dyn Tracker,
) -> Result<Vec<String>, TrackerError> {
    let command = TrackerError::Command;
    let mut lines = Vec::new();
    let schema = DELIVERABLES_SCHEMA.join("/");
    if !marker.owned_paths().contains(&schema) && write_backlog_id_pattern(root, tracker)? {
        lines.push(format!("wrote {schema}"));
    }
    lines.extend(crate::install::write_ci_workflow(root, marker, Some(adapter)).map_err(command)?);
    Ok(lines)
}

/// Rewrites the `backlogId` pattern of the deliverables schema of the
/// install at `root` to the pattern of `tracker` (spec 3.9). Returns `false`
/// when the file is absent or already holds the pattern, so a second run
/// writes nothing. The rest of the file keeps its bytes. A tracker that cannot
/// name its pattern is the error of the call, and the file keeps all its bytes.
pub(crate) fn write_backlog_id_pattern(
    root: &Path,
    tracker: &dyn Tracker,
) -> Result<bool, TrackerError> {
    let command = TrackerError::Command;
    let path = DELIVERABLES_SCHEMA
        .iter()
        .fold(root.to_path_buf(), |path, part| path.join(part));
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(command(format!("{}: {error}", path.display()))),
    };
    let pattern = tracker.backlog_id_pattern()?;
    let rewritten = replace_backlog_id_pattern(&text, &pattern)
        .map_err(|fault| command(format!("{}: {fault}", path.display())))?;
    if rewritten == text {
        return Ok(false);
    }
    crate::written::write(&path, rewritten)
        .map_err(|error| command(format!("{}: {error}", path.display())))?;
    Ok(true)
}

/// `text`, the deliverables schema, with the `pattern` of its `backlogId`
/// definition replaced by `pattern`. The definition is a flat object whose
/// `pattern` string follows its opening brace. `update` builds the content of
/// the schema for an install with it (`install::sync`).
pub(crate) fn replace_backlog_id_pattern(text: &str, pattern: &str) -> Result<String, String> {
    const DEFINITION: &str = "\"backlogId\": {";
    const KEY: &str = "\"pattern\":";
    let missing = || "$defs.backlogId has no pattern to rewrite".to_string();
    let definition = text.find(DEFINITION).ok_or_else(missing)? + DEFINITION.len();
    let key = text[definition..].find(KEY).ok_or_else(missing)? + definition;
    if text[definition..key].contains('}') {
        return Err(missing());
    }
    let start = key + KEY.len();
    let literal_start = start + (text[start..].len() - text[start..].trim_start().len());
    let mut stream =
        serde_json::Deserializer::from_str(&text[literal_start..]).into_iter::<Value>();
    let old = stream
        .next()
        .and_then(Result::ok)
        .filter(Value::is_string)
        .ok_or_else(missing)?;
    debug_assert!(old.is_string());
    let literal_end = literal_start + stream.byte_offset();
    let new = serde_json::to_string(pattern).expect("a string always serializes");
    Ok(format!(
        "{}{new}{}",
        &text[..literal_start],
        &text[literal_end..]
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A deliverables schema with a flat `backlogId` definition between other
    /// definitions that hold patterns of their own.
    const SCHEMA: &str = "{\n  \"$defs\": {\n    \"knowledgeId\": {\n      \"type\": \"string\",\n      \"pattern\": \"^k$\"\n    },\n    \"backlogId\": {\n      \"type\": \"string\",\n      \"pattern\": \"^(WI-[0-9]+|PP-[0-9]+-[0-9]+)$\"\n    },\n    \"text\": {\n      \"type\": \"string\",\n      \"pattern\": \"^t$\"\n    }\n  }\n}\n";

    /// The pattern of `backlogId` is replaced and every other byte stays.
    #[test]
    fn the_pattern_of_backlog_id_is_replaced_and_every_other_byte_stays() {
        let rewritten = replace_backlog_id_pattern(SCHEMA, "^#[0-9]+$").expect("rewritten");
        assert_eq!(
            rewritten,
            SCHEMA.replace("^(WI-[0-9]+|PP-[0-9]+-[0-9]+)$", "^#[0-9]+$")
        );
    }

    /// A pattern with a character that JSON escapes is written escaped.
    #[test]
    fn a_pattern_with_a_character_that_json_escapes_is_written_escaped() {
        let rewritten = replace_backlog_id_pattern(SCHEMA, "^\\d+\"$").expect("rewritten");
        let parsed: Value = serde_json::from_str(&rewritten).expect("valid JSON");
        assert_eq!(parsed["$defs"]["backlogId"]["pattern"], json!("^\\d+\"$"));
    }

    /// A schema with no `backlogId` pattern is one named fault.
    #[test]
    fn a_schema_without_the_pattern_is_one_named_fault() {
        for text in [
            "{}",
            "{\"backlogId\": {\"type\": \"string\"}, \"other\": {\"pattern\": \"^x$\"}}",
            "{\"backlogId\": {\"pattern\": 3}}",
        ] {
            assert_eq!(
                replace_backlog_id_pattern(text, "^x$"),
                Err("$defs.backlogId has no pattern to rewrite".to_string()),
                "{text}"
            );
        }
    }

    /// The tail of `init --tracker` that every adapter shares (spec 3.3)
    /// writes the `backlogId` pattern of the tracker it gets, and brings a held
    /// seeded CI workflow to the content for the adapter, with no code of the
    /// adapter in it: here the GitHub adapter, and a tracker whose pattern is
    /// no JSON pattern.
    #[test]
    fn the_shared_tail_writes_the_pattern_of_the_tracker_and_syncs_the_workflow() {
        const WORKFLOW: &str = ".github/workflows/knowledge.yml";
        let dir = tempfile::tempdir().expect("tempdir");
        let init = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .expect("run git init");
        assert!(init.success(), "git init failed");
        let schema = dir.path().join(".claude/schemas/deliverables.json");
        fs::create_dir_all(schema.parent().expect("a parent")).expect("create .claude/schemas");
        fs::write(&schema, SCHEMA).expect("write the schema");
        let workflow = dir.path().join(WORKFLOW);
        fs::create_dir_all(workflow.parent().expect("a parent")).expect("create the workflows");
        fs::write(&workflow, "stale\n").expect("write the workflow");
        let mut marker = Marker::read_or_default(dir.path(), "WI").expect("a fresh marker");
        marker.set_baselines(serde_json::Map::from_iter([(
            format!(":/{WORKFLOW}"),
            json!(crate::baseline::hash(b"stale\n")),
        )]));
        let tracker = test_support::PatternOnly {
            pattern: Some("^#[0-9]+$"),
        };

        let lines =
            finish_init(dir.path(), &mut marker, Adapter::Github, &tracker).expect("the tail");

        assert_eq!(
            lines,
            [
                "wrote .claude/schemas/deliverables.json".to_string(),
                format!("wrote {WORKFLOW}"),
            ]
        );
        assert_eq!(
            fs::read_to_string(&schema).expect("read the schema"),
            SCHEMA.replace("^(WI-[0-9]+|PP-[0-9]+-[0-9]+)$", "^#[0-9]+$")
        );
        assert_ne!(
            fs::read_to_string(&workflow).expect("read the workflow"),
            "stale\n"
        );
    }

    /// C37: a tracker that cannot name its pattern fails the shared tail with
    /// its own error, and the tail writes nothing: the schema keeps its bytes
    /// and the seeded workflow stays unwritten.
    #[test]
    fn a_tracker_that_cannot_name_its_pattern_fails_the_tail_and_writes_nothing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let schema = dir.path().join(".claude/schemas/deliverables.json");
        fs::create_dir_all(schema.parent().expect("a parent")).expect("create .claude/schemas");
        fs::write(&schema, SCHEMA).expect("write the schema");
        let mut marker = Marker::read_or_default(dir.path(), "WI").expect("a fresh marker");
        let tracker = test_support::PatternOnly { pattern: None };

        let error = finish_init(dir.path(), &mut marker, Adapter::Beads, &tracker)
            .expect_err("the tail fails");

        assert_eq!(error, test_support::PatternOnly::unreachable());
        assert_eq!(
            error.to_string(),
            "tracker pattern-only cannot be reached: the backend is down; bring it up"
        );
        assert_eq!(
            fs::read_to_string(&schema).expect("read the schema"),
            SCHEMA
        );
        assert!(!dir.path().join(".github").exists());
    }

    /// The tracker key reads back as written, and each fault is one sentence.
    #[test]
    fn the_tracker_key_reads_back_and_each_fault_is_one_sentence() {
        for config in [
            TrackerConfig {
                adapter: Adapter::Json,
                repo: None,
            },
            TrackerConfig {
                adapter: Adapter::Beads,
                repo: None,
            },
            TrackerConfig {
                adapter: Adapter::Github,
                repo: Some("acme/widgets".to_string()),
            },
        ] {
            assert_eq!(TrackerConfig::from_value(&config.to_value()), Ok(config));
        }
        let faults = [
            (json!("json"), "tracker must be an object"),
            (json!({}), "tracker.adapter must be json, github, or beads"),
            (
                json!({"adapter": "svn"}),
                "tracker.adapter must be json, github, or beads",
            ),
            (
                json!({"adapter": 1}),
                "tracker.adapter must be json, github, or beads",
            ),
            (
                json!({"adapter": "github"}),
                "tracker.repo must be <owner>/<name> for the github adapter",
            ),
            (
                json!({"adapter": "github", "repo": "widgets"}),
                "tracker.repo must be <owner>/<name> for the github adapter",
            ),
            (
                json!({"adapter": "json", "repo": "a/b"}),
                "tracker.repo belongs to the github adapter only",
            ),
            (
                json!({"adapter": "json", "extra": 1}),
                "tracker: unknown key \"extra\"",
            ),
        ];
        for (value, fault) in faults {
            assert_eq!(
                TrackerConfig::from_value(&value),
                Err(fault.to_string()),
                "{value}"
            );
        }
    }

    /// A repository name is two non-empty parts around one slash.
    #[test]
    fn a_repository_name_is_two_parts_around_one_slash() {
        for good in ["a/b", "acme/wid-gets_1.rs"] {
            assert!(is_repo_name(good), "{good}");
        }
        for bad in ["", "a", "a/", "/b", "a/b/c", "a b/c", "a/b c"] {
            assert!(!is_repo_name(bad), "{bad}");
        }
    }

    /// Spec 3.4: only an unreachable tracker differs between a gate and
    /// another command.
    #[test]
    fn an_unreachable_tracker_exits_1_from_a_gate_and_2_from_another_command() {
        let unreachable = TrackerError::Unreachable {
            adapter: "json",
            cause: "backlog/ is missing".to_string(),
            setup: "restore backlog/ from version control".to_string(),
        };
        assert_eq!(
            (unreachable.exit_code(true), unreachable.exit_code(false)),
            (1, 2)
        );
        for other in [
            TrackerError::NotFound("unknown id".to_string()),
            TrackerError::Command("load failure".to_string()),
        ] {
            assert_eq!((other.exit_code(true), other.exit_code(false)), (2, 2));
        }
        assert_eq!(
            unreachable.to_string(),
            "tracker json cannot be reached: backlog/ is missing; restore backlog/ from version control"
        );
    }

    /// Spec 3.10: the session commands of each adapter.
    #[test]
    fn the_session_commands_are_those_of_the_spec() {
        assert_eq!(
            session_commands(Adapter::Json),
            ("houserules list --open", "houserules check-backlog")
        );
        assert_eq!(
            session_commands(Adapter::Github),
            (
                "gh issue list --label houserules:batch --state open",
                "houserules check-backlog"
            )
        );
        assert_eq!(
            session_commands(Adapter::Beads),
            ("bd ready", "bd dolt push")
        );
    }

    /// The adapter names are those `.houserules.json` and the command line
    /// use.
    #[test]
    fn the_adapter_names_round_trip() {
        for adapter in [Adapter::Json, Adapter::Github, Adapter::Beads] {
            assert_eq!(Adapter::parse(adapter.as_str()), Some(adapter));
        }
        assert_eq!(Adapter::parse("Json"), None);
    }
}
