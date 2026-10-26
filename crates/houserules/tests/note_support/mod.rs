//! The change record of schema 1.0.0 that the tests of the record readers share
//! (`record.rs` and `record_list_audit.rs`): the note that the binary of batch
//! 37 wrote, with the values of one repository in the place of its shas
//! (`fixtures/change-record-1.0.0/README.md` has the provenance).

use std::fs;
use std::path::Path;

use serde_json::Value;

/// The note of schema 1.0.0 for the range `base` to `head`, a repository whose
/// rule sources hash to `rules_hash`. The base is the merge base, and the head
/// is the key. The note holds no `links.sessions`, as schema 1.0.0 has none.
pub fn note_1_0_0(base: &str, head: &str, rules_hash: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("change-record-1.0.0")
        .join("note.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let text = text
        .replace("__BASE7__", &base[..7])
        .replace("__HEAD7__", &head[..7])
        .replace("__BASE__", base)
        .replace("__HEAD__", head)
        .replace("__RULES_HASH__", rules_hash);
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("parse the note: {error}"))
}
