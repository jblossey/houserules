//! Proves the render/check-knowledge/read-parity golden re-baseline is
//! ONLY the flat-command rewrite -- ported from `tools/diff-shape-gate.mjs`
//! (batch 18 T5 fix round 1, HR-062, review finding "no-Node ruling"): the
//! owner's standing "no Node tools in this codebase" ruling
//! (docs/design.md decision 30) applies to this script the same as it did
//! to `check-report-claims.mjs`, and no interim label like `tools/
//! make-corpus.mjs`'s is available for a script this batch adds from
//! scratch. Dev-only: not shipped in `template/` or the payload, the same
//! status `gen-goldens.rs` has (batch 20 T2, HR-066: `check-report-
//! claims.rs` left this list when it moved behind the flat surface as a
//! shipped subcommand -- `crate::report_claims`'s own module doc has the
//! full account).
//!
//! Diffs the OLD frozen-corpus slices, read from git history at `OLD_REF`
//! (the corpus's own `tests/corpus/render/`, `tests/corpus/check/`, and
//! the four `tests/corpus/knowledge/*for*.json` read-parity files, all of
//! which this task's commits delete from the working tree -- reading them
//! via `git show`/`git ls-tree` instead of the filesystem is what keeps
//! this script rerunnable after that deletion lands), against their
//! replacements on disk today (`tests/goldens/render/`, `tests/goldens/
//! check/`, `tests/goldens/read-parity/`), and asserts every changed line
//! differs from its old counterpart ONLY by substituting entries from
//! `MAPPING_TABLE` below -- no other byte moves. The read-parity extension
//! is fix round 1's own addition (spec §3 boundary clarification, commit
//! 878265b: `standing`'s command string reaches `for`'s JSON, so its
//! slices join the same rewrite boundary render and check-knowledge
//! already did).
//!
//! The command-mapping table this reads is the same one this task's
//! report documents; it is deliberately the narrow subset of that table's
//! rows that these three slice categories actually contain, not the whole
//! rewrite's table (CLAUDE.md's, the agent templates', and the skills'
//! forms never reach this generated output, so a gate scoped to them
//! would never exercise those rows).
//!
//! `OLD_REF` is this batch's own T5 task base (115ed1a in short form), the
//! last commit where `tests/corpus/render/`, `tests/corpus/check/`, and
//! the four read-parity files existed -- full 40-hex form, since a short
//! sha can collide as the repository grows
//! (`houserules.pinned-shas-live-on-mains-ancestry`); it is main's own tip
//! as of the branch point this task built on, so it stays reachable from
//! `origin/main` in a fresh clone.
//!
//! Usage: `cargo run --quiet --bin diff-shape-gate`. Exits 0 when every
//! changed line is explained by the table, prints the file and the two
//! lines and exits 1 on the first line that is not.

use std::path::{Path, PathBuf};
use std::process::Command;

/// This checkout's repository root, resolved at compile time -- every
/// other `src/bin/*.rs` file's own copy of this helper.
fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// This batch's own T5 task base, full 40-hex form -- this module's own
/// doc has the reachability account.
const OLD_REF: &str = "115ed1a4203677fa2bff28617b11e1a5e48ebef3";

/// Every distinct `tools/kb.sh <cmd>` / `tools/backlog.sh <cmd>` ->
/// `houserules <flat>` phrase that appears verbatim inside the render/
/// check-knowledge/read-parity slices (the `GENERATED` header, the
/// per-area `Detail:` footer, the knowledge skill's three
/// retrieval-protocol lines, check-knowledge's "generated file is out of
/// date" hint, and `for`'s `"standing"` field) -- one row per old->new
/// form, per this task's report. Order does not matter for correctness
/// here (no row's old text is a substring of another row's old text), but
/// longest-first keeps the intent visually obvious.
const MAPPING_TABLE: [(&str, &str); 7] = [
    (
        "tools/kb.sh audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>",
        "houserules audit --base <BASE> --head HEAD --ids <ids, comma-separated> --report <REPORT_FILE>",
    ),
    (
        "tools/kb.sh for <every file you will change>",
        "houserules for <every file you will change>",
    ),
    (
        "tools/kb.sh validate <REPORT_FILE>",
        "houserules validate <REPORT_FILE>",
    ),
    ("tools/kb.sh get <ids>", "houserules get <ids>"),
    ("tools/kb.sh get <id>", "houserules get <id>"),
    ("tools/kb.sh standing", "houserules standing"),
    ("tools/kb.sh render", "houserules render"),
];

/// Applies every `MAPPING_TABLE` row to `line`, in table order.
fn apply_mapping(line: &str) -> String {
    let mut mapped = line.to_string();
    for (old_text, new_text) in MAPPING_TABLE {
        mapped = mapped.replace(old_text, new_text);
    }
    mapped
}

/// One unexplained line: the file (and field, for a JSON capture) it came
/// from, its 1-based line index (-1 for a whole-file/file-set problem),
/// the old and new text, and what the mapping table produced (or a named
/// reason the line pair was never a same-line substitution at all).
struct Problem {
    file: String,
    index: i64,
    old_line: String,
    new_line: String,
    mapped: String,
}

