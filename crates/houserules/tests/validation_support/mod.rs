//! The capture files that the `--validation` flag of `evidence` and `record
//! write` reads (batch 38, spec 4; acceptance case 3), and the refusals that
//! the flag gives for each way a capture can fail.
//!
//! A capture holds the header that `houserules capture` writes on line 1 and
//! the output of `openspec validate --json` after it. The builders here write
//! that file by hand, so a case sets the head and the dirty flag that it needs.
//! A test that wants the header of a real run calls `houserules capture`.
//!
//! It also holds the helpers that `tests/evidence.rs` and `tests/record.rs`
//! share for the cases of the flag: the `review` key of a fixture, a path as an
//! argument, the `validation` of an intent, and the captures that a command
//! refuses. The fixture that adds the
//! changes (`openspec_changes`) and the file writer (`capture_file`) stay in
//! each file, because each file has its own `Install` type, which holds the
//! scratch repository that they write into.
//!
//! The module allows dead code, because the test files that include it use
//! different parts of it.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The `review` key whose only intent adapter is `openspec`.
pub fn openspec_review() -> Value {
    json!({"intent": {"sources": ["openspec"]}})
}

/// `path` as an argument.
pub fn arg(path: &Path) -> &str {
    path.to_str().expect("a UTF-8 path")
}

/// The `validation` of a change that OpenSpec judged: the verdict and the summary.
pub fn validation(ok: bool, summary: &str) -> Value {
    json!({"tool": "openspec", "ok": ok, "summary": summary})
}

/// The `validation` of each intent of `document`, a packet document or a change record, in the
/// order of the list.
pub fn validations(document: &Value) -> Vec<Value> {
    document["intent"]
        .as_array()
        .expect("the intents")
        .iter()
        .map(|intent| intent["validation"].clone())
        .collect()
}

/// The header on line 1 of a capture, as `houserules capture` writes it for the
/// command that the units document, with the `head` and `dirty` facts of the run
/// that a case names.
pub fn header(head: Option<&str>, dirty: Option<bool>) -> Value {
    json!({
        "schema": "houserules/capture@1.0.0",
        "argv": ["sh", "-c", "openspec validate --all --json 2>/dev/null"],
        "cwd": "/work",
        "start": "2026-10-26T10:00:00.000000000Z",
        "end": "2026-10-26T10:00:01.000000000Z",
        "exit": 0,
        "signal": null,
        "head": head,
        "dirty": dirty,
        "lossy": 0,
    })
}

/// The capture file of `header` and `output`: the header, a line feed, and the
/// output. An empty output leaves the header alone, with no line feed, as
/// `capture` writes it.
pub fn capture_of(header: &Value, output: &str) -> String {
    if output.is_empty() {
        header.to_string()
    } else {
        format!("{header}\n{output}")
    }
}

/// The capture of a run at `head` in a clean work tree whose output is
/// `output`.
pub fn clean_capture(head: &str, output: &str) -> String {
    capture_of(&header(Some(head), Some(false)), output)
}

/// One item of the document of `openspec validate --json`: the `id`, the `kind`
/// (`change` or `spec`), the verdict, and one issue for each level of `levels`.
pub fn item(id: &str, kind: &str, valid: bool, levels: &[&str]) -> Value {
    let issues: Vec<Value> = levels
        .iter()
        .map(|level| json!({"level": level, "path": "proposal", "message": "A message."}))
        .collect();
    json!({"id": id, "type": kind, "valid": valid, "issues": issues, "durationMs": 3})
}

/// The document of `openspec validate --json` over `items`, as text: the items,
/// the summary, the version `1.0`, and the root.
pub fn document_of(items: &[Value]) -> String {
    json!({
        "items": items,
        "summary": {"totals": {"items": items.len()}},
        "version": "1.0",
        "root": "/work/openspec",
    })
    .to_string()
        + "\n"
}

/// Why a capture is refused: a name for a failure message, the file that the
/// test writes, and the reason of the line `<command>: --validation <path>:
/// <reason>`.
pub struct Refusal {
    /// What is wrong with the file.
    pub name: &'static str,
    /// The bytes of the file.
    pub file: String,
    /// The reason of spec 4.1, whole.
    pub reason: String,
}

/// The first seven characters of `sha`.
fn short(sha: &str) -> &str {
    &sha[..7]
}

