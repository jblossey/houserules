//! `houserules check-commit` (HR-062, batch 18 T2, spec §6): runs every
//! knowledge entry's `commits`-type check (`process.conventional-commits`'s
//! subject pattern and body-line budget, `security-hygiene.no-coauthor`'s
//! trailer gate, and any other entry a project adds with `check.type:
//! commits` -- read from the loaded base, never a hardcoded pair) against
//! either a not-yet-committed message or a range of already-committed
//! history.
//!
//! ## The two real call sites this command's shape is derived from
//!
//! commitlint is the retired shape both arms were derived from, at the two
//! call sites that used to exec it: `template/.githooks/commit-msg` (the
//! hook arm) ran `commitlint --edit "$msg_file"` -- git hands the hook the
//! proposed message's path as `$1` before the commit exists, so there was
//! no git range to read yet, only one message. `.github/workflows/ci.yml`'s
//! `commitlint` job (the CI arm) instead ran `pnpm exec commitlint --from
//! <base-sha> --to <head-sha> --verbose` over an already-merged range. This
//! command's own two arms mirror those call shapes directly: a positional
//! message-file argument for the hook, `--from`/`--to` (the same flag names
//! commitlint's own CI invocation used) for the range, defaulting `--to` to
//! `HEAD` the way `audit`'s own `--head` already does. Exactly one arm
//! applies per invocation; `cmd_check_commit` reports the other
//! combinations as named usage errors, exit 2.
//!
//! Batch 18 T6 (the reference rewrite's own repository flip) retired
//! commitlint from both call sites: the hook now probes `houserules
//! check-commit --help` and execs this command directly (`template/
//! .githooks/commit-msg`'s own doc comment has the exact probe-and-exec
//! text); the CI job is `check-commit`, not `commitlint` (`.github/
//! workflows/ci.yml`). commitlint itself left the repository (`package.json`,
//! `pnpm-lock.yaml`, `commitlint.config.mjs` deleted); the shape above is
//! historical, not a live pointer.
//!
//! ## Reuse, not reimplementation
//!
//! `super::audit::CommitsCheck` is the one place `subject`/`body_absent`/
//! `body_line_max` are evaluated against a `(subject, body)` pair; both
//! `run_check`'s `CheckType::Commits` arm and this file's own `check_commit`
//! call it, so the two commands can never independently drift on what
//! counts as a violation (`audit.rs`'s own module doc has that struct's
//! full account). The range arm reads its commits through `audit::rev` and
//! `audit::commits_in`, the exact same git-plumbing `audit` itself uses for
//! a `commits`-type check -- no second `git log` invocation of this file's
//! own. Only the message-file arm is genuinely new: it makes two git calls
//! of its own on the raw message -- a repository-local `core.commentChar`
//! lookup, then `git stripspace` (the Measured-parity section below has the
//! full account) -- and `split_message` then turns the stripped result into
//! the same `(subject, body)` shape `commits_in` reads back off real
//! history (see that function's own doc for the deliberately simple split
//! it uses).
//!
//! ## Output shape
//!
//! Unlike `audit`'s one row per checked id, `check-commit` reports one line
//! per violating `(check, commit)` pair -- every commit in a range is
//! checked against every `commits`-type entry, not only the first that
//! fails, matching commitlint's own range behavior (it reports every
//! offending commit, not merely the first). A violated check's own `level`
//! survives onto its finding (spec §6's level-survives ruling, T2 review
//! issue 4): a warn-level violation prints as `warn: <id>: <evidence>` and
//! never fails the run, exactly as `audit` records the same violation as a
//! non-fatal `warn` row rather than a `fail` one --
//! `backlog::commands::check_backlog`'s own `(errors, warnings)` shape and
//! its `cmd_check_backlog`'s `warn: ` prefix are the precedent this
//! mirrors. Otherwise `check-knowledge`'s own shape is the model for the
//! CLI surface: `ok` on stdout and exit 0 when no fail-level finding
//! exists, fail-level findings on stderr and exit 1 when one does, exit 2
//! for a usage or load failure (a bad ref, an unreadable message file, or a
//! malformed check pattern) -- one named line, never a panic
//! (`houserules.crash-paths-are-named`).
//!
//! ## Measured parity with commitlint (spec §6, T2 review issue 3)
//!
//! The message-file arm evaluates the same text commitlint evaluates at
//! the hook's own call site, not the raw file `git` hands the hook:
//! `git commit --verbose` leaves the proposed message, git's own comment
//! template, a scissors line, and the staged diff all in that one file, and
//! commitlint's `--edit` mode discards everything from the scissors line
//! onward and every comment line before it ever reads a subject or body.
//! `strip_verbose_and_comments` reproduces that exactly: a scissors marker
//! built from `comment_char` (a repository-local `git config
//! core.commentChar` lookup, mirroring commitlint's own, `#` when unset)
//! cuts the diff (which can hold arbitrarily long lines with nothing to do
//! with the real message), then `git stripspace --strip-comments` -- git's
//! own plumbing, no new crate -- removes the comment lines above it, honoring
//! the same repository-local `core.commentChar` on its own. See
//! `comment_char`'s own doc for the one naivety deliberately mirrored from
//! commitlint rather than fixed (a persisted `core.commentChar=auto`).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use super::audit::{CommitsCheck, commits_in, rev};
use super::check_shape::{CheckLevel, CheckType};
use super::model::{Base, CheckField, load_base};

