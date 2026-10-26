//! The wording gate of the per-change review layer (spec 3 and case 10 of
//! section 21): no user-facing text of the kit or the binary says "pull
//! request" without "merge request", and none names the request with the
//! GitHub abbreviation "PR", because a request is a GitHub pull request or a
//! GitLab merge request, and a text says "pull or merge request".
//!
//! The scope is derived, not listed (`quality.gates-derive-their-scope`): every
//! file that git tracks below `template/` (the files `init` and `update` write
//! into a project, the packs and the skills among them) and below `ci/` (the
//! published GitLab unit), the README, the upgrade guide, and the help of the
//! binary: `houserules --help` and the help of every subcommand, found by
//! reading the `Commands:` block of each help text down the tree. A file that
//! cannot be read is a named failure, never a skip.
//!
//! Spec 21.10 names the exceptions. Each one is a row of [`EXCEPTIONS`] with its
//! reason, and the gate fails for a row that excuses nothing:
//!
//! - The published GitHub unit lives at `.github/workflows/`, outside the
//!   scope, so it needs no row.
//! - A GitHub-only line of the upgrade guide, of the finishing skill, or of the
//!   help of the binary has a row. The abbreviation "PR" is a word of its own:
//!   the pattern reads it between word boundaries, in capitals, so that
//!   `gh pr create` and `PR_NUMBER` never match.
//! - The names of GitHub's own files (`pull_request_template.md`) and events
//!   (`pull_request`) hold an underscore, so the pattern for the two words never
//!   reaches them; [`github_names_hold_an_underscore`] pins it.

mod help_support;
mod tracker_support;

use std::cell::Cell;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::process::Command;

use help_support::{folded, repo_file};
use regress::Regex;

/// The published GitLab unit, a kit text outside `template/`.
const GITLAB_UNIT: &str = "ci/gitlab-review.yml";

/// The finishing skill of the reference workflow.
const FINISHING_SKILL: &str =
    "template/workflows/reference/.claude/skills/finishing-a-feature/SKILL.md";

/// A text that may say "pull request" or "PR" alone (spec 21.10).
struct Exception {
    /// The text that holds the words: a path relative to the repository root,
    /// or the command that printed the help.
    file: &'static str,
    /// The words of the text, with white space folded.
    phrase: &'static str,
    /// Why the text is GitHub-only.
    reason: &'static str,
}

/// The exceptions of the gate. A row that excuses no hit fails the gate.
const EXCEPTIONS: [Exception; 7] = [
    Exception {
        file: "docs/upgrading-to-2.md",
        phrase: "checks out the head commit of a pull request",
        reason: "the seeded workflow is a GitHub Actions file, and the hand step of this paragraph names the `pull_request` event of GitHub",
    },
    Exception {
        file: "docs/upgrading-to-2.md",
        phrase: "a CI workflow that does not check out the PR head",
        reason: "the heading names the hand step for the seeded GitHub Actions file, whose checkout reads the head of a GitHub pull request",
    },
    Exception {
        file: "docs/upgrading-to-2.md",
        phrase: "lacks the PR-head checkout; add ref:",
        reason: "the line is the text that `update` prints for the seeded GitHub Actions file",
    },
    Exception {
        file: "docs/upgrading-to-2.md",
        phrase: "the read-only token, the PR-head checkout, and the persisted credential",
        reason: "the list names the markers of the seeded GitHub Actions file, in the order of the file",
    },
    Exception {
        file: FINISHING_SKILL,
        phrase: "release-please PR",
        reason: "release-please opens its release request on GitHub only, and the step names that pull request",
    },
    Exception {
        file: "houserules --help",
        phrase: "or the PR-head checkout",
        reason: "the sentence is the line of `update` about the seeded GitHub Actions file",
    },
    Exception {
        file: "houserules update --help",
        phrase: "or the PR-head checkout",
        reason: "the sentence is the line of `update` about the seeded GitHub Actions file",
    },
];

/// One text under the gate: where it comes from and its content.
struct Text {
    /// A path relative to the repository root, or the command that printed it.
    name: String,
    content: String,
}

