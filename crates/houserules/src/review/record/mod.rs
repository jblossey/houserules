//! `houserules record`: the change record that a merge leaves as a git note
//! (spec 8).
//!
//! `record write` computes the record of a pushed range from git, the audit,
//! and the inputs of the host, fits it to `review.record.maxBytes` ([`budget`]),
//! and stores it as a note ([`notes`]) or writes its bytes to a file.
//! `record show` finds the record of a commit (the record that it keys, else the
//! record whose range holds it), checks its rule hashes against git, and prints
//! it ([`document`]). `record list` lists the records of a range ([`list`], on
//! the reader of [`range`]), and `record audit` sets the post-merge audit of a
//! record ([`audit`]).
//!
//! A command exits 2 for a setup error and 1 for a refusal: a key that holds a
//! record of another range, a clone with no records, a commit with no record,
//! a record that holds an audit already, an audit that would bring a record past
//! its budget, and a record whose rule hash differs from git's. Each is one named
//! line on stderr (`houserules.crash-paths-are-named`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand};
use indexmap::IndexMap;

mod audit;
pub(crate) mod budget;
mod document;
#[cfg(test)]
mod fixtures;
mod list;
pub(crate) mod notes;
mod range;
mod sample;

use super::bundle;
use super::change::{self, AuditSource, RulesPresence};
use super::ci;
use super::evidence;
use super::git;
use super::intent::{self, IntentRef};
use super::route;
use super::rules_hash;
use crate::emit::emit;
use crate::install::marker::Marker;
use crate::install::marker::review::{Adapter, no_intent_source};
use crate::published;
use crate::published::audit_result::{AuditResult, References, Summary};
use crate::published::change_record::{ChangeRecord, Host, Sample};
use crate::published::route::Route;
use crate::rules::load_base;

/// The arguments of `houserules record`.
#[derive(Args)]
pub(crate) struct RecordArgs {
    /// What to do with the record.
    #[command(subcommand)]
    pub(crate) action: RecordAction,
}

impl RecordArgs {
    /// The `--dir` by which D4 finds the install that the subcommand reads.
    pub(crate) fn dir(&self) -> &Option<PathBuf> {
        match &self.action {
            RecordAction::Write(write) => &write.dir,
            RecordAction::Show(show) => &show.dir,
            RecordAction::List(list) => &list.dir,
            RecordAction::Audit(audit) => &audit.dir,
        }
    }
}

