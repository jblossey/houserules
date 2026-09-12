//! `checkBase` and the `check-knowledge` command: `tools/kb.mjs`'s
//! knowledge-base validator, ported byte-for-byte (HR-054 task 4; the
//! reviewed goldens under `tests/goldens/check/` are the parity gate --
//! see `crates/houserules/tests/check_parity.rs`).
//!
//! `checkBase`'s own JSON-Schema-subset validator is
//! `template/tools/lib/json-store.mjs`'s `validate` (lines 97-156 at the
//! frozen sha), ported here as `validate` operating on `serde_json::Value`
//! directly: `checkBase` runs it against every knowledge file's raw
//! content, so the check surface needs the schema-subset engine, not only
//! the typed `Entry`/`AreaDef` shapes `render.rs` reads (see
//! `model.rs`'s module doc for why loading stays lenient about shape).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use regress::Regex;
use serde_json::Value;

use super::glob::compile;
use super::model::{Base, load_base};
use super::render::{RULE_KINDS, SKILL_PATH, render_all};

/// Size limits `check_base` enforces on the generated markdown files and
/// `CLAUDE.md` -- the exact values `tools/kb.mjs`'s `BUDGETS` constant
/// pins (`claudeMdLines`/`claudeMdBytes`/`standingLines`/`areaLines`/
/// `skillLines`).
struct Budgets {
    claude_md_lines: usize,
    claude_md_bytes: usize,
    standing_lines: usize,
    area_lines: usize,
    skill_lines: usize,
}

const BUDGETS: Budgets = Budgets {
    claude_md_lines: 200,
    claude_md_bytes: 12288,
    standing_lines: 60,
    area_lines: 160,
    skill_lines: 120,
};

/// Maps a `check` entry's `type` to the fields `check_shape` requires it to
/// carry -- `tools/kb.mjs`'s `CHECK_FIELDS`. An unknown type (already
/// reported by the schema's own `enum`) yields `None`, so `check_shape`
/// skips it rather than reporting it a second time.
fn check_fields(check_type: &str) -> Option<&'static [&'static str]> {
    match check_type {
        "grep-absent" => Some(&["files", "pattern", "scope"]),
        "commits" => Some(&[]),
        "co-change" => Some(&["if", "then"]),
        "diff-append-only" => Some(&["files"]),
        "report-field" => Some(&["if", "field"]),
        _ => None,
    }
}

/// JavaScript truthiness for a JSON value read through `serde_json`:
/// absent, `null`, `false`, `0`, and `""` are falsy; every array, object,
/// non-empty string, non-zero number, and `true` is truthy. `check_shape`'s
/// `commits`-needs-a-field rule (`!check.subject && !check.body_absent &&
/// !check.body_line_max`) and `check_base`'s `if (item.standing && ...)`
/// guard (fix round 1, finding 3) are its callers; `model::load_base`'s
/// `CheckField` classification (batch 17 T3 fix round 1, issue 7) is a
/// third, reusing this rather than a second hand-written truthiness check
/// that could drift from it.
pub(super) fn falsy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => !b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f == 0.0),
        Some(Value::String(s)) => s.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => false,
    }
}

/// JavaScript's default `ToString` coercion for a JSON value read through
/// `serde_json`, the same conversion a template literal (`` `${id}` ``)
/// applies -- distinct from `JSON.stringify`, notably for strings (no
/// added quotes), arrays (comma-joined elements, not bracketed), and
/// objects (always the literal `"[object Object]"`, never their fields).
/// `check_base`'s "see"/"verify" messages interpolate a non-string entry
/// this way (fix round 1, finding 3; verified live, node 24.18.1:
/// `${123}` -> `"123"`, `${[1,2]}` -> `"1,2"`, `${{a:1}}` -> `"[object
/// Object]"`, `${null}` -> `"null"`, `${true}` -> `"true"`), and
/// `check_shape` coerces `check.flags` the same way when it is present
/// but not a string (`check.flags ?? ''` keeps a non-nullish value
/// as-is, and the `RegExp` constructor's own `ToString` on it is this
/// same coercion).
fn to_js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(items) => items.iter().map(to_js_string).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// ECMAScript `RegExp` flag-string validity: every character must be one
/// of `dgimsuvy`, no character may repeat, and `u`/`v` are mutually
/// exclusive -- the checks V8's `RegExp` constructor performs before it
/// even looks at the pattern (verified live, node 24.18.1: `new
/// RegExp('(', 'zz')` throws the flags message, not a pattern one, so an
/// invalid `flags` value is checked first). `regress::Regex::with_flags`
/// does not perform this check itself (verified live, regress 0.12.0:
/// `with_flags("a", "zz")` is `Ok`), so `regex_validity_message` does it
/// here first. The message, when invalid, is V8's own and fully
/// reproducible (verified live for `"zz"`): it names the whole flags
/// string, not a single offending character.
fn validate_flags(flags: &str) -> Result<(), String> {
    const VALID: &str = "dgimsuvy";
    let mut seen = HashSet::new();
    for c in flags.chars() {
        if !VALID.contains(c) || !seen.insert(c) {
            return Err(format!(
                "Invalid flags supplied to RegExp constructor '{flags}'"
            ));
        }
    }
    if seen.contains(&'u') && seen.contains(&'v') {
        return Err(format!(
            "Invalid flags supplied to RegExp constructor '{flags}'"
        ));
    }
    Ok(())
}

/// Classifies `pattern`'s structural defect into one of the four V8 error
/// reasons this port reproduces byte-exact (verified live, node 24.18.1):
/// an unterminated group, an unmatched `)`, an unterminated character
/// class, or a trailing bare backslash -- a balanced, unescaped-groups-
/// and-classes scan, the same one this function used to decide validity
/// with before fix round 1, finding 2 (task-4-review.json: the scan's
/// verdict itself diverged from V8's for `*abc`, `a{2,1}`, and
/// `(?<1x>a)`, all invalid patterns the scan accepted). `None` when
/// `pattern` fits none of these four shapes; `regex_validity_message`
/// then falls back to `regress`'s own reason text for the residual
/// categories -- the recorded deviation (docs/specs/
/// 2026-09-04-batch-15-tier2-spec.md §6): full V8 reason-text parity is
/// unreachable without embedding a JS engine, but the validity verdict
/// itself now always matches, decided by `regress`, an ECMAScript-regex
/// engine, not this classifier.
fn classify_structural_reason(pattern: &str) -> Option<&'static str> {
    let mut paren_depth: i32 = 0;
    let mut in_class = false;
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if chars.next().is_none() {
                    return Some("\\ at end of pattern");
                }
            }
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '(' if !in_class => paren_depth += 1,
            ')' if !in_class => {
                paren_depth -= 1;
                if paren_depth < 0 {
                    return Some("Unmatched ')'");
                }
            }
            _ => {}
        }
    }
    if in_class {
        return Some("Unterminated character class");
    }
    if paren_depth > 0 {
        return Some("Unterminated group");
    }
    None
}

/// The message `check_shape` reports for one regex-typed field, or `None`
/// when `pattern` compiles under `flags` -- checkShape's `try { RegExp(...)
/// } catch (error) { ... error.message }`, ported: `flags` is checked
/// first (matching V8's own order), then `pattern` is decided by
/// `regress`, an ECMAScript-syntax engine (fix round 1, finding 2: this
/// replaced a hand-rolled structural scan whose verdict itself diverged
/// from V8's; `classify_structural_reason` still runs, but only to choose
/// a V8-exact reason string once `regress` has already said "invalid").
fn regex_validity_message(pattern: &str, flags: &str) -> Option<String> {
    if let Err(message) = validate_flags(flags) {
        return Some(message);
    }
    match Regex::with_flags(pattern, flags) {
        Ok(_) => None,
        Err(error) => {
            let reason = classify_structural_reason(pattern)
                .map_or_else(|| error.to_string(), str::to_string);
            Some(format!(
                "Invalid regular expression: /{pattern}/{flags}: {reason}"
            ))
        }
    }
}

/// Validates one entry's `check` object's shape -- `tools/kb.mjs`'s
/// `checkShape`: every field its `type` requires (`check_fields`), the
/// `commits` type's own "needs one of three" rule, and that `pattern`,
/// `subject`, and `body_absent` (whichever are present as strings) compile
/// as regular expressions. `at` prefixes every message the same way the
/// caller's other per-entry messages are prefixed.
fn check_shape(check: &Value, at: &str, errors: &mut Vec<String>) {
    let check_type = check.get("type").and_then(Value::as_str).unwrap_or("");
    let Some(fields) = check_fields(check_type) else {
        return; // the schema already reported an unknown type
    };
    for field in fields {
        if check.get(*field).is_none() {
            errors.push(format!("{at}: check \"{check_type}\" needs \"{field}\""));
        }
    }
    if check_type == "commits"
        && falsy(check.get("subject"))
        && falsy(check.get("body_absent"))
        && falsy(check.get("body_line_max"))
    {
        errors.push(format!(
            "{at}: check \"commits\" needs \"subject\", \"body_absent\", or \"body_line_max\""
        ));
    }
    // `check.flags ?? ''`: only an absent key or an explicit `null`
    // defaults to empty; any other value -- including one the schema
    // never sanctions, like a number -- is coerced and used as-is,
    // matching `RegExp`'s own `ToString` on a non-string flags argument
    // (fix round 1, finding 3).
    let flags = match check.get("flags") {
        None | Some(Value::Null) => String::new(),
        Some(value) => to_js_string(value),
    };
    for field in ["pattern", "subject", "body_absent"] {
        let Some(pattern) = check.get(field).and_then(Value::as_str) else {
            continue;
        };
        if let Some(message) = regex_validity_message(pattern, &flags) {
            errors.push(format!(
                "{at}: check {field} is not a valid regex ({message})"
            ));
        }
    }
}

/// Checks `path` (relative to `root`) against a line budget and, when
/// `max_bytes` is given, a byte budget -- `tools/kb.mjs`'s `checkBudget`.
/// A missing file is its own finding, distinct from either budget.
fn check_budget(
    root: &Path,
    path: &str,
    max_lines: usize,
    max_bytes: Option<usize>,
    errors: &mut Vec<String>,
) {
    let abs = root.join(path);
    let Ok(text) = fs::read_to_string(&abs) else {
        errors.push(format!("{path}: missing"));
        return;
    };
    let lines = text.split('\n').count() - usize::from(text.ends_with('\n'));
    if lines > max_lines {
        errors.push(format!("{path}: {lines} lines, budget {max_lines}"));
    }
    let bytes = text.len();
    if let Some(max_bytes) = max_bytes
        && bytes > max_bytes
    {
        errors.push(format!("{path}: {bytes} bytes, budget {max_bytes}"));
    }
}

