//! The evidence checks of `check-report-claims`. `check_self_review_evidence`
//! holds each `self_review` item to the paths of its `evidence`: every path
//! exists, a claim that says every, all, or no remaining names a capture, and a
//! claim that says pin, pins, or pinned names an existing path.
//! `check_rerun_probes` holds the `reran` path of each fix-round finding to the
//! folder of the report. A lookup that fails for another cause than absence gives
//! the line `could not be read` with the error of the operating system. The
//! parent module doc states the stance of every Limits list.
//!
//! Limits (every doc comment of this file that says "see the module doc's
//! limits" means this list):
//! - The trigger words are lexical: every, all, and no remaining; pin, pins, and
//!   pinned; whole words, in any case. The phrases "at all" and "after all" do
//!   not count as the word all. A claim that states a closed set or a pinned
//!   behavior in other words (each, none, locked) triggers nothing. A claim that
//!   quotes a past statement, or denies one ("not every case"), triggers all the
//!   same: a classified sample of 30 closure claims of the corpus held two clear
//!   non-claims and two borderline process statements.
//! - A path of `evidence` counts as a capture when `capture::read` reads it as
//!   one. The check does not prove that the capture holds the enumeration that
//!   the claim describes: a capture of an unrelated command passes. A path
//!   counts as existing when the operating system finds it, a file or a
//!   directory. Only a path that the system does not find reads as `does not
//!   exist`; a path that the system refuses to look up gets a line of its own
//!   (`lookup_failure`) and does not count as existing. A file that exists but
//!   that the system refuses to read counts as existing and as no capture.
//! - An `evidence` path and a `reran` path resolve as a `capture` path does:
//!   absolute, or joined onto the install root (`resolve_against`), without a
//!   line suffix. A path with a line suffix does not exist.
//! - `check_rerun_probes` compares canonical paths, so a symbolic link in the
//!   folder of the report that leads out of it is outside. A path that cannot be
//!   made canonical gets the line of `lookup_failure`. The folder is the one that
//!   holds the report file as it was resolved; a report given by a symbolic link
//!   is judged by the folder of the link.
//! - A `self_review` item that is no object with a string `claim`, an `evidence`
//!   entry that is no string, and a `reran` that is no string are skipped
//!   (`validate` refuses them).

use std::io::ErrorKind;
use std::path::Path;
use std::sync::LazyLock;

use regress::Regex;
use serde_json::Value;

use super::resolve_against;
use crate::capture;

/// The closure words of a claim: every, all, and no remaining, as whole words in
/// any case.
static CLOSURE_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::with_flags(r"\b(?:every|all|no\s+remaining)\b", "i").expect("valid closure-word pattern")
});

/// The pin words of a claim: pin, pins, and pinned, as whole words in any case.
static PIN_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::with_flags(r"\bpin(?:s|ned)?\b", "i").expect("valid pin-word pattern"));

/// The first closure word in `claim` as the claim spells it, with each run of
/// white space as one space, so that a finding stays on one line. The word all
/// does not count after at or after ("at all", "after all").
fn closure_word(claim: &str) -> Option<String> {
    CLOSURE_WORD.find_iter(claim).find_map(|found| {
        let word = &claim[found.range()];
        let excepted =
            word.eq_ignore_ascii_case("all") && follows_at_or_after(&claim[..found.start()]);
        (!excepted).then(|| letters_set_apart_by_one_space(word))
    })
}

/// The first pin word in `claim`, as the claim spells it.
fn pin_word(claim: &str) -> Option<&str> {
    PIN_WORD.find(claim).map(|found| &claim[found.range()])
}