/// `git show <OLD_REF>:<rel_path>` as a UTF-8 string -- panics (loudly) if
/// the path never existed there.
fn read_at_old_ref(root: &Path, rel_path: &str) -> String {
    let output = Command::new("git")
        .args(["show", &format!("{OLD_REF}:{rel_path}")])
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("run git show {OLD_REF}:{rel_path}: {error}"));
    assert!(
        output.status.success(),
        "git show {OLD_REF}:{rel_path} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap_or_else(|error| panic!("{rel_path}: not UTF-8: {error}"))
}

/// Every regular file `git ls-tree` finds under `dir_path` at `OLD_REF`,
/// repo-relative POSIX paths, sorted.
fn list_at_old_ref(root: &Path, dir_path: &str) -> Vec<String> {
    let output = Command::new("git")
        .args(["ls-tree", "-r", "--name-only", OLD_REF, "--", dir_path])
        .current_dir(root)
        .output()
        .unwrap_or_else(|error| panic!("run git ls-tree for {dir_path}: {error}"));
    assert!(output.status.success(), "git ls-tree {dir_path} failed");
    let text = String::from_utf8(output.stdout).expect("utf8 ls-tree output");
    let mut paths: Vec<String> = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort();
    paths
}

/// Every regular file under `dir` on disk today, recursed, repo-relative
/// POSIX paths, sorted.
fn walk_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("read_dir {}: {error}", dir.display()))
        .filter_map(|entry| entry.ok())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_files(root, &path, out);
        } else {
            let rel = path
                .strip_prefix(root)
                .expect("path is under root")
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
}

/// Compares `old_text` and `new_text` line by line, returning every
/// unexplained line (empty when every changed line is explained by
/// `apply_mapping`). A line-count mismatch is reported as one problem for
/// the whole pair, not per line -- a same-line substitution can never
/// change how many lines a file has.
fn unexplained_lines(old_text: &str, new_text: &str) -> Vec<Problem> {
    let old_lines: Vec<&str> = old_text.split('\n').collect();
    let new_lines: Vec<&str> = new_text.split('\n').collect();
    if old_lines.len() != new_lines.len() {
        return vec![Problem {
            file: String::new(),
            index: -1,
            old_line: format!("<{} lines>", old_lines.len()),
            new_line: format!("<{} lines>", new_lines.len()),
            mapped: "<line count itself changed -- not a same-line substitution>".to_string(),
        }];
    }
    let mut problems = Vec::new();
    for (index, (old_line, new_line)) in old_lines.iter().zip(new_lines.iter()).enumerate() {
        if old_line == new_line {
            continue;
        }
        let mapped = apply_mapping(old_line);
        if mapped != *new_line {
            problems.push(Problem {
                file: String::new(),
                index: index as i64 + 1,
                old_line: old_line.to_string(),
                new_line: new_line.to_string(),
                mapped,
            });
        }
    }
    problems
}

/// Checks one whole-file category (`render`): every file under
/// `tests/corpus/<category>/` at `OLD_REF` must exist, byte-for-byte
/// mapped, under `tests/goldens/<category>/` today. Two-stage contract:
/// the file SET must match exactly first (a missing or extra golden is
/// its own problem, reported once, without opening a single file); only
/// once the sets agree does this compare file contents line by line.
fn check_whole_file_category(root: &Path, category: &str) -> Vec<Problem> {
    let old_prefix = format!("tests/corpus/{category}/");
    let new_dir = root.join("tests/goldens").join(category);
    let mut old_set: Vec<String> = list_at_old_ref(root, &format!("tests/corpus/{category}"))
        .into_iter()
        .map(|p| p[old_prefix.len()..].to_string())
        .collect();
    old_set.sort();
    let mut new_set = Vec::new();
    walk_files(&new_dir, &new_dir, &mut new_set);
    new_set.sort();
    if old_set != new_set {
        return vec![Problem {
            file: format!("({category} file set)"),
            index: -1,
            old_line: old_set.join(", "),
            new_line: new_set.join(", "),
            mapped: "<the golden file set itself differs from the frozen corpus file set>"
                .to_string(),
        }];
    }
    let mut problems = Vec::new();
    for relative in &old_set {
        let old_text = read_at_old_ref(root, &format!("{old_prefix}{relative}"));
        let new_text = std::fs::read_to_string(new_dir.join(relative))
            .unwrap_or_else(|error| panic!("read {}: {error}", new_dir.join(relative).display()));
        for mut problem in unexplained_lines(&old_text, &new_text) {
            problem.file = format!("{old_prefix}{relative}");
            problems.push(problem);
        }
    }
    problems
}