/// Resolves a local `$ref` (`#/a/b/...`) against `root`, the one indirection
/// `validate`'s schema walk follows -- `tools/lib/json-store.mjs`'s
/// `deref`. The JS original throws on an unsupported or unresolved ref,
/// crashing the whole check; every schema this binary ships resolves
/// cleanly, so this path is unreached in practice, but `validate` reports
/// it as a finding instead of panicking (`quality.principles`: prefer a
/// checked failure to a crash) when it is.
fn deref<'a>(root: &'a Value, reference: &str) -> Result<&'a Value, String> {
    let Some(path) = reference.strip_prefix("#/") else {
        return Err(format!("unsupported $ref {reference}"));
    };
    let mut node = root;
    for key in path.split('/') {
        match node.get(key) {
            Some(next) => node = next,
            None => return Err(format!("unresolved $ref {reference}")),
        }
    }
    Ok(node)
}

/// JavaScript's `Object.keys()` for a JSON value read through
/// `serde_json`: an object's own keys in declared order; an array's
/// index keys as strings (`["a","b"]` -> `["0","1"]`, the same coercion
/// `checkBase`'s `Object.keys(areas)` performs when `areas.json` parses to
/// a JSON array instead of an object); anything else, none. `check_base`'s
/// "unknown area" sweep is the one caller: `areas.json` as an array must
/// still report each of its elements' index as an unknown area, matching
/// the frozen JS exactly (fix round 1, finding 1).
fn object_keys(value: &Value) -> Vec<String> {
    match value {
        Value::Object(map) => map.keys().cloned().collect(),
        Value::Array(items) => (0..items.len()).map(|i| i.to_string()).collect(),
        _ => Vec::new(),
    }
}

/// `true` when `value` satisfies one of `type_spec`'s JSON Schema type
/// names (a single string or an array of them) -- `tools/lib/json-store.mjs`'s
/// `hasType`.
fn has_type(value: &Value, type_spec: &Value) -> bool {
    let types: Vec<&str> = match type_spec {
        Value::Array(types) => types.iter().filter_map(Value::as_str).collect(),
        Value::String(t) => vec![t.as_str()],
        _ => Vec::new(),
    };
    types.iter().any(|t| match *t {
        "null" => value.is_null(),
        "array" => value.is_array(),
        "object" => value.is_object(),
        "integer" => value.is_number() && value.as_f64().is_some_and(|n| n.fract() == 0.0),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "number" => value.is_number(),
        _ => false,
    })
}

/// Renders a JSON Schema `type` field the way a "must be X" finding names
/// it: a bare word for one type, `"a or b"` for a list of them.
fn type_name(type_spec: &Value) -> String {
    match type_spec {
        Value::Array(types) => types
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" or "),
        Value::String(t) => t.clone(),
        _ => String::new(),
    }
}

/// Validates `value` against a JSON Schema subset: local `$ref`, `type`
/// (string or list), `enum`, `pattern`, `minLength`, `maxLength`,
/// `minimum`, `items`, `uniqueItems`, `required`, `properties`,
/// `additionalProperties` (`false` or a schema) -- ported from
/// `template/tools/lib/json-store.mjs`'s `validate`, message text and
/// early-return branches both included: an `enum` or `type` mismatch stops
/// that branch there, exactly as the JS does, so a value that fails `type`
/// is never also reported against the string/number/array/object rules
/// below it. Every violation is appended to `errors` as `<at>: <problem>`.
/// `pattern` compiles with `regress` (an ECMAScript-syntax engine, the
/// same one `regex_validity_message` uses): a `pattern` that fails to
/// compile is itself a named finding (`schema pattern ... does not
/// compile`), never a silent skip of the constraint (fix round 1, finding
/// 5 -- the eager-glob precedent's principle, applied here: a malformed
/// input is reported, not swallowed).
pub(crate) fn validate(
    value: &Value,
    schema: &Value,
    at: &str,
    errors: &mut Vec<String>,
    root: &Value,
) {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        match deref(root, reference) {
            Ok(target) => validate(value, target, at, errors, root),
            Err(message) => errors.push(format!("{at}: {message}")),
        }
        return;
    }
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array)
        && !enum_values.contains(value)
    {
        let rendered = enum_values
            .iter()
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ");
        errors.push(format!("{at}: must be one of {rendered}"));
        return;
    }
    if let Some(type_spec) = schema.get("type")
        && !has_type(value, type_spec)
    {
        errors.push(format!("{at}: must be {}", type_name(type_spec)));
        return;
    }
    if let Value::String(s) = value {
        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
            match Regex::new(pattern) {
                Ok(re) => {
                    if re.find(s).is_none() {
                        errors.push(format!("{at}: must match {pattern}"));
                    }
                }
                Err(_) => {
                    errors.push(format!("{at}: schema pattern {pattern:?} does not compile"));
                }
            }
        }
        let length = s.encode_utf16().count();
        if let Some(min_length) = schema.get("minLength").and_then(Value::as_u64)
            && (length as u64) < min_length
        {
            errors.push(format!("{at}: shorter than {min_length}"));
        }
        if let Some(max_length) = schema.get("maxLength").and_then(Value::as_u64)
            && (length as u64) > max_length
        {
            errors.push(format!("{at}: longer than {max_length} characters"));
        }
    }
    if let Value::Number(n) = value
        && let Some(minimum) = schema.get("minimum").and_then(Value::as_f64)
        && n.as_f64().is_some_and(|v| v < minimum)
    {
        errors.push(format!("{at}: below {minimum}"));
    }
    if let Value::Array(items) = value {
        if let Some(item_schema) = schema.get("items") {
            for (i, item) in items.iter().enumerate() {
                validate(item, item_schema, &format!("{at}[{i}]"), errors, root);
            }
        }
        if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
            let mut seen = HashSet::new();
            let all_unique = items
                .iter()
                .all(|item| seen.insert(serde_json::to_string(item).unwrap_or_default()));
            if !all_unique {
                errors.push(format!("{at}: items must be unique"));
            }
        }
    }
    if let Value::Object(map) = value {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !map.contains_key(key) {
                    errors.push(format!("{at}: missing \"{key}\""));
                }
            }
        }
        for (key, child) in map {
            let child_schema = schema.get("properties").and_then(|p| p.get(key));
            match child_schema {
                Some(child_schema) => {
                    validate(child, child_schema, &format!("{at}.{key}"), errors, root)
                }
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => {
                        errors.push(format!("{at}: unknown field \"{key}\""));
                    }
                    Some(additional @ Value::Object(_)) => {
                        validate(child, additional, &format!("{at}.{key}"), errors, root);
                    }
                    _ => {}
                },
            }
        }
    }
}

/// Where `tracked_tree_files` derived its file list from -- named in
/// every dead-glob finding (fix round 1, important issue 2) so a reader
/// knows which set decided liveness, since the two disagree exactly on
/// the residue a gitignored or untracked file leaves behind.
#[derive(Clone, Copy)]
enum TreeSource {
    /// `git ls-files` succeeded: this repository's tracked files, a
    /// gitignored or plain untracked file excluded regardless of whether
    /// it sits on disk.
    GitTracked,
    /// `git ls-files` found nothing to consult: `root` is not a git
    /// working tree, git itself could not run there, or `root` IS a git
    /// repository but its index is still empty (nothing `git add`ed
    /// yet). Every file on disk, minus a top-level `.git`, stands in
    /// instead.
    Filesystem,
}

impl TreeSource {
    /// The parenthetical a dead-glob finding names its source with.
    fn label(self) -> &'static str {
        match self {
            TreeSource::GitTracked => "git ls-files",
            TreeSource::Filesystem => "filesystem walk, no git-tracked files found",
        }
    }
}

/// The file list `check_base`'s dead-glob gate (HR-074) matches area
/// globs against, as forward-slash-relative paths, plus which of the two
/// derivations below produced it. `git ls-files` is primary wherever
/// `root` is a git working tree: the residue-gate's own pattern
/// (`bin/residue-gate.rs`'s module doc, `quality.gates-derive-their-
/// scope`) and the one that answers "tracked" the way the spec's own
/// "files in the tracked tree" language means it -- a directory retired
/// from tracking (HR-074's own incident: cli's `bin/**` after the T3
/// retirement) must go dead the moment `git rm`/an untracked rename takes
/// it out of the index, not stay alive on whichever machine still has the
/// old file, or is missing a `.gitignore`'d one, on disk (fix round 1,
/// important issue 2: reproduced with a gitignored `bin/artifact.bin`
/// and an untracked `tools/legacy/old.sh`, both of which kept their globs
/// alive under a filesystem-only derivation while `git ls-files` saw
/// neither).
///
/// The filesystem walk is an explicit, documented fallback for the two
/// cases `git ls-files` cannot answer for: `root` is not a git working
/// tree at all (a non-git `--dir` tree, or this module's own un-init'ed
/// tempdir test fixtures) -- `check-knowledge` is a shipped, git-
/// independent command (`--dir` bypasses git resolution entirely, and
/// `check_base` itself never calls git otherwise, `model::load_base`'s
/// own module doc), so a tree with no git history at all must still get
/// an answer -- or `root` is a git repository whose index is empty (this
/// function's own inline doc has the second case's own account).
///
/// The second element is every tracked path `git_ls_files` could not
/// decode as UTF-8 (fix round 3, new_breakage 1, task-1-review-r3.json),
/// named so `check_base` can report each as its own finding -- never
/// silently dropped, and never allowed to change the derivation `label`
/// reports for every OTHER, valid path. `TreeSource` keeps exactly two
/// variants for this rather than gaining a third: a decode failure never
/// pushes a repository off the git derivation on its own (that still
/// only happens when zero paths decode, the pre-existing empty-set
/// case above), so `GitTracked`'s own label stays true for every finding
/// that names a real glob -- there is no third state for it to speak
/// for. A file this narrowly targeted decode failure could plausibly
/// still cause -- every tracked path failing to decode at once -- falls
/// through to the empty-set branch below, and Filesystem's own label
/// already covers the FILE LIST honestly there (git returned nothing
/// USABLE, which is what "no git-tracked files found" means). The SKIPS
/// still travel with it, though (fix round 4, new_breakage, task-1-
/// review-r4.json): the branch below carries whatever `git_ls_files`
/// named as undecodable into its own return, rather than discarding
/// them the moment `files` came back empty -- a repository whose one
/// tracked path is entirely non-UTF-8 still gets that path named, not
/// silently absorbed into a bare `Filesystem` fallback that looks
/// exactly like "nothing is tracked here at all".
fn tracked_tree_files(root: &Path) -> Result<(Vec<String>, TreeSource, Vec<String>), String> {
    // An empty index falls back too (found live-running this same fix
    // round's own remedy, task-1-review.json's important issue 2): a
    // `git init` with nothing yet `git add`ed reads identically to "no
    // git repository" from a liveness point of view -- reproduced live,
    // `git init` then `houserules init --dir .` then `check-knowledge`
    // (exactly the sequence `init` itself prints as "next:") flagged
    // EVERY area's EVERY glob dead before this fallback existed, since
    // nothing had been staged yet. The residue this gate exists to
    // exclude (a gitignored or untracked file surviving a real
    // retirement) presupposes an established, non-empty tracked history
    // to retire something FROM; an index with nothing in it at all is
    // never that case, so trusting the filesystem there costs nothing
    // the primary derivation was built to catch.
    let (files, skipped) = git_ls_files(root).unwrap_or_default();
    if !files.is_empty() {
        return Ok((files, TreeSource::GitTracked, skipped));
    }
    // Fix round 4, new_breakage (task-1-review-r4.json): `skipped` is
    // carried into the Filesystem fallback too, not dropped with
    // `files` -- every tracked path git named but could not decode
    // still gets its own finding even in the corner where NONE of
    // git's entries decoded (so `files` above is empty and this
    // function falls back to the filesystem walk for its file list).
    // Losing `skipped` here would be the exact silence fix round 3
    // closed, surviving in the one branch that fix did not wire.
    let files = filesystem_tree_files(root)?;
    Ok((files, TreeSource::Filesystem, skipped))
}

