//! `houserules bundle`: the evidence bundle in the description of a pull or
//! merge request (spec 7).
//!
//! The bundle is a section of the description (spec 7.1). [`sections`] finds it
//! by one fence rule, the rule that the `Intent:` reader of the CI units reads
//! by too: a fenced block opens at a line that starts with three or more
//! backticks and holds no other backtick, and closes at the next line that
//! starts with at least as many backticks followed only by white space. A line
//! inside a fenced block never starts or ends a section or holds a part of the
//! bundle. [`parse`] reads the parts of one section.
//!
//! `bundle write` (spec 7.2) copies the header line of each capture and the
//! tail of its output into the section, and runs in any directory unless
//! `--intent` asks for a read of the install. `bundle check` (spec 7.3) runs no
//! audit; it reads the description from a file and prints one line for each
//! finding. Every line it prints, a setup error on stderr included, starts with
//! `bundle check: `, and each value from the description prints through
//! `review::text`, so no description text starts a line of a job log or changes
//! the layout of a line.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Subcommand};

use super::change::{self, Analysis};
use super::git;
use super::intent::{self, IntentRef, NO_SUCH_WORK_ITEM, Title};
use super::text::{code_span, escaped_line, fence_of};
use crate::capture::{self, Capture, Header, ReadError};
use crate::install::marker::Marker;
use crate::install::marker::review::{BundleConfig, IntentConfig};
use crate::published::intent::Intent;
use crate::rules::{compile, load_base};
use crate::shell;
use crate::tracker::{self, Lookup, Tracker};

/// The heading line that starts the section (spec 7.1).
const HEADING: &str = "## Evidence bundle";

/// The info string of the fenced block of a run (spec 7.1).
const RUN_INFO: &str = "houserules-run";

/// The fewest backticks that open a fenced block.
const FENCE_LEAST: usize = 3;

/// The start of every line that `bundle check` prints (spec 7.3).
const CHECK_PREFIX: &str = "bundle check: ";

/// The arguments of `houserules bundle`.
#[derive(Args)]
pub(crate) struct BundleArgs {
    /// What to do with the bundle.
    #[command(subcommand)]
    pub(crate) action: BundleAction,
}

/// The `--dir` of a command that takes none and reads the install of the
/// current directory.
static CURRENT_DIRECTORY: Option<PathBuf> = None;

impl BundleArgs {
    /// The `--dir` by which D4 finds the install that the subcommand reads: the
    /// one of `bundle check`, and no `--dir`, so the current directory, for a
    /// `bundle write` with an `--intent`. `None` for a `bundle write` that reads
    /// no install.
    pub(crate) fn dir(&self) -> Option<&Option<PathBuf>> {
        match &self.action {
            BundleAction::Write(write) => write.intent.as_ref().map(|_| &CURRENT_DIRECTORY),
            BundleAction::Check(check) => Some(&check.dir),
        }
    }
}

/// The subcommands of `houserules bundle`.
#[derive(Subcommand)]
pub(crate) enum BundleAction {
    /// Writes the evidence bundle section for the description of a pull or
    /// merge request, on stdout or to `--out`. Each `--capture` is a file that
    /// `houserules capture` wrote: the section copies its header line whole and
    /// the last `--tail` lines of its output, so no header passes through a
    /// model. `--purpose` is needed unless `--intent <source>:<ref>` names an
    /// intent whose adapter gives a why; the why is then the purpose. The
    /// command reads an `--intent` in the install of the current directory, at
    /// `HEAD`. `--budget-reason` adds the line that a diff above the budget of
    /// the tier needs. The command takes no `--dir`. The same inputs give the
    /// same bytes. It exits 2 for a capture that does not parse, an `--intent`
    /// that does not resolve, neither `--purpose` nor an `--intent` with a why,
    /// an empty `--tier` or `--criterion`, a `--purpose`, `--intent`, `--tier`,
    /// `--criterion`, or `--budget-reason` that holds a line break, a
    /// `--work-item` that is empty or holds a comma or a line break, or an
    /// `--out` file that cannot be written.
    Write(WriteArgs),
    /// Checks the evidence bundle in the description of a pull or merge
    /// request, read from `--body`, against the range `--base` to `--head`. It
    /// runs no audit. It prints one line for each finding, each line starts
    /// with `bundle check: `, and it exits 1 when it prints a finding: no
    /// section or more than one, a missing Purpose, Work items, Risk tier, or
    /// Criterion line, an intent or a work item that does not resolve, no run
    /// block, a run that has no capture header, that ran at another commit, in
    /// a dirty work tree, or with a failed exit, and a diff above the budget of
    /// the tier with no Budget reason line. A work item that cannot be
    /// resolved, because the install has no tracker or because its tracker
    /// cannot be reached, is counted in one line and fails nothing. An
    /// `--author` in `review.bundle.exemptAuthors`, or a `--head-ref` that
    /// matches a glob of `review.bundle.exemptBranches`, exempts the request.
    /// A clean bundle prints
    /// `bundle check: ok (<n> runs at <head7>, diff <lines> lines)`. With no
    /// `review.bundle.budgets` it checks no size. It exits 2 for a setup
    /// error, for example: a `--body` file that cannot be read, a ref that is
    /// no commit, a `review` key with a fault, or a git older than 2.41.
    Check(CheckArgs),
}

/// The arguments of `houserules bundle write`.
#[derive(Args)]
pub(crate) struct WriteArgs {
    /// One or two sentences on why the change exists. Needed unless `--intent`
    /// names an intent whose adapter gives a why.
    #[arg(long, value_name = "TEXT")]
    pub(crate) purpose: Option<String>,
    /// The intent to name, written `<source>:<ref>`. The source is `work-item`
    /// or `file`. The command reads it in the install of the current
    /// directory, at `HEAD`.
    #[arg(long, value_name = "SOURCE:REF")]
    pub(crate) intent: Option<String>,
    /// A work-item id; repeat the flag or give several values.
    #[arg(long = "work-item", value_name = "ID", required = true, num_args = 1..)]
    pub(crate) work_items: Vec<String>,
    /// The risk tier of the change.
    #[arg(long, value_name = "NAME")]
    pub(crate) tier: String,
    /// The criterion that selected the tier.
    #[arg(long, value_name = "TEXT")]
    pub(crate) criterion: String,
    /// Why the diff is above the budget of the tier; add it only then.
    #[arg(long, value_name = "TEXT")]
    pub(crate) budget_reason: Option<String>,
    /// A capture file that `houserules capture` wrote; repeat the flag or give
    /// several files.
    #[arg(long = "capture", value_name = "FILE", required = true, num_args = 1..)]
    pub(crate) captures: Vec<PathBuf>,
    /// The number of lines from the end of each output to copy; defaults to 20.
    #[arg(
        long,
        value_name = "N",
        default_value_t = 20,
        hide_default_value = true
    )]
    pub(crate) tail: usize,
    /// Writes the section to this file in place of stdout.
    #[arg(long, value_name = "FILE")]
    pub(crate) out: Option<PathBuf>,
}

