//! The `validate` command: schema-validates one or more deliverable JSON
//! files against `.claude/schemas/deliverables.json`, plus the two
//! task-report invariants the schema's shape rules cannot express --
//! `tools/kb.mjs`'s `validateDeliverable` and `checkTaskReportAudit`,
//! ported byte-for-byte (batch 17 T3, docs/specs/2026-09-04-batch-15-tier2-
//! spec.md §5 phase 2).
//!
//! Data-layer rule (spec §3): this validates a deliverable file's *shape*,
//! so it stays on the generic, already-ported JSON-Schema-subset engine
//! (`super::validate`, `check.rs`) run directly against the raw
//! `serde_json::Value`, exactly as `rules::mod`'s own module doc already
//! anticipated for this surface -- a typed `TaskReport`/`TaskReview`/
//! `ReReview`/`BranchReview` parse is not this command's validation path
//! and would duplicate the schema engine's semantics a second time in the
//! type system (`quality.principles`: one write path). There is
//! consequently no typed deliverable model layer left for this crate to
//! carry forward -- see this module's sibling doc comment on the deletion
//! of `rules::deliverables` and `json_shape` for the full account.

use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::emit::emit;

use super::deliverable::read_deliverable_value;
use super::model::load_base;

/// The absolute form of `path`, matching Node's `path.resolve(cwd, path)`
/// exactly -- `std::path::absolute` comes close but is not quite it: its
/// own docs say it deliberately keeps `..` components unresolved on
/// POSIX ("this function does not access the filesystem", so a `..` after
/// a possible symlink cannot be collapsed with confidence), where
/// `path.resolve` always collapses them textually, since Node's own
/// version is a pure string operation with no such caution to begin with
/// (verified live against `tools/kb.mjs` at the frozen sha:
/// `path.resolve('/foo/../../baz')` is `/baz`, never `/../baz`). Both
/// join onto `std::env::current_dir`/`process.cwd()` for a relative
/// `path` -- CI issue 1's own root cause was a test asserting a symlinked
/// temp directory's own name would survive that join, when neither
/// engine's real current-directory query ever preserves one
/// (`getcwd(3)`, which both ultimately call, is specified to resolve
/// every symlink; verified live, `tools/kb.sh validate` run through a
/// symlinked cwd reports the real directory, not the symlink's name --
/// see `validate_stats_audit_parity.rs`'s own symlink test). CI round 2's
/// own fix: `strip_verbatim_disk_prefix`'s doc has the Windows-only half
/// of this parity (a `\\?\` extended-length prefix `path.resolve`/
/// `GetFullPathNameW` never produce, but a Windows `canonicalize` --
/// this crate's own `tests/common/mod.rs::repo_root`, an already-absolute
/// argument built from it -- always does).
fn resolve_like_node(path: &Path) -> std::io::Result<PathBuf> {
    if path.is_absolute() {
        return Ok(normalize_lexically(&strip_verbatim_disk_prefix(path)));
    }
    let cwd = std::env::current_dir()?;
    Ok(normalize_lexically(&strip_verbatim_disk_prefix(
        &cwd.join(path),
    )))
}

/// Strips a `\\?\` extended-length-path prefix from `path`'s own plain-disk
/// form, if present -- CI round 2, issue 1 (Windows only; a no-op
/// everywhere else, since the prefix cannot occur there): the binary's
/// own parity slices showed it verbatim (`\\?\D:\a\houserules\...`) where
/// Node's real output never carries one, because `std::fs::canonicalize`
/// returns Windows' own UNC-verbatim form and neither `path.resolve` nor
/// the Win32 calls it wraps (`GetFullPathNameW`, `GetCurrentDirectoryW`)
/// ever produce it (verified against the installed toolchain's own
/// `std::fs::canonicalize` docs: its "Platform-specific behavior" section
/// says plainly that on Windows "this converts the path to use extended
/// length path syntax", the `\\?\` form). Considered the `dunce` crate first
/// (crates.io, security-hygiene.dependency-vetting) -- it does this and
/// more (also declines to strip a path Windows would then read
/// differently: a reserved device name, or one past `MAX_PATH`) -- but
/// its own safety check is `const fn ... -> bool { false }` outside
/// `cfg(windows)` (its published source), making the transformation
/// itself impossible to exercise without a Windows runner, which this
/// development environment does not have; this crate's own narrower,
/// always-active equivalent keeps it testable here (below), with the
/// Windows CI parity slices as the platform proof for the cases it
/// cannot reach: a resolved path shaped this narrowly (a deliverable
/// JSON file a user or agent placed and is now pointing this command
/// at) is not expected to carry a reserved device name or exceed
/// `MAX_PATH`, and matching Node's own unprotected behavior there is the
/// correct parity, not a gap this fix introduces.
fn strip_verbatim_disk_prefix(path: &Path) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