/// Runs `git ls-files -z` at `root`, decoding each NUL-separated entry on
/// its own -- `None` when `root` is not a git working tree or git itself
/// is not on `PATH`; `tracked_tree_files` walks the filesystem instead in
/// either case. Never panics: an unavailable git is exactly the "fall
/// back" case from this function's point of view, not a fatal one
/// (`bin/residue-gate.rs`'s own `tracked_files` panics on the identical
/// command, but that dev-only tool always runs inside this checkout's
/// own git history, where the command cannot fail this way).
///
/// `-z` is not optional (fix round 2, new_breakage 1, task-1-
/// review-r2.json): without it, git's own `core.quotePath` (on by
/// default) double-quotes and octal-escapes any path byte outside
/// printable ASCII, so a tracked `docs/café.md` came back as the
/// 12-character-escaped literal `"docs/caf\303\251.md"` -- a string no
/// glob matches -- and a naive `.lines()` split has the identical
/// failure mode for an embedded newline. `-z` disables that quoting and
/// NUL-terminates each entry instead of newline-terminating it.
///
/// Decoding happens per entry, not once over the whole buffer (fix round
/// 3, new_breakage 1, task-1-review-r3.json): a single `String::from_utf8`
/// over the joined output turned ONE tracked path with non-UTF-8 bytes
/// (a latin-1 filename, say) into a `None` for the ENTIRE result, which
/// silently downgraded every other, perfectly valid path to the
/// filesystem fallback -- under a label ("no git-tracked files found")
/// that was false, since git had found some. Splitting on NUL first and
/// decoding each piece keeps every valid entry in the returned list
/// (`-z`'s own trailing NUL leaves one empty final piece, filtered like
/// before) and collects every undecodable one, as its own raw bytes
/// rendered lossily for display, into the second list instead of
/// discarding it.
fn git_ls_files(root: &Path) -> Option<(Vec<String>, Vec<String>)> {
    let output = std::process::Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for entry in output.stdout.split(|&byte| byte == 0) {
        if entry.is_empty() {
            continue;
        }
        match std::str::from_utf8(entry) {
            Ok(path) => files.push(path.to_string()),
            Err(_) => skipped.push(String::from_utf8_lossy(entry).into_owned()),
        }
    }
    Some((files, skipped))
}

/// The fallback tree: every regular file under `root`, as forward-slash-
/// relative paths, recursed depth-first and skipping a top-level `.git`
/// directory (VCS internals, never a project file an area glob could
/// legitimately target). A directory `fs::read_dir` cannot open, or a
/// directory entry it cannot read, is a named, fatal error here (fix
/// round 1, important issue 2; `quality.gates-derive-their-scope`): the
/// prior cut's silent `return` on the same failure under-counted the
/// tree instead of failing loudly, which is the same shape
/// `bin/residue-gate.rs`'s own module doc ("Unreadable and binary paths")
/// already ruled against for the sibling gate.
fn filesystem_tree_files(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    walk_tree(root, root, &mut out)?;
    Ok(out)
}

/// `filesystem_tree_files`'s recursive step: appends every file under
/// `dir` (relative to `root`) to `out`, skipping a `.git` subdirectory.
fn walk_tree(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("{}: {error}", dir.display()))?;
        let path = entry.path();
        if path.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            walk_tree(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(relative);
        }
    }
    Ok(())
}

/// The dead-glob gate (HR-074): every glob in a non-empty `paths` list
/// that matches zero files in `tree`, as `knowledge/areas.json.<area>.
/// paths: "<glob>" matches no tracked file (<source>)` findings, one per
/// dead glob, `source` naming which of `tracked_tree_files`'s two
/// derivations decided liveness (fix round 1, important issue 2) --
/// distinct from an empty `paths` list (`global`/`process`'s own shape),
/// which this never reports, because iterating zero globs finds zero dead
/// ones by construction (no special case needed or wanted: the empty-list
/// legality this task's brief requires falls out of checking per glob,
/// not per area). `glob` is compiled once, outside the per-path scan (fix
/// round 1, minor issue 7: recompiling per candidate path measured
/// 0.01s -> 0.33s on this repository's own tree, 170,100 compiles for
/// 9450 files x 18 globs); `glob::compile`, not `glob::glob_match`, is
/// this crate's one matching engine now used here (that module's own
/// doc). A glob's compile failure is unreachable here: `model::build_areas`
/// already validates every area's globs at load time, so `base.areas`
/// never carries one `check_base` could reach.
fn dead_glob_findings(
    areas: &[(String, super::model::AreaDef)],
    tree: &[String],
    source: TreeSource,
) -> Vec<String> {
    let mut findings = Vec::new();
    for (area, def) in areas {
        for glob in &def.paths {
            let matcher =
                compile(glob).expect("area globs are validated at load time (model::build_areas)");
            let alive = tree.iter().any(|path| matcher.is_match(path));
            if !alive {
                findings.push(format!(
                    "knowledge/areas.json.{area}.paths: {glob:?} matches no tracked file ({})",
                    source.label()
                ));
            }
        }
    }
    findings
}

/// Validates every file directly under `knowledge/archive/` against the
/// knowledge schema (`base.schema`, the same root every active topic file
/// validates against) and collects every archived entry's id, so a `see`
/// citation to one still resolves (`knowledge-base.ids-are-permanent`,
/// below).
///
/// An absent `knowledge/archive/` directory is not itself a finding --
/// most repositories have archived nothing yet. Any other reason the
/// directory or one of its files cannot be read or parsed is a named
/// error: corruption is loud (`crate::archive::list_archive_json_files`
/// and `crate::archive::read_json_as`, this function's own two calls into
/// the archive module, carry that policy). A finding names its file by
/// the repository-relative path this function already computes, on every
/// platform: `read_json_as` takes that name directly rather than
/// deriving one from the path it reads.
///
/// This is a SCHEMA check plus id collection only: an archived entry's
/// `verify` path or area-glob liveness is never judged here, only whether
/// the file's shape still parses as a valid knowledge topic file.
fn check_archive(base: &Base, errors: &mut Vec<String>) -> HashSet<String> {
    let mut ids = HashSet::new();
    let dir = base.root.join("knowledge/archive");
    let names = match crate::archive::list_archive_json_files(&dir) {
        Ok(Some(names)) => names,
        Ok(None) => return ids,
        Err(message) => {
            errors.push(message);
            return ids;
        }
    };
    for name in names {
        let relative = format!("knowledge/archive/{name}");
        match crate::archive::read_json_as(&dir.join(&name), &relative) {
            Ok(content) => {
                validate(&content, &base.schema, &relative, errors, &base.schema);
                for entry in content
                    .get("entries")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let Some(id) = entry.get("id").and_then(Value::as_str) {
                        ids.insert(id.to_string());
                    }
                }
            }
            Err(message) => errors.push(message),
        }
    }
    ids
}

