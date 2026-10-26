//! Generates the JSON Schema of every published document from the Rust type
//! that serializes it (`crate::published`), and keeps the manifest of the
//! committed schema files. Dev tooling: not part of the command surface of
//! `houserules`, not shipped in `template/` or the payload.
//!
//! # What this writes
//!
//! `schemas/<name>/<version>.json` for each document in [`registry`]: a JSON
//! Schema of draft 2020-12 with the serialize contract, the Pages URL of the
//! file as its `$id`, the `schema` key described as a `const` of the
//! document's own id, and each description as the doc comment of its type
//! with the source line wraps joined into spaces. `schemas/published.json`
//! lists each published file with its SHA-256; only `--record` writes it.
//!
//! # Usage
//!
//! - `cargo run --quiet --bin gen-schemas` writes every schema file whose bytes
//!   differ from the generated ones and prints a `wrote` line for each.
//! - `-- --check` writes nothing. It prints one line to stderr for each file
//!   that differs or is missing and exits 1 when it printed any.
//! - `-- --record` writes the schema files, then adds a manifest row for each
//!   file that has none. It replaces the row of a listed file only when no
//!   release tag holds that file (`git tag --list 'v*'`, then `git cat-file -e
//!   refs/tags/<tag>:schemas/<path>` for each tag), because a file of a
//!   release never changes. When a release tag holds a file whose row differs,
//!   `--record` writes nothing and exits 1.
//! - `--dir <root>` names the repository root; the default is this checkout.
//!
//! A task that adds a document appends its type to [`registry`] and runs
//! `--record`.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use schemars::generate::SchemaSettings;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

#[path = "../published/mod.rs"]
#[allow(
    dead_code,
    reason = "the module is shared with the main binary, which uses the rest"
)]
mod published;

use published::Published;
use published::audit_result::AuditResult;
use published::capture::Header;
use published::change_record::ChangeRecord;
use published::evidence::Evidence;
use published::route::RouteDocument;
use published::rule_package::RulePackage;

/// The Pages site of this repository, below which each schema file lives at
/// `<name>/<version>.json`.
const BASE_URL: &str = "https://jblossey.github.io/houserules/schemas";

/// The directory of the schema files, relative to the repository root.
const SCHEMAS_DIR: &str = "schemas";

/// The manifest file name, relative to [`SCHEMAS_DIR`].
const MANIFEST: &str = "published.json";

/// The usage line, printed for an unknown argument.
const USAGE: &str = "usage: gen-schemas [--check | --record] [--dir <root>]";

/// The settings of every published schema: draft 2020-12, describing how the
/// type is serialized.
fn settings() -> SchemaSettings {
    SchemaSettings::draft2020_12().for_serialize()
}

/// Joins the source line wraps of a doc comment into spaces. A blank line (a
/// paragraph break) stays as one blank line between the joined paragraphs. A
/// published description is prose: a list or a code block in it would lose its
/// line breaks.
fn join_wrapped_lines(text: &str) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim) {
        if !line.is_empty() {
            lines.push(line);
        } else if !lines.is_empty() {
            paragraphs.push(lines.join(" "));
            lines.clear();
        }
    }
    if !lines.is_empty() {
        paragraphs.push(lines.join(" "));
    }
    paragraphs.join("\n\n")
}

/// Joins the line wraps of every `description` string below `value`
/// ([`join_wrapped_lines`]). A property that is named `description` holds a
/// schema, not a string, so its own description is the one that joins.
fn join_line_wraps(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                match child {
                    Value::String(text) if key == "description" => {
                        *text = join_wrapped_lines(text);
                    }
                    _ => join_line_wraps(child),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(join_line_wraps),
        _ => {}
    }
}

/// The schema of the document type `T`, with `$id` set to the Pages URL of its
/// file, the `schema` key described as a `const` of the document's id, and the
/// line wraps of every description joined.
fn schema_for<T: Published>() -> Value {
    let root = settings().into_generator().into_root_schema_for::<T>();
    let Value::Object(generated) = serde_json::to_value(&root).expect("a schema serializes") else {
        panic!("the root schema of {} is no object", T::NAME);
    };
    let mut schema = Map::new();
    let mut rest = generated;
    if let Some(meta_schema) = rest.shift_remove("$schema") {
        schema.insert("$schema".to_string(), meta_schema);
    }
    schema.insert(
        "$id".to_string(),
        Value::String(format!("{BASE_URL}/{}", file_of::<T>())),
    );
    schema.extend(rest);
    let schema_key = schema
        .get_mut("properties")
        .and_then(|properties| properties.get_mut("schema"))
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("{} has no schema property", T::NAME));
    schema_key.insert("const".to_string(), Value::String(published::id::<T>()));
    let mut schema = Value::Object(schema);
    join_line_wraps(&mut schema);
    schema
}

/// The path of the schema file of `T` below the schemas directory.
fn file_of<T: Published>() -> String {
    format!("{}/{}.json", T::NAME, T::VERSION)
}