/// The subcommands of `houserules record`.
#[derive(Subcommand)]
pub(crate) enum RecordAction {
    /// Computes the change record of the range `--base` to `--head` and writes it
    /// as a git note on `--key` (default: `--head`) under the notes ref of the
    /// install: `refs/notes/houserules` for a top-level install, else
    /// `refs/notes/houserules-sub/<install path>`. `--out` writes the bytes of
    /// the note to a file in place of the note; only the CI units push the ref.
    /// The record holds the commits of the range, the rule hashes at the merge
    /// base and at the head, the audit rows, the intents, the evidence, the
    /// route, the sample draw, the evidence bundle in `--bundle` (the description
    /// of the request), and the reviewers. The audit rows come from `--audit`,
    /// else from an audit that the command runs in process. `--intent
    /// <source>:<ref>` names an intent, else the `Intent:` line of the bundle
    /// does. `--reviewer` names one approver; repeat the flag.
    /// `--reviewers-pending` writes no reviewers and leaves room for them. A
    /// later job may set them, or the approvers could not be read and stay
    /// unknown. `--score` adds an external score to the route. `--host` records
    /// the host. A record over `review.record.maxBytes` is cut in a fixed order,
    /// and `truncated` names each cut. A key that holds a record of the same
    /// range gets the new record, and its post-merge audit stays. A key that
    /// holds a record of another range exits 1. An audit that cannot run, or a
    /// tracker that cannot be reached, does not stop the record: `audit.error`
    /// holds the line of the audit, `audit.references` holds the failure lines of
    /// the references, the command prints each line on stderr, and the exit is 0.
    /// It exits 2 for a setup error, for example: a ref that is no commit, an
    /// `--audit` file of another range or that is no audit result, a `--bundle`
    /// file that cannot be read, an `--intent` whose source is no adapter, a
    /// `--score` outside 0 to 1, a `review` key with a fault, a git older than
    /// 2.41, or an `--out` file that cannot be written.
    Write(Box<WriteArgs>),
    /// Prints the change record of a commit: the record that the commit keys,
    /// else the record whose range holds the commit. It prints a Markdown
    /// summary: the packet of the change, the route, the sample, the reviewers,
    /// and the post-merge audit. `--json` prints the record document
    /// `houserules/change-record@1.0.0` in place of it. It reads the notes ref of
    /// the install, and recomputes the rule hashes at the merge base and at the
    /// head from git. It exits 1 for a clone with no notes ref, for a commit with
    /// no record, and for a record whose rule hash differs from git's. It exits 2
    /// when the merge base of the record is not in the clone, and for a setup
    /// error, for example a ref that is no commit or a note that is no change
    /// record.
    Show(ShowArgs),
    /// Lists the change records of the install, one line for each record:
    /// `<key7> <route> <n> reviewers <sampled|-> <pass|fail|unaudited>`. The lines
    /// run by the committer date of the key of the record, newest first, and by
    /// sha for a tie. A record whose key the clone lacks has no date and comes
    /// last. `<n>` counts the reviewers of the record, those left out of its list
    /// included. `sampled` marks a record whose draw selected it for a post-merge
    /// audit, and the last word gives the verdict of that audit, `unaudited` when
    /// there is none. `<range>` keeps the records whose key is in the range, as
    /// `git rev-list` reads it; without it the command lists every record.
    /// `--sampled` keeps the sampled records. `--json` prints a list of one
    /// object for each record in place of the lines: `key` (the full sha),
    /// `route`, `reviewers`, `sampled` (`true` or `false`), and `audit` (`pass`,
    /// `fail`, or `unaudited`). A range reaches past the history of a shallow
    /// clone when the commits of one side that the other side does not reach (for
    /// a range of one revision, the commits it reaches) include a shallow commit,
    /// when an end is a full object name that the clone lacks, or when an end is a
    /// relative revision (`<rev>~<n>` or `<rev>^<n>`) that does not resolve while
    /// its `<rev>` does. A range with any other end that git cannot resolve, an
    /// abbreviated object name included, is a range that git cannot read. It
    /// exits 1 for a clone with no notes ref. It exits 2 for a range that git
    /// cannot read, for a range that reaches past the history of a shallow clone,
    /// and for a note that is no change record.
    List(list::ListArgs),
    /// Sets the post-merge audit of a record: the record that the commit `<sha>`
    /// keys, else the record whose range holds the commit. `--verdict` is `pass`
    /// or `fail`, `--by` names the person who audited, and `--note` adds a text.
    /// The command cuts `--by` to 100 bytes and `--note` to 1,000 bytes of UTF-8,
    /// each on a character boundary, and an empty `--note` leaves no note. It sets
    /// the audit in the notes ref of the clone and changes no other byte of the
    /// record; the project pushes the ref. It exits 1 for a clone with no notes
    /// ref, for a commit with no record, for a record that holds an audit already,
    /// and for an audit that would bring the record past `review.record.maxBytes`.
    /// It exits 2 for a setup error, for example: a ref that is no commit, an
    /// empty `--by`, or a note that is no change record.
    Audit(audit::AuditArgs),
}

/// The arguments of `houserules record write`.
#[derive(Args)]
pub(crate) struct WriteArgs {
    /// The base ref of the range. The range runs from the merge base of `--base`
    /// and `--head` to `--head`. git's empty tree starts the range at the first
    /// commit.
    #[arg(long, value_name = "REF")]
    base: String,
    /// The head ref of the range.
    #[arg(long, value_name = "REF")]
    head: String,
    /// The commit that the note annotates; defaults to `--head`. For a squash or
    /// a rebase merge it is the last commit of the push.
    #[arg(long, value_name = "SHA")]
    key: Option<String>,
    /// The file that `audit --json` wrote for the same range. Its `base` and
    /// `head` must name the same commits as `--base` and `--head`. Without this
    /// flag, the command runs the audit in process.
    #[arg(long, value_name = "FILE")]
    audit: Option<PathBuf>,
    /// The file that holds the description of the pull or merge request. The
    /// record keeps its evidence bundle. Without this flag, or with a description
    /// that has no bundle section, the record has no bundle.
    #[arg(long, value_name = "FILE")]
    bundle: Option<PathBuf>,
    /// The intent to add, written `<source>:<ref>` as the `Intent:` line of an
    /// evidence bundle writes it. The source is `work-item` or `file`. Without
    /// this flag, the `Intent:` line of the bundle names it.
    #[arg(long, value_name = "SOURCE:REF")]
    intent: Option<String>,
    /// The login of an approver; repeat the flag. The record keeps the names
    /// sorted and without repeats, the first 20 of them, each cut to 100
    /// characters, no more names than fit 3,072 bytes of the serialized array,
    /// and the count of the names left out.
    #[arg(long = "reviewer", value_name = "LOGIN")]
    reviewers: Vec<String>,
    /// Writes no reviewers and marks them pending: a later job may set them, or
    /// the approvers could not be read and stay unknown. It budgets the note
    /// 4,096 bytes below `review.record.maxBytes`, the room that a later job may
    /// fill with at most 3,072 bytes of names. The minimal record takes at most
    /// the rest at every cap. It cannot be combined with `--reviewer`.
    #[arg(long, conflicts_with = "reviewers")]
    reviewers_pending: bool,
    /// An external score from 0 to 1 for the route. It needs
    /// `review.route.ownerScore` or `review.route.seniorScore`.
    #[arg(long, value_name = "SCORE", allow_negative_numbers = true)]
    score: Option<f64>,
    /// The host that the record names: `github` or `gitlab`. Without this flag,
    /// the record names none.
    #[arg(long, value_enum)]
    host: Option<ci::Host>,
    /// Writes the bytes of the note to this file in place of the note.
    #[arg(long, value_name = "FILE")]
    out: Option<PathBuf>,
    /// Install root to read from; defaults to the nearest directory at or above
    /// the current one that holds `.houserules.json`, no higher than the git top
    /// level, else the git top level.
    #[arg(long)]
    dir: Option<PathBuf>,
}