/// Validates a loaded base against the schema and every cross-entry and
/// generated-file invariant -- `tools/kb.mjs`'s `checkBase`, two stages in
/// the same order: schema/id/area/standing/see/verify/check-shape errors
/// accumulate first, and if any fired, `check_base` returns immediately
/// (rendering needs a valid base, the same reason the JS bails at
/// `if (errors.any) return errors.list`); only a clean first stage reaches
/// the dead-glob, stale/stray, and budget checks below, which need the
/// generated files (and, for the dead-glob gate, a valid `areas` list) to
/// exist meaningfully.
pub(crate) fn check_base(base: &Base) -> Vec<String> {
    let mut archive_errors = Vec::new();
    let archived_ids = check_archive(base, &mut archive_errors);

    let mut errors = Vec::new();
    let areas_schema = base
        .schema
        .get("$defs")
        .and_then(|d| d.get("areas"))
        .cloned()
        .unwrap_or(Value::Null);
    validate(
        &base.areas_raw,
        &areas_schema,
        "knowledge/areas.json",
        &mut errors,
        &base.schema,
    );

    let area_names: Vec<&str> = base
        .schema
        .pointer("/$defs/area/enum")
        .and_then(Value::as_array)
        .map(|values| values.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let areas_obj = base.areas_raw.as_object().cloned().unwrap_or_default();
    for area in &area_names {
        if !areas_obj.contains_key(*area) {
            errors.push(format!("knowledge/areas.json: area \"{area}\" is missing"));
        }
    }
    for area in object_keys(&base.areas_raw) {
        if !area_names.contains(&area.as_str()) {
            errors.push(format!("knowledge/areas.json: unknown area \"{area}\""));
        }
    }

    let mut seen: HashMap<&str, &str> = HashMap::new();
    for (file, name, content) in &base.topic_files {
        validate(content, &base.schema, file, &mut errors, &base.schema);
        let topic_field = content.get("topic").and_then(Value::as_str).unwrap_or("");
        if topic_field != name {
            errors.push(format!(
                "{file}: topic \"{topic_field}\" must equal the file name \"{name}\""
            ));
        }
        let entries = content
            .get("entries")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for item in entries {
            let Some(id) = item.get("id").and_then(Value::as_str) else {
                continue;
            };
            let at = format!("{file} {id}");
            if !id.starts_with(&format!("{name}.")) {
                errors.push(format!("{at}: id must start with \"{name}.\""));
            }
            if let Some(&prior_file) = seen.get(id) {
                errors.push(format!("{at}: duplicate id (also in {prior_file})"));
            }
            seen.insert(id, file);
            // JS truthiness (`if (item.standing && ...)`), not the schema
            // type: a truthy non-boolean `standing` (a non-empty string,
            // say) still trips this check, alongside the schema stage's
            // own "must be boolean" finding for the same field (fix
            // round 1, finding 3).
            if !falsy(item.get("standing")) {
                let kind = item.get("kind").and_then(Value::as_str).unwrap_or("");
                let area = item.get("area").and_then(Value::as_str).unwrap_or("");
                if !(RULE_KINDS.contains(&kind) && ["global", "process"].contains(&area)) {
                    errors.push(format!(
                        "{at}: standing needs kind rule or invariant and area global or process"
                    ));
                }
            }
            // Every `see` entry is checked, coerced to its JS `ToString`
            // form when it is not already a string (fix round 1, finding
            // 3): `base.entries` is keyed by real string ids, so a
            // non-string entry never matches one and always reports,
            // alongside the schema stage's own "must be string" finding.
            for see_id in item
                .get("see")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let see_id = to_js_string(see_id);
                if !base.entries.contains_key(see_id.as_str())
                    && !archived_ids.contains(see_id.as_str())
                {
                    errors.push(format!("{at}: see \"{see_id}\" does not exist"));
                }
            }
            // Every `verify` entry is checked the same way. The frozen JS
            // actually crashes uncaught for a non-string entry here
            // (`path.join` rejects a non-string argument, verified live,
            // node 24.18.1) -- a real bug in the source this ports, not a
            // contract worth reproducing bug-for-bug: `PathBuf::join`
            // accepts any string, so this reports a normal finding
            // instead (the coerced form almost never names a real file),
            // which is strictly more useful than the JS stack trace and
            // never silently drops the entry either way.
            for verify_path in item
                .get("verify")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let verify_path = to_js_string(verify_path);
                if !base.root.join(&verify_path).exists() {
                    errors.push(format!(
                        "{at}: verify path \"{verify_path}\" does not exist"
                    ));
                }
            }
            if let Some(check) = item.get("check").filter(|c| c.is_object()) {
                check_shape(check, &at, &mut errors);
            }
        }
    }
    if !errors.is_empty() {
        errors.extend(archive_errors);
        return errors; // rendering needs a valid base
    }

    match tracked_tree_files(&base.root) {
        Ok((tree, source, skipped)) => {
            // Named, non-fatal (fix round 3, new_breakage 1, task-1-
            // review-r3.json): an undecodable path never changes `source`
            // or drops out silently -- `dead_glob_findings` still runs
            // against every path that DID decode, using the derivation
            // `source` names truthfully.
            for entry in &skipped {
                errors.push(format!(
                    "knowledge/areas.json: a tracked path is not valid UTF-8, skipped from \
                     the dead-glob scan: {entry}"
                ));
            }
            errors.extend(dead_glob_findings(&base.areas, &tree, source));
        }
        Err(message) => errors.push(message),
    }

    let rendered = render_all(base);
    let rendered_paths: HashSet<&str> = rendered.iter().map(|(path, _)| path.as_str()).collect();
    for (path, content) in &rendered {
        let abs = base.root.join(path);
        let current = fs::read_to_string(&abs).ok();
        if current.as_deref() != Some(content.as_str()) {
            errors.push(format!(
                "{path}: generated file is out of date (run houserules render)"
            ));
        }
    }
    let rules_dir = base.root.join(".claude/rules");
    if rules_dir.is_dir() {
        // Node's `readdirSync` returns strcmp-sorted names (libuv sorts
        // scandir results), so this explicit sort reproduces
        // `tools/kb.mjs`'s own stray-file order rather than diverging
        // from it (task-4-review.json, fix round 1, finding 10: verified
        // live with five out-of-order `.md` files, both binaries printed
        // the same five lines in the same order).
        let mut names: Vec<String> = fs::read_dir(&rules_dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".md"))
            .collect();
        names.sort();
        for name in names {
            let relative = format!(".claude/rules/{name}");
            if !rendered_paths.contains(relative.as_str()) {
                errors.push(format!("{relative}: not generated by kb; remove it"));
            }
        }
    }
    check_budget(
        &base.root,
        "CLAUDE.md",
        BUDGETS.claude_md_lines,
        Some(BUDGETS.claude_md_bytes),
        &mut errors,
    );
    for (path, _) in &rendered {
        if path == ".claude/rules/standing-rules.md" {
            check_budget(&base.root, path, BUDGETS.standing_lines, None, &mut errors);
        } else if path == SKILL_PATH {
            check_budget(&base.root, path, BUDGETS.skill_lines, None, &mut errors);
        } else {
            check_budget(&base.root, path, BUDGETS.area_lines, None, &mut errors);
        }
    }
    errors.extend(archive_errors);
    errors
}

