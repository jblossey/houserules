//! The README as an adopter funnel (`writing-style.readme-is-an-adopter-funnel`). The owner reads
//! the README before a release, so each test states one thing that the reader sees and ties it to a
//! run of the binary or to a file of the tree:
//!
//! - the line under the badges is the description of the crate, and the first screen says what
//!   houserules does, what it needs, what is optional, when it fits, and how it sits beside other
//!   tools;
//! - the sections come in the order of the funnel (install, usage, the features, the limits, the
//!   concepts, then the documentation), and the table of contents lists them;
//! - each term that the README uses before its Concepts section has an entry there, and each entry
//!   there names a term that the rest of the README uses;
//! - the reference text lives in pages under `docs/`: each page holds the sections that left the
//!   README, the README holds none of them but a short Updating section that links its page, and
//!   its Documentation section links each page once;
//! - each feature section links the page that holds its detail;
//! - the setup section names each file that `init` writes, and `init` writes no CI file, no git
//!   hook, and no git ref;
//! - the table of check types names each type of the knowledge schema, and the example `check` of
//!   each row passes `check-knowledge` in an install;
//! - the requirements name the oldest git that the commands need, the line that an older git
//!   prints, and the commands whose source checks the version;
//! - the coding agents section names the one value of `--target` that `init` takes;
//! - the CI section names the inputs that both CI units take, the jobs that hold a write scope, the
//!   head that the gates job audits, and the job that posts nothing for a request from a fork;
//! - the intent section and the limits section state what houserules does and does not do;
//! - the CLI reference page lists the commands of `--help`, no more and no fewer;
//! - the ownership table of its page lists the files that `houserules files` prints;
//! - each `sh` block of the quick start, of the intent section, and of the review section runs as
//!   printed, one after the other, in the scratch repository that the quick start makes, prints the
//!   `text` or `json` block that follows it, and changes no file outside that repository;
//! - each other `sh` block of the README or of a reference page sits in a section that another test
//!   runs or that is declared a form with its reason (`quality.gates-derive-their-scope`);
//! - the reference workflow page states the faster task loop in the words of the kit files;
//! - no sentence of the README or of a reference page names an internal item, a ruling, a batch, a
//!   release number, or "this release", and every link of those files resolves.
//!
//! The Stability section is pinned in `freeze_successor_38.rs`. The `sh` blocks of the evidence page
//! and of the review-layer page run in `help_36.rs` and `workflow_texts.rs`, each in a fresh install.

mod ci_support;
mod docs_support;
mod help_support;
mod tracker_support;

use std::collections::BTreeSet;
use std::fs;

use ci_support::{Host, parse_unit};
use docs_support::{blocks, headings, section};
use help_support::{folded, help_of_args, read, repo_file, states};
use regress::Regex;
use serde_json::Value;
use tracker_support::{commit_all, git, houserules, repo_root, run, run_ok, scratch_git_repo};

/// The one line under the badges. It is under 120 characters, and the crate's `description` in
/// `crates/houserules/Cargo.toml` is the same text.
const ONE_LINER: &str = "A CLI that makes your AI coding agents adhere to your rules, audits each change, and proves your intent is obeyed.";

/// The manifest whose `description` the line under the badges equals.
const CRATE_MANIFEST: &str = "crates/houserules/Cargo.toml";

/// The statements of the first screen: the problem, what houserules is made of and needs, the core
/// and its optional layers, how it sits beside the tools that a reader may know, when it fits, and
/// where its limits are.
const FIRST_SCREEN: [&str; 14] = [
    "AI coding agents forget a project's rules between sessions, and nobody can review every line they write.",
    "houserules is one binary and plain files in your repository.",
    "It needs no server, database, account, or hosted service.",
    "That is the core. Everything else is an optional layer that you add when you need it:",
    "You can stop after the [quick start](#quick-start).",
    "It works beside other tools:",
    "houserules writes `AGENTS.md` and the files of Claude Code, and it adds a check to a rule.",
    "The checks of houserules match patterns, paths, diffs, and commit messages, and a check can run such a tool as a [runner](#checks-that-run-your-tools).",
    "houserules supplies the rules of each change and [links a commit to its plan](#linking-a-change-to-its-intent).",
    "houserules keeps what each check found and a record of each merge.",
    "houserules gives it a check to require, and it blocks nothing itself.",
    "It fits a repository where agents change code every day and every agent, tool, and session must follow the same rules.",
    "For one person, one agent, and a short `AGENTS.md`, houserules is likely more than you need.",
    "[Limits](#limits) lists what it does not do.",
];

/// The level-two headings of the README, in the order of the funnel.
const FUNNEL: [&str; 14] = [
    "Table of contents",
    "Install",
    "Quick start",
    "Rules and checks",
    "Integration",
    "Linking a change to its intent",
    "Reviewing a change",
    "A workflow for a team of agents",
    "Limits",
    "Concepts",
    "Documentation",
    "Stability",
    "Contributing and support",
    "License",
];

/// The terms of the Concepts section, in its alphabetical order. Each is the bold term of one
/// entry.
const CONCEPTS: [&str; 26] = [
    "area",
    "audit row",
    "capture",
    "change record",
    "check",
    "entry",
    "evidence bundle",
    "evidence packet",
    "harness",
    "install root",
    "intent",
    "judged",
    "kit",
    "override, exclusion",
    "pack",
    "render",
    "route",
    "rule package",
    "runner",
    "sanction",
    "standing rule",
    "topic",
    "tracker",
    "trailer",
    "uncovered surface",
    "work item",
];

/// The terms that the README must define in Concepts, because its prose uses each of them before the
/// section. Each is a term of [`CONCEPTS`].
const UNDEFINED_BEFORE: [&str; 12] = [
    "harness",
    "install root",
    "area",
    "standing rule",
    "pack",
    "check",
    "tracker",
    "intent",
    "evidence packet",
    "route",
    "uncovered surface",
    "sanction",
];

/// A page under `docs/` that holds reference text that left the README.
struct Page {
    /// The path of the page, relative to the repository root.
    path: &'static str,
    /// The level-one heading on the first line of the page, its only one.
    title: &'static str,
    /// The titles of the level-two headings of the page, in order.
    sections: &'static [&'static str],
}