/// The arguments of `houserules bundle check`.
#[derive(Args)]
pub(crate) struct CheckArgs {
    /// The file that holds the description of the pull or merge request.
    #[arg(long, value_name = "FILE")]
    pub(crate) body: PathBuf,
    /// The base ref of the range. The range runs from the merge base of
    /// `--base` and `--head` to `--head`.
    #[arg(long, value_name = "REF")]
    pub(crate) base: String,
    /// The head ref of the range. Each run of the bundle must have run at this
    /// commit.
    #[arg(long, value_name = "REF")]
    pub(crate) head: String,
    /// The login of the author of the request; an author in
    /// `review.bundle.exemptAuthors` needs no bundle.
    #[arg(long, value_name = "LOGIN")]
    pub(crate) author: Option<String>,
    /// The source branch of the request; a branch that matches a glob of
    /// `review.bundle.exemptBranches` needs no bundle.
    #[arg(long, value_name = "BRANCH")]
    pub(crate) head_ref: Option<String>,
    /// Install root to read from; defaults to the nearest directory at or
    /// above the current one that holds `.houserules.json`, no higher than the
    /// git top level, else the git top level.
    #[arg(long)]
    pub(crate) dir: Option<PathBuf>,
}

// ---- the fence rule and the section -------------------------------------------------

/// `true` for the white space of the section rules: space, tab, line feed,
/// vertical tab, form feed, and carriage return, the class `[[:space:]]` of the
/// units' reader.
fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0b' | '\x0c' | '\r')
}

/// What the fence rule of spec 7.1 makes of one line of a description.
#[derive(Debug, PartialEq, Eq)]
enum Line<'a> {
    /// A line outside every fenced block.
    Text,
    /// The line that opens a fenced block, with its info string.
    Opens(&'a str),
    /// A line inside a fenced block.
    Inside,
    /// The line that closes a fenced block.
    Closes,
}

/// The fence rule of spec 7.1 over the lines of a description, one line at a
/// time and in order.
#[derive(Default)]
struct Fences {
    /// The number of backticks that opened the block the lines are in; 0 outside
    /// every block.
    open: usize,
}

impl Fences {
    /// The kind of `line`, and the state for the next line.
    fn step<'a>(&mut self, line: &'a str) -> Line<'a> {
        let ticks = line.len() - line.trim_start_matches('`').len();
        let rest = &line[ticks..];
        if self.open > 0 {
            if ticks >= self.open && rest.chars().all(is_space) {
                self.open = 0;
                return Line::Closes;
            }
            return Line::Inside;
        }
        if ticks >= FENCE_LEAST && !rest.contains('`') {
            self.open = ticks;
            return Line::Opens(rest.trim_matches(is_space));
        }
        Line::Text
    }
}

/// `true` for the heading line of the section: `## Evidence bundle` and white
/// space.
fn is_heading(line: &str) -> bool {
    line.strip_prefix(HEADING)
        .is_some_and(|rest| rest.chars().all(is_space))
}

/// The sections of the description `body` (spec 7.1), in order: the zero-based
/// number of the line of the heading, and the text from the heading line to the
/// next line that starts with `## ` outside a fenced block, or to the end of the
/// description. The fence state runs through the whole description, so a heading
/// inside a fenced block of another section starts no section.
pub(crate) fn sections(body: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let mut fences = Fences::default();
    let mut open: Option<(usize, usize)> = None;
    let mut start = 0;
    for (number, line) in body.split('\n').enumerate() {
        if matches!(fences.step(line), Line::Text) && line.starts_with("## ") {
            if let Some((first, from)) = open.take() {
                found.push((first, &body[from..start]));
            }
            if is_heading(line) {
                open = Some((number, start));
            }
        }
        start += line.len() + 1;
    }
    if let Some((first, from)) = open {
        found.push((first, &body[from..]));
    }
    found
}

// ---- the parts of a section ---------------------------------------------------------

/// The parts of a section (spec 7.1). A part the section lacks is empty.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Bundle {
    /// The text of the `Purpose:` line.
    pub(crate) purpose: String,
    /// The text of the `Intent:` line, written `<source>:<ref>`; `None` for a
    /// section with no such line or an empty one.
    pub(crate) intent: Option<String>,
    /// The ids of the `Work items:` line, split at the commas, each once, in the
    /// order of the line.
    pub(crate) work_items: Vec<String>,
    /// The text of the `Risk tier:` line.
    pub(crate) tier: String,
    /// The text of the `Criterion:` line.
    pub(crate) criterion: String,
    /// The text of the `Budget reason:` line.
    pub(crate) budget_reason: String,
    /// The fenced blocks with the info string `houserules-run`, in order.
    pub(crate) runs: Vec<BundleRun>,
}

/// One run of a bundle: a fenced block that starts with the header line of a
/// capture.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BundleRun {
    /// The first line of the block, without a carriage return; empty for a block
    /// with no line.
    pub(crate) header_line: String,
}

impl BundleRun {
    /// The run of the block whose lines are `lines`.
    fn of(lines: &[&str]) -> Self {
        Self {
            header_line: lines
                .first()
                .map(|line| (*line).to_string())
                .unwrap_or_default(),
        }
    }

    /// The capture header in the first line, or `None` when the line is no
    /// capture header.
    pub(crate) fn header(&self) -> Option<Header> {
        capture::parse_header(self.header_line.as_bytes()).ok()
    }
}

/// The text after each label in a section, the first one found for each.
#[derive(Default)]
struct Parts {
    purpose: Option<String>,
    intent: Option<String>,
    work_items: Option<String>,
    tier: Option<String>,
    criterion: Option<String>,
    budget_reason: Option<String>,
}