/// Runs the `check-knowledge` subcommand: loads the knowledge base at
/// `root` (resolving the enclosing git repository's top level when `root`
/// is `None`, exactly like `cmd_render`), then runs `check_base` against
/// it. A load failure (missing knowledge dir, missing `schema.json`, a
/// malformed area glob) prints one named line and exits 2 -- distinct from
/// a clean load whose check findings print as `houserules check-knowledge`'s own
/// stderr lines and exit 1 (spec §6's CLI-failure-path deviation; docs/specs/
/// 2026-09-04-batch-15-tier2-spec.md).
pub(crate) fn cmd_check_knowledge(root: Option<PathBuf>) -> ExitCode {
    let root = match crate::root::resolve_root(root) {
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
    let errors = check_base(&base);
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("{error}");
        }
        return ExitCode::from(1);
    }
    println!("knowledge: ok");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::process::Command;

    use serde_json::json;

    use super::*;

    /// Runs `git` at `root`, asserting success -- this module's own copy
    /// of the small per-module helper every git-backed test module in
    /// this crate keeps (`audit.rs`, `check_commit.rs`,
    /// `report_claims.rs`'s own `fn git`; `gen-goldens.rs`'s module doc
    /// explains why each keeps its own rather than sharing one).
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

    /// Registers `raw_path` (arbitrary bytes, not required to be valid
    /// UTF-8) as a tracked file in `root`'s git index, through git's
    /// plumbing layer -- never `fs::write` on a non-UTF-8 name, which
    /// panics on macOS: PR #14's macos-latest CI job (run 34605787940,
    /// job 103283751346) failed at exactly that `unwrap()` with `Os error
    /// 92, Illegal byte sequence`, because APFS rejects an invalid-UTF-8
    /// byte sequence at file CREATION. `#[cfg(unix)]` alone only gates the
    /// COMPILER capability this test needs (`OsStrExt`); the FILESYSTEM
    /// capability a real write also needs is Linux-only, a second,
    /// distinct layer (`houserules.platform-gated-tests`'s own body now
    /// carries this lesson). Git's object database and index are
    /// byte-oriented and never touch a real path on disk for this, so
    /// APFS never sees the name: `git hash-object -w --stdin` writes
    /// `content` as a blob and returns its id, and `git update-index
    /// --add --cacheinfo <mode> <id> <path>` (the three-separate-
    /// arguments form -- git's own docs name it "for backward
    /// compatibility" beside the single comma-joined form, kept here for
    /// the opposite reason: the comma form cannot carry a path with
    /// invalid UTF-8 bytes, since a Rust `&str` cannot hold one either)
    /// registers `raw_path` at that blob without writing it anywhere.
    /// `git ls-files -z` then reports it identically to a real file.
    ///
    /// `#[cfg(unix)]`: its only two callers are themselves `#[cfg(unix)]`
    /// (`raw_path`'s own construction needs `OsStrExt`), so on every other
    /// target this function is unused -- ungated, it would fail
    /// `cargo clippy --all-targets -- -D warnings` on windows-latest CI
    /// (`.github/workflows/ci.yml`'s rust job) with a `dead_code` warning
    /// turned error, the same class of failure this whole branch-fix round
    /// exists to close.
    #[cfg(unix)]
    fn seed_undecodable_tracked_path(root: &Path, raw_path: &std::ffi::OsStr, content: &str) {
        let hash_output = Command::new("git")
            .args(["hash-object", "-w", "--stdin"])
            .current_dir(root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                child
                    .stdin
                    .take()
                    .expect("piped stdin")
                    .write_all(content.as_bytes())?;
                child.wait_with_output()
            })
            .expect("run git hash-object -w --stdin");
        assert!(
            hash_output.status.success(),
            "git hash-object failed: {}",
            String::from_utf8_lossy(&hash_output.stderr)
        );
        let blob = String::from_utf8(hash_output.stdout)
            .expect("git hash-object prints a hex object id")
            .trim()
            .to_string();

        let status = Command::new("git")
            .args(["update-index", "--add", "--cacheinfo", "100644", &blob])
            .arg(raw_path)
            .current_dir(root)
            .status()
            .expect("run git update-index --cacheinfo");
        assert!(status.success(), "git update-index --cacheinfo failed");
    }

    /// Shallow-merges `overrides`' fields onto `base` -- the Rust
    /// equivalent of `tests/kb.test.mjs`'s `{...entry(), ...over}` object
    /// spread; both sides are always JSON objects in this module's usage.
    fn merge(base: &mut Value, overrides: Value) {
        if let (Value::Object(base_map), Value::Object(over_map)) = (base, overrides) {
            for (key, value) in over_map {
                base_map.insert(key, value);
            }
        }
    }

    /// A standing `process.sequential` rule entry, with every field a
    /// caller might override -- Rust port of `tests/kb.test.mjs`'s
    /// `entry()` helper.
    fn entry(overrides: Value) -> Value {
        let mut base = json!({
            "id": "process.sequential",
            "kind": "rule",
            "area": "process",
            "standing": true,
            "summary": "Run agents sequentially.",
            "body": ["One at a time."],
            "tags": ["dispatch"],
            "source": {"date": "2026-08-29", "by": "user"},
        });
        merge(&mut base, overrides);
        base
    }

    /// The seed knowledge schema (`template/knowledge/schema.json`) with
    /// its area enum project-extended -- Rust port of `tests/kb.test.mjs`'s
    /// module-level `SCHEMA` fixture, which the seed area enum is a
    /// starter for; these tests run on a project-extended enum, proving the
    /// extension path works.
    fn seed_schema() -> Value {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template/knowledge/schema.json");
        let mut schema: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        schema["$defs"]["area"]["enum"] = json!([
            "global", "process", "rust", "webview", "api", "schemas", "infra", "docs"
        ]);
        schema
    }

    /// A minimal areas map covering every glob shape `tests/kb.test.mjs`'s
    /// module-level `AREAS` fixture routes paths through.
    fn areas_json() -> Value {
        json!({
            "global": {"paths": []},
            "process": {"paths": []},
            "rust": {"paths": ["crates/**", "Cargo.toml"]},
            "webview": {"paths": ["apps/desktop/src/**"]},
            "api": {"paths": ["apps/api/**"]},
            "schemas": {"paths": ["packages/schemas/**"]},
            "infra": {"paths": ["tools/**", ".github/**"]},
            "docs": {"paths": ["docs/**", "CLAUDE.md"]},
        })
    }

    /// One representative file per glob `areas_json()` declares (HR-074):
    /// once the dead-glob gate ships, an area's non-empty `paths` list
    /// needs every glob it names to match at least one file in the tree,
    /// so every fixture whose `areas.json` declares a glob needs a file
    /// that glob actually matches -- these are that file, one per glob,
    /// content unused. `docs`'s `CLAUDE.md` glob is covered separately
    /// (`make_repo` always writes that file itself).
    fn write_area_marker_files(root: &Path) {
        for relative in [
            "crates/marker.rs",
            "Cargo.toml",
            "apps/desktop/src/marker.ts",
            "apps/api/marker.ts",
            "packages/schemas/marker.json",
            "tools/marker.sh",
            ".github/marker.yml",
            "docs/marker.md",
        ] {
            let path = root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "marker\n").unwrap();
        }
    }

    /// Groups `entries` by their id prefix and writes each group as its own
    /// topic file -- Rust port of `tests/kb.test.mjs`'s `writeTopics`.
    fn write_topics(root: &Path, entries: &[Value]) {
        let mut by_topic: std::collections::BTreeMap<String, Vec<Value>> =
            std::collections::BTreeMap::new();
        for e in entries {
            let id = e["id"].as_str().expect("entry id is a string");
            let topic = id.split('.').next().expect("entry id has a topic prefix");
            by_topic
                .entry(topic.to_string())
                .or_default()
                .push(e.clone());
        }
        for (topic, topic_entries) in by_topic {
            let content = json!({
                "$schema": "./schema.json",
                "topic": topic,
                "title": format!("{topic} title"),
                "entries": topic_entries,
            });
            fs::write(
                root.join(format!("knowledge/{topic}.json")),
                serde_json::to_string(&content).unwrap(),
            )
            .unwrap();
        }
    }

    /// A knowledge base under `root`: the project-extended seed schema,
    /// `AREAS`, `entries` split into topic files, and a starter `CLAUDE.md`
    /// -- Rust port of `tests/kb.test.mjs`'s `makeRepo` (its `files`
    /// parameter is a caller writing to `root` directly afterward instead,
    /// since every override in this module's ported cases is a single
    /// extra `fs::write`).
    fn make_repo(root: &Path, entries: &[Value]) {
        fs::create_dir_all(root.join("knowledge")).unwrap();
        fs::write(
            root.join("knowledge/schema.json"),
            serde_json::to_string(&seed_schema()).unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas_json()).unwrap(),
        )
        .unwrap();
        write_topics(root, entries);
        fs::write(root.join("CLAUDE.md"), "# Test\n").unwrap();
        write_area_marker_files(root);
    }

    /// This checkout's `template/` directory, resolved at compile time from
    /// the crate's manifest directory so it is correct regardless of the
    /// test runner's working directory.
    fn template_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../template")
    }

    /// A knowledge base under `root` seeded with the real
    /// `template/knowledge` content, starter `CLAUDE.md`, and the other
    /// files its entries' `verify` paths name -- Rust port of
    /// `tests/kb.test.mjs`'s `makeSeedRepo` (its git init/commit are
    /// dropped: `--dir` bypasses git resolution entirely, and `check_base`
    /// itself never calls git).
    fn make_seed_repo(root: &Path) {
        let template = template_root();
        fs::create_dir_all(root.join("knowledge")).unwrap();
        let mut names: Vec<String> = fs::read_dir(template.join("knowledge"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".json"))
            .collect();
        names.sort();
        for name in names {
            fs::copy(
                template.join("knowledge").join(&name),
                root.join("knowledge").join(&name),
            )
            .unwrap();
        }
        fs::write(
            root.join("CLAUDE.md"),
            fs::read_to_string(template.join("CLAUDE.md")).unwrap(),
        )
        .unwrap();
        // Fix round 1, important issue 4 (task-1-review.json): every path
        // here is a real file `houserules init` itself writes -- nothing
        // stands in for one. The seed's `docs`/`tools` areas
        // (`template/knowledge/areas.json`) declare `docs/**`,
        // `tools/**`, and `.github/**`; `docs/README.md` (HR-087),
        // `tools/claude-session-start.sh`, and
        // `.github/workflows/knowledge.yml` are what a real `init` puts
        // under each, so copying them is what keeps this seed a fixture
        // that pins the shipped product rather than a fixture padded to
        // pass around it.
        for path in [
            ".claude/schemas/deliverables.json",
            ".claude/evals/record.json",
            "backlog/schema.json",
            ".claude/skills/finishing-a-feature/SKILL.md",
            ".claude/skills/orchestrating/SKILL.md",
            "tools/claude-session-start.sh",
            ".github/workflows/knowledge.yml",
            "docs/README.md",
        ] {
            let dest = root.join(path);
            fs::create_dir_all(dest.parent().unwrap()).unwrap();
            fs::copy(template.join(path), &dest).unwrap();
        }
    }

    /// tests/kb.test.mjs, describe('checkBase'): "passes a valid base".
    #[test]
    fn passes_a_valid_base() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(check_base(&base), Vec::<String>::new());
    }

    /// Fix round 1, finding 1 (task-4-review.json): `paths` as a string
    /// instead of an array must reach `check_base` as a schema finding,
    /// not fail the load. Verified live against the frozen JS on a copy
    /// of the `mini` corpus fixture: `knowledge/areas.json.tools.paths:
    /// must be array`, exit 1 (this test's own schema names the area
    /// `process`, this module's fixtures' own area set).
    #[test]
    fn check_base_reports_paths_as_a_string_instead_of_failing_to_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        let mut areas = areas_json();
        areas["process"] = json!({"paths": "not-an-array"});
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas).unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert_eq!(
            errors,
            vec!["knowledge/areas.json.process.paths: must be array".to_string()]
        );
    }

    /// Fix round 1, finding 1: an area def that is not an object at all.
    /// Verified live: `knowledge/areas.json.tools: must be object`, exit 1.
    #[test]
    fn check_base_reports_an_area_def_that_is_not_an_object_instead_of_failing_to_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        let mut areas = areas_json();
        areas["process"] = json!(5);
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas).unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert_eq!(
            errors,
            vec!["knowledge/areas.json.process: must be object".to_string()]
        );
    }

    /// Fix round 1, finding 1: `paths` holding non-string entries.
    /// Verified live: two `must be string` findings, exit 1.
    #[test]
    fn check_base_reports_non_string_paths_entries_instead_of_failing_to_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        let mut areas = areas_json();
        areas["process"] = json!({"paths": [1, 2]});
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas).unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert_eq!(
            errors,
            vec![
                "knowledge/areas.json.process.paths[0]: must be string".to_string(),
                "knowledge/areas.json.process.paths[1]: must be string".to_string(),
            ]
        );
    }

    /// Fix round 1, finding 1: `areas.json` as a JSON array instead of an
    /// object. Verified live against the frozen JS: one "must be object"
    /// finding, then "area ... is missing" for every schema-enum area
    /// (`in` on an array is always false), then "unknown area" for the
    /// array's own index keys (`Object.keys(array)` gives index strings) --
    /// ten findings total on the real areas.json's seven-area enum; this
    /// module's own eight-area schema and two-element array give nine.
    #[test]
    fn check_base_reports_areas_json_as_an_array_instead_of_failing_to_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&json!(["a", "b"])).unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert_eq!(
            errors,
            vec![
                "knowledge/areas.json: must be object".to_string(),
                "knowledge/areas.json: area \"global\" is missing".to_string(),
                "knowledge/areas.json: area \"process\" is missing".to_string(),
                "knowledge/areas.json: area \"rust\" is missing".to_string(),
                "knowledge/areas.json: area \"webview\" is missing".to_string(),
                "knowledge/areas.json: area \"api\" is missing".to_string(),
                "knowledge/areas.json: area \"schemas\" is missing".to_string(),
                "knowledge/areas.json: area \"infra\" is missing".to_string(),
                "knowledge/areas.json: area \"docs\" is missing".to_string(),
                "knowledge/areas.json: unknown area \"0\"".to_string(),
                "knowledge/areas.json: unknown area \"1\"".to_string(),
            ]
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "reports schema, id, area,
    /// standing, see, verify, and check-shape errors".
    #[test]
    fn reports_schema_id_area_standing_see_verify_and_check_shape_errors() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let entries = [
            entry(json!({"id": "process.dup", "see": ["nope.x"], "verify": ["missing.txt"]})),
            entry(json!({"id": "process.dup", "summary": "x".repeat(161)})),
            entry(json!({"id": "process.bad-standing", "kind": "gotcha"})),
            entry(json!({
                "id": "process.bad-check",
                "standing": false,
                "check": {"type": "grep-absent", "level": "fail", "pattern": "("},
            })),
            entry(json!({
                "id": "process.bad-commits",
                "standing": false,
                "check": {"type": "commits", "level": "warn"},
            })),
        ];
        make_repo(root, &entries);

        // writeTopics files an entry under its own (matching) topic file,
        // so `other.x` would never violate the "wrong topic" check below;
        // splice it into process.json directly instead.
        let process_path = root.join("knowledge/process.json");
        let mut topic: Value =
            serde_json::from_str(&fs::read_to_string(&process_path).unwrap()).unwrap();
        topic["entries"]
            .as_array_mut()
            .unwrap()
            .insert(0, entry(json!({"id": "other.x"})));
        fs::write(&process_path, serde_json::to_string(&topic).unwrap()).unwrap();

        // Simulate a missing area, an unknown one, and a malformed one.
        let mut areas = areas_json();
        let areas_map = areas.as_object_mut().unwrap();
        areas_map.remove("docs");
        areas_map.insert("extra".to_string(), json!({"paths": []}));
        areas_map.insert("rust".to_string(), json!({"nope": 1}));
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas).unwrap(),
        )
        .unwrap();

        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        for expected in [
            "knowledge/areas.json: area \"docs\" is missing",
            "knowledge/areas.json: unknown area \"extra\"",
            "knowledge/areas.json.rust: missing \"paths\"",
            "knowledge/areas.json.rust: unknown field \"nope\"",
            "knowledge/process.json other.x: id must start with \"process.\"",
            "knowledge/process.json process.dup: duplicate id (also in knowledge/process.json)",
            "knowledge/process.json.entries[2].summary: longer than 160 characters",
            "knowledge/process.json process.bad-standing: standing needs kind rule or invariant and area global or process",
            "knowledge/process.json process.dup: see \"nope.x\" does not exist",
            "knowledge/process.json process.dup: verify path \"missing.txt\" does not exist",
            "knowledge/process.json process.bad-check: check \"grep-absent\" needs \"files\"",
            "knowledge/process.json process.bad-check: check \"grep-absent\" needs \"scope\"",
            "knowledge/process.json process.bad-commits: check \"commits\" needs \"subject\", \"body_absent\", or \"body_line_max\"",
        ] {
            assert!(
                errors.contains(&expected.to_string()),
                "missing {expected:?} in {errors:#?}"
            );
        }
        assert!(
            errors
                .iter()
                .any(|e| e.contains("process.bad-check: check pattern is not a valid regex")),
            "{errors:#?}"
        );
    }

    /// Fix round 1, finding 3 (task-4-review.json): `standing: "yes"` is
    /// JS-truthy but schema-invalid, so the frozen JS reports both the
    /// schema's "must be boolean" finding AND the standing-needs-kind
    /// finding for the same entry -- a naive `as_bool` read (`None` for
    /// a string) missed the second line entirely. Verified live, node
    /// 24.18.1, against the mini fixture with `mini.build-cache`
    /// (kind `rule`, area `tools`) given `standing: "yes"`.
    #[test]
    fn check_base_ports_standing_through_js_truthiness_not_schema_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(
            root,
            &[entry(
                json!({"id": "process.a", "standing": "yes", "kind": "gotcha"}),
            )],
        );
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(
            errors.contains(
                &"knowledge/process.json.entries[0].standing: must be boolean".to_string()
            )
        );
        assert!(errors.contains(
            &"knowledge/process.json process.a: standing needs kind rule or invariant and area global or process"
                .to_string()
        ));
    }

    /// Fix round 1, finding 3: `see: [123]` is JS-truthy-coerced to the
    /// string `"123"` for the existence check, so the frozen JS reports
    /// both the schema's "must be string" finding AND the "does not
    /// exist" finding -- an `as_str` filter silently dropped the second
    /// line for any non-string entry. Verified live, node 24.18.1.
    #[test]
    fn check_base_ports_see_through_js_tostring_not_schema_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(
            root,
            &[entry(
                json!({"id": "process.a", "standing": false, "see": [123]}),
            )],
        );
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(
            errors
                .contains(&"knowledge/process.json.entries[0].see[0]: must be string".to_string())
        );
        assert!(
            errors.contains(
                &"knowledge/process.json process.a: see \"123\" does not exist".to_string()
            )
        );
    }

    /// Fix round 1, finding 3's swept sibling: `check.flags` read with
    /// `?? ''` on the JS side keeps a non-nullish, non-string value
    /// as-is (RegExp's own `ToString` on it), rather than defaulting to
    /// no flags the way an `as_str` read with `unwrap_or("")` would.
    /// `flags: 5` coerces to `"5"`, an ECMAScript-illegal flag character.
    #[test]
    fn check_shape_coerces_a_non_string_flags_value() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(
            root,
            &[entry(json!({
                "id": "process.a", "standing": false,
                "check": {"type": "grep-absent", "level": "fail", "files": "**", "pattern": "x", "scope": "changed", "flags": 5},
            }))],
        );
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(
            errors.iter().any(|e| e.contains(
                "process.a: check pattern is not a valid regex (Invalid flags supplied to RegExp constructor '5')"
            )),
            "{errors:#?}"
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "reports a topic whose
    /// name differs from its file name".
    #[test]
    fn reports_a_topic_whose_name_differs_from_its_file_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::write(
            root.join("knowledge/process.json"),
            serde_json::to_string(&json!({
                "$schema": "./schema.json", "topic": "other", "title": "t", "entries": [],
            }))
            .unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        assert!(
            check_base(&base).contains(
                &"knowledge/process.json: topic \"other\" must equal the file name \"process\""
                    .to_string()
            )
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "accepts a commits check
    /// that has only body_line_max".
    #[test]
    fn accepts_a_commits_check_that_has_only_body_line_max() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(
            root,
            &[entry(
                json!({"check": {"type": "commits", "level": "warn", "body_line_max": 80}}),
            )],
        );
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(check_base(&base), Vec::<String>::new());
    }

    /// tests/kb.test.mjs, describe('checkBase'): "rejects a commits check
    /// with body_line_max below 1".
    #[test]
    fn rejects_a_commits_check_with_body_line_max_below_1() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(
            root,
            &[entry(
                json!({"check": {"type": "commits", "level": "warn", "body_line_max": 0}}),
            )],
        );
        let base = load_base(root).expect("loads");
        assert!(
            check_base(&base)
                .iter()
                .any(|e| e.ends_with("check.body_line_max: below 1"))
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "skips the unreadable
    /// entries the schema already reported" -- also proves `model.rs`'s
    /// lenient topic loading (this fixture's `entries` array would have
    /// failed `load_base` outright under strict per-item deserialization,
    /// before this task; see `model.rs`'s module doc).
    #[test]
    fn skips_the_unreadable_entries_the_schema_already_reported() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::write(
            root.join("knowledge/process.json"),
            serde_json::to_string(&json!({
                "$schema": "./schema.json", "topic": "process", "title": "t",
                "entries": [{"kind": "rule"}, null],
            }))
            .unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(errors.contains(&"knowledge/process.json.entries[0]: missing \"id\"".to_string()));
        assert!(errors.contains(&"knowledge/process.json.entries[1]: must be object".to_string()));
    }

    /// tests/kb.test.mjs, describe('checkBase'): "does not crash on a topic
    /// with no entries array" (the `topicLines` half of that case has no
    /// Rust-owned surface to port to: render's corpus parity already
    /// exercises entry counting for well-formed topics).
    #[test]
    fn does_not_crash_on_a_topic_with_no_entries_array() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::write(
            root.join("knowledge/rust.json"),
            serde_json::to_string(
                &json!({"$schema": "./schema.json", "topic": "rust", "title": "t"}),
            )
            .unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        assert!(
            check_base(&base).contains(&"knowledge/rust.json: missing \"entries\"".to_string())
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "accepts a see reference
    /// to an existing entry and a verify path that exists".
    #[test]
    fn accepts_a_see_reference_to_an_existing_entry_and_a_verify_path_that_exists() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(
            root,
            &[
                entry(json!({"id": "process.a"})),
                entry(
                    json!({"id": "process.b", "standing": false, "see": ["process.a"], "verify": ["CLAUDE.md"]}),
                ),
            ],
        );
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(check_base(&base), Vec::<String>::new());
    }

    /// tests/kb.test.mjs, describe('checkBase'): "ignores a check whose
    /// type the schema already rejected, without crashing".
    #[test]
    fn ignores_a_check_whose_type_the_schema_already_rejected_without_crashing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(
            root,
            &[entry(json!({
                "id": "process.a", "standing": false,
                "check": {"type": "unknown-type", "level": "fail"},
            }))],
        );
        let base = load_base(root).expect("loads");
        assert!(
            !check_base(&base)
                .iter()
                .any(|e| e.contains("check \"unknown-type\""))
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "flags a missing
    /// CLAUDE.md".
    #[test]
    fn flags_a_missing_claude_md() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::remove_file(root.join("CLAUDE.md")).unwrap();
        let base = load_base(root).expect("loads");
        assert!(check_base(&base).contains(&"CLAUDE.md: missing".to_string()));
    }

    /// tests/kb.test.mjs, describe('checkBase'): "accepts a CLAUDE.md with
    /// no trailing newline".
    #[test]
    fn accepts_a_claude_md_with_no_trailing_newline() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::write(root.join("CLAUDE.md"), "# Test").unwrap();
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(check_base(&base), Vec::<String>::new());
    }

    /// tests/kb.test.mjs, describe('checkBase'): "flags CLAUDE.md over the
    /// line budget".
    #[test]
    fn flags_claude_md_over_the_line_budget() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::write(root.join("CLAUDE.md"), "x\n".repeat(201)).unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with("CLAUDE.md: ") && e.ends_with(" lines, budget 200"))
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "flags CLAUDE.md over the
    /// byte budget".
    #[test]
    fn flags_claude_md_over_the_byte_budget() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::write(root.join("CLAUDE.md"), "x".repeat(12289)).unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with("CLAUDE.md: ") && e.ends_with(" bytes, budget 12288"))
        );
    }

    /// tests/kb.test.mjs, describe('checkBase'): "flags a stray file in
    /// .claude/rules and ignores non-markdown files there".
    #[test]
    fn flags_a_stray_file_and_ignores_non_markdown_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        fs::create_dir_all(root.join(".claude/rules")).unwrap();
        fs::write(root.join(".claude/rules/extra.md"), "# extra\n").unwrap();
        fs::write(root.join(".claude/rules/notes.txt"), "ignore me\n").unwrap();
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(
            errors.contains(&".claude/rules/extra.md: not generated by kb; remove it".to_string())
        );
        assert!(!errors.iter().any(|e| e.contains("notes.txt")));
    }

    /// HR-074: an area whose `paths` list declares a glob that matches
    /// zero files in the tracked tree fails, naming the area and the dead
    /// glob -- the incident this closes (batch 20 branch review
    /// retrospective, violated_rules[0]: cli's `bin/**` matched nothing
    /// from the T3 retirement until a fix round, silently orphaning
    /// `quality.absence-is-designed` and `houserules.live-run-recipe`,
    /// with no gate noticing). `rust`'s other two globs, and every other
    /// area's globs, still match a real file (`write_area_marker_files`),
    /// so this also proves the negative direction in the same assertion:
    /// a live glob next to a dead one reports only the dead one, and
    /// `global`/`process`'s own deliberately empty `paths` lists report
    /// nothing at all.
    ///
    /// This fixture's tempdir is never `git init`ed, so it exercises
    /// `tracked_tree_files`'s documented FALLBACK derivation (filesystem
    /// walk); `check_base_decides_glob_liveness_from_git_tracked_files_
    /// not_the_working_directory` below exercises the primary, git-backed
    /// one.
    #[test]
    fn check_base_reports_a_dead_glob_naming_the_area_and_glob() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        let mut areas = areas_json();
        areas["rust"] = json!({"paths": ["crates/**", "Cargo.toml", "bin/**"]});
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas).unwrap(),
        )
        .unwrap();
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(
            check_base(&base),
            vec![
                "knowledge/areas.json.rust.paths: \"bin/**\" matches no tracked file \
                 (filesystem walk, no git-tracked files found)"
                    .to_string()
            ]
        );
    }

    /// HR-074's own subtle branch, named explicitly: a deliberately empty
    /// `paths` list (`global`'s and `process`'s own shape, matching the
    /// real `knowledge/areas.json`) is a legal choice, not the "declares
    /// globs that match nothing" defect the gate targets -- it must never
    /// appear in a dead-glob finding. `passes_a_valid_base` already proves
    /// this fixture reports nothing at all; this test names the invariant
    /// on its own so a future change that starts checking areas as a
    /// whole (all-or-nothing) rather than per glob cannot silently regress
    /// it unnoticed.
    #[test]
    fn check_base_does_not_flag_a_deliberately_empty_paths_list() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_repo(root, &[entry(json!({}))]);
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        let errors = check_base(&base);
        assert!(
            !errors
                .iter()
                .any(|e| e.contains("areas.json.process") || e.contains("areas.json.global")),
            "a deliberately empty paths list must never be flagged as dead: {errors:#?}"
        );
    }

    /// Fix round 1, important issue 2 (task-1-review.json): glob liveness
    /// must be decided by git-tracked files where `root` is a git
    /// repository, not by whatever happens to sit on disk -- the
    /// reviewer's own reproduction, the exact residue-survival shape the
    /// dead-glob gate exists to close (a directory retired from tracking,
    /// HR-074's own incident) can recur silently otherwise. A gitignored-
    /// but-present file (`bin/artifact.bin`) and a plain untracked-but-
    /// present file (`tools/legacy/old.sh`), neither ever `git add`ed,
    /// must NOT keep their globs alive, even though `bin/**` and
    /// `tools/legacy/**` both match something on disk.
    #[test]
    fn check_base_decides_glob_liveness_from_git_tracked_files_not_the_working_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        make_repo(root, &[entry(json!({}))]);
        let mut areas = areas_json();
        areas["rust"] = json!({"paths": ["crates/**", "Cargo.toml", "bin/**"]});
        areas["webview"] = json!({"paths": ["apps/desktop/src/**", "tools/legacy/**"]});
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas).unwrap(),
        )
        .unwrap();
        // Track everything real the fixture has created so far -- only
        // the residue seeded below is meant to stay untracked.
        git(root, &["add", "-A"]);

        fs::write(root.join(".gitignore"), "bin/\n").unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::write(root.join("bin/artifact.bin"), "residue").unwrap();
        fs::create_dir_all(root.join("tools/legacy")).unwrap();
        fs::write(root.join("tools/legacy/old.sh"), "#!/bin/sh\n").unwrap();

        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        let mut errors = check_base(&base);
        errors.sort();
        assert_eq!(
            errors,
            vec![
                "knowledge/areas.json.rust.paths: \"bin/**\" matches no tracked file \
                 (git ls-files)"
                    .to_string(),
                "knowledge/areas.json.webview.paths: \"tools/legacy/**\" matches no tracked \
                 file (git ls-files)"
                    .to_string(),
            ]
        );
    }

    /// Found live-running this same fix round's own I2 remedy, not in the
    /// review itself: a git repository whose index is still empty --
    /// `git init` then `houserules init` then `houserules check-
    /// knowledge`, before the adopter's first `git add`, exactly the
    /// sequence `init` itself prints as "next:" -- must not report every
    /// glob dead. `tracked_tree_files`'s own doc has the full account.
    #[test]
    fn check_base_falls_back_to_the_filesystem_when_the_git_index_is_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        make_repo(root, &[entry(json!({}))]);
        // Deliberately no `git add`: the index stays empty, while every
        // marker file `make_repo` writes is genuinely present on disk.
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(check_base(&base), Vec::<String>::new());
    }

    /// Fix round 2, new_breakage 1 (task-1-review-r2.json): `git ls-files`
    /// without `-z` lets git's own `core.quotePath` octal-escape a
    /// non-ASCII path, so a tracked `docs/café.md` came back as the
    /// 12-character-escaped string `"docs/caf\303\251.md"` -- which no
    /// glob matches -- instead of the real path. A tracked non-ASCII file
    /// under a globbed area must not make that glob look dead.
    #[test]
    fn check_base_matches_a_glob_against_a_tracked_non_ascii_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        make_repo(root, &[entry(json!({}))]);
        // Remove the plain-ASCII docs marker `write_area_marker_files`
        // wrote: docs/** must be satisfied by the accented file alone, or
        // a naive line-split's mangling of it would hide behind this
        // other match instead of failing the test.
        fs::remove_file(root.join("docs/marker.md")).unwrap();
        fs::write(root.join("docs/café.md"), "# café\n").unwrap();
        git(root, &["add", "-A"]);

        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(check_base(&base), Vec::<String>::new());
    }

    /// Fix round 3, new_breakage 1 (task-1-review-r3.json): the round 2
    /// fix decoded the whole `git ls-files -z` buffer in one
    /// `String::from_utf8` call, so ONE tracked path with invalid UTF-8
    /// bytes (a latin-1 name, here) turned the WHOLE call into `None`,
    /// silently downgrading every other, perfectly valid path to the
    /// filesystem derivation under a label ("no git-tracked files found")
    /// that was false -- git had found plenty. A seeded dead glob
    /// (`infra`'s extra `"handbook/**"`) must still be reported, still
    /// labeled `(git ls-files)` -- proving the repo stayed on the git
    /// derivation -- and the undecodable path itself must appear as its
    /// own named, non-fatal finding, never silently dropped and never
    /// fatal to the scan of any other path.
    ///
    /// Unix-only (branch review, critical issue 1): `OsStrExt::from_bytes`
    /// is a Unix-only extension trait (`std::os::unix::ffi`), so this test
    /// does not compile on Windows at all -- `crates/houserules/tests/
    /// install.rs`'s own `init_marks_shell_scripts_and_the_git_hook_
    /// executable` is the crate's precedent for gating a whole test this
    /// way rather than only the one line that needs it.
    ///
    /// Seeded through the git index, not the filesystem (branch-fix round
    /// 2: PR #14's macos-latest job panicked at an `fs::write` unwrap on
    /// this exact name -- `seed_undecodable_tracked_path`'s own doc has
    /// the full account). This is safe here specifically because
    /// `check_base`'s dead-glob scan matches every tracked path against a
    /// glob by NAME alone and never opens one to read its content, so a
    /// path that exists only in the index, never on disk, exercises the
    /// identical code path a real file would.
    #[cfg(unix)]
    #[test]
    fn check_base_names_an_undecodable_tracked_path_and_stays_on_the_git_derivation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        make_repo(root, &[entry(json!({}))]);
        let mut areas = areas_json();
        areas["infra"] = json!({"paths": ["tools/**", ".github/**", "handbook/**"]});
        fs::write(
            root.join("knowledge/areas.json"),
            serde_json::to_string(&areas).unwrap(),
        )
        .unwrap();
        git(root, &["add", "-A"]);

        // `0xE9` alone is not a valid UTF-8 continuation byte, so this
        // name cannot decode as UTF-8 no matter how the `-z` output is
        // split. `OsStrExt::from_bytes` builds the raw path directly,
        // bypassing Rust's own UTF-8 requirement on `&str`/`String` --
        // the same way a real latin-1 filename reaches a git repository
        // from a non-Rust tool.
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"docs/caf\xe9.md");
        seed_undecodable_tracked_path(root, name, "# non-utf8\n");

        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        let mut errors = check_base(&base);
        errors.sort();
        assert_eq!(
            errors,
            vec![
                "knowledge/areas.json.infra.paths: \"handbook/**\" matches no tracked file \
                 (git ls-files)"
                    .to_string(),
                "knowledge/areas.json: a tracked path is not valid UTF-8, skipped from the \
                 dead-glob scan: docs/caf\u{fffd}.md"
                    .to_string(),
            ]
        ); // '.' (0x2E) sorts before ':' (0x3A): the `.infra.paths` finding leads.
    }

    /// Fix round 4, new_breakage (task-1-review-r4.json): `tracked_tree_
    /// files` returned the skipped list only on the `GitTracked` branch;
    /// the `Filesystem` fallback returned `Vec::new()` unconditionally,
    /// so when NO tracked path decodes -- `files` above comes back
    /// empty, `git_ls_files`'s own `skipped` list is discarded with it --
    /// the reader was told nothing at all: `knowledge: ok`, though git
    /// had returned a path it could not read. Reproduced with a
    /// repository whose ONLY tracked path is undecodable: it is `git
    /// add`ed before this fixture writes anything else, so it is the
    /// sole entry `git ls-files` names, and none of it decodes.
    ///
    /// Unix-only (branch review, critical issue 1): see the sibling test
    /// above for why `OsStrExt::from_bytes` gates the whole function.
    ///
    /// Seeded through the git index, not the filesystem (branch-fix round
    /// 2: `seed_undecodable_tracked_path`'s own doc has the PR #14
    /// macos-latest account). Safe here for the same reason as the
    /// sibling test above: `check_base`'s dead-glob scan never reads a
    /// tracked path's content, only its name, so an index-only entry with
    /// no file on disk at all still exercises the real code path.
    #[cfg(unix)]
    #[test]
    fn check_base_names_an_undecodable_tracked_path_even_when_none_of_them_decode() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);

        // `0xE9` alone is not a valid UTF-8 continuation byte.
        use std::os::unix::ffi::OsStrExt;
        let name = std::ffi::OsStr::from_bytes(b"caf\xe9.md");
        seed_undecodable_tracked_path(root, name, "# non-utf8\n");

        // Every real file this fixture writes next stays untracked: the
        // filesystem fallback below still finds them all on disk (this
        // is the zero-decode corner, so `tracked_tree_files` falls back
        // to it regardless), so no glob goes dead and the skip is the
        // only finding this test pins.
        make_repo(root, &[entry(json!({}))]);

        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(
            check_base(&base),
            vec![
                "knowledge/areas.json: a tracked path is not valid UTF-8, skipped from the \
                 dead-glob scan: caf\u{fffd}.md"
                    .to_string()
            ]
        );
    }

    /// tests/kb.test.mjs, describe('render'): "checkBase reports drift,
    /// stray rule files, and budget overruns" -- one of the two checkBase
    /// tests T3 left JS-owned since it hadn't ported `checkBase` yet; this
    /// task does, so it ports (and its JS case is deleted in the same
    /// commit, along with its sibling below).
    #[test]
    fn checkbase_reports_drift_stray_rule_files_and_budget_overruns() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let entries = [
            entry(json!({})),
            entry(json!({"id": "process.ask", "kind": "invariant", "summary": "Ask when unsure."})),
            entry(json!({
                "id": "rust.clean", "area": "rust", "standing": false, "kind": "gotcha",
                "summary": "Clean before retry.",
            })),
            entry(json!({
                "id": "rust.floor", "area": "rust", "standing": false, "summary": "Never lower a floor.",
            })),
            entry(json!({
                "id": "rust.old", "area": "rust", "standing": false, "kind": "history", "summary": "Old.",
            })),
        ];
        make_repo(root, &entries);
        let base = load_base(root).expect("loads");
        assert!(check_base(&base).contains(
            &".claude/rules/standing-rules.md: generated file is out of date (run houserules render)".to_string()
        ));
        crate::rules::render::render(&base, false).expect("render");
        fs::write(root.join(".claude/rules/stray.md"), "x").unwrap();
        fs::write(root.join("CLAUDE.md"), "x\n".repeat(201)).unwrap();
        assert_eq!(
            check_base(&base),
            vec![
                ".claude/rules/stray.md: not generated by kb; remove it".to_string(),
                "CLAUDE.md: 201 lines, budget 200".to_string(),
            ]
        );
        fs::write(root.join("CLAUDE.md"), format!("{}\n", "x".repeat(12300))).unwrap();
        assert!(check_base(&base).contains(&"CLAUDE.md: 12301 bytes, budget 12288".to_string()));
    }

    /// tests/kb.test.mjs, describe('render'): "checkBase reports a
    /// generated file over its line budget".
    #[test]
    fn checkbase_reports_a_generated_file_over_its_line_budget() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let many: Vec<Value> = (0..61)
            .map(|i| entry(json!({"id": format!("process.r{i:02}")})))
            .collect();
        make_repo(root, &many);
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        assert!(
            check_base(&base)
                .contains(&".claude/rules/standing-rules.md: 65 lines, budget 60".to_string())
        );
    }

    /// Fix round 1, finding 7 (task-4-review.json): `BUDGETS.skill_lines`
    /// (120) had no Rust-side pin at all -- the root corpus slice passes
    /// for any skill budget above the real file's length, so only a
    /// too-low value would ever be caught. This puts the knowledge
    /// skill's own standing section over 120 lines and asserts the
    /// skill-path budget message fires.
    #[test]
    fn checkbase_reports_the_knowledge_skill_over_its_line_budget() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        let many: Vec<Value> = (0..105)
            .map(|i| entry(json!({"id": format!("process.r{i}")})))
            .collect();
        make_repo(root, &many);
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let errors = check_base(&base);
        assert!(
            errors
                .iter()
                .any(|e| e.starts_with(&format!("{SKILL_PATH}: "))
                    && e.ends_with(" lines, budget 120")),
            "{errors:#?}"
        );
    }

    /// tests/kb.test.mjs, describe('the repository knowledge base'):
    /// "renders and passes checkBase against the real template/knowledge
    /// seed". A regression check over data that is already correct, not
    /// new behavior, so it has no natural RED; the next test proves it is
    /// not vacuous by breaking the same seed on purpose.
    #[test]
    fn renders_and_passes_check_base_against_the_real_template_knowledge_seed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_seed_repo(root);
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let base = load_base(root).expect("loads");
        assert_eq!(check_base(&base), Vec::<String>::new());
    }

    /// tests/kb.test.mjs, describe('the repository knowledge base'):
    /// "fails checkBase when the seeded process.json is deliberately
    /// broken" -- disclosed-mutation proof for the test above.
    #[test]
    fn fails_check_base_when_the_seeded_process_json_is_deliberately_broken() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        make_seed_repo(root);
        let base = load_base(root).expect("loads");
        crate::rules::render::render(&base, false).expect("render");
        let process_path = root.join("knowledge/process.json");
        let mut broken: Value =
            serde_json::from_str(&fs::read_to_string(&process_path).unwrap()).unwrap();
        for e in broken["entries"].as_array_mut().unwrap() {
            if e["id"].as_str() == Some("process.conventional-commits") {
                e["check"]["body_line_max"] = json!(0);
            }
        }
        fs::write(&process_path, serde_json::to_string(&broken).unwrap()).unwrap();
        let base = load_base(root).expect("loads");
        assert!(
            check_base(&base)
                .iter()
                .any(|e| e.ends_with("check.body_line_max: below 1"))
        );
    }

    /// `classify_structural_reason` names the four V8 reasons this port
    /// reproduces byte-exact, beyond the one corpus-pinned case (`(` ->
    /// "Unterminated group", exercised end to end by `check_parity.rs`'s
    /// `mini-bad` corpus test): the other three, and `None` for a
    /// well-formed pattern (this repository's own
    /// `process.conventional-commits` check subject).
    #[test]
    fn classify_structural_reason_names_the_four_recognised_v8_reasons() {
        assert_eq!(classify_structural_reason("("), Some("Unterminated group"));
        assert_eq!(classify_structural_reason(")"), Some("Unmatched ')'"));
        assert_eq!(
            classify_structural_reason("["),
            Some("Unterminated character class")
        );
        assert_eq!(
            classify_structural_reason("\\"),
            Some("\\ at end of pattern")
        );
        assert_eq!(
            classify_structural_reason(
                "^(?=.{1,100}$)(feat|fix|chore|test|ci|docs|refactor|perf|build|style|revert)(\\([^)]+\\))?!?: .+"
            ),
            None
        );
    }

    /// Fix round 1, finding 2 (task-4-review.json): the pre-fix scan's
    /// verdict itself diverged from V8's for these three patterns -- each
    /// compiled clean under the old scan (`knowledge: ok`, exit 0) where
    /// the frozen JS finds them invalid (exit 1). `regress` decides
    /// validity now, so the verdict matches; its own reason text (not
    /// V8's) is the recorded residual divergence (spec §6). Verified
    /// live, regress 0.12.0: "Invalid atom character", "Invalid
    /// quantifier", "Invalid token at named capture group identifier".
    #[test]
    fn regex_validity_message_flips_verdict_to_match_v8_for_the_reviewers_three_patterns() {
        assert_eq!(
            regex_validity_message("*abc", ""),
            Some("Invalid regular expression: /*abc/: Invalid atom character".to_string())
        );
        assert_eq!(
            regex_validity_message("a{2,1}", ""),
            Some("Invalid regular expression: /a{2,1}/: Invalid quantifier".to_string())
        );
        assert_eq!(
            regex_validity_message("(?<1x>a)", ""),
            Some(
                "Invalid regular expression: /(?<1x>a)/: Invalid token at named capture group identifier"
                    .to_string()
            )
        );
    }

    /// Fix round 1, finding 2: `check_shape` never validated `flags` at
    /// all (it only rendered them into the message); `"zz"` is invalid
    /// ECMAScript flags (verified live, node 24.18.1) and
    /// `regress::Regex::with_flags` does not reject it either (verified
    /// live, regress 0.12.0), so `validate_flags` is this port's own
    /// check. The message is V8's own, fully reproducible.
    #[test]
    fn regex_validity_message_reports_invalid_flags_v8_verbatim() {
        assert_eq!(
            regex_validity_message("valid", "zz"),
            Some("Invalid flags supplied to RegExp constructor 'zz'".to_string())
        );
    }

    /// `u` and `v` may not both appear (verified live, node 24.18.1:
    /// `new RegExp('a', 'uv')` throws the same flags message).
    #[test]
    fn validate_flags_rejects_u_and_v_together() {
        assert!(validate_flags("uv").is_err());
    }

    /// A well-formed pattern under well-formed flags is accepted --
    /// `regex_validity_message` returns `None`.
    #[test]
    fn regex_validity_message_accepts_a_well_formed_pattern_and_flags() {
        assert_eq!(regex_validity_message("^(feat|fix): .+", "i"), None);
    }

    /// Fix round 1, finding 5 (task-4-review.json): a schema `pattern`
    /// using lookahead compiled under the old plain `regex` crate not at
    /// all, so `validate` silently dropped the constraint -- a value
    /// that should have failed the pattern passed clean instead.
    /// `regress` supports lookahead (verified live, regress 0.12.0), so
    /// the constraint is enforced now: a non-matching value is reported,
    /// a matching one is not.
    #[test]
    fn validate_enforces_a_schema_pattern_using_lookahead() {
        let schema = json!({"type": "string", "pattern": "^(?=.{3,5}$).*$"});

        let mut errors = Vec::new();
        validate(&json!("ab"), &schema, "field", &mut errors, &schema);
        assert_eq!(
            errors,
            vec!["field: must match ^(?=.{3,5}$).*$".to_string()]
        );

        let mut errors = Vec::new();
        validate(&json!("abcd"), &schema, "field", &mut errors, &schema);
        assert_eq!(errors, Vec::<String>::new());
    }

    /// Fix round 1, finding 5: a schema `pattern` that fails to compile
    /// at all (an unbalanced paren, here) is a named finding, not a
    /// silent skip of the constraint -- the eager-glob precedent's
    /// principle applied to a malformed schema pattern.
    #[test]
    fn validate_reports_an_uncompilable_schema_pattern_as_a_named_finding() {
        let schema = json!({"type": "string", "pattern": "("});
        let mut errors = Vec::new();
        validate(&json!("anything"), &schema, "field", &mut errors, &schema);
        assert_eq!(
            errors,
            vec!["field: schema pattern \"(\" does not compile".to_string()]
        );
    }
}
