//! `checkBase` and the `check-knowledge` command: `tools/kb.mjs`'s
//! knowledge-base validator, ported byte-for-byte (HR-054 task 4; the
//! frozen fixture corpus under `tests/corpus/check/` is the parity gate --
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

use super::model::{Base, load_base};
use super::render::{RULE_KINDS, SKILL_PATH, render_all, repo_root_from_cwd};

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
/// guard (fix round 1, finding 3) are its callers.
fn falsy(value: Option<&Value>) -> bool {
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

/// Validates a loaded base against the schema and every cross-entry and
/// generated-file invariant -- `tools/kb.mjs`'s `checkBase`, two stages in
/// the same order: schema/id/area/standing/see/verify/check-shape errors
/// accumulate first, and if any fired, `check_base` returns immediately
/// (rendering needs a valid base, the same reason the JS bails at
/// `if (errors.any) return errors.list`); only a clean first stage reaches
/// the stale/stray/budget checks, which need the generated files to exist
/// meaningfully.
pub(crate) fn check_base(base: &Base) -> Vec<String> {
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
                if !base.entries.contains_key(see_id.as_str()) {
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
        return errors; // rendering needs a valid base
    }

    let rendered = render_all(base);
    let rendered_paths: HashSet<&str> = rendered.iter().map(|(path, _)| path.as_str()).collect();
    for (path, content) in &rendered {
        let abs = base.root.join(path);
        let current = fs::read_to_string(&abs).ok();
        if current.as_deref() != Some(content.as_str()) {
            errors.push(format!(
                "{path}: generated file is out of date (run tools/kb.sh render)"
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
    errors
}

/// Runs the `check-knowledge` subcommand: loads the knowledge base at
/// `root` (resolving the enclosing git repository's top level when `root`
/// is `None`, exactly like `cmd_render`), then runs `check_base` against
/// it. A load failure (missing knowledge dir, missing `schema.json`, a
/// malformed area glob) prints one named line and exits 2 -- distinct from
/// a clean load whose check findings print as `tools/kb.sh check`'s own
/// stderr lines and exit 1 (spec §6's CLI-failure-path deviation; docs/specs/
/// 2026-09-04-batch-15-tier2-spec.md).
pub(crate) fn cmd_check_knowledge(root: Option<PathBuf>) -> ExitCode {
    let root = match root {
        Some(path) => path,
        None => match repo_root_from_cwd() {
            Ok(path) => path,
            Err(error) => {
                eprintln!("{error}");
                return ExitCode::from(2);
            }
        },
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

    use serde_json::json;

    use super::*;

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
        for path in [
            ".claude/schemas/deliverables.json",
            ".claude/evals/record.json",
            "backlog/schema.json",
            ".claude/skills/finishing-a-feature/SKILL.md",
            ".claude/skills/orchestrating/SKILL.md",
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
            &".claude/rules/standing-rules.md: generated file is out of date (run tools/kb.sh render)".to_string()
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