/// Checks the `check` category: every `*.json` file under
/// `tests/corpus/check/` at `OLD_REF` (the whole directory moved as one
/// unit), matched by name against `tests/goldens/check/`, each parsed and
/// compared field by field (`stdout`, `stderr` line by line; `exit` as a
/// single scalar). Two-stage contract, same as `check_whole_file_category`:
/// the file-name SET must match exactly first; only once it does does this
/// open and compare any capture's fields.
fn check_check_category(root: &Path) -> Vec<Problem> {
    let old_prefix = "tests/corpus/check/";
    let mut old_names: Vec<String> = list_at_old_ref(root, "tests/corpus/check")
        .into_iter()
        .map(|p| p[old_prefix.len()..].to_string())
        .filter(|name| name.ends_with(".json"))
        .collect();
    old_names.sort();
    check_named_json_captures(root, old_prefix, "check", &old_names)
}

/// Checks the `read-parity` category: `for`'s four slices (root and
/// `mini`, bare and `--full`), the only files batch 18 T5 fix round 1
/// moved out of `tests/corpus/knowledge/` -- an EXPLICIT list, not that
/// whole directory's contents, since most of `tests/corpus/knowledge/`
/// (topics/index/standing/get) carries no command reference and stays
/// frozen (this module's own doc has the full account).
fn check_read_parity_category(root: &Path) -> Vec<Problem> {
    let old_prefix = "tests/corpus/knowledge/";
    let old_names = [
        "for-tools-kb-mjs.json".to_string(),
        "for-tools-kb-mjs-full.json".to_string(),
        "mini/for-mini-tools-build-sh.json".to_string(),
        "mini/for-mini-tools-build-sh-full.json".to_string(),
    ];
    check_named_json_captures(root, old_prefix, "read-parity", &old_names)
}

/// Shared two-stage contract for a named list of JSON captures: the SET of
/// `old_names` (each resolved at `OLD_REF` under `old_prefix`) must exist,
/// unchanged in membership, under `tests/goldens/<new_dir_name>/` first (a
/// missing or extra golden is its own problem, reported once, without
/// opening a single file); only once the sets agree does this parse each
/// pair and compare `stdout`/`stderr` line by line and `exit` as a scalar.
fn check_named_json_captures(
    root: &Path,
    old_prefix: &str,
    new_dir_name: &str,
    old_names: &[String],
) -> Vec<Problem> {
    let new_dir = root.join("tests/goldens").join(new_dir_name);
    let mut new_names = Vec::new();
    walk_files(&new_dir, &new_dir, &mut new_names);
    new_names.retain(|name| name.ends_with(".json"));
    new_names.sort();
    let mut expected_names = old_names.to_vec();
    expected_names.sort();
    if expected_names != new_names {
        return vec![Problem {
            file: format!("({new_dir_name} file set)"),
            index: -1,
            old_line: expected_names.join(", "),
            new_line: new_names.join(", "),
            mapped: "<the golden file set itself differs from the frozen corpus file set>"
                .to_string(),
        }];
    }
    let mut problems = Vec::new();
    for name in old_names {
        let old_capture: serde_json::Value =
            serde_json::from_str(&read_at_old_ref(root, &format!("{old_prefix}{name}")))
                .unwrap_or_else(|error| panic!("parse {old_prefix}{name}: {error}"));
        let new_text = std::fs::read_to_string(new_dir.join(name))
            .unwrap_or_else(|error| panic!("read {}: {error}", new_dir.join(name).display()));
        let new_capture: serde_json::Value =
            serde_json::from_str(&new_text).unwrap_or_else(|error| panic!("parse {name}: {error}"));
        for field in ["stdout", "stderr"] {
            let old_field = old_capture[field].as_str().unwrap_or("");
            let new_field = new_capture[field].as_str().unwrap_or("");
            for mut problem in unexplained_lines(old_field, new_field) {
                problem.file = format!("{old_prefix}{name} ({field})");
                problems.push(problem);
            }
        }
        let old_exit = old_capture["exit"].as_i64();
        let new_exit = new_capture["exit"].as_i64();
        if old_exit != new_exit {
            problems.push(Problem {
                file: format!("{old_prefix}{name} (exit)"),
                index: -1,
                old_line: format!("{old_exit:?}"),
                new_line: format!("{new_exit:?}"),
                mapped: "<exit code changed -- not a text substitution>".to_string(),
            });
        }
    }
    problems
}

/// Runs all three category checks (`render`, `check`, `read-parity`),
/// prints every unexplained line and exits 1 if any remain, or prints the
/// single PASS summary line and exits 0 -- this module's own doc has the
/// full account of what "unexplained" means.
fn main() {
    let root = repo_root();
    let mut problems = check_whole_file_category(&root, "render");
    problems.extend(check_check_category(&root));
    problems.extend(check_read_parity_category(&root));

    if !problems.is_empty() {
        println!("-- {} unexplained line(s) --", problems.len());
        for problem in &problems {
            println!("{}:{}", problem.file, problem.index);
            println!("  old:    {}", problem.old_line);
            println!("  new:    {}", problem.new_line);
            println!("  mapped: {}", problem.mapped);
        }
        println!("\nsummary: {} unexplained line(s) -- FAIL", problems.len());
        std::process::exit(1);
    }
    println!(
        "summary: every changed render/check-knowledge/read-parity line (old content read from {OLD_REF}) is explained by the command-mapping table -- PASS"
    );
}