impl Parts {
    /// Takes the text after the label that `line` starts with, if any, unless the
    /// label has its text already. A label counts at the start of the line only.
    fn read(&mut self, line: &str) {
        let slots = [
            ("Purpose:", &mut self.purpose),
            ("Intent:", &mut self.intent),
            ("Work items:", &mut self.work_items),
            ("Risk tier:", &mut self.tier),
            ("Criterion:", &mut self.criterion),
            ("Budget reason:", &mut self.budget_reason),
        ];
        for (label, slot) in slots {
            if let Some(text) = line.strip_prefix(label) {
                slot.get_or_insert_with(|| text.trim_matches(is_space).to_string());
                return;
            }
        }
    }
}

/// The ids of a `Work items:` line: the texts between the commas, without white
/// space around them, without the empty ones and the repeats.
fn work_item_ids(line: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for id in line.split(',').map(|id| id.trim_matches(is_space)) {
        if !id.is_empty() && !ids.iter().any(|seen| seen == id) {
            ids.push(id.to_string());
        }
    }
    ids
}

/// The parts of `section`, a text from [`sections`]. A line that ends with a
/// carriage return reads like the same line without it. A label counts outside
/// every fenced block only, and the first line of a label wins. A fenced block
/// with the info string `houserules-run` that nothing closes runs to the end of
/// the section.
pub(crate) fn parse(section: &str) -> Bundle {
    let mut parts = Parts::default();
    let mut fences = Fences::default();
    let mut block: Option<Vec<&str>> = None;
    let mut runs = Vec::new();
    for line in section.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match fences.step(line) {
            Line::Opens(info) => block = (info == RUN_INFO).then(Vec::new),
            Line::Inside => {
                if let Some(lines) = &mut block {
                    lines.push(line);
                }
            }
            Line::Closes => {
                if let Some(lines) = block.take() {
                    runs.push(BundleRun::of(&lines));
                }
            }
            Line::Text => parts.read(line),
        }
    }
    if let Some(lines) = block {
        runs.push(BundleRun::of(&lines));
    }
    Bundle {
        purpose: parts.purpose.unwrap_or_default(),
        intent: parts.intent.filter(|text| !text.is_empty()),
        work_items: work_item_ids(&parts.work_items.unwrap_or_default()),
        tier: parts.tier.unwrap_or_default(),
        criterion: parts.criterion.unwrap_or_default(),
        budget_reason: parts.budget_reason.unwrap_or_default(),
        runs,
    }
}

// ---- the intent ---------------------------------------------------------------------

/// The entry that [`intent::resolve`] gives for `named`, whatever the config
/// lists: the adapter of its source resolves it (spec 4.9). `analysis` has no
/// work items, so no other intent is read. A failure that is not the named
/// intent's own is the `Err` with its line.
fn resolve_one(root: &Path, analysis: &Analysis, named: &IntentRef) -> Result<Intent, String> {
    intent::resolve(root, analysis, &IntentConfig::default(), Some(named))?
        .into_iter()
        .find(|found| found.source == named.source && found.reference == named.reference)
        .ok_or_else(|| {
            format!(
                "no entry for the intent {}:{}",
                named.source, named.reference
            )
        })
}

// ---- bundle write -------------------------------------------------------------------

/// The line of `bundle write` for a flag whose value holds a line break.
fn one_line(flag: &str, value: &str) -> Result<(), String> {
    if value.contains(['\n', '\r']) {
        Err(format!("bundle write: {flag} must be one line"))
    } else {
        Ok(())
    }
}

/// A capture, read for the section: the header line and the tail of the output.
struct Copied {
    /// The first line of the capture file.
    header: String,
    /// The last lines of the output.
    tail: Vec<String>,
}

impl Copied {
    /// The header line and the last `tail` lines of the output of `capture`.
    fn of(capture: &Capture, tail: usize) -> Self {
        let header = capture
            .bytes
            .split(|byte| *byte == b'\n')
            .next()
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .unwrap_or_default();
        let output = String::from_utf8_lossy(capture.output());
        let lines: Vec<&str> = output.lines().collect();
        let skip = lines.len().saturating_sub(tail);
        Self {
            header,
            tail: lines[skip..]
                .iter()
                .map(|line| (*line).to_string())
                .collect(),
        }
    }

    /// The fenced block of the run (spec 7.1). The fence is longer than every
    /// backtick run in the block, and the info string follows it. The header
    /// line stays as the capture holds it. The block holds one line for each
    /// line of the tail, an empty line included: `review::text` writes a
    /// control character of a line as a `\u` escape, a lone carriage return
    /// among them, so no line of the tail ends another.
    fn block(&self) -> String {
        let tail: Vec<String> = self.tail.iter().map(|line| escaped_line(line)).collect();
        let mut lines = vec![self.header.as_str()];
        lines.extend(tail.iter().map(String::as_str));
        let fence = fence_of(&lines.join("\n"));
        let mut block = format!("{fence}{RUN_INFO}\n{}\n", self.header);
        for line in &tail {
            block.push_str(line);
            block.push('\n');
        }
        block.push_str(&fence);
        block.push('\n');
        block
    }
}

/// The arguments of `bundle write` that it checked before it reads the install.
struct Prepared {
    /// The `Purpose:` text, if the flag holds one.
    purpose: Option<String>,
    /// The intent that `--intent` names.
    intent: Option<IntentRef>,
    /// The `Work items:` ids, each without white space around it.
    work_items: Vec<String>,
    /// The tier, without white space around it.
    tier: String,
    /// The criterion, without white space around it.
    criterion: String,
    /// The `Budget reason:` text, if the flag holds one.
    budget_reason: Option<String>,
    /// The runs, in the order of the flags.
    runs: Vec<Copied>,
}

