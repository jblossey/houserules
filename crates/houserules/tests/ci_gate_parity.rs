//! HR-117: `.github/workflows/ci.yml`'s own `run:` steps, parsed fresh
//! from the file, must each have a matching mention in the knowledge
//! base's `houserules.task-gates-mirror-ci` entry, also read fresh --
//! so a CI job that ships with no corresponding entry in the task-end
//! gate list (`houserules.task-gates-mirror-ci`'s own origin story: the
//! `check-commit` job went unnamed there and a per-commit check shipped
//! unrun at CI's real `--from`) fails THIS test instead of surfacing only
//! at the next incident.
//!
//! # Extraction: `run:` steps, inline or folded-block
//!
//! `extract_ci_run_lines` reads every step's `run:` value across every
//! job: `- run: <command>` on one line, or `run: >` followed by indented
//! continuation lines (a YAML folded block scalar), joined with spaces.
//! This is a narrow, hand-rolled reader for exactly ci.yml's own two
//! `run:` shapes, not a general YAML parser (no dependency this crate
//! does not already carry can express "every future step shape ci.yml
//! might use" better than parsing the two shapes it uses today,
//! `quality.principles`' YAGNI): a step written in a THIRD shape (a
//! literal `|` block, say) is unreachable content this reader does not
//! see, which is why the corpus test below also pins ci.yml's own run-
//! step COUNT, not only that parsing does not crash.
//!
//! # Matching: `identify` then `ALIASES`, both fail loudly on the unknown
//!
//! Each extracted command is reduced to a short identifier by `identify`
//! -- a small, exhaustive match over the handful of command shapes ci.yml
//! uses today (`mise run <task>`, `cargo <subcommand>`). A command
//! `identify` does not recognize is a PANIC naming the exact text, never
//! a silent skip (`houserules.crash-paths-are-named`'s shape, applied to
//! a dev-only gate test): ci.yml gaining a run step in a genuinely new
//! shape must stop this test, not slide past it.
//!
//! `ALIASES` then names, per identifier, the substring(s) the knowledge
//! entry's body is trusted to describe it with. Several of the entry's
//! own sentences deliberately paraphrase rather than quote (`"the full
//! suite"` for `cargo test --locked`, `"the lint chain"` for `mise run
//! lint`) -- `ALIASES` is the one place that paraphrase is recorded, so a
//! reviewer can see exactly which commands are matched by wording rather
//! than by literal quote. An identifier absent from `ALIASES` panics too:
//! the alias table must stay complete for every command `identify`
//! currently classifies, the same "derive, then fail loudly on the gap"
//! shape as the extraction step above.

use std::fs;
use std::path::{Path, PathBuf};

/// This checkout's repository root, resolved at compile time -- this
/// file's own copy of the pattern every `tests/*.rs` file in this crate
/// keeps independently (`install.rs`'s own doc explains why: a file
/// needing none of `mod common;`'s other helpers still warns the rest of
/// that module dead if pulled in just for this one function).
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// Every `run:` step's command text in `yaml`, in document order: a
/// `- run: <command>` line contributes `<command>` trimmed; a `run: >`
/// line (a folded block scalar, used where a command needs GitHub
/// Actions `${{ }}` expressions on their own lines) contributes every
/// following line more indented than the `run:` line itself, joined with
/// single spaces, up to the first line that is blank or back at or below
/// that indentation. This module's own doc, "Extraction", has the fuller
/// account of why this narrow reader -- not a general YAML parser -- is
/// the right size for ci.yml's own two `run:` shapes.
fn extract_ci_run_lines(yaml: &str) -> Vec<String> {
    let lines: Vec<&str> = yaml.split('\n').collect();
    let mut commands = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        let after_dash = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        if let Some(rest) = after_dash.strip_prefix("run:") {
            let rest = rest.trim();
            if rest == ">" {
                let mut collected = Vec::new();
                let mut cursor = index + 1;
                while cursor < lines.len() {
                    let next = lines[cursor];
                    if next.trim().is_empty() {
                        break;
                    }
                    let next_indent = next.len() - next.trim_start().len();
                    if next_indent <= indent {
                        break;
                    }
                    collected.push(next.trim());
                    cursor += 1;
                }
                commands.push(collected.join(" "));
                index = cursor;
                continue;
            } else if !rest.is_empty() {
                commands.push(rest.to_string());
            }
        }
        index += 1;
    }
    commands
}