/// This checkout's repository root.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The paths that git tracks below `prefix`, relative to the repository root.
/// A failure of git is a failure of the gate.
fn tracked(prefix: &str) -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-files", "-z", "--", prefix])
        .current_dir(repo_root())
        .env("LC_ALL", "C")
        .output()
        .expect("run git ls-files");
    assert!(
        output.status.success(),
        "git ls-files {prefix}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let paths: Vec<String> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            String::from_utf8(path.to_vec())
                .unwrap_or_else(|_| panic!("a tracked path below {prefix} is not UTF-8"))
        })
        .collect();
    assert!(!paths.is_empty(), "git tracks no file below {prefix}");
    paths
}

/// The text of the tracked file `relative`. A file that is not UTF-8 text is a
/// named failure.
fn read_text(relative: &str) -> Text {
    let bytes = fs::read(repo_file(relative)).unwrap_or_else(|error| panic!("{relative}: {error}"));
    let content = String::from_utf8(bytes)
        .unwrap_or_else(|_| panic!("{relative} is no UTF-8 text; the gate cannot read it"));
    Text {
        name: relative.to_string(),
        content,
    }
}

/// What `houserules <args>` prints on stdout. The command exits 0.
fn printed(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_houserules"))
        .env("HOUSERULES_SKIP_SELF_UPDATE", "1")
        .args(args)
        .output()
        .expect("run houserules");
    assert_eq!(output.status.code(), Some(0), "{args:?} exits 0");
    String::from_utf8(output.stdout).expect("UTF-8 help")
}

/// The command paths of the help tree below `path`, depth first: each
/// subcommand that the `Commands:` block of `houserules <path> --help` lists
/// (the `help` subcommand left out), then its own subcommands.
fn help_tree(path: &[String]) -> Vec<Vec<String>> {
    let mut args: Vec<&str> = path.iter().map(String::as_str).collect();
    args.push("--help");
    let help = printed(&args);
    let mut lines = help.lines().skip_while(|line| *line != "Commands:");
    lines.next();
    let mut found = Vec::new();
    for line in lines.take_while(|line| !line.is_empty()) {
        let Some(name) = line.split_whitespace().next() else {
            continue;
        };
        if name == "help" {
            continue;
        }
        let mut child = path.to_vec();
        child.push(name.to_string());
        found.push(child.clone());
        found.extend(help_tree(&child));
    }
    found
}

/// Every text under the gate.
fn corpus() -> Vec<Text> {
    let mut texts: Vec<Text> = ["template", "ci"]
        .iter()
        .flat_map(|prefix| tracked(prefix))
        .map(|file| read_text(&file))
        .collect();
    for file in ["README.md", "docs/upgrading-to-2.md"] {
        assert!(
            !tracked(file).is_empty(),
            "{file} is no tracked file of the gate's scope"
        );
        texts.push(read_text(file));
    }
    let mut commands = vec![Vec::new()];
    commands.extend(help_tree(&[]));
    for command in commands {
        let mut args: Vec<&str> = command.iter().map(String::as_str).collect();
        args.push("--help");
        texts.push(Text {
            name: format!("houserules {}", args.join(" ")),
            content: printed(&args),
        });
    }
    texts
}

/// The offsets in `text` where `phrase` starts.
fn offsets_of(text: &str, phrase: &str) -> Vec<(usize, usize)> {
    text.match_indices(phrase)
        .map(|(start, found)| (start, start + found.len()))
        .collect()
}

/// The byte ranges of `flat` that the gate reads: a "pull request" (any case,
/// any white space between the words, singular or plural) that no "merge
/// request" follows as its alternative, and the abbreviation "PR" (capitals,
/// between word boundaries), in the order of the text.
fn hits_in(flat: &str) -> Vec<Range<usize>> {
    let words = Regex::with_flags(r"pull\s+requests?", "i").expect("the pattern compiles");
    let alternative = Regex::with_flags(r"^\s+(or|/)\s+(a\s+|the\s+)?merge\s+requests?", "i")
        .expect("the pattern compiles");
    let abbreviation = Regex::new(r"\bPRs?\b").expect("the pattern compiles");
    let mut hits: Vec<Range<usize>> = words
        .find_iter(flat)
        .map(|hit| hit.range())
        .filter(|range| alternative.find(&flat[range.end..]).is_none())
        .collect();
    hits.extend(abbreviation.find_iter(flat).map(|hit| hit.range()));
    hits.sort_by_key(|range| range.start);
    hits
}