/// Checks the arguments of `bundle write` and reads its captures. Every capture
/// that does not parse has its line; any other fault is the one line of the first.
fn prepare(args: &WriteArgs) -> Result<Prepared, Vec<String>> {
    let one = |line: String| vec![line];
    if args
        .work_items
        .iter()
        .any(|id| id.trim().is_empty() || id.contains([',', '\n', '\r']))
    {
        return Err(one(
            "bundle write: --work-item holds an empty id, a comma, or a line break".to_string(),
        ));
    }
    let (tier, criterion) = (args.tier.trim(), args.criterion.trim());
    for (flag, value) in [("--tier", tier), ("--criterion", criterion)] {
        if value.is_empty() {
            return Err(one(format!("bundle write: {flag} is empty")));
        }
    }
    let purpose = non_blank(args.purpose.as_deref());
    let budget_reason = non_blank(args.budget_reason.as_deref());
    let intent_text = args.intent.as_deref().map(str::trim);
    for (flag, value) in [
        ("--purpose", purpose),
        ("--intent", intent_text),
        ("--tier", Some(tier)),
        ("--criterion", Some(criterion)),
        ("--budget-reason", budget_reason),
    ] {
        one_line(flag, value.unwrap_or_default()).map_err(one)?;
    }
    let intent = intent_text
        .map(|text| {
            IntentRef::parse(text).map_err(|line| format!("bundle write: --intent {text}: {line}"))
        })
        .transpose()
        .map_err(one)?;
    let mut runs = Vec::new();
    let mut faults = Vec::new();
    for path in &args.captures {
        match capture::read(path) {
            Ok(capture) => runs.push(Copied::of(&capture, args.tail)),
            Err(ReadError::Missing) => {
                faults.push(format!("bundle write: {} does not exist", path.display()));
            }
            Err(ReadError::Unreadable(error)) => faults.push(format!(
                "bundle write: {} could not be read ({error})",
                path.display()
            )),
            Err(ReadError::NoCapture(reason)) => faults.push(format!(
                "bundle write: {} is no capture ({reason})",
                path.display()
            )),
        }
    }
    if !faults.is_empty() {
        return Err(faults);
    }
    Ok(Prepared {
        purpose: purpose.map(str::to_string),
        intent,
        work_items: args
            .work_items
            .iter()
            .map(|id| id.trim().to_string())
            .collect(),
        tier: tier.to_string(),
        criterion: criterion.to_string(),
        budget_reason: budget_reason.map(str::to_string),
        runs,
    })
}

/// `text` without white space around it, or `None` when nothing is left.
fn non_blank(text: Option<&str>) -> Option<&str> {
    text.map(str::trim).filter(|text| !text.is_empty())
}

/// The intent `named` as the install at `root` resolves it at `HEAD`. The
/// marker is read so that an install of houserules 1.x is refused (D4); the
/// intent needs the commit and the tracker, and no change analysis. An intent
/// that does not resolve is the `Err` with the adapter's line.
fn resolve_at_head(root: &Path, named: &IntentRef) -> Result<Intent, Vec<String>> {
    let one = |line: String| vec![line];
    Marker::read_or_default_for_gate(root, "WI").map_err(one)?;
    let head = git::resolve_commit(root, "HEAD").map_err(one)?;
    let found = intent::resolve_named(root, &head, named).map_err(one)?;
    match &found.unresolved {
        Some(line) => Err(one(format!(
            "bundle write: --intent {}:{}: {line}",
            named.source, named.reference
        ))),
        None => Ok(found),
    }
}

/// The section of spec 7.1 for `prepared`, with the `Purpose:` line left out
/// when the intent's why is the purpose. The `Err` is the line of spec 7.2 for a
/// section with neither a purpose nor an intent with a why.
fn section_text(prepared: &Prepared, intent: Option<&Intent>) -> Result<String, String> {
    let why_given = intent
        .and_then(|found| found.why.as_deref())
        .is_some_and(|why| !why.trim().is_empty());
    if prepared.purpose.is_none() && !why_given {
        return Err("bundle write: --purpose is needed unless --intent names an intent whose adapter gives a why".to_string());
    }
    let mut text = format!("{HEADING}\n\n");
    if let Some(purpose) = &prepared.purpose {
        text.push_str(&format!("Purpose: {purpose}\n"));
    }
    if let Some(named) = &prepared.intent {
        text.push_str(&format!("Intent: {}:{}\n", named.source, named.reference));
    }
    text.push_str(&format!("Work items: {}\n", prepared.work_items.join(", ")));
    text.push_str(&format!("Risk tier: {}\n", prepared.tier));
    text.push_str(&format!("Criterion: {}\n", prepared.criterion));
    if let Some(reason) = &prepared.budget_reason {
        text.push_str(&format!("Budget reason: {reason}\n"));
    }
    for run in &prepared.runs {
        text.push('\n');
        text.push_str(&run.block());
    }
    Ok(text)
}

/// Prints each line of `lines` on stderr and gives exit 2.
fn fail(lines: &[String]) -> ExitCode {
    for line in lines {
        eprintln!("{line}");
    }
    ExitCode::from(2)
}

/// Runs `houserules bundle write`: prints the section, or writes it to `--out`.
/// A setup error is one line on stderr for each fault and exit 2
/// (`houserules.crash-paths-are-named`).
fn cmd_write(args: &WriteArgs) -> ExitCode {
    let prepared = match prepare(args) {
        Ok(prepared) => prepared,
        Err(lines) => return fail(&lines),
    };
    let intent = match &prepared.intent {
        None => None,
        Some(named) => {
            let root = match crate::root::resolve_root(None) {
                Ok(root) => root,
                Err(code) => return code,
            };
            match resolve_at_head(&root, named) {
                Ok(found) => Some(found),
                Err(lines) => return fail(&lines),
            }
        }
    };
    let text = match section_text(&prepared, intent.as_ref()) {
        Ok(text) => text,
        Err(line) => return fail(&[line]),
    };
    match &args.out {
        Some(file) => {
            if let Err(error) = fs::write(file, text) {
                return fail(&[format!("bundle write: {}: {error}", file.display())]);
            }
        }
        None => print!("{text}"),
    }
    ExitCode::SUCCESS
}

// ---- bundle check -------------------------------------------------------------------

/// What `bundle check` prints, and whether a line of it is a finding.
#[derive(Debug, Default)]
struct Outcome {
    /// The lines, each with its prefix.
    lines: Vec<String>,
    /// `true` once a finding is among the lines.
    failed: bool,
}

impl Outcome {
    /// A line that fails the check.
    fn finding(&mut self, text: impl AsRef<str>) {
        self.lines.push(format!("{CHECK_PREFIX}{}", text.as_ref()));
        self.failed = true;
    }

    /// A line that tells and fails nothing.
    fn notice(&mut self, text: impl AsRef<str>) {
        self.lines.push(format!("{CHECK_PREFIX}{}", text.as_ref()));
    }