/// `git commit --verbose`'s own "cut here" line's fixed second half; the
/// comment character itself (`comment_char`) is the repository-specific
/// part git derives it from.
const SCISSORS_SUFFIX: &str = " ------------------------ >8 ------------------------";

/// Reads `core.commentChar` from the repository at `root`, mirroring
/// commitlint's own `--edit` lookup byte for byte -- `@commitlint/cli`'s
/// `cli.js` (`git config core.commentChar`, trimmed stdout, `#` on a
/// nonzero exit or empty output) -- so the scissors marker this file
/// builds names the same character commitlint would use, on the same
/// repository, for the same message (spec §6's measured-parity ruling; a
/// controller-ruled fix over the T2 re-review r2 escalation, which had
/// found this file hardcoded to `#`). An absent config value (`git
/// config`'s own "not found" exit) is the common case and not a failure;
/// an unlaunchable git is not distinguished from it here for the same
/// reason commitlint's own lookup does not either -- `run_stripspace`,
/// called right after with the same `root`, already turns a genuinely
/// broken `git` into its own named `Err` for this arm.
///
/// One naivety is deliberately mirrored, not fixed, per spec §6's own
/// ruling on this exact corner: neither commitlint nor this function
/// resolves git's own deprecated `core.commentChar=auto` into the real
/// character git picks (`git help config` has that resolution rule); both
/// instead treat the four-byte string `auto` itself as the comment
/// character, which never matches the scissors line a real commit
/// actually wrote.
///
/// Retained, not merely asserted: a repository with `core.commentChar`
/// persisted as `auto` (`t2-evidence/commentchar-auto-repo`) and the
/// `git commit --verbose` file it produced
/// (`t2-evidence/verbose-message-fixture-commentchar-auto.txt`,
/// byte-identical to `VERBOSE_MESSAGE_FIXTURE` in this file's own tests --
/// git's own resolution picked `#` for it, the common outcome) both live
/// under `t2-evidence/`. `commitlint --edit` and `check-commit` are each
/// captured against it (`live_run`): exit 1 both, same
/// `body-max-line-length` failure, not exit 0.
fn comment_char(root: &Path) -> String {
    let output = Command::new("git")
        .args(["config", "--get", "core.commentChar"])
        .current_dir(root)
        .output();
    match output {
        Ok(output) if output.status.success() => {
            let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if value.is_empty() {
                "#".to_string()
            } else {
                value
            }
        }
        _ => "#".to_string(),
    }
}

/// Removes `git commit --verbose`'s scissors block, then every comment
/// line, from a raw not-yet-committed message -- the exact text commitlint
/// evaluates at the hook's own call site (spec §6's measured-parity
/// ruling). The scissors cut is a plain line search built from
/// `comment_char` (see that function's own doc for the one naivety it
/// deliberately mirrors from commitlint rather than fixing); the comment
/// strip runs `git stripspace --strip-comments` inside `root`, so a
/// repository-local `core.commentChar` applies to it too -- git's own
/// plumbing, not a reimplementation, matching the mechanism the T2 review
/// verified live (`git stripspace --strip-comments` alone leaves the diff
/// below the scissors line untouched; cutting first is required).
fn strip_verbose_and_comments(root: &Path, raw: &str) -> Result<String, String> {
    let scissors_line = format!("{}{SCISSORS_SUFFIX}", comment_char(root));
    let cut = match raw.lines().position(|line| line == scissors_line) {
        Some(index) => raw.lines().take(index).collect::<Vec<_>>().join("\n"),
        None => raw.to_string(),
    };
    run_stripspace(root, &cut)
}