/// The findings in `text`: each hit of [`hits_in`] that no exception for this
/// text excuses. `used` records each exception that excused a hit.
fn findings_in(text: &Text, used: &[Cell<bool>]) -> Vec<String> {
    let flat = folded(&text.content);
    let mut found = Vec::new();
    for range in hits_in(&flat) {
        let excusing = EXCEPTIONS.iter().position(|exception| {
            exception.file == text.name
                && offsets_of(&flat, exception.phrase)
                    .iter()
                    .any(|(start, end)| *start <= range.start && range.end <= *end)
        });
        if let Some(row) = excusing {
            used[row].set(true);
            continue;
        }
        let from = flat[..range.start]
            .char_indices()
            .rev()
            .nth(40)
            .map_or(0, |(at, _)| at);
        let to = flat[range.end..]
            .char_indices()
            .nth(40)
            .map_or(flat.len(), |(at, _)| range.end + at);
        found.push(format!("{}: ...{}...", text.name, &flat[from..to]));
    }
    found
}

/// The flags of [`EXCEPTIONS`] that start unused.
fn unused_flags() -> Vec<Cell<bool>> {
    EXCEPTIONS.iter().map(|_| Cell::new(false)).collect()
}

/// The failure message of the gate for `findings`: each names the request with "pull request" or
/// "PR" and no "merge request".
fn failure_message(findings: &[String]) -> String {
    format!(
        "{} text(s) name the request with \"pull request\" or \"PR\" and no \"merge request\":\n{}",
        findings.len(),
        findings.join("\n")
    )
}

/// Spec 3 and case 10 of section 21: every text of the scope says "pull or merge
/// request" in place of "pull request" or "PR" alone, or names GitHub alone on a
/// row of [`EXCEPTIONS`]. The gate fails for a row that excuses no hit.
#[test]
fn no_user_facing_text_says_pull_request_or_pr_without_merge_request() {
    let used = unused_flags();
    let findings: Vec<String> = corpus()
        .iter()
        .flat_map(|text| findings_in(text, &used))
        .collect();
    assert!(findings.is_empty(), "{}", failure_message(&findings));
    for (exception, used) in EXCEPTIONS.iter().zip(&used) {
        assert!(
            used.get(),
            "the exception for {} excuses no hit ({}): delete the row",
            exception.file,
            exception.reason
        );
    }
}

/// The failure message names each form that the gate reads: the two words
/// "pull request" and the abbreviation "PR", each without "merge request".
#[test]
fn the_failure_message_names_both_forms_that_the_gate_reads() {
    let text = Text {
        name: "scratch.md".to_string(),
        content: "Open the PR.".to_string(),
    };
    let findings = findings_in(&text, &unused_flags());
    assert_eq!(findings.len(), 1, "{findings:?}");
    let message = failure_message(&findings);
    for form in ["\"pull request\"", "\"PR\"", "\"merge request\""] {
        assert!(
            message.contains(form),
            "the message lacks {form}: {message}"
        );
    }
}

/// The scope comes from git and from the help tree: every tracked file below
/// `template/` and below `ci/` is read, the README and the upgrade guide are
/// read, and the help of the root, of every top-level command, and of the
/// subcommands of the review layer is read.
#[test]
fn the_scope_is_derived_from_the_tracked_files_and_from_the_help_tree() {
    let texts = corpus();
    let names: Vec<&str> = texts.iter().map(|text| text.name.as_str()).collect();
    for file in tracked("template") {
        assert!(names.contains(&file.as_str()), "{file} is out of scope");
    }
    for file in tracked("ci") {
        assert!(names.contains(&file.as_str()), "{file} is out of scope");
    }
    assert!(
        names.contains(&GITLAB_UNIT),
        "the published GitLab unit is out of scope"
    );
    for file in ["README.md", "docs/upgrading-to-2.md"] {
        assert!(names.contains(&file), "{file} is out of scope");
    }
    assert!(names.contains(&"houserules --help"));
    for command in [
        "evidence", "route", "bundle", "record", "ci", "init", "update",
    ] {
        let name = format!("houserules {command} --help");
        assert!(
            names.contains(&name.as_str()),
            "the help of {command} is out of scope"
        );
    }
    for nested in [
        "houserules ci print --help",
        "houserules ci write --help",
        "houserules bundle check --help",
        "houserules record write --help",
    ] {
        assert!(names.contains(&nested), "{nested} is out of scope");
    }
}