/// Collapses `.`/`..` path components without touching the filesystem --
/// `path.resolve`'s own final normalization step (`resolve_like_node`'s
/// doc). `Path::components` already drops repeated separators and a bare
/// `.` component on its own; only `..` needs handling here: it pops the
/// last pushed normal component, or is dropped outright at the root
/// (there being nothing above it to keep -- verified live: Node's own
/// `path.resolve('/foo/../../baz')` is `/baz`, not `/../baz`).
fn normalize_lexically(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::RootDir) | None => {}
                _ => out.push(component),
            },
            other => out.push(other),
        }
    }
    out
}

/// Path, relative to the repo root, of the agent-deliverables JSON Schema.
const DELIVERABLES_SCHEMA: &str = ".claude/schemas/deliverables.json";

/// Maps a deliverable's `kind` field to its definition name in
/// `DELIVERABLES_SCHEMA` -- `tools/kb.mjs`'s `DELIVERABLE_KINDS`.
const DELIVERABLE_KINDS: [(&str, &str); 4] = [
    ("task-report", "taskReport"),
    ("task-review", "taskReview"),
    ("re-review", "reReview"),
    ("branch-review", "branchReview"),
];

/// `task-report` statuses that claim the task is genuinely finished --
/// `tools/kb.mjs`'s `TERMINAL_STATUSES`.
const TERMINAL_STATUSES: [&str; 2] = ["DONE", "DONE_WITH_CONCERNS"];

/// One validated deliverable: the file it read, the deliverable `kind` it
/// found, and every schema/invariant violation (empty when valid) --
/// `validateDeliverable`'s return shape.
#[derive(Debug)]
pub(super) struct ValidatedDeliverable {
    pub file: PathBuf,
    pub kind: String,
    pub errors: Vec<String>,
}

/// Checks the two task-report invariants the schema's shape rules cannot
/// express: a terminal `status` (DONE or DONE_WITH_CONCERNS) needs a
/// filled `self_audit`, and that audit's `summary.skipped` must be 0 -- a
/// nonzero count means the audit ran without `--report` and skipped every
/// report-field check, so its rows are not trustworthy. BLOCKED and
/// NEEDS_CONTEXT reports are exempt -- `tools/kb.mjs`'s
/// `checkTaskReportAudit`, ported.
fn check_task_report_audit(value: &Value, path: &str, errors: &mut Vec<String>) {
    let Some(status) = value.get("status").and_then(Value::as_str) else {
        return;
    };
    if !TERMINAL_STATUSES.contains(&status) {
        return;
    }
    let self_audit = value.get("self_audit");
    if matches!(self_audit, Some(Value::Null)) {
        errors.push(format!(
            "{path}: status \"{status}\" needs a non-null self_audit"
        ));
        return;
    }
    let skipped = self_audit
        .and_then(|sa| sa.get("summary"))
        .and_then(|summary| summary.get("skipped"));
    if let Some(Value::Number(skipped)) = skipped
        && skipped.as_f64().is_some_and(|n| n > 0.0)
    {
        errors.push(format!(
            "{path}: self_audit.summary.skipped is {skipped}; re-run audit with --report"
        ));
    }
}