/// The reference pages, in the order of the Documentation section of the README.
const PAGES: [Page; 10] = [
    Page {
        path: "docs/updating.md",
        title: "Updating and uninstalling",
        sections: &["Updating", "Uninstalling"],
    },
    Page {
        path: "docs/subdirectory-install.md",
        title: "A subdirectory install",
        sections: &[],
    },
    Page {
        path: "docs/trackers.md",
        title: "Trackers",
        sections: &["Deferrals", "Trailers"],
    },
    Page {
        path: "docs/evidence.md",
        title: "The houserules step and evidence",
        sections: &["The houserules step", "Evidence"],
    },
    Page {
        path: "docs/runners.md",
        title: "Runners",
        sections: &[],
    },
    Page {
        path: "docs/review-layer.md",
        title: "The review layer",
        sections: &[],
    },
    Page {
        path: "docs/ownership.md",
        title: "Ownership model",
        sections: &[],
    },
    Page {
        path: "docs/reference-workflow.md",
        title: "The reference workflow",
        sections: &["The task loop"],
    },
    Page {
        path: "docs/process-cost.md",
        title: "Measure process cost",
        sections: &[],
    },
    Page {
        path: "docs/cli.md",
        title: "CLI reference",
        sections: &[],
    },
];

/// The section that the README keeps for a moved topic, and the page that the section links. The
/// heading stays because the doc comment of the update check names the variable that turns the
/// check off "in this crate's README under Updating".
const UPDATING_STUB: (&str, &str) = ("### Updating", "docs/updating.md");

/// The variable that turns the update check off. The stub of [`UPDATING_STUB`] names it.
const NO_UPDATE_CHECK: &str = "`HOUSERULES_NO_UPDATE_CHECK=1`";

/// Each feature section of the README, and the page under `docs/` that holds its detail and that
/// the section links.
const FEATURE_PAGES: [(&str, &str); 10] = [
    ("### Setting up your repository", "docs/ownership.md"),
    (
        "### Setting up your repository",
        "docs/updating.md#uninstalling",
    ),
    ("### Checks that run your tools", "docs/runners.md"),
    ("### Shared rule sets", "docs/upgrading-to-2.md#packs"),
    ("### Coding agents", "docs/upgrading-to-2.md#render-targets"),
    ("### CI", "docs/review-layer.md"),
    ("### Work items", "docs/trackers.md"),
    ("## Linking a change to its intent", "docs/openspec.md"),
    ("## Reviewing a change", "docs/review-layer.md"),
    (
        "## A workflow for a team of agents",
        "docs/reference-workflow.md",
    ),
];

/// The statements of the requirements section.
const REQUIREMENTS: [&str; 4] = [
    "`route`, `evidence`, `bundle check`, and `record write` read the attributes of a commit with `git --attr-source`, so they need git 2.41 or later.",
    "`audit` needs it only when an `intent-required` check must find an intent in the changed paths.",
    "With an older git, the command stops with `<command>: git <version> is older than 2.41, which --attr-source needs`.",
    "**`gh`** for the GitHub tracker, and **`bd`** for the Beads tracker.",
];

/// The line that an older git gives, as the source formats it. The requirements show it with
/// `<command>` and `<version>` in place of the two values and the floor filled in.
const GIT_FLOOR_FORMAT: &str =
    "\"{command}: git {version} is older than {}.{}, which --attr-source needs\"";

/// The floor of the git version, as the source declares it.
const GIT_FLOOR_CONSTANT: &str = "const MINIMUM_VERSION: (u64, u64) = (2, 41);";

/// The commands that need git 2.41 or later, as the README names them. `audit` is the one whose
/// help does not name the floor, because only an `intent-required` check reads it.
const GIT_FLOOR_COMMANDS: [&str; 5] =
    ["audit", "bundle check", "evidence", "record write", "route"];

/// The subsections of the integration section, in order.
const INTEGRATION: [&str; 3] = ["Coding agents", "CI", "Work items"];

/// The statements of the integration section about the CI units.
const INTEGRATION_STATEMENTS: [&str; 5] = [
    "`init` writes no CI file.",
    "Make the gates job a required check in the branch protection of your host, so that a red audit blocks the merge: houserules itself merges nothing and blocks nothing.",
    "Two jobs hold the write scopes, and neither runs houserules or the code of the repository:",
    "It checks out the head of the request, so it audits the request with the rules of the request itself.",
    "On a request from a fork, `report` posts nothing.",
];

/// The jobs of the GitHub unit that hold a write scope, as the CI section states it.
const WRITING_JOBS: [&str; 2] = ["record-push", "report"];

/// The condition of the GitHub unit under which the `report` job runs: a request from the
/// repository itself, never from a fork.
const SAME_REPOSITORY: &str = "github.event.pull_request.head.repo.full_name == github.repository";

/// The commit that the `gates` job of the GitHub unit checks out: the head of the request.
const REQUEST_HEAD: &str = "${{ github.event.pull_request.head.sha }}";

/// The notice of the GitLab unit for a request from a fork, after which its `report` job posts
/// nothing.
const FORK_NOTICE: &str = "this merge request comes from a fork, so this run posts nothing";

/// The sentence of the integration section that says when `render` writes `.claude/rules/`.
const CLAUDE_RULES_SENTENCE: &str = "`houserules init --target claude` adds the files of Claude Code: one rule file for each area in `.claude/rules/`,";

/// The values of `--target` that the coding agents section names, as `init --help` lists them.
const TARGETS: [&str; 1] = ["claude"];

/// The statements of the coding agents section about the tools that read its files.
const CODING_AGENTS: [&str; 3] = [
    "Codex, Cursor, and the coding agent of GitHub Copilot read `AGENTS.md`",
    "The one value of `--target` is `claude`:",
    "houserules writes no file in the rule format of another tool, such as `.cursor/rules/` or `.github/copilot-instructions.md`.",
];

/// The statements of the setup section about what `init` writes and leaves alone.
const SETUP: [&str; 4] = [
    "It writes these files and keeps each one that exists:",
    "`init` prints no `wrote` line for it",
    "A `CLAUDE.md` that exists stays as it is: add the line to it, or `check-knowledge` fails.",
    "It writes no CI file, no git hook, and no git ref, and the files of Claude Code come only with `--target claude`.",
];

/// The statements of the intent section about the trailer and OpenSpec.
const INTENT_STATEMENTS: [&str; 5] = [
    "it does not compare the diff with the plan.",
    "Nothing requires the trailer until you add an entry with an `intent-required` check:",
    "houserules reads the `openspec/` folder and edits no OpenSpec file;",
    "A block that OpenSpec or another tool keeps in `AGENTS.md` stays as it is, since `render` writes only between its own markers.",
    "After a squash merge, the record of the merge reads the squash commit, so keep the trailers in its message.",
];

/// The statements of the limits section: what houserules does not do or guarantee.
const LIMITS: [&str; 7] = [
    "houserules enforces only what a check tests. `audit` lists a rule without a check as `open`, and no command lists the rules that carry a check.",
    "Its own checks match patterns, paths, diffs, and commit messages, and they read no meaning of code.",
    "It blocks nothing by itself. A red audit stops a merge only when your host requires the job, and a sanction keeps the audit red.",
    "In CI, a request is audited with its own rules, so a request that changes `knowledge/` changes the rules that judge it.",
    "A repository can override or exclude any entry of a pack, and no lock stops it.",
    "It signs nothing.",
    "houserules does not tell an agent's commit from a person's, and it keeps no prompt and no session, only a trailer that points at one.",
];