/// Reduces one extracted `run:` command -- from ci.yml's own steps OR
/// from `mise.toml`'s `[tasks.lint]` run array, the same small,
/// exhaustive match covers both sources -- to a short, stable
/// identifier. This module's own doc, "Matching", explains why an
/// unrecognized shape panics instead of returning an unmatched
/// placeholder.
fn identify(command: &str) -> &'static str {
    match command {
        "mise run lint" => "lint",
        "mise run test" | "cargo test --locked" => "test",
        "cargo fmt --check" => "fmt",
        _ if command.starts_with("cargo clippy") => "clippy",
        _ if command.starts_with("mise run audit") => "audit",
        _ if command.starts_with("mise run houserules -- check-commit") => "check-commit",
        _ if command.starts_with("shellcheck ") => "shellcheck",
        "cargo deny check" => "deny",
        _ if command.starts_with("cargo check --target") => "cross-target",
        _ if command.starts_with("mise run houserules -- check-knowledge") => "check-knowledge",
        _ if command.starts_with("mise run houserules -- check-backlog") => "check-backlog",
        _ if command.starts_with("mise run houserules -- render") => "render",
        _ if command.ends_with("--bin residue-gate") => "residue-gate",
        _ if command.ends_with("--bin vacuous-exception-gate") => "vacuous-exception-gate",
        _ if command.ends_with("--bin payload-stamp-gate") => "payload-stamp-gate",
        _ if command.ends_with("--bin dist-generate-check") => "dist-generate-check",
        _ if command.contains("mingw-w64") => "mingw-toolchain",
        _ => panic!(
            "run command has no known identifier: {command:?} -- add a match arm in \
             tests/ci_gate_parity.rs's own `identify`, and an ALIASES entry naming the \
             substring houserules.task-gates-mirror-ci's body must contain for it"
        ),
    }
}

/// Per identifier, the substring(s) `houserules.task-gates-mirror-ci`'s
/// body is trusted to describe that command with -- this module's own
/// doc, "Matching", has the paraphrase account. `check-knowledge` and
/// `check-backlog` share one alias, "knowledge gates": the body names
/// them together ("both knowledge gates"), never individually.
const ALIASES: &[(&str, &[&str])] = &[
    ("lint", &["lint chain"]),
    ("test", &["full suite"]),
    ("fmt", &["cargo fmt --check"]),
    ("clippy", &["clippy"]),
    ("audit", &["mise run audit", "knowledge-base audit"]),
    ("check-commit", &["check-commit"]),
    ("shellcheck", &["shellcheck"]),
    ("deny", &["cargo deny"]),
    ("cross-target", &["cross-target check"]),
    ("check-knowledge", &["knowledge gates"]),
    ("check-backlog", &["knowledge gates"]),
    ("render", &["render --check"]),
    ("residue-gate", &["residue gate"]),
    ("vacuous-exception-gate", &["vacuous-exception gate"]),
    ("payload-stamp-gate", &["payload-stamp gate"]),
    ("dist-generate-check", &["dist-generate-check gate"]),
    ("mingw-toolchain", &["mingw-w64"]),
];