/// Validates one deliverable file at `path` against the definition its
/// `kind` names in `root`'s `DELIVERABLES_SCHEMA`, plus
/// `check_task_report_audit` for a `task-report`. `path` is used verbatim
/// as the returned `file` and as every error message's location prefix --
/// callers resolve it to an absolute path first (`cmd_validate` does, the
/// same as `tools/kb.mjs`'s `main` resolving each CLI argument against
/// `cwd` before calling `validateDeliverable`).
fn validate_deliverable(root: &Path, path: &Path) -> Result<ValidatedDeliverable, String> {
    let schema = read_deliverable_value(&root.join(DELIVERABLES_SCHEMA))?;
    let value = read_deliverable_value(path)?;
    let path_display = path.display().to_string();

    let kind_str = value.get("kind").and_then(Value::as_str);
    let def = kind_str.and_then(|kind| {
        DELIVERABLE_KINDS
            .iter()
            .find(|(candidate, _)| *candidate == kind)
            .map(|(_, def)| *def)
    });
    let Some(def) = def else {
        let kind_repr = match value.get("kind") {
            Some(kind_value) => {
                serde_json::to_string(kind_value).unwrap_or_else(|_| "null".to_string())
            }
            None => "undefined".to_string(),
        };
        return Err(format!(
            "{path_display}: unknown deliverable kind {kind_repr}"
        ));
    };

    let mut errors = Vec::new();
    let reference = json!({"$ref": format!("#/$defs/{def}")});
    super::validate(&value, &reference, &path_display, &mut errors, &schema);
    if def == "taskReport" {
        check_task_report_audit(&value, &path_display, &mut errors);
    }
    Ok(ValidatedDeliverable {
        file: path.to_path_buf(),
        kind: kind_str
            .expect("a recognized kind is always a string")
            .to_string(),
        errors,
    })
}