    /// The text of stdout, and the exit code.
    fn into_output(self) -> (String, u8) {
        let text = self.lines.iter().map(|line| format!("{line}\n")).collect();
        (text, u8::from(self.failed))
    }
}

/// What the `Intent:` line of a bundle names.
enum NamedIntent {
    /// The bundle has no `Intent:` line.
    Absent,
    /// The adapter does not resolve it: the text to print for it and the
    /// adapter's line.
    Unresolved { label: String, line: String },
    /// The adapter resolves it; `has_why` says whether the intent gives a why,
    /// which then stands in for the purpose.
    Resolved { has_why: bool },
}

/// What the adapter of the `Intent:` line `text` makes of it.
fn name_intent(root: &Path, analysis: &Analysis, text: &str) -> Result<NamedIntent, String> {
    let named = match IntentRef::parse(text) {
        Ok(named) => named,
        Err(line) => {
            return Ok(NamedIntent::Unresolved {
                label: text.to_string(),
                line,
            });
        }
    };
    let found = resolve_one(root, analysis, &named)?;
    Ok(match found.unresolved.clone() {
        Some(line) => NamedIntent::Unresolved {
            label: format!("{}:{}", named.source, named.reference),
            line,
        },
        None => NamedIntent::Resolved {
            has_why: found
                .why
                .as_deref()
                .is_some_and(|why| !why.trim().is_empty()),
        },
    })
}

/// The findings for run `index` (counted from 1) of the bundle, which must have
/// run at `head`. The lines come in the order of spec 7.3: the header, the
/// commit, the work tree, the exit.
fn check_run(index: usize, run: &BundleRun, head: &str, outcome: &mut Outcome) {
    let Some(header) = run.header() else {
        outcome.finding(format!("run {index}: the first line is no capture header"));
        return;
    };
    let who = format!("run {index} ({})", code_span(&shell::join(&header.argv)));
    match header.head.as_deref() {
        Some(ran) if ran == head => {}
        Some(ran) => outcome.finding(format!(
            "{who} ran at {}, not at {}",
            code_span(git::short(ran)),
            git::short(head)
        )),
        None => outcome.finding(format!("{who} ran at none, not at {}", git::short(head))),
    }
    if header.dirty == Some(true) {
        outcome.finding(format!("{who} ran in a dirty work tree"));
    }
    match (header.exit, header.signal) {
        (Some(0), _) => {}
        (Some(code), _) => outcome.finding(format!("{who} exited {code}")),
        (None, Some(signal)) => outcome.finding(format!("{who} ended by signal {signal}")),
        (None, None) => outcome.finding(format!("{who} ended with no exit code")),
    }
}

/// The findings for the work items `ids` of the bundle (spec 7.3). The tracker
/// gets one call for each work item that it is asked about. A work item that
/// the tracker does not hold is a finding. Once the tracker cannot be reached,
/// it is asked no more, as the `work-item` adapter of `intent` does. The work
/// item that found it out of reach and every later one are counted in one line
/// with the line of the tracker, and that line fails nothing (spec 4.9). Any
/// other failure of the tracker is the `Err`.
fn check_work_items(
    tracker: &dyn Tracker,
    ids: &[String],
    outcome: &mut Outcome,
) -> Result<(), String> {
    for (at, id) in ids.iter().enumerate() {
        match intent::title_of(tracker, id)? {
            Title::Found(_) => {}
            Title::Missing => {
                outcome.finding(format!("work item {}: {NO_SUCH_WORK_ITEM}", code_span(id)));
            }
            Title::Unreachable(line) => {
                outcome.notice(format!(
                    "{} work items not resolved: {line}",
                    ids.len() - at
                ));
                break;
            }
        }
    }
    Ok(())
}

/// The findings of the diff budget (spec 7.3): the line without budgets, a tier
/// with no budget, and a diff above the budget of the tier with no reason.
fn check_budget(bundle: &Bundle, config: &BundleConfig, diff_lines: u64, outcome: &mut Outcome) {
    if config.budgets.is_empty() {
        outcome.notice("no diff budgets configured (review.bundle.budgets)");
        return;
    }
    if bundle.tier.is_empty() {
        return;
    }
    match config.budgets.get(&bundle.tier) {
        None => {
            let keys: Vec<String> = config.budgets.keys().map(|key| code_span(key)).collect();
            outcome.finding(format!(
                "risk tier \"{}\" has no budget; review.bundle.budgets names {}",
                code_span(&bundle.tier),
                keys.join(", ")
            ));
        }
        Some(&budget) if diff_lines > budget && bundle.budget_reason.is_empty() => {
            outcome.finding(format!(
                "diff {diff_lines} lines > {} budget {budget}; add a Budget reason line",
                code_span(&bundle.tier)
            ));
        }
        Some(_) => {}
    }
}

/// The lines of `bundle check` for the description `body` (spec 7.3). The
/// `Err` is a failure that is not a finding: the tracker or the marker cannot
/// be read.
fn check_description(
    root: &Path,
    analysis: &Analysis,
    config: &BundleConfig,
    body: &str,
) -> Result<Outcome, String> {
    let mut outcome = Outcome::default();
    let all = sections(body);
    let section = match all.as_slice() {
        [] => {
            outcome.finding("no \"## Evidence bundle\" section");
            return Ok(outcome);
        }
        [(_, section)] => *section,
        many => {
            outcome.finding(format!(
                "{} \"## Evidence bundle\" sections; keep one",
                many.len()
            ));
            return Ok(outcome);
        }
    };
    let bundle = parse(section);
    let alone = Analysis {
        work_items: Vec::new(),
        ..analysis.clone()
    };
    let intent = match &bundle.intent {
        Some(text) => name_intent(root, &alone, text)?,
        None => NamedIntent::Absent,
    };
    let why_given = matches!(intent, NamedIntent::Resolved { has_why: true });

    if bundle.purpose.is_empty() && !why_given {
        outcome.finding("no Purpose line");
    }
    if bundle.work_items.is_empty() {
        outcome.finding("no Work items line");
    }
    if bundle.tier.is_empty() {
        outcome.finding("no Risk tier line");
    }
    if bundle.criterion.is_empty() {
        outcome.finding("no Criterion line");
    }
    if let NamedIntent::Unresolved { label, line } = &intent {
        outcome.finding(format!("intent {}: {line}", code_span(label)));
    }
    if !bundle.work_items.is_empty() {
        match tracker::lookup(root).map_err(|error| error.to_string())? {
            Lookup::Absent(_) => outcome.notice(format!(
                "{} work items not resolved: no tracker (run houserules init --tracker <name>)",
                bundle.work_items.len()
            )),
            Lookup::Installed(tracker) => {
                check_work_items(tracker.as_ref(), &bundle.work_items, &mut outcome)?;
            }
        }
    }
    if bundle.runs.is_empty() {
        outcome.finding("no houserules-run block");
    }
    for (at, run) in bundle.runs.iter().enumerate() {
        check_run(at + 1, run, &analysis.head, &mut outcome);
    }
    check_budget(&bundle, config, analysis.diff.lines, &mut outcome);
    if !outcome.failed {
        outcome.notice(format!(
            "ok ({} runs at {}, diff {} lines)",
            bundle.runs.len(),
            git::short(&analysis.head),
            analysis.diff.lines
        ));
    }
    Ok(outcome)
}

