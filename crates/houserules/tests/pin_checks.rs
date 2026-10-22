//! The pin checks and the durable-sha scope (spec 8.1 and 8.3, acceptance
//! cases 8, 9 (HR-218), and 10).
//!
//! Every case runs the compiled binary against a scratch git repository that
//! `init --tracker json` seeded, with the `security-hygiene` pack added, so the
//! entries under test are the kit's own. A case file of
//! `tests/fixtures/pin-cases/` is committed under the name that the entry's
//! `files` globs select, and `audit --base <the install commit>` judges it. The
//! scratch repository commits `.gitattributes` with `* -text` first, so that
//! git keeps the CRLF bytes of a case file under any `core.autocrlf`.
//!
//! A test whose doc comment calls it a pin asserts a result that the
//! package.json alternative alone gives as well.

mod tracker_support;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use tempfile::TempDir;
use tracker_support::{commit_all, git, repo_root, run, run_ok, stderr, stdout};

/// The entry that flags a non-exact dependency pin of a `package.json` or a
/// `Cargo.toml`.
const EXACT_PINS: &str = "security-hygiene.exact-pins";

/// The entry that flags a tool pin of a mise config file that is not a full
/// version.
const TOOL_PINS: &str = "security-hygiene.tool-pins-are-exact";

/// The entry that flags a `.tool-versions` pin that is not a full version.
const TOOL_VERSIONS: &str = "security-hygiene.tool-versions-pins-are-exact";

/// The entry that flags a citation of a commit that the default branch does
/// not reach.
const DURABLE: &str = "knowledge-base.cite-durable-refs";

/// What an entry must give for one case file.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Expect {
    /// The row fails and names the case file at this line.
    Flagged(usize),
    /// The row passes after it read the one changed file.
    Clean,
    /// The row passes and read no file: the globs of the entry leave the
    /// changed file out.
    NotRead,
}

/// One case file, the path that it takes in the scratch repository, and what
/// an entry must give for it.
struct Case {
    /// The file below `tests/fixtures/pin-cases/`.
    file: &'static str,
    /// The path in the scratch repository. The `files` globs of the entry
    /// select it, or leave it out for `Expect::NotRead`.
    installed_as: &'static str,
    expect: Expect,
}

/// A `Case` for the case file `file`, installed as `installed_as`.
const fn pin_case(file: &'static str, installed_as: &'static str, expect: Expect) -> Case {
    Case {
        file,
        installed_as,
        expect,
    }
}

/// The Cargo and `package.json` cases of `security-hygiene.exact-pins`. The
/// flagged lines are the lines of the regress run over the same files.
const EXACT_PINS_CASES: [Case; 20] = [
    pin_case("must-fail-bare.toml", "Cargo.toml", Expect::Flagged(9)),
    pin_case("must-fail-crlf-body.toml", "Cargo.toml", Expect::Flagged(3)),
    pin_case("must-fail-crlf.toml", "Cargo.toml", Expect::Flagged(2)),
    pin_case("must-fail-inline.toml", "Cargo.toml", Expect::Flagged(4)),
    pin_case("must-fail-line.toml", "Cargo.toml", Expect::Flagged(5)),
    pin_case(
        "must-fail-multiline-array.toml",
        "Cargo.toml",
        Expect::Flagged(5),
    ),
    pin_case(
        "must-fail-quote-comment.toml",
        "Cargo.toml",
        Expect::Flagged(2),
    ),
    pin_case(
        "must-fail-quoted-key.toml",
        "Cargo.toml",
        Expect::Flagged(6),
    ),
    pin_case("must-fail-table.toml", "Cargo.toml", Expect::Flagged(4)),
    pin_case(
        "must-fail-target-cfg.toml",
        "Cargo.toml",
        Expect::Flagged(2),
    ),
    pin_case("must-fail-workspace.toml", "Cargo.toml", Expect::Flagged(4)),
    pin_case("must-pass-indented.toml", "Cargo.toml", Expect::Clean),
    pin_case(
        "must-pass-package-after-deps.toml",
        "Cargo.toml",
        Expect::Clean,
    ),
    pin_case("must-pass-table.toml", "Cargo.toml", Expect::Clean),
    pin_case("must-pass.toml", "Cargo.toml", Expect::Clean),
    pin_case(
        "pkg/must-fail-after-engines.json",
        "package.json",
        Expect::Flagged(3),
    ),
    pin_case(
        "pkg/must-fail-caret.json",
        "package.json",
        Expect::Flagged(3),
    ),
    pin_case(
        "pkg/must-fail-star.json",
        "package.json",
        Expect::Flagged(3),
    ),
    pin_case("pkg/must-fail-x.json", "package.json", Expect::Flagged(3)),
    pin_case("pkg/must-pass-engines.json", "package.json", Expect::Clean),
];