/// Every refusal of spec 4.1 that a written file gives, for a command that
/// runs at `head`: a file that is no capture, a capture of a run outside a work
/// tree, of another head, and of a dirty work tree, an output that is no JSON,
/// and a JSON that is no document of `openspec validate`. The reason
/// `cannot be read (<os error>)` needs no file, so a test builds it from the
/// error of the operating system.
pub fn refusals(head: &str, other: &str) -> Vec<Refusal> {
    let mut header_of_another_schema = header(Some(head), Some(false));
    header_of_another_schema["schema"] = json!("houserules/capture@2.0.0");
    let mut header_with_control_characters = header(Some(head), Some(false));
    header_with_control_characters["schema"] = json!("x\u{1}\n\u{7f}y");
    let mut header_without_argv = header(Some(head), Some(false));
    header_without_argv
        .as_object_mut()
        .expect("an object")
        .remove("argv");
    let not_a_document = r#"is no openspec validate document (version "1.0" with items)"#;
    let bad_documents = [
        ("a JSON object with no key", "{}"),
        ("a JSON array", "[]"),
        ("a JSON string", "\"1.0\""),
        (
            "a document of another version",
            r#"{"version":"2.0","items":[]}"#,
        ),
        (
            "a version that is no string",
            r#"{"version":1.0,"items":[]}"#,
        ),
        ("a document with no items", r#"{"version":"1.0"}"#),
        ("items that are no array", r#"{"version":"1.0","items":{}}"#),
        (
            "an item that is no object",
            r#"{"version":"1.0","items":["x"]}"#,
        ),
        (
            "an item with no id",
            r#"{"version":"1.0","items":[{"type":"change","valid":true,"issues":[]}]}"#,
        ),
        (
            "an item with an id that is no string",
            r#"{"version":"1.0","items":[{"id":7,"type":"change","valid":true,"issues":[]}]}"#,
        ),
        (
            "an item with no type",
            r#"{"version":"1.0","items":[{"id":"a","valid":true,"issues":[]}]}"#,
        ),
        (
            "an item with no verdict",
            r#"{"version":"1.0","items":[{"id":"a","type":"change","issues":[]}]}"#,
        ),
        (
            "an item with a verdict that is no boolean",
            r#"{"version":"1.0","items":[{"id":"a","type":"change","valid":"yes","issues":[]}]}"#,
        ),
        (
            "an item with no issues",
            r#"{"version":"1.0","items":[{"id":"a","type":"change","valid":true}]}"#,
        ),
        (
            "an item whose issues are no array",
            r#"{"version":"1.0","items":[{"id":"a","type":"change","valid":true,"issues":2}]}"#,
        ),
        (
            "a good item beside a bad one",
            r#"{"version":"1.0","items":[{"id":"a","type":"change","valid":true,"issues":[]},{"id":"b"}]}"#,
        ),
    ];
    let mut refusals = vec![
        Refusal {
            name: "an empty file",
            file: String::new(),
            reason: "is no capture (it is empty)".to_string(),
        },
        Refusal {
            name: "a file whose first line is no JSON object",
            file: "plain text\n".to_string(),
            reason: "is no capture (its first line is no JSON object)".to_string(),
        },
        Refusal {
            name: "a header with no argv",
            file: capture_of(&header_without_argv, "{}"),
            reason: "is no capture (its header lacks argv)".to_string(),
        },
        Refusal {
            name: "a header of another major schema",
            file: capture_of(&header_of_another_schema, "{}"),
            reason: "is no capture (its schema is houserules/capture@2.0.0, not \
                     houserules/capture@1)"
                .to_string(),
        },
        Refusal {
            name: "a schema with control characters",
            file: capture_of(&header_with_control_characters, "{}"),
            reason: "is no capture (its schema is x\\u0001\\u000a\\u007fy, not \
                     houserules/capture@1)"
                .to_string(),
        },
        Refusal {
            name: "a run outside a git work tree",
            file: capture_of(&header(None, None), &document_of(&[])),
            reason: "ran outside a git work tree".to_string(),
        },
        Refusal {
            name: "a run at another head",
            file: clean_capture(other, &document_of(&[])),
            reason: format!("ran at {}, not at {}", short(other), short(head)),
        },
        Refusal {
            name: "a run in a dirty work tree",
            file: capture_of(&header(Some(head), Some(true)), &document_of(&[])),
            reason: "ran in a dirty work tree".to_string(),
        },
        Refusal {
            name: "a run with no output",
            file: clean_capture(head, ""),
            reason: "holds no JSON (EOF while parsing a value at line 1 column 0)".to_string(),
        },
        Refusal {
            name: "an output that is no JSON",
            file: clean_capture(head, "oops\n"),
            reason: "holds no JSON (expected value at line 1 column 1)".to_string(),
        },
        Refusal {
            name: "an output with text after the document",
            file: clean_capture(head, "{\"version\":\"1.0\",\"items\":[]} x\n"),
            reason: "holds no JSON (trailing characters at line 1 column 30)".to_string(),
        },
    ];
    refusals.extend(bad_documents.iter().map(|(name, output)| Refusal {
        name,
        file: clean_capture(head, &format!("{output}\n")),
        reason: not_a_document.to_string(),
    }));
    refusals
}

/// A capture that a command refuses: a name for a failure message, the path to
/// pass as `--validation`, and the reason of the line `<command>: --validation
/// <path>: <reason>`.
pub struct RefusedCapture {
    /// What is wrong with the path.
    pub name: String,
    /// The path of the capture.
    pub path: PathBuf,
    /// The reason of spec 4.1, whole.
    pub reason: String,
}

/// One refusal for each of the seven reasons of spec 4.1, as a path that a command can read, for
/// a command that runs at `head`: a file that is missing, and the first file of [`refusals`] for
/// each other reason, written below `dir`. `other` is the commit of the capture of another head.
pub fn refused_captures(dir: &Path, head: &str, other: &str) -> Vec<RefusedCapture> {
    let missing = dir.join("missing.json");
    let error = fs::read(&missing).expect_err("the path cannot be read");
    let mut captures = vec![RefusedCapture {
        name: "a capture file that is missing".to_string(),
        reason: format!("cannot be read ({error})"),
        path: missing,
    }];
    let table = refusals(head, other);
    for reason in [
        "is no capture",
        "ran outside a git work tree",
        "ran at ",
        "ran in a dirty work tree",
        "holds no JSON",
        "is no openspec validate document",
    ] {
        let refusal = table
            .iter()
            .find(|refusal| refusal.reason.starts_with(reason))
            .unwrap_or_else(|| panic!("no refusal for the reason {reason}"));
        let path = dir.join(format!("refused-{}.json", captures.len()));
        fs::write(&path, &refusal.file).expect("write the capture");
        captures.push(RefusedCapture {
            name: refusal.name.to_string(),
            path,
            reason: refusal.reason.clone(),
        });
    }
    captures
}
