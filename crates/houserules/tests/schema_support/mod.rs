//! Helpers that the tests of the published schemas share: validate a
//! document against a committed schema, list the schema files under a
//! `schemas/` directory, and check that directory against its manifest
//! `published.json`. A test file that needs them declares this module and
//! `tracker_support`, whose `sha256_hex` the manifest check calls.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// The manifest file name, relative to a `schemas/` directory.
pub const MANIFEST: &str = "published.json";

/// The schema id prefix of the Pages site; a schema file's `$id` is this,
/// then `/<name>/<version>.json`.
pub const BASE_URL: &str = "https://jblossey.github.io/houserules/schemas";

/// The `$schema` of every published schema.
pub const META_SCHEMA: &str = "https://json-schema.org/draft/2020-12/schema";

/// Reads and parses the JSON file at `path`.
pub fn read_json(path: &Path) -> Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

/// Asserts that `document` is valid against the schema file `schema_file`,
/// a path relative to the repository root. The check reads no network and
/// no other file: the schemas hold only references to their own `$defs`.
pub fn assert_valid(document: &Value, schema_file: &str) {
    let schema = read_json(&crate::tracker_support::repo_root().join(schema_file));
    let validator = jsonschema::validator_for(&schema)
        .unwrap_or_else(|error| panic!("{schema_file} is no usable schema: {error}"));
    let errors: Vec<String> = validator
        .iter_errors(document)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "the document does not validate against {schema_file}:\n{}\n{document:#}",
        errors.join("\n")
    );
}

/// The schema files below `schemas_dir`, as `<name>/<version>.json` paths
/// with `/` separators, sorted. The manifest is no schema file. The walk
/// reads the directory, so a file added later is in the list.
pub fn schema_files(schemas_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect(schemas_dir, schemas_dir, &mut files);
    files.retain(|file| file != MANIFEST);
    files.sort();
    files
}

/// Appends every file below `dir` to `files`, as a `/`-separated path
/// relative to `base`.
fn collect(base: &Path, dir: &Path, files: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
    {
        let entry = entry.expect("a directory entry");
        let path = entry.path();
        if entry.file_type().expect("a file type").is_dir() {
            collect(base, &path, files);
        } else {
            let relative: PathBuf = path.strip_prefix(base).expect("below the base").into();
            let parts: Vec<String> = relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            files.push(parts.join("/"));
        }
    }
}

/// The problems of `schemas_dir` against its manifest, one line each, sorted:
/// a listed file with other bytes, a listed file that is missing, a listed
/// file that cannot be read (with the error), and a file below the directory
/// that the manifest does not list. A directory with no manifest has the one
/// problem `published.json: missing`.
pub fn manifest_problems(schemas_dir: &Path) -> Vec<String> {
    let manifest_path = schemas_dir.join(MANIFEST);
    if !manifest_path.exists() {
        return vec![format!("{MANIFEST}: missing")];
    }
    let manifest = read_json(&manifest_path);
    let rows = manifest["files"]
        .as_array()
        .unwrap_or_else(|| panic!("{MANIFEST} has no files array"));
    let mut listed = BTreeMap::new();
    for row in rows {
        let path = row["path"].as_str().expect("a row path").to_string();
        let sha256 = row["sha256"].as_str().expect("a row sha256").to_string();
        listed.insert(path, sha256);
    }
    let mut problems = Vec::new();
    for (path, sha256) in &listed {
        match fs::read(schemas_dir.join(path)) {
            Ok(bytes) if crate::tracker_support::sha256_hex(&bytes) != *sha256 => {
                problems.push(format!("{path}: its bytes differ from the manifest"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                problems.push(format!("{path}: listed but missing"));
            }
            Err(error) => problems.push(format!("{path}: cannot be read: {error}")),
        }
    }
    for file in schema_files(schemas_dir) {
        if !listed.contains_key(&file) {
            problems.push(format!("{file}: not listed in the manifest"));
        }
    }
    problems.sort();
    problems
}