/// One published document: where its schema file lives and how to generate it.
struct Registered {
    /// The path below the schemas directory, `<name>/<version>.json`.
    file: String,
    /// Generates the schema.
    schema: fn() -> Value,
}

/// Registers the document type `T`.
fn register<T: Published>() -> Registered {
    Registered {
        file: file_of::<T>(),
        schema: schema_for::<T>,
    }
}

/// Every published document, in the order of the registry. A task that adds a
/// document appends its type here.
fn registry() -> Vec<Registered> {
    vec![
        register::<AuditResult>(),
        register::<Header>(),
        register::<ChangeRecord>(),
        register::<Evidence>(),
        register::<RouteDocument>(),
        register::<RulePackage>(),
    ]
}

/// A generated schema file: its path below the schemas directory, its text,
/// and the SHA-256 of the text.
struct Generated {
    file: String,
    text: String,
    sha256: String,
}

/// Generates every registered schema, sorted by path.
fn generate_all() -> Vec<Generated> {
    let mut all: Vec<Generated> = registry()
        .into_iter()
        .map(|registered| {
            let text = format!(
                "{}\n",
                serde_json::to_string_pretty(&(registered.schema)()).expect("a schema serializes")
            );
            let sha256 = sha256_hex(text.as_bytes());
            Generated {
                file: registered.file,
                text,
                sha256,
            }
        })
        .collect();
    all.sort_by(|a, b| a.file.cmp(&b.file));
    all
}

/// The lowercase hex SHA-256 digest of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The manifest `schemas/published.json`.
#[derive(Serialize, Deserialize)]
struct Manifest {
    /// One row for each published file.
    files: Vec<Row>,
}

/// A row of the manifest: a schema file and the digest of its bytes.
#[derive(Serialize, Deserialize)]
struct Row {
    /// The path below the schemas directory, `<name>/<version>.json`.
    path: String,
    /// The lowercase hex SHA-256 of the file.
    sha256: String,
}

/// The mode of one run.
#[derive(Clone, Copy)]
enum Mode {
    /// Write the schema files that differ.
    Write,
    /// Compare the schema files with the generated ones and write nothing.
    Check,
    /// Write the schema files, then record the manifest rows.
    Record,
}

/// Reads the arguments after the program name. `None` is a usage error.
fn parse_args(args: &[String]) -> Option<(Mode, Option<PathBuf>)> {
    let mut mode = None;
    let mut dir = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let wanted = match arg.as_str() {
            "--check" => Some(Mode::Check),
            "--record" => Some(Mode::Record),
            "--dir" => {
                dir = Some(PathBuf::from(rest.next()?));
                None
            }
            _ => return None,
        };
        if let Some(wanted) = wanted
            && mode.replace(wanted).is_some()
        {
            return None;
        }
    }
    Some((mode.unwrap_or(Mode::Write), dir))
}

/// This checkout's repository root, resolved at compile time.
fn default_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", root.display()))
}

/// The path of the schemas directory under `root`.
fn schemas_dir(root: &Path) -> PathBuf {
    root.join(SCHEMAS_DIR)
}

/// The printed form of a schema file: `schemas/<file>`.
fn shown(file: &str) -> String {
    format!("{SCHEMAS_DIR}/{file}")
}

/// Compares each generated file with the file on disk. Prints one line to
/// stderr for each file that differs, is missing, or cannot be read, and
/// returns whether all match.
fn check(root: &Path, generated: &[Generated]) -> bool {
    let mut matches = true;
    for file in generated {
        match fs::read(schemas_dir(root).join(&file.file)) {
            Ok(bytes) if bytes == file.text.as_bytes() => {}
            Ok(_) => {
                eprintln!("{}: differs from the generated schema", shown(&file.file));
                matches = false;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                eprintln!("{}: missing", shown(&file.file));
                matches = false;
            }
            Err(error) => {
                eprintln!("{}: cannot be read: {error}", shown(&file.file));
                matches = false;
            }
        }
    }
    matches
}

/// Writes each generated file whose bytes differ from the file on disk and
/// prints a `wrote` line for it.
fn write_files(root: &Path, generated: &[Generated]) -> Result<(), String> {
    for file in generated {
        let path = schemas_dir(root).join(&file.file);
        if fs::read_to_string(&path).is_ok_and(|text| text == file.text) {
            continue;
        }
        let parent = path.parent().expect("a schema file has a directory");
        fs::create_dir_all(parent)
            .and_then(|()| fs::write(&path, &file.text))
            .map_err(|error| format!("write {}: {error}", path.display()))?;
        println!("wrote {}", shown(&file.file));
    }
    Ok(())
}