/// The sections whose `sh` blocks run as one session, in README order, in one repository.
const SESSION: [&str; 3] = [
    "Quick start",
    "Linking a change to its intent",
    "Reviewing a change",
];

/// The sentence of the quick start that says where it runs.
#[cfg(unix)]
const SCRATCH_SENTENCE: &str =
    "The quick start runs in a new scratch repository, so it changes no file of yours.";

/// The folder of the scratch repository that the first block of the quick start makes and enters.
/// The session runs in `sh`, so this and the other helpers of the session build on unix only.
#[cfg(unix)]
const DEMO: &str = "houserules-demo";

/// The word that the README prints in place of the folder of the scratch repository.
#[cfg(unix)]
const REPOSITORY: &str = "<repository>";

/// The values of a run that the README does not pin, each with the pattern that finds it (an
/// ECMAScript pattern with the flag `m`, so that `^` and `$` match at each line) and the words that
/// the README prints in its place: the `next:` line of `init`, whose commands depend on the state
/// of the repository; the hash of the rules in the head line of a change record, which changes with
/// the kit packs of each release; and an abbreviated commit sha. Each word occurs in a block that
/// the session prints.
#[cfg(unix)]
const RUN_VALUES: [(&str, &str); 3] = [
    (r"^next: .*$", "next: <the commands to run next>"),
    (r"\brules [0-9a-f]{12}\b", "rules <hash>"),
    (r"\b[0-9a-f]{7}\b", "<sha>"),
];

/// `printed` with the folder of the scratch repository, the version of the binary, and each value
/// of [`RUN_VALUES`] replaced by the words that the README prints in its place. `folder` is the
/// folder of the scratch repository as the test made it, and `canonical` the same folder with its
/// links resolved.
#[cfg(unix)]
fn named_values(printed: &str, folder: &str, canonical: &str) -> String {
    let mut named = printed
        .replace(canonical, REPOSITORY)
        .replace(folder, REPOSITORY)
        .replace(env!("CARGO_PKG_VERSION"), "<version>");
    for (pattern, word) in RUN_VALUES {
        let value = Regex::with_flags(pattern, "m").expect("the pattern compiles");
        let mut replaced = String::new();
        let mut last = 0;
        for hit in value.find_iter(&named) {
            replaced.push_str(&named[last..hit.start()]);
            replaced.push_str(word);
            last = hit.end();
        }
        replaced.push_str(&named[last..]);
        named = replaced;
    }
    named
}

/// The sections of the README whose `sh` blocks are forms, each with the reason that it does not
/// run as printed.
const FORMS: [(&str, &str); 2] = [
    (
        "Install",
        "it installs software on the machine and needs the network",
    ),
    (
        "Rules and checks",
        "it lists forms with placeholders and sources on a network",
    ),
];

