//! The run checks of `check-report-claims`. A run is a deliverable field shaped
//! `{ capture, sha256, command, exit, excerpt? }` (`collect_runs`).
//! `check_captures` ties each run to the capture file that it cites: the file
//! exists, is a capture, and has the recorded hash, and the recorded command and
//! exit equal the header of the capture; a run that lacks one of those three
//! keys gets a line that names the key. `check_excerpts` tests that the excerpt
//! of a run is a byte-exact substring of the output of its capture. The parent
//! module doc states the stance of every Limits list.
//!
//! Limits (every doc comment of this file that says "see the module doc's
//! limits" means this list):
//! - `check_captures` proves that the file is the one the report recorded and
//!   that the report repeats the header of the file. It does not prove that the
//!   command ran as the header says: a header that a person wrote by hand, with
//!   its hash recorded, passes.
//! - The `command` of a run equals the `argv` of the header joined for a POSIX
//!   shell (`shell::join`), compared as text. The same words typed with other
//!   quoting differ.
//! - `check_excerpts` proves that the excerpt is in the output of the capture.
//!   It does not prove that the excerpt is the part of the output that matters.
//!   An empty excerpt is in every output, and the schema refuses it.
//! - A run is a JSON object at one of the five places of `collect_runs`; any
//!   other value at one of those places is skipped (`validate` refuses it). A
//!   `capture` or an `excerpt` of another type than a string counts as missing.
//!   A `sha256`, `command`, or `exit` is missing only when its key is absent;
//!   a value of another type, and an `exit` that is no integer of 32 bits, is
//!   wrong: it differs from the capture's and its line shows it as written.
//! - Each check reads a capture whole, into memory, and a capture has no size
//!   limit.

use std::path::Path;

use serde_json::Value;

use super::resolve_against;
use crate::capture::{self, Capture, ReadError};
use crate::shell;

/// One deliverable field shaped `{ capture, sha256, command, exit, excerpt? }`,
/// as the report spells it. `capture` and `excerpt` are `None` when the report
/// lacks the key or holds another type than a string. `sha256`, `command`, and
/// `exit` are `None` only when the report lacks the key; a value of any type
/// stays as written, so that a finding can show it.
pub(super) struct RunRef {
    /// The JSON path of the run (`tdd[2].red`), which starts each finding.
    label: String,
    /// The path of the capture file, as the report spells it.
    capture: Option<String>,
    /// The recorded hash of the capture file, as written.
    sha256: Option<Value>,
    /// The recorded command, as written.
    command: Option<Value>,
    /// The recorded exit, as written: `null` is a recorded `null`.
    exit: Option<Value>,
    /// The recorded excerpt of the output.
    excerpt: Option<String>,
}

/// The string at `key` of `value`, or `None`.
fn string_at(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Pushes `value` onto `runs` under `label` when it is a JSON object; anything
/// else (a missing value, or one that is no object) is skipped.
fn push_run(runs: &mut Vec<RunRef>, label: String, value: Option<&Value>) {
    let Some(value) = value.filter(|value| value.is_object()) else {
        return;
    };
    runs.push(RunRef {
        label,
        capture: string_at(value, "capture"),
        sha256: value.get("sha256").cloned(),
        command: value.get("command").cloned(),
        exit: value.get("exit").cloned(),
        excerpt: string_at(value, "excerpt"),
    });
}

/// Every deliverable field shaped `{ capture, sha256, command, exit, excerpt? }`
/// (the `run` definition in `.claude/schemas/deliverables.json`) this tool can
/// inspect, each paired with a JSON-path-ish label for its findings. Covers
/// `tests`, `live_run`, each `tdd[].red`/`.green`, and each
/// `fix_rounds[].tests[]` -- every place the schema allows a run.
pub(super) fn collect_runs(report: &Value) -> Vec<RunRef> {
    let mut runs = Vec::new();
    if let Some(tests) = report.get("tests").and_then(Value::as_array) {
        for (i, run) in tests.iter().enumerate() {
            push_run(&mut runs, format!("tests[{i}]"), Some(run));
        }
    }
    if let Some(live_run) = report.get("live_run").and_then(Value::as_array) {
        for (i, run) in live_run.iter().enumerate() {
            push_run(&mut runs, format!("live_run[{i}]"), Some(run));
        }
    }
    if let Some(tdd) = report.get("tdd").and_then(Value::as_array) {
        for (i, cycle) in tdd.iter().enumerate() {
            push_run(&mut runs, format!("tdd[{i}].red"), cycle.get("red"));
            push_run(&mut runs, format!("tdd[{i}].green"), cycle.get("green"));
        }
    }
    if let Some(rounds) = report.get("fix_rounds").and_then(Value::as_array) {
        for (i, round) in rounds.iter().enumerate() {
            if let Some(tests) = round.get("tests").and_then(Value::as_array) {
                for (j, run) in tests.iter().enumerate() {
                    push_run(&mut runs, format!("fix_rounds[{i}].tests[{j}]"), Some(run));
                }
            }
        }
    }
    runs
}

/// The capture that `run` cites, read from its path: a relative path resolves
/// against `root`, and an absolute path stands as it is (`resolve_against`).
/// `None` when the run cites no capture. The path is the one the report spells.
fn read_cited<'a>(root: &Path, run: &'a RunRef) -> Option<(&'a str, Result<Capture, ReadError>)> {
    let path = run.capture.as_deref()?;
    Some((path, capture::read(&resolve_against(root, path))))
}