/// The mise cases of `security-hygiene.tool-pins-are-exact`.
const TOOL_PINS_CASES: [Case; 9] = [
    pin_case("mise-fail-inline.toml", "mise.toml", Expect::Flagged(2)),
    pin_case("mise-fail-latest.toml", "mise.toml", Expect::Flagged(2)),
    pin_case("mise-fail-lts.toml", "mise.toml", Expect::Flagged(2)),
    pin_case("mise-fail-minor.toml", "mise.toml", Expect::Flagged(2)),
    pin_case("mise-fail-prefix.toml", "mise.toml", Expect::Flagged(2)),
    pin_case("mise-fail-stable.toml", "mise.toml", Expect::Flagged(2)),
    pin_case("mise-pass-ref.toml", "mise.toml", Expect::Clean),
    pin_case("mise-pass-task-multiline.toml", "mise.toml", Expect::Clean),
    pin_case("mise-pass.toml", "mise.toml", Expect::Clean),
];

/// The `.tool-versions` cases of
/// `security-hygiene.tool-versions-pins-are-exact`.
const TOOL_VERSIONS_CASES: [Case; 7] = [
    pin_case(
        "tool-versions-fail-caret.txt",
        ".tool-versions",
        Expect::Flagged(1),
    ),
    pin_case(
        "tool-versions-fail-comment.txt",
        ".tool-versions",
        Expect::Flagged(1),
    ),
    pin_case(
        "tool-versions-fail-ge.txt",
        ".tool-versions",
        Expect::Flagged(1),
    ),
    pin_case(
        "tool-versions-fail-stable.txt",
        ".tool-versions",
        Expect::Flagged(1),
    ),
    pin_case(
        "tool-versions-fail-tilde.txt",
        ".tool-versions",
        Expect::Flagged(1),
    ),
    pin_case(
        "tool-versions-fail.txt",
        ".tool-versions",
        Expect::Flagged(1),
    ),
    pin_case("tool-versions-pass.txt", ".tool-versions", Expect::Clean),
];

/// One mise config path for each glob of `security-hygiene.tool-pins-are-exact`,
/// in the order of the globs.
const MISE_PATHS: [&str; 13] = [
    "mise.toml",
    ".mise.toml",
    ".mise.local.toml",
    "mise.local.toml",
    "mise/config.toml",
    "mise/config.local.toml",
    "mise/conf.d/extra/tools.toml",
    ".mise/config.toml",
    ".mise/config.local.toml",
    ".mise/conf.d/extra/tools.toml",
    ".config/mise.toml",
    ".config/mise/tools.toml",
    ".config/mise/conf.d/extra/tools.toml",
];

/// The folder of the committed case files.
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/pin-cases")
}