/// How a test meets the `sh` blocks of a section of a reference page.
enum Shown {
    /// The test file that runs the blocks, each in a fresh install.
    Run(&'static str),
    /// The reason that the blocks are forms, which do not run as printed.
    Form(&'static str),
}

/// The sections of the reference pages that hold an `sh` block: the page, the title of the
/// section (the title of the page for the text above its first level-two heading), and how a test
/// meets its blocks.
const PAGE_BLOCKS: [(&str, &str, Shown); 9] = [
    (
        "docs/updating.md",
        "Updating",
        Shown::Form("it replaces the binary on the machine and needs the network"),
    ),
    (
        "docs/subdirectory-install.md",
        "A subdirectory install",
        Shown::Form("it names a subdirectory of your own project"),
    ),
    (
        "docs/trackers.md",
        "Trackers",
        Shown::Form("it names a tracker and the items of your own install"),
    ),
    (
        "docs/trackers.md",
        "Deferrals",
        Shown::Form("it files a work item of your own tracker"),
    ),
    (
        "docs/evidence.md",
        "The houserules step",
        Shown::Form("it names a base branch and a report of your own"),
    ),
    ("docs/evidence.md", "Evidence", Shown::Run("help_36.rs")),
    (
        "docs/review-layer.md",
        "The review layer",
        Shown::Run("workflow_texts.rs"),
    ),
    (
        "docs/reference-workflow.md",
        "The reference workflow",
        Shown::Form("it installs the whole workflow, which the tests of the workflow install"),
    ),
    (
        "docs/process-cost.md",
        "Measure process cost",
        Shown::Form("it reads the workspaces of your own agents"),
    ),
];

/// The rules of the task loop, as the reference workflow page states them (spec 19).
const TASK_LOOP: [&str; 7] = [
    "- An implementer runs the focused test while it iterates. Before it commits, it runs the tests of the files that its task changes and every other gate of the brief, and it names the targets in its report. The person who runs the work runs the full suite, and every suite that the plan marks as integration-only, once per integration.",
    "- A reading that the spec or the brief leaves open, and that a test would pin, is a question before the implementer writes that test. The implementer stops, names the reading and its choices, and returns NEEDS_CONTEXT. The person who runs the work rules it and resumes the task.",
    "- The ledger of a batch records the start and the close of each dispatch with its UTC time.",
    "- The person who runs the work asks the project's owner or decider in a plain message, never in a question tool that holds its turn. It keeps dispatching every task and fix round that needs no ruling.",
    "- A wave orders the tasks; it is no barrier. A task starts as soon as every task it needs is integrated.",
    "- A plan may name a file that two tasks share, such as a command-line entry point. The two tasks may run at once, and the person who runs the work resolves the later cherry-pick by keeping both sides. Any other resolution is a fix round for the later task.",
    "- While a fix-diff review runs, the person who runs the work may integrate the fix and run the integration gates. A new Critical or Important finding undoes the integration and opens the next round.",
];

/// The phrase of each rule of the task loop that the kit file of the rule carries too, with the file
/// below `template/workflows/reference/`.
const TASK_LOOP_PHRASES: [(&str, &str); 7] = [
    (
        "every other gate of the brief",
        ".claude/agents/implementer.md",
    ),
    ("NEEDS_CONTEXT", ".claude/agents/implementer.md"),
    ("UTC time", ".claude/skills/orchestrating/SKILL.md"),
    (
        "in a plain message, never in a question tool that holds",
        ".claude/skills/orchestrating/SKILL.md",
    ),
    (
        "A wave orders the tasks; it is no barrier.",
        ".claude/skills/orchestrating/SKILL.md",
    ),
    ("keeping both sides", ".claude/skills/planning/SKILL.md"),
    (
        "may integrate the fix and run the integration gates",
        ".claude/skills/orchestrating/SKILL.md",
    ),
];

/// The patterns that a README must not hold, each with what it names (knowledge entry
/// `writing-style.readme-is-an-adopter-funnel`: internal ids, ruling citations, history, and
/// statements that go stale at the next release). A schema id keeps its version after an `@`.
const ROT: [(&str, &str); 7] = [
    (r"\bHR-\d+", "an internal item id"),
    (r"\bdesign\.md \d", "a ruling of the decision log"),
    (r"\bbatch \d+", "a batch"),
    (r"\bthis release\b", "a statement bound to one release"),
    (
        r"\bearlier release\b",
        "a statement bound to a past release",
    ),
    (r"\bnew in\b", "changelog prose"),
    (
        r"(^|[^@\w./-])v?\d+\.\d+\.\d+\b",
        "a release number outside a schema id",
    ),
];

/// The versions of other standards that the README names. Each is no release of houserules.
const STANDARD_VERSIONS: [&str; 1] = ["SARIF 2.1.0"];

/// The text of the README.
fn readme() -> String {
    read("README.md")
}

/// The README and each reference page, as pairs of the path and the text.
fn documents() -> Vec<(&'static str, String)> {
    std::iter::once("README.md")
        .chain(PAGES.iter().map(|page| page.path))
        .map(|path| (path, read(path)))
        .collect()
}

/// The path, relative to the repository root, that the relative link target `file` names in the
/// document at `document`: the folder of the document joined with the target, with each `..`
/// taking one folder off.
fn resolved(document: &str, file: &str) -> String {
    let mut parts: Vec<&str> = document.split('/').collect();
    parts.pop();
    for part in file.split('/') {
        match part {
            ".." => {
                assert!(
                    parts.pop().is_some(),
                    "`{file}` of {document} leaves the repository"
                );
            }
            "." | "" => {}
            name => parts.push(name),
        }
    }
    parts.join("/")
}

/// The GitHub anchor of a heading: lower case, spaces to hyphens, and no other punctuation.
fn slug(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The commands that `houserules <args> --help` lists under `Commands:`, without `help`.
fn commands_of(args: &[&str]) -> BTreeSet<String> {
    let mut all = args.to_vec();
    all.push("--help");
    let output = houserules().args(&all).output().expect("run --help");
    assert!(output.status.success(), "houserules {all:?} fails");
    String::from_utf8(output.stdout)
        .expect("UTF-8 help")
        .lines()
        .skip_while(|line| *line != "Commands:")
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| *name != "help")
        .map(str::to_string)
        .collect()
}

/// The spans of inline code of `text`, in order.
fn code_spans(text: &str) -> Vec<&str> {
    text.split('`').skip(1).step_by(2).collect()
}

/// The `.rs` files that git tracks below `crates/houserules/src`, as paths relative to the
/// repository root (`quality.gates-derive-their-scope`: an untracked file is no part of the kit).
/// A failure of git, and a tree with no such file, fail the test.
fn tracked_rust_files() -> Vec<String> {
    let listing = git(
        &repo_root(),
        &["ls-files", "-z", "--", "crates/houserules/src"],
    );
    let files: Vec<String> = listing
        .split('\0')
        .filter(|path| path.ends_with(".rs"))
        .map(str::to_string)
        .collect();
    assert!(!files.is_empty(), "git tracks no source file");
    files
}

/// The value of the `description` key of the `[package]` table of the crate manifest.
fn crate_description() -> String {
    let manifest = read(CRATE_MANIFEST);
    let package = manifest
        .split("\n[")
        .find(|table| table.starts_with("[package]"))
        .expect("the manifest has a [package] table");
    let line = package
        .lines()
        .find(|line| line.starts_with("description = "))
        .expect("the package has a description");
    line.trim_start_matches("description = ")
        .trim_matches('"')
        .to_string()
}

/// The first screen is the title, the badges, the line of the description, and a few short
/// paragraphs: the problem that houserules solves, what it is made of and needs, the core and its
/// optional layers, and when it fits. It stays short.
#[test]
fn the_first_screen_says_what_houserules_is_and_when_it_fits() {
    let text = readme();
    let screen = text.split("\n## ").next().expect("a first screen");
    assert_eq!(screen.lines().next(), Some("# houserules"));
    let described: Vec<&str> = screen
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty() && !line.starts_with("[!["))
        .collect();
    assert_eq!(described[0], ONE_LINER, "the line under the badges");
    assert!(
        ONE_LINER.chars().count() < 120,
        "the line is under 120 characters"
    );
    assert!(
        screen.lines().count() <= 25,
        "the first screen holds {} lines",
        screen.lines().count()
    );
    states("the first screen", &folded(screen), &FIRST_SCREEN);
}

/// The description of the crate, which a package index shows, is the line under the badges.
#[test]
fn the_crate_description_is_the_line_under_the_badges() {
    assert_eq!(crate_description(), ONE_LINER, "{CRATE_MANIFEST}");
}

/// The funnel is install, usage, the features, the concepts, then the documentation, and the table
/// of contents lists each section, in order, with its anchor.
#[test]
fn the_sections_follow_the_funnel_and_the_table_of_contents_lists_them() {
    let text = readme();
    assert_eq!(headings(&text, 2), FUNNEL);
    let listed: Vec<String> = section(&text, "## Table of contents")
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let expected: Vec<String> = FUNNEL[1..]
        .iter()
        .map(|title| format!("- [{title}](#{})", slug(title)))
        .collect();
    assert_eq!(listed, expected);
}

/// The Concepts section defines each term in one entry, `- **<term>**: <definition>`, in
/// alphabetical order. Each term that first-time readers lacked has an entry, and each entry names
/// a term that the README uses outside the section.
#[test]
fn the_concepts_define_the_terms_that_the_readme_uses() {
    let text = readme();
    let concepts = section(&text, "## Concepts");
    let terms: Vec<&str> = concepts
        .lines()
        .filter_map(|line| line.strip_prefix("- **"))
        .map(|line| line.split("**:").next().expect("a term"))
        .collect();
    assert_eq!(terms, CONCEPTS, "the terms of the Concepts section");
    let mut sorted = terms.clone();
    sorted.sort_unstable();
    assert_eq!(
        terms, sorted,
        "the Concepts section is in alphabetical order"
    );
    for term in UNDEFINED_BEFORE {
        assert!(CONCEPTS.contains(&term), "Concepts lacks `{term}`");
    }
    let outside = folded(&text.replace(&concepts, "")).to_lowercase();
    for term in CONCEPTS {
        for word in term.split(", ") {
            assert!(
                outside.contains(word),
                "the README uses `{word}` nowhere but in Concepts"
            );
        }
    }
}

/// The reference text that left the README lives in pages under `docs/`. Each page starts with its
/// title, its only level-one heading, and holds the level-two sections that it lists. The README
/// holds none of these titles as a heading, but the one stub that keeps its heading, and its
/// Documentation section links each page once, in the order of [`PAGES`].
#[test]
fn the_reference_pages_hold_the_sections_that_left_the_readme() {
    let text = readme();
    let readme_titles: BTreeSet<String> =
        (2..=3).flat_map(|level| headings(&text, level)).collect();
    let documentation = section(&text, "## Documentation");
    let (stub_heading, stub_page) = UPDATING_STUB;
    let stub_title = stub_heading.trim_start_matches('#').trim();
    let paths: Vec<&str> = PAGES.iter().map(|page| page.path).collect();
    assert_eq!(
        paths,
        docs_support::REFERENCE_PAGES,
        "the shared list of the reference pages"
    );
    let mut linked_at = Vec::new();
    for page in &PAGES {
        let body = read(page.path);
        let first = format!("# {}", page.title);
        assert_eq!(body.lines().next(), Some(first.as_str()), "{}", page.path);
        assert_eq!(headings(&body, 1), [page.title], "{}", page.path);
        assert_eq!(headings(&body, 2), page.sections, "{}", page.path);
        for title in std::iter::once(&page.title).chain(page.sections) {
            assert!(
                *title == stub_title || !readme_titles.contains(*title),
                "the README still holds the section `{title}` of {}",
                page.path
            );
        }
        let link = format!("]({})", page.path);
        assert_eq!(
            documentation.matches(&link).count(),
            1,
            "the Documentation section links {} once",
            page.path
        );
        linked_at.push(documentation.find(&link).expect("a link"));
    }
    assert!(
        linked_at.windows(2).all(|pair| pair[0] < pair[1]),
        "the Documentation section links the pages in the order of PAGES"
    );

    let stub = folded(&section(&text, stub_heading));
    assert!(
        stub.contains(&format!("]({stub_page})")),
        "`{stub_heading}` links {stub_page}"
    );
    assert!(
        stub.contains(NO_UPDATE_CHECK),
        "`{stub_heading}` names {NO_UPDATE_CHECK}"
    );
}

/// Each feature section describes its feature briefly and links the page under `docs/` that holds
/// the detail.
#[test]
fn each_feature_section_links_the_page_of_its_detail() {
    let text = readme();
    for (heading, page) in FEATURE_PAGES {
        let feature = section(&text, heading);
        assert!(
            feature.contains(&format!("]({page})")),
            "`{heading}` links {page}"
        );
        let file = page.split('#').next().expect("a path");
        assert!(repo_file(file).is_file(), "{file} is no file");
    }
}

/// An install writes `.claude/rules/` only with the Claude renderer, which `init --target claude`
/// enables, and the section on coding agents says so in the sentence that names the folder.
#[test]
fn the_coding_agents_section_names_the_renderer_that_writes_the_claude_rules() {
    let text = readme();
    states(
        "the coding agents section",
        &folded(&section(&text, "### Coding agents")),
        &[CLAUDE_RULES_SENTENCE],
    );

    let default = scratch_git_repo();
    run_ok(default.path(), &["init"]);
    assert!(
        !default.path().join(".claude").join("rules").exists(),
        "a default install writes no .claude/rules"
    );
    let claude = scratch_git_repo();
    run_ok(claude.path(), &["init", "--target", "claude"]);
    assert!(
        claude.path().join(".claude").join("rules").is_dir(),
        "`init --target claude` writes .claude/rules"
    );
}

/// The values that `houserules init --help` lists under `Possible values:` of `--target`.
fn target_values() -> BTreeSet<String> {
    let output = houserules()
        .args(["init", "--help"])
        .output()
        .expect("run init --help");
    assert!(output.status.success(), "init --help fails");
    String::from_utf8(output.stdout)
        .expect("UTF-8 help")
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("--target"))
        .skip_while(|line| line.trim() != "Possible values:")
        .skip(1)
        .take_while(|line| line.trim_start().starts_with("- "))
        .filter_map(|line| line.trim_start().strip_prefix("- "))
        .filter_map(|value| value.split(':').next())
        .map(str::to_string)
        .collect()
}