/// Runs the `validate` subcommand: resolves `root` (`--dir`, or the
/// enclosing git repository's top level) and loads the knowledge base
/// there before dispatching -- see `stats::cmd_stats`'s doc for why this
/// replicates `tools/kb.mjs`'s own unconditional `loadBase` call even
/// though `validate_deliverable` needs only `root`'s path, not the loaded
/// base's contents. Validates every file in `files` (each absolutized the
/// same way `tools/kb.mjs`'s `main` resolves its CLI arguments against
/// `cwd`) and prints the JSON results array.
pub(crate) fn cmd_validate(dir: Option<PathBuf>, files: Vec<PathBuf>) -> ExitCode {
    // Root resolution and `load_base` run before the arity check, matching
    // `tools/kb.mjs`'s own `main`: `loadBase(repoRoot(cwd))` always runs
    // ahead of the command `switch`, so `validate`'s own `if
    // (!positional.length) throw ...` is reached only after it. Fix round
    // 1, issue 9 (task-3-review.json): the earlier cut checked arity
    // first, inverting this order for `validate` alone (`cmd_stats`
    // already matched JS here) -- observable on a repository whose
    // `knowledge/schema.json` is invalid: `tools/kb.sh validate` with no
    // files printed the schema load error, `houserules validate` printed
    // "validate needs at least one file" instead, both exit 2.
    let root = match crate::root::resolve_root(dir) {
        Ok(root) => root,
        Err(code) => return code,
    };
    if let Err(error) = load_base(&root) {
        eprintln!("{error}");
        return ExitCode::from(2);
    }
    if files.is_empty() {
        eprintln!("validate needs at least one file");
        return ExitCode::from(2);
    }

    let mut results = Vec::with_capacity(files.len());
    for file in &files {
        let absolute = match resolve_like_node(file) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{}: {error}", file.display());
                return ExitCode::from(2);
            }
        };
        match validate_deliverable(&root, &absolute) {
            Ok(result) => results.push(result),
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        }
    }
    let any_errors = results.iter().any(|result| !result.errors.is_empty());
    let json_results: Vec<Value> = results
        .into_iter()
        .map(|result| {
            json!({
                "file": result.file.display().to_string(),
                "kind": result.kind,
                "errors": result.errors,
            })
        })
        .collect();
    print!("{}", emit(&Value::Array(json_results)));
    if any_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;

    /// CI round 2, issue 1: pure string logic, so this runs the same on
    /// every platform even though the prefix itself only ever appears in
    /// a real path on Windows -- the point of writing it this way rather
    /// than reaching only for a Windows-gated library call, since no
    /// Windows runner exists in this development environment to exercise
    /// one otherwise (this function's own doc has the full account).
    #[test]
    fn strip_verbatim_disk_prefix_removes_the_prefix_from_a_plain_disk_path() {
        assert_eq!(
            strip_verbatim_disk_prefix(Path::new(r"\\?\C:\Users\runneradmin")),
            PathBuf::from(r"C:\Users\runneradmin")
        );
    }

    /// A UNC-share verbatim path (`\\?\UNC\server\share\...`) is not a
    /// plain disk path -- `C` at the position right after the prefix is
    /// `U`, not a drive letter followed by `:` -- so it must be left
    /// alone: stripping it would silently change which server the path
    /// names, not just its cosmetic form.
    #[test]
    fn strip_verbatim_disk_prefix_leaves_a_unc_share_path_alone() {
        let unc = Path::new(r"\\?\UNC\server\share\file.json");
        assert_eq!(strip_verbatim_disk_prefix(unc), unc.to_path_buf());
    }

    /// A path that never carried the prefix at all -- the common case on
    /// every non-Windows platform, and most Windows paths too -- passes
    /// through unchanged.
    #[test]
    fn strip_verbatim_disk_prefix_leaves_an_unprefixed_path_alone() {
        let plain = Path::new("/foo/bar.json");
        assert_eq!(strip_verbatim_disk_prefix(plain), plain.to_path_buf());
    }

    fn template_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template")
    }

    /// A minimal repo root carrying only the vendored deliverables schema --
    /// `validate_deliverable` needs nothing else from `root`.
    fn schema_root() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join(DELIVERABLES_SCHEMA);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::copy(
            template_root().join(".claude/schemas/deliverables.json"),
            &dest,
        )
        .unwrap();
        dir
    }

    fn report_sample() -> Value {
        json!({
            "kind": "task-report",
            "task": 1,
            "backlog": ["WI-001"],
            "status": "DONE",
            "implemented": "x",
            "commits": [{"sha": "abc1234", "subject": "feat: x"}],
            "tests": [{"command": "vitest", "output": "ok"}],
            "live_run": [{"command": "houserules init --dir scratch", "output": "ok", "exit": 0}],
            "tdd": [{
                "test": "t", "mode": "natural",
                "red": {"command": "c", "output": "FAIL"},
                "green": {"command": "c", "output": "PASS"},
            }],
            "files_changed": ["a.mjs"],
            "docs_verified": [],
            "dependency_vetting": null,
            "coverage": null,
            "self_audit": {
                "summary": {
                    "base": "abc1234", "head": "abc1235", "deterministic": 1,
                    "pass": 1, "fail": 0, "warn": 0, "skipped": 0, "judged": 0,
                },
                "rows": [{
                    "id": "process.sequential", "mode": "deterministic",
                    "result": "pass", "evidence": "x",
                }],
            },
            "self_review": [],
            "concerns": [],
            "knowledge_used": ["process.sequential"],
        })
    }

    fn write_report(root: &Path, value: &Value) -> PathBuf {
        let file = root.join("report.json");
        fs::write(&file, serde_json::to_string(value).unwrap()).unwrap();
        file
    }

    /// tests/kb.test.mjs, describe('validate'): "validates a well-formed
    /// task report with no errors".
    #[test]
    fn validates_a_well_formed_task_report_with_no_errors() {
        let root = schema_root();
        let file = write_report(root.path(), &report_sample());
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(result.kind, "task-report");
        assert_eq!(result.errors, Vec::<String>::new());
    }

    /// tests/kb.test.mjs, describe('validate'): "rejects a DONE report with
    /// a null self_audit" (HR-041).
    #[test]
    fn rejects_a_done_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}: status \"DONE\" needs a non-null self_audit",
                file.display()
            )]
        );
    }

    /// tests/kb.test.mjs: "rejects a DONE_WITH_CONCERNS report with a null
    /// self_audit".
    #[test]
    fn rejects_a_done_with_concerns_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("DONE_WITH_CONCERNS");
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}: status \"DONE_WITH_CONCERNS\" needs a non-null self_audit",
                file.display()
            )]
        );
    }

    /// tests/kb.test.mjs: "rejects a DONE report whose
    /// self_audit.summary.skipped is greater than 0" (batch 12 branch
    /// review, template_defects 2).
    #[test]
    fn rejects_a_done_report_whose_self_audit_summary_skipped_is_greater_than_0() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"]["summary"]["skipped"] = json!(2);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}: self_audit.summary.skipped is 2; re-run audit with --report",
                file.display()
            )]
        );
    }

    /// tests/kb.test.mjs: "accepts a BLOCKED report with a null
    /// self_audit".
    #[test]
    fn accepts_a_blocked_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("BLOCKED");
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// tests/kb.test.mjs: "accepts a NEEDS_CONTEXT report with a null
    /// self_audit".
    #[test]
    fn accepts_a_needs_context_report_with_a_null_self_audit() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("NEEDS_CONTEXT");
        report["self_audit"] = Value::Null;
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// tests/kb.test.mjs: "accepts a BLOCKED report whose
    /// self_audit.summary.skipped is greater than 0" -- a non-terminal
    /// report's skipped audit rows are never inspected, pinning
    /// `check_task_report_audit`'s early return.
    #[test]
    fn accepts_a_blocked_report_whose_self_audit_summary_skipped_is_greater_than_0() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("BLOCKED");
        report["self_audit"]["summary"]["skipped"] = json!(2);
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// tests/kb.test.mjs: "rejects a task report without live_run"
    /// (HR-016).
    #[test]
    fn rejects_a_task_report_without_live_run() {
        let root = schema_root();
        let mut report = report_sample();
        report.as_object_mut().unwrap().remove("live_run");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}: missing \"live_run\"", file.display())]
        );
    }

    /// tests/kb.test.mjs: "rejects a live_run that is not an array".
    #[test]
    fn rejects_a_live_run_that_is_not_an_array() {
        let root = schema_root();
        let mut report = report_sample();
        report["live_run"] = json!("nope");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.live_run: must be array", file.display())]
        );
    }

    /// tests/kb.test.mjs: "rejects a live_run entry without a command".
    #[test]
    fn rejects_a_live_run_entry_without_a_command() {
        let root = schema_root();
        let mut report = report_sample();
        report["live_run"] = json!([{"output": "ok"}]);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.live_run[0]: missing \"command\"",
                file.display()
            )]
        );
    }

    /// tests/kb.test.mjs: "rejects a tdd cycle without mode" (HR-017).
    #[test]
    fn rejects_a_tdd_cycle_without_mode() {
        let root = schema_root();
        let mut report = report_sample();
        report["tdd"][0].as_object_mut().unwrap().remove("mode");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.tdd[0]: missing \"mode\"", file.display())]
        );
    }

    /// tests/kb.test.mjs: "rejects a tdd cycle with an unknown mode".
    #[test]
    fn rejects_a_tdd_cycle_with_an_unknown_mode() {
        let root = schema_root();
        let mut report = report_sample();
        report["tdd"][0]["mode"] = json!("guessed");
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.tdd[0].mode: must be one of \"natural\", \"mutation\", \"reconstructed\"",
                file.display()
            )]
        );
    }

    /// tests/kb.test.mjs: "accepts a self_audit summary stamped
    /// empty_range: true" (HR-024).
    #[test]
    fn accepts_a_self_audit_summary_stamped_empty_range_true() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"] = json!({
            "summary": {
                "base": "abc1234", "head": "abc1234", "deterministic": 1,
                "pass": 1, "fail": 0, "warn": 0, "skipped": 0, "judged": 0,
                "empty_range": true,
            },
            "rows": [{
                "id": "process.sequential", "mode": "deterministic",
                "result": "pass", "evidence": "empty range: 0 commits checked",
            }],
        });
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// tests/kb.test.mjs: "rejects a self_audit summary with empty_range:
    /// false".
    #[test]
    fn rejects_a_self_audit_summary_with_empty_range_false() {
        let root = schema_root();
        let mut report = report_sample();
        report["self_audit"] = json!({
            "summary": {
                "base": "abc1234", "head": "abc1235", "deterministic": 1,
                "pass": 1, "fail": 0, "warn": 0, "skipped": 0, "judged": 0,
                "empty_range": false,
            },
            "rows": [{
                "id": "process.sequential", "mode": "deterministic",
                "result": "pass", "evidence": "1 commits checked",
            }],
        });
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!(
                "{}.self_audit.summary.empty_range: must be one of true",
                file.display()
            )]
        );
    }

    /// tests/kb.test.mjs: "reports a bad enum value and an unknown field".
    #[test]
    fn reports_a_bad_enum_value_and_an_unknown_field() {
        let root = schema_root();
        let mut report = report_sample();
        report["status"] = json!("MAYBE");
        report["extra"] = json!(1);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![
                format!(
                    "{}.status: must be one of \"DONE\", \"DONE_WITH_CONCERNS\", \"BLOCKED\", \"NEEDS_CONTEXT\"",
                    file.display()
                ),
                format!("{}: unknown field \"extra\"", file.display()),
            ]
        );
    }

    /// tests/kb.test.mjs: "accepts a run whose exit code is an integer".
    #[test]
    fn accepts_a_run_whose_exit_code_is_an_integer() {
        let root = schema_root();
        let mut report = report_sample();
        report["tests"] = json!([{"command": "vitest", "output": "ok", "exit": 2}]);
        let file = write_report(root.path(), &report);
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// tests/kb.test.mjs: "rejects a run whose exit code is not an
    /// integer".
    #[test]
    fn rejects_a_run_whose_exit_code_is_not_an_integer() {
        let root = schema_root();
        let mut report = report_sample();
        report["tests"] = json!([{"command": "vitest", "output": "ok", "exit": "2"}]);
        let file = write_report(root.path(), &report);
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.tests[0].exit: must be integer", file.display())]
        );
    }

    /// tests/kb.test.mjs: "validates a task-review, rejecting a
    /// rule_adherence result the schema forbids".
    #[test]
    fn validates_a_task_review_rejecting_a_forbidden_rule_adherence_result() {
        let root = schema_root();
        let file = root.path().join("review.json");
        fs::write(
            &file,
            serde_json::to_string(&json!({
                "kind": "task-review", "task": 1, "base": "abc1234", "head": "abc1235",
                "spec_compliance": {"verdict": "compliant", "items": []},
                "rule_adherence": [{"id": "a.b", "mode": "judged", "result": "open", "evidence": "x"}],
                "strengths": [], "issues": [],
                "assessment": {"verdict": "approved", "text": "ok"},
            }))
            .unwrap(),
        )
        .unwrap();
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(result.kind, "task-review");
        assert_eq!(result.errors.len(), 1);
        assert!(
            result.errors[0].ends_with(
                "rule_adherence[0].result: must be one of \"pass\", \"fail\", \"warn\", \"skipped\""
            ),
            "{:?}",
            result.errors
        );
    }

    fn re_review_sample() -> Value {
        json!({
            "kind": "re-review", "task": 1, "round": 1, "fix_base": "abc1234", "head": "abc1235",
            "finding_verdicts": [{"finding": "f", "verdict": "addressed", "evidence": "a.mjs:1"}],
            "rule_adherence": [{"id": "process.sequential", "mode": "judged", "result": "pass", "evidence": "x"}],
            "new_breakage": [], "out_of_scope": [],
            "verdict": {"state": "all-addressed", "open": []},
        })
    }

    /// tests/kb.test.mjs: "accepts a re-review verdict with text" (HR-021).
    #[test]
    fn accepts_a_re_review_verdict_with_text() {
        let root = schema_root();
        let mut re_review = re_review_sample();
        re_review["verdict"]["text"] = json!("scheduled for task 7");
        let file = root.path().join("re-review.json");
        fs::write(&file, serde_json::to_string(&re_review).unwrap()).unwrap();
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// tests/kb.test.mjs: "accepts a re-review verdict without text".
    #[test]
    fn accepts_a_re_review_verdict_without_text() {
        let root = schema_root();
        let file = root.path().join("re-review.json");
        fs::write(&file, serde_json::to_string(&re_review_sample()).unwrap()).unwrap();
        assert_eq!(
            validate_deliverable(root.path(), &file).unwrap().errors,
            Vec::<String>::new()
        );
    }

    /// tests/kb.test.mjs: "rejects a re-review verdict.text of the wrong
    /// type".
    #[test]
    fn rejects_a_re_review_verdict_text_of_the_wrong_type() {
        let root = schema_root();
        let mut re_review = re_review_sample();
        re_review["verdict"]["text"] = json!(42);
        let file = root.path().join("re-review.json");
        fs::write(&file, serde_json::to_string(&re_review).unwrap()).unwrap();
        let result = validate_deliverable(root.path(), &file).unwrap();
        assert_eq!(
            result.errors,
            vec![format!("{}.verdict.text: must be string", file.display())]
        );
    }

    /// tests/kb.test.mjs: "rejects an unknown kind, a missing file, and
    /// invalid JSON as usage errors".
    #[test]
    fn rejects_an_unknown_kind_a_missing_file_and_invalid_json() {
        let root = schema_root();
        let memo = root.path().join("memo.json");
        fs::write(
            &memo,
            serde_json::to_string(&json!({"kind": "memo"})).unwrap(),
        )
        .unwrap();
        let error = validate_deliverable(root.path(), &memo).unwrap_err();
        assert!(
            error.contains("unknown deliverable kind \"memo\""),
            "{error}"
        );

        let missing = root.path().join("missing.json");
        assert!(validate_deliverable(root.path(), &missing).is_err());

        let broken = root.path().join("broken.json");
        fs::write(&broken, "{").unwrap();
        let error = validate_deliverable(root.path(), &broken).unwrap_err();
        assert!(error.contains("invalid JSON"), "{error}");
    }
}