/// The author or the branch that the config exempts, if any (spec 7.3). An
/// empty value is no value: a unit passes one when the host gives none.
fn exempt(
    config: &BundleConfig,
    author: Option<&str>,
    head_ref: Option<&str>,
) -> Result<Option<String>, String> {
    if let Some(author) = author.filter(|login| !login.is_empty())
        && config.exempt_authors.iter().any(|login| login == author)
    {
        return Ok(Some(author.to_string()));
    }
    if let Some(branch) = head_ref.filter(|name| !name.is_empty()) {
        for glob in &config.exempt_branches {
            let matcher = compile(glob).map_err(|error| error.to_string())?;
            if matcher.is_match(branch) {
                return Ok(Some(branch.to_string()));
            }
        }
    }
    Ok(None)
}

/// The lines of a setup error of `bundle check` for the texts `texts`: every
/// line of every text starts with the prefix of the command (spec 7.3).
fn setup_lines(texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .flat_map(|text| text.split('\n'))
        .map(|line| format!("{CHECK_PREFIX}{line}"))
        .collect()
}

/// The text and the exit code of `bundle check`, or the lines of a setup error.
fn run_check(root: &Path, args: &CheckArgs) -> Result<(String, u8), Vec<String>> {
    let setup = |text: String| setup_lines(&[text]);
    let marker = Marker::read_or_default_for_gate(root, "WI").map_err(setup)?;
    let config = marker.review().map_err(|lines| setup_lines(&lines))?;
    let author = args.author.as_deref();
    let head_ref = args.head_ref.as_deref();
    if let Some(who) = exempt(&config.bundle, author, head_ref).map_err(setup)? {
        let mut outcome = Outcome::default();
        outcome.notice(format!("{} is exempt", code_span(&who)));
        return Ok(outcome.into_output());
    }
    let body = fs::read_to_string(&args.body)
        .map_err(|error| setup(format!("{}: {error}", args.body.display())))?;
    git::require_attr_source("bundle check").map_err(|line| vec![line])?;
    let base = load_base(root).map_err(|error| setup(error.to_string()))?;
    let analysis = change::analyze(root, &base, &config, &args.base, &args.head).map_err(setup)?;
    let outcome = check_description(root, &analysis, &config.bundle, &body).map_err(setup)?;
    Ok(outcome.into_output())
}

/// Runs `houserules bundle check`: prints the findings and exits 1 when there is
/// one. A setup error is one line on stderr for each fault and exit 2
/// (`houserules.crash-paths-are-named`).
fn cmd_check(args: &CheckArgs) -> ExitCode {
    let root = match crate::root::resolve_root(args.dir.clone()) {
        Ok(root) => root,
        Err(code) => return code,
    };
    match run_check(&root, args) {
        Ok((text, code)) => {
            print!("{text}");
            ExitCode::from(code)
        }
        Err(lines) => fail(&lines),
    }
}