/// The bytes of the case file `name`.
fn case_bytes(name: &str) -> Vec<u8> {
    let path = fixture_dir().join(name);
    fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The shipped kit pack `name`, read from `template/packs/`.
fn kit_pack(name: &str) -> Value {
    let path = repo_root().join(format!("template/packs/{name}.json"));
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

/// The entry `id` of the shipped kit pack `pack`.
fn kit_entry(pack: &str, id: &str) -> Value {
    let pack = kit_pack(pack);
    pack["entries"]
        .as_array()
        .expect("a pack has an entries array")
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("the pack ships no entry {id}"))
        .clone()
}

/// A scratch repository on `main`: `init --tracker json`, the
/// `security-hygiene` pack, and `.gitattributes` in one base commit.
struct Scratch {
    repo: TempDir,
}

impl Scratch {
    /// A scratch install with the `security-hygiene` pack, committed once.
    fn new() -> Self {
        let repo = tempfile::tempdir().expect("tempdir");
        let root = repo.path();
        git(root, &["init", "-q", "-b", "main"]);
        run_ok(root, &["init", "--tracker", "json"]);
        run_ok(root, &["pack", "add", "security-hygiene"]);
        fs::write(root.join(".gitattributes"), "* -text\n").expect("write .gitattributes");
        commit_all(root, "chore: install the kit");
        Scratch { repo }
    }

    /// The root of the scratch install.
    fn root(&self) -> &Path {
        self.repo.path()
    }

    /// The full sha of `HEAD`.
    fn head(&self) -> String {
        git(self.root(), &["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Writes `content` to `relative` and commits it. Returns the new sha.
    fn commit_file(&self, relative: &str, content: &[u8]) -> String {
        let path = self.root().join(relative);
        fs::create_dir_all(path.parent().expect("a parent folder")).expect("create folders");
        fs::write(path, content).expect("write the file");
        commit_all(self.root(), "feat: add the file")
    }

    /// The JSON result of `audit --base <base>`. The exit code carries no
    /// evidence here: a failing row exits 1.
    fn audit_from(&self, base: &str) -> Value {
        let output = run(self.root(), &["audit", "--base", base]);
        serde_json::from_str(&stdout(&output)).unwrap_or_else(|error| {
            panic!(
                "audit prints JSON ({error}): stdout {}, stderr {}",
                stdout(&output),
                stderr(&output)
            )
        })
    }

    /// Commits `content` as `relative` and returns the audit of that commit
    /// alone.
    fn audit_file(&self, relative: &str, content: &[u8]) -> Value {
        let base = self.head();
        self.commit_file(relative, content);
        self.audit_from(&base)
    }
}

/// The row of `result` for the entry `id`, or `None` when the audit has none.
fn row<'a>(result: &'a Value, id: &str) -> Option<&'a Value> {
    result["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .find(|row| row["id"] == id)
}

/// What is wrong with `row` for `expect`, or `None` when it fits. A flagged
/// row names `path` at the line; the pattern follows it in the evidence.
fn deviation(row: Option<&Value>, path: &str, expect: Expect) -> Option<String> {
    let Some(row) = row else {
        return Some("the audit has no row for the entry".to_string());
    };
    let result = row["result"].as_str().unwrap_or_default();
    let evidence = row["evidence"].as_str().unwrap_or_default();
    let fits = match expect {
        Expect::Flagged(line) => {
            result == "fail" && evidence.starts_with(&format!("{path}:{line} matches "))
        }
        Expect::Clean => result == "pass" && evidence == "1 files checked",
        Expect::NotRead => result == "pass" && evidence == "0 files checked",
    };
    if fits {
        None
    } else {
        let shown: String = evidence.chars().take(60).collect();
        Some(format!("{result} {shown:?}, expected {expect:?}"))
    }
}

/// The lines that name each of `cases` on which `rule` gave something else
/// than the case expects. An empty list means every case fits.
fn deviations<'a>(
    scratch: &Scratch,
    rule: &str,
    cases: impl IntoIterator<Item = &'a Case>,
) -> Vec<String> {
    let mut found = Vec::new();
    for case in cases {
        let result = scratch.audit_file(case.installed_as, &case_bytes(case.file));
        if let Some(line) = deviation(row(&result, rule), case.installed_as, case.expect) {
            found.push(format!("{} as {}: {line}", case.file, case.installed_as));
        }
    }
    found
}

/// The cases of `cases` that `keep` selects.
fn select(cases: &[Case], keep: impl Fn(&Case) -> bool) -> Vec<&Case> {
    cases.iter().filter(|case| keep(case)).collect()
}

/// Asserts that `found` is empty, naming every deviation.
fn assert_no_deviations(rule: &str, found: &[String]) {
    assert!(
        found.is_empty(),
        "{rule} deviates on {} case(s):\n{}",
        found.len(),
        found.join("\n")
    );
}

// ---- case 8: every case file through audit ----

/// Case 8: `security-hygiene.exact-pins` flags each Cargo must-fail case at its
/// own line: a dependency of a dependencies table whose version is not exact.
#[test]
fn exact_pins_flags_every_cargo_must_fail_case_at_its_own_line() {
    let scratch = Scratch::new();
    let cases = select(&EXACT_PINS_CASES, |case| {
        case.installed_as == "Cargo.toml" && matches!(case.expect, Expect::Flagged(_))
    });
    let found = deviations(&scratch, EXACT_PINS, cases);
    assert_no_deviations(EXACT_PINS, &found);
}

/// Case 8: `security-hygiene.exact-pins` passes each Cargo must-pass case:
/// exact pins, a `[package]` version, and a digit-led value of a table that is
/// no dependencies table. A pin: the package.json alternative passes them too.
#[test]
fn exact_pins_passes_every_cargo_must_pass_case() {
    let scratch = Scratch::new();
    let cases = select(&EXACT_PINS_CASES, |case| {
        case.installed_as == "Cargo.toml" && case.expect == Expect::Clean
    });
    let found = deviations(&scratch, EXACT_PINS, cases);
    assert_no_deviations(EXACT_PINS, &found);
}

/// Case 8: `security-hygiene.exact-pins` keeps its result for each
/// `package.json` case. A pin: the shape-first package.json alternative gives
/// the same matches as the lookbehind-first one it replaced.
#[test]
fn exact_pins_keeps_its_result_for_every_package_json_case() {
    let scratch = Scratch::new();
    let cases = select(&EXACT_PINS_CASES, |case| {
        case.installed_as == "package.json"
    });
    let found = deviations(&scratch, EXACT_PINS, cases);
    assert_no_deviations(EXACT_PINS, &found);
}

/// Case 8: `security-hygiene.tool-pins-are-exact` flags each mise must-fail
/// case at its own line and passes each must-pass case, among them a task
/// string that holds the line `sleep 1`. No Cargo case is a mise file, but a
/// Cargo manifest saved under a mise name passes too: only a `[tools]` table
/// is read.
#[test]
fn tool_pins_give_the_result_of_every_case_file() {
    let scratch = Scratch::new();
    let mut found = deviations(&scratch, TOOL_PINS, &TOOL_PINS_CASES);
    let cargo_cases: Vec<Case> = EXACT_PINS_CASES
        .iter()
        .filter(|case| case.installed_as == "Cargo.toml")
        .map(|case| pin_case(case.file, "mise.toml", Expect::Clean))
        .collect();
    found.extend(deviations(&scratch, TOOL_PINS, &cargo_cases));
    assert_no_deviations(TOOL_PINS, &found);
}

/// Case 8: `security-hygiene.tool-versions-pins-are-exact` flags each
/// `.tool-versions` must-fail case at its own line and passes the must-pass
/// case. A mise file never reaches it: the line `sleep 1` of a task string
/// would match its pattern.
#[test]
fn tool_versions_pins_give_the_result_of_every_case_file() {
    let scratch = Scratch::new();
    let mut found = deviations(&scratch, TOOL_VERSIONS, &TOOL_VERSIONS_CASES);
    let mise_cases: Vec<Case> = TOOL_PINS_CASES
        .iter()
        .map(|case| pin_case(case.file, "mise.toml", Expect::NotRead))
        .collect();
    found.extend(deviations(&scratch, TOOL_VERSIONS, &mise_cases));
    assert_no_deviations(TOOL_VERSIONS, &found);
}

/// Case 8: `security-hygiene.tool-pins-are-exact` reads each of the 13 mise
/// config paths, one for each glob, and one nested path. A `.tool-versions`
/// file with a flagged pin is left to its own entry.
#[test]
fn tool_pins_read_every_mise_config_path_and_no_tool_versions_file() {
    let scratch = Scratch::new();
    let latest = b"[tools]\nbd = \"latest\"\n";
    let mut found = Vec::new();
    for path in MISE_PATHS.iter().copied().chain(["tools/mise.toml"]) {
        let result = scratch.audit_file(path, latest);
        if let Some(line) = deviation(row(&result, TOOL_PINS), path, Expect::Flagged(2)) {
            found.push(format!("{path}: {line}"));
        }
    }
    let result = scratch.audit_file(".tool-versions", b"bd latest\n");
    if let Some(line) = deviation(row(&result, TOOL_PINS), ".tool-versions", Expect::NotRead) {
        found.push(format!(".tool-versions: {line}"));
    }
    assert_no_deviations(TOOL_PINS, &found);
}

/// C11: the pin checks accept these misses, and each one passes. A TOML
/// parser would read them; the reversal condition is a miss in a real
/// manifest.
#[test]
fn the_accepted_misses_of_c11_pass() {
    let scratch = Scratch::new();
    let cases: [(&str, &str, &[u8]); 8] = [
        (
            EXACT_PINS,
            "Cargo.toml",
            b"[ dependencies ]\nregex = \"1.11\"\n",
        ),
        (
            EXACT_PINS,
            "Cargo.toml",
            b"[dependencies]\nserde.version = \"1\"\n",
        ),
        (
            TOOL_PINS,
            "mise.toml",
            b"[tools.node]\nversion = \"latest\"\n",
        ),
        (
            TOOL_PINS,
            "mise.toml",
            b"[tools]\nnode = [\"22\", \"20\"]\n",
        ),
        (
            TOOL_PINS,
            "mise.toml",
            b"[tools]\nnode = \"path:/opt/node\"\n",
        ),
        (TOOL_PINS, "mise.toml", b"[tools]\ngo = \"sub-1:1.22\"\n"),
        (TOOL_PINS, "mise.toml", b"[tools]\nzig = \"ref:master\"\n"),
        (TOOL_VERSIONS, ".tool-versions", b"node latest 22.1.0\n"),
    ];
    let mut found = Vec::new();
    for (rule, path, content) in cases {
        let result = scratch.audit_file(path, content);
        if let Some(line) = deviation(row(&result, rule), path, Expect::Clean) {
            let shown = String::from_utf8_lossy(content);
            found.push(format!("{rule} on {path} {shown:?}: {line}"));
        }
    }
    assert_no_deviations("the accepted misses", &found);
}

// ---- the shape of the entries (spec 8.1, 8.3, case 14 for the kit) ----

/// The check fields that the three pin entries share, and the `files` lists
/// that tell them apart. `tool-pins-are-exact` reads the mise globs of the
/// `if` list of `security-hygiene.dependency-vetting`, in that order, and
/// `.tool-versions` goes to its own entry.
#[test]
fn the_pin_entries_have_the_shape_of_spec_8_1() {
    let vetting = kit_entry("security-hygiene", "security-hygiene.dependency-vetting");
    let vetting_globs: Vec<&str> = vetting["check"]["if"]
        .as_array()
        .expect("the vetting check has an if list")
        .iter()
        .map(|glob| glob.as_str().expect("a glob"))
        .collect();
    let mise_globs: Vec<&str> = vetting_globs
        .iter()
        .copied()
        .filter(|glob| *glob != "**/.tool-versions" && glob.contains("mise"))
        .collect();
    assert_eq!(mise_globs.len(), 13, "{mise_globs:?}");
    for (glob, path) in mise_globs.iter().zip(MISE_PATHS) {
        let matcher = globset::GlobBuilder::new(glob)
            .literal_separator(true)
            .build()
            .unwrap_or_else(|error| panic!("glob {glob}: {error}"))
            .compile_matcher();
        assert!(matcher.is_match(path), "{path} is no path of {glob}");
    }
    assert!(vetting_globs.contains(&"**/.tool-versions"));

    let expected: [(&str, bool, Vec<&str>); 3] = [
        (EXACT_PINS, true, vec!["**/package.json", "**/Cargo.toml"]),
        (TOOL_PINS, false, mise_globs),
        (TOOL_VERSIONS, false, vec!["**/.tool-versions"]),
    ];
    for (id, standing, files) in expected {
        let entry = kit_entry("security-hygiene", id);
        let check = &entry["check"];
        assert_eq!(entry["kind"], "rule", "{id}");
        assert_eq!(entry["area"], "global", "{id}");
        assert_eq!(
            entry["standing"].as_bool().unwrap_or(false),
            standing,
            "{id}"
        );
        assert_eq!(check["type"], "grep-absent", "{id}");
        assert_eq!(check["level"], "fail", "{id}");
        assert_eq!(check["flags"], "m", "{id}");
        assert_eq!(check["scope"], "changed", "{id}");
        assert_eq!(check["files"], serde_json::json!(files), "{id}");
        let mut keys: Vec<&str> = check
            .as_object()
            .expect("a check object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["files", "flags", "level", "pattern", "scope", "type"],
            "{id}"
        );
    }
}

/// Spec 8.1: the body of `security-hygiene.exact-pins` loses the sentence
/// that calls a bare Cargo range invisible to the pattern, and keeps its
/// other two sentences.
#[test]
fn the_exact_pins_body_drops_the_sentence_on_bare_cargo_ranges() {
    let entry = kit_entry("security-hygiene", EXACT_PINS);
    let body: Vec<&str> = entry["body"]
        .as_array()
        .expect("a body")
        .iter()
        .map(|paragraph| paragraph.as_str().expect("a paragraph"))
        .collect();
    assert_eq!(body.len(), 2, "{body:?}");
    assert!(
        body.iter()
            .all(|paragraph| !paragraph.contains("bare Cargo range")
                && !paragraph.contains("invisible")),
        "{body:?}"
    );
}

/// Spec 8.3: the check of `knowledge-base.cite-durable-refs` is the durable-sha
/// check at level `fail` over these six paths: the four it read before, then
/// the plans and the eval record. The test compares the whole check, so a
/// further field or path fails it.
#[test]
fn the_durable_sha_scope_holds_the_plans_and_the_eval_record() {
    let entry = kit_entry("knowledge-base", DURABLE);
    assert_eq!(
        entry["check"],
        serde_json::json!({
            "type": "durable-sha",
            "level": "fail",
            "files": [
                "backlog/**",
                "knowledge/**",
                "docs/specs/**",
                "docs/design.md",
                "docs/plans/**",
                ".claude/evals/record.json"
            ]
        })
    );
}

// ---- case 9 (HR-218): the plans and the eval record ----

/// A scratch repository with an interim commit: a commit on the branch
/// `batch` that `main` does not reach. Returns the scratch, the sha of
/// `main`, and the sha of the interim commit.
fn scratch_with_an_interim_commit() -> (Scratch, String, String) {
    let scratch = Scratch::new();
    let main_tip = scratch.head();
    git(scratch.root(), &["checkout", "-q", "-b", "batch"]);
    let interim = scratch.commit_file("src/lib.rs", b"pub fn lookup() {}\n");
    (scratch, main_tip, interim)
}

/// HR-218: a plan that cites an interim sha fails, at the line of the
/// citation.
#[test]
fn a_plan_citing_an_interim_sha_fails() {
    let (scratch, main_tip, interim) = scratch_with_an_interim_commit();
    let plan = "docs/plans/2026-10-22-batch-1.md";
    scratch.commit_file(
        plan,
        format!("# Plan\n\nTask 1 landed at {interim}.\n").as_bytes(),
    );

    let result = scratch.audit_from(&main_tip);

    let row = row(&result, DURABLE).expect("a row for the durable-sha entry");
    assert_eq!(row["result"], "fail");
    let evidence = row["evidence"].as_str().expect("evidence");
    assert!(evidence.starts_with(&format!("{plan}:3: ")), "{evidence}");
    assert!(evidence.contains(&interim), "{evidence}");
}

/// HR-218: an eval record that cites an interim sha fails, at the line of the
/// citation.
#[test]
fn an_eval_record_citing_an_interim_sha_fails() {
    let (scratch, main_tip, interim) = scratch_with_an_interim_commit();
    let record = ".claude/evals/record.json";
    scratch.commit_file(
        record,
        format!("{{\n  \"runs\": [\n    {{\"template\": \"{interim}\"}}\n  ]\n}}\n").as_bytes(),
    );

    let result = scratch.audit_from(&main_tip);

    let row = row(&result, DURABLE).expect("a row for the durable-sha entry");
    assert_eq!(row["result"], "fail");
    let evidence = row["evidence"].as_str().expect("evidence");
    assert!(evidence.starts_with(&format!("{record}:3: ")), "{evidence}");
    assert!(evidence.contains(&interim), "{evidence}");
}

/// A plan that cites a commit of the default branch passes, and the evidence
/// says that the check read the plan.
#[test]
fn a_plan_citing_a_commit_of_the_default_branch_passes() {
    let (scratch, main_tip, _interim) = scratch_with_an_interim_commit();
    scratch.commit_file(
        "docs/plans/2026-10-22-batch-1.md",
        format!("# Plan\n\nThe base is {main_tip}.\n").as_bytes(),
    );

    let result = scratch.audit_from(&main_tip);

    let row = row(&result, DURABLE).expect("a row for the durable-sha entry");
    assert_eq!(row["result"], "pass");
    assert_eq!(
        row["evidence"],
        format!(
            "1 file(s) checked for in-branch sha citations (against the default branch, \
             {main_tip})"
        )
    );
}

/// Spec 13: the eval run appends the blob ids of template files
/// (`git rev-parse HEAD:<path>`) to the record. A blob id resolves to no
/// commit, so the record passes, and the evidence says that the check read it.
#[test]
fn an_eval_record_holding_a_blob_id_passes() {
    let (scratch, main_tip, _interim) = scratch_with_an_interim_commit();
    let blob = git(scratch.root(), &["rev-parse", "HEAD:AGENTS.md"])
        .trim()
        .to_string();
    scratch.commit_file(
        ".claude/evals/record.json",
        format!("{{\n  \"runs\": [\n    {{\"template\": \"{blob}\"}}\n  ]\n}}\n").as_bytes(),
    );

    let result = scratch.audit_from(&main_tip);

    let row = row(&result, DURABLE).expect("a row for the durable-sha entry");
    assert_eq!(row["result"], "pass");
    assert_eq!(
        row["evidence"],
        format!(
            "1 file(s) checked for in-branch sha citations (against the default branch, \
             {main_tip})"
        )
    );
}