/// `mise.toml`'s `[tasks.lint]` own `run` array, read fresh: the text
/// between `run = [` and the array's own closing `]`, one command per
/// quoted, comma-terminated line. A narrow, hand-rolled reader for
/// exactly this one array's shape (plain double-quoted strings, no
/// escapes) -- not a general TOML parser, matching `extract_ci_run_
/// lines`'s own YAGNI reasoning for ci.yml's `run:` steps.
fn extract_mise_lint_run_lines(toml: &str) -> Vec<String> {
    let section_start = toml
        .find("[tasks.lint]")
        .expect("mise.toml has a [tasks.lint] section");
    let after_section = &toml[section_start..];
    let array_start = after_section
        .find("run = [")
        .expect("[tasks.lint] has its own run array");
    let after_array_start = &after_section[array_start + "run = [".len()..];
    let array_end = after_array_start
        .find("\n]")
        .expect("[tasks.lint]'s run array has its own closing ]");
    let body = &after_array_start[..array_end];
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim().trim_end_matches(',');
            let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
            Some(inner.to_string())
        })
        .collect()
}

/// `houserules.task-gates-mirror-ci`'s own `body` array, read fresh from
/// `knowledge/houserules.json` and joined with spaces -- never copied
/// inline, so an edit to the entry changes what this test checks the same
/// way it changes what a reader sees.
fn task_gates_mirror_ci_body_text() -> String {
    let path = repo_root().join("knowledge/houserules.json");
    let knowledge: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()));
    let entries = knowledge["entries"]
        .as_array()
        .expect("knowledge/houserules.json has an entries array");
    let entry = entries
        .iter()
        .find(|entry| entry["id"] == "houserules.task-gates-mirror-ci")
        .expect("houserules.task-gates-mirror-ci entry exists in knowledge/houserules.json");
    entry["body"]
        .as_array()
        .expect("houserules.task-gates-mirror-ci's body is an array")
        .iter()
        .filter_map(|value| value.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A synthetic fixture covering both `run:` shapes, so the parsing tests
/// below do not depend on ci.yml's own current text staying exactly as
/// it is today.
const FIXTURE: &str = "\
jobs:
  checks:
    steps:
      - run: mise run lint
      - if: github.event_name == 'pull_request'
        run: >
          mise run audit --
          --base ${{ github.event.pull_request.base.sha }}
          --head ${{ github.event.pull_request.head.sha }}
  rust:
    steps:
      - run: cargo fmt --check
";

/// A `- run: <command>` line contributes its command trimmed, for every
/// job in the document.
#[test]
fn extract_ci_run_lines_reads_an_inline_command() {
    let commands = extract_ci_run_lines(FIXTURE);
    assert!(
        commands.contains(&"mise run lint".to_string()),
        "{commands:?}"
    );
    assert!(
        commands.contains(&"cargo fmt --check".to_string()),
        "{commands:?}"
    );
}

/// A `run: >` folded block scalar's continuation lines join into one
/// command, space-separated, GitHub Actions `${{ }}` expressions
/// included.
#[test]
fn extract_ci_run_lines_joins_a_folded_block_scalar_with_spaces() {
    let commands = extract_ci_run_lines(FIXTURE);
    assert!(
        commands.contains(
            &"mise run audit -- --base ${{ github.event.pull_request.base.sha }} --head \
              ${{ github.event.pull_request.head.sha }}"
                .to_string()
        ),
        "{commands:?}"
    );
}

/// Commands come back in the document's own order (the `checks` job's
/// `mise run lint` before the `rust` job's `cargo fmt --check`), never
/// re-sorted or grouped by job.
#[test]
fn extract_ci_run_lines_preserves_document_order() {
    let commands = extract_ci_run_lines(FIXTURE);
    let lint_index = commands
        .iter()
        .position(|c| c == "mise run lint")
        .expect("mise run lint present");
    let fmt_index = commands
        .iter()
        .position(|c| c == "cargo fmt --check")
        .expect("cargo fmt --check present");
    assert!(lint_index < fmt_index, "{commands:?}");
}

/// The live corpus proof: ci.yml's own run steps, parsed fresh, are
/// exactly eight today (`checks`: the mingw-w64 install, lint, test,
/// audit; `rust`: fmt, clippy, test; `check-commit`: one) -- a growth or
/// shrink here means ci.yml changed shape, which this pinned count
/// catches even for a command `identify` would otherwise classify under
/// an existing, already-matched identifier (a second `cargo clippy`
/// line, say).
#[test]
fn ci_yml_declares_exactly_eight_run_steps_today() {
    let path = repo_root().join(".github/workflows/ci.yml");
    let yaml = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let commands = extract_ci_run_lines(&yaml);
    assert_eq!(commands.len(), 8, "{commands:?}");
}

/// The live parity proof (HR-117's own reason to exist): every command
/// ci.yml's own `run:` steps declare, parsed fresh, has a matching
/// mention in `houserules.task-gates-mirror-ci`'s own body, also read
/// fresh -- both sides derived from their sources at test time, never
/// copied inline.
/// Asserts every command in `commands` (each reduced to an identifier
/// via `identify`) has a matching mention in `body` -- the shared check
/// both `every_ci_run_command_has_a_matching_mention...` and
/// `every_mise_lint_member_has_a_matching_mention...` run, so ci.yml's
/// own run steps and `mise.toml`'s `[tasks.lint]` run array are checked
/// identically against the same knowledge entry, with `source` naming
/// which one a failure came from.
fn assert_every_command_has_a_matching_mention(commands: &[String], body: &str, source: &str) {
    let body = body.to_lowercase();
    for command in commands {
        let identifier = identify(command);
        let aliases = ALIASES
            .iter()
            .find(|(key, _)| *key == identifier)
            .map(|(_, substrings)| *substrings)
            .unwrap_or_else(|| {
                panic!("identifier {identifier:?} (from {command:?}) has no ALIASES entry")
            });
        let matched = aliases
            .iter()
            .any(|needle| body.contains(&needle.to_lowercase()));
        assert!(
            matched,
            "{source} run command {command:?} (identifier {identifier:?}) has no matching \
             mention in houserules.task-gates-mirror-ci's body -- update the knowledge entry, \
             or ALIASES here if the wording is an intentional paraphrase; checked for: \
             {aliases:?}"
        );
    }
}

/// The live parity proof (HR-117's own reason to exist): every command
/// ci.yml's own `run:` steps declare, parsed fresh, has a matching
/// mention in `houserules.task-gates-mirror-ci`'s own body, also read
/// fresh -- both sides derived from their sources at test time, never
/// copied inline.
#[test]
fn every_ci_run_command_has_a_matching_mention_in_the_task_gates_mirror_ci_entry() {
    let path = repo_root().join(".github/workflows/ci.yml");
    let yaml = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let commands = extract_ci_run_lines(&yaml);
    assert!(!commands.is_empty(), "ci.yml declared no run steps at all");
    let body = task_gates_mirror_ci_body_text();
    assert_every_command_has_a_matching_mention(&commands, &body, "ci.yml");
}

/// HR-117's own review finding: the "lint chain" the knowledge entry
/// names is itself a hand-kept enumeration (`shellcheck, cargo deny,
/// both knowledge gates, ...`) nothing previously checked against
/// `mise.toml`'s own `[tasks.lint]` run array -- so that inner list
/// could omit a real lint step exactly the way the outer CI-job list
/// once omitted `check-commit`. Parses `mise.toml` fresh and checks each
/// of its ten members the same way the outer test checks ci.yml's own
/// eight.
#[test]
fn every_mise_lint_member_has_a_matching_mention_in_the_task_gates_mirror_ci_entry() {
    let path = repo_root().join("mise.toml");
    let toml = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let commands = extract_mise_lint_run_lines(&toml);
    assert_eq!(
        commands.len(),
        10,
        "expected mise.toml's [tasks.lint] to declare 10 run members today, got {commands:?}"
    );
    let body = task_gates_mirror_ci_body_text();
    assert_every_command_has_a_matching_mention(&commands, &body, "mise.toml [tasks.lint]");
}