/// How a finding spells the exit of a header: the code, or `null` for a command
/// that a signal ended.
fn spell_header_exit(exit: Option<i32>) -> String {
    exit.map_or_else(|| "null".to_string(), |code| code.to_string())
}

/// How a finding spells a recorded value: a string as it is, any other value as
/// the JSON that the report holds.
pub(super) fn spell_recorded(recorded: &Value) -> String {
    recorded
        .as_str()
        .map_or_else(|| recorded.to_string(), str::to_string)
}

/// The line for a key that a run with a capture lacks.
fn missing_key(label: &str, key: &str) -> String {
    format!("{label}: no {key}; run houserules report-facts")
}

/// Check 1. Every `collect_runs` entry cites a capture that exists, is a
/// capture, and has the recorded `sha256`, and its `command` and `exit` equal
/// the header's. The three keys are independent: a run can give a line for each
/// of them. A key that the run lacks gets the line that points to
/// `report-facts`; a key that holds a value which differs from the capture's,
/// of any type, gets the line of the difference with the value as written.
pub(super) fn check_captures(root: &Path, runs: &[RunRef], errors: &mut Vec<String>) {
    for run in runs {
        let label = &run.label;
        let Some((path, read)) = read_cited(root, run) else {
            errors.push(format!(
                "{label}: no capture; run the command through houserules capture"
            ));
            continue;
        };
        let capture = match read {
            Ok(capture) => capture,
            Err(ReadError::Missing) => {
                errors.push(format!("{label}: capture {path} does not exist"));
                continue;
            }
            Err(ReadError::Unreadable(os_error)) => {
                errors.push(format!(
                    "{label}: capture {path} could not be read ({os_error})"
                ));
                continue;
            }
            Err(ReadError::NoCapture(reason)) => {
                errors.push(format!("{label}: {path} is no capture ({reason})"));
                continue;
            }
        };
        let actual = capture.sha256();
        match &run.sha256 {
            None => errors.push(missing_key(label, "sha256")),
            Some(recorded) if recorded.as_str() == Some(actual.as_str()) => {}
            Some(recorded) => errors.push(format!(
                "{label}: capture {path} has sha256 {actual}, but the report records {}; re-read the capture, then remove sha256 and run report-facts",
                spell_recorded(recorded)
            )),
        }
        let header_command = shell::join(&capture.header.argv);
        match &run.command {
            None => errors.push(missing_key(label, "command")),
            Some(recorded) if recorded.as_str() == Some(header_command.as_str()) => {}
            Some(_) => errors.push(format!(
                "{label}: command differs from the capture's ({header_command})"
            )),
        }
        match &run.exit {
            None => errors.push(missing_key(label, "exit")),
            Some(recorded) if *recorded == Value::from(capture.header.exit) => {}
            Some(recorded) => errors.push(format!(
                "{label}: exit {recorded} differs from the capture's {}",
                spell_header_exit(capture.header.exit)
            )),
        }
    }
}