/// The release tags of the repository at `root` (`v*`), sorted by name.
fn release_tags(root: &Path) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .args(["tag", "--list", "--sort=refname", "v*"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("run git tag: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git tag --list failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

/// The first of `tags` whose tree holds `schemas/<file>`.
fn tag_holding(root: &Path, tags: &[String], file: &str) -> Option<String> {
    tags.iter()
        .find(|tag| {
            Command::new("git")
                .args([
                    "cat-file",
                    "-e",
                    &format!("refs/tags/{tag}:{}", shown(file)),
                ])
                .current_dir(root)
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .cloned()
}

/// What `--record` does with the row of one file.
enum Recorded {
    /// The file has no row: add one.
    Added,
    /// The row differs and no release tag holds the file: replace it.
    Replaced,
    /// The row holds the digest of the generated file.
    Kept,
    /// The row differs and a release tag holds the file; the payload is the
    /// first such tag.
    Refused(String),
}

/// Writes the schema files, then records the manifest rows. Returns whether
/// every row was recorded; a refusal prints its line to stderr and stops the
/// run before any write. The `recorded` lines print after the manifest is
/// written, so a manifest that cannot be written leaves none.
fn record(root: &Path, generated: &[Generated]) -> Result<bool, String> {
    let manifest_path = schemas_dir(root).join(MANIFEST);
    let mut rows: BTreeMap<String, String> = match fs::read_to_string(&manifest_path) {
        Ok(text) => serde_json::from_str::<Manifest>(&text)
            .map_err(|error| format!("parse {}: {error}", manifest_path.display()))?
            .files
            .into_iter()
            .map(|row| (row.path, row.sha256))
            .collect(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => BTreeMap::new(),
        Err(error) => return Err(format!("read {}: {error}", manifest_path.display())),
    };
    let tags = release_tags(root)?;
    let outcomes: Vec<Recorded> = generated
        .iter()
        .map(|file| match rows.get(&file.file) {
            None => Recorded::Added,
            Some(sha256) if *sha256 == file.sha256 => Recorded::Kept,
            Some(_) => match tag_holding(root, &tags, &file.file) {
                Some(tag) => Recorded::Refused(tag),
                None => Recorded::Replaced,
            },
        })
        .collect();
    let mut refused = false;
    for (file, outcome) in generated.iter().zip(&outcomes) {
        if let Recorded::Refused(tag) = outcome {
            eprintln!(
                "{}: release {tag} holds this file, so its row stays; give the change a new schema version",
                shown(&file.file)
            );
            refused = true;
        }
    }
    if refused {
        return Ok(false);
    }
    write_files(root, generated)?;
    let mut recorded = Vec::new();
    for (file, outcome) in generated.iter().zip(&outcomes) {
        let verb = match outcome {
            Recorded::Added => "recorded",
            Recorded::Replaced => "re-recorded",
            Recorded::Kept | Recorded::Refused(_) => continue,
        };
        rows.insert(file.file.clone(), file.sha256.clone());
        recorded.push(format!("{verb} {}", shown(&file.file)));
    }
    let manifest = Manifest {
        files: rows
            .into_iter()
            .map(|(path, sha256)| Row { path, sha256 })
            .collect(),
    };
    let text = format!(
        "{}\n",
        serde_json::to_string_pretty(&manifest).expect("the manifest serializes")
    );
    if !fs::read_to_string(&manifest_path).is_ok_and(|current| current == text) {
        fs::create_dir_all(schemas_dir(root))
            .and_then(|()| fs::write(&manifest_path, text))
            .map_err(|error| format!("write {}: {error}", manifest_path.display()))?;
    }
    for line in recorded {
        println!("{line}");
    }
    Ok(true)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((mode, dir)) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let root = dir.unwrap_or_else(default_root);
    let generated = generate_all();
    let result = match mode {
        Mode::Check => Ok(check(&root, &generated)),
        Mode::Write => write_files(&root, &generated).map(|()| true),
        Mode::Record => record(&root, &generated),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("gen-schemas: {message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_wrapped_sentence_joins_into_one_line() {
        assert_eq!(
            join_wrapped_lines("keys follow the\norder of the fields."),
            "keys follow the order of the fields."
        );
    }

    #[test]
    fn a_paragraph_break_stays_and_each_paragraph_joins() {
        assert_eq!(
            join_wrapped_lines("First paragraph\nwraps.\n\nSecond\nparagraph\nwraps."),
            "First paragraph wraps.\n\nSecond paragraph wraps."
        );
    }

    #[test]
    fn every_description_of_a_schema_joins_and_no_other_string_does() {
        let mut schema = json!({
            "description": "top\nlevel",
            "properties": {
                "description": {
                    "description": "a property that\nis named description",
                    "type": "string"
                },
                "head": {
                    "anyOf": [
                        { "type": "null", "description": "in a\nlist" },
                        { "const": "kept\nas written" }
                    ]
                }
            }
        });
        join_line_wraps(&mut schema);
        assert_eq!(
            schema,
            json!({
                "description": "top level",
                "properties": {
                    "description": {
                        "description": "a property that is named description",
                        "type": "string"
                    },
                    "head": {
                        "anyOf": [
                            { "type": "null", "description": "in a list" },
                            { "const": "kept\nas written" }
                        ]
                    }
                }
            })
        );
    }
}