/// The arguments of `houserules record show`.
#[derive(Args)]
pub(crate) struct ShowArgs {
    /// The commit to show: a commit that a record keys, or one in the range of a
    /// record.
    #[arg(value_name = "SHA")]
    sha: String,
    /// Prints the record document in place of the Markdown summary.
    #[arg(long)]
    json: bool,
    /// Install root to read from; defaults to the nearest directory at or above
    /// the current one that holds `.houserules.json`, no higher than the git top
    /// level, else the git top level.
    #[arg(long)]
    dir: Option<PathBuf>,
}

/// Why a command stopped: the exit code and the lines for stderr.
struct Failure {
    code: u8,
    lines: Vec<String>,
}

impl Failure {
    /// A refusal of the command: exit 1 with `lines`.
    fn refused(lines: Vec<String>) -> Self {
        Self { code: 1, lines }
    }
}

impl From<String> for Failure {
    /// A setup error: exit 2 with the line.
    fn from(line: String) -> Self {
        Self {
            code: 2,
            lines: vec![line],
        }
    }
}

impl From<Vec<String>> for Failure {
    /// A setup error: exit 2 with the lines.
    fn from(lines: Vec<String>) -> Self {
        Self { code: 2, lines }
    }
}

/// Runs `houserules record`: the subcommand that `args` names.
pub(crate) fn cmd_record(args: RecordArgs) -> ExitCode {
    let root = match crate::root::resolve_root(args.dir().clone()) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let result = match &args.action {
        RecordAction::Write(write) => run_write(&root, write),
        RecordAction::Show(show) => run_show(&root, show),
        RecordAction::List(list) => list::run(&root, list),
        RecordAction::Audit(audit) => audit::run(&root, audit),
    };
    match result {
        Ok(Done { stdout, stderr }) => {
            print!("{stdout}");
            for line in stderr {
                eprintln!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(failure) => {
            for line in &failure.lines {
                eprintln!("{line}");
            }
            ExitCode::from(failure.code)
        }
    }
}

/// What a command that succeeded prints.
struct Done {
    /// The text for stdout.
    stdout: String,
    /// The lines for stderr: the line of an audit that could not run, and the
    /// failure lines of the reference summary.
    stderr: Vec<String>,
}

// ---- record write ---------------------------------------------------------------------

/// The score as the route reads it, or the line for a score outside 0 to 1. A
/// score of `-0` reads as 0.
fn score_in_range(score: f64) -> Result<f64, String> {
    if (0.0..=1.0).contains(&score) {
        Ok(score.abs())
    } else {
        Err(format!("record: --score {score} is outside 0 to 1"))
    }
}

/// Reads `--intent`: `<source>:<ref>` with a source that is an adapter, else
/// the line for the intent that cannot be named.
fn parse_intent(text: &str) -> Result<IntentRef, String> {
    let named =
        IntentRef::parse(text).map_err(|line| format!("record: --intent {text}: {line}"))?;
    if Adapter::named(&named.source).is_none() {
        return Err(format!(
            "record: --intent {}:{}: {}",
            named.source,
            named.reference,
            no_intent_source(&named.source)
        ));
    }
    Ok(named)
}

/// The first bundle section of the description in `file`, parsed. `None` for a
/// description with no such section.
fn read_bundle(file: &Path) -> Result<Option<bundle::Bundle>, String> {
    let text = fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    Ok(bundle::sections(&text)
        .first()
        .map(|(_, section)| bundle::parse(section)))
}

/// The commits of the range of `analysis`, oldest first: those of the head that
/// the base does not reach, or every commit of the head for a range from the
/// empty tree.
fn commits_of(root: &Path, analysis: &change::Analysis) -> Result<Vec<String>, String> {
    let range = if analysis.merge_base.is_none() {
        analysis.head.clone()
    } else {
        format!("{}..{}", analysis.base, analysis.head)
    };
    let listing = git::run(
        root,
        &["rev-list", "--topo-order", "--reverse", &range, "--"],
    )?;
    Ok(listing.lines().map(str::to_string).collect())
}

/// The audit result of a range whose audit could not run: no row, no reference,
/// and counts of 0.
fn empty_audit(analysis: &change::Analysis) -> AuditResult {
    let base = git::short(&analysis.base).to_string();
    let head = git::short(&analysis.head).to_string();
    AuditResult {
        schema: published::id::<AuditResult>(),
        base: base.clone(),
        head: head.clone(),
        ids: Vec::new(),
        changed_files: Vec::new(),
        areas: Vec::new(),
        area_files: IndexMap::new(),
        rules: Vec::new(),
        summary: Summary {
            base,
            head,
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

/// The host of the record for the host that `--host` names.
fn host_of(host: ci::Host) -> Host {
    match host {
        ci::Host::Github => Host::Github,
        ci::Host::Gitlab => Host::Gitlab,
    }
}

/// Runs `record write`: computes the record, fits it, and stores it or writes
/// its bytes to `--out`.
fn run_write(root: &Path, args: &WriteArgs) -> Result<Done, Failure> {
    let score = args
        .score
        .map(score_in_range)
        .transpose()
        .map_err(Failure::from)?;
    let named = args
        .intent
        .as_deref()
        .map(parse_intent)
        .transpose()
        .map_err(Failure::from)?;
    let description = args
        .bundle
        .as_deref()
        .map(read_bundle)
        .transpose()
        .map_err(Failure::from)?
        .flatten();
    git::require_attr_source("record write")?;
    let base = load_base(root).map_err(|error| Failure::from(error.to_string()))?;
    let marker = Marker::read_or_default_for_gate(root, "WI")?;
    let config = marker.review()?;
    let analysis = change::analyze(root, &base, &config, &args.base, &args.head)?;
    let key = match &args.key {
        Some(key) => git::resolve_commit(root, key)?,
        None => analysis.head.clone(),
    };
    let packs = evidence::pack_pins_at(root, &analysis.install, &analysis.head)?;
    let (audit, audit_error) = match &args.audit {
        Some(file) => {
            let source = AuditSource::File(file.clone());
            (change::audit_rows(root, &base, source, &analysis)?, None)
        }
        None => match change::audit_rows(root, &base, AuditSource::InProcess, &analysis) {
            Ok(result) => (result, None),
            Err(line) => (empty_audit(&analysis), Some(line)),
        },
    };
    let coverage = change::coverage(&analysis, &audit, &base);
    let judged = audit_error.is_none().then_some((&audit, &coverage));
    let decision = route::decide(&analysis, judged, &config.route, score)?;
    let named = named.or_else(|| {
        description
            .as_ref()
            .and_then(|bundle| bundle.intent.as_deref())
            .and_then(|line| IntentRef::parse(line).ok())
    });
    let intents = intent::resolve(root, &analysis, &config.intent, named.as_ref())?;
    let sample = if decision.route == Route::AutoMergeEligible {
        let rate = config
            .route
            .sample_rate
            .expect("the review reader requires sampleRate for an auto-merge-eligible entry");
        Some(Sample {
            rate,
            sampled: sample::drawn(&key, rate),
        })
    } else {
        None
    };
    let evidence = evidence::build(&analysis, &coverage, &audit, None, intents, packs, decision);
    let facts = document::Facts {
        analysis: &analysis,
        evidence: &evidence,
        audit: &audit,
        audit_error: audit_error.clone(),
        commits: commits_of(root, &analysis)?,
        key: &key,
        host: args.host.map(host_of),
        sample,
        bundle: description.as_ref(),
        reviewers: document::Reviewers::of(&args.reviewers, args.reviewers_pending),
    };
    let mut record = document::build(&facts);
    if let Some(old) = notes::read_keyed(root, &analysis.install, &key)? {
        if old.rules.merge_base_hash != record.rules.merge_base_hash || old.head != record.head {
            return Err(Failure::refused(vec![format!(
                "record: {} already holds a record of another range",
                git::short(&key)
            )]));
        }
        record.post_merge_audit = old.post_merge_audit;
    }
    let reserve = if args.reviewers_pending {
        budget::PENDING_RESERVE
    } else {
        0
    };
    let record = budget::fit(record, config.max_bytes, reserve);
    let bytes = document::bytes(&record);
    match &args.out {
        Some(file) => fs::write(file, &bytes)
            .map_err(|error| Failure::from(format!("{}: {error}", file.display())))?,
        None => notes::write(root, &analysis.install, &key, &bytes)?,
    }
    let stderr = audit_error
        .into_iter()
        .chain(audit.references.failures.iter().cloned())
        .collect();
    Ok(Done {
        stdout: String::new(),
        stderr,
    })
}

// ---- record show ----------------------------------------------------------------------

/// The line for a hash that differs from git's: `claimed` is the hash of the
/// record, `given` the hash of git, `at` where the hash was read.
fn mismatch_line(
    record: &ChangeRecord,
    claimed: Option<&str>,
    given: Option<&str>,
    at: &str,
) -> String {
    let twelve = |hash: Option<&str>| {
        hash.map_or_else(
            || "none".to_string(),
            |hash| evidence::first_chars(hash, 12).to_string(),
        )
    };
    format!(
        "record: {} claims rules {} at {at}; git gives {}",
        git::short(&record.key),
        twelve(claimed),
        twelve(given)
    )
}

/// Checks the rule hashes of `record` against git. A merge base that is not in
/// the clone is a setup error; each hash that differs is a line of a refusal.
fn check_hashes(root: &Path, install: &str, record: &ChangeRecord) -> Result<(), Failure> {
    let mut lines = Vec::new();
    let checks_merge_base = record.rules.merge_base_rules == RulesPresence::Present.as_str();
    if let (true, Some(merge_base)) = (checks_merge_base, &record.merge_base) {
        if git::resolve_commit(root, merge_base).is_err() {
            return Err(Failure::from(notes::not_in_clone_line(merge_base)));
        }
        let given = rules_hash::rules_hash(root, install, merge_base)?;
        if given != record.rules.merge_base_hash {
            lines.push(mismatch_line(
                record,
                record.rules.merge_base_hash.as_deref(),
                given.as_deref(),
                "the merge base",
            ));
        }
    }
    let given = rules_hash::rules_hash(root, install, &record.head)?;
    if given.as_deref() != Some(record.rules.head_hash.as_str()) {
        lines.push(mismatch_line(
            record,
            Some(&record.rules.head_hash),
            given.as_deref(),
            "the head",
        ));
    }
    if lines.is_empty() {
        Ok(())
    } else {
        Err(Failure::refused(lines))
    }
}

/// The line of a reader whose clone lacks the notes ref of `install` (spec 8.1).
fn no_records_line(install: &str) -> String {
    let reference = notes::notes_ref(install);
    format!("no change records here; fetch them: git fetch origin {reference}:{reference}")
}

/// The records of the install at `root` whose key is in `range`, for a reader
/// that is no `record` command (`stats --records`). A clone without the notes
/// ref gets the line of spec 8.1 as the error, and a range that git cannot read
/// or that a shallow clone cannot walk to its end gets the line of
/// [`range::records`].
pub(crate) fn records_of(root: &Path, range: &str) -> Result<Vec<ChangeRecord>, String> {
    let install = change::install_path(root)?;
    if !notes::ref_exists(root, &install)? {
        return Err(no_records_line(&install));
    }
    Ok(range::records(root, &install, Some(range))?
        .into_iter()
        .map(|entry| entry.record)
        .collect())
}

/// Runs `record show`: finds the record of the commit, checks its rule hashes,
/// and prints it.
fn run_show(root: &Path, args: &ShowArgs) -> Result<Done, Failure> {
    let sha = git::resolve_commit(root, &args.sha)?;
    let install = change::install_path(root)?;
    if !notes::ref_exists(root, &install)? {
        return Err(Failure::refused(vec![no_records_line(&install)]));
    }
    let Some(record) = notes::read(root, &install, &sha)? else {
        return Err(Failure::refused(vec![format!(
            "record: no record for {}",
            git::short(&sha)
        )]));
    };
    check_hashes(root, &install, &record)?;
    let stdout = if args.json {
        emit(&serde_json::to_value(&record).expect("a change record serializes"))
    } else {
        document::markdown(&record)
    };
    Ok(Done {
        stdout,
        stderr: Vec::new(),
    })
}