/// Whether `needle` occurs in `haystack`, byte for byte. The empty needle
/// occurs in every haystack.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Check 2 (HR-197). The `excerpt` of every `collect_runs` entry is a
/// byte-exact substring of the output of its capture: the bytes after the
/// header line. A run whose capture cannot be read adds no line here; check 1
/// names the capture.
pub(super) fn check_excerpts(root: &Path, runs: &[RunRef], errors: &mut Vec<String>) {
    for run in runs {
        let Some(excerpt) = run.excerpt.as_deref() else {
            continue;
        };
        let Some((path, Ok(capture))) = read_cited(root, run) else {
            continue;
        };
        if !contains_bytes(capture.output(), excerpt.as_bytes()) {
            errors.push(format!(
                "{}.excerpt: not a byte-exact substring of the output of {path}",
                run.label
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::{Value, json};

    use super::{check_captures, collect_runs};
    use crate::report_claims::check_report_claims;
    use crate::report_claims::tests::{
        EARLY_END, base_report, capture_header, clean_run, init_scratch_repo, write_capture,
        write_json,
    };

    /// The findings of `check_report_claims` over `report`, written as
    /// `report.json` in `dir`.
    fn findings(dir: &Path, report: &Value) -> Vec<String> {
        let report_path = dir.join("report.json");
        write_json(&report_path, report);
        check_report_claims(&report_path, dir).expect("report loads")
    }

    /// `report` with `run` as its one `tests` entry.
    fn report_with_test(head: &str, run: Value) -> Value {
        let mut report = base_report(head);
        report["tests"] = json!([run]);
        report
    }

    // ---- check 1: the capture of a run ----

    /// A run of the old form (a `command` and an `output`, no `capture`) gets
    /// the same line in each of the five places a run can stand, each with its
    /// own label.
    #[test]
    fn a_run_without_a_capture_names_the_remedy_in_every_place() {
        let (dir, head) = init_scratch_repo("check-report-claims-no-capture-");
        let old = json!({"command": "cargo test", "output": "ok\n", "exit": 0});
        let mut report = base_report(&head);
        report["tests"] = json!([old]);
        report["live_run"] = json!([old]);
        report["tdd"] = json!([{"test": "t", "mode": "reconstructed", "red": old, "green": old}]);
        report["fix_rounds"] = json!([{"round": 1, "findings": [], "commits": [], "tests": [old]}]);
        let line = "no capture; run the command through houserules capture";
        assert_eq!(
            findings(dir.path(), &report),
            [
                "tests[0]",
                "live_run[0]",
                "tdd[0].red",
                "tdd[0].green",
                "fix_rounds[0].tests[0]"
            ]
            .map(|label| format!("{label}: {line}"))
        );
    }

    /// A capture that does not exist is named by the path as the report spells
    /// it.
    #[test]
    fn a_missing_capture_file_is_named() {
        let (dir, head) = init_scratch_repo("check-report-claims-capture-missing-");
        let run = json!({
            "capture": "evidence/gone.txt",
            "sha256": "0".repeat(64),
            "command": "cargo test",
            "exit": 0,
        });
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            ["tests[0]: capture evidence/gone.txt does not exist"]
        );
    }

    /// A capture that exists and cannot be read carries the operating system's
    /// own error, never a silent skip (`houserules.crash-paths-are-named`). A
    /// directory at the cited path is such a file on every platform; the test
    /// takes the expected text from the same read.
    #[test]
    fn an_unreadable_capture_is_named_with_the_os_error() {
        let (dir, head) = init_scratch_repo("check-report-claims-capture-unreadable-");
        let cited = dir.path().join("evidence/folder.txt");
        std::fs::create_dir_all(&cited).expect("create the directory");
        let os_error = std::fs::read(&cited).expect_err("a directory is no file to read");
        assert_ne!(os_error.kind(), std::io::ErrorKind::NotFound);
        let run = json!({
            "capture": "evidence/folder.txt",
            "sha256": "0".repeat(64),
            "command": "cargo test",
            "exit": 0,
        });
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            [format!(
                "tests[0]: capture evidence/folder.txt could not be read ({os_error})"
            )]
        );
    }

    /// A file that is no capture names the reason that the reader gives, one of
    /// the four of spec 3.
    #[test]
    fn a_file_that_is_no_capture_names_its_reason() {
        let (dir, head) = init_scratch_repo("check-report-claims-no-capture-file-");
        let run = json!({
            "capture": "evidence/not-a-capture.txt",
            "sha256": "0".repeat(64),
            "command": "cargo test",
            "exit": 0,
        });
        let report = report_with_test(&head, run);
        std::fs::create_dir_all(dir.path().join("evidence")).expect("create the folder");
        for (content, reason) in [
            ("", "it is empty"),
            ("plain text\n", "its first line is no JSON object"),
            (
                "{\"schema\":\"houserules/capture@1.0.0\"}\n",
                "its header lacks argv",
            ),
            (
                "{\"schema\":\"houserules/capture@2.0.0\"}\n",
                "its schema is houserules/capture@2.0.0, not houserules/capture@1",
            ),
        ] {
            std::fs::write(dir.path().join("evidence/not-a-capture.txt"), content)
                .expect("write the file");
            assert_eq!(
                findings(dir.path(), &report),
                [format!(
                    "tests[0]: evidence/not-a-capture.txt is no capture ({reason})"
                )],
                "{content:?}"
            );
        }
    }

    /// A capture whose bytes changed after the report recorded its hash gives
    /// the hash line with both hashes.
    #[test]
    fn a_changed_capture_is_a_hash_finding() {
        let (dir, head) = init_scratch_repo("check-report-claims-capture-changed-");
        let argv = ["cargo", "test"];
        let run = clean_run(
            dir.path(),
            "evidence/red.txt",
            &argv,
            "test result: FAILED\n",
        );
        clean_run(dir.path(), "evidence/red.txt", &argv, "test result: ok\n");
        let actual = crate::baseline::hash(
            &std::fs::read(dir.path().join("evidence/red.txt")).expect("read the capture"),
        );
        let recorded = run["sha256"].as_str().expect("a recorded hash").to_string();
        assert_ne!(actual, recorded);
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            [format!(
                "tests[0]: capture evidence/red.txt has sha256 {actual}, but the report records {recorded}; re-read the capture, then remove sha256 and run report-facts"
            )]
        );
    }

    /// A `command` that the author edited differs from the joined `argv` of the
    /// header, which the line quotes.
    #[test]
    fn an_edited_command_is_a_finding() {
        let (dir, head) = init_scratch_repo("check-report-claims-command-edited-");
        let mut run = clean_run(
            dir.path(),
            "evidence/red.txt",
            &["cargo", "test", "--locked"],
            "FAILED\n",
        );
        run["command"] = json!("cargo test");
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            ["tests[0]: command differs from the capture's (cargo test --locked)"]
        );
    }

    /// The header joins an argument that needs quoting the way a shell reads it
    /// back, so a `command` the author copied from the header matches.
    #[test]
    fn a_command_with_a_quoted_argument_matches_the_joined_header() {
        let (dir, head) = init_scratch_repo("check-report-claims-command-quoted-");
        let run = clean_run(
            dir.path(),
            "evidence/out.txt",
            &["sh", "-c", "echo a b"],
            "a b\n",
        );
        assert_eq!(run["command"], json!("sh -c 'echo a b'"));
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            Vec::<String>::new()
        );
    }

    /// An `exit` that differs from the header's gives the line with both values;
    /// a header that a signal ended reads `null` on either side.
    #[test]
    fn an_edited_exit_is_a_finding() {
        let (dir, head) = init_scratch_repo("check-report-claims-exit-edited-");
        for (header_exit, report_exit, line) in [
            (Some(101), json!(0), "exit 0 differs from the capture's 101"),
            (None, json!(0), "exit 0 differs from the capture's null"),
            (
                Some(1),
                json!(null),
                "exit null differs from the capture's 1",
            ),
        ] {
            let mut run = write_capture(
                dir.path(),
                "evidence/run.txt",
                &capture_header(&["cargo", "test"], header_exit, EARLY_END),
                "output\n",
            );
            run["exit"] = report_exit;
            assert_eq!(
                findings(dir.path(), &report_with_test(&head, run)),
                [format!("tests[0]: {line}")]
            );
        }
    }

    /// A run that lacks its hash, its command, and its exit gets one line for
    /// each key, in that order, and each line names the command that fills it.
    #[test]
    fn a_run_without_a_hash_a_command_and_an_exit_gets_a_line_for_each_key() {
        let (dir, head) = init_scratch_repo("check-report-claims-run-fields-missing-");
        let mut run = clean_run(dir.path(), "evidence/run.txt", &["cargo", "test"], "ok\n");
        let fields = run.as_object_mut().expect("a run object");
        fields.remove("sha256");
        fields.remove("command");
        fields.remove("exit");
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            [
                "tests[0]: no sha256; run houserules report-facts",
                "tests[0]: no command; run houserules report-facts",
                "tests[0]: no exit; run houserules report-facts",
            ]
        );
    }

    /// A run that lacks one key gets the line of that key and no other: the
    /// keys it holds still equal the capture's.
    #[test]
    fn a_run_that_lacks_one_key_gets_the_line_of_that_key_only() {
        let (dir, head) = init_scratch_repo("check-report-claims-run-field-missing-");
        for key in ["sha256", "command", "exit"] {
            let mut run = clean_run(dir.path(), "evidence/run.txt", &["cargo", "test"], "ok\n");
            run.as_object_mut().expect("a run object").remove(key);
            assert_eq!(
                findings(dir.path(), &report_with_test(&head, run)),
                [format!("tests[0]: no {key}; run houserules report-facts")],
                "{key}"
            );
        }
    }

    /// A key that holds a value of another type than the schema gives is
    /// wrong, not missing: its line shows the value as the report writes it,
    /// and `report-facts` is no remedy for it.
    #[test]
    fn a_key_of_another_type_is_wrong_and_not_missing() {
        let (dir, head) = init_scratch_repo("check-report-claims-run-field-type-");
        let clean = clean_run(dir.path(), "evidence/run.txt", &["cargo", "test"], "ok\n");
        let actual = clean["sha256"].as_str().expect("a hash").to_string();
        let hash_line = |recorded: &str| {
            format!(
                "tests[0]: capture evidence/run.txt has sha256 {actual}, but the report records {recorded}; re-read the capture, then remove sha256 and run report-facts"
            )
        };
        let exit_line =
            |recorded: &str| format!("tests[0]: exit {recorded} differs from the capture's 0");
        let command_line = "tests[0]: command differs from the capture's (cargo test)".to_string();
        for (key, value, line) in [
            ("exit", json!("0"), exit_line("\"0\"")),
            ("exit", json!(0.5), exit_line("0.5")),
            ("exit", json!(true), exit_line("true")),
            ("exit", json!([0]), exit_line("[0]")),
            ("exit", json!(5_000_000_000_i64), exit_line("5000000000")),
            ("sha256", json!(5), hash_line("5")),
            ("sha256", json!(null), hash_line("null")),
            ("command", json!(7), command_line),
        ] {
            let mut run = clean.clone();
            run[key] = value.clone();
            assert_eq!(
                findings(dir.path(), &report_with_test(&head, run)),
                [line],
                "{key} {value}"
            );
        }
    }

    /// The differences of one run are independent: a changed capture, an edited
    /// command, and an edited exit give three lines, in that order.
    #[test]
    fn every_difference_of_one_run_gets_its_own_line() {
        let (dir, head) = init_scratch_repo("check-report-claims-run-all-differ-");
        let argv = ["cargo", "test"];
        let mut run = write_capture(
            dir.path(),
            "evidence/run.txt",
            &capture_header(&argv, Some(1), EARLY_END),
            "before\n",
        );
        write_capture(
            dir.path(),
            "evidence/run.txt",
            &capture_header(&argv, Some(1), EARLY_END),
            "after\n",
        );
        run["command"] = json!("cargo build");
        run["exit"] = json!(0);
        let findings = findings(dir.path(), &report_with_test(&head, run));
        assert_eq!(findings.len(), 3, "{findings:?}");
        assert!(findings[0].starts_with("tests[0]: capture evidence/run.txt has sha256 "));
        assert_eq!(
            findings[1..],
            [
                "tests[0]: command differs from the capture's (cargo test)",
                "tests[0]: exit 0 differs from the capture's 1"
            ]
        );
    }

    /// Pin: a report whose runs all cite intact captures passes, in each of the
    /// five places and with an excerpt that the output holds. The old checks
    /// do not see the new form, so this passes at the base too.
    #[test]
    fn a_report_of_intact_captures_passes() {
        let (dir, head) = init_scratch_repo("check-report-claims-captures-intact-");
        let mut run = clean_run(
            dir.path(),
            "evidence/run.txt",
            &["cargo", "test"],
            "test result: ok. 3 passed\n",
        );
        run["excerpt"] = json!("3 passed");
        let mut report = base_report(&head);
        report["tests"] = json!([run]);
        report["live_run"] = json!([run]);
        report["tdd"] = json!([{"test": "t", "mode": "reconstructed", "red": run, "green": run}]);
        report["fix_rounds"] = json!([{"round": 1, "findings": [], "commits": [], "tests": [run]}]);
        assert_eq!(findings(dir.path(), &report), Vec::<String>::new());
    }

    /// Pin: a capture cited by an absolute path outside the install root
    /// passes check 1: a parallel implementer's worktree holds no copy of the
    /// workspace, so it cites its captures by absolute path. Only check 1 runs,
    /// so the pin holds wherever the temporary directory lies; the ephemeral-path
    /// check names a path below `/tmp/`, which the integration test of the same
    /// name covers.
    #[test]
    fn a_capture_cited_by_an_absolute_path_outside_the_install_root_passes() {
        let (dir, head) = init_scratch_repo("check-report-claims-capture-absolute-");
        let outside = tempfile::Builder::new()
            .prefix("check-report-claims-capture-outside-")
            .tempdir()
            .expect("a folder outside the install root");
        let mut run = clean_run(outside.path(), "red.txt", &["cargo", "test"], "FAILED\n");
        run["capture"] = json!(
            outside
                .path()
                .join("red.txt")
                .to_str()
                .expect("a UTF-8 temp path")
        );
        let mut errors = Vec::new();
        check_captures(
            dir.path(),
            &collect_runs(&report_with_test(&head, run)),
            &mut errors,
        );
        assert_eq!(errors, Vec::<String>::new());
    }

    /// A capture in a folder whose name holds a space and a non-ASCII
    /// character resolves against the install root, and the line for a missing
    /// one names the path as the report spells it (review focus 4).
    #[test]
    fn a_capture_in_a_folder_with_a_space_and_a_non_ascii_name_resolves() {
        let (dir, head) = init_scratch_repo("check-report-claims-capture-unicode-");
        let rel = "évidence dir/red.txt";
        let run = clean_run(dir.path(), rel, &["cargo", "test"], "FAILED\n");
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run.clone())),
            Vec::<String>::new()
        );
        std::fs::remove_file(dir.path().join(rel)).expect("remove the capture");
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            [format!("tests[0]: capture {rel} does not exist")]
        );
    }

    // ---- check 2: the excerpt of a run ----

    /// A run that cites a capture with `output`, with `excerpt` as its excerpt.
    fn run_with_excerpt(dir: &Path, output: &str, excerpt: &str) -> Value {
        let mut run = clean_run(dir, "evidence/run.txt", &["cargo", "test"], output);
        run["excerpt"] = json!(excerpt);
        run
    }

    /// Pin: an excerpt that the output holds byte for byte passes, at the start,
    /// in the middle, and at the end of the output.
    #[test]
    fn an_excerpt_inside_the_output_passes() {
        let (dir, head) = init_scratch_repo("check-report-claims-excerpt-ok-");
        for excerpt in ["running 3 tests", "1 passed; 1 failed", "done"] {
            let run = run_with_excerpt(
                dir.path(),
                "running 3 tests\ntest result: FAILED. 1 passed; 1 failed\ndone",
                excerpt,
            );
            assert_eq!(
                findings(dir.path(), &report_with_test(&head, run)),
                Vec::<String>::new(),
                "{excerpt}"
            );
        }
    }

    /// An excerpt that lost one character in its middle is no substring of the
    /// output.
    #[test]
    fn an_excerpt_shortened_by_one_character_in_its_middle_fails() {
        let (dir, head) = init_scratch_repo("check-report-claims-excerpt-short-");
        let run = run_with_excerpt(
            dir.path(),
            "test result: FAILED. 1 passed; 1 failed\n",
            "FAILED. 1 pssed; 1 failed",
        );
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            ["tests[0].excerpt: not a byte-exact substring of the output of evidence/run.txt"]
        );
    }

    /// A capture of a command that printed nothing is the header alone: no
    /// excerpt matches it (review focus 1).
    #[test]
    fn an_excerpt_of_an_empty_capture_fails() {
        let (dir, head) = init_scratch_repo("check-report-claims-excerpt-empty-");
        let run = run_with_excerpt(dir.path(), "", "anything");
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            ["tests[0].excerpt: not a byte-exact substring of the output of evidence/run.txt"]
        );
    }

    /// The header line is no output: text that only the header holds is no
    /// excerpt.
    #[test]
    fn text_of_the_header_is_not_output() {
        let (dir, head) = init_scratch_repo("check-report-claims-excerpt-header-");
        let run = run_with_excerpt(dir.path(), "plain output\n", "houserules/capture@1.0.0");
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            ["tests[0].excerpt: not a byte-exact substring of the output of evidence/run.txt"]
        );
    }

    /// The comparison is on bytes: a CR before the LF counts, and a multi-byte
    /// character matches as itself.
    #[test]
    fn an_excerpt_is_compared_byte_for_byte() {
        let (dir, head) = init_scratch_repo("check-report-claims-excerpt-bytes-");
        let output = "one\r\ntwo\r\nna\u{ef}ve\n";
        for (excerpt, passes) in [
            ("one\r\ntwo", true),
            ("one\ntwo", false),
            ("na\u{ef}ve", true),
            ("naive", false),
        ] {
            let run = run_with_excerpt(dir.path(), output, excerpt);
            let expected: Vec<String> = if passes {
                Vec::new()
            } else {
                vec!["tests[0].excerpt: not a byte-exact substring of the output of evidence/run.txt".to_string()]
            };
            assert_eq!(
                findings(dir.path(), &report_with_test(&head, run)),
                expected,
                "{excerpt:?}"
            );
        }
    }

    /// An excerpt of a capture that cannot be read adds no line of its own:
    /// check 1 names the capture once.
    #[test]
    fn an_excerpt_of_a_missing_capture_adds_no_line() {
        let (dir, head) = init_scratch_repo("check-report-claims-excerpt-missing-");
        let mut run = json!({
            "capture": "evidence/gone.txt",
            "sha256": "0".repeat(64),
            "command": "cargo test",
            "exit": 0,
        });
        run["excerpt"] = json!("anything");
        assert_eq!(
            findings(dir.path(), &report_with_test(&head, run)),
            ["tests[0]: capture evidence/gone.txt does not exist"]
        );
    }

    /// An excerpt that fails is labelled by the place of its run.
    #[test]
    fn an_excerpt_is_labelled_by_the_place_of_its_run() {
        let (dir, head) = init_scratch_repo("check-report-claims-excerpt-labels-");
        let run = run_with_excerpt(dir.path(), "output\n", "not in the output");
        let mut report = base_report(&head);
        report["tests"] = json!([run]);
        report["live_run"] = json!([run]);
        report["tdd"] = json!([{"test": "t", "mode": "reconstructed", "red": run, "green": run}]);
        report["fix_rounds"] = json!([{"round": 1, "findings": [], "commits": [], "tests": [run]}]);
        let tail = ".excerpt: not a byte-exact substring of the output of evidence/run.txt";
        assert_eq!(
            findings(dir.path(), &report),
            [
                "tests[0]",
                "live_run[0]",
                "tdd[0].red",
                "tdd[0].green",
                "fix_rounds[0].tests[0]"
            ]
            .map(|label| format!("{label}{tail}"))
        );
    }
}