/// The letters of `word`, a closure word, set apart by one space where the claim
/// holds white space. A closure word holds nothing but letters and white space.
fn letters_set_apart_by_one_space(word: &str) -> String {
    word.split(|c: char| !c.is_alphabetic())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The line for a `path` that the lookup of `label` could not find. Only a path
/// that is not found does not exist; any other failure shows the error of the
/// operating system.
fn lookup_failure(label: &str, path: &str, error: &std::io::Error) -> String {
    if error.kind() == ErrorKind::NotFound {
        format!("{label}: {path} does not exist")
    } else {
        format!("{label}: {path} could not be read ({error})")
    }
}

/// Whether the last word of `before` is at or after, set apart from the text
/// that follows by white space.
fn follows_at_or_after(before: &str) -> bool {
    let trimmed = before.trim_end();
    let set_apart = trimmed.len() < before.len();
    let last_word = trimmed
        .rsplit(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or_default();
    set_apart && (last_word.eq_ignore_ascii_case("at") || last_word.eq_ignore_ascii_case("after"))
}

/// What the `evidence` of one `self_review` item holds, after the findings of
/// its missing paths.
struct Evidence {
    /// The number of paths that exist.
    existing: usize,
    /// The number of paths that are captures.
    captures: usize,
}

/// Reads the `evidence` of item `index`, adds a line for each path that the
/// lookup does not find or fails on, and counts the paths that exist and the ones
/// that are captures.
fn read_evidence(root: &Path, index: usize, item: &Value, errors: &mut Vec<String>) -> Evidence {
    let mut evidence = Evidence {
        existing: 0,
        captures: 0,
    };
    let paths = item.get("evidence").and_then(Value::as_array);
    for (position, path) in paths.into_iter().flatten().enumerate() {
        let Some(path) = path.as_str() else {
            continue;
        };
        let resolved = resolve_against(root, path);
        if let Err(error) = std::fs::metadata(&resolved) {
            let label = format!("self_review[{index}].evidence[{position}]");
            errors.push(lookup_failure(&label, path, &error));
            continue;
        }
        evidence.existing += 1;
        if capture::read(&resolved).is_ok() {
            evidence.captures += 1;
        }
    }
    evidence
}

/// Check 5 (HR-204 (1), (2)). Every `evidence` path of a `self_review` item
/// exists. An item whose `claim` holds the word every or all, or the phrase no
/// remaining, names at least one capture in `evidence`; an item whose `claim`
/// holds pin, pins, or pinned names at least one existing path. The lines of an
/// item come in this order: its missing paths, the closure line, the pin line.
/// See the module doc's Limits for the trigger words.
pub(super) fn check_self_review_evidence(root: &Path, report: &Value, errors: &mut Vec<String>) {
    let Some(items) = report.get("self_review").and_then(Value::as_array) else {
        return;
    };
    for (index, item) in items.iter().enumerate() {
        let Some(claim) = item.get("claim").and_then(Value::as_str) else {
            continue;
        };
        let evidence = read_evidence(root, index, item, errors);
        if evidence.captures == 0
            && let Some(word) = closure_word(claim)
        {
            errors.push(format!(
                "self_review[{index}]: claims \"{word}\", but its evidence names no capture"
            ));
        }
        if evidence.existing == 0
            && let Some(word) = pin_word(claim)
        {
            errors.push(format!(
                "self_review[{index}]: claims \"{word}\", but its evidence names no existing path"
            ));
        }
    }
}

/// Check 6 (HR-204 (4)). Every `reran` path of a fix-round finding exists and
/// lies in the folder that holds the report (`report_path`) or below it. Both
/// sides are canonical paths, so a `..` segment or a link cannot lead out of the
/// folder. A path that the system fails to make canonical for another cause than
/// absence gets the line of `lookup_failure`.
pub(super) fn check_rerun_probes(
    root: &Path,
    report_path: &Path,
    report: &Value,
    errors: &mut Vec<String>,
) {
    let Some(rounds) = report.get("fix_rounds").and_then(Value::as_array) else {
        return;
    };
    let workspace = report_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let canonical_workspace =
        std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
    for (round, entry) in rounds.iter().enumerate() {
        let findings = entry.get("findings").and_then(Value::as_array);
        for (finding, item) in findings.into_iter().flatten().enumerate() {
            let Some(path) = item.get("reran").and_then(Value::as_str) else {
                continue;
            };
            let label = format!("fix_rounds[{round}].findings[{finding}].reran");
            match std::fs::canonicalize(resolve_against(root, path)) {
                Err(error) => errors.push(lookup_failure(&label, path, &error)),
                Ok(canonical) if !canonical.starts_with(&canonical_workspace) => {
                    errors.push(format!(
                        "{label}: {path} is not in the workspace {}",
                        workspace.display()
                    ));
                }
                Ok(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::report_claims::tests::{clean_run, init_scratch_repo};

    /// The findings of `check_self_review_evidence` for `items` in a scratch
    /// install that holds a source file, a folder, and a capture.
    fn evidence_findings(items: &Value) -> Vec<String> {
        let (dir, _head) = init_scratch_repo("check-report-claims-evidence-");
        std::fs::write(dir.path().join("source.rs"), "fn main() {}\n").unwrap();
        std::fs::create_dir(dir.path().join("folder")).unwrap();
        clean_run(dir.path(), "run.txt", &["true"], "ok\n");
        let mut errors = Vec::new();
        check_self_review_evidence(dir.path(), &json!({ "self_review": items }), &mut errors);
        errors
    }

    /// An item of the old form (a plain string), an item without a string
    /// claim, and an evidence entry that is no string add no finding and keep
    /// the position of the entries after them.
    #[test]
    fn what_the_schema_refuses_is_skipped_and_keeps_the_positions() {
        let errors = evidence_findings(&json!([
            "Every old-form item",
            {"claim": 3, "evidence": ["missing.txt"]},
            {"claim": "ok", "evidence": [7, "missing.txt"]},
        ]));
        assert_eq!(
            errors,
            ["self_review[2].evidence[1]: missing.txt does not exist"]
        );
    }

    /// A folder is an existing path for a pin claim but no capture for a
    /// closure claim, and a capture serves both.
    #[test]
    fn a_folder_is_an_existing_path_and_a_capture_serves_both_claims() {
        let errors = evidence_findings(&json!([
            {"claim": "It pins the shape", "evidence": ["folder"]},
            {"claim": "Every case is covered", "evidence": ["folder"]},
            {"claim": "Every case pins the shape", "evidence": ["run.txt"]},
            {"claim": "A claim that says nothing of the kind", "evidence": []},
        ]));
        assert_eq!(
            errors,
            ["self_review[1]: claims \"Every\", but its evidence names no capture"]
        );
    }

    /// A claim that says all in a longer word, or after at or after, and no
    /// pin word needs no evidence.
    #[test]
    fn a_claim_without_a_trigger_word_needs_no_evidence() {
        let errors = evidence_findings(&json!([
            {"claim": "The installation passed, after all"},
            {"claim": "It is not needed at all"},
            {"claim": "The spinning wheel and the pinhole"},
        ]));
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// The word is the text as the claim spells it, whole words only, in any
    /// case; each run of white space inside the phrase is one space.
    #[test]
    fn a_closure_word_is_a_whole_word_in_any_case() {
        let word = closure_word;
        assert_eq!(word("Every site").as_deref(), Some("Every"));
        assert_eq!(word("ALL sites").as_deref(), Some("ALL"));
        assert_eq!(
            word("There is No  Remaining caller").as_deref(),
            Some("No Remaining")
        );
        assert_eq!(
            word("There is no\r\n\tremaining caller").as_deref(),
            Some("no remaining")
        );
        assert_eq!(word("the overall install, a ball, an allocator"), None);
        assert_eq!(word("everyone and everything"), None);
        assert_eq!(word("none remaining"), None);
    }

    /// The word is the text as the claim spells it, whole words only, in any
    /// case.
    #[test]
    fn a_pin_word_is_a_whole_word_in_any_case() {
        assert_eq!(pin_word("It pins the shape"), Some("pins"));
        assert_eq!(pin_word("PINNED and Pin"), Some("PINNED"));
        assert_eq!(pin_word("Pin it"), Some("Pin"));
        assert_eq!(pin_word("a spinning wheel, a pinhole, pinning"), None);
    }

    /// Only a path that is not found does not exist; every other failure of the
    /// lookup quotes its error.
    #[test]
    fn only_a_path_that_is_not_found_reads_as_absent() {
        let line = |kind| {
            lookup_failure(
                "label",
                "x.txt",
                &std::io::Error::new(kind, "the error of the system"),
            )
        };
        assert_eq!(line(ErrorKind::NotFound), "label: x.txt does not exist");
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::InvalidInput,
            ErrorKind::NotADirectory,
        ] {
            assert_eq!(
                line(kind),
                "label: x.txt could not be read (the error of the system)"
            );
        }
    }

    /// "at all" and "after all" do not count as the word all; another all in
    /// the same claim still does.
    #[test]
    fn at_all_and_after_all_are_not_the_word_all() {
        let word = closure_word;
        assert_eq!(word("Needed at all"), None);
        assert_eq!(word("Fine, after all."), None);
        assert_eq!(word("AT ALL sites"), None);
        assert_eq!(word("not at all and all of them").as_deref(), Some("all"));
        assert_eq!(word("in all cases").as_deref(), Some("all"));
        assert_eq!(word("flat all").as_deref(), Some("all"));
        assert_eq!(word("at-all").as_deref(), Some("all"));
    }
}