/// Runs `houserules bundle`: the subcommand that `args` names.
pub(crate) fn cmd_bundle(args: BundleArgs) -> ExitCode {
    match args.action {
        BundleAction::Write(write) => cmd_write(&write),
        BundleAction::Check(check) => cmd_check(&check),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use serde_json::{Value, json};

    use super::*;
    use crate::tracker::{DeferRequest, ListOpts, Resolution, Tracker, TrackerError, lines};

    /// The fence states of `lines` in order.
    fn kinds<'a>(lines: &[&'a str]) -> Vec<Line<'a>> {
        let mut fences = Fences::default();
        lines.iter().map(|line| fences.step(line)).collect()
    }

    #[test]
    fn a_fence_opens_at_three_backticks_and_holds_no_other() {
        assert_eq!(
            kinds(&["```", "x", "```"]),
            [Line::Opens(""), Line::Inside, Line::Closes]
        );
        assert_eq!(
            kinds(&["```houserules-run  ", "x", "```"]),
            [Line::Opens("houserules-run"), Line::Inside, Line::Closes]
        );
        assert_eq!(kinds(&["``", "```a`b"]), [Line::Text, Line::Text]);
        assert_eq!(kinds(&["```inline```"]), [Line::Text]);
        assert_eq!(kinds(&[" ```"]), [Line::Text]);
    }

    #[test]
    fn a_fence_closes_at_a_run_at_least_as_long_followed_by_white_space_only() {
        assert_eq!(
            kinds(&["````", "```", "```` x", "`````  \t", "after"]),
            [
                Line::Opens(""),
                Line::Inside,
                Line::Inside,
                Line::Closes,
                Line::Text
            ]
        );
    }

    #[test]
    fn a_carriage_return_ends_a_closing_fence_and_a_heading() {
        assert_eq!(kinds(&["```", "```\r"]), [Line::Opens(""), Line::Closes]);
        assert!(is_heading("## Evidence bundle\r"));
        assert!(is_heading("## Evidence bundle \t"));
        assert!(is_heading("## Evidence bundle\x0b\x0c"));
        assert!(!is_heading("## Evidence bundle\u{a0}"));
        assert!(!is_heading("## Evidence bundle2"));
        assert!(!is_heading("### Evidence bundle"));
        assert!(!is_heading(" ## Evidence bundle"));
    }

    #[test]
    fn a_section_runs_from_its_heading_to_the_next_heading_outside_a_fence() {
        let body =
            "Intro\n## Evidence bundle\nPurpose: p\n```\n## Evidence bundle\n```\n## Other\ntext\n";

        let found = sections(body);

        assert_eq!(
            found,
            [(
                1,
                "## Evidence bundle\nPurpose: p\n```\n## Evidence bundle\n```\n"
            )]
        );
    }

    #[test]
    fn a_section_runs_to_the_end_of_the_description() {
        assert_eq!(
            sections("## Evidence bundle\nPurpose: p"),
            [(0, "## Evidence bundle\nPurpose: p")]
        );
        assert_eq!(sections(""), []);
        assert_eq!(sections("## Evidence bundle"), [(0, "## Evidence bundle")]);
    }

    #[test]
    fn a_second_heading_starts_a_second_section() {
        assert_eq!(
            sections("## Evidence bundle\na\n## Evidence bundle\nb\n"),
            [
                (0, "## Evidence bundle\na\n"),
                (2, "## Evidence bundle\nb\n")
            ]
        );
    }

    #[test]
    fn a_heading_in_a_fence_that_nothing_closes_starts_no_section() {
        assert_eq!(sections("## Notes\n```\n## Evidence bundle\n"), []);
    }

    #[test]
    fn a_label_counts_at_the_start_of_a_line_outside_a_fence_and_the_first_wins() {
        let bundle = parse(
            "## Evidence bundle\n\nPurpose: first\n Purpose: indented\nPurpose: second\nIntent: file:a.md\n```\nCriterion: inside\n```\nCriterion:   out  \nRisk tier: light\nBudget reason: because\n",
        );

        assert_eq!(bundle.purpose, "first");
        assert_eq!(bundle.intent.as_deref(), Some("file:a.md"));
        assert_eq!(bundle.criterion, "out");
        assert_eq!(bundle.tier, "light");
        assert_eq!(bundle.budget_reason, "because");
    }

    #[test]
    fn a_part_that_is_missing_or_empty_is_empty() {
        let bundle = parse("## Evidence bundle\nPurpose:\nIntent:   \n");

        assert_eq!(bundle, Bundle::default());
    }

    #[test]
    fn the_work_items_split_at_the_commas_and_name_each_id_once() {
        let bundle = parse("## Evidence bundle\nWork items:  HR-1 ,HR-2,, HR-1 ,\n");

        assert_eq!(bundle.work_items, ["HR-1", "HR-2"]);
    }

    #[test]
    fn a_run_is_a_block_with_the_info_string_houserules_run() {
        let bundle = parse(
            "## Evidence bundle\n```houserules-run\nfirst header\ntail\n```\n```text\nnot a run\n```\n````houserules-run\nsecond header\n````\n",
        );

        let headers: Vec<&str> = bundle
            .runs
            .iter()
            .map(|run| run.header_line.as_str())
            .collect();
        assert_eq!(headers, ["first header", "second header"]);
    }

    #[test]
    fn a_run_that_nothing_closes_runs_to_the_end_and_an_empty_run_has_no_header() {
        let open = parse("## Evidence bundle\n```houserules-run\nheader\nmore\n");
        let empty = parse("## Evidence bundle\n```houserules-run\n```\n");

        assert_eq!(
            open.runs,
            [BundleRun {
                header_line: "header".to_string()
            }]
        );
        assert_eq!(
            empty.runs,
            [BundleRun {
                header_line: String::new()
            }]
        );
        assert!(empty.runs[0].header().is_none());
    }

    #[test]
    fn a_carriage_return_at_the_end_of_a_line_is_not_read() {
        let bundle = parse(
            "## Evidence bundle\r\nPurpose: p\r\nWork items: HR-1\r\n```houserules-run\r\nheader\r\n```\r\n",
        );

        assert_eq!(bundle.purpose, "p");
        assert_eq!(bundle.work_items, ["HR-1"]);
        assert_eq!(bundle.runs[0].header_line, "header");
    }

    #[test]
    fn a_header_line_that_is_a_capture_header_parses() {
        let line = r#"{"schema":"houserules/capture@1.0.0","argv":["a","b"],"cwd":"/x","start":"s","end":"e","exit":0,"signal":null,"head":null,"dirty":null,"lossy":0}"#;

        let header = BundleRun {
            header_line: line.to_string(),
        }
        .header()
        .expect("a header");

        assert_eq!(header.argv, ["a", "b"]);
        assert_eq!(header.exit, Some(0));
        assert_eq!(header.head, None);
    }

    /// A copied capture with the header `header` and the tail lines `tail`.
    fn copied(header: &str, tail: &[&str]) -> Copied {
        Copied {
            header: header.to_string(),
            tail: tail.iter().map(|line| (*line).to_string()).collect(),
        }
    }

    #[test]
    fn a_block_holds_the_header_and_then_the_tail_between_its_fences() {
        assert_eq!(
            copied("HEADER", &["a", "", "b"]).block(),
            "```houserules-run\nHEADER\na\n\nb\n```\n"
        );
        assert_eq!(
            copied("HEADER", &[]).block(),
            "```houserules-run\nHEADER\n```\n"
        );
        assert_eq!(
            copied("HEADER", &[""]).block(),
            "```houserules-run\nHEADER\n\n```\n"
        );
    }

    #[test]
    fn a_blank_line_at_the_end_of_the_tail_keeps_its_line() {
        assert_eq!(
            copied("HEADER", &["a", ""]).block(),
            "```houserules-run\nHEADER\na\n\n```\n"
        );
        assert_eq!(
            copied("HEADER", &["", ""]).block(),
            "```houserules-run\nHEADER\n\n\n```\n"
        );
    }

    #[test]
    fn a_lone_carriage_return_of_the_tail_is_escaped_and_ends_no_line() {
        assert_eq!(
            copied("HEADER", &["50%\r100%"]).block(),
            "```houserules-run\nHEADER\n50%\\u000d100%\n```\n"
        );
    }

    #[test]
    fn the_fence_of_a_block_is_longer_than_a_backtick_run_in_the_header_or_the_tail() {
        assert_eq!(
            copied("H", &["```"]).block(),
            "````houserules-run\nH\n```\n````\n"
        );
        let fence = "`".repeat(7);
        assert_eq!(
            copied("a``````b", &["x"]).block(),
            format!("{fence}houserules-run\na``````b\nx\n{fence}\n")
        );
    }

    #[test]
    fn a_control_character_of_the_tail_is_escaped_and_the_header_stays_whole() {
        assert_eq!(
            copied("H\u{7f}", &["red\u{1b}[0m"]).block(),
            "```houserules-run\nH\u{7f}\nred\\u001b[0m\n```\n"
        );
    }

    #[test]
    fn a_block_that_the_parser_reads_gives_back_its_header_and_nothing_else() {
        let block = copied("HEADER", &["## Evidence bundle", "Purpose: no", "```"]).block();

        let bundle = parse(&format!("## Evidence bundle\n\nPurpose: yes\n\n{block}"));

        assert_eq!(bundle.purpose, "yes");
        assert_eq!(bundle.runs.len(), 1);
        assert_eq!(bundle.runs[0].header_line, "HEADER");
        assert_eq!(sections(&format!("## Evidence bundle\n\n{block}")).len(), 1);
    }

    /// What the stub tracker answers to a `get` of an id.
    enum Says {
        Holds,
        Misses,
        Down,
        Fails,
    }

    /// A tracker that answers `get` from a table and records each id it was asked
    /// for. An id that the table lacks is missed.
    struct Table {
        answers: Vec<(&'static str, Says)>,
        asked: RefCell<Vec<String>>,
    }

    impl Table {
        fn new(answers: Vec<(&'static str, Says)>) -> Self {
            Self {
                answers,
                asked: RefCell::new(Vec::new()),
            }
        }
    }

    impl Tracker for Table {
        fn name(&self) -> &'static str {
            "stub"
        }

        fn matches_id(&self, _id: &str) -> bool {
            true
        }

        fn resolve(&self, _id: &str) -> Result<Resolution, TrackerError> {
            unreachable!("the check only gets")
        }

        fn list(&self, _opts: &ListOpts) -> Result<Vec<Value>, TrackerError> {
            unreachable!("the check only gets")
        }

        fn batch(&self, _number: &str) -> Result<Value, TrackerError> {
            unreachable!("the check only gets")
        }

        fn get(&self, ids: &[String]) -> Result<Vec<Value>, TrackerError> {
            self.asked.borrow_mut().push(ids[0].clone());
            let answer = self
                .answers
                .iter()
                .find(|(id, _)| *id == ids[0])
                .map(|(_, answer)| answer);
            match answer {
                Some(Says::Holds) => Ok(vec![json!({"id": ids[0], "status": "open"})]),
                Some(Says::Down) => Err(TrackerError::Unreachable {
                    adapter: "stub",
                    cause: "the stub is down".to_string(),
                    setup: "bring it up".to_string(),
                }),
                Some(Says::Fails) => {
                    Err(TrackerError::Command("the backlog is broken".to_string()))
                }
                Some(Says::Misses) | None => {
                    Err(TrackerError::NotFound(lines::unknown_id(&ids[0])))
                }
            }
        }

        fn set(&mut self, _id: &str, _assignments: &[String]) -> Result<Value, TrackerError> {
            unreachable!("the check only gets")
        }

        fn defer(&mut self, _request: &DeferRequest) -> Result<String, TrackerError> {
            unreachable!("the check only gets")
        }

        fn archive_backlog_half(&mut self, _lines: &mut Vec<String>) -> Result<(), TrackerError> {
            unreachable!("the check only gets")
        }

        fn check(&self) -> Result<(Vec<String>, Vec<String>), TrackerError> {
            unreachable!("the check only gets")
        }

        fn backlog_id_pattern(&self) -> Result<String, TrackerError> {
            unreachable!("the check only gets")
        }
    }

    /// The outcome of the check of the work items `ids` at `tracker`.
    fn work_items_at(tracker: &Table, ids: &[&str]) -> Result<Outcome, String> {
        let ids: Vec<String> = ids.iter().map(|id| (*id).to_string()).collect();
        let mut outcome = Outcome::default();
        check_work_items(tracker, &ids, &mut outcome)?;
        Ok(outcome)
    }

    /// The line for the work items from the first one that the stub cannot reach.
    fn not_resolved(count: usize) -> String {
        format!(
            "bundle check: {count} work items not resolved: tracker stub cannot be reached: the stub is down; bring it up"
        )
    }

    #[test]
    fn a_work_item_that_the_tracker_holds_gives_no_line() {
        let tracker = Table::new(vec![("A", Says::Holds)]);

        let outcome = work_items_at(&tracker, &["A"]).expect("an outcome");

        assert!(outcome.lines.is_empty());
        assert!(!outcome.failed);
    }

    #[test]
    fn a_work_item_that_the_tracker_misses_is_a_finding_and_the_next_one_is_asked() {
        let tracker = Table::new(vec![("A", Says::Misses), ("B", Says::Holds)]);

        let outcome = work_items_at(&tracker, &["A", "B"]).expect("an outcome");

        assert_eq!(
            outcome.lines,
            ["bundle check: work item `A`: the tracker holds no such work item"]
        );
        assert!(outcome.failed);
        assert_eq!(*tracker.asked.borrow(), ["A", "B"]);
    }

    #[test]
    fn the_work_items_from_the_first_one_out_of_reach_are_counted_and_not_asked() {
        let tracker = Table::new(vec![("A", Says::Holds), ("B", Says::Down)]);

        let outcome = work_items_at(&tracker, &["A", "B", "C"]).expect("an outcome");

        assert_eq!(outcome.lines, [not_resolved(2)]);
        assert!(!outcome.failed);
        assert_eq!(*tracker.asked.borrow(), ["A", "B"]);
    }

    #[test]
    fn a_finding_before_a_work_item_out_of_reach_stays_a_finding() {
        let tracker = Table::new(vec![("A", Says::Misses), ("B", Says::Down)]);

        let outcome = work_items_at(&tracker, &["A", "B", "C"]).expect("an outcome");

        assert_eq!(
            outcome.lines,
            [
                "bundle check: work item `A`: the tracker holds no such work item".to_string(),
                not_resolved(2)
            ]
        );
        assert!(outcome.failed);
    }

    #[test]
    fn a_tracker_that_fails_stops_the_check_with_its_line() {
        let tracker = Table::new(vec![("A", Says::Fails)]);

        let error = work_items_at(&tracker, &["A", "B"]).expect_err("a failure");

        assert_eq!(error, "the backlog is broken");
        assert_eq!(*tracker.asked.borrow(), ["A"]);
    }
}