/// The finder: "pull or merge request" and its alternatives pass, "pull request"
/// alone fails in any case, in the plural, and across a line break.
#[test]
fn the_finder_reads_the_two_words_across_white_space_and_case() {
    let finds = |content: &str| {
        let text = Text {
            name: "scratch.md".to_string(),
            content: content.to_string(),
        };
        findings_in(&text, &unused_flags()).len()
    };
    for clean in [
        "Open the pull or merge request.",
        "Pull or merge requests are welcome.",
        "Open a pull request or merge request.",
        "Open a pull request / merge request.",
        "Open a pull request or a merge request.",
        "No request here.",
    ] {
        assert_eq!(finds(clean), 0, "{clean}");
    }
    for flagged in [
        "Open the pull request.",
        "Open the Pull Request.",
        "Pull requests are welcome.",
        "Open the pull\nrequest.",
        "Open the pull    request.",
        "A pull request, and a merge request elsewhere.",
    ] {
        assert_eq!(finds(flagged), 1, "{flagged}");
    }
}

/// The abbreviation: "PR" and "PRs" in capitals, between word boundaries, fail;
/// the command `gh pr create`, a variable such as `PR_NUMBER`, and a word that
/// holds the letters pass.
#[test]
fn the_finder_reads_the_abbreviation_as_a_word() {
    let finds = |content: &str| {
        let text = Text {
            name: "scratch.md".to_string(),
            content: content.to_string(),
        };
        findings_in(&text, &unused_flags()).len()
    };
    for clean in [
        "Run `gh pr create --title t`.",
        "Read $PR_NUMBER and ${PR_NUMBER}.",
        "The PRESENT tense, OPRAH, and GPRS.",
        "A pr in lower case is a word of a shell.",
        "Open the pull or merge request.",
    ] {
        assert_eq!(finds(clean), 0, "{clean}");
    }
    for flagged in [
        "Open the PR.",
        "Open the PRs.",
        "Check out the PR-head commit.",
        "Open a (PR) today.",
        "Open the\nPR here.",
    ] {
        assert_eq!(finds(flagged), 1, "{flagged}");
    }
}

/// The names of GitHub's own files and events hold an underscore, so the
/// pattern never reaches them: no row of [`EXCEPTIONS`] is needed (spec 21.10).
#[test]
fn github_names_hold_an_underscore() {
    let text = Text {
        name: "scratch.md".to_string(),
        content: "Write `.github/pull_request_template.md` and `PULL_REQUEST_TEMPLATE/`; the \
                  `pull_request` and `pull_request_target` events start the run."
            .to_string(),
    };
    assert!(findings_in(&text, &unused_flags()).is_empty());
}

/// A disclosed mutation: a scratch copy of the finishing skill, and one of the
/// published GitLab unit, is clean; the same copy with "pull request" added, and
/// with "PR" added, is a finding that names the file. The mutation proves that
/// the gate can fail, for a skill and for the unit outside `template/`.
#[test]
fn the_gate_flags_a_request_word_added_to_a_scratch_copy_of_a_kit_text() {
    let scratch = tempfile::tempdir().expect("tempdir");
    for (index, file) in [FINISHING_SKILL, GITLAB_UNIT].into_iter().enumerate() {
        let original = read_text(file);
        let copy = scratch.path().join(format!("copy-{index}.md"));
        let scan = |content: &str| {
            fs::write(&copy, content).expect("write the scratch copy");
            let text = Text {
                name: file.to_string(),
                content: fs::read_to_string(&copy).expect("read the scratch copy"),
            };
            findings_in(&text, &unused_flags())
        };
        assert!(
            scan(&original.content).is_empty(),
            "the control copy of {file} is clean"
        );
        for added in ["Open the pull request.", "Open the PR."] {
            let findings = scan(&format!("{}\n{added}\n", original.content));
            assert_eq!(findings.len(), 1, "{file} with {added:?}: {findings:?}");
            assert!(findings[0].starts_with(&format!("{file}: ")));
        }
    }
}