/// Turns the stdin-writer thread's own result into a named `Err`, or
/// `Ok(())` when the write succeeded -- extracted out of `run_stripspace`
/// so its three outcomes (a clean write, a failed write, a panicked
/// thread) are each a direct, deterministic unit test, rather than a test
/// racing a real `git` process into failing mid-read: `git stripspace`
/// reads its whole input before it decides success or failure, so forcing
/// a genuine "write failed, yet the child still exited 0" case would need
/// a fake child process, not a real one. A silently discarded write
/// failure is exactly the gap this closes: `git` sees only the bytes that
/// did arrive before a broken pipe, treats that truncation as the whole
/// message once its own stdin reaches EOF, and can still exit 0 on it --
/// so the nonzero-status check alone does not catch a lost write.
fn writer_outcome(result: std::thread::Result<io::Result<()>>) -> Result<(), String> {
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(format!("could not write to git stripspace: {error}")),
        Err(_) => Err("the git stripspace stdin writer thread panicked".to_string()),
    }
}

/// Runs `git stripspace --strip-comments` over `text` inside `root` --
/// git's own commit-message cleanup plumbing (no new crate; `git help
/// stripspace`, verified live: `-s`/`--strip-comments` "skip and remove
/// all lines starting with comment character (core.commentChar, default
/// #)", read from whichever repository `current_dir` places it in).
/// Writes `text` to the child's stdin on its own thread rather than inline
/// before `wait_with_output` -- the standard fix for the classic deadlock
/// where a large enough `text` fills the child's stdout pipe before this
/// process finishes writing its stdin, and the two ends block on each
/// other forever (unreached in practice here, since the scissors cut above
/// already removes the one part of a `--verbose` message that can be
/// large, but cheap enough to close off rather than merely note). The
/// writer is joined -- and a failed or panicked write named, through
/// `writer_outcome` -- before the exit-status check below, not after:
/// `writer_outcome`'s own doc has the reason a failed write cannot be
/// left for that check to catch. An unlaunchable or failing `git`, or a
/// lost write, is a named `Err` (`houserules.crash-paths-are-named`),
/// matching every other git call in this crate.
fn run_stripspace(root: &Path, text: &str) -> Result<String, String> {
    let mut child = Command::new("git")
        .args(["stripspace", "--strip-comments"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run git stripspace: {error}"))?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let owned_text = text.to_string();
    let writer = std::thread::spawn(move || stdin.write_all(owned_text.as_bytes()));
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not run git stripspace: {error}"))?;
    writer_outcome(writer.join())?;
    if !output.status.success() {
        return Err(format!(
            "git stripspace failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Splits a message into `(subject, body)`: the first line is the
/// subject, and every line after it -- blank-line-separated from the
/// subject or not -- is the body. This is deliberately simpler than `git
/// log`'s own `%s`/`%b` split (`audit.rs`'s `commits_in`), which joins a
/// subject's wrapped continuation lines and can therefore drop a line
/// that never reaches a blank separator into the subject instead of the
/// body: safe for `subject`/`body_line_max` (a merged line only ever
/// makes the checked subject STRICTER, and the checked body SHORTER,
/// never the reverse), and it is the only choice that cannot silently
/// exempt a trailer line from `body_absent` by folding it into an
/// unchecked subject.
///
/// `split_message` itself stays git-free and comment-blind: on the
/// message-file arm, by the time this function runs, its `raw` argument
/// is already the output of `strip_verbose_and_comments` (the
/// Measured-parity section above has that pass's own account), so this
/// function never sees a comment-prefixed line or a scissors block in
/// practice, whatever the repository's own `core.commentChar` resolves
/// to. That leaves an asymmetry with the hook's own hard-coded
/// trailer grep, which is NOT run through that stripping: it reads the
/// raw, unstripped message file directly and exits before `houserules
/// check-commit` is even invoked, so a `Co-Authored-By` trailer sitting
/// only in a `git commit --verbose` file's comment template or diff still
/// trips the grep, even though the stripped text this function sees would
/// no longer carry it.
fn split_message(raw: &str) -> (String, String) {
    match raw.split_once('\n') {
        Some((subject, rest)) => (subject.to_string(), rest.to_string()),
        None => (raw.to_string(), String::new()),
    }
}

/// `check-commit`'s inputs: exactly one of `message_file` (the hook arm) or
/// `from` (the range arm, `to` defaulting to `HEAD`) is set --
/// `cmd_check_commit` enforces that exclusivity before building this.
pub(crate) struct CheckCommitOpts {
    pub message_file: Option<PathBuf>,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// Runs every `commits`-type knowledge entry in `base` against `opts`'
/// commit(s), returning `(fails, warns)`: one `"<id>: <evidence>"` finding
/// per violation, sorted into whichever list its own check's `level`
/// names -- `backlog::commands::check_backlog`'s own `(errors, warnings)`
/// return shape (`cli.rs`'s own doc). Findings are grouped by commit (the
/// range arm's own `git log` order, newest first, matching `audit`'s own
/// `commits_in`), each commit checked against every entry in id order. An
/// unreadable message file, an unresolvable `--from`/`--to` ref, a failed
/// `git stripspace`, or a malformed check pattern is a named `Err`, never
/// a panic (`houserules.crash-paths-are-named`).
fn check_commit(base: &Base, opts: &CheckCommitOpts) -> Result<(Vec<String>, Vec<String>), String> {
    let commits = match (&opts.message_file, &opts.from) {
        (Some(path), None) => {
            let raw =
                fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
            let cleaned = strip_verbose_and_comments(&base.root, &raw)?;
            vec![split_message(&cleaned)]
        }
        (None, Some(from)) => {
            let base_sha = rev(&base.root, from)?;
            let head_sha = rev(&base.root, opts.to.as_deref().unwrap_or("HEAD"))?;
            commits_in(&base.root, &base_sha, &head_sha)?
        }
        _ => unreachable!("cmd_check_commit enforces exactly one of message_file or from"),
    };

    let mut ids: Vec<&String> = base.entries.keys().collect();
    ids.sort();
    let mut checks = Vec::new();
    for id in ids {
        let entry = &base.entries[id];
        let CheckField::Valid(check) = &entry.check else {
            continue;
        };
        if check.kind != CheckType::Commits {
            continue;
        }
        let compiled = CommitsCheck::compile(check).map_err(|error| format!("{id}: {error}"))?;
        checks.push((id, compiled));
    }

    let mut fails = Vec::new();
    let mut warns = Vec::new();
    for (subject, body) in &commits {
        for (id, compiled) in &checks {
            if let Some(evidence) = compiled.violation(subject, body) {
                let line = format!("{id}: {evidence}");
                match compiled.level() {
                    CheckLevel::Fail => fails.push(line),
                    CheckLevel::Warn => warns.push(line),
                }
            }
        }
    }
    Ok((fails, warns))
}

/// Runs the `check-commit` subcommand: resolves `root` (`--dir`, or the
/// enclosing git repository's top level), loads the knowledge base there,
/// and runs every `commits`-type check against `message_file` or the
/// `from`/`to` range. Neither `message_file` nor `from` given, or both, is
/// a named usage error, exit 2, matching `audit`'s own `--report`/
/// `--workspace` exclusivity message shape; `to` without `from` is the
/// same. Warn-level findings print as `warn: <finding>` on stdout
/// unconditionally (`cmd_check_backlog`'s own `warn: ` convention);
/// fail-level findings print on stderr and exit 1, only when at least one
/// exists -- a warn-only run still prints `check-commit: ok` and exits 0.
pub(crate) fn cmd_check_commit(
    dir: Option<PathBuf>,
    message_file: Option<PathBuf>,
    from: Option<String>,
    to: Option<String>,
) -> ExitCode {
    match (&message_file, &from, &to) {
        (None, None, _) => {
            eprintln!("check-commit needs a message file or --from <ref>");
            return ExitCode::from(2);
        }
        (Some(_), Some(_), _) => {
            eprintln!("check-commit takes a message file or --from, not both");
            return ExitCode::from(2);
        }
        (Some(_), None, Some(_)) => {
            eprintln!("check-commit's --to needs --from");
            return ExitCode::from(2);
        }
        _ => {}
    }
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    let base = match load_base(&root) {
        Ok(base) => base,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let opts = CheckCommitOpts {
        message_file,
        from,
        to,
    };
    match check_commit(&base, &opts) {
        Ok((fails, warns)) => {
            for warn in &warns {
                println!("warn: {warn}");
            }
            if !fails.is_empty() {
                for fail in &fails {
                    eprintln!("{fail}");
                }
                return ExitCode::from(1);
            }
            println!("check-commit: ok");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::Command;

    use serde_json::{Value, json};

    use super::*;
    use crate::rules::model::load_base;

    /// Runs `git` in `root`, panicking with its stderr on failure -- this
    /// file's own copy of `audit.rs`'s identically named helper (that
    /// file's own module doc explains why each check-surface file keeps
    /// its own small set of these rather than sharing a test-only crate).
    fn git(root: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn write_file(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    /// `audit.rs`'s own `commit` helper.
    fn commit(root: &Path, message: &str, body: Option<&str>) -> String {
        git(root, &["add", "-A"]);
        let mut args = vec![
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t.t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            message,
        ];
        if let Some(body) = body {
            args.push("-m");
            args.push(body);
        }
        git(root, &args);
        git(root, &["rev-parse", "HEAD"]).trim().to_string()
    }

    /// A standing `process.sequential` rule entry, with every field a
    /// caller might override -- `audit.rs`'s own `entry` helper.
    fn entry(overrides: Value) -> Value {
        let mut base = json!({
            "id": "process.sequential", "kind": "rule", "area": "process", "standing": true,
            "summary": "Run agents sequentially.", "body": ["One at a time."], "tags": ["dispatch"],
            "source": {"date": "2026-08-29", "by": "user"},
        });
        if let (Value::Object(base_map), Value::Object(over_map)) = (&mut base, overrides) {
            for (key, value) in over_map {
                base_map.insert(key, value);
            }
        }
        base
    }

    /// A knowledge base under a fresh git repo, every entry filed under
    /// `knowledge/process.json`: `check_commit` reads every entry
    /// regardless of area (unlike `audit`'s own area-scoped package), so
    /// this fixture skips the area-glob machinery `audit.rs`'s own
    /// `make_repo` exercises and only needs `load_base` to succeed against
    /// the real vendored schema.
    fn make_repo(entries: &[Value]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        let schema =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json");
        write_file(
            root,
            "knowledge/schema.json",
            &std::fs::read_to_string(schema).unwrap(),
        );
        write_file(
            root,
            "knowledge/areas.json",
            &serde_json::to_string(&json!({
                "global": {"paths": []}, "process": {"paths": []},
                "docs": {"paths": []}, "tools": {"paths": []},
            }))
            .unwrap(),
        );
        write_file(
            root,
            "knowledge/process.json",
            &serde_json::to_string(&json!({
                "$schema": "./schema.json", "topic": "process", "title": "process title",
                "entries": entries,
            }))
            .unwrap(),
        );
        write_file(root, "CLAUDE.md", "# Test\n");
        commit(root, "chore: init", None);
        dir
    }

    fn conventional_commits_entry() -> Value {
        entry(json!({
            "id": "process.conventional-commits", "summary": "Conventional commits.",
            "check": {
                "type": "commits", "level": "fail",
                "subject": "^(feat|fix|chore|docs|test): .+", "body_line_max": 100,
            },
        }))
    }

    fn no_coauthor_entry() -> Value {
        entry(json!({
            "id": "security-hygiene.no-coauthor", "summary": "No co-author trailer.",
            "check": {
                "type": "commits", "level": "fail",
                "body_absent": "^(co-authored-by|claude-session):", "flags": "i",
            },
        }))
    }

    fn write_message(root: &Path, content: &str) -> PathBuf {
        let path = root.join("MSG");
        std::fs::write(&path, content).unwrap();
        path
    }

    fn message_opts(path: PathBuf) -> CheckCommitOpts {
        CheckCommitOpts {
            message_file: Some(path),
            from: None,
            to: None,
        }
    }

    fn range_opts(from: &str, to: Option<&str>) -> CheckCommitOpts {
        CheckCommitOpts {
            message_file: None,
            from: Some(from.to_string()),
            to: to.map(str::to_string),
        }
    }

    // ---- writer_outcome ----

    #[test]
    fn writer_outcome_passes_through_a_successful_write() {
        assert_eq!(writer_outcome(Ok(Ok(()))), Ok(()));
    }

    #[test]
    fn writer_outcome_names_a_failed_write_instead_of_silently_discarding_it() {
        let error = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        assert_eq!(
            writer_outcome(Ok(Err(error))),
            Err("could not write to git stripspace: broken pipe".to_string())
        );
    }

    #[test]
    fn writer_outcome_names_a_panicked_writer_thread_instead_of_silently_discarding_it() {
        let panicked: std::thread::Result<io::Result<()>> = Err(Box::new("boom"));
        assert_eq!(
            writer_outcome(panicked),
            Err("the git stripspace stdin writer thread panicked".to_string())
        );
    }

    // ---- split_message ----

    #[test]
    fn split_message_treats_a_single_line_message_as_subject_only() {
        assert_eq!(
            split_message("feat: x"),
            ("feat: x".to_string(), String::new())
        );
    }

    #[test]
    fn split_message_keeps_every_line_after_the_first_as_the_body() {
        assert_eq!(
            split_message("feat: x\n\nBody line one.\nBody line two."),
            (
                "feat: x".to_string(),
                "\nBody line one.\nBody line two.".to_string()
            )
        );
    }

    // ---- check_commit: message-file arm ----

    fn soft_subject_entry() -> Value {
        entry(json!({
            "id": "process.softsubject", "summary": "A warn-level subject rule.",
            "check": {"type": "commits", "level": "warn", "subject": "^ok: "},
        }))
    }

    #[test]
    fn passes_a_message_file_that_satisfies_every_commits_check() {
        let dir = make_repo(&[conventional_commits_entry(), no_coauthor_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), "feat: a clean subject\n\nAn unrelated line.\n");
        let (fails, warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(fails, Vec::<String>::new());
        assert_eq!(warns, Vec::<String>::new());
    }

    #[test]
    fn fails_a_message_file_whose_subject_does_not_match_the_conventional_pattern() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), "bad subject\n");
        let (fails, warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(
            fails,
            vec![
                "process.conventional-commits: commit \"bad subject\" does not match ^(feat|fix|chore|docs|test): .+"
                    .to_string()
            ]
        );
        assert_eq!(warns, Vec::<String>::new());
    }

    #[test]
    fn fails_a_message_file_carrying_a_co_authored_by_trailer() {
        let dir = make_repo(&[no_coauthor_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), "feat: x\n\nCo-Authored-By: someone\n");
        let (fails, _warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(
            fails,
            vec![
                "security-hygiene.no-coauthor: commit \"feat: x\" body matches ^(co-authored-by|claude-session):"
                    .to_string()
            ]
        );
    }

    #[test]
    fn fails_a_message_file_with_a_body_line_over_the_limit() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), &format!("feat: x\n\n{}\n", "x".repeat(101)));
        let (fails, _warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(
            fails,
            vec![
                "process.conventional-commits: commit \"feat: x\" has a body line over 100 characters"
                    .to_string()
            ]
        );
    }

    /// Spec §6's level-survives ruling (T2 review issue 4): a warn-level
    /// commits check's violation lands in `warns`, not `fails`, on the
    /// message-file arm.
    #[test]
    fn a_warn_level_commits_check_lands_in_warns_not_fails_on_the_message_file_arm() {
        let dir = make_repo(&[soft_subject_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), "not an ok subject\n");
        let (fails, warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(fails, Vec::<String>::new(), "{fails:#?}");
        assert_eq!(
            warns,
            vec![
                "process.softsubject: commit \"not an ok subject\" does not match ^ok: "
                    .to_string()
            ]
        );
    }

    #[test]
    fn errors_naming_an_unreadable_message_file_instead_of_panicking() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let missing = dir.path().join("no-such-file");
        let error = check_commit(&base, &message_opts(missing.clone())).unwrap_err();
        assert!(
            error.starts_with(&format!("{}: ", missing.display())),
            "{error}"
        );
    }

    #[test]
    fn errors_naming_a_malformed_check_pattern_instead_of_panicking() {
        let dir = make_repo(&[entry(json!({
            "id": "process.badpattern", "summary": "A malformed check.",
            "check": {"type": "commits", "level": "fail", "subject": "("},
        }))]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), "feat: x\n");
        let error = check_commit(&base, &message_opts(msg)).unwrap_err();
        assert!(error.starts_with("process.badpattern: "), "{error}");
    }

    /// A message file `git commit --verbose` really produced, captured
    /// live via a `commit-msg` hook that copied `$1` before git's own
    /// cleanup ran: one file staged whose only line is 161 `x` characters,
    /// `GIT_EDITOR` inserting the subject `feat: a real subject` above
    /// git's own comment template. Spec §6's measured-parity ruling names
    /// this exact reproduction: `commitlint --edit` on this file exits 0
    /// (verified live, `live_run`), and so must `check-commit`.
    const VERBOSE_MESSAGE_FIXTURE: &str = "feat: a real subject\n\n# Please enter the commit message for your changes. Lines starting\n# with '#' will be ignored, and an empty message aborts the commit.\n#\n# On branch main\n#\n# Initial commit\n#\n# Changes to be committed:\n#\tnew file:   long.txt\n#\n# ------------------------ >8 ------------------------\n# Do not modify or remove the line above.\n# Everything below it will be ignored.\ndiff --git a/long.txt b/long.txt\nnew file mode 100644\nindex 0000000..e63bcf0\n--- /dev/null\n+++ b/long.txt\n@@ -0,0 +1 @@\n+xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n";

    /// T2 review issue 3, reproduced and pinned: the 161-character diff
    /// line lives only below the scissors marker, so it must never reach
    /// `body_line_max` -- `check-commit` on the real captured file passes
    /// clean, matching commitlint's own verdict on the identical bytes.
    #[test]
    fn message_file_arm_matches_commitlints_verdict_on_a_real_verbose_commit_message() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), VERBOSE_MESSAGE_FIXTURE);
        let (fails, warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(fails, Vec::<String>::new(), "{fails:#?}");
        assert_eq!(warns, Vec::<String>::new());
    }

    /// The control the T2 review verified live: the same over-limit line,
    /// promoted to real body content (no scissors block, no comments),
    /// still fails -- proving the fix above narrows what gets removed
    /// rather than neutering `body_line_max` outright.
    #[test]
    fn a_body_line_over_the_limit_still_fails_when_it_is_not_behind_the_scissors_block() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(
            dir.path(),
            &format!("feat: a real subject\n\n{}\n", "x".repeat(161)),
        );
        let (fails, _warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(fails.len(), 1, "{fails:#?}");
    }

    /// Persists `core.commentChar` on `root` via `git config` (not a
    /// one-off `-c` override) -- the realistic way an adopter sets it, and
    /// the only form `comment_char`'s own `git config --get` lookup (and
    /// commitlint's identical one) actually reads back.
    fn persist_comment_char(root: &Path, comment_char: &str) {
        git(root, &["config", "core.commentChar", comment_char]);
    }

    /// The `;`-commented twin of `VERBOSE_MESSAGE_FIXTURE`: a message file
    /// `git commit --verbose` really produced from a repository with
    /// `core.commentChar` persisted as `;` (`git config core.commentChar
    /// ';'`, not a one-off override), otherwise identical (same staged
    /// 161-`x`-character file, same `GIT_EDITOR`-inserted subject).
    const VERBOSE_MESSAGE_FIXTURE_SEMICOLON: &str = "feat: a real subject\n\n; Please enter the commit message for your changes. Lines starting\n; with ';' will be ignored, and an empty message aborts the commit.\n;\n; On branch main\n;\n; Initial commit\n;\n; Changes to be committed:\n;\tnew file:   long.txt\n;\n; ------------------------ >8 ------------------------\n; Do not modify or remove the line above.\n; Everything below it will be ignored.\ndiff --git a/long.txt b/long.txt\nnew file mode 100644\nindex 0000000..e63bcf0\n--- /dev/null\n+++ b/long.txt\n@@ -0,0 +1 @@\n+xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx\n";

    /// The controller-ruled fix over the T2 re-review r2 escalation: a
    /// repository with `core.commentChar` genuinely persisted is no
    /// longer a divergence from commitlint. `commitlint --edit` on this
    /// exact file, run from a repository with the same persisted value,
    /// exits 0 (`live_run`); so must `check-commit`.
    #[test]
    fn message_file_arm_matches_commitlints_verdict_with_a_persisted_semicolon_comment_char() {
        let dir = make_repo(&[conventional_commits_entry()]);
        persist_comment_char(dir.path(), ";");
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), VERBOSE_MESSAGE_FIXTURE_SEMICOLON);
        let (fails, warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(fails, Vec::<String>::new(), "{fails:#?}");
        assert_eq!(warns, Vec::<String>::new());
    }

    /// The `;`-comment-char control, mirroring the `#` one above: the same
    /// over-limit line promoted to real body content still fails with
    /// `core.commentChar` persisted as `;`, proving the dynamic marker
    /// still narrows rather than neuters `body_line_max`.
    #[test]
    fn a_body_line_over_the_limit_still_fails_with_a_persisted_semicolon_comment_char() {
        let dir = make_repo(&[conventional_commits_entry()]);
        persist_comment_char(dir.path(), ";");
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(
            dir.path(),
            &format!("feat: a real subject\n\n{}\n", "x".repeat(161)),
        );
        let (fails, _warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(fails.len(), 1, "{fails:#?}");
    }

    /// The mirrored naivety spec §6 rules (`comment_char`'s own doc points
    /// here): a persisted `core.commentChar=auto` is taken literally, not
    /// resolved the way git itself would. `VERBOSE_MESSAGE_FIXTURE` is
    /// this exact fixture: retained separately as
    /// `t2-evidence/commentchar-auto-repo` (a repository with
    /// `core.commentChar` persisted as `auto`) and
    /// `t2-evidence/verbose-message-fixture-commentchar-auto.txt` (the
    /// `git commit --verbose` file it produced -- git's own "auto"
    /// resolution picked `#` for it, the common outcome, byte-identical to
    /// this constant), with both `commitlint --edit` (`live_run`, exit 1,
    /// `body-max-line-length`) and `check-commit` (`live_run`, exit 1)
    /// captured against it. Pinned here the same way: check-commit still
    /// fails, matching commitlint's own measured verdict on the identical
    /// bytes and repository state.
    #[test]
    fn a_persisted_auto_comment_char_still_fails_mirroring_commitlints_own_naivety() {
        let dir = make_repo(&[conventional_commits_entry()]);
        persist_comment_char(dir.path(), "auto");
        let base = load_base(dir.path()).expect("load base");
        let msg = write_message(dir.path(), VERBOSE_MESSAGE_FIXTURE);
        let (fails, _warns) = check_commit(&base, &message_opts(msg)).expect("check_commit");
        assert_eq!(fails.len(), 1, "{fails:#?}");
    }

    // ---- check_commit: range arm ----

    #[test]
    fn passes_a_range_whose_every_commit_satisfies_the_check() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "feat: good one", None);
        commit(root, "fix: good two", None);
        let base = load_base(root).expect("load base");
        let (fails, warns) =
            check_commit(&base, &range_opts(&base_sha, None)).expect("check_commit");
        assert_eq!(fails, Vec::<String>::new());
        assert_eq!(warns, Vec::<String>::new());
    }

    #[test]
    fn fails_a_range_naming_only_the_offending_commit() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "feat: good", None);
        commit(root, "bad subject", None);
        let base = load_base(root).expect("load base");
        let (fails, _warns) =
            check_commit(&base, &range_opts(&base_sha, None)).expect("check_commit");
        assert_eq!(
            fails,
            vec![
                "process.conventional-commits: commit \"bad subject\" does not match ^(feat|fix|chore|docs|test): .+"
                    .to_string()
            ]
        );
    }

    #[test]
    fn reports_every_offending_commit_in_a_range_not_only_the_first() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "bad one", None);
        commit(root, "bad two", None);
        let base = load_base(root).expect("load base");
        let (fails, _warns) =
            check_commit(&base, &range_opts(&base_sha, None)).expect("check_commit");
        assert_eq!(fails.len(), 2, "{fails:#?}");
    }

    #[test]
    fn resolves_to_as_head_by_default() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "bad subject", None);
        let base = load_base(root).expect("load base");
        let (fails, _warns) =
            check_commit(&base, &range_opts(&base_sha, Some("HEAD"))).expect("check_commit");
        assert_eq!(fails.len(), 1);
    }

    /// Spec §6's level-survives ruling, the range arm: a warn-level
    /// commits check's violation over a real range lands in `warns`, not
    /// `fails`.
    #[test]
    fn a_warn_level_commits_check_lands_in_warns_not_fails_on_the_range_arm() {
        let dir = make_repo(&[soft_subject_entry()]);
        let root = dir.path();
        let base_sha = commit(root, "chore: base", None);
        commit(root, "not an ok subject", None);
        let base = load_base(root).expect("load base");
        let (fails, warns) =
            check_commit(&base, &range_opts(&base_sha, None)).expect("check_commit");
        assert_eq!(fails, Vec::<String>::new(), "{fails:#?}");
        assert_eq!(
            warns,
            vec![
                "process.softsubject: commit \"not an ok subject\" does not match ^ok: "
                    .to_string()
            ]
        );
    }

    #[test]
    fn errors_naming_a_bad_from_ref_instead_of_panicking() {
        let dir = make_repo(&[conventional_commits_entry()]);
        let base = load_base(dir.path()).expect("load base");
        let error = check_commit(&base, &range_opts("nope", None)).unwrap_err();
        assert_eq!(error, "bad ref \"nope\"");
    }
}