/// The coding agents section names the tools that read `AGENTS.md`, the one value of `--target`
/// that `init --help` lists, and the formats of other tools that houserules does not write.
#[test]
fn the_coding_agents_section_names_the_one_target_and_the_tools_that_read_agents_md() {
    let text = readme();
    states(
        "the coding agents section",
        &folded(&section(&text, "### Coding agents")),
        &CODING_AGENTS,
    );
    let expected: BTreeSet<String> = TARGETS.iter().map(|value| value.to_string()).collect();
    assert_eq!(target_values(), expected, "the values of `init --target`");
}

/// The setup section names each file that `init` writes in a repository with a commit: each path of
/// a `wrote` line, and `.houserules.json`, for which `init` prints no `wrote` line. `init` writes no
/// CI file and no git hook, and the refs of the repository stay as they were.
#[test]
fn the_setup_section_names_each_file_that_init_writes() {
    let text = readme();
    let setup = section(&text, "### Setting up your repository");
    states("the setup section", &folded(&setup), &SETUP);
    let named: BTreeSet<&str> = code_spans(&setup).into_iter().collect();

    let repo = scratch_git_repo();
    fs::write(repo.path().join("app.py"), "print('hi')\n").expect("write a file");
    commit_all(repo.path(), "feat: start");
    let refs_before = git(repo.path(), &["for-each-ref", "--format=%(refname)"]);
    let hooks_folder = repo.path().join(".git").join("hooks");
    let hooks = || -> BTreeSet<String> {
        if !hooks_folder.exists() {
            return BTreeSet::new();
        }
        fs::read_dir(&hooks_folder)
            .expect("read .git/hooks")
            .map(|entry| {
                entry
                    .expect("an entry of .git/hooks")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    };
    let hooks_before = hooks();
    let output = run_ok(repo.path(), &["init"]);
    let printed = String::from_utf8(output.stdout).expect("UTF-8 output");
    let mut written: Vec<&str> = printed
        .lines()
        .filter_map(|line| line.strip_prefix("wrote "))
        .collect();
    assert!(
        !written.contains(&".houserules.json"),
        "init prints a `wrote` line for .houserules.json:\n{printed}"
    );
    assert!(
        repo.path().join(".houserules.json").is_file(),
        "init writes no .houserules.json"
    );
    written.push(".houserules.json");
    for path in written {
        assert!(named.contains(path), "the setup section lacks `{path}`");
    }
    for folder in [".github", ".gitlab", ".githooks"] {
        assert!(!repo.path().join(folder).exists(), "init writes {folder}/");
    }
    assert_eq!(
        git(repo.path(), &["for-each-ref", "--format=%(refname)"]),
        refs_before,
        "init changes the refs of the repository"
    );
    assert_eq!(hooks(), hooks_before, "init writes a hook into .git/hooks");
}

/// The table of check types of the rules section: one row for each type of the knowledge schema, in
/// any order, each with an example `check` of its own type.
fn check_examples() -> Vec<(String, Value)> {
    let text = readme();
    let rules = section(&text, "## Rules and checks");
    let rows: Vec<(String, Value)> = rules
        .lines()
        .filter(|line| line.starts_with("| `"))
        .map(|line| {
            let spans = code_spans(line);
            let example: Value =
                serde_json::from_str(spans[spans.len() - 1]).unwrap_or_else(|error| {
                    panic!("the example of `{}` is no JSON: {error}", spans[0])
                });
            (spans[0].to_string(), example)
        })
        .collect();
    let schema: Value =
        serde_json::from_str(&read("template/knowledge/schema.json")).expect("the schema is JSON");
    let types: BTreeSet<String> = schema["$defs"]["check"]["properties"]["type"]["enum"]
        .as_array()
        .expect("the enum of check types")
        .iter()
        .map(|kind| kind.as_str().expect("a type").to_string())
        .collect();
    let listed: BTreeSet<String> = rows.iter().map(|(kind, _)| kind.clone()).collect();
    assert_eq!(listed, types, "the check types of the table");
    assert_eq!(rows.len(), types.len(), "the table lists a type twice");
    for (kind, example) in &rows {
        assert_eq!(
            example["type"].as_str(),
            Some(kind.as_str()),
            "the example of `{kind}`"
        );
    }
    rows
}

/// Each example `check` of the table, as the check of an entry of its own, passes `check-knowledge`
/// in an install whose `runners` key defines the tool that the `runner` example names.
#[test]
fn each_example_check_of_the_table_passes_check_knowledge() {
    let examples = check_examples();
    let repo = scratch_git_repo();
    run_ok(repo.path(), &["init"]);
    let entries: Vec<Value> = examples
        .iter()
        .map(|(kind, check)| {
            serde_json::json!({
                "id": format!("examples.{kind}"),
                "kind": "rule",
                "area": "global",
                "summary": format!("An entry with a {kind} check."),
                "body": ["The README shows the check."],
                "tags": ["example"],
                "source": { "date": "2026-10-26", "by": "docs" },
                "check": check,
            })
        })
        .collect();
    let topic = serde_json::json!({
        "topic": "examples",
        "title": "The examples of the README",
        "entries": entries,
    });
    fs::write(
        repo.path().join("knowledge").join("examples.json"),
        serde_json::to_string_pretty(&topic).expect("serialize the topic"),
    )
    .expect("write the topic file");
    let tools: BTreeSet<&str> = examples
        .iter()
        .filter_map(|(_, check)| check["tool"].as_str())
        .collect();
    let marker_path = repo.path().join(".houserules.json");
    let mut marker: Value =
        serde_json::from_str(&fs::read_to_string(&marker_path).expect("read the marker"))
            .expect("the marker is JSON");
    for tool in tools {
        marker["runners"][tool] = serde_json::json!({
            "command": [tool],
            "parser": "exit-code",
            "install": format!("install {tool}"),
        });
    }
    fs::write(
        &marker_path,
        serde_json::to_string_pretty(&marker).expect("serialize the marker"),
    )
    .expect("write the marker");
    run_ok(repo.path(), &["render"]);
    commit_all(repo.path(), "docs: add the examples");
    let output = run(repo.path(), &["check-knowledge"]);
    assert!(
        output.status.success(),
        "check-knowledge fails:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The intent section states what the trailer does, what `evidence` shows of a plan, and how
/// houserules meets OpenSpec.
#[test]
fn the_intent_section_states_what_the_trailer_and_openspec_do() {
    let text = readme();
    states(
        "the intent section",
        &folded(&section(&text, "## Linking a change to its intent")),
        &INTENT_STATEMENTS,
    );
}

/// The limits section states what houserules does not do or guarantee, in one place.
#[test]
fn the_limits_section_states_what_houserules_does_not_do() {
    let text = readme();
    states(
        "the limits section",
        &folded(&section(&text, "## Limits")),
        &LIMITS,
    );
}

/// The requirements name git 2.41 (the floor of `--attr-source`) and the line that an older git
/// gives, as the source formats it. The commands that need the floor are the commands whose source
/// calls the check of the version, found by reading the source files that git tracks.
#[test]
fn the_requirements_name_the_git_floor_and_the_commands_that_need_it() {
    let text = readme();
    let requirements = folded(&section(&text, "### Requirements"));
    states("the requirements", &requirements, &REQUIREMENTS);

    let floor = read("crates/houserules/src/review/git.rs");
    assert!(
        floor.contains(GIT_FLOOR_FORMAT),
        "review/git.rs formats the line of an older git"
    );
    assert!(
        floor.contains(GIT_FLOOR_CONSTANT),
        "review/git.rs declares the floor 2.41"
    );

    let call = Regex::new(r#"require_attr_source\("([^"]+)"\)"#).expect("the pattern compiles");
    let mut named = BTreeSet::new();
    for file in tracked_rust_files() {
        let source = read(&file);
        for hit in call.find_iter(&source) {
            let range = hit.group(1).expect("the name of the command");
            named.insert(source[range].to_string());
        }
    }
    let expected: BTreeSet<String> = GIT_FLOOR_COMMANDS
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert_eq!(
        named, expected,
        "the commands whose source checks the git version"
    );
    for command in GIT_FLOOR_COMMANDS {
        assert!(
            requirements.contains(&format!("`{command}`")),
            "the requirements name `{command}`"
        );
    }
    for args in [
        &["route"][..],
        &["evidence"],
        &["bundle", "check"],
        &["record", "write"],
    ] {
        let mut all = args.to_vec();
        all.push("--help");
        assert!(
            help_of_args(&all).contains("older than 2.41"),
            "the help of {args:?} names the floor"
        );
    }
}

/// The integration section names its three integrations in order and states what it says of the CI
/// units. The jobs of the GitHub unit that hold a write scope are the two that the section names;
/// its `gates` job checks out the head of the request; its `report` job runs only for a request
/// from the repository itself, and the `report` job of the GitLab unit posts nothing for a request
/// from a fork.
#[test]
fn the_integration_section_states_the_ci_units() {
    let text = readme();
    let integration = section(&text, "## Integration");
    assert_eq!(headings(&integration, 3), INTEGRATION);
    states(
        "the integration section",
        &folded(&integration),
        &INTEGRATION_STATEMENTS,
    );

    let github = parse_unit(
        &read(".github/workflows/houserules-review.yml"),
        Host::GitHub,
    );
    let report = github.job("report");
    assert!(
        report["if"]
            .as_str()
            .is_some_and(|condition| condition.contains(SAME_REPOSITORY)),
        "the report job of the GitHub unit runs for a fork: {}",
        report["if"]
    );
    let gates = github.job("gates");
    assert_eq!(
        gates["steps"][0]["with"]["ref"].as_str(),
        Some(REQUEST_HEAD),
        "the gates job of the GitHub unit checks out another commit"
    );
    assert!(
        read("ci/gitlab-review.yml").contains(FORK_NOTICE),
        "the GitLab unit lacks the notice for a request from a fork"
    );
    let writing: BTreeSet<String> = github
        .roles()
        .into_iter()
        .filter(|role| {
            github
                .permissions(role)
                .values()
                .any(|access| access == "write")
        })
        .collect();
    let expected: BTreeSet<String> = WRITING_JOBS.iter().map(|job| job.to_string()).collect();
    assert_eq!(writing, expected, "the jobs that hold a write scope");
}

/// The inputs that the CI section names are the inputs that both CI units take, and the
/// `validation` input of each is empty by default and names the capture of `openspec validate
/// --json`.
#[test]
fn the_inputs_of_the_ci_units_are_the_inputs_that_the_readme_names() {
    let github = parse_unit(
        &read(".github/workflows/houserules-review.yml"),
        Host::GitHub,
    );
    let gitlab = parse_unit(&read("ci/gitlab-review.yml"), Host::gitlab("root"));
    let common: BTreeSet<String> = github
        .input_names()
        .intersection(&gitlab.input_names())
        .cloned()
        .collect();

    let text = folded(&section(&readme(), "### CI"));
    let sentence = text
        .split("Both units take the inputs ")
        .nth(1)
        .and_then(|rest| rest.split(". ").next())
        .expect("the README names the inputs of both units");
    let named: BTreeSet<String> = code_spans(sentence)
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(named, common, "the inputs of both units");

    for unit in [&github, &gitlab] {
        let validation = unit.input("validation");
        assert_eq!(validation["default"].as_str(), Some(""), "{validation}");
        assert!(
            validation["description"]
                .as_str()
                .is_some_and(|text| text.contains("openspec validate --json")),
            "{validation}"
        );
    }
}

/// The CLI reference page lists the commands as `--help` prints them. Each command of the help has
/// a row, no row names a command that the help lacks, and a row that names the subcommands of a
/// command names the subcommands of its help. The test compares sets of names: the page keeps its
/// own grouping order and its own descriptions.
#[test]
fn the_cli_reference_lists_the_commands_of_the_help() {
    let text = read("docs/cli.md");
    let reference = section(&text, "# CLI reference");
    let mut named = BTreeSet::new();
    for line in reference.lines().filter(|line| line.starts_with("| `")) {
        let span = code_spans(line)[0];
        let name = span.split_whitespace().next().expect("a command name");
        named.insert(name.to_string());
        if let (Some(open), Some(close)) = (span.find('<'), span.find('>'))
            && span[open..close].contains("\\|")
        {
            let alternatives: BTreeSet<String> = span[open + 1..close]
                .split("\\|")
                .map(str::to_string)
                .collect();
            assert_eq!(
                alternatives,
                commands_of(&[name]),
                "the subcommands of `{name}` in the row"
            );
        }
    }
    assert_eq!(named, commands_of(&[]), "the commands of the CLI reference");
}

/// `houserules.readme-mirrors-kit-owned`: the ownership table of its page lists, for each class of
/// `houserules files`, the files that the command prints.
#[test]
fn the_ownership_table_lists_the_files_that_the_files_command_prints() {
    let output = houserules().arg("files").output().expect("run files");
    assert!(output.status.success());
    let classes: Value = serde_json::from_slice(&output.stdout).expect("files prints JSON");
    let classes = classes.as_object().expect("an object of classes");
    let text = read("docs/ownership.md");
    let table = section(&text, "# Ownership model");
    for (class, files) in classes {
        let marker = format!("(`{class}`) |");
        let row = table
            .lines()
            .find(|line| line.starts_with("| ") && line.contains(&marker))
            .unwrap_or_else(|| panic!("the table has no row for {class}"));
        let cell = row
            .trim_end_matches('|')
            .rsplit(" | ")
            .next()
            .expect("a files cell");
        let mut listed: Vec<&str> = code_spans(cell)
            .into_iter()
            .filter(|span| !span.starts_with("--"))
            .collect();
        let mut printed: Vec<&str> = files
            .as_array()
            .expect("a list of files")
            .iter()
            .map(|file| file.as_str().expect("a path"))
            .collect();
        listed.sort_unstable();
        printed.sort_unstable();
        assert_eq!(listed, printed, "the files of the class {class}");
    }
    assert_eq!(
        table
            .lines()
            .filter(|line| line.contains("(`") && line.starts_with("| "))
            .count(),
        classes.len(),
        "the table holds one row for each class"
    );
}

/// The number of `sh` blocks of `text`.
fn sh_blocks(text: &str) -> usize {
    blocks(text)
        .iter()
        .filter(|block| block.info == "sh")
        .count()
}

/// Every level-two section of the README that holds an `sh` block is run in the session or
/// declared a form with its reason, and each declaration names a section that holds one.
#[test]
fn every_section_with_a_sh_block_is_run_or_named_as_a_form() {
    let text = readme();
    let with_blocks: BTreeSet<String> = headings(&text, 2)
        .into_iter()
        .filter(|title| sh_blocks(&section(&text, &format!("## {title}"))) > 0)
        .collect();
    let declared: BTreeSet<String> = SESSION
        .iter()
        .map(|name| name.to_string())
        .chain(FORMS.iter().map(|(name, _)| name.to_string()))
        .collect();
    assert_eq!(with_blocks, declared);
    for (name, reason) in FORMS {
        assert!(!reason.is_empty(), "`{name}` is a form with no reason");
    }
}

/// Every section of a reference page that holds an `sh` block is run by a test of its own or
/// declared a form with its reason, and each declaration names a section that holds one. A section
/// is the text under a level-two heading, or the text of the page above its first level-two
/// heading, named by the title of the page. A test file that runs the blocks names the page and the
/// section.
#[test]
fn every_section_of_a_reference_page_with_a_sh_block_is_run_or_named_as_a_form() {
    let mut with_blocks = BTreeSet::new();
    for page in &PAGES {
        let body = read(page.path);
        let mut in_sections = 0;
        for title in page.sections {
            let count = sh_blocks(&section(&body, &format!("## {title}")));
            in_sections += count;
            if count > 0 {
                with_blocks.insert((page.path.to_string(), title.to_string()));
            }
        }
        if sh_blocks(&body) > in_sections {
            with_blocks.insert((page.path.to_string(), page.title.to_string()));
        }
    }
    let declared: BTreeSet<(String, String)> = PAGE_BLOCKS
        .iter()
        .map(|(path, title, _)| (path.to_string(), title.to_string()))
        .collect();
    assert_eq!(with_blocks, declared);
    for (path, title, shown) in &PAGE_BLOCKS {
        match shown {
            Shown::Run(file) => {
                let source = read(&format!("crates/houserules/tests/{file}"));
                assert!(
                    source.contains(&format!("\"{path}\"")) && source.contains(title),
                    "{file} does not run the blocks of `{title}` of {path}"
                );
            }
            Shown::Form(reason) => {
                assert!(
                    !reason.is_empty(),
                    "`{title}` of {path} is a form with no reason"
                );
            }
        }
    }
}

/// The `sh` blocks of the quick start, of the intent section, and of the review section run one
/// after the other, as printed, in one shell that starts in an empty folder. The first block makes
/// the scratch repository [`DEMO`], with one commit, and enters it. Each block exits 0, and prints
/// the `text` or `json` block that follows it, or nothing when none follows. The README shows each
/// value of the run by the word of [`named_values`], and each word of [`RUN_VALUES`] occurs in a
/// block that the session prints. After the session, the folder holds the scratch repository and
/// nothing else: the quick start changes no file outside it.
#[cfg(unix)]
#[test]
fn the_session_blocks_run_as_printed_in_a_repository_with_commits() {
    let text = readme();
    states(
        "the quick start",
        &folded(&section(&text, "## Quick start")),
        &[SCRATCH_SENTENCE],
    );
    let mut runs: Vec<(String, String)> = Vec::new();
    for name in SESSION {
        let all = blocks(&section(&text, &format!("## {name}")));
        for (at, block) in all.iter().enumerate() {
            if block.info != "sh" {
                continue;
            }
            let expected = match all.get(at + 1) {
                Some(next) if next.info == "text" || next.info == "json" => next.body.clone(),
                _ => String::new(),
            };
            runs.push((block.body.clone(), expected));
        }
    }
    assert!(runs.len() >= 10, "the session holds {} blocks", runs.len());
    for (_, word) in RUN_VALUES {
        assert!(
            runs.iter().any(|(_, expected)| expected.contains(word)),
            "no block of the session prints `{word}`"
        );
    }

    let mut script = String::new();
    for (at, (body, _)) in runs.iter().enumerate() {
        script.push_str(&format!(
            "printf '@@{at}@@\\n'\n{body}\nprintf '@@exit %d@@\\n' \"$?\"\n"
        ));
    }
    let dir = tempfile::tempdir().expect("a scratch directory");
    let (_, printed) = help_support::run_block(dir.path(), &script);
    let canonical = dir.path().canonicalize().expect("the session folder");
    let printed = named_values(
        &printed,
        &dir.path().join(DEMO).display().to_string(),
        &canonical.join(DEMO).display().to_string(),
    );
    let left: Vec<String> = fs::read_dir(dir.path())
        .expect("read the session folder")
        .map(|entry| {
            entry
                .expect("an entry of the session folder")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        left,
        [DEMO],
        "the session changes files outside the scratch repository"
    );

    let mut results: Vec<(String, i32)> = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in printed.lines() {
        if let Some(code) = line
            .strip_prefix("@@exit ")
            .and_then(|rest| rest.strip_suffix("@@"))
        {
            let body = current.take().expect("an exit marker closes a block");
            results.push((body.join("\n"), code.parse().expect("an exit code")));
        } else if line.starts_with("@@") && line.ends_with("@@") && !line.contains(' ') {
            current = Some(Vec::new());
        } else if let Some(body) = current.as_mut() {
            body.push(line);
        }
    }
    assert_eq!(results.len(), runs.len(), "the session printed:\n{printed}");
    for ((script, expected), (output, code)) in runs.iter().zip(&results) {
        assert_eq!(*code, 0, "the block exits 0:\n{script}\n{output}");
        assert_eq!(
            output.trim_end(),
            expected,
            "the block does not print the text that follows it:\n{script}"
        );
    }
}

/// Spec 19: the reference workflow page states the rules of the faster task loop, and each rule
/// shares a phrase with the kit file that carries it.
#[test]
fn the_reference_workflow_states_the_task_loop_in_the_words_of_the_kit() {
    let text = read("docs/reference-workflow.md");
    let workflow = section(&text, "# The reference workflow");
    assert_eq!(headings(&workflow, 2), ["The task loop"]);
    let section_text = folded(&section(&workflow, "## The task loop"));
    states("the task loop", &section_text, &TASK_LOOP);

    for (phrase, file) in TASK_LOOP_PHRASES {
        let kit = folded(&read(&format!("template/workflows/reference/{file}")));
        assert!(kit.contains(phrase), "{file} lacks: {phrase}");
        assert!(
            section_text.contains(phrase),
            "the task loop lacks: {phrase}"
        );
    }
}

/// The README and the reference pages name no internal item, ruling, batch, past release, or release
/// number, and hold no changelog prose (`writing-style.readme-is-an-adopter-funnel`).
#[test]
fn the_readme_holds_no_history_and_no_statement_that_goes_stale() {
    let mut found = Vec::new();
    for (path, text) in documents() {
        let allowed: Vec<_> = STANDARD_VERSIONS
            .iter()
            .flat_map(|phrase| {
                text.match_indices(phrase)
                    .map(|(at, _)| at..at + phrase.len())
            })
            .collect();
        for (pattern, what) in ROT {
            let rot = Regex::new(pattern).expect("the pattern compiles");
            for hit in rot.find_iter(&text) {
                let range = hit.range();
                if allowed
                    .iter()
                    .any(|phrase| phrase.start <= range.start && range.end <= phrase.end)
                {
                    continue;
                }
                let line = text[..range.start].matches('\n').count() + 1;
                found.push(format!("{path}:{line}: {what}: {}", &text[range]));
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

/// Every link of the README and of the reference pages that is no web link resolves, against the
/// folder of the file that holds it: a file of the tree, and an anchor in the same file or in the
/// Markdown file that the link names.
#[test]
fn every_relative_link_of_the_readme_resolves() {
    let link = Regex::new(r"\]\(([^)\s]+)\)").expect("the pattern compiles");
    let mut checked_in_readme = 0;
    for (path, text) in documents() {
        for hit in link.find_iter(&text) {
            let target = &text[hit.group(1).expect("a target")];
            if target.starts_with("http://") || target.starts_with("https://") {
                continue;
            }
            let (file, anchor) = match target.split_once('#') {
                Some((file, anchor)) => (file, Some(anchor)),
                None => (target, None),
            };
            let page = if file.is_empty() {
                text.clone()
            } else {
                let file = resolved(path, file);
                assert!(
                    repo_file(&file).exists(),
                    "the link `{target}` of {path} names no file"
                );
                if anchor.is_some() {
                    read(&file)
                } else {
                    String::new()
                }
            };
            if let Some(anchor) = anchor {
                let slugs: BTreeSet<String> = (1..=6)
                    .flat_map(|level| headings(&page, level))
                    .map(|title| slug(&title))
                    .collect();
                assert!(
                    slugs.contains(anchor),
                    "the link `{target}` of {path} names no heading"
                );
            }
            if path == "README.md" {
                checked_in_readme += 1;
            }
        }
    }
    assert!(
        checked_in_readme > 20,
        "the README holds {checked_in_readme} relative links"
    );
}
